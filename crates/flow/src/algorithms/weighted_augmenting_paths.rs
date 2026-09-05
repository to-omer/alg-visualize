//! Bounded source-defined realization of Bernstein--Blikstad--Saranurak--Tu.
//!
//! The source algorithm computes exact maximum flow by capacity scaling and
//! repeated calls to a relabel-prioritized weighted push-relabel routine.  Its
//! weights come from a directed expander hierarchy and an order respecting
//! that hierarchy.  The paper's randomized `n^{2+o(1)}` hierarchy builder is
//! intentionally replaced here by a deterministic small-graph oracle: every
//! residual SCC is the sole expanding level, inter-SCC arcs form the DAG part,
//! and every directed cut is enumerated to certify the largest valid `phi`.
//! This preserves the source definitions and transitions, but does not claim
//! the paper's asymptotic construction time.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{EdgeId, FlowEdge, FlowNetwork, NodeIndex};
use crate::residual::{
    ResidualArc, ResidualArcId, ResidualDirection, ResidualError, ResidualState,
};

/// Node ceiling for exhaustive directed-cut certification.
pub const WEIGHTED_AUGMENTING_PATHS_MAX_NODES: usize = 8;
/// Edge ceiling for exhaustive directed-cut certification.
pub const WEIGHTED_AUGMENTING_PATHS_MAX_EDGES: usize = 12;
/// Capacity ceiling keeping exact scaling traces interactive.
pub const WEIGHTED_AUGMENTING_PATHS_MAX_CAPACITY: u64 = 64;
/// Maximum exact hierarchy cuts inspected across a run.
pub const WEIGHTED_AUGMENTING_PATHS_MAX_CUTS: u64 = 250_000;
/// Maximum accelerated relabel jumps across a run.
pub const WEIGHTED_AUGMENTING_PATHS_MAX_RELABEL_JUMPS: u64 = 500_000;
/// Maximum augmenting paths across all scaling phases.
pub const WEIGHTED_AUGMENTING_PATHS_MAX_AUGMENTATIONS: u64 = 100_000;
/// Maximum public reversible boundaries.
pub const WEIGHTED_AUGMENTING_PATHS_MAX_TRACE_EVENTS: usize = 32_768;
/// Maximum residual approximation rounds in one scaling phase.
pub const WEIGHTED_AUGMENTING_PATHS_MAX_ROUNDS: u64 = 64;

/// Source-defined role of one residual direction in the one-level hierarchy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeightedAugmentingHierarchyKind {
    /// Arc between residual SCCs; these arcs form an acyclic graph.
    Dag,
    /// Arc internal to a residual SCC; this is the expanding set `X_1`.
    Expanding,
}

/// Semantic publication boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeightedAugmentingPathsStage {
    /// Zero flow before the most-significant capacity bit.
    Ready,
    /// The next capacity prefix and doubled previous flow are installed.
    BeginCapacityPhase,
    /// Residual SCCs and the canonical one-level hierarchy are installed.
    BuildHierarchy,
    /// All relevant directed cuts have certified the hierarchy's `phi`.
    CertifyExpansion,
    /// A respecting topological order and edge weights are installed.
    AssignWeights,
    /// One or more source relabel jumps were compressed into one sweep.
    RelabelSweep,
    /// One source-defined admissible path was augmented.
    AugmentPath,
    /// One weighted push-relabel call terminated.
    FinishWeightedRound,
    /// A scaling-phase residual cut proves the current prefix is maximal.
    FinishCapacityPhase,
    /// The independent original-network max-flow/min-cut checker passed.
    CheckCertificate,
    /// Certified exact maximum flow is public.
    Optimal,
}

/// Per-node hierarchy, label, and cut projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedAugmentingNodeState {
    /// Canonical original node.
    pub node: NodeIndex,
    /// Canonical residual SCC ordinal.
    pub component: usize,
    /// One-based hierarchy-respecting topological order.
    pub order: usize,
    /// Current weighted push-relabel level.
    pub label: u64,
    /// Vertices above `9h` are dead for the current call.
    pub alive: bool,
    /// Membership in the cut attaining the displayed `phi` ratio.
    pub expansion_witness_side: bool,
    /// Current residual reachability from the source.
    pub source_side: bool,
}

/// Per-original-edge state at one scaling prefix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedAugmentingEdgeState {
    /// Stable original identity.
    pub edge: EdgeId,
    /// Capacity represented by the current binary prefix.
    pub scaled_capacity: u64,
    /// Current integral flow.
    pub flow: u64,
}

/// One stable original-edge residual direction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedAugmentingResidualArcState {
    /// Stable residual identity.
    pub id: ResidualArcId,
    /// Residual tail.
    pub from: NodeIndex,
    /// Residual head.
    pub to: NodeIndex,
    /// Current residual capacity; zero directions remain visible.
    pub capacity: u64,
    /// Hierarchy role, absent before hierarchy construction.
    pub hierarchy_kind: Option<WeightedAugmentingHierarchyKind>,
    /// Source weight `|tau(u)-tau(v)|`; zero before assignment.
    pub weight: u64,
    /// Whether this direction is currently admissible.
    pub admissible: bool,
    /// Whether this direction belongs to the active augmenting path.
    pub active: bool,
}

/// Exact deterministic work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WeightedAugmentingPathsMetrics {
    /// Binary capacity-prefix phases.
    pub capacity_phases: u64,
    /// Residual hierarchy builds.
    pub hierarchy_builds: u64,
    /// Directed cuts inspected for exact expansion certification.
    pub hierarchy_cuts: u64,
    /// Weighted push-relabel calls.
    pub weighted_rounds: u64,
    /// Public compressed relabel sweeps.
    pub relabel_sweeps: u64,
    /// Source accelerated jumps to the next incident-weight multiple.
    pub relabel_jumps: u64,
    /// Residual/original arcs inspected while finding and refreshing relabels.
    pub relabel_arc_inspections: u64,
    /// Admissibility flag changes.
    pub admissible_updates: u64,
    /// Source-defined augmenting paths.
    pub augmentations: u64,
    /// Total augmented units across capacity prefixes.
    pub augmented_units: u128,
    /// Exact residual cut checks at phase ends.
    pub residual_cut_checks: u64,
    /// Independent original-network certificate checks.
    pub certificate_checks: u64,
    /// Public reversible transitions.
    pub state_transitions: u64,
}

/// Complete reversible state at one semantic boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedAugmentingPathsSnapshot {
    /// Semantic stage.
    pub stage: WeightedAugmentingPathsStage,
    /// Zero-based capacity-prefix phase.
    pub phase: usize,
    /// Total number of capacity bits.
    pub phase_count: usize,
    /// Bit position currently being installed, most significant first.
    pub capacity_bit: usize,
    /// Zero-based weighted residual round inside the phase.
    pub round: u64,
    /// Source height parameter `h`.
    pub height: u64,
    /// Exact reduced expansion ratio numerator.
    pub phi_numerator: u128,
    /// Exact positive expansion ratio denominator.
    pub phi_denominator: u128,
    /// Active path bottleneck, zero off an augmentation boundary.
    pub active_bottleneck: u64,
    /// Node projections.
    pub nodes: Vec<WeightedAugmentingNodeState>,
    /// Original-edge projections.
    pub edges: Vec<WeightedAugmentingEdgeState>,
    /// Both stable residual directions per original edge.
    pub residual_arcs: Vec<WeightedAugmentingResidualArcState>,
    /// Active path in stable residual identities.
    pub active_path: Vec<ResidualArcId>,
    /// Exact counters.
    pub metrics: WeightedAugmentingPathsMetrics,
    /// Final flow only after independent certification.
    pub certified_flows: Option<Vec<u64>>,
}

/// One reversible source transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedAugmentingPathsTraceEvent {
    /// Stable catalog identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: WeightedAugmentingPathsSnapshot,
    /// State after the transition.
    pub after: WeightedAugmentingPathsSnapshot,
}

/// Certified exact maximum-flow result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedAugmentingPathsResult {
    /// Original-edge flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Independent maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact bounded-work counters.
    pub metrics: WeightedAugmentingPathsMetrics,
    /// Fast-profile terminal state.
    pub final_snapshot: WeightedAugmentingPathsSnapshot,
}

/// Exact result plus all reversible source boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedAugmentingPathsTraceResult {
    /// Same certified result returned by the fast profile.
    pub result: WeightedAugmentingPathsResult,
    /// Ready boundary.
    pub base_snapshot: WeightedAugmentingPathsSnapshot,
    /// Complete deterministic event sequence.
    pub events: Vec<WeightedAugmentingPathsTraceEvent>,
    /// Independently certified terminal boundary.
    pub final_snapshot: WeightedAugmentingPathsSnapshot,
}

/// Construction, admission, replay, or certification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum WeightedAugmentingPathsError {
    /// Input exceeds the explicit interactive admission band.
    #[error("graph exceeds weighted augmenting-path admission limits")]
    AdmissionLimit,
    /// The source-defined zero-feasible max-flow domain was violated.
    #[error(
        "weighted augmenting paths require distinct terminals, zero lower bounds and supplies, no self-loops, and positive capacities"
    )]
    GraphRequirement,
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Independent maximum-flow/minimum-cut verification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact bounded arithmetic overflowed.
    #[error("weighted augmenting-path arithmetic overflow")]
    ArithmeticOverflow,
    /// A declared bounded-work ceiling was reached.
    #[error("weighted augmenting-path deterministic work limit exceeded")]
    WorkLimit,
    /// A source hierarchy, label, path, or scaling invariant failed.
    #[error("weighted augmenting-path construction invariant failed")]
    Invariant,
    /// A public trace failed deterministic replay or snapshot validation.
    #[error("weighted augmenting-path trace verification failed")]
    TraceVerification,
}

/// Solves exact maximum flow using the bounded source construction.
///
/// # Errors
///
/// Rejects unsupported graphs, bounded-work exhaustion, invariant failures,
/// arithmetic overflow, or failed independent certification.
pub fn solve_weighted_augmenting_paths(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<WeightedAugmentingPathsResult, WeightedAugmentingPathsError> {
    solve_internal(graph, source, sink, false).map(|run| run.result)
}

/// Records every scaling, hierarchy, relabel, augmentation, and check boundary.
///
/// # Errors
///
/// Returns the same failures as [`solve_weighted_augmenting_paths`].
pub fn trace_weighted_augmenting_paths(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<WeightedAugmentingPathsTraceResult, WeightedAugmentingPathsError> {
    let run = solve_internal(graph, source, sink, true)?;
    Ok(WeightedAugmentingPathsTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    })
}

/// Re-executes a public trace and checks every boundary by exact equality.
///
/// # Errors
///
/// Rejects a malformed snapshot, broken event chain, or any divergence from a
/// fresh deterministic source execution.
pub fn verify_weighted_augmenting_paths_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: &WeightedAugmentingPathsTraceResult,
) -> Result<(), WeightedAugmentingPathsError> {
    validate_snapshot(graph, &trace.base_snapshot)?;
    validate_snapshot(graph, &trace.final_snapshot)?;
    let mut cursor = &trace.base_snapshot;
    for event in &trace.events {
        if event.catalog_id != "weighted-augmenting-paths" || &event.before != cursor {
            return Err(WeightedAugmentingPathsError::TraceVerification);
        }
        validate_snapshot(graph, &event.after)?;
        cursor = &event.after;
    }
    if cursor != &trace.final_snapshot || trace.result.final_snapshot != trace.final_snapshot {
        return Err(WeightedAugmentingPathsError::TraceVerification);
    }
    let expected = trace_weighted_augmenting_paths(graph, source, sink)?;
    if &expected != trace {
        return Err(WeightedAugmentingPathsError::TraceVerification);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Ratio {
    numerator: u128,
    denominator: u128,
}

impl Ratio {
    const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    fn new(numerator: u128, denominator: u128) -> Result<Self, WeightedAugmentingPathsError> {
        if numerator == 0 || denominator == 0 {
            return Err(WeightedAugmentingPathsError::Invariant);
        }
        let divisor = gcd(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    fn less_than(self, other: Self) -> Result<bool, WeightedAugmentingPathsError> {
        let left = self
            .numerator
            .checked_mul(other.denominator)
            .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
        let right = other
            .numerator
            .checked_mul(self.denominator)
            .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
        Ok(left < right)
    }
}

const fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    if left == 0 { 1 } else { left }
}

#[derive(Clone, Debug)]
struct Hierarchy {
    component: Vec<usize>,
    order: Vec<usize>,
    weights: Vec<u64>,
    phi: Ratio,
    witness: Vec<bool>,
}

struct InternalRun {
    result: WeightedAugmentingPathsResult,
    base_snapshot: WeightedAugmentingPathsSnapshot,
    events: Vec<WeightedAugmentingPathsTraceEvent>,
    final_snapshot: WeightedAugmentingPathsSnapshot,
}

struct Runner<'graph> {
    graph: &'graph FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    state: ResidualState<'graph>,
    scaled_capacities: Vec<u64>,
    labels: Vec<u64>,
    alive: Vec<bool>,
    admissible: Vec<bool>,
    hierarchy: Option<Hierarchy>,
    phase: usize,
    phase_count: usize,
    capacity_bit: usize,
    round: u64,
    height: u64,
    active_path: Vec<ResidualArcId>,
    active_bottleneck: u64,
    metrics: WeightedAugmentingPathsMetrics,
    certified_flows: Option<Vec<u64>>,
    trace: bool,
    base_snapshot: WeightedAugmentingPathsSnapshot,
    current_snapshot: WeightedAugmentingPathsSnapshot,
    events: Vec<WeightedAugmentingPathsTraceEvent>,
}

fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: bool,
) -> Result<InternalRun, WeightedAugmentingPathsError> {
    validate_input(graph, source, sink)?;
    let phase_count = capacity_bit_count(graph);
    let scaled_capacities = vec![0; graph.edges().len()];
    let state = ResidualState::from_current_capacities_and_flows(
        graph,
        &scaled_capacities,
        &vec![0; graph.edges().len()],
    )?;
    let empty = empty_snapshot(graph, phase_count);
    let mut runner = Runner {
        graph,
        source,
        sink,
        state,
        scaled_capacities,
        labels: vec![0; graph.nodes().len()],
        alive: vec![true; graph.nodes().len()],
        admissible: vec![false; graph.edges().len() * 2],
        hierarchy: None,
        phase: 0,
        phase_count,
        capacity_bit: phase_count.saturating_sub(1),
        round: 0,
        height: 0,
        active_path: Vec::new(),
        active_bottleneck: 0,
        metrics: WeightedAugmentingPathsMetrics::default(),
        certified_flows: None,
        trace,
        base_snapshot: empty.clone(),
        current_snapshot: empty,
        events: Vec::new(),
    };
    runner.run()?;
    let certificate = check_max_flow(graph, source, sink, runner.state.flows())?;
    runner.metrics.certificate_checks = runner
        .metrics
        .certificate_checks
        .checked_add(1)
        .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
    runner.emit(WeightedAugmentingPathsStage::CheckCertificate)?;
    runner.certified_flows = Some(runner.state.flows().to_vec());
    runner.emit(WeightedAugmentingPathsStage::Optimal)?;
    let final_snapshot = runner.current_snapshot.clone();
    let result = WeightedAugmentingPathsResult {
        flows: runner.state.flows().to_vec(),
        certificate,
        metrics: runner.metrics,
        final_snapshot: final_snapshot.clone(),
    };
    Ok(InternalRun {
        result,
        base_snapshot: runner.base_snapshot,
        events: runner.events,
        final_snapshot,
    })
}

impl Runner<'_> {
    fn run(&mut self) -> Result<(), WeightedAugmentingPathsError> {
        for phase in 0..self.phase_count {
            self.phase = phase;
            self.capacity_bit = self.phase_count - phase - 1;
            self.begin_capacity_phase()?;
            self.metrics.capacity_phases = self
                .metrics
                .capacity_phases
                .checked_add(1)
                .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
            self.emit(WeightedAugmentingPathsStage::BeginCapacityPhase)?;
            self.round = 0;
            loop {
                if self.round >= WEIGHTED_AUGMENTING_PATHS_MAX_ROUNDS {
                    return Err(WeightedAugmentingPathsError::WorkLimit);
                }
                if !residual_reachable(&self.state, self.source)[self.sink.as_usize()] {
                    break;
                }
                self.run_weighted_round()?;
                self.round = self
                    .round
                    .checked_add(1)
                    .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
            }
            self.metrics.residual_cut_checks = self
                .metrics
                .residual_cut_checks
                .checked_add(1)
                .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
            self.active_path.clear();
            self.active_bottleneck = 0;
            self.emit(WeightedAugmentingPathsStage::FinishCapacityPhase)?;
        }
        Ok(())
    }

    fn begin_capacity_phase(&mut self) -> Result<(), WeightedAugmentingPathsError> {
        let previous_flows = self.state.flows().to_vec();
        for (ordinal, edge) in self.graph.edges().iter().enumerate() {
            let bit = (edge.capacity() >> self.capacity_bit) & 1;
            self.scaled_capacities[ordinal] = self.scaled_capacities[ordinal]
                .checked_mul(2)
                .and_then(|value| value.checked_add(bit))
                .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
        }
        let doubled = previous_flows
            .into_iter()
            .map(|flow| {
                flow.checked_mul(2)
                    .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if doubled
            .iter()
            .zip(&self.scaled_capacities)
            .any(|(flow, capacity)| flow > capacity)
        {
            return Err(WeightedAugmentingPathsError::Invariant);
        }
        self.state = ResidualState::from_current_capacities_and_flows(
            self.graph,
            &self.scaled_capacities,
            &doubled,
        )?;
        self.labels.fill(0);
        self.alive.fill(true);
        self.admissible.fill(false);
        self.hierarchy = None;
        self.height = 0;
        self.active_path.clear();
        self.active_bottleneck = 0;
        Ok(())
    }

    fn run_weighted_round(&mut self) -> Result<(), WeightedAugmentingPathsError> {
        self.metrics.weighted_rounds = self
            .metrics
            .weighted_rounds
            .checked_add(1)
            .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
        self.labels.fill(0);
        self.alive.fill(true);
        self.admissible.fill(false);
        self.active_path.clear();
        self.active_bottleneck = 0;
        let hierarchy = build_hierarchy(self.graph, &self.state, &mut self.metrics)?;
        self.hierarchy = Some(Hierarchy {
            phi: Ratio::ONE,
            witness: vec![false; self.graph.nodes().len()],
            ..hierarchy.clone()
        });
        self.emit(WeightedAugmentingPathsStage::BuildHierarchy)?;
        self.hierarchy = Some(hierarchy);
        self.emit(WeightedAugmentingPathsStage::CertifyExpansion)?;
        self.height = source_height(
            self.graph.nodes().len(),
            self.hierarchy
                .as_ref()
                .ok_or(WeightedAugmentingPathsError::Invariant)?
                .phi,
        )?;
        self.emit(WeightedAugmentingPathsStage::AssignWeights)?;

        loop {
            while let Some(vertex) = self.first_relabel_candidate()? {
                self.metrics.relabel_sweeps = self
                    .metrics
                    .relabel_sweeps
                    .checked_add(1)
                    .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
                self.relabel_sweep(vertex)?;
            }
            if !self.alive[self.source.as_usize()] {
                break;
            }
            let path = self.trace_admissible_path()?;
            let bottleneck = path
                .iter()
                .map(|id| {
                    self.state
                        .arc(id)
                        .map(|arc| arc.capacity)
                        .ok_or(WeightedAugmentingPathsError::Invariant)
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .min()
                .ok_or(WeightedAugmentingPathsError::Invariant)?;
            self.state.augment(&path, bottleneck)?;
            for id in &path {
                if self.state.arc(id).is_some_and(|arc| arc.capacity == 0) {
                    let ordinal = residual_ordinal(self.graph, id)?;
                    if self.admissible[ordinal] {
                        self.admissible[ordinal] = false;
                        self.metrics.admissible_updates = self
                            .metrics
                            .admissible_updates
                            .checked_add(1)
                            .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
                    }
                }
            }
            self.metrics.augmentations = self
                .metrics
                .augmentations
                .checked_add(1)
                .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
            if self.metrics.augmentations > WEIGHTED_AUGMENTING_PATHS_MAX_AUGMENTATIONS {
                return Err(WeightedAugmentingPathsError::WorkLimit);
            }
            self.metrics.augmented_units = self
                .metrics
                .augmented_units
                .checked_add(u128::from(bottleneck))
                .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
            self.active_path = path;
            self.active_bottleneck = bottleneck;
            self.emit(WeightedAugmentingPathsStage::AugmentPath)?;
            self.active_path.clear();
            self.active_bottleneck = 0;
        }
        self.emit(WeightedAugmentingPathsStage::FinishWeightedRound)?;
        Ok(())
    }

    fn first_relabel_candidate(
        &mut self,
    ) -> Result<Option<NodeIndex>, WeightedAugmentingPathsError> {
        let nodes = self.graph.node_indices().collect::<Vec<_>>();
        for node in nodes {
            if !self.alive[node.as_usize()] || node == self.sink {
                continue;
            }
            if !self.has_admissible_out_edge(node)? {
                return Ok(Some(node));
            }
        }
        Ok(None)
    }

    fn has_admissible_out_edge(
        &mut self,
        node: NodeIndex,
    ) -> Result<bool, WeightedAugmentingPathsError> {
        let outgoing = self.state.outgoing_arcs(node);
        for arc in outgoing {
            self.add_relabel_arc_inspections(1)?;
            if self.admissible[residual_ordinal(self.graph, &arc.id)?] {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn relabel_sweep(&mut self, node: NodeIndex) -> Result<(), WeightedAugmentingPathsError> {
        let limit = self
            .height
            .checked_mul(9)
            .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
        while self.alive[node.as_usize()] && !self.has_admissible_out_edge(node)? {
            let old = self.labels[node.as_usize()];
            let next = self.next_relabel_level(node, old, limit)?;
            self.labels[node.as_usize()] = next;
            self.metrics.relabel_jumps = self
                .metrics
                .relabel_jumps
                .checked_add(1)
                .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
            if self.metrics.relabel_jumps > WEIGHTED_AUGMENTING_PATHS_MAX_RELABEL_JUMPS {
                return Err(WeightedAugmentingPathsError::WorkLimit);
            }
            if next > limit {
                self.alive[node.as_usize()] = false;
            } else {
                self.refresh_incident_admissibility(node)?;
            }
            self.emit(WeightedAugmentingPathsStage::RelabelSweep)?;
            if !self.alive[node.as_usize()] {
                break;
            }
        }
        Ok(())
    }

    fn next_relabel_level(
        &mut self,
        node: NodeIndex,
        old: u64,
        limit: u64,
    ) -> Result<u64, WeightedAugmentingPathsError> {
        let hierarchy = self
            .hierarchy
            .as_ref()
            .ok_or(WeightedAugmentingPathsError::Invariant)?;
        let mut next = None;
        let mut inspected = 0_u64;
        for edge_index in self
            .graph
            .outgoing_edges(node)
            .iter()
            .chain(self.graph.incoming_edges(node))
        {
            inspected = inspected
                .checked_add(1)
                .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
            let weight = *hierarchy
                .weights
                .get(edge_index.as_usize())
                .ok_or(WeightedAugmentingPathsError::Invariant)?;
            let candidate = old
                .checked_div(weight)
                .and_then(|quotient| quotient.checked_add(1))
                .and_then(|quotient| quotient.checked_mul(weight))
                .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
            next = Some(next.map_or(candidate, |current: u64| current.min(candidate)));
        }
        self.add_relabel_arc_inspections(inspected)?;
        Ok(next
            .unwrap_or_else(|| limit.saturating_add(1))
            .min(limit.saturating_add(1)))
    }

    fn refresh_incident_admissibility(
        &mut self,
        node: NodeIndex,
    ) -> Result<(), WeightedAugmentingPathsError> {
        let incident = self
            .graph
            .outgoing_edges(node)
            .iter()
            .chain(self.graph.incoming_edges(node))
            .copied()
            .collect::<Vec<_>>();
        for edge_index in incident {
            self.add_relabel_arc_inspections(2)?;
            let edge = self
                .graph
                .edge(edge_index)
                .ok_or(WeightedAugmentingPathsError::Invariant)?;
            let weight = self
                .hierarchy
                .as_ref()
                .and_then(|hierarchy| hierarchy.weights.get(edge_index.as_usize()))
                .copied()
                .ok_or(WeightedAugmentingPathsError::Invariant)?;
            if !self.labels[node.as_usize()].is_multiple_of(weight) {
                continue;
            }
            for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
                let id = ResidualArcId::new(edge.id().clone(), direction);
                let arc = self
                    .state
                    .arc(&id)
                    .ok_or(WeightedAugmentingPathsError::Invariant)?;
                let left = self.labels[arc.from.as_usize()];
                let right = self.labels[arc.to.as_usize()];
                let threshold = weight
                    .checked_mul(2)
                    .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
                let should_be = arc.capacity > 0 && left.saturating_sub(right) >= threshold;
                let ordinal = residual_ordinal(self.graph, &id)?;
                if self.admissible[ordinal] != should_be {
                    self.admissible[ordinal] = should_be;
                    self.metrics.admissible_updates = self
                        .metrics
                        .admissible_updates
                        .checked_add(1)
                        .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
                }
            }
        }
        Ok(())
    }

    fn add_relabel_arc_inspections(
        &mut self,
        count: u64,
    ) -> Result<(), WeightedAugmentingPathsError> {
        self.metrics.relabel_arc_inspections = self
            .metrics
            .relabel_arc_inspections
            .checked_add(count)
            .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
        Ok(())
    }

    fn trace_admissible_path(&self) -> Result<Vec<ResidualArcId>, WeightedAugmentingPathsError> {
        let mut node = self.source;
        let mut path = Vec::new();
        let mut seen = vec![false; self.graph.nodes().len()];
        while node != self.sink {
            if seen[node.as_usize()] {
                return Err(WeightedAugmentingPathsError::Invariant);
            }
            seen[node.as_usize()] = true;
            let mut selected = None;
            for arc in self.state.outgoing_arcs(node) {
                if self.admissible[residual_ordinal(self.graph, &arc.id)?] {
                    selected = Some(arc);
                    break;
                }
            }
            let arc = selected.ok_or(WeightedAugmentingPathsError::Invariant)?;
            if self.labels[arc.from.as_usize()] <= self.labels[arc.to.as_usize()] {
                return Err(WeightedAugmentingPathsError::Invariant);
            }
            node = arc.to;
            path.push(arc.id);
        }
        Ok(path)
    }

    fn emit(
        &mut self,
        stage: WeightedAugmentingPathsStage,
    ) -> Result<(), WeightedAugmentingPathsError> {
        let next_transition = self
            .metrics
            .state_transitions
            .checked_add(1)
            .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
        if usize::try_from(next_transition).map_or(true, |count| {
            count > WEIGHTED_AUGMENTING_PATHS_MAX_TRACE_EVENTS
        }) {
            return Err(WeightedAugmentingPathsError::WorkLimit);
        }
        self.metrics.state_transitions = next_transition;
        let after = self.snapshot(stage)?;
        validate_snapshot(self.graph, &after)?;
        if self.trace {
            self.events.push(WeightedAugmentingPathsTraceEvent {
                catalog_id: "weighted-augmenting-paths",
                before: self.current_snapshot.clone(),
                after: after.clone(),
            });
        }
        self.current_snapshot = after;
        Ok(())
    }

    fn snapshot(
        &self,
        stage: WeightedAugmentingPathsStage,
    ) -> Result<WeightedAugmentingPathsSnapshot, WeightedAugmentingPathsError> {
        let reachable = residual_reachable(&self.state, self.source);
        let hierarchy = self.hierarchy.as_ref();
        let active = &self.active_path;
        let nodes = self
            .graph
            .node_indices()
            .map(|node| WeightedAugmentingNodeState {
                node,
                component: hierarchy.map_or(0, |item| item.component[node.as_usize()]),
                order: hierarchy.map_or(0, |item| item.order[node.as_usize()]),
                label: self.labels[node.as_usize()],
                alive: self.alive[node.as_usize()],
                expansion_witness_side: hierarchy.is_some_and(|item| item.witness[node.as_usize()]),
                source_side: reachable[node.as_usize()],
            })
            .collect();
        let edges = self
            .graph
            .edges()
            .iter()
            .enumerate()
            .map(|(ordinal, edge)| WeightedAugmentingEdgeState {
                edge: edge.id().clone(),
                scaled_capacity: self.scaled_capacities[ordinal],
                flow: self.state.flows()[ordinal],
            })
            .collect();
        let mut residual_arcs = Vec::with_capacity(self.graph.edges().len() * 2);
        for (ordinal, edge) in self.graph.edges().iter().enumerate() {
            for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
                let id = ResidualArcId::new(edge.id().clone(), direction);
                let arc = self
                    .state
                    .arc(&id)
                    .ok_or(WeightedAugmentingPathsError::Invariant)?;
                let hierarchy_kind = hierarchy.map(|item| {
                    if item.component[arc.from.as_usize()] == item.component[arc.to.as_usize()] {
                        WeightedAugmentingHierarchyKind::Expanding
                    } else {
                        WeightedAugmentingHierarchyKind::Dag
                    }
                });
                residual_arcs.push(WeightedAugmentingResidualArcState {
                    id: id.clone(),
                    from: arc.from,
                    to: arc.to,
                    capacity: arc.capacity,
                    hierarchy_kind,
                    weight: hierarchy.map_or(0, |item| item.weights[ordinal]),
                    admissible: self.admissible[residual_ordinal(self.graph, &id)?],
                    active: active.contains(&id),
                });
            }
        }
        Ok(WeightedAugmentingPathsSnapshot {
            stage,
            phase: self.phase,
            phase_count: self.phase_count,
            capacity_bit: self.capacity_bit,
            round: self.round,
            height: self.height,
            phi_numerator: hierarchy.map_or(0, |item| item.phi.numerator),
            phi_denominator: hierarchy.map_or(1, |item| item.phi.denominator),
            active_bottleneck: self.active_bottleneck,
            nodes,
            edges,
            residual_arcs,
            active_path: active.clone(),
            metrics: self.metrics,
            certified_flows: self.certified_flows.clone(),
        })
    }
}

fn validate_input(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), WeightedAugmentingPathsError> {
    if graph.nodes().len() > WEIGHTED_AUGMENTING_PATHS_MAX_NODES
        || graph.edges().len() > WEIGHTED_AUGMENTING_PATHS_MAX_EDGES
        || graph
            .edges()
            .iter()
            .any(|edge| edge.capacity() > WEIGHTED_AUGMENTING_PATHS_MAX_CAPACITY)
    {
        return Err(WeightedAugmentingPathsError::AdmissionLimit);
    }
    if source == sink
        || graph.node(source).is_none()
        || graph.node(sink).is_none()
        || graph.nodes().iter().any(|node| node.supply() != 0)
        || graph
            .edges()
            .iter()
            .any(|edge| edge.lower() != 0 || edge.from() == edge.to() || edge.capacity() == 0)
    {
        return Err(WeightedAugmentingPathsError::GraphRequirement);
    }
    Ok(())
}

fn capacity_bit_count(graph: &FlowNetwork) -> usize {
    let maximum = graph
        .edges()
        .iter()
        .map(FlowEdge::capacity)
        .max()
        .unwrap_or(0);
    if maximum == 0 {
        1
    } else {
        usize::try_from(u64::BITS - maximum.leading_zeros()).unwrap_or(1)
    }
}

fn source_height(node_count: usize, phi: Ratio) -> Result<u64, WeightedAugmentingPathsError> {
    let n =
        u128::try_from(node_count).map_err(|_| WeightedAugmentingPathsError::ArithmeticOverflow)?;
    let log = u128::from(usize::BITS - node_count.max(2).saturating_sub(1).leading_zeros());
    let normalized = n
        .checked_mul(log)
        .and_then(|value| value.checked_mul(phi.denominator))
        .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
    let theorem_height = normalized
        .checked_add(phi.numerator - 1)
        .and_then(|value| value.checked_div(phi.numerator))
        .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
    let max_simple_path = node_count.saturating_sub(1).saturating_pow(2);
    let guard = max_simple_path.saturating_add(2) / 3;
    let guard =
        u128::try_from(guard).map_err(|_| WeightedAugmentingPathsError::ArithmeticOverflow)?;
    u64::try_from(theorem_height.max(guard).max(1))
        .map_err(|_| WeightedAugmentingPathsError::ArithmeticOverflow)
}

fn build_hierarchy(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: &mut WeightedAugmentingPathsMetrics,
) -> Result<Hierarchy, WeightedAugmentingPathsError> {
    metrics.hierarchy_builds = metrics
        .hierarchy_builds
        .checked_add(1)
        .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
    let component = residual_components(graph, state);
    let order = respecting_order(graph, state, &component)?;
    let weights = graph
        .edges()
        .iter()
        .map(|edge| {
            u64::try_from(order[edge.from().as_usize()].abs_diff(order[edge.to().as_usize()]))
                .ok()
                .filter(|&weight| weight > 0)
                .ok_or(WeightedAugmentingPathsError::Invariant)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (phi, witness) = certify_phi(graph, state, &component, metrics)?;
    Ok(Hierarchy {
        component,
        order,
        weights,
        phi,
        witness,
    })
}

fn residual_components(graph: &FlowNetwork, state: &ResidualState<'_>) -> Vec<usize> {
    let n = graph.nodes().len();
    let mut reach = vec![vec![false; n]; n];
    for start in graph.node_indices() {
        let mut queue = VecDeque::from([start]);
        reach[start.as_usize()][start.as_usize()] = true;
        while let Some(node) = queue.pop_front() {
            for arc in state.outgoing_arcs(node) {
                if !reach[start.as_usize()][arc.to.as_usize()] {
                    reach[start.as_usize()][arc.to.as_usize()] = true;
                    queue.push_back(arc.to);
                }
            }
        }
    }
    let mut component = vec![usize::MAX; n];
    let mut next = 0;
    for left in 0..n {
        if component[left] != usize::MAX {
            continue;
        }
        for right in left..n {
            if component[right] == usize::MAX && reach[left][right] && reach[right][left] {
                component[right] = next;
            }
        }
        next += 1;
    }
    component
}

fn respecting_order(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    component: &[usize],
) -> Result<Vec<usize>, WeightedAugmentingPathsError> {
    let component_count = component.iter().copied().max().map_or(0, |value| value + 1);
    let mut adjacency = vec![vec![false; component_count]; component_count];
    let mut indegree = vec![0; component_count];
    for node in graph.node_indices() {
        for arc in state.outgoing_arcs(node) {
            let from = component[arc.from.as_usize()];
            let to = component[arc.to.as_usize()];
            if from != to && !adjacency[from][to] {
                adjacency[from][to] = true;
                indegree[to] += 1;
            }
        }
    }
    let mut ready = BinaryHeap::new();
    for (ordinal, &degree) in indegree.iter().enumerate() {
        if degree == 0 {
            ready.push(Reverse(ordinal));
        }
    }
    let mut components = Vec::with_capacity(component_count);
    while let Some(Reverse(current)) = ready.pop() {
        components.push(current);
        for next in 0..component_count {
            if adjacency[current][next] {
                indegree[next] -= 1;
                if indegree[next] == 0 {
                    ready.push(Reverse(next));
                }
            }
        }
    }
    if components.len() != component_count {
        return Err(WeightedAugmentingPathsError::Invariant);
    }
    let mut order = vec![0; graph.nodes().len()];
    let mut next_order = 1;
    for current in components {
        for node in graph.node_indices() {
            if component[node.as_usize()] == current {
                order[node.as_usize()] = next_order;
                next_order += 1;
            }
        }
    }
    Ok(order)
}

fn certify_phi(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    component: &[usize],
    metrics: &mut WeightedAugmentingPathsMetrics,
) -> Result<(Ratio, Vec<bool>), WeightedAugmentingPathsError> {
    let n = graph.nodes().len();
    let component_count = component.iter().copied().max().map_or(0, |value| value + 1);
    let arcs = positive_residual_arcs(graph, state);
    let mut best = None;
    let mut best_witness = vec![false; n];
    for current in 0..component_count {
        let members = (0..n)
            .filter(|&node| component[node] == current)
            .collect::<Vec<_>>();
        if members.len() < 2 {
            continue;
        }
        let subset_count = 1_usize
            .checked_shl(
                u32::try_from(members.len())
                    .map_err(|_| WeightedAugmentingPathsError::ArithmeticOverflow)?,
            )
            .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
        for mask in 1..subset_count - 1 {
            metrics.hierarchy_cuts = metrics
                .hierarchy_cuts
                .checked_add(1)
                .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
            if metrics.hierarchy_cuts > WEIGHTED_AUGMENTING_PATHS_MAX_CUTS {
                return Err(WeightedAugmentingPathsError::WorkLimit);
            }
            let mut side = vec![false; n];
            for (position, &node) in members.iter().enumerate() {
                side[node] = mask & (1 << position) != 0;
            }
            let mut out = 0_u128;
            let mut incoming = 0_u128;
            let mut volume_side = 0_u128;
            let mut volume_other = 0_u128;
            for arc in &arcs {
                if component[arc.from.as_usize()] != current
                    || component[arc.to.as_usize()] != current
                {
                    continue;
                }
                let capacity = u128::from(arc.capacity);
                let from_side = side[arc.from.as_usize()];
                let to_side = side[arc.to.as_usize()];
                if from_side && !to_side {
                    out = out
                        .checked_add(capacity)
                        .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
                } else if !from_side && to_side {
                    incoming = incoming
                        .checked_add(capacity)
                        .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
                }
                if from_side {
                    volume_side = volume_side
                        .checked_add(capacity)
                        .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
                } else {
                    volume_other = volume_other
                        .checked_add(capacity)
                        .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
                }
                if to_side {
                    volume_side = volume_side
                        .checked_add(capacity)
                        .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
                } else {
                    volume_other = volume_other
                        .checked_add(capacity)
                        .ok_or(WeightedAugmentingPathsError::ArithmeticOverflow)?;
                }
            }
            let denominator = volume_side.min(volume_other);
            if denominator == 0 {
                continue;
            }
            let ratio = Ratio::new(out.min(incoming), denominator)?;
            let improves = match best {
                Some(current) => ratio.less_than(current)?,
                None => true,
            };
            if improves {
                best = Some(ratio);
                best_witness = side;
            }
        }
    }
    Ok((best.unwrap_or(Ratio::ONE), best_witness))
}

fn positive_residual_arcs(graph: &FlowNetwork, state: &ResidualState<'_>) -> Vec<ResidualArc> {
    graph
        .node_indices()
        .flat_map(|node| state.outgoing_arcs(node))
        .collect()
}

fn residual_reachable(state: &ResidualState<'_>, source: NodeIndex) -> Vec<bool> {
    let mut reachable = vec![false; state.graph().nodes().len()];
    let mut queue = VecDeque::from([source]);
    reachable[source.as_usize()] = true;
    while let Some(node) = queue.pop_front() {
        for arc in state.outgoing_arcs(node) {
            if !reachable[arc.to.as_usize()] {
                reachable[arc.to.as_usize()] = true;
                queue.push_back(arc.to);
            }
        }
    }
    reachable
}

fn residual_ordinal(
    graph: &FlowNetwork,
    id: &ResidualArcId,
) -> Result<usize, WeightedAugmentingPathsError> {
    let edge = graph
        .edge_index(id.original_edge())
        .ok_or(WeightedAugmentingPathsError::Invariant)?
        .as_usize();
    Ok(edge * 2 + usize::from(matches!(id.direction(), ResidualDirection::Reverse)))
}

fn empty_snapshot(graph: &FlowNetwork, phase_count: usize) -> WeightedAugmentingPathsSnapshot {
    WeightedAugmentingPathsSnapshot {
        stage: WeightedAugmentingPathsStage::Ready,
        phase: 0,
        phase_count,
        capacity_bit: phase_count.saturating_sub(1),
        round: 0,
        height: 0,
        phi_numerator: 0,
        phi_denominator: 1,
        active_bottleneck: 0,
        nodes: graph
            .node_indices()
            .map(|node| WeightedAugmentingNodeState {
                node,
                component: 0,
                order: 0,
                label: 0,
                alive: true,
                expansion_witness_side: false,
                source_side: false,
            })
            .collect(),
        edges: graph
            .edges()
            .iter()
            .map(|edge| WeightedAugmentingEdgeState {
                edge: edge.id().clone(),
                scaled_capacity: 0,
                flow: 0,
            })
            .collect(),
        residual_arcs: graph
            .edges()
            .iter()
            .flat_map(|edge| {
                [ResidualDirection::Forward, ResidualDirection::Reverse].map(|direction| {
                    let (from, to) = if matches!(direction, ResidualDirection::Forward) {
                        (edge.from(), edge.to())
                    } else {
                        (edge.to(), edge.from())
                    };
                    WeightedAugmentingResidualArcState {
                        id: ResidualArcId::new(edge.id().clone(), direction),
                        from,
                        to,
                        capacity: 0,
                        hierarchy_kind: None,
                        weight: 0,
                        admissible: false,
                        active: false,
                    }
                })
            })
            .collect(),
        active_path: Vec::new(),
        metrics: WeightedAugmentingPathsMetrics::default(),
        certified_flows: None,
    }
}

fn validate_snapshot(
    graph: &FlowNetwork,
    snapshot: &WeightedAugmentingPathsSnapshot,
) -> Result<(), WeightedAugmentingPathsError> {
    if snapshot.nodes.len() != graph.nodes().len()
        || snapshot.edges.len() != graph.edges().len()
        || snapshot.residual_arcs.len() != graph.edges().len() * 2
        || snapshot.phi_denominator == 0
        || snapshot.phase_count == 0
        || snapshot.phase >= snapshot.phase_count
        || snapshot.capacity_bit >= snapshot.phase_count
    {
        return Err(WeightedAugmentingPathsError::TraceVerification);
    }
    if snapshot
        .nodes
        .iter()
        .enumerate()
        .any(|(ordinal, node)| node.node.as_usize() != ordinal)
    {
        return Err(WeightedAugmentingPathsError::TraceVerification);
    }
    for (ordinal, (edge, state)) in graph.edges().iter().zip(&snapshot.edges).enumerate() {
        if state.edge != *edge.id()
            || state.scaled_capacity > edge.capacity()
            || state.flow > state.scaled_capacity
        {
            return Err(WeightedAugmentingPathsError::TraceVerification);
        }
        for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
            let residual = &snapshot.residual_arcs
                [ordinal * 2 + usize::from(matches!(direction, ResidualDirection::Reverse))];
            let expected_id = ResidualArcId::new(edge.id().clone(), direction);
            let expected_capacity = if matches!(direction, ResidualDirection::Forward) {
                state.scaled_capacity - state.flow
            } else {
                state.flow
            };
            if residual.id != expected_id || residual.capacity != expected_capacity {
                return Err(WeightedAugmentingPathsError::TraceVerification);
            }
        }
    }
    if snapshot.active_bottleneck == 0 && !snapshot.active_path.is_empty() {
        return Err(WeightedAugmentingPathsError::TraceVerification);
    }
    if snapshot.certified_flows.is_some()
        != matches!(snapshot.stage, WeightedAugmentingPathsStage::Optimal)
    {
        return Err(WeightedAugmentingPathsError::TraceVerification);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;

    fn graph() -> (FlowNetwork, NodeIndex, NodeIndex) {
        let nodes = ["s", "a", "b", "t"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
            .collect();
        let edge = |id: &str, from: &str, to: &str, capacity| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge"),
            from: NodeId::parse(from).expect("tail"),
            to: NodeId::parse(to).expect("head"),
            lower: 0,
            capacity,
            cost: 0,
        };
        let graph = FlowNetwork::new(
            nodes,
            vec![
                edge("e1", "s", "a", 7),
                edge("e2", "s", "b", 4),
                edge("e3", "a", "b", 3),
                edge("e4", "a", "t", 4),
                edge("e5", "b", "t", 6),
                edge("e6", "b", "a", 2),
            ],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source"))
            .expect("s");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink"))
            .expect("t");
        (graph, source, sink)
    }

    #[test]
    fn capacity_scaling_weighted_paths_certify_exact_flow() {
        let (graph, source, sink) = graph();
        let result = solve_weighted_augmenting_paths(&graph, source, sink).expect("solve");
        assert_eq!(result.certificate.value, 10);
        assert!(result.metrics.capacity_phases >= 3);
        assert!(result.metrics.hierarchy_builds > 0);
        assert!(result.metrics.relabel_jumps > 0);
        assert!(result.metrics.augmentations > 0);
        assert_eq!(
            result.final_snapshot.stage,
            WeightedAugmentingPathsStage::Optimal
        );
    }

    #[test]
    fn trace_reexecutes_every_boundary() {
        let (graph, source, sink) = graph();
        let trace = trace_weighted_augmenting_paths(&graph, source, sink).expect("trace");
        verify_weighted_augmenting_paths_trace(&graph, source, sink, &trace).expect("verify");
        assert!(
            trace
                .events
                .iter()
                .any(|event| { event.after.stage == WeightedAugmentingPathsStage::BuildHierarchy })
        );
        assert!(
            trace.events.iter().any(|event| {
                event.after.stage == WeightedAugmentingPathsStage::CertifyExpansion
            })
        );
        assert!(
            trace
                .events
                .iter()
                .any(|event| { event.after.stage == WeightedAugmentingPathsStage::AugmentPath })
        );
        let relabel_jumps = trace
            .events
            .iter()
            .filter(|event| event.after.stage == WeightedAugmentingPathsStage::RelabelSweep)
            .collect::<Vec<_>>();
        assert_eq!(
            u64::try_from(relabel_jumps.len()).expect("relabel event count"),
            trace.result.metrics.relabel_jumps
        );
        assert!(relabel_jumps.iter().all(|event| {
            event
                .before
                .nodes
                .iter()
                .zip(&event.after.nodes)
                .filter(|(before, after)| before != after)
                .count()
                == 1
        }));
    }

    #[test]
    fn mutated_trace_is_rejected() {
        let (graph, source, sink) = graph();
        let mut trace = trace_weighted_augmenting_paths(&graph, source, sink).expect("trace");
        trace.events[0].after.metrics.capacity_phases += 1;
        assert_eq!(
            verify_weighted_augmenting_paths_trace(&graph, source, sink, &trace),
            Err(WeightedAugmentingPathsError::TraceVerification)
        );
    }

    #[test]
    fn unsupported_lower_bounds_are_rejected() {
        let (graph, source, sink) = graph();
        let mut edges = graph
            .edges()
            .iter()
            .map(|edge| UnresolvedFlowEdge {
                id: edge.id().clone(),
                from: graph.node(edge.from()).expect("from").id().clone(),
                to: graph.node(edge.to()).expect("to").id().clone(),
                lower: u64::from(edge.id().as_str() == "e1"),
                capacity: edge.capacity(),
                cost: edge.cost(),
            })
            .collect::<Vec<_>>();
        edges.sort_by(|left, right| left.id.cmp(&right.id));
        let nodes = graph.nodes().to_vec();
        let invalid = FlowNetwork::new(nodes, edges).expect("valid model");
        assert_eq!(
            solve_weighted_augmenting_paths(&invalid, source, sink),
            Err(WeightedAugmentingPathsError::GraphRequirement)
        );
    }
}
