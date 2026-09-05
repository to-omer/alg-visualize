//! Deterministic Ford–Fulkerson augmenting-path kernel.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArc, ResidualArcId, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetricId,
    FlowTraceMetrics, FlowTraceRecorder, FlowTraceSnapshot, residual_arc_entity_refs,
    residual_path_node_order,
};

/// Conservative interactive node limit for pseudo-polynomial path selection.
pub const FORD_FULKERSON_MAX_NODES: usize = 2_000;
/// Conservative interactive edge limit for pseudo-polynomial path selection.
pub const FORD_FULKERSON_MAX_EDGES: usize = 20_000;
/// Hard work ceiling that prevents an integral-capacity instance from running forever.
pub const FORD_FULKERSON_MAX_AUGMENTATIONS: u64 = 10_000;

/// Exact operation counts from the deterministic augmenting-path kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FordFulkersonMetrics {
    /// Complete path searches, including the final unsuccessful search.
    pub path_searches: u64,
    /// Positive residual arcs inspected by the selector.
    pub residual_arc_scans: u128,
    /// Successful residual augmentations.
    pub augmentations: u64,
    /// Capacity scales entered; zero for non-scaling selectors.
    pub scaling_phases: u64,
}

/// Certified canonical Ford–Fulkerson result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FordFulkersonResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed max-flow/min-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: FordFulkersonMetrics,
}

/// Certified result with a complete reversible selector trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FordFulkersonTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: FordFulkersonResult,
    /// Replay boundary before the first path search.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after the verified optimal phase.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Ford–Fulkerson construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum FordFulkersonError {
    /// Input exceeds the practical admission band for this algorithm.
    #[error("graph exceeds Ford-Fulkerson admission limits")]
    AdmissionLimit,
    /// The pseudo-polynomial augmentation ceiling was reached.
    #[error("Ford-Fulkerson augmentation work limit reached")]
    WorkLimit,
    /// Lower-bound circulation construction failed.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Final solver-independent certificate rejected the result.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact operation counters exceeded their declared domain.
    #[error("Ford-Fulkerson metric overflow")]
    MetricOverflow,
    /// A predecessor chain contradicted the selected residual walk.
    #[error("Ford-Fulkerson predecessor invariant failed")]
    PredecessorInvariant,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves maximum flow with stable-ID, depth-first augmenting paths.
///
/// This is the default deterministic selector for the general
/// Ford–Fulkerson catalog entry.
///
/// # Errors
///
/// Rejects out-of-band graphs, infeasible lower bounds, work-limit exhaustion,
/// residual invariant failure, metric overflow, or a rejected certificate.
pub fn solve_ford_fulkerson(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<FordFulkersonResult, FordFulkersonError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_ford_fulkerson_preset_with_feasibility(
        graph,
        source,
        sink,
        FordFulkersonExecutionPreset::General,
        &mut feasibility,
    )
}

/// Solves the explicit stable-ID DFS Ford–Fulkerson preset.
///
/// # Errors
///
/// Returns the same failures as [`solve_ford_fulkerson`].
pub fn solve_dfs_ford_fulkerson(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<FordFulkersonResult, FordFulkersonError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_ford_fulkerson_preset_with_feasibility(
        graph,
        source,
        sink,
        FordFulkersonExecutionPreset::Dfs,
        &mut feasibility,
    )
}

/// Solves maximum flow by repeatedly choosing a maximum-bottleneck path.
///
/// # Errors
///
/// Returns the same bounded execution failures as [`solve_ford_fulkerson`].
pub fn solve_widest_augmenting_path(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<FordFulkersonResult, FordFulkersonError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_ford_fulkerson_preset_with_feasibility(
        graph,
        source,
        sink,
        FordFulkersonExecutionPreset::Widest,
        &mut feasibility,
    )
}

/// Solves maximum flow using powers-of-two residual-capacity scaling.
///
/// # Errors
///
/// Returns the same bounded execution failures as [`solve_ford_fulkerson`].
pub fn solve_capacity_scaling_augmenting_path(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<FordFulkersonResult, FordFulkersonError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_ford_fulkerson_preset_with_feasibility(
        graph,
        source,
        sink,
        FordFulkersonExecutionPreset::CapacityScaling,
        &mut feasibility,
    )
}

/// Solves one named Ford--Fulkerson execution preset while explicitly
/// publishing auxiliary feasibility work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_ford_fulkerson_preset_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: FordFulkersonExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<FordFulkersonResult, FordFulkersonError> {
    solve_internal(graph, source, sink, false, preset, feasibility).map(|run| run.result)
}

/// Traces the default stable-ID DFS selector under the general catalog entry.
///
/// # Errors
///
/// Returns the same execution failures as [`solve_ford_fulkerson`], plus trace
/// invariant failures.
pub fn trace_ford_fulkerson(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<FordFulkersonTraceResult, FordFulkersonError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_ford_fulkerson_preset_with_feasibility(
        graph,
        source,
        sink,
        FordFulkersonExecutionPreset::General,
        &mut feasibility,
    )
}

/// Traces the explicit stable-ID DFS Ford–Fulkerson preset.
///
/// # Errors
///
/// Returns the same execution failures as [`solve_dfs_ford_fulkerson`], plus
/// trace invariant failures.
pub fn trace_dfs_ford_fulkerson(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<FordFulkersonTraceResult, FordFulkersonError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_ford_fulkerson_preset_with_feasibility(
        graph,
        source,
        sink,
        FordFulkersonExecutionPreset::Dfs,
        &mut feasibility,
    )
}

/// Traces maximum-bottleneck labels, the chosen path, and augmentation.
///
/// # Errors
///
/// Returns the same bounded execution failures as
/// [`solve_widest_augmenting_path`], plus trace invariant failures.
pub fn trace_widest_augmenting_path(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<FordFulkersonTraceResult, FordFulkersonError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_ford_fulkerson_preset_with_feasibility(
        graph,
        source,
        sink,
        FordFulkersonExecutionPreset::Widest,
        &mut feasibility,
    )
}

/// Traces each capacity scale and its stable-ID DFS augmentations.
///
/// # Errors
///
/// Returns the same bounded execution failures as
/// [`solve_capacity_scaling_augmenting_path`], plus trace invariant failures.
pub fn trace_capacity_scaling_augmenting_path(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<FordFulkersonTraceResult, FordFulkersonError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_ford_fulkerson_preset_with_feasibility(
        graph,
        source,
        sink,
        FordFulkersonExecutionPreset::CapacityScaling,
        &mut feasibility,
    )
}

/// Traces one named Ford--Fulkerson execution preset while explicitly
/// publishing auxiliary feasibility work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_ford_fulkerson_preset_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: FordFulkersonExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<FordFulkersonTraceResult, FordFulkersonError> {
    trace_internal(graph, source, sink, preset, feasibility)
}

/// Closed set of source-distinct Ford--Fulkerson execution presets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FordFulkersonExecutionPreset {
    /// General catalog entry using the stable depth-first selector.
    General,
    /// Explicit depth-first Ford--Fulkerson variant.
    Dfs,
    /// Maximum-bottleneck augmenting-path selector.
    Widest,
    /// Powers-of-two residual-capacity scaling selector.
    CapacityScaling,
}

impl FordFulkersonExecutionPreset {
    const fn search_catalog_id(self) -> &'static str {
        match self {
            Self::General => "ford-fulkerson.search-dfs",
            Self::Dfs => "dfs-ford-fulkerson.search",
            Self::Widest => "widest-augmenting-path.search",
            Self::CapacityScaling => "capacity-scaling-augmenting-path.search",
        }
    }

    const fn augment_catalog_id(self) -> &'static str {
        match self {
            Self::General => "ford-fulkerson.augment",
            Self::Dfs => "dfs-ford-fulkerson.augment",
            Self::Widest => "widest-augmenting-path.augment",
            Self::CapacityScaling => "capacity-scaling-augmenting-path.augment",
        }
    }

    const fn inspect_catalog_id(self) -> &'static str {
        match self {
            Self::General => "ford-fulkerson.inspect-residual-arc",
            Self::Dfs => "dfs-ford-fulkerson.inspect-residual-arc",
            Self::Widest => "widest-augmenting-path.inspect-residual-arc",
            Self::CapacityScaling => "capacity-scaling-augmenting-path.inspect-residual-arc",
        }
    }

    const fn search_complete_catalog_id(self) -> &'static str {
        match self {
            Self::General => "ford-fulkerson.complete-search",
            Self::Dfs => "dfs-ford-fulkerson.complete-search",
            Self::Widest => "widest-augmenting-path.complete-search",
            Self::CapacityScaling => "capacity-scaling-augmenting-path.complete-search",
        }
    }

    const fn path_prefix_catalog_id(self) -> &'static str {
        match self {
            Self::General => "ford-fulkerson.extend-path-prefix",
            Self::Dfs => "dfs-ford-fulkerson.extend-path-prefix",
            Self::Widest => "widest-augmenting-path.extend-path-prefix",
            Self::CapacityScaling => "capacity-scaling-augmenting-path.extend-path-prefix",
        }
    }

    const fn optimal_catalog_id(self) -> &'static str {
        match self {
            Self::General => "ford-fulkerson.optimal",
            Self::Dfs => "dfs-ford-fulkerson.optimal",
            Self::Widest => "widest-augmenting-path.optimal",
            Self::CapacityScaling => "capacity-scaling-augmenting-path.optimal",
        }
    }

    const fn search_pseudocode_line(self) -> &'static str {
        match self {
            Self::General | Self::Dfs => "ford-fulkerson:search-stable-dfs-path",
            Self::Widest => "widest-augmenting-path:extract-maximum-width-label",
            Self::CapacityScaling => "capacity-scaling-augmenting-path:search-delta-eligible-path",
        }
    }
}

struct FordFulkersonInternalRun {
    result: FordFulkersonResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn trace_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: FordFulkersonExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<FordFulkersonTraceResult, FordFulkersonError> {
    let run = solve_internal(graph, source, sink, true, preset, feasibility)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(FordFulkersonError::PredecessorInvariant)?;
    Ok(FordFulkersonTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    record_trace: bool,
    preset: FordFulkersonExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<FordFulkersonInternalRun, FordFulkersonError> {
    if graph.nodes().len() > FORD_FULKERSON_MAX_NODES
        || graph.edges().len() > FORD_FULKERSON_MAX_EDGES
    {
        return Err(FordFulkersonError::AdmissionLimit);
    }
    let initial =
        feasibility.find_max_flow_initial(graph, source, sink, FeasibilityUse::InitialFlow)?;
    let mut state = ResidualState::from_flows(graph, &initial.flows)?;
    let mut metrics = FordFulkersonMetrics::default();
    let mut recorder = start_trace_recorder(graph, &state, metrics, record_trace)?;
    if preset == FordFulkersonExecutionPreset::CapacityScaling {
        run_capacity_scaling(
            graph,
            &mut state,
            source,
            sink,
            &mut metrics,
            recorder.as_mut(),
            preset,
        )?;
    } else {
        run_unscaled_selector(
            graph,
            &mut state,
            source,
            sink,
            &mut metrics,
            recorder.as_mut(),
            preset,
        )?;
    }
    let flows = state.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    record_trace_transition(
        recorder.as_mut(),
        graph,
        &state,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: preset.optimal_catalog_id(),
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "ford-fulkerson:return-max-flow-min-cut",
        },
        FordFulkersonTraceView::empty(graph),
        None,
    )?;
    let result = FordFulkersonResult {
        flows,
        certificate,
        metrics,
    };
    Ok(FordFulkersonInternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn run_unscaled_selector(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    metrics: &mut FordFulkersonMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    preset: FordFulkersonExecutionPreset,
) -> Result<(), FordFulkersonError> {
    loop {
        increment_path_search(metrics)?;
        record_search_start(
            recorder.as_deref_mut(),
            graph,
            state,
            source,
            *metrics,
            preset,
            None,
        )?;
        let search = if preset == FordFulkersonExecutionPreset::Widest {
            widest_augmenting_path(
                state,
                source,
                sink,
                metrics,
                recorder.as_deref_mut(),
                preset,
            )?
        } else {
            depth_first_augmenting_path(
                state,
                source,
                sink,
                1,
                metrics,
                recorder.as_deref_mut(),
                preset,
            )?
        };
        record_search(
            recorder.as_deref_mut(),
            graph,
            state,
            *metrics,
            preset,
            &search,
            search
                .bottleneck
                .map(|value| ("bottleneck", i128::from(value))),
        )?;
        let Some(path) = search.path else {
            return Ok(());
        };
        augment_selected_path(
            recorder.as_deref_mut(),
            graph,
            state,
            metrics,
            preset,
            search.labels,
            search.search_order,
            path,
        )?;
    }
}

fn run_capacity_scaling(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    metrics: &mut FordFulkersonMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    preset: FordFulkersonExecutionPreset,
) -> Result<(), FordFulkersonError> {
    let mut delta = largest_power_of_two_residual(state);
    while delta > 0 {
        metrics.scaling_phases = metrics
            .scaling_phases
            .checked_add(1)
            .ok_or(FordFulkersonError::MetricOverflow)?;
        loop {
            increment_path_search(metrics)?;
            record_search_start(
                recorder.as_deref_mut(),
                graph,
                state,
                source,
                *metrics,
                preset,
                Some(("delta", i128::from(delta))),
            )?;
            let search = depth_first_augmenting_path(
                state,
                source,
                sink,
                delta,
                metrics,
                recorder.as_deref_mut(),
                preset,
            )?;
            record_search(
                recorder.as_deref_mut(),
                graph,
                state,
                *metrics,
                preset,
                &search,
                Some(("delta", i128::from(delta))),
            )?;
            let Some(path) = search.path else {
                break;
            };
            augment_selected_path(
                recorder.as_deref_mut(),
                graph,
                state,
                metrics,
                preset,
                search.labels,
                search.search_order,
                path,
            )?;
        }
        delta /= 2;
    }
    Ok(())
}

fn increment_path_search(metrics: &mut FordFulkersonMetrics) -> Result<(), FordFulkersonError> {
    metrics.path_searches = metrics
        .path_searches
        .checked_add(1)
        .ok_or(FordFulkersonError::MetricOverflow)?;
    Ok(())
}

fn record_search(
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: FordFulkersonMetrics,
    preset: FordFulkersonExecutionPreset,
    search: &AugmentingPathSearch,
    detail: Option<(&'static str, i128)>,
) -> Result<(), FlowTraceError> {
    if let Some(path) = search.path.as_ref() {
        // Publish only proper prefixes here. The following complete-search
        // boundary owns the first rendering of the full s-t path; publishing
        // the full prefix twice would create two adjacent source events with
        // identical graph state.
        for prefix_length in 1..path.len() {
            record_trace_transition(
                recorder.as_deref_mut(),
                graph,
                state,
                metrics,
                FlowTraceEventMetadata {
                    catalog_id: preset.path_prefix_catalog_id(),
                    minimum_granularity: TraceGranularityV1::Micro,
                    pseudocode_line: "ford-fulkerson:extend-selected-residual-path-prefix",
                },
                FordFulkersonTraceView {
                    labels: search.labels.clone(),
                    search_order: search.search_order.clone(),
                    path: path[..prefix_length].to_vec(),
                },
                Some(("prefix-length", prefix_length as i128)),
            )?;
        }
    }
    record_trace_transition(
        recorder,
        graph,
        state,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: preset.search_complete_catalog_id(),
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: preset.search_pseudocode_line(),
        },
        FordFulkersonTraceView::from_search(search),
        detail,
    )
}

fn record_search_start(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    metrics: FordFulkersonMetrics,
    preset: FordFulkersonExecutionPreset,
    detail: Option<(&'static str, i128)>,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        vec![None; graph.nodes().len()],
        vec![source],
        Vec::new(),
        Vec::new(),
        trace_metrics(metrics),
    );
    recorder.record_transition_with_detail_and_focus(
        FlowTraceEventMetadata {
            catalog_id: preset.search_catalog_id(),
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: preset.search_pseudocode_line(),
        },
        &snapshot,
        detail,
        vec![FlowTraceEntityRef::Node(
            graph.nodes()[source.as_usize()].id().clone(),
        )],
    )
}

#[allow(clippy::too_many_arguments)]
fn augment_selected_path(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    metrics: &mut FordFulkersonMetrics,
    preset: FordFulkersonExecutionPreset,
    labels: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    path: Vec<ResidualArcId>,
) -> Result<(), FordFulkersonError> {
    if metrics.augmentations >= FORD_FULKERSON_MAX_AUGMENTATIONS {
        return Err(FordFulkersonError::WorkLimit);
    }
    let bottleneck = path
        .iter()
        .filter_map(|id| state.arc(id).map(|arc| arc.capacity))
        .min()
        .ok_or(FordFulkersonError::PredecessorInvariant)?;
    state.augment(&path, bottleneck)?;
    metrics.augmentations = metrics
        .augmentations
        .checked_add(1)
        .ok_or(FordFulkersonError::MetricOverflow)?;
    record_trace_transition(
        recorder,
        graph,
        state,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: preset.augment_catalog_id(),
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "ford-fulkerson:augment-by-bottleneck",
        },
        FordFulkersonTraceView {
            labels,
            search_order,
            path,
        },
        Some(("bottleneck", i128::from(bottleneck))),
    )?;
    Ok(())
}

fn largest_power_of_two_residual(state: &ResidualState<'_>) -> u64 {
    let maximum = state
        .graph()
        .node_indices()
        .flat_map(|node| state.outgoing_arcs(node))
        .map(|arc| arc.capacity)
        .max()
        .unwrap_or(0);
    if maximum == 0 {
        0
    } else {
        1_u64 << (u64::BITS - 1 - maximum.leading_zeros())
    }
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    metrics: FordFulkersonMetrics,
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
        Vec::new(),
        trace_metrics(metrics),
    );
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

struct FordFulkersonTraceView {
    labels: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    path: Vec<ResidualArcId>,
}

impl FordFulkersonTraceView {
    fn from_search(search: &AugmentingPathSearch) -> Self {
        Self {
            labels: search.labels.clone(),
            search_order: search.search_order.clone(),
            path: search.path.clone().unwrap_or_default(),
        }
    }

    fn empty(graph: &FlowNetwork) -> Self {
        Self {
            labels: vec![None; graph.nodes().len()],
            search_order: Vec::new(),
            path: Vec::new(),
        }
    }
}

fn record_trace_transition(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: FordFulkersonMetrics,
    metadata: FlowTraceEventMetadata,
    view: FordFulkersonTraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let focus_arcs = if metadata.minimum_granularity == TraceGranularityV1::Micro {
        view.path.last().cloned().into_iter().collect::<Vec<_>>()
    } else {
        view.path.clone()
    };
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
        Vec::new(),
        trace_metrics(metrics),
    );
    recorder.record_transition_with_detail_and_focus(
        metadata,
        &snapshot,
        detail,
        residual_arc_entity_refs(graph, state, &focus_arcs)?,
    )
}

const fn trace_metrics(metrics: FordFulkersonMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: metrics.path_searches as u128,
        scaling_phases: metrics.scaling_phases as u128,
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

struct AugmentingPathSearch {
    path: Option<Vec<ResidualArcId>>,
    labels: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    bottleneck: Option<u64>,
}

struct DfsFrame {
    arcs: Vec<ResidualArc>,
    next_arc: usize,
}

fn depth_first_augmenting_path(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    minimum_capacity: u64,
    metrics: &mut FordFulkersonMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    preset: FordFulkersonExecutionPreset,
) -> Result<AugmentingPathSearch, FordFulkersonError> {
    let mut recorder = recorder;
    let mut predecessor = vec![None; state.graph().nodes().len()];
    let mut visited = vec![false; state.graph().nodes().len()];
    let mut depths = vec![None; state.graph().nodes().len()];
    let mut search_order = vec![source];
    visited[source.as_usize()] = true;
    depths[source.as_usize()] = Some(0_i128);
    let mut stack = vec![DfsFrame {
        arcs: state.outgoing_arcs(source),
        next_arc: 0,
    }];
    while let Some(frame) = stack.last_mut() {
        let Some(arc) = frame.arcs.get(frame.next_arc).cloned() else {
            stack.pop();
            continue;
        };
        frame.next_arc += 1;
        metrics.residual_arc_scans = metrics
            .residual_arc_scans
            .checked_add(1)
            .ok_or(FordFulkersonError::MetricOverflow)?;
        record_residual_arc_scan(recorder.as_deref_mut(), preset, &arc.id)?;
        if arc.capacity < minimum_capacity || visited[arc.to.as_usize()] {
            continue;
        }
        visited[arc.to.as_usize()] = true;
        predecessor[arc.to.as_usize()] = Some(arc.id.clone());
        depths[arc.to.as_usize()] =
            depths[arc.from.as_usize()].and_then(|depth| depth.checked_add(1));
        search_order.push(arc.to);
        if arc.to == sink {
            let path = reconstruct_path(state, source, sink, &predecessor)?;
            let bottleneck = path
                .iter()
                .filter_map(|id| state.arc(id).map(|candidate| candidate.capacity))
                .min();
            return Ok(AugmentingPathSearch {
                path: Some(path),
                labels: depths,
                search_order,
                bottleneck,
            });
        }
        stack.push(DfsFrame {
            arcs: state.outgoing_arcs(arc.to),
            next_arc: 0,
        });
    }
    Ok(AugmentingPathSearch {
        path: None,
        labels: depths,
        search_order,
        bottleneck: None,
    })
}

fn widest_augmenting_path(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    metrics: &mut FordFulkersonMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    preset: FordFulkersonExecutionPreset,
) -> Result<AugmentingPathSearch, FordFulkersonError> {
    let mut recorder = recorder;
    let node_count = state.graph().nodes().len();
    let mut predecessor = vec![None; node_count];
    let mut widths = vec![0_u64; node_count];
    let mut settled = vec![false; node_count];
    let mut search_order = Vec::new();
    let mut heap = BinaryHeap::new();
    widths[source.as_usize()] = u64::MAX;
    heap.push((u64::MAX, Reverse(source)));
    while let Some((width, Reverse(node))) = heap.pop() {
        if settled[node.as_usize()] || width != widths[node.as_usize()] {
            continue;
        }
        settled[node.as_usize()] = true;
        search_order.push(node);
        if node == sink {
            let path = reconstruct_path(state, source, sink, &predecessor)?;
            return Ok(AugmentingPathSearch {
                path: Some(path),
                labels: widest_labels(&widths, source),
                search_order,
                bottleneck: Some(width),
            });
        }
        for arc in state.outgoing_arcs(node) {
            metrics.residual_arc_scans = metrics
                .residual_arc_scans
                .checked_add(1)
                .ok_or(FordFulkersonError::MetricOverflow)?;
            record_residual_arc_scan(recorder.as_deref_mut(), preset, &arc.id)?;
            if settled[arc.to.as_usize()] {
                continue;
            }
            let candidate = width.min(arc.capacity);
            if candidate > widths[arc.to.as_usize()] {
                widths[arc.to.as_usize()] = candidate;
                predecessor[arc.to.as_usize()] = Some(arc.id);
                heap.push((candidate, Reverse(arc.to)));
            }
        }
    }
    Ok(AugmentingPathSearch {
        path: None,
        labels: widest_labels(&widths, source),
        search_order,
        bottleneck: None,
    })
}

fn record_residual_arc_scan(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    preset: FordFulkersonExecutionPreset,
    arc: &ResidualArcId,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    recorder.record_metric_observation(
        FlowTraceEventMetadata {
            catalog_id: preset.inspect_catalog_id(),
            minimum_granularity: TraceGranularityV1::Micro,
            pseudocode_line: "ford-fulkerson:inspect-residual-arc",
        },
        FlowTraceMetricId::ResidualArcScans,
        FlowTraceEntityRef::ResidualArc(arc.clone()),
    )
}

fn widest_labels(widths: &[u64], source: NodeIndex) -> Vec<Option<i128>> {
    let source_width = widths
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != source.as_usize())
        .map(|(_, width)| *width)
        .max()
        .unwrap_or(0);
    widths
        .iter()
        .enumerate()
        .map(|(index, &width)| {
            let normalized = if index == source.as_usize() {
                source_width
            } else {
                width
            };
            (normalized > 0).then_some(i128::from(normalized))
        })
        .collect()
}

fn reconstruct_path(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    predecessor: &[Option<ResidualArcId>],
) -> Result<Vec<ResidualArcId>, FordFulkersonError> {
    let mut reversed = Vec::new();
    let mut cursor = sink;
    while cursor != source {
        let id = predecessor
            .get(cursor.as_usize())
            .and_then(Clone::clone)
            .ok_or(FordFulkersonError::PredecessorInvariant)?;
        let arc = state
            .arc(&id)
            .ok_or(FordFulkersonError::PredecessorInvariant)?;
        if arc.to != cursor {
            return Err(FordFulkersonError::PredecessorInvariant);
        }
        reversed.push(id);
        cursor = arc.from;
        if reversed.len() > state.graph().nodes().len() {
            return Err(FordFulkersonError::PredecessorInvariant);
        }
    }
    reversed.reverse();
    Ok(reversed)
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_edmonds_karp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    fn network(edges: &[(&str, &str, &str, u64, u64)]) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let node_ids = ["a", "b", "s", "t"]
            .into_iter()
            .map(|id| NodeId::parse(id).expect("node id"))
            .collect::<Vec<_>>();
        let graph = FlowNetwork::new(
            node_ids
                .iter()
                .cloned()
                .map(|id| FlowNode::new(id, 0))
                .collect(),
            edges
                .iter()
                .map(|&(id, from, to, lower, capacity)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("edge id"),
                    from: NodeId::parse(from).expect("tail"),
                    to: NodeId::parse(to).expect("head"),
                    lower,
                    capacity,
                    cost: 0,
                })
                .collect(),
        )
        .expect("valid graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        (graph, source, sink)
    }

    #[test]
    fn stable_dfs_matches_the_independent_max_flow_value() {
        let (graph, source, sink) = network(&[
            ("e1", "s", "a", 0, 1_000),
            ("e2", "a", "b", 0, 1),
            ("e3", "b", "t", 0, 1_000),
            ("e4", "s", "b", 0, 1_000),
            ("e5", "a", "t", 0, 1_000),
        ]);
        let expected = solve_edmonds_karp(&graph, source, sink).expect("reference maximum");
        let actual = solve_ford_fulkerson(&graph, source, sink).expect("DFS maximum");

        assert_eq!(actual.certificate.value, expected.certificate.value);
        assert_eq!(actual.certificate.cut_bound, expected.certificate.cut_bound);
        assert!(actual.metrics.augmentations > expected.metrics.augmentations);
        assert_eq!(
            actual.metrics.path_searches,
            actual.metrics.augmentations + 1
        );
    }

    #[test]
    fn widest_path_avoids_the_narrow_cross_edge_selected_by_stable_dfs() {
        let (graph, source, sink) = network(&[
            ("e1", "s", "a", 0, 1_000),
            ("e2", "a", "b", 0, 1),
            ("e3", "b", "t", 0, 1_000),
            ("e4", "s", "b", 0, 1_000),
            ("e5", "a", "t", 0, 1_000),
        ]);
        let dfs = solve_dfs_ford_fulkerson(&graph, source, sink).expect("DFS maximum");
        let widest = solve_widest_augmenting_path(&graph, source, sink).expect("widest maximum");

        assert_eq!(widest.certificate.value, dfs.certificate.value);
        assert_eq!(widest.metrics.augmentations, 2);
        assert!(widest.metrics.augmentations < dfs.metrics.augmentations);
    }

    #[test]
    fn capacity_scaling_records_every_power_of_two_phase_and_exact_delta() {
        let (graph, source, sink) = network(&[
            ("sa", "s", "a", 0, 10),
            ("sb", "s", "b", 0, 6),
            ("at", "a", "t", 0, 10),
            ("bt", "b", "t", 0, 6),
        ]);
        let expected = solve_edmonds_karp(&graph, source, sink).expect("reference maximum");
        let traced = trace_capacity_scaling_augmenting_path(&graph, source, sink)
            .expect("capacity-scaling trace");

        assert_eq!(traced.result.certificate.value, expected.certificate.value);
        assert_eq!(traced.result.metrics.scaling_phases, 4);
        let searches = traced
            .events
            .iter()
            .filter(|event| event.catalog_id == "capacity-scaling-augmenting-path.search")
            .collect::<Vec<_>>();
        assert_eq!(
            searches
                .first()
                .and_then(|event| event.detail.as_ref())
                .map(|detail| (detail.label.as_str(), detail.value)),
            Some(("delta", 8))
        );
        assert_eq!(
            searches
                .last()
                .and_then(|event| event.detail.as_ref())
                .map(|detail| (detail.label.as_str(), detail.value)),
            Some(("delta", 1))
        );
    }

    #[test]
    fn preserves_lower_bounds_parallel_edges_opposites_and_self_loops() {
        let (graph, source, sink) = network(&[
            ("lower", "s", "a", 2, 3),
            ("parallel", "s", "a", 0, 4),
            ("out", "a", "t", 2, 7),
            ("opposite", "a", "s", 0, 1),
            ("loop", "a", "a", 1, 5),
        ]);
        let result = solve_dfs_ford_fulkerson(&graph, source, sink).expect("maximum flow");

        assert_eq!(result.certificate.value, 7);
        assert_eq!(result.certificate.cut_bound, 7);
    }

    #[test]
    fn trace_replays_both_directions_and_matches_fast_result() {
        let (graph, source, sink) = network(&[
            ("sa", "s", "a", 0, 5),
            ("sb", "s", "b", 0, 4),
            ("ab", "a", "b", 0, 2),
            ("at", "a", "t", 0, 3),
            ("bt", "b", "t", 0, 6),
        ]);
        let fast = solve_dfs_ford_fulkerson(&graph, source, sink).expect("fast result");
        let traced = trace_dfs_ford_fulkerson(&graph, source, sink).expect("trace result");

        assert_eq!(traced.result, fast);
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "dfs-ford-fulkerson.search")
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "dfs-ford-fulkerson.augment")
        );
        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward event");
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(replay.flows, fast.flows);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse event");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn search_start_publishes_the_source_as_the_only_active_frontier() {
        let (graph, source, sink) = network(&[("sa", "s", "a", 0, 5), ("at", "a", "t", 0, 5)]);
        let traced = trace_ford_fulkerson(&graph, source, sink).expect("trace result");
        let first = traced.events.first().expect("search-start event");
        let source_id = graph.nodes()[source.as_usize()].id().clone();

        assert_eq!(first.catalog_id, "ford-fulkerson.search-dfs");
        assert_eq!(
            first.entity_refs,
            vec![FlowTraceEntityRef::Node(source_id.clone())]
        );
        let mut replay = traced.base_snapshot.clone();
        apply_trace_event(&graph, &mut replay, first, FlowTraceDirection::Forward)
            .expect("search-start event replays");
        assert_eq!(replay.search_order, vec![source_id]);
        assert!(replay.active_path.is_empty());
    }

    #[test]
    fn all_selectors_match_edmonds_karp_on_bounded_cyclic_capacity_fixtures() {
        const ARCS: [(&str, &str, &str); 10] = [
            ("sa", "s", "a"),
            ("sb", "s", "b"),
            ("st", "s", "t"),
            ("as", "a", "s"),
            ("ab", "a", "b"),
            ("at", "a", "t"),
            ("bs", "b", "s"),
            ("ba", "b", "a"),
            ("bt", "b", "t"),
            ("ta", "t", "a"),
        ];
        for seed in 0_u64..32 {
            let edges = ARCS
                .iter()
                .enumerate()
                .map(|(index, &(id, from, to))| {
                    let capacity = (seed
                        .wrapping_mul(13)
                        .wrapping_add((index as u64).wrapping_mul(23)))
                        % 17;
                    (id, from, to, 0, capacity)
                })
                .collect::<Vec<_>>();
            let (graph, source, sink) = network(&edges);
            let expected = solve_edmonds_karp(&graph, source, sink)
                .expect("reference maximum")
                .certificate
                .value;
            for actual in [
                solve_ford_fulkerson(&graph, source, sink).expect("general selector"),
                solve_dfs_ford_fulkerson(&graph, source, sink).expect("DFS selector"),
                solve_widest_augmenting_path(&graph, source, sink).expect("widest selector"),
                solve_capacity_scaling_augmenting_path(&graph, source, sink)
                    .expect("capacity-scaling selector"),
            ] {
                assert_eq!(actual.certificate.value, expected, "fixture {seed}");
                assert_eq!(actual.certificate.value, actual.certificate.cut_bound);
            }
        }
    }
}
