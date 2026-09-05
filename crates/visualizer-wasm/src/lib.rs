//! Scenario-driven WebAssembly session adapter.

#![forbid(unsafe_code)]

mod flow_runtime;
mod flow_trace_contract;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};

use flow_runtime::{
    AssignmentRunner, BlockingPreflowRunner, CapacityScalingRunner, ClassicalMinCostFlowRunner,
    ClassifyRuntimeFailure, ConvexCostFlowRunner, CostScalingRunner, DinicRunner,
    DistanceDirectedRunner, FordFulkersonRunner, MaxFlowRunner, MinCostFlowRunner,
    NetworkSimplexRunner, ParametricMaxFlowRunner, PlanarMaxFlowRunner, PseudoflowRunner,
    PushRelabelRunner, RuntimeFailure, RuntimeRunner, SapRunner, TransportationRunner,
};
use flow_trace_contract::{
    exact_event_work_unit, is_source_detail_primitive, is_source_primary_work_boundary,
    source_detail_allows_two_node_auxiliary_focus,
};

use flow::{
    AlgorithmId, AuctionError, AuctionOutcome, AugmentingElectricalError,
    AugmentingElectricalResult, AugmentingElectricalSnapshot, AugmentingElectricalStage,
    AugmentingElectricalTraceEvent, AugmentingElectricalTraceResult, BellmanFordSspError,
    BlockingPreflowError, BlockingPrimalDualError, BorradaileKleinError, BoykovKolmogorovError,
    CancelTightenError, CancelTightenRational, CancelTightenSnapshot, CancelTightenStage,
    CancelTightenTraceEvent, CapacityScalingError, CatalogModelKind, ConvexCostError,
    ConvexCostProblem, ConvexCostScalingError, ConvexCostScalingSnapshot, ConvexCostScalingStage,
    ConvexCostScalingTraceEvent, ConvexCostSnapshot, ConvexCostStage, ConvexNetworkSimplexError,
    ConvexNetworkSimplexSnapshot, ConvexNetworkSimplexStage, ConvexNetworkSimplexTraceEvent,
    CostScalingError, DeterministicAlmostLinearCycleKind, DeterministicAlmostLinearMaxFlowError,
    DeterministicAlmostLinearMaxFlowResult, DeterministicAlmostLinearMaxFlowSnapshot,
    DeterministicAlmostLinearMaxFlowStage, DeterministicAlmostLinearMaxFlowTraceEvent,
    DeterministicAlmostLinearMaxFlowTraceResult, DinicError, DistanceDirectedError,
    DoubleScalingArcId, DoubleScalingBranch, DoubleScalingError, DoubleScalingNodeRef,
    DoubleScalingSnapshot, DoubleScalingStage, DoubleScalingTraceEvent, DualNetworkSimplexError,
    DualNetworkSimplexSnapshot, DualNetworkSimplexStage, DualNetworkSimplexTraceEvent,
    DynamicCapacityUpdate, DynamicEibfsError, DynamicEibfsSolveError,
    DynamicMinRatioCycleEventKind, DynamicTreeBlockingError, DynamicTreePushRelabelError,
    EibfsError, ElectricalFlowError, ElectricalFlowResult, ElectricalFlowSnapshot,
    ElectricalFlowStage, ElectricalFlowTraceEvent, ElectricalFlowTraceResult,
    ElectricalIpmMcfError, ElectricalIpmMcfResult, ElectricalIpmMcfSnapshot, ElectricalIpmMcfStage,
    ElectricalIpmMcfTraceEvent, ElectricalIpmMcfTraceResult, EnhancedCapacityScalingError,
    EnhancedCapacityScalingSnapshot, EnhancedCapacityScalingStage,
    EnhancedCapacityScalingTraceEvent, EpsilonRelaxationError, FeasibilityError,
    FlowAssignmentMetrics, FlowAuctionMetrics, FlowAugmentingElectricalEdgeStateV1,
    FlowAugmentingElectricalNodeStateV1, FlowAugmentingElectricalOverlayV1,
    FlowAugmentingElectricalStageV1, FlowAugmentingPathMetrics, FlowBinaryBlockingStageV1,
    FlowBipartiteMatchingMetrics, FlowBlockingFlowMetrics, FlowBlockingPreflowMetrics,
    FlowBlockingPrimalDualMetrics, FlowCancelTightenNodeStateV1, FlowCancelTightenOverlayV1,
    FlowCancelTightenStageV1, FlowCapacityScalingMetrics, FlowConvexCostArcRefV1,
    FlowConvexCostBoundary, FlowConvexCostEdgeStateV1, FlowConvexCostOverlayV1,
    FlowConvexCostSegmentStateV1, FlowConvexCostStageV1, FlowConvexNetworkSimplexArcRefV1,
    FlowConvexNetworkSimplexArtificialEdgeV1, FlowConvexNetworkSimplexBasisV1,
    FlowConvexNetworkSimplexEdgeStateV1, FlowConvexNetworkSimplexNodeStateV1,
    FlowConvexNetworkSimplexOverlayV1, FlowConvexNetworkSimplexStageV1, FlowCostScalingMetrics,
    FlowCurrentSceneV9, FlowCycleCancelingMetrics, FlowDetailStepCapabilityV1,
    FlowDeterministicAlmostLinearCycleKindV1, FlowDeterministicAlmostLinearEdgeStateV1,
    FlowDeterministicAlmostLinearNodeStateV1, FlowDeterministicAlmostLinearOverlayV1,
    FlowDeterministicAlmostLinearStageV1, FlowDistanceDirectedMetrics, FlowDistanceLabelMetrics,
    FlowDoubleScalingArcRefV1, FlowDoubleScalingEdgeStateV1, FlowDoubleScalingNodeKindV1,
    FlowDoubleScalingNodeStateV1, FlowDoubleScalingOverlayV1, FlowDoubleScalingStageV1,
    FlowDualNetworkSimplexEdgeStateV1, FlowDualNetworkSimplexNodeStateV1,
    FlowDualNetworkSimplexOverlayV1, FlowDualNetworkSimplexStageV1, FlowDynamicTreeBlockingMetrics,
    FlowDynamicTreeNetworkSimplexMetrics, FlowDynamicTreePushRelabelMetrics, FlowEibfsMetrics,
    FlowElectricalEdgeStateV1, FlowElectricalFlowOverlayV1, FlowElectricalFlowStageV1,
    FlowElectricalIpmMcfEdgeStateV1, FlowElectricalIpmMcfNodeStateV1,
    FlowElectricalIpmMcfOverlayV1, FlowElectricalIpmMcfStageV1, FlowElectricalNodeStateV1,
    FlowEnhancedCapacityScalingComponentV1, FlowEnhancedCapacityScalingEdgeStateV1,
    FlowEnhancedCapacityScalingNodeStateV1, FlowEnhancedCapacityScalingOverlayV1,
    FlowEnhancedCapacityScalingStageV1, FlowFrameworkMcfDynamicOperationV1,
    FlowFrameworkMcfEdgeStateV1, FlowFrameworkMcfError, FlowFrameworkMcfFinalPointEdgeV1,
    FlowFrameworkMcfFinalPointNodeV1, FlowFrameworkMcfIteration, FlowFrameworkMcfLevelStateV1,
    FlowFrameworkMcfOverlayV1, FlowFrameworkMcfResult, FlowFrameworkMcfStageV1,
    FlowFrameworkMcfTraceResult, FlowGoldbergRaoMetrics, FlowIbfsMetrics,
    FlowInteriorPointEdgeStateV1, FlowInteriorPointMaxFlowOverlayV1,
    FlowInteriorPointMaxFlowStageV1, FlowInteriorPointNodeStateV1, FlowLeftmostPlanarMetrics,
    FlowMinimumMeanCycleCancelingMetrics, FlowMinimumRatioCycleEdgeStateV1,
    FlowMinimumRatioCycleMcfEdgeStateV1, FlowMinimumRatioCycleMcfNodeStateV1,
    FlowMinimumRatioCycleMcfOverlayV1, FlowMinimumRatioCycleMcfStageV1,
    FlowMinimumRatioCycleNodeStateV1, FlowMinimumRatioCycleOverlayV1, FlowMinimumRatioCycleStageV1,
    FlowNetworkSimplexMetrics, FlowOrlinMaxFlowCompactArcKindV1, FlowOrlinMaxFlowCompactArcRefV1,
    FlowOrlinMaxFlowCompactArcStateV1, FlowOrlinMaxFlowNodeStateV1, FlowOrlinMaxFlowOverlayV1,
    FlowOrlinMaxFlowPhaseCaseV1, FlowOrlinMaxFlowResidualArcStateV1, FlowOrlinMaxFlowStageV1,
    FlowOrlinMcfArcRefV1, FlowOrlinMcfArcStateV1, FlowOrlinMcfBranchV1, FlowOrlinMcfComponentV1,
    FlowOrlinMcfNodeKindV1, FlowOrlinMcfNodeStateV1, FlowOrlinMcfOverlayV1, FlowOrlinMcfStageV1,
    FlowOutOfKilterMetrics, FlowParametricBreakpointV1, FlowParametricEdgeCapacityV1,
    FlowParametricMetricsV1, FlowParametricOverlayV1, FlowParametricSegmentV1,
    FlowParametricTraversalV1, FlowPlanarMetrics, FlowPolynomialDualEdgeStateV1,
    FlowPolynomialDualNodeStateV1, FlowPolynomialDualSimplexOverlayV1,
    FlowPolynomialDualSimplexStageV1, FlowPolynomialPrimalArtificialEdgeStateV1,
    FlowPolynomialPrimalBasisStateV1, FlowPolynomialPrimalEdgeStateV1,
    FlowPolynomialPrimalNodeFlagV1, FlowPolynomialPrimalNodeKindV1,
    FlowPolynomialPrimalNodeStateV1, FlowPolynomialPrimalResidualRefV1,
    FlowPolynomialPrimalSimplexOverlayV1, FlowPolynomialPrimalSimplexStageV1,
    FlowPotentialDijkstraMetrics, FlowPredictionAssistedEpsilonEdgeStateV1,
    FlowPredictionAssistedEpsilonNodeStateV1, FlowPredictionAssistedEpsilonOverlayV1,
    FlowPredictionAssistedEpsilonStageV1, FlowPrimalDualIpmMcfArcKindV1,
    FlowPrimalDualIpmMcfArcStateV1, FlowPrimalDualIpmMcfNodeKindV1,
    FlowPrimalDualIpmMcfNodeStateV1, FlowPrimalDualIpmMcfOverlayV1, FlowPrimalDualIpmMcfRatioV1,
    FlowPrimalDualIpmMcfStageV1, FlowProblemModelV1, FlowPseudoflowMetrics, FlowPushRelabelMetrics,
    FlowRandomizedAlmostLinearEdgeStateV1, FlowRandomizedAlmostLinearMcfEdgeStateV1,
    FlowRandomizedAlmostLinearMcfNodeStateV1, FlowRandomizedAlmostLinearMcfOverlayV1,
    FlowRandomizedAlmostLinearMcfStageV1, FlowRandomizedAlmostLinearNodeStateV1,
    FlowRandomizedAlmostLinearOverlayV1, FlowRandomizedAlmostLinearProbabilityV1,
    FlowRandomizedAlmostLinearStageV1, FlowRationalV1, FlowRelaxedMndcAssignmentCellV1,
    FlowRelaxedMndcCycleV1, FlowRelaxedMndcNodeStateV1, FlowRelaxedMndcOverlayV1,
    FlowRelaxedMndcStageV1, FlowResidualArcRefV1, FlowResourceLimitReasonV1, FlowScenarioV1,
    FlowSolveStatusV1, FlowStepAvailabilityV1, FlowTardosFixedVariableV1,
    FlowTardosFrameworkOverlayV1, FlowTardosFrameworkStageV1, FlowTardosNodeStateV1,
    FlowTardosResidualStateV1, FlowTraceDirection, FlowTraceEntityRefSceneV1, FlowTraceError,
    FlowTraceEventDetailSceneV1, FlowTraceEventRoleV1, FlowTraceEventSceneV1,
    FlowTraceEventSemanticsV1, FlowTraceMetrics, FlowTracePrimaryWorkBlockV1, FlowTraceSnapshot,
    FlowTraceWorkDeltaV1, FlowTraceWorkProgressV1, FlowTraceWorkUnitV1, FlowTransportationMetrics,
    FlowUpdateV1, FlowWarmStartMetrics, FlowWeightedAugmentingEdgeStateV1,
    FlowWeightedAugmentingHierarchyKindV1, FlowWeightedAugmentingNodeStateV1,
    FlowWeightedAugmentingPathsOverlayV1, FlowWeightedAugmentingPathsStageV1,
    FlowWeightedAugmentingResidualArcStateV1, FlowWeightedPushRelabelShortcutArcRefV1,
    FlowWeightedPushRelabelShortcutEdgeStateV1, FlowWeightedPushRelabelShortcutNodeStateV1,
    FlowWeightedPushRelabelShortcutOverlayV1, FlowWeightedPushRelabelShortcutResidualArcStateV1,
    FlowWeightedPushRelabelShortcutStageV1, FordFulkersonError, GoldbergRaoError, HassinError,
    HopcroftKarpError, HungarianError, HungarianOutcome, IbfsError, ImplementationStatus,
    InteriorPointMaxFlowError, InteriorPointMaxFlowResult, InteriorPointMaxFlowSnapshot,
    InteriorPointMaxFlowStage, InteriorPointMaxFlowTraceEvent, InteriorPointMaxFlowTraceResult,
    MinimumMeanCycleCancelingError, MinimumRatioCycleError, MinimumRatioCycleMcfError,
    MinimumRatioCycleMcfResult, MinimumRatioCycleMcfSnapshot, MinimumRatioCycleMcfStage,
    MinimumRatioCycleMcfTraceEvent, MinimumRatioCycleMcfTraceResult, MinimumRatioCycleResult,
    MinimumRatioCycleSnapshot, MinimumRatioCycleStage, MinimumRatioCycleTraceEvent,
    MinimumRatioCycleTraceResult, NetworkSimplexError, NodeId, OrlinMaxCompactArcKind,
    OrlinMaxError, OrlinMaxPhaseCase, OrlinMaxSnapshot, OrlinMaxStage, OrlinMaxTraceEvent,
    OrlinMcfArcId, OrlinMcfBranch, OrlinMcfError, OrlinMcfNodeKind, OrlinMcfSnapshot,
    OrlinMcfStage, OrlinMcfTraceEvent, OutOfKilterError,
    PREDICTION_EPSILON_MAX_TRACE_PROJECTION_UNITS, ParametricBreakpointRerunMetrics,
    ParametricPseudoflowEventKind, ParametricPseudoflowMetrics, ParametricRaceWinner,
    ParametricRational, ParametricTraceEventKind, ParametricTraversalOrientation,
    PolynomialDualResidualRef, PolynomialDualSimplexError, PolynomialDualSimplexSnapshot,
    PolynomialDualSimplexStage, PolynomialDualSimplexTraceEvent, PolynomialPrimalBasisState,
    PolynomialPrimalResidualRef, PolynomialPrimalScanKind, PolynomialPrimalSimplexError,
    PolynomialPrimalSimplexSnapshot, PolynomialPrimalSimplexStage,
    PolynomialPrimalSimplexTraceEvent, PotentialDijkstraSspError, PredictionAssistedEpsilonError,
    PredictionAssistedEpsilonSnapshot, PredictionAssistedEpsilonStage,
    PredictionAssistedEpsilonTraceEvent, PredictionAssistedEpsilonTraceResult, PrimalDualError,
    PrimalDualIpmArcKind, PrimalDualIpmError, PrimalDualIpmNodeKind, PrimalDualIpmResult,
    PrimalDualIpmSnapshot, PrimalDualIpmStage, PrimalDualIpmTraceEvent, PrimalDualIpmTraceResult,
    PseudoflowError, PushRelabelError, RandomizedAlmostLinearMaxFlowError,
    RandomizedAlmostLinearMaxFlowResult, RandomizedAlmostLinearMaxFlowSnapshot,
    RandomizedAlmostLinearMaxFlowStage, RandomizedAlmostLinearMaxFlowTraceEvent,
    RandomizedAlmostLinearMaxFlowTraceResult, RandomizedAlmostLinearMcfError,
    RandomizedAlmostLinearMcfResult, RandomizedAlmostLinearMcfSnapshot,
    RandomizedAlmostLinearMcfStage, RandomizedAlmostLinearMcfTraceEvent,
    RandomizedAlmostLinearMcfTraceResult, RelaxationError, RelaxedMndcError, RelaxedMndcSnapshot,
    RelaxedMndcStage, RelaxedMndcTraceEvent, ResidualArcId, ResidualDirection, ResidualState,
    RunProfileV1, RuntimeRouteKind, SapError, SimpleCycleCancelingError,
    SuccessiveShortestAugmentingPathError, SynchronousPushRelabelError, TardosFixedBound,
    TardosFrameworkError, TardosFrameworkSnapshot, TardosFrameworkStage, TardosFrameworkTraceEvent,
    TardosFrameworkTraceResult, TraceGranularityV1, TransportationError, WarmStartPushRelabelError,
    WeightedAugmentingHierarchyKind, WeightedAugmentingPathsError, WeightedAugmentingPathsResult,
    WeightedAugmentingPathsSnapshot, WeightedAugmentingPathsStage,
    WeightedAugmentingPathsTraceEvent, WeightedAugmentingPathsTraceResult,
    WeightedPushRelabelShortcutDirection, WeightedPushRelabelShortcutEdgeKind,
    WeightedPushRelabelShortcutError, WeightedPushRelabelShortcutResult,
    WeightedPushRelabelShortcutSnapshot, WeightedPushRelabelShortcutStage,
    WeightedPushRelabelShortcutTraceEvent, WeightedPushRelabelShortcutTraceResult,
    algorithm_catalog, apply_trace_event, check_balance_infeasibility,
    check_max_flow_infeasibility, check_transportation_infeasibility, decode_flow_dsl,
    decode_flow_scenario, find_algorithm_by_id, fixed_flow_divergences,
    generate_flow_graph_json as generate_flow_graph_candidate, generator_algorithm_fixtures,
    prepare_dynamic_eibfs, solve_auction, solve_augmenting_electrical_flow,
    solve_binary_blocking_first_step, solve_borradaile_klein_planar, solve_boykov_kolmogorov,
    solve_deterministic_almost_linear_max_flow, solve_distance_directed_dd2,
    solve_distance_directed_scaling, solve_dynamic_eibfs, solve_eibfs, solve_electrical_flow,
    solve_goldberg_rao, solve_hassin_st_planar, solve_hopcroft_karp, solve_hungarian, solve_ibfs,
    solve_interior_point_max_flow, solve_minimum_ratio_cycle, solve_orlin_max_flow,
    solve_parametric_pseudoflow, solve_randomized_almost_linear_max_flow,
    solve_successive_shortest_augmenting_path, solve_synchronous_parallel_push_relabel,
    solve_warm_start_push_relabel, solve_weighted_augmenting_paths,
    solve_weighted_push_relabel_shortcut, supply_divergences, trace_auction,
    trace_augmenting_electrical_flow, trace_binary_blocking_first_step,
    trace_borradaile_klein_planar, trace_boykov_kolmogorov,
    trace_deterministic_almost_linear_max_flow, trace_distance_directed_dd2,
    trace_distance_directed_scaling, trace_dynamic_eibfs, trace_eibfs, trace_electrical_flow,
    trace_goldberg_rao, trace_hassin_st_planar, trace_hopcroft_karp, trace_hungarian, trace_ibfs,
    trace_interior_point_max_flow, trace_minimum_ratio_cycle, trace_orlin_max_flow,
    trace_parametric_pseudoflow, trace_randomized_almost_linear_max_flow,
    trace_successive_shortest_augmenting_path, trace_synchronous_parallel_push_relabel,
    trace_warm_start_push_relabel, trace_weighted_augmenting_paths,
    trace_weighted_push_relabel_shortcut, validate_unit_capacity_dinic_graph,
    validate_unit_network_dinic_graph,
};
use num_rational::BigRational;
use ordered_map::{
    AlgorithmInstance, CanonicalSnapshot, Operation as ModelOperation, OperationResult, OrderedMap,
    StatePatchRecord, StructureSnapshot, TraceEvent,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use visualizer_core::dsl::{parse_initial, parse_operations, validate_document_size};
use visualizer_core::generator::{
    InitialGeneratorSpec, OperationGeneratorSpec, generate_initial, generate_operations,
};
use visualizer_core::jcs::canonicalize;
use visualizer_core::plugin::engine_contract_v1;
use visualizer_core::scenario::Entry;
use visualizer_core::scenario::{
    Operation, ScenarioV1, decode_ordered_map, decode_scenario_envelope,
};
use wasm_bindgen::prelude::*;

/// Strictly validates and RFC 8785-canonicalizes a Scenario.
///
/// # Errors
///
/// Rejects invalid JSON, unsupported contracts, and noncanonical bounded values.
#[wasm_bindgen]
pub fn canonical_scenario_json(source: &str) -> Result<String, JsError> {
    let scenario =
        decode_ordered_map(source.as_bytes()).map_err(|error| JsError::new(&error.to_string()))?;
    canonicalize_scenario(&scenario)
}

/// Strictly validates and canonicalizes a flow Scenario.
///
/// # Errors
///
/// Rejects invalid JSON, unsupported contracts, and invalid graph values.
#[wasm_bindgen]
pub fn canonical_flow_scenario_json(source: &str) -> Result<String, JsError> {
    let scenario = decode_flow_scenario(source.as_bytes())
        .map_err(|error| JsError::new(&error.to_string()))?;
    canonicalize_serializable(&scenario)
}

/// Parses Flow DSL, applies the same strict semantic validation as JSON, and
/// returns canonical Scenario JSON.
///
/// # Errors
///
/// Rejects malformed DSL with a stable diagnostic code and UTF-16 location, or
/// any graph/model value rejected by the typed flow Scenario contract.
#[wasm_bindgen]
pub fn canonical_flow_dsl_json(source: &str) -> Result<String, JsError> {
    let scenario = decode_flow_dsl(source).map_err(|error| JsError::new(&error.to_string()))?;
    canonicalize_serializable(&scenario)
}

/// Returns the closed runtime/plugin transport handshake as canonical JSON.
///
/// # Errors
///
/// Returns an error only if the build-time contract cannot be serialized.
#[wasm_bindgen]
pub fn engine_contract_json() -> Result<String, JsError> {
    canonicalize_serializable(&engine_contract_v1())
}

/// Returns the complete source-backed flow algorithm catalog as canonical JSON.
///
/// # Errors
///
/// Returns an error only if the static descriptor table cannot be serialized.
#[wasm_bindgen]
pub fn flow_algorithm_catalog_json() -> Result<String, JsError> {
    canonicalize_serializable(&algorithm_catalog())
}

/// Returns descriptor-level conformance contracts joined to the strict source table.
///
/// # Errors
///
/// Rejects any malformed source row or catalog source key that is not present
/// in the Confirmed table before serialization is attempted.
#[wasm_bindgen]
pub fn flow_algorithm_conformance_contracts_json() -> Result<String, JsError> {
    let contracts = flow::flow_algorithm_conformance_contracts()
        .map_err(|error| JsError::new(&error.to_string()))?;
    canonicalize_serializable(&contracts)
}

/// Returns the canonical 50-family generator/algorithm fixture manifest.
///
/// The payload includes trace, fast, and practical-boundary generator specs,
/// total catalog compatibility records, and finite expected-counter evidence.
///
/// # Errors
///
/// Returns an error only if the static fixture manifest cannot be serialized.
#[wasm_bindgen]
pub fn flow_generator_fixture_manifest_json() -> Result<String, JsError> {
    canonicalize_serializable(&generator_algorithm_fixtures())
}

fn canonicalize_scenario(scenario: &ScenarioV1) -> Result<String, JsError> {
    canonicalize_serializable(scenario)
}

fn canonicalize_serializable(value: &impl Serialize) -> Result<String, JsError> {
    let encoded = serde_json::to_vec(value).map_err(|error| JsError::new(&error.to_string()))?;
    let canonical = canonicalize(&encoded).map_err(|error| JsError::new(&error.to_string()))?;
    String::from_utf8(canonical).map_err(|error| JsError::new(&error.to_string()))
}

/// Validates an explicitly edited Scenario and declares the derived revisions
/// produced by this build.
///
/// # Errors
///
/// Rejects invalid Scenario input or canonical serialization failure.
#[wasm_bindgen]
pub fn canonical_edited_scenario_json(source: &str) -> Result<String, JsError> {
    let mut scenario =
        decode_ordered_map(source.as_bytes()).map_err(|error| JsError::new(&error.to_string()))?;
    scenario.declare_current_derived_revisions();
    canonicalize_scenario(&scenario)
}

/// Returns whether an untouched imported Scenario declares historical derived
/// output that this build does not reproduce.
///
/// # Errors
///
/// Rejects invalid Scenario input.
#[wasm_bindgen]
pub fn scenario_has_legacy_derived_revisions(source: &str) -> Result<bool, JsError> {
    let scenario =
        decode_ordered_map(source.as_bytes()).map_err(|error| JsError::new(&error.to_string()))?;
    Ok(scenario.has_legacy_derived_revisions())
}

/// Parses strict initial-entry DSL and returns a JSON entry array.
///
/// # Errors
///
/// Returns a stable source diagnostic for invalid DSL.
#[wasm_bindgen]
pub fn parse_initial_dsl_json(source: &str) -> Result<String, JsError> {
    let entries = parse_initial(source.as_bytes()).map_err(|error| {
        JsError::new(&serde_json::to_string(&error).unwrap_or_else(|_| error.to_string()))
    })?;
    serde_json::to_string(&entries).map_err(|error| JsError::new(&error.to_string()))
}

/// Parses strict operation DSL and returns a JSON operation array.
///
/// # Errors
///
/// Returns a stable source diagnostic for invalid DSL.
#[wasm_bindgen]
pub fn parse_operations_dsl_json(source: &str) -> Result<String, JsError> {
    let operations = parse_operations(source.as_bytes()).map_err(|error| {
        JsError::new(&serde_json::to_string(&error).unwrap_or_else(|_| error.to_string()))
    })?;
    serde_json::to_string(&operations).map_err(|error| JsError::new(&error.to_string()))
}

/// Validates the shared byte budget of the initial and operation DSL streams.
///
/// # Errors
///
/// Returns a stable resource-limit diagnostic when their combined UTF-8 bytes
/// exceed the manual-input limit.
#[wasm_bindgen]
pub fn validate_dsl_document_size(initial: &str, operations: &str) -> Result<(), JsError> {
    validate_document_size(initial.as_bytes(), operations.as_bytes()).map_err(|error| {
        JsError::new(&serde_json::to_string(&error).unwrap_or_else(|_| error.to_string()))
    })
}

/// Materializes an initial generator spec and its provenance.
///
/// # Errors
///
/// Rejects invalid or infeasible generator settings.
#[wasm_bindgen]
pub fn generate_initial_json(spec_json: &str) -> Result<String, JsError> {
    let spec: InitialGeneratorSpec =
        serde_json::from_str(spec_json).map_err(|error| JsError::new(&error.to_string()))?;
    let generated = generate_initial(&spec).map_err(|error| JsError::new(&error.to_string()))?;
    serde_json::to_string(&generated).map_err(|error| JsError::new(&error.to_string()))
}

/// Materializes an operation generator spec from an initial entry sequence.
///
/// # Errors
///
/// Rejects invalid initial JSON and invalid or infeasible generator settings.
#[wasm_bindgen]
pub fn generate_operations_json(spec_json: &str, initial_json: &str) -> Result<String, JsError> {
    let spec: OperationGeneratorSpec =
        serde_json::from_str(spec_json).map_err(|error| JsError::new(&error.to_string()))?;
    let initial: Vec<Entry> =
        serde_json::from_str(initial_json).map_err(|error| JsError::new(&error.to_string()))?;
    let generated =
        generate_operations(&spec, &initial).map_err(|error| JsError::new(&error.to_string()))?;
    serde_json::to_string(&generated).map_err(|error| JsError::new(&error.to_string()))
}

/// Materializes a deterministic flow graph candidate and provenance.
///
/// # Errors
///
/// Rejects an invalid generator DTO, preflight size violation, bounded RNG
/// failure, or canonical digest failure.
#[wasm_bindgen]
pub fn generate_flow_graph_json(spec_json: &str) -> Result<String, JsError> {
    let generated = generate_flow_graph_candidate(spec_json)
        .map_err(|error| JsError::new(&error.to_string()))?;
    serde_json::to_string(&generated).map_err(|error| JsError::new(&error.to_string()))
}

/// One reversible operation delta returned to the Worker.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionFrame {
    base_item_index: usize,
    item_index: usize,
    item_count: usize,
    initial_build: bool,
    result: OperationResult,
    trace: Vec<TraceEvent>,
    patches: Vec<StatePatchRecord>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CurrentSessionFrame {
    item_index: usize,
    item_count: usize,
    structure: StructureSnapshot,
    canonical: CanonicalSnapshot,
}

#[derive(Clone, Debug)]
struct WorkItem {
    initial_build: bool,
    operation: ModelOperation,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeekProgress {
    cursor: usize,
    done: bool,
    target: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame: Option<CurrentSessionFrame>,
}

const CHECKPOINT_INTERVAL: usize = 2_048;
const MAX_CHECKPOINTS: usize = 32;
const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;
const MAX_FRAME_JSON_BYTES: usize = 32 * 1024 * 1024 - 16;

struct BoundedJsonWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedJsonWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(16 * 1024),
            limit,
        }
    }
}

impl Write for BoundedJsonWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(buffer.len()) > self.limit {
            return Err(io::Error::other("frame JSON byte limit exceeded"));
        }
        self.bytes
            .try_reserve(buffer.len())
            .map_err(|_| io::Error::other("frame JSON allocation failed"))?;
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn serialize_frame(value: &impl Serialize) -> Result<String, JsError> {
    serialize_frame_with_limit(value, MAX_FRAME_JSON_BYTES).map_err(|error| JsError::new(&error))
}

fn serialize_frame_with_limit(value: &impl Serialize, limit: usize) -> Result<String, String> {
    let mut writer = BoundedJsonWriter::new(limit);
    serde_json::to_writer(&mut writer, value).map_err(|error| error.to_string())?;
    String::from_utf8(writer.bytes).map_err(|error| error.to_string())
}

#[derive(Clone, Debug)]
struct Checkpoint {
    cursor: usize,
    estimated_bytes: usize,
    algorithm: AlgorithmInstance,
}

#[derive(Clone, Copy, Debug)]
struct StagedNext;

#[derive(Clone, Debug)]
struct StagedSeek {
    algorithm: AlgorithmInstance,
    cursor: usize,
    target: usize,
}

/// Ordered-map plugin state owned by one Worker job.
struct OrderedMapSession {
    scenario: ScenarioV1,
    algorithm: AlgorithmInstance,
    work: Vec<WorkItem>,
    cursor: usize,
    staged_seek: Option<StagedSeek>,
    checkpoints: Vec<Checkpoint>,
    checkpoint_bytes: usize,
    index_algorithm: Option<AlgorithmInstance>,
    index_cursor: usize,
    staged_next: Option<StagedNext>,
}

impl OrderedMapSession {
    /// Strictly decodes a Scenario and creates its selected algorithm instance.
    ///
    /// When `show_build` is false, initial inserts are applied before this
    /// constructor returns and are not exposed as timeline items.
    ///
    /// # Errors
    ///
    /// Returns a JavaScript error for invalid JSON, unsupported revisions,
    /// noncanonical keys, invalid configuration, or bounded runtime failure.
    pub fn new(scenario_json: &str) -> Result<Self, JsError> {
        let scenario = decode_ordered_map(scenario_json.as_bytes())
            .map_err(|error| JsError::new(&error.to_string()))?;
        Self::from_scenario(scenario).map_err(|error| JsError::new(&error))
    }

    /// Stable selected algorithm identifier.
    pub fn algorithm_id(&self) -> String {
        self.algorithm.id().to_owned()
    }

    /// Number of visible timeline items.
    pub fn item_count(&self) -> usize {
        self.work.len()
    }

    /// Number of items committed since the current reset/seek.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Highest timeline boundary covered by the background seek index.
    pub fn seek_coverage(&self) -> usize {
        self.index_cursor
    }

    /// Serializes the current full scene without applying another operation.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn current_frame_json(&self) -> Result<String, JsError> {
        serialize_frame(&self.current_frame()?)
    }

    /// Applies and serializes the next visible item without publishing its cursor.
    ///
    /// Returns `undefined` after the end boundary. The algorithm is advanced only
    /// inside the Worker's synchronous request turn; the committed cursor remains
    /// unchanged until [`WasmSession::commit_staged_next`] is called. A caller
    /// that cannot publish the packet must call [`WasmSession::discard_staged_next`]
    /// to reconstruct the last committed boundary.
    ///
    /// # Errors
    ///
    /// Returns an error for bounded algorithm failure or serialization failure.
    pub fn stage_next_json(&mut self) -> Result<Option<String>, JsError> {
        self.stage_next_json_with_limit(MAX_FRAME_JSON_BYTES)
            .map_err(|error| JsError::new(&error))
    }

    /// Publishes the previously staged item after packet transfer succeeds.
    pub fn commit_staged_next(&mut self) {
        if self.staged_next.take().is_none() {
            return;
        }
        self.cursor += 1;
        self.maybe_store_active_checkpoint();
    }

    /// Reconstructs the committed boundary after packet publication fails.
    ///
    /// # Errors
    ///
    /// Returns an error if the last committed boundary cannot be reconstructed.
    pub fn discard_staged_next(&mut self) -> Result<(), JsError> {
        if self.staged_next.take().is_some() {
            self.restore_current_boundary()
                .map_err(|error| JsError::new(&error))?;
        }
        Ok(())
    }

    fn stage_next_json_with_limit(
        &mut self,
        frame_json_limit: usize,
    ) -> Result<Option<String>, String> {
        if self.staged_seek.is_some() {
            return Err("cannot step while a seek is active".to_owned());
        }
        if self.staged_next.is_some() {
            return Err("a staged step is already pending".to_owned());
        }
        let Some(item) = self.work.get(self.cursor).cloned() else {
            return Ok(None);
        };
        let base_item_index = self.cursor;
        let recorded = match self
            .algorithm
            .apply_recorded_reconstructible(item.operation)
        {
            Ok(recorded) => recorded,
            Err(error) => {
                self.restore_current_boundary()?;
                return Err(error.to_string());
            }
        };
        let frame = SessionFrame {
            base_item_index,
            item_index: self.cursor + 1,
            item_count: self.work.len(),
            initial_build: item.initial_build,
            result: recorded.result,
            trace: recorded.trace,
            patches: recorded.patches,
        };
        let json = match serialize_frame_with_limit(&frame, frame_json_limit) {
            Ok(json) => json,
            Err(error) => {
                self.restore_current_boundary()?;
                return Err(error);
            }
        };
        self.staged_next = Some(StagedNext);
        Ok(Some(json))
    }

    /// Rebuilds the session and commits exactly `target` items from its start.
    ///
    /// # Errors
    ///
    /// Rejects a target after the end boundary and propagates replay failure.
    pub fn seek_json(&mut self, target: usize) -> Result<String, JsError> {
        self.begin_seek(target)?;
        while self
            .staged_seek
            .as_ref()
            .is_some_and(|seek| seek.cursor != seek.target)
        {
            self.resume_seek_json(4_096)?;
        }
        let frame = self
            .staged_seek
            .as_ref()
            .ok_or_else(|| JsError::new("seek candidate is missing"))?;
        let json = serialize_frame(&CurrentSessionFrame {
            item_index: frame.cursor,
            item_count: self.work.len(),
            structure: frame.algorithm.structure_snapshot(),
            canonical: frame.algorithm.canonical_snapshot(),
        })?;
        self.commit_staged_seek();
        Ok(json)
    }

    /// Starts a cancellable seek without replaying more than the hidden initial build.
    ///
    /// Forward seeks continue from the current state. Backward seeks restore the
    /// effective initial boundary and are then resumed in bounded chunks.
    ///
    /// # Errors
    ///
    /// Rejects a target after the end boundary or a failed initial restore.
    pub fn begin_seek(&mut self, target: usize) -> Result<(), JsError> {
        if target > self.work.len() {
            return Err(JsError::new("seek target exceeds item count"));
        }
        let current_distance = target.checked_sub(self.cursor);
        let checkpoint = self
            .checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.cursor <= target)
            .max_by_key(|checkpoint| checkpoint.cursor)
            .filter(|checkpoint| {
                current_distance.is_none_or(|distance| target - checkpoint.cursor < distance)
            })
            .cloned();
        let (algorithm, cursor) = if let Some(checkpoint) = checkpoint {
            (checkpoint.algorithm, checkpoint.cursor)
        } else if target < self.cursor {
            let replacement =
                Self::from_scenario(self.scenario.clone()).map_err(|error| JsError::new(&error))?;
            (replacement.algorithm, 0)
        } else {
            (self.algorithm.clone(), self.cursor)
        };
        self.staged_seek = Some(StagedSeek {
            algorithm,
            cursor,
            target,
        });
        Ok(())
    }

    /// Replays at most `max_items` and returns progress plus the final frame.
    ///
    /// # Errors
    ///
    /// Rejects a zero chunk, a missing seek, algorithm failure, or serialization failure.
    pub fn resume_seek_json(&mut self, max_items: usize) -> Result<String, JsError> {
        self.resume_seek_json_with_limit(max_items, MAX_FRAME_JSON_BYTES)
            .map_err(|error| JsError::new(&error))
    }

    /// Publishes the staged seek only after its final packet transfer succeeds.
    pub fn commit_staged_seek(&mut self) {
        let Some(staged) = self.staged_seek.take() else {
            return;
        };
        self.algorithm = staged.algorithm;
        self.cursor = staged.cursor;
        self.maybe_store_active_checkpoint();
    }

    /// Drops an in-progress or completed seek candidate without changing current state.
    pub fn discard_staged_seek(&mut self) {
        self.staged_seek = None;
    }

    fn resume_seek_json_with_limit(
        &mut self,
        max_items: usize,
        frame_json_limit: usize,
    ) -> Result<String, String> {
        if max_items == 0 {
            return Err("seek chunk must be positive".to_owned());
        }
        let seek = self
            .staged_seek
            .as_mut()
            .ok_or_else(|| "no active seek".to_owned())?;
        let stop = seek.target.min(seek.cursor.saturating_add(max_items));
        while seek.cursor < stop {
            let item = self
                .work
                .get(seek.cursor)
                .ok_or_else(|| "seek replay exceeded item count".to_owned())?;
            seek.algorithm
                .apply(item.operation.clone(), &mut Vec::new())
                .map_err(|error| error.to_string())?;
            if seek.algorithm.structure_entity_count() > ordered_map::MAX_VISUAL_ENTITIES {
                return Err("ordered-map resource limit exceeded: visual entity count".to_owned());
            }
            seek.cursor += 1;
        }
        let done = seek.cursor == seek.target;
        let frame = if done {
            Some(CurrentSessionFrame {
                item_index: seek.cursor,
                item_count: self.work.len(),
                structure: seek.algorithm.structure_snapshot(),
                canonical: seek.algorithm.canonical_snapshot(),
            })
        } else {
            None
        };
        serialize_frame_with_limit(
            &SeekProgress {
                cursor: seek.cursor,
                done,
                target: seek.target,
                frame,
            },
            frame_json_limit,
        )
    }

    /// Advances the independent background seek index by a bounded number of items.
    ///
    /// Returns `true` once every timeline boundary is covered.
    ///
    /// # Errors
    ///
    /// Rejects a zero chunk and propagates algorithm failures.
    pub fn resume_seek_index(&mut self, max_items: usize) -> Result<bool, JsError> {
        if max_items == 0 {
            return Err(JsError::new("seek-index chunk must be positive"));
        }
        let Some(mut algorithm) = self.index_algorithm.take() else {
            return Ok(true);
        };
        let stop = self
            .work
            .len()
            .min(self.index_cursor.saturating_add(max_items));
        while self.index_cursor < stop {
            let item = self
                .work
                .get(self.index_cursor)
                .ok_or_else(|| JsError::new("seek index exceeded item count"))?;
            algorithm
                .apply(item.operation.clone(), &mut Vec::new())
                .map_err(|error| JsError::new(&error.to_string()))?;
            if algorithm.structure_entity_count() > ordered_map::MAX_VISUAL_ENTITIES {
                return Err(JsError::new(
                    "ordered-map resource limit exceeded: visual entity count",
                ));
            }
            self.index_cursor += 1;
            if self.index_cursor.is_multiple_of(CHECKPOINT_INTERVAL) {
                let estimated_bytes = algorithm.estimated_bytes();
                self.store_checkpoint_with_limits(
                    self.index_cursor,
                    estimated_bytes,
                    MAX_CHECKPOINT_BYTES,
                    MAX_CHECKPOINTS,
                    |_| algorithm.clone(),
                );
            }
        }
        let done = self.index_cursor == self.work.len();
        if done {
            let estimated_bytes = algorithm.estimated_bytes();
            self.store_checkpoint_with_limits(
                self.index_cursor,
                estimated_bytes,
                MAX_CHECKPOINT_BYTES,
                MAX_CHECKPOINTS,
                |_| algorithm,
            );
        } else {
            self.index_algorithm = Some(algorithm);
        }
        Ok(done)
    }

    /// Returns the strict canonical Scenario representation retained by the
    /// session. This does not depend on Worker health.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn scenario_json(&self) -> Result<String, JsError> {
        let encoded = serde_json::to_string(&self.scenario)
            .map_err(|error| JsError::new(&error.to_string()))?;
        canonical_scenario_json(&encoded)
    }
}

impl OrderedMapSession {
    fn restore_current_boundary(&mut self) -> Result<(), String> {
        let target = self.cursor;
        let checkpoint = self
            .checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.cursor <= target)
            .max_by_key(|checkpoint| checkpoint.cursor)
            .cloned();
        let (mut algorithm, mut cursor) = if let Some(checkpoint) = checkpoint {
            (checkpoint.algorithm, checkpoint.cursor)
        } else {
            let replacement = Self::from_scenario(self.scenario.clone())?;
            (replacement.algorithm, 0)
        };
        while cursor < target {
            let item = self
                .work
                .get(cursor)
                .ok_or_else(|| "rollback replay exceeded item count".to_owned())?;
            algorithm
                .apply(item.operation.clone(), &mut Vec::new())
                .map_err(|error| error.to_string())?;
            if algorithm.structure_entity_count() > ordered_map::MAX_VISUAL_ENTITIES {
                return Err("ordered-map resource limit exceeded: visual entity count".to_owned());
            }
            cursor += 1;
        }
        self.algorithm = algorithm;
        Ok(())
    }

    fn current_frame(&self) -> Result<CurrentSessionFrame, JsError> {
        if self.algorithm.structure_entity_count() > ordered_map::MAX_VISUAL_ENTITIES {
            return Err(JsError::new(
                "ordered-map resource limit exceeded: visual entity count",
            ));
        }
        Ok(CurrentSessionFrame {
            item_index: self.cursor,
            item_count: self.work.len(),
            structure: self.algorithm.structure_snapshot(),
            canonical: self.algorithm.canonical_snapshot(),
        })
    }

    fn from_scenario(scenario: ScenarioV1) -> Result<Self, String> {
        let seed = scenario
            .payload
            .algorithm_seed
            .parse::<u64>()
            .map_err(|_| "algorithm_seed is not a canonical u64".to_owned())?;
        let mut algorithm = AlgorithmInstance::from_spec(&scenario.payload.algorithm, seed)
            .map_err(|error| error.to_string())?;
        let initial = scenario
            .payload
            .initial
            .entries
            .iter()
            .map(|entry| {
                Ok(ModelOperation::Insert {
                    key: parse_key(&entry.key)?,
                    value: entry.value.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut work = Vec::with_capacity(
            scenario.payload.operations.items.len()
                + if scenario.payload.initial.show_build {
                    initial.len()
                } else {
                    0
                },
        );
        if scenario.payload.initial.show_build {
            work.extend(initial.into_iter().map(|operation| WorkItem {
                initial_build: true,
                operation,
            }));
        } else {
            for operation in initial {
                algorithm
                    .apply(operation, &mut Vec::new())
                    .map_err(|error| error.to_string())?;
                if algorithm.structure_entity_count() > ordered_map::MAX_VISUAL_ENTITIES {
                    return Err(
                        "ordered-map resource limit exceeded: visual entity count".to_owned()
                    );
                }
            }
        }
        for operation in &scenario.payload.operations.items {
            work.push(WorkItem {
                initial_build: false,
                operation: convert_operation(operation)?,
            });
        }
        let initial_estimated_bytes = algorithm.estimated_bytes();
        let checkpoint_allowed = initial_estimated_bytes <= MAX_CHECKPOINT_BYTES;
        let checkpoint_bytes = if checkpoint_allowed {
            initial_estimated_bytes
        } else {
            0
        };
        let checkpoints = if checkpoint_allowed {
            vec![Checkpoint {
                cursor: 0,
                estimated_bytes: initial_estimated_bytes,
                algorithm: algorithm.clone(),
            }]
        } else {
            Vec::new()
        };
        Ok(Self {
            scenario,
            index_algorithm: Some(algorithm.clone()),
            algorithm,
            work,
            cursor: 0,
            staged_seek: None,
            checkpoints,
            checkpoint_bytes,
            index_cursor: 0,
            staged_next: None,
        })
    }

    fn maybe_store_active_checkpoint(&mut self) {
        if self.cursor.is_multiple_of(CHECKPOINT_INTERVAL) || self.cursor == self.work.len() {
            let estimated_bytes = self.algorithm.estimated_bytes();
            self.store_checkpoint_with_limits(
                self.cursor,
                estimated_bytes,
                MAX_CHECKPOINT_BYTES,
                MAX_CHECKPOINTS,
                |session| session.algorithm.clone(),
            );
        }
    }

    fn store_checkpoint_with_limits(
        &mut self,
        cursor: usize,
        estimated_bytes: usize,
        max_bytes: usize,
        max_checkpoints: usize,
        factory: impl FnOnce(&Self) -> AlgorithmInstance,
    ) -> bool {
        if !self.admit_checkpoint_with_limits(cursor, estimated_bytes, max_bytes, max_checkpoints) {
            return false;
        }
        let algorithm = factory(self);
        self.insert_checkpoint(cursor, estimated_bytes, algorithm);
        true
    }

    fn admit_checkpoint_with_limits(
        &mut self,
        cursor: usize,
        estimated_bytes: usize,
        max_bytes: usize,
        max_checkpoints: usize,
    ) -> bool {
        if self
            .checkpoints
            .iter()
            .any(|checkpoint| checkpoint.cursor == cursor)
        {
            return false;
        }
        if estimated_bytes > max_bytes || max_checkpoints == 0 {
            return false;
        }
        while self.checkpoints.len() >= max_checkpoints
            || self.checkpoint_bytes.saturating_add(estimated_bytes) > max_bytes
        {
            if self.checkpoints.is_empty() {
                return false;
            }
            let remove = self.checkpoint_to_evict();
            let removed = self.checkpoints.remove(remove);
            self.checkpoint_bytes = self
                .checkpoint_bytes
                .saturating_sub(removed.estimated_bytes);
        }
        true
    }

    fn insert_checkpoint(
        &mut self,
        cursor: usize,
        estimated_bytes: usize,
        algorithm: AlgorithmInstance,
    ) {
        self.checkpoints.push(Checkpoint {
            cursor,
            estimated_bytes,
            algorithm,
        });
        self.checkpoints
            .sort_unstable_by_key(|checkpoint| checkpoint.cursor);
        self.checkpoint_bytes = self.checkpoint_bytes.saturating_add(estimated_bytes);
    }

    fn checkpoint_to_evict(&self) -> usize {
        if self.checkpoints.len() <= 2 {
            return 0;
        }
        (1..self.checkpoints.len() - 1)
            .min_by_key(|&index| {
                self.checkpoints[index + 1].cursor - self.checkpoints[index - 1].cursor
            })
            .unwrap_or(1)
    }
}

const MAX_FLOW_FRAME_CACHE_BYTES: usize = 64 * 1024 * 1024;
// The byte budget is the authoritative memory ceiling. Keeping every admitted
// small-graph Detail frame avoids regenerating a long deterministic trace each
// time sequential playback crosses an arbitrary 256-frame window.
const MAX_FLOW_FRAME_CACHE_FRAMES: usize = 64;
const FLOW_TIMELINE_FINGERPRINT_DOMAIN: &[u8] = b"flow-timeline-cache/v1\0";

struct PreparedFlowTimeline {
    frames: Vec<Box<FlowCurrentSceneV9>>,
    serialized_sizes: Vec<usize>,
    stored_bytes: usize,
    identity: FlowTimelineIdentity,
}

impl PreparedFlowTimeline {
    fn from_full_frames(frames: Vec<FlowCurrentSceneV9>) -> Result<Self, JsError> {
        let frame_count = u64::try_from(frames.len())
            .map_err(|_| JsError::new("flow timeline frame count overflow"))?;
        let mut hasher = Sha256::new();
        hasher.update(FLOW_TIMELINE_FINGERPRINT_DOMAIN);
        hasher.update(frame_count.to_le_bytes());
        let mut serialized_sizes = Vec::with_capacity(frames.len());
        let mut stored_bytes = 0_usize;
        for frame in &frames {
            let encoded =
                serde_json::to_vec(frame).map_err(|error| JsError::new(&error.to_string()))?;
            let encoded_len = u64::try_from(encoded.len())
                .map_err(|_| JsError::new("flow timeline frame size overflow"))?;
            hasher.update(encoded_len.to_le_bytes());
            hasher.update(&encoded);
            stored_bytes = stored_bytes
                .checked_add(encoded.len())
                .ok_or_else(|| JsError::new("flow timeline size overflowed"))?;
            serialized_sizes.push(encoded.len());
        }
        Ok(Self {
            frames: frames.into_iter().map(Box::new).collect(),
            serialized_sizes,
            stored_bytes,
            identity: FlowTimelineIdentity {
                frame_count: usize::try_from(frame_count)
                    .map_err(|_| JsError::new("flow timeline frame count overflow"))?,
                sha256: hasher.finalize().into(),
            },
        })
    }

    fn from_source_frames(frames: Vec<FlowCurrentSceneV9>) -> Result<Self, JsError> {
        Self::from_full_frames(frames)
    }

    fn len(&self) -> usize {
        self.frames.len()
    }

    fn full_frame(&self, index: usize) -> Option<&FlowCurrentSceneV9> {
        self.frames.get(index).map(Box::as_ref)
    }

    fn materialize(&self, index: usize) -> Result<FlowCurrentSceneV9, JsError> {
        self.frames
            .get(index)
            .map(|scene| scene.as_ref().clone())
            .ok_or_else(|| JsError::new("flow timeline cursor is out of range"))
    }

    const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FlowTimelineIdentity {
    frame_count: usize,
    sha256: [u8; 32],
}

struct FlowTimelineCache {
    identity: FlowTimelineIdentity,
    prepared: PreparedFlowTimeline,
    frames: BTreeMap<usize, FlowCurrentSceneV9>,
    serialized_bytes: usize,
}

impl FlowTimelineCache {
    fn from_prepared_timeline(
        timeline: PreparedFlowTimeline,
        current: usize,
        target: usize,
    ) -> Result<Self, JsError> {
        Self::try_from_prepared_timeline_with_limits(
            timeline,
            current,
            target,
            None,
            MAX_FLOW_FRAME_CACHE_BYTES,
            MAX_FLOW_FRAME_CACHE_FRAMES,
        )
        .map_err(|error| JsError::new(&error))
    }

    fn try_from_prepared_timeline_with_limits(
        timeline: PreparedFlowTimeline,
        current: usize,
        target: usize,
        expected: Option<&FlowTimelineIdentity>,
        max_bytes: usize,
        max_frames: usize,
    ) -> Result<Self, String> {
        if timeline.len() == 0 {
            return Err("flow timeline must contain its base frame".to_owned());
        }
        if current >= timeline.len() || target >= timeline.len() {
            return Err("flow timeline cache cursor is out of range".to_owned());
        }
        if max_bytes == 0 || max_frames == 0 {
            return Err("flow timeline cache budget must be positive".to_owned());
        }

        let identity = timeline.identity.clone();
        if expected.is_some_and(|expected| expected != &identity) {
            return Err(
                "regenerated flow timeline identity does not match the prepared timeline"
                    .to_owned(),
            );
        }

        let required = BTreeSet::from([0, current, target]);
        let mut cached = BTreeMap::new();
        let mut cached_bytes = 0_usize;
        for index in required.iter().copied() {
            if timeline.full_frame(index).is_some() {
                continue;
            }
            let frame_bytes = timeline.serialized_sizes[index];
            let within_count = cached.len() < max_frames;
            let within_bytes = cached_bytes
                .checked_add(frame_bytes)
                .is_some_and(|total| total <= max_bytes);
            if !within_count || !within_bytes {
                return Err(
                    "required flow timeline frame exceeds the rolling cache budget".to_owned(),
                );
            }
            let frame = timeline
                .materialize(index)
                .map_err(|error| format!("sparse flow frame {index}: {error:?}"))?;
            cached_bytes = cached_bytes
                .checked_add(frame_bytes)
                .ok_or_else(|| "flow timeline cache byte count overflow".to_owned())?;
            cached.insert(index, frame);
        }
        if !required
            .iter()
            .all(|index| timeline.full_frame(*index).is_some() || cached.contains_key(index))
        {
            return Err("flow timeline cache did not retain every required frame".to_owned());
        }
        Ok(Self {
            identity,
            prepared: timeline,
            frames: cached,
            serialized_bytes: cached_bytes,
        })
    }

    fn get(&self, cursor: usize) -> Option<&FlowCurrentSceneV9> {
        debug_assert!(self.serialized_bytes <= MAX_FLOW_FRAME_CACHE_BYTES);
        self.prepared
            .full_frame(cursor)
            .or_else(|| self.frames.get(&cursor))
    }

    fn ensure_materialized(&mut self, cursor: usize, current: usize) -> Result<(), JsError> {
        if cursor >= self.identity.frame_count {
            return Err(JsError::new("flow event cursor is out of range"));
        }
        if self.get(cursor).is_some() {
            return Ok(());
        }
        let scene = self.prepared.materialize(cursor)?;
        let scene_bytes = serialized_flow_scene_bytes(&scene)?;
        while self.frames.len() >= MAX_FLOW_FRAME_CACHE_FRAMES
            || self.serialized_bytes.saturating_add(scene_bytes) > MAX_FLOW_FRAME_CACHE_BYTES
        {
            let removable = self
                .frames
                .keys()
                .copied()
                .filter(|index| *index != current && *index != cursor)
                .max_by_key(|index| index.abs_diff(cursor));
            let Some(remove) = removable else {
                return Err(JsError::new(
                    "required flow timeline frame exceeds the rolling cache budget",
                ));
            };
            let removed = self
                .frames
                .remove(&remove)
                .ok_or_else(|| JsError::new("flow timeline cache eviction drifted"))?;
            self.serialized_bytes = self
                .serialized_bytes
                .saturating_sub(serialized_flow_scene_bytes(&removed)?);
        }
        self.serialized_bytes = self
            .serialized_bytes
            .checked_add(scene_bytes)
            .ok_or_else(|| JsError::new("flow timeline cache byte count overflow"))?;
        self.frames.insert(cursor, scene);
        Ok(())
    }

    fn event_count(&self) -> usize {
        self.identity.frame_count.saturating_sub(1)
    }
}

struct StagedFlowNext {
    target: usize,
    replacement_timeline: Option<FlowTimelineCache>,
}

const PREDICTION_ASSISTED_EPSILON_ALGORITHM: &str = "prediction-assisted-epsilon-relaxation";
const TARDOS_FRAMEWORK_ALGORITHM: &str = "tardos-framework";

struct PredictionAssistedEpsilonConfig {
    predicted_prices: Vec<i128>,
    scaling_parameter: u32,
}

fn prediction_assisted_epsilon_config(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
) -> Result<PredictionAssistedEpsilonConfig, String> {
    let config = &scenario.payload.algorithm.config;
    if config.len() != 2
        || !config.contains_key("predicted_potentials")
        || !config.contains_key("scaling_parameter")
    {
        return Err("prediction-assisted epsilon relaxation requires exactly predicted_potentials and scaling_parameter".to_owned());
    }
    let predictions = config["predicted_potentials"]
        .as_object()
        .ok_or_else(|| "predicted_potentials must be an object".to_owned())?;
    if predictions.len() != graph.nodes().len()
        || predictions.keys().any(|node| {
            graph
                .nodes()
                .iter()
                .all(|entry| entry.id().as_str() != node)
        })
    {
        return Err(
            "predicted_potentials must contain every canonical graph node exactly once".to_owned(),
        );
    }
    let predicted_prices = graph
        .nodes()
        .iter()
        .map(|node| {
            let value = predictions
                .get(node.id().as_str())
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "each predicted potential must be an i128 string".to_owned())?;
            value
                .parse::<i128>()
                .ok()
                .filter(|parsed| parsed.to_string() == value)
                .ok_or_else(|| "predicted potentials must be canonical i128 strings".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let scaling_parameter = config["scaling_parameter"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| (2..=4).contains(value))
        .ok_or_else(|| "scaling_parameter must be the integer 2, 3, or 4".to_owned())?;
    Ok(PredictionAssistedEpsilonConfig {
        predicted_prices,
        scaling_parameter,
    })
}

fn tardos_framework_config(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
) -> Result<Vec<i128>, String> {
    let config = &scenario.payload.algorithm.config;
    if config.len() != 1 || !config.contains_key("potentials") {
        return Err("Tardos framework requires exactly potentials".to_owned());
    }
    let potentials = config["potentials"]
        .as_object()
        .ok_or_else(|| "Tardos framework potentials must be an object".to_owned())?;
    if potentials.len() != graph.nodes().len()
        || potentials.keys().any(|node| {
            graph
                .nodes()
                .iter()
                .all(|entry| entry.id().as_str() != node)
        })
    {
        return Err(
            "Tardos framework potentials must contain every canonical graph node exactly once"
                .to_owned(),
        );
    }
    graph
        .nodes()
        .iter()
        .map(|node| {
            let value = potentials
                .get(node.id().as_str())
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "each Tardos potential must be an i128 string".to_owned())?;
            value
                .parse::<i128>()
                .ok()
                .filter(|parsed| parsed.to_string() == value)
                .ok_or_else(|| "Tardos potentials must be canonical i128 strings".to_owned())
        })
        .collect()
}

fn validate_runtime_algorithm(algorithm_id: &str) -> Result<AlgorithmId, &'static str> {
    let algorithm_id = algorithm_id
        .parse::<AlgorithmId>()
        .map_err(|_| "flow algorithm is not present in the catalog")?;
    let descriptor =
        find_algorithm_by_id(algorithm_id).ok_or("flow algorithm is not present in the catalog")?;
    if descriptor.status != ImplementationStatus::Executable {
        return Err("flow algorithm is not executable in this build");
    }
    Ok(algorithm_id)
}

fn admission_product_exceeds_limit(factors: impl IntoIterator<Item = u128>, limit: u64) -> bool {
    let limit = u128::from(limit);
    let mut product = 1_u128;
    for factor in factors {
        let Some(next) = product.checked_mul(factor) else {
            return true;
        };
        product = next;
        if product > limit {
            return true;
        }
    }
    false
}

fn kernel_resource_admission_limited(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    descriptor: &flow::AlgorithmDescriptor,
) -> bool {
    let contract = descriptor.admission_contract;
    let terminal_capacity_exceeds_u64 = match &scenario.payload.model {
        FlowProblemModelV1::MaxFlow { source, sink }
        | FlowProblemModelV1::MinCostMaxFlow { source, sink }
        | FlowProblemModelV1::PlanarMaxFlow {
            source,
            sink,
            embedding: _,
        } => terminal_capacity_bound(graph, source, sink).is_none(),
        _ => false,
    };
    terminal_capacity_exceeds_u64
        || matches!(
            descriptor.runtime_route,
            RuntimeRouteKind::MinCostFlow
                | RuntimeRouteKind::MinCostMaxFlow
                | RuntimeRouteKind::Assignment
                | RuntimeRouteKind::Transportation
                | RuntimeRouteKind::ConvexCostFlow
        ) && objective_magnitude_bound(&scenario.payload.graph.edges).is_none()
        || contract.max_nodes.is_some_and(|limit| {
            usize::try_from(limit)
                .ok()
                .is_none_or(|limit| graph.nodes().len() > limit)
        })
        || contract.max_edges.is_some_and(|limit| {
            usize::try_from(limit)
                .ok()
                .is_none_or(|limit| graph.edges().len() > limit)
        })
        || contract
            .max_capacity
            .is_some_and(|limit| graph.edges().iter().any(|edge| edge.capacity() > limit.0))
        || contract.max_absolute_cost.is_some_and(|limit| {
            graph
                .edges()
                .iter()
                .any(|edge| edge.cost().unsigned_abs() > limit.0)
        })
        || contract.max_assignment_space.is_some_and(|limit| {
            admission_product_exceeds_limit(
                graph
                    .edges()
                    .iter()
                    .map(|edge| u128::from(edge.capacity() - edge.lower()) + 1),
                limit.0,
            )
        })
        || contract.max_capacity_state_space.is_some_and(|limit| {
            admission_product_exceeds_limit(
                graph
                    .edges()
                    .iter()
                    .map(|edge| u128::from(edge.capacity()) + 1),
                limit.0,
            )
        })
}

fn terminal_capacity_bound(graph: &flow::FlowNetwork, source: &str, sink: &str) -> Option<u64> {
    let source = graph.node_index(&flow::NodeId::parse(source).ok()?)?;
    let sink = graph.node_index(&flow::NodeId::parse(sink).ok()?)?;
    let source_capacity = graph
        .edges()
        .iter()
        .filter(|edge| edge.from() == source)
        .try_fold(0_u128, |total, edge| {
            total.checked_add(u128::from(edge.capacity()))
        })?;
    let sink_capacity = graph
        .edges()
        .iter()
        .filter(|edge| edge.to() == sink)
        .try_fold(0_u128, |total, edge| {
            total.checked_add(u128::from(edge.capacity()))
        })?;
    u64::try_from(source_capacity.min(sink_capacity)).ok()
}

fn objective_magnitude_bound(edges: &[flow::FlowEdgeV1]) -> Option<u128> {
    let mut total = 0_u128;
    for edge in edges {
        if let Some(convex) = &edge.convex_cost {
            let base = convex
                .base_cost_at_zero
                .parse::<i128>()
                .ok()?
                .unsigned_abs();
            total = total.checked_add(base)?;
            let mut start = 0_u64;
            for segment in &convex.segments {
                let end = segment.end_flow.parse::<u64>().ok()?;
                let width = end.checked_sub(start)?;
                let slope = segment.marginal_cost.parse::<i64>().ok()?;
                total = total.checked_add(
                    u128::from(width).checked_mul(i128::from(slope).unsigned_abs())?,
                )?;
                start = end;
            }
        } else {
            let capacity = edge.capacity.parse::<u64>().ok()?;
            let cost = edge.cost.parse::<i64>().ok()?;
            total = total
                .checked_add(u128::from(capacity).checked_mul(i128::from(cost).unsigned_abs())?)?;
        }
        if total > i128::MAX.unsigned_abs() {
            return None;
        }
    }
    Some(total)
}

fn scenario_catalog_model(model: &FlowProblemModelV1) -> CatalogModelKind {
    match model {
        FlowProblemModelV1::MaxFlow { .. } => CatalogModelKind::MaxFlow,
        FlowProblemModelV1::ParametricMaxFlow { .. } => CatalogModelKind::ParametricMaxFlow,
        FlowProblemModelV1::FixedFlowMinCost { .. } => CatalogModelKind::FixedFlowMinCost,
        FlowProblemModelV1::MinCostMaxFlow { .. } => CatalogModelKind::MinCostMaxFlow,
        FlowProblemModelV1::Circulation { .. } => CatalogModelKind::Circulation,
        FlowProblemModelV1::Transshipment { .. } => CatalogModelKind::Transshipment,
        FlowProblemModelV1::BipartiteMatching { .. } => CatalogModelKind::BipartiteMatching,
        FlowProblemModelV1::Assignment { .. } => CatalogModelKind::Assignment,
        FlowProblemModelV1::Transportation { .. } => CatalogModelKind::Transportation,
        FlowProblemModelV1::PlanarMaxFlow { .. } => CatalogModelKind::PlanarMaxFlow,
        FlowProblemModelV1::ConvexCostFlow { .. } => CatalogModelKind::ConvexCostFlow,
    }
}

fn validate_model_contract(
    scenario: &FlowScenarioV1,
    descriptor: &flow::AlgorithmDescriptor,
) -> Result<(), &'static str> {
    let model = scenario_catalog_model(&scenario.payload.model);
    if descriptor.models.contains(&model) {
        Ok(())
    } else {
        Err("selected flow algorithm does not support the requested problem model")
    }
}

fn validate_execution_contract(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    algorithm_id: AlgorithmId,
) -> Result<(), String> {
    if scenario.payload.algorithm.id == PREDICTION_ASSISTED_EPSILON_ALGORITHM {
        prediction_assisted_epsilon_config(scenario, graph)?;
    } else if scenario.payload.algorithm.id == TARDOS_FRAMEWORK_ALGORITHM {
        tardos_framework_config(scenario, graph)?;
    } else if !scenario.payload.algorithm.config.is_empty() {
        return Err("Phase-2 flow solvers require an empty algorithm config".to_owned());
    }
    if matches!(scenario.payload.run_profile, RunProfileV1::CpuParallel) {
        return Err("CPU-parallel flow execution is not available in this phase".to_owned());
    }
    validate_declared_initial_flow_contract(scenario, algorithm_id)?;
    Ok(())
}

fn validate_declared_initial_flow_contract(
    scenario: &FlowScenarioV1,
    algorithm_id: AlgorithmId,
) -> Result<(), String> {
    // Only these two kernels consume the persisted per-edge declaration. Every
    // other runner constructs its own source-defined initial state. Requiring
    // an explicit declaration to equal the lower bound prevents the ready
    // frame from displaying state that the first source event would discard.
    let consumes_declared_flow = matches!(
        algorithm_id,
        AlgorithmId::BinaryBlockingFlow | AlgorithmId::WarmStartPushRelabel
    );
    if consumes_declared_flow {
        return Ok(());
    }
    if scenario.payload.graph.edges.iter().any(|edge| {
        edge.initial_flow
            .as_deref()
            .is_some_and(|initial| initial != edge.lower)
    }) {
        return Err(format!(
            "{} constructs its own initial state and does not consume a non-lower initial_flow declaration",
            algorithm_id.as_str()
        ));
    }
    Ok(())
}

/// A model and runtime runner pair that has already been proven executable.
///
/// Construction is exhaustive over both closed enums, so adding either a
/// problem model or a runtime route requires an explicit decision here instead
/// of falling through to a runtime-only dispatch error.
#[derive(Clone, Copy)]
enum FlowDispatch<'a> {
    ParametricMaxFlow(ParametricMaxFlowRunner),
    MaxFlow {
        source: &'a str,
        sink: &'a str,
        runner: MaxFlowRunner,
    },
    FixedFlowMinCost {
        source: &'a str,
        sink: &'a str,
        required_flow: &'a str,
        runner: MinCostFlowRunner,
    },
    BalanceMinCost(MinCostFlowRunner),
    MinCostMaxFlow {
        source: &'a str,
        sink: &'a str,
    },
    BipartiteMatching {
        left: &'a [String],
        right: &'a [String],
        adapter: Option<(&'a str, &'a str)>,
    },
    Assignment {
        agents: &'a [String],
        tasks: &'a [String],
        objective: flow::AssignmentObjectiveV1,
        runner: AssignmentRunner,
    },
    Transportation {
        origins: &'a [String],
        destinations: &'a [String],
        runner: TransportationRunner,
    },
    PlanarMaxFlow {
        source: &'a str,
        sink: &'a str,
        embedding: &'a flow::FlowPlanarEmbeddingV1,
        runner: PlanarMaxFlowRunner,
    },
    ConvexCostFlow(ConvexCostFlowRunner),
}

impl<'a> FlowDispatch<'a> {
    #[allow(
        clippy::too_many_lines,
        reason = "the closed model-runner product is intentionally compiler-exhaustive"
    )]
    fn try_new(model: &'a FlowProblemModelV1, runner: RuntimeRunner) -> Result<Self, &'static str> {
        match model {
            FlowProblemModelV1::ParametricMaxFlow { .. } => match runner {
                RuntimeRunner::ParametricMaxFlow(runner) => Ok(Self::ParametricMaxFlow(runner)),
                RuntimeRunner::MaxFlow(_)
                | RuntimeRunner::MinCostFlow(_)
                | RuntimeRunner::MinCostMaxFlow
                | RuntimeRunner::BipartiteMatching
                | RuntimeRunner::Assignment(_)
                | RuntimeRunner::Transportation(_)
                | RuntimeRunner::PlanarMaxFlow(_)
                | RuntimeRunner::ConvexCostFlow(_) => incompatible_flow_dispatch(),
            },
            FlowProblemModelV1::MaxFlow { source, sink } => match runner {
                RuntimeRunner::MaxFlow(runner) => Ok(Self::MaxFlow {
                    source,
                    sink,
                    runner,
                }),
                RuntimeRunner::MinCostFlow(_)
                | RuntimeRunner::MinCostMaxFlow
                | RuntimeRunner::ParametricMaxFlow(_)
                | RuntimeRunner::BipartiteMatching
                | RuntimeRunner::Assignment(_)
                | RuntimeRunner::Transportation(_)
                | RuntimeRunner::PlanarMaxFlow(_)
                | RuntimeRunner::ConvexCostFlow(_) => incompatible_flow_dispatch(),
            },
            FlowProblemModelV1::FixedFlowMinCost {
                source,
                sink,
                required_flow,
            } => match runner {
                RuntimeRunner::MinCostFlow(runner) => Ok(Self::FixedFlowMinCost {
                    source,
                    sink,
                    required_flow,
                    runner,
                }),
                RuntimeRunner::MaxFlow(_)
                | RuntimeRunner::MinCostMaxFlow
                | RuntimeRunner::ParametricMaxFlow(_)
                | RuntimeRunner::BipartiteMatching
                | RuntimeRunner::Assignment(_)
                | RuntimeRunner::Transportation(_)
                | RuntimeRunner::PlanarMaxFlow(_)
                | RuntimeRunner::ConvexCostFlow(_) => incompatible_flow_dispatch(),
            },
            FlowProblemModelV1::Circulation {} | FlowProblemModelV1::Transshipment {} => {
                match runner {
                    RuntimeRunner::MinCostFlow(runner) => Ok(Self::BalanceMinCost(runner)),
                    RuntimeRunner::MaxFlow(_)
                    | RuntimeRunner::MinCostMaxFlow
                    | RuntimeRunner::ParametricMaxFlow(_)
                    | RuntimeRunner::BipartiteMatching
                    | RuntimeRunner::Assignment(_)
                    | RuntimeRunner::Transportation(_)
                    | RuntimeRunner::PlanarMaxFlow(_)
                    | RuntimeRunner::ConvexCostFlow(_) => incompatible_flow_dispatch(),
                }
            }
            FlowProblemModelV1::MinCostMaxFlow { source, sink } => match runner {
                RuntimeRunner::MinCostMaxFlow => Ok(Self::MinCostMaxFlow { source, sink }),
                RuntimeRunner::MaxFlow(_)
                | RuntimeRunner::MinCostFlow(_)
                | RuntimeRunner::ParametricMaxFlow(_)
                | RuntimeRunner::BipartiteMatching
                | RuntimeRunner::Assignment(_)
                | RuntimeRunner::Transportation(_)
                | RuntimeRunner::PlanarMaxFlow(_)
                | RuntimeRunner::ConvexCostFlow(_) => incompatible_flow_dispatch(),
            },
            FlowProblemModelV1::BipartiteMatching {
                left,
                right,
                flow_adapter,
            } => match runner {
                RuntimeRunner::BipartiteMatching => Ok(Self::BipartiteMatching {
                    left,
                    right,
                    adapter: flow_adapter
                        .as_ref()
                        .map(|adapter| (adapter.source.as_str(), adapter.sink.as_str())),
                }),
                RuntimeRunner::MaxFlow(_)
                | RuntimeRunner::MinCostFlow(_)
                | RuntimeRunner::MinCostMaxFlow
                | RuntimeRunner::ParametricMaxFlow(_)
                | RuntimeRunner::Assignment(_)
                | RuntimeRunner::Transportation(_)
                | RuntimeRunner::PlanarMaxFlow(_)
                | RuntimeRunner::ConvexCostFlow(_) => incompatible_flow_dispatch(),
            },
            FlowProblemModelV1::Assignment {
                agents,
                tasks,
                objective,
            } => match runner {
                RuntimeRunner::Assignment(runner) => Ok(Self::Assignment {
                    agents,
                    tasks,
                    objective: *objective,
                    runner,
                }),
                RuntimeRunner::MaxFlow(_)
                | RuntimeRunner::MinCostFlow(_)
                | RuntimeRunner::MinCostMaxFlow
                | RuntimeRunner::ParametricMaxFlow(_)
                | RuntimeRunner::BipartiteMatching
                | RuntimeRunner::Transportation(_)
                | RuntimeRunner::PlanarMaxFlow(_)
                | RuntimeRunner::ConvexCostFlow(_) => incompatible_flow_dispatch(),
            },
            FlowProblemModelV1::Transportation {
                origins,
                destinations,
            } => match runner {
                RuntimeRunner::Transportation(runner) => Ok(Self::Transportation {
                    origins,
                    destinations,
                    runner,
                }),
                RuntimeRunner::MaxFlow(_)
                | RuntimeRunner::MinCostFlow(_)
                | RuntimeRunner::MinCostMaxFlow
                | RuntimeRunner::ParametricMaxFlow(_)
                | RuntimeRunner::BipartiteMatching
                | RuntimeRunner::Assignment(_)
                | RuntimeRunner::PlanarMaxFlow(_)
                | RuntimeRunner::ConvexCostFlow(_) => incompatible_flow_dispatch(),
            },
            FlowProblemModelV1::PlanarMaxFlow {
                source,
                sink,
                embedding,
            } => match runner {
                RuntimeRunner::PlanarMaxFlow(runner) => Ok(Self::PlanarMaxFlow {
                    source,
                    sink,
                    embedding,
                    runner,
                }),
                RuntimeRunner::MaxFlow(_)
                | RuntimeRunner::MinCostFlow(_)
                | RuntimeRunner::MinCostMaxFlow
                | RuntimeRunner::ParametricMaxFlow(_)
                | RuntimeRunner::BipartiteMatching
                | RuntimeRunner::Assignment(_)
                | RuntimeRunner::Transportation(_)
                | RuntimeRunner::ConvexCostFlow(_) => incompatible_flow_dispatch(),
            },
            FlowProblemModelV1::ConvexCostFlow {} => match runner {
                RuntimeRunner::ConvexCostFlow(runner) => Ok(Self::ConvexCostFlow(runner)),
                RuntimeRunner::MaxFlow(_)
                | RuntimeRunner::MinCostFlow(_)
                | RuntimeRunner::MinCostMaxFlow
                | RuntimeRunner::ParametricMaxFlow(_)
                | RuntimeRunner::BipartiteMatching
                | RuntimeRunner::Assignment(_)
                | RuntimeRunner::Transportation(_)
                | RuntimeRunner::PlanarMaxFlow(_) => incompatible_flow_dispatch(),
            },
        }
    }
}

fn incompatible_flow_dispatch<T>() -> Result<T, &'static str> {
    Err("selected flow algorithm and problem model are not executable together")
}

/// Flow-plugin session with isolated event candidates and reversible committed history.
struct FlowSession {
    scenario: FlowScenarioV1,
    algorithm_id: AlgorithmId,
    resource_admission_limited: bool,
    frames: Vec<FlowCurrentSceneV9>,
    timeline: Option<FlowTimelineCache>,
    prepared: bool,
    cursor: usize,
    committed_end: usize,
    staged_next: Option<StagedFlowNext>,
    staged_seek: Option<usize>,
}

#[derive(Debug)]
struct ValidatedFlowSessionInput {
    scenario: FlowScenarioV1,
    algorithm_id: AlgorithmId,
    graph: flow::FlowNetwork,
    resource_admission_limited: bool,
}

fn validate_flow_session_input(scenario_json: &str) -> Result<ValidatedFlowSessionInput, String> {
    let scenario =
        decode_flow_scenario(scenario_json.as_bytes()).map_err(|error| error.to_string())?;
    let algorithm_id =
        validate_runtime_algorithm(&scenario.payload.algorithm.id).map_err(str::to_owned)?;
    let graph = scenario
        .canonical_network()
        .map_err(|error| error.to_string())?;
    let descriptor = find_algorithm_by_id(algorithm_id)
        .ok_or_else(|| "flow algorithm is not present in the catalog".to_owned())?;
    validate_model_contract(&scenario, descriptor).map_err(str::to_owned)?;
    validate_execution_contract(&scenario, &graph, algorithm_id)?;
    validate_catalog_graph_contract(&scenario, descriptor, &graph).map_err(str::to_owned)?;
    let maximum_nodes = usize::try_from(descriptor.initial_band.max_nodes)
        .map_err(|_| "catalog node admission limit does not fit usize".to_owned())?;
    let maximum_edges = usize::try_from(descriptor.initial_band.max_edges)
        .map_err(|_| "catalog edge admission limit does not fit usize".to_owned())?;
    let catalog_admission_limited =
        graph.nodes().len() > maximum_nodes || graph.edges().len() > maximum_edges;
    let resource_admission_limited = catalog_admission_limited
        || kernel_resource_admission_limited(&scenario, &graph, descriptor);
    Ok(ValidatedFlowSessionInput {
        scenario,
        algorithm_id,
        graph,
        resource_admission_limited,
    })
}

impl FlowSession {
    fn new(scenario_json: &str) -> Result<Self, JsError> {
        let validated =
            validate_flow_session_input(scenario_json).map_err(|error| JsError::new(&error))?;
        let ValidatedFlowSessionInput {
            scenario,
            algorithm_id,
            graph,
            resource_admission_limited,
        } = validated;
        let ready = if resource_admission_limited {
            ready_plain_flow_scene(&scenario, &graph)?
        } else {
            ready_flow_scene(&scenario)?
        };
        Ok(Self {
            scenario,
            algorithm_id,
            resource_admission_limited,
            frames: vec![ready],
            timeline: None,
            prepared: false,
            cursor: 0,
            committed_end: 0,
            staged_next: None,
            staged_seek: None,
        })
    }

    fn current_frame_json(&self) -> Result<String, JsError> {
        self.frame_json_at(self.cursor)
    }

    fn frame_json_at(&self, cursor: usize) -> Result<String, JsError> {
        if cursor > self.committed_end {
            return Err(JsError::new("flow event cursor is not committed"));
        }
        self.cached_frame(cursor)
            .ok_or_else(|| JsError::new("flow event cursor is out of range"))
            .and_then(serialize_frame)
    }

    fn cached_frame(&self, cursor: usize) -> Option<&FlowCurrentSceneV9> {
        if cursor == 0 {
            return self.frames.first();
        }
        self.timeline
            .as_ref()
            .and_then(|timeline| timeline.get(cursor))
    }

    fn prepared_event_count(&self) -> usize {
        self.timeline
            .as_ref()
            .map_or(0, FlowTimelineCache::event_count)
    }

    fn ensure_frame_cached(&mut self, target: usize) -> Result<(), JsError> {
        let Some(timeline) = self.timeline.as_mut() else {
            return Err(JsError::new("flow timeline has not been prepared"));
        };
        if target > timeline.event_count() {
            return Err(JsError::new("flow event cursor is out of range"));
        }
        timeline.ensure_materialized(target, self.cursor)
    }

    #[cfg(test)]
    fn prepare_frames(&self) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let timeline = self.prepare_timeline()?;
        (0..timeline.len())
            .map(|index| timeline.materialize(index))
            .collect()
    }

    fn prepare_timeline(&self) -> Result<PreparedFlowTimeline, JsError> {
        let (prepared, captured_feasibility, captured_feasibility_metrics) =
            if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
                let (prepared, captured) =
                    flow::feasibility::capture_feasibility_traces(|feasibility| {
                        self.prepare_source_frames(feasibility)
                    });
                (
                    prepared,
                    captured,
                    flow::feasibility::FeasibilityMetricSummary::default(),
                )
            } else {
                let (prepared, metrics) =
                    flow::feasibility::capture_feasibility_metrics(|feasibility| {
                        self.prepare_source_frames(feasibility)
                    });
                (prepared, Vec::new(), metrics)
            };
        let frames = prepared?;
        let graph = self
            .scenario
            .canonical_network()
            .map_err(|error| JsError::new(&error.to_string()))?;
        let frames = compose_captured_feasibility_traces(
            &self.frames[0],
            &graph,
            frames,
            captured_feasibility,
            captured_feasibility_metrics,
        )?;
        let source_base = frames
            .first()
            .ok_or_else(|| JsError::new("flow execution produced no source base frame"))?;
        if let Err(error) = validate_public_ready_source_base(&self.frames[0], source_base) {
            #[cfg(test)]
            eprintln!(
                "FLOW_READY_BASE_DIVERGENCE\t{}\t{}",
                self.algorithm_id.as_str(),
                frames
                    .get(1)
                    .and_then(|frame| frame.trace_event.as_ref())
                    .map_or("<no-source-event>", |event| event.catalog_id.as_str())
            );
            return Err(JsError::new(error));
        }
        normalize_prepared_flow_timeline_sparse_with_limit(
            &self.scenario,
            frames,
            MAX_EAGER_FLOW_TIMELINE_BYTES,
        )
    }

    fn prepare_source_frames(
        &self,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        if self.resource_admission_limited {
            return Ok(self.catalog_admission_frames());
        }
        let graph = self
            .scenario
            .canonical_network()
            .map_err(|error| JsError::new(&error.to_string()))?;
        let algorithm = self.algorithm_id;
        let descriptor = find_algorithm_by_id(algorithm)
            .ok_or_else(|| JsError::new("flow algorithm is not present in the catalog"))?;
        let runner = RuntimeRunner::for_algorithm(algorithm);
        if runner.route() != descriptor.runtime_route {
            return Err(JsError::new(
                "flow catalog route disagrees with the closed runtime registry",
            ));
        }
        self.prepare_frames_for_runner(&graph, runner, feasibility)
    }

    fn catalog_admission_frames(&self) -> Vec<FlowCurrentSceneV9> {
        let mut limited = self.frames[0].clone();
        limited.apply_resource_limit_with_reason(FlowResourceLimitReasonV1::InputAdmission);
        vec![self.frames[0].clone(), limited]
    }

    fn prepare_frames_for_runner(
        &self,
        graph: &flow::FlowNetwork,
        runner: RuntimeRunner,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let dispatch =
            FlowDispatch::try_new(&self.scenario.payload.model, runner).map_err(JsError::new)?;
        match dispatch {
            FlowDispatch::ParametricMaxFlow(runner) => {
                self.prepare_parametric_max_flow(graph, runner, feasibility)
            }
            FlowDispatch::MaxFlow {
                source,
                sink,
                runner,
            } => {
                let (source, sink) = terminal_indices(graph, source, sink)?;
                self.prepare_max_flow(graph, source, sink, runner, feasibility)
            }
            FlowDispatch::FixedFlowMinCost {
                source,
                sink,
                required_flow,
                runner,
            } => {
                let (source, sink) = terminal_indices(graph, source, sink)?;
                let required_flow = required_flow
                    .parse::<u64>()
                    .map_err(|_| JsError::new("required flow is not a canonical u64"))?;
                let target = fixed_flow_divergences(graph, source, sink, required_flow)
                    .map_err(|error| JsError::new(&error.to_string()))?;
                self.prepare_min_cost_flow(graph, &target, runner, feasibility)
            }
            FlowDispatch::BalanceMinCost(runner) => {
                let target =
                    supply_divergences(graph).map_err(|error| JsError::new(&error.to_string()))?;
                self.prepare_min_cost_flow(graph, &target, runner, feasibility)
            }
            FlowDispatch::MinCostMaxFlow { source, sink } => {
                let (source, sink) = terminal_indices(graph, source, sink)?;
                self.prepare_successive_shortest_augmenting_path(graph, source, sink)
            }
            FlowDispatch::BipartiteMatching {
                left,
                right,
                adapter,
            } => self.prepare_hopcroft_karp(graph, left, right, adapter),
            FlowDispatch::Assignment {
                agents,
                tasks,
                objective,
                runner,
            } => match runner {
                AssignmentRunner::Hungarian => {
                    self.prepare_hungarian(graph, agents, tasks, objective)
                }
                AssignmentRunner::Auction => self.prepare_auction(graph, agents, tasks, objective),
            },
            FlowDispatch::Transportation {
                origins,
                destinations,
                runner,
            } => self.prepare_transportation(graph, origins, destinations, runner, feasibility),
            FlowDispatch::PlanarMaxFlow {
                source,
                sink,
                embedding,
                runner,
            } => {
                let (source, sink) = terminal_indices(graph, source, sink)?;
                match runner {
                    PlanarMaxFlowRunner::Hassin => {
                        self.prepare_hassin_st_planar(graph, source, sink, embedding)
                    }
                    PlanarMaxFlowRunner::BorradaileKlein => {
                        self.prepare_borradaile_klein_planar(graph, source, sink, embedding)
                    }
                }
            }
            FlowDispatch::ConvexCostFlow(runner) => {
                self.prepare_convex_cost_flow(graph, runner, feasibility)
            }
        }
    }

    fn prepare_parametric_max_flow(
        &self,
        graph: &flow::FlowNetwork,
        runner: ParametricMaxFlowRunner,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let problem = self
            .scenario
            .parametric_problem(graph)
            .map_err(|error| JsError::new(&error.to_string()))?;
        match (runner, self.scenario.payload.run_profile) {
            (ParametricMaxFlowRunner::Pseudoflow, RunProfileV1::Trace) => {
                let run = trace_parametric_pseudoflow(graph, &problem)
                    .map_err(|error| JsError::new(&error.to_string()))?;
                parametric_pseudoflow_trace_frames(&self.scenario, graph, &problem, &run)
            }
            (ParametricMaxFlowRunner::Pseudoflow, RunProfileV1::Fast) => {
                let result = solve_parametric_pseudoflow(graph, &problem)
                    .map_err(|error| JsError::new(&error.to_string()))?;
                parametric_fast_frames(
                    &self.scenario,
                    graph,
                    &problem,
                    &result.segments,
                    &result.breakpoints,
                    canonical_parametric_metrics(result.metrics),
                )
            }
            (ParametricMaxFlowRunner::BreakpointRerun, RunProfileV1::Trace) => {
                let run = flow::trace_parametric_breakpoint_rerun_with_feasibility(
                    graph,
                    &problem,
                    feasibility,
                )
                .map_err(|error| JsError::new(&error.to_string()))?;
                parametric_rerun_trace_frames(&self.scenario, graph, &problem, &run)
            }
            (ParametricMaxFlowRunner::BreakpointRerun, RunProfileV1::Fast) => {
                let result = flow::solve_parametric_breakpoint_rerun_with_feasibility(
                    graph,
                    &problem,
                    feasibility,
                )
                .map_err(|error| JsError::new(&error.to_string()))?;
                parametric_fast_frames(
                    &self.scenario,
                    graph,
                    &problem,
                    &result.segments,
                    &result.breakpoints,
                    rerun_parametric_metrics(result.metrics),
                )
            }
            (
                ParametricMaxFlowRunner::Pseudoflow | ParametricMaxFlowRunner::BreakpointRerun,
                RunProfileV1::CpuParallel,
            ) => Err(JsError::new(
                "parametric max-flow supports only trace and fast profiles",
            )),
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the typed runner match is intentionally exhaustive at the WASM projection boundary"
    )]
    fn prepare_max_flow(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        runner: MaxFlowRunner,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
            return self.prepare_max_flow_trace(graph, source, sink, runner, feasibility);
        }
        let mut scene = ready_flow_scene(&self.scenario)?;
        match runner {
            MaxFlowRunner::FordFulkerson(preset) => {
                match solve_ford_fulkerson_preset(preset, graph, source, sink, feasibility) {
                    Ok(result) => apply_ford_fulkerson_result(&mut scene, graph, &result)?,
                    Err(error) => {
                        return self.ford_fulkerson_error_frames(graph, source, sink, error);
                    }
                }
            }
            MaxFlowRunner::EdmondsKarp => {
                match flow::solve_edmonds_karp_with_feasibility(graph, source, sink, feasibility) {
                    Ok(result) => scene
                        .apply_max_flow_result(
                            graph,
                            &result.flows,
                            &result.certificate,
                            result.metrics.bfs_runs,
                            result.metrics.residual_arc_scans,
                            result.metrics.augmentations,
                        )
                        .map_err(|error| JsError::new(&error.to_string()))?,
                    Err(error) => return self.max_flow_error_frames(graph, source, sink, error),
                }
            }
            MaxFlowRunner::Sap(preset) => {
                match solve_sap_preset(preset, graph, source, sink, feasibility) {
                    Ok(result) => apply_sap_result(&mut scene, graph, &result)?,
                    Err(error) => return self.sap_error_frames(graph, source, sink, error),
                }
            }
            MaxFlowRunner::Dinic(preset) => {
                match solve_dinic_preset(preset, graph, source, sink, feasibility) {
                    Ok(result) => apply_dinic_result(&mut scene, graph, &result)?,
                    Err(error) => return self.dinic_error_frames(graph, source, sink, error),
                }
            }
            MaxFlowRunner::BlockingPreflow(preset) => {
                match solve_blocking_preflow_preset(preset, graph, source, sink, feasibility) {
                    Ok(result) => apply_blocking_preflow_result(&mut scene, graph, &result)?,
                    Err(error) => {
                        return self.blocking_preflow_error_frames(graph, source, sink, error);
                    }
                }
            }
            MaxFlowRunner::Pseudoflow(preset) => {
                return self.prepare_pseudoflow_fast(graph, source, sink, preset, feasibility);
            }
            MaxFlowRunner::BoykovKolmogorov => match solve_boykov_kolmogorov(graph, source, sink) {
                Ok(result) => apply_boykov_kolmogorov_result(&mut scene, graph, &result)?,
                Err(error) => return self.boykov_kolmogorov_error_frames(error),
            },
            MaxFlowRunner::Ibfs => match solve_ibfs(graph, source, sink) {
                Ok(result) => apply_ibfs_result(&mut scene, graph, &result)?,
                Err(error) => return self.ibfs_error_frames(error),
            },
            MaxFlowRunner::Eibfs => match solve_eibfs(graph, source, sink) {
                Ok(result) => apply_eibfs_result(&mut scene, graph, &result)?,
                Err(error) => return self.eibfs_error_frames(error),
            },
            MaxFlowRunner::DynamicEibfs => {
                return self.prepare_dynamic_eibfs_fast(graph, source, sink);
            }
            MaxFlowRunner::DynamicTreeBlocking => {
                return self.prepare_dynamic_tree_blocking_fast(graph, source, sink, feasibility);
            }
            MaxFlowRunner::DynamicTreePushRelabel => {
                return self.prepare_dynamic_tree_push_relabel_fast(
                    graph,
                    source,
                    sink,
                    feasibility,
                );
            }
            MaxFlowRunner::SynchronousPushRelabel => {
                return self.prepare_synchronous_push_relabel_fast(graph, source, sink);
            }
            MaxFlowRunner::WarmStartPushRelabel => {
                return self.prepare_warm_start_push_relabel_fast(graph, source, sink);
            }
            MaxFlowRunner::BinaryBlocking => {
                return self.prepare_binary_blocking_fast(graph, source, sink);
            }
            MaxFlowRunner::GoldbergRao => {
                return self.prepare_goldberg_rao_fast(graph, source, sink);
            }
            MaxFlowRunner::Orlin => {
                return self.prepare_orlin_max_flow_fast(graph, source, sink);
            }
            MaxFlowRunner::ElectricalFlow => {
                return self.prepare_electrical_flow_fast(graph, source, sink);
            }
            MaxFlowRunner::AugmentingElectricalFlow => {
                return self.prepare_augmenting_electrical_fast(graph, source, sink);
            }
            MaxFlowRunner::InteriorPoint => {
                return self.prepare_interior_point_max_flow_fast(graph, source, sink);
            }
            MaxFlowRunner::MinimumRatioCycle => {
                return self.prepare_minimum_ratio_cycle_fast(graph);
            }
            MaxFlowRunner::WeightedAugmentingPaths => {
                return self.prepare_weighted_augmenting_paths_fast(graph, source, sink);
            }
            MaxFlowRunner::WeightedPushRelabel => {
                return self.prepare_weighted_push_relabel_shortcut_fast(graph, source, sink);
            }
            MaxFlowRunner::RandomizedAlmostLinear => {
                return self.prepare_randomized_almost_linear_fast(graph, source, sink);
            }
            MaxFlowRunner::DeterministicAlmostLinear => {
                return self.prepare_deterministic_almost_linear_fast(graph, source, sink);
            }
            MaxFlowRunner::DistanceDirected(preset) => {
                return self.prepare_distance_directed_fast(graph, source, sink, preset);
            }
            MaxFlowRunner::PushRelabel(preset) => {
                return self.prepare_push_relabel_fast(graph, source, sink, preset, feasibility);
            }
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_push_relabel_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        preset: PushRelabelRunner,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_push_relabel_preset(preset, graph, source, sink, feasibility) {
            Ok(result) => apply_push_relabel_result(&mut scene, graph, &result)?,
            Err(error) => return self.push_relabel_error_frames(graph, source, sink, error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_synchronous_push_relabel_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_synchronous_parallel_push_relabel(graph, source, sink) {
            Ok(result) => apply_synchronous_push_relabel_result(&mut scene, graph, &result)?,
            Err(error) => return self.synchronous_push_relabel_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_warm_start_push_relabel_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        let predicted = scenario_initial_flows(&self.scenario, graph)?;
        match solve_warm_start_push_relabel(graph, source, sink, &predicted) {
            Ok(result) => apply_warm_start_result(&mut scene, graph, &result)?,
            Err(error) => return self.warm_start_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_dynamic_tree_blocking_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match flow::solve_dynamic_tree_blocking_flow_with_feasibility(
            graph,
            source,
            sink,
            feasibility,
        ) {
            Ok(result) => apply_dynamic_tree_blocking_result(&mut scene, graph, &result)?,
            Err(error) => {
                return self.dynamic_tree_blocking_error_frames(graph, source, sink, error);
            }
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_dynamic_tree_push_relabel_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match flow::solve_dynamic_tree_push_relabel_with_feasibility(
            graph,
            source,
            sink,
            feasibility,
        ) {
            Ok(result) => apply_dynamic_tree_push_relabel_result(&mut scene, graph, &result)?,
            Err(error) => {
                return self.dynamic_tree_push_relabel_error_frames(graph, source, sink, error);
            }
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_goldberg_rao_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_goldberg_rao(graph, source, sink) {
            Ok(result) => apply_goldberg_rao_result(&mut scene, graph, &result)?,
            Err(error) => return self.goldberg_rao_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_orlin_max_flow_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_orlin_max_flow(graph, source, sink) {
            Ok(result) => apply_orlin_max_flow_result(&mut scene, graph, &result)?,
            Err(error) => return self.orlin_max_flow_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_electrical_flow_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_electrical_flow(graph, source, sink) {
            Ok(result) => apply_electrical_flow_result(&mut scene, graph, source, sink, &result)?,
            Err(error) => return self.electrical_flow_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_augmenting_electrical_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_augmenting_electrical_flow(graph, source, sink) {
            Ok(result) => {
                apply_augmenting_electrical_result(&mut scene, graph, source, sink, &result)?;
            }
            Err(error) => return self.augmenting_electrical_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_interior_point_max_flow_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_interior_point_max_flow(graph, source, sink) {
            Ok(result) => {
                apply_interior_point_max_flow_result(&mut scene, graph, source, sink, &result)?;
            }
            Err(error) => return self.interior_point_max_flow_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_minimum_ratio_cycle_fast(
        &self,
        graph: &flow::FlowNetwork,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_minimum_ratio_cycle(graph) {
            Ok(result) => apply_minimum_ratio_cycle_result(&mut scene, graph, &result)?,
            Err(error) => return self.minimum_ratio_cycle_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_weighted_augmenting_paths_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_weighted_augmenting_paths(graph, source, sink) {
            Ok(result) => {
                apply_weighted_augmenting_paths_result(&mut scene, graph, source, sink, &result)?;
            }
            Err(error) => return self.weighted_augmenting_paths_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_weighted_push_relabel_shortcut_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_weighted_push_relabel_shortcut(graph, source, sink) {
            Ok(result) => apply_weighted_push_relabel_shortcut_result(
                &mut scene, graph, source, sink, &result,
            )?,
            Err(error) => return self.weighted_push_relabel_shortcut_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_randomized_almost_linear_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_randomized_almost_linear_max_flow(graph, source, sink) {
            Ok(result) => {
                apply_randomized_almost_linear_result(&mut scene, graph, source, sink, &result)?;
            }
            Err(error) => return self.randomized_almost_linear_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_deterministic_almost_linear_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_deterministic_almost_linear_max_flow(graph, source, sink) {
            Ok(result) => {
                apply_deterministic_almost_linear_result(&mut scene, graph, source, sink, &result)?;
            }
            Err(error) => return self.deterministic_almost_linear_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_binary_blocking_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let initial_flows = scenario_initial_flows(&self.scenario, graph)?;
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_binary_blocking_first_step(graph, source, sink, &initial_flows) {
            Ok(result) => scene
                .apply_binary_blocking_result(graph, &result)
                .map_err(|error| JsError::new(&error.to_string()))?,
            Err(error) => return self.goldberg_rao_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_distance_directed_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        preset: DistanceDirectedRunner,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_distance_directed_preset(preset, graph, source, sink) {
            Ok(result) => apply_distance_directed_result(&mut scene, graph, &result)?,
            Err(error) => return self.distance_directed_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_hopcroft_karp(
        &self,
        graph: &flow::FlowNetwork,
        left: &[String],
        right: &[String],
        adapter: Option<(&str, &str)>,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
            return match trace_hopcroft_karp(graph, left, right, adapter) {
                Ok(run) => hopcroft_karp_trace_frames(&self.scenario, graph, &run),
                Err(error) => self.hopcroft_karp_error_frames(error),
            };
        }
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_hopcroft_karp(graph, left, right, adapter) {
            Ok(result) => scene
                .apply_bipartite_matching_result(
                    graph,
                    &result.flows,
                    &result.certificate,
                    FlowBipartiteMatchingMetrics {
                        bfs_runs: result.metrics.bfs_runs,
                        edge_scans: result.metrics.edge_scans,
                        phases: result.metrics.phases,
                        augmentations: result.metrics.augmentations,
                        dfs_roots: result.metrics.dfs_roots,
                    },
                )
                .map_err(|error| JsError::new(&error.to_string()))?,
            Err(error) => return self.hopcroft_karp_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn hopcroft_karp_error_frames(
        &self,
        error: HopcroftKarpError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            HopcroftKarpError::AdmissionLimit | HopcroftKarpError::WorkLimit => {
                scene.apply_resource_limit();
            }
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_hungarian(
        &self,
        graph: &flow::FlowNetwork,
        agents: &[String],
        tasks: &[String],
        objective: flow::AssignmentObjectiveV1,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
            return match trace_hungarian(graph, agents, tasks, objective) {
                Ok(run) => hungarian_trace_frames(&self.scenario, graph, &run),
                Err(error) => self.hungarian_error_frames(error),
            };
        }
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_hungarian(graph, agents, tasks, objective) {
            Ok(result) => apply_hungarian_result(&mut scene, graph, &result)?,
            Err(error) => return self.hungarian_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn hungarian_error_frames(
        &self,
        error: HungarianError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            HungarianError::AdmissionLimit | HungarianError::WorkLimit => {
                scene.apply_resource_limit();
            }
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_auction(
        &self,
        graph: &flow::FlowNetwork,
        agents: &[String],
        tasks: &[String],
        objective: flow::AssignmentObjectiveV1,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
            return match trace_auction(graph, agents, tasks, objective) {
                Ok(run) => auction_trace_frames(&self.scenario, graph, &run),
                Err(error) => self.auction_error_frames(error),
            };
        }
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_auction(graph, agents, tasks, objective) {
            Ok(result) => apply_auction_result(&mut scene, graph, &result)?,
            Err(error) => return self.auction_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn auction_error_frames(
        &self,
        error: AuctionError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            AuctionError::AdmissionLimit | AuctionError::WorkLimit => {
                scene.apply_resource_limit();
            }
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_transportation(
        &self,
        graph: &flow::FlowNetwork,
        origins: &[String],
        destinations: &[String],
        runner: TransportationRunner,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let modi = matches!(runner, TransportationRunner::Modi);
        if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
            let preset = if modi {
                flow::TransportationPreset::Modi
            } else {
                flow::TransportationPreset::TransportationSimplex
            };
            let run = flow::trace_transportation_preset_with_feasibility(
                graph,
                origins,
                destinations,
                preset,
                feasibility,
            );
            return match run {
                Ok(run) => min_cost_trace_frames(
                    &self.scenario,
                    graph,
                    &run.result.certificate,
                    &run.base_snapshot,
                    &run.events,
                    &run.final_snapshot,
                ),
                Err(error) => self.transportation_error_frames(graph, origins, destinations, error),
            };
        }
        let mut scene = ready_flow_scene(&self.scenario)?;
        let preset = if modi {
            flow::TransportationPreset::Modi
        } else {
            flow::TransportationPreset::TransportationSimplex
        };
        let result = flow::solve_transportation_preset_with_feasibility(
            graph,
            origins,
            destinations,
            preset,
            feasibility,
        );
        match result {
            Ok(result) => apply_transportation_result(&mut scene, graph, &result)?,
            Err(error) => {
                return self.transportation_error_frames(graph, origins, destinations, error);
            }
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn transportation_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        origins: &[String],
        destinations: &[String],
        error: TransportationError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            TransportationError::Feasibility(FeasibilityError::Infeasible(witness)) => {
                check_transportation_infeasibility(graph, origins, destinations, &witness)
                    .map_err(|error| JsError::new(&error.to_string()))?;
                scene.apply_infeasibility(&witness);
            }
            TransportationError::AdmissionLimit | TransportationError::WorkLimit => {
                scene.apply_resource_limit();
            }
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_hassin_st_planar(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        embedding: &flow::FlowPlanarEmbeddingV1,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
            return match trace_hassin_st_planar(graph, source, sink, embedding) {
                Ok(run) => max_flow_trace_frames_from_parts(
                    &self.scenario,
                    graph,
                    &run.result.certificate,
                    &run.base_snapshot,
                    &run.events,
                    &run.final_snapshot,
                ),
                Err(error) => self.hassin_error_frames(error),
            };
        }
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_hassin_st_planar(graph, source, sink, embedding) {
            Ok(result) => scene
                .apply_planar_max_flow_result(
                    graph,
                    &result.flows,
                    &result.certificate,
                    FlowPlanarMetrics {
                        dual_faces: result.metrics.dual_faces,
                        dual_shortest_path_runs: result.metrics.dual_shortest_path_runs,
                        dual_arc_scans: result.metrics.dual_arc_scans,
                        settled_faces: result.metrics.settled_faces,
                        positive_flow_edges: result.metrics.positive_flow_edges,
                    },
                )
                .map_err(|error| JsError::new(&error.to_string()))?,
            Err(error) => return self.hassin_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn hassin_error_frames(&self, error: HassinError) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            HassinError::AdmissionLimit | HassinError::WorkLimit => scene.apply_resource_limit(),
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_borradaile_klein_planar(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        embedding: &flow::FlowPlanarEmbeddingV1,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
            return match trace_borradaile_klein_planar(graph, source, sink, embedding) {
                Ok(run) => max_flow_trace_frames_from_parts(
                    &self.scenario,
                    graph,
                    &run.result.certificate,
                    &run.base_snapshot,
                    &run.events,
                    &run.final_snapshot,
                ),
                Err(error) => self.borradaile_klein_error_frames(error),
            };
        }
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_borradaile_klein_planar(graph, source, sink, embedding) {
            Ok(result) => scene
                .apply_leftmost_planar_max_flow_result(
                    graph,
                    &result.flows,
                    &result.certificate,
                    FlowLeftmostPlanarMetrics {
                        dual_faces: result.metrics.dual_faces,
                        preprocessing_runs: result.metrics.preprocessing_runs,
                        dart_scans: result
                            .metrics
                            .dual_arc_scans
                            .checked_add(result.metrics.rotation_dart_scans)
                            .ok_or_else(|| JsError::new("planar metric overflow"))?,
                        right_first_searches: result.metrics.right_first_searches,
                        augmentations: result.metrics.augmentations,
                        saturated_path_darts: result.metrics.saturated_path_darts,
                        discovered_vertices: result.metrics.discovered_vertices,
                    },
                )
                .map_err(|error| JsError::new(&error.to_string()))?,
            Err(error) => return self.borradaile_klein_error_frames(error),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn borradaile_klein_error_frames(
        &self,
        error: BorradaileKleinError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            BorradaileKleinError::AdmissionLimit | BorradaileKleinError::WorkLimit => {
                scene.apply_resource_limit();
            }
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    #[allow(clippy::too_many_lines)]
    fn prepare_max_flow_trace(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        runner: MaxFlowRunner,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        match runner {
            MaxFlowRunner::FordFulkerson(preset) => {
                match trace_ford_fulkerson_preset(preset, graph, source, sink, feasibility) {
                    Ok(run) => ford_fulkerson_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.ford_fulkerson_error_frames(graph, source, sink, error),
                }
            }
            MaxFlowRunner::EdmondsKarp => {
                match flow::trace_edmonds_karp_with_feasibility(graph, source, sink, feasibility) {
                    Ok(run) => max_flow_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.max_flow_error_frames(graph, source, sink, error),
                }
            }
            MaxFlowRunner::Sap(preset) => {
                match trace_sap_preset(preset, graph, source, sink, feasibility) {
                    Ok(run) => sap_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.sap_error_frames(graph, source, sink, error),
                }
            }
            MaxFlowRunner::DistanceDirected(preset) => {
                self.prepare_distance_directed_trace(graph, source, sink, preset)
            }
            MaxFlowRunner::Dinic(preset) => {
                match trace_dinic_preset(preset, graph, source, sink, feasibility) {
                    Ok(run) => dinic_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.dinic_error_frames(graph, source, sink, error),
                }
            }
            MaxFlowRunner::DynamicTreeBlocking => {
                self.prepare_dynamic_tree_blocking_trace(graph, source, sink, feasibility)
            }
            MaxFlowRunner::DynamicTreePushRelabel => {
                self.prepare_dynamic_tree_push_relabel_trace(graph, source, sink, feasibility)
            }
            MaxFlowRunner::BinaryBlocking => {
                self.prepare_binary_blocking_trace(graph, source, sink)
            }
            MaxFlowRunner::GoldbergRao => self.prepare_goldberg_rao_trace(graph, source, sink),
            MaxFlowRunner::Orlin => match trace_orlin_max_flow(graph, source, sink) {
                Ok(run) => orlin_max_flow_trace_frames(&self.scenario, graph, &run),
                Err(error) => self.orlin_max_flow_error_frames(error),
            },
            MaxFlowRunner::ElectricalFlow => match trace_electrical_flow(graph, source, sink) {
                Ok(run) => electrical_flow_trace_frames(&self.scenario, graph, source, sink, &run),
                Err(error) => self.electrical_flow_error_frames(error),
            },
            MaxFlowRunner::AugmentingElectricalFlow => {
                match trace_augmenting_electrical_flow(graph, source, sink) {
                    Ok(run) => augmenting_electrical_trace_frames(
                        &self.scenario,
                        graph,
                        source,
                        sink,
                        &run,
                    ),
                    Err(error) => self.augmenting_electrical_error_frames(error),
                }
            }
            MaxFlowRunner::InteriorPoint => {
                match trace_interior_point_max_flow(graph, source, sink) {
                    Ok(run) => interior_point_max_flow_trace_frames(
                        &self.scenario,
                        graph,
                        source,
                        sink,
                        &run,
                    ),
                    Err(error) => self.interior_point_max_flow_error_frames(error),
                }
            }
            MaxFlowRunner::MinimumRatioCycle => match trace_minimum_ratio_cycle(graph) {
                Ok(run) => minimum_ratio_cycle_trace_frames(&self.scenario, graph, &run),
                Err(error) => self.minimum_ratio_cycle_error_frames(error),
            },
            MaxFlowRunner::WeightedAugmentingPaths => {
                match trace_weighted_augmenting_paths(graph, source, sink) {
                    Ok(run) => weighted_augmenting_paths_trace_frames(
                        &self.scenario,
                        graph,
                        source,
                        sink,
                        &run,
                    ),
                    Err(error) => self.weighted_augmenting_paths_error_frames(error),
                }
            }
            MaxFlowRunner::WeightedPushRelabel => {
                match trace_weighted_push_relabel_shortcut(graph, source, sink) {
                    Ok(run) => weighted_push_relabel_shortcut_trace_frames(
                        &self.scenario,
                        graph,
                        source,
                        sink,
                        &run,
                    ),
                    Err(error) => self.weighted_push_relabel_shortcut_error_frames(error),
                }
            }
            MaxFlowRunner::RandomizedAlmostLinear => {
                match trace_randomized_almost_linear_max_flow(graph, source, sink) {
                    Ok(run) => randomized_almost_linear_trace_frames(
                        &self.scenario,
                        graph,
                        source,
                        sink,
                        &run,
                    ),
                    Err(error) => self.randomized_almost_linear_error_frames(error),
                }
            }
            MaxFlowRunner::DeterministicAlmostLinear => {
                match trace_deterministic_almost_linear_max_flow(graph, source, sink) {
                    Ok(run) => deterministic_almost_linear_trace_frames(
                        &self.scenario,
                        graph,
                        source,
                        sink,
                        &run,
                    ),
                    Err(error) => self.deterministic_almost_linear_error_frames(error),
                }
            }
            MaxFlowRunner::BlockingPreflow(preset) => {
                match trace_blocking_preflow_preset(preset, graph, source, sink, feasibility) {
                    Ok(run) => blocking_preflow_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.blocking_preflow_error_frames(graph, source, sink, error),
                }
            }
            MaxFlowRunner::PushRelabel(preset) => {
                match trace_push_relabel_preset(preset, graph, source, sink, feasibility) {
                    Ok(run) => push_relabel_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.push_relabel_error_frames(graph, source, sink, error),
                }
            }
            MaxFlowRunner::SynchronousPushRelabel => {
                match trace_synchronous_parallel_push_relabel(graph, source, sink) {
                    Ok(run) => max_flow_trace_frames_from_parts(
                        &self.scenario,
                        graph,
                        &run.result.certificate,
                        &run.base_snapshot,
                        &run.events,
                        &run.final_snapshot,
                    ),
                    Err(error) => self.synchronous_push_relabel_error_frames(error),
                }
            }
            MaxFlowRunner::WarmStartPushRelabel => {
                let predicted = scenario_initial_flows(&self.scenario, graph)?;
                match trace_warm_start_push_relabel(graph, source, sink, &predicted) {
                    Ok(run) => max_flow_trace_frames_from_parts(
                        &self.scenario,
                        graph,
                        &run.result.certificate,
                        &run.base_snapshot,
                        &run.events,
                        &run.final_snapshot,
                    ),
                    Err(error) => self.warm_start_error_frames(error),
                }
            }
            MaxFlowRunner::Pseudoflow(preset) => match preset {
                PseudoflowRunner::Hochbaum => {
                    match flow::trace_hochbaum_pseudoflow_with_feasibility(
                        graph,
                        source,
                        sink,
                        feasibility,
                    ) {
                        Ok(run) => pseudoflow_trace_frames(&self.scenario, graph, &run),
                        Err(error) => self.pseudoflow_error_frames(graph, source, sink, error),
                    }
                }
                PseudoflowRunner::Simplex => match flow::trace_pseudoflow_simplex_with_feasibility(
                    graph,
                    source,
                    sink,
                    feasibility,
                ) {
                    Ok(run) => max_flow_trace_frames_from_parts(
                        &self.scenario,
                        graph,
                        &run.result.certificate,
                        &run.base_snapshot,
                        &run.events,
                        &run.final_snapshot,
                    ),
                    Err(error) => self.pseudoflow_error_frames(graph, source, sink, error),
                },
            },
            MaxFlowRunner::BoykovKolmogorov => match trace_boykov_kolmogorov(graph, source, sink) {
                Ok(run) => max_flow_trace_frames_from_parts(
                    &self.scenario,
                    graph,
                    &run.result.certificate,
                    &run.base_snapshot,
                    &run.events,
                    &run.final_snapshot,
                ),
                Err(error) => self.boykov_kolmogorov_error_frames(error),
            },
            MaxFlowRunner::Ibfs => match trace_ibfs(graph, source, sink) {
                Ok(run) => max_flow_trace_frames_from_parts(
                    &self.scenario,
                    graph,
                    &run.result.certificate,
                    &run.base_snapshot,
                    &run.events,
                    &run.final_snapshot,
                ),
                Err(error) => self.ibfs_error_frames(error),
            },
            MaxFlowRunner::Eibfs => match trace_eibfs(graph, source, sink) {
                Ok(run) => max_flow_trace_frames_from_parts(
                    &self.scenario,
                    graph,
                    &run.result.certificate,
                    &run.base_snapshot,
                    &run.events,
                    &run.final_snapshot,
                ),
                Err(error) => self.eibfs_error_frames(error),
            },
            MaxFlowRunner::DynamicEibfs => self.prepare_dynamic_eibfs_trace(graph, source, sink),
        }
    }

    fn prepare_dynamic_tree_blocking_trace(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        match flow::trace_dynamic_tree_blocking_flow_with_feasibility(
            graph,
            source,
            sink,
            feasibility,
        ) {
            Ok(run) => dynamic_tree_blocking_trace_frames(&self.scenario, graph, &run),
            Err(error) => self.dynamic_tree_blocking_error_frames(graph, source, sink, error),
        }
    }

    fn prepare_dynamic_tree_push_relabel_trace(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        match flow::trace_dynamic_tree_push_relabel_with_feasibility(
            graph,
            source,
            sink,
            feasibility,
        ) {
            Ok(run) => dynamic_tree_push_relabel_trace_frames(&self.scenario, graph, &run),
            Err(error) => self.dynamic_tree_push_relabel_error_frames(graph, source, sink, error),
        }
    }

    fn prepare_goldberg_rao_trace(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        match trace_goldberg_rao(graph, source, sink) {
            Ok(run) => max_flow_trace_frames_from_parts(
                &self.scenario,
                graph,
                &run.result.certificate,
                &run.base_snapshot,
                &run.events,
                &run.final_snapshot,
            ),
            Err(error) => self.goldberg_rao_error_frames(error),
        }
    }

    fn prepare_binary_blocking_trace(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let initial_flows = scenario_initial_flows(&self.scenario, graph)?;
        match trace_binary_blocking_first_step(graph, source, sink, &initial_flows) {
            Ok(run) => {
                flow::check_binary_blocking_step_trace(graph, source, sink, &run)
                    .map_err(|_| JsError::new("binary blocking-flow source trace is invalid"))?;
                binary_blocking_trace_frames(&self.scenario, graph, &run)
            }
            Err(error) => self.goldberg_rao_error_frames(error),
        }
    }

    fn prepare_distance_directed_trace(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        preset: DistanceDirectedRunner,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        match trace_distance_directed_preset(preset, graph, source, sink) {
            Ok(run) => max_flow_trace_frames_from_parts(
                &self.scenario,
                graph,
                &run.result.certificate,
                &run.base_snapshot,
                &run.events,
                &run.final_snapshot,
            ),
            Err(error) => self.distance_directed_error_frames(error),
        }
    }

    fn prepare_dynamic_eibfs_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let updates = dynamic_eibfs_updates(&self.scenario)?;
        let problem = prepare_dynamic_eibfs(graph, source, sink, &updates)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let result = match solve_dynamic_eibfs(graph, source, sink, &updates) {
            Ok(result) => result,
            Err(error) => return self.dynamic_eibfs_error_frames(error),
        };
        let final_graph = problem
            .graph_at_prefix(updates.len())
            .map_err(|error| JsError::new(&error.to_string()))?;
        let mut scene = ready_flow_scene(&self.scenario)?;
        scene
            .apply_dynamic_eibfs_result(&final_graph, &result)
            .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_dynamic_eibfs_trace(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let updates = dynamic_eibfs_updates(&self.scenario)?;
        let problem = prepare_dynamic_eibfs(graph, source, sink, &updates)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let run = match trace_dynamic_eibfs(graph, source, sink, &updates) {
            Ok(run) => run,
            Err(error) => return self.dynamic_eibfs_error_frames(error),
        };
        let certificate = &run
            .result
            .prefixes
            .last()
            .ok_or_else(|| JsError::new("Dynamic EIBFS returned no certified prefix"))?
            .certificate;
        max_flow_trace_frames_from_parts(
            &self.scenario,
            problem.envelope(),
            certificate,
            &run.base_snapshot,
            &run.events,
            &run.final_snapshot,
        )
    }

    fn dynamic_eibfs_error_frames(
        &self,
        error: DynamicEibfsSolveError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            DynamicEibfsSolveError::Input(
                DynamicEibfsError::UpdateLimit
                | DynamicEibfsError::Static(EibfsError::AdmissionLimit | EibfsError::WorkLimit),
            )
            | DynamicEibfsSolveError::Kernel(
                EibfsError::AdmissionLimit
                | EibfsError::WorkLimit
                | EibfsError::Trace(FlowTraceError::EventLimit),
            ) => scene.apply_resource_limit(),
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn ibfs_error_frames(&self, error: IbfsError) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            IbfsError::AdmissionLimit | IbfsError::WorkLimit => scene.apply_resource_limit(),
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn boykov_kolmogorov_error_frames(
        &self,
        error: BoykovKolmogorovError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            BoykovKolmogorovError::AdmissionLimit
            | BoykovKolmogorovError::WorkLimit
            | BoykovKolmogorovError::Trace(FlowTraceError::EventLimit) => {
                scene.apply_resource_limit();
            }
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn eibfs_error_frames(&self, error: EibfsError) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            EibfsError::AdmissionLimit
            | EibfsError::WorkLimit
            | EibfsError::Trace(FlowTraceError::EventLimit) => scene.apply_resource_limit(),
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn classified_failure_frames<E>(
        &self,
        error: E,
        verify_infeasibility: impl FnOnce(&flow::InfeasibilityWitness) -> Result<(), JsError>,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError>
    where
        E: ClassifyRuntimeFailure,
    {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error.classify() {
            RuntimeFailure::Infeasible(witness) => {
                verify_infeasibility(&witness)?;
                scene.apply_infeasibility(&witness);
            }
            RuntimeFailure::ResourceLimit => scene.apply_resource_limit(),
            RuntimeFailure::Fatal(message) => {
                #[cfg(test)]
                eprintln!("FLOW_FATAL_RUNTIME_FAILURE\t{message}");
                return Err(JsError::new(&message));
            }
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn max_flow_error_frames<E>(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        error: E,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError>
    where
        E: ClassifyRuntimeFailure,
    {
        self.classified_failure_frames(error, |witness| {
            check_max_flow_infeasibility(graph, source, sink, witness)
                .map_err(|error| JsError::new(&error.to_string()))
        })
    }

    fn balance_error_frames<E>(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: E,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError>
    where
        E: ClassifyRuntimeFailure,
    {
        self.classified_failure_frames(error, |witness| {
            check_balance_infeasibility(graph, target, witness)
                .map_err(|error| JsError::new(&error.to_string()))
        })
    }

    fn prepare_pseudoflow_fast(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        preset: PseudoflowRunner,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match preset {
            PseudoflowRunner::Hochbaum => match flow::solve_hochbaum_pseudoflow_with_feasibility(
                graph,
                source,
                sink,
                feasibility,
            ) {
                Ok(result) => apply_pseudoflow_result(&mut scene, graph, &result)?,
                Err(error) => return self.pseudoflow_error_frames(graph, source, sink, error),
            },
            PseudoflowRunner::Simplex => match flow::solve_pseudoflow_simplex_with_feasibility(
                graph,
                source,
                sink,
                feasibility,
            ) {
                Ok(result) => apply_pseudoflow_simplex_result(&mut scene, graph, &result)?,
                Err(error) => return self.pseudoflow_error_frames(graph, source, sink, error),
            },
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn ford_fulkerson_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        error: FordFulkersonError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.max_flow_error_frames(graph, source, sink, error)
    }

    fn dinic_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        error: DinicError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.max_flow_error_frames(graph, source, sink, error)
    }

    fn dynamic_tree_blocking_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        error: DynamicTreeBlockingError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.max_flow_error_frames(graph, source, sink, error)
    }

    fn dynamic_tree_push_relabel_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        error: DynamicTreePushRelabelError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.max_flow_error_frames(graph, source, sink, error)
    }

    fn synchronous_push_relabel_error_frames(
        &self,
        error: SynchronousPushRelabelError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            SynchronousPushRelabelError::AdmissionLimit
            | SynchronousPushRelabelError::WorkLimit
            | SynchronousPushRelabelError::Trace(FlowTraceError::EventLimit) => {
                scene.apply_resource_limit();
            }
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn warm_start_error_frames(
        &self,
        error: WarmStartPushRelabelError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            WarmStartPushRelabelError::AdmissionLimit
            | WarmStartPushRelabelError::WorkLimit
            | WarmStartPushRelabelError::Trace(FlowTraceError::EventLimit) => {
                scene.apply_resource_limit();
            }
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn goldberg_rao_error_frames(
        &self,
        error: GoldbergRaoError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            GoldbergRaoError::AdmissionLimit
            | GoldbergRaoError::WorkLimit
            | GoldbergRaoError::Trace(FlowTraceError::EventLimit) => {
                scene.apply_resource_limit();
            }
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn distance_directed_error_frames(
        &self,
        error: DistanceDirectedError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            DistanceDirectedError::AdmissionLimit
            | DistanceDirectedError::WorkLimit
            | DistanceDirectedError::Trace(FlowTraceError::EventLimit) => {
                scene.apply_resource_limit();
            }
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn blocking_preflow_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        error: BlockingPreflowError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.max_flow_error_frames(graph, source, sink, error)
    }

    fn pseudoflow_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        error: PseudoflowError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.max_flow_error_frames(graph, source, sink, error)
    }

    fn sap_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        error: SapError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.max_flow_error_frames(graph, source, sink, error)
    }

    fn push_relabel_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
        error: PushRelabelError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.max_flow_error_frames(graph, source, sink, error)
    }

    fn cancel_tighten_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: CancelTightenError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn relaxed_mndc_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: RelaxedMndcError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn enhanced_capacity_scaling_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: EnhancedCapacityScalingError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn orlin_mcf_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: OrlinMcfError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn orlin_max_flow_error_frames(
        &self,
        error: OrlinMaxError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            OrlinMaxError::AdmissionLimit | OrlinMaxError::WorkLimit => {
                scene.apply_resource_limit();
            }
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn electrical_flow_error_frames(
        &self,
        error: ElectricalFlowError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            ElectricalFlowError::AdmissionLimit | ElectricalFlowError::NonConvergence => {
                scene.apply_resource_limit();
            }
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn augmenting_electrical_error_frames(
        &self,
        error: AugmentingElectricalError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            AugmentingElectricalError::AdmissionLimit => {
                scene.apply_resource_limit_with_reason(FlowResourceLimitReasonV1::InputAdmission);
            }
            AugmentingElectricalError::BoostResourceLimit => {
                scene.apply_resource_limit_with_reason(FlowResourceLimitReasonV1::TransformedGraph);
            }
            AugmentingElectricalError::WorkLimit => {
                scene.apply_resource_limit_with_reason(FlowResourceLimitReasonV1::RuntimeWork);
            }
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn interior_point_max_flow_error_frames(
        &self,
        error: InteriorPointMaxFlowError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            InteriorPointMaxFlowError::AdmissionLimit
            | InteriorPointMaxFlowError::ReductionLimit
            | InteriorPointMaxFlowError::NonConvergence => scene.apply_resource_limit(),
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn minimum_ratio_cycle_error_frames(
        &self,
        error: MinimumRatioCycleError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            MinimumRatioCycleError::AdmissionLimit | MinimumRatioCycleError::WorkLimit => {
                scene.apply_resource_limit();
            }
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn minimum_ratio_cycle_mcf_error_frames(
        &self,
        error: MinimumRatioCycleMcfError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            MinimumRatioCycleMcfError::AdmissionLimit | MinimumRatioCycleMcfError::WorkLimit => {
                scene.apply_resource_limit();
            }
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn randomized_almost_linear_error_frames(
        &self,
        error: RandomizedAlmostLinearMaxFlowError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            RandomizedAlmostLinearMaxFlowError::AdmissionLimit
            | RandomizedAlmostLinearMaxFlowError::ForestLimit => scene.apply_resource_limit(),
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn randomized_almost_linear_mcf_error_frames(
        &self,
        error: RandomizedAlmostLinearMcfError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            RandomizedAlmostLinearMcfError::AdmissionLimit
            | RandomizedAlmostLinearMcfError::IsolationExhausted => scene.apply_resource_limit(),
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn flow_framework_mcf_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: FlowFrameworkMcfError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn weighted_augmenting_paths_error_frames(
        &self,
        error: WeightedAugmentingPathsError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            WeightedAugmentingPathsError::AdmissionLimit
            | WeightedAugmentingPathsError::WorkLimit => scene.apply_resource_limit(),
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn weighted_push_relabel_shortcut_error_frames(
        &self,
        error: WeightedPushRelabelShortcutError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            WeightedPushRelabelShortcutError::AdmissionLimit
            | WeightedPushRelabelShortcutError::WorkLimit => scene.apply_resource_limit(),
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn deterministic_almost_linear_error_frames(
        &self,
        error: DeterministicAlmostLinearMaxFlowError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            DeterministicAlmostLinearMaxFlowError::AdmissionLimit
            | DeterministicAlmostLinearMaxFlowError::ForestLimit => scene.apply_resource_limit(),
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn primal_dual_ipm_mcf_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: PrimalDualIpmError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn electrical_ipm_mcf_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: ElectricalIpmMcfError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn dual_network_simplex_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: DualNetworkSimplexError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn polynomial_primal_simplex_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: PolynomialPrimalSimplexError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn polynomial_dual_simplex_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: PolynomialDualSimplexError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn double_scaling_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: DoubleScalingError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn convex_cost_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        error: ConvexCostError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            ConvexCostError::Oracle(MinimumMeanCycleCancelingError::Feasibility(
                FeasibilityError::Infeasible(witness),
            )) => {
                let target = supply_divergences(graph)
                    .map_err(|source| JsError::new(&source.to_string()))?;
                check_balance_infeasibility(graph, &target, &witness)
                    .map_err(|source| JsError::new(&source.to_string()))?;
                scene.apply_infeasibility(&witness);
            }
            ConvexCostError::AdmissionLimit
            | ConvexCostError::Oracle(
                MinimumMeanCycleCancelingError::AdmissionLimit
                | MinimumMeanCycleCancelingError::WorkLimit,
            ) => scene.apply_resource_limit(),
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn convex_cost_scaling_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        error: ConvexCostScalingError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            ConvexCostScalingError::Feasibility(FeasibilityError::Infeasible(witness))
            | ConvexCostScalingError::Convex(ConvexCostError::Oracle(
                MinimumMeanCycleCancelingError::Feasibility(FeasibilityError::Infeasible(witness)),
            )) => {
                let target = supply_divergences(graph)
                    .map_err(|source| JsError::new(&source.to_string()))?;
                check_balance_infeasibility(graph, &target, &witness)
                    .map_err(|source| JsError::new(&source.to_string()))?;
                scene.apply_infeasibility(&witness);
            }
            ConvexCostScalingError::AdmissionLimit
            | ConvexCostScalingError::WorkLimit
            | ConvexCostScalingError::Convex(
                ConvexCostError::AdmissionLimit
                | ConvexCostError::Oracle(
                    MinimumMeanCycleCancelingError::AdmissionLimit
                    | MinimumMeanCycleCancelingError::WorkLimit,
                ),
            ) => scene.apply_resource_limit(),
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn convex_network_simplex_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        error: ConvexNetworkSimplexError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            ConvexNetworkSimplexError::Feasibility(FeasibilityError::Infeasible(witness))
            | ConvexNetworkSimplexError::Convex(ConvexCostError::Oracle(
                MinimumMeanCycleCancelingError::Feasibility(FeasibilityError::Infeasible(witness)),
            )) => {
                let target = supply_divergences(graph)
                    .map_err(|source| JsError::new(&source.to_string()))?;
                check_balance_infeasibility(graph, &target, &witness)
                    .map_err(|source| JsError::new(&source.to_string()))?;
                scene.apply_infeasibility(&witness);
            }
            ConvexNetworkSimplexError::AdmissionLimit
            | ConvexNetworkSimplexError::WorkLimit
            | ConvexNetworkSimplexError::Convex(
                ConvexCostError::AdmissionLimit
                | ConvexCostError::Oracle(
                    MinimumMeanCycleCancelingError::AdmissionLimit
                    | MinimumMeanCycleCancelingError::WorkLimit,
                ),
            ) => scene.apply_resource_limit(),
            source => return Err(JsError::new(&source.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn prepare_convex_cost_flow(
        &self,
        graph: &flow::FlowNetwork,
        runner: ConvexCostFlowRunner,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let problem = self
            .scenario
            .convex_cost_problem(graph)
            .map_err(|error| JsError::new(&error.to_string()))?;
        if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
            return match runner {
                ConvexCostFlowRunner::SegmentExpanded => {
                    match flow::trace_segment_expanded_convex_cost_with_feasibility(
                        &problem,
                        feasibility,
                    ) {
                        Ok(run) => convex_cost_trace_frames(&self.scenario, graph, &problem, &run),
                        Err(error) => self.convex_cost_error_frames(graph, error),
                    }
                }
                ConvexCostFlowRunner::CostScaling => {
                    match flow::trace_convex_cost_scaling_with_feasibility(&problem, feasibility) {
                        Ok(run) => {
                            convex_cost_scaling_trace_frames(&self.scenario, graph, &problem, &run)
                        }
                        Err(error) => self.convex_cost_scaling_error_frames(graph, error),
                    }
                }
                ConvexCostFlowRunner::NetworkSimplex => {
                    match flow::trace_convex_network_simplex_with_feasibility(&problem, feasibility)
                    {
                        Ok(run) => convex_network_simplex_trace_frames(
                            &self.scenario,
                            graph,
                            &problem,
                            &run,
                        ),
                        Err(error) => self.convex_network_simplex_error_frames(graph, error),
                    }
                }
            };
        }
        let mut scene = ready_flow_scene(&self.scenario)?;
        match runner {
            ConvexCostFlowRunner::SegmentExpanded => {
                match flow::solve_segment_expanded_convex_cost_with_feasibility(
                    &problem,
                    feasibility,
                ) {
                    Ok(result) => {
                        let snapshot = convex_result_snapshot(&problem, &result);
                        let overlay = convex_cost_overlay(
                            graph,
                            &problem,
                            &snapshot,
                            FlowConvexCostStageV1::Optimal,
                        )?;
                        scene
                            .apply_convex_cost_boundary(
                                graph,
                                FlowConvexCostBoundary {
                                    flows: &result.flows,
                                    node_labels: &snapshot.node_labels,
                                    search_order: &snapshot.search_order,
                                    remaining_divergence: &[],
                                    overlay,
                                    event_id: 0,
                                    event_count: 0,
                                },
                            )
                            .map_err(|error| JsError::new(&error.to_string()))?;
                        scene.set_convex_cost_metrics(
                            result.metrics.mean_cycle_searches,
                            result.metrics.dynamic_programming_rounds,
                            result.metrics.residual_arc_scans,
                            result.metrics.canceled_cycles,
                        );
                        scene.set_convex_cost_outcome(graph, &result.certificate);
                        Ok(vec![self.frames[0].clone(), scene])
                    }
                    Err(error) => self.convex_cost_error_frames(graph, error),
                }
            }
            ConvexCostFlowRunner::CostScaling => {
                match flow::solve_convex_cost_scaling_with_feasibility(&problem, feasibility) {
                    Ok(result) => {
                        convex_cost_scaling_fast_frames(&self.scenario, graph, &problem, &result)
                    }
                    Err(error) => self.convex_cost_scaling_error_frames(graph, error),
                }
            }
            ConvexCostFlowRunner::NetworkSimplex => {
                match flow::solve_convex_network_simplex_with_feasibility(&problem, feasibility) {
                    Ok(result) => {
                        convex_network_simplex_fast_frames(&self.scenario, graph, &problem, &result)
                    }
                    Err(error) => self.convex_network_simplex_error_frames(graph, error),
                }
            }
        }
    }

    // Keeping the bounded complete-solver presets in one closed dispatch makes
    // fallback behavior directly reviewable at the WASM trust boundary.
    #[allow(clippy::too_many_lines)]
    fn prepare_min_cost_flow(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        runner: MinCostFlowRunner,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        match runner {
            MinCostFlowRunner::DeterministicAlmostLinear => {
                if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
                    return match flow::trace_flow_framework_mcf_with_feasibility(
                        graph,
                        target,
                        flow::FLOW_FRAMEWORK_MCF_MAX_TRACE_ITERATIONS,
                        feasibility,
                    ) {
                        Ok(run) => flow_framework_mcf_trace_frames(&self.scenario, graph, &run),
                        Err(error) => self.flow_framework_mcf_error_frames(graph, target, error),
                    };
                }
                match flow::execute_flow_framework_mcf_with_feasibility(
                    graph,
                    target,
                    flow::FLOW_FRAMEWORK_MCF_MAX_ITERATIONS,
                    feasibility,
                ) {
                    Ok(result) => flow_framework_mcf_fast_frames(&self.scenario, graph, &result),
                    Err(error) => self.flow_framework_mcf_error_frames(graph, target, error),
                }
            }
            MinCostFlowRunner::RandomizedAlmostLinear => {
                if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
                    return match flow::trace_randomized_almost_linear_mcf_with_feasibility(
                        graph,
                        target,
                        feasibility,
                    ) {
                        Ok(run) => {
                            randomized_almost_linear_mcf_trace_frames(&self.scenario, graph, &run)
                        }
                        Err(error) => self.randomized_almost_linear_mcf_error_frames(error),
                    };
                }
                match flow::solve_randomized_almost_linear_mcf_with_feasibility(
                    graph,
                    target,
                    feasibility,
                ) {
                    Ok(result) => {
                        randomized_almost_linear_mcf_fast_frames(&self.scenario, graph, &result)
                    }
                    Err(error) => self.randomized_almost_linear_mcf_error_frames(error),
                }
            }
            MinCostFlowRunner::MinimumRatioCycle => {
                if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
                    return match flow::trace_minimum_ratio_cycle_mcf_with_feasibility(
                        graph,
                        target,
                        feasibility,
                    ) {
                        Ok(run) => {
                            minimum_ratio_cycle_mcf_trace_frames(&self.scenario, graph, &run)
                        }
                        Err(error) => self.minimum_ratio_cycle_mcf_error_frames(error),
                    };
                }
                match flow::solve_minimum_ratio_cycle_mcf_with_feasibility(
                    graph,
                    target,
                    feasibility,
                ) {
                    Ok(result) => {
                        minimum_ratio_cycle_mcf_fast_frames(&self.scenario, graph, &result)
                    }
                    Err(error) => self.minimum_ratio_cycle_mcf_error_frames(error),
                }
            }
            MinCostFlowRunner::ElectricalFlowInteriorPoint => {
                if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
                    return match flow::trace_electrical_flow_interior_point_mcf_with_feasibility(
                        graph,
                        target,
                        feasibility,
                    ) {
                        Ok(run) => electrical_ipm_mcf_trace_frames(&self.scenario, graph, &run),
                        Err(error) => self.electrical_ipm_mcf_error_frames(graph, target, error),
                    };
                }
                match flow::solve_electrical_flow_interior_point_mcf_with_feasibility(
                    graph,
                    target,
                    feasibility,
                ) {
                    Ok(result) => electrical_ipm_mcf_fast_frames(&self.scenario, graph, &result),
                    Err(error) => self.electrical_ipm_mcf_error_frames(graph, target, error),
                }
            }
            MinCostFlowRunner::PrimalDualInteriorPoint => {
                if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
                    return match flow::trace_primal_dual_interior_point_mcf_with_feasibility(
                        graph,
                        target,
                        feasibility,
                    ) {
                        Ok(run) => primal_dual_ipm_mcf_trace_frames(&self.scenario, graph, &run),
                        Err(error) => self.primal_dual_ipm_mcf_error_frames(graph, target, error),
                    };
                }
                match flow::solve_primal_dual_interior_point_mcf_with_feasibility(
                    graph,
                    target,
                    feasibility,
                ) {
                    Ok(result) => primal_dual_ipm_mcf_fast_frames(&self.scenario, graph, &result),
                    Err(error) => self.primal_dual_ipm_mcf_error_frames(graph, target, error),
                }
            }
            MinCostFlowRunner::TardosFramework => {
                let potentials = tardos_framework_config(&self.scenario, graph)
                    .map_err(|error| JsError::new(&error))?;
                if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
                    return match flow::trace_tardos_framework_primitive_with_feasibility(
                        graph,
                        target,
                        &potentials,
                        feasibility,
                    ) {
                        Ok(run) => tardos_framework_trace_frames(&self.scenario, graph, &run),
                        Err(error) => self.tardos_framework_error_frames(graph, target, error),
                    };
                }
                match flow::solve_tardos_framework_primitive_with_feasibility(
                    graph,
                    target,
                    &potentials,
                    feasibility,
                ) {
                    Ok(result) => tardos_framework_fast_frames(&self.scenario, graph, &result),
                    Err(error) => self.tardos_framework_error_frames(graph, target, error),
                }
            }
            MinCostFlowRunner::PredictionAssistedEpsilonRelaxation => {
                let config = prediction_assisted_epsilon_config(&self.scenario, graph)
                    .map_err(|error| JsError::new(&error))?;
                if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
                    return match flow::trace_prediction_assisted_epsilon_relaxation_with_feasibility(
                        graph,
                        target,
                        &config.predicted_prices,
                        config.scaling_parameter,
                        feasibility,
                    ) {
                        Ok(run) => prediction_assisted_epsilon_trace_frames(
                            &self.scenario,
                            graph,
                            target,
                            config.scaling_parameter,
                            &run,
                        ),
                        Err(error) => {
                            self.prediction_assisted_epsilon_error_frames(graph, target, error)
                        }
                    };
                }
                match flow::solve_prediction_assisted_epsilon_relaxation_with_feasibility(
                    graph,
                    target,
                    &config.predicted_prices,
                    config.scaling_parameter,
                    feasibility,
                ) {
                    Ok(result) => prediction_assisted_epsilon_fast_frames(
                        &self.scenario,
                        graph,
                        target,
                        config.scaling_parameter,
                        &result,
                    ),
                    Err(error) => {
                        self.prediction_assisted_epsilon_error_frames(graph, target, error)
                    }
                }
            }
            MinCostFlowRunner::Classical(runner) => {
                if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
                    return self.prepare_min_cost_flow_trace(graph, target, runner, feasibility);
                }
                let mut scene = ready_flow_scene(&self.scenario)?;
                apply_classical_min_cost_result(runner, &mut scene, graph, target, feasibility)?;
                Ok(vec![self.frames[0].clone(), scene])
            }
        }
    }

    // Keeping every trace preset in one closed dispatch makes catalog identity
    // reviewable and prevents an implicit fallback to a neighboring solver.
    #[allow(clippy::too_many_lines)]
    fn prepare_min_cost_flow_trace(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        runner: ClassicalMinCostFlowRunner,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        match runner {
            ClassicalMinCostFlowRunner::MinimumMeanCycleCanceling => {
                match flow::trace_minimum_mean_cycle_canceling_with_feasibility(
                    graph,
                    target,
                    feasibility,
                ) {
                    Ok(run) => {
                        minimum_mean_cycle_canceling_trace_frames(&self.scenario, graph, &run)
                    }
                    Err(error) => {
                        self.minimum_mean_cycle_canceling_error_frames(graph, target, error)
                    }
                }
            }
            ClassicalMinCostFlowRunner::CancelAndTighten => {
                match flow::trace_cancel_and_tighten_with_feasibility(graph, target, feasibility) {
                    Ok(run) => cancel_tighten_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.cancel_tighten_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::RelaxedMostNegativeCycle => {
                match flow::trace_relaxed_mndc_with_feasibility(graph, target, feasibility) {
                    Ok(run) => relaxed_mndc_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.relaxed_mndc_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::EnhancedCapacityScaling => {
                match flow::trace_enhanced_capacity_scaling_with_feasibility(
                    graph,
                    target,
                    feasibility,
                ) {
                    Ok(run) => enhanced_capacity_scaling_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.enhanced_capacity_scaling_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::Orlin => {
                match flow::trace_orlin_mcf_with_feasibility(graph, target, feasibility) {
                    Ok(run) => orlin_mcf_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.orlin_mcf_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::DualNetworkSimplex => {
                match flow::trace_dual_network_simplex_with_feasibility(graph, target, feasibility)
                {
                    Ok(run) => dual_network_simplex_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.dual_network_simplex_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::PolynomialDualNetworkSimplex => {
                match flow::trace_polynomial_dual_network_simplex_with_feasibility(
                    graph,
                    target,
                    feasibility,
                ) {
                    Ok(run) => polynomial_dual_simplex_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.polynomial_dual_simplex_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::PolynomialPrimalNetworkSimplex => {
                match flow::trace_polynomial_primal_network_simplex_with_feasibility(
                    graph,
                    target,
                    feasibility,
                ) {
                    Ok(run) => polynomial_primal_simplex_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.polynomial_primal_simplex_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::DoubleScaling => {
                match flow::trace_double_scaling_with_feasibility(graph, target, feasibility) {
                    Ok(run) => double_scaling_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.double_scaling_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::SimpleCycleCanceling => {
                match flow::trace_simple_cycle_canceling_with_feasibility(
                    graph,
                    target,
                    feasibility,
                ) {
                    Ok(run) => simple_cycle_canceling_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.simple_cycle_canceling_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::PotentialDijkstraSsp => {
                match flow::trace_potential_dijkstra_ssp_with_feasibility(
                    graph,
                    target,
                    feasibility,
                ) {
                    Ok(run) => potential_dijkstra_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.potential_dijkstra_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::PrimalDual => {
                match flow::trace_primal_dual_with_feasibility(graph, target, feasibility) {
                    Ok(run) => primal_dual_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.primal_dual_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::BlockingFlowPrimalDual => {
                self.prepare_blocking_primal_dual_trace(graph, target, feasibility)
            }
            ClassicalMinCostFlowRunner::CapacityScaling(CapacityScalingRunner::Capacity) => {
                match flow::trace_capacity_scaling_with_feasibility(graph, target, feasibility) {
                    Ok(run) => min_cost_trace_frames(
                        &self.scenario,
                        graph,
                        &run.result.certificate,
                        &run.base_snapshot,
                        &run.events,
                        &run.final_snapshot,
                    ),
                    Err(error) => self.capacity_scaling_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::CapacityScaling(CapacityScalingRunner::Excess) => {
                match flow::trace_excess_scaling_mcf_with_feasibility(graph, target, feasibility) {
                    Ok(run) => min_cost_trace_frames(
                        &self.scenario,
                        graph,
                        &run.result.certificate,
                        &run.base_snapshot,
                        &run.events,
                        &run.final_snapshot,
                    ),
                    Err(error) => self.capacity_scaling_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::OutOfKilter => {
                match flow::trace_out_of_kilter_with_feasibility(graph, target, feasibility) {
                    Ok(run) => min_cost_trace_frames(
                        &self.scenario,
                        graph,
                        &run.result.certificate,
                        &run.base_snapshot,
                        &run.events,
                        &run.final_snapshot,
                    ),
                    Err(error) => self.out_of_kilter_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::Relaxation => {
                match flow::trace_relaxation_with_feasibility(graph, target, feasibility) {
                    Ok(run) => min_cost_trace_frames(
                        &self.scenario,
                        graph,
                        &run.result.certificate,
                        &run.base_snapshot,
                        &run.events,
                        &run.final_snapshot,
                    ),
                    Err(error) => self.relaxation_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::EpsilonRelaxation => {
                match flow::trace_epsilon_relaxation_with_feasibility(graph, target, feasibility) {
                    Ok(run) => min_cost_trace_frames(
                        &self.scenario,
                        graph,
                        &run.result.certificate,
                        &run.base_snapshot,
                        &run.events,
                        &run.final_snapshot,
                    ),
                    Err(error) => self.epsilon_relaxation_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::CostScaling(preset) => {
                self.prepare_cost_scaling_trace(graph, target, preset, feasibility)
            }
            ClassicalMinCostFlowRunner::NetworkSimplex(NetworkSimplexRunner::Primal) => {
                self.prepare_network_simplex_trace(graph, target, false, feasibility)
            }
            ClassicalMinCostFlowRunner::NetworkSimplex(NetworkSimplexRunner::DynamicTree) => {
                self.prepare_network_simplex_trace(graph, target, true, feasibility)
            }
            ClassicalMinCostFlowRunner::SuccessiveShortestPath => {
                match flow::trace_successive_shortest_path_with_feasibility(
                    graph,
                    target,
                    feasibility,
                ) {
                    Ok(run) => bellman_ford_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.min_cost_error_frames(graph, target, error),
                }
            }
            ClassicalMinCostFlowRunner::BellmanFordSsp => {
                match flow::trace_bellman_ford_ssp_with_feasibility(graph, target, feasibility) {
                    Ok(run) => bellman_ford_trace_frames(&self.scenario, graph, &run),
                    Err(error) => self.min_cost_error_frames(graph, target, error),
                }
            }
        }
    }

    fn prepare_network_simplex_trace(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        dynamic_tree: bool,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        if dynamic_tree {
            return match flow::trace_dynamic_tree_network_simplex_with_feasibility(
                graph,
                target,
                feasibility,
            ) {
                Ok(run) => min_cost_trace_frames(
                    &self.scenario,
                    graph,
                    &run.result.certificate,
                    &run.base_snapshot,
                    &run.events,
                    &run.final_snapshot,
                ),
                Err(error) => self.network_simplex_error_frames(graph, target, error),
            };
        }
        match flow::trace_primal_network_simplex_with_feasibility(graph, target, feasibility) {
            Ok(run) => min_cost_trace_frames(
                &self.scenario,
                graph,
                &run.result.certificate,
                &run.base_snapshot,
                &run.events,
                &run.final_snapshot,
            ),
            Err(error) => self.network_simplex_error_frames(graph, target, error),
        }
    }

    fn prepare_blocking_primal_dual_trace(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        match flow::trace_blocking_primal_dual_with_feasibility(graph, target, feasibility) {
            Ok(run) => min_cost_trace_frames(
                &self.scenario,
                graph,
                &run.result.certificate,
                &run.base_snapshot,
                &run.events,
                &run.final_snapshot,
            ),
            Err(error) => self.blocking_primal_dual_error_frames(graph, target, error),
        }
    }

    fn prepare_cost_scaling_trace(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        preset: CostScalingRunner,
        feasibility: &mut flow::feasibility::FeasibilityExecution,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let preset = match preset {
            CostScalingRunner::CostScaling => flow::CostScalingExecutionPreset::CostScaling,
            CostScalingRunner::PushRelabel => flow::CostScalingExecutionPreset::PushRelabel,
            CostScalingRunner::AugmentRelabel => flow::CostScalingExecutionPreset::AugmentRelabel,
            CostScalingRunner::PartialAugmentRelabel => {
                flow::CostScalingExecutionPreset::PartialAugmentRelabel
            }
            CostScalingRunner::PriceRefinement => flow::CostScalingExecutionPreset::PriceRefinement,
            CostScalingRunner::ArcFixing => flow::CostScalingExecutionPreset::ArcFixing,
            CostScalingRunner::Generalized => {
                flow::CostScalingExecutionPreset::GeneralizedPushRelabel
            }
        };
        let run =
            flow::trace_cost_scaling_preset_with_feasibility(graph, target, preset, feasibility);
        match run {
            Ok(run) => cost_scaling_trace_frames(&self.scenario, graph, &run),
            Err(error) => self.cost_scaling_error_frames(graph, target, error),
        }
    }

    fn prepare_successive_shortest_augmenting_path(
        &self,
        graph: &flow::FlowNetwork,
        source: flow::NodeIndex,
        sink: flow::NodeIndex,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        if matches!(self.scenario.payload.run_profile, RunProfileV1::Trace) {
            return match trace_successive_shortest_augmenting_path(graph, source, sink) {
                Ok(run) => {
                    successive_shortest_augmenting_path_trace_frames(&self.scenario, graph, &run)
                }
                Err(error) => self.successive_shortest_augmenting_path_error_frames(error),
            };
        }
        let mut scene = ready_flow_scene(&self.scenario)?;
        match solve_successive_shortest_augmenting_path(graph, source, sink) {
            Ok(result) => scene
                .apply_min_cost_max_flow_result(
                    graph,
                    &result.flows,
                    &result.certificate,
                    FlowPotentialDijkstraMetrics {
                        dijkstra_runs: result.metrics.dijkstra_runs,
                        settled_nodes: result.metrics.settled_nodes,
                        potential_updates: result.metrics.potential_updates,
                        residual_arc_scans: result.metrics.residual_arc_scans,
                        augmentations: result.metrics.augmentations,
                    },
                )
                .map_err(|error| JsError::new(&error.to_string()))?,
            Err(error) => {
                return self.successive_shortest_augmenting_path_error_frames(error);
            }
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn successive_shortest_augmenting_path_error_frames(
        &self,
        error: SuccessiveShortestAugmentingPathError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        let mut scene = ready_flow_scene(&self.scenario)?;
        match error {
            SuccessiveShortestAugmentingPathError::AdmissionLimit
            | SuccessiveShortestAugmentingPathError::WorkLimit => scene.apply_resource_limit(),
            error => return Err(JsError::new(&error.to_string())),
        }
        Ok(vec![self.frames[0].clone(), scene])
    }

    fn min_cost_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: BellmanFordSspError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn potential_dijkstra_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: PotentialDijkstraSspError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn primal_dual_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: PrimalDualError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn blocking_primal_dual_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: BlockingPrimalDualError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn capacity_scaling_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: CapacityScalingError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn cost_scaling_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: CostScalingError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn out_of_kilter_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: OutOfKilterError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn relaxation_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: RelaxationError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn epsilon_relaxation_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: EpsilonRelaxationError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn prediction_assisted_epsilon_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: PredictionAssistedEpsilonError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn tardos_framework_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: TardosFrameworkError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn network_simplex_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: NetworkSimplexError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn simple_cycle_canceling_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: SimpleCycleCancelingError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn minimum_mean_cycle_canceling_error_frames(
        &self,
        graph: &flow::FlowNetwork,
        target: &[i128],
        error: MinimumMeanCycleCancelingError,
    ) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
        self.balance_error_frames(graph, target, error)
    }

    fn scenario_json(&self) -> Result<String, JsError> {
        canonicalize_serializable(&self.scenario)
    }

    fn begin_seek(&mut self, target: usize) -> Result<(), JsError> {
        if self.staged_next.is_some() {
            return Err(JsError::new("a staged flow event is already pending"));
        }
        if self.staged_seek.is_some() {
            return Err(JsError::new("a staged flow seek is already pending"));
        }
        let available_end = if self.prepared {
            self.prepared_event_count()
        } else {
            self.committed_end
        };
        if target > available_end {
            return Err(JsError::new(
                "flow session has no prepared event at that cursor",
            ));
        }
        if self.prepared {
            self.ensure_frame_cached(target)?;
        }
        self.staged_seek = Some(target);
        Ok(())
    }

    fn resume_seek_json(&self, max_items: usize) -> Result<String, JsError> {
        if max_items == 0 {
            return Err(JsError::new("seek chunk must be positive"));
        }
        let Some(target) = self.staged_seek else {
            return Err(JsError::new("no active seek"));
        };
        let frame = self
            .cached_frame(target)
            .ok_or_else(|| JsError::new("flow event cursor is out of range"))?;
        serialize_frame(&serde_json::json!({
            "cursor": target.to_string(),
            "done": true,
            "target": target.to_string(),
            "frame": frame
        }))
    }

    fn stage_next_json(&mut self) -> Result<Option<String>, JsError> {
        if self.staged_next.is_some() {
            return Err(JsError::new("a staged flow event is already pending"));
        }
        if !self.prepared {
            let frames = self.prepare_timeline()?;
            let timeline = FlowTimelineCache::from_prepared_timeline(frames, self.cursor, 1)?;
            let scene = timeline
                .get(1)
                .ok_or_else(|| JsError::new("flow execution produced no event"))?;
            let frame = serialize_frame(scene)?;
            self.staged_next = Some(StagedFlowNext {
                target: 1,
                replacement_timeline: Some(timeline),
            });
            return Ok(Some(frame));
        }
        let target = self.cursor.saturating_add(1);
        if target > self.prepared_event_count() {
            return Ok(None);
        }
        self.ensure_frame_cached(target)?;
        let scene = self
            .cached_frame(target)
            .ok_or_else(|| JsError::new("flow event cursor is out of range"))?;
        let frame = serialize_frame(scene)?;
        self.staged_next = Some(StagedFlowNext {
            target,
            replacement_timeline: None,
        });
        Ok(Some(frame))
    }

    fn commit_staged_next(&mut self) {
        if let Some(staged) = self.staged_next.take() {
            if let Some(timeline) = staged.replacement_timeline {
                if let Some(base) = timeline.get(0) {
                    self.frames[0] = base.clone();
                }
                self.timeline = Some(timeline);
                self.prepared = true;
            }
            self.cursor = staged.target;
            self.committed_end = self.committed_end.max(staged.target);
        }
    }

    fn discard_staged_next(&mut self) {
        self.staged_next = None;
    }

    fn commit_staged_seek(&mut self) {
        if let Some(target) = self.staged_seek.take() {
            self.cursor = target;
            self.committed_end = self.committed_end.max(target);
        }
    }

    fn discard_staged_seek(&mut self) {
        self.staged_seek = None;
    }
}

#[derive(Clone, Copy)]
enum CapturedFeasibilityPlacement {
    InitialFlow,
    PrecheckOnly,
    BeforeOccurrence {
        catalog_id: &'static str,
        occurrence: u64,
    },
}

#[derive(Clone, Copy)]
enum CapturedFeasibilityProjection {
    PublicFlow,
    OverlayOnly {
        public_entities: bool,
        use_kind: flow::FlowFeasibilityUseV1,
    },
}

fn captured_feasibility_placement(
    source_graph: &flow::FlowNetwork,
    captured: &flow::feasibility::CapturedFeasibilityTrace,
) -> Result<CapturedFeasibilityPlacement, JsError> {
    match &captured.use_kind {
        flow::feasibility::FeasibilityUse::InitialFlow => {
            if captured.graph != *source_graph {
                return Err(JsError::new(
                    "initial-flow feasibility must use the public source graph",
                ));
            }
            Ok(CapturedFeasibilityPlacement::InitialFlow)
        }
        flow::feasibility::FeasibilityUse::PrecheckOnly => {
            if captured.graph != *source_graph {
                return Err(JsError::new(
                    "feasibility precheck must use the public source graph",
                ));
            }
            Ok(CapturedFeasibilityPlacement::PrecheckOnly)
        }
        flow::feasibility::FeasibilityUse::BeforeEvent { anchor } => {
            if anchor.occurrence == 0 {
                return Err(JsError::new(
                    "feasibility source anchor occurrence must be one-based",
                ));
            }
            Ok(CapturedFeasibilityPlacement::BeforeOccurrence {
                catalog_id: anchor.catalog_id,
                occurrence: anchor.occurrence,
            })
        }
    }
}

fn append_captured_feasibility_frames(
    output: &mut Vec<FlowCurrentSceneV9>,
    base_scene: &FlowCurrentSceneV9,
    public_graph: &flow::FlowNetwork,
    captured: &flow::feasibility::CapturedFeasibilityTrace,
    projection: CapturedFeasibilityProjection,
) -> Result<FlowCurrentSceneV9, JsError> {
    flow::feasibility::check_captured_feasibility_trace(captured)
        .map_err(|error| JsError::new(&error.to_string()))?;
    let mut replay = captured.result.trace.base_snapshot.clone();
    let mut final_scene = base_scene.clone();
    for event in &captured.result.trace.events {
        flow::feasibility::apply_feasibility_trace_event(
            &mut replay,
            event,
            flow::feasibility::FeasibilityTraceDirection::Forward,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
        let event_id = u64::try_from(output.len())
            .map_err(|_| JsError::new("feasibility event identity overflow"))?;
        let mut scene = base_scene.clone();
        match projection {
            CapturedFeasibilityProjection::PublicFlow => {
                scene
                    .apply_feasibility_trace_snapshot(
                        &captured.graph,
                        &captured.request,
                        &replay,
                        Some(event),
                        event_id,
                        0,
                    )
                    .map_err(|error| JsError::new(&error.to_string()))?;
                scene.metrics.clone_from(&base_scene.metrics);
            }
            CapturedFeasibilityProjection::OverlayOnly { use_kind, .. } => {
                scene
                    .apply_auxiliary_feasibility_trace_snapshot(
                        &flow::FlowAuxiliaryFeasibilityProjection {
                            public_graph,
                            kernel_graph: &captured.graph,
                            request: &captured.request,
                            snapshot: &replay,
                            event: Some(event),
                            event_id,
                            event_count: 0,
                            use_kind,
                        },
                    )
                    .map_err(|error| JsError::new(&error.to_string()))?;
                if !same_public_flow_state(&scene, base_scene) {
                    return Err(JsError::new(
                        "overlay-only feasibility changed public flow state",
                    ));
                }
            }
        }
        let public_entities = matches!(
            projection,
            CapturedFeasibilityProjection::PublicFlow
                | CapturedFeasibilityProjection::OverlayOnly {
                    public_entities: true,
                    ..
                }
        );
        scene.trace_event = Some(feasibility_trace_event_scene(
            &replay,
            event,
            event_id,
            public_entities,
        )?);
        scene.trace_event_semantics = None;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.outcome = None;
        final_scene = scene.clone();
        output.push(scene);
    }
    if replay != captured.result.trace.final_snapshot || captured.result.trace.events.is_empty() {
        return Err(JsError::new(
            "captured feasibility trace final snapshot mismatch",
        ));
    }
    Ok(final_scene)
}

fn same_public_flow_state(left: &FlowCurrentSceneV9, right: &FlowCurrentSceneV9) -> bool {
    left.edge_states == right.edge_states && left.residual_arcs == right.residual_arcs
}

/// Builds the internal source boundary used to join an explicitly traced
/// feasibility prefix to an algorithm trace. Only original flow state may be
/// carried across that join; labels, overlays, counters, and other initialized
/// algorithm state belong to the first source event.
fn flow_only_source_base(
    mut ready: FlowCurrentSceneV9,
    projected: &FlowCurrentSceneV9,
    event_count: u64,
) -> FlowCurrentSceneV9 {
    ready.edge_states.clone_from(&projected.edge_states);
    ready.residual_arcs.clone_from(&projected.residual_arcs);
    ready.event_count = event_count.to_string();
    ready
}

/// Builds a neutral public-graph underlay for an algorithm-owned auxiliary
/// feasibility trace. The recovery keeps the committed public flow and work
/// counters, but it must not inherit stale node annotations or a different
/// algorithm overlay from the preceding boundary.
fn auxiliary_feasibility_underlay(
    public_ready: &FlowCurrentSceneV9,
    algorithm_state: &FlowCurrentSceneV9,
) -> Result<FlowCurrentSceneV9, JsError> {
    let public_execution_identity = serde_json::to_value((
        &public_ready.model,
        &public_ready.graph.nodes,
        &public_ready.algorithm,
        &public_ready.run_profile,
        &public_ready.trace_granularity,
    ))
    .map_err(|error| JsError::new(&error.to_string()))?;
    let algorithm_execution_identity = serde_json::to_value((
        &algorithm_state.model,
        &algorithm_state.graph.nodes,
        &algorithm_state.algorithm,
        &algorithm_state.run_profile,
        &algorithm_state.trace_granularity,
    ))
    .map_err(|error| JsError::new(&error.to_string()))?;
    let public_edge_identities = public_ready
        .graph
        .edges
        .iter()
        .map(|edge| (&edge.id, &edge.from, &edge.to))
        .collect::<Vec<_>>();
    let algorithm_edge_identities = algorithm_state
        .graph
        .edges
        .iter()
        .map(|edge| (&edge.id, &edge.from, &edge.to))
        .collect::<Vec<_>>();
    if public_execution_identity != algorithm_execution_identity
        || public_edge_identities != algorithm_edge_identities
    {
        return Err(JsError::new(
            "auxiliary feasibility underlay does not match its public execution",
        ));
    }
    let mut underlay = public_ready.clone();
    underlay
        .edge_states
        .clone_from(&algorithm_state.edge_states);
    underlay
        .residual_arcs
        .clone_from(&algorithm_state.residual_arcs);
    underlay.metrics.clone_from(&algorithm_state.metrics);
    underlay
        .event_count
        .clone_from(&algorithm_state.event_count);
    if !same_public_flow_state(&underlay, algorithm_state) {
        return Err(JsError::new(
            "auxiliary feasibility underlay could not preserve public flow state",
        ));
    }
    Ok(underlay)
}

fn trace_snapshot_source_base(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    snapshot: &FlowTraceSnapshot,
    event_count: u64,
) -> Result<FlowCurrentSceneV9, JsError> {
    let ready = ready_flow_scene(scenario)?;
    let mut projected = ready.clone();
    projected
        .apply_trace_snapshot(graph, snapshot, None, event_count)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(flow_only_source_base(ready, &projected, event_count))
}

fn validate_initial_flow_is_the_only_source_base_change(
    public_ready: &FlowCurrentSceneV9,
    source_base: &FlowCurrentSceneV9,
) -> Result<(), JsError> {
    let mut without_initial_flow = source_base.clone();
    without_initial_flow
        .edge_states
        .clone_from(&public_ready.edge_states);
    without_initial_flow
        .residual_arcs
        .clone_from(&public_ready.residual_arcs);
    validate_public_ready_source_base(public_ready, &without_initial_flow).map_err(JsError::new)
}

#[expect(
    clippy::too_many_lines,
    reason = "prefix validation, exact source anchoring, and terminal publication form one ordered transaction"
)]
fn compose_captured_feasibility_traces(
    public_ready: &FlowCurrentSceneV9,
    source_graph: &flow::FlowNetwork,
    frames: Vec<FlowCurrentSceneV9>,
    captured: Vec<flow::feasibility::CapturedFeasibilityTrace>,
    captured_metrics: flow::feasibility::FeasibilityMetricSummary,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    if !captured.is_empty() && captured_metrics.invocations != 0 {
        return Err(JsError::new(
            "feasibility execution cannot retain traces and metric-only records together",
        ));
    }
    if captured_metrics.invocations == 0
        && captured_metrics.total != flow::feasibility::FeasibilityTraceMetrics::default()
    {
        return Err(JsError::new(
            "zero feasibility invocations cannot carry aggregate work",
        ));
    }
    if captured_metrics.invocations != 0
        && captured_metrics.total == flow::feasibility::FeasibilityTraceMetrics::default()
    {
        return Err(JsError::new(
            "feasibility invocation cannot report zero source work",
        ));
    }
    if captured_metrics.invocations != 0 && !matches!(public_ready.run_profile, RunProfileV1::Fast)
    {
        return Err(JsError::new(
            "metric-only feasibility work requires the fast run profile",
        ));
    }
    let source_base = frames
        .first()
        .ok_or_else(|| JsError::new("flow execution produced no source base frame"))?
        .clone();
    let mut placements = captured
        .into_iter()
        .map(|captured| {
            captured_feasibility_placement(source_graph, &captured)
                .map(|placement| (placement, captured))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut output = vec![public_ready.clone()];
    let mut prefix_underlay = public_ready.clone();
    let mut prefix_infeasible = false;
    let mut has_initial_flow = false;
    let prefix_count = placements
        .iter()
        .take_while(|(placement, _)| {
            matches!(
                placement,
                CapturedFeasibilityPlacement::InitialFlow
                    | CapturedFeasibilityPlacement::PrecheckOnly
            )
        })
        .count();
    if placements[prefix_count..].iter().any(|(placement, _)| {
        matches!(
            placement,
            CapturedFeasibilityPlacement::InitialFlow | CapturedFeasibilityPlacement::PrecheckOnly
        )
    }) {
        return Err(JsError::new(
            "source feasibility prefix appeared after an anchored recovery",
        ));
    }
    for (prefix_index, (placement, captured)) in placements.drain(..prefix_count).enumerate() {
        let captured_infeasible = matches!(
            &captured.result.outcome,
            flow::feasibility::FeasibilityTraceOutcome::Infeasible(_)
        );
        if captured_infeasible && prefix_index + 1 != prefix_count {
            return Err(JsError::new(
                "an infeasible feasibility call was followed by source execution",
            ));
        }
        prefix_underlay.feasibility_overlay = None;
        prefix_underlay.trace_event = None;
        prefix_underlay.trace_event_semantics = None;
        let projection = match placement {
            CapturedFeasibilityPlacement::InitialFlow => {
                has_initial_flow = true;
                CapturedFeasibilityProjection::PublicFlow
            }
            CapturedFeasibilityPlacement::PrecheckOnly => {
                CapturedFeasibilityProjection::OverlayOnly {
                    public_entities: true,
                    use_kind: flow::FlowFeasibilityUseV1::PrecheckOnly,
                }
            }
            CapturedFeasibilityPlacement::BeforeOccurrence { .. } => {
                return Err(JsError::new("anchored feasibility appeared in the prefix"));
            }
        };
        let projected = append_captured_feasibility_frames(
            &mut output,
            &prefix_underlay,
            source_graph,
            &captured,
            projection,
        )?;
        if matches!(projection, CapturedFeasibilityProjection::PublicFlow) {
            prefix_underlay = projected;
        }
        prefix_infeasible = captured_infeasible;
    }
    if !prefix_infeasible && !same_public_flow_state(&prefix_underlay, &source_base) {
        return Err(JsError::new(
            "declared feasibility prefix does not produce the algorithm source flow base",
        ));
    }
    if has_initial_flow {
        validate_initial_flow_is_the_only_source_base_change(public_ready, &source_base)?;
    } else {
        validate_public_ready_source_base(public_ready, &source_base).map_err(JsError::new)?;
    }

    let mut frames = frames;
    if prefix_infeasible {
        if frames.len() != 2
            || frames[1].trace_event.is_some()
            || frames[1].solve_status != FlowSolveStatusV1::Infeasible
            || frames[1].outcome.is_none()
        {
            return Err(JsError::new(
                "captured infeasibility did not terminate in one certified source frame",
            ));
        }
        let final_feasibility = output
            .last_mut()
            .ok_or_else(|| JsError::new("captured infeasibility produced no frame"))?;
        if final_feasibility
            .trace_event
            .as_ref()
            .is_none_or(|event| event.catalog_id != "feasibility.infeasible")
        {
            return Err(JsError::new(
                "captured infeasibility omitted its terminal source event",
            ));
        }
        final_feasibility.solve_status = FlowSolveStatusV1::Infeasible;
        final_feasibility.outcome = frames[1].outcome.take();
        // The shared feasibility event already owns the mathematical terminal
        // transition. Keeping the handler's metadata-free duplicate would add
        // one unnamed, visually redundant step.
        frames.truncate(1);
    }

    let mut previous_algorithm_state = source_base.clone();
    let mut source_occurrences = BTreeMap::<String, u64>::new();
    for mut frame in frames.into_iter().skip(1) {
        let catalog_id = frame
            .trace_event
            .as_ref()
            .map(|event| event.catalog_id.clone());
        let occurrence = if let Some(catalog_id) = &catalog_id {
            let count = source_occurrences.entry(catalog_id.clone()).or_default();
            *count = count
                .checked_add(1)
                .ok_or_else(|| JsError::new("source event occurrence overflow"))?;
            Some(*count)
        } else {
            None
        };
        while placements.first().is_some_and(|(placement, _)| {
            matches!(
                placement,
                CapturedFeasibilityPlacement::BeforeOccurrence {
                    catalog_id: anchor,
                    occurrence: anchored_occurrence,
                } if Some(*anchor) == catalog_id.as_deref() && Some(*anchored_occurrence) == occurrence
            )
        }) {
            let (_, captured) = placements.remove(0);
            let recovery_underlay =
                auxiliary_feasibility_underlay(public_ready, &previous_algorithm_state)?;
            previous_algorithm_state = append_captured_feasibility_frames(
                &mut output,
                &recovery_underlay,
                source_graph,
                &captured,
                CapturedFeasibilityProjection::OverlayOnly {
                    public_entities: false,
                    use_kind: flow::FlowFeasibilityUseV1::AnchoredRecovery,
                },
            )?;
        }
        frame.set_feasibility_work(captured_metrics);
        previous_algorithm_state = frame.clone();
        output.push(frame);
    }
    if !placements.is_empty() {
        return Err(JsError::new(
            "captured feasibility recovery anchor was not published",
        ));
    }
    Ok(output)
}

/// The frame published by `FlowSession::new` is the only legal timeline base.
/// Solvers must publish every renderer-visible initialization as an owned trace
/// event instead of replacing Ready with a hidden precomputed state.
fn validate_public_ready_source_base(
    public_ready: &FlowCurrentSceneV9,
    source_base: &FlowCurrentSceneV9,
) -> Result<(), &'static str> {
    if source_base.trace_event.is_some() || source_base.trace_event_semantics.is_some() {
        return Err("flow source timeline base must not contain trace metadata");
    }
    let mut expected = public_ready.clone();
    let mut actual = source_base.clone();
    "0".clone_into(&mut expected.event_id);
    "0".clone_into(&mut expected.event_count);
    "0".clone_into(&mut actual.event_id);
    "0".clone_into(&mut actual.event_count);
    let expected = serde_json::to_value(expected)
        .map_err(|_| "failed to serialize published Ready state for comparison")?;
    let actual = serde_json::to_value(actual)
        .map_err(|_| "failed to serialize source base for comparison")?;
    if expected != actual {
        return Err(
            "flow source timeline base diverges from the published Ready state; initialization must be an explicit source event",
        );
    }
    Ok(())
}

fn seek_flow_json(
    session: &mut FlowSession,
    target: usize,
    frame_json_limit: usize,
) -> Result<String, JsError> {
    session.begin_seek(target)?;
    publish_staged_flow_seek_json(session, frame_json_limit).map_err(|error| JsError::new(&error))
}

fn publish_staged_flow_seek_json(
    session: &mut FlowSession,
    frame_json_limit: usize,
) -> Result<String, String> {
    let result = session
        .staged_seek
        .and_then(|cursor| session.cached_frame(cursor))
        .ok_or_else(|| "flow seek candidate is missing".to_owned())
        .and_then(|frame| serialize_frame_with_limit(frame, frame_json_limit));
    match result {
        Ok(frame) => {
            session.commit_staged_seek();
            Ok(frame)
        }
        Err(error) => {
            session.discard_staged_seek();
            Err(error)
        }
    }
}

fn solve_dinic_preset(
    preset: DinicRunner,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    feasibility: &mut flow::feasibility::FeasibilityExecution,
) -> Result<flow::DinicResult, DinicError> {
    let preset = match preset {
        DinicRunner::General => flow::DinicExecutionPreset::General,
        DinicRunner::UnitCapacity => flow::DinicExecutionPreset::UnitCapacity,
        DinicRunner::UnitNetwork => flow::DinicExecutionPreset::UnitNetwork,
    };
    flow::solve_dinic_preset_with_feasibility(graph, source, sink, preset, feasibility)
}

fn solve_ford_fulkerson_preset(
    preset: FordFulkersonRunner,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    feasibility: &mut flow::feasibility::FeasibilityExecution,
) -> Result<flow::FordFulkersonResult, FordFulkersonError> {
    let preset = match preset {
        FordFulkersonRunner::FordFulkerson => flow::FordFulkersonExecutionPreset::General,
        FordFulkersonRunner::DepthFirst => flow::FordFulkersonExecutionPreset::Dfs,
        FordFulkersonRunner::WidestPath => flow::FordFulkersonExecutionPreset::Widest,
        FordFulkersonRunner::CapacityScaling => flow::FordFulkersonExecutionPreset::CapacityScaling,
    };
    flow::solve_ford_fulkerson_preset_with_feasibility(graph, source, sink, preset, feasibility)
}

fn trace_ford_fulkerson_preset(
    preset: FordFulkersonRunner,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    feasibility: &mut flow::feasibility::FeasibilityExecution,
) -> Result<flow::FordFulkersonTraceResult, FordFulkersonError> {
    let preset = match preset {
        FordFulkersonRunner::FordFulkerson => flow::FordFulkersonExecutionPreset::General,
        FordFulkersonRunner::DepthFirst => flow::FordFulkersonExecutionPreset::Dfs,
        FordFulkersonRunner::WidestPath => flow::FordFulkersonExecutionPreset::Widest,
        FordFulkersonRunner::CapacityScaling => flow::FordFulkersonExecutionPreset::CapacityScaling,
    };
    flow::trace_ford_fulkerson_preset_with_feasibility(graph, source, sink, preset, feasibility)
}

fn apply_ford_fulkerson_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::FordFulkersonResult,
) -> Result<(), JsError> {
    scene
        .apply_augmenting_path_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowAugmentingPathMetrics {
                path_searches: result.metrics.path_searches,
                scaling_phases: result.metrics.scaling_phases,
                residual_arc_scans: result.metrics.residual_arc_scans,
                augmentations: result.metrics.augmentations,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn apply_ibfs_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::IbfsResult,
) -> Result<(), JsError> {
    scene
        .apply_ibfs_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowIbfsMetrics {
                passes: result.metrics.passes,
                forward_passes: result.metrics.forward_passes,
                reverse_passes: result.metrics.reverse_passes,
                residual_arc_scans: result.metrics.residual_arc_scans,
                adoption_arc_scans: result.metrics.adoption_arc_scans,
                tree_attachments: result.metrics.tree_attachments,
                augmentations: result.metrics.augmentations,
                augmented_path_arcs: result.metrics.augmented_path_arcs,
                saturated_tree_arcs: result.metrics.saturated_tree_arcs,
                orphan_creations: result.metrics.orphan_creations,
                orphan_visits: result.metrics.orphan_visits,
                same_level_adoptions: result.metrics.same_level_adoptions,
                orphan_relabels: result.metrics.orphan_relabels,
                tree_removals: result.metrics.tree_removals,
                active_vertex_scans: result.metrics.active_vertex_scans,
                state_transitions: result.metrics.state_transitions,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn apply_boykov_kolmogorov_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::BoykovKolmogorovResult,
) -> Result<(), JsError> {
    scene
        .apply_ibfs_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowIbfsMetrics {
                passes: result.metrics.active_visits,
                forward_passes: result.metrics.passive_vertices,
                reverse_passes: result.metrics.tree_attachments,
                residual_arc_scans: result
                    .metrics
                    .growth_arc_scans
                    .saturating_add(result.metrics.adoption_arc_scans),
                adoption_arc_scans: result.metrics.adoption_arc_scans,
                tree_attachments: result.metrics.orphan_creations,
                augmentations: result.metrics.augmentations,
                augmented_path_arcs: result.metrics.augmented_path_arcs,
                saturated_tree_arcs: result.metrics.orphan_creations,
                orphan_creations: result.metrics.orphan_creations,
                orphan_visits: result.metrics.adoptions,
                same_level_adoptions: result.metrics.adoptions,
                orphan_relabels: result.metrics.tree_removals,
                tree_removals: result.metrics.orphan_visits,
                active_vertex_scans: result.metrics.reactivations,
                state_transitions: result.metrics.state_transitions,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn apply_eibfs_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::EibfsResult,
) -> Result<(), JsError> {
    scene
        .apply_eibfs_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowEibfsMetrics {
                phases: result.metrics.phases,
                forward_phases: result.metrics.forward_phases,
                reverse_phases: result.metrics.reverse_phases,
                residual_arc_scans: result.metrics.residual_arc_scans,
                adoption_arc_scans: result.metrics.adoption_arc_scans,
                tree_attachments: result.metrics.tree_attachments,
                bridge_pushes: result.metrics.bridge_pushes,
                tree_path_pushes: result.metrics.tree_path_pushes,
                saturated_tree_arcs: result.metrics.saturated_tree_arcs,
                orphan_creations: result.metrics.orphan_creations,
                orphan_visits: result.metrics.orphan_visits,
                orphan_relabels: result.metrics.orphan_relabels,
                tree_removals: result.metrics.tree_removals,
                side_migrations: result.metrics.side_migrations,
                recovery_cancellations: result.metrics.recovery_cancellations,
                state_transitions: result.metrics.state_transitions,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn apply_dinic_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::DinicResult,
) -> Result<(), JsError> {
    scene
        .apply_blocking_flow_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowBlockingFlowMetrics {
                bfs_runs: result.metrics.bfs_runs,
                residual_arc_scans: result.metrics.residual_arc_scans,
                augmentations: result.metrics.augmentations,
                blocking_flow_phases: result.metrics.blocking_flow_phases,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn apply_dynamic_tree_blocking_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::DynamicTreeBlockingResult,
) -> Result<(), JsError> {
    scene
        .apply_dynamic_tree_blocking_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowDynamicTreeBlockingMetrics {
                bfs_runs: result.metrics.bfs_runs,
                residual_arc_scans: result.metrics.residual_arc_scans,
                augmentations: result.metrics.augmentations,
                blocking_flow_phases: result.metrics.blocking_flow_phases,
                path_minimum_queries: result.metrics.path_minimum_queries,
                path_updates: result.metrics.path_updates,
                tree_links: result.metrics.tree_links,
                tree_cuts: result.metrics.tree_cuts,
                dead_end_prunes: result.metrics.dead_end_prunes,
                state_transitions: result.metrics.state_transitions,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn apply_goldberg_rao_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::GoldbergRaoResult,
) -> Result<(), JsError> {
    scene
        .apply_goldberg_rao_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowGoldbergRaoMetrics {
                distance_searches: result.metrics.distance_searches,
                phases: result.metrics.phases,
                residual_arc_scans: result.metrics.residual_arc_scans,
                update_steps: result.metrics.update_steps,
                canonical_cut_evaluations: result.metrics.canonical_cut_evaluations,
                blocking_updates: result.metrics.blocking_updates,
                zero_length_arc_observations: result.metrics.zero_length_arc_observations,
                special_arc_observations: result.metrics.special_arc_observations,
                nontrivial_contractions: result.metrics.nontrivial_contractions,
                cut_updates: result.metrics.cut_updates,
                contracted_augmentations: result.metrics.contracted_augmentations,
                delta_limited_updates: result.metrics.delta_limited_updates,
                component_routing_paths: result.metrics.component_routing_paths,
                augmented_units: result.metrics.augmented_units,
                state_transitions: result.metrics.state_transitions,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn apply_dynamic_tree_push_relabel_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::DynamicTreePushRelabelResult,
) -> Result<(), JsError> {
    scene
        .apply_dynamic_tree_push_relabel_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowDynamicTreePushRelabelMetrics {
                source_pushes: result.metrics.source_pushes,
                tree_size_limit: result.metrics.tree_size_limit,
                residual_arc_scans: result.metrics.residual_arc_scans,
                tree_path_sends: result.metrics.tree_path_sends,
                component_size_queries: result.metrics.component_size_queries,
                tree_links: result.metrics.tree_links,
                tree_cuts: result.metrics.tree_cuts,
                relabels: result.metrics.relabels,
                final_tree_materializations: result.metrics.final_tree_materializations,
                queue_additions: result.metrics.queue_additions,
                size_gate_rejections: result.metrics.size_gate_rejections,
                pushes: result.metrics.pushes,
                saturating_pushes: result.metrics.saturating_pushes,
                nonsaturating_pushes: result.metrics.nonsaturating_pushes,
                discharges: result.metrics.discharges,
                active_vertex_selections: result.metrics.active_vertex_selections,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn solve_sap_preset(
    preset: SapRunner,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    feasibility: &mut flow::feasibility::FeasibilityExecution,
) -> Result<flow::SapResult, SapError> {
    let preset = match preset {
        SapRunner::ShortestAugmentingPath => flow::SapExecutionPreset::Plain,
        SapRunner::Improved => flow::SapExecutionPreset::Improved,
    };
    flow::solve_sap_preset_with_feasibility(graph, source, sink, preset, feasibility)
}

fn trace_sap_preset(
    preset: SapRunner,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    feasibility: &mut flow::feasibility::FeasibilityExecution,
) -> Result<flow::SapTraceResult, SapError> {
    let preset = match preset {
        SapRunner::ShortestAugmentingPath => flow::SapExecutionPreset::Plain,
        SapRunner::Improved => flow::SapExecutionPreset::Improved,
    };
    flow::trace_sap_preset_with_feasibility(graph, source, sink, preset, feasibility)
}

fn solve_distance_directed_preset(
    preset: DistanceDirectedRunner,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
) -> Result<flow::DistanceDirectedResult, DistanceDirectedError> {
    match preset {
        DistanceDirectedRunner::ExactTree => solve_distance_directed_dd2(graph, source, sink),
        DistanceDirectedRunner::CapacityScaling => {
            solve_distance_directed_scaling(graph, source, sink)
        }
    }
}

fn trace_distance_directed_preset(
    preset: DistanceDirectedRunner,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
) -> Result<flow::DistanceDirectedTraceResult, DistanceDirectedError> {
    match preset {
        DistanceDirectedRunner::ExactTree => trace_distance_directed_dd2(graph, source, sink),
        DistanceDirectedRunner::CapacityScaling => {
            trace_distance_directed_scaling(graph, source, sink)
        }
    }
}

fn apply_sap_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::SapResult,
) -> Result<(), JsError> {
    scene
        .apply_distance_label_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowDistanceLabelMetrics {
                residual_arc_scans: result.metrics.residual_arc_scans,
                augmentations: result.metrics.augmentations,
                relabels: result.metrics.relabels,
                retreats: result.metrics.retreats,
                reverse_bfs_runs: result.metrics.reverse_bfs_runs,
                gap_terminations: result.metrics.gap_terminations,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn apply_distance_directed_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::DistanceDirectedResult,
) -> Result<(), JsError> {
    scene
        .apply_distance_directed_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowDistanceDirectedMetrics {
                reverse_bfs_runs: result.metrics.reverse_bfs_runs,
                scaling_phases: result.metrics.scaling_phases,
                residual_arc_scans: result.metrics.residual_arc_scans,
                augmentations: result.metrics.augmentations,
                tree_repairs: result.metrics.tree_repairs,
                invalid_tree_arcs: result.metrics.invalid_tree_arcs,
                tree_arc_replacements: result.metrics.tree_arc_replacements,
                relabels: result.metrics.relabels,
                node_deletions: result.metrics.node_deletions,
                saturated_tree_arcs: result.metrics.saturated_tree_arcs,
                cascading_invalidations: result.metrics.cascading_invalidations,
                current_arc_advances: result.metrics.current_arc_advances,
                state_transitions: result.metrics.state_transitions,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn solve_blocking_preflow_preset(
    preset: BlockingPreflowRunner,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    feasibility: &mut flow::feasibility::FeasibilityExecution,
) -> Result<flow::BlockingPreflowResult, BlockingPreflowError> {
    let preset = match preset {
        BlockingPreflowRunner::Karzanov => flow::BlockingPreflowExecutionPreset::Karzanov,
        BlockingPreflowRunner::Mpm => flow::BlockingPreflowExecutionPreset::Mpm,
    };
    flow::solve_blocking_preflow_preset_with_feasibility(graph, source, sink, preset, feasibility)
}

fn trace_blocking_preflow_preset(
    preset: BlockingPreflowRunner,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    feasibility: &mut flow::feasibility::FeasibilityExecution,
) -> Result<flow::BlockingPreflowTraceResult, BlockingPreflowError> {
    let preset = match preset {
        BlockingPreflowRunner::Karzanov => flow::BlockingPreflowExecutionPreset::Karzanov,
        BlockingPreflowRunner::Mpm => flow::BlockingPreflowExecutionPreset::Mpm,
    };
    flow::trace_blocking_preflow_preset_with_feasibility(graph, source, sink, preset, feasibility)
}

fn apply_blocking_preflow_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::BlockingPreflowResult,
) -> Result<(), JsError> {
    scene
        .apply_blocking_preflow_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowBlockingPreflowMetrics {
                bfs_runs: result.metrics.bfs_runs,
                residual_arc_scans: result.metrics.residual_arc_scans,
                blocking_flow_phases: result.metrics.blocking_flow_phases,
                pushes: result.metrics.pushes,
                saturating_pushes: result.metrics.saturating_pushes,
                nonsaturating_pushes: result.metrics.nonsaturating_pushes,
                balancing_iterations: result.metrics.balancing_iterations,
                vertex_eliminations: result.metrics.vertex_eliminations,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn apply_pseudoflow_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::PseudoflowResult,
) -> Result<(), JsError> {
    scene
        .apply_pseudoflow_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowPseudoflowMetrics {
                residual_arc_scans: result.metrics.residual_arc_scans,
                mergers: result.metrics.mergers,
                relabels: result.metrics.relabels,
                pushes: result
                    .metrics
                    .normalization_pushes
                    .checked_add(result.metrics.recovery_arc_pushes)
                    .ok_or_else(|| JsError::new("pseudoflow metric overflow"))?,
                saturating_pushes: result.metrics.saturating_pushes,
                nonsaturating_pushes: result.metrics.nonsaturating_pushes,
                recovery_paths: result.metrics.recovery_paths,
                pivot_cycle_arcs: 0,
                internal_leaves: 0,
                entering_leaves: 0,
                strong_root_leaves: 0,
                weak_root_leaves: 0,
                degenerate_pivots: 0,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn apply_pseudoflow_simplex_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::PseudoflowSimplexResult,
) -> Result<(), JsError> {
    scene
        .apply_pseudoflow_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowPseudoflowMetrics {
                residual_arc_scans: result.metrics.residual_arc_scans,
                mergers: result.metrics.pivots,
                relabels: result.metrics.relabels,
                pushes: result
                    .metrics
                    .pivot_arc_pushes
                    .checked_add(result.metrics.recovery_arc_pushes)
                    .ok_or_else(|| JsError::new("pseudoflow-simplex metric overflow"))?,
                saturating_pushes: result.metrics.saturating_pushes,
                nonsaturating_pushes: result.metrics.nonsaturating_pushes,
                recovery_paths: result.metrics.recovery_paths,
                pivot_cycle_arcs: result.metrics.pivot_cycle_arcs,
                internal_leaves: result.metrics.internal_leaves,
                entering_leaves: result.metrics.entering_leaves,
                strong_root_leaves: result.metrics.strong_root_leaves,
                weak_root_leaves: result.metrics.weak_root_leaves,
                degenerate_pivots: result.metrics.degenerate_pivots,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn solve_push_relabel_preset(
    preset: PushRelabelRunner,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    feasibility: &mut flow::feasibility::FeasibilityExecution,
) -> Result<flow::PushRelabelResult, PushRelabelError> {
    let preset = push_relabel_execution_preset(preset);
    flow::solve_push_relabel_preset_with_feasibility(graph, source, sink, preset, feasibility)
}

fn trace_push_relabel_preset(
    preset: PushRelabelRunner,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    feasibility: &mut flow::feasibility::FeasibilityExecution,
) -> Result<flow::PushRelabelTraceResult, PushRelabelError> {
    let preset = push_relabel_execution_preset(preset);
    flow::trace_push_relabel_preset_with_feasibility(graph, source, sink, preset, feasibility)
}

const fn push_relabel_execution_preset(
    preset: PushRelabelRunner,
) -> flow::PushRelabelExecutionPreset {
    match preset {
        PushRelabelRunner::Generic => flow::PushRelabelExecutionPreset::Generic,
        PushRelabelRunner::CurrentArc => flow::PushRelabelExecutionPreset::CurrentArc,
        PushRelabelRunner::Fifo => flow::PushRelabelExecutionPreset::Fifo,
        PushRelabelRunner::RelabelToFront => flow::PushRelabelExecutionPreset::RelabelToFront,
        PushRelabelRunner::HighestLabel => flow::PushRelabelExecutionPreset::HighestLabel,
        PushRelabelRunner::PartialAugmentRelabel => {
            flow::PushRelabelExecutionPreset::PartialAugmentRelabel
        }
        PushRelabelRunner::GlobalRelabel => flow::PushRelabelExecutionPreset::GlobalRelabel,
        PushRelabelRunner::GapRelabel => flow::PushRelabelExecutionPreset::GapRelabel,
        PushRelabelRunner::ExcessScaling => flow::PushRelabelExecutionPreset::ExcessScaling,
    }
}

fn apply_push_relabel_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::PushRelabelResult,
) -> Result<(), JsError> {
    scene
        .apply_push_relabel_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowPushRelabelMetrics {
                residual_arc_scans: result.metrics.residual_arc_scans,
                relabels: result.metrics.relabels,
                pushes: result.metrics.pushes,
                saturating_pushes: result.metrics.saturating_pushes,
                nonsaturating_pushes: result.metrics.nonsaturating_pushes,
                discharges: result.metrics.discharges,
                active_vertex_selections: result.metrics.active_vertex_selections,
                augmentations: result.metrics.augmentations,
                path_searches: result.metrics.path_searches,
                retreats: result.metrics.retreats,
                global_relabels: result.metrics.global_relabels,
                gap_relabels: result.metrics.gap_relabels,
                scaling_phases: result.metrics.scaling_phases,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn apply_synchronous_push_relabel_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::SynchronousPushRelabelResult,
) -> Result<(), JsError> {
    scene
        .apply_push_relabel_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowPushRelabelMetrics {
                residual_arc_scans: result.metrics.residual_arc_scans,
                relabels: result.metrics.relabels,
                pushes: result.metrics.pushes,
                saturating_pushes: 0,
                nonsaturating_pushes: 0,
                discharges: result.metrics.active_vertex_visits,
                active_vertex_selections: result.metrics.active_vertex_visits,
                augmentations: result.metrics.recovery_paths,
                path_searches: result.metrics.recovery_paths,
                retreats: result.metrics.ownership_conflicts,
                global_relabels: result.metrics.global_relabels,
                gap_relabels: 0,
                scaling_phases: 0,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.metrics[0] = result.metrics.global_relabels.to_string();
    scene.metrics[1] = result.metrics.rounds.to_string();
    scene.metrics[6] = result.metrics.rounds.to_string();
    Ok(())
}

fn apply_warm_start_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::WarmStartPushRelabelResult,
) -> Result<(), JsError> {
    scene
        .apply_warm_start_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowWarmStartMetrics {
                auxiliary_solves: result.metrics.auxiliary_solves,
                eta: result.metrics.eta,
                residual_arc_scans: result.metrics.residual_arc_scans,
                recovery_paths: result.metrics.recovery_paths,
                cut_saturation_error: result.metrics.cut_saturation_error,
                imbalance_error: result.metrics.imbalance_error,
                relabels: result.metrics.relabels,
                cut_transfers: result.metrics.cut_transfers,
                predicted_positive_edges: result.metrics.predicted_positive_edges,
                gap_relabels: result.metrics.gap_relabels,
                pushes: result.metrics.pushes,
                saturating_pushes: result.metrics.saturating_pushes,
                nonsaturating_pushes: result.metrics.nonsaturating_pushes,
                discharges: result.metrics.discharges,
                active_vertex_selections: result.metrics.active_vertex_selections,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn trace_dinic_preset(
    preset: DinicRunner,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    feasibility: &mut flow::feasibility::FeasibilityExecution,
) -> Result<flow::DinicTraceResult, DinicError> {
    let preset = match preset {
        DinicRunner::General => flow::DinicExecutionPreset::General,
        DinicRunner::UnitCapacity => flow::DinicExecutionPreset::UnitCapacity,
        DinicRunner::UnitNetwork => flow::DinicExecutionPreset::UnitNetwork,
    };
    flow::trace_dinic_preset_with_feasibility(graph, source, sink, preset, feasibility)
}

fn validate_catalog_graph_contract(
    scenario: &FlowScenarioV1,
    descriptor: &flow::AlgorithmDescriptor,
    graph: &flow::FlowNetwork,
) -> Result<(), &'static str> {
    for &requirement in descriptor.graph_requirements {
        if !catalog_graph_requirement_is_satisfied(scenario, graph, requirement) {
            return Err(catalog_graph_requirement_error(requirement));
        }
    }
    validate_catalog_negative_cycle_contract(descriptor, graph)?;
    if descriptor.admission_contract.strict_interior_required {
        if !catalog_graph_is_inside_precondition_validation_margin(descriptor, graph) {
            return Err(
                "strict-interior precondition cannot be certified beyond the supported validation margin",
            );
        }
        if !catalog_graph_has_strict_interior(scenario, graph) {
            return Err(
                "selected flow algorithm requires a feasible flow strictly inside every edge bound",
            );
        }
    }
    Ok(())
}

fn catalog_graph_requirement_is_satisfied(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    requirement: flow::GraphRequirement,
) -> bool {
    match requirement {
        flow::GraphRequirement::NoSelfLoops => {
            graph.edges().iter().all(|edge| edge.from() != edge.to())
        }
        flow::GraphRequirement::ZeroFlowFeasible => {
            graph.nodes().iter().all(|node| node.supply() == 0)
                && graph.edges().iter().all(|edge| edge.lower() == 0)
        }
        flow::GraphRequirement::PositiveCapacity => {
            graph.edges().iter().all(|edge| edge.capacity() > 0)
        }
        flow::GraphRequirement::NonEmptyEdges => !graph.edges().is_empty(),
        flow::GraphRequirement::ZeroCost => graph.edges().iter().all(|edge| edge.cost() == 0),
        flow::GraphRequirement::DistinctTerminals => {
            catalog_terminals_are_distinct_and_present(scenario)
        }
        flow::GraphRequirement::UnderlyingConnected => catalog_graph_is_underlying_connected(graph),
        flow::GraphRequirement::UnitCapacity => validate_unit_capacity_dinic_graph(graph).is_ok(),
        flow::GraphRequirement::UnitNetwork => catalog_terminal_indices(scenario, graph)
            .is_some_and(|(source, sink)| {
                validate_unit_network_dinic_graph(graph, source, sink).is_ok()
            }),
        flow::GraphRequirement::Bipartite => catalog_graph_bipartition(graph).is_some(),
        flow::GraphRequirement::BalancedBipartite => catalog_graph_bipartition(graph)
            .is_some_and(|partition| partition.left_count * 2 == graph.nodes().len()),
        flow::GraphRequirement::TransportationNetwork => {
            catalog_graph_is_transportation_network(scenario, graph)
        }
        flow::GraphRequirement::PlanarEmbedding => matches!(
            scenario.payload.model,
            FlowProblemModelV1::PlanarMaxFlow { .. }
        ),
        flow::GraphRequirement::StronglyConnected => catalog_graph_is_strongly_connected(graph),
        flow::GraphRequirement::NonbindingTransshipmentCapacities => {
            catalog_graph_has_nonbinding_transshipment_capacities(scenario, graph)
        }
    }
}

const fn catalog_graph_requirement_error(requirement: flow::GraphRequirement) -> &'static str {
    match requirement {
        flow::GraphRequirement::NoSelfLoops => {
            "selected flow algorithm requires a graph without self-loops"
        }
        flow::GraphRequirement::ZeroFlowFeasible => {
            "selected flow algorithm requires zero supplies and zero lower bounds"
        }
        flow::GraphRequirement::PositiveCapacity => {
            "selected flow algorithm requires every edge capacity to be positive"
        }
        flow::GraphRequirement::NonEmptyEdges => {
            "selected flow algorithm requires at least one edge"
        }
        flow::GraphRequirement::ZeroCost => "selected flow algorithm requires zero-cost edges",
        flow::GraphRequirement::DistinctTerminals => {
            "selected flow algorithm requires distinct source and sink nodes"
        }
        flow::GraphRequirement::UnderlyingConnected => {
            "selected flow algorithm requires a connected underlying graph"
        }
        flow::GraphRequirement::UnitCapacity => {
            "selected flow algorithm requires zero lower bounds and unit capacities"
        }
        flow::GraphRequirement::UnitNetwork => "selected flow algorithm requires a unit network",
        flow::GraphRequirement::Bipartite => "selected flow algorithm requires a bipartite graph",
        flow::GraphRequirement::BalancedBipartite => {
            "selected flow algorithm requires equal bipartition sizes"
        }
        flow::GraphRequirement::TransportationNetwork => {
            "selected flow algorithm requires a directed transportation network"
        }
        flow::GraphRequirement::PlanarEmbedding => {
            "selected flow algorithm requires a verified planar embedding"
        }
        flow::GraphRequirement::StronglyConnected => {
            "selected flow algorithm requires positive-width strong connectivity"
        }
        flow::GraphRequirement::NonbindingTransshipmentCapacities => {
            "selected flow algorithm requires every residual capacity width to cover the lower-adjusted required flow"
        }
    }
}

fn validate_catalog_negative_cycle_contract(
    descriptor: &flow::AlgorithmDescriptor,
    graph: &flow::FlowNetwork,
) -> Result<(), &'static str> {
    if descriptor.negative_cycle_policy != flow::NegativeCyclePolicy::RequireAbsentAnywhere {
        return Ok(());
    }
    if !catalog_graph_is_inside_precondition_validation_margin(descriptor, graph) {
        return Err(
            "negative-cycle precondition cannot be certified beyond the supported validation margin",
        );
    }
    let lower_flows = graph
        .edges()
        .iter()
        .map(flow::FlowEdge::lower)
        .collect::<Vec<_>>();
    match flow::check_residual_min_cost_optimality(graph, &lower_flows) {
        Ok(_) => Ok(()),
        Err(flow::CertificateError::NegativeCycle) => Err(
            "selected flow algorithm requires a lower-bound residual graph without negative-cost cycles",
        ),
        Err(flow::CertificateError::ArithmeticOverflow) => Err(
            "selected flow algorithm residual-cost arithmetic exceeds the supported exact range",
        ),
        Err(error) => unreachable!(
            "canonical lower-bound residual validation produced an internal error: {error}"
        ),
    }
}

fn catalog_graph_is_inside_precondition_validation_margin(
    descriptor: &flow::AlgorithmDescriptor,
    graph: &flow::FlowNetwork,
) -> bool {
    usize::try_from(descriptor.initial_band.max_nodes)
        .ok()
        .and_then(|maximum| maximum.checked_add(1))
        .is_some_and(|maximum| graph.nodes().len() <= maximum)
        && usize::try_from(descriptor.initial_band.max_edges)
            .ok()
            .and_then(|maximum| maximum.checked_add(1))
            .is_some_and(|maximum| graph.edges().len() <= maximum)
}

fn catalog_graph_has_strict_interior(scenario: &FlowScenarioV1, graph: &flow::FlowNetwork) -> bool {
    if graph
        .edges()
        .iter()
        .any(|edge| edge.lower() >= edge.capacity())
    {
        return false;
    }
    let Some(target) = catalog_required_divergences(scenario, graph) else {
        return false;
    };
    strict_interior_cut_conditions_hold(graph, &target)
}

fn catalog_required_divergences(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
) -> Option<Vec<i128>> {
    match &scenario.payload.model {
        FlowProblemModelV1::FixedFlowMinCost {
            source,
            sink,
            required_flow,
        } => {
            let source = flow::NodeId::parse(source)
                .ok()
                .and_then(|id| graph.node_index(&id))?;
            let sink = flow::NodeId::parse(sink)
                .ok()
                .and_then(|id| graph.node_index(&id))?;
            let Ok(required_flow) = required_flow.parse::<u64>() else {
                return None;
            };
            fixed_flow_divergences(graph, source, sink, required_flow).ok()
        }
        FlowProblemModelV1::Circulation {} | FlowProblemModelV1::Transshipment {} => {
            supply_divergences(graph).ok()
        }
        _ => None,
    }
}

fn catalog_graph_has_nonbinding_transshipment_capacities(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
) -> bool {
    let Some(target) = catalog_required_divergences(scenario, graph) else {
        return false;
    };
    let lower_flows = graph
        .edges()
        .iter()
        .map(flow::FlowEdge::lower)
        .collect::<Vec<_>>();
    let Ok(lower_divergence) = flow::divergences(graph, &lower_flows) else {
        return false;
    };
    let Some(required_width) =
        target
            .iter()
            .zip(lower_divergence)
            .try_fold(0_u128, |total, (&required, transported)| {
                let remaining = required.checked_sub(transported)?;
                if remaining <= 0 {
                    return Some(total);
                }
                total.checked_add(u128::try_from(remaining).ok()?)
            })
    else {
        return false;
    };
    let Ok(required_width) = u64::try_from(required_width) else {
        return false;
    };
    graph.edges().iter().all(|edge| {
        edge.capacity()
            .checked_sub(edge.lower())
            .is_some_and(|width| width >= required_width)
    })
}

fn strict_interior_cut_conditions_hold(graph: &flow::FlowNetwork, target: &[i128]) -> bool {
    let node_count = graph.nodes().len();
    if node_count >= usize::BITS as usize
        || target.len() != node_count
        || target.iter().copied().sum::<i128>() != 0
    {
        return false;
    }
    let all = (1_usize << node_count) - 1;
    for subset in 1..all {
        let balance = target
            .iter()
            .enumerate()
            .filter(|(node, _)| subset & (1_usize << node) != 0)
            .map(|(_, value)| value)
            .sum::<i128>();
        let mut lower_out = 0_i128;
        let mut upper_out = 0_i128;
        let mut lower_in = 0_i128;
        let mut upper_in = 0_i128;
        let mut crosses_cut = false;
        for edge in graph.edges() {
            let from_inside = subset & (1_usize << edge.from().as_usize()) != 0;
            let to_inside = subset & (1_usize << edge.to().as_usize()) != 0;
            if from_inside == to_inside {
                continue;
            }
            crosses_cut = true;
            if from_inside {
                lower_out += i128::from(edge.lower());
                upper_out += i128::from(edge.capacity());
            } else {
                lower_in += i128::from(edge.lower());
                upper_in += i128::from(edge.capacity());
            }
        }
        if !crosses_cut {
            if balance != 0 {
                return false;
            }
            continue;
        }
        if balance <= lower_out - upper_in || balance >= upper_out - lower_in {
            return false;
        }
    }
    true
}

fn catalog_terminals_are_distinct_and_present(scenario: &FlowScenarioV1) -> bool {
    let terminals = match &scenario.payload.model {
        FlowProblemModelV1::MaxFlow { source, sink }
        | FlowProblemModelV1::ParametricMaxFlow { source, sink, .. }
        | FlowProblemModelV1::FixedFlowMinCost { source, sink, .. }
        | FlowProblemModelV1::MinCostMaxFlow { source, sink }
        | FlowProblemModelV1::PlanarMaxFlow { source, sink, .. } => Some((source, sink)),
        _ => None,
    };
    let Some((source, sink)) = terminals else {
        return false;
    };
    source != sink
        && scenario
            .payload
            .graph
            .nodes
            .iter()
            .any(|node| &node.id == source)
        && scenario
            .payload
            .graph
            .nodes
            .iter()
            .any(|node| &node.id == sink)
}

fn catalog_terminal_indices(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
) -> Option<(flow::NodeIndex, flow::NodeIndex)> {
    let (source, sink) = match &scenario.payload.model {
        FlowProblemModelV1::MaxFlow { source, sink }
        | FlowProblemModelV1::ParametricMaxFlow { source, sink, .. }
        | FlowProblemModelV1::FixedFlowMinCost { source, sink, .. }
        | FlowProblemModelV1::MinCostMaxFlow { source, sink }
        | FlowProblemModelV1::PlanarMaxFlow { source, sink, .. } => (source, sink),
        FlowProblemModelV1::Circulation {}
        | FlowProblemModelV1::Transshipment {}
        | FlowProblemModelV1::BipartiteMatching { .. }
        | FlowProblemModelV1::Assignment { .. }
        | FlowProblemModelV1::Transportation { .. }
        | FlowProblemModelV1::ConvexCostFlow {} => return None,
    };
    let source = NodeId::parse(source).ok()?;
    let sink = NodeId::parse(sink).ok()?;
    Some((graph.node_index(&source)?, graph.node_index(&sink)?))
}

struct CatalogGraphBipartition {
    left_count: usize,
}

fn catalog_graph_bipartition(graph: &flow::FlowNetwork) -> Option<CatalogGraphBipartition> {
    let node_count = graph.nodes().len();
    if node_count < 2 {
        return None;
    }
    let mut adjacency = vec![Vec::new(); node_count];
    for edge in graph.edges() {
        let from = edge.from().as_usize();
        let to = edge.to().as_usize();
        if from == to {
            return None;
        }
        adjacency[from].push(to);
        adjacency[to].push(from);
    }

    let mut color = vec![None; node_count];
    let mut component_by_node = vec![0_usize; node_count];
    let mut component_count = 0_usize;
    for start in 0..node_count {
        if color[start].is_some() {
            continue;
        }
        let component = component_count;
        component_count += 1;
        color[start] = Some(false);
        component_by_node[start] = component;
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            let node_color = color[node]?;
            for &neighbor in &adjacency[node] {
                match color[neighbor] {
                    Some(neighbor_color) if neighbor_color == node_color => return None,
                    Some(_) => {}
                    None => {
                        color[neighbor] = Some(!node_color);
                        component_by_node[neighbor] = component;
                        stack.push(neighbor);
                    }
                }
            }
        }
    }

    let mut edge_counts = vec![0_usize; component_count];
    let mut zero_to_one_counts = vec![0_usize; component_count];
    for edge in graph.edges() {
        let from = edge.from().as_usize();
        let to = edge.to().as_usize();
        let component = component_by_node[from];
        edge_counts[component] += 1;
        if color[from] == Some(false) && color[to] == Some(true) {
            zero_to_one_counts[component] += 1;
        }
    }
    let flips = edge_counts
        .iter()
        .zip(&zero_to_one_counts)
        .map(|(&edges, &zero_to_one)| zero_to_one * 2 < edges)
        .collect::<Vec<_>>();
    let left = (0..node_count)
        .map(|node| color[node].is_some_and(|value| value == flips[component_by_node[node]]))
        .collect::<Vec<_>>();
    let left_count = left.iter().filter(|&&value| value).count();
    Some(CatalogGraphBipartition { left_count })
}

fn catalog_graph_is_transportation_network(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
) -> bool {
    let FlowProblemModelV1::Transportation {
        origins,
        destinations,
    } = &scenario.payload.model
    else {
        return false;
    };
    flow::TransportationGraph::validate_declaration(graph, origins, destinations).is_ok()
}

fn catalog_graph_is_strongly_connected(graph: &flow::FlowNetwork) -> bool {
    let node_count = graph.nodes().len();
    if node_count == 0 {
        return false;
    }
    let mut forward = vec![Vec::new(); node_count];
    let mut reverse = vec![Vec::new(); node_count];
    for edge in graph.edges() {
        if edge.capacity() == edge.lower() {
            continue;
        }
        let from = edge.from().as_usize();
        let to = edge.to().as_usize();
        forward[from].push(to);
        reverse[to].push(from);
    }
    catalog_adjacency_reaches_all(&forward) && catalog_adjacency_reaches_all(&reverse)
}

fn catalog_adjacency_reaches_all(adjacency: &[Vec<usize>]) -> bool {
    let Some(_) = adjacency.first() else {
        return false;
    };
    let mut seen = vec![false; adjacency.len()];
    seen[0] = true;
    let mut stack = vec![0_usize];
    while let Some(node) = stack.pop() {
        for &neighbor in &adjacency[node] {
            if seen[neighbor] {
                continue;
            }
            seen[neighbor] = true;
            stack.push(neighbor);
        }
    }
    seen.into_iter().all(|visited| visited)
}

fn catalog_graph_is_underlying_connected(graph: &flow::FlowNetwork) -> bool {
    if graph.nodes().is_empty() {
        return false;
    }
    let mut adjacency = vec![Vec::new(); graph.nodes().len()];
    for edge in graph.edges() {
        adjacency[edge.from().as_usize()].push(edge.to().as_usize());
        adjacency[edge.to().as_usize()].push(edge.from().as_usize());
    }
    let mut seen = vec![false; graph.nodes().len()];
    seen[0] = true;
    let mut stack = vec![0_usize];
    while let Some(node) = stack.pop() {
        for &neighbor in &adjacency[node] {
            if seen[neighbor] {
                continue;
            }
            seen[neighbor] = true;
            stack.push(neighbor);
        }
    }
    seen.into_iter().all(|visited| visited)
}

fn ready_flow_scene(scenario: &FlowScenarioV1) -> Result<FlowCurrentSceneV9, JsError> {
    let graph = scenario
        .canonical_network()
        .map_err(|error| JsError::new(&error.to_string()))?;
    if matches!(
        scenario.payload.model,
        FlowProblemModelV1::ParametricMaxFlow { .. }
    ) {
        let problem = scenario
            .parametric_problem(&graph)
            .map_err(|error| JsError::new(&error.to_string()))?;
        return ready_parametric_scene(scenario, &graph, &problem);
    }
    ready_plain_flow_scene(scenario, &graph)
}

fn scenario_trace_steps(
    scenario: &FlowScenarioV1,
) -> Result<flow::AlgorithmStepContractV1, JsError> {
    let algorithm =
        validate_runtime_algorithm(&scenario.payload.algorithm.id).map_err(JsError::new)?;
    find_algorithm_by_id(algorithm)
        .map(|descriptor| descriptor.trace_steps)
        .ok_or_else(|| JsError::new("flow algorithm is not present in the catalog"))
}

fn ready_plain_flow_scene(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
) -> Result<FlowCurrentSceneV9, JsError> {
    let flows = scenario_initial_flows(scenario, graph)?;
    let state = ResidualState::from_flows(graph, &flows)
        .map_err(|error| JsError::new(&error.to_string()))?;
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        &state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        FlowTraceMetrics::default(),
    );
    let payload = &scenario.payload;
    let mut scene = FlowCurrentSceneV9::ready(
        payload.model.clone(),
        payload.graph.clone(),
        payload.algorithm.clone(),
        payload.run_profile,
        payload.trace_granularity,
        scenario_trace_steps(scenario)?,
    );
    scene
        .apply_trace_snapshot(graph, &snapshot, None, 0)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(scene)
}

fn scenario_initial_flows(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
) -> Result<Vec<u64>, JsError> {
    let declarations = scenario
        .payload
        .graph
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect::<BTreeMap<_, _>>();
    graph
        .edges()
        .iter()
        .map(|edge| {
            let declaration = declarations
                .get(edge.id().as_str())
                .ok_or_else(|| JsError::new("flow edge declaration is missing"))?;
            declaration
                .initial_flow
                .as_deref()
                .unwrap_or(&declaration.lower)
                .parse::<u64>()
                .map_err(|_| JsError::new("flow initial value is not a canonical u64"))
        })
        .collect()
}

fn dynamic_eibfs_updates(scenario: &FlowScenarioV1) -> Result<Vec<DynamicCapacityUpdate>, JsError> {
    scenario
        .payload
        .updates
        .as_ref()
        .ok_or_else(|| JsError::new("Dynamic EIBFS capacity updates are missing"))?
        .iter()
        .map(|update| {
            let FlowUpdateV1::SetCapacity { edge, capacity } = update else {
                return Err(JsError::new(
                    "Dynamic EIBFS accepts only set-capacity updates",
                ));
            };
            Ok(DynamicCapacityUpdate::new(
                flow::EdgeId::parse(edge).map_err(|error| JsError::new(&error.to_string()))?,
                capacity
                    .parse()
                    .map_err(|_| JsError::new("updated capacity is not a canonical u64"))?,
            ))
        })
        .collect()
}

fn flow_rational(parameter: &ParametricRational) -> FlowRationalV1 {
    FlowRationalV1 {
        numerator: parameter.numerator().to_string(),
        denominator: parameter.denominator().to_string(),
    }
}

fn parametric_segment(segment: &flow::ParametricSegment) -> FlowParametricSegmentV1 {
    FlowParametricSegmentV1 {
        lower: flow_rational(&segment.lower),
        upper: flow_rational(&segment.upper),
        intercept: segment.minimal_cut.intercept.to_string(),
        slope: segment.minimal_cut.slope.to_string(),
        minimal_source_side: segment
            .minimal_cut
            .source_side
            .iter()
            .map(|node| node.as_str().to_owned())
            .collect(),
        maximal_source_side: segment
            .maximal_cut
            .source_side
            .iter()
            .map(|node| node.as_str().to_owned())
            .collect(),
    }
}

fn parametric_breakpoint(breakpoint: &flow::ParametricBreakpoint) -> FlowParametricBreakpointV1 {
    let nodes = |source_side: &[NodeId]| {
        source_side
            .iter()
            .map(|node| node.as_str().to_owned())
            .collect()
    };
    FlowParametricBreakpointV1 {
        parameter: flow_rational(&breakpoint.parameter),
        before_source_side: nodes(&breakpoint.before_source_side),
        after_source_side: nodes(&breakpoint.after_source_side),
        exact_minimal_source_side: nodes(&breakpoint.exact_minimal_source_side),
        exact_maximal_source_side: nodes(&breakpoint.exact_maximal_source_side),
        entering_nodes: nodes(&breakpoint.entering_nodes),
    }
}

fn parametric_edge_capacities(
    graph: &flow::FlowNetwork,
    problem: &flow::ParametricMaxFlowProblem,
    parameter: &ParametricRational,
) -> Result<Vec<FlowParametricEdgeCapacityV1>, JsError> {
    graph
        .edges()
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            let capacity = problem
                .capacity_at(graph, index, parameter)
                .map_err(|error| JsError::new(&error.to_string()))?;
            Ok(FlowParametricEdgeCapacityV1 {
                edge_id: edge.id().as_str().to_owned(),
                capacity: flow_rational(&capacity),
            })
        })
        .collect()
}

fn parametric_overlay(
    graph: &flow::FlowNetwork,
    problem: &flow::ParametricMaxFlowProblem,
    stage: &str,
    parameter: &ParametricRational,
    recorded_segments: Vec<FlowParametricSegmentV1>,
    recorded_breakpoints: Vec<FlowParametricBreakpointV1>,
    traversal: Option<FlowParametricTraversalV1>,
) -> Result<FlowParametricOverlayV1, JsError> {
    let visual_scale_max_capacity = problem
        .visual_scale_max_capacity(graph)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(FlowParametricOverlayV1 {
        stage: stage.to_owned(),
        parameter: flow_rational(parameter),
        edge_capacities: parametric_edge_capacities(graph, problem, parameter)?,
        visual_scale_max_capacity: flow_rational(&visual_scale_max_capacity),
        recorded_segments,
        recorded_breakpoints,
        traversal,
    })
}

fn ready_parametric_scene(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    problem: &flow::ParametricMaxFlowProblem,
) -> Result<FlowCurrentSceneV9, JsError> {
    let payload = &scenario.payload;
    let mut scene = FlowCurrentSceneV9::ready(
        payload.model.clone(),
        payload.graph.clone(),
        payload.algorithm.clone(),
        payload.run_profile,
        payload.trace_granularity,
        scenario_trace_steps(scenario)?,
    );
    scene.edge_states.clear();
    scene.residual_arcs.clear();
    scene.node_trace_states.clear();
    scene.parametric_overlay = Some(parametric_overlay(
        graph,
        problem,
        "ready",
        problem.minimum(),
        Vec::new(),
        Vec::new(),
        None,
    )?);
    Ok(scene)
}

fn parametric_outcome(
    segments: &[flow::ParametricSegment],
    breakpoints: &[flow::ParametricBreakpoint],
    metrics: FlowParametricMetricsV1,
) -> flow::FlowOutcomeV1 {
    flow::FlowOutcomeV1::ParametricMaxFlow {
        segments: segments.iter().map(parametric_segment).collect(),
        breakpoints: breakpoints.iter().map(parametric_breakpoint).collect(),
        metrics: Box::new(metrics),
    }
}

fn complete_parametric_overlay_outputs(
    segments: &[flow::ParametricSegment],
    breakpoints: &[flow::ParametricBreakpoint],
) -> (
    Vec<FlowParametricSegmentV1>,
    Vec<FlowParametricBreakpointV1>,
) {
    (
        segments.iter().map(parametric_segment).collect(),
        breakpoints.iter().map(parametric_breakpoint).collect(),
    )
}

fn canonical_parametric_metrics(metrics: ParametricPseudoflowMetrics) -> FlowParametricMetricsV1 {
    FlowParametricMetricsV1::ParametricPseudoflow {
        forest_initializations: metrics.forest_initializations.to_string(),
        parameter_advances: metrics.parameter_advances.to_string(),
        forest_reuses: metrics.forest_reuses.to_string(),
        renormalization_pushes: metrics.renormalization_pushes.to_string(),
        renormalization_splits: metrics.renormalization_splits.to_string(),
        mergers: metrics.mergers.to_string(),
        relabels: metrics.relabels.to_string(),
        free_run_races: metrics.free_run_races.to_string(),
        forward_race_wins: metrics.forward_race_wins.to_string(),
        reverse_race_wins: metrics.reverse_race_wins.to_string(),
        cooperative_race_steps: metrics.cooperative_race_steps.to_string(),
        contraction_views: metrics.contraction_views.to_string(),
        smaller_child_restarts: metrics.smaller_child_restarts.to_string(),
        larger_child_continuations: metrics.larger_child_continuations.to_string(),
        maximum_depth: metrics.maximum_depth.to_string(),
        residual_arc_scans: metrics.residual_arc_scans.to_string(),
    }
}

fn rerun_parametric_metrics(metrics: ParametricBreakpointRerunMetrics) -> FlowParametricMetricsV1 {
    FlowParametricMetricsV1::BreakpointRerun {
        pseudoflow_runs: metrics.pseudoflow_runs.to_string(),
        oracle_runs: metrics.oracle_runs.to_string(),
        static_residual_arc_scans: metrics.static_residual_arc_scans.to_string(),
        intersections: metrics.intersections.to_string(),
        subproblems: metrics.subproblems.to_string(),
        segments: metrics.segments.to_string(),
        breakpoints: metrics.breakpoints.to_string(),
        simultaneous_breakpoints: metrics.simultaneous_breakpoints.to_string(),
        maximum_depth: metrics.maximum_depth.to_string(),
    }
}

fn apply_parametric_metrics(scene: &mut FlowCurrentSceneV9, metrics: &FlowParametricMetricsV1) {
    let values: Vec<String> = match metrics {
        FlowParametricMetricsV1::ParametricPseudoflow {
            forest_initializations,
            parameter_advances,
            forest_reuses,
            renormalization_pushes,
            renormalization_splits,
            mergers,
            relabels,
            free_run_races,
            forward_race_wins,
            reverse_race_wins,
            cooperative_race_steps,
            contraction_views,
            smaller_child_restarts,
            larger_child_continuations,
            maximum_depth,
            residual_arc_scans,
        } => vec![
            forest_initializations,
            parameter_advances,
            forest_reuses,
            renormalization_pushes,
            renormalization_splits,
            mergers,
            relabels,
            free_run_races,
            forward_race_wins,
            reverse_race_wins,
            cooperative_race_steps,
            contraction_views,
            smaller_child_restarts,
            larger_child_continuations,
            maximum_depth,
            residual_arc_scans,
        ]
        .into_iter()
        .cloned()
        .collect(),
        FlowParametricMetricsV1::BreakpointRerun {
            pseudoflow_runs,
            oracle_runs,
            static_residual_arc_scans,
            intersections,
            subproblems,
            segments,
            breakpoints,
            simultaneous_breakpoints,
            maximum_depth,
        } => vec![
            pseudoflow_runs,
            oracle_runs,
            static_residual_arc_scans,
            intersections,
            subproblems,
            segments,
            breakpoints,
            simultaneous_breakpoints,
            maximum_depth,
        ]
        .into_iter()
        .cloned()
        .collect(),
    };
    for (target, value) in scene.metrics.iter_mut().zip(values) {
        *target = value;
    }
}

fn parametric_fast_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    problem: &flow::ParametricMaxFlowProblem,
    segments: &[flow::ParametricSegment],
    breakpoints: &[flow::ParametricBreakpoint],
    metrics: FlowParametricMetricsV1,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let mut base = ready_parametric_scene(scenario, graph, problem)?;
    "1".clone_into(&mut base.event_count);
    let mut optimal = base.clone();
    "1".clone_into(&mut optimal.event_id);
    optimal.solve_status = FlowSolveStatusV1::Optimal;
    let projected_segments = segments.iter().map(parametric_segment).collect::<Vec<_>>();
    let projected_breakpoints = breakpoints
        .iter()
        .map(parametric_breakpoint)
        .collect::<Vec<_>>();
    optimal.parametric_overlay = Some(parametric_overlay(
        graph,
        problem,
        "optimal",
        problem.maximum(),
        projected_segments,
        projected_breakpoints,
        None,
    )?);
    apply_parametric_metrics(&mut optimal, &metrics);
    optimal.outcome = Some(parametric_outcome(segments, breakpoints, metrics));
    Ok(vec![base, optimal])
}

fn canonical_event_kind(kind: ParametricPseudoflowEventKind) -> &'static str {
    match kind {
        ParametricPseudoflowEventKind::InitializeForest => "initialize-forest",
        ParametricPseudoflowEventKind::InspectResidualArc => "inspect-residual-arc",
        ParametricPseudoflowEventKind::FreeRunRace => "free-run-race",
        ParametricPseudoflowEventKind::CreateContractionViews => "create-contraction-views",
        ParametricPseudoflowEventKind::RestartSmallerChild => "restart-smaller-child",
        ParametricPseudoflowEventKind::ContinueLargerChild => "continue-larger-child",
        ParametricPseudoflowEventKind::RecordSegment => "record-segment",
        ParametricPseudoflowEventKind::RecordBreakpoint => "record-breakpoint",
        ParametricPseudoflowEventKind::Optimal => "optimal",
    }
}

fn traversal_orientation(orientation: ParametricTraversalOrientation) -> String {
    match orientation {
        ParametricTraversalOrientation::Forward => "forward",
        ParametricTraversalOrientation::Reverse => "reverse",
    }
    .to_owned()
}

fn race_winner(winner: ParametricRaceWinner) -> String {
    match winner {
        ParametricRaceWinner::Forward => "forward",
        ParametricRaceWinner::Reverse => "reverse",
    }
    .to_owned()
}

fn canonical_parametric_traversal(
    event: &flow::ParametricPseudoflowTraceEvent,
) -> FlowParametricTraversalV1 {
    FlowParametricTraversalV1 {
        kind: canonical_event_kind(event.kind).to_owned(),
        lower: flow_rational(&event.lower),
        upper: flow_rational(&event.upper),
        probe: event.parameter.as_ref().map(flow_rational),
        orientation: event.orientation.map(traversal_orientation),
        race_winner: event.race_winner.map(race_winner),
        cold_static_rerun: false,
        static_run_ordinal: None,
        scale_denominator: None,
        lower_source_side: Vec::new(),
        upper_source_side: Vec::new(),
        normalized_tree_reused: event.normalized_tree_reused,
        labels_retained: event.labels_retained,
        active_nodes: event.active_nodes.map(|value| value.to_string()),
        left_active_nodes: event.left_active_nodes.map(|value| value.to_string()),
        right_active_nodes: event.right_active_nodes.map(|value| value.to_string()),
        renormalization_pushes: event.renormalization_pushes.to_string(),
        renormalization_splits: event.renormalization_splits.to_string(),
    }
}

fn parametric_trace_event_scene(
    algorithm_id: &str,
    event_id: u64,
    event_kind: &str,
    minimum_granularity: TraceGranularityV1,
) -> flow::FlowTraceEventSceneV1 {
    flow::FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: None,
        catalog_id: format!("{algorithm_id}.{event_kind}"),
        minimum_granularity,
        pseudocode_line: format!("{algorithm_id}:{event_kind}"),
        patch_count: 0,
        entity_refs: Vec::new(),
        detail: None,
    }
}

const fn canonical_parametric_granularity(
    kind: ParametricPseudoflowEventKind,
) -> TraceGranularityV1 {
    match kind {
        ParametricPseudoflowEventKind::InitializeForest
        | ParametricPseudoflowEventKind::CreateContractionViews
        | ParametricPseudoflowEventKind::Optimal => TraceGranularityV1::Phase,
        ParametricPseudoflowEventKind::InspectResidualArc
        | ParametricPseudoflowEventKind::FreeRunRace => TraceGranularityV1::Micro,
        ParametricPseudoflowEventKind::RestartSmallerChild
        | ParametricPseudoflowEventKind::ContinueLargerChild
        | ParametricPseudoflowEventKind::RecordSegment
        | ParametricPseudoflowEventKind::RecordBreakpoint => TraceGranularityV1::Operation,
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the exhaustive typed parametric-event projection keeps every source event kind visibly closed"
)]
fn parametric_pseudoflow_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    problem: &flow::ParametricMaxFlowProblem,
    run: &flow::ParametricPseudoflowTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("parametric trace event count overflow"))?;
    let mut base = ready_parametric_scene(scenario, graph, problem)?;
    base.event_count = event_count.to_string();
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut segments = Vec::new();
    let mut breakpoints = Vec::new();
    for event in &run.events {
        if event.kind == ParametricPseudoflowEventKind::RecordSegment
            && let Some(segment) = run
                .result
                .segments
                .iter()
                .find(|segment| segment.lower == event.lower && segment.upper == event.upper)
        {
            let projected = parametric_segment(segment);
            if !segments.iter().any(|candidate: &FlowParametricSegmentV1| {
                candidate.lower == projected.lower && candidate.upper == projected.upper
            }) {
                segments.push(projected);
            }
        }
        if event.kind == ParametricPseudoflowEventKind::RecordBreakpoint
            && let Some(parameter) = &event.parameter
            && let Some(breakpoint) = run
                .result
                .breakpoints
                .iter()
                .find(|breakpoint| breakpoint.parameter == *parameter)
        {
            breakpoints.push(parametric_breakpoint(breakpoint));
        }
        let parameter = if event.kind == ParametricPseudoflowEventKind::Optimal {
            problem.maximum()
        } else {
            event.parameter.as_ref().unwrap_or(&event.lower)
        };
        let metrics = canonical_parametric_metrics(event.metrics);
        let mut scene = base.clone();
        scene.event_id = event.event_id.to_string();
        let mut trace_event = parametric_trace_event_scene(
            scenario.payload.algorithm.id.as_str(),
            event.event_id,
            canonical_event_kind(event.kind),
            canonical_parametric_granularity(event.kind),
        );
        trace_event.entity_refs = event
            .active_node_ids
            .iter()
            .map(|node| FlowTraceEntityRefSceneV1::Node {
                node_id: graph.nodes()[node.as_usize()].id().as_str().to_owned(),
            })
            .collect();
        if let Some(edge) = &event.inspected_edge {
            trace_event
                .entity_refs
                .push(FlowTraceEntityRefSceneV1::Edge {
                    edge_id: edge.as_str().to_owned(),
                });
            trace_event.detail = Some(FlowTraceEventDetailSceneV1 {
                label: "residual arc scan".to_owned(),
                value: event.metrics.residual_arc_scans.to_string(),
            });
        }
        trace_event.patch_count = u32::try_from(trace_event.entity_refs.len())
            .map_err(|_| JsError::new("parametric trace entity-ref count overflow"))?;
        scene.trace_event = Some(trace_event);
        scene.solve_status = FlowSolveStatusV1::Running;
        let (published_segments, published_breakpoints) =
            if event.kind == ParametricPseudoflowEventKind::Optimal {
                complete_parametric_overlay_outputs(&run.result.segments, &run.result.breakpoints)
            } else {
                (segments.clone(), breakpoints.clone())
            };
        let stage = canonical_event_kind(event.kind);
        scene.parametric_overlay = Some(parametric_overlay(
            graph,
            problem,
            stage,
            parameter,
            published_segments,
            published_breakpoints,
            Some(canonical_parametric_traversal(event)),
        )?);
        apply_parametric_metrics(&mut scene, &metrics);
        if event.kind == ParametricPseudoflowEventKind::Optimal {
            scene.solve_status = FlowSolveStatusV1::Optimal;
            scene.outcome = Some(parametric_outcome(
                &run.result.segments,
                &run.result.breakpoints,
                canonical_parametric_metrics(run.result.metrics),
            ));
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    Ok(timeline.finish())
}

const fn rerun_parametric_granularity(kind: ParametricTraceEventKind) -> TraceGranularityV1 {
    match kind {
        ParametricTraceEventKind::InitializeEndpoints | ParametricTraceEventKind::Optimal => {
            TraceGranularityV1::Phase
        }
        ParametricTraceEventKind::IntersectCutFunctions
        | ParametricTraceEventKind::InspectStaticResidualArc => TraceGranularityV1::Micro,
        ParametricTraceEventKind::ColdStaticSolve
        | ParametricTraceEventKind::SolveIntersection
        | ParametricTraceEventKind::RecordSegment
        | ParametricTraceEventKind::RecordBreakpoint
        | ParametricTraceEventKind::CertifyStaticOracle => TraceGranularityV1::Operation,
    }
}

fn rerun_event_kind(kind: ParametricTraceEventKind) -> &'static str {
    match kind {
        ParametricTraceEventKind::ColdStaticSolve => "cold-static-solve",
        ParametricTraceEventKind::InspectStaticResidualArc => "inspect-static-residual-arc",
        ParametricTraceEventKind::InitializeEndpoints => "initialize-endpoints",
        ParametricTraceEventKind::IntersectCutFunctions => "intersect-cut-functions",
        ParametricTraceEventKind::SolveIntersection => "solve-intersection",
        ParametricTraceEventKind::RecordSegment => "record-segment",
        ParametricTraceEventKind::RecordBreakpoint => "record-breakpoint",
        ParametricTraceEventKind::CertifyStaticOracle => "certify-static-oracle",
        ParametricTraceEventKind::Optimal => "optimal",
    }
}

fn rerun_parametric_traversal(event: &flow::ParametricTraceEvent) -> FlowParametricTraversalV1 {
    let nodes = |source_side: &[NodeId]| {
        source_side
            .iter()
            .map(|node| node.as_str().to_owned())
            .collect()
    };
    FlowParametricTraversalV1 {
        kind: rerun_event_kind(event.kind).to_owned(),
        lower: flow_rational(&event.lower),
        upper: flow_rational(&event.upper),
        probe: event.parameter.as_ref().map(flow_rational),
        orientation: None,
        race_winner: None,
        cold_static_rerun: event.cold_static_rerun,
        static_run_ordinal: event.static_run_ordinal.map(|value| value.to_string()),
        scale_denominator: event.scale_denominator.as_ref().map(ToString::to_string),
        lower_source_side: nodes(&event.lower_source_side),
        upper_source_side: nodes(&event.upper_source_side),
        normalized_tree_reused: event.normalized_tree_reused,
        labels_retained: false,
        active_nodes: None,
        left_active_nodes: None,
        right_active_nodes: None,
        renormalization_pushes: "0".to_owned(),
        renormalization_splits: "0".to_owned(),
    }
}

fn parametric_rerun_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    problem: &flow::ParametricMaxFlowProblem,
    run: &flow::ParametricBreakpointRerunTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("parametric trace event count overflow"))?;
    let mut base = ready_parametric_scene(scenario, graph, problem)?;
    base.event_count = event_count.to_string();
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut segments = Vec::new();
    let mut breakpoints = Vec::new();
    for event in &run.events {
        if event.kind == ParametricTraceEventKind::RecordSegment
            && let Some(segment) = run
                .result
                .segments
                .iter()
                .find(|segment| segment.lower == event.lower && segment.upper == event.upper)
        {
            let projected = parametric_segment(segment);
            if !segments.iter().any(|candidate: &FlowParametricSegmentV1| {
                candidate.lower == projected.lower && candidate.upper == projected.upper
            }) {
                segments.push(projected);
            }
        }
        if event.kind == ParametricTraceEventKind::RecordBreakpoint
            && let Some(parameter) = &event.parameter
            && let Some(breakpoint) = run
                .result
                .breakpoints
                .iter()
                .find(|breakpoint| breakpoint.parameter == *parameter)
        {
            breakpoints.push(parametric_breakpoint(breakpoint));
        }
        let parameter = if event.kind == ParametricTraceEventKind::Optimal {
            problem.maximum()
        } else {
            event.parameter.as_ref().unwrap_or(&event.lower)
        };
        let metrics = rerun_parametric_metrics(event.metrics);
        let mut scene = base.clone();
        scene.event_id = event.event_id.to_string();
        let mut trace_event = parametric_trace_event_scene(
            scenario.payload.algorithm.id.as_str(),
            event.event_id,
            rerun_event_kind(event.kind),
            rerun_parametric_granularity(event.kind),
        );
        if let Some(edge) = &event.inspected_edge {
            trace_event.entity_refs = vec![FlowTraceEntityRefSceneV1::Edge {
                edge_id: edge.as_str().to_owned(),
            }];
            trace_event.patch_count = 1;
        }
        scene.trace_event = Some(trace_event);
        scene.solve_status = FlowSolveStatusV1::Running;
        let (published_segments, published_breakpoints) =
            if event.kind == ParametricTraceEventKind::Optimal {
                complete_parametric_overlay_outputs(&run.result.segments, &run.result.breakpoints)
            } else {
                (segments.clone(), breakpoints.clone())
            };
        let stage = rerun_event_kind(event.kind);
        scene.parametric_overlay = Some(parametric_overlay(
            graph,
            problem,
            stage,
            parameter,
            published_segments,
            published_breakpoints,
            Some(rerun_parametric_traversal(event)),
        )?);
        apply_parametric_metrics(&mut scene, &metrics);
        if event.kind == ParametricTraceEventKind::Optimal {
            scene.solve_status = FlowSolveStatusV1::Optimal;
            scene.outcome = Some(parametric_outcome(
                &run.result.segments,
                &run.result.breakpoints,
                rerun_parametric_metrics(run.result.metrics),
            ));
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    Ok(timeline.finish())
}

fn terminal_indices(
    graph: &flow::FlowNetwork,
    source: &str,
    sink: &str,
) -> Result<(flow::NodeIndex, flow::NodeIndex), JsError> {
    let source = graph
        .node_index(&NodeId::parse(source).map_err(|error| JsError::new(&error.to_string()))?)
        .ok_or_else(|| JsError::new("source node is missing"))?;
    let sink = graph
        .node_index(&NodeId::parse(sink).map_err(|error| JsError::new(&error.to_string()))?)
        .ok_or_else(|| JsError::new("sink node is missing"))?;
    Ok((source, sink))
}

fn max_flow_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::EdmondsKarpTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    max_flow_trace_frames_from_parts(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

fn ford_fulkerson_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::FordFulkersonTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    max_flow_trace_frames_from_parts(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

fn dinic_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::DinicTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let events =
        rebind_trace_namespace(&run.events, "dinic", scenario.payload.algorithm.id.as_str());
    max_flow_trace_frames_from_parts(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &events,
        &run.final_snapshot,
    )
}

fn rebind_trace_namespace(
    events: &[flow::FlowTraceEvent],
    source_namespace: &str,
    target_namespace: &str,
) -> Vec<flow::FlowTraceEvent> {
    if source_namespace == target_namespace {
        return events.to_vec();
    }
    let catalog_prefix = format!("{source_namespace}.");
    let pseudocode_prefix = format!("{source_namespace}:");
    events
        .iter()
        .cloned()
        .map(|mut event| {
            if let Some(suffix) = event.catalog_id.strip_prefix(&catalog_prefix) {
                event.catalog_id = format!("{target_namespace}.{suffix}");
            }
            if let Some(suffix) = event.pseudocode_line.strip_prefix(&pseudocode_prefix) {
                event.pseudocode_line = format!("{target_namespace}:{suffix}");
            }
            event
        })
        .collect()
}

fn dynamic_tree_blocking_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::DynamicTreeBlockingTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let events = rebind_trace_namespace(
        &run.events,
        "dynamic-tree-blocking",
        scenario.payload.algorithm.id.as_str(),
    );
    max_flow_trace_frames_from_parts(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &events,
        &run.final_snapshot,
    )
}

fn dynamic_tree_push_relabel_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::DynamicTreePushRelabelTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    max_flow_trace_frames_from_parts(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

fn blocking_preflow_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::BlockingPreflowTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    max_flow_trace_frames_from_parts(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

fn pseudoflow_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::PseudoflowTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    max_flow_trace_frames_from_parts(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

fn sap_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::SapTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    max_flow_trace_frames_from_parts(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

fn push_relabel_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::PushRelabelTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    max_flow_trace_frames_from_parts(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

/// Maximum transient serialized size admitted while a deterministic flow
/// timeline is prepared. Complexity-faithful Detail can expose one boundary
/// per primitive on the practical 24-node/80-edge teaching presets, so this
/// preparation ceiling is intentionally larger than the resident cache.
/// `FlowTimelineCache` still retains at most 64 MiB and regenerates evicted
/// windows deterministically. Larger traces publish a resource-limit result;
/// the same scenario remains available in fast mode.
const MAX_EAGER_FLOW_TIMELINE_BYTES: usize = 256 * 1024 * 1024;

fn algorithm_state_value(scene: &FlowCurrentSceneV9) -> Result<serde_json::Value, JsError> {
    let mut value = serde_json::to_value(scene)
        .map_err(|error| JsError::new(&format!("failed to compare flow scene state: {error}")))?;
    let Some(object) = value.as_object_mut() else {
        return Err(JsError::new("flow scene state must serialize as an object"));
    };
    for field in [
        "event_id",
        "event_count",
        "solve_status",
        "trace_event",
        "trace_event_semantics",
        "outcome",
        "metrics",
        // Feasibility owns a revisioned auxiliary domain. It is compared
        // separately so publishing that immutable domain cannot masquerade
        // as a mutation of every public graph entity.
        "feasibility_overlay",
    ] {
        object.remove(field);
    }
    Ok(value)
}

fn feasibility_dynamic_state_value(
    scene: &FlowCurrentSceneV9,
) -> Result<Option<serde_json::Value>, JsError> {
    let Some(overlay) = &scene.feasibility_overlay else {
        return Ok(None);
    };
    let mut value = serde_json::to_value(overlay).map_err(|error| {
        JsError::new(&format!(
            "failed to compare feasibility working state: {error}"
        ))
    })?;
    let Some(object) = value.as_object_mut() else {
        return Err(JsError::new(
            "feasibility working state must serialize as an object",
        ));
    };
    // The domain is immutable input metadata, not algorithm working state.
    // Keeping it in the diff makes the first overlay publication claim that
    // every declared node and edge changed.
    object.remove("domain");
    Ok(Some(value))
}

fn collect_changed_feasibility_entities(
    before: &FlowCurrentSceneV9,
    after: &FlowCurrentSceneV9,
    node_ids: &BTreeSet<&str>,
    edge_ids: &BTreeSet<&str>,
    changed: &mut BTreeSet<FlowTraceEntityRefSceneV1>,
) -> Result<bool, JsError> {
    let before_state = feasibility_dynamic_state_value(before)?;
    let after_state = feasibility_dynamic_state_value(after)?;
    if before_state == after_state {
        return Ok(false);
    }
    match (&before_state, &after_state) {
        (Some(before_state), Some(after_state)) => collect_changed_entity_refs(
            Some(before_state),
            Some(after_state),
            None,
            node_ids,
            edge_ids,
            changed,
        ),
        (None, Some(_)) => {
            // The unpublished replay base already contains the complete node
            // state. The first visible source event owns only its explicit
            // local effect; treating overlay insertion as a subtree insertion
            // would fabricate a whole-graph mutation.
            if let Some(event) = &after.trace_event {
                changed.extend(event.entity_refs.iter().cloned());
            }
        }
        (Some(_) | None, None) => {}
    }
    Ok(true)
}

fn changed_entity_identity(
    value: &serde_json::Value,
    field_name: Option<&str>,
    node_ids: &BTreeSet<&str>,
    edge_ids: &BTreeSet<&str>,
) -> Option<FlowTraceEntityRefSceneV1> {
    if let Some(identity) = changed_scalar_entity_identity(value, field_name, node_ids, edge_ids) {
        return Some(identity);
    }
    let object = value.as_object()?;
    if let Some(node_id) = object.get("node_id").and_then(serde_json::Value::as_str)
        && node_ids.contains(node_id)
    {
        return Some(FlowTraceEntityRefSceneV1::Node {
            node_id: node_id.to_owned(),
        });
    }
    let edge_id = object.get("edge_id").and_then(serde_json::Value::as_str)?;
    if !edge_ids.contains(edge_id) {
        return None;
    }
    match object.get("direction").and_then(serde_json::Value::as_str) {
        Some(direction @ ("forward" | "reverse")) => Some(FlowTraceEntityRefSceneV1::ResidualArc {
            edge_id: edge_id.to_owned(),
            direction: direction.to_owned(),
        }),
        _ => Some(FlowTraceEntityRefSceneV1::Edge {
            edge_id: edge_id.to_owned(),
        }),
    }
}

fn changed_scalar_entity_identity(
    value: &serde_json::Value,
    field_name: Option<&str>,
    node_ids: &BTreeSet<&str>,
    edge_ids: &BTreeSet<&str>,
) -> Option<FlowTraceEntityRefSceneV1> {
    let id = value.as_str()?;
    let field_name = field_name.unwrap_or_default();
    let (node_hint, edge_hint) = match field_name {
        "node_id"
        | "matched_node_id"
        | "active_node"
        | "parent_node_id"
        | "tree_parent_node_id"
        | "original_node_id"
        | "bad_nodes"
        | "cut_side"
        | "pivot_cut"
        | "members"
        | "strong_nodes"
        | "entering_nodes"
        | "source_side"
        | "minimal_source_side"
        | "maximal_source_side"
        | "before_source_side"
        | "after_source_side"
        | "exact_minimal_source_side"
        | "exact_maximal_source_side"
        | "lower_source_side"
        | "upper_source_side"
        | "parent"
        | "child"
        | "source"
        | "target" => (true, false),
        "edge_id"
        | "capacity_edge_id"
        | "contraction_arc"
        | "leaving_edge"
        | "entering_edge"
        | "original_edge_id"
        | "bad_edges"
        | "rounding_processed_edge"
        | "changed_edge" => (false, true),
        "entity_id" => (true, true),
        _ => return None,
    };
    let is_node = node_ids.contains(id);
    let is_edge = edge_ids.contains(id);
    match (node_hint && is_node, edge_hint && is_edge) {
        (true, false) => Some(FlowTraceEntityRefSceneV1::Node {
            node_id: id.to_owned(),
        }),
        (false, true) => Some(FlowTraceEntityRefSceneV1::Edge {
            edge_id: id.to_owned(),
        }),
        _ => None,
    }
}

fn identity_array_map<'a>(
    values: &'a [serde_json::Value],
    field_name: Option<&str>,
    node_ids: &BTreeSet<&str>,
    edge_ids: &BTreeSet<&str>,
) -> Option<BTreeMap<FlowTraceEntityRefSceneV1, &'a serde_json::Value>> {
    let mut result = BTreeMap::new();
    for value in values {
        let identity = changed_entity_identity(value, field_name, node_ids, edge_ids)?;
        if result.insert(identity, value).is_some() {
            return None;
        }
    }
    Some(result)
}

fn collect_changed_subtree_refs(
    value: &serde_json::Value,
    on_before_side: bool,
    field_name: Option<&str>,
    node_ids: &BTreeSet<&str>,
    edge_ids: &BTreeSet<&str>,
    changed: &mut BTreeSet<FlowTraceEntityRefSceneV1>,
) {
    let mut collect = |child_field, child| {
        let (before, after) = if on_before_side {
            (Some(child), None)
        } else {
            (None, Some(child))
        };
        collect_changed_entity_refs(before, after, child_field, node_ids, edge_ids, changed);
    };
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                collect(Some(key), child);
            }
        }
        serde_json::Value::Array(array) => {
            for child in array {
                collect(field_name, child);
            }
        }
        _ => {}
    }
}

fn collect_changed_entity_refs(
    before: Option<&serde_json::Value>,
    after: Option<&serde_json::Value>,
    field_name: Option<&str>,
    node_ids: &BTreeSet<&str>,
    edge_ids: &BTreeSet<&str>,
    changed: &mut BTreeSet<FlowTraceEntityRefSceneV1>,
) {
    if before == after {
        return;
    }
    for entity in [before, after]
        .into_iter()
        .flatten()
        .filter_map(|value| changed_entity_identity(value, field_name, node_ids, edge_ids))
    {
        changed.insert(entity);
    }
    match (before, after) {
        (Some(serde_json::Value::Object(before)), Some(serde_json::Value::Object(after))) => {
            let keys = before.keys().chain(after.keys()).collect::<BTreeSet<_>>();
            for key in keys {
                collect_changed_entity_refs(
                    before.get(key),
                    after.get(key),
                    Some(key),
                    node_ids,
                    edge_ids,
                    changed,
                );
            }
        }
        (Some(serde_json::Value::Array(before)), Some(serde_json::Value::Array(after))) => {
            if let (Some(before_by_identity), Some(after_by_identity)) = (
                identity_array_map(before, field_name, node_ids, edge_ids),
                identity_array_map(after, field_name, node_ids, edge_ids),
            ) {
                let identities = before_by_identity
                    .keys()
                    .chain(after_by_identity.keys())
                    .cloned()
                    .collect::<BTreeSet<_>>();
                for identity in identities {
                    if before_by_identity.get(&identity) != after_by_identity.get(&identity) {
                        changed.insert(identity);
                    }
                }
                return;
            }
            for index in 0..before.len().max(after.len()) {
                collect_changed_entity_refs(
                    before.get(index),
                    after.get(index),
                    field_name,
                    node_ids,
                    edge_ids,
                    changed,
                );
            }
        }
        (Some(before @ (serde_json::Value::Object(_) | serde_json::Value::Array(_))), _) => {
            collect_changed_subtree_refs(before, true, field_name, node_ids, edge_ids, changed);
        }
        (_, Some(after @ (serde_json::Value::Object(_) | serde_json::Value::Array(_)))) => {
            collect_changed_subtree_refs(after, false, field_name, node_ids, edge_ids, changed);
        }
        _ => {}
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the common entity diff keeps all renderer-visible scene channels together"
)]
fn changed_flow_entities(
    before: &FlowCurrentSceneV9,
    after: &FlowCurrentSceneV9,
) -> Result<(Vec<FlowTraceEntityRefSceneV1>, bool, bool, bool), JsError> {
    let mut changed = BTreeSet::new();
    let mut primal_changed = false;
    let mut balance_or_label_changed = false;

    let before_nodes = before
        .graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    for node in &after.graph.nodes {
        if before_nodes
            .get(node.id.as_str())
            .is_some_and(|before_node| before_node.supply != node.supply)
        {
            primal_changed = true;
            changed.insert(FlowTraceEntityRefSceneV1::Node {
                node_id: node.id.clone(),
            });
        }
    }

    let before_edges = before
        .graph
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect::<BTreeMap<_, _>>();
    for edge in &after.graph.edges {
        if before_edges
            .get(edge.id.as_str())
            .is_some_and(|before_edge| {
                before_edge.lower != edge.lower
                    || before_edge.capacity != edge.capacity
                    || before_edge.cost != edge.cost
                    || before_edge.initial_flow != edge.initial_flow
            })
        {
            primal_changed = true;
            changed.insert(FlowTraceEntityRefSceneV1::Edge {
                edge_id: edge.id.clone(),
            });
        }
    }

    let before_flows = before
        .edge_states
        .iter()
        .map(|edge| (edge.edge_id.as_str(), edge.flow.as_str()))
        .collect::<BTreeMap<_, _>>();
    for edge in &after.edge_states {
        if before_flows
            .get(edge.edge_id.as_str())
            .is_some_and(|before_flow| *before_flow != edge.flow)
        {
            primal_changed = true;
            changed.insert(FlowTraceEntityRefSceneV1::Edge {
                edge_id: edge.edge_id.clone(),
            });
        }
    }

    let before_residual = before
        .residual_arcs
        .iter()
        .map(|arc| ((arc.edge_id.as_str(), arc.direction.as_str()), arc))
        .collect::<BTreeMap<_, _>>();
    for arc in &after.residual_arcs {
        if let Some(before_arc) =
            before_residual.get(&(arc.edge_id.as_str(), arc.direction.as_str()))
            && *before_arc != arc
        {
            changed.insert(FlowTraceEntityRefSceneV1::ResidualArc {
                edge_id: arc.edge_id.clone(),
                direction: arc.direction.clone(),
            });
        }
    }

    let before_trace_nodes = before
        .node_trace_states
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    for node in &after.node_trace_states {
        if let Some(before_node) = before_trace_nodes.get(node.node_id.as_str())
            && *before_node != node
        {
            if before_node.label != node.label
                || before_node.remaining_divergence != node.remaining_divergence
            {
                balance_or_label_changed = true;
            }
            changed.insert(FlowTraceEntityRefSceneV1::Node {
                node_id: node.node_id.clone(),
            });
        }
    }

    let node_ids = after
        .graph
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    let edge_ids = after
        .graph
        .edges
        .iter()
        .map(|edge| edge.id.as_str())
        .collect::<BTreeSet<_>>();
    let feasibility_state_changed =
        collect_changed_feasibility_entities(before, after, &node_ids, &edge_ids, &mut changed)?;
    let algorithm_state_changed = if after.algorithm.id == "augmenting-electrical-flow" {
        // The electrical direction is a deliberately global numeric field.
        // Recursively treating every changed vector entry as a focus identity
        // floods the graph and repeats a full-scene JSON diff at every pivot.
        // The source event already publishes the exact local pivot/path refs;
        // the typed overlay itself renders the global field transition.
        let before_state =
            serde_json::to_value(&before.augmenting_electrical_overlay).map_err(|error| {
                JsError::new(&format!("failed to compare flow scene state: {error}"))
            })?;
        let after_state =
            serde_json::to_value(&after.augmenting_electrical_overlay).map_err(|error| {
                JsError::new(&format!("failed to compare flow scene state: {error}"))
            })?;
        if before_state == after_state {
            false
        } else {
            if let Some(event) = &after.trace_event {
                changed.extend(event.entity_refs.iter().cloned());
            }
            true
        }
    } else {
        let before_state = algorithm_state_value(before)?;
        let after_state = algorithm_state_value(after)?;
        let state_changed = before_state != after_state;
        collect_changed_entity_refs(
            Some(&before_state),
            Some(&after_state),
            None,
            &node_ids,
            &edge_ids,
            &mut changed,
        );
        state_changed
    };
    let state_changed = algorithm_state_changed || feasibility_state_changed;

    Ok((
        changed.into_iter().collect(),
        primal_changed,
        balance_or_label_changed,
        state_changed,
    ))
}

fn primary_work_delta(
    before: &FlowCurrentSceneV9,
    after: &FlowCurrentSceneV9,
) -> Result<Option<u128>, JsError> {
    let ordinal = usize::from(after.trace_steps.primary_work.metric_ordinal);
    let before_value = primary_work_value(before, ordinal)?;
    let after_value = primary_work_value(after, ordinal)?;
    let delta = after_value
        .checked_sub(before_value)
        .ok_or_else(|| JsError::new("primary work counter decreased across a trace event"))?;
    Ok((delta != 0).then_some(delta))
}

fn primary_work_value(scene: &FlowCurrentSceneV9, ordinal: usize) -> Result<u128, JsError> {
    scene
        .metrics
        .get(ordinal)
        .ok_or_else(|| JsError::new("primary work metric ordinal is out of range"))?
        .parse::<u128>()
        .map_err(|_| JsError::new("primary work metric is not a canonical integer"))
}

fn trace_work_deltas(
    before: &FlowCurrentSceneV9,
    after: &FlowCurrentSceneV9,
    event: &FlowTraceEventSceneV1,
) -> Result<Vec<FlowTraceWorkDeltaV1>, JsError> {
    let mut result = vec![FlowTraceWorkDeltaV1 {
        unit: FlowTraceWorkUnitV1::PublishedTransition,
        count: "1".to_owned(),
    }];
    if event.minimum_granularity == TraceGranularityV1::Micro {
        result.push(FlowTraceWorkDeltaV1 {
            unit: FlowTraceWorkUnitV1::DetailPrimitive,
            count: "1".to_owned(),
        });
    }
    if let Some(count) = primary_work_delta(before, after)? {
        if let Err(message) = validate_primary_work_boundary(&event.catalog_id) {
            #[cfg(test)]
            panic!("{message}: {}", event.catalog_id);
            #[cfg(not(test))]
            return Err(JsError::new(message));
        }
        result.push(FlowTraceWorkDeltaV1 {
            unit: FlowTraceWorkUnitV1::PrimaryWork,
            count: count.to_string(),
        });
    }
    if let Some(unit) = exact_event_work_unit(&event.catalog_id) {
        result.push(FlowTraceWorkDeltaV1 {
            unit,
            count: "1".to_owned(),
        });
    }
    Ok(result)
}

fn validate_primary_work_boundary(catalog_id: &str) -> Result<(), &'static str> {
    if is_source_detail_primitive(catalog_id) || is_source_primary_work_boundary(catalog_id) {
        return Ok(());
    }
    Err("primary work advanced on an undeclared source boundary")
}

fn event_semantics(
    before: &FlowCurrentSceneV9,
    after: &FlowCurrentSceneV9,
    work_progress: FlowTraceWorkProgressV1,
) -> Result<FlowTraceEventSemanticsV1, JsError> {
    let event = after
        .trace_event
        .as_ref()
        .ok_or_else(|| JsError::new("nonzero flow boundary is missing trace metadata"))?;
    let (changed_entity_refs, primal_changed, balance_or_label_changed, state_changed) =
        changed_flow_entities(before, after)?;
    let role = if matches!(
        after.solve_status,
        FlowSolveStatusV1::PrimitiveComplete
            | FlowSolveStatusV1::Optimal
            | FlowSolveStatusV1::Infeasible
    ) {
        FlowTraceEventRoleV1::Certify
    } else if primal_changed {
        FlowTraceEventRoleV1::Commit
    } else if state_changed || balance_or_label_changed {
        FlowTraceEventRoleV1::Mutate
    } else if !event.entity_refs.is_empty() || event.detail.is_some() {
        FlowTraceEventRoleV1::Select
    } else {
        FlowTraceEventRoleV1::Observe
    };
    let work_deltas = trace_work_deltas(before, after, event)?;
    let aggregation_count = work_deltas
        .iter()
        .map(|delta| {
            delta
                .count
                .parse::<u128>()
                .map_err(|_| JsError::new("work delta is not a canonical integer"))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .unwrap_or(1)
        .to_string();
    let primary_work_block = trace_primary_work_block(&work_deltas)?;
    Ok(FlowTraceEventSemanticsV1 {
        role,
        work_deltas,
        aggregation_count,
        work_progress,
        primary_work_block,
        changed_entity_refs,
    })
}

fn trace_primary_work_block(
    work_deltas: &[FlowTraceWorkDeltaV1],
) -> Result<Option<FlowTracePrimaryWorkBlockV1>, JsError> {
    let Some(primary_delta) = work_deltas
        .iter()
        .find(|delta| delta.unit == FlowTraceWorkUnitV1::PrimaryWork)
    else {
        return Ok(None);
    };
    let delta = primary_delta
        .count
        .parse::<u128>()
        .map_err(|_| JsError::new("primary-work delta is not a canonical integer"))?;
    if delta == 0 {
        return Err(JsError::new("primary-work delta must be positive"));
    }
    let total = delta.to_string();
    Ok(Some(FlowTracePrimaryWorkBlockV1 {
        first: "1".to_owned(),
        last: total.clone(),
        total,
    }))
}

fn normalize_prepared_flow_timeline(
    frames: Vec<FlowCurrentSceneV9>,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let mut frames = preserve_source_boundaries(frames)?;
    normalize_event_ids_and_boundaries(&mut frames)?;
    let work = prepared_trace_work_contract(&frames)?;
    attach_prepared_trace_semantics(&mut frames, work)?;
    attach_parent_phase_ids(&mut frames);
    Ok(frames)
}

/// Preserves exactly the solver-published boundaries. Measured counter ranges
/// remain attached to their owning source event; no display-only frames are
/// inserted between two algorithm states.
fn preserve_source_boundaries(
    mut frames: Vec<FlowCurrentSceneV9>,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let Some(base) = frames.first() else {
        return Err(JsError::new("flow execution produced no base frame"));
    };
    if !matches!(base.run_profile, RunProfileV1::Trace) {
        return Ok(frames);
    }
    for frame in frames.iter_mut().skip(1) {
        if frame.trace_event.is_none() {
            continue;
        }
        let granularity = effective_source_granularity(frame);
        frame
            .trace_event
            .as_mut()
            .expect("checked trace event presence")
            .minimum_granularity = granularity;
    }
    Ok(frames)
}

fn effective_source_granularity(scene: &FlowCurrentSceneV9) -> TraceGranularityV1 {
    let declared = scene
        .trace_event
        .as_ref()
        .map_or(TraceGranularityV1::Micro, |event| event.minimum_granularity);
    match declared {
        TraceGranularityV1::Phase
            if matches!(
                scene.trace_steps.phase_availability,
                FlowStepAvailabilityV1::Available
            ) =>
        {
            TraceGranularityV1::Phase
        }
        TraceGranularityV1::Phase | TraceGranularityV1::Operation
            if matches!(
                scene.trace_steps.operation_availability,
                FlowStepAvailabilityV1::Available
            ) =>
        {
            TraceGranularityV1::Operation
        }
        TraceGranularityV1::Phase | TraceGranularityV1::Operation | TraceGranularityV1::Micro => {
            TraceGranularityV1::Micro
        }
    }
}

fn normalize_event_ids_and_boundaries(frames: &mut [FlowCurrentSceneV9]) -> Result<(), JsError> {
    let event_count = frames
        .len()
        .checked_sub(1)
        .ok_or_else(|| JsError::new("flow execution produced no base frame"))?;
    if event_count == 0 {
        return Err(JsError::new("flow execution produced no event"));
    }
    let event_count = event_count.to_string();
    for (index, scene) in frames.iter_mut().enumerate() {
        let event_id = index.to_string();
        event_id.clone_into(&mut scene.event_id);
        event_count.clone_into(&mut scene.event_count);
        if let Some(event) = scene.trace_event.as_mut() {
            event_id.clone_into(&mut event.event_id);
        }
    }
    for scene in frames.iter_mut().skip(1) {
        if let Some(event) = scene.trace_event.as_ref() {
            let detail_only_contract = matches!(
                scene.trace_steps.phase_availability,
                FlowStepAvailabilityV1::Unavailable { .. }
            ) && matches!(
                scene.trace_steps.operation_availability,
                FlowStepAvailabilityV1::Unavailable { .. }
            ) && matches!(
                scene.trace_steps.detail,
                FlowDetailStepCapabilityV1::Available { .. }
            );
            let forced_detail = is_source_detail_primitive(&event.catalog_id);
            let declared_detail = forced_detail
                || is_source_primary_work_boundary(&event.catalog_id)
                || detail_only_contract;
            let undeclared_detail =
                event.minimum_granularity == TraceGranularityV1::Micro && !declared_detail;
            if undeclared_detail {
                return Err(JsError::new(
                    "flow event publishes an undeclared Detail primitive",
                ));
            }
            if let Err(message) = validate_source_micro_locality(scene, event) {
                #[cfg(test)]
                panic!("{message}: {} {:?}", event.catalog_id, event.entity_refs);
                #[cfg(not(test))]
                return Err(JsError::new(message));
            }
        }
        if let Some(event) = scene.trace_event.as_mut() {
            let forced_detail = is_source_detail_primitive(&event.catalog_id);
            if forced_detail {
                event.minimum_granularity = TraceGranularityV1::Micro;
            }
        }
    }
    Ok(())
}

/// Rejects graph-wide or ambiguous focus on one source Detail boundary.
/// Aggregate paths, cuts, forests, and dense numeric fields stay in their
/// typed overlays; the vivid ordinary-graph focus identifies only the one
/// primitive operand currently inspected or changed.
fn validate_source_micro_locality(
    scene: &FlowCurrentSceneV9,
    event: &FlowTraceEventSceneV1,
) -> Result<(), &'static str> {
    if event.minimum_granularity != TraceGranularityV1::Micro {
        return Ok(());
    }
    let exact_refs = event.entity_refs.iter().collect::<BTreeSet<_>>();
    if exact_refs.len() != event.entity_refs.len() {
        return Err("flow Detail primitive contains duplicate focus identities");
    }
    let focused_nodes = event
        .entity_refs
        .iter()
        .filter_map(|entity| match entity {
            FlowTraceEntityRefSceneV1::Node { node_id } => Some(node_id.as_str()),
            FlowTraceEntityRefSceneV1::Edge { .. }
            | FlowTraceEntityRefSceneV1::ResidualArc { .. } => None,
        })
        .collect::<BTreeSet<_>>();
    let graph_nodes = scene
        .graph
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    if !focused_nodes.is_subset(&graph_nodes) {
        return Err("flow Detail primitive focuses an unknown node");
    }
    let focused_edges = event
        .entity_refs
        .iter()
        .filter_map(|entity| match entity {
            FlowTraceEntityRefSceneV1::Edge { edge_id }
            | FlowTraceEntityRefSceneV1::ResidualArc { edge_id, .. } => Some(edge_id.as_str()),
            FlowTraceEntityRefSceneV1::Node { .. } => None,
        })
        .collect::<BTreeSet<_>>();
    if focused_edges.len() > 1 {
        return Err("flow Detail primitive must focus at most one ordinary edge");
    }
    let maximum_focused_nodes = if focused_edges.is_empty()
        && !source_detail_allows_two_node_auxiliary_focus(&event.catalog_id)
    {
        1
    } else {
        2
    };
    if focused_nodes.len() > maximum_focused_nodes {
        return Err("flow Detail primitive focuses too many ordinary nodes");
    }
    if let Some(edge_id) = focused_edges.first() {
        let edge = scene
            .graph
            .edges
            .iter()
            .find(|edge| edge.id.as_str() == *edge_id)
            .ok_or("flow Detail primitive focuses an unknown edge")?;
        if focused_nodes
            .iter()
            .any(|node_id| *node_id != edge.from.as_str() && *node_id != edge.to.as_str())
        {
            return Err("flow Detail primitive node focus is not an endpoint of its edge");
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct PreparedTraceWorkContract {
    primary_ordinal: usize,
    base_primary_work: u128,
    primary_total: u128,
    detail_total: u128,
}

fn prepared_trace_work_contract(
    frames: &[FlowCurrentSceneV9],
) -> Result<PreparedTraceWorkContract, JsError> {
    let declared_primary = &frames[0].trace_steps.primary_work;
    if frames.iter().skip(1).any(|scene| {
        let candidate = &scene.trace_steps.primary_work;
        candidate.metric_ordinal != declared_primary.metric_ordinal
            || candidate.unit != declared_primary.unit
            || candidate.abstraction != declared_primary.abstraction
    }) {
        return Err(JsError::new(
            "primary work contract changed inside one prepared trace",
        ));
    }
    let primary_ordinal = usize::from(frames[0].trace_steps.primary_work.metric_ordinal);
    let base_primary_work = primary_work_value(&frames[0], primary_ordinal)?;
    let final_primary_work = primary_work_value(
        frames
            .last()
            .ok_or_else(|| JsError::new("flow execution produced no final frame"))?,
        primary_ordinal,
    )?;
    let primary_total = final_primary_work
        .checked_sub(base_primary_work)
        .ok_or_else(|| JsError::new("primary work counter decreased over the complete trace"))?;
    let detail_total = u128::try_from(frames.len().saturating_sub(1))
        .map_err(|_| JsError::new("Detail source-event total overflowed"))?;
    Ok(PreparedTraceWorkContract {
        primary_ordinal,
        base_primary_work,
        primary_total,
        detail_total,
    })
}

fn attach_prepared_trace_semantics(
    frames: &mut [FlowCurrentSceneV9],
    work: PreparedTraceWorkContract,
) -> Result<(), JsError> {
    let mut detail_completed = 0_u128;
    for index in 1..frames.len() {
        let (before, after) = frames.split_at_mut(index);
        let Some(event) = after[0].trace_event.as_mut() else {
            after[0].trace_event_semantics = None;
            continue;
        };
        let mut seen_entity_refs = BTreeSet::new();
        event
            .entity_refs
            .retain(|entity| seen_entity_refs.insert(entity.clone()));
        detail_completed = detail_completed
            .checked_add(1)
            .ok_or_else(|| JsError::new("Detail source-event progress overflowed"))?;
        let current_primary_work = primary_work_value(&after[0], work.primary_ordinal)?;
        let primary_completed = current_primary_work
            .checked_sub(work.base_primary_work)
            .ok_or_else(|| JsError::new("primary work progress decreased below its base"))?;
        if primary_completed > work.primary_total {
            return Err(JsError::new(
                "primary work progress exceeded the complete trace total",
            ));
        }
        let work_progress = FlowTraceWorkProgressV1 {
            detail_completed: detail_completed.to_string(),
            detail_total: work.detail_total.to_string(),
            primary_completed: primary_completed.to_string(),
            primary_total: work.primary_total.to_string(),
        };
        let semantics = event_semantics(&before[index - 1], &after[0], work_progress)?;
        after[0].trace_event_semantics = Some(semantics);
    }
    Ok(())
}

fn attach_parent_phase_ids(frames: &mut [FlowCurrentSceneV9]) {
    let mut current_phase_id = None;
    for scene in frames {
        let Some(event) = scene.trace_event.as_mut() else {
            continue;
        };
        if event.minimum_granularity == TraceGranularityV1::Phase {
            event.parent_phase_id = None;
            current_phase_id = Some(event.event_id.clone());
        } else {
            event.parent_phase_id.clone_from(&current_phase_id);
        }
    }
}

fn normalize_prepared_flow_timeline_sparse_with_limit(
    scenario: &FlowScenarioV1,
    frames: Vec<FlowCurrentSceneV9>,
    limit: usize,
) -> Result<PreparedFlowTimeline, JsError> {
    let normalized = normalize_prepared_flow_timeline(frames)?;
    let timeline = PreparedFlowTimeline::from_source_frames(normalized)?;
    if timeline.stored_bytes() > limit {
        let base = timeline.materialize(0)?;
        let limited =
            normalize_prepared_flow_timeline(flow_trace_resource_limit_frames(scenario, &base))?;
        return PreparedFlowTimeline::from_full_frames(limited);
    }
    Ok(timeline)
}

#[cfg(test)]
fn normalize_prepared_flow_timeline_with_limit(
    scenario: &FlowScenarioV1,
    frames: Vec<FlowCurrentSceneV9>,
    limit: usize,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let estimated_frame_bytes = frames
        .first()
        .ok_or_else(|| JsError::new("flow execution produced no base frame"))
        .and_then(serialized_flow_scene_bytes)?;
    let estimated_timeline_bytes = frames.len().saturating_mul(estimated_frame_bytes);
    if estimated_timeline_bytes > limit {
        let base = frames
            .first()
            .ok_or_else(|| JsError::new("flow execution produced no base frame"))?;
        return normalize_prepared_flow_timeline(flow_trace_resource_limit_frames(scenario, base));
    }
    let normalized = normalize_prepared_flow_timeline(frames)?;
    let mut serialized_bytes = 0_usize;
    for scene in &normalized {
        let scene_bytes = serialized_flow_scene_bytes(scene)?;
        let Some(total) = serialized_bytes.checked_add(scene_bytes) else {
            let base = normalized
                .first()
                .ok_or_else(|| JsError::new("flow execution produced no base frame"))?;
            return normalize_prepared_flow_timeline(flow_trace_resource_limit_frames(
                scenario, base,
            ));
        };
        if total > limit {
            let base = normalized
                .first()
                .ok_or_else(|| JsError::new("flow execution produced no base frame"))?;
            return normalize_prepared_flow_timeline(flow_trace_resource_limit_frames(
                scenario, base,
            ));
        }
        serialized_bytes = total;
    }
    Ok(normalized)
}

struct EagerFlowTimeline {
    frames: Vec<FlowCurrentSceneV9>,
    serialized_bytes: usize,
}

impl EagerFlowTimeline {
    fn new(base: FlowCurrentSceneV9) -> Result<Self, JsError> {
        let serialized_bytes = serialized_flow_scene_bytes(&base)?;
        Ok(Self {
            frames: vec![base],
            serialized_bytes,
        })
    }

    fn try_push(&mut self, scene: FlowCurrentSceneV9) -> Result<bool, JsError> {
        self.try_push_with_limit(scene, MAX_EAGER_FLOW_TIMELINE_BYTES)
    }

    fn try_push_with_limit(
        &mut self,
        scene: FlowCurrentSceneV9,
        limit: usize,
    ) -> Result<bool, JsError> {
        let scene_bytes = serialized_flow_scene_bytes(&scene)?;
        let Some(total) = self.serialized_bytes.checked_add(scene_bytes) else {
            return Ok(false);
        };
        if total > limit {
            return Ok(false);
        }
        self.serialized_bytes = total;
        self.frames.push(scene);
        Ok(true)
    }

    /// Defers the exact serialized-size check to `PreparedFlowTimeline`.
    ///
    /// Use only after the base-size/event-count preflight has admitted the
    /// bounded trace. This avoids serializing every full scene twice while the
    /// final prepared-timeline constructor still enforces the hard byte limit.
    fn push_with_deferred_size_validation(&mut self, scene: FlowCurrentSceneV9) {
        self.frames.push(scene);
    }

    fn finish(self) -> Vec<FlowCurrentSceneV9> {
        self.frames
    }
}

fn serialized_flow_scene_bytes(scene: &FlowCurrentSceneV9) -> Result<usize, JsError> {
    serde_json::to_vec(scene)
        .map(|json| json.len())
        .map_err(|error| JsError::new(&error.to_string()))
}

fn flow_trace_resource_limit_frames(
    _scenario: &FlowScenarioV1,
    base: &FlowCurrentSceneV9,
) -> Vec<FlowCurrentSceneV9> {
    let mut normalized_base = base.clone();
    "0".clone_into(&mut normalized_base.event_id);
    "1".clone_into(&mut normalized_base.event_count);
    // A preparation limit is a diagnostic about this exact trace state, not a
    // transition back to the scenario's zero-flow ready scene. Rebuilding the
    // frame here used to reset feasible initial flows and typed overlays without
    // any source event owning those visible changes.
    let mut limited = normalized_base.clone();
    "1".clone_into(&mut limited.event_id);
    "1".clone_into(&mut limited.event_count);
    limited.apply_resource_limit_with_reason(FlowResourceLimitReasonV1::TracePublication);
    vec![normalized_base, limited]
}

fn trace_timeline_resource_limit_frames(
    scenario: &FlowScenarioV1,
    base: &FlowCurrentSceneV9,
    event_count: u64,
) -> Result<Option<Vec<FlowCurrentSceneV9>>, JsError> {
    let estimated_frame_bytes = serialized_flow_scene_bytes(base)?;
    let frame_count = usize::try_from(event_count)
        .ok()
        .and_then(|count| count.checked_add(1));
    let estimated_timeline_bytes = frame_count
        .and_then(|count| estimated_frame_bytes.checked_mul(count))
        .unwrap_or(usize::MAX);
    if estimated_timeline_bytes <= MAX_EAGER_FLOW_TIMELINE_BYTES {
        return Ok(None);
    }
    Ok(Some(flow_trace_resource_limit_frames(scenario, base)))
}

fn hopcroft_karp_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::HopcroftKarpTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("flow trace event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut replay = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let mut scene = base.clone();
        scene
            .apply_trace_snapshot(graph, &replay, Some(event), event_count)
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        if index + 1 == run.events.len() {
            scene.set_bipartite_matching_outcome(&run.result.certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    if replay != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new("Hopcroft-Karp trace final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn hungarian_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::HungarianTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("flow trace event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut replay = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let mut scene = base.clone();
        scene
            .apply_trace_snapshot(graph, &replay, Some(event), event_count)
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        if index + 1 == run.events.len() {
            match &run.result.outcome {
                HungarianOutcome::Optimal { certificate, .. } => {
                    scene.set_assignment_outcome(graph, certificate);
                }
                HungarianOutcome::Infeasible { witness, .. } => {
                    scene.set_assignment_infeasible_outcome(witness);
                }
            }
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    if replay != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new("Hungarian trace final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn apply_hungarian_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::HungarianResult,
) -> Result<(), JsError> {
    let metrics = FlowAssignmentMetrics {
        agent_searches: result.metrics.agent_searches,
        cell_scans: result.metrics.cell_scans,
        dual_updates: result.metrics.dual_updates,
        predecessor_updates: result.metrics.predecessor_updates,
        augmentations: result.metrics.augmentations,
    };
    match &result.outcome {
        HungarianOutcome::Optimal { flows, certificate } => scene
            .apply_assignment_result(graph, flows, certificate, metrics)
            .map_err(|error| JsError::new(&error.to_string())),
        HungarianOutcome::Infeasible {
            partial_flows,
            witness,
        } => scene
            .apply_assignment_infeasibility(graph, partial_flows, witness, metrics)
            .map_err(|error| JsError::new(&error.to_string())),
    }
}

fn auction_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::AuctionTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("flow trace event count overflow"))?;
    let base = trace_snapshot_source_base(scenario, graph, &run.base_snapshot, event_count)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut replay = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let mut scene = base.clone();
        scene
            .apply_trace_snapshot(graph, &replay, Some(event), event_count)
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        if index + 1 == run.events.len() {
            match &run.result.outcome {
                AuctionOutcome::Optimal { certificate, .. } => {
                    scene.set_assignment_outcome(graph, certificate);
                }
                AuctionOutcome::Infeasible { witness, .. } => {
                    scene.set_assignment_infeasible_outcome(witness);
                }
            }
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    if replay != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new("Auction trace final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn apply_auction_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::AuctionResult,
) -> Result<(), JsError> {
    let metrics = FlowAuctionMetrics {
        feasibility_searches: result.metrics.feasibility_searches,
        feasibility_augmentations: result.metrics.feasibility_augmentations,
        edge_scans: result.metrics.edge_scans,
        scaling_phases: result.metrics.scaling_phases,
        bids: result.metrics.bids,
        price_raises: result.metrics.price_raises,
        awards: result.metrics.awards,
        evictions: result.metrics.evictions,
    };
    match &result.outcome {
        AuctionOutcome::Optimal { flows, certificate } => scene
            .apply_auction_result(graph, flows, certificate, metrics)
            .map_err(|error| JsError::new(&error.to_string())),
        AuctionOutcome::Infeasible {
            partial_flows,
            witness,
        } => scene
            .apply_auction_infeasibility(graph, partial_flows, witness, metrics)
            .map_err(|error| JsError::new(&error.to_string())),
    }
}

fn apply_transportation_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::TransportationResult,
) -> Result<(), JsError> {
    scene
        .apply_transportation_result(
            graph,
            &result.flows,
            &result.certificate,
            FlowTransportationMetrics {
                feasibility_searches: result.metrics.feasibility_searches,
                support_cycle_cancellations: result.metrics.support_cycle_cancellations,
                basis_extensions: result.metrics.basis_extensions,
                potential_recomputations: result.metrics.potential_recomputations,
                pricing_searches: result.metrics.pricing_searches,
                pricing_scans: result.metrics.pricing_scans,
                pivots: result.metrics.pivots,
                nondegenerate_pivots: result.metrics.nondegenerate_pivots,
                degenerate_pivots: result.metrics.degenerate_pivots,
                basis_exchanges: result.metrics.basis_exchanges,
                structure_scans: result.metrics.structure_scans,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn max_flow_trace_frames_from_parts(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    certificate: &flow::MaxFlowCertificate,
    base_snapshot: &FlowTraceSnapshot,
    events: &[flow::FlowTraceEvent],
    final_snapshot: &FlowTraceSnapshot,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count =
        u64::try_from(events.len()).map_err(|_| JsError::new("flow trace event count overflow"))?;
    let base = trace_snapshot_source_base(scenario, graph, base_snapshot, event_count)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut replay = base_snapshot.clone();
    for (index, event) in events.iter().enumerate() {
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let mut scene = base.clone();
        scene
            .apply_trace_snapshot(graph, &replay, Some(event), event_count)
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        if index + 1 == events.len() {
            scene.set_max_flow_outcome(certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    if replay != *final_snapshot || events.is_empty() {
        return Err(JsError::new("max-flow trace final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

#[expect(
    clippy::too_many_lines,
    reason = "the binary blocking trace projector validates each source-work boundary before publishing it"
)]
fn binary_blocking_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::BinaryBlockingStepTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("binary blocking-flow event count overflow"))?;
    let base = trace_snapshot_source_base(scenario, graph, &run.base_snapshot, event_count)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut replay = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        let scans_before = replay.metrics.residual_arc_scans;
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let scan_delta = replay
            .metrics
            .residual_arc_scans
            .checked_sub(scans_before)
            .ok_or_else(|| JsError::new("binary blocking-flow scan counter regressed"))?;
        let is_scan_boundary = matches!(
            event.catalog_id.as_str(),
            "binary-blocking-flow.inspect-initial-cut-arc"
                | "binary-blocking-flow.inspect-residual-arc"
                | "binary-blocking-flow.build-reverse-zero-one-adjacency"
                | "binary-blocking-flow.relax-binary-distance"
                | "binary-blocking-flow.inspect-binary-length"
                | "binary-blocking-flow.build-zero-scc-adjacency"
                | "binary-blocking-flow.inspect-zero-scc-reverse-arc"
                | "binary-blocking-flow.inspect-canonical-cut-arc"
                | "binary-blocking-flow.inspect-contracted-arc"
                | "binary-blocking-flow.build-lift-adjacency"
                | "binary-blocking-flow.inspect-lift-arc"
        );
        if is_scan_boundary {
            if !(1..=flow::BINARY_BLOCKING_TRACE_SCAN_BLOCK_MAX).contains(&scan_delta)
                || event.minimum_granularity != flow::TraceGranularityV1::Micro
                || !matches!(
                    event.entity_refs.as_slice(),
                    [flow::FlowTraceEntityRef::ResidualArc(_)]
                )
            {
                return Err(JsError::new(
                    "binary blocking-flow scan boundary is not source-faithful",
                ));
            }
        } else if scan_delta != 0 {
            return Err(JsError::new(
                "binary blocking-flow scan work is hidden in a non-scan boundary",
            ));
        }
        let mut scene = base.clone();
        scene
            .apply_trace_snapshot(graph, &replay, Some(event), event_count)
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        let stage = match event.catalog_id.as_str() {
            "binary-blocking-flow.inspect-initial-cut-arc"
            | "binary-blocking-flow.inspect-binary-length"
            | "binary-blocking-flow.inspect-residual-arc"
            | "binary-blocking-flow.build-reverse-zero-one-adjacency"
            | "binary-blocking-flow.relax-binary-distance"
            | "binary-blocking-flow.build-zero-scc-adjacency"
            | "binary-blocking-flow.inspect-zero-scc-reverse-arc"
            | "binary-blocking-flow.inspect-canonical-cut-arc" => {
                FlowBinaryBlockingStageV1::Analyzing
            }
            "binary-blocking-flow.analyze-binary-network" => FlowBinaryBlockingStageV1::Analyzed,
            "binary-blocking-flow.contract-zero-scc"
            | "binary-blocking-flow.inspect-contracted-arc"
            | "binary-blocking-flow.build-lift-adjacency"
            | "binary-blocking-flow.inspect-lift-arc"
            | "binary-blocking-flow.apply-contracted-flow"
            | "binary-blocking-flow.apply-lift-path" => FlowBinaryBlockingStageV1::Contracted,
            "binary-blocking-flow.complete-primitive" => FlowBinaryBlockingStageV1::Complete,
            _ => {
                return Err(JsError::new(
                    "binary blocking-flow trace has an unknown event identity",
                ));
            }
        };
        let projection = binary_blocking_trace_projection(&run.result, &replay, event, stage)?;
        scene
            .set_binary_blocking_overlay(graph, &projection, stage)
            .map_err(|error| JsError::new(&error.to_string()))?;
        if index + 1 == run.events.len() {
            scene.set_binary_blocking_outcome(&run.result);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    let count = |catalog_id: &str| {
        run.events
            .iter()
            .filter(|event| event.catalog_id == catalog_id)
            .count()
    };
    if replay != run.final_snapshot
        || count("binary-blocking-flow.analyze-binary-network") != 1
        || count("binary-blocking-flow.contract-zero-scc") != 1
        || count("binary-blocking-flow.complete-primitive") != 1
        || run.events.last().map(|event| event.catalog_id.as_str())
            != Some("binary-blocking-flow.complete-primitive")
    {
        return Err(JsError::new(
            "binary blocking-flow trace final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn binary_blocking_trace_projection(
    result: &flow::BinaryBlockingStepResult,
    snapshot: &FlowTraceSnapshot,
    event: &flow::FlowTraceEvent,
    stage: FlowBinaryBlockingStageV1,
) -> Result<flow::BinaryBlockingStepResult, JsError> {
    let mut projection = result.clone();
    projection.flows.clone_from(&snapshot.flows);
    projection.distances = snapshot
        .node_labels
        .iter()
        .map(|distance| {
            distance
                .map(u64::try_from)
                .transpose()
                .map_err(|_| JsError::new("binary blocking distance is outside u64"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let classification_complete = matches!(
        event.catalog_id.as_str(),
        "binary-blocking-flow.build-zero-scc-adjacency"
            | "binary-blocking-flow.inspect-zero-scc-reverse-arc"
            | "binary-blocking-flow.inspect-canonical-cut-arc"
            | "binary-blocking-flow.analyze-binary-network"
            | "binary-blocking-flow.contract-zero-scc"
            | "binary-blocking-flow.inspect-contracted-arc"
            | "binary-blocking-flow.build-lift-adjacency"
            | "binary-blocking-flow.inspect-lift-arc"
            | "binary-blocking-flow.apply-contracted-flow"
            | "binary-blocking-flow.apply-lift-path"
            | "binary-blocking-flow.complete-primitive"
    );
    if !classification_complete {
        projection.base_zero_arcs.clear();
        projection.special_arcs.clear();
        projection.admissible_arcs.clear();
        projection.zero_admissible_arcs.clear();
    }

    if matches!(
        stage,
        FlowBinaryBlockingStageV1::Analyzing | FlowBinaryBlockingStageV1::Analyzed
    ) {
        projection.component_of = (0..snapshot.node_labels.len()).collect();
    }
    Ok(projection)
}

fn bellman_ford_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::BellmanFordSspTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    min_cost_trace_frames(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

fn potential_dijkstra_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::PotentialDijkstraSspTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    min_cost_trace_frames(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

fn primal_dual_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::PrimalDualTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    min_cost_trace_frames(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

fn simple_cycle_canceling_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::SimpleCycleCancelingTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    min_cost_trace_frames(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

fn minimum_mean_cycle_canceling_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::MinimumMeanCycleCancelingTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    min_cost_trace_frames(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

// The projection deliberately keeps boundary validation, reversible event
// construction, metrics, and final certificate publication in one sequence.
#[allow(clippy::too_many_lines)]
fn convex_cost_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    problem: &ConvexCostProblem<'_>,
    run: &flow::ConvexCostTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("convex-cost event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut parent_phase_id = None;
    for (index, event) in run.events.iter().enumerate() {
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("convex-cost event identity overflow"))?;
        let stage = convex_scene_stage(event.stage);
        if event.expanded_event.minimum_granularity == TraceGranularityV1::Phase {
            parent_phase_id = Some(event_id);
        }
        let mut scene = base.clone();
        let mut overlay_snapshot = event.after.clone();
        if event.expanded_event.catalog_id == "minimum-mean-cycle-canceling.inspect-residual-arc" {
            overlay_snapshot.active_cycle.clone_from(&event.focus_arcs);
        }
        let overlay = convex_cost_overlay(graph, problem, &overlay_snapshot, stage)?;
        let final_event = index + 1 == run.events.len();
        let final_labels = if final_event {
            run.result
                .certificate
                .potentials
                .iter()
                .copied()
                .map(Some)
                .collect::<Vec<_>>()
        } else {
            event.after.node_labels.clone()
        };
        scene
            .apply_convex_cost_boundary(
                graph,
                FlowConvexCostBoundary {
                    flows: &event.after.flows,
                    node_labels: &final_labels,
                    search_order: &event.after.search_order,
                    remaining_divergence: &[],
                    overlay,
                    event_id,
                    event_count,
                },
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        set_convex_snapshot_metrics(&mut scene, &event.after);
        let mut active_refs = event
            .focus_arcs
            .iter()
            .chain(&event.after.active_cycle)
            .filter_map(|arc| {
                graph
                    .edges()
                    .get(arc.edge)
                    .map(|edge| FlowTraceEntityRefSceneV1::ResidualArc {
                        edge_id: edge.id().as_str().to_owned(),
                        direction: match arc.direction {
                            flow::ConvexResidualDirection::Forward => "forward",
                            flow::ConvexResidualDirection::Reverse => "reverse",
                        }
                        .to_owned(),
                    })
            })
            .collect::<Vec<_>>();
        active_refs.sort();
        active_refs.dedup();
        scene.trace_event = Some(FlowTraceEventSceneV1 {
            event_id: event_id.to_string(),
            parent_phase_id: (event.expanded_event.minimum_granularity
                != TraceGranularityV1::Phase)
                .then(|| parent_phase_id.map(|value| value.to_string()))
                .flatten(),
            catalog_id: match event.expanded_event.catalog_id.as_str() {
                "minimum-mean-cycle-canceling.start-selector" => {
                    "segment-expanded-convex-mcf.start-selector"
                }
                "minimum-mean-cycle-canceling.inspect-residual-arc" => {
                    "segment-expanded-convex-mcf.inspect-residual-arc"
                }
                "minimum-mean-cycle-canceling.select-minimum-mean-cycle" => {
                    "segment-expanded-convex-mcf.select-minimum-mean-cycle"
                }
                "minimum-mean-cycle-canceling.cancel-minimum-mean-cycle" => {
                    "segment-expanded-convex-mcf.cancel-cycle"
                }
                "minimum-mean-cycle-canceling.optimal" => "segment-expanded-convex-mcf.optimal",
                _ => {
                    return Err(JsError::new(
                        "convex-cost event has unknown expanded catalog identity",
                    ));
                }
            }
            .to_owned(),
            minimum_granularity: event.expanded_event.minimum_granularity,
            pseudocode_line: match event.expanded_event.catalog_id.as_str() {
                "minimum-mean-cycle-canceling.start-selector" => {
                    "segment-expanded-convex-mcf:start-expanded-selector"
                }
                "minimum-mean-cycle-canceling.inspect-residual-arc" => {
                    "segment-expanded-convex-mcf:inspect-marginal-residual-arc"
                }
                "minimum-mean-cycle-canceling.select-minimum-mean-cycle" => {
                    "segment-expanded-convex-mcf:select-marginal-cycle"
                }
                "minimum-mean-cycle-canceling.cancel-minimum-mean-cycle" => {
                    "segment-expanded-convex-mcf:cancel-expanded-cycle"
                }
                "minimum-mean-cycle-canceling.optimal" => {
                    "segment-expanded-convex-mcf:certify-marginal-residual"
                }
                _ => {
                    return Err(JsError::new(
                        "convex-cost event has unknown expanded pseudocode identity",
                    ));
                }
            }
            .to_owned(),
            patch_count: u32::try_from(event.expanded_event.patches.len())
                .map_err(|_| JsError::new("convex-cost patch count overflow"))?,
            entity_refs: active_refs,
            detail: event
                .detail
                .as_ref()
                .map(|(label, value)| FlowTraceEventDetailSceneV1 {
                    label: label.clone(),
                    value: value.to_string(),
                }),
        });
        scene.solve_status = FlowSolveStatusV1::Running;
        if final_event {
            scene.set_convex_cost_outcome(graph, &run.result.certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    if run.events.is_empty() || run.final_snapshot != run.events.last().unwrap().after {
        return Err(JsError::new("convex-cost trace final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn convex_result_snapshot(
    problem: &ConvexCostProblem<'_>,
    result: &flow::ConvexCostResult,
) -> ConvexCostSnapshot {
    let mut segments = Vec::new();
    for (edge, (objective, segment_flows)) in problem
        .edge_costs()
        .iter()
        .zip(&result.segment_flows)
        .enumerate()
    {
        let mut start_flow = 0_u64;
        for (segment, (&flow, objective_segment)) in
            segment_flows.iter().zip(&objective.segments).enumerate()
        {
            segments.push(flow::ConvexSegmentState {
                edge,
                segment,
                start_flow,
                end_flow: objective_segment.end_flow,
                flow,
                marginal_cost: objective_segment.marginal_cost,
            });
            start_flow = objective_segment.end_flow;
        }
    }
    ConvexCostSnapshot {
        flows: result.flows.clone(),
        segments,
        node_labels: result
            .certificate
            .potentials
            .iter()
            .copied()
            .map(Some)
            .collect(),
        search_order: Vec::new(),
        active_cycle: Vec::new(),
        metrics: result.metrics,
    }
}

fn convex_scene_stage(stage: ConvexCostStage) -> FlowConvexCostStageV1 {
    match stage {
        ConvexCostStage::Initialize => FlowConvexCostStageV1::Initialize,
        ConvexCostStage::SelectMinimumMeanCycle => FlowConvexCostStageV1::SelectMinimumMeanCycle,
        ConvexCostStage::CancelCycle => FlowConvexCostStageV1::CancelCycle,
        ConvexCostStage::Optimal => FlowConvexCostStageV1::Optimal,
    }
}

fn set_convex_snapshot_metrics(scene: &mut FlowCurrentSceneV9, snapshot: &ConvexCostSnapshot) {
    scene.set_convex_cost_metrics(
        snapshot.metrics.mean_cycle_searches,
        snapshot.metrics.dynamic_programming_rounds,
        snapshot.metrics.residual_arc_scans,
        snapshot.metrics.canceled_cycles,
    );
}

fn convex_cost_overlay(
    graph: &flow::FlowNetwork,
    problem: &ConvexCostProblem<'_>,
    snapshot: &ConvexCostSnapshot,
    stage: FlowConvexCostStageV1,
) -> Result<FlowConvexCostOverlayV1, JsError> {
    if snapshot.flows.len() != graph.edges().len() {
        return Err(JsError::new("convex-cost snapshot flow shape mismatch"));
    }
    let mut projected_edges = Vec::with_capacity(graph.edges().len());
    let mut segment_cursor = 0_usize;
    for (edge_index, ((edge, objective), &flow)) in graph
        .edges()
        .iter()
        .zip(problem.edge_costs())
        .zip(&snapshot.flows)
        .enumerate()
    {
        let mut projected_segments = Vec::with_capacity(objective.segments.len());
        let mut total_cost = objective.base_cost_at_zero;
        let mut forward_marginal_cost = None;
        let mut reverse_marginal_cost = None;
        for segment_index in 0..objective.segments.len() {
            let segment_state = snapshot
                .segments
                .get(segment_cursor)
                .ok_or_else(|| JsError::new("convex-cost snapshot segment shape mismatch"))?;
            if segment_state.edge != edge_index || segment_state.segment != segment_index {
                return Err(JsError::new(
                    "convex-cost snapshot segment identity mismatch",
                ));
            }
            let length = segment_state.end_flow - segment_state.start_flow;
            total_cost = i128::from(segment_state.flow)
                .checked_mul(i128::from(segment_state.marginal_cost))
                .and_then(|term| total_cost.checked_add(term))
                .ok_or_else(|| JsError::new("convex-cost edge objective overflow"))?;
            if forward_marginal_cost.is_none() && segment_state.flow < length {
                forward_marginal_cost = Some(segment_state.marginal_cost.to_string());
            }
            if segment_state.flow > 0 && flow > edge.lower() {
                reverse_marginal_cost = Some(segment_state.marginal_cost.to_string());
            }
            projected_segments.push(FlowConvexCostSegmentStateV1 {
                segment: segment_index.to_string(),
                start_flow: segment_state.start_flow.to_string(),
                end_flow: segment_state.end_flow.to_string(),
                flow: segment_state.flow.to_string(),
                marginal_cost: segment_state.marginal_cost.to_string(),
            });
            segment_cursor += 1;
        }
        projected_edges.push(FlowConvexCostEdgeStateV1 {
            edge_id: edge.id().as_str().to_owned(),
            base_cost_at_zero: objective.base_cost_at_zero.to_string(),
            flow: flow.to_string(),
            total_cost: total_cost.to_string(),
            forward_marginal_cost,
            reverse_marginal_cost,
            segments: projected_segments,
        });
    }
    if segment_cursor != snapshot.segments.len() {
        return Err(JsError::new("convex-cost snapshot has trailing segments"));
    }
    let active_cycle = snapshot
        .active_cycle
        .iter()
        .map(|arc| {
            let edge = graph
                .edges()
                .get(arc.edge)
                .ok_or_else(|| JsError::new("convex-cost active edge mismatch"))?;
            Ok(FlowConvexCostArcRefV1 {
                edge_id: edge.id().as_str().to_owned(),
                segment: arc.segment.to_string(),
                direction: match arc.direction {
                    flow::ConvexResidualDirection::Forward => "forward",
                    flow::ConvexResidualDirection::Reverse => "reverse",
                }
                .to_owned(),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowConvexCostOverlayV1 {
        stage,
        scale: None,
        edges: projected_edges,
        active_cycle,
        eligible_arcs: Vec::new(),
    })
}

fn convex_cost_scaling_fast_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    problem: &ConvexCostProblem<'_>,
    result: &flow::ConvexCostScalingResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let snapshot = convex_cost_scaling_result_snapshot(problem, result)?;
    let mut scene = ready_flow_scene(scenario)?;
    apply_convex_cost_scaling_scene_boundary(
        &mut scene,
        graph,
        problem,
        &snapshot,
        FlowConvexCostStageV1::Optimal,
        0,
        0,
    )?;
    scene.set_convex_cost_scaling_metrics(result.metrics);
    scene.set_convex_cost_outcome(graph, &result.certificate);
    Ok(vec![ready_flow_scene(scenario)?, scene])
}

#[allow(clippy::too_many_lines)]
fn convex_cost_scaling_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    problem: &ConvexCostProblem<'_>,
    run: &flow::ConvexCostScalingTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("convex-cost scaling event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    let mut parent_phase_id = None;
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("convex-cost scaling trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("convex-cost scaling event identity overflow"))?;
        if event.stage == ConvexCostScalingStage::StartScale {
            parent_phase_id = Some(event_id);
        }
        let mut scene = base.clone();
        apply_convex_cost_scaling_scene_boundary(
            &mut scene,
            graph,
            problem,
            &event.after,
            convex_cost_scaling_scene_stage(event.stage),
            event_id,
            event_count,
        )?;
        scene.set_convex_cost_scaling_metrics(event.after.metrics);
        let path_entity_refs = event
            .after
            .active_path
            .iter()
            .map(|arc| {
                let edge = graph
                    .edges()
                    .get(arc.edge)
                    .ok_or_else(|| JsError::new("convex-cost scaling path edge mismatch"))?;
                Ok(FlowTraceEntityRefSceneV1::ResidualArc {
                    edge_id: edge.id().as_str().to_owned(),
                    direction: convex_direction(arc.direction).to_owned(),
                })
            })
            .collect::<Result<Vec<_>, JsError>>()?;
        let entity_refs = match event.stage {
            ConvexCostScalingStage::UpdatePotentials => {
                // The final marginal arc identifies the deficit vertex whose
                // shortest-path distance is the exact dual-update cutoff. A
                // zero cutoff is still a completed comparison, so publish that
                // one graph owner instead of flashing the whole active path or
                // emitting an apparently targetless operation.
                let arc =
                    event.after.active_path.last().ok_or_else(|| {
                        JsError::new("convex dual update omitted its active path")
                    })?;
                let edge = graph
                    .edges()
                    .get(arc.edge)
                    .ok_or_else(|| JsError::new("convex dual update edge mismatch"))?;
                let cutoff_node = match arc.direction {
                    flow::ConvexResidualDirection::Forward => edge.to(),
                    flow::ConvexResidualDirection::Reverse => edge.from(),
                };
                let node = graph
                    .node(cutoff_node)
                    .ok_or_else(|| JsError::new("convex dual update node mismatch"))?;
                vec![FlowTraceEntityRefSceneV1::Node {
                    node_id: node.id().as_str().to_owned(),
                }]
            }
            ConvexCostScalingStage::ShortestPath | ConvexCostScalingStage::Augment => {
                // The convex layer already renders the complete ordered marginal
                // path, segment ordinals, and directions. Generic focus would
                // duplicate a long path as an undifferentiated graph-wide flash.
                Vec::new()
            }
            _ => path_entity_refs,
        };
        scene.trace_event = Some(FlowTraceEventSceneV1 {
            event_id: event_id.to_string(),
            parent_phase_id: (event.stage != ConvexCostScalingStage::StartScale)
                .then(|| parent_phase_id.map(|value| value.to_string()))
                .flatten(),
            catalog_id: convex_cost_scaling_catalog_id(event.stage).to_owned(),
            minimum_granularity: match event.stage {
                ConvexCostScalingStage::StartScale => TraceGranularityV1::Phase,
                ConvexCostScalingStage::InspectMarginalArc
                | ConvexCostScalingStage::ShortestPath => TraceGranularityV1::Micro,
                _ => TraceGranularityV1::Operation,
            },
            pseudocode_line: convex_cost_scaling_pseudocode_line(event.stage).to_owned(),
            patch_count: convex_cost_scaling_patch_count(event)?,
            entity_refs,
            detail: event
                .detail
                .map(|(label, value)| FlowTraceEventDetailSceneV1 {
                    label: label.to_owned(),
                    value: value.to_string(),
                }),
        });
        scene.solve_status = FlowSolveStatusV1::Running;
        if event.stage == ConvexCostScalingStage::Optimal {
            scene.set_convex_cost_outcome(graph, &run.result.certificate);
        }
        current = event.after.clone();
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    if current != run.final_snapshot || run.final_snapshot.flows != run.result.flows {
        return Err(JsError::new("convex-cost scaling final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn apply_convex_cost_scaling_scene_boundary(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    problem: &ConvexCostProblem<'_>,
    snapshot: &ConvexCostScalingSnapshot,
    stage: FlowConvexCostStageV1,
    event_id: u64,
    event_count: u64,
) -> Result<(), JsError> {
    let overlay = convex_cost_scaling_overlay(graph, problem, snapshot, stage)?;
    let labels = snapshot
        .potentials
        .iter()
        .copied()
        .map(Some)
        .collect::<Vec<_>>();
    let search_order = snapshot
        .search_order
        .iter()
        .map(|&node| {
            graph
                .node(node)
                .map(|entry| entry.id().clone())
                .ok_or_else(|| JsError::new("convex-cost scaling search node mismatch"))
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    scene
        .apply_convex_cost_boundary(
            graph,
            FlowConvexCostBoundary {
                flows: &snapshot.flows,
                node_labels: &labels,
                search_order: &search_order,
                remaining_divergence: &snapshot.remaining_divergence,
                overlay,
                event_id,
                event_count,
            },
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn convex_cost_scaling_result_snapshot(
    problem: &ConvexCostProblem<'_>,
    result: &flow::ConvexCostScalingResult,
) -> Result<ConvexCostScalingSnapshot, JsError> {
    let mut segments = Vec::new();
    for (edge, (objective, segment_flows)) in problem
        .edge_costs()
        .iter()
        .zip(&result.segment_flows)
        .enumerate()
    {
        let mut start = 0_u64;
        for (segment, (&flow, piece)) in segment_flows.iter().zip(&objective.segments).enumerate() {
            segments.push(flow::ConvexSegmentState {
                edge,
                segment,
                start_flow: start,
                end_flow: piece.end_flow,
                flow,
                marginal_cost: piece.marginal_cost,
            });
            start = piece.end_flow;
        }
    }
    if segments.len()
        != problem
            .edge_costs()
            .iter()
            .map(|objective| objective.segments.len())
            .sum::<usize>()
    {
        return Err(JsError::new(
            "convex-cost scaling result segment shape mismatch",
        ));
    }
    Ok(ConvexCostScalingSnapshot {
        flows: result.flows.clone(),
        segments,
        potentials: result.certificate.potentials.clone(),
        remaining_divergence: vec![0; problem.graph().nodes().len()],
        search_order: Vec::new(),
        active_path: Vec::new(),
        scale: 1,
        metrics: result.metrics,
    })
}

fn convex_cost_scaling_overlay(
    graph: &flow::FlowNetwork,
    problem: &ConvexCostProblem<'_>,
    snapshot: &ConvexCostScalingSnapshot,
    stage: FlowConvexCostStageV1,
) -> Result<FlowConvexCostOverlayV1, JsError> {
    let projected = ConvexCostSnapshot {
        flows: snapshot.flows.clone(),
        segments: snapshot.segments.clone(),
        node_labels: snapshot.potentials.iter().copied().map(Some).collect(),
        search_order: snapshot
            .search_order
            .iter()
            .map(|&node| {
                graph
                    .node(node)
                    .map(|entry| entry.id().clone())
                    .ok_or_else(|| JsError::new("convex-cost scaling search node mismatch"))
            })
            .collect::<Result<Vec<_>, JsError>>()?,
        active_cycle: snapshot.active_path.clone(),
        metrics: flow::MinimumMeanCycleCancelingMetrics::default(),
    };
    let mut overlay = convex_cost_overlay(graph, problem, &projected, stage)?;
    overlay.scale = Some(snapshot.scale.to_string());
    overlay.eligible_arcs = convex_cost_scaling_eligible_arcs(graph, snapshot)?;
    Ok(overlay)
}

fn convex_cost_scaling_eligible_arcs(
    graph: &flow::FlowNetwork,
    snapshot: &ConvexCostScalingSnapshot,
) -> Result<Vec<FlowConvexCostArcRefV1>, JsError> {
    let mut by_edge = vec![Vec::new(); graph.edges().len()];
    for segment in &snapshot.segments {
        let states = by_edge
            .get_mut(segment.edge)
            .ok_or_else(|| JsError::new("convex-cost scaling segment edge mismatch"))?;
        states.push(segment);
    }
    let mut eligible = Vec::new();
    for (edge_index, (edge, segments)) in graph.edges().iter().zip(by_edge).enumerate() {
        let aggregate = *snapshot
            .flows
            .get(edge_index)
            .ok_or_else(|| JsError::new("convex-cost scaling flow shape mismatch"))?;
        if let Some(segment) = segments
            .iter()
            .find(|segment| segment.flow < segment.end_flow - segment.start_flow)
        {
            let residual = segment.end_flow - segment.start_flow - segment.flow;
            if residual >= snapshot.scale {
                eligible.push(FlowConvexCostArcRefV1 {
                    edge_id: edge.id().as_str().to_owned(),
                    segment: segment.segment.to_string(),
                    direction: "forward".to_owned(),
                });
            }
        }
        if aggregate > edge.lower() {
            let segment = segments
                .iter()
                .rev()
                .find(|segment| segment.flow > 0)
                .ok_or_else(|| JsError::new("convex-cost scaling reverse segment missing"))?;
            let removable = aggregate - segment.start_flow.max(edge.lower());
            if removable >= snapshot.scale {
                eligible.push(FlowConvexCostArcRefV1 {
                    edge_id: edge.id().as_str().to_owned(),
                    segment: segment.segment.to_string(),
                    direction: "reverse".to_owned(),
                });
            }
        }
    }
    Ok(eligible)
}

const fn convex_cost_scaling_scene_stage(stage: ConvexCostScalingStage) -> FlowConvexCostStageV1 {
    match stage {
        ConvexCostScalingStage::Initialize => FlowConvexCostStageV1::Initialize,
        ConvexCostScalingStage::StartScale => FlowConvexCostStageV1::StartScale,
        ConvexCostScalingStage::SaturateMarginal => FlowConvexCostStageV1::SaturateMarginal,
        ConvexCostScalingStage::InspectMarginalArc => FlowConvexCostStageV1::InspectMarginalArc,
        ConvexCostScalingStage::ShortestPath => FlowConvexCostStageV1::ShortestPath,
        ConvexCostScalingStage::UpdatePotentials => FlowConvexCostStageV1::UpdatePotentials,
        ConvexCostScalingStage::Augment => FlowConvexCostStageV1::Augment,
        ConvexCostScalingStage::CompleteScale => FlowConvexCostStageV1::CompleteScale,
        ConvexCostScalingStage::Optimal => FlowConvexCostStageV1::Optimal,
    }
}

const fn convex_cost_scaling_catalog_id(stage: ConvexCostScalingStage) -> &'static str {
    match stage {
        ConvexCostScalingStage::Initialize => "convex-cost-scaling.initialize-marginal-residual",
        ConvexCostScalingStage::StartScale => "convex-cost-scaling.start-delta-scale",
        ConvexCostScalingStage::SaturateMarginal => {
            "convex-cost-scaling.saturate-negative-eligible-marginal"
        }
        ConvexCostScalingStage::InspectMarginalArc => {
            "convex-cost-scaling.inspect-marginal-residual-arc"
        }
        ConvexCostScalingStage::ShortestPath => {
            "convex-cost-scaling.shortest-marginal-residual-path"
        }
        ConvexCostScalingStage::UpdatePotentials => {
            "convex-cost-scaling.update-reduced-cost-potentials"
        }
        ConvexCostScalingStage::Augment => "convex-cost-scaling.augment-to-breakpoint",
        ConvexCostScalingStage::CompleteScale => "convex-cost-scaling.complete-delta-scale",
        ConvexCostScalingStage::Optimal => "convex-cost-scaling.certify-expanded-oracle",
    }
}

const fn convex_cost_scaling_pseudocode_line(stage: ConvexCostScalingStage) -> &'static str {
    match stage {
        ConvexCostScalingStage::Initialize => "convex-cost-scaling:initialize-prefix",
        ConvexCostScalingStage::StartScale => "convex-cost-scaling:begin-delta-phase",
        ConvexCostScalingStage::SaturateMarginal => {
            "convex-cost-scaling:saturate-negative-marginal"
        }
        ConvexCostScalingStage::InspectMarginalArc => {
            "convex-cost-scaling:inspect-one-marginal-residual-arc"
        }
        ConvexCostScalingStage::ShortestPath => "convex-cost-scaling:dijkstra-on-boundary-segments",
        ConvexCostScalingStage::UpdatePotentials => "convex-cost-scaling:update-potentials",
        ConvexCostScalingStage::Augment => "convex-cost-scaling:augment-to-next-breakpoint",
        ConvexCostScalingStage::CompleteScale => "convex-cost-scaling:halve-delta",
        ConvexCostScalingStage::Optimal => "convex-cost-scaling:compare-expanded-oracle",
    }
}

fn convex_cost_scaling_patch_count(event: &ConvexCostScalingTraceEvent) -> Result<u32, JsError> {
    let scalar_changes = event
        .before
        .flows
        .iter()
        .zip(&event.after.flows)
        .filter(|(before, after)| before != after)
        .count()
        + event
            .before
            .potentials
            .iter()
            .zip(&event.after.potentials)
            .filter(|(before, after)| before != after)
            .count()
        + event
            .before
            .remaining_divergence
            .iter()
            .zip(&event.after.remaining_divergence)
            .filter(|(before, after)| before != after)
            .count();
    let structural_changes = usize::from(event.before.scale != event.after.scale)
        + usize::from(event.before.search_order != event.after.search_order)
        + usize::from(event.before.active_path != event.after.active_path)
        + usize::from(event.before.metrics != event.after.metrics);
    u32::try_from(scalar_changes + structural_changes)
        .map_err(|_| JsError::new("convex-cost scaling patch count overflow"))
}

fn convex_network_simplex_fast_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    problem: &ConvexCostProblem<'_>,
    result: &flow::ConvexNetworkSimplexResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let mut scene = ready_flow_scene(scenario)?;
    apply_convex_network_simplex_scene_boundary(
        &mut scene,
        graph,
        problem,
        &result.final_snapshot,
        FlowConvexNetworkSimplexStageV1::Optimal,
        result.artificial_cost,
        0,
        0,
    )?;
    scene.set_convex_network_simplex_metrics(result.metrics);
    scene.set_convex_cost_outcome(graph, &result.certificate);
    Ok(vec![ready_flow_scene(scenario)?, scene])
}

fn convex_network_simplex_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    problem: &ConvexCostProblem<'_>,
    run: &flow::ConvexNetworkSimplexTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("convex network-simplex event count overflow"))?;
    let artificial_cost = run.result.artificial_cost;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    let mut parent_pivot_id = None;
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("convex network-simplex trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("convex network-simplex event identity overflow"))?;
        if event.stage == ConvexNetworkSimplexStage::FormCycle {
            parent_pivot_id = Some(event_id);
        }
        let mut scene = base.clone();
        apply_convex_network_simplex_scene_boundary(
            &mut scene,
            graph,
            problem,
            &event.after,
            convex_network_simplex_scene_stage(event.stage),
            artificial_cost,
            event_id,
            event_count,
        )?;
        scene.set_convex_network_simplex_metrics(event.after.metrics);
        scene.trace_event = Some(FlowTraceEventSceneV1 {
            event_id: event_id.to_string(),
            parent_phase_id: (event.stage != ConvexNetworkSimplexStage::FormCycle)
                .then(|| parent_pivot_id.map(|value| value.to_string()))
                .flatten(),
            catalog_id: convex_network_simplex_catalog_id(event.stage).to_owned(),
            minimum_granularity: match event.stage {
                ConvexNetworkSimplexStage::FormCycle => TraceGranularityV1::Phase,
                ConvexNetworkSimplexStage::Price => TraceGranularityV1::Micro,
                _ => TraceGranularityV1::Operation,
            },
            pseudocode_line: convex_network_simplex_pseudocode_line(event.stage).to_owned(),
            patch_count: convex_network_simplex_patch_count(event)?,
            entity_refs: convex_network_simplex_entity_refs(graph, &event.after)?,
            detail: event
                .detail
                .map(|(label, value)| FlowTraceEventDetailSceneV1 {
                    label: label.to_owned(),
                    value: value.to_string(),
                }),
        });
        scene.solve_status = FlowSolveStatusV1::Running;
        if event.stage == ConvexNetworkSimplexStage::Optimal {
            scene.set_convex_cost_outcome(graph, &run.result.certificate);
        }
        current = event.after.clone();
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    if current != run.final_snapshot || run.final_snapshot != run.result.final_snapshot {
        return Err(JsError::new(
            "convex network-simplex final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

#[allow(clippy::too_many_arguments)]
fn apply_convex_network_simplex_scene_boundary(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    problem: &ConvexCostProblem<'_>,
    snapshot: &ConvexNetworkSimplexSnapshot,
    stage: FlowConvexNetworkSimplexStageV1,
    artificial_cost: i128,
    event_id: u64,
    event_count: u64,
) -> Result<(), JsError> {
    let convex_overlay = convex_network_simplex_cost_overlay(graph, problem, snapshot, stage)?;
    let simplex_overlay = convex_network_simplex_overlay(graph, snapshot, stage, artificial_cost)?;
    let labels = snapshot
        .potentials
        .iter()
        .take(graph.nodes().len())
        .copied()
        .map(Some)
        .collect::<Vec<_>>();
    let applied = scene.apply_convex_network_simplex_boundary(
        graph,
        FlowConvexCostBoundary {
            flows: &snapshot.flows,
            node_labels: &labels,
            search_order: &[],
            remaining_divergence: &[],
            overlay: convex_overlay,
            event_id,
            event_count,
        },
        simplex_overlay,
    );
    applied.map_err(|error| JsError::new(&error.to_string()))
}

fn convex_network_simplex_cost_overlay(
    graph: &flow::FlowNetwork,
    problem: &ConvexCostProblem<'_>,
    snapshot: &ConvexNetworkSimplexSnapshot,
    stage: FlowConvexNetworkSimplexStageV1,
) -> Result<FlowConvexCostOverlayV1, JsError> {
    let projected = ConvexCostSnapshot {
        flows: snapshot.flows.clone(),
        segments: snapshot.segments.clone(),
        node_labels: snapshot
            .potentials
            .iter()
            .take(graph.nodes().len())
            .copied()
            .map(Some)
            .collect(),
        search_order: Vec::new(),
        active_cycle: Vec::new(),
        metrics: flow::MinimumMeanCycleCancelingMetrics::default(),
    };
    convex_cost_overlay(
        graph,
        problem,
        &projected,
        if stage == FlowConvexNetworkSimplexStageV1::Optimal {
            FlowConvexCostStageV1::Optimal
        } else {
            FlowConvexCostStageV1::Initialize
        },
    )
}

fn convex_network_simplex_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &ConvexNetworkSimplexSnapshot,
    stage: FlowConvexNetworkSimplexStageV1,
    artificial_cost: i128,
) -> Result<FlowConvexNetworkSimplexOverlayV1, JsError> {
    if snapshot.potentials.len() != graph.nodes().len() + 1
        || snapshot.parents.len() != graph.nodes().len() + 1
        || snapshot.edges.len() != graph.edges().len()
        || snapshot.artificial_edges.len() != graph.nodes().len()
    {
        return Err(JsError::new(
            "convex network-simplex snapshot shape mismatch",
        ));
    }
    let nodes = snapshot
        .potentials
        .iter()
        .zip(&snapshot.parents)
        .enumerate()
        .map(|(node, (&potential, &parent))| {
            Ok(FlowConvexNetworkSimplexNodeStateV1 {
                entity_id: convex_simplex_node_entity(graph, node)?.to_owned(),
                potential: potential.to_string(),
                parent: parent
                    .map(|value| convex_simplex_node_entity(graph, value).map(str::to_owned))
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let cycle_entities = snapshot
        .active_cycle
        .iter()
        .map(|reference| convex_simplex_arc_entity(graph, reference))
        .collect::<Result<BTreeSet<_>, JsError>>()?;
    let entering_entity = snapshot
        .entering
        .as_ref()
        .map(|reference| convex_simplex_arc_entity(graph, reference))
        .transpose()?;
    let leaving_entity = snapshot
        .leaving
        .as_ref()
        .map(|reference| convex_simplex_arc_entity(graph, reference))
        .transpose()?;
    let edges = project_convex_simplex_edges(
        graph,
        snapshot,
        &cycle_entities,
        entering_entity.as_deref(),
        leaving_entity.as_deref(),
    )?;
    let artificial_edges = project_convex_simplex_artificial_edges(
        graph,
        snapshot,
        &cycle_entities,
        entering_entity.as_deref(),
        leaving_entity.as_deref(),
    )?;
    Ok(FlowConvexNetworkSimplexOverlayV1 {
        stage,
        artificial_cost: artificial_cost.to_string(),
        nodes,
        edges,
        artificial_edges,
        entering: snapshot
            .entering
            .as_ref()
            .map(|reference| convex_simplex_arc_ref(graph, reference))
            .transpose()?,
        leaving: snapshot
            .leaving
            .as_ref()
            .map(|reference| convex_simplex_arc_ref(graph, reference))
            .transpose()?,
        cycle: snapshot
            .active_cycle
            .iter()
            .map(|reference| convex_simplex_arc_ref(graph, reference))
            .collect::<Result<Vec<_>, JsError>>()?,
    })
}

fn project_convex_simplex_edges(
    graph: &flow::FlowNetwork,
    snapshot: &ConvexNetworkSimplexSnapshot,
    cycle: &BTreeSet<String>,
    entering: Option<&str>,
    leaving: Option<&str>,
) -> Result<Vec<FlowConvexNetworkSimplexEdgeStateV1>, JsError> {
    snapshot
        .edges
        .iter()
        .zip(graph.edges())
        .enumerate()
        .map(|(index, (state, edge))| {
            if state.edge != index {
                return Err(JsError::new(
                    "convex network-simplex edge identity mismatch",
                ));
            }
            let entity = edge.id().as_str();
            Ok(FlowConvexNetworkSimplexEdgeStateV1 {
                edge_id: entity.to_owned(),
                basis: convex_simplex_basis(state.basis),
                active_segment: state.active_segment.map(|value| value.to_string()),
                in_cycle: cycle.contains(entity),
                entering: entering == Some(entity),
                leaving: leaving == Some(entity),
            })
        })
        .collect()
}

fn project_convex_simplex_artificial_edges(
    graph: &flow::FlowNetwork,
    snapshot: &ConvexNetworkSimplexSnapshot,
    cycle: &BTreeSet<String>,
    entering: Option<&str>,
    leaving: Option<&str>,
) -> Result<Vec<FlowConvexNetworkSimplexArtificialEdgeV1>, JsError> {
    snapshot
        .artificial_edges
        .iter()
        .zip(graph.nodes())
        .enumerate()
        .map(|(index, (state, node))| {
            if state.node != index {
                return Err(JsError::new(
                    "convex network-simplex artificial identity mismatch",
                ));
            }
            let entity = format!("artificial:{}", node.id().as_str());
            Ok(FlowConvexNetworkSimplexArtificialEdgeV1 {
                entity_id: entity.clone(),
                node_id: node.id().as_str().to_owned(),
                source: convex_simplex_node_entity(graph, state.source)?.to_owned(),
                target: convex_simplex_node_entity(graph, state.target)?.to_owned(),
                flow: state.flow.to_string(),
                basis: if state.tree {
                    FlowConvexNetworkSimplexBasisV1::Tree
                } else {
                    FlowConvexNetworkSimplexBasisV1::Breakpoint
                },
                in_cycle: cycle.contains(&entity),
                entering: entering == Some(entity.as_str()),
                leaving: leaving == Some(entity.as_str()),
            })
        })
        .collect()
}

fn convex_simplex_arc_ref(
    graph: &flow::FlowNetwork,
    reference: &flow::ConvexNetworkSimplexArcRef,
) -> Result<FlowConvexNetworkSimplexArcRefV1, JsError> {
    let (entity_id, segment, direction) = match reference {
        flow::ConvexNetworkSimplexArcRef::Original {
            edge,
            segment,
            direction,
        } => (
            graph
                .edges()
                .get(*edge)
                .ok_or_else(|| JsError::new("convex network-simplex arc edge mismatch"))?
                .id()
                .as_str()
                .to_owned(),
            Some(segment.to_string()),
            convex_direction(*direction).to_owned(),
        ),
        flow::ConvexNetworkSimplexArcRef::Artificial { node, direction } => (
            format!(
                "artificial:{}",
                graph
                    .nodes()
                    .get(*node)
                    .ok_or_else(|| JsError::new("convex network-simplex arc node mismatch"))?
                    .id()
                    .as_str()
            ),
            None,
            convex_direction(*direction).to_owned(),
        ),
    };
    Ok(FlowConvexNetworkSimplexArcRefV1 {
        entity_id,
        segment,
        direction,
    })
}

fn convex_simplex_arc_entity(
    graph: &flow::FlowNetwork,
    reference: &flow::ConvexNetworkSimplexArcRef,
) -> Result<String, JsError> {
    match reference {
        flow::ConvexNetworkSimplexArcRef::Original { edge, .. } => graph
            .edges()
            .get(*edge)
            .map(|edge| edge.id().as_str().to_owned())
            .ok_or_else(|| JsError::new("convex network-simplex arc edge mismatch")),
        flow::ConvexNetworkSimplexArcRef::Artificial { node, .. } => {
            let node = graph
                .nodes()
                .get(*node)
                .ok_or_else(|| JsError::new("convex network-simplex arc node mismatch"))?;
            Ok(format!("artificial:{}", node.id().as_str()))
        }
    }
}

fn convex_simplex_node_entity(graph: &flow::FlowNetwork, node: usize) -> Result<&str, JsError> {
    if node == graph.nodes().len() {
        return Ok("artificial-root");
    }
    graph
        .nodes()
        .get(node)
        .map(|node| node.id().as_str())
        .ok_or_else(|| JsError::new("convex network-simplex node mismatch"))
}

const fn convex_simplex_basis(
    basis: flow::ConvexNetworkSimplexBasisState,
) -> FlowConvexNetworkSimplexBasisV1 {
    match basis {
        flow::ConvexNetworkSimplexBasisState::Tree => FlowConvexNetworkSimplexBasisV1::Tree,
        flow::ConvexNetworkSimplexBasisState::Breakpoint => {
            FlowConvexNetworkSimplexBasisV1::Breakpoint
        }
    }
}

const fn convex_network_simplex_scene_stage(
    stage: ConvexNetworkSimplexStage,
) -> FlowConvexNetworkSimplexStageV1 {
    match stage {
        ConvexNetworkSimplexStage::InitializeBasis => {
            FlowConvexNetworkSimplexStageV1::InitializeBasis
        }
        ConvexNetworkSimplexStage::Price => FlowConvexNetworkSimplexStageV1::Price,
        ConvexNetworkSimplexStage::FormCycle => FlowConvexNetworkSimplexStageV1::FormCycle,
        ConvexNetworkSimplexStage::CrossBreakpoint => {
            FlowConvexNetworkSimplexStageV1::CrossBreakpoint
        }
        ConvexNetworkSimplexStage::ExchangeBasis => FlowConvexNetworkSimplexStageV1::ExchangeBasis,
        ConvexNetworkSimplexStage::FlipBound => FlowConvexNetworkSimplexStageV1::FlipBound,
        ConvexNetworkSimplexStage::Optimal => FlowConvexNetworkSimplexStageV1::Optimal,
    }
}

const fn convex_network_simplex_catalog_id(stage: ConvexNetworkSimplexStage) -> &'static str {
    match stage {
        ConvexNetworkSimplexStage::InitializeBasis => {
            "convex-network-simplex.initialize-compact-basis"
        }
        ConvexNetworkSimplexStage::Price => "convex-network-simplex.price-forward-backward",
        ConvexNetworkSimplexStage::FormCycle => "convex-network-simplex.form-fundamental-cycle",
        ConvexNetworkSimplexStage::CrossBreakpoint => {
            "convex-network-simplex.cross-segment-breakpoint"
        }
        ConvexNetworkSimplexStage::ExchangeBasis => "convex-network-simplex.exchange-basis",
        ConvexNetworkSimplexStage::FlipBound => "convex-network-simplex.flip-entering-bound",
        ConvexNetworkSimplexStage::Optimal => "convex-network-simplex.certify-expanded-oracle",
    }
}

const fn convex_network_simplex_pseudocode_line(stage: ConvexNetworkSimplexStage) -> &'static str {
    match stage {
        ConvexNetworkSimplexStage::InitializeBasis => {
            "convex-network-simplex:initialize-big-m-star"
        }
        ConvexNetworkSimplexStage::Price => "convex-network-simplex:price-both-directions",
        ConvexNetworkSimplexStage::FormCycle => "convex-network-simplex:form-fundamental-cycle",
        ConvexNetworkSimplexStage::CrossBreakpoint => {
            "convex-network-simplex:cross-next-breakpoint"
        }
        ConvexNetworkSimplexStage::ExchangeBasis => "convex-network-simplex:exchange-once",
        ConvexNetworkSimplexStage::FlipBound => "convex-network-simplex:retain-at-breakpoint",
        ConvexNetworkSimplexStage::Optimal => "convex-network-simplex:compare-expanded-oracle",
    }
}

fn convex_network_simplex_entity_refs(
    graph: &flow::FlowNetwork,
    snapshot: &ConvexNetworkSimplexSnapshot,
) -> Result<Vec<FlowTraceEntityRefSceneV1>, JsError> {
    snapshot
        .active_cycle
        .iter()
        .filter_map(|reference| match reference {
            flow::ConvexNetworkSimplexArcRef::Original {
                edge, direction, ..
            } => Some(
                graph
                    .edges()
                    .get(*edge)
                    .map(|entry| FlowTraceEntityRefSceneV1::ResidualArc {
                        edge_id: entry.id().as_str().to_owned(),
                        direction: convex_direction(*direction).to_owned(),
                    })
                    .ok_or_else(|| JsError::new("convex network-simplex trace edge mismatch")),
            ),
            flow::ConvexNetworkSimplexArcRef::Artificial { .. } => None,
        })
        .collect()
}

fn convex_network_simplex_patch_count(
    event: &ConvexNetworkSimplexTraceEvent,
) -> Result<u32, JsError> {
    let scalar_changes = event
        .before
        .flows
        .iter()
        .zip(&event.after.flows)
        .filter(|(before, after)| before != after)
        .count()
        + event
            .before
            .potentials
            .iter()
            .zip(&event.after.potentials)
            .filter(|(before, after)| before != after)
            .count();
    let structural_changes = usize::from(event.before.parents != event.after.parents)
        + usize::from(event.before.edges != event.after.edges)
        + usize::from(event.before.artificial_edges != event.after.artificial_edges)
        + usize::from(event.before.active_cycle != event.after.active_cycle)
        + usize::from(event.before.entering != event.after.entering)
        + usize::from(event.before.leaving != event.after.leaving)
        + usize::from(event.before.metrics != event.after.metrics);
    u32::try_from(scalar_changes + structural_changes)
        .map_err(|_| JsError::new("convex network-simplex patch count overflow"))
}

const fn convex_direction(direction: flow::ConvexResidualDirection) -> &'static str {
    match direction {
        flow::ConvexResidualDirection::Forward => "forward",
        flow::ConvexResidualDirection::Reverse => "reverse",
    }
}

fn cancel_tighten_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::CancelTightenTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("cancel-and-tighten event count overflow"))?;
    let ready = ready_flow_scene(scenario)?;
    let mut projected = ready.clone();
    let base_overlay = cancel_tighten_overlay(graph, &run.base_snapshot, None)?;
    projected
        .apply_cancel_tighten_boundary(
            graph,
            &run.base_snapshot.flows,
            base_overlay,
            0,
            event_count,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    let base = flow_only_source_base(ready, &projected, event_count);
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    let mut parent_phase_id = None;
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("cancel-and-tighten trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("cancel-and-tighten event identity overflow"))?;
        if event.after.stage == CancelTightenStage::BeginPhase {
            parent_phase_id = Some(event_id);
        }
        let mut scene = base.clone();
        let overlay = cancel_tighten_overlay(graph, &event.after, event.delta)?;
        scene
            .apply_cancel_tighten_boundary(
                graph,
                &event.after.flows,
                overlay,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(cancel_tighten_trace_event_scene(
            graph,
            event,
            event_id,
            parent_phase_id,
        )?);
        set_cancel_tighten_metrics(&mut scene, &event.after);
        if index + 1 == run.events.len() {
            scene.set_min_cost_flow_outcome(graph, &run.result.certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new(
            "cancel-and-tighten trace final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn cancel_tighten_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &CancelTightenSnapshot,
    delta: Option<u64>,
) -> Result<FlowCancelTightenOverlayV1, JsError> {
    if snapshot.potentials.len() != graph.nodes().len()
        || snapshot.ranks.len() != graph.nodes().len()
    {
        return Err(JsError::new(
            "cancel-and-tighten overlay shape does not match graph",
        ));
    }
    let nodes = graph
        .nodes()
        .iter()
        .zip(&snapshot.potentials)
        .zip(&snapshot.ranks)
        .map(|((node, potential), rank)| FlowCancelTightenNodeStateV1 {
            node_id: node.id().as_str().to_owned(),
            potential: cancel_tighten_rational(potential),
            rank: rank.map(|value| value.to_string()),
        })
        .collect();
    Ok(FlowCancelTightenOverlayV1 {
        stage: cancel_tighten_scene_stage(snapshot.stage),
        epsilon: cancel_tighten_rational(&snapshot.epsilon),
        phase: snapshot.phase.to_string(),
        nodes,
        admissible_arcs: cancel_tighten_residual_refs(&snapshot.admissible_arcs),
        active_cycle: cancel_tighten_residual_refs(&snapshot.active_cycle),
        inspected_arcs: snapshot
            .inspected_arc
            .as_ref()
            .map(std::slice::from_ref)
            .map(cancel_tighten_residual_refs)
            .unwrap_or_default(),
        delta: delta.map(|value| value.to_string()),
    })
}

fn cancel_tighten_rational(value: &CancelTightenRational) -> FlowRationalV1 {
    FlowRationalV1 {
        numerator: value.numerator().to_string(),
        denominator: value.denominator().to_string(),
    }
}

const fn cancel_tighten_scene_stage(stage: CancelTightenStage) -> FlowCancelTightenStageV1 {
    match stage {
        CancelTightenStage::Ready => FlowCancelTightenStageV1::Ready,
        CancelTightenStage::Initialize => FlowCancelTightenStageV1::Initialize,
        CancelTightenStage::BeginPhase => FlowCancelTightenStageV1::BeginPhase,
        CancelTightenStage::InspectCycleArc => FlowCancelTightenStageV1::InspectCycleArc,
        CancelTightenStage::SelectCycle => FlowCancelTightenStageV1::SelectCycle,
        CancelTightenStage::CancelCycle => FlowCancelTightenStageV1::CancelCycle,
        CancelTightenStage::InspectRankArc => FlowCancelTightenStageV1::InspectRankArc,
        CancelTightenStage::Tighten => FlowCancelTightenStageV1::Tighten,
        CancelTightenStage::Optimal => FlowCancelTightenStageV1::Optimal,
    }
}

fn cancel_tighten_residual_refs(ids: &[ResidualArcId]) -> Vec<FlowResidualArcRefV1> {
    ids.iter()
        .map(|id| FlowResidualArcRefV1 {
            edge_id: id.original_edge().as_str().to_owned(),
            direction: match id.direction() {
                ResidualDirection::Forward => "forward",
                ResidualDirection::Reverse => "reverse",
            }
            .to_owned(),
        })
        .collect()
}

fn cancel_tighten_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &CancelTightenTraceEvent,
    event_id: u64,
    parent_phase_id: Option<u64>,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let residual_ref = |arc: &ResidualArcId| FlowTraceEntityRefSceneV1::ResidualArc {
        edge_id: arc.original_edge().as_str().to_owned(),
        direction: match arc.direction() {
            ResidualDirection::Forward => "forward",
            ResidualDirection::Reverse => "reverse",
        }
        .to_owned(),
    };
    let entity_refs = match event.after.stage {
        CancelTightenStage::InspectCycleArc | CancelTightenStage::InspectRankArc => event
            .after
            .inspected_arc
            .as_ref()
            .map(residual_ref)
            .into_iter()
            .collect(),
        CancelTightenStage::SelectCycle => event
            .after
            .active_cycle
            .first()
            .map(residual_ref)
            .into_iter()
            .collect(),
        CancelTightenStage::CancelCycle => {
            let delta = event
                .delta
                .ok_or_else(|| JsError::new("cancel-and-tighten cancellation omitted delta"))?;
            let residual = ResidualState::from_flows(graph, &event.before.flows)
                .map_err(|error| JsError::new(&error.to_string()))?;
            event
                .after
                .active_cycle
                .iter()
                .filter(|arc| {
                    residual
                        .arc(arc)
                        .is_some_and(|residual_arc| residual_arc.capacity == delta)
                })
                .min()
                .map(residual_ref)
                .into_iter()
                .collect()
        }
        CancelTightenStage::Tighten => event
            .after
            .ranks
            .iter()
            .enumerate()
            .filter_map(|(index, rank)| rank.map(|rank| (rank, index)))
            .max()
            .and_then(|(_, index)| graph.nodes().get(index))
            .map(|node| FlowTraceEntityRefSceneV1::Node {
                node_id: node.id().as_str().to_owned(),
            })
            .into_iter()
            .collect(),
        CancelTightenStage::Ready
        | CancelTightenStage::Initialize
        | CancelTightenStage::BeginPhase
        | CancelTightenStage::Optimal => Vec::new(),
    };
    let patch_count = cancel_tighten_patch_count(event)?;
    let detail = match event.after.stage {
        CancelTightenStage::InspectCycleArc => Some(FlowTraceEventDetailSceneV1 {
            label: "cycle-search scan".to_owned(),
            value: event.after.metrics.residual_arc_scans.to_string(),
        }),
        CancelTightenStage::InspectRankArc => Some(FlowTraceEventDetailSceneV1 {
            label: "ranking scan".to_owned(),
            value: event.after.metrics.residual_arc_scans.to_string(),
        }),
        _ => event.delta.map(|value| FlowTraceEventDetailSceneV1 {
            label: "bottleneck".to_owned(),
            value: value.to_string(),
        }),
    };
    let minimum_granularity = match event.after.stage {
        CancelTightenStage::InspectCycleArc | CancelTightenStage::InspectRankArc => {
            flow::TraceGranularityV1::Micro
        }
        CancelTightenStage::SelectCycle | CancelTightenStage::CancelCycle => {
            flow::TraceGranularityV1::Operation
        }
        _ => flow::TraceGranularityV1::Phase,
    };
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: if event.after.stage == CancelTightenStage::BeginPhase {
            None
        } else {
            parent_phase_id.map(|value| value.to_string())
        },
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity,
        pseudocode_line: cancel_tighten_pseudocode_line(event.after.stage).to_owned(),
        patch_count,
        entity_refs,
        detail,
    })
}

fn cancel_tighten_patch_count(event: &CancelTightenTraceEvent) -> Result<u32, JsError> {
    let changes = event
        .before
        .flows
        .iter()
        .zip(&event.after.flows)
        .filter(|(before, after)| before != after)
        .count()
        + event
            .before
            .potentials
            .iter()
            .zip(&event.after.potentials)
            .filter(|(before, after)| before != after)
            .count()
        + usize::from(event.before.epsilon != event.after.epsilon)
        + usize::from(event.before.stage != event.after.stage);
    u32::try_from(changes.max(1))
        .map_err(|_| JsError::new("cancel-and-tighten patch count overflow"))
}

const fn cancel_tighten_pseudocode_line(stage: CancelTightenStage) -> &'static str {
    match stage {
        CancelTightenStage::Ready => "cancel-and-tighten:ready",
        CancelTightenStage::Initialize => "cancel-and-tighten:initialize-exact-epsilon-state",
        CancelTightenStage::BeginPhase => "cancel-and-tighten:begin-cancel-tighten-phase",
        CancelTightenStage::InspectCycleArc => {
            "cancel-and-tighten:inspect-arc-for-admissible-cycle"
        }
        CancelTightenStage::SelectCycle => "cancel-and-tighten:select-admissible-cycle",
        CancelTightenStage::CancelCycle => "cancel-and-tighten:saturate-admissible-cycle",
        CancelTightenStage::InspectRankArc => "cancel-and-tighten:inspect-arc-for-topological-rank",
        CancelTightenStage::Tighten => "cancel-and-tighten:tighten-by-topological-rank",
        CancelTightenStage::Optimal => "cancel-and-tighten:return-independent-certificate",
    }
}

fn set_cancel_tighten_metrics(scene: &mut FlowCurrentSceneV9, snapshot: &CancelTightenSnapshot) {
    scene.set_cancel_tighten_metrics(
        snapshot.metrics.phases,
        snapshot.metrics.cycle_searches,
        snapshot.metrics.cancellations,
        snapshot.metrics.tightenings,
        snapshot.metrics.residual_arc_scans,
    );
}

fn relaxed_mndc_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::RelaxedMndcTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("relaxed-MNDC event count overflow"))?;
    let ready = ready_flow_scene(scenario)?;
    let mut projected = ready.clone();
    let base_overlay = relaxed_mndc_overlay(graph, &run.base_snapshot, None)?;
    projected
        .apply_relaxed_mndc_boundary(
            graph,
            &run.base_snapshot.flows,
            base_overlay,
            0,
            event_count,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    let base = flow_only_source_base(ready, &projected, event_count);
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    let mut parent_phase_id = None;
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("relaxed-MNDC trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("relaxed-MNDC event identity overflow"))?;
        if event.after.stage == RelaxedMndcStage::BeginPhase {
            parent_phase_id = Some(event_id);
        }
        let deltas = (event.after.stage == RelaxedMndcStage::CancelFamily)
            .then_some(event.deltas.as_slice());
        let overlay = relaxed_mndc_overlay(graph, &event.after, deltas)?;
        let mut scene = base.clone();
        scene
            .apply_relaxed_mndc_boundary(graph, &event.after.flows, overlay, event_id, event_count)
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(relaxed_mndc_trace_event_scene(
            graph,
            event,
            event_id,
            parent_phase_id,
        )?);
        set_relaxed_mndc_metrics(&mut scene, &event.after);
        if index + 1 == run.events.len() {
            scene.set_min_cost_flow_outcome(graph, &run.result.certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new("relaxed-MNDC trace final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

#[expect(
    clippy::too_many_lines,
    reason = "the converter keeps the assignment, cycle family, and active-cell schema projection together"
)]
fn relaxed_mndc_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &RelaxedMndcSnapshot,
    deltas: Option<&[u64]>,
) -> Result<FlowRelaxedMndcOverlayV1, JsError> {
    let node_count = graph.nodes().len();
    let scale_divisor = relaxed_mndc_scale_divisor(snapshot)?;
    let assignment_present = !snapshot.assignment.is_empty();
    if assignment_present
        && (snapshot.assignment.len() != node_count
            || snapshot.left_duals.len() != node_count
            || snapshot.right_duals.len() != node_count)
    {
        return Err(JsError::new(
            "relaxed-MNDC assignment overlay shape does not match graph",
        ));
    }
    if !assignment_present && (!snapshot.left_duals.is_empty() || !snapshot.right_duals.is_empty())
    {
        return Err(JsError::new(
            "relaxed-MNDC duals exist without a public assignment",
        ));
    }
    if deltas.is_some_and(|values| values.len() != snapshot.family.len()) {
        return Err(JsError::new(
            "relaxed-MNDC bottleneck count does not match selected family",
        ));
    }
    let nodes = graph
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            if !assignment_present {
                return Ok(FlowRelaxedMndcNodeStateV1 {
                    node_id: node.id().as_str().to_owned(),
                    matched_node_id: node.id().as_str().to_owned(),
                    left_dual: "0".to_owned(),
                    right_dual: "0".to_owned(),
                    selected_arc: None,
                });
            }
            let choice = &snapshot.assignment[index];
            let matched = graph
                .node(choice.column)
                .ok_or_else(|| JsError::new("relaxed-MNDC matched node is absent"))?;
            Ok(FlowRelaxedMndcNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                matched_node_id: matched.id().as_str().to_owned(),
                left_dual: relaxed_mndc_scaled_value(snapshot.left_duals[index], scale_divisor)?,
                right_dual: relaxed_mndc_scaled_value(snapshot.right_duals[index], scale_divisor)?,
                selected_arc: choice.residual_arc.as_ref().map(relaxed_mndc_residual_ref),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let family = snapshot
        .family
        .iter()
        .enumerate()
        .map(|(index, cycle)| {
            Ok(FlowRelaxedMndcCycleV1 {
                transformed_cost: relaxed_mndc_scaled_value(cycle.transformed_cost, scale_divisor)?,
                arcs: cycle.arcs.iter().map(relaxed_mndc_residual_ref).collect(),
                delta: deltas.map(|values| values[index].to_string()),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let divisor_u128 = u128::try_from(scale_divisor)
        .map_err(|_| JsError::new("relaxed-MNDC scale divisor overflow"))?;
    Ok(FlowRelaxedMndcOverlayV1 {
        stage: relaxed_mndc_scene_stage(snapshot.stage),
        epsilon: FlowRationalV1 {
            numerator: (snapshot.epsilon.numerator() / divisor_u128).to_string(),
            denominator: (snapshot.epsilon.denominator() / divisor_u128).to_string(),
        },
        phase: snapshot.phase.to_string(),
        assignment_value: snapshot
            .assignment_value
            .map(|value| relaxed_mndc_scaled_value(value, scale_divisor))
            .transpose()?,
        nodes,
        family,
        inspected_arcs: snapshot
            .active_residual_arc
            .as_ref()
            .map(relaxed_mndc_residual_ref)
            .into_iter()
            .collect(),
        active_assignment_cell: snapshot
            .active_assignment_cell
            .map(
                |(row, column)| -> Result<FlowRelaxedMndcAssignmentCellV1, JsError> {
                    let row_node = graph
                        .node(row)
                        .ok_or_else(|| JsError::new("relaxed-MNDC assignment row is absent"))?;
                    let column_node = graph
                        .node(column)
                        .ok_or_else(|| JsError::new("relaxed-MNDC assignment column is absent"))?;
                    Ok(FlowRelaxedMndcAssignmentCellV1 {
                        row_node_id: row_node.id().as_str().to_owned(),
                        column_node_id: column_node.id().as_str().to_owned(),
                    })
                },
            )
            .transpose()?,
    })
}

fn enhanced_capacity_scaling_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::EnhancedCapacityScalingTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("enhanced capacity scaling event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    let mut parent_phase_id = None;
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new(
                "enhanced capacity scaling trace discontinuity",
            ));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("enhanced capacity scaling event identity overflow"))?;
        if event.after.stage == EnhancedCapacityScalingStage::BeginPhase {
            parent_phase_id = Some(event_id);
        }
        let overlay = enhanced_capacity_scaling_overlay(graph, &event.after, Some(event))?;
        let display_flows = enhanced_capacity_scaling_display_flows(graph, &event.after);
        let mut scene = base.clone();
        scene
            .apply_enhanced_capacity_scaling_boundary(
                graph,
                &display_flows,
                overlay,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(enhanced_capacity_scaling_trace_event_scene(
            graph,
            event,
            event_id,
            parent_phase_id,
        )?);
        scene.set_enhanced_capacity_scaling_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene.set_min_cost_flow_outcome(graph, &run.result.certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new(
            "enhanced capacity scaling final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn enhanced_capacity_scaling_display_flows(
    graph: &flow::FlowNetwork,
    snapshot: &EnhancedCapacityScalingSnapshot,
) -> Vec<u64> {
    snapshot
        .certified_flows
        .clone()
        .unwrap_or_else(|| graph.edges().iter().map(flow::FlowEdge::lower).collect())
}

fn enhanced_capacity_scaling_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &EnhancedCapacityScalingSnapshot,
    event: Option<&EnhancedCapacityScalingTraceEvent>,
) -> Result<FlowEnhancedCapacityScalingOverlayV1, JsError> {
    if snapshot.potentials.len() != graph.nodes().len()
        || snapshot.distances.len() != graph.nodes().len()
        || snapshot.virtual_flow_numerators.len() != graph.edges().len()
    {
        return Err(JsError::new(
            "enhanced capacity scaling overlay shape does not match graph",
        ));
    }
    let projection = enhanced_scaling_component_projection(graph, snapshot)?;
    let edges = enhanced_scaling_edge_projection(graph, snapshot, &projection.component_by_node)?;
    Ok(FlowEnhancedCapacityScalingOverlayV1 {
        stage: enhanced_capacity_scaling_scene_stage(snapshot.stage),
        delta: normalized_unsigned_rational(snapshot.delta_numerator, snapshot.denominator)?,
        phase: snapshot.metrics.scaling_phases.to_string(),
        components: projection.components,
        nodes: projection.nodes,
        edges,
        source_component: snapshot
            .source_component
            .map(|component| enhanced_scaling_node_id(graph, component))
            .transpose()?,
        sink_component: snapshot
            .sink_component
            .map(|component| enhanced_scaling_node_id(graph, component))
            .transpose()?,
        path: snapshot
            .path
            .iter()
            .map(relaxed_mndc_residual_ref)
            .collect(),
        contraction_arc: event.and_then(|item| {
            item.contraction_arc
                .as_ref()
                .map(|edge| edge.as_str().to_owned())
        }),
        augmentation: event
            .and_then(|item| item.augmentation_numerator)
            .map(|numerator| normalized_unsigned_rational(numerator, snapshot.denominator))
            .transpose()?,
    })
}

struct EnhancedScalingComponentProjection {
    components: Vec<FlowEnhancedCapacityScalingComponentV1>,
    nodes: Vec<FlowEnhancedCapacityScalingNodeStateV1>,
    component_by_node: Vec<Option<String>>,
}

fn enhanced_scaling_component_projection(
    graph: &flow::FlowNetwork,
    snapshot: &EnhancedCapacityScalingSnapshot,
) -> Result<EnhancedScalingComponentProjection, JsError> {
    let mut component_by_node = vec![None; graph.nodes().len()];
    let components = snapshot
        .components
        .iter()
        .map(|component| {
            let component_id = graph
                .node(component.id)
                .ok_or_else(|| JsError::new("enhanced scaling component is absent"))?
                .id()
                .as_str()
                .to_owned();
            let members = component
                .members
                .iter()
                .map(|&member| {
                    component_by_node[member.as_usize()] = Some(component_id.clone());
                    graph
                        .node(member)
                        .ok_or_else(|| JsError::new("enhanced scaling member is absent"))
                        .map(|node| node.id().as_str().to_owned())
                })
                .collect::<Result<Vec<_>, JsError>>()?;
            Ok(FlowEnhancedCapacityScalingComponentV1 {
                component_id,
                members,
                excess: normalized_signed_rational(
                    component.excess_numerator,
                    snapshot.denominator,
                )?,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let nodes = graph
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            Ok(FlowEnhancedCapacityScalingNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                component_id: component_by_node[index]
                    .clone()
                    .ok_or_else(|| JsError::new("enhanced scaling partition is incomplete"))?,
                potential: snapshot.potentials[index].to_string(),
                distance: snapshot.distances[index].map(|value| value.to_string()),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(EnhancedScalingComponentProjection {
        components,
        nodes,
        component_by_node,
    })
}

fn enhanced_scaling_edge_projection(
    graph: &flow::FlowNetwork,
    snapshot: &EnhancedCapacityScalingSnapshot,
    component_by_node: &[Option<String>],
) -> Result<Vec<FlowEnhancedCapacityScalingEdgeStateV1>, JsError> {
    let threshold = snapshot
        .delta_numerator
        .checked_mul(
            u128::try_from(graph.nodes().len())
                .map_err(|_| JsError::new("enhanced scaling threshold overflow"))?,
        )
        .and_then(|value| value.checked_mul(3))
        .ok_or_else(|| JsError::new("enhanced scaling threshold overflow"))?;
    graph
        .edges()
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            let from_component = &component_by_node[edge.from().as_usize()];
            let to_component = &component_by_node[edge.to().as_usize()];
            let internal = from_component == to_component;
            let reduced_cost = i128::from(edge.cost())
                .checked_add(snapshot.potentials[edge.from().as_usize()])
                .and_then(|value| value.checked_sub(snapshot.potentials[edge.to().as_usize()]))
                .ok_or_else(|| JsError::new("enhanced scaling reduced-cost overflow"))?;
            let virtual_flow = u128::try_from(snapshot.virtual_flow_numerators[index])
                .map_err(|_| JsError::new("enhanced scaling virtual flow is negative"))?;
            Ok(FlowEnhancedCapacityScalingEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                virtual_flow: normalized_unsigned_rational(virtual_flow, snapshot.denominator)?,
                reduced_cost: reduced_cost.to_string(),
                internal,
                strongly_feasible: !internal && virtual_flow >= threshold,
                tight: reduced_cost == 0,
            })
        })
        .collect()
}

fn orlin_mcf_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::OrlinMcfTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("Orlin MCF event count overflow"))?;
    let mut base = ready_flow_scene(scenario)?;
    base.event_count = event_count.to_string();
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    let mut parent_phase_id = None;
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("Orlin MCF trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("Orlin MCF event identity overflow"))?;
        if event.after.stage == OrlinMcfStage::BeginPhase {
            parent_phase_id = Some(event_id);
        }
        let overlay = orlin_mcf_overlay(graph, &event.after, Some(event))?;
        let display_flows = orlin_mcf_display_flows(graph, &event.after);
        let mut scene = base.clone();
        scene
            .apply_orlin_mcf_boundary(graph, &display_flows, overlay, event_id, event_count)
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(orlin_mcf_trace_event_scene(
            event,
            event_id,
            parent_phase_id,
        )?);
        scene.set_orlin_mcf_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene.set_min_cost_flow_outcome(graph, &run.result.certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new("Orlin MCF final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn orlin_mcf_display_flows(graph: &flow::FlowNetwork, snapshot: &OrlinMcfSnapshot) -> Vec<u64> {
    snapshot
        .certified_flows
        .clone()
        .unwrap_or_else(|| graph.edges().iter().map(flow::FlowEdge::lower).collect())
}

fn orlin_mcf_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &OrlinMcfSnapshot,
    event: Option<&OrlinMcfTraceEvent>,
) -> Result<FlowOrlinMcfOverlayV1, JsError> {
    let expected_arcs = graph
        .edges()
        .iter()
        .filter(|edge| edge.capacity() > edge.lower())
        .count()
        .saturating_mul(2);
    if snapshot.arcs.len() != expected_arcs {
        return Err(JsError::new("Orlin MCF overlay shape does not match graph"));
    }
    let (components, nodes, component_id_by_root) =
        orlin_mcf_component_projection(graph, snapshot)?;
    let arcs = orlin_mcf_arc_projection(graph, snapshot, &nodes)?;
    Ok(FlowOrlinMcfOverlayV1 {
        stage: orlin_mcf_scene_stage(snapshot.stage),
        delta: normalized_unsigned_rational(snapshot.delta_numerator, snapshot.denominator)?,
        phase: snapshot.metrics.scaling_phases.to_string(),
        components,
        nodes,
        arcs,
        source_component: snapshot
            .source_component
            .and_then(|root| component_id_by_root.get(&root).cloned()),
        sink_component: snapshot
            .sink_component
            .and_then(|root| component_id_by_root.get(&root).cloned()),
        path: snapshot.path.iter().map(orlin_mcf_scene_arc_ref).collect(),
        inspected_segment: snapshot
            .inspected_segment
            .iter()
            .map(orlin_mcf_scene_arc_ref)
            .collect(),
        inspection_serial: (!snapshot.inspected_segment.is_empty())
            .then(|| snapshot.metrics.residual_arc_scans.to_string()),
        contraction_arc: event
            .and_then(|item| item.contraction_arc.as_ref())
            .map(orlin_mcf_scene_arc_ref),
        augmentation: event
            .and_then(|item| item.augmentation_numerator)
            .map(|numerator| normalized_unsigned_rational(numerator, snapshot.denominator))
            .transpose()?,
        eliminated_capacity_nodes: snapshot.eliminated_capacity_nodes.to_string(),
        shortcut_arcs: snapshot.shortcut_arcs.to_string(),
    })
}

type OrlinMcfComponentProjection = (
    Vec<FlowOrlinMcfComponentV1>,
    Vec<FlowOrlinMcfNodeStateV1>,
    BTreeMap<usize, String>,
);

fn orlin_mcf_component_projection(
    graph: &flow::FlowNetwork,
    snapshot: &OrlinMcfSnapshot,
) -> Result<OrlinMcfComponentProjection, JsError> {
    let transformed_ids = snapshot
        .nodes
        .iter()
        .map(|state| orlin_mcf_node_id(graph, &state.kind))
        .collect::<Result<Vec<_>, _>>()?;
    let mut members_by_component = BTreeMap::<usize, Vec<String>>::new();
    for (index, node) in snapshot.nodes.iter().enumerate() {
        if node.component >= snapshot.nodes.len() {
            return Err(JsError::new("Orlin MCF component root is out of range"));
        }
        members_by_component
            .entry(node.component)
            .or_default()
            .push(transformed_ids[index].clone());
    }
    let component_id_by_root = members_by_component
        .keys()
        .map(|&root| {
            transformed_ids
                .get(root)
                .cloned()
                .map(|id| (root, id))
                .ok_or_else(|| JsError::new("Orlin MCF component identity is absent"))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let components = members_by_component
        .iter()
        .map(|(&root, members)| {
            Ok(FlowOrlinMcfComponentV1 {
                component_id: component_id_by_root[&root].clone(),
                members: members.clone(),
                excess: normalized_signed_rational(
                    snapshot.nodes[root].component_excess_numerator,
                    snapshot.denominator,
                )?,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let nodes = snapshot
        .nodes
        .iter()
        .zip(&transformed_ids)
        .map(|(state, node_id)| {
            let (kind, capacity_edge_id) = match &state.kind {
                OrlinMcfNodeKind::Original(_) => (FlowOrlinMcfNodeKindV1::Original, None),
                OrlinMcfNodeKind::Capacity(edge_id) => (
                    FlowOrlinMcfNodeKindV1::Capacity,
                    Some(edge_id.as_str().to_owned()),
                ),
            };
            Ok(FlowOrlinMcfNodeStateV1 {
                node_id: node_id.clone(),
                kind,
                capacity_edge_id,
                component_id: component_id_by_root
                    .get(&state.component)
                    .cloned()
                    .ok_or_else(|| JsError::new("Orlin MCF node component is absent"))?,
                potential: state.potential.to_string(),
                distance: state.distance.map(|value| value.to_string()),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok((components, nodes, component_id_by_root))
}

fn orlin_mcf_arc_projection(
    graph: &flow::FlowNetwork,
    snapshot: &OrlinMcfSnapshot,
    nodes: &[FlowOrlinMcfNodeStateV1],
) -> Result<Vec<FlowOrlinMcfArcStateV1>, JsError> {
    let component_by_node = nodes
        .iter()
        .map(|node| (node.node_id.clone(), node.component_id.clone()))
        .collect::<BTreeMap<_, _>>();
    let threshold = snapshot
        .delta_numerator
        .checked_mul(
            u128::try_from(snapshot.nodes.len())
                .map_err(|_| JsError::new("Orlin MCF contraction threshold overflow"))?,
        )
        .and_then(|value| value.checked_mul(3))
        .ok_or_else(|| JsError::new("Orlin MCF contraction threshold overflow"))?;
    snapshot
        .arcs
        .iter()
        .map(|state| {
            let edge = graph
                .edges()
                .iter()
                .find(|edge| edge.id() == &state.edge_id)
                .ok_or_else(|| JsError::new("Orlin MCF transformed edge is absent"))?;
            let original = graph
                .node(match state.branch {
                    OrlinMcfBranch::Flow => edge.from(),
                    OrlinMcfBranch::Slack => edge.to(),
                })
                .ok_or_else(|| JsError::new("Orlin MCF transformed endpoint is absent"))?
                .id()
                .as_str();
            let capacity = format!("capacity:{}", state.edge_id.as_str());
            let internal = component_by_node[original] == component_by_node[&capacity];
            let flow = u128::try_from(state.flow_numerator)
                .map_err(|_| JsError::new("Orlin MCF transformed flow is negative"))?;
            Ok(FlowOrlinMcfArcStateV1 {
                edge_id: state.edge_id.as_str().to_owned(),
                branch: orlin_mcf_scene_branch(state.branch),
                flow: normalized_unsigned_rational(flow, snapshot.denominator)?,
                reduced_cost: state.reduced_cost.to_string(),
                internal,
                strongly_feasible: !internal && flow >= threshold,
                tight: state.reduced_cost == 0,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()
}

fn orlin_mcf_node_id(
    graph: &flow::FlowNetwork,
    kind: &OrlinMcfNodeKind,
) -> Result<String, JsError> {
    match kind {
        OrlinMcfNodeKind::Original(index) => graph
            .node(*index)
            .map(|node| node.id().as_str().to_owned())
            .ok_or_else(|| JsError::new("Orlin MCF original node is absent")),
        OrlinMcfNodeKind::Capacity(edge_id) => {
            if graph.edges().iter().any(|edge| edge.id() == edge_id) {
                Ok(format!("capacity:{}", edge_id.as_str()))
            } else {
                Err(JsError::new("Orlin MCF capacity edge is absent"))
            }
        }
    }
}

const fn orlin_mcf_scene_branch(branch: OrlinMcfBranch) -> FlowOrlinMcfBranchV1 {
    match branch {
        OrlinMcfBranch::Flow => FlowOrlinMcfBranchV1::Flow,
        OrlinMcfBranch::Slack => FlowOrlinMcfBranchV1::Slack,
    }
}

fn orlin_mcf_scene_arc_ref(arc: &OrlinMcfArcId) -> FlowOrlinMcfArcRefV1 {
    FlowOrlinMcfArcRefV1 {
        edge_id: arc.edge_id.as_str().to_owned(),
        branch: orlin_mcf_scene_branch(arc.branch),
        direction: match arc.direction {
            ResidualDirection::Forward => "forward",
            ResidualDirection::Reverse => "reverse",
        }
        .to_owned(),
    }
}

const fn orlin_mcf_scene_stage(stage: OrlinMcfStage) -> FlowOrlinMcfStageV1 {
    match stage {
        OrlinMcfStage::Ready => FlowOrlinMcfStageV1::Ready,
        OrlinMcfStage::TransformCapacities => FlowOrlinMcfStageV1::TransformCapacities,
        OrlinMcfStage::InitializeDual => FlowOrlinMcfStageV1::InitializeDual,
        OrlinMcfStage::CompleteRegeneration => FlowOrlinMcfStageV1::CompleteRegeneration,
        OrlinMcfStage::BeginPhase => FlowOrlinMcfStageV1::BeginPhase,
        OrlinMcfStage::Contract => FlowOrlinMcfStageV1::Contract,
        OrlinMcfStage::InspectContractibleArc => FlowOrlinMcfStageV1::InspectContractibleArc,
        OrlinMcfStage::InspectReachabilityArc => FlowOrlinMcfStageV1::InspectReachabilityArc,
        OrlinMcfStage::InspectCompressedResidualArc => {
            FlowOrlinMcfStageV1::InspectCompressedResidualArc
        }
        OrlinMcfStage::InspectCompressedArc => FlowOrlinMcfStageV1::InspectCompressedArc,
        OrlinMcfStage::SelectCompressedPath => FlowOrlinMcfStageV1::SelectCompressedPath,
        OrlinMcfStage::Augment => FlowOrlinMcfStageV1::Augment,
        OrlinMcfStage::CompletePhase => FlowOrlinMcfStageV1::CompletePhase,
        OrlinMcfStage::HalveScale => FlowOrlinMcfStageV1::HalveScale,
        OrlinMcfStage::ExpandDual => FlowOrlinMcfStageV1::ExpandDual,
        OrlinMcfStage::RecoverPrimal => FlowOrlinMcfStageV1::RecoverPrimal,
        OrlinMcfStage::Optimal => FlowOrlinMcfStageV1::Optimal,
    }
}

fn orlin_mcf_trace_event_scene(
    event: &OrlinMcfTraceEvent,
    event_id: u64,
    parent_phase_id: Option<u64>,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let stage = event.after.stage;
    let micro_inspection = matches!(
        stage,
        OrlinMcfStage::InspectContractibleArc
            | OrlinMcfStage::InspectReachabilityArc
            | OrlinMcfStage::InspectCompressedResidualArc
            | OrlinMcfStage::InspectCompressedArc
    );
    let public_residual_ref = |arc: &flow::OrlinMcfArcId| FlowTraceEntityRefSceneV1::ResidualArc {
        edge_id: arc.edge_id.as_str().to_owned(),
        direction: match (arc.branch, arc.direction) {
            (OrlinMcfBranch::Flow, ResidualDirection::Forward)
            | (OrlinMcfBranch::Slack, ResidualDirection::Reverse) => "forward",
            (OrlinMcfBranch::Flow, ResidualDirection::Reverse)
            | (OrlinMcfBranch::Slack, ResidualDirection::Forward) => "reverse",
        }
        .to_owned(),
    };
    // A compressed transformed segment can contain both branches of one
    // original residual direction. Its typed overlay retains those exact
    // transformed arcs; the ordinary graph owns one stable public identity.
    // Micro inspections publish only that current segment, never the retained
    // aggregate path.
    let mut entity_refs = if micro_inspection {
        event
            .after
            .inspected_segment
            .iter()
            .map(public_residual_ref)
            .collect::<BTreeSet<_>>()
    } else {
        event
            .after
            .path
            .iter()
            .map(public_residual_ref)
            .collect::<BTreeSet<_>>()
    };
    if let Some(arc) = &event.contraction_arc {
        entity_refs.insert(FlowTraceEntityRefSceneV1::Edge {
            edge_id: arc.edge_id.as_str().to_owned(),
        });
    }
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: if stage == OrlinMcfStage::BeginPhase {
            None
        } else {
            parent_phase_id.map(|value| value.to_string())
        },
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: match stage {
            OrlinMcfStage::InspectContractibleArc
            | OrlinMcfStage::InspectReachabilityArc
            | OrlinMcfStage::InspectCompressedResidualArc
            | OrlinMcfStage::InspectCompressedArc => TraceGranularityV1::Micro,
            OrlinMcfStage::Contract
            | OrlinMcfStage::SelectCompressedPath
            | OrlinMcfStage::Augment => TraceGranularityV1::Operation,
            _ => TraceGranularityV1::Phase,
        },
        pseudocode_line: orlin_mcf_pseudocode_line(stage).to_owned(),
        patch_count: u32::try_from(
            event
                .after
                .path
                .len()
                .max(event.after.inspected_segment.len())
                .max(1),
        )
        .map_err(|_| JsError::new("Orlin MCF patch count overflow"))?,
        entity_refs: entity_refs.into_iter().collect(),
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: match stage {
                OrlinMcfStage::TransformCapacities => "capacity nodes",
                OrlinMcfStage::Contract => "quotient components",
                OrlinMcfStage::InspectContractibleArc
                | OrlinMcfStage::InspectReachabilityArc
                | OrlinMcfStage::InspectCompressedResidualArc
                | OrlinMcfStage::InspectCompressedArc => "residual arc scan",
                OrlinMcfStage::SelectCompressedPath => "compressed shortcuts",
                OrlinMcfStage::Augment => "augmentation numerator",
                _ => "delta numerator",
            }
            .to_owned(),
            value: match stage {
                OrlinMcfStage::TransformCapacities => {
                    event.after.metrics.capacity_nodes.to_string()
                }
                OrlinMcfStage::Contract => event.after.metrics.contractions.to_string(),
                OrlinMcfStage::InspectContractibleArc
                | OrlinMcfStage::InspectReachabilityArc
                | OrlinMcfStage::InspectCompressedResidualArc
                | OrlinMcfStage::InspectCompressedArc => {
                    event.after.metrics.residual_arc_scans.to_string()
                }
                OrlinMcfStage::SelectCompressedPath => event.after.shortcut_arcs.to_string(),
                OrlinMcfStage::Augment => event
                    .augmentation_numerator
                    .unwrap_or(event.after.delta_numerator)
                    .to_string(),
                _ => event.after.delta_numerator.to_string(),
            },
        }),
    })
}

const fn orlin_mcf_pseudocode_line(stage: OrlinMcfStage) -> &'static str {
    match stage {
        OrlinMcfStage::Ready => "validate bounded input",
        OrlinMcfStage::TransformCapacities => "replace each finite arc by a demand node",
        OrlinMcfStage::InitializeDual => "initialize dual-feasible prices",
        OrlinMcfStage::CompleteRegeneration => "regenerate Δ from component imbalance",
        OrlinMcfStage::BeginPhase => "begin α=3/4 scaling phase",
        OrlinMcfStage::Contract => "contract a tight 3nΔ branch",
        OrlinMcfStage::InspectContractibleArc => "inspect a contraction candidate branch",
        OrlinMcfStage::InspectReachabilityArc => "inspect reverse reachability residual arc",
        OrlinMcfStage::InspectCompressedResidualArc => {
            "classify a residual arc for quotient compression"
        }
        OrlinMcfStage::InspectCompressedArc => "relax one compressed residual segment",
        OrlinMcfStage::SelectCompressedPath => "eliminate capacity nodes and run shortest path",
        OrlinMcfStage::Augment => "send exact Δ and update prices",
        OrlinMcfStage::CompletePhase => "finish unreachable active-pair scan",
        OrlinMcfStage::HalveScale => "halve Δ exactly",
        OrlinMcfStage::ExpandDual => "expand contracted dual prices",
        OrlinMcfStage::RecoverPrimal => "recover flow on zero reduced-cost branches",
        OrlinMcfStage::Optimal => "certify original bounded optimum",
    }
}

fn orlin_max_flow_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::OrlinMaxTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("Orlin max-flow event count overflow"))?;
    // The public timeline starts before Orlin's quotient/cut workspace is
    // materialized. The first source event owns that visible construction;
    // publishing `run.base_snapshot` here would hide real initialization work
    // inside an apparent Ready boundary.
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    let mut parent_phase_id = None;
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("Orlin max-flow trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("Orlin max-flow event identity overflow"))?;
        if event.after.stage == OrlinMaxStage::BeginImprovement {
            parent_phase_id = Some(event_id);
        }
        let flows = orlin_max_flow_display_flows(&event.after)?;
        let mut scene = base.clone();
        scene
            .apply_orlin_max_flow_boundary(
                graph,
                &flows,
                orlin_max_flow_overlay(graph, &event.after)?,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(orlin_max_flow_trace_event_scene(
            graph,
            event,
            event_id,
            parent_phase_id,
        )?);
        scene.set_orlin_max_flow_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene.set_max_flow_outcome(&run.result.certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new("Orlin max-flow final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn orlin_max_flow_display_flows(snapshot: &OrlinMaxSnapshot) -> Result<Vec<u64>, JsError> {
    snapshot
        .residual_arcs
        .chunks_exact(2)
        .map(|pair| {
            pair.get(1)
                .filter(|arc| arc.id.direction() == ResidualDirection::Reverse)
                .map(|arc| arc.capacity)
                .ok_or_else(|| JsError::new("Orlin max-flow residual ordering drift"))
        })
        .collect()
}

fn orlin_max_flow_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &OrlinMaxSnapshot,
) -> Result<FlowOrlinMaxFlowOverlayV1, JsError> {
    let mut component_ids = BTreeMap::<usize, String>::new();
    for (node, state) in graph.nodes().iter().zip(&snapshot.nodes) {
        component_ids
            .entry(state.component)
            .or_insert_with(|| node.id().as_str().to_owned());
    }
    if snapshot.nodes.len() != graph.nodes().len() {
        return Err(JsError::new("Orlin max-flow node projection drift"));
    }
    let nodes = graph
        .nodes()
        .iter()
        .zip(&snapshot.nodes)
        .map(|(node, state)| {
            Ok(FlowOrlinMaxFlowNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                component_id: component_ids
                    .get(&state.component)
                    .cloned()
                    .ok_or_else(|| JsError::new("Orlin max-flow component is absent"))?,
                critical: state.critical,
                anti_potential: state.anti_potential.to_string(),
                source_side: state.source_side,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let residual_arcs = snapshot
        .residual_arcs
        .iter()
        .map(|arc| FlowOrlinMaxFlowResidualArcStateV1 {
            edge_id: arc.id.original_edge().as_str().to_owned(),
            direction: residual_direction_text(arc.id.direction()).to_owned(),
            capacity: arc.capacity.to_string(),
            abundant: arc.abundant,
            anti_abundant: arc.anti_abundant,
            small: arc.small,
            medium: arc.medium,
            inspection_serial: arc.inspection_serial.map(|value| value.to_string()),
        })
        .collect();
    let compact_arcs = snapshot
        .compact_arcs
        .iter()
        .map(|arc| {
            Ok(FlowOrlinMaxFlowCompactArcStateV1 {
                ordinal: arc.ordinal.to_string(),
                from_component: component_ids
                    .get(&arc.from_component)
                    .cloned()
                    .ok_or_else(|| JsError::new("Orlin compact tail is absent"))?,
                to_component: component_ids
                    .get(&arc.to_component)
                    .cloned()
                    .ok_or_else(|| JsError::new("Orlin compact head is absent"))?,
                kind: orlin_max_flow_compact_kind(arc.kind),
                capacity: arc.capacity.to_string(),
                flow: arc.flow.to_string(),
                witness: arc.witness.iter().map(scene_residual_ref).collect(),
                inspection_serial: arc.inspection_serial.map(|value| value.to_string()),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowOrlinMaxFlowOverlayV1 {
        stage: orlin_max_flow_scene_stage(snapshot.stage),
        delta: snapshot.delta.to_string(),
        gamma: FlowRationalV1 {
            numerator: snapshot.gamma_numerator.to_string(),
            denominator: snapshot.gamma_denominator.to_string(),
        },
        phase_case: snapshot.phase_case.map(orlin_max_flow_scene_case),
        nodes,
        residual_arcs,
        compact_arcs,
        active_compact_path: snapshot
            .active_compact_path
            .iter()
            .map(|&(ordinal, reverse)| FlowOrlinMaxFlowCompactArcRefV1 {
                ordinal: ordinal.to_string(),
                reverse,
            })
            .collect(),
        active_original_path: snapshot
            .active_original_path
            .iter()
            .map(scene_residual_ref)
            .collect(),
        threshold: snapshot.threshold.to_string(),
    })
}

fn scene_residual_ref(id: &ResidualArcId) -> FlowResidualArcRefV1 {
    FlowResidualArcRefV1 {
        edge_id: id.original_edge().as_str().to_owned(),
        direction: residual_direction_text(id.direction()).to_owned(),
    }
}

const fn residual_direction_text(direction: ResidualDirection) -> &'static str {
    match direction {
        ResidualDirection::Forward => "forward",
        ResidualDirection::Reverse => "reverse",
    }
}

const fn orlin_max_flow_scene_stage(stage: OrlinMaxStage) -> FlowOrlinMaxFlowStageV1 {
    match stage {
        OrlinMaxStage::Ready => FlowOrlinMaxFlowStageV1::Ready,
        OrlinMaxStage::BeginImprovement => FlowOrlinMaxFlowStageV1::BeginImprovement,
        OrlinMaxStage::ContractAbundant => FlowOrlinMaxFlowStageV1::ContractAbundant,
        OrlinMaxStage::InspectClassificationArc => {
            FlowOrlinMaxFlowStageV1::InspectClassificationArc
        }
        OrlinMaxStage::Classify => FlowOrlinMaxFlowStageV1::Classify,
        OrlinMaxStage::SelectCase => FlowOrlinMaxFlowStageV1::SelectCase,
        OrlinMaxStage::InspectCompactConstructionArc => {
            FlowOrlinMaxFlowStageV1::InspectCompactConstructionArc
        }
        OrlinMaxStage::TransferCapacity => FlowOrlinMaxFlowStageV1::TransferCapacity,
        OrlinMaxStage::BuildSubproblem => FlowOrlinMaxFlowStageV1::BuildSubproblem,
        OrlinMaxStage::AugmentSubproblem => FlowOrlinMaxFlowStageV1::AugmentSubproblem,
        OrlinMaxStage::InspectSubproblemArc => FlowOrlinMaxFlowStageV1::InspectSubproblemArc,
        OrlinMaxStage::CompleteSubproblem => FlowOrlinMaxFlowStageV1::CompleteSubproblem,
        OrlinMaxStage::InspectDecompositionArc => FlowOrlinMaxFlowStageV1::InspectDecompositionArc,
        OrlinMaxStage::InspectLiftResidualArc => FlowOrlinMaxFlowStageV1::InspectLiftResidualArc,
        OrlinMaxStage::LiftPath => FlowOrlinMaxFlowStageV1::LiftPath,
        OrlinMaxStage::ExpandContraction => FlowOrlinMaxFlowStageV1::ExpandContraction,
        OrlinMaxStage::InspectExpansionResidualArc => {
            FlowOrlinMaxFlowStageV1::InspectExpansionResidualArc
        }
        OrlinMaxStage::InspectCutResidualArc => FlowOrlinMaxFlowStageV1::InspectCutResidualArc,
        OrlinMaxStage::UpdateCut => FlowOrlinMaxFlowStageV1::UpdateCut,
        OrlinMaxStage::Optimal => FlowOrlinMaxFlowStageV1::Optimal,
    }
}

const fn orlin_max_flow_scene_case(phase_case: OrlinMaxPhaseCase) -> FlowOrlinMaxFlowPhaseCaseV1 {
    match phase_case {
        OrlinMaxPhaseCase::OriginalApproximation => {
            FlowOrlinMaxFlowPhaseCaseV1::OriginalApproximation
        }
        OrlinMaxPhaseCase::CompactApproximation => {
            FlowOrlinMaxFlowPhaseCaseV1::CompactApproximation
        }
        OrlinMaxPhaseCase::CompactExact => FlowOrlinMaxFlowPhaseCaseV1::CompactExact,
    }
}

const fn orlin_max_flow_compact_kind(
    kind: OrlinMaxCompactArcKind,
) -> FlowOrlinMaxFlowCompactArcKindV1 {
    match kind {
        OrlinMaxCompactArcKind::Original => FlowOrlinMaxFlowCompactArcKindV1::Original,
        OrlinMaxCompactArcKind::AbundantPseudo => FlowOrlinMaxFlowCompactArcKindV1::AbundantPseudo,
        OrlinMaxCompactArcKind::TransferredPseudo => {
            FlowOrlinMaxFlowCompactArcKindV1::TransferredPseudo
        }
    }
}

fn orlin_max_flow_entity_refs(
    graph: &flow::FlowNetwork,
    event: &OrlinMaxTraceEvent,
    stage: OrlinMaxStage,
) -> Vec<FlowTraceEntityRefSceneV1> {
    let local_arc_stage = matches!(
        stage,
        OrlinMaxStage::InspectClassificationArc
            | OrlinMaxStage::InspectCompactConstructionArc
            | OrlinMaxStage::InspectSubproblemArc
            | OrlinMaxStage::InspectDecompositionArc
            | OrlinMaxStage::InspectLiftResidualArc
            | OrlinMaxStage::InspectExpansionResidualArc
            | OrlinMaxStage::InspectCutResidualArc
    );
    let mut active_path_refs = event
        .after
        .active_original_path
        .iter()
        .map(|arc| FlowTraceEntityRefSceneV1::ResidualArc {
            edge_id: arc.original_edge().as_str().to_owned(),
            direction: residual_direction_text(arc.direction()).to_owned(),
        })
        .collect::<Vec<_>>();
    let mut active_compact_refs = event
        .after
        .active_compact_path
        .iter()
        .filter_map(|&(ordinal, _)| event.after.compact_arcs.get(ordinal))
        .flat_map(|arc| arc.witness.iter())
        .map(|arc| FlowTraceEntityRefSceneV1::ResidualArc {
            edge_id: arc.original_edge().as_str().to_owned(),
            direction: residual_direction_text(arc.direction()).to_owned(),
        })
        .collect::<Vec<_>>();
    if local_arc_stage {
        active_path_refs.truncate(1);
        active_compact_refs.truncate(1);
    }
    if !active_path_refs.is_empty() {
        active_path_refs
    } else if !active_compact_refs.is_empty() {
        active_compact_refs
    } else {
        match stage {
            OrlinMaxStage::Classify => event
                .after
                .residual_arcs
                .iter()
                .filter(|arc| arc.abundant || arc.anti_abundant || arc.small || arc.medium)
                .map(|arc| FlowTraceEntityRefSceneV1::ResidualArc {
                    edge_id: arc.id.original_edge().as_str().to_owned(),
                    direction: residual_direction_text(arc.id.direction()).to_owned(),
                })
                .collect(),
            OrlinMaxStage::SelectCase => event
                .after
                .nodes
                .iter()
                .position(|node| node.critical)
                .map(|index| {
                    vec![FlowTraceEntityRefSceneV1::Node {
                        node_id: graph.nodes()[index].id().as_str().to_owned(),
                    }]
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }
}

fn orlin_max_flow_detail(
    event: &OrlinMaxTraceEvent,
    stage: OrlinMaxStage,
) -> (&'static str, String) {
    match stage {
        OrlinMaxStage::ContractAbundant => {
            ("contractions", event.after.metrics.contractions.to_string())
        }
        OrlinMaxStage::Classify => (
            "critical observations",
            event.after.metrics.critical_node_observations.to_string(),
        ),
        OrlinMaxStage::TransferCapacity => (
            "transferred units",
            event.after.metrics.transferred_units.to_string(),
        ),
        OrlinMaxStage::BuildSubproblem => {
            ("compact arcs", event.after.compact_arcs.len().to_string())
        }
        OrlinMaxStage::AugmentSubproblem => ("threshold", event.after.threshold.to_string()),
        OrlinMaxStage::InspectClassificationArc
        | OrlinMaxStage::InspectCompactConstructionArc
        | OrlinMaxStage::InspectSubproblemArc
        | OrlinMaxStage::InspectDecompositionArc
        | OrlinMaxStage::InspectLiftResidualArc
        | OrlinMaxStage::InspectExpansionResidualArc
        | OrlinMaxStage::InspectCutResidualArc => (
            "arc scans",
            event.after.metrics.residual_arc_scans.to_string(),
        ),
        OrlinMaxStage::LiftPath => ("lifted paths", event.after.metrics.lifted_paths.to_string()),
        OrlinMaxStage::ExpandContraction => (
            "expansion paths",
            event.after.metrics.expansion_paths.to_string(),
        ),
        _ => ("delta", event.after.delta.to_string()),
    }
}

fn orlin_max_flow_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &OrlinMaxTraceEvent,
    event_id: u64,
    parent_phase_id: Option<u64>,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let stage = event.after.stage;
    let entity_refs = orlin_max_flow_entity_refs(graph, event, stage);
    let (label, value) = orlin_max_flow_detail(event, stage);
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: if stage == OrlinMaxStage::BeginImprovement {
            None
        } else {
            parent_phase_id.map(|value| value.to_string())
        },
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: match stage {
            OrlinMaxStage::ContractAbundant
            | OrlinMaxStage::TransferCapacity
            | OrlinMaxStage::AugmentSubproblem
            | OrlinMaxStage::LiftPath
            | OrlinMaxStage::ExpandContraction => TraceGranularityV1::Operation,
            OrlinMaxStage::InspectClassificationArc
            | OrlinMaxStage::InspectCompactConstructionArc
            | OrlinMaxStage::InspectSubproblemArc
            | OrlinMaxStage::InspectDecompositionArc
            | OrlinMaxStage::InspectLiftResidualArc
            | OrlinMaxStage::InspectExpansionResidualArc
            | OrlinMaxStage::InspectCutResidualArc => TraceGranularityV1::Micro,
            _ => TraceGranularityV1::Phase,
        },
        pseudocode_line: orlin_max_flow_pseudocode_line(stage).to_owned(),
        patch_count: u32::try_from(
            event
                .after
                .active_original_path
                .len()
                .max(event.after.active_compact_path.len())
                .max(1),
        )
        .map_err(|_| JsError::new("Orlin max-flow patch count overflow"))?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

const fn orlin_max_flow_pseudocode_line(stage: OrlinMaxStage) -> &'static str {
    match stage {
        OrlinMaxStage::Ready => "install zero flow and source cut",
        OrlinMaxStage::BeginImprovement => "set Δ := r(S,T)",
        OrlinMaxStage::ContractAbundant => "contract abundant cycles and external arcs",
        OrlinMaxStage::InspectClassificationArc => {
            "inspect one residual or quotient arc for phase classification"
        }
        OrlinMaxStage::Classify => "classify anti-abundant arcs and Δ-critical nodes",
        OrlinMaxStage::SelectCase => "select original, Δ-compact, or (Δ,Γ)-compact case",
        OrlinMaxStage::InspectCompactConstructionArc => {
            "inspect one quotient arc while building the compact network"
        }
        OrlinMaxStage::TransferCapacity => "transfer an anti-abundant path to a pseudo-arc",
        OrlinMaxStage::BuildSubproblem => "materialize retained arcs and abundant pseudo-arcs",
        OrlinMaxStage::AugmentSubproblem => "augment one threshold-residual logical path",
        OrlinMaxStage::InspectSubproblemArc => "inspect one threshold-residual logical arc",
        OrlinMaxStage::CompleteSubproblem => "certify the subproblem residual cut",
        OrlinMaxStage::InspectDecompositionArc => "inspect one logical flow-decomposition arc",
        OrlinMaxStage::InspectLiftResidualArc => "inspect one original residual lift route",
        OrlinMaxStage::LiftPath => "lift one compact flow path to original residual arcs",
        OrlinMaxStage::ExpandContraction => "rebalance one contracted component",
        OrlinMaxStage::InspectExpansionResidualArc => {
            "inspect one residual arc while expanding a contraction"
        }
        OrlinMaxStage::InspectCutResidualArc => "inspect one residual arc for the next source cut",
        OrlinMaxStage::UpdateCut => "install the next residual cut",
        OrlinMaxStage::Optimal => "certify maximum flow and minimum cut",
    }
}

fn apply_orlin_max_flow_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &flow::OrlinMaxResult,
) -> Result<(), JsError> {
    scene
        .apply_orlin_max_flow_boundary(
            graph,
            &result.flows,
            orlin_max_flow_overlay(graph, &result.final_snapshot)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_orlin_max_flow_metrics(result.metrics);
    scene.set_max_flow_outcome(&result.certificate);
    Ok(())
}

fn electrical_flow_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    run: &ElectricalFlowTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("electrical-flow event count overflow"))?;
    // Keep the public Ready scene as the only timeline base. The first source
    // event owns construction of the electrical overlay together with the
    // grounded-Laplacian assembly it actually performed.
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    let mut parent_phase_id = None;
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("electrical-flow trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("electrical-flow event identity overflow"))?;
        let mut scene = base.clone();
        scene
            .apply_electrical_flow_boundary(
                graph,
                source,
                sink,
                electrical_flow_overlay(graph, sink, &event.after)?,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(electrical_flow_trace_event_scene(
            graph,
            event,
            event_id,
            parent_phase_id,
        )?);
        if event.after.stage != ElectricalFlowStage::ConjugateGradientIteration {
            parent_phase_id = Some(event_id);
        }
        scene.set_electrical_flow_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene
                .set_electrical_flow_outcome()
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new("electrical-flow final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn electrical_flow_overlay(
    graph: &flow::FlowNetwork,
    sink: flow::NodeIndex,
    snapshot: &ElectricalFlowSnapshot,
) -> Result<FlowElectricalFlowOverlayV1, JsError> {
    if snapshot.potentials.len() != graph.nodes().len()
        || snapshot.residuals.len() != graph.nodes().len()
        || snapshot.search_directions.len() != graph.nodes().len()
        || snapshot.edges.len() != graph.edges().len()
    {
        return Err(JsError::new("electrical-flow snapshot shape mismatch"));
    }
    let nodes = graph
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| FlowElectricalNodeStateV1 {
            node_id: node.id().as_str().to_owned(),
            potential: snapshot.potentials[index].decimal(),
            residual: snapshot.residuals[index].decimal(),
            search_direction: snapshot.search_directions[index].decimal(),
            grounded: index == sink.as_usize(),
        })
        .collect();
    let edges = graph
        .edges()
        .iter()
        .zip(&snapshot.edges)
        .map(|(edge, state)| {
            if state.edge != *edge.id() {
                return Err(JsError::new("electrical-flow edge identity mismatch"));
            }
            Ok(FlowElectricalEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                resistance: FlowRationalV1 {
                    numerator: "1".to_owned(),
                    denominator: state.conductance.to_string(),
                },
                conductance: state.conductance.to_string(),
                voltage_drop: state.voltage_drop.decimal(),
                current: state.current.decimal(),
                congestion: state.congestion.decimal(),
                energy: state.energy.decimal(),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowElectricalFlowOverlayV1 {
        stage: electrical_flow_scene_stage(snapshot.stage),
        target_current: "1".to_owned(),
        relative_tolerance: flow::ELECTRICAL_FLOW_RELATIVE_TOLERANCE.to_string(),
        iteration: snapshot.iteration.to_string(),
        residual_l2: snapshot.residual_l2.decimal(),
        effective_resistance: snapshot.effective_resistance.decimal(),
        total_energy: snapshot.total_energy.decimal(),
        exact_effective_resistance: snapshot.exact_effective_resistance.as_ref().map(|value| {
            FlowRationalV1 {
                numerator: value.numerator.to_string(),
                denominator: value.denominator.to_string(),
            }
        }),
        maximum_absolute_error: snapshot
            .maximum_absolute_error
            .map(flow::ElectricalScalar::decimal),
        converged: snapshot.converged,
        nodes,
        edges,
    })
}

const fn electrical_flow_scene_stage(stage: ElectricalFlowStage) -> FlowElectricalFlowStageV1 {
    match stage {
        ElectricalFlowStage::Ready => FlowElectricalFlowStageV1::Ready,
        ElectricalFlowStage::AssembleLaplacian => FlowElectricalFlowStageV1::AssembleLaplacian,
        ElectricalFlowStage::InitializeConjugateGradient => {
            FlowElectricalFlowStageV1::InitializeConjugateGradient
        }
        ElectricalFlowStage::ConjugateGradientIteration => {
            FlowElectricalFlowStageV1::ConjugateGradientIteration
        }
        ElectricalFlowStage::RecoverCurrents => FlowElectricalFlowStageV1::RecoverCurrents,
        ElectricalFlowStage::CheckExactReference => FlowElectricalFlowStageV1::CheckExactReference,
        ElectricalFlowStage::Complete => FlowElectricalFlowStageV1::Complete,
    }
}

#[allow(clippy::too_many_lines)]
fn electrical_flow_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &ElectricalFlowTraceEvent,
    event_id: u64,
    parent_phase_id: Option<u64>,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let stage = event.after.stage;
    // A CG iteration changes a dense vector. That global numerical state is
    // rendered by the electrical overlay and convergence badge; it is not a
    // claim that every graph node is the current inspected primitive. Only
    // source-published matrix-row work owns a local focus.
    let entity_refs = event
        .active_nodes
        .iter()
        .map(|node| FlowTraceEntityRefSceneV1::Node {
            node_id: graph.nodes()[node.as_usize()].id().as_str().to_owned(),
        })
        .collect();
    let changed_nodes = event
        .before
        .potentials
        .iter()
        .zip(&event.after.potentials)
        .filter(|(before, after)| before != after)
        .count();
    let changed_edges = event
        .before
        .edges
        .iter()
        .zip(&event.after.edges)
        .filter(|(before, after)| before != after)
        .count();
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: (stage == ElectricalFlowStage::ConjugateGradientIteration)
            .then(|| parent_phase_id.map(|value| value.to_string()))
            .flatten(),
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: match stage {
            ElectricalFlowStage::ConjugateGradientIteration => TraceGranularityV1::Micro,
            ElectricalFlowStage::RecoverCurrents => TraceGranularityV1::Operation,
            _ => TraceGranularityV1::Phase,
        },
        pseudocode_line: if event.catalog_id == "electrical-flow.matrix-scalar-product" {
            "accumulate one grounded Laplacian row coefficient"
        } else {
            electrical_flow_pseudocode_line(stage)
        }
        .to_owned(),
        patch_count: u32::try_from(changed_nodes + changed_edges + 1)
            .map_err(|_| JsError::new("electrical-flow patch count overflow"))?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: if event.catalog_id == "electrical-flow.matrix-scalar-product" {
                "matrix scalar products"
            } else {
                match stage {
                    ElectricalFlowStage::ConjugateGradientIteration => "residual L2",
                    ElectricalFlowStage::RecoverCurrents => "total energy",
                    ElectricalFlowStage::CheckExactReference | ElectricalFlowStage::Complete => {
                        "maximum absolute error"
                    }
                    _ => "grounded dimension",
                }
            }
            .to_owned(),
            value: if event.catalog_id == "electrical-flow.matrix-scalar-product" {
                event.after.metrics.matrix_scalar_products.to_string()
            } else {
                match stage {
                    ElectricalFlowStage::ConjugateGradientIteration => {
                        event.after.residual_l2.decimal()
                    }
                    ElectricalFlowStage::RecoverCurrents => event.after.total_energy.decimal(),
                    ElectricalFlowStage::CheckExactReference | ElectricalFlowStage::Complete => {
                        event
                            .after
                            .maximum_absolute_error
                            .map_or_else(|| "0".to_owned(), flow::ElectricalScalar::decimal)
                    }
                    _ => event.after.metrics.grounded_dimension.to_string(),
                }
            },
        }),
    })
}

const fn electrical_flow_pseudocode_line(stage: ElectricalFlowStage) -> &'static str {
    match stage {
        ElectricalFlowStage::Ready => "validate undirected resistor model",
        ElectricalFlowStage::AssembleLaplacian => "assemble L = B C B^T and ground t",
        ElectricalFlowStage::InitializeConjugateGradient => {
            "initialize Jacobi-preconditioned residual"
        }
        ElectricalFlowStage::ConjugateGradientIteration => {
            "advance one conjugate-gradient direction"
        }
        ElectricalFlowStage::RecoverCurrents => "recover f = C B^T phi and edge energy",
        ElectricalFlowStage::CheckExactReference => {
            "compare KCL, Ohm, energy, and exact rational solve"
        }
        ElectricalFlowStage::Complete => "publish minimum-energy primitive certificate",
    }
}

fn apply_electrical_flow_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    result: &ElectricalFlowResult,
) -> Result<(), JsError> {
    scene
        .apply_electrical_flow_boundary(
            graph,
            source,
            sink,
            electrical_flow_overlay(graph, sink, &result.final_snapshot)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_electrical_flow_metrics(result.metrics);
    scene
        .set_electrical_flow_outcome()
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(())
}

fn augmenting_electrical_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    run: &AugmentingElectricalTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("augmenting-electrical-flow event count overflow"))?;
    // The reduction and its typed overlay are source-owned work. Keep Ready
    // untouched and publish them with the first recorded construction event.
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new(
                "augmenting-electrical-flow trace discontinuity",
            ));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("augmenting-electrical-flow event identity overflow"))?;
        let mut scene = base.clone();
        scene
            .apply_augmenting_electrical_boundary(
                graph,
                source,
                sink,
                &augmenting_electrical_flows(graph, &event.after)?,
                augmenting_electrical_overlay(graph, &event.after)?,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(augmenting_electrical_trace_event_scene(
            graph, event, event_id,
        )?);
        scene.set_augmenting_electrical_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene
                .set_augmenting_electrical_outcome(&run.result.certificate)
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        timeline.push_with_deferred_size_validation(scene);
        current = event.after.clone();
    }
    if current != run.final_snapshot
        || run.final_snapshot != run.result.final_snapshot
        || run.events.is_empty()
    {
        return Err(JsError::new(
            "augmenting-electrical-flow final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn augmenting_electrical_flows(
    graph: &flow::FlowNetwork,
    snapshot: &AugmentingElectricalSnapshot,
) -> Result<Vec<u64>, JsError> {
    if snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new(
            "augmenting-electrical-flow snapshot edge count mismatch",
        ));
    }
    graph
        .edges()
        .iter()
        .zip(&snapshot.edges)
        .map(|(edge, state)| {
            if state.edge != *edge.id() {
                return Err(JsError::new(
                    "augmenting-electrical-flow edge identity mismatch",
                ));
            }
            Ok(state.final_flow.unwrap_or(0))
        })
        .collect()
}

#[expect(
    clippy::too_many_lines,
    reason = "the converter keeps the node, edge, path, extraction, and scalar schema projection together"
)]
fn augmenting_electrical_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &AugmentingElectricalSnapshot,
) -> Result<FlowAugmentingElectricalOverlayV1, JsError> {
    if snapshot.nodes.len() != graph.nodes().len() || snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new(
            "augmenting-electrical-flow snapshot shape mismatch",
        ));
    }
    let nodes = graph
        .nodes()
        .iter()
        .zip(&snapshot.nodes)
        .map(|(node, state)| {
            if state.node.as_usize() >= graph.nodes().len()
                || graph.nodes()[state.node.as_usize()].id() != node.id()
            {
                return Err(JsError::new(
                    "augmenting-electrical-flow node identity mismatch",
                ));
            }
            Ok(FlowAugmentingElectricalNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                potential: state.potential.decimal(),
                coupling_violation: state.coupling_violation.decimal(),
                target_source_side: state.target_source_side,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let edges = graph
        .edges()
        .iter()
        .zip(&snapshot.edges)
        .map(|(edge, state)| {
            if state.edge != *edge.id() {
                return Err(JsError::new(
                    "augmenting-electrical-flow edge identity mismatch",
                ));
            }
            Ok(FlowAugmentingElectricalEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                central_flow: state.central_flow.decimal(),
                electrical_current: state.electrical_current.decimal(),
                forward_residual: state.forward_residual.decimal(),
                backward_residual: state.backward_residual.decimal(),
                congestion: state.congestion.decimal(),
                resistance: state.resistance.decimal(),
                boost_segments: state.boost_segments.to_string(),
                rounded_central_flow: state.rounded_central_flow.map(|flow| flow.to_string()),
                extraction_central_scaled: state
                    .extraction_central_scaled
                    .map(|flow| flow.to_string()),
                extraction_toward_source: state
                    .extraction_toward_source
                    .map(|flow| flow.to_string()),
                extraction_out_of_sink: state.extraction_out_of_sink.map(|flow| flow.to_string()),
                final_flow: state.final_flow.map(|flow| flow.to_string()),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowAugmentingElectricalOverlayV1 {
        stage: augmenting_electrical_scene_stage(snapshot.stage),
        original_target: snapshot.original_target.to_string(),
        transformed_target: snapshot.transformed_target.to_string(),
        working_target: snapshot.working_target.to_string(),
        current_value: snapshot.current_value.decimal(),
        alpha: snapshot.alpha.decimal(),
        remaining: snapshot.remaining.decimal(),
        electrical_energy: snapshot.electrical_energy.decimal(),
        congestion_l3: snapshot.congestion_l3.decimal(),
        congestion_l4: snapshot.congestion_l4.decimal(),
        coupling_l2: snapshot.coupling_l2.decimal(),
        working_nodes: snapshot.working_nodes.to_string(),
        working_edges: snapshot.working_edges.to_string(),
        active_working_edge: snapshot.active_working_edge.map(|edge| edge.to_string()),
        active_pivot_node: snapshot.active_pivot_node.map(|node| node.to_string()),
        active_working_path: snapshot
            .active_working_path
            .iter()
            .map(|arc| {
                let from = graph
                    .node(arc.from)
                    .ok_or_else(|| JsError::new("augmenting-electrical path source is missing"))?;
                let to = graph
                    .node(arc.to)
                    .ok_or_else(|| JsError::new("augmenting-electrical path target is missing"))?;
                Ok(flow::scene::FlowAugmentingElectricalWorkingArcV1 {
                    edge: arc.edge.to_string(),
                    direction: if arc.forward { "forward" } else { "reverse" }.to_owned(),
                    from_node: from.id().as_str().to_owned(),
                    to_node: to.id().as_str().to_owned(),
                    flow_after: arc.flow_after.to_string(),
                })
            })
            .collect::<Result<Vec<_>, JsError>>()?,
        active_extraction_cycle: snapshot
            .active_extraction_cycle
            .iter()
            .map(|arc| flow::scene::FlowAugmentingElectricalExtractionArcV1 {
                edge: arc.edge.to_string(),
                kind: match arc.kind {
                    flow::AugmentingElectricalExtractionArcKind::Central => {
                        flow::scene::FlowAugmentingElectricalExtractionArcKindV1::Central
                    }
                    flow::AugmentingElectricalExtractionArcKind::TowardSource => {
                        flow::scene::FlowAugmentingElectricalExtractionArcKindV1::TowardSource
                    }
                    flow::AugmentingElectricalExtractionArcKind::OutOfSink => {
                        flow::scene::FlowAugmentingElectricalExtractionArcKindV1::OutOfSink
                    }
                },
            })
            .collect(),
        active_discrete_amount: snapshot
            .active_discrete_amount
            .map(|amount| amount.to_string()),
        nodes,
        edges,
    })
}

const fn augmenting_electrical_scene_stage(
    stage: AugmentingElectricalStage,
) -> FlowAugmentingElectricalStageV1 {
    match stage {
        AugmentingElectricalStage::Ready => FlowAugmentingElectricalStageV1::Ready,
        AugmentingElectricalStage::BuildDirectedReduction => {
            FlowAugmentingElectricalStageV1::BuildDirectedReduction
        }
        AugmentingElectricalStage::AddPreconditioning => {
            FlowAugmentingElectricalStageV1::AddPreconditioning
        }
        AugmentingElectricalStage::InstallTargetCut => {
            FlowAugmentingElectricalStageV1::InstallTargetCut
        }
        AugmentingElectricalStage::SolveElectricalDirection
        | AugmentingElectricalStage::SolveElectricalPivot => {
            FlowAugmentingElectricalStageV1::SolveElectricalDirection
        }
        AugmentingElectricalStage::BoostHighEnergyArc => {
            FlowAugmentingElectricalStageV1::BoostHighEnergyArc
        }
        AugmentingElectricalStage::AugmentPrimalDual => {
            FlowAugmentingElectricalStageV1::AugmentPrimalDual
        }
        AugmentingElectricalStage::FixCoupling => FlowAugmentingElectricalStageV1::FixCoupling,
        AugmentingElectricalStage::CollapseBoostPaths => {
            FlowAugmentingElectricalStageV1::CollapseBoostPaths
        }
        AugmentingElectricalStage::RoundCentralFlow => {
            FlowAugmentingElectricalStageV1::RoundCentralFlow
        }
        AugmentingElectricalStage::CleanupAugmentingPath => {
            FlowAugmentingElectricalStageV1::CleanupAugmentingPath
        }
        AugmentingElectricalStage::ExtractDirectedFlow => {
            FlowAugmentingElectricalStageV1::ExtractDirectedFlow
        }
        AugmentingElectricalStage::CancelExtractionCycle => {
            FlowAugmentingElectricalStageV1::CancelExtractionCycle
        }
        AugmentingElectricalStage::RoundDirectedFlow => {
            FlowAugmentingElectricalStageV1::RoundDirectedFlow
        }
        AugmentingElectricalStage::CheckCertificate => {
            FlowAugmentingElectricalStageV1::CheckCertificate
        }
        AugmentingElectricalStage::Optimal => FlowAugmentingElectricalStageV1::Optimal,
    }
}

fn augmenting_electrical_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &AugmentingElectricalTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let changed_node_indices = event
        .before
        .nodes
        .iter()
        .zip(&event.after.nodes)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let changed_edge_indices = event
        .before
        .edges
        .iter()
        .zip(&event.after.edges)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let changed_entity_refs: Vec<FlowTraceEntityRefSceneV1> = if changed_edge_indices.is_empty() {
        changed_node_indices
            .iter()
            .map(|index| FlowTraceEntityRefSceneV1::Node {
                node_id: graph.nodes()[*index].id().as_str().to_owned(),
            })
            .collect()
    } else {
        changed_edge_indices
            .iter()
            .map(|index| FlowTraceEntityRefSceneV1::Edge {
                edge_id: graph.edges()[*index].id().as_str().to_owned(),
            })
            .collect()
    };
    // Direction solves and primal-dual vector updates are global typed-overlay
    // changes. Keep the cyan local focus for an actual elimination pivot (or
    // an extraction path whose original edges changed), never for the entire
    // numerical state vector.
    let mut entity_refs = if matches!(
        event.after.stage,
        AugmentingElectricalStage::CleanupAugmentingPath
            | AugmentingElectricalStage::ExtractDirectedFlow
            | AugmentingElectricalStage::CancelExtractionCycle
            | AugmentingElectricalStage::RoundDirectedFlow
    ) {
        changed_entity_refs
    } else {
        Vec::new()
    };
    if let Some(node) = event.after.active_pivot_node {
        let node = usize::try_from(node)
            .map_err(|_| JsError::new("augmenting-electrical pivot node overflow"))?;
        if let Some(state) = graph.nodes().get(node) {
            let candidate = FlowTraceEntityRefSceneV1::Node {
                node_id: state.id().as_str().to_owned(),
            };
            if !entity_refs.contains(&candidate) {
                entity_refs.push(candidate);
            }
        }
    }
    let stage = event.after.stage;
    let (label, value) = augmenting_electrical_event_detail(&event.after);
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: (event_id > 1).then(|| "1".to_owned()),
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: if matches!(
            stage,
            AugmentingElectricalStage::SolveElectricalDirection
                | AugmentingElectricalStage::AugmentPrimalDual
                | AugmentingElectricalStage::FixCoupling
                | AugmentingElectricalStage::CleanupAugmentingPath
        ) {
            TraceGranularityV1::Operation
        } else if stage == AugmentingElectricalStage::SolveElectricalPivot {
            TraceGranularityV1::Micro
        } else {
            TraceGranularityV1::Phase
        },
        pseudocode_line: augmenting_electrical_pseudocode_line(stage).to_owned(),
        patch_count: u32::try_from(changed_node_indices.len() + changed_edge_indices.len() + 1)
            .map_err(|_| JsError::new("augmenting-electrical-flow patch count overflow"))?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn augmenting_electrical_event_detail(
    snapshot: &AugmentingElectricalSnapshot,
) -> (&'static str, String) {
    match snapshot.stage {
        AugmentingElectricalStage::SolveElectricalPivot => (
            "pivot equation",
            snapshot.active_pivot_node.unwrap_or(0).to_string(),
        ),
        AugmentingElectricalStage::SolveElectricalDirection => {
            ("electrical energy", snapshot.electrical_energy.decimal())
        }
        AugmentingElectricalStage::BoostHighEnergyArc => {
            ("boosts", snapshot.metrics.boosts.to_string())
        }
        AugmentingElectricalStage::AugmentPrimalDual => {
            ("routed fraction alpha", snapshot.alpha.decimal())
        }
        AugmentingElectricalStage::FixCoupling => ("coupling L2", snapshot.coupling_l2.decimal()),
        AugmentingElectricalStage::CollapseBoostPaths => {
            ("working edges", snapshot.working_edges.to_string())
        }
        AugmentingElectricalStage::CleanupAugmentingPath => {
            ("remaining", snapshot.remaining.decimal())
        }
        AugmentingElectricalStage::RoundDirectedFlow
        | AugmentingElectricalStage::CheckCertificate
        | AugmentingElectricalStage::Optimal => {
            ("original max flow", snapshot.original_target.to_string())
        }
        _ => ("working target", snapshot.working_target.to_string()),
    }
}

const fn augmenting_electrical_pseudocode_line(stage: AugmentingElectricalStage) -> &'static str {
    match stage {
        AugmentingElectricalStage::Ready => "validate bounded directed max-flow instance",
        AugmentingElectricalStage::BuildDirectedReduction => {
            "replace each directed edge by the source three-edge symmetric gadget"
        }
        AugmentingElectricalStage::AddPreconditioning => {
            "add symmetric s-t preconditioning arcs of capacity 2U"
        }
        AugmentingElectricalStage::InstallTargetCut => {
            "enumerate bounded cuts and install the exact working target"
        }
        AugmentingElectricalStage::SolveElectricalDirection => {
            "solve the residual-barrier electrical direction r = 1/u+^2 + 1/u-^2"
        }
        AugmentingElectricalStage::SolveElectricalPivot => {
            "eliminate one selected Laplacian equation row"
        }
        AugmentingElectricalStage::BoostHighEnergyArc => {
            "replace one high-energy edge by the source-defined boost path"
        }
        AugmentingElectricalStage::AugmentPrimalDual => {
            "advance f and y by an l4-safe electrical step"
        }
        AugmentingElectricalStage::FixCoupling => {
            "solve the fixing electrical flow and restore coupling"
        }
        AugmentingElectricalStage::CollapseBoostPaths => {
            "contract explicit boost paths to their throughput-equivalent roots"
        }
        AugmentingElectricalStage::RoundCentralFlow => {
            "round the central flow inside floor/ceiling bounds"
        }
        AugmentingElectricalStage::CleanupAugmentingPath => {
            "augment one integral residual path toward the exact target"
        }
        AugmentingElectricalStage::ExtractDirectedFlow => {
            "remove preconditioners and invert the three-edge reduction"
        }
        AugmentingElectricalStage::CancelExtractionCycle => {
            "cancel one auxiliary cycle in doubled extraction units"
        }
        AugmentingElectricalStage::RoundDirectedFlow => "round the half-integral directed witness",
        AugmentingElectricalStage::CheckCertificate => {
            "independently check feasibility, maximality, and minimum cut"
        }
        AugmentingElectricalStage::Optimal => "publish the certified directed maximum flow",
    }
}

fn apply_augmenting_electrical_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    result: &AugmentingElectricalResult,
) -> Result<(), JsError> {
    if result.flows != augmenting_electrical_flows(graph, &result.final_snapshot)? {
        return Err(JsError::new(
            "augmenting-electrical-flow result projection mismatch",
        ));
    }
    scene
        .apply_augmenting_electrical_boundary(
            graph,
            source,
            sink,
            &result.flows,
            augmenting_electrical_overlay(graph, &result.final_snapshot)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_augmenting_electrical_metrics(result.metrics);
    scene
        .set_augmenting_electrical_outcome(&result.certificate)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(())
}

fn interior_point_max_flow_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    run: &InteriorPointMaxFlowTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("interior-point-max-flow event count overflow"))?;
    // The source's first reduction event owns the IPM overlay; Ready remains
    // the unchanged public graph with no precomputed central state.
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("interior-point-max-flow trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("interior-point-max-flow event identity overflow"))?;
        let mut scene = base.clone();
        scene
            .apply_interior_point_max_flow_boundary(
                graph,
                source,
                sink,
                &interior_point_max_flow_flows(graph, &event.after)?,
                interior_point_max_flow_overlay(graph, &event.after)?,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(interior_point_max_flow_trace_event_scene(
            graph, event, event_id,
        )?);
        scene.set_interior_point_max_flow_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene
                .set_interior_point_max_flow_outcome(&run.result.certificate)
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot
        || run.final_snapshot != run.result.final_snapshot
        || run.events.is_empty()
    {
        return Err(JsError::new(
            "interior-point-max-flow final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn interior_point_max_flow_flows(
    graph: &flow::FlowNetwork,
    snapshot: &InteriorPointMaxFlowSnapshot,
) -> Result<Vec<u64>, JsError> {
    if snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new(
            "interior-point-max-flow snapshot edge count mismatch",
        ));
    }
    graph
        .edges()
        .iter()
        .zip(&snapshot.edges)
        .map(|(edge, state)| {
            if state.edge != *edge.id() {
                return Err(JsError::new(
                    "interior-point-max-flow edge identity mismatch",
                ));
            }
            Ok(state.final_flow.unwrap_or(0))
        })
        .collect()
}

fn interior_point_max_flow_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &InteriorPointMaxFlowSnapshot,
) -> Result<FlowInteriorPointMaxFlowOverlayV1, JsError> {
    if snapshot.nodes.len() != graph.nodes().len() || snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new(
            "interior-point-max-flow snapshot shape mismatch",
        ));
    }
    let nodes = graph
        .nodes()
        .iter()
        .zip(&snapshot.nodes)
        .map(|(node, state)| {
            if state.node.as_usize() >= graph.nodes().len()
                || graph.nodes()[state.node.as_usize()].id() != node.id()
            {
                return Err(JsError::new(
                    "interior-point-max-flow node identity mismatch",
                ));
            }
            Ok(FlowInteriorPointNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                potential: state.potential.decimal(),
                target_source_side: state.target_source_side,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let edges = graph
        .edges()
        .iter()
        .zip(&snapshot.edges)
        .map(|(edge, state)| {
            if state.edge != *edge.id() {
                return Err(JsError::new(
                    "interior-point-max-flow edge identity mismatch",
                ));
            }
            Ok(FlowInteriorPointEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                fractional_flow: state.fractional_flow.decimal(),
                electrical_current: state.electrical_current.decimal(),
                slack: state.slack.decimal(),
                measure: state.measure.decimal(),
                resistance: state.resistance.decimal(),
                congestion: state.congestion.decimal(),
                normalized_away: state.normalized_away,
                final_flow: state.final_flow.map(|flow| flow.to_string()),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowInteriorPointMaxFlowOverlayV1 {
        stage: interior_point_max_flow_scene_stage(snapshot.stage),
        target_value: snapshot.target_value.to_string(),
        mu: snapshot.mu.decimal(),
        duality_gap: snapshot.duality_gap.decimal(),
        centrality: snapshot.centrality.decimal(),
        congestion_l4: snapshot.congestion_l4.decimal(),
        step_size: snapshot.step_size.decimal(),
        electrical_energy: snapshot.electrical_energy.decimal(),
        b_matching_nodes: snapshot.b_matching_nodes.to_string(),
        b_matching_edges: snapshot.b_matching_edges.to_string(),
        working_nodes: snapshot.working_nodes.to_string(),
        working_edges: snapshot.working_edges.to_string(),
        active_working_edge: snapshot.active_working_edge.map(|edge| edge.to_string()),
        nodes,
        edges,
    })
}

const fn interior_point_max_flow_scene_stage(
    stage: InteriorPointMaxFlowStage,
) -> FlowInteriorPointMaxFlowStageV1 {
    match stage {
        InteriorPointMaxFlowStage::Ready => FlowInteriorPointMaxFlowStageV1::Ready,
        InteriorPointMaxFlowStage::EnumerateTargetCut => {
            FlowInteriorPointMaxFlowStageV1::EnumerateTargetCut
        }
        InteriorPointMaxFlowStage::BuildBMatchingReduction => {
            FlowInteriorPointMaxFlowStageV1::BuildBMatchingReduction
        }
        InteriorPointMaxFlowStage::BuildMinCostReduction => {
            FlowInteriorPointMaxFlowStageV1::BuildMinCostReduction
        }
        InteriorPointMaxFlowStage::InitializeCentralPath => {
            FlowInteriorPointMaxFlowStageV1::InitializeCentralPath
        }
        InteriorPointMaxFlowStage::SolveElectricalDirection
        | InteriorPointMaxFlowStage::SolveElectricalPivot => {
            FlowInteriorPointMaxFlowStageV1::SolveElectricalDirection
        }
        InteriorPointMaxFlowStage::DescentStep => FlowInteriorPointMaxFlowStageV1::DescentStep,
        InteriorPointMaxFlowStage::SolveCenteringDirection => {
            FlowInteriorPointMaxFlowStageV1::SolveCenteringDirection
        }
        InteriorPointMaxFlowStage::CenteringStep => FlowInteriorPointMaxFlowStageV1::CenteringStep,
        InteriorPointMaxFlowStage::ExtractFractionalFlow => {
            FlowInteriorPointMaxFlowStageV1::ExtractFractionalFlow
        }
        InteriorPointMaxFlowStage::RoundIntegralFlow => {
            FlowInteriorPointMaxFlowStageV1::RoundIntegralFlow
        }
        InteriorPointMaxFlowStage::CheckCertificate => {
            FlowInteriorPointMaxFlowStageV1::CheckCertificate
        }
        InteriorPointMaxFlowStage::Optimal => FlowInteriorPointMaxFlowStageV1::Optimal,
    }
}

fn interior_point_max_flow_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &InteriorPointMaxFlowTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let changed_nodes = event
        .before
        .nodes
        .iter()
        .zip(&event.after.nodes)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let changed_edges = event
        .before
        .edges
        .iter()
        .zip(&event.after.edges)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let changed_entity_refs: Vec<FlowTraceEntityRefSceneV1> = if changed_edges.is_empty() {
        changed_nodes
            .iter()
            .map(|index| FlowTraceEntityRefSceneV1::Node {
                node_id: graph.nodes()[*index].id().as_str().to_owned(),
            })
            .collect()
    } else {
        changed_edges
            .iter()
            .map(|index| FlowTraceEntityRefSceneV1::Edge {
                edge_id: graph.edges()[*index].id().as_str().to_owned(),
            })
            .collect()
    };
    // The associated electrical solve updates dense reduced vectors. Its
    // potentials, currents, slacks, and convergence scalars already have a
    // dedicated overlay; only a selected elimination row is a local focus.
    let mut entity_refs = if matches!(
        event.after.stage,
        InteriorPointMaxFlowStage::ExtractFractionalFlow
            | InteriorPointMaxFlowStage::RoundIntegralFlow
    ) {
        changed_entity_refs
    } else {
        Vec::new()
    };
    if event.after.stage == InteriorPointMaxFlowStage::SolveElectricalPivot {
        entity_refs.extend(event.after.active_pivot_original_edges.iter().map(|edge| {
            FlowTraceEntityRefSceneV1::Edge {
                edge_id: edge.as_str().to_owned(),
            }
        }));
        entity_refs.extend(event.after.active_pivot_original_nodes.iter().map(|node| {
            FlowTraceEntityRefSceneV1::Node {
                node_id: graph.nodes()[node.as_usize()].id().as_str().to_owned(),
            }
        }));
    }
    let stage = event.after.stage;
    let (label, value) = interior_point_max_flow_event_detail(&event.after)?;
    let catalog_id = event.catalog_id.strip_prefix("ipm.").map_or_else(
        || event.catalog_id.to_owned(),
        |suffix| format!("interior-point-max-flow.{suffix}"),
    );
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: (event_id > 1).then(|| "1".to_owned()),
        catalog_id,
        minimum_granularity: if matches!(
            stage,
            InteriorPointMaxFlowStage::SolveElectricalDirection
                | InteriorPointMaxFlowStage::DescentStep
                | InteriorPointMaxFlowStage::SolveCenteringDirection
                | InteriorPointMaxFlowStage::CenteringStep
        ) {
            TraceGranularityV1::Operation
        } else if stage == InteriorPointMaxFlowStage::SolveElectricalPivot {
            TraceGranularityV1::Micro
        } else {
            TraceGranularityV1::Phase
        },
        pseudocode_line: interior_point_max_flow_pseudocode_line(stage).to_owned(),
        patch_count: u32::try_from(changed_nodes.len() + changed_edges.len() + 1)
            .map_err(|_| JsError::new("interior-point-max-flow patch count overflow"))?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn interior_point_max_flow_event_detail(
    snapshot: &InteriorPointMaxFlowSnapshot,
) -> Result<(&'static str, String), JsError> {
    Ok(match snapshot.stage {
        InteriorPointMaxFlowStage::EnumerateTargetCut
        | InteriorPointMaxFlowStage::RoundIntegralFlow
        | InteriorPointMaxFlowStage::CheckCertificate
        | InteriorPointMaxFlowStage::Optimal => ("target flow", snapshot.target_value.to_string()),
        InteriorPointMaxFlowStage::BuildBMatchingReduction => {
            ("b-matching edges", snapshot.b_matching_edges.to_string())
        }
        InteriorPointMaxFlowStage::BuildMinCostReduction => {
            ("G_b arcs", snapshot.working_edges.to_string())
        }
        InteriorPointMaxFlowStage::SolveElectricalDirection
        | InteriorPointMaxFlowStage::SolveCenteringDirection => {
            ("electrical energy", snapshot.electrical_energy.decimal())
        }
        InteriorPointMaxFlowStage::SolveElectricalPivot => (
            "pivot working node",
            snapshot
                .active_pivot_node
                .ok_or_else(|| JsError::new("interior-point pivot node is absent"))?
                .to_string(),
        ),
        InteriorPointMaxFlowStage::DescentStep => ("step delta", snapshot.step_size.decimal()),
        InteriorPointMaxFlowStage::InitializeCentralPath
        | InteriorPointMaxFlowStage::CenteringStep
        | InteriorPointMaxFlowStage::ExtractFractionalFlow => {
            ("duality gap", snapshot.duality_gap.decimal())
        }
        InteriorPointMaxFlowStage::Ready => ("target flow", "0".to_owned()),
    })
}

const fn interior_point_max_flow_pseudocode_line(stage: InteriorPointMaxFlowStage) -> &'static str {
    match stage {
        InteriorPointMaxFlowStage::Ready => "validate bounded unit-capacity input",
        InteriorPointMaxFlowStage::EnumerateTargetCut => {
            "enumerate bounded source-side cuts and install F*"
        }
        InteriorPointMaxFlowStage::BuildBMatchingReduction => {
            "build the source G -> G-bar perfect b-matching reduction"
        }
        InteriorPointMaxFlowStage::BuildMinCostReduction => {
            "build the source G-bar -> G_b unit-length demand-flow reduction"
        }
        InteriorPointMaxFlowStage::InitializeCentralPath => {
            "install Lemma 5.4's explicit zero-centered (f,s,nu)"
        }
        InteriorPointMaxFlowStage::SolveElectricalDirection => {
            "solve the associated electrical demand flow with r = s/f"
        }
        InteriorPointMaxFlowStage::SolveElectricalPivot => {
            "eliminate one selected reduced Laplacian equation row"
        }
        InteriorPointMaxFlowStage::DescentStep => {
            "apply equations (38)-(40) to primal flow and dual slack"
        }
        InteriorPointMaxFlowStage::SolveCenteringDirection => {
            "solve the equations (44)-(47) electrical correction"
        }
        InteriorPointMaxFlowStage::CenteringStep => {
            "apply equation (48) and restore gamma-centered feasibility"
        }
        InteriorPointMaxFlowStage::ExtractFractionalFlow => {
            "cancel working cycles and decompose direct P-to-Q paths into a fractional b-matching"
        }
        InteriorPointMaxFlowStage::RoundIntegralFlow => {
            "split demands, match the completed support, augment to a perfect b-matching, and extract flow"
        }
        InteriorPointMaxFlowStage::CheckCertificate => {
            "independently check feasibility, maximality, and original minimum cut"
        }
        InteriorPointMaxFlowStage::Optimal => "publish the certified unit-capacity maximum flow",
    }
}

fn apply_interior_point_max_flow_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    result: &InteriorPointMaxFlowResult,
) -> Result<(), JsError> {
    if result.flows != interior_point_max_flow_flows(graph, &result.final_snapshot)? {
        return Err(JsError::new(
            "interior-point-max-flow result projection mismatch",
        ));
    }
    scene
        .apply_interior_point_max_flow_boundary(
            graph,
            source,
            sink,
            &result.flows,
            interior_point_max_flow_overlay(graph, &result.final_snapshot)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_interior_point_max_flow_metrics(result.metrics);
    scene
        .set_interior_point_max_flow_outcome(&result.certificate)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(())
}

fn minimum_ratio_cycle_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &MinimumRatioCycleTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("minimum-ratio-cycle event count overflow"))?;
    // Candidate vectors and cycle state first appear at their source event;
    // they are not part of the public Ready graph.
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("minimum-ratio-cycle trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("minimum-ratio-cycle event identity overflow"))?;
        let mut scene = base.clone();
        scene
            .apply_minimum_ratio_cycle_boundary(
                graph,
                minimum_ratio_cycle_overlay(graph, &event.after)?,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(minimum_ratio_cycle_trace_event_scene(
            graph, event, event_id,
        )?);
        scene.set_minimum_ratio_cycle_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene
                .set_minimum_ratio_cycle_outcome()
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot
        || run.final_snapshot != run.result.final_snapshot
        || run.events.is_empty()
    {
        return Err(JsError::new("minimum-ratio-cycle final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn minimum_ratio_cycle_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &MinimumRatioCycleSnapshot,
) -> Result<FlowMinimumRatioCycleOverlayV1, JsError> {
    if snapshot.nodes.len() != graph.nodes().len() || snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new("minimum-ratio-cycle snapshot shape mismatch"));
    }
    let nodes = graph
        .nodes()
        .iter()
        .zip(&snapshot.nodes)
        .map(|(node, state)| {
            if state.node.as_usize() >= graph.nodes().len()
                || graph.nodes()[state.node.as_usize()].id() != node.id()
            {
                return Err(JsError::new("minimum-ratio-cycle node identity mismatch"));
            }
            Ok(FlowMinimumRatioCycleNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                component: state.component.to_string(),
                parent_node_id: state
                    .parent
                    .map(|parent| graph.nodes()[parent.as_usize()].id().as_str().to_owned()),
                depth: state.depth.to_string(),
                candidate_balance: state.candidate_balance.to_string(),
                on_candidate: state.on_candidate,
                on_selected: state.on_selected,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let edges = graph
        .edges()
        .iter()
        .zip(&snapshot.edges)
        .map(|(edge, state)| {
            if state.edge != *edge.id() {
                return Err(JsError::new("minimum-ratio-cycle edge identity mismatch"));
            }
            Ok(FlowMinimumRatioCycleEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                gradient: state.gradient.to_string(),
                length: state.length.to_string(),
                tree_edge: state.tree_edge,
                candidate_sign: state.candidate_sign.to_string(),
                selected_sign: state.selected_sign.to_string(),
                numerator_contribution: state.numerator_contribution.to_string(),
                denominator_contribution: state.denominator_contribution.to_string(),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowMinimumRatioCycleOverlayV1 {
        stage: minimum_ratio_cycle_scene_stage(snapshot.stage),
        candidate_ratio: snapshot.candidate_ratio.map(|ratio| FlowRationalV1 {
            numerator: ratio.numerator.to_string(),
            denominator: ratio.denominator.to_string(),
        }),
        best_ratio: snapshot.best_ratio.map(|ratio| FlowRationalV1 {
            numerator: ratio.numerator.to_string(),
            denominator: ratio.denominator.to_string(),
        }),
        selected_edge_count: snapshot.selected_edge_count.to_string(),
        maximum_absolute_balance: snapshot.maximum_absolute_balance.to_string(),
        enumerated_vectors: snapshot.metrics.enumerated_vectors.to_string(),
        simple_cycles: snapshot.metrics.simple_cycles.to_string(),
        fundamental_cycles: snapshot.metrics.fundamental_cycles.to_string(),
        nodes,
        edges,
    })
}

const fn minimum_ratio_cycle_scene_stage(
    stage: MinimumRatioCycleStage,
) -> FlowMinimumRatioCycleStageV1 {
    match stage {
        MinimumRatioCycleStage::Ready => FlowMinimumRatioCycleStageV1::Ready,
        MinimumRatioCycleStage::MapGradientLength => {
            FlowMinimumRatioCycleStageV1::MapGradientLength
        }
        MinimumRatioCycleStage::BuildSpanningForest => {
            FlowMinimumRatioCycleStageV1::BuildSpanningForest
        }
        MinimumRatioCycleStage::InspectVector => FlowMinimumRatioCycleStageV1::InspectVector,
        MinimumRatioCycleStage::EvaluateCycle => FlowMinimumRatioCycleStageV1::EvaluateCycle,
        MinimumRatioCycleStage::UpdateBest => FlowMinimumRatioCycleStageV1::UpdateBest,
        MinimumRatioCycleStage::VerifyCycleSpace => FlowMinimumRatioCycleStageV1::VerifyCycleSpace,
        MinimumRatioCycleStage::CheckExhaustiveOracle => {
            FlowMinimumRatioCycleStageV1::CheckExhaustiveOracle
        }
        MinimumRatioCycleStage::Complete => FlowMinimumRatioCycleStageV1::Complete,
    }
}

fn minimum_ratio_cycle_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &MinimumRatioCycleTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let changed_nodes = event
        .before
        .nodes
        .iter()
        .zip(&event.after.nodes)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let changed_edges = event
        .before
        .edges
        .iter()
        .zip(&event.after.edges)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let publishes_global_cycle_state = matches!(
        event.after.stage,
        MinimumRatioCycleStage::InspectVector
            | MinimumRatioCycleStage::EvaluateCycle
            | MinimumRatioCycleStage::UpdateBest
    );
    let mut entity_refs = if publishes_global_cycle_state {
        // These boundaries publish a complete ternary vector, its connected
        // cycle, or the incumbent cycle. Their per-edge signs are already
        // rendered by the typed overlay. Treating every support edge as local
        // Detail focus would falsely present one global checkpoint as many
        // simultaneously inspected primitives.
        Vec::new()
    } else {
        changed_edges
            .iter()
            .map(|index| FlowTraceEntityRefSceneV1::Edge {
                edge_id: graph.edges()[*index].id().as_str().to_owned(),
            })
            .collect::<Vec<_>>()
    };
    if entity_refs.is_empty() && !publishes_global_cycle_state {
        entity_refs.extend(
            changed_nodes
                .iter()
                .map(|index| FlowTraceEntityRefSceneV1::Node {
                    node_id: graph.nodes()[*index].id().as_str().to_owned(),
                }),
        );
    }
    let (label, value) = minimum_ratio_cycle_event_detail(&event.after);
    let catalog_id = event
        .catalog_id
        .strip_prefix("minimum-ratio-cycle.")
        .map_or_else(
            || event.catalog_id.to_owned(),
            |suffix| format!("minimum-ratio-cycle-max-flow.{suffix}"),
        );
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: (event_id > 2).then(|| "2".to_owned()),
        catalog_id,
        minimum_granularity: if matches!(
            event.after.stage,
            MinimumRatioCycleStage::InspectVector
                | MinimumRatioCycleStage::EvaluateCycle
                | MinimumRatioCycleStage::UpdateBest
        ) {
            if event.after.stage == MinimumRatioCycleStage::InspectVector {
                TraceGranularityV1::Micro
            } else {
                TraceGranularityV1::Operation
            }
        } else {
            TraceGranularityV1::Phase
        },
        pseudocode_line: minimum_ratio_cycle_pseudocode_line(event.after.stage).to_owned(),
        patch_count: u32::try_from(changed_nodes.len() + changed_edges.len() + 1)
            .map_err(|_| JsError::new("minimum-ratio-cycle patch count overflow"))?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn minimum_ratio_cycle_event_detail(
    snapshot: &MinimumRatioCycleSnapshot,
) -> (&'static str, String) {
    match snapshot.stage {
        MinimumRatioCycleStage::MapGradientLength => {
            ("mapped edges", snapshot.edges.len().to_string())
        }
        MinimumRatioCycleStage::BuildSpanningForest => (
            "cycle-space dimension",
            snapshot.metrics.fundamental_cycles.to_string(),
        ),
        MinimumRatioCycleStage::InspectVector => (
            "ternary vector",
            snapshot.metrics.enumerated_vectors.to_string(),
        ),
        MinimumRatioCycleStage::EvaluateCycle => (
            "candidate numerator",
            snapshot
                .candidate_ratio
                .map_or(0, |ratio| ratio.numerator)
                .to_string(),
        ),
        MinimumRatioCycleStage::UpdateBest => (
            "best numerator",
            snapshot
                .best_ratio
                .map_or(0, |ratio| ratio.numerator)
                .to_string(),
        ),
        MinimumRatioCycleStage::VerifyCycleSpace => (
            "maximum balance",
            snapshot.maximum_absolute_balance.to_string(),
        ),
        MinimumRatioCycleStage::CheckExhaustiveOracle => (
            "DFS expansions",
            snapshot.metrics.dfs_expansions.to_string(),
        ),
        MinimumRatioCycleStage::Complete => {
            ("simple cycles", snapshot.metrics.simple_cycles.to_string())
        }
        MinimumRatioCycleStage::Ready => ("mapped edges", "0".to_owned()),
    }
}

const fn minimum_ratio_cycle_pseudocode_line(stage: MinimumRatioCycleStage) -> &'static str {
    match stage {
        MinimumRatioCycleStage::Ready => "validate the bounded undirected ratio instance",
        MinimumRatioCycleStage::MapGradientLength => "map cost to g and positive capacity to l",
        MinimumRatioCycleStage::BuildSpanningForest => {
            "build the canonical forest and fundamental cycle basis"
        }
        MinimumRatioCycleStage::InspectVector => {
            "inspect one geometrically spaced ternary sign vector"
        }
        MinimumRatioCycleStage::EvaluateCycle => {
            "evaluate g^T delta / ||diag(l) delta||_1 for one simple circulation"
        }
        MinimumRatioCycleStage::UpdateBest => "replace the exact minimum-ratio incumbent",
        MinimumRatioCycleStage::VerifyCycleSpace => {
            "verify B^T delta = 0, degree two, and connected support"
        }
        MinimumRatioCycleStage::CheckExhaustiveOracle => {
            "compare against the independent DFS simple-cycle oracle"
        }
        MinimumRatioCycleStage::Complete => {
            "publish the primitive certificate without a max-flow claim"
        }
    }
}

fn apply_minimum_ratio_cycle_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    result: &MinimumRatioCycleResult,
) -> Result<(), JsError> {
    scene
        .apply_minimum_ratio_cycle_boundary(
            graph,
            minimum_ratio_cycle_overlay(graph, &result.final_snapshot)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_minimum_ratio_cycle_metrics(result.metrics);
    scene
        .set_minimum_ratio_cycle_outcome()
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(())
}

fn minimum_ratio_cycle_mcf_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &MinimumRatioCycleMcfTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("minimum-ratio-cycle MCF event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("minimum-ratio-cycle MCF trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("minimum-ratio-cycle MCF event identity overflow"))?;
        let mut scene = base.clone();
        scene
            .apply_minimum_ratio_cycle_mcf_boundary(
                graph,
                minimum_ratio_cycle_mcf_overlay(graph, &event.after)?,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(minimum_ratio_cycle_mcf_trace_event_scene(
            graph, event, event_id,
        )?);
        scene.set_minimum_ratio_cycle_mcf_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene
                .set_minimum_ratio_cycle_mcf_outcome()
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot
        || run.final_snapshot != run.result.final_snapshot
        || run.events.is_empty()
    {
        return Err(JsError::new(
            "minimum-ratio-cycle MCF final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn minimum_ratio_cycle_mcf_fast_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    result: &MinimumRatioCycleMcfResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let ready = ready_flow_scene(scenario)?;
    let mut scene = ready.clone();
    scene
        .apply_minimum_ratio_cycle_mcf_boundary(
            graph,
            minimum_ratio_cycle_mcf_overlay(graph, &result.final_snapshot)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_minimum_ratio_cycle_mcf_metrics(result.metrics);
    scene
        .set_minimum_ratio_cycle_mcf_outcome()
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(vec![ready, scene])
}

fn minimum_ratio_cycle_mcf_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &MinimumRatioCycleMcfSnapshot,
) -> Result<FlowMinimumRatioCycleMcfOverlayV1, JsError> {
    if snapshot.nodes.len() != graph.nodes().len() || snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new(
            "minimum-ratio-cycle MCF snapshot shape mismatch",
        ));
    }
    let nodes = graph
        .nodes()
        .iter()
        .zip(&snapshot.nodes)
        .map(|(node, state)| {
            if state.node.as_usize() >= graph.nodes().len()
                || graph.nodes()[state.node.as_usize()].id() != node.id()
            {
                return Err(JsError::new(
                    "minimum-ratio-cycle MCF node identity mismatch",
                ));
            }
            Ok(FlowMinimumRatioCycleMcfNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                component: state.component.to_string(),
                parent_node_id: state
                    .parent
                    .map(|parent| graph.nodes()[parent.as_usize()].id().as_str().to_owned()),
                depth: state.depth.to_string(),
                candidate_balance: state.candidate_balance.to_string(),
                on_candidate: state.on_candidate,
                on_selected: state.on_selected,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let edges = graph
        .edges()
        .iter()
        .zip(&snapshot.edges)
        .map(|(edge, state)| {
            if state.edge != *edge.id() {
                return Err(JsError::new(
                    "minimum-ratio-cycle MCF edge identity mismatch",
                ));
            }
            Ok(FlowMinimumRatioCycleMcfEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                fixed_on_face: state.fixed_on_face,
                initial_flow: state.initial_flow.decimal(),
                updated_flow: state.updated_flow.decimal(),
                lower_slack: state.lower_slack.decimal(),
                upper_slack: state.upper_slack.decimal(),
                gradient: state.gradient.decimal(),
                length: state.length.decimal(),
                tree_edge: state.tree_edge,
                candidate_sign: state.candidate_sign.to_string(),
                selected_sign: state.selected_sign.to_string(),
                numerator_contribution: state.numerator_contribution.decimal(),
                denominator_contribution: state.denominator_contribution.decimal(),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowMinimumRatioCycleMcfOverlayV1 {
        stage: minimum_ratio_cycle_mcf_scene_stage(snapshot.stage),
        alpha: snapshot.alpha.decimal(),
        optimum_cost: snapshot.optimum_cost.to_string(),
        initial_cost: snapshot.initial_cost.decimal(),
        current_cost: snapshot.current_cost.decimal(),
        cost_gap: snapshot.cost_gap.decimal(),
        potential_before: snapshot.potential_before.decimal(),
        current_potential: snapshot.current_potential.decimal(),
        candidate_ratio: snapshot
            .candidate_ratio
            .map(flow::MinimumRatioCycleMcfScalar::decimal),
        best_ratio: snapshot
            .best_ratio
            .map(flow::MinimumRatioCycleMcfScalar::decimal),
        kappa: snapshot.kappa.decimal(),
        eta: snapshot.eta.decimal(),
        weighted_step_norm: snapshot.weighted_step_norm.decimal(),
        potential_decrease: snapshot.potential_decrease.decimal(),
        guaranteed_decrease: snapshot.guaranteed_decrease.decimal(),
        stationary: snapshot.stationary,
        selected_edge_count: snapshot.selected_edge_count.to_string(),
        maximum_absolute_balance: snapshot.maximum_absolute_balance.to_string(),
        feasible_flows: snapshot.metrics.feasible_flows.to_string(),
        enumerated_vectors: snapshot.metrics.enumerated_vectors.to_string(),
        simple_cycles: snapshot.metrics.simple_cycles.to_string(),
        fundamental_cycles: snapshot.metrics.fundamental_cycles.to_string(),
        nodes,
        edges,
    })
}

const fn minimum_ratio_cycle_mcf_scene_stage(
    stage: MinimumRatioCycleMcfStage,
) -> FlowMinimumRatioCycleMcfStageV1 {
    match stage {
        MinimumRatioCycleMcfStage::Ready => FlowMinimumRatioCycleMcfStageV1::Ready,
        MinimumRatioCycleMcfStage::EnumerateFeasibleSet => {
            FlowMinimumRatioCycleMcfStageV1::EnumerateFeasibleSet
        }
        MinimumRatioCycleMcfStage::ContractFixedFace => {
            FlowMinimumRatioCycleMcfStageV1::ContractFixedFace
        }
        MinimumRatioCycleMcfStage::InitializeStrictInterior => {
            FlowMinimumRatioCycleMcfStageV1::InitializeStrictInterior
        }
        MinimumRatioCycleMcfStage::EvaluatePotential => {
            FlowMinimumRatioCycleMcfStageV1::EvaluatePotential
        }
        MinimumRatioCycleMcfStage::MapGradientLength => {
            FlowMinimumRatioCycleMcfStageV1::MapGradientLength
        }
        MinimumRatioCycleMcfStage::BuildSpanningForest => {
            FlowMinimumRatioCycleMcfStageV1::BuildSpanningForest
        }
        MinimumRatioCycleMcfStage::InspectVector => FlowMinimumRatioCycleMcfStageV1::InspectVector,
        MinimumRatioCycleMcfStage::EvaluateCycle => FlowMinimumRatioCycleMcfStageV1::EvaluateCycle,
        MinimumRatioCycleMcfStage::UpdateBest => FlowMinimumRatioCycleMcfStageV1::UpdateBest,
        MinimumRatioCycleMcfStage::VerifyCycleSpace => {
            FlowMinimumRatioCycleMcfStageV1::VerifyCycleSpace
        }
        MinimumRatioCycleMcfStage::ApplySourceStep => {
            FlowMinimumRatioCycleMcfStageV1::ApplySourceStep
        }
        MinimumRatioCycleMcfStage::MeasurePotentialDecrease => {
            FlowMinimumRatioCycleMcfStageV1::MeasurePotentialDecrease
        }
        MinimumRatioCycleMcfStage::CheckDfsOracle => {
            FlowMinimumRatioCycleMcfStageV1::CheckDfsOracle
        }
        MinimumRatioCycleMcfStage::Complete => FlowMinimumRatioCycleMcfStageV1::Complete,
    }
}

fn minimum_ratio_cycle_mcf_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &MinimumRatioCycleMcfTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let changed_nodes = event
        .before
        .nodes
        .iter()
        .zip(&event.after.nodes)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let changed_edges = event
        .before
        .edges
        .iter()
        .zip(&event.after.edges)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let mut entity_refs = changed_edges
        .iter()
        .map(|index| FlowTraceEntityRefSceneV1::Edge {
            edge_id: graph.edges()[*index].id().as_str().to_owned(),
        })
        .collect::<Vec<_>>();
    if entity_refs.is_empty() {
        entity_refs.extend(
            changed_nodes
                .iter()
                .map(|index| FlowTraceEntityRefSceneV1::Node {
                    node_id: graph.nodes()[*index].id().as_str().to_owned(),
                }),
        );
    }
    if matches!(
        event.after.stage,
        MinimumRatioCycleMcfStage::InspectVector
            | MinimumRatioCycleMcfStage::EvaluateCycle
            | MinimumRatioCycleMcfStage::UpdateBest
    ) {
        // These boundaries own one signed auxiliary vector, not a set of
        // independently touched physical edges. The typed overlay renders the
        // exact nonzero coordinates, directions, and ratio transition without
        // turning a dense vector into whole-graph generic focus.
        entity_refs.clear();
    }
    let (label, value) = minimum_ratio_cycle_mcf_event_detail(&event.after);
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: (event_id > 5).then(|| "5".to_owned()),
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: if matches!(
            event.after.stage,
            MinimumRatioCycleMcfStage::InspectVector
                | MinimumRatioCycleMcfStage::EvaluateCycle
                | MinimumRatioCycleMcfStage::UpdateBest
        ) {
            if event.after.stage == MinimumRatioCycleMcfStage::InspectVector {
                TraceGranularityV1::Micro
            } else {
                TraceGranularityV1::Operation
            }
        } else {
            TraceGranularityV1::Phase
        },
        pseudocode_line: minimum_ratio_cycle_mcf_pseudocode_line(event.after.stage).to_owned(),
        patch_count: u32::try_from(changed_nodes.len() + changed_edges.len() + 1)
            .map_err(|_| JsError::new("minimum-ratio-cycle MCF patch count overflow"))?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn minimum_ratio_cycle_mcf_event_detail(
    snapshot: &MinimumRatioCycleMcfSnapshot,
) -> (&'static str, String) {
    match snapshot.stage {
        MinimumRatioCycleMcfStage::Ready => ("feasible flows", "0".to_owned()),
        MinimumRatioCycleMcfStage::EnumerateFeasibleSet => (
            "feasible flows",
            snapshot.metrics.feasible_flows.to_string(),
        ),
        MinimumRatioCycleMcfStage::ContractFixedFace => (
            "active edges",
            snapshot
                .edges
                .iter()
                .filter(|edge| !edge.fixed_on_face)
                .count()
                .to_string(),
        ),
        MinimumRatioCycleMcfStage::InitializeStrictInterior => {
            ("initial cost", snapshot.initial_cost.decimal())
        }
        MinimumRatioCycleMcfStage::EvaluatePotential => {
            ("potential", snapshot.current_potential.decimal())
        }
        MinimumRatioCycleMcfStage::MapGradientLength => ("alpha", snapshot.alpha.decimal()),
        MinimumRatioCycleMcfStage::BuildSpanningForest => (
            "cycle-space dimension",
            snapshot.metrics.fundamental_cycles.to_string(),
        ),
        MinimumRatioCycleMcfStage::InspectVector => (
            "enumerated vectors",
            snapshot.metrics.enumerated_vectors.to_string(),
        ),
        MinimumRatioCycleMcfStage::EvaluateCycle => (
            "candidate ratio",
            snapshot
                .candidate_ratio
                .map_or_else(|| "0".to_owned(), flow::MinimumRatioCycleMcfScalar::decimal),
        ),
        MinimumRatioCycleMcfStage::UpdateBest => (
            "best ratio",
            snapshot
                .best_ratio
                .map_or_else(|| "0".to_owned(), flow::MinimumRatioCycleMcfScalar::decimal),
        ),
        MinimumRatioCycleMcfStage::VerifyCycleSpace => (
            "maximum balance",
            snapshot.maximum_absolute_balance.to_string(),
        ),
        MinimumRatioCycleMcfStage::ApplySourceStep => ("eta", snapshot.eta.decimal()),
        MinimumRatioCycleMcfStage::MeasurePotentialDecrease => {
            ("potential decrease", snapshot.potential_decrease.decimal())
        }
        MinimumRatioCycleMcfStage::CheckDfsOracle => (
            "DFS expansions",
            snapshot.metrics.dfs_expansions.to_string(),
        ),
        MinimumRatioCycleMcfStage::Complete => {
            ("source steps", snapshot.metrics.source_steps.to_string())
        }
    }
}

const fn minimum_ratio_cycle_mcf_pseudocode_line(stage: MinimumRatioCycleMcfStage) -> &'static str {
    match stage {
        MinimumRatioCycleMcfStage::Ready => "validate the bounded MCF progress instance",
        MinimumRatioCycleMcfStage::EnumerateFeasibleSet => {
            "enumerate the exact feasible face and compute F* for bounded auditing"
        }
        MinimumRatioCycleMcfStage::ContractFixedFace => {
            "contract coordinates fixed throughout the feasible affine face"
        }
        MinimumRatioCycleMcfStage::InitializeStrictInterior => {
            "average all feasible integer flows to obtain a strict relative-interior point"
        }
        MinimumRatioCycleMcfStage::EvaluatePotential => {
            "evaluate 20m log(c^T f-F*) plus the alpha-power barriers"
        }
        MinimumRatioCycleMcfStage::MapGradientLength => {
            "evaluate the source gradient g(f) and lengths l(f)"
        }
        MinimumRatioCycleMcfStage::BuildSpanningForest => {
            "build a canonical active-edge forest and cycle-space dimension"
        }
        MinimumRatioCycleMcfStage::InspectVector => {
            "inspect the current signed edge vector at a geometric enumeration checkpoint"
        }
        MinimumRatioCycleMcfStage::EvaluateCycle => {
            "evaluate g^T delta / ||diag(l) delta||_1 for one simple circulation"
        }
        MinimumRatioCycleMcfStage::UpdateBest => "replace the exact ratio-cycle incumbent",
        MinimumRatioCycleMcfStage::VerifyCycleSpace => {
            "verify B^T delta = 0 and connected degree-two support"
        }
        MinimumRatioCycleMcfStage::ApplySourceStep => {
            "set eta g^T delta = -kappa^2/50 and update f"
        }
        MinimumRatioCycleMcfStage::MeasurePotentialDecrease => {
            "verify Phi(f+eta delta) <= Phi(f)-kappa^2/500"
        }
        MinimumRatioCycleMcfStage::CheckDfsOracle => {
            "compare the selected direction with an independent DFS cycle oracle"
        }
        MinimumRatioCycleMcfStage::Complete => {
            "publish one checked progress primitive without a terminal MCF claim"
        }
    }
}

fn randomized_almost_linear_mcf_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &RandomizedAlmostLinearMcfTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("randomized almost-linear MCF event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new(
                "randomized almost-linear MCF trace discontinuity",
            ));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("randomized almost-linear MCF event identity overflow"))?;
        let mut scene = base.clone();
        scene
            .apply_randomized_almost_linear_mcf_boundary(
                graph,
                &randomized_almost_linear_mcf_flows(graph, &event.after)?,
                randomized_almost_linear_mcf_overlay(graph, &event.after)?,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(randomized_almost_linear_mcf_trace_event_scene(
            graph, event, event_id,
        )?);
        scene.set_randomized_almost_linear_mcf_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene
                .set_randomized_almost_linear_mcf_outcome(graph, &run.result.certificate)
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot
        || run.final_snapshot != run.result.final_snapshot
        || run.events.is_empty()
    {
        return Err(JsError::new(
            "randomized almost-linear MCF final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn randomized_almost_linear_mcf_fast_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    result: &RandomizedAlmostLinearMcfResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let ready = ready_flow_scene(scenario)?;
    let mut scene = ready.clone();
    scene
        .apply_randomized_almost_linear_mcf_boundary(
            graph,
            &result.flows,
            randomized_almost_linear_mcf_overlay(graph, &result.final_snapshot)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_randomized_almost_linear_mcf_metrics(result.final_snapshot.metrics);
    scene
        .set_randomized_almost_linear_mcf_outcome(graph, &result.certificate)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(vec![ready, scene])
}

fn randomized_almost_linear_mcf_flows(
    graph: &flow::FlowNetwork,
    snapshot: &RandomizedAlmostLinearMcfSnapshot,
) -> Result<Vec<u64>, JsError> {
    if snapshot.exact_recovery {
        snapshot
            .edges
            .iter()
            .map(|edge| {
                edge.final_flow.ok_or_else(|| {
                    JsError::new("randomized almost-linear MCF rounded flow is missing")
                })
            })
            .collect()
    } else {
        Ok(graph.edges().iter().map(flow::FlowEdge::lower).collect())
    }
}

#[allow(clippy::too_many_lines)]
fn randomized_almost_linear_mcf_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &RandomizedAlmostLinearMcfSnapshot,
) -> Result<FlowRandomizedAlmostLinearMcfOverlayV1, JsError> {
    if snapshot.nodes.len() != graph.nodes().len() || snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new(
            "randomized almost-linear MCF snapshot shape mismatch",
        ));
    }
    let nodes = graph
        .nodes()
        .iter()
        .zip(&snapshot.nodes)
        .map(|(node, state)| {
            if graph
                .nodes()
                .get(state.node.as_usize())
                .map(flow::FlowNode::id)
                != Some(node.id())
            {
                return Err(JsError::new(
                    "randomized almost-linear MCF node identity mismatch",
                ));
            }
            Ok(FlowRandomizedAlmostLinearMcfNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                required_divergence: state.required_divergence.to_string(),
                component: state.component.to_string(),
                parent_node_id: state
                    .parent
                    .map(|parent| graph.nodes()[parent.as_usize()].id().as_str().to_owned()),
                depth: state.depth.to_string(),
                on_selected_cycle: state.on_selected_cycle,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let edges = graph
        .edges()
        .iter()
        .zip(&snapshot.edges)
        .map(|(edge, state)| {
            if state.edge != *edge.id() {
                return Err(JsError::new(
                    "randomized almost-linear MCF edge identity mismatch",
                ));
            }
            Ok(FlowRandomizedAlmostLinearMcfEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                fixed_on_face: state.fixed_on_face,
                initial_flow: state.initial_flow.decimal(),
                current_flow: state.current_flow.decimal(),
                stale_flow: state.stale_flow.decimal(),
                final_point_flow: state.final_point_flow.as_ref().map(big_rational_scene),
                final_flow: state.final_flow.map(|value| value.to_string()),
                isolation_draw: state.isolation_draw.to_string(),
                isolated_cost: state.isolated_cost.to_string(),
                isolated_optimum_flow: state.isolated_optimum_flow.map(|flow| flow.to_string()),
                tree_edge: state.tree_edge,
                candidate_sign: state.candidate_sign.to_string(),
                selected_sign: state.selected_sign.to_string(),
                gradient: state.gradient.decimal(),
                length: state.length.decimal(),
                detected: state.detected,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowRandomizedAlmostLinearMcfOverlayV1 {
        stage: randomized_almost_linear_mcf_scene_stage(snapshot.stage),
        assignment_cursor: snapshot
            .assignment_cursor
            .as_ref()
            .map(|cursor| cursor.as_str().to_owned()),
        assignment_serial: (snapshot.stage
            == RandomizedAlmostLinearMcfStage::InspectFeasibleAssignment)
            .then(|| snapshot.metrics.enumerated_assignments.to_string()),
        oracle_vector_serial: (snapshot.stage
            == RandomizedAlmostLinearMcfStage::InspectOracleVector)
            .then(|| snapshot.metrics.oracle_vector_evaluations.to_string()),
        seed: snapshot.seed.to_string(),
        alpha: snapshot.alpha.decimal(),
        epsilon: snapshot.epsilon.decimal(),
        kappa: snapshot.kappa.decimal(),
        eta: snapshot.eta.decimal(),
        initial_cost: snapshot.initial_cost.decimal(),
        current_cost: snapshot.current_cost.decimal(),
        optimum_cost: snapshot.optimum_cost.to_string(),
        isolated_optimum_cost: snapshot.isolated_optimum_cost.to_string(),
        potential: snapshot.potential.decimal(),
        minimum_ratio: snapshot
            .minimum_ratio
            .map(flow::RandomizedAlmostLinearMcfScalar::decimal),
        isolation_attempt: snapshot.isolation_attempt.to_string(),
        isolation_scale: snapshot.isolation_scale.to_string(),
        failure_numerator: snapshot.failure_probability_bound.numerator.to_string(),
        failure_denominator: snapshot.failure_probability_bound.denominator.to_string(),
        forest_pool_size: snapshot.forest_pool_size.to_string(),
        sampled_forest_index: snapshot.sampled_forest_index.map(|value| value.to_string()),
        final_point_gap: snapshot.final_point_gap.as_ref().map(big_rational_scene),
        final_point_threshold: big_rational_scene(&snapshot.final_point_threshold),
        final_point_mix: snapshot.final_point_mix.as_ref().map(big_rational_scene),
        exact_recovery: snapshot.exact_recovery,
        feasible_flows: snapshot.metrics.feasible_flows.to_string(),
        detected_coordinates: snapshot.metrics.detected_coordinates.to_string(),
        rebuilds: snapshot.metrics.rebuilds.to_string(),
        nodes,
        edges,
    })
}

const fn randomized_almost_linear_mcf_scene_stage(
    stage: RandomizedAlmostLinearMcfStage,
) -> FlowRandomizedAlmostLinearMcfStageV1 {
    use FlowRandomizedAlmostLinearMcfStageV1 as Scene;
    use RandomizedAlmostLinearMcfStage as Core;
    match stage {
        Core::Ready => Scene::Ready,
        Core::InspectFeasibleAssignment => Scene::InspectFeasibleAssignment,
        Core::EnumerateFeasibleSet => Scene::EnumerateFeasibleSet,
        Core::SampleIsolationCosts => Scene::SampleIsolationCosts,
        Core::SelectIsolatedOptimum => Scene::SelectIsolatedOptimum,
        Core::InitializeRelativeInterior => Scene::InitializeRelativeInterior,
        Core::InspectOracleVector => Scene::InspectOracleVector,
        Core::BuildForestPool => Scene::BuildForestPool,
        Core::SampleTreeChain => Scene::SampleTreeChain,
        Core::RefreshGradientLength => Scene::RefreshGradientLength,
        Core::QueryMinimumRatioCycle => Scene::QueryMinimumRatioCycle,
        Core::PotentialReductionStep => Scene::PotentialReductionStep,
        Core::DetectChangedCoordinates => Scene::DetectChangedCoordinates,
        Core::RebuildTreeChain => Scene::RebuildTreeChain,
        Core::ConstructFinalPoint => Scene::ConstructFinalPoint,
        Core::RoundNearestInteger => Scene::RoundNearestInteger,
        Core::CheckCertificate => Scene::CheckCertificate,
        Core::Optimal => Scene::Optimal,
    }
}

const fn randomized_almost_linear_mcf_catalog_id(
    stage: RandomizedAlmostLinearMcfStage,
) -> &'static str {
    use RandomizedAlmostLinearMcfStage as Stage;
    match stage {
        Stage::Ready => "randomized-almost-linear-mcf-oracle-demonstrator.ready",
        Stage::InspectFeasibleAssignment => {
            "randomized-almost-linear-mcf-oracle-demonstrator.inspect-feasible-assignment"
        }
        Stage::EnumerateFeasibleSet => {
            "randomized-almost-linear-mcf-oracle-demonstrator.enumerate-feasible-set"
        }
        Stage::SampleIsolationCosts => {
            "randomized-almost-linear-mcf-oracle-demonstrator.sample-isolation-costs"
        }
        Stage::SelectIsolatedOptimum => {
            "randomized-almost-linear-mcf-oracle-demonstrator.select-isolated-optimum"
        }
        Stage::InitializeRelativeInterior => {
            "randomized-almost-linear-mcf-oracle-demonstrator.initialize-relative-interior"
        }
        Stage::InspectOracleVector => {
            "randomized-almost-linear-mcf-oracle-demonstrator.inspect-oracle-vector"
        }
        Stage::BuildForestPool => {
            "randomized-almost-linear-mcf-oracle-demonstrator.build-forest-pool"
        }
        Stage::SampleTreeChain => {
            "randomized-almost-linear-mcf-oracle-demonstrator.sample-tree-chain"
        }
        Stage::RefreshGradientLength => {
            "randomized-almost-linear-mcf-oracle-demonstrator.refresh-gradient-length"
        }
        Stage::QueryMinimumRatioCycle => {
            "randomized-almost-linear-mcf-oracle-demonstrator.query-minimum-ratio-cycle"
        }
        Stage::PotentialReductionStep => {
            "randomized-almost-linear-mcf-oracle-demonstrator.potential-reduction-step"
        }
        Stage::DetectChangedCoordinates => {
            "randomized-almost-linear-mcf-oracle-demonstrator.detect-changed-coordinates"
        }
        Stage::RebuildTreeChain => {
            "randomized-almost-linear-mcf-oracle-demonstrator.rebuild-tree-chain"
        }
        Stage::ConstructFinalPoint => {
            "randomized-almost-linear-mcf-oracle-demonstrator.construct-final-point"
        }
        Stage::RoundNearestInteger => {
            "randomized-almost-linear-mcf-oracle-demonstrator.round-nearest-integer"
        }
        Stage::CheckCertificate => {
            "randomized-almost-linear-mcf-oracle-demonstrator.check-certificate"
        }
        Stage::Optimal => "randomized-almost-linear-mcf-oracle-demonstrator.optimal",
    }
}

const fn randomized_almost_linear_mcf_granularity(
    stage: RandomizedAlmostLinearMcfStage,
) -> TraceGranularityV1 {
    use RandomizedAlmostLinearMcfStage as Stage;
    match stage {
        Stage::Ready
        | Stage::EnumerateFeasibleSet
        | Stage::InitializeRelativeInterior
        | Stage::BuildForestPool
        | Stage::Optimal => TraceGranularityV1::Phase,
        Stage::InspectFeasibleAssignment
        | Stage::InspectOracleVector
        | Stage::QueryMinimumRatioCycle => TraceGranularityV1::Micro,
        Stage::SampleIsolationCosts
        | Stage::SelectIsolatedOptimum
        | Stage::SampleTreeChain
        | Stage::RefreshGradientLength
        | Stage::PotentialReductionStep
        | Stage::DetectChangedCoordinates
        | Stage::RebuildTreeChain
        | Stage::ConstructFinalPoint
        | Stage::RoundNearestInteger
        | Stage::CheckCertificate => TraceGranularityV1::Operation,
    }
}

fn randomized_almost_linear_mcf_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &RandomizedAlmostLinearMcfTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let changed_nodes = event
        .before
        .nodes
        .iter()
        .zip(&event.after.nodes)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let changed_edges = event
        .before
        .edges
        .iter()
        .zip(&event.after.edges)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let mut entity_refs = changed_edges
        .iter()
        .map(|&index| FlowTraceEntityRefSceneV1::Edge {
            edge_id: graph.edges()[index].id().as_str().to_owned(),
        })
        .collect::<Vec<_>>();
    if event.after.stage == RandomizedAlmostLinearMcfStage::InspectFeasibleAssignment {
        let cursor = event.after.assignment_cursor.as_ref().ok_or_else(|| {
            JsError::new("randomized almost-linear MCF assignment cursor is missing")
        })?;
        entity_refs.push(FlowTraceEntityRefSceneV1::Edge {
            edge_id: cursor.as_str().to_owned(),
        });
        entity_refs.sort();
        entity_refs.dedup();
    }
    if event.after.stage == RandomizedAlmostLinearMcfStage::InspectOracleVector {
        entity_refs = event
            .after
            .edges
            .iter()
            .filter(|state| state.candidate_sign != 0)
            .map(|state| FlowTraceEntityRefSceneV1::Edge {
                edge_id: state.edge.as_str().to_owned(),
            })
            .collect();
        if entity_refs.is_empty() {
            entity_refs = event
                .after
                .edges
                .iter()
                .filter(|state| state.selected_sign != 0)
                .map(|state| FlowTraceEntityRefSceneV1::Edge {
                    edge_id: state.edge.as_str().to_owned(),
                })
                .collect();
        }
    }
    if entity_refs.is_empty() {
        entity_refs.extend(
            changed_nodes
                .iter()
                .map(|&index| FlowTraceEntityRefSceneV1::Node {
                    node_id: graph.nodes()[index].id().as_str().to_owned(),
                }),
        );
    }
    if matches!(
        event.after.stage,
        RandomizedAlmostLinearMcfStage::InspectFeasibleAssignment
            | RandomizedAlmostLinearMcfStage::EnumerateFeasibleSet
            | RandomizedAlmostLinearMcfStage::SampleIsolationCosts
            | RandomizedAlmostLinearMcfStage::SelectIsolatedOptimum
            | RandomizedAlmostLinearMcfStage::InspectOracleVector
            | RandomizedAlmostLinearMcfStage::RefreshGradientLength
            | RandomizedAlmostLinearMcfStage::ConstructFinalPoint
            | RandomizedAlmostLinearMcfStage::RoundNearestInteger
    ) {
        // These boundaries publish a whole auxiliary vector. The dedicated
        // main-canvas layer renders every coordinate and its direction, so a
        // second generic focus would incorrectly read as "all edges changed".
        entity_refs.clear();
    }
    let (label, value) = randomized_almost_linear_mcf_event_detail(&event.after);
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: (event_id > 5).then(|| "5".to_owned()),
        catalog_id: randomized_almost_linear_mcf_catalog_id(event.after.stage).to_owned(),
        minimum_granularity: randomized_almost_linear_mcf_granularity(event.after.stage),
        pseudocode_line: randomized_almost_linear_mcf_pseudocode_line(event.after.stage).to_owned(),
        patch_count: u32::try_from(changed_nodes.len() + changed_edges.len() + 1)
            .map_err(|_| JsError::new("randomized almost-linear MCF patch count overflow"))?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn randomized_almost_linear_mcf_event_detail(
    snapshot: &RandomizedAlmostLinearMcfSnapshot,
) -> (&'static str, String) {
    use RandomizedAlmostLinearMcfStage as Stage;
    match snapshot.stage {
        Stage::Ready | Stage::EnumerateFeasibleSet => (
            "feasible flows",
            snapshot.metrics.feasible_flows.to_string(),
        ),
        Stage::InspectFeasibleAssignment => (
            "assignment",
            snapshot.metrics.enumerated_assignments.to_string(),
        ),
        Stage::SampleIsolationCosts | Stage::SelectIsolatedOptimum => {
            ("isolation attempt", snapshot.isolation_attempt.to_string())
        }
        Stage::InitializeRelativeInterior => ("initial cost", snapshot.initial_cost.decimal()),
        Stage::InspectOracleVector => (
            "oracle vectors",
            snapshot.metrics.oracle_vector_evaluations.to_string(),
        ),
        Stage::BuildForestPool => ("forest pool", snapshot.forest_pool_size.to_string()),
        Stage::SampleTreeChain => (
            "sampled forest",
            snapshot.sampled_forest_index.unwrap_or(0).to_string(),
        ),
        Stage::RefreshGradientLength => ("alpha", snapshot.alpha.decimal()),
        Stage::QueryMinimumRatioCycle => (
            "minimum ratio",
            snapshot.minimum_ratio.map_or_else(
                || "0".to_owned(),
                flow::RandomizedAlmostLinearMcfScalar::decimal,
            ),
        ),
        Stage::PotentialReductionStep => ("eta", snapshot.eta.decimal()),
        Stage::DetectChangedCoordinates => (
            "detected coordinates",
            snapshot.metrics.detected_coordinates.to_string(),
        ),
        Stage::RebuildTreeChain => ("rebuilds", snapshot.metrics.rebuilds.to_string()),
        Stage::ConstructFinalPoint => (
            "final-point gap numerator",
            snapshot
                .final_point_gap
                .as_ref()
                .map_or_else(|| "0".to_owned(), |gap| gap.numer().to_string()),
        ),
        Stage::RoundNearestInteger | Stage::CheckCertificate | Stage::Optimal => {
            ("optimum cost", snapshot.optimum_cost.to_string())
        }
    }
}

const fn randomized_almost_linear_mcf_pseudocode_line(
    stage: RandomizedAlmostLinearMcfStage,
) -> &'static str {
    use RandomizedAlmostLinearMcfStage as Stage;
    match stage {
        Stage::Ready => "validate the bounded integral MCF instance",
        Stage::InspectFeasibleAssignment => {
            "check one geometrically spaced bounded-flow assignment"
        }
        Stage::EnumerateFeasibleSet => "enumerate the feasible face for bounded exact auditing",
        Stage::SampleIsolationCosts => "sample independent z_e and form D c_e + z_e",
        Stage::SelectIsolatedOptimum => "select the unique isolated optimum and failure bound",
        Stage::InitializeRelativeInterior => {
            "average the feasible face to obtain a relative-interior point"
        }
        Stage::InspectOracleVector => {
            "inspect the exact ratio oracle's signed edge vector at a geometric checkpoint"
        }
        Stage::BuildForestPool => "build the bounded spanning-forest population",
        Stage::SampleTreeChain => "sample a tree-chain representative from the seeded distribution",
        Stage::RefreshGradientLength => {
            "refresh source gradient g and length l on stale coordinates"
        }
        Stage::QueryMinimumRatioCycle => {
            "query the exact bounded replacement for the sampled min-ratio oracle"
        }
        Stage::PotentialReductionStep => "apply the source-scaled alpha-power potential step",
        Stage::DetectChangedCoordinates => {
            "Detect coordinates with l_e times accumulated change at least epsilon"
        }
        Stage::RebuildTreeChain => "rebuild the bounded tree-chain view after lazy refresh",
        Stage::ConstructFinalPoint => {
            "materialize an exact feasible point within 1/(12m^3U^3) of the isolated optimum"
        }
        Stage::RoundNearestInteger => {
            "round every published final-point coordinate to nearest integer"
        }
        Stage::CheckCertificate => "independently check balance, bounds, and residual optimality",
        Stage::Optimal => "publish the certified original-cost optimum",
    }
}

fn flow_framework_mcf_trace_event_count(run: &FlowFrameworkMcfTraceResult) -> Result<u64, JsError> {
    let dynamic_events = run
        .iterations
        .iter()
        .try_fold(0_usize, |count, iteration| {
            count
                .checked_add(iteration.dynamic_trace.events.len())
                .and_then(|count| count.checked_add(usize::from(iteration.source.reinitialized)))
                .ok_or_else(|| JsError::new("Flow Framework MCF event count overflow"))
        })?;
    u64::try_from(
        dynamic_events
            .checked_add(4)
            .ok_or_else(|| JsError::new("Flow Framework MCF terminal event count overflow"))?,
    )
    .map_err(|_| JsError::new("Flow Framework MCF event count overflow"))
}

struct FlowFrameworkMcfTraceFrameContext<'a> {
    graph: &'a flow::FlowNetwork,
    base: &'a FlowCurrentSceneV9,
    run: &'a FlowFrameworkMcfTraceResult,
    lower_flows: Vec<u64>,
    zero_cycle: Vec<BigRational>,
    event_count: u64,
}

impl FlowFrameworkMcfTraceFrameContext<'_> {
    fn initial_scene(&self) -> Result<FlowCurrentSceneV9, JsError> {
        let first_dynamic = self
            .run
            .iterations
            .first()
            .map(|iteration| &iteration.dynamic_trace.base_snapshot);
        let mut scene = self.base.clone();
        scene
            .apply_flow_framework_mcf_boundary(
                self.graph,
                &self.lower_flows,
                flow_framework_mcf_overlay(
                    self.graph,
                    FlowFrameworkMcfStageV1::InitializeSourcePoint,
                    None,
                    &self.run.base_snapshot,
                    &self.run.base_snapshot.original_flow,
                    &self.zero_cycle,
                    first_dynamic,
                    None,
                    None,
                    None,
                )?,
                1,
                self.event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.trace_event = Some(flow_framework_trace_event_scene(
            self.graph,
            FlowFrameworkMcfStageV1::InitializeSourcePoint,
            &self.zero_cycle,
            1,
        )?);
        set_flow_framework_metrics(&mut scene, 0, first_dynamic, 0)?;
        Ok(scene)
    }

    fn reinitialize_scene(
        &self,
        iteration: &flow::FlowFrameworkMcfTraceIteration,
        previous_original: &[BigRational],
        event_id: u64,
        primary_work_offset: u128,
    ) -> Result<FlowCurrentSceneV9, JsError> {
        let dynamic_base = &iteration.dynamic_trace.base_snapshot;
        let mut scene = self.base.clone();
        scene
            .apply_flow_framework_mcf_boundary(
                self.graph,
                &self.lower_flows,
                flow_framework_mcf_overlay(
                    self.graph,
                    FlowFrameworkMcfStageV1::PeriodicReinitialize,
                    Some(&iteration.source),
                    &self.run.result.final_snapshot,
                    previous_original,
                    &self.zero_cycle,
                    Some(dynamic_base),
                    None,
                    None,
                    None,
                )?,
                event_id,
                self.event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.trace_event = Some(flow_framework_trace_event_scene(
            self.graph,
            FlowFrameworkMcfStageV1::PeriodicReinitialize,
            &self.zero_cycle,
            event_id,
        )?);
        set_flow_framework_metrics(
            &mut scene,
            iteration.source.iteration,
            Some(dynamic_base),
            primary_work_offset,
        )?;
        Ok(scene)
    }

    fn dynamic_scene(
        &self,
        iteration: &flow::FlowFrameworkMcfTraceIteration,
        event: &flow::DynamicMinRatioCycleTraceEvent,
        previous_original: &[BigRational],
        event_id: u64,
        primary_work_offset: u128,
    ) -> Result<FlowCurrentSceneV9, JsError> {
        let stage = flow_framework_stage(&event.kind);
        let movement_applied = event
            .after
            .flow
            .iter()
            .any(|amount| amount != &BigRational::from_integer(0.into()));
        let cycle_visible = matches!(
            &event.kind,
            DynamicMinRatioCycleEventKind::CycleQueried { accepted: true, .. }
                | DynamicMinRatioCycleEventKind::FlowApplied { .. }
                | DynamicMinRatioCycleEventKind::Completed
        );
        let original_flow = if movement_applied {
            iteration.source.original_flow.as_slice()
        } else {
            previous_original
        };
        let cycle = if cycle_visible {
            iteration.source.original_cycle.as_slice()
        } else {
            self.zero_cycle.as_slice()
        };
        let mut scene = self.base.clone();
        scene
            .apply_flow_framework_mcf_boundary(
                self.graph,
                &self.lower_flows,
                flow_framework_mcf_overlay(
                    self.graph,
                    stage,
                    Some(&iteration.source),
                    &self.run.result.final_snapshot,
                    original_flow,
                    cycle,
                    Some(&event.after),
                    Some(flow_framework_dynamic_operation(&event.kind)),
                    None,
                    None,
                )?,
                event_id,
                self.event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.trace_event = Some(flow_framework_trace_event_scene(
            self.graph, stage, cycle, event_id,
        )?);
        set_flow_framework_metrics(
            &mut scene,
            iteration.source.iteration,
            Some(&event.after),
            primary_work_offset,
        )?;
        Ok(scene)
    }

    fn terminal_scene(
        &self,
        stage: FlowFrameworkMcfStageV1,
        event_id: u64,
        primary_work: u128,
    ) -> Result<FlowCurrentSceneV9, JsError> {
        let optimal = stage == FlowFrameworkMcfStageV1::Optimal;
        let exact_original = if optimal {
            self.run
                .result
                .solution
                .flows
                .iter()
                .map(|&amount| BigRational::from_integer(amount.into()))
                .collect::<Vec<_>>()
        } else {
            self.run.result.final_snapshot.original_flow.clone()
        };
        let generic = if optimal {
            self.run.result.solution.flows.clone()
        } else {
            self.lower_flows.clone()
        };
        let mut scene = self.base.clone();
        scene
            .apply_flow_framework_mcf_boundary(
                self.graph,
                &generic,
                flow_framework_mcf_overlay(
                    self.graph,
                    stage,
                    self.run.result.last_iteration.as_ref(),
                    &self.run.result.final_snapshot,
                    &exact_original,
                    &self.zero_cycle,
                    None,
                    None,
                    matches!(
                        stage,
                        FlowFrameworkMcfStageV1::CheckCertificate
                            | FlowFrameworkMcfStageV1::Optimal
                    )
                    .then_some(&self.run.result.solution.augmented_rounding),
                    optimal.then_some("source-additive-half-gap"),
                )?,
                event_id,
                self.event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.trace_event = Some(flow_framework_trace_event_scene(
            self.graph,
            stage,
            &self.zero_cycle,
            event_id,
        )?);
        set_flow_framework_metrics(&mut scene, self.run.result.iterations, None, primary_work)?;
        if optimal {
            scene
                .set_flow_framework_mcf_outcome(self.graph, &self.run.result.solution.certificate)
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        Ok(scene)
    }
}

fn flow_framework_mcf_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &FlowFrameworkMcfTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = flow_framework_mcf_trace_event_count(run)?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let context = FlowFrameworkMcfTraceFrameContext {
        graph,
        base: &base,
        run,
        lower_flows: lower_bound_flows(graph),
        zero_cycle: vec![BigRational::from_integer(0.into()); graph.edges().len()],
        event_count,
    };
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    if !timeline.try_push(context.initial_scene()?)? {
        return Ok(flow_trace_resource_limit_frames(scenario, &base));
    }
    let mut event_id = 1_u64;
    let mut previous_original = run.base_snapshot.original_flow.clone();
    let mut primary_work_offset = 0_u128;

    for iteration in &run.iterations {
        if iteration.source.reinitialized {
            event_id = event_id
                .checked_add(1)
                .ok_or_else(|| JsError::new("Flow Framework MCF event identity overflow"))?;
            if !timeline.try_push(context.reinitialize_scene(
                iteration,
                &previous_original,
                event_id,
                primary_work_offset,
            )?)? {
                return Ok(flow_trace_resource_limit_frames(scenario, &base));
            }
        }

        for event in &iteration.dynamic_trace.events {
            event_id = event_id
                .checked_add(1)
                .ok_or_else(|| JsError::new("Flow Framework MCF event identity overflow"))?;
            if !timeline.try_push(context.dynamic_scene(
                iteration,
                event,
                &previous_original,
                event_id,
                primary_work_offset,
            )?)? {
                return Ok(flow_trace_resource_limit_frames(scenario, &base));
            }
        }
        let final_dynamic = iteration
            .dynamic_trace
            .events
            .last()
            .map_or(&iteration.dynamic_trace.base_snapshot, |event| &event.after);
        primary_work_offset = primary_work_offset
            .checked_add(flow_framework_dynamic_primary_work(final_dynamic)?)
            .ok_or_else(|| JsError::new("Flow Framework MCF primary work overflow"))?;
        previous_original.clone_from(&iteration.source.original_flow);
    }

    for stage in [
        FlowFrameworkMcfStageV1::RoundFractionalFlow,
        FlowFrameworkMcfStageV1::CheckCertificate,
        FlowFrameworkMcfStageV1::Optimal,
    ] {
        event_id = event_id
            .checked_add(1)
            .ok_or_else(|| JsError::new("Flow Framework MCF event identity overflow"))?;
        if !timeline.try_push(context.terminal_scene(stage, event_id, primary_work_offset)?)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    if event_id != event_count {
        return Err(JsError::new("Flow Framework MCF event count mismatch"));
    }
    Ok(timeline.finish())
}

fn flow_framework_mcf_fast_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    result: &FlowFrameworkMcfResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let ready = ready_flow_scene(scenario)?;
    let exact_original = result
        .solution
        .flows
        .iter()
        .map(|&amount| BigRational::from_integer(amount.into()))
        .collect::<Vec<_>>();
    let zero_cycle = vec![BigRational::from_integer(0.into()); graph.edges().len()];
    let mut scene = ready.clone();
    scene
        .apply_flow_framework_mcf_boundary(
            graph,
            &result.solution.flows,
            flow_framework_mcf_overlay(
                graph,
                FlowFrameworkMcfStageV1::Optimal,
                result.last_iteration.as_ref(),
                &result.final_snapshot,
                &exact_original,
                &zero_cycle,
                None,
                None,
                Some(&result.solution.augmented_rounding),
                Some("source-additive-half-gap"),
            )?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    set_flow_framework_metrics(
        &mut scene,
        result.iterations,
        None,
        result.dynamic_edge_inspections,
    )?;
    scene
        .set_flow_framework_mcf_outcome(graph, &result.solution.certificate)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(vec![ready, scene])
}

#[allow(clippy::too_many_arguments)]
fn flow_framework_mcf_overlay(
    graph: &flow::FlowNetwork,
    stage: FlowFrameworkMcfStageV1,
    iteration: Option<&FlowFrameworkMcfIteration>,
    snapshot: &flow::FlowFrameworkMcfSnapshot,
    original_flow: &[BigRational],
    cycle: &[BigRational],
    dynamic: Option<&flow::DynamicMinRatioCycleSnapshot>,
    dynamic_operation: Option<FlowFrameworkMcfDynamicOperationV1>,
    rounded: Option<&flow::CostedFlowRoundingResult>,
    termination: Option<&str>,
) -> Result<FlowFrameworkMcfOverlayV1, JsError> {
    let progress_visible = iteration.is_some()
        && matches!(
            stage,
            FlowFrameworkMcfStageV1::QueryMinimumRatioCycle
                | FlowFrameworkMcfStageV1::SourceProgress
                | FlowFrameworkMcfStageV1::RoundFractionalFlow
                | FlowFrameworkMcfStageV1::CheckCertificate
                | FlowFrameworkMcfStageV1::Optimal
        );
    let zero = BigRational::from_integer(0.into());
    let (
        potential_before,
        potential_after,
        gap_before,
        gap_after,
        exact_gap_before,
        exact_gap_after,
    ) = iteration.map_or_else(
        || {
            (
                snapshot.potential.decimal(),
                snapshot.potential.decimal(),
                snapshot.gap.decimal(),
                snapshot.gap.decimal(),
                &snapshot.exact_gap,
                &snapshot.exact_gap,
            )
        },
        |iteration| {
            (
                iteration.potential_before.decimal(),
                iteration.potential_after.decimal(),
                iteration.gap_before.decimal(),
                iteration.gap_after.decimal(),
                &iteration.exact_gap_before,
                &iteration.exact_gap_after,
            )
        },
    );
    let levels = flow_framework_mcf_levels(dynamic);
    let edges = flow_framework_mcf_edges(graph, original_flow, cycle)?;
    let accepted_ratio = iteration.filter(|_| progress_visible).map_or_else(
        || big_rational_scene(&zero),
        |value| big_rational_scene(&value.accepted_ratio),
    );
    let target_progress = iteration.filter(|_| progress_visible).map_or_else(
        || big_rational_scene(&zero),
        |value| big_rational_scene(&value.target_progress),
    );
    let final_point = flow_framework_mcf_final_point(stage, snapshot, rounded)?;
    let dynamic_operation_serial = match dynamic_operation {
        Some(_) => Some(
            dynamic
                .ok_or_else(|| {
                    JsError::new("Flow Framework MCF dynamic operation has no source snapshot")
                })?
                .metrics
                .state_transitions
                .to_string(),
        ),
        None => None,
    };
    Ok(FlowFrameworkMcfOverlayV1 {
        stage,
        dynamic_operation,
        dynamic_operation_serial,
        iteration: iteration
            .map_or(snapshot.iterations, |value| value.iteration)
            .to_string(),
        reinitialized: iteration.is_some_and(|value| value.reinitialized),
        potential_before,
        potential_after,
        gap_before,
        gap_after,
        exact_gap_before: big_rational_scene(exact_gap_before),
        exact_gap_after: big_rational_scene(exact_gap_after),
        stopping_gap: big_rational_scene(&flow::flow_framework_mcf_stopping_gap()),
        accepted_ratio,
        target_progress,
        termination: termination.map(str::to_owned),
        optimum_cost: final_point.optimum_cost,
        final_point_nodes: final_point.nodes,
        final_point_edges: final_point.edges,
        levels,
        edges,
    })
}

fn flow_framework_mcf_levels(
    dynamic: Option<&flow::DynamicMinRatioCycleSnapshot>,
) -> Vec<FlowFrameworkMcfLevelStateV1> {
    (0..2)
        .map(|level| FlowFrameworkMcfLevelStateV1 {
            level: level.to_string(),
            active_branch: dynamic
                .and_then(|snapshot| snapshot.runtime.levels.get(level))
                .map_or(0, |level| level.input.active_branch)
                .to_string(),
            passes: dynamic
                .and_then(|snapshot| snapshot.passes.get(level))
                .copied()
                .unwrap_or(0)
                .to_string(),
        })
        .collect()
}

fn flow_framework_mcf_edges(
    graph: &flow::FlowNetwork,
    original_flow: &[BigRational],
    cycle: &[BigRational],
) -> Result<Vec<FlowFrameworkMcfEdgeStateV1>, JsError> {
    if original_flow.len() != graph.edges().len() || cycle.len() != graph.edges().len() {
        return Err(JsError::new(
            "Flow Framework MCF original-edge projection mismatch",
        ));
    }
    let zero = BigRational::from_integer(0.into());
    Ok(graph
        .edges()
        .iter()
        .zip(original_flow)
        .zip(cycle)
        .map(|((edge, flow), coefficient)| FlowFrameworkMcfEdgeStateV1 {
            edge_id: edge.id().as_str().to_owned(),
            flow: big_rational_scene(flow),
            cycle_coefficient: big_rational_scene(coefficient),
            selected: coefficient != &zero,
        })
        .collect())
}

struct FlowFrameworkMcfFinalPointProjection {
    optimum_cost: Option<String>,
    nodes: Option<Vec<FlowFrameworkMcfFinalPointNodeV1>>,
    edges: Option<Vec<FlowFrameworkMcfFinalPointEdgeV1>>,
}

fn flow_framework_mcf_final_point(
    stage: FlowFrameworkMcfStageV1,
    snapshot: &flow::FlowFrameworkMcfSnapshot,
    rounded: Option<&flow::CostedFlowRoundingResult>,
) -> Result<FlowFrameworkMcfFinalPointProjection, JsError> {
    let visible = matches!(
        stage,
        FlowFrameworkMcfStageV1::RoundFractionalFlow
            | FlowFrameworkMcfStageV1::CheckCertificate
            | FlowFrameworkMcfStageV1::Optimal
    );
    if !visible && rounded.is_some() {
        return Err(JsError::new(
            "Flow Framework MCF rounding appeared before the final point",
        ));
    }
    if rounded.is_some_and(|value| value.flows.len() != snapshot.augmented_edges.len()) {
        return Err(JsError::new(
            "Flow Framework MCF augmented rounding projection mismatch",
        ));
    }
    Ok(FlowFrameworkMcfFinalPointProjection {
        optimum_cost: visible.then(|| snapshot.optimum_cost.to_string()),
        nodes: visible.then(|| {
            snapshot
                .augmented_nodes
                .iter()
                .map(|node| FlowFrameworkMcfFinalPointNodeV1 {
                    node_id: node.node_id.clone(),
                    required_divergence: node.required_divergence.to_string(),
                })
                .collect()
        }),
        edges: visible.then(|| {
            snapshot
                .augmented_edges
                .iter()
                .enumerate()
                .map(|(index, edge)| FlowFrameworkMcfFinalPointEdgeV1 {
                    edge_id: edge.edge_id.clone(),
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    lower: edge.lower.to_string(),
                    capacity: edge.capacity.to_string(),
                    cost: edge.cost.to_string(),
                    flow: big_rational_scene(&edge.flow),
                    auxiliary: edge.auxiliary,
                    rounded_flow: rounded.map(|value| value.flows[index].to_string()),
                })
                .collect()
        }),
    })
}

fn flow_framework_stage(kind: &DynamicMinRatioCycleEventKind) -> FlowFrameworkMcfStageV1 {
    match kind {
        DynamicMinRatioCycleEventKind::DetectReturned { .. } => FlowFrameworkMcfStageV1::Detect,
        DynamicMinRatioCycleEventKind::CycleQueried { .. }
        | DynamicMinRatioCycleEventKind::QueryReturned { .. }
        | DynamicMinRatioCycleEventKind::LevelShifted { .. } => {
            FlowFrameworkMcfStageV1::QueryMinimumRatioCycle
        }
        DynamicMinRatioCycleEventKind::FlowApplied { .. }
        | DynamicMinRatioCycleEventKind::Completed => FlowFrameworkMcfStageV1::SourceProgress,
        DynamicMinRatioCycleEventKind::TopologyStageApplied { .. }
        | DynamicMinRatioCycleEventKind::PeriodicRebuilt { .. } => {
            FlowFrameworkMcfStageV1::PeriodicReinitialize
        }
    }
}

const fn flow_framework_dynamic_operation(
    kind: &DynamicMinRatioCycleEventKind,
) -> FlowFrameworkMcfDynamicOperationV1 {
    match kind {
        DynamicMinRatioCycleEventKind::TopologyStageApplied { .. } => {
            FlowFrameworkMcfDynamicOperationV1::TopologyStageApplied
        }
        DynamicMinRatioCycleEventKind::PeriodicRebuilt { .. } => {
            FlowFrameworkMcfDynamicOperationV1::PeriodicRebuilt
        }
        DynamicMinRatioCycleEventKind::CycleQueried { accepted: true, .. } => {
            FlowFrameworkMcfDynamicOperationV1::CycleQueriedAccepted
        }
        DynamicMinRatioCycleEventKind::CycleQueried {
            accepted: false, ..
        } => FlowFrameworkMcfDynamicOperationV1::CycleQueriedRejected,
        DynamicMinRatioCycleEventKind::LevelShifted { .. } => {
            FlowFrameworkMcfDynamicOperationV1::LevelShifted
        }
        DynamicMinRatioCycleEventKind::FlowApplied { .. } => {
            FlowFrameworkMcfDynamicOperationV1::FlowApplied
        }
        DynamicMinRatioCycleEventKind::QueryReturned { .. } => {
            FlowFrameworkMcfDynamicOperationV1::QueryReturned
        }
        DynamicMinRatioCycleEventKind::DetectReturned { .. } => {
            FlowFrameworkMcfDynamicOperationV1::DetectReturned
        }
        DynamicMinRatioCycleEventKind::Completed => FlowFrameworkMcfDynamicOperationV1::Completed,
    }
}

fn flow_framework_trace_event_scene(
    graph: &flow::FlowNetwork,
    stage: FlowFrameworkMcfStageV1,
    cycle: &[BigRational],
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let zero = BigRational::from_integer(0.into());
    let mut entity_refs = graph
        .edges()
        .iter()
        .zip(cycle)
        .filter(|(_, coefficient)| *coefficient != &zero)
        .map(|(edge, _)| FlowTraceEntityRefSceneV1::Edge {
            edge_id: edge.id().as_str().to_owned(),
        })
        .collect::<Vec<_>>();
    let cycle_edge_count = entity_refs.len();
    if matches!(
        stage,
        FlowFrameworkMcfStageV1::QueryMinimumRatioCycle | FlowFrameworkMcfStageV1::SourceProgress
    ) {
        // The dedicated source panel renders the complete signed circulation,
        // exact coefficient, fractional-flow change, ratio, and progress. A
        // second generic focus would turn every edge of a small cycle into an
        // undifferentiated whole-graph highlight.
        entity_refs.clear();
    }
    let (label, pseudocode) = match stage {
        FlowFrameworkMcfStageV1::InitializeSourcePoint => {
            ("initial point", "construct the strict source initial point")
        }
        FlowFrameworkMcfStageV1::PeriodicReinitialize => (
            "periodic reinitialize",
            "refresh f-tilde, gap, gradient, length, and the dynamic epoch",
        ),
        FlowFrameworkMcfStageV1::Detect => (
            "Detect",
            "Detect coordinates before refreshing source attributes",
        ),
        FlowFrameworkMcfStageV1::QueryMinimumRatioCycle => (
            "minimum-ratio cycle",
            "Query the topology-aware shifted tree chain",
        ),
        FlowFrameworkMcfStageV1::SourceProgress => (
            "source progress",
            "apply the accepted source-scaled circulation atomically",
        ),
        FlowFrameworkMcfStageV1::RoundFractionalFlow => (
            "flow rounding",
            "round the exact fractional flow while preserving divergence",
        ),
        FlowFrameworkMcfStageV1::CheckCertificate => (
            "certificate",
            "independently check the original minimum-cost flow",
        ),
        FlowFrameworkMcfStageV1::Optimal => (
            "termination",
            "publish after the source additive-half gate and checked rounding",
        ),
    };
    let detail = match stage {
        FlowFrameworkMcfStageV1::QueryMinimumRatioCycle
        | FlowFrameworkMcfStageV1::SourceProgress => cycle_edge_count.to_string(),
        FlowFrameworkMcfStageV1::PeriodicReinitialize
        | FlowFrameworkMcfStageV1::RoundFractionalFlow
        | FlowFrameworkMcfStageV1::CheckCertificate
        | FlowFrameworkMcfStageV1::Optimal => "1".to_owned(),
        FlowFrameworkMcfStageV1::InitializeSourcePoint | FlowFrameworkMcfStageV1::Detect => {
            "0".to_owned()
        }
    };
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: None,
        catalog_id: flow_framework_mcf_catalog_id(stage).to_owned(),
        minimum_granularity: match stage {
            FlowFrameworkMcfStageV1::InitializeSourcePoint
            | FlowFrameworkMcfStageV1::PeriodicReinitialize
            | FlowFrameworkMcfStageV1::Optimal => TraceGranularityV1::Phase,
            FlowFrameworkMcfStageV1::QueryMinimumRatioCycle => TraceGranularityV1::Micro,
            FlowFrameworkMcfStageV1::Detect
            | FlowFrameworkMcfStageV1::SourceProgress
            | FlowFrameworkMcfStageV1::RoundFractionalFlow
            | FlowFrameworkMcfStageV1::CheckCertificate => TraceGranularityV1::Operation,
        },
        pseudocode_line: pseudocode.to_owned(),
        patch_count: u32::try_from(cycle_edge_count.saturating_add(1))
            .map_err(|_| JsError::new("Flow Framework MCF patch count overflow"))?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value: detail,
        }),
    })
}

const fn flow_framework_mcf_catalog_id(stage: FlowFrameworkMcfStageV1) -> &'static str {
    match stage {
        FlowFrameworkMcfStageV1::InitializeSourcePoint => {
            "deterministic-almost-linear-mcf.initialize-source-point"
        }
        FlowFrameworkMcfStageV1::PeriodicReinitialize => {
            "deterministic-almost-linear-mcf.periodic-reinitialize"
        }
        FlowFrameworkMcfStageV1::Detect => "deterministic-almost-linear-mcf.detect",
        FlowFrameworkMcfStageV1::QueryMinimumRatioCycle => {
            "deterministic-almost-linear-mcf.query-minimum-ratio-cycle"
        }
        FlowFrameworkMcfStageV1::SourceProgress => {
            "deterministic-almost-linear-mcf.source-progress"
        }
        FlowFrameworkMcfStageV1::RoundFractionalFlow => {
            "deterministic-almost-linear-mcf.round-fractional-flow"
        }
        FlowFrameworkMcfStageV1::CheckCertificate => {
            "deterministic-almost-linear-mcf.check-certificate"
        }
        FlowFrameworkMcfStageV1::Optimal => "deterministic-almost-linear-mcf.optimal",
    }
}

fn set_flow_framework_metrics(
    scene: &mut FlowCurrentSceneV9,
    iteration: u64,
    snapshot: Option<&flow::DynamicMinRatioCycleSnapshot>,
    primary_work_offset: u128,
) -> Result<(), JsError> {
    let metrics = snapshot
        .map(|snapshot| snapshot.metrics.clone())
        .unwrap_or_default();
    let primary_work = primary_work_offset
        .checked_add(flow_framework_dynamic_primary_work_from_metrics(&metrics)?)
        .ok_or_else(|| JsError::new("Flow Framework MCF primary work overflow"))?;
    scene.set_flow_framework_mcf_metrics(
        iteration,
        metrics.cycle_queries,
        metrics.shifts,
        metrics.rebuilds,
        metrics.detected_edges,
        metrics.state_transitions,
        primary_work,
        metrics.intermediate_edge_inspections,
        metrics.terminal_edge_inspections,
        metrics.detection_edge_scans,
    );
    Ok(())
}

fn flow_framework_dynamic_primary_work(
    snapshot: &flow::DynamicMinRatioCycleSnapshot,
) -> Result<u128, JsError> {
    flow_framework_dynamic_primary_work_from_metrics(&snapshot.metrics)
}

fn flow_framework_dynamic_primary_work_from_metrics(
    metrics: &flow::DynamicMinRatioCycleMetrics,
) -> Result<u128, JsError> {
    u128::from(metrics.intermediate_edge_inspections)
        .checked_add(u128::from(metrics.terminal_edge_inspections))
        .and_then(|total| total.checked_add(u128::from(metrics.detection_edge_scans)))
        .ok_or_else(|| JsError::new("Flow Framework MCF primary work overflow"))
}

fn big_rational_scene(value: &BigRational) -> FlowRationalV1 {
    FlowRationalV1 {
        numerator: value.numer().to_string(),
        denominator: value.denom().to_string(),
    }
}

fn lower_bound_flows(graph: &flow::FlowNetwork) -> Vec<u64> {
    graph.edges().iter().map(flow::FlowEdge::lower).collect()
}

fn weighted_augmenting_paths_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    _source: flow::NodeIndex,
    _sink: flow::NodeIndex,
    run: &WeightedAugmentingPathsTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("weighted augmenting-path event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("weighted augmenting-path trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("weighted augmenting-path event identity overflow"))?;
        let mut scene = base.clone();
        let flows = weighted_augmenting_paths_flows(graph, &event.after)?;
        scene
            .apply_weighted_augmenting_paths_boundary(
                graph,
                &flows,
                weighted_augmenting_paths_overlay(graph, &event.after)?,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(weighted_augmenting_paths_trace_event_scene(
            graph, event, event_id,
        )?);
        scene.set_weighted_augmenting_paths_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene
                .set_weighted_augmenting_paths_outcome(&run.result.certificate)
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot
        || run.final_snapshot != run.result.final_snapshot
        || run.events.is_empty()
    {
        return Err(JsError::new(
            "weighted augmenting-path final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn weighted_augmenting_paths_flows(
    graph: &flow::FlowNetwork,
    snapshot: &WeightedAugmentingPathsSnapshot,
) -> Result<Vec<u64>, JsError> {
    if snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new(
            "weighted augmenting-path snapshot shape mismatch",
        ));
    }
    snapshot
        .edges
        .iter()
        .zip(graph.edges())
        .map(|(state, edge)| {
            if state.edge != *edge.id()
                || state.flow > state.scaled_capacity
                || state.scaled_capacity > edge.capacity()
            {
                return Err(JsError::new(
                    "weighted augmenting-path edge projection mismatch",
                ));
            }
            Ok(state.flow)
        })
        .collect()
}

fn weighted_augmenting_paths_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &WeightedAugmentingPathsSnapshot,
) -> Result<FlowWeightedAugmentingPathsOverlayV1, JsError> {
    if snapshot.nodes.len() != graph.nodes().len()
        || snapshot.edges.len() != graph.edges().len()
        || snapshot.residual_arcs.len() != graph.edges().len() * 2
    {
        return Err(JsError::new(
            "weighted augmenting-path snapshot shape mismatch",
        ));
    }
    let nodes = snapshot
        .nodes
        .iter()
        .map(|state| {
            let node = graph
                .node(state.node)
                .ok_or_else(|| JsError::new("weighted augmenting-path node out of range"))?;
            Ok(FlowWeightedAugmentingNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                component: state.component.to_string(),
                order: state.order.to_string(),
                label: state.label.to_string(),
                alive: state.alive,
                expansion_witness_side: state.expansion_witness_side,
                source_side: state.source_side,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let edges = snapshot
        .edges
        .iter()
        .zip(graph.edges())
        .map(|(state, edge)| {
            if state.edge != *edge.id() {
                return Err(JsError::new(
                    "weighted augmenting-path edge identity mismatch",
                ));
            }
            Ok(FlowWeightedAugmentingEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                scaled_capacity: state.scaled_capacity.to_string(),
                flow: state.flow.to_string(),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let residual_arcs = snapshot
        .residual_arcs
        .iter()
        .map(|state| {
            let from = graph
                .node(state.from)
                .ok_or_else(|| JsError::new("weighted augmenting-path residual tail missing"))?;
            let to = graph
                .node(state.to)
                .ok_or_else(|| JsError::new("weighted augmenting-path residual head missing"))?;
            Ok(FlowWeightedAugmentingResidualArcStateV1 {
                edge_id: state.id.original_edge().as_str().to_owned(),
                direction: residual_direction_name(state.id.direction()).to_owned(),
                from: from.id().as_str().to_owned(),
                to: to.id().as_str().to_owned(),
                capacity: state.capacity.to_string(),
                hierarchy_kind: state.hierarchy_kind.map(|kind| match kind {
                    WeightedAugmentingHierarchyKind::Dag => {
                        FlowWeightedAugmentingHierarchyKindV1::Dag
                    }
                    WeightedAugmentingHierarchyKind::Expanding => {
                        FlowWeightedAugmentingHierarchyKindV1::Expanding
                    }
                }),
                weight: state.weight.to_string(),
                admissible: state.admissible,
                active: state.active,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let active_path = snapshot
        .active_path
        .iter()
        .map(|id| FlowResidualArcRefV1 {
            edge_id: id.original_edge().as_str().to_owned(),
            direction: residual_direction_name(id.direction()).to_owned(),
        })
        .collect();
    Ok(FlowWeightedAugmentingPathsOverlayV1 {
        stage: weighted_augmenting_paths_scene_stage(snapshot.stage),
        phase: snapshot.phase.to_string(),
        phase_count: snapshot.phase_count.to_string(),
        capacity_bit: snapshot.capacity_bit.to_string(),
        round: snapshot.round.to_string(),
        height: snapshot.height.to_string(),
        phi_numerator: snapshot.phi_numerator.to_string(),
        phi_denominator: snapshot.phi_denominator.to_string(),
        active_bottleneck: snapshot.active_bottleneck.to_string(),
        hierarchy_cuts: snapshot.metrics.hierarchy_cuts.to_string(),
        relabel_jumps: snapshot.metrics.relabel_jumps.to_string(),
        augmentations: snapshot.metrics.augmentations.to_string(),
        augmented_units: snapshot.metrics.augmented_units.to_string(),
        nodes,
        edges,
        residual_arcs,
        active_path,
    })
}

const fn residual_direction_name(direction: ResidualDirection) -> &'static str {
    match direction {
        ResidualDirection::Forward => "forward",
        ResidualDirection::Reverse => "reverse",
    }
}

const fn weighted_augmenting_paths_scene_stage(
    stage: WeightedAugmentingPathsStage,
) -> FlowWeightedAugmentingPathsStageV1 {
    use FlowWeightedAugmentingPathsStageV1 as Scene;
    use WeightedAugmentingPathsStage as Core;
    match stage {
        Core::Ready => Scene::Ready,
        Core::BeginCapacityPhase => Scene::BeginCapacityPhase,
        Core::BuildHierarchy => Scene::BuildHierarchy,
        Core::CertifyExpansion => Scene::CertifyExpansion,
        Core::AssignWeights => Scene::AssignWeights,
        Core::RelabelSweep => Scene::RelabelSweep,
        Core::AugmentPath => Scene::AugmentPath,
        Core::FinishWeightedRound => Scene::FinishWeightedRound,
        Core::FinishCapacityPhase => Scene::FinishCapacityPhase,
        Core::CheckCertificate => Scene::CheckCertificate,
        Core::Optimal => Scene::Optimal,
    }
}

const fn weighted_augmenting_paths_catalog_id(stage: WeightedAugmentingPathsStage) -> &'static str {
    use WeightedAugmentingPathsStage as Stage;
    match stage {
        Stage::Ready => "weighted-augmenting-paths.ready",
        Stage::BeginCapacityPhase => "weighted-augmenting-paths.begin-capacity-phase",
        Stage::BuildHierarchy => "weighted-augmenting-paths.build-hierarchy",
        Stage::CertifyExpansion => "weighted-augmenting-paths.certify-expansion",
        Stage::AssignWeights => "weighted-augmenting-paths.assign-weights",
        Stage::RelabelSweep => "weighted-augmenting-paths.relabel-sweep",
        Stage::AugmentPath => "weighted-augmenting-paths.augment-path",
        Stage::FinishWeightedRound => "weighted-augmenting-paths.finish-weighted-round",
        Stage::FinishCapacityPhase => "weighted-augmenting-paths.finish-capacity-phase",
        Stage::CheckCertificate => "weighted-augmenting-paths.check-certificate",
        Stage::Optimal => "weighted-augmenting-paths.optimal",
    }
}

fn weighted_augmenting_paths_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &WeightedAugmentingPathsTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let changed_nodes = event
        .before
        .nodes
        .iter()
        .zip(&event.after.nodes)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let changed_edges = event
        .before
        .edges
        .iter()
        .zip(&event.after.edges)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let mut entity_refs = changed_edges
        .iter()
        .map(|index| FlowTraceEntityRefSceneV1::Edge {
            edge_id: graph.edges()[*index].id().as_str().to_owned(),
        })
        .collect::<Vec<_>>();
    entity_refs.extend(
        changed_nodes
            .iter()
            .map(|index| FlowTraceEntityRefSceneV1::Node {
                node_id: graph.nodes()[*index].id().as_str().to_owned(),
            }),
    );
    if event.after.stage == WeightedAugmentingPathsStage::RelabelSweep {
        if changed_nodes.len() != 1 {
            return Err(JsError::new(
                "weighted augmenting-path relabel must change exactly one node",
            ));
        }
        entity_refs.clear();
        entity_refs.push(FlowTraceEntityRefSceneV1::Node {
            node_id: graph.nodes()[changed_nodes[0]].id().as_str().to_owned(),
        });
    }
    let (label, value) = weighted_augmenting_paths_event_detail(&event.after);
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: None,
        catalog_id: weighted_augmenting_paths_catalog_id(event.after.stage).to_owned(),
        minimum_granularity: if matches!(
            event.after.stage,
            WeightedAugmentingPathsStage::AugmentPath
        ) {
            TraceGranularityV1::Operation
        } else if event.after.stage == WeightedAugmentingPathsStage::RelabelSweep {
            TraceGranularityV1::Micro
        } else {
            TraceGranularityV1::Phase
        },
        pseudocode_line: weighted_augmenting_paths_pseudocode_line(event.after.stage).to_owned(),
        patch_count: u32::try_from(changed_nodes.len() + changed_edges.len() + 1)
            .map_err(|_| JsError::new("weighted augmenting-path patch count overflow"))?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn weighted_augmenting_paths_event_detail(
    snapshot: &WeightedAugmentingPathsSnapshot,
) -> (&'static str, String) {
    use WeightedAugmentingPathsStage as Stage;
    match snapshot.stage {
        Stage::Ready | Stage::BeginCapacityPhase => {
            ("capacity phase", (snapshot.phase + 1).to_string())
        }
        Stage::BuildHierarchy => (
            "residual SCCs",
            snapshot
                .nodes
                .iter()
                .map(|node| node.component)
                .max()
                .map_or(0, |value| value + 1)
                .to_string(),
        ),
        Stage::CertifyExpansion => ("phi numerator", snapshot.phi_numerator.to_string()),
        Stage::AssignWeights => ("height h", snapshot.height.to_string()),
        Stage::RelabelSweep => ("relabel jumps", snapshot.metrics.relabel_jumps.to_string()),
        Stage::AugmentPath => ("bottleneck", snapshot.active_bottleneck.to_string()),
        Stage::FinishWeightedRound => ("weighted round", (snapshot.round + 1).to_string()),
        Stage::FinishCapacityPhase => ("augmentations", snapshot.metrics.augmentations.to_string()),
        Stage::CheckCertificate | Stage::Optimal => (
            "certificate checks",
            snapshot.metrics.certificate_checks.to_string(),
        ),
    }
}

const fn weighted_augmenting_paths_pseudocode_line(
    stage: WeightedAugmentingPathsStage,
) -> &'static str {
    use WeightedAugmentingPathsStage as Stage;
    match stage {
        Stage::Ready => "validate the bounded integral simple max-flow instance",
        Stage::BeginCapacityPhase => "double the previous flow and append the next capacity bit",
        Stage::BuildHierarchy => "put inter-SCC arcs in D and internal arcs in X_1",
        Stage::CertifyExpansion => "enumerate every directed SCC cut and certify maximal phi",
        Stage::AssignWeights => "compute a respecting order tau and w(u,v)=|tau_u-tau_v|",
        Stage::RelabelSweep => {
            "raise a label to incident-weight multiples until admissible or dead"
        }
        Stage::AugmentPath => "follow decreasing admissible labels and augment the bottleneck",
        Stage::FinishWeightedRound => "return when the residual source is dead",
        Stage::FinishCapacityPhase => "verify that the scaled residual graph has no s-t path",
        Stage::CheckCertificate => "check the full-capacity original flow and minimum cut",
        Stage::Optimal => "publish exact maximum flow without claiming n^(2+o(1)) runtime",
    }
}

fn apply_weighted_augmenting_paths_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    _source: flow::NodeIndex,
    _sink: flow::NodeIndex,
    result: &WeightedAugmentingPathsResult,
) -> Result<(), JsError> {
    if result.flows != weighted_augmenting_paths_flows(graph, &result.final_snapshot)? {
        return Err(JsError::new(
            "weighted augmenting-path result projection mismatch",
        ));
    }
    scene
        .apply_weighted_augmenting_paths_boundary(
            graph,
            &result.flows,
            weighted_augmenting_paths_overlay(graph, &result.final_snapshot)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_weighted_augmenting_paths_metrics(result.metrics);
    scene
        .set_weighted_augmenting_paths_outcome(&result.certificate)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(())
}

fn weighted_push_relabel_shortcut_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    _source: flow::NodeIndex,
    _sink: flow::NodeIndex,
    run: &WeightedPushRelabelShortcutTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("weighted push-relabel event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("weighted push-relabel trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("weighted push-relabel event identity overflow"))?;
        let mut scene = base.clone();
        let flows = weighted_push_relabel_shortcut_flows(graph, &event.after)?;
        let overlay = weighted_push_relabel_shortcut_overlay(graph, &event.after)?;
        scene
            .apply_weighted_push_relabel_shortcut_boundary(
                graph,
                &flows,
                overlay,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(weighted_push_relabel_shortcut_trace_event_scene(
            graph, event, event_id,
        )?);
        scene.set_weighted_push_relabel_shortcut_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene
                .set_weighted_push_relabel_shortcut_outcome(&run.result.certificate)
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot
        || run.final_snapshot != run.result.final_snapshot
        || run.events.is_empty()
    {
        return Err(JsError::new(
            "weighted push-relabel final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn weighted_push_relabel_shortcut_flows(
    graph: &flow::FlowNetwork,
    snapshot: &WeightedPushRelabelShortcutSnapshot,
) -> Result<Vec<u64>, JsError> {
    if let Some(flows) = &snapshot.exact_flows {
        if flows.len() != graph.edges().len() {
            return Err(JsError::new(
                "weighted push-relabel exact-flow shape mismatch",
            ));
        }
        return Ok(flows.clone());
    }
    if snapshot.edges.is_empty() {
        return Ok(vec![0; graph.edges().len()]);
    }
    if snapshot.edges.len() < graph.edges().len() {
        return Err(JsError::new(
            "weighted push-relabel augmented-edge shape mismatch",
        ));
    }
    snapshot
        .edges
        .iter()
        .take(graph.edges().len())
        .zip(graph.edges())
        .map(|(state, edge)| {
            if state.original_edge.as_ref() != Some(edge.id()) || state.flow > edge.capacity() {
                return Err(JsError::new(
                    "weighted push-relabel original-edge projection mismatch",
                ));
            }
            Ok(state.flow)
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
fn weighted_push_relabel_shortcut_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &WeightedPushRelabelShortcutSnapshot,
) -> Result<FlowWeightedPushRelabelShortcutOverlayV1, JsError> {
    validate_weighted_push_relabel_shortcut_shape(graph, snapshot)?;
    let node_ids = weighted_push_relabel_shortcut_node_ids(graph, snapshot)?;
    let nodes = snapshot
        .nodes
        .iter()
        .zip(&node_ids)
        .map(
            |(state, node_id)| FlowWeightedPushRelabelShortcutNodeStateV1 {
                node_id: node_id.clone(),
                original: state.original_node.is_some(),
                component: state.component.to_string(),
                order: state.order.to_string(),
                label: state.label.to_string(),
                alive: state.alive,
                sparse_cut_side: state.sparse_cut_side,
                source_side: state.source_side,
            },
        )
        .collect::<Vec<_>>();
    let edge_ids = weighted_push_relabel_shortcut_edge_ids(snapshot, &node_ids)?;
    let edges =
        snapshot
            .edges
            .iter()
            .zip(&edge_ids)
            .map(|(state, edge_id)| {
                let from = node_ids.get(state.from).cloned().ok_or_else(|| {
                    JsError::new("weighted push-relabel edge tail is out of range")
                })?;
                let to = node_ids.get(state.to).cloned().ok_or_else(|| {
                    JsError::new("weighted push-relabel edge head is out of range")
                })?;
                Ok(FlowWeightedPushRelabelShortcutEdgeStateV1 {
                    edge_id: edge_id.clone(),
                    kind: match state.kind {
                        WeightedPushRelabelShortcutEdgeKind::Original => "original",
                        WeightedPushRelabelShortcutEdgeKind::Shortcut => "shortcut",
                    }
                    .to_owned(),
                    from,
                    to,
                    capacity: state.capacity.to_string(),
                    flow: state.flow.to_string(),
                    weight: state.weight.to_string(),
                    shortcut_component: state.shortcut_component.map(|value| value.to_string()),
                })
            })
            .collect::<Result<Vec<_>, JsError>>()?;
    let residual_arcs = snapshot
        .residual_arcs
        .iter()
        .map(|state| {
            Ok(FlowWeightedPushRelabelShortcutResidualArcStateV1 {
                edge_id: edge_ids.get(state.id.edge).cloned().ok_or_else(|| {
                    JsError::new("weighted push-relabel residual edge is out of range")
                })?,
                direction: weighted_push_relabel_shortcut_direction_name(state.id.direction)
                    .to_owned(),
                from: node_ids.get(state.from).cloned().ok_or_else(|| {
                    JsError::new("weighted push-relabel residual tail is out of range")
                })?,
                to: node_ids.get(state.to).cloned().ok_or_else(|| {
                    JsError::new("weighted push-relabel residual head is out of range")
                })?,
                capacity: state.capacity.to_string(),
                weight: state.weight.to_string(),
                admissible: state.admissible,
                active: state.active,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let active_path = snapshot
        .active_path
        .iter()
        .map(|arc| {
            Ok(FlowWeightedPushRelabelShortcutArcRefV1 {
                edge_id: edge_ids.get(arc.edge).cloned().ok_or_else(|| {
                    JsError::new("weighted push-relabel active path edge is out of range")
                })?,
                direction: weighted_push_relabel_shortcut_direction_name(arc.direction).to_owned(),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let inspected_arcs = if let Some(edge) = snapshot.inspected_edge {
        let edge_id = edge_ids
            .get(edge)
            .ok_or_else(|| JsError::new("weighted push-relabel inspected edge is out of range"))?;
        let directions = match snapshot.inspected_direction {
            Some(direction) => vec![direction],
            None => vec![
                WeightedPushRelabelShortcutDirection::Forward,
                WeightedPushRelabelShortcutDirection::Reverse,
            ],
        };
        directions
            .into_iter()
            .map(|direction| FlowWeightedPushRelabelShortcutArcRefV1 {
                edge_id: edge_id.clone(),
                direction: weighted_push_relabel_shortcut_direction_name(direction).to_owned(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let active_relabel_nodes = snapshot
        .active_relabel_node
        .map(|node| {
            node_ids
                .get(node)
                .cloned()
                .ok_or_else(|| JsError::new("weighted push-relabel relabel node is out of range"))
        })
        .transpose()?
        .into_iter()
        .collect();
    Ok(FlowWeightedPushRelabelShortcutOverlayV1 {
        stage: weighted_push_relabel_shortcut_scene_stage(snapshot.stage),
        hierarchy_levels: snapshot.hierarchy_levels.to_string(),
        psi_numerator: snapshot.psi_numerator.to_string(),
        psi_denominator: snapshot.psi_denominator.to_string(),
        height: snapshot.height.to_string(),
        demand: snapshot.demand.to_string(),
        routed: snapshot.routed.to_string(),
        weighted_length: snapshot.weighted_length.to_string(),
        weighted_length_units: snapshot.weighted_length_units.to_string(),
        sparse_cut_level: snapshot.sparse_cut_level.to_string(),
        sparse_cut_capacity: snapshot.sparse_cut_capacity.to_string(),
        active_bottleneck: snapshot.active_bottleneck.to_string(),
        relabel_steps: snapshot.metrics.relabel_steps.to_string(),
        augmentations: snapshot.metrics.augmentations.to_string(),
        shortcut_traversals: snapshot.metrics.shortcut_traversals.to_string(),
        residual_rounds: snapshot.metrics.residual_rounds.to_string(),
        completion_relabel_steps: snapshot.metrics.completion_relabel_steps.to_string(),
        completion_augmentations: snapshot.metrics.completion_augmentations.to_string(),
        nodes,
        edges,
        residual_arcs,
        active_path,
        inspected_arcs,
        active_relabel_nodes,
    })
}

fn validate_weighted_push_relabel_shortcut_shape(
    graph: &flow::FlowNetwork,
    snapshot: &WeightedPushRelabelShortcutSnapshot,
) -> Result<(), JsError> {
    if snapshot.stage == WeightedPushRelabelShortcutStage::Ready {
        if snapshot.nodes.len() != graph.nodes().len()
            || !snapshot.edges.is_empty()
            || !snapshot.residual_arcs.is_empty()
        {
            return Err(JsError::new(
                "weighted push-relabel ready snapshot shape mismatch",
            ));
        }
    } else if snapshot.nodes.len() < graph.nodes().len()
        || snapshot.edges.len() < graph.edges().len()
        || snapshot.residual_arcs.len() != snapshot.edges.len() * 2
    {
        return Err(JsError::new(
            "weighted push-relabel snapshot shape mismatch",
        ));
    }
    Ok(())
}

fn weighted_push_relabel_shortcut_node_ids(
    graph: &flow::FlowNetwork,
    snapshot: &WeightedPushRelabelShortcutSnapshot,
) -> Result<Vec<String>, JsError> {
    snapshot
        .nodes
        .iter()
        .map(|state| {
            if let Some(node) = state.original_node {
                graph
                    .node(node)
                    .map(|value| value.id().as_str().to_owned())
                    .ok_or_else(|| JsError::new("weighted push-relabel node out of range"))
            } else {
                Ok(format!("shortcut:{}", state.component))
            }
        })
        .collect()
}

fn weighted_push_relabel_shortcut_edge_ids(
    snapshot: &WeightedPushRelabelShortcutSnapshot,
    node_ids: &[String],
) -> Result<Vec<String>, JsError> {
    snapshot
        .edges
        .iter()
        .map(|state| {
            if let Some(edge) = &state.original_edge {
                return Ok(edge.as_str().to_owned());
            }
            let component = state.shortcut_component.ok_or_else(|| {
                JsError::new("weighted push-relabel shortcut component is absent")
            })?;
            let from = node_ids.get(state.from).ok_or_else(|| {
                JsError::new("weighted push-relabel shortcut tail is out of range")
            })?;
            let to = node_ids.get(state.to).ok_or_else(|| {
                JsError::new("weighted push-relabel shortcut head is out of range")
            })?;
            Ok(format!("shortcut-edge:{component}:{from}:{to}"))
        })
        .collect()
}

const fn weighted_push_relabel_shortcut_direction_name(
    direction: WeightedPushRelabelShortcutDirection,
) -> &'static str {
    match direction {
        WeightedPushRelabelShortcutDirection::Forward => "forward",
        WeightedPushRelabelShortcutDirection::Reverse => "reverse",
    }
}

const fn weighted_push_relabel_shortcut_scene_stage(
    stage: WeightedPushRelabelShortcutStage,
) -> FlowWeightedPushRelabelShortcutStageV1 {
    use FlowWeightedPushRelabelShortcutStageV1 as Scene;
    use WeightedPushRelabelShortcutStage as Core;
    match stage {
        Core::Ready => Scene::Ready,
        Core::BuildWeakHierarchy => Scene::BuildWeakHierarchy,
        Core::BuildShortcutGraph => Scene::BuildShortcutGraph,
        Core::AssignWeights => Scene::AssignWeights,
        Core::InitializeDemand => Scene::InitializeDemand,
        Core::RelabelSweep => Scene::RelabelSweep,
        Core::RelabelCheckpoint => Scene::RelabelCheckpoint,
        Core::InspectPrimitiveArcCheckpoint => Scene::InspectPrimitiveArcCheckpoint,
        Core::AugmentPath => Scene::AugmentPath,
        Core::MeasureShortFlow => Scene::MeasureShortFlow,
        Core::ComputeDistanceLayers => Scene::ComputeDistanceLayers,
        Core::SelectSparseCut => Scene::SelectSparseCut,
        Core::CompletionInspectPrimitiveArcCheckpoint => {
            Scene::CompletionInspectPrimitiveArcCheckpoint
        }
        Core::CompletionRelabelCheckpoint => Scene::CompletionRelabelCheckpoint,
        Core::CompletionAugmentPath => Scene::CompletionAugmentPath,
        Core::CompletionResidualRound => Scene::CompletionResidualRound,
        Core::CompleteResidualRounds => Scene::CompleteResidualRounds,
        Core::CheckCertificate => Scene::CheckCertificate,
        Core::Optimal => Scene::Optimal,
    }
}

const fn weighted_push_relabel_shortcut_catalog_id(
    stage: WeightedPushRelabelShortcutStage,
) -> &'static str {
    use WeightedPushRelabelShortcutStage as Stage;
    match stage {
        Stage::Ready => "weighted-push-relabel.ready",
        Stage::BuildWeakHierarchy => "weighted-push-relabel.build-weak-hierarchy",
        Stage::BuildShortcutGraph => "weighted-push-relabel.build-shortcut-graph",
        Stage::AssignWeights => "weighted-push-relabel.assign-weights",
        Stage::InitializeDemand => "weighted-push-relabel.initialize-demand",
        Stage::RelabelSweep => "weighted-push-relabel.relabel-sweep",
        Stage::RelabelCheckpoint => "weighted-push-relabel.relabel-checkpoint",
        Stage::InspectPrimitiveArcCheckpoint => {
            "weighted-push-relabel.inspect-primitive-arc-checkpoint"
        }
        Stage::AugmentPath => "weighted-push-relabel.augment-path",
        Stage::MeasureShortFlow => "weighted-push-relabel.measure-short-flow",
        Stage::ComputeDistanceLayers => "weighted-push-relabel.compute-distance-layers",
        Stage::SelectSparseCut => "weighted-push-relabel.select-sparse-cut",
        Stage::CompletionInspectPrimitiveArcCheckpoint => {
            "weighted-push-relabel.completion-inspect-primitive-arc-checkpoint"
        }
        Stage::CompletionRelabelCheckpoint => "weighted-push-relabel.completion-relabel-checkpoint",
        Stage::CompletionAugmentPath => "weighted-push-relabel.completion-augment-path",
        Stage::CompletionResidualRound => "weighted-push-relabel.completion-residual-round",
        Stage::CompleteResidualRounds => "weighted-push-relabel.complete-residual-rounds",
        Stage::CheckCertificate => "weighted-push-relabel.check-certificate",
        Stage::Optimal => "weighted-push-relabel.optimal",
    }
}

#[allow(clippy::too_many_lines)]
fn weighted_push_relabel_shortcut_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &WeightedPushRelabelShortcutTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let changed_original_nodes = event
        .before
        .nodes
        .iter()
        .zip(&event.after.nodes)
        .take(graph.nodes().len())
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let changed_original_edges = event
        .before
        .edges
        .iter()
        .zip(&event.after.edges)
        .take(graph.edges().len())
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let mut entity_refs = changed_original_edges
        .iter()
        .map(|index| FlowTraceEntityRefSceneV1::Edge {
            edge_id: graph.edges()[*index].id().as_str().to_owned(),
        })
        .collect::<Vec<_>>();
    if entity_refs.is_empty() {
        entity_refs.extend(changed_original_nodes.iter().map(|index| {
            FlowTraceEntityRefSceneV1::Node {
                node_id: graph.nodes()[*index].id().as_str().to_owned(),
            }
        }));
    }
    if matches!(
        event.after.stage,
        WeightedPushRelabelShortcutStage::InspectPrimitiveArcCheckpoint
            | WeightedPushRelabelShortcutStage::CompletionInspectPrimitiveArcCheckpoint
    ) {
        entity_refs.clear();
        let inspected = event
            .after
            .inspected_edge
            .ok_or_else(|| JsError::new("weighted push-relabel inspected edge is absent"))?;
        let inspected_state =
            event.after.edges.get(inspected).ok_or_else(|| {
                JsError::new("weighted push-relabel inspected edge is out of range")
            })?;
        if let Some(original_edge) = &inspected_state.original_edge {
            let original_index = graph.edge_index(original_edge).ok_or_else(|| {
                JsError::new("weighted push-relabel original inspected edge is absent")
            })?;
            let edge = graph.edge(original_index).ok_or_else(|| {
                JsError::new("weighted push-relabel original inspected edge is out of range")
            })?;
            entity_refs.push(if let Some(direction) = event.after.inspected_direction {
                FlowTraceEntityRefSceneV1::ResidualArc {
                    edge_id: edge.id().as_str().to_owned(),
                    direction: weighted_push_relabel_shortcut_direction_name(direction).to_owned(),
                }
            } else {
                FlowTraceEntityRefSceneV1::Edge {
                    edge_id: edge.id().as_str().to_owned(),
                }
            });
        }
    } else if matches!(
        event.after.stage,
        WeightedPushRelabelShortcutStage::RelabelCheckpoint
            | WeightedPushRelabelShortcutStage::CompletionRelabelCheckpoint
    ) {
        entity_refs.clear();
        let node = event
            .after
            .active_relabel_node
            .ok_or_else(|| JsError::new("weighted push-relabel active relabel node is absent"))?;
        let relabel_state =
            event.after.nodes.get(node).ok_or_else(|| {
                JsError::new("weighted push-relabel relabel node is out of range")
            })?;
        if let Some(original_node) = relabel_state.original_node {
            let original = graph.node(original_node).ok_or_else(|| {
                JsError::new("weighted push-relabel original relabel node is absent")
            })?;
            entity_refs.push(FlowTraceEntityRefSceneV1::Node {
                node_id: original.id().as_str().to_owned(),
            });
        }
    }
    let (label, value) = weighted_push_relabel_shortcut_event_detail(&event.after)?;
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: None,
        catalog_id: weighted_push_relabel_shortcut_catalog_id(event.after.stage).to_owned(),
        minimum_granularity: if matches!(
            event.after.stage,
            WeightedPushRelabelShortcutStage::RelabelSweep
                | WeightedPushRelabelShortcutStage::AugmentPath
                | WeightedPushRelabelShortcutStage::CompletionAugmentPath
        ) {
            TraceGranularityV1::Operation
        } else if matches!(
            event.after.stage,
            WeightedPushRelabelShortcutStage::RelabelCheckpoint
                | WeightedPushRelabelShortcutStage::InspectPrimitiveArcCheckpoint
                | WeightedPushRelabelShortcutStage::CompletionRelabelCheckpoint
                | WeightedPushRelabelShortcutStage::CompletionInspectPrimitiveArcCheckpoint
        ) {
            TraceGranularityV1::Micro
        } else {
            TraceGranularityV1::Phase
        },
        pseudocode_line: weighted_push_relabel_shortcut_pseudocode_line(event.after.stage)
            .to_owned(),
        patch_count: u32::try_from(changed_original_nodes.len() + changed_original_edges.len() + 1)
            .map_err(|_| JsError::new("weighted push-relabel patch count overflow"))?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn weighted_push_relabel_shortcut_event_detail(
    snapshot: &WeightedPushRelabelShortcutSnapshot,
) -> Result<(&'static str, String), JsError> {
    use WeightedPushRelabelShortcutStage as Stage;
    let detail = match snapshot.stage {
        Stage::Ready | Stage::BuildWeakHierarchy => {
            ("hierarchy levels", snapshot.hierarchy_levels.to_string())
        }
        Stage::BuildShortcutGraph => (
            "shortcut edges",
            snapshot.metrics.shortcut_edges.to_string(),
        ),
        Stage::AssignWeights | Stage::InitializeDemand => ("height h", snapshot.height.to_string()),
        Stage::RelabelSweep => ("relabel steps", snapshot.metrics.relabel_steps.to_string()),
        Stage::RelabelCheckpoint => {
            let _node = snapshot.active_relabel_node.ok_or_else(|| {
                JsError::new("weighted push-relabel active relabel node is absent")
            })?;
            (
                "relabel checkpoint",
                snapshot.metrics.relabel_steps.to_string(),
            )
        }
        Stage::CompletionRelabelCheckpoint => {
            let _node = snapshot.active_relabel_node.ok_or_else(|| {
                JsError::new("weighted push-relabel completion relabel node is absent")
            })?;
            (
                "completion relabel checkpoint",
                snapshot.metrics.completion_relabel_steps.to_string(),
            )
        }
        Stage::InspectPrimitiveArcCheckpoint => {
            let _edge = snapshot
                .inspected_edge
                .ok_or_else(|| JsError::new("weighted push-relabel inspected edge is absent"))?;
            let direction = snapshot.inspected_direction.map_or(
                "both directions",
                weighted_push_relabel_shortcut_direction_name,
            );
            (
                if direction == "both directions" {
                    "bidirectional inspection checkpoint"
                } else {
                    "directed inspection checkpoint"
                },
                snapshot.metrics.primitive_arc_inspections.to_string(),
            )
        }
        Stage::CompletionInspectPrimitiveArcCheckpoint => {
            let _edge = snapshot.inspected_edge.ok_or_else(|| {
                JsError::new("weighted push-relabel completion inspected edge is absent")
            })?;
            (
                "completion arc inspection checkpoint",
                snapshot.metrics.primitive_arc_inspections.to_string(),
            )
        }
        Stage::AugmentPath | Stage::CompletionAugmentPath => {
            ("bottleneck", snapshot.active_bottleneck.to_string())
        }
        Stage::MeasureShortFlow => ("routed units", snapshot.routed.to_string()),
        Stage::ComputeDistanceLayers => (
            "distance scans",
            snapshot.metrics.distance_arc_scans.to_string(),
        ),
        Stage::SelectSparseCut => ("cut capacity", snapshot.sparse_cut_capacity.to_string()),
        Stage::CompletionResidualRound | Stage::CompleteResidualRounds => (
            "residual rounds",
            snapshot.metrics.residual_rounds.to_string(),
        ),
        Stage::CheckCertificate | Stage::Optimal => (
            "certificate checks",
            snapshot.metrics.certificate_checks.to_string(),
        ),
    };
    Ok(detail)
}

const fn weighted_push_relabel_shortcut_pseudocode_line(
    stage: WeightedPushRelabelShortcutStage,
) -> &'static str {
    use WeightedPushRelabelShortcutStage as Stage;
    match stage {
        Stage::Ready => "validate the bounded positive-capacity max-flow instance",
        Stage::BuildWeakHierarchy => "build one SCC level and a respecting order tau",
        Stage::BuildShortcutGraph => "add a bidirectional Steiner star for each component",
        Stage::AssignWeights => "set original weight |tau(u)-tau(v)| and shortcut weight |C|",
        Stage::InitializeDemand => "set n^3 U terminal demand, psi=1, and source height h",
        Stage::RelabelSweep => "relabel every alive non-sink without an admissible out-arc",
        Stage::RelabelCheckpoint => {
            "publish source-time progress through literal weighted-label increments"
        }
        Stage::InspectPrimitiveArcCheckpoint => {
            "publish source-time progress through concrete augmented-edge inspections"
        }
        Stage::AugmentPath => "follow admissible arcs and augment a simple shortcut path",
        Stage::MeasureShortFlow => "measure routed value and average weighted path length",
        Stage::ComputeDistanceLayers => "zero forward-order weights and run residual Dijkstra",
        Stage::SelectSparseCut => "select the minimum residual distance-layer cut",
        Stage::CompletionInspectPrimitiveArcCheckpoint => {
            "inspect a concrete arc in the exact original-residual kernel"
        }
        Stage::CompletionRelabelCheckpoint => {
            "increment one original-node label in the exact residual kernel"
        }
        Stage::CompletionAugmentPath => {
            "augment one concrete path in the exact original residual graph"
        }
        Stage::CompletionResidualRound => {
            "apply one exact weighted residual-kernel call to the original flow"
        }
        Stage::CompleteResidualRounds => {
            "repeat weighted push-relabel on original residual graphs until routed value is zero"
        }
        Stage::CheckCertificate => "check original conservation, capacity, flow value, and min cut",
        Stage::Optimal => "publish exact flow without claiming the source asymptotic runtime",
    }
}

fn apply_weighted_push_relabel_shortcut_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    _source: flow::NodeIndex,
    _sink: flow::NodeIndex,
    result: &WeightedPushRelabelShortcutResult,
) -> Result<(), JsError> {
    if result.flows != weighted_push_relabel_shortcut_flows(graph, &result.final_snapshot)? {
        return Err(JsError::new(
            "weighted push-relabel result projection mismatch",
        ));
    }
    scene
        .apply_weighted_push_relabel_shortcut_boundary(
            graph,
            &result.flows,
            weighted_push_relabel_shortcut_overlay(graph, &result.final_snapshot)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_weighted_push_relabel_shortcut_metrics(result.metrics);
    scene
        .set_weighted_push_relabel_shortcut_outcome(&result.certificate)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(())
}

fn randomized_almost_linear_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    run: &RandomizedAlmostLinearMaxFlowTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("randomized almost-linear event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("randomized almost-linear trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("randomized almost-linear event identity overflow"))?;
        let mut scene = base.clone();
        let flows = randomized_almost_linear_flows(graph, &event.after)?;
        scene
            .apply_randomized_almost_linear_boundary(
                graph,
                source,
                sink,
                &flows,
                randomized_almost_linear_overlay(graph, &event.after)?,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(randomized_almost_linear_trace_event_scene(
            graph, event, event_id,
        )?);
        scene.set_randomized_almost_linear_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene
                .set_randomized_almost_linear_outcome(&run.result.certificate)
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot
        || run.final_snapshot != run.result.final_snapshot
        || run.events.is_empty()
    {
        return Err(JsError::new(
            "randomized almost-linear final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn randomized_almost_linear_flows(
    graph: &flow::FlowNetwork,
    snapshot: &RandomizedAlmostLinearMaxFlowSnapshot,
) -> Result<Vec<u64>, JsError> {
    if snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new(
            "randomized almost-linear snapshot shape mismatch",
        ));
    }
    if snapshot.edges.iter().all(|edge| edge.final_flow.is_some()) {
        snapshot
            .edges
            .iter()
            .map(|edge| {
                edge.final_flow
                    .ok_or_else(|| JsError::new("randomized almost-linear rounded flow missing"))
            })
            .collect()
    } else if snapshot.edges.iter().all(|edge| edge.final_flow.is_none()) {
        Ok(vec![0; graph.edges().len()])
    } else {
        Err(JsError::new(
            "randomized almost-linear partial rounded flow",
        ))
    }
}

#[allow(clippy::too_many_lines)]
fn randomized_almost_linear_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &RandomizedAlmostLinearMaxFlowSnapshot,
) -> Result<FlowRandomizedAlmostLinearOverlayV1, JsError> {
    if snapshot.nodes.len() != graph.nodes().len() || snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new(
            "randomized almost-linear snapshot shape mismatch",
        ));
    }
    let nodes = graph
        .nodes()
        .iter()
        .zip(&snapshot.nodes)
        .map(|(node, state)| {
            if state.node.as_usize() >= graph.nodes().len()
                || graph.nodes()[state.node.as_usize()].id() != node.id()
            {
                return Err(JsError::new(
                    "randomized almost-linear node identity mismatch",
                ));
            }
            let parent = state
                .tree_parent
                .map(|parent| {
                    if parent == graph.nodes().len() {
                        Ok("__artificial_star__".to_owned())
                    } else {
                        graph
                            .nodes()
                            .get(parent)
                            .map(|parent_node| parent_node.id().as_str().to_owned())
                            .ok_or_else(|| {
                                JsError::new("randomized almost-linear parent out of range")
                            })
                    }
                })
                .transpose()?;
            Ok(FlowRandomizedAlmostLinearNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                tree_parent_node_id: parent,
                tree_component: state.tree_component.to_string(),
                source_side: state.source_side,
                artificial_direction: state.artificial_direction.to_string(),
                artificial_flow: state.artificial_flow.decimal(),
                artificial_capacity: state.artificial_capacity.decimal(),
                artificial_tree_memberships: state.artificial_tree_memberships.to_string(),
                active_artificial_tree_edge: state.active_artificial_tree_edge,
                active_artificial_sign: state.active_artificial_sign.to_string(),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let edges = graph
        .edges()
        .iter()
        .zip(&snapshot.edges)
        .map(|(edge, state)| {
            if state.edge != *edge.id() {
                return Err(JsError::new(
                    "randomized almost-linear edge identity mismatch",
                ));
            }
            Ok(FlowRandomizedAlmostLinearEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                interior_flow: state.interior_flow.decimal(),
                gradient: state.gradient.decimal(),
                length: state.length.decimal(),
                sampled_tree_memberships: state.sampled_tree_memberships.to_string(),
                active_tree_edge: state.active_tree_edge,
                active_cycle_sign: state.active_cycle_sign.to_string(),
                changed_coordinate: state.changed_coordinate,
                isolation_draw: state.isolation_draw.to_string(),
                final_point_flow: state
                    .final_point_flow
                    .map(flow::RandomizedAlmostLinearScalar::decimal),
                final_flow: state.final_flow.map(|flow| flow.to_string()),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowRandomizedAlmostLinearOverlayV1 {
        stage: randomized_almost_linear_scene_stage(snapshot.stage),
        seed: snapshot.seed.to_string(),
        random_draws: snapshot.random_draws.to_string(),
        alpha: snapshot.alpha.decimal(),
        potential: snapshot.potential.decimal(),
        cost_gap: snapshot.cost_gap.decimal(),
        selected_ratio: snapshot
            .selected_ratio
            .map(flow::RandomizedAlmostLinearScalar::decimal),
        exact_pool_ratio: snapshot
            .exact_pool_ratio
            .map(flow::RandomizedAlmostLinearScalar::decimal),
        miss_probability: FlowRandomizedAlmostLinearProbabilityV1 {
            numerator: snapshot.miss_probability.numerator.to_string(),
            denominator: snapshot.miss_probability.denominator.to_string(),
        },
        forest_pool_size: snapshot.forest_pool_size.to_string(),
        sample_count: snapshot.sample_count.to_string(),
        iteration: snapshot.iteration.to_string(),
        rebuild_epoch: snapshot.rebuild_epoch.to_string(),
        return_flow: snapshot.return_flow.decimal(),
        return_capacity: snapshot.return_capacity.to_string(),
        return_gradient: snapshot.return_gradient.decimal(),
        return_length: snapshot.return_length.decimal(),
        return_tree_memberships: snapshot.return_tree_memberships.to_string(),
        active_return_tree_edge: snapshot.active_return_tree_edge,
        active_return_sign: snapshot.active_return_sign.to_string(),
        return_isolation_draw: snapshot.return_isolation_draw.to_string(),
        final_point_return_flow: snapshot
            .final_point_return_flow
            .map(flow::RandomizedAlmostLinearScalar::decimal),
        final_return_flow: snapshot.final_return_flow.map(|flow| flow.to_string()),
        artificial_edges: snapshot.artificial_edges.to_string(),
        artificial_flow: snapshot.artificial_flow.decimal(),
        final_artificial_flow: snapshot.final_artificial_flow.map(|flow| flow.to_string()),
        isolation_scale: snapshot.isolation_scale.to_string(),
        isolation_attempt: snapshot.isolation_attempt.to_string(),
        isolation_failure_probability: FlowRandomizedAlmostLinearProbabilityV1 {
            numerator: snapshot.isolation_failure_probability.numerator.to_string(),
            denominator: snapshot
                .isolation_failure_probability
                .denominator
                .to_string(),
        },
        isolated_objective: snapshot.isolated_objective.map(|value| value.to_string()),
        final_point_threshold: snapshot.final_point_threshold.decimal(),
        final_point_gap: snapshot
            .final_point_gap
            .map(flow::RandomizedAlmostLinearScalar::decimal),
        final_point_mix: snapshot
            .final_point_mix
            .map(flow::RandomizedAlmostLinearScalar::decimal),
        target_value: snapshot.target_value.to_string(),
        nodes,
        edges,
    })
}

const fn randomized_almost_linear_scene_stage(
    stage: RandomizedAlmostLinearMaxFlowStage,
) -> FlowRandomizedAlmostLinearStageV1 {
    use FlowRandomizedAlmostLinearStageV1 as Scene;
    use RandomizedAlmostLinearMaxFlowStage as Core;
    match stage {
        Core::Ready => Scene::Ready,
        Core::BuildReturnEdgeReduction => Scene::BuildReturnEdgeReduction,
        Core::BuildInitialPoint => Scene::BuildInitialPoint,
        Core::EnumerateForestPool => Scene::EnumerateForestPool,
        Core::SampleTreeChain => Scene::SampleTreeChain,
        Core::InspectFundamentalCycle => Scene::InspectFundamentalCycle,
        Core::QueryMinimumRatioCycle => Scene::QueryMinimumRatioCycle,
        Core::SamplingFailure => Scene::SamplingFailure,
        Core::PotentialReductionStep => Scene::PotentialReductionStep,
        Core::DetectChangedCoordinates => Scene::DetectChangedCoordinates,
        Core::RebuildTreeChain => Scene::RebuildTreeChain,
        Core::InspectFeasibleAssignment => Scene::InspectFeasibleAssignment,
        Core::EnumerateFeasibleSet => Scene::EnumerateFeasibleSet,
        Core::SampleIsolationCosts => Scene::SampleIsolationCosts,
        Core::SelectIsolatedOptimum => Scene::SelectIsolatedOptimum,
        Core::ConstructFinalPoint => Scene::ConstructFinalPoint,
        Core::RoundNearestInteger => Scene::RoundNearestInteger,
        Core::CheckCertificate => Scene::CheckCertificate,
        Core::Optimal => Scene::Optimal,
    }
}

fn randomized_almost_linear_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &RandomizedAlmostLinearMaxFlowTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let changed_nodes = event
        .before
        .nodes
        .iter()
        .zip(&event.after.nodes)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let changed_edges = event
        .before
        .edges
        .iter()
        .zip(&event.after.edges)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let publishes_active_cycle = matches!(
        event.after.stage,
        RandomizedAlmostLinearMaxFlowStage::InspectFundamentalCycle
            | RandomizedAlmostLinearMaxFlowStage::QueryMinimumRatioCycle
            | RandomizedAlmostLinearMaxFlowStage::PotentialReductionStep
    );
    let mut entity_refs =
        if event.after.stage == RandomizedAlmostLinearMaxFlowStage::InspectFeasibleAssignment {
            let edge = graph
                .edges()
                .last()
                .ok_or_else(|| JsError::new("randomized assignment focus edge missing"))?;
            vec![
                FlowTraceEntityRefSceneV1::Edge {
                    edge_id: edge.id().as_str().to_owned(),
                },
                FlowTraceEntityRefSceneV1::Node {
                    node_id: graph.nodes()[edge.from().as_usize()]
                        .id()
                        .as_str()
                        .to_owned(),
                },
                FlowTraceEntityRefSceneV1::Node {
                    node_id: graph.nodes()[edge.to().as_usize()].id().as_str().to_owned(),
                },
            ]
        } else if publishes_active_cycle {
            // The active fundamental cycle is a global typed-overlay state.
            // Its signed support remains visible without turning every edge in
            // that support into an unrelated local focus halo.
            Vec::new()
        } else {
            changed_edges
                .iter()
                .map(|index| FlowTraceEntityRefSceneV1::Edge {
                    edge_id: graph.edges()[*index].id().as_str().to_owned(),
                })
                .collect::<Vec<_>>()
        };
    if entity_refs.is_empty() && !publishes_active_cycle {
        entity_refs.extend(
            changed_nodes
                .iter()
                .map(|index| FlowTraceEntityRefSceneV1::Node {
                    node_id: graph.nodes()[*index].id().as_str().to_owned(),
                }),
        );
    }
    let (label, value) = randomized_almost_linear_event_detail(&event.after);
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: (event_id > 3).then(|| "3".to_owned()),
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: match event.after.stage {
            RandomizedAlmostLinearMaxFlowStage::InspectFundamentalCycle
            | RandomizedAlmostLinearMaxFlowStage::InspectFeasibleAssignment => {
                TraceGranularityV1::Micro
            }
            RandomizedAlmostLinearMaxFlowStage::QueryMinimumRatioCycle
            | RandomizedAlmostLinearMaxFlowStage::PotentialReductionStep
            | RandomizedAlmostLinearMaxFlowStage::DetectChangedCoordinates => {
                TraceGranularityV1::Operation
            }
            _ => TraceGranularityV1::Phase,
        },
        pseudocode_line: randomized_almost_linear_pseudocode_line(event.after.stage).to_owned(),
        patch_count: u32::try_from(changed_nodes.len() + changed_edges.len() + 1)
            .map_err(|_| JsError::new("randomized almost-linear patch count overflow"))?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn randomized_almost_linear_event_detail(
    snapshot: &RandomizedAlmostLinearMaxFlowSnapshot,
) -> (&'static str, String) {
    use RandomizedAlmostLinearMaxFlowStage as Stage;
    match snapshot.stage {
        Stage::Ready | Stage::BuildReturnEdgeReduction => {
            ("return capacity", snapshot.return_capacity.to_string())
        }
        Stage::BuildInitialPoint => ("artificial flow", snapshot.artificial_flow.decimal()),
        Stage::EnumerateForestPool => ("forest population", snapshot.forest_pool_size.to_string()),
        Stage::SampleTreeChain | Stage::RebuildTreeChain => {
            ("random draws", snapshot.random_draws.to_string())
        }
        Stage::InspectFundamentalCycle => (
            "fundamental cycle",
            snapshot.metrics.fundamental_cycles.to_string(),
        ),
        Stage::QueryMinimumRatioCycle | Stage::SamplingFailure => (
            "sampled ratio",
            snapshot.selected_ratio.map_or_else(
                || "none".to_owned(),
                flow::RandomizedAlmostLinearScalar::decimal,
            ),
        ),
        Stage::PotentialReductionStep => ("potential", snapshot.potential.decimal()),
        Stage::DetectChangedCoordinates => (
            "detected coordinates",
            snapshot.metrics.detected_coordinates.to_string(),
        ),
        Stage::InspectFeasibleAssignment => (
            "assignment",
            snapshot.metrics.enumerated_assignments.to_string(),
        ),
        Stage::EnumerateFeasibleSet => (
            "feasible circulations",
            snapshot.metrics.feasible_flows.to_string(),
        ),
        Stage::SampleIsolationCosts => {
            ("isolation attempt", snapshot.isolation_attempt.to_string())
        }
        Stage::SelectIsolatedOptimum => (
            "isolated objective",
            snapshot
                .isolated_objective
                .map_or_else(|| "none".to_owned(), |value| value.to_string()),
        ),
        Stage::ConstructFinalPoint => (
            "final-point gap",
            snapshot.final_point_gap.map_or_else(
                || "none".to_owned(),
                flow::RandomizedAlmostLinearScalar::decimal,
            ),
        ),
        Stage::RoundNearestInteger | Stage::CheckCertificate | Stage::Optimal => {
            ("maximum flow", snapshot.target_value.to_string())
        }
    }
}

const fn randomized_almost_linear_pseudocode_line(
    stage: RandomizedAlmostLinearMaxFlowStage,
) -> &'static str {
    use RandomizedAlmostLinearMaxFlowStage as Stage;
    match stage {
        Stage::Ready => "validate the bounded integral max-flow instance",
        Stage::BuildReturnEdgeReduction => "add t -> s with capacity mU and cost -1",
        Stage::BuildInitialPoint => "install midpoint flow and artificial-star balancing edges",
        Stage::EnumerateForestPool => "enumerate the bounded spanning-forest population",
        Stage::SampleTreeChain => "sample a seeded forest chain with replacement",
        Stage::InspectFundamentalCycle => "evaluate this forest fundamental cycle",
        Stage::QueryMinimumRatioCycle => "query the best sampled fundamental min-ratio cycle",
        Stage::SamplingFailure => "record a finite-population approximation miss",
        Stage::PotentialReductionStep => "apply eta <g,Delta> = -kappa^2 / 50",
        Stage::DetectChangedCoordinates => "detect slowly-changing gradient and length coordinates",
        Stage::RebuildTreeChain => "rebuild the sampled hierarchy after schedule or miss",
        Stage::InspectFeasibleAssignment => "inspect the current integral original-edge assignment",
        Stage::EnumerateFeasibleSet => {
            "enumerate integral feasible circulations in the bounded return-edge reduction"
        }
        Stage::SampleIsolationCosts => "sample independent z_e in {1,...,2MU} over scale D",
        Stage::SelectIsolatedOptimum => "select the unique optimum of the perturbed reduction",
        Stage::ConstructFinalPoint => {
            "construct a feasible point within 1 / (12 M^3 U^3) of the isolated optimum"
        }
        Stage::RoundNearestInteger => {
            "round every original and return-edge coordinate to nearest integer"
        }
        Stage::CheckCertificate => "check original flow, return flow, artificial zero, and min cut",
        Stage::Optimal => "publish certified maximum flow without claiming almost-linear runtime",
    }
}

fn apply_randomized_almost_linear_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    result: &RandomizedAlmostLinearMaxFlowResult,
) -> Result<(), JsError> {
    if result.flows != randomized_almost_linear_flows(graph, &result.final_snapshot)? {
        return Err(JsError::new(
            "randomized almost-linear result projection mismatch",
        ));
    }
    scene
        .apply_randomized_almost_linear_boundary(
            graph,
            source,
            sink,
            &result.flows,
            randomized_almost_linear_overlay(graph, &result.final_snapshot)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_randomized_almost_linear_metrics(result.metrics);
    scene
        .set_randomized_almost_linear_outcome(&result.certificate)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(())
}

fn electrical_ipm_mcf_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &ElectricalIpmMcfTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("electrical IPM MCF event count overflow"))?;
    // The public Ready scene is the only timeline base. State assembled before
    // the first source boundary becomes visible together with that boundary;
    // it must not appear as an unlabelled, precomputed frame.
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("electrical IPM MCF trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("electrical IPM MCF event identity overflow"))?;
        let mut scene = base.clone();
        scene
            .apply_electrical_ipm_mcf_boundary(
                graph,
                &electrical_ipm_mcf_flows(graph, &event.after)?,
                electrical_ipm_mcf_overlay(graph, &event.after)?,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(electrical_ipm_mcf_trace_event_scene(
            graph, event, event_id,
        )?);
        scene.set_electrical_ipm_mcf_metrics(event.after.metrics);
        if event.after.stage == ElectricalIpmMcfStage::Optimal {
            scene
                .set_electrical_ipm_mcf_outcome(graph, &run.result.certificate)
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot
        || run.final_snapshot != run.result.final_snapshot
        || run.events.is_empty()
    {
        return Err(JsError::new("electrical IPM MCF final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn electrical_ipm_mcf_fast_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    result: &ElectricalIpmMcfResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    if result.flows != electrical_ipm_mcf_flows(graph, &result.final_snapshot)? {
        return Err(JsError::new(
            "electrical IPM MCF result projection mismatch",
        ));
    }
    let mut scene = ready_flow_scene(scenario)?;
    scene
        .apply_electrical_ipm_mcf_boundary(
            graph,
            &result.flows,
            electrical_ipm_mcf_overlay(graph, &result.final_snapshot)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_electrical_ipm_mcf_metrics(result.metrics);
    scene
        .set_electrical_ipm_mcf_outcome(graph, &result.certificate)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(vec![ready_flow_scene(scenario)?, scene])
}

fn electrical_ipm_mcf_flows(
    graph: &flow::FlowNetwork,
    snapshot: &ElectricalIpmMcfSnapshot,
) -> Result<Vec<u64>, JsError> {
    if snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new("electrical IPM MCF flow shape mismatch"));
    }
    if snapshot.edges.iter().all(|edge| edge.final_flow.is_some()) {
        snapshot
            .edges
            .iter()
            .map(|edge| {
                edge.final_flow
                    .ok_or_else(|| JsError::new("electrical IPM MCF recovered flow missing"))
            })
            .collect()
    } else if snapshot.edges.iter().all(|edge| edge.final_flow.is_none()) {
        Ok(graph.edges().iter().map(flow::FlowEdge::lower).collect())
    } else {
        Err(JsError::new("electrical IPM MCF partial recovered flow"))
    }
}

fn electrical_ipm_mcf_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &ElectricalIpmMcfSnapshot,
) -> Result<FlowElectricalIpmMcfOverlayV1, JsError> {
    if snapshot.nodes.len() != graph.nodes().len() || snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new("electrical IPM MCF snapshot shape mismatch"));
    }
    let nodes = snapshot
        .nodes
        .iter()
        .zip(graph.nodes())
        .map(|(state, node)| {
            if graph.node(state.node) != Some(node) {
                return Err(JsError::new("electrical IPM MCF node identity mismatch"));
            }
            Ok(FlowElectricalIpmMcfNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                potential: state.potential.decimal(),
                potential_direction: state.potential_direction.decimal(),
                balance_residual: state.balance_residual.decimal(),
                anchored: state.anchored,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let edges = snapshot
        .edges
        .iter()
        .zip(graph.edges())
        .map(|(state, edge)| {
            if &state.edge != edge.id() {
                return Err(JsError::new("electrical IPM MCF edge identity mismatch"));
            }
            Ok(FlowElectricalIpmMcfEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                perturbation: state.perturbation.to_string(),
                isolated_cost: state.isolated_cost.to_string(),
                fixed_on_face: state.fixed_on_face,
                face_lower: state.face_lower.to_string(),
                face_upper: state.face_upper.to_string(),
                fractional_flow: state.fractional_flow.decimal(),
                upper_complement: state.upper_complement.decimal(),
                lower_slack: state.lower_slack.decimal(),
                upper_multiplier: state.upper_multiplier.decimal(),
                resistance: state.resistance.decimal(),
                conductance: state.conductance.decimal(),
                electrical_current: state.electrical_current.decimal(),
                lower_slack_direction: state.lower_slack_direction.decimal(),
                upper_multiplier_direction: state.upper_multiplier_direction.decimal(),
                final_flow: state.final_flow.map(|flow| flow.to_string()),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowElectricalIpmMcfOverlayV1 {
        stage: electrical_ipm_mcf_scene_stage(snapshot.stage),
        seed: snapshot.seed.to_string(),
        mu: snapshot.mu.decimal(),
        epsilon_3: snapshot.epsilon_3.decimal(),
        recovery_epsilon: snapshot.recovery_epsilon.decimal(),
        duality_gap_bound: snapshot.duality_gap_bound.decimal(),
        centrality_residual: snapshot.centrality_residual.decimal(),
        balance_residual: snapshot.balance_residual.decimal(),
        step_size: snapshot.step_size.decimal(),
        electrical_energy: snapshot.electrical_energy.decimal(),
        linear_residual: snapshot.linear_residual.decimal(),
        barrier_objective: snapshot.barrier_objective.decimal(),
        isolation_scale: snapshot.isolation_scale.to_string(),
        perturbation_bound: snapshot.perturbation_bound.to_string(),
        isolation_attempt: snapshot.isolation_attempt.to_string(),
        isolated_optimum_cost: snapshot.isolated_optimum_cost.to_string(),
        isolated_gap: snapshot.isolated_gap.to_string(),
        nodes,
        edges,
    })
}

const fn electrical_ipm_mcf_scene_stage(
    stage: ElectricalIpmMcfStage,
) -> FlowElectricalIpmMcfStageV1 {
    use ElectricalIpmMcfStage as Core;
    use FlowElectricalIpmMcfStageV1 as Scene;
    match stage {
        Core::Ready => Scene::Ready,
        Core::NormalizeLowerBounds => Scene::NormalizeLowerBounds,
        Core::IsolationAttempt => Scene::IsolationAttempt,
        Core::SelectIsolatedCosts => Scene::SelectIsolatedCosts,
        Core::ContractFixedFace => Scene::ContractFixedFace,
        Core::InitializeDualInterior => Scene::InitializeDualInterior,
        Core::AssembleElectricalLaplacian => Scene::AssembleElectricalLaplacian,
        Core::SolveNewtonDirection => Scene::SolveNewtonDirection,
        Core::DampedCenteringStep => Scene::DampedCenteringStep,
        Core::Centered => Scene::Centered,
        Core::DecreaseBarrier => Scene::DecreaseBarrier,
        Core::ApproximateFlow => Scene::ApproximateFlow,
        Core::RoundNearestInteger => Scene::RoundNearestInteger,
        Core::CheckCertificate => Scene::CheckCertificate,
        Core::Optimal => Scene::Optimal,
    }
}

fn electrical_ipm_mcf_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &ElectricalIpmMcfTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let changed_nodes = event
        .after
        .nodes
        .iter()
        .enumerate()
        .filter(|(index, after)| event.before.nodes.get(*index) != Some(*after))
        .collect::<Vec<_>>();
    let changed_edges = event
        .after
        .edges
        .iter()
        .enumerate()
        .filter(|(index, after)| event.before.edges.get(*index) != Some(*after))
        .collect::<Vec<_>>();
    let mut entity_refs = Vec::with_capacity(2);
    if let Some((_, edge)) = changed_edges.iter().copied().max_by(|left, right| {
        electrical_ipm_mcf_edge_focus_order(event.after.stage, left.1, right.1)
            .then_with(|| right.0.cmp(&left.0))
    }) {
        entity_refs.push(FlowTraceEntityRefSceneV1::Edge {
            edge_id: edge.edge.as_str().to_owned(),
        });
    }
    if let Some((_, node)) = changed_nodes.iter().copied().max_by(|left, right| {
        electrical_ipm_mcf_node_focus_score(left.1)
            .total_cmp(&electrical_ipm_mcf_node_focus_score(right.1))
            .then_with(|| right.0.cmp(&left.0))
    }) {
        let node = graph
            .node(node.node)
            .ok_or_else(|| JsError::new("electrical IPM changed node out of range"))?;
        entity_refs.push(FlowTraceEntityRefSceneV1::Node {
            node_id: node.id().as_str().to_owned(),
        });
    }
    let patch_count = u32::try_from(changed_nodes.len() + changed_edges.len() + 1)
        .map_err(|_| JsError::new("electrical IPM patch count overflow"))?;
    let (label, value) = electrical_ipm_mcf_event_detail(&event.after);
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: None,
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: if matches!(
            event.after.stage,
            ElectricalIpmMcfStage::AssembleElectricalLaplacian
                | ElectricalIpmMcfStage::SolveNewtonDirection
                | ElectricalIpmMcfStage::DampedCenteringStep
                | ElectricalIpmMcfStage::DecreaseBarrier
        ) {
            TraceGranularityV1::Operation
        } else {
            TraceGranularityV1::Phase
        },
        pseudocode_line: electrical_ipm_mcf_pseudocode_line(event.after.stage).to_owned(),
        patch_count,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn electrical_ipm_mcf_edge_focus_order(
    stage: ElectricalIpmMcfStage,
    left: &flow::ElectricalIpmMcfEdgeState,
    right: &flow::ElectricalIpmMcfEdgeState,
) -> std::cmp::Ordering {
    match stage {
        ElectricalIpmMcfStage::IsolationAttempt | ElectricalIpmMcfStage::SelectIsolatedCosts => {
            left.perturbation.cmp(&right.perturbation)
        }
        ElectricalIpmMcfStage::ContractFixedFace => (
            left.fixed_on_face,
            left.face_upper.saturating_sub(left.face_lower),
        )
            .cmp(&(
                right.fixed_on_face,
                right.face_upper.saturating_sub(right.face_lower),
            )),
        ElectricalIpmMcfStage::RoundNearestInteger
        | ElectricalIpmMcfStage::CheckCertificate
        | ElectricalIpmMcfStage::Optimal => left.final_flow.cmp(&right.final_flow),
        _ => (left.electrical_current.get().abs()
            + left.lower_slack_direction.get().abs()
            + left.upper_multiplier_direction.get().abs())
        .total_cmp(
            &(right.electrical_current.get().abs()
                + right.lower_slack_direction.get().abs()
                + right.upper_multiplier_direction.get().abs()),
        ),
    }
}

fn electrical_ipm_mcf_node_focus_score(node: &flow::ElectricalIpmMcfNodeState) -> f64 {
    node.potential_direction.get().abs() + node.balance_residual.get().abs()
}

fn electrical_ipm_mcf_event_detail(snapshot: &ElectricalIpmMcfSnapshot) -> (&'static str, String) {
    use ElectricalIpmMcfStage as Stage;
    match snapshot.stage {
        Stage::Ready | Stage::NormalizeLowerBounds => (
            "enumerated assignments",
            snapshot.metrics.enumerated_assignments.to_string(),
        ),
        Stage::IsolationAttempt => ("isolation attempt", snapshot.isolation_attempt.to_string()),
        Stage::SelectIsolatedCosts => ("isolated gap", snapshot.isolated_gap.to_string()),
        Stage::ContractFixedFace => (
            "fixed coordinates",
            snapshot.metrics.fixed_coordinates.to_string(),
        ),
        Stage::InitializeDualInterior
        | Stage::Centered
        | Stage::DecreaseBarrier
        | Stage::ApproximateFlow => ("mu", snapshot.mu.decimal()),
        Stage::AssembleElectricalLaplacian | Stage::SolveNewtonDirection => {
            ("electrical energy", snapshot.electrical_energy.decimal())
        }
        Stage::DampedCenteringStep => (
            "centrality residual",
            snapshot.centrality_residual.decimal(),
        ),
        Stage::RoundNearestInteger => (
            "rounded coordinates",
            snapshot.metrics.rounding_operations.to_string(),
        ),
        Stage::CheckCertificate | Stage::Optimal => (
            "certificate checks",
            snapshot.metrics.certificate_checks.to_string(),
        ),
    }
}

const fn electrical_ipm_mcf_pseudocode_line(stage: ElectricalIpmMcfStage) -> &'static str {
    use ElectricalIpmMcfStage as Stage;
    match stage {
        Stage::Ready => "electrical-ipm:validate-bounded-standard-mcf",
        Stage::NormalizeLowerBounds => "electrical-ipm:enumerate-affine-integer-feasible-set",
        Stage::IsolationAttempt => "electrical-ipm:draw-independent-edge-perturbations",
        Stage::SelectIsolatedCosts => "electrical-ipm:verify-unique-perturbed-optimum",
        Stage::ContractFixedFace => "electrical-ipm:contract-fixed-feasible-face-coordinates",
        Stage::InitializeDualInterior => "electrical-ipm:initialize-positive-dual-slacks",
        Stage::AssembleElectricalLaplacian => "electrical-ipm:form-schur-complement-laplacian",
        Stage::SolveNewtonDirection => "electrical-ipm:solve-anchored-electrical-newton-system",
        Stage::DampedCenteringStep => {
            "electrical-ipm:assemble-solve-and-accept-newton-centering-iteration"
        }
        Stage::Centered => "electrical-ipm:check-central-neighborhood",
        Stage::DecreaseBarrier => "electrical-ipm:mu-times-one-minus-epsilon-three",
        Stage::ApproximateFlow => "electrical-ipm:check-standard-mcf-recovery-error",
        Stage::RoundNearestInteger => "electrical-ipm:round-each-flow-coordinate",
        Stage::CheckCertificate => "electrical-ipm:check-isolation-rounding-and-original-mcf",
        Stage::Optimal => "electrical-ipm:publish-certified-optimum",
    }
}

fn primal_dual_ipm_mcf_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &PrimalDualIpmTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("primal-dual IPM event count overflow"))?;
    // Publish all pre-iteration construction at the first owned source event,
    // rather than replacing Ready with a hidden internal snapshot.
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }

    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("primal-dual IPM trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("primal-dual IPM event identity overflow"))?;
        let mut scene = base.clone();
        scene
            .apply_primal_dual_ipm_mcf_boundary(
                graph,
                &primal_dual_ipm_mcf_flows(graph, &event.after)?,
                primal_dual_ipm_mcf_overlay(graph, &event.after, run.result.seed)?,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(primal_dual_ipm_mcf_trace_event_scene(
            graph, event, event_id,
        )?);
        scene.set_primal_dual_ipm_mcf_metrics(event.after.metrics);
        if event.after.stage == PrimalDualIpmStage::Optimal {
            scene
                .set_primal_dual_ipm_mcf_outcome(graph, &run.result.certificate)
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot
        || run.final_snapshot != run.result.final_snapshot
        || run.events.is_empty()
    {
        return Err(JsError::new("primal-dual IPM final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn primal_dual_ipm_mcf_fast_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    result: &PrimalDualIpmResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    if result.flows != primal_dual_ipm_mcf_flows(graph, &result.final_snapshot)? {
        return Err(JsError::new("primal-dual IPM result projection mismatch"));
    }
    let mut scene = ready_flow_scene(scenario)?;
    scene
        .apply_primal_dual_ipm_mcf_boundary(
            graph,
            &result.flows,
            primal_dual_ipm_mcf_overlay(graph, &result.final_snapshot, result.seed)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_primal_dual_ipm_mcf_metrics(result.metrics);
    scene
        .set_primal_dual_ipm_mcf_outcome(graph, &result.certificate)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(vec![ready_flow_scene(scenario)?, scene])
}

fn primal_dual_ipm_mcf_flows(
    graph: &flow::FlowNetwork,
    snapshot: &PrimalDualIpmSnapshot,
) -> Result<Vec<u64>, JsError> {
    if snapshot.final_flows.len() != graph.edges().len() {
        return Err(JsError::new("primal-dual IPM flow shape mismatch"));
    }
    if snapshot.final_flows.iter().all(Option::is_some) {
        snapshot
            .final_flows
            .iter()
            .map(|flow| flow.ok_or_else(|| JsError::new("primal-dual IPM recovered flow missing")))
            .collect()
    } else if snapshot.final_flows.iter().all(Option::is_none) {
        Ok(graph.edges().iter().map(flow::FlowEdge::lower).collect())
    } else {
        Err(JsError::new("primal-dual IPM partial recovered flow"))
    }
}

fn primal_dual_ipm_mcf_auxiliary_node_ids(
    graph: &flow::FlowNetwork,
    snapshot: &PrimalDualIpmSnapshot,
) -> Result<Vec<String>, JsError> {
    snapshot
        .nodes
        .iter()
        .enumerate()
        .map(|(ordinal, state)| {
            if state.node != ordinal {
                return Err(JsError::new("primal-dual IPM node ordinal mismatch"));
            }
            match &state.kind {
                PrimalDualIpmNodeKind::Original(node) => graph
                    .nodes()
                    .get(node.as_usize())
                    .map(|node| format!("node:{}", node.id().as_str()))
                    .ok_or_else(|| JsError::new("primal-dual IPM original node out of range")),
                PrimalDualIpmNodeKind::Capacity(edge) => graph
                    .edge_index(edge)
                    .map(|_| format!("capacity:{}", edge.as_str()))
                    .ok_or_else(|| JsError::new("primal-dual IPM capacity edge is unknown")),
            }
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
fn primal_dual_ipm_mcf_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &PrimalDualIpmSnapshot,
    seed: u64,
) -> Result<FlowPrimalDualIpmMcfOverlayV1, JsError> {
    if snapshot.final_flows.len() != graph.edges().len() {
        return Err(JsError::new("primal-dual IPM snapshot shape mismatch"));
    }
    let auxiliary_node_ids = primal_dual_ipm_mcf_auxiliary_node_ids(graph, snapshot)?;
    let nodes = snapshot
        .nodes
        .iter()
        .map(|state| {
            let (kind, original_node_id, original_edge_id) = match &state.kind {
                PrimalDualIpmNodeKind::Original(node) => {
                    let node_id = graph
                        .nodes()
                        .get(node.as_usize())
                        .ok_or_else(|| JsError::new("primal-dual IPM original node out of range"))?
                        .id()
                        .as_str()
                        .to_owned();
                    (
                        FlowPrimalDualIpmMcfNodeKindV1::Original,
                        Some(node_id),
                        None,
                    )
                }
                PrimalDualIpmNodeKind::Capacity(edge) => {
                    if graph.edge_index(edge).is_none() {
                        return Err(JsError::new("primal-dual IPM capacity edge is unknown"));
                    }
                    (
                        FlowPrimalDualIpmMcfNodeKindV1::Capacity,
                        None,
                        Some(edge.as_str().to_owned()),
                    )
                }
            };
            Ok(FlowPrimalDualIpmMcfNodeStateV1 {
                auxiliary_id: auxiliary_node_ids[state.node].clone(),
                kind,
                original_node_id,
                original_edge_id,
                potential: state.potential.to_string(),
                component: state.component.to_string(),
                in_crossover_set: state.in_crossover_set,
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let arcs = snapshot
        .arcs
        .iter()
        .enumerate()
        .map(|(ordinal, state)| {
            if state.arc != ordinal || graph.edge_index(&state.original_edge).is_none() {
                return Err(JsError::new("primal-dual IPM arc identity mismatch"));
            }
            let from = auxiliary_node_ids
                .get(state.from)
                .cloned()
                .ok_or_else(|| JsError::new("primal-dual IPM arc tail out of range"))?;
            let to = auxiliary_node_ids
                .get(state.to)
                .cloned()
                .ok_or_else(|| JsError::new("primal-dual IPM arc head out of range"))?;
            Ok(FlowPrimalDualIpmMcfArcStateV1 {
                auxiliary_id: format!("aux:{ordinal}"),
                original_edge_id: state.original_edge.as_str().to_owned(),
                from,
                to,
                kind: match state.kind {
                    PrimalDualIpmArcKind::Upper => FlowPrimalDualIpmMcfArcKindV1::Upper,
                    PrimalDualIpmArcKind::Lower => FlowPrimalDualIpmMcfArcKindV1::Lower,
                    PrimalDualIpmArcKind::Artificial => FlowPrimalDualIpmMcfArcKindV1::Artificial,
                },
                flow: state.flow.to_string(),
                slack: state.slack.to_string(),
                resistance: state.resistance.as_ref().map(ToString::to_string),
                deleted: state.deleted,
                contracted: state.contracted,
                in_minor: state.in_minor,
                in_tree: state.in_tree,
                forest_candidate: snapshot.forest_candidate_arcs.contains(&state.arc),
                active_cycle_sign: state.active_cycle_sign.to_string(),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowPrimalDualIpmMcfOverlayV1 {
        stage: primal_dual_ipm_mcf_scene_stage(snapshot.stage),
        seed: seed.to_string(),
        mu: snapshot.mu.to_string(),
        beta: snapshot.beta.to_string(),
        gamma: snapshot.gamma.to_string(),
        proxy_gap: snapshot.proxy_gap.to_string(),
        centrality_numerator: snapshot.centrality_numerator.to_string(),
        sampled_arc: snapshot.sampled_arc.map(|arc| format!("aux:{arc}")),
        cycle_alpha: snapshot.cycle_alpha.to_string(),
        tree_condition_number: snapshot.tree_condition_number.as_ref().map(|ratio| {
            FlowPrimalDualIpmMcfRatioV1 {
                numerator: ratio.numer().to_string(),
                denominator: ratio.denom().to_string(),
            }
        }),
        forest_subset_serial: (snapshot.stage == PrimalDualIpmStage::InspectForestSubset)
            .then(|| snapshot.metrics.forest_subsets.to_string()),
        nodes,
        arcs,
    })
}

const fn primal_dual_ipm_mcf_scene_stage(stage: PrimalDualIpmStage) -> FlowPrimalDualIpmMcfStageV1 {
    use FlowPrimalDualIpmMcfStageV1 as Scene;
    use PrimalDualIpmStage as Core;
    match stage {
        Core::Ready => Scene::Ready,
        Core::NormalizeInput => Scene::NormalizeInput,
        Core::BuildCapacityReduction => Scene::BuildCapacityReduction,
        Core::InitializeCentralPoint => Scene::InitializeCentralPoint,
        Core::BuildMinor => Scene::BuildMinor,
        Core::DecreaseMu => Scene::DecreaseMu,
        Core::InspectForestSubset => Scene::InspectForestSubset,
        Core::BuildLowStretchForest => Scene::BuildLowStretchForest,
        Core::SampleFundamentalCycle => Scene::SampleFundamentalCycle,
        Core::CenteringCycleUpdate => Scene::CenteringCycleUpdate,
        Core::Centered => Scene::Centered,
        Core::ProxyReached => Scene::ProxyReached,
        Core::CrossoverGrowCut => Scene::CrossoverGrowCut,
        Core::RestoreOriginalDual => Scene::RestoreOriginalDual,
        Core::RecoverAdmissibleFlow => Scene::RecoverAdmissibleFlow,
        Core::CheckCertificate => Scene::CheckCertificate,
        Core::Optimal => Scene::Optimal,
    }
}

fn primal_dual_ipm_mcf_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &PrimalDualIpmTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let mut nodes = BTreeSet::new();
    let mut edges = BTreeSet::new();
    for (index, after) in event.after.nodes.iter().enumerate() {
        if event.before.nodes.get(index) != Some(after) {
            match &after.kind {
                PrimalDualIpmNodeKind::Original(node) => {
                    let node_id = graph
                        .nodes()
                        .get(node.as_usize())
                        .ok_or_else(|| JsError::new("primal-dual IPM changed node out of range"))?;
                    nodes.insert(node_id.id().as_str().to_owned());
                }
                PrimalDualIpmNodeKind::Capacity(edge) => {
                    edges.insert(edge.as_str().to_owned());
                }
            }
        }
    }
    for (index, after) in event.after.arcs.iter().enumerate() {
        if event.before.arcs.get(index) != Some(after) {
            edges.insert(after.original_edge.as_str().to_owned());
        }
    }
    let mut entity_refs = edges
        .into_iter()
        .map(|edge_id| FlowTraceEntityRefSceneV1::Edge { edge_id })
        .collect::<Vec<_>>();
    entity_refs.extend(
        nodes
            .into_iter()
            .map(|node_id| FlowTraceEntityRefSceneV1::Node { node_id }),
    );
    if matches!(
        event.after.stage,
        PrimalDualIpmStage::InspectForestSubset
            | PrimalDualIpmStage::SampleFundamentalCycle
            | PrimalDualIpmStage::CenteringCycleUpdate
    ) {
        // These operations target auxiliary arcs. Projecting them to original
        // edges merges upper/lower/artificial identities and can highlight the
        // whole physical graph. The dedicated auxiliary overlay publishes the
        // exact subset/cycle, so generic focus must remain empty here.
        entity_refs.clear();
    }
    let changed_nodes = event
        .after
        .nodes
        .iter()
        .enumerate()
        .filter(|(index, after)| event.before.nodes.get(*index) != Some(*after))
        .count();
    let changed_arcs = event
        .after
        .arcs
        .iter()
        .enumerate()
        .filter(|(index, after)| event.before.arcs.get(*index) != Some(*after))
        .count();
    let patch_count = u32::try_from(changed_nodes + changed_arcs + 1)
        .map_err(|_| JsError::new("primal-dual IPM patch count overflow"))?;
    let (label, value) = primal_dual_ipm_mcf_event_detail(&event.after);
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: None,
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: if matches!(
            event.after.stage,
            PrimalDualIpmStage::InspectForestSubset
                | PrimalDualIpmStage::SampleFundamentalCycle
                | PrimalDualIpmStage::CenteringCycleUpdate
                | PrimalDualIpmStage::CrossoverGrowCut
        ) {
            if event.after.stage == PrimalDualIpmStage::InspectForestSubset {
                TraceGranularityV1::Micro
            } else {
                TraceGranularityV1::Operation
            }
        } else {
            TraceGranularityV1::Phase
        },
        pseudocode_line: primal_dual_ipm_mcf_pseudocode_line(event.after.stage).to_owned(),
        patch_count,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn primal_dual_ipm_mcf_event_detail(snapshot: &PrimalDualIpmSnapshot) -> (&'static str, String) {
    use PrimalDualIpmStage as Stage;
    match snapshot.stage {
        Stage::Ready | Stage::NormalizeInput => ("auxiliary arcs", snapshot.arcs.len().to_string()),
        Stage::BuildCapacityReduction | Stage::InitializeCentralPoint => {
            ("mu", snapshot.mu.to_string())
        }
        Stage::BuildMinor => (
            "minor arcs",
            snapshot
                .arcs
                .iter()
                .filter(|arc| arc.in_minor)
                .count()
                .to_string(),
        ),
        Stage::DecreaseMu | Stage::Centered => ("mu", snapshot.mu.to_string()),
        Stage::InspectForestSubset => (
            "candidate subset arcs",
            snapshot.forest_candidate_arcs.len().to_string(),
        ),
        Stage::BuildLowStretchForest => (
            "forest subsets",
            snapshot.metrics.forest_subsets.to_string(),
        ),
        Stage::SampleFundamentalCycle => (
            "sampled arc",
            snapshot
                .sampled_arc
                .map_or_else(|| "0".to_owned(), |arc| arc.to_string()),
        ),
        Stage::CenteringCycleUpdate => ("cycle alpha", snapshot.cycle_alpha.to_string()),
        Stage::ProxyReached => ("proxy gap", snapshot.proxy_gap.to_string()),
        Stage::CrossoverGrowCut | Stage::RestoreOriginalDual => (
            "crossover shifts",
            snapshot.metrics.crossover_shifts.to_string(),
        ),
        Stage::RecoverAdmissibleFlow => (
            "recovery augmentations",
            snapshot.metrics.recovery_augmentations.to_string(),
        ),
        Stage::CheckCertificate | Stage::Optimal => (
            "certificate checks",
            snapshot.metrics.certificate_checks.to_string(),
        ),
    }
}

const fn primal_dual_ipm_mcf_pseudocode_line(stage: PrimalDualIpmStage) -> &'static str {
    use PrimalDualIpmStage as Stage;
    match stage {
        Stage::Ready => "integer-ipm:validate-bounded-integral-instance",
        Stage::NormalizeInput => "integer-ipm:normalize-lowers-signs-and-common-gcd",
        Stage::BuildCapacityReduction => "integer-ipm:replace-each-capacity-by-up-down-arcs",
        Stage::InitializeCentralPoint => "integer-ipm:appendix-a-integer-central-initialization",
        Stage::BuildMinor => "integer-ipm:sticky-delete-x-small-contract-s-small",
        Stage::DecreaseMu => "integer-ipm:mu-ceil-one-minus-delta-over-root-m",
        Stage::InspectForestSubset => {
            "integer-ipm:inspect-one-exact-minimum-condition-forest-subset"
        }
        Stage::BuildLowStretchForest => "integer-ipm:select-minimum-condition-spanning-forest",
        Stage::SampleFundamentalCycle => "integer-ipm:sample-fundamental-cycle-by-resistance",
        Stage::CenteringCycleUpdate => "integer-ipm:apply-rounded-cycle-correction",
        Stage::Centered => "integer-ipm:check-one-norm-centrality",
        Stage::ProxyReached => "integer-ipm:check-active-minor-proxy-gap",
        Stage::CrossoverGrowCut => "integer-ipm:grow-nested-zero-reduced-cost-cut",
        Stage::RestoreOriginalDual => "integer-ipm:rebuild-original-cost-tree-potentials",
        Stage::RecoverAdmissibleFlow => "integer-ipm:max-flow-on-zero-reduced-cost-network",
        Stage::CheckCertificate => "integer-ipm:independent-exact-mcf-check",
        Stage::Optimal => "integer-ipm:publish-certified-optimum",
    }
}

fn deterministic_almost_linear_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    run: &DeterministicAlmostLinearMaxFlowTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("deterministic almost-linear event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new(
                "deterministic almost-linear trace discontinuity",
            ));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("deterministic almost-linear event identity overflow"))?;
        let mut scene = base.clone();
        let flows = deterministic_almost_linear_flows(graph, &event.after)?;
        scene
            .apply_deterministic_almost_linear_boundary(
                graph,
                source,
                sink,
                &flows,
                deterministic_almost_linear_overlay(graph, &event.after)?,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(deterministic_almost_linear_trace_event_scene(
            graph, event, event_id,
        )?);
        scene.set_deterministic_almost_linear_metrics(event.after.metrics);
        if index + 1 == run.events.len() {
            scene
                .set_deterministic_almost_linear_outcome(&run.result.certificate)
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot
        || run.final_snapshot != run.result.final_snapshot
        || run.events.is_empty()
    {
        return Err(JsError::new(
            "deterministic almost-linear final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn deterministic_almost_linear_flows(
    graph: &flow::FlowNetwork,
    snapshot: &DeterministicAlmostLinearMaxFlowSnapshot,
) -> Result<Vec<u64>, JsError> {
    if snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new(
            "deterministic almost-linear snapshot shape mismatch",
        ));
    }
    if snapshot.edges.iter().all(|edge| edge.final_flow.is_some()) {
        snapshot
            .edges
            .iter()
            .map(|edge| {
                edge.final_flow
                    .ok_or_else(|| JsError::new("deterministic almost-linear rounded flow missing"))
            })
            .collect()
    } else if snapshot.edges.iter().all(|edge| edge.final_flow.is_none()) {
        Ok(vec![0; graph.edges().len()])
    } else {
        Err(JsError::new(
            "deterministic almost-linear partial rounded flow",
        ))
    }
}

#[allow(clippy::too_many_lines)]
fn deterministic_almost_linear_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &DeterministicAlmostLinearMaxFlowSnapshot,
) -> Result<FlowDeterministicAlmostLinearOverlayV1, JsError> {
    if snapshot.nodes.len() != graph.nodes().len() || snapshot.edges.len() != graph.edges().len() {
        return Err(JsError::new(
            "deterministic almost-linear snapshot shape mismatch",
        ));
    }
    let nodes = graph
        .nodes()
        .iter()
        .zip(&snapshot.nodes)
        .map(|(node, state)| {
            if state.node.as_usize() >= graph.nodes().len()
                || graph.nodes()[state.node.as_usize()].id() != node.id()
            {
                return Err(JsError::new(
                    "deterministic almost-linear node identity mismatch",
                ));
            }
            let parent = state
                .tree_parent
                .map(|parent| {
                    if parent == graph.nodes().len() {
                        Ok("__artificial_star__".to_owned())
                    } else {
                        graph
                            .nodes()
                            .get(parent)
                            .map(|parent_node| parent_node.id().as_str().to_owned())
                            .ok_or_else(|| {
                                JsError::new("deterministic almost-linear parent out of range")
                            })
                    }
                })
                .transpose()?;
            Ok(FlowDeterministicAlmostLinearNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                tree_parent_node_id: parent,
                forest_component: state.tree_component.to_string(),
                source_side: state.source_side,
                artificial_direction: state.artificial_direction.to_string(),
                artificial_flow: state.artificial_flow.decimal(),
                artificial_capacity: state.artificial_capacity.decimal(),
                artificial_tree_level_mask: state.artificial_tree_level_mask.to_string(),
                active_artificial_tree_edge: state.active_artificial_tree_edge,
                active_artificial_sign: state.active_artificial_sign.to_string(),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let edges = graph
        .edges()
        .iter()
        .zip(&snapshot.edges)
        .map(|(edge, state)| {
            if state.edge != *edge.id() {
                return Err(JsError::new(
                    "deterministic almost-linear edge identity mismatch",
                ));
            }
            Ok(FlowDeterministicAlmostLinearEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                interior_flow: state.interior_flow.decimal(),
                gradient: state.gradient.decimal(),
                length: state.length.decimal(),
                tree_level_mask: state.tree_level_mask.to_string(),
                forest_level_mask: state.forest_level_mask.to_string(),
                active_tree_edge: state.active_tree_edge,
                active_core_edge: state.active_core_edge,
                active_spanner_edge: state.active_spanner_edge,
                embedding_hops: state.embedding_hops.to_string(),
                embedding_stretch: state.embedding_stretch.decimal(),
                active_cycle_sign: state.active_cycle_sign.to_string(),
                changed_coordinate: state.changed_coordinate,
                final_point_flow: state.final_point_flow.as_ref().map(big_rational_scene),
                rounding_flow: state.rounding_flow.as_ref().map(big_rational_scene),
                rounding_forest_edge: state.rounding_forest_edge,
                rounding_cycle_sign: state.rounding_cycle_sign.to_string(),
                final_flow: state.final_flow.map(|flow| flow.to_string()),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowDeterministicAlmostLinearOverlayV1 {
        stage: deterministic_almost_linear_scene_stage(snapshot.stage),
        alpha: snapshot.alpha.decimal(),
        potential: snapshot.potential.decimal(),
        cost_gap: snapshot.cost_gap.decimal(),
        selected_ratio: snapshot
            .selected_ratio
            .map(flow::DeterministicAlmostLinearScalar::decimal),
        exact_pool_ratio: snapshot
            .exact_pool_ratio
            .map(flow::DeterministicAlmostLinearScalar::decimal),
        selected_off_tree_edge: snapshot.selected_off_tree_edge.map(|edge| edge.to_string()),
        selected_cycle_kind: snapshot.selected_cycle_kind.map(|kind| match kind {
            DeterministicAlmostLinearCycleKind::Tree => {
                FlowDeterministicAlmostLinearCycleKindV1::Tree
            }
            DeterministicAlmostLinearCycleKind::Spanner => {
                FlowDeterministicAlmostLinearCycleKindV1::Spanner
            }
        }),
        forest_pool_size: snapshot.forest_pool_size.to_string(),
        level_count: snapshot.level_count.to_string(),
        branch_count: snapshot.branch_count.to_string(),
        built_branch_records: snapshot.metrics.branch_records.to_string(),
        active_branches: snapshot
            .active_branches
            .iter()
            .map(u64::to_string)
            .collect(),
        passes: snapshot.passes.iter().map(u64::to_string).collect(),
        active_level: snapshot.active_level.map(|level| level.to_string()),
        fundamental_cycles: snapshot.metrics.fundamental_cycles.to_string(),
        core_vertices: snapshot.core_vertices.to_string(),
        core_edges: snapshot.core_edges.to_string(),
        spanner_edges: snapshot.spanner_edges.to_string(),
        embedding_hops: snapshot.embedding_hops.to_string(),
        iteration: snapshot.iteration.to_string(),
        rebuild_epoch: snapshot.rebuild_epoch.to_string(),
        return_flow: snapshot.return_flow.decimal(),
        return_capacity: snapshot.return_capacity.to_string(),
        return_gradient: snapshot.return_gradient.decimal(),
        return_length: snapshot.return_length.decimal(),
        return_tree_level_mask: snapshot.return_tree_level_mask.to_string(),
        active_return_tree_edge: snapshot.active_return_tree_edge,
        active_return_sign: snapshot.active_return_sign.to_string(),
        final_point_return_flow: snapshot
            .final_point_return_flow
            .as_ref()
            .map(big_rational_scene),
        rounding_return_flow: snapshot
            .rounding_return_flow
            .as_ref()
            .map(big_rational_scene),
        rounding_return_forest_edge: snapshot.rounding_return_forest_edge,
        rounding_return_sign: snapshot.rounding_return_sign.to_string(),
        final_return_flow: snapshot.final_return_flow.map(|flow| flow.to_string()),
        artificial_edges: snapshot.artificial_edges.to_string(),
        artificial_flow: snapshot.artificial_flow.decimal(),
        final_artificial_flow: snapshot.final_artificial_flow.map(|flow| flow.to_string()),
        final_point_gap: snapshot.final_point_gap.as_ref().map(big_rational_scene),
        final_point_threshold: big_rational_scene(&snapshot.final_point_threshold),
        final_point_mix: snapshot.final_point_mix.as_ref().map(big_rational_scene),
        rounding_processed_edge: snapshot
            .rounding_processed_edge
            .as_ref()
            .map(|edge| edge.as_str().to_owned()),
        target_value: snapshot.target_value.to_string(),
        nodes,
        edges,
    })
}

const fn deterministic_almost_linear_scene_stage(
    stage: DeterministicAlmostLinearMaxFlowStage,
) -> FlowDeterministicAlmostLinearStageV1 {
    use DeterministicAlmostLinearMaxFlowStage as Core;
    use FlowDeterministicAlmostLinearStageV1 as Scene;
    match stage {
        Core::Ready => Scene::Ready,
        Core::BuildReturnEdgeReduction => Scene::BuildReturnEdgeReduction,
        Core::BuildInitialPoint => Scene::BuildInitialPoint,
        Core::EnumerateForestPool => Scene::EnumerateForestPool,
        Core::InstallBranchRecord => Scene::InstallBranchRecord,
        Core::BuildBranchCollection => Scene::BuildBranchCollection,
        Core::BuildCoreGraph => Scene::BuildCoreGraph,
        Core::BuildSpannerEmbedding => Scene::BuildSpannerEmbedding,
        Core::InspectFundamentalCycle => Scene::InspectFundamentalCycle,
        Core::QueryMinimumRatioCycle => Scene::QueryMinimumRatioCycle,
        Core::QueryFailure => Scene::QueryFailure,
        Core::ShiftBranch => Scene::ShiftBranch,
        Core::RebuildDeeperLevels => Scene::RebuildDeeperLevels,
        Core::PotentialReductionStep => Scene::PotentialReductionStep,
        Core::DetectChangedCoordinates => Scene::DetectChangedCoordinates,
        Core::ScheduledRebuild => Scene::ScheduledRebuild,
        Core::EnumerateFeasibleSet => Scene::EnumerateFeasibleSet,
        Core::ConstructFinalPoint => Scene::ConstructFinalPoint,
        Core::RoundingIntegralEdge => Scene::RoundingIntegralEdge,
        Core::RoundingLinkFractionalEdge => Scene::RoundingLinkFractionalEdge,
        Core::RoundingCancelFractionalCycle => Scene::RoundingCancelFractionalCycle,
        Core::FinishFlowRounding => Scene::FinishFlowRounding,
        Core::CheckCertificate => Scene::CheckCertificate,
        Core::Optimal => Scene::Optimal,
    }
}

fn deterministic_almost_linear_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &DeterministicAlmostLinearMaxFlowTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let changed_nodes = event
        .before
        .nodes
        .iter()
        .zip(&event.after.nodes)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let changed_edges = event
        .before
        .edges
        .iter()
        .zip(&event.after.edges)
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect::<Vec<_>>();
    let publishes_active_cycle = matches!(
        event.after.stage,
        DeterministicAlmostLinearMaxFlowStage::InspectFundamentalCycle
            | DeterministicAlmostLinearMaxFlowStage::QueryMinimumRatioCycle
            | DeterministicAlmostLinearMaxFlowStage::PotentialReductionStep
    );
    let publishes_branch_geometry =
        event.after.stage == DeterministicAlmostLinearMaxFlowStage::InstallBranchRecord;
    let mut entity_refs = if publishes_active_cycle || publishes_branch_geometry {
        // Cycle membership, orientation, and the closing work-edge ordinal are
        // already explicit in the deterministic typed overlay. Do not render
        // the whole cycle or branch geometry as local Detail targets.
        Vec::new()
    } else {
        changed_edges
            .iter()
            .map(|index| FlowTraceEntityRefSceneV1::Edge {
                edge_id: graph.edges()[*index].id().as_str().to_owned(),
            })
            .collect::<Vec<_>>()
    };
    if entity_refs.is_empty() && !publishes_active_cycle && !publishes_branch_geometry {
        entity_refs.extend(
            changed_nodes
                .iter()
                .map(|index| FlowTraceEntityRefSceneV1::Node {
                    node_id: graph.nodes()[*index].id().as_str().to_owned(),
                }),
        );
    }
    let (label, value) = deterministic_almost_linear_event_detail(&event.after);
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: (event_id > 3).then(|| "3".to_owned()),
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: if matches!(
            event.after.stage,
            DeterministicAlmostLinearMaxFlowStage::InspectFundamentalCycle
                | DeterministicAlmostLinearMaxFlowStage::InstallBranchRecord
        ) {
            TraceGranularityV1::Micro
        } else if matches!(
            event.after.stage,
            DeterministicAlmostLinearMaxFlowStage::QueryMinimumRatioCycle
                | DeterministicAlmostLinearMaxFlowStage::ShiftBranch
                | DeterministicAlmostLinearMaxFlowStage::PotentialReductionStep
                | DeterministicAlmostLinearMaxFlowStage::DetectChangedCoordinates
                | DeterministicAlmostLinearMaxFlowStage::RoundingIntegralEdge
                | DeterministicAlmostLinearMaxFlowStage::RoundingLinkFractionalEdge
                | DeterministicAlmostLinearMaxFlowStage::RoundingCancelFractionalCycle
        ) {
            TraceGranularityV1::Operation
        } else {
            TraceGranularityV1::Phase
        },
        pseudocode_line: deterministic_almost_linear_pseudocode_line(event.after.stage).to_owned(),
        patch_count: u32::try_from(changed_nodes.len() + changed_edges.len() + 1)
            .map_err(|_| JsError::new("deterministic almost-linear patch count overflow"))?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn deterministic_almost_linear_event_detail(
    snapshot: &DeterministicAlmostLinearMaxFlowSnapshot,
) -> (&'static str, String) {
    use DeterministicAlmostLinearMaxFlowStage as Stage;
    match snapshot.stage {
        Stage::Ready | Stage::BuildReturnEdgeReduction => {
            ("return capacity", snapshot.return_capacity.to_string())
        }
        Stage::BuildInitialPoint => ("artificial flow", snapshot.artificial_flow.decimal()),
        Stage::EnumerateForestPool => ("forest population", snapshot.forest_pool_size.to_string()),
        Stage::InstallBranchRecord | Stage::BuildBranchCollection => (
            "installed branch records",
            snapshot.metrics.branch_records.to_string(),
        ),
        Stage::ScheduledRebuild => ("rebuild epoch", snapshot.rebuild_epoch.to_string()),
        Stage::BuildCoreGraph => ("core edges", snapshot.core_edges.to_string()),
        Stage::BuildSpannerEmbedding => ("embedding hops", snapshot.embedding_hops.to_string()),
        Stage::InspectFundamentalCycle => (
            "fundamental cycle",
            snapshot.metrics.fundamental_cycles.to_string(),
        ),
        Stage::QueryMinimumRatioCycle => (
            "chain ratio",
            snapshot.selected_ratio.map_or_else(
                || "0".to_owned(),
                flow::DeterministicAlmostLinearScalar::decimal,
            ),
        ),
        Stage::QueryFailure => (
            "oracle ratio",
            snapshot.exact_pool_ratio.map_or_else(
                || "0".to_owned(),
                flow::DeterministicAlmostLinearScalar::decimal,
            ),
        ),
        Stage::ShiftBranch | Stage::RebuildDeeperLevels => (
            "shifted level",
            snapshot
                .active_level
                .map_or_else(|| "0".to_owned(), |level| level.to_string()),
        ),
        Stage::PotentialReductionStep => ("potential", snapshot.potential.decimal()),
        Stage::DetectChangedCoordinates => (
            "detected coordinates",
            snapshot.metrics.detected_coordinates.to_string(),
        ),
        Stage::EnumerateFeasibleSet => (
            "feasible circulations",
            snapshot.metrics.feasible_circulations.to_string(),
        ),
        Stage::ConstructFinalPoint => (
            "additive gap numerator",
            snapshot
                .final_point_gap
                .as_ref()
                .map_or_else(|| "0".to_owned(), |gap| gap.numer().to_string()),
        ),
        Stage::RoundingIntegralEdge | Stage::RoundingLinkFractionalEdge => (
            "processed edges",
            snapshot.metrics.rounding_processed_edges.to_string(),
        ),
        Stage::RoundingCancelFractionalCycle => (
            "canceled cycles",
            snapshot.metrics.rounding_cycles.to_string(),
        ),
        Stage::FinishFlowRounding | Stage::CheckCertificate | Stage::Optimal => {
            ("maximum flow", snapshot.target_value.to_string())
        }
    }
}

const fn deterministic_almost_linear_pseudocode_line(
    stage: DeterministicAlmostLinearMaxFlowStage,
) -> &'static str {
    use DeterministicAlmostLinearMaxFlowStage as Stage;
    match stage {
        Stage::Ready => "validate the bounded integral max-flow instance",
        Stage::BuildReturnEdgeReduction => "add t -> s with capacity mU and cost -1",
        Stage::BuildInitialPoint => "install midpoint flow and artificial-star balancing edges",
        Stage::EnumerateForestPool => "enumerate the bounded exact forest population",
        Stage::InstallBranchRecord => {
            "install one level/branch tree, contracted core, and spanner embedding"
        }
        Stage::BuildBranchCollection => "build stable deterministic low-stretch branch records",
        Stage::BuildCoreGraph => "contract the active partial forest into its explicit core",
        Stage::BuildSpannerEmbedding => "build a deterministic core spanner and every embedding",
        Stage::InspectFundamentalCycle => "evaluate this forest fundamental cycle",
        Stage::QueryMinimumRatioCycle => "query tree and off-spanner fundamental cycles",
        Stage::QueryFailure => "compare the chain candidate with the bounded exact oracle",
        Stage::ShiftBranch => "shift the largest level whose pass budget remains",
        Stage::RebuildDeeperLevels => "rebuild every level below the shifted branch",
        Stage::PotentialReductionStep => "apply eta <g,Delta> = -kappa^2 / 50",
        Stage::DetectChangedCoordinates => "detect changed gradient and length coordinates",
        Stage::ScheduledRebuild => "refresh the whole deterministic chain on schedule",
        Stage::EnumerateFeasibleSet => {
            "enumerate bounded feasible return-edge integer circulations"
        }
        Stage::ConstructFinalPoint => {
            "construct an exact feasible point with additive cost gap below one half"
        }
        Stage::RoundingIntegralEdge => "skip an already-integral coordinate",
        Stage::RoundingLinkFractionalEdge => {
            "link a fractional edge between distinct forest components"
        }
        Stage::RoundingCancelFractionalCycle => {
            "cancel a fractional cycle in a non-increasing-cost direction"
        }
        Stage::FinishFlowRounding => {
            "finish when the fractional-edge forest implies every flow is integral"
        }
        Stage::CheckCertificate => "check original flow, return flow, artificial zero, and min cut",
        Stage::Optimal => "publish certified flow without claiming project almost-linear runtime",
    }
}

fn apply_deterministic_almost_linear_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    source: flow::NodeIndex,
    sink: flow::NodeIndex,
    result: &DeterministicAlmostLinearMaxFlowResult,
) -> Result<(), JsError> {
    if result.flows != deterministic_almost_linear_flows(graph, &result.final_snapshot)? {
        return Err(JsError::new(
            "deterministic almost-linear result projection mismatch",
        ));
    }
    scene
        .apply_deterministic_almost_linear_boundary(
            graph,
            source,
            sink,
            &result.flows,
            deterministic_almost_linear_overlay(graph, &result.final_snapshot)?,
            1,
            1,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_deterministic_almost_linear_metrics(result.metrics);
    scene
        .set_deterministic_almost_linear_outcome(&result.certificate)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(())
}

fn normalized_unsigned_rational(
    numerator: u128,
    denominator: u128,
) -> Result<FlowRationalV1, JsError> {
    if denominator == 0 {
        return Err(JsError::new(
            "enhanced scaling rational denominator is zero",
        ));
    }
    let divisor = unsigned_gcd(numerator, denominator);
    Ok(FlowRationalV1 {
        numerator: (numerator / divisor).to_string(),
        denominator: (denominator / divisor).to_string(),
    })
}

fn normalized_signed_rational(
    numerator: i128,
    denominator: u128,
) -> Result<FlowRationalV1, JsError> {
    if denominator == 0 {
        return Err(JsError::new(
            "enhanced scaling rational denominator is zero",
        ));
    }
    let divisor = unsigned_gcd(numerator.unsigned_abs(), denominator);
    let reduced_abs = numerator.unsigned_abs() / divisor;
    let reduced = if numerator.is_negative() {
        if reduced_abs == 1_u128 << 127 {
            i128::MIN
        } else {
            i128::try_from(reduced_abs)
                .ok()
                .and_then(i128::checked_neg)
                .ok_or_else(|| JsError::new("enhanced scaling rational overflow"))?
        }
    } else {
        i128::try_from(reduced_abs)
            .map_err(|_| JsError::new("enhanced scaling rational overflow"))?
    };
    Ok(FlowRationalV1 {
        numerator: reduced.to_string(),
        denominator: (denominator / divisor).to_string(),
    })
}

const fn unsigned_gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    if left == 0 { 1 } else { left }
}

fn enhanced_scaling_node_id(
    graph: &flow::FlowNetwork,
    index: flow::NodeIndex,
) -> Result<String, JsError> {
    graph
        .node(index)
        .ok_or_else(|| JsError::new("enhanced scaling component node is absent"))
        .map(|node| node.id().as_str().to_owned())
}

const fn enhanced_capacity_scaling_scene_stage(
    stage: EnhancedCapacityScalingStage,
) -> FlowEnhancedCapacityScalingStageV1 {
    match stage {
        EnhancedCapacityScalingStage::Ready => FlowEnhancedCapacityScalingStageV1::Ready,
        EnhancedCapacityScalingStage::Initialize => FlowEnhancedCapacityScalingStageV1::Initialize,
        EnhancedCapacityScalingStage::CompleteRegeneration => {
            FlowEnhancedCapacityScalingStageV1::CompleteRegeneration
        }
        EnhancedCapacityScalingStage::BeginPhase => FlowEnhancedCapacityScalingStageV1::BeginPhase,
        EnhancedCapacityScalingStage::Contract => FlowEnhancedCapacityScalingStageV1::Contract,
        EnhancedCapacityScalingStage::InspectResidualArc => {
            FlowEnhancedCapacityScalingStageV1::InspectResidualArc
        }
        EnhancedCapacityScalingStage::SelectPath => FlowEnhancedCapacityScalingStageV1::SelectPath,
        EnhancedCapacityScalingStage::Augment => FlowEnhancedCapacityScalingStageV1::Augment,
        EnhancedCapacityScalingStage::CompletePhase => {
            FlowEnhancedCapacityScalingStageV1::CompletePhase
        }
        EnhancedCapacityScalingStage::HalveScale => FlowEnhancedCapacityScalingStageV1::HalveScale,
        EnhancedCapacityScalingStage::RecoverPrimal => {
            FlowEnhancedCapacityScalingStageV1::RecoverPrimal
        }
        EnhancedCapacityScalingStage::Optimal => FlowEnhancedCapacityScalingStageV1::Optimal,
    }
}

fn enhanced_capacity_scaling_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &EnhancedCapacityScalingTraceEvent,
    event_id: u64,
    parent_phase_id: Option<u64>,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let mut entity_refs = event
        .after
        .path
        .iter()
        .map(|arc| FlowTraceEntityRefSceneV1::ResidualArc {
            edge_id: arc.original_edge().as_str().to_owned(),
            direction: match arc.direction() {
                ResidualDirection::Forward => "forward",
                ResidualDirection::Reverse => "reverse",
            }
            .to_owned(),
        })
        .collect::<Vec<_>>();
    if let Some(edge) = &event.contraction_arc {
        entity_refs.push(FlowTraceEntityRefSceneV1::Edge {
            edge_id: edge.as_str().to_owned(),
        });
    }
    if entity_refs.is_empty() {
        entity_refs = event
            .after
            .components
            .iter()
            .filter_map(|component| component.members.first())
            .filter_map(|&node| graph.node(node))
            .map(|node| FlowTraceEntityRefSceneV1::Node {
                node_id: node.id().as_str().to_owned(),
            })
            .collect();
    }
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: if event.after.stage == EnhancedCapacityScalingStage::BeginPhase {
            None
        } else {
            parent_phase_id.map(|value| value.to_string())
        },
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: match event.after.stage {
            EnhancedCapacityScalingStage::InspectResidualArc => TraceGranularityV1::Micro,
            EnhancedCapacityScalingStage::Contract
            | EnhancedCapacityScalingStage::SelectPath
            | EnhancedCapacityScalingStage::Augment => TraceGranularityV1::Operation,
            _ => TraceGranularityV1::Phase,
        },
        pseudocode_line: enhanced_capacity_scaling_pseudocode_line(event.after.stage).to_owned(),
        patch_count: enhanced_capacity_scaling_patch_count(event)?,
        entity_refs,
        detail: Some(FlowTraceEventDetailSceneV1 {
            label: match event.after.stage {
                EnhancedCapacityScalingStage::InspectResidualArc => "residual arc scan",
                EnhancedCapacityScalingStage::Contract => "quotient components",
                EnhancedCapacityScalingStage::Augment => "augmentation numerator",
                _ => "delta numerator",
            }
            .to_owned(),
            value: match event.after.stage {
                EnhancedCapacityScalingStage::InspectResidualArc => {
                    event.after.metrics.residual_arc_scans.to_string()
                }
                EnhancedCapacityScalingStage::Contract => event.after.components.len().to_string(),
                EnhancedCapacityScalingStage::Augment => event
                    .augmentation_numerator
                    .unwrap_or(event.after.delta_numerator)
                    .to_string(),
                _ => event.after.delta_numerator.to_string(),
            },
        }),
    })
}

fn enhanced_capacity_scaling_patch_count(
    event: &EnhancedCapacityScalingTraceEvent,
) -> Result<u32, JsError> {
    let changes = event
        .before
        .virtual_flow_numerators
        .iter()
        .zip(&event.after.virtual_flow_numerators)
        .filter(|(before, after)| before != after)
        .count()
        + event
            .before
            .potentials
            .iter()
            .zip(&event.after.potentials)
            .filter(|(before, after)| before != after)
            .count()
        + usize::from(event.before.denominator != event.after.denominator)
        + usize::from(event.before.delta_numerator != event.after.delta_numerator)
        + usize::from(event.before.components != event.after.components)
        + usize::from(event.before.path != event.after.path)
        + usize::from(event.before.stage != event.after.stage);
    u32::try_from(changes.max(1))
        .map_err(|_| JsError::new("enhanced capacity scaling patch count overflow"))
}

const fn enhanced_capacity_scaling_pseudocode_line(
    stage: EnhancedCapacityScalingStage,
) -> &'static str {
    match stage {
        EnhancedCapacityScalingStage::Ready => "orlin-ecs:zero-pseudoflow",
        EnhancedCapacityScalingStage::Initialize => "orlin-ecs:initialize-dual-prices",
        EnhancedCapacityScalingStage::CompleteRegeneration => "orlin-ecs:complete-regeneration",
        EnhancedCapacityScalingStage::BeginPhase => "orlin-ecs:begin-delta-phase",
        EnhancedCapacityScalingStage::Contract => "orlin-ecs:contract-strongly-feasible-arc",
        EnhancedCapacityScalingStage::InspectResidualArc => {
            "orlin-ecs:inspect-original-residual-direction"
        }
        EnhancedCapacityScalingStage::SelectPath => "orlin-ecs:shortest-quotient-path",
        EnhancedCapacityScalingStage::Augment => "orlin-ecs:send-exact-delta",
        EnhancedCapacityScalingStage::CompletePhase => "orlin-ecs:active-sets-empty",
        EnhancedCapacityScalingStage::HalveScale => "orlin-ecs:halve-delta-exactly",
        EnhancedCapacityScalingStage::RecoverPrimal => "orlin-ecs:recover-tight-primal-flow",
        EnhancedCapacityScalingStage::Optimal => "orlin-ecs:return-independent-certificate",
    }
}

fn dual_network_simplex_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::DualNetworkSimplexTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("dual network simplex event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("dual network simplex trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("dual network simplex event identity overflow"))?;
        let overlay = dual_network_simplex_overlay(graph, &event.after)?;
        let display_flows = dual_network_simplex_display_flows(graph, &event.after)?;
        let mut scene = base.clone();
        scene
            .apply_dual_network_simplex_boundary(
                graph,
                &display_flows,
                overlay,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(dual_network_simplex_trace_event_scene(
            graph, event, event_id,
        )?);
        scene
            .set_dual_network_simplex_metrics(event.after.metrics)
            .map_err(|error| JsError::new(&error.to_string()))?;
        if index + 1 == run.events.len() {
            scene.set_min_cost_flow_outcome(graph, &run.result.certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new("dual network simplex final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn dual_network_simplex_display_flows(
    graph: &flow::FlowNetwork,
    snapshot: &DualNetworkSimplexSnapshot,
) -> Result<Vec<u64>, JsError> {
    if let Some(flows) = &snapshot.certified_flows {
        return Ok(flows.clone());
    }
    graph
        .edges()
        .iter()
        .zip(&snapshot.basic_flows)
        .map(|(edge, &basic)| {
            let variable = u64::try_from(basic.max(0))
                .map_err(|_| JsError::new("dual network simplex display flow overflow"))?;
            let flow = edge
                .lower()
                .checked_add(variable)
                .ok_or_else(|| JsError::new("dual network simplex display flow overflow"))?;
            if flow > edge.capacity() {
                Err(JsError::new(
                    "dual network simplex display flow exceeds capacity",
                ))
            } else {
                Ok(flow)
            }
        })
        .collect()
}

fn dual_network_simplex_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &DualNetworkSimplexSnapshot,
) -> Result<FlowDualNetworkSimplexOverlayV1, JsError> {
    let tree = snapshot
        .tree_edges
        .iter()
        .map(flow::EdgeId::as_str)
        .collect::<BTreeSet<_>>();
    let cut = snapshot
        .cut_side
        .iter()
        .map(|&index| {
            graph
                .node(index)
                .map(|node| node.id().as_str().to_owned())
                .ok_or_else(|| JsError::new("dual network simplex cut node is absent"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let cut_set = cut.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let initialized = snapshot
        .initialized_nodes
        .iter()
        .map(|index| index.as_usize())
        .collect::<BTreeSet<_>>();
    let nodes = graph
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            Ok(FlowDualNetworkSimplexNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                potential: snapshot
                    .potentials
                    .get(index)
                    .ok_or_else(|| JsError::new("dual network simplex potential is absent"))?
                    .to_string(),
                initialized: initialized.contains(&index),
                in_cut: cut_set.contains(node.id().as_str()),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let edges = graph
        .edges()
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            Ok(FlowDualNetworkSimplexEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                basic_flow: snapshot
                    .basic_flows
                    .get(index)
                    .ok_or_else(|| JsError::new("dual network simplex basic flow is absent"))?
                    .to_string(),
                reduced_cost: snapshot
                    .reduced_costs
                    .get(index)
                    .ok_or_else(|| JsError::new("dual network simplex reduced cost is absent"))?
                    .to_string(),
                in_tree: tree.contains(edge.id().as_str()),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(FlowDualNetworkSimplexOverlayV1 {
        stage: dual_network_simplex_scene_stage(snapshot.stage),
        nodes,
        edges,
        cut_side: cut,
        leaving_edge: snapshot
            .leaving_edge
            .as_ref()
            .map(|edge| edge.as_str().to_owned()),
        entering_edge: snapshot
            .entering_edge
            .as_ref()
            .map(|edge| edge.as_str().to_owned()),
        inspected_edge: snapshot
            .inspected_edge
            .as_ref()
            .map(|edge| edge.as_str().to_owned()),
        pivot_price_delta: snapshot.pivot_price_delta.map(|value| value.to_string()),
    })
}

const fn dual_network_simplex_scene_stage(
    stage: DualNetworkSimplexStage,
) -> FlowDualNetworkSimplexStageV1 {
    match stage {
        DualNetworkSimplexStage::Ready => FlowDualNetworkSimplexStageV1::Ready,
        DualNetworkSimplexStage::InspectInitialArc => {
            FlowDualNetworkSimplexStageV1::InspectInitialArc
        }
        DualNetworkSimplexStage::InitializeDualTree => {
            FlowDualNetworkSimplexStageV1::InitializeDualTree
        }
        DualNetworkSimplexStage::SelectLeaving => FlowDualNetworkSimplexStageV1::SelectLeaving,
        DualNetworkSimplexStage::InspectEnteringArc => {
            FlowDualNetworkSimplexStageV1::InspectEnteringArc
        }
        DualNetworkSimplexStage::SelectEntering => FlowDualNetworkSimplexStageV1::SelectEntering,
        DualNetworkSimplexStage::Pivot => FlowDualNetworkSimplexStageV1::Pivot,
        DualNetworkSimplexStage::Optimal => FlowDualNetworkSimplexStageV1::Optimal,
    }
}

fn dual_network_simplex_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &DualNetworkSimplexTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let stage = event.after.stage;
    let mut entity_refs = Vec::new();
    let focused_edges = match stage {
        DualNetworkSimplexStage::InspectInitialArc
        | DualNetworkSimplexStage::InspectEnteringArc => {
            event.after.inspected_edge.iter().collect::<Vec<_>>()
        }
        DualNetworkSimplexStage::SelectLeaving => {
            event.after.leaving_edge.iter().collect::<Vec<_>>()
        }
        DualNetworkSimplexStage::SelectEntering | DualNetworkSimplexStage::Pivot => event
            .after
            .leaving_edge
            .iter()
            .chain(event.after.entering_edge.iter())
            .collect(),
        _ => event
            .after
            .inspected_edge
            .iter()
            .chain(event.after.leaving_edge.iter())
            .chain(event.after.entering_edge.iter())
            .collect(),
    };
    for edge in focused_edges {
        entity_refs.push(FlowTraceEntityRefSceneV1::Edge {
            edge_id: edge.as_str().to_owned(),
        });
    }
    if entity_refs.is_empty() {
        entity_refs = event
            .after
            .cut_side
            .iter()
            .filter_map(|&index| graph.node(index))
            .map(|node| FlowTraceEntityRefSceneV1::Node {
                node_id: node.id().as_str().to_owned(),
            })
            .collect();
    }
    let detail = match event.after.stage {
        DualNetworkSimplexStage::InspectInitialArc => Some((
            "shortest-path arc scan",
            event.after.metrics.shortest_path_arc_scans.to_string(),
        )),
        DualNetworkSimplexStage::SelectLeaving => {
            event.after.leaving_edge.as_ref().and_then(|id| {
                graph
                    .edges()
                    .iter()
                    .position(|edge| edge.id() == id)
                    .and_then(|index| event.after.basic_flows.get(index))
                    .map(|flow| ("basic flow", flow.to_string()))
            })
        }
        DualNetworkSimplexStage::InspectEnteringArc => Some((
            "pricing arc scan",
            event.after.metrics.entering_arc_scans.to_string(),
        )),
        DualNetworkSimplexStage::SelectEntering | DualNetworkSimplexStage::Pivot => event
            .after
            .pivot_price_delta
            .map(|delta| ("price delta", delta.to_string())),
        _ => Some(("tree edges", event.after.tree_edges.len().to_string())),
    };
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: None,
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: match event.after.stage {
            DualNetworkSimplexStage::InspectInitialArc
            | DualNetworkSimplexStage::InspectEnteringArc => TraceGranularityV1::Micro,
            DualNetworkSimplexStage::SelectLeaving
            | DualNetworkSimplexStage::SelectEntering
            | DualNetworkSimplexStage::Pivot => TraceGranularityV1::Operation,
            _ => TraceGranularityV1::Phase,
        },
        pseudocode_line: dual_network_simplex_pseudocode_line(event.after.stage).to_owned(),
        patch_count: dual_network_simplex_patch_count(event)?,
        entity_refs,
        detail: detail.map(|(label, value)| FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn dual_network_simplex_patch_count(event: &DualNetworkSimplexTraceEvent) -> Result<u32, JsError> {
    let changes = event
        .before
        .basic_flows
        .iter()
        .zip(&event.after.basic_flows)
        .filter(|(before, after)| before != after)
        .count()
        + event
            .before
            .potentials
            .iter()
            .zip(&event.after.potentials)
            .filter(|(before, after)| before != after)
            .count()
        + usize::from(event.before.tree_edges != event.after.tree_edges)
        + usize::from(event.before.cut_side != event.after.cut_side)
        + usize::from(event.before.inspected_edge != event.after.inspected_edge)
        + usize::from(event.before.stage != event.after.stage);
    u32::try_from(changes.max(1))
        .map_err(|_| JsError::new("dual network simplex patch count overflow"))
}

const fn dual_network_simplex_pseudocode_line(stage: DualNetworkSimplexStage) -> &'static str {
    match stage {
        DualNetworkSimplexStage::Ready => "dual-ns:shift-lower-bounds",
        DualNetworkSimplexStage::InspectInitialArc => "dual-ns:inspect-shortest-path-arc",
        DualNetworkSimplexStage::InitializeDualTree => "dual-ns:shortest-path-dual-tree",
        DualNetworkSimplexStage::SelectLeaving => "dual-ns:select-negative-basic-arc",
        DualNetworkSimplexStage::InspectEnteringArc => "dual-ns:inspect-cut-pricing-arc",
        DualNetworkSimplexStage::SelectEntering => "dual-ns:minimum-reduced-cost-cut-arc",
        DualNetworkSimplexStage::Pivot => "dual-ns:exchange-tree-and-shift-cut-price",
        DualNetworkSimplexStage::Optimal => "dual-ns:return-independent-certificate",
    }
}

fn polynomial_dual_simplex_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::PolynomialDualSimplexTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("polynomial dual simplex event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    let mut parent_phase_id = None;
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("polynomial dual simplex trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("polynomial dual simplex event identity overflow"))?;
        if event.after.stage == PolynomialDualSimplexStage::BeginScale {
            parent_phase_id = Some(event_id);
        }
        let overlay = polynomial_dual_simplex_overlay(graph, &event.after)?;
        let display_flows = polynomial_dual_simplex_display_flows(graph, &event.after)?;
        let mut scene = base.clone();
        scene
            .apply_polynomial_dual_simplex_boundary(
                graph,
                &display_flows,
                overlay,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(polynomial_dual_simplex_trace_event_scene(
            graph,
            event,
            event_id,
            parent_phase_id,
        )?);
        scene
            .set_polynomial_dual_simplex_metrics(event.after.metrics)
            .map_err(|error| JsError::new(&error.to_string()))?;
        if index + 1 == run.events.len() {
            scene.set_min_cost_flow_outcome(graph, &run.result.certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new(
            "polynomial dual simplex final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn polynomial_dual_simplex_display_flows(
    graph: &flow::FlowNetwork,
    snapshot: &PolynomialDualSimplexSnapshot,
) -> Result<Vec<u64>, JsError> {
    if let Some(flows) = &snapshot.certified_flows {
        return Ok(flows.clone());
    }
    graph
        .edges()
        .iter()
        .zip(&snapshot.basic_flows)
        .map(|(edge, &basic)| {
            let variable = u64::try_from(basic.max(0))
                .map_err(|_| JsError::new("polynomial dual simplex display flow overflow"))?;
            edge.lower()
                .checked_add(variable)
                .filter(|&flow| flow <= edge.capacity())
                .ok_or_else(|| JsError::new("polynomial dual simplex display flow is invalid"))
        })
        .collect()
}

fn polynomial_dual_simplex_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &PolynomialDualSimplexSnapshot,
) -> Result<FlowPolynomialDualSimplexOverlayV1, JsError> {
    let denominator = u128::try_from(snapshot.scale_denominator)
        .map_err(|_| JsError::new("polynomial dual simplex denominator is invalid"))?;
    let tree = snapshot
        .tree_edges
        .iter()
        .map(flow::EdgeId::as_str)
        .collect::<BTreeSet<_>>();
    let bad_edges = snapshot
        .bad_edges
        .iter()
        .map(|edge| edge.as_str().to_owned())
        .collect::<Vec<_>>();
    let bad_edge_set = bad_edges
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let bad_nodes = polynomial_dual_node_ids(graph, &snapshot.bad_nodes)?;
    let bad_node_set = bad_nodes
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let pivot_cut = polynomial_dual_node_ids(graph, &snapshot.pivot_cut)?;
    let pivot_cut_set = pivot_cut
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let active_node = snapshot
        .active_node
        .map(|index| {
            graph
                .node(index)
                .map(|node| node.id().as_str().to_owned())
                .ok_or_else(|| JsError::new("polynomial dual simplex active node is absent"))
        })
        .transpose()?;
    let path_by_edge = snapshot
        .augment_path
        .iter()
        .map(|reference| (reference.edge_index, reference.forward))
        .collect::<BTreeMap<_, _>>();
    let nodes = polynomial_dual_node_states(
        graph,
        snapshot,
        denominator,
        active_node.as_deref(),
        &bad_node_set,
        &pivot_cut_set,
    )?;
    let edges = polynomial_dual_edge_states(
        graph,
        snapshot,
        denominator,
        &tree,
        &bad_edge_set,
        &path_by_edge,
    )?;
    Ok(FlowPolynomialDualSimplexOverlayV1 {
        stage: polynomial_dual_simplex_scene_stage(snapshot.stage),
        phase: snapshot.phase.to_string(),
        delta: normalized_signed_rational(snapshot.delta_numerator, denominator)?,
        nodes,
        edges,
        active_node,
        augment_path: snapshot
            .augment_path
            .iter()
            .map(|reference| polynomial_dual_residual_ref(graph, *reference))
            .collect::<Result<Vec<_>, _>>()?,
        bad_edges,
        bad_nodes,
        leaving_edge: snapshot
            .leaving_edge
            .as_ref()
            .map(|edge| edge.as_str().to_owned()),
        entering_edge: snapshot
            .entering_edge
            .as_ref()
            .map(|edge| edge.as_str().to_owned()),
        pivot_cut,
        pivot_price_delta: snapshot.pivot_price_delta.map(|value| value.to_string()),
    })
}

fn polynomial_dual_node_states(
    graph: &flow::FlowNetwork,
    snapshot: &PolynomialDualSimplexSnapshot,
    denominator: u128,
    active_node: Option<&str>,
    bad_nodes: &BTreeSet<&str>,
    pivot_cut: &BTreeSet<&str>,
) -> Result<Vec<FlowPolynomialDualNodeStateV1>, JsError> {
    graph
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            Ok(FlowPolynomialDualNodeStateV1 {
                node_id: node.id().as_str().to_owned(),
                potential: snapshot
                    .potentials
                    .get(index)
                    .ok_or_else(|| JsError::new("polynomial dual simplex potential is absent"))?
                    .to_string(),
                excess: normalized_signed_rational(
                    *snapshot
                        .excess_numerators
                        .get(index)
                        .ok_or_else(|| JsError::new("polynomial dual simplex excess is absent"))?,
                    denominator,
                )?,
                root: index == snapshot.root,
                active: active_node == Some(node.id().as_str()),
                bad: bad_nodes.contains(node.id().as_str()),
                in_pivot_cut: pivot_cut.contains(node.id().as_str()),
            })
        })
        .collect()
}

fn polynomial_dual_edge_states(
    graph: &flow::FlowNetwork,
    snapshot: &PolynomialDualSimplexSnapshot,
    denominator: u128,
    tree: &BTreeSet<&str>,
    bad_edges: &BTreeSet<&str>,
    path_by_edge: &BTreeMap<usize, bool>,
) -> Result<Vec<FlowPolynomialDualEdgeStateV1>, JsError> {
    graph
        .edges()
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            let path_direction = path_by_edge.get(&index).copied();
            Ok(FlowPolynomialDualEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                pseudoflow: normalized_signed_rational(
                    *snapshot.pseudoflow_numerators.get(index).ok_or_else(|| {
                        JsError::new("polynomial dual simplex pseudoflow is absent")
                    })?,
                    denominator,
                )?,
                basic_flow: snapshot
                    .basic_flows
                    .get(index)
                    .ok_or_else(|| JsError::new("polynomial dual simplex basic flow is absent"))?
                    .to_string(),
                reduced_cost: snapshot
                    .reduced_costs
                    .get(index)
                    .ok_or_else(|| JsError::new("polynomial dual simplex reduced cost is absent"))?
                    .to_string(),
                in_tree: tree.contains(edge.id().as_str()),
                bad: bad_edges.contains(edge.id().as_str()),
                in_augment_path: path_direction.is_some(),
                augment_direction: path_direction
                    .map(|forward| if forward { "forward" } else { "reverse" }.to_owned()),
            })
        })
        .collect()
}

fn polynomial_dual_node_ids(
    graph: &flow::FlowNetwork,
    nodes: &[flow::NodeIndex],
) -> Result<Vec<String>, JsError> {
    nodes
        .iter()
        .map(|&index| {
            graph
                .node(index)
                .map(|node| node.id().as_str().to_owned())
                .ok_or_else(|| JsError::new("polynomial dual simplex node is absent"))
        })
        .collect()
}

fn polynomial_dual_residual_ref(
    graph: &flow::FlowNetwork,
    reference: PolynomialDualResidualRef,
) -> Result<FlowResidualArcRefV1, JsError> {
    let edge = graph
        .edges()
        .get(reference.edge_index)
        .ok_or_else(|| JsError::new("polynomial dual simplex path edge is absent"))?;
    Ok(FlowResidualArcRefV1 {
        edge_id: edge.id().as_str().to_owned(),
        direction: if reference.forward {
            "forward"
        } else {
            "reverse"
        }
        .to_owned(),
    })
}

const fn polynomial_dual_simplex_scene_stage(
    stage: PolynomialDualSimplexStage,
) -> FlowPolynomialDualSimplexStageV1 {
    match stage {
        PolynomialDualSimplexStage::Ready => FlowPolynomialDualSimplexStageV1::Ready,
        PolynomialDualSimplexStage::InspectInitialArc => {
            FlowPolynomialDualSimplexStageV1::InspectInitialArc
        }
        PolynomialDualSimplexStage::InitializeTree => {
            FlowPolynomialDualSimplexStageV1::InitializeTree
        }
        PolynomialDualSimplexStage::InitializePseudoflow => {
            FlowPolynomialDualSimplexStageV1::InitializePseudoflow
        }
        PolynomialDualSimplexStage::BeginScale => FlowPolynomialDualSimplexStageV1::BeginScale,
        PolynomialDualSimplexStage::InspectAugmentationArc => {
            FlowPolynomialDualSimplexStageV1::InspectAugmentationArc
        }
        PolynomialDualSimplexStage::SelectActive => FlowPolynomialDualSimplexStageV1::SelectActive,
        PolynomialDualSimplexStage::AugmentToRoot => {
            FlowPolynomialDualSimplexStageV1::AugmentToRoot
        }
        PolynomialDualSimplexStage::SelectBadArc => FlowPolynomialDualSimplexStageV1::SelectBadArc,
        PolynomialDualSimplexStage::InspectEnteringArc => {
            FlowPolynomialDualSimplexStageV1::InspectEnteringArc
        }
        PolynomialDualSimplexStage::SelectEntering => {
            FlowPolynomialDualSimplexStageV1::SelectEntering
        }
        PolynomialDualSimplexStage::PivotMakeGood => {
            FlowPolynomialDualSimplexStageV1::PivotMakeGood
        }
        PolynomialDualSimplexStage::FinishScale => FlowPolynomialDualSimplexStageV1::FinishScale,
        PolynomialDualSimplexStage::Optimal => FlowPolynomialDualSimplexStageV1::Optimal,
    }
}

fn polynomial_dual_simplex_entity_refs(
    graph: &flow::FlowNetwork,
    event: &PolynomialDualSimplexTraceEvent,
) -> Result<Vec<FlowTraceEntityRefSceneV1>, JsError> {
    let stage = event.after.stage;
    let mut entity_refs = Vec::new();
    match stage {
        PolynomialDualSimplexStage::InspectInitialArc
        | PolynomialDualSimplexStage::InspectAugmentationArc
        | PolynomialDualSimplexStage::InspectEnteringArc => {
            if let Some(edge) = &event.after.inspected_edge {
                entity_refs.push(FlowTraceEntityRefSceneV1::Edge {
                    edge_id: edge.as_str().to_owned(),
                });
            }
        }
        PolynomialDualSimplexStage::SelectActive => {
            if let Some(node) = event.after.active_node.and_then(|index| graph.node(index)) {
                entity_refs.push(FlowTraceEntityRefSceneV1::Node {
                    node_id: node.id().as_str().to_owned(),
                });
            }
        }
        PolynomialDualSimplexStage::AugmentToRoot => {
            entity_refs = event
                .after
                .augment_path
                .iter()
                .map(|&reference| {
                    let reference = polynomial_dual_residual_ref(graph, reference)?;
                    Ok(FlowTraceEntityRefSceneV1::ResidualArc {
                        edge_id: reference.edge_id,
                        direction: reference.direction,
                    })
                })
                .collect::<Result<Vec<_>, JsError>>()?;
        }
        PolynomialDualSimplexStage::SelectBadArc => {
            if let Some(edge) = &event.after.leaving_edge {
                entity_refs.push(FlowTraceEntityRefSceneV1::Edge {
                    edge_id: edge.as_str().to_owned(),
                });
            }
        }
        PolynomialDualSimplexStage::SelectEntering | PolynomialDualSimplexStage::PivotMakeGood => {
            entity_refs.extend(
                [&event.after.leaving_edge, &event.after.entering_edge]
                    .into_iter()
                    .flatten()
                    .map(|edge| FlowTraceEntityRefSceneV1::Edge {
                        edge_id: edge.as_str().to_owned(),
                    }),
            );
        }
        _ => {}
    }
    Ok(entity_refs)
}

fn polynomial_dual_simplex_detail(
    event: &PolynomialDualSimplexTraceEvent,
) -> Option<(&'static str, String)> {
    match event.after.stage {
        PolynomialDualSimplexStage::InspectInitialArc => Some((
            "initial-tree arc scans",
            event.after.metrics.initial_arc_scans.to_string(),
        )),
        PolynomialDualSimplexStage::InspectAugmentationArc => Some((
            "augmentation-path arc scans",
            event.after.metrics.augmentation_arc_scans.to_string(),
        )),
        PolynomialDualSimplexStage::InspectEnteringArc => Some((
            "pricing arc scans",
            event.after.metrics.entering_arc_scans.to_string(),
        )),
        PolynomialDualSimplexStage::SelectActive | PolynomialDualSimplexStage::AugmentToRoot => {
            event.after.active_node.and_then(|node| {
                event
                    .after
                    .excess_numerators
                    .get(node.as_usize())
                    .map(|value| ("scaled excess numerator", value.to_string()))
            })
        }
        PolynomialDualSimplexStage::SelectEntering | PolynomialDualSimplexStage::PivotMakeGood => {
            event
                .after
                .pivot_price_delta
                .map(|value| ("price delta", value.to_string()))
        }
        PolynomialDualSimplexStage::BeginScale | PolynomialDualSimplexStage::FinishScale => {
            Some(("scale phase", event.after.phase.to_string()))
        }
        _ => Some(("tree edges", event.after.tree_edges.len().to_string())),
    }
}

fn polynomial_dual_simplex_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &PolynomialDualSimplexTraceEvent,
    event_id: u64,
    parent_phase_id: Option<u64>,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let entity_refs = polynomial_dual_simplex_entity_refs(graph, event)?;
    let detail = polynomial_dual_simplex_detail(event);
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: if event.after.stage == PolynomialDualSimplexStage::BeginScale {
            None
        } else {
            parent_phase_id.map(|value| value.to_string())
        },
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: match event.after.stage {
            PolynomialDualSimplexStage::InspectInitialArc
            | PolynomialDualSimplexStage::InspectAugmentationArc
            | PolynomialDualSimplexStage::InspectEnteringArc => TraceGranularityV1::Micro,
            PolynomialDualSimplexStage::SelectActive
            | PolynomialDualSimplexStage::AugmentToRoot
            | PolynomialDualSimplexStage::SelectBadArc
            | PolynomialDualSimplexStage::SelectEntering
            | PolynomialDualSimplexStage::PivotMakeGood => TraceGranularityV1::Operation,
            _ => TraceGranularityV1::Phase,
        },
        pseudocode_line: polynomial_dual_simplex_pseudocode_line(event.after.stage).to_owned(),
        patch_count: polynomial_dual_simplex_patch_count(event)?,
        entity_refs,
        detail: detail.map(|(label, value)| FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn polynomial_dual_simplex_patch_count(
    event: &PolynomialDualSimplexTraceEvent,
) -> Result<u32, JsError> {
    let changes = event
        .before
        .pseudoflow_numerators
        .iter()
        .zip(&event.after.pseudoflow_numerators)
        .filter(|(before, after)| before != after)
        .count()
        + event
            .before
            .basic_flows
            .iter()
            .zip(&event.after.basic_flows)
            .filter(|(before, after)| before != after)
            .count()
        + event
            .before
            .potentials
            .iter()
            .zip(&event.after.potentials)
            .filter(|(before, after)| before != after)
            .count()
        + usize::from(event.before.tree_edges != event.after.tree_edges)
        + usize::from(event.before.active_node != event.after.active_node)
        + usize::from(event.before.inspected_edge != event.after.inspected_edge)
        + usize::from(event.before.augment_path != event.after.augment_path)
        + usize::from(event.before.bad_edges != event.after.bad_edges)
        + usize::from(event.before.stage != event.after.stage);
    u32::try_from(changes.max(1))
        .map_err(|_| JsError::new("polynomial dual simplex patch count overflow"))
}

const fn polynomial_dual_simplex_pseudocode_line(
    stage: PolynomialDualSimplexStage,
) -> &'static str {
    match stage {
        PolynomialDualSimplexStage::Ready => "scaling-simplex:validate-transshipment-domain",
        PolynomialDualSimplexStage::InspectInitialArc => {
            "scaling-simplex:inspect-shortest-path-tree-arc"
        }
        PolynomialDualSimplexStage::InitializeTree => "scaling-simplex:shortest-path-tree",
        PolynomialDualSimplexStage::InitializePseudoflow => {
            "scaling-simplex:send-delta-root-to-each-node"
        }
        PolynomialDualSimplexStage::BeginScale => "scaling-simplex:begin-delta-scale",
        PolynomialDualSimplexStage::InspectAugmentationArc => {
            "scaling-simplex:inspect-tree-path-arc"
        }
        PolynomialDualSimplexStage::SelectActive => "scaling-simplex:select-excess-above-delta",
        PolynomialDualSimplexStage::AugmentToRoot => "scaling-simplex:send-delta-to-root",
        PolynomialDualSimplexStage::SelectBadArc => "make-good:first-bad-root-arc",
        PolynomialDualSimplexStage::InspectEnteringArc => "make-good:inspect-cut-pricing-arc",
        PolynomialDualSimplexStage::SelectEntering => "make-good:min-reduced-cost-cut-arc",
        PolynomialDualSimplexStage::PivotMakeGood => "make-good:exchange-tree-and-shift-price",
        PolynomialDualSimplexStage::FinishScale => "scaling-simplex:halve-delta",
        PolynomialDualSimplexStage::Optimal => "scaling-simplex:return-independent-certificate",
    }
}

fn polynomial_primal_simplex_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::PolynomialPrimalSimplexTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("polynomial primal simplex event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    let mut parent_phase_id = None;
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new(
                "polynomial primal simplex trace discontinuity",
            ));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("polynomial primal simplex event identity overflow"))?;
        if event.after.stage == PolynomialPrimalSimplexStage::BeginScale {
            parent_phase_id = Some(event_id);
        }
        let overlay = polynomial_primal_simplex_overlay(graph, &event.after)?;
        let display_flows = polynomial_primal_simplex_display_flows(graph, &event.after)?;
        let mut scene = base.clone();
        scene
            .apply_polynomial_primal_simplex_boundary(
                graph,
                &display_flows,
                overlay,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(polynomial_primal_simplex_trace_event_scene(
            graph,
            event,
            event_id,
            parent_phase_id,
        )?);
        scene
            .set_polynomial_primal_simplex_metrics(event.after.metrics)
            .map_err(|error| JsError::new(&error.to_string()))?;
        if index + 1 == run.events.len() {
            scene.set_min_cost_flow_outcome(graph, &run.result.certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new(
            "polynomial primal simplex final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn polynomial_primal_simplex_display_flows(
    graph: &flow::FlowNetwork,
    snapshot: &PolynomialPrimalSimplexSnapshot,
) -> Result<Vec<u64>, JsError> {
    if let Some(flows) = &snapshot.certified_flows {
        return Ok(flows.clone());
    }
    graph
        .edges()
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            let variable = snapshot
                .unperturbed_basic_flows
                .get(index)
                .copied()
                .ok_or_else(|| JsError::new("polynomial primal simplex flow is absent"))?;
            let variable = u64::try_from(variable)
                .map_err(|_| JsError::new("polynomial primal simplex display flow overflow"))?;
            edge.lower()
                .checked_add(variable)
                .filter(|&flow| flow <= edge.capacity())
                .ok_or_else(|| JsError::new("polynomial primal simplex display flow is invalid"))
        })
        .collect()
}

fn polynomial_primal_nodes(
    graph: &flow::FlowNetwork,
    snapshot: &PolynomialPrimalSimplexSnapshot,
) -> Vec<FlowPolynomialPrimalNodeStateV1> {
    let node_count = graph.nodes().len();
    let eligible = snapshot
        .eligible_nodes
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let awake = snapshot
        .awake_nodes
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let n_star = snapshot.n_star.iter().copied().collect::<BTreeSet<_>>();
    snapshot
        .premultipliers
        .iter()
        .enumerate()
        .map(|(index, potential)| {
            let mut flags = Vec::new();
            if eligible.contains(&index) {
                flags.push(FlowPolynomialPrimalNodeFlagV1::Eligible);
            }
            if awake.contains(&index) {
                flags.push(FlowPolynomialPrimalNodeFlagV1::Awake);
            }
            if n_star.contains(&index) {
                flags.push(FlowPolynomialPrimalNodeFlagV1::InNStar);
            }
            if snapshot.root == index {
                flags.push(FlowPolynomialPrimalNodeFlagV1::Root);
            }
            FlowPolynomialPrimalNodeStateV1 {
                entity_id: if index == node_count {
                    "artificial-root".to_owned()
                } else {
                    graph.nodes()[index].id().as_str().to_owned()
                },
                kind: if index == node_count {
                    FlowPolynomialPrimalNodeKindV1::ArtificialRoot
                } else {
                    FlowPolynomialPrimalNodeKindV1::Original
                },
                premultiplier: polynomial_primal_rational(potential),
                flags,
            }
        })
        .collect()
}

fn polynomial_primal_simplex_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &PolynomialPrimalSimplexSnapshot,
) -> Result<FlowPolynomialPrimalSimplexOverlayV1, JsError> {
    let node_count = graph.nodes().len();
    let edge_count = graph.edges().len();
    if snapshot.premultipliers.len() != node_count + 1
        || snapshot.basis_states.len() != edge_count + node_count
        || snapshot.perturbed_flows.len() != edge_count + node_count
        || snapshot.unperturbed_basic_flows.len() != edge_count + node_count
    {
        return Err(JsError::new(
            "polynomial primal simplex extended snapshot mismatch",
        ));
    }
    let nodes = polynomial_primal_nodes(graph, snapshot);
    let cycle_entities = snapshot
        .cycle
        .iter()
        .map(|reference| polynomial_primal_arc_entity(graph, reference.arc_index))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let entering_entity = snapshot
        .entering
        .map(|reference| polynomial_primal_arc_entity(graph, reference.arc_index))
        .transpose()?;
    let leaving_entity = snapshot
        .leaving_arc
        .map(|index| polynomial_primal_arc_entity(graph, index))
        .transpose()?;
    let edges = graph
        .edges()
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            let reduced = BigRational::from_integer(edge.cost().into())
                - &snapshot.premultipliers[edge.from().as_usize()]
                + &snapshot.premultipliers[edge.to().as_usize()];
            Ok(FlowPolynomialPrimalEdgeStateV1 {
                edge_id: edge.id().as_str().to_owned(),
                basis: polynomial_primal_scene_basis(snapshot.basis_states[index]),
                perturbed_flow: snapshot.perturbed_flows[index].to_string(),
                unperturbed_basic_flow: snapshot.unperturbed_basic_flows[index].to_string(),
                reduced_cost: polynomial_primal_rational(&reduced),
                in_cycle: cycle_entities.contains(edge.id().as_str()),
                entering: entering_entity.as_deref() == Some(edge.id().as_str()),
                leaving: leaving_entity.as_deref() == Some(edge.id().as_str()),
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    let artificial_edges = graph
        .nodes()
        .iter()
        .enumerate()
        .map(|(node, original)| {
            let index = edge_count + node;
            let entity_id = format!("artificial:{}", original.id().as_str());
            FlowPolynomialPrimalArtificialEdgeStateV1 {
                entity_id: entity_id.clone(),
                node_id: original.id().as_str().to_owned(),
                basis: polynomial_primal_scene_basis(snapshot.basis_states[index]),
                perturbed_flow: snapshot.perturbed_flows[index].to_string(),
                unperturbed_basic_flow: snapshot.unperturbed_basic_flows[index].to_string(),
                in_cycle: cycle_entities.contains(&entity_id),
                entering: entering_entity.as_ref() == Some(&entity_id),
                leaving: leaving_entity.as_ref() == Some(&entity_id),
            }
        })
        .collect();
    Ok(FlowPolynomialPrimalSimplexOverlayV1 {
        stage: polynomial_primal_scene_stage(snapshot.stage),
        phase: snapshot.phase.to_string(),
        epsilon: snapshot.epsilon.as_ref().map(polynomial_primal_rational),
        perturbation_scale: (node_count + 1).to_string(),
        nodes,
        edges,
        artificial_edges,
        entering: snapshot
            .entering
            .map(|reference| polynomial_primal_residual_ref(graph, reference))
            .transpose()?,
        leaving_entity,
        cycle: snapshot
            .cycle
            .iter()
            .map(|&reference| polynomial_primal_residual_ref(graph, reference))
            .collect::<Result<Vec<_>, _>>()?,
        delta: snapshot.delta.as_ref().map(polynomial_primal_rational),
        potential_shift: snapshot
            .potential_shift
            .as_ref()
            .map(polynomial_primal_rational),
    })
}

fn polynomial_primal_rational(value: &BigRational) -> FlowRationalV1 {
    FlowRationalV1 {
        numerator: value.numer().to_string(),
        denominator: value.denom().to_string(),
    }
}

fn polynomial_primal_arc_entity(
    graph: &flow::FlowNetwork,
    arc_index: usize,
) -> Result<String, JsError> {
    if let Some(edge) = graph.edges().get(arc_index) {
        return Ok(edge.id().as_str().to_owned());
    }
    graph
        .nodes()
        .get(arc_index.saturating_sub(graph.edges().len()))
        .map(|node| format!("artificial:{}", node.id().as_str()))
        .ok_or_else(|| JsError::new("polynomial primal simplex arc identity is absent"))
}

fn polynomial_primal_residual_ref(
    graph: &flow::FlowNetwork,
    reference: PolynomialPrimalResidualRef,
) -> Result<FlowPolynomialPrimalResidualRefV1, JsError> {
    let entity_id = polynomial_primal_arc_entity(graph, reference.arc_index)?;
    Ok(FlowPolynomialPrimalResidualRefV1 {
        original_edge_id: graph
            .edges()
            .get(reference.arc_index)
            .map(|edge| edge.id().as_str().to_owned()),
        entity_id,
        direction: if reference.forward {
            "forward"
        } else {
            "reverse"
        }
        .to_owned(),
    })
}

const fn polynomial_primal_scene_basis(
    state: PolynomialPrimalBasisState,
) -> FlowPolynomialPrimalBasisStateV1 {
    match state {
        PolynomialPrimalBasisState::Lower => FlowPolynomialPrimalBasisStateV1::Lower,
        PolynomialPrimalBasisState::Tree => FlowPolynomialPrimalBasisStateV1::Tree,
        PolynomialPrimalBasisState::Upper => FlowPolynomialPrimalBasisStateV1::Upper,
    }
}

const fn polynomial_primal_scene_stage(
    stage: PolynomialPrimalSimplexStage,
) -> FlowPolynomialPrimalSimplexStageV1 {
    match stage {
        PolynomialPrimalSimplexStage::Ready => FlowPolynomialPrimalSimplexStageV1::Ready,
        PolynomialPrimalSimplexStage::InitializeBasis => {
            FlowPolynomialPrimalSimplexStageV1::InitializeBasis
        }
        PolynomialPrimalSimplexStage::BeginScale => FlowPolynomialPrimalSimplexStageV1::BeginScale,
        PolynomialPrimalSimplexStage::SelectAdmissible => {
            FlowPolynomialPrimalSimplexStageV1::SelectAdmissible
        }
        PolynomialPrimalSimplexStage::InspectResidual => {
            FlowPolynomialPrimalSimplexStageV1::InspectResidual
        }
        PolynomialPrimalSimplexStage::Pivot => FlowPolynomialPrimalSimplexStageV1::Pivot,
        PolynomialPrimalSimplexStage::ModifyPremultipliers => {
            FlowPolynomialPrimalSimplexStageV1::ModifyPremultipliers
        }
        PolynomialPrimalSimplexStage::FinishScale => {
            FlowPolynomialPrimalSimplexStageV1::FinishScale
        }
        PolynomialPrimalSimplexStage::Optimal => FlowPolynomialPrimalSimplexStageV1::Optimal,
    }
}

#[allow(clippy::too_many_lines)]
fn polynomial_primal_simplex_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &PolynomialPrimalSimplexTraceEvent,
    event_id: u64,
    parent_phase_id: Option<u64>,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let extended_arc_ref = |reference: PolynomialPrimalResidualRef| {
        if let Some(edge) = graph.edges().get(reference.arc_index) {
            Some(FlowTraceEntityRefSceneV1::ResidualArc {
                edge_id: edge.id().as_str().to_owned(),
                direction: if reference.forward {
                    "forward"
                } else {
                    "reverse"
                }
                .to_owned(),
            })
        } else {
            graph
                .nodes()
                .get(reference.arc_index.saturating_sub(graph.edges().len()))
                .map(|node| FlowTraceEntityRefSceneV1::Node {
                    node_id: node.id().as_str().to_owned(),
                })
        }
    };
    let extended_arc_index_ref = |index: usize| {
        if let Some(edge) = graph.edges().get(index) {
            Some(FlowTraceEntityRefSceneV1::Edge {
                edge_id: edge.id().as_str().to_owned(),
            })
        } else {
            graph
                .nodes()
                .get(index.saturating_sub(graph.edges().len()))
                .map(|node| FlowTraceEntityRefSceneV1::Node {
                    node_id: node.id().as_str().to_owned(),
                })
        }
    };
    let entity_refs = match event.after.stage {
        PolynomialPrimalSimplexStage::InspectResidual => {
            let mut focus = event
                .after
                .inspected_residual
                .and_then(extended_arc_ref)
                .into_iter()
                .collect::<Vec<_>>();
            if focus.is_empty() {
                let inspected = event
                    .after
                    .inspected_arc
                    .ok_or_else(|| JsError::new("polynomial primal inspected arc is absent"))?;
                focus.extend(extended_arc_index_ref(inspected));
            }
            if focus.is_empty() {
                return Err(JsError::new(
                    "polynomial primal inspected arc cannot be projected",
                ));
            }
            focus
        }
        PolynomialPrimalSimplexStage::SelectAdmissible => event
            .after
            .entering
            .and_then(extended_arc_ref)
            .into_iter()
            .collect(),
        PolynomialPrimalSimplexStage::Pivot => event
            .after
            .leaving_arc
            .and_then(extended_arc_index_ref)
            .or_else(|| event.after.entering.and_then(extended_arc_ref))
            .into_iter()
            .collect(),
        PolynomialPrimalSimplexStage::ModifyPremultipliers => event
            .before
            .premultipliers
            .iter()
            .zip(&event.after.premultipliers)
            .enumerate()
            .find_map(|(index, (before, after))| {
                (before != after)
                    .then(|| graph.nodes().get(index))
                    .flatten()
            })
            .map(|node| FlowTraceEntityRefSceneV1::Node {
                node_id: node.id().as_str().to_owned(),
            })
            .into_iter()
            .collect(),
        PolynomialPrimalSimplexStage::Ready
        | PolynomialPrimalSimplexStage::InitializeBasis
        | PolynomialPrimalSimplexStage::BeginScale
        | PolynomialPrimalSimplexStage::FinishScale
        | PolynomialPrimalSimplexStage::Optimal => Vec::new(),
    };
    let detail = match event.after.stage {
        PolynomialPrimalSimplexStage::InspectResidual => {
            let kind = match event.after.scan_kind {
                Some(PolynomialPrimalScanKind::Scale) => "scale scan ordinal",
                Some(PolynomialPrimalScanKind::Admissible) => "admissible scan ordinal",
                Some(PolynomialPrimalScanKind::FundamentalCycle) => "cycle scan ordinal",
                Some(PolynomialPrimalScanKind::Optimality) => "optimality scan ordinal",
                None => return Err(JsError::new("polynomial primal scan kind is absent")),
            };
            let ordinal = event
                .after
                .metrics
                .admissible_arc_scans
                .checked_add(event.after.metrics.optimality_arc_scans)
                .and_then(|value| value.checked_add(event.after.metrics.cycle_arc_scans))
                .ok_or_else(|| JsError::new("polynomial primal scan ordinal overflow"))?;
            Some((kind, ordinal.to_string()))
        }
        PolynomialPrimalSimplexStage::Pivot => event
            .after
            .delta
            .as_ref()
            .filter(|value| value.is_integer())
            .map(|value| ("augmentation", value.to_string())),
        PolynomialPrimalSimplexStage::ModifyPremultipliers => event
            .after
            .potential_shift
            .as_ref()
            .filter(|value| value.is_integer())
            .map(|value| ("premultiplier shift", value.to_string())),
        _ => event
            .after
            .epsilon
            .as_ref()
            .filter(|value| value.is_integer())
            .map(|value| ("epsilon", value.to_string())),
    };
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: if event.after.stage == PolynomialPrimalSimplexStage::BeginScale {
            None
        } else {
            parent_phase_id.map(|value| value.to_string())
        },
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: match event.after.stage {
            PolynomialPrimalSimplexStage::InspectResidual => TraceGranularityV1::Micro,
            PolynomialPrimalSimplexStage::SelectAdmissible
            | PolynomialPrimalSimplexStage::Pivot
            | PolynomialPrimalSimplexStage::ModifyPremultipliers => TraceGranularityV1::Operation,
            _ => TraceGranularityV1::Phase,
        },
        pseudocode_line: polynomial_primal_pseudocode_line(event.after.stage).to_owned(),
        patch_count: polynomial_primal_patch_count(event)?,
        entity_refs,
        detail: detail.map(|(label, value)| FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value,
        }),
    })
}

fn polynomial_primal_patch_count(
    event: &PolynomialPrimalSimplexTraceEvent,
) -> Result<u32, JsError> {
    let changes = event
        .before
        .perturbed_flows
        .iter()
        .zip(&event.after.perturbed_flows)
        .filter(|(before, after)| before != after)
        .count()
        + event
            .before
            .premultipliers
            .iter()
            .zip(&event.after.premultipliers)
            .filter(|(before, after)| before != after)
            .count()
        + usize::from(event.before.basis_states != event.after.basis_states)
        + usize::from(event.before.n_star != event.after.n_star)
        + usize::from(event.before.stage != event.after.stage);
    u32::try_from(changes.max(1))
        .map_err(|_| JsError::new("polynomial primal simplex patch count overflow"))
}

const fn polynomial_primal_pseudocode_line(stage: PolynomialPrimalSimplexStage) -> &'static str {
    match stage {
        PolynomialPrimalSimplexStage::Ready => "orlin-pns:perturb-right-hand-side",
        PolynomialPrimalSimplexStage::InitializeBasis => "orlin-pns:artificial-star-basis",
        PolynomialPrimalSimplexStage::BeginScale => "orlin-pns:improve-approximation",
        PolynomialPrimalSimplexStage::SelectAdmissible => "orlin-pns:select-admissible-arc",
        PolynomialPrimalSimplexStage::InspectResidual => {
            "orlin-pns:inspect-one-extended-arc-in-the-active-search"
        }
        PolynomialPrimalSimplexStage::Pivot => "orlin-pns:primal-cycle-pivot",
        PolynomialPrimalSimplexStage::ModifyPremultipliers => {
            "orlin-pns:modify-epsilon-premultipliers"
        }
        PolynomialPrimalSimplexStage::FinishScale => "orlin-pns:epsilon-half-optimal",
        PolynomialPrimalSimplexStage::Optimal => "orlin-pns:return-independent-certificate",
    }
}

fn relaxed_mndc_scale_divisor(snapshot: &RelaxedMndcSnapshot) -> Result<i128, JsError> {
    let mut left = snapshot.epsilon.numerator();
    let mut right = snapshot.epsilon.denominator();
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    i128::try_from(left.max(1)).map_err(|_| JsError::new("relaxed-MNDC scale divisor overflow"))
}

fn relaxed_mndc_scaled_value(value: i128, divisor: i128) -> Result<String, JsError> {
    if divisor <= 0 || value % divisor != 0 {
        return Err(JsError::new(
            "relaxed-MNDC assignment evidence cannot be canonically rescaled",
        ));
    }
    Ok((value / divisor).to_string())
}

fn relaxed_mndc_residual_ref(id: &ResidualArcId) -> FlowResidualArcRefV1 {
    FlowResidualArcRefV1 {
        edge_id: id.original_edge().as_str().to_owned(),
        direction: match id.direction() {
            ResidualDirection::Forward => "forward",
            ResidualDirection::Reverse => "reverse",
        }
        .to_owned(),
    }
}

const fn relaxed_mndc_scene_stage(stage: RelaxedMndcStage) -> FlowRelaxedMndcStageV1 {
    match stage {
        RelaxedMndcStage::Ready => FlowRelaxedMndcStageV1::Ready,
        RelaxedMndcStage::Initialize => FlowRelaxedMndcStageV1::Initialize,
        RelaxedMndcStage::BeginPhase => FlowRelaxedMndcStageV1::BeginPhase,
        RelaxedMndcStage::InspectResidualArc => FlowRelaxedMndcStageV1::InspectResidualArc,
        RelaxedMndcStage::InspectAssignmentCell => FlowRelaxedMndcStageV1::InspectAssignmentCell,
        RelaxedMndcStage::SelectFamily => FlowRelaxedMndcStageV1::SelectFamily,
        RelaxedMndcStage::CancelFamily => FlowRelaxedMndcStageV1::CancelFamily,
        RelaxedMndcStage::PhaseOptimal => FlowRelaxedMndcStageV1::PhaseOptimal,
        RelaxedMndcStage::Optimal => FlowRelaxedMndcStageV1::Optimal,
    }
}

#[allow(clippy::too_many_lines)]
fn relaxed_mndc_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &RelaxedMndcTraceEvent,
    event_id: u64,
    parent_phase_id: Option<u64>,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let residual_ref = |arc: &ResidualArcId| FlowTraceEntityRefSceneV1::ResidualArc {
        edge_id: arc.original_edge().as_str().to_owned(),
        direction: match arc.direction() {
            ResidualDirection::Forward => "forward",
            ResidualDirection::Reverse => "reverse",
        }
        .to_owned(),
    };
    let entity_refs = if let Some(arc) = &event.after.active_residual_arc {
        vec![residual_ref(arc)]
    } else if let Some((row, column)) = event.after.active_assignment_cell {
        let nodes = if row == column {
            vec![row]
        } else {
            vec![row, column]
        };
        nodes
            .into_iter()
            .map(|node| {
                graph
                    .node(node)
                    .map(|node| FlowTraceEntityRefSceneV1::Node {
                        node_id: node.id().as_str().to_owned(),
                    })
                    .ok_or_else(|| JsError::new("relaxed-MNDC active assignment node is absent"))
            })
            .collect::<Result<Vec<_>, JsError>>()?
    } else {
        match event.after.stage {
            RelaxedMndcStage::SelectFamily => event
                .after
                .family
                .iter()
                .flat_map(|cycle| &cycle.arcs)
                .min()
                .map(residual_ref)
                .into_iter()
                .collect(),
            RelaxedMndcStage::CancelFamily => {
                let delta = event
                    .deltas
                    .first()
                    .copied()
                    .ok_or_else(|| JsError::new("relaxed-MNDC cancellation omitted delta"))?;
                let cycle = event
                    .after
                    .family
                    .first()
                    .ok_or_else(|| JsError::new("relaxed-MNDC cancellation omitted family"))?;
                let residual = ResidualState::from_flows(graph, &event.before.flows)
                    .map_err(|error| JsError::new(&error.to_string()))?;
                cycle
                    .arcs
                    .iter()
                    .filter(|arc| {
                        residual
                            .arc(arc)
                            .is_some_and(|residual_arc| residual_arc.capacity == delta)
                    })
                    .min()
                    .map(residual_ref)
                    .into_iter()
                    .collect()
            }
            RelaxedMndcStage::Ready
            | RelaxedMndcStage::Initialize
            | RelaxedMndcStage::BeginPhase
            | RelaxedMndcStage::InspectResidualArc
            | RelaxedMndcStage::InspectAssignmentCell
            | RelaxedMndcStage::PhaseOptimal
            | RelaxedMndcStage::Optimal => Vec::new(),
        }
    };
    let detail = if event.after.stage == RelaxedMndcStage::InspectResidualArc {
        Some(FlowTraceEventDetailSceneV1 {
            label: "residual arc scan".to_owned(),
            value: event.after.metrics.residual_arc_scans.to_string(),
        })
    } else if event.after.stage == RelaxedMndcStage::InspectAssignmentCell {
        Some(FlowTraceEventDetailSceneV1 {
            label: "assignment cell scan".to_owned(),
            value: event.after.metrics.assignment_cell_scans.to_string(),
        })
    } else if event.deltas.is_empty() {
        event
            .after
            .assignment_value
            .map(|value| {
                relaxed_mndc_scale_divisor(&event.after)
                    .and_then(|divisor| relaxed_mndc_scaled_value(value, divisor))
                    .map(|value| FlowTraceEventDetailSceneV1 {
                        label: "assignment value".to_owned(),
                        value,
                    })
            })
            .transpose()?
    } else {
        Some(FlowTraceEventDetailSceneV1 {
            label: "cycles canceled".to_owned(),
            value: event.deltas.len().to_string(),
        })
    };
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: if event.after.stage == RelaxedMndcStage::BeginPhase {
            None
        } else {
            parent_phase_id.map(|value| value.to_string())
        },
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity: match event.after.stage {
            RelaxedMndcStage::InspectResidualArc | RelaxedMndcStage::InspectAssignmentCell => {
                flow::TraceGranularityV1::Micro
            }
            RelaxedMndcStage::SelectFamily | RelaxedMndcStage::CancelFamily => {
                flow::TraceGranularityV1::Operation
            }
            _ => flow::TraceGranularityV1::Phase,
        },
        pseudocode_line: relaxed_mndc_pseudocode_line(event.after.stage).to_owned(),
        patch_count: relaxed_mndc_patch_count(event)?,
        entity_refs,
        detail,
    })
}

fn relaxed_mndc_patch_count(event: &RelaxedMndcTraceEvent) -> Result<u32, JsError> {
    let changes = event
        .before
        .flows
        .iter()
        .zip(&event.after.flows)
        .filter(|(before, after)| before != after)
        .count()
        + event
            .before
            .left_duals
            .iter()
            .zip(&event.after.left_duals)
            .filter(|(before, after)| before != after)
            .count()
        + event
            .before
            .right_duals
            .iter()
            .zip(&event.after.right_duals)
            .filter(|(before, after)| before != after)
            .count()
        + usize::from(event.before.epsilon != event.after.epsilon)
        + usize::from(event.before.assignment != event.after.assignment)
        + usize::from(event.before.family != event.after.family)
        + usize::from(event.before.active_residual_arc != event.after.active_residual_arc)
        + usize::from(event.before.active_assignment_cell != event.after.active_assignment_cell)
        + usize::from(event.before.stage != event.after.stage);
    u32::try_from(changes.max(1)).map_err(|_| JsError::new("relaxed-MNDC patch count overflow"))
}

const fn relaxed_mndc_pseudocode_line(stage: RelaxedMndcStage) -> &'static str {
    match stage {
        RelaxedMndcStage::Ready => "relaxed-mndc:ready-feasible-flow",
        RelaxedMndcStage::Initialize => "relaxed-mndc:initialize-epsilon-to-maximum-cost",
        RelaxedMndcStage::BeginPhase => "relaxed-mndc:halve-epsilon",
        RelaxedMndcStage::InspectResidualArc => "relaxed-mndc:inspect-positive-residual-arc",
        RelaxedMndcStage::InspectAssignmentCell => "relaxed-mndc:inspect-assignment-cell",
        RelaxedMndcStage::SelectFamily => "relaxed-mndc:solve-split-node-assignment",
        RelaxedMndcStage::CancelFamily => "relaxed-mndc:cancel-node-disjoint-family",
        RelaxedMndcStage::PhaseOptimal => "relaxed-mndc:certify-shifted-negative-cycle-absence",
        RelaxedMndcStage::Optimal => "relaxed-mndc:return-independent-certificate",
    }
}

fn set_relaxed_mndc_metrics(scene: &mut FlowCurrentSceneV9, snapshot: &RelaxedMndcSnapshot) {
    let metrics = snapshot.metrics;
    scene.set_relaxed_mndc_metrics(
        metrics.scaling_phases,
        metrics.assignment_solves,
        metrics.assignment_augmentations,
        metrics.assignment_cell_scans,
        metrics.residual_arc_scans,
        metrics.canceled_families,
        metrics.canceled_cycles,
        metrics.canceled_cycle_arcs,
        metrics.dropped_zero_cycles,
    );
}

fn double_scaling_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::DoubleScalingTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("double-scaling event count overflow"))?;
    let ready = ready_flow_scene(scenario)?;
    let mut projected = ready.clone();
    let base_overlay = double_scaling_overlay(graph, &run.base_snapshot)?;
    projected
        .apply_double_scaling_boundary(
            graph,
            &run.base_snapshot.display_flows,
            base_overlay,
            0,
            event_count,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    let base = flow_only_source_base(ready, &projected, event_count);
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    let mut parent_phase_id = None;
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("double-scaling trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("double-scaling event identity overflow"))?;
        if event.after.stage == DoubleScalingStage::StartCostPhase {
            parent_phase_id = Some(event_id);
        }
        let mut scene = base.clone();
        let overlay = double_scaling_overlay(graph, &event.after)?;
        scene
            .apply_double_scaling_boundary(
                graph,
                &event.after.display_flows,
                overlay,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(double_scaling_trace_event_scene(
            graph,
            event,
            event_id,
            parent_phase_id,
        )?);
        set_double_scaling_metrics(&mut scene, &event.after);
        if index + 1 == run.events.len() {
            scene.set_min_cost_flow_outcome(graph, &run.result.certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
        current = event.after.clone();
    }
    if current != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new("double-scaling trace final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn double_scaling_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &DoubleScalingSnapshot,
) -> Result<FlowDoubleScalingOverlayV1, JsError> {
    let variable_edges = graph
        .edges()
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.capacity() > edge.lower())
        .collect::<Vec<_>>();
    let node_count = graph.nodes().len() + variable_edges.len();
    if snapshot.transformed_flows.len() != variable_edges.len()
        || snapshot.prices.len() != node_count
        || snapshot.imbalances.len() != node_count
        || snapshot.cursors.len() != node_count
    {
        return Err(JsError::new(
            "double-scaling overlay shape does not match graph",
        ));
    }
    let mut nodes = Vec::with_capacity(node_count);
    for (index, node) in graph.nodes().iter().enumerate() {
        nodes.push(FlowDoubleScalingNodeStateV1 {
            entity_id: node.id().as_str().to_owned(),
            kind: FlowDoubleScalingNodeKindV1::Original,
            price: snapshot.prices[index].to_string(),
            imbalance: snapshot.imbalances[index].to_string(),
            cursor: snapshot.cursors[index].to_string(),
        });
    }
    for (variable, (_, edge)) in variable_edges.iter().enumerate() {
        let index = graph.nodes().len() + variable;
        nodes.push(FlowDoubleScalingNodeStateV1 {
            entity_id: edge.id().as_str().to_owned(),
            kind: FlowDoubleScalingNodeKindV1::Edge,
            price: snapshot.prices[index].to_string(),
            imbalance: snapshot.imbalances[index].to_string(),
            cursor: snapshot.cursors[index].to_string(),
        });
    }
    let edges = variable_edges
        .iter()
        .zip(&snapshot.transformed_flows)
        .map(|((_, edge), branches)| FlowDoubleScalingEdgeStateV1 {
            edge_id: edge.id().as_str().to_owned(),
            flow_branch: branches[0].to_string(),
            slack_branch: branches[1].to_string(),
        })
        .collect();
    let admissible_arcs = double_scaling_admissible_arcs(graph, snapshot, &variable_edges)?;
    Ok(FlowDoubleScalingOverlayV1 {
        stage: double_scaling_scene_stage(snapshot.stage),
        epsilon: snapshot.epsilon.to_string(),
        cost_multiplier: snapshot.cost_multiplier.to_string(),
        delta: snapshot.delta.to_string(),
        cost_phase: snapshot.cost_phase.to_string(),
        capacity_phase: snapshot.capacity_phase.to_string(),
        nodes,
        edges,
        admissible_arcs,
        active_path: snapshot
            .active_path
            .iter()
            .map(|id| double_scaling_arc_ref(graph, *id))
            .collect::<Result<_, _>>()?,
        inspected_arc: snapshot
            .inspected_arc
            .map(|id| double_scaling_arc_ref(graph, id))
            .transpose()?,
        selected_root: snapshot
            .selected_root
            .map(|node| double_scaling_node_identity(graph, node))
            .transpose()?,
        selected_deficit: snapshot
            .selected_deficit
            .map(|node| double_scaling_node_identity(graph, node))
            .transpose()?,
    })
}

fn double_scaling_admissible_arcs(
    graph: &flow::FlowNetwork,
    snapshot: &DoubleScalingSnapshot,
    variable_edges: &[(usize, &flow::FlowEdge)],
) -> Result<Vec<FlowDoubleScalingArcRefV1>, JsError> {
    let mut arcs = Vec::new();
    for (variable, (edge_index, edge)) in variable_edges.iter().enumerate() {
        let right = graph.nodes().len() + variable;
        let scaled_cost = i128::from(edge.cost())
            .checked_mul(snapshot.cost_multiplier)
            .ok_or_else(|| JsError::new("double-scaling overlay cost overflow"))?;
        for (branch, left, cost, branch_flow) in [
            (
                DoubleScalingBranch::Flow,
                edge.from().as_usize(),
                scaled_cost,
                snapshot.transformed_flows[variable][0],
            ),
            (
                DoubleScalingBranch::Slack,
                edge.to().as_usize(),
                0,
                snapshot.transformed_flows[variable][1],
            ),
        ] {
            let forward_reduced = cost
                .checked_add(snapshot.prices[left])
                .and_then(|value| value.checked_sub(snapshot.prices[right]))
                .ok_or_else(|| JsError::new("double-scaling overlay price overflow"))?;
            if forward_reduced < 0 {
                arcs.push(double_scaling_arc_ref(
                    graph,
                    DoubleScalingArcId {
                        edge_index: *edge_index,
                        branch,
                        direction: ResidualDirection::Forward,
                    },
                )?);
            }
            let reverse_reduced = (-cost)
                .checked_add(snapshot.prices[right])
                .and_then(|value| value.checked_sub(snapshot.prices[left]))
                .ok_or_else(|| JsError::new("double-scaling overlay price overflow"))?;
            if branch_flow > 0 && reverse_reduced < 0 {
                arcs.push(double_scaling_arc_ref(
                    graph,
                    DoubleScalingArcId {
                        edge_index: *edge_index,
                        branch,
                        direction: ResidualDirection::Reverse,
                    },
                )?);
            }
        }
    }
    Ok(arcs)
}

fn double_scaling_arc_ref(
    graph: &flow::FlowNetwork,
    id: DoubleScalingArcId,
) -> Result<FlowDoubleScalingArcRefV1, JsError> {
    let edge = graph
        .edges()
        .get(id.edge_index)
        .ok_or_else(|| JsError::new("double-scaling arc edge does not exist"))?;
    Ok(FlowDoubleScalingArcRefV1 {
        edge_id: edge.id().as_str().to_owned(),
        branch: match id.branch {
            DoubleScalingBranch::Flow => "flow",
            DoubleScalingBranch::Slack => "slack",
        }
        .to_owned(),
        direction: match id.direction {
            ResidualDirection::Forward => "forward",
            ResidualDirection::Reverse => "reverse",
        }
        .to_owned(),
    })
}

fn double_scaling_node_identity(
    graph: &flow::FlowNetwork,
    node: DoubleScalingNodeRef,
) -> Result<String, JsError> {
    match node {
        DoubleScalingNodeRef::Original(index) => graph
            .nodes()
            .get(index)
            .map(|node| format!("node:{}", node.id().as_str()))
            .ok_or_else(|| JsError::new("double-scaling original node does not exist")),
        DoubleScalingNodeRef::Edge(index) => graph
            .edges()
            .get(index)
            .map(|edge| format!("edge:{}", edge.id().as_str()))
            .ok_or_else(|| JsError::new("double-scaling edge node does not exist")),
    }
}

const fn double_scaling_scene_stage(stage: DoubleScalingStage) -> FlowDoubleScalingStageV1 {
    match stage {
        DoubleScalingStage::Ready => FlowDoubleScalingStageV1::Ready,
        DoubleScalingStage::Initialize => FlowDoubleScalingStageV1::Initialize,
        DoubleScalingStage::StartCostPhase => FlowDoubleScalingStageV1::StartCostPhase,
        DoubleScalingStage::StartCapacityPhase => FlowDoubleScalingStageV1::StartCapacityPhase,
        DoubleScalingStage::SelectRoot => FlowDoubleScalingStageV1::SelectRoot,
        DoubleScalingStage::InspectArc => FlowDoubleScalingStageV1::InspectArc,
        DoubleScalingStage::Advance => FlowDoubleScalingStageV1::Advance,
        DoubleScalingStage::Relabel => FlowDoubleScalingStageV1::Relabel,
        DoubleScalingStage::Retreat => FlowDoubleScalingStageV1::Retreat,
        DoubleScalingStage::Augment => FlowDoubleScalingStageV1::Augment,
        DoubleScalingStage::CompleteCostPhase => FlowDoubleScalingStageV1::CompleteCostPhase,
        DoubleScalingStage::Optimal => FlowDoubleScalingStageV1::Optimal,
    }
}

#[allow(clippy::too_many_lines)]
fn double_scaling_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &DoubleScalingTraceEvent,
    event_id: u64,
    parent_phase_id: Option<u64>,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let mut entity_refs = if event.after.stage == DoubleScalingStage::InspectArc {
        let arc = event
            .after
            .inspected_arc
            .ok_or_else(|| JsError::new("double-scaling scan is missing its inspected arc"))?;
        let edge = graph
            .edges()
            .get(arc.edge_index)
            .ok_or_else(|| JsError::new("double-scaling trace edge does not exist"))?;
        vec![FlowTraceEntityRefSceneV1::ResidualArc {
            edge_id: edge.id().as_str().to_owned(),
            direction: match arc.direction {
                ResidualDirection::Forward => "forward",
                ResidualDirection::Reverse => "reverse",
            }
            .to_owned(),
        }]
    } else {
        Vec::new()
    };
    let mut edge_indexes = if event.after.stage == DoubleScalingStage::InspectArc {
        Vec::new()
    } else {
        event
            .after
            .active_path
            .iter()
            .map(|arc| arc.edge_index)
            .collect::<Vec<_>>()
    };
    if edge_indexes.is_empty() && event.after.stage != DoubleScalingStage::InspectArc {
        for node in [event.after.selected_root, event.after.selected_deficit]
            .into_iter()
            .flatten()
        {
            if let DoubleScalingNodeRef::Edge(index) = node {
                edge_indexes.push(index);
            }
        }
    }
    edge_indexes.sort_unstable();
    edge_indexes.dedup();
    entity_refs.extend(
        edge_indexes
            .into_iter()
            .map(|index| {
                graph
                    .edges()
                    .get(index)
                    .map(|edge| FlowTraceEntityRefSceneV1::Edge {
                        edge_id: edge.id().as_str().to_owned(),
                    })
                    .ok_or_else(|| JsError::new("double-scaling trace edge does not exist"))
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    if event.after.stage != DoubleScalingStage::InspectArc {
        for node in [event.after.selected_root, event.after.selected_deficit]
            .into_iter()
            .flatten()
        {
            if let DoubleScalingNodeRef::Original(index) = node {
                let original = graph
                    .nodes()
                    .get(index)
                    .ok_or_else(|| JsError::new("double-scaling trace node does not exist"))?;
                entity_refs.push(FlowTraceEntityRefSceneV1::Node {
                    node_id: original.id().as_str().to_owned(),
                });
            }
        }
    }
    let detail = match event.after.stage {
        DoubleScalingStage::InspectArc => Some(FlowTraceEventDetailSceneV1 {
            label: "transformed residual-arc scan".to_owned(),
            value: event.after.metrics.transformed_arc_scans.to_string(),
        }),
        DoubleScalingStage::StartCostPhase | DoubleScalingStage::Relabel => {
            Some(FlowTraceEventDetailSceneV1 {
                label: "epsilon".to_owned(),
                value: event.after.epsilon.to_string(),
            })
        }
        DoubleScalingStage::StartCapacityPhase | DoubleScalingStage::Augment => {
            Some(FlowTraceEventDetailSceneV1 {
                label: "delta".to_owned(),
                value: event.after.delta.to_string(),
            })
        }
        _ => None,
    };
    let minimum_granularity = match event.after.stage {
        DoubleScalingStage::InspectArc => flow::TraceGranularityV1::Micro,
        DoubleScalingStage::Advance
        | DoubleScalingStage::Relabel
        | DoubleScalingStage::Retreat
        | DoubleScalingStage::Augment
        | DoubleScalingStage::SelectRoot => flow::TraceGranularityV1::Operation,
        _ => flow::TraceGranularityV1::Phase,
    };
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: if event.after.stage == DoubleScalingStage::StartCostPhase {
            None
        } else {
            parent_phase_id.map(|value| value.to_string())
        },
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity,
        pseudocode_line: double_scaling_pseudocode_line(event.after.stage).to_owned(),
        patch_count: double_scaling_patch_count(event)?,
        entity_refs,
        detail,
    })
}

fn double_scaling_patch_count(event: &DoubleScalingTraceEvent) -> Result<u32, JsError> {
    let changes = event
        .before
        .display_flows
        .iter()
        .zip(&event.after.display_flows)
        .filter(|(before, after)| before != after)
        .count()
        + event
            .before
            .transformed_flows
            .iter()
            .zip(&event.after.transformed_flows)
            .filter(|(before, after)| before != after)
            .count()
        + event
            .before
            .prices
            .iter()
            .zip(&event.after.prices)
            .filter(|(before, after)| before != after)
            .count()
        + event
            .before
            .imbalances
            .iter()
            .zip(&event.after.imbalances)
            .filter(|(before, after)| before != after)
            .count()
        + event
            .before
            .cursors
            .iter()
            .zip(&event.after.cursors)
            .filter(|(before, after)| before != after)
            .count()
        + usize::from(event.before.inspected_arc != event.after.inspected_arc)
        + usize::from(event.before.stage != event.after.stage);
    u32::try_from(changes.max(1)).map_err(|_| JsError::new("double-scaling patch count overflow"))
}

const fn double_scaling_pseudocode_line(stage: DoubleScalingStage) -> &'static str {
    match stage {
        DoubleScalingStage::Ready => "double-scaling:ready",
        DoubleScalingStage::Initialize => "double-scaling:build-transportation-network",
        DoubleScalingStage::StartCostPhase => "double-scaling:halve-epsilon-reset-and-shift",
        DoubleScalingStage::StartCapacityPhase => "double-scaling:start-delta-phase",
        DoubleScalingStage::SelectRoot => "double-scaling:select-large-excess-root",
        DoubleScalingStage::InspectArc => "double-scaling:inspect-transformed-residual-arc",
        DoubleScalingStage::Advance => "double-scaling:advance-admissible-path",
        DoubleScalingStage::Relabel => "double-scaling:decrease-dead-end-price",
        DoubleScalingStage::Retreat => "double-scaling:retreat-predecessor",
        DoubleScalingStage::Augment => "double-scaling:augment-exact-delta",
        DoubleScalingStage::CompleteCostPhase => "double-scaling:publish-feasible-flow",
        DoubleScalingStage::Optimal => "double-scaling:return-independent-certificate",
    }
}

fn set_double_scaling_metrics(scene: &mut FlowCurrentSceneV9, snapshot: &DoubleScalingSnapshot) {
    scene.set_double_scaling_metrics(
        snapshot.metrics.cost_phases,
        snapshot.metrics.capacity_phases,
        snapshot.metrics.path_searches,
        snapshot.metrics.advances,
        snapshot.metrics.relabels,
        snapshot.metrics.retreats,
        snapshot.metrics.augmentations,
        snapshot.metrics.transformed_arc_resets,
        snapshot.metrics.transformed_arc_scans,
    );
}

fn successive_shortest_augmenting_path_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::SuccessiveShortestAugmentingPathTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("flow trace event count overflow"))?;
    let base = trace_snapshot_source_base(scenario, graph, &run.base_snapshot, event_count)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut replay = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let mut scene = base.clone();
        scene
            .apply_trace_snapshot(graph, &replay, Some(event), event_count)
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        if index + 1 == run.events.len() {
            scene.set_min_cost_max_flow_outcome(graph, &run.result.certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    if replay != run.final_snapshot || run.events.is_empty() {
        return Err(JsError::new(
            "minimum-cost maximum-flow trace final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

fn tardos_framework_fast_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    result: &flow::TardosFrameworkResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let mut scene = ready_flow_scene(scenario)?;
    let overlay = tardos_framework_overlay(graph, &result.final_snapshot)?;
    scene
        .apply_tardos_framework_boundary(graph, &result.flows, overlay, 0, 0)
        .map_err(|error| JsError::new(&error.to_string()))?;
    scene.set_tardos_framework_metrics(result.metrics);
    scene
        .set_tardos_framework_outcome()
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(vec![ready_flow_scene(scenario)?, scene])
}

fn tardos_framework_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &TardosFrameworkTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("Tardos framework event count overflow"))?;
    let ready = ready_flow_scene(scenario)?;
    let mut projected = ready.clone();
    let base_overlay = tardos_framework_overlay(graph, &run.base_snapshot)?;
    projected
        .apply_tardos_framework_boundary(
            graph,
            &run.base_snapshot.flows,
            base_overlay,
            0,
            event_count,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
    let base = flow_only_source_base(ready, &projected, event_count);
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("Tardos framework trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("Tardos framework event identity overflow"))?;
        let mut scene = base.clone();
        let overlay = tardos_framework_overlay(graph, &event.after)?;
        scene
            .apply_tardos_framework_boundary(
                graph,
                &event.after.flows,
                overlay,
                event_id,
                event_count,
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.set_tardos_framework_metrics(event.after.metrics);
        scene.solve_status = FlowSolveStatusV1::Running;
        scene.trace_event = Some(tardos_framework_trace_event_scene(graph, event, event_id)?);
        if event.after.stage == TardosFrameworkStage::Complete {
            scene
                .set_tardos_framework_outcome()
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        current = event.after.clone();
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    if current != run.final_snapshot || run.final_snapshot != run.result.final_snapshot {
        return Err(JsError::new("Tardos framework final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

fn tardos_framework_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &TardosFrameworkSnapshot,
) -> Result<FlowTardosFrameworkOverlayV1, JsError> {
    if snapshot.potentials.len() != graph.nodes().len() {
        return Err(JsError::new("Tardos framework potential shape mismatch"));
    }
    let nodes = graph
        .nodes()
        .iter()
        .zip(&snapshot.potentials)
        .map(|(node, potential)| FlowTardosNodeStateV1 {
            node_id: node.id().as_str().to_owned(),
            potential: potential.to_string(),
        })
        .collect();
    let residual_arcs = snapshot
        .residual_arcs
        .iter()
        .map(|residual| FlowTardosResidualStateV1 {
            edge_id: residual.arc.original_edge().as_str().to_owned(),
            direction: tardos_direction(residual.arc.direction()).to_owned(),
            capacity: residual.capacity.to_string(),
            reduced_cost: residual.reduced_cost.to_string(),
            fixes_variable: residual.fixes_variable,
        })
        .collect();
    let fixed_variables = snapshot
        .fixed_variables
        .iter()
        .map(|fixed| FlowTardosFixedVariableV1 {
            edge_id: fixed.edge.as_str().to_owned(),
            bound: match fixed.bound {
                TardosFixedBound::Lower => "lower",
                TardosFixedBound::Upper => "upper",
            }
            .to_owned(),
            value: fixed.value.to_string(),
            direction: tardos_direction(fixed.witness_arc.direction()).to_owned(),
            reduced_cost: fixed.reduced_cost.to_string(),
        })
        .collect();
    Ok(FlowTardosFrameworkOverlayV1 {
        stage: tardos_framework_scene_stage(snapshot.stage),
        epsilon: snapshot.epsilon.to_string(),
        threshold: snapshot.threshold.to_string(),
        determinant_bound: "1".to_owned(),
        nodes,
        residual_arcs,
        fixed_variables,
    })
}

const fn tardos_framework_scene_stage(stage: TardosFrameworkStage) -> FlowTardosFrameworkStageV1 {
    match stage {
        TardosFrameworkStage::Ready => FlowTardosFrameworkStageV1::Ready,
        TardosFrameworkStage::ConstructFeasibleFlow => {
            FlowTardosFrameworkStageV1::ConstructFeasibleFlow
        }
        TardosFrameworkStage::MeasureEpsilon => FlowTardosFrameworkStageV1::MeasureEpsilon,
        TardosFrameworkStage::ClassifyFixedVariables => {
            FlowTardosFrameworkStageV1::ClassifyFixedVariables
        }
        TardosFrameworkStage::Complete => FlowTardosFrameworkStageV1::Complete,
    }
}

const fn tardos_direction(direction: ResidualDirection) -> &'static str {
    match direction {
        ResidualDirection::Forward => "forward",
        ResidualDirection::Reverse => "reverse",
    }
}

fn tardos_framework_trace_event_scene(
    graph: &flow::FlowNetwork,
    event: &TardosFrameworkTraceEvent,
    event_id: u64,
) -> Result<FlowTraceEventSceneV1, JsError> {
    let entity_refs = match event.catalog_id {
        "tardos-framework.scan-residual-arc" => event
            .after
            .residual_arcs
            .last()
            .map(|residual| {
                vec![FlowTraceEntityRefSceneV1::ResidualArc {
                    edge_id: residual.arc.original_edge().as_str().to_owned(),
                    direction: tardos_direction(residual.arc.direction()).to_owned(),
                }]
            })
            .unwrap_or_default(),
        "tardos-framework.inspect-fixed-variable" => event
            .after
            .fixed_variables
            .last()
            .map(|fixed| {
                vec![FlowTraceEntityRefSceneV1::ResidualArc {
                    edge_id: fixed.edge.as_str().to_owned(),
                    direction: tardos_direction(fixed.witness_arc.direction()).to_owned(),
                }]
            })
            .unwrap_or_default(),
        "tardos-framework.classify-fixed-variables" => event
            .after
            .fixed_variables
            .iter()
            .map(|fixed| FlowTraceEntityRefSceneV1::ResidualArc {
                edge_id: fixed.edge.as_str().to_owned(),
                direction: tardos_direction(fixed.witness_arc.direction()).to_owned(),
            })
            .collect(),
        _ => graph
            .nodes()
            .iter()
            .map(|node| FlowTraceEntityRefSceneV1::Node {
                node_id: node.id().as_str().to_owned(),
            })
            .collect(),
    };
    let detail = match event.catalog_id {
        "tardos-framework.scan-residual-arc" => event
            .after
            .residual_arcs
            .last()
            .map(|residual| ("reduced-cost", residual.reduced_cost)),
        "tardos-framework.measure-epsilon" => Some(("epsilon", event.after.epsilon)),
        "tardos-framework.inspect-fixed-variable" => event
            .after
            .fixed_variables
            .last()
            .map(|fixed| ("reduced-cost", fixed.reduced_cost)),
        "tardos-framework.classify-fixed-variables" => Some((
            "fixed-variables",
            i128::try_from(event.after.fixed_variables.len())
                .map_err(|_| JsError::new("Tardos framework fixed count overflow"))?,
        )),
        "tardos-framework.complete-primitive" => Some(("threshold", event.after.threshold)),
        "tardos-framework.construct-feasible-flow" => None,
        _ => return Err(JsError::new("unknown Tardos framework event identity")),
    };
    let minimum_granularity = match event.catalog_id {
        "tardos-framework.construct-feasible-flow" | "tardos-framework.complete-primitive" => {
            TraceGranularityV1::Phase
        }
        "tardos-framework.scan-residual-arc" | "tardos-framework.inspect-fixed-variable" => {
            TraceGranularityV1::Micro
        }
        "tardos-framework.measure-epsilon" | "tardos-framework.classify-fixed-variables" => {
            TraceGranularityV1::Operation
        }
        _ => return Err(JsError::new("unknown Tardos framework granularity")),
    };
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: None,
        catalog_id: event.catalog_id.to_owned(),
        minimum_granularity,
        pseudocode_line: tardos_framework_pseudocode_line(event.catalog_id)?.to_owned(),
        patch_count: tardos_framework_patch_count(event)?,
        entity_refs,
        detail: detail.map(|(label, value)| FlowTraceEventDetailSceneV1 {
            label: label.to_owned(),
            value: value.to_string(),
        }),
    })
}

fn tardos_framework_pseudocode_line(catalog_id: &str) -> Result<&'static str, JsError> {
    match catalog_id {
        "tardos-framework.construct-feasible-flow" => {
            Ok("tardos-framework:construct-feasible-b-flow")
        }
        "tardos-framework.scan-residual-arc" => {
            Ok("tardos-framework:price-one-positive-residual-direction")
        }
        "tardos-framework.measure-epsilon" => Ok("tardos-framework:measure-residual-epsilon"),
        "tardos-framework.inspect-fixed-variable" => {
            Ok("tardos-framework:inspect-strict-proximity-witness")
        }
        "tardos-framework.classify-fixed-variables" => {
            Ok("tardos-framework:fix-if-reduced-cost-exceeds-n-epsilon")
        }
        "tardos-framework.complete-primitive" => {
            Ok("tardos-framework:return-fixed-variable-witness")
        }
        _ => Err(JsError::new("unknown Tardos framework pseudocode identity")),
    }
}

fn tardos_framework_patch_count(event: &TardosFrameworkTraceEvent) -> Result<u32, JsError> {
    let changes = event
        .before
        .flows
        .iter()
        .zip(&event.after.flows)
        .filter(|(before, after)| before != after)
        .count()
        .saturating_add(
            event
                .before
                .residual_arcs
                .iter()
                .zip(&event.after.residual_arcs)
                .filter(|(before, after)| before != after)
                .count(),
        )
        .saturating_add(event.after.fixed_variables.len())
        .saturating_add(1);
    u32::try_from(changes.max(1)).map_err(|_| JsError::new("Tardos framework patch count overflow"))
}

fn prediction_assisted_epsilon_fast_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    target: &[i128],
    scaling_parameter: u32,
    result: &flow::PredictionAssistedEpsilonResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let mut scene = ready_flow_scene(scenario)?;
    apply_prediction_assisted_epsilon_scene_boundary(
        &mut scene,
        graph,
        target,
        &result.final_snapshot,
        scaling_parameter,
        Some(result.certificate_aligned_prediction_error),
        0,
        0,
    )?;
    scene.set_prediction_assisted_epsilon_metrics(result.metrics);
    scene.set_min_cost_flow_outcome(graph, &result.certificate);
    Ok(vec![ready_flow_scene(scenario)?, scene])
}

fn prediction_assisted_epsilon_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    target: &[i128],
    scaling_parameter: u32,
    run: &PredictionAssistedEpsilonTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let projection_width = graph
        .nodes()
        .len()
        .checked_mul(6)
        .and_then(|value| value.checked_add(graph.edges().len().saturating_mul(2)))
        .and_then(|value| value.checked_add(10))
        .ok_or_else(|| JsError::new("prediction-assisted trace projection overflow"))?;
    let projection_units = run
        .events
        .len()
        .checked_add(1)
        .and_then(|frames| frames.checked_mul(projection_width))
        .ok_or_else(|| JsError::new("prediction-assisted trace projection overflow"))?;
    if projection_units > PREDICTION_EPSILON_MAX_TRACE_PROJECTION_UNITS {
        return Ok(flow_trace_resource_limit_frames(
            scenario,
            &ready_flow_scene(scenario)?,
        ));
    }
    let event_count = u64::try_from(run.events.len())
        .map_err(|_| JsError::new("prediction-assisted event count overflow"))?;
    let base = ready_flow_scene(scenario)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut current = run.base_snapshot.clone();
    let mut parent_attempt_id = None;
    for (index, event) in run.events.iter().enumerate() {
        if event.before != current {
            return Err(JsError::new("prediction-assisted trace discontinuity"));
        }
        let event_id = u64::try_from(index + 1)
            .map_err(|_| JsError::new("prediction-assisted event identity overflow"))?;
        if event.stage == PredictionAssistedEpsilonStage::BeginAttempt {
            parent_attempt_id = Some(event_id);
        }
        let mut scene = base.clone();
        let prediction_error = (event.stage == PredictionAssistedEpsilonStage::Optimal)
            .then_some(run.result.certificate_aligned_prediction_error);
        apply_prediction_assisted_epsilon_scene_boundary(
            &mut scene,
            graph,
            target,
            &event.after,
            scaling_parameter,
            prediction_error,
            event_id,
            event_count,
        )?;
        scene.set_prediction_assisted_epsilon_metrics(event.after.metrics);
        scene.trace_event = Some(FlowTraceEventSceneV1 {
            event_id: event_id.to_string(),
            parent_phase_id: (event.stage != PredictionAssistedEpsilonStage::BeginAttempt)
                .then(|| parent_attempt_id.map(|value| value.to_string()))
                .flatten(),
            catalog_id: prediction_assisted_epsilon_catalog_id(event.stage).to_owned(),
            minimum_granularity: prediction_assisted_epsilon_granularity(event.stage),
            pseudocode_line: prediction_assisted_epsilon_pseudocode_line(event.stage).to_owned(),
            patch_count: prediction_assisted_epsilon_patch_count(event)?,
            entity_refs: prediction_assisted_epsilon_entity_refs(graph, &event.after)?,
            detail: event
                .detail
                .map(|(label, value)| FlowTraceEventDetailSceneV1 {
                    label: label.to_owned(),
                    value: value.to_string(),
                }),
        });
        scene.solve_status = FlowSolveStatusV1::Running;
        if event.stage == PredictionAssistedEpsilonStage::Optimal {
            scene.set_min_cost_flow_outcome(graph, &run.result.certificate);
        }
        current = event.after.clone();
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    if current != run.final_snapshot || run.final_snapshot != run.result.final_snapshot {
        return Err(JsError::new(
            "prediction-assisted epsilon final snapshot mismatch",
        ));
    }
    Ok(timeline.finish())
}

#[allow(clippy::too_many_arguments)]
fn apply_prediction_assisted_epsilon_scene_boundary(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    snapshot: &PredictionAssistedEpsilonSnapshot,
    scaling_parameter: u32,
    prediction_error: Option<i128>,
    event_id: u64,
    event_count: u64,
) -> Result<(), JsError> {
    let overlay =
        prediction_assisted_epsilon_overlay(graph, snapshot, scaling_parameter, prediction_error)?;
    scene
        .apply_prediction_assisted_epsilon_boundary(
            graph,
            target,
            &snapshot.flows,
            overlay,
            event_id,
            event_count,
        )
        .map_err(|error| JsError::new(&error.to_string()))
}

fn prediction_assisted_epsilon_overlay(
    graph: &flow::FlowNetwork,
    snapshot: &PredictionAssistedEpsilonSnapshot,
    scaling_parameter: u32,
    prediction_error: Option<i128>,
) -> Result<FlowPredictionAssistedEpsilonOverlayV1, JsError> {
    if snapshot.raw_predicted_prices.len() != graph.nodes().len()
        || snapshot.predicted_prices.len() != graph.nodes().len()
        || snapshot.prediction_clipped.len() != graph.nodes().len()
        || snapshot.prices.len() != graph.nodes().len()
        || snapshot.surpluses.len() != graph.nodes().len()
        || snapshot.scaled_costs.len() != graph.edges().len()
    {
        return Err(JsError::new("prediction-assisted snapshot shape mismatch"));
    }
    let nodes = graph
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| FlowPredictionAssistedEpsilonNodeStateV1 {
            node_id: node.id().as_str().to_owned(),
            raw_predicted_price: snapshot.raw_predicted_prices[index].to_string(),
            predicted_price: snapshot.predicted_prices[index].to_string(),
            prediction_clipped: snapshot.prediction_clipped[index],
            price: snapshot.prices[index].to_string(),
            surplus: snapshot.surpluses[index].to_string(),
            active: snapshot.active_node == Some(index),
        })
        .collect();
    let edges = graph
        .edges()
        .iter()
        .zip(&snapshot.scaled_costs)
        .map(|(edge, cost)| FlowPredictionAssistedEpsilonEdgeStateV1 {
            edge_id: edge.id().as_str().to_owned(),
            scaled_cost: cost.to_string(),
        })
        .collect();
    let active_node = snapshot
        .active_node
        .map(|index| {
            graph
                .nodes()
                .get(index)
                .map(|node| node.id().as_str().to_owned())
                .ok_or_else(|| JsError::new("prediction-assisted active node mismatch"))
        })
        .transpose()?;
    let active_arc = snapshot
        .active_arc
        .as_ref()
        .map(|arc| {
            if graph.edge_index(arc.original_edge()).is_none() {
                return Err(JsError::new("prediction-assisted active residual mismatch"));
            }
            Ok(FlowResidualArcRefV1 {
                edge_id: arc.original_edge().as_str().to_owned(),
                direction: match arc.direction() {
                    ResidualDirection::Forward => "forward",
                    ResidualDirection::Reverse => "reverse",
                }
                .to_owned(),
            })
        })
        .transpose()?;
    Ok(FlowPredictionAssistedEpsilonOverlayV1 {
        stage: prediction_assisted_epsilon_scene_stage(snapshot.stage),
        scaling_parameter: scaling_parameter.to_string(),
        attempt: snapshot.attempt.to_string(),
        maximum_attempt: snapshot.maximum_attempt.to_string(),
        exponent: snapshot.exponent.to_string(),
        scale_exponent: snapshot.scale_exponent.map(|value| value.to_string()),
        certificate_aligned_prediction_error: prediction_error.map(|value| value.to_string()),
        nodes,
        edges,
        active_node,
        active_arc,
    })
}

const fn prediction_assisted_epsilon_scene_stage(
    stage: PredictionAssistedEpsilonStage,
) -> FlowPredictionAssistedEpsilonStageV1 {
    match stage {
        PredictionAssistedEpsilonStage::PreprocessPrediction => {
            FlowPredictionAssistedEpsilonStageV1::PreprocessPrediction
        }
        PredictionAssistedEpsilonStage::BeginAttempt => {
            FlowPredictionAssistedEpsilonStageV1::BeginAttempt
        }
        PredictionAssistedEpsilonStage::InitializeScale => {
            FlowPredictionAssistedEpsilonStageV1::InitializeScale
        }
        PredictionAssistedEpsilonStage::SelectSurplus => {
            FlowPredictionAssistedEpsilonStageV1::SelectSurplus
        }
        PredictionAssistedEpsilonStage::InspectAdmissibleArc => {
            FlowPredictionAssistedEpsilonStageV1::InspectAdmissibleArc
        }
        PredictionAssistedEpsilonStage::InspectPriceBreakpointArc => {
            FlowPredictionAssistedEpsilonStageV1::InspectPriceBreakpointArc
        }
        PredictionAssistedEpsilonStage::Push => FlowPredictionAssistedEpsilonStageV1::Push,
        PredictionAssistedEpsilonStage::RaisePrice => {
            FlowPredictionAssistedEpsilonStageV1::RaisePrice
        }
        PredictionAssistedEpsilonStage::CompleteUpIteration => {
            FlowPredictionAssistedEpsilonStageV1::CompleteUpIteration
        }
        PredictionAssistedEpsilonStage::CompleteScale => {
            FlowPredictionAssistedEpsilonStageV1::CompleteScale
        }
        PredictionAssistedEpsilonStage::AbortAttempt => {
            FlowPredictionAssistedEpsilonStageV1::AbortAttempt
        }
        PredictionAssistedEpsilonStage::Optimal => FlowPredictionAssistedEpsilonStageV1::Optimal,
    }
}

const fn prediction_assisted_epsilon_catalog_id(
    stage: PredictionAssistedEpsilonStage,
) -> &'static str {
    match stage {
        PredictionAssistedEpsilonStage::PreprocessPrediction => {
            "prediction-assisted-epsilon-relaxation.preprocess-prediction"
        }
        PredictionAssistedEpsilonStage::BeginAttempt => {
            "prediction-assisted-epsilon-relaxation.begin-exponent-attempt"
        }
        PredictionAssistedEpsilonStage::InitializeScale => {
            "prediction-assisted-epsilon-relaxation.initialize-scaled-epsilon-cs"
        }
        PredictionAssistedEpsilonStage::SelectSurplus => {
            "prediction-assisted-epsilon-relaxation.select-positive-surplus"
        }
        PredictionAssistedEpsilonStage::InspectAdmissibleArc => {
            "prediction-assisted-epsilon-relaxation.inspect-admissible-arc"
        }
        PredictionAssistedEpsilonStage::InspectPriceBreakpointArc => {
            "prediction-assisted-epsilon-relaxation.inspect-price-breakpoint-arc"
        }
        PredictionAssistedEpsilonStage::Push => {
            "prediction-assisted-epsilon-relaxation.push-epsilon-balanced-arc"
        }
        PredictionAssistedEpsilonStage::RaisePrice => {
            "prediction-assisted-epsilon-relaxation.raise-price"
        }
        PredictionAssistedEpsilonStage::CompleteUpIteration => {
            "prediction-assisted-epsilon-relaxation.complete-up-iteration"
        }
        PredictionAssistedEpsilonStage::CompleteScale => {
            "prediction-assisted-epsilon-relaxation.complete-scale"
        }
        PredictionAssistedEpsilonStage::AbortAttempt => {
            "prediction-assisted-epsilon-relaxation.abort-exponent-attempt"
        }
        PredictionAssistedEpsilonStage::Optimal => {
            "prediction-assisted-epsilon-relaxation.certify-optimum"
        }
    }
}

const fn prediction_assisted_epsilon_pseudocode_line(
    stage: PredictionAssistedEpsilonStage,
) -> &'static str {
    match stage {
        PredictionAssistedEpsilonStage::PreprocessPrediction => {
            "prediction-assisted-epsilon-relaxation:algorithm-1-shift-clip"
        }
        PredictionAssistedEpsilonStage::BeginAttempt => {
            "prediction-assisted-epsilon-relaxation:remark-1-guess-t"
        }
        PredictionAssistedEpsilonStage::InitializeScale => {
            "prediction-assisted-epsilon-relaxation:initialize-empty-admissible"
        }
        PredictionAssistedEpsilonStage::SelectSurplus => {
            "prediction-assisted-epsilon-relaxation:select-positive-surplus"
        }
        PredictionAssistedEpsilonStage::InspectAdmissibleArc => {
            "prediction-assisted-epsilon-relaxation:test-epsilon-admissible-arc"
        }
        PredictionAssistedEpsilonStage::InspectPriceBreakpointArc => {
            "prediction-assisted-epsilon-relaxation:test-next-price-breakpoint"
        }
        PredictionAssistedEpsilonStage::Push => {
            "prediction-assisted-epsilon-relaxation:push-equality-arc"
        }
        PredictionAssistedEpsilonStage::RaisePrice => {
            "prediction-assisted-epsilon-relaxation:raise-to-next-breakpoint"
        }
        PredictionAssistedEpsilonStage::CompleteUpIteration => {
            "prediction-assisted-epsilon-relaxation:complete-up-iteration"
        }
        PredictionAssistedEpsilonStage::CompleteScale => {
            "prediction-assisted-epsilon-relaxation:descend-cost-scale"
        }
        PredictionAssistedEpsilonStage::AbortAttempt => {
            "prediction-assisted-epsilon-relaxation:remark-1-abort-guess"
        }
        PredictionAssistedEpsilonStage::Optimal => {
            "prediction-assisted-epsilon-relaxation:certify-original-cost"
        }
    }
}

const fn prediction_assisted_epsilon_granularity(
    stage: PredictionAssistedEpsilonStage,
) -> TraceGranularityV1 {
    match stage {
        PredictionAssistedEpsilonStage::InspectAdmissibleArc
        | PredictionAssistedEpsilonStage::InspectPriceBreakpointArc => TraceGranularityV1::Micro,
        PredictionAssistedEpsilonStage::SelectSurplus
        | PredictionAssistedEpsilonStage::Push
        | PredictionAssistedEpsilonStage::RaisePrice
        | PredictionAssistedEpsilonStage::CompleteUpIteration => TraceGranularityV1::Operation,
        PredictionAssistedEpsilonStage::PreprocessPrediction
        | PredictionAssistedEpsilonStage::BeginAttempt
        | PredictionAssistedEpsilonStage::InitializeScale
        | PredictionAssistedEpsilonStage::CompleteScale
        | PredictionAssistedEpsilonStage::AbortAttempt
        | PredictionAssistedEpsilonStage::Optimal => TraceGranularityV1::Phase,
    }
}

fn prediction_assisted_epsilon_entity_refs(
    graph: &flow::FlowNetwork,
    snapshot: &PredictionAssistedEpsilonSnapshot,
) -> Result<Vec<FlowTraceEntityRefSceneV1>, JsError> {
    let mut refs = Vec::new();
    if let Some(index) = snapshot.active_node {
        let node = graph
            .nodes()
            .get(index)
            .ok_or_else(|| JsError::new("prediction-assisted entity node mismatch"))?;
        refs.push(FlowTraceEntityRefSceneV1::Node {
            node_id: node.id().as_str().to_owned(),
        });
    }
    if let Some(arc) = &snapshot.active_arc {
        refs.push(FlowTraceEntityRefSceneV1::ResidualArc {
            edge_id: arc.original_edge().as_str().to_owned(),
            direction: match arc.direction() {
                ResidualDirection::Forward => "forward",
                ResidualDirection::Reverse => "reverse",
            }
            .to_owned(),
        });
    }
    Ok(refs)
}

fn prediction_assisted_epsilon_patch_count(
    event: &PredictionAssistedEpsilonTraceEvent,
) -> Result<u32, JsError> {
    let vector_changes = event
        .before
        .prices
        .iter()
        .zip(&event.after.prices)
        .filter(|(before, after)| before != after)
        .count()
        + event
            .before
            .scaled_costs
            .iter()
            .zip(&event.after.scaled_costs)
            .filter(|(before, after)| before != after)
            .count()
        + event
            .before
            .flows
            .iter()
            .zip(&event.after.flows)
            .filter(|(before, after)| before != after)
            .count()
        + event
            .before
            .surpluses
            .iter()
            .zip(&event.after.surpluses)
            .filter(|(before, after)| before != after)
            .count();
    let scalar_changes = usize::from(event.before.attempt != event.after.attempt)
        + usize::from(event.before.exponent != event.after.exponent)
        + usize::from(event.before.scale_exponent != event.after.scale_exponent)
        + usize::from(event.before.active_node != event.after.active_node)
        + usize::from(event.before.active_arc != event.after.active_arc)
        + usize::from(event.before.metrics != event.after.metrics);
    u32::try_from(vector_changes + scalar_changes)
        .map_err(|_| JsError::new("prediction-assisted patch count overflow"))
}

fn cost_scaling_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    run: &flow::CostScalingTraceResult,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    min_cost_trace_frames(
        scenario,
        graph,
        &run.result.certificate,
        &run.base_snapshot,
        &run.events,
        &run.final_snapshot,
    )
}

fn feasibility_trace_event_scene(
    snapshot: &flow::feasibility::FeasibilityTraceSnapshot,
    event: &flow::feasibility::FeasibilityTraceEvent,
    event_id: u64,
    project_public_entities: bool,
) -> Result<FlowTraceEventSceneV1, JsError> {
    Ok(FlowTraceEventSceneV1 {
        event_id: event_id.to_string(),
        parent_phase_id: None,
        catalog_id: feasibility_catalog_id(event.kind).to_owned(),
        minimum_granularity: feasibility_granularity(event.kind),
        pseudocode_line: feasibility_pseudocode_line(event.kind).to_owned(),
        patch_count: u32::try_from(event.patches.len())
            .map_err(|_| JsError::new("feasibility patch count overflow"))?,
        entity_refs: if project_public_entities {
            feasibility_entity_refs(event)
        } else {
            Vec::new()
        },
        detail: feasibility_event_detail(snapshot, event),
    })
}

fn feasibility_entity_refs(
    event: &flow::feasibility::FeasibilityTraceEvent,
) -> Vec<FlowTraceEntityRefSceneV1> {
    let mut refs = Vec::new();
    if let Some(flow::feasibility::FeasibilityNodeId::Original(node)) = &event.focus_node {
        refs.push(FlowTraceEntityRefSceneV1::Node {
            node_id: node.as_str().to_owned(),
        });
    }
    if let Some(focus) = &event.focus_arc
        && let flow::feasibility::FeasibilityArcId::Original(edge) = &focus.arc
    {
        refs.push(FlowTraceEntityRefSceneV1::ResidualArc {
            edge_id: edge.as_str().to_owned(),
            direction: feasibility_residual_direction(focus.direction).to_owned(),
        });
    }
    refs
}

fn feasibility_event_detail(
    snapshot: &flow::feasibility::FeasibilityTraceSnapshot,
    event: &flow::feasibility::FeasibilityTraceEvent,
) -> Option<FlowTraceEventDetailSceneV1> {
    use flow::feasibility::{FeasibilityTraceEventKind as Kind, FeasibilityTracePatch as Patch};
    let detail = match event.kind {
        Kind::AddOriginalArc | Kind::AddReturnArc | Kind::AddImbalanceArc => event
            .focus_arc
            .as_ref()
            .and_then(|focus| snapshot.arcs.iter().find(|arc| arc.id == focus.arc))
            .map(|arc| ("capacity", arc.capacity.to_string())),
        // The focused typed node is carried by the dedicated overlay. Trace
        // detail values are numeric by contract, so do not smuggle node IDs
        // through that scalar channel.
        Kind::InspectNodeImbalance | Kind::MarkReachable => None,
        Kind::InitializeSourceHeight | Kind::Relabel => event
            .focus_node
            .as_ref()
            .and_then(|node| snapshot.nodes.iter().find(|state| state.id == *node))
            .map(|node| ("height", node.height.to_string())),
        Kind::InspectSourceArc
        | Kind::InspectDischargeArc
        | Kind::InspectRelabelArc
        | Kind::InspectCutArc => event.focus_arc.as_ref().and_then(|focus| {
            snapshot
                .arcs
                .iter()
                .find(|arc| arc.id == focus.arc)
                .and_then(|arc| {
                    let residual = match focus.direction {
                        flow::feasibility::FeasibilityResidualDirection::Forward => {
                            arc.capacity.checked_sub(arc.flow)?
                        }
                        flow::feasibility::FeasibilityResidualDirection::Reverse => arc.flow,
                    };
                    Some(("residual-capacity", residual.to_string()))
                })
        }),
        Kind::Push => event.patches.iter().find_map(|patch| match patch {
            Patch::ArcFlow { before, after, .. } => {
                Some(("amount", after.abs_diff(*before).to_string()))
            }
            _ => None,
        }),
        Kind::ActivateNode | Kind::SelectActiveNode | Kind::CompleteDischarge => event
            .focus_node
            .as_ref()
            .and_then(|node| snapshot.nodes.iter().find(|state| state.id == *node))
            .map(|node| ("excess", node.excess.to_string())),
        Kind::AdvanceCurrentArc => event
            .focus_node
            .as_ref()
            .and_then(|node| snapshot.nodes.iter().find(|state| state.id == *node))
            .map(|node| ("current-arc", node.current_arc.to_string())),
        Kind::CompleteRouting | Kind::Feasible => Some(("routed", snapshot.routed.to_string())),
        Kind::ExtractOriginalFlow => event.focus_arc.as_ref().and_then(|focus| {
            let flow::feasibility::FeasibilityArcId::Original(edge) = &focus.arc else {
                return None;
            };
            snapshot
                .original_flows
                .iter()
                .find(|state| state.edge == *edge)
                .map(|state| ("flow", state.flow.to_string()))
        }),
        Kind::Infeasible => Some((
            "unsatisfied",
            snapshot
                .total_required
                .saturating_sub(snapshot.routed)
                .to_string(),
        )),
    };
    detail.map(|(label, value)| FlowTraceEventDetailSceneV1 {
        label: label.to_owned(),
        value,
    })
}

const fn feasibility_residual_direction(
    direction: flow::feasibility::FeasibilityResidualDirection,
) -> &'static str {
    match direction {
        flow::feasibility::FeasibilityResidualDirection::Forward => "forward",
        flow::feasibility::FeasibilityResidualDirection::Reverse => "reverse",
    }
}

const fn feasibility_granularity(
    kind: flow::feasibility::FeasibilityTraceEventKind,
) -> TraceGranularityV1 {
    use flow::feasibility::FeasibilityTraceEventKind as Kind;
    if matches!(
        kind,
        Kind::InitializeSourceHeight | Kind::Feasible | Kind::Infeasible
    ) {
        return TraceGranularityV1::Phase;
    }
    if matches!(
        kind,
        Kind::InspectNodeImbalance
            | Kind::InspectSourceArc
            | Kind::InspectDischargeArc
            | Kind::InspectRelabelArc
            | Kind::InspectCutArc
            | Kind::Push
            | Kind::AdvanceCurrentArc
            | Kind::ExtractOriginalFlow
    ) {
        return TraceGranularityV1::Micro;
    }
    TraceGranularityV1::Operation
}

const fn feasibility_catalog_id(
    kind: flow::feasibility::FeasibilityTraceEventKind,
) -> &'static str {
    use flow::feasibility::FeasibilityTraceEventKind as Kind;
    match kind {
        Kind::AddOriginalArc => "feasibility.add-original-arc",
        Kind::AddReturnArc => "feasibility.add-return-arc",
        Kind::InspectNodeImbalance => "feasibility.inspect-node-imbalance",
        Kind::AddImbalanceArc => "feasibility.add-imbalance-arc",
        Kind::InitializeSourceHeight => "feasibility.initialize-source-height",
        Kind::InspectSourceArc => "feasibility.inspect-source-arc",
        Kind::ActivateNode => "feasibility.activate-node",
        Kind::SelectActiveNode => "feasibility.select-active-node",
        Kind::InspectDischargeArc => "feasibility.inspect-discharge-arc",
        Kind::InspectRelabelArc => "feasibility.inspect-relabel-arc",
        Kind::Push => "feasibility.push",
        Kind::AdvanceCurrentArc => "feasibility.advance-current-arc",
        Kind::Relabel => "feasibility.relabel",
        Kind::CompleteDischarge => "feasibility.complete-discharge",
        Kind::CompleteRouting => "feasibility.complete-routing",
        Kind::InspectCutArc => "feasibility.inspect-cut-arc",
        Kind::MarkReachable => "feasibility.mark-reachable",
        Kind::ExtractOriginalFlow => "feasibility.extract-original-flow",
        Kind::Feasible => "feasibility.feasible",
        Kind::Infeasible => "feasibility.infeasible",
    }
}

const fn feasibility_pseudocode_line(
    kind: flow::feasibility::FeasibilityTraceEventKind,
) -> &'static str {
    use flow::feasibility::FeasibilityTraceEventKind as Kind;
    match kind {
        Kind::AddOriginalArc => "feasibility:shift-one-lower-bounded-edge",
        Kind::AddReturnArc => "feasibility:add-temporary-return-edge",
        Kind::InspectNodeImbalance => "feasibility:inspect-one-shifted-node-balance",
        Kind::AddImbalanceArc => "feasibility:add-one-super-terminal-edge",
        Kind::InitializeSourceHeight => "feasibility:initialize-auxiliary-push-relabel",
        Kind::InspectSourceArc => "feasibility:inspect-one-super-source-adjacency",
        Kind::ActivateNode => "feasibility:enqueue-one-active-node",
        Kind::SelectActiveNode => "feasibility:dequeue-one-active-node",
        Kind::InspectDischargeArc => "feasibility:inspect-one-current-adjacency",
        Kind::InspectRelabelArc => "feasibility:inspect-one-relabel-adjacency",
        Kind::Push => "feasibility:push-one-auxiliary-residual-arc",
        Kind::AdvanceCurrentArc => "feasibility:advance-one-current-arc",
        Kind::Relabel => "feasibility:raise-one-height",
        Kind::CompleteDischarge => "feasibility:complete-one-discharge",
        Kind::CompleteRouting => "feasibility:publish-routed-imbalance",
        Kind::InspectCutArc => "feasibility:inspect-one-cut-bfs-adjacency",
        Kind::MarkReachable => "feasibility:mark-one-cut-reachable-node",
        Kind::ExtractOriginalFlow => "feasibility:extract-one-original-flow",
        Kind::Feasible => "feasibility:certify-routed-imbalance",
        Kind::Infeasible => "feasibility:certify-unsatisfied-cut",
    }
}

fn min_cost_trace_frames(
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    certificate: &flow::MinCostFlowCertificate,
    base_snapshot: &FlowTraceSnapshot,
    events: &[flow::FlowTraceEvent],
    final_snapshot: &FlowTraceSnapshot,
) -> Result<Vec<FlowCurrentSceneV9>, JsError> {
    let event_count =
        u64::try_from(events.len()).map_err(|_| JsError::new("flow trace event count overflow"))?;
    let base = trace_snapshot_source_base(scenario, graph, base_snapshot, event_count)?;
    if let Some(frames) = trace_timeline_resource_limit_frames(scenario, &base, event_count)? {
        return Ok(frames);
    }
    let mut timeline = EagerFlowTimeline::new(base.clone())?;
    let mut replay = base_snapshot.clone();
    for (index, event) in events.iter().enumerate() {
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let mut scene = base.clone();
        scene
            .apply_trace_snapshot(graph, &replay, Some(event), event_count)
            .map_err(|error| JsError::new(&error.to_string()))?;
        scene.solve_status = FlowSolveStatusV1::Running;
        if index + 1 == events.len() {
            scene.set_min_cost_flow_outcome(graph, certificate);
        }
        if !timeline.try_push(scene)? {
            return Ok(flow_trace_resource_limit_frames(scenario, &base));
        }
    }
    if replay != *final_snapshot || events.is_empty() {
        return Err(JsError::new("min-cost trace final snapshot mismatch"));
    }
    Ok(timeline.finish())
}

#[allow(clippy::too_many_lines)]
fn apply_classical_min_cost_result(
    runner: ClassicalMinCostFlowRunner,
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::feasibility::FeasibilityExecution,
) -> Result<(), JsError> {
    match runner {
        ClassicalMinCostFlowRunner::SimpleCycleCanceling => {
            apply_simple_cycle_canceling_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::MinimumMeanCycleCanceling => {
            apply_minimum_mean_cycle_canceling_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::CancelAndTighten => {
            apply_cancel_tighten_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::RelaxedMostNegativeCycle => {
            apply_relaxed_mndc_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::EnhancedCapacityScaling => {
            apply_enhanced_capacity_scaling_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::Orlin => {
            apply_orlin_mcf_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::DualNetworkSimplex => {
            apply_dual_network_simplex_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::PolynomialDualNetworkSimplex => {
            apply_polynomial_dual_simplex_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::PolynomialPrimalNetworkSimplex => {
            apply_polynomial_primal_simplex_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::DoubleScaling => {
            apply_double_scaling_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::PotentialDijkstraSsp => {
            apply_potential_dijkstra_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::PrimalDual => {
            apply_primal_dual_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::BlockingFlowPrimalDual => {
            apply_blocking_primal_dual_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::CapacityScaling(preset) => {
            apply_capacity_scaling_result(preset, scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::OutOfKilter => {
            apply_out_of_kilter_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::Relaxation => {
            apply_relaxation_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::EpsilonRelaxation => {
            apply_epsilon_relaxation_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::CostScaling(preset) => {
            apply_cost_scaling_result(preset, scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::NetworkSimplex(NetworkSimplexRunner::Primal) => {
            apply_network_simplex_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::NetworkSimplex(NetworkSimplexRunner::DynamicTree) => {
            apply_dynamic_tree_network_simplex_result(scene, graph, target, feasibility)
        }
        ClassicalMinCostFlowRunner::SuccessiveShortestPath => {
            match flow::solve_successive_shortest_path_with_feasibility(graph, target, feasibility)
            {
                Ok(result) => scene
                    .apply_min_cost_flow_result(
                        graph,
                        &result.flows,
                        &result.certificate,
                        result.metrics.relaxation_passes,
                        result.metrics.residual_arc_scans,
                        result.metrics.augmentations,
                    )
                    .map_err(|error| JsError::new(&error.to_string())),
                Err(BellmanFordSspError::Feasibility(FeasibilityError::Infeasible(witness))) => {
                    check_balance_infeasibility(graph, target, &witness)
                        .map_err(|error| JsError::new(&error.to_string()))?;
                    scene.apply_infeasibility(&witness);
                    Ok(())
                }
                Err(
                    BellmanFordSspError::AdmissionLimit | BellmanFordSspError::AugmentationLimit,
                ) => {
                    scene.apply_resource_limit();
                    Ok(())
                }
                Err(error) => Err(JsError::new(&error.to_string())),
            }
        }
        ClassicalMinCostFlowRunner::BellmanFordSsp => {
            match flow::solve_bellman_ford_ssp_with_feasibility(graph, target, feasibility) {
                Ok(result) => scene
                    .apply_min_cost_flow_result(
                        graph,
                        &result.flows,
                        &result.certificate,
                        result.metrics.relaxation_passes,
                        result.metrics.residual_arc_scans,
                        result.metrics.augmentations,
                    )
                    .map_err(|error| JsError::new(&error.to_string())),
                Err(BellmanFordSspError::Feasibility(FeasibilityError::Infeasible(witness))) => {
                    check_balance_infeasibility(graph, target, &witness)
                        .map_err(|error| JsError::new(&error.to_string()))?;
                    scene.apply_infeasibility(&witness);
                    Ok(())
                }
                Err(
                    BellmanFordSspError::AdmissionLimit | BellmanFordSspError::AugmentationLimit,
                ) => {
                    scene.apply_resource_limit();
                    Ok(())
                }
                Err(error) => Err(JsError::new(&error.to_string())),
            }
        }
    }
}

fn apply_simple_cycle_canceling_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_simple_cycle_canceling_with_feasibility(graph, target, feasibility) {
        Ok(result) => {
            scene
                .apply_cycle_canceling_result(
                    graph,
                    &result.flows,
                    &result.certificate,
                    FlowCycleCancelingMetrics {
                        cycle_searches: result.metrics.cycle_searches,
                        relaxation_passes: result.metrics.relaxation_passes,
                        residual_arc_scans: result.metrics.residual_arc_scans,
                        canceled_cycles: result.metrics.canceled_cycles,
                    },
                )
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        Err(SimpleCycleCancelingError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(SimpleCycleCancelingError::AdmissionLimit | SimpleCycleCancelingError::WorkLimit) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_minimum_mean_cycle_canceling_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_minimum_mean_cycle_canceling_with_feasibility(graph, target, feasibility) {
        Ok(result) => {
            scene
                .apply_minimum_mean_cycle_canceling_result(
                    graph,
                    &result.flows,
                    &result.certificate,
                    FlowMinimumMeanCycleCancelingMetrics {
                        mean_cycle_searches: result.metrics.mean_cycle_searches,
                        dynamic_programming_rounds: result.metrics.dynamic_programming_rounds,
                        residual_arc_scans: result.metrics.residual_arc_scans,
                        canceled_cycles: result.metrics.canceled_cycles,
                    },
                )
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        Err(MinimumMeanCycleCancelingError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(
            MinimumMeanCycleCancelingError::AdmissionLimit
            | MinimumMeanCycleCancelingError::WorkLimit,
        ) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_cancel_tighten_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_cancel_and_tighten_with_feasibility(graph, target, feasibility) {
        Ok(result) => {
            let overlay = cancel_tighten_overlay(graph, &result.final_snapshot, None)?;
            scene
                .apply_cancel_tighten_boundary(graph, &result.flows, overlay, 1, 1)
                .map_err(|error| JsError::new(&error.to_string()))?;
            set_cancel_tighten_metrics(scene, &result.final_snapshot);
            scene.set_min_cost_flow_outcome(graph, &result.certificate);
        }
        Err(CancelTightenError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(CancelTightenError::AdmissionLimit | CancelTightenError::WorkLimit) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_relaxed_mndc_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_relaxed_mndc_with_feasibility(graph, target, feasibility) {
        Ok(result) => {
            let overlay = relaxed_mndc_overlay(graph, &result.final_snapshot, None)?;
            scene
                .apply_relaxed_mndc_boundary(graph, &result.flows, overlay, 1, 1)
                .map_err(|error| JsError::new(&error.to_string()))?;
            set_relaxed_mndc_metrics(scene, &result.final_snapshot);
            scene.set_min_cost_flow_outcome(graph, &result.certificate);
        }
        Err(RelaxedMndcError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(RelaxedMndcError::AdmissionLimit | RelaxedMndcError::WorkLimit) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_enhanced_capacity_scaling_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_enhanced_capacity_scaling_with_feasibility(graph, target, feasibility) {
        Ok(result) => {
            let overlay = enhanced_capacity_scaling_overlay(graph, &result.final_snapshot, None)?;
            scene
                .apply_enhanced_capacity_scaling_boundary(graph, &result.flows, overlay, 1, 1)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.set_enhanced_capacity_scaling_metrics(result.metrics);
            scene.set_min_cost_flow_outcome(graph, &result.certificate);
        }
        Err(EnhancedCapacityScalingError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(
            EnhancedCapacityScalingError::AdmissionLimit | EnhancedCapacityScalingError::WorkLimit,
        ) => scene.apply_resource_limit(),
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_orlin_mcf_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_orlin_mcf_with_feasibility(graph, target, feasibility) {
        Ok(result) => {
            let overlay = orlin_mcf_overlay(graph, &result.final_snapshot, None)?;
            scene
                .apply_orlin_mcf_boundary(graph, &result.flows, overlay, 1, 1)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.set_orlin_mcf_metrics(result.metrics);
            scene.set_min_cost_flow_outcome(graph, &result.certificate);
        }
        Err(OrlinMcfError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(OrlinMcfError::AdmissionLimit | OrlinMcfError::WorkLimit) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_dual_network_simplex_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_dual_network_simplex_with_feasibility(graph, target, feasibility) {
        Ok(result) => {
            let overlay = dual_network_simplex_overlay(graph, &result.final_snapshot)?;
            scene
                .apply_dual_network_simplex_boundary(graph, &result.flows, overlay, 1, 1)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene
                .set_dual_network_simplex_metrics(result.metrics)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.set_min_cost_flow_outcome(graph, &result.certificate);
        }
        Err(DualNetworkSimplexError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(DualNetworkSimplexError::AdmissionLimit | DualNetworkSimplexError::WorkLimit) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_polynomial_dual_simplex_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_polynomial_dual_network_simplex_with_feasibility(graph, target, feasibility) {
        Ok(result) => {
            let overlay = polynomial_dual_simplex_overlay(graph, &result.final_snapshot)?;
            scene
                .apply_polynomial_dual_simplex_boundary(graph, &result.flows, overlay, 1, 1)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene
                .set_polynomial_dual_simplex_metrics(result.metrics)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.set_min_cost_flow_outcome(graph, &result.certificate);
        }
        Err(PolynomialDualSimplexError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(PolynomialDualSimplexError::AdmissionLimit | PolynomialDualSimplexError::WorkLimit) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_polynomial_primal_simplex_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_polynomial_primal_network_simplex_with_feasibility(graph, target, feasibility)
    {
        Ok(result) => {
            let overlay = polynomial_primal_simplex_overlay(graph, &result.final_snapshot)?;
            scene
                .apply_polynomial_primal_simplex_boundary(graph, &result.flows, overlay, 1, 1)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene
                .set_polynomial_primal_simplex_metrics(result.metrics)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.set_min_cost_flow_outcome(graph, &result.certificate);
        }
        Err(PolynomialPrimalSimplexError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(
            PolynomialPrimalSimplexError::AdmissionLimit | PolynomialPrimalSimplexError::WorkLimit,
        ) => scene.apply_resource_limit(),
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_double_scaling_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_double_scaling_with_feasibility(graph, target, feasibility) {
        Ok(result) => {
            let overlay = double_scaling_overlay(graph, &result.final_snapshot)?;
            scene
                .apply_double_scaling_boundary(graph, &result.flows, overlay, 1, 1)
                .map_err(|error| JsError::new(&error.to_string()))?;
            set_double_scaling_metrics(scene, &result.final_snapshot);
            scene.set_min_cost_flow_outcome(graph, &result.certificate);
        }
        Err(DoubleScalingError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(DoubleScalingError::AdmissionLimit | DoubleScalingError::WorkLimit) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_potential_dijkstra_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_potential_dijkstra_ssp_with_feasibility(graph, target, feasibility) {
        Ok(result) => {
            scene
                .apply_potential_dijkstra_ssp_result(
                    graph,
                    &result.flows,
                    &result.certificate,
                    FlowPotentialDijkstraMetrics {
                        dijkstra_runs: result.metrics.dijkstra_runs,
                        settled_nodes: result.metrics.settled_nodes,
                        potential_updates: result.metrics.potential_updates,
                        residual_arc_scans: result.metrics.residual_arc_scans,
                        augmentations: result.metrics.augmentations,
                    },
                )
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        Err(PotentialDijkstraSspError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(
            PotentialDijkstraSspError::AdmissionLimit
            | PotentialDijkstraSspError::AugmentationLimit,
        ) => scene.apply_resource_limit(),
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_primal_dual_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_primal_dual_with_feasibility(graph, target, feasibility) {
        Ok(result) => scene
            .apply_potential_dijkstra_ssp_result(
                graph,
                &result.flows,
                &result.certificate,
                FlowPotentialDijkstraMetrics {
                    dijkstra_runs: result.metrics.dijkstra_runs,
                    settled_nodes: result.metrics.settled_nodes,
                    potential_updates: result.metrics.potential_updates,
                    residual_arc_scans: result.metrics.residual_arc_scans,
                    augmentations: result.metrics.augmentations,
                },
            )
            .map_err(|error| JsError::new(&error.to_string()))?,
        Err(PrimalDualError::Kernel(PotentialDijkstraSspError::Feasibility(
            FeasibilityError::Infeasible(witness),
        ))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(PrimalDualError::Kernel(
            PotentialDijkstraSspError::AdmissionLimit
            | PotentialDijkstraSspError::AugmentationLimit,
        )) => scene.apply_resource_limit(),
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_blocking_primal_dual_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_blocking_primal_dual_with_feasibility(graph, target, feasibility) {
        Ok(result) => scene
            .apply_blocking_primal_dual_result(
                graph,
                &result.flows,
                &result.certificate,
                FlowBlockingPrimalDualMetrics {
                    admissible_bfs_runs: result.metrics.admissible_bfs_runs,
                    slack_searches: result.metrics.slack_searches,
                    settled_nodes: result.metrics.settled_nodes,
                    residual_arc_scans: result.metrics.residual_arc_scans,
                    potential_updates: result.metrics.potential_updates,
                    blocking_flow_phases: result.metrics.blocking_flow_phases,
                    augmentations: result.metrics.augmentations,
                },
            )
            .map_err(|error| JsError::new(&error.to_string()))?,
        Err(BlockingPrimalDualError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(
            BlockingPrimalDualError::AdmissionLimit
            | BlockingPrimalDualError::WorkLimit
            | BlockingPrimalDualError::Trace(FlowTraceError::EventLimit),
        ) => scene.apply_resource_limit(),
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_capacity_scaling_result(
    preset: CapacityScalingRunner,
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    let result = match preset {
        CapacityScalingRunner::Capacity => {
            flow::solve_capacity_scaling_with_feasibility(graph, target, feasibility)
        }
        CapacityScalingRunner::Excess => {
            flow::solve_excess_scaling_mcf_with_feasibility(graph, target, feasibility)
        }
    };
    match result {
        Ok(result) => scene
            .apply_capacity_scaling_result(
                graph,
                &result.flows,
                &result.certificate,
                FlowCapacityScalingMetrics {
                    dijkstra_runs: result.metrics.dijkstra_runs,
                    settled_nodes: result.metrics.settled_nodes,
                    potential_updates: result.metrics.potential_updates,
                    residual_arc_scans: result.metrics.residual_arc_scans,
                    augmentations: result.metrics.augmentations,
                    scaling_phases: result.metrics.scaling_phases,
                    phase_saturations: result.metrics.phase_saturations,
                },
            )
            .map_err(|error| JsError::new(&error.to_string()))?,
        Err(CapacityScalingError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(CapacityScalingError::AdmissionLimit | CapacityScalingError::WorkLimit) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_cost_scaling_result(
    preset: CostScalingRunner,
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    let execution_preset = match preset {
        CostScalingRunner::CostScaling => flow::CostScalingExecutionPreset::CostScaling,
        CostScalingRunner::PushRelabel => flow::CostScalingExecutionPreset::PushRelabel,
        CostScalingRunner::AugmentRelabel => flow::CostScalingExecutionPreset::AugmentRelabel,
        CostScalingRunner::PartialAugmentRelabel => {
            flow::CostScalingExecutionPreset::PartialAugmentRelabel
        }
        CostScalingRunner::PriceRefinement => flow::CostScalingExecutionPreset::PriceRefinement,
        CostScalingRunner::ArcFixing => flow::CostScalingExecutionPreset::ArcFixing,
        CostScalingRunner::Generalized => flow::CostScalingExecutionPreset::GeneralizedPushRelabel,
    };
    let result = flow::solve_cost_scaling_preset_with_feasibility(
        graph,
        target,
        execution_preset,
        feasibility,
    );
    match result {
        Ok(result) => scene
            .apply_cost_scaling_result(
                graph,
                &result.flows,
                &result.certificate,
                FlowCostScalingMetrics {
                    refine_phases: result.metrics.refine_phases,
                    initial_saturations: result.metrics.initial_saturations,
                    pushes: result.metrics.pushes,
                    saturating_pushes: result.metrics.saturating_pushes,
                    nonsaturating_pushes: result.metrics.nonsaturating_pushes,
                    relabels: result.metrics.relabels,
                    active_vertex_selections: result.metrics.active_vertex_selections,
                    discharges: result.metrics.discharges,
                    residual_arc_scans: result.metrics.residual_arc_scans,
                    current_arc_advances: result.metrics.current_arc_advances,
                    path_searches: result.metrics.path_searches,
                    path_advances: result.metrics.path_advances,
                    retreats: result.metrics.retreats,
                    path_augmentations: result.metrics.path_augmentations,
                    deficit_augmentations: result.metrics.deficit_augmentations,
                    length_limit_augmentations: result.metrics.length_limit_augmentations,
                    price_refinement_attempts: result.metrics.price_refinement_attempts,
                    price_refinement_successes: result.metrics.price_refinement_successes,
                    price_refinement_failures: result.metrics.price_refinement_failures,
                    price_refinement_rounds: result.metrics.price_refinement_rounds,
                    price_refinement_relaxations: result.metrics.price_refinement_relaxations,
                    price_refinement_arc_scans: result.metrics.price_refinement_arc_scans,
                    arc_fixing_passes: result.metrics.arc_fixing_passes,
                    arcs_fixed: result.metrics.arcs_fixed,
                    arcs_unfixed: result.metrics.arcs_unfixed,
                    fix_ins: result.metrics.fix_ins,
                    fixed_arc_skips: result.metrics.fixed_arc_skips,
                    arc_fixing_recoveries: result.metrics.arc_fixing_recoveries,
                },
                &result.fixed_edges,
            )
            .map_err(|error| JsError::new(&error.to_string()))?,
        Err(CostScalingError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(CostScalingError::AdmissionLimit | CostScalingError::WorkLimit) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_out_of_kilter_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_out_of_kilter_with_feasibility(graph, target, feasibility) {
        Ok(result) => scene
            .apply_out_of_kilter_result(
                graph,
                &result.flows,
                &result.certificate,
                FlowOutOfKilterMetrics {
                    label_searches: result.metrics.label_searches,
                    residual_arc_scans: result.metrics.residual_arc_scans,
                    breakthroughs: result.metrics.breakthroughs,
                    price_updates: result.metrics.price_updates,
                    selected_arcs: result.metrics.selected_arcs,
                },
            )
            .map_err(|error| JsError::new(&error.to_string()))?,
        Err(OutOfKilterError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(OutOfKilterError::AdmissionLimit | OutOfKilterError::WorkLimit) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_relaxation_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_relaxation_with_feasibility(graph, target, feasibility) {
        Ok(result) => scene
            .apply_relaxation_result(
                graph,
                &result.flows,
                &result.prices,
                &result.certificate,
                result
                    .metrics
                    .projected_trace_metrics()
                    .ok_or_else(|| JsError::new("relaxation metric projection overflow"))?,
            )
            .map_err(|error| JsError::new(&error.to_string()))?,
        Err(RelaxationError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(
            RelaxationError::AdmissionLimit
            | RelaxationError::WorkLimit
            | RelaxationError::Trace(FlowTraceError::EventLimit),
        ) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_epsilon_relaxation_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_epsilon_relaxation_with_feasibility(graph, target, feasibility) {
        Ok(result) => scene
            .apply_relaxation_result(
                graph,
                &result.flows,
                &result.prices,
                &result.certificate,
                result.metrics.projected_trace_metrics(),
            )
            .map_err(|error| JsError::new(&error.to_string()))?,
        Err(EpsilonRelaxationError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(
            EpsilonRelaxationError::AdmissionLimit
            | EpsilonRelaxationError::WorkLimit
            | EpsilonRelaxationError::Trace(FlowTraceError::EventLimit),
        ) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_network_simplex_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_primal_network_simplex_with_feasibility(graph, target, feasibility) {
        Ok(result) => scene
            .apply_network_simplex_result(
                graph,
                &result.flows,
                &result.certificate,
                FlowNetworkSimplexMetrics {
                    pricing_searches: result.metrics.pricing_searches,
                    pricing_arc_scans: result.metrics.pricing_arc_scans,
                    pivots: result.metrics.pivots,
                    nondegenerate_pivots: result.metrics.nondegenerate_pivots,
                    degenerate_pivots: result.metrics.degenerate_pivots,
                    basis_exchanges: result.metrics.basis_exchanges,
                    bound_flips: result.metrics.bound_flips,
                    cycle_arc_scans: result.metrics.cycle_arc_scans,
                    potential_recomputations: result.metrics.potential_recomputations,
                },
            )
            .map_err(|error| JsError::new(&error.to_string()))?,
        Err(NetworkSimplexError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(NetworkSimplexError::AdmissionLimit | NetworkSimplexError::WorkLimit) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

fn apply_dynamic_tree_network_simplex_result(
    scene: &mut FlowCurrentSceneV9,
    graph: &flow::FlowNetwork,
    target: &[i128],
    feasibility: &mut flow::FeasibilityExecution,
) -> Result<(), JsError> {
    match flow::solve_dynamic_tree_network_simplex_with_feasibility(graph, target, feasibility) {
        Ok(result) => {
            let simplex = result.metrics.simplex;
            scene
                .apply_dynamic_tree_network_simplex_result(
                    graph,
                    &result.flows,
                    &result.certificate,
                    FlowDynamicTreeNetworkSimplexMetrics {
                        pricing_searches: simplex.pricing_searches,
                        pricing_arc_scans: simplex.pricing_arc_scans,
                        pivots: simplex.pivots,
                        nondegenerate_pivots: simplex.nondegenerate_pivots,
                        degenerate_pivots: simplex.degenerate_pivots,
                        basis_exchanges: simplex.basis_exchanges,
                        bound_flips: simplex.bound_flips,
                        cycle_arc_scans: simplex.cycle_arc_scans,
                        potential_recomputations: simplex.potential_recomputations,
                        path_minimum_queries: result.metrics.path_minimum_queries,
                        path_updates: result.metrics.path_updates,
                        directional_forest_rebuilds: result.metrics.directional_forest_rebuilds,
                        directional_value_validations: result.metrics.directional_value_validations,
                        tree_links: result.metrics.tree_links,
                        tree_cuts: result.metrics.tree_cuts,
                        tree_rebuilds: simplex.tree_rebuilds,
                    },
                )
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        Err(NetworkSimplexError::Feasibility(FeasibilityError::Infeasible(witness))) => {
            check_balance_infeasibility(graph, target, &witness)
                .map_err(|error| JsError::new(&error.to_string()))?;
            scene.apply_infeasibility(&witness);
        }
        Err(NetworkSimplexError::AdmissionLimit | NetworkSimplexError::WorkLimit) => {
            scene.apply_resource_limit();
        }
        Err(error) => return Err(JsError::new(&error.to_string())),
    }
    Ok(())
}

/// Statically dispatched plugin session. The build-time enum prevents runtime
/// plugin code loading and keeps plugin state isolated behind one Worker owner.
enum SessionKind {
    OrderedMap(Box<OrderedMapSession>),
    Flow(Box<FlowSession>),
}

/// Stateful, statically dispatched visualization session owned by one Worker.
#[wasm_bindgen]
pub struct WasmSession {
    kind: SessionKind,
}

#[wasm_bindgen]
impl WasmSession {
    /// Probes the bounded shared envelope and constructs its selected plugin.
    ///
    /// # Errors
    ///
    /// Rejects unsupported plugins and any plugin-local validation failure.
    #[wasm_bindgen(constructor)]
    pub fn new(scenario_json: &str) -> Result<WasmSession, JsError> {
        let envelope = decode_scenario_envelope(scenario_json.as_bytes())
            .map_err(|error| JsError::new(&error.to_string()))?;
        let kind = match envelope.plugin.as_str() {
            "ordered-map" => {
                SessionKind::OrderedMap(Box::new(OrderedMapSession::new(scenario_json)?))
            }
            "flow" => SessionKind::Flow(Box::new(FlowSession::new(scenario_json)?)),
            _ => return Err(JsError::new("unsupported Scenario plugin")),
        };
        Ok(Self { kind })
    }

    /// Stable plugin identifier selected from the strict Scenario envelope.
    #[must_use]
    pub fn plugin_id(&self) -> String {
        match &self.kind {
            SessionKind::OrderedMap(_) => "ordered-map".to_owned(),
            SessionKind::Flow(_) => "flow".to_owned(),
        }
    }

    /// Append-only plugin registry ordinal used by packet V6.
    #[must_use]
    pub fn plugin_ordinal(&self) -> u32 {
        match &self.kind {
            SessionKind::OrderedMap(_) => visualizer_core::plugin::ORDERED_MAP_PLUGIN_ORDINAL,
            SessionKind::Flow(_) => visualizer_core::plugin::FLOW_PLUGIN_ORDINAL,
        }
    }

    /// Transport container revision used by this plugin.
    #[must_use]
    pub fn transport_version(&self) -> u16 {
        match &self.kind {
            SessionKind::OrderedMap(_) => 5,
            SessionKind::Flow(_) => 6,
        }
    }

    /// Stable selected algorithm identifier.
    #[must_use]
    pub fn algorithm_id(&self) -> String {
        match &self.kind {
            SessionKind::OrderedMap(session) => session.algorithm_id(),
            SessionKind::Flow(session) => session.scenario.payload.algorithm.id.clone(),
        }
    }

    /// Number of finite ordered-map items; flow solve cursors are event IDs.
    #[must_use]
    pub fn item_count(&self) -> usize {
        match &self.kind {
            SessionKind::OrderedMap(session) => session.item_count(),
            SessionKind::Flow(session) => {
                if session.prepared {
                    session.prepared_event_count()
                } else {
                    1
                }
            }
        }
    }

    /// Current finite ordered-map cursor. Flow exposes its exact event cursor separately.
    #[must_use]
    pub fn cursor(&self) -> usize {
        match &self.kind {
            SessionKind::OrderedMap(session) => session.cursor(),
            SessionKind::Flow(session) => session.cursor,
        }
    }

    /// Exact plugin cursor serialized without JavaScript number conversion.
    #[must_use]
    pub fn event_cursor(&self) -> String {
        match &self.kind {
            SessionKind::OrderedMap(session) => session.cursor().to_string(),
            SessionKind::Flow(session) => session.cursor.to_string(),
        }
    }

    /// Highest finite ordered-map boundary indexed for seek.
    #[must_use]
    pub fn seek_coverage(&self) -> usize {
        match &self.kind {
            SessionKind::OrderedMap(session) => session.seek_coverage(),
            SessionKind::Flow(session) => session.committed_end,
        }
    }

    /// Serializes the current plugin scene without mutating committed state.
    ///
    /// # Errors
    ///
    /// Returns an error for a bounded scene serialization failure.
    pub fn current_frame_json(&self) -> Result<String, JsError> {
        match &self.kind {
            SessionKind::OrderedMap(session) => session.current_frame_json(),
            SessionKind::Flow(session) => session.current_frame_json(),
        }
    }

    /// Stages the next plugin operation without committing its cursor.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid state or bounded algorithm failure.
    pub fn stage_next_json(&mut self) -> Result<Option<String>, JsError> {
        match &mut self.kind {
            SessionKind::OrderedMap(session) => session.stage_next_json(),
            SessionKind::Flow(session) => session.stage_next_json(),
        }
    }

    /// Commits a previously transferred plugin candidate.
    pub fn commit_staged_next(&mut self) {
        match &mut self.kind {
            SessionKind::OrderedMap(session) => session.commit_staged_next(),
            SessionKind::Flow(session) => session.commit_staged_next(),
        }
    }

    /// Rejects a staged plugin candidate and preserves the committed boundary.
    ///
    /// # Errors
    ///
    /// Returns an error if ordered-map rollback reconstruction fails.
    pub fn discard_staged_next(&mut self) -> Result<(), JsError> {
        match &mut self.kind {
            SessionKind::OrderedMap(session) => session.discard_staged_next(),
            SessionKind::Flow(session) => {
                session.discard_staged_next();
                Ok(())
            }
        }
    }

    /// Synchronously seeks to a bounded plugin cursor.
    ///
    /// # Errors
    ///
    /// Rejects an invalid cursor or replay failure.
    pub fn seek_json(&mut self, target: usize) -> Result<String, JsError> {
        match &mut self.kind {
            SessionKind::OrderedMap(session) => session.seek_json(target),
            SessionKind::Flow(session) => seek_flow_json(session, target, MAX_FRAME_JSON_BYTES),
        }
    }

    /// Begins a cancellable seek candidate.
    ///
    /// # Errors
    ///
    /// Rejects a cursor outside the selected plugin's committed history.
    pub fn begin_seek(&mut self, target: usize) -> Result<(), JsError> {
        match &mut self.kind {
            SessionKind::OrderedMap(session) => session.begin_seek(target),
            SessionKind::Flow(session) => session.begin_seek(target),
        }
    }

    /// Resumes a seek with a bounded work chunk.
    ///
    /// # Errors
    ///
    /// Rejects zero work or a missing seek candidate.
    pub fn resume_seek_json(&mut self, max_items: usize) -> Result<String, JsError> {
        match &mut self.kind {
            SessionKind::OrderedMap(session) => session.resume_seek_json(max_items),
            SessionKind::Flow(session) => session.resume_seek_json(max_items),
        }
    }

    /// Commits the final staged seek publication.
    pub fn commit_staged_seek(&mut self) {
        match &mut self.kind {
            SessionKind::OrderedMap(session) => session.commit_staged_seek(),
            SessionKind::Flow(session) => session.commit_staged_seek(),
        }
    }

    /// Rejects an in-progress or final staged seek publication.
    pub fn discard_staged_seek(&mut self) {
        match &mut self.kind {
            SessionKind::OrderedMap(session) => session.discard_staged_seek(),
            SessionKind::Flow(session) => session.discard_staged_seek(),
        }
    }

    /// Advances the selected plugin's background seek index.
    ///
    /// # Errors
    ///
    /// Rejects zero work and propagates bounded replay failures.
    pub fn resume_seek_index(&mut self, max_items: usize) -> Result<bool, JsError> {
        match &mut self.kind {
            SessionKind::OrderedMap(session) => session.resume_seek_index(max_items),
            SessionKind::Flow(_) if max_items == 0 => {
                Err(JsError::new("seek-index chunk must be positive"))
            }
            SessionKind::Flow(_) => Ok(true),
        }
    }

    /// Returns the selected plugin's strict canonical Scenario.
    ///
    /// # Errors
    ///
    /// Returns an error if canonical serialization fails.
    pub fn scenario_json(&self) -> Result<String, JsError> {
        match &self.kind {
            SessionKind::OrderedMap(session) => session.scenario_json(),
            SessionKind::Flow(session) => session.scenario_json(),
        }
    }
}

impl WasmSession {
    #[cfg(test)]
    fn stage_next_json_with_limit(
        &mut self,
        frame_json_limit: usize,
    ) -> Result<Option<String>, String> {
        match &mut self.kind {
            SessionKind::OrderedMap(session) => {
                session.stage_next_json_with_limit(frame_json_limit)
            }
            SessionKind::Flow(session) => session
                .stage_next_json()
                .map_err(|error| format!("{error:?}")),
        }
    }

    #[cfg(test)]
    fn resume_seek_json_with_limit(
        &mut self,
        max_items: usize,
        frame_json_limit: usize,
    ) -> Result<String, String> {
        match &mut self.kind {
            SessionKind::OrderedMap(session) => {
                session.resume_seek_json_with_limit(max_items, frame_json_limit)
            }
            SessionKind::Flow(session) => session
                .resume_seek_json(max_items)
                .map_err(|error| format!("{error:?}")),
        }
    }

    #[cfg(test)]
    fn ordered_map_mut(&mut self) -> &mut OrderedMapSession {
        match &mut self.kind {
            SessionKind::OrderedMap(session) => session,
            SessionKind::Flow(_) => panic!("test expected an ordered-map session"),
        }
    }
}

fn parse_key(key: &str) -> Result<u64, String> {
    key.parse::<u64>()
        .map_err(|_| "operation key is not a canonical u64".to_owned())
}

fn convert_operation(operation: &Operation) -> Result<ModelOperation, String> {
    match operation {
        Operation::Insert { key, value } => Ok(ModelOperation::Insert {
            key: parse_key(key)?,
            value: value.clone(),
        }),
        Operation::Remove { key } => Ok(ModelOperation::Remove {
            key: parse_key(key)?,
        }),
        Operation::Get { key } => Ok(ModelOperation::Get {
            key: parse_key(key)?,
        }),
        Operation::LowerBound { key } => Ok(ModelOperation::LowerBound {
            key: parse_key(key)?,
        }),
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
