import { directedFlowBipartition } from "./flow-graph-shape";
import type { FlowEdgeV1, FlowNodeV1, FlowProblemModelV1 } from "./flow-scene";

export const FLOW_VIEWBOX_WIDTH = 900;
export const FLOW_VIEWBOX_HEIGHT = 540;
export const FLOW_NODE_RADIUS = 29;
/** Visible node diameter plus a 4px separation channel. */
export const FLOW_NODE_MIN_CENTER_SPACING = FLOW_NODE_RADIUS * 2 + 4;
/** Largest same-direction parallel group rendered as individually numbered lanes. */
export const FLOW_DETAIL_PARALLEL_LANE_LIMIT = 9;

export type FlowPoint = Readonly<{ x: number; y: number }>;

export type FlowEdgeRoute = Readonly<{
	edgeId: string;
	from: string;
	to: string;
	path: string;
	reversePath: string;
	label: FlowPoint;
	labelWidth: number;
	/** Exact background/collision width used by the renderer and layout. */
	labelBoxWidth: number;
	labelHeight: number;
	labelYOffset: number;
	labelCollisionFree: boolean;
	/** Point on this exact route connected to the annotation by a leader. */
	labelAnchor: FlowPoint;
	/** Stable point on this exact route used only by the lane identity token. */
	laneToken: FlowPoint;
	/** Tangent direction at `laneToken`, in SVG degrees. */
	laneTokenAngle: number;
	/** Stable midpoint used to enforce the bounded parallel-lane spread. */
	routeMidpoint: FlowPoint;
	/** Stable one-based lane within original edges sharing the same direction. */
	parallelIndex: number;
	parallelCount: number;
	residualForwardLabel: FlowPoint;
	residualReverseLabel: FlowPoint;
	selfLoop: boolean;
}>;

export type FlowLayout = Readonly<{
	positions: ReadonlyMap<string, FlowPoint>;
	routes: ReadonlyMap<string, FlowEdgeRoute>;
}>;

export type FlowLayoutOptions = Readonly<{
	/** Only these edge labels participate in collision placement. */
	labelEdgeIds?: ReadonlySet<string>;
	/** Event-local labels placed before stable context labels. */
	labelPriorityEdgeIds?: readonly string[];
	/** Rendered label dimensions beyond the edge's base value label. */
	labelMetrics?: ReadonlyMap<
		string,
		Readonly<{ widthAddition: number; height: number; yOffset: number }>
	>;
	/** Problem semantics used only when a node has no explicit coordinates. */
	model?: FlowProblemModelV1;
	/** Override automatic semantic placement for diagnostics and fixtures. */
	placement?: FlowPlacementPolicy;
}>;

export type FlowPlacementPolicy =
	| "auto"
	| "terminal"
	| "balance"
	| "bipartite"
	| "circular";

export type FlowNodePositionOptions = Readonly<{
	edges?: readonly FlowEdgeV1[];
	model?: FlowProblemModelV1;
	placement?: FlowPlacementPolicy;
}>;

type Curve = Readonly<{
	path: string;
	reversePath: string;
	at: (t: number) => FlowPoint;
	tangent: (t: number) => FlowPoint;
}>;

type PendingRoute = Readonly<{
	edge: FlowEdgeV1;
	curve: Curve;
	selfLoop: boolean;
	parallelIndex: number;
	parallelCount: number;
}>;

type LabelBox = Readonly<{
	left: number;
	right: number;
	top: number;
	bottom: number;
}>;

const MAX_LANE_SPREAD = 160;
const ROUTE_VIEWBOX_MARGIN = 16;
const LABEL_HEIGHT = 24;
const POSITION_MIN_X = 68;
const POSITION_MAX_X = 832;
const POSITION_MIN_Y = 68;
const POSITION_MAX_Y = 472;
const SEMANTIC_MIN_Y = 78;
const SEMANTIC_MAX_Y = 462;

function compareText(left: string, right: string): number {
	return left < right ? -1 : left > right ? 1 : 0;
}

function finiteCoordinate(
	value: string | undefined,
	fallback: number,
	minimum: number,
	maximum: number,
): number {
	if (value === undefined || value.length === 0) return fallback;
	try {
		const parsed = BigInt(value);
		if (parsed < BigInt(minimum)) return minimum;
		if (parsed > BigInt(maximum)) return maximum;
		return Number(parsed);
	} catch {
		return fallback;
	}
}

function formatNumber(value: number): string {
	const rounded = Math.round(value * 1_000) / 1_000;
	return Object.is(rounded, -0) ? "0" : rounded.toString();
}

function pathPoint(point: FlowPoint): string {
	return `${formatNumber(point.x)} ${formatNumber(point.y)}`;
}

function add(left: FlowPoint, right: FlowPoint): FlowPoint {
	return { x: left.x + right.x, y: left.y + right.y };
}

function subtract(left: FlowPoint, right: FlowPoint): FlowPoint {
	return { x: left.x - right.x, y: left.y - right.y };
}

function scale(point: FlowPoint, factor: number): FlowPoint {
	return { x: point.x * factor, y: point.y * factor };
}

function length(point: FlowPoint): number {
	return Math.hypot(point.x, point.y);
}

function normalized(point: FlowPoint): FlowPoint {
	const magnitude = length(point);
	return magnitude < 1e-9
		? { x: 1, y: 0 }
		: { x: point.x / magnitude, y: point.y / magnitude };
}

function normal(point: FlowPoint): FlowPoint {
	const unit = normalized(point);
	return { x: -unit.y, y: unit.x };
}

function quadraticCurve(
	from: FlowPoint,
	to: FlowPoint,
	laneOffset: number,
): Curve {
	const direction = normalized(subtract(to, from));
	const side = normal(direction);
	const start = add(from, scale(direction, FLOW_NODE_RADIUS + 8));
	const end = add(to, scale(direction, -(FLOW_NODE_RADIUS + 14)));
	const midpoint = scale(add(from, to), 0.5);
	const control = add(midpoint, scale(side, laneOffset * 2));
	const at = (t: number): FlowPoint => {
		const inverse = 1 - t;
		return add(
			add(scale(start, inverse * inverse), scale(control, 2 * inverse * t)),
			scale(end, t * t),
		);
	};
	const tangent = (t: number): FlowPoint =>
		add(
			scale(subtract(control, start), 2 * (1 - t)),
			scale(subtract(end, control), 2 * t),
		);
	return {
		path: `M ${pathPoint(start)} Q ${pathPoint(control)} ${pathPoint(end)}`,
		reversePath: `M ${pathPoint(end)} Q ${pathPoint(control)} ${pathPoint(start)}`,
		at,
		tangent,
	};
}

function loopDirection(center: FlowPoint, edgeId: string): FlowPoint {
	const towardCenter = subtract(
		{
			x: FLOW_VIEWBOX_WIDTH / 2,
			y: FLOW_VIEWBOX_HEIGHT / 2,
		},
		center,
	);
	if (length(towardCenter) > 145) return normalized(towardCenter);
	let hash = 2_166_136_261;
	for (const codePoint of edgeId) {
		hash ^= codePoint.codePointAt(0) ?? 0;
		hash = Math.imul(hash, 16_777_619) >>> 0;
	}
	const angle = ((hash % 4) * Math.PI) / 2 - Math.PI / 2;
	return { x: Math.cos(angle), y: Math.sin(angle) };
}

function cubicLoopCurve(
	center: FlowPoint,
	edgeId: string,
	loopIndex: number,
	loopCount: number,
): Curve {
	const base = loopDirection(center, edgeId);
	const ring = Math.floor(loopIndex / 3);
	const ringCount = Math.max(1, Math.ceil(loopCount / 3));
	const ringProgress = ringCount === 1 ? 0 : ring / (ringCount - 1);
	const fan = (loopIndex % 3) - 1;
	const fanAngle = fan * 0.34;
	const direction = {
		x: base.x * Math.cos(fanAngle) - base.y * Math.sin(fanAngle),
		y: base.x * Math.sin(fanAngle) + base.y * Math.cos(fanAngle),
	};
	const side = normal(direction);
	const radial = 64 + ringProgress * 180;
	const lateral = 46 + ringProgress * 92;
	const start = add(
		center,
		add(scale(direction, 21), scale(side, FLOW_NODE_RADIUS - 8)),
	);
	const end = add(
		center,
		add(scale(direction, 21), scale(side, -(FLOW_NODE_RADIUS - 8))),
	);
	const rawFirstControl = add(
		center,
		add(scale(direction, radial), scale(side, lateral)),
	);
	const rawSecondControl = add(
		center,
		add(scale(direction, radial), scale(side, -lateral)),
	);
	const clampRoutePoint = (point: FlowPoint): FlowPoint => ({
		x: Math.min(
			FLOW_VIEWBOX_WIDTH - ROUTE_VIEWBOX_MARGIN,
			Math.max(ROUTE_VIEWBOX_MARGIN, point.x),
		),
		y: Math.min(
			FLOW_VIEWBOX_HEIGHT - ROUTE_VIEWBOX_MARGIN,
			Math.max(ROUTE_VIEWBOX_MARGIN, point.y),
		),
	});
	const firstControl = clampRoutePoint(rawFirstControl);
	const secondControl = clampRoutePoint(rawSecondControl);
	const at = (t: number): FlowPoint => {
		const inverse = 1 - t;
		return add(
			add(
				add(
					scale(start, inverse ** 3),
					scale(firstControl, 3 * inverse ** 2 * t),
				),
				scale(secondControl, 3 * inverse * t * t),
			),
			scale(end, t ** 3),
		);
	};
	const tangent = (t: number): FlowPoint => {
		const inverse = 1 - t;
		return add(
			add(
				scale(subtract(firstControl, start), 3 * inverse * inverse),
				scale(subtract(secondControl, firstControl), 6 * inverse * t),
			),
			scale(subtract(end, secondControl), 3 * t * t),
		);
	};
	return {
		path: `M ${pathPoint(start)} C ${pathPoint(firstControl)} ${pathPoint(secondControl)} ${pathPoint(end)}`,
		reversePath: `M ${pathPoint(end)} C ${pathPoint(secondControl)} ${pathPoint(firstControl)} ${pathPoint(start)}`,
		at,
		tangent,
	};
}

function circularFallbacks(
	nodes: readonly FlowNodeV1[],
): Map<string, FlowPoint> {
	const result = new Map<string, FlowPoint>();
	const count = Math.max(1, nodes.length);
	for (const [index, node] of nodes.entries()) {
		const angle = -Math.PI / 2 + (index / count) * Math.PI * 2;
		result.set(node.id, {
			x: FLOW_VIEWBOX_WIDTH / 2 + Math.cos(angle) * 310,
			y: FLOW_VIEWBOX_HEIGHT / 2 + Math.sin(angle) * 185,
		});
	}
	return result;
}

function distributeColumns(
	columns: ReadonlyMap<number, readonly FlowNodeV1[]>,
	xAt: (column: number) => number,
): Map<string, FlowPoint> {
	const result = new Map<string, FlowPoint>();
	for (const column of [...columns.keys()].sort(
		(left, right) => left - right,
	)) {
		const members = [...(columns.get(column) ?? [])].sort((left, right) =>
			compareText(left.id, right.id),
		);
		for (const [index, node] of members.entries()) {
			const ratio = (index + 1) / (members.length + 1);
			result.set(node.id, {
				x: xAt(column),
				y: SEMANTIC_MIN_Y + ratio * (SEMANTIC_MAX_Y - SEMANTIC_MIN_Y),
			});
		}
	}
	return result;
}

function shortestDistances(
	root: string,
	nodes: readonly FlowNodeV1[],
	edges: readonly FlowEdgeV1[],
	reverse: boolean,
): Map<string, number> {
	const adjacency = new Map(nodes.map((node) => [node.id, [] as string[]]));
	for (const edge of edges) {
		const from = reverse ? edge.to : edge.from;
		const to = reverse ? edge.from : edge.to;
		adjacency.get(from)?.push(to);
	}
	for (const neighbors of adjacency.values()) neighbors.sort(compareText);
	const distances = new Map<string, number>();
	if (!adjacency.has(root)) return distances;
	distances.set(root, 0);
	const queue = [root];
	for (let cursor = 0; cursor < queue.length; cursor += 1) {
		const current = queue[cursor];
		if (current === undefined) continue;
		const distance = distances.get(current);
		if (distance === undefined) continue;
		for (const neighbor of adjacency.get(current) ?? []) {
			if (distances.has(neighbor)) continue;
			distances.set(neighbor, distance + 1);
			queue.push(neighbor);
		}
	}
	return distances;
}

function terminalFallbacks(
	nodes: readonly FlowNodeV1[],
	edges: readonly FlowEdgeV1[],
	model: FlowProblemModelV1 | undefined,
): Map<string, FlowPoint> | undefined {
	if (model === undefined) {
		return undefined;
	}
	const terminals =
		model.kind === "max-flow" ||
		model.kind === "planar-max-flow" ||
		model.kind === "fixed-flow-min-cost" ||
		model.kind === "min-cost-max-flow"
			? { source: model.source, sink: model.sink }
			: model.kind === "bipartite-matching" && model.flow_adapter !== undefined
				? model.flow_adapter
				: undefined;
	if (terminals === undefined) return undefined;
	const fromSource = shortestDistances(terminals.source, nodes, edges, false);
	const toSink = shortestDistances(terminals.sink, nodes, edges, true);
	const columns = new Map<number, FlowNodeV1[]>();
	for (const node of nodes) {
		const sourceDistance = fromSource.get(node.id);
		const sinkDistance = toSink.get(node.id);
		let column: number;
		if (node.id === terminals.source) column = 0;
		else if (node.id === terminals.sink) column = 8;
		else if (sourceDistance !== undefined && sinkDistance !== undefined) {
			const denominator = sourceDistance + sinkDistance;
			column =
				denominator === 0 ? 4 : Math.round((sourceDistance / denominator) * 8);
			column = Math.max(1, Math.min(7, column));
		} else if (sourceDistance !== undefined) {
			column = Math.max(1, Math.min(6, sourceDistance));
		} else if (sinkDistance !== undefined) {
			column = Math.max(2, Math.min(7, 8 - sinkDistance));
		} else {
			column = 4;
		}
		const members = columns.get(column);
		if (members === undefined) columns.set(column, [node]);
		else members.push(node);
	}
	return distributeColumns(columns, (column) => 100 + column * 87.5);
}

function parseSupply(node: FlowNodeV1): bigint {
	try {
		return BigInt(node.supply);
	} catch {
		return 0n;
	}
}

function balanceFallbacks(
	nodes: readonly FlowNodeV1[],
): Map<string, FlowPoint> | undefined {
	if (!nodes.some((node) => parseSupply(node) !== 0n)) return undefined;
	const groups: FlowNodeV1[][] = [[], [], []];
	for (const node of nodes) {
		const supply = parseSupply(node);
		groups[supply > 0n ? 0 : supply < 0n ? 2 : 1]?.push(node);
	}

	const rowsPerColumn =
		Math.floor(
			(SEMANTIC_MAX_Y - SEMANTIC_MIN_Y) / FLOW_NODE_MIN_CENTER_SPACING,
		) + 1;
	const nonemptyGroups = groups.filter((group) => group.length > 0);
	const groupColumnCounts = nonemptyGroups.map((group) =>
		Math.ceil(group.length / rowsPerColumn),
	);
	// Keep one empty semantic column between supply, neutral, and demand bands.
	// This preserves the left-to-right balance reading while allowing a large
	// zero-balance set to wrap instead of collapsing into one overlapping stack.
	const requiredSlots =
		groupColumnCounts.reduce((total, count) => total + count, 0) +
		Math.max(0, nonemptyGroups.length - 1);
	const maximumSlots =
		Math.floor(
			(POSITION_MAX_X - POSITION_MIN_X) / FLOW_NODE_MIN_CENTER_SPACING,
		) + 1;
	if (requiredSlots > maximumSlots) return undefined;

	const result = new Map<string, FlowPoint>();
	const xSpacing =
		requiredSlots === 1
			? 0
			: (POSITION_MAX_X - POSITION_MIN_X) / (requiredSlots - 1);
	let slot = 0;
	for (const [groupIndex, group] of nonemptyGroups.entries()) {
		const ordered = [...group].sort((left, right) =>
			compareText(left.id, right.id),
		);
		const columnCount = groupColumnCounts[groupIndex] ?? 1;
		for (let column = 0; column < columnCount; column += 1) {
			const members = ordered.slice(
				column * rowsPerColumn,
				(column + 1) * rowsPerColumn,
			);
			for (const [row, node] of members.entries()) {
				const ratio = members.length === 1 ? 0.5 : row / (members.length - 1);
				result.set(node.id, {
					x:
						requiredSlots === 1
							? FLOW_VIEWBOX_WIDTH / 2
							: POSITION_MIN_X + (slot + column) * xSpacing,
					y: SEMANTIC_MIN_Y + ratio * (SEMANTIC_MAX_Y - SEMANTIC_MIN_Y),
				});
			}
		}
		slot += columnCount;
		if (groupIndex < nonemptyGroups.length - 1) slot += 1;
	}
	return result;
}

function bipartiteFallbacks(
	nodes: readonly FlowNodeV1[],
	edges: readonly FlowEdgeV1[],
): Map<string, FlowPoint> | undefined {
	const partition = directedFlowBipartition(nodes, edges);
	if (partition === undefined || partition.directionCoherence < 0.8) {
		return undefined;
	}
	const columns = new Map<number, FlowNodeV1[]>([
		[0, nodes.filter((node) => partition.left.has(node.id))],
		[1, nodes.filter((node) => partition.right.has(node.id))],
	]);
	return distributeColumns(columns, (column) => (column === 0 ? 230 : 670));
}

function declaredPartitionFallbacks(
	nodes: readonly FlowNodeV1[],
	model: FlowProblemModelV1 | undefined,
): Map<string, FlowPoint> | undefined {
	const leftIds =
		model?.kind === "assignment"
			? model.agents
			: model?.kind === "transportation"
				? model.origins
				: model?.kind === "bipartite-matching"
					? model.left
					: undefined;
	const rightIds =
		model?.kind === "assignment"
			? model.tasks
			: model?.kind === "transportation"
				? model.destinations
				: model?.kind === "bipartite-matching"
					? model.right
					: undefined;
	if (leftIds === undefined || rightIds === undefined) return undefined;
	const byId = new Map(nodes.map((node) => [node.id, node]));
	const left = leftIds.flatMap((id) => {
		const node = byId.get(id);
		return node === undefined ? [] : [node];
	});
	const right = rightIds.flatMap((id) => {
		const node = byId.get(id);
		return node === undefined ? [] : [node];
	});
	if (left.length !== leftIds.length || right.length !== rightIds.length) {
		return undefined;
	}
	return distributeColumns(
		new Map<number, FlowNodeV1[]>([
			[0, left],
			[1, right],
		]),
		(column) => (column === 0 ? 230 : 670),
	);
}

function automaticFallbacks(
	nodes: readonly FlowNodeV1[],
	edges: readonly FlowEdgeV1[],
	model: FlowProblemModelV1 | undefined,
	placement: FlowPlacementPolicy,
): Map<string, FlowPoint> {
	if (placement === "terminal" || placement === "auto") {
		const terminal = terminalFallbacks(nodes, edges, model);
		if (terminal !== undefined) return terminal;
	}
	if (placement === "balance" || placement === "auto") {
		const balance = balanceFallbacks(nodes);
		if (balance !== undefined) return balance;
	}
	if (placement === "bipartite" || placement === "auto") {
		const declared = declaredPartitionFallbacks(nodes, model);
		if (declared !== undefined) return declared;
		const bipartite = bipartiteFallbacks(nodes, edges);
		if (bipartite !== undefined) return bipartite;
	}
	return circularFallbacks(nodes);
}

function isDenseSingleRowMaxFlow(
	nodes: readonly FlowNodeV1[],
	model: FlowProblemModelV1 | undefined,
): model is Extract<FlowProblemModelV1, { kind: "max-flow" }> {
	if (model?.kind !== "max-flow") return false;
	if (
		nodes.length < 2 ||
		nodes.filter((node) => node.id === model.source).length !== 1 ||
		nodes.filter((node) => node.id === model.sink).length !== 1 ||
		nodes.some((node) => node.position === undefined)
	) {
		return false;
	}
	if (new Set(nodes.map((node) => node.position?.y)).size !== 1) return false;

	const displayedX = nodes
		.map((node) =>
			finiteCoordinate(
				node.position?.x,
				FLOW_VIEWBOX_WIDTH / 2,
				POSITION_MIN_X,
				POSITION_MAX_X,
			),
		)
		.sort((left, right) => left - right);
	return displayedX.some(
		(position, index) =>
			index > 0 &&
			position - (displayedX[index - 1] ?? position) <
				FLOW_NODE_MIN_CENTER_SPACING,
	);
}

function completeDagFallbacks(
	nodes: readonly FlowNodeV1[],
	edges: readonly FlowEdgeV1[],
	model: FlowProblemModelV1 | undefined,
): Map<string, FlowPoint> | undefined {
	if (
		model?.kind !== "max-flow" ||
		nodes.length < 4 ||
		nodes.length > 12 ||
		edges.length !== (nodes.length * (nodes.length - 1)) / 2
	) {
		return undefined;
	}
	const nodeIds = new Set(nodes.map((node) => node.id));
	const indegree = new Map(nodes.map((node) => [node.id, 0]));
	const directedPairs = new Set<string>();
	for (const edge of edges) {
		const pair = `${edge.from}\u0000${edge.to}`;
		if (
			edge.from === edge.to ||
			!nodeIds.has(edge.from) ||
			!nodeIds.has(edge.to) ||
			directedPairs.has(pair)
		) {
			return undefined;
		}
		directedPairs.add(pair);
		indegree.set(edge.to, (indegree.get(edge.to) ?? 0) + 1);
	}
	const topological = [...nodes].sort((left, right) => {
		const degreeDifference =
			(indegree.get(left.id) ?? 0) - (indegree.get(right.id) ?? 0);
		return degreeDifference || compareText(left.id, right.id);
	});
	if (
		topological[0]?.id !== model.source ||
		topological.at(-1)?.id !== model.sink
	) {
		return undefined;
	}
	for (const [index, node] of topological.entries()) {
		if ((indegree.get(node.id) ?? -1) !== index) return undefined;
		for (const later of topological.slice(index + 1)) {
			if (!directedPairs.has(`${node.id}\u0000${later.id}`)) {
				return undefined;
			}
		}
	}

	const result = new Map<string, FlowPoint>();
	for (const [index, node] of topological.entries()) {
		const progress = index / (topological.length - 1);
		const centered = 2 * progress - 1;
		result.set(node.id, {
			x: POSITION_MIN_X + progress * (POSITION_MAX_X - POSITION_MIN_X),
			y: 115 + 285 * centered * centered,
		});
	}
	return result;
}

function packedSingleRowMaxFlowPositions(
	nodes: readonly FlowNodeV1[],
	model: Extract<FlowProblemModelV1, { kind: "max-flow" }>,
): Map<string, FlowPoint> | undefined {
	const result = new Map<string, FlowPoint>();
	const source = nodes.find((node) => node.id === model.source);
	const sink = nodes.find((node) => node.id === model.sink);
	const middle = nodes.filter(
		(node) => node.id !== model.source && node.id !== model.sink,
	);
	const terminalClearance = FLOW_NODE_MIN_CENTER_SPACING + 8;
	const minimumX = POSITION_MIN_X + terminalClearance;
	const maximumX = POSITION_MAX_X - terminalClearance;
	const minimumY = POSITION_MIN_Y;
	const maximumY = POSITION_MAX_Y;
	const availableWidth = maximumX - minimumX;
	const availableHeight = maximumY - minimumY;
	const maximumColumns =
		Math.floor(availableWidth / FLOW_NODE_MIN_CENTER_SPACING) + 1;
	const maximumRows =
		Math.floor(availableHeight / FLOW_NODE_MIN_CENTER_SPACING) + 1;
	if (middle.length > maximumColumns * maximumRows) return undefined;

	if (source !== undefined) {
		result.set(source.id, {
			x: POSITION_MIN_X,
			y: FLOW_VIEWBOX_HEIGHT / 2,
		});
	}
	if (sink !== undefined) {
		result.set(sink.id, {
			x: POSITION_MAX_X,
			y: FLOW_VIEWBOX_HEIGHT / 2,
		});
	}
	if (middle.length === 0) return result;

	const aspectRatio = availableWidth / availableHeight;
	const idealColumns = Math.ceil(Math.sqrt(middle.length * aspectRatio));
	const columns = Math.min(
		maximumColumns,
		Math.max(1, Math.ceil(middle.length / maximumRows), idealColumns),
	);
	const rows = Math.ceil(middle.length / columns);
	for (const [index, node] of middle.entries()) {
		const column = index % columns;
		const row = Math.floor(index / columns);
		const nodesInRow =
			row === rows - 1 ? middle.length - row * columns : columns;
		const centeredColumn = column + (columns - Math.max(1, nodesInRow)) / 2;
		result.set(node.id, {
			x:
				columns === 1
					? (minimumX + maximumX) / 2
					: minimumX +
						(centeredColumn * availableWidth) / Math.max(1, columns - 1),
			y:
				rows === 1
					? (minimumY + maximumY) / 2
					: minimumY + (row * availableHeight) / Math.max(1, rows - 1),
		});
	}
	return result;
}

export function buildFlowNodePositions(
	nodes: readonly FlowNodeV1[],
	options: FlowNodePositionOptions = {},
): Map<string, FlowPoint> {
	const ordered = [...nodes].sort((left, right) =>
		compareText(left.id, right.id),
	);
	const model = options.model;
	const semanticFallbacks =
		completeDagFallbacks(ordered, options.edges ?? [], model) ??
		(isDenseSingleRowMaxFlow(ordered, model)
			? packedSingleRowMaxFlowPositions(ordered, model)
			: undefined);
	const preferSemanticPlacement = semanticFallbacks !== undefined;
	const fallbacks =
		semanticFallbacks ??
		automaticFallbacks(
			ordered,
			options.edges ?? [],
			model,
			options.placement ?? "auto",
		);
	const positions = new Map<string, FlowPoint>();
	for (const node of ordered) {
		const fallback = fallbacks.get(node.id) ?? {
			x: FLOW_VIEWBOX_WIDTH / 2,
			y: FLOW_VIEWBOX_HEIGHT / 2,
		};
		positions.set(node.id, {
			x: finiteCoordinate(
				preferSemanticPlacement ? undefined : node.position?.x,
				fallback.x,
				POSITION_MIN_X,
				POSITION_MAX_X,
			),
			y: finiteCoordinate(
				preferSemanticPlacement ? undefined : node.position?.y,
				fallback.y,
				POSITION_MIN_Y,
				POSITION_MAX_Y,
			),
		});
	}
	return positions;
}

function pairKey(edge: FlowEdgeV1): string {
	return edge.from < edge.to
		? `${edge.from}\u0000${edge.to}`
		: `${edge.to}\u0000${edge.from}`;
}

function laneOffsetBounds(
	from: FlowPoint,
	to: FlowPoint,
): readonly [number, number] {
	const side = normal(subtract(to, from));
	const control = scale(add(from, to), 0.5);
	let minimum = Number.NEGATIVE_INFINITY;
	let maximum = Number.POSITIVE_INFINITY;
	for (const [coordinate, coefficient, lower, upper] of [
		[
			control.x,
			side.x * 2,
			ROUTE_VIEWBOX_MARGIN,
			FLOW_VIEWBOX_WIDTH - ROUTE_VIEWBOX_MARGIN,
		],
		[
			control.y,
			side.y * 2,
			ROUTE_VIEWBOX_MARGIN,
			FLOW_VIEWBOX_HEIGHT - ROUTE_VIEWBOX_MARGIN,
		],
	] as const) {
		if (Math.abs(coefficient) < 1e-9) continue;
		const first = (lower - coordinate) / coefficient;
		const second = (upper - coordinate) / coefficient;
		minimum = Math.max(minimum, Math.min(first, second));
		maximum = Math.min(maximum, Math.max(first, second));
	}
	return [minimum, maximum];
}

function fitLaneOffsets(
	offsets: readonly number[],
	from: FlowPoint,
	to: FlowPoint,
): number[] {
	if (offsets.length === 0) return [];
	const [minimum, maximum] = laneOffsetBounds(from, to);
	const rawMinimum = Math.min(...offsets);
	const rawMaximum = Math.max(...offsets);
	const rawCenter = (rawMinimum + rawMaximum) / 2;
	const availableSpread = Math.max(0, maximum - minimum);
	const scaleFactor = Math.min(
		1,
		availableSpread / Math.max(1e-9, rawMaximum - rawMinimum),
	);
	const scaled = offsets.map(
		(offset) => rawCenter + (offset - rawCenter) * scaleFactor,
	);
	const scaledMinimum = Math.min(...scaled);
	const scaledMaximum = Math.max(...scaled);
	const minimumCenter = minimum + (scaledMaximum - scaledMinimum) / 2;
	const maximumCenter = maximum - (scaledMaximum - scaledMinimum) / 2;
	const fittedCenter = Math.min(
		maximumCenter,
		Math.max(minimumCenter, (scaledMinimum + scaledMaximum) / 2),
	);
	const shift = fittedCenter - (scaledMinimum + scaledMaximum) / 2;
	return scaled.map((offset) => offset + shift);
}

function buildPendingRoutes(
	edges: readonly FlowEdgeV1[],
	positions: ReadonlyMap<string, FlowPoint>,
): PendingRoute[] {
	const groups = new Map<string, FlowEdgeV1[]>();
	for (const edge of edges) {
		const key = pairKey(edge);
		const group = groups.get(key);
		if (group === undefined) groups.set(key, [edge]);
		else group.push(edge);
	}
	const result: PendingRoute[] = [];
	for (const key of [...groups.keys()].sort(compareText)) {
		const group = groups.get(key);
		if (group === undefined) continue;
		group.sort((left, right) => compareText(left.id, right.id));
		if (group[0]?.from === group[0]?.to) {
			for (const [index, edge] of group.entries()) {
				const center = positions.get(edge.from);
				if (center === undefined) continue;
				result.push({
					edge,
					curve: cubicLoopCurve(center, edge.from, index, group.length),
					selfLoop: true,
					parallelIndex: index + 1,
					parallelCount: group.length,
				});
			}
			continue;
		}
		const [low = "", high = ""] = key.split("\u0000");
		const forward = group.filter(
			(edge) => edge.from === low && edge.to === high,
		);
		const reverse = group.filter(
			(edge) => edge.from === high && edge.to === low,
		);
		const hasOpposite = forward.length > 0 && reverse.length > 0;
		for (const directionGroup of [forward, reverse]) {
			const laneSpacing = Math.min(
				38,
				MAX_LANE_SPREAD / Math.max(1, directionGroup.length - 1),
			);
			const firstEdge = directionGroup[0];
			if (firstEdge === undefined) continue;
			const firstFrom = positions.get(firstEdge.from);
			const firstTo = positions.get(firstEdge.to);
			if (firstFrom === undefined || firstTo === undefined) continue;
			const laneOffsets = fitLaneOffsets(
				directionGroup.map((_, index) =>
					hasOpposite
						? (index + 0.7) * laneSpacing
						: (index - (directionGroup.length - 1) / 2) * laneSpacing,
				),
				firstFrom,
				firstTo,
			);
			for (const [index, edge] of directionGroup.entries()) {
				const from = positions.get(edge.from);
				const to = positions.get(edge.to);
				const laneOffset = laneOffsets[index];
				if (from === undefined || to === undefined || laneOffset === undefined)
					continue;
				result.push({
					edge,
					curve: quadraticCurve(from, to, laneOffset),
					selfLoop: false,
					parallelIndex: index + 1,
					parallelCount: directionGroup.length,
				});
			}
		}
	}
	return result.sort((left, right) => compareText(left.edge.id, right.edge.id));
}

function laneTokenTime(route: PendingRoute): number {
	if (route.selfLoop || route.parallelCount <= 1) return 0.5;
	return 0.18 + (0.64 * (route.parallelIndex - 1)) / (route.parallelCount - 1);
}

function labelWidth(edge: FlowEdgeV1): number {
	// Flow is bounded by capacity, so reserving the capacity digit budget twice
	// covers the exact `FLOW … · CAP …` line. Cost is rendered on its own line.
	const characters = Math.max(
		edge.capacity.length * 2 + 12,
		edge.cost.length + 7,
	);
	return Math.max(104, 22 + characters * 7);
}

function labelBox(
	center: FlowPoint,
	width: number,
	height = LABEL_HEIGHT,
	yOffset = 0,
): LabelBox {
	return {
		left: center.x - width / 2,
		right: center.x + width / 2,
		top: center.y + yOffset - height / 2,
		bottom: center.y + yOffset + height / 2,
	};
}

function boxesOverlap(left: LabelBox, right: LabelBox, padding = 6): boolean {
	return !(
		left.right + padding < right.left ||
		right.right + padding < left.left ||
		left.bottom + padding < right.top ||
		right.bottom + padding < left.top
	);
}

function labelHitsNode(box: LabelBox, position: FlowPoint): boolean {
	const closestX = Math.max(box.left, Math.min(position.x, box.right));
	const closestY = Math.max(box.top, Math.min(position.y, box.bottom));
	return Math.hypot(closestX - position.x, closestY - position.y) < 43;
}

function insideViewbox(box: LabelBox): boolean {
	return box.left >= 8 && box.right <= 892 && box.top >= 8 && box.bottom <= 532;
}

function chooseLabel(
	route: PendingRoute,
	positions: ReadonlyMap<string, FlowPoint>,
	placed: readonly LabelBox[],
	metrics: Readonly<{ widthAddition: number; height: number; yOffset: number }>,
): {
	point: FlowPoint;
	anchor: FlowPoint;
	box: LabelBox;
	width: number;
	boxWidth: number;
	collisionFree: boolean;
} {
	const width = labelWidth(route.edge) + (route.parallelCount > 1 ? 42 : 0);
	const collisionWidth = width + Math.max(0, metrics.widthAddition);
	const times = route.selfLoop
		? [0.5, 0.42, 0.58, 0.34, 0.66, 0.26, 0.74]
		: [0.46, 0.34, 0.6, 0.72, 0.24, 0.14, 0.84];
	const normalOffsets = route.selfLoop
		? route.parallelCount > 1
			? [20, -20, 38, -38, 56, -56]
			: [20, -20, 38, -38, 56, -56, 0]
		: route.parallelCount > 1
			? [38, -38, 56, -56, 74, -74, 92, -92]
			: [30, -30, 48, -48, 66, -66, 84, -84, 0];
	let fallback:
		| {
				point: FlowPoint;
				anchor: FlowPoint;
				box: LabelBox;
				width: number;
				boxWidth: number;
				collisionFree: false;
		  }
		| undefined;
	let fallbackPenalty = Number.POSITIVE_INFINITY;
	for (const time of times) {
		const point = route.curve.at(time);
		const side = normal(route.curve.tangent(time));
		for (const offset of normalOffsets) {
			const renderedCenter = add(point, scale(side, offset));
			const candidate = {
				x: renderedCenter.x,
				y: renderedCenter.y - metrics.yOffset,
			};
			const box = labelBox(
				candidate,
				collisionWidth,
				metrics.height,
				metrics.yOffset,
			);
			const nodeHits = [...positions.values()].filter((node) =>
				labelHitsNode(box, node),
			).length;
			const overlapArea = placed.reduce((total, other) => {
				const overlapWidth = Math.max(
					0,
					Math.min(box.right, other.right) - Math.max(box.left, other.left) + 6,
				);
				const overlapHeight = Math.max(
					0,
					Math.min(box.bottom, other.bottom) - Math.max(box.top, other.top) + 6,
				);
				return total + overlapWidth * overlapHeight;
			}, 0);
			const penalty =
				(insideViewbox(box) ? 0 : 1_000_000) + nodeHits * 100_000 + overlapArea;
			if (penalty < fallbackPenalty) {
				fallback = {
					point: candidate,
					anchor: point,
					box,
					width,
					boxWidth: collisionWidth,
					collisionFree: false,
				};
				fallbackPenalty = penalty;
			}
			if (
				insideViewbox(box) &&
				nodeHits === 0 &&
				!placed.some((other) => boxesOverlap(box, other))
			) {
				return {
					point: candidate,
					anchor: point,
					box,
					width,
					boxWidth: collisionWidth,
					collisionFree: true,
				};
			}
		}
	}
	return (
		fallback ?? {
			point: route.curve.at(0.5),
			anchor: route.curve.at(0.5),
			box: labelBox(
				route.curve.at(0.5),
				collisionWidth,
				metrics.height,
				metrics.yOffset,
			),
			width,
			boxWidth: collisionWidth,
			collisionFree: false,
		}
	);
}

function residualLabel(
	route: PendingRoute,
	time: number,
	offset: number,
): FlowPoint {
	const point = route.curve.at(time);
	return add(point, scale(normal(route.curve.tangent(time)), offset));
}

function unplacedLabel(
	route: PendingRoute,
	positions: ReadonlyMap<string, FlowPoint>,
	metrics: Readonly<{ widthAddition: number; height: number; yOffset: number }>,
): ReturnType<typeof chooseLabel> {
	const anchor = route.curve.at(0.5);
	const side = normal(route.curve.tangent(0.5));
	const width = labelWidth(route.edge) + (route.parallelCount > 1 ? 42 : 0);
	const boxWidth = width + Math.max(0, metrics.widthAddition);
	const candidates = [38, -38, 56, -56].map((offset) => {
		const renderedCenter = add(anchor, scale(side, offset));
		const point = {
			x: renderedCenter.x,
			y: renderedCenter.y - metrics.yOffset,
		};
		const box = labelBox(point, boxWidth, metrics.height, metrics.yOffset);
		const nodeHits = [...positions.values()].filter((node) =>
			labelHitsNode(box, node),
		).length;
		return {
			point,
			anchor,
			box,
			width,
			boxWidth,
			collisionFree: false as const,
			penalty: (insideViewbox(box) ? 0 : 1_000_000) + nodeHits * 100_000,
		};
	});
	const best = candidates.reduce((left, right) =>
		right.penalty < left.penalty ? right : left,
	);
	return {
		point: best.point,
		anchor: best.anchor,
		box: best.box,
		width: best.width,
		boxWidth: best.boxWidth,
		collisionFree: false,
	};
}

/** Builds deterministic graph geometry independent of the current event state. */
export function buildFlowLayout(
	nodes: readonly FlowNodeV1[],
	edges: readonly FlowEdgeV1[],
	options: FlowLayoutOptions = {},
): FlowLayout {
	const positions = buildFlowNodePositions(nodes, {
		edges,
		...(options.model === undefined ? {} : { model: options.model }),
		...(options.placement === undefined
			? {}
			: { placement: options.placement }),
	});
	const pending = buildPendingRoutes(edges, positions);
	const labelPriority = new Map(
		(options.labelPriorityEdgeIds ?? []).map((edgeId, index) => [
			edgeId,
			index,
		]),
	);
	const orderedPending = [...pending].sort((left, right) => {
		const leftPriority = labelPriority.get(left.edge.id);
		const rightPriority = labelPriority.get(right.edge.id);
		if (leftPriority !== undefined || rightPriority !== undefined) {
			if (leftPriority === undefined) return 1;
			if (rightPriority === undefined) return -1;
			if (leftPriority !== rightPriority) return leftPriority - rightPriority;
		}
		return compareText(left.edge.id, right.edge.id);
	});
	const placed: LabelBox[] = [];
	const routes = new Map<string, FlowEdgeRoute>();
	for (const route of orderedPending) {
		const shouldPlaceLabel =
			options.labelEdgeIds === undefined ||
			options.labelEdgeIds.has(route.edge.id);
		const metrics = options.labelMetrics?.get(route.edge.id) ?? {
			widthAddition: 0,
			height: LABEL_HEIGHT,
			yOffset: 0,
		};
		const label = shouldPlaceLabel
			? chooseLabel(route, positions, placed, metrics)
			: unplacedLabel(route, positions, metrics);
		// A current source event must stay visible even when graph geometry leaves
		// no node-free slot. Reserve its least-bad fallback before placing stable
		// context labels so those labels never cover the active annotation.
		if (label.collisionFree || labelPriority.has(route.edge.id)) {
			placed.push(label.box);
		}
		const tokenTime = laneTokenTime(route);
		const tokenTangent = route.curve.tangent(tokenTime);
		routes.set(route.edge.id, {
			edgeId: route.edge.id,
			from: route.edge.from,
			to: route.edge.to,
			path: route.curve.path,
			reversePath: route.curve.reversePath,
			label: label.point,
			labelWidth: label.width,
			labelBoxWidth: label.boxWidth,
			labelHeight: metrics.height,
			labelYOffset: metrics.yOffset,
			labelCollisionFree: label.collisionFree,
			labelAnchor: label.anchor,
			laneToken: route.curve.at(tokenTime),
			laneTokenAngle:
				(Math.atan2(tokenTangent.y, tokenTangent.x) * 180) / Math.PI,
			routeMidpoint: route.curve.at(0.5),
			parallelIndex: route.parallelIndex,
			parallelCount: route.parallelCount,
			residualForwardLabel: residualLabel(route, 0.42, 11),
			residualReverseLabel: residualLabel(route, 0.58, -11),
			selfLoop: route.selfLoop,
		});
	}
	return { positions, routes };
}
