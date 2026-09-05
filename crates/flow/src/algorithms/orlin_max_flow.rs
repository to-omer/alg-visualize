//! Bounded explicit realization of Orlin's 2013 maximum-flow construction.
//!
//! This module follows Sections 3--7 of Orlin, "Max Flows in O(nm) Time,
//! or Better".  It keeps the source-defined improvement phases, abundant-arc
//! contractions, critical-node compact network, anti-abundant capacity
//! transfer, three-way dense/compact case split, pseudo-arc lifting, and
//! contraction expansion.  Dynamic transitive closure and dynamic trees are
//! replaced by deterministic explicit searches suitable for small interactive
//! graphs, so this implementation does not claim the paper's end-to-end
//! `O(nm)` bound.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};

/// Conservative node limit for the explicit transitive-closure realization.
pub const ORLIN_MAX_FLOW_MAX_NODES: usize = 48;
/// Conservative edge limit for the explicit compact-network realization.
pub const ORLIN_MAX_FLOW_MAX_EDGES: usize = 192;
/// Maximum improvement phases.
pub const ORLIN_MAX_FLOW_MAX_PHASES: u64 = 4_096;
/// Maximum capacity transfers, logical augmentations, lifts, and repairs.
pub const ORLIN_MAX_FLOW_MAX_TRANSITIONS: u64 = 200_000;
/// Maximum residual/logical arc scans.
pub const ORLIN_MAX_FLOW_MAX_SCANS: u128 = 30_000_000;
/// Maximum semantic trace boundaries.
pub const ORLIN_MAX_FLOW_MAX_TRACE_EVENTS: usize = 100_000;
/// Dense source-time prefix retained for each bounded scan region.
const ORLIN_MAX_FLOW_SCAN_CHECKPOINT_PREFIX: u128 = 16;
/// Maximum number of real arc inspections represented by one later checkpoint.
const ORLIN_MAX_FLOW_SCAN_CHECKPOINT_STRIDE: u128 = 2_048;

/// Source-defined improvement-phase branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrlinMaxPhaseCase {
    /// More than `m^(9/16)` critical nodes: improve the residual network.
    OriginalApproximation,
    /// Between `m^(1/3)` and `m^(9/16)` critical nodes: approximate compact flow.
    CompactApproximation,
    /// Fewer than `m^(1/3)` critical nodes: exact `(delta,gamma)` compact flow.
    CompactExact,
}

/// Semantic boundary in the source construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrlinMaxStage {
    /// Zero flow and the source cut are installed.
    Ready,
    /// A `delta`-improvement phase begins.
    BeginImprovement,
    /// Abundant cycles or external arcs have been contracted.
    ContractAbundant,
    /// A real residual or quotient arc inspection in phase classification was checkpointed.
    InspectClassificationArc,
    /// Residual arcs and critical nodes are classified.
    Classify,
    /// One of the three source cases is selected.
    SelectCase,
    /// A real quotient arc inspection while building the compact network was checkpointed.
    InspectCompactConstructionArc,
    /// Capacity is transferred from an anti-abundant path to a pseudo-arc.
    TransferCapacity,
    /// The residual or compact logical network is materialized.
    BuildSubproblem,
    /// One threshold-residual augmenting path is applied to the subproblem.
    AugmentSubproblem,
    /// A real threshold-residual logical arc inspection checkpoint is published.
    InspectSubproblemArc,
    /// The subproblem has the required residual cut.
    CompleteSubproblem,
    /// A real logical-flow decomposition arc inspection checkpoint is published.
    InspectDecompositionArc,
    /// A real original residual-route inspection checkpoint is published.
    InspectLiftResidualArc,
    /// One positive logical path is lifted to original residual arcs.
    LiftPath,
    /// A contracted component is rebalanced on abundant arcs.
    ExpandContraction,
    /// A real residual arc inspection while expanding a contraction was checkpointed.
    InspectExpansionResidualArc,
    /// A real residual arc inspection in the next-cut search was checkpointed.
    InspectCutResidualArc,
    /// The next source cut and improvement gap are installed.
    UpdateCut,
    /// The independent maximum-flow/minimum-cut certificate passed.
    Optimal,
}

/// Compact-network arc role.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrlinMaxCompactArcKind {
    /// A positive residual arc between retained critical components.
    Original,
    /// A `2 delta` pseudo-arc representing an abundant path.
    AbundantPseudo,
    /// A pseudo-arc created by anti-abundant capacity transfer.
    TransferredPseudo,
}

/// Residual direction plus source-defined classification at one boundary.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMaxResidualArcState {
    /// Stable residual identity.
    pub id: ResidualArcId,
    /// Current residual capacity.
    pub capacity: u64,
    /// Whether this direction is abundant at the current `delta`.
    pub abundant: bool,
    /// Whether this direction is anti-abundant.
    pub anti_abundant: bool,
    /// Whether its endpoint-pair capacity is small.
    pub small: bool,
    /// Whether it meets the current medium-capacity test.
    pub medium: bool,
    /// Exact cumulative scan ordinal when this residual direction is the
    /// source-time inspection cursor.
    pub inspection_serial: Option<u128>,
}

/// Original-node projection of the quotient/compact classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMaxNodeState {
    /// Canonical contracted-component ordinal.
    pub component: usize,
    /// Whether this component is retained in the active compact network.
    pub critical: bool,
    /// Exact anti-abundant potential `incoming - outgoing`.
    pub anti_potential: i128,
    /// Whether the node lies on the current source side.
    pub source_side: bool,
}

/// Public compact-network arc.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMaxCompactArcState {
    /// Stable phase-local ordinal.
    pub ordinal: usize,
    /// Tail contracted component.
    pub from_component: usize,
    /// Head contracted component.
    pub to_component: usize,
    /// Source-defined role.
    pub kind: OrlinMaxCompactArcKind,
    /// Logical capacity.
    pub capacity: u128,
    /// Current logical subproblem flow.
    pub flow: u128,
    /// Expanded witness in original residual identities.
    pub witness: Vec<ResidualArcId>,
    /// Exact cumulative scan ordinal when this logical direction is the
    /// source-time inspection cursor.
    pub inspection_serial: Option<u128>,
}

/// Exact deterministic operation counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OrlinMaxMetrics {
    /// Improvement phases entered.
    pub improvement_phases: u64,
    /// Abundant residual directions observed at phase starts.
    pub abundant_arc_observations: u64,
    /// Node merges caused by abundant cycles/external arcs.
    pub contractions: u64,
    /// Critical components observed across phase classifications.
    pub critical_node_observations: u64,
    /// Compact networks materialized.
    pub compact_networks: u64,
    /// Anti-abundant path-capacity transfers.
    pub capacity_transfers: u64,
    /// Units reserved by capacity transfer.
    pub transferred_units: u128,
    /// Abundant and transferred pseudo-arcs materialized.
    pub pseudo_arcs: u64,
    /// Approximate logical flow subproblems.
    pub approximate_subproblems: u64,
    /// Exact logical flow subproblems.
    pub exact_subproblems: u64,
    /// Threshold-residual logical augmentations.
    pub subproblem_augmentations: u64,
    /// Positive logical paths lifted to original arcs.
    pub lifted_paths: u64,
    /// Abundant internal repair paths used during expansion.
    pub expansion_paths: u64,
    /// Installed residual cuts after improvement phases.
    pub cut_updates: u64,
    /// Residual/logical arcs inspected.
    pub residual_arc_scans: u128,
}

/// Complete public state at one reversible boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMaxSnapshot {
    /// Semantic stage.
    pub stage: OrlinMaxStage,
    /// Integral residual cut bound at the start/current end of the phase.
    pub delta: u128,
    /// Exact gamma numerator for the small-critical case.
    pub gamma_numerator: u128,
    /// Exact positive gamma denominator.
    pub gamma_denominator: u128,
    /// Selected phase case, when known.
    pub phase_case: Option<OrlinMaxPhaseCase>,
    /// Original-node quotient state.
    pub nodes: Vec<OrlinMaxNodeState>,
    /// Both directions of every original edge.
    pub residual_arcs: Vec<OrlinMaxResidualArcState>,
    /// Materialized logical subproblem.
    pub compact_arcs: Vec<OrlinMaxCompactArcState>,
    /// Active logical residual path as `(arc ordinal, reverse)`.
    pub active_compact_path: Vec<(usize, bool)>,
    /// Active expanded original residual directions.
    pub active_original_path: Vec<ResidualArcId>,
    /// Current threshold used by the bounded subproblem solver.
    pub threshold: u128,
    /// Certified original-edge flow vector at the terminal boundary.
    pub certified_flows: Option<Vec<u64>>,
    /// Exact operation counters.
    pub metrics: OrlinMaxMetrics,
}

/// One reversible source transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMaxTraceEvent {
    /// Stable event-catalog identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: OrlinMaxSnapshot,
    /// State after the transition.
    pub after: OrlinMaxSnapshot,
}

/// Certified exact maximum-flow result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMaxResult {
    /// Original-edge flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Independent maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact bounded-kernel counters.
    pub metrics: OrlinMaxMetrics,
    /// Fast-profile terminal state.
    pub final_snapshot: OrlinMaxSnapshot,
}

/// Exact result with all source boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMaxTraceResult {
    /// Same result returned by the fast profile.
    pub result: OrlinMaxResult,
    /// Ready boundary.
    pub base_snapshot: OrlinMaxSnapshot,
    /// Complete deterministic event sequence.
    pub events: Vec<OrlinMaxTraceEvent>,
    /// Independently certified terminal boundary.
    pub final_snapshot: OrlinMaxSnapshot,
}

/// Construction, replay, or certification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum OrlinMaxError {
    /// Input exceeds the explicit interactive admission band.
    #[error("graph exceeds Orlin max-flow admission limits")]
    AdmissionLimit,
    /// The source-defined zero-feasible max-flow domain was violated.
    #[error("Orlin max flow requires distinct terminals, zero lower bounds, and zero supplies")]
    GraphRequirement,
    /// Original residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Independent maximum-flow/minimum-cut verification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact arithmetic exceeded the bounded implementation domain.
    #[error("Orlin max-flow exact arithmetic overflow")]
    ArithmeticOverflow,
    /// A deterministic scan, transition, phase, or trace ceiling was reached.
    #[error("Orlin max-flow deterministic work limit exceeded")]
    WorkLimit,
    /// Contraction, compaction, lifting, or expansion contradicted its invariant.
    #[error("Orlin max-flow construction invariant failed")]
    Invariant,
    /// A public trace failed deterministic replay or snapshot validation.
    #[error("Orlin max-flow trace verification failed")]
    TraceVerification,
}

/// Solves maximum flow with the bounded explicit Orlin construction.
///
/// # Errors
///
/// Rejects out-of-band/non-source-domain graphs, bounded work exhaustion,
/// construction inconsistency, arithmetic overflow, or failed certification.
pub fn solve_orlin_max_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<OrlinMaxResult, OrlinMaxError> {
    solve_internal(graph, source, sink, false).map(|run| run.result)
}

/// Records every contraction, classification, transfer, subproblem, lift,
/// expansion, and cut-update boundary.
///
/// # Errors
///
/// Returns the same failures as [`solve_orlin_max_flow`].
pub fn trace_orlin_max_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<OrlinMaxTraceResult, OrlinMaxError> {
    let run = solve_internal(graph, source, sink, true)?;
    Ok(OrlinMaxTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Ratio {
    numerator: u128,
    denominator: u128,
}

impl Ratio {
    const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };

    fn new(numerator: u128, denominator: u128) -> Result<Self, OrlinMaxError> {
        if denominator == 0 {
            return Err(OrlinMaxError::ArithmeticOverflow);
        }
        let divisor = gcd(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    fn half(self) -> Result<Self, OrlinMaxError> {
        Self::new(
            self.numerator,
            self.denominator
                .checked_mul(2)
                .ok_or(OrlinMaxError::ArithmeticOverflow)?,
        )
    }

    fn admits(self, value: u128) -> Result<bool, OrlinMaxError> {
        Ok(value
            .checked_mul(self.denominator)
            .ok_or(OrlinMaxError::ArithmeticOverflow)?
            <= self.numerator)
    }
}

fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

#[derive(Clone, Debug)]
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(len: usize) -> Self {
        Self {
            parent: (0..len).collect(),
        }
    }

    fn root(&self, mut node: usize) -> usize {
        while self.parent[node] != node {
            node = self.parent[node];
        }
        node
    }

    fn union(&mut self, left: usize, right: usize) -> bool {
        let left = self.root(left);
        let right = self.root(right);
        if left == right {
            return false;
        }
        let (keep, remove) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        self.parent[remove] = keep;
        true
    }

    fn ordinals(&self) -> Vec<usize> {
        let mut roots = BTreeMap::new();
        for node in 0..self.parent.len() {
            let root = self.root(node);
            let next = roots.len();
            roots.entry(root).or_insert(next);
        }
        (0..self.parent.len())
            .map(|node| roots[&self.root(node)])
            .collect()
    }
}

#[derive(Clone, Debug)]
struct QuotientArc {
    from: usize,
    to: usize,
    capacity: u128,
    routes: Vec<ResidualArcId>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default)]
struct ClassFlags {
    abundant: bool,
    anti_abundant: bool,
    small: bool,
    medium: bool,
}

#[derive(Clone, Debug)]
struct PhaseClassification {
    component_of: Vec<usize>,
    source_component: usize,
    sink_component: usize,
    quotient_arcs: Vec<QuotientArc>,
    flags: Vec<ClassFlags>,
    residual_flags: BTreeMap<ResidualArcId, ClassFlags>,
    critical: Vec<bool>,
    anti_potential: Vec<i128>,
    newly_contracted: u64,
}

#[derive(Clone, Debug)]
struct CompactArc {
    from: usize,
    to: usize,
    capacity: u128,
    kind: OrlinMaxCompactArcKind,
    witness: Vec<usize>,
}

#[derive(Clone, Debug)]
struct TransferRecord {
    witness: Vec<usize>,
    amount: u128,
    metrics: OrlinMaxMetrics,
}

#[derive(Clone, Debug)]
enum CompactBuildBoundary {
    Inspect(OriginalScanCheckpoint),
    Transfer(TransferRecord),
}

#[derive(Clone, Debug)]
struct CompactBuildRun {
    arcs: Vec<CompactArc>,
    boundaries: Vec<CompactBuildBoundary>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LocalResidualId {
    arc: usize,
    reverse: bool,
}

#[derive(Clone, Debug)]
struct LocalAugmentation {
    path: Vec<LocalResidualId>,
    amount: u128,
    threshold: u128,
    flows: Vec<u128>,
    metrics: OrlinMaxMetrics,
}

#[derive(Clone, Debug)]
struct CompactScanCheckpoint {
    inspected: LocalResidualId,
    flows: Vec<u128>,
    threshold: u128,
    metrics: OrlinMaxMetrics,
}

#[derive(Clone, Debug)]
enum LocalBoundary {
    Inspect(CompactScanCheckpoint),
    Augment(LocalAugmentation),
}

#[derive(Clone, Debug)]
struct LocalRun {
    flows: Vec<u128>,
    boundaries: Vec<LocalBoundary>,
    source_side: Vec<bool>,
    cut: u128,
    threshold: u128,
}

#[derive(Clone, Debug)]
struct LiftedPath {
    compact_path: Vec<usize>,
    amount: u128,
}

#[derive(Clone, Debug)]
struct DecompositionRun {
    paths: Vec<LiftedPath>,
    checkpoints: Vec<CompactScanCheckpoint>,
}

#[derive(Clone, Debug)]
struct OriginalScanCheckpoint {
    inspected: ResidualArcId,
    flows: Vec<u64>,
    metrics: OrlinMaxMetrics,
}

#[derive(Clone, Debug)]
struct AppliedQuotientPath {
    active: Vec<ResidualArcId>,
    checkpoints: Vec<OriginalScanCheckpoint>,
}

#[derive(Clone, Debug)]
enum ExpansionBoundary {
    Inspect(OriginalScanCheckpoint),
    Repair {
        path: Vec<ResidualArcId>,
        flows: Vec<u64>,
        metrics: OrlinMaxMetrics,
    },
}

struct CompactScanCollector {
    enabled: bool,
    observed: u128,
    last_published_scan: Option<u128>,
    pending: Option<CompactScanCheckpoint>,
    checkpoints: Vec<CompactScanCheckpoint>,
}

impl CompactScanCollector {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            observed: 0,
            last_published_scan: None,
            pending: None,
            checkpoints: Vec::new(),
        }
    }

    fn observe(
        &mut self,
        inspected: LocalResidualId,
        flows: &[u128],
        threshold: u128,
        metrics: OrlinMaxMetrics,
    ) {
        if !self.enabled {
            return;
        }
        self.observed += 1;
        self.pending = Some(CompactScanCheckpoint {
            inspected,
            flows: flows.to_vec(),
            threshold,
            metrics,
        });
        if should_publish_orlin_scan_checkpoint(self.observed) {
            self.publish_pending();
        }
    }

    fn flush(&mut self) {
        if self.enabled {
            self.publish_pending();
        }
    }

    fn publish_pending(&mut self) {
        let Some(checkpoint) = self.pending.take() else {
            return;
        };
        if self.last_published_scan == Some(checkpoint.metrics.residual_arc_scans) {
            return;
        }
        self.last_published_scan = Some(checkpoint.metrics.residual_arc_scans);
        self.checkpoints.push(checkpoint);
    }
}

struct OriginalScanCollector {
    enabled: bool,
    observed: u128,
    last_published_scan: Option<u128>,
    pending: Option<OriginalScanCheckpoint>,
    checkpoints: Vec<OriginalScanCheckpoint>,
}

impl OriginalScanCollector {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            observed: 0,
            last_published_scan: None,
            pending: None,
            checkpoints: Vec::new(),
        }
    }

    fn observe(&mut self, inspected: &ResidualArcId, flows: &[u64], metrics: OrlinMaxMetrics) {
        if !self.enabled {
            return;
        }
        self.observed += 1;
        self.pending = Some(OriginalScanCheckpoint {
            inspected: inspected.clone(),
            flows: flows.to_vec(),
            metrics,
        });
        if should_publish_orlin_scan_checkpoint(self.observed) {
            self.publish_pending();
        }
    }

    fn finish(mut self) -> Vec<OriginalScanCheckpoint> {
        if self.enabled {
            self.publish_pending();
        }
        self.checkpoints
    }

    fn flush(&mut self) {
        if self.enabled {
            self.publish_pending();
        }
    }

    fn publish_pending(&mut self) {
        let Some(checkpoint) = self.pending.take() else {
            return;
        };
        if self.last_published_scan == Some(checkpoint.metrics.residual_arc_scans) {
            return;
        }
        self.last_published_scan = Some(checkpoint.metrics.residual_arc_scans);
        self.checkpoints.push(checkpoint);
    }
}

const fn should_publish_orlin_scan_checkpoint(observed: u128) -> bool {
    observed <= ORLIN_MAX_FLOW_SCAN_CHECKPOINT_PREFIX
        || observed.is_multiple_of(ORLIN_MAX_FLOW_SCAN_CHECKPOINT_STRIDE)
}

struct InternalRun {
    result: OrlinMaxResult,
    base_snapshot: OrlinMaxSnapshot,
    events: Vec<OrlinMaxTraceEvent>,
    final_snapshot: OrlinMaxSnapshot,
}

struct PublicContext {
    delta: u128,
    gamma: Ratio,
    phase_case: Option<OrlinMaxPhaseCase>,
    cut: Vec<bool>,
    component_of: Vec<usize>,
    critical_by_component: Vec<bool>,
    anti_potential_by_component: Vec<i128>,
    residual_flags: BTreeMap<ResidualArcId, ClassFlags>,
    quotient_arcs: Vec<QuotientArc>,
    compact_arcs: Vec<CompactArc>,
    compact_flows: Vec<u128>,
    active_compact_path: Vec<LocalResidualId>,
    active_original_path: Vec<ResidualArcId>,
    threshold: u128,
    certified_flows: Option<Vec<u64>>,
    metrics: OrlinMaxMetrics,
}

impl PublicContext {
    fn ready(graph: &FlowNetwork, source: NodeIndex, delta: u128) -> Self {
        let mut cut = vec![false; graph.nodes().len()];
        cut[source.as_usize()] = true;
        Self {
            delta,
            gamma: Ratio::ZERO,
            phase_case: None,
            cut,
            component_of: (0..graph.nodes().len()).collect(),
            critical_by_component: vec![false; graph.nodes().len()],
            anti_potential_by_component: vec![0; graph.nodes().len()],
            residual_flags: BTreeMap::new(),
            quotient_arcs: Vec::new(),
            compact_arcs: Vec::new(),
            compact_flows: Vec::new(),
            active_compact_path: Vec::new(),
            active_original_path: Vec::new(),
            threshold: 0,
            certified_flows: None,
            metrics: OrlinMaxMetrics::default(),
        }
    }
}

struct EventRecorder {
    enabled: bool,
    base: OrlinMaxSnapshot,
    cursor: OrlinMaxSnapshot,
    events: Vec<OrlinMaxTraceEvent>,
}

impl EventRecorder {
    fn new(base: OrlinMaxSnapshot, enabled: bool) -> Self {
        Self {
            enabled,
            base: base.clone(),
            cursor: base,
            events: Vec::new(),
        }
    }

    fn emit(
        &mut self,
        catalog_id: &'static str,
        after: OrlinMaxSnapshot,
    ) -> Result<(), OrlinMaxError> {
        if self.enabled {
            if self.events.len() >= ORLIN_MAX_FLOW_MAX_TRACE_EVENTS {
                return Err(OrlinMaxError::WorkLimit);
            }
            self.events.push(OrlinMaxTraceEvent {
                catalog_id,
                before: self.cursor.clone(),
                after: after.clone(),
            });
        }
        self.cursor = after;
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    record_trace: bool,
) -> Result<InternalRun, OrlinMaxError> {
    validate_graph(graph, source, sink)?;
    let mut state = ResidualState::at_lower_bounds(graph);
    let initial_delta = source_cut_capacity(&state, source, sink)?;
    let mut context = PublicContext::ready(graph, source, initial_delta);
    let base_snapshot = public_snapshot(graph, &state, OrlinMaxStage::Ready, &context)?;
    let mut recorder = EventRecorder::new(base_snapshot, record_trace);
    let mut union = UnionFind::new(graph.nodes().len());

    while context.delta > 0 {
        if context.metrics.improvement_phases >= ORLIN_MAX_FLOW_MAX_PHASES {
            return Err(OrlinMaxError::WorkLimit);
        }
        context.metrics.improvement_phases =
            checked_add_u64(context.metrics.improvement_phases, 1)?;
        context.gamma = Ratio::ZERO;
        context.phase_case = None;
        context.compact_arcs.clear();
        context.compact_flows.clear();
        context.quotient_arcs.clear();
        context.active_compact_path.clear();
        context.active_original_path.clear();
        context.threshold = 0;
        emit(
            &mut recorder,
            graph,
            &state,
            OrlinMaxStage::BeginImprovement,
            "orlin-max-flow.begin-improvement",
            &context,
        )?;

        let phase_start_flows = state.flows().to_vec();
        let (mut classification, classification_checkpoints) = classify_phase(
            graph,
            &state,
            source,
            sink,
            context.delta,
            &mut union,
            &mut context.metrics,
            record_trace,
        )?;
        let post_classification_metrics = context.metrics;
        for checkpoint in classification_checkpoints {
            context.metrics = checkpoint.metrics;
            context.active_original_path = vec![checkpoint.inspected];
            let checkpoint_state = ResidualState::from_flows(graph, &checkpoint.flows)?;
            emit(
                &mut recorder,
                graph,
                &checkpoint_state,
                OrlinMaxStage::InspectClassificationArc,
                "orlin-max-flow.inspect-classification-arc",
                &context,
            )?;
        }
        context.metrics = post_classification_metrics;
        context.active_original_path.clear();
        context
            .component_of
            .clone_from(&classification.component_of);
        context
            .critical_by_component
            .clone_from(&classification.critical);
        context
            .anti_potential_by_component
            .clone_from(&classification.anti_potential);
        context
            .residual_flags
            .clone_from(&classification.residual_flags);
        context
            .quotient_arcs
            .clone_from(&classification.quotient_arcs);
        if classification.newly_contracted > 0 {
            context.metrics.contractions = checked_add_u64(
                context.metrics.contractions,
                classification.newly_contracted,
            )?;
            emit(
                &mut recorder,
                graph,
                &state,
                OrlinMaxStage::ContractAbundant,
                "orlin-max-flow.contract-abundant",
                &context,
            )?;
        }
        context.metrics.critical_node_observations = checked_add_u64(
            context.metrics.critical_node_observations,
            count_true(&classification.critical)?,
        )?;
        emit(
            &mut recorder,
            graph,
            &state,
            OrlinMaxStage::Classify,
            "orlin-max-flow.classify",
            &context,
        )?;

        let critical_count = classification
            .critical
            .iter()
            .filter(|&&value| value)
            .count();
        let edge_count = graph.edges().len().max(1);
        let phase_case = select_phase_case(critical_count, edge_count)?;
        context.phase_case = Some(phase_case);
        if phase_case == OrlinMaxPhaseCase::CompactExact {
            let gamma = choose_gamma(
                &classification,
                graph.nodes().len(),
                edge_count,
                context.delta,
            )?;
            context.gamma = gamma;
            classification.critical = critical_for_scale(
                &classification,
                graph.nodes().len(),
                edge_count,
                context.delta,
                gamma,
            )?;
            context
                .critical_by_component
                .clone_from(&classification.critical);
        }
        emit(
            &mut recorder,
            graph,
            &state,
            OrlinMaxStage::SelectCase,
            "orlin-max-flow.select-case",
            &context,
        )?;

        let compact_build = match phase_case {
            OrlinMaxPhaseCase::OriginalApproximation => CompactBuildRun {
                arcs: original_subproblem(&classification),
                boundaries: Vec::new(),
            },
            OrlinMaxPhaseCase::CompactApproximation | OrlinMaxPhaseCase::CompactExact => {
                context.metrics.compact_networks =
                    checked_add_u64(context.metrics.compact_networks, 1)?;
                build_compact_network(
                    &classification,
                    &state,
                    context.delta,
                    &mut context.metrics,
                    record_trace,
                )?
            }
        };
        let post_build_metrics = context.metrics;

        for boundary in compact_build.boundaries {
            match boundary {
                CompactBuildBoundary::Inspect(checkpoint) => {
                    context.metrics = checkpoint.metrics;
                    context.active_original_path = vec![checkpoint.inspected];
                    let checkpoint_state = ResidualState::from_flows(graph, &checkpoint.flows)?;
                    emit(
                        &mut recorder,
                        graph,
                        &checkpoint_state,
                        OrlinMaxStage::InspectCompactConstructionArc,
                        "orlin-max-flow.inspect-compact-construction-arc",
                        &context,
                    )?;
                }
                CompactBuildBoundary::Transfer(transfer) => {
                    context.metrics = transfer.metrics;
                    context.threshold = transfer.amount;
                    context.active_original_path =
                        expand_quotient_witness(&classification.quotient_arcs, &transfer.witness)?;
                    emit(
                        &mut recorder,
                        graph,
                        &state,
                        OrlinMaxStage::TransferCapacity,
                        "orlin-max-flow.transfer-capacity",
                        &context,
                    )?;
                }
            }
        }
        context.metrics = post_build_metrics;
        context.active_original_path.clear();
        context.threshold = 0;
        context.compact_arcs = compact_build.arcs;
        context.compact_flows = vec![0; context.compact_arcs.len()];
        context
            .quotient_arcs
            .clone_from(&classification.quotient_arcs);
        emit(
            &mut recorder,
            graph,
            &state,
            OrlinMaxStage::BuildSubproblem,
            "orlin-max-flow.build-subproblem",
            &context,
        )?;

        let local_target = match phase_case {
            OrlinMaxPhaseCase::OriginalApproximation => {
                context.metrics.approximate_subproblems =
                    checked_add_u64(context.metrics.approximate_subproblems, 1)?;
                Ratio::new(context.delta, checked_mul_u128(4, edge_count as u128)?)?
            }
            OrlinMaxPhaseCase::CompactApproximation => {
                context.metrics.approximate_subproblems =
                    checked_add_u64(context.metrics.approximate_subproblems, 1)?;
                Ratio::new(context.delta, checked_mul_u128(8, edge_count as u128)?)?
            }
            OrlinMaxPhaseCase::CompactExact => {
                context.metrics.exact_subproblems =
                    checked_add_u64(context.metrics.exact_subproblems, 1)?;
                Ratio::ZERO
            }
        };
        let local = solve_local_subproblem(
            &context.compact_arcs,
            classification.source_component,
            classification.sink_component,
            local_target,
            &mut context.metrics,
            record_trace,
        )?;
        let post_local_metrics = context.metrics;
        for boundary in &local.boundaries {
            match boundary {
                LocalBoundary::Inspect(checkpoint) => {
                    context.metrics = checkpoint.metrics;
                    context.compact_flows.clone_from(&checkpoint.flows);
                    context.active_compact_path = vec![checkpoint.inspected];
                    context.threshold = checkpoint.threshold;
                    emit(
                        &mut recorder,
                        graph,
                        &state,
                        OrlinMaxStage::InspectSubproblemArc,
                        "orlin-max-flow.inspect-subproblem-arc",
                        &context,
                    )?;
                }
                LocalBoundary::Augment(augmentation) => {
                    if augmentation.amount == 0 {
                        return Err(OrlinMaxError::Invariant);
                    }
                    context.metrics = augmentation.metrics;
                    context.compact_flows.clone_from(&augmentation.flows);
                    context.active_compact_path.clone_from(&augmentation.path);
                    context.threshold = augmentation.threshold;
                    emit(
                        &mut recorder,
                        graph,
                        &state,
                        OrlinMaxStage::AugmentSubproblem,
                        "orlin-max-flow.augment-subproblem",
                        &context,
                    )?;
                }
            }
        }
        context.metrics = post_local_metrics;
        context.compact_flows.clone_from(&local.flows);
        context.active_compact_path.clear();
        context.threshold = local.threshold;
        if !local.source_side[classification.source_component]
            || local.source_side[classification.sink_component]
            || !local_target.admits(local.cut)?
        {
            return Err(OrlinMaxError::Invariant);
        }
        emit(
            &mut recorder,
            graph,
            &state,
            OrlinMaxStage::CompleteSubproblem,
            "orlin-max-flow.complete-subproblem",
            &context,
        )?;

        let decomposition = decompose_local_flow(
            &context.compact_arcs,
            &local.flows,
            classification.source_component,
            classification.sink_component,
            &mut context.metrics,
            record_trace,
        )?;
        for checkpoint in &decomposition.checkpoints {
            context.metrics = checkpoint.metrics;
            context.compact_flows.clone_from(&checkpoint.flows);
            context.active_compact_path = vec![checkpoint.inspected];
            context.active_original_path.clear();
            context.threshold = checkpoint.threshold;
            emit(
                &mut recorder,
                graph,
                &state,
                OrlinMaxStage::InspectDecompositionArc,
                "orlin-max-flow.inspect-decomposition-arc",
                &context,
            )?;
        }
        let logical_value = decomposition.paths.iter().try_fold(0_u128, |sum, path| {
            sum.checked_add(path.amount)
                .ok_or(OrlinMaxError::ArithmeticOverflow)
        })?;
        if logical_value > context.delta {
            return Err(OrlinMaxError::Invariant);
        }
        for lifted in &decomposition.paths {
            let witness = lifted
                .compact_path
                .iter()
                .flat_map(|&ordinal| context.compact_arcs[ordinal].witness.iter().copied())
                .collect::<Vec<_>>();
            context.active_compact_path = lifted
                .compact_path
                .iter()
                .map(|&arc| LocalResidualId {
                    arc,
                    reverse: false,
                })
                .collect();
            let applied = apply_quotient_path(
                &mut state,
                &classification.quotient_arcs,
                &witness,
                lifted.amount,
                &mut context.metrics,
                record_trace,
            )?;
            for checkpoint in &applied.checkpoints {
                context.metrics = checkpoint.metrics;
                context.active_original_path = vec![checkpoint.inspected.clone()];
                let checkpoint_state = ResidualState::from_flows(graph, &checkpoint.flows)?;
                emit(
                    &mut recorder,
                    graph,
                    &checkpoint_state,
                    OrlinMaxStage::InspectLiftResidualArc,
                    "orlin-max-flow.inspect-lift-residual-arc",
                    &context,
                )?;
            }
            context.active_original_path = applied.active;
            context.metrics.lifted_paths = checked_add_u64(context.metrics.lifted_paths, 1)?;
            emit(
                &mut recorder,
                graph,
                &state,
                OrlinMaxStage::LiftPath,
                "orlin-max-flow.lift-path",
                &context,
            )?;
        }
        context.active_compact_path.clear();
        context.active_original_path.clear();

        let repairs = expand_contractions(
            graph,
            &mut state,
            source,
            sink,
            &classification.component_of,
            &phase_start_flows,
            context.delta,
            &mut context.metrics,
            record_trace,
        )?;
        for boundary in repairs {
            match boundary {
                ExpansionBoundary::Inspect(checkpoint) => {
                    context.metrics = checkpoint.metrics;
                    context.active_original_path = vec![checkpoint.inspected];
                    let checkpoint_state = ResidualState::from_flows(graph, &checkpoint.flows)?;
                    emit(
                        &mut recorder,
                        graph,
                        &checkpoint_state,
                        OrlinMaxStage::InspectExpansionResidualArc,
                        "orlin-max-flow.inspect-expansion-residual-arc",
                        &context,
                    )?;
                }
                ExpansionBoundary::Repair {
                    path,
                    flows,
                    metrics,
                } => {
                    context.metrics = metrics;
                    context.active_original_path = path;
                    let checkpoint_state = ResidualState::from_flows(graph, &flows)?;
                    emit(
                        &mut recorder,
                        graph,
                        &checkpoint_state,
                        OrlinMaxStage::ExpandContraction,
                        "orlin-max-flow.expand-contraction",
                        &context,
                    )?;
                }
            }
        }
        context.active_original_path.clear();

        let final_target = match phase_case {
            OrlinMaxPhaseCase::OriginalApproximation | OrlinMaxPhaseCase::CompactApproximation => {
                Ratio::new(context.delta, checked_mul_u128(4, edge_count as u128)?)?
            }
            OrlinMaxPhaseCase::CompactExact => context.gamma,
        };
        let (next_cut, next_delta, cut_checkpoints) = find_cut_with_bound(
            &state,
            source,
            sink,
            final_target,
            &mut context.metrics,
            record_trace,
        )?;
        let post_cut_metrics = context.metrics;
        for checkpoint in cut_checkpoints {
            context.metrics = checkpoint.metrics;
            context.active_original_path = vec![checkpoint.inspected];
            let checkpoint_state = ResidualState::from_flows(graph, &checkpoint.flows)?;
            emit(
                &mut recorder,
                graph,
                &checkpoint_state,
                OrlinMaxStage::InspectCutResidualArc,
                "orlin-max-flow.inspect-cut-residual-arc",
                &context,
            )?;
        }
        context.metrics = post_cut_metrics;
        context.active_original_path.clear();
        if next_delta >= context.delta && next_delta != 0 {
            return Err(OrlinMaxError::Invariant);
        }
        context.cut = next_cut;
        context.delta = next_delta;
        context.residual_flags.clear();
        context.critical_by_component.fill(false);
        context.anti_potential_by_component.fill(0);
        context.compact_arcs.clear();
        context.compact_flows.clear();
        context.metrics.cut_updates = checked_add_u64(context.metrics.cut_updates, 1)?;
        context.active_compact_path.clear();
        context.active_original_path.clear();
        emit(
            &mut recorder,
            graph,
            &state,
            OrlinMaxStage::UpdateCut,
            "orlin-max-flow.update-cut",
            &context,
        )?;
    }

    let flows = state.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    context.certified_flows = Some(flows.clone());
    context.phase_case = None;
    context.compact_arcs.clear();
    context.compact_flows.clear();
    context.active_compact_path.clear();
    context.active_original_path.clear();
    context.threshold = 0;
    emit(
        &mut recorder,
        graph,
        &state,
        OrlinMaxStage::Optimal,
        "orlin-max-flow.optimal",
        &context,
    )?;
    let final_snapshot = recorder.cursor.clone();
    let result = OrlinMaxResult {
        flows,
        certificate,
        metrics: context.metrics,
        final_snapshot: final_snapshot.clone(),
    };
    Ok(InternalRun {
        result,
        base_snapshot: recorder.base,
        events: recorder.events,
        final_snapshot,
    })
}

fn validate_graph(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), OrlinMaxError> {
    if graph.nodes().len() > ORLIN_MAX_FLOW_MAX_NODES
        || graph.edges().len() > ORLIN_MAX_FLOW_MAX_EDGES
    {
        return Err(OrlinMaxError::AdmissionLimit);
    }
    if source == sink
        || graph.nodes().iter().any(|node| node.supply() != 0)
        || graph.edges().iter().any(|edge| edge.lower() != 0)
    {
        return Err(OrlinMaxError::GraphRequirement);
    }
    Ok(())
}

fn emit(
    recorder: &mut EventRecorder,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    boundary: OrlinMaxStage,
    catalog_id: &'static str,
    context: &PublicContext,
) -> Result<(), OrlinMaxError> {
    recorder.emit(
        catalog_id,
        public_snapshot(graph, state, boundary, context)?,
    )
}

fn public_snapshot(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    boundary: OrlinMaxStage,
    context: &PublicContext,
) -> Result<OrlinMaxSnapshot, OrlinMaxError> {
    let nodes = (0..graph.nodes().len())
        .map(|node| {
            let component = *context
                .component_of
                .get(node)
                .ok_or(OrlinMaxError::Invariant)?;
            Ok(OrlinMaxNodeState {
                component,
                critical: context
                    .critical_by_component
                    .get(component)
                    .copied()
                    .unwrap_or(false),
                anti_potential: context
                    .anti_potential_by_component
                    .get(component)
                    .copied()
                    .unwrap_or(0),
                source_side: context.cut.get(node).copied().unwrap_or(false),
            })
        })
        .collect::<Result<Vec<_>, OrlinMaxError>>()?;
    let mut residual_arcs = Vec::with_capacity(graph.edges().len() * 2);
    for edge in graph.edges() {
        for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
            let id = ResidualArcId::new(edge.id().clone(), direction);
            let arc = state.arc(&id).ok_or(OrlinMaxError::Invariant)?;
            let flags = context.residual_flags.get(&id).copied().unwrap_or_default();
            let inspection_serial = (matches!(
                boundary,
                OrlinMaxStage::InspectClassificationArc
                    | OrlinMaxStage::InspectCompactConstructionArc
                    | OrlinMaxStage::InspectLiftResidualArc
                    | OrlinMaxStage::InspectExpansionResidualArc
                    | OrlinMaxStage::InspectCutResidualArc
            ) && context
                .active_original_path
                .iter()
                .any(|active| active == &id))
            .then_some(context.metrics.residual_arc_scans);
            residual_arcs.push(OrlinMaxResidualArcState {
                id,
                capacity: arc.capacity,
                abundant: flags.abundant,
                anti_abundant: flags.anti_abundant,
                small: flags.small,
                medium: flags.medium,
                inspection_serial,
            });
        }
    }
    let compact_arcs = context
        .compact_arcs
        .iter()
        .enumerate()
        .map(|(ordinal, arc)| {
            Ok(OrlinMaxCompactArcState {
                ordinal,
                from_component: arc.from,
                to_component: arc.to,
                kind: arc.kind,
                capacity: arc.capacity,
                flow: context.compact_flows.get(ordinal).copied().unwrap_or(0),
                witness: expand_quotient_witness(&context.quotient_arcs, &arc.witness)?,
                inspection_serial: (matches!(
                    boundary,
                    OrlinMaxStage::InspectSubproblemArc | OrlinMaxStage::InspectDecompositionArc
                ) && context
                    .active_compact_path
                    .iter()
                    .any(|active| active.arc == ordinal))
                .then_some(context.metrics.residual_arc_scans),
            })
        })
        .collect::<Result<Vec<_>, OrlinMaxError>>()?;
    Ok(OrlinMaxSnapshot {
        stage: boundary,
        delta: context.delta,
        gamma_numerator: context.gamma.numerator,
        gamma_denominator: context.gamma.denominator,
        phase_case: context.phase_case,
        nodes,
        residual_arcs,
        compact_arcs,
        active_compact_path: context
            .active_compact_path
            .iter()
            .map(|id| (id.arc, id.reverse))
            .collect(),
        active_original_path: context.active_original_path.clone(),
        threshold: context.threshold,
        certified_flows: context.certified_flows.clone(),
        metrics: context.metrics,
    })
}

fn source_cut_capacity(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<u128, OrlinMaxError> {
    let mut capacity = 0_u128;
    for arc in state.outgoing_arcs(source) {
        if arc.to == source || arc.from == sink || arc.from == arc.to {
            continue;
        }
        capacity = capacity
            .checked_add(u128::from(arc.capacity))
            .ok_or(OrlinMaxError::ArithmeticOverflow)?;
    }
    Ok(capacity)
}

fn checked_add_u64(left: u64, right: u64) -> Result<u64, OrlinMaxError> {
    left.checked_add(right)
        .ok_or(OrlinMaxError::ArithmeticOverflow)
}

fn checked_mul_u128(left: u128, right: u128) -> Result<u128, OrlinMaxError> {
    left.checked_mul(right)
        .ok_or(OrlinMaxError::ArithmeticOverflow)
}

fn count_true(values: &[bool]) -> Result<u64, OrlinMaxError> {
    u64::try_from(values.iter().filter(|&&value| value).count())
        .map_err(|_| OrlinMaxError::ArithmeticOverflow)
}

fn charge_scan(metrics: &mut OrlinMaxMetrics) -> Result<(), OrlinMaxError> {
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(OrlinMaxError::ArithmeticOverflow)?;
    if metrics.residual_arc_scans > ORLIN_MAX_FLOW_MAX_SCANS {
        return Err(OrlinMaxError::WorkLimit);
    }
    Ok(())
}

fn charge_transition(current: u64) -> Result<u64, OrlinMaxError> {
    let next = checked_add_u64(current, 1)?;
    if next > ORLIN_MAX_FLOW_MAX_TRANSITIONS {
        return Err(OrlinMaxError::WorkLimit);
    }
    Ok(next)
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "the phase classifier keeps the exact source kernel and its trace collector in one checked boundary"
)]
fn classify_phase(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    delta: u128,
    union: &mut UnionFind,
    metrics: &mut OrlinMaxMetrics,
    record_trace: bool,
) -> Result<(PhaseClassification, Vec<OriginalScanCheckpoint>), OrlinMaxError> {
    let mut collector = OriginalScanCollector::new(record_trace);
    let (newly_contracted, physical_abundant) = update_contractions(
        graph,
        state,
        source,
        sink,
        delta,
        union,
        metrics,
        &mut collector,
    )?;
    let component_of = union.ordinals();
    let component_count = component_of
        .iter()
        .copied()
        .max()
        .map_or(0, |maximum| maximum + 1);
    let source_component = component_of[source.as_usize()];
    let sink_component = component_of[sink.as_usize()];
    if source_component == sink_component {
        return Err(OrlinMaxError::Invariant);
    }
    let quotient_arcs =
        build_quotient_arcs(state, source, sink, &component_of, metrics, &mut collector)?;
    let capacities = quotient_arcs
        .iter()
        .map(|arc| ((arc.from, arc.to), arc.capacity))
        .collect::<BTreeMap<_, _>>();
    let factor = checked_mul_u128(
        64,
        checked_mul_u128(
            graph.edges().len().max(1) as u128,
            graph.edges().len().max(1) as u128,
        )?,
    )?;
    let mut flags = Vec::with_capacity(quotient_arcs.len());
    let mut residual_flags = BTreeMap::<ResidualArcId, ClassFlags>::new();
    for id in physical_abundant {
        residual_flags.entry(id).or_default().abundant = true;
    }
    let mut anti_potential = vec![0_i128; component_count];
    for arc in &quotient_arcs {
        charge_scan(metrics)?;
        let inspected = arc.routes.first().ok_or(OrlinMaxError::Invariant)?;
        collector.observe(inspected, state.flows(), *metrics);
        let reverse = capacities.get(&(arc.to, arc.from)).copied().unwrap_or(0);
        let pair = arc
            .capacity
            .checked_add(reverse)
            .ok_or(OrlinMaxError::ArithmeticOverflow)?;
        let abundant = arc.capacity >= checked_mul_u128(2, delta)?;
        let external = arc.from == source_component || arc.to == sink_component;
        let anti_abundant = if external {
            !abundant
        } else {
            !abundant && reverse >= checked_mul_u128(2, delta)?
        };
        let small = checked_mul_u128(pair, factor)? < delta;
        let medium =
            checked_mul_u128(arc.capacity, factor)? >= delta && pair < checked_mul_u128(4, delta)?;
        let arc_flags = ClassFlags {
            abundant,
            anti_abundant,
            small,
            medium,
        };
        flags.push(arc_flags);
        for id in &arc.routes {
            let entry = residual_flags.entry(id.clone()).or_default();
            entry.abundant |= abundant;
            entry.anti_abundant |= anti_abundant;
            entry.small |= small;
            entry.medium |= medium;
        }
        if anti_abundant {
            let value =
                i128::try_from(arc.capacity).map_err(|_| OrlinMaxError::ArithmeticOverflow)?;
            anti_potential[arc.to] = anti_potential[arc.to]
                .checked_add(value)
                .ok_or(OrlinMaxError::ArithmeticOverflow)?;
            anti_potential[arc.from] = anti_potential[arc.from]
                .checked_sub(value)
                .ok_or(OrlinMaxError::ArithmeticOverflow)?;
        }
    }
    let provisional = PhaseClassification {
        component_of,
        source_component,
        sink_component,
        quotient_arcs,
        flags,
        residual_flags,
        critical: Vec::new(),
        anti_potential,
        newly_contracted,
    };
    let critical = critical_for_scale(
        &provisional,
        graph.nodes().len(),
        graph.edges().len().max(1),
        delta,
        Ratio::new(delta, 1)?,
    )?;
    let abundant_count = provisional
        .residual_flags
        .values()
        .filter(|flags| flags.abundant)
        .count();
    metrics.abundant_arc_observations = checked_add_u64(
        metrics.abundant_arc_observations,
        u64::try_from(abundant_count).map_err(|_| OrlinMaxError::ArithmeticOverflow)?,
    )?;
    Ok((
        PhaseClassification {
            critical,
            ..provisional
        },
        collector.finish(),
    ))
}

#[allow(
    clippy::too_many_arguments,
    reason = "the contraction scan receives each independently checked phase invariant explicitly"
)]
fn update_contractions(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    delta: u128,
    union: &mut UnionFind,
    metrics: &mut OrlinMaxMetrics,
    collector: &mut OriginalScanCollector,
) -> Result<(u64, BTreeSet<ResidualArcId>), OrlinMaxError> {
    let abundant_threshold = checked_mul_u128(2, delta)?;
    let mut abundant_ids = BTreeSet::new();
    for node in 0..graph.nodes().len() {
        let node_index = NodeIndex::try_from_usize(node).ok_or(OrlinMaxError::Invariant)?;
        for arc in state.outgoing_arcs(node_index) {
            charge_scan(metrics)?;
            collector.observe(&arc.id, state.flows(), *metrics);
            if arc.to == source || arc.from == sink || arc.from == arc.to {
                continue;
            }
            if u128::from(arc.capacity) >= abundant_threshold {
                abundant_ids.insert(arc.id);
            }
        }
    }

    let mut merges = 0_u64;
    loop {
        let ordinals = union.ordinals();
        let component_count = ordinals.iter().copied().max().map_or(0, |max| max + 1);
        let source_component = ordinals[source.as_usize()];
        let sink_component = ordinals[sink.as_usize()];
        let mut reach = vec![vec![false; component_count]; component_count];
        for (node, row) in reach.iter_mut().enumerate() {
            row[node] = true;
        }
        for id in &abundant_ids {
            let arc = state.arc(id).ok_or(OrlinMaxError::Invariant)?;
            let from = ordinals[arc.from.as_usize()];
            let to = ordinals[arc.to.as_usize()];
            reach[from][to] = true;
        }
        for via in 0..component_count {
            let via_reach = reach[via].clone();
            for row in &mut reach {
                if !row[via] {
                    continue;
                }
                for (cell, &via_cell) in row.iter_mut().zip(&via_reach) {
                    *cell |= via_cell;
                }
            }
        }
        let representatives = component_representatives(&ordinals, component_count)?;
        let mut changed = false;
        for left in 0..component_count {
            for right in (left + 1)..component_count {
                if reach[left][right] && reach[right][left] {
                    if (left == source_component && right == sink_component)
                        || (left == sink_component && right == source_component)
                    {
                        return Err(OrlinMaxError::Invariant);
                    }
                    if union.union(representatives[left], representatives[right]) {
                        merges = checked_add_u64(merges, 1)?;
                        changed = true;
                    }
                }
            }
        }
        for id in &abundant_ids {
            let arc = state.arc(id).ok_or(OrlinMaxError::Invariant)?;
            let from = ordinals[arc.from.as_usize()];
            let to = ordinals[arc.to.as_usize()];
            let external = from == source_component || to == sink_component;
            if !external || from == to {
                continue;
            }
            if (from == source_component && to == sink_component)
                || (from == sink_component && to == source_component)
            {
                return Err(OrlinMaxError::Invariant);
            }
            if union.union(representatives[from], representatives[to]) {
                merges = checked_add_u64(merges, 1)?;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok((merges, abundant_ids))
}

fn component_representatives(
    component_of: &[usize],
    component_count: usize,
) -> Result<Vec<usize>, OrlinMaxError> {
    let mut representatives = vec![None; component_count];
    for (node, &component) in component_of.iter().enumerate() {
        if representatives[component].is_none() {
            representatives[component] = Some(node);
        }
    }
    representatives
        .into_iter()
        .map(|node| node.ok_or(OrlinMaxError::Invariant))
        .collect()
}

fn build_quotient_arcs(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    component_of: &[usize],
    metrics: &mut OrlinMaxMetrics,
    collector: &mut OriginalScanCollector,
) -> Result<Vec<QuotientArc>, OrlinMaxError> {
    let mut grouped = BTreeMap::<(usize, usize), (u128, Vec<ResidualArcId>)>::new();
    for node in 0..state.graph().nodes().len() {
        let node_index = NodeIndex::try_from_usize(node).ok_or(OrlinMaxError::Invariant)?;
        for arc in state.outgoing_arcs(node_index) {
            charge_scan(metrics)?;
            collector.observe(&arc.id, state.flows(), *metrics);
            if arc.to == source || arc.from == sink || arc.from == arc.to {
                continue;
            }
            let from = component_of[arc.from.as_usize()];
            let to = component_of[arc.to.as_usize()];
            if from == to {
                continue;
            }
            let entry = grouped.entry((from, to)).or_default();
            entry.0 = entry
                .0
                .checked_add(u128::from(arc.capacity))
                .ok_or(OrlinMaxError::ArithmeticOverflow)?;
            entry.1.push(arc.id);
        }
    }
    Ok(grouped
        .into_iter()
        .map(|((from, to), (capacity, routes))| QuotientArc {
            from,
            to,
            capacity,
            routes,
        })
        .collect())
}

fn critical_for_scale(
    classification: &PhaseClassification,
    original_node_count: usize,
    edge_count: usize,
    delta: u128,
    scale: Ratio,
) -> Result<Vec<bool>, OrlinMaxError> {
    let component_count = classification.anti_potential.len();
    let mut critical = vec![false; component_count];
    critical[classification.source_component] = true;
    critical[classification.sink_component] = true;
    let capacities = classification
        .quotient_arcs
        .iter()
        .map(|arc| ((arc.from, arc.to), arc.capacity))
        .collect::<BTreeMap<_, _>>();
    let medium_factor = checked_mul_u128(
        64,
        checked_mul_u128(edge_count as u128, edge_count as u128)?,
    )?;
    for arc in &classification.quotient_arcs {
        let reverse = capacities.get(&(arc.to, arc.from)).copied().unwrap_or(0);
        let pair = arc
            .capacity
            .checked_add(reverse)
            .ok_or(OrlinMaxError::ArithmeticOverflow)?;
        let lower_left = checked_mul_u128(
            checked_mul_u128(arc.capacity, medium_factor)?,
            scale.denominator,
        )?;
        if lower_left >= scale.numerator && pair < checked_mul_u128(4, delta)? {
            critical[arc.from] = true;
            critical[arc.to] = true;
        }
    }
    let potential_factor = checked_mul_u128(
        16,
        checked_mul_u128(original_node_count as u128, edge_count as u128)?,
    )?;
    for (component, &potential) in classification.anti_potential.iter().enumerate() {
        let magnitude = potential.unsigned_abs();
        let left = checked_mul_u128(
            checked_mul_u128(magnitude, potential_factor)?,
            scale.denominator,
        )?;
        if left >= scale.numerator {
            critical[component] = true;
        }
    }
    Ok(critical)
}

fn select_phase_case(
    critical_count: usize,
    edge_count: usize,
) -> Result<OrlinMaxPhaseCase, OrlinMaxError> {
    let critical = critical_count as u128;
    let edges = edge_count as u128;
    if checked_pow(critical, 16)? > checked_pow(edges, 9)? {
        Ok(OrlinMaxPhaseCase::OriginalApproximation)
    } else if checked_pow(critical, 3)? >= edges {
        Ok(OrlinMaxPhaseCase::CompactApproximation)
    } else {
        Ok(OrlinMaxPhaseCase::CompactExact)
    }
}

fn checked_pow(mut base: u128, mut exponent: u32) -> Result<u128, OrlinMaxError> {
    let mut value = 1_u128;
    while exponent > 0 {
        if exponent & 1 == 1 {
            value = checked_mul_u128(value, base)?;
        }
        exponent >>= 1;
        if exponent > 0 {
            base = checked_mul_u128(base, base)?;
        }
    }
    Ok(value)
}

fn choose_gamma(
    classification: &PhaseClassification,
    original_node_count: usize,
    edge_count: usize,
    delta: u128,
) -> Result<Ratio, OrlinMaxError> {
    let mut gamma = Ratio::new(delta, 1)?;
    for _ in 0..128 {
        let next = gamma.half()?;
        let next_count =
            critical_for_scale(classification, original_node_count, edge_count, delta, next)?
                .into_iter()
                .filter(|value| *value)
                .count() as u128;
        if checked_pow(next_count, 3)? >= checked_mul_u128(8, edge_count as u128)? {
            return Ok(gamma);
        }
        gamma = next;
        if gamma.numerator < gamma.denominator {
            let zero_count = critical_for_scale(
                classification,
                original_node_count,
                edge_count,
                delta,
                Ratio::ZERO,
            )?
            .into_iter()
            .filter(|value| *value)
            .count() as u128;
            if checked_pow(zero_count, 3)? < checked_mul_u128(8, edge_count as u128)? {
                return Ok(Ratio::ZERO);
            }
        }
    }
    Err(OrlinMaxError::WorkLimit)
}

fn original_subproblem(classification: &PhaseClassification) -> Vec<CompactArc> {
    classification
        .quotient_arcs
        .iter()
        .enumerate()
        .map(|(ordinal, arc)| CompactArc {
            from: arc.from,
            to: arc.to,
            capacity: arc.capacity,
            kind: OrlinMaxCompactArcKind::Original,
            witness: vec![ordinal],
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
fn build_compact_network(
    classification: &PhaseClassification,
    state: &ResidualState<'_>,
    delta: u128,
    metrics: &mut OrlinMaxMetrics,
    record_trace: bool,
) -> Result<CompactBuildRun, OrlinMaxError> {
    let mut remaining = classification
        .quotient_arcs
        .iter()
        .map(|arc| arc.capacity)
        .collect::<Vec<_>>();
    let mut transferred = Vec::<CompactArc>::new();
    let mut boundaries = Vec::<CompactBuildBoundary>::new();
    let mut collector = OriginalScanCollector::new(record_trace);
    loop {
        let path = find_transfer_path(classification, state, &remaining, metrics, &mut collector)?;
        collector.flush();
        boundaries.extend(
            std::mem::take(&mut collector.checkpoints)
                .into_iter()
                .map(CompactBuildBoundary::Inspect),
        );
        let Some(path) = path else {
            break;
        };
        if path.len() < 2 {
            return Err(OrlinMaxError::Invariant);
        }
        let amount = path
            .iter()
            .map(|&arc| remaining[arc])
            .min()
            .ok_or(OrlinMaxError::Invariant)?;
        if amount == 0 {
            return Err(OrlinMaxError::Invariant);
        }
        for &arc in &path {
            remaining[arc] -= amount;
        }
        metrics.capacity_transfers = charge_transition(metrics.capacity_transfers)?;
        metrics.transferred_units = metrics
            .transferred_units
            .checked_add(amount)
            .ok_or(OrlinMaxError::ArithmeticOverflow)?;
        let first = classification
            .quotient_arcs
            .get(path[0])
            .ok_or(OrlinMaxError::Invariant)?;
        let last = classification
            .quotient_arcs
            .get(*path.last().ok_or(OrlinMaxError::Invariant)?)
            .ok_or(OrlinMaxError::Invariant)?;
        transferred.push(CompactArc {
            from: first.from,
            to: last.to,
            capacity: amount,
            kind: OrlinMaxCompactArcKind::TransferredPseudo,
            witness: path.clone(),
        });
        boundaries.push(CompactBuildBoundary::Transfer(TransferRecord {
            witness: path,
            amount,
            metrics: *metrics,
        }));
    }

    let mut compact = Vec::<CompactArc>::new();
    let mut endpoint_pairs = BTreeSet::<(usize, usize)>::new();
    for (ordinal, arc) in classification.quotient_arcs.iter().enumerate() {
        if remaining[ordinal] == 0
            || !classification.critical[arc.from]
            || !classification.critical[arc.to]
        {
            continue;
        }
        endpoint_pairs.insert((arc.from, arc.to));
        compact.push(CompactArc {
            from: arc.from,
            to: arc.to,
            capacity: remaining[ordinal],
            kind: OrlinMaxCompactArcKind::Original,
            witness: vec![ordinal],
        });
    }
    for arc in transferred {
        endpoint_pairs.insert((arc.from, arc.to));
        compact.push(arc);
    }

    let critical_components = classification
        .critical
        .iter()
        .enumerate()
        .filter_map(|(component, &critical)| critical.then_some(component))
        .collect::<Vec<_>>();
    for &from in &critical_components {
        for &to in &critical_components {
            if from == to || endpoint_pairs.contains(&(from, to)) {
                continue;
            }
            let path =
                find_abundant_path(classification, state, from, to, metrics, &mut collector)?;
            collector.flush();
            boundaries.extend(
                std::mem::take(&mut collector.checkpoints)
                    .into_iter()
                    .map(CompactBuildBoundary::Inspect),
            );
            if let Some(path) = path {
                compact.push(CompactArc {
                    from,
                    to,
                    capacity: checked_mul_u128(2, delta)?,
                    kind: OrlinMaxCompactArcKind::AbundantPseudo,
                    witness: path,
                });
                endpoint_pairs.insert((from, to));
            }
        }
    }
    compact.sort_by_key(|arc| {
        (
            arc.from,
            arc.to,
            match arc.kind {
                OrlinMaxCompactArcKind::Original => 0_u8,
                OrlinMaxCompactArcKind::TransferredPseudo => 1,
                OrlinMaxCompactArcKind::AbundantPseudo => 2,
            },
            arc.witness.clone(),
        )
    });
    metrics.pseudo_arcs = checked_add_u64(
        metrics.pseudo_arcs,
        u64::try_from(
            compact
                .iter()
                .filter(|arc| arc.kind != OrlinMaxCompactArcKind::Original)
                .count(),
        )
        .map_err(|_| OrlinMaxError::ArithmeticOverflow)?,
    )?;
    Ok(CompactBuildRun {
        arcs: compact,
        boundaries,
    })
}

fn find_transfer_path(
    classification: &PhaseClassification,
    state: &ResidualState<'_>,
    remaining: &[u128],
    metrics: &mut OrlinMaxMetrics,
    collector: &mut OriginalScanCollector,
) -> Result<Option<Vec<usize>>, OrlinMaxError> {
    let component_count = classification.critical.len();
    for source in 0..component_count {
        if !classification.critical[source] {
            continue;
        }
        let mut predecessor = vec![None::<usize>; component_count];
        let mut distance = vec![usize::MAX; component_count];
        let mut queue = VecDeque::from([source]);
        distance[source] = 0;
        while let Some(node) = queue.pop_front() {
            for (ordinal, arc) in classification.quotient_arcs.iter().enumerate() {
                charge_scan(metrics)?;
                let inspected = arc.routes.first().ok_or(OrlinMaxError::Invariant)?;
                collector.observe(inspected, state.flows(), *metrics);
                if arc.from != node
                    || remaining.get(ordinal).copied().unwrap_or(0) == 0
                    || !classification.flags[ordinal].anti_abundant
                {
                    continue;
                }
                if arc.to == source || distance[arc.to] != usize::MAX {
                    continue;
                }
                let next_distance = distance[node]
                    .checked_add(1)
                    .ok_or(OrlinMaxError::ArithmeticOverflow)?;
                distance[arc.to] = next_distance;
                predecessor[arc.to] = Some(ordinal);
                if classification.critical[arc.to] {
                    if next_distance >= 2 {
                        return reconstruct_quotient_path(
                            &classification.quotient_arcs,
                            &predecessor,
                            source,
                            arc.to,
                        )
                        .map(Some);
                    }
                    continue;
                }
                queue.push_back(arc.to);
            }
        }
    }
    Ok(None)
}

fn find_abundant_path(
    classification: &PhaseClassification,
    state: &ResidualState<'_>,
    source: usize,
    sink: usize,
    metrics: &mut OrlinMaxMetrics,
    collector: &mut OriginalScanCollector,
) -> Result<Option<Vec<usize>>, OrlinMaxError> {
    let mut predecessor = vec![None::<usize>; classification.critical.len()];
    let mut seen = vec![false; classification.critical.len()];
    let mut queue = VecDeque::from([source]);
    seen[source] = true;
    while let Some(node) = queue.pop_front() {
        for (ordinal, arc) in classification.quotient_arcs.iter().enumerate() {
            charge_scan(metrics)?;
            let inspected = arc.routes.first().ok_or(OrlinMaxError::Invariant)?;
            collector.observe(inspected, state.flows(), *metrics);
            if arc.from != node || !classification.flags[ordinal].abundant || seen[arc.to] {
                continue;
            }
            if arc.to != sink && classification.critical[arc.to] {
                continue;
            }
            seen[arc.to] = true;
            predecessor[arc.to] = Some(ordinal);
            if arc.to == sink {
                return reconstruct_quotient_path(
                    &classification.quotient_arcs,
                    &predecessor,
                    source,
                    sink,
                )
                .map(Some);
            }
            queue.push_back(arc.to);
        }
    }
    Ok(None)
}

fn reconstruct_quotient_path(
    arcs: &[QuotientArc],
    predecessor: &[Option<usize>],
    source: usize,
    mut sink: usize,
) -> Result<Vec<usize>, OrlinMaxError> {
    let mut reverse = Vec::new();
    while sink != source {
        let ordinal = predecessor
            .get(sink)
            .and_then(|entry| *entry)
            .ok_or(OrlinMaxError::Invariant)?;
        let arc = arcs.get(ordinal).ok_or(OrlinMaxError::Invariant)?;
        if arc.to != sink {
            return Err(OrlinMaxError::Invariant);
        }
        reverse.push(ordinal);
        sink = arc.from;
    }
    reverse.reverse();
    Ok(reverse)
}

fn solve_local_subproblem(
    arcs: &[CompactArc],
    source: usize,
    sink: usize,
    target: Ratio,
    metrics: &mut OrlinMaxMetrics,
    record_trace: bool,
) -> Result<LocalRun, OrlinMaxError> {
    let node_count = arcs
        .iter()
        .flat_map(|arc| [arc.from, arc.to])
        .max()
        .map_or(source.max(sink) + 1, |maximum| maximum + 1)
        .max(source.max(sink) + 1);
    let mut flows = vec![0_u128; arcs.len()];
    let mut boundaries = Vec::new();
    let mut collector = CompactScanCollector::new(record_trace);
    let maximum_capacity = arcs.iter().map(|arc| arc.capacity).max().unwrap_or(0);
    let mut threshold = highest_power_of_two(maximum_capacity).max(1);
    loop {
        let search = local_bfs(
            &LocalBfsQuery {
                arcs,
                flows: &flows,
                node_count,
                source,
                sink,
                threshold,
            },
            metrics,
            &mut collector,
        )?;
        collector.flush();
        boundaries.extend(collector.checkpoints.drain(..).map(LocalBoundary::Inspect));
        if let Some(path) = search.path {
            let amount = path
                .iter()
                .map(|id| local_residual_capacity(arcs, &flows, *id))
                .min()
                .ok_or(OrlinMaxError::Invariant)?;
            if amount < threshold || amount == 0 {
                return Err(OrlinMaxError::Invariant);
            }
            for id in &path {
                if id.reverse {
                    flows[id.arc] -= amount;
                } else {
                    flows[id.arc] = flows[id.arc]
                        .checked_add(amount)
                        .ok_or(OrlinMaxError::ArithmeticOverflow)?;
                }
            }
            metrics.subproblem_augmentations = charge_transition(metrics.subproblem_augmentations)?;
            boundaries.push(LocalBoundary::Augment(LocalAugmentation {
                path,
                amount,
                threshold,
                flows: flows.clone(),
                metrics: *metrics,
            }));
            continue;
        }
        let cut = local_cut_capacity(arcs, &flows, &search.reachable)?;
        if target.admits(cut)? {
            return Ok(LocalRun {
                flows,
                boundaries,
                source_side: search.reachable,
                cut,
                threshold,
            });
        }
        if threshold == 1 {
            return Err(OrlinMaxError::Invariant);
        }
        threshold /= 2;
    }
}

struct LocalSearch {
    path: Option<Vec<LocalResidualId>>,
    reachable: Vec<bool>,
}

struct LocalBfsQuery<'a> {
    arcs: &'a [CompactArc],
    flows: &'a [u128],
    node_count: usize,
    source: usize,
    sink: usize,
    threshold: u128,
}

fn local_bfs(
    query: &LocalBfsQuery<'_>,
    metrics: &mut OrlinMaxMetrics,
    collector: &mut CompactScanCollector,
) -> Result<LocalSearch, OrlinMaxError> {
    let arcs = query.arcs;
    let flows = query.flows;
    let node_count = query.node_count;
    let source = query.source;
    let sink = query.sink;
    let threshold = query.threshold;
    let mut predecessor = vec![None::<LocalResidualId>; node_count];
    let mut reachable = vec![false; node_count];
    let mut queue = VecDeque::from([source]);
    reachable[source] = true;
    while let Some(node) = queue.pop_front() {
        for (ordinal, arc) in arcs.iter().enumerate() {
            for id in [
                LocalResidualId {
                    arc: ordinal,
                    reverse: false,
                },
                LocalResidualId {
                    arc: ordinal,
                    reverse: true,
                },
            ] {
                charge_scan(metrics)?;
                collector.observe(id, flows, threshold, *metrics);
                let (from, to) = if id.reverse {
                    (arc.to, arc.from)
                } else {
                    (arc.from, arc.to)
                };
                if from != node
                    || reachable[to]
                    || local_residual_capacity(arcs, flows, id) < threshold
                {
                    continue;
                }
                reachable[to] = true;
                predecessor[to] = Some(id);
                if to == sink {
                    return Ok(LocalSearch {
                        path: Some(reconstruct_local_path(arcs, &predecessor, source, sink)?),
                        reachable,
                    });
                }
                queue.push_back(to);
            }
        }
    }
    Ok(LocalSearch {
        path: None,
        reachable,
    })
}

fn reconstruct_local_path(
    arcs: &[CompactArc],
    predecessor: &[Option<LocalResidualId>],
    source: usize,
    mut sink: usize,
) -> Result<Vec<LocalResidualId>, OrlinMaxError> {
    let mut reverse = Vec::new();
    while sink != source {
        let id = predecessor
            .get(sink)
            .and_then(|entry| *entry)
            .ok_or(OrlinMaxError::Invariant)?;
        let arc = arcs.get(id.arc).ok_or(OrlinMaxError::Invariant)?;
        let (from, to) = if id.reverse {
            (arc.to, arc.from)
        } else {
            (arc.from, arc.to)
        };
        if to != sink {
            return Err(OrlinMaxError::Invariant);
        }
        reverse.push(id);
        sink = from;
    }
    reverse.reverse();
    Ok(reverse)
}

fn local_residual_capacity(arcs: &[CompactArc], flows: &[u128], id: LocalResidualId) -> u128 {
    if id.reverse {
        flows[id.arc]
    } else {
        arcs[id.arc].capacity - flows[id.arc]
    }
}

fn local_cut_capacity(
    arcs: &[CompactArc],
    flows: &[u128],
    reachable: &[bool],
) -> Result<u128, OrlinMaxError> {
    let mut cut = 0_u128;
    for (ordinal, arc) in arcs.iter().enumerate() {
        if reachable[arc.from] && !reachable[arc.to] {
            cut = cut
                .checked_add(arc.capacity - flows[ordinal])
                .ok_or(OrlinMaxError::ArithmeticOverflow)?;
        }
        if reachable[arc.to] && !reachable[arc.from] {
            cut = cut
                .checked_add(flows[ordinal])
                .ok_or(OrlinMaxError::ArithmeticOverflow)?;
        }
    }
    Ok(cut)
}

fn highest_power_of_two(value: u128) -> u128 {
    if value == 0 {
        0
    } else {
        1_u128 << value.ilog2()
    }
}

fn decompose_local_flow(
    arcs: &[CompactArc],
    flows: &[u128],
    source: usize,
    sink: usize,
    metrics: &mut OrlinMaxMetrics,
    record_trace: bool,
) -> Result<DecompositionRun, OrlinMaxError> {
    let node_count = arcs
        .iter()
        .flat_map(|arc| [arc.from, arc.to])
        .max()
        .map_or(source.max(sink) + 1, |maximum| maximum + 1);
    let mut remaining = flows.to_vec();
    let mut paths = Vec::new();
    let mut collector = CompactScanCollector::new(record_trace);
    loop {
        let mut predecessor = vec![None::<usize>; node_count];
        let mut seen = vec![false; node_count];
        let mut queue = VecDeque::from([source]);
        seen[source] = true;
        while let Some(node) = queue.pop_front() {
            for (ordinal, arc) in arcs.iter().enumerate() {
                charge_scan(metrics)?;
                collector.observe(
                    LocalResidualId {
                        arc: ordinal,
                        reverse: false,
                    },
                    &remaining,
                    0,
                    *metrics,
                );
                if arc.from != node || remaining[ordinal] == 0 || seen[arc.to] {
                    continue;
                }
                seen[arc.to] = true;
                predecessor[arc.to] = Some(ordinal);
                queue.push_back(arc.to);
            }
        }
        if !seen[sink] {
            break;
        }
        let mut path = Vec::new();
        let mut node = sink;
        while node != source {
            let ordinal = predecessor[node].ok_or(OrlinMaxError::Invariant)?;
            path.push(ordinal);
            node = arcs[ordinal].from;
        }
        path.reverse();
        let amount = path
            .iter()
            .map(|&ordinal| remaining[ordinal])
            .min()
            .ok_or(OrlinMaxError::Invariant)?;
        for &ordinal in &path {
            remaining[ordinal] -= amount;
        }
        paths.push(LiftedPath {
            compact_path: path,
            amount,
        });
    }

    let expected_value = local_flow_value(arcs, flows, source)?;
    let decomposed_value = paths.iter().try_fold(0_u128, |sum, path| {
        sum.checked_add(path.amount)
            .ok_or(OrlinMaxError::ArithmeticOverflow)
    })?;
    if expected_value != decomposed_value {
        return Err(OrlinMaxError::Invariant);
    }
    collector.flush();
    Ok(DecompositionRun {
        paths,
        checkpoints: collector.checkpoints,
    })
}

fn local_flow_value(
    arcs: &[CompactArc],
    flows: &[u128],
    source: usize,
) -> Result<u128, OrlinMaxError> {
    let outgoing = arcs
        .iter()
        .zip(flows)
        .filter(|(arc, _)| arc.from == source)
        .try_fold(0_u128, |sum, (_, &flow)| {
            sum.checked_add(flow)
                .ok_or(OrlinMaxError::ArithmeticOverflow)
        })?;
    let incoming = arcs
        .iter()
        .zip(flows)
        .filter(|(arc, _)| arc.to == source)
        .try_fold(0_u128, |sum, (_, &flow)| {
            sum.checked_add(flow)
                .ok_or(OrlinMaxError::ArithmeticOverflow)
        })?;
    outgoing
        .checked_sub(incoming)
        .ok_or(OrlinMaxError::Invariant)
}

fn expand_quotient_witness(
    quotient_arcs: &[QuotientArc],
    witness: &[usize],
) -> Result<Vec<ResidualArcId>, OrlinMaxError> {
    let mut expanded = Vec::new();
    for &ordinal in witness {
        let arc = quotient_arcs.get(ordinal).ok_or(OrlinMaxError::Invariant)?;
        expanded.extend(arc.routes.iter().cloned());
    }
    Ok(expanded)
}

fn apply_quotient_path(
    state: &mut ResidualState<'_>,
    quotient_arcs: &[QuotientArc],
    witness: &[usize],
    amount: u128,
    metrics: &mut OrlinMaxMetrics,
    record_trace: bool,
) -> Result<AppliedQuotientPath, OrlinMaxError> {
    if amount == 0 || witness.is_empty() {
        return Err(OrlinMaxError::Invariant);
    }
    let mut active = Vec::new();
    let mut collector = OriginalScanCollector::new(record_trace);
    let mut previous_to = None;
    for &ordinal in witness {
        let arc = quotient_arcs.get(ordinal).ok_or(OrlinMaxError::Invariant)?;
        if previous_to.is_some_and(|node| node != arc.from) {
            return Err(OrlinMaxError::Invariant);
        }
        previous_to = Some(arc.to);
        let mut remaining = amount;
        for id in &arc.routes {
            charge_scan(metrics)?;
            collector.observe(id, state.flows(), *metrics);
            let capacity = state.arc(id).ok_or(OrlinMaxError::Invariant)?.capacity;
            if capacity == 0 {
                continue;
            }
            let take = remaining.min(u128::from(capacity));
            if take == 0 {
                continue;
            }
            let take = u64::try_from(take).map_err(|_| OrlinMaxError::ArithmeticOverflow)?;
            state.augment(std::slice::from_ref(id), take)?;
            active.push(id.clone());
            remaining -= u128::from(take);
            if remaining == 0 {
                break;
            }
        }
        if remaining != 0 {
            return Err(OrlinMaxError::Invariant);
        }
    }
    Ok(AppliedQuotientPath {
        active,
        checkpoints: collector.finish(),
    })
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "contraction expansion is one atomic source transition with explicit certificate inputs"
)]
fn expand_contractions(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    component_of: &[usize],
    phase_start_flows: &[u64],
    delta: u128,
    metrics: &mut OrlinMaxMetrics,
    record_trace: bool,
) -> Result<Vec<ExpansionBoundary>, OrlinMaxError> {
    let component_count = component_of.iter().copied().max().map_or(0, |max| max + 1);
    let mut divergence = phase_flow_divergence(graph, phase_start_flows, state.flows())?;
    let abundant_threshold = checked_mul_u128(2, delta)?;
    let start_state = ResidualState::from_flows(graph, phase_start_flows)?;
    let mut abundant = Vec::<ResidualArcId>::new();
    let mut collector = OriginalScanCollector::new(record_trace);
    let mut boundaries = Vec::new();
    for node in 0..graph.nodes().len() {
        let index = NodeIndex::try_from_usize(node).ok_or(OrlinMaxError::Invariant)?;
        for arc in start_state.outgoing_arcs(index) {
            charge_scan(metrics)?;
            collector.observe(&arc.id, state.flows(), *metrics);
            if arc.to == source || arc.from == sink || arc.from == arc.to {
                continue;
            }
            if u128::from(arc.capacity) >= abundant_threshold
                && component_of[arc.from.as_usize()] == component_of[arc.to.as_usize()]
            {
                abundant.push(arc.id);
            }
        }
    }
    collector.flush();
    boundaries.extend(
        std::mem::take(&mut collector.checkpoints)
            .into_iter()
            .map(ExpansionBoundary::Inspect),
    );
    abundant.sort_unstable();
    for component in 0..component_count {
        let members = component_of
            .iter()
            .enumerate()
            .filter_map(|(node, &candidate)| (candidate == component).then_some(node))
            .collect::<Vec<_>>();
        if members.len() <= 1 {
            continue;
        }
        let root = if members.contains(&source.as_usize()) {
            source.as_usize()
        } else if members.contains(&sink.as_usize()) {
            sink.as_usize()
        } else {
            members[0]
        };
        for &node in &members {
            if node == root || node == source.as_usize() || node == sink.as_usize() {
                continue;
            }
            let imbalance = divergence[node];
            if imbalance == 0 {
                continue;
            }
            let (from, to, amount) = if imbalance > 0 {
                (
                    root,
                    node,
                    u128::try_from(imbalance).map_err(|_| OrlinMaxError::ArithmeticOverflow)?,
                )
            } else {
                (node, root, imbalance.unsigned_abs())
            };
            let path = find_physical_abundant_path(
                graph,
                state,
                component_of,
                component,
                &abundant,
                from,
                to,
                metrics,
                &mut collector,
            )?
            .ok_or(OrlinMaxError::Invariant)?;
            collector.flush();
            boundaries.extend(
                std::mem::take(&mut collector.checkpoints)
                    .into_iter()
                    .map(ExpansionBoundary::Inspect),
            );
            augment_physical_path(state, &path, amount)?;
            divergence[node] = 0;
            divergence[root] = divergence[root]
                .checked_add(imbalance)
                .ok_or(OrlinMaxError::ArithmeticOverflow)?;
            metrics.expansion_paths = charge_transition(metrics.expansion_paths)?;
            boundaries.push(ExpansionBoundary::Repair {
                path,
                flows: state.flows().to_vec(),
                metrics: *metrics,
            });
        }
        if root != source.as_usize() && root != sink.as_usize() && divergence[root] != 0 {
            return Err(OrlinMaxError::Invariant);
        }
    }
    let final_divergence = phase_flow_divergence(graph, phase_start_flows, state.flows())?;
    if final_divergence
        .iter()
        .enumerate()
        .any(|(node, &value)| node != source.as_usize() && node != sink.as_usize() && value != 0)
    {
        return Err(OrlinMaxError::Invariant);
    }
    Ok(boundaries)
}

fn phase_flow_divergence(
    graph: &FlowNetwork,
    before: &[u64],
    after: &[u64],
) -> Result<Vec<i128>, OrlinMaxError> {
    if before.len() != graph.edges().len() || after.len() != graph.edges().len() {
        return Err(OrlinMaxError::Invariant);
    }
    let mut divergence = vec![0_i128; graph.nodes().len()];
    for ((edge, &before), &after) in graph.edges().iter().zip(before).zip(after) {
        let change = i128::from(after) - i128::from(before);
        divergence[edge.from().as_usize()] = divergence[edge.from().as_usize()]
            .checked_add(change)
            .ok_or(OrlinMaxError::ArithmeticOverflow)?;
        divergence[edge.to().as_usize()] = divergence[edge.to().as_usize()]
            .checked_sub(change)
            .ok_or(OrlinMaxError::ArithmeticOverflow)?;
    }
    Ok(divergence)
}

#[allow(clippy::too_many_arguments)]
fn find_physical_abundant_path(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    component_of: &[usize],
    component: usize,
    abundant: &[ResidualArcId],
    source: usize,
    sink: usize,
    metrics: &mut OrlinMaxMetrics,
    collector: &mut OriginalScanCollector,
) -> Result<Option<Vec<ResidualArcId>>, OrlinMaxError> {
    let mut predecessor = vec![None::<ResidualArcId>; graph.nodes().len()];
    let mut seen = vec![false; graph.nodes().len()];
    let mut queue = VecDeque::from([source]);
    seen[source] = true;
    while let Some(node) = queue.pop_front() {
        for id in abundant {
            charge_scan(metrics)?;
            collector.observe(id, state.flows(), *metrics);
            let arc = state.arc(id).ok_or(OrlinMaxError::Invariant)?;
            if arc.capacity == 0
                || arc.from.as_usize() != node
                || component_of[arc.to.as_usize()] != component
                || seen[arc.to.as_usize()]
            {
                continue;
            }
            seen[arc.to.as_usize()] = true;
            predecessor[arc.to.as_usize()] = Some(id.clone());
            if arc.to.as_usize() == sink {
                let mut path = Vec::new();
                let mut cursor = sink;
                while cursor != source {
                    let predecessor_id = predecessor[cursor]
                        .clone()
                        .ok_or(OrlinMaxError::Invariant)?;
                    let predecessor_arc =
                        state.arc(&predecessor_id).ok_or(OrlinMaxError::Invariant)?;
                    path.push(predecessor_id);
                    cursor = predecessor_arc.from.as_usize();
                }
                path.reverse();
                return Ok(Some(path));
            }
            queue.push_back(arc.to.as_usize());
        }
    }
    Ok(None)
}

fn augment_physical_path(
    state: &mut ResidualState<'_>,
    path: &[ResidualArcId],
    amount: u128,
) -> Result<(), OrlinMaxError> {
    let mut remaining = amount;
    while remaining > 0 {
        let capacity = path
            .iter()
            .map(|id| {
                state
                    .arc(id)
                    .map(|arc| arc.capacity)
                    .ok_or(OrlinMaxError::Invariant)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .min()
            .ok_or(OrlinMaxError::Invariant)?;
        let amount = remaining.min(u128::from(capacity));
        if amount == 0 {
            return Err(OrlinMaxError::Invariant);
        }
        let amount = u64::try_from(amount).map_err(|_| OrlinMaxError::ArithmeticOverflow)?;
        state.augment(path, amount)?;
        remaining -= u128::from(amount);
    }
    Ok(())
}

fn find_cut_with_bound(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    target: Ratio,
    metrics: &mut OrlinMaxMetrics,
    record_trace: bool,
) -> Result<(Vec<bool>, u128, Vec<OriginalScanCheckpoint>), OrlinMaxError> {
    let mut collector = OriginalScanCollector::new(record_trace);
    let maximum = (0..state.graph().nodes().len())
        .map(|node| NodeIndex::try_from_usize(node).ok_or(OrlinMaxError::Invariant))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flat_map(|node| state.outgoing_arcs(node))
        .filter(|arc| arc.to != source && arc.from != sink && arc.from != arc.to)
        .map(|arc| u128::from(arc.capacity))
        .max()
        .unwrap_or(0);
    let mut threshold = highest_power_of_two(maximum).max(1);
    loop {
        let reachable =
            residual_reachable(state, source, sink, threshold, metrics, &mut collector)?;
        if !reachable[sink.as_usize()] {
            let cut =
                residual_cut_capacity(state, source, sink, &reachable, metrics, &mut collector)?;
            if target.admits(cut)? {
                return Ok((reachable, cut, collector.finish()));
            }
        }
        if threshold == 1 {
            return Err(OrlinMaxError::Invariant);
        }
        threshold /= 2;
    }
}

fn residual_reachable(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    threshold: u128,
    metrics: &mut OrlinMaxMetrics,
    collector: &mut OriginalScanCollector,
) -> Result<Vec<bool>, OrlinMaxError> {
    let mut reachable = vec![false; state.graph().nodes().len()];
    let mut queue = VecDeque::from([source]);
    reachable[source.as_usize()] = true;
    while let Some(node) = queue.pop_front() {
        for arc in state.outgoing_arcs(node) {
            charge_scan(metrics)?;
            collector.observe(&arc.id, state.flows(), *metrics);
            if arc.to == source
                || arc.from == sink
                || arc.from == arc.to
                || u128::from(arc.capacity) < threshold
                || reachable[arc.to.as_usize()]
            {
                continue;
            }
            reachable[arc.to.as_usize()] = true;
            queue.push_back(arc.to);
        }
    }
    Ok(reachable)
}

fn residual_cut_capacity(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    reachable: &[bool],
    metrics: &mut OrlinMaxMetrics,
    collector: &mut OriginalScanCollector,
) -> Result<u128, OrlinMaxError> {
    let mut cut = 0_u128;
    for node in 0..state.graph().nodes().len() {
        if !reachable[node] {
            continue;
        }
        let index = NodeIndex::try_from_usize(node).ok_or(OrlinMaxError::Invariant)?;
        for arc in state.outgoing_arcs(index) {
            charge_scan(metrics)?;
            collector.observe(&arc.id, state.flows(), *metrics);
            if arc.to == source || arc.from == sink || arc.from == arc.to {
                continue;
            }
            if !reachable[arc.to.as_usize()] {
                cut = cut
                    .checked_add(u128::from(arc.capacity))
                    .ok_or(OrlinMaxError::ArithmeticOverflow)?;
            }
        }
    }
    Ok(cut)
}

fn validate_public_snapshot(
    graph: &FlowNetwork,
    snapshot: &OrlinMaxSnapshot,
) -> Result<(), OrlinMaxError> {
    if snapshot.gamma_denominator == 0
        || snapshot.nodes.len() != graph.nodes().len()
        || snapshot.residual_arcs.len() != graph.edges().len() * 2
    {
        return Err(OrlinMaxError::TraceVerification);
    }
    for (ordinal, arc) in snapshot.compact_arcs.iter().enumerate() {
        if arc.ordinal != ordinal
            || arc.flow > arc.capacity
            || arc.from_component == arc.to_component
            || arc.witness.is_empty()
        {
            return Err(OrlinMaxError::TraceVerification);
        }
    }
    for &(ordinal, _) in &snapshot.active_compact_path {
        if ordinal >= snapshot.compact_arcs.len() {
            return Err(OrlinMaxError::TraceVerification);
        }
    }
    for id in &snapshot.active_original_path {
        if graph.edge_index(id.original_edge()).is_none() {
            return Err(OrlinMaxError::TraceVerification);
        }
    }
    for arc in &snapshot.residual_arcs {
        if graph.edge_index(arc.id.original_edge()).is_none() {
            return Err(OrlinMaxError::TraceVerification);
        }
    }
    if snapshot.stage == OrlinMaxStage::Optimal {
        if snapshot.delta != 0 || snapshot.certified_flows.is_none() {
            return Err(OrlinMaxError::TraceVerification);
        }
    } else if snapshot.certified_flows.is_some() {
        return Err(OrlinMaxError::TraceVerification);
    }
    Ok(())
}

/// Independently checks public snapshot structure, event continuity, the final
/// max-flow certificate, and byte-for-byte deterministic source replay.
///
/// # Errors
///
/// Returns [`OrlinMaxError::TraceVerification`] for a forged trace.
pub fn check_orlin_max_flow_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: &OrlinMaxTraceResult,
) -> Result<(), OrlinMaxError> {
    validate_graph(graph, source, sink)?;
    validate_public_snapshot(graph, &trace.base_snapshot)?;
    if trace.base_snapshot.stage != OrlinMaxStage::Ready {
        return Err(OrlinMaxError::TraceVerification);
    }
    let mut cursor = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != cursor {
            return Err(OrlinMaxError::TraceVerification);
        }
        validate_public_snapshot(graph, &event.after)?;
        cursor = &event.after;
    }
    if cursor != &trace.final_snapshot
        || trace.result.final_snapshot != trace.final_snapshot
        || trace.final_snapshot.stage != OrlinMaxStage::Optimal
        || trace.final_snapshot.certified_flows.as_deref() != Some(trace.result.flows.as_slice())
    {
        return Err(OrlinMaxError::TraceVerification);
    }
    check_max_flow(graph, source, sink, &trace.result.flows)
        .map_err(|_| OrlinMaxError::TraceVerification)?;
    let expected = solve_internal(graph, source, sink, true)?;
    if trace.base_snapshot != expected.base_snapshot
        || trace.events != expected.events
        || trace.final_snapshot != expected.final_snapshot
        || trace.result != expected.result
    {
        return Err(OrlinMaxError::TraceVerification);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    fn graph(
        node_ids: &[&str],
        edges: &[(&str, &str, &str, u64)],
    ) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let graph = FlowNetwork::new(
            node_ids
                .iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
                .collect(),
            edges
                .iter()
                .map(|&(id, from, to, capacity)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("edge id"),
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
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        (graph, source, sink)
    }

    fn dinic_phase_stress_graph(count: usize) -> (FlowNetwork, NodeIndex, NodeIndex) {
        assert!(count >= 2);
        let node_id = |index: usize| {
            if index == 0 {
                "s".to_owned()
            } else if index + 1 == count {
                "t".to_owned()
            } else {
                format!("v{index:04}")
            }
        };
        let nodes = (0..count)
            .map(|index| FlowNode::new(NodeId::parse(&node_id(index)).expect("node id"), 0))
            .collect::<Vec<_>>();
        let mut edges = Vec::with_capacity(count * 2 - 3);
        for index in 0..count - 1 {
            edges.push(UnresolvedFlowEdge {
                id: EdgeId::parse(&format!("chain-{index:04}")).expect("edge id"),
                from: NodeId::parse(&node_id(index)).expect("from"),
                to: NodeId::parse(&node_id(index + 1)).expect("to"),
                lower: 0,
                capacity: if index + 2 == count {
                    1
                } else {
                    u64::try_from(count).expect("count")
                },
                cost: 0,
            });
        }
        for index in 0..count.saturating_sub(2) {
            edges.push(UnresolvedFlowEdge {
                id: EdgeId::parse(&format!("shortcut-{index:04}")).expect("edge id"),
                from: NodeId::parse(&node_id(index)).expect("from"),
                to: NodeId::parse("t").expect("to"),
                lower: 0,
                capacity: 1,
                cost: 0,
            });
        }
        let graph = FlowNetwork::new(nodes, edges).expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        (graph, source, sink)
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn improvement_phases_produce_an_exact_certified_flow() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "t"],
            &[
                ("sa", "s", "a", 5),
                ("sb", "s", "b", 4),
                ("ab", "a", "b", 2),
                ("at", "a", "t", 3),
                ("bt", "b", "t", 6),
            ],
        );
        let fast = solve_orlin_max_flow(&graph, source, sink).expect("fast");
        let trace = trace_orlin_max_flow(&graph, source, sink).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(fast.certificate.value, 9);
        assert_eq!(fast.certificate.cut_bound, 9);
        assert!(
            trace
                .events
                .iter()
                .any(|event| { event.after.stage == OrlinMaxStage::Classify })
        );
        assert!(
            trace
                .events
                .iter()
                .any(|event| { event.after.stage == OrlinMaxStage::LiftPath })
        );
        assert!(trace.events.iter().any(|event| {
            event.after.stage == OrlinMaxStage::InspectSubproblemArc
                && event.after.active_compact_path.len() == 1
        }));
        assert!(trace.events.iter().any(|event| {
            event.after.stage == OrlinMaxStage::InspectDecompositionArc
                && event.after.active_compact_path.len() == 1
        }));
        assert!(trace.events.iter().any(|event| {
            event.after.stage == OrlinMaxStage::InspectLiftResidualArc
                && event.after.active_original_path.len() == 1
        }));
        assert!(trace.events.iter().any(|event| {
            event.after.stage == OrlinMaxStage::InspectClassificationArc
                && event.after.active_original_path.len() == 1
        }));
        for event in &trace.events {
            let scan_delta = event
                .after
                .metrics
                .residual_arc_scans
                .checked_sub(event.before.metrics.residual_arc_scans)
                .expect("monotone scan counter");
            if matches!(
                event.after.stage,
                OrlinMaxStage::InspectClassificationArc
                    | OrlinMaxStage::InspectCompactConstructionArc
                    | OrlinMaxStage::InspectSubproblemArc
                    | OrlinMaxStage::InspectDecompositionArc
                    | OrlinMaxStage::InspectLiftResidualArc
                    | OrlinMaxStage::InspectExpansionResidualArc
                    | OrlinMaxStage::InspectCutResidualArc
            ) {
                assert!(scan_delta > 0);
                assert!(scan_delta <= ORLIN_MAX_FLOW_SCAN_CHECKPOINT_STRIDE);
            } else {
                assert_eq!(
                    scan_delta, 0,
                    "charged scans must not be hidden in {:?}",
                    event.after.stage
                );
            }
            match event.after.stage {
                OrlinMaxStage::InspectSubproblemArc | OrlinMaxStage::InspectDecompositionArc => {
                    let serials = event
                        .after
                        .compact_arcs
                        .iter()
                        .filter_map(|arc| arc.inspection_serial)
                        .collect::<Vec<_>>();
                    assert_eq!(serials, vec![event.after.metrics.residual_arc_scans]);
                    assert!(
                        event
                            .after
                            .residual_arcs
                            .iter()
                            .all(|arc| arc.inspection_serial.is_none())
                    );
                }
                OrlinMaxStage::InspectClassificationArc
                | OrlinMaxStage::InspectCompactConstructionArc
                | OrlinMaxStage::InspectLiftResidualArc
                | OrlinMaxStage::InspectExpansionResidualArc
                | OrlinMaxStage::InspectCutResidualArc => {
                    let serials = event
                        .after
                        .residual_arcs
                        .iter()
                        .filter_map(|arc| arc.inspection_serial)
                        .collect::<Vec<_>>();
                    assert_eq!(serials, vec![event.after.metrics.residual_arc_scans]);
                    assert!(
                        event
                            .after
                            .compact_arcs
                            .iter()
                            .all(|arc| arc.inspection_serial.is_none())
                    );
                }
                _ => {
                    assert!(
                        event
                            .after
                            .compact_arcs
                            .iter()
                            .all(|arc| arc.inspection_serial.is_none())
                    );
                    assert!(
                        event
                            .after
                            .residual_arcs
                            .iter()
                            .all(|arc| arc.inspection_serial.is_none())
                    );
                }
            }
        }
        for pair in trace.events.windows(2) {
            if matches!(
                pair[0].after.stage,
                OrlinMaxStage::InspectClassificationArc
                    | OrlinMaxStage::InspectCompactConstructionArc
                    | OrlinMaxStage::InspectSubproblemArc
                    | OrlinMaxStage::InspectDecompositionArc
                    | OrlinMaxStage::InspectLiftResidualArc
                    | OrlinMaxStage::InspectExpansionResidualArc
                    | OrlinMaxStage::InspectCutResidualArc
            ) && pair[0].after.stage == pair[1].after.stage
            {
                assert_ne!(
                    pair[0].after.metrics.residual_arc_scans,
                    pair[1].after.metrics.residual_arc_scans,
                    "successive inspections must advance the visible source cursor"
                );
            }
        }
        check_orlin_max_flow_trace(&graph, source, sink, &trace).expect("checker");
    }

    #[test]
    fn dense_source_scans_remain_attributed_on_the_40_node_phase_stress_case() {
        let (graph, source, sink) = dinic_phase_stress_graph(40);
        let trace = trace_orlin_max_flow(&graph, source, sink).expect("trace");
        let unattributed_scan_stages = trace
            .events
            .iter()
            .filter(|event| {
                event.after.metrics.residual_arc_scans > event.before.metrics.residual_arc_scans
                    && !matches!(
                        event.after.stage,
                        OrlinMaxStage::InspectClassificationArc
                            | OrlinMaxStage::InspectCompactConstructionArc
                            | OrlinMaxStage::InspectSubproblemArc
                            | OrlinMaxStage::InspectDecompositionArc
                            | OrlinMaxStage::InspectLiftResidualArc
                            | OrlinMaxStage::InspectExpansionResidualArc
                            | OrlinMaxStage::InspectCutResidualArc
                    )
            })
            .map(|event| event.after.stage)
            .collect::<Vec<_>>();
        assert!(
            unattributed_scan_stages.is_empty(),
            "every charged scan must be published by a local inspection boundary; found {unattributed_scan_stages:?}"
        );
        assert!(trace.events.iter().any(|event| {
            event.after.stage == OrlinMaxStage::InspectClassificationArc
                && event.after.active_original_path.len() == 1
        }));
        assert!(trace.events.iter().any(|event| {
            event.after.stage == OrlinMaxStage::InspectCutResidualArc
                && event.after.active_original_path.len() == 1
        }));
        let maximum_scan_delta = trace
            .events
            .iter()
            .map(|event| {
                event
                    .after
                    .metrics
                    .residual_arc_scans
                    .checked_sub(event.before.metrics.residual_arc_scans)
                    .expect("monotone scan counter")
            })
            .max()
            .unwrap_or(0);
        assert!(
            maximum_scan_delta <= ORLIN_MAX_FLOW_SCAN_CHECKPOINT_STRIDE,
            "one semantic boundary hides {maximum_scan_delta} source arc inspections"
        );
        assert!(trace.events.iter().any(|event| {
            event.after.stage == OrlinMaxStage::InspectDecompositionArc
                && event.after.metrics.residual_arc_scans > event.before.metrics.residual_arc_scans
        }));
        let decomposition = trace
            .events
            .iter()
            .filter(|event| event.after.stage == OrlinMaxStage::InspectDecompositionArc)
            .map(|event| {
                event
                    .after
                    .compact_arcs
                    .iter()
                    .map(|arc| arc.flow)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert!(
            decomposition.windows(2).all(|pair| pair[1]
                .iter()
                .zip(&pair[0])
                .all(|(after, before)| after <= before)),
            "decomposition checkpoints must publish the remaining logical flow"
        );
        assert!(
            decomposition
                .first()
                .zip(decomposition.last())
                .is_some_and(|(first, last)| first != last),
            "the stress trace must expose at least one visible decomposition-flow reduction"
        );
        check_orlin_max_flow_trace(&graph, source, sink, &trace).expect("checker");
    }

    #[test]
    fn all_three_source_cases_and_capacity_transfer_are_exercised() {
        let (original, original_source, original_sink) = graph(
            &["s", "a", "b", "t"],
            &[
                ("sa", "s", "a", 5),
                ("sb", "s", "b", 4),
                ("ab", "a", "b", 2),
                ("at", "a", "t", 3),
                ("bt", "b", "t", 6),
            ],
        );
        let original_trace =
            trace_orlin_max_flow(&original, original_source, original_sink).expect("original");
        assert!(original_trace.events.iter().any(|event| {
            event.after.phase_case == Some(OrlinMaxPhaseCase::OriginalApproximation)
        }));

        let compact_approx_edges = [
            ("sa", "s", "a", 10_000),
            ("ab", "a", "b", 1),
            ("ba", "b", "a", 40_000),
            ("bc", "b", "c", 1),
            ("cb", "c", "b", 40_000),
            ("ct", "c", "t", 10_000),
            ("z0", "s", "s", 0),
            ("z1", "a", "a", 0),
            ("z2", "b", "b", 0),
            ("z3", "c", "c", 0),
            ("z4", "t", "t", 0),
            ("z5", "s", "t", 0),
        ];
        let (compact, compact_source, compact_sink) =
            graph(&["s", "a", "b", "c", "t"], &compact_approx_edges);
        let compact_trace =
            trace_orlin_max_flow(&compact, compact_source, compact_sink).expect("compact");
        assert_eq!(compact_trace.result.certificate.value, 1);
        assert!(compact_trace.events.iter().any(|event| {
            event.after.phase_case == Some(OrlinMaxPhaseCase::CompactApproximation)
        }));
        assert!(compact_trace.events.iter().any(|event| {
            event.after.stage == OrlinMaxStage::TransferCapacity
                && event.after.active_original_path.len() >= 2
        }));
        assert!(compact_trace.events.iter().any(|event| {
            event.after.compact_arcs.iter().any(|arc| {
                arc.kind == OrlinMaxCompactArcKind::TransferredPseudo && arc.witness.len() >= 2
            })
        }));
        check_orlin_max_flow_trace(&compact, compact_source, compact_sink, &compact_trace)
            .expect("compact checker");

        let compact_exact_edges = [
            ("st", "s", "t", 1),
            ("z0", "s", "s", 0),
            ("z1", "s", "t", 0),
            ("z2", "t", "s", 0),
            ("z3", "t", "t", 0),
            ("z4", "s", "s", 0),
            ("z5", "s", "t", 0),
            ("z6", "t", "s", 0),
            ("z7", "t", "t", 0),
        ];
        let (exact, exact_source, exact_sink) = graph(&["s", "t"], &compact_exact_edges);
        let exact_trace =
            trace_orlin_max_flow(&exact, exact_source, exact_sink).expect("compact exact");
        assert_eq!(exact_trace.result.certificate.value, 1);
        assert!(
            exact_trace
                .events
                .iter()
                .any(|event| { event.after.phase_case == Some(OrlinMaxPhaseCase::CompactExact) })
        );
        assert!(exact_trace.result.metrics.exact_subproblems > 0);
        check_orlin_max_flow_trace(&exact, exact_source, exact_sink, &exact_trace)
            .expect("exact checker");
    }

    #[test]
    fn parallel_opposite_self_loop_and_zero_capacity_edges_are_supported() {
        let (graph, source, sink) = graph(
            &["s", "x", "t"],
            &[
                ("loop", "x", "x", 7),
                ("sx-a", "s", "x", 2),
                ("sx-b", "s", "x", 3),
                ("xs", "x", "s", 4),
                ("xt", "x", "t", 4),
                ("st", "s", "t", 1),
                ("zero", "t", "s", 0),
            ],
        );
        let result = solve_orlin_max_flow(&graph, source, sink).expect("solve");
        assert_eq!(result.certificate.value, 5);
        assert_eq!(result.certificate.cut_bound, 5);
    }

    #[test]
    fn deterministic_bounded_networks_are_independently_certified() {
        fn next(seed: &mut u64) -> u64 {
            *seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            *seed
        }
        let mut seed = 7_u64;
        for case in 0..64 {
            let mut edges = Vec::new();
            let candidates = [
                ("s", "a"),
                ("s", "b"),
                ("a", "b"),
                ("b", "a"),
                ("a", "t"),
                ("b", "t"),
                ("s", "t"),
            ];
            for (ordinal, &(from, to)) in candidates.iter().enumerate() {
                let capacity = next(&mut seed) % 4;
                edges.push((
                    format!("e{case}-{ordinal}"),
                    from.to_owned(),
                    to.to_owned(),
                    capacity,
                ));
            }
            let borrowed = edges
                .iter()
                .map(|(id, from, to, capacity)| {
                    (id.as_str(), from.as_str(), to.as_str(), *capacity)
                })
                .collect::<Vec<_>>();
            let (graph, source, sink) = graph(&["s", "a", "b", "t"], &borrowed);
            solve_orlin_max_flow(&graph, source, sink).expect("certified case");
        }
    }

    #[test]
    fn trace_checker_rejects_a_forged_compact_flow() {
        let (graph, source, sink) = graph(
            &["s", "a", "t"],
            &[("sa", "s", "a", 3), ("at", "a", "t", 3)],
        );
        let mut trace = trace_orlin_max_flow(&graph, source, sink).expect("trace");
        let event = trace
            .events
            .iter_mut()
            .find(|event| !event.after.compact_arcs.is_empty())
            .expect("subproblem event");
        event.after.compact_arcs[0].capacity += 1;
        assert_eq!(
            check_orlin_max_flow_trace(&graph, source, sink, &trace),
            Err(OrlinMaxError::TraceVerification)
        );
    }

    #[test]
    fn graph_requirements_and_admission_fail_closed() {
        let (mut graph, source, sink) = graph(&["s", "t"], &[("st", "s", "t", 1)]);
        let wrong_terminal = source;
        assert_eq!(
            solve_orlin_max_flow(&graph, source, wrong_terminal),
            Err(OrlinMaxError::GraphRequirement)
        );

        let nodes = (0..=ORLIN_MAX_FLOW_MAX_NODES)
            .map(|ordinal| FlowNode::new(NodeId::parse(&format!("n{ordinal}")).expect("id"), 0))
            .collect::<Vec<_>>();
        graph = FlowNetwork::new(nodes, Vec::new()).expect("oversized graph");
        let first = NodeIndex::try_from_usize(0).expect("first");
        let second = NodeIndex::try_from_usize(1).expect("second");
        assert_eq!(
            solve_orlin_max_flow(&graph, first, second),
            Err(OrlinMaxError::AdmissionLimit)
        );
        let _ = sink;
    }
}
