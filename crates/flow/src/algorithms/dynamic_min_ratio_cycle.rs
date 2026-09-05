//! Bounded topology-aware composition of Algorithm 2 and Definition 4.5.
//!
//! This module joins the checked dynamic tree-chain epoch runtime, stable-slot
//! `FindCycle`, deterministic Shift/Rebuild planning, and exact flow tracking.
//! Root batches may insert/delete edges, split vertices, reinsert active edges,
//! or replace attributes. All branches advance atomically, periodic rebuilds
//! replace the smallest violating suffix, unsuccessful queries shift the
//! largest eligible level, and accepted root circulations update `f`, `Query`,
//! and `Detect` in the stable source-edge universe.
//!
//! The implementation is an exhaustive small-instance realization. It uses
//! explicit integer rebuild limits in place of the paper's asymptotic
//! polylogarithmic threshold, a maintained checked candidate heap, and a
//! canonical-forest link-cut realization of the source tree-flow interface.
//! These bounded components preserve exact source identities without claiming
//! the paper's incremental or amortized runtime bounds.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use thiserror::Error;

use super::{
    BoundedLinkCutFlowCertificate, BoundedLinkCutFlowError, DynamicActiveBranchProjectionInput,
    DynamicCoreGraphStageUpdate, DynamicLevelGraphSnapshot, DynamicLevelProjectionError,
    DynamicMwuCollectionBridgeConfig, DynamicMwuCollectionBridgeError,
    DynamicTreeChainCandidateHeapError, DynamicTreeChainCandidateHeapMetrics,
    DynamicTreeChainCandidateHeapRefreshTrace, DynamicTreeChainCandidateHeapState,
    DynamicTreeChainCycleCandidate, DynamicTreeChainCycleQueryError,
    DynamicTreeChainCycleQueryMetrics, DynamicTreeChainCycleQueryTraceResult,
    DynamicTreeChainEpochError, DynamicTreeChainEpochRuntimeError,
    DynamicTreeChainEpochRuntimeMaterialization, DynamicTreeChainEpochRuntimeOperation,
    DynamicTreeChainEpochRuntimeState, DynamicTreeChainEpochRuntimeTraceResult,
    DynamicTreeChainPropagationError, DynamicTreeChainPropagationInput, ShiftedTreeChainEdge,
    ShiftedTreeChainGraph, apply_bounded_link_cut_flow, check_bounded_link_cut_flow_certificate,
    check_dynamic_tree_chain_candidate_heap_refresh, check_dynamic_tree_chain_cycle_query_trace,
    check_dynamic_tree_chain_epoch_runtime_trace, execute_dynamic_tree_chain_epoch_runtime,
    initialize_dynamic_level_projection, initialize_dynamic_tree_chain_epoch_runtime,
    materialize_dynamic_tree_chain_epoch_runtime, plan_dynamic_tree_chain_rebuild_from_mwu,
    plan_dynamic_tree_chain_shift_from_mwu, trace_dynamic_mwu_sparse_core_collection,
    trace_dynamic_tree_chain_candidate_heap_refresh, trace_dynamic_tree_chain_cycle_query,
    trace_dynamic_tree_chain_epoch_runtime, trace_dynamic_tree_chain_propagation,
};

/// Maximum public operations in one bounded execution.
pub const DYNAMIC_MIN_RATIO_CYCLE_MAX_OPERATIONS: usize = 256;
/// Maximum reversible composition boundaries.
pub const DYNAMIC_MIN_RATIO_CYCLE_MAX_TRACE_EVENTS: usize = 4_096;
/// Maximum internal query/shift attempts in one execution.
pub const DYNAMIC_MIN_RATIO_CYCLE_MAX_QUERIES: u64 = 1_024;
/// Maximum exact numerator or denominator width.
pub const DYNAMIC_MIN_RATIO_CYCLE_MAX_RATIONAL_BITS: u64 = 512;
/// Maximum explicit pass-range exponent.
pub const DYNAMIC_MIN_RATIO_CYCLE_MAX_PSI: u64 = 16;
/// Maximum concrete strict rebuild limit.
pub const DYNAMIC_MIN_RATIO_CYCLE_MAX_REBUILD_LIMIT: u64 = 1_000_000;
/// Denominator in the source IPM progress identity `eta * <g, Delta> = -kappa^2 / 50`.
pub const DYNAMIC_MIN_RATIO_CYCLE_SOURCE_STEP_DENOMINATOR: i64 = 50;

const CATALOG_ID: &str = "dynamic-min-ratio-cycle";

/// Exact bounded Algorithm 2 parameters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicMinRatioCycleConfig {
    /// Source MWU/stable-universe configuration for every dynamic level.
    pub level_configs: Vec<DynamicMwuCollectionBridgeConfig>,
    /// Number of terminal MWU tree branches.
    pub terminal_branches: usize,
    /// Explicit bounded replacement for the source `Psi` range.
    pub psi: u64,
    /// Strict acceptance threshold `kappa * alpha`.
    pub kappa_alpha: BigRational,
    /// Positive Definition 4.5 detection threshold.
    pub epsilon: BigRational,
    /// Strict update-count limit for every dynamic level.
    pub rebuild_after_updates: Vec<u64>,
}

/// One public dynamic min-ratio-cycle request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicMinRatioCycleOperation {
    /// Apply one atomic root topology/attribute stage and route a good cycle.
    Update {
        /// Nonempty ordered root update records.
        updates: Vec<DynamicCoreGraphStageUpdate>,
        /// Exact positive target progress.
        eta: BigRational,
    },
    /// Apply one atomic stage and scale the accepted circulation by its actual
    /// ratio using the source IPM identity `eta * <g, Delta> = -kappa^2 / 50`.
    SourceProgressUpdate {
        /// Nonempty ordered root update records.
        updates: Vec<DynamicCoreGraphStageUpdate>,
    },
    /// Return one maintained stable flow coordinate, including an inactive slot.
    Query {
        /// Stable root edge slot.
        edge: usize,
    },
    /// Return and reset currently active detectable root edges.
    Detect,
}

/// Public response from `Query` or `Detect`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicMinRatioCycleResponse {
    /// Exact stable flow coordinate.
    Query {
        /// Stable root edge slot.
        edge: usize,
        /// Current exact maintained flow.
        flow: BigRational,
    },
    /// Active stable edges crossing the current detection threshold.
    Detect {
        /// Number of completed update stages.
        stage: u64,
        /// Detected active stable slots in increasing order.
        edges: Vec<usize>,
    },
}

/// Exact normalized flow application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicMinRatioCycleFlowApplication {
    /// Exact positive target `-eta * <g, Delta>` before normalization.
    pub target_progress: BigRational,
    /// Exact strictly negative `<g, Delta>`.
    pub gradient_dot: BigRational,
    /// Exact `eta / <g, Delta>`.
    pub beta: BigRational,
    /// Exact stable-slot vector subtracted from maintained flow.
    pub normalized_delta: Vec<BigRational>,
    /// Exact maintained flow after subtraction.
    pub flow: Vec<BigRational>,
    /// Exact canonical-forest link-cut execution and bounded `Detect` evidence.
    pub link_cut_certificate: BoundedLinkCutFlowCertificate,
}

#[derive(Clone, Copy)]
enum ProgressTarget<'a> {
    Fixed(&'a BigRational),
    AcceptedRatioSquared,
}

/// Exact cross-component work counters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DynamicMinRatioCycleMetrics {
    /// Completed root update stages.
    pub updates: u64,
    /// Public flow-coordinate queries.
    pub flow_queries: u64,
    /// Public detection calls.
    pub detect_calls: u64,
    /// Periodic suffix rebuilds.
    pub rebuilds: u64,
    /// Rebuilds by first replaced level.
    pub level_rebuilds: Vec<u64>,
    /// `FindCycle` calls.
    pub cycle_queries: u64,
    /// Candidate qualities refreshed into the bounded source heap.
    pub candidate_heap_refreshes: u64,
    /// Candidate heap pushes.
    pub candidate_heap_pushes: u64,
    /// Candidate heap removals.
    pub candidate_heap_pops: u64,
    /// Retained candidate rows replaced after an exact quality change.
    pub candidate_heap_updates: u64,
    /// Shift calls.
    pub shifts: u64,
    /// Shifts by selected level.
    pub level_shifts: Vec<u64>,
    /// Source records propagated across all level inputs.
    pub propagated_updates: u64,
    /// Intermediate core slots inspected by queries.
    pub intermediate_edge_inspections: u64,
    /// Terminal tree/edge pairs inspected by queries.
    pub terminal_edge_inspections: u64,
    /// Nonzero normalized stable coordinates applied.
    pub flow_coordinate_updates: u64,
    /// Active-edge threshold checks performed by `Detect`.
    pub detection_edge_scans: u64,
    /// Total stable edges returned by `Detect`.
    pub detected_edges: u64,
    /// Reversible public boundaries.
    pub state_transitions: u64,
}

/// Complete topology, schedule, flow, and detection state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicMinRatioCycleSnapshot {
    /// Continued all-level topology/epoch runtime.
    pub runtime: DynamicTreeChainEpochRuntimeState,
    /// Completed root update stages.
    pub stage: u64,
    /// Completed full branch passes at each dynamic level.
    pub passes: Vec<u64>,
    /// Input update records received since each level's last rebuild.
    pub updates_since_rebuild: Vec<u64>,
    /// Last accepted current-root candidate.
    pub last_candidate: Option<DynamicTreeChainCycleCandidate>,
    /// Maintained source-candidate indexed heap across query boundaries.
    pub candidate_heap: DynamicTreeChainCandidateHeapState,
    /// Maintained flow indexed by stable root edge slot.
    pub flow: Vec<BigRational>,
    /// Absolute normalized movement since each slot's last detection.
    pub undetected_absolute_update: Vec<BigRational>,
    /// Last update stage when each slot was returned by `Detect`.
    pub last_detected_stage: Vec<Option<u64>>,
    /// Whether completion was emitted.
    pub complete: bool,
    /// Exact composition counters.
    pub metrics: DynamicMinRatioCycleMetrics,
}

/// Meaning of one reversible composition boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicMinRatioCycleEventKind {
    /// One root stage was atomically propagated through current epochs.
    TopologyStageApplied {
        /// New update stage.
        stage: u64,
        /// Exact records applied to each level input.
        propagated_updates: Vec<u64>,
        /// Checked runtime publication.
        runtime_trace: Box<DynamicTreeChainEpochRuntimeTraceResult>,
    },
    /// The smallest strict-limit violation rebuilt its suffix.
    PeriodicRebuilt {
        /// First rebuilt level.
        level: usize,
        /// Configured strict limit.
        strict_limit: u64,
        /// Counter before suffix reset.
        updates_before_reset: u64,
        /// Checked MWU-backed epoch publication.
        runtime_trace: Box<DynamicTreeChainEpochRuntimeTraceResult>,
    },
    /// One stable-slot `FindCycle` result was tested.
    CycleQueried {
        /// Complete checked query transcript.
        query_trace: Box<DynamicTreeChainCycleQueryTraceResult>,
        /// Maintained source candidate heap refreshed from the checked query rows.
        candidate_heap_trace: Box<DynamicTreeChainCandidateHeapRefreshTrace>,
        /// Whether the strict threshold accepted its best candidate.
        accepted: bool,
    },
    /// The largest eligible level shifted and published a fresh suffix.
    LevelShifted {
        /// Shifted dynamic level.
        level: usize,
        /// Active branch before publication.
        previous_branch: usize,
        /// Active branch after publication.
        next_branch: usize,
        /// Whether the selected branch wrapped to zero.
        wrapped: bool,
        /// Checked MWU-backed epoch publication.
        runtime_trace: Box<DynamicTreeChainEpochRuntimeTraceResult>,
    },
    /// The accepted circulation was normalized and accumulated.
    FlowApplied {
        /// Exact cumulative flow update.
        application: Box<DynamicMinRatioCycleFlowApplication>,
    },
    /// One stable flow coordinate was returned.
    QueryReturned {
        /// Stable root edge slot.
        edge: usize,
        /// Current exact flow.
        flow: BigRational,
    },
    /// One active detection set was returned and reset.
    DetectReturned {
        /// Number of completed update stages.
        stage: u64,
        /// Detected active stable slots.
        edges: Vec<usize>,
    },
    /// Every requested operation completed.
    Completed,
}

/// One fully reversible composition boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicMinRatioCycleTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// Source meaning of this boundary.
    pub kind: DynamicMinRatioCycleEventKind,
    /// State before the boundary.
    pub before: DynamicMinRatioCycleSnapshot,
    /// State after the boundary.
    pub after: DynamicMinRatioCycleSnapshot,
}

/// Exact fast execution result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicMinRatioCycleResult {
    /// Query/detection responses in request order.
    pub responses: Vec<DynamicMinRatioCycleResponse>,
    /// Terminal composed state.
    pub final_snapshot: DynamicMinRatioCycleSnapshot,
}

/// Complete reversible multi-stage transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicMinRatioCycleTraceResult {
    /// Initial checked topology and zero-flow state.
    pub base_snapshot: DynamicMinRatioCycleSnapshot,
    /// All component publications followed by completion.
    pub events: Vec<DynamicMinRatioCycleTraceEvent>,
    /// Exact public result.
    pub result: DynamicMinRatioCycleResult,
}

/// Explicit bounded composition failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicMinRatioCycleError {
    /// Initial runtime, configuration, operation, or stable slot is malformed.
    #[error("dynamic min-ratio-cycle input is invalid")]
    InvalidInput,
    /// The execution exceeds its explicit small-instance band.
    #[error("dynamic min-ratio-cycle exceeds its admission band")]
    AdmissionLimit,
    /// Every level exhausted its pass budget without a good cycle.
    #[error("dynamic min-ratio-cycle strategy exhausted")]
    StrategyExhausted,
    /// One continued-runtime publication failed.
    #[error("dynamic min-ratio-cycle runtime failed: {0}")]
    Runtime(#[from] DynamicTreeChainEpochRuntimeError),
    /// One deterministic MWU epoch plan failed.
    #[error("dynamic min-ratio-cycle epoch plan failed: {0}")]
    Epoch(#[from] DynamicTreeChainEpochError),
    /// One stable-slot query failed.
    #[error("dynamic min-ratio-cycle query failed: {0}")]
    Query(#[from] DynamicTreeChainCycleQueryError),
    /// One bounded candidate-heap rebuild or selection failed.
    #[error("dynamic min-ratio-cycle candidate heap failed: {0}")]
    CandidateHeap(#[from] DynamicTreeChainCandidateHeapError),
    /// One bounded link-cut tree-flow application failed.
    #[error("dynamic min-ratio-cycle link-cut flow failed: {0}")]
    LinkCutFlow(#[from] BoundedLinkCutFlowError),
    /// One initial MWU branch collection failed.
    #[error("dynamic min-ratio-cycle initialization bridge failed: {0}")]
    InitializationBridge(#[from] DynamicMwuCollectionBridgeError),
    /// One initial active-branch projection failed.
    #[error("dynamic min-ratio-cycle initialization projection failed: {0}")]
    InitializationProjection(#[from] DynamicLevelProjectionError),
    /// Initial fixed-epoch propagation failed.
    #[error("dynamic min-ratio-cycle initialization propagation failed: {0}")]
    InitializationPropagation(#[from] DynamicTreeChainPropagationError),
    /// Checked exact work arithmetic overflowed.
    #[error("dynamic min-ratio-cycle arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied transcript is not the exact independent composition.
    #[error("dynamic min-ratio-cycle trace verification failed")]
    TraceVerification,
}

/// Builds a checked continued runtime from one exact root graph and one MWU
/// configuration per dynamic level.
///
/// # Errors
///
/// Rejects an empty level chain or any MWU, projection, propagation, or runtime
/// initialization failure.
pub fn initialize_dynamic_min_ratio_cycle_runtime(
    root_graph: &ShiftedTreeChainGraph,
    level_configs: &[DynamicMwuCollectionBridgeConfig],
) -> Result<DynamicTreeChainEpochRuntimeState, DynamicMinRatioCycleError> {
    if level_configs.is_empty() {
        return Err(DynamicMinRatioCycleError::InvalidInput);
    }
    let mut graph = root_graph.clone();
    let mut levels = Vec::with_capacity(level_configs.len());
    for (level, &config) in level_configs.iter().enumerate() {
        let bridge = trace_dynamic_mwu_sparse_core_collection(&graph, config)?;
        let active_branch = bridge
            .result
            .initialized
            .final_snapshot
            .branch_snapshots
            .first()
            .ok_or(DynamicMinRatioCycleError::InvalidInput)?;
        levels.push(DynamicActiveBranchProjectionInput {
            collection: bridge.result.collection,
            active_branch: 0,
        });
        if level + 1 < level_configs.len() {
            let projection = initialize_dynamic_level_projection(active_branch)?;
            graph = ShiftedTreeChainGraph {
                node_count: projection.graph.active_node_count,
                edges: projection
                    .graph
                    .edge_slots
                    .iter()
                    .flatten()
                    .map(|edge| ShiftedTreeChainEdge {
                        source_edge: edge.edge,
                        from: edge.from,
                        to: edge.to,
                        length: edge.length.clone(),
                        gradient: edge.gradient.clone(),
                    })
                    .collect(),
            };
        }
    }
    let input = DynamicTreeChainPropagationInput { levels };
    let propagation = trace_dynamic_tree_chain_propagation(&input, &[])?;
    initialize_dynamic_tree_chain_epoch_runtime(&input, &[], &propagation).map_err(Into::into)
}

struct InternalRun {
    base_snapshot: DynamicMinRatioCycleSnapshot,
    events: Vec<DynamicMinRatioCycleTraceEvent>,
    result: DynamicMinRatioCycleResult,
}

#[derive(Clone)]
struct SessionCore {
    initial_runtime: DynamicTreeChainEpochRuntimeState,
    config: DynamicMinRatioCycleConfig,
    operations: Vec<DynamicMinRatioCycleOperation>,
    base_snapshot: DynamicMinRatioCycleSnapshot,
    snapshot: DynamicMinRatioCycleSnapshot,
    events: Vec<DynamicMinRatioCycleTraceEvent>,
    responses: Vec<DynamicMinRatioCycleResponse>,
    record: bool,
}

/// Resumable fast Algorithm 2 machine for adaptive callers such as the Flow Framework.
///
/// The current state is readable but not replaceable. Each operation is first
/// evaluated on an owned candidate, so an error leaves the session bit-for-bit
/// unchanged. [`DynamicMinRatioCycleSession::finish`] emits the same terminal
/// result as [`execute_dynamic_min_ratio_cycle`] over the accumulated operations.
#[derive(Clone)]
pub struct DynamicMinRatioCycleSession {
    core: SessionCore,
}

impl DynamicMinRatioCycleSession {
    /// Initializes a zero-flow adaptive session over one checked epoch runtime.
    ///
    /// # Errors
    ///
    /// Rejects an invalid runtime or Algorithm 2 configuration.
    pub fn new(
        initial_runtime: &DynamicTreeChainEpochRuntimeState,
        config: &DynamicMinRatioCycleConfig,
    ) -> Result<Self, DynamicMinRatioCycleError> {
        Ok(Self {
            core: SessionCore::new(initial_runtime, config, false)?,
        })
    }

    /// Applies one adaptive operation atomically.
    ///
    /// # Errors
    ///
    /// Rejects an invalid or out-of-band operation, component failure, or
    /// exhausted shift strategy without changing the session.
    pub fn apply(
        &mut self,
        operation: DynamicMinRatioCycleOperation,
    ) -> Result<Option<DynamicMinRatioCycleResponse>, DynamicMinRatioCycleError> {
        self.core.apply(operation)
    }

    /// Returns the current checked state without exposing mutation authority.
    #[must_use]
    pub fn snapshot(&self) -> &DynamicMinRatioCycleSnapshot {
        &self.core.snapshot
    }

    /// Emits the terminal fast result for all successfully applied operations.
    ///
    /// # Errors
    ///
    /// Returns a checked work-overflow failure while publishing completion.
    pub fn finish(self) -> Result<DynamicMinRatioCycleResult, DynamicMinRatioCycleError> {
        self.core.finish().map(|run| run.result)
    }
}

/// Resumable traced Algorithm 2 machine with one final independent replay check.
///
/// Operations may be chosen after inspecting the preceding immutable snapshot.
/// Finishing produces the ordinary batch transcript, so the existing
/// production-independent checker remains the single transcript oracle.
pub struct DynamicMinRatioCycleTraceSession {
    core: SessionCore,
}

impl DynamicMinRatioCycleTraceSession {
    /// Initializes a traced adaptive session over one checked epoch runtime.
    ///
    /// # Errors
    ///
    /// Rejects an invalid runtime or Algorithm 2 configuration.
    pub fn new(
        initial_runtime: &DynamicTreeChainEpochRuntimeState,
        config: &DynamicMinRatioCycleConfig,
    ) -> Result<Self, DynamicMinRatioCycleError> {
        Ok(Self {
            core: SessionCore::new(initial_runtime, config, true)?,
        })
    }

    /// Applies one adaptive operation and records all nested component boundaries atomically.
    ///
    /// # Errors
    ///
    /// Rejects an invalid or out-of-band operation, component failure, exhausted
    /// shift strategy, or trace-budget exhaustion without changing the session.
    pub fn apply(
        &mut self,
        operation: DynamicMinRatioCycleOperation,
    ) -> Result<Option<DynamicMinRatioCycleResponse>, DynamicMinRatioCycleError> {
        self.core.apply(operation)
    }

    /// Returns the current checked state without exposing mutation authority.
    #[must_use]
    pub fn snapshot(&self) -> &DynamicMinRatioCycleSnapshot {
        &self.core.snapshot
    }

    /// Emits and independently verifies the complete adaptive transcript.
    ///
    /// # Errors
    ///
    /// Returns completion-budget or independent replay-checker failure.
    pub fn finish(self) -> Result<DynamicMinRatioCycleTraceResult, DynamicMinRatioCycleError> {
        let initial_runtime = self.core.initial_runtime.clone();
        let config = self.core.config.clone();
        let operations = self.core.operations.clone();
        let run = self.core.finish()?;
        let trace = DynamicMinRatioCycleTraceResult {
            base_snapshot: run.base_snapshot,
            events: run.events,
            result: run.result,
        };
        check_dynamic_min_ratio_cycle_trace(&initial_runtime, &config, &operations, &trace)?;
        Ok(trace)
    }
}

/// Executes the topology-aware bounded Algorithm 2 composition.
///
/// # Errors
///
/// Rejects malformed/out-of-band input, component failure, strategy
/// exhaustion, or exact arithmetic overflow.
pub fn execute_dynamic_min_ratio_cycle(
    initial_runtime: &DynamicTreeChainEpochRuntimeState,
    config: &DynamicMinRatioCycleConfig,
    operations: &[DynamicMinRatioCycleOperation],
) -> Result<DynamicMinRatioCycleResult, DynamicMinRatioCycleError> {
    run_internal(initial_runtime, config, operations, false).map(|run| run.result)
}

/// Records every topology, rebuild, query, shift, flow, and detection boundary.
///
/// # Errors
///
/// Returns any execution or independent checker failure.
pub fn trace_dynamic_min_ratio_cycle(
    initial_runtime: &DynamicTreeChainEpochRuntimeState,
    config: &DynamicMinRatioCycleConfig,
    operations: &[DynamicMinRatioCycleOperation],
) -> Result<DynamicMinRatioCycleTraceResult, DynamicMinRatioCycleError> {
    let run = run_internal(initial_runtime, config, operations, true)?;
    let trace = DynamicMinRatioCycleTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_dynamic_min_ratio_cycle_trace(initial_runtime, config, operations, &trace)?;
    Ok(trace)
}

/// Independently reconstructs all component publications and public state.
///
/// The checker never invokes the composed runner. Runtime and query component
/// traces are delegated to their independent checkers; scheduling, counters,
/// normalization, cumulative flow, detection, and responses are rebuilt here.
///
/// # Errors
///
/// Rejects input or any component, schedule, candidate, flow, response, metric,
/// event, or terminal-state drift.
pub fn check_dynamic_min_ratio_cycle_trace(
    initial_runtime: &DynamicTreeChainEpochRuntimeState,
    config: &DynamicMinRatioCycleConfig,
    operations: &[DynamicMinRatioCycleOperation],
    trace: &DynamicMinRatioCycleTraceResult,
) -> Result<(), DynamicMinRatioCycleError> {
    let initial_materialization = validate_input(initial_runtime, config, operations, true)?;
    let mut snapshot = initial_snapshot(initial_runtime, config, &initial_materialization);
    if trace.base_snapshot != snapshot {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    let mut event_index = 0_usize;
    let mut responses = Vec::new();
    for operation in operations {
        match operation {
            DynamicMinRatioCycleOperation::Update { updates, eta } => {
                audit_update(
                    config,
                    updates,
                    ProgressTarget::Fixed(eta),
                    trace,
                    &mut snapshot,
                    &mut event_index,
                )?;
            }
            DynamicMinRatioCycleOperation::SourceProgressUpdate { updates } => {
                audit_update(
                    config,
                    updates,
                    ProgressTarget::AcceptedRatioSquared,
                    trace,
                    &mut snapshot,
                    &mut event_index,
                )?;
            }
            DynamicMinRatioCycleOperation::Query { edge } => {
                responses.push(audit_query(*edge, trace, &mut snapshot, &mut event_index)?);
            }
            DynamicMinRatioCycleOperation::Detect => {
                responses.push(audit_detect(
                    config,
                    trace,
                    &mut snapshot,
                    &mut event_index,
                )?);
            }
        }
    }
    let completion = trace
        .events
        .get(event_index)
        .ok_or(DynamicMinRatioCycleError::TraceVerification)?;
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = audit_increment(snapshot.metrics.state_transitions)?;
    audit_match(
        completion,
        &DynamicMinRatioCycleEventKind::Completed,
        &before,
        &snapshot,
    )?;
    if event_index + 1 != trace.events.len()
        || trace.result.responses != responses
        || trace.result.final_snapshot != snapshot
    {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    Ok(())
}

fn run_internal(
    initial_runtime: &DynamicTreeChainEpochRuntimeState,
    config: &DynamicMinRatioCycleConfig,
    operations: &[DynamicMinRatioCycleOperation],
    record: bool,
) -> Result<InternalRun, DynamicMinRatioCycleError> {
    validate_input(initial_runtime, config, operations, false)?;
    let mut session = SessionCore::new(initial_runtime, config, record)?;
    for operation in operations {
        session.apply(operation.clone())?;
    }
    session.finish()
}

impl SessionCore {
    fn new(
        initial_runtime: &DynamicTreeChainEpochRuntimeState,
        config: &DynamicMinRatioCycleConfig,
        record: bool,
    ) -> Result<Self, DynamicMinRatioCycleError> {
        let materialization = validate_input(initial_runtime, config, &[], false)?;
        let snapshot = initial_snapshot(initial_runtime, config, &materialization);
        Ok(Self {
            initial_runtime: initial_runtime.clone(),
            config: config.clone(),
            operations: Vec::new(),
            base_snapshot: snapshot.clone(),
            snapshot,
            events: Vec::new(),
            responses: Vec::new(),
            record,
        })
    }

    fn apply(
        &mut self,
        operation: DynamicMinRatioCycleOperation,
    ) -> Result<Option<DynamicMinRatioCycleResponse>, DynamicMinRatioCycleError> {
        if self.snapshot.complete {
            return Err(DynamicMinRatioCycleError::InvalidInput);
        }
        let next_operation_count = self
            .operations
            .len()
            .checked_add(1)
            .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?;
        if next_operation_count > DYNAMIC_MIN_RATIO_CYCLE_MAX_OPERATIONS {
            return Err(DynamicMinRatioCycleError::AdmissionLimit);
        }
        validate_input(
            &self.initial_runtime,
            &self.config,
            std::slice::from_ref(&operation),
            false,
        )?;

        let mut candidate = self.snapshot.clone();
        let mut candidate_events = Vec::new();
        let response = apply_operation(
            &self.config,
            &operation,
            &mut candidate,
            &mut candidate_events,
            self.record,
        )?;
        if self
            .events
            .len()
            .checked_add(candidate_events.len())
            .is_none_or(|count| count > DYNAMIC_MIN_RATIO_CYCLE_MAX_TRACE_EVENTS)
        {
            return Err(DynamicMinRatioCycleError::AdmissionLimit);
        }

        self.snapshot = candidate;
        self.events.extend(candidate_events);
        self.operations.push(operation);
        if let Some(response) = &response {
            self.responses.push(response.clone());
        }
        Ok(response)
    }

    fn finish(mut self) -> Result<InternalRun, DynamicMinRatioCycleError> {
        if self.snapshot.complete {
            return Err(DynamicMinRatioCycleError::InvalidInput);
        }
        let before = self.snapshot.clone();
        let mut candidate = self.snapshot.clone();
        candidate.complete = true;
        let mut completion = Vec::new();
        publish(
            &mut candidate,
            &mut completion,
            self.record,
            before,
            DynamicMinRatioCycleEventKind::Completed,
        )?;
        if self
            .events
            .len()
            .checked_add(completion.len())
            .is_none_or(|count| count > DYNAMIC_MIN_RATIO_CYCLE_MAX_TRACE_EVENTS)
        {
            return Err(DynamicMinRatioCycleError::AdmissionLimit);
        }
        self.snapshot = candidate;
        self.events.extend(completion);
        Ok(InternalRun {
            base_snapshot: self.base_snapshot,
            events: self.events,
            result: DynamicMinRatioCycleResult {
                responses: self.responses,
                final_snapshot: self.snapshot,
            },
        })
    }
}

fn apply_operation(
    config: &DynamicMinRatioCycleConfig,
    operation: &DynamicMinRatioCycleOperation,
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    events: &mut Vec<DynamicMinRatioCycleTraceEvent>,
    record: bool,
) -> Result<Option<DynamicMinRatioCycleResponse>, DynamicMinRatioCycleError> {
    match operation {
        DynamicMinRatioCycleOperation::Update { updates, eta } => {
            apply_update(
                config,
                updates,
                ProgressTarget::Fixed(eta),
                snapshot,
                events,
                record,
            )?;
            Ok(None)
        }
        DynamicMinRatioCycleOperation::SourceProgressUpdate { updates } => {
            apply_update(
                config,
                updates,
                ProgressTarget::AcceptedRatioSquared,
                snapshot,
                events,
                record,
            )?;
            Ok(None)
        }
        DynamicMinRatioCycleOperation::Query { edge } => {
            let before = snapshot.clone();
            let flow = snapshot.flow[*edge].clone();
            snapshot.metrics.flow_queries = increment(snapshot.metrics.flow_queries)?;
            publish(
                snapshot,
                events,
                record,
                before,
                DynamicMinRatioCycleEventKind::QueryReturned {
                    edge: *edge,
                    flow: flow.clone(),
                },
            )?;
            Ok(Some(DynamicMinRatioCycleResponse::Query {
                edge: *edge,
                flow,
            }))
        }
        DynamicMinRatioCycleOperation::Detect => {
            let before = snapshot.clone();
            let current = materialize_dynamic_tree_chain_epoch_runtime(&snapshot.runtime)?;
            let edges = detection_edges(snapshot, &current, &config.epsilon);
            for &edge in &edges {
                snapshot.undetected_absolute_update[edge] = BigRational::zero();
                snapshot.last_detected_stage[edge] = Some(snapshot.stage);
            }
            account_detection(
                snapshot,
                current.epoch_snapshot.levels[0]
                    .source_graph
                    .edge_slots
                    .iter()
                    .flatten()
                    .count(),
                edges.len(),
            )?;
            let stage = snapshot.stage;
            publish(
                snapshot,
                events,
                record,
                before,
                DynamicMinRatioCycleEventKind::DetectReturned {
                    stage,
                    edges: edges.clone(),
                },
            )?;
            Ok(Some(DynamicMinRatioCycleResponse::Detect { stage, edges }))
        }
    }
}

fn apply_update(
    config: &DynamicMinRatioCycleConfig,
    updates: &[DynamicCoreGraphStageUpdate],
    progress_target: ProgressTarget<'_>,
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    events: &mut Vec<DynamicMinRatioCycleTraceEvent>,
    record: bool,
) -> Result<(), DynamicMinRatioCycleError> {
    let mut current_materialization = if updates.is_empty() {
        materialize_dynamic_tree_chain_epoch_runtime(&snapshot.runtime)?
    } else {
        apply_attribute_stage(updates, snapshot, events, record)?
    };

    if !updates.is_empty() {
        current_materialization =
            apply_periodic_rebuild(config, snapshot, events, record, current_materialization)?;
    }

    let candidate = loop {
        if snapshot.metrics.cycle_queries >= DYNAMIC_MIN_RATIO_CYCLE_MAX_QUERIES {
            return Err(DynamicMinRatioCycleError::AdmissionLimit);
        }
        let before = snapshot.clone();
        let query_trace =
            trace_dynamic_tree_chain_cycle_query(&snapshot.runtime, config.terminal_branches)?;
        let candidate_heap_trace = trace_dynamic_tree_chain_candidate_heap_refresh(
            &snapshot.candidate_heap,
            &query_trace,
        )?;
        let candidate = candidate_heap_trace.selected.clone();
        let metrics = query_trace.result.final_snapshot.metrics;
        if candidate != query_trace.result.best_candidate {
            return Err(DynamicMinRatioCycleError::InvalidInput);
        }
        let accepted = candidate
            .as_ref()
            .is_some_and(|candidate| candidate.ratio > config.kappa_alpha);
        add_query_metrics(&mut snapshot.metrics, metrics)?;
        add_candidate_heap_metrics(&mut snapshot.metrics, candidate_heap_trace.metrics)?;
        snapshot
            .candidate_heap
            .clone_from(&candidate_heap_trace.after);
        if accepted {
            snapshot.last_candidate.clone_from(&candidate);
        }
        let kind = record.then(|| DynamicMinRatioCycleEventKind::CycleQueried {
            query_trace: Box::new(query_trace),
            candidate_heap_trace: Box::new(candidate_heap_trace),
            accepted,
        });
        publish_optional(snapshot, events, record, before, kind)?;
        if accepted {
            break candidate.ok_or(DynamicMinRatioCycleError::StrategyExhausted)?;
        }
        current_materialization =
            apply_shift(config, snapshot, events, record, &current_materialization)?;
    };

    let before = snapshot.clone();
    let target_progress = resolve_progress_target(progress_target, &candidate);
    let root_graph = &current_materialization.epoch_snapshot.levels[0].source_graph;
    let application = flow_application(
        snapshot,
        root_graph,
        &config.epsilon,
        &target_progress,
        &candidate,
    )?;
    apply_flow(snapshot, &application)?;
    publish(
        snapshot,
        events,
        record,
        before,
        DynamicMinRatioCycleEventKind::FlowApplied {
            application: Box::new(application),
        },
    )
}

fn apply_attribute_stage(
    updates: &[DynamicCoreGraphStageUpdate],
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    events: &mut Vec<DynamicMinRatioCycleTraceEvent>,
    record: bool,
) -> Result<DynamicTreeChainEpochRuntimeMaterialization, DynamicMinRatioCycleError> {
    let before = snapshot.clone();
    let operation = DynamicTreeChainEpochRuntimeOperation::ApplyRootStage {
        updates: updates.to_vec(),
    };
    let (runtime, materialization, runtime_trace) =
        apply_runtime(&snapshot.runtime, &operation, record)?;
    snapshot.runtime = runtime;
    let propagated = propagated_counts(&snapshot.runtime)?;
    snapshot.stage = increment(snapshot.stage)?;
    add_propagated(snapshot, &propagated)?;
    snapshot.metrics.updates = increment(snapshot.metrics.updates)?;
    let kind =
        runtime_trace.map(
            |runtime_trace| DynamicMinRatioCycleEventKind::TopologyStageApplied {
                stage: snapshot.stage,
                propagated_updates: propagated,
                runtime_trace: Box::new(runtime_trace),
            },
        );
    publish_optional(snapshot, events, record, before, kind)?;
    Ok(materialization)
}

fn apply_periodic_rebuild(
    config: &DynamicMinRatioCycleConfig,
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    events: &mut Vec<DynamicMinRatioCycleTraceEvent>,
    record: bool,
    materialization: DynamicTreeChainEpochRuntimeMaterialization,
) -> Result<DynamicTreeChainEpochRuntimeMaterialization, DynamicMinRatioCycleError> {
    let Some(level) = first_rebuild_level(snapshot, config) else {
        return Ok(materialization);
    };
    let before = snapshot.clone();
    let strict_limit = config.rebuild_after_updates[level];
    let updates_before_reset = snapshot.updates_since_rebuild[level];
    let plan = plan_dynamic_tree_chain_rebuild_from_mwu(
        &materialization.epoch_snapshot,
        level,
        &config.level_configs[level..],
    )?;
    let operation = DynamicTreeChainEpochRuntimeOperation::ApplyEpochPlan {
        plan: Box::new(plan),
    };
    let (runtime, materialization, runtime_trace) =
        apply_runtime(&snapshot.runtime, &operation, record)?;
    snapshot.runtime = runtime;
    reset_suffix(snapshot, level)?;
    snapshot.metrics.rebuilds = increment(snapshot.metrics.rebuilds)?;
    snapshot.metrics.level_rebuilds[level] = increment(snapshot.metrics.level_rebuilds[level])?;
    let kind = runtime_trace.map(
        |runtime_trace| DynamicMinRatioCycleEventKind::PeriodicRebuilt {
            level,
            strict_limit,
            updates_before_reset,
            runtime_trace: Box::new(runtime_trace),
        },
    );
    publish_optional(snapshot, events, record, before, kind)?;
    Ok(materialization)
}

fn apply_shift(
    config: &DynamicMinRatioCycleConfig,
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    events: &mut Vec<DynamicMinRatioCycleTraceEvent>,
    record: bool,
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
) -> Result<DynamicTreeChainEpochRuntimeMaterialization, DynamicMinRatioCycleError> {
    let level = largest_eligible_level(&snapshot.passes, config.psi)
        .ok_or(DynamicMinRatioCycleError::StrategyExhausted)?;
    let previous = materialization.epoch_snapshot.levels[level].active_branch;
    let plan = plan_dynamic_tree_chain_shift_from_mwu(
        &materialization.epoch_snapshot,
        level,
        &config.level_configs[level + 1..],
    )?;
    let operation = DynamicTreeChainEpochRuntimeOperation::ApplyEpochPlan {
        plan: Box::new(plan),
    };
    let before = snapshot.clone();
    let (runtime, next_materialization, runtime_trace) =
        apply_runtime(&snapshot.runtime, &operation, record)?;
    let next = next_materialization.epoch_snapshot.levels[level].active_branch;
    let wrapped = next == 0;
    snapshot.runtime = runtime;
    update_passes(&mut snapshot.passes, level, wrapped)?;
    snapshot.metrics.shifts = increment(snapshot.metrics.shifts)?;
    snapshot.metrics.level_shifts[level] = increment(snapshot.metrics.level_shifts[level])?;
    let kind = runtime_trace.map(
        |runtime_trace| DynamicMinRatioCycleEventKind::LevelShifted {
            level,
            previous_branch: previous,
            next_branch: next,
            wrapped,
            runtime_trace: Box::new(runtime_trace),
        },
    );
    publish_optional(snapshot, events, record, before, kind)?;
    Ok(next_materialization)
}

fn apply_runtime(
    initial: &DynamicTreeChainEpochRuntimeState,
    operation: &DynamicTreeChainEpochRuntimeOperation,
    record: bool,
) -> Result<
    (
        DynamicTreeChainEpochRuntimeState,
        DynamicTreeChainEpochRuntimeMaterialization,
        Option<DynamicTreeChainEpochRuntimeTraceResult>,
    ),
    DynamicMinRatioCycleError,
> {
    if record {
        let trace = trace_dynamic_tree_chain_epoch_runtime(initial, operation)?;
        Ok((
            trace.result.final_state.clone(),
            trace.result.final_materialization.clone(),
            Some(trace),
        ))
    } else {
        let result = execute_dynamic_tree_chain_epoch_runtime(initial, operation)?;
        Ok((result.final_state, result.final_materialization, None))
    }
}

fn audit_update(
    config: &DynamicMinRatioCycleConfig,
    updates: &[DynamicCoreGraphStageUpdate],
    progress_target: ProgressTarget<'_>,
    trace: &DynamicMinRatioCycleTraceResult,
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    event_index: &mut usize,
) -> Result<(), DynamicMinRatioCycleError> {
    if !updates.is_empty() {
        audit_topology_stage(config, updates, trace, snapshot, event_index)?;
    }
    let candidate = loop {
        let event = audit_event(trace, snapshot, *event_index)?;
        let DynamicMinRatioCycleEventKind::CycleQueried {
            query_trace,
            candidate_heap_trace,
            accepted,
        } = &event.kind
        else {
            return Err(DynamicMinRatioCycleError::TraceVerification);
        };
        check_dynamic_tree_chain_cycle_query_trace(
            &snapshot.runtime,
            config.terminal_branches,
            query_trace,
        )?;
        check_dynamic_tree_chain_candidate_heap_refresh(
            query_trace,
            &snapshot.candidate_heap,
            candidate_heap_trace,
        )
        .map_err(|_| DynamicMinRatioCycleError::TraceVerification)?;
        let candidate = candidate_heap_trace.selected.clone();
        if candidate != query_trace.result.best_candidate {
            return Err(DynamicMinRatioCycleError::TraceVerification);
        }
        let expected_accepted = candidate
            .as_ref()
            .is_some_and(|candidate| candidate.ratio > config.kappa_alpha);
        let mut after = snapshot.clone();
        audit_add_query_metrics(
            &mut after.metrics,
            query_trace.result.final_snapshot.metrics,
        )?;
        audit_add_candidate_heap_metrics(&mut after.metrics, candidate_heap_trace.metrics)?;
        after.candidate_heap.clone_from(&candidate_heap_trace.after);
        if expected_accepted {
            after.last_candidate.clone_from(&candidate);
        }
        after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
        if *accepted != expected_accepted || event.after != after {
            return Err(DynamicMinRatioCycleError::TraceVerification);
        }
        *snapshot = after;
        *event_index += 1;
        if expected_accepted {
            break candidate.ok_or(DynamicMinRatioCycleError::TraceVerification)?;
        }
        audit_shift(config, trace, snapshot, event_index)?;
    };
    let event = audit_event(trace, snapshot, *event_index)?;
    let target_progress = resolve_progress_target(progress_target, &candidate);
    let DynamicMinRatioCycleEventKind::FlowApplied { application } = &event.kind else {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    };
    let current = materialize_dynamic_tree_chain_epoch_runtime(&snapshot.runtime)?;
    let application = audit_flow_application(
        snapshot,
        &current.epoch_snapshot.levels[0].source_graph,
        &config.epsilon,
        &target_progress,
        &candidate,
        application,
    )?;
    let mut after = snapshot.clone();
    audit_apply_flow(&mut after, &application)?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    audit_match(
        event,
        &DynamicMinRatioCycleEventKind::FlowApplied {
            application: Box::new(application),
        },
        snapshot,
        &after,
    )?;
    *snapshot = after;
    *event_index += 1;
    Ok(())
}

fn audit_topology_stage(
    config: &DynamicMinRatioCycleConfig,
    updates: &[DynamicCoreGraphStageUpdate],
    trace: &DynamicMinRatioCycleTraceResult,
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    event_index: &mut usize,
) -> Result<(), DynamicMinRatioCycleError> {
    let event = audit_event(trace, snapshot, *event_index)?;
    let DynamicMinRatioCycleEventKind::TopologyStageApplied {
        stage,
        propagated_updates,
        runtime_trace,
    } = &event.kind
    else {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    };
    let operation = DynamicTreeChainEpochRuntimeOperation::ApplyRootStage {
        updates: updates.to_vec(),
    };
    check_dynamic_tree_chain_epoch_runtime_trace(&snapshot.runtime, &operation, runtime_trace)?;
    let propagated = audit_propagated_counts(&runtime_trace.result.final_state)?;
    let mut after = snapshot.clone();
    after.runtime = runtime_trace.result.final_state.clone();
    after.stage = audit_increment(after.stage)?;
    audit_add_propagated(&mut after, &propagated)?;
    after.metrics.updates = audit_increment(after.metrics.updates)?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    if *stage != after.stage || *propagated_updates != propagated || event.after != after {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    *snapshot = after;
    *event_index += 1;
    if let Some(level) = first_rebuild_level(snapshot, config) {
        audit_rebuild(config, level, trace, snapshot, event_index)?;
    }
    Ok(())
}

fn audit_rebuild(
    config: &DynamicMinRatioCycleConfig,
    level: usize,
    trace: &DynamicMinRatioCycleTraceResult,
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    event_index: &mut usize,
) -> Result<(), DynamicMinRatioCycleError> {
    let event = audit_event(trace, snapshot, *event_index)?;
    let DynamicMinRatioCycleEventKind::PeriodicRebuilt {
        level: actual_level,
        strict_limit,
        updates_before_reset,
        runtime_trace,
    } = &event.kind
    else {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    };
    let current = materialize_dynamic_tree_chain_epoch_runtime(&snapshot.runtime)?;
    let expected_plan = plan_dynamic_tree_chain_rebuild_from_mwu(
        &current.epoch_snapshot,
        level,
        &config.level_configs[level..],
    )?;
    let operation = DynamicTreeChainEpochRuntimeOperation::ApplyEpochPlan {
        plan: Box::new(expected_plan),
    };
    check_dynamic_tree_chain_epoch_runtime_trace(&snapshot.runtime, &operation, runtime_trace)?;
    let mut after = snapshot.clone();
    after.runtime = runtime_trace.result.final_state.clone();
    reset_suffix_audit(&mut after, level)?;
    after.metrics.rebuilds = audit_increment(after.metrics.rebuilds)?;
    after.metrics.level_rebuilds[level] = audit_increment(after.metrics.level_rebuilds[level])?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    if *actual_level != level
        || *strict_limit != config.rebuild_after_updates[level]
        || *updates_before_reset != snapshot.updates_since_rebuild[level]
        || event.after != after
    {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    *snapshot = after;
    *event_index += 1;
    Ok(())
}

fn audit_shift(
    config: &DynamicMinRatioCycleConfig,
    trace: &DynamicMinRatioCycleTraceResult,
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    event_index: &mut usize,
) -> Result<(), DynamicMinRatioCycleError> {
    let event = audit_event(trace, snapshot, *event_index)?;
    let DynamicMinRatioCycleEventKind::LevelShifted {
        level,
        previous_branch,
        next_branch,
        wrapped,
        runtime_trace,
    } = &event.kind
    else {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    };
    let expected_level = largest_eligible_level(&snapshot.passes, config.psi)
        .ok_or(DynamicMinRatioCycleError::TraceVerification)?;
    let current = materialize_dynamic_tree_chain_epoch_runtime(&snapshot.runtime)?;
    let expected_previous = current.epoch_snapshot.levels[expected_level].active_branch;
    let expected_plan = plan_dynamic_tree_chain_shift_from_mwu(
        &current.epoch_snapshot,
        expected_level,
        &config.level_configs[expected_level + 1..],
    )?;
    let operation = DynamicTreeChainEpochRuntimeOperation::ApplyEpochPlan {
        plan: Box::new(expected_plan),
    };
    check_dynamic_tree_chain_epoch_runtime_trace(&snapshot.runtime, &operation, runtime_trace)?;
    let expected_next = runtime_trace
        .result
        .final_materialization
        .epoch_snapshot
        .levels[expected_level]
        .active_branch;
    let expected_wrapped = expected_next == 0;
    let mut after = snapshot.clone();
    after.runtime = runtime_trace.result.final_state.clone();
    update_passes_audit(&mut after.passes, expected_level, expected_wrapped)?;
    after.metrics.shifts = audit_increment(after.metrics.shifts)?;
    after.metrics.level_shifts[expected_level] =
        audit_increment(after.metrics.level_shifts[expected_level])?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    if *level != expected_level
        || *previous_branch != expected_previous
        || *next_branch != expected_next
        || *wrapped != expected_wrapped
        || event.after != after
    {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    *snapshot = after;
    *event_index += 1;
    Ok(())
}

fn audit_query(
    edge: usize,
    trace: &DynamicMinRatioCycleTraceResult,
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    event_index: &mut usize,
) -> Result<DynamicMinRatioCycleResponse, DynamicMinRatioCycleError> {
    let event = audit_event(trace, snapshot, *event_index)?;
    let flow = snapshot
        .flow
        .get(edge)
        .ok_or(DynamicMinRatioCycleError::TraceVerification)?
        .clone();
    let mut after = snapshot.clone();
    after.metrics.flow_queries = audit_increment(after.metrics.flow_queries)?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    audit_match(
        event,
        &DynamicMinRatioCycleEventKind::QueryReturned {
            edge,
            flow: flow.clone(),
        },
        snapshot,
        &after,
    )?;
    *snapshot = after;
    *event_index += 1;
    Ok(DynamicMinRatioCycleResponse::Query { edge, flow })
}

fn audit_detect(
    config: &DynamicMinRatioCycleConfig,
    trace: &DynamicMinRatioCycleTraceResult,
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    event_index: &mut usize,
) -> Result<DynamicMinRatioCycleResponse, DynamicMinRatioCycleError> {
    let event = audit_event(trace, snapshot, *event_index)?;
    let current = materialize_dynamic_tree_chain_epoch_runtime(&snapshot.runtime)?;
    let edges = audit_detection_edges(snapshot, &current, &config.epsilon)?;
    let mut after = snapshot.clone();
    for &edge in &edges {
        after.undetected_absolute_update[edge] = BigRational::zero();
        after.last_detected_stage[edge] = Some(after.stage);
    }
    audit_account_detection(
        &mut after,
        current.epoch_snapshot.levels[0]
            .source_graph
            .edge_slots
            .iter()
            .flatten()
            .count(),
        edges.len(),
    )?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    audit_match(
        event,
        &DynamicMinRatioCycleEventKind::DetectReturned {
            stage: after.stage,
            edges: edges.clone(),
        },
        snapshot,
        &after,
    )?;
    *snapshot = after;
    *event_index += 1;
    Ok(DynamicMinRatioCycleResponse::Detect {
        stage: snapshot.stage,
        edges,
    })
}

fn validate_input(
    initial_runtime: &DynamicTreeChainEpochRuntimeState,
    config: &DynamicMinRatioCycleConfig,
    operations: &[DynamicMinRatioCycleOperation],
    audit: bool,
) -> Result<DynamicTreeChainEpochRuntimeMaterialization, DynamicMinRatioCycleError> {
    let materialization = materialize_dynamic_tree_chain_epoch_runtime(initial_runtime)?;
    let levels = initial_runtime.levels.len();
    if levels == 0
        || config.level_configs.len() != levels
        || config.rebuild_after_updates.len() != levels
        || config.terminal_branches == 0
        || config.psi == 0
        || config.kappa_alpha <= BigRational::zero()
        || config.epsilon <= BigRational::zero()
    {
        return Err(input_failure(audit));
    }
    if operations.len() > DYNAMIC_MIN_RATIO_CYCLE_MAX_OPERATIONS
        || config.psi > DYNAMIC_MIN_RATIO_CYCLE_MAX_PSI
        || config
            .rebuild_after_updates
            .iter()
            .any(|limit| *limit > DYNAMIC_MIN_RATIO_CYCLE_MAX_REBUILD_LIMIT)
        || rational_too_wide(&config.kappa_alpha)
        || rational_too_wide(&config.epsilon)
    {
        return Err(admission_failure(audit));
    }
    let slots = materialization.epoch_snapshot.levels[0]
        .source_graph
        .edge_slots
        .len();
    for (level, level_config) in config.level_configs.iter().enumerate() {
        let runtime_level = &initial_runtime.levels[level];
        if level_config.branches != runtime_level.input.collection.branches.len()
            || level_config.stable_edge_slots
                != runtime_level.input.collection.branches[0]
                    .core
                    .forest
                    .edge_slots
                    .len()
        {
            return Err(input_failure(audit));
        }
    }
    for operation in operations {
        match operation {
            DynamicMinRatioCycleOperation::Update { updates, eta } => {
                if updates.is_empty() || *eta <= BigRational::zero() {
                    return Err(input_failure(audit));
                }
                if rational_too_wide(eta) {
                    return Err(admission_failure(audit));
                }
            }
            DynamicMinRatioCycleOperation::Query { edge } if *edge >= slots => {
                return Err(input_failure(audit));
            }
            DynamicMinRatioCycleOperation::SourceProgressUpdate { .. }
            | DynamicMinRatioCycleOperation::Query { .. }
            | DynamicMinRatioCycleOperation::Detect => {}
        }
    }
    Ok(materialization)
}

fn initial_snapshot(
    initial_runtime: &DynamicTreeChainEpochRuntimeState,
    config: &DynamicMinRatioCycleConfig,
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
) -> DynamicMinRatioCycleSnapshot {
    let slots = materialization.epoch_snapshot.levels[0]
        .source_graph
        .edge_slots
        .len();
    let levels = initial_runtime.levels.len();
    DynamicMinRatioCycleSnapshot {
        runtime: initial_runtime.clone(),
        stage: 0,
        passes: vec![0; levels],
        updates_since_rebuild: vec![0; levels],
        last_candidate: None,
        candidate_heap: DynamicTreeChainCandidateHeapState::default(),
        flow: vec![BigRational::zero(); slots],
        undetected_absolute_update: vec![BigRational::zero(); slots],
        last_detected_stage: vec![None; slots],
        complete: false,
        metrics: DynamicMinRatioCycleMetrics {
            level_rebuilds: vec![0; config.level_configs.len()],
            level_shifts: vec![0; config.level_configs.len()],
            ..DynamicMinRatioCycleMetrics::default()
        },
    }
}

fn propagated_counts(
    state: &DynamicTreeChainEpochRuntimeState,
) -> Result<Vec<u64>, DynamicMinRatioCycleError> {
    state
        .levels
        .iter()
        .map(|level| {
            let batch = level
                .batches
                .last()
                .ok_or(DynamicMinRatioCycleError::InvalidInput)?;
            u64::try_from(batch.updates.len())
                .map_err(|_| DynamicMinRatioCycleError::ArithmeticOverflow)
        })
        .collect()
}

fn audit_propagated_counts(
    state: &DynamicTreeChainEpochRuntimeState,
) -> Result<Vec<u64>, DynamicMinRatioCycleError> {
    state
        .levels
        .iter()
        .map(|level| {
            level
                .batches
                .last()
                .and_then(|batch| u64::try_from(batch.updates.len()).ok())
                .ok_or(DynamicMinRatioCycleError::TraceVerification)
        })
        .collect()
}

fn add_propagated(
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    updates: &[u64],
) -> Result<(), DynamicMinRatioCycleError> {
    for (total, update) in snapshot.updates_since_rebuild.iter_mut().zip(updates) {
        *total = total
            .checked_add(*update)
            .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?;
        snapshot.metrics.propagated_updates = snapshot
            .metrics
            .propagated_updates
            .checked_add(*update)
            .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?;
    }
    Ok(())
}

fn audit_add_propagated(
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    updates: &[u64],
) -> Result<(), DynamicMinRatioCycleError> {
    if updates.len() != snapshot.updates_since_rebuild.len() {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    for (total, update) in snapshot.updates_since_rebuild.iter_mut().zip(updates) {
        *total = total
            .checked_add(*update)
            .ok_or(DynamicMinRatioCycleError::TraceVerification)?;
        snapshot.metrics.propagated_updates = snapshot
            .metrics
            .propagated_updates
            .checked_add(*update)
            .ok_or(DynamicMinRatioCycleError::TraceVerification)?;
    }
    Ok(())
}

fn first_rebuild_level(
    snapshot: &DynamicMinRatioCycleSnapshot,
    config: &DynamicMinRatioCycleConfig,
) -> Option<usize> {
    snapshot
        .updates_since_rebuild
        .iter()
        .zip(&config.rebuild_after_updates)
        .position(|(updates, limit)| updates > limit)
}

fn reset_suffix(
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    level: usize,
) -> Result<(), DynamicMinRatioCycleError> {
    snapshot
        .passes
        .get_mut(level..)
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?
        .fill(0);
    snapshot
        .updates_since_rebuild
        .get_mut(level..)
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?
        .fill(0);
    Ok(())
}

fn reset_suffix_audit(
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    level: usize,
) -> Result<(), DynamicMinRatioCycleError> {
    if level >= snapshot.passes.len() || level >= snapshot.updates_since_rebuild.len() {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    snapshot.passes[level..].fill(0);
    snapshot.updates_since_rebuild[level..].fill(0);
    Ok(())
}

fn largest_eligible_level(passes: &[u64], psi: u64) -> Option<usize> {
    let ceiling = psi.checked_mul(2)?;
    passes.iter().rposition(|passes| *passes < ceiling)
}

fn update_passes(
    passes: &mut [u64],
    level: usize,
    wrapped: bool,
) -> Result<(), DynamicMinRatioCycleError> {
    if wrapped {
        passes[level] = increment(passes[level])?;
    }
    passes
        .get_mut(level + 1..)
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?
        .fill(0);
    Ok(())
}

fn update_passes_audit(
    passes: &mut [u64],
    level: usize,
    wrapped: bool,
) -> Result<(), DynamicMinRatioCycleError> {
    if level >= passes.len() {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    if wrapped {
        passes[level] = audit_increment(passes[level])?;
    }
    passes[level + 1..].fill(0);
    Ok(())
}

fn flow_application(
    snapshot: &DynamicMinRatioCycleSnapshot,
    graph: &DynamicLevelGraphSnapshot,
    epsilon: &BigRational,
    target_progress: &BigRational,
    candidate: &DynamicTreeChainCycleCandidate,
) -> Result<DynamicMinRatioCycleFlowApplication, DynamicMinRatioCycleError> {
    if candidate.coefficients.len() != snapshot.flow.len()
        || candidate.gradient >= BigRational::zero()
        || target_progress <= &BigRational::zero()
    {
        return Err(DynamicMinRatioCycleError::InvalidInput);
    }
    let beta = target_progress / &candidate.gradient;
    let normalized_delta = candidate
        .coefficients
        .iter()
        .map(|coefficient| &beta * coefficient)
        .collect::<Vec<_>>();
    if rational_too_wide(&beta) || normalized_delta.iter().any(rational_too_wide) {
        return Err(DynamicMinRatioCycleError::AdmissionLimit);
    }
    let link_cut_certificate = apply_bounded_link_cut_flow(
        graph,
        &snapshot.flow,
        &snapshot.undetected_absolute_update,
        &normalized_delta,
        epsilon,
    )?;
    let flow = link_cut_certificate.final_flow.clone();
    if link_cut_certificate.normalized_gradient_dot != &beta * &candidate.gradient
        || link_cut_certificate.normalized_weighted_length
            != beta.abs() * &candidate.weighted_length
        || flow.iter().any(rational_too_wide)
    {
        return Err(DynamicMinRatioCycleError::InvalidInput);
    }
    Ok(DynamicMinRatioCycleFlowApplication {
        target_progress: target_progress.clone(),
        gradient_dot: candidate.gradient.clone(),
        beta,
        normalized_delta,
        flow,
        link_cut_certificate,
    })
}

fn audit_flow_application(
    snapshot: &DynamicMinRatioCycleSnapshot,
    graph: &DynamicLevelGraphSnapshot,
    epsilon: &BigRational,
    target_progress: &BigRational,
    candidate: &DynamicTreeChainCycleCandidate,
    supplied: &DynamicMinRatioCycleFlowApplication,
) -> Result<DynamicMinRatioCycleFlowApplication, DynamicMinRatioCycleError> {
    if candidate.coefficients.len() != snapshot.flow.len()
        || candidate.gradient >= BigRational::zero()
        || target_progress <= &BigRational::zero()
    {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    let beta = target_progress / &candidate.gradient;
    let normalized_delta = candidate
        .coefficients
        .iter()
        .map(|coefficient| &beta * coefficient)
        .collect::<Vec<_>>();
    check_bounded_link_cut_flow_certificate(
        graph,
        &snapshot.flow,
        &snapshot.undetected_absolute_update,
        &normalized_delta,
        epsilon,
        &supplied.link_cut_certificate,
    )
    .map_err(|_| DynamicMinRatioCycleError::TraceVerification)?;
    let expected = DynamicMinRatioCycleFlowApplication {
        target_progress: target_progress.clone(),
        gradient_dot: candidate.gradient.clone(),
        beta,
        normalized_delta,
        flow: supplied.link_cut_certificate.final_flow.clone(),
        link_cut_certificate: supplied.link_cut_certificate.clone(),
    };
    if supplied != &expected
        || expected.link_cut_certificate.normalized_gradient_dot
            != &expected.beta * &candidate.gradient
        || expected.link_cut_certificate.normalized_weighted_length
            != expected.beta.abs() * &candidate.weighted_length
    {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    Ok(expected)
}

fn resolve_progress_target(
    target: ProgressTarget<'_>,
    candidate: &DynamicTreeChainCycleCandidate,
) -> BigRational {
    match target {
        ProgressTarget::Fixed(target) => target.clone(),
        ProgressTarget::AcceptedRatioSquared => {
            &candidate.ratio * &candidate.ratio
                / BigRational::from_integer(BigInt::from(
                    DYNAMIC_MIN_RATIO_CYCLE_SOURCE_STEP_DENOMINATOR,
                ))
        }
    }
}

fn apply_flow(
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    application: &DynamicMinRatioCycleFlowApplication,
) -> Result<(), DynamicMinRatioCycleError> {
    snapshot.flow.clone_from(&application.flow);
    snapshot
        .undetected_absolute_update
        .clone_from(&application.link_cut_certificate.final_movement);
    let nonzero = application
        .normalized_delta
        .iter()
        .filter(|value| !value.is_zero())
        .count();
    snapshot.metrics.flow_coordinate_updates = snapshot
        .metrics
        .flow_coordinate_updates
        .checked_add(
            u64::try_from(nonzero).map_err(|_| DynamicMinRatioCycleError::ArithmeticOverflow)?,
        )
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?;
    Ok(())
}

fn audit_apply_flow(
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    application: &DynamicMinRatioCycleFlowApplication,
) -> Result<(), DynamicMinRatioCycleError> {
    apply_flow(snapshot, application).map_err(|_| DynamicMinRatioCycleError::TraceVerification)
}

fn detection_edges(
    snapshot: &DynamicMinRatioCycleSnapshot,
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
    epsilon: &BigRational,
) -> Vec<usize> {
    materialization.epoch_snapshot.levels[0]
        .source_graph
        .edge_slots
        .iter()
        .enumerate()
        .filter_map(|(edge, row)| {
            row.as_ref().and_then(|row| {
                (&row.length * &snapshot.undetected_absolute_update[edge] >= *epsilon)
                    .then_some(edge)
            })
        })
        .collect()
}

fn audit_detection_edges(
    snapshot: &DynamicMinRatioCycleSnapshot,
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
    epsilon: &BigRational,
) -> Result<Vec<usize>, DynamicMinRatioCycleError> {
    if materialization.epoch_snapshot.levels[0]
        .source_graph
        .edge_slots
        .len()
        != snapshot.undetected_absolute_update.len()
    {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    Ok(detection_edges(snapshot, materialization, epsilon))
}

fn account_detection(
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    scanned: usize,
    detected: usize,
) -> Result<(), DynamicMinRatioCycleError> {
    snapshot.metrics.detect_calls = increment(snapshot.metrics.detect_calls)?;
    snapshot.metrics.detection_edge_scans = snapshot
        .metrics
        .detection_edge_scans
        .checked_add(
            u64::try_from(scanned).map_err(|_| DynamicMinRatioCycleError::ArithmeticOverflow)?,
        )
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?;
    snapshot.metrics.detected_edges = snapshot
        .metrics
        .detected_edges
        .checked_add(
            u64::try_from(detected).map_err(|_| DynamicMinRatioCycleError::ArithmeticOverflow)?,
        )
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?;
    Ok(())
}

fn audit_account_detection(
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    scanned: usize,
    detected: usize,
) -> Result<(), DynamicMinRatioCycleError> {
    account_detection(snapshot, scanned, detected)
        .map_err(|_| DynamicMinRatioCycleError::TraceVerification)
}

fn add_query_metrics(
    metrics: &mut DynamicMinRatioCycleMetrics,
    query: DynamicTreeChainCycleQueryMetrics,
) -> Result<(), DynamicMinRatioCycleError> {
    metrics.cycle_queries = increment(metrics.cycle_queries)?;
    metrics.intermediate_edge_inspections = metrics
        .intermediate_edge_inspections
        .checked_add(query.intermediate_edge_inspections)
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?;
    metrics.terminal_edge_inspections = metrics
        .terminal_edge_inspections
        .checked_add(query.terminal_edge_inspections)
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?;
    Ok(())
}

fn audit_add_query_metrics(
    metrics: &mut DynamicMinRatioCycleMetrics,
    query: DynamicTreeChainCycleQueryMetrics,
) -> Result<(), DynamicMinRatioCycleError> {
    add_query_metrics(metrics, query).map_err(|_| DynamicMinRatioCycleError::TraceVerification)
}

fn add_candidate_heap_metrics(
    metrics: &mut DynamicMinRatioCycleMetrics,
    heap: DynamicTreeChainCandidateHeapMetrics,
) -> Result<(), DynamicMinRatioCycleError> {
    metrics.candidate_heap_refreshes = metrics
        .candidate_heap_refreshes
        .checked_add(heap.candidate_refreshes)
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?;
    metrics.candidate_heap_pushes = metrics
        .candidate_heap_pushes
        .checked_add(heap.heap_pushes)
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?;
    metrics.candidate_heap_pops = metrics
        .candidate_heap_pops
        .checked_add(heap.heap_pops)
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?;
    metrics.candidate_heap_updates = metrics
        .candidate_heap_updates
        .checked_add(heap.heap_updates)
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)?;
    Ok(())
}

fn audit_add_candidate_heap_metrics(
    metrics: &mut DynamicMinRatioCycleMetrics,
    heap: DynamicTreeChainCandidateHeapMetrics,
) -> Result<(), DynamicMinRatioCycleError> {
    add_candidate_heap_metrics(metrics, heap)
        .map_err(|_| DynamicMinRatioCycleError::TraceVerification)
}

fn publish(
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    events: &mut Vec<DynamicMinRatioCycleTraceEvent>,
    record: bool,
    before: DynamicMinRatioCycleSnapshot,
    kind: DynamicMinRatioCycleEventKind,
) -> Result<(), DynamicMinRatioCycleError> {
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        if events.len() >= DYNAMIC_MIN_RATIO_CYCLE_MAX_TRACE_EVENTS {
            return Err(DynamicMinRatioCycleError::AdmissionLimit);
        }
        events.push(DynamicMinRatioCycleTraceEvent {
            catalog_id: CATALOG_ID,
            kind,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(())
}

fn publish_optional(
    snapshot: &mut DynamicMinRatioCycleSnapshot,
    events: &mut Vec<DynamicMinRatioCycleTraceEvent>,
    record: bool,
    before: DynamicMinRatioCycleSnapshot,
    kind: Option<DynamicMinRatioCycleEventKind>,
) -> Result<(), DynamicMinRatioCycleError> {
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        let kind = kind.ok_or(DynamicMinRatioCycleError::InvalidInput)?;
        if events.len() >= DYNAMIC_MIN_RATIO_CYCLE_MAX_TRACE_EVENTS {
            return Err(DynamicMinRatioCycleError::AdmissionLimit);
        }
        events.push(DynamicMinRatioCycleTraceEvent {
            catalog_id: CATALOG_ID,
            kind,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(())
}

fn audit_event<'a>(
    trace: &'a DynamicMinRatioCycleTraceResult,
    snapshot: &DynamicMinRatioCycleSnapshot,
    index: usize,
) -> Result<&'a DynamicMinRatioCycleTraceEvent, DynamicMinRatioCycleError> {
    let event = trace
        .events
        .get(index)
        .ok_or(DynamicMinRatioCycleError::TraceVerification)?;
    if event.catalog_id != CATALOG_ID || event.before != *snapshot {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    Ok(event)
}

fn audit_match(
    event: &DynamicMinRatioCycleTraceEvent,
    kind: &DynamicMinRatioCycleEventKind,
    before: &DynamicMinRatioCycleSnapshot,
    after: &DynamicMinRatioCycleSnapshot,
) -> Result<(), DynamicMinRatioCycleError> {
    if event.catalog_id != CATALOG_ID
        || event.kind != *kind
        || event.before != *before
        || event.after != *after
    {
        return Err(DynamicMinRatioCycleError::TraceVerification);
    }
    Ok(())
}

fn rational_too_wide(value: &BigRational) -> bool {
    value.numer().bits() > DYNAMIC_MIN_RATIO_CYCLE_MAX_RATIONAL_BITS
        || value.denom().bits() > DYNAMIC_MIN_RATIO_CYCLE_MAX_RATIONAL_BITS
}

fn increment(value: u64) -> Result<u64, DynamicMinRatioCycleError> {
    value
        .checked_add(1)
        .ok_or(DynamicMinRatioCycleError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, DynamicMinRatioCycleError> {
    value
        .checked_add(1)
        .ok_or(DynamicMinRatioCycleError::TraceVerification)
}

fn input_failure(audit: bool) -> DynamicMinRatioCycleError {
    if audit {
        DynamicMinRatioCycleError::TraceVerification
    } else {
        DynamicMinRatioCycleError::InvalidInput
    }
}

fn admission_failure(audit: bool) -> DynamicMinRatioCycleError {
    if audit {
        DynamicMinRatioCycleError::TraceVerification
    } else {
        DynamicMinRatioCycleError::AdmissionLimit
    }
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;

    use super::*;
    use crate::algorithms::{
        DynamicCoreEncodedSide, DynamicCoreGraphStageEdge, DynamicCoreIncidence,
        DynamicCoreIncidenceEndpoint,
    };

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn ratio(numerator: i64, denominator: i64) -> BigRational {
        BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
    }

    fn edge(
        source_edge: usize,
        from: usize,
        to: usize,
        length: i64,
        gradient: i64,
    ) -> ShiftedTreeChainEdge {
        ShiftedTreeChainEdge {
            source_edge,
            from,
            to,
            length: rational(length),
            gradient: rational(gradient),
        }
    }

    fn root_graph() -> ShiftedTreeChainGraph {
        ShiftedTreeChainGraph {
            node_count: 4,
            edges: vec![
                edge(0, 0, 1, 1, 2),
                edge(2, 1, 2, 1, 3),
                edge(3, 2, 3, 1, 5),
                edge(4, 0, 3, 2, 7),
            ],
        }
    }

    fn bridge_config() -> DynamicMwuCollectionBridgeConfig {
        DynamicMwuCollectionBridgeConfig {
            branches: 2,
            maximum_node_count: 5,
            stable_edge_slots: 5,
        }
    }

    fn runtime() -> DynamicTreeChainEpochRuntimeState {
        initialize_dynamic_min_ratio_cycle_runtime(
            &root_graph(),
            &[bridge_config(), bridge_config()],
        )
        .expect("runtime")
    }

    fn config(rebuild_limit: u64) -> DynamicMinRatioCycleConfig {
        DynamicMinRatioCycleConfig {
            level_configs: vec![bridge_config(), bridge_config()],
            terminal_branches: 2,
            psi: 2,
            kappa_alpha: ratio(1, 1_000),
            epsilon: ratio(1, 1_000),
            rebuild_after_updates: vec![rebuild_limit; 2],
        }
    }

    fn inserted_row() -> DynamicCoreGraphStageEdge {
        DynamicCoreGraphStageEdge {
            edge: 1,
            from: 0,
            to: 2,
            length: rational(1),
            gradient: rational(-10),
        }
    }

    fn topology_operations() -> Vec<DynamicMinRatioCycleOperation> {
        let inserted = inserted_row();
        let moved = DynamicCoreGraphStageEdge {
            from: 4,
            ..inserted.clone()
        };
        vec![
            DynamicMinRatioCycleOperation::Update {
                updates: vec![DynamicCoreGraphStageUpdate::Insert {
                    edge: inserted.clone(),
                }],
                eta: rational(1),
            },
            DynamicMinRatioCycleOperation::Update {
                updates: vec![DynamicCoreGraphStageUpdate::VertexSplit {
                    retained_vertex: 0,
                    new_vertex: 4,
                    new_side_incidences: vec![DynamicCoreIncidence {
                        edge: 1,
                        endpoint: DynamicCoreIncidenceEndpoint::Tail,
                    }],
                    encoded_side: DynamicCoreEncodedSide::New,
                    encoded_incidences: vec![DynamicCoreIncidence {
                        edge: 1,
                        endpoint: DynamicCoreIncidenceEndpoint::Tail,
                    }],
                }],
                eta: rational(1),
            },
            DynamicMinRatioCycleOperation::Update {
                updates: vec![DynamicCoreGraphStageUpdate::Delete { edge: moved }],
                eta: rational(1),
            },
            DynamicMinRatioCycleOperation::Query { edge: 1 },
            DynamicMinRatioCycleOperation::Detect,
            DynamicMinRatioCycleOperation::Detect,
        ]
    }

    #[test]
    fn insert_split_delete_preserves_stable_flow_and_detect_semantics() {
        let state = runtime();
        let operations = topology_operations();
        let trace =
            trace_dynamic_min_ratio_cycle(&state, &config(1_000), &operations).expect("trace");
        assert_eq!(trace.result.final_snapshot.stage, 3);
        assert_eq!(trace.result.final_snapshot.metrics.updates, 3);
        assert_eq!(trace.result.final_snapshot.runtime.metrics.root_stages, 3);
        assert!(trace.result.final_snapshot.flow[1] != BigRational::zero());
        let current =
            materialize_dynamic_tree_chain_epoch_runtime(&trace.result.final_snapshot.runtime)
                .expect("current");
        assert!(
            current.epoch_snapshot.levels[0].source_graph.edge_slots[1].is_none(),
            "deleted stable slot remains a hole"
        );
        assert!(matches!(
            &trace.result.responses[0],
            DynamicMinRatioCycleResponse::Query { edge: 1, .. }
        ));
        assert!(matches!(
            (&trace.result.responses[1], &trace.result.responses[2]),
            (
                DynamicMinRatioCycleResponse::Detect { .. },
                DynamicMinRatioCycleResponse::Detect { edges, .. }
            ) if edges.is_empty()
        ));
        check_dynamic_min_ratio_cycle_trace(&state, &config(1_000), &operations, &trace)
            .expect("check");
    }

    #[test]
    fn strict_zero_limit_rebuilds_smallest_suffix_before_query() {
        let state = runtime();
        let operations = vec![DynamicMinRatioCycleOperation::Update {
            updates: vec![DynamicCoreGraphStageUpdate::Insert {
                edge: inserted_row(),
            }],
            eta: rational(1),
        }];
        let trace = trace_dynamic_min_ratio_cycle(&state, &config(0), &operations).expect("trace");
        assert!(matches!(
            trace.events[1].kind,
            DynamicMinRatioCycleEventKind::PeriodicRebuilt {
                level: 0,
                strict_limit: 0,
                ..
            }
        ));
        assert_eq!(trace.result.final_snapshot.metrics.rebuilds, 1);
        assert_eq!(
            trace.result.final_snapshot.updates_since_rebuild,
            vec![0, 0]
        );
    }

    #[test]
    fn source_progress_update_uses_the_accepted_ratio_squared_over_fifty() {
        let state = runtime();
        let config = config(1_000);
        let operations = vec![DynamicMinRatioCycleOperation::SourceProgressUpdate {
            updates: vec![DynamicCoreGraphStageUpdate::Insert {
                edge: inserted_row(),
            }],
        }];
        let trace =
            trace_dynamic_min_ratio_cycle(&state, &config, &operations).expect("source step");
        let candidate = trace
            .events
            .iter()
            .find_map(|event| match &event.kind {
                DynamicMinRatioCycleEventKind::CycleQueried {
                    query_trace,
                    accepted: true,
                    ..
                } => query_trace.result.best_candidate.as_ref(),
                _ => None,
            })
            .expect("accepted candidate");
        let application = trace
            .events
            .iter()
            .find_map(|event| match &event.kind {
                DynamicMinRatioCycleEventKind::FlowApplied { application } => {
                    Some(application.as_ref())
                }
                _ => None,
            })
            .expect("flow application");
        let expected = &candidate.ratio * &candidate.ratio
            / BigRational::from_integer(BigInt::from(
                DYNAMIC_MIN_RATIO_CYCLE_SOURCE_STEP_DENOMINATOR,
            ));
        assert_eq!(application.target_progress, expected);
        assert_eq!(
            &application.beta * &application.gradient_dot,
            application.target_progress
        );
        assert_eq!(
            execute_dynamic_min_ratio_cycle(&state, &config, &operations).expect("fast"),
            trace.result
        );

        let mut forged = trace;
        let DynamicMinRatioCycleEventKind::FlowApplied { application } = &mut forged
            .events
            .iter_mut()
            .find(|event| {
                matches!(
                    event.kind,
                    DynamicMinRatioCycleEventKind::FlowApplied { .. }
                )
            })
            .expect("flow event")
            .kind
        else {
            panic!("flow event");
        };
        application.target_progress += rational(1);
        assert_eq!(
            check_dynamic_min_ratio_cycle_trace(&state, &config, &operations, &forged),
            Err(DynamicMinRatioCycleError::TraceVerification)
        );
    }

    #[test]
    fn source_progress_without_attribute_changes_skips_the_topology_stage() {
        let state = runtime();
        let config = config(1_000);
        let operations = vec![DynamicMinRatioCycleOperation::SourceProgressUpdate {
            updates: Vec::new(),
        }];
        let trace =
            trace_dynamic_min_ratio_cycle(&state, &config, &operations).expect("source query");
        assert!(matches!(
            trace.events.first().map(|event| &event.kind),
            Some(DynamicMinRatioCycleEventKind::CycleQueried { .. })
        ));
        assert!(!trace.events.iter().any(|event| matches!(
            event.kind,
            DynamicMinRatioCycleEventKind::TopologyStageApplied { .. }
        )));
        assert_eq!(trace.result.final_snapshot.stage, 0);
        assert_eq!(trace.result.final_snapshot.metrics.updates, 0);
        assert!(
            trace
                .result
                .final_snapshot
                .flow
                .iter()
                .any(|flow| !flow.is_zero())
        );
        check_dynamic_min_ratio_cycle_trace(&state, &config, &operations, &trace).expect("check");
    }

    #[test]
    fn unchanged_source_progress_reuses_the_maintained_candidate_heap() {
        let state = runtime();
        let config = config(1_000);
        let operations = vec![
            DynamicMinRatioCycleOperation::SourceProgressUpdate {
                updates: Vec::new(),
            },
            DynamicMinRatioCycleOperation::SourceProgressUpdate {
                updates: Vec::new(),
            },
        ];
        let trace =
            trace_dynamic_min_ratio_cycle(&state, &config, &operations).expect("source queries");
        let heaps = trace
            .events
            .iter()
            .filter_map(|event| match &event.kind {
                DynamicMinRatioCycleEventKind::CycleQueried {
                    candidate_heap_trace,
                    ..
                } => Some(candidate_heap_trace.as_ref()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(heaps.len(), 2);
        assert!(!heaps[0].transitions.is_empty());
        assert!(heaps[1].transitions.is_empty());
        assert_eq!(heaps[1].before, heaps[0].after);
        assert_eq!(heaps[1].after, heaps[0].after);
        assert_eq!(
            trace.result.final_snapshot.metrics.candidate_heap_updates,
            0
        );
        assert_eq!(
            trace.result.final_snapshot.metrics.candidate_heap_pushes,
            heaps[0].metrics.heap_pushes
        );
        check_dynamic_min_ratio_cycle_trace(&state, &config, &operations, &trace).expect("check");
    }

    #[test]
    fn checker_rejects_runtime_candidate_flow_and_detection_tampering() {
        let state = runtime();
        let operations = topology_operations();
        let source =
            trace_dynamic_min_ratio_cycle(&state, &config(1_000), &operations).expect("trace");

        let mut runtime = source.clone();
        let DynamicMinRatioCycleEventKind::TopologyStageApplied { runtime_trace, .. } =
            &mut runtime.events[0].kind
        else {
            panic!("topology event");
        };
        runtime_trace.result.final_state.metrics.root_stages += 1;
        assert!(
            check_dynamic_min_ratio_cycle_trace(&state, &config(1_000), &operations, &runtime)
                .is_err()
        );

        let mut candidate = source.clone();
        let query = candidate
            .events
            .iter_mut()
            .find_map(|event| match &mut event.kind {
                DynamicMinRatioCycleEventKind::CycleQueried { query_trace, .. } => {
                    Some(query_trace)
                }
                _ => None,
            })
            .expect("query");
        query
            .result
            .best_candidate
            .as_mut()
            .expect("candidate")
            .coefficients[0] += 1;
        assert!(
            check_dynamic_min_ratio_cycle_trace(&state, &config(1_000), &operations, &candidate)
                .is_err()
        );

        let mut heap = source.clone();
        let candidate_heap = heap
            .events
            .iter_mut()
            .find_map(|event| match &mut event.kind {
                DynamicMinRatioCycleEventKind::CycleQueried {
                    candidate_heap_trace,
                    ..
                } => Some(candidate_heap_trace),
                _ => None,
            })
            .expect("candidate heap");
        candidate_heap.metrics.heap_pushes += 1;
        assert_eq!(
            check_dynamic_min_ratio_cycle_trace(&state, &config(1_000), &operations, &heap),
            Err(DynamicMinRatioCycleError::TraceVerification)
        );

        let mut flow = source.clone();
        let flow_event = flow
            .events
            .iter_mut()
            .find(|event| {
                matches!(
                    event.kind,
                    DynamicMinRatioCycleEventKind::FlowApplied { .. }
                )
            })
            .expect("flow");
        flow_event.after.flow[0] += rational(1);
        assert_eq!(
            check_dynamic_min_ratio_cycle_trace(&state, &config(1_000), &operations, &flow),
            Err(DynamicMinRatioCycleError::TraceVerification)
        );

        let mut detect = source;
        let detect_event = detect
            .events
            .iter_mut()
            .find(|event| {
                matches!(
                    event.kind,
                    DynamicMinRatioCycleEventKind::DetectReturned { .. }
                )
            })
            .expect("detect");
        detect_event.after.metrics.detected_edges += 1;
        assert_eq!(
            check_dynamic_min_ratio_cycle_trace(&state, &config(1_000), &operations, &detect),
            Err(DynamicMinRatioCycleError::TraceVerification)
        );
    }

    #[test]
    fn checker_rejects_link_cut_certificate_tampering() {
        let state = runtime();
        let operations = topology_operations();
        let mut trace =
            trace_dynamic_min_ratio_cycle(&state, &config(1_000), &operations).expect("trace");
        let certificate = trace
            .events
            .iter_mut()
            .find_map(|event| match &mut event.kind {
                DynamicMinRatioCycleEventKind::FlowApplied { application } => {
                    Some(&mut application.link_cut_certificate)
                }
                _ => None,
            })
            .expect("link-cut certificate");
        certificate.metrics.movement_path_adds += 1;
        assert_eq!(
            check_dynamic_min_ratio_cycle_trace(&state, &config(1_000), &operations, &trace),
            Err(DynamicMinRatioCycleError::TraceVerification)
        );
    }

    #[test]
    fn resumable_fast_and_trace_sessions_are_batch_identical() {
        let state = runtime();
        let config = config(1_000);
        let operations = topology_operations();
        let expected_fast =
            execute_dynamic_min_ratio_cycle(&state, &config, &operations).expect("batch fast");
        let expected_trace =
            trace_dynamic_min_ratio_cycle(&state, &config, &operations).expect("batch trace");

        let mut fast = DynamicMinRatioCycleSession::new(&state, &config).expect("fast session");
        for operation in operations.clone() {
            fast.apply(operation).expect("fast operation");
            assert!(!fast.snapshot().complete);
        }
        assert_eq!(fast.finish().expect("fast finish"), expected_fast);

        let mut traced =
            DynamicMinRatioCycleTraceSession::new(&state, &config).expect("trace session");
        for operation in operations {
            traced.apply(operation).expect("trace operation");
            assert!(!traced.snapshot().complete);
        }
        assert_eq!(traced.finish().expect("trace finish"), expected_trace);
    }

    #[test]
    fn adaptive_session_preserves_failed_operation_and_reconciles_query_work() {
        let state = runtime();
        let config = config(1_000);
        let inserted = inserted_row();
        let insert = DynamicMinRatioCycleOperation::Update {
            updates: vec![DynamicCoreGraphStageUpdate::Insert {
                edge: inserted.clone(),
            }],
            eta: rational(1),
        };
        let mut session = DynamicMinRatioCycleTraceSession::new(&state, &config).expect("session");
        assert_eq!(session.apply(insert.clone()).expect("insert"), None);
        let first_flow = session.snapshot().flow[1].clone();
        assert!(!first_flow.is_zero());

        let before_failure = session.snapshot().clone();
        assert_eq!(
            session.apply(DynamicMinRatioCycleOperation::Query { edge: 5 }),
            Err(DynamicMinRatioCycleError::InvalidInput)
        );
        assert_eq!(session.snapshot(), &before_failure);

        let adaptive_gradient = if first_flow.is_positive() { -11 } else { -9 };
        let replaced = DynamicCoreGraphStageEdge {
            gradient: rational(adaptive_gradient),
            ..inserted.clone()
        };
        let replace = DynamicMinRatioCycleOperation::Update {
            updates: vec![DynamicCoreGraphStageUpdate::ReplaceAttributes {
                before: inserted,
                after: replaced,
            }],
            eta: rational(1),
        };
        let query = DynamicMinRatioCycleOperation::Query { edge: 1 };
        let detect = DynamicMinRatioCycleOperation::Detect;
        session.apply(replace.clone()).expect("adaptive replace");
        let response = session
            .apply(query.clone())
            .expect("query")
            .expect("query response");
        assert!(matches!(
            response,
            DynamicMinRatioCycleResponse::Query { edge: 1, .. }
        ));
        session.apply(detect.clone()).expect("detect");
        let trace = session.finish().expect("finish");

        let operations = vec![insert, replace, query, detect];
        let batch =
            trace_dynamic_min_ratio_cycle(&state, &config, &operations).expect("adaptive batch");
        assert_eq!(trace, batch, "the failed operation was not committed");

        let mut intermediate = 0_u64;
        let mut terminal = 0_u64;
        for event in &trace.events {
            if let DynamicMinRatioCycleEventKind::CycleQueried { query_trace, .. } = &event.kind {
                intermediate += query_trace
                    .result
                    .final_snapshot
                    .metrics
                    .intermediate_edge_inspections;
                terminal += query_trace
                    .result
                    .final_snapshot
                    .metrics
                    .terminal_edge_inspections;
            }
        }
        assert_eq!(
            trace
                .result
                .final_snapshot
                .metrics
                .intermediate_edge_inspections,
            intermediate
        );
        assert_eq!(
            trace
                .result
                .final_snapshot
                .metrics
                .terminal_edge_inspections,
            terminal
        );
    }

    #[test]
    fn invalid_stage_is_atomic_and_stable_query_bounds_are_closed() {
        let state = runtime();
        let invalid = vec![DynamicMinRatioCycleOperation::Update {
            updates: Vec::new(),
            eta: rational(1),
        }];
        assert_eq!(
            execute_dynamic_min_ratio_cycle(&state, &config(1_000), &invalid),
            Err(DynamicMinRatioCycleError::InvalidInput)
        );
        assert_eq!(
            execute_dynamic_min_ratio_cycle(
                &state,
                &config(1_000),
                &[DynamicMinRatioCycleOperation::Query { edge: 5 }],
            ),
            Err(DynamicMinRatioCycleError::InvalidInput)
        );
        assert_eq!(
            state.metrics,
            crate::DynamicTreeChainEpochRuntimeMetrics::default()
        );
    }
}
