import {
	derivePlanarTopology,
	type FlowCurrentSceneV9,
	type FlowPlanarDartV1,
	planarDartKey,
} from "./flow-scene";

export type FlowPlanarDualFace = Readonly<{
	id: string;
	index: number;
	x: number;
	y: number;
	role: "source-side" | "sink-side" | "interior";
	active: boolean;
	distance?: string;
}>;

export type FlowPlanarDualEdge = Readonly<{
	edgeId: string;
	fromFace: number;
	toFace: number;
	forwardLength: string;
	reverseLength: "0";
	activeDirection?: "forward" | "reverse";
	labelX: number;
	labelY: number;
	labelAnchorX: number;
	labelAnchorY: number;
}>;

export type FlowPlanarDualOverlay = Readonly<{
	kind: "hassin-split" | "borradaile-klein-unsplit";
	faces: readonly FlowPlanarDualFace[];
	edges: readonly FlowPlanarDualEdge[];
}>;

type Position = Readonly<{ x: number; y: number }>;

type UnplacedDualEdge = Omit<
	FlowPlanarDualEdge,
	"labelX" | "labelY" | "labelAnchorX" | "labelAnchorY"
>;

type LabelBox = Readonly<{
	left: number;
	top: number;
	right: number;
	bottom: number;
}>;

export type FlowPlanarLabelObstacle = LabelBox &
	Readonly<{
		/** Larger weights protect semantic objects such as vertices before labels. */
		weight: number;
	}>;

function boxOverlapArea(left: LabelBox, right: LabelBox): number {
	const width = Math.max(
		0,
		Math.min(left.right, right.right) - Math.max(left.left, right.left),
	);
	const height = Math.max(
		0,
		Math.min(left.bottom, right.bottom) - Math.max(left.top, right.top),
	);
	return width * height;
}

function dualLabelBox(x: number, y: number, text: string): LabelBox {
	const width = Math.max(40, text.length * 5.2 + 8);
	const height = 14;
	return {
		left: x - width / 2,
		right: x + width / 2,
		top: y - height + 2,
		bottom: y + 2,
	};
}

/** Places each dual annotation near its own arc without overlapping peers. */
function placeDualEdgeLabels(
	faces: readonly FlowPlanarDualFace[],
	edges: readonly UnplacedDualEdge[],
	obstacles: readonly FlowPlanarLabelObstacle[],
): FlowPlanarDualEdge[] {
	const placed: LabelBox[] = [];
	return edges.map((edge, edgeIndex) => {
		const from = faces[edge.fromFace];
		const to = faces[edge.toFace];
		if (from === undefined || to === undefined) {
			throw new Error("Planar dual edge references an absent face");
		}
		const anchorX = (from.x + to.x) / 2;
		const anchorY = (from.y + to.y) / 2;
		const dx = to.x - from.x;
		const dy = to.y - from.y;
		const length = Math.max(1, Math.hypot(dx, dy));
		const tangentX = dx / length;
		const tangentY = dy / length;
		const normalX = -tangentY;
		const normalY = tangentX;
		const text = `${edge.forwardLength} → / 0 ←`;
		const candidates: Array<
			Readonly<{ x: number; y: number; distance: number }>
		> = [];
		for (const normalDistance of [
			15, -15, 27, -27, 39, -39, 51, -51, 63, -63, 75, -75, 87, -87, 99, -99,
			111, -111,
		]) {
			for (const tangentDistance of [
				0, 24, -24, 48, -48, 72, -72, 96, -96, 120, -120,
			]) {
				candidates.push({
					x: anchorX + normalX * normalDistance + tangentX * tangentDistance,
					y: anchorY + normalY * normalDistance + tangentY * tangentDistance,
					distance: Math.abs(normalDistance) + Math.abs(tangentDistance),
				});
			}
		}
		let best = candidates[0] as (typeof candidates)[number];
		let bestBox = dualLabelBox(best.x, best.y, text);
		let bestScore = Number.POSITIVE_INFINITY;
		for (const candidate of candidates) {
			const box = dualLabelBox(candidate.x, candidate.y, text);
			const labelOverlap = placed.reduce(
				(sum, existing) => sum + boxOverlapArea(box, existing),
				0,
			);
			const faceOverlap = faces.reduce((sum, face) => {
				const faceBox = {
					left: face.x - 20,
					right: face.x + 20,
					top: face.y - 20,
					bottom: face.y + 20,
				};
				return sum + boxOverlapArea(box, faceBox);
			}, 0);
			const obstaclePenalty = obstacles.reduce(
				(sum, obstacle) =>
					sum + boxOverlapArea(box, obstacle) * obstacle.weight,
				0,
			);
			const score =
				labelOverlap * 10_000 +
				obstaclePenalty +
				faceOverlap * 1_000_000 +
				candidate.distance +
				edgeIndex * 0.0001;
			if (score < bestScore) {
				best = candidate;
				bestBox = box;
				bestScore = score;
			}
			if (labelOverlap === 0 && faceOverlap === 0 && obstaclePenalty === 0) {
				break;
			}
		}
		placed.push(bestBox);
		return {
			...edge,
			labelX: best.x,
			labelY: best.y,
			labelAnchorX: anchorX,
			labelAnchorY: anchorY,
		};
	});
}

export type FlowPlanarDualInput = Readonly<
	Pick<
		FlowCurrentSceneV9,
		| "algorithm"
		| "graph"
		| "model"
		| "residual_arcs"
		| "solve_status"
		| "trace_event"
	>
>;

function reverseDart(dart: FlowPlanarDartV1): FlowPlanarDartV1 {
	return {
		edge_id: dart.edge_id,
		direction: dart.direction === "forward" ? "reverse" : "forward",
	};
}

function endpoints(
	dart: FlowPlanarDartV1,
	edgeById: ReadonlyMap<string, FlowCurrentSceneV9["graph"]["edges"][number]>,
): readonly [string, string] | undefined {
	const edge = edgeById.get(dart.edge_id);
	if (edge === undefined) return undefined;
	return dart.direction === "forward"
		? [edge.from, edge.to]
		: [edge.to, edge.from];
}

function faceCenter(
	darts: readonly FlowPlanarDartV1[],
	edgeById: ReadonlyMap<string, FlowCurrentSceneV9["graph"]["edges"][number]>,
	positions: ReadonlyMap<string, Position>,
): Position | undefined {
	const nodeIds = new Set<string>();
	for (const dart of darts) {
		const pair = endpoints(dart, edgeById);
		if (pair === undefined) return undefined;
		nodeIds.add(pair[0]);
		nodeIds.add(pair[1]);
	}
	if (nodeIds.size === 0) return undefined;
	let x = 0;
	let y = 0;
	for (const nodeId of nodeIds) {
		const position = positions.get(nodeId);
		if (position === undefined) return undefined;
		x += position.x;
		y += position.y;
	}
	return { x: x / nodeIds.size, y: y / nodeIds.size };
}

function separateSplitFaceCenters(
	centers: Position[],
	source: number,
	sink: number,
): void {
	const sourceCenter = centers[source];
	const sinkCenter = centers[sink];
	if (sourceCenter === undefined || sinkCenter === undefined) return;
	const dx = sinkCenter.x - sourceCenter.x;
	const dy = sinkCenter.y - sourceCenter.y;
	if (Math.hypot(dx, dy) >= 42) return;
	const length = Math.max(1, Math.hypot(dx, dy));
	const offsetX = (-dy / length) * 24;
	const offsetY = (dx / length) * 24;
	centers[source] = {
		x: sourceCenter.x + offsetX,
		y: sourceCenter.y + offsetY,
	};
	centers[sink] = {
		x: sinkCenter.x - offsetX,
		y: sinkCenter.y - offsetY,
	};
}

function separateCoincidentFaceCenters(centers: Position[]): void {
	const minimumDistance = 66;
	for (let pass = 0; pass < 12; pass += 1) {
		let moved = false;
		for (let first = 0; first < centers.length; first += 1) {
			for (let second = first + 1; second < centers.length; second += 1) {
				const left = centers[first];
				const right = centers[second];
				if (left === undefined || right === undefined) continue;
				let dx = right.x - left.x;
				let dy = right.y - left.y;
				let distance = Math.hypot(dx, dy);
				if (distance >= minimumDistance) continue;
				if (distance < 0.001) {
					const angle =
						-Math.PI / 2 +
						(2 * Math.PI * (first + second + 1)) / centers.length;
					dx = Math.cos(angle);
					dy = Math.sin(angle);
					distance = 1;
				}
				const shift = (minimumDistance - distance) / 2;
				const shiftX = (dx / distance) * shift;
				const shiftY = (dy / distance) * shift;
				centers[first] = { x: left.x - shiftX, y: left.y - shiftY };
				centers[second] = { x: right.x + shiftX, y: right.y + shiftY };
				moved = true;
			}
		}
		if (!moved) break;
	}
}

/** Builds the exact split-dual drawing used by Hassin's implementation. */
export function buildHassinPlanarDualOverlay(
	scene: FlowPlanarDualInput,
	positions: ReadonlyMap<string, Position>,
	labelObstacles: readonly FlowPlanarLabelObstacle[] = [],
): FlowPlanarDualOverlay | undefined {
	if (
		scene.algorithm.id !== "hassin-st-planar" ||
		scene.model.kind !== "planar-max-flow" ||
		(scene.solve_status === "ready" && scene.trace_event === undefined) ||
		scene.model.embedding.terminal_corners === undefined
	) {
		return undefined;
	}
	const topology = derivePlanarTopology(
		scene.model,
		scene.graph.nodes,
		scene.graph.edges,
	);
	if (topology === undefined) return undefined;
	const corners = scene.model.embedding.terminal_corners;
	const outerBoundary = topology.faces[topology.outerFace];
	if (outerBoundary === undefined) return undefined;
	const sourcePosition = outerBoundary.findIndex(
		(dart) => planarDartKey(dart) === planarDartKey(corners.source),
	);
	const sinkPosition = outerBoundary.findIndex(
		(dart) => planarDartKey(dart) === planarDartKey(corners.sink),
	);
	if (
		sourcePosition < 0 ||
		sinkPosition < 0 ||
		sourcePosition === sinkPosition
	) {
		return undefined;
	}

	const firstOuterSegment = new Set<string>();
	for (
		let cursor = sourcePosition;
		cursor !== sinkPosition;
		cursor = (cursor + 1) % outerBoundary.length
	) {
		const dart = outerBoundary[cursor];
		if (dart === undefined) return undefined;
		firstOuterSegment.add(planarDartKey(dart));
	}
	const sinkFace = topology.faces.length;
	const leftFaceByDart = new Map(topology.leftFaceByDart);
	for (const dart of outerBoundary) {
		if (!firstOuterSegment.has(planarDartKey(dart))) {
			leftFaceByDart.set(planarDartKey(dart), sinkFace);
		}
	}

	const splitFaces = Array.from(
		{ length: topology.faces.length + 1 },
		() => [] as FlowPlanarDartV1[],
	);
	for (const face of topology.faces) {
		for (const dart of face) {
			const faceIndex = leftFaceByDart.get(planarDartKey(dart));
			if (faceIndex === undefined) return undefined;
			splitFaces[faceIndex]?.push(dart);
		}
	}
	const edgeById = new Map(scene.graph.edges.map((edge) => [edge.id, edge]));
	const centers = splitFaces.map((face) =>
		faceCenter(face, edgeById, positions),
	);
	if (centers.some((center) => center === undefined)) return undefined;
	separateSplitFaceCenters(centers as Position[], topology.outerFace, sinkFace);
	// Parallel primal edges can give every dual face the same geometric
	// centroid.  Separating only the two split outer faces leaves the remaining
	// faces (and all dual arcs) collapsed into one unreadable mark.
	separateCoincidentFaceCenters(centers as Position[]);

	const activeArc = scene.residual_arcs.find((arc) => arc.active);
	let activeFace: number | undefined;
	if (scene.trace_event?.catalog_id === "hassin-st-planar.settle-dual-face") {
		if (activeArc === undefined) {
			activeFace = topology.outerFace;
		} else {
			const forwardLeft = leftFaceByDart.get(
				planarDartKey({ edge_id: activeArc.edge_id, direction: "forward" }),
			);
			const reverseLeft = leftFaceByDart.get(
				planarDartKey({ edge_id: activeArc.edge_id, direction: "reverse" }),
			);
			activeFace =
				activeArc.direction === "forward" ? reverseLeft : forwardLeft;
		}
	}
	const activeDistance =
		scene.trace_event?.catalog_id === "hassin-st-planar.settle-dual-face" &&
		scene.trace_event.detail?.label === "dual-distance"
			? scene.trace_event.detail.value
			: undefined;

	const faces = centers.map((center, index) => {
		const position = center as Position;
		return {
			id:
				index === topology.outerFace
					? "fₛ"
					: index === sinkFace
						? "fₜ"
						: `f${index}`,
			index,
			x: position.x,
			y: position.y,
			role:
				index === topology.outerFace
					? ("source-side" as const)
					: index === sinkFace
						? ("sink-side" as const)
						: ("interior" as const),
			active: index === activeFace,
			...(index === activeFace && activeDistance !== undefined
				? { distance: activeDistance }
				: {}),
		};
	});
	const unplacedEdges: UnplacedDualEdge[] = scene.graph.edges.flatMap(
		(edge) => {
			const forward = { edge_id: edge.id, direction: "forward" as const };
			const fromFace = leftFaceByDart.get(planarDartKey(forward));
			const toFace = leftFaceByDart.get(planarDartKey(reverseDart(forward)));
			if (
				fromFace === undefined ||
				toFace === undefined ||
				fromFace === toFace
			) {
				return [];
			}
			const activeDirection =
				activeArc?.edge_id === edge.id ? activeArc.direction : undefined;
			return [
				{
					edgeId: edge.id,
					fromFace,
					toFace,
					forwardLength: edge.capacity,
					reverseLength: "0" as const,
					...(activeDirection === undefined ? {} : { activeDirection }),
				},
			];
		},
	);
	const edges = placeDualEdgeLabels(faces, unplacedEdges, labelObstacles);
	return { kind: "hassin-split", faces, edges };
}

/** Builds the unsplit dual used by clockwise-cycle preprocessing. */
export function buildBorradaileKleinPlanarDualOverlay(
	scene: FlowPlanarDualInput,
	positions: ReadonlyMap<string, Position>,
	labelObstacles: readonly FlowPlanarLabelObstacle[] = [],
): FlowPlanarDualOverlay | undefined {
	if (
		scene.algorithm.id !== "borradaile-klein-planar" ||
		scene.model.kind !== "planar-max-flow"
	) {
		return undefined;
	}
	const topology = derivePlanarTopology(
		scene.model,
		scene.graph.nodes,
		scene.graph.edges,
	);
	if (topology === undefined) return undefined;
	const edgeById = new Map(scene.graph.edges.map((edge) => [edge.id, edge]));
	const centers = topology.faces.map((face) =>
		faceCenter(face, edgeById, positions),
	);
	if (centers.some((center) => center === undefined)) return undefined;
	separateCoincidentFaceCenters(centers as Position[]);
	const faces = centers.map((center, index) => {
		const position = center as Position;
		return {
			id: index === topology.outerFace ? "f∞" : `f${index}`,
			index,
			x: position.x,
			y: position.y,
			role:
				index === topology.outerFace
					? ("source-side" as const)
					: ("interior" as const),
			active: false,
		};
	});
	const unplacedEdges: UnplacedDualEdge[] = scene.graph.edges.flatMap(
		(edge) => {
			const forward = { edge_id: edge.id, direction: "forward" as const };
			const fromFace = topology.leftFaceByDart.get(planarDartKey(forward));
			const toFace = topology.leftFaceByDart.get(
				planarDartKey(reverseDart(forward)),
			);
			if (
				fromFace === undefined ||
				toFace === undefined ||
				fromFace === toFace
			) {
				return [];
			}
			return [
				{
					edgeId: edge.id,
					fromFace,
					toFace,
					forwardLength: edge.capacity,
					reverseLength: "0" as const,
				},
			];
		},
	);
	const edges = placeDualEdgeLabels(faces, unplacedEdges, labelObstacles);
	return { kind: "borradaile-klein-unsplit", faces, edges };
}

/** Selects the algorithm-specific planar dual without inferring an embedding. */
export function buildPlanarDualOverlay(
	scene: FlowPlanarDualInput,
	positions: ReadonlyMap<string, Position>,
	labelObstacles: readonly FlowPlanarLabelObstacle[] = [],
): FlowPlanarDualOverlay | undefined {
	return (
		buildHassinPlanarDualOverlay(scene, positions, labelObstacles) ??
		buildBorradaileKleinPlanarDualOverlay(scene, positions, labelObstacles)
	);
}
