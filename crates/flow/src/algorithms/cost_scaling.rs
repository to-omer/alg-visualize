//! Exact cost-scaling minimum-cost flow with generic push--relabel refinement.

use std::collections::{BTreeSet, VecDeque};

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{EdgeId, FlowNetwork, NodeIndex};
use crate::residual::{
    ResidualArc, ResidualArcId, ResidualDirection, ResidualError, ResidualState,
};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetricId,
    FlowTraceMetrics, FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative interactive admission limit for generic cost scaling.
pub const COST_SCALING_MAX_NODES: usize = 512;
/// Conservative interactive edge admission limit for generic cost scaling.
pub const COST_SCALING_MAX_EDGES: usize = 4_096;
/// Deterministic guard against unexpectedly many push/relabel operations.
pub const COST_SCALING_MAX_STATE_TRANSITIONS: u128 = 500_000;
/// Deterministic guard against pathological current-arc scanning.
pub const COST_SCALING_MAX_RESIDUAL_ARC_SCANS: u128 = 4_000_000;
/// Source-backed default path bound for partial augment--relabel refinement.
pub const PARTIAL_AUGMENT_RELABEL_MCF_PATH_LENGTH: usize = 4;
/// Published speculative arc-fixing coefficient, exactly `0.225 = 9 / 40`.
pub const ARC_FIXING_BETA_NUMERATOR: u128 = 9;
/// Denominator of the published speculative arc-fixing coefficient.
pub const ARC_FIXING_BETA_DENOMINATOR: u128 = 40;

/// Exact deterministic counters from generic cost-scaling refinement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CostScalingMetrics {
    /// Epsilon-refinement phases entered, including phases with no active node.
    pub refine_phases: u64,
    /// Negative reduced-cost residual arcs saturated at refine boundaries.
    pub initial_saturations: u64,
    /// Admissible local pushes performed while restoring conservation.
    pub pushes: u64,
    /// Pushes that exhaust the selected residual arc.
    pub saturating_pushes: u64,
    /// Pushes that exhaust the active vertex before the selected residual arc.
    pub nonsaturating_pushes: u64,
    /// Price increases performed after a complete current-arc scan.
    pub relabels: u64,
    /// Active vertices selected by the deterministic smallest-ID scheduler.
    pub active_vertex_selections: u64,
    /// Active-vertex discharges completed.
    pub discharges: u64,
    /// Residual arcs inspected by phase saturation, current-arc, and relabel scans.
    pub residual_arc_scans: u128,
    /// Current arcs rejected because they were absent, saturated, or inadmissible.
    pub current_arc_advances: u128,
    /// Admissible-path searches started by augment--relabel variants.
    pub path_searches: u64,
    /// Admissible arcs appended to augment--relabel search paths.
    pub path_advances: u128,
    /// Search-path arcs removed after relabeling a non-root tip.
    pub retreats: u64,
    /// Completed sequential path augmentations.
    pub path_augmentations: u64,
    /// Path augmentations ending at a deficit vertex.
    pub deficit_augmentations: u64,
    /// Partial augmentations ending at the configured path-length bound.
    pub length_limit_augmentations: u64,
    /// Potential-only epsilon-refinement attempts.
    pub price_refinement_attempts: u64,
    /// Potential-only attempts that proved the unchanged flow epsilon-optimal.
    pub price_refinement_successes: u64,
    /// Potential-only attempts rejected by a negative difference-constraint cycle.
    pub price_refinement_failures: u64,
    /// Complete Bellman--Ford difference-constraint rounds.
    pub price_refinement_rounds: u64,
    /// Successful potential relaxations across all price-refinement attempts.
    pub price_refinement_relaxations: u128,
    /// Residual arc identities inspected by price refinement.
    pub price_refinement_arc_scans: u128,
    /// Completed scans that recomputed the speculative fixed-arc set.
    pub arc_fixing_passes: u64,
    /// Original arcs newly excluded from refinement.
    pub arcs_fixed: u64,
    /// Fixed arcs restored by threshold return, fix-in, or restricted-set recovery.
    pub arcs_unfixed: u64,
    /// Fixed arcs restored and saturated after violating complementary slackness.
    pub fix_ins: u64,
    /// Residual identities skipped because their original arc was fixed.
    pub fixed_arc_skips: u128,
    /// Conservative recoveries that atomically restored every fixed arc.
    pub arc_fixing_recoveries: u64,
}

/// Certified canonical cost-scaling result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostScalingResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed objective and dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: CostScalingMetrics,
    /// Initial scaled epsilon before the first refine phase.
    pub initial_epsilon: i128,
    /// Original arcs still fixed at the certified final boundary.
    pub fixed_edges: Vec<EdgeId>,
}

/// Certified cost-scaling result with reversible push/relabel events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostScalingTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: CostScalingResult,
    /// Replay boundary at the arbitrary feasible initial flow.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after independent minimum-cost certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Cost-scaling construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CostScalingError {
    /// Input exceeds the practical interactive admission band.
    #[error("graph exceeds cost-scaling admission limits")]
    AdmissionLimit,
    /// A deterministic work ceiling was reached.
    #[error("cost-scaling work limit reached")]
    WorkLimit,
    /// A feasibility precheck proved that the requested balances are impossible.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Final independent certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded its declared domain.
    #[error("cost-scaling arithmetic overflow")]
    ArithmeticOverflow,
    /// A positive-excess vertex had no positive-capacity outgoing residual arc.
    #[error("cost-scaling active vertex has no outgoing residual capacity")]
    MissingOutgoingResidual,
    /// An implementation boundary violated epsilon optimality.
    #[error("cost-scaling epsilon-optimality invariant failed")]
    EpsilonOptimality,
    /// A refine phase failed to restore every required node balance.
    #[error("cost-scaling refine phase did not restore conservation")]
    Conservation,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves exact balanced minimum-cost flow by Goldberg--Tarjan cost scaling.
///
/// Costs are multiplied by `n + 1`, permitting integral epsilon and price
/// arithmetic. Every refine phase halves epsilon, saturates all negative
/// reduced-cost residual arcs, then uses the generic current-arc push/relabel
/// procedure to restore the requested balances. At scaled epsilon one, the
/// corresponding unscaled error is strictly below `1 / n`, which is exact for
/// integral costs.
///
/// # Errors
///
/// Rejects admission, feasibility, arithmetic, residual mutation, work-limit,
/// invariant, or independent certificate failures.
pub fn solve_cost_scaling(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingResult, CostScalingError> {
    solve_internal(
        graph,
        required_divergence,
        false,
        TraceIdentity::CostScaling,
        RefineMethod::Push,
    )
    .map(|run| run.result)
}

/// Solves the same exact kernel under the explicit push--relabel catalog entry.
///
/// # Errors
///
/// Returns the same failures as [`solve_cost_scaling`].
pub fn solve_cost_scaling_push_relabel(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingResult, CostScalingError> {
    solve_internal(
        graph,
        required_divergence,
        false,
        TraceIdentity::PushRelabel,
        RefineMethod::Push,
    )
    .map(|run| run.result)
}

/// Runs Goldberg's generalized cost-scaling framework with the refine
/// parameter fixed explicitly to the generic current-arc push--relabel method.
///
/// This is a configured framework variant, not an independent optimization
/// kernel. Its dedicated trace identity makes the chosen refine contract
/// visible without pretending that it computes a different optimum.
///
/// # Errors
///
/// Returns the same failures as [`solve_cost_scaling`].
pub fn solve_generalized_cost_scaling_push_relabel(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingResult, CostScalingError> {
    solve_internal(
        graph,
        required_divergence,
        false,
        TraceIdentity::GeneralizedPushRelabel,
        RefineMethod::Push,
    )
    .map(|run| run.result)
}

/// Records every refine, saturation, current-arc, push, and relabel boundary.
///
/// # Errors
///
/// Returns the same failures as [`solve_cost_scaling`] plus trace failures.
pub fn trace_cost_scaling(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingTraceResult, CostScalingError> {
    trace_internal(graph, required_divergence, TraceIdentity::CostScaling)
}

/// Records the same exact kernel under the push--relabel catalog identity.
///
/// # Errors
///
/// Returns the same failures as [`solve_cost_scaling_push_relabel`] plus trace
/// failures.
pub fn trace_cost_scaling_push_relabel(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingTraceResult, CostScalingError> {
    trace_internal(graph, required_divergence, TraceIdentity::PushRelabel)
}

/// Records the generalized framework with its push--relabel refine parameter
/// and dedicated catalog event identity.
///
/// # Errors
///
/// Returns the same failures as [`solve_generalized_cost_scaling_push_relabel`]
/// plus trace failures.
pub fn trace_generalized_cost_scaling_push_relabel(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingTraceResult, CostScalingError> {
    trace_internal(
        graph,
        required_divergence,
        TraceIdentity::GeneralizedPushRelabel,
    )
}

/// Solves exact minimum-cost flow with unbounded augment--relabel refinement.
///
/// Each FIFO-selected active root grows an admissible residual path. A stuck
/// path tip is relabeled and, unless it is the root, the search retreats one
/// arc. Reaching a deficit vertex triggers the source-defined sequential
/// maximum push along the path.
///
/// # Errors
///
/// Returns the same failures as [`solve_cost_scaling`].
pub fn solve_augment_relabel(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingResult, CostScalingError> {
    solve_internal(
        graph,
        required_divergence,
        false,
        TraceIdentity::AugmentRelabel,
        RefineMethod::Augment,
    )
    .map(|run| run.result)
}

/// Solves exact minimum-cost flow with length-four partial augment--relabel.
///
/// # Errors
///
/// Returns the same failures as [`solve_cost_scaling`].
pub fn solve_partial_augment_relabel_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingResult, CostScalingError> {
    solve_internal(
        graph,
        required_divergence,
        false,
        TraceIdentity::PartialAugmentRelabel,
        RefineMethod::Partial,
    )
    .map(|run| run.result)
}

/// Records every path extension, tip relabel, retreat, and augmentation for
/// augment--relabel refinement.
///
/// # Errors
///
/// Returns the same failures as [`solve_augment_relabel`] plus trace failures.
pub fn trace_augment_relabel(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingTraceResult, CostScalingError> {
    trace_internal(graph, required_divergence, TraceIdentity::AugmentRelabel)
}

/// Records the length-four partial augment--relabel refinement.
///
/// # Errors
///
/// Returns the same failures as [`solve_partial_augment_relabel_mcf`] plus
/// trace failures.
pub fn trace_partial_augment_relabel_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingTraceResult, CostScalingError> {
    trace_internal(
        graph,
        required_divergence,
        TraceIdentity::PartialAugmentRelabel,
    )
}

/// Solves exact minimum-cost flow with Goldberg's price-refinement heuristic.
///
/// At each reduced epsilon, a difference-constraints pass first attempts to
/// certify the unchanged feasible flow by modifying only node prices. A
/// successful attempt skips push--relabel refinement; a negative cycle rolls
/// prices back atomically before the exact generic refine is run.
///
/// # Errors
///
/// Returns the same failures as [`solve_cost_scaling`].
pub fn solve_price_refinement(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingResult, CostScalingError> {
    solve_internal(
        graph,
        required_divergence,
        false,
        TraceIdentity::PriceRefinement,
        RefineMethod::Push,
    )
    .map(|run| run.result)
}

/// Records potential-only attempts, successful relaxations, rollback on
/// failure, and any fallback push--relabel refinement.
///
/// # Errors
///
/// Returns the same failures as [`solve_price_refinement`] plus trace failures.
pub fn trace_price_refinement(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingTraceResult, CostScalingError> {
    trace_internal(graph, required_divergence, TraceIdentity::PriceRefinement)
}

/// Solves exact minimum-cost flow with a bound-only speculative Arc Fixing variant.
///
/// The published default threshold `beta = 0.225 n^(3/4)` is represented with
/// exact integer arithmetic. Unlike the paper's broader heuristic, this project
/// fixes only nonzero-width original arcs already at a complementary-slackness
/// capacity bound. A violation takes precedence and performs the paper's fix-in;
/// otherwise an arc that re-enters the threshold band is restored. If a
/// capacitated super-source/super-sink max-flow proves the restricted residual
/// problem infeasible, every fixed arc is restored and the refine phase retries.
///
/// # Errors
///
/// Returns the same failures as [`solve_cost_scaling`].
pub fn solve_arc_fixing(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingResult, CostScalingError> {
    solve_internal(
        graph,
        required_divergence,
        false,
        TraceIdentity::ArcFixing,
        RefineMethod::Push,
    )
    .map(|run| run.result)
}

/// Records threshold unfixing, fix-ins, fixed-set replacement, and recovery.
///
/// # Errors
///
/// Returns the same failures as [`solve_arc_fixing`] plus trace failures.
pub fn trace_arc_fixing(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CostScalingTraceResult, CostScalingError> {
    trace_internal(graph, required_divergence, TraceIdentity::ArcFixing)
}

#[derive(Clone, Copy)]
enum TraceIdentity {
    CostScaling,
    PushRelabel,
    GeneralizedPushRelabel,
    AugmentRelabel,
    PartialAugmentRelabel,
    PriceRefinement,
    ArcFixing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RefineMethod {
    Push,
    Augment,
    Partial,
}

/// Closed set of cost-scaling trace identities sharing the exact refinement
/// kernel while preserving their distinct source catalogs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CostScalingExecutionPreset {
    /// Canonical Goldberg--Tarjan cost scaling.
    CostScaling,
    /// Explicit cost-scaling push--relabel catalog identity.
    PushRelabel,
    /// Generalized framework configured with push--relabel refinement.
    GeneralizedPushRelabel,
    /// Unbounded augment--relabel refinement.
    AugmentRelabel,
    /// Length-bounded partial augment--relabel refinement.
    PartialAugmentRelabel,
    /// Price-refinement heuristic followed by exact refinement when needed.
    PriceRefinement,
    /// Bound-only speculative arc fixing.
    ArcFixing,
}

impl CostScalingExecutionPreset {
    const fn identity(self) -> TraceIdentity {
        match self {
            Self::CostScaling => TraceIdentity::CostScaling,
            Self::PushRelabel => TraceIdentity::PushRelabel,
            Self::GeneralizedPushRelabel => TraceIdentity::GeneralizedPushRelabel,
            Self::AugmentRelabel => TraceIdentity::AugmentRelabel,
            Self::PartialAugmentRelabel => TraceIdentity::PartialAugmentRelabel,
            Self::PriceRefinement => TraceIdentity::PriceRefinement,
            Self::ArcFixing => TraceIdentity::ArcFixing,
        }
    }
}

/// Solves one cost-scaling preset while reporting the exact feasible-flow
/// construction performed by that same execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_cost_scaling_preset_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    preset: CostScalingExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<CostScalingResult, CostScalingError> {
    let identity = preset.identity();
    solve_internal_with_feasibility(
        graph,
        required_divergence,
        false,
        identity,
        identity.method(),
        feasibility,
    )
    .map(|run| run.result)
}

/// Traces one cost-scaling preset while explicitly publishing the feasible
/// initial-flow construction performed by that same source execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_cost_scaling_preset_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    preset: CostScalingExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<CostScalingTraceResult, CostScalingError> {
    trace_internal_with_feasibility(graph, required_divergence, preset.identity(), feasibility)
}

impl TraceIdentity {
    const fn method(self) -> RefineMethod {
        match self {
            Self::CostScaling
            | Self::PushRelabel
            | Self::GeneralizedPushRelabel
            | Self::PriceRefinement
            | Self::ArcFixing => RefineMethod::Push,
            Self::AugmentRelabel => RefineMethod::Augment,
            Self::PartialAugmentRelabel => RefineMethod::Partial,
        }
    }

    const fn uses_price_refinement(self) -> bool {
        matches!(self, Self::PriceRefinement)
    }

    const fn uses_arc_fixing(self) -> bool {
        matches!(self, Self::ArcFixing)
    }

    const fn event(self, event: EventKind) -> FlowTraceEventMetadata {
        let (catalog_id, pseudocode_line) = match self {
            Self::CostScaling => cost_scaling_event(event),
            Self::PushRelabel => cost_scaling_push_event(event),
            Self::GeneralizedPushRelabel => generalized_cost_scaling_event(event),
            Self::AugmentRelabel => augment_relabel_event(event),
            Self::PartialAugmentRelabel => partial_augment_relabel_event(event),
            Self::PriceRefinement => price_refinement_event(event),
            Self::ArcFixing => arc_fixing_event(event),
        };
        FlowTraceEventMetadata {
            catalog_id,
            minimum_granularity: event.granularity(),
            pseudocode_line,
        }
    }

    const fn inspect_event(self) -> FlowTraceEventMetadata {
        let catalog_id = match self {
            Self::CostScaling => "cost-scaling.inspect-residual-arc",
            Self::PushRelabel => "cost-scaling-push-relabel.inspect-residual-arc",
            Self::GeneralizedPushRelabel => "generalized-cost-scaling.inspect-residual-arc",
            Self::AugmentRelabel => "augment-relabel.inspect-residual-arc",
            Self::PartialAugmentRelabel => "partial-augment-relabel-mcf.inspect-residual-arc",
            Self::PriceRefinement => "price-refinement.inspect-residual-arc",
            Self::ArcFixing => "arc-fixing.inspect-residual-arc",
        };
        FlowTraceEventMetadata {
            catalog_id,
            minimum_granularity: TraceGranularityV1::Micro,
            pseudocode_line: "cost-scaling:inspect-residual-arc",
        }
    }
}

const fn generalized_cost_scaling_event(event: EventKind) -> (&'static str, &'static str) {
    match event {
        EventKind::Initialize => (
            "generalized-cost-scaling.initialize-feasible-circulation",
            "generalized-cost-scaling:initialize-and-bind-push-relabel-refine",
        ),
        EventKind::StartRefine => (
            "generalized-cost-scaling.start-refine",
            "generalized-cost-scaling:halve-epsilon-and-start-bound-refine",
        ),
        EventKind::Saturate => (
            "generalized-cost-scaling.saturate-negative-arc",
            "generalized-cost-scaling:saturate-negative-reduced-cost-residual",
        ),
        EventKind::SelectActive => (
            "generalized-cost-scaling.select-active-vertex",
            "generalized-cost-scaling:select-smallest-id-positive-excess",
        ),
        EventKind::Advance => (
            "generalized-cost-scaling.advance-current-arc",
            "generalized-cost-scaling:advance-current-arc",
        ),
        EventKind::Push => (
            "generalized-cost-scaling.push",
            "generalized-cost-scaling:push-on-admissible-current-arc",
        ),
        EventKind::Relabel => (
            "generalized-cost-scaling.relabel",
            "generalized-cost-scaling:increase-price-and-reset-current-arc",
        ),
        EventKind::CompleteDischarge => (
            "generalized-cost-scaling.complete-discharge",
            "generalized-cost-scaling:finish-active-vertex-discharge",
        ),
        EventKind::CompleteRefine => (
            "generalized-cost-scaling.complete-refine",
            "generalized-cost-scaling:verify-epsilon-optimal-feasible-flow",
        ),
        EventKind::Optimal => (
            "generalized-cost-scaling.optimal",
            "generalized-cost-scaling:return-independent-certificate",
        ),
        _ => cost_scaling_event(event),
    }
}

const fn cost_scaling_event(event: EventKind) -> (&'static str, &'static str) {
    match event {
        EventKind::Initialize => (
            "cost-scaling.initialize-feasible-circulation",
            "cost-scaling:initialize-feasible-circulation-and-prices",
        ),
        EventKind::StartRefine => (
            "cost-scaling.start-refine",
            "cost-scaling:halve-epsilon-and-start-refine",
        ),
        EventKind::Saturate => (
            "cost-scaling.saturate-negative-arc",
            "cost-scaling:saturate-negative-reduced-cost-arcs",
        ),
        EventKind::SelectActive => (
            "cost-scaling.select-active-vertex",
            "cost-scaling:select-active-vertex",
        ),
        EventKind::Advance => (
            "cost-scaling.advance-current-arc",
            "cost-scaling:advance-current-arc",
        ),
        EventKind::Push => (
            "cost-scaling.push",
            "cost-scaling:push-on-admissible-current-arc",
        ),
        EventKind::Relabel => (
            "cost-scaling.relabel",
            "cost-scaling:increase-price-and-reset-current-arc",
        ),
        EventKind::CompleteDischarge => (
            "cost-scaling.complete-discharge",
            "cost-scaling:complete-active-vertex-discharge",
        ),
        EventKind::CompleteRefine => (
            "cost-scaling.complete-refine",
            "cost-scaling:restore-epsilon-optimal-circulation",
        ),
        EventKind::Optimal => (
            "cost-scaling.optimal",
            "cost-scaling:return-minimum-cost-flow",
        ),
        EventKind::PathAdvance
        | EventKind::Retreat
        | EventKind::AugmentDeficit
        | EventKind::AugmentLimit
        | EventKind::StartPriceRefinement
        | EventKind::PriceRelax
        | EventKind::CompletePriceRound
        | EventKind::PriceRefinementSuccess
        | EventKind::PriceRefinementFailure
        | EventKind::UnfixThreshold
        | EventKind::FixIn
        | EventKind::UpdateFixedSet
        | EventKind::RecoverFixedSet => (
            "cost-scaling.unused-path-control",
            "cost-scaling:unused-path-control",
        ),
    }
}

const fn cost_scaling_push_event(event: EventKind) -> (&'static str, &'static str) {
    match event {
        EventKind::Initialize => (
            "cost-scaling-push-relabel.initialize-feasible-circulation",
            "cost-scaling-push-relabel:initialize-feasible-circulation-and-prices",
        ),
        EventKind::StartRefine => (
            "cost-scaling-push-relabel.start-refine",
            "cost-scaling-push-relabel:halve-epsilon-and-start-refine",
        ),
        EventKind::Saturate => (
            "cost-scaling-push-relabel.saturate-negative-arc",
            "cost-scaling-push-relabel:saturate-negative-reduced-cost-arcs",
        ),
        EventKind::SelectActive => (
            "cost-scaling-push-relabel.select-active-vertex",
            "cost-scaling-push-relabel:select-active-vertex",
        ),
        EventKind::Advance => (
            "cost-scaling-push-relabel.advance-current-arc",
            "cost-scaling-push-relabel:advance-current-arc",
        ),
        EventKind::Push => (
            "cost-scaling-push-relabel.push",
            "cost-scaling-push-relabel:push-on-admissible-current-arc",
        ),
        EventKind::Relabel => (
            "cost-scaling-push-relabel.relabel",
            "cost-scaling-push-relabel:increase-price-and-reset-current-arc",
        ),
        EventKind::CompleteDischarge => (
            "cost-scaling-push-relabel.complete-discharge",
            "cost-scaling-push-relabel:complete-active-vertex-discharge",
        ),
        EventKind::CompleteRefine => (
            "cost-scaling-push-relabel.complete-refine",
            "cost-scaling-push-relabel:restore-epsilon-optimal-circulation",
        ),
        EventKind::Optimal => (
            "cost-scaling-push-relabel.optimal",
            "cost-scaling-push-relabel:return-minimum-cost-flow",
        ),
        EventKind::PathAdvance
        | EventKind::Retreat
        | EventKind::AugmentDeficit
        | EventKind::AugmentLimit
        | EventKind::StartPriceRefinement
        | EventKind::PriceRelax
        | EventKind::CompletePriceRound
        | EventKind::PriceRefinementSuccess
        | EventKind::PriceRefinementFailure
        | EventKind::UnfixThreshold
        | EventKind::FixIn
        | EventKind::UpdateFixedSet
        | EventKind::RecoverFixedSet => (
            "cost-scaling-push-relabel.unused-path-control",
            "cost-scaling-push-relabel:unused-path-control",
        ),
    }
}

const fn augment_relabel_event(event: EventKind) -> (&'static str, &'static str) {
    match event {
        EventKind::Initialize => (
            "augment-relabel.initialize-feasible-circulation",
            "augment-relabel:initialize-feasible-circulation-and-prices",
        ),
        EventKind::StartRefine => (
            "augment-relabel.start-refine",
            "augment-relabel:halve-epsilon-and-start-refine",
        ),
        EventKind::Saturate => (
            "augment-relabel.saturate-negative-arc",
            "augment-relabel:saturate-negative-reduced-cost-arcs",
        ),
        EventKind::SelectActive => (
            "augment-relabel.select-active-root",
            "augment-relabel:dequeue-positive-excess-root",
        ),
        EventKind::Advance => (
            "augment-relabel.advance-current-arc",
            "augment-relabel:scan-next-current-arc",
        ),
        EventKind::PathAdvance => (
            "augment-relabel.advance-path",
            "augment-relabel:append-admissible-path-arc",
        ),
        EventKind::Push => (
            "augment-relabel.push-path-arc",
            "augment-relabel:push-maximum-on-one-path-arc",
        ),
        EventKind::Relabel => (
            "augment-relabel.relabel-tip",
            "augment-relabel:increase-tip-price-and-reset-current-arc",
        ),
        EventKind::Retreat => (
            "augment-relabel.retreat-path",
            "augment-relabel:remove-last-path-arc",
        ),
        EventKind::AugmentDeficit => (
            "augment-relabel.augment-to-deficit",
            "augment-relabel:sequentially-push-path-to-deficit",
        ),
        EventKind::AugmentLimit => (
            "augment-relabel.augment-at-limit",
            "augment-relabel:sequentially-push-bounded-path",
        ),
        EventKind::CompleteDischarge => (
            "augment-relabel.complete-path-search",
            "augment-relabel:complete-active-root-path-search",
        ),
        EventKind::CompleteRefine => (
            "augment-relabel.complete-refine",
            "augment-relabel:restore-epsilon-optimal-circulation",
        ),
        EventKind::Optimal => (
            "augment-relabel.optimal",
            "augment-relabel:return-minimum-cost-flow",
        ),
        EventKind::StartPriceRefinement
        | EventKind::PriceRelax
        | EventKind::CompletePriceRound
        | EventKind::PriceRefinementSuccess
        | EventKind::PriceRefinementFailure
        | EventKind::UnfixThreshold
        | EventKind::FixIn
        | EventKind::UpdateFixedSet
        | EventKind::RecoverFixedSet => (
            "augment-relabel.unused-price-refinement",
            "augment-relabel:unused-price-refinement",
        ),
    }
}

const fn partial_augment_relabel_event(event: EventKind) -> (&'static str, &'static str) {
    match event {
        EventKind::Initialize => (
            "partial-augment-relabel-mcf.initialize-feasible-circulation",
            "partial-augment-relabel-mcf:initialize-feasible-circulation-and-prices",
        ),
        EventKind::StartRefine => (
            "partial-augment-relabel-mcf.start-refine",
            "partial-augment-relabel-mcf:halve-epsilon-and-start-refine",
        ),
        EventKind::Saturate => (
            "partial-augment-relabel-mcf.saturate-negative-arc",
            "partial-augment-relabel-mcf:saturate-negative-reduced-cost-arcs",
        ),
        EventKind::SelectActive => (
            "partial-augment-relabel-mcf.select-active-root",
            "partial-augment-relabel-mcf:dequeue-positive-excess-root",
        ),
        EventKind::Advance => (
            "partial-augment-relabel-mcf.advance-current-arc",
            "partial-augment-relabel-mcf:scan-next-current-arc",
        ),
        EventKind::PathAdvance => (
            "partial-augment-relabel-mcf.advance-path",
            "partial-augment-relabel-mcf:append-admissible-path-arc",
        ),
        EventKind::Push => (
            "partial-augment-relabel-mcf.push-path-arc",
            "partial-augment-relabel-mcf:push-maximum-on-one-path-arc",
        ),
        EventKind::Relabel => (
            "partial-augment-relabel-mcf.relabel-tip",
            "partial-augment-relabel-mcf:increase-tip-price-and-reset-current-arc",
        ),
        EventKind::Retreat => (
            "partial-augment-relabel-mcf.retreat-path",
            "partial-augment-relabel-mcf:remove-last-path-arc",
        ),
        EventKind::AugmentDeficit => (
            "partial-augment-relabel-mcf.augment-to-deficit",
            "partial-augment-relabel-mcf:sequentially-push-path-to-deficit",
        ),
        EventKind::AugmentLimit => (
            "partial-augment-relabel-mcf.augment-at-limit",
            "partial-augment-relabel-mcf:sequentially-push-bounded-path",
        ),
        EventKind::CompleteDischarge => (
            "partial-augment-relabel-mcf.complete-path-search",
            "partial-augment-relabel-mcf:complete-active-root-path-search",
        ),
        EventKind::CompleteRefine => (
            "partial-augment-relabel-mcf.complete-refine",
            "partial-augment-relabel-mcf:restore-epsilon-optimal-circulation",
        ),
        EventKind::Optimal => (
            "partial-augment-relabel-mcf.optimal",
            "partial-augment-relabel-mcf:return-minimum-cost-flow",
        ),
        EventKind::StartPriceRefinement
        | EventKind::PriceRelax
        | EventKind::CompletePriceRound
        | EventKind::PriceRefinementSuccess
        | EventKind::PriceRefinementFailure
        | EventKind::UnfixThreshold
        | EventKind::FixIn
        | EventKind::UpdateFixedSet
        | EventKind::RecoverFixedSet => (
            "partial-augment-relabel-mcf.unused-price-refinement",
            "partial-augment-relabel-mcf:unused-price-refinement",
        ),
    }
}

const fn price_refinement_event(event: EventKind) -> (&'static str, &'static str) {
    match event {
        EventKind::Initialize => (
            "price-refinement.initialize-feasible-circulation",
            "price-refinement:initialize-feasible-circulation-and-prices",
        ),
        EventKind::StartRefine => (
            "price-refinement.start-refine",
            "price-refinement:halve-epsilon",
        ),
        EventKind::StartPriceRefinement => (
            "price-refinement.start-potential-only-attempt",
            "price-refinement:keep-flow-and-start-difference-constraints",
        ),
        EventKind::PriceRelax => (
            "price-refinement.relax-price",
            "price-refinement:relax-one-reversed-residual-constraint",
        ),
        EventKind::CompletePriceRound => (
            "price-refinement.complete-relaxation-round",
            "price-refinement:complete-one-difference-constraint-round",
        ),
        EventKind::PriceRefinementSuccess => (
            "price-refinement.succeed-without-flow-change",
            "price-refinement:certify-epsilon-optimality-and-skip-refine",
        ),
        EventKind::PriceRefinementFailure => (
            "price-refinement.fail-and-rollback-prices",
            "price-refinement:detect-negative-cycle-and-restore-prices",
        ),
        EventKind::Saturate => (
            "price-refinement.saturate-negative-arc",
            "price-refinement:fallback-saturate-negative-reduced-cost-arcs",
        ),
        EventKind::SelectActive => (
            "price-refinement.select-active-vertex",
            "price-refinement:fallback-select-active-vertex",
        ),
        EventKind::Advance => (
            "price-refinement.advance-current-arc",
            "price-refinement:fallback-advance-current-arc",
        ),
        EventKind::Push => (
            "price-refinement.push",
            "price-refinement:fallback-push-on-admissible-current-arc",
        ),
        EventKind::Relabel => (
            "price-refinement.relabel",
            "price-refinement:fallback-increase-price-and-reset-current-arc",
        ),
        EventKind::CompleteDischarge => (
            "price-refinement.complete-discharge",
            "price-refinement:fallback-complete-active-vertex-discharge",
        ),
        EventKind::CompleteRefine => (
            "price-refinement.complete-refine",
            "price-refinement:complete-epsilon-phase",
        ),
        EventKind::Optimal => (
            "price-refinement.optimal",
            "price-refinement:return-minimum-cost-flow",
        ),
        EventKind::PathAdvance
        | EventKind::Retreat
        | EventKind::AugmentDeficit
        | EventKind::AugmentLimit
        | EventKind::UnfixThreshold
        | EventKind::FixIn
        | EventKind::UpdateFixedSet
        | EventKind::RecoverFixedSet => (
            "price-refinement.unused-path-control",
            "price-refinement:unused-path-control",
        ),
    }
}

const fn arc_fixing_event(event: EventKind) -> (&'static str, &'static str) {
    match event {
        EventKind::Initialize => (
            "arc-fixing.initialize-feasible-circulation",
            "arc-fixing:initialize-feasible-circulation-prices-and-empty-fixed-set",
        ),
        EventKind::StartRefine => (
            "arc-fixing.start-refine",
            "arc-fixing:halve-epsilon-and-check-fixed-arcs",
        ),
        EventKind::UnfixThreshold => (
            "arc-fixing.unfix-threshold-arcs",
            "arc-fixing:restore-arcs-inside-beta-band",
        ),
        EventKind::FixIn => (
            "arc-fixing.fix-in",
            "arc-fixing:unfix-and-saturate-complementary-slackness-violation",
        ),
        EventKind::RecoverFixedSet => (
            "arc-fixing.recover-fixed-set",
            "arc-fixing:restore-all-fixed-arcs-when-restricted-refine-is-infeasible",
        ),
        EventKind::Saturate => (
            "arc-fixing.saturate-negative-arc",
            "arc-fixing:saturate-unfixed-negative-reduced-cost-arcs",
        ),
        EventKind::SelectActive => (
            "arc-fixing.select-active-vertex",
            "arc-fixing:select-active-vertex",
        ),
        EventKind::Advance => (
            "arc-fixing.advance-current-arc",
            "arc-fixing:skip-fixed-or-inadmissible-current-arc",
        ),
        EventKind::Push => (
            "arc-fixing.push",
            "arc-fixing:push-on-unfixed-admissible-current-arc",
        ),
        EventKind::Relabel => (
            "arc-fixing.relabel",
            "arc-fixing:increase-price-over-unfixed-residual-arcs",
        ),
        EventKind::CompleteDischarge => (
            "arc-fixing.complete-discharge",
            "arc-fixing:complete-active-vertex-discharge",
        ),
        EventKind::CompleteRefine => (
            "arc-fixing.complete-refine",
            "arc-fixing:restore-epsilon-optimal-circulation",
        ),
        EventKind::UpdateFixedSet => (
            "arc-fixing.update-fixed-set",
            "arc-fixing:fix-bound-arcs-outside-published-beta-threshold",
        ),
        EventKind::Optimal => (
            "arc-fixing.optimal",
            "arc-fixing:return-certified-minimum-cost-flow",
        ),
        EventKind::PathAdvance
        | EventKind::Retreat
        | EventKind::AugmentDeficit
        | EventKind::AugmentLimit
        | EventKind::StartPriceRefinement
        | EventKind::PriceRelax
        | EventKind::CompletePriceRound
        | EventKind::PriceRefinementSuccess
        | EventKind::PriceRefinementFailure => {
            ("arc-fixing.unused-control", "arc-fixing:unused-control")
        }
    }
}

#[derive(Clone, Copy)]
enum EventKind {
    Initialize,
    StartRefine,
    Saturate,
    SelectActive,
    Advance,
    PathAdvance,
    Push,
    Relabel,
    Retreat,
    AugmentDeficit,
    AugmentLimit,
    CompleteDischarge,
    CompleteRefine,
    Optimal,
    StartPriceRefinement,
    PriceRelax,
    CompletePriceRound,
    PriceRefinementSuccess,
    PriceRefinementFailure,
    UnfixThreshold,
    FixIn,
    UpdateFixedSet,
    RecoverFixedSet,
}

impl EventKind {
    const fn granularity(self) -> TraceGranularityV1 {
        match self {
            Self::Initialize
            | Self::StartRefine
            | Self::CompleteRefine
            | Self::Optimal
            | Self::StartPriceRefinement
            | Self::PriceRefinementSuccess
            | Self::PriceRefinementFailure
            | Self::RecoverFixedSet => TraceGranularityV1::Phase,
            Self::Saturate
            | Self::SelectActive
            | Self::Relabel
            | Self::Retreat
            | Self::AugmentDeficit
            | Self::AugmentLimit
            | Self::CompleteDischarge
            | Self::CompletePriceRound
            | Self::UnfixThreshold
            | Self::FixIn
            | Self::UpdateFixedSet => TraceGranularityV1::Operation,
            Self::Advance | Self::PathAdvance | Self::Push | Self::PriceRelax => {
                TraceGranularityV1::Micro
            }
        }
    }
}

struct InternalRun {
    result: CostScalingResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

struct WorkingState<'graph> {
    residual: ResidualState<'graph>,
    excess: Vec<i128>,
    prices: Vec<i128>,
    metrics: CostScalingMetrics,
    transitions: u128,
    fixed_edges: BTreeSet<EdgeId>,
    identity: TraceIdentity,
}

fn trace_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    identity: TraceIdentity,
) -> Result<CostScalingTraceResult, CostScalingError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_internal_with_feasibility(graph, required_divergence, identity, &mut feasibility)
}

fn trace_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    identity: TraceIdentity,
    feasibility: &mut FeasibilityExecution,
) -> Result<CostScalingTraceResult, CostScalingError> {
    let run = solve_internal_with_feasibility(
        graph,
        required_divergence,
        true,
        identity,
        identity.method(),
        feasibility,
    )?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(CostScalingError::Conservation)?;
    Ok(CostScalingTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_enabled: bool,
    identity: TraceIdentity,
    method: RefineMethod,
) -> Result<InternalRun, CostScalingError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(
        graph,
        required_divergence,
        trace_enabled,
        identity,
        method,
        &mut feasibility,
    )
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_enabled: bool,
    identity: TraceIdentity,
    method: RefineMethod,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, CostScalingError> {
    validate_admission(graph)?;
    let feasible =
        feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::InitialFlow)?;
    let mut work = WorkingState {
        residual: ResidualState::from_flows(graph, &feasible.flows)?,
        excess: vec![0; graph.nodes().len()],
        prices: vec![0; graph.nodes().len()],
        metrics: CostScalingMetrics::default(),
        transitions: 0,
        fixed_edges: BTreeSet::new(),
        identity,
    };
    let cost_multiplier = i128::try_from(graph.nodes().len())
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    let initial_epsilon = initial_epsilon(graph, cost_multiplier)?;
    validate_epsilon_optimality(&work, cost_multiplier, initial_epsilon)?;

    let mut recorder = start_trace_recorder(graph, &work, trace_enabled)?;
    record_trace(
        recorder.as_mut(),
        graph,
        &work,
        identity.event(EventKind::Initialize),
        TraceView::prices(&work),
        Some(("epsilon", initial_epsilon)),
    )?;

    let all_arcs = all_residual_arc_ids(graph);
    let outgoing = outgoing_residual_arc_ids(graph);
    let mut epsilon = initial_epsilon;
    while epsilon > 1 {
        epsilon /= 2;
        refine(
            graph,
            &mut work,
            cost_multiplier,
            epsilon,
            &all_arcs,
            &outgoing,
            identity,
            method,
            &mut recorder,
        )?;
    }
    if work.excess.iter().any(|&value| value != 0) {
        return Err(CostScalingError::Conservation);
    }

    let flows = work.residual.flows().to_vec();
    let actual = divergences(graph, &flows)?;
    if actual != required_divergence {
        return Err(CostScalingError::Conservation);
    }
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    record_trace(
        recorder.as_mut(),
        graph,
        &work,
        identity.event(EventKind::Optimal),
        TraceView::prices(&work),
        Some(("total-cost", certificate.total_cost)),
    )?;
    Ok(InternalRun {
        result: CostScalingResult {
            flows,
            certificate,
            metrics: work.metrics,
            initial_epsilon,
            fixed_edges: work.fixed_edges.iter().cloned().collect(),
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

#[allow(clippy::too_many_arguments)]
fn refine(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    cost_multiplier: i128,
    epsilon: i128,
    all_arcs: &[ResidualArcId],
    outgoing: &[Vec<ResidualArcId>],
    identity: TraceIdentity,
    method: RefineMethod,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    work.metrics.refine_phases = work
        .metrics
        .refine_phases
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(EventKind::StartRefine),
        TraceView::prices(work),
        Some(("epsilon", epsilon)),
    )?;

    if identity.uses_arc_fixing() {
        prepare_fixed_arcs(graph, work, cost_multiplier, identity, recorder)?;
    }

    if identity.uses_price_refinement()
        && attempt_price_refinement(
            graph,
            work,
            cost_multiplier,
            epsilon,
            all_arcs,
            identity,
            recorder,
        )?
    {
        if work.excess.iter().any(|&value| value != 0) {
            return Err(CostScalingError::Conservation);
        }
        validate_epsilon_optimality(work, cost_multiplier, epsilon)?;
        record_trace(
            recorder.as_mut(),
            graph,
            work,
            identity.event(EventKind::CompleteRefine),
            TraceView::prices(work),
            Some(("epsilon", epsilon)),
        )?;
        return Ok(());
    }

    saturate_negative_arcs(
        graph,
        work,
        cost_multiplier,
        epsilon,
        all_arcs,
        identity,
        recorder,
    )?;
    validate_nonnegative_reduced_costs(work, cost_multiplier)?;

    if identity.uses_arc_fixing() {
        recover_restricted_refine_if_needed(graph, work, identity, recorder)?;
    }

    run_refine_method(
        graph,
        work,
        cost_multiplier,
        epsilon,
        outgoing,
        identity,
        method,
        recorder,
    )?;

    repair_after_arc_fixing_refine(
        graph,
        work,
        cost_multiplier,
        epsilon,
        all_arcs,
        outgoing,
        identity,
        recorder,
    )?;

    if work.excess.iter().any(|&value| value != 0) {
        return Err(CostScalingError::Conservation);
    }
    validate_epsilon_optimality(work, cost_multiplier, epsilon)?;
    if identity.uses_arc_fixing() {
        validate_fixed_complementary_slackness(graph, work, cost_multiplier)?;
    }
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(EventKind::CompleteRefine),
        TraceView::prices(work),
        Some(("epsilon", epsilon)),
    )?;
    if identity.uses_arc_fixing() {
        update_fixed_set(graph, work, cost_multiplier, identity, recorder)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_refine_method(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    cost_multiplier: i128,
    epsilon: i128,
    outgoing: &[Vec<ResidualArcId>],
    identity: TraceIdentity,
    method: RefineMethod,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    match method {
        RefineMethod::Push => refine_push_relabel(
            graph,
            work,
            cost_multiplier,
            epsilon,
            outgoing,
            identity,
            recorder,
        ),
        RefineMethod::Augment => refine_augment_relabel(
            graph,
            work,
            cost_multiplier,
            epsilon,
            outgoing,
            usize::MAX,
            identity,
            recorder,
        ),
        RefineMethod::Partial => refine_augment_relabel(
            graph,
            work,
            cost_multiplier,
            epsilon,
            outgoing,
            PARTIAL_AUGMENT_RELABEL_MCF_PATH_LENGTH,
            identity,
            recorder,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn repair_after_arc_fixing_refine(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    cost_multiplier: i128,
    epsilon: i128,
    all_arcs: &[ResidualArcId],
    outgoing: &[Vec<ResidualArcId>],
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    if !identity.uses_arc_fixing() {
        return Ok(());
    }
    loop {
        let unfixed_before = work.metrics.arcs_unfixed;
        prepare_fixed_arcs(graph, work, cost_multiplier, identity, recorder)?;
        if work.metrics.arcs_unfixed == unfixed_before {
            return Ok(());
        }
        saturate_negative_arcs(
            graph,
            work,
            cost_multiplier,
            epsilon,
            all_arcs,
            identity,
            recorder,
        )?;
        validate_nonnegative_reduced_costs(work, cost_multiplier)?;
        recover_restricted_refine_if_needed(graph, work, identity, recorder)?;
        refine_push_relabel(
            graph,
            work,
            cost_multiplier,
            epsilon,
            outgoing,
            identity,
            recorder,
        )?;
    }
}

fn prepare_fixed_arcs(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    cost_multiplier: i128,
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    let threshold = speculative_fixing_threshold(graph.nodes().len(), cost_multiplier)?;
    let fixed_before = work.fixed_edges.iter().cloned().collect::<Vec<_>>();
    let mut fix_in_directions = Vec::new();
    let mut threshold_unfixes = Vec::new();
    for edge_id in &fixed_before {
        increment_edge_scan(work, edge_id, identity, recorder.as_mut())?;
        let edge_index = graph
            .edge_index(edge_id)
            .ok_or(CostScalingError::EpsilonOptimality)?;
        let edge = graph
            .edge(edge_index)
            .ok_or(CostScalingError::EpsilonOptimality)?;
        let flow = *work
            .residual
            .flows()
            .get(edge_index.as_usize())
            .ok_or(CostScalingError::EpsilonOptimality)?;
        let reduced = scaled_edge_reduced_cost(edge, &work.prices, cost_multiplier)?;
        let fix_in_direction = if flow == edge.lower() && reduced < 0 {
            Some(ResidualDirection::Forward)
        } else if flow == edge.capacity() && reduced > 0 {
            Some(ResidualDirection::Reverse)
        } else if flow == edge.lower() || flow == edge.capacity() {
            None
        } else {
            return Err(CostScalingError::EpsilonOptimality);
        };
        if let Some(direction) = fix_in_direction {
            fix_in_directions.push((edge_id.clone(), direction));
        } else if reduced
            .checked_abs()
            .ok_or(CostScalingError::ArithmeticOverflow)?
            <= threshold
        {
            threshold_unfixes.push(edge_id.clone());
        }
    }
    for (edge_id, direction) in fix_in_directions {
        let id = ResidualArcId::new(edge_id.clone(), direction);
        let arc = work
            .residual
            .arc(&id)
            .ok_or(CostScalingError::MissingOutgoingResidual)?;
        work.fixed_edges.remove(&edge_id);
        if arc.capacity > 0 {
            increment_transition(work)?;
            work.residual
                .augment(std::slice::from_ref(&id), arc.capacity)?;
            update_excess(&mut work.excess, arc.from, arc.to, arc.capacity)?;
            work.metrics.initial_saturations = work
                .metrics
                .initial_saturations
                .checked_add(1)
                .ok_or(CostScalingError::ArithmeticOverflow)?;
        }
        work.metrics.arcs_unfixed = work
            .metrics
            .arcs_unfixed
            .checked_add(1)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        work.metrics.fix_ins = work
            .metrics
            .fix_ins
            .checked_add(1)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        record_trace(
            recorder.as_mut(),
            graph,
            work,
            identity.event(EventKind::FixIn),
            TraceView::arc(work, id),
            Some(("delta", i128::from(arc.capacity))),
        )?;
    }
    if !threshold_unfixes.is_empty() {
        for edge_id in threshold_unfixes {
            work.fixed_edges.remove(&edge_id);
            work.metrics.arcs_unfixed = work
                .metrics
                .arcs_unfixed
                .checked_add(1)
                .ok_or(CostScalingError::ArithmeticOverflow)?;
        }
        record_trace(
            recorder.as_mut(),
            graph,
            work,
            identity.event(EventKind::UnfixThreshold),
            TraceView::prices(work),
            Some(("beta", threshold)),
        )?;
    }
    Ok(())
}

fn recover_restricted_refine_if_needed(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    if work.fixed_edges.is_empty()
        || restricted_excess_can_reach_deficit(graph, work, identity, recorder)?
    {
        return Ok(());
    }
    let restored = usize_to_i128(work.fixed_edges.len())?;
    let restored_u64 =
        u64::try_from(work.fixed_edges.len()).map_err(|_| CostScalingError::ArithmeticOverflow)?;
    work.fixed_edges.clear();
    work.metrics.arcs_unfixed = work
        .metrics
        .arcs_unfixed
        .checked_add(restored_u64)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    work.metrics.arc_fixing_recoveries = work
        .metrics
        .arc_fixing_recoveries
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(EventKind::RecoverFixedSet),
        TraceView::prices(work),
        Some(("restored-arcs", restored)),
    )?;
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct RestrictedFeasibilityArc {
    to: usize,
    reverse: usize,
    capacity: u128,
}

fn add_restricted_feasibility_arc(
    network: &mut [Vec<RestrictedFeasibilityArc>],
    from: usize,
    to: usize,
    capacity: u128,
) {
    let forward_reverse = network[to].len();
    let reverse_reverse = network[from].len();
    network[from].push(RestrictedFeasibilityArc {
        to,
        reverse: forward_reverse,
        capacity,
    });
    network[to].push(RestrictedFeasibilityArc {
        to: from,
        reverse: reverse_reverse,
        capacity: 0,
    });
}

fn restricted_excess_can_reach_deficit(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<bool, CostScalingError> {
    let node_count = graph.nodes().len();
    let source = node_count;
    let sink = node_count + 1;
    let mut network = vec![Vec::new(); node_count + 2];
    for node in graph.node_indices() {
        let arcs = work.residual.outgoing_arcs(node);
        for arc in arcs {
            increment_scan(work, &arc.id, identity, recorder.as_mut())?;
            if arc.capacity > 0 && !work.fixed_edges.contains(arc.id.original_edge()) {
                add_restricted_feasibility_arc(
                    &mut network,
                    node.as_usize(),
                    arc.to.as_usize(),
                    u128::from(arc.capacity),
                );
            }
        }
    }
    let mut required = 0_u128;
    let mut deficit = 0_u128;
    for node in graph.node_indices() {
        let excess = work.excess[node.as_usize()];
        if excess > 0 {
            let capacity =
                u128::try_from(excess).map_err(|_| CostScalingError::ArithmeticOverflow)?;
            required = required
                .checked_add(capacity)
                .ok_or(CostScalingError::ArithmeticOverflow)?;
            add_restricted_feasibility_arc(&mut network, source, node.as_usize(), capacity);
        } else if excess < 0 {
            let capacity = u128::try_from(
                excess
                    .checked_neg()
                    .ok_or(CostScalingError::ArithmeticOverflow)?,
            )
            .map_err(|_| CostScalingError::ArithmeticOverflow)?;
            deficit = deficit
                .checked_add(capacity)
                .ok_or(CostScalingError::ArithmeticOverflow)?;
            add_restricted_feasibility_arc(&mut network, node.as_usize(), sink, capacity);
        }
    }
    if required != deficit {
        return Ok(false);
    }

    let mut routed = 0_u128;
    while routed < required {
        let mut parent = vec![None; network.len()];
        let mut queue = VecDeque::from([source]);
        parent[source] = Some((source, usize::MAX));
        while let Some(from) = queue.pop_front() {
            for (arc_index, arc) in network[from].iter().enumerate() {
                if arc.capacity > 0 && parent[arc.to].is_none() {
                    parent[arc.to] = Some((from, arc_index));
                    if arc.to == sink {
                        break;
                    }
                    queue.push_back(arc.to);
                }
            }
            if parent[sink].is_some() {
                break;
            }
        }
        if parent[sink].is_none() {
            return Ok(false);
        }
        let mut delta = required - routed;
        let mut cursor = sink;
        while cursor != source {
            let (from, arc_index) = parent[cursor].ok_or(CostScalingError::EpsilonOptimality)?;
            delta = delta.min(network[from][arc_index].capacity);
            cursor = from;
        }
        cursor = sink;
        while cursor != source {
            let (from, arc_index) = parent[cursor].ok_or(CostScalingError::EpsilonOptimality)?;
            let reverse = network[from][arc_index].reverse;
            network[from][arc_index].capacity -= delta;
            network[cursor][reverse].capacity = network[cursor][reverse]
                .capacity
                .checked_add(delta)
                .ok_or(CostScalingError::ArithmeticOverflow)?;
            cursor = from;
        }
        routed = routed
            .checked_add(delta)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
    }
    Ok(true)
}

fn update_fixed_set(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    cost_multiplier: i128,
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    let threshold = speculative_fixing_threshold(graph.nodes().len(), cost_multiplier)?;
    let before = work.fixed_edges.len();
    work.metrics.arc_fixing_passes = work
        .metrics
        .arc_fixing_passes
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    for (ordinal, edge) in graph.edges().iter().enumerate() {
        increment_edge_scan(work, edge.id(), identity, recorder.as_mut())?;
        if work.fixed_edges.contains(edge.id()) || edge.lower() == edge.capacity() {
            continue;
        }
        let flow = *work
            .residual
            .flows()
            .get(ordinal)
            .ok_or(CostScalingError::EpsilonOptimality)?;
        if flow != edge.lower() && flow != edge.capacity() {
            continue;
        }
        let reduced = scaled_edge_reduced_cost(edge, &work.prices, cost_multiplier)?;
        let complementary_slack =
            (flow == edge.lower() && reduced >= 0) || (flow == edge.capacity() && reduced <= 0);
        if complementary_slack
            && reduced
                .checked_abs()
                .ok_or(CostScalingError::ArithmeticOverflow)?
                > threshold
        {
            work.fixed_edges.insert(edge.id().clone());
            work.metrics.arcs_fixed = work
                .metrics
                .arcs_fixed
                .checked_add(1)
                .ok_or(CostScalingError::ArithmeticOverflow)?;
        }
    }
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(EventKind::UpdateFixedSet),
        TraceView::prices(work),
        Some(("beta", threshold)),
    )?;
    if work.fixed_edges.len() < before {
        return Err(CostScalingError::EpsilonOptimality);
    }
    Ok(())
}

fn validate_fixed_complementary_slackness(
    graph: &FlowNetwork,
    work: &WorkingState<'_>,
    cost_multiplier: i128,
) -> Result<(), CostScalingError> {
    for edge_id in &work.fixed_edges {
        let edge_index = graph
            .edge_index(edge_id)
            .ok_or(CostScalingError::EpsilonOptimality)?;
        let edge = graph
            .edge(edge_index)
            .ok_or(CostScalingError::EpsilonOptimality)?;
        let flow = *work
            .residual
            .flows()
            .get(edge_index.as_usize())
            .ok_or(CostScalingError::EpsilonOptimality)?;
        let reduced = scaled_edge_reduced_cost(edge, &work.prices, cost_multiplier)?;
        let valid =
            (flow == edge.lower() && reduced >= 0) || (flow == edge.capacity() && reduced <= 0);
        if !valid {
            return Err(CostScalingError::EpsilonOptimality);
        }
    }
    Ok(())
}

/// Attempts the price-only part of a cost-scaling phase as a system of
/// residual difference constraints. For every positive-capacity residual arc
/// `u -> v`, the new prices must satisfy
/// `price[u] - price[v] <= scaled_cost(u,v) + epsilon`.
///
/// Relaxing the reversed constraints from an implicit zero-distance super
/// source is a deterministic Bellman--Ford realization of Goldberg's scaling
/// shortest-path price refinement. A change in the `n`th round proves a
/// negative constraint cycle. Candidate prices are replay-visible while the
/// attempt runs, but are restored atomically on failure before exact fallback
/// refinement mutates the flow.
#[allow(clippy::too_many_arguments)]
fn attempt_price_refinement(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    cost_multiplier: i128,
    epsilon: i128,
    all_arcs: &[ResidualArcId],
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<bool, CostScalingError> {
    work.metrics.price_refinement_attempts = work
        .metrics
        .price_refinement_attempts
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(EventKind::StartPriceRefinement),
        TraceView::prices(work),
        Some(("epsilon", epsilon)),
    )?;

    let base_prices = work.prices.clone();
    let mut adjustments = vec![0_i128; graph.nodes().len()];
    for round in 1..=graph.nodes().len() {
        let changed = run_price_relaxation_round(
            graph,
            work,
            cost_multiplier,
            epsilon,
            all_arcs,
            &base_prices,
            &mut adjustments,
            round,
            identity,
            recorder,
        )?;

        if !changed {
            finish_price_refinement_success(
                graph,
                work,
                cost_multiplier,
                epsilon,
                all_arcs,
                identity,
                recorder,
            )?;
            return Ok(true);
        }
    }

    rollback_price_refinement_failure(graph, work, base_prices, identity, recorder)?;
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn run_price_relaxation_round(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    cost_multiplier: i128,
    epsilon: i128,
    all_arcs: &[ResidualArcId],
    base_prices: &[i128],
    adjustments: &mut [i128],
    round: usize,
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<bool, CostScalingError> {
    let mut changed = false;
    for id in all_arcs {
        increment_scan(work, id, identity, recorder.as_mut())?;
        work.metrics.price_refinement_arc_scans = work
            .metrics
            .price_refinement_arc_scans
            .checked_add(1)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        let arc = work
            .residual
            .arc(id)
            .ok_or(CostScalingError::MissingOutgoingResidual)?;
        if arc.capacity == 0 {
            continue;
        }
        let weight = scaled_reduced_cost(&arc, base_prices, cost_multiplier)?
            .checked_add(epsilon)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        let candidate = adjustments[arc.to.as_usize()]
            .checked_add(weight)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        if candidate >= adjustments[arc.from.as_usize()] {
            continue;
        }
        relax_price_constraint(
            graph,
            work,
            &arc,
            base_prices,
            adjustments,
            candidate,
            identity,
            recorder,
        )?;
        changed = true;
    }
    work.metrics.price_refinement_rounds = work
        .metrics
        .price_refinement_rounds
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(EventKind::CompletePriceRound),
        TraceView::prices(work),
        Some(("round", usize_to_i128(round)?)),
    )?;
    Ok(changed)
}

#[allow(clippy::too_many_arguments)]
fn relax_price_constraint(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    arc: &ResidualArc,
    base_prices: &[i128],
    adjustments: &mut [i128],
    candidate: i128,
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    increment_transition(work)?;
    adjustments[arc.from.as_usize()] = candidate;
    work.prices[arc.from.as_usize()] = base_prices[arc.from.as_usize()]
        .checked_add(candidate)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    work.metrics.price_refinement_relaxations = work
        .metrics
        .price_refinement_relaxations
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(EventKind::PriceRelax),
        TraceView::active(work, arc.from, vec![arc.id.clone()]),
        Some(("price", work.prices[arc.from.as_usize()])),
    )?;
    Ok(())
}

fn finish_price_refinement_success(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    cost_multiplier: i128,
    epsilon: i128,
    all_arcs: &[ResidualArcId],
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    validate_epsilon_optimality(work, cost_multiplier, epsilon)?;
    work.metrics.price_refinement_successes = work
        .metrics
        .price_refinement_successes
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    let view = if recorder.is_some() {
        let witness = minimum_price_refinement_slack_arc(work, cost_multiplier, epsilon, all_arcs)?;
        TraceView::arc(work, witness)
    } else {
        TraceView::prices(work)
    };
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(EventKind::PriceRefinementSuccess),
        view,
        Some(("flow-changes", 0)),
    )?;
    Ok(())
}

fn minimum_price_refinement_slack_arc(
    work: &WorkingState<'_>,
    cost_multiplier: i128,
    epsilon: i128,
    all_arcs: &[ResidualArcId],
) -> Result<ResidualArcId, CostScalingError> {
    let mut minimum: Option<(i128, ResidualArcId)> = None;
    for id in all_arcs {
        let arc = work
            .residual
            .arc(id)
            .ok_or(CostScalingError::MissingOutgoingResidual)?;
        if arc.capacity == 0 || work.fixed_edges.contains(id.original_edge()) {
            continue;
        }
        let slack = scaled_reduced_cost(&arc, &work.prices, cost_multiplier)?
            .checked_add(epsilon)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        if slack < 0 {
            return Err(CostScalingError::EpsilonOptimality);
        }
        if minimum.as_ref().is_none_or(|(current, _)| slack < *current) {
            minimum = Some((slack, id.clone()));
        }
    }
    minimum
        .map(|(_, id)| id)
        .ok_or(CostScalingError::MissingOutgoingResidual)
}

fn rollback_price_refinement_failure(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    base_prices: Vec<i128>,
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    work.prices = base_prices;
    work.metrics.price_refinement_failures = work
        .metrics
        .price_refinement_failures
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(EventKind::PriceRefinementFailure),
        TraceView::prices(work),
        Some(("negative-cycle", 1)),
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn refine_push_relabel(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    cost_multiplier: i128,
    epsilon: i128,
    outgoing: &[Vec<ResidualArcId>],
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    let mut current = vec![0_usize; graph.nodes().len()];
    while let Some(active) = smallest_active(graph, &work.excess) {
        select_active_root(graph, work, active, identity, recorder)?;
        discharge(
            graph,
            work,
            active,
            cost_multiplier,
            epsilon,
            outgoing,
            &mut current,
            identity,
            recorder,
        )?;
        work.metrics.discharges = work
            .metrics
            .discharges
            .checked_add(1)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        record_trace(
            recorder.as_mut(),
            graph,
            work,
            identity.event(EventKind::CompleteDischarge),
            TraceView::prices(work),
            None,
        )?;
    }
    Ok(())
}

fn select_active_root(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    active: NodeIndex,
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    increment_transition(work)?;
    work.metrics.active_vertex_selections = work
        .metrics
        .active_vertex_selections
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(EventKind::SelectActive),
        TraceView::active(work, active, Vec::new()),
        Some(("excess", work.excess[active.as_usize()])),
    )?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PathTermination {
    Deficit,
    LengthLimit,
}

#[allow(clippy::too_many_arguments)]
fn refine_augment_relabel(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    cost_multiplier: i128,
    epsilon: i128,
    outgoing: &[Vec<ResidualArcId>],
    maximum_path_length: usize,
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    let node_count = graph.nodes().len();
    let mut current = vec![0_usize; node_count];
    let mut queue = VecDeque::new();
    let mut queued = vec![false; node_count];
    for node in graph.node_indices() {
        enqueue_if_active(work, node, &mut queue, &mut queued);
    }

    while let Some(root) = queue.pop_front() {
        queued[root.as_usize()] = false;
        if work.excess[root.as_usize()] <= 0 {
            continue;
        }
        select_active_root(graph, work, root, identity, recorder)?;
        work.metrics.path_searches = work
            .metrics
            .path_searches
            .checked_add(1)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        augment_relabel_path_search(
            graph,
            work,
            root,
            cost_multiplier,
            epsilon,
            outgoing,
            &mut current,
            maximum_path_length,
            &mut queue,
            &mut queued,
            identity,
            recorder,
        )?;
        enqueue_if_active(work, root, &mut queue, &mut queued);
    }
    Ok(())
}

fn enqueue_if_active(
    work: &WorkingState<'_>,
    node: NodeIndex,
    queue: &mut VecDeque<NodeIndex>,
    queued: &mut [bool],
) {
    let index = node.as_usize();
    if work.excess[index] > 0 && !queued[index] {
        queue.push_back(node);
        queued[index] = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn augment_relabel_path_search(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    root: NodeIndex,
    cost_multiplier: i128,
    epsilon: i128,
    outgoing: &[Vec<ResidualArcId>],
    current: &mut [usize],
    maximum_path_length: usize,
    queue: &mut VecDeque<NodeIndex>,
    queued: &mut [bool],
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    let mut tip = root;
    let mut path = Vec::new();
    let termination = loop {
        if work.excess[tip.as_usize()] < 0 {
            break PathTermination::Deficit;
        }
        if path.len() >= maximum_path_length {
            break PathTermination::LengthLimit;
        }

        let tip_index = tip.as_usize();
        let ids = outgoing
            .get(tip_index)
            .ok_or(CostScalingError::MissingOutgoingResidual)?;
        let admissible = next_admissible_path_arc(
            graph,
            work,
            root,
            tip,
            &path,
            cost_multiplier,
            ids,
            &mut current[tip_index],
            identity,
            recorder,
        )?;

        if let Some(arc) = admissible {
            increment_transition(work)?;
            tip = arc.to;
            path.push(arc.id);
            work.metrics.path_advances = work
                .metrics
                .path_advances
                .checked_add(1)
                .ok_or(CostScalingError::ArithmeticOverflow)?;
            record_trace(
                recorder.as_mut(),
                graph,
                work,
                identity.event(EventKind::PathAdvance),
                TraceView::search_path(work, root, tip, path.clone()),
                Some(("path-length", usize_to_i128(path.len())?)),
            )?;
            continue;
        }

        let new_price =
            relabel_price(work, tip, cost_multiplier, epsilon, ids, identity, recorder)?;
        current[tip_index] = 0;
        record_trace(
            recorder.as_mut(),
            graph,
            work,
            identity.event(EventKind::Relabel),
            TraceView::search_path(work, root, tip, path.clone()),
            Some(("price", new_price)),
        )?;
        if tip != root {
            let removed = path.pop().ok_or(CostScalingError::Conservation)?;
            tip = work
                .residual
                .arc(&removed)
                .map(|arc| arc.from)
                .ok_or(CostScalingError::MissingOutgoingResidual)?;
            increment_transition(work)?;
            work.metrics.retreats = work
                .metrics
                .retreats
                .checked_add(1)
                .ok_or(CostScalingError::ArithmeticOverflow)?;
            record_trace(
                recorder.as_mut(),
                graph,
                work,
                identity.event(EventKind::Retreat),
                TraceView::search_path(work, root, tip, path.clone()),
                Some(("path-length", usize_to_i128(path.len())?)),
            )?;
        }
    };

    sequential_path_augmentation(
        graph,
        work,
        root,
        tip,
        &path,
        termination,
        queue,
        queued,
        identity,
        recorder,
    )?;
    validate_epsilon_optimality(work, cost_multiplier, epsilon)
}

#[allow(clippy::too_many_arguments)]
fn next_admissible_path_arc(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    root: NodeIndex,
    tip: NodeIndex,
    path: &[ResidualArcId],
    cost_multiplier: i128,
    outgoing: &[ResidualArcId],
    current: &mut usize,
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<Option<ResidualArc>, CostScalingError> {
    while *current < outgoing.len() {
        let id = outgoing[*current].clone();
        increment_scan(work, &id, identity, recorder.as_mut())?;
        let arc = work
            .residual
            .arc(&id)
            .ok_or(CostScalingError::MissingOutgoingResidual)?;
        if arc.capacity > 0 && scaled_reduced_cost(&arc, &work.prices, cost_multiplier)? < 0 {
            return Ok(Some(arc));
        }
        increment_transition(work)?;
        *current = current
            .checked_add(1)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        work.metrics.current_arc_advances = work
            .metrics
            .current_arc_advances
            .checked_add(1)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        record_trace(
            recorder.as_mut(),
            graph,
            work,
            identity.event(EventKind::Advance),
            TraceView::search_path(work, root, tip, path.to_vec()),
            Some(("cursor", usize_to_i128(*current)?)),
        )?;
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn sequential_path_augmentation(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    root: NodeIndex,
    tip: NodeIndex,
    path: &[ResidualArcId],
    termination: PathTermination,
    queue: &mut VecDeque<NodeIndex>,
    queued: &mut [bool],
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    if path.is_empty()
        || (termination == PathTermination::Deficit && work.excess[tip.as_usize()] >= 0)
        || (termination == PathTermination::LengthLimit
            && path.len() != PARTIAL_AUGMENT_RELABEL_MCF_PATH_LENGTH)
    {
        return Err(CostScalingError::Conservation);
    }

    for id in path {
        increment_transition(work)?;
        let arc = work
            .residual
            .arc(id)
            .ok_or(CostScalingError::MissingOutgoingResidual)?;
        if arc.capacity == 0 || work.excess[arc.from.as_usize()] <= 0 {
            return Err(CostScalingError::Conservation);
        }
        let old_head_excess = work.excess[arc.to.as_usize()];
        let amount = positive_excess_capacity(work.excess[arc.from.as_usize()])?.min(arc.capacity);
        let saturating = amount == arc.capacity;
        work.residual
            .augment(std::slice::from_ref(&arc.id), amount)?;
        update_excess(&mut work.excess, arc.from, arc.to, amount)?;
        work.metrics.pushes = work
            .metrics
            .pushes
            .checked_add(1)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        let counter = if saturating {
            &mut work.metrics.saturating_pushes
        } else {
            &mut work.metrics.nonsaturating_pushes
        };
        *counter = counter
            .checked_add(1)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        if old_head_excess <= 0 && work.excess[arc.to.as_usize()] > 0 {
            enqueue_if_active(work, arc.to, queue, queued);
        }
    }

    work.metrics.path_augmentations = work
        .metrics
        .path_augmentations
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    let (event, counter) = match termination {
        PathTermination::Deficit => (
            EventKind::AugmentDeficit,
            &mut work.metrics.deficit_augmentations,
        ),
        PathTermination::LengthLimit => (
            EventKind::AugmentLimit,
            &mut work.metrics.length_limit_augmentations,
        ),
    };
    *counter = counter
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(event),
        TraceView::search_path(work, root, tip, path.to_vec()),
        Some(("arc-pushes", usize_to_i128(path.len())?)),
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn saturate_negative_arcs(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    cost_multiplier: i128,
    epsilon: i128,
    all_arcs: &[ResidualArcId],
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    for id in all_arcs {
        increment_scan(work, id, identity, recorder.as_mut())?;
        if work.fixed_edges.contains(id.original_edge()) {
            increment_fixed_arc_skip(work)?;
            continue;
        }
        let arc = work
            .residual
            .arc(id)
            .ok_or(CostScalingError::MissingOutgoingResidual)?;
        if arc.capacity == 0 || scaled_reduced_cost(&arc, &work.prices, cost_multiplier)? >= 0 {
            continue;
        }
        increment_transition(work)?;
        let amount = arc.capacity;
        work.residual.augment(std::slice::from_ref(id), amount)?;
        update_excess(&mut work.excess, arc.from, arc.to, amount)?;
        work.metrics.initial_saturations = work
            .metrics
            .initial_saturations
            .checked_add(1)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        record_trace(
            recorder.as_mut(),
            graph,
            work,
            identity.event(EventKind::Saturate),
            TraceView::arc(work, id.clone()),
            Some(("delta", i128::from(amount))),
        )?;
    }
    validate_epsilon_optimality(work, cost_multiplier, epsilon)
}

#[allow(clippy::too_many_arguments)]
fn discharge(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    active: NodeIndex,
    cost_multiplier: i128,
    epsilon: i128,
    outgoing: &[Vec<ResidualArcId>],
    current: &mut [usize],
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    let active_index = active.as_usize();
    let ids = outgoing
        .get(active_index)
        .ok_or(CostScalingError::MissingOutgoingResidual)?;
    while work.excess[active_index] > 0 {
        if current[active_index] < ids.len() {
            let id = ids[current[active_index]].clone();
            increment_scan(work, &id, identity, recorder.as_mut())?;
            let arc = work
                .residual
                .arc(&id)
                .ok_or(CostScalingError::MissingOutgoingResidual)?;
            let fixed = work.fixed_edges.contains(id.original_edge());
            if fixed {
                increment_fixed_arc_skip(work)?;
            }
            let admissible = !fixed
                && arc.capacity > 0
                && scaled_reduced_cost(&arc, &work.prices, cost_multiplier)? < 0;
            if admissible {
                push(graph, work, active, arc, identity, recorder)?;
            } else {
                increment_transition(work)?;
                current[active_index] = current[active_index]
                    .checked_add(1)
                    .ok_or(CostScalingError::ArithmeticOverflow)?;
                work.metrics.current_arc_advances = work
                    .metrics
                    .current_arc_advances
                    .checked_add(1)
                    .ok_or(CostScalingError::ArithmeticOverflow)?;
                record_trace(
                    recorder.as_mut(),
                    graph,
                    work,
                    identity.event(EventKind::Advance),
                    TraceView::active(work, active, vec![id]),
                    Some(("cursor", usize_to_i128(current[active_index])?)),
                )?;
            }
        } else {
            relabel(
                graph,
                work,
                active,
                cost_multiplier,
                epsilon,
                ids,
                identity,
                recorder,
            )?;
            current[active_index] = 0;
        }
    }
    Ok(())
}

fn push(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    active: NodeIndex,
    arc: ResidualArc,
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    if arc.from != active || arc.capacity == 0 || work.excess[active.as_usize()] <= 0 {
        return Err(CostScalingError::EpsilonOptimality);
    }
    increment_transition(work)?;
    let amount = positive_excess_capacity(work.excess[active.as_usize()])?.min(arc.capacity);
    let saturating = amount == arc.capacity;
    work.residual
        .augment(std::slice::from_ref(&arc.id), amount)?;
    update_excess(&mut work.excess, arc.from, arc.to, amount)?;
    work.metrics.pushes = work
        .metrics
        .pushes
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    if saturating {
        work.metrics.saturating_pushes = work
            .metrics
            .saturating_pushes
            .checked_add(1)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
    } else {
        work.metrics.nonsaturating_pushes = work
            .metrics
            .nonsaturating_pushes
            .checked_add(1)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
    }
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(EventKind::Push),
        TraceView::active(work, active, vec![arc.id]),
        Some(("delta", i128::from(amount))),
    )
    .map_err(CostScalingError::from)
}

#[allow(clippy::too_many_arguments)]
fn relabel(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    active: NodeIndex,
    cost_multiplier: i128,
    epsilon: i128,
    outgoing: &[ResidualArcId],
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    let new_price = relabel_price(
        work,
        active,
        cost_multiplier,
        epsilon,
        outgoing,
        identity,
        recorder,
    )?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        identity.event(EventKind::Relabel),
        TraceView::active(work, active, Vec::new()),
        Some(("price", new_price)),
    )
    .map_err(CostScalingError::from)
}

fn relabel_price(
    work: &mut WorkingState<'_>,
    active: NodeIndex,
    cost_multiplier: i128,
    epsilon: i128,
    outgoing: &[ResidualArcId],
    identity: TraceIdentity,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<i128, CostScalingError> {
    increment_transition(work)?;
    let old_price = work.prices[active.as_usize()];
    let mut new_price = None;
    for id in outgoing {
        increment_scan(work, id, identity, recorder.as_mut())?;
        if work.fixed_edges.contains(id.original_edge()) {
            increment_fixed_arc_skip(work)?;
            continue;
        }
        let arc = work
            .residual
            .arc(id)
            .ok_or(CostScalingError::MissingOutgoingResidual)?;
        if arc.capacity == 0 {
            continue;
        }
        let candidate = work.prices[arc.to.as_usize()]
            .checked_add(scaled_cost(&arc, cost_multiplier)?)
            .and_then(|value| value.checked_add(epsilon))
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        new_price = Some(new_price.map_or(candidate, |current: i128| current.min(candidate)));
    }
    // A zero-excess DFS tip can be a residual dead end in a graph that is not
    // strongly connected. Raising its price by exactly epsilon makes the
    // incoming admissible path arc ineligible without weakening epsilon
    // optimality; an active root cannot be such a dead end in a feasible
    // refine phase.
    let new_price = match new_price {
        Some(new_price) => new_price,
        None if work.excess[active.as_usize()] == 0 => old_price
            .checked_add(epsilon)
            .ok_or(CostScalingError::ArithmeticOverflow)?,
        None => return Err(CostScalingError::MissingOutgoingResidual),
    };
    if new_price <= old_price {
        return Err(CostScalingError::EpsilonOptimality);
    }
    work.prices[active.as_usize()] = new_price;
    validate_outgoing_epsilon_optimality(work, active, cost_multiplier, epsilon)?;
    work.metrics.relabels = work
        .metrics
        .relabels
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    Ok(new_price)
}

fn validate_admission(graph: &FlowNetwork) -> Result<(), CostScalingError> {
    if graph.nodes().len() > COST_SCALING_MAX_NODES || graph.edges().len() > COST_SCALING_MAX_EDGES
    {
        return Err(CostScalingError::AdmissionLimit);
    }
    Ok(())
}

fn initial_epsilon(graph: &FlowNetwork, cost_multiplier: i128) -> Result<i128, CostScalingError> {
    let maximum = graph.edges().iter().try_fold(0_i128, |maximum, edge| {
        i128::from(edge.cost())
            .checked_abs()
            .and_then(|value| value.checked_mul(cost_multiplier))
            .map(|value| maximum.max(value))
            .ok_or(CostScalingError::ArithmeticOverflow)
    })?;
    let mut epsilon = 1_i128;
    while epsilon < maximum {
        epsilon = epsilon
            .checked_mul(2)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
    }
    Ok(epsilon)
}

fn all_residual_arc_ids(graph: &FlowNetwork) -> Vec<ResidualArcId> {
    let mut ids = graph
        .edges()
        .iter()
        .flat_map(|edge| {
            [ResidualDirection::Forward, ResidualDirection::Reverse]
                .map(|direction| ResidualArcId::new(edge.id().clone(), direction))
        })
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

fn outgoing_residual_arc_ids(graph: &FlowNetwork) -> Vec<Vec<ResidualArcId>> {
    graph
        .node_indices()
        .map(|node| {
            let mut ids = graph
                .outgoing_edges(node)
                .iter()
                .filter_map(|&index| {
                    graph.edge(index).map(|edge| {
                        ResidualArcId::new(edge.id().clone(), ResidualDirection::Forward)
                    })
                })
                .chain(graph.incoming_edges(node).iter().filter_map(|&index| {
                    graph.edge(index).map(|edge| {
                        ResidualArcId::new(edge.id().clone(), ResidualDirection::Reverse)
                    })
                }))
                .collect::<Vec<_>>();
            ids.sort_unstable();
            ids
        })
        .collect()
}

fn smallest_active(graph: &FlowNetwork, excess: &[i128]) -> Option<NodeIndex> {
    graph
        .node_indices()
        .find(|node| excess[node.as_usize()] > 0)
}

fn scaled_cost(arc: &ResidualArc, cost_multiplier: i128) -> Result<i128, CostScalingError> {
    arc.cost
        .checked_mul(cost_multiplier)
        .ok_or(CostScalingError::ArithmeticOverflow)
}

fn scaled_edge_reduced_cost(
    edge: &crate::model::FlowEdge,
    prices: &[i128],
    cost_multiplier: i128,
) -> Result<i128, CostScalingError> {
    i128::from(edge.cost())
        .checked_mul(cost_multiplier)
        .and_then(|value| value.checked_sub(prices[edge.from().as_usize()]))
        .and_then(|value| value.checked_add(prices[edge.to().as_usize()]))
        .ok_or(CostScalingError::ArithmeticOverflow)
}

fn speculative_fixing_threshold(
    node_count: usize,
    cost_multiplier: i128,
) -> Result<i128, CostScalingError> {
    let n = u128::try_from(node_count).map_err(|_| CostScalingError::ArithmeticOverflow)?;
    let multiplier =
        u128::try_from(cost_multiplier).map_err(|_| CostScalingError::ArithmeticOverflow)?;
    let weighted = ARC_FIXING_BETA_NUMERATOR
        .checked_mul(multiplier)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    let right = checked_pow_u128(weighted, 4)?
        .checked_mul(checked_pow_u128(n, 3)?)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    let mut low = 0_u128;
    let mut high = n
        .checked_mul(2)
        .and_then(|value| value.checked_mul(multiplier))
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    while low < high {
        let middle = low + (high - low) / 2;
        let scaled = ARC_FIXING_BETA_DENOMINATOR
            .checked_mul(middle)
            .ok_or(CostScalingError::ArithmeticOverflow)?;
        if checked_pow_u128(scaled, 4)? >= right {
            high = middle;
        } else {
            low = middle
                .checked_add(1)
                .ok_or(CostScalingError::ArithmeticOverflow)?;
        }
    }
    i128::try_from(low).map_err(|_| CostScalingError::ArithmeticOverflow)
}

fn checked_pow_u128(mut base: u128, mut exponent: u32) -> Result<u128, CostScalingError> {
    let mut result = 1_u128;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result = result
                .checked_mul(base)
                .ok_or(CostScalingError::ArithmeticOverflow)?;
        }
        exponent >>= 1;
        if exponent > 0 {
            base = base
                .checked_mul(base)
                .ok_or(CostScalingError::ArithmeticOverflow)?;
        }
    }
    Ok(result)
}

fn scaled_reduced_cost(
    arc: &ResidualArc,
    prices: &[i128],
    cost_multiplier: i128,
) -> Result<i128, CostScalingError> {
    scaled_cost(arc, cost_multiplier)?
        .checked_sub(prices[arc.from.as_usize()])
        .and_then(|value| value.checked_add(prices[arc.to.as_usize()]))
        .ok_or(CostScalingError::ArithmeticOverflow)
}

fn update_excess(
    excess: &mut [i128],
    from: NodeIndex,
    to: NodeIndex,
    amount: u64,
) -> Result<(), CostScalingError> {
    let delta = i128::from(amount);
    excess[from.as_usize()] = excess[from.as_usize()]
        .checked_sub(delta)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    excess[to.as_usize()] = excess[to.as_usize()]
        .checked_add(delta)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    Ok(())
}

fn positive_excess_capacity(value: i128) -> Result<u64, CostScalingError> {
    if value <= 0 {
        return Err(CostScalingError::Conservation);
    }
    Ok(u64::try_from(value).unwrap_or(u64::MAX))
}

fn increment_transition(work: &mut WorkingState<'_>) -> Result<(), CostScalingError> {
    work.transitions = work
        .transitions
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    if work.transitions > COST_SCALING_MAX_STATE_TRANSITIONS {
        return Err(CostScalingError::WorkLimit);
    }
    Ok(())
}

fn increment_scan(
    work: &mut WorkingState<'_>,
    arc: &ResidualArcId,
    identity: TraceIdentity,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    increment_inspection(
        work,
        FlowTraceEntityRef::ResidualArc(arc.clone()),
        identity,
        recorder,
    )
}

fn increment_edge_scan(
    work: &mut WorkingState<'_>,
    edge: &EdgeId,
    identity: TraceIdentity,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    increment_inspection(
        work,
        FlowTraceEntityRef::Edge(edge.clone()),
        identity,
        recorder,
    )
}

fn increment_inspection(
    work: &mut WorkingState<'_>,
    entity_ref: FlowTraceEntityRef,
    identity: TraceIdentity,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), CostScalingError> {
    work.metrics.residual_arc_scans = work
        .metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    if work.metrics.residual_arc_scans > COST_SCALING_MAX_RESIDUAL_ARC_SCANS {
        return Err(CostScalingError::WorkLimit);
    }
    if let Some(recorder) = recorder {
        recorder.record_metric_observation(
            identity.inspect_event(),
            FlowTraceMetricId::ResidualArcScans,
            entity_ref,
        )?;
    }
    Ok(())
}

fn increment_fixed_arc_skip(work: &mut WorkingState<'_>) -> Result<(), CostScalingError> {
    work.metrics.fixed_arc_skips = work
        .metrics
        .fixed_arc_skips
        .checked_add(1)
        .ok_or(CostScalingError::ArithmeticOverflow)?;
    Ok(())
}

fn usize_to_i128(value: usize) -> Result<i128, CostScalingError> {
    i128::try_from(value).map_err(|_| CostScalingError::ArithmeticOverflow)
}

fn validate_epsilon_optimality(
    work: &WorkingState<'_>,
    cost_multiplier: i128,
    epsilon: i128,
) -> Result<(), CostScalingError> {
    if epsilon <= 0 {
        return Err(CostScalingError::EpsilonOptimality);
    }
    for node in work.residual.graph().node_indices() {
        validate_outgoing_epsilon_optimality(work, node, cost_multiplier, epsilon)?;
    }
    Ok(())
}

fn validate_outgoing_epsilon_optimality(
    work: &WorkingState<'_>,
    node: NodeIndex,
    cost_multiplier: i128,
    epsilon: i128,
) -> Result<(), CostScalingError> {
    for arc in work.residual.outgoing_arcs(node) {
        if work.fixed_edges.contains(arc.id.original_edge()) {
            continue;
        }
        if scaled_reduced_cost(&arc, &work.prices, cost_multiplier)? < -epsilon {
            return Err(CostScalingError::EpsilonOptimality);
        }
    }
    Ok(())
}

fn validate_nonnegative_reduced_costs(
    work: &WorkingState<'_>,
    cost_multiplier: i128,
) -> Result<(), CostScalingError> {
    for node in work.residual.graph().node_indices() {
        for arc in work.residual.outgoing_arcs(node) {
            if scaled_reduced_cost(&arc, &work.prices, cost_multiplier)? < 0 {
                return Err(CostScalingError::EpsilonOptimality);
            }
        }
    }
    Ok(())
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    work: &WorkingState<'_>,
    enabled: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, FlowTraceError> {
    if !enabled {
        return Ok(None);
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        &work.residual,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        work.excess.clone(),
        trace_metrics(&work.metrics, work.identity),
    )
    .with_fixed_edges(work.fixed_edges.iter().cloned().collect());
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

struct TraceView {
    labels: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    path: Vec<ResidualArcId>,
}

impl TraceView {
    fn prices(work: &WorkingState<'_>) -> Self {
        Self {
            labels: work.prices.iter().copied().map(Some).collect(),
            search_order: Vec::new(),
            path: Vec::new(),
        }
    }

    fn active(work: &WorkingState<'_>, active: NodeIndex, path: Vec<ResidualArcId>) -> Self {
        Self {
            labels: work.prices.iter().copied().map(Some).collect(),
            search_order: vec![active],
            path,
        }
    }

    fn arc(work: &WorkingState<'_>, arc: ResidualArcId) -> Self {
        Self {
            labels: work.prices.iter().copied().map(Some).collect(),
            search_order: Vec::new(),
            path: vec![arc],
        }
    }

    fn search_path(
        work: &WorkingState<'_>,
        root: NodeIndex,
        tip: NodeIndex,
        path: Vec<ResidualArcId>,
    ) -> Self {
        let search_order = if root == tip {
            vec![root]
        } else {
            vec![root, tip]
        };
        Self {
            labels: work.prices.iter().copied().map(Some).collect(),
            search_order,
            path,
        }
    }
}

fn record_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    work: &WorkingState<'_>,
    metadata: FlowTraceEventMetadata,
    view: TraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        &work.residual,
        view.labels,
        view.search_order,
        view.path,
        work.excess.clone(),
        trace_metrics(&work.metrics, work.identity),
    )
    .with_fixed_edges(work.fixed_edges.iter().cloned().collect());
    recorder.record_transition_with_detail(metadata, &snapshot, detail)
}

const fn trace_metrics(metrics: &CostScalingMetrics, identity: TraceIdentity) -> FlowTraceMetrics {
    if identity.uses_arc_fixing() {
        return FlowTraceMetrics {
            bfs_runs: metrics.arc_fixing_passes as u128,
            relaxation_passes: metrics.arcs_unfixed as u128,
            residual_arc_scans: metrics.residual_arc_scans,
            augmentations: metrics.arcs_fixed as u128,
            path_searches: metrics.fix_ins as u128,
            scaling_phases: metrics.refine_phases as u128,
            blocking_flow_phases: metrics.arc_fixing_recoveries as u128,
            relabels: metrics.relabels as u128,
            retreats: metrics.fixed_arc_skips,
            reverse_bfs_runs: metrics.current_arc_advances,
            gap_terminations: metrics.initial_saturations as u128,
            pushes: metrics.pushes as u128,
            saturating_pushes: metrics.saturating_pushes as u128,
            nonsaturating_pushes: metrics.nonsaturating_pushes as u128,
            discharges: metrics.discharges as u128,
            active_vertex_selections: metrics.active_vertex_selections as u128,
        };
    }
    if metrics.price_refinement_attempts > 0 {
        return FlowTraceMetrics {
            bfs_runs: 0,
            relaxation_passes: metrics.price_refinement_rounds as u128,
            residual_arc_scans: metrics.residual_arc_scans,
            augmentations: metrics.price_refinement_successes as u128,
            path_searches: metrics.price_refinement_attempts as u128,
            scaling_phases: metrics.refine_phases as u128,
            blocking_flow_phases: metrics.price_refinement_failures as u128,
            relabels: metrics.relabels as u128,
            retreats: metrics.price_refinement_arc_scans,
            reverse_bfs_runs: metrics.price_refinement_relaxations,
            gap_terminations: metrics.initial_saturations as u128,
            pushes: metrics.pushes as u128,
            saturating_pushes: metrics.saturating_pushes as u128,
            nonsaturating_pushes: metrics.nonsaturating_pushes as u128,
            discharges: metrics.discharges as u128,
            active_vertex_selections: metrics.active_vertex_selections as u128,
        };
    }
    let path_variant = metrics.path_searches > 0;
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: if path_variant {
            metrics.path_augmentations as u128
        } else {
            metrics.initial_saturations as u128
        },
        path_searches: metrics.path_searches as u128,
        scaling_phases: metrics.refine_phases as u128,
        blocking_flow_phases: metrics.deficit_augmentations as u128,
        relabels: metrics.relabels as u128,
        retreats: if path_variant {
            metrics.retreats as u128
        } else {
            metrics.current_arc_advances
        },
        reverse_bfs_runs: metrics.path_advances,
        gap_terminations: metrics.length_limit_augmentations as u128,
        pushes: metrics.pushes as u128,
        saturating_pushes: metrics.saturating_pushes as u128,
        nonsaturating_pushes: metrics.nonsaturating_pushes as u128,
        discharges: if path_variant {
            metrics.path_augmentations as u128
        } else {
            metrics.discharges as u128
        },
        active_vertex_selections: metrics.active_vertex_selections as u128,
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_simple_cycle_canceling;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    fn network(nodes: &[(&str, i64)], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|(id, supply)| FlowNode::new(NodeId::parse(id).expect("valid node"), *supply))
                .collect(),
            edges
                .iter()
                .map(|(id, from, to, lower, capacity, cost)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("valid edge"),
                    from: NodeId::parse(from).expect("valid tail"),
                    to: NodeId::parse(to).expect("valid head"),
                    lower: *lower,
                    capacity: *capacity,
                    cost: *cost,
                })
                .collect(),
        )
        .expect("valid graph")
    }

    #[test]
    fn exploits_disconnected_finite_negative_cycle() {
        let graph = network(
            &[("s", 0), ("t", 0), ("x", 0), ("y", 0)],
            &[
                ("st", "s", "t", 0, 1, 5),
                ("xy", "x", "y", 0, 2, -4),
                ("yx", "y", "x", 0, 2, 1),
            ],
        );
        let target = [1, -1, 0, 0];

        let result = solve_cost_scaling(&graph, &target).expect("cost scaling succeeds");

        assert_eq!(result.certificate.total_cost, -1);
        assert_eq!(result.flows, vec![1, 2, 2]);
        assert!(result.metrics.initial_saturations > 0);
        assert!(result.metrics.pushes > 0);
        assert!(result.metrics.relabels > 0);
    }

    #[test]
    fn respects_lower_bounds_and_exact_supply_balance() {
        let graph = network(
            &[("a", 2), ("b", 0), ("c", -2)],
            &[
                ("ab", "a", "b", 1, 3, -2),
                ("ac", "a", "c", 0, 2, 4),
                ("bc", "b", "c", 1, 3, 1),
            ],
        );
        let target = [2, 0, -2];

        let result = solve_cost_scaling_push_relabel(&graph, &target)
            .expect("cost-scaling push-relabel succeeds");

        assert_eq!(result.flows, vec![2, 0, 2]);
        assert_eq!(result.certificate.total_cost, -2);
        check_min_cost_flow(&graph, &target, &result.flows).expect("independent certificate");

        let reference =
            solve_simple_cycle_canceling(&graph, &target).expect("lower-bound reference succeeds");
        let arc_fixing =
            solve_arc_fixing(&graph, &target).expect("lower-bound Arc Fixing succeeds");
        let arc_trace =
            trace_arc_fixing(&graph, &target).expect("lower-bound Arc Fixing trace succeeds");
        assert_eq!(arc_trace.result, arc_fixing);
        assert_eq!(
            arc_fixing.certificate.total_cost,
            reference.certificate.total_cost
        );
        check_min_cost_flow(&graph, &target, &arc_fixing.flows)
            .expect("lower-bound Arc Fixing certificate");
    }

    #[test]
    fn halves_power_of_two_epsilon_through_exact_final_scale() {
        let graph = network(
            &[("a", 0), ("b", 0), ("c", 0)],
            &[("ab", "a", "b", 0, 2, -5), ("ba", "b", "a", 0, 2, 4)],
        );
        let target = [0, 0, 0];

        let trace = trace_cost_scaling(&graph, &target).expect("trace succeeds");
        let epsilons = trace
            .events
            .iter()
            .filter(|event| event.catalog_id == "cost-scaling.start-refine")
            .filter_map(|event| event.detail.as_ref().map(|detail| detail.value))
            .collect::<Vec<_>>();

        assert_eq!(trace.result.initial_epsilon, 32);
        assert_eq!(epsilons, vec![16, 8, 4, 2, 1]);
        assert_eq!(trace.result.metrics.refine_phases, 5);
        assert_eq!(trace.result.certificate.total_cost, -2);
    }

    #[test]
    fn fast_trace_catalog_variants_and_reverse_replay_agree() {
        let graph = network(
            &[("s", 0), ("a", 0), ("t", 0)],
            &[
                ("sa", "s", "a", 0, 3, 2),
                ("st", "s", "t", 0, 3, 7),
                ("at", "a", "t", 0, 3, -1),
            ],
        );
        let target = [2, 0, -2];
        let fast = solve_cost_scaling(&graph, &target).expect("fast succeeds");
        let (trace, initialization) = crate::feasibility::capture_feasibility_traces(|execution| {
            trace_cost_scaling_preset_with_feasibility(
                &graph,
                &target,
                CostScalingExecutionPreset::CostScaling,
                execution,
            )
        });
        let trace = trace.expect("trace succeeds");
        let (variant, variant_initialization) =
            crate::feasibility::capture_feasibility_traces(|execution| {
                trace_cost_scaling_preset_with_feasibility(
                    &graph,
                    &target,
                    CostScalingExecutionPreset::PushRelabel,
                    execution,
                )
            });
        let variant = variant.expect("variant trace succeeds");

        assert_eq!(fast, trace.result);
        assert_eq!(fast, variant.result);
        assert_eq!(initialization.len(), 1);
        assert_eq!(variant_initialization.len(), 1);
        crate::feasibility::check_captured_feasibility_trace(&initialization[0])
            .expect("cost-scaling feasibility subtrace");
        crate::feasibility::check_captured_feasibility_trace(&variant_initialization[0])
            .expect("push-relabel feasibility subtrace");
        assert_eq!(
            initialization[0]
                .result
                .trace
                .final_snapshot
                .original_flows
                .iter()
                .map(|edge| edge.flow)
                .collect::<Vec<_>>(),
            trace.base_snapshot.flows
        );
        assert_eq!(initialization[0].use_kind, FeasibilityUse::InitialFlow);
        assert!(!initialization[0].result.trace.events.is_empty());
        assert!(
            variant
                .events
                .iter()
                .all(|event| { event.catalog_id.starts_with("cost-scaling-push-relabel.") })
        );

        let mut replay = trace.base_snapshot.clone();
        for event in &trace.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
        }
        assert_eq!(replay, trace.final_snapshot);
        for event in trace.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, trace.base_snapshot);
    }

    #[test]
    fn augment_relabel_variants_are_exact_and_reversible() {
        let graph = network(
            &[("s", 0), ("a", 0), ("t", 0), ("x", 0), ("y", 0)],
            &[
                ("sa", "s", "a", 0, 3, 2),
                ("at", "a", "t", 0, 3, -1),
                ("st", "s", "t", 0, 3, 7),
                ("xy", "x", "y", 0, 2, -4),
                ("yx", "y", "x", 0, 2, 1),
            ],
        );
        let target = [2, 0, -2, 0, 0];
        let expected = solve_simple_cycle_canceling(&graph, &target)
            .expect("cycle-canceling reference succeeds");

        for (prefix, trace) in [
            (
                "augment-relabel.",
                trace_augment_relabel(&graph, &target).expect("augment-relabel succeeds"),
            ),
            (
                "partial-augment-relabel-mcf.",
                trace_partial_augment_relabel_mcf(&graph, &target)
                    .expect("partial augment-relabel succeeds"),
            ),
        ] {
            assert_eq!(
                trace.result.certificate.total_cost,
                expected.certificate.total_cost
            );
            assert!(trace.result.metrics.path_searches > 0);
            assert!(trace.result.metrics.path_augmentations > 0);
            assert!(
                trace
                    .events
                    .iter()
                    .all(|event| event.catalog_id.starts_with(prefix))
            );

            let mut replay = trace.base_snapshot.clone();
            for event in &trace.events {
                apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                    .expect("forward replay");
            }
            assert_eq!(replay, trace.final_snapshot);
            for event in trace.events.iter().rev() {
                apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                    .expect("reverse replay");
            }
            assert_eq!(replay, trace.base_snapshot);
        }
    }

    #[test]
    fn price_refinement_skips_flow_refine_when_prices_alone_suffice() {
        let graph = network(&[("s", 0), ("t", 0)], &[("st", "s", "t", 0, 1, 5)]);
        let target = [1, -1];

        let result = solve_price_refinement(&graph, &target).expect("price refinement succeeds");

        assert_eq!(result.flows, vec![1]);
        assert_eq!(result.certificate.total_cost, 5);
        assert!(result.metrics.price_refinement_attempts > 0);
        assert_eq!(
            result.metrics.price_refinement_successes,
            result.metrics.price_refinement_attempts
        );
        assert_eq!(result.metrics.price_refinement_failures, 0);
        assert_eq!(result.metrics.initial_saturations, 0);
        assert_eq!(result.metrics.pushes, 0);
    }

    #[test]
    fn price_refinement_negative_cycle_rolls_back_then_falls_back_exactly() {
        let graph = network(
            &[("s", 0), ("t", 0), ("x", 0), ("y", 0)],
            &[
                ("st", "s", "t", 0, 1, 5),
                ("xy", "x", "y", 0, 2, -4),
                ("yx", "y", "x", 0, 2, 1),
            ],
        );
        let target = [1, -1, 0, 0];
        let expected = solve_simple_cycle_canceling(&graph, &target)
            .expect("cycle-canceling reference succeeds");

        let result =
            solve_price_refinement(&graph, &target).expect("price-refinement fallback succeeds");

        assert_eq!(result.flows, expected.flows);
        assert_eq!(result.certificate.total_cost, -1);
        assert!(result.metrics.price_refinement_failures > 0);
        assert!(result.metrics.price_refinement_successes > 0);
        assert!(result.metrics.initial_saturations > 0);
        assert!(result.metrics.pushes > 0);
    }

    #[test]
    fn price_refinement_trace_keeps_flow_fixed_and_replays_rollback() {
        let graph = network(
            &[("a", 0), ("b", 0)],
            &[("ab", "a", "b", 0, 2, -4), ("ba", "b", "a", 0, 2, 1)],
        );
        let target = [0, 0];
        let trace = trace_price_refinement(&graph, &target).expect("trace succeeds");
        let mut replay = trace.base_snapshot.clone();
        let mut attempt_flows = None;
        let mut attempt_prices = None;
        let mut saw_success = false;
        let mut saw_failure = false;

        for event in &trace.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
            match event.catalog_id.as_str() {
                "price-refinement.start-potential-only-attempt" => {
                    attempt_flows = Some(replay.flows.clone());
                    attempt_prices = Some(replay.node_labels.clone());
                }
                "price-refinement.relax-price"
                | "price-refinement.complete-relaxation-round"
                | "price-refinement.succeed-without-flow-change" => {
                    assert_eq!(Some(&replay.flows), attempt_flows.as_ref());
                    if event.catalog_id == "price-refinement.succeed-without-flow-change" {
                        assert_eq!(replay.active_path.len(), 1);
                        let witness = &replay.active_path[0];
                        assert!(
                            replay
                                .residual_capacities
                                .iter()
                                .any(|(id, capacity)| id == witness && *capacity > 0)
                        );
                        saw_success = true;
                    }
                }
                "price-refinement.fail-and-rollback-prices" => {
                    assert_eq!(Some(&replay.flows), attempt_flows.as_ref());
                    assert_eq!(Some(&replay.node_labels), attempt_prices.as_ref());
                    saw_failure = true;
                }
                _ => {}
            }
        }
        assert!(saw_success);
        assert!(saw_failure);
        assert!(trace.events.iter().any(|event| {
            event.catalog_id == "price-refinement.complete-refine"
                && event
                    .detail
                    .as_ref()
                    .is_some_and(|detail| detail.label == "epsilon")
        }));
        assert_eq!(replay, trace.final_snapshot);
        for event in trace.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, trace.base_snapshot);
    }

    #[test]
    fn partial_variant_stops_at_four_arcs_while_full_variant_reaches_deficit() {
        let graph = network(
            &[
                ("v0", 0),
                ("v1", 0),
                ("v2", 0),
                ("v3", 0),
                ("v4", 0),
                ("v5", 0),
                ("v6", 0),
            ],
            &[
                ("e01", "v0", "v1", 0, 1, -1),
                ("e12", "v1", "v2", 0, 1, -1),
                ("e23", "v2", "v3", 0, 1, -1),
                ("e34", "v3", "v4", 0, 1, -1),
                ("e45", "v4", "v5", 0, 1, -1),
                ("e56", "v5", "v6", 0, 1, -1),
            ],
        );
        let target = [0; 7];
        let full = trace_augment_relabel(&graph, &target).expect("full variant succeeds");
        let partial =
            trace_partial_augment_relabel_mcf(&graph, &target).expect("partial variant succeeds");

        assert!(
            full.events
                .iter()
                .any(|event| event.catalog_id == "augment-relabel.augment-to-deficit")
        );
        assert_eq!(full.result.metrics.length_limit_augmentations, 0);
        assert!(partial.events.iter().any(|event| {
            event.catalog_id == "partial-augment-relabel-mcf.augment-at-limit"
                && event.detail.as_ref().is_some_and(|detail| {
                    detail.label == "arc-pushes"
                        && detail.value
                            == i128::try_from(PARTIAL_AUGMENT_RELABEL_MCF_PATH_LENGTH)
                                .expect("small bound")
                })
        }));
        assert!(partial.result.metrics.length_limit_augmentations > 0);
        assert_eq!(full.result.certificate.total_cost, 0);
        assert_eq!(partial.result.certificate.total_cost, 0);
    }

    #[test]
    fn augment_relabel_replay_boundaries_preserve_epsilon_optimality() {
        let graph = network(
            &[
                ("v0", 0),
                ("v1", 0),
                ("v2", 0),
                ("v3", 0),
                ("v4", 0),
                ("v5", 0),
                ("v6", 0),
            ],
            &[
                ("e01", "v0", "v1", 0, 1, -1),
                ("e12", "v1", "v2", 0, 1, -1),
                ("e23", "v2", "v3", 0, 1, -1),
                ("e34", "v3", "v4", 0, 1, -1),
                ("e45", "v4", "v5", 0, 1, -1),
                ("e56", "v5", "v6", 0, 1, -1),
            ],
        );
        let target = [0; 7];
        for trace in [
            trace_augment_relabel(&graph, &target).expect("full trace"),
            trace_partial_augment_relabel_mcf(&graph, &target).expect("partial trace"),
        ] {
            let mut epsilon = trace.result.initial_epsilon;
            let mut snapshot = trace.base_snapshot.clone();
            let mut checked = 0;
            for event in &trace.events {
                apply_trace_event(&graph, &mut snapshot, event, FlowTraceDirection::Forward)
                    .expect("forward replay");
                if event.catalog_id.ends_with(".start-refine") {
                    epsilon = event.detail.as_ref().expect("epsilon detail").value;
                }
                if event.catalog_id.ends_with(".relabel-tip")
                    || event.catalog_id.ends_with(".retreat-path")
                    || event.catalog_id.ends_with(".augment-to-deficit")
                    || event.catalog_id.ends_with(".augment-at-limit")
                    || event.catalog_id.ends_with(".complete-refine")
                {
                    let residual =
                        ResidualState::from_flows(&graph, &snapshot.flows).expect("valid state");
                    let prices = snapshot
                        .node_labels
                        .iter()
                        .map(|price| price.expect("price label"))
                        .collect();
                    let work = WorkingState {
                        residual,
                        excess: snapshot.remaining_divergence.clone(),
                        prices,
                        metrics: CostScalingMetrics::default(),
                        transitions: 0,
                        fixed_edges: BTreeSet::new(),
                        identity: TraceIdentity::PartialAugmentRelabel,
                    };
                    validate_epsilon_optimality(&work, 8, epsilon)
                        .expect("epsilon-optimal replay boundary");
                    checked += 1;
                }
            }
            assert!(checked > 0);
        }
    }

    #[test]
    fn sequential_path_augmentation_uses_each_tail_excess_not_one_bottleneck() {
        let graph = network(
            &[("s", 0), ("a", 0), ("b", 0), ("t", 0)],
            &[
                ("e0", "s", "a", 0, 5, -1),
                ("e1", "a", "b", 0, 3, -1),
                ("e2", "b", "t", 0, 2, -1),
            ],
        );
        let mut excess = vec![0; graph.nodes().len()];
        for (node, value) in [("s", -5), ("a", 2), ("b", 1), ("t", 2)] {
            let index = graph
                .node_index(&NodeId::parse(node).expect("valid node"))
                .expect("known node");
            excess[index.as_usize()] = value;
        }
        let mut work = WorkingState {
            residual: ResidualState::from_flows(&graph, &[5, 3, 2]).expect("valid flow"),
            excess,
            prices: vec![0; 4],
            metrics: CostScalingMetrics::default(),
            transitions: 0,
            fixed_edges: BTreeSet::new(),
            identity: TraceIdentity::AugmentRelabel,
        };
        let path = [
            ResidualArcId::new(
                EdgeId::parse("e2").expect("valid edge"),
                ResidualDirection::Reverse,
            ),
            ResidualArcId::new(
                EdgeId::parse("e1").expect("valid edge"),
                ResidualDirection::Reverse,
            ),
            ResidualArcId::new(
                EdgeId::parse("e0").expect("valid edge"),
                ResidualDirection::Reverse,
            ),
        ];
        let mut queue = VecDeque::new();
        let mut queued = vec![false; 4];
        let mut recorder = None;

        sequential_path_augmentation(
            &graph,
            &mut work,
            graph
                .node_index(&NodeId::parse("t").expect("valid node"))
                .expect("known node"),
            graph
                .node_index(&NodeId::parse("s").expect("valid node"))
                .expect("known node"),
            &path,
            PathTermination::Deficit,
            &mut queue,
            &mut queued,
            TraceIdentity::AugmentRelabel,
            &mut recorder,
        )
        .expect("sequential augmentation succeeds");

        assert_eq!(work.residual.flows(), &[0, 0, 0]);
        assert!(work.excess.iter().all(|&value| value == 0));
        assert_eq!(work.metrics.pushes, 3);
        assert_eq!(work.metrics.path_augmentations, 1);
    }

    #[test]
    fn push_and_relabel_trace_boundaries_preserve_epsilon_optimality() {
        let graph = network(
            &[("a", 0), ("b", 0), ("c", 0)],
            &[
                ("ab", "a", "b", 0, 3, -4),
                ("bc", "b", "c", 0, 3, 1),
                ("ca", "c", "a", 0, 3, 1),
            ],
        );
        let target = [0, 0, 0];
        let trace = trace_cost_scaling(&graph, &target).expect("trace succeeds");
        let multiplier = 4;
        let mut epsilon = trace.result.initial_epsilon;
        let mut snapshot = trace.base_snapshot.clone();
        let mut checked_operations = 0;

        for event in &trace.events {
            apply_trace_event(&graph, &mut snapshot, event, FlowTraceDirection::Forward)
                .expect("forward replay");
            if event.catalog_id == "cost-scaling.start-refine" {
                epsilon = event.detail.as_ref().expect("epsilon detail").value;
            }
            if event.catalog_id == "cost-scaling.push"
                || event.catalog_id == "cost-scaling.relabel"
                || event.catalog_id == "cost-scaling.complete-refine"
            {
                let residual =
                    ResidualState::from_flows(&graph, &snapshot.flows).expect("valid state");
                let prices = snapshot
                    .node_labels
                    .iter()
                    .map(|price| price.expect("price label"))
                    .collect();
                let work = WorkingState {
                    residual,
                    excess: snapshot.remaining_divergence.clone(),
                    prices,
                    metrics: CostScalingMetrics::default(),
                    transitions: 0,
                    fixed_edges: BTreeSet::new(),
                    identity: TraceIdentity::CostScaling,
                };
                validate_epsilon_optimality(&work, multiplier, epsilon)
                    .expect("epsilon optimal boundary");
                checked_operations += 1;
            }
        }
        assert!(checked_operations > 0);
    }

    #[test]
    fn speculative_fixing_threshold_uses_exact_published_coefficient() {
        assert_eq!(speculative_fixing_threshold(4, 5), Ok(4));
        assert_eq!(speculative_fixing_threshold(16, 17), Ok(31));

        for (nodes, multiplier) in [(1_usize, 2_i128), (4, 5), (16, 17), (31, 32)] {
            let threshold =
                speculative_fixing_threshold(nodes, multiplier).expect("representable threshold");
            let threshold = u128::try_from(threshold).expect("nonnegative threshold");
            let n = u128::try_from(nodes).expect("small node count");
            let multiplier = u128::try_from(multiplier).expect("positive multiplier");
            let right = checked_pow_u128(ARC_FIXING_BETA_NUMERATOR * multiplier, 4)
                .expect("small fourth power")
                * checked_pow_u128(n, 3).expect("small cube");
            let scaled = ARC_FIXING_BETA_DENOMINATOR * threshold;
            assert!(checked_pow_u128(scaled, 4).expect("small fourth power") >= right);
            if threshold > 0 {
                let prior = ARC_FIXING_BETA_DENOMINATOR * (threshold - 1);
                assert!(checked_pow_u128(prior, 4).expect("small fourth power") < right);
            }
        }
    }

    #[test]
    fn arc_fixing_fix_in_trace_is_exact_reversible_and_bound_safe() {
        let graph = network(
            &[("v0", 0), ("v1", 0), ("v2", 0)],
            &[
                ("e0_1", "v0", "v1", 0, 4, 5),
                ("e0_2", "v0", "v2", 0, 1, -3),
                ("e1_2", "v1", "v2", 0, 1, -5),
                ("e2_0", "v2", "v0", 0, 2, 1),
            ],
        );
        let target = [0, 0, 0];
        let expected = solve_simple_cycle_canceling(&graph, &target)
            .expect("cycle-canceling reference succeeds");
        let fast = solve_arc_fixing(&graph, &target).expect("fast arc fixing succeeds");
        let trace = trace_arc_fixing(&graph, &target).expect("trace arc fixing succeeds");

        assert_eq!(trace.result, fast);
        assert_eq!(fast.certificate.total_cost, expected.certificate.total_cost);
        assert!(fast.metrics.arc_fixing_passes > 0);
        assert!(fast.metrics.arcs_fixed > 0);
        assert!(fast.metrics.arcs_unfixed > 0);
        assert!(fast.metrics.fix_ins > 0);
        assert!(fast.metrics.fixed_arc_skips > 0);
        assert!(
            trace
                .events
                .iter()
                .any(|event| event.catalog_id == "arc-fixing.update-fixed-set")
        );
        assert!(
            trace
                .events
                .iter()
                .any(|event| event.catalog_id == "arc-fixing.unfix-threshold-arcs")
        );
        assert!(
            trace
                .events
                .iter()
                .any(|event| event.catalog_id == "arc-fixing.fix-in")
        );

        let cost_multiplier = 4;
        let mut replay = trace.base_snapshot.clone();
        validate_fixed_snapshot(&graph, &replay, cost_multiplier, false);
        for event in &trace.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
            validate_fixed_snapshot(
                &graph,
                &replay,
                cost_multiplier,
                event.catalog_id == "arc-fixing.update-fixed-set",
            );
        }
        assert_eq!(replay, trace.final_snapshot);
        for event in trace.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, trace.base_snapshot);
    }

    #[test]
    fn restricted_refine_recovery_restores_all_fixed_edges_atomically() {
        let graph = network(
            &[("active", 0), ("deficit", 0)],
            &[("bridge", "active", "deficit", 0, 1, 0)],
        );
        let bridge = EdgeId::parse("bridge").expect("valid edge ID");
        let mut work = WorkingState {
            residual: ResidualState::from_flows(&graph, &[0]).expect("valid state"),
            excess: vec![1, -1],
            prices: vec![0, 0],
            metrics: CostScalingMetrics::default(),
            transitions: 0,
            fixed_edges: BTreeSet::from([bridge.clone()]),
            identity: TraceIdentity::ArcFixing,
        };
        let mut recorder = start_trace_recorder(&graph, &work, true).expect("trace recorder");

        recover_restricted_refine_if_needed(
            &graph,
            &mut work,
            TraceIdentity::ArcFixing,
            &mut recorder,
        )
        .expect("conservative recovery succeeds");

        assert!(work.fixed_edges.is_empty());
        assert_eq!(work.metrics.arcs_unfixed, 1);
        assert_eq!(work.metrics.arc_fixing_recoveries, 1);
        let (base, events, final_snapshot) = recorder.expect("enabled recorder").finish();
        assert_eq!(base.fixed_edges, vec![bridge.clone()]);
        assert!(final_snapshot.fixed_edges.is_empty());
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].catalog_id, "arc-fixing.inspect-residual-arc");
        assert_eq!(events[1].catalog_id, "arc-fixing.recover-fixed-set");
        assert!(
            events[1]
                .entity_refs
                .contains(&crate::trace::FlowTraceEntityRef::Edge(bridge))
        );
    }

    #[test]
    fn restricted_refine_recovery_detects_shared_capacity_bottleneck() {
        let graph = network(
            &[("a", 0), ("b", 0), ("x", 0), ("t", 0)],
            &[
                ("a_x", "a", "x", 0, 1, 0),
                ("b_x", "b", "x", 0, 1, 0),
                ("fixed", "a", "t", 0, 1, 0),
                ("x_t", "x", "t", 0, 1, 0),
            ],
        );
        let fixed = EdgeId::parse("fixed").expect("valid fixed edge ID");
        let mut work = WorkingState {
            residual: ResidualState::from_flows(&graph, &[0, 0, 0, 0]).expect("valid state"),
            excess: vec![1, 1, -2, 0],
            prices: vec![0; 4],
            metrics: CostScalingMetrics::default(),
            transitions: 0,
            fixed_edges: BTreeSet::from([fixed]),
            identity: TraceIdentity::ArcFixing,
        };
        let mut recorder = None;

        recover_restricted_refine_if_needed(
            &graph,
            &mut work,
            TraceIdentity::ArcFixing,
            &mut recorder,
        )
        .expect("capacitated recovery succeeds");

        assert!(work.fixed_edges.is_empty());
        assert_eq!(work.metrics.arc_fixing_recoveries, 1);
    }

    #[test]
    fn fix_in_precedes_threshold_unfix_inside_beta_band() {
        let graph = network(
            &[("v0", 0), ("v1", 0), ("v2", 0)],
            &[("edge", "v0", "v1", 0, 1, 0)],
        );
        let edge = EdgeId::parse("edge").expect("valid edge ID");
        let mut work = WorkingState {
            residual: ResidualState::from_flows(&graph, &[0]).expect("valid state"),
            excess: vec![0; 3],
            prices: vec![2, 0, 0],
            metrics: CostScalingMetrics::default(),
            transitions: 0,
            fixed_edges: BTreeSet::from([edge]),
            identity: TraceIdentity::ArcFixing,
        };
        let mut recorder = start_trace_recorder(&graph, &work, true).expect("trace recorder");

        prepare_fixed_arcs(
            &graph,
            &mut work,
            4,
            TraceIdentity::ArcFixing,
            &mut recorder,
        )
        .expect("fix-in succeeds");

        assert!(work.fixed_edges.is_empty());
        assert_eq!(work.residual.flows(), &[1]);
        assert_eq!(work.excess, vec![-1, 1, 0]);
        assert_eq!(work.metrics.arcs_unfixed, 1);
        assert_eq!(work.metrics.fix_ins, 1);
        let (_, events, _) = recorder.expect("enabled recorder").finish();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].catalog_id, "arc-fixing.inspect-residual-arc");
        assert_eq!(events[1].catalog_id, "arc-fixing.fix-in");
    }

    fn validate_fixed_snapshot(
        graph: &FlowNetwork,
        snapshot: &FlowTraceSnapshot,
        cost_multiplier: i128,
        require_complementary_slackness: bool,
    ) {
        assert!(
            snapshot
                .fixed_edges
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
        if snapshot.fixed_edges.is_empty() {
            return;
        }
        let prices = snapshot
            .node_labels
            .iter()
            .map(|price| price.expect("arc-fixing snapshot carries every price"))
            .collect::<Vec<_>>();
        for edge_id in &snapshot.fixed_edges {
            let edge_index = graph.edge_index(edge_id).expect("known fixed edge");
            let edge = graph.edge(edge_index).expect("known edge");
            let flow = snapshot.flows[edge_index.as_usize()];
            let reduced = scaled_edge_reduced_cost(edge, &prices, cost_multiplier)
                .expect("exact reduced cost");
            assert!(
                flow == edge.lower() || flow == edge.capacity(),
                "fixed edge {} must stay at a capacity bound",
                edge_id.as_str()
            );
            if !require_complementary_slackness {
                continue;
            }
            assert!(
                (flow == edge.lower() && reduced >= 0) || (flow == edge.capacity() && reduced <= 0),
                "fixed edge {} must stay at a complementary-slackness bound",
                edge_id.as_str()
            );
        }
    }

    #[test]
    fn agrees_with_cycle_canceling_on_small_deterministic_instances() {
        let mut seed = 0x9e37_79b9_u64;
        for case in 0..64 {
            let node_count = 2 + usize::try_from(next(&mut seed) % 4).expect("small count");
            let nodes = (0..node_count)
                .map(|index| (format!("v{index}"), 0_i64))
                .collect::<Vec<_>>();
            let mut edges = Vec::new();
            for from in 0..node_count {
                for to in 0..node_count {
                    if from == to || next(&mut seed).is_multiple_of(3) {
                        continue;
                    }
                    let capacity = 1 + next(&mut seed) % 4;
                    let cost = i64::try_from(next(&mut seed) % 11).expect("small cost") - 5;
                    edges.push((
                        format!("e{from}_{to}"),
                        format!("v{from}"),
                        format!("v{to}"),
                        0,
                        capacity,
                        cost,
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
            let graph = network_owned(&nodes, &edges);
            let target = vec![0_i128; node_count];
            let expected = solve_simple_cycle_canceling(&graph, &target)
                .unwrap_or_else(|error| panic!("reference case {case}: {error}"));
            let actual = solve_cost_scaling(&graph, &target)
                .unwrap_or_else(|error| panic!("cost scaling case {case}: {error}"));
            let augment = solve_augment_relabel(&graph, &target)
                .unwrap_or_else(|error| panic!("augment-relabel case {case}: {error}"));
            let partial = solve_partial_augment_relabel_mcf(&graph, &target)
                .unwrap_or_else(|error| panic!("partial augment-relabel case {case}: {error}"));
            let price_refinement = solve_price_refinement(&graph, &target)
                .unwrap_or_else(|error| panic!("price refinement case {case}: {error}"));
            let arc_fixing = solve_arc_fixing(&graph, &target)
                .unwrap_or_else(|error| panic!("arc fixing case {case}: {error}"));
            let generalized = solve_generalized_cost_scaling_push_relabel(&graph, &target)
                .unwrap_or_else(|error| panic!("generalized cost scaling case {case}: {error}"));
            assert_eq!(
                actual.certificate.total_cost, expected.certificate.total_cost,
                "case {case}"
            );
            assert_eq!(
                augment.certificate.total_cost, expected.certificate.total_cost,
                "augment case {case}"
            );
            assert_eq!(
                partial.certificate.total_cost, expected.certificate.total_cost,
                "partial case {case}"
            );
            assert_eq!(
                price_refinement.certificate.total_cost, expected.certificate.total_cost,
                "price-refinement case {case}"
            );
            assert_eq!(
                arc_fixing.certificate.total_cost, expected.certificate.total_cost,
                "arc-fixing case {case}"
            );
            assert_eq!(
                generalized.certificate.total_cost, expected.certificate.total_cost,
                "generalized case {case}"
            );
        }
    }

    #[test]
    fn generalized_framework_binds_push_refine_and_keeps_a_dedicated_trace_identity() {
        let graph = network(
            &[("s", 2), ("m", 0), ("t", -2)],
            &[
                ("sm", "s", "m", 0, 2, -3),
                ("mt", "m", "t", 0, 2, 5),
                ("st", "s", "t", 0, 2, 4),
            ],
        );
        let target = [2, 0, -2];
        let canonical = solve_cost_scaling_push_relabel(&graph, &target)
            .expect("canonical push-refine variant solves");
        let traced = trace_generalized_cost_scaling_push_relabel(&graph, &target)
            .expect("configured generalized variant traces");

        assert_eq!(traced.result.flows, canonical.flows);
        assert_eq!(traced.result.certificate, canonical.certificate);
        assert_eq!(traced.result.metrics, canonical.metrics);
        assert!(
            traced
                .events
                .iter()
                .all(|event| event.catalog_id.starts_with("generalized-cost-scaling."))
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "generalized-cost-scaling.start-refine")
        );

        let mut snapshot = traced.base_snapshot.clone();
        for event in &traced.events {
            crate::trace::apply_trace_event(
                &graph,
                &mut snapshot,
                event,
                crate::trace::FlowTraceDirection::Forward,
            )
            .expect("forward replay");
        }
        assert_eq!(snapshot, traced.final_snapshot);
        for event in traced.events.iter().rev() {
            crate::trace::apply_trace_event(
                &graph,
                &mut snapshot,
                event,
                crate::trace::FlowTraceDirection::Reverse,
            )
            .expect("reverse replay");
        }
        assert_eq!(snapshot, traced.base_snapshot);
    }

    fn network_owned(
        nodes: &[(String, i64)],
        edges: &[(String, String, String, u64, u64, i64)],
    ) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|(id, supply)| FlowNode::new(NodeId::parse(id).expect("valid node"), *supply))
                .collect(),
            edges
                .iter()
                .map(|(id, from, to, lower, capacity, cost)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("valid edge"),
                    from: NodeId::parse(from).expect("valid tail"),
                    to: NodeId::parse(to).expect("valid head"),
                    lower: *lower,
                    capacity: *capacity,
                    cost: *cost,
                })
                .collect(),
        )
        .expect("valid graph")
    }

    fn next(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *seed
    }
}
