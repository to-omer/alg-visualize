//! Exact resumable normalized forest for canonical parametric pseudoflow.
//!
//! Kept private until forward/reverse free runs and contraction recursion are
//! complete. Quantities remain in one `BigRational` unit across parameters.

use std::collections::{BTreeMap, BTreeSet};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use thiserror::Error;

use crate::algorithms::parametric_breakpoint_rerun::{
    ParametricBreakpoint, ParametricBreakpointRerunResult, ParametricCut, ParametricMaxFlowProblem,
    ParametricRational, ParametricSegment, certify_parametric_result,
    solve_parametric_breakpoint_rerun,
};
use crate::model::{EdgeId, FlowNetwork, NodeId, NodeIndex};

const MAX_RESIDUAL_SCANS: u128 = 10_000_000;
const MAX_TRANSITIONS: u64 = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Orientation {
    Forward,
    Reverse,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ExactDirection {
    Forward,
    Reverse,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ExactResidualId {
    arc: usize,
    direction: ExactDirection,
}

#[derive(Clone, Debug)]
struct ExactArc {
    original_edge: usize,
    stable_id: EdgeId,
    from: NodeIndex,
    to: NodeIndex,
    capacity: BigRational,
    flow: BigRational,
}

#[derive(Clone, Debug)]
struct ExactResidualArc {
    id: ExactResidualId,
    from: NodeIndex,
    to: NodeIndex,
    capacity: BigRational,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct WarmMetrics {
    initializations: u64,
    parameter_advances: u64,
    forest_reuses: u64,
    renormalization_pushes: u64,
    renormalization_splits: u64,
    mergers: u64,
    relabels: u64,
    normalization_pushes: u64,
    normalization_splits: u64,
    residual_arc_scans: u128,
    transitions: u64,
}

const PARAMETRIC_PSEUDOFLOW_TRACE_SCAN_PREFIX: u128 = 512;

#[derive(Clone, Debug)]
struct WarmScanCheckpoint {
    edge: EdgeId,
    metrics: WarmMetrics,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
enum WarmPseudoflowError {
    #[error("parametric pseudoflow work limit reached")]
    WorkLimit,
    #[error("parameter advance violates the retained orientation")]
    NonMonotoneAdvance,
    #[error("exact parametric capacity is negative")]
    NegativeCapacity,
    #[error("normalized-forest invariant failed")]
    Invariant,
}

/// Aggregate work observed while independently replaying a complete analysis
/// with retained exact forward and reverse normalized forests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParametricWarmVerificationMetrics {
    /// Monotone parameter changes across both orientations.
    pub parameter_advances: u64,
    /// Parameter changes that retained the existing forest and labels.
    pub forest_reuses: u64,
    /// Appendix-B balance pushes across retained tree arcs.
    pub renormalization_pushes: u64,
    /// Appendix-B splits forced by an exhausted parent residual.
    pub renormalization_splits: u64,
    /// Strong-to-weak branch mergers after all parameter changes.
    pub mergers: u64,
    /// Label increases after all parameter changes.
    pub relabels: u64,
    /// Exact intersection/midpoint probes executed as paired free-run races.
    pub free_run_races: u64,
    /// Races in which the forward retained state reached optimality first.
    pub forward_race_wins: u64,
    /// Races in which the reverse retained state reached optimality first.
    pub reverse_race_wins: u64,
    /// One-merger transitions executed cooperatively during free-run races.
    pub cooperative_race_steps: u64,
}

/// A retained-forest replay contradicted the certified cold complete analysis.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ParametricWarmVerificationError {
    /// The supplied complete analysis has invalid coverage or source sets.
    #[error("invalid complete-analysis result for warm verification")]
    InvalidResult,
    /// Exact retained normalized-forest execution violated an invariant.
    #[error("exact warm parametric pseudoflow verification failed")]
    Kernel,
}

/// Exact counters for the explicit-tree canonical parametric traversal.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParametricPseudoflowMetrics {
    /// Fresh forward or reverse normalized-forest initializations.
    pub forest_initializations: u64,
    /// Monotone capacity retarget operations.
    pub parameter_advances: u64,
    /// Retargets that retained an existing forest and labels.
    pub forest_reuses: u64,
    /// Appendix-B pushes used to restore nonroot balance.
    pub renormalization_pushes: u64,
    /// Appendix-B splits caused by an exhausted parent residual.
    pub renormalization_splits: u64,
    /// Labeling-pseudoflow mergers across all nonduplicated work.
    pub mergers: u64,
    /// Label increases across all nonduplicated work.
    pub relabels: u64,
    /// Forward/reverse cooperative races.
    pub free_run_races: u64,
    /// Races completed first by the forward state.
    pub forward_race_wins: u64,
    /// Races completed first by the reverse state.
    pub reverse_race_wins: u64,
    /// One-merger cooperative race steps.
    pub cooperative_race_steps: u64,
    /// Logical source/sink contraction child views created.
    pub contraction_views: u64,
    /// Smaller recursive children restarted with two fresh forests.
    pub smaller_child_restarts: u64,
    /// Larger recursive children that retained parent checkpoints.
    pub larger_child_continuations: u64,
    /// Maximum exact-intersection recursion depth.
    pub maximum_depth: u64,
    /// Residual arcs inspected by every retained normalized-forest run.
    pub residual_arc_scans: u128,
}

/// Complete exact source-set analysis produced by retained forward/reverse
/// normalized forests and logical contraction recursion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParametricPseudoflowResult {
    /// Closed value regions with open-interior source-cut extrema.
    pub segments: Vec<ParametricSegment>,
    /// Exact source-set transitions.
    pub breakpoints: Vec<ParametricBreakpoint>,
    /// Exact implementation counters.
    pub metrics: ParametricPseudoflowMetrics,
}

/// Public orientation of one retained normalized forest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParametricTraversalOrientation {
    /// Increasing parameter on the original graph.
    Forward,
    /// Decreasing parameter on the reversed graph.
    Reverse,
}

/// First retained state to reach optimality in a cooperative free-run race.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParametricRaceWinner {
    /// The increasing original-graph state finished first.
    Forward,
    /// The decreasing reversed-graph state finished first.
    Reverse,
}

/// Closed semantic event kinds for exact parametric playback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParametricPseudoflowEventKind {
    /// Initialize a fresh exact normalized forest.
    InitializeForest,
    /// Inspect one exact residual arc while building a normalized forest.
    InspectResidualArc,
    /// Retarget two retained endpoint forests and race them cooperatively.
    FreeRunRace,
    /// Create tie-safe logical source/sink contraction views.
    CreateContractionViews,
    /// Restart the smaller child with two fresh forests.
    RestartSmallerChild,
    /// Continue the larger child from parent checkpoints.
    ContinueLargerChild,
    /// Record one certified open-interior value region.
    RecordSegment,
    /// Publish one exact nested source-set transition.
    RecordBreakpoint,
    /// Complete independent cold-oracle certification.
    Optimal,
}

/// One exact semantic state transition in canonical parametric traversal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParametricPseudoflowTraceEvent {
    /// One-based stable event identity.
    pub event_id: u64,
    /// Closed event kind.
    pub kind: ParametricPseudoflowEventKind,
    /// Current closed subproblem lower endpoint.
    pub lower: ParametricRational,
    /// Current closed subproblem upper endpoint.
    pub upper: ParametricRational,
    /// Selected exact probe or breakpoint, when applicable.
    pub parameter: Option<ParametricRational>,
    /// Forest orientation for a single-forest event.
    pub orientation: Option<ParametricTraversalOrientation>,
    /// Exact original edge inspected by a residual-scan Detail.
    pub inspected_edge: Option<EdgeId>,
    /// First finisher for a paired race.
    pub race_winner: Option<ParametricRaceWinner>,
    /// Whether the event retained a real normalized forest.
    pub normalized_tree_reused: bool,
    /// Whether preexisting labels were retained without decrease.
    pub labels_retained: bool,
    /// Active vertices before a contraction split.
    pub active_nodes: Option<u64>,
    /// Exact active graph vertices owned by this forest/subproblem operation.
    pub active_node_ids: Vec<NodeIndex>,
    /// Active vertices in the left child view.
    pub left_active_nodes: Option<u64>,
    /// Active vertices in the right child view.
    pub right_active_nodes: Option<u64>,
    /// Appendix-B pushes caused by this event.
    pub renormalization_pushes: u64,
    /// Appendix-B splits caused by this event.
    pub renormalization_splits: u64,
    /// Metrics after this event.
    pub metrics: ParametricPseudoflowMetrics,
}

/// Canonical exact result with deterministic semantic traversal events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParametricPseudoflowTraceResult {
    /// Same certified result returned by the fast profile.
    pub result: ParametricPseudoflowResult,
    /// Depth-first forward/reverse race and contraction traversal.
    pub events: Vec<ParametricPseudoflowTraceEvent>,
}

/// Canonical parametric pseudoflow traversal failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ParametricPseudoflowError {
    /// Exact forest execution violated a normalized-tree invariant.
    #[error("parametric pseudoflow normalized-forest execution failed")]
    Forest,
    /// Exact intersection recursion or contraction view was inconsistent.
    #[error("parametric pseudoflow recursion invariant failed")]
    Invariant,
    /// The independent cold complete-analysis oracle disagreed.
    #[error("parametric pseudoflow independent certificate failed")]
    Certificate,
    /// A bounded work or counter limit was exceeded.
    #[error("parametric pseudoflow work limit reached")]
    WorkLimit,
}

#[derive(Clone)]
struct WarmPseudoflowState<'graph> {
    graph: &'graph FlowNetwork,
    problem: &'graph ParametricMaxFlowProblem,
    orientation: Orientation,
    source: NodeIndex,
    sink: NodeIndex,
    parameter: ParametricRational,
    arcs: Vec<ExactArc>,
    parent: Vec<Option<NodeIndex>>,
    down_arc: Vec<Option<ExactResidualId>>,
    labels: Vec<Option<u32>>,
    metrics: WarmMetrics,
    optimal: bool,
    scan_checkpoints: Vec<WarmScanCheckpoint>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RaceWinner {
    Forward,
    Reverse,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RaceMetrics {
    races: u64,
    forward_wins: u64,
    reverse_wins: u64,
    cooperative_steps: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WarmCutState {
    membership: Vec<bool>,
    intercept: BigInt,
    slope: BigInt,
}

#[derive(Clone, Debug)]
struct ContractionView {
    fixed_source: Vec<bool>,
    fixed_sink: Vec<bool>,
}

impl ContractionView {
    fn root(graph: &FlowNetwork, problem: &ParametricMaxFlowProblem) -> Self {
        let mut fixed_source = vec![false; graph.nodes().len()];
        let mut fixed_sink = vec![false; graph.nodes().len()];
        fixed_source[problem.source().as_usize()] = true;
        fixed_sink[problem.sink().as_usize()] = true;
        Self {
            fixed_source,
            fixed_sink,
        }
    }

    fn left_child(&self, exact_maximal: &[bool]) -> Result<Self, ParametricPseudoflowError> {
        let mut child = self.clone();
        for (fixed, in_maximal) in child.fixed_sink.iter_mut().zip(exact_maximal) {
            *fixed |= !*in_maximal;
        }
        child.validate()?;
        Ok(child)
    }

    fn right_child(&self, exact_minimal: &[bool]) -> Result<Self, ParametricPseudoflowError> {
        let mut child = self.clone();
        for (fixed, in_minimal) in child.fixed_source.iter_mut().zip(exact_minimal) {
            *fixed |= *in_minimal;
        }
        child.validate()?;
        Ok(child)
    }

    fn active_count(&self) -> usize {
        self.fixed_source
            .iter()
            .zip(&self.fixed_sink)
            .filter(|(source, sink)| !**source && !**sink)
            .count()
    }

    fn active_node_ids(&self) -> Result<Vec<NodeIndex>, ParametricPseudoflowError> {
        self.fixed_source
            .iter()
            .zip(&self.fixed_sink)
            .enumerate()
            .filter_map(|(index, (source, sink))| {
                (!*source && !*sink).then_some(
                    NodeIndex::try_from_usize(index).ok_or(ParametricPseudoflowError::Invariant),
                )
            })
            .collect()
    }

    fn accepts(&self, source_side: &[bool]) -> bool {
        source_side.len() == self.fixed_source.len()
            && self
                .fixed_source
                .iter()
                .zip(source_side)
                .all(|(fixed, present)| !*fixed || *present)
            && self
                .fixed_sink
                .iter()
                .zip(source_side)
                .all(|(fixed, present)| !*fixed || !*present)
    }

    fn validate(&self) -> Result<(), ParametricPseudoflowError> {
        if self.fixed_source.len() != self.fixed_sink.len()
            || self
                .fixed_source
                .iter()
                .zip(&self.fixed_sink)
                .any(|(source, sink)| *source && *sink)
        {
            return Err(ParametricPseudoflowError::Invariant);
        }
        Ok(())
    }
}

struct RaceOutcome<'graph> {
    forward: WarmPseudoflowState<'graph>,
    reverse: WarmPseudoflowState<'graph>,
    minimal: Vec<bool>,
    maximal: Vec<bool>,
    winner: RaceWinner,
    steps: u64,
    forward_delta: WarmMetrics,
    reverse_delta: WarmMetrics,
    scan_checkpoints: Vec<RaceScanCheckpoint>,
}

struct RaceScanCheckpoint {
    orientation: ParametricTraversalOrientation,
    edge: EdgeId,
    forward_delta: WarmMetrics,
    reverse_delta: WarmMetrics,
}

struct ParametricAnalyzer<'graph> {
    graph: &'graph FlowNetwork,
    problem: &'graph ParametricMaxFlowProblem,
    segments: Vec<ParametricSegment>,
    exact_extrema: BTreeMap<ParametricRational, (Vec<bool>, Vec<bool>)>,
    metrics: ParametricPseudoflowMetrics,
    events: Vec<ParametricPseudoflowTraceEvent>,
    record_trace: bool,
}

struct ParametricInternalRun {
    result: ParametricPseudoflowResult,
    events: Vec<ParametricPseudoflowTraceEvent>,
}

struct TraceEventInput {
    kind: ParametricPseudoflowEventKind,
    lower: ParametricRational,
    upper: ParametricRational,
    parameter: Option<ParametricRational>,
    orientation: Option<ParametricTraversalOrientation>,
    race_winner: Option<ParametricRaceWinner>,
    normalized_tree_reused: bool,
    labels_retained: bool,
    active_nodes: Option<u64>,
    active_node_ids: Vec<NodeIndex>,
    left_active_nodes: Option<u64>,
    right_active_nodes: Option<u64>,
    renormalization_pushes: u64,
    renormalization_splits: u64,
}

impl<'graph> WarmPseudoflowState<'graph> {
    fn new_forward(
        graph: &'graph FlowNetwork,
        problem: &'graph ParametricMaxFlowProblem,
        parameter: ParametricRational,
    ) -> Result<Self, WarmPseudoflowError> {
        Self::new(graph, problem, parameter, Orientation::Forward)
    }

    fn new_reverse(
        graph: &'graph FlowNetwork,
        problem: &'graph ParametricMaxFlowProblem,
        parameter: ParametricRational,
    ) -> Result<Self, WarmPseudoflowError> {
        Self::new(graph, problem, parameter, Orientation::Reverse)
    }

    fn new(
        graph: &'graph FlowNetwork,
        problem: &'graph ParametricMaxFlowProblem,
        parameter: ParametricRational,
        orientation: Orientation,
    ) -> Result<Self, WarmPseudoflowError> {
        let (source, sink) = match orientation {
            Orientation::Forward => (problem.source(), problem.sink()),
            Orientation::Reverse => (problem.sink(), problem.source()),
        };
        let mut arcs = Vec::with_capacity(graph.edges().len());
        for (position, edge) in graph.edges().iter().enumerate() {
            let (from, to) = match orientation {
                Orientation::Forward => (edge.from(), edge.to()),
                Orientation::Reverse => (edge.to(), edge.from()),
            };
            arcs.push(ExactArc {
                original_edge: position,
                stable_id: edge.id().clone(),
                from,
                to,
                capacity: exact_capacity(graph, problem, position, &parameter)?,
                flow: BigRational::zero(),
            });
        }
        arcs.sort_by(|left, right| left.stable_id.cmp(&right.stable_id));
        let mut labels = vec![Some(1); graph.nodes().len()];
        labels[source.as_usize()] = None;
        labels[sink.as_usize()] = None;
        let mut state = Self {
            graph,
            problem,
            orientation,
            source,
            sink,
            parameter,
            arcs,
            parent: vec![None; graph.nodes().len()],
            down_arc: vec![None; graph.nodes().len()],
            labels,
            metrics: WarmMetrics {
                initializations: 1,
                ..WarmMetrics::default()
            },
            optimal: false,
            scan_checkpoints: Vec::new(),
        };
        state.saturate_boundary_arcs();
        state.count_transition()?;
        state.validate_normalized()?;
        Ok(state)
    }

    fn run_to_optimal(&mut self) -> Result<Vec<bool>, WarmPseudoflowError> {
        while !self.step_labeling()? {
            // One deterministic merger per step keeps the state resumable.
        }
        Ok(self.strong_membership())
    }

    fn step_labeling(&mut self) -> Result<bool, WarmPseudoflowError> {
        if self.optimal {
            return Ok(true);
        }
        let Some(merger) = self.select_merger()? else {
            let membership = self.strong_membership();
            self.validate_optimal_partition(&membership)?;
            self.optimal = true;
            return Ok(true);
        };
        let strong = self.strong_nodes();
        self.relabel_strong_set(&merger, &strong)?;
        let path = self.merge_path(&merger)?;
        let root = self.root(merger.from)?;
        let excess = self.excesses()[root.as_usize()].clone();
        if !excess.is_positive() {
            return Err(WarmPseudoflowError::Invariant);
        }
        self.invert_and_attach(&merger)?;
        self.metrics.mergers = self
            .metrics
            .mergers
            .checked_add(1)
            .ok_or(WarmPseudoflowError::WorkLimit)?;
        self.count_transition()?;
        self.normalize_merger_path(&path, excess)?;
        self.validate_normalized()?;
        Ok(false)
    }

    fn prepare_parameter(
        &mut self,
        next: ParametricRational,
    ) -> Result<Vec<bool>, WarmPseudoflowError> {
        if !self.optimal {
            return Err(WarmPseudoflowError::Invariant);
        }
        let monotone = match self.orientation {
            Orientation::Forward => next >= self.parameter,
            Orientation::Reverse => next <= self.parameter,
        };
        if !monotone {
            return Err(WarmPseudoflowError::NonMonotoneAdvance);
        }
        let previous_strong = self.strong_membership();
        if next == self.parameter {
            return Ok(previous_strong);
        }
        let previous_labels = self.labels.clone();
        for arc in &mut self.arcs {
            let capacity = exact_capacity(self.graph, self.problem, arc.original_edge, &next)?;
            let boundary = (arc.from == self.source && arc.to != self.source)
                || (arc.to == self.sink && arc.from != self.sink);
            if boundary {
                arc.capacity = capacity.clone();
                arc.flow = capacity;
            } else if capacity != arc.capacity || arc.flow > capacity {
                return Err(WarmPseudoflowError::Invariant);
            }
        }
        self.parameter = next;
        self.optimal = false;
        self.metrics.parameter_advances = self
            .metrics
            .parameter_advances
            .checked_add(1)
            .ok_or(WarmPseudoflowError::WorkLimit)?;
        self.metrics.forest_reuses = self
            .metrics
            .forest_reuses
            .checked_add(1)
            .ok_or(WarmPseudoflowError::WorkLimit)?;
        self.count_transition()?;
        self.renormalize_after_terminal_update()?;
        if self
            .labels
            .iter()
            .zip(&previous_labels)
            .any(|(next, previous)| match (next, previous) {
                (Some(next), Some(previous)) => next < previous,
                (None, None) => false,
                _ => true,
            })
        {
            return Err(WarmPseudoflowError::Invariant);
        }
        Ok(previous_strong)
    }

    fn finish_prepared_parameter(
        &mut self,
        previous_strong: &[bool],
    ) -> Result<Vec<bool>, WarmPseudoflowError> {
        let next_strong = self.run_to_optimal()?;
        ensure_subset(previous_strong, &next_strong)?;
        Ok(next_strong)
    }

    fn run_prepared_to_optimal(
        &mut self,
        previous_strong: &[bool],
    ) -> Result<Vec<bool>, WarmPseudoflowError> {
        self.finish_prepared_parameter(previous_strong)
    }

    fn advance_parameter(
        &mut self,
        next: ParametricRational,
    ) -> Result<Vec<bool>, WarmPseudoflowError> {
        let previous_strong = self.prepare_parameter(next)?;
        self.run_prepared_to_optimal(&previous_strong)
    }

    fn maximal_source_membership(&self) -> Result<Vec<bool>, WarmPseudoflowError> {
        if self.orientation != Orientation::Reverse || !self.optimal {
            return Err(WarmPseudoflowError::Invariant);
        }
        let mut source_side = self
            .strong_membership()
            .into_iter()
            .map(|present| !present)
            .collect::<Vec<_>>();
        source_side[self.problem.source().as_usize()] = true;
        source_side[self.problem.sink().as_usize()] = false;
        Ok(source_side)
    }

    fn saturate_boundary_arcs(&mut self) {
        for arc in &mut self.arcs {
            if (arc.from == self.source && arc.to != self.source)
                || (arc.to == self.sink && arc.from != self.sink)
            {
                arc.flow = arc.capacity.clone();
            }
        }
    }

    fn renormalize_after_terminal_update(&mut self) -> Result<(), WarmPseudoflowError> {
        let mut order = self.internal_nodes().collect::<Vec<_>>();
        order.sort_by(|left, right| {
            self.depth(*right)
                .unwrap_or(usize::MAX)
                .cmp(&self.depth(*left).unwrap_or(usize::MAX))
                .then_with(|| left.cmp(right))
        });
        let mut excesses = self.excesses();
        for node in order {
            let Some(parent) = self.parent[node.as_usize()] else {
                continue;
            };
            let down = self.down_arc[node.as_usize()].ok_or(WarmPseudoflowError::Invariant)?;
            let excess = excesses[node.as_usize()].clone();
            if excess.is_zero() {
                if self.residual(down)?.capacity.is_zero() {
                    self.split_branch(node)?;
                }
                continue;
            }
            if excess.is_positive() {
                let upward = Self::reverse_residual(down);
                let pushed = minimum(&excess, &self.residual(upward)?.capacity);
                if pushed.is_positive() {
                    self.augment(upward, &pushed)?;
                    excesses[node.as_usize()] -= pushed.clone();
                    excesses[parent.as_usize()] += pushed;
                    self.count_renormalization_push()?;
                }
                if excesses[node.as_usize()].is_positive() {
                    self.split_branch(node)?;
                }
            } else {
                let needed = -excess;
                let pushed = minimum(&needed, &self.residual(down)?.capacity);
                if pushed.is_positive() {
                    self.augment(down, &pushed)?;
                    excesses[parent.as_usize()] -= pushed.clone();
                    excesses[node.as_usize()] += pushed;
                    self.count_renormalization_push()?;
                }
                if excesses[node.as_usize()].is_negative()
                    || self.residual(down)?.capacity.is_zero()
                {
                    self.split_branch(node)?;
                }
            }
        }
        if excesses != self.excesses() {
            return Err(WarmPseudoflowError::Invariant);
        }
        self.validate_normalized()
    }

    fn count_renormalization_push(&mut self) -> Result<(), WarmPseudoflowError> {
        self.metrics.renormalization_pushes = self
            .metrics
            .renormalization_pushes
            .checked_add(1)
            .ok_or(WarmPseudoflowError::WorkLimit)?;
        self.count_transition()
    }

    fn split_branch(&mut self, node: NodeIndex) -> Result<(), WarmPseudoflowError> {
        if self.parent[node.as_usize()].take().is_none() {
            return Err(WarmPseudoflowError::Invariant);
        }
        self.down_arc[node.as_usize()] = None;
        self.metrics.renormalization_splits = self
            .metrics
            .renormalization_splits
            .checked_add(1)
            .ok_or(WarmPseudoflowError::WorkLimit)?;
        self.count_transition()
    }

    fn relabel_strong_set(
        &mut self,
        merger: &ExactResidualArc,
        strong_nodes: &[NodeIndex],
    ) -> Result<(), WarmPseudoflowError> {
        let new_label = self.labels[merger.to.as_usize()]
            .and_then(|label| label.checked_add(1))
            .ok_or(WarmPseudoflowError::Invariant)?;
        let mut changed = false;
        for node in strong_nodes {
            let label = &mut self.labels[node.as_usize()];
            if label.is_some_and(|current| current < new_label) {
                *label = Some(new_label);
                changed = true;
            }
        }
        if changed {
            self.metrics.relabels = self
                .metrics
                .relabels
                .checked_add(1)
                .ok_or(WarmPseudoflowError::WorkLimit)?;
            self.count_transition()?;
        }
        Ok(())
    }

    fn select_merger(&mut self) -> Result<Option<ExactResidualArc>, WarmPseudoflowError> {
        let excesses = self.excesses();
        let roots = self.roots()?;
        let mut candidates = Vec::new();
        for node in self.internal_nodes().collect::<Vec<_>>() {
            if !excesses[roots[node.as_usize()].as_usize()].is_positive() {
                continue;
            }
            for arc in self.outgoing_residuals(node)? {
                self.count_scan(&arc)?;
                if !self.is_internal(arc.to)
                    || excesses[roots[arc.to.as_usize()].as_usize()].is_positive()
                {
                    continue;
                }
                let weak_label =
                    self.labels[arc.to.as_usize()].ok_or(WarmPseudoflowError::Invariant)?;
                let stable_id = self.arcs[arc.id.arc].stable_id.clone();
                candidates.push((
                    weak_label,
                    arc.to,
                    arc.from,
                    stable_id,
                    arc.id.direction,
                    arc,
                ));
            }
        }
        candidates.sort_by(|left, right| {
            (&left.0, &left.1, &left.2, &left.3, &left.4)
                .cmp(&(&right.0, &right.1, &right.2, &right.3, &right.4))
        });
        Ok(candidates.into_iter().next().map(|candidate| candidate.5))
    }

    fn merge_path(
        &self,
        merger: &ExactResidualArc,
    ) -> Result<Vec<ExactResidualId>, WarmPseudoflowError> {
        let strong_root = self.root(merger.from)?;
        let mut strong_reversed = Vec::new();
        let mut cursor = merger.from;
        while cursor != strong_root {
            strong_reversed
                .push(self.down_arc[cursor.as_usize()].ok_or(WarmPseudoflowError::Invariant)?);
            cursor = self.parent[cursor.as_usize()].ok_or(WarmPseudoflowError::Invariant)?;
        }
        strong_reversed.reverse();
        let mut path = strong_reversed;
        path.push(merger.id);
        cursor = merger.to;
        while let Some(parent) = self.parent[cursor.as_usize()] {
            let down = self.down_arc[cursor.as_usize()].ok_or(WarmPseudoflowError::Invariant)?;
            let upward = Self::reverse_residual(down);
            let arc = self.residual(upward)?;
            if arc.from != cursor || arc.to != parent {
                return Err(WarmPseudoflowError::Invariant);
            }
            path.push(upward);
            cursor = parent;
        }
        Ok(path)
    }

    fn invert_and_attach(&mut self, merger: &ExactResidualArc) -> Result<(), WarmPseudoflowError> {
        let mut chain = Vec::new();
        let mut cursor = merger.from;
        while let Some(parent) = self.parent[cursor.as_usize()] {
            let down = self.down_arc[cursor.as_usize()].ok_or(WarmPseudoflowError::Invariant)?;
            chain.push((cursor, parent, down));
            cursor = parent;
            if chain.len() > self.graph.nodes().len() {
                return Err(WarmPseudoflowError::Invariant);
            }
        }
        for (child, parent, down) in chain {
            self.parent[parent.as_usize()] = Some(child);
            self.down_arc[parent.as_usize()] = Some(Self::reverse_residual(down));
        }
        self.parent[merger.from.as_usize()] = Some(merger.to);
        self.down_arc[merger.from.as_usize()] = Some(Self::reverse_residual(merger.id));
        Ok(())
    }

    fn normalize_merger_path(
        &mut self,
        path: &[ExactResidualId],
        initial_amount: BigRational,
    ) -> Result<(), WarmPseudoflowError> {
        let mut amount = initial_amount;
        for &id in path {
            if amount.is_zero() {
                break;
            }
            let arc = self.residual(id)?;
            let pushed = minimum(&amount, &arc.capacity);
            if pushed.is_positive() {
                self.augment(id, &pushed)?;
                self.metrics.normalization_pushes = self
                    .metrics
                    .normalization_pushes
                    .checked_add(1)
                    .ok_or(WarmPseudoflowError::WorkLimit)?;
            }
            if arc.capacity < amount {
                if self.parent[arc.from.as_usize()] != Some(arc.to) {
                    return Err(WarmPseudoflowError::Invariant);
                }
                self.parent[arc.from.as_usize()] = None;
                self.down_arc[arc.from.as_usize()] = None;
                self.metrics.normalization_splits = self
                    .metrics
                    .normalization_splits
                    .checked_add(1)
                    .ok_or(WarmPseudoflowError::WorkLimit)?;
                amount = arc.capacity;
            }
            self.count_transition()?;
        }
        Ok(())
    }

    fn outgoing_residuals(
        &self,
        node: NodeIndex,
    ) -> Result<Vec<ExactResidualArc>, WarmPseudoflowError> {
        let mut result = Vec::new();
        for index in 0..self.arcs.len() {
            for direction in [ExactDirection::Forward, ExactDirection::Reverse] {
                let arc = self.residual(ExactResidualId {
                    arc: index,
                    direction,
                })?;
                if arc.from == node && arc.capacity.is_positive() {
                    result.push(arc);
                }
            }
        }
        result.sort_by(|left, right| {
            (&self.arcs[left.id.arc].stable_id, left.id.direction)
                .cmp(&(&self.arcs[right.id.arc].stable_id, right.id.direction))
        });
        Ok(result)
    }

    fn residual(&self, id: ExactResidualId) -> Result<ExactResidualArc, WarmPseudoflowError> {
        let arc = self
            .arcs
            .get(id.arc)
            .ok_or(WarmPseudoflowError::Invariant)?;
        Ok(match id.direction {
            ExactDirection::Forward => ExactResidualArc {
                id,
                from: arc.from,
                to: arc.to,
                capacity: &arc.capacity - &arc.flow,
            },
            ExactDirection::Reverse => ExactResidualArc {
                id,
                from: arc.to,
                to: arc.from,
                capacity: arc.flow.clone(),
            },
        })
    }

    fn augment(
        &mut self,
        id: ExactResidualId,
        amount: &BigRational,
    ) -> Result<(), WarmPseudoflowError> {
        if !amount.is_positive() || self.residual(id)?.capacity < *amount {
            return Err(WarmPseudoflowError::Invariant);
        }
        let arc = self
            .arcs
            .get_mut(id.arc)
            .ok_or(WarmPseudoflowError::Invariant)?;
        match id.direction {
            ExactDirection::Forward => arc.flow += amount,
            ExactDirection::Reverse => arc.flow -= amount,
        }
        if arc.flow.is_negative() || arc.flow > arc.capacity {
            return Err(WarmPseudoflowError::Invariant);
        }
        Ok(())
    }

    const fn reverse_residual(id: ExactResidualId) -> ExactResidualId {
        ExactResidualId {
            arc: id.arc,
            direction: match id.direction {
                ExactDirection::Forward => ExactDirection::Reverse,
                ExactDirection::Reverse => ExactDirection::Forward,
            },
        }
    }

    fn excesses(&self) -> Vec<BigRational> {
        let mut excesses = vec![BigRational::zero(); self.graph.nodes().len()];
        for arc in &self.arcs {
            excesses[arc.from.as_usize()] -= arc.flow.clone();
            excesses[arc.to.as_usize()] += arc.flow.clone();
        }
        excesses
    }

    fn root(&self, node: NodeIndex) -> Result<NodeIndex, WarmPseudoflowError> {
        let mut cursor = node;
        for _ in 0..=self.graph.nodes().len() {
            match self.parent[cursor.as_usize()] {
                Some(parent) => cursor = parent,
                None => return Ok(cursor),
            }
        }
        Err(WarmPseudoflowError::Invariant)
    }

    fn depth(&self, node: NodeIndex) -> Result<usize, WarmPseudoflowError> {
        let mut cursor = node;
        for depth in 0..=self.graph.nodes().len() {
            match self.parent[cursor.as_usize()] {
                Some(parent) => cursor = parent,
                None => return Ok(depth),
            }
        }
        Err(WarmPseudoflowError::Invariant)
    }

    fn roots(&self) -> Result<Vec<NodeIndex>, WarmPseudoflowError> {
        self.graph
            .node_indices()
            .map(|node| {
                if self.is_internal(node) {
                    self.root(node)
                } else {
                    Ok(node)
                }
            })
            .collect()
    }

    fn strong_nodes(&self) -> Vec<NodeIndex> {
        let excesses = self.excesses();
        self.internal_nodes()
            .filter(|node| {
                self.root(*node)
                    .is_ok_and(|root| excesses[root.as_usize()].is_positive())
            })
            .collect()
    }

    fn strong_membership(&self) -> Vec<bool> {
        let mut membership = vec![false; self.graph.nodes().len()];
        membership[self.source.as_usize()] = true;
        for node in self.strong_nodes() {
            membership[node.as_usize()] = true;
        }
        membership[self.sink.as_usize()] = false;
        membership
    }

    fn validate_normalized(&self) -> Result<(), WarmPseudoflowError> {
        let excesses = self.excesses();
        let mut tree_arcs = vec![false; self.arcs.len()];
        for node in self.internal_nodes() {
            if let Some(parent) = self.parent[node.as_usize()] {
                if !self.is_internal(parent) || !excesses[node.as_usize()].is_zero() {
                    return Err(WarmPseudoflowError::Invariant);
                }
                let down = self.down_arc[node.as_usize()].ok_or(WarmPseudoflowError::Invariant)?;
                let residual = self.residual(down)?;
                if residual.from != parent
                    || residual.to != node
                    || !residual.capacity.is_positive()
                {
                    return Err(WarmPseudoflowError::Invariant);
                }
                let parent_label =
                    self.labels[parent.as_usize()].ok_or(WarmPseudoflowError::Invariant)?;
                let child_label =
                    self.labels[node.as_usize()].ok_or(WarmPseudoflowError::Invariant)?;
                if parent_label > child_label {
                    return Err(WarmPseudoflowError::Invariant);
                }
                tree_arcs[down.arc] = true;
            } else if self.down_arc[node.as_usize()].is_some() {
                return Err(WarmPseudoflowError::Invariant);
            }
            self.root(node)?;
        }
        for (index, arc) in self.arcs.iter().enumerate() {
            if arc.flow.is_negative() || arc.flow > arc.capacity {
                return Err(WarmPseudoflowError::Invariant);
            }
            if arc.flow.is_positive() && arc.flow < arc.capacity && !tree_arcs[index] {
                return Err(WarmPseudoflowError::Invariant);
            }
            if ((arc.from == self.source && arc.to != self.source)
                || (arc.to == self.sink && arc.from != self.sink))
                && arc.flow != arc.capacity
            {
                return Err(WarmPseudoflowError::Invariant);
            }
        }
        for node in self.internal_nodes() {
            let from_label = self.labels[node.as_usize()].ok_or(WarmPseudoflowError::Invariant)?;
            for arc in self.outgoing_residuals(node)? {
                if !self.is_internal(arc.to) {
                    continue;
                }
                let to_label =
                    self.labels[arc.to.as_usize()].ok_or(WarmPseudoflowError::Invariant)?;
                if from_label > to_label.saturating_add(1) {
                    return Err(WarmPseudoflowError::Invariant);
                }
            }
        }
        Ok(())
    }

    fn validate_optimal_partition(&mut self, strong: &[bool]) -> Result<(), WarmPseudoflowError> {
        if strong.len() != self.graph.nodes().len()
            || !strong[self.source.as_usize()]
            || strong[self.sink.as_usize()]
        {
            return Err(WarmPseudoflowError::Invariant);
        }
        for node in self.internal_nodes().collect::<Vec<_>>() {
            if !strong[node.as_usize()] {
                continue;
            }
            for arc in self.outgoing_residuals(node)? {
                self.count_scan(&arc)?;
                if self.is_internal(arc.to) && !strong[arc.to.as_usize()] {
                    return Err(WarmPseudoflowError::Invariant);
                }
            }
        }
        Ok(())
    }

    fn count_scan(&mut self, arc: &ExactResidualArc) -> Result<(), WarmPseudoflowError> {
        if self.metrics.residual_arc_scans >= MAX_RESIDUAL_SCANS {
            return Err(WarmPseudoflowError::WorkLimit);
        }
        self.metrics.residual_arc_scans += 1;
        if self.metrics.residual_arc_scans <= PARAMETRIC_PSEUDOFLOW_TRACE_SCAN_PREFIX
            || self.metrics.residual_arc_scans.is_power_of_two()
        {
            self.scan_checkpoints.push(WarmScanCheckpoint {
                edge: self.arcs[arc.id.arc].stable_id.clone(),
                metrics: self.metrics,
            });
        }
        Ok(())
    }

    fn count_transition(&mut self) -> Result<(), WarmPseudoflowError> {
        if self.metrics.transitions >= MAX_TRANSITIONS {
            return Err(WarmPseudoflowError::WorkLimit);
        }
        self.metrics.transitions += 1;
        Ok(())
    }

    fn is_internal(&self, node: NodeIndex) -> bool {
        node != self.source && node != self.sink
    }

    fn internal_nodes(&self) -> impl Iterator<Item = NodeIndex> + '_ {
        self.graph
            .node_indices()
            .filter(|node| self.is_internal(*node))
    }
}

fn exact_capacity(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    edge_position: usize,
    parameter: &ParametricRational,
) -> Result<BigRational, WarmPseudoflowError> {
    let edge = graph
        .edges()
        .get(edge_position)
        .ok_or(WarmPseudoflowError::Invariant)?;
    let slope = problem
        .slope(edge_position)
        .ok_or(WarmPseudoflowError::Invariant)?;
    let parameter = BigRational::new(
        parameter.numerator().clone(),
        parameter.denominator().clone(),
    );
    let capacity = BigRational::from_integer(BigInt::from(edge.capacity()))
        + BigRational::from_integer(slope.clone()) * parameter;
    if capacity.is_negative() {
        return Err(WarmPseudoflowError::NegativeCapacity);
    }
    Ok(capacity)
}

fn minimum(left: &BigRational, right: &BigRational) -> BigRational {
    if left <= right {
        left.clone()
    } else {
        right.clone()
    }
}

fn ensure_subset(left: &[bool], right: &[bool]) -> Result<(), WarmPseudoflowError> {
    if left.len() != right.len() || left.iter().zip(right).any(|(left, right)| *left && !*right) {
        return Err(WarmPseudoflowError::Invariant);
    }
    Ok(())
}

/// Runs the explicit-tree canonical parametric traversal.
///
/// Forward and reverse normalized forests retain exact rational pseudoflows
/// and labels across parameter changes. Recursive children carry logical
/// source/sink contraction views; the smaller child restarts two forests and
/// the larger child continues the parent checkpoints. The explicit arrays do
/// not claim Hochbaum's dynamic-tree `O(m n log n)` implementation bound.
///
/// # Errors
///
/// Rejects normalized-forest, exact-intersection, contraction, work-limit, or
/// independent cold-oracle contradictions.
pub fn solve_parametric_pseudoflow(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
) -> Result<ParametricPseudoflowResult, ParametricPseudoflowError> {
    solve_parametric_internal(graph, problem, false).map(|run| run.result)
}

/// Runs the same canonical traversal while recording exact reuse, race,
/// contraction, restart, continuation, and certificate events.
///
/// # Errors
///
/// Returns the same failures as [`solve_parametric_pseudoflow`].
pub fn trace_parametric_pseudoflow(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
) -> Result<ParametricPseudoflowTraceResult, ParametricPseudoflowError> {
    let run = solve_parametric_internal(graph, problem, true)?;
    let trace = ParametricPseudoflowTraceResult {
        result: run.result,
        events: run.events,
    };
    check_parametric_pseudoflow_trace(graph, problem, &trace)?;
    Ok(trace)
}

/// Replays the complete exact traversal and compares every semantic event.
///
/// # Errors
///
/// Rejects any result, event identity, metric, or source-state disagreement.
pub fn check_parametric_pseudoflow_trace(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    trace: &ParametricPseudoflowTraceResult,
) -> Result<(), ParametricPseudoflowError> {
    let mut certificate_metrics =
        crate::algorithms::parametric_breakpoint_rerun::ParametricBreakpointRerunMetrics::default();
    certify_parametric_result(
        graph,
        problem,
        &mut certificate_metrics,
        &trace.result.segments,
        &trace.result.breakpoints,
    )
    .map_err(|_| ParametricPseudoflowError::Certificate)?;
    if trace.events.is_empty()
        || trace.events.last().map(|event| event.kind)
            != Some(ParametricPseudoflowEventKind::Optimal)
        || trace.events.last().map(|event| event.metrics) != Some(trace.result.metrics)
    {
        return Err(ParametricPseudoflowError::Invariant);
    }
    let mut recorded_segments = Vec::new();
    let mut recorded_breakpoints = Vec::new();
    for (index, event) in trace.events.iter().enumerate() {
        let distinct_active_nodes = event
            .active_node_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if event.event_id != index as u64 + 1
            || event.lower < *problem.minimum()
            || event.upper > *problem.maximum()
            || event.lower > event.upper
            || (event.labels_retained && !event.normalized_tree_reused)
            || distinct_active_nodes.len() != event.active_node_ids.len()
            || event
                .active_node_ids
                .iter()
                .any(|node| node.as_usize() >= graph.nodes().len())
            || (event.active_nodes.is_none() && !event.active_node_ids.is_empty())
            || (!event.active_node_ids.is_empty()
                && event.active_nodes.is_some_and(|count| {
                    usize::try_from(count).ok() != Some(event.active_node_ids.len())
                }))
            || (event.kind == ParametricPseudoflowEventKind::FreeRunRace
                && event.race_winner.is_none())
            || (event.kind == ParametricPseudoflowEventKind::Optimal
                && (index + 1 != trace.events.len()
                    || event.lower != *problem.minimum()
                    || event.upper != *problem.maximum()
                    || event.parameter.is_some()))
        {
            return Err(ParametricPseudoflowError::Invariant);
        }
        match event.kind {
            ParametricPseudoflowEventKind::RecordSegment => {
                recorded_segments.push((event.lower.clone(), event.upper.clone()));
            }
            ParametricPseudoflowEventKind::RecordBreakpoint => {
                let Some(parameter) = event.parameter.clone() else {
                    return Err(ParametricPseudoflowError::Invariant);
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
        return Err(ParametricPseudoflowError::Invariant);
    }
    Ok(())
}

fn solve_parametric_internal(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    record_trace: bool,
) -> Result<ParametricInternalRun, ParametricPseudoflowError> {
    let mut analyzer = ParametricAnalyzer {
        graph,
        problem,
        segments: Vec::new(),
        exact_extrema: BTreeMap::new(),
        metrics: ParametricPseudoflowMetrics::default(),
        events: Vec::new(),
        record_trace,
    };
    if problem.minimum() == problem.maximum() {
        let forward = analyzer.fresh_forward(problem.minimum())?;
        let reverse = analyzer.fresh_reverse(problem.maximum())?;
        let minimal = forward.strong_membership();
        let maximal = reverse
            .maximal_source_membership()
            .map_err(|_| ParametricPseudoflowError::Forest)?;
        analyzer.push_segment(
            problem.minimum().clone(),
            problem.maximum().clone(),
            &minimal,
            &maximal,
        )?;
    } else {
        let forward = analyzer.fresh_forward(problem.minimum())?;
        let reverse = analyzer.fresh_reverse(problem.maximum())?;
        let root_view = ContractionView::root(graph, problem);
        analyzer.recurse(
            problem.minimum().clone(),
            forward,
            problem.maximum().clone(),
            reverse,
            &root_view,
            0,
        )?;
    }
    normalize_parametric_segments(&mut analyzer.segments)?;
    let breakpoints =
        derive_parametric_breakpoints(graph, &analyzer.segments, &analyzer.exact_extrema)?;
    // The cold-rerun solver is an independent terminal checker, not work from
    // the retained-forest source traversal. Keep it fail-closed without
    // publishing its duplicate static feasibility executions.
    let oracle = solve_parametric_breakpoint_rerun(graph, problem)
        .map_err(|_| ParametricPseudoflowError::Certificate)?;
    if analyzer.segments != oracle.segments || breakpoints != oracle.breakpoints {
        return Err(ParametricPseudoflowError::Certificate);
    }
    for breakpoint in &breakpoints {
        analyzer.record_event(TraceEventInput {
            kind: ParametricPseudoflowEventKind::RecordBreakpoint,
            lower: breakpoint.parameter.clone(),
            upper: breakpoint.parameter.clone(),
            parameter: Some(breakpoint.parameter.clone()),
            orientation: None,
            race_winner: None,
            normalized_tree_reused: true,
            labels_retained: true,
            active_nodes: None,
            active_node_ids: Vec::new(),
            left_active_nodes: None,
            right_active_nodes: None,
            renormalization_pushes: 0,
            renormalization_splits: 0,
        })?;
    }
    analyzer.record_event(TraceEventInput {
        kind: ParametricPseudoflowEventKind::Optimal,
        lower: problem.minimum().clone(),
        upper: problem.maximum().clone(),
        parameter: None,
        orientation: None,
        race_winner: None,
        normalized_tree_reused: analyzer.metrics.forest_reuses > 0,
        labels_retained: analyzer.metrics.forest_reuses > 0,
        active_nodes: None,
        active_node_ids: Vec::new(),
        left_active_nodes: None,
        right_active_nodes: None,
        renormalization_pushes: 0,
        renormalization_splits: 0,
    })?;
    let result = ParametricPseudoflowResult {
        segments: analyzer.segments,
        breakpoints,
        metrics: analyzer.metrics,
    };
    Ok(ParametricInternalRun {
        result,
        events: analyzer.events,
    })
}

impl<'graph> ParametricAnalyzer<'graph> {
    fn fresh_forward(
        &mut self,
        parameter: &ParametricRational,
    ) -> Result<WarmPseudoflowState<'graph>, ParametricPseudoflowError> {
        self.fresh(
            parameter,
            Orientation::Forward,
            ParametricTraversalOrientation::Forward,
        )
    }

    fn fresh_reverse(
        &mut self,
        parameter: &ParametricRational,
    ) -> Result<WarmPseudoflowState<'graph>, ParametricPseudoflowError> {
        self.fresh(
            parameter,
            Orientation::Reverse,
            ParametricTraversalOrientation::Reverse,
        )
    }

    fn fresh(
        &mut self,
        parameter: &ParametricRational,
        orientation: Orientation,
        public_orientation: ParametricTraversalOrientation,
    ) -> Result<WarmPseudoflowState<'graph>, ParametricPseudoflowError> {
        let mut state = match orientation {
            Orientation::Forward => {
                WarmPseudoflowState::new_forward(self.graph, self.problem, parameter.clone())
            }
            Orientation::Reverse => {
                WarmPseudoflowState::new_reverse(self.graph, self.problem, parameter.clone())
            }
        }
        .map_err(|_| ParametricPseudoflowError::Forest)?;
        self.absorb_work(state.metrics)?;
        self.record_event(TraceEventInput {
            kind: ParametricPseudoflowEventKind::InitializeForest,
            lower: parameter.clone(),
            upper: parameter.clone(),
            parameter: Some(parameter.clone()),
            orientation: Some(public_orientation),
            race_winner: None,
            normalized_tree_reused: false,
            labels_retained: false,
            active_nodes: Some(count_to_u64(self.graph.nodes().len())?),
            active_node_ids: self.graph.node_indices().collect(),
            left_active_nodes: None,
            right_active_nodes: None,
            renormalization_pushes: 0,
            renormalization_splits: 0,
        })?;
        let mut published = state.metrics;
        state
            .run_to_optimal()
            .map_err(|_| ParametricPseudoflowError::Forest)?;
        for checkpoint in &state.scan_checkpoints {
            self.absorb_work(warm_metrics_delta(checkpoint.metrics, published)?)?;
            self.record_event(TraceEventInput {
                kind: ParametricPseudoflowEventKind::InspectResidualArc,
                lower: parameter.clone(),
                upper: parameter.clone(),
                parameter: Some(parameter.clone()),
                orientation: Some(public_orientation),
                race_winner: None,
                normalized_tree_reused: false,
                labels_retained: false,
                active_nodes: None,
                active_node_ids: Vec::new(),
                left_active_nodes: None,
                right_active_nodes: None,
                renormalization_pushes: 0,
                renormalization_splits: 0,
            })?;
            if self.record_trace {
                self.events
                    .last_mut()
                    .ok_or(ParametricPseudoflowError::Invariant)?
                    .inspected_edge = Some(checkpoint.edge.clone());
            }
            published = checkpoint.metrics;
        }
        self.absorb_work(warm_metrics_delta(state.metrics, published)?)?;
        state.scan_checkpoints.clear();
        Ok(state)
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn recurse(
        &mut self,
        lower: ParametricRational,
        forward: WarmPseudoflowState<'graph>,
        upper: ParametricRational,
        reverse: WarmPseudoflowState<'graph>,
        view: &ContractionView,
        depth: u64,
    ) -> Result<(), ParametricPseudoflowError> {
        if lower >= upper
            || forward.parameter != lower
            || reverse.parameter != upper
            || !forward.optimal
            || !reverse.optimal
        {
            return Err(ParametricPseudoflowError::Invariant);
        }
        let depth_limit = u64::try_from(self.graph.nodes().len())
            .ok()
            .and_then(|nodes| nodes.checked_mul(2))
            .ok_or(ParametricPseudoflowError::WorkLimit)?;
        if depth > depth_limit {
            return Err(ParametricPseudoflowError::WorkLimit);
        }
        self.metrics.maximum_depth = self.metrics.maximum_depth.max(depth);
        let lower_minimal = forward.strong_membership();
        let upper_maximal = reverse
            .maximal_source_membership()
            .map_err(|_| ParametricPseudoflowError::Forest)?;
        ensure_public_subset(&lower_minimal, &upper_maximal)?;
        // A closed child endpoint can expose ties outside its open-interval
        // contraction view. Strict view membership is checked at every
        // interior probe and newly discovered breakpoint instead.
        let lower_cut = warm_cut_state(self.graph, self.problem, lower_minimal)?;
        let upper_cut = warm_cut_state(self.graph, self.problem, upper_maximal)?;
        if lower_cut.membership == upper_cut.membership
            || (lower_cut.intercept == upper_cut.intercept && lower_cut.slope == upper_cut.slope)
        {
            return self.solve_interval(lower, forward, upper, reverse, view);
        }
        let denominator = &lower_cut.slope - &upper_cut.slope;
        if denominator.is_zero() {
            return Err(ParametricPseudoflowError::Invariant);
        }
        let parameter =
            ParametricRational::new(&upper_cut.intercept - &lower_cut.intercept, denominator)
                .map_err(|_| ParametricPseudoflowError::Invariant)?;
        if parameter <= lower || parameter >= upper {
            return self.solve_interval(lower, forward, upper, reverse, view);
        }

        let lower_checkpoint = forward.clone();
        let upper_checkpoint = reverse.clone();
        let outcome = race_checkpoints(forward, reverse, parameter.clone())?;
        self.absorb_race(&outcome, &lower, &upper, &parameter)?;
        self.record_race(&lower, &upper, &parameter, &outcome, view.active_count())?;
        ensure_public_subset(&lower_cut.membership, &outcome.minimal)?;
        ensure_public_subset(&outcome.minimal, &outcome.maximal)?;
        ensure_public_subset(&outcome.maximal, &upper_cut.membership)?;
        if !view.accepts(&outcome.minimal) || !view.accepts(&outcome.maximal) {
            return Err(ParametricPseudoflowError::Invariant);
        }
        match self.exact_extrema.get(&parameter) {
            Some((minimal, maximal))
                if *minimal != outcome.minimal || *maximal != outcome.maximal =>
            {
                return Err(ParametricPseudoflowError::Invariant);
            }
            None => {
                self.exact_extrema.insert(
                    parameter.clone(),
                    (outcome.minimal.clone(), outcome.maximal.clone()),
                );
            }
            Some(_) => {}
        }

        // Nodes in exact-max minus exact-min are a tie span. They stay active
        // until an open child interval proves a strict side; contracting them
        // at the breakpoint would destroy degenerate interval extrema.
        let left_view = view.left_child(&outcome.maximal)?;
        let right_view = view.right_child(&outcome.minimal)?;
        self.metrics.contraction_views = checked_add(self.metrics.contraction_views, 2)?;
        self.metrics.larger_child_continuations =
            checked_add(self.metrics.larger_child_continuations, 1)?;
        self.metrics.smaller_child_restarts = checked_add(self.metrics.smaller_child_restarts, 1)?;
        let active_nodes = count_to_u64(view.active_count())?;
        let left_active_nodes = count_to_u64(left_view.active_count())?;
        let right_active_nodes = count_to_u64(right_view.active_count())?;
        let active_node_ids = view.active_node_ids()?;
        let left_active_node_ids = left_view.active_node_ids()?;
        let right_active_node_ids = right_view.active_node_ids()?;
        self.record_event(TraceEventInput {
            kind: ParametricPseudoflowEventKind::CreateContractionViews,
            lower: lower.clone(),
            upper: upper.clone(),
            parameter: Some(parameter.clone()),
            orientation: None,
            race_winner: None,
            normalized_tree_reused: true,
            labels_retained: true,
            active_nodes: Some(active_nodes),
            active_node_ids: active_node_ids.clone(),
            left_active_nodes: Some(left_active_nodes),
            right_active_nodes: Some(right_active_nodes),
            renormalization_pushes: 0,
            renormalization_splits: 0,
        })?;
        let next_depth = depth
            .checked_add(1)
            .ok_or(ParametricPseudoflowError::WorkLimit)?;

        if left_view.active_count() <= right_view.active_count() {
            self.record_child_policy(
                &lower,
                &upper,
                &parameter,
                left_active_nodes,
                right_active_nodes,
                &left_active_node_ids,
                &right_active_node_ids,
                true,
            )?;
            let left_forward = self.fresh_forward(&lower)?;
            let left_reverse = self.fresh_reverse(&parameter)?;
            self.recurse(
                lower,
                left_forward,
                parameter.clone(),
                left_reverse,
                &left_view,
                next_depth,
            )?;
            self.recurse(
                parameter,
                outcome.forward,
                upper,
                upper_checkpoint,
                &right_view,
                next_depth,
            )
        } else {
            self.record_child_policy(
                &lower,
                &upper,
                &parameter,
                left_active_nodes,
                right_active_nodes,
                &left_active_node_ids,
                &right_active_node_ids,
                false,
            )?;
            self.recurse(
                lower,
                lower_checkpoint,
                parameter.clone(),
                outcome.reverse,
                &left_view,
                next_depth,
            )?;
            let right_forward = self.fresh_forward(&parameter)?;
            let right_reverse = self.fresh_reverse(&upper)?;
            self.recurse(
                parameter,
                right_forward,
                upper,
                right_reverse,
                &right_view,
                next_depth,
            )
        }
    }

    fn solve_interval(
        &mut self,
        lower: ParametricRational,
        forward: WarmPseudoflowState<'graph>,
        upper: ParametricRational,
        reverse: WarmPseudoflowState<'graph>,
        view: &ContractionView,
    ) -> Result<(), ParametricPseudoflowError> {
        let midpoint = rational_midpoint(&lower, &upper);
        let outcome = race_checkpoints(forward, reverse, midpoint.clone())?;
        self.absorb_race(&outcome, &lower, &upper, &midpoint)?;
        self.record_race(&lower, &upper, &midpoint, &outcome, view.active_count())?;
        ensure_public_subset(&outcome.minimal, &outcome.maximal)?;
        if !view.accepts(&outcome.minimal) || !view.accepts(&outcome.maximal) {
            return Err(ParametricPseudoflowError::Invariant);
        }
        self.push_segment(lower, upper, &outcome.minimal, &outcome.maximal)
    }

    fn push_segment(
        &mut self,
        lower: ParametricRational,
        upper: ParametricRational,
        minimal: &[bool],
        maximal: &[bool],
    ) -> Result<(), ParametricPseudoflowError> {
        if lower > upper {
            return Err(ParametricPseudoflowError::Invariant);
        }
        ensure_public_subset(minimal, maximal)?;
        let minimal = warm_cut_state(self.graph, self.problem, minimal.to_vec())?;
        let maximal = warm_cut_state(self.graph, self.problem, maximal.to_vec())?;
        if lower < upper
            && (minimal.intercept != maximal.intercept || minimal.slope != maximal.slope)
        {
            return Err(ParametricPseudoflowError::Invariant);
        }
        self.segments.push(ParametricSegment {
            lower: lower.clone(),
            upper: upper.clone(),
            minimal_cut: public_warm_cut(self.graph, &minimal)?,
            maximal_cut: public_warm_cut(self.graph, &maximal)?,
        });
        self.record_event(TraceEventInput {
            kind: ParametricPseudoflowEventKind::RecordSegment,
            lower,
            upper,
            parameter: None,
            orientation: None,
            race_winner: None,
            normalized_tree_reused: self.metrics.forest_reuses > 0,
            labels_retained: self.metrics.forest_reuses > 0,
            active_nodes: None,
            active_node_ids: Vec::new(),
            left_active_nodes: None,
            right_active_nodes: None,
            renormalization_pushes: 0,
            renormalization_splits: 0,
        })?;
        Ok(())
    }

    fn absorb_race(
        &mut self,
        outcome: &RaceOutcome<'_>,
        lower: &ParametricRational,
        upper: &ParametricRational,
        parameter: &ParametricRational,
    ) -> Result<(), ParametricPseudoflowError> {
        let mut published_forward = WarmMetrics::default();
        let mut published_reverse = WarmMetrics::default();
        for checkpoint in &outcome.scan_checkpoints {
            self.absorb_work(warm_metrics_delta(
                checkpoint.forward_delta,
                published_forward,
            )?)?;
            self.absorb_work(warm_metrics_delta(
                checkpoint.reverse_delta,
                published_reverse,
            )?)?;
            self.record_event(TraceEventInput {
                kind: ParametricPseudoflowEventKind::InspectResidualArc,
                lower: lower.clone(),
                upper: upper.clone(),
                parameter: Some(parameter.clone()),
                orientation: Some(checkpoint.orientation),
                race_winner: None,
                normalized_tree_reused: true,
                labels_retained: true,
                active_nodes: None,
                active_node_ids: Vec::new(),
                left_active_nodes: None,
                right_active_nodes: None,
                renormalization_pushes: 0,
                renormalization_splits: 0,
            })?;
            if self.record_trace {
                self.events
                    .last_mut()
                    .ok_or(ParametricPseudoflowError::Invariant)?
                    .inspected_edge = Some(checkpoint.edge.clone());
            }
            published_forward = checkpoint.forward_delta;
            published_reverse = checkpoint.reverse_delta;
        }
        self.absorb_work(warm_metrics_delta(
            outcome.forward_delta,
            published_forward,
        )?)?;
        self.absorb_work(warm_metrics_delta(
            outcome.reverse_delta,
            published_reverse,
        )?)?;
        self.metrics.free_run_races = checked_add(self.metrics.free_run_races, 1)?;
        self.metrics.cooperative_race_steps =
            checked_add(self.metrics.cooperative_race_steps, outcome.steps)?;
        match outcome.winner {
            RaceWinner::Forward => {
                self.metrics.forward_race_wins = checked_add(self.metrics.forward_race_wins, 1)?;
            }
            RaceWinner::Reverse => {
                self.metrics.reverse_race_wins = checked_add(self.metrics.reverse_race_wins, 1)?;
            }
        }
        Ok(())
    }

    fn record_race(
        &mut self,
        lower: &ParametricRational,
        upper: &ParametricRational,
        parameter: &ParametricRational,
        outcome: &RaceOutcome<'_>,
        active_nodes: usize,
    ) -> Result<(), ParametricPseudoflowError> {
        self.record_event(TraceEventInput {
            kind: ParametricPseudoflowEventKind::FreeRunRace,
            lower: lower.clone(),
            upper: upper.clone(),
            parameter: Some(parameter.clone()),
            orientation: None,
            race_winner: Some(match outcome.winner {
                RaceWinner::Forward => ParametricRaceWinner::Forward,
                RaceWinner::Reverse => ParametricRaceWinner::Reverse,
            }),
            normalized_tree_reused: true,
            labels_retained: true,
            active_nodes: Some(count_to_u64(active_nodes)?),
            active_node_ids: Vec::new(),
            left_active_nodes: None,
            right_active_nodes: None,
            renormalization_pushes: checked_add(
                outcome.forward_delta.renormalization_pushes,
                outcome.reverse_delta.renormalization_pushes,
            )?,
            renormalization_splits: checked_add(
                outcome.forward_delta.renormalization_splits,
                outcome.reverse_delta.renormalization_splits,
            )?,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn record_child_policy(
        &mut self,
        lower: &ParametricRational,
        upper: &ParametricRational,
        parameter: &ParametricRational,
        left_active_nodes: u64,
        right_active_nodes: u64,
        left_active_node_ids: &[NodeIndex],
        right_active_node_ids: &[NodeIndex],
        restart_left: bool,
    ) -> Result<(), ParametricPseudoflowError> {
        let (restart_nodes, continue_nodes, restart_node_ids, continue_node_ids) = if restart_left {
            (
                left_active_nodes,
                right_active_nodes,
                left_active_node_ids,
                right_active_node_ids,
            )
        } else {
            (
                right_active_nodes,
                left_active_nodes,
                right_active_node_ids,
                left_active_node_ids,
            )
        };
        for (kind, active_nodes, active_node_ids, reused) in [
            (
                ParametricPseudoflowEventKind::RestartSmallerChild,
                restart_nodes,
                restart_node_ids,
                false,
            ),
            (
                ParametricPseudoflowEventKind::ContinueLargerChild,
                continue_nodes,
                continue_node_ids,
                true,
            ),
        ] {
            self.record_event(TraceEventInput {
                kind,
                lower: lower.clone(),
                upper: upper.clone(),
                parameter: Some(parameter.clone()),
                orientation: None,
                race_winner: None,
                normalized_tree_reused: reused,
                labels_retained: reused,
                active_nodes: Some(active_nodes),
                active_node_ids: active_node_ids.to_vec(),
                left_active_nodes: Some(left_active_nodes),
                right_active_nodes: Some(right_active_nodes),
                renormalization_pushes: 0,
                renormalization_splits: 0,
            })?;
        }
        Ok(())
    }

    fn record_event(&mut self, input: TraceEventInput) -> Result<(), ParametricPseudoflowError> {
        if !self.record_trace {
            return Ok(());
        }
        let event_id = u64::try_from(self.events.len())
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(ParametricPseudoflowError::WorkLimit)?;
        self.events.push(ParametricPseudoflowTraceEvent {
            event_id,
            kind: input.kind,
            lower: input.lower,
            upper: input.upper,
            parameter: input.parameter,
            orientation: input.orientation,
            inspected_edge: None,
            race_winner: input.race_winner,
            normalized_tree_reused: input.normalized_tree_reused,
            labels_retained: input.labels_retained,
            active_nodes: input.active_nodes,
            active_node_ids: input.active_node_ids,
            left_active_nodes: input.left_active_nodes,
            right_active_nodes: input.right_active_nodes,
            renormalization_pushes: input.renormalization_pushes,
            renormalization_splits: input.renormalization_splits,
            metrics: self.metrics,
        });
        Ok(())
    }

    fn absorb_work(&mut self, work: WarmMetrics) -> Result<(), ParametricPseudoflowError> {
        self.metrics.forest_initializations =
            checked_add(self.metrics.forest_initializations, work.initializations)?;
        self.metrics.parameter_advances =
            checked_add(self.metrics.parameter_advances, work.parameter_advances)?;
        self.metrics.forest_reuses = checked_add(self.metrics.forest_reuses, work.forest_reuses)?;
        self.metrics.renormalization_pushes = checked_add(
            self.metrics.renormalization_pushes,
            work.renormalization_pushes,
        )?;
        self.metrics.renormalization_splits = checked_add(
            self.metrics.renormalization_splits,
            work.renormalization_splits,
        )?;
        self.metrics.mergers = checked_add(self.metrics.mergers, work.mergers)?;
        self.metrics.relabels = checked_add(self.metrics.relabels, work.relabels)?;
        self.metrics.residual_arc_scans = self
            .metrics
            .residual_arc_scans
            .checked_add(work.residual_arc_scans)
            .ok_or(ParametricPseudoflowError::WorkLimit)?;
        Ok(())
    }
}

fn race_checkpoints<'graph>(
    mut forward: WarmPseudoflowState<'graph>,
    mut reverse: WarmPseudoflowState<'graph>,
    parameter: ParametricRational,
) -> Result<RaceOutcome<'graph>, ParametricPseudoflowError> {
    let forward_before = forward.metrics;
    let reverse_before = reverse.metrics;
    let forward_floor = forward
        .prepare_parameter(parameter.clone())
        .map_err(|_| ParametricPseudoflowError::Forest)?;
    let reverse_floor = reverse
        .prepare_parameter(parameter)
        .map_err(|_| ParametricPseudoflowError::Forest)?;
    let mut forward_done = forward.optimal;
    let mut reverse_done = reverse.optimal;
    let mut winner = None;
    let mut steps = 0_u64;
    let mut scan_checkpoints = Vec::new();
    while !forward_done || !reverse_done {
        if !forward_done {
            forward_done = forward
                .step_labeling()
                .map_err(|_| ParametricPseudoflowError::Forest)?;
            for checkpoint in std::mem::take(&mut forward.scan_checkpoints) {
                scan_checkpoints.push(RaceScanCheckpoint {
                    orientation: ParametricTraversalOrientation::Forward,
                    edge: checkpoint.edge,
                    forward_delta: warm_metrics_delta(checkpoint.metrics, forward_before)?,
                    reverse_delta: warm_metrics_delta(reverse.metrics, reverse_before)?,
                });
            }
            steps = checked_add(steps, 1)?;
            if forward_done && winner.is_none() {
                winner = Some(RaceWinner::Forward);
            }
        }
        if !reverse_done {
            reverse_done = reverse
                .step_labeling()
                .map_err(|_| ParametricPseudoflowError::Forest)?;
            for checkpoint in std::mem::take(&mut reverse.scan_checkpoints) {
                scan_checkpoints.push(RaceScanCheckpoint {
                    orientation: ParametricTraversalOrientation::Reverse,
                    edge: checkpoint.edge,
                    forward_delta: warm_metrics_delta(forward.metrics, forward_before)?,
                    reverse_delta: warm_metrics_delta(checkpoint.metrics, reverse_before)?,
                });
            }
            steps = checked_add(steps, 1)?;
            if reverse_done && winner.is_none() {
                winner = Some(RaceWinner::Reverse);
            }
        }
    }
    let minimal = forward.strong_membership();
    let reverse_strong = reverse.strong_membership();
    ensure_public_subset(&forward_floor, &minimal)?;
    ensure_public_subset(&reverse_floor, &reverse_strong)?;
    let maximal = reverse
        .maximal_source_membership()
        .map_err(|_| ParametricPseudoflowError::Forest)?;
    Ok(RaceOutcome {
        forward_delta: warm_metrics_delta(forward.metrics, forward_before)?,
        reverse_delta: warm_metrics_delta(reverse.metrics, reverse_before)?,
        forward,
        reverse,
        minimal,
        maximal,
        winner: winner.unwrap_or(RaceWinner::Forward),
        steps,
        scan_checkpoints,
    })
}

fn warm_metrics_delta(
    after: WarmMetrics,
    before: WarmMetrics,
) -> Result<WarmMetrics, ParametricPseudoflowError> {
    macro_rules! delta {
        ($field:ident) => {
            after
                .$field
                .checked_sub(before.$field)
                .ok_or(ParametricPseudoflowError::Invariant)?
        };
    }
    Ok(WarmMetrics {
        initializations: delta!(initializations),
        parameter_advances: delta!(parameter_advances),
        forest_reuses: delta!(forest_reuses),
        renormalization_pushes: delta!(renormalization_pushes),
        renormalization_splits: delta!(renormalization_splits),
        mergers: delta!(mergers),
        relabels: delta!(relabels),
        normalization_pushes: delta!(normalization_pushes),
        normalization_splits: delta!(normalization_splits),
        residual_arc_scans: delta!(residual_arc_scans),
        transitions: delta!(transitions),
    })
}

fn warm_cut_state(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    membership: Vec<bool>,
) -> Result<WarmCutState, ParametricPseudoflowError> {
    if membership.len() != graph.nodes().len()
        || !membership[problem.source().as_usize()]
        || membership[problem.sink().as_usize()]
    {
        return Err(ParametricPseudoflowError::Invariant);
    }
    let mut intercept = BigInt::zero();
    let mut slope = BigInt::zero();
    for (position, edge) in graph.edges().iter().enumerate() {
        if membership[edge.from().as_usize()] && !membership[edge.to().as_usize()] {
            intercept += BigInt::from(edge.capacity());
            slope += problem
                .slope(position)
                .ok_or(ParametricPseudoflowError::Invariant)?;
        }
    }
    Ok(WarmCutState {
        membership,
        intercept,
        slope,
    })
}

fn public_warm_cut(
    graph: &FlowNetwork,
    cut: &WarmCutState,
) -> Result<ParametricCut, ParametricPseudoflowError> {
    Ok(ParametricCut {
        source_side: membership_ids(graph, &cut.membership)?,
        intercept: cut.intercept.clone(),
        slope: cut.slope.clone(),
    })
}

fn membership_ids(
    graph: &FlowNetwork,
    membership: &[bool],
) -> Result<Vec<NodeId>, ParametricPseudoflowError> {
    if membership.len() != graph.nodes().len() {
        return Err(ParametricPseudoflowError::Invariant);
    }
    Ok(graph
        .nodes()
        .iter()
        .zip(membership)
        .filter(|(_, present)| **present)
        .map(|(node, _)| node.id().clone())
        .collect())
}

fn normalize_parametric_segments(
    segments: &mut Vec<ParametricSegment>,
) -> Result<(), ParametricPseudoflowError> {
    segments.sort_by(|left, right| left.lower.cmp(&right.lower));
    let mut normalized: Vec<ParametricSegment> = Vec::with_capacity(segments.len());
    for segment in segments.drain(..) {
        if segment.lower > segment.upper
            || (segment.lower == segment.upper && !normalized.is_empty())
        {
            return Err(ParametricPseudoflowError::Invariant);
        }
        if let Some(previous) = normalized.last_mut() {
            if previous.upper != segment.lower {
                return Err(ParametricPseudoflowError::Invariant);
            }
            if previous.minimal_cut.intercept == segment.minimal_cut.intercept
                && previous.minimal_cut.slope == segment.minimal_cut.slope
            {
                ensure_id_subset(
                    &previous.minimal_cut.source_side,
                    &segment.minimal_cut.source_side,
                )?;
                ensure_id_subset(
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

fn derive_parametric_breakpoints(
    graph: &FlowNetwork,
    segments: &[ParametricSegment],
    extrema: &BTreeMap<ParametricRational, (Vec<bool>, Vec<bool>)>,
) -> Result<Vec<ParametricBreakpoint>, ParametricPseudoflowError> {
    segments
        .windows(2)
        .map(|pair| {
            let parameter = pair[0].upper.clone();
            if parameter != pair[1].lower {
                return Err(ParametricPseudoflowError::Invariant);
            }
            let (exact_minimal, exact_maximal) = extrema
                .get(&parameter)
                .ok_or(ParametricPseudoflowError::Invariant)?;
            ensure_public_subset(exact_minimal, exact_maximal)?;
            let before = membership_from_ids_public(graph, &pair[0].minimal_cut.source_side)?;
            let after = membership_from_ids_public(graph, &pair[1].minimal_cut.source_side)?;
            ensure_public_subset(&before, &after)?;
            let exact_minimal_ids = membership_ids(graph, exact_minimal)?;
            let exact_maximal_ids = membership_ids(graph, exact_maximal)?;
            let entering_nodes = exact_maximal_ids
                .iter()
                .filter(|id| !exact_minimal_ids.contains(id))
                .cloned()
                .collect::<Vec<_>>();
            if entering_nodes.is_empty() {
                return Err(ParametricPseudoflowError::Invariant);
            }
            Ok(ParametricBreakpoint {
                parameter,
                before_source_side: pair[0].minimal_cut.source_side.clone(),
                after_source_side: pair[1].minimal_cut.source_side.clone(),
                exact_minimal_source_side: exact_minimal_ids,
                exact_maximal_source_side: exact_maximal_ids,
                entering_nodes,
            })
        })
        .collect()
}

fn membership_from_ids_public(
    graph: &FlowNetwork,
    ids: &[NodeId],
) -> Result<Vec<bool>, ParametricPseudoflowError> {
    let mut membership = vec![false; graph.nodes().len()];
    for id in ids {
        let index = graph
            .node_index(id)
            .ok_or(ParametricPseudoflowError::Invariant)?;
        if membership[index.as_usize()] {
            return Err(ParametricPseudoflowError::Invariant);
        }
        membership[index.as_usize()] = true;
    }
    Ok(membership)
}

fn ensure_public_subset(left: &[bool], right: &[bool]) -> Result<(), ParametricPseudoflowError> {
    if left.len() != right.len() || left.iter().zip(right).any(|(left, right)| *left && !*right) {
        return Err(ParametricPseudoflowError::Invariant);
    }
    Ok(())
}

fn ensure_id_subset(left: &[NodeId], right: &[NodeId]) -> Result<(), ParametricPseudoflowError> {
    if left.iter().any(|id| !right.contains(id)) {
        return Err(ParametricPseudoflowError::Invariant);
    }
    Ok(())
}

fn checked_add(left: u64, right: u64) -> Result<u64, ParametricPseudoflowError> {
    left.checked_add(right)
        .ok_or(ParametricPseudoflowError::WorkLimit)
}

fn count_to_u64(value: usize) -> Result<u64, ParametricPseudoflowError> {
    u64::try_from(value).map_err(|_| ParametricPseudoflowError::WorkLimit)
}

/// Independently replays every open region and exact breakpoint while keeping
/// one forward forest from the lower endpoint and one reverse forest from the
/// upper endpoint. This is an identity/correctness gate for the future
/// contraction scheduler; it is not the cold solver and does not claim the
/// source algorithm's complete-analysis bound.
///
/// # Errors
///
/// Rejects malformed region coverage, a non-nested declared source set, or any
/// disagreement between retained-forest extrema and the certified result.
pub fn verify_parametric_warm_continuation(
    graph: &FlowNetwork,
    problem: &ParametricMaxFlowProblem,
    result: &ParametricBreakpointRerunResult,
) -> Result<ParametricWarmVerificationMetrics, ParametricWarmVerificationError> {
    let first = result
        .segments
        .first()
        .ok_or(ParametricWarmVerificationError::InvalidResult)?;
    let last = result
        .segments
        .last()
        .ok_or(ParametricWarmVerificationError::InvalidResult)?;
    if first.lower != *problem.minimum() || last.upper != *problem.maximum() {
        return Err(ParametricWarmVerificationError::InvalidResult);
    }
    let mut probes = result
        .breakpoints
        .iter()
        .map(|breakpoint| breakpoint.parameter.clone())
        .collect::<Vec<_>>();
    for segment in &result.segments {
        if segment.lower < segment.upper {
            probes.push(rational_midpoint(&segment.lower, &segment.upper));
        }
    }
    probes.sort();
    probes.dedup();

    let mut forward = WarmPseudoflowState::new_forward(graph, problem, problem.minimum().clone())
        .map_err(|_| ParametricWarmVerificationError::Kernel)?;
    forward
        .run_to_optimal()
        .map_err(|_| ParametricWarmVerificationError::Kernel)?;
    let forward_anchor = forward.clone();
    for parameter in &probes {
        let (expected, _) = expected_memberships(graph, result, parameter)?;
        let actual = forward
            .advance_parameter(parameter.clone())
            .map_err(|_| ParametricWarmVerificationError::Kernel)?;
        if actual != expected {
            return Err(ParametricWarmVerificationError::Kernel);
        }
    }

    let mut reverse = WarmPseudoflowState::new_reverse(graph, problem, problem.maximum().clone())
        .map_err(|_| ParametricWarmVerificationError::Kernel)?;
    reverse
        .run_to_optimal()
        .map_err(|_| ParametricWarmVerificationError::Kernel)?;
    let reverse_anchor = reverse.clone();
    for parameter in probes.iter().rev() {
        let (_, expected) = expected_memberships(graph, result, parameter)?;
        reverse
            .advance_parameter(parameter.clone())
            .map_err(|_| ParametricWarmVerificationError::Kernel)?;
        let actual = reverse
            .maximal_source_membership()
            .map_err(|_| ParametricWarmVerificationError::Kernel)?;
        if actual != expected {
            return Err(ParametricWarmVerificationError::Kernel);
        }
    }

    let mut race_metrics = RaceMetrics::default();
    for parameter in &probes {
        let (minimal, maximal, winner, steps) = race_to_parameter(
            forward_anchor.clone(),
            reverse_anchor.clone(),
            parameter.clone(),
        )?;
        let (expected_minimal, expected_maximal) = expected_memberships(graph, result, parameter)?;
        if minimal != expected_minimal || maximal != expected_maximal {
            return Err(ParametricWarmVerificationError::Kernel);
        }
        race_metrics.races += 1;
        race_metrics.cooperative_steps = race_metrics
            .cooperative_steps
            .checked_add(steps)
            .ok_or(ParametricWarmVerificationError::Kernel)?;
        match winner {
            RaceWinner::Forward => race_metrics.forward_wins += 1,
            RaceWinner::Reverse => race_metrics.reverse_wins += 1,
        }
    }

    Ok(aggregate_metrics(
        forward.metrics,
        reverse.metrics,
        race_metrics,
    ))
}

fn aggregate_metrics(
    forward: WarmMetrics,
    reverse: WarmMetrics,
    race: RaceMetrics,
) -> ParametricWarmVerificationMetrics {
    ParametricWarmVerificationMetrics {
        parameter_advances: forward.parameter_advances + reverse.parameter_advances,
        forest_reuses: forward.forest_reuses + reverse.forest_reuses,
        renormalization_pushes: forward.renormalization_pushes + reverse.renormalization_pushes,
        renormalization_splits: forward.renormalization_splits + reverse.renormalization_splits,
        mergers: forward.mergers + reverse.mergers,
        relabels: forward.relabels + reverse.relabels,
        free_run_races: race.races,
        forward_race_wins: race.forward_wins,
        reverse_race_wins: race.reverse_wins,
        cooperative_race_steps: race.cooperative_steps,
    }
}

fn race_to_parameter<'graph>(
    mut forward: WarmPseudoflowState<'graph>,
    mut reverse: WarmPseudoflowState<'graph>,
    parameter: ParametricRational,
) -> Result<(Vec<bool>, Vec<bool>, RaceWinner, u64), ParametricWarmVerificationError> {
    let forward_floor = forward
        .prepare_parameter(parameter.clone())
        .map_err(|_| ParametricWarmVerificationError::Kernel)?;
    let reverse_floor = reverse
        .prepare_parameter(parameter)
        .map_err(|_| ParametricWarmVerificationError::Kernel)?;
    let mut forward_done = forward.optimal;
    let mut reverse_done = reverse.optimal;
    let mut winner = None;
    let mut steps = 0_u64;
    while !forward_done || !reverse_done {
        if !forward_done {
            forward_done = forward
                .step_labeling()
                .map_err(|_| ParametricWarmVerificationError::Kernel)?;
            steps = steps
                .checked_add(1)
                .ok_or(ParametricWarmVerificationError::Kernel)?;
            if forward_done && winner.is_none() {
                winner = Some(RaceWinner::Forward);
            }
        }
        if !reverse_done {
            reverse_done = reverse
                .step_labeling()
                .map_err(|_| ParametricWarmVerificationError::Kernel)?;
            steps = steps
                .checked_add(1)
                .ok_or(ParametricWarmVerificationError::Kernel)?;
            if reverse_done && winner.is_none() {
                winner = Some(RaceWinner::Reverse);
            }
        }
    }
    let minimal = forward.strong_membership();
    ensure_subset(&forward_floor, &minimal).map_err(|_| ParametricWarmVerificationError::Kernel)?;
    let reverse_strong = reverse.strong_membership();
    ensure_subset(&reverse_floor, &reverse_strong)
        .map_err(|_| ParametricWarmVerificationError::Kernel)?;
    let maximal = reverse
        .maximal_source_membership()
        .map_err(|_| ParametricWarmVerificationError::Kernel)?;
    Ok((
        minimal,
        maximal,
        winner.unwrap_or(RaceWinner::Forward),
        steps,
    ))
}

fn rational_midpoint(left: &ParametricRational, right: &ParametricRational) -> ParametricRational {
    let left = BigRational::new(left.numerator().clone(), left.denominator().clone());
    let right = BigRational::new(right.numerator().clone(), right.denominator().clone());
    let midpoint = (left + right) / BigRational::from_integer(BigInt::from(2));
    ParametricRational::new(midpoint.numer().clone(), midpoint.denom().clone())
        .expect("the midpoint denominator is nonzero")
}

fn expected_memberships(
    graph: &FlowNetwork,
    result: &ParametricBreakpointRerunResult,
    parameter: &ParametricRational,
) -> Result<(Vec<bool>, Vec<bool>), ParametricWarmVerificationError> {
    if let Some(breakpoint) = result
        .breakpoints
        .iter()
        .find(|breakpoint| breakpoint.parameter == *parameter)
    {
        return Ok((
            membership_from_ids(graph, &breakpoint.exact_minimal_source_side)?,
            membership_from_ids(graph, &breakpoint.exact_maximal_source_side)?,
        ));
    }
    let segment = result
        .segments
        .iter()
        .find(|segment| segment.lower < *parameter && *parameter < segment.upper)
        .ok_or(ParametricWarmVerificationError::InvalidResult)?;
    Ok((
        membership_from_ids(graph, &segment.minimal_cut.source_side)?,
        membership_from_ids(graph, &segment.maximal_cut.source_side)?,
    ))
}

fn membership_from_ids(
    graph: &FlowNetwork,
    ids: &[NodeId],
) -> Result<Vec<bool>, ParametricWarmVerificationError> {
    let mut membership = vec![false; graph.nodes().len()];
    for id in ids {
        let index = graph
            .node_index(id)
            .ok_or(ParametricWarmVerificationError::InvalidResult)?;
        if membership[index.as_usize()] {
            return Err(ParametricWarmVerificationError::InvalidResult);
        }
        membership[index.as_usize()] = true;
    }
    Ok(membership)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::parametric_breakpoint_rerun::{
        ParametricBreakpointRerunResult, ParametricCapacitySlope, solve_parametric_breakpoint_rerun,
    };
    use crate::model::{FlowNode, NodeId, UnresolvedFlowEdge};

    fn node(id: &str) -> FlowNode {
        FlowNode::new(NodeId::parse(id).expect("node ID"), 0)
    }

    fn edge(id: &str, from: &str, to: &str, capacity: u64) -> UnresolvedFlowEdge {
        UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge ID"),
            from: NodeId::parse(from).expect("from"),
            to: NodeId::parse(to).expect("to"),
            lower: 0,
            capacity,
            cost: 0,
        }
    }

    fn ids(graph: &FlowNetwork, membership: &[bool]) -> Vec<String> {
        graph
            .nodes()
            .iter()
            .zip(membership)
            .filter(|(_, present)| **present)
            .map(|(node, _)| node.id().as_str().to_owned())
            .collect()
    }

    fn fixture() -> (FlowNetwork, ParametricMaxFlowProblem) {
        let graph = FlowNetwork::new(
            vec![node("a"), node("s"), node("t")],
            vec![edge("sa", "s", "a", 1), edge("at", "a", "t", 5)],
        )
        .expect("graph");
        let problem = ParametricMaxFlowProblem::new(
            &graph,
            graph
                .node_index(&NodeId::parse("s").expect("s"))
                .expect("source"),
            graph
                .node_index(&NodeId::parse("t").expect("t"))
                .expect("sink"),
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
    fn forward_state_reuses_forest_and_labels() {
        let (graph, problem) = fixture();
        let canonical = solve_parametric_pseudoflow(&graph, &problem).expect("canonical solve");
        let traced = trace_parametric_pseudoflow(&graph, &problem).expect("canonical trace");
        let cold = solve_parametric_breakpoint_rerun(&graph, &problem).expect("cold solve");
        assert_eq!(traced.result, canonical);
        assert!(
            traced
                .events
                .windows(2)
                .all(|pair| pair[0].event_id + 1 == pair[1].event_id)
        );
        assert!(traced.events.iter().any(|event| {
            event.kind == ParametricPseudoflowEventKind::FreeRunRace
                && event.normalized_tree_reused
                && event.labels_retained
        }));
        assert!(
            traced.events.iter().any(|event| {
                event.kind == ParametricPseudoflowEventKind::CreateContractionViews
            })
        );
        assert_eq!(
            traced.events.last().map(|event| event.kind),
            Some(ParametricPseudoflowEventKind::Optimal)
        );
        assert_eq!(canonical.segments, cold.segments);
        assert_eq!(canonical.breakpoints, cold.breakpoints);
        assert!(canonical.metrics.free_run_races > 0);
        assert!(canonical.metrics.forest_reuses > 0);
        let mut state = WarmPseudoflowState::new_forward(
            &graph,
            &problem,
            ParametricRational::from_integer(BigInt::zero()),
        )
        .expect("state");
        assert_eq!(ids(&graph, &state.run_to_optimal().expect("zero")), ["s"]);
        let labels = state.labels.clone();
        assert_eq!(
            ids(
                &graph,
                &state
                    .advance_parameter(ParametricRational::from_integer(BigInt::from(2)))
                    .expect("breakpoint")
            ),
            ["s"]
        );
        assert_eq!(
            ids(
                &graph,
                &state
                    .advance_parameter(ParametricRational::from_integer(BigInt::from(4)))
                    .expect("upper")
            ),
            ["a", "s"]
        );
        assert_eq!(state.metrics.forest_reuses, 2);
        assert!(state.labels.iter().zip(labels).all(|(next, previous)| {
            match (*next, previous) {
                (Some(next), Some(previous)) => next >= previous,
                (None, None) => true,
                _ => false,
            }
        }));
    }

    #[test]
    fn reverse_state_tracks_maximal_source_cut() {
        let (graph, problem) = fixture();
        let mut state = WarmPseudoflowState::new_reverse(
            &graph,
            &problem,
            ParametricRational::from_integer(BigInt::from(4)),
        )
        .expect("state");
        state.run_to_optimal().expect("upper");
        assert_eq!(
            ids(
                &graph,
                &state.maximal_source_membership().expect("maximal upper")
            ),
            ["a", "s"]
        );
        state
            .advance_parameter(ParametricRational::from_integer(BigInt::from(2)))
            .expect("breakpoint");
        assert_eq!(
            ids(
                &graph,
                &state
                    .maximal_source_membership()
                    .expect("maximal breakpoint")
            ),
            ["a", "s"]
        );
        state
            .advance_parameter(ParametricRational::from_integer(BigInt::zero()))
            .expect("lower");
        assert_eq!(
            ids(
                &graph,
                &state.maximal_source_membership().expect("maximal lower")
            ),
            ["s"]
        );
    }

    fn expected_extrema(
        result: &ParametricBreakpointRerunResult,
        parameter: &ParametricRational,
    ) -> (Vec<String>, Vec<String>) {
        if let Some(breakpoint) = result
            .breakpoints
            .iter()
            .find(|breakpoint| breakpoint.parameter == *parameter)
        {
            return (
                breakpoint
                    .exact_minimal_source_side
                    .iter()
                    .map(|id| id.as_str().to_owned())
                    .collect(),
                breakpoint
                    .exact_maximal_source_side
                    .iter()
                    .map(|id| id.as_str().to_owned())
                    .collect(),
            );
        }
        let segment = result
            .segments
            .iter()
            .find(|segment| segment.lower < *parameter && *parameter < segment.upper)
            .expect("strict interior probe");
        (
            segment
                .minimal_cut
                .source_side
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
            segment
                .maximal_cut
                .source_side
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
        )
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn deterministic_multigraphs_match_cold_extrema_during_warm_runs() {
        let mut seed = 0xc011_d00d_u64;
        let mut observed_renormalization = false;
        let mut observed_split = false;
        let mut draw = |modulus: u64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            seed % modulus
        };
        for case in 0..20 {
            let mut edges = Vec::new();
            let mut slopes = Vec::new();
            for name in ["a", "b", "c"] {
                let id = format!("s{name}");
                edges.push(edge(&id, "s", name, 1 + draw(6)));
                let slope = draw(3);
                if slope > 0 {
                    slopes.push(ParametricCapacitySlope {
                        edge: EdgeId::parse(&id).expect("source edge"),
                        slope: BigInt::from(slope),
                    });
                }
            }
            for name in ["a", "b", "c"] {
                let id = format!("{name}t");
                let magnitude = draw(3);
                edges.push(edge(&id, name, "t", 3 * magnitude + 2 + draw(6)));
                if magnitude > 0 {
                    slopes.push(ParametricCapacitySlope {
                        edge: EdgeId::parse(&id).expect("sink edge"),
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
            if case % 2 == 0 {
                edges.push(edge("ab-parallel", "a", "b", draw(4)));
            }
            if case % 3 == 0 {
                edges.push(edge("b-loop", "b", "b", draw(4)));
            }
            let graph = FlowNetwork::new(
                vec![node("a"), node("b"), node("c"), node("s"), node("t")],
                edges,
            )
            .expect("graph");
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
            .expect("problem");
            let cold =
                solve_parametric_breakpoint_rerun(&graph, &problem).unwrap_or_else(|error| {
                    panic!("cold case {case}: {error}; graph={graph:?}; problem={problem:?}")
                });
            let canonical = solve_parametric_pseudoflow(&graph, &problem)
                .unwrap_or_else(|error| panic!("canonical case {case}: {error}"));
            observed_renormalization |= canonical.metrics.renormalization_pushes > 0;
            observed_split |= canonical.metrics.renormalization_splits > 0;
            assert_eq!(canonical.segments, cold.segments, "segments case {case}");
            assert_eq!(
                canonical.breakpoints, cold.breakpoints,
                "breakpoints case {case}"
            );
            let verified = verify_parametric_warm_continuation(&graph, &problem, &cold)
                .unwrap_or_else(|error| panic!("warm verifier case {case}: {error}"));
            let mut probes = cold
                .breakpoints
                .iter()
                .map(|breakpoint| breakpoint.parameter.clone())
                .collect::<Vec<_>>();
            probes.extend(
                cold.segments
                    .iter()
                    .filter(|segment| segment.lower < segment.upper)
                    .map(|segment| rational_midpoint(&segment.lower, &segment.upper)),
            );
            probes.sort();
            probes.dedup();
            assert_eq!(verified.free_run_races, probes.len() as u64);
            assert_eq!(
                verified.forward_race_wins + verified.reverse_race_wins,
                verified.free_run_races
            );

            let mut forward =
                WarmPseudoflowState::new_forward(&graph, &problem, problem.minimum().clone())
                    .expect("forward state");
            forward.run_to_optimal().expect("forward lower");
            for parameter in &probes {
                let (expected_minimal, _) = expected_extrema(&cold, parameter);
                let actual = forward
                    .advance_parameter(parameter.clone())
                    .unwrap_or_else(|error| panic!("forward case {case}: {error}"));
                assert_eq!(
                    ids(&graph, &actual),
                    expected_minimal,
                    "forward case {case} at {}",
                    parameter.canonical()
                );
            }

            let mut reverse =
                WarmPseudoflowState::new_reverse(&graph, &problem, problem.maximum().clone())
                    .expect("reverse state");
            reverse.run_to_optimal().expect("reverse upper");
            for parameter in probes.iter().rev() {
                let (_, expected_maximal) = expected_extrema(&cold, parameter);
                reverse
                    .advance_parameter(parameter.clone())
                    .unwrap_or_else(|error| panic!("reverse case {case}: {error}"));
                assert_eq!(
                    ids(
                        &graph,
                        &reverse.maximal_source_membership().expect("maximal source")
                    ),
                    expected_maximal,
                    "reverse case {case} at {}",
                    parameter.canonical()
                );
            }
        }
        assert!(observed_renormalization);
        assert!(observed_split);
    }
}
