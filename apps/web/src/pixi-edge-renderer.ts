import type { Graphics } from "pixi.js";

import type { Position } from "./graph-layout";
import {
	type CanvasController,
	currentPosition,
	directedEdgeKey,
} from "./pixi-controller";
import { edgeKey } from "./trace-visualization";
import { traversalGradientSegments } from "./traversal-gradient";

function appendEdges(
	graphics: Graphics,
	pairs: readonly [string, string][],
	position: (key: string) => Position | undefined,
	progress = 1,
) {
	for (const [source, target] of pairs) {
		const from = position(source);
		const to = position(target);
		if (from !== undefined && to !== undefined) {
			graphics
				.moveTo(from.x, from.y)
				.lineTo(
					from.x + (to.x - from.x) * progress,
					from.y + (to.y - from.y) * progress,
				);
		}
	}
}

function appendTraversalGradient(
	controller: CanvasController,
	position: (key: string) => Position | undefined,
) {
	const traversal = controller.presentation.traversalEdge;
	if (traversal === undefined || controller.traversalProgress >= 1) {
		controller.host.dataset.renderedTraversalSegmentCount = "0";
		return;
	}
	const from = position(traversal.source);
	const to = position(traversal.target);
	if (from === undefined || to === undefined) {
		controller.host.dataset.renderedTraversalSegmentCount = "0";
		return;
	}
	const segments = traversalGradientSegments(controller.traversalProgress);
	controller.host.dataset.renderedTraversalSegmentCount = String(
		segments.length,
	);
	for (const segment of segments) {
		controller.edges
			.moveTo(
				from.x + (to.x - from.x) * segment.start,
				from.y + (to.y - from.y) * segment.start,
			)
			.lineTo(
				from.x + (to.x - from.x) * segment.end,
				from.y + (to.y - from.y) * segment.end,
			)
			.stroke({
				alpha: segment.alpha,
				color: segment.color,
				pixelLine: true,
				width: controller.mode === "summary" ? 2 : 4,
			});
	}
}

export function drawEdges(controller: CanvasController) {
	const position = (key: string) => currentPosition(controller, key);
	const transitionPosition = (key: string) =>
		currentPosition(controller, key) ??
		controller.edgeTransitionPositions.get(key);
	const currentEdges = new Set(controller.presentation.currentEdges);
	const traversalKey = controller.presentation.traversalEdge?.key;
	const visitedEdges = new Set(controller.presentation.visitedEdges);
	const base: [string, string][] = [];
	const visited: [string, string][] = [];
	const current: [string, string][] = [];
	const enteringBase: [string, string][] = [];
	const enteringVisited: [string, string][] = [];
	const enteringCurrent: [string, string][] = [];
	const exitingBase: [string, string][] = [];
	const exitingCurrent: [string, string][] = [];
	for (const pair of controller.edgePairs) {
		const semanticKey = edgeKey(pair[0], pair[1]);
		const entering = controller.enteringEdgeKeys.has(directedEdgeKey(pair));
		const target =
			currentEdges.has(semanticKey) && semanticKey !== traversalKey
				? entering
					? enteringCurrent
					: current
				: visitedEdges.has(semanticKey)
					? entering
						? enteringVisited
						: visited
					: entering
						? enteringBase
						: base;
		target.push(pair);
	}
	for (const pair of controller.exitingEdgePairs) {
		const target = currentEdges.has(edgeKey(pair[0], pair[1]))
			? exitingCurrent
			: exitingBase;
		target.push(pair);
	}
	const easedProgress = 1 - (1 - controller.edgeProgress) ** 3;
	controller.edges.clear();
	appendEdges(controller.edges, exitingBase, transitionPosition);
	controller.edges.stroke({
		alpha: 0.75 * (1 - easedProgress),
		color: 0x7c878c,
		pixelLine: true,
		width: controller.mode === "summary" ? 1 : 1.5,
	});
	appendEdges(controller.edges, exitingCurrent, transitionPosition);
	controller.edges.stroke({
		alpha: 1 - easedProgress,
		color: 0xffa36b,
		pixelLine: true,
		width: controller.mode === "summary" ? 2 : 4,
	});
	appendEdges(controller.edges, base, position);
	appendEdges(controller.edges, enteringBase, position, easedProgress);
	controller.edges.stroke({
		alpha: 0.8,
		color: 0x465056,
		pixelLine: true,
		width: controller.mode === "summary" ? 1 : 1.5,
	});
	appendEdges(controller.edges, visited, position);
	appendEdges(controller.edges, enteringVisited, position, easedProgress);
	controller.edges.stroke({
		alpha: 0.95,
		color: 0x53c7c5,
		pixelLine: true,
		width: controller.mode === "summary" ? 1.5 : 3,
	});
	appendEdges(controller.edges, current, position);
	appendEdges(controller.edges, enteringCurrent, position, easedProgress);
	controller.edges.stroke({
		alpha: 1,
		color: 0xffa36b,
		pixelLine: true,
		width: controller.mode === "summary" ? 2 : 4,
	});
	appendTraversalGradient(controller, position);
}
