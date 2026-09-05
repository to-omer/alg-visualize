//! Forest-reusing capacity updates for Excesses IBFS.

use std::collections::{BTreeSet, VecDeque};

use thiserror::Error;

use super::{
    EibfsEngine, EibfsError, EibfsMetrics, EibfsNode, ForestSide, ForestState, PhaseDirection,
    ScanKind,
};
use crate::algorithms::dynamic_eibfs::{
    DynamicCapacityUpdate, DynamicEibfsError, materialize_current_graph, prepare_dynamic_eibfs,
};
use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{EdgeId, FlowEdge, FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    DynamicEibfsTraceOverlay, DynamicEibfsTraceStage, DynamicEibfsTraceViolation, FlowTraceEvent,
    FlowTraceEventMetadata, FlowTraceMetrics, FlowTraceRecorder, FlowTraceSnapshot,
};

/// Exact counters owned by the dynamic capacity-update layer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicEibfsMetrics {
    /// Sequential updates applied, including no-ops.
    pub updates: u64,
    /// Strict current-capacity increases.
    pub capacity_increases: u64,
    /// Strict current-capacity decreases.
    pub capacity_decreases: u64,
    /// Updates whose value already matched the current capacity.
    pub no_op_updates: u64,
    /// Reverse pushes that repaired `flow > current capacity`.
    pub over_capacity_repairs: u64,
    /// Total units removed by over-capacity repair.
    pub over_capacity_units: u128,
    /// Parent relations invalidated by a disappearing residual arc.
    pub invalidated_parent_arcs: u64,
    /// Correct-sign nonroots promoted to excess/deficit roots.
    pub promoted_roots: u64,
    /// Type-(2) newly residual source-to-sink forest arcs saturated.
    pub bridge_violations: u64,
    /// Type-(3) same-forest label violations saturated.
    pub label_violations: u64,
    /// Type-(4) current-arc cursors rewound.
    pub current_arc_violations: u64,
    /// Type-(5) forest-boundary residual arcs saturated.
    pub boundary_violations: u64,
    /// Forest vertices retained byte-for-byte across update repair.
    pub exactly_reused_forest_nodes: u64,
    /// Invariant-repair outer iterations.
    pub repair_iterations: u64,
    /// Same-cut cancellations performed only on certification clones.
    pub certification_recoveries: u64,
    /// Units cancelled only on certification clones.
    pub certification_recovered_units: u128,
}

/// One independently certified update prefix, including prefix zero.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicEibfsPrefixResult {
    /// Zero is the initial graph; update `i` produces prefix `i`.
    pub update_index: usize,
    /// Changed edge for nonzero prefixes.
    pub changed_edge: Option<EdgeId>,
    /// Capacity immediately before the update that created this prefix.
    pub old_capacity: Option<u64>,
    /// Capacity installed by the update that created this prefix.
    pub new_capacity: Option<u64>,
    /// Current capacities in canonical edge-ID order.
    pub capacities: Vec<u64>,
    /// Recovered feasible maximum flow in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Solver-independent maximum-flow/minimum-cut witness.
    pub certificate: MaxFlowCertificate,
}

/// Certified result for the initial graph and every sequential update prefix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicEibfsResult {
    /// Prefix zero followed by one result per update.
    pub prefixes: Vec<DynamicEibfsPrefixResult>,
    /// Cumulative forest/search counters; certification clones are excluded.
    pub eibfs_metrics: EibfsMetrics,
    /// Capacity-update and certification counters.
    pub dynamic_metrics: DynamicEibfsMetrics,
}

/// Certified Dynamic EIBFS prefixes plus a complete reversible update trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicEibfsTraceResult {
    /// Same certified prefixes produced by the non-tracing profile.
    pub result: DynamicEibfsResult,
    /// Replay boundary before initial forest initialization.
    pub base_snapshot: FlowTraceSnapshot,
    /// Reversible initial-solve, update-repair, and prefix-certification events.
    pub events: Vec<FlowTraceEvent>,
    /// Final independently certified feasible prefix boundary.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Dynamic EIBFS preparation, repair, or certification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicEibfsSolveError {
    /// Capacity-update input is outside the source-scoped contract.
    #[error(transparent)]
    Input(#[from] DynamicEibfsError),
    /// The reused EIBFS kernel rejected a forest or work invariant.
    #[error(transparent)]
    Kernel(#[from] EibfsError),
    /// Independent prefix certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// A cumulative exact counter overflowed.
    #[error("Dynamic EIBFS metric arithmetic overflow")]
    ArithmeticOverflow,
}

/// Solves the initial graph and a sequence of capacity updates by reusing the
/// EIBFS pseudoflow, forests, labels, and current arcs.
///
/// # Errors
///
/// Rejects input outside [`prepare_dynamic_eibfs`], a bounded repair/search
/// ceiling, any forest invariant failure, or an independently invalid prefix.
pub fn solve_dynamic_eibfs(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    updates: &[DynamicCapacityUpdate],
) -> Result<DynamicEibfsResult, DynamicEibfsSolveError> {
    solve_dynamic_eibfs_internal(graph, source, sink, updates, false).map(|run| run.result)
}

/// Solves Dynamic EIBFS while recording update repairs, certification clones,
/// and explicit reusable-pseudoflow restoration.
///
/// # Errors
///
/// Returns the same failures as [`solve_dynamic_eibfs`], plus reversible trace
/// projection or validation failures.
pub fn trace_dynamic_eibfs(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    updates: &[DynamicCapacityUpdate],
) -> Result<DynamicEibfsTraceResult, DynamicEibfsSolveError> {
    let run = solve_dynamic_eibfs_internal(graph, source, sink, updates, true)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(EibfsError::ForestInvariant)?;
    Ok(DynamicEibfsTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct DynamicEibfsInternalRun {
    result: DynamicEibfsResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

#[derive(Clone)]
struct DynamicTraceCursor {
    update_index: usize,
    update_total: usize,
    changed_edge: Option<EdgeId>,
    old_capacity: Option<u64>,
    new_capacity: Option<u64>,
}

fn solve_dynamic_eibfs_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    updates: &[DynamicCapacityUpdate],
    with_trace: bool,
) -> Result<DynamicEibfsInternalRun, DynamicEibfsSolveError> {
    let problem = prepare_dynamic_eibfs(graph, source, sink, updates)?;
    let envelope = problem.envelope();
    let (mut engine, mut recorder, initial_cursor) = initialize_dynamic_eibfs(
        envelope,
        problem.initial_capacities(),
        source,
        sink,
        problem.updates().len(),
        with_trace,
    )?;
    let mut dynamic_metrics = DynamicEibfsMetrics::default();
    let mut prefixes = vec![certify_prefix(
        &engine,
        &initial_cursor,
        &mut dynamic_metrics,
        &mut recorder,
    )?];
    resume_warm_state(
        &mut engine,
        &initial_cursor,
        &dynamic_metrics,
        &mut recorder,
    )?;
    solve_update_prefixes(
        &mut engine,
        problem.updates(),
        &mut dynamic_metrics,
        &mut recorder,
        &mut prefixes,
    )?;

    Ok(DynamicEibfsInternalRun {
        result: DynamicEibfsResult {
            prefixes,
            eibfs_metrics: engine.metrics,
            dynamic_metrics,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn initialize_dynamic_eibfs<'graph>(
    envelope: &'graph FlowNetwork,
    initial_capacities: &[u64],
    source: NodeIndex,
    sink: NodeIndex,
    update_total: usize,
    with_trace: bool,
) -> Result<
    (
        EibfsEngine<'graph>,
        Option<FlowTraceRecorder<'graph>>,
        DynamicTraceCursor,
    ),
    DynamicEibfsSolveError,
> {
    let zero_flow = envelope
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    let residual =
        ResidualState::from_current_capacities_and_flows(envelope, initial_capacities, &zero_flow)
            .map_err(EibfsError::from)?;
    let mut recorder = if with_trace {
        let base = FlowTraceSnapshot::capture(
            envelope,
            &residual,
            vec![None; envelope.nodes().len()],
            Vec::new(),
            Vec::new(),
            vec![0; envelope.nodes().len()],
            FlowTraceMetrics::default(),
        );
        Some(FlowTraceRecorder::new(envelope, base).map_err(EibfsError::from)?)
    } else {
        None
    };
    let mut engine = EibfsEngine::new(envelope, source, sink, residual)?;
    let initial_cursor = DynamicTraceCursor {
        update_index: 0,
        update_total,
        changed_edge: None,
        old_capacity: None,
        new_capacity: None,
    };
    let dynamic_metrics = DynamicEibfsMetrics::default();
    engine.dynamic_overlay = Some(dynamic_trace_overlay(
        &initial_cursor,
        DynamicEibfsTraceStage::InitialSolve,
        None,
        None,
        &dynamic_metrics,
        &engine.metrics,
    ));
    engine.record(
        recorder.as_mut(),
        FlowTraceEventMetadata {
            catalog_id: "dynamic-eibfs.initialize-pseudoflow-forests",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "dynamic-eibfs:initialize-reusable-s-t-forests",
        },
        vec![source, sink],
        Vec::new(),
        Some(("forest-roots", 2)),
    )?;
    engine.run(&mut recorder)?;
    Ok((engine, recorder, initial_cursor))
}

fn solve_update_prefixes<'graph>(
    engine: &mut EibfsEngine<'graph>,
    updates: &[DynamicCapacityUpdate],
    dynamic_metrics: &mut DynamicEibfsMetrics,
    recorder: &mut Option<FlowTraceRecorder<'graph>>,
    prefixes: &mut Vec<DynamicEibfsPrefixResult>,
) -> Result<(), DynamicEibfsSolveError> {
    for (ordinal, update) in updates.iter().enumerate() {
        let edge_index = engine
            .graph
            .edge_index(&update.edge)
            .ok_or(DynamicEibfsError::MissingEdge)?;
        let old_capacity = engine.residual.capacities()[edge_index.as_usize()];
        let cursor = DynamicTraceCursor {
            update_index: ordinal + 1,
            update_total: updates.len(),
            changed_edge: Some(update.edge.clone()),
            old_capacity: Some(old_capacity),
            new_capacity: Some(update.capacity),
        };
        apply_update(engine, update, &cursor, dynamic_metrics, recorder)?;
        prefixes.push(certify_prefix(engine, &cursor, dynamic_metrics, recorder)?);
        if ordinal + 1 < updates.len() {
            resume_warm_state(engine, &cursor, dynamic_metrics, recorder)?;
        }
    }
    Ok(())
}

fn certify_prefix(
    engine: &EibfsEngine<'_>,
    cursor: &DynamicTraceCursor,
    metrics: &mut DynamicEibfsMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<DynamicEibfsPrefixResult, DynamicEibfsSolveError> {
    engine.validate_stable_forest()?;
    let cut = terminal_cut(engine);
    let mut certified = engine.clone();
    certified.dynamic_overlay = Some(dynamic_trace_overlay(
        cursor,
        DynamicEibfsTraceStage::PrefixRecovery,
        None,
        None,
        metrics,
        &engine.metrics,
    ));
    certified.begin_recovery(&cut, recorder.as_mut())?;
    certified.recover_feasible_flow(&cut, recorder.as_mut())?;
    metrics.certification_recoveries = add_u64(
        metrics.certification_recoveries,
        certified
            .metrics
            .recovery_cancellations
            .checked_sub(engine.metrics.recovery_cancellations)
            .ok_or(DynamicEibfsSolveError::ArithmeticOverflow)?,
    )?;
    metrics.certification_recovered_units = add_u128(
        metrics.certification_recovered_units,
        certified
            .metrics
            .recovered_units
            .checked_sub(engine.metrics.recovered_units)
            .ok_or(DynamicEibfsSolveError::ArithmeticOverflow)?,
    )?;

    let capacities = engine.residual.capacities().to_vec();
    let current_graph = materialize_current_graph(engine.graph, &capacities)?;
    let flows = certified.residual.flows().to_vec();
    let certificate = check_max_flow(&current_graph, engine.source, engine.sink, &flows)?;
    certified.dynamic_overlay = Some(dynamic_trace_overlay(
        cursor,
        DynamicEibfsTraceStage::PrefixCertified,
        None,
        Some(certificate.value),
        metrics,
        &engine.metrics,
    ));
    let cut_order = certificate
        .source_side
        .iter()
        .filter_map(|id| engine.graph.node_index(id))
        .collect::<Vec<_>>();
    certified.record(
        recorder.as_mut(),
        FlowTraceEventMetadata {
            catalog_id: "dynamic-eibfs.prefix-certified",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "dynamic-eibfs:certify-current-capacity-prefix-clone",
        },
        cut_order,
        Vec::new(),
        Some(("value", certificate.value)),
    )?;
    Ok(DynamicEibfsPrefixResult {
        update_index: cursor.update_index,
        changed_edge: cursor.changed_edge.clone(),
        old_capacity: cursor.old_capacity,
        new_capacity: cursor.new_capacity,
        capacities,
        flows,
        certificate,
    })
}

fn resume_warm_state(
    engine: &mut EibfsEngine<'_>,
    cursor: &DynamicTraceCursor,
    metrics: &DynamicEibfsMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), DynamicEibfsSolveError> {
    engine.dynamic_overlay = Some(dynamic_trace_overlay(
        cursor,
        DynamicEibfsTraceStage::ResumeReusablePseudoflow,
        None,
        None,
        metrics,
        &engine.metrics,
    ));
    engine.record(
        recorder.as_mut(),
        FlowTraceEventMetadata {
            catalog_id: "dynamic-eibfs.resume-reusable-pseudoflow",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "dynamic-eibfs:restore-warm-pseudoflow-after-clone-certificate",
        },
        Vec::new(),
        Vec::new(),
        Some((
            "prefix",
            i128::try_from(cursor.update_index)
                .map_err(|_| DynamicEibfsSolveError::ArithmeticOverflow)?,
        )),
    )?;
    Ok(())
}

fn dynamic_trace_overlay(
    cursor: &DynamicTraceCursor,
    stage: DynamicEibfsTraceStage,
    violation: Option<DynamicEibfsTraceViolation>,
    prefix_value: Option<i128>,
    metrics: &DynamicEibfsMetrics,
    eibfs_metrics: &EibfsMetrics,
) -> DynamicEibfsTraceOverlay {
    DynamicEibfsTraceOverlay {
        stage,
        update_index: cursor.update_index,
        update_total: cursor.update_total,
        changed_edge: cursor.changed_edge.clone(),
        old_capacity: cursor.old_capacity,
        new_capacity: cursor.new_capacity,
        violation,
        reused_forest_nodes: metrics.exactly_reused_forest_nodes,
        updates_applied: metrics.updates,
        capacity_increases: metrics.capacity_increases,
        capacity_decreases: metrics.capacity_decreases,
        no_op_updates: metrics.no_op_updates,
        over_capacity_repairs: metrics.over_capacity_repairs,
        invalidated_parent_arcs: metrics.invalidated_parent_arcs,
        promoted_roots: metrics.promoted_roots,
        repair_arc_scans: eibfs_metrics.dynamic_repair_arc_scans,
        state_transitions: eibfs_metrics.state_transitions,
        bridge_violations: metrics.bridge_violations,
        label_violations: metrics.label_violations,
        current_arc_violations: metrics.current_arc_violations,
        boundary_violations: metrics.boundary_violations,
        repair_iterations: metrics.repair_iterations,
        certification_recoveries: metrics.certification_recoveries,
        prefix_value,
    }
}

fn set_dynamic_stage(
    engine: &mut EibfsEngine<'_>,
    cursor: &DynamicTraceCursor,
    stage: DynamicEibfsTraceStage,
    violation: Option<DynamicEibfsTraceViolation>,
    metrics: &DynamicEibfsMetrics,
) {
    let overlay = dynamic_trace_overlay(cursor, stage, violation, None, metrics, &engine.metrics);
    engine.dynamic_overlay = Some(overlay);
}

fn record_dynamic_update(
    engine: &EibfsEngine<'_>,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
    catalog_id: &'static str,
    pseudocode_line: &'static str,
    active_path: Vec<ResidualArcId>,
    detail: u64,
) -> Result<(), DynamicEibfsSolveError> {
    engine.record(
        recorder.as_mut(),
        FlowTraceEventMetadata {
            catalog_id,
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line,
        },
        Vec::new(),
        active_path,
        Some(("amount", i128::from(detail))),
    )?;
    Ok(())
}

impl DynamicEibfsTraceViolation {
    const fn catalog_id(self) -> &'static str {
        match self {
            Self::OverCapacity => "dynamic-eibfs.repair-over-capacity",
            Self::Bridge => "dynamic-eibfs.repair-new-bridge",
            Self::Label => "dynamic-eibfs.repair-label-violation",
            Self::CurrentArc => "dynamic-eibfs.rewind-current-arc",
            Self::Boundary => "dynamic-eibfs.repair-forest-boundary",
        }
    }

    const fn pseudocode_line(self) -> &'static str {
        match self {
            Self::OverCapacity => "dynamic-eibfs:reverse-push-capacity-overflow",
            Self::Bridge => "dynamic-eibfs:saturate-new-source-to-sink-bridge",
            Self::Label => "dynamic-eibfs:saturate-new-label-violating-arc",
            Self::CurrentArc => "dynamic-eibfs:rewind-to-new-admissible-current-arc",
            Self::Boundary => "dynamic-eibfs:saturate-new-forest-boundary-arc",
        }
    }
}

fn terminal_cut(engine: &EibfsEngine<'_>) -> Vec<bool> {
    engine
        .nodes
        .iter()
        .map(|node| match engine.direction {
            PhaseDirection::Forward => node.state.belongs_to(ForestSide::Source),
            PhaseDirection::Reverse => !node.state.belongs_to(ForestSide::Sink),
        })
        .collect()
}

fn apply_update<'graph>(
    engine: &mut EibfsEngine<'graph>,
    update: &DynamicCapacityUpdate,
    cursor: &DynamicTraceCursor,
    metrics: &mut DynamicEibfsMetrics,
    recorder: &mut Option<FlowTraceRecorder<'graph>>,
) -> Result<(), DynamicEibfsSolveError> {
    let Some(installed) = install_capacity_update(engine, update, cursor, metrics, recorder)?
    else {
        return Ok(());
    };
    repair_capacity_and_forest(
        engine,
        update,
        cursor,
        installed.over_capacity,
        metrics,
        recorder,
    )?;
    restore_new_residual_violations(engine, &installed.newly_residual, cursor, metrics, recorder)?;
    metrics.exactly_reused_forest_nodes = add_u64(
        metrics.exactly_reused_forest_nodes,
        reused_forest_nodes(&installed.forest_before, &engine.nodes)?,
    )?;
    continue_reused_search(engine, cursor, metrics, recorder)
}

struct InstalledCapacityUpdate {
    forest_before: Vec<EibfsNode>,
    newly_residual: Vec<ResidualArcId>,
    over_capacity: u64,
}

fn install_capacity_update(
    engine: &mut EibfsEngine<'_>,
    update: &DynamicCapacityUpdate,
    cursor: &DynamicTraceCursor,
    metrics: &mut DynamicEibfsMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<Option<InstalledCapacityUpdate>, DynamicEibfsSolveError> {
    metrics.updates = add_u64(metrics.updates, 1)?;
    let edge_index = engine
        .graph
        .edge_index(&update.edge)
        .ok_or(DynamicEibfsError::MissingEdge)?;
    let old_capacity = *engine
        .residual
        .capacities()
        .get(edge_index.as_usize())
        .ok_or(DynamicEibfsError::MissingEdge)?;
    match update.capacity.cmp(&old_capacity) {
        std::cmp::Ordering::Greater => {
            metrics.capacity_increases = add_u64(metrics.capacity_increases, 1)?;
        }
        std::cmp::Ordering::Less => {
            metrics.capacity_decreases = add_u64(metrics.capacity_decreases, 1)?;
        }
        std::cmp::Ordering::Equal => {
            metrics.no_op_updates = add_u64(metrics.no_op_updates, 1)?;
            set_dynamic_stage(
                engine,
                cursor,
                DynamicEibfsTraceStage::ApplyUpdate,
                None,
                metrics,
            );
            record_dynamic_update(
                engine,
                recorder,
                "dynamic-eibfs.apply-no-op-capacity-update",
                "dynamic-eibfs:retain-identical-current-capacity",
                Vec::new(),
                update.capacity,
            )?;
            return Ok(None);
        }
    }

    let forest_before = engine.nodes.clone();
    engine.begin_work_epoch();
    let changed_arc_ids = [ResidualDirection::Forward, ResidualDirection::Reverse]
        .map(|direction| ResidualArcId::new(update.edge.clone(), direction));
    let residual_before = changed_arc_ids
        .iter()
        .map(|id| {
            engine
                .residual
                .arc(id)
                .map(|arc| arc.capacity)
                .ok_or(EibfsError::ForestInvariant)
        })
        .collect::<Result<Vec<_>, _>>()?;
    engine
        .residual
        .set_current_capacity(&update.edge, update.capacity)
        .map_err(EibfsError::from)?;
    set_dynamic_stage(
        engine,
        cursor,
        DynamicEibfsTraceStage::ApplyUpdate,
        None,
        metrics,
    );
    record_dynamic_update(
        engine,
        recorder,
        "dynamic-eibfs.apply-capacity-update",
        "dynamic-eibfs:install-current-capacity-inside-envelope",
        changed_arc_ids.to_vec(),
        update.capacity,
    )?;
    let newly_residual = changed_arc_ids
        .into_iter()
        .zip(residual_before)
        .filter_map(|(id, before)| {
            let after = engine.residual.arc(&id)?.capacity;
            (before == 0 && after > 0).then_some(id)
        })
        .collect::<Vec<_>>();
    let over_capacity = engine
        .residual
        .capacity_violation(&update.edge)
        .ok_or(DynamicEibfsError::MissingEdge)?;
    Ok(Some(InstalledCapacityUpdate {
        forest_before,
        newly_residual,
        over_capacity,
    }))
}

fn repair_capacity_and_forest(
    engine: &mut EibfsEngine<'_>,
    update: &DynamicCapacityUpdate,
    cursor: &DynamicTraceCursor,
    over_capacity: u64,
    metrics: &mut DynamicEibfsMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), DynamicEibfsSolveError> {
    if over_capacity > 0 {
        let reverse = ResidualArcId::new(update.edge.clone(), ResidualDirection::Reverse);
        engine.push_residual(&reverse, over_capacity)?;
        engine.count_transition()?;
        metrics.over_capacity_repairs = add_u64(metrics.over_capacity_repairs, 1)?;
        metrics.over_capacity_units = add_u128(metrics.over_capacity_units, over_capacity.into())?;
    }
    stabilize_after_repair(engine, cursor, metrics, recorder)?;
    if over_capacity > 0 {
        set_dynamic_stage(
            engine,
            cursor,
            DynamicEibfsTraceStage::RepairCapacity,
            Some(DynamicEibfsTraceViolation::OverCapacity),
            metrics,
        );
        record_dynamic_update(
            engine,
            recorder,
            "dynamic-eibfs.repair-over-capacity",
            "dynamic-eibfs:reverse-push-capacity-overflow",
            vec![ResidualArcId::new(
                update.edge.clone(),
                ResidualDirection::Reverse,
            )],
            over_capacity,
        )?;
    }
    Ok(())
}

fn continue_reused_search<'graph>(
    engine: &mut EibfsEngine<'graph>,
    cursor: &DynamicTraceCursor,
    metrics: &DynamicEibfsMetrics,
    recorder: &mut Option<FlowTraceRecorder<'graph>>,
) -> Result<(), DynamicEibfsSolveError> {
    set_dynamic_stage(
        engine,
        cursor,
        DynamicEibfsTraceStage::ContinueSolve,
        None,
        metrics,
    );
    engine.record(
        recorder.as_mut(),
        FlowTraceEventMetadata {
            catalog_id: "dynamic-eibfs.continue-reused-search",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "dynamic-eibfs:continue-from-repaired-warm-forests",
        },
        Vec::new(),
        Vec::new(),
        Some((
            "reused-nodes",
            i128::from(metrics.exactly_reused_forest_nodes),
        )),
    )?;
    engine.run(recorder)?;
    Ok(())
}

fn restore_new_residual_violations(
    engine: &mut EibfsEngine<'_>,
    newly_residual: &[ResidualArcId],
    cursor: &DynamicTraceCursor,
    metrics: &mut DynamicEibfsMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), DynamicEibfsSolveError> {
    let mut violations = Vec::new();
    for id in newly_residual {
        let (capacity, from, to) = engine
            .residual
            .arc(id)
            .map(|arc| (arc.capacity, arc.from, arc.to))
            .ok_or(EibfsError::ForestInvariant)?;
        if capacity == 0 || from == to {
            continue;
        }
        engine.count_scan(ScanKind::DynamicRepair)?;
        let violation = classify_violation(engine, id.clone(), from, to)?;
        let trace_violation = violation.as_ref().map(|violation| match violation.kind {
            ViolationKind::Bridge => DynamicEibfsTraceViolation::Bridge,
            ViolationKind::Label => DynamicEibfsTraceViolation::Label,
            ViolationKind::CurrentArc => DynamicEibfsTraceViolation::CurrentArc,
            ViolationKind::Boundary => DynamicEibfsTraceViolation::Boundary,
        });
        set_dynamic_stage(
            engine,
            cursor,
            if trace_violation.is_some() {
                DynamicEibfsTraceStage::RepairViolation
            } else {
                DynamicEibfsTraceStage::RepairForest
            },
            trace_violation,
            metrics,
        );
        engine.record(
            recorder.as_mut(),
            FlowTraceEventMetadata {
                catalog_id: "dynamic-eibfs.inspect-newly-residual-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "dynamic-eibfs:classify-newly-residual-arc",
            },
            Vec::new(),
            vec![id.clone()],
            Some(("violation", i128::from(violation.is_some()))),
        )?;
        if let Some(violation) = violation {
            violations.push(violation);
        }
    }
    violations
        .sort_unstable_by(|left, right| (&left.kind, &left.arc).cmp(&(&right.kind, &right.arc)));
    for violation in violations {
        if engine
            .residual
            .arc(&violation.arc)
            .is_some_and(|arc| arc.capacity > 0)
        {
            repair_violation(engine, &violation, metrics)?;
            stabilize_after_repair(engine, cursor, metrics, recorder)?;
            let trace_violation = match violation.kind {
                ViolationKind::Bridge => DynamicEibfsTraceViolation::Bridge,
                ViolationKind::Label => DynamicEibfsTraceViolation::Label,
                ViolationKind::CurrentArc => DynamicEibfsTraceViolation::CurrentArc,
                ViolationKind::Boundary => DynamicEibfsTraceViolation::Boundary,
            };
            set_dynamic_stage(
                engine,
                cursor,
                DynamicEibfsTraceStage::RepairViolation,
                Some(trace_violation),
                metrics,
            );
            engine.record(
                recorder.as_mut(),
                FlowTraceEventMetadata {
                    catalog_id: trace_violation.catalog_id(),
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: trace_violation.pseudocode_line(),
                },
                Vec::new(),
                vec![violation.arc.clone()],
                None,
            )?;
        }
    }
    engine.validate_stable_forest()?;
    Ok(())
}

fn stabilize_after_repair(
    engine: &mut EibfsEngine<'_>,
    cursor: &DynamicTraceCursor,
    metrics: &mut DynamicEibfsMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), DynamicEibfsSolveError> {
    metrics.repair_iterations = add_u64(metrics.repair_iterations, 1)?;
    promote_correct_sign_roots(engine, metrics)?;
    let checkpoints = adopt_invalidated_parents(engine, metrics)?;
    engine.repair_bad_signs(None)?;
    promote_correct_sign_roots(engine, metrics)?;
    publish_dynamic_repair_scan_checkpoints(engine, cursor, metrics, recorder, checkpoints)
}

fn promote_correct_sign_roots(
    engine: &mut EibfsEngine<'_>,
    metrics: &mut DynamicEibfsMetrics,
) -> Result<(), DynamicEibfsSolveError> {
    let candidates = engine
        .graph
        .node_indices()
        .filter_map(|node| {
            if node == engine.source || node == engine.sink {
                return None;
            }
            let excess = engine.excess[node.as_usize()];
            let state = engine.nodes[node.as_usize()].state;
            let has_parent = engine.nodes[node.as_usize()].parent.is_some();
            let side = if excess > 0
                && (matches!(state, ForestState::Free | ForestState::SourceOrphan)
                    || state == ForestState::Source && has_parent)
            {
                Some(ForestSide::Source)
            } else if excess < 0
                && (matches!(state, ForestState::Free | ForestState::SinkOrphan)
                    || state == ForestState::Sink && has_parent)
            {
                Some(ForestSide::Sink)
            } else {
                None
            }?;
            Some((node, side))
        })
        .collect::<Vec<_>>();

    for (node, side) in candidates {
        let previous_state = engine.nodes[node.as_usize()].state;
        let old_side = previous_state.normal();
        let retain_label = old_side.or(previous_state.orphan()) == Some(side);
        if let Some(parent_side) = old_side
            && engine.nodes[node.as_usize()].parent.is_some()
        {
            engine.detach_parent(node, parent_side)?;
        }
        let state = &mut engine.nodes[node.as_usize()];
        state.state = match side {
            ForestSide::Source => ForestState::Source,
            ForestSide::Sink => ForestState::Sink,
        };
        state.parent = None;
        if !retain_label {
            let label = match (side, engine.direction) {
                (ForestSide::Source, PhaseDirection::Forward) => engine
                    .source_depth
                    .checked_add(1)
                    .ok_or(DynamicEibfsSolveError::ArithmeticOverflow)?,
                (ForestSide::Source, PhaseDirection::Reverse) => engine.source_depth,
                (ForestSide::Sink, PhaseDirection::Reverse) => engine
                    .sink_depth
                    .checked_add(1)
                    .ok_or(DynamicEibfsSolveError::ArithmeticOverflow)?,
                (ForestSide::Sink, PhaseDirection::Forward) => engine.sink_depth,
            };
            state.current_arc = 0;
            state.growth_cursor = 0;
            match side {
                ForestSide::Source => state.source_label = label,
                ForestSide::Sink => state.sink_label = label,
            }
        }
        engine.count_transition()?;
        metrics.promoted_roots = add_u64(metrics.promoted_roots, 1)?;
    }
    Ok(())
}

#[derive(Clone)]
struct DynamicRepairScanCheckpoint {
    node: NodeIndex,
    arc: ResidualArcId,
    residual_capacity: u64,
    repaired: bool,
    eibfs_metrics: EibfsMetrics,
    dynamic_metrics: DynamicEibfsMetrics,
}

fn adopt_invalidated_parents(
    engine: &mut EibfsEngine<'_>,
    metrics: &mut DynamicEibfsMetrics,
) -> Result<Vec<DynamicRepairScanCheckpoint>, DynamicEibfsSolveError> {
    let invalid = engine
        .graph
        .node_indices()
        .filter_map(|node| {
            let side = engine.nodes[node.as_usize()].state.normal()?;
            let id = engine.nodes[node.as_usize()].parent.as_ref()?;
            Some((node, side, id.clone()))
        })
        .collect::<Vec<_>>();
    let mut queue = VecDeque::new();
    let mut queued = BTreeSet::new();
    let mut checkpoints = Vec::with_capacity(invalid.len());
    for (node, side, id) in invalid {
        engine.count_scan(ScanKind::DynamicRepair)?;
        let residual_capacity = engine
            .residual
            .arc(&id)
            .ok_or(EibfsError::ForestInvariant)?
            .capacity;
        if residual_capacity == 0 {
            engine.make_orphan(node, side, &mut queue, &mut queued)?;
            metrics.invalidated_parent_arcs = add_u64(metrics.invalidated_parent_arcs, 1)?;
        }
        checkpoints.push(DynamicRepairScanCheckpoint {
            node,
            arc: id,
            residual_capacity,
            repaired: residual_capacity == 0,
            eibfs_metrics: engine.metrics,
            dynamic_metrics: *metrics,
        });
    }
    engine.adopt_orphans(&mut queue, &mut queued, None)?;
    Ok(checkpoints)
}

fn publish_dynamic_repair_scan_checkpoints(
    engine: &mut EibfsEngine<'_>,
    cursor: &DynamicTraceCursor,
    metrics: &DynamicEibfsMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
    checkpoints: Vec<DynamicRepairScanCheckpoint>,
) -> Result<(), DynamicEibfsSolveError> {
    let final_eibfs_metrics = engine.metrics;
    for checkpoint in checkpoints {
        engine.metrics = checkpoint.eibfs_metrics;
        set_dynamic_stage(
            engine,
            cursor,
            DynamicEibfsTraceStage::RepairForest,
            None,
            &checkpoint.dynamic_metrics,
        );
        let (catalog_id, pseudocode_line) = if checkpoint.repaired {
            (
                "dynamic-eibfs.repair-invalidated-parent",
                "dynamic-eibfs:readopt-capacity-invalidated-forest-parent",
            )
        } else {
            (
                "dynamic-eibfs.inspect-retained-parent",
                "dynamic-eibfs:inspect-retained-parent-residual",
            )
        };
        engine.record(
            recorder.as_mut(),
            FlowTraceEventMetadata {
                catalog_id,
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line,
            },
            vec![checkpoint.node],
            vec![checkpoint.arc],
            Some(("residual", i128::from(checkpoint.residual_capacity))),
        )?;
    }
    engine.metrics = final_eibfs_metrics;
    set_dynamic_stage(
        engine,
        cursor,
        DynamicEibfsTraceStage::RepairForest,
        None,
        metrics,
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ViolationKind {
    Bridge,
    Label,
    CurrentArc,
    Boundary,
}

struct RepairViolation {
    kind: ViolationKind,
    arc: ResidualArcId,
    rewind: Option<(NodeIndex, usize)>,
}

fn classify_violation(
    engine: &EibfsEngine<'_>,
    id: ResidualArcId,
    from: NodeIndex,
    to: NodeIndex,
) -> Result<Option<RepairViolation>, DynamicEibfsSolveError> {
    let from_side = engine.nodes[from.as_usize()].state.normal();
    let to_side = engine.nodes[to.as_usize()].state.normal();
    if from_side == Some(ForestSide::Source) && to_side == Some(ForestSide::Sink) {
        return Ok(Some(RepairViolation {
            kind: ViolationKind::Bridge,
            arc: id,
            rewind: None,
        }));
    }
    if from_side == Some(ForestSide::Source) && to_side == Some(ForestSide::Source) {
        let from_label = engine.label(from, ForestSide::Source);
        let to_label = engine.label(to, ForestSide::Source);
        if to_label > from_label.saturating_add(1) {
            return Ok(Some(RepairViolation {
                kind: ViolationKind::Label,
                arc: id,
                rewind: None,
            }));
        }
        if to_label == from_label.saturating_add(1) {
            let position = engine.incoming[to.as_usize()]
                .binary_search(&id)
                .map_err(|_| EibfsError::ForestInvariant)?;
            if position < engine.nodes[to.as_usize()].current_arc {
                return Ok(Some(RepairViolation {
                    kind: ViolationKind::CurrentArc,
                    arc: id,
                    rewind: Some((to, position)),
                }));
            }
        }
    }
    if from_side == Some(ForestSide::Sink) && to_side == Some(ForestSide::Sink) {
        let from_label = engine.label(from, ForestSide::Sink);
        let to_label = engine.label(to, ForestSide::Sink);
        if from_label > to_label.saturating_add(1) {
            return Ok(Some(RepairViolation {
                kind: ViolationKind::Label,
                arc: id,
                rewind: None,
            }));
        }
        if from_label == to_label.saturating_add(1) {
            let position = engine.outgoing[from.as_usize()]
                .binary_search(&id)
                .map_err(|_| EibfsError::ForestInvariant)?;
            if position < engine.nodes[from.as_usize()].current_arc {
                return Ok(Some(RepairViolation {
                    kind: ViolationKind::CurrentArc,
                    arc: id,
                    rewind: Some((from, position)),
                }));
            }
        }
    }
    let source_boundary = from_side == Some(ForestSide::Source)
        && engine.label(from, ForestSide::Source) <= engine.source_depth
        && to_side != Some(ForestSide::Source);
    let sink_boundary = to_side == Some(ForestSide::Sink)
        && engine.label(to, ForestSide::Sink) <= engine.sink_depth
        && from_side != Some(ForestSide::Sink);
    if source_boundary || sink_boundary {
        return Ok(Some(RepairViolation {
            kind: ViolationKind::Boundary,
            arc: id,
            rewind: None,
        }));
    }
    Ok(None)
}

fn repair_violation(
    engine: &mut EibfsEngine<'_>,
    violation: &RepairViolation,
    metrics: &mut DynamicEibfsMetrics,
) -> Result<(), DynamicEibfsSolveError> {
    match violation.kind {
        ViolationKind::CurrentArc => {
            let (node, position) = violation.rewind.ok_or(EibfsError::ForestInvariant)?;
            engine.nodes[node.as_usize()].current_arc = position;
            engine.count_transition()?;
            metrics.current_arc_violations = add_u64(metrics.current_arc_violations, 1)?;
        }
        ViolationKind::Bridge | ViolationKind::Label | ViolationKind::Boundary => {
            let capacity = engine
                .residual
                .arc(&violation.arc)
                .ok_or(EibfsError::ForestInvariant)?
                .capacity;
            if capacity == 0 {
                return Err(EibfsError::ForestInvariant.into());
            }
            engine.push_residual(&violation.arc, capacity)?;
            engine.count_transition()?;
            let counter = match violation.kind {
                ViolationKind::Bridge => &mut metrics.bridge_violations,
                ViolationKind::Label => &mut metrics.label_violations,
                ViolationKind::Boundary => &mut metrics.boundary_violations,
                ViolationKind::CurrentArc => return Err(EibfsError::ForestInvariant.into()),
            };
            *counter = add_u64(*counter, 1)?;
        }
    }
    Ok(())
}

fn reused_forest_nodes(
    before: &[EibfsNode],
    after: &[EibfsNode],
) -> Result<u64, DynamicEibfsSolveError> {
    if before.len() != after.len() {
        return Err(EibfsError::ForestInvariant.into());
    }
    u64::try_from(
        before
            .iter()
            .zip(after)
            .filter(|(before, after)| before.state != ForestState::Free && before == after)
            .count(),
    )
    .map_err(|_| DynamicEibfsSolveError::ArithmeticOverflow)
}

fn add_u64(left: u64, right: u64) -> Result<u64, DynamicEibfsSolveError> {
    left.checked_add(right)
        .ok_or(DynamicEibfsSolveError::ArithmeticOverflow)
}

fn add_u128(left: u128, right: u128) -> Result<u128, DynamicEibfsSolveError> {
    left.checked_add(right)
        .ok_or(DynamicEibfsSolveError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use crate::algorithms::{solve_edmonds_karp, solve_eibfs};
    use crate::model::{FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    fn graph(edges: &[(&str, &str, &str, u64)]) -> (FlowNetwork, NodeIndex, NodeIndex) {
        graph_with_nodes(&["s", "a", "b", "t"], edges)
    }

    fn graph_with_nodes(
        node_ids: &[&str],
        edges: &[(&str, &str, &str, u64)],
    ) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let nodes = node_ids
            .iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
            .collect();
        let graph = FlowNetwork::new(
            nodes,
            edges
                .iter()
                .map(|&(id, from, to, capacity)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("edge"),
                    from: NodeId::parse(from).expect("from"),
                    to: NodeId::parse(to).expect("to"),
                    lower: 0,
                    capacity,
                    cost: 0,
                })
                .collect(),
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source"))
            .expect("source index");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink"))
            .expect("sink index");
        (graph, source, sink)
    }

    fn assert_prefix_oracles(
        graph: &FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        updates: &[DynamicCapacityUpdate],
        result: &DynamicEibfsResult,
        context: &str,
    ) {
        let problem = prepare_dynamic_eibfs(graph, source, sink, updates).expect("problem");
        for prefix in &result.prefixes {
            let current = materialize_current_graph(problem.envelope(), &prefix.capacities)
                .expect("current graph");
            let static_eibfs = solve_eibfs(&current, source, sink).unwrap_or_else(|error| {
                panic!(
                    "{context}, prefix {} static EIBFS: {error:?}",
                    prefix.update_index
                )
            });
            let edmonds_karp = solve_edmonds_karp(&current, source, sink).unwrap_or_else(|error| {
                panic!(
                    "{context}, prefix {} oracle: {error:?}",
                    prefix.update_index
                )
            });
            assert_eq!(
                prefix.certificate.value, static_eibfs.certificate.value,
                "{context}, prefix {} static EIBFS",
                prefix.update_index
            );
            assert_eq!(
                prefix.certificate.value, edmonds_karp.certificate.value,
                "{context}, prefix {} Edmonds-Karp",
                prefix.update_index
            );
            check_max_flow(&current, source, sink, &prefix.flows).unwrap_or_else(|error| {
                panic!(
                    "{context}, prefix {} certificate: {error:?}",
                    prefix.update_index
                )
            });
        }
    }

    fn next_random(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *state
    }

    #[test]
    fn reuses_state_across_capacity_increase_and_decrease() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 3),
            ("at", "a", "t", 3),
            ("sb", "s", "b", 1),
            ("bt", "b", "t", 1),
        ]);
        let updates = vec![
            DynamicCapacityUpdate::new(EdgeId::parse("sb").expect("edge"), 3),
            DynamicCapacityUpdate::new(EdgeId::parse("sa").expect("edge"), 1),
            DynamicCapacityUpdate::new(EdgeId::parse("at").expect("edge"), 1),
        ];
        let result = solve_dynamic_eibfs(&graph, source, sink, &updates).expect("dynamic solve");

        assert_eq!(
            result
                .prefixes
                .iter()
                .map(|prefix| prefix.certificate.value)
                .collect::<Vec<_>>(),
            vec![4, 4, 2, 2]
        );
        assert_eq!(result.dynamic_metrics.updates, 3);
        assert_eq!(result.dynamic_metrics.capacity_increases, 1);
        assert_eq!(result.dynamic_metrics.capacity_decreases, 2);
        assert!(result.dynamic_metrics.over_capacity_repairs > 0);
        assert!(result.dynamic_metrics.exactly_reused_forest_nodes > 0);
    }

    #[test]
    fn every_prefix_matches_static_eibfs_and_edmonds_karp() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 2),
            ("sb", "s", "b", 2),
            ("ab", "a", "b", 1),
            ("ba", "b", "a", 1),
            ("at", "a", "t", 2),
            ("bt", "b", "t", 2),
        ]);
        let updates = vec![
            DynamicCapacityUpdate::new(EdgeId::parse("ab").expect("edge"), 3),
            DynamicCapacityUpdate::new(EdgeId::parse("at").expect("edge"), 1),
            DynamicCapacityUpdate::new(EdgeId::parse("sb").expect("edge"), 4),
            DynamicCapacityUpdate::new(EdgeId::parse("bt").expect("edge"), 4),
            DynamicCapacityUpdate::new(EdgeId::parse("sa").expect("edge"), 0),
        ];
        for prefix_len in 1..=updates.len() {
            solve_dynamic_eibfs(&graph, source, sink, &updates[..prefix_len])
                .unwrap_or_else(|error| panic!("dynamic prefix {prefix_len}: {error:?}"));
        }
        let result = solve_dynamic_eibfs(&graph, source, sink, &updates).expect("dynamic solve");
        assert_prefix_oracles(&graph, source, sink, &updates, &result, "fixed cycle");
    }

    #[test]
    fn deterministic_adversarial_updates_match_prefix_oracles() {
        const TOPOLOGY: [(&str, &str, &str); 14] = [
            ("sa", "s", "a"),
            ("sa-parallel", "s", "a"),
            ("sb", "s", "b"),
            ("st", "s", "t"),
            ("ab", "a", "b"),
            ("ba", "b", "a"),
            ("aa", "a", "a"),
            ("ac", "a", "c"),
            ("bc", "b", "c"),
            ("cb", "c", "b"),
            ("ca", "c", "a"),
            ("at", "a", "t"),
            ("bt", "b", "t"),
            ("ct", "c", "t"),
        ];
        let mut aggregate = DynamicEibfsMetrics::default();
        for case in 0_u64..32 {
            let mut random = 0x9e37_79b9_7f4a_7c15_u64 ^ case;
            let edges = TOPOLOGY
                .iter()
                .map(|&(id, from, to)| (id, from, to, next_random(&mut random) % 6))
                .collect::<Vec<_>>();
            let (graph, source, sink) = graph_with_nodes(&["s", "a", "b", "c", "t"], &edges);
            let updates = (0..24)
                .map(|_| {
                    let edge = usize::try_from(next_random(&mut random) % TOPOLOGY.len() as u64)
                        .expect("topology index");
                    DynamicCapacityUpdate::new(
                        EdgeId::parse(TOPOLOGY[edge].0).expect("edge"),
                        next_random(&mut random) % 8,
                    )
                })
                .collect::<Vec<_>>();
            let context = format!("adversarial case {case}");
            for prefix_len in 1..=updates.len() {
                solve_dynamic_eibfs(&graph, source, sink, &updates[..prefix_len]).unwrap_or_else(
                    |error| {
                        panic!(
                            "{context}, solve prefix {prefix_len}: {error:?}; edges={edges:?}; updates={updates:?}"
                        )
                    },
                );
            }
            let result = solve_dynamic_eibfs(&graph, source, sink, &updates)
                .unwrap_or_else(|error| panic!("{context}: {error:?}"));
            assert_prefix_oracles(&graph, source, sink, &updates, &result, &context);
            aggregate.updates += result.dynamic_metrics.updates;
            aggregate.over_capacity_repairs += result.dynamic_metrics.over_capacity_repairs;
            aggregate.bridge_violations += result.dynamic_metrics.bridge_violations;
            aggregate.label_violations += result.dynamic_metrics.label_violations;
            aggregate.current_arc_violations += result.dynamic_metrics.current_arc_violations;
            aggregate.boundary_violations += result.dynamic_metrics.boundary_violations;
        }
        assert_eq!(aggregate.updates, 32 * 24);
        assert!(aggregate.over_capacity_repairs > 0);
        assert!(aggregate.bridge_violations > 0);
        assert!(aggregate.label_violations > 0);
        assert!(aggregate.current_arc_violations > 0);
        assert!(aggregate.boundary_violations > 0);
    }

    #[test]
    fn supports_no_op_and_u64_max_capacity_updates() {
        let (graph, source, sink) = graph(&[("st", "s", "t", u64::MAX)]);
        let updates = vec![
            DynamicCapacityUpdate::new(EdgeId::parse("st").expect("edge"), u64::MAX),
            DynamicCapacityUpdate::new(EdgeId::parse("st").expect("edge"), 0),
            DynamicCapacityUpdate::new(EdgeId::parse("st").expect("edge"), u64::MAX),
        ];
        let result = solve_dynamic_eibfs(&graph, source, sink, &updates).expect("dynamic solve");
        assert_eq!(result.dynamic_metrics.no_op_updates, 1);
        assert_eq!(
            result.dynamic_metrics.over_capacity_units,
            u128::from(u64::MAX)
        );
        assert_eq!(
            result
                .prefixes
                .iter()
                .map(|prefix| prefix.certificate.value)
                .collect::<Vec<_>>(),
            vec![
                i128::from(u64::MAX),
                i128::from(u64::MAX),
                0,
                i128::from(u64::MAX)
            ]
        );
    }

    #[test]
    fn dynamic_trace_replays_capacity_repair_and_warm_state_restoration() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 3),
            ("at", "a", "t", 3),
            ("sb", "s", "b", 1),
            ("bt", "b", "t", 1),
        ]);
        let updates = vec![
            DynamicCapacityUpdate::new(EdgeId::parse("sb").expect("edge"), 3),
            DynamicCapacityUpdate::new(EdgeId::parse("sa").expect("edge"), 1),
            DynamicCapacityUpdate::new(EdgeId::parse("at").expect("edge"), 1),
        ];
        let fast = solve_dynamic_eibfs(&graph, source, sink, &updates).expect("fast");
        let traced = trace_dynamic_eibfs(&graph, source, sink, &updates).expect("trace");
        let problem = prepare_dynamic_eibfs(&graph, source, sink, &updates).expect("problem");

        assert_eq!(traced.result, fast);
        assert_eq!(
            traced
                .events
                .iter()
                .filter(|event| event.catalog_id == "dynamic-eibfs.resume-reusable-pseudoflow")
                .count(),
            updates.len()
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| { event.catalog_id == "dynamic-eibfs.repair-over-capacity" })
        );

        let mut replay = traced.base_snapshot.clone();
        let mut saw_temporary_overflow = false;
        for event in &traced.events {
            apply_trace_event(
                problem.envelope(),
                &mut replay,
                event,
                FlowTraceDirection::Forward,
            )
            .expect("forward replay");
            if event.catalog_id == "dynamic-eibfs.apply-capacity-update"
                && replay
                    .dynamic_eibfs_overlay
                    .as_ref()
                    .is_some_and(|overlay| {
                        overlay
                            .changed_edge
                            .as_ref()
                            .is_some_and(|edge| edge.as_str() == "sa")
                    })
            {
                let index = problem
                    .envelope()
                    .edge_index(&EdgeId::parse("sa").expect("edge"))
                    .expect("index")
                    .as_usize();
                saw_temporary_overflow = replay.flows[index] > replay.edge_capacities[index];
            }
        }
        assert!(saw_temporary_overflow);
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(
            replay
                .dynamic_eibfs_overlay
                .as_ref()
                .map(|overlay| overlay.stage),
            Some(DynamicEibfsTraceStage::PrefixCertified)
        );

        for event in traced.events.iter().rev() {
            apply_trace_event(
                problem.envelope(),
                &mut replay,
                event,
                FlowTraceDirection::Reverse,
            )
            .expect("reverse replay");
        }
        assert_eq!(replay, traced.base_snapshot);
    }
}
