import type {
	Application,
	Container,
	Graphics,
	Particle,
	ParticleContainer,
	Text,
	Texture,
} from "pixi.js";

import type { StructureSnapshot } from "./engine-types";
import type { CanvasMode, GraphLayout, Position } from "./graph-layout";
import type { TracePresentation } from "./trace-visualization";

export type NodeEmphasis = "normal" | "visited" | "compare" | "mutation";

export type NodeView = {
	auxiliary: boolean;
	container: Container;
	exiting: boolean;
	emphasis: NodeEmphasis;
	label: Text | undefined;
	multiEntry: boolean;
	selected: boolean;
	shape: Graphics;
	target: Position;
	width: number;
};

export type CanvasController = {
	activeLabel: string | undefined;
	app: Application;
	activeKey: string | undefined;
	cameraInitialized: boolean;
	edgePairs: [string, string][];
	edgeProgress: number;
	edgeTransitionPositions: Map<string, Position>;
	enteringEdgeKeys: Set<string>;
	edges: Graphics;
	edgesDirty: boolean;
	exitingEdgePairs: [string, string][];
	denseCurrentPositions: Map<string, Position>;
	denseEmphasizedKeys: Set<string>;
	denseMode: boolean;
	denseParticleContainer: ParticleContainer<Particle>;
	denseParticles: Map<string, Particle>;
	denseTargetPositions: Map<string, Position>;
	denseTexture: Texture;
	follow: boolean;
	host: HTMLDivElement;
	layout: GraphLayout | undefined;
	layoutHeight: number;
	layoutImportantSignature: string;
	layoutStructure: StructureSnapshot | undefined;
	layoutWidth: number;
	mode: CanvasMode;
	motionDurationMs: number;
	nodeViews: Map<string, NodeView>;
	onContextState: (state: "lost" | "restored") => void;
	onSelect: (key: string) => void;
	onTrackExecutionChange: (tracking: boolean) => void;
	summaryNodes: Graphics;
	summaryDirty: boolean;
	summaryPositions: Map<string, Position>;
	reducedMotion: boolean;
	presentation: TracePresentation;
	rootKey: string | undefined;
	selectedKey: string | undefined;
	trackExecution: boolean;
	traversalProgress: number;
	world: Container;
};

const MIN_ZOOM = 0.001;
const MAX_ZOOM = 4;
const MAX_FIT_ZOOM = 1.75;
export const BATCHED_DETAIL_THRESHOLD = 500;
export const BATCHED_DETAIL_EXIT_THRESHOLD = 450;

export function directedEdgeKey([source, target]: readonly [
	string,
	string,
]): string {
	return `${source}>${target}`;
}

export function sameEdges(
	left: readonly [string, string][],
	right: readonly [string, string][],
): boolean {
	if (left.length !== right.length) return false;
	const rightKeys = new Set(right.map(directedEdgeKey));
	return left.every((pair) => rightKeys.has(directedEdgeKey(pair)));
}

export function displayResolution(): number {
	return Math.min(Math.max(window.devicePixelRatio, 2), 3);
}

export function updateDenseParticleScreenPositions(
	controller: CanvasController,
) {
	if (!controller.denseMode) {
		return;
	}
	const scaleX = controller.world.scale.x;
	const scaleY = controller.world.scale.y;
	for (const [key, position] of controller.denseCurrentPositions) {
		const particle = controller.denseParticles.get(key);
		if (particle !== undefined) {
			particle.x = controller.world.x + position.x * scaleX;
			particle.y = controller.world.y + position.y * scaleY;
		}
	}
	controller.denseParticleContainer.update();
}

function publishLayoutSpan(controller: CanvasController) {
	const positions = controller.denseMode
		? controller.denseTargetPositions
		: controller.mode === "summary"
			? controller.summaryPositions
			: undefined;
	if (positions === undefined || positions.size === 0) {
		controller.host.dataset.layoutSpanX = "0.000";
		controller.host.dataset.layoutSpanY = "0.000";
		return;
	}
	const xs = [...positions.values()].map(
		(position) => position.x * controller.world.scale.x,
	);
	const ys = [...positions.values()].map(
		(position) => position.y * controller.world.scale.y,
	);
	controller.host.dataset.layoutSpanX = (
		Math.max(...xs) - Math.min(...xs)
	).toFixed(3);
	controller.host.dataset.layoutSpanY = (
		Math.max(...ys) - Math.min(...ys)
	).toFixed(3);
}

export function currentPosition(
	controller: CanvasController,
	key: string,
): Position | undefined {
	if (controller.mode === "summary") {
		return controller.summaryPositions.get(key);
	}
	if (controller.denseMode) {
		return controller.denseCurrentPositions.get(key);
	}
	const container = controller.nodeViews.get(key)?.container;
	return container === undefined
		? undefined
		: { x: container.x, y: container.y };
}

export function publishCamera(controller: CanvasController) {
	controller.host.dataset.cameraX = controller.world.x.toFixed(3);
	controller.host.dataset.cameraY = controller.world.y.toFixed(3);
	controller.host.dataset.follow = String(controller.follow);
	controller.host.dataset.trackExecution = String(controller.trackExecution);
	controller.host.dataset.zoom = controller.world.scale.x.toFixed(4);
	controller.host.dataset.zoomY = controller.world.scale.y.toFixed(4);
	const rootPosition =
		controller.rootKey === undefined
			? undefined
			: currentPosition(controller, controller.rootKey);
	if (rootPosition !== undefined) {
		controller.host.dataset.rootScreenX = (
			controller.world.x +
			rootPosition.x * controller.world.scale.x
		).toFixed(3);
		controller.host.dataset.rootScreenY = (
			controller.world.y +
			rootPosition.y * controller.world.scale.y
		).toFixed(3);
	} else {
		delete controller.host.dataset.rootScreenX;
		delete controller.host.dataset.rootScreenY;
	}
	updateDenseParticleScreenPositions(controller);
	publishLayoutSpan(controller);
}

export function approachCameraOnKey(
	controller: CanvasController,
	key: string,
	amount: number,
) {
	const position = currentPosition(controller, key);
	if (position === undefined) return;
	const targetX =
		controller.app.screen.width / 2 - position.x * controller.world.scale.x;
	const targetY =
		controller.app.screen.height / 2 - position.y * controller.world.scale.y;
	const deltaX = targetX - controller.world.x;
	const deltaY = targetY - controller.world.y;
	if (Math.abs(deltaX) <= 0.05 && Math.abs(deltaY) <= 0.05) {
		controller.world.position.set(targetX, targetY);
	} else {
		controller.world.position.set(
			controller.world.x + deltaX * amount,
			controller.world.y + deltaY * amount,
		);
	}
	controller.host.dataset.trackedKey = key;
	publishCamera(controller);
}

export function fitCamera(controller: CanvasController) {
	const positions =
		controller.mode === "summary"
			? controller.summaryPositions
			: controller.denseMode
				? controller.denseTargetPositions
				: new Map(
						[...controller.nodeViews].map(([key, view]) => [key, view.target]),
					);
	if (positions.size === 0) {
		controller.world.scale.set(1);
		controller.world.position.set(0, 0);
		publishCamera(controller);
		return;
	}
	const xs = [...positions].flatMap(([key, position]) => {
		const width = controller.nodeViews.get(key)?.width ?? 0;
		return [position.x - width / 2, position.x + width / 2];
	});
	const ys = [...positions.values()].map((position) => position.y);
	const minimumX = Math.min(...xs);
	const maximumX = Math.max(...xs);
	const minimumY = Math.min(...ys);
	const maximumY = Math.max(...ys);
	const width = Math.max(80, maximumX - minimumX + 80);
	const height = Math.max(80, maximumY - minimumY + 80);
	const fitScaleX = Math.min(
		MAX_FIT_ZOOM,
		Math.max(MIN_ZOOM, (controller.app.screen.width / width) * 0.9),
	);
	const fitScaleY = Math.min(
		MAX_FIT_ZOOM,
		Math.max(MIN_ZOOM, (controller.app.screen.height / height) * 0.9),
	);
	const scaleX = controller.denseMode
		? fitScaleX
		: Math.min(fitScaleX, fitScaleY);
	const scaleY = controller.denseMode
		? fitScaleY
		: Math.min(fitScaleX, fitScaleY);
	controller.world.scale.set(scaleX, scaleY);
	controller.world.position.set(
		controller.app.screen.width / 2 - ((minimumX + maximumX) / 2) * scaleX,
		controller.app.screen.height / 2 - ((minimumY + maximumY) / 2) * scaleY,
	);
	controller.cameraInitialized = true;
	publishCamera(controller);
}

function selectBatchedNode(
	controller: CanvasController,
	surface: HTMLCanvasElement,
	event: PointerEvent,
) {
	const positions =
		controller.mode === "summary"
			? controller.summaryPositions
			: controller.denseCurrentPositions;
	const bounds = surface.getBoundingClientRect();
	const pointerX = event.clientX - bounds.left;
	const pointerY = event.clientY - bounds.top;
	const maximumDistance = 14;
	let selected: { distance: number; key: string } | undefined;
	for (const [key, position] of positions) {
		const nodeX = controller.world.x + position.x * controller.world.scale.x;
		const nodeY = controller.world.y + position.y * controller.world.scale.y;
		const distance = (nodeX - pointerX) ** 2 + (nodeY - pointerY) ** 2;
		if (
			distance <= maximumDistance ** 2 &&
			(selected === undefined || distance < selected.distance)
		) {
			selected = { distance, key };
		}
	}
	if (selected !== undefined) {
		controller.onSelect(selected.key);
	}
}

export function installCameraControls(
	controller: CanvasController,
	surface: HTMLCanvasElement,
): () => void {
	let pointer:
		| { dragged: boolean; id: number; x: number; y: number }
		| undefined;
	const disableAutomaticCamera = () => {
		controller.follow = false;
		if (controller.trackExecution) {
			controller.trackExecution = false;
			controller.onTrackExecutionChange(false);
		}
	};
	const wheel = (event: WheelEvent) => {
		event.preventDefault();
		disableAutomaticCamera();
		const bounds = surface.getBoundingClientRect();
		const x = event.clientX - bounds.left;
		const y = event.clientY - bounds.top;
		const previous = controller.world.scale.x;
		const next = Math.min(
			MAX_ZOOM,
			Math.max(MIN_ZOOM, previous * Math.exp(-event.deltaY * 0.0015)),
		);
		const ratio = next / previous;
		controller.world.position.set(
			x - (x - controller.world.x) * ratio,
			y - (y - controller.world.y) * ratio,
		);
		controller.world.scale.set(next, controller.world.scale.y * ratio);
		publishCamera(controller);
	};
	const pointerDown = (event: PointerEvent) => {
		if (event.button !== 0) {
			return;
		}
		pointer = {
			dragged: false,
			id: event.pointerId,
			x: event.clientX,
			y: event.clientY,
		};
		publishCamera(controller);
		surface.setPointerCapture(event.pointerId);
	};
	const pointerMove = (event: PointerEvent) => {
		if (pointer?.id !== event.pointerId) {
			return;
		}
		disableAutomaticCamera();
		const deltaX = event.clientX - pointer.x;
		const deltaY = event.clientY - pointer.y;
		controller.world.x += deltaX;
		controller.world.y += deltaY;
		pointer = {
			dragged: pointer.dragged || Math.abs(deltaX) > 2 || Math.abs(deltaY) > 2,
			id: event.pointerId,
			x: event.clientX,
			y: event.clientY,
		};
		publishCamera(controller);
	};
	const pointerUp = (event: PointerEvent) => {
		if (pointer?.id === event.pointerId) {
			if (
				!pointer.dragged &&
				(controller.denseMode || controller.mode === "summary")
			) {
				selectBatchedNode(controller, surface, event);
			}
			pointer = undefined;
			surface.releasePointerCapture(event.pointerId);
		}
	};
	surface.addEventListener("wheel", wheel, { passive: false });
	surface.addEventListener("pointerdown", pointerDown, true);
	surface.addEventListener("pointermove", pointerMove, true);
	surface.addEventListener("pointerup", pointerUp, true);
	surface.addEventListener("pointercancel", pointerUp, true);
	return () => {
		surface.removeEventListener("wheel", wheel);
		surface.removeEventListener("pointerdown", pointerDown, true);
		surface.removeEventListener("pointermove", pointerMove, true);
		surface.removeEventListener("pointerup", pointerUp, true);
		surface.removeEventListener("pointercancel", pointerUp, true);
	};
}
