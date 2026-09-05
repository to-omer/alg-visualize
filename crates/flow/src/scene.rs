//! Renderer-facing flow scene contracts.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::algorithms::{BinaryBlockingStepResult, ConvexCostCertificate, DynamicEibfsResult};
use crate::catalog::{
    AlgorithmDetailStepV1, AlgorithmStepAvailabilityV1, AlgorithmStepContractV1,
    AlgorithmWorkAbstractionV1, AlgorithmWorkVisualizationV1,
};
use crate::certificate::{
    AssignmentCertificate, AssignmentHallWitness, BipartiteMatchingCertificate, MaxFlowCertificate,
    MinCostFlowCertificate, MinCostMaxFlowCertificate, divergences,
};
use crate::feasibility::{
    CapturedFeasibilityRequest, FeasibilityArcId, FeasibilityMetricSummary, FeasibilityNodeId,
    FeasibilityResidualArcId, FeasibilityResidualDirection, FeasibilityTraceEvent,
    FeasibilityTraceEventKind, FeasibilityTraceMetrics, FeasibilityTraceSnapshot,
    InfeasibilityWitness,
};
use crate::model::{EdgeId, FlowEdge, FlowNetwork};
use crate::residual::{ResidualDirection, ResidualError, ResidualState};
use crate::scenario::{
    FRAME_ENCODING_REVISION, FlowAlgorithmSelectionV1, FlowGraphV1, FlowProblemModelV1,
    FlowRationalV1, RunProfileV1, TraceGranularityV1,
};
use crate::trace::{
    DynamicEibfsTraceStage, DynamicEibfsTraceViolation, EibfsTraceMembership,
    EibfsTracePhaseDirection, EibfsTraceRootKind, FlowTraceEntityRef, FlowTraceEvent,
    FlowTraceMetrics, FlowTraceSnapshot,
};

/// Number of absolute counters in `flow-metrics/6`.
pub const FLOW_METRIC_COUNT: usize = 16;

/// Exact counters for non-BFS augmenting-path fast results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowAugmentingPathMetrics {
    /// Complete selector searches.
    pub path_searches: u64,
    /// Capacity-scaling phases; zero for unscaled selectors.
    pub scaling_phases: u64,
    /// Positive residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Successful residual augmentations.
    pub augmentations: u64,
}

/// Exact counters for potential + Dijkstra minimum-cost flow results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowPotentialDijkstraMetrics {
    /// Complete reduced-cost Dijkstra searches.
    pub dijkstra_runs: u64,
    /// Vertices permanently settled across all searches.
    pub settled_nodes: u128,
    /// Dual-potential update phases.
    pub potential_updates: u64,
    /// Positive residual arcs inspected by Dijkstra.
    pub residual_arc_scans: u128,
    /// Successful shortest-path augmentations.
    pub augmentations: u64,
}

/// Exact counters for an explicit split-dual planar shortest-path result.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowPlanarMetrics {
    /// Number of split-dual faces.
    pub dual_faces: u64,
    /// Dual Dijkstra invocations.
    pub dual_shortest_path_runs: u64,
    /// Dual arcs inspected.
    pub dual_arc_scans: u128,
    /// Dual faces permanently settled.
    pub settled_faces: u64,
    /// Original edges carrying positive reconstructed flow.
    pub positive_flow_edges: u64,
}

/// Exact counters for the bounded explicit-tree leftmost-path planar solver.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowLeftmostPlanarMetrics {
    /// Number of faces in the unsplit planar dual.
    pub dual_faces: u64,
    /// Dual shortest-path preprocessing runs.
    pub preprocessing_runs: u64,
    /// Dual and clockwise-rotation darts inspected.
    pub dart_scans: u128,
    /// Right-first trees rebuilt, including the final failed search.
    pub right_first_searches: u64,
    /// Leftmost residual paths saturated.
    pub augmentations: u64,
    /// Path darts made nonresidual by saturation.
    pub saturated_path_darts: u64,
    /// Vertices discovered across all rebuilt trees.
    pub discovered_vertices: u128,
}

/// Exact counters for restricted-primal Dinitz minimum-cost flow results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowBlockingPrimalDualMetrics {
    /// Equality-subnetwork breadth-first searches.
    pub admissible_bfs_runs: u64,
    /// Reduced-cost multi-source shortest-slack searches.
    pub slack_searches: u64,
    /// Vertices permanently settled across slack searches.
    pub settled_nodes: u128,
    /// Positive residual arcs inspected by all restricted-primal work.
    pub residual_arc_scans: u128,
    /// Dual-price tightening phases.
    pub potential_updates: u64,
    /// Completed shortest-level blocking-flow phases.
    pub blocking_flow_phases: u64,
    /// Successful equality-path augmentations.
    pub augmentations: u64,
}

/// Exact counters for finite-capacity scaling minimum-cost flow results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowCapacityScalingMetrics {
    /// Complete reduced-cost Dijkstra searches.
    pub dijkstra_runs: u64,
    /// Vertices permanently settled across all searches.
    pub settled_nodes: u128,
    /// Dual-potential update phases after successful searches.
    pub potential_updates: u64,
    /// Positive scale-eligible residual arcs inspected by Dijkstra.
    pub residual_arc_scans: u128,
    /// Successful scale-eligible residual-path augmentations.
    pub augmentations: u64,
    /// Powers-of-two capacity scales entered, including empty phases.
    pub scaling_phases: u64,
    /// Negative reduced-cost arcs saturated at phase boundaries.
    pub phase_saturations: u64,
}

/// Exact counters for generic cost-scaling push--relabel results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowCostScalingMetrics {
    /// Epsilon-refinement phases entered.
    pub refine_phases: u64,
    /// Negative reduced-cost arcs saturated at refine boundaries.
    pub initial_saturations: u64,
    /// Admissible local pushes.
    pub pushes: u64,
    /// Pushes that exhaust the selected residual arc.
    pub saturating_pushes: u64,
    /// Pushes that exhaust the active vertex first.
    pub nonsaturating_pushes: u64,
    /// Price increases after complete current-arc scans.
    pub relabels: u64,
    /// Active vertices selected.
    pub active_vertex_selections: u64,
    /// Active-vertex discharges completed.
    pub discharges: u64,
    /// Residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Rejected current arcs.
    pub current_arc_advances: u128,
    /// Admissible-path searches started by augment--relabel variants.
    pub path_searches: u64,
    /// Admissible path extensions.
    pub path_advances: u128,
    /// Search retreats after relabeling a non-root tip.
    pub retreats: u64,
    /// Sequential path augmentations.
    pub path_augmentations: u64,
    /// Augmentations ending at a deficit vertex.
    pub deficit_augmentations: u64,
    /// Augmentations ending at the partial path-length bound.
    pub length_limit_augmentations: u64,
    /// Potential-only epsilon-refinement attempts.
    pub price_refinement_attempts: u64,
    /// Attempts that skipped flow-changing refinement.
    pub price_refinement_successes: u64,
    /// Attempts rejected by a negative constraint cycle.
    pub price_refinement_failures: u64,
    /// Complete difference-constraint relaxation rounds.
    pub price_refinement_rounds: u64,
    /// Successful price relaxations.
    pub price_refinement_relaxations: u128,
    /// Residual arc identities inspected by price refinement.
    pub price_refinement_arc_scans: u128,
    /// Arc-fixing set recomputations.
    pub arc_fixing_passes: u64,
    /// Original arcs newly fixed.
    pub arcs_fixed: u64,
    /// Original arcs restored by threshold, fix-in, or recovery.
    pub arcs_unfixed: u64,
    /// Complementary-slackness fix-ins.
    pub fix_ins: u64,
    /// Residual identities skipped while fixed.
    pub fixed_arc_skips: u128,
    /// Conservative all-arc recovery passes.
    pub arc_fixing_recoveries: u64,
}

/// Exact counters for feasible-start Fulkerson Out-of-Kilter.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowOutOfKilterMetrics {
    /// Modified-labeling searches.
    pub label_searches: u64,
    /// Positive residual arcs inspected by labeling and cut-price scans.
    pub residual_arc_scans: u128,
    /// Flow-changing breakthrough corrections.
    pub breakthroughs: u64,
    /// Nonbreakthrough updates of the unlabeled node prices.
    pub price_updates: u64,
    /// Original arcs selected for repeated correction.
    pub selected_arcs: u64,
}

/// Exact counters for the natural primal network simplex.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowNetworkSimplexMetrics {
    /// Complete cyclic block-pricing searches.
    pub pricing_searches: u64,
    /// Original arcs inspected by pricing.
    pub pricing_arc_scans: u128,
    /// Basic-cycle pivots.
    pub pivots: u64,
    /// Positive-delta pivots.
    pub nondegenerate_pivots: u64,
    /// Zero-delta basis pivots.
    pub degenerate_pivots: u64,
    /// Entering/leaving tree-arc exchanges.
    pub basis_exchanges: u64,
    /// Entering arcs flipped directly to the opposite bound.
    pub bound_flips: u64,
    /// Tree arcs inspected while forming bottlenecks.
    pub cycle_arc_scans: u128,
    /// Full tree-potential reconstructions.
    pub potential_recomputations: u64,
}

/// Exact counters for the bounded directional dynamic-tree network simplex.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowDynamicTreeNetworkSimplexMetrics {
    /// Complete cyclic block-pricing searches.
    pub pricing_searches: u64,
    /// Original arcs inspected by pricing.
    pub pricing_arc_scans: u128,
    /// Basic-cycle pivots.
    pub pivots: u64,
    /// Positive-delta pivots.
    pub nondegenerate_pivots: u64,
    /// Zero-delta basis pivots.
    pub degenerate_pivots: u64,
    /// Entering/leaving tree-arc exchanges.
    pub basis_exchanges: u64,
    /// Entering arcs flipped directly to the opposite bound.
    pub bound_flips: u64,
    /// Tree arcs inspected to retain the explicit strong-feasibility tie rule.
    pub cycle_arc_scans: u128,
    /// Full tree-potential reconstructions.
    pub potential_recomputations: u64,
    /// Link-cut path-minimum queries.
    pub path_minimum_queries: u64,
    /// Lazy directional root-path updates.
    pub path_updates: u64,
    /// Directional forest constructions.
    pub directional_forest_rebuilds: u64,
    /// Directional link-cut values checked against explicit flows.
    pub directional_value_validations: u64,
    /// Entering tree arcs linked during exchanges.
    pub tree_links: u64,
    /// Leaving tree arcs cut during exchanges.
    pub tree_cuts: u64,
    /// Explicit rooted-tree reconstructions retained by this bounded kernel.
    pub tree_rebuilds: u64,
}

/// Exact counters for the shared Transportation Simplex / MODI kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowTransportationMetrics {
    /// Feasible shipment constructions.
    pub feasibility_searches: u64,
    /// Positive-support cycles removed before basis completion.
    pub support_cycle_cancellations: u64,
    /// Zero-shipment routes added to the basis forest.
    pub basis_extensions: u64,
    /// Full row/column potential reconstructions.
    pub potential_recomputations: u64,
    /// Complete Bland pricing searches.
    pub pricing_searches: u64,
    /// Nonbasic routes inspected by pricing.
    pub pricing_scans: u128,
    /// Fundamental-cycle pivots.
    pub pivots: u64,
    /// Positive-theta pivots.
    pub nondegenerate_pivots: u64,
    /// Zero-theta basis exchanges.
    pub degenerate_pivots: u64,
    /// Entering/leaving basis exchanges.
    pub basis_exchanges: u64,
    /// Routes inspected by support and basis-path work.
    pub structure_scans: u128,
}

/// Exact counters for feasible-flow negative-cycle cancellation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowCycleCancelingMetrics {
    /// Complete all-component negative-cycle searches.
    pub cycle_searches: u64,
    /// Bellman–Ford outer relaxation passes.
    pub relaxation_passes: u64,
    /// Positive residual arcs inspected during relaxation.
    pub residual_arc_scans: u128,
    /// Negative residual cycles canceled.
    pub canceled_cycles: u64,
}

/// Exact counters for Karp minimum-mean cycle cancellation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowMinimumMeanCycleCancelingMetrics {
    /// Complete all-component mean-cycle selections.
    pub mean_cycle_searches: u64,
    /// Karp dynamic-programming rounds.
    pub dynamic_programming_rounds: u64,
    /// Residual arcs inspected by selector work.
    pub residual_arc_scans: u128,
    /// Minimum-mean residual cycles canceled.
    pub canceled_cycles: u64,
}

/// Exact counters for blocking-flow maximum-flow fast results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowBlockingFlowMetrics {
    /// Breadth-first searches, including the final failed search.
    pub bfs_runs: u64,
    /// Positive residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Successful residual augmentations.
    pub augmentations: u64,
    /// Completed blocking-flow phases.
    pub blocking_flow_phases: u64,
}

/// Exact counters for bounded Goldberg--Rao binary-length phases.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowGoldbergRaoMetrics {
    /// Reverse 0--1 distance computations.
    pub distance_searches: u64,
    /// Gap-halving phases entered.
    pub phases: u64,
    /// Residual arcs inspected by all explicit work.
    pub residual_arc_scans: u128,
    /// Binary blocking-flow update steps.
    pub update_steps: u64,
    /// Canonical distance cuts evaluated.
    pub canonical_cut_evaluations: u64,
    /// Update steps ending in a blocking flow below delta.
    pub blocking_updates: u64,
    /// Base-zero residual-arc observations.
    pub zero_length_arc_observations: u64,
    /// Special residual-arc observations.
    pub special_arc_observations: u64,
    /// Nontrivial zero-length SCC contractions.
    pub nontrivial_contractions: u64,
    /// Gap upper-bound replacements.
    pub cut_updates: u64,
    /// Contracted admissible-path augmentations.
    pub contracted_augmentations: u64,
    /// Update steps delivering exactly delta.
    pub delta_limited_updates: u64,
    /// Internal SCC routing paths used during lifting.
    pub component_routing_paths: u64,
    /// Total flow units delivered by binary updates.
    pub augmented_units: u128,
    /// Mutations charged to the deterministic work ceiling.
    pub state_transitions: u64,
}

/// Exact counters for link-cut-tree blocking-flow fast results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowDynamicTreeBlockingMetrics {
    /// Breadth-first searches, including the final failed search.
    pub bfs_runs: u64,
    /// Positive residual arcs inspected while building level graphs.
    pub residual_arc_scans: u128,
    /// Complete source-to-sink root-path updates.
    pub augmentations: u64,
    /// Completed blocking-flow phases.
    pub blocking_flow_phases: u64,
    /// Root-path minimum queries.
    pub path_minimum_queries: u64,
    /// Lazy root-path residual-capacity updates.
    pub path_updates: u64,
    /// Candidate level arcs linked into represented trees.
    pub tree_links: u64,
    /// Saturated or pruned represented arcs cut from trees.
    pub tree_cuts: u64,
    /// Dead represented roots whose incoming level arcs were deleted.
    pub dead_end_prunes: u64,
    /// Link, path-update, cut, and prune transitions.
    pub state_transitions: u64,
}

/// Exact counters for dynamic-tree FIFO push--relabel fast results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowDynamicTreePushRelabelMetrics {
    /// Saturating pushes used to initialize the source preflow.
    pub source_pushes: u64,
    /// Paper parameter `k`, the maximum represented-tree vertex count.
    pub tree_size_limit: u64,
    /// Current-edge and relabel residual-arc inspections.
    pub residual_arc_scans: u128,
    /// Lazy represented root-path sends.
    pub tree_path_sends: u64,
    /// Represented component-size queries used by the `k` gate.
    pub component_size_queries: u64,
    /// Admissible current edges linked into represented trees.
    pub tree_links: u64,
    /// Saturated or relabel-invalidated represented edges cut during the run.
    pub tree_cuts: u64,
    /// Valid distance-label increases.
    pub relabels: u64,
    /// Tree edges whose implicit flow is materialized only at termination.
    pub final_tree_materializations: u64,
    /// Zero-to-positive roots added to the FIFO queue.
    pub queue_additions: u64,
    /// Eligible edges rejected by the `k` gate.
    pub size_gate_rejections: u64,
    /// Source, ordinary, and root-path push operations.
    pub pushes: u64,
    /// Push/send operations that exhaust at least one selected residual edge.
    pub saturating_pushes: u64,
    /// Push/send operations that first exhaust the selected excess.
    pub nonsaturating_pushes: u64,
    /// FIFO discharge operations.
    pub discharges: u64,
    /// Active roots removed from the FIFO queue.
    pub active_vertex_selections: u64,
}

/// Exact counters for layered-network blocking-preflow fast results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowBlockingPreflowMetrics {
    /// Breadth-first searches, including the final failed search.
    pub bfs_runs: u64,
    /// Positive residual or layered arcs inspected.
    pub residual_arc_scans: u128,
    /// Completed blocking-flow phases.
    pub blocking_flow_phases: u64,
    /// Single-arc residual mutations inside layered-network work.
    pub pushes: u64,
    /// Pushes that exhaust their selected residual arc.
    pub saturating_pushes: u64,
    /// Pushes that stop before exhausting their selected residual arc.
    pub nonsaturating_pushes: u64,
    /// Karzanov balancing iterations that return stranded excess.
    pub balancing_iterations: u64,
    /// MPM vertices selected and eliminated by minimum potential.
    pub vertex_eliminations: u64,
}

/// Exact counters for shortest-augmenting-path fast results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowDistanceLabelMetrics {
    /// Positive residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Successful source-to-sink augmentations.
    pub augmentations: u64,
    /// Distance-label increases.
    pub relabels: u64,
    /// Backtracks after relabeling a non-source vertex.
    pub retreats: u64,
    /// Reverse-BFS label initializations.
    pub reverse_bfs_runs: u64,
    /// Gap-heuristic terminations.
    pub gap_terminations: u64,
}

/// Exact counters for Ahuja--Orlin DD2 shortest-path-tree repair.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowDistanceDirectedMetrics {
    /// Reverse breadth-first exact-tree constructions.
    pub reverse_bfs_runs: u64,
    /// Capacity thresholds entered by the scaling preset.
    pub scaling_phases: u64,
    /// Positive threshold-eligible residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Successful unique tree-path augmentations.
    pub augmentations: u64,
    /// Completed update-tree repairs.
    pub tree_repairs: u64,
    /// Invalid tree arcs processed.
    pub invalid_tree_arcs: u64,
    /// Tree parents replaced without relabeling.
    pub tree_arc_replacements: u64,
    /// Exact distance-label increases.
    pub relabels: u64,
    /// Nodes deleted as unreachable at the active threshold.
    pub node_deletions: u64,
    /// Tree arcs saturated by augmentation.
    pub saturated_tree_arcs: u64,
    /// Child parent arcs invalidated by an ancestor relabel.
    pub cascading_invalidations: u64,
    /// Rejected current-arc candidates.
    pub current_arc_advances: u128,
    /// Bounded kernel transitions.
    pub state_transitions: u64,
}

/// Exact counters for preflow push–relabel fast results.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowPushRelabelMetrics {
    /// Positive residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Valid height increases.
    pub relabels: u64,
    /// Local pushes, including source initialization.
    pub pushes: u64,
    /// Pushes that exhaust their residual arc.
    pub saturating_pushes: u64,
    /// Pushes that exhaust the active vertex's excess first.
    pub nonsaturating_pushes: u64,
    /// Completed active-vertex discharges.
    pub discharges: u64,
    /// Active vertices selected by the scheduling policy.
    pub active_vertex_selections: u64,
    /// Successful bounded-path augmentations for partial augment–relabel.
    pub augmentations: u64,
    /// Bounded admissible-path searches for partial augment–relabel.
    pub path_searches: u64,
    /// Recursive admissible-path search retreats after relabeling.
    pub retreats: u64,
    /// Exact reverse-BFS global relabel operations.
    pub global_relabels: u64,
    /// Gap relabel operations that raise a nonempty set of vertices.
    pub gap_relabels: u64,
    /// Excess-dominator scaling phases.
    pub scaling_phases: u64,
}

/// Exact counters for prediction-seeded warm-start Push--Relabel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowWarmStartMetrics {
    /// Gap-relabel auxiliary maximum-flow problems solved.
    pub auxiliary_solves: u64,
    /// Exact prediction error parameter.
    pub eta: u128,
    /// Positive residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Conservation-recovery paths.
    pub recovery_paths: u64,
    /// Residual flow needed to make the prediction cut-saturating.
    pub cut_saturation_error: u128,
    /// Total predicted nonterminal excess and deficit.
    pub imbalance_error: u128,
    /// Valid auxiliary height increases.
    pub relabels: u64,
    /// Nodes moved across the maintained cut.
    pub cut_transfers: u64,
    /// Original edges carrying positive predicted flow.
    pub predicted_positive_edges: u64,
    /// Nonempty gap relabel batches.
    pub gap_relabels: u64,
    /// Auxiliary local pushes.
    pub pushes: u64,
    /// Auxiliary saturating pushes.
    pub saturating_pushes: u64,
    /// Auxiliary nonsaturating pushes.
    pub nonsaturating_pushes: u64,
    /// Auxiliary completed discharges.
    pub discharges: u64,
    /// Auxiliary active-vertex selections.
    pub active_vertex_selections: u64,
}

/// Exact counters projected from labeling pseudoflow and flow recovery.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowPseudoflowMetrics {
    /// Positive residual arcs inspected by merger selection and recovery.
    pub residual_arc_scans: u128,
    /// Strong-to-weak branch mergers.
    pub mergers: u64,
    /// Strong-set label updates.
    pub relabels: u64,
    /// Normalization and recovery residual-arc pushes.
    pub pushes: u64,
    /// Pushes that exhaust their residual arc.
    pub saturating_pushes: u64,
    /// Pushes that leave positive capacity on their residual arc.
    pub nonsaturating_pushes: u64,
    /// Residual paths used during feasible-flow recovery.
    pub recovery_paths: u64,
    /// Real residual arcs on pseudoflow-simplex pivot cycles.
    pub pivot_cycle_arcs: u128,
    /// Pivots leaving through an internal basis arc.
    pub internal_leaves: u64,
    /// Pivots in which the entering arc immediately leaves.
    pub entering_leaves: u64,
    /// Pivots leaving through the virtual strong-root excess arc.
    pub strong_root_leaves: u64,
    /// Pivots leaving through the virtual weak-root deficit arc.
    pub weak_root_leaves: u64,
    /// Zero-delta basis pivots.
    pub degenerate_pivots: u64,
}

/// Exact counters for standard one-direction IBFS.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowIbfsMetrics {
    /// Source/sink growth passes entered.
    pub passes: u64,
    /// Source-tree growth passes entered.
    pub forward_passes: u64,
    /// Sink-tree growth passes entered.
    pub reverse_passes: u64,
    /// All positive residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Positive residual arcs inspected during adoption.
    pub adoption_arc_scans: u128,
    /// Vertices attached to either BFS tree.
    pub tree_attachments: u64,
    /// Shortest-path augmentations.
    pub augmentations: u64,
    /// Sum of arcs across augmented paths.
    pub augmented_path_arcs: u128,
    /// Tree arcs saturated by augmentation.
    pub saturated_tree_arcs: u64,
    /// FIFO orphan records created.
    pub orphan_creations: u64,
    /// FIFO orphan records processed.
    pub orphan_visits: u64,
    /// Same-level orphan adoptions.
    pub same_level_adoptions: u64,
    /// Orphan distance increases.
    pub orphan_relabels: u64,
    /// Orphans removed from a tree.
    pub tree_removals: u64,
    /// Boundary vertices whose adjacency was exhausted.
    pub active_vertex_scans: u64,
    /// Logical mutations charged to the work ceiling.
    pub state_transitions: u64,
}

/// Exact counters for explicit-tree round-robin Excesses IBFS.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowEibfsMetrics {
    /// Growth phases entered.
    pub phases: u64,
    /// Source-forest growth phases.
    pub forward_phases: u64,
    /// Sink-forest growth phases.
    pub reverse_phases: u64,
    /// All charged residual and positive-flow arc inspections.
    pub residual_arc_scans: u128,
    /// Round-robin adoption arc inspections.
    pub adoption_arc_scans: u128,
    /// Free-node forest attachments.
    pub tree_attachments: u64,
    /// Connecting-arc pseudoflow pushes.
    pub bridge_pushes: u64,
    /// Forest-path pushes used to drain a bad-sign node.
    pub tree_path_pushes: u64,
    /// Tree arcs saturated while draining.
    pub saturated_tree_arcs: u64,
    /// Orphans created.
    pub orphan_creations: u64,
    /// Orphans visited by adoption.
    pub orphan_visits: u64,
    /// Orphan relabel operations.
    pub orphan_relabels: u64,
    /// Orphans removed from their old forest.
    pub tree_removals: u64,
    /// Bad-sign roots moved between forests.
    pub side_migrations: u64,
    /// Same-cut cancellation paths used for feasible-flow recovery.
    pub recovery_cancellations: u64,
    /// Logical mutations charged to the work ceiling.
    pub state_transitions: u64,
}

/// Exact counters for native Hopcroft–Karp matching phases.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowBipartiteMatchingMetrics {
    /// Alternating BFS runs, including the final unsuccessful search.
    pub bfs_runs: u64,
    /// Compatibility edges inspected by BFS and layered DFS.
    pub edge_scans: u128,
    /// Reachable shortest-path phases completed.
    pub phases: u64,
    /// Vertex-disjoint shortest augmenting paths applied.
    pub augmentations: u64,
    /// Free-left roots submitted to layered DFS.
    pub dfs_roots: u64,
}

/// Exact counters for native rectangular Hungarian assignment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowAssignmentMetrics {
    /// Agent-rooted augmenting searches.
    pub agent_searches: u64,
    /// Dense task cells inspected, including forbidden pairs.
    pub cell_scans: u128,
    /// Dual-label updates.
    pub dual_updates: u64,
    /// Strict predecessor/slack improvements.
    pub predecessor_updates: u64,
    /// Alternating assignment augmentations.
    pub augmentations: u64,
}

/// Exact counters for sequential epsilon-scaling assignment auction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowAuctionMetrics {
    /// Alternating feasibility searches before price iteration.
    pub feasibility_searches: u64,
    /// Feasibility augmentations completed before price iteration.
    pub feasibility_augmentations: u64,
    /// Allowed assignment edges inspected.
    pub edge_scans: u128,
    /// Epsilon scales entered.
    pub scaling_phases: u64,
    /// Unassigned-agent bids.
    pub bids: u64,
    /// Object price increases.
    pub price_raises: u64,
    /// Object awards.
    pub awards: u64,
    /// Previously assigned agents displaced by awards.
    pub evictions: u64,
}

/// Scene-facing availability of source-level detailed playback.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "availability", rename_all = "kebab-case")]
pub enum FlowDetailStepCapabilityV1 {
    /// The trace records the stated primitive boundary.
    Available {
        /// Meaning of one detailed step.
        unit: String,
    },
    /// The trace aggregates its internal primitives.
    Unavailable {
        /// User-facing explanation for the disabled playback option.
        reason: String,
    },
}

/// Scene-facing availability of an intermediate boundary kind.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "availability", rename_all = "kebab-case")]
pub enum FlowStepAvailabilityV1 {
    /// The endpoint records this boundary kind.
    Available,
    /// The endpoint intentionally omits or aggregates this boundary kind.
    Unavailable {
        /// User-facing explanation for the disabled playback option.
        reason: String,
    },
}

/// Algorithm-specific meaning and availability of playback boundaries.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowAlgorithmStepContractV1 {
    /// Meaning of one phase boundary.
    pub phase_unit: String,
    /// Whether intermediate phase boundaries are recorded.
    pub phase_availability: FlowStepAvailabilityV1,
    /// Meaning of one complete invariant-preserving operation.
    pub operation_unit: String,
    /// Whether intermediate operation boundaries are recorded.
    pub operation_availability: FlowStepAvailabilityV1,
    /// Availability and meaning of detailed boundaries.
    pub detail: FlowDetailStepCapabilityV1,
    /// Monotone implementation-work witness used by trace-density audits.
    pub primary_work: FlowPrimaryWorkV1,
}

/// Renderer-facing primary work-counter contract.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPrimaryWorkV1 {
    /// Ordinal in the fixed scene metric vector.
    pub metric_ordinal: u8,
    /// Human-readable plural counter unit.
    pub unit: String,
    /// Whether the unit is a primitive, iteration, or bounded oracle call.
    pub abstraction: FlowWorkAbstractionV1,
    /// Work-domain classification. Renderers must not infer graph focus from it.
    pub visualization: FlowWorkVisualizationKindV1,
}

/// Renderer-facing category of primary implementation work.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowWorkVisualizationKindV1 {
    /// Work measured over residual, pricing, matching, or assignment edges.
    EdgeField,
    /// Work measured over cycle, forest, or vector candidates.
    CandidateField,
    /// Work measured by matrix products or elimination pivots.
    NumericField,
}

/// Abstraction level of a primary implementation-work counter.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowWorkAbstractionV1 {
    /// Directly enumerated combinatorial work.
    Primitive,
    /// One source-level numerical or combinatorial iteration.
    Iteration,
    /// One bounded source-defined oracle/data-structure query.
    OracleCall,
}

impl From<AlgorithmStepContractV1> for FlowAlgorithmStepContractV1 {
    fn from(value: AlgorithmStepContractV1) -> Self {
        Self {
            phase_unit: value.phase_unit.to_owned(),
            phase_availability: step_availability(value.phase_availability),
            operation_unit: value.operation_unit.to_owned(),
            operation_availability: step_availability(value.operation_availability),
            detail: match value.detail {
                AlgorithmDetailStepV1::Available { unit } => {
                    FlowDetailStepCapabilityV1::Available {
                        unit: unit.to_owned(),
                    }
                }
                AlgorithmDetailStepV1::Unavailable { reason } => {
                    FlowDetailStepCapabilityV1::Unavailable {
                        reason: reason.to_owned(),
                    }
                }
            },
            primary_work: FlowPrimaryWorkV1 {
                metric_ordinal: value.primary_work.metric_ordinal,
                unit: value.primary_work.unit.to_owned(),
                abstraction: match value.primary_work.abstraction {
                    AlgorithmWorkAbstractionV1::Primitive => FlowWorkAbstractionV1::Primitive,
                    AlgorithmWorkAbstractionV1::Iteration => FlowWorkAbstractionV1::Iteration,
                    AlgorithmWorkAbstractionV1::OracleCall => FlowWorkAbstractionV1::OracleCall,
                },
                visualization: match value.primary_work.visualization {
                    AlgorithmWorkVisualizationV1::EdgeField => {
                        FlowWorkVisualizationKindV1::EdgeField
                    }
                    AlgorithmWorkVisualizationV1::CandidateField => {
                        FlowWorkVisualizationKindV1::CandidateField
                    }
                    AlgorithmWorkVisualizationV1::NumericField => {
                        FlowWorkVisualizationKindV1::NumericField
                    }
                },
            },
        }
    }
}

fn step_availability(value: AlgorithmStepAvailabilityV1) -> FlowStepAvailabilityV1 {
    match value {
        AlgorithmStepAvailabilityV1::Available => FlowStepAvailabilityV1::Available,
        AlgorithmStepAvailabilityV1::Unavailable { reason } => {
            FlowStepAvailabilityV1::Unavailable {
                reason: reason.to_owned(),
            }
        }
    }
}

/// Bounded current snapshot emitted before or between committed solve events.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowCurrentSceneV9 {
    /// Plugin-local scene schema.
    #[schemars(schema_with = "flow_scene_v9_schema_version")]
    #[ts(type = "9")]
    pub result_schema_version: u32,
    /// Must equal `flow-scene/9`.
    #[schemars(schema_with = "flow_scene_v9_revision")]
    #[ts(type = "\"flow-scene/9\"")]
    pub frame_revision: String,
    /// Canonical committed event identity, represented as a decimal string.
    pub event_id: String,
    /// Total number of replayable trace events as an exact decimal.
    pub event_count: String,
    /// Whether a solver result is present in this publication.
    pub solve_status: FlowSolveStatusV1,
    /// Machine-readable reason attached to a terminal resource-limit boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resource_limit_reason: Option<FlowResourceLimitReasonV1>,
    /// Problem semantics.
    pub model: FlowProblemModelV1,
    /// Materialized graph declarations used by the renderer.
    pub graph: FlowGraphV1,
    /// Selected catalog entry.
    pub algorithm: FlowAlgorithmSelectionV1,
    /// Execution profile.
    pub run_profile: RunProfileV1,
    /// Requested trace detail.
    pub trace_granularity: TraceGranularityV1,
    /// Catalog-owned meaning and availability of playback boundaries.
    pub trace_steps: FlowAlgorithmStepContractV1,
    /// Current original-edge flows keyed by stable edge identity.
    pub edge_states: Vec<FlowEdgeStateV1>,
    /// Both residual directions, including zero-capacity arcs.
    pub residual_arcs: Vec<FlowResidualArcStateV1>,
    /// Current algorithm annotations in canonical node order.
    pub node_trace_states: Vec<FlowNodeTraceStateV1>,
    /// Normalized pseudoflow forest, absent for algorithms without this overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pseudoflow_forest: Option<FlowPseudoflowForestV1>,
    /// Dedicated Excesses-IBFS pseudoflow forest. Never combined with the
    /// recovered certified-flow boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub eibfs_overlay: Option<FlowEibfsOverlayV1>,
    /// Dynamic EIBFS update, repair, and prefix-certification context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dynamic_eibfs_overlay: Option<FlowDynamicEibfsOverlayV1>,
    /// Exact affine-capacity and traversal state for parametric maximum flow.
    /// Generic integer flow/residual collections stay empty when this is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parametric_overlay: Option<FlowParametricOverlayV1>,
    /// Actual lower-bound shift, super-terminal construction, auxiliary
    /// Push--Relabel routing, and cut/extraction state used for initialization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub feasibility_overlay: Option<FlowFeasibilityOverlayV2>,
    /// Allocation-free aggregate of feasibility work retained by a fast
    /// execution. This is deliberately separate from algorithm-primary metrics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub feasibility_work: Option<FlowFeasibilityWorkSummaryV1>,
    /// Standalone Goldberg--Rao binary blocking-flow subproblem state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub binary_blocking_overlay: Option<FlowBinaryBlockingOverlayV1>,
    /// Exact rational Cancel-and-Tighten prices and admissible subgraph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cancel_tighten_overlay: Option<FlowCancelTightenOverlayV1>,
    /// Exact dyadic scale, split-node assignment, and selected negative cycles
    /// for relaxed most-negative-cycle canceling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub relaxed_mndc_overlay: Option<FlowRelaxedMndcOverlayV1>,
    /// Orlin enhanced RHS-scaling quotient components, exact dyadic
    /// pseudoflow, dual prices, and active shortest path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub enhanced_capacity_scaling_overlay: Option<FlowEnhancedCapacityScalingOverlayV1>,
    /// Orlin finite-capacity transformation, transformed branches, quotient
    /// components, and compressed shortest-path state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub orlin_mcf_overlay: Option<FlowOrlinMcfOverlayV1>,
    /// Orlin 2013 improvement phases, abundant contractions, compact network,
    /// capacity transfers, pseudo-arc lifting, and expansion state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub orlin_max_flow_overlay: Option<FlowOrlinMaxFlowOverlayV1>,
    /// Unit-current electrical potentials, PCG residuals, oriented currents,
    /// energy, congestion, and exact-reference certificate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub electrical_flow_overlay: Option<FlowElectricalFlowOverlayV1>,
    /// Mądry augmenting-electrical central flow, coupled potentials,
    /// residual-barrier direction, boost expansion, and exact cleanup state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub augmenting_electrical_overlay: Option<FlowAugmentingElectricalOverlayV1>,
    /// Mądry 2013 unit-capacity flow-to-matching reduction, central path,
    /// electrical descent/centering directions, and source b-matching recovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub interior_point_max_flow_overlay: Option<FlowInteriorPointMaxFlowOverlayV1>,
    /// Exact bounded minimum-ratio-cycle objective, forest, candidate, and
    /// selected signed circulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub minimum_ratio_cycle_overlay: Option<FlowMinimumRatioCycleOverlayV1>,
    /// Chen et al. MCF alpha-power potential, strict relative-interior flow,
    /// exact ratio-cycle query, and one source-scaled progress step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub minimum_ratio_cycle_mcf_overlay: Option<FlowMinimumRatioCycleMcfOverlayV1>,
    /// Chen et al. randomized MCF isolation, relative-interior potential,
    /// sampled tree chain, lazy Detect refresh, and exact bounded recovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub randomized_almost_linear_mcf_overlay: Option<FlowRandomizedAlmostLinearMcfOverlayV1>,
    /// Bounded source Flow Framework iterations, exact ratio cycle, gap,
    /// periodic reinitialization, and certified Kang--Payor termination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub flow_framework_mcf_overlay: Option<FlowFrameworkMcfOverlayV1>,
    /// Bernstein et al. capacity prefixes, directed expander hierarchy,
    /// respecting order, weighted labels, admissible arcs, and path list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub weighted_augmenting_paths_overlay: Option<FlowWeightedAugmentingPathsOverlayV1>,
    /// Bernstein et al. weak SCC hierarchy, Steiner-star shortcut graph,
    /// weighted labels, admissible arcs, distance cut, and exact repair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub weighted_push_relabel_shortcut_overlay: Option<FlowWeightedPushRelabelShortcutOverlayV1>,
    /// Chen et al. return-edge reduction, artificial initial point, seeded
    /// finite tree chain, source potential steps, and exact bounded repair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub randomized_almost_linear_overlay: Option<FlowRandomizedAlmostLinearOverlayV1>,
    /// Deterministic shifted tree chain, contracted core, explicit spanner
    /// embeddings, shift/pass game, source potential, and bounded repair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub deterministic_almost_linear_overlay: Option<FlowDeterministicAlmostLinearOverlayV1>,
    /// Becker--Karrenbauer--Mehlhorn integer central path, sticky minor,
    /// randomized cycle centering, crossover tree, and admissible recovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub primal_dual_ipm_mcf_overlay: Option<FlowPrimalDualIpmMcfOverlayV1>,
    /// Daitch--Spielman standard-MCF isolation, electrical Newton systems,
    /// dual short-step central path, and nearest-integer exact recovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub electrical_ipm_mcf_overlay: Option<FlowElectricalIpmMcfOverlayV1>,
    /// Dual-feasible tree basis, signed basic flow, cut, and pivot state for
    /// the natural dual network simplex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dual_network_simplex_overlay: Option<FlowDualNetworkSimplexOverlayV1>,
    /// Exact dyadic pseudoflow, excess, rooted tree, and `Make-Good` pivot
    /// state for polynomial dual network simplex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub polynomial_dual_simplex_overlay: Option<FlowPolynomialDualSimplexOverlayV1>,
    /// Exact epsilon-scaling premultiplier state for Orlin's polynomial
    /// primal network simplex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub polynomial_primal_simplex_overlay: Option<FlowPolynomialPrimalSimplexOverlayV1>,
    /// Exact transportation-network state for nested cost/capacity scaling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub double_scaling_overlay: Option<FlowDoubleScalingOverlayV1>,
    /// Exact segment occupancy and marginal costs for native convex flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub convex_cost_overlay: Option<FlowConvexCostOverlayV1>,
    /// Compact convex-simplex basis, artificial root, and fundamental cycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub convex_network_simplex_overlay: Option<FlowConvexNetworkSimplexOverlayV1>,
    /// Raw prediction, Algorithm 1 preprocessing, robust exponent attempts,
    /// and epsilon-relaxation state from Chen--Yao--Yin Algorithm 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub prediction_assisted_epsilon_overlay: Option<FlowPredictionAssistedEpsilonOverlayV1>,
    /// Tardos network-matrix epsilon measurement and fixed-variable proof.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tardos_framework_overlay: Option<FlowTardosFrameworkOverlayV1>,
    /// Event metadata at the current nonzero trace boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trace_event: Option<FlowTraceEventSceneV1>,
    /// Common semantic header derived for every generic and custom producer by
    /// the publication timeline normalizer. Absent only at the input boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trace_event_semantics: Option<FlowTraceEventSemanticsV1>,
    /// Independently verified optimum data, absent before solve completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub outcome: Option<FlowOutcomeV1>,
    /// Absolute catalog-ordered metric values as exact decimals.
    pub metrics: [String; FLOW_METRIC_COUNT],
}

fn flow_scene_v9_schema_version(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "integer",
        "const": 9,
    })
}

fn flow_scene_v9_revision(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "const": FRAME_ENCODING_REVISION,
    })
}

fn flow_feasibility_overlay_v2_revision(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "const": "flow-feasibility-overlay/2",
    })
}

macro_rules! literal_string_schema {
    ($name:ident => [$($value:literal),+ $(,)?]) => {
        fn $name(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
            schemars::json_schema!({
                "type": "string",
                "enum": [$($value),+],
            })
        }
    };
}

literal_string_schema!(flow_direction_schema => ["forward", "reverse"]);
literal_string_schema!(flow_ternary_sign_schema => ["-1", "0", "1"]);
literal_string_schema!(flow_nonzero_sign_schema => ["-1", "1"]);
literal_string_schema!(flow_eibfs_membership_schema => ["free", "source", "sink"]);
literal_string_schema!(flow_eibfs_root_kind_schema => [
    "none", "source", "sink", "excess", "deficit",
]);
literal_string_schema!(flow_source_sink_schema => ["source", "sink"]);
literal_string_schema!(flow_flow_slack_schema => ["flow", "slack"]);
literal_string_schema!(flow_lower_upper_schema => ["lower", "upper"]);
literal_string_schema!(flow_original_shortcut_schema => ["original", "shortcut"]);
literal_string_schema!(flow_dynamic_eibfs_stage_schema => [
    "initial-solve",
    "apply-update",
    "repair-capacity",
    "repair-forest",
    "repair-violation",
    "continue-solve",
    "prefix-recovery",
    "prefix-certified",
    "resume-reusable-pseudoflow",
]);
literal_string_schema!(flow_dynamic_eibfs_violation_schema => [
    "over-capacity",
    "bridge",
    "label",
    "current-arc",
    "boundary",
]);

/// Generates the normative draft 2020-12 structural schema for scene V9.
///
/// Algorithm-specific arithmetic, graph, and certificate invariants are
/// deliberately checked by independent Rust and TypeScript semantic validators.
#[must_use]
pub fn flow_scene_schema_v9() -> schemars::Schema {
    schemars::generate::SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<FlowCurrentSceneV9>()
}

/// Renderer-facing state of one stable residual direction.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowResidualArcStateV1 {
    /// Stable original edge identity.
    pub edge_id: String,
    /// `forward` adds original flow; `reverse` removes it.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
    /// Tail node at this residual direction.
    pub from: String,
    /// Head node at this residual direction.
    pub to: String,
    /// Exact nonnegative residual capacity.
    pub capacity: String,
    /// Exact signed residual unit cost.
    pub cost: String,
    /// Whether the current event's selected path contains this arc.
    pub active: bool,
    /// Whether the original arc is temporarily excluded by a fixing heuristic.
    #[serde(default, skip_serializing_if = "is_false")]
    pub fixed: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(value: &bool) -> bool {
    !*value
}

/// Stable identity of one residual direction used by an algorithm overlay.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowResidualArcRefV1 {
    /// Original edge identity.
    pub edge_id: String,
    /// `forward` or `reverse` relative to the original edge.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
}

/// Stable role of a node in the lower-bound feasibility transformation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowFeasibilityNodeKindV1 {
    /// A node declared by the input graph.
    Original,
    /// Artificial source that supplies positive shifted imbalance.
    SuperSource,
    /// Artificial sink that receives negative shifted imbalance.
    SuperSink,
}

/// Collision-free reference to an original or artificial feasibility node.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFeasibilityNodeRefV1 {
    /// Structural node role.
    pub kind: FlowFeasibilityNodeKindV1,
    /// Stable input identity, present exactly for an original node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub original_node_id: Option<String>,
}

/// Stable role of a logical edge in the feasibility transformation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowFeasibilityArcKindV1 {
    /// Lower-shifted width of an input edge.
    Original,
    /// Temporary sink-to-source circulation edge for maximum flow.
    LowerBoundReturn,
    /// Artificial edge leaving the super-source.
    FromSuperSource,
    /// Artificial edge entering the super-sink.
    ToSuperSink,
}

/// Collision-free reference to one logical feasibility edge.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFeasibilityArcRefV1 {
    /// Structural edge role.
    pub kind: FlowFeasibilityArcKindV1,
    /// Stable input-edge identity for an original logical edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub original_edge_id: Option<String>,
    /// Original endpoint that owns a super-terminal imbalance edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub imbalance_node_id: Option<String>,
    /// Original tail of the temporary return edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub return_from: Option<String>,
    /// Original head of the temporary return edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub return_to: Option<String>,
}

/// One focused residual direction of a logical feasibility edge.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFeasibilityResidualArcRefV1 {
    /// Logical edge identity.
    pub arc: FlowFeasibilityArcRefV1,
    /// `forward` adds logical flow and `reverse` cancels it.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
}

/// Replay stage of the actual lower-bound feasibility constructor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowFeasibilityStageV1 {
    /// No source operation has run yet.
    Ready,
    /// One lower-shifted input edge was materialized.
    AddOriginalArc,
    /// The temporary sink-to-source edge was materialized.
    AddReturnArc,
    /// One shifted node imbalance was inspected.
    InspectNodeImbalance,
    /// One super-terminal imbalance edge was materialized.
    AddImbalanceArc,
    /// The super-source height was initialized.
    InitializeSourceHeight,
    /// One super-source adjacency entry was inspected.
    InspectSourceArc,
    /// One positive-excess node entered the FIFO queue.
    ActivateNode,
    /// One FIFO active node was selected.
    SelectActiveNode,
    /// One current adjacency entry was inspected.
    InspectDischargeArc,
    /// One adjacency entry was inspected for relabeling.
    InspectRelabelArc,
    /// Flow was pushed through one auxiliary residual direction.
    Push,
    /// One current-arc cursor advanced.
    AdvanceCurrentArc,
    /// One active node was relabeled.
    Relabel,
    /// One active-node discharge completed.
    CompleteDischarge,
    /// The routed imbalance total was published.
    CompleteRouting,
    /// One residual adjacency entry was inspected by the cut BFS.
    InspectCutArc,
    /// One node became reachable in the cut BFS.
    MarkReachable,
    /// One original flow was checked during extraction.
    ExtractOriginalFlow,
    /// The transformed circulation was proved feasible.
    Feasible,
    /// The residual cut proved infeasibility.
    Infeasible,
}

/// Semantic role of one captured feasibility execution in the enclosing run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowFeasibilityUseV1 {
    /// Constructs the public initial flow consumed by the selected algorithm.
    InitialFlow,
    /// Certifies feasibility without changing the enclosing public flow state.
    PrecheckOnly,
    /// Recovers an algorithm-owned transformed state at an explicit trace anchor.
    AnchoredRecovery,
}

/// All source-owned data needed to project one auxiliary feasibility event.
///
/// Grouping these values keeps the projection boundary explicit: the public
/// graph supplies the enclosing algorithm identity, while the kernel graph and
/// captured request/snapshot supply the exact transformed computation.
pub struct FlowAuxiliaryFeasibilityProjection<'a> {
    /// Public input graph owned by the enclosing algorithm.
    pub public_graph: &'a FlowNetwork,
    /// Exact network passed to the feasibility kernel.
    pub kernel_graph: &'a FlowNetwork,
    /// Exact request captured at the source call site.
    pub request: &'a CapturedFeasibilityRequest,
    /// Replayed feasibility state after the current event.
    pub snapshot: &'a FeasibilityTraceSnapshot,
    /// Current source event, when projecting a non-event boundary.
    pub event: Option<&'a FeasibilityTraceEvent>,
    /// Stable event identity in the enclosing public trace.
    pub event_id: u64,
    /// Total event count when already known by the caller.
    pub event_count: u64,
    /// Semantic role of this feasibility execution in the enclosing run.
    pub use_kind: FlowFeasibilityUseV1,
}

/// Relationship between the feasibility kernel's exact input network and the
/// public graph underneath the algorithm trace.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowFeasibilityDomainKindV1 {
    /// The feasibility kernel received the public input graph unchanged.
    PublicInput,
    /// Every kernel node has the same stable identity as a public node, while
    /// one or more edge declarations differ.
    NodeAlignedTransformation,
    /// At least one kernel node has no public stable-identity counterpart.
    StandaloneTransformation,
}

/// One exact node declaration in the network passed to the feasibility
/// kernel.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFeasibilityDomainNodeV1 {
    /// Stable identity inside the kernel input graph.
    pub node_id: String,
    /// Equal stable identity in the public graph, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub public_node_id: Option<String>,
}

/// One exact edge declaration in the network passed to the feasibility
/// kernel. Costs are intentionally absent because the feasibility constructor
/// never reads them.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFeasibilityDomainEdgeV1 {
    /// Stable kernel-edge identity.
    pub edge_id: String,
    /// Stable kernel tail-node identity.
    pub from_node_id: String,
    /// Stable kernel head-node identity.
    pub to_node_id: String,
    /// Immutable lower bound consumed by the lower-bound shift.
    pub lower: String,
    /// Immutable upper capacity consumed by the lower-bound shift.
    pub capacity: String,
    /// Public edge whose exact route may be reused for presentation. This is
    /// present only when identity and endpoint direction both match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub public_route_edge_id: Option<String>,
}

/// One node's exact target outflow-minus-inflow value.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFeasibilityRequiredDivergenceV1 {
    /// Stable kernel-node identity.
    pub node_id: String,
    /// Canonical signed decimal target.
    pub required_divergence: String,
}

/// Exact request passed to the feasibility constructor.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum FlowFeasibilityRequestV1 {
    /// A lower-bounded circulation with one target divergence per node.
    Balance {
        /// Canonical kernel-node order and exact targets.
        required_divergences: Vec<FlowFeasibilityRequiredDivergenceV1>,
    },
    /// A lower-bounded maximum-flow initialization with one temporary return
    /// edge from sink to source.
    MaxFlowInitial {
        /// Stable public/kernel source identity.
        source_node_id: String,
        /// Stable public/kernel sink identity.
        sink_node_id: String,
    },
}

/// Self-contained, revisioned input domain for one feasibility trace. It lets
/// a strict consumer validate construction prefixes even when the source
/// algorithm invokes the kernel on a transformed network.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFeasibilityDomainV1 {
    /// Exact relationship to the public graph.
    pub kind: FlowFeasibilityDomainKindV1,
    /// Canonically ordered kernel nodes.
    pub nodes: Vec<FlowFeasibilityDomainNodeV1>,
    /// Canonically ordered kernel edges.
    pub edges: Vec<FlowFeasibilityDomainEdgeV1>,
    /// Exact source request executed on this network.
    pub request: FlowFeasibilityRequestV1,
}

/// One original or artificial node at a feasibility replay boundary.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFeasibilityNodeStateV1 {
    /// Collision-free node identity.
    pub node: FlowFeasibilityNodeRefV1,
    /// Current Push--Relabel height.
    pub height: String,
    /// Current nonnegative auxiliary excess.
    pub excess: String,
    /// Current adjacency cursor.
    pub current_arc: String,
    /// Whether this node is currently queued as active.
    pub active: bool,
    /// Whether this node belongs to the infeasibility-cut source side.
    pub reachable: bool,
    /// Zero-based FIFO position when active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub queue_position: Option<String>,
}

/// One logical auxiliary edge at a feasibility replay boundary.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFeasibilityArcStateV1 {
    /// Collision-free logical edge identity.
    pub arc: FlowFeasibilityArcRefV1,
    /// Logical forward tail.
    pub from: FlowFeasibilityNodeRefV1,
    /// Logical forward head.
    pub to: FlowFeasibilityNodeRefV1,
    /// Immutable lower-shifted capacity.
    pub capacity: String,
    /// Current logical forward flow.
    pub flow: String,
    /// Current forward residual capacity.
    pub forward_residual: String,
    /// Current reverse residual capacity.
    pub reverse_residual: String,
    /// Whether this logical edge contains the current focused direction.
    pub focused: bool,
    /// Focused residual direction, absent off the current edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub focused_direction: Option<String>,
}

/// Exact counters owned by the actual feasibility source execution.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFeasibilityMetricsV1 {
    /// Input edges inspected while shifting lower bounds.
    pub original_edge_inspections: String,
    /// Input nodes inspected while constructing imbalance edges.
    pub original_node_inspections: String,
    /// Auxiliary adjacency entries inspected by Push--Relabel.
    pub auxiliary_adjacency_inspections: String,
    /// Local auxiliary pushes.
    pub pushes: String,
    /// Strict height increases.
    pub relabels: String,
    /// FIFO active-node selections.
    pub active_node_selections: String,
    /// Completed discharges.
    pub discharges: String,
    /// Cut-BFS adjacency inspections.
    pub cut_adjacency_inspections: String,
    /// Original flows checked during extraction.
    pub extracted_original_edges: String,
}

/// Aggregate feasibility work retained without a reversible event stream.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFeasibilityWorkSummaryV1 {
    /// Number of actual feasibility-kernel invocations in the source run.
    pub invocations: String,
    /// Exact counters summed across those invocations.
    pub metrics: FlowFeasibilityMetricsV1,
}

/// Dedicated projection of the real lower-bound feasibility subroutine.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFeasibilityOverlayV2 {
    /// Nested contract revision. The containing scene revision is unchanged
    /// because this independently versioned payload is rejected atomically by
    /// older strict decoders.
    #[schemars(schema_with = "flow_feasibility_overlay_v2_revision")]
    #[ts(type = "\"flow-feasibility-overlay/2\"")]
    pub revision: String,
    /// How this feasibility execution contributes to the enclosing algorithm.
    pub use_kind: FlowFeasibilityUseV1,
    /// Exact input graph, request, and public-identity relationship.
    pub domain: FlowFeasibilityDomainV1,
    /// Current source boundary.
    pub stage: FlowFeasibilityStageV1,
    /// Original and artificial node states.
    pub nodes: Vec<FlowFeasibilityNodeStateV1>,
    /// Logical original, return, and super-terminal edges.
    pub arcs: Vec<FlowFeasibilityArcStateV1>,
    /// FIFO active queue from front to back.
    pub active_queue: Vec<FlowFeasibilityNodeRefV1>,
    /// Locally focused node, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub focus_node: Option<FlowFeasibilityNodeRefV1>,
    /// Locally focused residual direction, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub focus_arc: Option<FlowFeasibilityResidualArcRefV1>,
    /// Total positive shifted imbalance to route.
    pub total_required: String,
    /// Amount delivered to the super-sink after routing completes.
    pub routed: String,
    /// Exact source-work counters.
    pub metrics: FlowFeasibilityMetricsV1,
}

/// Publication boundary within one standalone binary blocking-flow primitive.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowBinaryBlockingStageV1 {
    /// The current residual arc is being inspected; aggregate classes and SCCs
    /// are not available yet.
    Analyzing,
    /// Binary lengths, exact distances, and admissible arcs are available.
    Analyzed,
    /// Zero-length admissible SCCs are exposed before the lifted update.
    Contracted,
    /// The contracted augmentation and internal lift are atomically committed.
    Complete,
}

/// Exact node classification for a standalone binary blocking-flow step.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowBinaryBlockingNodeStateV1 {
    /// Stable original-node identity.
    pub node_id: String,
    /// Exact binary distance to the sink, absent when unreachable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub distance: Option<String>,
    /// Canonical zero-length SCC ordinal.
    pub component: String,
}

/// Dedicated structural projection of one binary blocking-flow primitive.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowBinaryBlockingOverlayV1 {
    /// Current semantic boundary.
    pub stage: FlowBinaryBlockingStageV1,
    /// Positive valid upper bound on the residual flow gap.
    pub upper_bound: String,
    /// Positive integral update cap selected for the phase.
    pub delta: String,
    /// Flow value delivered by the complete primitive, or zero before commit.
    pub delivered: String,
    /// Exact node distances and zero-SCC membership.
    pub nodes: Vec<FlowBinaryBlockingNodeStateV1>,
    /// Residual arcs whose base binary length is zero.
    pub base_zero_arcs: Vec<FlowResidualArcRefV1>,
    /// Residual arcs receiving the source-defined special zero length.
    pub special_arcs: Vec<FlowResidualArcRefV1>,
    /// Complete corrected admissible residual subgraph.
    pub admissible_arcs: Vec<FlowResidualArcRefV1>,
    /// Zero-length admissible arcs contracted into SCCs.
    pub zero_admissible_arcs: Vec<FlowResidualArcRefV1>,
}

/// Publication boundary within exact rational Cancel-and-Tighten.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowCancelTightenStageV1 {
    /// Feasible flow and initial error bound before publication.
    Ready,
    /// Zero price and initial epsilon were published.
    Initialize,
    /// A new cancel/tighten phase began.
    BeginPhase,
    /// One residual arc was inspected for admissible-cycle detection.
    InspectCycleArc,
    /// One all-admissible residual cycle is selected.
    SelectCycle,
    /// The selected residual cycle was saturated.
    CancelCycle,
    /// One residual arc was inspected for admissible-DAG ranking.
    InspectRankArc,
    /// A topological rank tightened every exact price.
    Tighten,
    /// The independent minimum-cost certificate succeeded.
    Optimal,
}

/// Exact Cancel-and-Tighten state attached to one original node.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowCancelTightenNodeStateV1 {
    /// Stable original-node identity.
    pub node_id: String,
    /// Reduced canonical exact price.
    pub potential: FlowRationalV1,
    /// Stable topological ordinal at a tighten boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rank: Option<String>,
}

/// Dedicated projection of exact rational Cancel-and-Tighten state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowCancelTightenOverlayV1 {
    /// Current semantic boundary.
    pub stage: FlowCancelTightenStageV1,
    /// Exact current epsilon-optimality error.
    pub epsilon: FlowRationalV1,
    /// One-based outer phase, or zero before the first phase.
    pub phase: String,
    /// Canonical node prices and optional topological ranks.
    pub nodes: Vec<FlowCancelTightenNodeStateV1>,
    /// Every positive residual arc with strictly negative reduced cost.
    pub admissible_arcs: Vec<FlowResidualArcRefV1>,
    /// Ordered residual cycle selected by the current boundary.
    pub active_cycle: Vec<FlowResidualArcRefV1>,
    /// Concrete residual operand inspected at this source boundary.
    pub inspected_arcs: Vec<FlowResidualArcRefV1>,
    /// Exact bottleneck committed by a cancel-cycle event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delta: Option<String>,
}

/// Publication boundary within relaxed most-negative-cycle canceling.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowRelaxedMndcStageV1 {
    /// Feasible flow before the source algorithm initializes epsilon.
    Ready,
    /// Initial `epsilon = C` state.
    Initialize,
    /// Epsilon was halved for a new outer phase.
    BeginPhase,
    /// One concrete positive residual arc was inspected while building the
    /// split-node assignment graph.
    InspectResidualArc,
    /// One concrete Hungarian row/column cell was inspected.
    InspectAssignmentCell,
    /// The exact split-node assignment selected a negative cycle family.
    SelectFamily,
    /// Every selected node-disjoint cycle was pushed to its bottleneck.
    CancelFamily,
    /// Nonnegative assignment value proves epsilon-optimality for this phase.
    PhaseOptimal,
    /// Independent exact minimum-cost certification succeeded.
    Optimal,
}

/// One split-node assignment row and its exact dual values.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowRelaxedMndcNodeStateV1 {
    /// Stable identity of the original node and assignment left copy.
    pub node_id: String,
    /// Stable identity of the matched right copy.
    pub matched_node_id: String,
    /// Exact Hungarian left-copy dual.
    pub left_dual: String,
    /// Exact Hungarian right-copy dual for this node's right copy.
    pub right_dual: String,
    /// Selected residual arc, absent for an artificial identity match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selected_arc: Option<FlowResidualArcRefV1>,
}

/// One negative node-disjoint cycle selected by the assignment.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowRelaxedMndcCycleV1 {
    /// Exact assignment-domain cost after the epsilon shift.
    pub transformed_cost: String,
    /// Ordered residual cycle.
    pub arcs: Vec<FlowResidualArcRefV1>,
    /// Bottleneck committed at a cancel-family boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delta: Option<String>,
}

/// Concrete split-node matrix operand inspected by the Hungarian kernel.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowRelaxedMndcAssignmentCellV1 {
    /// Original node owning the left-copy row.
    pub row_node_id: String,
    /// Original node owning the right-copy column.
    pub column_node_id: String,
}

/// Dedicated projection of the nested epsilon/assignment/cycle-family state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowRelaxedMndcOverlayV1 {
    /// Current semantic boundary.
    pub stage: FlowRelaxedMndcStageV1,
    /// Exact dyadic outer relaxation parameter.
    pub epsilon: FlowRationalV1,
    /// One-based outer phase, or zero before the first halving.
    pub phase: String,
    /// Exact assignment optimum, absent outside assignment boundaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assignment_value: Option<String>,
    /// Canonical assignment rows and left/right dual values.
    pub nodes: Vec<FlowRelaxedMndcNodeStateV1>,
    /// Strictly negative node-disjoint cycle family.
    pub family: Vec<FlowRelaxedMndcCycleV1>,
    /// Concrete residual operand inspected at this boundary.
    pub inspected_arcs: Vec<FlowResidualArcRefV1>,
    /// Concrete split-node assignment operand inspected at this boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_assignment_cell: Option<FlowRelaxedMndcAssignmentCellV1>,
}

/// Publication boundary in Orlin's enhanced RHS-capacity-scaling algorithm.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowEnhancedCapacityScalingStageV1 {
    /// Lower-bound-shifted zero pseudoflow.
    Ready,
    /// Globally dual-feasible prices were installed.
    Initialize,
    /// Delta jumped to the largest quotient imbalance.
    CompleteRegeneration,
    /// A new delta phase began.
    BeginPhase,
    /// A strongly feasible tight arc merged two components.
    Contract,
    /// One original residual direction was inspected.
    InspectResidualArc,
    /// An active source/sink pair and quotient shortest path were selected.
    SelectPath,
    /// Exact delta was sent and prices were updated.
    Augment,
    /// No active source/sink pair remains at the current delta.
    CompletePhase,
    /// Delta was halved exactly.
    HalveScale,
    /// A feasible original flow was recovered on zero-reduced-cost arcs.
    RecoverPrimal,
    /// Independent minimum-cost certification succeeded.
    Optimal,
}

/// One contracted quotient component and its exact aggregate imbalance.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowEnhancedCapacityScalingComponentV1 {
    /// Stable component identity: the smallest member node ID.
    pub component_id: String,
    /// Canonically ordered original members.
    pub members: Vec<String>,
    /// Canonically reduced exact aggregate excess projected from the common dyadic scale.
    pub excess: FlowRationalV1,
}

/// Original-node projection of a quotient component and dual state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowEnhancedCapacityScalingNodeStateV1 {
    /// Stable original-node identity.
    pub node_id: String,
    /// Stable identity of the containing quotient component.
    pub component_id: String,
    /// Exact original-node dual potential.
    pub potential: String,
    /// Quotient shortest-path distance at select/augment boundaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub distance: Option<String>,
}

/// Original-edge projection of exact virtual pseudoflow and dual slack.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowEnhancedCapacityScalingEdgeStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Exact lower-bound-shifted virtual flow.
    pub virtual_flow: FlowRationalV1,
    /// Exact forward reduced cost.
    pub reduced_cost: String,
    /// Whether both endpoints already belong to one quotient component.
    pub internal: bool,
    /// Whether the source contraction threshold is met at this boundary.
    pub strongly_feasible: bool,
    /// Whether the forward reduced cost is zero.
    pub tight: bool,
}

/// Dedicated projection of Orlin's quotient scaling state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowEnhancedCapacityScalingOverlayV1 {
    /// Current semantic boundary.
    pub stage: FlowEnhancedCapacityScalingStageV1,
    /// Exact current delta.
    pub delta: FlowRationalV1,
    /// One-based phase count, or zero before the first phase.
    pub phase: String,
    /// Canonically ordered quotient components.
    pub components: Vec<FlowEnhancedCapacityScalingComponentV1>,
    /// Canonically ordered original-node state.
    pub nodes: Vec<FlowEnhancedCapacityScalingNodeStateV1>,
    /// Canonically ordered original-edge state.
    pub edges: Vec<FlowEnhancedCapacityScalingEdgeStateV1>,
    /// Active source quotient component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_component: Option<String>,
    /// Active sink quotient component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sink_component: Option<String>,
    /// Selected quotient residual path.
    pub path: Vec<FlowResidualArcRefV1>,
    /// Strongly feasible forward arc contracted by this boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub contraction_arc: Option<String>,
    /// Exact amount committed at an augment boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub augmentation: Option<FlowRationalV1>,
}

/// Publication boundary in Orlin's capacitated strongly-polynomial MCF algorithm.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowOrlinMcfStageV1 {
    /// Original bounded instance is ready.
    Ready,
    /// Every positive-width edge has become a capacity node and two branches.
    TransformCapacities,
    /// Globally dual-feasible transformed prices were installed.
    InitializeDual,
    /// Delta jumped to the largest quotient imbalance.
    CompleteRegeneration,
    /// A new exact-dyadic delta phase began.
    BeginPhase,
    /// One transformed residual branch was checked for contraction.
    InspectContractibleArc,
    /// One transformed residual branch was checked during reverse reachability.
    InspectReachabilityArc,
    /// One transformed residual branch was classified for quotient compression.
    InspectCompressedResidualArc,
    /// One compressed residual segment was relaxed.
    InspectCompressedArc,
    /// A strongly feasible tight branch merged two transformed components.
    Contract,
    /// A path was selected after eliminating uncontracted capacity nodes.
    SelectCompressedPath,
    /// Exact delta was sent and transformed prices were updated.
    Augment,
    /// No reachable active pair remains at the current delta.
    CompletePhase,
    /// Delta was halved exactly.
    HalveScale,
    /// Contracted prices were expanded to every transformed node.
    ExpandDual,
    /// A feasible original flow was recovered on zero-reduced-cost branches.
    RecoverPrimal,
    /// Independent minimum-cost certification succeeded.
    Optimal,
}

/// Stable transformed-node role.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowOrlinMcfNodeKindV1 {
    /// Node inherited from the original bounded graph.
    Original,
    /// Demand node replacing one positive-width original edge.
    Capacity,
}

/// Flow or slack branch incident to a transformed capacity node.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowOrlinMcfBranchV1 {
    /// Costed original-tail to capacity-node branch.
    Flow,
    /// Zero-cost original-head to capacity-node branch.
    Slack,
}

/// One residual branch identity in the transformed graph.
#[derive(Clone, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowOrlinMcfArcRefV1 {
    /// Original positive-width edge identity.
    pub edge_id: String,
    /// Flow or slack branch.
    pub branch: FlowOrlinMcfBranchV1,
    /// Forward or reverse transformed residual direction.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
}

/// One transformed quotient component.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowOrlinMcfComponentV1 {
    /// Stable component identity: the first transformed member identity.
    pub component_id: String,
    /// Canonically ordered original and capacity-node members.
    pub members: Vec<String>,
    /// Exact aggregate component imbalance.
    pub excess: FlowRationalV1,
}

/// One original or capacity node at an Orlin MCF boundary.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowOrlinMcfNodeStateV1 {
    /// Stable identity: original node id or `capacity:<edge-id>`.
    pub node_id: String,
    /// Original or transformed-capacity role.
    pub kind: FlowOrlinMcfNodeKindV1,
    /// Original edge represented by a capacity node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capacity_edge_id: Option<String>,
    /// Stable identity of the containing quotient component.
    pub component_id: String,
    /// Exact transformed-node dual potential.
    pub potential: String,
    /// Capped shortest-path dual label at selection and augmentation boundaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub distance: Option<String>,
}

/// One transformed flow/slack branch.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowOrlinMcfArcStateV1 {
    /// Original positive-width edge identity.
    pub edge_id: String,
    /// Costed flow or zero-cost slack branch.
    pub branch: FlowOrlinMcfBranchV1,
    /// Exact nonnegative transformed pseudoflow.
    pub flow: FlowRationalV1,
    /// Exact forward reduced cost.
    pub reduced_cost: String,
    /// Whether both branch endpoints are contracted together.
    pub internal: bool,
    /// Whether the source contraction threshold is met.
    pub strongly_feasible: bool,
    /// Whether the branch is dual-tight.
    pub tight: bool,
}

/// Dedicated finite-capacity transformation and compressed-quotient projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowOrlinMcfOverlayV1 {
    /// Current source-defined boundary.
    pub stage: FlowOrlinMcfStageV1,
    /// Exact current delta.
    pub delta: FlowRationalV1,
    /// One-based phase count, or zero before the first phase.
    pub phase: String,
    /// Canonically ordered transformed quotient components.
    pub components: Vec<FlowOrlinMcfComponentV1>,
    /// Original nodes followed by positive-width capacity nodes.
    pub nodes: Vec<FlowOrlinMcfNodeStateV1>,
    /// Flow then slack branch for each positive-width original edge.
    pub arcs: Vec<FlowOrlinMcfArcStateV1>,
    /// Active source quotient component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_component: Option<String>,
    /// Active sink quotient component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sink_component: Option<String>,
    /// Expanded transformed residual path selected by compressed Dijkstra.
    pub path: Vec<FlowOrlinMcfArcRefV1>,
    /// Exact transformed branch or two-branch shortcut inspected at this boundary.
    pub inspected_segment: Vec<FlowOrlinMcfArcRefV1>,
    /// Monotone residual-arc scan ordinal at an inspection boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub inspection_serial: Option<String>,
    /// Strongly feasible transformed branch contracted at this boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub contraction_arc: Option<FlowOrlinMcfArcRefV1>,
    /// Exact amount committed at an augment boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub augmentation: Option<FlowRationalV1>,
    /// Capacity nodes eliminated in the selected compressed graph.
    pub eliminated_capacity_nodes: String,
    /// Two-arc shortcuts materialized in the selected compressed graph.
    pub shortcut_arcs: String,
}

/// Publication boundary in Orlin's 2013 maximum-flow construction.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowOrlinMaxFlowStageV1 {
    /// Zero flow and the source cut are installed.
    Ready,
    /// A delta-improvement phase begins.
    BeginImprovement,
    /// Abundant cycles or external arcs were contracted.
    ContractAbundant,
    /// A real residual or quotient arc inspection in phase classification was checkpointed.
    InspectClassificationArc,
    /// Residual directions and critical components were classified.
    Classify,
    /// One of the three source cases was selected.
    SelectCase,
    /// A real quotient arc inspection while building the compact network was checkpointed.
    InspectCompactConstructionArc,
    /// Anti-abundant capacity was transferred to a pseudo-arc.
    TransferCapacity,
    /// The logical residual or compact network was materialized.
    BuildSubproblem,
    /// A threshold-residual logical path was augmented.
    AugmentSubproblem,
    /// A real threshold-residual logical arc inspection was checkpointed.
    InspectSubproblemArc,
    /// The logical subproblem reached its target cut.
    CompleteSubproblem,
    /// A real logical-flow decomposition arc inspection was checkpointed.
    InspectDecompositionArc,
    /// A real original residual-route inspection was checkpointed.
    InspectLiftResidualArc,
    /// A positive logical path was lifted to original residual directions.
    LiftPath,
    /// A contracted component was rebalanced on abundant directions.
    ExpandContraction,
    /// A real residual arc inspection while expanding a contraction was checkpointed.
    InspectExpansionResidualArc,
    /// A real residual arc inspection in the next-cut search was checkpointed.
    InspectCutResidualArc,
    /// The next residual source cut was installed.
    UpdateCut,
    /// Independent maximum-flow/minimum-cut certification succeeded.
    Optimal,
}

/// Source-defined branch selected from the critical-component count.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowOrlinMaxFlowPhaseCaseV1 {
    /// Improve the un-compacted residual network approximately.
    OriginalApproximation,
    /// Improve the delta-compact network approximately.
    CompactApproximation,
    /// Solve the `(delta,gamma)` compact network exactly.
    CompactExact,
}

/// Role of one logical compact-network arc.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowOrlinMaxFlowCompactArcKindV1 {
    /// Positive residual capacity between retained components.
    Original,
    /// A `2 delta` abundant-path pseudo-arc.
    AbundantPseudo,
    /// A pseudo-arc created by anti-abundant capacity transfer.
    TransferredPseudo,
}

/// Original-node projection of quotient classification.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowOrlinMaxFlowNodeStateV1 {
    /// Stable original node identity.
    pub node_id: String,
    /// Stable component identity, chosen from its first original member.
    pub component_id: String,
    /// Whether the component is retained in the active compact network.
    pub critical: bool,
    /// Exact anti-abundant potential `incoming - outgoing`.
    pub anti_potential: String,
    /// Whether this node belongs to the current source side.
    pub source_side: bool,
}

/// Source classification of one original residual direction.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowOrlinMaxFlowResidualArcStateV1 {
    /// Stable original edge identity.
    pub edge_id: String,
    /// `forward` or `reverse`.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
    /// Exact current residual capacity.
    pub capacity: String,
    /// Whether the direction is abundant.
    pub abundant: bool,
    /// Whether the direction is anti-abundant.
    pub anti_abundant: bool,
    /// Whether the endpoint-pair capacity is small.
    pub small: bool,
    /// Whether it meets the active medium-capacity test.
    pub medium: bool,
    /// Exact cumulative source scan ordinal while this direction is inspected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub inspection_serial: Option<String>,
}

/// One active compact residual direction.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowOrlinMaxFlowCompactArcRefV1 {
    /// Stable phase-local compact arc ordinal.
    pub ordinal: String,
    /// Whether the logical residual direction reverses the compact arc.
    pub reverse: bool,
}

/// One materialized compact-network arc.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowOrlinMaxFlowCompactArcStateV1 {
    /// Stable phase-local ordinal.
    pub ordinal: String,
    /// Tail component identity.
    pub from_component: String,
    /// Head component identity.
    pub to_component: String,
    /// Original, abundant pseudo, or transferred pseudo role.
    pub kind: FlowOrlinMaxFlowCompactArcKindV1,
    /// Exact logical capacity.
    pub capacity: String,
    /// Exact logical flow.
    pub flow: String,
    /// Expanded original residual witness.
    pub witness: Vec<FlowResidualArcRefV1>,
    /// Exact cumulative source scan ordinal while this logical direction is inspected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub inspection_serial: Option<String>,
}

/// Dedicated Orlin 2013 maximum-flow projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowOrlinMaxFlowOverlayV1 {
    /// Current source boundary.
    pub stage: FlowOrlinMaxFlowStageV1,
    /// Integral current residual cut gap.
    pub delta: String,
    /// Exact small-case gamma.
    pub gamma: FlowRationalV1,
    /// Selected three-way case, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub phase_case: Option<FlowOrlinMaxFlowPhaseCaseV1>,
    /// Original nodes in canonical order.
    pub nodes: Vec<FlowOrlinMaxFlowNodeStateV1>,
    /// Forward then reverse state for every original edge.
    pub residual_arcs: Vec<FlowOrlinMaxFlowResidualArcStateV1>,
    /// Current logical residual/compact network.
    pub compact_arcs: Vec<FlowOrlinMaxFlowCompactArcStateV1>,
    /// Selected logical residual path.
    pub active_compact_path: Vec<FlowOrlinMaxFlowCompactArcRefV1>,
    /// Expanded physical residual directions used by transfer/lift/expansion.
    pub active_original_path: Vec<FlowResidualArcRefV1>,
    /// Current threshold or transferred amount.
    pub threshold: String,
}

/// Publication boundary in the natural dual network simplex.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowDualNetworkSimplexStageV1 {
    /// Lower bounds are shifted but no tree basis exists yet.
    Ready,
    /// One original arc is being inspected while building tentative prices.
    InspectInitialArc,
    /// A shortest-path tree supplies a dual-feasible basis.
    InitializeDualTree,
    /// A negative-flow basic arc was selected.
    SelectLeaving,
    /// One original arc is being priced against the active cut.
    InspectEnteringArc,
    /// A minimum reduced-cost cut arc was selected.
    SelectEntering,
    /// The tree basis and cut-side prices changed atomically.
    Pivot,
    /// The basic tree flow is primal feasible and certified optimal.
    Optimal,
}

/// Original-node price and active cut membership.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowDualNetworkSimplexNodeStateV1 {
    /// Stable original-node identity.
    pub node_id: String,
    /// Exact dual price.
    pub potential: String,
    /// Whether this potential is defined in the current initialization scan.
    pub initialized: bool,
    /// Whether the node lies on the selected leaving arc's head side.
    pub in_cut: bool,
}

/// Original-edge basis, signed basic flow, and dual slack.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowDualNetworkSimplexEdgeStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Exact signed lower-bound-shifted basic flow.
    pub basic_flow: String,
    /// Exact forward reduced cost.
    pub reduced_cost: String,
    /// Whether this edge is in the spanning-tree basis.
    pub in_tree: bool,
}

/// Dedicated projection of the natural dual-network-simplex basis.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowDualNetworkSimplexOverlayV1 {
    /// Current semantic boundary.
    pub stage: FlowDualNetworkSimplexStageV1,
    /// Canonically ordered node state.
    pub nodes: Vec<FlowDualNetworkSimplexNodeStateV1>,
    /// Canonically ordered edge state.
    pub edges: Vec<FlowDualNetworkSimplexEdgeStateV1>,
    /// Canonically ordered head-side cut.
    pub cut_side: Vec<String>,
    /// Selected negative-flow tree arc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub leaving_edge: Option<String>,
    /// Selected minimum reduced-cost replacement arc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entering_edge: Option<String>,
    /// Original edge inspected at this exact source-time checkpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub inspected_edge: Option<String>,
    /// Exact entering reduced cost applied to cut-side prices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pivot_price_delta: Option<String>,
}

/// Publication boundary in Orlin--Plotkin--Tardos `Scaling-Simplex`.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowPolynomialDualSimplexStageV1 {
    /// Domain checks passed before a tree is public.
    Ready,
    /// One original arc was inspected while building the initial dual tree.
    InspectInitialArc,
    /// A dual-feasible shortest-path arborescence was installed.
    InitializeTree,
    /// Root-to-node auxiliary flow made every nonroot excess positive.
    InitializePseudoflow,
    /// A new exact dyadic scale began.
    BeginScale,
    /// One original arc was inspected while constructing an augmentation path.
    InspectAugmentationArc,
    /// A node with excess above delta and its root path were selected.
    SelectActive,
    /// Delta was sent from the active node to the root.
    AugmentToRoot,
    /// `Make-Good` selected a bad subtree and leaving arc.
    SelectBadArc,
    /// One original arc was inspected while pricing the bad-subtree cut.
    InspectEnteringArc,
    /// The minimum reduced-cost arc leaving the bad subtree was selected.
    SelectEntering,
    /// One dual-simplex tree exchange restored more good arcs.
    PivotMakeGood,
    /// The current scale has no active nonroot node.
    FinishScale,
    /// The integral basic tree flow was independently certified.
    Optimal,
}

/// Exact node state in polynomial dual `Scaling-Simplex`.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[allow(clippy::struct_excessive_bools)]
#[serde(deny_unknown_fields)]
pub struct FlowPolynomialDualNodeStateV1 {
    /// Stable original-node identity.
    pub node_id: String,
    /// Exact dual price.
    pub potential: String,
    /// Exact auxiliary excess at the current dyadic scale.
    pub excess: FlowRationalV1,
    /// Whether this node is the fixed root.
    pub root: bool,
    /// Whether this node is active at the current boundary.
    pub active: bool,
    /// Whether its root path contains a zero-flow downward edge.
    pub bad: bool,
    /// Whether it lies on the selected leaving arc's head-side cut.
    pub in_pivot_cut: bool,
}

/// Exact edge state in polynomial dual `Scaling-Simplex`.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPolynomialDualEdgeStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Exact auxiliary tree-supported pseudoflow.
    pub pseudoflow: FlowRationalV1,
    /// Integral signed basic flow induced by the tree and balances.
    pub basic_flow: String,
    /// Exact forward reduced cost.
    pub reduced_cost: String,
    /// Whether this edge belongs to the spanning-tree basis.
    pub in_tree: bool,
    /// Whether this is a zero-flow downward tree arc.
    pub bad: bool,
    /// Whether the selected active-to-root path uses this edge.
    pub in_augment_path: bool,
    /// Selected path orientation when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub augment_direction: Option<String>,
}

/// Dedicated projection of polynomial dual scaling and `Make-Good` state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPolynomialDualSimplexOverlayV1 {
    /// Current semantic boundary.
    pub stage: FlowPolynomialDualSimplexStageV1,
    /// One-based scale phase, or zero before scaling starts.
    pub phase: String,
    /// Exact current dyadic scaling parameter.
    pub delta: FlowRationalV1,
    /// Canonically ordered original-node state.
    pub nodes: Vec<FlowPolynomialDualNodeStateV1>,
    /// Canonically ordered original-edge state.
    pub edges: Vec<FlowPolynomialDualEdgeStateV1>,
    /// Selected active node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_node: Option<String>,
    /// Directed active-to-root path.
    pub augment_path: Vec<FlowResidualArcRefV1>,
    /// All current zero-flow downward tree arcs.
    pub bad_edges: Vec<String>,
    /// All nodes below at least one bad arc.
    pub bad_nodes: Vec<String>,
    /// First bad edge on the root-to-entering-tail path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub leaving_edge: Option<String>,
    /// Minimum reduced-cost edge leaving the bad-node set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entering_edge: Option<String>,
    /// Head-side cut of the selected leaving edge.
    pub pivot_cut: Vec<String>,
    /// Exact cut-price shift.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pivot_price_delta: Option<String>,
}

/// Publication boundary in Orlin's scaling-premultiplier algorithm.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowPolynomialPrimalSimplexStageV1 {
    /// The private perturbed star exists before its first publication.
    Ready,
    /// The exact artificial-star basic feasible solution is public.
    InitializeBasis,
    /// A new epsilon phase initialized `N*`.
    BeginScale,
    /// One concrete extended residual arc was inspected by a scaling search.
    InspectResidual,
    /// An eligible awake tail selected an epsilon/4-admissible arc.
    SelectAdmissible,
    /// A fundamental-cycle primal pivot was committed.
    Pivot,
    /// Eligible-node premultipliers were increased.
    ModifyPremultipliers,
    /// `N*` emptied and epsilon/2-optimality was checked.
    FinishScale,
    /// Artificial flow vanished and the original flow was certified.
    Optimal,
}

/// Lower/tree/upper partition of a primal network-simplex basis.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowPolynomialPrimalBasisStateV1 {
    /// Nonbasic at its lower bound.
    Lower,
    /// Basic spanning-tree arc.
    Tree,
    /// Nonbasic at its upper bound.
    Upper,
}

/// One directed extended residual arc used by selection and cycle overlays.
#[derive(Clone, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPolynomialPrimalResidualRefV1 {
    /// Stable original edge ID or `artificial:<node-id>`.
    pub entity_id: String,
    /// Original edge identity when this is not an artificial star arc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub original_edge_id: Option<String>,
    /// `forward` follows the stored arc and `reverse` cancels it.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
}

/// Original or artificial-root premultiplier state.
#[derive(
    Clone, Copy, Debug, Deserialize, JsonSchema, Eq, Ord, PartialEq, PartialOrd, Serialize, TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum FlowPolynomialPrimalNodeKindV1 {
    /// Original public graph node.
    Original,
    /// Extra phase-I root.
    ArtificialRoot,
}

/// Set-valued node annotations; duplicates are invalid.
#[derive(
    Clone, Copy, Debug, Deserialize, JsonSchema, Eq, Ord, PartialEq, PartialOrd, Serialize, TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum FlowPolynomialPrimalNodeFlagV1 {
    /// Rooted-tree eligible.
    Eligible,
    /// Awake under `N*` or the epsilon/4 grid.
    Awake,
    /// Still in `N*`.
    InNStar,
    /// Current rooted-tree root.
    Root,
}

/// Original or artificial-root premultiplier state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPolynomialPrimalNodeStateV1 {
    /// Stable node ID or `artificial-root`.
    pub entity_id: String,
    /// Distinguishes the extra phase-I root from original nodes.
    pub kind: FlowPolynomialPrimalNodeKindV1,
    /// Exact paper-convention premultiplier.
    pub premultiplier: FlowRationalV1,
    /// Eligible, awake, `N*`, and root memberships.
    pub flags: Vec<FlowPolynomialPrimalNodeFlagV1>,
}

/// Original-edge basis, exact perturbed flow, and reduced cost.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPolynomialPrimalEdgeStateV1 {
    /// Stable original edge identity.
    pub edge_id: String,
    /// Lower/tree/upper basis partition.
    pub basis: FlowPolynomialPrimalBasisStateV1,
    /// Exact lower-shifted perturbed flow after multiplication by `n+1`.
    pub perturbed_flow: String,
    /// Exact unperturbed lower-shifted basic flow.
    pub unperturbed_basic_flow: String,
    /// Exact forward reduced cost under current premultipliers.
    pub reduced_cost: FlowRationalV1,
    /// Whether either residual direction occurs in the fundamental cycle.
    pub in_cycle: bool,
    /// Whether this original edge is the selected entering arc.
    pub entering: bool,
    /// Whether this original edge is the leaving tree arc.
    pub leaving: bool,
}

/// Artificial star-arc state retained after phase-I initialization.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPolynomialPrimalArtificialEdgeStateV1 {
    /// Stable `artificial:<node-id>` identity.
    pub entity_id: String,
    /// Original node joined to the artificial root.
    pub node_id: String,
    /// Lower/tree/upper basis partition.
    pub basis: FlowPolynomialPrimalBasisStateV1,
    /// Exact scaled perturbed flow.
    pub perturbed_flow: String,
    /// Exact unperturbed basic flow.
    pub unperturbed_basic_flow: String,
    /// Cycle membership in either residual direction.
    pub in_cycle: bool,
    /// Selected entering status.
    pub entering: bool,
    /// Selected leaving status.
    pub leaving: bool,
}

/// Dedicated projection of the polynomial primal-network-simplex state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPolynomialPrimalSimplexOverlayV1 {
    /// Current semantic boundary.
    pub stage: FlowPolynomialPrimalSimplexStageV1,
    /// One-based epsilon phase, or zero before scaling.
    pub phase: String,
    /// Current exact epsilon when a scale has begun.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub epsilon: Option<FlowRationalV1>,
    /// Integer scale used for symbolic RHS perturbation.
    pub perturbation_scale: String,
    /// Original nodes followed by the artificial root.
    pub nodes: Vec<FlowPolynomialPrimalNodeStateV1>,
    /// Canonically ordered original-edge state.
    pub edges: Vec<FlowPolynomialPrimalEdgeStateV1>,
    /// Artificial star arcs in canonical original-node order.
    pub artificial_edges: Vec<FlowPolynomialPrimalArtificialEdgeStateV1>,
    /// Selected admissible residual arc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entering: Option<FlowPolynomialPrimalResidualRefV1>,
    /// Stable leaving original or artificial arc identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub leaving_entity: Option<String>,
    /// Directed fundamental cycle, entering residual first.
    pub cycle: Vec<FlowPolynomialPrimalResidualRefV1>,
    /// Exact primal augmentation at a pivot boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delta: Option<FlowRationalV1>,
    /// Exact premultiplier increase at a modify boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub potential_shift: Option<FlowRationalV1>,
}

/// Publication boundary within Pasche's compact convex network simplex.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowConvexNetworkSimplexStageV1 {
    /// Big-M artificial-root star and compact edge states are available.
    InitializeBasis,
    /// Forward and reverse marginal reduced costs were priced.
    Price,
    /// One negative fundamental cycle was formed.
    FormCycle,
    /// The cycle reached one segment or global breakpoint.
    CrossBreakpoint,
    /// The entering edge replaced one final tree edge.
    ExchangeBasis,
    /// The entering edge remained nonbasic at its next breakpoint.
    FlipBound,
    /// Native and expanded optimum certificates agree.
    Optimal,
}

/// Compact basis state of one original convex edge.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowConvexNetworkSimplexBasisV1 {
    /// The selected segment is one edge of the extended spanning tree.
    Tree,
    /// The aggregate edge flow is fixed at a compact breakpoint.
    Breakpoint,
}

/// Original node or artificial root in the extended basis tree.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowConvexNetworkSimplexNodeStateV1 {
    /// Stable original-node identity or `artificial-root`.
    pub entity_id: String,
    /// Exact tree potential.
    pub potential: String,
    /// Parent identity; absent only for the artificial root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent: Option<String>,
}

/// Compact basis projection of one original convex edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowConvexNetworkSimplexEdgeStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Tree or nonbasic-breakpoint partition.
    pub basis: FlowConvexNetworkSimplexBasisV1,
    /// Declared segment selected by the compact state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_segment: Option<String>,
    /// Membership in the ordered fundamental cycle.
    pub in_cycle: bool,
    /// Whether either direction is the priced entering arc.
    pub entering: bool,
    /// Whether either direction is the most recent Cunningham breakpoint arc.
    pub leaving: bool,
}

/// One artificial root-star edge retained by the compact basis.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowConvexNetworkSimplexArtificialEdgeV1 {
    /// Stable `artificial:<node-id>` identity.
    pub entity_id: String,
    /// Original node incident to this artificial edge.
    pub node_id: String,
    /// Stable source extended-node identity.
    pub source: String,
    /// Stable target extended-node identity.
    pub target: String,
    /// Exact nonnegative artificial flow.
    pub flow: String,
    /// Tree or nonbasic-breakpoint partition.
    pub basis: FlowConvexNetworkSimplexBasisV1,
    /// Membership in the ordered fundamental cycle.
    pub in_cycle: bool,
    /// Whether this direction is selected as entering.
    pub entering: bool,
    /// Whether this direction is the latest selected breakpoint.
    pub leaving: bool,
}

/// One directed original or artificial compact arc.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowConvexNetworkSimplexArcRefV1 {
    /// Stable original edge or `artificial:<node-id>` identity.
    pub entity_id: String,
    /// Declared segment for original edges; absent for artificial edges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub segment: Option<String>,
    /// `forward` or `reverse` relative to the stored edge orientation.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
}

/// Dedicated compact basis, cycle, and artificial-root projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowConvexNetworkSimplexOverlayV1 {
    /// Current source-level boundary.
    pub stage: FlowConvexNetworkSimplexStageV1,
    /// Strict Big-M cost of every artificial root edge.
    pub artificial_cost: String,
    /// Original nodes followed by the artificial root.
    pub nodes: Vec<FlowConvexNetworkSimplexNodeStateV1>,
    /// Original edges in canonical order.
    pub edges: Vec<FlowConvexNetworkSimplexEdgeStateV1>,
    /// Artificial edges in canonical original-node order.
    pub artificial_edges: Vec<FlowConvexNetworkSimplexArtificialEdgeV1>,
    /// Priced non-tree direction, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entering: Option<FlowConvexNetworkSimplexArcRefV1>,
    /// Most recently selected breakpoint direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub leaving: Option<FlowConvexNetworkSimplexArcRefV1>,
    /// Ordered directed fundamental cycle from the tree join.
    pub cycle: Vec<FlowConvexNetworkSimplexArcRefV1>,
}

/// Publication boundary within prediction-assisted epsilon relaxation.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowPredictionAssistedEpsilonStageV1 {
    /// Algorithm 1 shifted and clipped the supplied dual prediction.
    PreprocessPrediction,
    /// Remark 1 began a new exponent guess.
    BeginAttempt,
    /// One scaled epsilon-CS pseudoflow with an empty admissible graph exists.
    InitializeScale,
    /// The canonical positive-surplus node was selected.
    SelectSurplus,
    /// One outgoing residual arc was tested for epsilon admissibility.
    InspectAdmissibleArc,
    /// One outgoing residual arc was tested as the next price breakpoint.
    InspectPriceBreakpointArc,
    /// Positive surplus was pushed over an equality residual arc.
    Push,
    /// The selected node price rose to the next residual breakpoint.
    RaisePrice,
    /// One positive-surplus iteration finished.
    CompleteUpIteration,
    /// One scaled epsilon-relaxation problem became feasible.
    CompleteScale,
    /// Remark 1 rejected a too-small exponent guess.
    AbortAttempt,
    /// The original-cost optimum was independently certified.
    Optimal,
}

/// Exact prediction, price, and surplus state of one original node.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPredictionAssistedEpsilonNodeStateV1 {
    /// Stable original-node identity.
    pub node_id: String,
    /// Unmodified configured prediction.
    pub raw_predicted_price: String,
    /// Algorithm 1 shifted and clipped prediction.
    pub predicted_price: String,
    /// Whether Algorithm 1 clipped this node at `(n-1)C`.
    pub prediction_clipped: bool,
    /// Current dual price in the active scaled-cost domain.
    pub price: String,
    /// Current paper-convention signed surplus.
    pub surplus: String,
    /// Whether this is the currently selected positive-surplus node.
    pub active: bool,
}

/// Active scaled-cost state of one original edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPredictionAssistedEpsilonEdgeStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Exact `floor((n+1)a/c^t)` cost.
    pub scaled_cost: String,
}

/// Dedicated projection of Chen--Yao--Yin Algorithm 1, Algorithm 2, and
/// Remark 1's robust unknown-error schedule.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPredictionAssistedEpsilonOverlayV1 {
    /// Current source-level boundary.
    pub stage: FlowPredictionAssistedEpsilonStageV1,
    /// Source scaling multiplier `c` in `[2,4]`.
    pub scaling_parameter: String,
    /// One-based robust attempt ordinal, or zero before the first attempt.
    pub attempt: String,
    /// Largest source-bounded attempt ordinal.
    pub maximum_attempt: String,
    /// Current exponent guess `T`, or zero before the first attempt.
    pub exponent: String,
    /// Current descending scaled-cost exponent `t`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scale_exponent: Option<String>,
    /// Certificate-aligned infinity prediction error, available only at optimum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub certificate_aligned_prediction_error: Option<String>,
    /// Canonical original-node states.
    pub nodes: Vec<FlowPredictionAssistedEpsilonNodeStateV1>,
    /// Canonical original-edge scaled costs.
    pub edges: Vec<FlowPredictionAssistedEpsilonEdgeStateV1>,
    /// Selected positive-surplus node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_node: Option<String>,
    /// Most recent active equality residual arc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_arc: Option<FlowResidualArcRefV1>,
}

/// Publication boundary within one Tardos network-matrix fixing primitive.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowTardosFrameworkStageV1 {
    /// No feasibility claim has been published.
    Ready,
    /// A feasible flow has been constructed.
    ConstructFeasibleFlow,
    /// Exact epsilon and every positive residual reduced cost are visible.
    MeasureEpsilon,
    /// Strictly over-threshold residual directions are classified.
    ClassifyFixedVariables,
    /// The fixed-variable theorem result passed its independent checker.
    Complete,
}

/// One configured node label in the Tardos theorem state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowTardosNodeStateV1 {
    /// Stable original-node identity.
    pub node_id: String,
    /// Exact configured potential.
    pub potential: String,
}

/// One positive residual direction priced by the theorem primitive.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowTardosResidualStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// `forward` or `reverse` relative to the original edge.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
    /// Exact positive residual capacity.
    pub capacity: String,
    /// Exact residual reduced cost.
    pub reduced_cost: String,
    /// Whether `reduced_cost > n * epsilon` fixes the original variable.
    pub fixes_variable: bool,
}

/// One bound value shared by every optimal flow under Tardos's lemma.
#[derive(Clone, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowTardosFixedVariableV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// `lower` or `upper`.
    #[schemars(schema_with = "flow_lower_upper_schema")]
    #[ts(type = "\"lower\" | \"upper\"")]
    pub bound: String,
    /// Exact fixed original-edge flow.
    pub value: String,
    /// Witness residual direction.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
    /// Exact witness reduced cost.
    pub reduced_cost: String,
}

/// Dedicated exact projection of one source-defined Tardos fixing step.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowTardosFrameworkOverlayV1 {
    /// Current theorem boundary.
    pub stage: FlowTardosFrameworkStageV1,
    /// Least nonnegative epsilon satisfying all residual inequalities.
    pub epsilon: String,
    /// Exact strict test boundary `n * epsilon` for `Delta(A) = 1`.
    pub threshold: String,
    /// Network-incidence maximum subdeterminant.
    pub determinant_bound: String,
    /// Canonical original-node potentials.
    pub nodes: Vec<FlowTardosNodeStateV1>,
    /// Canonical positive residual directions, absent before measurement.
    pub residual_arcs: Vec<FlowTardosResidualStateV1>,
    /// Canonical original variables fixed in every optimum.
    pub fixed_variables: Vec<FlowTardosFixedVariableV1>,
}

/// Publication boundary within the bounded electrical-flow primitive.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowElectricalFlowStageV1 {
    /// Input is valid but no grounded system is visible.
    Ready,
    /// The capacity-scaled weighted Laplacian was assembled.
    AssembleLaplacian,
    /// Zero potential and the Jacobi-PCG direction were initialized.
    InitializeConjugateGradient,
    /// One complete PCG iteration committed.
    ConjugateGradientIteration,
    /// Signed currents, congestion, and energy were recovered.
    RecoverCurrents,
    /// The independent exact rational solve agreed.
    CheckExactReference,
    /// The primitive certificate is complete; no max-flow claim is made.
    Complete,
}

/// One node in the grounded electrical linear system.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowElectricalNodeStateV1 {
    /// Stable canonical node identity.
    pub node_id: String,
    /// Current finite approximate voltage potential.
    pub potential: String,
    /// Current grounded-system residual; zero at the grounded sink.
    pub residual: String,
    /// Current Jacobi-PCG search direction; zero at the grounded sink.
    pub search_direction: String,
    /// Whether this node is the zero-potential grounded sink.
    pub grounded: bool,
}

/// One arbitrarily oriented undirected resistor.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowElectricalEdgeStateV1 {
    /// Stable original edge identity.
    pub edge_id: String,
    /// Exact resistance `1 / u_e^2`.
    pub resistance: FlowRationalV1,
    /// Exact integer conductance `u_e^2`.
    pub conductance: String,
    /// Signed stored-tail to stored-head voltage drop.
    pub voltage_drop: String,
    /// Signed current relative to the stored orientation.
    pub current: String,
    /// Absolute capacity congestion `|current| / u_e`.
    pub congestion: String,
    /// Edge energy `r_e current_e^2`.
    pub energy: String,
}

/// Dedicated projection of Christiano et al. §2.3's electrical primitive.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowElectricalFlowOverlayV1 {
    /// Current source-level numerical boundary.
    pub stage: FlowElectricalFlowStageV1,
    /// Unit current injected at the source and removed at the sink.
    pub target_current: String,
    /// Relative Euclidean PCG stopping tolerance.
    pub relative_tolerance: String,
    /// Completed PCG iterations.
    pub iteration: String,
    /// Euclidean norm of the grounded linear-system residual.
    pub residual_l2: String,
    /// Approximate effective source-sink resistance.
    pub effective_resistance: String,
    /// Sum of approximate per-edge energies.
    pub total_energy: String,
    /// Exact rational effective resistance after reference checking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exact_effective_resistance: Option<FlowRationalV1>,
    /// Maximum potential/current error against the exact reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub maximum_absolute_error: Option<String>,
    /// Whether the PCG residual met the closed tolerance.
    pub converged: bool,
    /// Canonical node states.
    pub nodes: Vec<FlowElectricalNodeStateV1>,
    /// Canonical arbitrarily oriented resistor states.
    pub edges: Vec<FlowElectricalEdgeStateV1>,
}

/// Publication boundary in the bounded augmenting-electrical solver.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowAugmentingElectricalStageV1 {
    /// Valid input with no transformed graph.
    Ready,
    /// The folklore three-edge directed reduction was built.
    BuildDirectedReduction,
    /// Symmetric source preconditioners were added.
    AddPreconditioning,
    /// The bounded exact target cut was installed.
    InstallTargetCut,
    /// A residual-barrier electrical direction was solved.
    SolveElectricalDirection,
    /// A high-energy arc became the source-defined series path.
    BoostHighEnergyArc,
    /// Primal flow and dual embedding advanced together.
    AugmentPrimalDual,
    /// A second electrical solve restored coupling.
    FixCoupling,
    /// Boost paths were contracted to equivalent roots.
    CollapseBoostPaths,
    /// Fractional central flow was rounded.
    RoundCentralFlow,
    /// One integral cleanup path was augmented.
    CleanupAugmentingPath,
    /// Preconditioners were removed and the directed reduction inverted.
    ExtractDirectedFlow,
    /// One auxiliary extraction cycle was canceled.
    CancelExtractionCycle,
    /// The half-integral directed flow was rounded.
    RoundDirectedFlow,
    /// Independent max-flow/min-cut checking passed.
    CheckCertificate,
    /// Certified completion.
    Optimal,
}

/// Original-node projection of the augmenting-electrical dual embedding.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowAugmentingElectricalNodeStateV1 {
    /// Stable canonical node identity.
    pub node_id: String,
    /// Finite dual embedding value.
    pub potential: String,
    /// Largest incident normalized coupling violation.
    pub coupling_violation: String,
    /// Membership in the exact original target cut.
    pub target_source_side: bool,
}

/// `h(e)`-root projection onto one original directed edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowAugmentingElectricalEdgeStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Signed transformed central flow.
    pub central_flow: String,
    /// Latest signed electrical current.
    pub electrical_current: String,
    /// Representative forward residual capacity.
    pub forward_residual: String,
    /// Representative backward residual capacity.
    pub backward_residual: String,
    /// Absolute electrical congestion.
    pub congestion: String,
    /// Residual-barrier resistance.
    pub resistance: String,
    /// Number of explicit leaf segments in the root's boost path.
    pub boost_segments: String,
    /// Integral transformed flow selected for the central `h(e)` root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rounded_central_flow: Option<String>,
    /// Doubled directed flow recovered from the central reduction arc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub extraction_central_scaled: Option<String>,
    /// Remaining auxiliary reduction flow directed toward the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub extraction_toward_source: Option<String>,
    /// Remaining auxiliary reduction flow directed out of the sink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub extraction_out_of_sink: Option<String>,
    /// Final original integral flow after extraction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_flow: Option<String>,
}

/// One oriented transformed edge on the active discrete cleanup path.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowAugmentingElectricalWorkingArcV1 {
    /// Working-edge ordinal.
    pub edge: String,
    /// Traversal direction relative to the working edge.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
    /// Stable original-node identity at which this traversal starts.
    pub from_node: String,
    /// Stable original-node identity at which this traversal ends.
    pub to_node: String,
    /// Integral working-edge flow after this cleanup augmentation.
    pub flow_after: String,
}

/// Directed-reduction role of an arc on an extraction cycle.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowAugmentingElectricalExtractionArcKindV1 {
    /// Central original-edge copy.
    Central,
    /// Auxiliary copy directed toward the source.
    TowardSource,
    /// Auxiliary copy directed out of the sink.
    OutOfSink,
}

/// One directed-reduction arc on the active extraction cycle.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowAugmentingElectricalExtractionArcV1 {
    /// Original-edge ordinal that owns the reduction arc.
    pub edge: String,
    /// Reduction role of the arc.
    pub kind: FlowAugmentingElectricalExtractionArcKindV1,
}

/// Dedicated projection of Mądry's augmenting-electrical framework.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowAugmentingElectricalOverlayV1 {
    /// Current semantic boundary.
    pub stage: FlowAugmentingElectricalStageV1,
    /// Exact original directed maximum-flow target.
    pub original_target: String,
    /// Exact target after directed-to-undirected reduction.
    pub transformed_target: String,
    /// Exact target after preconditioning.
    pub working_target: String,
    /// Current finite working-flow value.
    pub current_value: String,
    /// Current routed fraction.
    pub alpha: String,
    /// Additive working flow still missing.
    pub remaining: String,
    /// Latest electrical energy.
    pub electrical_energy: String,
    /// Latest congestion `l3` norm.
    pub congestion_l3: String,
    /// Latest congestion `l4` norm.
    pub congestion_l4: String,
    /// Global normalized coupling `l2` norm.
    pub coupling_l2: String,
    /// Explicit working node count.
    pub working_nodes: String,
    /// Explicit working edge count.
    pub working_edges: String,
    /// Active phase-local working edge ordinal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_working_edge: Option<String>,
    /// Active reduced-system pivot node ordinal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_pivot_node: Option<String>,
    /// Exact oriented working path used by a discrete cleanup augmentation.
    pub active_working_path: Vec<FlowAugmentingElectricalWorkingArcV1>,
    /// Exact reduction cycle canceled during directed extraction.
    pub active_extraction_cycle: Vec<FlowAugmentingElectricalExtractionArcV1>,
    /// Integral amount sent through the active path or removed from the cycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_discrete_amount: Option<String>,
    /// Canonical original-node projections.
    pub nodes: Vec<FlowAugmentingElectricalNodeStateV1>,
    /// Canonical original-edge projections.
    pub edges: Vec<FlowAugmentingElectricalEdgeStateV1>,
}

/// Publication boundary in the bounded Section 4/5 path-following solver.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowInteriorPointMaxFlowStageV1 {
    /// Valid unit-capacity input only.
    Ready,
    /// Every bounded source-side cut was inspected.
    EnumerateTargetCut,
    /// The perfect bipartite `b`-matching instance was built.
    BuildBMatchingReduction,
    /// The unit-length demand-flow graph `G_b` was built.
    BuildMinCostReduction,
    /// Lemma 5.4's explicit zero-centered state was installed.
    InitializeCentralPath,
    /// The associated `r=s/f` electrical demand flow was solved.
    SolveElectricalDirection,
    /// The source descent equations advanced primal and dual state.
    DescentStep,
    /// The electrical centering correction was solved.
    SolveCenteringDirection,
    /// The corrected state restored centrality.
    CenteringStep,
    /// Direct matching arcs were projected to original fractional flow.
    ExtractFractionalFlow,
    /// The source b-matching recovery produced an integral target flow.
    RoundIntegralFlow,
    /// The independent original-graph certificate passed.
    CheckCertificate,
    /// Certified completion.
    Optimal,
}

/// Original-node projection of the reduced electrical embedding.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowInteriorPointNodeStateV1 {
    /// Stable canonical original-node identity.
    pub node_id: String,
    /// Finite average reduced potential.
    pub potential: String,
    /// Membership in the exact enumerated target cut.
    pub target_source_side: bool,
}

/// Direct matching-arc projection onto one original directed edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowInteriorPointEdgeStateV1 {
    /// Stable canonical original-edge identity.
    pub edge_id: String,
    /// Fractional flow on `(p_e,q_e)`.
    pub fractional_flow: String,
    /// Latest associated or centering electrical current.
    pub electrical_current: String,
    /// Current positive dual slack.
    pub slack: String,
    /// Current positive arc measure.
    pub measure: String,
    /// Current resistance `s/f`.
    pub resistance: String,
    /// Latest absolute associated-flow congestion.
    pub congestion: String,
    /// Whether terminal normalization removed this original edge.
    pub normalized_away: bool,
    /// Final integral original flow after source b-matching recovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_flow: Option<String>,
}

/// Dedicated source-reduction and central-path projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowInteriorPointMaxFlowOverlayV1 {
    /// Current semantic boundary.
    pub stage: FlowInteriorPointMaxFlowStageV1,
    /// Exact bounded minimum-cut target.
    pub target_value: String,
    /// Weighted mean complementarity.
    pub mu: String,
    /// Current primal-dual gap.
    pub duality_gap: String,
    /// Relative weighted centrality norm.
    pub centrality: String,
    /// Latest weighted congestion four-norm.
    pub congestion_l4: String,
    /// Latest safe path-following step fraction.
    pub step_size: String,
    /// Latest electrical energy.
    pub electrical_energy: String,
    /// Vertices in the perfect `b`-matching instance.
    pub b_matching_nodes: String,
    /// Edges in the perfect `b`-matching instance.
    pub b_matching_edges: String,
    /// Vertices in `G_b`.
    pub working_nodes: String,
    /// Arcs in `G_b`.
    pub working_edges: String,
    /// Active reduced arc ordinal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_working_edge: Option<String>,
    /// Canonical original-node projections.
    pub nodes: Vec<FlowInteriorPointNodeStateV1>,
    /// Canonical original-edge projections.
    pub edges: Vec<FlowInteriorPointEdgeStateV1>,
}

/// Publication boundary in the bounded exact minimum-ratio-cycle primitive.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowMinimumRatioCycleStageV1 {
    /// Valid input before interpreting the source `g` and `l` vectors.
    Ready,
    /// Original costs and capacities were mapped to gradient and length.
    MapGradientLength,
    /// A canonical spanning forest was built.
    BuildSpanningForest,
    /// A geometrically spaced ternary candidate vector is being inspected.
    InspectVector,
    /// One signed simple circulation was evaluated.
    EvaluateCycle,
    /// The incumbent exact ratio changed.
    UpdateBest,
    /// The selected vector passed incidence conservation.
    VerifyCycleSpace,
    /// An independent DFS cycle enumeration agreed.
    CheckExhaustiveOracle,
    /// Independently checked primitive completion.
    Complete,
}

/// One original node in the cycle-space and spanning-forest projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowMinimumRatioCycleNodeStateV1 {
    /// Stable canonical original-node identity.
    pub node_id: String,
    /// Deterministic component ordinal.
    pub component: String,
    /// Canonical forest parent, absent at a component root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent_node_id: Option<String>,
    /// Canonical forest depth.
    pub depth: String,
    /// Signed incidence balance of the visible candidate.
    pub candidate_balance: String,
    /// Whether the node belongs to the visible candidate cycle.
    pub on_candidate: bool,
    /// Whether the node belongs to the selected cycle.
    pub on_selected: bool,
}

/// One original edge in the exact ratio objective.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowMinimumRatioCycleEdgeStateV1 {
    /// Stable canonical original-edge identity.
    pub edge_id: String,
    /// Signed source gradient `g_e`, mapped from cost.
    pub gradient: String,
    /// Positive source length `l_e`, mapped from capacity.
    pub length: String,
    /// Membership in the deterministic spanning forest.
    pub tree_edge: bool,
    /// Visible candidate sign in `{-1,0,1}`.
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub candidate_sign: String,
    /// Selected optimum sign in `{-1,0,1}`.
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub selected_sign: String,
    /// Exact visible `g_e delta_e` contribution.
    pub numerator_contribution: String,
    /// Exact visible `l_e |delta_e|` contribution.
    pub denominator_contribution: String,
}

/// Dedicated projection of Chen et al.'s undirected ratio subproblem.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowMinimumRatioCycleOverlayV1 {
    /// Current semantic boundary.
    pub stage: FlowMinimumRatioCycleStageV1,
    /// Visible candidate ratio, present only during candidate inspection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub candidate_ratio: Option<FlowRationalV1>,
    /// Best exact ratio found so far; absent for an acyclic input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub best_ratio: Option<FlowRationalV1>,
    /// Number of selected original edges.
    pub selected_edge_count: String,
    /// Largest absolute visible incidence imbalance.
    pub maximum_absolute_balance: String,
    /// Ternary sign vectors inspected so far.
    pub enumerated_vectors: String,
    /// Signed simple cycles evaluated so far.
    pub simple_cycles: String,
    /// Dimension of the spanning-forest fundamental cycle basis.
    pub fundamental_cycles: String,
    /// Canonical node projections.
    pub nodes: Vec<FlowMinimumRatioCycleNodeStateV1>,
    /// Canonical edge projections.
    pub edges: Vec<FlowMinimumRatioCycleEdgeStateV1>,
}

/// One signed original edge in the certified ratio cycle.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowMinimumRatioCycleArcV1 {
    /// Stable canonical original-edge identity.
    pub edge_id: String,
    /// `1` follows the stored orientation and `-1` opposes it.
    #[schemars(schema_with = "flow_nonzero_sign_schema")]
    #[ts(type = "\"-1\" | \"1\"")]
    pub sign: String,
}

/// Publication boundary in the bounded minimum-ratio-cycle MCF primitive.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum FlowMinimumRatioCycleMcfStageV1 {
    Ready,
    EnumerateFeasibleSet,
    ContractFixedFace,
    InitializeStrictInterior,
    EvaluatePotential,
    MapGradientLength,
    BuildSpanningForest,
    InspectVector,
    EvaluateCycle,
    UpdateBest,
    VerifyCycleSpace,
    ApplySourceStep,
    MeasurePotentialDecrease,
    CheckDfsOracle,
    Complete,
}

/// One original node in the MCF cycle-space projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowMinimumRatioCycleMcfNodeStateV1 {
    /// Stable original-node identity.
    pub node_id: String,
    /// Deterministic active-edge component ordinal.
    pub component: String,
    /// Canonical spanning-forest parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent_node_id: Option<String>,
    /// Spanning-forest depth.
    pub depth: String,
    /// Signed incidence balance of the visible candidate.
    pub candidate_balance: String,
    /// Candidate-cycle membership.
    pub on_candidate: bool,
    /// Selected-cycle membership.
    pub on_selected: bool,
}

/// One original edge in the MCF source potential and cycle objective.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct FlowMinimumRatioCycleMcfEdgeStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Coordinate is fixed throughout the exact feasible face.
    pub fixed_on_face: bool,
    /// Initial strict relative-interior flow.
    pub initial_flow: String,
    /// Flow after the source-scaled step.
    pub updated_flow: String,
    /// Initial lower residual.
    pub lower_slack: String,
    /// Initial upper residual.
    pub upper_slack: String,
    /// Source gradient.
    pub gradient: String,
    /// Source length.
    pub length: String,
    /// Active-edge spanning-forest membership.
    pub tree_edge: bool,
    /// Visible candidate sign.
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub candidate_sign: String,
    /// Selected cycle sign.
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub selected_sign: String,
    /// Visible numerator contribution.
    pub numerator_contribution: String,
    /// Visible denominator contribution.
    pub denominator_contribution: String,
}

/// Dedicated source-faithful one-step MCF progress state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowMinimumRatioCycleMcfOverlayV1 {
    /// Current source/audit boundary.
    pub stage: FlowMinimumRatioCycleMcfStageV1,
    /// Source alpha-power exponent.
    pub alpha: String,
    /// Exact bounded optimum value used as `F*`.
    pub optimum_cost: String,
    /// Initial fractional objective.
    pub initial_cost: String,
    /// Current fractional objective.
    pub current_cost: String,
    /// Current objective gap.
    pub cost_gap: String,
    /// Initial source potential.
    pub potential_before: String,
    /// Current source potential.
    pub current_potential: String,
    /// Visible candidate ratio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub candidate_ratio: Option<String>,
    /// Selected exact minimum ratio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub best_ratio: Option<String>,
    /// Certified query quality used by the source update.
    pub kappa: String,
    /// Source step multiplier.
    pub eta: String,
    /// Weighted step norm.
    pub weighted_step_norm: String,
    /// Measured source potential decrease.
    pub potential_decrease: String,
    /// Guaranteed source potential decrease.
    pub guaranteed_decrease: String,
    /// Initial relative-interior point is already cost-optimal.
    pub stationary: bool,
    /// Number of selected original edges.
    pub selected_edge_count: String,
    /// Largest candidate incidence imbalance.
    pub maximum_absolute_balance: String,
    /// Feasible integer flows retained by the bounded audit.
    pub feasible_flows: String,
    /// Ternary vectors inspected.
    pub enumerated_vectors: String,
    /// Signed simple cycles evaluated.
    pub simple_cycles: String,
    /// Active cycle-space dimension.
    pub fundamental_cycles: String,
    /// Canonical node projections.
    pub nodes: Vec<FlowMinimumRatioCycleMcfNodeStateV1>,
    /// Canonical edge projections.
    pub edges: Vec<FlowMinimumRatioCycleMcfEdgeStateV1>,
}

/// Publication boundary in the bounded randomized almost-linear MCF solver.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum FlowRandomizedAlmostLinearMcfStageV1 {
    Ready,
    InspectFeasibleAssignment,
    EnumerateFeasibleSet,
    SampleIsolationCosts,
    SelectIsolatedOptimum,
    InitializeRelativeInterior,
    InspectOracleVector,
    BuildForestPool,
    SampleTreeChain,
    RefreshGradientLength,
    QueryMinimumRatioCycle,
    PotentialReductionStep,
    DetectChangedCoordinates,
    RebuildTreeChain,
    ConstructFinalPoint,
    RoundNearestInteger,
    CheckCertificate,
    Optimal,
}

/// One original node in the sampled tree-chain projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowRandomizedAlmostLinearMcfNodeStateV1 {
    /// Stable canonical node identity.
    pub node_id: String,
    /// Required outflow-minus-inflow divergence.
    pub required_divergence: String,
    /// Rooted forest component.
    pub component: String,
    /// Rooted forest parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent_node_id: Option<String>,
    /// Rooted forest depth.
    pub depth: String,
    /// Selected ratio-cycle membership.
    pub on_selected_cycle: bool,
}

/// One original edge in isolation, tree sampling, and lazy refresh state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct FlowRandomizedAlmostLinearMcfEdgeStateV1 {
    /// Stable canonical edge identity.
    pub edge_id: String,
    /// Coordinate is fixed throughout the exact feasible face.
    pub fixed_on_face: bool,
    /// Relative-interior flow.
    pub initial_flow: String,
    /// Current fractional flow.
    pub current_flow: String,
    /// Last lazily refreshed flow.
    pub stale_flow: String,
    /// Exact feasible near-optimal final-point coordinate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_flow: Option<FlowRationalV1>,
    /// Final integral flow after nearest-integer rounding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_flow: Option<String>,
    /// Independent isolation draw.
    pub isolation_draw: String,
    /// Isolated unit cost `D c_e + z_e`.
    pub isolated_cost: String,
    /// Coordinate of the unique isolated integral optimum once selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub isolated_optimum_flow: Option<String>,
    /// Sampled spanning-forest membership.
    pub tree_edge: bool,
    /// Signed vector currently inspected by the exact ratio-cycle oracle.
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub candidate_sign: String,
    /// Selected cycle sign.
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub selected_sign: String,
    /// Source gradient coordinate.
    pub gradient: String,
    /// Source length coordinate.
    pub length: String,
    /// Lazy Detect refreshed this coordinate.
    pub detected: bool,
}

/// Dedicated end-to-end randomized almost-linear MCF trace state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowRandomizedAlmostLinearMcfOverlayV1 {
    /// Current source or bounded-replacement phase.
    pub stage: FlowRandomizedAlmostLinearMcfStageV1,
    /// Edge whose coordinate completed the published feasible assignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assignment_cursor: Option<String>,
    /// Exact one-based feasible-assignment inspection count at this checkpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assignment_serial: Option<String>,
    /// Exact one-based signed-vector inspection count at this oracle checkpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub oracle_vector_serial: Option<String>,
    /// Stable seeded run identity.
    pub seed: String,
    /// Source alpha-power exponent.
    pub alpha: String,
    /// Lazy refresh threshold.
    pub epsilon: String,
    /// Query quality parameter.
    pub kappa: String,
    /// Source step multiplier.
    pub eta: String,
    /// Relative-interior objective.
    pub initial_cost: String,
    /// Current fractional objective.
    pub current_cost: String,
    /// Exact optimum original objective.
    pub optimum_cost: String,
    /// Unique isolated optimum objective.
    pub isolated_optimum_cost: String,
    /// Current source potential.
    pub potential: String,
    /// Selected ratio, absent for a stationary face.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub minimum_ratio: Option<String>,
    /// One-based isolation attempt.
    pub isolation_attempt: String,
    /// Lexicographic isolation scale.
    pub isolation_scale: String,
    /// Isolation failure bound numerator.
    pub failure_numerator: String,
    /// Isolation failure bound denominator.
    pub failure_denominator: String,
    /// Bounded forest population size.
    pub forest_pool_size: String,
    /// Seeded forest ordinal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sampled_forest_index: Option<String>,
    /// Exact perturbed-objective gap of the final point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_gap: Option<FlowRationalV1>,
    /// Chen et al. final-point threshold `1/(12m^3U^3)`.
    pub final_point_threshold: FlowRationalV1,
    /// Exact barycenter mixing weight used to construct the final point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_mix: Option<FlowRationalV1>,
    /// Nearest-integer recovery has completed.
    pub exact_recovery: bool,
    /// Feasible integer flows retained.
    pub feasible_flows: String,
    /// Coordinates refreshed by Detect.
    pub detected_coordinates: String,
    /// Tree-chain rebuild count.
    pub rebuilds: String,
    /// Canonical node projections.
    pub nodes: Vec<FlowRandomizedAlmostLinearMcfNodeStateV1>,
    /// Canonical edge projections.
    pub edges: Vec<FlowRandomizedAlmostLinearMcfEdgeStateV1>,
}

/// Publication boundary in the bounded source Flow Framework coordinator.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowFrameworkMcfStageV1 {
    /// Midpoint/auxiliary source initial point is ready.
    InitializeSourcePoint,
    /// Current exact flow was absorbed into a fresh dynamic epoch.
    PeriodicReinitialize,
    /// Definition 4.5 `Detect` boundary.
    Detect,
    /// Algorithm 2 returned one accepted minimum-ratio cycle.
    QueryMinimumRatioCycle,
    /// Source-scaled circulation was applied.
    SourceProgress,
    /// Kang--Payor exact rounding is being checked against known `F*`.
    RoundFractionalFlow,
    /// Original-graph MCF certificate is being checked.
    CheckCertificate,
    /// Certified exact optimum.
    Optimal,
}

/// Exact inner dynamic-stack operation responsible for a published boundary.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowFrameworkMcfDynamicOperationV1 {
    /// One root update stage propagated through the active epochs.
    TopologyStageApplied,
    /// A strict update limit rebuilt a suffix of the tree chain.
    PeriodicRebuilt,
    /// A queried cycle passed the source acceptance threshold.
    CycleQueriedAccepted,
    /// A queried cycle failed the source acceptance threshold.
    CycleQueriedRejected,
    /// The largest eligible level shifted to its next branch.
    LevelShifted,
    /// The normalized accepted circulation changed the maintained flow.
    FlowApplied,
    /// One stable flow coordinate was returned.
    QueryReturned,
    /// Detect returned and reset its exact active-coordinate set.
    DetectReturned,
    /// The requested inner operation sequence completed without another mutation.
    Completed,
}

/// One original edge at a Flow Framework boundary.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFrameworkMcfEdgeStateV1 {
    /// Stable canonical edge identity.
    pub edge_id: String,
    /// Exact current fractional flow.
    pub flow: FlowRationalV1,
    /// Accepted circulation coefficient; zero before cycle application.
    pub cycle_coefficient: FlowRationalV1,
    /// Whether the accepted cycle uses this edge.
    pub selected: bool,
}

/// One dynamic level at a Flow Framework boundary.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFrameworkMcfLevelStateV1 {
    /// Zero-based level.
    pub level: String,
    /// Active MWU branch.
    pub active_branch: String,
    /// Completed wrapped passes.
    pub passes: String,
}

/// One augmented node in the deterministic source final-point proof.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFrameworkMcfFinalPointNodeV1 {
    /// Stable node identity, including the auxiliary source-point node.
    pub node_id: String,
    /// Required outgoing-minus-incoming divergence.
    pub required_divergence: String,
}

/// One augmented edge in the deterministic source final-point proof.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFrameworkMcfFinalPointEdgeV1 {
    /// Stable edge identity.
    pub edge_id: String,
    /// Stable tail-node identity.
    pub from: String,
    /// Stable head-node identity.
    pub to: String,
    /// Integral lower bound.
    pub lower: String,
    /// Integral upper bound.
    pub capacity: String,
    /// Integral unit cost.
    pub cost: String,
    /// Exact source final-point coordinate.
    pub flow: FlowRationalV1,
    /// Whether the edge belongs to the source auxiliary construction.
    pub auxiliary: bool,
    /// Kang--Payor result; absent at the pre-rounding boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rounded_flow: Option<String>,
}

/// Dedicated bounded Flow Framework trace state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowFrameworkMcfOverlayV1 {
    /// Current source boundary.
    pub stage: FlowFrameworkMcfStageV1,
    /// Exact inner operation, present only for a dynamic-stack event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dynamic_operation: Option<FlowFrameworkMcfDynamicOperationV1>,
    /// Cumulative inner state-transition count at that exact operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dynamic_operation_serial: Option<String>,
    /// Completed source iterations.
    pub iteration: String,
    /// Whether this iteration began with periodic reinitialization.
    pub reinitialized: bool,
    /// Source potential before the accepted step.
    pub potential_before: String,
    /// Source potential after the accepted step.
    pub potential_after: String,
    /// Objective gap before the accepted step.
    pub gap_before: String,
    /// Objective gap after the accepted step.
    pub gap_after: String,
    /// Exact augmented-cost gap before the accepted step.
    pub exact_gap_before: FlowRationalV1,
    /// Exact augmented-cost gap after the accepted step.
    pub exact_gap_after: FlowRationalV1,
    /// Exact source final-point threshold; currently $1/2$.
    pub stopping_gap: FlowRationalV1,
    /// Exact accepted minimum ratio.
    pub accepted_ratio: FlowRationalV1,
    /// Exact source target $\kappa^2/50$.
    pub target_progress: FlowRationalV1,
    /// Explicit source final-point rule, present only at the terminal boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub termination: Option<String>,
    /// Scalar bounded-oracle optimum used only as the final-point value anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub optimum_cost: Option<String>,
    /// Augmented divergence requirements, present only at final-point boundaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_nodes: Option<Vec<FlowFrameworkMcfFinalPointNodeV1>>,
    /// Exact augmented point and optional checked rounding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_edges: Option<Vec<FlowFrameworkMcfFinalPointEdgeV1>>,
    /// Dynamic Algorithm 2 levels.
    pub levels: Vec<FlowFrameworkMcfLevelStateV1>,
    /// Canonical original edges.
    pub edges: Vec<FlowFrameworkMcfEdgeStateV1>,
}

/// Publication boundary in the bounded weighted augmenting-path solver.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum FlowWeightedAugmentingPathsStageV1 {
    Ready,
    BeginCapacityPhase,
    BuildHierarchy,
    CertifyExpansion,
    AssignWeights,
    RelabelSweep,
    AugmentPath,
    FinishWeightedRound,
    FinishCapacityPhase,
    CheckCertificate,
    Optimal,
}

/// Source-defined one-level hierarchy role.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowWeightedAugmentingHierarchyKindV1 {
    /// Residual arc between SCCs in the acyclic part `D`.
    Dag,
    /// Residual arc internal to an SCC in the expanding set `X_1`.
    Expanding,
}

/// One original node in the hierarchy, label, and cut projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowWeightedAugmentingNodeStateV1 {
    /// Stable original-node identity.
    pub node_id: String,
    /// Residual SCC ordinal.
    pub component: String,
    /// One-based hierarchy-respecting topological order.
    pub order: String,
    /// Current weighted relabel level.
    pub label: String,
    /// Vertices above `9h` are dead in the active call.
    pub alive: bool,
    /// Membership in the exact directed cut attaining the displayed `phi`.
    pub expansion_witness_side: bool,
    /// Current residual reachability from the source.
    pub source_side: bool,
}

/// One original edge at the active binary capacity prefix.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowWeightedAugmentingEdgeStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Capacity represented by the active prefix.
    pub scaled_capacity: String,
    /// Current integral original-edge flow.
    pub flow: String,
}

/// One stable original-edge residual direction in the source overlay.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowWeightedAugmentingResidualArcStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// `forward` or `reverse` relative to the original edge.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
    /// Residual tail identity.
    pub from: String,
    /// Residual head identity.
    pub to: String,
    /// Current exact residual capacity.
    pub capacity: String,
    /// Hierarchy role, absent before hierarchy construction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hierarchy_kind: Option<FlowWeightedAugmentingHierarchyKindV1>,
    /// Source weight `|tau(u)-tau(v)|`, zero before assignment.
    pub weight: String,
    /// Current source admissibility flag.
    pub admissible: bool,
    /// Active augmenting-path membership.
    pub active: bool,
}

/// Dedicated capacity-scaling and weighted-path projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowWeightedAugmentingPathsOverlayV1 {
    /// Current source/audit boundary.
    pub stage: FlowWeightedAugmentingPathsStageV1,
    /// Zero-based capacity-prefix phase.
    pub phase: String,
    /// Total number of capacity bits.
    pub phase_count: String,
    /// Active binary bit position.
    pub capacity_bit: String,
    /// Weighted residual round inside the phase.
    pub round: String,
    /// Source height parameter `h`.
    pub height: String,
    /// Exact reduced expansion ratio numerator.
    pub phi_numerator: String,
    /// Exact positive expansion ratio denominator.
    pub phi_denominator: String,
    /// Active path bottleneck, zero off augmentation boundaries.
    pub active_bottleneck: String,
    /// Binary expansion witness inspected across the run.
    pub hierarchy_cuts: String,
    /// Accelerated relabel jumps across the run.
    pub relabel_jumps: String,
    /// Source augmenting paths across all prefixes.
    pub augmentations: String,
    /// Total units augmented across all prefixes.
    pub augmented_units: String,
    /// Canonical node projections.
    pub nodes: Vec<FlowWeightedAugmentingNodeStateV1>,
    /// Canonical original-edge projections.
    pub edges: Vec<FlowWeightedAugmentingEdgeStateV1>,
    /// Both stable residual directions per original edge.
    pub residual_arcs: Vec<FlowWeightedAugmentingResidualArcStateV1>,
    /// Active path in stable residual identities.
    pub active_path: Vec<FlowResidualArcRefV1>,
}

/// Publication boundary in the bounded weighted push-relabel shortcut solver.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum FlowWeightedPushRelabelShortcutStageV1 {
    Ready,
    BuildWeakHierarchy,
    BuildShortcutGraph,
    AssignWeights,
    InitializeDemand,
    RelabelSweep,
    RelabelCheckpoint,
    InspectPrimitiveArcCheckpoint,
    AugmentPath,
    MeasureShortFlow,
    ComputeDistanceLayers,
    SelectSparseCut,
    CompletionInspectPrimitiveArcCheckpoint,
    CompletionRelabelCheckpoint,
    CompletionAugmentPath,
    CompletionResidualRound,
    CompleteResidualRounds,
    CheckCertificate,
    Optimal,
}

/// Original or Steiner node in the explicit shortcut graph.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
// These are four independent wire facts: identity, liveness, and two cut
// certificates. Collapsing them into an enum would admit fewer valid states.
#[allow(clippy::struct_excessive_bools)]
pub struct FlowWeightedPushRelabelShortcutNodeStateV1 {
    /// Stable original ID or `shortcut:<component>` for a Steiner root.
    pub node_id: String,
    /// Whether this is an immutable original node.
    pub original: bool,
    /// One-level SCC component ordinal.
    pub component: String,
    /// One-based respecting order, zero for Steiner roots.
    pub order: String,
    /// Weighted relabel level.
    pub label: String,
    /// Whether the node is at or below `9h`.
    pub alive: bool,
    /// Membership in the selected modified-distance cut.
    pub sparse_cut_side: bool,
    /// Membership in the certified original minimum cut.
    pub source_side: bool,
}

/// One directed original or shortcut edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowWeightedPushRelabelShortcutEdgeStateV1 {
    /// Stable original or synthetic edge identity.
    pub edge_id: String,
    /// `original` or `shortcut`.
    #[schemars(schema_with = "flow_original_shortcut_schema")]
    #[ts(type = "\"original\" | \"shortcut\"")]
    pub kind: String,
    /// Stable augmented tail ID.
    pub from: String,
    /// Stable augmented head ID.
    pub to: String,
    /// Exact capacity after the public unit `psi` scale.
    pub capacity: String,
    /// Current source-kernel or repaired flow.
    pub flow: String,
    /// Source-defined positive weight.
    pub weight: String,
    /// Owning component for shortcut edges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub shortcut_component: Option<String>,
}

/// One stable residual direction in the explicit augmented graph.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowWeightedPushRelabelShortcutResidualArcStateV1 {
    /// Stable augmented edge identity.
    pub edge_id: String,
    /// `forward` or `reverse`.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
    /// Stable residual tail ID.
    pub from: String,
    /// Stable residual head ID.
    pub to: String,
    /// Exact residual capacity.
    pub capacity: String,
    /// Source-defined positive weight.
    pub weight: String,
    /// Persistent source admissibility flag.
    pub admissible: bool,
    /// Active source-path membership.
    pub active: bool,
}

/// Stable residual reference for original and synthetic shortcut edges.
#[derive(Clone, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowWeightedPushRelabelShortcutArcRefV1 {
    /// Stable augmented edge identity.
    pub edge_id: String,
    /// `forward` or `reverse`.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
}

/// Dedicated hierarchy, shortcut graph, weighted labels, and completion state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowWeightedPushRelabelShortcutOverlayV1 {
    /// Current source/audit boundary.
    pub stage: FlowWeightedPushRelabelShortcutStageV1,
    /// Number of weak hierarchy levels.
    pub hierarchy_levels: String,
    /// Shortcut capacity-scale numerator.
    pub psi_numerator: String,
    /// Shortcut capacity-scale denominator.
    pub psi_denominator: String,
    /// Source height parameter.
    pub height: String,
    /// Artificial source/sink demand.
    pub demand: String,
    /// Units routed by the shortcut call.
    pub routed: String,
    /// Total weighted path length.
    pub weighted_length: String,
    /// Positive units denominator for average length.
    pub weighted_length_units: String,
    /// Selected distance-layer threshold.
    pub sparse_cut_level: String,
    /// Residual capacity of the selected distance cut.
    pub sparse_cut_capacity: String,
    /// Active path bottleneck.
    pub active_bottleneck: String,
    /// Literal relabel increments.
    pub relabel_steps: String,
    /// Source augmenting paths.
    pub augmentations: String,
    /// Shortcut residual traversals.
    pub shortcut_traversals: String,
    /// Original residual graphs processed by the exact source outer loop.
    pub residual_rounds: String,
    /// Literal relabel increments in original-residual calls.
    pub completion_relabel_steps: String,
    /// Path augmentations in original-residual calls.
    pub completion_augmentations: String,
    /// Original and Steiner nodes.
    pub nodes: Vec<FlowWeightedPushRelabelShortcutNodeStateV1>,
    /// Directed original and shortcut edges.
    pub edges: Vec<FlowWeightedPushRelabelShortcutEdgeStateV1>,
    /// Both stable residual directions per augmented edge.
    pub residual_arcs: Vec<FlowWeightedPushRelabelShortcutResidualArcStateV1>,
    /// Active path in augmented residual identities.
    pub active_path: Vec<FlowWeightedPushRelabelShortcutArcRefV1>,
    /// Residual directions represented by the current source-time inspection checkpoint.
    pub inspected_arcs: Vec<FlowWeightedPushRelabelShortcutArcRefV1>,
    /// Zero or one augmented node represented by the current relabel checkpoint.
    pub active_relabel_nodes: Vec<String>,
}

/// Publication boundary in the bounded randomized almost-linear realization.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowRandomizedAlmostLinearStageV1 {
    /// Valid max-flow input before reduction.
    Ready,
    /// The `t -> s` return edge is visible.
    BuildReturnEdgeReduction,
    /// The midpoint/artificial-star strict interior is visible.
    BuildInitialPoint,
    /// The finite spanning-forest population was enumerated.
    EnumerateForestPool,
    /// The seeded tree chain was sampled.
    SampleTreeChain,
    /// One exact fundamental-cycle evaluation is visible at a geometric checkpoint.
    InspectFundamentalCycle,
    /// Fundamental cycles were queried.
    QueryMinimumRatioCycle,
    /// The sample missed the bounded quality band.
    SamplingFailure,
    /// One source potential step completed.
    PotentialReductionStep,
    /// Slowly changing coordinates were detected.
    DetectChangedCoordinates,
    /// The sampled chain was rebuilt.
    RebuildTreeChain,
    /// A geometric checkpoint of one exact integral assignment.
    InspectFeasibleAssignment,
    /// Bounded integral return-edge circulations were enumerated.
    EnumerateFeasibleSet,
    /// Independent isolation perturbations were sampled.
    SampleIsolationCosts,
    /// A unique perturbed optimum was selected.
    SelectIsolatedOptimum,
    /// A source-accurate near-optimal final point was constructed.
    ConstructFinalPoint,
    /// Every flow coordinate was rounded to the nearest integer.
    RoundNearestInteger,
    /// The original max-flow certificate was checked.
    CheckCertificate,
    /// Certified maximum flow is public.
    Optimal,
}

/// One original node plus its artificial initial-point edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowRandomizedAlmostLinearNodeStateV1 {
    /// Stable original-node identity.
    pub node_id: String,
    /// Selected-forest parent; `__artificial_star__` denotes the source star.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tree_parent_node_id: Option<String>,
    /// Selected-forest component ordinal.
    pub tree_component: String,
    /// Membership in the certified terminal min cut.
    pub source_side: bool,
    /// Artificial direction in `{-1,0,1}`.
    pub artificial_direction: String,
    /// Current strict-interior artificial flow.
    pub artificial_flow: String,
    /// Artificial capacity.
    pub artificial_capacity: String,
    /// Number of sampled forests containing the artificial edge.
    pub artificial_tree_memberships: String,
    /// Membership in the selected forest.
    pub active_artificial_tree_edge: bool,
    /// Active-cycle sign on the artificial edge.
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub active_artificial_sign: String,
}

/// One original arc in the sampled-tree/IPM projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowRandomizedAlmostLinearEdgeStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Current strict-interior flow.
    pub interior_flow: String,
    /// Current source gradient.
    pub gradient: String,
    /// Current positive source length.
    pub length: String,
    /// Number of sampled trees containing the edge.
    pub sampled_tree_memberships: String,
    /// Membership in the selected forest.
    pub active_tree_edge: bool,
    /// Signed active fundamental-cycle membership.
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub active_cycle_sign: String,
    /// Whether Detect refreshed the coordinate.
    pub changed_coordinate: bool,
    /// Isolation draw on this coordinate.
    pub isolation_draw: String,
    /// Feasible near-optimal final-point coordinate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_flow: Option<String>,
    /// Exact rounded integral flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_flow: Option<String>,
}

/// Exact finite-population probability.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowRandomizedAlmostLinearProbabilityV1 {
    /// Reduced nonnegative numerator.
    pub numerator: String,
    /// Reduced positive denominator.
    pub denominator: String,
}

/// Dedicated return-reduction, tree-chain, IPM, and final-point projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowRandomizedAlmostLinearOverlayV1 {
    /// Current semantic boundary.
    pub stage: FlowRandomizedAlmostLinearStageV1,
    /// Fixed replay seed.
    pub seed: String,
    /// PRNG draws consumed.
    pub random_draws: String,
    /// Source parameter alpha.
    pub alpha: String,
    /// Current source potential.
    pub potential: String,
    /// Current cost gap above `F*`.
    pub cost_gap: String,
    /// Best sampled ratio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selected_ratio: Option<String>,
    /// Exact finite-pool ratio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exact_pool_ratio: Option<String>,
    /// Exact probability that all draws miss a good forest.
    pub miss_probability: FlowRandomizedAlmostLinearProbabilityV1,
    /// Forest population size.
    pub forest_pool_size: String,
    /// Draws per tree chain.
    pub sample_count: String,
    /// Completed source steps.
    pub iteration: String,
    /// Current rebuild epoch.
    pub rebuild_epoch: String,
    /// Strict-interior return flow.
    pub return_flow: String,
    /// Return capacity `mU`.
    pub return_capacity: String,
    /// Return gradient.
    pub return_gradient: String,
    /// Return length.
    pub return_length: String,
    /// Number of sampled forests containing the return edge.
    pub return_tree_memberships: String,
    /// Membership in the selected forest.
    pub active_return_tree_edge: bool,
    /// Active-cycle sign on `t -> s`.
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub active_return_sign: String,
    /// Isolation draw on the return edge.
    pub return_isolation_draw: String,
    /// Near-optimal return-edge final-point coordinate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_return_flow: Option<String>,
    /// Exact rounded return flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_return_flow: Option<String>,
    /// Number of artificial-star edges.
    pub artificial_edges: String,
    /// Sum of strict-interior artificial flow.
    pub artificial_flow: String,
    /// Exact rounded artificial flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_artificial_flow: Option<String>,
    /// Integral scale `D = 4 M^2 U^2` used for perturbation.
    pub isolation_scale: String,
    /// Successful one-based isolation attempt.
    pub isolation_attempt: String,
    /// Probability bound that all completed isolation attempts failed.
    pub isolation_failure_probability: FlowRandomizedAlmostLinearProbabilityV1,
    /// Scaled objective of the unique isolated optimum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub isolated_objective: Option<String>,
    /// Source additive final-point threshold.
    pub final_point_threshold: String,
    /// Verified final-point objective gap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_gap: Option<String>,
    /// Bounded-oracle convex mixing coefficient.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_mix: Option<String>,
    /// Exact bounded max-flow target.
    pub target_value: String,
    /// Canonical node projections.
    pub nodes: Vec<FlowRandomizedAlmostLinearNodeStateV1>,
    /// Canonical original-edge projections.
    pub edges: Vec<FlowRandomizedAlmostLinearEdgeStateV1>,
}

/// Publication boundary in the bounded deterministic almost-linear realization.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum FlowDeterministicAlmostLinearStageV1 {
    Ready,
    BuildReturnEdgeReduction,
    BuildInitialPoint,
    EnumerateForestPool,
    InstallBranchRecord,
    BuildBranchCollection,
    BuildCoreGraph,
    BuildSpannerEmbedding,
    InspectFundamentalCycle,
    QueryMinimumRatioCycle,
    QueryFailure,
    ShiftBranch,
    RebuildDeeperLevels,
    PotentialReductionStep,
    DetectChangedCoordinates,
    ScheduledRebuild,
    EnumerateFeasibleSet,
    ConstructFinalPoint,
    RoundingIntegralEdge,
    RoundingLinkFractionalEdge,
    RoundingCancelFractionalCycle,
    FinishFlowRounding,
    CheckCertificate,
    Optimal,
}

/// Selected deterministic candidate family.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum FlowDeterministicAlmostLinearCycleKindV1 {
    Tree,
    Spanner,
}

/// One original node plus its artificial strict-interior edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)]
pub struct FlowDeterministicAlmostLinearNodeStateV1 {
    pub node_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tree_parent_node_id: Option<String>,
    pub forest_component: String,
    pub source_side: bool,
    pub artificial_direction: String,
    pub artificial_flow: String,
    pub artificial_capacity: String,
    pub artificial_tree_level_mask: String,
    pub active_artificial_tree_edge: bool,
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub active_artificial_sign: String,
}

/// One original edge in the deterministic tree/core/spanner projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
// These booleans intentionally describe independent SVG layers that may be
// active at the same publication boundary.
#[allow(clippy::struct_excessive_bools, missing_docs)]
pub struct FlowDeterministicAlmostLinearEdgeStateV1 {
    pub edge_id: String,
    pub interior_flow: String,
    pub gradient: String,
    pub length: String,
    pub tree_level_mask: String,
    pub forest_level_mask: String,
    pub active_tree_edge: bool,
    pub active_core_edge: bool,
    pub active_spanner_edge: bool,
    pub embedding_hops: String,
    pub embedding_stretch: String,
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub active_cycle_sign: String,
    pub changed_coordinate: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_flow: Option<FlowRationalV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rounding_flow: Option<FlowRationalV1>,
    pub rounding_forest_edge: bool,
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub rounding_cycle_sign: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_flow: Option<String>,
}

/// Dedicated deterministic shift/rebuild, core/spanner, IPM, and rounding state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[allow(missing_docs)]
pub struct FlowDeterministicAlmostLinearOverlayV1 {
    pub stage: FlowDeterministicAlmostLinearStageV1,
    pub alpha: String,
    pub potential: String,
    pub cost_gap: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selected_ratio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exact_pool_ratio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selected_off_tree_edge: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selected_cycle_kind: Option<FlowDeterministicAlmostLinearCycleKindV1>,
    pub forest_pool_size: String,
    pub level_count: String,
    pub branch_count: String,
    pub built_branch_records: String,
    pub active_branches: Vec<String>,
    pub passes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_level: Option<String>,
    pub fundamental_cycles: String,
    pub core_vertices: String,
    pub core_edges: String,
    pub spanner_edges: String,
    pub embedding_hops: String,
    pub iteration: String,
    pub rebuild_epoch: String,
    pub return_flow: String,
    pub return_capacity: String,
    pub return_gradient: String,
    pub return_length: String,
    pub return_tree_level_mask: String,
    pub active_return_tree_edge: bool,
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub active_return_sign: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_return_flow: Option<FlowRationalV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rounding_return_flow: Option<FlowRationalV1>,
    pub rounding_return_forest_edge: bool,
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub rounding_return_sign: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_return_flow: Option<String>,
    pub artificial_edges: String,
    pub artificial_flow: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_artificial_flow: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_gap: Option<FlowRationalV1>,
    pub final_point_threshold: FlowRationalV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_point_mix: Option<FlowRationalV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rounding_processed_edge: Option<String>,
    pub target_value: String,
    pub nodes: Vec<FlowDeterministicAlmostLinearNodeStateV1>,
    pub edges: Vec<FlowDeterministicAlmostLinearEdgeStateV1>,
}

/// Publication boundary in the bounded electrical-flow IPM MCF realization.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum FlowElectricalIpmMcfStageV1 {
    Ready,
    NormalizeLowerBounds,
    IsolationAttempt,
    SelectIsolatedCosts,
    ContractFixedFace,
    InitializeDualInterior,
    AssembleElectricalLaplacian,
    SolveNewtonDirection,
    DampedCenteringStep,
    Centered,
    DecreaseBarrier,
    ApproximateFlow,
    RoundNearestInteger,
    CheckCertificate,
    Optimal,
}

/// One original-node projection of the electrical Newton system.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowElectricalIpmMcfNodeStateV1 {
    /// Canonical original node.
    pub node_id: String,
    /// Dual potential.
    pub potential: String,
    /// Latest Newton potential direction.
    pub potential_direction: String,
    /// Approximate primal balance residual.
    pub balance_residual: String,
    /// Gauge anchor of the weak working component.
    pub anchored: bool,
}

/// One original-edge projection of isolation and barrier quantities.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct FlowElectricalIpmMcfEdgeStateV1 {
    /// Canonical original edge.
    pub edge_id: String,
    /// Isolation perturbation.
    pub perturbation: String,
    /// Integer isolated cost.
    pub isolated_cost: String,
    /// Coordinate is fixed on the exact integer feasible face.
    pub fixed_on_face: bool,
    /// Exact lower coordinate of that face.
    pub face_lower: String,
    /// Exact upper coordinate of that face.
    pub face_upper: String,
    /// Fractional central estimate in original coordinates.
    pub fractional_flow: String,
    /// Upper-complement estimate.
    pub upper_complement: String,
    /// Lower dual slack.
    pub lower_slack: String,
    /// Upper-bound multiplier.
    pub upper_multiplier: String,
    /// Electrical resistance.
    pub resistance: String,
    /// Electrical conductance.
    pub conductance: String,
    /// Latest electrical current.
    pub electrical_current: String,
    /// Latest lower-slack Newton direction.
    pub lower_slack_direction: String,
    /// Latest upper-multiplier Newton direction.
    pub upper_multiplier_direction: String,
    /// Rounded terminal flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub final_flow: Option<String>,
}

/// Dedicated Daitch--Spielman standard-MCF isolation and electrical-IPM state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowElectricalIpmMcfOverlayV1 {
    /// Current source/recovery boundary.
    pub stage: FlowElectricalIpmMcfStageV1,
    /// Reproducible isolation seed.
    pub seed: String,
    /// Current logarithmic barrier parameter.
    pub mu: String,
    /// Source short-step decrement.
    pub epsilon_3: String,
    /// Source exact-recovery accuracy.
    pub recovery_epsilon: String,
    /// Current barrier duality-gap bound.
    pub duality_gap_bound: String,
    /// Normalized central-neighborhood residual.
    pub centrality_residual: String,
    /// Maximum balance residual.
    pub balance_residual: String,
    /// Latest accepted Newton step.
    pub step_size: String,
    /// Latest electrical energy.
    pub electrical_energy: String,
    /// Latest dense-solve residual.
    pub linear_residual: String,
    /// Current dual barrier objective.
    pub barrier_objective: String,
    /// Isolation scale `4m^2U^2`.
    pub isolation_scale: String,
    /// Perturbation range upper bound `2mU`.
    pub perturbation_bound: String,
    /// Current/accepted one-based isolation attempt.
    pub isolation_attempt: String,
    /// Exact perturbed objective of the unique isolated optimum.
    pub isolated_optimum_cost: String,
    /// Exact isolated optimum gap.
    pub isolated_gap: String,
    /// Original-node state.
    pub nodes: Vec<FlowElectricalIpmMcfNodeStateV1>,
    /// Original-edge state.
    pub edges: Vec<FlowElectricalIpmMcfEdgeStateV1>,
}

/// Publication boundary in the bounded integer primal-dual MCF realization.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum FlowPrimalDualIpmMcfStageV1 {
    Ready,
    NormalizeInput,
    BuildCapacityReduction,
    InitializeCentralPoint,
    BuildMinor,
    DecreaseMu,
    InspectForestSubset,
    BuildLowStretchForest,
    SampleFundamentalCycle,
    CenteringCycleUpdate,
    Centered,
    ProxyReached,
    CrossoverGrowCut,
    RestoreOriginalDual,
    RecoverAdmissibleFlow,
    CheckCertificate,
    Optimal,
}

/// Kind of one node in the source capacitated-to-uncapacitated reduction.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowPrimalDualIpmMcfNodeKindV1 {
    /// Canonical node from the user graph.
    Original,
    /// Per-edge node enforcing the original finite capacity.
    Capacity,
}

/// Role of one uncapacitated auxiliary arc.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowPrimalDualIpmMcfArcKindV1 {
    /// Cost-carrying original-tail branch.
    Upper,
    /// Zero-cost original-head complement branch.
    Lower,
    /// High-cost Appendix-A initialization branch.
    Artificial,
}

/// Exact reduced rational used for the tree condition number.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPrimalDualIpmMcfRatioV1 {
    /// Canonical signed numerator.
    pub numerator: String,
    /// Canonical positive denominator.
    pub denominator: String,
}

/// One node in the explicit auxiliary graph.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPrimalDualIpmMcfNodeStateV1 {
    /// Stable scene-local identity (`node:*` or `capacity:*`).
    pub auxiliary_id: String,
    /// Original/capacity role.
    pub kind: FlowPrimalDualIpmMcfNodeKindV1,
    /// Canonical original node for `original` entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub original_node_id: Option<String>,
    /// Canonical original edge for `capacity` entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub original_edge_id: Option<String>,
    /// Scaled integral dual potential.
    pub potential: String,
    /// Stable sticky-contraction component.
    pub component: String,
    /// Membership in the current nested crossover cut.
    pub in_crossover_set: bool,
}

/// One arc in the explicit auxiliary graph.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
// The booleans are independent renderer layers and can overlap at crossover.
#[allow(clippy::struct_excessive_bools)]
pub struct FlowPrimalDualIpmMcfArcStateV1 {
    /// Stable scene-local arc identity.
    pub auxiliary_id: String,
    /// Canonical original edge represented by the branch.
    pub original_edge_id: String,
    /// Stable auxiliary tail identity.
    pub from: String,
    /// Stable auxiliary head identity.
    pub to: String,
    /// Capacity-reduction role.
    pub kind: FlowPrimalDualIpmMcfArcKindV1,
    /// Scaled positive primal coordinate (zero only before initialization).
    pub flow: String,
    /// Scaled positive dual slack (zero is allowed during crossover).
    pub slack: String,
    /// Current integer resistance on active minor arcs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resistance: Option<String>,
    /// Sticky primal deletion.
    pub deleted: bool,
    /// Sticky dual contraction.
    pub contracted: bool,
    /// Membership in the current minor.
    pub in_minor: bool,
    /// Membership in the low-stretch or crossover tree.
    pub in_tree: bool,
    /// Membership in the exact forest subset currently being inspected.
    pub forest_candidate: bool,
    /// `-1`, `0`, or `1` orientation in the sampled fundamental cycle.
    #[schemars(schema_with = "flow_ternary_sign_schema")]
    #[ts(type = "\"-1\" | \"0\" | \"1\"")]
    pub active_cycle_sign: String,
}

/// Dedicated integer-grid, minor, centering, and crossover projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPrimalDualIpmMcfOverlayV1 {
    /// Current source boundary.
    pub stage: FlowPrimalDualIpmMcfStageV1,
    /// Reproducible randomized centering seed.
    pub seed: String,
    /// Current central-path parameter.
    pub mu: String,
    /// Demand/capacity integer-grid scale.
    pub beta: String,
    /// Cost integer-grid scale.
    pub gamma: String,
    /// Active-minor complementarity gap.
    pub proxy_gap: String,
    /// Exact one-norm centrality numerator.
    pub centrality_numerator: String,
    /// Latest sampled auxiliary arc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sampled_arc: Option<String>,
    /// Latest rounded cycle correction.
    pub cycle_alpha: String,
    /// Exact selected-forest condition number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tree_condition_number: Option<FlowPrimalDualIpmMcfRatioV1>,
    /// Monotone one-based subset enumeration ordinal during forest selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub forest_subset_serial: Option<String>,
    /// Explicit auxiliary nodes.
    pub nodes: Vec<FlowPrimalDualIpmMcfNodeStateV1>,
    /// Explicit auxiliary arcs.
    pub arcs: Vec<FlowPrimalDualIpmMcfArcStateV1>,
}

/// Publication boundary within exact double scaling.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowDoubleScalingStageV1 {
    /// Transportation mapping exists before the initialization event.
    Ready,
    /// Lower-bound removal and transportation mapping were published.
    Initialize,
    /// A new epsilon phase reset transformed flow and shifted right prices.
    StartCostPhase,
    /// A new integral imbalance scale began.
    StartCapacityPhase,
    /// A canonical large-excess root was selected.
    SelectRoot,
    /// One transformed residual arc was inspected by the current-arc scan.
    InspectArc,
    /// One admissible transformed residual arc extended the path.
    Advance,
    /// A dead-end tip price changed by epsilon.
    Relabel,
    /// The nonadmissible predecessor was removed.
    Retreat,
    /// Exact delta flow was sent over the active path.
    Augment,
    /// The current transformed pseudoflow became feasible.
    CompleteCostPhase,
    /// Independent minimum-cost certification succeeded.
    Optimal,
}

/// Kind of one node in the transformed transportation network.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowDoubleScalingNodeKindV1 {
    /// Left-side node corresponding to an original graph node.
    Original,
    /// Right-side demand node corresponding to a positive-width original edge.
    Edge,
}

/// Exact double-scaling state for one transformed node.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowDoubleScalingNodeStateV1 {
    /// Stable original node or edge identity.
    pub entity_id: String,
    /// Left original-node or right edge-node classification.
    pub kind: FlowDoubleScalingNodeKindV1,
    /// Exact scaled project-sign price.
    pub price: String,
    /// Required divergence minus current divergence.
    pub imbalance: String,
    /// Persistent canonical outgoing-arc cursor.
    pub cursor: String,
}

/// One residual direction of a transformed flow or slack branch.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowDoubleScalingArcRefV1 {
    /// Stable original edge identity owning both transportation branches.
    pub edge_id: String,
    /// `flow` for tail-to-edge-node or `slack` for head-to-edge-node.
    #[schemars(schema_with = "flow_flow_slack_schema")]
    #[ts(type = "\"flow\" | \"slack\"")]
    pub branch: String,
    /// `forward` or `reverse` relative to the transportation branch.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
}

/// Transformed branch flows associated with one positive-width original edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowDoubleScalingEdgeStateV1 {
    /// Stable original edge identity.
    pub edge_id: String,
    /// Flow on the costed tail-to-edge-node branch.
    pub flow_branch: String,
    /// Flow on the zero-cost head-to-edge-node slack branch.
    pub slack_branch: String,
}

/// Dedicated projection of the exact transformed double-scaling state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowDoubleScalingOverlayV1 {
    /// Current source-algorithm boundary.
    pub stage: FlowDoubleScalingStageV1,
    /// Current scaled epsilon error.
    pub epsilon: String,
    /// Exact positive cost multiplier used by the kernel.
    pub cost_multiplier: String,
    /// Current integral imbalance scale, or zero outside an inner phase.
    pub delta: String,
    /// One-based outer cost phase.
    pub cost_phase: String,
    /// One-based inner capacity phase within the outer phase.
    pub capacity_phase: String,
    /// Every transformed node in canonical left-then-right order.
    pub nodes: Vec<FlowDoubleScalingNodeStateV1>,
    /// Both transformed branch flows for every positive-width edge.
    pub edges: Vec<FlowDoubleScalingEdgeStateV1>,
    /// Every negative-reduced-cost transformed residual arc.
    pub admissible_arcs: Vec<FlowDoubleScalingArcRefV1>,
    /// Ordered current admissible path, retained at augmentation boundaries.
    pub active_path: Vec<FlowDoubleScalingArcRefV1>,
    /// Exact transformed residual arc inspected at this boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub inspected_arc: Option<FlowDoubleScalingArcRefV1>,
    /// Canonical `node:<id>` or `edge:<id>` selected root identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selected_root: Option<String>,
    /// Canonical selected deficit endpoint at augmentation boundaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selected_deficit: Option<String>,
}

/// Publication boundary of the segment-expanded convex-cost oracle.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowConvexCostStageV1 {
    /// A canonical feasible segment occupancy is available.
    Initialize,
    /// A minimum-mean marginal residual cycle was selected.
    SelectMinimumMeanCycle,
    /// The active cycle was canceled to its exact bottleneck.
    CancelCycle,
    /// A powers-of-two native capacity scale began.
    StartScale,
    /// One negative eligible marginal piece was saturated.
    SaturateMarginal,
    /// One native marginal residual arc is being inspected by Dijkstra.
    InspectMarginalArc,
    /// A reduced-cost path through marginal pieces was selected.
    ShortestPath,
    /// Exact shortest-path distances were applied to node potentials.
    UpdatePotentials,
    /// Flow was augmented to an imbalance or segment boundary.
    Augment,
    /// No eligible source-to-deficit path remains at the current scale.
    CompleteScale,
    /// No negative marginal residual cycle remains.
    Optimal,
}

/// One replay-visible interval of an original convex edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowConvexCostSegmentStateV1 {
    /// Zero-based canonical segment ordinal.
    pub segment: String,
    /// Inclusive lower flow boundary.
    pub start_flow: String,
    /// Exclusive upper flow boundary.
    pub end_flow: String,
    /// Current prefix occupancy within this segment.
    pub flow: String,
    /// Exact marginal unit cost.
    pub marginal_cost: String,
}

/// Current aggregate and marginal state of one original edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowConvexCostEdgeStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Exact constant objective contribution at zero flow.
    pub base_cost_at_zero: String,
    /// Aggregate current original-edge flow.
    pub flow: String,
    /// Exact current objective contribution of this edge.
    pub total_cost: String,
    /// Cost of adding the next unit, absent at capacity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub forward_marginal_cost: Option<String>,
    /// Cost recovered by removing the last unit, absent at the lower bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reverse_marginal_cost: Option<String>,
    /// Complete canonical segment partition.
    pub segments: Vec<FlowConvexCostSegmentStateV1>,
}

/// One active expanded residual segment projected onto an original edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowConvexCostArcRefV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Zero-based segment ordinal.
    pub segment: String,
    /// `forward` adds flow and `reverse` removes flow.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub direction: String,
}

/// Dedicated native convex-cost projection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowConvexCostOverlayV1 {
    /// Current source-algorithm boundary.
    pub stage: FlowConvexCostStageV1,
    /// Current powers-of-two native scale, absent for the expanded oracle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scale: Option<String>,
    /// Every original edge in canonical identity order.
    pub edges: Vec<FlowConvexCostEdgeStateV1>,
    /// Ordered active expanded residual cycle.
    pub active_cycle: Vec<FlowConvexCostArcRefV1>,
    /// Every current marginal residual piece eligible at the active scale.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub eligible_arcs: Vec<FlowConvexCostArcRefV1>,
}

/// Exact inputs for one segment-expanded convex-cost scene boundary.
pub struct FlowConvexCostBoundary<'a> {
    /// Aggregate flow on each original edge.
    pub flows: &'a [u64],
    /// Optional exact node label or final dual potential.
    pub node_labels: &'a [Option<i128>],
    /// Stable node search order at this boundary.
    pub search_order: &'a [crate::model::NodeId],
    /// Exact remaining divergence; empty for the expanded cycle oracle.
    pub remaining_divergence: &'a [i128],
    /// Segment occupancy, marginal costs, and active cycle.
    pub overlay: FlowConvexCostOverlayV1,
    /// Exact trace event identity.
    pub event_id: u64,
    /// Exact number of trace events in the prepared run.
    pub event_count: u64,
}

/// Why one standalone binary blocking-flow invocation stopped.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowBinaryBlockingTerminationV1 {
    /// No admissible source-to-sink path remains before reaching delta.
    Blocking,
    /// Exactly delta flow units were delivered.
    DeltaReached,
}

/// Current algorithm-owned forest and optional strong-branch partition.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPseudoflowForestV1 {
    /// In-tree residual directions. Dynamic-tree blocking flow intentionally
    /// uses child-to-represented-root direction; other current users expose
    /// parent-to-child direction.
    pub arcs: Vec<FlowResidualArcRefV1>,
    /// Canonical node identities in branches with positive root excess.
    pub strong_nodes: Vec<String>,
}

/// Dedicated renderer-facing Excesses-IBFS pseudoflow state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowEibfsOverlayV1 {
    /// `forward` grows the source forest; `reverse` grows the sink forest.
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub phase_direction: String,
    /// Exact source-forest boundary label.
    pub source_depth: String,
    /// Exact sink-forest boundary label.
    pub sink_depth: String,
    /// Canonical explicit node membership and retained labels.
    pub nodes: Vec<FlowEibfsNodeStateV1>,
    /// Explicit structural parent/child plus actual admissible residual direction.
    pub forest_arcs: Vec<FlowEibfsForestArcV1>,
}

/// One renderer-facing EIBFS node state.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowEibfsNodeStateV1 {
    /// Stable node identity.
    pub node_id: String,
    /// Retained source-distance label.
    pub source_label: String,
    /// Retained sink-distance label.
    pub sink_label: String,
    /// `free`, `source`, or `sink` forest membership.
    #[schemars(schema_with = "flow_eibfs_membership_schema")]
    #[ts(type = "\"free\" | \"source\" | \"sink\"")]
    pub membership: String,
    /// `none`, `source`, `sink`, `excess`, or `deficit`.
    #[schemars(schema_with = "flow_eibfs_root_kind_schema")]
    #[ts(type = "\"none\" | \"source\" | \"sink\" | \"excess\" | \"deficit\"")]
    pub root_kind: String,
    /// Whether the node is awaiting parent repair.
    pub orphan: bool,
    /// Exact finite pseudoflow imbalance.
    pub imbalance: String,
}

/// One renderer-facing EIBFS forest relation.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowEibfsForestArcV1 {
    /// Structural parent node.
    pub parent: String,
    /// Structural child node.
    pub child: String,
    /// `source` or `sink` forest.
    #[schemars(schema_with = "flow_source_sink_schema")]
    #[ts(type = "\"source\" | \"sink\"")]
    pub side: String,
    /// Actual admissible residual direction; child-to-parent for the sink forest.
    pub admissible_residual: FlowResidualArcRefV1,
}

/// Renderer-facing Dynamic EIBFS update and prefix context.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowDynamicEibfsOverlayV1 {
    /// Stable kebab-case update/recovery stage.
    #[schemars(schema_with = "flow_dynamic_eibfs_stage_schema")]
    #[ts(
        type = "\"initial-solve\" | \"apply-update\" | \"repair-capacity\" | \"repair-forest\" | \"repair-violation\" | \"continue-solve\" | \"prefix-recovery\" | \"prefix-certified\" | \"resume-reusable-pseudoflow\""
    )]
    pub stage: String,
    /// Prefix zero is the initial graph.
    pub update_index: String,
    /// Fixed total number of capacity updates.
    pub update_total: String,
    /// Changed stable edge for nonzero prefixes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub changed_edge: Option<String>,
    /// Capacity immediately before the current update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub old_capacity: Option<String>,
    /// Capacity installed by the current update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub new_capacity: Option<String>,
    /// Stable violation kind repaired at this boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[schemars(schema_with = "flow_dynamic_eibfs_violation_schema")]
    #[ts(type = "\"over-capacity\" | \"bridge\" | \"label\" | \"current-arc\" | \"boundary\"")]
    pub violation: Option<String>,
    /// Cumulative exactly retained forest nodes.
    pub reused_forest_nodes: String,
    /// Cumulative applied updates, including no-ops.
    pub updates_applied: String,
    /// Cumulative strict capacity increases.
    pub capacity_increases: String,
    /// Cumulative strict capacity decreases.
    pub capacity_decreases: String,
    /// Cumulative no-op updates.
    pub no_op_updates: String,
    /// Cumulative over-capacity repairs.
    pub over_capacity_repairs: String,
    /// Cumulative invalidated parent arcs.
    pub invalidated_parent_arcs: String,
    /// Cumulative promoted correct-sign roots.
    pub promoted_roots: String,
    /// Cumulative dynamic repair residual-arc scans.
    pub repair_arc_scans: String,
    /// Cumulative reusable warm solver state transitions.
    pub state_transitions: String,
    /// Cumulative new bridge repairs.
    pub bridge_violations: String,
    /// Cumulative label repairs.
    pub label_violations: String,
    /// Cumulative current-arc rewinds.
    pub current_arc_violations: String,
    /// Cumulative forest-boundary repairs.
    pub boundary_violations: String,
    /// Cumulative stabilization iterations.
    pub repair_iterations: String,
    /// Cumulative certification-clone recovery paths.
    pub certification_recoveries: String,
    /// Independently certified prefix value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub prefix_value: Option<String>,
}

/// Exact renderer state for one parameter location and the progressively
/// published complete-analysis regions.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowParametricOverlayV1 {
    /// Source-owned traversal stage rendered in the graph and inspector.
    /// This duplicates `traversal.kind` while a trace is running so generic
    /// overlay presentation never has to infer a stage from nested state.
    pub stage: String,
    /// Current exact parameter selected by the traversal event.
    pub parameter: FlowRationalV1,
    /// Current exact capacities keyed by stable original edge identity.
    pub edge_capacities: Vec<FlowParametricEdgeCapacityV1>,
    /// Fixed maximum capacity over both domain endpoints, used for stable
    /// thickness/color scaling while replay seeks between parameters.
    pub visual_scale_max_capacity: FlowRationalV1,
    /// Certified segments published by this replay boundary.
    pub recorded_segments: Vec<FlowParametricSegmentV1>,
    /// Certified atomic transitions published by this replay boundary.
    pub recorded_breakpoints: Vec<FlowParametricBreakpointV1>,
    /// Current retained-forest/contraction operation, absent in fast output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub traversal: Option<FlowParametricTraversalV1>,
}

/// Exact evaluated capacity of one original edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowParametricEdgeCapacityV1 {
    /// Stable original edge identity.
    pub edge_id: String,
    /// Nonnegative exact affine capacity at the current parameter.
    pub capacity: FlowRationalV1,
}

/// One exact affine optimum region, including the complete tie interval.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowParametricSegmentV1 {
    /// Inclusive lower endpoint.
    pub lower: FlowRationalV1,
    /// Inclusive upper endpoint.
    pub upper: FlowRationalV1,
    /// Affine value intercept.
    pub intercept: String,
    /// Affine value slope.
    pub slope: String,
    /// Inclusion-minimal source side on the open interior.
    pub minimal_source_side: Vec<String>,
    /// Inclusion-maximal source side on the open interior. A different set
    /// makes the full degenerate tie span explicit.
    pub maximal_source_side: Vec<String>,
}

/// One exact, atomic nested source-set transition.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowParametricBreakpointV1 {
    /// Exact breakpoint parameter.
    pub parameter: FlowRationalV1,
    /// Source side immediately before the transition.
    pub before_source_side: Vec<String>,
    /// Source side immediately after the transition.
    pub after_source_side: Vec<String>,
    /// Inclusion-minimal source side exactly at the breakpoint.
    pub exact_minimal_source_side: Vec<String>,
    /// Inclusion-maximal source side exactly at the breakpoint.
    pub exact_maximal_source_side: Vec<String>,
    /// Simultaneously entering nodes; no intermediate subset is claimed.
    pub entering_nodes: Vec<String>,
}

/// Current canonical parametric-pseudoflow semantic operation.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowParametricTraversalV1 {
    /// Stable kebab-case event kind.
    pub kind: String,
    /// Current closed recursive interval lower endpoint.
    pub lower: FlowRationalV1,
    /// Current closed recursive interval upper endpoint.
    pub upper: FlowRationalV1,
    /// Exact race probe or breakpoint when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub probe: Option<FlowRationalV1>,
    /// `forward` or `reverse` for a single-forest event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub orientation: Option<String>,
    /// `forward` or `reverse` first finisher for a cooperative race.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[schemars(schema_with = "flow_direction_schema")]
    #[ts(type = "\"forward\" | \"reverse\"")]
    pub race_winner: Option<String>,
    /// Whether this event performs a deliberately cold static solve.
    #[serde(default)]
    pub cold_static_rerun: bool,
    /// One-based cold static-run ordinal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub static_run_ordinal: Option<String>,
    /// Integralization denominator of a cold static subproblem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scale_denominator: Option<String>,
    /// Lower endpoint or before-breakpoint source side for cold comparison.
    #[serde(default)]
    pub lower_source_side: Vec<String>,
    /// Upper endpoint or after-breakpoint source side for cold comparison.
    #[serde(default)]
    pub upper_source_side: Vec<String>,
    /// Whether a real normalized tree was retained.
    pub normalized_tree_reused: bool,
    /// Whether preexisting labels were retained without decrease.
    pub labels_retained: bool,
    /// Active vertices before contraction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_nodes: Option<String>,
    /// Active vertices in the lower child view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub left_active_nodes: Option<String>,
    /// Active vertices in the upper child view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub right_active_nodes: Option<String>,
    /// Appendix-B balance pushes caused by this event.
    pub renormalization_pushes: String,
    /// Appendix-B parent splits caused by this event.
    pub renormalization_splits: String,
}

/// Named exact counters for the selected complete-analysis implementation.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "implementation", rename_all = "kebab-case")]
pub enum FlowParametricMetricsV1 {
    /// Explicit-tree retained forward/reverse normalized forests.
    ParametricPseudoflow {
        /// Fresh normalized-forest initializations.
        forest_initializations: String,
        /// Monotone parameter retargets.
        parameter_advances: String,
        /// Retargets that retained an existing forest and labels.
        forest_reuses: String,
        /// Appendix-B balance pushes.
        renormalization_pushes: String,
        /// Appendix-B parent splits.
        renormalization_splits: String,
        /// Labeling-pseudoflow mergers.
        mergers: String,
        /// Label increases.
        relabels: String,
        /// Cooperative forward/reverse races.
        free_run_races: String,
        /// Forward race wins.
        forward_race_wins: String,
        /// Reverse race wins.
        reverse_race_wins: String,
        /// One-merger cooperative race steps.
        cooperative_race_steps: String,
        /// Logical contraction child views.
        contraction_views: String,
        /// Smaller-child fresh restarts.
        smaller_child_restarts: String,
        /// Larger-child checkpoint continuations.
        larger_child_continuations: String,
        /// Maximum exact-intersection recursion depth.
        maximum_depth: String,
        /// Residual arcs inspected by retained normalized-forest runs.
        residual_arc_scans: String,
    },
    /// Deliberately cold static rerun oracle used for comparison and checking.
    BreakpointRerun {
        /// Static Hochbaum pseudoflow runs.
        pseudoflow_runs: String,
        /// Independent Edmonds–Karp oracle runs.
        oracle_runs: String,
        /// Residual arcs inspected inside all cold static solver runs.
        static_residual_arc_scans: String,
        /// Exact affine intersections considered.
        intersections: String,
        /// Recursive subintervals processed.
        subproblems: String,
        /// Certified optimum segments.
        segments: String,
        /// Exact source-set transitions.
        breakpoints: String,
        /// Transitions containing multiple simultaneous nodes.
        simultaneous_breakpoints: String,
        /// Maximum exact-intersection recursion depth.
        maximum_depth: String,
    },
}

/// Replay-visible annotations for one original node.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowNodeTraceStateV1 {
    /// Stable original node identity.
    pub node_id: String,
    /// Algorithm-defined exact label, absent when unvisited or inactive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    /// First zero-based occurrence in the current deterministic search order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub search_ordinal: Option<u32>,
    /// Remaining signed divergence for min-cost-flow routing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub remaining_divergence: Option<String>,
}

/// Bounded metadata for the currently committed trace event.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowTraceEventSceneV1 {
    /// Exact event identity.
    pub event_id: String,
    /// Owning phase event identity when this is an operation/micro event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent_phase_id: Option<String>,
    /// Revision-owned catalog identity.
    pub catalog_id: String,
    /// Coarsest requested granularity that shows this event directly.
    pub minimum_granularity: TraceGranularityV1,
    /// Revision-owned pseudocode line identity.
    pub pseudocode_line: String,
    /// Number of reversible patches in the transaction.
    pub patch_count: u32,
    /// Sorted stable identities affected or highlighted by this event.
    pub entity_refs: Vec<FlowTraceEntityRefSceneV1>,
    /// Optional exact selector/phase scalar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<FlowTraceEventDetailSceneV1>,
}

/// Dominant renderer-visible effect of one published event boundary.
///
/// The timeline normalizer derives this after projection with the precedence
/// certify, primal commit, working-state mutation, selection, observation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowTraceEventRoleV1 {
    /// Reads or exposes state without selecting a persistent structure.
    Observe,
    /// Selects a path, cycle, vertex, arc, or phase-local structure.
    Select,
    /// Changes algorithm working state without committing primal flow data.
    Mutate,
    /// Commits primal flow, capacity, or balance state.
    Commit,
    /// Publishes an independently checked terminal result.
    Certify,
}

/// Exact counter unit advanced by a published event.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize, TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum FlowTraceWorkUnitV1 {
    /// One public timeline transition, present for every event.
    PublishedTransition,
    /// One visible detail boundary, including exact primary-work interpolation.
    DetailPrimitive,
    /// Increments of the endpoint's catalog-declared primary work counter.
    PrimaryWork,
    /// Complete breadth-first searches.
    BfsRun,
    /// Complete Bellman–Ford-style relaxation passes.
    RelaxationPass,
    /// Positive residual arcs inspected.
    ResidualArcScan,
    /// Successful flow augmentations.
    Augmentation,
    /// Non-BFS augmenting-path searches.
    PathSearch,
    /// Capacity, excess, cost, or epsilon scale phases.
    ScalingPhase,
    /// Completed blocking-flow phases.
    BlockingFlowPhase,
    /// Distance-label or price relabels.
    Relabel,
    /// Distance-label retreats.
    Retreat,
    /// Reverse breadth-first-search initializations.
    ReverseBfsRun,
    /// Gap-heuristic terminations.
    GapTermination,
    /// Local flow pushes.
    Push,
    /// Saturating local pushes.
    SaturatingPush,
    /// Nonsaturating local pushes.
    NonsaturatingPush,
    /// Active-vertex discharges.
    Discharge,
    /// Active-vertex selections.
    ActiveVertexSelection,
    /// Dual-potential or vertex-price updates.
    PotentialUpdate,
    /// Network-simplex pivots, including bound flips and basis exchanges.
    SimplexPivot,
    /// Complete searches for a negative residual cycle.
    NegativeCycleSearch,
    /// Successful negative-cycle cancellations.
    CycleCancellation,
    /// Saturations of one explicitly selected residual arc.
    ArcSaturation,
}

/// One nonnegative exact counter delta attributable to the current event.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowTraceWorkDeltaV1 {
    /// Counter kind; unlike a generic scalar, this remains comparable across algorithms.
    pub unit: FlowTraceWorkUnitV1,
    /// Canonical unsigned decimal.
    pub count: String,
}

/// Semantic facts common to generic reversible events and custom snapshots.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowTraceEventSemanticsV1 {
    /// Dominant published effect: observe, select, mutate, commit, or certify.
    pub role: FlowTraceEventRoleV1,
    /// Exact counter deltas; heterogeneous units are never summed together.
    pub work_deltas: Vec<FlowTraceWorkDeltaV1>,
    /// Largest same-unit work delta represented by this publication, at least 1.
    pub aggregation_count: String,
    /// Exact position in both the visible Detail stream and primary work stream.
    pub work_progress: FlowTraceWorkProgressV1,
    /// Exact action-local primary-work block owned by this boundary, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub primary_work_block: Option<FlowTracePrimaryWorkBlockV1>,
    /// Stable graph entities whose published projection changed at this boundary.
    pub changed_entity_refs: Vec<FlowTraceEntityRefSceneV1>,
}

/// Exact action-local range represented by one primary-work boundary.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowTracePrimaryWorkBlockV1 {
    /// One-based first counted unit in the source action.
    pub first: String,
    /// One-based last counted unit in the source action.
    pub last: String,
    /// Total counted units owned by the source action.
    pub total: String,
}

/// Exact work position for one immutable published trace boundary.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowTraceWorkProgressV1 {
    /// Number of complexity-faithful Detail boundaries published through this event.
    pub detail_completed: String,
    /// Number of complexity-faithful Detail boundaries in the complete trace.
    pub detail_total: String,
    /// Primary implementation-work units completed through this event.
    pub primary_completed: String,
    /// Primary implementation-work units in the complete trace.
    pub primary_total: String,
}

/// Serializable exact scalar attached to a trace event.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowTraceEventDetailSceneV1 {
    /// Short semantic label owned by the event catalog revision.
    pub label: String,
    /// Exact signed value.
    pub value: String,
}

/// Serializable stable trace entity identity.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "kebab-case")]
pub enum FlowTraceEntityRefSceneV1 {
    /// Original node.
    Node {
        /// Stable node identity.
        node_id: String,
    },
    /// Original edge.
    Edge {
        /// Stable edge identity.
        edge_id: String,
    },
    /// Derived residual direction.
    ResidualArc {
        /// Stable original edge identity.
        edge_id: String,
        /// `forward` or `reverse`.
        #[schemars(schema_with = "flow_direction_schema")]
        #[ts(type = "\"forward\" | \"reverse\"")]
        direction: String,
    },
}

/// Current visual state of one original edge.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowEdgeStateV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Canonical unsigned flow decimal.
    pub flow: String,
}

/// Solver-independent result information safe for renderer publication.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "kebab-case")]
pub enum FlowOutcomeV1 {
    /// Maximum-flow value and lower-aware original minimum cut.
    MaxFlow {
        /// Exact signed net source outflow.
        value: String,
        /// Exact lower-aware cut bound.
        cut_bound: String,
        /// Original node IDs on the source side of the cut.
        source_side: Vec<String>,
    },
    /// Independently checked completion of one binary blocking-flow primitive.
    BinaryBlockingFlow {
        /// Positive valid upper bound used by this invocation.
        upper_bound: String,
        /// Positive integral primitive cap.
        delta: String,
        /// Exact flow units delivered by this invocation.
        delivered: String,
        /// Source-defined stopping condition; this is not a max-flow claim.
        termination: FlowBinaryBlockingTerminationV1,
        /// Number of canonical zero-length SCCs.
        component_count: String,
        /// Number of SCCs containing at least two original nodes.
        nontrivial_component_count: String,
        /// Number of residual augmentation operations including SCC lifting.
        augmentation_operations: String,
    },
    /// One independently checked Tardos network-matrix variable-fixing step.
    TardosFramework {
        /// Least nonnegative epsilon satisfying all residual inequalities.
        epsilon: String,
        /// Exact strict fixing threshold `n * epsilon`.
        threshold: String,
        /// Incidence-matrix determinant bound, always one here.
        determinant_bound: String,
        /// Original variables proved equal to a bound in every optimum.
        fixed_variables: Vec<FlowTardosFixedVariableV1>,
    },
    /// Independently checked unit-current minimum-energy electrical primitive.
    ElectricalFlow {
        /// Approximate unit-current effective source-sink resistance.
        effective_resistance: String,
        /// Exact rational effective resistance from the independent oracle.
        exact_effective_resistance: FlowRationalV1,
        /// Approximate total electrical energy, equal to effective resistance.
        total_energy: String,
        /// Terminal PCG residual norm.
        residual_l2: String,
        /// Largest potential/current error against the exact oracle.
        maximum_absolute_error: String,
        /// Completed PCG iterations.
        iterations: String,
    },
    /// Independently checked undirected minimum-ratio-cycle primitive.
    MinimumRatioCycle {
        /// Exact selected ratio; absent iff the input graph is acyclic.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        ratio: Option<FlowRationalV1>,
        /// Canonical selected signed original edges.
        cycle: Vec<FlowMinimumRatioCycleArcV1>,
        /// Number of exact simple-cycle candidates evaluated.
        simple_cycles: String,
        /// Number of ternary sign vectors inspected.
        enumerated_vectors: String,
    },
    /// One independently checked source-level MCF potential-reduction step.
    MinimumRatioCycleMcf {
        /// Selected exact ratio; absent for a stationary feasible face.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        ratio: Option<String>,
        /// Canonical selected signed original edges.
        cycle: Vec<FlowMinimumRatioCycleArcV1>,
        /// Source alpha-power exponent.
        alpha: String,
        /// Certified quality parameter.
        kappa: String,
        /// Source step multiplier.
        eta: String,
        /// Measured source potential decrease.
        potential_decrease: String,
        /// Promised source potential decrease.
        guaranteed_decrease: String,
        /// Whether no improving source step was required.
        stationary: bool,
    },
    /// Complete exact minimum-cut analysis over a closed parameter interval.
    ParametricMaxFlow {
        /// All certified affine optimum regions in increasing parameter order.
        segments: Vec<FlowParametricSegmentV1>,
        /// All exact nested source-set transitions in increasing order.
        breakpoints: Vec<FlowParametricBreakpointV1>,
        /// Named exact implementation counters.
        metrics: Box<FlowParametricMetricsV1>,
    },
    /// Minimum cost and a reconstructed feasible dual potential.
    MinCostFlow {
        /// Exact signed total cost.
        total_cost: String,
        /// One dual potential per canonical node.
        potentials: Vec<FlowNodePotentialV1>,
    },
    /// Maximum flow/minimum cut plus the minimum cost at that exact value.
    MinCostMaxFlow {
        /// Exact signed net source outflow.
        value: String,
        /// Exact lower-aware cut bound.
        cut_bound: String,
        /// Original node IDs on the source side of the cut.
        source_side: Vec<String>,
        /// Exact signed total cost.
        total_cost: String,
        /// One dual potential per canonical node.
        potentials: Vec<FlowNodePotentialV1>,
    },
    /// Maximum-cardinality bipartite matching and minimum vertex cover.
    BipartiteMatching {
        /// Exact matching cardinality.
        cardinality: String,
        /// Matched compatibility edges in canonical edge-ID order.
        pairs: Vec<FlowBipartiteMatchingPairV1>,
        /// Left vertices belonging to the reconstructed minimum cover.
        cover_left: Vec<String>,
        /// Right vertices belonging to the reconstructed minimum cover.
        cover_right: Vec<String>,
    },
    /// Complete rectangular assignment with oriented primal/dual equality.
    Assignment {
        /// `minimize` or `maximize` as declared by the model.
        objective: crate::assignment::AssignmentObjectiveV1,
        /// Exact objective in the original edge-cost orientation.
        total_cost: String,
        /// One selected allowed edge per agent.
        pairs: Vec<FlowAssignmentPairV1>,
        /// One oriented dual label per canonical agent.
        agent_labels: Vec<FlowAssignmentLabelV1>,
        /// One oriented dual label per canonical task.
        task_labels: Vec<FlowAssignmentLabelV1>,
    },
    /// Hall-deficient agent neighborhood proving assignment infeasibility.
    AssignmentInfeasible {
        /// Exact positive value `|hall_agents| - |neighbor_tasks|`.
        deficiency: String,
        /// Canonical deficient subset of agents.
        hall_agents: Vec<String>,
        /// Exact canonical allowed-edge neighborhood.
        neighbor_tasks: Vec<String>,
    },
    /// Original-node cut proving that required imbalance cannot be routed.
    Infeasible {
        /// Exact unsatisfied transformed imbalance.
        unsatisfied: String,
        /// Original nodes on the reachable side of the verified auxiliary cut.
        reachable_original_nodes: Vec<String>,
    },
}

/// Serializable matched compatibility edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowBipartiteMatchingPairV1 {
    /// Stable compatibility-edge identity.
    pub edge_id: String,
    /// Stable left-vertex identity.
    pub left: String,
    /// Stable right-vertex identity.
    pub right: String,
}

/// Serializable selected assignment edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowAssignmentPairV1 {
    /// Stable allowed-edge identity.
    pub edge_id: String,
    /// Stable agent identity.
    pub agent: String,
    /// Stable task identity.
    pub task: String,
    /// Original signed edge cost.
    pub cost: String,
}

/// Serializable assignment dual label.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowAssignmentLabelV1 {
    /// Stable partition-node identity.
    pub node_id: String,
    /// Exact oriented dual label.
    pub label: String,
}

/// Stable node identity paired with an exact signed potential.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowNodePotentialV1 {
    /// Stable original-node identity.
    pub node_id: String,
    /// Reconstructed exact potential.
    pub potential: String,
}

/// Closed high-level solve state shown by the flow workspace.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowSolveStatusV1 {
    /// Input is validated; no solve event has been committed.
    Ready,
    /// A verified trace boundary is committed but no final certificate yet.
    Running,
    /// A standalone algorithm primitive completed and its invariant checker passed.
    PrimitiveComplete,
    /// A complete independently checked optimum is present.
    Optimal,
    /// A complete independently checked infeasibility witness is present.
    Infeasible,
    /// Deterministic resource admission or runtime work ceiling was reached.
    ResourceLimit,
    /// The candidate was cancelled and the last committed scene remains active.
    Cancelled,
}

/// Public cause for a solver boundary that intentionally carries no candidate result.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowResourceLimitReasonV1 {
    /// The canonical input is outside the implementation's published admission band.
    InputAdmission,
    /// The algorithm reached its deterministic source-work ceiling before certification.
    RuntimeWork,
    /// An algorithm-owned transformed graph would exceed its deterministic ceiling.
    TransformedGraph,
    /// The complete event timeline would exceed the publication-size ceiling.
    TracePublication,
    /// A bounded numerical iteration did not converge within its published limit.
    NumericalConvergence,
    /// A bounded path did not expose a more specific limit category.
    DeclaredCeiling,
}

/// Renderer projection failure for an otherwise typed solver boundary.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum FlowSceneError {
    /// Snapshot and immutable canonical graph disagree.
    #[error("flow scene snapshot does not match the canonical graph")]
    SnapshotGraphMismatch,
    /// A bounded count cannot be represented by the scene contract.
    #[error("flow scene bounded count overflow")]
    CountOverflow,
    /// Reconstructing a residual state from certified flows failed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
}

impl FlowCurrentSceneV9 {
    /// Creates the initial validated-input scene.
    #[must_use]
    pub fn ready(
        model: FlowProblemModelV1,
        graph: FlowGraphV1,
        algorithm: FlowAlgorithmSelectionV1,
        run_profile: RunProfileV1,
        trace_granularity: TraceGranularityV1,
        trace_steps: AlgorithmStepContractV1,
    ) -> Self {
        let edge_states = graph
            .edges
            .iter()
            .map(|edge| FlowEdgeStateV1 {
                edge_id: edge.id.clone(),
                flow: edge
                    .initial_flow
                    .clone()
                    .unwrap_or_else(|| edge.lower.clone()),
            })
            .collect();
        Self {
            result_schema_version: 9,
            frame_revision: FRAME_ENCODING_REVISION.to_owned(),
            event_id: "0".to_owned(),
            event_count: "0".to_owned(),
            solve_status: FlowSolveStatusV1::Ready,
            resource_limit_reason: None,
            model,
            graph,
            algorithm,
            run_profile,
            trace_granularity,
            trace_steps: trace_steps.into(),
            edge_states,
            residual_arcs: Vec::new(),
            node_trace_states: Vec::new(),
            pseudoflow_forest: None,
            eibfs_overlay: None,
            dynamic_eibfs_overlay: None,
            parametric_overlay: None,
            feasibility_overlay: None,
            feasibility_work: None,
            binary_blocking_overlay: None,
            cancel_tighten_overlay: None,
            relaxed_mndc_overlay: None,
            enhanced_capacity_scaling_overlay: None,
            orlin_mcf_overlay: None,
            orlin_max_flow_overlay: None,
            electrical_flow_overlay: None,
            augmenting_electrical_overlay: None,
            interior_point_max_flow_overlay: None,
            minimum_ratio_cycle_overlay: None,
            minimum_ratio_cycle_mcf_overlay: None,
            randomized_almost_linear_mcf_overlay: None,
            flow_framework_mcf_overlay: None,
            weighted_augmenting_paths_overlay: None,
            weighted_push_relabel_shortcut_overlay: None,
            randomized_almost_linear_overlay: None,
            deterministic_almost_linear_overlay: None,
            primal_dual_ipm_mcf_overlay: None,
            electrical_ipm_mcf_overlay: None,
            dual_network_simplex_overlay: None,
            polynomial_dual_simplex_overlay: None,
            polynomial_primal_simplex_overlay: None,
            double_scaling_overlay: None,
            convex_cost_overlay: None,
            convex_network_simplex_overlay: None,
            prediction_assisted_epsilon_overlay: None,
            tardos_framework_overlay: None,
            trace_event: None,
            trace_event_semantics: None,
            outcome: None,
            metrics: std::array::from_fn(|_| "0".to_owned()),
        }
    }

    /// Publishes allocation-free feasibility work retained by a fast-profile
    /// run. A zero-invocation execution has no summary.
    pub fn set_feasibility_work(&mut self, summary: FeasibilityMetricSummary) {
        self.feasibility_work = (summary.invocations != 0).then(|| FlowFeasibilityWorkSummaryV1 {
            invocations: summary.invocations.to_string(),
            metrics: feasibility_metrics(summary.total),
        });
    }

    /// Applies a certified maximum-flow result to a ready scene.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_max_flow_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        bfs_runs: u64,
        residual_arc_scans: u128,
        augmentations: u64,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[0] = bfs_runs.to_string();
        self.metrics[2] = residual_arc_scans.to_string();
        self.metrics[3] = augmentations.to_string();
        Ok(())
    }

    /// Applies a certified st-planar dual-shortest-path result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_planar_max_flow_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowPlanarMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[2] = metrics.dual_arc_scans.to_string();
        self.metrics[4] = metrics.dual_shortest_path_runs.to_string();
        self.metrics[5] = metrics.dual_faces.to_string();
        self.metrics[11] = metrics.positive_flow_edges.to_string();
        self.metrics[15] = metrics.settled_faces.to_string();
        Ok(())
    }

    /// Applies a certified bounded leftmost-path planar maximum-flow result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_leftmost_planar_max_flow_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowLeftmostPlanarMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[0] = metrics.right_first_searches.to_string();
        self.metrics[1] = metrics.preprocessing_runs.to_string();
        self.metrics[2] = metrics.dart_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.right_first_searches.to_string();
        self.metrics[5] = metrics.dual_faces.to_string();
        self.metrics[6] = metrics.preprocessing_runs.to_string();
        self.metrics[12] = metrics.saturated_path_darts.to_string();
        self.metrics[15] = metrics.discovered_vertices.to_string();
        Ok(())
    }

    /// Applies a certified non-BFS augmenting-path maximum-flow result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_augmenting_path_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowAugmentingPathMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.path_searches.to_string();
        self.metrics[5] = metrics.scaling_phases.to_string();
        Ok(())
    }

    /// Applies a certified blocking-flow maximum-flow result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_blocking_flow_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowBlockingFlowMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[0] = metrics.bfs_runs.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[6] = metrics.blocking_flow_phases.to_string();
        Ok(())
    }

    /// Applies a certified Goldberg--Rao maximum-flow result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_goldberg_rao_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowGoldbergRaoMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[0] = metrics.distance_searches.to_string();
        self.metrics[1] = metrics.phases.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.update_steps.to_string();
        self.metrics[4] = metrics.canonical_cut_evaluations.to_string();
        self.metrics[6] = metrics.blocking_updates.to_string();
        self.metrics[7] = metrics.zero_length_arc_observations.to_string();
        self.metrics[8] = metrics.special_arc_observations.to_string();
        self.metrics[9] = metrics.nontrivial_contractions.to_string();
        self.metrics[10] = metrics.cut_updates.to_string();
        self.metrics[11] = metrics.contracted_augmentations.to_string();
        self.metrics[12] = metrics.delta_limited_updates.to_string();
        self.metrics[13] = metrics.component_routing_paths.to_string();
        self.metrics[14] = metrics.augmented_units.to_string();
        self.metrics[15] = metrics.state_transitions.to_string();
        Ok(())
    }

    /// Applies one independently checked binary blocking-flow primitive.
    ///
    /// This deliberately publishes `primitive-complete`, not `optimal`: one
    /// Goldberg--Rao subproblem does not certify a maximum flow.
    ///
    /// # Errors
    ///
    /// Rejects a flow vector or residual identity that does not match the
    /// immutable graph.
    pub fn apply_binary_blocking_result(
        &mut self,
        graph: &FlowNetwork,
        result: &BinaryBlockingStepResult,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, &result.flows)?;
        self.set_binary_blocking_overlay(graph, result, FlowBinaryBlockingStageV1::Complete)?;
        self.set_binary_blocking_outcome(result);
        self.apply_binary_blocking_metrics(result.metrics);
        Ok(())
    }

    /// Projects binary lengths and SCC membership at a trace boundary.
    ///
    /// # Errors
    ///
    /// Rejects unknown node or residual identities.
    pub fn set_binary_blocking_overlay(
        &mut self,
        graph: &FlowNetwork,
        result: &BinaryBlockingStepResult,
        stage: FlowBinaryBlockingStageV1,
    ) -> Result<(), FlowSceneError> {
        self.binary_blocking_overlay = Some(binary_blocking_overlay(graph, result, stage)?);
        Ok(())
    }

    /// Attaches the checked primitive outcome without claiming max-flow optimality.
    pub fn set_binary_blocking_outcome(&mut self, result: &BinaryBlockingStepResult) {
        let (component_count, nontrivial_component_count) =
            binary_component_counts(&result.component_of);
        self.solve_status = FlowSolveStatusV1::PrimitiveComplete;
        self.outcome = Some(FlowOutcomeV1::BinaryBlockingFlow {
            upper_bound: result.upper_bound.to_string(),
            delta: result.delta.to_string(),
            delivered: result.value.to_string(),
            termination: if result.blocking {
                FlowBinaryBlockingTerminationV1::Blocking
            } else {
                FlowBinaryBlockingTerminationV1::DeltaReached
            },
            component_count: component_count.to_string(),
            nontrivial_component_count: nontrivial_component_count.to_string(),
            augmentation_operations: result.augmentation.len().to_string(),
        });
    }

    fn apply_binary_blocking_metrics(&mut self, metrics: crate::algorithms::GoldbergRaoMetrics) {
        self.metrics[0] = metrics.distance_searches.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        "1".clone_into(&mut self.metrics[3]);
        self.metrics[6] = metrics.blocking_updates.to_string();
        self.metrics[7] = metrics.zero_length_arc_observations.to_string();
        self.metrics[8] = metrics.special_arc_observations.to_string();
        self.metrics[9] = metrics.nontrivial_contractions.to_string();
        self.metrics[11] = metrics.contracted_augmentations.to_string();
        self.metrics[12] = metrics.delta_limited_updates.to_string();
        self.metrics[13] = metrics.component_routing_paths.to_string();
        self.metrics[14] = metrics.augmented_units.to_string();
        self.metrics[15] = metrics.state_transitions.to_string();
    }

    /// Applies a certified dynamic-tree blocking-flow result.
    ///
    /// The fixed scene metric vector is interpreted by the algorithm-specific
    /// inspector as path-minimum, prune, link, cut, and path-update counters.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_dynamic_tree_blocking_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowDynamicTreeBlockingMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[0] = metrics.bfs_runs.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.path_minimum_queries.to_string();
        self.metrics[6] = metrics.blocking_flow_phases.to_string();
        self.metrics[8] = metrics.dead_end_prunes.to_string();
        self.metrics[11] = metrics.tree_links.to_string();
        self.metrics[12] = metrics.tree_cuts.to_string();
        self.metrics[13] = metrics.path_updates.to_string();
        self.metrics[15] = metrics.state_transitions.to_string();
        Ok(())
    }

    /// Applies a certified dynamic-tree FIFO push--relabel result.
    ///
    /// The fixed scene metric vector mirrors the trace projection so fast and
    /// trace profiles expose the same paper-specific counters.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_dynamic_tree_push_relabel_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowDynamicTreePushRelabelMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[0] = metrics.source_pushes.to_string();
        self.metrics[1] = metrics.tree_size_limit.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.tree_path_sends.to_string();
        self.metrics[4] = metrics.component_size_queries.to_string();
        self.metrics[5] = metrics.tree_links.to_string();
        self.metrics[6] = metrics.tree_cuts.to_string();
        self.metrics[7] = metrics.relabels.to_string();
        self.metrics[8] = metrics.final_tree_materializations.to_string();
        self.metrics[9] = metrics.queue_additions.to_string();
        self.metrics[10] = metrics.size_gate_rejections.to_string();
        self.metrics[11] = metrics.pushes.to_string();
        self.metrics[12] = metrics.saturating_pushes.to_string();
        self.metrics[13] = metrics.nonsaturating_pushes.to_string();
        self.metrics[14] = metrics.discharges.to_string();
        self.metrics[15] = metrics.active_vertex_selections.to_string();
        Ok(())
    }

    /// Applies a certified layered-network blocking-preflow result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_blocking_preflow_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowBlockingPreflowMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[0] = metrics.bfs_runs.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[6] = metrics.blocking_flow_phases.to_string();
        self.metrics[11] = metrics.pushes.to_string();
        self.metrics[12] = metrics.saturating_pushes.to_string();
        self.metrics[13] = metrics.nonsaturating_pushes.to_string();
        self.metrics[14] = metrics.balancing_iterations.to_string();
        self.metrics[15] = metrics.vertex_eliminations.to_string();
        Ok(())
    }

    /// Applies a certified distance-label maximum-flow result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_distance_label_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowDistanceLabelMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[7] = metrics.relabels.to_string();
        self.metrics[8] = metrics.retreats.to_string();
        self.metrics[9] = metrics.reverse_bfs_runs.to_string();
        self.metrics[10] = metrics.gap_terminations.to_string();
        Ok(())
    }

    /// Applies a certified DD2 exact-tree maximum-flow result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_distance_directed_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowDistanceDirectedMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[1] = metrics.current_arc_advances.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.tree_repairs.to_string();
        self.metrics[5] = metrics.scaling_phases.to_string();
        self.metrics[7] = metrics.relabels.to_string();
        self.metrics[8] = metrics.invalid_tree_arcs.to_string();
        self.metrics[9] = metrics.reverse_bfs_runs.to_string();
        self.metrics[10] = metrics.node_deletions.to_string();
        self.metrics[11] = metrics.tree_arc_replacements.to_string();
        self.metrics[12] = metrics.saturated_tree_arcs.to_string();
        self.metrics[13] = metrics.cascading_invalidations.to_string();
        self.metrics[15] = metrics.state_transitions.to_string();
        Ok(())
    }

    /// Applies a certified push–relabel maximum-flow result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_push_relabel_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowPushRelabelMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.path_searches.to_string();
        self.metrics[5] = metrics.scaling_phases.to_string();
        self.metrics[7] = metrics.relabels.to_string();
        self.metrics[8] = metrics.retreats.to_string();
        self.metrics[9] = metrics.global_relabels.to_string();
        self.metrics[10] = metrics.gap_relabels.to_string();
        self.metrics[11] = metrics.pushes.to_string();
        self.metrics[12] = metrics.saturating_pushes.to_string();
        self.metrics[13] = metrics.nonsaturating_pushes.to_string();
        self.metrics[14] = metrics.discharges.to_string();
        self.metrics[15] = metrics.active_vertex_selections.to_string();
        Ok(())
    }

    /// Applies a certified flow recovered from a predicted pseudoflow.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_warm_start_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowWarmStartMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[0] = metrics.auxiliary_solves.to_string();
        self.metrics[1] = metrics.eta.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.auxiliary_solves.to_string();
        self.metrics[4] = metrics.recovery_paths.to_string();
        self.metrics[5] = metrics.cut_saturation_error.to_string();
        self.metrics[6] = metrics.imbalance_error.to_string();
        self.metrics[7] = metrics.relabels.to_string();
        self.metrics[8] = metrics.cut_transfers.to_string();
        self.metrics[9] = metrics.predicted_positive_edges.to_string();
        self.metrics[10] = metrics.gap_relabels.to_string();
        self.metrics[11] = metrics.pushes.to_string();
        self.metrics[12] = metrics.saturating_pushes.to_string();
        self.metrics[13] = metrics.nonsaturating_pushes.to_string();
        self.metrics[14] = metrics.discharges.to_string();
        self.metrics[15] = metrics.active_vertex_selections.to_string();
        Ok(())
    }

    /// Applies a certified flow recovered from an optimal pseudoflow forest.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual scene.
    pub fn apply_pseudoflow_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowPseudoflowMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.recovery_paths.to_string();
        self.metrics[4] = metrics.pivot_cycle_arcs.to_string();
        self.metrics[5] = metrics.strong_root_leaves.to_string();
        self.metrics[6] = metrics.weak_root_leaves.to_string();
        self.metrics[7] = metrics.relabels.to_string();
        self.metrics[8] = metrics.internal_leaves.to_string();
        self.metrics[9] = metrics.entering_leaves.to_string();
        self.metrics[11] = metrics.pushes.to_string();
        self.metrics[12] = metrics.saturating_pushes.to_string();
        self.metrics[13] = metrics.nonsaturating_pushes.to_string();
        self.metrics[14] = metrics.degenerate_pivots.to_string();
        self.metrics[15] = metrics.mergers.to_string();
        Ok(())
    }

    /// Applies a certified standard-IBFS maximum-flow result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual scene.
    pub fn apply_ibfs_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowIbfsMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[0] = metrics.passes.to_string();
        self.metrics[1] = metrics.forward_passes.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.augmented_path_arcs.to_string();
        self.metrics[5] = metrics.reverse_passes.to_string();
        self.metrics[6] = metrics.tree_attachments.to_string();
        self.metrics[7] = metrics.orphan_relabels.to_string();
        self.metrics[8] = metrics.tree_removals.to_string();
        self.metrics[9] = metrics.adoption_arc_scans.to_string();
        self.metrics[10] = metrics.orphan_creations.to_string();
        self.metrics[11] = metrics.orphan_visits.to_string();
        self.metrics[12] = metrics.saturated_tree_arcs.to_string();
        self.metrics[13] = metrics.same_level_adoptions.to_string();
        self.metrics[14] = metrics.active_vertex_scans.to_string();
        self.metrics[15] = metrics.state_transitions.to_string();
        Ok(())
    }

    /// Applies a certified Excesses-IBFS result after explicit pseudoflow recovery.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual scene.
    pub fn apply_eibfs_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MaxFlowCertificate,
        metrics: FlowEibfsMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        self.metrics[0] = metrics.phases.to_string();
        self.metrics[1] = metrics.forward_phases.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.bridge_pushes.to_string();
        self.metrics[4] = metrics.tree_path_pushes.to_string();
        self.metrics[5] = metrics.reverse_phases.to_string();
        self.metrics[6] = metrics.tree_attachments.to_string();
        self.metrics[7] = metrics.orphan_relabels.to_string();
        self.metrics[8] = metrics.tree_removals.to_string();
        self.metrics[9] = metrics.adoption_arc_scans.to_string();
        self.metrics[10] = metrics.orphan_creations.to_string();
        self.metrics[11] = metrics.orphan_visits.to_string();
        self.metrics[12] = metrics.saturated_tree_arcs.to_string();
        self.metrics[13] = metrics.side_migrations.to_string();
        self.metrics[14] = metrics.recovery_cancellations.to_string();
        self.metrics[15] = metrics.state_transitions.to_string();
        Ok(())
    }

    /// Applies the final certified Dynamic EIBFS prefix and its reuse summary.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the final
    /// current-capacity residual scene.
    pub fn apply_dynamic_eibfs_result(
        &mut self,
        graph: &FlowNetwork,
        result: &DynamicEibfsResult,
    ) -> Result<(), FlowSceneError> {
        let prefix = result
            .prefixes
            .last()
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let changed_edge = prefix
            .changed_edge
            .as_ref()
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let old_capacity = prefix
            .old_capacity
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let new_capacity = prefix
            .new_capacity
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let metrics = result.dynamic_metrics;
        self.apply_fast_snapshot(graph, &prefix.flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(&prefix.certificate);
        self.dynamic_eibfs_overlay = Some(FlowDynamicEibfsOverlayV1 {
            stage: "prefix-certified".to_owned(),
            update_index: prefix.update_index.to_string(),
            update_total: result
                .prefixes
                .len()
                .checked_sub(1)
                .ok_or(FlowSceneError::CountOverflow)?
                .to_string(),
            changed_edge: Some(changed_edge.as_str().to_owned()),
            old_capacity: Some(old_capacity.to_string()),
            new_capacity: Some(new_capacity.to_string()),
            violation: None,
            reused_forest_nodes: metrics.exactly_reused_forest_nodes.to_string(),
            updates_applied: metrics.updates.to_string(),
            capacity_increases: metrics.capacity_increases.to_string(),
            capacity_decreases: metrics.capacity_decreases.to_string(),
            no_op_updates: metrics.no_op_updates.to_string(),
            over_capacity_repairs: metrics.over_capacity_repairs.to_string(),
            invalidated_parent_arcs: metrics.invalidated_parent_arcs.to_string(),
            promoted_roots: metrics.promoted_roots.to_string(),
            repair_arc_scans: result.eibfs_metrics.dynamic_repair_arc_scans.to_string(),
            state_transitions: result.eibfs_metrics.state_transitions.to_string(),
            bridge_violations: metrics.bridge_violations.to_string(),
            label_violations: metrics.label_violations.to_string(),
            current_arc_violations: metrics.current_arc_violations.to_string(),
            boundary_violations: metrics.boundary_violations.to_string(),
            repair_iterations: metrics.repair_iterations.to_string(),
            certification_recoveries: metrics.certification_recoveries.to_string(),
            prefix_value: Some(prefix.certificate.value.to_string()),
        });
        self.metrics = [
            metrics.updates.to_string(),
            metrics.capacity_increases.to_string(),
            result.eibfs_metrics.dynamic_repair_arc_scans.to_string(),
            metrics.bridge_violations.to_string(),
            metrics.label_violations.to_string(),
            metrics.capacity_decreases.to_string(),
            metrics.exactly_reused_forest_nodes.to_string(),
            metrics.current_arc_violations.to_string(),
            metrics.invalidated_parent_arcs.to_string(),
            metrics.boundary_violations.to_string(),
            metrics.over_capacity_repairs.to_string(),
            metrics.promoted_roots.to_string(),
            metrics.no_op_updates.to_string(),
            metrics.repair_iterations.to_string(),
            metrics.certification_recoveries.to_string(),
            result.eibfs_metrics.state_transitions.to_string(),
        ];
        Ok(())
    }

    /// Applies a certified native bipartite-matching result.
    ///
    /// # Errors
    ///
    /// Rejects a unit-flow projection that cannot reconstruct the scene state.
    pub fn apply_bipartite_matching_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &BipartiteMatchingCertificate,
        metrics: FlowBipartiteMatchingMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.set_bipartite_matching_outcome(certificate);
        self.metrics[0] = metrics.bfs_runs.to_string();
        self.metrics[2] = metrics.edge_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.dfs_roots.to_string();
        self.metrics[6] = metrics.phases.to_string();
        Ok(())
    }

    /// Applies a certified native assignment optimum.
    ///
    /// # Errors
    ///
    /// Rejects a unit-flow projection that cannot reconstruct the scene state.
    pub fn apply_assignment_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &AssignmentCertificate,
        metrics: FlowAssignmentMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.set_assignment_outcome(graph, certificate);
        self.set_assignment_metrics(metrics);
        Ok(())
    }

    /// Applies a verified Hall-infeasible result and its final partial matching.
    ///
    /// # Errors
    ///
    /// Rejects a partial unit-flow projection that cannot reconstruct the scene.
    pub fn apply_assignment_infeasibility(
        &mut self,
        graph: &FlowNetwork,
        partial_flows: &[u64],
        witness: &AssignmentHallWitness,
        metrics: FlowAssignmentMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, partial_flows)?;
        self.set_assignment_infeasible_outcome(witness);
        self.set_assignment_metrics(metrics);
        Ok(())
    }

    /// Applies a certified native assignment auction optimum.
    ///
    /// # Errors
    ///
    /// Rejects a unit-flow projection that cannot reconstruct the scene state.
    pub fn apply_auction_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &AssignmentCertificate,
        metrics: FlowAuctionMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.set_assignment_outcome(graph, certificate);
        self.set_auction_metrics(metrics);
        Ok(())
    }

    /// Applies verified Hall infeasibility from the auction precheck.
    ///
    /// # Errors
    ///
    /// Rejects a partial unit-flow projection that cannot reconstruct the scene.
    pub fn apply_auction_infeasibility(
        &mut self,
        graph: &FlowNetwork,
        partial_flows: &[u64],
        witness: &AssignmentHallWitness,
        metrics: FlowAuctionMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, partial_flows)?;
        self.set_assignment_infeasible_outcome(witness);
        self.set_auction_metrics(metrics);
        Ok(())
    }

    /// Attaches a verified assignment outcome without changing trace state.
    pub fn set_assignment_outcome(
        &mut self,
        graph: &FlowNetwork,
        certificate: &AssignmentCertificate,
    ) {
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.outcome = Some(FlowOutcomeV1::Assignment {
            objective: certificate.objective,
            total_cost: certificate.total_cost.to_string(),
            pairs: certificate
                .pairs
                .iter()
                .map(|pair| FlowAssignmentPairV1 {
                    edge_id: pair.edge.as_str().to_owned(),
                    agent: pair.agent.as_str().to_owned(),
                    task: pair.task.as_str().to_owned(),
                    cost: pair.cost.to_string(),
                })
                .collect(),
            agent_labels: assignment_labels(graph, &certificate.agent_labels, true, &self.model),
            task_labels: assignment_labels(graph, &certificate.task_labels, false, &self.model),
        });
    }

    /// Attaches a verified Hall witness without changing trace state.
    pub fn set_assignment_infeasible_outcome(&mut self, witness: &AssignmentHallWitness) {
        self.solve_status = FlowSolveStatusV1::Infeasible;
        self.outcome = Some(FlowOutcomeV1::AssignmentInfeasible {
            deficiency: witness.deficiency.to_string(),
            hall_agents: witness
                .agents
                .iter()
                .map(|node| node.as_str().to_owned())
                .collect(),
            neighbor_tasks: witness
                .neighbor_tasks
                .iter()
                .map(|node| node.as_str().to_owned())
                .collect(),
        });
    }

    /// Projects native assignment counters into the fixed metrics vector.
    pub fn set_assignment_metrics(&mut self, metrics: FlowAssignmentMetrics) {
        self.metrics[0] = metrics.agent_searches.to_string();
        self.metrics[1] = metrics.dual_updates.to_string();
        self.metrics[2] = metrics.cell_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.predecessor_updates.to_string();
    }

    /// Projects auction counters into the fixed metrics vector.
    pub fn set_auction_metrics(&mut self, metrics: FlowAuctionMetrics) {
        self.metrics[0] = metrics.feasibility_searches.to_string();
        self.metrics[1] = metrics.price_raises.to_string();
        self.metrics[2] = metrics.edge_scans.to_string();
        self.metrics[3] = metrics.awards.to_string();
        self.metrics[4] = metrics.feasibility_augmentations.to_string();
        self.metrics[5] = metrics.scaling_phases.to_string();
        self.metrics[6] = metrics.evictions.to_string();
        self.metrics[15] = metrics.bids.to_string();
    }

    /// Applies a certified minimum-cost-flow result to a ready scene.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_min_cost_flow_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MinCostFlowCertificate,
        relaxation_passes: u64,
        residual_arc_scans: u128,
        augmentations: u64,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        self.metrics[1] = relaxation_passes.to_string();
        self.metrics[2] = residual_arc_scans.to_string();
        self.metrics[3] = augmentations.to_string();
        Ok(())
    }

    /// Projects one exact Cancel-and-Tighten boundary.
    ///
    /// # Errors
    ///
    /// Rejects mismatched flow, node, or residual identities.
    pub fn apply_cancel_tighten_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowCancelTightenOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_cancel_tighten_overlay(graph, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.cancel_tighten_overlay = Some(overlay);
        Ok(())
    }

    /// Projects exact Cancel-and-Tighten counters into the shared metric slots.
    pub fn set_cancel_tighten_metrics(
        &mut self,
        phases: u64,
        cycle_searches: u64,
        cancellations: u64,
        tightenings: u64,
        residual_arc_scans: u128,
    ) {
        self.metrics[2] = residual_arc_scans.to_string();
        self.metrics[3] = cancellations.to_string();
        self.metrics[4] = cycle_searches.to_string();
        self.metrics[5] = phases.to_string();
        self.metrics[7] = tightenings.to_string();
    }

    /// Projects one relaxed most-negative-cycle assignment boundary.
    ///
    /// # Errors
    ///
    /// Rejects mismatched flow, node, assignment, cycle, or residual identities.
    pub fn apply_relaxed_mndc_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowRelaxedMndcOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_relaxed_mndc_overlay(graph, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.relaxed_mndc_overlay = Some(overlay);
        Ok(())
    }

    /// Projects relaxed-MNDC nested assignment and cancellation counters.
    #[allow(clippy::too_many_arguments)]
    pub fn set_relaxed_mndc_metrics(
        &mut self,
        phases: u64,
        assignment_solves: u64,
        assignment_augmentations: u64,
        assignment_cell_scans: u128,
        residual_arc_scans: u128,
        canceled_families: u64,
        canceled_cycles: u64,
        canceled_cycle_arcs: u64,
        dropped_zero_cycles: u64,
    ) {
        self.metrics[0] = assignment_solves.to_string();
        self.metrics[1] = assignment_augmentations.to_string();
        self.metrics[2] = assignment_cell_scans.to_string();
        self.metrics[3] = canceled_cycles.to_string();
        self.metrics[4] = canceled_families.to_string();
        self.metrics[5] = phases.to_string();
        self.metrics[6] = canceled_cycle_arcs.to_string();
        self.metrics[7] = residual_arc_scans.to_string();
        self.metrics[8] = dropped_zero_cycles.to_string();
    }

    /// Projects one exact Orlin enhanced-capacity-scaling boundary.
    ///
    /// # Errors
    ///
    /// Rejects mismatched quotient membership, stable node/edge identities,
    /// active components, or residual path references.
    pub fn apply_enhanced_capacity_scaling_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowEnhancedCapacityScalingOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_enhanced_capacity_scaling_overlay(graph, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        for residual in &mut self.residual_arcs {
            residual.active = overlay.path.iter().any(|selected| {
                selected.edge_id == residual.edge_id && selected.direction == residual.direction
            });
        }
        self.enhanced_capacity_scaling_overlay = Some(overlay);
        Ok(())
    }

    /// Projects Orlin enhanced-scaling work counters into shared metric slots.
    pub fn set_enhanced_capacity_scaling_metrics(
        &mut self,
        metrics: crate::algorithms::EnhancedCapacityScalingMetrics,
    ) {
        self.metrics[0] = metrics.scaling_phases.to_string();
        self.metrics[1] = metrics.complete_regenerations.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.shortest_path_runs.to_string();
        self.metrics[5] = metrics.contractions.to_string();
        self.metrics[6] = metrics.augmented_arcs.to_string();
        self.metrics[7] = metrics.potential_updates.to_string();
        self.metrics[8] = metrics.primal_recoveries.to_string();
    }

    /// Projects one exact Orlin capacitated-MCF boundary.
    ///
    /// # Errors
    ///
    /// Rejects malformed capacity-node identities, quotient partitions,
    /// transformed branches, compressed paths, or exact rational values.
    pub fn apply_orlin_mcf_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowOrlinMcfOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_orlin_mcf_overlay(graph, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        for residual in &mut self.residual_arcs {
            residual.active = overlay.path.iter().any(|selected| {
                let projected_direction = match selected.branch {
                    FlowOrlinMcfBranchV1::Flow => selected.direction.as_str(),
                    FlowOrlinMcfBranchV1::Slack => match selected.direction.as_str() {
                        "forward" => "reverse",
                        "reverse" => "forward",
                        _ => return false,
                    },
                };
                selected.edge_id == residual.edge_id
                    && projected_direction == residual.direction.as_str()
            });
        }
        self.orlin_mcf_overlay = Some(overlay);
        Ok(())
    }

    /// Projects Orlin capacitated-MCF work counters into shared metric slots.
    pub fn set_orlin_mcf_metrics(&mut self, metrics: crate::algorithms::OrlinMcfMetrics) {
        self.metrics[0] = metrics.capacity_nodes.to_string();
        self.metrics[1] = metrics.scaling_phases.to_string();
        self.metrics[2] = metrics.complete_regenerations.to_string();
        self.metrics[3] = metrics.contractions.to_string();
        self.metrics[4] = metrics.shortest_path_runs.to_string();
        self.metrics[5] = metrics.eliminated_capacity_nodes.to_string();
        self.metrics[6] = metrics.shortcut_arcs.to_string();
        self.metrics[7] = metrics.augmentations.to_string();
        self.metrics[8] = metrics.augmented_arcs.to_string();
        self.metrics[9] = metrics.potential_updates.to_string();
        self.metrics[10] = metrics.residual_arc_scans.to_string();
        self.metrics[11] = metrics.primal_recoveries.to_string();
    }

    /// Projects one Orlin 2013 maximum-flow boundary.
    ///
    /// # Errors
    ///
    /// Rejects identity, residual-capacity, quotient partition, compact-arc,
    /// active-path, or exact-integer drift before mutating the scene.
    pub fn apply_orlin_max_flow_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowOrlinMaxFlowOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_orlin_max_flow_overlay(graph, flows, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        for residual in &mut self.residual_arcs {
            residual.active = overlay.active_original_path.iter().any(|selected| {
                selected.edge_id == residual.edge_id && selected.direction == residual.direction
            });
        }
        self.orlin_max_flow_overlay = Some(overlay);
        Ok(())
    }

    /// Projects Orlin 2013 maximum-flow counters into shared metric slots.
    pub fn set_orlin_max_flow_metrics(&mut self, metrics: crate::algorithms::OrlinMaxMetrics) {
        self.metrics[0] = metrics.improvement_phases.to_string();
        self.metrics[1] = metrics.abundant_arc_observations.to_string();
        self.metrics[2] = metrics.contractions.to_string();
        self.metrics[3] = metrics.critical_node_observations.to_string();
        self.metrics[4] = metrics.compact_networks.to_string();
        self.metrics[5] = metrics.capacity_transfers.to_string();
        self.metrics[6] = metrics.transferred_units.to_string();
        self.metrics[7] = metrics.pseudo_arcs.to_string();
        self.metrics[8] = metrics.approximate_subproblems.to_string();
        self.metrics[9] = metrics.exact_subproblems.to_string();
        self.metrics[10] = metrics.subproblem_augmentations.to_string();
        self.metrics[11] = metrics.lifted_paths.to_string();
        self.metrics[12] = metrics.expansion_paths.to_string();
        self.metrics[13] = metrics.cut_updates.to_string();
        self.metrics[14] = metrics.residual_arc_scans.to_string();
    }

    /// Projects one unit-current electrical-flow primitive boundary.
    ///
    /// Integer flow/residual fields remain empty because signed real currents
    /// are not capacity-feasible directed max-flow values.
    ///
    /// # Errors
    ///
    /// Rejects node/edge identity drift, malformed finite decimals, a wrong
    /// resistance mapping, or KCL/Ohm/energy inconsistency before mutation.
    pub fn apply_electrical_flow_boundary(
        &mut self,
        graph: &FlowNetwork,
        source: crate::model::NodeIndex,
        sink: crate::model::NodeIndex,
        overlay: FlowElectricalFlowOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_electrical_flow_overlay(graph, source, sink, &overlay)?;
        self.edge_states = graph
            .edges()
            .iter()
            .map(|edge| FlowEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                flow: "0".to_owned(),
            })
            .collect();
        self.residual_arcs.clear();
        self.node_trace_states = overlay
            .nodes
            .iter()
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                // Electrical quantities are finite decimals, while generic trace
                // labels intentionally remain exact integers. Keep the numerical
                // state exclusively in the typed electrical overlay.
                label: None,
                search_ordinal: None,
                remaining_divergence: None,
            })
            .collect();
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.electrical_flow_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes a checked minimum-energy primitive outcome without claiming
    /// maximum-flow feasibility or optimality.
    ///
    /// # Errors
    ///
    /// Rejects a missing or nonterminal exact-reference boundary.
    pub fn set_electrical_flow_outcome(&mut self) -> Result<(), FlowSceneError> {
        let overlay = self
            .electrical_flow_overlay
            .as_ref()
            .filter(|overlay| {
                overlay.stage == FlowElectricalFlowStageV1::Complete && overlay.converged
            })
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        self.solve_status = FlowSolveStatusV1::PrimitiveComplete;
        self.outcome = Some(FlowOutcomeV1::ElectricalFlow {
            effective_resistance: overlay.effective_resistance.clone(),
            exact_effective_resistance: overlay
                .exact_effective_resistance
                .clone()
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
            total_energy: overlay.total_energy.clone(),
            residual_l2: overlay.residual_l2.clone(),
            maximum_absolute_error: overlay
                .maximum_absolute_error
                .clone()
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
            iterations: overlay.iteration.clone(),
        });
        Ok(())
    }

    /// Projects electrical primitive work counters into catalog metric slots.
    pub fn set_electrical_flow_metrics(
        &mut self,
        metrics: crate::algorithms::ElectricalFlowMetrics,
    ) {
        self.metrics[0] = metrics.laplacian_assemblies.to_string();
        self.metrics[1] = metrics.grounded_dimension.to_string();
        self.metrics[2] = metrics.conjugate_gradient_iterations.to_string();
        self.metrics[3] = metrics.matrix_scalar_products.to_string();
        self.metrics[4] = metrics.edge_scans.to_string();
        self.metrics[5] = metrics.exact_elimination_pivots.to_string();
        self.metrics[6] = metrics.certificate_checks.to_string();
        self.metrics[7] = metrics.state_transitions.to_string();
    }

    /// Projects one bounded augmenting-electrical source boundary.
    ///
    /// Fractional central flow remains exclusively in the typed overlay. The
    /// generic edge/residual layer carries zero flow until exact directed
    /// extraction publishes an integral candidate.
    ///
    /// # Errors
    ///
    /// Rejects identity drift, noncanonical decimals/counts, inconsistent
    /// targets, invalid residual/barrier values, or malformed terminal flows.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_augmenting_electrical_boundary(
        &mut self,
        graph: &FlowNetwork,
        source: crate::model::NodeIndex,
        sink: crate::model::NodeIndex,
        flows: &[u64],
        overlay: FlowAugmentingElectricalOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_augmenting_electrical_overlay(graph, source, sink, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.node_trace_states = overlay
            .nodes
            .iter()
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                label: None,
                search_ordinal: None,
                remaining_divergence: None,
            })
            .collect();
        self.augmenting_electrical_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes the independently certified directed maximum-flow outcome.
    ///
    /// # Errors
    ///
    /// Rejects a missing/nonterminal overlay or incomplete integral edge flow.
    pub fn set_augmenting_electrical_outcome(
        &mut self,
        certificate: &MaxFlowCertificate,
    ) -> Result<(), FlowSceneError> {
        let overlay = self
            .augmenting_electrical_overlay
            .as_ref()
            .filter(|overlay| overlay.stage == FlowAugmentingElectricalStageV1::Optimal)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if overlay.edges.iter().any(|edge| edge.final_flow.is_none()) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        Ok(())
    }

    /// Projects augmenting-electrical work counters into catalog metric slots.
    pub fn set_augmenting_electrical_metrics(
        &mut self,
        metrics: crate::algorithms::AugmentingElectricalMetrics,
    ) {
        self.metrics[0] = metrics.enumerated_cuts.to_string();
        self.metrics[1] = metrics.electrical_solves.to_string();
        self.metrics[2] = metrics.elimination_pivots.to_string();
        self.metrics[3] = metrics.progress_steps.to_string();
        self.metrics[4] = metrics.fixing_steps.to_string();
        self.metrics[5] = metrics.boosts.to_string();
        self.metrics[6] = metrics.boost_vertices.to_string();
        self.metrics[7] = metrics.rounding_paths.to_string();
        self.metrics[8] = metrics.cleanup_augmentations.to_string();
        self.metrics[9] = metrics.extraction_cycles.to_string();
        self.metrics[10] = metrics.certificate_checks.to_string();
        self.metrics[11] = metrics.state_transitions.to_string();
    }

    /// Projects one bounded Mądry 2013 central-path boundary.
    ///
    /// Fractional matching flow remains in the typed overlay. Generic flow and
    /// residual state remain zero until b-matching recovery publishes an
    /// integral original-graph candidate.
    ///
    /// # Errors
    ///
    /// Rejects identity drift, noncanonical finite decimals/counts, an invalid
    /// target cut, malformed reduction sizes, or inconsistent terminal flows.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_interior_point_max_flow_boundary(
        &mut self,
        graph: &FlowNetwork,
        source: crate::model::NodeIndex,
        sink: crate::model::NodeIndex,
        flows: &[u64],
        overlay: FlowInteriorPointMaxFlowOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_interior_point_max_flow_overlay(graph, source, sink, flows, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.node_trace_states = overlay
            .nodes
            .iter()
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                label: None,
                search_ordinal: None,
                remaining_divergence: None,
            })
            .collect();
        self.interior_point_max_flow_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes the independently certified original maximum-flow outcome.
    ///
    /// # Errors
    ///
    /// Rejects a missing/nonterminal overlay or incomplete integral edge flow.
    pub fn set_interior_point_max_flow_outcome(
        &mut self,
        certificate: &MaxFlowCertificate,
    ) -> Result<(), FlowSceneError> {
        let overlay = self
            .interior_point_max_flow_overlay
            .as_ref()
            .filter(|overlay| overlay.stage == FlowInteriorPointMaxFlowStageV1::Optimal)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if overlay.edges.iter().any(|edge| edge.final_flow.is_none()) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_max_flow_outcome(certificate);
        Ok(())
    }

    /// Projects bounded path-following work counters into catalog metric slots.
    pub fn set_interior_point_max_flow_metrics(
        &mut self,
        metrics: crate::algorithms::InteriorPointMaxFlowMetrics,
    ) {
        self.metrics[0] = metrics.enumerated_cuts.to_string();
        self.metrics[1] = metrics.b_matching_nodes.to_string();
        self.metrics[2] = metrics.b_matching_edges.to_string();
        self.metrics[3] = metrics.working_nodes.to_string();
        self.metrics[4] = metrics.working_edges.to_string();
        self.metrics[5] = metrics.electrical_solves.to_string();
        self.metrics[6] = metrics.elimination_pivots.to_string();
        self.metrics[7] = metrics.progress_steps.to_string();
        self.metrics[8] = metrics.centering_steps.to_string();
        self.metrics[9] = metrics.recovery_arc_scans.to_string();
        self.metrics[10] = metrics.certificate_checks.to_string();
        self.metrics[11] = metrics.state_transitions.to_string();
    }

    /// Projects one bounded exact minimum-ratio-cycle boundary.
    ///
    /// Generic directed flows and residual arcs remain zero/empty because the
    /// selected vector is a signed undirected circulation, not a feasible
    /// directed maximum flow.
    ///
    /// # Errors
    ///
    /// Rejects source mapping, canonical forest, rational objective, signed
    /// circulation, or stable identity drift before mutating the scene.
    pub fn apply_minimum_ratio_cycle_boundary(
        &mut self,
        graph: &FlowNetwork,
        overlay: FlowMinimumRatioCycleOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_minimum_ratio_cycle_overlay(graph, &overlay)?;
        self.edge_states = graph
            .edges()
            .iter()
            .map(|edge| FlowEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                flow: "0".to_owned(),
            })
            .collect();
        self.residual_arcs.clear();
        self.node_trace_states = overlay
            .nodes
            .iter()
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                label: Some(node.component.clone()),
                search_ordinal: None,
                remaining_divergence: Some(node.candidate_balance.clone()),
            })
            .collect();
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.minimum_ratio_cycle_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes the exact ratio primitive outcome without claiming max flow.
    ///
    /// # Errors
    ///
    /// Rejects a missing or nonterminal checked overlay.
    pub fn set_minimum_ratio_cycle_outcome(&mut self) -> Result<(), FlowSceneError> {
        let overlay = self
            .minimum_ratio_cycle_overlay
            .as_ref()
            .filter(|overlay| overlay.stage == FlowMinimumRatioCycleStageV1::Complete)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let cycle = overlay
            .edges
            .iter()
            .filter(|edge| edge.selected_sign != "0")
            .map(|edge| FlowMinimumRatioCycleArcV1 {
                edge_id: edge.edge_id.clone(),
                sign: edge.selected_sign.clone(),
            })
            .collect();
        self.solve_status = FlowSolveStatusV1::PrimitiveComplete;
        self.outcome = Some(FlowOutcomeV1::MinimumRatioCycle {
            ratio: overlay.best_ratio.clone(),
            cycle,
            simple_cycles: overlay.simple_cycles.clone(),
            enumerated_vectors: overlay.enumerated_vectors.clone(),
        });
        Ok(())
    }

    /// Projects exact ratio-oracle work into catalog metric slots.
    pub fn set_minimum_ratio_cycle_metrics(
        &mut self,
        metrics: crate::algorithms::MinimumRatioCycleMetrics,
    ) {
        self.metrics[0] = metrics.forest_edge_scans.to_string();
        self.metrics[1] = metrics.fundamental_cycles.to_string();
        self.metrics[2] = metrics.enumerated_vectors.to_string();
        self.metrics[3] = metrics.simple_cycles.to_string();
        self.metrics[4] = metrics.ratio_comparisons.to_string();
        self.metrics[5] = metrics.best_updates.to_string();
        self.metrics[6] = metrics.dfs_expansions.to_string();
        self.metrics[7] = metrics.certificate_checks.to_string();
        self.metrics[8] = metrics.state_transitions.to_string();
    }

    /// Projects one bounded source-faithful MCF ratio-cycle boundary.
    ///
    /// Generic integral flow and residual collections stay zero/empty because
    /// this descriptor publishes a fractional progress primitive rather than a
    /// terminal directed flow.
    ///
    /// # Errors
    ///
    /// Rejects identity drift, malformed finite decimals, an inconsistent
    /// source potential, gradient/length map, signed cycle, or progress step.
    pub fn apply_minimum_ratio_cycle_mcf_boundary(
        &mut self,
        graph: &FlowNetwork,
        overlay: FlowMinimumRatioCycleMcfOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_minimum_ratio_cycle_mcf_overlay(graph, &overlay)?;
        self.edge_states = graph
            .edges()
            .iter()
            .map(|edge| FlowEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                flow: "0".to_owned(),
            })
            .collect();
        self.residual_arcs.clear();
        self.node_trace_states = overlay
            .nodes
            .iter()
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                label: Some(node.component.clone()),
                search_ordinal: None,
                remaining_divergence: Some(node.candidate_balance.clone()),
            })
            .collect();
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.minimum_ratio_cycle_mcf_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes one checked source progress primitive without claiming MCF.
    ///
    /// # Errors
    ///
    /// Rejects a missing or nonterminal typed overlay.
    pub fn set_minimum_ratio_cycle_mcf_outcome(&mut self) -> Result<(), FlowSceneError> {
        let overlay = self
            .minimum_ratio_cycle_mcf_overlay
            .as_ref()
            .filter(|overlay| overlay.stage == FlowMinimumRatioCycleMcfStageV1::Complete)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let cycle = overlay
            .edges
            .iter()
            .filter(|edge| edge.selected_sign != "0")
            .map(|edge| FlowMinimumRatioCycleArcV1 {
                edge_id: edge.edge_id.clone(),
                sign: edge.selected_sign.clone(),
            })
            .collect();
        self.solve_status = FlowSolveStatusV1::PrimitiveComplete;
        self.outcome = Some(FlowOutcomeV1::MinimumRatioCycleMcf {
            ratio: overlay.best_ratio.clone(),
            cycle,
            alpha: overlay.alpha.clone(),
            kappa: overlay.kappa.clone(),
            eta: overlay.eta.clone(),
            potential_decrease: overlay.potential_decrease.clone(),
            guaranteed_decrease: overlay.guaranteed_decrease.clone(),
            stationary: overlay.stationary,
        });
        Ok(())
    }

    /// Projects bounded MCF ratio-cycle work into catalog metric slots.
    pub fn set_minimum_ratio_cycle_mcf_metrics(
        &mut self,
        metrics: crate::algorithms::MinimumRatioCycleMcfMetrics,
    ) {
        self.metrics[0] = metrics.enumerated_assignments.to_string();
        self.metrics[1] = metrics.feasible_flows.to_string();
        self.metrics[2] = metrics.forest_edge_scans.to_string();
        self.metrics[3] = metrics.fundamental_cycles.to_string();
        self.metrics[4] = metrics.enumerated_vectors.to_string();
        self.metrics[5] = metrics.simple_cycles.to_string();
        self.metrics[6] = metrics.ratio_comparisons.to_string();
        self.metrics[7] = metrics.best_updates.to_string();
        self.metrics[8] = metrics.dfs_expansions.to_string();
        self.metrics[9] = metrics.source_steps.to_string();
        self.metrics[10] = metrics.certificate_checks.to_string();
        self.metrics[11] = metrics.state_transitions.to_string();
    }

    /// Projects one randomized almost-linear MCF source boundary.
    ///
    /// # Errors
    ///
    /// Rejects graph identity drift, malformed exact/floating scalars, an
    /// inconsistent isolation transform, or invalid integral residual state.
    pub fn apply_randomized_almost_linear_mcf_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowRandomizedAlmostLinearMcfOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_randomized_almost_linear_mcf_overlay(graph, flows, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.node_trace_states = overlay
            .nodes
            .iter()
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                label: Some(node.depth.clone()),
                search_ordinal: None,
                remaining_divergence: Some(node.component.clone()),
            })
            .collect();
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.randomized_almost_linear_mcf_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes the independently certified terminal MCF result.
    ///
    /// # Errors
    ///
    /// Rejects a missing or nonterminal typed overlay.
    pub fn set_randomized_almost_linear_mcf_outcome(
        &mut self,
        graph: &FlowNetwork,
        certificate: &MinCostFlowCertificate,
    ) -> Result<(), FlowSceneError> {
        self.randomized_almost_linear_mcf_overlay
            .as_ref()
            .filter(|overlay| {
                overlay.stage == FlowRandomizedAlmostLinearMcfStageV1::Optimal
                    && overlay.exact_recovery
            })
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        Ok(())
    }

    /// Projects bounded randomized MCF work into catalog metric slots.
    pub fn set_randomized_almost_linear_mcf_metrics(
        &mut self,
        metrics: crate::algorithms::RandomizedAlmostLinearMcfMetrics,
    ) {
        self.metrics[0] = metrics.enumerated_assignments.to_string();
        self.metrics[1] = metrics.feasible_flows.to_string();
        self.metrics[2] = metrics.isolation_attempts.to_string();
        self.metrics[3] = metrics.random_draws.to_string();
        self.metrics[4] = metrics.forest_pool_size.to_string();
        self.metrics[5] = metrics.ratio_queries.to_string();
        self.metrics[6] = metrics.source_steps.to_string();
        self.metrics[7] = metrics.detect_scans.to_string();
        self.metrics[8] = metrics.detected_coordinates.to_string();
        self.metrics[9] = metrics.rebuilds.to_string();
        self.metrics[10] = metrics.certificate_checks.to_string();
        self.metrics[11] = metrics.state_transitions.to_string();
        self.metrics[12] = metrics.oracle_vector_evaluations.to_string();
    }

    /// Projects one bounded source Flow Framework boundary.
    ///
    /// # Errors
    ///
    /// Rejects graph identity, exact rational, circulation, level, generic
    /// flow, or stopping-rule drift.
    pub fn apply_flow_framework_mcf_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowFrameworkMcfOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_flow_framework_mcf_overlay(graph, flows, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Running;
        self.outcome = None;
        self.flow_framework_mcf_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes a certificate only from the source final-point boundary.
    ///
    /// # Errors
    ///
    /// Rejects a missing/nonterminal overlay or any other stopping label.
    pub fn set_flow_framework_mcf_outcome(
        &mut self,
        graph: &FlowNetwork,
        certificate: &MinCostFlowCertificate,
    ) -> Result<(), FlowSceneError> {
        self.flow_framework_mcf_overlay
            .as_ref()
            .filter(|overlay| {
                overlay.stage == FlowFrameworkMcfStageV1::Optimal
                    && overlay.termination.as_deref() == Some("source-additive-half-gap")
            })
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        Ok(())
    }

    /// Projects bounded coordinator work into stable catalog metric slots.
    #[allow(
        clippy::too_many_arguments,
        reason = "the fixed metric-slot wire contract is clearer at its single call site than a one-use wrapper"
    )]
    pub fn set_flow_framework_mcf_metrics(
        &mut self,
        iteration: u64,
        cycle_queries: u64,
        shifts: u64,
        rebuilds: u64,
        detected: u64,
        state_transitions: u64,
        primary_work: u128,
        intermediate_edge_inspections: u64,
        terminal_edge_inspections: u64,
        detection_edge_scans: u64,
    ) {
        self.metrics[0] = iteration.to_string();
        self.metrics[1] = cycle_queries.to_string();
        self.metrics[2] = shifts.to_string();
        self.metrics[3] = rebuilds.to_string();
        self.metrics[4] = detected.to_string();
        self.metrics[5] = state_transitions.to_string();
        self.metrics[6] = primary_work.to_string();
        self.metrics[7] = intermediate_edge_inspections.to_string();
        self.metrics[8] = terminal_edge_inspections.to_string();
        self.metrics[9] = detection_edge_scans.to_string();
    }

    /// Projects one bounded weighted augmenting-path boundary.
    ///
    /// The typed overlay is authoritative for prefix capacities and their
    /// residual directions; the generic edge state carries the same flow in
    /// the immutable full-capacity graph.
    ///
    /// # Errors
    ///
    /// Rejects canonical identity drift, malformed exact decimals, an invalid
    /// hierarchy/order/weight projection, or inconsistent prefix residuals.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_weighted_augmenting_paths_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowWeightedAugmentingPathsOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_weighted_augmenting_paths_overlay(graph, flows, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.node_trace_states = overlay
            .nodes
            .iter()
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                label: Some(node.label.clone()),
                search_ordinal: None,
                remaining_divergence: Some("0".to_owned()),
            })
            .collect();
        self.weighted_augmenting_paths_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes the independently checked weighted augmenting-path outcome.
    ///
    /// # Errors
    ///
    /// Rejects a missing or nonterminal source overlay.
    pub fn set_weighted_augmenting_paths_outcome(
        &mut self,
        certificate: &MaxFlowCertificate,
    ) -> Result<(), FlowSceneError> {
        let overlay = self
            .weighted_augmenting_paths_overlay
            .as_ref()
            .filter(|overlay| overlay.stage == FlowWeightedAugmentingPathsStageV1::Optimal)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if overlay
            .edges
            .iter()
            .zip(&self.edge_states)
            .any(|(edge, state)| edge.edge_id != state.edge_id || edge.flow != state.flow)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        self.set_max_flow_outcome(certificate);
        Ok(())
    }

    /// Projects bounded weighted augmenting-path work into metric slots.
    pub fn set_weighted_augmenting_paths_metrics(
        &mut self,
        metrics: crate::algorithms::WeightedAugmentingPathsMetrics,
    ) {
        self.metrics[0] = metrics.capacity_phases.to_string();
        self.metrics[1] = metrics.hierarchy_builds.to_string();
        self.metrics[2] = metrics.hierarchy_cuts.to_string();
        self.metrics[3] = metrics.weighted_rounds.to_string();
        self.metrics[4] = metrics.relabel_sweeps.to_string();
        self.metrics[5] = metrics.relabel_jumps.to_string();
        self.metrics[6] = metrics.admissible_updates.to_string();
        self.metrics[7] = metrics.augmentations.to_string();
        self.metrics[8] = metrics.augmented_units.to_string();
        self.metrics[9] = metrics.residual_cut_checks.to_string();
        self.metrics[10] = metrics.certificate_checks.to_string();
        self.metrics[11] = metrics.state_transitions.to_string();
        self.metrics[12] = metrics.relabel_arc_inspections.to_string();
    }

    /// Projects one bounded weighted push-relabel shortcut boundary.
    ///
    /// # Errors
    ///
    /// Rejects malformed identities, residual arithmetic, source weights,
    /// active paths, or a mismatch with the generic original-edge projection.
    pub fn apply_weighted_push_relabel_shortcut_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowWeightedPushRelabelShortcutOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_weighted_push_relabel_shortcut_overlay(graph, flows, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.node_trace_states = overlay
            .nodes
            .iter()
            .filter(|node| node.original)
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                label: Some(node.label.clone()),
                search_ordinal: node.order.parse::<u32>().ok(),
                remaining_divergence: Some("0".to_owned()),
            })
            .collect();
        self.weighted_push_relabel_shortcut_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes the independently checked exact shortcut-kernel outcome.
    ///
    /// # Errors
    ///
    /// Rejects a missing/nonterminal overlay or final original-flow drift.
    pub fn set_weighted_push_relabel_shortcut_outcome(
        &mut self,
        certificate: &MaxFlowCertificate,
    ) -> Result<(), FlowSceneError> {
        let overlay = self
            .weighted_push_relabel_shortcut_overlay
            .as_ref()
            .filter(|overlay| overlay.stage == FlowWeightedPushRelabelShortcutStageV1::Optimal)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let original_edges = overlay
            .edges
            .iter()
            .filter(|edge| edge.kind == "original")
            .collect::<Vec<_>>();
        if original_edges.len() != self.edge_states.len()
            || original_edges
                .iter()
                .zip(&self.edge_states)
                .any(|(edge, state)| edge.edge_id != state.edge_id || edge.flow != state.flow)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        self.set_max_flow_outcome(certificate);
        Ok(())
    }

    /// Projects weighted push-relabel shortcut work into metric slots.
    pub fn set_weighted_push_relabel_shortcut_metrics(
        &mut self,
        metrics: crate::algorithms::WeightedPushRelabelShortcutMetrics,
    ) {
        self.metrics[0] = metrics.hierarchy_builds.to_string();
        self.metrics[1] = metrics.shortcut_stars.to_string();
        self.metrics[2] = metrics.shortcut_edges.to_string();
        self.metrics[3] = metrics.primitive_arc_inspections.to_string();
        self.metrics[4] = metrics.relabel_sweeps.to_string();
        self.metrics[5] = metrics.admissible_updates.to_string();
        self.metrics[6] = metrics.augmentations.to_string();
        self.metrics[7] = metrics.shortcut_traversals.to_string();
        self.metrics[8] = metrics.routed_units.to_string();
        self.metrics[9] = metrics.distance_arc_scans.to_string();
        self.metrics[10] = metrics.sparse_cut_checks.to_string();
        self.metrics[11] = metrics.residual_rounds.to_string();
        self.metrics[12] = metrics.completion_relabel_steps.to_string();
        self.metrics[13] = metrics.completion_augmentations.to_string();
        self.metrics[14] = metrics.certificate_checks.to_string();
        self.metrics[15] = metrics.state_transitions.to_string();
    }

    /// Projects one bounded randomized almost-linear boundary.
    ///
    /// Fractional augmented-circulation state stays in the typed overlay.
    /// Generic residual state is built only from the supplied integral original
    /// flow, which is zero before source rounding and certified at `optimal`.
    ///
    /// # Errors
    ///
    /// Rejects identity drift, malformed finite scalars/probability, invalid
    /// return reduction, or inconsistent terminal flows before mutation.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_randomized_almost_linear_boundary(
        &mut self,
        graph: &FlowNetwork,
        source: crate::model::NodeIndex,
        sink: crate::model::NodeIndex,
        flows: &[u64],
        overlay: FlowRandomizedAlmostLinearOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_randomized_almost_linear_overlay(graph, source, sink, flows, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.node_trace_states = overlay
            .nodes
            .iter()
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                label: Some(node.tree_component.clone()),
                search_ordinal: None,
                // The augmented state is a circulation: the artificial-edge
                // flow is not node excess and may become fractional after an
                // IPM step. Keep it solely in the typed overlay.
                remaining_divergence: Some("0".to_owned()),
            })
            .collect();
        self.randomized_almost_linear_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes the independently checked maximum-flow outcome.
    ///
    /// # Errors
    ///
    /// Rejects a missing/nonterminal overlay or incomplete repaired flow.
    pub fn set_randomized_almost_linear_outcome(
        &mut self,
        certificate: &MaxFlowCertificate,
    ) -> Result<(), FlowSceneError> {
        let overlay = self
            .randomized_almost_linear_overlay
            .as_ref()
            .filter(|overlay| overlay.stage == FlowRandomizedAlmostLinearStageV1::Optimal)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if overlay.edges.iter().any(|edge| edge.final_flow.is_none())
            || overlay.final_return_flow.as_deref() != Some(overlay.target_value.as_str())
            || overlay.final_artificial_flow.as_deref() != Some("0")
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        self.set_max_flow_outcome(certificate);
        Ok(())
    }

    /// Projects bounded randomized framework counters into metric slots.
    pub fn set_randomized_almost_linear_metrics(
        &mut self,
        metrics: crate::algorithms::RandomizedAlmostLinearMaxFlowMetrics,
    ) {
        self.metrics[0] = metrics.enumerated_cuts.to_string();
        self.metrics[1] = metrics.forest_subsets.to_string();
        self.metrics[2] = metrics.forest_pool_size.to_string();
        self.metrics[3] = metrics.sampled_forests.to_string();
        self.metrics[4] = metrics.fundamental_cycles.to_string();
        self.metrics[5] = metrics.successful_queries.to_string();
        self.metrics[6] = metrics.sampling_failures.to_string();
        self.metrics[7] = metrics.potential_steps.to_string();
        self.metrics[8] = metrics.detected_coordinates.to_string();
        self.metrics[9] = metrics.rebuilds.to_string();
        self.metrics[10] = metrics.enumerated_assignments.to_string();
        self.metrics[11] = metrics.feasible_flows.to_string();
        self.metrics[12] = metrics.isolation_attempts.to_string();
        self.metrics[13] = metrics.rounding_operations.to_string();
        self.metrics[14] = metrics.certificate_checks.to_string();
        self.metrics[15] = metrics.state_transitions.to_string();
    }

    /// Projects one bounded deterministic shifted-tree-chain boundary.
    ///
    /// # Errors
    ///
    /// Rejects identity drift, malformed masks/branch state, invalid
    /// core-spanner metadata, or inconsistent terminal repair before mutation.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_deterministic_almost_linear_boundary(
        &mut self,
        graph: &FlowNetwork,
        source: crate::model::NodeIndex,
        sink: crate::model::NodeIndex,
        flows: &[u64],
        overlay: FlowDeterministicAlmostLinearOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_deterministic_almost_linear_overlay(graph, source, sink, flows, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.node_trace_states = overlay
            .nodes
            .iter()
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                label: Some(node.forest_component.clone()),
                search_ordinal: None,
                remaining_divergence: Some("0".to_owned()),
            })
            .collect();
        self.deterministic_almost_linear_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes the independently checked deterministic maximum-flow outcome.
    ///
    /// # Errors
    ///
    /// Rejects a missing/nonterminal overlay or incomplete repair.
    pub fn set_deterministic_almost_linear_outcome(
        &mut self,
        certificate: &MaxFlowCertificate,
    ) -> Result<(), FlowSceneError> {
        let overlay = self
            .deterministic_almost_linear_overlay
            .as_ref()
            .filter(|overlay| overlay.stage == FlowDeterministicAlmostLinearStageV1::Optimal)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if overlay.edges.iter().any(|edge| edge.final_flow.is_none())
            || overlay.final_return_flow.as_deref() != Some(overlay.target_value.as_str())
            || overlay.final_artificial_flow.as_deref() != Some("0")
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        self.set_max_flow_outcome(certificate);
        Ok(())
    }

    /// Projects deterministic tree-chain work into dedicated metric slots.
    pub fn set_deterministic_almost_linear_metrics(
        &mut self,
        metrics: crate::algorithms::DeterministicAlmostLinearMaxFlowMetrics,
    ) {
        self.metrics[0] = metrics.enumerated_cuts.to_string();
        self.metrics[1] = metrics.forest_subsets.to_string();
        self.metrics[2] = metrics.forest_pool_size.to_string();
        self.metrics[3] = metrics.branch_records.to_string();
        self.metrics[4] = metrics.core_builds.to_string();
        self.metrics[5] = metrics.spanner_embeddings.to_string();
        self.metrics[6] = metrics.fundamental_cycles.to_string();
        self.metrics[7] = metrics.successful_queries.to_string();
        self.metrics[8] = metrics.query_failures.to_string();
        self.metrics[9] = metrics.branch_shifts.to_string();
        self.metrics[10] = metrics.branch_wraps.to_string();
        self.metrics[11] = metrics.deeper_rebuilds.to_string();
        self.metrics[12] = metrics.potential_steps.to_string();
        self.metrics[13] = metrics.detected_coordinates.to_string();
        self.metrics[14] = metrics.scheduled_rebuilds.to_string();
        self.metrics[15] = metrics.enumerated_assignments.to_string();
    }

    /// Projects one bounded electrical-flow IPM MCF boundary.
    ///
    /// # Errors
    ///
    /// Rejects noncanonical finite scalars, identity drift, malformed
    /// isolation/face data, or inconsistent rounded flow before mutation.
    pub fn apply_electrical_ipm_mcf_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowElectricalIpmMcfOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_electrical_ipm_mcf_overlay(graph, flows, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.node_trace_states = overlay
            .nodes
            .iter()
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                label: Some(if node.anchored { "1" } else { "0" }.to_owned()),
                search_ordinal: None,
                // Fractional residuals remain in the typed overlay; this
                // compatibility field is integral by the generic scene schema.
                remaining_divergence: Some("0".to_owned()),
            })
            .collect();
        self.electrical_ipm_mcf_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes the independently certified original minimum-cost flow.
    ///
    /// # Errors
    ///
    /// Rejects a missing/nonterminal electrical-IPM overlay.
    pub fn set_electrical_ipm_mcf_outcome(
        &mut self,
        graph: &FlowNetwork,
        certificate: &MinCostFlowCertificate,
    ) -> Result<(), FlowSceneError> {
        self.electrical_ipm_mcf_overlay
            .as_ref()
            .filter(|overlay| overlay.stage == FlowElectricalIpmMcfStageV1::Optimal)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        Ok(())
    }

    /// Projects bounded isolation and electrical-IPM work into metric slots.
    pub fn set_electrical_ipm_mcf_metrics(
        &mut self,
        metrics: crate::algorithms::ElectricalIpmMcfMetrics,
    ) {
        self.metrics[0] = metrics.enumerated_assignments.to_string();
        self.metrics[1] = metrics.feasible_flows.to_string();
        self.metrics[2] = metrics.isolation_attempts.to_string();
        self.metrics[3] = metrics.random_draws.to_string();
        self.metrics[4] = metrics.fixed_coordinates.to_string();
        self.metrics[5] = metrics.laplacian_assemblies.to_string();
        self.metrics[6] = metrics.newton_solves.to_string();
        self.metrics[7] = metrics.elimination_pivots.to_string();
        self.metrics[8] = metrics.centering_steps.to_string();
        self.metrics[9] = metrics.barrier_reductions.to_string();
        self.metrics[10] = metrics.line_search_reductions.to_string();
        self.metrics[11] = metrics.rounding_operations.to_string();
        self.metrics[12] = metrics.certificate_checks.to_string();
        self.metrics[13] = metrics.state_transitions.to_string();
    }

    /// Projects one exact integer primal-dual MCF boundary.
    ///
    /// # Errors
    ///
    /// Rejects malformed arbitrary-precision decimals, auxiliary identity
    /// drift, invalid minor/tree relations, or inconsistent original flows.
    pub fn apply_primal_dual_ipm_mcf_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowPrimalDualIpmMcfOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_primal_dual_ipm_mcf_overlay(graph, flows, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.node_trace_states = overlay
            .nodes
            .iter()
            .filter_map(|node| {
                node.original_node_id
                    .as_ref()
                    .map(|node_id| FlowNodeTraceStateV1 {
                        node_id: node_id.clone(),
                        label: Some(node.component.clone()),
                        search_ordinal: None,
                        remaining_divergence: Some("0".to_owned()),
                    })
            })
            .collect();
        self.primal_dual_ipm_mcf_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes the independently verified original MCF optimum.
    ///
    /// # Errors
    ///
    /// Rejects a missing or nonterminal integer-IPM overlay.
    pub fn set_primal_dual_ipm_mcf_outcome(
        &mut self,
        graph: &FlowNetwork,
        certificate: &MinCostFlowCertificate,
    ) -> Result<(), FlowSceneError> {
        self.primal_dual_ipm_mcf_overlay
            .as_ref()
            .filter(|overlay| overlay.stage == FlowPrimalDualIpmMcfStageV1::Optimal)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        Ok(())
    }

    /// Projects bounded integer-IPM work into the fixed metric vector.
    pub fn set_primal_dual_ipm_mcf_metrics(
        &mut self,
        metrics: crate::algorithms::PrimalDualIpmMetrics,
    ) {
        self.metrics[0] = metrics.outer_iterations.to_string();
        self.metrics[1] = metrics.centering_steps.to_string();
        self.metrics[2] = metrics.forest_subsets.to_string();
        self.metrics[3] = metrics.random_draws.to_string();
        self.metrics[4] = metrics.sampled_cycles.to_string();
        self.metrics[5] = metrics.cycle_updates.to_string();
        self.metrics[6] = metrics.deleted_arcs.to_string();
        self.metrics[7] = metrics.contracted_arcs.to_string();
        self.metrics[8] = metrics.crossover_shifts.to_string();
        self.metrics[9] = metrics.recovery_augmentations.to_string();
        self.metrics[10] = metrics.certificate_checks.to_string();
        self.metrics[11] = metrics.state_transitions.to_string();
    }

    /// Projects one exact natural dual-network-simplex boundary.
    ///
    /// # Errors
    ///
    /// Rejects mismatched stable identities, tree membership, cut membership,
    /// signed basic-flow values, or reduced costs.
    pub fn apply_dual_network_simplex_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowDualNetworkSimplexOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_dual_network_simplex_overlay(graph, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.dual_network_simplex_overlay = Some(overlay);
        Ok(())
    }

    /// Projects natural dual-network-simplex work counters into shared slots.
    ///
    /// # Errors
    ///
    /// Returns [`FlowSceneError::CountOverflow`] when the bounded kernel counters
    /// cannot be combined into the declared primary-work metric.
    pub fn set_dual_network_simplex_metrics(
        &mut self,
        metrics: crate::algorithms::DualNetworkSimplexMetrics,
    ) -> Result<(), FlowSceneError> {
        self.metrics[0] = metrics.pivots.to_string();
        self.metrics[1] = metrics.leaving_searches.to_string();
        self.metrics[2] = metrics.entering_arc_scans.to_string();
        self.metrics[3] = metrics
            .shortest_path_arc_scans
            .checked_add(metrics.entering_arc_scans)
            .ok_or(FlowSceneError::CountOverflow)?
            .to_string();
        self.metrics[4] = metrics.zero_price_pivots.to_string();
        self.metrics[5] = metrics.tree_rebuilds.to_string();
        self.metrics[6] = metrics.price_updates.to_string();
        Ok(())
    }

    /// Projects one exact polynomial dual scaling-simplex boundary.
    ///
    /// # Errors
    ///
    /// Rejects malformed tree support, dyadic values, bad-subtree membership,
    /// active paths, or dual pivot selections.
    pub fn apply_polynomial_dual_simplex_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowPolynomialDualSimplexOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_polynomial_dual_simplex_overlay(graph, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.polynomial_dual_simplex_overlay = Some(overlay);
        Ok(())
    }

    /// Projects polynomial dual scaling-simplex counters into shared slots.
    ///
    /// # Errors
    ///
    /// Returns [`FlowSceneError::CountOverflow`] when the bounded kernel counters
    /// cannot be combined into the declared primary-work metric.
    pub fn set_polynomial_dual_simplex_metrics(
        &mut self,
        metrics: crate::algorithms::PolynomialDualSimplexMetrics,
    ) -> Result<(), FlowSceneError> {
        self.metrics[0] = metrics.scaling_phases.to_string();
        self.metrics[1] = metrics.augmentations.to_string();
        self.metrics[2] = metrics.pivots.to_string();
        self.metrics[3] = metrics.active_searches.to_string();
        self.metrics[4] = metrics.bad_arc_searches.to_string();
        self.metrics[5] = metrics
            .initial_arc_scans
            .checked_add(metrics.augmentation_arc_scans)
            .and_then(|value| value.checked_add(metrics.entering_arc_scans))
            .ok_or(FlowSceneError::CountOverflow)?
            .to_string();
        self.metrics[6] = metrics.initial_arc_scans.to_string();
        self.metrics[7] = metrics.augmentation_arc_scans.to_string();
        self.metrics[8] = metrics.entering_arc_scans.to_string();
        self.metrics[9] = metrics.tree_rebuilds.to_string();
        self.metrics[10] = metrics.zero_price_pivots.to_string();
        self.metrics[11] = metrics.price_updates.to_string();
        Ok(())
    }

    /// Projects one exact scaling-premultiplier primal-simplex boundary.
    ///
    /// # Errors
    ///
    /// Rejects malformed extended identities, basis states, exact rationals,
    /// selected cycles, or reduced costs.
    pub fn apply_polynomial_primal_simplex_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowPolynomialPrimalSimplexOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_polynomial_primal_simplex_overlay(graph, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.polynomial_primal_simplex_overlay = Some(overlay);
        Ok(())
    }

    /// Projects exact scaling-premultiplier work counters into shared slots.
    ///
    /// # Errors
    ///
    /// Returns [`FlowSceneError::CountOverflow`] when the bounded kernel counters
    /// cannot be combined into the declared primary-work metric.
    pub fn set_polynomial_primal_simplex_metrics(
        &mut self,
        metrics: crate::algorithms::PolynomialPrimalSimplexMetrics,
    ) -> Result<(), FlowSceneError> {
        self.metrics[0] = metrics.scaling_phases.to_string();
        self.metrics[1] = metrics.pivots.to_string();
        self.metrics[2] = metrics.admissible_searches.to_string();
        self.metrics[3] = metrics
            .admissible_arc_scans
            .checked_add(metrics.optimality_arc_scans)
            .and_then(|total| total.checked_add(metrics.cycle_arc_scans))
            .ok_or(FlowSceneError::CountOverflow)?
            .to_string();
        self.metrics[4] = metrics.premultiplier_updates.to_string();
        self.metrics[5] = metrics.updated_nodes.to_string();
        self.metrics[6] = metrics.reawakened_nodes.to_string();
        self.metrics[7] = metrics.basis_exchanges.to_string();
        self.metrics[8] = metrics.bound_flips.to_string();
        self.metrics[9] = metrics.cycle_arc_scans.to_string();
        self.metrics[10] = metrics.optimality_searches.to_string();
        self.metrics[11] = metrics.optimality_arc_scans.to_string();
        self.metrics[12] = metrics.tree_rebuilds.to_string();
        Ok(())
    }

    /// Projects one exact double-scaling transportation boundary.
    ///
    /// # Errors
    ///
    /// Rejects mismatched original flow, transformed node, edge, or arc identities.
    pub fn apply_double_scaling_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowDoubleScalingOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_double_scaling_overlay(graph, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.double_scaling_overlay = Some(overlay);
        Ok(())
    }

    /// Projects exact double-scaling counters into shared metric slots.
    #[allow(clippy::too_many_arguments)]
    pub fn set_double_scaling_metrics(
        &mut self,
        cost_phases: u64,
        capacity_phases: u64,
        path_searches: u64,
        advances: u64,
        relabels: u64,
        retreats: u64,
        augmentations: u64,
        transformed_arc_resets: u128,
        transformed_arc_scans: u128,
    ) {
        self.metrics[0] = cost_phases.to_string();
        self.metrics[1] = capacity_phases.to_string();
        self.metrics[2] = transformed_arc_scans.to_string();
        self.metrics[3] = augmentations.to_string();
        self.metrics[4] = path_searches.to_string();
        self.metrics[5] = relabels.to_string();
        self.metrics[6] = advances.to_string();
        self.metrics[7] = retreats.to_string();
        self.metrics[8] = transformed_arc_resets.to_string();
    }

    /// Projects one exact segment-expanded convex-cost boundary.
    ///
    /// # Errors
    ///
    /// Rejects segment partitions, prefix occupancy, aggregate flows, node
    /// annotations, or active marginal arcs that disagree with the graph.
    pub fn apply_convex_cost_boundary(
        &mut self,
        graph: &FlowNetwork,
        boundary: FlowConvexCostBoundary<'_>,
    ) -> Result<(), FlowSceneError> {
        validate_convex_cost_overlay(graph, boundary.flows, &boundary.overlay)?;
        if boundary.node_labels.len() != graph.nodes().len()
            || (!boundary.remaining_divergence.is_empty()
                && boundary.remaining_divergence.len() != graph.nodes().len())
            || boundary
                .search_order
                .iter()
                .any(|node| graph.node_index(node).is_none())
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        self.apply_fast_snapshot(graph, boundary.flows)?;
        let ordinals = boundary
            .search_order
            .iter()
            .enumerate()
            .map(|(index, node)| (node, index))
            .collect::<BTreeMap<_, _>>();
        self.node_trace_states = graph
            .nodes()
            .iter()
            .enumerate()
            .map(|(index, node)| FlowNodeTraceStateV1 {
                node_id: node.id().as_str().to_owned(),
                label: boundary.node_labels[index].map(|value| value.to_string()),
                search_ordinal: ordinals
                    .get(node.id())
                    .and_then(|value| u32::try_from(*value).ok()),
                remaining_divergence: boundary
                    .remaining_divergence
                    .get(index)
                    .map(ToString::to_string),
            })
            .collect();
        self.event_id = boundary.event_id.to_string();
        self.event_count = boundary.event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.convex_cost_overlay = Some(boundary.overlay);
        Ok(())
    }

    /// Publishes the independently certified native convex optimum.
    pub fn set_convex_cost_outcome(
        &mut self,
        graph: &FlowNetwork,
        certificate: &ConvexCostCertificate,
    ) {
        self.set_min_cost_flow_outcome(
            graph,
            &MinCostFlowCertificate {
                total_cost: certificate.total_cost,
                potentials: certificate.potentials.clone(),
            },
        );
    }

    /// Projects exact expanded-oracle counters into the shared metric slots.
    pub fn set_convex_cost_metrics(
        &mut self,
        mean_cycle_searches: u64,
        dynamic_programming_rounds: u64,
        residual_arc_scans: u128,
        canceled_cycles: u64,
    ) {
        self.metrics[0] = mean_cycle_searches.to_string();
        self.metrics[1] = dynamic_programming_rounds.to_string();
        self.metrics[2] = residual_arc_scans.to_string();
        self.metrics[3] = canceled_cycles.to_string();
    }

    /// Projects native marginal Δ-scaling counters into shared metric slots.
    pub fn set_convex_cost_scaling_metrics(
        &mut self,
        metrics: crate::algorithms::ConvexCostScalingMetrics,
    ) {
        self.metrics[0] = metrics.scaling_phases.to_string();
        self.metrics[1] = metrics.dijkstra_runs.to_string();
        self.metrics[2] = metrics.marginal_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.potential_updates.to_string();
        self.metrics[5] = metrics.phase_saturations.to_string();
        self.metrics[6] = metrics.breakpoint_crossings.to_string();
        self.metrics[7] = metrics.settled_nodes.to_string();
    }

    /// Projects one exact compact convex network-simplex boundary.
    ///
    /// # Errors
    ///
    /// Rejects mismatched segment occupancy, extended tree identities,
    /// potentials, compact states, or a discontinuous fundamental cycle.
    pub fn apply_convex_network_simplex_boundary(
        &mut self,
        graph: &FlowNetwork,
        boundary: FlowConvexCostBoundary<'_>,
        overlay: FlowConvexNetworkSimplexOverlayV1,
    ) -> Result<(), FlowSceneError> {
        validate_convex_network_simplex_overlay(
            graph,
            boundary.flows,
            &boundary.overlay,
            &overlay,
        )?;
        self.apply_convex_cost_boundary(graph, boundary)?;
        self.convex_network_simplex_overlay = Some(overlay);
        Ok(())
    }

    /// Projects exact combined-pivot work counters into shared metric slots.
    pub fn set_convex_network_simplex_metrics(
        &mut self,
        metrics: crate::algorithms::ConvexNetworkSimplexMetrics,
    ) {
        self.metrics[0] = metrics.pricing_searches.to_string();
        self.metrics[1] = metrics.combined_pivots.to_string();
        self.metrics[2] = metrics.pricing_arc_scans.to_string();
        self.metrics[3] = metrics.breakpoint_crossings.to_string();
        self.metrics[4] = metrics.basis_exchanges.to_string();
        self.metrics[5] = metrics.bound_flips.to_string();
        self.metrics[6] = metrics.multi_crossing_pivots.to_string();
        self.metrics[7] = metrics.cycle_arc_scans.to_string();
        self.metrics[8] = metrics.tree_rebuilds.to_string();
        self.metrics[9] = metrics.degenerate_crossings.to_string();
        self.metrics[10] = metrics.nondegenerate_crossings.to_string();
    }

    /// Projects one exact prediction-assisted epsilon-relaxation boundary.
    ///
    /// # Errors
    ///
    /// Rejects noncanonical prediction, scale, node, edge, active residual,
    /// or pseudoflow state that disagrees with the original graph.
    pub fn apply_prediction_assisted_epsilon_boundary(
        &mut self,
        graph: &FlowNetwork,
        required_divergence: &[i128],
        flows: &[u64],
        overlay: FlowPredictionAssistedEpsilonOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_prediction_assisted_epsilon_overlay(graph, required_divergence, flows, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.node_trace_states = overlay
            .nodes
            .iter()
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                label: Some(node.price.clone()),
                search_ordinal: node.active.then_some(0),
                remaining_divergence: Some(node.surplus.clone()),
            })
            .collect();
        self.prediction_assisted_epsilon_overlay = Some(overlay);
        Ok(())
    }

    /// Projects prediction-assisted epsilon-relaxation work counters into
    /// the shared metric slots.
    pub fn set_prediction_assisted_epsilon_metrics(
        &mut self,
        metrics: crate::algorithms::PredictionAssistedEpsilonMetrics,
    ) {
        self.metrics[0] = metrics.attempts.to_string();
        self.metrics[1] = metrics.aborted_attempts.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.pushes.to_string();
        self.metrics[4] = metrics.up_iterations.to_string();
        self.metrics[5] = metrics.price_rises.to_string();
        self.metrics[6] = metrics.scaling_phases.to_string();
        self.metrics[7] = metrics.maximum_exponent_attempted.to_string();
        self.metrics[8] = metrics.clipped_predictions.to_string();
        self.metrics[9] = metrics.pushed_flow_units.to_string();
        self.metrics[10] = metrics.saturating_pushes.to_string();
        self.metrics[11] = metrics.nonsaturating_pushes.to_string();
    }

    /// Projects one exact Tardos network-matrix variable-fixing boundary.
    ///
    /// # Errors
    ///
    /// Rejects node labels, residual directions, epsilon, threshold, or fixed
    /// variables that cannot be recomputed from the graph and flow vector.
    pub fn apply_tardos_framework_boundary(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        overlay: FlowTardosFrameworkOverlayV1,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        validate_tardos_framework_overlay(graph, flows, &overlay)?;
        self.apply_fast_snapshot(graph, flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.solve_status = FlowSolveStatusV1::Ready;
        self.outcome = None;
        self.node_trace_states = overlay
            .nodes
            .iter()
            .map(|node| FlowNodeTraceStateV1 {
                node_id: node.node_id.clone(),
                label: Some(node.potential.clone()),
                search_ordinal: None,
                remaining_divergence: None,
            })
            .collect();
        self.tardos_framework_overlay = Some(overlay);
        Ok(())
    }

    /// Publishes a checked primitive outcome without claiming flow optimality.
    ///
    /// # Errors
    ///
    /// Rejects a nonterminal or missing Tardos overlay.
    pub fn set_tardos_framework_outcome(&mut self) -> Result<(), FlowSceneError> {
        let overlay = self
            .tardos_framework_overlay
            .as_ref()
            .filter(|overlay| overlay.stage == FlowTardosFrameworkStageV1::Complete)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        self.solve_status = FlowSolveStatusV1::PrimitiveComplete;
        self.outcome = Some(FlowOutcomeV1::TardosFramework {
            epsilon: overlay.epsilon.clone(),
            threshold: overlay.threshold.clone(),
            determinant_bound: overlay.determinant_bound.clone(),
            fixed_variables: overlay.fixed_variables.clone(),
        });
        Ok(())
    }

    /// Projects Tardos primitive work counters into shared metric slots.
    pub fn set_tardos_framework_metrics(
        &mut self,
        metrics: crate::algorithms::TardosFrameworkMetrics,
    ) {
        self.metrics[0] = metrics.feasibility_constructions.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.fixed_variables.to_string();
        self.metrics[15] = metrics.state_transitions.to_string();
    }

    /// Applies a certified potential + Dijkstra SSP result to a ready scene.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_potential_dijkstra_ssp_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MinCostFlowCertificate,
        metrics: FlowPotentialDijkstraMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.dijkstra_runs.to_string();
        self.metrics[7] = metrics.potential_updates.to_string();
        self.metrics[15] = metrics.settled_nodes.to_string();
        Ok(())
    }

    /// Applies a certified restricted-primal Dinitz result to a ready scene.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_blocking_primal_dual_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MinCostFlowCertificate,
        metrics: FlowBlockingPrimalDualMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        self.metrics[0] = metrics.admissible_bfs_runs.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.slack_searches.to_string();
        self.metrics[6] = metrics.blocking_flow_phases.to_string();
        self.metrics[7] = metrics.potential_updates.to_string();
        self.metrics[15] = metrics.settled_nodes.to_string();
        Ok(())
    }

    /// Applies a certified finite-capacity scaling result to a ready scene.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_capacity_scaling_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MinCostFlowCertificate,
        metrics: FlowCapacityScalingMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.dijkstra_runs.to_string();
        self.metrics[5] = metrics.scaling_phases.to_string();
        self.metrics[7] = metrics.potential_updates.to_string();
        self.metrics[11] = metrics.phase_saturations.to_string();
        self.metrics[12] = metrics.phase_saturations.to_string();
        self.metrics[15] = metrics.settled_nodes.to_string();
        Ok(())
    }

    /// Applies a certified generic cost-scaling push--relabel result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_cost_scaling_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MinCostFlowCertificate,
        metrics: FlowCostScalingMetrics,
        fixed_edges: &[EdgeId],
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        let fixed = fixed_edges
            .iter()
            .map(EdgeId::as_str)
            .collect::<BTreeSet<_>>();
        for arc in &mut self.residual_arcs {
            arc.fixed = fixed.contains(arc.edge_id.as_str());
        }
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        if self.algorithm.id == "arc-fixing" {
            self.metrics[0] = metrics.arc_fixing_passes.to_string();
            self.metrics[1] = metrics.arcs_unfixed.to_string();
            self.metrics[2] = metrics.residual_arc_scans.to_string();
            self.metrics[3] = metrics.arcs_fixed.to_string();
            self.metrics[4] = metrics.fix_ins.to_string();
            self.metrics[5] = metrics.refine_phases.to_string();
            self.metrics[6] = metrics.arc_fixing_recoveries.to_string();
            self.metrics[7] = metrics.relabels.to_string();
            self.metrics[8] = metrics.fixed_arc_skips.to_string();
            self.metrics[9] = metrics.current_arc_advances.to_string();
            self.metrics[10] = metrics.initial_saturations.to_string();
            self.metrics[11] = metrics.pushes.to_string();
            self.metrics[12] = metrics.saturating_pushes.to_string();
            self.metrics[13] = metrics.nonsaturating_pushes.to_string();
            self.metrics[14] = metrics.discharges.to_string();
            self.metrics[15] = metrics.active_vertex_selections.to_string();
            return Ok(());
        }
        if self.algorithm.id == "price-refinement" {
            self.metrics[1] = metrics.price_refinement_rounds.to_string();
            self.metrics[2] = metrics.residual_arc_scans.to_string();
            self.metrics[3] = metrics.price_refinement_successes.to_string();
            self.metrics[4] = metrics.price_refinement_attempts.to_string();
            self.metrics[5] = metrics.refine_phases.to_string();
            self.metrics[6] = metrics.price_refinement_failures.to_string();
            self.metrics[7] = metrics.relabels.to_string();
            self.metrics[8] = metrics.price_refinement_arc_scans.to_string();
            self.metrics[9] = metrics.price_refinement_relaxations.to_string();
            self.metrics[10] = metrics.initial_saturations.to_string();
            self.metrics[11] = metrics.pushes.to_string();
            self.metrics[12] = metrics.saturating_pushes.to_string();
            self.metrics[13] = metrics.nonsaturating_pushes.to_string();
            self.metrics[14] = metrics.discharges.to_string();
            self.metrics[15] = metrics.active_vertex_selections.to_string();
            return Ok(());
        }
        let path_variant = matches!(
            self.algorithm.id.as_str(),
            "augment-relabel" | "partial-augment-relabel-mcf"
        );
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[5] = metrics.refine_phases.to_string();
        self.metrics[7] = metrics.relabels.to_string();
        self.metrics[8] = if path_variant {
            metrics.retreats.to_string()
        } else {
            metrics.current_arc_advances.to_string()
        };
        self.metrics[11] = metrics.pushes.to_string();
        self.metrics[12] = metrics.saturating_pushes.to_string();
        self.metrics[13] = metrics.nonsaturating_pushes.to_string();
        self.metrics[14] = if path_variant {
            metrics.path_augmentations
        } else {
            metrics.discharges
        }
        .to_string();
        self.metrics[15] = metrics.active_vertex_selections.to_string();
        self.metrics[3] = if path_variant {
            metrics.path_augmentations
        } else {
            metrics.initial_saturations
        }
        .to_string();
        self.metrics[4] = metrics.path_searches.to_string();
        self.metrics[6] = metrics.deficit_augmentations.to_string();
        self.metrics[9] = metrics.path_advances.to_string();
        self.metrics[10] = metrics.length_limit_augmentations.to_string();
        Ok(())
    }

    /// Applies a certified feasible-start Out-of-Kilter result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_out_of_kilter_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MinCostFlowCertificate,
        metrics: FlowOutOfKilterMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        self.metrics[0] = metrics.label_searches.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.breakthroughs.to_string();
        self.metrics[7] = metrics.price_updates.to_string();
        self.metrics[15] = metrics.selected_arcs.to_string();
        Ok(())
    }

    /// Applies a certified price-coordinate relaxation result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_relaxation_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        prices: &[i128],
        certificate: &MinCostFlowCertificate,
        metrics: FlowTraceMetrics,
    ) -> Result<(), FlowSceneError> {
        if prices.len() != graph.nodes().len() {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        self.apply_fast_snapshot(graph, flows)?;
        self.node_trace_states = graph
            .nodes()
            .iter()
            .zip(prices)
            .map(|(node, price)| FlowNodeTraceStateV1 {
                node_id: node.id().as_str().to_owned(),
                label: Some(price.to_string()),
                search_ordinal: None,
                remaining_divergence: None,
            })
            .collect();
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        self.metrics = trace_metrics(metrics);
        Ok(())
    }

    /// Applies a certified natural primal-network-simplex result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_network_simplex_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MinCostFlowCertificate,
        metrics: FlowNetworkSimplexMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        self.metrics[2] = metrics.pricing_arc_scans.to_string();
        self.metrics[3] = metrics.pivots.to_string();
        self.metrics[4] = metrics.pricing_searches.to_string();
        self.metrics[7] = metrics.potential_recomputations.to_string();
        self.metrics[8] = metrics.cycle_arc_scans.to_string();
        self.metrics[10] = metrics.bound_flips.to_string();
        self.metrics[11] = metrics.basis_exchanges.to_string();
        self.metrics[12] = metrics.nondegenerate_pivots.to_string();
        self.metrics[13] = metrics.degenerate_pivots.to_string();
        Ok(())
    }

    /// Applies a certified directional dynamic-tree network-simplex result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_dynamic_tree_network_simplex_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MinCostFlowCertificate,
        metrics: FlowDynamicTreeNetworkSimplexMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        self.metrics[0] = metrics.directional_forest_rebuilds.to_string();
        self.metrics[1] = metrics.path_minimum_queries.to_string();
        self.metrics[2] = metrics.pricing_arc_scans.to_string();
        self.metrics[3] = metrics.pivots.to_string();
        self.metrics[4] = metrics.pricing_searches.to_string();
        self.metrics[5] = metrics.path_updates.to_string();
        self.metrics[6] = metrics.directional_value_validations.to_string();
        self.metrics[7] = metrics.potential_recomputations.to_string();
        self.metrics[8] = metrics.cycle_arc_scans.to_string();
        self.metrics[9] = metrics.tree_links.to_string();
        self.metrics[10] = metrics.bound_flips.to_string();
        self.metrics[11] = metrics.basis_exchanges.to_string();
        self.metrics[12] = metrics.nondegenerate_pivots.to_string();
        self.metrics[13] = metrics.degenerate_pivots.to_string();
        self.metrics[14] = metrics.tree_cuts.to_string();
        self.metrics[15] = metrics.tree_rebuilds.to_string();
        Ok(())
    }

    /// Applies a certified Transportation Simplex or MODI optimum.
    ///
    /// # Errors
    ///
    /// Rejects a shipment vector that cannot reconstruct the residual scene.
    pub fn apply_transportation_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MinCostFlowCertificate,
        metrics: FlowTransportationMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        self.metrics[0] = metrics.feasibility_searches.to_string();
        self.metrics[1] = metrics.support_cycle_cancellations.to_string();
        self.metrics[2] = metrics.pricing_scans.to_string();
        self.metrics[3] = metrics.pivots.to_string();
        self.metrics[4] = metrics.pricing_searches.to_string();
        self.metrics[5] = metrics.basis_extensions.to_string();
        self.metrics[6] = metrics.basis_exchanges.to_string();
        self.metrics[7] = metrics.potential_recomputations.to_string();
        self.metrics[8] = metrics.degenerate_pivots.to_string();
        self.metrics[9] = metrics.structure_scans.to_string();
        self.metrics[10] = metrics.nondegenerate_pivots.to_string();
        Ok(())
    }

    /// Applies a certified SSAP minimum-cost maximum-flow result.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_min_cost_max_flow_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MinCostMaxFlowCertificate,
        metrics: FlowPotentialDijkstraMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_max_flow_outcome(graph, certificate);
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.augmentations.to_string();
        self.metrics[4] = metrics.dijkstra_runs.to_string();
        self.metrics[7] = metrics.potential_updates.to_string();
        self.metrics[15] = metrics.settled_nodes.to_string();
        Ok(())
    }

    /// Applies a certified cycle-canceling result to a ready scene.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_cycle_canceling_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MinCostFlowCertificate,
        metrics: FlowCycleCancelingMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        self.metrics[1] = metrics.relaxation_passes.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.canceled_cycles.to_string();
        self.metrics[4] = metrics.cycle_searches.to_string();
        Ok(())
    }

    /// Applies a certified minimum-mean-cycle-canceling result to a ready scene.
    ///
    /// # Errors
    ///
    /// Rejects a certified flow vector that cannot reconstruct the residual
    /// scene contract.
    pub fn apply_minimum_mean_cycle_canceling_result(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
        certificate: &MinCostFlowCertificate,
        metrics: FlowMinimumMeanCycleCancelingMetrics,
    ) -> Result<(), FlowSceneError> {
        self.apply_fast_snapshot(graph, flows)?;
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.set_min_cost_flow_outcome(graph, certificate);
        self.metrics[1] = metrics.dynamic_programming_rounds.to_string();
        self.metrics[2] = metrics.residual_arc_scans.to_string();
        self.metrics[3] = metrics.canceled_cycles.to_string();
        self.metrics[4] = metrics.mean_cycle_searches.to_string();
        Ok(())
    }

    /// Projects one boundary of the actual lower-bound feasibility subroutine.
    ///
    /// Artificial nodes and edges remain in a dedicated overlay; original-edge
    /// flows are simultaneously projected through the normal flow encoding.
    ///
    /// # Errors
    ///
    /// Rejects mismatched original identities, invalid auxiliary capacities,
    /// or a snapshot that cannot reconstruct a bounded original residual state.
    pub fn apply_feasibility_trace_snapshot(
        &mut self,
        graph: &FlowNetwork,
        request: &CapturedFeasibilityRequest,
        snapshot: &FeasibilityTraceSnapshot,
        event: Option<&FeasibilityTraceEvent>,
        event_id: u64,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        if snapshot.original_flows.len() != graph.edges().len() {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let mut flows = Vec::with_capacity(graph.edges().len());
        for (edge, projected) in graph.edges().iter().zip(&snapshot.original_flows) {
            if projected.edge != *edge.id()
                || projected.flow < edge.lower()
                || projected.flow > edge.capacity()
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            flows.push(projected.flow);
        }
        self.apply_fast_snapshot(graph, &flows)?;
        self.event_id = event_id.to_string();
        self.event_count = event_count.to_string();
        self.metrics = feasibility_scene_metrics(snapshot.metrics)?;
        self.feasibility_overlay = Some(feasibility_overlay(
            graph,
            graph,
            request,
            snapshot,
            event,
            FlowFeasibilityUseV1::InitialFlow,
        )?);
        Ok(())
    }

    /// Projects a feasibility check or recovery while preserving the public
    /// algorithm state underneath it.
    ///
    /// This is intentionally overlay-only: transformed node and edge
    /// identities do not belong to the input graph, so projecting their flow
    /// vector onto public edges would be a type error. The caller composes the
    /// captured work counters with the enclosing algorithm counters.
    ///
    /// # Errors
    ///
    /// Rejects an invalid auxiliary overlay snapshot.
    pub fn apply_auxiliary_feasibility_trace_snapshot(
        &mut self,
        projection: &FlowAuxiliaryFeasibilityProjection<'_>,
    ) -> Result<(), FlowSceneError> {
        if projection.use_kind == FlowFeasibilityUseV1::InitialFlow {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        self.event_id = projection.event_id.to_string();
        self.event_count = projection.event_count.to_string();
        self.feasibility_overlay = Some(feasibility_overlay(
            projection.public_graph,
            projection.kernel_graph,
            projection.request,
            projection.snapshot,
            projection.event,
            projection.use_kind,
        )?);
        Ok(())
    }

    /// Projects a complete reversible algorithm boundary for the renderer.
    ///
    /// # Errors
    ///
    /// Rejects mismatched stable identities, shapes, capacities, or bounded
    /// scene counts instead of publishing a partial projection.
    pub fn apply_trace_snapshot(
        &mut self,
        graph: &FlowNetwork,
        snapshot: &FlowTraceSnapshot,
        event: Option<&FlowTraceEvent>,
        event_count: u64,
    ) -> Result<(), FlowSceneError> {
        if snapshot.flows.len() != graph.edges().len()
            || snapshot.node_labels.len() != graph.nodes().len()
            || (!snapshot.remaining_divergence.is_empty()
                && snapshot.remaining_divergence.len() != graph.nodes().len())
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        self.event_id = event.map_or_else(|| "0".to_owned(), |item| item.event_id.to_string());
        self.event_count = event_count.to_string();
        self.apply_current_capacities(graph, snapshot)?;
        self.edge_states = edge_states(graph, &snapshot.flows);
        self.residual_arcs = residual_arc_states(graph, snapshot)?;
        self.node_trace_states = node_trace_states(graph, snapshot)?;
        self.pseudoflow_forest = pseudoflow_forest(graph, snapshot)?;
        self.eibfs_overlay = eibfs_overlay(graph, snapshot)?;
        self.dynamic_eibfs_overlay = dynamic_eibfs_overlay(graph, snapshot)?;
        self.trace_event = event.map(trace_event_scene).transpose()?;
        self.metrics = snapshot.dynamic_eibfs_overlay.as_ref().map_or_else(
            || trace_metrics(snapshot.metrics),
            |overlay| dynamic_eibfs_trace_metrics(overlay, snapshot.metrics),
        );
        Ok(())
    }

    fn apply_current_capacities(
        &mut self,
        graph: &FlowNetwork,
        snapshot: &FlowTraceSnapshot,
    ) -> Result<(), FlowSceneError> {
        if self.graph.edges.len() != graph.edges().len()
            || snapshot.edge_capacities.len() != graph.edges().len()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let mut capacities = graph
            .edges()
            .iter()
            .zip(&snapshot.edge_capacities)
            .map(|(edge, capacity)| (edge.id().as_str(), *capacity))
            .collect::<BTreeMap<_, _>>();
        for edge in &mut self.graph.edges {
            edge.capacity = capacities
                .remove(edge.id.as_str())
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?
                .to_string();
        }
        if !capacities.is_empty() {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        Ok(())
    }

    /// Attaches a verified max-flow/min-cut outcome without changing trace state.
    pub fn set_max_flow_outcome(&mut self, certificate: &MaxFlowCertificate) {
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.outcome = Some(FlowOutcomeV1::MaxFlow {
            value: certificate.value.to_string(),
            cut_bound: certificate.cut_bound.to_string(),
            source_side: certificate
                .source_side
                .iter()
                .map(|node| node.as_str().to_owned())
                .collect(),
        });
    }

    /// Attaches a verified matching/minimum-cover outcome without changing trace state.
    pub fn set_bipartite_matching_outcome(&mut self, certificate: &BipartiteMatchingCertificate) {
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.outcome = Some(FlowOutcomeV1::BipartiteMatching {
            cardinality: certificate.cardinality.to_string(),
            pairs: certificate
                .pairs
                .iter()
                .map(|pair| FlowBipartiteMatchingPairV1 {
                    edge_id: pair.edge.as_str().to_owned(),
                    left: pair.left.as_str().to_owned(),
                    right: pair.right.as_str().to_owned(),
                })
                .collect(),
            cover_left: certificate
                .cover_left
                .iter()
                .map(|node| node.as_str().to_owned())
                .collect(),
            cover_right: certificate
                .cover_right
                .iter()
                .map(|node| node.as_str().to_owned())
                .collect(),
        });
    }

    /// Attaches a verified min-cost primal/dual outcome without changing trace state.
    pub fn set_min_cost_flow_outcome(
        &mut self,
        graph: &FlowNetwork,
        certificate: &MinCostFlowCertificate,
    ) {
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.outcome = Some(FlowOutcomeV1::MinCostFlow {
            total_cost: certificate.total_cost.to_string(),
            potentials: graph
                .node_indices()
                .zip(&certificate.potentials)
                .filter_map(|(index, potential)| {
                    graph.node(index).map(|node| FlowNodePotentialV1 {
                        node_id: node.id().as_str().to_owned(),
                        potential: potential.to_string(),
                    })
                })
                .collect(),
        });
    }

    /// Attaches verified maximum-flow and minimum-cost certificates together.
    pub fn set_min_cost_max_flow_outcome(
        &mut self,
        graph: &FlowNetwork,
        certificate: &MinCostMaxFlowCertificate,
    ) {
        self.solve_status = FlowSolveStatusV1::Optimal;
        self.outcome = Some(FlowOutcomeV1::MinCostMaxFlow {
            value: certificate.max_flow.value.to_string(),
            cut_bound: certificate.max_flow.cut_bound.to_string(),
            source_side: certificate
                .max_flow
                .source_side
                .iter()
                .map(|node| node.as_str().to_owned())
                .collect(),
            total_cost: certificate.min_cost.total_cost.to_string(),
            potentials: graph
                .node_indices()
                .zip(&certificate.min_cost.potentials)
                .filter_map(|(index, potential)| {
                    graph.node(index).map(|node| FlowNodePotentialV1 {
                        node_id: node.id().as_str().to_owned(),
                        potential: potential.to_string(),
                    })
                })
                .collect(),
        });
    }

    fn apply_fast_snapshot(
        &mut self,
        graph: &FlowNetwork,
        flows: &[u64],
    ) -> Result<(), FlowSceneError> {
        let state = ResidualState::from_flows(graph, flows)?;
        let snapshot = FlowTraceSnapshot::capture(
            graph,
            &state,
            vec![None; graph.nodes().len()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            FlowTraceMetrics::default(),
        );
        self.apply_trace_snapshot(graph, &snapshot, None, 1)?;
        self.event_id.clear();
        self.event_id.push('1');
        Ok(())
    }

    /// Applies an independently verified infeasibility witness.
    pub fn apply_infeasibility(&mut self, witness: &InfeasibilityWitness) {
        self.event_id.clear();
        self.event_id.push('1');
        self.event_count.clear();
        self.event_count.push('1');
        self.solve_status = FlowSolveStatusV1::Infeasible;
        self.outcome = Some(FlowOutcomeV1::Infeasible {
            unsatisfied: witness.unsatisfied.to_string(),
            reachable_original_nodes: witness
                .reachable_original_nodes
                .iter()
                .map(|node| node.as_str().to_owned())
                .collect(),
        });
    }

    /// Marks an otherwise unspecified declared ceiling without publishing a candidate optimum.
    pub fn apply_resource_limit(&mut self) {
        self.apply_resource_limit_with_reason(FlowResourceLimitReasonV1::DeclaredCeiling);
    }

    /// Marks a deterministic resource ceiling and preserves its public cause.
    pub fn apply_resource_limit_with_reason(&mut self, reason: FlowResourceLimitReasonV1) {
        self.event_id.clear();
        self.event_id.push('1');
        self.event_count.clear();
        self.event_count.push('1');
        self.solve_status = FlowSolveStatusV1::ResourceLimit;
        self.resource_limit_reason = Some(reason);
        self.outcome = None;
    }
}

fn feasibility_scene_metrics(
    metrics: FeasibilityTraceMetrics,
) -> Result<[String; FLOW_METRIC_COUNT], FlowSceneError> {
    let residual_arc_scans = metrics
        .original_edge_inspections
        .checked_add(metrics.original_node_inspections)
        .and_then(|value| value.checked_add(metrics.auxiliary_adjacency_inspections))
        .and_then(|value| value.checked_add(metrics.cut_adjacency_inspections))
        .and_then(|value| value.checked_add(metrics.extracted_original_edges))
        .ok_or(FlowSceneError::CountOverflow)?;
    let mut result = std::array::from_fn(|_| "0".to_owned());
    result[2] = residual_arc_scans.to_string();
    result[7] = metrics.relabels.to_string();
    result[11] = metrics.pushes.to_string();
    result[14] = metrics.discharges.to_string();
    result[15] = metrics.active_node_selections.to_string();
    Ok(result)
}

fn feasibility_overlay(
    public_graph: &FlowNetwork,
    kernel_graph: &FlowNetwork,
    request: &CapturedFeasibilityRequest,
    snapshot: &FeasibilityTraceSnapshot,
    event: Option<&FeasibilityTraceEvent>,
    use_kind: FlowFeasibilityUseV1,
) -> Result<FlowFeasibilityOverlayV2, FlowSceneError> {
    let focus_arc = event.and_then(|item| item.focus_arc.as_ref());
    let nodes = snapshot
        .nodes
        .iter()
        .map(|node| FlowFeasibilityNodeStateV1 {
            node: feasibility_node_ref(&node.id),
            height: node.height.to_string(),
            excess: node.excess.to_string(),
            current_arc: node.current_arc.to_string(),
            active: node.active,
            reachable: node.reachable,
            queue_position: snapshot
                .active_queue
                .iter()
                .position(|queued| queued == &node.id)
                .map(|position| position.to_string()),
        })
        .collect();
    let arcs = snapshot
        .arcs
        .iter()
        .map(|arc| {
            let forward_residual = arc
                .capacity
                .checked_sub(arc.flow)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let focused_direction = focus_arc
                .filter(|focused| focused.arc == arc.id)
                .map(|focused| feasibility_direction(focused.direction).to_owned());
            Ok(FlowFeasibilityArcStateV1 {
                arc: feasibility_arc_ref(&arc.id),
                from: feasibility_node_ref(&arc.from),
                to: feasibility_node_ref(&arc.to),
                capacity: arc.capacity.to_string(),
                flow: arc.flow.to_string(),
                forward_residual: forward_residual.to_string(),
                reverse_residual: arc.flow.to_string(),
                focused: focused_direction.is_some(),
                focused_direction,
            })
        })
        .collect::<Result<Vec<_>, FlowSceneError>>()?;
    let domain = feasibility_domain(public_graph, kernel_graph, request, use_kind)?;
    let domain_is_public = domain.kind == FlowFeasibilityDomainKindV1::PublicInput;
    if domain_is_public != (use_kind != FlowFeasibilityUseV1::AnchoredRecovery) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(FlowFeasibilityOverlayV2 {
        revision: "flow-feasibility-overlay/2".to_owned(),
        use_kind,
        domain,
        stage: event.map_or(FlowFeasibilityStageV1::Ready, |item| {
            feasibility_stage(item.kind)
        }),
        nodes,
        arcs,
        active_queue: snapshot
            .active_queue
            .iter()
            .map(feasibility_node_ref)
            .collect(),
        focus_node: event
            .and_then(|item| item.focus_node.as_ref())
            .map(feasibility_node_ref),
        focus_arc: focus_arc.map(feasibility_residual_arc_ref),
        total_required: snapshot.total_required.to_string(),
        routed: snapshot.routed.to_string(),
        metrics: feasibility_metrics(snapshot.metrics),
    })
}

fn feasibility_domain(
    public_graph: &FlowNetwork,
    kernel_graph: &FlowNetwork,
    request: &CapturedFeasibilityRequest,
    use_kind: FlowFeasibilityUseV1,
) -> Result<FlowFeasibilityDomainV1, FlowSceneError> {
    let nodes = kernel_graph
        .nodes()
        .iter()
        .map(|node| FlowFeasibilityDomainNodeV1 {
            node_id: node.id().as_str().to_owned(),
            public_node_id: public_graph
                .node_index(node.id())
                .map(|_| node.id().as_str().to_owned()),
        })
        .collect::<Vec<_>>();
    let edges = kernel_graph
        .edges()
        .iter()
        .map(|edge| {
            let from = kernel_graph
                .node(edge.from())
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?
                .id();
            let to = kernel_graph
                .node(edge.to())
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?
                .id();
            let public_route_edge_id = public_graph.edge_index(edge.id()).and_then(|index| {
                let public_edge = public_graph.edge(index)?;
                let public_from = public_graph.node(public_edge.from())?.id();
                let public_to = public_graph.node(public_edge.to())?.id();
                (public_from == from && public_to == to).then(|| edge.id().as_str().to_owned())
            });
            Ok(FlowFeasibilityDomainEdgeV1 {
                edge_id: edge.id().as_str().to_owned(),
                from_node_id: from.as_str().to_owned(),
                to_node_id: to.as_str().to_owned(),
                lower: edge.lower().to_string(),
                capacity: edge.capacity().to_string(),
                public_route_edge_id,
            })
        })
        .collect::<Result<Vec<_>, FlowSceneError>>()?;
    let kind = if use_kind != FlowFeasibilityUseV1::AnchoredRecovery {
        if kernel_graph != public_graph {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        FlowFeasibilityDomainKindV1::PublicInput
    } else if nodes.iter().all(|node| node.public_node_id.is_some()) {
        FlowFeasibilityDomainKindV1::NodeAlignedTransformation
    } else {
        FlowFeasibilityDomainKindV1::StandaloneTransformation
    };
    let request = match request {
        CapturedFeasibilityRequest::Balance {
            required_divergence,
        } => {
            if required_divergence.len() != kernel_graph.nodes().len() {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            FlowFeasibilityRequestV1::Balance {
                required_divergences: kernel_graph
                    .nodes()
                    .iter()
                    .zip(required_divergence)
                    .map(|(node, value)| FlowFeasibilityRequiredDivergenceV1 {
                        node_id: node.id().as_str().to_owned(),
                        required_divergence: value.to_string(),
                    })
                    .collect(),
            }
        }
        CapturedFeasibilityRequest::MaxFlowInitial { source, sink } => {
            let source_node_id = kernel_graph
                .node(*source)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?
                .id()
                .as_str()
                .to_owned();
            let sink_node_id = kernel_graph
                .node(*sink)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?
                .id()
                .as_str()
                .to_owned();
            FlowFeasibilityRequestV1::MaxFlowInitial {
                source_node_id,
                sink_node_id,
            }
        }
    };
    Ok(FlowFeasibilityDomainV1 {
        kind,
        nodes,
        edges,
        request,
    })
}

fn feasibility_metrics(metrics: FeasibilityTraceMetrics) -> FlowFeasibilityMetricsV1 {
    FlowFeasibilityMetricsV1 {
        original_edge_inspections: metrics.original_edge_inspections.to_string(),
        original_node_inspections: metrics.original_node_inspections.to_string(),
        auxiliary_adjacency_inspections: metrics.auxiliary_adjacency_inspections.to_string(),
        pushes: metrics.pushes.to_string(),
        relabels: metrics.relabels.to_string(),
        active_node_selections: metrics.active_node_selections.to_string(),
        discharges: metrics.discharges.to_string(),
        cut_adjacency_inspections: metrics.cut_adjacency_inspections.to_string(),
        extracted_original_edges: metrics.extracted_original_edges.to_string(),
    }
}

const fn feasibility_stage(kind: FeasibilityTraceEventKind) -> FlowFeasibilityStageV1 {
    match kind {
        FeasibilityTraceEventKind::AddOriginalArc => FlowFeasibilityStageV1::AddOriginalArc,
        FeasibilityTraceEventKind::AddReturnArc => FlowFeasibilityStageV1::AddReturnArc,
        FeasibilityTraceEventKind::InspectNodeImbalance => {
            FlowFeasibilityStageV1::InspectNodeImbalance
        }
        FeasibilityTraceEventKind::AddImbalanceArc => FlowFeasibilityStageV1::AddImbalanceArc,
        FeasibilityTraceEventKind::InitializeSourceHeight => {
            FlowFeasibilityStageV1::InitializeSourceHeight
        }
        FeasibilityTraceEventKind::InspectSourceArc => FlowFeasibilityStageV1::InspectSourceArc,
        FeasibilityTraceEventKind::ActivateNode => FlowFeasibilityStageV1::ActivateNode,
        FeasibilityTraceEventKind::SelectActiveNode => FlowFeasibilityStageV1::SelectActiveNode,
        FeasibilityTraceEventKind::InspectDischargeArc => {
            FlowFeasibilityStageV1::InspectDischargeArc
        }
        FeasibilityTraceEventKind::InspectRelabelArc => FlowFeasibilityStageV1::InspectRelabelArc,
        FeasibilityTraceEventKind::Push => FlowFeasibilityStageV1::Push,
        FeasibilityTraceEventKind::AdvanceCurrentArc => FlowFeasibilityStageV1::AdvanceCurrentArc,
        FeasibilityTraceEventKind::Relabel => FlowFeasibilityStageV1::Relabel,
        FeasibilityTraceEventKind::CompleteDischarge => FlowFeasibilityStageV1::CompleteDischarge,
        FeasibilityTraceEventKind::CompleteRouting => FlowFeasibilityStageV1::CompleteRouting,
        FeasibilityTraceEventKind::InspectCutArc => FlowFeasibilityStageV1::InspectCutArc,
        FeasibilityTraceEventKind::MarkReachable => FlowFeasibilityStageV1::MarkReachable,
        FeasibilityTraceEventKind::ExtractOriginalFlow => {
            FlowFeasibilityStageV1::ExtractOriginalFlow
        }
        FeasibilityTraceEventKind::Feasible => FlowFeasibilityStageV1::Feasible,
        FeasibilityTraceEventKind::Infeasible => FlowFeasibilityStageV1::Infeasible,
    }
}

fn feasibility_node_ref(node: &FeasibilityNodeId) -> FlowFeasibilityNodeRefV1 {
    match node {
        FeasibilityNodeId::Original(id) => FlowFeasibilityNodeRefV1 {
            kind: FlowFeasibilityNodeKindV1::Original,
            original_node_id: Some(id.as_str().to_owned()),
        },
        FeasibilityNodeId::SuperSource => FlowFeasibilityNodeRefV1 {
            kind: FlowFeasibilityNodeKindV1::SuperSource,
            original_node_id: None,
        },
        FeasibilityNodeId::SuperSink => FlowFeasibilityNodeRefV1 {
            kind: FlowFeasibilityNodeKindV1::SuperSink,
            original_node_id: None,
        },
    }
}

fn feasibility_arc_ref(arc: &FeasibilityArcId) -> FlowFeasibilityArcRefV1 {
    match arc {
        FeasibilityArcId::Original(id) => FlowFeasibilityArcRefV1 {
            kind: FlowFeasibilityArcKindV1::Original,
            original_edge_id: Some(id.as_str().to_owned()),
            imbalance_node_id: None,
            return_from: None,
            return_to: None,
        },
        FeasibilityArcId::LowerBoundReturn { from, to } => FlowFeasibilityArcRefV1 {
            kind: FlowFeasibilityArcKindV1::LowerBoundReturn,
            original_edge_id: None,
            imbalance_node_id: None,
            return_from: Some(from.as_str().to_owned()),
            return_to: Some(to.as_str().to_owned()),
        },
        FeasibilityArcId::FromSuperSource(node) => FlowFeasibilityArcRefV1 {
            kind: FlowFeasibilityArcKindV1::FromSuperSource,
            original_edge_id: None,
            imbalance_node_id: Some(node.as_str().to_owned()),
            return_from: None,
            return_to: None,
        },
        FeasibilityArcId::ToSuperSink(node) => FlowFeasibilityArcRefV1 {
            kind: FlowFeasibilityArcKindV1::ToSuperSink,
            original_edge_id: None,
            imbalance_node_id: Some(node.as_str().to_owned()),
            return_from: None,
            return_to: None,
        },
    }
}

fn feasibility_residual_arc_ref(
    residual: &FeasibilityResidualArcId,
) -> FlowFeasibilityResidualArcRefV1 {
    FlowFeasibilityResidualArcRefV1 {
        arc: feasibility_arc_ref(&residual.arc),
        direction: feasibility_direction(residual.direction).to_owned(),
    }
}

const fn feasibility_direction(direction: FeasibilityResidualDirection) -> &'static str {
    match direction {
        FeasibilityResidualDirection::Forward => "forward",
        FeasibilityResidualDirection::Reverse => "reverse",
    }
}

fn edge_states(graph: &FlowNetwork, flows: &[u64]) -> Vec<FlowEdgeStateV1> {
    graph
        .edges()
        .iter()
        .zip(flows)
        .map(|(edge, flow)| FlowEdgeStateV1 {
            edge_id: edge.id().as_str().to_owned(),
            flow: flow.to_string(),
        })
        .collect()
}

fn binary_blocking_overlay(
    graph: &FlowNetwork,
    result: &BinaryBlockingStepResult,
    stage: FlowBinaryBlockingStageV1,
) -> Result<FlowBinaryBlockingOverlayV1, FlowSceneError> {
    if result.distances.len() != graph.nodes().len()
        || result.component_of.len() != graph.nodes().len()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let nodes = graph
        .node_indices()
        .zip(&result.distances)
        .zip(&result.component_of)
        .map(|((index, distance), component)| {
            graph
                .node(index)
                .map(|node| FlowBinaryBlockingNodeStateV1 {
                    node_id: node.id().as_str().to_owned(),
                    distance: distance.map(|value| value.to_string()),
                    component: component.to_string(),
                })
                .ok_or(FlowSceneError::SnapshotGraphMismatch)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let residual_refs = |ids: &[crate::residual::ResidualArcId]| {
        ids.iter()
            .map(|id| residual_arc_ref(graph, id))
            .collect::<Result<Vec<_>, _>>()
    };
    Ok(FlowBinaryBlockingOverlayV1 {
        stage,
        upper_bound: result.upper_bound.to_string(),
        delta: result.delta.to_string(),
        delivered: if stage == FlowBinaryBlockingStageV1::Complete {
            result.value.to_string()
        } else {
            "0".to_owned()
        },
        nodes,
        base_zero_arcs: residual_refs(&result.base_zero_arcs)?,
        special_arcs: residual_refs(&result.special_arcs)?,
        admissible_arcs: residual_refs(&result.admissible_arcs)?,
        zero_admissible_arcs: residual_refs(&result.zero_admissible_arcs)?,
    })
}

fn residual_arc_ref(
    graph: &FlowNetwork,
    id: &crate::residual::ResidualArcId,
) -> Result<FlowResidualArcRefV1, FlowSceneError> {
    if graph.edge_index(id.original_edge()).is_none() {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(FlowResidualArcRefV1 {
        edge_id: id.original_edge().as_str().to_owned(),
        direction: match id.direction() {
            ResidualDirection::Forward => "forward",
            ResidualDirection::Reverse => "reverse",
        }
        .to_owned(),
    })
}

fn binary_component_counts(component_of: &[usize]) -> (usize, usize) {
    let mut counts = BTreeMap::<usize, usize>::new();
    for component in component_of {
        *counts.entry(*component).or_default() += 1;
    }
    let nontrivial = counts.values().filter(|&&count| count > 1).count();
    (counts.len(), nontrivial)
}

fn assignment_labels(
    graph: &FlowNetwork,
    labels: &[i128],
    agents: bool,
    model: &FlowProblemModelV1,
) -> Vec<FlowAssignmentLabelV1> {
    let ids = match model {
        FlowProblemModelV1::Assignment {
            agents: agent_ids,
            tasks,
            ..
        } => {
            if agents {
                agent_ids
            } else {
                tasks
            }
        }
        _ => return Vec::new(),
    };
    ids.iter()
        .zip(labels)
        .filter_map(|(id, label)| {
            crate::model::NodeId::parse(id)
                .ok()
                .and_then(|node| graph.node_index(&node))
                .map(|_| FlowAssignmentLabelV1 {
                    node_id: id.clone(),
                    label: label.to_string(),
                })
        })
        .collect()
}

fn residual_arc_states(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
) -> Result<Vec<FlowResidualArcStateV1>, FlowSceneError> {
    let active = snapshot.active_path.iter().collect::<BTreeSet<_>>();
    let fixed = snapshot.fixed_edges.iter().collect::<BTreeSet<_>>();
    snapshot
        .residual_capacities
        .iter()
        .map(|(id, capacity)| {
            let edge_index = graph
                .edge_index(id.original_edge())
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let edge = graph
                .edge(edge_index)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let flow = *snapshot
                .flows
                .get(edge_index.as_usize())
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let current_capacity = *snapshot
                .edge_capacities
                .get(edge_index.as_usize())
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let temporary_over_capacity = flow > current_capacity
                && snapshot
                    .dynamic_eibfs_overlay
                    .as_ref()
                    .is_some_and(|overlay| {
                        overlay.stage == DynamicEibfsTraceStage::ApplyUpdate
                            && overlay.changed_edge.as_ref() == Some(edge.id())
                            && overlay.new_capacity == Some(current_capacity)
                            && overlay.old_capacity.is_some_and(|old| flow <= old)
                    });
            if flow > current_capacity && !temporary_over_capacity {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            let (from, to, expected_capacity, cost, direction) = match id.direction() {
                ResidualDirection::Forward => (
                    edge.from(),
                    edge.to(),
                    current_capacity.saturating_sub(flow),
                    i128::from(edge.cost()),
                    "forward",
                ),
                ResidualDirection::Reverse => (
                    edge.to(),
                    edge.from(),
                    flow - edge.lower(),
                    -i128::from(edge.cost()),
                    "reverse",
                ),
            };
            if *capacity != expected_capacity {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            let from = graph
                .node(from)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let to = graph
                .node(to)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            Ok(FlowResidualArcStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                direction: direction.to_owned(),
                from: from.id().as_str().to_owned(),
                to: to.id().as_str().to_owned(),
                capacity: capacity.to_string(),
                cost: cost.to_string(),
                active: active.contains(id),
                fixed: fixed.contains(id.original_edge()),
            })
        })
        .collect()
}

fn pseudoflow_forest(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
) -> Result<Option<FlowPseudoflowForestV1>, FlowSceneError> {
    if snapshot.forest_arcs.is_empty() && snapshot.strong_nodes.is_empty() {
        return Ok(None);
    }
    let arcs = snapshot
        .forest_arcs
        .iter()
        .map(|id| {
            if graph.edge_index(id.original_edge()).is_none() {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            Ok(FlowResidualArcRefV1 {
                edge_id: id.original_edge().as_str().to_owned(),
                direction: match id.direction() {
                    ResidualDirection::Forward => "forward",
                    ResidualDirection::Reverse => "reverse",
                }
                .to_owned(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let strong_nodes = snapshot
        .strong_nodes
        .iter()
        .map(|node| {
            graph
                .node_index(node)
                .map(|_| node.as_str().to_owned())
                .ok_or(FlowSceneError::SnapshotGraphMismatch)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(FlowPseudoflowForestV1 { arcs, strong_nodes }))
}

fn eibfs_overlay(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
) -> Result<Option<FlowEibfsOverlayV1>, FlowSceneError> {
    let Some(overlay) = snapshot.eibfs_overlay.as_ref() else {
        return Ok(None);
    };
    let nodes = overlay
        .nodes
        .iter()
        .map(|node| {
            if graph.node_index(&node.node).is_none() {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            Ok(FlowEibfsNodeStateV1 {
                node_id: node.node.as_str().to_owned(),
                source_label: node.source_label.to_string(),
                sink_label: node.sink_label.to_string(),
                membership: trace_membership(node.membership).to_owned(),
                root_kind: match node.root_kind {
                    EibfsTraceRootKind::None => "none",
                    EibfsTraceRootKind::Source => "source",
                    EibfsTraceRootKind::Sink => "sink",
                    EibfsTraceRootKind::Excess => "excess",
                    EibfsTraceRootKind::Deficit => "deficit",
                }
                .to_owned(),
                orphan: node.orphan,
                imbalance: node.imbalance.to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let forest_arcs = overlay
        .forest_arcs
        .iter()
        .map(|relation| {
            if graph.node_index(&relation.parent).is_none()
                || graph.node_index(&relation.child).is_none()
                || graph
                    .edge_index(relation.admissible_residual.original_edge())
                    .is_none()
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            Ok(FlowEibfsForestArcV1 {
                parent: relation.parent.as_str().to_owned(),
                child: relation.child.as_str().to_owned(),
                side: trace_membership(relation.side).to_owned(),
                admissible_residual: FlowResidualArcRefV1 {
                    edge_id: relation
                        .admissible_residual
                        .original_edge()
                        .as_str()
                        .to_owned(),
                    direction: match relation.admissible_residual.direction() {
                        ResidualDirection::Forward => "forward",
                        ResidualDirection::Reverse => "reverse",
                    }
                    .to_owned(),
                },
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(FlowEibfsOverlayV1 {
        phase_direction: match overlay.phase_direction {
            EibfsTracePhaseDirection::Forward => "forward",
            EibfsTracePhaseDirection::Reverse => "reverse",
        }
        .to_owned(),
        source_depth: overlay.source_depth.to_string(),
        sink_depth: overlay.sink_depth.to_string(),
        nodes,
        forest_arcs,
    }))
}

fn dynamic_eibfs_overlay(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
) -> Result<Option<FlowDynamicEibfsOverlayV1>, FlowSceneError> {
    let Some(overlay) = snapshot.dynamic_eibfs_overlay.as_ref() else {
        return Ok(None);
    };
    if overlay
        .changed_edge
        .as_ref()
        .is_some_and(|edge| graph.edge_index(edge).is_none())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(Some(FlowDynamicEibfsOverlayV1 {
        stage: match overlay.stage {
            DynamicEibfsTraceStage::InitialSolve => "initial-solve",
            DynamicEibfsTraceStage::ApplyUpdate => "apply-update",
            DynamicEibfsTraceStage::RepairCapacity => "repair-capacity",
            DynamicEibfsTraceStage::RepairForest => "repair-forest",
            DynamicEibfsTraceStage::RepairViolation => "repair-violation",
            DynamicEibfsTraceStage::ContinueSolve => "continue-solve",
            DynamicEibfsTraceStage::PrefixRecovery => "prefix-recovery",
            DynamicEibfsTraceStage::PrefixCertified => "prefix-certified",
            DynamicEibfsTraceStage::ResumeReusablePseudoflow => "resume-reusable-pseudoflow",
        }
        .to_owned(),
        update_index: overlay.update_index.to_string(),
        update_total: overlay.update_total.to_string(),
        changed_edge: overlay
            .changed_edge
            .as_ref()
            .map(|edge| edge.as_str().to_owned()),
        old_capacity: overlay.old_capacity.map(|value| value.to_string()),
        new_capacity: overlay.new_capacity.map(|value| value.to_string()),
        violation: overlay
            .violation
            .map(dynamic_eibfs_violation)
            .map(str::to_owned),
        reused_forest_nodes: overlay.reused_forest_nodes.to_string(),
        updates_applied: overlay.updates_applied.to_string(),
        capacity_increases: overlay.capacity_increases.to_string(),
        capacity_decreases: overlay.capacity_decreases.to_string(),
        no_op_updates: overlay.no_op_updates.to_string(),
        over_capacity_repairs: overlay.over_capacity_repairs.to_string(),
        invalidated_parent_arcs: overlay.invalidated_parent_arcs.to_string(),
        promoted_roots: overlay.promoted_roots.to_string(),
        repair_arc_scans: overlay.repair_arc_scans.to_string(),
        state_transitions: overlay.state_transitions.to_string(),
        bridge_violations: overlay.bridge_violations.to_string(),
        label_violations: overlay.label_violations.to_string(),
        current_arc_violations: overlay.current_arc_violations.to_string(),
        boundary_violations: overlay.boundary_violations.to_string(),
        repair_iterations: overlay.repair_iterations.to_string(),
        certification_recoveries: overlay.certification_recoveries.to_string(),
        prefix_value: overlay.prefix_value.map(|value| value.to_string()),
    }))
}

const fn dynamic_eibfs_violation(violation: DynamicEibfsTraceViolation) -> &'static str {
    match violation {
        DynamicEibfsTraceViolation::OverCapacity => "over-capacity",
        DynamicEibfsTraceViolation::Bridge => "bridge",
        DynamicEibfsTraceViolation::Label => "label",
        DynamicEibfsTraceViolation::CurrentArc => "current-arc",
        DynamicEibfsTraceViolation::Boundary => "boundary",
    }
}

fn dynamic_eibfs_trace_metrics(
    overlay: &crate::trace::DynamicEibfsTraceOverlay,
    _trace: FlowTraceMetrics,
) -> [String; FLOW_METRIC_COUNT] {
    [
        overlay.updates_applied.to_string(),
        overlay.capacity_increases.to_string(),
        overlay.repair_arc_scans.to_string(),
        overlay.bridge_violations.to_string(),
        overlay.label_violations.to_string(),
        overlay.capacity_decreases.to_string(),
        overlay.reused_forest_nodes.to_string(),
        overlay.current_arc_violations.to_string(),
        overlay.invalidated_parent_arcs.to_string(),
        overlay.boundary_violations.to_string(),
        overlay.over_capacity_repairs.to_string(),
        overlay.promoted_roots.to_string(),
        overlay.no_op_updates.to_string(),
        overlay.repair_iterations.to_string(),
        overlay.certification_recoveries.to_string(),
        overlay.state_transitions.to_string(),
    ]
}

fn validate_cancel_tighten_overlay(
    graph: &FlowNetwork,
    overlay: &FlowCancelTightenOverlayV1,
) -> Result<(), FlowSceneError> {
    if overlay.nodes.len() != graph.nodes().len()
        || overlay
            .nodes
            .iter()
            .zip(graph.nodes())
            .any(|(state, node)| state.node_id != node.id().as_str())
        || overlay
            .admissible_arcs
            .iter()
            .chain(&overlay.active_cycle)
            .chain(&overlay.inspected_arcs)
            .any(|arc| {
                graph
                    .edges()
                    .iter()
                    .all(|edge| edge.id().as_str() != arc.edge_id)
                    || !matches!(arc.direction.as_str(), "forward" | "reverse")
            })
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

fn validate_relaxed_mndc_overlay(
    graph: &FlowNetwork,
    overlay: &FlowRelaxedMndcOverlayV1,
) -> Result<(), FlowSceneError> {
    if overlay.nodes.len() != graph.nodes().len()
        || overlay
            .nodes
            .iter()
            .zip(graph.nodes())
            .any(|(state, node)| state.node_id != node.id().as_str())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let node_ids = graph
        .nodes()
        .iter()
        .map(|node| node.id().as_str())
        .collect::<BTreeSet<_>>();
    if overlay.inspected_arcs.iter().any(|arc| {
        graph
            .edges()
            .iter()
            .all(|edge| edge.id().as_str() != arc.edge_id)
            || !matches!(arc.direction.as_str(), "forward" | "reverse")
    }) || overlay.active_assignment_cell.as_ref().is_some_and(|cell| {
        !node_ids.contains(cell.row_node_id.as_str())
            || !node_ids.contains(cell.column_node_id.as_str())
    }) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let matched_ids = overlay
        .nodes
        .iter()
        .map(|state| state.matched_node_id.as_str())
        .collect::<BTreeSet<_>>();
    if matched_ids != node_ids {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    for state in &overlay.nodes {
        let Some(selected) = &state.selected_arc else {
            if state.node_id != state.matched_node_id {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            continue;
        };
        let edge = graph
            .edges()
            .iter()
            .find(|edge| edge.id().as_str() == selected.edge_id)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let endpoints = match selected.direction.as_str() {
            "forward" => (edge.from(), edge.to()),
            "reverse" => (edge.to(), edge.from()),
            _ => return Err(FlowSceneError::SnapshotGraphMismatch),
        };
        if graph
            .node(endpoints.0)
            .is_none_or(|node| node.id().as_str() != state.node_id)
            || graph
                .node(endpoints.1)
                .is_none_or(|node| node.id().as_str() != state.matched_node_id)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    let mut family_nodes = BTreeSet::new();
    for cycle in &overlay.family {
        if cycle.arcs.is_empty() {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let mut first_from = None;
        let mut previous_to = None;
        for arc in &cycle.arcs {
            let edge = graph
                .edges()
                .iter()
                .find(|edge| edge.id().as_str() == arc.edge_id)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let endpoints = match arc.direction.as_str() {
                "forward" => (edge.from(), edge.to()),
                "reverse" => (edge.to(), edge.from()),
                _ => return Err(FlowSceneError::SnapshotGraphMismatch),
            };
            if previous_to.is_some_and(|node| node != endpoints.0)
                || !family_nodes.insert(endpoints.0)
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            first_from.get_or_insert(endpoints.0);
            previous_to = Some(endpoints.1);
        }
        if first_from != previous_to {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    Ok(())
}

fn validate_enhanced_capacity_scaling_overlay(
    graph: &FlowNetwork,
    overlay: &FlowEnhancedCapacityScalingOverlayV1,
) -> Result<(), FlowSceneError> {
    if overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
        || overlay
            .nodes
            .iter()
            .zip(graph.nodes())
            .any(|(state, node)| state.node_id != node.id().as_str())
        || overlay
            .edges
            .iter()
            .zip(graph.edges())
            .any(|(state, edge)| state.edge_id != edge.id().as_str())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let index = validate_enhanced_scaling_components(graph, overlay)?;
    validate_enhanced_scaling_edges(graph, overlay, &index)?;
    if overlay
        .contraction_arc
        .as_ref()
        .is_some_and(|id| graph.edges().iter().all(|edge| edge.id().as_str() != id))
        || overlay.augmentation.as_ref().is_some_and(|value| {
            !equal_unsigned_rationals(value, &overlay.delta)
                || value.numerator.parse::<u128>().is_err()
        })
        || overlay.path.iter().any(|arc| {
            graph
                .edges()
                .iter()
                .all(|edge| edge.id().as_str() != arc.edge_id)
                || !matches!(arc.direction.as_str(), "forward" | "reverse")
        })
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

struct EnhancedScalingOverlayIndex {
    delta_numerator: u128,
    delta_denominator: u128,
    component_by_node: BTreeMap<String, String>,
}

fn validate_enhanced_scaling_components(
    graph: &FlowNetwork,
    overlay: &FlowEnhancedCapacityScalingOverlayV1,
) -> Result<EnhancedScalingOverlayIndex, FlowSceneError> {
    let denominator = overlay
        .delta
        .denominator
        .parse::<u128>()
        .ok()
        .filter(|&value| value > 0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let delta = overlay
        .delta
        .numerator
        .parse::<u128>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let mut component_by_node = BTreeMap::<String, String>::new();
    let mut component_ids = BTreeSet::new();
    for component in &overlay.components {
        if component.members.is_empty()
            || component.component_id != component.members[0]
            || !component_ids.insert(component.component_id.clone())
            || component
                .excess
                .denominator
                .parse::<u128>()
                .ok()
                .is_none_or(|value| value == 0)
            || component.excess.numerator.parse::<i128>().is_err()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        for member in &component.members {
            if graph
                .nodes()
                .iter()
                .all(|node| node.id().as_str() != member)
                || component_by_node
                    .insert(member.clone(), component.component_id.clone())
                    .is_some()
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
        }
    }
    if component_by_node.len() != graph.nodes().len()
        || overlay.nodes.iter().any(|state| {
            component_by_node.get(&state.node_id) != Some(&state.component_id)
                || state.potential.parse::<i128>().is_err()
                || state
                    .distance
                    .as_ref()
                    .is_some_and(|value| value.parse::<i128>().is_err())
        })
        || overlay
            .source_component
            .as_deref()
            .is_some_and(|id| !component_ids.contains(id))
        || overlay
            .sink_component
            .as_deref()
            .is_some_and(|id| !component_ids.contains(id))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(EnhancedScalingOverlayIndex {
        delta_numerator: delta,
        delta_denominator: denominator,
        component_by_node,
    })
}

fn validate_enhanced_scaling_edges(
    graph: &FlowNetwork,
    overlay: &FlowEnhancedCapacityScalingOverlayV1,
    index: &EnhancedScalingOverlayIndex,
) -> Result<(), FlowSceneError> {
    let threshold_numerator = index
        .delta_numerator
        .checked_mul(
            u128::try_from(graph.nodes().len())
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?,
        )
        .and_then(|value| value.checked_mul(3))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    for (state, edge) in overlay.edges.iter().zip(graph.edges()) {
        let flow = state
            .virtual_flow
            .numerator
            .parse::<u128>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let flow_denominator = state
            .virtual_flow
            .denominator
            .parse::<u128>()
            .ok()
            .filter(|&value| value > 0)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let reduced = state
            .reduced_cost
            .parse::<i128>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let internal = index.component_by_node[graph
            .node(edge.from())
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?
            .id()
            .as_str()]
            == index.component_by_node[graph
                .node(edge.to())
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?
                .id()
                .as_str()];
        let strongly_feasible = !internal
            && flow
                .checked_mul(index.delta_denominator)
                .zip(threshold_numerator.checked_mul(flow_denominator))
                .is_some_and(|(left, right)| left >= right);
        if state.internal != internal
            || state.tight != (reduced == 0)
            || state.strongly_feasible != strongly_feasible
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    Ok(())
}

struct OrlinMcfOverlayIndex {
    delta_numerator: u128,
    delta_denominator: u128,
    component_by_node: BTreeMap<String, String>,
    potential_by_node: BTreeMap<String, i128>,
    transformed_node_count: usize,
}

fn validate_orlin_mcf_overlay(
    graph: &FlowNetwork,
    overlay: &FlowOrlinMcfOverlayV1,
) -> Result<(), FlowSceneError> {
    let variable_edges = graph
        .edges()
        .iter()
        .filter(|edge| edge.capacity() > edge.lower())
        .collect::<Vec<_>>();
    if overlay.nodes.len() != graph.nodes().len() + variable_edges.len()
        || overlay.arcs.len() != variable_edges.len().saturating_mul(2)
        || overlay.phase.parse::<u64>().is_err()
        || overlay.eliminated_capacity_nodes.parse::<u64>().is_err()
        || overlay.shortcut_arcs.parse::<u64>().is_err()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    for (state, node) in overlay.nodes.iter().zip(graph.nodes()) {
        if state.node_id != node.id().as_str()
            || state.kind != FlowOrlinMcfNodeKindV1::Original
            || state.capacity_edge_id.is_some()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    for (state, edge) in overlay.nodes[graph.nodes().len()..]
        .iter()
        .zip(&variable_edges)
    {
        if state.node_id != format!("capacity:{}", edge.id().as_str())
            || state.kind != FlowOrlinMcfNodeKindV1::Capacity
            || state.capacity_edge_id.as_deref() != Some(edge.id().as_str())
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    let index = validate_orlin_mcf_components(overlay)?;
    validate_orlin_mcf_arcs(graph, overlay, &variable_edges, &index)?;
    validate_orlin_mcf_path(graph, overlay, &variable_edges, &index)?;
    validate_orlin_mcf_inspection(graph, overlay, &variable_edges)?;
    let augmentation_valid = overlay.augmentation.as_ref().is_none_or(|value| {
        overlay.stage == FlowOrlinMcfStageV1::Augment
            && equal_unsigned_rationals(value, &overlay.delta)
    });
    let contraction_valid = overlay.contraction_arc.as_ref().is_none_or(|arc| {
        overlay.stage == FlowOrlinMcfStageV1::Contract
            && arc.direction == "forward"
            && orlin_mcf_arc_edge(&variable_edges, arc).is_some()
    });
    if !augmentation_valid || !contraction_valid {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

fn validate_orlin_mcf_inspection(
    graph: &FlowNetwork,
    overlay: &FlowOrlinMcfOverlayV1,
    variable_edges: &[&FlowEdge],
) -> Result<(), FlowSceneError> {
    let inspection_stage = matches!(
        overlay.stage,
        FlowOrlinMcfStageV1::InspectContractibleArc
            | FlowOrlinMcfStageV1::InspectReachabilityArc
            | FlowOrlinMcfStageV1::InspectCompressedResidualArc
            | FlowOrlinMcfStageV1::InspectCompressedArc
    );
    let serial_valid = overlay.inspection_serial.as_ref().is_some_and(|serial| {
        serial
            .parse::<u128>()
            .ok()
            .is_some_and(|value| value > 0 && value.to_string() == *serial)
    });
    let segment_valid = (1..=2).contains(&overlay.inspected_segment.len())
        && overlay
            .inspected_segment
            .iter()
            .all(|arc| orlin_mcf_arc_endpoints(graph, variable_edges, arc).is_some());
    if inspection_stage != (serial_valid && segment_valid)
        || (!inspection_stage
            && (!overlay.inspected_segment.is_empty() || overlay.inspection_serial.is_some()))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

fn validate_orlin_mcf_components(
    overlay: &FlowOrlinMcfOverlayV1,
) -> Result<OrlinMcfOverlayIndex, FlowSceneError> {
    let complete_distance_stage = matches!(
        overlay.stage,
        FlowOrlinMcfStageV1::SelectCompressedPath | FlowOrlinMcfStageV1::Augment
    );
    let distance_inspection_stage = overlay.stage == FlowOrlinMcfStageV1::InspectCompressedArc;
    let delta_denominator = overlay
        .delta
        .denominator
        .parse::<u128>()
        .ok()
        .filter(|&value| value > 0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let delta_numerator = overlay
        .delta
        .numerator
        .parse::<u128>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let (component_by_node, component_ids) = validate_orlin_mcf_component_partition(overlay)?;
    let potential_by_node = validate_orlin_mcf_node_metadata(
        overlay,
        &component_by_node,
        &component_ids,
        complete_distance_stage,
        distance_inspection_stage,
    )?;
    Ok(OrlinMcfOverlayIndex {
        delta_numerator,
        delta_denominator,
        component_by_node,
        potential_by_node,
        transformed_node_count: overlay.nodes.len(),
    })
}

fn validate_orlin_mcf_component_partition(
    overlay: &FlowOrlinMcfOverlayV1,
) -> Result<(BTreeMap<String, String>, BTreeSet<String>), FlowSceneError> {
    let rank = overlay
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.node_id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    if rank.len() != overlay.nodes.len() {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mut component_by_node = BTreeMap::new();
    let mut component_ids = BTreeSet::new();
    let mut previous_component_rank = None;
    for component in &overlay.components {
        let first_rank = component
            .members
            .first()
            .and_then(|member| rank.get(member))
            .copied()
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if component.component_id != component.members[0]
            || previous_component_rank.is_some_and(|previous| previous >= first_rank)
            || !component_ids.insert(component.component_id.clone())
            || component.excess.numerator.parse::<i128>().is_err()
            || component
                .excess
                .denominator
                .parse::<u128>()
                .ok()
                .is_none_or(|value| value == 0)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        previous_component_rank = Some(first_rank);
        let mut previous_member_rank = None;
        for member in &component.members {
            let member_rank = rank
                .get(member)
                .copied()
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            if previous_member_rank.is_some_and(|previous| previous >= member_rank)
                || component_by_node
                    .insert(member.clone(), component.component_id.clone())
                    .is_some()
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            previous_member_rank = Some(member_rank);
        }
    }
    Ok((component_by_node, component_ids))
}

fn validate_orlin_mcf_node_metadata(
    overlay: &FlowOrlinMcfOverlayV1,
    component_by_node: &BTreeMap<String, String>,
    component_ids: &BTreeSet<String>,
    complete_distance_stage: bool,
    distance_inspection_stage: bool,
) -> Result<BTreeMap<String, i128>, FlowSceneError> {
    let mut potential_by_node = BTreeMap::new();
    let distance_shape_invalid = if complete_distance_stage {
        overlay.nodes.iter().any(|state| state.distance.is_none())
    } else if distance_inspection_stage {
        overlay.nodes.iter().all(|state| state.distance.is_none())
    } else {
        overlay.nodes.iter().any(|state| state.distance.is_some())
    };
    if component_by_node.len() != overlay.nodes.len()
        || distance_shape_invalid
        || overlay.nodes.iter().any(|state| {
            component_by_node.get(&state.node_id) != Some(&state.component_id)
                || state
                    .potential
                    .parse::<i128>()
                    .ok()
                    .is_none_or(|potential| {
                        potential_by_node
                            .insert(state.node_id.clone(), potential)
                            .is_some()
                    })
                || state
                    .distance
                    .as_ref()
                    .is_some_and(|distance| distance.parse::<i128>().is_err())
        })
        || overlay
            .source_component
            .as_ref()
            .is_some_and(|component| !component_ids.contains(component))
        || overlay
            .sink_component
            .as_ref()
            .is_some_and(|component| !component_ids.contains(component))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(potential_by_node)
}

fn validate_orlin_mcf_arcs(
    graph: &FlowNetwork,
    overlay: &FlowOrlinMcfOverlayV1,
    variable_edges: &[&FlowEdge],
    index: &OrlinMcfOverlayIndex,
) -> Result<(), FlowSceneError> {
    let threshold = index
        .delta_numerator
        .checked_mul(
            u128::try_from(index.transformed_node_count)
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?,
        )
        .and_then(|value| value.checked_mul(3))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    for (edge, states) in variable_edges.iter().zip(overlay.arcs.chunks_exact(2)) {
        for (state, branch) in states
            .iter()
            .zip([FlowOrlinMcfBranchV1::Flow, FlowOrlinMcfBranchV1::Slack])
        {
            if state.edge_id != edge.id().as_str() || state.branch != branch {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            let original = graph
                .node(match branch {
                    FlowOrlinMcfBranchV1::Flow => edge.from(),
                    FlowOrlinMcfBranchV1::Slack => edge.to(),
                })
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?
                .id()
                .as_str();
            let capacity = format!("capacity:{}", edge.id().as_str());
            let flow = state
                .flow
                .numerator
                .parse::<u128>()
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
            let flow_denominator = state
                .flow
                .denominator
                .parse::<u128>()
                .ok()
                .filter(|&value| value > 0)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let reduced = state
                .reduced_cost
                .parse::<i128>()
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
            let expected_reduced = match branch {
                FlowOrlinMcfBranchV1::Flow => i128::from(edge.cost()),
                FlowOrlinMcfBranchV1::Slack => 0,
            }
            .checked_add(index.potential_by_node[original])
            .and_then(|value| value.checked_sub(index.potential_by_node[&capacity]))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let internal = index.component_by_node[original] == index.component_by_node[&capacity];
            let strongly_feasible = !internal
                && flow
                    .checked_mul(index.delta_denominator)
                    .zip(threshold.checked_mul(flow_denominator))
                    .is_some_and(|(left, right)| left >= right);
            if reduced != expected_reduced
                || state.internal != internal
                || state.tight != (reduced == 0)
                || state.strongly_feasible != strongly_feasible
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
        }
    }
    Ok(())
}

fn orlin_mcf_arc_edge<'a>(
    variable_edges: &'a [&FlowEdge],
    arc: &FlowOrlinMcfArcRefV1,
) -> Option<&'a FlowEdge> {
    variable_edges
        .iter()
        .copied()
        .find(|edge| edge.id().as_str() == arc.edge_id)
        .filter(|_| matches!(arc.direction.as_str(), "forward" | "reverse"))
}

fn orlin_mcf_arc_endpoints(
    graph: &FlowNetwork,
    variable_edges: &[&FlowEdge],
    arc: &FlowOrlinMcfArcRefV1,
) -> Option<(String, String)> {
    let edge = orlin_mcf_arc_edge(variable_edges, arc)?;
    let original = graph
        .node(match arc.branch {
            FlowOrlinMcfBranchV1::Flow => edge.from(),
            FlowOrlinMcfBranchV1::Slack => edge.to(),
        })?
        .id()
        .as_str()
        .to_owned();
    let capacity = format!("capacity:{}", edge.id().as_str());
    Some(match arc.direction.as_str() {
        "forward" => (original, capacity),
        "reverse" => (capacity, original),
        _ => return None,
    })
}

fn validate_orlin_mcf_path(
    graph: &FlowNetwork,
    overlay: &FlowOrlinMcfOverlayV1,
    variable_edges: &[&FlowEdge],
    index: &OrlinMcfOverlayIndex,
) -> Result<(), FlowSceneError> {
    if overlay.path.is_empty() {
        return if overlay.source_component.is_none() && overlay.sink_component.is_none() {
            Ok(())
        } else {
            Err(FlowSceneError::SnapshotGraphMismatch)
        };
    }
    if !matches!(
        overlay.stage,
        FlowOrlinMcfStageV1::SelectCompressedPath | FlowOrlinMcfStageV1::Augment
    ) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mut component = overlay
        .source_component
        .clone()
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    for arc in &overlay.path {
        let (from, to) = orlin_mcf_arc_endpoints(graph, variable_edges, arc)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if index.component_by_node[&from] != component {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        component.clone_from(&index.component_by_node[&to]);
    }
    if overlay.sink_component.as_ref() != Some(&component) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_electrical_flow_overlay(
    graph: &FlowNetwork,
    source: crate::model::NodeIndex,
    sink: crate::model::NodeIndex,
    overlay: &FlowElectricalFlowOverlayV1,
) -> Result<(), FlowSceneError> {
    if overlay.target_current != "1"
        || overlay.relative_tolerance
            != crate::algorithms::ELECTRICAL_FLOW_RELATIVE_TOLERANCE.to_string()
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
        || source == sink
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let iteration = overlay
        .iteration
        .parse::<u64>()
        .ok()
        .filter(|value| value.to_string() == overlay.iteration)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let residual_l2 = electrical_f64(&overlay.residual_l2)
        .filter(|value| *value >= 0.0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let effective_resistance = electrical_f64(&overlay.effective_resistance)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let total_energy = electrical_f64(&overlay.total_energy)
        .filter(|value| *value >= 0.0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let mut potentials = Vec::with_capacity(overlay.nodes.len());
    let mut grounded_count = 0_usize;
    for (index, node) in overlay.nodes.iter().enumerate() {
        if node.node_id != graph.nodes()[index].id().as_str()
            || node.grounded != (index == sink.as_usize())
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        grounded_count += usize::from(node.grounded);
        let potential =
            electrical_f64(&node.potential).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let residual =
            electrical_f64(&node.residual).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let direction =
            electrical_f64(&node.search_direction).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if node.grounded && (potential != 0.0 || residual != 0.0 || direction != 0.0) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        potentials.push(potential);
    }
    if grounded_count != 1 {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    let recovered = matches!(
        overlay.stage,
        FlowElectricalFlowStageV1::RecoverCurrents
            | FlowElectricalFlowStageV1::CheckExactReference
            | FlowElectricalFlowStageV1::Complete
    );
    let exact_checked = matches!(
        overlay.stage,
        FlowElectricalFlowStageV1::CheckExactReference | FlowElectricalFlowStageV1::Complete
    );
    let mut divergence = vec![0.0_f64; graph.nodes().len()];
    let mut recomputed_energy = 0.0_f64;
    for (index, state) in overlay.edges.iter().enumerate() {
        let edge = &graph.edges()[index];
        let conductance = u128::from(edge.capacity()) * u128::from(edge.capacity());
        if state.edge_id != edge.id().as_str()
            || state.conductance != conductance.to_string()
            || parse_scene_rational(&state.resistance)
                != Some(BigRational::new(BigInt::from(1), BigInt::from(conductance)))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let voltage =
            electrical_f64(&state.voltage_drop).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let current =
            electrical_f64(&state.current).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let congestion = electrical_f64(&state.congestion)
            .filter(|value| *value >= 0.0)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let energy = electrical_f64(&state.energy)
            .filter(|value| *value >= 0.0)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if !recovered {
            if voltage != 0.0 || current != 0.0 || congestion != 0.0 || energy != 0.0 {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            continue;
        }
        let expected_voltage =
            potentials[edge.from().as_usize()] - potentials[edge.to().as_usize()];
        let conductance_f64 = conductance
            .to_f64()
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let expected_current = conductance_f64 * expected_voltage;
        let expected_congestion = current.abs()
            / edge
                .capacity()
                .to_f64()
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let expected_energy = current * current / conductance_f64;
        if !electrical_close(voltage, expected_voltage)
            || !electrical_close(current, expected_current)
            || !electrical_close(congestion, expected_congestion)
            || !electrical_close(energy, expected_energy)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        divergence[edge.from().as_usize()] += current;
        divergence[edge.to().as_usize()] -= current;
        recomputed_energy += energy;
    }
    if recovered {
        for (node, &actual) in divergence.iter().enumerate() {
            let expected = if node == source.as_usize() {
                1.0
            } else if node == sink.as_usize() {
                -1.0
            } else {
                0.0
            };
            if !electrical_close(actual, expected) {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
        }
        if !electrical_close(recomputed_energy, total_energy)
            || !electrical_close(
                potentials[source.as_usize()] - potentials[sink.as_usize()],
                effective_resistance,
            )
            || !electrical_close(total_energy, effective_resistance)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    } else if total_energy != 0.0 || effective_resistance != 0.0 {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if exact_checked {
        let exact = overlay
            .exact_effective_resistance
            .as_ref()
            .and_then(parse_scene_rational)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let exact_f64 = exact
            .to_f64()
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let maximum_error = overlay
            .maximum_absolute_error
            .as_deref()
            .and_then(electrical_f64)
            .filter(|value| *value >= 0.0)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if !electrical_close(exact_f64, effective_resistance)
            || maximum_error > 1.0e-8 * (1.0 + exact_f64.abs())
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    } else if overlay.exact_effective_resistance.is_some()
        || overlay.maximum_absolute_error.is_some()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let pre_iteration = matches!(
        overlay.stage,
        FlowElectricalFlowStageV1::Ready
            | FlowElectricalFlowStageV1::AssembleLaplacian
            | FlowElectricalFlowStageV1::InitializeConjugateGradient
    );
    if overlay.stage == FlowElectricalFlowStageV1::Ready && iteration != 0
        || pre_iteration && overlay.converged
        || recovered && !overlay.converged
        || overlay.converged && residual_l2 > crate::algorithms::ELECTRICAL_FLOW_RELATIVE_TOLERANCE
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_augmenting_electrical_overlay(
    graph: &FlowNetwork,
    source: crate::model::NodeIndex,
    sink: crate::model::NodeIndex,
    overlay: &FlowAugmentingElectricalOverlayV1,
) -> Result<(), FlowSceneError> {
    let canonical_unsigned = |value: &str| {
        value
            .parse::<u64>()
            .ok()
            .filter(|parsed| parsed.to_string() == value)
    };
    let canonical_signed = |value: &str| {
        value
            .parse::<i64>()
            .ok()
            .filter(|parsed| parsed.to_string() == value)
    };
    if source == sink
        || source.as_usize() >= graph.nodes().len()
        || sink.as_usize() >= graph.nodes().len()
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let original_target = canonical_unsigned(&overlay.original_target)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let transformed_target = canonical_unsigned(&overlay.transformed_target)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let working_target =
        canonical_unsigned(&overlay.working_target).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let working_nodes =
        canonical_unsigned(&overlay.working_nodes).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let working_edges =
        canonical_unsigned(&overlay.working_edges).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let active = overlay
        .active_working_edge
        .as_deref()
        .map(|value| canonical_unsigned(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    let active_pivot = overlay
        .active_pivot_node
        .as_deref()
        .map(|value| canonical_unsigned(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    let active_discrete_amount = overlay
        .active_discrete_amount
        .as_deref()
        .map(|value| canonical_unsigned(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    let maximum_working_nodes =
        u64::try_from(crate::algorithms::AUGMENTING_ELECTRICAL_MAX_WORKING_NODES)
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let maximum_working_edges =
        u64::try_from(crate::algorithms::AUGMENTING_ELECTRICAL_MAX_WORKING_EDGES)
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    if active.is_some_and(|edge| edge >= working_edges)
        || active_pivot.is_some_and(|node| node >= working_nodes)
        || active_pivot.is_some()
            && overlay.stage != FlowAugmentingElectricalStageV1::SolveElectricalDirection
        || overlay.active_working_path.iter().any(|arc| {
            canonical_unsigned(&arc.edge).is_none_or(|edge| edge >= working_edges)
                || !matches!(arc.direction.as_str(), "forward" | "reverse")
                || canonical_signed(&arc.flow_after).is_none()
                || !graph
                    .nodes()
                    .iter()
                    .any(|node| node.id().as_str() == arc.from_node)
                || !graph
                    .nodes()
                    .iter()
                    .any(|node| node.id().as_str() == arc.to_node)
        })
        || overlay
            .active_working_path
            .windows(2)
            .any(|pair| pair[0].to_node != pair[1].from_node)
        || overlay
            .active_working_path
            .first()
            .is_some_and(|arc| arc.from_node != graph.nodes()[source.as_usize()].id().as_str())
        || overlay
            .active_working_path
            .last()
            .is_some_and(|arc| arc.to_node != graph.nodes()[sink.as_usize()].id().as_str())
        || overlay.active_extraction_cycle.iter().any(|arc| {
            canonical_unsigned(&arc.edge)
                .and_then(|edge| usize::try_from(edge).ok())
                .is_none_or(|edge| edge >= graph.edges().len())
        })
        || (!overlay.active_working_path.is_empty() && !overlay.active_extraction_cycle.is_empty())
        || (overlay.active_working_path.is_empty() && overlay.active_extraction_cycle.is_empty())
            != active_discrete_amount.is_none()
        || !overlay.active_working_path.is_empty()
            && overlay.stage != FlowAugmentingElectricalStageV1::CleanupAugmentingPath
        || !overlay.active_extraction_cycle.is_empty()
            && overlay.stage != FlowAugmentingElectricalStageV1::CancelExtractionCycle
        || active_discrete_amount.is_some_and(|amount| amount == 0)
        || working_nodes > maximum_working_nodes
        || working_edges > maximum_working_edges
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let alpha = electrical_f64(&overlay.alpha)
        .filter(|value| (0.0..=1.0 + 1.0e-8).contains(value))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let current = electrical_f64(&overlay.current_value)
        .filter(|value| *value >= 0.0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let remaining = electrical_f64(&overlay.remaining)
        .filter(|value| *value >= 0.0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    for scalar in [
        &overlay.electrical_energy,
        &overlay.congestion_l3,
        &overlay.congestion_l4,
        &overlay.coupling_l2,
    ] {
        if electrical_f64(scalar).is_none_or(|value| value < 0.0) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    let mut side = vec![false; graph.nodes().len()];
    for (index, node) in overlay.nodes.iter().enumerate() {
        if node.node_id != graph.nodes()[index].id().as_str()
            || electrical_f64(&node.potential).is_none()
            || electrical_f64(&node.coupling_violation).is_none_or(|value| value < 0.0)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        side[index] = node.target_source_side;
    }
    let cut_installed = !matches!(
        overlay.stage,
        FlowAugmentingElectricalStageV1::Ready
            | FlowAugmentingElectricalStageV1::BuildDirectedReduction
            | FlowAugmentingElectricalStageV1::AddPreconditioning
    );
    if cut_installed {
        if !side[source.as_usize()] || side[sink.as_usize()] {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let cut = graph.edges().iter().try_fold(0_u64, |sum, edge| {
            if side[edge.from().as_usize()] && !side[edge.to().as_usize()] {
                sum.checked_add(edge.capacity())
            } else {
                Some(sum)
            }
        });
        if cut != Some(original_target) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    } else if side.iter().any(|&source_side| source_side) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let capacity_sum =
        u64::try_from(graph.capacity_sum()).map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let expected_transformed = capacity_sum
        .checked_add(
            original_target
                .checked_mul(2)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
        )
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let maximum_capacity = graph
        .edges()
        .iter()
        .map(FlowEdge::capacity)
        .max()
        .unwrap_or(0);
    let expected_working = expected_transformed
        .checked_add(
            u64::try_from(graph.edges().len())
                .ok()
                .and_then(|edges| edges.checked_mul(6))
                .and_then(|preconditioners| preconditioners.checked_mul(maximum_capacity))
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
        )
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let ready = overlay.stage == FlowAugmentingElectricalStageV1::Ready;
    let reduction_only = overlay.stage == FlowAugmentingElectricalStageV1::BuildDirectedReduction;
    let working_target_f64 = working_target
        .to_f64()
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    if ready && (transformed_target != 0 || working_target != 0)
        || reduction_only && (transformed_target != expected_transformed || working_target != 0)
        || !ready
            && !reduction_only
            && (transformed_target != expected_transformed || working_target != expected_working)
        || working_target > 0
            && (!electrical_close(current + remaining, working_target_f64)
                || !electrical_close(current, alpha * working_target_f64))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let terminal_flows = matches!(
        overlay.stage,
        FlowAugmentingElectricalStageV1::RoundDirectedFlow
            | FlowAugmentingElectricalStageV1::CheckCertificate
            | FlowAugmentingElectricalStageV1::Optimal
    );
    let rounded_working_flows = matches!(
        overlay.stage,
        FlowAugmentingElectricalStageV1::RoundCentralFlow
            | FlowAugmentingElectricalStageV1::CleanupAugmentingPath
            | FlowAugmentingElectricalStageV1::ExtractDirectedFlow
            | FlowAugmentingElectricalStageV1::CancelExtractionCycle
            | FlowAugmentingElectricalStageV1::RoundDirectedFlow
            | FlowAugmentingElectricalStageV1::CheckCertificate
            | FlowAugmentingElectricalStageV1::Optimal
    );
    let extraction_visible = matches!(
        overlay.stage,
        FlowAugmentingElectricalStageV1::ExtractDirectedFlow
            | FlowAugmentingElectricalStageV1::CancelExtractionCycle
            | FlowAugmentingElectricalStageV1::RoundDirectedFlow
            | FlowAugmentingElectricalStageV1::CheckCertificate
            | FlowAugmentingElectricalStageV1::Optimal
    );
    for (index, edge_state) in overlay.edges.iter().enumerate() {
        let edge = &graph.edges()[index];
        let boost_segments = canonical_unsigned(&edge_state.boost_segments)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if edge_state.edge_id != edge.id().as_str()
            || electrical_f64(&edge_state.central_flow).is_none()
            || electrical_f64(&edge_state.electrical_current).is_none()
            || electrical_f64(&edge_state.forward_residual).is_none_or(|value| value < 0.0)
            || electrical_f64(&edge_state.backward_residual).is_none_or(|value| value < 0.0)
            || electrical_f64(&edge_state.congestion).is_none_or(|value| value < 0.0)
            || electrical_f64(&edge_state.resistance).is_none_or(|value| value < 0.0)
            || ready && boost_segments != 0
            || !ready && boost_segments == 0
            || rounded_working_flows != edge_state.rounded_central_flow.is_some()
            || edge_state
                .rounded_central_flow
                .as_deref()
                .is_some_and(|value| canonical_signed(value).is_none())
            || extraction_visible
                != (edge_state.extraction_central_scaled.is_some()
                    && edge_state.extraction_toward_source.is_some()
                    && edge_state.extraction_out_of_sink.is_some())
            || [
                edge_state.extraction_central_scaled.as_deref(),
                edge_state.extraction_toward_source.as_deref(),
                edge_state.extraction_out_of_sink.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|value| canonical_unsigned(value).is_none())
            || terminal_flows != edge_state.final_flow.is_some()
            || edge_state.final_flow.as_deref().is_some_and(|value| {
                canonical_unsigned(value).is_none_or(|flow| flow > edge.capacity())
            })
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_interior_point_max_flow_overlay(
    graph: &FlowNetwork,
    source: crate::model::NodeIndex,
    sink: crate::model::NodeIndex,
    flows: &[u64],
    overlay: &FlowInteriorPointMaxFlowOverlayV1,
) -> Result<(), FlowSceneError> {
    let canonical_u64 = |value: &str| {
        value
            .parse::<u64>()
            .ok()
            .filter(|parsed| parsed.to_string() == value)
    };
    if source == sink
        || source.as_usize() >= graph.nodes().len()
        || sink.as_usize() >= graph.nodes().len()
        || graph.nodes().len() > crate::algorithms::INTERIOR_POINT_MAX_FLOW_MAX_NODES
        || graph.edges().len() > crate::algorithms::INTERIOR_POINT_MAX_FLOW_MAX_EDGES
        || graph.nodes().iter().any(|node| node.supply() != 0)
        || graph.edges().iter().any(|edge| {
            edge.lower() != 0
                || edge.capacity() != 1
                || edge.cost() != 0
                || edge.from() == edge.to()
        })
        || flows.len() != graph.edges().len()
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let target =
        canonical_u64(&overlay.target_value).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let b_nodes =
        canonical_u64(&overlay.b_matching_nodes).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let b_edges =
        canonical_u64(&overlay.b_matching_edges).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let working_nodes =
        canonical_u64(&overlay.working_nodes).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let working_edges =
        canonical_u64(&overlay.working_edges).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let active = overlay
        .active_working_edge
        .as_deref()
        .map(|value| canonical_u64(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    if active.is_some_and(|edge| edge >= working_edges)
        || working_nodes
            > u64::try_from(crate::algorithms::INTERIOR_POINT_MAX_FLOW_MAX_WORKING_NODES)
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?
        || working_edges
            > u64::try_from(crate::algorithms::INTERIOR_POINT_MAX_FLOW_MAX_WORKING_EDGES)
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mu = electrical_f64(&overlay.mu)
        .filter(|value| *value >= 0.0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let gap = electrical_f64(&overlay.duality_gap)
        .filter(|value| *value >= 0.0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let centrality = electrical_f64(&overlay.centrality)
        .filter(|value| *value >= 0.0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let congestion_l4 = electrical_f64(&overlay.congestion_l4)
        .filter(|value| *value >= 0.0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let step_size = electrical_f64(&overlay.step_size)
        .filter(|value| (0.0..=0.5 + 1.0e-8).contains(value))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let energy = electrical_f64(&overlay.electrical_energy)
        .filter(|value| *value >= 0.0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let _ = (congestion_l4, step_size, energy);

    let ready = overlay.stage == FlowInteriorPointMaxFlowStageV1::Ready;
    let target_installed = !ready;
    let b_built = !matches!(
        overlay.stage,
        FlowInteriorPointMaxFlowStageV1::Ready
            | FlowInteriorPointMaxFlowStageV1::EnumerateTargetCut
    );
    let working_built = !matches!(
        overlay.stage,
        FlowInteriorPointMaxFlowStageV1::Ready
            | FlowInteriorPointMaxFlowStageV1::EnumerateTargetCut
            | FlowInteriorPointMaxFlowStageV1::BuildBMatchingReduction
    );
    let initialized = !matches!(
        overlay.stage,
        FlowInteriorPointMaxFlowStageV1::Ready
            | FlowInteriorPointMaxFlowStageV1::EnumerateTargetCut
            | FlowInteriorPointMaxFlowStageV1::BuildBMatchingReduction
            | FlowInteriorPointMaxFlowStageV1::BuildMinCostReduction
    );
    let terminal_flows = matches!(
        overlay.stage,
        FlowInteriorPointMaxFlowStageV1::RoundIntegralFlow
            | FlowInteriorPointMaxFlowStageV1::CheckCertificate
            | FlowInteriorPointMaxFlowStageV1::Optimal
    );
    let relevant = graph
        .edges()
        .iter()
        .filter(|edge| edge.to() != source && edge.from() != sink)
        .count();
    let expected_b_nodes = relevant
        .checked_mul(2)
        .and_then(|value| value.checked_add(graph.nodes().len().saturating_sub(2) * 2))
        .and_then(|value| value.checked_add(2))
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let expected_b_edges = relevant
        .checked_mul(3)
        .and_then(|value| value.checked_add(graph.nodes().len().saturating_sub(2)))
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let mut normalized_indegree = vec![0_u64; graph.nodes().len()];
    let mut normalized_outdegree = vec![0_u64; graph.nodes().len()];
    for edge in graph
        .edges()
        .iter()
        .filter(|edge| edge.to() != source && edge.from() != sink)
    {
        normalized_outdegree[edge.from().as_usize()] += 1;
        normalized_indegree[edge.to().as_usize()] += 1;
    }
    let source_demand = normalized_outdegree[source.as_usize()]
        .checked_sub(target)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let sink_demand = normalized_indegree[sink.as_usize()]
        .checked_sub(target)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let mut expected_direct_arcs = 0_u64;
    for edge in graph
        .edges()
        .iter()
        .filter(|edge| edge.to() != source && edge.from() != sink)
    {
        let tail_demand = if edge.from() == source {
            source_demand
        } else {
            normalized_outdegree[edge.from().as_usize()]
        };
        let head_demand = if edge.to() == sink {
            sink_demand
        } else {
            normalized_indegree[edge.to().as_usize()]
        };
        expected_direct_arcs = expected_direct_arcs
            .checked_add(1 + u64::from(tail_demand > 0) + u64::from(head_demand > 0))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    }
    for node in graph.node_indices() {
        if node != source && node != sink {
            expected_direct_arcs = expected_direct_arcs
                .checked_add(
                    normalized_indegree[node.as_usize()].min(normalized_outdegree[node.as_usize()]),
                )
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        }
    }
    let expected_working_edges = expected_b_nodes
        .checked_mul(2)
        .and_then(|value| value.checked_add(expected_direct_arcs))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    if ready && target != 0
        || !b_built && (b_nodes != 0 || b_edges != 0)
        || b_built && (b_nodes != expected_b_nodes || b_edges != expected_b_edges)
        || !working_built && (working_nodes != 0 || working_edges != 0)
        || working_built
            && (working_nodes != expected_b_nodes + 1 || working_edges != expected_working_edges)
        || !initialized && (mu != 0.0 || gap != 0.0 || centrality != 0.0)
        || initialized && (mu <= 0.0 || gap <= 0.0)
        || matches!(
            overlay.stage,
            FlowInteriorPointMaxFlowStageV1::DescentStep
                | FlowInteriorPointMaxFlowStageV1::SolveCenteringDirection
        ) && centrality > 3.0 / 400.0 + 1.0e-6
        || initialized
            && !matches!(
                overlay.stage,
                FlowInteriorPointMaxFlowStageV1::DescentStep
                    | FlowInteriorPointMaxFlowStageV1::SolveCenteringDirection
            )
            && centrality > 1.0 / 400.0 + 1.0e-6
        || matches!(
            overlay.stage,
            FlowInteriorPointMaxFlowStageV1::ExtractFractionalFlow
                | FlowInteriorPointMaxFlowStageV1::RoundIntegralFlow
                | FlowInteriorPointMaxFlowStageV1::CheckCertificate
                | FlowInteriorPointMaxFlowStageV1::Optimal
        ) && gap > 0.5 + 1.0e-6
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    let mut side = vec![false; graph.nodes().len()];
    for (index, node) in overlay.nodes.iter().enumerate() {
        if node.node_id != graph.nodes()[index].id().as_str()
            || electrical_f64(&node.potential).is_none()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        side[index] = node.target_source_side;
    }
    if target_installed {
        if !side[source.as_usize()] || side[sink.as_usize()] {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let cut = graph.edges().iter().try_fold(0_u64, |sum, edge| {
            if side[edge.from().as_usize()] && !side[edge.to().as_usize()] {
                sum.checked_add(edge.capacity())
            } else {
                Some(sum)
            }
        });
        if cut != Some(target) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    } else if side.iter().any(|value| *value) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    for (index, edge_state) in overlay.edges.iter().enumerate() {
        let edge = &graph.edges()[index];
        let normalized_away = edge.to() == source || edge.from() == sink;
        let fractional = electrical_f64(&edge_state.fractional_flow).filter(|value| *value >= 0.0);
        let current = electrical_f64(&edge_state.electrical_current);
        let slack = electrical_f64(&edge_state.slack).filter(|value| *value >= 0.0);
        let measure = electrical_f64(&edge_state.measure).filter(|value| *value >= 0.0);
        let resistance = electrical_f64(&edge_state.resistance).filter(|value| *value >= 0.0);
        let congestion = electrical_f64(&edge_state.congestion).filter(|value| *value >= 0.0);
        if edge_state.edge_id != edge.id().as_str()
            || edge_state.normalized_away != normalized_away
            || fractional.is_none()
            || current.is_none()
            || slack.is_none()
            || measure.is_none()
            || resistance.is_none()
            || congestion.is_none()
            || !working_built
                && [fractional, slack, measure, resistance]
                    .into_iter()
                    .flatten()
                    .any(|value| value != 0.0)
            || working_built
                && !normalized_away
                && [fractional, slack, measure, resistance]
                    .into_iter()
                    .flatten()
                    .any(|value| value <= 0.0)
            || terminal_flows != edge_state.final_flow.is_some()
            || edge_state
                .final_flow
                .as_deref()
                .is_some_and(|value| canonical_u64(value).is_none_or(|flow| flow > edge.capacity()))
            || flows[index]
                != edge_state
                    .final_flow
                    .as_deref()
                    .and_then(canonical_u64)
                    .unwrap_or(0)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    Ok(())
}

fn scene_canonical_integer<T>(value: &str) -> Option<T>
where
    T: std::str::FromStr + ToString,
{
    value
        .parse::<T>()
        .ok()
        .filter(|parsed| parsed.to_string() == value)
}

fn scene_canonical_bigint(value: &str) -> Option<BigInt> {
    value
        .parse::<BigInt>()
        .ok()
        .filter(|parsed| parsed.to_string() == value)
}

#[allow(clippy::too_many_lines)]
fn validate_minimum_ratio_cycle_overlay(
    graph: &FlowNetwork,
    overlay: &FlowMinimumRatioCycleOverlayV1,
) -> Result<(), FlowSceneError> {
    if graph.nodes().len() < 2
        || graph.nodes().len() > crate::algorithms::MINIMUM_RATIO_CYCLE_MAX_NODES
        || graph.edges().is_empty()
        || graph.edges().len() > crate::algorithms::MINIMUM_RATIO_CYCLE_MAX_EDGES
        || graph.nodes().iter().any(|node| node.supply() != 0)
        || graph.edges().iter().any(|edge| {
            edge.lower() != 0
                || edge.capacity() == 0
                || edge.capacity() > crate::algorithms::MINIMUM_RATIO_CYCLE_MAX_LENGTH
                || edge.cost().unsigned_abs()
                    > u64::try_from(crate::algorithms::MINIMUM_RATIO_CYCLE_MAX_ABS_GRADIENT)
                        .expect("positive bounded constant")
                || edge.from() == edge.to()
        })
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let selected_edge_count = scene_canonical_integer::<u64>(&overlay.selected_edge_count)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let maximum_balance = scene_canonical_integer::<u64>(&overlay.maximum_absolute_balance)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let enumerated = scene_canonical_integer::<u64>(&overlay.enumerated_vectors)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let simple_cycles = scene_canonical_integer::<u64>(&overlay.simple_cycles)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let fundamental_cycles = scene_canonical_integer::<u64>(&overlay.fundamental_cycles)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let vector_count = (0..graph.edges().len()).try_fold(1_u64, |value, _| {
        value
            .checked_mul(3)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)
    })?;
    if enumerated >= vector_count
        || simple_cycles > enumerated
        || selected_edge_count
            > u64::try_from(graph.edges().len())
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    let forest_built = !matches!(
        overlay.stage,
        FlowMinimumRatioCycleStageV1::Ready | FlowMinimumRatioCycleStageV1::MapGradientLength
    );
    let terminal = overlay.stage == FlowMinimumRatioCycleStageV1::Complete;
    let candidate_visible = matches!(
        overlay.stage,
        FlowMinimumRatioCycleStageV1::EvaluateCycle | FlowMinimumRatioCycleStageV1::UpdateBest
    );
    let vector_inspection = overlay.stage == FlowMinimumRatioCycleStageV1::InspectVector;
    if terminal && enumerated != vector_count - 1
        || !candidate_visible && overlay.candidate_ratio.is_some()
        || candidate_visible && overlay.candidate_ratio.is_none()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    let forest = scene_minimum_ratio_forest(graph)?;
    if forest_built
        && fundamental_cycles
            != u64::try_from(forest.fundamental_cycles)
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?
        || !forest_built && fundamental_cycles != 0
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    let mut candidate_signs = Vec::with_capacity(graph.edges().len());
    let mut selected_signs = Vec::with_capacity(graph.edges().len());
    let mut candidate_balances = vec![0_i32; graph.nodes().len()];
    let mut candidate_incident = vec![false; graph.nodes().len()];
    let mut selected_incident = vec![false; graph.nodes().len()];
    for (index, state) in overlay.edges.iter().enumerate() {
        let edge = &graph.edges()[index];
        let gradient = scene_canonical_integer::<i64>(&state.gradient)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let length = scene_canonical_integer::<u64>(&state.length)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let candidate_sign = scene_canonical_integer::<i64>(&state.candidate_sign)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let selected_sign = scene_canonical_integer::<i64>(&state.selected_sign)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let numerator = scene_canonical_integer::<i128>(&state.numerator_contribution)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let denominator = scene_canonical_integer::<u128>(&state.denominator_contribution)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if state.edge_id != edge.id().as_str()
            || gradient != edge.cost()
            || length != edge.capacity()
            || !matches!(candidate_sign, -1..=1)
            || !matches!(selected_sign, -1..=1)
            || !forest_built && state.tree_edge
            || forest_built && state.tree_edge != forest.tree_edges[index]
            || numerator != i128::from(edge.cost()) * i128::from(candidate_sign)
            || denominator
                != if candidate_sign == 0 {
                    0
                } else {
                    u128::from(edge.capacity())
                }
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let candidate_sign =
            i8::try_from(candidate_sign).map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let selected_sign =
            i8::try_from(selected_sign).map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        candidate_signs.push(candidate_sign);
        selected_signs.push(selected_sign);
        candidate_balances[edge.from().as_usize()] -= i32::from(candidate_sign);
        candidate_balances[edge.to().as_usize()] += i32::from(candidate_sign);
        if candidate_sign != 0 {
            candidate_incident[edge.from().as_usize()] = true;
            candidate_incident[edge.to().as_usize()] = true;
        }
        if selected_sign != 0 {
            selected_incident[edge.from().as_usize()] = true;
            selected_incident[edge.to().as_usize()] = true;
        }
    }
    let computed_maximum_balance = candidate_balances
        .iter()
        .map(|value| u64::from(value.unsigned_abs()))
        .max()
        .unwrap_or(0);
    if computed_maximum_balance != maximum_balance
        || selected_signs.iter().filter(|&&sign| sign != 0).count()
            != usize::try_from(selected_edge_count)
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    for (index, state) in overlay.nodes.iter().enumerate() {
        let component = scene_canonical_integer::<u64>(&state.component)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let depth = scene_canonical_integer::<u64>(&state.depth)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let balance = state
            .candidate_balance
            .parse::<i32>()
            .ok()
            .filter(|value| value.to_string() == state.candidate_balance)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let expected_parent =
            forest.parents[index].map(|parent| graph.nodes()[parent].id().as_str().to_owned());
        if state.node_id != graph.nodes()[index].id().as_str()
            || component
                != u64::try_from(if forest_built {
                    forest.components[index]
                } else {
                    index
                })
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?
            || state.parent_node_id != if forest_built { expected_parent } else { None }
            || depth
                != u64::try_from(if forest_built {
                    forest.depths[index]
                } else {
                    0
                })
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?
            || balance != candidate_balances[index]
            || state.on_candidate != candidate_incident[index]
            || state.on_selected != selected_incident[index]
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }

    let candidate_ratio = if vector_inspection {
        None
    } else {
        scene_minimum_ratio_cycle_value(graph, &candidate_signs)?
    };
    let selected_ratio = scene_minimum_ratio_cycle_value(graph, &selected_signs)?;
    let parsed_candidate = overlay
        .candidate_ratio
        .as_ref()
        .map(|ratio| parse_scene_rational(ratio).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    let parsed_best = overlay
        .best_ratio
        .as_ref()
        .map(|ratio| parse_scene_rational(ratio).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    if (!vector_inspection && parsed_candidate != candidate_ratio)
        || parsed_best != selected_ratio
        || selected_ratio.is_none() != selected_signs.iter().all(|&sign| sign == 0)
        || matches!(
            overlay.stage,
            FlowMinimumRatioCycleStageV1::VerifyCycleSpace
                | FlowMinimumRatioCycleStageV1::CheckExhaustiveOracle
                | FlowMinimumRatioCycleStageV1::Complete
        ) && (maximum_balance != 0 || candidate_signs.iter().any(|&sign| sign != 0))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

struct SceneMinimumRatioForest {
    tree_edges: Vec<bool>,
    components: Vec<usize>,
    parents: Vec<Option<usize>>,
    depths: Vec<usize>,
    fundamental_cycles: usize,
}

fn scene_minimum_ratio_forest(
    graph: &FlowNetwork,
) -> Result<SceneMinimumRatioForest, FlowSceneError> {
    let node_count = graph.nodes().len();
    let mut union_parent = (0..node_count).collect::<Vec<_>>();
    let mut tree_edges = vec![false; graph.edges().len()];
    for (index, edge) in graph.edges().iter().enumerate() {
        let left = scene_find_root(&mut union_parent, edge.from().as_usize());
        let right = scene_find_root(&mut union_parent, edge.to().as_usize());
        if left != right {
            let (root, child) = if left < right {
                (left, right)
            } else {
                (right, left)
            };
            union_parent[child] = root;
            tree_edges[index] = true;
        }
    }
    let mut adjacency = vec![Vec::<usize>::new(); node_count];
    for (index, edge) in graph.edges().iter().enumerate() {
        if tree_edges[index] {
            adjacency[edge.from().as_usize()].push(edge.to().as_usize());
            adjacency[edge.to().as_usize()].push(edge.from().as_usize());
        }
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
    }
    let mut components = vec![0_usize; node_count];
    let mut parents = vec![None; node_count];
    let mut depths = vec![0_usize; node_count];
    let mut seen = vec![false; node_count];
    let mut component_count = 0_usize;
    for root in 0..node_count {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        components[root] = component_count;
        let mut queue = VecDeque::from([root]);
        while let Some(node) = queue.pop_front() {
            for &next in &adjacency[node] {
                if !seen[next] {
                    seen[next] = true;
                    components[next] = component_count;
                    parents[next] = Some(node);
                    depths[next] = depths[node]
                        .checked_add(1)
                        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
                    queue.push_back(next);
                }
            }
        }
        component_count = component_count
            .checked_add(1)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    }
    let fundamental_cycles = graph
        .edges()
        .len()
        .checked_add(component_count)
        .and_then(|value| value.checked_sub(node_count))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    Ok(SceneMinimumRatioForest {
        tree_edges,
        components,
        parents,
        depths,
        fundamental_cycles,
    })
}

fn scene_find_root(parent: &mut [usize], value: usize) -> usize {
    if parent[value] != value {
        parent[value] = scene_find_root(parent, parent[value]);
    }
    parent[value]
}

fn scene_minimum_ratio_cycle_value(
    graph: &FlowNetwork,
    signs: &[i8],
) -> Result<Option<BigRational>, FlowSceneError> {
    if signs.len() != graph.edges().len() {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if signs.iter().all(|&sign| sign == 0) {
        return Ok(None);
    }
    let mut balances = vec![0_i32; graph.nodes().len()];
    let mut degrees = vec![0_u8; graph.nodes().len()];
    let mut adjacency = vec![Vec::<usize>::new(); graph.nodes().len()];
    let mut numerator = 0_i128;
    let mut denominator = 0_u128;
    for (edge, &sign) in graph.edges().iter().zip(signs) {
        if !matches!(sign, -1..=1) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if sign == 0 {
            continue;
        }
        let from = edge.from().as_usize();
        let to = edge.to().as_usize();
        balances[from] -= i32::from(sign);
        balances[to] += i32::from(sign);
        degrees[from] = degrees[from]
            .checked_add(1)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        degrees[to] = degrees[to]
            .checked_add(1)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        adjacency[from].push(to);
        adjacency[to].push(from);
        numerator = numerator
            .checked_add(i128::from(edge.cost()) * i128::from(sign))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        denominator = denominator
            .checked_add(u128::from(edge.capacity()))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    }
    if numerator > 0
        || balances.iter().any(|&value| value != 0)
        || degrees.iter().any(|&degree| degree != 0 && degree != 2)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let start = degrees
        .iter()
        .position(|&degree| degree != 0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let mut seen = vec![false; graph.nodes().len()];
    seen[start] = true;
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        for &next in &adjacency[node] {
            if !seen[next] {
                seen[next] = true;
                stack.push(next);
            }
        }
    }
    if degrees
        .iter()
        .enumerate()
        .any(|(node, &degree)| degree != 0 && !seen[node])
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(Some(BigRational::new(
        BigInt::from(numerator),
        BigInt::from(denominator),
    )))
}

fn electrical_f64(value: &str) -> Option<f64> {
    let parsed = value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())?;
    (parsed.to_string() == value).then_some(parsed)
}

fn electrical_close(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1.0e-8 * (1.0 + left.abs().max(right.abs()))
}

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn validate_randomized_almost_linear_overlay(
    graph: &FlowNetwork,
    source: crate::model::NodeIndex,
    sink: crate::model::NodeIndex,
    flows: &[u64],
    overlay: &FlowRandomizedAlmostLinearOverlayV1,
) -> Result<(), FlowSceneError> {
    if source == sink
        || graph.node(source).is_none()
        || graph.node(sink).is_none()
        || graph.nodes().len() < 2
        || graph.nodes().len() > crate::algorithms::RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_NODES
        || graph.edges().is_empty()
        || graph.edges().len() > crate::algorithms::RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_EDGES
        || graph.nodes().iter().any(|node| node.supply() != 0)
        || graph
            .edges()
            .iter()
            .any(|edge| edge.lower() != 0 || edge.capacity() == 0 || edge.from() == edge.to())
        || flows.len() != graph.edges().len()
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let canonical_u64 = |value: &str| scene_canonical_integer::<u64>(value);
    let canonical_u128 = |value: &str| scene_canonical_integer::<u128>(value);
    let canonical_i8 = |value: &str| scene_canonical_integer::<i8>(value);
    let finite = |value: &str| electrical_f64(value);
    let _seed = canonical_u64(&overlay.seed).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let draws =
        canonical_u64(&overlay.random_draws).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let alpha = finite(&overlay.alpha).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let potential = finite(&overlay.potential).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let gap = finite(&overlay.cost_gap).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let forest_pool =
        canonical_u64(&overlay.forest_pool_size).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let sample_count =
        canonical_u64(&overlay.sample_count).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let iteration =
        canonical_u64(&overlay.iteration).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let rebuild_epoch =
        canonical_u64(&overlay.rebuild_epoch).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let return_capacity =
        canonical_u64(&overlay.return_capacity).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let return_flow = finite(&overlay.return_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let return_gradient =
        finite(&overlay.return_gradient).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let return_length =
        finite(&overlay.return_length).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let return_sign = canonical_i8(&overlay.active_return_sign)
        .filter(|sign| matches!(sign, -1..=1))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let return_isolation_draw = canonical_u64(&overlay.return_isolation_draw)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let final_point_return_flow = overlay
        .final_point_return_flow
        .as_deref()
        .map(|value| finite(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    let artificial_edges =
        canonical_u64(&overlay.artificial_edges).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let artificial_flow =
        finite(&overlay.artificial_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let target =
        canonical_u64(&overlay.target_value).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let isolation_scale = canonical_u128(&overlay.isolation_scale)
        .filter(|value| *value > 0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let isolation_attempt =
        canonical_u64(&overlay.isolation_attempt).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let isolation_probability_numerator =
        canonical_u128(&overlay.isolation_failure_probability.numerator)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let isolation_probability_denominator =
        canonical_u128(&overlay.isolation_failure_probability.denominator)
            .filter(|value| *value > 0)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let isolated_objective = overlay
        .isolated_objective
        .as_deref()
        .map(|value| {
            scene_canonical_integer::<i128>(value).ok_or(FlowSceneError::SnapshotGraphMismatch)
        })
        .transpose()?;
    let final_point_threshold = finite(&overlay.final_point_threshold)
        .filter(|value| *value > 0.0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let final_point_gap = overlay
        .final_point_gap
        .as_deref()
        .map(|value| finite(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    let final_point_mix = overlay
        .final_point_mix
        .as_deref()
        .map(|value| finite(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    let probability_numerator = canonical_u128(&overlay.miss_probability.numerator)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let probability_denominator = canonical_u128(&overlay.miss_probability.denominator)
        .filter(|value| *value > 0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let expected_return_capacity = (graph.edges().len() as u64)
        .checked_mul(
            graph
                .edges()
                .iter()
                .map(crate::model::FlowEdge::capacity)
                .max()
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
        )
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let reduction_edges = u128::try_from(graph.edges().len() + 1)
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let congestion_bound = u128::from(expected_return_capacity);
    let expected_isolation_scale = 4_u128
        .checked_mul(reduction_edges)
        .and_then(|value| value.checked_mul(reduction_edges))
        .and_then(|value| value.checked_mul(congestion_bound))
        .and_then(|value| value.checked_mul(congestion_bound))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let threshold_denominator = 12_u128
        .checked_mul(reduction_edges)
        .and_then(|value| value.checked_mul(reduction_edges))
        .and_then(|value| value.checked_mul(reduction_edges))
        .and_then(|value| value.checked_mul(congestion_bound))
        .and_then(|value| value.checked_mul(congestion_bound))
        .and_then(|value| value.checked_mul(congestion_bound))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let expected_failure_denominator = 2_u128
        .checked_pow(
            u32::try_from(isolation_attempt).map_err(|_| FlowSceneError::SnapshotGraphMismatch)?,
        )
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let source_scalars_valid = alpha > 0.0 && alpha < 1.0 && potential.is_finite() && gap > 0.0;
    let return_flow_valid = return_flow > 0.0 && return_flow < return_capacity as f64;
    let return_contract = return_flow_valid
        && return_gradient.is_finite()
        && return_length > 0.0
        && return_capacity == expected_return_capacity;
    let resource_contract = (4..=6).contains(&sample_count)
        && forest_pool <= crate::algorithms::RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_FORESTS as u64;
    let progress_contract = iteration
        <= crate::algorithms::RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_IPM_STEPS
        && rebuild_epoch <= crate::algorithms::RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_REBUILDS + 3;
    let probability_contract = probability_numerator <= probability_denominator
        && isolation_attempt
            <= crate::algorithms::RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_ISOLATION_ATTEMPTS as u64
        && isolation_probability_numerator == 1
        && isolation_probability_denominator == expected_failure_denominator
        && isolation_scale == expected_isolation_scale
        && overlay.final_point_threshold
            == crate::algorithms::stable_scene_decimal(1.0 / threshold_denominator as f64);
    let artificial_contract = artificial_flow >= 0.0;
    if !(source_scalars_valid
        && return_contract
        && resource_contract
        && progress_contract
        && probability_contract
        && artificial_contract)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let forest_ready = !matches!(
        overlay.stage,
        FlowRandomizedAlmostLinearStageV1::Ready
            | FlowRandomizedAlmostLinearStageV1::BuildReturnEdgeReduction
            | FlowRandomizedAlmostLinearStageV1::BuildInitialPoint
    );
    if forest_ready != (forest_pool > 0)
        || forest_ready != (draws > 0)
            && !matches!(
                overlay.stage,
                FlowRandomizedAlmostLinearStageV1::EnumerateForestPool
            )
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let isolation_ready = matches!(
        overlay.stage,
        FlowRandomizedAlmostLinearStageV1::SampleIsolationCosts
            | FlowRandomizedAlmostLinearStageV1::SelectIsolatedOptimum
            | FlowRandomizedAlmostLinearStageV1::ConstructFinalPoint
            | FlowRandomizedAlmostLinearStageV1::RoundNearestInteger
            | FlowRandomizedAlmostLinearStageV1::CheckCertificate
            | FlowRandomizedAlmostLinearStageV1::Optimal
    );
    let final_point_ready = matches!(
        overlay.stage,
        FlowRandomizedAlmostLinearStageV1::ConstructFinalPoint
            | FlowRandomizedAlmostLinearStageV1::RoundNearestInteger
            | FlowRandomizedAlmostLinearStageV1::CheckCertificate
            | FlowRandomizedAlmostLinearStageV1::Optimal
    );
    let terminal = matches!(
        overlay.stage,
        FlowRandomizedAlmostLinearStageV1::RoundNearestInteger
            | FlowRandomizedAlmostLinearStageV1::CheckCertificate
            | FlowRandomizedAlmostLinearStageV1::Optimal
    );
    if isolation_ready != (isolation_attempt > 0)
        || isolation_ready != isolated_objective.is_some()
        || isolation_ready != (return_isolation_draw > 0)
        || final_point_ready != final_point_return_flow.is_some()
        || final_point_ready != final_point_gap.is_some()
        || final_point_ready != final_point_mix.is_some()
        || final_point_gap.is_some_and(|gap| gap < 0.0 || gap > final_point_threshold)
        || final_point_mix.is_some_and(|mix| !(0.0..=0.25).contains(&mix))
        || terminal != overlay.final_return_flow.is_some()
        || terminal != overlay.final_artificial_flow.is_some()
        || terminal != overlay.edges.iter().all(|edge| edge.final_flow.is_some())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    let mut divergence = vec![0_i32; graph.nodes().len() + 1];
    let star = graph.nodes().len();
    let mut source_side = Vec::new();
    let mut counted_artificial = 0_u64;
    for (index, state) in overlay.nodes.iter().enumerate() {
        let direction = canonical_i8(&state.artificial_direction)
            .filter(|value| matches!(value, -1..=1))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let active_sign = canonical_i8(&state.active_artificial_sign)
            .filter(|value| matches!(value, -1..=1))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let flow = finite(&state.artificial_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let capacity =
            finite(&state.artificial_capacity).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let component =
            canonical_u64(&state.tree_component).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let valid_parent = state.tree_parent_node_id.as_deref().is_none_or(|parent| {
            parent == "__artificial_star__"
                || graph
                    .nodes()
                    .iter()
                    .any(|node| node.id().as_str() == parent)
        });
        if state.node_id != graph.nodes()[index].id().as_str()
            || component > graph.nodes().len() as u64
            || !valid_parent
            || direction == 0 && (flow != 0.0 || capacity != 0.0 || active_sign != 0)
            || direction != 0 && !(flow > 0.0 && flow < capacity)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if state.source_side {
            source_side.push(graph.nodes()[index].id().clone());
        }
        if direction != 0 {
            counted_artificial = counted_artificial.saturating_add(1);
            let (from, to) = if direction > 0 {
                (star, index)
            } else {
                (index, star)
            };
            divergence[from] += i32::from(active_sign);
            divergence[to] -= i32::from(active_sign);
        }
    }
    if counted_artificial != artificial_edges {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    for (index, state) in overlay.edges.iter().enumerate() {
        let edge = &graph.edges()[index];
        let interior_flow =
            finite(&state.interior_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let gradient = finite(&state.gradient).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let length = finite(&state.length).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let memberships = canonical_u64(&state.sampled_tree_memberships)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let sign = canonical_i8(&state.active_cycle_sign)
            .filter(|value| matches!(value, -1..=1))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let isolation_draw =
            canonical_u64(&state.isolation_draw).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let final_point_flow = state
            .final_point_flow
            .as_deref()
            .map(|value| finite(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
            .transpose()?;
        let final_flow = state
            .final_flow
            .as_deref()
            .map(|value| canonical_u64(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
            .transpose()?;
        let identity_valid = state.edge_id == edge.id().as_str();
        let flow_valid =
            if overlay.stage == FlowRandomizedAlmostLinearStageV1::InspectFeasibleAssignment {
                interior_flow >= 0.0 && interior_flow <= edge.capacity() as f64
            } else {
                interior_flow > 0.0 && interior_flow < edge.capacity() as f64
            };
        let coordinates_valid = gradient.is_finite() && length > 0.0;
        if !identity_valid
            || !flow_valid
            || !coordinates_valid
            || memberships > sample_count
            || isolation_ready != (isolation_draw > 0)
            || final_point_ready != final_point_flow.is_some()
            || final_point_flow.is_some_and(|flow| flow < 0.0 || flow > edge.capacity() as f64)
            || terminal
                && final_point_flow.is_none_or(|flow| flow.round().to_u64() != Some(flows[index]))
            || final_flow != terminal.then_some(flows[index])
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        divergence[edge.from().as_usize()] += i32::from(sign);
        divergence[edge.to().as_usize()] -= i32::from(sign);
    }
    divergence[sink.as_usize()] += i32::from(return_sign);
    divergence[source.as_usize()] -= i32::from(return_sign);
    if divergence.into_iter().any(|value| value != 0) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if terminal {
        let final_return = overlay
            .final_return_flow
            .as_deref()
            .and_then(canonical_u64)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let final_artificial = overlay
            .final_artificial_flow
            .as_deref()
            .and_then(canonical_u64)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let certificate = crate::certificate::check_max_flow(graph, source, sink, flows)
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        if final_return != target
            || final_artificial != 0
            || final_point_return_flow
                .is_none_or(|flow| flow.round().to_u64() != Some(final_return))
            || u64::try_from(certificate.value).ok() != Some(target)
            || certificate.source_side != source_side
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    } else if flows.iter().any(|flow| *flow != 0) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn validate_deterministic_almost_linear_overlay(
    graph: &FlowNetwork,
    source: crate::model::NodeIndex,
    sink: crate::model::NodeIndex,
    flows: &[u64],
    overlay: &FlowDeterministicAlmostLinearOverlayV1,
) -> Result<(), FlowSceneError> {
    if source == sink
        || graph.node(source).is_none()
        || graph.node(sink).is_none()
        || graph.nodes().len() < 2
        || graph.nodes().len() > crate::algorithms::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_NODES
        || graph.edges().is_empty()
        || graph.edges().len() > crate::algorithms::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_EDGES
        || graph.nodes().iter().any(|node| node.supply() != 0)
        || graph
            .edges()
            .iter()
            .any(|edge| edge.lower() != 0 || edge.capacity() == 0 || edge.from() == edge.to())
        || flows.len() != graph.edges().len()
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let u64_value = |value: &str| scene_canonical_integer::<u64>(value);
    let i8_value = |value: &str| scene_canonical_integer::<i8>(value);
    let finite = |value: &str| electrical_f64(value);
    let alpha = finite(&overlay.alpha).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let potential = finite(&overlay.potential).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let gap = finite(&overlay.cost_gap).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let forest_pool =
        u64_value(&overlay.forest_pool_size).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let level_count =
        u64_value(&overlay.level_count).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let level_count_usize =
        usize::try_from(level_count).map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let branch_count =
        u64_value(&overlay.branch_count).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let built_branch_records =
        u64_value(&overlay.built_branch_records).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    if level_count == 0 || branch_count == 0 {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let _fundamental_cycles =
        u64_value(&overlay.fundamental_cycles).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let iteration = u64_value(&overlay.iteration).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let rebuild_epoch =
        u64_value(&overlay.rebuild_epoch).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let return_capacity =
        u64_value(&overlay.return_capacity).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let return_flow = finite(&overlay.return_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let return_gradient =
        finite(&overlay.return_gradient).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let return_length =
        finite(&overlay.return_length).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let return_mask =
        u64_value(&overlay.return_tree_level_mask).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let return_sign = i8_value(&overlay.active_return_sign)
        .filter(|value| matches!(value, -1..=1))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let target = u64_value(&overlay.target_value).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let artificial_edges =
        u64_value(&overlay.artificial_edges).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let artificial_flow =
        finite(&overlay.artificial_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let final_point_threshold = parse_scene_rational(&overlay.final_point_threshold)
        .filter(|value| *value == BigRational::new(BigInt::from(1), BigInt::from(2)))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let final_point_gap = overlay
        .final_point_gap
        .as_ref()
        .map(|value| parse_scene_rational(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    let final_point_mix = overlay
        .final_point_mix
        .as_ref()
        .map(|value| parse_scene_rational(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    let final_point_return = overlay
        .final_point_return_flow
        .as_ref()
        .map(|value| parse_scene_rational(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    let rounding_return = overlay
        .rounding_return_flow
        .as_ref()
        .map(|value| parse_scene_rational(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
        .transpose()?;
    let rounding_return_sign = i8_value(&overlay.rounding_return_sign)
        .filter(|value| matches!(value, -1..=1))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let mask_limit = 1_u64 << crate::algorithms::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS;
    let expected_return_capacity = (graph.edges().len() as u64)
        .checked_mul(
            graph
                .edges()
                .iter()
                .map(crate::model::FlowEdge::capacity)
                .max()
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
        )
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let records_per_collection = level_count
        .checked_mul(branch_count)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let record_window_start = rebuild_epoch
        .checked_mul(records_per_collection)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let record_window_end = record_window_start
        .checked_add(records_per_collection)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let active_level = overlay.active_level.as_deref().and_then(u64_value);
    let branch_record_progress_valid = built_branch_records >= record_window_start
        && built_branch_records <= record_window_end
        && if overlay.stage == FlowDeterministicAlmostLinearStageV1::InstallBranchRecord {
            let ordinal = built_branch_records.saturating_sub(record_window_start + 1);
            let expected_level = ordinal / branch_count;
            let expected_branch = ordinal % branch_count;
            active_level == Some(expected_level)
                && usize::try_from(expected_level)
                    .ok()
                    .and_then(|level| overlay.active_branches.get(level))
                    .and_then(|value| u64_value(value))
                    == Some(expected_branch)
        } else if overlay.stage == FlowDeterministicAlmostLinearStageV1::BuildBranchCollection {
            built_branch_records == record_window_end
        } else {
            true
        };
    let branch_state_valid = level_count
        == crate::algorithms::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS as u64
        && branch_count == crate::algorithms::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES as u64
        && overlay.active_branches.len() == level_count_usize
        && overlay.passes.len() == level_count_usize
        && overlay
            .active_branches
            .iter()
            .all(|value| u64_value(value).is_some_and(|branch| branch < branch_count))
        && overlay.passes.iter().all(|value| {
            u64_value(value).is_some_and(|passes| {
                passes <= crate::algorithms::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_PASSES
            })
        })
        && overlay
            .active_level
            .as_deref()
            .is_none_or(|value| u64_value(value).is_some_and(|level| level < level_count))
        && branch_record_progress_valid;
    let selected_state_valid = [
        overlay.selected_ratio.is_some(),
        overlay.selected_off_tree_edge.is_some(),
        overlay.selected_cycle_kind.is_some(),
    ]
    .into_iter()
    .all(|present| present == overlay.selected_ratio.is_some())
        && overlay.selected_ratio.as_deref().is_none_or(|value| {
            finite(value).is_some_and(|ratio| {
                ratio < 0.0
                    || overlay.stage
                        == FlowDeterministicAlmostLinearStageV1::InspectFundamentalCycle
            })
        })
        && overlay
            .selected_off_tree_edge
            .as_deref()
            .is_none_or(|value| {
                u64_value(value).is_some_and(|edge| {
                    edge < (graph.edges().len() + graph.nodes().len() + 1) as u64
                })
            });
    if !(alpha > 0.0
        && alpha < 1.0
        && potential.is_finite()
        && gap > 0.0
        && forest_pool
            <= crate::algorithms::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_FORESTS as u64
        && iteration <= crate::algorithms::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_IPM_STEPS
        && return_capacity == expected_return_capacity
        && return_flow > 0.0
        && return_flow < return_capacity as f64
        && return_gradient.is_finite()
        && return_length > 0.0
        && return_mask < mask_limit
        && artificial_flow >= 0.0
        && branch_state_valid
        && selected_state_valid)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let geometry_visible = matches!(
        overlay.stage,
        FlowDeterministicAlmostLinearStageV1::InstallBranchRecord
            | FlowDeterministicAlmostLinearStageV1::BuildCoreGraph
            | FlowDeterministicAlmostLinearStageV1::BuildSpannerEmbedding
            | FlowDeterministicAlmostLinearStageV1::QueryMinimumRatioCycle
            | FlowDeterministicAlmostLinearStageV1::QueryFailure
            | FlowDeterministicAlmostLinearStageV1::ShiftBranch
            | FlowDeterministicAlmostLinearStageV1::RebuildDeeperLevels
            | FlowDeterministicAlmostLinearStageV1::PotentialReductionStep
            | FlowDeterministicAlmostLinearStageV1::DetectChangedCoordinates
            | FlowDeterministicAlmostLinearStageV1::EnumerateFeasibleSet
            | FlowDeterministicAlmostLinearStageV1::ConstructFinalPoint
            | FlowDeterministicAlmostLinearStageV1::RoundingIntegralEdge
            | FlowDeterministicAlmostLinearStageV1::RoundingLinkFractionalEdge
            | FlowDeterministicAlmostLinearStageV1::RoundingCancelFractionalCycle
            | FlowDeterministicAlmostLinearStageV1::FinishFlowRounding
            | FlowDeterministicAlmostLinearStageV1::CheckCertificate
            | FlowDeterministicAlmostLinearStageV1::Optimal
    );
    let core_vertices =
        u64_value(&overlay.core_vertices).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let core_edges = u64_value(&overlay.core_edges).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let spanner_edges =
        u64_value(&overlay.spanner_edges).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    u64_value(&overlay.embedding_hops).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    if geometry_visible != (core_vertices > 0)
        || spanner_edges > core_edges
        || core_vertices > (graph.nodes().len() + 1) as u64
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let final_point_ready = matches!(
        overlay.stage,
        FlowDeterministicAlmostLinearStageV1::ConstructFinalPoint
            | FlowDeterministicAlmostLinearStageV1::RoundingIntegralEdge
            | FlowDeterministicAlmostLinearStageV1::RoundingLinkFractionalEdge
            | FlowDeterministicAlmostLinearStageV1::RoundingCancelFractionalCycle
            | FlowDeterministicAlmostLinearStageV1::FinishFlowRounding
            | FlowDeterministicAlmostLinearStageV1::CheckCertificate
            | FlowDeterministicAlmostLinearStageV1::Optimal
    );
    let rounding_ready = matches!(
        overlay.stage,
        FlowDeterministicAlmostLinearStageV1::RoundingIntegralEdge
            | FlowDeterministicAlmostLinearStageV1::RoundingLinkFractionalEdge
            | FlowDeterministicAlmostLinearStageV1::RoundingCancelFractionalCycle
            | FlowDeterministicAlmostLinearStageV1::FinishFlowRounding
            | FlowDeterministicAlmostLinearStageV1::CheckCertificate
            | FlowDeterministicAlmostLinearStageV1::Optimal
    );
    let terminal = matches!(
        overlay.stage,
        FlowDeterministicAlmostLinearStageV1::FinishFlowRounding
            | FlowDeterministicAlmostLinearStageV1::CheckCertificate
            | FlowDeterministicAlmostLinearStageV1::Optimal
    );
    let rounding_operation = matches!(
        overlay.stage,
        FlowDeterministicAlmostLinearStageV1::RoundingIntegralEdge
            | FlowDeterministicAlmostLinearStageV1::RoundingLinkFractionalEdge
            | FlowDeterministicAlmostLinearStageV1::RoundingCancelFractionalCycle
    );
    let processed_edge_valid = overlay
        .rounding_processed_edge
        .as_deref()
        .is_none_or(|edge| {
            graph
                .edges()
                .iter()
                .any(|candidate| candidate.id().as_str() == edge)
                || edge.starts_with("deterministic-rounding-return")
        });
    if final_point_ready != final_point_gap.is_some()
        || final_point_ready != final_point_mix.is_some()
        || final_point_ready != final_point_return.is_some()
        || rounding_ready != rounding_return.is_some()
        || !rounding_ready && (overlay.rounding_return_forest_edge || rounding_return_sign != 0)
        || final_point_gap
            .as_ref()
            .is_some_and(|gap| gap < &BigRational::zero() || gap >= &final_point_threshold)
        || final_point_mix.as_ref().is_some_and(|mix| {
            mix <= &BigRational::zero() || mix > &BigRational::new(BigInt::from(1), BigInt::from(4))
        })
        || terminal != overlay.final_return_flow.is_some()
        || terminal != overlay.final_artificial_flow.is_some()
        || terminal != overlay.edges.iter().all(|edge| edge.final_flow.is_some())
        || rounding_operation != overlay.rounding_processed_edge.is_some()
        || !processed_edge_valid
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    let star = graph.nodes().len();
    let mut divergence = vec![0_i32; star + 1];
    let mut final_point_divergence = vec![BigRational::zero(); star];
    let mut rounding_divergence = vec![BigRational::zero(); star];
    let mut rounding_cycle_divergence = vec![0_i32; star];
    let mut rounding_forest_endpoints = Vec::new();
    let mut counted_artificial = 0_u64;
    let mut source_side = Vec::new();
    for (index, state) in overlay.nodes.iter().enumerate() {
        let direction = i8_value(&state.artificial_direction)
            .filter(|value| matches!(value, -1..=1))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let sign = i8_value(&state.active_artificial_sign)
            .filter(|value| matches!(value, -1..=1))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let flow = finite(&state.artificial_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let capacity =
            finite(&state.artificial_capacity).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let mask = u64_value(&state.artificial_tree_level_mask)
            .filter(|value| *value < mask_limit)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let component =
            u64_value(&state.forest_component).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let parent_valid = state.tree_parent_node_id.as_deref().is_none_or(|parent| {
            parent == "__artificial_star__"
                || graph
                    .nodes()
                    .iter()
                    .any(|node| node.id().as_str() == parent)
        });
        if state.node_id != graph.nodes()[index].id().as_str()
            || component > star as u64
            || !parent_valid
            || state.active_artificial_tree_edge && mask == 0
            || direction == 0 && (flow != 0.0 || capacity != 0.0 || sign != 0)
            || direction != 0 && !(flow > 0.0 && flow < capacity)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if state.source_side {
            source_side.push(graph.nodes()[index].id().clone());
        }
        if direction != 0 {
            counted_artificial = counted_artificial.saturating_add(1);
            let (from, to) = if direction > 0 {
                (star, index)
            } else {
                (index, star)
            };
            divergence[from] += i32::from(sign);
            divergence[to] -= i32::from(sign);
        }
    }
    if counted_artificial != artificial_edges {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    for (index, state) in overlay.edges.iter().enumerate() {
        let edge = &graph.edges()[index];
        let interior = finite(&state.interior_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let gradient = finite(&state.gradient).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let length = finite(&state.length).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let stretch =
            finite(&state.embedding_stretch).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let tree_mask = u64_value(&state.tree_level_mask)
            .filter(|value| *value < mask_limit)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let _forest_mask = u64_value(&state.forest_level_mask)
            .filter(|value| *value < mask_limit && *value & !tree_mask == 0)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let hops = u64_value(&state.embedding_hops).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let sign = i8_value(&state.active_cycle_sign)
            .filter(|value| matches!(value, -1..=1))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let rounding_sign = i8_value(&state.rounding_cycle_sign)
            .filter(|value| matches!(value, -1..=1))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let final_point_flow = state
            .final_point_flow
            .as_ref()
            .map(|value| parse_scene_rational(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
            .transpose()?;
        let rounding_flow = state
            .rounding_flow
            .as_ref()
            .map(|value| parse_scene_rational(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
            .transpose()?;
        let final_flow = state
            .final_flow
            .as_deref()
            .map(|value| u64_value(value).ok_or(FlowSceneError::SnapshotGraphMismatch))
            .transpose()?;
        if state.edge_id != edge.id().as_str()
            || !(interior > 0.0 && interior < edge.capacity() as f64)
            || !gradient.is_finite()
            || length <= 0.0
            || stretch < 0.0
            || state.active_tree_edge && tree_mask == 0
            || state.active_spanner_edge && !state.active_core_edge
            || hops == 0 && stretch != 0.0
            || final_point_ready != final_point_flow.is_some()
            || rounding_ready != rounding_flow.is_some()
            || final_point_flow.as_ref().is_some_and(|flow| {
                flow < &BigRational::zero()
                    || flow > &BigRational::from_integer(BigInt::from(edge.capacity()))
            })
            || rounding_flow.as_ref().is_some_and(|flow| {
                flow < &BigRational::zero()
                    || flow > &BigRational::from_integer(BigInt::from(edge.capacity()))
            })
            || state.rounding_forest_edge
                && rounding_flow.as_ref().is_none_or(BigRational::is_integer)
            || !rounding_ready && (state.rounding_forest_edge || rounding_sign != 0)
            || !matches!(
                overlay.stage,
                FlowDeterministicAlmostLinearStageV1::RoundingCancelFractionalCycle
            ) && rounding_sign != 0
            || terminal
                && rounding_flow.as_ref().is_none_or(|flow| {
                    !flow.is_integer() || flow.to_integer() != BigInt::from(flows[index])
                })
            || final_flow != terminal.then_some(flows[index])
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if let Some(flow) = final_point_flow {
            final_point_divergence[edge.from().as_usize()] += &flow;
            final_point_divergence[edge.to().as_usize()] -= flow;
        }
        if let Some(flow) = rounding_flow {
            rounding_divergence[edge.from().as_usize()] += &flow;
            rounding_divergence[edge.to().as_usize()] -= flow;
        }
        if state.rounding_forest_edge {
            rounding_forest_endpoints.push((edge.from().as_usize(), edge.to().as_usize()));
        }
        rounding_cycle_divergence[edge.from().as_usize()] += i32::from(rounding_sign);
        rounding_cycle_divergence[edge.to().as_usize()] -= i32::from(rounding_sign);
        divergence[edge.from().as_usize()] += i32::from(sign);
        divergence[edge.to().as_usize()] -= i32::from(sign);
    }
    divergence[sink.as_usize()] += i32::from(return_sign);
    divergence[source.as_usize()] -= i32::from(return_sign);
    if divergence.into_iter().any(|value| value != 0) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if let Some(return_flow) = &final_point_return {
        if return_flow < &BigRational::zero()
            || return_flow > &BigRational::from_integer(BigInt::from(return_capacity))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        final_point_divergence[sink.as_usize()] += return_flow;
        final_point_divergence[source.as_usize()] -= return_flow;
        let expected_gap = BigRational::from_integer(BigInt::from(target)) - return_flow;
        if final_point_gap.as_ref() != Some(&expected_gap) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    if let Some(return_flow) = &rounding_return {
        if return_flow < &BigRational::zero()
            || return_flow > &BigRational::from_integer(BigInt::from(return_capacity))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        rounding_divergence[sink.as_usize()] += return_flow;
        rounding_divergence[source.as_usize()] -= return_flow;
        if terminal
            && (!return_flow.is_integer() || return_flow.to_integer() != BigInt::from(target))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    if overlay.rounding_return_forest_edge {
        if rounding_return.as_ref().is_none_or(BigRational::is_integer) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        rounding_forest_endpoints.push((sink.as_usize(), source.as_usize()));
    }
    rounding_cycle_divergence[sink.as_usize()] += i32::from(rounding_return_sign);
    rounding_cycle_divergence[source.as_usize()] -= i32::from(rounding_return_sign);
    let rounding_cycle_visible = matches!(
        overlay.stage,
        FlowDeterministicAlmostLinearStageV1::RoundingCancelFractionalCycle
    );
    if final_point_ready && final_point_divergence.iter().any(|value| !value.is_zero())
        || rounding_ready && rounding_divergence.iter().any(|value| !value.is_zero())
        || rounding_cycle_divergence.iter().any(|value| *value != 0)
        || rounding_cycle_visible
            != (rounding_return_sign != 0
                || overlay
                    .edges
                    .iter()
                    .any(|edge| edge.rounding_cycle_sign != "0"))
        || rounding_return_sign < 0
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mut forest_parent = (0..star).collect::<Vec<_>>();
    for (left, right) in rounding_forest_endpoints {
        let mut left_root = left;
        while forest_parent[left_root] != left_root {
            left_root = forest_parent[left_root];
        }
        let mut right_root = right;
        while forest_parent[right_root] != right_root {
            right_root = forest_parent[right_root];
        }
        if left_root == right_root {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        forest_parent[right_root] = left_root;
    }
    if terminal {
        let final_return = overlay
            .final_return_flow
            .as_deref()
            .and_then(u64_value)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let final_artificial = overlay
            .final_artificial_flow
            .as_deref()
            .and_then(u64_value)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let certificate = crate::certificate::check_max_flow(graph, source, sink, flows)
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        if final_return != target
            || final_artificial != 0
            || u64::try_from(certificate.value).ok() != Some(target)
            || certificate.source_side != source_side
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    } else if flows.iter().any(|flow| *flow != 0) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn validate_randomized_almost_linear_mcf_overlay(
    graph: &FlowNetwork,
    flows: &[u64],
    overlay: &FlowRandomizedAlmostLinearMcfOverlayV1,
) -> Result<(), FlowSceneError> {
    let assignment_checkpoint =
        overlay.stage == FlowRandomizedAlmostLinearMcfStageV1::InspectFeasibleAssignment;
    let oracle_checkpoint =
        overlay.stage == FlowRandomizedAlmostLinearMcfStageV1::InspectOracleVector;
    let assignment_serial = overlay
        .assignment_serial
        .as_deref()
        .and_then(scene_canonical_integer::<u64>);
    let oracle_vector_serial = overlay
        .oracle_vector_serial
        .as_deref()
        .and_then(scene_canonical_integer::<u64>);
    let expected_assignment_cursor = assignment_checkpoint
        .then(|| graph.edges().last().map(|edge| edge.id().as_str()))
        .flatten();
    if flows.len() != graph.edges().len()
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
        || assignment_checkpoint != overlay.assignment_cursor.is_some()
        || assignment_checkpoint != assignment_serial.is_some()
        || assignment_serial.is_some_and(|value| value == 0 || !value.is_power_of_two())
        || overlay.assignment_cursor.as_deref() != expected_assignment_cursor
        || oracle_checkpoint != oracle_vector_serial.is_some()
        || oracle_vector_serial.is_some_and(|value| value == 0)
        || overlay
            .failure_denominator
            .parse::<u64>()
            .ok()
            .filter(|&value| value > 0)
            .is_none()
        || overlay.failure_numerator.parse::<u64>().is_err()
        || overlay.isolation_attempt.parse::<usize>().is_err()
        || overlay.forest_pool_size.parse::<usize>().is_err()
        || overlay
            .sampled_forest_index
            .as_ref()
            .is_some_and(|value| value.parse::<usize>().is_err())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let decimal_fields = [
        &overlay.alpha,
        &overlay.epsilon,
        &overlay.kappa,
        &overlay.eta,
        &overlay.initial_cost,
        &overlay.current_cost,
        &overlay.potential,
    ];
    if decimal_fields
        .into_iter()
        .any(|value| electrical_f64(value).is_none())
        || overlay
            .minimum_ratio
            .as_ref()
            .is_some_and(|value| electrical_f64(value).is_none())
        || overlay.optimum_cost.parse::<i128>().is_err()
        || overlay.isolated_optimum_cost.parse::<i128>().is_err()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let required = overlay
        .nodes
        .iter()
        .map(|state| state.required_divergence.parse::<i128>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    if required
        .iter()
        .try_fold(0_i128, |sum, &value| sum.checked_add(value))
        != Some(0)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    for (node, state) in graph.node_indices().zip(&overlay.nodes) {
        let canonical = graph
            .node(node)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if state.node_id != canonical.id().as_str()
            || state.component.parse::<usize>().is_err()
            || state.depth.parse::<usize>().is_err()
            || state.parent_node_id.as_ref().is_some_and(|id| {
                graph
                    .nodes()
                    .iter()
                    .all(|candidate| candidate.id().as_str() != id)
            })
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    let isolation_scale = overlay
        .isolation_scale
        .parse::<i128>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let isolated_optimum_cost = overlay
        .isolated_optimum_cost
        .parse::<i128>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let final_point_gap = overlay
        .final_point_gap
        .as_ref()
        .and_then(parse_scene_rational);
    let final_point_threshold = parse_scene_rational(&overlay.final_point_threshold)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let final_point_mix = overlay
        .final_point_mix
        .as_ref()
        .and_then(parse_scene_rational);
    let m = BigInt::from(graph.edges().len());
    let u = BigInt::from(
        graph
            .edges()
            .iter()
            .map(FlowEdge::capacity)
            .max()
            .unwrap_or(1)
            .max(1),
    );
    let expected_threshold = BigRational::new(
        BigInt::from(1_u8),
        BigInt::from(12_u8) * &m * &m * &m * &u * &u * &u,
    );
    let expected_scale = BigInt::from(4_u8) * &m * &m * &u * &u;
    let final_point_ready = matches!(
        overlay.stage,
        FlowRandomizedAlmostLinearMcfStageV1::ConstructFinalPoint
            | FlowRandomizedAlmostLinearMcfStageV1::RoundNearestInteger
            | FlowRandomizedAlmostLinearMcfStageV1::CheckCertificate
            | FlowRandomizedAlmostLinearMcfStageV1::Optimal
    );
    let rounded_ready = matches!(
        overlay.stage,
        FlowRandomizedAlmostLinearMcfStageV1::RoundNearestInteger
            | FlowRandomizedAlmostLinearMcfStageV1::CheckCertificate
            | FlowRandomizedAlmostLinearMcfStageV1::Optimal
    );
    if BigInt::from(isolation_scale) != expected_scale
        || final_point_threshold != expected_threshold
        || final_point_ready != final_point_gap.is_some()
        || final_point_ready != final_point_mix.is_some()
        || final_point_gap
            .as_ref()
            .is_some_and(|gap| gap < &BigRational::zero() || gap > &final_point_threshold)
        || final_point_mix.as_ref().is_some_and(|mix| {
            mix <= &BigRational::zero()
                || mix > &BigRational::new(BigInt::from(1_u8), BigInt::from(4_u8))
        })
        || overlay.exact_recovery != rounded_ready
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let isolation_visible = !matches!(
        overlay.stage,
        FlowRandomizedAlmostLinearMcfStageV1::Ready
            | FlowRandomizedAlmostLinearMcfStageV1::InspectFeasibleAssignment
            | FlowRandomizedAlmostLinearMcfStageV1::EnumerateFeasibleSet
    );
    let isolated_optimum_visible = !matches!(
        overlay.stage,
        FlowRandomizedAlmostLinearMcfStageV1::Ready
            | FlowRandomizedAlmostLinearMcfStageV1::InspectFeasibleAssignment
            | FlowRandomizedAlmostLinearMcfStageV1::EnumerateFeasibleSet
            | FlowRandomizedAlmostLinearMcfStageV1::SampleIsolationCosts
    );
    let mut point_divergence = vec![BigRational::zero(); graph.nodes().len()];
    let mut scaled_point_cost = BigRational::zero();
    let mut isolated_divergence = vec![0_i128; graph.nodes().len()];
    let mut isolated_original_cost = 0_i128;
    let mut isolated_perturbed_cost = 0_i128;
    for ((edge, state), &flow) in graph.edges().iter().zip(&overlay.edges).zip(flows) {
        let initial =
            electrical_f64(&state.initial_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let current =
            electrical_f64(&state.current_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let stale =
            electrical_f64(&state.stale_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let final_point = state
            .final_point_flow
            .as_ref()
            .and_then(parse_scene_rational);
        let final_flow = state
            .final_flow
            .as_deref()
            .map(str::parse::<u64>)
            .transpose()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let draw = state
            .isolation_draw
            .parse::<u64>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let isolated_cost = state
            .isolated_cost
            .parse::<i128>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let isolated_optimum_flow = state
            .isolated_optimum_flow
            .as_deref()
            .map(str::parse::<u64>)
            .transpose()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let expected_isolated = isolation_scale
            .checked_mul(i128::from(edge.cost()))
            .and_then(|value| value.checked_add(i128::from(draw)))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if state.edge_id != edge.id().as_str()
            || !(edge.lower() as f64..=edge.capacity() as f64).contains(&initial)
            || !(edge.lower() as f64..=edge.capacity() as f64).contains(&current)
            || !(edge.lower() as f64..=edge.capacity() as f64).contains(&stale)
            || final_point_ready != final_point.is_some()
            || rounded_ready != final_flow.is_some()
            || final_flow.is_some_and(|value| value > edge.capacity())
            || !matches!(state.candidate_sign.as_str(), "-1" | "0" | "1")
            || !matches!(state.selected_sign.as_str(), "-1" | "0" | "1")
            || (overlay.stage != FlowRandomizedAlmostLinearMcfStageV1::InspectOracleVector
                && state.candidate_sign != "0")
            || electrical_f64(&state.gradient).is_none()
            || electrical_f64(&state.length).is_none()
            || (isolation_visible && draw == 0)
            || (isolation_visible && isolated_cost != expected_isolated)
            || isolated_optimum_visible != isolated_optimum_flow.is_some()
            || isolated_optimum_flow.is_some_and(|value| value > edge.capacity())
            || (rounded_ready && final_flow != Some(flow))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if let Some(value) = isolated_optimum_flow {
            let signed = i128::from(value);
            isolated_divergence[edge.from().as_usize()] += signed;
            isolated_divergence[edge.to().as_usize()] -= signed;
            isolated_original_cost = isolated_original_cost
                .checked_add(
                    i128::from(edge.cost())
                        .checked_mul(signed)
                        .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
                )
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            isolated_perturbed_cost = isolated_perturbed_cost
                .checked_add(
                    isolated_cost
                        .checked_mul(signed)
                        .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
                )
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        }
        if let Some(point) = final_point {
            let lower = BigRational::from_integer(BigInt::from(edge.lower()));
            let upper = BigRational::from_integer(BigInt::from(edge.capacity()));
            let quotient = point.numer() / point.denom();
            let remainder = point.numer() % point.denom();
            let nearest = if remainder * BigInt::from(2_u8) >= *point.denom() {
                quotient + BigInt::from(1_u8)
            } else {
                quotient
            };
            let nearest_u64 = nearest
                .to_u64()
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let distance = (&point - BigRational::from_integer(nearest)).abs();
            if point < lower
                || point > upper
                || distance >= BigRational::new(BigInt::from(1_u8), BigInt::from(4_u8))
                || final_flow.is_some_and(|value| value != nearest_u64)
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            point_divergence[edge.from().as_usize()] += &point;
            point_divergence[edge.to().as_usize()] -= &point;
            scaled_point_cost += BigRational::from_integer(BigInt::from(isolated_cost)) * point;
        }
    }
    if isolated_optimum_visible
        && (isolated_divergence != required
            || isolated_original_cost
                != overlay
                    .optimum_cost
                    .parse::<i128>()
                    .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?
            || isolated_perturbed_cost != isolated_optimum_cost)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if final_point_ready {
        let expected_divergence = required
            .iter()
            .map(|&value| BigRational::from_integer(BigInt::from(value)))
            .collect::<Vec<_>>();
        let measured_gap = (scaled_point_cost
            - BigRational::from_integer(BigInt::from(isolated_optimum_cost)))
            / BigRational::from_integer(BigInt::from(isolation_scale));
        if point_divergence != expected_divergence
            || final_point_gap.as_ref() != Some(&measured_gap)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn validate_flow_framework_mcf_overlay(
    graph: &FlowNetwork,
    flows: &[u64],
    overlay: &FlowFrameworkMcfOverlayV1,
) -> Result<(), FlowSceneError> {
    let dynamic_serial = overlay
        .dynamic_operation_serial
        .as_deref()
        .and_then(scene_canonical_integer::<u64>);
    let dynamic_shape_valid = match overlay.stage {
        FlowFrameworkMcfStageV1::InitializeSourcePoint
        | FlowFrameworkMcfStageV1::RoundFractionalFlow
        | FlowFrameworkMcfStageV1::CheckCertificate
        | FlowFrameworkMcfStageV1::Optimal => overlay.dynamic_operation.is_none(),
        FlowFrameworkMcfStageV1::PeriodicReinitialize => matches!(
            overlay.dynamic_operation,
            None | Some(
                FlowFrameworkMcfDynamicOperationV1::TopologyStageApplied
                    | FlowFrameworkMcfDynamicOperationV1::PeriodicRebuilt
            )
        ),
        FlowFrameworkMcfStageV1::Detect => matches!(
            overlay.dynamic_operation,
            Some(FlowFrameworkMcfDynamicOperationV1::DetectReturned)
        ),
        FlowFrameworkMcfStageV1::QueryMinimumRatioCycle => matches!(
            overlay.dynamic_operation,
            Some(
                FlowFrameworkMcfDynamicOperationV1::CycleQueriedAccepted
                    | FlowFrameworkMcfDynamicOperationV1::CycleQueriedRejected
                    | FlowFrameworkMcfDynamicOperationV1::LevelShifted
                    | FlowFrameworkMcfDynamicOperationV1::QueryReturned
            )
        ),
        FlowFrameworkMcfStageV1::SourceProgress => matches!(
            overlay.dynamic_operation,
            Some(
                FlowFrameworkMcfDynamicOperationV1::FlowApplied
                    | FlowFrameworkMcfDynamicOperationV1::Completed
            )
        ),
    };
    if flows.len() != graph.edges().len()
        || overlay.edges.len() != graph.edges().len()
        || overlay.levels.len() != 2
        || !dynamic_shape_valid
        || overlay.dynamic_operation.is_some() != dynamic_serial.is_some()
        || dynamic_serial.is_some_and(|serial| serial == 0)
        || electrical_f64(&overlay.potential_before).is_none()
        || electrical_f64(&overlay.potential_after).is_none()
        || electrical_f64(&overlay.gap_before).is_none_or(|value| value < 0.0)
        || electrical_f64(&overlay.gap_after).is_none_or(|value| value < 0.0)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let accepted_ratio = parse_scene_rational(&overlay.accepted_ratio)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let target_progress = parse_scene_rational(&overlay.target_progress)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let exact_gap_before = parse_scene_rational(&overlay.exact_gap_before)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let exact_gap_after = parse_scene_rational(&overlay.exact_gap_after)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let stopping_gap =
        parse_scene_rational(&overlay.stopping_gap).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let iteration = overlay
        .iteration
        .parse::<u64>()
        .ok()
        .filter(|value| value.to_string() == overlay.iteration)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let initial = overlay.stage == FlowFrameworkMcfStageV1::InitializeSourcePoint;
    let progress_visible = iteration > 0
        && matches!(
            overlay.stage,
            FlowFrameworkMcfStageV1::QueryMinimumRatioCycle
                | FlowFrameworkMcfStageV1::SourceProgress
                | FlowFrameworkMcfStageV1::RoundFractionalFlow
                | FlowFrameworkMcfStageV1::CheckCertificate
                | FlowFrameworkMcfStageV1::Optimal
        );
    if (!progress_visible && (!accepted_ratio.is_zero() || !target_progress.is_zero()))
        || (progress_visible
            && (accepted_ratio <= BigRational::zero() || target_progress <= BigRational::zero()))
        || (initial && iteration != 0)
        || overlay.reinitialized != (iteration > 1)
        || exact_gap_before.is_negative()
        || exact_gap_after.is_negative()
        || stopping_gap != BigRational::new(BigInt::from(1), BigInt::from(2))
        || (iteration == 0 && exact_gap_before != exact_gap_after)
        || (iteration > 0 && exact_gap_after >= exact_gap_before)
        || (matches!(
            overlay.stage,
            FlowFrameworkMcfStageV1::RoundFractionalFlow
                | FlowFrameworkMcfStageV1::CheckCertificate
                | FlowFrameworkMcfStageV1::Optimal
        ) && exact_gap_after > stopping_gap)
        || (overlay.stage == FlowFrameworkMcfStageV1::Optimal)
            != (overlay.termination.as_deref() == Some("source-additive-half-gap"))
        || (overlay.stage != FlowFrameworkMcfStageV1::Optimal && overlay.termination.is_some())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    let final_point_visible = matches!(
        overlay.stage,
        FlowFrameworkMcfStageV1::RoundFractionalFlow
            | FlowFrameworkMcfStageV1::CheckCertificate
            | FlowFrameworkMcfStageV1::Optimal
    );
    let rounded_visible = matches!(
        overlay.stage,
        FlowFrameworkMcfStageV1::CheckCertificate | FlowFrameworkMcfStageV1::Optimal
    );
    if final_point_visible
        != (overlay.optimum_cost.is_some()
            && overlay.final_point_nodes.is_some()
            && overlay.final_point_edges.is_some())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if final_point_visible {
        let optimum_cost = scene_canonical_integer::<i128>(
            overlay
                .optimum_cost
                .as_deref()
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
        )
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let nodes = overlay
            .final_point_nodes
            .as_ref()
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let edges = overlay
            .final_point_edges
            .as_ref()
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if nodes.is_empty()
            || edges.is_empty()
            || edges.len() > crate::algorithms::FLOW_FRAMEWORK_MCF_MAX_AUGMENTED_EDGES
            || edges
                .iter()
                .any(|edge| edge.rounded_flow.is_some() != rounded_visible)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let mut required = BTreeMap::new();
        let mut fractional_divergence = BTreeMap::new();
        let mut rounded_divergence = BTreeMap::new();
        for node in nodes {
            let value = scene_canonical_integer::<i128>(&node.required_divergence)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            if required.insert(node.node_id.as_str(), value).is_some() {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            fractional_divergence.insert(node.node_id.as_str(), BigRational::zero());
            rounded_divergence.insert(node.node_id.as_str(), BigInt::zero());
        }
        if graph.nodes().iter().any(|node| {
            required.get(node.id().as_str()).copied() != Some(i128::from(node.supply()))
        }) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let mut exact_cost = BigRational::zero();
        let mut rounded_cost = BigInt::zero();
        let mut augmented_edges = BTreeSet::new();
        let mut original_edges = BTreeSet::new();
        for state in edges {
            let lower = scene_canonical_integer::<u64>(&state.lower)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let capacity = scene_canonical_integer::<u64>(&state.capacity)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let cost = scene_canonical_integer::<i64>(&state.cost)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let amount =
                parse_scene_rational(&state.flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            if !augmented_edges.insert(state.edge_id.as_str())
                || lower > capacity
                || amount < BigRational::from_integer(BigInt::from(lower))
                || amount > BigRational::from_integer(BigInt::from(capacity))
                || !required.contains_key(state.from.as_str())
                || !required.contains_key(state.to.as_str())
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            *fractional_divergence
                .get_mut(state.from.as_str())
                .ok_or(FlowSceneError::SnapshotGraphMismatch)? += &amount;
            *fractional_divergence
                .get_mut(state.to.as_str())
                .ok_or(FlowSceneError::SnapshotGraphMismatch)? -= &amount;
            exact_cost += &amount * BigInt::from(cost);

            if let Some(rounded) = state.rounded_flow.as_deref() {
                let rounded = scene_canonical_integer::<u64>(rounded)
                    .filter(|value| *value >= lower && *value <= capacity)
                    .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
                *rounded_divergence
                    .get_mut(state.from.as_str())
                    .ok_or(FlowSceneError::SnapshotGraphMismatch)? += BigInt::from(rounded);
                *rounded_divergence
                    .get_mut(state.to.as_str())
                    .ok_or(FlowSceneError::SnapshotGraphMismatch)? -= BigInt::from(rounded);
                rounded_cost += BigInt::from(rounded) * BigInt::from(cost);
                if state.auxiliary && rounded != 0 {
                    return Err(FlowSceneError::SnapshotGraphMismatch);
                }
            }

            if !state.auxiliary {
                let original = graph
                    .edges()
                    .iter()
                    .find(|edge| edge.id().as_str() == state.edge_id)
                    .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
                if !original_edges.insert(state.edge_id.as_str())
                    || state.from != graph.nodes()[original.from().as_usize()].id().as_str()
                    || state.to != graph.nodes()[original.to().as_usize()].id().as_str()
                    || lower != original.lower()
                    || capacity != original.capacity()
                    || cost != original.cost()
                {
                    return Err(FlowSceneError::SnapshotGraphMismatch);
                }
                let visible = overlay
                    .edges
                    .iter()
                    .find(|edge| edge.edge_id == state.edge_id)
                    .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
                if overlay.stage != FlowFrameworkMcfStageV1::Optimal
                    && parse_scene_rational(&visible.flow).as_ref() != Some(&amount)
                {
                    return Err(FlowSceneError::SnapshotGraphMismatch);
                }
                if overlay.stage == FlowFrameworkMcfStageV1::Optimal
                    && state.rounded_flow.as_deref() != Some(visible.flow.numerator.as_str())
                {
                    return Err(FlowSceneError::SnapshotGraphMismatch);
                }
            }
        }
        if original_edges.len() != graph.edges().len()
            || &exact_cost - BigRational::from_integer(BigInt::from(optimum_cost))
                != exact_gap_after
            || fractional_divergence.iter().any(|(node, value)| {
                value
                    != &BigRational::from_integer(BigInt::from(
                        required.get(node).copied().unwrap_or_default(),
                    ))
            })
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if rounded_visible
            && (rounded_cost != BigInt::from(optimum_cost)
                || BigRational::from_integer(rounded_cost.clone()) > exact_cost
                || rounded_divergence.iter().any(|(node, value)| {
                    value != &BigInt::from(required.get(node).copied().unwrap_or_default())
                }))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }

    let mut divergence = vec![BigRational::zero(); graph.nodes().len()];
    for ((edge, state), &generic_flow) in graph.edges().iter().zip(&overlay.edges).zip(flows) {
        let flow =
            parse_scene_rational(&state.flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let coefficient = parse_scene_rational(&state.cycle_coefficient)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if state.edge_id != edge.id().as_str()
            || state.selected == coefficient.is_zero()
            || flow < BigRational::from_integer(BigInt::from(edge.lower()))
            || flow > BigRational::from_integer(BigInt::from(edge.capacity()))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        divergence[edge.from().as_usize()] += &coefficient;
        divergence[edge.to().as_usize()] -= &coefficient;
        if overlay.stage == FlowFrameworkMcfStageV1::Optimal {
            if flow != BigRational::from_integer(BigInt::from(generic_flow)) {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
        } else if generic_flow != edge.lower() {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    if divergence.iter().any(|value| !value.is_zero()) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    for (level, state) in overlay.levels.iter().enumerate() {
        if state.level.parse::<usize>().ok() != Some(level)
            || state.level != level.to_string()
            || state
                .active_branch
                .parse::<usize>()
                .ok()
                .is_none_or(|branch| branch >= 2 || branch.to_string() != state.active_branch)
            || state
                .passes
                .parse::<u64>()
                .ok()
                .is_none_or(|passes| passes.to_string() != state.passes)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn validate_minimum_ratio_cycle_mcf_overlay(
    graph: &FlowNetwork,
    overlay: &FlowMinimumRatioCycleMcfOverlayV1,
) -> Result<(), FlowSceneError> {
    if graph.nodes().len() < 2
        || graph.nodes().len() > crate::algorithms::MINIMUM_RATIO_CYCLE_MCF_MAX_NODES
        || graph.edges().is_empty()
        || graph.edges().len() > crate::algorithms::MINIMUM_RATIO_CYCLE_MCF_MAX_EDGES
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let finite = |value: &str| electrical_f64(value);
    let integer = |value: &str| scene_canonical_integer::<u64>(value);
    let signed = |value: &str| scene_canonical_integer::<i8>(value);
    let alpha = finite(&overlay.alpha).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let optimum = scene_canonical_integer::<i128>(&overlay.optimum_cost)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let initial_cost =
        finite(&overlay.initial_cost).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let current_cost =
        finite(&overlay.current_cost).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let gap = finite(&overlay.cost_gap).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let potential_before =
        finite(&overlay.potential_before).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let current_potential =
        finite(&overlay.current_potential).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let kappa = finite(&overlay.kappa).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let eta = finite(&overlay.eta).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let weighted =
        finite(&overlay.weighted_step_norm).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let decrease =
        finite(&overlay.potential_decrease).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let guaranteed =
        finite(&overlay.guaranteed_decrease).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let selected_count = integer(&overlay.selected_edge_count)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let maximum_balance =
        integer(&overlay.maximum_absolute_balance).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let feasible_flows =
        integer(&overlay.feasible_flows).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let enumerated_vectors =
        integer(&overlay.enumerated_vectors).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let simple_cycles =
        integer(&overlay.simple_cycles).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let fundamental_cycles =
        integer(&overlay.fundamental_cycles).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    if !(alpha > 0.0 && alpha < 1.0)
        || gap < 0.0
        || kappa < 0.0
        || !(0.0..=0.99).contains(&kappa)
        || eta < 0.0
        || weighted < 0.0
        || decrease < 0.0
        || guaranteed < 0.0
        || feasible_flows == 0
        || enumerated_vectors > crate::algorithms::MINIMUM_RATIO_CYCLE_MCF_MAX_ENUMERATED_VECTORS
        || simple_cycles > enumerated_vectors
        || selected_count > graph.edges().len()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mapped = !overlay.stationary
        && minimum_ratio_cycle_mcf_stage_rank(overlay.stage)
            >= minimum_ratio_cycle_mcf_stage_rank(
                FlowMinimumRatioCycleMcfStageV1::MapGradientLength,
            );
    let forest_built = !overlay.stationary
        && minimum_ratio_cycle_mcf_stage_rank(overlay.stage)
            >= minimum_ratio_cycle_mcf_stage_rank(
                FlowMinimumRatioCycleMcfStageV1::BuildSpanningForest,
            );
    let applied = !overlay.stationary
        && minimum_ratio_cycle_mcf_stage_rank(overlay.stage)
            >= minimum_ratio_cycle_mcf_stage_rank(FlowMinimumRatioCycleMcfStageV1::ApplySourceStep);
    let measured = !overlay.stationary
        && minimum_ratio_cycle_mcf_stage_rank(overlay.stage)
            >= minimum_ratio_cycle_mcf_stage_rank(
                FlowMinimumRatioCycleMcfStageV1::MeasurePotentialDecrease,
            );
    let potential_visible = minimum_ratio_cycle_mcf_stage_rank(overlay.stage)
        >= minimum_ratio_cycle_mcf_stage_rank(FlowMinimumRatioCycleMcfStageV1::EvaluatePotential);
    let candidate_visible = matches!(
        overlay.stage,
        FlowMinimumRatioCycleMcfStageV1::EvaluateCycle
            | FlowMinimumRatioCycleMcfStageV1::UpdateBest
    );
    let vector_inspection = overlay.stage == FlowMinimumRatioCycleMcfStageV1::InspectVector;
    if candidate_visible != overlay.candidate_ratio.is_some() {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mut fixed = vec![false; graph.edges().len()];
    let mut initial = vec![0.0; graph.edges().len()];
    let mut updated = vec![0.0; graph.edges().len()];
    let mut gradients = vec![0.0; graph.edges().len()];
    let mut lengths = vec![0.0; graph.edges().len()];
    let mut candidate_signs = vec![0_i8; graph.edges().len()];
    let mut selected_signs = vec![0_i8; graph.edges().len()];
    let mut candidate_numerator = 0.0_f64;
    let mut candidate_denominator = 0.0_f64;
    for (index, (state, edge)) in overlay.edges.iter().zip(graph.edges()).enumerate() {
        let initial_flow =
            finite(&state.initial_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let updated_flow =
            finite(&state.updated_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let lower = finite(&state.lower_slack).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let upper = finite(&state.upper_slack).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let gradient = finite(&state.gradient).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let length = finite(&state.length).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let candidate_sign = signed(&state.candidate_sign)
            .filter(|value| matches!(value, -1..=1))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let selected_sign = signed(&state.selected_sign)
            .filter(|value| matches!(value, -1..=1))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let numerator =
            finite(&state.numerator_contribution).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let denominator =
            finite(&state.denominator_contribution).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if state.edge_id != edge.id().as_str()
            || initial_flow < edge.lower() as f64
            || initial_flow > edge.capacity() as f64
            || updated_flow < edge.lower() as f64 - 1.0e-9
            || updated_flow > edge.capacity() as f64 + 1.0e-9
            || !electrical_close(lower, initial_flow - edge.lower() as f64)
            || !electrical_close(upper, edge.capacity() as f64 - initial_flow)
            || length < 0.0
            || denominator < 0.0
            || state.fixed_on_face && (candidate_sign != 0 || selected_sign != 0)
            || !(candidate_visible || vector_inspection) && candidate_sign != 0
            || !forest_built && state.tree_edge
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if mapped && !state.fixed_on_face {
            let expected_length = upper.powf(-1.0 - alpha) + lower.powf(-1.0 - alpha);
            if lower <= 0.0 || upper <= 0.0 || !electrical_close(length, expected_length) {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
        } else if (!mapped || state.fixed_on_face)
            && (!electrical_close(gradient, 0.0) || !electrical_close(length, 0.0))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if !electrical_close(numerator, gradient * f64::from(candidate_sign))
            || !electrical_close(denominator, if candidate_sign == 0 { 0.0 } else { length })
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        fixed[index] = state.fixed_on_face;
        initial[index] = initial_flow;
        updated[index] = updated_flow;
        gradients[index] = gradient;
        lengths[index] = length;
        candidate_signs[index] = candidate_sign;
        selected_signs[index] = selected_sign;
        candidate_numerator += numerator;
        candidate_denominator += denominator;
    }
    let active = fixed.iter().filter(|&&value| !value).count();
    let u = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::capacity)
        .max()
        .unwrap_or(1)
        .max(2);
    let expected_alpha = 1.0 / (1_000.0 * ((active.max(1) as f64 * u as f64).max(2.0)).ln());
    let recomputed_initial_cost = graph
        .edges()
        .iter()
        .zip(&initial)
        .map(|(edge, flow)| edge.cost() as f64 * flow)
        .sum::<f64>();
    let recomputed_current_cost = graph
        .edges()
        .iter()
        .zip(&updated)
        .map(|(edge, flow)| edge.cost() as f64 * flow)
        .sum::<f64>();
    if !electrical_close(alpha, expected_alpha)
        || !electrical_close(initial_cost, recomputed_initial_cost)
        || !electrical_close(current_cost, recomputed_current_cost)
        || !electrical_close(gap, current_cost - optimum as f64)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if mapped && !overlay.stationary {
        let initial_gap = initial_cost - optimum as f64;
        for (index, edge) in graph.edges().iter().enumerate() {
            if fixed[index] {
                continue;
            }
            let expected = 20.0 * active.max(1) as f64 * edge.cost() as f64 / initial_gap
                + alpha
                    * ((edge.capacity() as f64 - initial[index]).powf(-1.0 - alpha)
                        - (initial[index] - edge.lower() as f64).powf(-1.0 - alpha));
            if !electrical_close(gradients[index], expected) {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
        }
    }
    let source_potential = |flow: &[f64]| -> Option<f64> {
        let objective = graph
            .edges()
            .iter()
            .zip(flow)
            .map(|(edge, amount)| edge.cost() as f64 * amount)
            .sum::<f64>();
        let objective_gap = objective - optimum as f64;
        if objective_gap <= 0.0 {
            return None;
        }
        let barriers = graph
            .edges()
            .iter()
            .zip(flow)
            .zip(&fixed)
            .filter(|(_, fixed)| !**fixed)
            .map(|((edge, amount), _)| {
                (edge.capacity() as f64 - amount).powf(-alpha)
                    + (amount - edge.lower() as f64).powf(-alpha)
            })
            .sum::<f64>();
        Some(20.0 * active.max(1) as f64 * objective_gap.ln() + barriers)
    };
    if overlay.stationary {
        if gap > 1.0e-9
            || overlay.best_ratio.is_some()
            || selected_signs.iter().any(|&sign| sign != 0)
            || !electrical_close(kappa, 0.0)
            || !electrical_close(eta, 0.0)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    } else {
        let before = source_potential(&initial).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if potential_visible && !electrical_close(potential_before, before) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if !potential_visible
            && (!electrical_close(potential_before, 0.0)
                || !electrical_close(current_potential, 0.0))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    if candidate_visible {
        let ratio = overlay
            .candidate_ratio
            .as_deref()
            .and_then(finite)
            .filter(|_| candidate_denominator > 0.0)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if !electrical_close(ratio, candidate_numerator / candidate_denominator) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    let mut candidate_balance = vec![0_i32; graph.nodes().len()];
    let mut selected_balance = vec![0_i32; graph.nodes().len()];
    for (index, edge) in graph.edges().iter().enumerate() {
        candidate_balance[edge.from().as_usize()] -= i32::from(candidate_signs[index]);
        candidate_balance[edge.to().as_usize()] += i32::from(candidate_signs[index]);
        selected_balance[edge.from().as_usize()] -= i32::from(selected_signs[index]);
        selected_balance[edge.to().as_usize()] += i32::from(selected_signs[index]);
    }
    for (index, (state, node)) in overlay.nodes.iter().zip(graph.nodes()).enumerate() {
        let component = integer(&state.component).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let depth = integer(&state.depth).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let balance = scene_canonical_integer::<i32>(&state.candidate_balance)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if state.node_id != node.id().as_str()
            || usize::try_from(component)
                .ok()
                .is_none_or(|value| value >= graph.nodes().len())
            || usize::try_from(depth).ok().is_none()
            || balance != candidate_balance[index]
            || state.on_candidate
                != graph.edges().iter().enumerate().any(|(edge_index, edge)| {
                    candidate_signs[edge_index] != 0
                        && (edge.from().as_usize() == index || edge.to().as_usize() == index)
                })
            || state.on_selected
                != graph.edges().iter().enumerate().any(|(edge_index, edge)| {
                    selected_signs[edge_index] != 0
                        && (edge.from().as_usize() == index || edge.to().as_usize() == index)
                })
            || state.parent_node_id.as_deref().is_some_and(|parent| {
                !graph
                    .nodes()
                    .iter()
                    .any(|candidate| candidate.id().as_str() == parent)
            })
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    let computed_maximum = candidate_balance
        .iter()
        .map(|value| u64::from(value.unsigned_abs()))
        .max()
        .unwrap_or(0);
    if maximum_balance != computed_maximum
        || selected_count != selected_signs.iter().filter(|&&sign| sign != 0).count()
        || selected_signs.iter().any(|&sign| sign != 0)
            && selected_balance.iter().any(|&balance| balance != 0)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if let Some(raw_ratio) = overlay.best_ratio.as_deref() {
        let ratio = finite(raw_ratio)
            .filter(|value| *value <= 0.0)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let numerator = gradients
            .iter()
            .zip(&selected_signs)
            .map(|(gradient, sign)| gradient * f64::from(*sign))
            .sum::<f64>();
        let denominator = lengths
            .iter()
            .zip(&selected_signs)
            .filter(|(_, sign)| **sign != 0)
            .map(|(length, _)| *length)
            .sum::<f64>();
        if denominator <= 0.0 || !electrical_close(ratio, numerator / denominator) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if applied && !overlay.stationary {
            let expected_kappa = (-ratio).min(0.99);
            let expected_eta = expected_kappa * expected_kappa / (50.0 * -numerator);
            let expected_weighted = expected_eta * denominator;
            if !electrical_close(kappa, expected_kappa)
                || !electrical_close(eta, expected_eta)
                || !electrical_close(weighted, expected_weighted)
                || updated.iter().zip(&initial).zip(&selected_signs).any(
                    |((&after, &before), &sign)| {
                        !electrical_close(after, before + eta * f64::from(sign))
                    },
                )
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
        }
    } else if selected_count != 0 {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if !applied
        && (updated
            .iter()
            .zip(&initial)
            .any(|(&left, &right)| !electrical_close(left, right))
            || !electrical_close(kappa, 0.0)
            || !electrical_close(eta, 0.0)
            || !electrical_close(weighted, 0.0)
            || !electrical_close(guaranteed, 0.0))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if measured && !overlay.stationary {
        let after = source_potential(&updated).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if !electrical_close(current_potential, after)
            || !electrical_close(decrease, potential_before - after)
            || !electrical_close(guaranteed, kappa * kappa / 500.0)
            || decrease + 1.0e-8 * potential_before.abs().max(1.0) < guaranteed
            || weighted > kappa / 25.0 + 1.0e-8
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    } else if !electrical_close(decrease, 0.0) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let active_components = active_component_count(graph, &fixed);
    let expected_dimension = active
        .checked_add(active_components)
        .and_then(|value| value.checked_sub(graph.nodes().len()))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    if forest_built
        && fundamental_cycles
            != u64::try_from(expected_dimension)
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_weighted_augmenting_paths_overlay(
    graph: &FlowNetwork,
    flows: &[u64],
    overlay: &FlowWeightedAugmentingPathsOverlayV1,
) -> Result<(), FlowSceneError> {
    let phase = overlay
        .phase
        .parse::<usize>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let phase_count = overlay
        .phase_count
        .parse::<usize>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let capacity_bit = overlay
        .capacity_bit
        .parse::<usize>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let height = overlay
        .height
        .parse::<u64>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let phi_numerator = overlay
        .phi_numerator
        .parse::<u128>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let phi_denominator = overlay
        .phi_denominator
        .parse::<u128>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let bottleneck = overlay
        .active_bottleneck
        .parse::<u64>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    if phase_count == 0
        || phase >= phase_count
        || capacity_bit >= phase_count
        || phi_denominator == 0
        || overlay.round.parse::<u64>().is_err()
        || overlay.hierarchy_cuts.parse::<u64>().is_err()
        || overlay.relabel_jumps.parse::<u64>().is_err()
        || overlay.augmentations.parse::<u64>().is_err()
        || overlay.augmented_units.parse::<u128>().is_err()
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
        || overlay.residual_arcs.len() != graph.edges().len() * 2
        || flows.len() != graph.edges().len()
        || (bottleneck == 0) != overlay.active_path.is_empty()
        || (overlay.stage == FlowWeightedAugmentingPathsStageV1::AugmentPath) != (bottleneck > 0)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let hierarchy_visible = overlay
        .residual_arcs
        .iter()
        .any(|arc| arc.hierarchy_kind.is_some());
    let requires_hierarchy = matches!(
        overlay.stage,
        FlowWeightedAugmentingPathsStageV1::BuildHierarchy
            | FlowWeightedAugmentingPathsStageV1::CertifyExpansion
            | FlowWeightedAugmentingPathsStageV1::AssignWeights
            | FlowWeightedAugmentingPathsStageV1::RelabelSweep
            | FlowWeightedAugmentingPathsStageV1::AugmentPath
            | FlowWeightedAugmentingPathsStageV1::FinishWeightedRound
    );
    let requires_height = matches!(
        overlay.stage,
        FlowWeightedAugmentingPathsStageV1::AssignWeights
            | FlowWeightedAugmentingPathsStageV1::RelabelSweep
            | FlowWeightedAugmentingPathsStageV1::AugmentPath
            | FlowWeightedAugmentingPathsStageV1::FinishWeightedRound
    );
    let terminal_boundary = matches!(
        overlay.stage,
        FlowWeightedAugmentingPathsStageV1::FinishCapacityPhase
            | FlowWeightedAugmentingPathsStageV1::CheckCertificate
            | FlowWeightedAugmentingPathsStageV1::Optimal
    );
    if hierarchy_visible != (phi_numerator > 0)
        || requires_hierarchy && !hierarchy_visible
        || requires_height && height == 0
        || (!requires_height && !terminal_boundary && height > 0)
        || terminal_boundary && hierarchy_visible != (height > 0)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    for (ordinal, (node, state)) in graph.nodes().iter().zip(&overlay.nodes).enumerate() {
        let component = state
            .component
            .parse::<usize>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let order = state
            .order
            .parse::<usize>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let label = state
            .label
            .parse::<u64>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        if state.node_id != node.id().as_str()
            || (hierarchy_visible && (component >= graph.nodes().len() || order == 0))
            || (!hierarchy_visible && (component != 0 || order != 0))
            || (!state.alive && label <= height.saturating_mul(9))
            || (state.alive && height > 0 && label > height.saturating_mul(9))
            || graph.node_indices().nth(ordinal).is_none()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    let mut active = BTreeSet::new();
    for item in &overlay.active_path {
        if item.direction != "forward" && item.direction != "reverse" {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if !active.insert((item.edge_id.as_str(), item.direction.as_str())) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    for (ordinal, (edge, edge_state)) in graph.edges().iter().zip(&overlay.edges).enumerate() {
        let scaled = edge_state
            .scaled_capacity
            .parse::<u64>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let flow = edge_state
            .flow
            .parse::<u64>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        if edge_state.edge_id != edge.id().as_str()
            || scaled > edge.capacity()
            || flow > scaled
            || flows[ordinal] != flow
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let from = graph
            .node(edge.from())
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?
            .id()
            .as_str();
        let to = graph
            .node(edge.to())
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?
            .id()
            .as_str();
        for direction_index in 0..2 {
            let arc = &overlay.residual_arcs[ordinal * 2 + direction_index];
            let forward = direction_index == 0;
            let expected_direction = if forward { "forward" } else { "reverse" };
            let expected_capacity = if forward { scaled - flow } else { flow };
            let expected_from = if forward { from } else { to };
            let expected_to = if forward { to } else { from };
            let weight = arc
                .weight
                .parse::<u64>()
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
            let capacity = arc
                .capacity
                .parse::<u64>()
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
            let active_here = active.contains(&(arc.edge_id.as_str(), arc.direction.as_str()));
            if arc.edge_id != edge.id().as_str()
                || arc.direction != expected_direction
                || arc.from != expected_from
                || arc.to != expected_to
                || capacity != expected_capacity
                || arc.active != active_here
                || arc.admissible && capacity == 0
                || hierarchy_visible != arc.hierarchy_kind.is_some()
                || hierarchy_visible != (weight > 0)
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
        }
    }
    if overlay.stage == FlowWeightedAugmentingPathsStageV1::Optimal
        && overlay
            .edges
            .iter()
            .zip(graph.edges())
            .any(|(state, edge)| state.scaled_capacity.parse::<u64>().ok() != Some(edge.capacity()))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_weighted_push_relabel_shortcut_overlay(
    graph: &FlowNetwork,
    flows: &[u64],
    overlay: &FlowWeightedPushRelabelShortcutOverlayV1,
) -> Result<(), FlowSceneError> {
    let hierarchy_levels = scene_canonical_integer::<usize>(&overlay.hierarchy_levels)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let psi_numerator = scene_canonical_integer::<u64>(&overlay.psi_numerator)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let psi_denominator = scene_canonical_integer::<u64>(&overlay.psi_denominator)
        .filter(|value| *value > 0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let height = scene_canonical_integer::<u64>(&overlay.height)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let demand = scene_canonical_integer::<u64>(&overlay.demand)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let routed = scene_canonical_integer::<u64>(&overlay.routed)
        .filter(|value| *value <= demand)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let weighted_length = scene_canonical_integer::<u128>(&overlay.weighted_length)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let weighted_units = scene_canonical_integer::<u64>(&overlay.weighted_length_units)
        .filter(|value| *value > 0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let bottleneck = scene_canonical_integer::<u64>(&overlay.active_bottleneck)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    for value in [
        &overlay.sparse_cut_level,
        &overlay.relabel_steps,
        &overlay.augmentations,
        &overlay.shortcut_traversals,
        &overlay.residual_rounds,
        &overlay.completion_relabel_steps,
        &overlay.completion_augmentations,
    ] {
        scene_canonical_integer::<u64>(value).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    }
    scene_canonical_integer::<u128>(&overlay.sparse_cut_capacity)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    if psi_numerator != 1
        || psi_denominator != 1
        || flows.len() != graph.edges().len()
        || (bottleneck == 0) != overlay.active_path.is_empty()
        || matches!(
            overlay.stage,
            FlowWeightedPushRelabelShortcutStageV1::AugmentPath
                | FlowWeightedPushRelabelShortcutStageV1::CompletionAugmentPath
        ) != (bottleneck > 0)
        || weighted_units != routed.max(1)
        || weighted_length > 0 && routed == 0
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if overlay.stage == FlowWeightedPushRelabelShortcutStageV1::Ready {
        if hierarchy_levels != 0
            || height != 0
            || demand != 0
            || routed != 0
            || overlay.nodes.len() != graph.nodes().len()
            || !overlay.edges.is_empty()
            || !overlay.residual_arcs.is_empty()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        return Ok(());
    }
    if hierarchy_levels != 1
        || height == 0
        || demand == 0
        || overlay.nodes.len() < graph.nodes().len()
        || overlay.edges.len() < graph.edges().len()
        || overlay.residual_arcs.len() != overlay.edges.len() * 2
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mut node_ids = BTreeMap::new();
    for (index, node) in overlay.nodes.iter().enumerate() {
        let component = scene_canonical_integer::<usize>(&node.component)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let order = scene_canonical_integer::<usize>(&node.order)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let label = scene_canonical_integer::<u64>(&node.label)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if component >= graph.nodes().len()
            || node_ids.insert(node.node_id.as_str(), index).is_some()
            || node.alive != (label <= height.saturating_mul(9))
            || (node.original && order == 0)
            || (!node.original && (order != 0 || !node.node_id.starts_with("shortcut:")))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if index < graph.nodes().len() {
            if !node.original || node.node_id != graph.nodes()[index].id().as_str() {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
        } else if node.original {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    let mut active = BTreeSet::new();
    for arc in &overlay.active_path {
        if (arc.direction != "forward" && arc.direction != "reverse")
            || !active.insert((arc.edge_id.as_str(), arc.direction.as_str()))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    let mut inspected = BTreeSet::new();
    for arc in &overlay.inspected_arcs {
        if (arc.direction != "forward" && arc.direction != "reverse")
            || !inspected.insert((arc.edge_id.as_str(), arc.direction.as_str()))
            || !active.insert((arc.edge_id.as_str(), arc.direction.as_str()))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    let inspection_checkpoint = matches!(
        overlay.stage,
        FlowWeightedPushRelabelShortcutStageV1::InspectPrimitiveArcCheckpoint
            | FlowWeightedPushRelabelShortcutStageV1::CompletionInspectPrimitiveArcCheckpoint
    );
    let relabel_checkpoint = matches!(
        overlay.stage,
        FlowWeightedPushRelabelShortcutStageV1::RelabelCheckpoint
            | FlowWeightedPushRelabelShortcutStageV1::CompletionRelabelCheckpoint
    );
    if inspection_checkpoint == overlay.inspected_arcs.is_empty()
        || relabel_checkpoint != (overlay.active_relabel_nodes.len() == 1)
        || overlay
            .active_relabel_nodes
            .iter()
            .any(|node| !node_ids.contains_key(node.as_str()))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mut residual_endpoints = BTreeMap::new();
    for (index, edge) in overlay.edges.iter().enumerate() {
        let capacity = scene_canonical_integer::<u64>(&edge.capacity)
            .filter(|value| *value > 0)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let flow = scene_canonical_integer::<u64>(&edge.flow)
            .filter(|value| *value <= capacity)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let weight = scene_canonical_integer::<u64>(&edge.weight)
            .filter(|value| *value > 0)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if !node_ids.contains_key(edge.from.as_str())
            || !node_ids.contains_key(edge.to.as_str())
            || edge.from == edge.to
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if index < graph.edges().len() {
            let original = &graph.edges()[index];
            let from = graph
                .node(original.from())
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?
                .id()
                .as_str();
            let to = graph
                .node(original.to())
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?
                .id()
                .as_str();
            if edge.kind != "original"
                || edge.edge_id != original.id().as_str()
                || edge.from != from
                || edge.to != to
                || capacity != original.capacity()
                || flow != flows[index]
                || edge.shortcut_component.is_some()
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
        } else if edge.kind != "shortcut"
            || !edge.edge_id.starts_with("shortcut-edge:")
            || edge
                .shortcut_component
                .as_deref()
                .is_none_or(|component| scene_canonical_integer::<usize>(component).is_none())
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        for direction_index in 0..2 {
            let arc = &overlay.residual_arcs[index * 2 + direction_index];
            let forward = direction_index == 0;
            let expected_direction = if forward { "forward" } else { "reverse" };
            let expected_from = if forward { &edge.from } else { &edge.to };
            let expected_to = if forward { &edge.to } else { &edge.from };
            let expected_capacity = if forward { capacity - flow } else { flow };
            let arc_capacity = scene_canonical_integer::<u64>(&arc.capacity)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let arc_weight = scene_canonical_integer::<u64>(&arc.weight)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let active_here = active.contains(&(arc.edge_id.as_str(), arc.direction.as_str()));
            if arc.edge_id != edge.edge_id
                || arc.direction != expected_direction
                || arc.from != *expected_from
                || arc.to != *expected_to
                || arc_capacity != expected_capacity
                || arc_weight != weight
                || arc.active != active_here
                || arc.admissible && arc_capacity == 0
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            residual_endpoints.insert(
                (arc.edge_id.as_str(), arc.direction.as_str()),
                (arc.from.as_str(), arc.to.as_str()),
            );
        }
    }
    let active_endpoints = overlay
        .active_path
        .iter()
        .map(|arc| {
            residual_endpoints
                .get(&(arc.edge_id.as_str(), arc.direction.as_str()))
                .copied()
                .ok_or(FlowSceneError::SnapshotGraphMismatch)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for pair in active_endpoints.windows(2) {
        if pair[0].1 != pair[1].0 {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    if overlay.inspected_arcs.iter().any(|arc| {
        !residual_endpoints.contains_key(&(arc.edge_id.as_str(), arc.direction.as_str()))
    }) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

const fn minimum_ratio_cycle_mcf_stage_rank(stage: FlowMinimumRatioCycleMcfStageV1) -> u8 {
    match stage {
        FlowMinimumRatioCycleMcfStageV1::Ready => 0,
        FlowMinimumRatioCycleMcfStageV1::EnumerateFeasibleSet => 1,
        FlowMinimumRatioCycleMcfStageV1::ContractFixedFace => 2,
        FlowMinimumRatioCycleMcfStageV1::InitializeStrictInterior => 3,
        FlowMinimumRatioCycleMcfStageV1::EvaluatePotential => 4,
        FlowMinimumRatioCycleMcfStageV1::MapGradientLength => 5,
        FlowMinimumRatioCycleMcfStageV1::BuildSpanningForest => 6,
        FlowMinimumRatioCycleMcfStageV1::InspectVector => 7,
        FlowMinimumRatioCycleMcfStageV1::EvaluateCycle => 8,
        FlowMinimumRatioCycleMcfStageV1::UpdateBest => 9,
        FlowMinimumRatioCycleMcfStageV1::VerifyCycleSpace => 10,
        FlowMinimumRatioCycleMcfStageV1::ApplySourceStep => 11,
        FlowMinimumRatioCycleMcfStageV1::MeasurePotentialDecrease => 12,
        FlowMinimumRatioCycleMcfStageV1::CheckDfsOracle => 13,
        FlowMinimumRatioCycleMcfStageV1::Complete => 14,
    }
}

fn active_component_count(graph: &FlowNetwork, fixed: &[bool]) -> usize {
    fn find(parent: &mut [usize], node: usize) -> usize {
        if parent[node] != node {
            parent[node] = find(parent, parent[node]);
        }
        parent[node]
    }
    let mut parent = (0..graph.nodes().len()).collect::<Vec<_>>();
    for (index, edge) in graph.edges().iter().enumerate() {
        if fixed[index] {
            continue;
        }
        let left = find(&mut parent, edge.from().as_usize());
        let right = find(&mut parent, edge.to().as_usize());
        if left != right {
            parent[right] = left;
        }
    }
    (0..parent.len())
        .map(|node| find(&mut parent, node))
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn validate_electrical_ipm_mcf_overlay(
    graph: &FlowNetwork,
    flows: &[u64],
    overlay: &FlowElectricalIpmMcfOverlayV1,
) -> Result<(), FlowSceneError> {
    if flows.len() != graph.edges().len()
        || graph.nodes().len() > crate::algorithms::ELECTRICAL_IPM_MCF_MAX_NODES
        || graph.edges().len() > crate::algorithms::ELECTRICAL_IPM_MCF_MAX_EDGES
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
        || scene_canonical_integer::<u64>(&overlay.seed).is_none()
        || scene_canonical_integer::<i128>(&overlay.isolation_scale).is_none()
        || scene_canonical_integer::<u64>(&overlay.perturbation_bound).is_none()
        || scene_canonical_integer::<u64>(&overlay.isolation_attempt).is_none()
        || scene_canonical_integer::<i128>(&overlay.isolated_optimum_cost).is_none()
        || scene_canonical_integer::<i128>(&overlay.isolated_gap).is_none()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mu = electrical_f64(&overlay.mu).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let epsilon_3 =
        electrical_f64(&overlay.epsilon_3).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let recovery =
        electrical_f64(&overlay.recovery_epsilon).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let gap =
        electrical_f64(&overlay.duality_gap_bound).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let centrality = electrical_f64(&overlay.centrality_residual)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let balance =
        electrical_f64(&overlay.balance_residual).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let step = electrical_f64(&overlay.step_size).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let energy =
        electrical_f64(&overlay.electrical_energy).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let linear =
        electrical_f64(&overlay.linear_residual).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    electrical_f64(&overlay.barrier_objective).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    if mu < 0.0
        || epsilon_3 < 0.0
        || recovery < 0.0
        || gap < 0.0
        || centrality < 0.0
        || balance < 0.0
        || !(0.0..=1.0).contains(&step)
        || energy < 0.0
        || linear < 0.0
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let isolated = !matches!(
        overlay.stage,
        FlowElectricalIpmMcfStageV1::Ready | FlowElectricalIpmMcfStageV1::NormalizeLowerBounds
    );
    let face_contracted = !matches!(
        overlay.stage,
        FlowElectricalIpmMcfStageV1::Ready
            | FlowElectricalIpmMcfStageV1::NormalizeLowerBounds
            | FlowElectricalIpmMcfStageV1::IsolationAttempt
            | FlowElectricalIpmMcfStageV1::SelectIsolatedCosts
    );
    let initialized = !matches!(
        overlay.stage,
        FlowElectricalIpmMcfStageV1::Ready
            | FlowElectricalIpmMcfStageV1::NormalizeLowerBounds
            | FlowElectricalIpmMcfStageV1::IsolationAttempt
            | FlowElectricalIpmMcfStageV1::SelectIsolatedCosts
            | FlowElectricalIpmMcfStageV1::ContractFixedFace
    );
    let rounded = matches!(
        overlay.stage,
        FlowElectricalIpmMcfStageV1::RoundNearestInteger
            | FlowElectricalIpmMcfStageV1::CheckCertificate
            | FlowElectricalIpmMcfStageV1::Optimal
    );
    let recovery_boundary = matches!(
        overlay.stage,
        FlowElectricalIpmMcfStageV1::ApproximateFlow
            | FlowElectricalIpmMcfStageV1::RoundNearestInteger
            | FlowElectricalIpmMcfStageV1::CheckCertificate
            | FlowElectricalIpmMcfStageV1::Optimal
    );
    let isolation_scale = overlay
        .isolation_scale
        .parse::<i128>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let perturbation_bound = overlay
        .perturbation_bound
        .parse::<u64>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let isolation_attempt = overlay
        .isolation_attempt
        .parse::<u64>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let isolated_gap = overlay
        .isolated_gap
        .parse::<i128>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let isolated_optimum_cost = overlay
        .isolated_optimum_cost
        .parse::<i128>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    if isolated != (isolation_scale > 0 && perturbation_bound > 0 && isolation_attempt > 0)
        || matches!(
            overlay.stage,
            FlowElectricalIpmMcfStageV1::SelectIsolatedCosts
                | FlowElectricalIpmMcfStageV1::ContractFixedFace
                | FlowElectricalIpmMcfStageV1::InitializeDualInterior
                | FlowElectricalIpmMcfStageV1::AssembleElectricalLaplacian
                | FlowElectricalIpmMcfStageV1::SolveNewtonDirection
                | FlowElectricalIpmMcfStageV1::DampedCenteringStep
                | FlowElectricalIpmMcfStageV1::Centered
                | FlowElectricalIpmMcfStageV1::DecreaseBarrier
                | FlowElectricalIpmMcfStageV1::ApproximateFlow
                | FlowElectricalIpmMcfStageV1::RoundNearestInteger
                | FlowElectricalIpmMcfStageV1::CheckCertificate
                | FlowElectricalIpmMcfStageV1::Optimal
        ) && isolated_gap <= 0
        || initialized && (epsilon_3 <= 0.0 || epsilon_3 >= 1.0 || recovery <= 0.0)
        || matches!(overlay.stage, FlowElectricalIpmMcfStageV1::Centered) && centrality > 2.01e-7
        || recovery_boundary && gap > recovery * (1.0 + 1.0e-8)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    for (state, node) in overlay.nodes.iter().zip(graph.nodes()) {
        if state.node_id != node.id().as_str()
            || electrical_f64(&state.potential).is_none()
            || electrical_f64(&state.potential_direction).is_none()
            || electrical_f64(&state.balance_residual).is_none()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    let mut working_edges = 0_usize;
    let mut rounded_isolated_cost = 0_i128;
    for ((state, edge), &published_flow) in overlay.edges.iter().zip(graph.edges()).zip(flows) {
        let perturbation = scene_canonical_integer::<u64>(&state.perturbation)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let isolated_cost = scene_canonical_integer::<i128>(&state.isolated_cost)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let face_lower = scene_canonical_integer::<u64>(&state.face_lower)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let face_upper = scene_canonical_integer::<u64>(&state.face_upper)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let fractional =
            electrical_f64(&state.fractional_flow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let complement =
            electrical_f64(&state.upper_complement).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let slack =
            electrical_f64(&state.lower_slack).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let multiplier =
            electrical_f64(&state.upper_multiplier).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let resistance =
            electrical_f64(&state.resistance).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let conductance =
            electrical_f64(&state.conductance).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        electrical_f64(&state.electrical_current)
            .zip(electrical_f64(&state.lower_slack_direction))
            .zip(electrical_f64(&state.upper_multiplier_direction))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if state.edge_id != edge.id().as_str()
            || face_lower < edge.lower()
            || face_upper > edge.capacity()
            || face_lower > face_upper
            || face_contracted && state.fixed_on_face != (face_lower == face_upper)
            || isolated
                && (perturbation == 0
                    || perturbation > perturbation_bound
                    || isolated_cost
                        != isolation_scale
                            .checked_mul(i128::from(edge.cost()))
                            .and_then(|value| value.checked_add(i128::from(perturbation)))
                            .ok_or(FlowSceneError::SnapshotGraphMismatch)?)
            || !isolated && perturbation != 0
            || complement < 0.0
            || resistance < 0.0
            || conductance < 0.0
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if face_contracted && !state.fixed_on_face {
            working_edges += 1;
            if initialized
                && (slack <= 0.0
                    || multiplier <= 0.0
                    || !electrical_close(fractional, face_lower as f64 + mu / slack)
                    || !electrical_close(complement, mu / multiplier))
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            if matches!(
                overlay.stage,
                FlowElectricalIpmMcfStageV1::AssembleElectricalLaplacian
                    | FlowElectricalIpmMcfStageV1::SolveNewtonDirection
            ) && (!electrical_close(resistance, (slack * slack + multiplier * multiplier) / mu)
                || !electrical_close(conductance, 1.0 / resistance))
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
        }
        match (&state.final_flow, rounded) {
            (Some(value), true) => {
                let final_flow = scene_canonical_integer::<u64>(value)
                    .filter(|value| *value == published_flow)
                    .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
                if final_flow < edge.lower()
                    || final_flow > edge.capacity()
                    || (fractional - final_flow as f64).abs() > 1.0 / 3.0 + 1.0e-8
                {
                    return Err(FlowSceneError::SnapshotGraphMismatch);
                }
                rounded_isolated_cost = isolated_cost
                    .checked_mul(i128::from(final_flow))
                    .and_then(|term| rounded_isolated_cost.checked_add(term))
                    .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            }
            (None, false) => {}
            _ => return Err(FlowSceneError::SnapshotGraphMismatch),
        }
    }
    if !electrical_close(gap, 2.0 * working_edges as f64 * mu)
        || rounded && rounded_isolated_cost != isolated_optimum_cost
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_primal_dual_ipm_mcf_overlay(
    graph: &FlowNetwork,
    flows: &[u64],
    overlay: &FlowPrimalDualIpmMcfOverlayV1,
) -> Result<(), FlowSceneError> {
    if flows.len() != graph.edges().len()
        || graph.nodes().len() > crate::algorithms::PRIMAL_DUAL_IPM_MCF_MAX_NODES
        || graph.edges().len() > crate::algorithms::PRIMAL_DUAL_IPM_MCF_MAX_EDGES
        || overlay.arcs.len() > crate::algorithms::PRIMAL_DUAL_IPM_MCF_MAX_AUXILIARY_ARCS
        || scene_canonical_integer::<u64>(&overlay.seed).is_none()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let inspecting_forest = overlay.stage == FlowPrimalDualIpmMcfStageV1::InspectForestSubset;
    let forest_serial_valid = overlay
        .forest_subset_serial
        .as_deref()
        .and_then(scene_canonical_integer::<u64>)
        .is_some_and(|value| value > 0);
    if inspecting_forest != forest_serial_valid {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mu = scene_canonical_bigint(&overlay.mu).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let beta = scene_canonical_bigint(&overlay.beta)
        .filter(|value| value > &BigInt::zero())
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let gamma = scene_canonical_bigint(&overlay.gamma)
        .filter(|value| value > &BigInt::zero())
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let proxy_gap = scene_canonical_bigint(&overlay.proxy_gap)
        .filter(|value| value >= &BigInt::zero())
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let centrality = scene_canonical_bigint(&overlay.centrality_numerator)
        .filter(|value| value >= &BigInt::zero())
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    scene_canonical_bigint(&overlay.cycle_alpha).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    if let Some(condition) = &overlay.tree_condition_number {
        scene_canonical_bigint(&condition.numerator)
            .filter(|value| value >= &BigInt::zero())
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        scene_canonical_bigint(&condition.denominator)
            .filter(|value| value > &BigInt::zero())
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    }
    let pre_auxiliary = matches!(
        overlay.stage,
        FlowPrimalDualIpmMcfStageV1::Ready | FlowPrimalDualIpmMcfStageV1::NormalizeInput
    );
    if pre_auxiliary
        != (overlay.arcs.is_empty() && overlay.nodes.len() == graph.nodes().len() && mu.is_zero())
        || !pre_auxiliary && overlay.nodes.len() < graph.nodes().len()
        || overlay.nodes.len() > graph.nodes().len() + graph.edges().len()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let initialized = !matches!(
        overlay.stage,
        FlowPrimalDualIpmMcfStageV1::Ready
            | FlowPrimalDualIpmMcfStageV1::NormalizeInput
            | FlowPrimalDualIpmMcfStageV1::BuildCapacityReduction
    );
    if initialized != (mu > BigInt::zero()) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if matches!(
        overlay.stage,
        FlowPrimalDualIpmMcfStageV1::ProxyReached
            | FlowPrimalDualIpmMcfStageV1::CrossoverGrowCut
            | FlowPrimalDualIpmMcfStageV1::RestoreOriginalDual
            | FlowPrimalDualIpmMcfStageV1::RecoverAdmissibleFlow
            | FlowPrimalDualIpmMcfStageV1::CheckCertificate
            | FlowPrimalDualIpmMcfStageV1::Optimal
    ) && &proxy_gap * 81 >= &beta * &gamma * 4
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if matches!(
        overlay.stage,
        FlowPrimalDualIpmMcfStageV1::Centered | FlowPrimalDualIpmMcfStageV1::ProxyReached
    ) && &centrality * 8 >= mu
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    let graph_nodes = graph
        .nodes()
        .iter()
        .map(|node| node.id().as_str())
        .collect::<BTreeSet<_>>();
    let graph_edges = graph
        .edges()
        .iter()
        .map(|edge| edge.id().as_str())
        .collect::<BTreeSet<_>>();
    let mut auxiliary_ids = BTreeSet::new();
    let mut seen_original_nodes = BTreeSet::new();
    let mut seen_capacity_edges = BTreeSet::new();
    for node in &overlay.nodes {
        if !auxiliary_ids.insert(node.auxiliary_id.as_str())
            || scene_canonical_bigint(&node.potential).is_none()
            || scene_canonical_integer::<u64>(&node.component).is_none()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        match node.kind {
            FlowPrimalDualIpmMcfNodeKindV1::Original => {
                let original = node
                    .original_node_id
                    .as_deref()
                    .filter(|value| graph_nodes.contains(value))
                    .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
                if node.original_edge_id.is_some() || !seen_original_nodes.insert(original) {
                    return Err(FlowSceneError::SnapshotGraphMismatch);
                }
            }
            FlowPrimalDualIpmMcfNodeKindV1::Capacity => {
                let original = node
                    .original_edge_id
                    .as_deref()
                    .filter(|value| graph_edges.contains(value))
                    .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
                if node.original_node_id.is_some() || !seen_capacity_edges.insert(original) {
                    return Err(FlowSceneError::SnapshotGraphMismatch);
                }
            }
        }
    }
    if seen_original_nodes.len() != graph.nodes().len() {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let crossover_stage = matches!(
        overlay.stage,
        FlowPrimalDualIpmMcfStageV1::CrossoverGrowCut
            | FlowPrimalDualIpmMcfStageV1::RestoreOriginalDual
            | FlowPrimalDualIpmMcfStageV1::RecoverAdmissibleFlow
            | FlowPrimalDualIpmMcfStageV1::CheckCertificate
            | FlowPrimalDualIpmMcfStageV1::Optimal
    );
    let minor_resistance_available = !matches!(
        overlay.stage,
        FlowPrimalDualIpmMcfStageV1::Ready
            | FlowPrimalDualIpmMcfStageV1::NormalizeInput
            | FlowPrimalDualIpmMcfStageV1::BuildCapacityReduction
            | FlowPrimalDualIpmMcfStageV1::InitializeCentralPoint
    );
    let mut auxiliary_arcs = BTreeSet::new();
    let mut projected_proxy_gap = BigInt::zero();
    let mut projected_centrality = BigInt::zero();
    let component_by_auxiliary = overlay
        .nodes
        .iter()
        .map(|node| (node.auxiliary_id.as_str(), node.component.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut cycle_divergence = overlay
        .nodes
        .iter()
        .map(|node| (node.component.as_str(), 0_i32))
        .collect::<BTreeMap<_, _>>();
    for arc in &overlay.arcs {
        let flow = scene_canonical_bigint(&arc.flow)
            .filter(|value| value >= &BigInt::zero())
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let slack = scene_canonical_bigint(&arc.slack)
            .filter(|value| value >= &BigInt::zero())
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let sign = scene_canonical_integer::<i8>(&arc.active_cycle_sign)
            .filter(|value| matches!(value, -1..=1))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if !auxiliary_arcs.insert(arc.auxiliary_id.as_str())
            || !graph_edges.contains(arc.original_edge_id.as_str())
            || !auxiliary_ids.contains(arc.from.as_str())
            || !auxiliary_ids.contains(arc.to.as_str())
            || arc.deleted && arc.contracted
            || arc.in_minor && (arc.deleted || arc.contracted)
            || arc.in_tree && !arc.in_minor && !crossover_stage
            || arc.forest_candidate && (!inspecting_forest || !arc.in_minor)
            || sign != 0 && !arc.in_minor
            || initialized && flow.is_zero()
            || initialized && !crossover_stage && slack.is_zero()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if let Some(resistance) = &arc.resistance {
            scene_canonical_bigint(resistance)
                .filter(|value| value > &BigInt::zero() && arc.in_minor)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        } else if arc.in_minor && minor_resistance_available {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        match arc.kind {
            FlowPrimalDualIpmMcfArcKindV1::Upper | FlowPrimalDualIpmMcfArcKindV1::Lower => {
                if !seen_capacity_edges.contains(arc.original_edge_id.as_str()) {
                    return Err(FlowSceneError::SnapshotGraphMismatch);
                }
            }
            FlowPrimalDualIpmMcfArcKindV1::Artificial => {}
        }
        if arc.in_minor {
            projected_proxy_gap += &flow * &slack;
            projected_centrality += (&flow * &slack - &mu).abs();
        }
        *cycle_divergence
            .get_mut(
                component_by_auxiliary
                    .get(arc.from.as_str())
                    .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
            )
            .ok_or(FlowSceneError::SnapshotGraphMismatch)? += i32::from(sign);
        *cycle_divergence
            .get_mut(
                component_by_auxiliary
                    .get(arc.to.as_str())
                    .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
            )
            .ok_or(FlowSceneError::SnapshotGraphMismatch)? -= i32::from(sign);
    }
    if overlay
        .sampled_arc
        .as_deref()
        .is_some_and(|arc| !auxiliary_arcs.contains(arc))
        || !crossover_stage
            && (projected_proxy_gap != proxy_gap || projected_centrality != centrality)
        || cycle_divergence.values().any(|value| *value != 0)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_orlin_max_flow_overlay(
    graph: &FlowNetwork,
    flows: &[u64],
    overlay: &FlowOrlinMaxFlowOverlayV1,
) -> Result<(), FlowSceneError> {
    if flows.len() != graph.edges().len()
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.residual_arcs.len() != graph.edges().len() * 2
        || overlay
            .gamma
            .denominator
            .parse::<u128>()
            .ok()
            .is_none_or(|value| value == 0)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let delta = overlay
        .delta
        .parse::<u128>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    overlay
        .gamma
        .numerator
        .parse::<u128>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;

    let mut component_state = BTreeMap::<String, (bool, String)>::new();
    let mut component_ids = BTreeSet::new();
    for (node, state) in graph.nodes().iter().zip(&overlay.nodes) {
        if state.node_id != node.id().as_str() || state.anti_potential.parse::<i128>().is_err() {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        component_ids.insert(state.component_id.clone());
        let expected = (state.critical, state.anti_potential.clone());
        if component_state
            .insert(state.component_id.clone(), expected.clone())
            .is_some_and(|previous| previous != expected)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    if component_ids.is_empty() {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    let mut inspected_residual = BTreeSet::new();
    for ((edge, &flow), pair) in graph
        .edges()
        .iter()
        .zip(flows)
        .zip(overlay.residual_arcs.chunks_exact(2))
    {
        let expected = [
            ("forward", edge.capacity() - flow),
            ("reverse", flow - edge.lower()),
        ];
        for (state, (direction, capacity)) in pair.iter().zip(expected) {
            if state.edge_id != edge.id().as_str()
                || state.direction != direction
                || state.capacity.parse::<u64>().ok() != Some(capacity)
                || state.inspection_serial.as_deref().is_some_and(|serial| {
                    serial.parse::<u128>().ok().is_none_or(|value| value == 0)
                })
                || (state.abundant && state.anti_abundant)
                || (state.small && state.medium)
                || (state.abundant && u128::from(capacity) < delta.saturating_mul(2))
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            if state.inspection_serial.is_some() {
                inspected_residual.insert((state.edge_id.as_str(), state.direction.as_str()));
            }
        }
    }

    let mut compact_ordinals = BTreeSet::new();
    let mut inspected_compact = BTreeSet::new();
    for (ordinal, arc) in overlay.compact_arcs.iter().enumerate() {
        let ordinal_text = ordinal.to_string();
        let capacity = arc
            .capacity
            .parse::<u128>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let flow = arc
            .flow
            .parse::<u128>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        if arc.ordinal != ordinal_text
            || !compact_ordinals.insert(ordinal_text)
            || arc.from_component == arc.to_component
            || !component_ids.contains(&arc.from_component)
            || !component_ids.contains(&arc.to_component)
            || flow > capacity
            || arc
                .inspection_serial
                .as_deref()
                .is_some_and(|serial| serial.parse::<u128>().ok().is_none_or(|value| value == 0))
            || arc.witness.is_empty()
            || arc
                .witness
                .iter()
                .any(|reference| !valid_residual_ref(graph, reference))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if arc.inspection_serial.is_some() {
            inspected_compact.insert(arc.ordinal.as_str());
        }
    }
    if overlay.active_compact_path.iter().any(|reference| {
        reference
            .ordinal
            .parse::<usize>()
            .ok()
            .is_none_or(|ordinal| ordinal >= overlay.compact_arcs.len())
    }) || overlay
        .active_original_path
        .iter()
        .any(|reference| !valid_residual_ref(graph, reference))
        || overlay.threshold.parse::<u128>().is_err()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let requires_case = matches!(
        overlay.stage,
        FlowOrlinMaxFlowStageV1::SelectCase
            | FlowOrlinMaxFlowStageV1::InspectCompactConstructionArc
            | FlowOrlinMaxFlowStageV1::TransferCapacity
            | FlowOrlinMaxFlowStageV1::BuildSubproblem
            | FlowOrlinMaxFlowStageV1::AugmentSubproblem
            | FlowOrlinMaxFlowStageV1::InspectSubproblemArc
            | FlowOrlinMaxFlowStageV1::CompleteSubproblem
            | FlowOrlinMaxFlowStageV1::InspectDecompositionArc
            | FlowOrlinMaxFlowStageV1::InspectLiftResidualArc
            | FlowOrlinMaxFlowStageV1::LiftPath
            | FlowOrlinMaxFlowStageV1::ExpandContraction
            | FlowOrlinMaxFlowStageV1::InspectExpansionResidualArc
            | FlowOrlinMaxFlowStageV1::InspectCutResidualArc
            | FlowOrlinMaxFlowStageV1::UpdateCut
    );
    if requires_case != overlay.phase_case.is_some()
        || (!matches!(
            overlay.stage,
            FlowOrlinMaxFlowStageV1::AugmentSubproblem
                | FlowOrlinMaxFlowStageV1::InspectSubproblemArc
                | FlowOrlinMaxFlowStageV1::InspectDecompositionArc
                | FlowOrlinMaxFlowStageV1::InspectLiftResidualArc
                | FlowOrlinMaxFlowStageV1::LiftPath
        ) && !overlay.active_compact_path.is_empty())
        || (!matches!(
            overlay.stage,
            FlowOrlinMaxFlowStageV1::InspectClassificationArc
                | FlowOrlinMaxFlowStageV1::InspectCompactConstructionArc
                | FlowOrlinMaxFlowStageV1::TransferCapacity
                | FlowOrlinMaxFlowStageV1::InspectLiftResidualArc
                | FlowOrlinMaxFlowStageV1::LiftPath
                | FlowOrlinMaxFlowStageV1::ExpandContraction
                | FlowOrlinMaxFlowStageV1::InspectExpansionResidualArc
                | FlowOrlinMaxFlowStageV1::InspectCutResidualArc
        ) && !overlay.active_original_path.is_empty())
        || (overlay.stage == FlowOrlinMaxFlowStageV1::Optimal && delta != 0)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let expected_compact = overlay
        .active_compact_path
        .iter()
        .map(|reference| reference.ordinal.as_str())
        .collect::<BTreeSet<_>>();
    let expected_residual = overlay
        .active_original_path
        .iter()
        .map(|reference| (reference.edge_id.as_str(), reference.direction.as_str()))
        .collect::<BTreeSet<_>>();
    let compact_inspection = matches!(
        overlay.stage,
        FlowOrlinMaxFlowStageV1::InspectSubproblemArc
            | FlowOrlinMaxFlowStageV1::InspectDecompositionArc
    );
    let residual_inspection = matches!(
        overlay.stage,
        FlowOrlinMaxFlowStageV1::InspectClassificationArc
            | FlowOrlinMaxFlowStageV1::InspectCompactConstructionArc
            | FlowOrlinMaxFlowStageV1::InspectLiftResidualArc
            | FlowOrlinMaxFlowStageV1::InspectExpansionResidualArc
            | FlowOrlinMaxFlowStageV1::InspectCutResidualArc
    );
    if (compact_inspection && inspected_compact != expected_compact)
        || (!compact_inspection && !inspected_compact.is_empty())
        || (residual_inspection && inspected_residual != expected_residual)
        || (!residual_inspection && !inspected_residual.is_empty())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

fn valid_residual_ref(graph: &FlowNetwork, reference: &FlowResidualArcRefV1) -> bool {
    matches!(reference.direction.as_str(), "forward" | "reverse")
        && graph
            .edges()
            .iter()
            .any(|edge| edge.id().as_str() == reference.edge_id)
}

fn validate_dual_network_simplex_overlay(
    graph: &FlowNetwork,
    overlay: &FlowDualNetworkSimplexOverlayV1,
) -> Result<(), FlowSceneError> {
    if overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
        || overlay
            .nodes
            .iter()
            .zip(graph.nodes())
            .any(|(state, node)| state.node_id != node.id().as_str())
        || overlay
            .edges
            .iter()
            .zip(graph.edges())
            .any(|(state, edge)| state.edge_id != edge.id().as_str())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let potentials = overlay
        .nodes
        .iter()
        .map(|state| {
            state
                .potential
                .parse::<i128>()
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let cut = overlay.cut_side.iter().cloned().collect::<BTreeSet<_>>();
    if cut.len() != overlay.cut_side.len()
        || overlay
            .nodes
            .iter()
            .any(|state| state.in_cut != cut.contains(&state.node_id))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let initialized_count = overlay
        .nodes
        .iter()
        .filter(|state| state.initialized)
        .count();
    if (overlay.stage == FlowDualNetworkSimplexStageV1::Ready && initialized_count != 0)
        || (overlay.stage == FlowDualNetworkSimplexStageV1::InspectInitialArc
            && initialized_count == 0)
        || (!matches!(
            overlay.stage,
            FlowDualNetworkSimplexStageV1::Ready | FlowDualNetworkSimplexStageV1::InspectInitialArc
        ) && initialized_count != graph.nodes().len())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mut tree_count = 0_usize;
    for (state, edge) in overlay.edges.iter().zip(graph.edges()) {
        let basic_flow = state
            .basic_flow
            .parse::<i128>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let reduced_cost = state
            .reduced_cost
            .parse::<i128>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let expected = i128::from(edge.cost())
            .checked_add(potentials[edge.from().as_usize()])
            .and_then(|value| value.checked_sub(potentials[edge.to().as_usize()]))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if reduced_cost != expected || (!state.in_tree && basic_flow != 0) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if state.in_tree {
            tree_count += 1;
        }
        if !matches!(
            overlay.stage,
            FlowDualNetworkSimplexStageV1::Ready | FlowDualNetworkSimplexStageV1::InspectInitialArc
        ) && (reduced_cost < 0 || (state.in_tree && reduced_cost != 0))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    if (overlay.stage == FlowDualNetworkSimplexStageV1::Ready && tree_count != 0)
        || (overlay.stage == FlowDualNetworkSimplexStageV1::InspectInitialArc
            && tree_count >= graph.nodes().len())
        || (!matches!(
            overlay.stage,
            FlowDualNetworkSimplexStageV1::Ready | FlowDualNetworkSimplexStageV1::InspectInitialArc
        ) && tree_count != graph.nodes().len().saturating_sub(1))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    validate_dual_network_simplex_selection(overlay)
}

fn validate_dual_network_simplex_selection(
    overlay: &FlowDualNetworkSimplexOverlayV1,
) -> Result<(), FlowSceneError> {
    let edge = |id: &str| overlay.edges.iter().find(|state| state.edge_id == id);
    let leaving = overlay.leaving_edge.as_deref().and_then(edge);
    let entering = overlay.entering_edge.as_deref().and_then(edge);
    let inspected = overlay.inspected_edge.as_deref().and_then(edge);
    if overlay.leaving_edge.is_some() != leaving.is_some()
        || overlay.entering_edge.is_some() != entering.is_some()
        || overlay.inspected_edge.is_some() != inspected.is_some()
        || overlay
            .pivot_price_delta
            .as_ref()
            .is_some_and(|value| value.parse::<i128>().ok().is_none_or(|delta| delta < 0))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let valid = match overlay.stage {
        FlowDualNetworkSimplexStageV1::Ready
        | FlowDualNetworkSimplexStageV1::InitializeDualTree
        | FlowDualNetworkSimplexStageV1::Optimal => {
            overlay.cut_side.is_empty()
                && leaving.is_none()
                && entering.is_none()
                && inspected.is_none()
                && overlay.pivot_price_delta.is_none()
        }
        FlowDualNetworkSimplexStageV1::InspectInitialArc => {
            overlay.cut_side.is_empty()
                && leaving.is_none()
                && entering.is_none()
                && inspected.is_some()
                && overlay.pivot_price_delta.is_none()
        }
        FlowDualNetworkSimplexStageV1::SelectLeaving => {
            !overlay.cut_side.is_empty()
                && leaving.is_some_and(|state| {
                    state.in_tree && state.basic_flow.parse::<i128>().is_ok_and(|flow| flow < 0)
                })
                && entering.is_none()
                && inspected.is_none()
                && overlay.pivot_price_delta.is_none()
        }
        FlowDualNetworkSimplexStageV1::InspectEnteringArc => {
            !overlay.cut_side.is_empty()
                && leaving.is_some_and(|state| state.in_tree)
                && inspected.is_some()
                && entering.is_none_or(|state| !state.in_tree)
                && (entering.is_some() == overlay.pivot_price_delta.is_some())
        }
        FlowDualNetworkSimplexStageV1::SelectEntering => {
            !overlay.cut_side.is_empty()
                && leaving.is_some_and(|state| state.in_tree)
                && entering.is_some_and(|state| !state.in_tree)
                && inspected.is_none()
                && overlay
                    .pivot_price_delta
                    .as_ref()
                    .is_some_and(|delta| entering.is_some_and(|state| state.reduced_cost == *delta))
        }
        FlowDualNetworkSimplexStageV1::Pivot => {
            !overlay.cut_side.is_empty()
                && leaving.is_some_and(|state| !state.in_tree)
                && entering.is_some_and(|state| state.in_tree && state.reduced_cost == "0")
                && inspected.is_none()
                && overlay.pivot_price_delta.is_some()
        }
    };
    if valid {
        Ok(())
    } else {
        Err(FlowSceneError::SnapshotGraphMismatch)
    }
}

fn validate_polynomial_dual_simplex_overlay(
    graph: &FlowNetwork,
    overlay: &FlowPolynomialDualSimplexOverlayV1,
) -> Result<(), FlowSceneError> {
    if overlay.phase.parse::<u64>().is_err()
        || parse_scene_rational(&overlay.delta).is_none_or(|delta| delta < BigRational::zero())
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
        || overlay
            .nodes
            .iter()
            .zip(graph.nodes())
            .any(|(state, node)| state.node_id != node.id().as_str())
        || overlay
            .edges
            .iter()
            .zip(graph.edges())
            .any(|(state, edge)| state.edge_id != edge.id().as_str())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let potentials = overlay
        .nodes
        .iter()
        .map(|state| {
            state
                .potential
                .parse::<i128>()
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let bad_nodes = unique_scene_ids(&overlay.bad_nodes)?;
    let bad_edges = unique_scene_ids(&overlay.bad_edges)?;
    let pivot_cut = unique_scene_ids(&overlay.pivot_cut)?;
    let pre_tree = matches!(
        overlay.stage,
        FlowPolynomialDualSimplexStageV1::Ready
            | FlowPolynomialDualSimplexStageV1::InspectInitialArc
    );
    let active_count = overlay.nodes.iter().filter(|state| state.active).count();
    let root_count = overlay.nodes.iter().filter(|state| state.root).count();
    if root_count != 1
        || overlay.nodes.first().is_none_or(|state| !state.root)
        || active_count != usize::from(overlay.active_node.is_some())
        || overlay.nodes.iter().any(|state| {
            parse_scene_rational(&state.excess).is_none()
                || state.bad != bad_nodes.contains(&state.node_id)
                || state.in_pivot_cut != pivot_cut.contains(&state.node_id)
                || state.active != (overlay.active_node.as_deref() == Some(&state.node_id))
        })
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }

    let mut tree = vec![false; graph.edges().len()];
    let mut pseudoflows = Vec::with_capacity(graph.edges().len());
    for (index, (state, edge)) in overlay.edges.iter().zip(graph.edges()).enumerate() {
        let flow =
            parse_scene_rational(&state.pseudoflow).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let basic = state
            .basic_flow
            .parse::<i128>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let reduced = state
            .reduced_cost
            .parse::<i128>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let expected = i128::from(edge.cost())
            .checked_add(potentials[edge.from().as_usize()])
            .and_then(|value| value.checked_sub(potentials[edge.to().as_usize()]))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if flow < BigRational::zero()
            || !state.in_tree && (!flow.is_zero() || basic != 0)
            || reduced != expected
            || state.bad != bad_edges.contains(&state.edge_id)
            || state.in_augment_path != state.augment_direction.is_some()
            || state
                .augment_direction
                .as_deref()
                .is_some_and(|value| !matches!(value, "forward" | "reverse"))
            || !pre_tree && (reduced < 0 || state.in_tree && reduced != 0)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        tree[index] = state.in_tree;
        pseudoflows.push(flow);
    }
    let tree_count = tree.iter().filter(|value| **value).count();
    if (pre_tree && tree_count != 0)
        || (!pre_tree
            && (tree_count != graph.nodes().len().saturating_sub(1)
                || !scene_tree_is_connected(graph, &tree)))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if !pre_tree {
        let (expected_bad_edges, expected_bad_nodes) =
            polynomial_dual_bad_sets(graph, &tree, &pseudoflows)?;
        if expected_bad_edges != bad_edges || expected_bad_nodes != bad_nodes {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    validate_polynomial_dual_selection(graph, overlay, &tree)
}

fn unique_scene_ids(values: &[String]) -> Result<BTreeSet<String>, FlowSceneError> {
    let set = values.iter().cloned().collect::<BTreeSet<_>>();
    if set.len() == values.len() {
        Ok(set)
    } else {
        Err(FlowSceneError::SnapshotGraphMismatch)
    }
}

fn scene_tree_is_connected(graph: &FlowNetwork, tree: &[bool]) -> bool {
    if graph.nodes().is_empty() {
        return false;
    }
    let mut reached = vec![false; graph.nodes().len()];
    let mut pending = vec![0_usize];
    while let Some(node) = pending.pop() {
        if reached[node] {
            continue;
        }
        reached[node] = true;
        for (index, edge) in graph.edges().iter().enumerate() {
            if !tree[index] {
                continue;
            }
            if edge.from().as_usize() == node {
                pending.push(edge.to().as_usize());
            } else if edge.to().as_usize() == node {
                pending.push(edge.from().as_usize());
            }
        }
    }
    reached.into_iter().all(|value| value)
}

fn polynomial_dual_bad_sets(
    graph: &FlowNetwork,
    tree: &[bool],
    pseudoflows: &[BigRational],
) -> Result<(BTreeSet<String>, BTreeSet<String>), FlowSceneError> {
    let node_count = graph.nodes().len();
    let mut parent = vec![None; node_count];
    let mut parent_edge = vec![None; node_count];
    let mut pending = VecDeque::from([0_usize]);
    parent[0] = Some(0);
    while let Some(node) = pending.pop_front() {
        for (index, edge) in graph.edges().iter().enumerate() {
            if !tree[index] {
                continue;
            }
            let next = if edge.from().as_usize() == node {
                Some(edge.to().as_usize())
            } else if edge.to().as_usize() == node {
                Some(edge.from().as_usize())
            } else {
                None
            };
            if let Some(next) = next.filter(|&next| parent[next].is_none()) {
                parent[next] = Some(node);
                parent_edge[next] = Some(index);
                pending.push_back(next);
            }
        }
    }
    if parent.iter().any(Option::is_none) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mut bad_mask = vec![false; graph.edges().len()];
    let mut bad_edges = BTreeSet::new();
    for node in 1..node_count {
        let edge_index = parent_edge[node].ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let edge = &graph.edges()[edge_index];
        if edge.from().as_usize() == parent[node].unwrap_or(usize::MAX)
            && edge.to().as_usize() == node
            && pseudoflows[edge_index].is_zero()
        {
            bad_mask[edge_index] = true;
            bad_edges.insert(edge.id().as_str().to_owned());
        }
    }
    let mut bad_nodes = BTreeSet::new();
    for node in 0..node_count {
        let mut cursor = node;
        while parent[cursor] != Some(cursor) {
            let edge_index = parent_edge[cursor].ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            if bad_mask[edge_index] {
                bad_nodes.insert(graph.nodes()[node].id().as_str().to_owned());
                break;
            }
            cursor = parent[cursor].ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        }
    }
    Ok((bad_edges, bad_nodes))
}

fn validate_polynomial_dual_selection(
    graph: &FlowNetwork,
    overlay: &FlowPolynomialDualSimplexOverlayV1,
    tree: &[bool],
) -> Result<(), FlowSceneError> {
    let edge_index = |id: &str| {
        graph
            .edges()
            .iter()
            .position(|edge| edge.id().as_str() == id)
    };
    let stage_active = matches!(
        overlay.stage,
        FlowPolynomialDualSimplexStageV1::SelectActive
            | FlowPolynomialDualSimplexStageV1::AugmentToRoot
    );
    let inspection = matches!(
        overlay.stage,
        FlowPolynomialDualSimplexStageV1::InspectAugmentationArc
            | FlowPolynomialDualSimplexStageV1::InspectEnteringArc
    );
    let active = overlay.active_node.is_some();
    if (!inspection && stage_active != active)
        || active == overlay.augment_path.is_empty()
        || overlay.augment_path.iter().any(|reference| {
            edge_index(&reference.edge_id).is_none_or(|index| !tree[index])
                || !matches!(reference.direction.as_str(), "forward" | "reverse")
        })
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let path_by_edge = overlay
        .augment_path
        .iter()
        .map(|reference| (reference.edge_id.as_str(), reference.direction.as_str()))
        .collect::<BTreeMap<_, _>>();
    if path_by_edge.len() != overlay.augment_path.len()
        || overlay.edges.iter().any(|state| {
            state.in_augment_path != path_by_edge.contains_key(state.edge_id.as_str())
                || state.augment_direction.as_deref()
                    != path_by_edge.get(state.edge_id.as_str()).copied()
        })
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if active {
        let mut cursor = graph
            .nodes()
            .iter()
            .position(|node| Some(node.id().as_str()) == overlay.active_node.as_deref())
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        for reference in &overlay.augment_path {
            let index =
                edge_index(&reference.edge_id).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let edge = &graph.edges()[index];
            cursor = match reference.direction.as_str() {
                "forward" if edge.from().as_usize() == cursor => edge.to().as_usize(),
                "reverse" if edge.to().as_usize() == cursor => edge.from().as_usize(),
                _ => return Err(FlowSceneError::SnapshotGraphMismatch),
            };
        }
        if cursor != 0 {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    let leaving = overlay.leaving_edge.as_deref().and_then(edge_index);
    let entering = overlay.entering_edge.as_deref().and_then(edge_index);
    let stage_has_leaving = matches!(
        overlay.stage,
        FlowPolynomialDualSimplexStageV1::SelectBadArc
            | FlowPolynomialDualSimplexStageV1::SelectEntering
            | FlowPolynomialDualSimplexStageV1::PivotMakeGood
    );
    let stage_has_entering = matches!(
        overlay.stage,
        FlowPolynomialDualSimplexStageV1::SelectEntering
            | FlowPolynomialDualSimplexStageV1::PivotMakeGood
    );
    let has_leaving = leaving.is_some();
    let has_entering = entering.is_some();
    let selection_mismatch =
        !inspection && (stage_has_leaving != has_leaving || stage_has_entering != has_entering);
    if selection_mismatch
        || has_entering && !has_leaving
        || has_leaving == overlay.pivot_cut.is_empty()
        || has_entering != overlay.pivot_price_delta.is_some()
        || overlay
            .pivot_price_delta
            .as_ref()
            .is_some_and(|value| value.parse::<i128>().ok().is_none_or(|value| value < 0))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if overlay.stage == FlowPolynomialDualSimplexStageV1::SelectBadArc
        || overlay.stage == FlowPolynomialDualSimplexStageV1::SelectEntering
    {
        if leaving.is_none_or(|index| !tree[index]) || entering.is_some_and(|index| tree[index]) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    } else if overlay.stage == FlowPolynomialDualSimplexStageV1::PivotMakeGood
        && (leaving.is_none_or(|index| tree[index]) || entering.is_none_or(|index| !tree[index]))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

fn validate_polynomial_primal_simplex_overlay(
    graph: &FlowNetwork,
    overlay: &FlowPolynomialPrimalSimplexOverlayV1,
) -> Result<(), FlowSceneError> {
    let original_nodes = graph.nodes().len();
    if overlay.nodes.len() != original_nodes + 1
        || overlay.edges.len() != graph.edges().len()
        || overlay.artificial_edges.len() != original_nodes
        || overlay.perturbation_scale.parse::<usize>().ok() != Some(original_nodes + 1)
        || overlay.phase.parse::<u64>().is_err()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    validate_polynomial_primal_nodes(graph, overlay)?;
    let valid_entities = validate_polynomial_primal_edges(graph, overlay)?;
    validate_polynomial_primal_selection(overlay, &valid_entities)
}

fn validate_polynomial_primal_nodes(
    graph: &FlowNetwork,
    overlay: &FlowPolynomialPrimalSimplexOverlayV1,
) -> Result<(), FlowSceneError> {
    let original_count = graph.nodes().len();
    let mut root_count = 0_usize;
    for (index, state) in overlay.nodes.iter().enumerate() {
        let expected = if index == original_count {
            "artificial-root"
        } else {
            graph.nodes()[index].id().as_str()
        };
        let flags = state.flags.iter().copied().collect::<BTreeSet<_>>();
        if state.entity_id != expected
            || state.kind
                != if index == original_count {
                    FlowPolynomialPrimalNodeKindV1::ArtificialRoot
                } else {
                    FlowPolynomialPrimalNodeKindV1::Original
                }
            || flags.len() != state.flags.len()
            || parse_scene_rational(&state.premultiplier).is_none()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        root_count += usize::from(flags.contains(&FlowPolynomialPrimalNodeFlagV1::Root));
    }
    if root_count != 1
        || overlay
            .epsilon
            .as_ref()
            .is_some_and(|value| parse_scene_rational(value).is_none())
        || matches!(
            overlay.stage,
            FlowPolynomialPrimalSimplexStageV1::BeginScale
                | FlowPolynomialPrimalSimplexStageV1::SelectAdmissible
                | FlowPolynomialPrimalSimplexStageV1::Pivot
                | FlowPolynomialPrimalSimplexStageV1::ModifyPremultipliers
                | FlowPolynomialPrimalSimplexStageV1::FinishScale
        ) && overlay.epsilon.is_none()
        || matches!(
            overlay.stage,
            FlowPolynomialPrimalSimplexStageV1::Ready
                | FlowPolynomialPrimalSimplexStageV1::InitializeBasis
        ) && overlay.epsilon.is_some()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

fn validate_polynomial_primal_edges(
    graph: &FlowNetwork,
    overlay: &FlowPolynomialPrimalSimplexOverlayV1,
) -> Result<BTreeSet<String>, FlowSceneError> {
    let potentials = overlay
        .nodes
        .iter()
        .map(|state| parse_scene_rational(&state.premultiplier))
        .collect::<Option<Vec<_>>>()
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let scale = i128::try_from(graph.nodes().len() + 1)
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let mut entities = BTreeSet::new();
    let mut tree_count = 0_usize;
    let artificial_root = graph.nodes().len();
    let mut tree_adjacency = vec![Vec::new(); artificial_root + 1];
    for (state, edge) in overlay.edges.iter().zip(graph.edges()) {
        if state.edge_id != edge.id().as_str()
            || !entities.insert(state.edge_id.clone())
            || state.perturbed_flow.parse::<i128>().is_err()
            || state.unperturbed_basic_flow.parse::<i128>().is_err()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let reduced = parse_scene_rational(&state.reduced_cost)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let expected = BigRational::from_integer(BigInt::from(edge.cost()))
            - &potentials[edge.from().as_usize()]
            + &potentials[edge.to().as_usize()];
        let flow = state
            .perturbed_flow
            .parse::<i128>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let capacity = i128::from(edge.capacity() - edge.lower())
            .checked_mul(scale)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let basis_valid = match state.basis {
            FlowPolynomialPrimalBasisStateV1::Lower => flow == 0,
            FlowPolynomialPrimalBasisStateV1::Tree => flow > 0 && flow < capacity,
            FlowPolynomialPrimalBasisStateV1::Upper => flow == capacity,
        };
        if reduced != expected || !basis_valid {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if state.basis == FlowPolynomialPrimalBasisStateV1::Tree {
            tree_count += 1;
            let from = edge.from().as_usize();
            let to = edge.to().as_usize();
            tree_adjacency[from].push(to);
            tree_adjacency[to].push(from);
        }
    }
    for (node_index, (state, node)) in overlay
        .artificial_edges
        .iter()
        .zip(graph.nodes())
        .enumerate()
    {
        let expected = format!("artificial:{}", node.id().as_str());
        if state.entity_id != expected
            || state.node_id != node.id().as_str()
            || !entities.insert(state.entity_id.clone())
            || state.perturbed_flow.parse::<i128>().is_err()
            || state.unperturbed_basic_flow.parse::<i128>().is_err()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let flow = state
            .perturbed_flow
            .parse::<i128>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        if flow < 0 || matches!(state.basis, FlowPolynomialPrimalBasisStateV1::Lower) && flow != 0 {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if state.basis == FlowPolynomialPrimalBasisStateV1::Tree {
            tree_count += 1;
            tree_adjacency[node_index].push(artificial_root);
            tree_adjacency[artificial_root].push(node_index);
        }
    }
    if tree_count != graph.nodes().len() {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let mut reached = vec![false; tree_adjacency.len()];
    let mut pending = vec![artificial_root];
    while let Some(node) = pending.pop() {
        if reached[node] {
            continue;
        }
        reached[node] = true;
        pending.extend(tree_adjacency[node].iter().copied());
    }
    if reached.iter().any(|&value| !value) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(entities)
}

fn validate_polynomial_primal_selection(
    overlay: &FlowPolynomialPrimalSimplexOverlayV1,
    valid_entities: &BTreeSet<String>,
) -> Result<(), FlowSceneError> {
    let valid_ref = |reference: &FlowPolynomialPrimalResidualRefV1| {
        valid_entities.contains(&reference.entity_id)
            && matches!(reference.direction.as_str(), "forward" | "reverse")
            && reference.original_edge_id.as_ref().is_none_or(|edge| {
                edge == &reference.entity_id
                    && overlay.edges.iter().any(|state| state.edge_id == *edge)
            })
            && (reference.entity_id.starts_with("artificial:")
                != reference.original_edge_id.is_some())
    };
    if overlay
        .entering
        .as_ref()
        .is_some_and(|value| !valid_ref(value))
        || overlay.cycle.iter().any(|value| !valid_ref(value))
        || overlay
            .leaving_entity
            .as_ref()
            .is_some_and(|value| !valid_entities.contains(value))
        || overlay
            .delta
            .as_ref()
            .is_some_and(|value| parse_scene_rational(value).is_none())
        || overlay
            .potential_shift
            .as_ref()
            .is_some_and(|value| parse_scene_rational(value).is_none())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let selection = matches!(
        overlay.stage,
        FlowPolynomialPrimalSimplexStageV1::SelectAdmissible
            | FlowPolynomialPrimalSimplexStageV1::Pivot
    );
    if selection != (overlay.entering.is_some() && !overlay.cycle.is_empty())
        || (overlay.stage == FlowPolynomialPrimalSimplexStageV1::Pivot) != overlay.delta.is_some()
        || (overlay.stage == FlowPolynomialPrimalSimplexStageV1::ModifyPremultipliers)
            != overlay.potential_shift.is_some()
        || (!selection && overlay.leaving_entity.is_some())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let cycle_entities = overlay
        .cycle
        .iter()
        .map(|reference| reference.entity_id.as_str())
        .collect::<BTreeSet<_>>();
    for state in &overlay.edges {
        if state.in_cycle != cycle_entities.contains(state.edge_id.as_str())
            || state.entering
                != overlay
                    .entering
                    .as_ref()
                    .is_some_and(|value| value.entity_id == state.edge_id)
            || state.leaving
                != overlay
                    .leaving_entity
                    .as_ref()
                    .is_some_and(|value| value == &state.edge_id)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    for state in &overlay.artificial_edges {
        if state.in_cycle != cycle_entities.contains(state.entity_id.as_str())
            || state.entering
                != overlay
                    .entering
                    .as_ref()
                    .is_some_and(|value| value.entity_id == state.entity_id)
            || state.leaving
                != overlay
                    .leaving_entity
                    .as_ref()
                    .is_some_and(|value| value == &state.entity_id)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    Ok(())
}

fn parse_scene_rational(value: &FlowRationalV1) -> Option<BigRational> {
    let numerator = value.numerator.parse::<BigInt>().ok()?;
    let denominator = value.denominator.parse::<BigInt>().ok()?;
    if denominator <= BigInt::zero() {
        return None;
    }
    let rational = BigRational::new(numerator, denominator);
    (rational.numer().to_string() == value.numerator
        && rational.denom().to_string() == value.denominator)
        .then_some(rational)
}

fn equal_unsigned_rationals(left: &FlowRationalV1, right: &FlowRationalV1) -> bool {
    let parsed = || {
        Some((
            left.numerator.parse::<u128>().ok()?,
            left.denominator
                .parse::<u128>()
                .ok()
                .filter(|&value| value > 0)?,
            right.numerator.parse::<u128>().ok()?,
            right
                .denominator
                .parse::<u128>()
                .ok()
                .filter(|&value| value > 0)?,
        ))
    };
    parsed().is_some_and(
        |(left_numerator, left_denominator, right_numerator, right_denominator)| {
            left_numerator
                .checked_mul(right_denominator)
                .zip(right_numerator.checked_mul(left_denominator))
                .is_some_and(|(left_product, right_product)| left_product == right_product)
        },
    )
}

fn validate_tardos_framework_overlay(
    graph: &FlowNetwork,
    flows: &[u64],
    overlay: &FlowTardosFrameworkOverlayV1,
) -> Result<(), FlowSceneError> {
    if flows.len() != graph.edges().len()
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.determinant_bound != "1"
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    ResidualState::from_flows(graph, flows).map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let potentials = overlay
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            if node.node_id != graph.nodes()[index].id().as_str() {
                return None;
            }
            prediction_scene_i128(&node.potential)
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let measured = matches!(
        overlay.stage,
        FlowTardosFrameworkStageV1::MeasureEpsilon
            | FlowTardosFrameworkStageV1::ClassifyFixedVariables
            | FlowTardosFrameworkStageV1::Complete
    );
    if !measured {
        if overlay.epsilon != "0"
            || overlay.threshold != "0"
            || !overlay.residual_arcs.is_empty()
            || !overlay.fixed_variables.is_empty()
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        return Ok(());
    }
    let expected = tardos_expected_residual_arcs(graph, flows, &potentials)?;
    if overlay.residual_arcs.len() > expected.len()
        || expected.windows(2).any(|pair| pair[0].0 == pair[1].0)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let epsilon = expected
        .iter()
        .filter_map(|(_, _, reduced)| reduced.checked_neg())
        .max()
        .unwrap_or(0)
        .max(0);
    let threshold = epsilon
        .checked_mul(
            i128::try_from(graph.nodes().len())
                .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?,
        )
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let classified = matches!(
        overlay.stage,
        FlowTardosFrameworkStageV1::ClassifyFixedVariables | FlowTardosFrameworkStageV1::Complete
    );
    if classified && overlay.residual_arcs.len() != expected.len() {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let scanned_epsilon = expected
        .iter()
        .take(overlay.residual_arcs.len())
        .filter_map(|(_, _, reduced)| reduced.checked_neg())
        .max()
        .unwrap_or(0)
        .max(0);
    let expected_epsilon = if classified { epsilon } else { scanned_epsilon };
    let threshold_is_valid = if classified {
        prediction_scene_i128(&overlay.threshold) == Some(threshold)
    } else if overlay.residual_arcs.len() == expected.len() {
        let projected = prediction_scene_i128(&overlay.threshold);
        projected == Some(0) || projected == Some(threshold)
    } else {
        prediction_scene_i128(&overlay.threshold) == Some(0)
    };
    if prediction_scene_i128(&overlay.epsilon) != Some(expected_epsilon) || !threshold_is_valid {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    validate_tardos_classification(graph, flows, overlay, &expected, threshold, classified)
}

fn validate_tardos_classification(
    graph: &FlowNetwork,
    flows: &[u64],
    overlay: &FlowTardosFrameworkOverlayV1,
    expected: &[(crate::residual::ResidualArcId, u64, i128)],
    threshold: i128,
    classified: bool,
) -> Result<(), FlowSceneError> {
    let expected_fixed = expected
        .iter()
        .enumerate()
        .filter(|(_, (_, _, reduced_cost))| *reduced_cost > threshold)
        .map(|(index, (arc, _, reduced_cost))| {
            Ok((
                index,
                tardos_expected_fixed_variable(graph, flows, arc, *reduced_cost)?,
            ))
        })
        .collect::<Result<Vec<_>, FlowSceneError>>()?;
    if !classified && !overlay.fixed_variables.is_empty() {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if overlay.fixed_variables.len() > expected_fixed.len()
        || (overlay.stage == FlowTardosFrameworkStageV1::Complete
            && overlay.fixed_variables.len() != expected_fixed.len())
        || overlay.fixed_variables
            != expected_fixed
                .iter()
                .take(overlay.fixed_variables.len())
                .map(|(_, fixed)| fixed.clone())
                .collect::<Vec<_>>()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let fixed_prefix = &expected_fixed[..overlay.fixed_variables.len()];
    for (index, ((arc, capacity, reduced_cost), state)) in
        expected.iter().zip(&overlay.residual_arcs).enumerate()
    {
        let direction = match arc.direction() {
            ResidualDirection::Forward => "forward",
            ResidualDirection::Reverse => "reverse",
        };
        let fixes = classified
            && fixed_prefix
                .iter()
                .any(|(fixed_index, _)| *fixed_index == index);
        if state.edge_id != arc.original_edge().as_str()
            || state.direction != direction
            || state
                .capacity
                .parse::<u64>()
                .ok()
                .filter(|value| value.to_string() == state.capacity)
                != Some(*capacity)
            || prediction_scene_i128(&state.reduced_cost) != Some(*reduced_cost)
            || state.fixes_variable != (classified && fixes)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    Ok(())
}

fn tardos_expected_residual_arcs(
    graph: &FlowNetwork,
    flows: &[u64],
    potentials: &[i128],
) -> Result<Vec<(crate::residual::ResidualArcId, u64, i128)>, FlowSceneError> {
    let residual = ResidualState::from_flows(graph, flows)
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let mut expected = Vec::with_capacity(graph.edges().len().saturating_mul(2));
    for node in graph.node_indices() {
        for arc in residual.outgoing_arcs(node) {
            let reduced_cost = arc
                .cost
                .checked_add(potentials[arc.from.as_usize()])
                .and_then(|value| value.checked_sub(potentials[arc.to.as_usize()]))
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            expected.push((arc.id, arc.capacity, reduced_cost));
        }
    }
    expected.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    Ok(expected)
}

fn tardos_expected_fixed_variable(
    graph: &FlowNetwork,
    flows: &[u64],
    arc: &crate::residual::ResidualArcId,
    reduced_cost: i128,
) -> Result<FlowTardosFixedVariableV1, FlowSceneError> {
    let edge_index = graph
        .edge_index(arc.original_edge())
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let edge = graph
        .edge(edge_index)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let flow = flows[edge_index.as_usize()];
    let (bound, direction) = match arc.direction() {
        ResidualDirection::Forward if flow == edge.lower() => ("lower", "forward"),
        ResidualDirection::Reverse if flow == edge.capacity() => ("upper", "reverse"),
        ResidualDirection::Forward | ResidualDirection::Reverse => {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    };
    Ok(FlowTardosFixedVariableV1 {
        edge_id: edge.id().as_str().to_owned(),
        bound: bound.to_owned(),
        value: flow.to_string(),
        direction: direction.to_owned(),
        reduced_cost: reduced_cost.to_string(),
    })
}

fn validate_prediction_assisted_epsilon_overlay(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    flows: &[u64],
    overlay: &FlowPredictionAssistedEpsilonOverlayV1,
) -> Result<(), FlowSceneError> {
    let header = validate_prediction_scene_header(graph, required_divergence, flows, overlay)?;
    let prices = validate_prediction_scene_nodes(graph, required_divergence, flows, overlay)?;
    let scaled_costs = validate_prediction_scene_costs(graph, overlay, header)?;
    if !matches!(
        overlay.stage,
        FlowPredictionAssistedEpsilonStageV1::PreprocessPrediction
            | FlowPredictionAssistedEpsilonStageV1::BeginAttempt
    ) {
        validate_prediction_scene_epsilon_cs(graph, flows, &prices, &scaled_costs)?;
    }
    if overlay.stage == FlowPredictionAssistedEpsilonStageV1::InitializeScale
        && prediction_scene_has_admissible_arc(graph, flows, &prices, &scaled_costs)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    validate_prediction_scene_active_arc(graph, flows, &prices, &scaled_costs, overlay)
}

#[derive(Clone, Copy)]
struct PredictionSceneHeader {
    scaling: u32,
    scale_exponent: Option<u32>,
}

fn validate_prediction_scene_header(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    flows: &[u64],
    overlay: &FlowPredictionAssistedEpsilonOverlayV1,
) -> Result<PredictionSceneHeader, FlowSceneError> {
    if required_divergence.len() != graph.nodes().len()
        || flows.len() != graph.edges().len()
        || overlay.nodes.len() != graph.nodes().len()
        || overlay.edges.len() != graph.edges().len()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let scaling = prediction_scene_u32(&overlay.scaling_parameter)
        .filter(|value| (2..=4).contains(value))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let attempt =
        prediction_scene_u32(&overlay.attempt).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let maximum_attempt = prediction_scene_u32(&overlay.maximum_attempt)
        .filter(|value| *value > 0)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let exponent =
        prediction_scene_u32(&overlay.exponent).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let scale_exponent = overlay
        .scale_exponent
        .as_deref()
        .and_then(prediction_scene_u32);
    if overlay.scale_exponent.is_some() != scale_exponent.is_some()
        || attempt > maximum_attempt
        || exponent > maximum_attempt
        || attempt != exponent
        || (attempt == 0) != (exponent == 0)
        || scale_exponent.is_some_and(|value| value >= exponent)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    validate_prediction_scene_stage(overlay, attempt, scale_exponent)?;
    Ok(PredictionSceneHeader {
        scaling,
        scale_exponent,
    })
}

fn validate_prediction_scene_nodes(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    flows: &[u64],
    overlay: &FlowPredictionAssistedEpsilonOverlayV1,
) -> Result<Vec<i128>, FlowSceneError> {
    let maximum_cost = graph
        .edges()
        .iter()
        .map(|edge| i128::from(edge.cost()).abs())
        .max()
        .unwrap_or(0);
    let prediction_upper = i128::try_from(graph.nodes().len().saturating_sub(1))
        .ok()
        .and_then(|value| value.checked_mul(maximum_cost))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let raw = overlay
        .nodes
        .iter()
        .map(|node| prediction_scene_i128(&node.raw_predicted_price))
        .collect::<Option<Vec<_>>>()
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let minimum_prediction = raw
        .iter()
        .copied()
        .min()
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let actual_divergence =
        divergences(graph, flows).map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    let mut prices = Vec::with_capacity(graph.nodes().len());
    let mut active_nodes = 0_usize;
    for (index, node) in overlay.nodes.iter().enumerate() {
        let predicted = prediction_scene_i128(&node.predicted_price)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let price =
            prediction_scene_i128(&node.price).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let surplus =
            prediction_scene_i128(&node.surplus).ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let shifted = raw[index]
            .checked_sub(minimum_prediction)
            .unwrap_or(i128::MAX);
        let expected_clipped = shifted > prediction_upper;
        let expected_prediction = shifted.min(prediction_upper);
        let expected_surplus = required_divergence[index]
            .checked_sub(actual_divergence[index])
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let expected_node = graph.nodes()[index].id().as_str();
        if node.node_id != expected_node
            || predicted != expected_prediction
            || node.prediction_clipped != expected_clipped
            || surplus != expected_surplus
            || node.active != (overlay.active_node.as_deref() == Some(expected_node))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        active_nodes += usize::from(node.active);
        prices.push(price);
    }
    if active_nodes != usize::from(overlay.active_node.is_some()) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(prices)
}

fn validate_prediction_scene_costs(
    graph: &FlowNetwork,
    overlay: &FlowPredictionAssistedEpsilonOverlayV1,
    header: PredictionSceneHeader,
) -> Result<Vec<i128>, FlowSceneError> {
    let cost_scale = i128::try_from(graph.nodes().len())
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let divisor = header
        .scale_exponent
        .map(|value| prediction_scene_pow(i128::from(header.scaling), value))
        .transpose()?
        .unwrap_or(1);
    let mut scaled_costs = Vec::with_capacity(graph.edges().len());
    for (index, state) in overlay.edges.iter().enumerate() {
        let edge = &graph.edges()[index];
        let scaled_cost = prediction_scene_i128(&state.scaled_cost)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if state.edge_id != edge.id().as_str()
            || header.scale_exponent.is_some()
                && scaled_cost
                    != prediction_scene_floor_div(
                        i128::from(edge.cost())
                            .checked_mul(cost_scale)
                            .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
                        divisor,
                    )
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        scaled_costs.push(scaled_cost);
    }
    Ok(scaled_costs)
}

fn prediction_scene_has_admissible_arc(
    graph: &FlowNetwork,
    flows: &[u64],
    prices: &[i128],
    scaled_costs: &[i128],
) -> bool {
    graph.edges().iter().enumerate().any(|(index, edge)| {
        let reduced = prices[edge.from().as_usize()]
            .checked_sub(prices[edge.to().as_usize()])
            .and_then(|value| value.checked_sub(scaled_costs[index]));
        reduced.is_some_and(|value| {
            (flows[index] < edge.capacity() && value == 1)
                || (flows[index] > edge.lower() && value == -1)
        })
    })
}

fn validate_prediction_scene_stage(
    overlay: &FlowPredictionAssistedEpsilonOverlayV1,
    attempt: u32,
    scale_exponent: Option<u32>,
) -> Result<(), FlowSceneError> {
    let active_node_expected = matches!(
        overlay.stage,
        FlowPredictionAssistedEpsilonStageV1::SelectSurplus
            | FlowPredictionAssistedEpsilonStageV1::InspectAdmissibleArc
            | FlowPredictionAssistedEpsilonStageV1::InspectPriceBreakpointArc
            | FlowPredictionAssistedEpsilonStageV1::Push
            | FlowPredictionAssistedEpsilonStageV1::RaisePrice
            | FlowPredictionAssistedEpsilonStageV1::CompleteUpIteration
    );
    let scale_expected = !matches!(
        overlay.stage,
        FlowPredictionAssistedEpsilonStageV1::PreprocessPrediction
            | FlowPredictionAssistedEpsilonStageV1::BeginAttempt
    );
    let error = overlay
        .certificate_aligned_prediction_error
        .as_deref()
        .and_then(prediction_scene_i128);
    if overlay.certificate_aligned_prediction_error.is_some() != error.is_some()
        || error.is_some_and(|value| value < 0)
        || (overlay.stage == FlowPredictionAssistedEpsilonStageV1::Optimal) != error.is_some()
        || (attempt == 0)
            != (overlay.stage == FlowPredictionAssistedEpsilonStageV1::PreprocessPrediction)
        || active_node_expected != overlay.active_node.is_some()
        || matches!(
            overlay.stage,
            FlowPredictionAssistedEpsilonStageV1::InspectAdmissibleArc
                | FlowPredictionAssistedEpsilonStageV1::InspectPriceBreakpointArc
                | FlowPredictionAssistedEpsilonStageV1::Push
        ) != overlay.active_arc.is_some()
        || scale_expected != scale_exponent.is_some()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

fn validate_prediction_scene_epsilon_cs(
    graph: &FlowNetwork,
    flows: &[u64],
    prices: &[i128],
    scaled_costs: &[i128],
) -> Result<(), FlowSceneError> {
    for (index, edge) in graph.edges().iter().enumerate() {
        let reduced = prices[edge.from().as_usize()]
            .checked_sub(prices[edge.to().as_usize()])
            .and_then(|value| value.checked_sub(scaled_costs[index]))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if (flows[index] < edge.capacity() && reduced > 1)
            || (flows[index] > edge.lower() && reduced < -1)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    Ok(())
}

fn validate_prediction_scene_active_arc(
    graph: &FlowNetwork,
    flows: &[u64],
    prices: &[i128],
    scaled_costs: &[i128],
    overlay: &FlowPredictionAssistedEpsilonOverlayV1,
) -> Result<(), FlowSceneError> {
    let Some(active) = &overlay.active_arc else {
        return Ok(());
    };
    let edge_index = graph
        .edges()
        .iter()
        .position(|edge| edge.id().as_str() == active.edge_id)
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let edge = &graph.edges()[edge_index];
    let original_reduced = prices[edge.from().as_usize()]
        .checked_sub(prices[edge.to().as_usize()])
        .and_then(|value| value.checked_sub(scaled_costs[edge_index]))
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    let (residual_reduced, residual_exists) = match active.direction.as_str() {
        "forward" => (original_reduced, flows[edge_index] < edge.capacity()),
        "reverse" => (
            original_reduced
                .checked_neg()
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?,
            flows[edge_index] > edge.lower(),
        ),
        _ => return Err(FlowSceneError::SnapshotGraphMismatch),
    };
    let inspection = matches!(
        overlay.stage,
        FlowPredictionAssistedEpsilonStageV1::InspectAdmissibleArc
            | FlowPredictionAssistedEpsilonStageV1::InspectPriceBreakpointArc
    );
    if (inspection && !residual_exists)
        || (overlay.stage == FlowPredictionAssistedEpsilonStageV1::Push && residual_reduced != 1)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

fn prediction_scene_i128(value: &str) -> Option<i128> {
    value
        .parse::<i128>()
        .ok()
        .filter(|parsed| parsed.to_string() == value)
}

fn prediction_scene_u32(value: &str) -> Option<u32> {
    value
        .parse::<u32>()
        .ok()
        .filter(|parsed| parsed.to_string() == value)
}

fn prediction_scene_pow(base: i128, exponent: u32) -> Result<i128, FlowSceneError> {
    (0..exponent).try_fold(1_i128, |value, _| {
        value
            .checked_mul(base)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)
    })
}

fn prediction_scene_floor_div(numerator: i128, denominator: i128) -> i128 {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    if remainder < 0 {
        quotient - 1
    } else {
        quotient
    }
}

fn validate_convex_network_simplex_overlay(
    graph: &FlowNetwork,
    flows: &[u64],
    convex: &FlowConvexCostOverlayV1,
    overlay: &FlowConvexNetworkSimplexOverlayV1,
) -> Result<(), FlowSceneError> {
    let original_nodes = graph.nodes().len();
    if flows.len() != graph.edges().len()
        || convex.edges.len() != graph.edges().len()
        || overlay.nodes.len() != original_nodes + 1
        || overlay.edges.len() != graph.edges().len()
        || overlay.artificial_edges.len() != original_nodes
        || overlay
            .artificial_cost
            .parse::<i128>()
            .ok()
            .is_none_or(|value| value <= 0)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let potentials = validate_convex_simplex_nodes(graph, overlay)?;
    let mut tree = vec![Vec::new(); original_nodes + 1];
    validate_convex_simplex_original_edges(graph, flows, convex, overlay, &potentials, &mut tree)?;
    validate_convex_simplex_artificial_edges(graph, overlay, &potentials, &mut tree)?;
    validate_convex_simplex_tree(graph, overlay, &tree)?;
    validate_convex_simplex_selection(graph, convex, overlay)
}

fn validate_convex_simplex_nodes(
    graph: &FlowNetwork,
    overlay: &FlowConvexNetworkSimplexOverlayV1,
) -> Result<Vec<i128>, FlowSceneError> {
    let original_nodes = graph.nodes().len();
    let mut potentials = Vec::with_capacity(original_nodes + 1);
    for (index, state) in overlay.nodes.iter().enumerate() {
        let expected = if index == original_nodes {
            "artificial-root"
        } else {
            graph.nodes()[index].id().as_str()
        };
        let potential = state
            .potential
            .parse::<i128>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        if state.entity_id != expected
            || (index == original_nodes) != state.parent.is_none()
            || state
                .parent
                .as_deref()
                .is_some_and(|parent| convex_simplex_node_index(graph, parent).is_none())
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        potentials.push(potential);
    }
    Ok(potentials)
}

fn validate_convex_simplex_original_edges(
    graph: &FlowNetwork,
    flows: &[u64],
    convex: &FlowConvexCostOverlayV1,
    overlay: &FlowConvexNetworkSimplexOverlayV1,
    potentials: &[i128],
    tree: &mut [Vec<usize>],
) -> Result<(), FlowSceneError> {
    for (((state, edge), flow), convex_state) in overlay
        .edges
        .iter()
        .zip(graph.edges())
        .zip(flows)
        .zip(&convex.edges)
    {
        if state.edge_id != edge.id().as_str()
            || convex_state.edge_id != state.edge_id
            || convex_state.flow.parse::<u64>().ok() != Some(*flow)
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let active = state
            .active_segment
            .as_ref()
            .map(|value| value.parse::<usize>())
            .transpose()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        let selected = active.and_then(|segment| {
            convex_state
                .segments
                .iter()
                .find(|entry| entry.segment.parse::<usize>().ok() == Some(segment))
        });
        let variable = edge.capacity() > edge.lower();
        if variable != selected.is_some()
            || state.basis == FlowConvexNetworkSimplexBasisV1::Tree && edge.from() == edge.to()
            || !validate_convex_simplex_piece_position(
                edge,
                *flow,
                state.basis,
                selected,
                overlay.stage == FlowConvexNetworkSimplexStageV1::CrossBreakpoint && state.in_cycle,
            )
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if state.basis == FlowConvexNetworkSimplexBasisV1::Tree {
            let piece = selected.ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            if overlay.stage != FlowConvexNetworkSimplexStageV1::CrossBreakpoint {
                let cost = piece
                    .marginal_cost
                    .parse::<i128>()
                    .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
                let reduced = cost
                    .checked_add(potentials[edge.from().as_usize()])
                    .and_then(|value| value.checked_sub(potentials[edge.to().as_usize()]))
                    .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
                if reduced != 0 {
                    return Err(FlowSceneError::SnapshotGraphMismatch);
                }
            }
            tree[edge.from().as_usize()].push(edge.to().as_usize());
            tree[edge.to().as_usize()].push(edge.from().as_usize());
        }
    }
    Ok(())
}

fn validate_convex_simplex_piece_position(
    edge: &crate::model::FlowEdge,
    flow: u64,
    basis: FlowConvexNetworkSimplexBasisV1,
    selected: Option<&FlowConvexCostSegmentStateV1>,
    combined_pivot_intermediate: bool,
) -> bool {
    let selected_contains = selected.is_some_and(|piece| {
        piece
            .start_flow
            .parse::<u64>()
            .ok()
            .zip(piece.end_flow.parse::<u64>().ok())
            .is_some_and(|(start, end)| start.max(edge.lower()) <= flow && flow <= end)
    });
    if edge.capacity() == edge.lower() {
        return selected.is_none() && flow == edge.lower();
    }
    if !selected_contains {
        return false;
    }
    if combined_pivot_intermediate {
        return true;
    }
    basis == FlowConvexNetworkSimplexBasisV1::Tree
        || flow == edge.lower()
        || flow == edge.capacity()
        || selected.is_some_and(|piece| {
            piece.end_flow.parse::<u64>().ok() == Some(flow)
                || piece
                    .start_flow
                    .parse::<u64>()
                    .ok()
                    .map(|start| start.max(edge.lower()))
                    == Some(flow)
        })
}

fn validate_convex_simplex_artificial_edges(
    graph: &FlowNetwork,
    overlay: &FlowConvexNetworkSimplexOverlayV1,
    potentials: &[i128],
    tree: &mut [Vec<usize>],
) -> Result<(), FlowSceneError> {
    let root = graph.nodes().len();
    let cost = overlay
        .artificial_cost
        .parse::<i128>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    for (node, (state, original)) in overlay
        .artificial_edges
        .iter()
        .zip(graph.nodes())
        .enumerate()
    {
        let entity = format!("artificial:{}", original.id().as_str());
        let source = convex_simplex_node_index(graph, &state.source);
        let target = convex_simplex_node_index(graph, &state.target);
        if state.entity_id != entity
            || state.node_id != original.id().as_str()
            || state
                .flow
                .parse::<i128>()
                .ok()
                .is_none_or(|value| value < 0)
            || !matches!((source, target), (Some(left), Some(right)) if (left == node && right == root) || (left == root && right == node))
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        if state.basis == FlowConvexNetworkSimplexBasisV1::Tree {
            let source = source.ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let target = target.ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let reduced = cost
                .checked_add(potentials[source])
                .and_then(|value| value.checked_sub(potentials[target]))
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            if reduced != 0 {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            tree[source].push(target);
            tree[target].push(source);
        }
    }
    Ok(())
}

fn validate_convex_simplex_tree(
    graph: &FlowNetwork,
    overlay: &FlowConvexNetworkSimplexOverlayV1,
    tree: &[Vec<usize>],
) -> Result<(), FlowSceneError> {
    if tree.iter().map(Vec::len).sum::<usize>() != graph.nodes().len().saturating_mul(2) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let root = graph.nodes().len();
    let mut reached = vec![false; root + 1];
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if reached[node] {
            continue;
        }
        reached[node] = true;
        pending.extend(tree[node].iter().copied());
    }
    if reached.iter().any(|&value| !value) {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    for (node, state) in overlay.nodes.iter().enumerate().take(root) {
        let parent = state
            .parent
            .as_deref()
            .and_then(|value| convex_simplex_node_index(graph, value))
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if !tree[node].contains(&parent) {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
        let mut cursor = node;
        for _ in 0..=root {
            if cursor == root {
                break;
            }
            cursor = overlay.nodes[cursor]
                .parent
                .as_deref()
                .and_then(|value| convex_simplex_node_index(graph, value))
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        }
        if cursor != root {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    Ok(())
}

fn validate_convex_simplex_selection(
    graph: &FlowNetwork,
    convex: &FlowConvexCostOverlayV1,
    overlay: &FlowConvexNetworkSimplexOverlayV1,
) -> Result<(), FlowSceneError> {
    for reference in overlay
        .cycle
        .iter()
        .chain(overlay.entering.iter())
        .chain(overlay.leaving.iter())
    {
        validate_convex_simplex_reference_segment(graph, convex, reference)?;
        convex_simplex_arc_endpoints(graph, overlay, reference)
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
    }
    for index in 0..overlay.cycle.len() {
        let (_, target) = convex_simplex_arc_endpoints(graph, overlay, &overlay.cycle[index])
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        let (next_source, _) = convex_simplex_arc_endpoints(
            graph,
            overlay,
            &overlay.cycle[(index + 1) % overlay.cycle.len()],
        )
        .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if target != next_source {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    validate_convex_simplex_stage_selection(overlay)?;
    let cycle = overlay
        .cycle
        .iter()
        .map(|arc| arc.entity_id.as_str())
        .collect::<BTreeSet<_>>();
    for state in &overlay.edges {
        validate_convex_simplex_flags(
            state.edge_id.as_str(),
            state.in_cycle,
            state.entering,
            state.leaving,
            &cycle,
            overlay,
        )?;
    }
    for state in &overlay.artificial_edges {
        validate_convex_simplex_flags(
            state.entity_id.as_str(),
            state.in_cycle,
            state.entering,
            state.leaving,
            &cycle,
            overlay,
        )?;
    }
    Ok(())
}

fn validate_convex_simplex_reference_segment(
    graph: &FlowNetwork,
    convex: &FlowConvexCostOverlayV1,
    reference: &FlowConvexNetworkSimplexArcRefV1,
) -> Result<(), FlowSceneError> {
    if let Some((edge_index, _)) = graph
        .edges()
        .iter()
        .enumerate()
        .find(|(_, edge)| edge.id().as_str() == reference.entity_id)
    {
        let segment = reference
            .segment
            .as_ref()
            .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
        if convex.edges[edge_index]
            .segments
            .iter()
            .any(|state| state.segment == *segment)
        {
            return Ok(());
        }
    } else if reference.entity_id.starts_with("artificial:") && reference.segment.is_none() {
        return Ok(());
    }
    Err(FlowSceneError::SnapshotGraphMismatch)
}

fn validate_convex_simplex_stage_selection(
    overlay: &FlowConvexNetworkSimplexOverlayV1,
) -> Result<(), FlowSceneError> {
    let has_cycle = !overlay.cycle.is_empty();
    let valid = match overlay.stage {
        FlowConvexNetworkSimplexStageV1::InitializeBasis
        | FlowConvexNetworkSimplexStageV1::Optimal => {
            !has_cycle && overlay.entering.is_none() && overlay.leaving.is_none()
        }
        FlowConvexNetworkSimplexStageV1::Price => !has_cycle && overlay.leaving.is_none(),
        FlowConvexNetworkSimplexStageV1::FormCycle => {
            has_cycle && overlay.entering.is_some() && overlay.leaving.is_none()
        }
        FlowConvexNetworkSimplexStageV1::CrossBreakpoint
        | FlowConvexNetworkSimplexStageV1::ExchangeBasis
        | FlowConvexNetworkSimplexStageV1::FlipBound => {
            has_cycle && overlay.entering.is_some() && overlay.leaving.is_some()
        }
    };
    if valid {
        Ok(())
    } else {
        Err(FlowSceneError::SnapshotGraphMismatch)
    }
}

fn validate_convex_simplex_flags(
    entity: &str,
    in_cycle: bool,
    entering: bool,
    leaving: bool,
    cycle: &BTreeSet<&str>,
    overlay: &FlowConvexNetworkSimplexOverlayV1,
) -> Result<(), FlowSceneError> {
    if in_cycle != cycle.contains(entity)
        || entering
            != overlay
                .entering
                .as_ref()
                .is_some_and(|arc| arc.entity_id == entity)
        || leaving
            != overlay
                .leaving
                .as_ref()
                .is_some_and(|arc| arc.entity_id == entity)
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

fn convex_simplex_arc_endpoints(
    graph: &FlowNetwork,
    overlay: &FlowConvexNetworkSimplexOverlayV1,
    reference: &FlowConvexNetworkSimplexArcRefV1,
) -> Option<(usize, usize)> {
    let forward = match reference.direction.as_str() {
        "forward" => true,
        "reverse" => false,
        _ => return None,
    };
    let endpoints = if let Some((_, edge)) = graph
        .edges()
        .iter()
        .enumerate()
        .find(|(_, edge)| edge.id().as_str() == reference.entity_id)
    {
        reference.segment.as_ref()?.parse::<usize>().ok()?;
        (edge.from().as_usize(), edge.to().as_usize())
    } else {
        let edge = overlay
            .artificial_edges
            .iter()
            .find(|edge| edge.entity_id == reference.entity_id)?;
        if reference.segment.is_some() {
            return None;
        }
        (
            convex_simplex_node_index(graph, &edge.source)?,
            convex_simplex_node_index(graph, &edge.target)?,
        )
    };
    Some(if forward {
        endpoints
    } else {
        (endpoints.1, endpoints.0)
    })
}

fn convex_simplex_node_index(graph: &FlowNetwork, entity: &str) -> Option<usize> {
    if entity == "artificial-root" {
        return Some(graph.nodes().len());
    }
    graph
        .nodes()
        .iter()
        .position(|node| node.id().as_str() == entity)
}

fn validate_double_scaling_overlay(
    graph: &FlowNetwork,
    overlay: &FlowDoubleScalingOverlayV1,
) -> Result<(), FlowSceneError> {
    let variable_edges = graph
        .edges()
        .iter()
        .filter(|edge| edge.capacity() > edge.lower())
        .collect::<Vec<_>>();
    if overlay.nodes.len() != graph.nodes().len() + variable_edges.len()
        || overlay.edges.len() != variable_edges.len()
        || overlay
            .nodes
            .iter()
            .take(graph.nodes().len())
            .zip(graph.nodes())
            .any(|(state, node)| {
                state.kind != FlowDoubleScalingNodeKindV1::Original
                    || state.entity_id != node.id().as_str()
            })
        || overlay
            .nodes
            .iter()
            .skip(graph.nodes().len())
            .zip(&variable_edges)
            .any(|(state, edge)| {
                state.kind != FlowDoubleScalingNodeKindV1::Edge
                    || state.entity_id != edge.id().as_str()
            })
        || overlay
            .edges
            .iter()
            .zip(&variable_edges)
            .any(|(state, edge)| state.edge_id != edge.id().as_str())
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let known_variable_edge = |edge_id: &str| {
        variable_edges
            .iter()
            .any(|edge| edge.id().as_str() == edge_id)
    };
    if overlay
        .admissible_arcs
        .iter()
        .chain(&overlay.active_path)
        .chain(&overlay.inspected_arc)
        .any(|arc| {
            !known_variable_edge(&arc.edge_id)
                || !matches!(arc.branch.as_str(), "flow" | "slack")
                || !matches!(arc.direction.as_str(), "forward" | "reverse")
        })
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    if (overlay.stage == FlowDoubleScalingStageV1::InspectArc) != overlay.inspected_arc.is_some() {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    let known_node_ref = |value: &str| {
        value
            .strip_prefix("node:")
            .is_some_and(|id| graph.nodes().iter().any(|node| node.id().as_str() == id))
            || value
                .strip_prefix("edge:")
                .is_some_and(&known_variable_edge)
    };
    if overlay
        .selected_root
        .iter()
        .chain(&overlay.selected_deficit)
        .any(|value| !known_node_ref(value))
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

fn validate_convex_cost_overlay(
    graph: &FlowNetwork,
    flows: &[u64],
    overlay: &FlowConvexCostOverlayV1,
) -> Result<(), FlowSceneError> {
    validate_convex_cost_segments(graph, flows, overlay)?;
    validate_convex_cost_arc_refs(overlay)?;
    let scale = convex_cost_scale(overlay)?;
    validate_convex_cost_eligible_arcs(graph, overlay, scale)?;
    validate_convex_cost_active_walk(graph, overlay, scale)
}

fn validate_convex_cost_segments(
    graph: &FlowNetwork,
    flows: &[u64],
    overlay: &FlowConvexCostOverlayV1,
) -> Result<(), FlowSceneError> {
    if flows.len() != graph.edges().len()
        || overlay.edges.len() != graph.edges().len()
        || overlay
            .edges
            .iter()
            .zip(graph.edges())
            .zip(flows)
            .any(|((state, edge), flow)| {
                state.edge_id != edge.id().as_str() || state.flow != flow.to_string()
            })
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    for (state, edge) in overlay.edges.iter().zip(graph.edges()) {
        let mut end = 0_u64;
        let mut occupied = 0_u64;
        let mut saw_partial = false;
        for (ordinal, segment) in state.segments.iter().enumerate() {
            let Ok(segment_ordinal) = segment.segment.parse::<usize>() else {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            };
            let (Ok(start), Ok(segment_end), Ok(flow)) = (
                segment.start_flow.parse::<u64>(),
                segment.end_flow.parse::<u64>(),
                segment.flow.parse::<u64>(),
            ) else {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            };
            if segment_ordinal != ordinal
                || start != end
                || segment_end <= start
                || flow > segment_end - start
                || saw_partial && flow != 0
            {
                return Err(FlowSceneError::SnapshotGraphMismatch);
            }
            if flow < segment_end - start {
                saw_partial = true;
            }
            occupied = occupied
                .checked_add(flow)
                .ok_or(FlowSceneError::CountOverflow)?;
            end = segment_end;
        }
        if end != edge.capacity()
            || occupied.to_string() != state.flow
            || (edge.capacity() > 0 && state.segments.is_empty())
            || (edge.capacity() == 0 && !state.segments.is_empty())
        {
            return Err(FlowSceneError::SnapshotGraphMismatch);
        }
    }
    Ok(())
}

fn validate_convex_cost_arc_refs(overlay: &FlowConvexCostOverlayV1) -> Result<(), FlowSceneError> {
    if overlay
        .active_cycle
        .iter()
        .chain(&overlay.eligible_arcs)
        .any(|arc| {
            !matches!(arc.direction.as_str(), "forward" | "reverse")
                || overlay
                    .edges
                    .iter()
                    .find(|edge| edge.edge_id == arc.edge_id)
                    .is_none_or(|edge| {
                        arc.segment
                            .parse::<usize>()
                            .ok()
                            .is_none_or(|segment| segment >= edge.segments.len())
                    })
        })
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

fn convex_cost_scale(overlay: &FlowConvexCostOverlayV1) -> Result<Option<u64>, FlowSceneError> {
    let scale = overlay
        .scale
        .as_deref()
        .map(str::parse::<u64>)
        .transpose()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
    if scale == Some(0)
        || scale.is_some()
            && matches!(
                overlay.stage,
                FlowConvexCostStageV1::SelectMinimumMeanCycle | FlowConvexCostStageV1::CancelCycle
            )
        || scale.is_none()
            && matches!(
                overlay.stage,
                FlowConvexCostStageV1::StartScale
                    | FlowConvexCostStageV1::SaturateMarginal
                    | FlowConvexCostStageV1::InspectMarginalArc
                    | FlowConvexCostStageV1::ShortestPath
                    | FlowConvexCostStageV1::UpdatePotentials
                    | FlowConvexCostStageV1::Augment
                    | FlowConvexCostStageV1::CompleteScale
            )
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(scale)
}

fn validate_convex_cost_eligible_arcs(
    graph: &FlowNetwork,
    overlay: &FlowConvexCostOverlayV1,
    scale: Option<u64>,
) -> Result<(), FlowSceneError> {
    let Some(scale) = scale else {
        if overlay.eligible_arcs.is_empty() {
            return Ok(());
        }
        return Err(FlowSceneError::SnapshotGraphMismatch);
    };
    let mut expected = Vec::new();
    for (state, edge) in overlay.edges.iter().zip(graph.edges()) {
        let aggregate = state
            .flow
            .parse::<u64>()
            .map_err(|_| FlowSceneError::SnapshotGraphMismatch)?;
        if let Some(segment) = state.segments.iter().find(|segment| {
            let start = segment.start_flow.parse::<u64>().ok();
            let end = segment.end_flow.parse::<u64>().ok();
            let flow = segment.flow.parse::<u64>().ok();
            start
                .zip(end)
                .zip(flow)
                .is_some_and(|((start, end), flow)| flow < end - start)
        }) {
            let start = parse_convex_segment_value(&segment.start_flow)?;
            let end = parse_convex_segment_value(&segment.end_flow)?;
            let flow = parse_convex_segment_value(&segment.flow)?;
            if end - start - flow >= scale {
                expected.push((edge.id().as_str(), segment.segment.as_str(), "forward"));
            }
        }
        if aggregate > edge.lower() {
            let segment = state
                .segments
                .iter()
                .rev()
                .find(|segment| segment.flow.parse::<u64>().is_ok_and(|flow| flow > 0))
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            let start = parse_convex_segment_value(&segment.start_flow)?;
            if aggregate - start.max(edge.lower()) >= scale {
                expected.push((edge.id().as_str(), segment.segment.as_str(), "reverse"));
            }
        }
    }
    let actual = overlay
        .eligible_arcs
        .iter()
        .map(|arc| {
            (
                arc.edge_id.as_str(),
                arc.segment.as_str(),
                arc.direction.as_str(),
            )
        })
        .collect::<Vec<_>>();
    if actual != expected {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

fn parse_convex_segment_value(value: &str) -> Result<u64, FlowSceneError> {
    value
        .parse::<u64>()
        .map_err(|_| FlowSceneError::SnapshotGraphMismatch)
}

fn validate_convex_cost_active_walk(
    graph: &FlowNetwork,
    overlay: &FlowConvexCostOverlayV1,
    scale: Option<u64>,
) -> Result<(), FlowSceneError> {
    let endpoints = overlay
        .active_cycle
        .iter()
        .map(|arc| {
            let edge = graph
                .edges()
                .iter()
                .find(|edge| edge.id().as_str() == arc.edge_id)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?;
            match arc.direction.as_str() {
                "forward" => Ok((edge.from(), edge.to())),
                "reverse" => Ok((edge.to(), edge.from())),
                _ => Err(FlowSceneError::SnapshotGraphMismatch),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if endpoints.windows(2).any(|pair| pair[0].1 != pair[1].0)
        || scale.is_none()
            && !endpoints.is_empty()
            && endpoints.last().map(|edge| edge.1) != endpoints.first().map(|edge| edge.0)
            && !(overlay.stage == FlowConvexCostStageV1::SelectMinimumMeanCycle
                && endpoints.len() == 1)
        || matches!(
            overlay.stage,
            FlowConvexCostStageV1::SaturateMarginal | FlowConvexCostStageV1::InspectMarginalArc
        ) && endpoints.len() != 1
        || matches!(
            overlay.stage,
            FlowConvexCostStageV1::UpdatePotentials | FlowConvexCostStageV1::Augment
        ) && endpoints.is_empty()
        || matches!(
            overlay.stage,
            FlowConvexCostStageV1::Initialize
                | FlowConvexCostStageV1::StartScale
                | FlowConvexCostStageV1::CompleteScale
                | FlowConvexCostStageV1::Optimal
        ) && !endpoints.is_empty()
    {
        return Err(FlowSceneError::SnapshotGraphMismatch);
    }
    Ok(())
}

const fn trace_membership(membership: EibfsTraceMembership) -> &'static str {
    match membership {
        EibfsTraceMembership::Free => "free",
        EibfsTraceMembership::Source => "source",
        EibfsTraceMembership::Sink => "sink",
    }
}

fn node_trace_states(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
) -> Result<Vec<FlowNodeTraceStateV1>, FlowSceneError> {
    // A fully balanced vector carries no visible information. Suppressing its
    // zeroes keeps the public Ready state identical to a checked zero-balance
    // kernel base, while nonzero source-created imbalance remains explicit.
    let show_remaining_divergence = snapshot
        .remaining_divergence
        .iter()
        .any(|&divergence| divergence != 0);
    let search_ordinals = snapshot.search_order.iter().enumerate().try_fold(
        BTreeMap::new(),
        |mut ordinals, (ordinal, node)| {
            let ordinal = u32::try_from(ordinal).map_err(|_| FlowSceneError::CountOverflow)?;
            ordinals.entry(node).or_insert(ordinal);
            Ok::<_, FlowSceneError>(ordinals)
        },
    )?;
    graph
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let label = snapshot
                .node_labels
                .get(index)
                .ok_or(FlowSceneError::SnapshotGraphMismatch)?
                .map(|value| value.to_string());
            let remaining_divergence = if show_remaining_divergence {
                Some(
                    snapshot
                        .remaining_divergence
                        .get(index)
                        .ok_or(FlowSceneError::SnapshotGraphMismatch)?
                        .to_string(),
                )
            } else {
                None
            };
            Ok(FlowNodeTraceStateV1 {
                node_id: node.id().as_str().to_owned(),
                label,
                search_ordinal: search_ordinals.get(node.id()).copied(),
                remaining_divergence,
            })
        })
        .collect()
}

fn trace_event_scene(event: &FlowTraceEvent) -> Result<FlowTraceEventSceneV1, FlowSceneError> {
    Ok(FlowTraceEventSceneV1 {
        event_id: event.event_id.to_string(),
        parent_phase_id: event.parent_phase_id.map(|value| value.to_string()),
        catalog_id: event.catalog_id.clone(),
        minimum_granularity: event.minimum_granularity,
        pseudocode_line: event.pseudocode_line.clone(),
        patch_count: u32::try_from(event.patches.len())
            .map_err(|_| FlowSceneError::CountOverflow)?,
        detail: event
            .detail
            .as_ref()
            .map(|detail| FlowTraceEventDetailSceneV1 {
                label: detail.label.clone(),
                value: detail.value.to_string(),
            }),
        entity_refs: event
            .entity_refs
            .iter()
            .map(|entity| match entity {
                FlowTraceEntityRef::Node(node) => FlowTraceEntityRefSceneV1::Node {
                    node_id: node.as_str().to_owned(),
                },
                FlowTraceEntityRef::Edge(edge) => FlowTraceEntityRefSceneV1::Edge {
                    edge_id: edge.as_str().to_owned(),
                },
                FlowTraceEntityRef::ResidualArc(arc) => FlowTraceEntityRefSceneV1::ResidualArc {
                    edge_id: arc.original_edge().as_str().to_owned(),
                    direction: match arc.direction() {
                        ResidualDirection::Forward => "forward",
                        ResidualDirection::Reverse => "reverse",
                    }
                    .to_owned(),
                },
            })
            .collect(),
    })
}

fn trace_metrics(metrics: FlowTraceMetrics) -> [String; FLOW_METRIC_COUNT] {
    let mut result = std::array::from_fn(|_| "0".to_owned());
    result[0] = metrics.bfs_runs.to_string();
    result[1] = metrics.relaxation_passes.to_string();
    result[2] = metrics.residual_arc_scans.to_string();
    result[3] = metrics.augmentations.to_string();
    result[4] = metrics.path_searches.to_string();
    result[5] = metrics.scaling_phases.to_string();
    result[6] = metrics.blocking_flow_phases.to_string();
    result[7] = metrics.relabels.to_string();
    result[8] = metrics.retreats.to_string();
    result[9] = metrics.reverse_bfs_runs.to_string();
    result[10] = metrics.gap_terminations.to_string();
    result[11] = metrics.pushes.to_string();
    result[12] = metrics.saturating_pushes.to_string();
    result[13] = metrics.nonsaturating_pushes.to_string();
    result[14] = metrics.discharges.to_string();
    result[15] = metrics.active_vertex_selections.to_string();
    result
}

#[cfg(test)]
#[path = "scene_tests.rs"]
mod tests;
