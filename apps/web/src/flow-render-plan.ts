import {
	buildFlowLayout,
	buildFlowNodePositions,
	FLOW_DETAIL_PARALLEL_LANE_LIMIT,
	FLOW_NODE_MIN_CENTER_SPACING,
	FLOW_VIEWBOX_HEIGHT,
	FLOW_VIEWBOX_WIDTH,
	type FlowEdgeRoute,
	type FlowPoint,
} from "./flow-layout";
import { constrainFlowCanvasLodToRenderLimits } from "./flow-lod-policy";
import {
	buildFlowOverlayPresentation,
	type FlowOverlayPresentation,
} from "./flow-overlay-presentation";
import { projectFlowParametricCut } from "./flow-parametric-view";
import type { FlowPlanarDualInput } from "./flow-planar-dual";
import type { FlowCurrentSceneV9, FlowEdgeV1, FlowNodeV1 } from "./flow-scene";
import {
	buildFlowEntityVisualization,
	type FlowEntityVisualization,
} from "./flow-visualization-adapter";

export type FlowLod = "detail" | "structure" | "overview";

export const FLOW_LOD_LIMITS = Object.freeze({
	detail: Object.freeze({ nodes: 50, edges: 64 }),
	structure: Object.freeze({ nodes: 600, edges: 1_200 }),
	structureNodeLabels: 160,
	structureEdgeLabels: 12,
	structureNodeEventLabels: 16,
	structureEdgeEventLabels: 6,
	structureNodeTraceCallouts: 6,
	overviewAggregateMarks: 900,
});

// Entity nodes can carry a focus/trace ring outside their 29px body. Keep the
// same 8px semantic clearance used by the layout packer before retaining
// individually rendered NETGEN/grid lanes.
const FLOW_ANNOTATED_NODE_MIN_CENTER_SPACING = FLOW_NODE_MIN_CENTER_SPACING + 8;

export type FlowEntityRenderPlan = Readonly<{
	kind: "entities";
	level: "detail" | "structure";
	context: FlowEntityRenderContext;
	nodes: readonly FlowNodeV1[];
	edges: readonly FlowEdgeV1[];
	nodeLabelIds: ReadonlySet<string>;
	edgeLabelIds: ReadonlySet<string>;
	visualization: FlowEntityVisualization;
	overlayPresentation: FlowOverlayPresentation;
}>;

export type FlowEntityRenderContext = Readonly<{
	model: FlowCurrentSceneV9["model"];
	residualArcs: FlowCurrentSceneV9["residual_arcs"];
	pseudoflowForest: FlowCurrentSceneV9["pseudoflow_forest"];
	outcome: FlowCurrentSceneV9["outcome"];
	algorithmId: FlowCurrentSceneV9["algorithm"]["id"];
	metrics: FlowCurrentSceneV9["metrics"];
	traceEvent: FlowCurrentSceneV9["trace_event"];
	traceEventSemantics: FlowCurrentSceneV9["trace_event_semantics"];
	hasBalances: boolean;
	gridgen: boolean;
	gridgraph: boolean;
	planarDualInput: FlowPlanarDualInput;
}>;

function buildFlowEntityRenderContext(
	scene: FlowCurrentSceneV9,
): FlowEntityRenderContext {
	return {
		model: scene.model,
		residualArcs: scene.residual_arcs,
		pseudoflowForest: scene.pseudoflow_forest,
		outcome: scene.outcome,
		algorithmId: scene.algorithm.id,
		metrics: scene.metrics,
		traceEvent: scene.trace_event,
		traceEventSemantics: scene.trace_event_semantics,
		hasBalances: scene.graph.nodes.some((node) => node.supply !== "0"),
		gridgen: isGridgenScene(scene),
		gridgraph: isGridgraphScene(scene),
		planarDualInput: {
			algorithm: scene.algorithm,
			graph: scene.graph,
			model: scene.model,
			residual_arcs: scene.residual_arcs,
			solve_status: scene.solve_status,
			...(scene.trace_event === undefined
				? {}
				: { trace_event: scene.trace_event }),
		},
	};
}

export type FlowOverviewCluster = Readonly<{
	id: string;
	x: number;
	y: number;
	memberCount: number;
	sourceSide: "none" | "mixed" | "all";
	terminal: "none" | "source" | "sink" | "both";
	terminalLabel?: string;
	balance: "none" | "supply" | "demand" | "mixed";
	supplyCount: number;
	demandCount: number;
	netBalance: bigint;
	containsSupernode: boolean;
	traceCount: number;
	traceIdentities: readonly string[];
	changeCount: number;
	changedIdentities: readonly string[];
}>;

export type FlowOverviewOriginalEdge = Readonly<{
	id: string;
	from: string;
	to: string;
	route: FlowEdgeRoute;
	edgeCount: number;
	capacity: bigint;
	flow: bigint;
	costKind: "negative" | "zero" | "positive" | "mixed";
	minimumCost: bigint;
	maximumCost: bigint;
	activeCount: number;
	fixedCount: number;
	cutCount: number;
	traceCount: number;
	traceIdentities: readonly string[];
	changeCount: number;
	changedIdentities: readonly string[];
}>;

export type FlowOverviewResidualArc = Readonly<{
	id: string;
	from: string;
	to: string;
	route: FlowEdgeRoute;
	arcCount: number;
	capacity: bigint;
	direction: "forward" | "reverse" | "mixed";
	activeCount: number;
	fixedCount: number;
	traceCount: number;
	traceIdentities: readonly string[];
	changeCount: number;
	changedIdentities: readonly string[];
}>;

export type FlowOverviewRenderPlan = Readonly<{
	kind: "overview";
	level: "overview";
	grid: Readonly<{ columns: number; rows: number }>;
	clusters: readonly FlowOverviewCluster[];
	originalEdges: readonly FlowOverviewOriginalEdge[];
	residualArcs: readonly FlowOverviewResidualArc[];
}>;

export type FlowRenderPlan = FlowEntityRenderPlan | FlowOverviewRenderPlan;

type MutableCluster = {
	id: string;
	xTotal: number;
	yTotal: number;
	memberCount: number;
	sourceSideCount: number;
	containsSource: boolean;
	containsSink: boolean;
	supplyCount: number;
	demandCount: number;
	netBalance: bigint;
	containsSupernode: boolean;
	traceCount: number;
	traceIdentities: string[];
	changeCount: number;
	changedIdentities: string[];
};

type Mutable<T> = { -readonly [Property in keyof T]: T[Property] };
type MutableOriginalEdge = Omit<Mutable<FlowOverviewOriginalEdge>, "route">;
type MutableResidualArc = Omit<Mutable<FlowOverviewResidualArc>, "route">;

function compareText(left: string, right: string): number {
	return left < right ? -1 : left > right ? 1 : 0;
}

function terminalIds(
	scene: FlowCurrentSceneV9,
): Readonly<{ source: string; sink: string }> | undefined {
	if (isGridgraphScene(scene)) return { source: "s", sink: "t" };
	return scene.model.kind === "max-flow" ||
		scene.model.kind === "parametric-max-flow" ||
		scene.model.kind === "planar-max-flow" ||
		scene.model.kind === "fixed-flow-min-cost" ||
		scene.model.kind === "min-cost-max-flow"
		? { source: scene.model.source, sink: scene.model.sink }
		: scene.model.kind === "bipartite-matching" &&
				scene.model.flow_adapter !== undefined
			? scene.model.flow_adapter
			: undefined;
}

function cutReachableIds(scene: FlowCurrentSceneV9): readonly string[] {
	if (scene.model.kind === "parametric-max-flow") {
		return projectFlowParametricCut(scene)?.minimalSourceSide ?? [];
	}
	if (
		scene.outcome?.kind === "max-flow" ||
		scene.outcome?.kind === "min-cost-max-flow"
	) {
		return scene.outcome.source_side;
	}
	return scene.outcome?.kind === "infeasible"
		? scene.outcome.reachable_original_nodes
		: [];
}

function crossesCertificateCut(
	scene: FlowCurrentSceneV9,
	reachable: ReadonlySet<string>,
	from: string,
	to: string,
): boolean {
	return scene.outcome?.kind === "infeasible"
		? reachable.has(from) !== reachable.has(to)
		: reachable.has(from) && !reachable.has(to);
}

export function isGridgenScene(scene: FlowCurrentSceneV9): boolean {
	if (scene.model.kind !== "transshipment") return false;
	let hasSupernode = false;
	for (const node of scene.graph.nodes) {
		if (node.id === "super") {
			hasSupernode = true;
			continue;
		}
		if (!/^g[0-9]{4}c[0-9]{4}$/.test(node.id)) return false;
	}
	return hasSupernode;
}

export function isGridgraphScene(scene: FlowCurrentSceneV9): boolean {
	if (scene.model.kind !== "transshipment") return false;
	let hasSource = false;
	let hasSink = false;
	let hasGridNode = false;
	for (const node of scene.graph.nodes) {
		if (node.id === "s") {
			hasSource = true;
			continue;
		}
		if (node.id === "t") {
			hasSink = true;
			continue;
		}
		if (!/^q[0-9]{4}c[0-9]{4}$/.test(node.id)) return false;
		hasGridNode = true;
	}
	return hasSource && hasSink && hasGridNode;
}

export function isWashingtonRandomLevelScene(
	scene: FlowCurrentSceneV9,
): boolean {
	if (scene.model.kind !== "max-flow") return false;
	if (scene.model.source !== "s" || scene.model.sink !== "t") return false;
	let hasSource = false;
	let hasSink = false;
	let hasLevelNode = false;
	for (const node of scene.graph.nodes) {
		if (node.id === "s") {
			hasSource = true;
			continue;
		}
		if (node.id === "t") {
			hasSink = true;
			continue;
		}
		if (!/^w[0-9]{4}r[0-9]{4}$/.test(node.id)) return false;
		hasLevelNode = true;
	}
	return hasSource && hasSink && hasLevelNode;
}

export function isGotoTorusScene(scene: FlowCurrentSceneV9): boolean {
	if (scene.model.kind !== "transshipment") return false;
	let hasGridNode = false;
	for (const node of scene.graph.nodes) {
		if (/^x[0-9]{4}$/.test(node.id)) continue;
		if (!/^t[0-9]{4}c[0-9]{4}$/.test(node.id)) return false;
		hasGridNode = true;
	}
	return hasGridNode;
}

export function isNetgenScene(scene: FlowCurrentSceneV9): boolean {
	let hasSource = false;
	let hasSink = false;
	for (const node of scene.graph.nodes) {
		if (/^s(?:x)?[0-9]{4}$/.test(node.id)) {
			hasSource = true;
			continue;
		}
		if (/^t(?:x)?[0-9]{4}$/.test(node.id)) {
			hasSink = true;
			continue;
		}
		if (!/^x[0-9]{4}$/.test(node.id)) return false;
	}
	return hasSource && hasSink;
}

function isGoldbergMeshScene(scene: FlowCurrentSceneV9): boolean {
	if (scene.model.kind !== "circulation" || scene.graph.edges.length === 0) {
		return false;
	}
	const coordinates = scene.graph.nodes.map((node) => {
		const match = /^m([0-9]{4})c([0-9]{4})$/.exec(node.id);
		return match === null
			? undefined
			: { row: Number(match[1]), column: Number(match[2]) };
	});
	if (coordinates.some((coordinate) => coordinate === undefined)) return false;
	const complete = coordinates as { row: number; column: number }[];
	const rows = Math.max(...complete.map(({ row }) => row)) + 1;
	const columns = Math.max(...complete.map(({ column }) => column)) + 1;
	if (rows < 3 || columns < 3 || rows * columns !== complete.length)
		return false;
	for (let index = 0; index < complete.length; index += 1) {
		const coordinate = complete[index];
		if (
			coordinate?.row !== Math.floor(index / columns) ||
			coordinate.column !== index % columns
		) {
			return false;
		}
	}
	if (scene.graph.edges.length % 2 !== 0) return false;
	const expectedOffsets = new Set<string>();
	for (let index = 0; index < scene.graph.edges.length; index += 2) {
		const forward = scene.graph.edges[index];
		const reverse = scene.graph.edges[index + 1];
		if (
			forward === undefined ||
			reverse === undefined ||
			forward.id !== `e${index.toString().padStart(6, "0")}` ||
			reverse.id !== `e${(index + 1).toString().padStart(6, "0")}` ||
			forward.from !== reverse.to ||
			forward.to !== reverse.from ||
			BigInt(forward.cost) !== -BigInt(reverse.cost)
		) {
			return false;
		}
		const from = /^m([0-9]{4})c([0-9]{4})$/.exec(forward.from);
		const to = /^m([0-9]{4})c([0-9]{4})$/.exec(forward.to);
		if (from === null || to === null) return false;
		const fromRow = Number(from[1]);
		const fromColumn = Number(from[2]);
		const toRow = Number(to[1]);
		const toColumn = Number(to[2]);
		const columnDistance = (toColumn - fromColumn + columns) % columns;
		const rowDistance = (toRow - fromRow + rows) % rows;
		const offset =
			fromRow === toRow &&
			columnDistance >= 1 &&
			columnDistance <= Math.floor((columns - 1) / 2)
				? `h${columnDistance}`
				: fromColumn === toColumn &&
						rowDistance >= 1 &&
						rowDistance <= Math.floor((rows - 1) / 2)
					? `v${rowDistance}`
					: undefined;
		if (offset === undefined) return false;
		const sourceOrdinal = fromRow * columns + fromColumn;
		if (sourceOrdinal === 0) expectedOffsets.add(offset);
		else if (!expectedOffsets.has(offset)) return false;
	}
	return (
		expectedOffsets.size > 0 &&
		scene.graph.edges.length === rows * columns * expectedOffsets.size * 2
	);
}

function netgenMinimumLaneSpacing(
	scene: FlowCurrentSceneV9,
): number | undefined {
	if (!isNetgenScene(scene)) return undefined;
	const positions = buildFlowNodePositions(scene.graph.nodes, {
		edges: scene.graph.edges,
		model: scene.model,
	});
	const points: FlowPoint[] = [];
	for (const node of scene.graph.nodes) {
		const point = positions.get(node.id);
		if (point === undefined) return undefined;
		points.push(point);
	}
	let minimum = Number.POSITIVE_INFINITY;
	for (const [index, point] of points.entries()) {
		for (const other of points.slice(index + 1)) {
			minimum = Math.min(
				minimum,
				Math.hypot(point.x - other.x, point.y - other.y),
			);
		}
	}
	return minimum;
}

function minimumConsecutiveSpacing(values: number[]): number {
	if (values.length <= 1) return Number.POSITIVE_INFINITY;
	values.sort((left, right) => left - right);
	let minimum = Number.POSITIVE_INFINITY;
	for (let index = 1; index < values.length; index += 1) {
		const previous = values[index - 1];
		const current = values[index];
		if (previous === undefined || current === undefined) continue;
		minimum = Math.min(minimum, current - previous);
	}
	return minimum;
}

export function automaticFlowLod(
	nodeCount: number,
	edgeCount: number,
): FlowLod {
	if (
		nodeCount <= FLOW_LOD_LIMITS.detail.nodes &&
		edgeCount <= FLOW_LOD_LIMITS.detail.edges
	) {
		return "detail";
	}
	if (
		nodeCount <= FLOW_LOD_LIMITS.structure.nodes &&
		edgeCount <= FLOW_LOD_LIMITS.structure.edges
	) {
		return "structure";
	}
	return "overview";
}

function structuredGridDimensions(
	scene: FlowCurrentSceneV9,
): Readonly<{ rows: number; columns: number }> | undefined {
	const gridgen = isGridgenScene(scene);
	const gridgraph = isGridgraphScene(scene);
	const washington = isWashingtonRandomLevelScene(scene);
	const goto = isGotoTorusScene(scene);
	if (!gridgen && !gridgraph && !washington && !goto) return undefined;
	let maximumRow = -1;
	let maximumColumn = -1;
	for (const node of scene.graph.nodes) {
		if (
			(gridgen && node.id === "super") ||
			(gridgraph && (node.id === "s" || node.id === "t")) ||
			(washington && (node.id === "s" || node.id === "t")) ||
			(goto && /^x[0-9]{4}$/.test(node.id))
		) {
			continue;
		}
		const pattern = gridgen
			? /^g([0-9]{4})c([0-9]{4})$/
			: gridgraph
				? /^q([0-9]{4})c([0-9]{4})$/
				: washington
					? /^w([0-9]{4})r([0-9]{4})$/
					: /^t([0-9]{4})c([0-9]{4})$/;
		const match = pattern.exec(node.id);
		if (match === null) return undefined;
		if (washington) {
			maximumColumn = Math.max(maximumColumn, Number(match[1]));
			maximumRow = Math.max(maximumRow, Number(match[2]));
		} else {
			maximumRow = Math.max(maximumRow, Number(match[1]));
			maximumColumn = Math.max(maximumColumn, Number(match[2]));
		}
	}
	return { rows: maximumRow + 1, columns: maximumColumn + 1 };
}

function structuredGridMinimumSpacing(
	scene: FlowCurrentSceneV9,
): number | undefined {
	if (structuredGridDimensions(scene) === undefined) return undefined;
	const gridgen = isGridgenScene(scene);
	const gridgraph = isGridgraphScene(scene);
	const washington = isWashingtonRandomLevelScene(scene);
	const xs = new Set<number>();
	const ys = new Set<number>();
	for (const node of scene.graph.nodes) {
		if (
			(gridgen && node.id === "super") ||
			(gridgraph && (node.id === "s" || node.id === "t")) ||
			(washington && (node.id === "s" || node.id === "t")) ||
			(!gridgen && !gridgraph && !washington && /^x[0-9]{4}$/.test(node.id))
		) {
			continue;
		}
		const x = Number(node.position?.x);
		const y = Number(node.position?.y);
		if (!Number.isFinite(x) || !Number.isFinite(y)) return undefined;
		xs.add(x);
		ys.add(y);
	}
	return Math.min(
		minimumConsecutiveSpacing([...xs]),
		minimumConsecutiveSpacing([...ys]),
	);
}

export function flowLodForScene(scene: FlowCurrentSceneV9): FlowLod {
	const countBased = automaticFlowLod(
		scene.graph.nodes.length,
		scene.graph.edges.length,
	);
	const directedPairCounts = new Map<string, number>();
	let maximumParallelCount = 0;
	for (const edge of scene.graph.edges) {
		const key = `${edge.from}\u0000${edge.to}`;
		const count = (directedPairCounts.get(key) ?? 0) + 1;
		directedPairCounts.set(key, count);
		maximumParallelCount = Math.max(maximumParallelCount, count);
	}
	const multiplicityAdjusted =
		countBased === "detail" &&
		maximumParallelCount > FLOW_DETAIL_PARALLEL_LANE_LIMIT
			? "structure"
			: countBased;
	// Transportation instances are admitted only up to 256 nodes / 2,048 routes.
	// Keep their bipartite basis, closed loop, and row/column prices as entities;
	// aggregating routes into Overview would erase the algorithm's core state.
	if (scene.model.kind === "transportation") {
		return multiplicityAdjusted === "detail" ? "detail" : "structure";
	}
	// Standard IBFS is admitted only to 256 nodes / 2,048 edges so its two
	// signed BFS trees and orphan queue remain inspectable. Overview clustering
	// would erase the parent, frontier, and adoption state the algorithm teaches.
	if (scene.algorithm.id === "ibfs" || scene.algorithm.id === "eibfs") {
		return multiplicityAdjusted === "detail" ? "detail" : "structure";
	}
	if (multiplicityAdjusted === "overview") return multiplicityAdjusted;
	// Entity rendering remains truthful only while individual routes can still
	// be distinguished. A graph can fit the absolute Structure limits yet have
	// so many pairwise routes that drawing every edge produces an opaque braid.
	// Overview preserves every edge in deterministic bundles and exposes their
	// source counts, so dense general MF/MCF instances cross over by density as
	// well as by the absolute node/edge ceilings.
	if (
		multiplicityAdjusted === "structure" &&
		scene.graph.edges.length >= 96 &&
		scene.graph.edges.length > scene.graph.nodes.length * 4
	) {
		return "overview";
	}
	const netgenSpacing = netgenMinimumLaneSpacing(scene);
	const netgenMinimumSpacing =
		multiplicityAdjusted === "detail"
			? FLOW_ANNOTATED_NODE_MIN_CENTER_SPACING
			: FLOW_NODE_MIN_CENTER_SPACING;
	if (netgenSpacing !== undefined && netgenSpacing < netgenMinimumSpacing) {
		return "overview";
	}
	const gridSpacing = structuredGridMinimumSpacing(scene);
	if (
		gridSpacing !== undefined &&
		gridSpacing < FLOW_ANNOTATED_NODE_MIN_CENTER_SPACING
	) {
		return "overview";
	}
	if (
		multiplicityAdjusted === "detail" &&
		isGoldbergMeshScene(scene) &&
		scene.graph.edges.length > 24
	) {
		return "structure";
	}
	return multiplicityAdjusted;
}

function prioritizedIds(
	allIds: readonly string[],
	priorityGroups: readonly (readonly string[])[],
	limit: number,
): Set<string> {
	const result = new Set<string>();
	for (const group of [...priorityGroups, allIds]) {
		for (const id of group) {
			if (result.size >= limit) return result;
			result.add(id);
		}
	}
	return result;
}

function stableAndEventLabelIds(
	allIds: readonly string[],
	stablePriorityGroups: readonly (readonly string[])[],
	eventPriorityGroups: readonly (readonly string[])[],
	limit: number,
	eventLimit: number,
): Set<string> {
	const stable = prioritizedIds(allIds, stablePriorityGroups, limit);
	const event = prioritizedIds([], eventPriorityGroups, eventLimit);
	return new Set([...stable, ...event]);
}

function nativeBipartiteDetailEdgeLabels(
	scene: FlowCurrentSceneV9,
): Set<string> {
	const allEdgeIds = scene.graph.edges.map((edge) => edge.id);
	if (
		(scene.model.kind !== "assignment" &&
			scene.model.kind !== "transportation") ||
		allEdgeIds.length <= 12
	) {
		return new Set(allEdgeIds);
	}
	const activeEdgeIds = scene.residual_arcs
		.filter((arc) => arc.active)
		.map((arc) => arc.edge_id);
	const referencedEdgeIds =
		scene.trace_event?.entity_refs.flatMap((reference) =>
			reference.kind === "node" ? [] : [reference.edge_id],
		) ?? [];
	const flowedEdgeIds = scene.edge_states
		.filter((state) => BigInt(state.flow) > 0n)
		.map((state) => state.edge_id);
	const basisEdgeIds =
		scene.model.kind === "transportation"
			? (scene.pseudoflow_forest?.arcs.map((arc) => arc.edge_id) ?? [])
			: [];
	const importantCount = new Set([
		...activeEdgeIds,
		...referencedEdgeIds,
		...flowedEdgeIds,
		...basisEdgeIds,
	]).size;
	const limit = Math.min(
		scene.model.kind === "transportation" ? 18 : 16,
		Math.max(scene.model.kind === "transportation" ? 8 : 6, importantCount),
	);
	const representativeEdgeIds = Array.from({ length: limit }, (_, index) => {
		const ordinal = Math.floor((index * allEdgeIds.length) / limit);
		return allEdgeIds[ordinal] ?? allEdgeIds[0] ?? "";
	}).filter((id) => id.length > 0);
	return prioritizedIds(
		representativeEdgeIds,
		[activeEdgeIds, referencedEdgeIds, flowedEdgeIds, basisEdgeIds],
		limit,
	);
}

function buildEntityPlan(
	scene: FlowCurrentSceneV9,
	level: "detail" | "structure",
): FlowEntityRenderPlan {
	const context = buildFlowEntityRenderContext(scene);
	const overlayPresentation = buildFlowOverlayPresentation(scene);
	const visualization = buildFlowEntityVisualization(
		scene,
		overlayPresentation,
	);
	if (level === "detail") {
		return {
			kind: "entities",
			level,
			context,
			nodes: scene.graph.nodes,
			edges: scene.graph.edges,
			nodeLabelIds: new Set(scene.graph.nodes.map((node) => node.id)),
			edgeLabelIds: nativeBipartiteDetailEdgeLabels(scene),
			visualization,
			overlayPresentation,
		};
	}

	const terminals = terminalIds(scene);
	const queuedTraceNodeIds = scene.node_trace_states
		.filter((state) => state.search_ordinal !== undefined)
		.sort(
			(left, right) => (left.search_ordinal ?? 0) - (right.search_ordinal ?? 0),
		)
		.map((state) => state.node_id);
	const annotatedTraceNodeIds = scene.node_trace_states
		.filter(
			(state) =>
				state.label !== undefined ||
				state.remaining_divergence !== undefined ||
				state.search_ordinal !== undefined,
		)
		.map((state) => state.node_id);
	const referencedNodeIds =
		scene.trace_event?.entity_refs.flatMap((reference) =>
			reference.kind === "node" ? [reference.node_id] : [],
		) ?? [];
	const terminalNodeIds = terminals ? [terminals.source, terminals.sink] : [];
	const balanceNodeIds = scene.graph.nodes
		.filter((node) => BigInt(node.supply) !== 0n)
		.map((node) => node.id);
	const specialNodeIds = isGridgenScene(scene) ? ["super"] : [];
	const eibfsOverlay = overlayPresentation.renderData.overlayViews.eibfs;
	const eibfsRootNodeIds =
		eibfsOverlay?.nodes
			.filter((node) => node.root_kind !== "none")
			.map((node) => node.node_id) ?? [];
	const eibfsOrphanNodeIds =
		eibfsOverlay?.nodes
			.filter((node) => node.orphan)
			.map((node) => node.node_id) ?? [];
	const eibfsFrontierNodeIds =
		eibfsOverlay?.nodes
			.filter((node) =>
				!node.orphan && eibfsOverlay.phase_direction === "forward"
					? node.membership === "source" &&
						node.source_label === eibfsOverlay.source_depth
					: !node.orphan &&
						node.membership === "sink" &&
						node.sink_label === eibfsOverlay.sink_depth,
			)
			.map((node) => node.node_id) ?? [];
	const allNodeIds = scene.graph.nodes.map((node) => node.id);
	const nodeLabelLimit =
		scene.model.kind === "transportation"
			? allNodeIds.length
			: FLOW_LOD_LIMITS.structureNodeLabels;

	const activeEdgeIds = scene.residual_arcs
		.filter((arc) => arc.active)
		.map((arc) => arc.edge_id);
	const fixedEdgeIds = scene.residual_arcs
		.filter((arc) => arc.fixed)
		.map((arc) => arc.edge_id);
	const basisEdgeIds =
		scene.pseudoflow_forest?.arcs.map((arc) => arc.edge_id) ?? [];
	const eibfsForestEdgeIds =
		eibfsOverlay?.forest_arcs.map(
			(relation) => relation.admissible_residual.edge_id,
		) ?? [];
	const referencedEdgeIds =
		scene.trace_event?.entity_refs.flatMap((reference) =>
			reference.kind === "node" ? [] : [reference.edge_id],
		) ?? [];
	const sourceSide = new Set(cutReachableIds(scene));
	const cutEdgeIds = scene.graph.edges
		.filter((edge) =>
			crossesCertificateCut(scene, sourceSide, edge.from, edge.to),
		)
		.map((edge) => edge.id);
	const flowedEdgeIds = scene.edge_states
		.filter((state) => BigInt(state.flow) > 0n)
		.map((state) => state.edge_id);
	const allEdgeIds = scene.graph.edges.map((edge) => edge.id);
	const signedPairMesh = isGoldbergMeshScene(scene);
	const edgeLabelLimit = signedPairMesh
		? 12
		: FLOW_LOD_LIMITS.structureEdgeLabels;
	const fallbackEdgeIds = signedPairMesh
		? allEdgeIds
				.filter((_, index) => index % Math.ceil(allEdgeIds.length / 12) === 0)
				.slice(0, 12)
		: allEdgeIds;

	return {
		kind: "entities",
		level,
		context,
		nodes: scene.graph.nodes,
		edges: scene.graph.edges,
		nodeLabelIds: stableAndEventLabelIds(
			allNodeIds,
			[terminalNodeIds, specialNodeIds, balanceNodeIds],
			[
				eibfsOrphanNodeIds,
				eibfsFrontierNodeIds,
				eibfsRootNodeIds,
				queuedTraceNodeIds,
				referencedNodeIds,
				annotatedTraceNodeIds,
			],
			nodeLabelLimit,
			FLOW_LOD_LIMITS.structureNodeEventLabels,
		),
		edgeLabelIds: stableAndEventLabelIds(
			fallbackEdgeIds,
			[],
			[
				activeEdgeIds,
				eibfsForestEdgeIds,
				basisEdgeIds,
				fixedEdgeIds,
				referencedEdgeIds,
				cutEdgeIds,
				flowedEdgeIds,
			],
			edgeLabelLimit,
			FLOW_LOD_LIMITS.structureEdgeEventLabels,
		),
		visualization,
		overlayPresentation,
	};
}

function clusterCell(
	x: number,
	y: number,
	columns: number,
	rows: number,
): Readonly<{ column: number; row: number; id: string }> {
	const column = Math.max(
		0,
		Math.min(columns - 1, Math.floor((x / FLOW_VIEWBOX_WIDTH) * columns)),
	);
	const row = Math.max(
		0,
		Math.min(rows - 1, Math.floor((y / FLOW_VIEWBOX_HEIGHT) * rows)),
	);
	return { column, row, id: `cluster:${row}:${column}` };
}

function buildClusters(
	scene: FlowCurrentSceneV9,
	columns: number,
	rows: number,
): Readonly<{
	clusters: FlowOverviewCluster[];
	nodeClusterIds: Map<string, string>;
	clusterNodes: FlowNodeV1[];
}> {
	const positions = buildFlowNodePositions(scene.graph.nodes, {
		edges: scene.graph.edges,
		model: scene.model,
	});
	const sourceSide = new Set(cutReachableIds(scene));
	const terminals = terminalIds(scene);
	const gridgen = isGridgenScene(scene);
	const traceNodeIdentities = new Map(
		scene.trace_event?.entity_refs.flatMap((reference) =>
			reference.kind === "node"
				? [[reference.node_id, `node:${reference.node_id}`] as const]
				: [],
		) ?? [],
	);
	const changedNodeIdentities = new Map(
		scene.trace_event_semantics?.changed_entity_refs.flatMap((reference) =>
			reference.kind === "node"
				? [[reference.node_id, `node:${reference.node_id}`] as const]
				: [],
		) ?? [],
	);
	const mutable = new Map<string, MutableCluster>();
	const nodeClusterIds = new Map<string, string>();

	for (const node of scene.graph.nodes) {
		const position = positions.get(node.id);
		if (position === undefined) continue;
		const cell = clusterCell(position.x, position.y, columns, rows);
		const cluster = mutable.get(cell.id) ?? {
			id: cell.id,
			xTotal: 0,
			yTotal: 0,
			memberCount: 0,
			sourceSideCount: 0,
			containsSource: false,
			containsSink: false,
			supplyCount: 0,
			demandCount: 0,
			netBalance: 0n,
			containsSupernode: false,
			traceCount: 0,
			traceIdentities: [],
			changeCount: 0,
			changedIdentities: [],
		};
		cluster.xTotal += position.x;
		cluster.yTotal += position.y;
		cluster.memberCount += 1;
		if (sourceSide.has(node.id)) cluster.sourceSideCount += 1;
		if (terminals?.source === node.id) {
			cluster.containsSource = true;
		}
		if (terminals?.sink === node.id) {
			cluster.containsSink = true;
		}
		const balance = BigInt(node.supply);
		if (balance > 0n) cluster.supplyCount += 1;
		if (balance < 0n) cluster.demandCount += 1;
		cluster.netBalance += balance;
		if (gridgen && node.id === "super") cluster.containsSupernode = true;
		const traceIdentity = traceNodeIdentities.get(node.id);
		if (traceIdentity !== undefined) {
			cluster.traceCount += 1;
			cluster.traceIdentities.push(traceIdentity);
		}
		const changedIdentity = changedNodeIdentities.get(node.id);
		if (changedIdentity !== undefined) {
			cluster.changeCount += 1;
			cluster.changedIdentities.push(changedIdentity);
		}
		mutable.set(cell.id, cluster);
		nodeClusterIds.set(node.id, cell.id);
	}

	const clusters = [...mutable.values()]
		.sort((left, right) => compareText(left.id, right.id))
		.map((cluster): FlowOverviewCluster => {
			const terminal = cluster.containsSource
				? cluster.containsSink
					? "both"
					: "source"
				: cluster.containsSink
					? "sink"
					: "none";
			const terminalLabel = cluster.containsSource
				? cluster.containsSink
					? "s/t"
					: "s"
				: cluster.containsSink
					? "t"
					: undefined;
			const balance =
				cluster.supplyCount > 0
					? cluster.demandCount > 0
						? "mixed"
						: "supply"
					: cluster.demandCount > 0
						? "demand"
						: "none";
			return {
				id: cluster.id,
				x: Math.round(cluster.xTotal / cluster.memberCount),
				y: Math.round(cluster.yTotal / cluster.memberCount),
				memberCount: cluster.memberCount,
				sourceSide:
					cluster.sourceSideCount === 0
						? "none"
						: cluster.sourceSideCount === cluster.memberCount
							? "all"
							: "mixed",
				terminal,
				...(terminalLabel === undefined ? {} : { terminalLabel }),
				balance,
				supplyCount: cluster.supplyCount,
				demandCount: cluster.demandCount,
				netBalance: cluster.netBalance,
				containsSupernode: cluster.containsSupernode,
				traceCount: cluster.traceCount,
				traceIdentities: cluster.traceIdentities,
				changeCount: cluster.changeCount,
				changedIdentities: cluster.changedIdentities,
			};
		});
	const clusterNodes = clusters.map(
		(cluster): FlowNodeV1 => ({
			id: cluster.id,
			supply: "0",
			position: { x: cluster.x.toString(), y: cluster.y.toString() },
		}),
	);
	return { clusters, nodeClusterIds, clusterNodes };
}

function originalGroupKey(from: string, to: string): string {
	return `${from}\u0000${to}`;
}

function residualGroupKey(from: string, to: string): string {
	return `${from}\u0000${to}`;
}

function buildOverviewAt(
	scene: FlowCurrentSceneV9,
	columns: number,
	rows: number,
): FlowOverviewRenderPlan {
	const { clusters, nodeClusterIds, clusterNodes } = buildClusters(
		scene,
		columns,
		rows,
	);
	const edgeStates = new Map(
		scene.edge_states.map((state) => [state.edge_id, BigInt(state.flow)]),
	);
	const activeEdgeIds = new Set(
		scene.residual_arcs.filter((arc) => arc.active).map((arc) => arc.edge_id),
	);
	const fixedEdgeIds = new Set(
		scene.residual_arcs.filter((arc) => arc.fixed).map((arc) => arc.edge_id),
	);
	const tracedOriginalEdges = new Map(
		scene.trace_event?.entity_refs.flatMap((reference) =>
			reference.kind === "edge"
				? [[reference.edge_id, `edge:${reference.edge_id}`] as const]
				: [],
		) ?? [],
	);
	const tracedResidualArcs = new Map(
		scene.trace_event?.entity_refs.flatMap((reference) =>
			reference.kind === "residual-arc"
				? [
						[
							`${reference.edge_id}:${reference.direction}`,
							`residual-arc:${reference.edge_id}:${reference.direction}`,
						] as const,
					]
				: [],
		) ?? [],
	);
	const changedOriginalEdges = new Map(
		scene.trace_event_semantics?.changed_entity_refs.flatMap((reference) =>
			reference.kind === "edge"
				? [[reference.edge_id, `edge:${reference.edge_id}`] as const]
				: [],
		) ?? [],
	);
	const changedResidualArcs = new Map(
		scene.trace_event_semantics?.changed_entity_refs.flatMap((reference) =>
			reference.kind === "residual-arc"
				? [
						[
							`${reference.edge_id}:${reference.direction}`,
							`residual-arc:${reference.edge_id}:${reference.direction}`,
						] as const,
					]
				: [],
		) ?? [],
	);
	const sourceSide = new Set(cutReachableIds(scene));
	const originalGroups = new Map<string, MutableOriginalEdge>();

	for (const edge of scene.graph.edges) {
		const from = nodeClusterIds.get(edge.from);
		const to = nodeClusterIds.get(edge.to);
		if (from === undefined || to === undefined) continue;
		const cost = BigInt(edge.cost);
		const costKind = cost < 0n ? "negative" : cost > 0n ? "positive" : "zero";
		const key = originalGroupKey(from, to);
		const traceIdentity = tracedOriginalEdges.get(edge.id);
		const changedIdentity = changedOriginalEdges.get(edge.id);
		const existing = originalGroups.get(key);
		if (existing === undefined) {
			originalGroups.set(key, {
				id: `overview-original:${key}`,
				from,
				to,
				edgeCount: 1,
				capacity: BigInt(edge.capacity),
				flow: edgeStates.get(edge.id) ?? BigInt(edge.lower),
				costKind,
				minimumCost: cost,
				maximumCost: cost,
				activeCount: activeEdgeIds.has(edge.id) ? 1 : 0,
				fixedCount: fixedEdgeIds.has(edge.id) ? 1 : 0,
				cutCount: crossesCertificateCut(scene, sourceSide, edge.from, edge.to)
					? 1
					: 0,
				traceCount: traceIdentity === undefined ? 0 : 1,
				traceIdentities: traceIdentity === undefined ? [] : [traceIdentity],
				changeCount: changedIdentity === undefined ? 0 : 1,
				changedIdentities:
					changedIdentity === undefined ? [] : [changedIdentity],
			});
			continue;
		}
		existing.edgeCount += 1;
		existing.capacity += BigInt(edge.capacity);
		existing.flow += edgeStates.get(edge.id) ?? BigInt(edge.lower);
		if (existing.costKind !== costKind) existing.costKind = "mixed";
		if (cost < existing.minimumCost) existing.minimumCost = cost;
		if (cost > existing.maximumCost) existing.maximumCost = cost;
		if (activeEdgeIds.has(edge.id)) existing.activeCount += 1;
		if (fixedEdgeIds.has(edge.id)) existing.fixedCount += 1;
		if (crossesCertificateCut(scene, sourceSide, edge.from, edge.to)) {
			existing.cutCount += 1;
		}
		if (traceIdentity !== undefined) {
			existing.traceCount += 1;
			existing.traceIdentities = [...existing.traceIdentities, traceIdentity];
		}
		if (changedIdentity !== undefined) {
			existing.changeCount += 1;
			existing.changedIdentities = [
				...existing.changedIdentities,
				changedIdentity,
			];
		}
	}

	const residualGroups = new Map<string, MutableResidualArc>();
	for (const arc of scene.residual_arcs) {
		const from = nodeClusterIds.get(arc.from);
		const to = nodeClusterIds.get(arc.to);
		if (from === undefined || to === undefined) continue;
		const key = residualGroupKey(from, to);
		const traceIdentity = tracedResidualArcs.get(
			`${arc.edge_id}:${arc.direction}`,
		);
		const changedIdentity = changedResidualArcs.get(
			`${arc.edge_id}:${arc.direction}`,
		);
		const existing = residualGroups.get(key);
		if (existing === undefined) {
			residualGroups.set(key, {
				id: `overview-residual:${key}`,
				from,
				to,
				arcCount: 1,
				capacity: BigInt(arc.capacity),
				direction: arc.direction,
				activeCount: arc.active ? 1 : 0,
				fixedCount: arc.fixed ? 1 : 0,
				traceCount: traceIdentity === undefined ? 0 : 1,
				traceIdentities: traceIdentity === undefined ? [] : [traceIdentity],
				changeCount: changedIdentity === undefined ? 0 : 1,
				changedIdentities:
					changedIdentity === undefined ? [] : [changedIdentity],
			});
			continue;
		}
		existing.arcCount += 1;
		existing.capacity += BigInt(arc.capacity);
		if (existing.direction !== arc.direction) existing.direction = "mixed";
		if (arc.active) existing.activeCount += 1;
		if (arc.fixed) existing.fixedCount += 1;
		if (traceIdentity !== undefined) {
			existing.traceCount += 1;
			existing.traceIdentities = [...existing.traceIdentities, traceIdentity];
		}
		if (changedIdentity !== undefined) {
			existing.changeCount += 1;
			existing.changedIdentities = [
				...existing.changedIdentities,
				changedIdentity,
			];
		}
	}

	const orderedOriginal = [...originalGroups.values()].sort((left, right) =>
		compareText(left.id, right.id),
	);
	const orderedResidual = [...residualGroups.values()].sort((left, right) =>
		compareText(left.id, right.id),
	);
	const syntheticEdges: FlowEdgeV1[] = [
		...orderedOriginal.map((edge) => ({
			id: edge.id,
			from: edge.from,
			to: edge.to,
			lower: "0",
			capacity: edge.capacity.toString(),
			cost:
				edge.costKind === "negative"
					? "-1"
					: edge.costKind === "positive"
						? "1"
						: "0",
		})),
		...orderedResidual.map((arc) => ({
			id: arc.id,
			from: arc.from,
			to: arc.to,
			lower: "0",
			capacity: arc.capacity.toString(),
			cost: "0",
		})),
	];
	const layout = buildFlowLayout(clusterNodes, syntheticEdges, {
		labelEdgeIds: new Set(),
	});
	const originalEdges = orderedOriginal.flatMap((edge) => {
		const route = layout.routes.get(edge.id);
		return route === undefined ? [] : [{ ...edge, route }];
	});
	const residualArcs = orderedResidual.flatMap((arc) => {
		const route = layout.routes.get(arc.id);
		return route === undefined ? [] : [{ ...arc, route }];
	});
	return {
		kind: "overview",
		level: "overview",
		grid: { columns, rows },
		clusters,
		originalEdges,
		residualArcs,
	};
}

function buildOverviewPlan(scene: FlowCurrentSceneV9): FlowOverviewRenderPlan {
	const detailed = buildOverviewAt(scene, 6, 4);
	if (
		detailed.originalEdges.length + detailed.residualArcs.length <=
		FLOW_LOD_LIMITS.overviewAggregateMarks
	) {
		return detailed;
	}
	return buildOverviewAt(scene, 4, 3);
}

/**
 * Builds a deterministic, allocation-bounded projection for the SVG renderer.
 * A caller-controlled level lets the viewport-aware LOD policy retain
 * hysteresis while this module remains responsible for bounded projection.
 */
export function buildFlowRenderPlan(
	scene: FlowCurrentSceneV9,
	level: FlowLod = flowLodForScene(scene),
): FlowRenderPlan {
	const safeLevel = constrainFlowCanvasLodToRenderLimits(level, {
		nodes: scene.graph.nodes.length,
		edges: scene.graph.edges.length + scene.residual_arcs.length,
	});
	return safeLevel === "overview"
		? buildOverviewPlan(scene)
		: buildEntityPlan(scene, safeLevel);
}
