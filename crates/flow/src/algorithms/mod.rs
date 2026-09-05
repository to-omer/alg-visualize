//! Exact solver kernels published only after independent certificate checks.

mod alpha_power_ipm;
mod auction;
mod augmenting_electrical_flow;
mod blocking_preflow;
mod blocking_primal_dual;
mod borradaile_klein;
mod bounded_dynamic_spanner;
mod bounded_link_cut_flow;
mod boykov_kolmogorov;
mod cancel_and_tighten;
mod capacity_scaling;
mod convex_cost;
mod convex_cost_scaling;
mod convex_network_simplex;
mod cost_scaling;
mod cycle_canceling;
pub(crate) mod data_structures;
mod deterministic_almost_linear_max_flow;
mod deterministic_spanner_sparsify;
mod dinic;
mod distance_directed;
mod double_scaling;
mod dual_network_simplex;
mod dynamic_active_branch_projection;
mod dynamic_core_graph;
mod dynamic_eibfs;
mod dynamic_flow_tracker;
mod dynamic_hidden_stability;
mod dynamic_level_projection;
mod dynamic_level_stage_adapter;
mod dynamic_low_stretch_forest;
mod dynamic_min_ratio_cycle;
mod dynamic_min_ratio_hidden_stability;
mod dynamic_min_ratio_shift_game;
mod dynamic_mwu_collection_bridge;
mod dynamic_shifted_tree_chain;
mod dynamic_sparse_core;
mod dynamic_sparse_core_collection;
mod dynamic_tree_blocking;
mod dynamic_tree_chain_candidate_heap;
mod dynamic_tree_chain_epoch_runtime;
mod dynamic_tree_chain_epochs;
mod dynamic_tree_chain_propagation;
mod dynamic_tree_chain_query;
mod dynamic_tree_push_relabel;
mod eibfs;
mod electrical_flow;
mod electrical_flow_interior_point_mcf;
mod enhanced_capacity_scaling;
mod epsilon_relaxation;
mod flow_framework_mcf;
mod flow_rounding;
mod ford_fulkerson;
mod goldberg_rao;
mod hassin;
mod hidden_stable_witness;
mod hld_branch_free_tree;
mod hopcroft_karp;
mod hungarian;
mod ibfs;
mod interior_point_max_flow;
mod low_stretch_forest_mwu;
mod max_flow;
mod min_cost;
mod minimum_mean_cycle_canceling;
mod minimum_ratio_cycle;
mod minimum_ratio_cycle_mcf;
mod network_simplex;
mod orlin_max_flow;
mod orlin_mcf;
mod out_of_kilter;
mod parametric_breakpoint_rerun;
mod parametric_pseudoflow;
mod polynomial_dual_network_simplex;
mod polynomial_primal_network_simplex;
mod prediction_assisted_epsilon_relaxation;
mod primal_dual;
mod primal_dual_interior_point_mcf;
mod pseudoflow;
mod push_relabel;
mod randomized_almost_linear_max_flow;
mod randomized_almost_linear_mcf;
mod relaxation;
mod relaxed_mndc;
mod sap;
mod shift_rebuild_game;
mod shifted_tree_chain;
mod shifted_tree_chain_query;
mod shifted_tree_chain_update;
mod successive_shortest_augmenting_path;
mod synchronous_push_relabel;
mod tardos_framework;
mod transportation_simplex;
mod warm_start_push_relabel;
mod weighted_augmenting_paths;
mod weighted_push_relabel_shortcut;

/// Produces a human-readable decimal that is stable across native and WASM
/// floating-point evaluation. Algorithm state keeps the exact IEEE-754 bits;
/// only the scene projection is rounded to fourteen significant decimal digits.
/// That is tighter than every floating invariant exposed by these bounded
/// visualizers while discarding architecture-dependent last-bit drift.
pub(crate) fn stable_scene_decimal(value: f64) -> String {
    debug_assert!(value.is_finite());
    if value == 0.0 {
        return "0".to_owned();
    }
    format!("{value:.13e}")
        .parse::<f64>()
        .expect("a finite formatted f64 must parse")
        .to_string()
}

pub use auction::{
    AUCTION_MAX_BIDS, AUCTION_MAX_EDGE_SCANS, AUCTION_MAX_EDGES, AUCTION_MAX_NODES,
    AUCTION_MAX_STATE_TRANSITIONS, AUCTION_MAX_TRACE_PROJECTION_CELLS, AuctionError,
    AuctionMetrics, AuctionOutcome, AuctionResult, AuctionTraceResult, solve_auction,
    trace_auction,
};
pub use augmenting_electrical_flow::{
    AUGMENTING_ELECTRICAL_MAX_BOOSTS, AUGMENTING_ELECTRICAL_MAX_CAPACITY,
    AUGMENTING_ELECTRICAL_MAX_DISCRETE_TRANSITIONS, AUGMENTING_ELECTRICAL_MAX_EDGES,
    AUGMENTING_ELECTRICAL_MAX_NODES, AUGMENTING_ELECTRICAL_MAX_PROGRESS_STEPS,
    AUGMENTING_ELECTRICAL_MAX_TRACE_EVENTS, AUGMENTING_ELECTRICAL_MAX_WORKING_EDGES,
    AUGMENTING_ELECTRICAL_MAX_WORKING_NODES, AugmentingElectricalEdgeState,
    AugmentingElectricalError, AugmentingElectricalExtractionArc,
    AugmentingElectricalExtractionArcKind, AugmentingElectricalMetrics,
    AugmentingElectricalNodeState, AugmentingElectricalResult, AugmentingElectricalScalar,
    AugmentingElectricalSnapshot, AugmentingElectricalStage, AugmentingElectricalTraceEvent,
    AugmentingElectricalTraceResult, AugmentingElectricalWorkingArc,
    check_augmenting_electrical_trace, solve_augmenting_electrical_flow,
    trace_augmenting_electrical_flow,
};
pub use blocking_preflow::{
    BLOCKING_PREFLOW_MAX_EDGES, BLOCKING_PREFLOW_MAX_NODES,
    BLOCKING_PREFLOW_MAX_RESIDUAL_ARC_SCANS, BLOCKING_PREFLOW_MAX_STATE_TRANSITIONS,
    BlockingPreflowError, BlockingPreflowExecutionPreset, BlockingPreflowMetrics,
    BlockingPreflowResult, BlockingPreflowTraceResult,
    solve_blocking_preflow_preset_with_feasibility, solve_karzanov_preflow, solve_mpm,
    trace_blocking_preflow_preset_with_feasibility, trace_karzanov_preflow, trace_mpm,
};
pub use blocking_primal_dual::{
    BLOCKING_PRIMAL_DUAL_MAX_EDGES, BLOCKING_PRIMAL_DUAL_MAX_NODES,
    BLOCKING_PRIMAL_DUAL_MAX_RESIDUAL_ARC_SCANS, BLOCKING_PRIMAL_DUAL_MAX_STATE_TRANSITIONS,
    BlockingPrimalDualError, BlockingPrimalDualMetrics, BlockingPrimalDualResult,
    BlockingPrimalDualTraceResult, solve_blocking_primal_dual, trace_blocking_primal_dual,
    trace_blocking_primal_dual_with_feasibility,
};
pub use borradaile_klein::{
    BORRADAILE_KLEIN_MAX_DART_SCANS, BORRADAILE_KLEIN_MAX_EDGES, BORRADAILE_KLEIN_MAX_NODES,
    BORRADAILE_KLEIN_MAX_TRACE_EVENTS, BorradaileKleinError, BorradaileKleinMetrics,
    BorradaileKleinResult, BorradaileKleinTraceResult, solve_borradaile_klein_planar,
    trace_borradaile_klein_planar,
};
pub use bounded_dynamic_spanner::{
    BOUNDED_DYNAMIC_SPANNER_MAX_LEVELS, BOUNDED_DYNAMIC_SPANNER_MAX_STAGES,
    BoundedDynamicSpannerEdgeState, BoundedDynamicSpannerEndpoint, BoundedDynamicSpannerError,
    BoundedDynamicSpannerLevelSnapshot, BoundedDynamicSpannerMetrics,
    BoundedDynamicSpannerProjectedEdge, BoundedDynamicSpannerProjectionCertificate,
    BoundedDynamicSpannerSnapshot, BoundedDynamicSpannerTrace, BoundedDynamicSpannerUpdate,
    check_bounded_dynamic_spanner_trace, trace_bounded_dynamic_spanner,
};
pub use bounded_link_cut_flow::{
    BoundedLinkCutFlowCertificate, BoundedLinkCutFlowError, BoundedLinkCutFlowMetrics,
    apply_bounded_link_cut_flow, check_bounded_link_cut_flow_certificate,
};
pub use boykov_kolmogorov::{
    BOYKOV_KOLMOGOROV_MAX_ARC_SCANS, BOYKOV_KOLMOGOROV_MAX_AUGMENTATIONS,
    BOYKOV_KOLMOGOROV_MAX_EDGES, BOYKOV_KOLMOGOROV_MAX_NODES, BOYKOV_KOLMOGOROV_MAX_TRANSITIONS,
    BoykovKolmogorovError, BoykovKolmogorovMetrics, BoykovKolmogorovResult,
    BoykovKolmogorovTraceCheckError, BoykovKolmogorovTraceResult, check_boykov_kolmogorov_trace,
    solve_boykov_kolmogorov, trace_boykov_kolmogorov, validate_boykov_kolmogorov_graph,
};
pub use cancel_and_tighten::{
    CANCEL_AND_TIGHTEN_MAX_CANCELLATIONS, CANCEL_AND_TIGHTEN_MAX_EDGES,
    CANCEL_AND_TIGHTEN_MAX_NODES, CANCEL_AND_TIGHTEN_MAX_PHASES,
    CANCEL_AND_TIGHTEN_MAX_RESIDUAL_ARC_SCANS, CancelTightenError, CancelTightenMetrics,
    CancelTightenRational, CancelTightenResult, CancelTightenSnapshot, CancelTightenStage,
    CancelTightenTraceEvent, CancelTightenTraceResult, check_cancel_and_tighten_trace,
    solve_cancel_and_tighten, trace_cancel_and_tighten, trace_cancel_and_tighten_with_feasibility,
};
pub use capacity_scaling::{
    CAPACITY_SCALING_MAX_AUGMENTATIONS, CAPACITY_SCALING_MAX_EDGES, CAPACITY_SCALING_MAX_NODES,
    CAPACITY_SCALING_MAX_RESIDUAL_ARC_SCANS, CapacityScalingError, CapacityScalingMetrics,
    CapacityScalingResult, CapacityScalingTraceResult, solve_capacity_scaling,
    solve_excess_scaling_mcf, trace_capacity_scaling, trace_capacity_scaling_with_feasibility,
    trace_excess_scaling_mcf, trace_excess_scaling_mcf_with_feasibility,
};
pub use convex_cost::{
    CONVEX_EXPANDED_MAX_EDGES, CONVEX_EXPANDED_MAX_NODES, CONVEX_EXPANDED_MAX_SEGMENTS,
    ConvexCostCertificate, ConvexCostError, ConvexCostProblem, ConvexCostResult, ConvexCostSegment,
    ConvexCostSnapshot, ConvexCostStage, ConvexCostTraceEvent, ConvexCostTraceResult,
    ConvexEdgeCost, ConvexResidualArc, ConvexResidualDirection, ConvexSegmentState,
    check_convex_cost_flow, check_segment_expanded_convex_trace,
    solve_segment_expanded_convex_cost, trace_segment_expanded_convex_cost,
    trace_segment_expanded_convex_cost_with_feasibility,
};
pub use convex_cost_scaling::{
    CONVEX_SCALING_MAX_AUGMENTATIONS, CONVEX_SCALING_MAX_EDGES,
    CONVEX_SCALING_MAX_MARGINAL_ARC_SCANS, CONVEX_SCALING_MAX_NODES,
    CONVEX_SCALING_MAX_PHASE_SATURATIONS, CONVEX_SCALING_MAX_SEGMENTS, ConvexCostScalingError,
    ConvexCostScalingMetrics, ConvexCostScalingResult, ConvexCostScalingSnapshot,
    ConvexCostScalingStage, ConvexCostScalingTraceEvent, ConvexCostScalingTraceResult,
    check_convex_cost_scaling_trace, solve_convex_cost_scaling, trace_convex_cost_scaling,
    trace_convex_cost_scaling_with_feasibility,
};
pub use convex_network_simplex::{
    CONVEX_SIMPLEX_MAX_ARC_SCANS, CONVEX_SIMPLEX_MAX_BREAKPOINT_CROSSINGS,
    CONVEX_SIMPLEX_MAX_EDGES, CONVEX_SIMPLEX_MAX_NODES, CONVEX_SIMPLEX_MAX_PIVOTS,
    CONVEX_SIMPLEX_MAX_SEGMENTS, ConvexNetworkSimplexArcRef, ConvexNetworkSimplexArtificialState,
    ConvexNetworkSimplexBasisState, ConvexNetworkSimplexEdgeState, ConvexNetworkSimplexError,
    ConvexNetworkSimplexMetrics, ConvexNetworkSimplexResult, ConvexNetworkSimplexSnapshot,
    ConvexNetworkSimplexStage, ConvexNetworkSimplexTraceEvent, ConvexNetworkSimplexTraceResult,
    check_convex_network_simplex_trace, solve_convex_network_simplex, trace_convex_network_simplex,
    trace_convex_network_simplex_with_feasibility,
};
pub use cost_scaling::{
    ARC_FIXING_BETA_DENOMINATOR, ARC_FIXING_BETA_NUMERATOR, COST_SCALING_MAX_EDGES,
    COST_SCALING_MAX_NODES, COST_SCALING_MAX_RESIDUAL_ARC_SCANS,
    COST_SCALING_MAX_STATE_TRANSITIONS, CostScalingError, CostScalingExecutionPreset,
    CostScalingMetrics, CostScalingResult, CostScalingTraceResult,
    PARTIAL_AUGMENT_RELABEL_MCF_PATH_LENGTH, solve_arc_fixing, solve_augment_relabel,
    solve_cost_scaling, solve_cost_scaling_push_relabel,
    solve_generalized_cost_scaling_push_relabel, solve_partial_augment_relabel_mcf,
    solve_price_refinement, trace_arc_fixing, trace_augment_relabel, trace_cost_scaling,
    trace_cost_scaling_preset_with_feasibility, trace_cost_scaling_push_relabel,
    trace_generalized_cost_scaling_push_relabel, trace_partial_augment_relabel_mcf,
    trace_price_refinement,
};
pub use cycle_canceling::{
    SIMPLE_CYCLE_CANCELING_MAX_CYCLES, SIMPLE_CYCLE_CANCELING_MAX_EDGES,
    SIMPLE_CYCLE_CANCELING_MAX_NODES, SIMPLE_CYCLE_CANCELING_MAX_RESIDUAL_ARC_SCANS,
    SimpleCycleCancelingError, SimpleCycleCancelingMetrics, SimpleCycleCancelingResult,
    SimpleCycleCancelingTraceResult, solve_simple_cycle_canceling, trace_simple_cycle_canceling,
    trace_simple_cycle_canceling_with_feasibility,
};
pub use deterministic_almost_linear_max_flow::{
    DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES, DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS,
    DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_ASSIGNMENTS,
    DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_EDGES,
    DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_FORESTS,
    DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_IPM_STEPS,
    DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_NODES,
    DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_PASSES,
    DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_TRACE_EVENTS, DeterministicAlmostLinearCycleKind,
    DeterministicAlmostLinearEdgeState, DeterministicAlmostLinearMaxFlowError,
    DeterministicAlmostLinearMaxFlowMetrics, DeterministicAlmostLinearMaxFlowResult,
    DeterministicAlmostLinearMaxFlowSnapshot, DeterministicAlmostLinearMaxFlowStage,
    DeterministicAlmostLinearMaxFlowTraceEvent, DeterministicAlmostLinearMaxFlowTraceResult,
    DeterministicAlmostLinearNodeState, DeterministicAlmostLinearScalar,
    check_deterministic_almost_linear_max_flow_trace, solve_deterministic_almost_linear_max_flow,
    trace_deterministic_almost_linear_max_flow,
};
pub use deterministic_spanner_sparsify::{
    DETERMINISTIC_SPANNER_MAX_EMBEDDING_ARCS, DETERMINISTIC_SPANNER_MAX_WITNESS_EDGES,
    DeterministicSpannerArc, DeterministicSpannerDegreeTarget,
    DeterministicSpannerSparsifyCertificate, DeterministicSpannerWitnessEdge,
};
pub use dinic::{
    DINIC_MAX_EDGES, DINIC_MAX_NODES, DinicError, DinicExecutionPreset, DinicMetrics, DinicResult,
    DinicTraceResult, solve_dinic, solve_dinic_preset_with_feasibility, solve_unit_capacity_dinic,
    solve_unit_network_dinic, trace_dinic, trace_dinic_preset_with_feasibility,
    trace_unit_capacity_dinic, trace_unit_network_dinic, validate_unit_capacity_dinic_graph,
    validate_unit_network_dinic_graph,
};
pub use distance_directed::{
    DISTANCE_DIRECTED_MAX_EDGES, DISTANCE_DIRECTED_MAX_NODES,
    DISTANCE_DIRECTED_MAX_RESIDUAL_ARC_SCANS, DISTANCE_DIRECTED_MAX_STATE_TRANSITIONS,
    DISTANCE_DIRECTED_MAX_TRACE_EVENTS, DistanceDirectedError, DistanceDirectedMetrics,
    DistanceDirectedResult, DistanceDirectedTraceResult, distance_directed_trace_metrics,
    solve_distance_directed_dd2, solve_distance_directed_scaling, trace_distance_directed_dd2,
    trace_distance_directed_scaling,
};
pub use double_scaling::{
    DOUBLE_SCALING_MAX_ARC_SCANS, DOUBLE_SCALING_MAX_EDGES, DOUBLE_SCALING_MAX_NODES,
    DOUBLE_SCALING_MAX_TRANSITIONS, DoubleScalingArcId, DoubleScalingBranch, DoubleScalingError,
    DoubleScalingMetrics, DoubleScalingNodeRef, DoubleScalingResult, DoubleScalingSnapshot,
    DoubleScalingStage, DoubleScalingTraceEvent, DoubleScalingTraceResult,
    check_double_scaling_trace, solve_double_scaling, trace_double_scaling,
    trace_double_scaling_with_feasibility,
};
pub use dual_network_simplex::{
    DUAL_NETWORK_SIMPLEX_MAX_ARC_SCANS, DUAL_NETWORK_SIMPLEX_MAX_EDGES,
    DUAL_NETWORK_SIMPLEX_MAX_NODES, DUAL_NETWORK_SIMPLEX_MAX_PIVOTS, DualNetworkSimplexError,
    DualNetworkSimplexMetrics, DualNetworkSimplexResult, DualNetworkSimplexSnapshot,
    DualNetworkSimplexStage, DualNetworkSimplexTraceEvent, DualNetworkSimplexTraceResult,
    check_dual_network_simplex_trace, solve_dual_network_simplex, trace_dual_network_simplex,
    trace_dual_network_simplex_with_feasibility,
};
pub use dynamic_active_branch_projection::{
    DynamicActiveBranchProjectionError, DynamicActiveBranchProjectionInput,
    DynamicActiveBranchProjectionMetrics, DynamicActiveBranchProjectionResult,
    DynamicActiveBranchProjectionTraceEvent, DynamicActiveBranchProjectionTraceResult,
    check_dynamic_active_branch_projection_trace, execute_dynamic_active_branch_projection,
    trace_dynamic_active_branch_projection,
};
pub use dynamic_core_graph::{
    DYNAMIC_CORE_MAX_EDGES, DYNAMIC_CORE_MAX_NODES, DYNAMIC_CORE_MAX_OPERATIONS,
    DYNAMIC_CORE_MAX_RATIONAL_BITS, DYNAMIC_CORE_MAX_TRACE_EVENTS, DynamicCoreEdge,
    DynamicCoreEncodedSide, DynamicCoreGraphError, DynamicCoreGraphEventKind,
    DynamicCoreGraphInput, DynamicCoreGraphMetrics, DynamicCoreGraphOperation,
    DynamicCoreGraphResult, DynamicCoreGraphSnapshot, DynamicCoreGraphStageBatch,
    DynamicCoreGraphStageEdge, DynamicCoreGraphStageEventKind, DynamicCoreGraphStageTraceEvent,
    DynamicCoreGraphStageTraceResult, DynamicCoreGraphStageUpdate, DynamicCoreGraphTraceEvent,
    DynamicCoreGraphTraceResult, DynamicCoreIncidence, DynamicCoreIncidenceEndpoint,
    DynamicCoreUpdate, check_dynamic_core_graph_stage_trace, check_dynamic_core_graph_trace,
    execute_dynamic_core_graph, execute_dynamic_core_graph_stages, trace_dynamic_core_graph,
    trace_dynamic_core_graph_stages,
};
pub use dynamic_eibfs::{
    DYNAMIC_EIBFS_MAX_UPDATES, DynamicCapacityUpdate, DynamicEibfsError, DynamicEibfsProblem,
    prepare_dynamic_eibfs,
};
pub use dynamic_flow_tracker::{
    DYNAMIC_FLOW_TRACKER_MAX_EDGES, DYNAMIC_FLOW_TRACKER_MAX_NODES,
    DYNAMIC_FLOW_TRACKER_MAX_OPERATIONS, DYNAMIC_FLOW_TRACKER_MAX_RATIONAL_BITS,
    DYNAMIC_FLOW_TRACKER_MAX_TRACE_EVENTS, DynamicFlowTrackerCoordinateUpdate,
    DynamicFlowTrackerEdge, DynamicFlowTrackerError, DynamicFlowTrackerEventKind,
    DynamicFlowTrackerGraph, DynamicFlowTrackerMetrics, DynamicFlowTrackerOperation,
    DynamicFlowTrackerResponse, DynamicFlowTrackerResult, DynamicFlowTrackerSnapshot,
    DynamicFlowTrackerTraceEvent, DynamicFlowTrackerTraceResult, check_dynamic_flow_tracker_trace,
    execute_dynamic_flow_tracker, trace_dynamic_flow_tracker,
};
pub use dynamic_hidden_stability::{
    DynamicHiddenStabilityAudit, DynamicHiddenStabilityCertificate, DynamicHiddenStabilityError,
    DynamicHiddenStabilityStageCertificate, check_dynamic_hidden_stability_isolation,
};
pub use dynamic_level_projection::{
    DYNAMIC_LEVEL_PROJECTION_MAX_UPDATES, DynamicLevelEdge, DynamicLevelGraphSnapshot,
    DynamicLevelProjectionError, DynamicLevelProjectionMetrics, DynamicLevelProjectionResult,
    DynamicLevelProjectionState, DynamicLevelProjectionTraceEvent,
    DynamicLevelProjectionTraceResult, DynamicLevelSplitProvenance, DynamicLevelStageBatch,
    DynamicLevelUpdate, DynamicLevelVertexBinding, DynamicLevelVertexMap,
    check_dynamic_level_projection_trace, execute_dynamic_level_projection,
    initialize_dynamic_level_projection, trace_dynamic_level_projection,
};
pub use dynamic_level_stage_adapter::{
    DynamicLevelStageAdapterError, DynamicLevelStageAdapterMetrics, DynamicLevelStageAdapterResult,
    DynamicLevelStageAdapterTraceEvent, DynamicLevelStageAdapterTraceResult,
    adapt_dynamic_level_stage_batch, check_dynamic_level_stage_adapter_trace,
    trace_dynamic_level_stage_adapter,
};
pub use dynamic_low_stretch_forest::{
    DYNAMIC_LSF_MAX_EDGES, DYNAMIC_LSF_MAX_NODES, DYNAMIC_LSF_MAX_OPERATIONS,
    DYNAMIC_LSF_MAX_RATIONAL_BITS, DYNAMIC_LSF_MAX_STAGE_UPDATES, DYNAMIC_LSF_MAX_TRACE_EVENTS,
    DynamicLowStretchForestEdge, DynamicLowStretchForestEncodedSide, DynamicLowStretchForestError,
    DynamicLowStretchForestEventKind, DynamicLowStretchForestIncidence,
    DynamicLowStretchForestIncidenceEndpoint, DynamicLowStretchForestInput,
    DynamicLowStretchForestMetrics, DynamicLowStretchForestOperation,
    DynamicLowStretchForestResult, DynamicLowStretchForestSnapshot,
    DynamicLowStretchForestStageBatch, DynamicLowStretchForestStageEventKind,
    DynamicLowStretchForestStageTraceEvent, DynamicLowStretchForestStageTraceResult,
    DynamicLowStretchForestStageUpdate, DynamicLowStretchForestTraceEvent,
    DynamicLowStretchForestTraceResult, check_dynamic_low_stretch_forest_stage_trace,
    check_dynamic_low_stretch_forest_trace, execute_dynamic_low_stretch_forest,
    execute_dynamic_low_stretch_forest_stages, trace_dynamic_low_stretch_forest,
    trace_dynamic_low_stretch_forest_stages,
};
pub use dynamic_min_ratio_cycle::{
    DYNAMIC_MIN_RATIO_CYCLE_MAX_OPERATIONS, DYNAMIC_MIN_RATIO_CYCLE_MAX_PSI,
    DYNAMIC_MIN_RATIO_CYCLE_MAX_QUERIES, DYNAMIC_MIN_RATIO_CYCLE_MAX_RATIONAL_BITS,
    DYNAMIC_MIN_RATIO_CYCLE_MAX_REBUILD_LIMIT, DYNAMIC_MIN_RATIO_CYCLE_MAX_TRACE_EVENTS,
    DYNAMIC_MIN_RATIO_CYCLE_SOURCE_STEP_DENOMINATOR, DynamicMinRatioCycleConfig,
    DynamicMinRatioCycleError, DynamicMinRatioCycleEventKind, DynamicMinRatioCycleFlowApplication,
    DynamicMinRatioCycleMetrics, DynamicMinRatioCycleOperation, DynamicMinRatioCycleResponse,
    DynamicMinRatioCycleResult, DynamicMinRatioCycleSession, DynamicMinRatioCycleSnapshot,
    DynamicMinRatioCycleTraceEvent, DynamicMinRatioCycleTraceResult,
    DynamicMinRatioCycleTraceSession, check_dynamic_min_ratio_cycle_trace,
    execute_dynamic_min_ratio_cycle, initialize_dynamic_min_ratio_cycle_runtime,
    trace_dynamic_min_ratio_cycle,
};
pub use dynamic_min_ratio_hidden_stability::{
    DynamicMinRatioHiddenStabilityAudit, DynamicMinRatioHiddenStabilityError,
    check_dynamic_min_ratio_hidden_stability_isolation,
};
pub use dynamic_min_ratio_shift_game::{
    DynamicMinRatioShiftGameAudit, DynamicMinRatioShiftGameError,
    check_dynamic_min_ratio_shift_game_isolation, trace_dynamic_min_ratio_shift_game_isolation,
};
pub use dynamic_mwu_collection_bridge::{
    DynamicMwuCollectionBridgeConfig, DynamicMwuCollectionBridgeError,
    DynamicMwuCollectionBridgeResult, DynamicMwuCollectionBridgeTraceResult,
    build_dynamic_mwu_sparse_core_collection, check_dynamic_mwu_sparse_core_collection_trace,
    trace_dynamic_mwu_sparse_core_collection,
};
pub use dynamic_shifted_tree_chain::{
    DYNAMIC_SHIFTED_TREE_CHAIN_MAX_CHAIN_OPERATIONS, DYNAMIC_SHIFTED_TREE_CHAIN_MAX_OPERATIONS,
    DYNAMIC_SHIFTED_TREE_CHAIN_MAX_PSI, DYNAMIC_SHIFTED_TREE_CHAIN_MAX_RATIONAL_BITS,
    DYNAMIC_SHIFTED_TREE_CHAIN_MAX_REBUILD_LIMIT, DYNAMIC_SHIFTED_TREE_CHAIN_MAX_TRACE_EVENTS,
    DynamicShiftedTreeChainConfig, DynamicShiftedTreeChainCoordinateUpdate,
    DynamicShiftedTreeChainError, DynamicShiftedTreeChainEventKind, DynamicShiftedTreeChainMetrics,
    DynamicShiftedTreeChainOperation, DynamicShiftedTreeChainResponse,
    DynamicShiftedTreeChainResult, DynamicShiftedTreeChainSnapshot,
    DynamicShiftedTreeChainTraceEvent, DynamicShiftedTreeChainTraceResult,
    check_dynamic_shifted_tree_chain_trace, execute_dynamic_shifted_tree_chain,
    trace_dynamic_shifted_tree_chain,
};
pub use dynamic_sparse_core::{
    DYNAMIC_SPARSE_CORE_MAX_BRANCHES, DYNAMIC_SPARSE_CORE_MAX_EDGES, DYNAMIC_SPARSE_CORE_MAX_NODES,
    DYNAMIC_SPARSE_CORE_MAX_OPERATIONS, DYNAMIC_SPARSE_CORE_MAX_RATIONAL_BITS,
    DYNAMIC_SPARSE_CORE_MAX_TRACE_EVENTS, DynamicSparseCoreEmbeddingArc, DynamicSparseCoreError,
    DynamicSparseCoreEventKind, DynamicSparseCoreInput, DynamicSparseCoreMetrics,
    DynamicSparseCoreRefreshReason, DynamicSparseCoreResult, DynamicSparseCoreSnapshot,
    DynamicSparseCoreSpannerBucket, DynamicSparseCoreStageEventKind,
    DynamicSparseCoreStageTraceEvent, DynamicSparseCoreStageTraceResult,
    DynamicSparseCoreTraceEvent, DynamicSparseCoreTraceResult, DynamicSparseCoreUpdate,
    check_dynamic_sparse_core_stage_trace, check_dynamic_sparse_core_trace,
    execute_dynamic_sparse_core, execute_dynamic_sparse_core_stages, trace_dynamic_sparse_core,
    trace_dynamic_sparse_core_stages,
};
pub use dynamic_sparse_core_collection::{
    DYNAMIC_SPARSE_CORE_COLLECTION_MAX_BRANCHES, DYNAMIC_SPARSE_CORE_COLLECTION_MAX_OPERATIONS,
    DYNAMIC_SPARSE_CORE_COLLECTION_MAX_TRACE_EVENTS, DynamicSparseCoreCollectionError,
    DynamicSparseCoreCollectionEventKind, DynamicSparseCoreCollectionInput,
    DynamicSparseCoreCollectionMetrics, DynamicSparseCoreCollectionResult,
    DynamicSparseCoreCollectionSnapshot, DynamicSparseCoreCollectionStageEventKind,
    DynamicSparseCoreCollectionStageTraceEvent, DynamicSparseCoreCollectionStageTraceResult,
    DynamicSparseCoreCollectionTraceEvent, DynamicSparseCoreCollectionTraceResult,
    check_dynamic_sparse_core_collection_stage_trace, check_dynamic_sparse_core_collection_trace,
    execute_dynamic_sparse_core_collection, execute_dynamic_sparse_core_collection_stages,
    trace_dynamic_sparse_core_collection, trace_dynamic_sparse_core_collection_stages,
};
pub use dynamic_tree_blocking::{
    DYNAMIC_TREE_BLOCKING_MAX_EDGES, DYNAMIC_TREE_BLOCKING_MAX_NODES,
    DYNAMIC_TREE_BLOCKING_MAX_RESIDUAL_ARC_SCANS, DYNAMIC_TREE_BLOCKING_MAX_STATE_TRANSITIONS,
    DYNAMIC_TREE_BLOCKING_MAX_TRACE_EVENTS, DynamicTreeBlockingError, DynamicTreeBlockingMetrics,
    DynamicTreeBlockingResult, DynamicTreeBlockingTraceResult, solve_dynamic_tree_blocking_flow,
    solve_dynamic_tree_blocking_flow_with_feasibility, trace_dynamic_tree_blocking_flow,
    trace_dynamic_tree_blocking_flow_with_feasibility,
};
pub use dynamic_tree_chain_candidate_heap::{
    DynamicTreeChainCandidateHeapEntry, DynamicTreeChainCandidateHeapError,
    DynamicTreeChainCandidateHeapMetrics, DynamicTreeChainCandidateHeapRefreshTrace,
    DynamicTreeChainCandidateHeapState, DynamicTreeChainCandidateHeapTrace,
    DynamicTreeChainCandidateHeapTransition, check_dynamic_tree_chain_candidate_heap_refresh,
    check_dynamic_tree_chain_candidate_heap_trace, trace_dynamic_tree_chain_candidate_heap,
    trace_dynamic_tree_chain_candidate_heap_refresh,
};
pub use dynamic_tree_chain_epoch_runtime::{
    DynamicTreeChainEpochRuntimeError, DynamicTreeChainEpochRuntimeLevel,
    DynamicTreeChainEpochRuntimeLevelTrace, DynamicTreeChainEpochRuntimeMaterialization,
    DynamicTreeChainEpochRuntimeMetrics, DynamicTreeChainEpochRuntimeOperation,
    DynamicTreeChainEpochRuntimeResult, DynamicTreeChainEpochRuntimeState,
    DynamicTreeChainEpochRuntimeTraceEvent, DynamicTreeChainEpochRuntimeTraceResult,
    check_dynamic_tree_chain_epoch_runtime_materialization,
    check_dynamic_tree_chain_epoch_runtime_trace, execute_dynamic_tree_chain_epoch_runtime,
    initialize_dynamic_tree_chain_epoch_runtime, materialize_dynamic_tree_chain_epoch_runtime,
    trace_dynamic_tree_chain_epoch_runtime,
};
pub use dynamic_tree_chain_epochs::{
    DynamicTreeChainEpochError, DynamicTreeChainEpochLevel, DynamicTreeChainEpochMetrics,
    DynamicTreeChainEpochMwuPlan, DynamicTreeChainEpochOperation, DynamicTreeChainEpochSnapshot,
    DynamicTreeChainEpochTraceEvent, DynamicTreeChainEpochTraceResult,
    DynamicTreeChainEpochTransitionResult, check_dynamic_tree_chain_epoch_mwu_plan,
    check_dynamic_tree_chain_epoch_trace, execute_dynamic_tree_chain_epoch_transition,
    initialize_dynamic_tree_chain_epochs, plan_dynamic_tree_chain_rebuild_from_mwu,
    plan_dynamic_tree_chain_shift_from_mwu, trace_dynamic_tree_chain_epoch_transition,
};
pub use dynamic_tree_chain_propagation::{
    DynamicTreeChainPropagationError, DynamicTreeChainPropagationInput,
    DynamicTreeChainPropagationMetrics, DynamicTreeChainPropagationResult,
    DynamicTreeChainPropagationTraceResult, check_dynamic_tree_chain_propagation_trace,
    execute_dynamic_tree_chain_propagation, trace_dynamic_tree_chain_propagation,
};
pub use dynamic_tree_chain_query::{
    DYNAMIC_TREE_CHAIN_QUERY_MAX_SCALAR_BITS, DYNAMIC_TREE_CHAIN_QUERY_MAX_TRACE_EVENTS,
    DynamicTreeChainCycleCandidate, DynamicTreeChainCycleQueryError,
    DynamicTreeChainCycleQueryEventKind, DynamicTreeChainCycleQueryMetrics,
    DynamicTreeChainCycleQueryResult, DynamicTreeChainCycleQuerySnapshot,
    DynamicTreeChainCycleQueryTraceEvent, DynamicTreeChainCycleQueryTraceResult,
    DynamicTreeChainCycleSource, DynamicTreeChainTerminalBranch, DynamicTreeChainTerminalResult,
    DynamicTreeChainTerminalTraceResult, check_dynamic_tree_chain_cycle_query_trace,
    check_dynamic_tree_chain_terminal_collection_trace, find_dynamic_tree_chain_cycle,
    trace_dynamic_tree_chain_cycle_query, trace_dynamic_tree_chain_terminal_collection,
};
pub use dynamic_tree_push_relabel::{
    DYNAMIC_TREE_PUSH_RELABEL_MAX_ARC_SCANS, DYNAMIC_TREE_PUSH_RELABEL_MAX_EDGES,
    DYNAMIC_TREE_PUSH_RELABEL_MAX_NODES, DYNAMIC_TREE_PUSH_RELABEL_MAX_STATE_TRANSITIONS,
    DYNAMIC_TREE_PUSH_RELABEL_MAX_TRACE_EVENTS, DynamicTreePushRelabelError,
    DynamicTreePushRelabelMetrics, DynamicTreePushRelabelResult, DynamicTreePushRelabelTraceResult,
    solve_dynamic_tree_push_relabel, solve_dynamic_tree_push_relabel_with_feasibility,
    trace_dynamic_tree_push_relabel, trace_dynamic_tree_push_relabel_with_feasibility,
};
pub use eibfs::{
    DynamicEibfsMetrics, DynamicEibfsPrefixResult, DynamicEibfsResult, DynamicEibfsSolveError,
    DynamicEibfsTraceResult, EIBFS_MAX_AUGMENTATIONS, EIBFS_MAX_EDGES, EIBFS_MAX_NODES,
    EIBFS_MAX_RESIDUAL_ARC_SCANS, EIBFS_MAX_STATE_TRANSITIONS, EIBFS_MAX_TRACE_PROJECTION_UNITS,
    EibfsError, EibfsMetrics, EibfsResult, EibfsTraceResult, solve_dynamic_eibfs, solve_eibfs,
    trace_dynamic_eibfs, trace_eibfs, validate_eibfs_graph,
};
pub use electrical_flow::{
    ELECTRICAL_FLOW_ITERATION_MULTIPLIER, ELECTRICAL_FLOW_MAX_CAPACITY, ELECTRICAL_FLOW_MAX_EDGES,
    ELECTRICAL_FLOW_MAX_NODES, ELECTRICAL_FLOW_RELATIVE_TOLERANCE, ElectricalEdgeState,
    ElectricalExactRational, ElectricalFlowError, ElectricalFlowMetrics, ElectricalFlowResult,
    ElectricalFlowSnapshot, ElectricalFlowStage, ElectricalFlowTraceEvent,
    ElectricalFlowTraceResult, ElectricalScalar, check_electrical_flow_trace,
    solve_electrical_flow, trace_electrical_flow,
};
pub use electrical_flow_interior_point_mcf::{
    ELECTRICAL_IPM_MCF_DEFAULT_SEED, ELECTRICAL_IPM_MCF_MAX_BARRIER_REDUCTIONS,
    ELECTRICAL_IPM_MCF_MAX_CAPACITY, ELECTRICAL_IPM_MCF_MAX_CENTERING_STEPS,
    ELECTRICAL_IPM_MCF_MAX_COST, ELECTRICAL_IPM_MCF_MAX_EDGES,
    ELECTRICAL_IPM_MCF_MAX_ENUMERATED_ASSIGNMENTS, ELECTRICAL_IPM_MCF_MAX_ISOLATION_ATTEMPTS,
    ELECTRICAL_IPM_MCF_MAX_NODES, ELECTRICAL_IPM_MCF_MAX_TRACE_EVENTS, ElectricalIpmMcfEdgeState,
    ElectricalIpmMcfError, ElectricalIpmMcfMetrics, ElectricalIpmMcfNodeState,
    ElectricalIpmMcfResult, ElectricalIpmMcfScalar, ElectricalIpmMcfSnapshot,
    ElectricalIpmMcfStage, ElectricalIpmMcfTraceEvent, ElectricalIpmMcfTraceResult,
    check_electrical_flow_interior_point_mcf_trace, solve_electrical_flow_interior_point_mcf,
    solve_electrical_flow_interior_point_mcf_with_seed, trace_electrical_flow_interior_point_mcf,
    trace_electrical_flow_interior_point_mcf_with_feasibility,
    trace_electrical_flow_interior_point_mcf_with_seed,
};
pub use enhanced_capacity_scaling::{
    ENHANCED_CAPACITY_SCALING_MAX_AUGMENTATIONS, ENHANCED_CAPACITY_SCALING_MAX_CONTRACTIONS,
    ENHANCED_CAPACITY_SCALING_MAX_EDGES, ENHANCED_CAPACITY_SCALING_MAX_NODES,
    ENHANCED_CAPACITY_SCALING_MAX_PHASES, ENHANCED_CAPACITY_SCALING_MAX_SCANS,
    EnhancedCapacityScalingComponent, EnhancedCapacityScalingError, EnhancedCapacityScalingMetrics,
    EnhancedCapacityScalingResult, EnhancedCapacityScalingSnapshot, EnhancedCapacityScalingStage,
    EnhancedCapacityScalingTraceEvent, EnhancedCapacityScalingTraceResult,
    check_enhanced_capacity_scaling_trace, solve_enhanced_capacity_scaling,
    trace_enhanced_capacity_scaling, trace_enhanced_capacity_scaling_with_feasibility,
};
pub use epsilon_relaxation::{
    EPSILON_RELAXATION_EPSILON, EPSILON_RELAXATION_MAX_EDGES, EPSILON_RELAXATION_MAX_NODES,
    EPSILON_RELAXATION_MAX_RESIDUAL_ARC_SCANS, EPSILON_RELAXATION_MAX_STATE_TRANSITIONS,
    EPSILON_RELAXATION_MAX_UP_ITERATIONS, EpsilonRelaxationError, EpsilonRelaxationMetrics,
    EpsilonRelaxationResult, EpsilonRelaxationTraceResult, solve_epsilon_relaxation,
    trace_epsilon_relaxation, trace_epsilon_relaxation_with_feasibility,
};
pub use flow_framework_mcf::{
    FLOW_FRAMEWORK_MCF_MAX_ASSIGNMENTS, FLOW_FRAMEWORK_MCF_MAX_AUGMENTED_EDGES,
    FLOW_FRAMEWORK_MCF_MAX_CAPACITY, FLOW_FRAMEWORK_MCF_MAX_COST, FLOW_FRAMEWORK_MCF_MAX_EDGES,
    FLOW_FRAMEWORK_MCF_MAX_ITERATIONS, FLOW_FRAMEWORK_MCF_MAX_NODES,
    FLOW_FRAMEWORK_MCF_MAX_TRACE_ITERATIONS, FlowFrameworkMcfAugmentedEdgeState,
    FlowFrameworkMcfAugmentedNodeState, FlowFrameworkMcfError, FlowFrameworkMcfIteration,
    FlowFrameworkMcfResult, FlowFrameworkMcfRoundedSolution, FlowFrameworkMcfScalar,
    FlowFrameworkMcfSession, FlowFrameworkMcfSnapshot, FlowFrameworkMcfTermination,
    FlowFrameworkMcfTraceIteration, FlowFrameworkMcfTraceResult, check_flow_framework_mcf_trace,
    execute_flow_framework_mcf, flow_framework_mcf_stopping_gap, trace_flow_framework_mcf,
    trace_flow_framework_mcf_with_feasibility,
};
pub use flow_rounding::{
    COSTED_FLOW_ROUNDING_MAX_EDGES, COSTED_FLOW_ROUNDING_MAX_NODES,
    COSTED_FLOW_ROUNDING_MAX_RATIONAL_BITS, COSTED_FLOW_ROUNDING_MAX_TRACE_EVENTS,
    CostedFlowRoundingCycleArc, CostedFlowRoundingError, CostedFlowRoundingEventKind,
    CostedFlowRoundingMetrics, CostedFlowRoundingResult, CostedFlowRoundingSnapshot,
    CostedFlowRoundingTraceEvent, CostedFlowRoundingTraceResult, check_costed_flow_rounding_trace,
    round_costed_flow, trace_costed_flow_rounding,
};
pub use ford_fulkerson::{
    FORD_FULKERSON_MAX_AUGMENTATIONS, FORD_FULKERSON_MAX_EDGES, FORD_FULKERSON_MAX_NODES,
    FordFulkersonError, FordFulkersonExecutionPreset, FordFulkersonMetrics, FordFulkersonResult,
    FordFulkersonTraceResult, solve_capacity_scaling_augmenting_path, solve_dfs_ford_fulkerson,
    solve_ford_fulkerson, solve_ford_fulkerson_preset_with_feasibility,
    solve_widest_augmenting_path, trace_capacity_scaling_augmenting_path, trace_dfs_ford_fulkerson,
    trace_ford_fulkerson, trace_ford_fulkerson_preset_with_feasibility,
    trace_widest_augmenting_path,
};
pub use goldberg_rao::{
    BINARY_BLOCKING_TRACE_SCAN_BLOCK_MAX, BinaryBlockingAugmentation, BinaryBlockingStepResult,
    BinaryBlockingStepTraceResult, GOLDBERG_RAO_MAX_EDGES, GOLDBERG_RAO_MAX_NODES,
    GOLDBERG_RAO_MAX_RESIDUAL_ARC_SCANS, GOLDBERG_RAO_MAX_STATE_TRANSITIONS,
    GOLDBERG_RAO_MAX_TRACE_EVENTS, GoldbergRaoError, GoldbergRaoMetrics, GoldbergRaoResult,
    GoldbergRaoTraceResult, check_binary_blocking_step, check_binary_blocking_step_trace,
    solve_binary_blocking_first_step, solve_binary_blocking_step, solve_goldberg_rao,
    trace_binary_blocking_first_step, trace_goldberg_rao,
};
pub use hassin::{
    HASSIN_MAX_DUAL_ARC_SCANS, HASSIN_MAX_EDGES, HASSIN_MAX_NODES, HassinError, HassinMetrics,
    HassinResult, HassinTraceResult, solve_hassin_st_planar, trace_hassin_st_planar,
};
pub use hidden_stable_witness::{
    HIDDEN_STABLE_WITNESS_MAX_EDGES, HIDDEN_STABLE_WITNESS_MAX_NODES,
    HIDDEN_STABLE_WITNESS_MAX_RATIONAL_BITS, HIDDEN_STABLE_WITNESS_MAX_STAGES,
    HIDDEN_STABLE_WITNESS_MAX_TRACE_EVENTS, HiddenStableEdgeWitness, HiddenStableStageCertificate,
    HiddenStableWitnessConfig, HiddenStableWitnessError, HiddenStableWitnessEventKind,
    HiddenStableWitnessMetrics, HiddenStableWitnessResult, HiddenStableWitnessSnapshot,
    HiddenStableWitnessStage, HiddenStableWitnessTraceEvent, HiddenStableWitnessTraceResult,
    check_hidden_stable_witness_trace, trace_hidden_stable_witness, verify_hidden_stable_witness,
};
pub use hld_branch_free_tree::{
    HldBranchFreeTree, HldBranchFreeTreeError, build_hld_branch_free_tree,
    check_hld_branch_free_tree, hld_ancestor_closure, is_branch_free,
};
pub use hopcroft_karp::{
    HOPCROFT_KARP_MAX_EDGE_SCANS, HOPCROFT_KARP_MAX_EDGES, HOPCROFT_KARP_MAX_NODES,
    HOPCROFT_KARP_MAX_STATE_TRANSITIONS, HopcroftKarpError, HopcroftKarpMetrics,
    HopcroftKarpResult, HopcroftKarpTraceResult, solve_hopcroft_karp, trace_hopcroft_karp,
};
pub use hungarian::{
    HUNGARIAN_MAX_CELL_SCANS, HUNGARIAN_MAX_EDGES, HUNGARIAN_MAX_NODES,
    HUNGARIAN_MAX_STATE_TRANSITIONS, HungarianError, HungarianMetrics, HungarianOutcome,
    HungarianResult, HungarianTraceResult, solve_hungarian, trace_hungarian,
};
pub use ibfs::{
    IBFS_MAX_AUGMENTATIONS, IBFS_MAX_EDGES, IBFS_MAX_NODES, IBFS_MAX_RESIDUAL_ARC_SCANS,
    IBFS_MAX_STATE_TRANSITIONS, IbfsError, IbfsMetrics, IbfsResult, IbfsTraceResult, solve_ibfs,
    trace_ibfs, validate_ibfs_graph,
};
pub use interior_point_max_flow::{
    INTERIOR_POINT_MAX_FLOW_MAX_EDGES, INTERIOR_POINT_MAX_FLOW_MAX_NODES,
    INTERIOR_POINT_MAX_FLOW_MAX_PROGRESS_STEPS, INTERIOR_POINT_MAX_FLOW_MAX_TRACE_EVENTS,
    INTERIOR_POINT_MAX_FLOW_MAX_WORKING_EDGES, INTERIOR_POINT_MAX_FLOW_MAX_WORKING_NODES,
    InteriorPointEdgeState, InteriorPointMaxFlowError, InteriorPointMaxFlowMetrics,
    InteriorPointMaxFlowResult, InteriorPointMaxFlowSnapshot, InteriorPointMaxFlowStage,
    InteriorPointMaxFlowTraceEvent, InteriorPointMaxFlowTraceResult, InteriorPointNodeState,
    InteriorPointScalar, check_interior_point_max_flow_trace, solve_interior_point_max_flow,
    trace_interior_point_max_flow,
};
pub use low_stretch_forest_mwu::{
    LOW_STRETCH_FOREST_MWU_MAX_CANDIDATES, LOW_STRETCH_FOREST_MWU_MAX_EDGES,
    LOW_STRETCH_FOREST_MWU_MAX_NODES, LOW_STRETCH_FOREST_MWU_MAX_RATIONAL_BITS,
    LOW_STRETCH_FOREST_MWU_MAX_ROUNDS, LOW_STRETCH_FOREST_MWU_MAX_TAYLOR_TERMS,
    LOW_STRETCH_FOREST_MWU_MAX_TRACE_EVENTS, LOW_STRETCH_FOREST_MWU_MAX_TREE_SUBSETS,
    LowStretchForestMwuBranch, LowStretchForestMwuConfig, LowStretchForestMwuError,
    LowStretchForestMwuEventKind, LowStretchForestMwuMetrics, LowStretchForestMwuResult,
    LowStretchForestMwuSnapshot, LowStretchForestMwuTraceEvent, LowStretchForestMwuTraceResult,
    LowStretchForestTreePiece, build_low_stretch_forest_mwu_collection,
    check_low_stretch_forest_mwu_trace, trace_low_stretch_forest_mwu_collection,
};
pub use max_flow::{
    EDMONDS_KARP_MAX_AUGMENTATIONS, EDMONDS_KARP_MAX_EDGES, EDMONDS_KARP_MAX_NODES,
    EDMONDS_KARP_MAX_RESIDUAL_ARC_SCANS, EdmondsKarpError, EdmondsKarpMetrics, EdmondsKarpResult,
    EdmondsKarpTraceResult, solve_edmonds_karp, solve_edmonds_karp_with_feasibility,
    trace_edmonds_karp, trace_edmonds_karp_with_feasibility,
};
pub use min_cost::{
    BELLMAN_FORD_SSP_MAX_AUGMENTATIONS, BELLMAN_FORD_SSP_MAX_EDGES, BELLMAN_FORD_SSP_MAX_NODES,
    BellmanFordSspError, BellmanFordSspMetrics, BellmanFordSspResult, BellmanFordSspTraceResult,
    POTENTIAL_DIJKSTRA_SSP_MAX_AUGMENTATIONS, POTENTIAL_DIJKSTRA_SSP_MAX_EDGES,
    POTENTIAL_DIJKSTRA_SSP_MAX_NODES, PotentialDijkstraSspError, PotentialDijkstraSspMetrics,
    PotentialDijkstraSspResult, PotentialDijkstraSspTraceResult, SuccessiveShortestPathError,
    SuccessiveShortestPathResult, SuccessiveShortestPathTraceCheckError,
    SuccessiveShortestPathTraceResult, check_successive_shortest_path_trace,
    solve_bellman_ford_ssp, solve_potential_dijkstra_ssp, solve_successive_shortest_path,
    trace_bellman_ford_ssp, trace_bellman_ford_ssp_with_feasibility, trace_potential_dijkstra_ssp,
    trace_potential_dijkstra_ssp_with_feasibility, trace_successive_shortest_path,
    trace_successive_shortest_path_with_feasibility,
};
pub use minimum_mean_cycle_canceling::{
    MINIMUM_MEAN_CYCLE_CANCELING_MAX_CYCLES, MINIMUM_MEAN_CYCLE_CANCELING_MAX_EDGES,
    MINIMUM_MEAN_CYCLE_CANCELING_MAX_NODES, MINIMUM_MEAN_CYCLE_CANCELING_MAX_RESIDUAL_ARC_SCANS,
    MinimumMeanCycleCancelingError, MinimumMeanCycleCancelingMetrics,
    MinimumMeanCycleCancelingResult, MinimumMeanCycleCancelingTraceResult,
    solve_minimum_mean_cycle_canceling, trace_minimum_mean_cycle_canceling,
    trace_minimum_mean_cycle_canceling_with_feasibility,
};
pub use minimum_ratio_cycle::{
    MINIMUM_RATIO_CYCLE_MAX_ABS_GRADIENT, MINIMUM_RATIO_CYCLE_MAX_DFS_EXPANSIONS,
    MINIMUM_RATIO_CYCLE_MAX_EDGES, MINIMUM_RATIO_CYCLE_MAX_ENUMERATED_VECTORS,
    MINIMUM_RATIO_CYCLE_MAX_LENGTH, MINIMUM_RATIO_CYCLE_MAX_NODES,
    MINIMUM_RATIO_CYCLE_MAX_TRACE_EVENTS, MinimumRatioCycleArc, MinimumRatioCycleEdgeState,
    MinimumRatioCycleError, MinimumRatioCycleMetrics, MinimumRatioCycleNodeState,
    MinimumRatioCycleRational, MinimumRatioCycleResult, MinimumRatioCycleSnapshot,
    MinimumRatioCycleStage, MinimumRatioCycleTraceEvent, MinimumRatioCycleTraceResult,
    check_minimum_ratio_cycle_trace, solve_minimum_ratio_cycle, trace_minimum_ratio_cycle,
};
pub use minimum_ratio_cycle_mcf::{
    MINIMUM_RATIO_CYCLE_MCF_MAX_CAPACITY, MINIMUM_RATIO_CYCLE_MCF_MAX_COST,
    MINIMUM_RATIO_CYCLE_MCF_MAX_DFS_EXPANSIONS, MINIMUM_RATIO_CYCLE_MCF_MAX_EDGES,
    MINIMUM_RATIO_CYCLE_MCF_MAX_ENUMERATED_ASSIGNMENTS,
    MINIMUM_RATIO_CYCLE_MCF_MAX_ENUMERATED_VECTORS, MINIMUM_RATIO_CYCLE_MCF_MAX_NODES,
    MINIMUM_RATIO_CYCLE_MCF_MAX_TRACE_EVENTS, MinimumRatioCycleMcfArc,
    MinimumRatioCycleMcfEdgeState, MinimumRatioCycleMcfError, MinimumRatioCycleMcfMetrics,
    MinimumRatioCycleMcfNodeState, MinimumRatioCycleMcfResult, MinimumRatioCycleMcfScalar,
    MinimumRatioCycleMcfSnapshot, MinimumRatioCycleMcfStage, MinimumRatioCycleMcfTraceEvent,
    MinimumRatioCycleMcfTraceResult, check_minimum_ratio_cycle_mcf_trace,
    solve_minimum_ratio_cycle_mcf, trace_minimum_ratio_cycle_mcf,
    trace_minimum_ratio_cycle_mcf_with_feasibility,
};
pub use network_simplex::{
    DynamicTreeNetworkSimplexMetrics, DynamicTreeNetworkSimplexResult,
    DynamicTreeNetworkSimplexTraceResult, NETWORK_SIMPLEX_MAX_EDGES, NETWORK_SIMPLEX_MAX_NODES,
    NETWORK_SIMPLEX_MAX_PIVOTS, NETWORK_SIMPLEX_MAX_PRICING_ARC_SCANS, NetworkSimplexError,
    NetworkSimplexMetrics, NetworkSimplexResult, NetworkSimplexTraceResult,
    solve_dynamic_tree_network_simplex, solve_primal_network_simplex,
    trace_dynamic_tree_network_simplex, trace_dynamic_tree_network_simplex_with_feasibility,
    trace_primal_network_simplex, trace_primal_network_simplex_with_feasibility,
};
pub use orlin_max_flow::{
    ORLIN_MAX_FLOW_MAX_EDGES, ORLIN_MAX_FLOW_MAX_NODES, ORLIN_MAX_FLOW_MAX_PHASES,
    ORLIN_MAX_FLOW_MAX_SCANS, ORLIN_MAX_FLOW_MAX_TRACE_EVENTS, ORLIN_MAX_FLOW_MAX_TRANSITIONS,
    OrlinMaxCompactArcKind, OrlinMaxCompactArcState, OrlinMaxError, OrlinMaxMetrics,
    OrlinMaxNodeState, OrlinMaxPhaseCase, OrlinMaxResidualArcState, OrlinMaxResult,
    OrlinMaxSnapshot, OrlinMaxStage, OrlinMaxTraceEvent, OrlinMaxTraceResult,
    check_orlin_max_flow_trace, solve_orlin_max_flow, trace_orlin_max_flow,
};
pub use orlin_mcf::{
    ORLIN_MCF_MAX_AUGMENTATIONS, ORLIN_MCF_MAX_EDGES, ORLIN_MCF_MAX_NODES, ORLIN_MCF_MAX_PHASES,
    ORLIN_MCF_MAX_SCANS, ORLIN_MCF_MAX_TRANSFORMED_NODES, OrlinMcfArcId, OrlinMcfArcState,
    OrlinMcfBranch, OrlinMcfError, OrlinMcfMetrics, OrlinMcfNodeKind, OrlinMcfNodeState,
    OrlinMcfResult, OrlinMcfSnapshot, OrlinMcfStage, OrlinMcfTraceEvent, OrlinMcfTraceResult,
    check_orlin_mcf_trace, solve_orlin_mcf, trace_orlin_mcf, trace_orlin_mcf_with_feasibility,
};
pub use out_of_kilter::{
    OUT_OF_KILTER_MAX_CORRECTIONS, OUT_OF_KILTER_MAX_EDGES, OUT_OF_KILTER_MAX_KILTER_ARC_SCANS,
    OUT_OF_KILTER_MAX_NODES, OUT_OF_KILTER_MAX_RESIDUAL_ARC_SCANS, OutOfKilterError,
    OutOfKilterMetrics, OutOfKilterResult, OutOfKilterTraceResult, solve_out_of_kilter,
    trace_out_of_kilter, trace_out_of_kilter_with_feasibility,
};
pub use parametric_breakpoint_rerun::{
    PARAMETRIC_BREAKPOINT_RERUN_MAX_DECIMAL_DIGITS, PARAMETRIC_BREAKPOINT_RERUN_MAX_EDGES,
    PARAMETRIC_BREAKPOINT_RERUN_MAX_NODES, PARAMETRIC_BREAKPOINT_RERUN_MAX_SUBPROBLEMS,
    ParametricBreakpoint, ParametricBreakpointRerunError, ParametricBreakpointRerunMetrics,
    ParametricBreakpointRerunResult, ParametricBreakpointRerunTraceResult, ParametricCapacitySlope,
    ParametricCut, ParametricMaxFlowProblem, ParametricRational, ParametricSegment,
    ParametricTraceEvent, ParametricTraceEventKind, check_parametric_breakpoint_rerun_trace,
    solve_parametric_breakpoint_rerun, trace_parametric_breakpoint_rerun,
    trace_parametric_breakpoint_rerun_with_feasibility,
};
pub use parametric_pseudoflow::{
    ParametricPseudoflowError, ParametricPseudoflowEventKind, ParametricPseudoflowMetrics,
    ParametricPseudoflowResult, ParametricPseudoflowTraceEvent, ParametricPseudoflowTraceResult,
    ParametricRaceWinner, ParametricTraversalOrientation, ParametricWarmVerificationError,
    ParametricWarmVerificationMetrics, check_parametric_pseudoflow_trace,
    solve_parametric_pseudoflow, trace_parametric_pseudoflow, verify_parametric_warm_continuation,
};
pub use polynomial_dual_network_simplex::{
    POLYNOMIAL_DUAL_SIMPLEX_MAX_ARC_SCANS, POLYNOMIAL_DUAL_SIMPLEX_MAX_AUGMENTATIONS,
    POLYNOMIAL_DUAL_SIMPLEX_MAX_EDGES, POLYNOMIAL_DUAL_SIMPLEX_MAX_NODES,
    POLYNOMIAL_DUAL_SIMPLEX_MAX_PHASES, POLYNOMIAL_DUAL_SIMPLEX_MAX_PIVOTS,
    PolynomialDualResidualRef, PolynomialDualSimplexError, PolynomialDualSimplexMetrics,
    PolynomialDualSimplexResult, PolynomialDualSimplexSnapshot, PolynomialDualSimplexStage,
    PolynomialDualSimplexTraceEvent, PolynomialDualSimplexTraceResult,
    check_polynomial_dual_network_simplex_trace, solve_polynomial_dual_network_simplex,
    trace_polynomial_dual_network_simplex, trace_polynomial_dual_network_simplex_with_feasibility,
};
pub use polynomial_primal_network_simplex::{
    POLYNOMIAL_PRIMAL_SIMPLEX_MAX_ARC_SCANS, POLYNOMIAL_PRIMAL_SIMPLEX_MAX_EDGES,
    POLYNOMIAL_PRIMAL_SIMPLEX_MAX_NODES, POLYNOMIAL_PRIMAL_SIMPLEX_MAX_PHASES,
    POLYNOMIAL_PRIMAL_SIMPLEX_MAX_PIVOTS, PolynomialPrimalBasisState, PolynomialPrimalResidualRef,
    PolynomialPrimalScanKind, PolynomialPrimalSimplexError, PolynomialPrimalSimplexMetrics,
    PolynomialPrimalSimplexResult, PolynomialPrimalSimplexSnapshot, PolynomialPrimalSimplexStage,
    PolynomialPrimalSimplexTraceEvent, PolynomialPrimalSimplexTraceResult,
    check_polynomial_primal_network_simplex_trace, solve_polynomial_primal_network_simplex,
    trace_polynomial_primal_network_simplex,
    trace_polynomial_primal_network_simplex_with_feasibility,
};
pub use prediction_assisted_epsilon_relaxation::{
    PREDICTION_EPSILON_MAX_ATTEMPTS, PREDICTION_EPSILON_MAX_EDGES, PREDICTION_EPSILON_MAX_NODES,
    PREDICTION_EPSILON_MAX_RESIDUAL_ARC_SCANS, PREDICTION_EPSILON_MAX_STATE_TRANSITIONS,
    PREDICTION_EPSILON_MAX_TRACE_EVENTS, PREDICTION_EPSILON_MAX_TRACE_PROJECTION_UNITS,
    PredictionAssistedEpsilonError, PredictionAssistedEpsilonMetrics,
    PredictionAssistedEpsilonResult, PredictionAssistedEpsilonSnapshot,
    PredictionAssistedEpsilonStage, PredictionAssistedEpsilonTraceEvent,
    PredictionAssistedEpsilonTraceResult, check_prediction_assisted_epsilon_trace,
    solve_prediction_assisted_epsilon_relaxation, trace_prediction_assisted_epsilon_relaxation,
    trace_prediction_assisted_epsilon_relaxation_with_feasibility,
};
pub use primal_dual::{
    PrimalDualError, PrimalDualResult, PrimalDualTraceResult, solve_primal_dual, trace_primal_dual,
    trace_primal_dual_with_feasibility,
};
pub use primal_dual_interior_point_mcf::{
    PRIMAL_DUAL_IPM_MCF_DEFAULT_SEED, PRIMAL_DUAL_IPM_MCF_MAX_AUXILIARY_ARCS,
    PRIMAL_DUAL_IPM_MCF_MAX_CAPACITY, PRIMAL_DUAL_IPM_MCF_MAX_COST,
    PRIMAL_DUAL_IPM_MCF_MAX_CYCLE_UPDATES, PRIMAL_DUAL_IPM_MCF_MAX_EDGES,
    PRIMAL_DUAL_IPM_MCF_MAX_FOREST_SUBSETS, PRIMAL_DUAL_IPM_MCF_MAX_NODES,
    PRIMAL_DUAL_IPM_MCF_MAX_OUTER_ITERATIONS, PRIMAL_DUAL_IPM_MCF_MAX_TRACE_EVENTS,
    PrimalDualIpmArcKind, PrimalDualIpmArcState, PrimalDualIpmError, PrimalDualIpmMetrics,
    PrimalDualIpmNodeKind, PrimalDualIpmNodeState, PrimalDualIpmResult, PrimalDualIpmSnapshot,
    PrimalDualIpmStage, PrimalDualIpmTraceEvent, PrimalDualIpmTraceResult,
    check_primal_dual_interior_point_mcf_trace, solve_primal_dual_interior_point_mcf,
    solve_primal_dual_interior_point_mcf_with_seed, trace_primal_dual_interior_point_mcf,
    trace_primal_dual_interior_point_mcf_with_feasibility,
    trace_primal_dual_interior_point_mcf_with_seed,
};
pub use pseudoflow::{
    PSEUDOFLOW_MAX_EDGES, PSEUDOFLOW_MAX_NODES, PSEUDOFLOW_MAX_RESIDUAL_ARC_SCANS,
    PSEUDOFLOW_MAX_STATE_TRANSITIONS, PseudoflowError, PseudoflowMetrics, PseudoflowResult,
    PseudoflowSimplexMetrics, PseudoflowSimplexResult, PseudoflowSimplexTraceResult,
    PseudoflowTraceResult, check_pseudoflow_simplex_trace, solve_hochbaum_pseudoflow,
    solve_hochbaum_pseudoflow_with_feasibility, solve_pseudoflow_simplex,
    solve_pseudoflow_simplex_with_feasibility, trace_hochbaum_pseudoflow,
    trace_hochbaum_pseudoflow_with_feasibility, trace_pseudoflow_simplex,
    trace_pseudoflow_simplex_with_feasibility,
};
pub use push_relabel::{
    PARTIAL_AUGMENT_RELABEL_PATH_LENGTH, PUSH_RELABEL_GLOBAL_RELABEL_SCAN_MULTIPLIER,
    PUSH_RELABEL_MAX_EDGES, PUSH_RELABEL_MAX_NODES, PUSH_RELABEL_MAX_RESIDUAL_ARC_SCANS,
    PUSH_RELABEL_MAX_STATE_TRANSITIONS, PushRelabelError, PushRelabelExecutionPreset,
    PushRelabelMetrics, PushRelabelResult, PushRelabelTraceResult, solve_current_arc_push_relabel,
    solve_excess_scaling_push_relabel, solve_fifo_push_relabel, solve_gap_relabel_push_relabel,
    solve_generic_push_relabel, solve_global_relabel_push_relabel,
    solve_highest_label_push_relabel, solve_partial_augment_relabel,
    solve_push_relabel_preset_with_feasibility, solve_relabel_to_front,
    trace_current_arc_push_relabel, trace_excess_scaling_push_relabel, trace_fifo_push_relabel,
    trace_gap_relabel_push_relabel, trace_generic_push_relabel, trace_global_relabel_push_relabel,
    trace_highest_label_push_relabel, trace_partial_augment_relabel,
    trace_push_relabel_preset_with_feasibility, trace_relabel_to_front,
};
pub use randomized_almost_linear_max_flow::{
    RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_DEFAULT_SEED,
    RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_ASSIGNMENTS, RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_EDGES,
    RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_FORESTS, RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_IPM_STEPS,
    RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_ISOLATION_ATTEMPTS,
    RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_NODES, RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_REBUILDS,
    RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_TRACE_EVENTS, RandomizedAlmostLinearEdgeState,
    RandomizedAlmostLinearMaxFlowError, RandomizedAlmostLinearMaxFlowMetrics,
    RandomizedAlmostLinearMaxFlowResult, RandomizedAlmostLinearMaxFlowSnapshot,
    RandomizedAlmostLinearMaxFlowStage, RandomizedAlmostLinearMaxFlowTraceEvent,
    RandomizedAlmostLinearMaxFlowTraceResult, RandomizedAlmostLinearNodeState,
    RandomizedAlmostLinearProbability, RandomizedAlmostLinearScalar,
    check_randomized_almost_linear_max_flow_trace, solve_randomized_almost_linear_max_flow,
    solve_randomized_almost_linear_max_flow_with_seed, trace_randomized_almost_linear_max_flow,
    trace_randomized_almost_linear_max_flow_with_seed,
};
pub use randomized_almost_linear_mcf::{
    RANDOMIZED_ALMOST_LINEAR_MCF_DEFAULT_SEED, RANDOMIZED_ALMOST_LINEAR_MCF_MAX_ASSIGNMENTS,
    RANDOMIZED_ALMOST_LINEAR_MCF_MAX_CAPACITY, RANDOMIZED_ALMOST_LINEAR_MCF_MAX_COST,
    RANDOMIZED_ALMOST_LINEAR_MCF_MAX_EDGES, RANDOMIZED_ALMOST_LINEAR_MCF_MAX_ISOLATION_ATTEMPTS,
    RANDOMIZED_ALMOST_LINEAR_MCF_MAX_NODES, RANDOMIZED_ALMOST_LINEAR_MCF_MAX_TRACE_EVENTS,
    RandomizedAlmostLinearMcfEdgeState, RandomizedAlmostLinearMcfError,
    RandomizedAlmostLinearMcfMetrics, RandomizedAlmostLinearMcfNodeState,
    RandomizedAlmostLinearMcfProbability, RandomizedAlmostLinearMcfResult,
    RandomizedAlmostLinearMcfScalar, RandomizedAlmostLinearMcfSnapshot,
    RandomizedAlmostLinearMcfStage, RandomizedAlmostLinearMcfTraceEvent,
    RandomizedAlmostLinearMcfTraceResult, check_randomized_almost_linear_mcf_trace,
    solve_randomized_almost_linear_mcf, solve_randomized_almost_linear_mcf_with_seed,
    trace_randomized_almost_linear_mcf, trace_randomized_almost_linear_mcf_with_feasibility,
    trace_randomized_almost_linear_mcf_with_seed,
};
pub use relaxation::{
    RELAXATION_MAX_ARC_SCANS, RELAXATION_MAX_EDGES, RELAXATION_MAX_ITERATIONS,
    RELAXATION_MAX_NODES, RelaxationError, RelaxationMetrics, RelaxationResult,
    RelaxationTraceResult, solve_relaxation, trace_relaxation, trace_relaxation_with_feasibility,
};
pub use relaxed_mndc::{
    RELAXED_MNDC_MAX_ASSIGNMENT_SOLVES, RELAXED_MNDC_MAX_EDGES, RELAXED_MNDC_MAX_FAMILIES,
    RELAXED_MNDC_MAX_NODES, RELAXED_MNDC_MAX_PHASES, RELAXED_MNDC_MAX_SCANS,
    RelaxedMndcAssignmentChoice, RelaxedMndcCycle, RelaxedMndcEpsilon, RelaxedMndcError,
    RelaxedMndcMetrics, RelaxedMndcResult, RelaxedMndcSnapshot, RelaxedMndcStage,
    RelaxedMndcTraceEvent, RelaxedMndcTraceResult, check_relaxed_mndc_trace, solve_relaxed_mndc,
    trace_relaxed_mndc, trace_relaxed_mndc_with_feasibility,
};
pub use sap::{
    SAP_MAX_EDGES, SAP_MAX_NODES, SAP_MAX_RESIDUAL_ARC_SCANS, SAP_MAX_STATE_TRANSITIONS, SapError,
    SapExecutionPreset, SapMetrics, SapResult, SapTraceResult, solve_isap,
    solve_sap_preset_with_feasibility, solve_shortest_augmenting_path, trace_isap,
    trace_sap_preset_with_feasibility, trace_shortest_augmenting_path,
};
pub use shift_rebuild_game::{
    SHIFT_REBUILD_GAME_MAX_BRANCHES, SHIFT_REBUILD_GAME_MAX_DEPTH, SHIFT_REBUILD_GAME_MAX_PSI,
    SHIFT_REBUILD_GAME_MAX_ROUNDS, SHIFT_REBUILD_GAME_MAX_SHIFTS,
    SHIFT_REBUILD_GAME_MAX_TRACE_EVENTS, ShiftRebuildGameConfig, ShiftRebuildGameError,
    ShiftRebuildGameEventKind, ShiftRebuildGameMetrics, ShiftRebuildGameResult,
    ShiftRebuildGameSnapshot, ShiftRebuildGameStage, ShiftRebuildGameTraceEvent,
    ShiftRebuildGameTraceResult, ShiftRebuildLevelState, ShiftRebuildRound,
    check_shift_rebuild_game_trace, play_shift_rebuild_game, trace_shift_rebuild_game,
};
pub use shifted_tree_chain::{
    SHIFTED_TREE_CHAIN_MAX_BRANCHES, SHIFTED_TREE_CHAIN_MAX_DEPTH, SHIFTED_TREE_CHAIN_MAX_EDGES,
    SHIFTED_TREE_CHAIN_MAX_NODES, SHIFTED_TREE_CHAIN_MAX_OPERATIONS,
    SHIFTED_TREE_CHAIN_MAX_RATIONAL_BITS, SHIFTED_TREE_CHAIN_MAX_TRACE_EVENTS,
    SHIFTED_TREE_CHAIN_MAX_TREE_SUBSETS, ShiftedTreeChainBranch, ShiftedTreeChainConfig,
    ShiftedTreeChainEdge, ShiftedTreeChainEmbeddingArc, ShiftedTreeChainError,
    ShiftedTreeChainEventKind, ShiftedTreeChainGraph, ShiftedTreeChainLevel,
    ShiftedTreeChainMetrics, ShiftedTreeChainOperation, ShiftedTreeChainRecursiveBranch,
    ShiftedTreeChainResult, ShiftedTreeChainSnapshot, ShiftedTreeChainStage,
    ShiftedTreeChainTraceEvent, ShiftedTreeChainTraceResult, check_shifted_tree_chain_snapshot,
    check_shifted_tree_chain_trace, execute_shifted_tree_chain, trace_shifted_tree_chain,
};
pub use shifted_tree_chain_query::{
    SHIFTED_TREE_CHAIN_QUERY_MAX_COEFFICIENT_BITS, SHIFTED_TREE_CHAIN_QUERY_MAX_TRACE_EVENTS,
    ShiftedTreeChainCycleCandidate, ShiftedTreeChainCycleSource, ShiftedTreeChainQueryError,
    ShiftedTreeChainQueryEventKind, ShiftedTreeChainQueryMetrics, ShiftedTreeChainQueryResult,
    ShiftedTreeChainQuerySnapshot, ShiftedTreeChainQueryTraceEvent,
    ShiftedTreeChainQueryTraceResult, check_shifted_tree_chain_cycle_query_trace,
    find_shifted_tree_chain_cycle, trace_shifted_tree_chain_cycle_query,
};
pub use shifted_tree_chain_update::{
    SHIFTED_TREE_CHAIN_UPDATE_MAX_PSI, SHIFTED_TREE_CHAIN_UPDATE_MAX_RATIONAL_BITS,
    SHIFTED_TREE_CHAIN_UPDATE_MAX_SHIFTS, SHIFTED_TREE_CHAIN_UPDATE_MAX_TRACE_EVENTS,
    ShiftedTreeChainFlowApplication, ShiftedTreeChainQueryDecision, ShiftedTreeChainUpdateConfig,
    ShiftedTreeChainUpdateError, ShiftedTreeChainUpdateEventKind, ShiftedTreeChainUpdateMetrics,
    ShiftedTreeChainUpdateResult, ShiftedTreeChainUpdateSnapshot, ShiftedTreeChainUpdateTraceEvent,
    ShiftedTreeChainUpdateTraceResult, check_shifted_tree_chain_update_trace,
    execute_shifted_tree_chain_update, trace_shifted_tree_chain_update,
};
pub use successive_shortest_augmenting_path::{
    SSAP_MAX_AUGMENTATIONS, SSAP_MAX_EDGES, SSAP_MAX_NODES, SSAP_MAX_RESIDUAL_ARC_SCANS,
    SuccessiveShortestAugmentingPathError, SuccessiveShortestAugmentingPathMetrics,
    SuccessiveShortestAugmentingPathResult, SuccessiveShortestAugmentingPathTraceResult,
    solve_successive_shortest_augmenting_path, trace_successive_shortest_augmenting_path,
};
pub use synchronous_push_relabel::{
    SYNCHRONOUS_PUSH_RELABEL_MAX_ARC_SCANS, SYNCHRONOUS_PUSH_RELABEL_MAX_EDGES,
    SYNCHRONOUS_PUSH_RELABEL_MAX_NODES, SYNCHRONOUS_PUSH_RELABEL_MAX_TRANSITIONS,
    SynchronousPushRelabelError, SynchronousPushRelabelMetrics, SynchronousPushRelabelResult,
    SynchronousPushRelabelTraceCheckError, SynchronousPushRelabelTraceResult,
    check_synchronous_push_relabel_trace, solve_synchronous_parallel_push_relabel,
    trace_synchronous_parallel_push_relabel,
};
pub use tardos_framework::{
    TARDOS_FRAMEWORK_FIXED_TRACE_EVENTS, TARDOS_FRAMEWORK_MAX_EDGES, TARDOS_FRAMEWORK_MAX_NODES,
    TardosFixedBound, TardosFixedVariable, TardosFrameworkError, TardosFrameworkMetrics,
    TardosFrameworkResult, TardosFrameworkSnapshot, TardosFrameworkStage,
    TardosFrameworkTraceEvent, TardosFrameworkTraceResult, TardosResidualState,
    check_tardos_framework_trace, solve_tardos_framework_primitive,
    trace_tardos_framework_primitive, trace_tardos_framework_primitive_with_feasibility,
};
pub use transportation_simplex::{
    TRANSPORTATION_MAX_PIVOTS, TRANSPORTATION_MAX_PRICING_SCANS,
    TRANSPORTATION_MAX_STATE_TRANSITIONS, TRANSPORTATION_MAX_STRUCTURE_SCANS,
    TRANSPORTATION_MAX_TRACE_PROJECTION_CELLS, TransportationError, TransportationMetrics,
    TransportationPreset, TransportationResult, TransportationTraceResult,
    check_transportation_infeasibility, solve_modi, solve_transportation_simplex, trace_modi,
    trace_transportation_preset_with_feasibility, trace_transportation_simplex,
};
pub use warm_start_push_relabel::{
    WARM_START_PUSH_RELABEL_MAX_EDGES, WARM_START_PUSH_RELABEL_MAX_NODES,
    WARM_START_PUSH_RELABEL_MAX_RECOVERY_ARC_SCANS,
    WARM_START_PUSH_RELABEL_MAX_RECOVERY_TRANSITIONS, WarmStartPushRelabelError,
    WarmStartPushRelabelMetrics, WarmStartPushRelabelResult, WarmStartPushRelabelTraceResult,
    check_warm_start_push_relabel_trace, solve_warm_start_push_relabel,
    trace_warm_start_push_relabel,
};
pub use weighted_augmenting_paths::{
    WEIGHTED_AUGMENTING_PATHS_MAX_AUGMENTATIONS, WEIGHTED_AUGMENTING_PATHS_MAX_CAPACITY,
    WEIGHTED_AUGMENTING_PATHS_MAX_CUTS, WEIGHTED_AUGMENTING_PATHS_MAX_EDGES,
    WEIGHTED_AUGMENTING_PATHS_MAX_NODES, WEIGHTED_AUGMENTING_PATHS_MAX_RELABEL_JUMPS,
    WEIGHTED_AUGMENTING_PATHS_MAX_ROUNDS, WEIGHTED_AUGMENTING_PATHS_MAX_TRACE_EVENTS,
    WeightedAugmentingEdgeState, WeightedAugmentingHierarchyKind, WeightedAugmentingNodeState,
    WeightedAugmentingPathsError, WeightedAugmentingPathsMetrics, WeightedAugmentingPathsResult,
    WeightedAugmentingPathsSnapshot, WeightedAugmentingPathsStage,
    WeightedAugmentingPathsTraceEvent, WeightedAugmentingPathsTraceResult,
    WeightedAugmentingResidualArcState, solve_weighted_augmenting_paths,
    trace_weighted_augmenting_paths, verify_weighted_augmenting_paths_trace,
};
pub use weighted_push_relabel_shortcut::{
    WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_AUGMENTATIONS, WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_CAPACITY,
    WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_EDGES, WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_NODES,
    WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_RELABEL_STEPS,
    WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_TRACE_EVENTS, WeightedPushRelabelShortcutArcId,
    WeightedPushRelabelShortcutDirection, WeightedPushRelabelShortcutEdgeKind,
    WeightedPushRelabelShortcutEdgeState, WeightedPushRelabelShortcutError,
    WeightedPushRelabelShortcutMetrics, WeightedPushRelabelShortcutNodeState,
    WeightedPushRelabelShortcutResidualArcState, WeightedPushRelabelShortcutResult,
    WeightedPushRelabelShortcutSnapshot, WeightedPushRelabelShortcutStage,
    WeightedPushRelabelShortcutTraceEvent, WeightedPushRelabelShortcutTraceResult,
    solve_weighted_push_relabel_shortcut, trace_weighted_push_relabel_shortcut,
    verify_weighted_push_relabel_shortcut_trace,
};

// Explicit feasibility bridges used by enclosing runtimes. Keeping these
// separate from ordinary public solvers makes hidden preprocessing impossible
// to opt into accidentally while preserving the untracked standalone API.
pub use blocking_primal_dual::solve_blocking_primal_dual_with_feasibility;
pub use cancel_and_tighten::solve_cancel_and_tighten_with_feasibility;
pub use capacity_scaling::{
    solve_capacity_scaling_with_feasibility, solve_excess_scaling_mcf_with_feasibility,
};
pub use convex_cost::solve_segment_expanded_convex_cost_with_feasibility;
pub use convex_cost_scaling::solve_convex_cost_scaling_with_feasibility;
pub use convex_network_simplex::solve_convex_network_simplex_with_feasibility;
pub use cost_scaling::solve_cost_scaling_preset_with_feasibility;
pub use cycle_canceling::solve_simple_cycle_canceling_with_feasibility;
pub use double_scaling::solve_double_scaling_with_feasibility;
pub use dual_network_simplex::solve_dual_network_simplex_with_feasibility;
pub use electrical_flow_interior_point_mcf::solve_electrical_flow_interior_point_mcf_with_feasibility;
pub use enhanced_capacity_scaling::solve_enhanced_capacity_scaling_with_feasibility;
pub use epsilon_relaxation::solve_epsilon_relaxation_with_feasibility;
pub use flow_framework_mcf::execute_flow_framework_mcf_with_feasibility;
pub use min_cost::{
    solve_bellman_ford_ssp_with_feasibility, solve_potential_dijkstra_ssp_with_feasibility,
    solve_successive_shortest_path_with_feasibility,
};
pub use minimum_mean_cycle_canceling::solve_minimum_mean_cycle_canceling_with_feasibility;
pub use minimum_ratio_cycle_mcf::solve_minimum_ratio_cycle_mcf_with_feasibility;
pub use network_simplex::{
    solve_dynamic_tree_network_simplex_with_feasibility,
    solve_primal_network_simplex_with_feasibility,
};
pub use orlin_mcf::solve_orlin_mcf_with_feasibility;
pub use out_of_kilter::solve_out_of_kilter_with_feasibility;
pub use parametric_breakpoint_rerun::solve_parametric_breakpoint_rerun_with_feasibility;
pub use polynomial_dual_network_simplex::solve_polynomial_dual_network_simplex_with_feasibility;
pub use polynomial_primal_network_simplex::solve_polynomial_primal_network_simplex_with_feasibility;
pub use prediction_assisted_epsilon_relaxation::solve_prediction_assisted_epsilon_relaxation_with_feasibility;
pub use primal_dual::solve_primal_dual_with_feasibility;
pub use primal_dual_interior_point_mcf::solve_primal_dual_interior_point_mcf_with_feasibility;
pub use randomized_almost_linear_mcf::solve_randomized_almost_linear_mcf_with_feasibility;
pub use relaxation::solve_relaxation_with_feasibility;
pub use relaxed_mndc::solve_relaxed_mndc_with_feasibility;
pub use tardos_framework::solve_tardos_framework_primitive_with_feasibility;
pub use transportation_simplex::solve_transportation_preset_with_feasibility;

#[cfg(test)]
mod stable_scene_decimal_tests {
    use super::stable_scene_decimal;

    #[test]
    fn removes_cross_target_last_bit_drift() {
        assert_eq!(
            stable_scene_decimal(1_841.203_712_178_942_9),
            stable_scene_decimal(1_841.203_712_178_942_6)
        );
        assert_eq!(
            stable_scene_decimal(51.404_415_276_939_226),
            stable_scene_decimal(51.404_415_276_939_23)
        );
    }

    #[test]
    fn stays_readable_across_scales_and_normalizes_negative_zero() {
        assert_eq!(stable_scene_decimal(12.5), "12.5");
        assert_eq!(stable_scene_decimal(0.000_000_000_001), "0.000000000001");
        assert_eq!(stable_scene_decimal(-0.0), "0");
    }
}
