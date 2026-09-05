import { FLOW_VIEWBOX_HEIGHT, FLOW_VIEWBOX_WIDTH } from "./flow-layout";

export type FlowCanvasPoint = Readonly<{ x: number; y: number }>;

export type FlowCanvasSize = Readonly<{ width: number; height: number }>;

export type FlowCanvasViewBox = Readonly<{
	x: number;
	y: number;
	width: number;
	height: number;
}>;

export type FlowCanvasViewportBounds = FlowCanvasViewBox;

export type FlowCanvasViewportState = Readonly<{
	viewBox: FlowCanvasViewBox;
	zoom: number;
}>;

export type FlowCanvasViewportPolicy = Readonly<{
	minimumZoom: number;
	maximumZoom: number;
	buttonZoomFactor: number;
	wheelSensitivity: number;
}>;

export const FLOW_CANVAS_VIEWPORT_BOUNDS: FlowCanvasViewportBounds =
	Object.freeze({
		x: 0,
		y: 0,
		width: FLOW_VIEWBOX_WIDTH,
		height: FLOW_VIEWBOX_HEIGHT,
	});

export const DEFAULT_FLOW_CANVAS_VIEWPORT_POLICY: FlowCanvasViewportPolicy =
	Object.freeze({
		minimumZoom: 1,
		maximumZoom: 8,
		buttonZoomFactor: 1.25,
		wheelSensitivity: 0.002,
	});

const MINIMUM_PINCH_DISTANCE = 1;
const MAXIMUM_WHEEL_DELTA = 240;
const DEFAULT_WHEEL_LINE_HEIGHT = 16;

function finite(value: number): boolean {
	return Number.isFinite(value);
}

function validBounds(bounds: FlowCanvasViewportBounds): boolean {
	return (
		finite(bounds.x) &&
		finite(bounds.y) &&
		finite(bounds.width) &&
		finite(bounds.height) &&
		bounds.width > 0 &&
		bounds.height > 0
	);
}

function validPolicy(policy: FlowCanvasViewportPolicy): boolean {
	return (
		finite(policy.minimumZoom) &&
		finite(policy.maximumZoom) &&
		finite(policy.buttonZoomFactor) &&
		finite(policy.wheelSensitivity) &&
		policy.minimumZoom >= 1 &&
		policy.maximumZoom >= policy.minimumZoom &&
		policy.buttonZoomFactor > 1 &&
		policy.wheelSensitivity > 0
	);
}

function validSize(size: FlowCanvasSize): boolean {
	return (
		finite(size.width) &&
		finite(size.height) &&
		size.width > 0 &&
		size.height > 0
	);
}

function validPoint(point: FlowCanvasPoint): boolean {
	return finite(point.x) && finite(point.y);
}

function renderedCanvasRect(
	size: FlowCanvasSize,
	bounds: FlowCanvasViewportBounds,
): FlowCanvasViewBox | undefined {
	if (!validSize(size) || !validBounds(bounds)) return undefined;
	const scale = Math.min(
		size.width / bounds.width,
		size.height / bounds.height,
	);
	const width = bounds.width * scale;
	const height = bounds.height * scale;
	return {
		x: (size.width - width) / 2,
		y: (size.height - height) / 2,
		width,
		height,
	};
}

function clamp(value: number, minimum: number, maximum: number): number {
	return Math.min(maximum, Math.max(minimum, value));
}

function sameViewBox(
	left: FlowCanvasViewportState,
	right: FlowCanvasViewportState,
): boolean {
	return (
		left.zoom === right.zoom &&
		left.viewBox.x === right.viewBox.x &&
		left.viewBox.y === right.viewBox.y &&
		left.viewBox.width === right.viewBox.width &&
		left.viewBox.height === right.viewBox.height
	);
}

/** Converts WheelEvent delta modes to deterministic CSS-pixel units. */
export function flowCanvasWheelDeltaPixels(
	deltaY: number,
	deltaMode: number,
	pageHeight: number,
): number {
	if (!finite(deltaY) || !finite(pageHeight) || pageHeight <= 0) {
		return Number.NaN;
	}
	if (deltaMode === 0) return deltaY;
	if (deltaMode === 1) return deltaY * DEFAULT_WHEEL_LINE_HEIGHT;
	if (deltaMode === 2) return deltaY * pageHeight;
	return Number.NaN;
}

/** Returns the complete graph extent. This is the target for the Fit action. */
export function fitFlowCanvasViewport(
	bounds: FlowCanvasViewportBounds = FLOW_CANVAS_VIEWPORT_BOUNDS,
): FlowCanvasViewportState {
	if (!validBounds(bounds)) {
		return {
			viewBox: FLOW_CANVAS_VIEWPORT_BOUNDS,
			zoom: 1,
		};
	}
	return {
		viewBox: { ...bounds },
		zoom: 1,
	};
}

/**
 * Restores the viewport invariant after persisted or event-derived state.
 * The returned viewBox always stays inside bounds and preserves its aspect ratio.
 */
export function constrainFlowCanvasViewport(
	state: FlowCanvasViewportState,
	bounds: FlowCanvasViewportBounds = FLOW_CANVAS_VIEWPORT_BOUNDS,
	policy: FlowCanvasViewportPolicy = DEFAULT_FLOW_CANVAS_VIEWPORT_POLICY,
): FlowCanvasViewportState {
	if (!validBounds(bounds) || !validPolicy(policy)) {
		return fitFlowCanvasViewport();
	}
	const requestedZoom = finite(state.zoom)
		? state.zoom
		: bounds.width / state.viewBox.width;
	if (!finite(requestedZoom) || requestedZoom <= 0) {
		return fitFlowCanvasViewport(bounds);
	}
	const zoom = clamp(requestedZoom, policy.minimumZoom, policy.maximumZoom);
	const width = bounds.width / zoom;
	const height = bounds.height / zoom;
	const requestedX = finite(state.viewBox.x) ? state.viewBox.x : bounds.x;
	const requestedY = finite(state.viewBox.y) ? state.viewBox.y : bounds.y;
	return {
		viewBox: {
			x: clamp(requestedX, bounds.x, bounds.x + bounds.width - width),
			y: clamp(requestedY, bounds.y, bounds.y + bounds.height - height),
			width,
			height,
		},
		zoom,
	};
}

function normalizedScreenPoint(
	point: FlowCanvasPoint,
	size: FlowCanvasSize,
	bounds: FlowCanvasViewportBounds,
): FlowCanvasPoint | undefined {
	if (!validPoint(point)) return undefined;
	const rendered = renderedCanvasRect(size, bounds);
	if (rendered === undefined) return undefined;
	return {
		x: clamp((point.x - rendered.x) / rendered.width, 0, 1),
		y: clamp((point.y - rendered.y) / rendered.height, 0, 1),
	};
}

function transformAroundScreenPoints(
	state: FlowCanvasViewportState,
	previousPoint: FlowCanvasPoint,
	currentPoint: FlowCanvasPoint,
	screenSize: FlowCanvasSize,
	zoomFactor: number,
	bounds: FlowCanvasViewportBounds,
	policy: FlowCanvasViewportPolicy,
): FlowCanvasViewportState {
	if (!finite(zoomFactor) || zoomFactor <= 0) return state;
	const previous = normalizedScreenPoint(previousPoint, screenSize, bounds);
	const current = normalizedScreenPoint(currentPoint, screenSize, bounds);
	if (previous === undefined || current === undefined) return state;
	const normalized = constrainFlowCanvasViewport(state, bounds, policy);
	const worldAnchor = {
		x: normalized.viewBox.x + previous.x * normalized.viewBox.width,
		y: normalized.viewBox.y + previous.y * normalized.viewBox.height,
	};
	const zoom = clamp(
		normalized.zoom * zoomFactor,
		policy.minimumZoom,
		policy.maximumZoom,
	);
	const width = bounds.width / zoom;
	const height = bounds.height / zoom;
	const transformed = constrainFlowCanvasViewport(
		{
			viewBox: {
				x: worldAnchor.x - current.x * width,
				y: worldAnchor.y - current.y * height,
				width,
				height,
			},
			zoom,
		},
		bounds,
		policy,
	);
	return sameViewBox(state, transformed) ? state : transformed;
}

/** Zooms around a point expressed in local canvas pixels. */
export function zoomFlowCanvasViewport(
	state: FlowCanvasViewportState,
	zoomFactor: number,
	anchor: FlowCanvasPoint,
	screenSize: FlowCanvasSize,
	bounds: FlowCanvasViewportBounds = FLOW_CANVAS_VIEWPORT_BOUNDS,
	policy: FlowCanvasViewportPolicy = DEFAULT_FLOW_CANVAS_VIEWPORT_POLICY,
): FlowCanvasViewportState {
	if (!validBounds(bounds) || !validPolicy(policy)) return state;
	return transformAroundScreenPoints(
		state,
		anchor,
		anchor,
		screenSize,
		zoomFactor,
		bounds,
		policy,
	);
}

export function zoomInFlowCanvasViewport(
	state: FlowCanvasViewportState,
	screenSize: FlowCanvasSize,
	bounds: FlowCanvasViewportBounds = FLOW_CANVAS_VIEWPORT_BOUNDS,
	policy: FlowCanvasViewportPolicy = DEFAULT_FLOW_CANVAS_VIEWPORT_POLICY,
): FlowCanvasViewportState {
	return zoomFlowCanvasViewport(
		state,
		policy.buttonZoomFactor,
		{ x: screenSize.width / 2, y: screenSize.height / 2 },
		screenSize,
		bounds,
		policy,
	);
}

export function zoomOutFlowCanvasViewport(
	state: FlowCanvasViewportState,
	screenSize: FlowCanvasSize,
	bounds: FlowCanvasViewportBounds = FLOW_CANVAS_VIEWPORT_BOUNDS,
	policy: FlowCanvasViewportPolicy = DEFAULT_FLOW_CANVAS_VIEWPORT_POLICY,
): FlowCanvasViewportState {
	return zoomFlowCanvasViewport(
		state,
		1 / policy.buttonZoomFactor,
		{ x: screenSize.width / 2, y: screenSize.height / 2 },
		screenSize,
		bounds,
		policy,
	);
}

/** Converts a wheel delta into a bounded exponential zoom step. */
export function wheelZoomFlowCanvasViewport(
	state: FlowCanvasViewportState,
	deltaY: number,
	anchor: FlowCanvasPoint,
	screenSize: FlowCanvasSize,
	bounds: FlowCanvasViewportBounds = FLOW_CANVAS_VIEWPORT_BOUNDS,
	policy: FlowCanvasViewportPolicy = DEFAULT_FLOW_CANVAS_VIEWPORT_POLICY,
): FlowCanvasViewportState {
	if (!finite(deltaY) || deltaY === 0) return state;
	const boundedDelta = clamp(deltaY, -MAXIMUM_WHEEL_DELTA, MAXIMUM_WHEEL_DELTA);
	return zoomFlowCanvasViewport(
		state,
		Math.exp(-boundedDelta * policy.wheelSensitivity),
		anchor,
		screenSize,
		bounds,
		policy,
	);
}

/** Pans by a pointer movement measured in local canvas pixels. */
export function panFlowCanvasViewport(
	state: FlowCanvasViewportState,
	screenDelta: FlowCanvasPoint,
	screenSize: FlowCanvasSize,
	bounds: FlowCanvasViewportBounds = FLOW_CANVAS_VIEWPORT_BOUNDS,
	policy: FlowCanvasViewportPolicy = DEFAULT_FLOW_CANVAS_VIEWPORT_POLICY,
): FlowCanvasViewportState {
	if (
		!validPoint(screenDelta) ||
		!validSize(screenSize) ||
		!validBounds(bounds) ||
		!validPolicy(policy)
	) {
		return state;
	}
	const rendered = renderedCanvasRect(screenSize, bounds);
	if (rendered === undefined) return state;
	const normalized = constrainFlowCanvasViewport(state, bounds, policy);
	const transformed = constrainFlowCanvasViewport(
		{
			viewBox: {
				...normalized.viewBox,
				x:
					normalized.viewBox.x -
					(screenDelta.x / rendered.width) * normalized.viewBox.width,
				y:
					normalized.viewBox.y -
					(screenDelta.y / rendered.height) * normalized.viewBox.height,
			},
			zoom: normalized.zoom,
		},
		bounds,
		policy,
	);
	return sameViewBox(state, transformed) ? state : transformed;
}

function midpoint(
	first: FlowCanvasPoint,
	second: FlowCanvasPoint,
): FlowCanvasPoint {
	return { x: (first.x + second.x) / 2, y: (first.y + second.y) / 2 };
}

function distance(first: FlowCanvasPoint, second: FlowCanvasPoint): number {
	return Math.hypot(second.x - first.x, second.y - first.y);
}

/** Applies translation and scale from two captured pointer pairs in one step. */
export function pinchFlowCanvasViewport(
	state: FlowCanvasViewportState,
	previousPointers: readonly [FlowCanvasPoint, FlowCanvasPoint],
	currentPointers: readonly [FlowCanvasPoint, FlowCanvasPoint],
	screenSize: FlowCanvasSize,
	bounds: FlowCanvasViewportBounds = FLOW_CANVAS_VIEWPORT_BOUNDS,
	policy: FlowCanvasViewportPolicy = DEFAULT_FLOW_CANVAS_VIEWPORT_POLICY,
): FlowCanvasViewportState {
	if (
		!previousPointers.every(validPoint) ||
		!currentPointers.every(validPoint) ||
		!validSize(screenSize) ||
		!validBounds(bounds) ||
		!validPolicy(policy)
	) {
		return state;
	}
	const previousDistance = distance(previousPointers[0], previousPointers[1]);
	const currentDistance = distance(currentPointers[0], currentPointers[1]);
	if (
		previousDistance < MINIMUM_PINCH_DISTANCE ||
		currentDistance < MINIMUM_PINCH_DISTANCE
	) {
		return state;
	}
	return transformAroundScreenPoints(
		state,
		midpoint(previousPointers[0], previousPointers[1]),
		midpoint(currentPointers[0], currentPointers[1]),
		screenSize,
		currentDistance / previousDistance,
		bounds,
		policy,
	);
}
