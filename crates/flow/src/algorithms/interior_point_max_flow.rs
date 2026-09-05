//! Bounded path-following interior-point maximum flow from Mądry (FOCS 2013).
//!
//! The source algorithm reduces a unit-capacity directed maximum-flow instance
//! to perfect bipartite `b`-matching and then to an uncapacitated min-cost
//! demand-flow instance `G_b`.  It follows the central path with the associated
//! electrical flow whose resistance is `r_e = s_e / f_e`.  Each iteration has
//! the source-defined descent update and a second electrical centering update.
//!
//! This module implements that Section 4/5 kernel literally for small graphs.
//! Dense deterministic elimination replaces the source's approximate
//! Laplacian solver.  A bounded cut enumeration installs only the target value.
//! After the duality gap is below `1/2`, the source's Section 3 / Theorem 3.3
//! recovery is followed: split the fractional `b`-matching into unit-demand
//! copies, complete it with dummy vertices, take a perfect matching in that
//! support, augment the rounded matching to a perfect `b`-matching, and extract
//! the integral flow.  A deterministic support-matching routine replaces the
//! source's asymptotically fast regular-bipartite matching subroutine.
//! The implementation deliberately does not claim the paper's improved
//! `O~(m^(10/7))` end-to-end bound (or its nearly-linear electrical solves).

#![allow(clippy::cast_precision_loss)]

use num_traits::ToPrimitive;
use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{EdgeId, FlowNetwork, NodeIndex};

/// Original-node ceiling for exact cut enumeration and dense reduction solves.
pub const INTERIOR_POINT_MAX_FLOW_MAX_NODES: usize = 8;
/// Original-edge ceiling for the bounded source reductions and dense solves.
pub const INTERIOR_POINT_MAX_FLOW_MAX_EDGES: usize = 10;
/// Maximum reduced min-cost-flow vertices.
pub const INTERIOR_POINT_MAX_FLOW_MAX_WORKING_NODES: usize = 64;
/// Maximum reduced min-cost-flow arcs, including the hub arcs.
pub const INTERIOR_POINT_MAX_FLOW_MAX_WORKING_EDGES: usize = 192;
/// Deterministic central-path progress ceiling.
pub const INTERIOR_POINT_MAX_FLOW_MAX_PROGRESS_STEPS: u64 = 4_096;
/// Maximum public trace boundaries.
pub const INTERIOR_POINT_MAX_FLOW_MAX_TRACE_EVENTS: usize = 16_384;

const GAMMA_HAT: f64 = 1.0 / 400.0;
const POSITIVE_FLOOR: f64 = 1.0e-12;
const NUMERICAL_TOLERANCE: f64 = 2.0e-8;

/// Finite replay-safe scalar stored by exact IEEE-754 bit identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct InteriorPointScalar(u64);

impl InteriorPointScalar {
    fn try_new(value: f64) -> Result<Self, InteriorPointMaxFlowError> {
        if !value.is_finite() {
            return Err(InteriorPointMaxFlowError::NumericalFailure);
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

    /// Stable finite decimal for the scene contract.
    #[must_use]
    pub fn decimal(self) -> String {
        super::stable_scene_decimal(self.get())
    }
}

/// Source-level or bounded rounding boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteriorPointMaxFlowStage {
    /// Valid unit-capacity graph; no target or reduction has been published.
    Ready,
    /// Every bounded `s`-side cut was inspected and the minimum value installed.
    EnumerateTargetCut,
    /// The source `G -> G-bar` perfect `b`-matching reduction was built.
    BuildBMatchingReduction,
    /// The source `G-bar -> G_b` unit-length demand-flow reduction was built.
    BuildMinCostReduction,
    /// Lemma 5.4's explicit zero-centered `(f,s,nu)` state was installed.
    InitializeCentralPath,
    /// The associated electrical demand flow with `r=s/f` was solved.
    SolveElectricalDirection,
    /// One selected reduced Laplacian equation was eliminated.
    SolveElectricalPivot,
    /// Equations (38)--(40) advanced primal flow and dual slack.
    DescentStep,
    /// The electrical correction for equations (44)--(47) was solved.
    SolveCenteringDirection,
    /// Equation (48) restored the centered feasible state.
    CenteringStep,
    /// Direct matching arcs were projected back to fractional original flow.
    ExtractFractionalFlow,
    /// The source b-matching recovery produced an integral unit flow.
    RoundIntegralFlow,
    /// Feasibility, residual maximality, and the original cut were checked.
    CheckCertificate,
    /// Certified unit-capacity maximum flow is public.
    Optimal,
}

/// Original-node projection of reduced electrical potentials and the target cut.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteriorPointNodeState {
    /// Canonical original node.
    pub node: NodeIndex,
    /// Average corresponding `p_v/q_v` reduced potential, or zero at a terminal.
    pub potential: InteriorPointScalar,
    /// Whether the enumerated exact target cut contains this node.
    pub target_source_side: bool,
}

/// Original-edge projection of the bipartite direct arc and central-path state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteriorPointEdgeState {
    /// Stable original edge identity.
    pub edge: EdgeId,
    /// Flow on the direct `(p_e,q_e)` matching arc.
    pub fractional_flow: InteriorPointScalar,
    /// Latest associated or centering electrical current on that direct arc.
    pub electrical_current: InteriorPointScalar,
    /// Dual slack of the direct arc.
    pub slack: InteriorPointScalar,
    /// Arc measure `nu_e`.
    pub measure: InteriorPointScalar,
    /// Resistance `s_e/f_e`.
    pub resistance: InteriorPointScalar,
    /// Absolute electrical congestion `|f-hat_e|/f_e`.
    pub congestion: InteriorPointScalar,
    /// Whether this arc is ignored by the source's terminal normalization.
    pub normalized_away: bool,
    /// Final integral original flow after source b-matching recovery.
    pub final_flow: Option<u64>,
}

/// Exact bounded work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InteriorPointMaxFlowMetrics {
    /// Original cuts enumerated to install the exact target value.
    pub enumerated_cuts: u64,
    /// Vertices in the perfect `b`-matching reduction.
    pub b_matching_nodes: u64,
    /// Edges in the perfect `b`-matching reduction.
    pub b_matching_edges: u64,
    /// Vertices in the min-cost demand-flow graph `G_b`.
    pub working_nodes: u64,
    /// Arcs in the min-cost demand-flow graph `G_b`.
    pub working_edges: u64,
    /// Associated and centering electrical systems solved.
    pub electrical_solves: u64,
    /// Dense Gaussian-elimination pivots.
    pub elimination_pivots: u64,
    /// Completed descent steps.
    pub progress_steps: u64,
    /// Completed centering steps.
    pub centering_steps: u64,
    /// Arc scans performed by cycle cancellation and support-matching recovery.
    pub recovery_arc_scans: u64,
    /// Independent final certificate checks.
    pub certificate_checks: u64,
    /// Public state transitions.
    pub state_transitions: u64,
}

/// Complete reversible public state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteriorPointMaxFlowSnapshot {
    /// Original-node projection.
    pub nodes: Vec<InteriorPointNodeState>,
    /// Original-edge projection.
    pub edges: Vec<InteriorPointEdgeState>,
    /// Exact bounded minimum-cut value.
    pub target_value: u64,
    /// Current weighted mean complementarity `mu-hat`.
    pub mu: InteriorPointScalar,
    /// Current primal-dual gap `sum f_e s_e`.
    pub duality_gap: InteriorPointScalar,
    /// Weighted centrality norm divided by `mu`.
    pub centrality: InteriorPointScalar,
    /// Latest weighted congestion four-norm.
    pub congestion_l4: InteriorPointScalar,
    /// Latest safe descent fraction.
    pub step_size: InteriorPointScalar,
    /// Latest electrical energy.
    pub electrical_energy: InteriorPointScalar,
    /// Source reduction vertex count.
    pub b_matching_nodes: u64,
    /// Source reduction edge count.
    pub b_matching_edges: u64,
    /// `G_b` vertex count.
    pub working_nodes: u64,
    /// `G_b` arc count.
    pub working_edges: u64,
    /// Active reduced arc ordinal, when one direct arc is highlighted.
    pub active_working_edge: Option<u64>,
    /// Active reduced Laplacian row during dense elimination.
    pub active_pivot_node: Option<u64>,
    /// Original nodes represented by the active reduced pivot row.
    pub active_pivot_original_nodes: Vec<NodeIndex>,
    /// Original edges whose reduction gadget touches the active pivot row.
    pub active_pivot_original_edges: Vec<EdgeId>,
    /// Source or bounded rounding stage.
    pub stage: InteriorPointMaxFlowStage,
    /// Exact work counters.
    pub metrics: InteriorPointMaxFlowMetrics,
}

/// One atomic reversible transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteriorPointMaxFlowTraceEvent {
    /// Stable event vocabulary identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: InteriorPointMaxFlowSnapshot,
    /// State after the transition.
    pub after: InteriorPointMaxFlowSnapshot,
}

/// Certified bounded solver result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteriorPointMaxFlowResult {
    /// Original-edge integral flows.
    pub flows: Vec<u64>,
    /// Independent maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Terminal public state.
    pub final_snapshot: InteriorPointMaxFlowSnapshot,
    /// Exact work counters.
    pub metrics: InteriorPointMaxFlowMetrics,
}

/// Result plus every source-level public boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteriorPointMaxFlowTraceResult {
    /// Same certified result as the fast profile.
    pub result: InteriorPointMaxFlowResult,
    /// Ready state.
    pub base_snapshot: InteriorPointMaxFlowSnapshot,
    /// Canonical reversible transitions.
    pub events: Vec<InteriorPointMaxFlowTraceEvent>,
    /// Terminal state, equal to `result.final_snapshot`.
    pub final_snapshot: InteriorPointMaxFlowSnapshot,
}

/// Admission, graph-contract, numerical, resource, rounding, or certificate failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum InteriorPointMaxFlowError {
    /// Input exceeds the deliberately small interactive band.
    #[error("interior-point-max-flow input exceeds admission limits")]
    AdmissionLimit,
    /// The source algorithm requires a simple unit-capacity max-flow model.
    #[error(
        "interior-point-max-flow requires unit capacities, zero lower bounds/costs/supplies, and no self-loops"
    )]
    GraphRequirement,
    /// Terminals are missing or equal.
    #[error("interior-point-max-flow terminals are invalid")]
    InvalidTerminals,
    /// The source reductions exceed their explicit bounded working band.
    #[error("interior-point-max-flow reduced instance exceeds working limits")]
    ReductionLimit,
    /// Dense elimination or a positivity/centrality invariant failed.
    #[error("interior-point-max-flow numerical invariant failed")]
    NumericalFailure,
    /// The deterministic central-path ceiling was reached.
    #[error("interior-point-max-flow central path did not converge within the bounded ceiling")]
    NonConvergence,
    /// No integral flow at the independently installed target exists.
    #[error("interior-point-max-flow b-matching recovery failed")]
    RoundingFailure,
    /// Independent maximum-flow/minimum-cut certification failed.
    #[error("interior-point-max-flow certificate failed")]
    CertificateFailure,
    /// Supplied trace differs from deterministic replay.
    #[error("interior-point-max-flow trace verification failed")]
    TraceVerification,
}

impl From<CertificateError> for InteriorPointMaxFlowError {
    fn from(_: CertificateError) -> Self {
        Self::CertificateFailure
    }
}

/// Solves one bounded unit-capacity instance with the source Section 4/5 kernel.
///
/// # Errors
///
/// Rejects unsupported or oversized input, reduction overflow, numerical
/// invariant failure, non-convergence, failed rounding, or failed certificate.
pub fn solve_interior_point_max_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<InteriorPointMaxFlowResult, InteriorPointMaxFlowError> {
    run_internal(graph, source, sink, false).map(|run| run.result)
}

/// Records every reduction, electrical, descent, centering, rounding, and check boundary.
///
/// # Errors
///
/// Returns the same failures as [`solve_interior_point_max_flow`] or replay failure.
pub fn trace_interior_point_max_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<InteriorPointMaxFlowTraceResult, InteriorPointMaxFlowError> {
    let run = run_internal(graph, source, sink, true)?;
    let trace = InteriorPointMaxFlowTraceResult {
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
    };
    check_interior_point_max_flow_trace(graph, source, sink, &trace)?;
    Ok(trace)
}

/// Independently checks every public boundary and terminal certificate.
///
/// # Errors
///
/// Rejects altered, omitted, reordered, or disconnected events.
pub fn check_interior_point_max_flow_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: &InteriorPointMaxFlowTraceResult,
) -> Result<(), InteriorPointMaxFlowError> {
    validate_graph(graph, source, sink)?;
    if !valid_interior_point_base(graph, source, sink, &trace.base_snapshot)
        || trace.final_snapshot.stage != InteriorPointMaxFlowStage::Optimal
        || trace.final_snapshot != trace.result.final_snapshot
        || trace.final_snapshot.metrics != trace.result.metrics
        || trace.events.is_empty()
        || trace.events.len() > INTERIOR_POINT_MAX_FLOW_MAX_TRACE_EVENTS
    {
        return Err(InteriorPointMaxFlowError::TraceVerification);
    }
    let mut cursor = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != cursor
            || !valid_interior_point_event(event)
            || event.after.metrics.state_transitions
                != event.before.metrics.state_transitions.saturating_add(1)
            || event.after.nodes.len() != graph.nodes().len()
            || event.after.edges.len() != graph.edges().len()
            || event.after.nodes.iter().enumerate().any(|(index, state)| {
                state.node.as_usize() != index || !state.potential.get().is_finite()
            })
            || event
                .after
                .edges
                .iter()
                .zip(graph.edges())
                .any(|(state, edge)| {
                    &state.edge != edge.id()
                        || !state.fractional_flow.get().is_finite()
                        || !state.electrical_current.get().is_finite()
                        || !state.slack.get().is_finite()
                        || !state.measure.get().is_finite()
                        || !state.resistance.get().is_finite()
                        || !state.congestion.get().is_finite()
                        || state.final_flow.is_some_and(|flow| flow > edge.capacity())
                })
        {
            return Err(InteriorPointMaxFlowError::TraceVerification);
        }
        cursor = &event.after;
    }
    if cursor != &trace.final_snapshot
        || trace.events.len() as u64 != trace.final_snapshot.metrics.state_transitions
        || validate_terminal(graph, source, sink, &trace.result).is_err()
    {
        return Err(InteriorPointMaxFlowError::TraceVerification);
    }
    Ok(())
}

fn valid_interior_point_base(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    snapshot: &InteriorPointMaxFlowSnapshot,
) -> bool {
    let scalar_is_zero = |value: InteriorPointScalar| value.get() == 0.0;
    snapshot.stage == InteriorPointMaxFlowStage::Ready
        && snapshot.target_value == 0
        && scalar_is_zero(snapshot.mu)
        && scalar_is_zero(snapshot.duality_gap)
        && scalar_is_zero(snapshot.centrality)
        && scalar_is_zero(snapshot.congestion_l4)
        && scalar_is_zero(snapshot.step_size)
        && scalar_is_zero(snapshot.electrical_energy)
        && snapshot.b_matching_nodes == 0
        && snapshot.b_matching_edges == 0
        && snapshot.working_nodes == 0
        && snapshot.working_edges == 0
        && snapshot.active_working_edge.is_none()
        && snapshot.active_pivot_node.is_none()
        && snapshot.active_pivot_original_nodes.is_empty()
        && snapshot.active_pivot_original_edges.is_empty()
        && snapshot.metrics == InteriorPointMaxFlowMetrics::default()
        && snapshot.nodes.len() == graph.nodes().len()
        && snapshot.nodes.iter().enumerate().all(|(index, node)| {
            node.node.as_usize() == index
                && scalar_is_zero(node.potential)
                && !node.target_source_side
        })
        && snapshot.edges.len() == graph.edges().len()
        && snapshot
            .edges
            .iter()
            .zip(graph.edges())
            .all(|(state, edge)| {
                &state.edge == edge.id()
                    && scalar_is_zero(state.fractional_flow)
                    && scalar_is_zero(state.electrical_current)
                    && scalar_is_zero(state.slack)
                    && scalar_is_zero(state.measure)
                    && scalar_is_zero(state.resistance)
                    && scalar_is_zero(state.congestion)
                    && state.normalized_away == (edge.to() == source || edge.from() == sink)
                    && state.final_flow.is_none()
            })
}

#[allow(clippy::too_many_lines, clippy::unnested_or_patterns)]
fn valid_interior_point_event(event: &InteriorPointMaxFlowTraceEvent) -> bool {
    let catalog_matches_stage = matches!(
        (event.catalog_id, event.after.stage),
        (
            "ipm.enumerate-target-cut",
            InteriorPointMaxFlowStage::EnumerateTargetCut
        ) | (
            "ipm.build-b-matching-reduction",
            InteriorPointMaxFlowStage::BuildBMatchingReduction
        ) | (
            "ipm.build-min-cost-reduction",
            InteriorPointMaxFlowStage::BuildMinCostReduction
        ) | (
            "ipm.initialize-zero-centered",
            InteriorPointMaxFlowStage::InitializeCentralPath
        ) | (
            "ipm.elimination-pivot",
            InteriorPointMaxFlowStage::SolveElectricalPivot
        ) | (
            "ipm.solve-associated-electrical",
            InteriorPointMaxFlowStage::SolveElectricalDirection
        ) | ("ipm.descent", InteriorPointMaxFlowStage::DescentStep)
            | (
                "ipm.solve-centering-electrical",
                InteriorPointMaxFlowStage::SolveCenteringDirection
            )
            | ("ipm.center", InteriorPointMaxFlowStage::CenteringStep)
            | (
                "ipm.extract-fractional-flow",
                InteriorPointMaxFlowStage::ExtractFractionalFlow
            )
            | (
                "ipm.round-integral-flow",
                InteriorPointMaxFlowStage::RoundIntegralFlow
            )
            | (
                "ipm.check-certificate",
                InteriorPointMaxFlowStage::CheckCertificate
            )
            | ("ipm.optimal", InteriorPointMaxFlowStage::Optimal)
    );
    let stage_transition = matches!(
        (event.before.stage, event.after.stage),
        (
            InteriorPointMaxFlowStage::Ready,
            InteriorPointMaxFlowStage::EnumerateTargetCut
        ) | (
            InteriorPointMaxFlowStage::EnumerateTargetCut,
            InteriorPointMaxFlowStage::BuildBMatchingReduction
        ) | (
            InteriorPointMaxFlowStage::BuildBMatchingReduction,
            InteriorPointMaxFlowStage::BuildMinCostReduction
        ) | (
            InteriorPointMaxFlowStage::BuildMinCostReduction,
            InteriorPointMaxFlowStage::InitializeCentralPath
        ) | (
            InteriorPointMaxFlowStage::InitializeCentralPath
                | InteriorPointMaxFlowStage::CenteringStep,
            InteriorPointMaxFlowStage::SolveElectricalPivot
                | InteriorPointMaxFlowStage::SolveElectricalDirection
        ) | (
            InteriorPointMaxFlowStage::SolveElectricalPivot,
            InteriorPointMaxFlowStage::SolveElectricalPivot
                | InteriorPointMaxFlowStage::SolveElectricalDirection
                | InteriorPointMaxFlowStage::SolveCenteringDirection
        ) | (
            InteriorPointMaxFlowStage::SolveElectricalDirection,
            InteriorPointMaxFlowStage::DescentStep
        ) | (
            InteriorPointMaxFlowStage::DescentStep,
            InteriorPointMaxFlowStage::SolveElectricalPivot
                | InteriorPointMaxFlowStage::SolveCenteringDirection
        ) | (
            InteriorPointMaxFlowStage::SolveCenteringDirection,
            InteriorPointMaxFlowStage::CenteringStep
        ) | (
            InteriorPointMaxFlowStage::InitializeCentralPath
                | InteriorPointMaxFlowStage::CenteringStep,
            InteriorPointMaxFlowStage::ExtractFractionalFlow
        ) | (
            InteriorPointMaxFlowStage::ExtractFractionalFlow,
            InteriorPointMaxFlowStage::RoundIntegralFlow
        ) | (
            InteriorPointMaxFlowStage::RoundIntegralFlow,
            InteriorPointMaxFlowStage::CheckCertificate
        ) | (
            InteriorPointMaxFlowStage::CheckCertificate,
            InteriorPointMaxFlowStage::Optimal
        )
    );
    let pivot_stage = event.after.stage == InteriorPointMaxFlowStage::SolveElectricalPivot;
    let pivot_scope = pivot_stage
        == (event.after.active_pivot_node.is_some()
            && (!event.after.active_pivot_original_nodes.is_empty()
                || !event.after.active_pivot_original_edges.is_empty()))
        && (pivot_stage
            || (event.after.active_pivot_original_nodes.is_empty()
                && event.after.active_pivot_original_edges.is_empty()));
    let pivot_metric = if pivot_stage {
        event.after.metrics.elimination_pivots
            == event.before.metrics.elimination_pivots.saturating_add(1)
    } else {
        event.after.metrics.elimination_pivots == event.before.metrics.elimination_pivots
    };
    catalog_matches_stage && stage_transition && pivot_scope && pivot_metric
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BipartiteRole {
    OriginalP(usize),
    OriginalQ(usize),
    InternalP(usize),
    InternalQ(usize),
    SourceQ(usize),
    SinkP(usize),
}

#[derive(Clone, Debug)]
struct BipartiteNode {
    demand: u64,
    role: BipartiteRole,
}

#[derive(Clone, Debug)]
struct BipartiteEdge {
    p: usize,
    q: usize,
    original_direct: Option<usize>,
}

#[derive(Clone, Debug)]
struct BipartiteReduction {
    p: Vec<BipartiteNode>,
    q: Vec<BipartiteNode>,
    edges: Vec<BipartiteEdge>,
    original_direct_edge: Vec<Option<usize>>,
}

#[derive(Clone, Debug)]
struct WorkingArc {
    from: usize,
    to: usize,
    flow: f64,
    slack: f64,
    measure: f64,
}

#[derive(Clone, Debug)]
struct WorkingReduction {
    node_count: usize,
    hub: usize,
    arcs: Vec<WorkingArc>,
    reduction_edge_arcs: Vec<Vec<usize>>,
    original_direct_arc: Vec<Option<usize>>,
    original_node_projection: Vec<Vec<usize>>,
    node_roles: Vec<Option<BipartiteRole>>,
}

type AssociatedElectricalSolution = (Vec<f64>, Vec<f64>, f64, Vec<usize>);

struct CenteringDirection {
    zero_flow: Vec<f64>,
    correction: Vec<f64>,
    potentials: Vec<f64>,
    energy: f64,
    pivot_nodes: Vec<usize>,
}

#[derive(Clone, Debug)]
struct KernelState {
    target: u64,
    target_source_side: Vec<bool>,
    normalized_away: Vec<bool>,
    reduction: Option<BipartiteReduction>,
    working: Option<WorkingReduction>,
    potentials: Vec<f64>,
    latest_currents: Vec<f64>,
    latest_congestions: Vec<f64>,
    extracted_original_flow: Vec<f64>,
    mu: f64,
    duality_gap: f64,
    centrality: f64,
    congestion_l4: f64,
    step_size: f64,
    energy: f64,
    final_flows: Vec<Option<u64>>,
    active_working_edge: Option<usize>,
    active_pivot_node: Option<usize>,
    stage: InteriorPointMaxFlowStage,
    metrics: InteriorPointMaxFlowMetrics,
}

struct InternalRun {
    result: InteriorPointMaxFlowResult,
    base_snapshot: InteriorPointMaxFlowSnapshot,
    events: Vec<InteriorPointMaxFlowTraceEvent>,
}

struct Recorder<'a> {
    graph: &'a FlowNetwork,
    state: KernelState,
    current: InteriorPointMaxFlowSnapshot,
    events: Vec<InteriorPointMaxFlowTraceEvent>,
    enabled: bool,
}

impl Recorder<'_> {
    fn emit(
        &mut self,
        catalog_id: &'static str,
        stage: InteriorPointMaxFlowStage,
    ) -> Result<(), InteriorPointMaxFlowError> {
        self.state.stage = stage;
        self.state.active_pivot_node = None;
        self.commit_event(catalog_id)
    }

    fn emit_elimination_pivots(
        &mut self,
        pivot_nodes: &[usize],
    ) -> Result<(), InteriorPointMaxFlowError> {
        for &pivot_node in pivot_nodes {
            self.state.stage = InteriorPointMaxFlowStage::SolveElectricalPivot;
            self.state.active_working_edge = None;
            self.state.active_pivot_node = Some(pivot_node);
            self.state.metrics.elimination_pivots = self
                .state
                .metrics
                .elimination_pivots
                .checked_add(1)
                .ok_or(InteriorPointMaxFlowError::NumericalFailure)?;
            self.commit_event("ipm.elimination-pivot")?;
        }
        Ok(())
    }

    fn commit_event(&mut self, catalog_id: &'static str) -> Result<(), InteriorPointMaxFlowError> {
        self.state.metrics.state_transitions = self
            .state
            .metrics
            .state_transitions
            .checked_add(1)
            .ok_or(InteriorPointMaxFlowError::NumericalFailure)?;
        let before = self.current.clone();
        let after = project_snapshot(self.graph, &self.state)?;
        if self.enabled {
            if self.events.len() >= INTERIOR_POINT_MAX_FLOW_MAX_TRACE_EVENTS {
                return Err(InteriorPointMaxFlowError::AdmissionLimit);
            }
            self.events.push(InteriorPointMaxFlowTraceEvent {
                catalog_id,
                before,
                after: after.clone(),
            });
        }
        self.current = after;
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
fn run_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    record_events: bool,
) -> Result<InternalRun, InteriorPointMaxFlowError> {
    validate_graph(graph, source, sink)?;
    let zero = InteriorPointScalar::try_new(0.0)?;
    let state = KernelState {
        target: 0,
        target_source_side: vec![false; graph.nodes().len()],
        normalized_away: graph
            .edges()
            .iter()
            .map(|edge| edge.to() == source || edge.from() == sink)
            .collect(),
        reduction: None,
        working: None,
        potentials: Vec::new(),
        latest_currents: Vec::new(),
        latest_congestions: Vec::new(),
        extracted_original_flow: vec![0.0; graph.edges().len()],
        mu: 0.0,
        duality_gap: 0.0,
        centrality: 0.0,
        congestion_l4: 0.0,
        step_size: 0.0,
        energy: 0.0,
        final_flows: vec![None; graph.edges().len()],
        active_working_edge: None,
        active_pivot_node: None,
        stage: InteriorPointMaxFlowStage::Ready,
        metrics: InteriorPointMaxFlowMetrics::default(),
    };
    let base_snapshot = InteriorPointMaxFlowSnapshot {
        nodes: graph
            .node_indices()
            .map(|node| InteriorPointNodeState {
                node,
                potential: zero,
                target_source_side: false,
            })
            .collect(),
        edges: graph
            .edges()
            .iter()
            .map(|edge| InteriorPointEdgeState {
                edge: edge.id().clone(),
                fractional_flow: zero,
                electrical_current: zero,
                slack: zero,
                measure: zero,
                resistance: zero,
                congestion: zero,
                normalized_away: edge.to() == source || edge.from() == sink,
                final_flow: None,
            })
            .collect(),
        target_value: 0,
        mu: zero,
        duality_gap: zero,
        centrality: zero,
        congestion_l4: zero,
        step_size: zero,
        electrical_energy: zero,
        b_matching_nodes: 0,
        b_matching_edges: 0,
        working_nodes: 0,
        working_edges: 0,
        active_working_edge: None,
        active_pivot_node: None,
        active_pivot_original_nodes: Vec::new(),
        active_pivot_original_edges: Vec::new(),
        stage: InteriorPointMaxFlowStage::Ready,
        metrics: InteriorPointMaxFlowMetrics::default(),
    };
    let mut recorder = Recorder {
        graph,
        state,
        current: base_snapshot.clone(),
        events: Vec::new(),
        enabled: record_events,
    };

    let (target, source_side, cut_count) = enumerate_min_cut(graph, source, sink)?;
    recorder.state.target = target;
    recorder.state.target_source_side = source_side;
    recorder.state.metrics.enumerated_cuts = cut_count;
    recorder.emit(
        "ipm.enumerate-target-cut",
        InteriorPointMaxFlowStage::EnumerateTargetCut,
    )?;

    let reduction = build_bipartite_reduction(graph, source, sink, target)?;
    recorder.state.metrics.b_matching_nodes = u64::try_from(reduction.p.len() + reduction.q.len())
        .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?;
    recorder.state.metrics.b_matching_edges = u64::try_from(reduction.edges.len())
        .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?;
    recorder.state.reduction = Some(reduction);
    recorder.emit(
        "ipm.build-b-matching-reduction",
        InteriorPointMaxFlowStage::BuildBMatchingReduction,
    )?;

    let working = build_working_reduction(
        graph,
        recorder
            .state
            .reduction
            .as_ref()
            .ok_or(InteriorPointMaxFlowError::ReductionLimit)?,
    )?;
    recorder.state.metrics.working_nodes =
        u64::try_from(working.node_count).map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?;
    recorder.state.metrics.working_edges =
        u64::try_from(working.arcs.len()).map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?;
    recorder.state.potentials = vec![0.0; working.node_count];
    recorder.state.latest_currents = vec![0.0; working.arcs.len()];
    recorder.state.latest_congestions = vec![0.0; working.arcs.len()];
    recorder.state.working = Some(working);
    recorder.emit(
        "ipm.build-min-cost-reduction",
        InteriorPointMaxFlowStage::BuildMinCostReduction,
    )?;

    update_path_metrics(&mut recorder.state)?;
    if (recorder.state.mu - 1.0).abs() > NUMERICAL_TOLERANCE
        || recorder.state.centrality > NUMERICAL_TOLERANCE
    {
        return Err(InteriorPointMaxFlowError::NumericalFailure);
    }
    recorder.emit(
        "ipm.initialize-zero-centered",
        InteriorPointMaxFlowStage::InitializeCentralPath,
    )?;

    let working_edges = recorder
        .state
        .working
        .as_ref()
        .ok_or(InteriorPointMaxFlowError::ReductionLimit)?
        .arcs
        .len();
    let gap_threshold = 0.5;
    while recorder.state.duality_gap > gap_threshold + NUMERICAL_TOLERANCE {
        if recorder.state.metrics.progress_steps >= INTERIOR_POINT_MAX_FLOW_MAX_PROGRESS_STEPS {
            return Err(InteriorPointMaxFlowError::NonConvergence);
        }
        let (electrical, potentials, energy, pivot_nodes) = {
            let working = recorder
                .state
                .working
                .as_ref()
                .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
            solve_associated_electrical(working, None)?
        };
        recorder.emit_elimination_pivots(&pivot_nodes)?;
        recorder.state.metrics.electrical_solves += 1;
        recorder.state.potentials = potentials;
        recorder.state.energy = energy;
        recorder.state.latest_currents.clone_from(&electrical);
        let (congestions, l4, active) = {
            let working = recorder
                .state
                .working
                .as_ref()
                .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
            congestion_state(working, &electrical)?
        };
        recorder.state.latest_congestions = congestions;
        recorder.state.congestion_l4 = l4;
        recorder.state.active_working_edge = active;
        recorder.emit(
            "ipm.solve-associated-electrical",
            InteriorPointMaxFlowStage::SolveElectricalDirection,
        )?;

        let delta = (GAMMA_HAT.sqrt() / l4.max(POSITIVE_FLOOR)).min(0.5);
        if !(delta > 0.0 && delta <= 0.5 && delta.is_finite()) {
            return Err(InteriorPointMaxFlowError::NumericalFailure);
        }
        recorder.state.step_size = delta;
        {
            let working = recorder
                .state
                .working
                .as_mut()
                .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
            for (index, arc) in working.arcs.iter_mut().enumerate() {
                let old_flow = arc.flow;
                let old_slack = arc.slack;
                let current = electrical[index];
                arc.flow = (1.0 - delta) * old_flow + delta * current;
                arc.slack = old_slack - delta / (1.0 - delta) * (old_slack / old_flow) * current;
                if arc.flow <= POSITIVE_FLOOR || arc.slack <= POSITIVE_FLOOR {
                    return Err(InteriorPointMaxFlowError::NumericalFailure);
                }
            }
        }
        recorder.state.metrics.progress_steps += 1;
        update_path_metrics(&mut recorder.state)?;
        recorder.emit("ipm.descent", InteriorPointMaxFlowStage::DescentStep)?;

        let centering = {
            let working = recorder
                .state
                .working
                .as_ref()
                .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
            centering_direction(working, recorder.state.mu)?
        };
        recorder.emit_elimination_pivots(&centering.pivot_nodes)?;
        recorder.state.metrics.electrical_solves += 1;
        recorder.state.potentials = centering.potentials;
        recorder
            .state
            .latest_currents
            .clone_from(&centering.correction);
        recorder.state.energy = centering.energy;
        recorder.state.active_working_edge = centering
            .correction
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.abs().total_cmp(&right.1.abs()))
            .map(|(index, _)| index);
        recorder.emit(
            "ipm.solve-centering-electrical",
            InteriorPointMaxFlowStage::SolveCenteringDirection,
        )?;

        {
            let working = recorder
                .state
                .working
                .as_mut()
                .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
            if centering.zero_flow.len() != working_edges
                || centering.correction.len() != working_edges
            {
                return Err(InteriorPointMaxFlowError::NumericalFailure);
            }
            for (index, arc) in working.arcs.iter_mut().enumerate() {
                let resistance = arc.slack / centering.zero_flow[index];
                arc.flow = centering.zero_flow[index] + centering.correction[index];
                arc.slack -= resistance * centering.correction[index];
                if arc.flow <= POSITIVE_FLOOR || arc.slack <= POSITIVE_FLOOR {
                    return Err(InteriorPointMaxFlowError::NumericalFailure);
                }
            }
        }
        recorder.state.metrics.centering_steps += 1;
        update_path_metrics(&mut recorder.state)?;
        if recorder.state.centrality > GAMMA_HAT + 1.0e-7 {
            return Err(InteriorPointMaxFlowError::NumericalFailure);
        }
        recorder.emit("ipm.center", InteriorPointMaxFlowStage::CenteringStep)?;
    }

    let (acyclic_flow, cycle_work) = cancel_working_flow_cycles(
        recorder
            .state
            .working
            .as_ref()
            .ok_or(InteriorPointMaxFlowError::ReductionLimit)?,
    )?;
    let fractional = fractional_b_matching(&recorder.state, &acyclic_flow)?;
    let reduction = recorder
        .state
        .reduction
        .clone()
        .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
    recorder.state.extracted_original_flow = reduction
        .original_direct_edge
        .iter()
        .map(|edge| edge.map_or(0.0, |index| fractional[index]))
        .collect();
    recorder.state.active_working_edge = None;
    recorder.state.latest_currents.fill(0.0);
    recorder.state.latest_congestions.fill(0.0);
    recorder.state.congestion_l4 = 0.0;
    recorder.state.step_size = 0.0;
    recorder.state.energy = 0.0;
    recorder.emit(
        "ipm.extract-fractional-flow",
        InteriorPointMaxFlowStage::ExtractFractionalFlow,
    )?;

    let (matching, rounding_work) = round_and_complete_b_matching(&reduction, &fractional)?;
    let flows = extract_integral_flow(graph, &reduction, &matching)?;
    recorder.state.metrics.recovery_arc_scans = cycle_work
        .checked_add(rounding_work)
        .ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
    recorder.state.final_flows = flows.iter().copied().map(Some).collect();
    recorder.emit(
        "ipm.round-integral-flow",
        InteriorPointMaxFlowStage::RoundIntegralFlow,
    )?;

    let certificate = check_max_flow(graph, source, sink, &flows)?;
    if certificate.value != i128::from(target) || certificate.cut_bound != i128::from(target) {
        return Err(InteriorPointMaxFlowError::CertificateFailure);
    }
    recorder.state.metrics.certificate_checks += 1;
    recorder.emit(
        "ipm.check-certificate",
        InteriorPointMaxFlowStage::CheckCertificate,
    )?;
    recorder.emit("ipm.optimal", InteriorPointMaxFlowStage::Optimal)?;

    let final_snapshot = recorder.current.clone();
    let result = InteriorPointMaxFlowResult {
        flows,
        certificate,
        final_snapshot,
        metrics: recorder.state.metrics,
    };
    validate_terminal(graph, source, sink, &result)?;
    Ok(InternalRun {
        result,
        base_snapshot,
        events: recorder.events,
    })
}

fn validate_graph(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), InteriorPointMaxFlowError> {
    if graph.nodes().len() > INTERIOR_POINT_MAX_FLOW_MAX_NODES
        || graph.edges().len() > INTERIOR_POINT_MAX_FLOW_MAX_EDGES
    {
        return Err(InteriorPointMaxFlowError::AdmissionLimit);
    }
    if source == sink
        || source.as_usize() >= graph.nodes().len()
        || sink.as_usize() >= graph.nodes().len()
    {
        return Err(InteriorPointMaxFlowError::InvalidTerminals);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0)
        || graph.edges().iter().any(|edge| {
            edge.lower() != 0
                || edge.capacity() != 1
                || edge.cost() != 0
                || edge.from() == edge.to()
        })
    {
        return Err(InteriorPointMaxFlowError::GraphRequirement);
    }
    Ok(())
}

fn enumerate_min_cut(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(u64, Vec<bool>, u64), InteriorPointMaxFlowError> {
    let free = graph
        .node_indices()
        .filter(|&node| node != source && node != sink)
        .collect::<Vec<_>>();
    let masks = 1_u64
        .checked_shl(
            u32::try_from(free.len()).map_err(|_| InteriorPointMaxFlowError::AdmissionLimit)?,
        )
        .ok_or(InteriorPointMaxFlowError::AdmissionLimit)?;
    let mut best = u64::MAX;
    let mut best_side = vec![false; graph.nodes().len()];
    let mut count = 0_u64;
    for mask in 0..masks {
        let mut side = vec![false; graph.nodes().len()];
        side[source.as_usize()] = true;
        for (bit, node) in free.iter().enumerate() {
            side[node.as_usize()] = mask & (1_u64 << bit) != 0;
        }
        let mut cut = 0_u64;
        for edge in graph.edges() {
            if side[edge.from().as_usize()] && !side[edge.to().as_usize()] {
                cut = cut
                    .checked_add(edge.capacity())
                    .ok_or(InteriorPointMaxFlowError::NumericalFailure)?;
            }
        }
        count += 1;
        if cut < best {
            best = cut;
            best_side = side;
        }
    }
    Ok((best, best_side, count))
}

#[allow(clippy::too_many_lines)]
fn build_bipartite_reduction(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    target: u64,
) -> Result<BipartiteReduction, InteriorPointMaxFlowError> {
    let mut p = Vec::new();
    let mut q = Vec::new();
    let mut p_edge = vec![None; graph.edges().len()];
    let mut q_edge = vec![None; graph.edges().len()];
    let mut p_vertex = vec![None; graph.nodes().len()];
    let mut q_vertex = vec![None; graph.nodes().len()];
    let relevant = graph
        .edges()
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.to() != source && edge.from() != sink)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    for &edge in &relevant {
        p_edge[edge] = Some(p.len());
        p.push(BipartiteNode {
            demand: 1,
            role: BipartiteRole::OriginalP(edge),
        });
        q_edge[edge] = Some(q.len());
        q.push(BipartiteNode {
            demand: 1,
            role: BipartiteRole::OriginalQ(edge),
        });
    }
    for node in graph.node_indices() {
        if node == source || node == sink {
            continue;
        }
        let incoming = relevant
            .iter()
            .filter(|&&edge| graph.edges()[edge].to() == node)
            .count();
        let outgoing = relevant
            .iter()
            .filter(|&&edge| graph.edges()[edge].from() == node)
            .count();
        p_vertex[node.as_usize()] = Some(p.len());
        p.push(BipartiteNode {
            demand: u64::try_from(incoming)
                .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?,
            role: BipartiteRole::InternalP(node.as_usize()),
        });
        q_vertex[node.as_usize()] = Some(q.len());
        q.push(BipartiteNode {
            demand: u64::try_from(outgoing)
                .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?,
            role: BipartiteRole::InternalQ(node.as_usize()),
        });
    }
    let source_q = q.len();
    let source_out = relevant
        .iter()
        .filter(|&&edge| graph.edges()[edge].from() == source)
        .count();
    let source_demand = u64::try_from(source_out)
        .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?
        .checked_sub(target)
        .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
    q.push(BipartiteNode {
        demand: source_demand,
        role: BipartiteRole::SourceQ(source.as_usize()),
    });
    let sink_p = p.len();
    let sink_in = relevant
        .iter()
        .filter(|&&edge| graph.edges()[edge].to() == sink)
        .count();
    let sink_demand = u64::try_from(sink_in)
        .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?
        .checked_sub(target)
        .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
    p.push(BipartiteNode {
        demand: sink_demand,
        role: BipartiteRole::SinkP(sink.as_usize()),
    });

    let mut edges = Vec::new();
    let mut original_direct_edge = vec![None; graph.edges().len()];
    for &edge_index in &relevant {
        let edge = &graph.edges()[edge_index];
        let pe = p_edge[edge_index].ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
        let qe = q_edge[edge_index].ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
        original_direct_edge[edge_index] = Some(edges.len());
        edges.push(BipartiteEdge {
            p: pe,
            q: qe,
            original_direct: Some(edge_index),
        });
        let tail_q = if edge.from() == source {
            source_q
        } else {
            q_vertex[edge.from().as_usize()].ok_or(InteriorPointMaxFlowError::ReductionLimit)?
        };
        edges.push(BipartiteEdge {
            p: pe,
            q: tail_q,
            original_direct: None,
        });
        let head_p = if edge.to() == sink {
            sink_p
        } else {
            p_vertex[edge.to().as_usize()].ok_or(InteriorPointMaxFlowError::ReductionLimit)?
        };
        edges.push(BipartiteEdge {
            p: head_p,
            q: qe,
            original_direct: None,
        });
    }
    for node in graph.node_indices() {
        if node == source || node == sink {
            continue;
        }
        edges.push(BipartiteEdge {
            p: p_vertex[node.as_usize()].ok_or(InteriorPointMaxFlowError::ReductionLimit)?,
            q: q_vertex[node.as_usize()].ok_or(InteriorPointMaxFlowError::ReductionLimit)?,
            original_direct: None,
        });
    }
    let p_demand = p
        .iter()
        .try_fold(0_u64, |sum, node| sum.checked_add(node.demand))
        .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
    let q_demand = q
        .iter()
        .try_fold(0_u64, |sum, node| sum.checked_add(node.demand))
        .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
    if p_demand != q_demand
        || p.len() + q.len() > INTERIOR_POINT_MAX_FLOW_MAX_WORKING_NODES
        || edges.len() > INTERIOR_POINT_MAX_FLOW_MAX_WORKING_EDGES
    {
        return Err(InteriorPointMaxFlowError::ReductionLimit);
    }
    Ok(BipartiteReduction {
        p,
        q,
        edges,
        original_direct_edge,
    })
}

#[allow(clippy::too_many_lines)]
fn build_working_reduction(
    graph: &FlowNetwork,
    reduction: &BipartiteReduction,
) -> Result<WorkingReduction, InteriorPointMaxFlowError> {
    let p_count = reduction.p.len();
    let q_count = reduction.q.len();
    let hub = p_count + q_count;
    let node_count = hub + 1;
    let mut arcs = Vec::new();
    let mut reduction_edge_arcs = vec![Vec::new(); reduction.edges.len()];
    let mut original_direct_arc = vec![None; graph.edges().len()];
    let mut p_direct = vec![0_u64; p_count];
    let mut q_direct = vec![0_u64; q_count];
    for (edge_index, edge) in reduction.edges.iter().enumerate() {
        let thickness = reduction.p[edge.p].demand.min(reduction.q[edge.q].demand);
        for _ in 0..thickness {
            let index = arcs.len();
            if let Some(original) = edge.original_direct {
                original_direct_arc[original] = Some(index);
            }
            reduction_edge_arcs[edge_index].push(index);
            arcs.push(WorkingArc {
                from: edge.p,
                to: p_count + edge.q,
                flow: 1.0,
                slack: 1.0,
                measure: 1.0,
            });
            p_direct[edge.p] += 1;
            q_direct[edge.q] += 1;
        }
    }
    for (index, node) in reduction.p.iter().enumerate() {
        let difference = i64::try_from(p_direct[index])
            .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?
            - i64::try_from(node.demand).map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?;
        let (forward, backward) = if difference >= 0 {
            (1.0, difference as f64 + 1.0)
        } else {
            (1.0 - difference as f64, 1.0)
        };
        arcs.push(initial_hub_arc(index, hub, forward));
        arcs.push(initial_hub_arc(hub, index, backward));
    }
    for (index, node) in reduction.q.iter().enumerate() {
        let work_node = p_count + index;
        let difference = i64::try_from(q_direct[index])
            .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?
            - i64::try_from(node.demand).map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?;
        let (forward, backward) = if difference >= 0 {
            (1.0, difference as f64 + 1.0)
        } else {
            (1.0 - difference as f64, 1.0)
        };
        arcs.push(initial_hub_arc(hub, work_node, forward));
        arcs.push(initial_hub_arc(work_node, hub, backward));
    }
    if node_count > INTERIOR_POINT_MAX_FLOW_MAX_WORKING_NODES
        || arcs.len() > INTERIOR_POINT_MAX_FLOW_MAX_WORKING_EDGES
        || original_direct_arc
            .iter()
            .enumerate()
            .any(|(index, arc)| reduction.original_direct_edge[index].is_some() != arc.is_some())
    {
        return Err(InteriorPointMaxFlowError::ReductionLimit);
    }
    let mut original_node_projection = vec![Vec::new(); graph.nodes().len()];
    for (index, node) in reduction.p.iter().enumerate() {
        match node.role {
            BipartiteRole::InternalP(_) | BipartiteRole::OriginalP(_) => {
                let projected = match node.role {
                    BipartiteRole::InternalP(value) => value,
                    BipartiteRole::OriginalP(edge) => graph.edges()[edge].from().as_usize(),
                    _ => unreachable!(),
                };
                original_node_projection[projected].push(index);
            }
            BipartiteRole::SinkP(_)
            | BipartiteRole::SourceQ(_)
            | BipartiteRole::OriginalQ(_)
            | BipartiteRole::InternalQ(_) => {}
        }
    }
    for (index, node) in reduction.q.iter().enumerate() {
        let work = p_count + index;
        match node.role {
            BipartiteRole::InternalQ(original) => original_node_projection[original].push(work),
            BipartiteRole::OriginalQ(edge) => {
                original_node_projection[graph.edges()[edge].to().as_usize()].push(work);
            }
            BipartiteRole::SourceQ(_)
            | BipartiteRole::SinkP(_)
            | BipartiteRole::OriginalP(_)
            | BipartiteRole::InternalP(_) => {}
        }
    }
    // Terminal projections are intentionally left empty; their scene potential is zero.
    let mut node_roles = reduction
        .p
        .iter()
        .map(|node| Some(node.role))
        .chain(reduction.q.iter().map(|node| Some(node.role)))
        .collect::<Vec<_>>();
    node_roles.push(None);
    Ok(WorkingReduction {
        node_count,
        hub,
        arcs,
        reduction_edge_arcs,
        original_direct_arc,
        original_node_projection,
        node_roles,
    })
}

fn initial_hub_arc(from: usize, to: usize, flow: f64) -> WorkingArc {
    WorkingArc {
        from,
        to,
        flow,
        slack: 1.0,
        measure: flow,
    }
}

fn solve_associated_electrical(
    working: &WorkingReduction,
    override_demands: Option<&[f64]>,
) -> Result<AssociatedElectricalSolution, InteriorPointMaxFlowError> {
    let demand = override_demands.map_or_else(|| divergences(working, None), ToOwned::to_owned);
    if demand.len() != working.node_count || demand.iter().sum::<f64>().abs() > NUMERICAL_TOLERANCE
    {
        return Err(InteriorPointMaxFlowError::NumericalFailure);
    }
    let dimension = working.node_count - 1;
    let mut matrix = vec![vec![0.0; dimension]; dimension];
    let mut rhs = vec![0.0; dimension];
    let mut map = vec![None; working.node_count];
    let mut row_nodes = Vec::with_capacity(dimension);
    let mut cursor = 0;
    for node in 0..working.node_count {
        if node != working.hub {
            map[node] = Some(cursor);
            row_nodes.push(node);
            rhs[cursor] = demand[node];
            cursor += 1;
        }
    }
    for arc in &working.arcs {
        let resistance = arc.slack / arc.flow;
        if resistance <= 0.0 || !resistance.is_finite() {
            return Err(InteriorPointMaxFlowError::NumericalFailure);
        }
        let conductance = 1.0 / resistance;
        if let Some(u) = map[arc.from] {
            matrix[u][u] += conductance;
        }
        if let Some(v) = map[arc.to] {
            matrix[v][v] += conductance;
        }
        if let (Some(u), Some(v)) = (map[arc.from], map[arc.to]) {
            matrix[u][v] -= conductance;
            matrix[v][u] -= conductance;
        }
    }
    let pivot_nodes = gaussian_solve(&mut matrix, &mut rhs, row_nodes)?;
    let mut potentials = vec![0.0; working.node_count];
    for node in 0..working.node_count {
        if let Some(index) = map[node] {
            potentials[node] = rhs[index];
        }
    }
    let mut currents = Vec::with_capacity(working.arcs.len());
    let mut energy = 0.0;
    for arc in &working.arcs {
        let resistance = arc.slack / arc.flow;
        let current = (potentials[arc.from] - potentials[arc.to]) / resistance;
        energy += resistance * current * current;
        currents.push(current);
    }
    if currents.iter().any(|value| !value.is_finite()) || !energy.is_finite() {
        return Err(InteriorPointMaxFlowError::NumericalFailure);
    }
    let actual = divergences(working, Some(&currents));
    if actual
        .iter()
        .zip(&demand)
        .any(|(left, right)| (left - right).abs() > NUMERICAL_TOLERANCE)
    {
        return Err(InteriorPointMaxFlowError::NumericalFailure);
    }
    Ok((currents, potentials, energy, pivot_nodes))
}

fn gaussian_solve(
    matrix: &mut [Vec<f64>],
    rhs: &mut [f64],
    mut row_nodes: Vec<usize>,
) -> Result<Vec<usize>, InteriorPointMaxFlowError> {
    let n = rhs.len();
    if matrix.len() != n || matrix.iter().any(|row| row.len() != n) || row_nodes.len() != n {
        return Err(InteriorPointMaxFlowError::NumericalFailure);
    }
    let mut pivot_nodes = Vec::with_capacity(n);
    for column in 0..n {
        let pivot = (column..n)
            .max_by(|&left, &right| {
                matrix[left][column]
                    .abs()
                    .total_cmp(&matrix[right][column].abs())
            })
            .ok_or(InteriorPointMaxFlowError::NumericalFailure)?;
        if matrix[pivot][column].abs() <= POSITIVE_FLOOR {
            return Err(InteriorPointMaxFlowError::NumericalFailure);
        }
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        row_nodes.swap(column, pivot);
        let divisor = matrix[column][column];
        for entry in &mut matrix[column][column..] {
            *entry /= divisor;
        }
        rhs[column] /= divisor;
        let pivot_row = matrix[column].clone();
        for row in 0..n {
            if row == column {
                continue;
            }
            let factor = matrix[row][column];
            if factor == 0.0 {
                continue;
            }
            for (item, pivot_value) in matrix[row][column..].iter_mut().zip(&pivot_row[column..]) {
                *item -= factor * pivot_value;
            }
            rhs[row] -= factor * rhs[column];
        }
        pivot_nodes.push(row_nodes[column]);
    }
    if rhs.iter().any(|value| !value.is_finite()) {
        return Err(InteriorPointMaxFlowError::NumericalFailure);
    }
    Ok(pivot_nodes)
}

fn divergences(working: &WorkingReduction, values: Option<&[f64]>) -> Vec<f64> {
    let mut result = vec![0.0; working.node_count];
    for (index, arc) in working.arcs.iter().enumerate() {
        let value = values.map_or(arc.flow, |items| items[index]);
        result[arc.from] += value;
        result[arc.to] -= value;
    }
    result
}

fn congestion_state(
    working: &WorkingReduction,
    electrical: &[f64],
) -> Result<(Vec<f64>, f64, Option<usize>), InteriorPointMaxFlowError> {
    if electrical.len() != working.arcs.len() {
        return Err(InteriorPointMaxFlowError::NumericalFailure);
    }
    let mut weighted_fourth = 0.0;
    let mut values = Vec::with_capacity(electrical.len());
    for (arc, current) in working.arcs.iter().zip(electrical) {
        let congestion = current.abs() / arc.flow;
        weighted_fourth += arc.measure * congestion.powi(4);
        values.push(congestion);
    }
    let l4 = weighted_fourth.sqrt().sqrt();
    if !l4.is_finite() {
        return Err(InteriorPointMaxFlowError::NumericalFailure);
    }
    let active = values
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .map(|(index, _)| index);
    Ok((values, l4, active))
}

fn centering_direction(
    working: &WorkingReduction,
    mean_mu: f64,
) -> Result<CenteringDirection, InteriorPointMaxFlowError> {
    let mut f_zero = Vec::with_capacity(working.arcs.len());
    let mut f_star = Vec::with_capacity(working.arcs.len());
    for arc in &working.arcs {
        let normalized = arc.flow * arc.slack / arc.measure;
        if normalized <= POSITIVE_FLOOR {
            return Err(InteriorPointMaxFlowError::NumericalFailure);
        }
        let star = (normalized - mean_mu) / normalized * arc.flow;
        let zero = arc.flow - star;
        if zero <= POSITIVE_FLOOR {
            return Err(InteriorPointMaxFlowError::NumericalFailure);
        }
        f_star.push(star);
        f_zero.push(zero);
    }
    let demand = divergences(working, Some(&f_star));
    let mut centered = working.clone();
    for (arc, zero) in centered.arcs.iter_mut().zip(&f_zero) {
        arc.flow = *zero;
    }
    let (correction, potentials, energy, pivot_nodes) =
        solve_associated_electrical(&centered, Some(&demand))?;
    Ok(CenteringDirection {
        zero_flow: f_zero,
        correction,
        potentials,
        energy,
        pivot_nodes,
    })
}

fn update_path_metrics(state: &mut KernelState) -> Result<(), InteriorPointMaxFlowError> {
    let working = state
        .working
        .as_ref()
        .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
    let measure_sum = working.arcs.iter().map(|arc| arc.measure).sum::<f64>();
    let gap = working
        .arcs
        .iter()
        .map(|arc| arc.flow * arc.slack)
        .sum::<f64>();
    let mu = gap / measure_sum;
    let centrality = working
        .arcs
        .iter()
        .map(|arc| {
            let normalized = arc.flow * arc.slack / arc.measure;
            arc.measure * (normalized - mu).powi(2)
        })
        .sum::<f64>()
        .sqrt()
        / mu;
    if !(mu > 0.0 && gap > 0.0 && mu.is_finite() && gap.is_finite() && centrality.is_finite()) {
        return Err(InteriorPointMaxFlowError::NumericalFailure);
    }
    state.mu = mu;
    state.duality_gap = gap;
    state.centrality = centrality;
    Ok(())
}

// Keeping the DFS next to the cycle-cancellation kernel makes the paper's
// recovery step auditable as one unit.
#[allow(clippy::items_after_statements)]
fn cancel_working_flow_cycles(
    working: &WorkingReduction,
) -> Result<(Vec<f64>, u64), InteriorPointMaxFlowError> {
    let mut flow = working.arcs.iter().map(|arc| arc.flow).collect::<Vec<_>>();
    let original_divergence = divergences(working, Some(&flow));
    let mut work = 0_u64;
    loop {
        let mut color = vec![0_u8; working.node_count];
        let mut parent_arc = vec![None::<usize>; working.node_count];
        let mut cycle = None::<Vec<usize>>;
        fn visit(
            node: usize,
            working: &WorkingReduction,
            flow: &[f64],
            color: &mut [u8],
            parent_arc: &mut [Option<usize>],
            cycle: &mut Option<Vec<usize>>,
            work: &mut u64,
        ) -> Result<(), InteriorPointMaxFlowError> {
            color[node] = 1;
            for (arc_index, arc) in working.arcs.iter().enumerate() {
                *work = work
                    .checked_add(1)
                    .ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
                if arc.from != node || flow[arc_index] <= NUMERICAL_TOLERANCE {
                    continue;
                }
                if color[arc.to] == 0 {
                    parent_arc[arc.to] = Some(arc_index);
                    visit(arc.to, working, flow, color, parent_arc, cycle, work)?;
                    if cycle.is_some() {
                        return Ok(());
                    }
                } else if color[arc.to] == 1 {
                    let mut found = vec![arc_index];
                    let mut cursor = node;
                    while cursor != arc.to {
                        let parent =
                            parent_arc[cursor].ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
                        found.push(parent);
                        cursor = working.arcs[parent].from;
                        if found.len() > working.node_count + 1 {
                            return Err(InteriorPointMaxFlowError::RoundingFailure);
                        }
                    }
                    *cycle = Some(found);
                    return Ok(());
                }
            }
            color[node] = 2;
            Ok(())
        }
        for node in 0..working.node_count {
            if color[node] == 0 {
                visit(
                    node,
                    working,
                    &flow,
                    &mut color,
                    &mut parent_arc,
                    &mut cycle,
                    &mut work,
                )?;
                if cycle.is_some() {
                    break;
                }
            }
        }
        let Some(cycle) = cycle else {
            break;
        };
        let amount = cycle
            .iter()
            .map(|&arc| flow[arc])
            .min_by(f64::total_cmp)
            .ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
        if amount <= NUMERICAL_TOLERANCE || !amount.is_finite() {
            return Err(InteriorPointMaxFlowError::RoundingFailure);
        }
        for arc in cycle {
            flow[arc] = (flow[arc] - amount).max(0.0);
            if flow[arc] <= NUMERICAL_TOLERANCE {
                flow[arc] = 0.0;
            }
        }
    }
    let cleaned_divergence = divergences(working, Some(&flow));
    if cleaned_divergence
        .iter()
        .zip(&original_divergence)
        .any(|(left, right)| (left - right).abs() > NUMERICAL_TOLERANCE * 4.0)
    {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    Ok((flow, work))
}

// This is the complete source recovery boundary from an acyclic demand flow
// to a fractional b-matching; splitting it would obscure its invariants.
#[allow(clippy::items_after_statements, clippy::too_many_lines)]
fn fractional_b_matching(
    state: &KernelState,
    acyclic_flow: &[f64],
) -> Result<Vec<f64>, InteriorPointMaxFlowError> {
    let reduction = state
        .reduction
        .as_ref()
        .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
    let working = state
        .working
        .as_ref()
        .ok_or(InteriorPointMaxFlowError::ReductionLimit)?;
    if working.reduction_edge_arcs.len() != reduction.edges.len() {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    if acyclic_flow.len() != working.arcs.len() {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    let mut arc_to_reduction = vec![None::<usize>; working.arcs.len()];
    for (edge_index, copies) in working.reduction_edge_arcs.iter().enumerate() {
        for &arc in copies {
            let slot = arc_to_reduction
                .get_mut(arc)
                .ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
            if slot.replace(edge_index).is_some() {
                return Err(InteriorPointMaxFlowError::RoundingFailure);
            }
        }
    }
    let p_count = reduction.p.len();
    let q_count = reduction.q.len();
    let mut remaining = acyclic_flow.to_vec();
    let mut supply = reduction
        .p
        .iter()
        .map(|node| node.demand as f64)
        .collect::<Vec<_>>();
    let mut demand = reduction
        .q
        .iter()
        .map(|node| node.demand as f64)
        .collect::<Vec<_>>();
    let mut values = vec![0.0; reduction.edges.len()];
    #[allow(clippy::too_many_arguments)]
    fn path_to_demand(
        node: usize,
        p_count: usize,
        q_count: usize,
        working: &WorkingReduction,
        remaining: &[f64],
        demand: &[f64],
        seen: &mut [bool],
        path: &mut Vec<usize>,
    ) -> Option<usize> {
        if (p_count..p_count + q_count).contains(&node)
            && demand[node - p_count] > NUMERICAL_TOLERANCE
        {
            return Some(node - p_count);
        }
        seen[node] = true;
        for (arc_index, arc) in working.arcs.iter().enumerate() {
            if arc.from != node || remaining[arc_index] <= NUMERICAL_TOLERANCE || seen[arc.to] {
                continue;
            }
            path.push(arc_index);
            if let Some(sink) = path_to_demand(
                arc.to, p_count, q_count, working, remaining, demand, seen, path,
            ) {
                return Some(sink);
            }
            path.pop();
        }
        None
    }
    let mut source = 0_usize;
    while source < p_count {
        while supply[source] > NUMERICAL_TOLERANCE {
            let mut seen = vec![false; working.node_count];
            let mut path = Vec::new();
            let sink = path_to_demand(
                source, p_count, q_count, working, &remaining, &demand, &mut seen, &mut path,
            )
            .ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
            let amount = path
                .iter()
                .map(|&arc| remaining[arc])
                .fold(supply[source].min(demand[sink]), f64::min);
            if amount <= NUMERICAL_TOLERANCE || !amount.is_finite() {
                return Err(InteriorPointMaxFlowError::RoundingFailure);
            }
            for &arc in &path {
                remaining[arc] = (remaining[arc] - amount).max(0.0);
            }
            supply[source] -= amount;
            demand[sink] -= amount;
            if path.len() == 1
                && let Some(edge) = arc_to_reduction[path[0]]
            {
                values[edge] += amount;
            }
        }
        source += 1;
    }
    if supply
        .iter()
        .chain(&demand)
        .any(|value| value.abs() > NUMERICAL_TOLERANCE * 4.0)
        || remaining
            .iter()
            .any(|value| value.abs() > NUMERICAL_TOLERANCE * 4.0)
    {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    let mut p_degree = vec![0.0; reduction.p.len()];
    let mut q_degree = vec![0.0; reduction.q.len()];
    for (edge, &value) in reduction.edges.iter().zip(&values) {
        let thickness = reduction.p[edge.p].demand.min(reduction.q[edge.q].demand) as f64;
        if value > thickness + NUMERICAL_TOLERANCE {
            return Err(InteriorPointMaxFlowError::RoundingFailure);
        }
        p_degree[edge.p] += value;
        q_degree[edge.q] += value;
    }
    if p_degree
        .iter()
        .zip(&reduction.p)
        .any(|(degree, node)| *degree > node.demand as f64 + NUMERICAL_TOLERANCE)
        || q_degree
            .iter()
            .zip(&reduction.q)
            .any(|(degree, node)| *degree > node.demand as f64 + NUMERICAL_TOLERANCE)
    {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    Ok(values)
}

#[derive(Clone, Copy, Debug)]
struct SplitFragment {
    left: usize,
    right: usize,
    reduction_edge: Option<usize>,
    weight: f64,
}

fn demand_offsets(nodes: &[BipartiteNode]) -> Result<Vec<usize>, InteriorPointMaxFlowError> {
    let mut offsets = Vec::with_capacity(nodes.len() + 1);
    offsets.push(0_usize);
    for node in nodes {
        let demand =
            usize::try_from(node.demand).map_err(|_| InteriorPointMaxFlowError::RoundingFailure)?;
        offsets.push(
            offsets
                .last()
                .copied()
                .and_then(|offset| offset.checked_add(demand))
                .ok_or(InteriorPointMaxFlowError::RoundingFailure)?,
        );
    }
    Ok(offsets)
}

fn distribute_to_unit_slots(
    offset: usize,
    count: usize,
    loads: &mut [f64],
    cursor: &mut usize,
    mut amount: f64,
    mut emit: impl FnMut(usize, f64),
) -> Result<(), InteriorPointMaxFlowError> {
    while amount > NUMERICAL_TOLERANCE {
        while *cursor < count && loads[offset + *cursor] >= 1.0 - NUMERICAL_TOLERANCE {
            loads[offset + *cursor] = 1.0;
            *cursor += 1;
        }
        if *cursor >= count {
            return Err(InteriorPointMaxFlowError::RoundingFailure);
        }
        let slot = offset + *cursor;
        let piece = amount.min((1.0 - loads[slot]).max(0.0));
        if piece <= NUMERICAL_TOLERANCE {
            return Err(InteriorPointMaxFlowError::RoundingFailure);
        }
        loads[slot] += piece;
        amount -= piece;
        emit(slot, piece);
    }
    Ok(())
}

fn append_dummy_completion(
    left_loads: &mut [f64],
    right_loads: &mut [f64],
    left_range: std::ops::Range<usize>,
    right_range: std::ops::Range<usize>,
    support: &mut Vec<SplitFragment>,
) -> Result<(), InteriorPointMaxFlowError> {
    let mut right = right_range.start;
    for left in left_range {
        let mut deficit = (1.0 - left_loads[left]).max(0.0);
        while deficit > NUMERICAL_TOLERANCE {
            while right < right_range.end && right_loads[right] >= 1.0 - NUMERICAL_TOLERANCE {
                right_loads[right] = 1.0;
                right += 1;
            }
            if right >= right_range.end {
                return Err(InteriorPointMaxFlowError::RoundingFailure);
            }
            let amount = deficit.min((1.0 - right_loads[right]).max(0.0));
            if amount <= NUMERICAL_TOLERANCE {
                return Err(InteriorPointMaxFlowError::RoundingFailure);
            }
            left_loads[left] += amount;
            right_loads[right] += amount;
            deficit -= amount;
            support.push(SplitFragment {
                left,
                right,
                reduction_edge: None,
                weight: amount,
            });
        }
    }
    Ok(())
}

fn append_right_dummy_completion(
    left_loads: &mut [f64],
    right_loads: &mut [f64],
    left_range: std::ops::Range<usize>,
    right_range: std::ops::Range<usize>,
    support: &mut Vec<SplitFragment>,
) -> Result<(), InteriorPointMaxFlowError> {
    let mut left = left_range.start;
    for right in right_range {
        let mut deficit = (1.0 - right_loads[right]).max(0.0);
        while deficit > NUMERICAL_TOLERANCE {
            while left < left_range.end && left_loads[left] >= 1.0 - NUMERICAL_TOLERANCE {
                left_loads[left] = 1.0;
                left += 1;
            }
            if left >= left_range.end {
                return Err(InteriorPointMaxFlowError::RoundingFailure);
            }
            let amount = deficit.min((1.0 - left_loads[left]).max(0.0));
            if amount <= NUMERICAL_TOLERANCE {
                return Err(InteriorPointMaxFlowError::RoundingFailure);
            }
            left_loads[left] += amount;
            right_loads[right] += amount;
            deficit -= amount;
            support.push(SplitFragment {
                left,
                right,
                reduction_edge: None,
                weight: amount,
            });
        }
    }
    Ok(())
}

// The local augment routine is part of this deterministic replacement for the
// paper's asymptotically fast support-matching subroutine.
#[allow(clippy::items_after_statements)]
fn support_perfect_matching(
    left_count: usize,
    right_count: usize,
    support: &[SplitFragment],
) -> Result<(Vec<usize>, u64), InteriorPointMaxFlowError> {
    if left_count != right_count {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    let mut adjacency = vec![Vec::new(); left_count];
    for (index, edge) in support.iter().enumerate() {
        if edge.left >= left_count
            || edge.right >= right_count
            || edge.weight <= NUMERICAL_TOLERANCE
        {
            return Err(InteriorPointMaxFlowError::RoundingFailure);
        }
        adjacency[edge.left].push(index);
    }
    let mut matched_right = vec![None::<(usize, usize)>; right_count];
    let mut work = 0_u64;
    fn augment(
        left: usize,
        adjacency: &[Vec<usize>],
        support: &[SplitFragment],
        matched_right: &mut [Option<(usize, usize)>],
        seen: &mut [bool],
        work: &mut u64,
    ) -> Result<bool, InteriorPointMaxFlowError> {
        for &edge_index in &adjacency[left] {
            *work = work
                .checked_add(1)
                .ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
            let right = support[edge_index].right;
            if seen[right] {
                continue;
            }
            seen[right] = true;
            let replace = match matched_right[right] {
                None => true,
                Some((previous, _)) => {
                    augment(previous, adjacency, support, matched_right, seen, work)?
                }
            };
            if replace {
                matched_right[right] = Some((left, edge_index));
                return Ok(true);
            }
        }
        Ok(false)
    }
    for left in 0..left_count {
        let mut seen = vec![false; right_count];
        if !augment(
            left,
            &adjacency,
            support,
            &mut matched_right,
            &mut seen,
            &mut work,
        )? {
            return Err(InteriorPointMaxFlowError::RoundingFailure);
        }
    }
    Ok((
        matched_right
            .into_iter()
            .map(|entry| entry.map(|(_, edge)| edge))
            .collect::<Option<Vec<_>>>()
            .ok_or(InteriorPointMaxFlowError::RoundingFailure)?,
        work,
    ))
}

// This function intentionally keeps the split, dummy completion, support
// matching, and final augmentation in their proof order.
#[allow(clippy::items_after_statements, clippy::too_many_lines)]
fn round_and_complete_b_matching(
    reduction: &BipartiteReduction,
    fractional: &[f64],
) -> Result<(Vec<u64>, u64), InteriorPointMaxFlowError> {
    if fractional.len() != reduction.edges.len() {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    let p_offsets = demand_offsets(&reduction.p)?;
    let q_offsets = demand_offsets(&reduction.q)?;
    let p_units = *p_offsets
        .last()
        .ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
    let q_units = *q_offsets
        .last()
        .ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
    if p_units != q_units {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    let mut p_loads = vec![0.0; p_units];
    let mut q_loads = vec![0.0; q_units];
    let mut p_cursor = vec![0_usize; reduction.p.len()];
    let mut q_cursor = vec![0_usize; reduction.q.len()];
    let mut half = Vec::<(usize, usize, usize, f64)>::new();
    for (edge_index, (edge, &weight)) in reduction.edges.iter().zip(fractional).enumerate() {
        distribute_to_unit_slots(
            p_offsets[edge.p],
            p_offsets[edge.p + 1] - p_offsets[edge.p],
            &mut p_loads,
            &mut p_cursor[edge.p],
            weight,
            |left, piece| half.push((left, edge.q, edge_index, piece)),
        )?;
    }
    let mut support = Vec::new();
    for (left, q, edge_index, weight) in half {
        distribute_to_unit_slots(
            q_offsets[q],
            q_offsets[q + 1] - q_offsets[q],
            &mut q_loads,
            &mut q_cursor[q],
            weight,
            |right, piece| {
                support.push(SplitFragment {
                    left,
                    right,
                    reduction_edge: Some(edge_index),
                    weight: piece,
                });
            },
        )?;
    }

    let left_deficit = p_loads.iter().map(|value| 1.0 - value).sum::<f64>();
    let right_deficit = q_loads.iter().map(|value| 1.0 - value).sum::<f64>();
    if (left_deficit - right_deficit).abs() > NUMERICAL_TOLERANCE * p_units.max(1) as f64 {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    let dummy_count = left_deficit
        .max(right_deficit)
        .max(0.0)
        .ceil()
        .to_usize()
        .ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
    let real_left_units = p_loads.len();
    let real_right_units = q_loads.len();
    p_loads.resize(real_left_units + dummy_count, 0.0);
    q_loads.resize(real_right_units + dummy_count, 0.0);
    append_dummy_completion(
        &mut p_loads,
        &mut q_loads,
        0..real_left_units,
        real_right_units..real_right_units + dummy_count,
        &mut support,
    )?;
    append_right_dummy_completion(
        &mut p_loads,
        &mut q_loads,
        real_left_units..real_left_units + dummy_count,
        0..real_right_units,
        &mut support,
    )?;
    append_dummy_completion(
        &mut p_loads,
        &mut q_loads,
        real_left_units..real_left_units + dummy_count,
        real_right_units..real_right_units + dummy_count,
        &mut support,
    )?;
    if p_loads
        .iter()
        .chain(&q_loads)
        .any(|degree| (degree - 1.0).abs() > NUMERICAL_TOLERANCE * 8.0)
    {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }

    let (selected_support, mut work) =
        support_perfect_matching(p_loads.len(), q_loads.len(), &support)?;
    let mut rounded_left = vec![None::<(usize, usize)>; real_left_units];
    let mut rounded_right = vec![None::<(usize, usize)>; real_right_units];
    let mut rounded_size = 0_usize;
    for edge_index in selected_support {
        let edge = support[edge_index];
        if let Some(reduction_edge) = edge.reduction_edge {
            if edge.left >= real_left_units
                || edge.right >= real_right_units
                || rounded_left[edge.left].is_some()
                || rounded_right[edge.right].is_some()
            {
                return Err(InteriorPointMaxFlowError::RoundingFailure);
            }
            rounded_left[edge.left] = Some((edge.right, reduction_edge));
            rounded_right[edge.right] = Some((edge.left, reduction_edge));
            rounded_size += 1;
        }
    }
    let fractional_size = fractional.iter().sum::<f64>();
    let fractional_floor = fractional_size
        .floor()
        .to_usize()
        .ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
    if rounded_size < fractional_floor {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    let mut full_adjacency = vec![Vec::<(usize, usize)>::new(); real_left_units];
    for (edge_index, edge) in reduction.edges.iter().enumerate() {
        for adjacency in &mut full_adjacency[p_offsets[edge.p]..p_offsets[edge.p + 1]] {
            for right in q_offsets[edge.q]..q_offsets[edge.q + 1] {
                adjacency.push((right, edge_index));
            }
        }
    }
    let mut match_right = rounded_right;
    fn augment_full(
        left: usize,
        adjacency: &[Vec<(usize, usize)>],
        match_left: &mut [Option<(usize, usize)>],
        match_right: &mut [Option<(usize, usize)>],
        seen: &mut [bool],
        work: &mut u64,
    ) -> Result<bool, InteriorPointMaxFlowError> {
        for &(right, reduction_edge) in &adjacency[left] {
            *work = work
                .checked_add(1)
                .ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
            if seen[right] {
                continue;
            }
            seen[right] = true;
            let replace = match match_right[right] {
                None => true,
                Some((previous, _)) => {
                    augment_full(previous, adjacency, match_left, match_right, seen, work)?
                }
            };
            if replace {
                if let Some((old_right, _)) = match_left[left] {
                    match_right[old_right] = None;
                }
                match_left[left] = Some((right, reduction_edge));
                match_right[right] = Some((left, reduction_edge));
                return Ok(true);
            }
        }
        Ok(false)
    }
    for left in 0..real_left_units {
        if rounded_left[left].is_none() {
            let mut seen = vec![false; real_right_units];
            let _ = augment_full(
                left,
                &full_adjacency,
                &mut rounded_left,
                &mut match_right,
                &mut seen,
                &mut work,
            )?;
        }
    }
    if rounded_left.iter().any(Option::is_none) || match_right.iter().any(Option::is_none) {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    let mut matching = vec![0_u64; reduction.edges.len()];
    for (_, edge_index) in rounded_left.into_iter().flatten() {
        matching[edge_index] = matching[edge_index]
            .checked_add(1)
            .ok_or(InteriorPointMaxFlowError::RoundingFailure)?;
    }
    let mut p_degree = vec![0_u64; reduction.p.len()];
    let mut q_degree = vec![0_u64; reduction.q.len()];
    for (edge, &value) in reduction.edges.iter().zip(&matching) {
        p_degree[edge.p] += value;
        q_degree[edge.q] += value;
    }
    if p_degree
        .iter()
        .zip(&reduction.p)
        .any(|(degree, node)| *degree != node.demand)
        || q_degree
            .iter()
            .zip(&reduction.q)
            .any(|(degree, node)| *degree != node.demand)
    {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    Ok((matching, work))
}

fn extract_integral_flow(
    graph: &FlowNetwork,
    reduction: &BipartiteReduction,
    matching: &[u64],
) -> Result<Vec<u64>, InteriorPointMaxFlowError> {
    if matching.len() != reduction.edges.len()
        || reduction.original_direct_edge.len() != graph.edges().len()
    {
        return Err(InteriorPointMaxFlowError::RoundingFailure);
    }
    reduction
        .original_direct_edge
        .iter()
        .map(|edge| {
            edge.map_or(Ok(0), |index| {
                matching
                    .get(index)
                    .copied()
                    .ok_or(InteriorPointMaxFlowError::RoundingFailure)
            })
        })
        .collect()
}

// Projection collects all reduction layers into one atomic replay snapshot.
#[allow(clippy::too_many_lines)]
fn project_snapshot(
    graph: &FlowNetwork,
    state: &KernelState,
) -> Result<InteriorPointMaxFlowSnapshot, InteriorPointMaxFlowError> {
    let zero = InteriorPointScalar::try_new(0.0)?;
    let nodes = graph
        .node_indices()
        .map(|node| {
            let potential = state.working.as_ref().map_or(0.0, |working| {
                let slots = &working.original_node_projection[node.as_usize()];
                if slots.is_empty() {
                    0.0
                } else {
                    slots
                        .iter()
                        .map(|&slot| state.potentials.get(slot).copied().unwrap_or(0.0))
                        .sum::<f64>()
                        / slots.len() as f64
                }
            });
            Ok(InteriorPointNodeState {
                node,
                potential: InteriorPointScalar::try_new(potential)?,
                target_source_side: state.target_source_side[node.as_usize()],
            })
        })
        .collect::<Result<Vec<_>, InteriorPointMaxFlowError>>()?;
    let edges = graph
        .edges()
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            let slot = state
                .working
                .as_ref()
                .and_then(|working| working.original_direct_arc[index]);
            let arc =
                slot.and_then(|slot| state.working.as_ref().map(|working| &working.arcs[slot]));
            let current = slot
                .and_then(|slot| state.latest_currents.get(slot))
                .copied()
                .unwrap_or(0.0);
            let congestion = slot
                .and_then(|slot| state.latest_congestions.get(slot))
                .copied()
                .unwrap_or(0.0);
            let extracted = matches!(
                state.stage,
                InteriorPointMaxFlowStage::ExtractFractionalFlow
                    | InteriorPointMaxFlowStage::RoundIntegralFlow
                    | InteriorPointMaxFlowStage::CheckCertificate
                    | InteriorPointMaxFlowStage::Optimal
            )
            .then(|| state.extracted_original_flow[index]);
            Ok(InteriorPointEdgeState {
                edge: edge.id().clone(),
                fractional_flow: InteriorPointScalar::try_new(
                    extracted.unwrap_or_else(|| arc.map_or(0.0, |item| item.flow)),
                )?,
                electrical_current: InteriorPointScalar::try_new(current)?,
                slack: InteriorPointScalar::try_new(arc.map_or(0.0, |item| item.slack))?,
                measure: InteriorPointScalar::try_new(arc.map_or(0.0, |item| item.measure))?,
                resistance: InteriorPointScalar::try_new(
                    arc.map_or(0.0, |item| item.slack / item.flow),
                )?,
                congestion: InteriorPointScalar::try_new(congestion)?,
                normalized_away: state.normalized_away[index],
                final_flow: state.final_flows[index],
            })
        })
        .collect::<Result<Vec<_>, InteriorPointMaxFlowError>>()?;
    let (b_nodes, b_edges) = state.reduction.as_ref().map_or((0, 0), |reduction| {
        (reduction.p.len() + reduction.q.len(), reduction.edges.len())
    });
    let (working_nodes, working_edges) = state
        .working
        .as_ref()
        .map_or((0, 0), |working| (working.node_count, working.arcs.len()));
    let active_pivot_role = state
        .active_pivot_node
        .zip(state.working.as_ref())
        .map(|(pivot, working)| {
            working
                .node_roles
                .get(pivot)
                .copied()
                .flatten()
                .ok_or(InteriorPointMaxFlowError::ReductionLimit)
        })
        .transpose()?;
    let active_pivot_original_nodes = match active_pivot_role {
        Some(
            BipartiteRole::InternalP(node)
            | BipartiteRole::InternalQ(node)
            | BipartiteRole::SourceQ(node)
            | BipartiteRole::SinkP(node),
        ) => vec![
            graph
                .node_indices()
                .nth(node)
                .ok_or(InteriorPointMaxFlowError::ReductionLimit)?,
        ],
        Some(BipartiteRole::OriginalP(_) | BipartiteRole::OriginalQ(_)) | None => Vec::new(),
    };
    let active_pivot_original_edges = match active_pivot_role {
        Some(BipartiteRole::OriginalP(edge) | BipartiteRole::OriginalQ(edge)) => vec![
            graph
                .edges()
                .get(edge)
                .ok_or(InteriorPointMaxFlowError::ReductionLimit)?
                .id()
                .clone(),
        ],
        Some(
            BipartiteRole::InternalP(_)
            | BipartiteRole::InternalQ(_)
            | BipartiteRole::SourceQ(_)
            | BipartiteRole::SinkP(_),
        )
        | None => Vec::new(),
    };
    Ok(InteriorPointMaxFlowSnapshot {
        nodes,
        edges,
        target_value: state.target,
        mu: if state.working.is_some() {
            InteriorPointScalar::try_new(state.mu)?
        } else {
            zero
        },
        duality_gap: if state.working.is_some() {
            InteriorPointScalar::try_new(state.duality_gap)?
        } else {
            zero
        },
        centrality: if state.working.is_some() {
            InteriorPointScalar::try_new(state.centrality)?
        } else {
            zero
        },
        congestion_l4: InteriorPointScalar::try_new(state.congestion_l4)?,
        step_size: InteriorPointScalar::try_new(state.step_size)?,
        electrical_energy: InteriorPointScalar::try_new(state.energy)?,
        b_matching_nodes: u64::try_from(b_nodes)
            .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?,
        b_matching_edges: u64::try_from(b_edges)
            .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?,
        working_nodes: u64::try_from(working_nodes)
            .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?,
        working_edges: u64::try_from(working_edges)
            .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?,
        active_working_edge: state
            .active_working_edge
            .map(u64::try_from)
            .transpose()
            .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?,
        active_pivot_node: state
            .active_pivot_node
            .map(u64::try_from)
            .transpose()
            .map_err(|_| InteriorPointMaxFlowError::ReductionLimit)?,
        active_pivot_original_nodes,
        active_pivot_original_edges,
        stage: state.stage,
        metrics: state.metrics,
    })
}

fn validate_terminal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    result: &InteriorPointMaxFlowResult,
) -> Result<(), InteriorPointMaxFlowError> {
    if result.final_snapshot.stage != InteriorPointMaxFlowStage::Optimal
        || result.final_snapshot.metrics != result.metrics
        || result.final_snapshot.edges.len() != graph.edges().len()
        || result
            .final_snapshot
            .edges
            .iter()
            .zip(&result.flows)
            .any(|(edge, flow)| edge.final_flow != Some(*flow))
        || check_max_flow(graph, source, sink, &result.flows)? != result.certificate
    {
        return Err(InteriorPointMaxFlowError::CertificateFailure);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FlowEdge, FlowNode, NodeId, UnresolvedFlowEdge};

    fn graph(edges: &[(&str, &str, &str)]) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let nodes = ["s", "a", "b", "t"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
            .collect::<Vec<_>>();
        let edges = edges
            .iter()
            .map(|(id, from, to)| UnresolvedFlowEdge {
                id: EdgeId::parse(id).expect("edge"),
                from: NodeId::parse(from).expect("from"),
                to: NodeId::parse(to).expect("to"),
                lower: 0,
                capacity: 1,
                cost: 0,
            })
            .collect();
        let graph = FlowNetwork::new(nodes, edges).expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        (graph, source, sink)
    }

    #[test]
    fn solves_two_path_unit_network_and_centers() {
        let (graph, source, sink) = graph(&[
            ("e0", "s", "a"),
            ("e1", "a", "t"),
            ("e2", "s", "b"),
            ("e3", "b", "t"),
        ]);
        let result = solve_interior_point_max_flow(&graph, source, sink).expect("solve");
        assert_eq!(result.certificate.value, 2);
        assert_eq!(result.flows, vec![1, 1, 1, 1]);
        assert!(result.metrics.progress_steps > 0);
        assert_eq!(
            result.metrics.progress_steps,
            result.metrics.centering_steps
        );
        assert_eq!(
            result.final_snapshot.stage,
            InteriorPointMaxFlowStage::Optimal
        );
    }

    #[test]
    fn trace_replays_and_exposes_both_electrical_directions() {
        let (graph, source, sink) = graph(&[
            ("e0", "s", "a"),
            ("e1", "a", "b"),
            ("e2", "b", "t"),
            ("e3", "s", "b"),
        ]);
        let trace = trace_interior_point_max_flow(&graph, source, sink).expect("trace");
        check_interior_point_max_flow_trace(&graph, source, sink, &trace).expect("check");
        assert!(
            trace
                .events
                .iter()
                .any(|event| event.after.stage
                    == InteriorPointMaxFlowStage::SolveElectricalDirection)
        );
        assert!(
            trace.events.iter().any(
                |event| event.after.stage == InteriorPointMaxFlowStage::SolveCenteringDirection
            )
        );
        assert_eq!(
            trace.result.flows,
            solve_interior_point_max_flow(&graph, source, sink)
                .expect("fast")
                .flows
        );
        let pivot_events = trace
            .events
            .iter()
            .filter(|event| event.after.stage == InteriorPointMaxFlowStage::SolveElectricalPivot)
            .collect::<Vec<_>>();
        assert_eq!(
            u64::try_from(pivot_events.len()).expect("pivot event count"),
            trace.result.metrics.elimination_pivots
        );
        assert!(!pivot_events.is_empty());
        for event in pivot_events {
            assert_eq!(
                event.after.metrics.elimination_pivots,
                event.before.metrics.elimination_pivots + 1
            );
            assert!(event.after.active_pivot_node.is_some());
            assert!(
                !event.after.active_pivot_original_nodes.is_empty()
                    || !event.after.active_pivot_original_edges.is_empty()
            );
        }
    }

    #[test]
    fn handles_zero_flow_and_terminal_normalization() {
        let (graph, source, sink) = graph(&[("e0", "a", "s"), ("e1", "t", "b")]);
        let result = solve_interior_point_max_flow(&graph, source, sink).expect("zero");
        assert_eq!(result.certificate.value, 0);
        assert_eq!(result.flows, vec![0, 0]);
        assert!(
            result
                .final_snapshot
                .edges
                .iter()
                .all(|edge| edge.normalized_away)
        );
    }

    #[test]
    fn rejects_non_unit_and_self_loop_inputs() {
        let (graph, source, sink) = graph(&[("e0", "s", "a"), ("e1", "a", "t")]);
        let nodes = graph.nodes().to_vec();
        let bad = FlowNetwork::new(
            nodes.clone(),
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("bad").expect("edge"),
                from: NodeId::parse("s").expect("from"),
                to: NodeId::parse("a").expect("to"),
                lower: 0,
                capacity: 2,
                cost: 0,
            }],
        )
        .expect("bad graph");
        assert_eq!(
            solve_interior_point_max_flow(&bad, source, sink),
            Err(InteriorPointMaxFlowError::GraphRequirement)
        );

        let self_loop = FlowNetwork::new(
            nodes,
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("loop").expect("edge"),
                from: NodeId::parse("a").expect("from"),
                to: NodeId::parse("a").expect("to"),
                lower: 0,
                capacity: 1,
                cost: 0,
            }],
        )
        .expect("self-loop graph");
        assert_eq!(
            solve_interior_point_max_flow(&self_loop, source, sink),
            Err(InteriorPointMaxFlowError::GraphRequirement)
        );
    }

    #[test]
    fn matches_exact_cut_on_every_six_edge_subset() {
        let candidates = [
            ("sa", "s", "a"),
            ("at", "a", "t"),
            ("sb", "s", "b"),
            ("bt", "b", "t"),
            ("ab", "a", "b"),
            ("ba", "b", "a"),
        ];
        for mask in 0_u64..(1_u64 << candidates.len()) {
            let edges = candidates
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1_u64 << index) != 0)
                .map(|(_, edge)| *edge)
                .collect::<Vec<_>>();
            let (network, source, sink) = graph(&edges);
            let expected = (0_u64..(1_u64 << network.nodes().len()))
                .filter(|side| {
                    side & (1_u64 << source.as_usize()) != 0
                        && side & (1_u64 << sink.as_usize()) == 0
                })
                .map(|side| {
                    network
                        .edges()
                        .iter()
                        .filter(|edge| {
                            side & (1_u64 << edge.from().as_usize()) != 0
                                && side & (1_u64 << edge.to().as_usize()) == 0
                        })
                        .map(FlowEdge::capacity)
                        .sum::<u64>()
                })
                .min()
                .expect("exact cut");
            let result = solve_interior_point_max_flow(&network, source, sink)
                .unwrap_or_else(|error| panic!("subset {mask:06b} failed: {error}"));
            assert_eq!(
                result.certificate.value,
                i128::from(expected),
                "subset {mask:06b}"
            );
            assert_eq!(
                check_max_flow(&network, source, sink, &result.flows).expect("certificate"),
                result.certificate,
                "subset {mask:06b}"
            );
        }
    }

    #[test]
    fn rounds_near_perfect_b_matching_then_augments_without_flow_enumeration() {
        let reduction = BipartiteReduction {
            p: vec![
                BipartiteNode {
                    demand: 2,
                    role: BipartiteRole::OriginalP(0),
                },
                BipartiteNode {
                    demand: 1,
                    role: BipartiteRole::OriginalP(1),
                },
            ],
            q: vec![
                BipartiteNode {
                    demand: 1,
                    role: BipartiteRole::OriginalQ(0),
                },
                BipartiteNode {
                    demand: 2,
                    role: BipartiteRole::OriginalQ(1),
                },
            ],
            edges: vec![
                BipartiteEdge {
                    p: 0,
                    q: 0,
                    original_direct: None,
                },
                BipartiteEdge {
                    p: 0,
                    q: 1,
                    original_direct: None,
                },
                BipartiteEdge {
                    p: 1,
                    q: 0,
                    original_direct: None,
                },
                BipartiteEdge {
                    p: 1,
                    q: 1,
                    original_direct: None,
                },
            ],
            original_direct_edge: Vec::new(),
        };
        let fractional = [0.75, 1.0, 0.2, 0.8];

        let (matching, work) =
            round_and_complete_b_matching(&reduction, &fractional).expect("source recovery");
        let (replayed, replay_work) =
            round_and_complete_b_matching(&reduction, &fractional).expect("deterministic replay");

        assert_eq!(matching, replayed);
        assert_eq!(work, replay_work);
        assert!(work > 0);
        assert_eq!(matching.iter().sum::<u64>(), 3);
        assert_eq!(matching[0] + matching[1], 2);
        assert_eq!(matching[2] + matching[3], 1);
        assert_eq!(matching[0] + matching[2], 1);
        assert_eq!(matching[1] + matching[3], 2);
    }

    #[test]
    fn altered_trace_is_rejected() {
        let (graph, source, sink) = graph(&[("e0", "s", "a"), ("e1", "a", "t")]);
        let mut trace = trace_interior_point_max_flow(&graph, source, sink).expect("trace");
        trace.events[0].catalog_id = "ipm.optimal";
        assert_eq!(
            check_interior_point_max_flow_trace(&graph, source, sink, &trace),
            Err(InteriorPointMaxFlowError::TraceVerification)
        );
    }

    #[test]
    fn consistently_forged_ready_snapshot_is_rejected() {
        let (graph, source, sink) = graph(&[("e0", "s", "a"), ("e1", "a", "t")]);
        let mut trace = trace_interior_point_max_flow(&graph, source, sink).expect("trace");
        trace.base_snapshot.target_value = 1;
        trace.events[0].before.target_value = 1;
        assert_eq!(
            check_interior_point_max_flow_trace(&graph, source, sink, &trace),
            Err(InteriorPointMaxFlowError::TraceVerification)
        );
    }
}
