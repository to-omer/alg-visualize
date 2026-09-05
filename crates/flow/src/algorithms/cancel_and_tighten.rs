//! Exact bounded Cancel-and-Tighten minimum-cost flow.
//!
//! The kernel follows Goldberg--Tarjan's primal strategy: cancel every cycle
//! consisting only of negative reduced-cost residual arcs, then tighten the
//! potential with a topological rank of the resulting admissible DAG. Exact
//! rational arithmetic keeps the `epsilon / n` update free of rounding.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArc, ResidualArcId, ResidualError, ResidualState};

/// Conservative interactive node limit for the explicit rational build.
pub const CANCEL_AND_TIGHTEN_MAX_NODES: usize = 128;
/// Conservative interactive edge limit for the explicit rational build.
pub const CANCEL_AND_TIGHTEN_MAX_EDGES: usize = 1_024;
/// Deterministic ceiling for outer cancel/tighten phases.
pub const CANCEL_AND_TIGHTEN_MAX_PHASES: u64 = 20_000;
/// Deterministic ceiling for individual cycle cancellations.
pub const CANCEL_AND_TIGHTEN_MAX_CANCELLATIONS: u64 = 200_000;
/// Deterministic ceiling for residual scans across cycle search and ranking.
pub const CANCEL_AND_TIGHTEN_MAX_RESIDUAL_ARC_SCANS: u128 = 20_000_000;

/// Canonical exact rational transported by the algorithm trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelTightenRational {
    numerator: BigInt,
    denominator: BigInt,
}

impl CancelTightenRational {
    fn from_ratio(value: &BigRational) -> Self {
        Self {
            numerator: value.numer().clone(),
            denominator: value.denom().clone(),
        }
    }

    fn to_ratio(&self) -> BigRational {
        BigRational::new(self.numerator.clone(), self.denominator.clone())
    }

    /// Reduced canonical numerator.
    pub fn numerator(&self) -> &BigInt {
        &self.numerator
    }

    /// Positive reduced canonical denominator.
    pub fn denominator(&self) -> &BigInt {
        &self.denominator
    }
}

/// A reversible public boundary in the source algorithm.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelTightenStage {
    /// Feasible flow exists but the algorithm state is not published yet.
    Ready,
    /// Zero potential and the initial error bound were published.
    Initialize,
    /// A new cancel/tighten phase began.
    BeginPhase,
    /// One concrete residual arc was inspected while building the admissible
    /// graph for cycle detection.
    InspectCycleArc,
    /// One admissible cycle was selected without changing flow.
    SelectCycle,
    /// The selected cycle was saturated by its exact bottleneck.
    CancelCycle,
    /// One concrete residual arc was inspected while building the admissible
    /// DAG for topological ranking.
    InspectRankArc,
    /// The admissible DAG rank tightened the exact potential and error.
    Tighten,
    /// An independent residual certificate proved optimality.
    Optimal,
}

/// Exact deterministic Cancel-and-Tighten counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CancelTightenMetrics {
    /// Outer cancel/tighten phases entered.
    pub phases: u64,
    /// Complete searches for an admissible directed cycle.
    pub cycle_searches: u64,
    /// Admissible cycles saturated.
    pub cancellations: u64,
    /// Admissible-DAG topological rankings.
    pub tightenings: u64,
    /// Positive residual arcs inspected by searches and rankings.
    pub residual_arc_scans: u128,
}

/// Complete exact state at one reversible boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelTightenSnapshot {
    /// Original-edge flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Exact node potentials in canonical node order.
    pub potentials: Vec<CancelTightenRational>,
    /// Exact current epsilon error bound.
    pub epsilon: CancelTightenRational,
    /// Topological rank from the most recent tighten boundary.
    pub ranks: Vec<Option<usize>>,
    /// Exact negative-reduced-cost residual identities.
    pub admissible_arcs: Vec<ResidualArcId>,
    /// Cycle highlighted by the current select/cancel boundary.
    pub active_cycle: Vec<ResidualArcId>,
    /// Concrete source-kernel operand at an arc-inspection boundary.
    pub inspected_arc: Option<ResidualArcId>,
    /// One-based outer phase ordinal, or zero before the first phase.
    pub phase: u64,
    /// Semantic boundary kind.
    pub stage: CancelTightenStage,
    /// Deterministic counters at this boundary.
    pub metrics: CancelTightenMetrics,
}

/// One reversible algorithm event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelTightenTraceEvent {
    /// Stable catalog event identity.
    pub catalog_id: &'static str,
    /// Snapshot before the atomic event.
    pub before: CancelTightenSnapshot,
    /// Snapshot after the atomic event.
    pub after: CancelTightenSnapshot,
    /// Exact cycle bottleneck for a cancellation event.
    pub delta: Option<u64>,
}

/// Certified Cancel-and-Tighten result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelTightenResult {
    /// Original-edge flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Independently reconstructed minimum-cost certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic counters.
    pub metrics: CancelTightenMetrics,
    /// Exact final algorithm state retained for non-trace visualization.
    pub final_snapshot: CancelTightenSnapshot,
}

/// Certified result plus the complete exact reversible trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelTightenTraceResult {
    /// Same result as the fast profile.
    pub result: CancelTightenResult,
    /// Boundary before initialization is published.
    pub base_snapshot: CancelTightenSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<CancelTightenTraceEvent>,
    /// Independently certified optimal boundary.
    pub final_snapshot: CancelTightenSnapshot,
}

/// Cancel-and-Tighten construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CancelTightenError {
    /// Input exceeds the explicit rational interactive band.
    #[error("graph exceeds cancel-and-tighten admission limits")]
    AdmissionLimit,
    /// A deterministic phase, cancellation, or scan ceiling was reached.
    #[error("cancel-and-tighten work limit reached")]
    WorkLimit,
    /// Requested balances are infeasible.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Final independent certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// A purported admissible graph, cycle, or topological order was invalid.
    #[error("cancel-and-tighten invariant failed")]
    Invariant,
    /// A public trace transition did not replay the source algorithm exactly.
    #[error("cancel-and-tighten trace verification failed")]
    TraceVerification,
}

struct WorkingState<'graph> {
    residual: ResidualState<'graph>,
    potentials: Vec<BigRational>,
    epsilon: BigRational,
    ranks: Vec<Option<usize>>,
    active_cycle: Vec<ResidualArcId>,
    inspected_arc: Option<ResidualArcId>,
    phase: u64,
    stage: CancelTightenStage,
    metrics: CancelTightenMetrics,
}

struct InternalRun {
    result: CancelTightenResult,
    base_snapshot: CancelTightenSnapshot,
    events: Vec<CancelTightenTraceEvent>,
    final_snapshot: CancelTightenSnapshot,
}

/// Solves a feasible minimum-cost flow with the bounded exact
/// Cancel-and-Tighten kernel.
///
/// # Errors
///
/// Returns admission, feasibility, residual, work-limit, invariant, or
/// independent certificate failures.
pub fn solve_cancel_and_tighten(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CancelTightenResult, CancelTightenError> {
    solve_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its initial feasible-flow construction to the
/// enclosing execution context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_cancel_and_tighten_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<CancelTightenResult, CancelTightenError> {
    solve_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Solves while retaining every exact rational cancel/tighten boundary.
///
/// # Errors
///
/// Returns the same failures as [`solve_cancel_and_tighten`] plus independent
/// trace verification failures.
pub fn trace_cancel_and_tighten(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CancelTightenTraceResult, CancelTightenError> {
    let run = solve_internal(graph, required_divergence, true)?;
    let traced = CancelTightenTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_cancel_and_tighten_trace(graph, required_divergence, &traced)?;
    Ok(traced)
}

/// Traces Cancel-and-Tighten while explicitly publishing its initial
/// feasible-flow construction to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_cancel_and_tighten_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<CancelTightenTraceResult, CancelTightenError> {
    let run = solve_internal_with_feasibility(graph, required_divergence, true, feasibility)?;
    let traced = CancelTightenTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_cancel_and_tighten_trace(graph, required_divergence, &traced)?;
    Ok(traced)
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_enabled: bool,
) -> Result<InternalRun, CancelTightenError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, required_divergence, trace_enabled, &mut feasibility)
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_enabled: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, CancelTightenError> {
    validate_admission(graph)?;
    let mut work = initial_working_state(graph, required_divergence, feasibility)?;
    validate_snapshot_state(graph, required_divergence, &work)?;
    let base_snapshot = snapshot(&work)?;
    let mut events = Vec::new();

    transition(
        graph,
        required_divergence,
        &mut work,
        trace_enabled,
        &mut events,
        "cancel-and-tighten.initialize",
        None,
        |state| {
            state.stage = CancelTightenStage::Initialize;
            Ok(())
        },
    )?;

    loop {
        let certificate =
            match check_min_cost_flow(graph, required_divergence, work.residual.flows()) {
                Ok(certificate) => Some(certificate),
                Err(CertificateError::NegativeCycle) => None,
                Err(error) => return Err(error.into()),
            };
        if let Some(certificate) = certificate {
            publish_optimal(
                graph,
                required_divergence,
                &mut work,
                trace_enabled,
                &mut events,
            )?;
            let final_snapshot = snapshot(&work)?;
            let result = CancelTightenResult {
                flows: work.residual.flows().to_vec(),
                certificate,
                metrics: work.metrics,
                final_snapshot: final_snapshot.clone(),
            };
            return Ok(InternalRun {
                result,
                base_snapshot,
                events,
                final_snapshot,
            });
        }

        if work.epsilon.is_zero() {
            return Err(CancelTightenError::Invariant);
        }
        begin_phase(
            graph,
            required_divergence,
            &mut work,
            trace_enabled,
            &mut events,
        )?;
        let ranks = cancel_admissible_cycles(
            graph,
            required_divergence,
            &mut work,
            trace_enabled,
            &mut events,
        )?;
        tighten(
            graph,
            required_divergence,
            &mut work,
            trace_enabled,
            &mut events,
            &ranks,
        )?;
    }
}

fn initial_working_state<'graph>(
    graph: &'graph FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<WorkingState<'graph>, CancelTightenError> {
    let feasible =
        feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::InitialFlow)?;
    let maximum_cost = graph
        .edges()
        .iter()
        .map(|edge| BigInt::from(edge.cost()).abs())
        .max()
        .unwrap_or_else(BigInt::zero);
    Ok(WorkingState {
        residual: ResidualState::from_flows(graph, &feasible.flows)?,
        potentials: vec![BigRational::zero(); graph.nodes().len()],
        epsilon: BigRational::from_integer(maximum_cost),
        ranks: vec![None; graph.nodes().len()],
        active_cycle: Vec::new(),
        inspected_arc: None,
        phase: 0,
        stage: CancelTightenStage::Ready,
        metrics: CancelTightenMetrics::default(),
    })
}

fn publish_optimal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    work: &mut WorkingState<'_>,
    trace_enabled: bool,
    events: &mut Vec<CancelTightenTraceEvent>,
) -> Result<(), CancelTightenError> {
    transition(
        graph,
        required_divergence,
        work,
        trace_enabled,
        events,
        "cancel-and-tighten.optimal",
        None,
        |state| {
            state.stage = CancelTightenStage::Optimal;
            state.active_cycle.clear();
            state.inspected_arc = None;
            state.ranks.fill(None);
            Ok(())
        },
    )
}

fn begin_phase(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    work: &mut WorkingState<'_>,
    trace_enabled: bool,
    events: &mut Vec<CancelTightenTraceEvent>,
) -> Result<(), CancelTightenError> {
    transition(
        graph,
        required_divergence,
        work,
        trace_enabled,
        events,
        "cancel-and-tighten.begin-phase",
        None,
        |state| {
            state.phase = state
                .phase
                .checked_add(1)
                .ok_or(CancelTightenError::WorkLimit)?;
            state.metrics.phases = state.phase;
            if state.phase > CANCEL_AND_TIGHTEN_MAX_PHASES {
                return Err(CancelTightenError::WorkLimit);
            }
            state.stage = CancelTightenStage::BeginPhase;
            state.active_cycle.clear();
            state.inspected_arc = None;
            state.ranks.fill(None);
            Ok(())
        },
    )
}

fn cancel_admissible_cycles(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    work: &mut WorkingState<'_>,
    trace_enabled: bool,
    events: &mut Vec<CancelTightenTraceEvent>,
) -> Result<Vec<usize>, CancelTightenError> {
    loop {
        let (cycle, search_arcs) = find_admissible_cycle(work)?;
        publish_residual_arc_inspections(
            graph,
            required_divergence,
            work,
            trace_enabled,
            events,
            CancelTightenStage::InspectCycleArc,
            &search_arcs,
        )?;
        let Some(cycle) = cycle else {
            let (ranks, rank_arcs) = topological_admissible_ranks(work)?;
            publish_residual_arc_inspections(
                graph,
                required_divergence,
                work,
                trace_enabled,
                events,
                CancelTightenStage::InspectRankArc,
                &rank_arcs,
            )?;
            return Ok(ranks);
        };
        transition(
            graph,
            required_divergence,
            work,
            trace_enabled,
            events,
            "cancel-and-tighten.select-admissible-cycle",
            None,
            |state| {
                add_cycle_search(state)?;
                state.stage = CancelTightenStage::SelectCycle;
                state.inspected_arc = None;
                state.active_cycle.clone_from(&cycle);
                Ok(())
            },
        )?;
        let delta = cycle_bottleneck(&work.residual, &cycle)?;
        transition(
            graph,
            required_divergence,
            work,
            trace_enabled,
            events,
            "cancel-and-tighten.cancel-admissible-cycle",
            Some(delta),
            |state| {
                state.residual.augment(&cycle, delta)?;
                state.metrics.cancellations = state
                    .metrics
                    .cancellations
                    .checked_add(1)
                    .ok_or(CancelTightenError::WorkLimit)?;
                if state.metrics.cancellations > CANCEL_AND_TIGHTEN_MAX_CANCELLATIONS {
                    return Err(CancelTightenError::WorkLimit);
                }
                state.stage = CancelTightenStage::CancelCycle;
                state.inspected_arc = None;
                Ok(())
            },
        )?;
    }
}

#[allow(clippy::too_many_arguments)]
fn tighten(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    work: &mut WorkingState<'_>,
    trace_enabled: bool,
    events: &mut Vec<CancelTightenTraceEvent>,
    ranks: &[usize],
) -> Result<(), CancelTightenError> {
    transition(
        graph,
        required_divergence,
        work,
        trace_enabled,
        events,
        "cancel-and-tighten.tighten",
        None,
        |state| {
            add_cycle_search(state)?;
            let divisor = BigRational::from_integer(BigInt::from(graph.nodes().len()));
            let step = state.epsilon.clone() / divisor;
            for (potential, &rank) in state.potentials.iter_mut().zip(ranks) {
                *potential -= step.clone() * BigRational::from_integer(BigInt::from(rank));
            }
            state.epsilon = step
                * BigRational::from_integer(BigInt::from(graph.nodes().len().saturating_sub(1)));
            state.ranks = ranks.iter().copied().map(Some).collect();
            state.active_cycle.clear();
            state.inspected_arc = None;
            state.stage = CancelTightenStage::Tighten;
            state.metrics.tightenings = state
                .metrics
                .tightenings
                .checked_add(1)
                .ok_or(CancelTightenError::WorkLimit)?;
            Ok(())
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn publish_residual_arc_inspections(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    work: &mut WorkingState<'_>,
    trace_enabled: bool,
    events: &mut Vec<CancelTightenTraceEvent>,
    stage: CancelTightenStage,
    inspected_arcs: &[ResidualArcId],
) -> Result<(), CancelTightenError> {
    let catalog_id = match stage {
        CancelTightenStage::InspectCycleArc => "cancel-and-tighten.inspect-cycle-residual-arc",
        CancelTightenStage::InspectRankArc => "cancel-and-tighten.inspect-ranking-residual-arc",
        _ => return Err(CancelTightenError::Invariant),
    };
    if !trace_enabled {
        let scan_count =
            u128::try_from(inspected_arcs.len()).map_err(|_| CancelTightenError::WorkLimit)?;
        return add_scans(work, scan_count);
    }
    for inspected_arc in inspected_arcs {
        transition(
            graph,
            required_divergence,
            work,
            trace_enabled,
            events,
            catalog_id,
            None,
            |state| {
                add_scans(state, 1)?;
                state.stage = stage;
                state.active_cycle.clear();
                state.inspected_arc = Some(inspected_arc.clone());
                Ok(())
            },
        )?;
    }
    Ok(())
}

fn validate_admission(graph: &FlowNetwork) -> Result<(), CancelTightenError> {
    if graph.nodes().is_empty()
        || graph.nodes().len() > CANCEL_AND_TIGHTEN_MAX_NODES
        || graph.edges().len() > CANCEL_AND_TIGHTEN_MAX_EDGES
    {
        return Err(CancelTightenError::AdmissionLimit);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn transition(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    work: &mut WorkingState<'_>,
    trace_enabled: bool,
    events: &mut Vec<CancelTightenTraceEvent>,
    catalog_id: &'static str,
    delta: Option<u64>,
    apply: impl FnOnce(&mut WorkingState<'_>) -> Result<(), CancelTightenError>,
) -> Result<(), CancelTightenError> {
    let before = trace_enabled.then(|| snapshot(work)).transpose()?;
    apply(work)?;
    validate_snapshot_state(graph, required_divergence, work)?;
    if let Some(before) = before {
        events.push(CancelTightenTraceEvent {
            catalog_id,
            before,
            after: snapshot(work)?,
            delta,
        });
    }
    Ok(())
}

fn snapshot(work: &WorkingState<'_>) -> Result<CancelTightenSnapshot, CancelTightenError> {
    Ok(CancelTightenSnapshot {
        flows: work.residual.flows().to_vec(),
        potentials: work
            .potentials
            .iter()
            .map(CancelTightenRational::from_ratio)
            .collect(),
        epsilon: CancelTightenRational::from_ratio(&work.epsilon),
        ranks: work.ranks.clone(),
        admissible_arcs: exact_admissible_arcs(work)?,
        active_cycle: work.active_cycle.clone(),
        inspected_arc: work.inspected_arc.clone(),
        phase: work.phase,
        stage: work.stage,
        metrics: work.metrics,
    })
}

fn validate_snapshot_state(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    work: &WorkingState<'_>,
) -> Result<(), CancelTightenError> {
    if work.potentials.len() != graph.nodes().len()
        || work.ranks.len() != graph.nodes().len()
        || work.epsilon.is_negative()
        || divergences(graph, work.residual.flows())? != required_divergence
    {
        return Err(CancelTightenError::Invariant);
    }
    for node in graph.node_indices() {
        for arc in work.residual.outgoing_arcs(node) {
            let reduced = reduced_cost(&arc, &work.potentials)?;
            if reduced < -work.epsilon.clone() {
                return Err(CancelTightenError::Invariant);
            }
        }
    }
    Ok(())
}

fn reduced_cost(
    arc: &ResidualArc,
    potentials: &[BigRational],
) -> Result<BigRational, CancelTightenError> {
    let from = potentials
        .get(arc.from.as_usize())
        .ok_or(CancelTightenError::Invariant)?;
    let to = potentials
        .get(arc.to.as_usize())
        .ok_or(CancelTightenError::Invariant)?;
    Ok(BigRational::from_integer(BigInt::from(arc.cost)) + from - to)
}

fn exact_admissible_arcs(
    work: &WorkingState<'_>,
) -> Result<Vec<ResidualArcId>, CancelTightenError> {
    let mut arcs = Vec::new();
    for node in work.residual.graph().node_indices() {
        for arc in work.residual.outgoing_arcs(node) {
            if reduced_cost(&arc, &work.potentials)?.is_negative() {
                arcs.push(arc.id);
            }
        }
    }
    arcs.sort_unstable();
    Ok(arcs)
}

fn find_admissible_cycle(
    work: &WorkingState<'_>,
) -> Result<(Option<Vec<ResidualArcId>>, Vec<ResidualArcId>), CancelTightenError> {
    let node_count = work.residual.graph().nodes().len();
    let mut outgoing = vec![Vec::<(ResidualArcId, NodeIndex)>::new(); node_count];
    let mut inspected_arcs = Vec::new();
    for node in work.residual.graph().node_indices() {
        for arc in work.residual.outgoing_arcs(node) {
            inspected_arcs.push(arc.id.clone());
            if reduced_cost(&arc, &work.potentials)?.is_negative() {
                outgoing[node.as_usize()].push((arc.id, arc.to));
            }
        }
        outgoing[node.as_usize()].sort_unstable_by(|left, right| left.0.cmp(&right.0));
    }
    let mut color = vec![0_u8; node_count];
    let mut parent = vec![None::<(NodeIndex, ResidualArcId)>; node_count];
    for root in work.residual.graph().node_indices() {
        if color[root.as_usize()] == 0
            && let Some(cycle) = dfs_cycle(root, &outgoing, &mut color, &mut parent)?
        {
            return Ok((Some(canonical_cycle(cycle)), inspected_arcs));
        }
    }
    Ok((None, inspected_arcs))
}

fn dfs_cycle(
    node: NodeIndex,
    outgoing: &[Vec<(ResidualArcId, NodeIndex)>],
    color: &mut [u8],
    parent: &mut [Option<(NodeIndex, ResidualArcId)>],
) -> Result<Option<Vec<ResidualArcId>>, CancelTightenError> {
    color[node.as_usize()] = 1;
    for (id, to) in &outgoing[node.as_usize()] {
        match color[to.as_usize()] {
            0 => {
                parent[to.as_usize()] = Some((node, id.clone()));
                if let Some(cycle) = dfs_cycle(*to, outgoing, color, parent)? {
                    return Ok(Some(cycle));
                }
            }
            1 => {
                let mut reversed = Vec::new();
                let mut cursor = node;
                while cursor != *to {
                    let (previous, arc) = parent[cursor.as_usize()]
                        .clone()
                        .ok_or(CancelTightenError::Invariant)?;
                    reversed.push(arc);
                    cursor = previous;
                    if reversed.len() > parent.len() {
                        return Err(CancelTightenError::Invariant);
                    }
                }
                reversed.reverse();
                reversed.push(id.clone());
                return Ok(Some(reversed));
            }
            2 => {}
            _ => return Err(CancelTightenError::Invariant),
        }
    }
    color[node.as_usize()] = 2;
    Ok(None)
}

fn canonical_cycle(mut cycle: Vec<ResidualArcId>) -> Vec<ResidualArcId> {
    if let Some((index, _)) = cycle
        .iter()
        .enumerate()
        .min_by(|left, right| left.1.cmp(right.1))
    {
        cycle.rotate_left(index);
    }
    cycle
}

fn cycle_bottleneck(
    residual: &ResidualState<'_>,
    cycle: &[ResidualArcId],
) -> Result<u64, CancelTightenError> {
    cycle
        .iter()
        .map(|id| {
            residual
                .arc(id)
                .filter(|arc| arc.capacity > 0)
                .map(|arc| arc.capacity)
                .ok_or(CancelTightenError::Invariant)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or(CancelTightenError::Invariant)
}

fn topological_admissible_ranks(
    work: &WorkingState<'_>,
) -> Result<(Vec<usize>, Vec<ResidualArcId>), CancelTightenError> {
    let node_count = work.residual.graph().nodes().len();
    let mut outgoing = vec![Vec::<NodeIndex>::new(); node_count];
    let mut indegree = vec![0_usize; node_count];
    let mut inspected_arcs = Vec::new();
    for node in work.residual.graph().node_indices() {
        for arc in work.residual.outgoing_arcs(node) {
            inspected_arcs.push(arc.id.clone());
            if reduced_cost(&arc, &work.potentials)?.is_negative() {
                outgoing[node.as_usize()].push(arc.to);
                indegree[arc.to.as_usize()] = indegree[arc.to.as_usize()]
                    .checked_add(1)
                    .ok_or(CancelTightenError::Invariant)?;
            }
        }
        outgoing[node.as_usize()].sort_unstable();
    }
    let mut ready = BinaryHeap::new();
    for node in work.residual.graph().node_indices() {
        if indegree[node.as_usize()] == 0 {
            ready.push(Reverse(node));
        }
    }
    let mut ranks = vec![usize::MAX; node_count];
    let mut ordinal = 0_usize;
    while let Some(Reverse(node)) = ready.pop() {
        ranks[node.as_usize()] = ordinal;
        ordinal = ordinal
            .checked_add(1)
            .ok_or(CancelTightenError::Invariant)?;
        for &to in &outgoing[node.as_usize()] {
            indegree[to.as_usize()] = indegree[to.as_usize()]
                .checked_sub(1)
                .ok_or(CancelTightenError::Invariant)?;
            if indegree[to.as_usize()] == 0 {
                ready.push(Reverse(to));
            }
        }
    }
    if ordinal != node_count {
        return Err(CancelTightenError::Invariant);
    }
    Ok((ranks, inspected_arcs))
}

fn add_cycle_search(work: &mut WorkingState<'_>) -> Result<(), CancelTightenError> {
    work.metrics.cycle_searches = work
        .metrics
        .cycle_searches
        .checked_add(1)
        .ok_or(CancelTightenError::WorkLimit)?;
    Ok(())
}

fn add_scans(work: &mut WorkingState<'_>, scans: u128) -> Result<(), CancelTightenError> {
    work.metrics.residual_arc_scans = work
        .metrics
        .residual_arc_scans
        .checked_add(scans)
        .ok_or(CancelTightenError::WorkLimit)?;
    if work.metrics.residual_arc_scans > CANCEL_AND_TIGHTEN_MAX_RESIDUAL_ARC_SCANS {
        return Err(CancelTightenError::WorkLimit);
    }
    Ok(())
}

/// Independently checks every public transition from graph data and the trace.
///
/// # Errors
///
/// Rejects discontinuity, a wrong admissible set, an invalid cycle
/// cancellation, a wrong topological tightening, or final-result mismatch.
pub fn check_cancel_and_tighten_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace: &CancelTightenTraceResult,
) -> Result<(), CancelTightenError> {
    if trace.events.is_empty()
        || trace.base_snapshot.stage != CancelTightenStage::Ready
        || trace.final_snapshot.stage != CancelTightenStage::Optimal
        || trace.final_snapshot.flows != trace.result.flows
        || trace.final_snapshot.metrics != trace.result.metrics
        || trace.final_snapshot != trace.result.final_snapshot
    {
        return Err(CancelTightenError::TraceVerification);
    }
    let mut current = trace.base_snapshot.clone();
    validate_public_snapshot(graph, required_divergence, &current)?;
    for event in &trace.events {
        if event.before != current {
            return Err(CancelTightenError::TraceVerification);
        }
        validate_public_snapshot(graph, required_divergence, &event.after)?;
        validate_public_transition(graph, event)?;
        current = event.after.clone();
    }
    validate_inspection_sequences(graph, &trace.events)?;
    if current != trace.final_snapshot {
        return Err(CancelTightenError::TraceVerification);
    }
    let certificate = check_min_cost_flow(graph, required_divergence, &trace.result.flows)?;
    if certificate != trace.result.certificate {
        return Err(CancelTightenError::TraceVerification);
    }
    Ok(())
}

fn validate_inspection_sequences(
    graph: &FlowNetwork,
    events: &[CancelTightenTraceEvent],
) -> Result<(), CancelTightenError> {
    let mut index = 0_usize;
    while index < events.len() {
        let stage = events[index].after.stage;
        if !matches!(
            stage,
            CancelTightenStage::InspectCycleArc | CancelTightenStage::InspectRankArc
        ) {
            index += 1;
            continue;
        }
        let residual = ResidualState::from_flows(graph, &events[index].before.flows)?;
        let expected = graph
            .node_indices()
            .flat_map(|node| residual.outgoing_arcs(node))
            .map(|arc| arc.id)
            .collect::<Vec<_>>();
        let group_start = index;
        while index < events.len() && events[index].after.stage == stage {
            index += 1;
        }
        let actual = events[group_start..index]
            .iter()
            .map(|event| {
                event
                    .after
                    .inspected_arc
                    .clone()
                    .ok_or(CancelTightenError::TraceVerification)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if actual != expected {
            return Err(CancelTightenError::TraceVerification);
        }
        match stage {
            CancelTightenStage::InspectCycleArc => {
                if events.get(index).is_none_or(|event| {
                    !matches!(
                        event.after.stage,
                        CancelTightenStage::SelectCycle | CancelTightenStage::InspectRankArc
                    )
                }) {
                    return Err(CancelTightenError::TraceVerification);
                }
            }
            CancelTightenStage::InspectRankArc => {
                if events
                    .get(index)
                    .is_none_or(|event| event.after.stage != CancelTightenStage::Tighten)
                {
                    return Err(CancelTightenError::TraceVerification);
                }
            }
            _ => unreachable!("inspection stages were filtered above"),
        }
    }
    Ok(())
}

fn validate_public_snapshot(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    snapshot: &CancelTightenSnapshot,
) -> Result<(), CancelTightenError> {
    if snapshot.potentials.len() != graph.nodes().len()
        || snapshot.ranks.len() != graph.nodes().len()
        || snapshot.epsilon.denominator <= BigInt::zero()
        || snapshot.epsilon.numerator < BigInt::zero()
        || CancelTightenRational::from_ratio(&snapshot.epsilon.to_ratio()) != snapshot.epsilon
        || snapshot
            .potentials
            .iter()
            .any(|value| CancelTightenRational::from_ratio(&value.to_ratio()) != *value)
        || divergences(graph, &snapshot.flows)? != required_divergence
    {
        return Err(CancelTightenError::TraceVerification);
    }
    let residual = ResidualState::from_flows(graph, &snapshot.flows)?;
    if snapshot.inspected_arc.as_ref().is_some_and(|arc| {
        residual
            .arc(arc)
            .is_none_or(|residual_arc| residual_arc.capacity == 0)
    }) {
        return Err(CancelTightenError::TraceVerification);
    }
    let potentials = snapshot
        .potentials
        .iter()
        .map(CancelTightenRational::to_ratio)
        .collect::<Vec<_>>();
    let epsilon = snapshot.epsilon.to_ratio();
    let mut expected = Vec::new();
    for node in graph.node_indices() {
        for arc in residual.outgoing_arcs(node) {
            let reduced = reduced_cost(&arc, &potentials)?;
            if reduced < -epsilon.clone() {
                return Err(CancelTightenError::TraceVerification);
            }
            if reduced.is_negative() {
                expected.push(arc.id);
            }
        }
    }
    expected.sort_unstable();
    if expected != snapshot.admissible_arcs {
        return Err(CancelTightenError::TraceVerification);
    }
    Ok(())
}

fn validate_public_transition(
    graph: &FlowNetwork,
    event: &CancelTightenTraceEvent,
) -> Result<(), CancelTightenError> {
    let expected_catalog_id = match event.after.stage {
        CancelTightenStage::Ready => return Err(CancelTightenError::TraceVerification),
        CancelTightenStage::Initialize => "cancel-and-tighten.initialize",
        CancelTightenStage::BeginPhase => "cancel-and-tighten.begin-phase",
        CancelTightenStage::InspectCycleArc => "cancel-and-tighten.inspect-cycle-residual-arc",
        CancelTightenStage::SelectCycle => "cancel-and-tighten.select-admissible-cycle",
        CancelTightenStage::CancelCycle => "cancel-and-tighten.cancel-admissible-cycle",
        CancelTightenStage::InspectRankArc => "cancel-and-tighten.inspect-ranking-residual-arc",
        CancelTightenStage::Tighten => "cancel-and-tighten.tighten",
        CancelTightenStage::Optimal => "cancel-and-tighten.optimal",
    };
    if event.catalog_id != expected_catalog_id {
        return Err(CancelTightenError::TraceVerification);
    }
    match event.after.stage {
        CancelTightenStage::Ready => Err(CancelTightenError::TraceVerification),
        CancelTightenStage::Initialize
        | CancelTightenStage::BeginPhase
        | CancelTightenStage::Optimal => {
            if event.before.flows != event.after.flows
                || event.before.potentials != event.after.potentials
                || event.before.epsilon != event.after.epsilon
            {
                return Err(CancelTightenError::TraceVerification);
            }
            Ok(())
        }
        CancelTightenStage::InspectCycleArc | CancelTightenStage::InspectRankArc => {
            validate_inspection_transition(graph, event)
        }
        CancelTightenStage::SelectCycle => {
            if event.before.flows != event.after.flows
                || event.before.potentials != event.after.potentials
                || event.before.epsilon != event.after.epsilon
                || event.after.inspected_arc.is_some()
            {
                return Err(CancelTightenError::TraceVerification);
            }
            validate_selected_cycle(graph, &event.before, &event.after.active_cycle)
        }
        CancelTightenStage::CancelCycle => validate_cancel_transition(graph, event),
        CancelTightenStage::Tighten => validate_tighten_transition(graph, event),
    }
}

fn validate_inspection_transition(
    graph: &FlowNetwork,
    event: &CancelTightenTraceEvent,
) -> Result<(), CancelTightenError> {
    if event.before.flows != event.after.flows
        || event.before.potentials != event.after.potentials
        || event.before.epsilon != event.after.epsilon
        || event.before.ranks != event.after.ranks
        || !event.after.active_cycle.is_empty()
        || event.delta.is_some()
    {
        return Err(CancelTightenError::TraceVerification);
    }
    let inspected = event
        .after
        .inspected_arc
        .as_ref()
        .ok_or(CancelTightenError::TraceVerification)?;
    let residual = ResidualState::from_flows(graph, &event.before.flows)?;
    if residual.arc(inspected).is_none_or(|arc| arc.capacity == 0) {
        return Err(CancelTightenError::TraceVerification);
    }
    let mut expected_metrics = event.before.metrics;
    expected_metrics.residual_arc_scans = expected_metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(CancelTightenError::TraceVerification)?;
    if event.after.metrics != expected_metrics {
        return Err(CancelTightenError::TraceVerification);
    }
    Ok(())
}

fn validate_selected_cycle(
    graph: &FlowNetwork,
    snapshot: &CancelTightenSnapshot,
    cycle: &[ResidualArcId],
) -> Result<(), CancelTightenError> {
    if cycle.is_empty() {
        return Err(CancelTightenError::TraceVerification);
    }
    if canonical_cycle(cycle.to_vec()) != cycle {
        return Err(CancelTightenError::TraceVerification);
    }
    let residual = ResidualState::from_flows(graph, &snapshot.flows)?;
    let admissible = snapshot
        .admissible_arcs
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut expected_from = None;
    let mut first_from = None;
    for id in cycle {
        if !admissible.contains(id) {
            return Err(CancelTightenError::TraceVerification);
        }
        let arc = residual
            .arc(id)
            .filter(|arc| arc.capacity > 0)
            .ok_or(CancelTightenError::TraceVerification)?;
        if expected_from.is_some_and(|node| node != arc.from) {
            return Err(CancelTightenError::TraceVerification);
        }
        first_from.get_or_insert(arc.from);
        expected_from = Some(arc.to);
    }
    if expected_from != first_from {
        return Err(CancelTightenError::TraceVerification);
    }
    Ok(())
}

fn validate_cancel_transition(
    graph: &FlowNetwork,
    event: &CancelTightenTraceEvent,
) -> Result<(), CancelTightenError> {
    if event.before.potentials != event.after.potentials
        || event.before.epsilon != event.after.epsilon
        || event.before.active_cycle != event.after.active_cycle
        || event.after.inspected_arc.is_some()
    {
        return Err(CancelTightenError::TraceVerification);
    }
    validate_selected_cycle(graph, &event.before, &event.before.active_cycle)?;
    let mut residual = ResidualState::from_flows(graph, &event.before.flows)?;
    let delta = event.delta.ok_or(CancelTightenError::TraceVerification)?;
    if cycle_bottleneck(&residual, &event.before.active_cycle)? != delta {
        return Err(CancelTightenError::TraceVerification);
    }
    residual.augment(&event.before.active_cycle, delta)?;
    if residual.flows() != event.after.flows {
        return Err(CancelTightenError::TraceVerification);
    }
    Ok(())
}

fn validate_tighten_transition(
    graph: &FlowNetwork,
    event: &CancelTightenTraceEvent,
) -> Result<(), CancelTightenError> {
    if event.before.flows != event.after.flows
        || event.after.ranks.iter().any(Option::is_none)
        || event.after.inspected_arc.is_some()
        || event.delta.is_some()
    {
        return Err(CancelTightenError::TraceVerification);
    }
    let residual = ResidualState::from_flows(graph, &event.before.flows)?;
    let before_potentials = event
        .before
        .potentials
        .iter()
        .map(CancelTightenRational::to_ratio)
        .collect::<Vec<_>>();
    let ranks = event
        .after
        .ranks
        .iter()
        .map(|rank| rank.ok_or(CancelTightenError::TraceVerification))
        .collect::<Result<Vec<_>, _>>()?;
    if ranks
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != graph.nodes().len()
        || ranks.iter().any(|&rank| rank >= graph.nodes().len())
    {
        return Err(CancelTightenError::TraceVerification);
    }
    for node in graph.node_indices() {
        for arc in residual.outgoing_arcs(node) {
            if reduced_cost(&arc, &before_potentials)?.is_negative()
                && ranks[arc.from.as_usize()] >= ranks[arc.to.as_usize()]
            {
                return Err(CancelTightenError::TraceVerification);
            }
        }
    }
    let before_epsilon = event.before.epsilon.to_ratio();
    let divisor = BigRational::from_integer(BigInt::from(graph.nodes().len()));
    let step = before_epsilon.clone() / divisor;
    let expected_epsilon = step.clone()
        * BigRational::from_integer(BigInt::from(graph.nodes().len().saturating_sub(1)));
    if event.after.epsilon.to_ratio() != expected_epsilon {
        return Err(CancelTightenError::TraceVerification);
    }
    for (index, before) in before_potentials.iter().enumerate() {
        let expected =
            before.clone() - step.clone() * BigRational::from_integer(BigInt::from(ranks[index]));
        if event.after.potentials[index].to_ratio() != expected {
            return Err(CancelTightenError::TraceVerification);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_minimum_mean_cycle_canceling;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;

    fn network(nodes: &[(&str, i64)], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|&(id, supply)| FlowNode::new(NodeId::parse(id).expect("node"), supply))
                .collect(),
            edges
                .iter()
                .map(
                    |&(id, from, to, lower, capacity, cost)| UnresolvedFlowEdge {
                        id: EdgeId::parse(id).expect("edge"),
                        from: NodeId::parse(from).expect("tail"),
                        to: NodeId::parse(to).expect("head"),
                        lower,
                        capacity,
                        cost,
                    },
                )
                .collect(),
        )
        .expect("network")
    }

    fn target(graph: &FlowNetwork) -> Vec<i128> {
        graph
            .nodes()
            .iter()
            .map(|node| i128::from(node.supply()))
            .collect()
    }

    #[test]
    fn cancels_admissible_cycles_then_tightens_with_exact_rational_rank() {
        let graph = network(
            &[("a", 0), ("b", 0), ("c", 0)],
            &[
                ("ab", "a", "b", 0, 3, -4),
                ("bc", "b", "c", 0, 3, 1),
                ("ca", "c", "a", 0, 3, 1),
                ("ac", "a", "c", 0, 2, 2),
            ],
        );
        let traced = trace_cancel_and_tighten(&graph, &target(&graph)).expect("trace");
        let fast = solve_cancel_and_tighten(&graph, &target(&graph)).expect("fast");

        assert_eq!(traced.result, fast);
        assert_eq!(traced.result.certificate.total_cost, -6);
        assert!(traced.result.metrics.cancellations > 0);
        assert!(traced.result.metrics.tightenings > 0);
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.after.stage == CancelTightenStage::SelectCycle)
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.after.stage == CancelTightenStage::Tighten)
        );
        let inspection_events = traced
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event.after.stage,
                    CancelTightenStage::InspectCycleArc | CancelTightenStage::InspectRankArc
                )
            })
            .collect::<Vec<_>>();
        assert!(!inspection_events.is_empty());
        assert_eq!(
            u128::try_from(inspection_events.len()).expect("inspection count"),
            traced.result.metrics.residual_arc_scans
        );
        for event in inspection_events {
            assert!(event.after.inspected_arc.is_some());
            assert_eq!(
                event.after.metrics.residual_arc_scans,
                event.before.metrics.residual_arc_scans + 1
            );
        }
        check_cancel_and_tighten_trace(&graph, &target(&graph), &traced).expect("checked trace");
    }

    #[test]
    fn supports_lower_bounds_supplies_parallel_arcs_and_self_loops() {
        let graph = network(
            &[("s", 2), ("t", -2)],
            &[
                ("cheap", "s", "t", 1, 3, -2),
                ("parallel", "s", "t", 0, 3, 4),
                ("return", "t", "s", 0, 2, 1),
                ("loop", "s", "s", 0, 1, -1),
            ],
        );
        let result = solve_cancel_and_tighten(&graph, &target(&graph)).expect("solve");
        check_min_cost_flow(&graph, &target(&graph), &result.flows).expect("certificate");
        assert_eq!(result.certificate.total_cost, -6);
    }

    #[test]
    fn deterministic_small_graphs_match_minimum_mean_cycle_canceling() {
        let mut seed = 0x7a51_19d3_u64;
        for case in 0..32 {
            let node_count = 2 + usize::try_from(next(&mut seed) % 4).expect("small");
            let nodes = (0..node_count)
                .map(|index| (format!("v{index}"), 0_i64))
                .collect::<Vec<_>>();
            let mut edges = Vec::new();
            for from in 0..node_count {
                for to in 0..node_count {
                    if next(&mut seed).is_multiple_of(3) {
                        continue;
                    }
                    edges.push((
                        format!("e{from}_{to}"),
                        format!("v{from}"),
                        format!("v{to}"),
                        0,
                        1 + next(&mut seed) % 3,
                        i64::try_from(next(&mut seed) % 9).expect("cost") - 4,
                    ));
                }
            }
            if edges.is_empty() {
                edges.push((
                    "fallback".to_owned(),
                    "v0".to_owned(),
                    "v1".to_owned(),
                    0,
                    1,
                    0,
                ));
            }
            let graph = FlowNetwork::new(
                nodes
                    .iter()
                    .map(|(id, supply)| FlowNode::new(NodeId::parse(id).expect("node"), *supply))
                    .collect(),
                edges
                    .iter()
                    .map(|(id, from, to, lower, capacity, cost)| UnresolvedFlowEdge {
                        id: EdgeId::parse(id).expect("edge"),
                        from: NodeId::parse(from).expect("tail"),
                        to: NodeId::parse(to).expect("head"),
                        lower: *lower,
                        capacity: *capacity,
                        cost: *cost,
                    })
                    .collect(),
            )
            .expect("graph");
            let target = vec![0_i128; node_count];
            let expected = solve_minimum_mean_cycle_canceling(&graph, &target)
                .unwrap_or_else(|error| panic!("MMCC case {case}: {error}"));
            let actual = solve_cancel_and_tighten(&graph, &target)
                .unwrap_or_else(|error| panic!("C&T case {case}: {error}"));
            assert_eq!(
                actual.certificate.total_cost, expected.certificate.total_cost,
                "case {case}"
            );
        }
    }

    #[test]
    fn trace_checker_rejects_corrupted_tighten_rank() {
        let graph = network(
            &[("a", 0), ("b", 0), ("c", 0)],
            &[
                ("ab", "a", "b", 0, 2, -3),
                ("bc", "b", "c", 0, 2, 1),
                ("ca", "c", "a", 0, 2, 1),
            ],
        );
        let target = target(&graph);
        let mut traced = trace_cancel_and_tighten(&graph, &target).expect("trace");
        let tighten = traced
            .events
            .iter_mut()
            .find(|event| event.after.stage == CancelTightenStage::Tighten)
            .expect("tighten event");
        tighten.after.ranks.swap(0, 1);
        assert_eq!(
            check_cancel_and_tighten_trace(&graph, &target, &traced),
            Err(CancelTightenError::TraceVerification)
        );
    }

    #[test]
    fn trace_checker_rejects_a_plausible_but_wrong_inspected_arc() {
        let graph = network(
            &[("a", 0), ("b", 0), ("c", 0)],
            &[
                ("ab", "a", "b", 0, 2, -3),
                ("bc", "b", "c", 0, 2, 1),
                ("ca", "c", "a", 0, 2, 1),
            ],
        );
        let target = target(&graph);
        let mut traced = trace_cancel_and_tighten(&graph, &target).expect("trace");
        let inspected = traced
            .events
            .iter_mut()
            .find(|event| event.after.stage == CancelTightenStage::InspectCycleArc)
            .expect("cycle scan event");
        let residual =
            ResidualState::from_flows(&graph, &inspected.before.flows).expect("residual state");
        let replacement = graph
            .node_indices()
            .flat_map(|node| residual.outgoing_arcs(node))
            .map(|arc| arc.id)
            .find(|arc| Some(arc) != inspected.after.inspected_arc.as_ref())
            .expect("a different valid residual arc");
        inspected.after.inspected_arc = Some(replacement);
        assert_eq!(
            check_cancel_and_tighten_trace(&graph, &target, &traced),
            Err(CancelTightenError::TraceVerification)
        );
    }

    fn next(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *seed
    }
}
