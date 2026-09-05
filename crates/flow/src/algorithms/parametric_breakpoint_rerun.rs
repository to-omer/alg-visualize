//! Exact bounded complete parametric analysis by cold static reruns.
//!
//! Hochbaum's source algorithm retains and renormalizes a normalized tree
//! between recursive subproblems. This independent oracle preserves the same
//! monotone-terminal-capacity model, nested-cut theorem, minimal/maximal cut
//! endpoints, and exact cut-function intersection recursion, but deliberately
//! starts the bounded explicit-tree pseudoflow from scratch at every sampled
//! parameter. It is a separate variant with complexity `O(q T_PF(n, m))`, not
//! Hochbaum's parametric pseudoflow implementation.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};
use thiserror::Error;

use crate::algorithms::{
    EdmondsKarpResult, solve_edmonds_karp, solve_hochbaum_pseudoflow_with_feasibility,
    trace_edmonds_karp, trace_hochbaum_pseudoflow_with_feasibility,
};
use crate::feasibility::FeasibilityExecution;
use crate::model::{EdgeId, FlowNetwork, NodeId, NodeIndex, UnresolvedFlowEdge};
use crate::residual::ResidualState;
use crate::trace::{
    FlowTraceDirection, FlowTraceEntityRef, FlowTraceEvent, FlowTraceSnapshot, apply_trace_event,
};

/// Conservative node admission for exact breakpoint visualization.
pub const PARAMETRIC_BREAKPOINT_RERUN_MAX_NODES: usize = 64;
/// Conservative edge admission for exact breakpoint visualization.
pub const PARAMETRIC_BREAKPOINT_RERUN_MAX_EDGES: usize = 512;
/// Maximum static pseudoflow subproblems in one complete analysis.
pub const PARAMETRIC_BREAKPOINT_RERUN_MAX_SUBPROBLEMS: u64 = 256;
/// Maximum decimal digits in one user-supplied rational component or slope.
pub const PARAMETRIC_BREAKPOINT_RERUN_MAX_DECIMAL_DIGITS: usize = 128;
/// Preserve every early residual-arc inspection before geometric sampling.
const PARAMETRIC_BREAKPOINT_RERUN_TRACE_SCAN_PREFIX: u128 = 512;
/// Maximum exact residual-arc scans represented by one later Detail boundary.
const PARAMETRIC_BREAKPOINT_RERUN_TRACE_SCAN_BLOCK_MAX: u128 = 256;

/// A normalized arbitrary-precision rational parameter.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ParametricRational(BigRational);

impl ParametricRational {
    /// Creates a normalized rational with a nonzero denominator.
    ///
    /// # Errors
    ///
    /// Rejects a zero denominator.
    pub fn new(
        numerator: BigInt,
        denominator: BigInt,
    ) -> Result<Self, ParametricBreakpointRerunError> {
        if denominator.is_zero() {
            return Err(ParametricBreakpointRerunError::InvalidInterval);
        }
        Ok(Self(BigRational::new(numerator, denominator)))
    }

    /// Creates an integral parameter value.
    #[must_use]
    pub fn from_integer(value: BigInt) -> Self {
        Self(BigRational::from_integer(value))
    }

    /// Returns the normalized signed numerator.
    #[must_use]
    pub fn numerator(&self) -> &BigInt {
        self.0.numer()
    }

    /// Returns the positive normalized denominator.
    #[must_use]
    pub fn denominator(&self) -> &BigInt {
        self.0.denom()
    }

    /// Returns `numerator/denominator`, including `/1` for integers.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!("{}/{}", self.numerator(), self.denominator())
    }

    fn inner(&self) -> &BigRational {
        &self.0
    }
}

/// One declared affine capacity coefficient keyed by stable edge identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParametricCapacitySlope {
    /// Stable original edge identity.
    pub edge: EdgeId,
    /// Signed integral coefficient in `capacity(lambda) = capacity(0) + slope * lambda`.
    pub slope: BigInt,
}

/// Validated monotone affine terminal-capacity problem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParametricMaxFlowProblem {
    source: NodeIndex,
    sink: NodeIndex,
    minimum: ParametricRational,
    maximum: ParametricRational,
    slopes: Vec<BigInt>,
}

impl ParametricMaxFlowProblem {
    /// Validates the complete parametric declaration without applying the
    /// executable subproblem-size admission limit.
    ///
    /// This linear-space path is used before an oversized Scenario may be
    /// classified as a resource-limit result.
    ///
    /// # Errors
    ///
    /// Rejects the same semantic and exact-number violations as [`Self::new`].
    pub fn validate_declaration(
        graph: &FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        minimum: ParametricRational,
        maximum: ParametricRational,
        coefficients: Vec<ParametricCapacitySlope>,
    ) -> Result<(), ParametricBreakpointRerunError> {
        Self::from_validated_declaration(graph, source, sink, minimum, maximum, coefficients)
            .map(|_| ())
    }

    /// Validates the source-defined monotone parametric maximum-flow model.
    ///
    /// Nonzero coefficients are accepted only on arcs leaving the source or
    /// entering the sink. Source coefficients must be positive and sink
    /// coefficients negative. Every capacity multiplied by the parameter's
    /// normalized denominator must remain in the `u64` static-solver domain.
    /// Lower bounds, costs, supplies, and terminal self-loops are rejected.
    ///
    /// # Errors
    ///
    /// Rejects invalid terminals, interval, coefficients, capacities, graph
    /// semantics, duplicate declarations, or bounded admission.
    pub fn new(
        graph: &FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        minimum: ParametricRational,
        maximum: ParametricRational,
        coefficients: Vec<ParametricCapacitySlope>,
    ) -> Result<Self, ParametricBreakpointRerunError> {
        if graph.nodes().len() > PARAMETRIC_BREAKPOINT_RERUN_MAX_NODES
            || graph.edges().len() > PARAMETRIC_BREAKPOINT_RERUN_MAX_EDGES
        {
            return Err(ParametricBreakpointRerunError::AdmissionLimit);
        }
        Self::from_validated_declaration(graph, source, sink, minimum, maximum, coefficients)
    }

    fn from_validated_declaration(
        graph: &FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        minimum: ParametricRational,
        maximum: ParametricRational,
        coefficients: Vec<ParametricCapacitySlope>,
    ) -> Result<Self, ParametricBreakpointRerunError> {
        if source == sink || graph.node(source).is_none() || graph.node(sink).is_none() {
            return Err(ParametricBreakpointRerunError::InvalidTerminals);
        }
        if minimum > maximum {
            return Err(ParametricBreakpointRerunError::InvalidInterval);
        }
        if rational_exceeds_admission(&minimum) || rational_exceeds_admission(&maximum) {
            return Err(ParametricBreakpointRerunError::AdmissionLimit);
        }
        if !graph.is_plain_max_flow_network() || graph.nodes().iter().any(|node| node.supply() != 0)
        {
            return Err(ParametricBreakpointRerunError::UnsupportedGraph);
        }

        let mut slopes = vec![BigInt::zero(); graph.edges().len()];
        let mut declared = BTreeMap::new();
        for coefficient in coefficients {
            if bigint_decimal_digits(&coefficient.slope)
                > PARAMETRIC_BREAKPOINT_RERUN_MAX_DECIMAL_DIGITS
            {
                return Err(ParametricBreakpointRerunError::AdmissionLimit);
            }
            if coefficient.slope.is_zero() {
                return Err(ParametricBreakpointRerunError::ZeroCoefficient);
            }
            let edge_index = graph
                .edge_index(&coefficient.edge)
                .ok_or(ParametricBreakpointRerunError::MissingEdge)?;
            if declared
                .insert(coefficient.edge.clone(), coefficient.slope.clone())
                .is_some()
            {
                return Err(ParametricBreakpointRerunError::DuplicateCoefficient);
            }
            let edge = graph
                .edge(edge_index)
                .ok_or(ParametricBreakpointRerunError::Invariant)?;
            let leaves_source = edge.from() == source && edge.to() != source;
            let enters_sink = edge.to() == sink && edge.from() != sink;
            if leaves_source && enters_sink {
                return Err(ParametricBreakpointRerunError::InvalidCoefficientLocation);
            }
            if leaves_source {
                if !coefficient.slope.is_positive() {
                    return Err(ParametricBreakpointRerunError::InvalidMonotonicity);
                }
            } else if enters_sink {
                if !coefficient.slope.is_negative() {
                    return Err(ParametricBreakpointRerunError::InvalidMonotonicity);
                }
            } else {
                return Err(ParametricBreakpointRerunError::InvalidCoefficientLocation);
            }
            slopes[edge_index.as_usize()] = coefficient.slope;
        }

        let problem = Self {
            source,
            sink,
            minimum,
            maximum,
            slopes,
        };
        problem.validate_endpoint_capacities(graph)?;
        Ok(problem)
    }

    /// Returns the source terminal.
    #[must_use]
    pub const fn source(&self) -> NodeIndex {
        self.source
    }

    /// Returns the sink terminal.
    #[must_use]
    pub const fn sink(&self) -> NodeIndex {
        self.sink
    }

    /// Returns the inclusive lower parameter endpoint.
    #[must_use]
    pub const fn minimum(&self) -> &ParametricRational {
        &self.minimum
    }

    /// Returns the inclusive upper parameter endpoint.
    #[must_use]
    pub const fn maximum(&self) -> &ParametricRational {
        &self.maximum
    }

    /// Returns the coefficient aligned with canonical original-edge order.
    #[must_use]
    pub fn slope(&self, edge_position: usize) -> Option<&BigInt> {
        self.slopes.get(edge_position)
    }

    /// Evaluates one original edge's exact affine capacity.
    ///
    /// # Errors
    ///
    /// Rejects a graph/index mismatch or a negative evaluated capacity.
    pub fn capacity_at(
        &self,
        graph: &FlowNetwork,
        edge_position: usize,
        parameter: &ParametricRational,
    ) -> Result<ParametricRational, ParametricBreakpointRerunError> {
        let edge = graph
            .edges()
            .get(edge_position)
            .ok_or(ParametricBreakpointRerunError::MissingEdge)?;
        let slope = self
            .slopes
            .get(edge_position)
            .ok_or(ParametricBreakpointRerunError::MissingEdge)?;
        let capacity = evaluate_affine(&BigInt::from(edge.capacity()), slope, parameter);
        if capacity.is_negative() {
            return Err(ParametricBreakpointRerunError::CapacityDomain);
        }
        Ok(ParametricRational(capacity))
    }

    /// Returns the fixed exact maximum edge capacity over both interval
    /// endpoints. Affinity makes this sufficient for stable visual scaling.
    ///
    /// # Errors
    ///
    /// Rejects a graph mismatch or an invalid evaluated capacity.
    pub fn visual_scale_max_capacity(
        &self,
        graph: &FlowNetwork,
    ) -> Result<ParametricRational, ParametricBreakpointRerunError> {
        graph.edges().iter().enumerate().try_fold(
            ParametricRational::from_integer(BigInt::zero()),
            |maximum, (index, _)| {
                let lower = self.capacity_at(graph, index, &self.minimum)?;
                let upper = self.capacity_at(graph, index, &self.maximum)?;
                Ok(maximum.max(lower).max(upper))
            },
        )
    }

    fn validate_endpoint_capacities(
        &self,
        graph: &FlowNetwork,
    ) -> Result<(), ParametricBreakpointRerunError> {
        for parameter in [&self.minimum, &self.maximum] {
            for (position, edge) in graph.edges().iter().enumerate() {
                let capacity = evaluate_affine(
                    &BigInt::from(edge.capacity()),
                    &self.slopes[position],
                    parameter,
                );
                if capacity.is_negative() {
                    return Err(ParametricBreakpointRerunError::CapacityDomain);
                }
                let scaled = capacity * BigRational::from_integer(parameter.denominator().clone());
                if !scaled.is_integer() {
                    return Err(ParametricBreakpointRerunError::CapacityDomain);
                }
                scaled
                    .to_integer()
                    .to_u64()
                    .ok_or(ParametricBreakpointRerunError::CapacityDomain)?;
            }
        }
        Ok(())
    }
}

/// One exact affine minimum-cut expression on a complete-analysis interval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParametricCut {
    /// Canonical node identities on the source side.
    pub source_side: Vec<NodeId>,
    /// Constant term of the cut-capacity expression.
    pub intercept: BigInt,
    /// Parameter coefficient of the cut-capacity expression.
    pub slope: BigInt,
}

/// One value interval and its complete open-interior minimum-cut extrema.
/// For a one-point domain, the extrema describe that exact point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParametricSegment {
    /// Lower endpoint of the interval; the value expression is valid here.
    pub lower: ParametricRational,
    /// Upper endpoint of the interval; the value expression is valid here.
    pub upper: ParametricRational,
    /// Inclusion-minimal source-side cut throughout the open interior.
    pub minimal_cut: ParametricCut,
    /// Inclusion-maximal source-side cut throughout the open interior.
    ///
    /// It has the same affine expression as `minimal_cut`. A different source
    /// side exposes a degenerate tie interval instead of silently selecting one
    /// representative cut.
    pub maximal_cut: ParametricCut,
}

/// One exact value where the canonical nested source set changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParametricBreakpoint {
    /// Exact rational parameter.
    pub parameter: ParametricRational,
    /// Inclusion-minimal source side on the open interval immediately before.
    pub before_source_side: Vec<NodeId>,
    /// Inclusion-minimal source side on the open interval immediately after.
    pub after_source_side: Vec<NodeId>,
    /// Inclusion-minimal source side exactly at the breakpoint.
    pub exact_minimal_source_side: Vec<NodeId>,
    /// Inclusion-maximal source side exactly at the breakpoint.
    pub exact_maximal_source_side: Vec<NodeId>,
    /// Nodes in the exact tie span, published as one atomic transition.
    ///
    /// Intermediate subsets are not claimed to be minimum cuts.
    pub entering_nodes: Vec<NodeId>,
}

/// Exact counters for the bounded explicit-rerun implementation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParametricBreakpointRerunMetrics {
    /// Static Hochbaum pseudoflow runs.
    pub pseudoflow_runs: u64,
    /// Independent Edmonds–Karp runs during construction and certification.
    pub oracle_runs: u64,
    /// Residual arcs inspected inside every cold pseudoflow and Edmonds–Karp run.
    pub static_residual_arc_scans: u128,
    /// Exact affine cut intersections considered.
    pub intersections: u64,
    /// Recursive subintervals processed.
    pub subproblems: u64,
    /// Final optimal intervals.
    pub segments: u64,
    /// Distinct internal breakpoints.
    pub breakpoints: u64,
    /// Breakpoints where more than one node enters.
    pub simultaneous_breakpoints: u64,
    /// Maximum recursive depth.
    pub maximum_depth: u64,
}

/// Complete exact parametric min-cut analysis.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParametricBreakpointRerunResult {
    /// Closed intervals covering the entire declared parameter domain.
    pub segments: Vec<ParametricSegment>,
    /// Internal source-set transitions in strictly increasing order.
    pub breakpoints: Vec<ParametricBreakpoint>,
    /// Exact bounded implementation counters.
    pub metrics: ParametricBreakpointRerunMetrics,
}

/// Semantic operation recorded by the parametric trace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParametricTraceEventKind {
    /// Complete one cold static solve; no forest or distance label was reused.
    ColdStaticSolve,
    /// Inspect one residual arc inside a cold static pseudoflow run.
    InspectStaticResidualArc,
    /// Solve minimal and maximal cuts at the two domain endpoints.
    InitializeEndpoints,
    /// Intersect two affine endpoint cut functions.
    IntersectCutFunctions,
    /// Run bounded explicit-tree pseudoflow at the exact intersection.
    SolveIntersection,
    /// Certify one complete-analysis interval.
    RecordSegment,
    /// Publish one nested source-set transition.
    RecordBreakpoint,
    /// Complete one independent Edmonds–Karp certificate oracle.
    CertifyStaticOracle,
    /// Finish independent whole-domain certification.
    Optimal,
}

/// One bounded reversible-by-reconstruction parametric trace event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParametricTraceEvent {
    /// One-based stable event identity.
    pub event_id: u64,
    /// Closed semantic event kind.
    pub kind: ParametricTraceEventKind,
    /// Current subproblem lower endpoint.
    pub lower: ParametricRational,
    /// Current subproblem upper endpoint.
    pub upper: ParametricRational,
    /// Exact selected intersection, when applicable.
    pub parameter: Option<ParametricRational>,
    /// `true` only for an explicitly cold static rerun event.
    pub cold_static_rerun: bool,
    /// Always `false` in this variant; exposed to prevent warm-run ambiguity.
    pub normalized_tree_reused: bool,
    /// One-based cold static-run ordinal, when this is a static solve event.
    pub static_run_ordinal: Option<u64>,
    /// Exact scale denominator used by the integral static subproblem.
    pub scale_denominator: Option<BigInt>,
    /// Exact original edge inspected by a nested static-solver Detail.
    pub inspected_edge: Option<EdgeId>,
    /// Smaller endpoint or before-breakpoint source side.
    pub lower_source_side: Vec<NodeId>,
    /// Larger endpoint or after-breakpoint source side.
    pub upper_source_side: Vec<NodeId>,
    /// Metrics after this event.
    pub metrics: ParametricBreakpointRerunMetrics,
}

/// Complete analysis plus its deterministic semantic trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParametricBreakpointRerunTraceResult {
    /// Same certified result returned by the fast profile.
    pub result: ParametricBreakpointRerunResult,
    /// Depth-first exact breakpoint traversal.
    pub events: Vec<ParametricTraceEvent>,
}

/// Parametric model, arithmetic, solver, or certificate failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ParametricBreakpointRerunError {
    /// Graph exceeds the interactive exact-analysis band.
    #[error("graph exceeds parametric breakpoint rerun admission limits")]
    AdmissionLimit,
    /// More bounded subproblems were required than the public contract allows.
    #[error("parametric breakpoint rerun work limit reached")]
    WorkLimit,
    /// Source or sink is absent or identical.
    #[error("invalid parametric max-flow terminals")]
    InvalidTerminals,
    /// Parameter endpoints are decreasing or a denominator is zero.
    #[error("invalid parametric interval")]
    InvalidInterval,
    /// Graph uses lower bounds, costs, or supplies outside this source model.
    #[error("parametric max-flow requires zero lower bounds, costs, and supplies")]
    UnsupportedGraph,
    /// A coefficient names no original edge.
    #[error("parametric coefficient edge is missing")]
    MissingEdge,
    /// One original edge has more than one coefficient declaration.
    #[error("duplicate parametric capacity coefficient")]
    DuplicateCoefficient,
    /// Zero coefficients must be omitted from the canonical declaration.
    #[error("zero parametric capacity coefficient must be omitted")]
    ZeroCoefficient,
    /// A nonzero coefficient is not on an eligible terminal-adjacent arc.
    #[error("parametric coefficient is not on an eligible terminal arc")]
    InvalidCoefficientLocation,
    /// Source or sink coefficient has the wrong monotonic direction.
    #[error("parametric terminal capacity monotonicity is invalid")]
    InvalidMonotonicity,
    /// An evaluated or denominator-scaled capacity is negative or outside `u64`.
    #[error("parametric capacity leaves the supported exact domain")]
    CapacityDomain,
    /// Static bounded pseudoflow failed at an exact rational parameter.
    #[error("parametric breakpoint rerun static subproblem failed")]
    StaticSolver,
    /// Independent Edmonds–Karp certification failed.
    #[error("parametric breakpoint rerun oracle failed")]
    Oracle,
    /// Nested-cut, intersection, coverage, or optimality invariant failed.
    #[error("parametric breakpoint rerun invariant failed")]
    Invariant,
    /// A nested solver advanced its scan counter without an attributable edge.
    #[error("parametric nested scan attribution failed at {catalog_id}")]
    TraceAttribution {
        /// Source event identity, or an end-of-trace marker for incomplete work.
        catalog_id: String,
    },
}

/// Computes all exact breakpoints without recording semantic traversal events.
///
/// # Errors
///
/// Rejects a failed static pseudoflow run, arithmetic-domain overflow, bounded
/// work exhaustion, or any independent certificate contradiction.
pub fn solve_parametric_breakpoint_rerun(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
) -> Result<ParametricBreakpointRerunResult, ParametricBreakpointRerunError> {
    solve_internal(graph, problem, false).map(|run| run.result)
}

/// Computes all exact breakpoints while reporting feasibility work from each
/// cold pseudoflow subproblem to the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_parametric_breakpoint_rerun_with_feasibility(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    feasibility: &mut FeasibilityExecution,
) -> Result<ParametricBreakpointRerunResult, ParametricBreakpointRerunError> {
    solve_internal_with_feasibility(graph, problem, false, feasibility).map(|run| run.result)
}

/// Computes all exact breakpoints and records the recursive cut traversal.
///
/// # Errors
///
/// Returns the same failures as [`solve_parametric_breakpoint_rerun`].
pub fn trace_parametric_breakpoint_rerun(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
) -> Result<ParametricBreakpointRerunTraceResult, ParametricBreakpointRerunError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_parametric_breakpoint_rerun_with_feasibility(graph, problem, &mut feasibility)
}

/// Traces the source execution while explicitly publishing auxiliary
/// feasibility work performed by each cold pseudoflow subproblem.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_parametric_breakpoint_rerun_with_feasibility(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    feasibility: &mut FeasibilityExecution,
) -> Result<ParametricBreakpointRerunTraceResult, ParametricBreakpointRerunError> {
    let run = solve_internal_with_feasibility(graph, problem, true, feasibility)?;
    let trace = ParametricBreakpointRerunTraceResult {
        result: run.result,
        events: run.events,
    };
    check_parametric_breakpoint_rerun_trace(graph, problem, &trace)?;
    Ok(trace)
}

/// Replays every cold solve and exact breakpoint event deterministically.
///
/// # Errors
///
/// Rejects any result, traversal, source-side, or metric disagreement.
pub fn check_parametric_breakpoint_rerun_trace(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    trace: &ParametricBreakpointRerunTraceResult,
) -> Result<(), ParametricBreakpointRerunError> {
    let mut certificate_metrics = ParametricBreakpointRerunMetrics::default();
    certify_parametric_result(
        graph,
        problem,
        &mut certificate_metrics,
        &trace.result.segments,
        &trace.result.breakpoints,
    )?;
    if trace.events.is_empty()
        || trace.events.last().map(|event| event.kind) != Some(ParametricTraceEventKind::Optimal)
        || trace.events.last().map(|event| event.metrics) != Some(trace.result.metrics)
        || trace.result.metrics.segments != trace.result.segments.len() as u64
        || trace.result.metrics.breakpoints != trace.result.breakpoints.len() as u64
    {
        return Err(ParametricBreakpointRerunError::Invariant);
    }
    let mut recorded_segments = Vec::new();
    let mut recorded_breakpoints = Vec::new();
    for (index, event) in trace.events.iter().enumerate() {
        if event.event_id != index as u64 + 1
            || event.lower < *problem.minimum()
            || event.upper > *problem.maximum()
            || event.lower > event.upper
            || event.normalized_tree_reused
            || (event.kind == ParametricTraceEventKind::Optimal
                && (index + 1 != trace.events.len()
                    || event.lower != *problem.minimum()
                    || event.upper != *problem.maximum()
                    || event.parameter.is_some()))
        {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        let cold = event.kind == ParametricTraceEventKind::ColdStaticSolve;
        let inspection = event.kind == ParametricTraceEventKind::InspectStaticResidualArc;
        if cold
            != (event.cold_static_rerun
                && event.static_run_ordinal.is_some()
                && event.scale_denominator.is_some())
            || (!cold
                && (event.cold_static_rerun
                    || event.static_run_ordinal.is_some()
                    || event.scale_denominator.is_some()))
            || inspection != event.inspected_edge.is_some()
        {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        match event.kind {
            ParametricTraceEventKind::RecordSegment => {
                recorded_segments.push((event.lower.clone(), event.upper.clone()));
            }
            ParametricTraceEventKind::RecordBreakpoint => {
                let Some(parameter) = event.parameter.clone() else {
                    return Err(ParametricBreakpointRerunError::Invariant);
                };
                recorded_breakpoints.push(parameter);
            }
            _ => {}
        }
    }
    if recorded_segments
        != trace
            .result
            .segments
            .iter()
            .map(|segment| (segment.lower.clone(), segment.upper.clone()))
            .collect::<Vec<_>>()
        || recorded_breakpoints
            != trace
                .result
                .breakpoints
                .iter()
                .map(|breakpoint| breakpoint.parameter.clone())
                .collect::<Vec<_>>()
    {
        return Err(ParametricBreakpointRerunError::Invariant);
    }
    Ok(())
}

struct ParametricRun {
    result: ParametricBreakpointRerunResult,
    events: Vec<ParametricTraceEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CutState {
    membership: Vec<bool>,
    intercept: BigInt,
    slope: BigInt,
}

#[derive(Clone, Debug)]
struct StaticSolution {
    minimal: CutState,
    maximal: CutState,
}

struct StaticOracleSolution {
    value: i128,
    residual_arc_scans: u128,
    minimal_membership: Vec<bool>,
    maximal_membership: Vec<bool>,
}

struct Kernel<'graph, 'execution> {
    graph: &'graph FlowNetwork,
    problem: &'graph ParametricMaxFlowProblem,
    feasibility: &'execution mut FeasibilityExecution,
    metrics: ParametricBreakpointRerunMetrics,
    segments: Vec<ParametricSegment>,
    events: Vec<ParametricTraceEvent>,
    record_trace: bool,
}

fn solve_internal(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    record_trace: bool,
) -> Result<ParametricRun, ParametricBreakpointRerunError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, problem, record_trace, &mut feasibility)
}

#[expect(
    clippy::too_many_lines,
    reason = "endpoint recursion, normalization, certification, and publication form one result transaction"
)]
fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    record_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<ParametricRun, ParametricBreakpointRerunError> {
    let mut kernel = Kernel {
        graph,
        problem,
        feasibility,
        metrics: ParametricBreakpointRerunMetrics::default(),
        segments: Vec::new(),
        events: Vec::new(),
        record_trace,
    };
    let lower_solution = kernel.solve_static(problem.minimum())?;
    let upper_solution = if problem.minimum() == problem.maximum() {
        lower_solution.clone()
    } else {
        kernel.solve_static(problem.maximum())?
    };
    ensure_subset(
        &lower_solution.minimal.membership,
        &upper_solution.maximal.membership,
    )?;
    kernel.record(
        ParametricTraceEventKind::InitializeEndpoints,
        problem.minimum(),
        problem.maximum(),
        None,
        &lower_solution.minimal,
        &upper_solution.maximal,
    )?;
    if problem.minimum() == problem.maximum() {
        kernel.push_point_segment(
            problem.minimum().clone(),
            &lower_solution.minimal,
            &lower_solution.maximal,
        )?;
    } else {
        kernel.recurse(
            problem.minimum().clone(),
            lower_solution.minimal,
            problem.maximum().clone(),
            upper_solution.maximal,
            1,
        )?;
    }
    normalize_segments(&mut kernel.segments)?;
    let breakpoints = derive_breakpoints(&kernel.segments)?;
    kernel.metrics.segments = u64::try_from(kernel.segments.len())
        .map_err(|_| ParametricBreakpointRerunError::WorkLimit)?;
    kernel.metrics.breakpoints =
        u64::try_from(breakpoints.len()).map_err(|_| ParametricBreakpointRerunError::WorkLimit)?;
    kernel.metrics.simultaneous_breakpoints = breakpoints
        .iter()
        .filter(|breakpoint| breakpoint.entering_nodes.len() > 1)
        .count()
        .try_into()
        .map_err(|_| ParametricBreakpointRerunError::WorkLimit)?;

    let mut result = ParametricBreakpointRerunResult {
        segments: std::mem::take(&mut kernel.segments),
        breakpoints,
        metrics: kernel.metrics,
    };
    let certificate_trace = kernel.record_trace.then_some(&mut kernel.events);
    certify_parametric_result_internal(
        graph,
        problem,
        &mut result.metrics,
        &result.segments,
        &result.breakpoints,
        certificate_trace,
    )?;
    kernel.metrics = result.metrics;
    for pair in result.segments.windows(2) {
        let lower_cut = cut_state_from_public(graph, &pair[0].minimal_cut)?;
        let upper_cut = cut_state_from_public(graph, &pair[1].maximal_cut)?;
        kernel.record(
            ParametricTraceEventKind::RecordBreakpoint,
            &pair[0].lower,
            &pair[1].upper,
            Some(&pair[0].upper),
            &lower_cut,
            &upper_cut,
        )?;
    }
    let first = result
        .segments
        .first()
        .ok_or(ParametricBreakpointRerunError::Invariant)?;
    let last = result
        .segments
        .last()
        .ok_or(ParametricBreakpointRerunError::Invariant)?;
    kernel.record(
        ParametricTraceEventKind::Optimal,
        problem.minimum(),
        problem.maximum(),
        None,
        &cut_state_from_public(graph, &first.minimal_cut)?,
        &cut_state_from_public(graph, &last.maximal_cut)?,
    )?;
    Ok(ParametricRun {
        result,
        events: kernel.events,
    })
}

impl Kernel<'_, '_> {
    fn push_point_segment(
        &mut self,
        parameter: ParametricRational,
        minimal_cut: &CutState,
        maximal_cut: &CutState,
    ) -> Result<(), ParametricBreakpointRerunError> {
        ensure_subset(&minimal_cut.membership, &maximal_cut.membership)?;
        self.record(
            ParametricTraceEventKind::RecordSegment,
            &parameter,
            &parameter,
            Some(&parameter),
            minimal_cut,
            maximal_cut,
        )?;
        self.segments.push(ParametricSegment {
            lower: parameter.clone(),
            upper: parameter,
            minimal_cut: public_cut(self.graph, minimal_cut)?,
            maximal_cut: public_cut(self.graph, maximal_cut)?,
        });
        Ok(())
    }

    fn recurse(
        &mut self,
        lower: ParametricRational,
        lower_cut: CutState,
        upper: ParametricRational,
        upper_cut: CutState,
        depth: u64,
    ) -> Result<(), ParametricBreakpointRerunError> {
        self.metrics.subproblems = self
            .metrics
            .subproblems
            .checked_add(1)
            .ok_or(ParametricBreakpointRerunError::WorkLimit)?;
        if self.metrics.subproblems > PARAMETRIC_BREAKPOINT_RERUN_MAX_SUBPROBLEMS {
            return Err(ParametricBreakpointRerunError::WorkLimit);
        }
        self.metrics.maximum_depth = self.metrics.maximum_depth.max(depth);
        ensure_subset(&lower_cut.membership, &upper_cut.membership)?;
        if lower_cut.membership == upper_cut.membership {
            return self.solve_and_push_interior(lower, upper);
        }
        if lower_cut.intercept == upper_cut.intercept && lower_cut.slope == upper_cut.slope {
            return self.solve_and_push_interior(lower, upper);
        }

        self.metrics.intersections = self
            .metrics
            .intersections
            .checked_add(1)
            .ok_or(ParametricBreakpointRerunError::WorkLimit)?;
        let denominator = &lower_cut.slope - &upper_cut.slope;
        if denominator.is_zero() {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        let parameter =
            ParametricRational::new(&upper_cut.intercept - &lower_cut.intercept, denominator)?;
        if rational_exceeds_admission(&parameter) {
            return Err(ParametricBreakpointRerunError::WorkLimit);
        }
        self.record(
            ParametricTraceEventKind::IntersectCutFunctions,
            &lower,
            &upper,
            Some(&parameter),
            &lower_cut,
            &upper_cut,
        )?;
        if parameter < lower || parameter > upper {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        if parameter == lower {
            return self.solve_and_push_interior(lower, upper);
        }
        if parameter == upper {
            return self.solve_and_push_interior(lower, upper);
        }

        let solved = self.solve_static(&parameter)?;
        ensure_subset(&lower_cut.membership, &solved.minimal.membership)?;
        ensure_subset(&solved.minimal.membership, &solved.maximal.membership)?;
        ensure_subset(&solved.maximal.membership, &upper_cut.membership)?;
        self.record(
            ParametricTraceEventKind::SolveIntersection,
            &lower,
            &upper,
            Some(&parameter),
            &solved.minimal,
            &solved.maximal,
        )?;
        self.recurse(
            lower,
            lower_cut,
            parameter.clone(),
            solved.minimal,
            depth
                .checked_add(1)
                .ok_or(ParametricBreakpointRerunError::WorkLimit)?,
        )?;
        self.recurse(
            parameter,
            solved.maximal,
            upper,
            upper_cut,
            depth
                .checked_add(1)
                .ok_or(ParametricBreakpointRerunError::WorkLimit)?,
        )
    }

    #[allow(clippy::too_many_lines)]
    fn solve_static(
        &mut self,
        parameter: &ParametricRational,
    ) -> Result<StaticSolution, ParametricBreakpointRerunError> {
        let scaled = materialize_scaled_graph(self.graph, self.problem, parameter)?;
        self.metrics.pseudoflow_runs = self
            .metrics
            .pseudoflow_runs
            .checked_add(1)
            .ok_or(ParametricBreakpointRerunError::WorkLimit)?;
        let scans_before = self.metrics.static_residual_arc_scans;
        let (result, mut scan_checkpoints) = if self.record_trace {
            let traced = trace_hochbaum_pseudoflow_with_feasibility(
                &scaled,
                self.problem.source(),
                self.problem.sink(),
                self.feasibility,
            )
            .map_err(|_| ParametricBreakpointRerunError::StaticSolver)?;
            let checkpoints = nested_scan_checkpoints(
                &scaled,
                &traced.base_snapshot,
                &traced.events,
                0,
                traced.result.metrics.residual_arc_scans,
            )?;
            (traced.result, checkpoints)
        } else {
            (
                solve_hochbaum_pseudoflow_with_feasibility(
                    &scaled,
                    self.problem.source(),
                    self.problem.sink(),
                    self.feasibility,
                )
                .map_err(|_| ParametricBreakpointRerunError::StaticSolver)?,
                Vec::new(),
            )
        };
        let (oracle, oracle_scan_checkpoints) = if self.record_trace {
            let traced = trace_edmonds_karp(&scaled, self.problem.source(), self.problem.sink())
                .map_err(|_| ParametricBreakpointRerunError::Oracle)?;
            let checkpoints = nested_scan_checkpoints(
                &scaled,
                &traced.base_snapshot,
                &traced.events,
                result.metrics.residual_arc_scans,
                traced.result.metrics.residual_arc_scans,
            )?;
            let oracle = static_oracle_from_result(
                &scaled,
                self.problem.source(),
                self.problem.sink(),
                &traced.result,
            )?;
            (oracle, checkpoints)
        } else {
            (
                solve_static_oracle(&scaled, self.problem.source(), self.problem.sink())?,
                Vec::new(),
            )
        };
        if result.certificate.value != oracle.value {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        let pseudoflow_source_side = membership_from_ids(&scaled, &result.certificate.source_side)?;
        let minimal_membership = oracle.minimal_membership;
        let maximal_membership = oracle.maximal_membership;
        if pseudoflow_source_side != minimal_membership {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        ensure_subset(&minimal_membership, &maximal_membership)?;
        let minimal = cut_state(self.graph, self.problem, minimal_membership)?;
        let maximal = cut_state(self.graph, self.problem, maximal_membership)?;
        for (completed, inspected_edge) in scan_checkpoints.drain(..) {
            self.metrics.static_residual_arc_scans = scans_before
                .checked_add(completed)
                .ok_or(ParametricBreakpointRerunError::WorkLimit)?;
            self.record(
                ParametricTraceEventKind::InspectStaticResidualArc,
                parameter,
                parameter,
                Some(parameter),
                &minimal,
                &maximal,
            )?;
            self.events
                .last_mut()
                .ok_or(ParametricBreakpointRerunError::Invariant)?
                .inspected_edge = Some(inspected_edge);
        }
        self.metrics.static_residual_arc_scans = scans_before
            .checked_add(result.metrics.residual_arc_scans)
            .ok_or(ParametricBreakpointRerunError::WorkLimit)?;
        self.metrics.oracle_runs = self
            .metrics
            .oracle_runs
            .checked_add(1)
            .ok_or(ParametricBreakpointRerunError::WorkLimit)?;
        for (completed, inspected_edge) in oracle_scan_checkpoints {
            self.metrics.static_residual_arc_scans = scans_before
                .checked_add(completed)
                .ok_or(ParametricBreakpointRerunError::WorkLimit)?;
            self.record(
                ParametricTraceEventKind::InspectStaticResidualArc,
                parameter,
                parameter,
                Some(parameter),
                &minimal,
                &maximal,
            )?;
            self.events
                .last_mut()
                .ok_or(ParametricBreakpointRerunError::Invariant)?
                .inspected_edge = Some(inspected_edge);
        }
        self.metrics.static_residual_arc_scans = scans_before
            .checked_add(result.metrics.residual_arc_scans)
            .and_then(|value| value.checked_add(oracle.residual_arc_scans))
            .ok_or(ParametricBreakpointRerunError::WorkLimit)?;
        let expected = BigRational::from_integer(BigInt::from(oracle.value))
            / BigRational::from_integer(parameter.denominator().clone());
        if evaluate_cut(&minimal, parameter) != expected
            || evaluate_cut(&maximal, parameter) != expected
        {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        self.record(
            ParametricTraceEventKind::ColdStaticSolve,
            parameter,
            parameter,
            Some(parameter),
            &minimal,
            &maximal,
        )?;
        Ok(StaticSolution { minimal, maximal })
    }

    fn push_segment(
        &mut self,
        lower: ParametricRational,
        upper: ParametricRational,
        minimal_cut: &CutState,
        maximal_cut: &CutState,
    ) -> Result<(), ParametricBreakpointRerunError> {
        if lower >= upper {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        ensure_subset(&minimal_cut.membership, &maximal_cut.membership)?;
        if minimal_cut.intercept != maximal_cut.intercept || minimal_cut.slope != maximal_cut.slope
        {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        self.record(
            ParametricTraceEventKind::RecordSegment,
            &lower,
            &upper,
            None,
            minimal_cut,
            maximal_cut,
        )?;
        self.segments.push(ParametricSegment {
            lower,
            upper,
            minimal_cut: public_cut(self.graph, minimal_cut)?,
            maximal_cut: public_cut(self.graph, maximal_cut)?,
        });
        Ok(())
    }

    fn solve_and_push_interior(
        &mut self,
        lower: ParametricRational,
        upper: ParametricRational,
    ) -> Result<(), ParametricBreakpointRerunError> {
        if lower >= upper {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        let midpoint = ParametricRational(
            (lower.inner() + upper.inner()) / BigRational::from_integer(BigInt::from(2)),
        );
        let solved = self.solve_static(&midpoint)?;
        self.push_segment(lower, upper, &solved.minimal, &solved.maximal)
    }

    fn record(
        &mut self,
        kind: ParametricTraceEventKind,
        lower: &ParametricRational,
        upper: &ParametricRational,
        parameter: Option<&ParametricRational>,
        lower_cut: &CutState,
        upper_cut: &CutState,
    ) -> Result<(), ParametricBreakpointRerunError> {
        record_parametric_event(
            self.graph,
            self.record_trace.then_some(&mut self.events),
            self.metrics,
            kind,
            lower,
            upper,
            parameter,
            lower_cut,
            upper_cut,
            None,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn record_parametric_event(
    graph: &FlowNetwork,
    events: Option<&mut Vec<ParametricTraceEvent>>,
    metrics: ParametricBreakpointRerunMetrics,
    kind: ParametricTraceEventKind,
    lower: &ParametricRational,
    upper: &ParametricRational,
    parameter: Option<&ParametricRational>,
    lower_cut: &CutState,
    upper_cut: &CutState,
    inspected_edge: Option<EdgeId>,
) -> Result<(), ParametricBreakpointRerunError> {
    let Some(events) = events else {
        return Ok(());
    };
    let event_id = u64::try_from(events.len())
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(ParametricBreakpointRerunError::WorkLimit)?;
    events.push(ParametricTraceEvent {
        event_id,
        kind,
        lower: lower.clone(),
        upper: upper.clone(),
        parameter: parameter.cloned(),
        cold_static_rerun: kind == ParametricTraceEventKind::ColdStaticSolve,
        normalized_tree_reused: false,
        static_run_ordinal: (kind == ParametricTraceEventKind::ColdStaticSolve)
            .then_some(metrics.pseudoflow_runs),
        scale_denominator: (kind == ParametricTraceEventKind::ColdStaticSolve)
            .then(|| parameter.map(ParametricRational::denominator).cloned())
            .flatten(),
        inspected_edge,
        lower_source_side: node_ids(graph, &lower_cut.membership)?,
        upper_source_side: node_ids(graph, &upper_cut.membership)?,
        metrics,
    });
    Ok(())
}

fn materialize_scaled_graph(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    parameter: &ParametricRational,
) -> Result<FlowNetwork, ParametricBreakpointRerunError> {
    let denominator = parameter.denominator();
    let edges = graph
        .edges()
        .iter()
        .enumerate()
        .map(|(position, edge)| {
            let numerator = BigInt::from(edge.capacity()) * denominator
                + problem
                    .slopes
                    .get(position)
                    .ok_or(ParametricBreakpointRerunError::Invariant)?
                    * parameter.numerator();
            let capacity = numerator
                .to_u64()
                .ok_or(ParametricBreakpointRerunError::CapacityDomain)?;
            let from = graph
                .node(edge.from())
                .ok_or(ParametricBreakpointRerunError::Invariant)?
                .id()
                .clone();
            let to = graph
                .node(edge.to())
                .ok_or(ParametricBreakpointRerunError::Invariant)?
                .id()
                .clone();
            Ok(UnresolvedFlowEdge {
                id: edge.id().clone(),
                from,
                to,
                lower: 0,
                capacity,
                cost: 0,
            })
        })
        .collect::<Result<Vec<_>, ParametricBreakpointRerunError>>()?;
    FlowNetwork::new(graph.nodes().to_vec(), edges)
        .map_err(|_| ParametricBreakpointRerunError::CapacityDomain)
}

fn solve_static_oracle(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<StaticOracleSolution, ParametricBreakpointRerunError> {
    let result = solve_edmonds_karp(graph, source, sink)
        .map_err(|_| ParametricBreakpointRerunError::Oracle)?;
    static_oracle_from_result(graph, source, sink, &result)
}

fn solve_static_certificate_oracle(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: bool,
) -> Result<(StaticOracleSolution, Vec<(u128, EdgeId)>), ParametricBreakpointRerunError> {
    if !trace {
        return Ok((solve_static_oracle(graph, source, sink)?, Vec::new()));
    }
    let run = trace_edmonds_karp(graph, source, sink)
        .map_err(|_| ParametricBreakpointRerunError::Oracle)?;
    let checkpoints = nested_scan_checkpoints(
        graph,
        &run.base_snapshot,
        &run.events,
        0,
        run.result.metrics.residual_arc_scans,
    )?;
    let oracle = static_oracle_from_result(graph, source, sink, &run.result)?;
    Ok((oracle, checkpoints))
}

fn static_oracle_from_result(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    result: &EdmondsKarpResult,
) -> Result<StaticOracleSolution, ParametricBreakpointRerunError> {
    let minimal_membership = minimal_source_membership(graph, &result.flows, source, sink)?;
    let maximal_membership = maximal_source_membership(graph, &result.flows, source, sink)?;
    ensure_subset(&minimal_membership, &maximal_membership)?;
    Ok(StaticOracleSolution {
        value: result.certificate.value,
        residual_arc_scans: result.metrics.residual_arc_scans,
        minimal_membership,
        maximal_membership,
    })
}

fn nested_scan_checkpoints(
    graph: &FlowNetwork,
    base: &FlowTraceSnapshot,
    events: &[FlowTraceEvent],
    completed_offset: u128,
    local_total: u128,
) -> Result<Vec<(u128, EdgeId)>, ParametricBreakpointRerunError> {
    let mut replay = base.clone();
    let mut last_observed = 0_u128;
    let mut last_checkpoint = 0_u128;
    let mut checkpoints = Vec::new();
    for event in events {
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)
            .map_err(|_| ParametricBreakpointRerunError::Invariant)?;
        let completed = replay.metrics.residual_arc_scans;
        if completed <= last_observed {
            continue;
        }
        let edge = event
            .entity_refs
            .iter()
            .find_map(|reference| match reference {
                FlowTraceEntityRef::Edge(edge) => Some(edge.clone()),
                FlowTraceEntityRef::ResidualArc(arc) => Some(arc.original_edge().clone()),
                FlowTraceEntityRef::Node(_) => None,
            })
            .ok_or_else(|| ParametricBreakpointRerunError::TraceAttribution {
                catalog_id: event.catalog_id.clone(),
            })?;
        last_observed = completed;
        if completed <= PARAMETRIC_BREAKPOINT_RERUN_TRACE_SCAN_PREFIX
            || completed.saturating_sub(last_checkpoint)
                >= PARAMETRIC_BREAKPOINT_RERUN_TRACE_SCAN_BLOCK_MAX
            || completed == local_total
        {
            checkpoints.push((
                completed_offset
                    .checked_add(completed)
                    .ok_or(ParametricBreakpointRerunError::WorkLimit)?,
                edge,
            ));
            last_checkpoint = completed;
        }
    }
    if last_observed != local_total || last_checkpoint != local_total {
        return Err(ParametricBreakpointRerunError::TraceAttribution {
            catalog_id: format!("end-of-trace:{last_observed}/{last_checkpoint}/{local_total}"),
        });
    }
    Ok(checkpoints)
}

fn cut_state(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    membership: Vec<bool>,
) -> Result<CutState, ParametricBreakpointRerunError> {
    if membership.len() != graph.nodes().len()
        || !membership
            .get(problem.source().as_usize())
            .copied()
            .unwrap_or(false)
        || membership
            .get(problem.sink().as_usize())
            .copied()
            .unwrap_or(true)
    {
        return Err(ParametricBreakpointRerunError::Invariant);
    }
    let mut intercept = BigInt::zero();
    let mut slope = BigInt::zero();
    for (position, edge) in graph.edges().iter().enumerate() {
        if membership[edge.from().as_usize()] && !membership[edge.to().as_usize()] {
            intercept += BigInt::from(edge.capacity());
            slope += problem
                .slopes
                .get(position)
                .ok_or(ParametricBreakpointRerunError::Invariant)?;
        }
    }
    Ok(CutState {
        membership,
        intercept,
        slope,
    })
}

fn public_cut(
    graph: &FlowNetwork,
    cut: &CutState,
) -> Result<ParametricCut, ParametricBreakpointRerunError> {
    Ok(ParametricCut {
        source_side: node_ids(graph, &cut.membership)?,
        intercept: cut.intercept.clone(),
        slope: cut.slope.clone(),
    })
}

fn cut_state_from_public(
    graph: &FlowNetwork,
    cut: &ParametricCut,
) -> Result<CutState, ParametricBreakpointRerunError> {
    Ok(CutState {
        membership: membership_from_ids(graph, &cut.source_side)?,
        intercept: cut.intercept.clone(),
        slope: cut.slope.clone(),
    })
}

fn evaluate_affine(
    intercept: &BigInt,
    slope: &BigInt,
    parameter: &ParametricRational,
) -> BigRational {
    BigRational::from_integer(intercept.clone())
        + BigRational::from_integer(slope.clone()) * parameter.inner()
}

fn bigint_decimal_digits(value: &BigInt) -> usize {
    value.to_string().trim_start_matches('-').len()
}

fn rational_exceeds_admission(value: &ParametricRational) -> bool {
    bigint_decimal_digits(value.numerator()) > PARAMETRIC_BREAKPOINT_RERUN_MAX_DECIMAL_DIGITS
        || bigint_decimal_digits(value.denominator())
            > PARAMETRIC_BREAKPOINT_RERUN_MAX_DECIMAL_DIGITS
}

fn evaluate_cut(cut: &CutState, parameter: &ParametricRational) -> BigRational {
    evaluate_affine(&cut.intercept, &cut.slope, parameter)
}

fn membership_from_ids(
    graph: &FlowNetwork,
    ids: &[NodeId],
) -> Result<Vec<bool>, ParametricBreakpointRerunError> {
    let mut membership = vec![false; graph.nodes().len()];
    for id in ids {
        let index = graph
            .node_index(id)
            .ok_or(ParametricBreakpointRerunError::Invariant)?;
        let slot = membership
            .get_mut(index.as_usize())
            .ok_or(ParametricBreakpointRerunError::Invariant)?;
        if *slot {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        *slot = true;
    }
    Ok(membership)
}

fn node_ids(
    graph: &FlowNetwork,
    membership: &[bool],
) -> Result<Vec<NodeId>, ParametricBreakpointRerunError> {
    if membership.len() != graph.nodes().len() {
        return Err(ParametricBreakpointRerunError::Invariant);
    }
    graph
        .nodes()
        .iter()
        .zip(membership)
        .filter(|(_, present)| **present)
        .map(|(node, _)| Ok(node.id().clone()))
        .collect()
}

fn ensure_subset(left: &[bool], right: &[bool]) -> Result<(), ParametricBreakpointRerunError> {
    if left.len() != right.len() || left.iter().zip(right).any(|(left, right)| *left && !*right) {
        return Err(ParametricBreakpointRerunError::Invariant);
    }
    Ok(())
}

fn minimal_source_membership(
    graph: &FlowNetwork,
    flows: &[u64],
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<Vec<bool>, ParametricBreakpointRerunError> {
    let state = ResidualState::from_flows(graph, flows)
        .map_err(|_| ParametricBreakpointRerunError::Invariant)?;
    let mut membership = vec![false; graph.nodes().len()];
    membership[source.as_usize()] = true;
    let mut queue = VecDeque::from([source]);
    while let Some(node) = queue.pop_front() {
        for arc in state.outgoing_arcs(node) {
            if !membership[arc.to.as_usize()] {
                membership[arc.to.as_usize()] = true;
                queue.push_back(arc.to);
            }
        }
    }
    if membership[sink.as_usize()] {
        return Err(ParametricBreakpointRerunError::Invariant);
    }
    Ok(membership)
}

fn maximal_source_membership(
    graph: &FlowNetwork,
    flows: &[u64],
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<Vec<bool>, ParametricBreakpointRerunError> {
    let state = ResidualState::from_flows(graph, flows)
        .map_err(|_| ParametricBreakpointRerunError::Invariant)?;
    let mut predecessors = vec![Vec::new(); graph.nodes().len()];
    for node in graph.node_indices() {
        for arc in state.outgoing_arcs(node) {
            predecessors
                .get_mut(arc.to.as_usize())
                .ok_or(ParametricBreakpointRerunError::Invariant)?
                .push(arc.from);
        }
    }
    for incoming in &mut predecessors {
        incoming.sort_unstable();
        incoming.dedup();
    }
    let mut reaches_sink = vec![false; graph.nodes().len()];
    reaches_sink[sink.as_usize()] = true;
    let mut queue = VecDeque::from([sink]);
    while let Some(node) = queue.pop_front() {
        for &predecessor in &predecessors[node.as_usize()] {
            if !reaches_sink[predecessor.as_usize()] {
                reaches_sink[predecessor.as_usize()] = true;
                queue.push_back(predecessor);
            }
        }
    }
    let membership = reaches_sink
        .into_iter()
        .map(|reaches| !reaches)
        .collect::<Vec<_>>();
    if !membership[source.as_usize()] || membership[sink.as_usize()] {
        return Err(ParametricBreakpointRerunError::Invariant);
    }
    Ok(membership)
}

fn normalize_segments(
    segments: &mut Vec<ParametricSegment>,
) -> Result<(), ParametricBreakpointRerunError> {
    segments.sort_by(|left, right| left.lower.cmp(&right.lower));
    let mut normalized: Vec<ParametricSegment> = Vec::with_capacity(segments.len());
    for segment in segments.drain(..) {
        if segment.lower > segment.upper
            || (segment.lower == segment.upper && !normalized.is_empty())
        {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        if let Some(previous) = normalized.last_mut() {
            if previous.upper != segment.lower {
                return Err(ParametricBreakpointRerunError::Invariant);
            }
            if same_cut_expression(&previous.minimal_cut, &segment.minimal_cut) {
                ensure_public_subset(
                    &previous.minimal_cut.source_side,
                    &segment.minimal_cut.source_side,
                )?;
                ensure_public_subset(
                    &previous.maximal_cut.source_side,
                    &segment.maximal_cut.source_side,
                )?;
                previous.upper = segment.upper;
                previous.maximal_cut = segment.maximal_cut;
                continue;
            }
        }
        normalized.push(segment);
    }
    *segments = normalized;
    Ok(())
}

fn derive_breakpoints(
    segments: &[ParametricSegment],
) -> Result<Vec<ParametricBreakpoint>, ParametricBreakpointRerunError> {
    segments
        .windows(2)
        .map(|pair| {
            let before = &pair[0];
            let after = &pair[1];
            if before.upper != after.lower {
                return Err(ParametricBreakpointRerunError::Invariant);
            }
            let before_set = source_side_set(&before.minimal_cut.source_side);
            let after_set = source_side_set(&after.minimal_cut.source_side);
            if before_set.iter().any(|node| !after_set.contains(node)) {
                return Err(ParametricBreakpointRerunError::Invariant);
            }
            let exact_minimal_source_side = before.minimal_cut.source_side.clone();
            let exact_maximal_source_side = after.maximal_cut.source_side.clone();
            let exact_minimal_set = source_side_set(&exact_minimal_source_side);
            ensure_public_subset(&exact_minimal_source_side, &exact_maximal_source_side)?;
            let entering_nodes = exact_maximal_source_side
                .iter()
                .filter(|node| !exact_minimal_set.contains(*node))
                .cloned()
                .collect::<Vec<_>>();
            if entering_nodes.is_empty() {
                return Err(ParametricBreakpointRerunError::Invariant);
            }
            Ok(ParametricBreakpoint {
                parameter: before.upper.clone(),
                before_source_side: before.minimal_cut.source_side.clone(),
                after_source_side: after.minimal_cut.source_side.clone(),
                exact_minimal_source_side,
                exact_maximal_source_side,
                entering_nodes,
            })
        })
        .collect()
}

fn source_side_set(source_side: &[NodeId]) -> BTreeSet<&NodeId> {
    source_side.iter().collect()
}

fn same_cut_expression(left: &ParametricCut, right: &ParametricCut) -> bool {
    left.intercept == right.intercept && left.slope == right.slope
}

fn ensure_public_subset(
    left: &[NodeId],
    right: &[NodeId],
) -> Result<(), ParametricBreakpointRerunError> {
    let right = source_side_set(right);
    if left.iter().any(|node| !right.contains(node)) {
        return Err(ParametricBreakpointRerunError::Invariant);
    }
    Ok(())
}

pub(crate) fn certify_parametric_result(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    metrics: &mut ParametricBreakpointRerunMetrics,
    segments: &[ParametricSegment],
    breakpoints: &[ParametricBreakpoint],
) -> Result<(), ParametricBreakpointRerunError> {
    certify_parametric_result_internal(graph, problem, metrics, segments, breakpoints, None)
}

#[expect(
    clippy::too_many_lines,
    reason = "segment, endpoint, interior-probe, and breakpoint witnesses are checked in one ordered certificate pass"
)]
fn certify_parametric_result_internal(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    metrics: &mut ParametricBreakpointRerunMetrics,
    segments: &[ParametricSegment],
    breakpoints: &[ParametricBreakpoint],
    mut trace_events: Option<&mut Vec<ParametricTraceEvent>>,
) -> Result<(), ParametricBreakpointRerunError> {
    let first = segments
        .first()
        .ok_or(ParametricBreakpointRerunError::Invariant)?;
    let last = segments
        .last()
        .ok_or(ParametricBreakpointRerunError::Invariant)?;
    if first.lower != *problem.minimum() || last.upper != *problem.maximum() {
        return Err(ParametricBreakpointRerunError::Invariant);
    }
    if breakpoints.len() != segments.len().saturating_sub(1) {
        return Err(ParametricBreakpointRerunError::Invariant);
    }
    let mut previous_minimal: Option<Vec<bool>> = None;
    let mut previous_maximal: Option<Vec<bool>> = None;
    for (position, segment) in segments.iter().enumerate() {
        if position > 0 && segments[position - 1].upper != segment.lower {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        let minimal = cut_state_from_public(graph, &segment.minimal_cut)?;
        let maximal = cut_state_from_public(graph, &segment.maximal_cut)?;
        ensure_subset(&minimal.membership, &maximal.membership)?;
        let reconstructed_minimal = cut_state(graph, problem, minimal.membership.clone())?;
        let reconstructed_maximal = cut_state(graph, problem, maximal.membership.clone())?;
        if reconstructed_minimal.intercept != minimal.intercept
            || reconstructed_minimal.slope != minimal.slope
            || reconstructed_maximal.intercept != maximal.intercept
            || reconstructed_maximal.slope != maximal.slope
            || (segment.lower < segment.upper
                && (minimal.intercept != maximal.intercept || minimal.slope != maximal.slope))
        {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        if let Some(previous) = previous_minimal.as_deref() {
            ensure_subset(previous, &minimal.membership)?;
        }
        if let Some(previous) = previous_maximal.as_deref() {
            ensure_subset(previous, &maximal.membership)?;
        }
        previous_minimal = Some(minimal.membership.clone());
        previous_maximal = Some(maximal.membership.clone());
        for endpoint in [&segment.lower, &segment.upper] {
            let scaled = materialize_scaled_graph(graph, problem, endpoint)?;
            let (oracle, checkpoints) = solve_static_certificate_oracle(
                &scaled,
                problem.source(),
                problem.sink(),
                trace_events.is_some(),
            )?;
            publish_certificate_oracle_trace(
                graph,
                metrics,
                trace_events.as_deref_mut(),
                &segment.lower,
                &segment.upper,
                endpoint,
                &minimal,
                &maximal,
                oracle.residual_arc_scans,
                checkpoints,
            )?;
            let declared_scaled = evaluate_cut(&minimal, endpoint)
                * BigRational::from_integer(endpoint.denominator().clone());
            if !declared_scaled.is_integer()
                || declared_scaled.to_integer() != BigInt::from(oracle.value)
            {
                return Err(ParametricBreakpointRerunError::Invariant);
            }
        }

        let probe = if segment.lower == segment.upper {
            segment.lower.clone()
        } else {
            ParametricRational(
                (segment.lower.inner() + segment.upper.inner())
                    / BigRational::from_integer(BigInt::from(2)),
            )
        };
        let scaled = materialize_scaled_graph(graph, problem, &probe)?;
        let (oracle, checkpoints) = solve_static_certificate_oracle(
            &scaled,
            problem.source(),
            problem.sink(),
            trace_events.is_some(),
        )?;
        publish_certificate_oracle_trace(
            graph,
            metrics,
            trace_events.as_deref_mut(),
            &segment.lower,
            &segment.upper,
            &probe,
            &minimal,
            &maximal,
            oracle.residual_arc_scans,
            checkpoints,
        )?;
        if oracle.minimal_membership != minimal.membership
            || oracle.maximal_membership != maximal.membership
        {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
    }
    for (pair, breakpoint) in segments.windows(2).zip(breakpoints) {
        let parameter = &pair[0].upper;
        if breakpoint.parameter != *parameter
            || breakpoint.before_source_side != pair[0].minimal_cut.source_side
            || breakpoint.after_source_side != pair[1].minimal_cut.source_side
            || breakpoint.exact_minimal_source_side != pair[0].minimal_cut.source_side
            || breakpoint.exact_maximal_source_side != pair[1].maximal_cut.source_side
        {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        let before = cut_state_from_public(graph, &pair[0].minimal_cut)?;
        let after = cut_state_from_public(graph, &pair[1].minimal_cut)?;
        if same_cut_expression(&pair[0].minimal_cut, &pair[1].minimal_cut)
            || evaluate_cut(&before, parameter) != evaluate_cut(&after, parameter)
        {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        let scaled = materialize_scaled_graph(graph, problem, parameter)?;
        let (oracle, checkpoints) = solve_static_certificate_oracle(
            &scaled,
            problem.source(),
            problem.sink(),
            trace_events.is_some(),
        )?;
        publish_certificate_oracle_trace(
            graph,
            metrics,
            trace_events.as_deref_mut(),
            parameter,
            parameter,
            parameter,
            &before,
            &after,
            oracle.residual_arc_scans,
            checkpoints,
        )?;
        if node_ids(graph, &oracle.minimal_membership)? != breakpoint.exact_minimal_source_side
            || node_ids(graph, &oracle.maximal_membership)? != breakpoint.exact_maximal_source_side
        {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
    }
    Ok(())
}

fn increment_oracle_runs(
    metrics: &mut ParametricBreakpointRerunMetrics,
) -> Result<(), ParametricBreakpointRerunError> {
    metrics.oracle_runs = metrics
        .oracle_runs
        .checked_add(1)
        .ok_or(ParametricBreakpointRerunError::WorkLimit)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn publish_certificate_oracle_trace(
    graph: &FlowNetwork,
    metrics: &mut ParametricBreakpointRerunMetrics,
    trace_events: Option<&mut Vec<ParametricTraceEvent>>,
    lower: &ParametricRational,
    upper: &ParametricRational,
    parameter: &ParametricRational,
    lower_cut: &CutState,
    upper_cut: &CutState,
    scan_count: u128,
    checkpoints: Vec<(u128, EdgeId)>,
) -> Result<(), ParametricBreakpointRerunError> {
    increment_oracle_runs(metrics)?;
    let scans_before = metrics.static_residual_arc_scans;
    let mut trace_events = trace_events;
    for (completed, edge) in checkpoints {
        if completed == 0 || completed > scan_count {
            return Err(ParametricBreakpointRerunError::Invariant);
        }
        metrics.static_residual_arc_scans = scans_before
            .checked_add(completed)
            .ok_or(ParametricBreakpointRerunError::WorkLimit)?;
        record_parametric_event(
            graph,
            trace_events.as_deref_mut(),
            *metrics,
            ParametricTraceEventKind::InspectStaticResidualArc,
            lower,
            upper,
            Some(parameter),
            lower_cut,
            upper_cut,
            Some(edge),
        )?;
    }
    metrics.static_residual_arc_scans = scans_before
        .checked_add(scan_count)
        .ok_or(ParametricBreakpointRerunError::WorkLimit)?;
    record_parametric_event(
        graph,
        trace_events,
        *metrics,
        ParametricTraceEventKind::CertifyStaticOracle,
        lower,
        upper,
        Some(parameter),
        lower_cut,
        upper_cut,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::trace_hochbaum_pseudoflow;
    use crate::model::{FlowNode, UnresolvedFlowEdge};
    use num_traits::One;

    fn node(id: &str) -> FlowNode {
        FlowNode::new(NodeId::parse(id).expect("node id"), 0)
    }

    fn edge(id: &str, from: &str, to: &str, capacity: u64) -> UnresolvedFlowEdge {
        UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge id"),
            from: NodeId::parse(from).expect("from"),
            to: NodeId::parse(to).expect("to"),
            lower: 0,
            capacity,
            cost: 0,
        }
    }

    fn fixture() -> (FlowNetwork, ParametricMaxFlowProblem) {
        // min cut changes {s} -> {s,a} at lambda = 2.
        let graph = FlowNetwork::new(
            vec![node("a"), node("s"), node("t")],
            vec![edge("sa", "s", "a", 1), edge("at", "a", "t", 5)],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        let problem = ParametricMaxFlowProblem::new(
            &graph,
            source,
            sink,
            ParametricRational::from_integer(BigInt::zero()),
            ParametricRational::from_integer(BigInt::from(4)),
            vec![ParametricCapacitySlope {
                edge: EdgeId::parse("sa").expect("sa"),
                slope: BigInt::from(2),
            }],
        )
        .expect("problem");
        (graph, problem)
    }

    #[test]
    fn exact_breakpoint_and_nested_segments_are_certified() {
        let (graph, problem) = fixture();
        let result = solve_parametric_breakpoint_rerun(&graph, &problem).expect("solve");
        assert_eq!(result.segments.len(), 2);
        assert_eq!(result.breakpoints.len(), 1);
        assert_eq!(result.breakpoints[0].parameter.canonical(), "2/1");
        assert_eq!(
            result.breakpoints[0]
                .entering_nodes
                .iter()
                .map(NodeId::as_str)
                .collect::<Vec<_>>(),
            ["a"]
        );
        assert_eq!(result.segments[0].minimal_cut.intercept, BigInt::from(1));
        assert_eq!(result.segments[0].minimal_cut.slope, BigInt::from(2));
        assert_eq!(result.segments[1].minimal_cut.intercept, BigInt::from(5));
        assert_eq!(result.segments[1].minimal_cut.slope, BigInt::zero());
    }

    #[test]
    fn trace_and_fast_results_match_exactly() {
        let (graph, problem) = fixture();
        let fast = solve_parametric_breakpoint_rerun(&graph, &problem).expect("fast");
        let traced = trace_parametric_breakpoint_rerun(&graph, &problem).expect("trace");
        assert_eq!(traced.result, fast);
        assert_eq!(
            traced.events.first().map(|event| event.kind),
            Some(ParametricTraceEventKind::InspectStaticResidualArc)
        );
        let first = traced
            .events
            .iter()
            .find(|event| event.kind == ParametricTraceEventKind::ColdStaticSolve)
            .expect("first cold solve");
        assert!(first.cold_static_rerun);
        assert!(!first.normalized_tree_reused);
        assert_eq!(first.static_run_ordinal, Some(1));
        assert_eq!(first.scale_denominator, Some(BigInt::one()));
        assert_eq!(
            traced.events.last().map(|event| event.kind),
            Some(ParametricTraceEventKind::Optimal)
        );
        assert!(
            traced
                .events
                .windows(2)
                .all(|pair| pair[0].event_id + 1 == pair[1].event_id)
        );
    }

    #[test]
    fn certification_oracle_attributes_every_scan_on_a_padded_graph() {
        let graph = FlowNetwork::new(
            vec![node("a"), node("s"), node("t"), node("z")],
            vec![
                edge("sa", "s", "a", 1),
                edge("at", "a", "t", 5),
                edge("az", "a", "z", 1),
            ],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        let problem = ParametricMaxFlowProblem::new(
            &graph,
            source,
            sink,
            ParametricRational::from_integer(BigInt::zero()),
            ParametricRational::from_integer(BigInt::from(4)),
            vec![ParametricCapacitySlope {
                edge: EdgeId::parse("sa").expect("sa"),
                slope: BigInt::from(2),
            }],
        )
        .expect("problem");
        let scaled =
            materialize_scaled_graph(&graph, &problem, problem.minimum()).expect("scaled graph");
        let pseudoflow =
            trace_hochbaum_pseudoflow(&scaled, source, sink).expect("nested pseudoflow trace");
        nested_scan_checkpoints(
            &scaled,
            &pseudoflow.base_snapshot,
            &pseudoflow.events,
            0,
            pseudoflow.result.metrics.residual_arc_scans,
        )
        .expect("attributed pseudoflow scans");
        let oracle = trace_edmonds_karp(&scaled, source, sink).expect("nested oracle trace");
        nested_scan_checkpoints(
            &scaled,
            &oracle.base_snapshot,
            &oracle.events,
            0,
            oracle.result.metrics.residual_arc_scans,
        )
        .expect("attributed oracle scans");
        let trace = trace_parametric_breakpoint_rerun(&graph, &problem).expect("trace");
        let mut previous_scans = 0_u128;
        for event in &trace.events {
            let delta = event
                .metrics
                .static_residual_arc_scans
                .checked_sub(previous_scans)
                .expect("monotone scan counter");
            if delta > 0 {
                assert_eq!(
                    event.kind,
                    ParametricTraceEventKind::InspectStaticResidualArc
                );
                assert!(delta <= PARAMETRIC_BREAKPOINT_RERUN_TRACE_SCAN_BLOCK_MAX);
                assert!(event.inspected_edge.is_some());
            }
            previous_scans = event.metrics.static_residual_arc_scans;
        }
        assert_eq!(
            previous_scans,
            trace.result.metrics.static_residual_arc_scans
        );
        let published_oracles = trace
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event.kind,
                    ParametricTraceEventKind::ColdStaticSolve
                        | ParametricTraceEventKind::CertifyStaticOracle
                )
            })
            .count();
        assert_eq!(
            u64::try_from(published_oracles).expect("published oracle count fits u64"),
            trace.result.metrics.oracle_runs
        );
    }

    #[test]
    fn nonintegral_breakpoint_is_normalized() {
        let graph = FlowNetwork::new(
            vec![node("a"), node("s"), node("t")],
            vec![edge("sa", "s", "a", 2), edge("at", "a", "t", 5)],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        let problem = ParametricMaxFlowProblem::new(
            &graph,
            source,
            sink,
            ParametricRational::from_integer(BigInt::zero()),
            ParametricRational::from_integer(BigInt::from(4)),
            vec![ParametricCapacitySlope {
                edge: EdgeId::parse("sa").expect("sa"),
                slope: BigInt::from(2),
            }],
        )
        .expect("problem");
        let result = solve_parametric_breakpoint_rerun(&graph, &problem).expect("solve");
        assert_eq!(result.breakpoints[0].parameter.canonical(), "3/2");
    }

    #[test]
    fn simultaneous_breakpoint_records_every_entering_node() {
        let graph = FlowNetwork::new(
            vec![node("a"), node("b"), node("s"), node("t")],
            vec![
                edge("sa", "s", "a", 1),
                edge("sb", "s", "b", 1),
                edge("at", "a", "t", 3),
                edge("bt", "b", "t", 3),
            ],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        let problem = ParametricMaxFlowProblem::new(
            &graph,
            source,
            sink,
            ParametricRational::from_integer(BigInt::zero()),
            ParametricRational::from_integer(BigInt::from(4)),
            vec![
                ParametricCapacitySlope {
                    edge: EdgeId::parse("sa").expect("sa"),
                    slope: BigInt::one(),
                },
                ParametricCapacitySlope {
                    edge: EdgeId::parse("sb").expect("sb"),
                    slope: BigInt::one(),
                },
            ],
        )
        .expect("problem");
        let result = solve_parametric_breakpoint_rerun(&graph, &problem).expect("solve");
        assert_eq!(result.breakpoints[0].parameter.canonical(), "2/1");
        assert_eq!(result.breakpoints[0].entering_nodes.len(), 2);
        assert_eq!(result.metrics.simultaneous_breakpoints, 1);
        assert_eq!(
            result.breakpoints[0]
                .exact_minimal_source_side
                .iter()
                .map(NodeId::as_str)
                .collect::<Vec<_>>(),
            ["s"]
        );
        assert_eq!(
            result.breakpoints[0]
                .exact_maximal_source_side
                .iter()
                .map(NodeId::as_str)
                .collect::<Vec<_>>(),
            ["a", "b", "s"]
        );
    }

    #[test]
    fn degenerate_interval_preserves_both_cut_extrema() {
        let graph = FlowNetwork::new(
            vec![node("a"), node("s"), node("t")],
            vec![edge("sa", "s", "a", 5), edge("at", "a", "t", 5)],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        let problem = ParametricMaxFlowProblem::new(
            &graph,
            source,
            sink,
            ParametricRational::from_integer(BigInt::zero()),
            ParametricRational::from_integer(BigInt::from(3)),
            Vec::new(),
        )
        .expect("problem");
        let result = solve_parametric_breakpoint_rerun(&graph, &problem).expect("solve");
        assert_eq!(result.segments.len(), 1);
        assert!(result.breakpoints.is_empty());
        assert_eq!(
            result.segments[0]
                .minimal_cut
                .source_side
                .iter()
                .map(NodeId::as_str)
                .collect::<Vec<_>>(),
            ["s"]
        );
        assert_eq!(
            result.segments[0]
                .maximal_cut
                .source_side
                .iter()
                .map(NodeId::as_str)
                .collect::<Vec<_>>(),
            ["a", "s"]
        );
    }

    #[test]
    fn one_point_domain_and_rational_endpoints_remain_exact() {
        let graph = FlowNetwork::new(
            vec![node("a"), node("s"), node("t")],
            vec![
                edge("sa", "s", "a", 1),
                edge("aa", "a", "a", 17),
                edge("at", "a", "t", 5),
            ],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        let slope = vec![ParametricCapacitySlope {
            edge: EdgeId::parse("sa").expect("sa"),
            slope: BigInt::from(2),
        }];
        let rational_problem = ParametricMaxFlowProblem::new(
            &graph,
            source,
            sink,
            ParametricRational::new(BigInt::one(), BigInt::from(2)).expect("one half"),
            ParametricRational::new(BigInt::from(5), BigInt::from(2)).expect("five halves"),
            slope.clone(),
        )
        .expect("rational endpoint problem");
        let rational =
            solve_parametric_breakpoint_rerun(&graph, &rational_problem).expect("rational solve");
        assert_eq!(rational.breakpoints[0].parameter.canonical(), "2/1");

        let point =
            ParametricRational::new(BigInt::from(3), BigInt::from(2)).expect("three halves");
        let point_problem = ParametricMaxFlowProblem::new(
            &graph,
            source,
            sink,
            point.clone(),
            point.clone(),
            slope,
        )
        .expect("point problem");
        let result =
            solve_parametric_breakpoint_rerun(&graph, &point_problem).expect("point solve");
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].lower, point);
        assert_eq!(result.segments[0].upper, result.segments[0].lower);
        assert!(result.breakpoints.is_empty());
    }

    #[test]
    fn denominator_scaling_overflow_is_rejected_at_admission() {
        let graph = FlowNetwork::new(
            vec![node("s"), node("t")],
            vec![edge("st", "s", "t", u64::MAX)],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        assert_eq!(
            ParametricMaxFlowProblem::new(
                &graph,
                source,
                sink,
                ParametricRational::new(BigInt::one(), BigInt::from(2)).expect("one half"),
                ParametricRational::new(BigInt::from(3), BigInt::from(2)).expect("three halves"),
                Vec::new(),
            ),
            Err(ParametricBreakpointRerunError::CapacityDomain)
        );
    }

    fn enumerate_cuts(graph: &FlowNetwork, problem: &ParametricMaxFlowProblem) -> Vec<CutState> {
        let internal = graph
            .node_indices()
            .filter(|node| *node != problem.source() && *node != problem.sink())
            .collect::<Vec<_>>();
        assert!(internal.len() < usize::BITS as usize);
        (0..(1_usize << internal.len()))
            .map(|mask| {
                let mut membership = vec![false; graph.nodes().len()];
                membership[problem.source().as_usize()] = true;
                for (position, node) in internal.iter().enumerate() {
                    membership[node.as_usize()] = mask & (1 << position) != 0;
                }
                cut_state(graph, problem, membership).expect("enumerated cut")
            })
            .collect()
    }

    fn enumerated_extrema(
        cuts: &[CutState],
        parameter: &ParametricRational,
    ) -> (BigRational, Vec<bool>, Vec<bool>) {
        let mut best: Option<BigRational> = None;
        let mut minimal = Vec::new();
        let mut maximal = Vec::new();
        for cut in cuts {
            let value = evaluate_cut(cut, parameter);
            match best.as_ref() {
                None => {
                    best = Some(value);
                    minimal = cut.membership.clone();
                    maximal = cut.membership.clone();
                }
                Some(current) if value < *current => {
                    best = Some(value);
                    minimal = cut.membership.clone();
                    maximal = cut.membership.clone();
                }
                Some(current) if value == *current => {
                    for (slot, present) in minimal.iter_mut().zip(&cut.membership) {
                        *slot &= *present;
                    }
                    for (slot, present) in maximal.iter_mut().zip(&cut.membership) {
                        *slot |= *present;
                    }
                }
                Some(_) => {}
            }
        }
        (best.expect("at least one cut"), minimal, maximal)
    }

    fn exhaustive_check(
        graph: &FlowNetwork,
        problem: &ParametricMaxFlowProblem,
        result: &ParametricBreakpointRerunResult,
    ) {
        let cuts = enumerate_cuts(graph, problem);
        let mut probes = vec![problem.minimum().clone(), problem.maximum().clone()];
        for (position, left) in cuts.iter().enumerate() {
            for right in cuts.iter().skip(position + 1) {
                let denominator = &left.slope - &right.slope;
                if denominator.is_zero() {
                    continue;
                }
                let parameter =
                    ParametricRational::new(&right.intercept - &left.intercept, denominator)
                        .expect("intersection");
                if parameter >= *problem.minimum() && parameter <= *problem.maximum() {
                    probes.push(parameter);
                }
            }
        }
        probes.sort();
        probes.dedup();
        let midpoints = probes
            .windows(2)
            .map(|pair| {
                ParametricRational(
                    (pair[0].inner() + pair[1].inner())
                        / BigRational::from_integer(BigInt::from(2)),
                )
            })
            .collect::<Vec<_>>();
        probes.extend(midpoints);
        probes.sort();
        probes.dedup();

        for parameter in probes {
            let (expected_value, expected_minimal, expected_maximal) =
                enumerated_extrema(&cuts, &parameter);
            let value_segment = result
                .segments
                .iter()
                .find(|segment| segment.lower <= parameter && parameter <= segment.upper)
                .expect("value segment covers every probe");
            let declared =
                cut_state_from_public(graph, &value_segment.minimal_cut).expect("declared cut");
            assert_eq!(
                evaluate_cut(&declared, &parameter),
                expected_value,
                "value mismatch at {}",
                parameter.canonical()
            );

            if let Some(breakpoint) = result
                .breakpoints
                .iter()
                .find(|breakpoint| breakpoint.parameter == parameter)
            {
                assert_eq!(
                    breakpoint.exact_minimal_source_side,
                    node_ids(graph, &expected_minimal).expect("minimal IDs")
                );
                assert_eq!(
                    breakpoint.exact_maximal_source_side,
                    node_ids(graph, &expected_maximal).expect("maximal IDs")
                );
            } else if parameter > *problem.minimum() && parameter < *problem.maximum() {
                let interior = result
                    .segments
                    .iter()
                    .find(|segment| segment.lower < parameter && parameter < segment.upper)
                    .expect("non-breakpoint probe is in an open segment");
                assert_eq!(
                    interior.minimal_cut.source_side,
                    node_ids(graph, &expected_minimal).expect("minimal IDs")
                );
                assert_eq!(
                    interior.maximal_cut.source_side,
                    node_ids(graph, &expected_maximal).expect("maximal IDs")
                );
            }
        }
    }

    #[test]
    fn bounded_random_instances_match_complete_cut_enumeration() {
        let mut seed = 0x5eed_f10f_u64;
        let mut draw = |modulus: u64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            seed % modulus
        };
        for case in 0..24 {
            let mut edges = Vec::new();
            let mut slopes = Vec::new();
            for (name, target) in [("a", "a"), ("b", "b"), ("c", "c")] {
                let id = format!("s{name}");
                edges.push(edge(&id, "s", target, 1 + draw(6)));
                let slope = draw(3);
                if slope > 0 {
                    slopes.push(ParametricCapacitySlope {
                        edge: EdgeId::parse(&id).expect("source edge ID"),
                        slope: BigInt::from(slope),
                    });
                }
            }
            for name in ["a", "b", "c"] {
                let id = format!("{name}t");
                let magnitude = draw(3);
                edges.push(edge(&id, name, "t", 3 * magnitude + 1 + draw(6)));
                if magnitude > 0 {
                    slopes.push(ParametricCapacitySlope {
                        edge: EdgeId::parse(&id).expect("sink edge ID"),
                        slope: -BigInt::from(magnitude),
                    });
                }
            }
            for (id, from, to) in [
                ("ab", "a", "b"),
                ("ba", "b", "a"),
                ("bc", "b", "c"),
                ("ca", "c", "a"),
            ] {
                edges.push(edge(id, from, to, draw(5)));
            }
            if case % 3 == 0 {
                edges.push(edge("ab-parallel", "a", "b", draw(5)));
            }
            if case % 4 == 0 {
                edges.push(edge("st-constant", "s", "t", draw(4)));
            }
            let graph = FlowNetwork::new(
                vec![node("a"), node("b"), node("c"), node("s"), node("t")],
                edges,
            )
            .expect("random graph");
            let problem = ParametricMaxFlowProblem::new(
                &graph,
                graph
                    .node_index(&NodeId::parse("s").expect("s"))
                    .expect("source"),
                graph
                    .node_index(&NodeId::parse("t").expect("t"))
                    .expect("sink"),
                ParametricRational::from_integer(BigInt::zero()),
                ParametricRational::from_integer(BigInt::from(3)),
                slopes,
            )
            .expect("random problem");
            let result = solve_parametric_breakpoint_rerun(&graph, &problem)
                .unwrap_or_else(|error| panic!("case {case}: {error}"));
            exhaustive_check(&graph, &problem, &result);
        }
    }

    #[test]
    fn model_rejects_wrong_monotonicity_and_nonterminal_coefficients() {
        let (graph, problem) = fixture();
        let source = problem.source();
        let sink = problem.sink();
        assert_eq!(
            ParametricMaxFlowProblem::new(
                &graph,
                source,
                sink,
                problem.minimum().clone(),
                problem.maximum().clone(),
                vec![ParametricCapacitySlope {
                    edge: EdgeId::parse("sa").expect("sa"),
                    slope: BigInt::from(-1),
                }],
            ),
            Err(ParametricBreakpointRerunError::InvalidMonotonicity)
        );
        assert_eq!(
            ParametricMaxFlowProblem::new(
                &graph,
                source,
                sink,
                problem.minimum().clone(),
                problem.maximum().clone(),
                vec![ParametricCapacitySlope {
                    edge: EdgeId::parse("at").expect("at"),
                    slope: BigInt::one(),
                }],
            ),
            Err(ParametricBreakpointRerunError::InvalidMonotonicity)
        );
    }

    #[test]
    fn coefficient_declaration_order_is_irrelevant() {
        let graph = FlowNetwork::new(
            vec![node("a"), node("b"), node("s"), node("t")],
            vec![
                edge("sa", "s", "a", 1),
                edge("sb", "s", "b", 2),
                edge("at", "a", "t", 7),
                edge("bt", "b", "t", 8),
            ],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        let coefficient = |edge: &str, slope: i64| ParametricCapacitySlope {
            edge: EdgeId::parse(edge).expect("edge ID"),
            slope: BigInt::from(slope),
        };
        let build = |coefficients| {
            ParametricMaxFlowProblem::new(
                &graph,
                source,
                sink,
                ParametricRational::from_integer(BigInt::zero()),
                ParametricRational::from_integer(BigInt::from(4)),
                coefficients,
            )
            .expect("problem")
        };
        let forward = build(vec![coefficient("sa", 2), coefficient("bt", -1)]);
        let reversed = build(vec![coefficient("bt", -1), coefficient("sa", 2)]);
        assert_eq!(
            solve_parametric_breakpoint_rerun(&graph, &forward).expect("forward"),
            solve_parametric_breakpoint_rerun(&graph, &reversed).expect("reversed")
        );
    }

    #[test]
    fn coefficient_admission_is_strict_and_stable_id_based() {
        let graph = FlowNetwork::new(
            vec![node("a"), node("s"), node("t")],
            vec![
                edge("sa", "s", "a", 1),
                edge("at", "a", "t", 5),
                edge("st", "s", "t", 3),
            ],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        let minimum = ParametricRational::from_integer(BigInt::zero());
        let maximum = ParametricRational::from_integer(BigInt::one());
        let build = |coefficients| {
            ParametricMaxFlowProblem::new(
                &graph,
                source,
                sink,
                minimum.clone(),
                maximum.clone(),
                coefficients,
            )
        };
        let coefficient = |edge: &str, slope: BigInt| ParametricCapacitySlope {
            edge: EdgeId::parse(edge).expect("edge ID"),
            slope,
        };

        assert_eq!(
            build(vec![coefficient("missing", BigInt::one())]),
            Err(ParametricBreakpointRerunError::MissingEdge)
        );
        assert_eq!(
            build(vec![
                coefficient("sa", BigInt::one()),
                coefficient("sa", BigInt::from(2)),
            ]),
            Err(ParametricBreakpointRerunError::DuplicateCoefficient)
        );
        assert_eq!(
            build(vec![coefficient("sa", BigInt::zero())]),
            Err(ParametricBreakpointRerunError::ZeroCoefficient)
        );
        assert_eq!(
            build(vec![coefficient("st", BigInt::one())]),
            Err(ParametricBreakpointRerunError::InvalidCoefficientLocation)
        );
        assert_eq!(
            build(vec![coefficient("sa", BigInt::from(10).pow(128_u32))]),
            Err(ParametricBreakpointRerunError::AdmissionLimit)
        );
    }
}
