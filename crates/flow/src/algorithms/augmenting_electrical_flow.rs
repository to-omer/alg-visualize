//! Bounded augmenting-electrical-flow solver following Mądry (FOCS 2016).
//!
//! The source algorithm maintains a feasible primal flow and a coupled dual
//! embedding, augments in the electrical direction for the residual-barrier
//! resistances
//! `r_e = 1/(u_e^+ - f_e)^2 + 1/(u_e^- + f_e)^2`, fixes the second-order
//! coupling error with another electrical solve, and boosts high-energy arcs
//! by replacing them with the source-defined series path.  This module makes
//! those operations explicit for small interactive graphs.
//!
//! The improved asymptotic analysis chooses an `eta` containing a hidden
//! constant.  It is therefore not meaningful at this project's tiny graph
//! sizes.  The bounded realization uses the exact Section 3 `l4`-safe step,
//! exposes the Section 4 `l3` gate, and executes a finite number of exact boost
//! expansions before continuing with the Section 3 step.  Dense deterministic
//! elimination replaces the source's approximate Laplacian solver.  The
//! implementation deliberately does not claim the source's
//! `O~(m^(10/7) U^(1/7))` end-to-end bound.

// Every integer-to-float conversion is protected by the admission ceilings
// above (the largest working target is below 2^32). Float-to-integer casts are
// confined to the checked floor/ceiling cleanup boundary.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::collections::VecDeque;

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{EdgeId, FlowNetwork, NodeIndex};

/// Conservative original-node limit for exact cut enumeration and dense solves.
pub const AUGMENTING_ELECTRICAL_MAX_NODES: usize = 5;
/// Conservative original-edge limit before the three-edge directed reduction.
pub const AUGMENTING_ELECTRICAL_MAX_EDGES: usize = 6;
/// Largest admitted integral capacity.
pub const AUGMENTING_ELECTRICAL_MAX_CAPACITY: u64 = 8;
/// Maximum explicit working nodes after source-defined boost expansions.
pub const AUGMENTING_ELECTRICAL_MAX_WORKING_NODES: usize = 192;
/// Maximum explicit working edges after source-defined boost expansions.
pub const AUGMENTING_ELECTRICAL_MAX_WORKING_EDGES: usize = 384;
/// Maximum primal progress steps.
pub const AUGMENTING_ELECTRICAL_MAX_PROGRESS_STEPS: u64 = 4_096;
/// Visualization-sized Section 4 boost budget before Section 3-only progress.
pub const AUGMENTING_ELECTRICAL_MAX_BOOSTS: u64 = 8;
/// Maximum integral rounding, cleanup, and extraction transitions.
pub const AUGMENTING_ELECTRICAL_MAX_DISCRETE_TRANSITIONS: u64 = 65_536;
/// Maximum public trace boundaries.
pub const AUGMENTING_ELECTRICAL_MAX_TRACE_EVENTS: usize = 20_000;

const COUPLING_DENOMINATOR: f64 = 33.0;
const FEASIBILITY_DENOMINATOR: f64 = 4.0;
const WELL_COUPLED_LIMIT: f64 = 0.010_000_1;
const NUMERICAL_TOLERANCE: f64 = 1.0e-8;

/// Finite replay-safe scalar stored by IEEE-754 bit identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AugmentingElectricalScalar(u64);

impl AugmentingElectricalScalar {
    fn try_new(value: f64) -> Result<Self, AugmentingElectricalError> {
        if !value.is_finite() {
            return Err(AugmentingElectricalError::NumericalFailure);
        }
        Ok(Self(if value == 0.0 {
            0.0_f64.to_bits()
        } else {
            value.to_bits()
        }))
    }

    /// Recovers the finite scalar.
    #[must_use]
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Stable finite decimal used by the scene contract.
    #[must_use]
    pub fn decimal(self) -> String {
        super::stable_scene_decimal(self.get())
    }
}

/// Source or exact-cleanup boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AugmentingElectricalStage {
    /// Valid input with zero public flow.
    Ready,
    /// The three-edge directed-to-undirected reduction was built.
    BuildDirectedReduction,
    /// Source-defined symmetric `s-t` preconditioning arcs were added.
    AddPreconditioning,
    /// The bounded exact cut target was installed.
    InstallTargetCut,
    /// A residual-barrier electrical direction was solved.
    SolveElectricalDirection,
    /// One exact dense-elimination pivot row was processed.
    SolveElectricalPivot,
    /// One high-energy arc was replaced by the source boost path.
    BoostHighEnergyArc,
    /// The primal and dual solutions advanced in the electrical direction.
    AugmentPrimalDual,
    /// The source fixing electrical solve restored coupling.
    FixCoupling,
    /// Boost paths were contracted to their throughput-equivalent roots.
    CollapseBoostPaths,
    /// The fractional central flow was rounded inside floor/ceiling bounds.
    RoundCentralFlow,
    /// One integral residual augmenting path completed the source cleanup.
    CleanupAugmentingPath,
    /// Preconditioners were removed and the directed reduction was inverted.
    ExtractDirectedFlow,
    /// One auxiliary cycle in the reduction witness was canceled.
    CancelExtractionCycle,
    /// The half-integral directed witness was rounded integrally.
    RoundDirectedFlow,
    /// The independent maximum-flow/minimum-cut checker accepted the result.
    CheckCertificate,
    /// The bounded solver is complete.
    Optimal,
}

/// Original-node projection of the primal-dual state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AugmentingElectricalNodeState {
    /// Canonical original node.
    pub node: NodeIndex,
    /// Current dual embedding value.
    pub potential: AugmentingElectricalScalar,
    /// Largest incident normalized coupling violation.
    pub coupling_violation: AugmentingElectricalScalar,
    /// Whether the exact target cut places this node on the source side.
    pub target_source_side: bool,
}

/// Projection of the reduction's `h(e)` root onto one original directed edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AugmentingElectricalEdgeState {
    /// Stable original edge identity.
    pub edge: EdgeId,
    /// Current signed central flow on the transformed `h(e)` path.
    pub central_flow: AugmentingElectricalScalar,
    /// Latest signed electrical current in original-edge orientation.
    pub electrical_current: AugmentingElectricalScalar,
    /// Forward residual on the representative segment.
    pub forward_residual: AugmentingElectricalScalar,
    /// Backward residual on the representative segment.
    pub backward_residual: AugmentingElectricalScalar,
    /// Latest absolute electrical congestion.
    pub congestion: AugmentingElectricalScalar,
    /// Latest representative residual-barrier resistance.
    pub resistance: AugmentingElectricalScalar,
    /// Number of currently expanded leaf segments for this root.
    pub boost_segments: u64,
    /// Integral flow selected for this transformed central root during cleanup.
    pub rounded_central_flow: Option<i64>,
    /// Nonnegative doubled directed flow recovered from the central reduction arc.
    pub extraction_central_scaled: Option<u64>,
    /// Remaining auxiliary reduction flow directed from this edge's head to the source.
    pub extraction_toward_source: Option<u64>,
    /// Remaining auxiliary reduction flow directed from the sink to this edge's tail.
    pub extraction_out_of_sink: Option<u64>,
    /// Final integral original flow once extraction is complete.
    pub final_flow: Option<u64>,
}

/// Deterministic bounded work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AugmentingElectricalMetrics {
    /// Exact original cuts enumerated to install the target value.
    pub enumerated_cuts: u64,
    /// Dense electrical systems solved, including fixing systems.
    pub electrical_solves: u64,
    /// Dense Gaussian-elimination pivots.
    pub elimination_pivots: u64,
    /// Electrical primal progress steps.
    pub progress_steps: u64,
    /// Coupling-fixing steps.
    pub fixing_steps: u64,
    /// Source-defined boost expansions.
    pub boosts: u64,
    /// New vertices introduced by boost paths.
    pub boost_vertices: u64,
    /// Integral paths used by floor/ceiling rounding.
    pub rounding_paths: u64,
    /// Integral residual paths used by early cleanup.
    pub cleanup_augmentations: u64,
    /// Cycles canceled while inverting the directed reduction.
    pub extraction_cycles: u64,
    /// Independent terminal certificate checks.
    pub certificate_checks: u64,
    /// Public semantic boundaries.
    pub state_transitions: u64,
}

/// Complete reversible state at one source-level boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AugmentingElectricalSnapshot {
    /// Semantic boundary.
    pub stage: AugmentingElectricalStage,
    /// Exact maximum-flow value of the original directed instance.
    pub original_target: u64,
    /// Target after the three-edge undirected reduction.
    pub transformed_target: u64,
    /// Target after adding source preconditioners.
    pub working_target: u64,
    /// Current working-flow value.
    pub current_value: AugmentingElectricalScalar,
    /// Routed fraction of `working_target`.
    pub alpha: AugmentingElectricalScalar,
    /// Additive working flow still missing.
    pub remaining: AugmentingElectricalScalar,
    /// Latest electrical energy.
    pub electrical_energy: AugmentingElectricalScalar,
    /// Latest congestion `l3` norm.
    pub congestion_l3: AugmentingElectricalScalar,
    /// Latest congestion `l4` norm.
    pub congestion_l4: AugmentingElectricalScalar,
    /// Global normalized coupling violation `l2` norm.
    pub coupling_l2: AugmentingElectricalScalar,
    /// Current explicit node count, including boost vertices.
    pub working_nodes: u64,
    /// Current explicit edge count, including boost segments.
    pub working_edges: u64,
    /// Phase-local active working edge ordinal.
    pub active_working_edge: Option<u64>,
    /// Working-node equation used by the current dense-elimination pivot.
    pub active_pivot_node: Option<u64>,
    /// Exact working residual path used by the current discrete cleanup.
    pub active_working_path: Vec<AugmentingElectricalWorkingArc>,
    /// Exact directed-reduction cycle canceled during extraction.
    pub active_extraction_cycle: Vec<AugmentingElectricalExtractionArc>,
    /// Integral amount sent through `active_working_path`.
    pub active_discrete_amount: Option<u64>,
    /// Original-node dual projection.
    pub nodes: Vec<AugmentingElectricalNodeState>,
    /// Original-edge central/electrical projection.
    pub edges: Vec<AugmentingElectricalEdgeState>,
    /// Exact work counters.
    pub metrics: AugmentingElectricalMetrics,
}

/// One oriented arc of a transformed working-graph cleanup path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AugmentingElectricalWorkingArc {
    /// Working-edge ordinal.
    pub edge: u64,
    /// Whether the working edge is traversed in its declared direction.
    pub forward: bool,
    /// Original graph node at which this residual traversal starts.
    pub from: NodeIndex,
    /// Original graph node at which this residual traversal ends.
    pub to: NodeIndex,
    /// Integral working-edge flow after the cleanup augmentation.
    pub flow_after: i64,
}

/// Role of one directed-reduction arc on an extraction cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AugmentingElectricalExtractionArcKind {
    /// The central copy of an original directed edge.
    Central,
    /// The auxiliary copy directed toward the original source.
    TowardSource,
    /// The auxiliary copy directed out of the original sink.
    OutOfSink,
}

/// One source-reduction arc on the active extraction cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AugmentingElectricalExtractionArc {
    /// Original-edge ordinal that owns this reduction arc.
    pub edge: u64,
    /// Reduction role of the arc.
    pub kind: AugmentingElectricalExtractionArcKind,
}

/// One reversible source transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AugmentingElectricalTraceEvent {
    /// Stable event identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: AugmentingElectricalSnapshot,
    /// State after the transition.
    pub after: AugmentingElectricalSnapshot,
}

/// Independently certified maximum-flow result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AugmentingElectricalResult {
    /// Integral original-edge flows.
    pub flows: Vec<u64>,
    /// Independent maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Terminal public state.
    pub final_snapshot: AugmentingElectricalSnapshot,
    /// Exact work counters.
    pub metrics: AugmentingElectricalMetrics,
}

/// Result plus all reversible source boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AugmentingElectricalTraceResult {
    /// Same result as the fast profile.
    pub result: AugmentingElectricalResult,
    /// Ready state before the reduction.
    pub base_snapshot: AugmentingElectricalSnapshot,
    /// Canonical deterministic transitions.
    pub events: Vec<AugmentingElectricalTraceEvent>,
    /// Terminal state, equal to `result.final_snapshot`.
    pub final_snapshot: AugmentingElectricalSnapshot,
}

/// Admission, numerical, resource, extraction, certificate, or replay failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum AugmentingElectricalError {
    /// Original input exceeds the conservative interactive band.
    #[error("augmenting-electrical-flow input exceeds admission limits")]
    AdmissionLimit,
    /// The graph is outside the source max-flow model used by the reduction.
    #[error(
        "augmenting-electrical-flow requires positive bounded capacities, zero lower bounds/costs/supplies, and no self loops"
    )]
    GraphRequirement,
    /// Source and sink are equal or missing.
    #[error("augmenting-electrical-flow terminals must be distinct")]
    InvalidTerminals,
    /// A dense electrical solve or finite projection failed.
    #[error("augmenting-electrical-flow numerical invariant failed")]
    NumericalFailure,
    /// A primal, dual, barrier, coupling, or conservation invariant failed.
    #[error("augmenting-electrical-flow source invariant failed")]
    SourceInvariant,
    /// A boost would exceed the explicit transformed-graph ceiling.
    #[error("augmenting-electrical-flow boost expansion exceeds resource limits")]
    BoostResourceLimit,
    /// The deterministic progress or discrete-work ceiling was reached.
    #[error("augmenting-electrical-flow work ceiling reached")]
    WorkLimit,
    /// Integral rounding or the directed reduction extraction failed.
    #[error("augmenting-electrical-flow exact cleanup failed")]
    CleanupFailure,
    /// The independent maximum-flow/minimum-cut certificate rejected the result.
    #[error("augmenting-electrical-flow certificate failed: {0}")]
    Certificate(#[from] CertificateError),
    /// A supplied trace differs from deterministic replay.
    #[error("augmenting-electrical-flow trace verification failed")]
    TraceVerification,
}

/// Solves one bounded directed maximum-flow instance.
///
/// # Errors
///
/// Rejects unsupported or oversized input and any numerical, source-invariant,
/// resource, exact-cleanup, or independent-certificate failure.
pub fn solve_augmenting_electrical_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<AugmentingElectricalResult, AugmentingElectricalError> {
    run_internal(graph, source, sink, false).map(|run| run.result)
}

/// Records every reduction, electrical, boost, fixing, and cleanup boundary.
///
/// # Errors
///
/// Returns the same errors as [`solve_augmenting_electrical_flow`] or a replay
/// verification failure.
pub fn trace_augmenting_electrical_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<AugmentingElectricalTraceResult, AugmentingElectricalError> {
    let run = run_internal(graph, source, sink, true)?;
    let trace = AugmentingElectricalTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_augmenting_electrical_trace(graph, source, sink, &trace)?;
    Ok(trace)
}

/// Independently checks a public trace and its terminal certificate.
///
/// # Errors
///
/// Rejects any altered, omitted, reordered, or disconnected boundary.
#[expect(
    clippy::too_many_lines,
    reason = "the public checker keeps all cross-field trace invariants in one auditable validation pass"
)]
pub fn check_augmenting_electrical_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: &AugmentingElectricalTraceResult,
) -> Result<(), AugmentingElectricalError> {
    validate_input(graph, source, sink)?;
    if !valid_augmenting_electrical_base(graph, source, sink, &trace.base_snapshot)?
        || trace.final_snapshot.stage != AugmentingElectricalStage::Optimal
        || trace.final_snapshot != trace.result.final_snapshot
        || trace.final_snapshot.metrics != trace.result.metrics
        || trace.events.is_empty()
        || trace.events.len() > AUGMENTING_ELECTRICAL_MAX_TRACE_EVENTS
    {
        return Err(AugmentingElectricalError::TraceVerification);
    }
    let mut cursor = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != cursor
            || !valid_augmenting_electrical_event(event)
            || event.after.metrics.state_transitions
                != event.before.metrics.state_transitions.saturating_add(1)
            || event.after.nodes.len() != graph.nodes().len()
            || event.after.edges.len() != graph.edges().len()
            || event
                .after
                .edges
                .iter()
                .all(|edge| edge.rounded_central_flow.is_some())
                != matches!(
                    event.after.stage,
                    AugmentingElectricalStage::RoundCentralFlow
                        | AugmentingElectricalStage::CleanupAugmentingPath
                        | AugmentingElectricalStage::ExtractDirectedFlow
                        | AugmentingElectricalStage::CancelExtractionCycle
                        | AugmentingElectricalStage::RoundDirectedFlow
                        | AugmentingElectricalStage::CheckCertificate
                        | AugmentingElectricalStage::Optimal
                )
            || event.after.edges.iter().any(|edge| {
                let components = [
                    edge.extraction_central_scaled.is_some(),
                    edge.extraction_toward_source.is_some(),
                    edge.extraction_out_of_sink.is_some(),
                ];
                components.iter().any(|present| *present)
                    && !components.iter().all(|present| *present)
            })
            || event.after.edges.iter().all(|edge| {
                edge.extraction_central_scaled.is_some()
                    && edge.extraction_toward_source.is_some()
                    && edge.extraction_out_of_sink.is_some()
            }) != matches!(
                event.after.stage,
                AugmentingElectricalStage::ExtractDirectedFlow
                    | AugmentingElectricalStage::CancelExtractionCycle
                    | AugmentingElectricalStage::RoundDirectedFlow
                    | AugmentingElectricalStage::CheckCertificate
                    | AugmentingElectricalStage::Optimal
            )
            || event.after.active_pivot_node.is_some()
                != (event.after.stage == AugmentingElectricalStage::SolveElectricalPivot)
            || event
                .after
                .active_pivot_node
                .is_some_and(|node| node >= event.after.working_nodes)
            || (event.after.stage == AugmentingElectricalStage::SolveElectricalPivot
                && event.after.metrics.elimination_pivots
                    != event.before.metrics.elimination_pivots.saturating_add(1))
            || event.after.active_working_path.iter().any(|arc| {
                arc.edge >= event.after.working_edges
                    || arc.from.as_usize() >= graph.nodes().len()
                    || arc.to.as_usize() >= graph.nodes().len()
            })
            || event
                .after
                .active_working_path
                .windows(2)
                .any(|pair| pair[0].to != pair[1].from)
            || event
                .after
                .active_working_path
                .first()
                .is_some_and(|arc| arc.from != source)
            || event
                .after
                .active_working_path
                .last()
                .is_some_and(|arc| arc.to != sink)
            || event.after.nodes.iter().enumerate().any(|(index, state)| {
                state.node.as_usize() != index
                    || !state.potential.get().is_finite()
                    || !state.coupling_violation.get().is_finite()
            })
            || event
                .after
                .edges
                .iter()
                .zip(graph.edges())
                .any(|(state, edge)| {
                    &state.edge != edge.id()
                        || !state.central_flow.get().is_finite()
                        || !state.electrical_current.get().is_finite()
                        || !state.forward_residual.get().is_finite()
                        || !state.backward_residual.get().is_finite()
                        || !state.congestion.get().is_finite()
                        || !state.resistance.get().is_finite()
                        || state.final_flow.is_some_and(|flow| flow > edge.capacity())
                })
        {
            return Err(AugmentingElectricalError::TraceVerification);
        }
        cursor = &event.after;
    }
    let certificate = check_max_flow(graph, source, sink, &trace.result.flows)?;
    if cursor != &trace.final_snapshot
        || trace.events.len() as u64 != trace.final_snapshot.metrics.state_transitions
        || certificate != trace.result.certificate
        || certificate.value != i128::from(trace.final_snapshot.original_target)
        || trace
            .final_snapshot
            .edges
            .iter()
            .zip(&trace.result.flows)
            .any(|(edge, flow)| edge.final_flow != Some(*flow))
    {
        return Err(AugmentingElectricalError::TraceVerification);
    }
    Ok(())
}

fn valid_augmenting_electrical_base(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    snapshot: &AugmentingElectricalSnapshot,
) -> Result<bool, AugmentingElectricalError> {
    let (original_target, _target_source_side, enumerated_cuts) =
        enumerate_target_cut(graph, source, sink)?;
    let zero = AugmentingElectricalScalar::try_new(0.0)?;
    let expected = AugmentingElectricalSnapshot {
        stage: AugmentingElectricalStage::Ready,
        original_target,
        transformed_target: 0,
        working_target: 0,
        current_value: zero,
        alpha: zero,
        remaining: zero,
        electrical_energy: zero,
        congestion_l3: zero,
        congestion_l4: zero,
        coupling_l2: zero,
        working_nodes: u64::try_from(graph.nodes().len())
            .map_err(|_| AugmentingElectricalError::WorkLimit)?,
        working_edges: 0,
        active_working_edge: None,
        active_pivot_node: None,
        active_working_path: Vec::new(),
        active_extraction_cycle: Vec::new(),
        active_discrete_amount: None,
        nodes: graph
            .node_indices()
            .map(|node| AugmentingElectricalNodeState {
                node,
                potential: zero,
                coupling_violation: zero,
                target_source_side: false,
            })
            .collect(),
        edges: graph
            .edges()
            .iter()
            .map(|edge| {
                Ok(AugmentingElectricalEdgeState {
                    edge: edge.id().clone(),
                    central_flow: zero,
                    electrical_current: zero,
                    forward_residual: AugmentingElectricalScalar::try_new(edge.capacity() as f64)?,
                    backward_residual: zero,
                    congestion: zero,
                    resistance: zero,
                    boost_segments: 0,
                    rounded_central_flow: None,
                    extraction_central_scaled: None,
                    extraction_toward_source: None,
                    extraction_out_of_sink: None,
                    final_flow: None,
                })
            })
            .collect::<Result<Vec<_>, AugmentingElectricalError>>()?,
        metrics: AugmentingElectricalMetrics {
            enumerated_cuts,
            ..AugmentingElectricalMetrics::default()
        },
    };
    Ok(snapshot == &expected)
}

#[allow(clippy::too_many_lines)]
fn valid_augmenting_electrical_event(event: &AugmentingElectricalTraceEvent) -> bool {
    let catalog_matches_stage = matches!(
        (event.catalog_id, event.after.stage),
        (
            "augmenting-electrical-flow.build-directed-reduction",
            AugmentingElectricalStage::BuildDirectedReduction
        ) | (
            "augmenting-electrical-flow.add-preconditioning",
            AugmentingElectricalStage::AddPreconditioning
        ) | (
            "augmenting-electrical-flow.install-target-cut",
            AugmentingElectricalStage::InstallTargetCut
        ) | (
            "augmenting-electrical-flow.solve-direction"
                | "augmenting-electrical-flow.resolve-after-boost",
            AugmentingElectricalStage::SolveElectricalDirection
        ) | (
            "augmenting-electrical-flow.elimination-pivot",
            AugmentingElectricalStage::SolveElectricalPivot
        ) | (
            "augmenting-electrical-flow.boost-high-energy",
            AugmentingElectricalStage::BoostHighEnergyArc
        ) | (
            "augmenting-electrical-flow.augment-primal-dual",
            AugmentingElectricalStage::AugmentPrimalDual
        ) | (
            "augmenting-electrical-flow.fix-coupling",
            AugmentingElectricalStage::FixCoupling
        ) | (
            "augmenting-electrical-flow.collapse-boost-paths",
            AugmentingElectricalStage::CollapseBoostPaths
        ) | (
            "augmenting-electrical-flow.round-central-flow",
            AugmentingElectricalStage::RoundCentralFlow
        ) | (
            "augmenting-electrical-flow.cleanup-augmenting-path",
            AugmentingElectricalStage::CleanupAugmentingPath
        ) | (
            "augmenting-electrical-flow.extract-directed-reduction",
            AugmentingElectricalStage::ExtractDirectedFlow
        ) | (
            "augmenting-electrical-flow.cancel-extraction-cycle",
            AugmentingElectricalStage::CancelExtractionCycle
        ) | (
            "augmenting-electrical-flow.round-directed-flow",
            AugmentingElectricalStage::RoundDirectedFlow
        ) | (
            "augmenting-electrical-flow.check-certificate",
            AugmentingElectricalStage::CheckCertificate
        ) | (
            "augmenting-electrical-flow.optimal",
            AugmentingElectricalStage::Optimal
        )
    );
    let stage_transition = matches!(
        (event.before.stage, event.after.stage),
        (
            AugmentingElectricalStage::Ready,
            AugmentingElectricalStage::BuildDirectedReduction
        ) | (
            AugmentingElectricalStage::BuildDirectedReduction,
            AugmentingElectricalStage::AddPreconditioning
        ) | (
            AugmentingElectricalStage::AddPreconditioning,
            AugmentingElectricalStage::InstallTargetCut
        ) | (
            AugmentingElectricalStage::InstallTargetCut
                | AugmentingElectricalStage::FixCoupling
                | AugmentingElectricalStage::BoostHighEnergyArc
                | AugmentingElectricalStage::AugmentPrimalDual
                | AugmentingElectricalStage::SolveElectricalPivot,
            AugmentingElectricalStage::SolveElectricalPivot
        ) | (
            AugmentingElectricalStage::SolveElectricalPivot,
            AugmentingElectricalStage::SolveElectricalDirection
                | AugmentingElectricalStage::FixCoupling
        ) | (
            AugmentingElectricalStage::InstallTargetCut | AugmentingElectricalStage::FixCoupling,
            AugmentingElectricalStage::SolveElectricalDirection
                | AugmentingElectricalStage::CollapseBoostPaths
        ) | (
            AugmentingElectricalStage::SolveElectricalDirection,
            AugmentingElectricalStage::BoostHighEnergyArc
                | AugmentingElectricalStage::AugmentPrimalDual
        ) | (
            AugmentingElectricalStage::BoostHighEnergyArc,
            AugmentingElectricalStage::SolveElectricalDirection
        ) | (
            AugmentingElectricalStage::AugmentPrimalDual,
            AugmentingElectricalStage::FixCoupling
        ) | (
            AugmentingElectricalStage::CollapseBoostPaths,
            AugmentingElectricalStage::RoundCentralFlow
        ) | (
            AugmentingElectricalStage::RoundCentralFlow
                | AugmentingElectricalStage::CleanupAugmentingPath,
            AugmentingElectricalStage::CleanupAugmentingPath
                | AugmentingElectricalStage::ExtractDirectedFlow
        ) | (
            AugmentingElectricalStage::ExtractDirectedFlow
                | AugmentingElectricalStage::CancelExtractionCycle,
            AugmentingElectricalStage::CancelExtractionCycle
                | AugmentingElectricalStage::RoundDirectedFlow
        ) | (
            AugmentingElectricalStage::RoundDirectedFlow,
            AugmentingElectricalStage::CheckCertificate
        ) | (
            AugmentingElectricalStage::CheckCertificate,
            AugmentingElectricalStage::Optimal
        )
    );
    catalog_matches_stage && stage_transition
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RootRole {
    OriginalH(usize),
    OriginalSourceAux(usize),
    OriginalSinkAux(usize),
    Precondition,
}

#[derive(Clone, Debug)]
struct RootEdge {
    from: usize,
    to: usize,
    upper: f64,
    lower_magnitude: f64,
    role: RootRole,
}

#[derive(Clone, Debug)]
struct WorkingEdge {
    from: usize,
    to: usize,
    upper: f64,
    lower_magnitude: f64,
    flow: f64,
    root: usize,
    root_sign: f64,
}

#[derive(Clone, Debug)]
struct KernelState {
    roots: Vec<RootEdge>,
    edges: Vec<WorkingEdge>,
    potentials: Vec<f64>,
    latest_currents: Vec<f64>,
    latest_resistances: Vec<f64>,
    latest_congestions: Vec<f64>,
    target_source_side: Vec<bool>,
    original_target: u64,
    transformed_target: u64,
    working_target: u64,
    alpha: f64,
    energy: f64,
    l3: f64,
    l4: f64,
    coupling_l2: f64,
    active_edge: Option<usize>,
    active_pivot_node: Option<usize>,
    active_working_path: Vec<(usize, bool)>,
    active_extraction_cycle: Vec<ReductionArcKind>,
    active_discrete_amount: Option<u64>,
    rounded_working_flows: Option<Vec<i64>>,
    reduction_extraction: Option<ReductionExtraction>,
    final_flows: Vec<Option<u64>>,
    stage: AugmentingElectricalStage,
    metrics: AugmentingElectricalMetrics,
}

struct InternalRun {
    result: AugmentingElectricalResult,
    base_snapshot: AugmentingElectricalSnapshot,
    events: Vec<AugmentingElectricalTraceEvent>,
    final_snapshot: AugmentingElectricalSnapshot,
}

struct Recorder<'a> {
    graph: &'a FlowNetwork,
    state: KernelState,
    trace: bool,
    events: Vec<AugmentingElectricalTraceEvent>,
    last_snapshot: Option<AugmentingElectricalSnapshot>,
}

impl Recorder<'_> {
    fn snapshot(&self) -> Result<AugmentingElectricalSnapshot, AugmentingElectricalError> {
        make_snapshot(self.graph, &self.state)
    }

    fn emit(
        &mut self,
        catalog_id: &'static str,
        stage: AugmentingElectricalStage,
        active_edge: Option<usize>,
    ) -> Result<(), AugmentingElectricalError> {
        let before = self
            .last_snapshot
            .clone()
            .ok_or(AugmentingElectricalError::TraceVerification)?;
        self.state.stage = stage;
        self.state.active_edge = active_edge;
        self.state.active_pivot_node = None;
        self.state.active_working_path.clear();
        self.state.active_extraction_cycle.clear();
        self.state.active_discrete_amount = None;
        self.commit_event(catalog_id, before)
    }

    fn emit_working_path(
        &mut self,
        catalog_id: &'static str,
        stage: AugmentingElectricalStage,
        path: &[(usize, bool)],
        amount: u64,
    ) -> Result<(), AugmentingElectricalError> {
        let before = self
            .last_snapshot
            .clone()
            .ok_or(AugmentingElectricalError::TraceVerification)?;
        self.state.stage = stage;
        self.state.active_edge = path.first().map(|(edge, _)| *edge);
        self.state.active_pivot_node = None;
        self.state.active_working_path = path.to_vec();
        self.state.active_extraction_cycle.clear();
        self.state.active_discrete_amount = Some(amount);
        self.commit_event(catalog_id, before)
    }

    fn emit_extraction_cycle(
        &mut self,
        cycle: Vec<ReductionArcKind>,
        amount: u64,
    ) -> Result<(), AugmentingElectricalError> {
        let before = self
            .last_snapshot
            .clone()
            .ok_or(AugmentingElectricalError::TraceVerification)?;
        self.state.stage = AugmentingElectricalStage::CancelExtractionCycle;
        self.state.active_edge = None;
        self.state.active_pivot_node = None;
        self.state.active_working_path.clear();
        self.state.active_extraction_cycle = cycle;
        self.state.active_discrete_amount = Some(amount);
        self.commit_event("augmenting-electrical-flow.cancel-extraction-cycle", before)
    }

    fn emit_elimination_pivots(
        &mut self,
        pivot_nodes: &[usize],
    ) -> Result<(), AugmentingElectricalError> {
        for &pivot_node in pivot_nodes {
            let before = self
                .last_snapshot
                .clone()
                .ok_or(AugmentingElectricalError::TraceVerification)?;
            self.state.stage = AugmentingElectricalStage::SolveElectricalPivot;
            self.state.active_edge = None;
            self.state.active_pivot_node = Some(pivot_node);
            self.state.active_working_path.clear();
            self.state.active_extraction_cycle.clear();
            self.state.active_discrete_amount = None;
            self.state.metrics.elimination_pivots = self
                .state
                .metrics
                .elimination_pivots
                .checked_add(1)
                .ok_or(AugmentingElectricalError::WorkLimit)?;
            self.commit_event("augmenting-electrical-flow.elimination-pivot", before)?;
        }
        Ok(())
    }

    fn commit_event(
        &mut self,
        catalog_id: &'static str,
        before: AugmentingElectricalSnapshot,
    ) -> Result<(), AugmentingElectricalError> {
        self.state.metrics.state_transitions = self
            .state
            .metrics
            .state_transitions
            .checked_add(1)
            .ok_or(AugmentingElectricalError::WorkLimit)?;
        let after = self.snapshot()?;
        self.last_snapshot = Some(after.clone());
        if self.trace {
            if self.events.len() >= AUGMENTING_ELECTRICAL_MAX_TRACE_EVENTS {
                return Err(AugmentingElectricalError::WorkLimit);
            }
            self.events.push(AugmentingElectricalTraceEvent {
                catalog_id,
                before,
                after,
            });
        }
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
fn run_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: bool,
) -> Result<InternalRun, AugmentingElectricalError> {
    validate_input(graph, source, sink)?;
    let (original_target, target_source_side, enumerated_cuts) =
        enumerate_target_cut(graph, source, sink)?;
    let n = graph.nodes().len();
    let m = graph.edges().len();
    let state = KernelState {
        roots: Vec::new(),
        edges: Vec::new(),
        potentials: vec![0.0; n],
        latest_currents: Vec::new(),
        latest_resistances: Vec::new(),
        latest_congestions: Vec::new(),
        // The cut is known to the bounded oracle here, but it is not part of
        // the published algorithm state until `install-target-cut` below.
        target_source_side: vec![false; n],
        original_target,
        transformed_target: 0,
        working_target: 0,
        alpha: 0.0,
        energy: 0.0,
        l3: 0.0,
        l4: 0.0,
        coupling_l2: 0.0,
        active_edge: None,
        active_pivot_node: None,
        active_working_path: Vec::new(),
        active_extraction_cycle: Vec::new(),
        active_discrete_amount: None,
        rounded_working_flows: None,
        reduction_extraction: None,
        final_flows: vec![None; m],
        stage: AugmentingElectricalStage::Ready,
        metrics: AugmentingElectricalMetrics {
            enumerated_cuts,
            ..AugmentingElectricalMetrics::default()
        },
    };
    let mut recorder = Recorder {
        graph,
        state,
        trace,
        events: Vec::new(),
        last_snapshot: None,
    };
    let base_snapshot = recorder.snapshot()?;
    recorder.last_snapshot = Some(base_snapshot.clone());

    build_directed_reduction(graph, source, sink, &mut recorder.state)?;
    recorder.emit(
        "augmenting-electrical-flow.build-directed-reduction",
        AugmentingElectricalStage::BuildDirectedReduction,
        None,
    )?;
    add_preconditioners(graph, source, sink, &mut recorder.state)?;
    recorder.emit(
        "augmenting-electrical-flow.add-preconditioning",
        AugmentingElectricalStage::AddPreconditioning,
        None,
    )?;
    recorder.state.target_source_side = target_source_side;
    recorder.emit(
        "augmenting-electrical-flow.install-target-cut",
        AugmentingElectricalStage::InstallTargetCut,
        None,
    )?;

    if recorder.state.working_target > 0 {
        loop {
            if recorder.state.metrics.progress_steps >= AUGMENTING_ELECTRICAL_MAX_PROGRESS_STEPS {
                return Err(AugmentingElectricalError::WorkLimit);
            }
            let remaining = remaining(&recorder.state)?;
            let early_threshold = (recorder.state.edges.len() as f64).sqrt();
            if remaining <= early_threshold + NUMERICAL_TOLERANCE {
                break;
            }

            let mut electrical = electrical_direction(
                recorder.state.potentials.len(),
                &recorder.state.edges,
                source.as_usize(),
                sink.as_usize(),
                bounded_f64(recorder.state.working_target)?,
            )?;
            recorder.emit_elimination_pivots(&electrical.pivot_nodes)?;
            install_electrical_state(&mut recorder.state, &electrical);
            recorder.emit(
                "augmenting-electrical-flow.solve-direction",
                AugmentingElectricalStage::SolveElectricalDirection,
                None,
            )?;

            let l3_gate = electrical.l3
                <= (recorder.state.edges.len() as f64).sqrt()
                    / (COUPLING_DENOMINATOR * (1.0 - recorder.state.alpha));
            if !l3_gate
                && recorder.state.metrics.progress_steps > 0
                && recorder.state.metrics.boosts < AUGMENTING_ELECTRICAL_MAX_BOOSTS
                && let Some(active) = select_boost_edge(&recorder.state.edges, &electrical)
            {
                boost_edge(graph, active, &mut recorder.state)?;
                recorder.emit(
                    "augmenting-electrical-flow.boost-high-energy",
                    AugmentingElectricalStage::BoostHighEnergyArc,
                    Some(active),
                )?;
                electrical = electrical_direction(
                    recorder.state.potentials.len(),
                    &recorder.state.edges,
                    source.as_usize(),
                    sink.as_usize(),
                    bounded_f64(recorder.state.working_target)?,
                )?;
                recorder.emit_elimination_pivots(&electrical.pivot_nodes)?;
                install_electrical_state(&mut recorder.state, &electrical);
                recorder.emit(
                    "augmenting-electrical-flow.resolve-after-boost",
                    AugmentingElectricalStage::SolveElectricalDirection,
                    None,
                )?;
            }

            let remaining_fraction = 1.0 - recorder.state.alpha;
            let improved_gate = electrical.l3
                <= (recorder.state.edges.len() as f64).sqrt()
                    / (COUPLING_DENOMINATOR * remaining_fraction);
            let source_l4_step = 1.0 / (COUPLING_DENOMINATOR * electrical.l4);
            let proposed = if improved_gate {
                1.0 / (COUPLING_DENOMINATOR * remaining_fraction * electrical.l3)
            } else {
                source_l4_step
            };
            let feasibility = 1.0 / (FEASIBILITY_DENOMINATOR * electrical.linf);
            let delta = proposed
                .min(source_l4_step)
                .min(feasibility)
                .min(remaining_fraction);
            if !delta.is_finite() || delta <= 0.0 {
                return Err(AugmentingElectricalError::NumericalFailure);
            }
            for (edge, current) in recorder.state.edges.iter_mut().zip(&electrical.currents) {
                edge.flow += delta * current;
            }
            for (potential, direction) in recorder
                .state
                .potentials
                .iter_mut()
                .zip(&electrical.potentials)
            {
                *potential += delta * direction;
            }
            recorder.state.alpha += delta;
            recorder.state.metrics.progress_steps += 1;
            check_primal_feasibility(&recorder.state.edges)?;
            recorder.emit(
                "augmenting-electrical-flow.augment-primal-dual",
                AugmentingElectricalStage::AugmentPrimalDual,
                None,
            )?;

            let fixing_pivots =
                fix_coupling(source.as_usize(), sink.as_usize(), &mut recorder.state)?;
            recorder.emit_elimination_pivots(&fixing_pivots)?;
            recorder.state.metrics.fixing_steps += 1;
            recorder.state.metrics.electrical_solves += 1;
            recorder.emit(
                "augmenting-electrical-flow.fix-coupling",
                AugmentingElectricalStage::FixCoupling,
                None,
            )?;
        }
    }

    let collapsed = collapse_roots(&recorder.state)?;
    recorder.state.edges = collapsed;
    recorder.state.potentials.truncate(n);
    recorder.state.latest_currents = vec![0.0; recorder.state.edges.len()];
    recorder.state.latest_resistances = vec![0.0; recorder.state.edges.len()];
    recorder.state.latest_congestions = vec![0.0; recorder.state.edges.len()];
    recorder.emit(
        "augmenting-electrical-flow.collapse-boost-paths",
        AugmentingElectricalStage::CollapseBoostPaths,
        None,
    )?;

    let central_value = (recorder.state.alpha * bounded_f64(recorder.state.working_target)?)
        .floor()
        .max(0.0) as i64;
    let mut integral = round_fractional_flow(
        n,
        &recorder.state.edges,
        source.as_usize(),
        sink.as_usize(),
        central_value,
        &mut recorder.state.metrics,
    )?;
    recorder.state.rounded_working_flows = Some(integral.clone());
    recorder.emit(
        "augmenting-electrical-flow.round-central-flow",
        AugmentingElectricalStage::RoundCentralFlow,
        None,
    )?;

    let cleanup_edges = recorder.state.edges.clone();
    cleanup_to_target(
        n,
        &cleanup_edges,
        source.as_usize(),
        sink.as_usize(),
        i64::try_from(recorder.state.working_target)
            .map_err(|_| AugmentingElectricalError::AdmissionLimit)?,
        &mut integral,
        &mut recorder,
    )?;

    let mut reduced =
        remove_preconditioners_and_extract_reduction(graph, &recorder.state, &integral)?;
    recorder.state.reduction_extraction = Some(reduced.clone());
    recorder.emit(
        "augmenting-electrical-flow.extract-directed-reduction",
        AugmentingElectricalStage::ExtractDirectedFlow,
        None,
    )?;
    cancel_reduction_cycles(graph, source, sink, &mut reduced, &mut recorder)?;
    let fractional_original = reduced
        .central_scaled
        .iter()
        .map(|&amount| amount as f64 / 2.0)
        .collect::<Vec<_>>();
    let original_edges = graph
        .edges()
        .iter()
        .enumerate()
        .map(|(index, edge)| WorkingEdge {
            from: edge.from().as_usize(),
            to: edge.to().as_usize(),
            upper: edge.capacity() as f64,
            lower_magnitude: 0.0,
            flow: fractional_original[index],
            root: index,
            root_sign: 1.0,
        })
        .collect::<Vec<_>>();
    let original_integral = round_fractional_flow(
        n,
        &original_edges,
        source.as_usize(),
        sink.as_usize(),
        i64::try_from(recorder.state.original_target)
            .map_err(|_| AugmentingElectricalError::AdmissionLimit)?,
        &mut recorder.state.metrics,
    )?;
    let flows = original_integral
        .into_iter()
        .map(|flow| u64::try_from(flow).map_err(|_| AugmentingElectricalError::CleanupFailure))
        .collect::<Result<Vec<_>, _>>()?;
    recorder.state.final_flows = flows.iter().copied().map(Some).collect();
    recorder.emit(
        "augmenting-electrical-flow.round-directed-flow",
        AugmentingElectricalStage::RoundDirectedFlow,
        None,
    )?;

    let certificate = check_max_flow(graph, source, sink, &flows)?;
    if certificate.value != i128::from(recorder.state.original_target) {
        return Err(AugmentingElectricalError::CleanupFailure);
    }
    recorder.state.metrics.certificate_checks += 1;
    recorder.emit(
        "augmenting-electrical-flow.check-certificate",
        AugmentingElectricalStage::CheckCertificate,
        None,
    )?;
    recorder.emit(
        "augmenting-electrical-flow.optimal",
        AugmentingElectricalStage::Optimal,
        None,
    )?;
    let final_snapshot = recorder.snapshot()?;
    let result = AugmentingElectricalResult {
        flows,
        certificate,
        final_snapshot: final_snapshot.clone(),
        metrics: recorder.state.metrics,
    };
    Ok(InternalRun {
        result,
        base_snapshot,
        events: recorder.events,
        final_snapshot,
    })
}

fn validate_input(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), AugmentingElectricalError> {
    if graph.nodes().len() > AUGMENTING_ELECTRICAL_MAX_NODES
        || graph.edges().len() > AUGMENTING_ELECTRICAL_MAX_EDGES
    {
        return Err(AugmentingElectricalError::AdmissionLimit);
    }
    if source == sink
        || source.as_usize() >= graph.nodes().len()
        || sink.as_usize() >= graph.nodes().len()
    {
        return Err(AugmentingElectricalError::InvalidTerminals);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0)
        || graph.edges().iter().any(|edge| {
            edge.lower() != 0
                || edge.cost() != 0
                || edge.capacity() == 0
                || edge.capacity() > AUGMENTING_ELECTRICAL_MAX_CAPACITY
                || edge.from() == edge.to()
        })
    {
        return Err(AugmentingElectricalError::GraphRequirement);
    }
    Ok(())
}

fn enumerate_target_cut(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(u64, Vec<bool>, u64), AugmentingElectricalError> {
    let n = graph.nodes().len();
    let free = (0..n)
        .filter(|&node| node != source.as_usize() && node != sink.as_usize())
        .collect::<Vec<_>>();
    let combinations = 1_usize
        .checked_shl(
            u32::try_from(free.len()).map_err(|_| AugmentingElectricalError::AdmissionLimit)?,
        )
        .ok_or(AugmentingElectricalError::AdmissionLimit)?;
    let mut best = u128::MAX;
    let mut best_side = vec![false; n];
    for mask in 0..combinations {
        let mut side = vec![false; n];
        side[source.as_usize()] = true;
        for (bit, &node) in free.iter().enumerate() {
            side[node] = mask & (1_usize << bit) != 0;
        }
        let cut = graph.edges().iter().try_fold(0_u128, |sum, edge| {
            if side[edge.from().as_usize()] && !side[edge.to().as_usize()] {
                sum.checked_add(u128::from(edge.capacity()))
            } else {
                Some(sum)
            }
        });
        let cut = cut.ok_or(AugmentingElectricalError::AdmissionLimit)?;
        if cut < best {
            best = cut;
            best_side = side;
        }
    }
    let value = u64::try_from(best).map_err(|_| AugmentingElectricalError::AdmissionLimit)?;
    Ok((
        value,
        best_side,
        u64::try_from(combinations).map_err(|_| AugmentingElectricalError::AdmissionLimit)?,
    ))
}

fn build_directed_reduction(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    state: &mut KernelState,
) -> Result<(), AugmentingElectricalError> {
    let mut capacity_sum = 0_u64;
    for (index, edge) in graph.edges().iter().enumerate() {
        let capacity = edge.capacity() as f64;
        capacity_sum = capacity_sum
            .checked_add(edge.capacity())
            .ok_or(AugmentingElectricalError::AdmissionLimit)?;
        let declarations = [
            (
                edge.from().as_usize(),
                edge.to().as_usize(),
                RootRole::OriginalH(index),
            ),
            (
                source.as_usize(),
                edge.to().as_usize(),
                RootRole::OriginalSourceAux(index),
            ),
            (
                edge.from().as_usize(),
                sink.as_usize(),
                RootRole::OriginalSinkAux(index),
            ),
        ];
        for (from, to, role) in declarations {
            let root = state.roots.len();
            state.roots.push(RootEdge {
                from,
                to,
                upper: capacity,
                lower_magnitude: capacity,
                role,
            });
            state.edges.push(WorkingEdge {
                from,
                to,
                upper: capacity,
                lower_magnitude: capacity,
                flow: 0.0,
                root,
                root_sign: 1.0,
            });
        }
    }
    state.transformed_target = capacity_sum
        .checked_add(
            state
                .original_target
                .checked_mul(2)
                .ok_or(AugmentingElectricalError::AdmissionLimit)?,
        )
        .ok_or(AugmentingElectricalError::AdmissionLimit)?;
    Ok(())
}

fn add_preconditioners(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    state: &mut KernelState,
) -> Result<(), AugmentingElectricalError> {
    let transformed_edges = state.roots.len();
    let maximum_capacity = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::capacity)
        .max()
        .unwrap_or(0);
    let capacity = maximum_capacity
        .checked_mul(2)
        .ok_or(AugmentingElectricalError::AdmissionLimit)?;
    for _ in 0..transformed_edges {
        let root = state.roots.len();
        state.roots.push(RootEdge {
            from: source.as_usize(),
            to: sink.as_usize(),
            upper: capacity as f64,
            lower_magnitude: capacity as f64,
            role: RootRole::Precondition,
        });
        state.edges.push(WorkingEdge {
            from: source.as_usize(),
            to: sink.as_usize(),
            upper: capacity as f64,
            lower_magnitude: capacity as f64,
            flow: 0.0,
            root,
            root_sign: 1.0,
        });
    }
    let precondition_value = u64::try_from(transformed_edges)
        .ok()
        .and_then(|count| count.checked_mul(capacity))
        .ok_or(AugmentingElectricalError::AdmissionLimit)?;
    state.working_target = state
        .transformed_target
        .checked_add(precondition_value)
        .ok_or(AugmentingElectricalError::AdmissionLimit)?;
    Ok(())
}

#[derive(Clone, Debug)]
struct ElectricalDirection {
    potentials: Vec<f64>,
    currents: Vec<f64>,
    resistances: Vec<f64>,
    congestions: Vec<f64>,
    energy: f64,
    l3: f64,
    l4: f64,
    linf: f64,
    pivot_nodes: Vec<usize>,
}

fn electrical_direction(
    node_count: usize,
    edges: &[WorkingEdge],
    source: usize,
    sink: usize,
    amount: f64,
) -> Result<ElectricalDirection, AugmentingElectricalError> {
    let resistances = edges
        .iter()
        .map(barrier_resistance)
        .collect::<Result<Vec<_>, _>>()?;
    let mut demand = vec![0.0; node_count];
    demand[source] = -amount;
    demand[sink] = amount;
    let (potentials, pivot_nodes) =
        solve_laplacian(node_count, edges, &resistances, sink, &demand)?;
    let mut currents = Vec::with_capacity(edges.len());
    let mut congestions = Vec::with_capacity(edges.len());
    let mut energy = 0.0;
    let mut cube_sum = 0.0;
    let mut fourth_sum = 0.0;
    let mut linf = 0.0_f64;
    for (edge, &resistance) in edges.iter().zip(&resistances) {
        let current = (potentials[edge.to] - potentials[edge.from]) / resistance;
        let residual = forward_residual(edge).min(backward_residual(edge));
        let congestion = current / residual;
        if !current.is_finite() || !congestion.is_finite() {
            return Err(AugmentingElectricalError::NumericalFailure);
        }
        energy += resistance * current * current;
        cube_sum += congestion.abs().powi(3);
        fourth_sum += congestion.powi(4);
        linf = linf.max(congestion.abs());
        currents.push(current);
        congestions.push(congestion);
    }
    if !energy.is_finite() || linf <= 0.0 {
        return Err(AugmentingElectricalError::NumericalFailure);
    }
    check_demand(node_count, edges, &currents, &demand, amount)?;
    Ok(ElectricalDirection {
        potentials,
        currents,
        resistances,
        congestions,
        energy,
        l3: cube_sum.cbrt(),
        l4: fourth_sum.sqrt().sqrt(),
        linf,
        pivot_nodes,
    })
}

fn install_electrical_state(state: &mut KernelState, direction: &ElectricalDirection) {
    state.latest_currents.clone_from(&direction.currents);
    state.latest_resistances.clone_from(&direction.resistances);
    state.latest_congestions.clone_from(&direction.congestions);
    state.energy = direction.energy;
    state.l3 = direction.l3;
    state.l4 = direction.l4;
    state.metrics.electrical_solves += 1;
}

fn barrier_resistance(edge: &WorkingEdge) -> Result<f64, AugmentingElectricalError> {
    let forward = forward_residual(edge);
    let backward = backward_residual(edge);
    if forward <= 0.0 || backward <= 0.0 || !forward.is_finite() && !edge.upper.is_infinite() {
        return Err(AugmentingElectricalError::SourceInvariant);
    }
    let forward_term = if forward.is_infinite() {
        0.0
    } else {
        1.0 / forward.powi(2)
    };
    let resistance = forward_term + 1.0 / backward.powi(2);
    if !resistance.is_finite() || resistance <= 0.0 {
        return Err(AugmentingElectricalError::NumericalFailure);
    }
    Ok(resistance)
}

fn forward_residual(edge: &WorkingEdge) -> f64 {
    edge.upper - edge.flow
}

fn backward_residual(edge: &WorkingEdge) -> f64 {
    edge.lower_magnitude + edge.flow
}

fn barrier_phi(edge: &WorkingEdge) -> Result<f64, AugmentingElectricalError> {
    let forward = forward_residual(edge);
    let backward = backward_residual(edge);
    if forward <= 0.0 || backward <= 0.0 {
        return Err(AugmentingElectricalError::SourceInvariant);
    }
    let forward_inverse = if forward.is_infinite() {
        0.0
    } else {
        1.0 / forward
    };
    let value = forward_inverse - 1.0 / backward;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(AugmentingElectricalError::NumericalFailure)
    }
}

#[allow(clippy::too_many_lines, clippy::needless_range_loop)]
fn solve_laplacian(
    node_count: usize,
    edges: &[WorkingEdge],
    resistances: &[f64],
    ground: usize,
    demand: &[f64],
) -> Result<(Vec<f64>, Vec<usize>), AugmentingElectricalError> {
    if demand.len() != node_count || resistances.len() != edges.len() || ground >= node_count {
        return Err(AugmentingElectricalError::NumericalFailure);
    }
    let mut component = vec![usize::MAX; node_count];
    let mut component_count = 0_usize;
    for start in 0..node_count {
        if component[start] != usize::MAX {
            continue;
        }
        component[start] = component_count;
        let mut queue = VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            for edge in edges {
                let next = if edge.from == node {
                    Some(edge.to)
                } else if edge.to == node {
                    Some(edge.from)
                } else {
                    None
                };
                if let Some(next) = next
                    && component[next] == usize::MAX
                {
                    component[next] = component_count;
                    queue.push_back(next);
                }
            }
        }
        component_count += 1;
    }
    let mut component_ground = vec![None::<usize>; component_count];
    for node in 0..node_count {
        let slot = &mut component_ground[component[node]];
        if node == ground || slot.is_none() {
            *slot = Some(node);
        }
    }
    let grounded = component_ground
        .into_iter()
        .map(|node| node.ok_or(AugmentingElectricalError::NumericalFailure))
        .collect::<Result<Vec<_>, _>>()?;
    for current_component in 0..component_count {
        let sum = demand
            .iter()
            .enumerate()
            .filter(|(node, _)| component[*node] == current_component)
            .map(|(_, value)| value)
            .sum::<f64>();
        if sum.abs()
            > NUMERICAL_TOLERANCE * demand.iter().map(|value| value.abs()).sum::<f64>().max(1.0)
        {
            return Err(AugmentingElectricalError::SourceInvariant);
        }
    }
    let mut inverse = vec![None; node_count];
    let mut dimension = 0;
    for (node, slot) in inverse.iter_mut().enumerate() {
        if grounded[component[node]] != node {
            *slot = Some(dimension);
            dimension += 1;
        }
    }
    if dimension == 0 {
        return Err(AugmentingElectricalError::NumericalFailure);
    }
    let mut matrix = vec![vec![0.0; dimension + 1]; dimension];
    for (edge, &resistance) in edges.iter().zip(resistances) {
        let conductance = 1.0 / resistance;
        if let Some(row) = inverse[edge.from] {
            matrix[row][row] += conductance;
        }
        if let Some(row) = inverse[edge.to] {
            matrix[row][row] += conductance;
        }
        if let (Some(left), Some(right)) = (inverse[edge.from], inverse[edge.to]) {
            matrix[left][right] -= conductance;
            matrix[right][left] -= conductance;
        }
    }
    for node in 0..node_count {
        if let Some(row) = inverse[node] {
            matrix[row][dimension] = demand[node];
        }
    }
    let mut row_nodes = inverse
        .iter()
        .enumerate()
        .filter_map(|(node, row)| row.map(|row| (row, node)))
        .collect::<Vec<_>>();
    row_nodes.sort_unstable_by_key(|(row, _)| *row);
    let mut row_nodes = row_nodes
        .into_iter()
        .map(|(_, node)| node)
        .collect::<Vec<_>>();
    let mut pivot_nodes = Vec::with_capacity(dimension);
    for pivot in 0..dimension {
        let selected = (pivot..dimension)
            .max_by(|&left, &right| {
                matrix[left][pivot]
                    .abs()
                    .total_cmp(&matrix[right][pivot].abs())
            })
            .ok_or(AugmentingElectricalError::NumericalFailure)?;
        if matrix[selected][pivot].abs() <= f64::EPSILON {
            return Err(AugmentingElectricalError::NumericalFailure);
        }
        matrix.swap(pivot, selected);
        row_nodes.swap(pivot, selected);
        pivot_nodes.push(row_nodes[pivot]);
        let divisor = matrix[pivot][pivot];
        for column in pivot..=dimension {
            matrix[pivot][column] /= divisor;
        }
        for row in 0..dimension {
            if row == pivot {
                continue;
            }
            let factor = matrix[row][pivot];
            if factor == 0.0 {
                continue;
            }
            for column in pivot..=dimension {
                matrix[row][column] -= factor * matrix[pivot][column];
            }
        }
    }
    let mut potentials = vec![0.0; node_count];
    for node in 0..node_count {
        if let Some(row) = inverse[node] {
            let value = matrix[row][dimension];
            if !value.is_finite() {
                return Err(AugmentingElectricalError::NumericalFailure);
            }
            potentials[node] = value;
        }
    }
    Ok((potentials, pivot_nodes))
}

fn check_demand(
    node_count: usize,
    edges: &[WorkingEdge],
    flows: &[f64],
    demand: &[f64],
    scale: f64,
) -> Result<(), AugmentingElectricalError> {
    let mut actual = vec![0.0; node_count];
    for (edge, &flow) in edges.iter().zip(flows) {
        actual[edge.from] -= flow;
        actual[edge.to] += flow;
    }
    let tolerance = NUMERICAL_TOLERANCE * scale.abs().max(1.0) * node_count as f64;
    if actual
        .iter()
        .zip(demand)
        .any(|(left, right)| (left - right).abs() > tolerance)
    {
        return Err(AugmentingElectricalError::SourceInvariant);
    }
    Ok(())
}

fn select_boost_edge(edges: &[WorkingEdge], direction: &ElectricalDirection) -> Option<usize> {
    edges
        .iter()
        .zip(&direction.congestions)
        .enumerate()
        .filter_map(|(index, (edge, congestion))| {
            let forward = forward_residual(edge);
            let backward = backward_residual(edge);
            let phi = (1.0 / forward) - (1.0 / backward);
            (phi.abs() > NUMERICAL_TOLERANCE && forward.is_finite())
                .then_some((index, congestion.abs()))
        })
        .max_by(|(left_index, left), (right_index, right)| {
            left.total_cmp(right)
                .then_with(|| right_index.cmp(left_index))
        })
        .map(|(index, _)| index)
}

#[allow(clippy::too_many_lines)]
fn boost_edge(
    graph: &FlowNetwork,
    active: usize,
    state: &mut KernelState,
) -> Result<(), AugmentingElectricalError> {
    let mut original = state
        .edges
        .get(active)
        .cloned()
        .ok_or(AugmentingElectricalError::SourceInvariant)?;
    if backward_residual(&original) < forward_residual(&original) {
        std::mem::swap(&mut original.from, &mut original.to);
        std::mem::swap(&mut original.upper, &mut original.lower_magnitude);
        original.flow = -original.flow;
        original.root_sign = -original.root_sign;
    }
    let residual = forward_residual(&original).min(backward_residual(&original));
    let phi = barrier_phi(&original)?;
    if phi <= NUMERICAL_TOLERANCE || !residual.is_finite() {
        return Err(AugmentingElectricalError::SourceInvariant);
    }
    let maximum_capacity = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::capacity)
        .max()
        .unwrap_or(1) as f64;
    let beta = 2_usize
        .checked_add((2.0 * maximum_capacity / residual).ceil() as usize)
        .ok_or(AugmentingElectricalError::BoostResourceLimit)?;
    if beta < 3
        || state.potentials.len().saturating_add(beta - 1) > AUGMENTING_ELECTRICAL_MAX_WORKING_NODES
        || state.edges.len().saturating_add(beta - 1) > AUGMENTING_ELECTRICAL_MAX_WORKING_EDGES
    {
        return Err(AugmentingElectricalError::BoostResourceLimit);
    }
    let auxiliary_capacity = (beta - 2) as f64 / phi - original.flow;
    if !auxiliary_capacity.is_finite()
        || auxiliary_capacity + NUMERICAL_TOLERANCE < maximum_capacity
    {
        return Err(AugmentingElectricalError::SourceInvariant);
    }
    let endpoint_potential = state.potentials[original.to];
    let first_new = state.potentials.len();
    for position in 1..beta {
        let potential = if position == 1 {
            endpoint_potential
        } else {
            endpoint_potential + phi - (position - 2) as f64 * phi / (beta - 2) as f64
        };
        state.potentials.push(potential);
    }
    let vertex = |position: usize| -> usize {
        if position == 0 {
            original.from
        } else if position == beta {
            original.to
        } else {
            first_new + position - 1
        }
    };
    let mut replacement = Vec::with_capacity(beta);
    for position in 1..=beta {
        let copy = position <= 2;
        replacement.push(WorkingEdge {
            from: vertex(position - 1),
            to: vertex(position),
            upper: if copy { original.upper } else { f64::INFINITY },
            lower_magnitude: if copy {
                original.lower_magnitude
            } else {
                auxiliary_capacity
            },
            flow: original.flow,
            root: original.root,
            root_sign: original.root_sign,
        });
    }
    state.edges.splice(active..=active, replacement);
    state.latest_currents = vec![0.0; state.edges.len()];
    state.latest_resistances = vec![0.0; state.edges.len()];
    state.latest_congestions = vec![0.0; state.edges.len()];
    state.metrics.boosts += 1;
    state.metrics.boost_vertices = state
        .metrics
        .boost_vertices
        .checked_add(u64::try_from(beta - 1).map_err(|_| AugmentingElectricalError::WorkLimit)?)
        .ok_or(AugmentingElectricalError::WorkLimit)?;
    let coupling = coupling_norm(&state.edges, &state.potentials)?;
    if coupling > WELL_COUPLED_LIMIT {
        return Err(AugmentingElectricalError::SourceInvariant);
    }
    state.coupling_l2 = coupling;
    Ok(())
}

fn fix_coupling(
    source: usize,
    sink: usize,
    state: &mut KernelState,
) -> Result<Vec<usize>, AugmentingElectricalError> {
    let mut theta = Vec::with_capacity(state.edges.len());
    for edge in &state.edges {
        let mismatch = state.potentials[edge.to] - state.potentials[edge.from] - barrier_phi(edge)?;
        theta.push(mismatch / barrier_resistance(edge)?);
    }
    for (edge, correction) in state.edges.iter_mut().zip(&theta) {
        edge.flow += correction;
    }
    check_primal_feasibility(&state.edges)?;
    let resistances = state
        .edges
        .iter()
        .map(barrier_resistance)
        .collect::<Result<Vec<_>, _>>()?;
    let mut sigma = vec![0.0; state.potentials.len()];
    for (edge, correction) in state.edges.iter().zip(&theta) {
        sigma[edge.from] -= correction;
        sigma[edge.to] += correction;
    }
    let demand = sigma.iter().map(|value| -value).collect::<Vec<_>>();
    let (fixing_potentials, pivot_nodes) = solve_laplacian(
        state.potentials.len(),
        &state.edges,
        &resistances,
        sink,
        &demand,
    )?;
    let fixing_currents = state
        .edges
        .iter()
        .zip(&resistances)
        .map(|(edge, resistance)| {
            (fixing_potentials[edge.to] - fixing_potentials[edge.from]) / resistance
        })
        .collect::<Vec<_>>();
    check_demand(
        state.potentials.len(),
        &state.edges,
        &fixing_currents,
        &demand,
        bounded_f64(state.working_target)?,
    )?;
    for (edge, correction) in state.edges.iter_mut().zip(&fixing_currents) {
        edge.flow += correction;
    }
    for (potential, correction) in state.potentials.iter_mut().zip(&fixing_potentials) {
        *potential += correction;
    }
    check_primal_feasibility(&state.edges)?;
    let coupling = coupling_norm(&state.edges, &state.potentials)?;
    if coupling > WELL_COUPLED_LIMIT {
        return Err(AugmentingElectricalError::SourceInvariant);
    }
    let expected = {
        let mut demand = vec![0.0; state.potentials.len()];
        let amount = state.alpha * bounded_f64(state.working_target)?;
        demand[source] = -amount;
        demand[sink] = amount;
        demand
    };
    let flows = state.edges.iter().map(|edge| edge.flow).collect::<Vec<_>>();
    check_demand(
        state.potentials.len(),
        &state.edges,
        &flows,
        &expected,
        bounded_f64(state.working_target)?,
    )?;
    state.coupling_l2 = coupling;
    Ok(pivot_nodes)
}

fn coupling_norm(
    edges: &[WorkingEdge],
    potentials: &[f64],
) -> Result<f64, AugmentingElectricalError> {
    let mut squared = 0.0;
    for edge in edges {
        let residual = forward_residual(edge).min(backward_residual(edge));
        let mismatch = potentials[edge.to] - potentials[edge.from] - barrier_phi(edge)?;
        let gamma = mismatch.abs() * residual;
        squared += gamma * gamma;
    }
    let norm = squared.sqrt();
    if norm.is_finite() {
        Ok(norm)
    } else {
        Err(AugmentingElectricalError::NumericalFailure)
    }
}

fn check_primal_feasibility(edges: &[WorkingEdge]) -> Result<(), AugmentingElectricalError> {
    if edges.iter().any(|edge| {
        edge.flow > edge.upper + NUMERICAL_TOLERANCE
            || edge.flow < -edge.lower_magnitude - NUMERICAL_TOLERANCE
            || !edge.flow.is_finite()
    }) {
        return Err(AugmentingElectricalError::SourceInvariant);
    }
    Ok(())
}

fn collapse_roots(state: &KernelState) -> Result<Vec<WorkingEdge>, AugmentingElectricalError> {
    let mut flows = vec![None::<f64>; state.roots.len()];
    for edge in &state.edges {
        let root_flow = edge.root_sign * edge.flow;
        match flows[edge.root] {
            Some(existing)
                if (existing - root_flow).abs()
                    > NUMERICAL_TOLERANCE * bounded_f64(state.working_target)?.max(1.0) =>
            {
                return Err(AugmentingElectricalError::CleanupFailure);
            }
            Some(_) => {}
            None => flows[edge.root] = Some(root_flow),
        }
    }
    state
        .roots
        .iter()
        .enumerate()
        .map(|(index, root)| {
            Ok(WorkingEdge {
                from: root.from,
                to: root.to,
                upper: root.upper,
                lower_magnitude: root.lower_magnitude,
                flow: flows[index].ok_or(AugmentingElectricalError::CleanupFailure)?,
                root: index,
                root_sign: 1.0,
            })
        })
        .collect()
}

fn round_fractional_flow(
    node_count: usize,
    edges: &[WorkingEdge],
    source: usize,
    sink: usize,
    target_value: i64,
    metrics: &mut AugmentingElectricalMetrics,
) -> Result<Vec<i64>, AugmentingElectricalError> {
    let mut lower = Vec::with_capacity(edges.len());
    let mut capacities = Vec::with_capacity(edges.len());
    for edge in edges {
        let floor = (edge.flow + NUMERICAL_TOLERANCE).floor() as i64;
        let ceil = (edge.flow - NUMERICAL_TOLERANCE).ceil() as i64;
        if floor < (-edge.lower_magnitude - NUMERICAL_TOLERANCE).ceil() as i64
            || !edge.upper.is_infinite() && ceil > (edge.upper + NUMERICAL_TOLERANCE).floor() as i64
            || ceil < floor
            || ceil - floor > 1
        {
            return Err(AugmentingElectricalError::CleanupFailure);
        }
        lower.push(floor);
        capacities.push(ceil - floor);
    }
    let mut increments = vec![0_i64; edges.len()];
    let mut required = vec![0_i64; node_count];
    required[source] = target_value;
    required[sink] = -target_value;
    for (edge, &flow) in edges.iter().zip(&lower) {
        required[edge.from] -= flow;
        required[edge.to] += flow;
    }
    let mut transitions = 0_u64;
    while let Some(start) = required.iter().position(|&value| value > 0) {
        transitions += 1;
        if transitions > AUGMENTING_ELECTRICAL_MAX_DISCRETE_TRANSITIONS {
            return Err(AugmentingElectricalError::WorkLimit);
        }
        let (end, path) = find_rounding_path(
            node_count,
            edges,
            &capacities,
            &increments,
            start,
            &required,
        )?;
        for (edge_index, forward) in path {
            if forward {
                increments[edge_index] += 1;
            } else {
                increments[edge_index] -= 1;
            }
        }
        required[start] -= 1;
        required[end] += 1;
        metrics.rounding_paths += 1;
    }
    if required.iter().any(|&value| value != 0) {
        return Err(AugmentingElectricalError::CleanupFailure);
    }
    lower
        .into_iter()
        .zip(increments)
        .map(|(left, right)| {
            left.checked_add(right)
                .ok_or(AugmentingElectricalError::CleanupFailure)
        })
        .collect()
}

fn find_rounding_path(
    node_count: usize,
    edges: &[WorkingEdge],
    capacities: &[i64],
    increments: &[i64],
    start: usize,
    required: &[i64],
) -> Result<(usize, Vec<(usize, bool)>), AugmentingElectricalError> {
    let mut parent = vec![None::<(usize, usize, bool)>; node_count];
    let mut queue = VecDeque::from([start]);
    parent[start] = Some((start, usize::MAX, true));
    let mut end = None;
    while let Some(node) = queue.pop_front() {
        if node != start && required[node] < 0 {
            end = Some(node);
            break;
        }
        for (index, edge) in edges.iter().enumerate() {
            let candidate = if edge.from == node && increments[index] < capacities[index] {
                Some((edge.to, true))
            } else if edge.to == node && increments[index] > 0 {
                Some((edge.from, false))
            } else {
                None
            };
            if let Some((next, forward)) = candidate
                && parent[next].is_none()
            {
                parent[next] = Some((node, index, forward));
                queue.push_back(next);
            }
        }
    }
    let end = end.ok_or(AugmentingElectricalError::CleanupFailure)?;
    let mut path = Vec::new();
    let mut cursor = end;
    while cursor != start {
        let (previous, edge, forward) =
            parent[cursor].ok_or(AugmentingElectricalError::CleanupFailure)?;
        path.push((edge, forward));
        cursor = previous;
    }
    path.reverse();
    Ok((end, path))
}

fn cleanup_to_target(
    node_count: usize,
    edges: &[WorkingEdge],
    source: usize,
    sink: usize,
    target: i64,
    flows: &mut [i64],
    recorder: &mut Recorder<'_>,
) -> Result<(), AugmentingElectricalError> {
    let mut current = integral_flow_value(edges, flows, source)?;
    while current < target {
        if recorder.state.metrics.cleanup_augmentations
            >= AUGMENTING_ELECTRICAL_MAX_DISCRETE_TRANSITIONS
        {
            return Err(AugmentingElectricalError::WorkLimit);
        }
        let path = find_integral_augmenting_path(node_count, edges, flows, source, sink)?;
        let amount = path
            .iter()
            .map(|&(index, forward)| integral_residual(&edges[index], flows[index], forward))
            .min()
            .ok_or(AugmentingElectricalError::CleanupFailure)?
            .min(target - current);
        if amount <= 0 {
            return Err(AugmentingElectricalError::CleanupFailure);
        }
        for (index, forward) in &path {
            if *forward {
                flows[*index] += amount;
            } else {
                flows[*index] -= amount;
            }
        }
        current += amount;
        recorder.state.rounded_working_flows = Some(flows.to_vec());
        recorder.state.metrics.cleanup_augmentations += 1;
        recorder.emit_working_path(
            "augmenting-electrical-flow.cleanup-augmenting-path",
            AugmentingElectricalStage::CleanupAugmentingPath,
            &path,
            u64::try_from(amount).map_err(|_| AugmentingElectricalError::CleanupFailure)?,
        )?;
    }
    if current != target {
        return Err(AugmentingElectricalError::CleanupFailure);
    }
    Ok(())
}

fn integral_flow_value(
    edges: &[WorkingEdge],
    flows: &[i64],
    source: usize,
) -> Result<i64, AugmentingElectricalError> {
    edges
        .iter()
        .zip(flows)
        .try_fold(0_i64, |value, (edge, &flow)| {
            if edge.from == source {
                value.checked_add(flow)
            } else if edge.to == source {
                value.checked_sub(flow)
            } else {
                Some(value)
            }
            .ok_or(AugmentingElectricalError::CleanupFailure)
        })
}

fn find_integral_augmenting_path(
    node_count: usize,
    edges: &[WorkingEdge],
    flows: &[i64],
    source: usize,
    sink: usize,
) -> Result<Vec<(usize, bool)>, AugmentingElectricalError> {
    let mut parent = vec![None::<(usize, usize, bool)>; node_count];
    let mut queue = VecDeque::from([source]);
    parent[source] = Some((source, usize::MAX, true));
    while let Some(node) = queue.pop_front() {
        if node == sink {
            break;
        }
        for (index, edge) in edges.iter().enumerate() {
            let candidate = if edge.from == node && integral_residual(edge, flows[index], true) > 0
            {
                Some((edge.to, true))
            } else if edge.to == node && integral_residual(edge, flows[index], false) > 0 {
                Some((edge.from, false))
            } else {
                None
            };
            if let Some((next, forward)) = candidate
                && parent[next].is_none()
            {
                parent[next] = Some((node, index, forward));
                queue.push_back(next);
            }
        }
    }
    if parent[sink].is_none() {
        return Err(AugmentingElectricalError::CleanupFailure);
    }
    let mut path = Vec::new();
    let mut cursor = sink;
    while cursor != source {
        let (previous, edge, forward) =
            parent[cursor].ok_or(AugmentingElectricalError::CleanupFailure)?;
        path.push((edge, forward));
        cursor = previous;
    }
    path.reverse();
    Ok(path)
}

fn integral_residual(edge: &WorkingEdge, flow: i64, forward: bool) -> i64 {
    if forward {
        edge.upper as i64 - flow
    } else {
        edge.lower_magnitude as i64 + flow
    }
}

#[derive(Clone, Debug)]
struct ReductionExtraction {
    central_scaled: Vec<i64>,
    toward_source: Vec<i64>,
    out_of_sink: Vec<i64>,
}

fn remove_preconditioners_and_extract_reduction(
    graph: &FlowNetwork,
    state: &KernelState,
    integral: &[i64],
) -> Result<ReductionExtraction, AugmentingElectricalError> {
    let mut h = vec![0_i64; graph.edges().len()];
    let mut source_aux = vec![0_i64; graph.edges().len()];
    let mut sink_aux = vec![0_i64; graph.edges().len()];
    for (root, &flow) in state.roots.iter().zip(integral) {
        match root.role {
            RootRole::OriginalH(index) => {
                h[index] = flow
                    .checked_add(bounded_i64(graph.edges()[index].capacity())?)
                    .ok_or(AugmentingElectricalError::CleanupFailure)?;
            }
            RootRole::OriginalSourceAux(index) => {
                source_aux[index] = bounded_i64(graph.edges()[index].capacity())?
                    .checked_sub(flow)
                    .ok_or(AugmentingElectricalError::CleanupFailure)?;
            }
            RootRole::OriginalSinkAux(index) => {
                sink_aux[index] = bounded_i64(graph.edges()[index].capacity())?
                    .checked_sub(flow)
                    .ok_or(AugmentingElectricalError::CleanupFailure)?;
            }
            RootRole::Precondition => {
                if flow != root.upper as i64 {
                    return Err(AugmentingElectricalError::CleanupFailure);
                }
            }
        }
    }
    if h.iter().any(|&value| value < 0)
        || source_aux.iter().any(|&value| value < 0)
        || sink_aux.iter().any(|&value| value < 0)
    {
        return Err(AugmentingElectricalError::CleanupFailure);
    }
    Ok(ReductionExtraction {
        central_scaled: h,
        toward_source: source_aux,
        out_of_sink: sink_aux,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReductionArcKind {
    H(usize),
    SourceAux(usize),
    SinkAux(usize),
}

fn cancel_reduction_cycles(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    extraction: &mut ReductionExtraction,
    recorder: &mut Recorder<'_>,
) -> Result<(), AugmentingElectricalError> {
    let n = graph.nodes().len();
    loop {
        let arcs = reduction_positive_arcs(graph, source, sink, extraction);
        let Some(cycle) = find_directed_cycle(n, &arcs) else {
            break;
        };
        if recorder.state.metrics.extraction_cycles
            >= AUGMENTING_ELECTRICAL_MAX_DISCRETE_TRANSITIONS
        {
            return Err(AugmentingElectricalError::WorkLimit);
        }
        let amount = cycle
            .iter()
            .map(|&arc| arcs[arc].2)
            .min()
            .ok_or(AugmentingElectricalError::CleanupFailure)?;
        let active_cycle = cycle.iter().map(|&arc| arcs[arc].3).collect::<Vec<_>>();
        for &arc in &cycle {
            match arcs[arc].3 {
                ReductionArcKind::H(index) => extraction.central_scaled[index] -= amount,
                ReductionArcKind::SourceAux(index) => {
                    extraction.toward_source[index] -= amount;
                }
                ReductionArcKind::SinkAux(index) => {
                    extraction.out_of_sink[index] -= amount;
                }
            }
        }
        recorder.state.reduction_extraction = Some(extraction.clone());
        recorder.state.metrics.extraction_cycles += 1;
        recorder.emit_extraction_cycle(
            active_cycle,
            u64::try_from(amount).map_err(|_| AugmentingElectricalError::CleanupFailure)?,
        )?;
    }
    if extraction
        .toward_source
        .iter()
        .chain(&extraction.out_of_sink)
        .any(|&value| value != 0)
    {
        return Err(AugmentingElectricalError::CleanupFailure);
    }
    Ok(())
}

fn reduction_positive_arcs(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    extraction: &ReductionExtraction,
) -> Vec<(usize, usize, i64, ReductionArcKind)> {
    let mut arcs = Vec::new();
    for (index, edge) in graph.edges().iter().enumerate() {
        if extraction.central_scaled[index] > 0 {
            arcs.push((
                edge.from().as_usize(),
                edge.to().as_usize(),
                extraction.central_scaled[index],
                ReductionArcKind::H(index),
            ));
        }
        if extraction.toward_source[index] > 0 {
            arcs.push((
                edge.to().as_usize(),
                source.as_usize(),
                extraction.toward_source[index],
                ReductionArcKind::SourceAux(index),
            ));
        }
        if extraction.out_of_sink[index] > 0 {
            arcs.push((
                sink.as_usize(),
                edge.from().as_usize(),
                extraction.out_of_sink[index],
                ReductionArcKind::SinkAux(index),
            ));
        }
    }
    arcs
}

fn find_directed_cycle(
    node_count: usize,
    arcs: &[(usize, usize, i64, ReductionArcKind)],
) -> Option<Vec<usize>> {
    let mut color = vec![0_u8; node_count];
    let mut parent = vec![None::<(usize, usize)>; node_count];
    for start in 0..node_count {
        if color[start] != 0 {
            continue;
        }
        let mut stack = vec![(start, 0_usize)];
        color[start] = 1;
        while let Some((node, next_arc)) = stack.pop() {
            let outgoing = arcs
                .iter()
                .enumerate()
                .filter(|(_, arc)| arc.0 == node)
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if next_arc >= outgoing.len() {
                color[node] = 2;
                continue;
            }
            stack.push((node, next_arc + 1));
            let arc_index = outgoing[next_arc];
            let next = arcs[arc_index].1;
            if color[next] == 0 {
                color[next] = 1;
                parent[next] = Some((node, arc_index));
                stack.push((next, 0));
            } else if color[next] == 1 {
                let mut cycle = vec![arc_index];
                let mut cursor = node;
                while cursor != next {
                    let (previous, parent_arc) = parent[cursor]?;
                    cycle.push(parent_arc);
                    cursor = previous;
                }
                cycle.reverse();
                return Some(cycle);
            }
        }
    }
    None
}

fn bounded_f64(value: u64) -> Result<f64, AugmentingElectricalError> {
    u32::try_from(value)
        .map(f64::from)
        .map_err(|_| AugmentingElectricalError::AdmissionLimit)
}

fn bounded_i64(value: u64) -> Result<i64, AugmentingElectricalError> {
    i64::try_from(value).map_err(|_| AugmentingElectricalError::AdmissionLimit)
}

fn remaining(state: &KernelState) -> Result<f64, AugmentingElectricalError> {
    Ok((1.0 - state.alpha).max(0.0) * bounded_f64(state.working_target)?)
}

#[allow(clippy::too_many_lines)]
fn make_snapshot(
    graph: &FlowNetwork,
    state: &KernelState,
) -> Result<AugmentingElectricalSnapshot, AugmentingElectricalError> {
    if state
        .rounded_working_flows
        .as_ref()
        .is_some_and(|flows| flows.len() != state.edges.len())
    {
        return Err(AugmentingElectricalError::SourceInvariant);
    }
    if state
        .reduction_extraction
        .as_ref()
        .is_some_and(|extraction| {
            extraction.central_scaled.len() != graph.edges().len()
                || extraction.toward_source.len() != graph.edges().len()
                || extraction.out_of_sink.len() != graph.edges().len()
        })
    {
        return Err(AugmentingElectricalError::SourceInvariant);
    }
    let mut root_representative = vec![None::<usize>; state.roots.len()];
    let mut segment_counts = vec![0_u64; state.roots.len()];
    for (index, edge) in state.edges.iter().enumerate() {
        root_representative[edge.root].get_or_insert(index);
        segment_counts[edge.root] = segment_counts[edge.root]
            .checked_add(1)
            .ok_or(AugmentingElectricalError::WorkLimit)?;
    }
    let mut incident_gamma = vec![0.0_f64; graph.nodes().len()];
    for edge in &state.edges {
        if edge.from >= state.potentials.len() || edge.to >= state.potentials.len() {
            return Err(AugmentingElectricalError::NumericalFailure);
        }
        let residual = forward_residual(edge).min(backward_residual(edge));
        let gamma =
            ((state.potentials[edge.to] - state.potentials[edge.from]) - barrier_phi(edge)?).abs()
                * residual;
        if edge.from < incident_gamma.len() {
            incident_gamma[edge.from] = incident_gamma[edge.from].max(gamma);
        }
        if edge.to < incident_gamma.len() {
            incident_gamma[edge.to] = incident_gamma[edge.to].max(gamma);
        }
    }
    let nodes = graph
        .node_indices()
        .map(|node| {
            Ok(AugmentingElectricalNodeState {
                node,
                potential: AugmentingElectricalScalar::try_new(state.potentials[node.as_usize()])?,
                coupling_violation: AugmentingElectricalScalar::try_new(
                    incident_gamma[node.as_usize()],
                )?,
                target_source_side: state.target_source_side[node.as_usize()],
            })
        })
        .collect::<Result<Vec<_>, AugmentingElectricalError>>()?;
    let edges = graph
        .edges()
        .iter()
        .enumerate()
        .map(|(original, edge)| {
            let root = state
                .roots
                .iter()
                .position(|root| root.role == RootRole::OriginalH(original));
            let representative =
                root.and_then(|root| root_representative[root].map(|segment| (root, segment)));
            let (central, current, forward, backward, congestion, resistance, segments) =
                representative.map_or(
                    (0.0, 0.0, edge.capacity() as f64, 0.0, 0.0, 0.0, 0),
                    |(root, segment)| {
                        let working = &state.edges[segment];
                        let sign = working.root_sign;
                        (
                            sign * working.flow,
                            state.latest_currents.get(segment).copied().unwrap_or(0.0) * sign,
                            forward_residual(working),
                            backward_residual(working),
                            state
                                .latest_congestions
                                .get(segment)
                                .copied()
                                .unwrap_or(0.0)
                                .abs(),
                            state
                                .latest_resistances
                                .get(segment)
                                .copied()
                                .unwrap_or(0.0),
                            segment_counts[root],
                        )
                    },
                );
            Ok(AugmentingElectricalEdgeState {
                edge: edge.id().clone(),
                central_flow: AugmentingElectricalScalar::try_new(central)?,
                electrical_current: AugmentingElectricalScalar::try_new(current)?,
                forward_residual: AugmentingElectricalScalar::try_new(forward)?,
                backward_residual: AugmentingElectricalScalar::try_new(backward)?,
                congestion: AugmentingElectricalScalar::try_new(congestion)?,
                resistance: AugmentingElectricalScalar::try_new(resistance)?,
                boost_segments: segments,
                rounded_central_flow: state
                    .rounded_working_flows
                    .as_ref()
                    .and_then(|flows| root.and_then(|root| flows.get(root).copied())),
                extraction_central_scaled: state
                    .reduction_extraction
                    .as_ref()
                    .map(|extraction| {
                        u64::try_from(extraction.central_scaled[original])
                            .map_err(|_| AugmentingElectricalError::SourceInvariant)
                    })
                    .transpose()?,
                extraction_toward_source: state
                    .reduction_extraction
                    .as_ref()
                    .map(|extraction| {
                        u64::try_from(extraction.toward_source[original])
                            .map_err(|_| AugmentingElectricalError::SourceInvariant)
                    })
                    .transpose()?,
                extraction_out_of_sink: state
                    .reduction_extraction
                    .as_ref()
                    .map(|extraction| {
                        u64::try_from(extraction.out_of_sink[original])
                            .map_err(|_| AugmentingElectricalError::SourceInvariant)
                    })
                    .transpose()?,
                final_flow: state.final_flows[original],
            })
        })
        .collect::<Result<Vec<_>, AugmentingElectricalError>>()?;
    Ok(AugmentingElectricalSnapshot {
        stage: state.stage,
        original_target: state.original_target,
        transformed_target: state.transformed_target,
        working_target: state.working_target,
        current_value: AugmentingElectricalScalar::try_new(
            state.alpha * bounded_f64(state.working_target)?,
        )?,
        alpha: AugmentingElectricalScalar::try_new(state.alpha)?,
        remaining: AugmentingElectricalScalar::try_new(remaining(state)?)?,
        electrical_energy: AugmentingElectricalScalar::try_new(state.energy)?,
        congestion_l3: AugmentingElectricalScalar::try_new(state.l3)?,
        congestion_l4: AugmentingElectricalScalar::try_new(state.l4)?,
        coupling_l2: AugmentingElectricalScalar::try_new(state.coupling_l2)?,
        working_nodes: u64::try_from(state.potentials.len())
            .map_err(|_| AugmentingElectricalError::WorkLimit)?,
        working_edges: u64::try_from(state.edges.len())
            .map_err(|_| AugmentingElectricalError::WorkLimit)?,
        active_working_edge: state
            .active_edge
            .map(|edge| u64::try_from(edge).map_err(|_| AugmentingElectricalError::WorkLimit))
            .transpose()?,
        active_pivot_node: state
            .active_pivot_node
            .map(|node| u64::try_from(node).map_err(|_| AugmentingElectricalError::WorkLimit))
            .transpose()?,
        active_working_path: state
            .active_working_path
            .iter()
            .map(|&(edge, forward)| {
                let working = state
                    .edges
                    .get(edge)
                    .ok_or(AugmentingElectricalError::SourceInvariant)?;
                let (from, to) = if forward {
                    (working.from, working.to)
                } else {
                    (working.to, working.from)
                };
                let from =
                    NodeIndex::try_from_usize(from).ok_or(AugmentingElectricalError::WorkLimit)?;
                let to =
                    NodeIndex::try_from_usize(to).ok_or(AugmentingElectricalError::WorkLimit)?;
                let flow_after = state
                    .rounded_working_flows
                    .as_ref()
                    .and_then(|flows| flows.get(edge))
                    .copied()
                    .ok_or(AugmentingElectricalError::SourceInvariant)?;
                Ok(AugmentingElectricalWorkingArc {
                    edge: u64::try_from(edge).map_err(|_| AugmentingElectricalError::WorkLimit)?,
                    forward,
                    from,
                    to,
                    flow_after,
                })
            })
            .collect::<Result<Vec<_>, AugmentingElectricalError>>()?,
        active_extraction_cycle: state
            .active_extraction_cycle
            .iter()
            .map(|&arc| {
                let (edge, kind) = match arc {
                    ReductionArcKind::H(edge) => {
                        (edge, AugmentingElectricalExtractionArcKind::Central)
                    }
                    ReductionArcKind::SourceAux(edge) => {
                        (edge, AugmentingElectricalExtractionArcKind::TowardSource)
                    }
                    ReductionArcKind::SinkAux(edge) => {
                        (edge, AugmentingElectricalExtractionArcKind::OutOfSink)
                    }
                };
                Ok(AugmentingElectricalExtractionArc {
                    edge: u64::try_from(edge).map_err(|_| AugmentingElectricalError::WorkLimit)?,
                    kind,
                })
            })
            .collect::<Result<Vec<_>, AugmentingElectricalError>>()?,
        active_discrete_amount: state.active_discrete_amount,
        nodes,
        edges,
        metrics: state.metrics,
    })
}

#[cfg(test)]
mod tests {
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;

    fn graph(edges: &[(&str, &str, &str, u64)]) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let mut ids = vec!["s", "a", "b", "t"];
        ids.sort_unstable();
        let nodes = ids
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
            .collect();
        let declarations = edges
            .iter()
            .map(|&(id, from, to, capacity)| UnresolvedFlowEdge {
                id: EdgeId::parse(id).expect("edge"),
                from: NodeId::parse(from).expect("from"),
                to: NodeId::parse(to).expect("to"),
                lower: 0,
                capacity,
                cost: 0,
            })
            .collect();
        let graph = FlowNetwork::new(nodes, declarations).expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        (graph, source, sink)
    }

    fn graph_with_nodes(
        node_ids: &[&str],
        edges: &[(&str, &str, &str, u64)],
    ) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let nodes = node_ids
            .iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
            .collect();
        let declarations = edges
            .iter()
            .map(|&(id, from, to, capacity)| UnresolvedFlowEdge {
                id: EdgeId::parse(id).expect("edge"),
                from: NodeId::parse(from).expect("from"),
                to: NodeId::parse(to).expect("to"),
                lower: 0,
                capacity,
                cost: 0,
            })
            .collect();
        let graph = FlowNetwork::new(nodes, declarations).expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        (graph, source, sink)
    }

    #[test]
    fn solves_and_certifies_a_directed_network() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 5),
            ("sb", "s", "b", 4),
            ("ab", "a", "b", 2),
            ("at", "a", "t", 3),
            ("bt", "b", "t", 6),
        ]);
        let result = solve_augmenting_electrical_flow(&graph, source, sink).expect("solve");
        assert_eq!(result.certificate.value, 9);
        assert_eq!(
            result.final_snapshot.stage,
            AugmentingElectricalStage::Optimal
        );
        assert!(result.metrics.progress_steps > 0);
        assert!(result.metrics.fixing_steps > 0);
        assert!(result.metrics.electrical_solves >= result.metrics.progress_steps * 2);
    }

    #[test]
    fn rejects_the_default_workbench_network_before_entering_the_kernel() {
        let (graph, source, sink) = graph_with_nodes(
            &["s", "a", "b", "c", "d", "t"],
            &[
                ("sa", "s", "a", 12),
                ("sb", "s", "b", 8),
                ("ac", "a", "c", 9),
                ("ad", "a", "d", 4),
                ("bc", "b", "c", 3),
                ("bd", "b", "d", 7),
                ("ct", "c", "t", 10),
                ("dt", "d", "t", 11),
            ],
        );
        assert_eq!(
            trace_augmenting_electrical_flow(&graph, source, sink),
            Err(AugmentingElectricalError::AdmissionLimit)
        );
    }

    #[test]
    fn exposes_source_defined_boost_expansion() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 8),
            ("at", "a", "t", 8),
            ("sb", "s", "b", 1),
            ("bt", "b", "t", 1),
        ]);
        let result = solve_augmenting_electrical_flow(&graph, source, sink).expect("solve");
        assert!(result.metrics.boosts > 0);
        assert!(result.metrics.boost_vertices > 0);
        assert_eq!(result.certificate.value, 9);
    }

    #[test]
    fn trace_replays_and_matches_fast_result() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 3),
            ("at", "a", "t", 3),
            ("st", "s", "t", 2),
        ]);
        let fast = solve_augmenting_electrical_flow(&graph, source, sink).expect("fast");
        let trace = trace_augmenting_electrical_flow(&graph, source, sink).expect("trace");
        assert_eq!(trace.result, fast);
        assert_eq!(
            trace.events.first().map(|event| event.before.clone()),
            Some(trace.base_snapshot.clone())
        );
        assert_eq!(
            trace.events.last().map(|event| event.after.clone()),
            Some(trace.final_snapshot.clone())
        );
        let pivot_events = trace
            .events
            .iter()
            .filter(|event| event.after.stage == AugmentingElectricalStage::SolveElectricalPivot)
            .collect::<Vec<_>>();
        assert_eq!(
            u64::try_from(pivot_events.len()).expect("pivot event count"),
            fast.metrics.elimination_pivots
        );
        assert!(!pivot_events.is_empty());
        for event in pivot_events {
            assert_eq!(
                event.after.metrics.elimination_pivots,
                event.before.metrics.elimination_pivots + 1
            );
            assert!(event.after.active_pivot_node.is_some());
            assert_eq!(
                event.catalog_id,
                "augmenting-electrical-flow.elimination-pivot"
            );
        }
        let rounding = trace
            .events
            .iter()
            .find(|event| event.after.stage == AugmentingElectricalStage::RoundCentralFlow)
            .expect("central-flow rounding boundary");
        assert!(
            rounding
                .before
                .edges
                .iter()
                .all(|edge| edge.rounded_central_flow.is_none())
        );
        assert!(
            rounding
                .after
                .edges
                .iter()
                .all(|edge| edge.rounded_central_flow.is_some())
        );
        let extraction = trace
            .events
            .iter()
            .find(|event| event.after.stage == AugmentingElectricalStage::ExtractDirectedFlow)
            .expect("directed-reduction extraction boundary");
        assert!(extraction.before.edges.iter().all(|edge| {
            edge.extraction_central_scaled.is_none()
                && edge.extraction_toward_source.is_none()
                && edge.extraction_out_of_sink.is_none()
        }));
        assert!(extraction.after.edges.iter().all(|edge| {
            edge.extraction_central_scaled.is_some()
                && edge.extraction_toward_source.is_some()
                && edge.extraction_out_of_sink.is_some()
        }));
    }

    #[test]
    fn forged_trace_is_rejected() {
        let (graph, source, sink) = graph(&[("st", "s", "t", 2)]);
        let mut trace = trace_augmenting_electrical_flow(&graph, source, sink).expect("trace");
        trace.events[0].catalog_id = "augmenting-electrical-flow.optimal";
        assert_eq!(
            check_augmenting_electrical_trace(&graph, source, sink, &trace),
            Err(AugmentingElectricalError::TraceVerification)
        );
    }

    #[test]
    fn consistently_forged_ready_target_is_rejected() {
        let (graph, source, sink) = graph(&[("st", "s", "t", 2)]);
        let mut trace = trace_augmenting_electrical_flow(&graph, source, sink).expect("trace");
        trace.base_snapshot.original_target += 1;
        trace.events[0].before.original_target += 1;
        assert_eq!(
            check_augmenting_electrical_trace(&graph, source, sink, &trace),
            Err(AugmentingElectricalError::TraceVerification)
        );
    }

    #[test]
    fn partial_directed_extraction_projection_is_rejected() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 3),
            ("at", "a", "t", 3),
            ("st", "s", "t", 2),
        ]);
        let mut trace = trace_augmenting_electrical_flow(&graph, source, sink).expect("trace");
        let extraction = trace
            .events
            .iter_mut()
            .find(|event| event.after.stage == AugmentingElectricalStage::ExtractDirectedFlow)
            .expect("directed-reduction extraction boundary");
        extraction.after.edges[0].extraction_toward_source = None;
        assert_eq!(
            check_augmenting_electrical_trace(&graph, source, sink, &trace),
            Err(AugmentingElectricalError::TraceVerification)
        );
    }

    #[test]
    fn disconnected_instance_returns_certified_zero_flow() {
        let (graph, source, sink) = graph(&[("ab", "a", "b", 4)]);
        let result = solve_augmenting_electrical_flow(&graph, source, sink).expect("zero max");
        assert_eq!(result.certificate.value, 0);
        assert!(result.flows.iter().all(|&flow| flow == 0));
    }

    #[test]
    fn rejects_zero_capacity_and_self_loops() {
        let (zero, source, sink) = graph(&[("st", "s", "t", 0)]);
        assert_eq!(
            solve_augmenting_electrical_flow(&zero, source, sink),
            Err(AugmentingElectricalError::GraphRequirement)
        );
        let (self_loop, source, sink) = graph(&[("aa", "a", "a", 2)]);
        assert_eq!(
            solve_augmenting_electrical_flow(&self_loop, source, sink),
            Err(AugmentingElectricalError::GraphRequirement)
        );
    }
}
