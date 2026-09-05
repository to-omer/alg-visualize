import { ordinaryFlowEventEntityRefs } from "./flow-event-highlight";
import {
	buildFlowLayout,
	FLOW_NODE_RADIUS,
	FLOW_VIEWBOX_HEIGHT,
	FLOW_VIEWBOX_WIDTH,
} from "./flow-layout";
import { projectFlowOriginalEdgeFeatures } from "./flow-original-edge-feature-projection";
import { buildPlanarDualOverlay } from "./flow-planar-dual";
import type { FlowEntityRenderPlan } from "./flow-render-plan";
import { absoluteBigInt } from "./flow-visual-scales";

export {
	exactRational,
	rationalCapacityBand,
	rationalMagnitudeStrokeWidth,
} from "./flow-graph-rational-scales";

export type FlowViewMode = "original" | "residual" | "both";

export type RmfgenFrameGroup = Readonly<{
	frame: number;
	x: number;
	y: number;
	width: number;
	height: number;
}>;

export type FlowEntityGraphPlan = Readonly<{
	render: FlowEntityRenderPlan;
	viewMode: FlowViewMode;
	frameGroups: readonly RmfgenFrameGroup[];
}>;

function edgeLabelValueBudget(value: unknown): number {
	if (value === undefined || value === null || Array.isArray(value)) return 0;
	if (typeof value === "object") {
		return Object.values(value).reduce(
			(total, field) => total + edgeLabelValueBudget(field),
			0,
		);
	}
	const rendered =
		typeof value === "boolean" ? (value ? "true" : "") : `${value}`;
	return rendered.length === 0 ? 0 : rendered.length + 6;
}

export function projectFlowEntityGraphState(graphPlan: FlowEntityGraphPlan) {
	const { render: plan, viewMode, frameGroups } = graphPlan;
	const { context } = plan;
	const visualization = plan.visualization;
	const ibfsView = visualization.ibfsView;
	const fixedOriginalEdges = visualization.fixedOriginalEdges;
	const matchedOriginalEdges = visualization.matchedOriginalEdges;
	const forestArcKeys = visualization.forestArcKeys;
	const rootwardForest = visualization.features.rootwardForest;
	const networkSimplex = visualization.features.networkSimplex;
	const transportation = visualization.features.transportation;
	const warmStartPushRelabel = visualization.features.warmStartPushRelabel;
	const renderData = plan.overlayPresentation.renderData;
	const overlayViews = renderData.overlayViews;
	const convexCostEdgeById = renderData.convexCostEdgeById;
	const dualSimplexEdgeById = renderData.dualSimplexEdgeById;
	const enhancedScalingEdgeById = renderData.enhancedScalingEdgeById;
	const orlinMaxActiveCompactByOrdinal =
		renderData.orlinMaxActiveCompactByOrdinal;
	const orlinMaxNodeById = renderData.orlinMaxNodeById;
	const predictionEdgeById = renderData.predictionEdgeById;
	const randomizedAlmostLinearEdgeById =
		renderData.randomizedAlmostLinearEdgeById;

	const forestChildIds = new Set(
		context.residualArcs
			.filter((arc) => forestArcKeys.has(`${arc.edge_id}:${arc.direction}`))
			.map((arc) => (rootwardForest ? arc.from : arc.to)),
	);
	const hasForestOverlay =
		(ibfsView === undefined &&
			context.pseudoflowForest !== undefined &&
			!warmStartPushRelabel) ||
		networkSimplex ||
		transportation;
	const showsCost = !new Set([
		"max-flow",
		"parametric-max-flow",
		"planar-max-flow",
		"bipartite-matching",
	]).has(context.model.kind);
	const labelMetrics = new Map(
		plan.edges.map((edge) => {
			const advancedStates = [
				convexCostEdgeById.get(edge.id),
				predictionEdgeById.get(edge.id),
				enhancedScalingEdgeById.get(edge.id),
				renderData.orlinMcfBranchesByEdge.get(edge.id),
				dualSimplexEdgeById.get(edge.id),
				renderData.polynomialDualEdgeById.get(edge.id),
				renderData.polynomialPrimalEdgeById.get(edge.id),
				renderData.doubleScalingEdgeById.get(edge.id),
				renderData.electricalEdgeById.get(edge.id),
				renderData.augmentingElectricalEdgeById.get(edge.id),
				renderData.interiorPointEdgeById.get(edge.id),
				renderData.electricalIpmMcfEdgeById.get(edge.id),
				renderData.minimumRatioEdgeById.get(edge.id),
				randomizedAlmostLinearEdgeById.get(edge.id),
				renderData.deterministicAlmostLinearEdgeById.get(edge.id),
			].filter((state) => state !== undefined);
			const advancedSingleLine = advancedStates.length > 0;
			const fixedWidth = fixedOriginalEdges.has(edge.id) ? 38 : 0;
			const matchedWidth = matchedOriginalEdges.has(edge.id) ? 50 : 0;
			const tardosWidth = renderData.tardosFixedByEdge.has(edge.id) ? 104 : 0;
			const measuredAdvancedWidth =
				advancedStates.length === 0
					? 0
					: 20 + Math.max(...advancedStates.map(edgeLabelValueBudget)) * 7;
			return [
				edge.id,
				{
					widthAddition:
						fixedWidth + matchedWidth + tardosWidth + measuredAdvancedWidth,
					height: advancedSingleLine ? 35 : showsCost ? 36 : 24,
					yOffset: renderData.orlinMcfBranchesByEdge.has(edge.id) ? 32 : 0,
				},
			] as const;
		}),
	);
	const labelPriorityEdgeIds = ordinaryFlowEventEntityRefs(context).flatMap(
		(entity) =>
			entity.kind === "edge" || entity.kind === "residual-arc"
				? [entity.edge_id]
				: [],
	);
	const layout = buildFlowLayout(plan.nodes, plan.edges, {
		labelEdgeIds: plan.edgeLabelIds,
		labelPriorityEdgeIds,
		labelMetrics,
		model: context.model,
	});
	const positions = layout.positions;
	const orlinMaxComponentMembers = new Map<string, string[]>();
	for (const node of overlayViews.orlinMaxFlow?.nodes ?? []) {
		const members = orlinMaxComponentMembers.get(node.component_id) ?? [];
		members.push(node.node_id);
		orlinMaxComponentMembers.set(node.component_id, members);
	}
	const orlinMaxComponentBoxes = [...orlinMaxComponentMembers].flatMap(
		([componentId, members]) => {
			const points = members.flatMap((member) => {
				const point = positions.get(member);
				return point === undefined ? [] : [point];
			});
			const state = orlinMaxNodeById.get(componentId);
			if (points.length === 0 || state === undefined) return [];
			const padding = members.length === 1 ? 34 : 44;
			const minX = Math.min(...points.map((point) => point.x)) - padding;
			const maxX = Math.max(...points.map((point) => point.x)) + padding;
			const minY = Math.min(...points.map((point) => point.y)) - padding;
			const maxY = Math.max(...points.map((point) => point.y)) + padding;
			return [
				{
					componentId,
					members,
					state,
					x: minX,
					y: minY,
					width: maxX - minX,
					height: maxY - minY,
				},
			];
		},
	);
	const maximumOrlinMaxCompactCapacity = (
		overlayViews.orlinMaxFlow?.compact_arcs ?? []
	).reduce((maximum, arc) => {
		const capacity = BigInt(arc.capacity);
		return capacity > maximum ? capacity : maximum;
	}, 1n);
	const orlinMaxCompactVisuals = (
		overlayViews.orlinMaxFlow?.compact_arcs ?? []
	).flatMap((arc) => {
		const from = positions.get(arc.from_component);
		const to = positions.get(arc.to_component);
		if (from === undefined || to === undefined) return [];
		const dx = to.x - from.x;
		const dy = to.y - from.y;
		const distance = Math.max(1, Math.hypot(dx, dy));
		const unitX = dx / distance;
		const unitY = dy / distance;
		const normalX = -unitY;
		const normalY = unitX;
		const bend = (Number(BigInt(arc.ordinal) % 5n) - 2) * 9;
		const start = { x: from.x + unitX * 28, y: from.y + unitY * 28 };
		const end = { x: to.x - unitX * 28, y: to.y - unitY * 28 };
		const control = {
			x: (start.x + end.x) / 2 + normalX * bend,
			y: (start.y + end.y) / 2 + normalY * bend,
		};
		const reversePath = `M ${end.x} ${end.y} Q ${control.x} ${control.y} ${start.x} ${start.y}`;
		const capacity = BigInt(arc.capacity);
		const flow = BigInt(arc.flow);
		return [
			{
				arc,
				path: `M ${start.x} ${start.y} Q ${control.x} ${control.y} ${end.x} ${end.y}`,
				reversePath,
				label: control,
				capacityWidth:
					3 +
					Number((capacity * 5_000n) / maximumOrlinMaxCompactCapacity) / 1_000,
				flowWidth:
					flow === 0n || capacity === 0n
						? 1.5
						: 1.5 + Number((flow * 4_000n) / capacity) / 1_000,
				activeReverse: orlinMaxActiveCompactByOrdinal.get(arc.ordinal),
			},
		];
	});
	const convexSimplexRootPosition = (() => {
		if (overlayViews.convexNetworkSimplex === undefined) return undefined;
		const candidates = [
			{ x: 54, y: 50 },
			{ x: FLOW_VIEWBOX_WIDTH - 54, y: 50 },
			{ x: 54, y: FLOW_VIEWBOX_HEIGHT - 50 },
			{ x: FLOW_VIEWBOX_WIDTH - 54, y: FLOW_VIEWBOX_HEIGHT - 50 },
		];
		return candidates.reduce(
			(best, candidate) => {
				const clearance = [...positions.values()].reduce(
					(minimum, position) =>
						Math.min(
							minimum,
							(candidate.x - position.x) ** 2 + (candidate.y - position.y) ** 2,
						),
					Number.POSITIVE_INFINITY,
				);
				return clearance > best.clearance ? { candidate, clearance } : best;
			},
			{ candidate: candidates[0] as { x: number; y: number }, clearance: -1 },
		).candidate;
	})();
	const enhancedScalingComponentBoxes = (
		overlayViews.enhancedCapacityScaling?.components ?? []
	).flatMap((component) => {
		const points = component.members.flatMap((member) => {
			const point = positions.get(member);
			return point === undefined ? [] : [point];
		});
		if (points.length === 0) return [];
		const padding = component.members.length === 1 ? 38 : 46;
		const minX = Math.min(...points.map((point) => point.x)) - padding;
		const maxX = Math.max(...points.map((point) => point.x)) + padding;
		const minY = Math.min(...points.map((point) => point.y)) - padding;
		const maxY = Math.max(...points.map((point) => point.y)) + padding;
		return [
			{
				...component,
				x: minX,
				y: minY,
				width: maxX - minX,
				height: maxY - minY,
				activeRole:
					overlayViews.enhancedCapacityScaling?.source_component ===
					component.component_id
						? ("source" as const)
						: overlayViews.enhancedCapacityScaling?.sink_component ===
								component.component_id
							? ("sink" as const)
							: undefined,
			},
		];
	});
	// The interaction/focus rings extend 13 px beyond the structural node disc.
	const nodeObstacleRadius = FLOW_NODE_RADIUS + 15;
	const planarLabelObstacles = [
		...[...positions.values()].map((position) => ({
			left: position.x - nodeObstacleRadius,
			right: position.x + nodeObstacleRadius,
			top: position.y - nodeObstacleRadius,
			bottom: position.y + nodeObstacleRadius,
			weight: 1_000_000,
		})),
		...[...layout.routes.values()]
			.filter((route) => route.labelCollisionFree)
			.map((route) => {
				const centerY = route.label.y + route.labelYOffset;
				return {
					left: route.label.x - route.labelBoxWidth / 2,
					right: route.label.x + route.labelBoxWidth / 2,
					top: centerY - route.labelHeight / 2,
					bottom: centerY + route.labelHeight / 2,
					weight: 10_000,
				};
			}),
	];
	const planarDual = buildPlanarDualOverlay(
		context.planarDualInput,
		positions,
		planarLabelObstacles,
	);
	const visibleEdgeIds = new Set(plan.edges.map((edge) => edge.id));
	const visibleResidualArcs = context.residualArcs
		.filter((arc) => visibleEdgeIds.has(arc.edge_id))
		.sort((left, right) => Number(left.active) - Number(right.active));
	const maxCapacity = plan.edges.reduce((maximum, edge) => {
		const capacity = BigInt(edge.capacity);
		return capacity > maximum ? capacity : maximum;
	}, 1n);
	const maxResidualCapacity = visibleResidualArcs.reduce((maximum, arc) => {
		const capacity = BigInt(arc.capacity);
		return capacity > maximum ? capacity : maximum;
	}, 1n);
	const maxAbsoluteCost = plan.edges.reduce((maximum, edge) => {
		const magnitude = absoluteBigInt(BigInt(edge.cost));
		return magnitude > maximum ? magnitude : maximum;
	}, 0n);
	const terminal = context.gridgraph
		? { source: "s", sink: "t" }
		: context.model.kind === "max-flow" ||
				context.model.kind === "parametric-max-flow" ||
				context.model.kind === "planar-max-flow" ||
				context.model.kind === "fixed-flow-min-cost" ||
				context.model.kind === "min-cost-max-flow"
			? { source: context.model.source, sink: context.model.sink }
			: context.model.kind === "bipartite-matching" &&
					context.model.flow_adapter !== undefined
				? context.model.flow_adapter
				: undefined;
	const randomizedReturnGeometry = (() => {
		if (
			overlayViews.randomizedAlmostLinear === undefined ||
			terminal === undefined
		)
			return undefined;
		const from = positions.get(terminal.sink);
		const to = positions.get(terminal.source);
		if (from === undefined || to === undefined) return undefined;
		const dx = to.x - from.x;
		const dy = to.y - from.y;
		const distance = Math.max(1, Math.hypot(dx, dy));
		const unitX = dx / distance;
		const unitY = dy / distance;
		const normalX = -unitY;
		const normalY = unitX;
		const bend = Math.min(118, Math.max(72, distance * 0.24));
		const start = { x: from.x + unitX * 32, y: from.y + unitY * 32 };
		const end = { x: to.x - unitX * 32, y: to.y - unitY * 32 };
		const control = {
			x: (start.x + end.x) / 2 + normalX * bend,
			y: (start.y + end.y) / 2 + normalY * bend,
		};
		return {
			path: `M ${start.x} ${start.y} Q ${control.x} ${control.y} ${end.x} ${end.y}`,
			reversePath: `M ${end.x} ${end.y} Q ${control.x} ${control.y} ${start.x} ${start.y}`,
			label: control,
			midpoint: {
				x: (start.x + 2 * control.x + end.x) / 4,
				y: (start.y + 2 * control.y + end.y) / 4,
			},
		};
	})();
	const randomizedArtificialStarPosition = (() => {
		if (
			overlayViews.randomizedAlmostLinear === undefined ||
			overlayViews.randomizedAlmostLinear.stage === "ready" ||
			overlayViews.randomizedAlmostLinear.stage ===
				"build-return-edge-reduction" ||
			BigInt(overlayViews.randomizedAlmostLinear.artificial_edges) === 0n
		)
			return undefined;
		const candidates = [
			{ x: 58, y: 54 },
			{ x: FLOW_VIEWBOX_WIDTH - 58, y: 54 },
			{ x: 58, y: FLOW_VIEWBOX_HEIGHT - 54 },
			{ x: FLOW_VIEWBOX_WIDTH - 58, y: FLOW_VIEWBOX_HEIGHT - 54 },
		];
		return candidates.reduce(
			(best, candidate) => {
				const clearance = [...positions.values()].reduce(
					(minimum, position) =>
						Math.min(
							minimum,
							(candidate.x - position.x) ** 2 + (candidate.y - position.y) ** 2,
						),
					Number.POSITIVE_INFINITY,
				);
				return clearance > best.clearance ? { candidate, clearance } : best;
			},
			{
				candidate: candidates[0] as { x: number; y: number },
				clearance: -1,
			},
		).candidate;
	})();
	const deterministicReturnGeometry = (() => {
		if (
			overlayViews.deterministicAlmostLinear === undefined ||
			terminal === undefined
		)
			return undefined;
		const from = positions.get(terminal.sink);
		const to = positions.get(terminal.source);
		if (from === undefined || to === undefined) return undefined;
		const dx = to.x - from.x;
		const dy = to.y - from.y;
		const distance = Math.max(1, Math.hypot(dx, dy));
		const unitX = dx / distance;
		const unitY = dy / distance;
		const normalX = -unitY;
		const normalY = unitX;
		const bend = Math.min(132, Math.max(84, distance * 0.27));
		const start = { x: from.x + unitX * 32, y: from.y + unitY * 32 };
		const end = { x: to.x - unitX * 32, y: to.y - unitY * 32 };
		const control = {
			x: (start.x + end.x) / 2 - normalX * bend,
			y: (start.y + end.y) / 2 - normalY * bend,
		};
		return {
			path: `M ${start.x} ${start.y} Q ${control.x} ${control.y} ${end.x} ${end.y}`,
			reversePath: `M ${end.x} ${end.y} Q ${control.x} ${control.y} ${start.x} ${start.y}`,
			label: control,
			midpoint: {
				x: (start.x + 2 * control.x + end.x) / 4,
				y: (start.y + 2 * control.y + end.y) / 4,
			},
		};
	})();
	const deterministicArtificialStarPosition = (() => {
		if (
			overlayViews.deterministicAlmostLinear === undefined ||
			overlayViews.deterministicAlmostLinear.stage === "ready" ||
			overlayViews.deterministicAlmostLinear.stage ===
				"build-return-edge-reduction" ||
			BigInt(overlayViews.deterministicAlmostLinear.artificial_edges) === 0n
		)
			return undefined;
		const candidates = [
			{ x: FLOW_VIEWBOX_WIDTH / 2, y: 44 },
			{ x: FLOW_VIEWBOX_WIDTH / 2, y: FLOW_VIEWBOX_HEIGHT - 44 },
			{ x: 54, y: FLOW_VIEWBOX_HEIGHT / 2 },
			{ x: FLOW_VIEWBOX_WIDTH - 54, y: FLOW_VIEWBOX_HEIGHT / 2 },
		];
		return candidates.reduce(
			(best, candidate) => {
				const clearance = [...positions.values()].reduce(
					(minimum, position) =>
						Math.min(
							minimum,
							(candidate.x - position.x) ** 2 + (candidate.y - position.y) ** 2,
						),
					Number.POSITIVE_INFINITY,
				);
				return clearance > best.clearance ? { candidate, clearance } : best;
			},
			{ candidate: candidates[0] as { x: number; y: number }, clearance: -1 },
		).candidate;
	})();
	const gridgen = context.gridgen;
	const hasBalances = context.hasBalances;
	const originalVisuals = projectFlowOriginalEdgeFeatures(
		plan,
		layout,
		maxCapacity,
	);
	return {
		plan,
		viewMode,
		frameGroups,
		context,
		visualization,
		renderData: plan.overlayPresentation.renderData,
		forestChildIds,
		hasForestOverlay,
		layout,
		positions,
		orlinMaxComponentBoxes,
		maximumOrlinMaxCompactCapacity,
		orlinMaxCompactVisuals,
		convexSimplexRootPosition,
		enhancedScalingComponentBoxes,
		planarDual,
		visibleResidualArcs,
		maxCapacity,
		maxResidualCapacity,
		maxAbsoluteCost,
		terminal,
		randomizedReturnGeometry,
		randomizedArtificialStarPosition,
		deterministicReturnGeometry,
		deterministicArtificialStarPosition,
		gridgen,
		hasBalances,
		originalVisuals,
	};
}
