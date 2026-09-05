/**
 * Canonical algorithm classification for the flow presentation layer.
 *
 * Rendering, legends, and inspector projections consume these predicates rather
 * than maintaining their own partially overlapping algorithm-id switches.
 */

const PUSH_RELABEL_ALGORITHMS = new Set([
	"generic-push-relabel",
	"fifo-push-relabel",
	"relabel-to-front",
	"highest-label-push-relabel",
	"partial-augment-relabel-max-flow",
	"current-arc-heuristic",
	"global-relabel-heuristic",
	"gap-relabel-heuristic",
	"excess-scaling-push-relabel",
	"dynamic-tree-push-relabel",
	"synchronous-parallel-push-relabel",
]);
export const PARTIAL_AUGMENT_RELABEL_ALGORITHM =
	"partial-augment-relabel-max-flow";
export const EXCESS_SCALING_PUSH_RELABEL_ALGORITHM =
	"excess-scaling-push-relabel";
export const SYNCHRONOUS_PUSH_RELABEL_ALGORITHM =
	"synchronous-parallel-push-relabel";
export const WARM_START_PUSH_RELABEL_ALGORITHM = "warm-start-push-relabel";
const RELABEL_HEURISTIC_ALGORITHMS = new Set([
	"global-relabel-heuristic",
	"gap-relabel-heuristic",
]);
const BLOCKING_PREFLOW_ALGORITHMS = new Set(["karzanov-preflow", "mpm"]);
export const DYNAMIC_TREE_BLOCKING_ALGORITHM = "dynamic-tree-blocking-flow";
export const DYNAMIC_TREE_PUSH_RELABEL_ALGORITHM = "dynamic-tree-push-relabel";
export const GOLDBERG_RAO_ALGORITHM = "goldberg-rao";
export const BINARY_BLOCKING_ALGORITHM = "binary-blocking-flow";
const DISTANCE_DIRECTED_ALGORITHMS = new Set([
	"distance-directed-augmenting-path",
	"distance-directed-scaling-augmenting-path",
]);
const PSEUDOFLOW_ALGORITHMS = new Set([
	"hochbaum-pseudoflow",
	"pseudoflow-simplex",
]);
export const PSEUDOFLOW_SIMPLEX_ALGORITHM = "pseudoflow-simplex";
export const POTENTIAL_DIJKSTRA_SSP_ALGORITHM = "potential-dijkstra-ssp";
export const SUCCESSIVE_SHORTEST_AUGMENTING_PATH_ALGORITHM =
	"successive-shortest-augmenting-path";
export const PRIMAL_DUAL_MCF_ALGORITHM = "primal-dual-mcf";
export const BLOCKING_PRIMAL_DUAL_ALGORITHM = "blocking-flow-primal-dual";
export const CAPACITY_SCALING_MCF_ALGORITHM = "capacity-scaling-mcf";
export const EXCESS_SCALING_MCF_ALGORITHM = "excess-scaling-mcf";
const COST_SCALING_ALGORITHMS = new Set([
	"cost-scaling",
	"cost-scaling-push-relabel",
	"augment-relabel",
	"partial-augment-relabel-mcf",
	"price-refinement",
	"arc-fixing",
	"generalized-cost-scaling",
]);
export const PRICE_REFINEMENT_ALGORITHM = "price-refinement";
export const ARC_FIXING_ALGORITHM = "arc-fixing";
const AUGMENT_RELABEL_MCF_ALGORITHMS = new Set([
	"augment-relabel",
	"partial-augment-relabel-mcf",
]);
export const SIMPLE_CYCLE_CANCELING_ALGORITHM = "simple-cycle-canceling";
export const MINIMUM_MEAN_CYCLE_CANCELING_ALGORITHM =
	"minimum-mean-cycle-canceling";
export const CANCEL_AND_TIGHTEN_ALGORITHM = "cancel-and-tighten";
export const RELAXED_MNDC_ALGORITHM = "relaxed-most-negative-cycle";
export const ENHANCED_CAPACITY_SCALING_ALGORITHM = "enhanced-capacity-scaling";
export const ORLIN_MCF_ALGORITHM = "orlin-mcf";
export const ORLIN_MAX_FLOW_ALGORITHM = "orlin-max-flow";
export const DUAL_NETWORK_SIMPLEX_ALGORITHM = "dual-network-simplex";
export const POLYNOMIAL_DUAL_SIMPLEX_ALGORITHM =
	"polynomial-dual-network-simplex";
export const POLYNOMIAL_PRIMAL_SIMPLEX_ALGORITHM =
	"polynomial-primal-network-simplex";
export const DOUBLE_SCALING_ALGORITHM = "double-scaling";
export const SEGMENT_EXPANDED_CONVEX_MCF_ALGORITHM =
	"segment-expanded-convex-mcf";
export const CONVEX_COST_SCALING_ALGORITHM = "convex-cost-scaling";
export const CONVEX_NETWORK_SIMPLEX_ALGORITHM = "convex-network-simplex";
const CONVEX_COST_ALGORITHMS = new Set([
	SEGMENT_EXPANDED_CONVEX_MCF_ALGORITHM,
	CONVEX_COST_SCALING_ALGORITHM,
	CONVEX_NETWORK_SIMPLEX_ALGORITHM,
]);
export const PRIMAL_NETWORK_SIMPLEX_ALGORITHM = "primal-network-simplex";
export const DYNAMIC_TREE_NETWORK_SIMPLEX_ALGORITHM =
	"dynamic-tree-network-simplex";
const NETWORK_SIMPLEX_ALGORITHMS = new Set([
	PRIMAL_NETWORK_SIMPLEX_ALGORITHM,
	DYNAMIC_TREE_NETWORK_SIMPLEX_ALGORITHM,
]);
export const OUT_OF_KILTER_ALGORITHM = "out-of-kilter";
export const RELAXATION_ALGORITHM = "relaxation";
export const EPSILON_RELAXATION_ALGORITHM = "epsilon-relaxation";
export const PREDICTION_ASSISTED_EPSILON_ALGORITHM =
	"prediction-assisted-epsilon-relaxation";
export const TARDOS_FRAMEWORK_ALGORITHM = "tardos-framework";
export const HOPCROFT_KARP_ALGORITHM = "hopcroft-karp";
export const HUNGARIAN_ALGORITHM = "hungarian";
export const AUCTION_ALGORITHM = "auction";
const TRANSPORTATION_ALGORITHMS = new Set(["transportation-simplex", "modi"]);
export const HASSIN_ST_PLANAR_ALGORITHM = "hassin-st-planar";
export const BORRADAILE_KLEIN_PLANAR_ALGORITHM = "borradaile-klein-planar";

export const POTENTIAL_DIJKSTRA_PRICE_EVENTS = new Set([
	"potential-dijkstra-ssp.initial-potentials",
	"potential-dijkstra-ssp.update-potentials",
	"potential-dijkstra-ssp.augment",
	"potential-dijkstra-ssp.optimal",
	"successive-shortest-augmenting-path.initial-potentials",
	"successive-shortest-augmenting-path.update-potentials",
	"successive-shortest-augmenting-path.augment",
	"successive-shortest-augmenting-path.optimal",
	"primal-dual-mcf.initialize-dual",
	"primal-dual-mcf.tighten-dual",
	"primal-dual-mcf.augment-admissible-path",
	"primal-dual-mcf.optimal",
	"blocking-flow-primal-dual.initialize-dual",
	"blocking-flow-primal-dual.tighten-dual",
	"blocking-flow-primal-dual.augment-admissible-path",
	"blocking-flow-primal-dual.optimal",
	"capacity-scaling-mcf.initialize-potentials",
	"capacity-scaling-mcf.start-scaling-phase",
	"capacity-scaling-mcf.saturate-negative-arc",
	"capacity-scaling-mcf.update-potentials",
	"capacity-scaling-mcf.augment",
	"capacity-scaling-mcf.complete-scaling-phase",
	"capacity-scaling-mcf.optimal",
	"excess-scaling-mcf.initialize-potentials",
	"excess-scaling-mcf.start-excess-phase",
	"excess-scaling-mcf.shortest-large-excess-path",
	"excess-scaling-mcf.no-reachable-large-deficit",
	"excess-scaling-mcf.update-potentials",
	"excess-scaling-mcf.augment-exact-delta",
	"excess-scaling-mcf.complete-excess-phase",
	"excess-scaling-mcf.optimal",
]);

export const BLOCKING_PRIMAL_DUAL_LEVEL_EVENTS = new Set([
	"blocking-flow-primal-dual.build-admissible-levels",
	"blocking-flow-primal-dual.complete-blocking-flow",
]);

export const MPM_POTENTIAL_EVENTS = new Set([
	"mpm.select-potential",
	"mpm.push-forward",
	"mpm.push-backward",
	"mpm.remove-vertex",
]);

export function isHopcroftKarpAlgorithm(id: string | undefined): boolean {
	return id === HOPCROFT_KARP_ALGORITHM;
}
export function isHungarianAlgorithm(id: string | undefined): boolean {
	return id === HUNGARIAN_ALGORITHM;
}
export function isAuctionAlgorithm(id: string | undefined): boolean {
	return id === AUCTION_ALGORITHM;
}
export function isTransportationAlgorithm(id: string | undefined): boolean {
	return id !== undefined && TRANSPORTATION_ALGORITHMS.has(id);
}
export function isHassinStPlanarAlgorithm(id: string | undefined): boolean {
	return id === HASSIN_ST_PLANAR_ALGORITHM;
}
export function isBorradaileKleinPlanarAlgorithm(
	id: string | undefined,
): boolean {
	return id === BORRADAILE_KLEIN_PLANAR_ALGORITHM;
}
export function isPushRelabelAlgorithm(id: string | undefined): boolean {
	return id !== undefined && PUSH_RELABEL_ALGORITHMS.has(id);
}
export function isPartialAugmentRelabelAlgorithm(
	id: string | undefined,
): boolean {
	return id === PARTIAL_AUGMENT_RELABEL_ALGORITHM;
}
export function isExcessScalingPushRelabelAlgorithm(
	id: string | undefined,
): boolean {
	return id === EXCESS_SCALING_PUSH_RELABEL_ALGORITHM;
}
export function isSynchronousPushRelabelAlgorithm(
	id: string | undefined,
): boolean {
	return id === SYNCHRONOUS_PUSH_RELABEL_ALGORITHM;
}
export function isWarmStartPushRelabelAlgorithm(
	id: string | undefined,
): boolean {
	return id === WARM_START_PUSH_RELABEL_ALGORITHM;
}
export function isRelabelHeuristicAlgorithm(id: string | undefined): boolean {
	return id !== undefined && RELABEL_HEURISTIC_ALGORITHMS.has(id);
}
export function isBlockingPreflowAlgorithm(id: string | undefined): boolean {
	return id !== undefined && BLOCKING_PREFLOW_ALGORITHMS.has(id);
}
export function isDynamicTreeBlockingAlgorithm(
	id: string | undefined,
): boolean {
	return id === DYNAMIC_TREE_BLOCKING_ALGORITHM;
}
export function isDynamicTreePushRelabelAlgorithm(
	id: string | undefined,
): boolean {
	return id === DYNAMIC_TREE_PUSH_RELABEL_ALGORITHM;
}
export function isGoldbergRaoAlgorithm(id: string | undefined): boolean {
	return id === GOLDBERG_RAO_ALGORITHM;
}
export function isBinaryBlockingAlgorithm(id: string | undefined): boolean {
	return id === BINARY_BLOCKING_ALGORITHM;
}
export function isDistanceDirectedAlgorithm(id: string | undefined): boolean {
	return id !== undefined && DISTANCE_DIRECTED_ALGORITHMS.has(id);
}
export function isRootwardForestAlgorithm(id: string | undefined): boolean {
	return (
		isDynamicTreeBlockingAlgorithm(id) ||
		isDynamicTreePushRelabelAlgorithm(id) ||
		isDistanceDirectedAlgorithm(id)
	);
}
export function isPseudoflowAlgorithm(id: string | undefined): boolean {
	return id !== undefined && PSEUDOFLOW_ALGORITHMS.has(id);
}
export function isPotentialDijkstraSspAlgorithm(
	id: string | undefined,
): boolean {
	return (
		id === POTENTIAL_DIJKSTRA_SSP_ALGORITHM ||
		id === SUCCESSIVE_SHORTEST_AUGMENTING_PATH_ALGORITHM ||
		id === PRIMAL_DUAL_MCF_ALGORITHM ||
		id === BLOCKING_PRIMAL_DUAL_ALGORITHM ||
		isCapacityScalingMcfAlgorithm(id)
	);
}
export function isBlockingPrimalDualAlgorithm(id: string | undefined): boolean {
	return id === BLOCKING_PRIMAL_DUAL_ALGORITHM;
}
export function isCapacityScalingMcfAlgorithm(id: string | undefined): boolean {
	return (
		id === CAPACITY_SCALING_MCF_ALGORITHM || id === EXCESS_SCALING_MCF_ALGORITHM
	);
}
export function isExcessScalingMcfAlgorithm(id: string | undefined): boolean {
	return id === EXCESS_SCALING_MCF_ALGORITHM;
}
export function isCostScalingAlgorithm(id: string | undefined): boolean {
	return id !== undefined && COST_SCALING_ALGORITHMS.has(id);
}
export function isAugmentRelabelMcfAlgorithm(id: string | undefined): boolean {
	return id !== undefined && AUGMENT_RELABEL_MCF_ALGORITHMS.has(id);
}
export function isPriceRefinementAlgorithm(id: string | undefined): boolean {
	return id === PRICE_REFINEMENT_ALGORITHM;
}
export function isArcFixingAlgorithm(id: string | undefined): boolean {
	return id === ARC_FIXING_ALGORITHM;
}
export function isOutOfKilterAlgorithm(id: string | undefined): boolean {
	return id === OUT_OF_KILTER_ALGORITHM;
}
export function isRelaxationAlgorithm(id: string | undefined): boolean {
	return id === RELAXATION_ALGORITHM;
}
export function isEpsilonRelaxationAlgorithm(id: string | undefined): boolean {
	return id === EPSILON_RELAXATION_ALGORITHM;
}
export function isPriceCoordinateRelaxationAlgorithm(
	id: string | undefined,
): boolean {
	return isRelaxationAlgorithm(id) || isEpsilonRelaxationAlgorithm(id);
}
export function isSimpleCycleCancelingAlgorithm(
	id: string | undefined,
): boolean {
	return id === SIMPLE_CYCLE_CANCELING_ALGORITHM;
}
export function isMinimumMeanCycleCancelingAlgorithm(
	id: string | undefined,
): boolean {
	return id === MINIMUM_MEAN_CYCLE_CANCELING_ALGORITHM;
}
export function isCancelAndTightenAlgorithm(id: string | undefined): boolean {
	return id === CANCEL_AND_TIGHTEN_ALGORITHM;
}
export function isRelaxedMndcAlgorithm(id: string | undefined): boolean {
	return id === RELAXED_MNDC_ALGORITHM;
}
export function isEnhancedCapacityScalingAlgorithm(
	id: string | undefined,
): boolean {
	return id === ENHANCED_CAPACITY_SCALING_ALGORITHM;
}
export function isOrlinMcfAlgorithm(id: string | undefined): boolean {
	return id === ORLIN_MCF_ALGORITHM;
}
export function isOrlinMaxFlowAlgorithm(id: string | undefined): boolean {
	return id === ORLIN_MAX_FLOW_ALGORITHM;
}
export function isDualNetworkSimplexAlgorithm(id: string | undefined): boolean {
	return id === DUAL_NETWORK_SIMPLEX_ALGORITHM;
}
export function isPolynomialDualSimplexAlgorithm(
	id: string | undefined,
): boolean {
	return id === POLYNOMIAL_DUAL_SIMPLEX_ALGORITHM;
}
export function isPolynomialPrimalSimplexAlgorithm(
	id: string | undefined,
): boolean {
	return id === POLYNOMIAL_PRIMAL_SIMPLEX_ALGORITHM;
}
export function isDoubleScalingAlgorithm(id: string | undefined): boolean {
	return id === DOUBLE_SCALING_ALGORITHM;
}
export function isConvexCostAlgorithm(id: string | undefined): boolean {
	return id !== undefined && CONVEX_COST_ALGORITHMS.has(id);
}
export function isCycleCancelingAlgorithm(id: string | undefined): boolean {
	return (
		isSimpleCycleCancelingAlgorithm(id) ||
		isMinimumMeanCycleCancelingAlgorithm(id)
	);
}
export function isNetworkSimplexAlgorithm(id: string | undefined): boolean {
	return id !== undefined && NETWORK_SIMPLEX_ALGORITHMS.has(id);
}
export function isDynamicTreeNetworkSimplexAlgorithm(
	id: string | undefined,
): boolean {
	return id === DYNAMIC_TREE_NETWORK_SIMPLEX_ALGORITHM;
}
export function isNetworkSimplexOptimalEvent(id: string | undefined): boolean {
	return (
		id === `${PRIMAL_NETWORK_SIMPLEX_ALGORITHM}.optimal` ||
		id === `${DYNAMIC_TREE_NETWORK_SIMPLEX_ALGORITHM}.optimal`
	);
}
