import { eibfsLifecycleStage, projectEibfsView } from "./flow-eibfs-view";
import { projectIbfsView } from "./flow-ibfs-view";
import {
	buildFlowOverlayPresentation,
	type FlowOverlayPresentation,
} from "./flow-overlay-presentation";
import { projectFlowParametricCut } from "./flow-parametric-view";
import type { FlowCurrentSceneV9 } from "./flow-scene";

const WARM_START_PUSH_RELABEL = "warm-start-push-relabel";
const PUSH_RELABEL = new Set([
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
const BLOCKING_PREFLOW = new Set(["karzanov-preflow", "mpm"]);
const PSEUDOFLOW = new Set(["hochbaum-pseudoflow", "pseudoflow-simplex"]);
const COST_SCALING = new Set([
	"cost-scaling",
	"cost-scaling-push-relabel",
	"augment-relabel",
	"partial-augment-relabel-mcf",
	"price-refinement",
	"arc-fixing",
	"generalized-cost-scaling",
]);
const DISTANCE_DIRECTED = new Set([
	"distance-directed-augmenting-path",
	"distance-directed-scaling-augmenting-path",
]);
const NETWORK_SIMPLEX = new Set([
	"primal-network-simplex",
	"dynamic-tree-network-simplex",
]);
const TRANSPORTATION = new Set(["transportation-simplex", "modi"]);
const CONVEX_COST = new Set([
	"segment-expanded-convex-mcf",
	"convex-cost-scaling",
	"convex-network-simplex",
]);

export type FlowEntityVisualization = ReturnType<
	typeof buildFlowEntityVisualization
>;

/**
 * Converts the wire scene into renderer-oriented indexes and semantic sets.
 *
 * This is the algorithm-aware boundary: the SVG renderer consumes this plan
 * and does not need to rediscover flow/cut/matching/forest semantics itself.
 */
export function buildFlowEntityVisualization(
	scene: FlowCurrentSceneV9,
	presentation: FlowOverlayPresentation = buildFlowOverlayPresentation(scene),
) {
	const algorithmId = scene.algorithm.id;
	const overlayViews = presentation.renderData.overlayViews;
	const edgeStates = new Map(
		scene.edge_states.map((state) => [state.edge_id, state]),
	);
	const parametricProjection = projectFlowParametricCut(scene);
	const parametricCut = {
		minimal: new Set(parametricProjection?.minimalSourceSide ?? []),
		tie: new Set(parametricProjection?.tiedNodes ?? []),
	};
	const sourceSide = new Set(
		scene.outcome?.kind === "max-flow" ||
			scene.outcome?.kind === "min-cost-max-flow"
			? scene.outcome.source_side
			: scene.outcome?.kind === "infeasible"
				? scene.outcome.reachable_original_nodes
				: scene.algorithm.id === WARM_START_PUSH_RELABEL &&
						scene.pseudoflow_forest !== undefined
					? scene.pseudoflow_forest.strong_nodes
					: [...parametricCut.minimal],
	);
	const infeasibleReachable = new Set(
		scene.outcome?.kind === "infeasible"
			? scene.outcome.reachable_original_nodes
			: [],
	);
	const potentials = new Map<string, string>(
		scene.outcome?.kind === "min-cost-flow" ||
			scene.outcome?.kind === "min-cost-max-flow"
			? scene.outcome.potentials.map((item) => [item.node_id, item.potential])
			: scene.outcome?.kind === "assignment"
				? [...scene.outcome.agent_labels, ...scene.outcome.task_labels].map(
						(item) => [item.node_id, item.label],
					)
				: [],
	);
	const nodeTraceStates = new Map(
		scene.node_trace_states.map((state) => [state.node_id, state]),
	);
	const ibfsView = projectIbfsView(scene);
	const eibfsView = projectEibfsView(scene);
	const activeOriginalEdges = new Set(
		scene.residual_arcs.filter((arc) => arc.active).map((arc) => arc.edge_id),
	);
	if (overlayViews.dynamicEibfs?.changed_edge !== undefined) {
		activeOriginalEdges.add(overlayViews.dynamicEibfs.changed_edge);
	}
	const activeForwardOriginalEdges = new Set(
		scene.residual_arcs
			.filter((arc) => arc.active && arc.direction === "forward")
			.map((arc) => arc.edge_id),
	);
	const activeReverseOriginalEdges = new Set(
		scene.residual_arcs
			.filter((arc) => arc.active && arc.direction === "reverse")
			.map((arc) => arc.edge_id),
	);
	const fixedOriginalEdges = new Set(
		scene.residual_arcs.filter((arc) => arc.fixed).map((arc) => arc.edge_id),
	);

	const matchedOriginalEdges = matchingEdges(scene);
	const matchingCoverNodes = new Set(
		scene.outcome?.kind === "bipartite-matching"
			? [...scene.outcome.cover_left, ...scene.outcome.cover_right]
			: [],
	);
	const assignmentHallNodes = new Set(
		scene.outcome?.kind === "assignment-infeasible"
			? [...scene.outcome.hall_agents, ...scene.outcome.neighbor_tasks]
			: [],
	);
	const forestArcKeys = new Set(
		(scene.pseudoflow_forest?.arcs ?? []).map(
			(arc) => `${arc.edge_id}:${arc.direction}`,
		),
	);
	const basisOriginalEdges = new Set(
		ibfsView === undefined
			? (scene.pseudoflow_forest?.arcs ?? []).map((arc) => arc.edge_id)
			: [],
	);
	const ibfsForestByEdge = new Map(
		(scene.pseudoflow_forest?.arcs ?? []).flatMap((arc) => {
			const key = `${arc.edge_id}:${arc.direction}`;
			const side = ibfsView?.sourceForestArcKeys.has(key)
				? ("source" as const)
				: ibfsView?.sinkForestArcKeys.has(key)
					? ("sink" as const)
					: undefined;
			return side === undefined
				? []
				: [[arc.edge_id, { ...arc, side }] as const];
		}),
	);
	const eibfsForestByEdge = new Map(
		(eibfsView?.forestArcs ?? []).map(
			(relation) =>
				[
					relation.admissible_residual.edge_id,
					{
						side: relation.side,
						direction: relation.displayDirection,
						parent: relation.parent,
						child: relation.child,
					},
				] as const,
		),
	);
	const strongNodeIds = new Set(scene.pseudoflow_forest?.strong_nodes ?? []);
	const predictedOriginalEdges = new Set(
		scene.algorithm.id === WARM_START_PUSH_RELABEL
			? scene.graph.edges
					.filter(
						(edge) =>
							edge.initial_flow !== undefined && edge.initial_flow !== "0",
					)
					.map((edge) => edge.id)
			: [],
	);

	return {
		randomizedSampleCount:
			overlayViews.randomizedAlmostLinear?.sample_count ?? "0",
		edgeStates,
		parametricCut,
		sourceSide,
		infeasibleReachable,
		potentials,
		nodeTraceStates,
		ibfsView,
		eibfsView,
		eibfsStage: eibfsLifecycleStage(scene),
		activeOriginalEdges,
		activeForwardOriginalEdges,
		activeReverseOriginalEdges,
		fixedOriginalEdges,
		matchedOriginalEdges,
		matchingCoverNodes,
		assignmentHallNodes,
		forestArcKeys,
		basisOriginalEdges,
		ibfsForestByEdge,
		eibfsForestByEdge,
		strongNodeIds,
		predictedOriginalEdges,
		features: {
			pushRelabel: PUSH_RELABEL.has(algorithmId),
			blockingPreflow: BLOCKING_PREFLOW.has(algorithmId),
			pseudoflow: PSEUDOFLOW.has(algorithmId),
			costScaling: COST_SCALING.has(algorithmId),
			distanceDirected: DISTANCE_DIRECTED.has(algorithmId),
			rootwardForest:
				DISTANCE_DIRECTED.has(algorithmId) ||
				algorithmId === "dynamic-tree-blocking-flow" ||
				algorithmId === "dynamic-tree-push-relabel",
			networkSimplex: NETWORK_SIMPLEX.has(algorithmId),
			dynamicTreeNetworkSimplex: algorithmId === "dynamic-tree-network-simplex",
			transportation: TRANSPORTATION.has(algorithmId),
			convexCost: CONVEX_COST.has(algorithmId),
			dynamicTreeBlocking: algorithmId === "dynamic-tree-blocking-flow",
			dynamicTreePushRelabel: algorithmId === "dynamic-tree-push-relabel",
			goldbergRao: algorithmId === "goldberg-rao",
			binaryBlocking: algorithmId === "binary-blocking-flow",
			warmStartPushRelabel: algorithmId === WARM_START_PUSH_RELABEL,
			blockingPrimalDual: algorithmId === "blocking-flow-primal-dual",
			capacityScaling:
				algorithmId === "capacity-scaling-mcf" ||
				algorithmId === "excess-scaling-mcf",
			arcFixing: algorithmId === "arc-fixing",
			outOfKilter: algorithmId === "out-of-kilter",
			relaxation: algorithmId === "relaxation",
			epsilonRelaxation: algorithmId === "epsilon-relaxation",
			predictionAssisted:
				algorithmId === "prediction-assisted-epsilon-relaxation",
			doubleScaling: algorithmId === "double-scaling",
			relaxedMndc: algorithmId === "relaxed-most-negative-cycle",
			enhancedCapacityScaling: algorithmId === "enhanced-capacity-scaling",
			orlinMcf: algorithmId === "orlin-mcf",
			orlinMaxFlow: algorithmId === "orlin-max-flow",
			dualNetworkSimplex: algorithmId === "dual-network-simplex",
			polynomialDualSimplex: algorithmId === "polynomial-dual-network-simplex",
			polynomialPrimalSimplex:
				algorithmId === "polynomial-primal-network-simplex",
			tardosFramework: algorithmId === "tardos-framework",
			hungarian: algorithmId === "hungarian",
			auction: algorithmId === "auction",
		},
	} as const;
}

function matchingEdges(scene: FlowCurrentSceneV9): Set<string> {
	if (
		scene.model.kind !== "bipartite-matching" &&
		scene.model.kind !== "assignment"
	) {
		return new Set();
	}
	const left = new Set(
		scene.model.kind === "bipartite-matching"
			? scene.model.left
			: scene.model.agents,
	);
	const right = new Set(
		scene.model.kind === "bipartite-matching"
			? scene.model.right
			: scene.model.tasks,
	);
	const flowByEdge = new Map(
		scene.edge_states.map((state) => [state.edge_id, state.flow]),
	);
	return new Set(
		scene.graph.edges
			.filter(
				(edge) =>
					left.has(edge.from) &&
					right.has(edge.to) &&
					flowByEdge.get(edge.id) === "1",
			)
			.map((edge) => edge.id),
	);
}
