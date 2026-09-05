//! Feasible-start Fulkerson out-of-kilter minimum-cost circulation.

use std::collections::VecDeque;

use thiserror::Error;

use crate::certificate::{CertificateError, MinCostFlowCertificate, check_min_cost_flow};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{EdgeIndex, FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative interactive node limit for the classical feasible-start method.
pub const OUT_OF_KILTER_MAX_NODES: usize = 256;
/// Conservative interactive edge limit for the classical feasible-start method.
pub const OUT_OF_KILTER_MAX_EDGES: usize = 2_048;
/// Deterministic ceiling on breakthrough and price-correction steps.
pub const OUT_OF_KILTER_MAX_CORRECTIONS: u64 = 100_000;
/// Deterministic ceiling on modified-labeling and cut scans.
pub const OUT_OF_KILTER_MAX_RESIDUAL_ARC_SCANS: u128 = 10_000_000;
/// Deterministic ceiling on original-arc kilter checks.
pub const OUT_OF_KILTER_MAX_KILTER_ARC_SCANS: u128 = 20_000_000;
/// Initial residual scans published one by one before geometric checkpoints.
const OUT_OF_KILTER_TRACE_SCAN_PREFIX: u128 = 4;

/// Exact deterministic counters from feasible-start Out-of-Kilter.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OutOfKilterMetrics {
    /// Searches for the first canonical out-of-kilter original arc.
    pub kilter_searches: u64,
    /// Original arcs inspected while selecting or checking kilter monotonicity.
    pub kilter_arc_scans: u128,
    /// Original arcs selected for repeated correction.
    pub selected_arcs: u64,
    /// Modified-labeling path searches.
    pub label_searches: u64,
    /// Nodes labeled across all searches, including each origin.
    pub labeled_nodes: u128,
    /// Positive residual arcs inspected by labeling and cut-price scans.
    pub residual_arc_scans: u128,
    /// Flow-changing breakthroughs around a residual cycle.
    pub breakthroughs: u64,
    /// Nonbreakthrough price corrections on the unlabeled set.
    pub price_updates: u64,
    /// Total flow units moved around breakthrough cycles.
    pub corrected_flow_units: u128,
}

/// Certified canonical feasible-start Out-of-Kilter result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutOfKilterResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Final Fulkerson node prices in canonical node-ID order.
    pub prices: Vec<i128>,
    /// Independently reconstructed objective and dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: OutOfKilterMetrics,
}

/// Certified result with reversible selection, labeling, and correction events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutOfKilterTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: OutOfKilterResult,
    /// Replay boundary at the arbitrary feasible initial circulation.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after independent minimum-cost certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Out-of-Kilter construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum OutOfKilterError {
    /// Input exceeds the practical interactive admission band.
    #[error("graph exceeds Out-of-Kilter admission limits")]
    AdmissionLimit,
    /// A deterministic correction or scan ceiling was reached.
    #[error("Out-of-Kilter work limit reached")]
    WorkLimit,
    /// No flow satisfies the requested balances and original bounds.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// The final independent primal/dual certificate failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded its declared domain.
    #[error("Out-of-Kilter arithmetic overflow")]
    ArithmeticOverflow,
    /// Modified labels, their predecessor path, or their cut were inconsistent.
    #[error("Out-of-Kilter labeling invariant failed")]
    LabelInvariant,
    /// A correction increased a kilter number or did not improve the selected arc.
    #[error("Out-of-Kilter monotonicity invariant failed")]
    KilterInvariant,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves exact balanced minimum-cost flow by Fulkerson's Out-of-Kilter method.
///
/// This implementation uses the simplification stated in Fulkerson (1961): an
/// arbitrary feasible circulation is constructed first, so only the two
/// complementary-slackness violations remain. Canonical residual-ID FIFO
/// labeling fixes the otherwise arbitrary labeling order.
///
/// # Errors
///
/// Rejects admission, infeasibility, checked arithmetic, residual mutation,
/// deterministic work limits, internal monotonicity, or final certification.
pub fn solve_out_of_kilter(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<OutOfKilterResult, OutOfKilterError> {
    solve_out_of_kilter_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its initial feasible-flow construction to the
/// enclosing execution context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_out_of_kilter_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<OutOfKilterResult, OutOfKilterError> {
    solve_out_of_kilter_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Runs feasible-start Out-of-Kilter with reversible pedagogical events.
///
/// # Errors
///
/// Returns the same failures as [`solve_out_of_kilter`] plus trace failures.
pub fn trace_out_of_kilter(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<OutOfKilterTraceResult, OutOfKilterError> {
    let run = solve_out_of_kilter_internal(graph, required_divergence, true)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(OutOfKilterError::LabelInvariant)?;
    Ok(OutOfKilterTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces Out-of-Kilter while explicitly publishing its initial feasible-flow
/// construction to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_out_of_kilter_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<OutOfKilterTraceResult, OutOfKilterError> {
    let run = solve_out_of_kilter_internal_with_feasibility(
        graph,
        required_divergence,
        true,
        feasibility,
    )?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(OutOfKilterError::LabelInvariant)?;
    Ok(OutOfKilterTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct OutOfKilterInternalRun {
    result: OutOfKilterResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn solve_out_of_kilter_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_events: bool,
) -> Result<OutOfKilterInternalRun, OutOfKilterError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_out_of_kilter_internal_with_feasibility(
        graph,
        required_divergence,
        record_events,
        &mut feasibility,
    )
}

fn solve_out_of_kilter_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_events: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<OutOfKilterInternalRun, OutOfKilterError> {
    validate_admission(graph)?;
    let feasible =
        feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::InitialFlow)?;
    let mut state = ResidualState::from_flows(graph, &feasible.flows)?;
    let mut prices = vec![0_i128; graph.nodes().len()];
    let mut metrics = OutOfKilterMetrics::default();
    let mut corrections = 0_u64;
    let mut recorder = start_trace_recorder(graph, &state, record_events)?;

    record_trace(
        recorder.as_mut(),
        graph,
        &state,
        &prices,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: "out-of-kilter.initialize-feasible-circulation",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "out-of-kilter:start-with-feasible-circulation-and-zero-prices",
        },
        TraceView::empty(),
        None,
    )?;

    let mut selected = None;
    loop {
        if selected.is_none() {
            selected = select_and_record(graph, &state, &prices, &mut metrics, recorder.as_mut())?;
            if selected.is_none() {
                record_optimal(graph, &state, &prices, metrics, recorder.as_mut())?;
                break;
            }
        }

        let correction = selected.as_ref().ok_or(OutOfKilterError::KilterInvariant)?;
        let current = correction_for_edge(&state, &prices, correction.edge)?;
        let Some(current) = current else {
            selected = None;
            continue;
        };
        if current.residual_id.direction() != correction.residual_id.direction() {
            return Err(OutOfKilterError::KilterInvariant);
        }
        if corrections >= OUT_OF_KILTER_MAX_CORRECTIONS {
            return Err(OutOfKilterError::WorkLimit);
        }

        let search = search_and_record(
            graph,
            &state,
            &prices,
            &current,
            &mut metrics,
            recorder.as_mut(),
        )?;
        let before = kilter_numbers(&state, &prices, &mut metrics)?;
        if let Some(path) = search.path {
            apply_breakthrough(
                graph,
                &mut state,
                &prices,
                &current,
                &path,
                search.search_order,
                &before,
                &mut metrics,
                &mut corrections,
                recorder.as_mut(),
            )?;
        } else {
            apply_price_update(
                graph,
                &state,
                &mut prices,
                &current,
                &search.labeled,
                search.search_order,
                &before,
                &mut metrics,
                &mut corrections,
                recorder.as_mut(),
            )?;
        }

        if correction_for_edge(&state, &prices, current.edge)?.is_none() {
            selected = None;
        }
    }

    let flows = state.flows().to_vec();
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    let result = OutOfKilterResult {
        flows,
        prices,
        certificate,
        metrics,
    };
    Ok(OutOfKilterInternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn select_and_record(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    prices: &[i128],
    metrics: &mut OutOfKilterMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<Option<Correction>, OutOfKilterError> {
    let selected = select_out_of_kilter_arc(state, prices, metrics)?;
    let Some(correction) = selected.as_ref() else {
        return Ok(None);
    };
    metrics.selected_arcs = metrics
        .selected_arcs
        .checked_add(1)
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    record_trace(
        recorder,
        graph,
        state,
        prices,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: "out-of-kilter.select-out-of-kilter-arc",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "out-of-kilter:select-first-canonical-out-of-kilter-arc",
        },
        TraceView::selected(correction),
        Some(("kilter-number", correction.kilter_number)),
    )?;
    Ok(selected)
}

fn record_optimal(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    prices: &[i128],
    metrics: OutOfKilterMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), FlowTraceError> {
    record_trace(
        recorder,
        graph,
        state,
        prices,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: "out-of-kilter.optimal",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "out-of-kilter:return-when-every-arc-is-in-kilter",
        },
        TraceView::empty(),
        Some(("total-kilter", 0)),
    )
}

fn search_and_record(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    prices: &[i128],
    correction: &Correction,
    metrics: &mut OutOfKilterMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<LabelSearch, OutOfKilterError> {
    let mut search = modified_label_search(state, prices, correction, metrics, recorder.is_some())?;
    for checkpoint in std::mem::take(&mut search.checkpoints) {
        record_trace(
            recorder.as_deref_mut(),
            graph,
            state,
            prices,
            checkpoint.metrics,
            FlowTraceEventMetadata {
                catalog_id: "out-of-kilter.modified-label-search",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "out-of-kilter:inspect-one-modified-label-residual-arc",
            },
            TraceView {
                search_order: checkpoint.search_order,
                active_path: checkpoint.active_path,
                focus: vec![FlowTraceEntityRef::ResidualArc(checkpoint.inspected)],
            },
            Some(("scan-ordinal", scan_ordinal(checkpoint.metrics)?)),
        )?;
    }
    Ok(search)
}

#[allow(clippy::too_many_arguments)]
fn apply_breakthrough(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    prices: &[i128],
    correction: &Correction,
    path: &[ResidualArcId],
    search_order: Vec<NodeIndex>,
    before: &[i128],
    metrics: &mut OutOfKilterMetrics,
    corrections: &mut u64,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), OutOfKilterError> {
    let cycle = cycle_from_correction(correction, path);
    let amount = cycle
        .iter()
        .map(|id| {
            state
                .arc(id)
                .filter(|arc| arc.capacity > 0)
                .map(|arc| arc.capacity)
                .ok_or(OutOfKilterError::LabelInvariant)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or(OutOfKilterError::LabelInvariant)?;
    state.augment(&cycle, amount)?;
    metrics.breakthroughs = metrics
        .breakthroughs
        .checked_add(1)
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    metrics.corrected_flow_units = metrics
        .corrected_flow_units
        .checked_add(u128::from(amount))
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    *corrections = corrections
        .checked_add(1)
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    let after = kilter_numbers(state, prices, metrics)?;
    validate_kilter_progress(before, &after, correction.edge)?;
    let focus = cycle
        .iter()
        .filter(|id| state.arc(id).is_some_and(|arc| arc.capacity == 0))
        .min()
        .or_else(|| cycle.first())
        .cloned()
        .map(FlowTraceEntityRef::ResidualArc)
        .into_iter()
        .collect();
    record_trace(
        recorder,
        graph,
        state,
        prices,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: "out-of-kilter.breakthrough",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "out-of-kilter:augment-selected-cycle-to-its-bottleneck",
        },
        TraceView {
            search_order,
            active_path: cycle,
            focus,
        },
        Some(("delta", i128::from(amount))),
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_price_update(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    prices: &mut Vec<i128>,
    correction: &Correction,
    labeled: &[bool],
    search_order: Vec<NodeIndex>,
    before: &[i128],
    metrics: &mut OutOfKilterMetrics,
    corrections: &mut u64,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), OutOfKilterError> {
    let price_search = price_delta(
        state,
        prices,
        correction,
        labeled,
        &search_order,
        metrics,
        recorder.is_some(),
    )?;
    for checkpoint in price_search.checkpoints {
        record_trace(
            recorder.as_deref_mut(),
            graph,
            state,
            prices,
            checkpoint.metrics,
            FlowTraceEventMetadata {
                catalog_id: "out-of-kilter.inspect-cut-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "out-of-kilter:inspect-one-labeled-cut-residual-arc",
            },
            TraceView {
                search_order: checkpoint.search_order,
                active_path: checkpoint.active_path,
                focus: vec![FlowTraceEntityRef::ResidualArc(checkpoint.inspected)],
            },
            Some(("scan-ordinal", scan_ordinal(checkpoint.metrics)?)),
        )?;
    }
    let delta = price_search.delta;
    let mut next_prices = prices.clone();
    for node in graph.node_indices() {
        if !labeled[node.as_usize()] {
            next_prices[node.as_usize()] = next_prices[node.as_usize()]
                .checked_add(delta)
                .ok_or(OutOfKilterError::ArithmeticOverflow)?;
        }
    }
    *prices = next_prices;
    metrics.price_updates = metrics
        .price_updates
        .checked_add(1)
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    *corrections = corrections
        .checked_add(1)
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    let after = kilter_numbers(state, prices, metrics)?;
    validate_kilter_progress(before, &after, correction.edge)?;
    record_trace(
        recorder,
        graph,
        state,
        prices,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: "out-of-kilter.raise-unlabeled-prices",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "out-of-kilter:raise-unlabeled-prices-by-minimum-cut-slack",
        },
        TraceView {
            search_order,
            active_path: vec![correction.residual_id.clone()],
            focus: vec![FlowTraceEntityRef::ResidualArc(
                correction.residual_id.clone(),
            )],
        },
        Some(("delta", delta)),
    )?;
    Ok(())
}

fn validate_admission(graph: &FlowNetwork) -> Result<(), OutOfKilterError> {
    if graph.nodes().len() > OUT_OF_KILTER_MAX_NODES
        || graph.edges().len() > OUT_OF_KILTER_MAX_EDGES
    {
        return Err(OutOfKilterError::AdmissionLimit);
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Correction {
    edge: EdgeIndex,
    residual_id: ResidualArcId,
    origin: NodeIndex,
    terminal: NodeIndex,
    reduced_cost: i128,
    kilter_number: i128,
}

fn select_out_of_kilter_arc(
    state: &ResidualState<'_>,
    prices: &[i128],
    metrics: &mut OutOfKilterMetrics,
) -> Result<Option<Correction>, OutOfKilterError> {
    metrics.kilter_searches = metrics
        .kilter_searches
        .checked_add(1)
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    for edge in state.graph().edge_indices() {
        record_kilter_scan(metrics)?;
        if let Some(correction) = correction_for_edge(state, prices, edge)? {
            return Ok(Some(correction));
        }
    }
    Ok(None)
}

fn correction_for_edge(
    state: &ResidualState<'_>,
    prices: &[i128],
    edge_index: EdgeIndex,
) -> Result<Option<Correction>, OutOfKilterError> {
    let edge = state
        .graph()
        .edge(edge_index)
        .ok_or(OutOfKilterError::KilterInvariant)?;
    let flow = *state
        .flows()
        .get(edge_index.as_usize())
        .ok_or(OutOfKilterError::KilterInvariant)?;
    let reduced_cost = original_reduced_cost(state.graph(), prices, edge_index)?;
    let (direction, distance) = if reduced_cost < 0 && flow < edge.capacity() {
        (ResidualDirection::Forward, edge.capacity() - flow)
    } else if reduced_cost > 0 && flow > edge.lower() {
        (ResidualDirection::Reverse, flow - edge.lower())
    } else {
        return Ok(None);
    };
    let residual_id = ResidualArcId::new(edge.id().clone(), direction);
    let residual = state
        .arc(&residual_id)
        .filter(|arc| arc.capacity == distance && arc.capacity > 0)
        .ok_or(OutOfKilterError::KilterInvariant)?;
    let magnitude = reduced_cost
        .checked_abs()
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    let kilter_number = magnitude
        .checked_mul(i128::from(distance))
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    Ok(Some(Correction {
        edge: edge_index,
        residual_id,
        origin: residual.to,
        terminal: residual.from,
        reduced_cost,
        kilter_number,
    }))
}

struct LabelSearch {
    labeled: Vec<bool>,
    search_order: Vec<NodeIndex>,
    path: Option<Vec<ResidualArcId>>,
    checkpoints: Vec<ResidualScanCheckpoint>,
}

struct ResidualScanCheckpoint {
    metrics: OutOfKilterMetrics,
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
    inspected: ResidualArcId,
}

fn modified_label_search(
    state: &ResidualState<'_>,
    prices: &[i128],
    correction: &Correction,
    metrics: &mut OutOfKilterMetrics,
    record_trace: bool,
) -> Result<LabelSearch, OutOfKilterError> {
    metrics.label_searches = metrics
        .label_searches
        .checked_add(1)
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    let node_count = state.graph().nodes().len();
    let mut labeled = vec![false; node_count];
    let mut predecessor = vec![None; node_count];
    let mut search_order = Vec::new();
    let mut checkpoints = Vec::new();
    let mut last_inspected = None;
    let mut queue = VecDeque::new();
    labeled[correction.origin.as_usize()] = true;
    search_order.push(correction.origin);
    queue.push_back(correction.origin);
    metrics.labeled_nodes = metrics
        .labeled_nodes
        .checked_add(1)
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;

    if correction.origin != correction.terminal {
        'search: while let Some(node) = queue.pop_front() {
            for arc in state.outgoing_arcs(node) {
                record_residual_scan(metrics)?;
                let reduced_cost = residual_reduced_cost(prices, &arc)?;
                let newly_labeled = reduced_cost <= 0 && !labeled[arc.to.as_usize()];
                let reached_terminal = newly_labeled && arc.to == correction.terminal;
                if newly_labeled {
                    labeled[arc.to.as_usize()] = true;
                    predecessor[arc.to.as_usize()] = Some(arc.id.clone());
                    search_order.push(arc.to);
                    metrics.labeled_nodes = metrics
                        .labeled_nodes
                        .checked_add(1)
                        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
                }
                last_inspected = Some(arc.id.clone());
                push_residual_scan_checkpoint(
                    record_trace,
                    metrics,
                    &search_order,
                    vec![arc.id.clone()],
                    arc.id,
                    &mut checkpoints,
                );
                if reached_terminal {
                    break 'search;
                }
                if newly_labeled {
                    queue.push_back(arc.to);
                }
            }
        }
    }
    let path = if labeled[correction.terminal.as_usize()] {
        Some(reconstruct_path(
            state,
            &predecessor,
            correction.origin,
            correction.terminal,
        )?)
    } else {
        None
    };
    let active_path = path.as_ref().map_or_else(
        || vec![correction.residual_id.clone()],
        |return_path| cycle_from_correction(correction, return_path),
    );
    if record_trace && let Some(inspected) = last_inspected {
        if checkpoints.last().is_some_and(|checkpoint| {
            checkpoint.metrics.residual_arc_scans == metrics.residual_arc_scans
        }) {
            let final_checkpoint = checkpoints
                .last_mut()
                .expect("checked final Out-of-Kilter scan checkpoint");
            final_checkpoint.search_order.clone_from(&search_order);
            final_checkpoint.active_path = active_path;
            final_checkpoint.inspected = inspected;
            final_checkpoint.metrics = *metrics;
        } else {
            checkpoints.push(ResidualScanCheckpoint {
                metrics: *metrics,
                search_order: search_order.clone(),
                active_path,
                inspected,
            });
        }
    }
    Ok(LabelSearch {
        labeled,
        search_order,
        path,
        checkpoints,
    })
}

fn reconstruct_path(
    state: &ResidualState<'_>,
    predecessor: &[Option<ResidualArcId>],
    origin: NodeIndex,
    terminal: NodeIndex,
) -> Result<Vec<ResidualArcId>, OutOfKilterError> {
    if origin == terminal {
        return Ok(Vec::new());
    }
    let mut reversed = Vec::new();
    let mut cursor = terminal;
    while cursor != origin {
        let id = predecessor
            .get(cursor.as_usize())
            .and_then(Clone::clone)
            .ok_or(OutOfKilterError::LabelInvariant)?;
        let arc = state
            .arc(&id)
            .filter(|arc| arc.capacity > 0 && arc.to == cursor)
            .ok_or(OutOfKilterError::LabelInvariant)?;
        cursor = arc.from;
        reversed.push(id);
        if reversed.len() > state.graph().nodes().len() {
            return Err(OutOfKilterError::LabelInvariant);
        }
    }
    reversed.reverse();
    Ok(reversed)
}

fn cycle_from_correction(
    correction: &Correction,
    return_path: &[ResidualArcId],
) -> Vec<ResidualArcId> {
    let mut cycle = Vec::with_capacity(return_path.len() + 1);
    cycle.push(correction.residual_id.clone());
    cycle.extend_from_slice(return_path);
    cycle
}

fn price_delta(
    state: &ResidualState<'_>,
    prices: &[i128],
    correction: &Correction,
    labeled: &[bool],
    search_order: &[NodeIndex],
    metrics: &mut OutOfKilterMetrics,
    record_trace: bool,
) -> Result<PriceDelta, OutOfKilterError> {
    if labeled.len() != state.graph().nodes().len()
        || !labeled[correction.origin.as_usize()]
        || labeled[correction.terminal.as_usize()]
    {
        return Err(OutOfKilterError::LabelInvariant);
    }
    let mut delta = correction
        .reduced_cost
        .checked_abs()
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    let mut checkpoints = Vec::new();
    let mut last_inspected = None;
    for node in state.graph().node_indices() {
        if !labeled[node.as_usize()] {
            continue;
        }
        for arc in state.outgoing_arcs(node) {
            record_residual_scan(metrics)?;
            last_inspected = Some(arc.id.clone());
            push_residual_scan_checkpoint(
                record_trace,
                metrics,
                search_order,
                vec![arc.id.clone()],
                arc.id.clone(),
                &mut checkpoints,
            );
            if labeled[arc.to.as_usize()] {
                continue;
            }
            let reduced_cost = residual_reduced_cost(prices, &arc)?;
            if reduced_cost <= 0 {
                return Err(OutOfKilterError::LabelInvariant);
            }
            delta = delta.min(reduced_cost);
        }
    }
    if delta <= 0 {
        return Err(OutOfKilterError::LabelInvariant);
    }
    if record_trace && let Some(inspected) = last_inspected {
        if checkpoints.last().is_some_and(|checkpoint| {
            checkpoint.metrics.residual_arc_scans == metrics.residual_arc_scans
        }) {
            let final_checkpoint = checkpoints
                .last_mut()
                .expect("checked final Out-of-Kilter cut checkpoint");
            final_checkpoint.search_order = search_order.to_vec();
            final_checkpoint.active_path = vec![inspected.clone()];
            final_checkpoint.inspected = inspected;
            final_checkpoint.metrics = *metrics;
        } else {
            checkpoints.push(ResidualScanCheckpoint {
                metrics: *metrics,
                search_order: search_order.to_vec(),
                active_path: vec![inspected.clone()],
                inspected,
            });
        }
    }
    Ok(PriceDelta { delta, checkpoints })
}

struct PriceDelta {
    delta: i128,
    checkpoints: Vec<ResidualScanCheckpoint>,
}

fn push_residual_scan_checkpoint(
    record_trace: bool,
    metrics: &OutOfKilterMetrics,
    search_order: &[NodeIndex],
    active_path: Vec<ResidualArcId>,
    inspected: ResidualArcId,
    checkpoints: &mut Vec<ResidualScanCheckpoint>,
) {
    if record_trace && should_publish_residual_scan(metrics.residual_arc_scans) {
        checkpoints.push(ResidualScanCheckpoint {
            metrics: *metrics,
            search_order: search_order.to_vec(),
            active_path,
            inspected,
        });
    }
}

const fn should_publish_residual_scan(scan: u128) -> bool {
    scan <= OUT_OF_KILTER_TRACE_SCAN_PREFIX || scan.is_power_of_two()
}

fn scan_ordinal(metrics: OutOfKilterMetrics) -> Result<i128, OutOfKilterError> {
    i128::try_from(metrics.residual_arc_scans).map_err(|_| OutOfKilterError::ArithmeticOverflow)
}

fn original_reduced_cost(
    graph: &FlowNetwork,
    prices: &[i128],
    edge_index: EdgeIndex,
) -> Result<i128, OutOfKilterError> {
    let edge = graph
        .edge(edge_index)
        .ok_or(OutOfKilterError::KilterInvariant)?;
    i128::from(edge.cost())
        .checked_add(
            *prices
                .get(edge.from().as_usize())
                .ok_or(OutOfKilterError::KilterInvariant)?,
        )
        .and_then(|value| value.checked_sub(prices.get(edge.to().as_usize()).copied()?))
        .ok_or(OutOfKilterError::ArithmeticOverflow)
}

fn residual_reduced_cost(
    prices: &[i128],
    arc: &crate::residual::ResidualArc,
) -> Result<i128, OutOfKilterError> {
    arc.cost
        .checked_add(
            *prices
                .get(arc.from.as_usize())
                .ok_or(OutOfKilterError::LabelInvariant)?,
        )
        .and_then(|value| value.checked_sub(prices.get(arc.to.as_usize()).copied()?))
        .ok_or(OutOfKilterError::ArithmeticOverflow)
}

fn kilter_numbers(
    state: &ResidualState<'_>,
    prices: &[i128],
    metrics: &mut OutOfKilterMetrics,
) -> Result<Vec<i128>, OutOfKilterError> {
    state
        .graph()
        .edge_indices()
        .map(|edge_index| {
            record_kilter_scan(metrics)?;
            Ok(
                correction_for_edge(state, prices, edge_index)?
                    .map_or(0, |item| item.kilter_number),
            )
        })
        .collect()
}

fn validate_kilter_progress(
    before: &[i128],
    after: &[i128],
    selected: EdgeIndex,
) -> Result<(), OutOfKilterError> {
    if before.len() != after.len()
        || before
            .iter()
            .zip(after)
            .any(|(&old, &new)| new < 0 || new > old)
        || after
            .get(selected.as_usize())
            .zip(before.get(selected.as_usize()))
            .is_none_or(|(&new, &old)| new >= old)
    {
        return Err(OutOfKilterError::KilterInvariant);
    }
    Ok(())
}

fn record_kilter_scan(metrics: &mut OutOfKilterMetrics) -> Result<(), OutOfKilterError> {
    metrics.kilter_arc_scans = metrics
        .kilter_arc_scans
        .checked_add(1)
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    if metrics.kilter_arc_scans > OUT_OF_KILTER_MAX_KILTER_ARC_SCANS {
        return Err(OutOfKilterError::WorkLimit);
    }
    Ok(())
}

fn record_residual_scan(metrics: &mut OutOfKilterMetrics) -> Result<(), OutOfKilterError> {
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(OutOfKilterError::ArithmeticOverflow)?;
    if metrics.residual_arc_scans > OUT_OF_KILTER_MAX_RESIDUAL_ARC_SCANS {
        return Err(OutOfKilterError::WorkLimit);
    }
    Ok(())
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    record_events: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, FlowTraceError> {
    if !record_events {
        return Ok(None);
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        FlowTraceMetrics::default(),
    );
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

struct TraceView {
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
    focus: Vec<FlowTraceEntityRef>,
}

impl TraceView {
    fn empty() -> Self {
        Self {
            search_order: Vec::new(),
            active_path: Vec::new(),
            focus: Vec::new(),
        }
    }

    fn selected(correction: &Correction) -> Self {
        Self {
            search_order: Vec::new(),
            active_path: vec![correction.residual_id.clone()],
            focus: vec![FlowTraceEntityRef::ResidualArc(
                correction.residual_id.clone(),
            )],
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn record_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    prices: &[i128],
    metrics: OutOfKilterMetrics,
    metadata: FlowTraceEventMetadata,
    view: TraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        prices.iter().copied().map(Some).collect(),
        view.search_order,
        view.active_path,
        Vec::new(),
        trace_metrics(metrics),
    );
    recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, view.focus)
}

const fn trace_metrics(metrics: OutOfKilterMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.label_searches as u128,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.breakthroughs as u128,
        path_searches: 0,
        scaling_phases: 0,
        blocking_flow_phases: 0,
        relabels: metrics.price_updates as u128,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: 0,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: metrics.selected_arcs as u128,
    }
}

#[cfg(test)]
mod tests {
    use crate::certificate::supply_divergences;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, FlowTracePatch, apply_trace_event};

    use super::*;
    use crate::algorithms::solve_simple_cycle_canceling;

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

    #[test]
    fn alternates_price_update_and_breakthrough_on_a_negative_cycle() {
        let graph = network(
            &[("x", 0), ("y", 0)],
            &[("xy", "x", "y", 0, 3, -4), ("yx", "y", "x", 0, 3, 1)],
        );
        let target = supply_divergences(&graph).expect("target");

        let result = solve_out_of_kilter(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [3, 3]);
        assert_eq!(result.certificate.total_cost, -9);
        assert_eq!(result.metrics.price_updates, 1);
        assert_eq!(result.metrics.breakthroughs, 1);
        assert_eq!(result.metrics.corrected_flow_units, 3);
        assert_eq!(result.prices, [1, 0]);
    }

    #[test]
    fn can_put_an_isolated_arc_in_kilter_by_prices_only() {
        let graph = network(&[("s", 0), ("t", 0)], &[("st", "s", "t", 0, 4, -7)]);
        let target = supply_divergences(&graph).expect("target");

        let result = solve_out_of_kilter(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [0]);
        assert_eq!(result.certificate.total_cost, 0);
        assert_eq!(result.metrics.price_updates, 1);
        assert_eq!(result.metrics.breakthroughs, 0);
        assert_eq!(result.prices, [7, 0]);
    }

    #[test]
    fn treats_a_negative_self_loop_as_a_direct_breakthrough() {
        let graph = network(&[("x", 0)], &[("loop", "x", "x", 0, 2, -5)]);
        let target = supply_divergences(&graph).expect("target");

        let result = solve_out_of_kilter(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [2]);
        assert_eq!(result.certificate.total_cost, -10);
        assert_eq!(result.metrics.breakthroughs, 1);
        assert_eq!(result.metrics.price_updates, 0);
    }

    #[test]
    fn preserves_lower_bounds_and_nonzero_transshipment_balances() {
        let graph = network(
            &[("a", 0), ("s", 2), ("t", -2)],
            &[
                ("at", "a", "t", 0, 2, 1),
                ("direct", "s", "t", 1, 2, 5),
                ("sa", "s", "a", 0, 2, 1),
            ],
        );
        let target = supply_divergences(&graph).expect("target");

        let result = solve_out_of_kilter(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [1, 1, 1]);
        assert_eq!(result.certificate.total_cost, 7);
        check_min_cost_flow(&graph, &target, &result.flows).expect("certificate");
    }

    #[test]
    fn handles_parallel_arcs_by_stable_edge_identity() {
        let graph = network(
            &[("s", 2), ("t", -2)],
            &[("cheap", "s", "t", 0, 1, 1), ("costly", "s", "t", 0, 2, 5)],
        );
        let target = supply_divergences(&graph).expect("target");

        let result = solve_out_of_kilter(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [1, 1]);
        assert_eq!(result.certificate.total_cost, 6);
    }

    #[test]
    fn trace_replays_price_and_flow_corrections_in_both_directions() {
        let graph = network(
            &[("x", 0), ("y", 0)],
            &[("xy", "x", "y", 0, 2, -3), ("yx", "y", "x", 0, 2, 1)],
        );
        let target = supply_divergences(&graph).expect("target");
        let fast = solve_out_of_kilter(&graph, &target).expect("fast result");
        let traced = trace_out_of_kilter(&graph, &target).expect("trace result");

        assert_eq!(traced.result, fast);
        let select = traced
            .events
            .iter()
            .find(|event| event.catalog_id == "out-of-kilter.select-out-of-kilter-arc")
            .expect("selection event");
        assert_eq!(
            select
                .detail
                .as_ref()
                .map(|detail| (detail.label.as_str(), detail.value)),
            Some(("kilter-number", 6))
        );
        let price = traced
            .events
            .iter()
            .find(|event| event.catalog_id == "out-of-kilter.raise-unlabeled-prices")
            .expect("price event");
        assert!(price.patches.iter().any(|patch| matches!(
            patch,
            FlowTracePatch::NodeLabel {
                before: Some(0),
                after: Some(1),
                ..
            }
        )));
        let breakthrough = traced
            .events
            .iter()
            .find(|event| event.catalog_id == "out-of-kilter.breakthrough")
            .expect("breakthrough event");
        assert!(breakthrough.patches.iter().any(|patch| matches!(
            patch,
            FlowTracePatch::EdgeFlow {
                before: 0,
                after: 2,
                ..
            }
        )));

        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(replay.flows, fast.flows);
        assert_eq!(
            replay.node_labels,
            fast.prices.iter().copied().map(Some).collect::<Vec<_>>()
        );
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn agrees_with_cycle_canceling_on_small_deterministic_circulations() {
        let mut seed = 0xa076_1d64_78bd_642f_u64;
        for case in 0..48 {
            let node_count = 2 + usize::try_from(next(&mut seed) % 4).expect("small count");
            let nodes = (0..node_count)
                .map(|index| FlowNode::new(NodeId::parse(&format!("v{index}")).expect("node"), 0))
                .collect::<Vec<_>>();
            let mut edges = Vec::new();
            for from in 0..node_count {
                for to in 0..node_count {
                    if from == to || next(&mut seed).is_multiple_of(3) {
                        continue;
                    }
                    edges.push(UnresolvedFlowEdge {
                        id: EdgeId::parse(&format!("e{from}_{to}")).expect("edge"),
                        from: NodeId::parse(&format!("v{from}")).expect("tail"),
                        to: NodeId::parse(&format!("v{to}")).expect("head"),
                        lower: 0,
                        capacity: 1 + next(&mut seed) % 4,
                        cost: i64::try_from(next(&mut seed) % 11).expect("cost") - 5,
                    });
                }
            }
            if edges.is_empty() {
                edges.push(UnresolvedFlowEdge {
                    id: EdgeId::parse("fallback").expect("edge"),
                    from: NodeId::parse("v0").expect("tail"),
                    to: NodeId::parse("v1").expect("head"),
                    lower: 0,
                    capacity: 1,
                    cost: 0,
                });
            }
            let graph = FlowNetwork::new(nodes, edges).expect("valid graph");
            let target = vec![0_i128; node_count];
            let expected = solve_simple_cycle_canceling(&graph, &target)
                .unwrap_or_else(|error| panic!("reference case {case}: {error}"));
            let actual = solve_out_of_kilter(&graph, &target)
                .unwrap_or_else(|error| panic!("Out-of-Kilter case {case}: {error}"));
            assert_eq!(
                actual.certificate.total_cost, expected.certificate.total_cost,
                "case {case}"
            );
        }
    }

    #[test]
    fn infeasible_balances_are_rejected_before_kilter_work() {
        let graph = network(&[("s", 1), ("t", -1)], &[]);
        let target = supply_divergences(&graph).expect("target");

        assert!(matches!(
            solve_out_of_kilter(&graph, &target),
            Err(OutOfKilterError::Feasibility(FeasibilityError::Infeasible(
                _
            )))
        ));
    }

    fn next(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *seed
    }
}
