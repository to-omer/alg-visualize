import "pixi.js/unsafe-eval";
import {
	Application,
	Container,
	Graphics,
	type Particle,
	ParticleContainer,
	Rectangle,
	Text,
} from "pixi.js";
import {
	type KeyboardEvent as ReactKeyboardEvent,
	useEffect,
	useMemo,
	useRef,
} from "react";
import { createScreenMarkers, updateScreenMarkers } from "./canvas-markers";
import { nodeEmphasis, particleTint } from "./dense-particle-renderer";
import { entityIdKey, type StructureSnapshot } from "./engine-types";
import { MAX_DETAIL_ENTITIES } from "./graph-layout";
import {
	approachCameraOnKey,
	BATCHED_DETAIL_THRESHOLD,
	type CanvasController,
	currentPosition,
	displayResolution,
	fitCamera,
	installCameraControls,
	updateDenseParticleScreenPositions,
} from "./pixi-controller";
import { drawEdges } from "./pixi-edge-renderer";
import { updateScene } from "./pixi-scene-renderer";
import type { TracePresentation } from "./trace-visualization";

function navigationNodes(structure: StructureSnapshot | undefined) {
	const nodes = structure?.nodes ?? [];
	if (nodes.length > MAX_DETAIL_ENTITIES) return nodes;
	return [...nodes].sort((left, right) => {
		const leftKey = left.keys[0];
		const rightKey = right.keys[0];
		if (leftKey !== undefined && rightKey !== undefined) {
			const order =
				leftKey.length - rightKey.length || leftKey.localeCompare(rightKey);
			if (order !== 0) return order;
		} else if (leftKey !== undefined) {
			return -1;
		} else if (rightKey !== undefined) {
			return 1;
		}
		return entityIdKey(left.id).localeCompare(entityIdKey(right.id));
	});
}

type PixiCanvasProps = {
	activeKey: string | undefined;
	fitSignal: number;
	onContextState: (state: "lost" | "restored") => void;
	onError: (message: string) => void;
	onSelect: (key: string) => void;
	onTrackExecutionChange: (tracking: boolean) => void;
	presentation: TracePresentation;
	selectedKey: string | undefined;
	structure: StructureSnapshot | undefined;
	trackExecution: boolean;
	transitionMs: number;
};

export function PixiCanvas({
	activeKey,
	fitSignal,
	onContextState,
	onError,
	onSelect,
	onTrackExecutionChange,
	presentation,
	selectedKey,
	structure,
	trackExecution,
	transitionMs,
}: PixiCanvasProps) {
	const host = useRef<HTMLDivElement>(null);
	const controller = useRef<CanvasController | undefined>(undefined);
	const lastFitSignal = useRef(fitSignal);
	const latest = useRef({
		activeKey,
		onContextState,
		onError,
		onSelect,
		onTrackExecutionChange,
		presentation,
		selectedKey,
		structure,
		trackExecution,
		transitionMs,
	});
	latest.current = {
		activeKey,
		onContextState,
		onError,
		onSelect,
		onTrackExecutionChange,
		presentation,
		selectedKey,
		structure,
		trackExecution,
		transitionMs,
	};

	useEffect(() => {
		const element = host.current;
		if (element === null) {
			return;
		}
		const app = new Application();
		const themeMedia = window.matchMedia("(prefers-color-scheme: light)");
		let disposed = false;
		let initialized = false;
		let cleanup: (() => void) | undefined;

		void app
			.init({
				antialias: true,
				autoDensity: true,
				background: themeMedia.matches ? "#f3f5f4" : "#15191b",
				preference: "webgl",
				resizeTo: element,
				resolution: displayResolution(),
			})
			.then(() => {
				initialized = true;
				if (disposed) {
					app.destroy(true, { children: true });
					return;
				}
				app.canvas.setAttribute("aria-hidden", "true");
				app.canvas.tabIndex = -1;
				app.canvas.setAttribute("data-testid", "structure-canvas");
				element.appendChild(app.canvas);
				element.dataset.renderResolution = app.renderer.resolution.toFixed(3);
				const warmup = new Container();
				const warmupShape = new Graphics()
					.circle(0, 0, 24)
					.fill(0x252b2e)
					.stroke({ color: 0x657178, width: 2 })
					.moveTo(-20, 30)
					.lineTo(20, 60)
					.stroke({ color: 0x465056, width: 1.5 });
				const warmupText = new Text({
					resolution: displayResolution(),
					text: "0",
					style: { fill: 0xe9edef, fontFamily: "ui-monospace", fontSize: 12 },
				});
				warmup.alpha = 0.01;
				warmup.position.set(-100, -100);
				warmup.addChild(warmupShape, warmupText);
				app.stage.addChild(warmup);
				const world = new Container({ isRenderGroup: true });
				const edges = new Graphics();
				const summaryNodes = new Graphics();
				const particleSource = new Graphics().circle(4, 4, 3).fill(0xffffff);
				const denseTexture = app.renderer.generateTexture({
					antialias: true,
					frame: new Rectangle(0, 0, 8, 8),
					target: particleSource,
				});
				particleSource.destroy();
				const denseParticleContainer = new ParticleContainer<Particle>({
					dynamicProperties: { position: true },
					texture: denseTexture,
				});
				world.addChild(edges, summaryNodes);
				app.stage.addChild(world, denseParticleContainer);
				const markers = createScreenMarkers();
				app.stage.addChild(markers.selected, markers.active);
				let warmupFrames = 0;
				const warmRenderer = () => {
					warmupFrames += 1;
					if (warmupFrames >= 2) {
						app.ticker.remove(warmRenderer);
						app.stage.removeChild(warmup);
						warmup.destroy({ children: true });
						element.dataset.renderer = "webgl";
					}
				};
				app.ticker.add(warmRenderer);
				const media = window.matchMedia("(prefers-reduced-motion: reduce)");
				element.dataset.motion = media.matches ? "reduced" : "full";
				const state: CanvasController = {
					activeLabel: undefined,
					activeKey: latest.current.activeKey,
					app,
					cameraInitialized: false,
					edgePairs: [],
					edgeProgress: 1,
					edgeTransitionPositions: new Map(),
					enteringEdgeKeys: new Set(),
					edges,
					edgesDirty: true,
					exitingEdgePairs: [],
					denseCurrentPositions: new Map(),
					denseEmphasizedKeys: new Set(),
					denseMode: false,
					denseParticleContainer,
					denseParticles: new Map(),
					denseTargetPositions: new Map(),
					denseTexture,
					follow: true,
					host: element,
					layout: undefined,
					layoutHeight: 0,
					layoutImportantSignature: "",
					layoutStructure: undefined,
					layoutWidth: 0,
					mode: "detail",
					motionDurationMs: latest.current.transitionMs,
					nodeViews: new Map(),
					onContextState: latest.current.onContextState,
					onSelect: latest.current.onSelect,
					onTrackExecutionChange: latest.current.onTrackExecutionChange,
					reducedMotion: media.matches,
					presentation: latest.current.presentation,
					rootKey: undefined,
					selectedKey: latest.current.selectedKey,
					trackExecution: latest.current.trackExecution,
					traversalProgress: 1,
					summaryDirty: true,
					summaryNodes,
					summaryPositions: new Map(),
					world,
				};
				controller.current = state;
				const contextLost = (event: Event) => {
					event.preventDefault();
					element.dataset.context = "lost";
					state.onContextState("lost");
				};
				const contextRestored = () => {
					element.dataset.context = "restored";
					state.onContextState("restored");
					updateScene(
						state,
						element,
						latest.current.structure,
						latest.current.activeKey,
						latest.current.selectedKey,
						latest.current.presentation,
						latest.current.transitionMs,
					);
				};
				app.canvas.addEventListener("webglcontextlost", contextLost);
				app.canvas.addEventListener("webglcontextrestored", contextRestored);
				const motionChanged = (event: MediaQueryListEvent) => {
					state.reducedMotion = event.matches;
					element.dataset.motion = event.matches ? "reduced" : "full";
				};
				media.addEventListener("change", motionChanged);
				const themeChanged = (event: MediaQueryListEvent) => {
					app.renderer.background.color = event.matches ? 0xf3f5f4 : 0x15191b;
				};
				themeMedia.addEventListener("change", themeChanged);
				const removeCameraControls = installCameraControls(state, app.canvas);
				const resizeObserver = new ResizeObserver(() => {
					updateScene(
						state,
						element,
						latest.current.structure,
						latest.current.activeKey,
						latest.current.selectedKey,
						latest.current.presentation,
						latest.current.transitionMs,
					);
				});
				resizeObserver.observe(element);
				updateScene(
					state,
					element,
					latest.current.structure,
					latest.current.activeKey,
					latest.current.selectedKey,
					latest.current.presentation,
					latest.current.transitionMs,
				);
				app.ticker.add((ticker) => {
					const amount = state.reducedMotion
						? 1
						: 1 -
							Math.exp(
								-ticker.deltaMS / Math.max(30, state.motionDurationMs / 3),
							);
					let geometryChanged = state.edgesDirty;
					if (state.edgeProgress < 1) {
						state.edgeProgress = state.reducedMotion
							? 1
							: Math.min(
									1,
									state.edgeProgress + ticker.deltaMS / state.motionDurationMs,
								);
						geometryChanged = true;
					}
					if (
						state.presentation.traversalEdge !== undefined &&
						state.traversalProgress < 1
					) {
						state.traversalProgress = state.reducedMotion
							? 1
							: Math.min(
									1,
									state.traversalProgress +
										ticker.deltaMS / state.motionDurationMs,
								);
						state.host.dataset.traversalProgress =
							state.traversalProgress.toFixed(3);
						geometryChanged = true;
					}
					if (state.denseMode) {
						for (const [key, target] of state.denseTargetPositions) {
							const current = state.denseCurrentPositions.get(key);
							if (current === undefined) {
								state.denseCurrentPositions.set(key, { ...target });
								geometryChanged = true;
								continue;
							}
							const deltaX = target.x - current.x;
							const deltaY = target.y - current.y;
							if (Math.abs(deltaX) > 0.05 || Math.abs(deltaY) > 0.05) {
								current.x += deltaX * amount;
								current.y += deltaY * amount;
								geometryChanged = true;
							} else {
								current.x = target.x;
								current.y = target.y;
							}
						}
						updateDenseParticleScreenPositions(state);
					}
					for (const [key, view] of state.nodeViews) {
						const active = key === state.activeKey;
						view.container.scale.set(
							active && state.world.scale.x < 1 ? 1 / state.world.scale.x : 1,
						);
						const deltaX = view.target.x - view.container.x;
						const deltaY = view.target.y - view.container.y;
						if (Math.abs(deltaX) > 0.05 || Math.abs(deltaY) > 0.05) {
							view.container.x += deltaX * amount;
							view.container.y += deltaY * amount;
							geometryChanged = true;
						} else {
							view.container.position.set(view.target.x, view.target.y);
						}
						view.container.alpha +=
							((view.exiting ? 0 : 1) - view.container.alpha) * amount;
						if (view.label !== undefined) {
							const labelThreshold =
								state.nodeViews.size <= 100
									? 0
									: state.nodeViews.size <= BATCHED_DETAIL_THRESHOLD
										? 0.42
										: 0.8;
							view.label.visible =
								active ||
								view.selected ||
								state.world.scale.x >= labelThreshold;
						}
						if (view.exiting && view.container.alpha < 0.01) {
							view.container.destroy({ children: true });
							state.nodeViews.delete(key);
						}
					}
					state.host.dataset.visibleLabelCount = String(
						[...state.nodeViews.values()].filter(
							(view) => !view.exiting && view.label?.visible === true,
						).length,
					);
					if (state.trackExecution && state.activeKey !== undefined) {
						approachCameraOnKey(state, state.activeKey, amount);
					}
					if (geometryChanged || state.summaryDirty) {
						drawEdges(state);
					}
					state.edgesDirty = false;
					if (state.mode === "summary" && state.summaryDirty) {
						summaryNodes.clear();
						for (const [key, position] of state.summaryPositions) {
							const emphasis = nodeEmphasis(state.presentation, key);
							summaryNodes
								.circle(position.x, position.y, emphasis === "normal" ? 2 : 4)
								.fill(particleTint(emphasis, false));
						}
						state.summaryDirty = false;
					}
					if (state.edgeProgress >= 1) {
						state.exitingEdgePairs = [];
						state.enteringEdgeKeys.clear();
						state.edgeTransitionPositions.clear();
						state.host.dataset.structuralTransition = "settled";
						state.host.dataset.enteringEdgeCount = "0";
						state.host.dataset.exitingEdgeCount = "0";
					}
					const activeScreenPosition =
						state.activeKey === undefined
							? undefined
							: currentPosition(state, state.activeKey);
					if (activeScreenPosition === undefined) {
						delete state.host.dataset.activeScreenX;
						delete state.host.dataset.activeScreenY;
					} else {
						state.host.dataset.activeScreenX = (
							state.world.x +
							activeScreenPosition.x * state.world.scale.x
						).toFixed(3);
						state.host.dataset.activeScreenY = (
							state.world.y +
							activeScreenPosition.y * state.world.scale.y
						).toFixed(3);
					}
					const activePosition =
						state.activeKey === undefined
							? undefined
							: state.mode === "summary"
								? state.summaryPositions.get(state.activeKey)
								: state.denseMode
									? state.denseCurrentPositions.get(state.activeKey)
									: undefined;
					const selectedPosition =
						state.selectedKey === undefined
							? undefined
							: state.mode === "summary"
								? state.summaryPositions.get(state.selectedKey)
								: state.denseMode
									? state.denseCurrentPositions.get(state.selectedKey)
									: undefined;
					updateScreenMarkers(
						markers,
						state,
						activePosition,
						selectedPosition,
						nodeEmphasis(state.presentation, state.activeKey ?? ""),
					);
				});
				cleanup = () => {
					app.ticker.remove(warmRenderer);
					resizeObserver.disconnect();
					removeCameraControls();
					media.removeEventListener("change", motionChanged);
					themeMedia.removeEventListener("change", themeChanged);
					app.canvas.removeEventListener("webglcontextlost", contextLost);
					app.canvas.removeEventListener(
						"webglcontextrestored",
						contextRestored,
					);
					denseTexture.destroy(true);
				};
			})
			.catch((error: unknown) => {
				if (!disposed) {
					const message =
						error instanceof Error
							? error.message
							: "WebGL renderer initialization failed";
					element.dataset.renderer = "failed";
					element.dataset.rendererError = message;
					latest.current.onError(message);
				}
			});

		return () => {
			disposed = true;
			cleanup?.();
			controller.current = undefined;
			if (initialized) {
				app.destroy(true, { children: true });
			}
		};
	}, []);

	useEffect(() => {
		const state = controller.current;
		const element = host.current;
		if (state !== undefined && element !== null) {
			state.onSelect = onSelect;
			state.onContextState = onContextState;
			state.onTrackExecutionChange = onTrackExecutionChange;
			state.trackExecution = trackExecution;
			updateScene(
				state,
				element,
				structure,
				activeKey,
				selectedKey,
				presentation,
				transitionMs,
			);
		}
	}, [
		activeKey,
		onContextState,
		onSelect,
		onTrackExecutionChange,
		presentation,
		selectedKey,
		structure,
		trackExecution,
		transitionMs,
	]);

	useEffect(() => {
		if (lastFitSignal.current === fitSignal) {
			return;
		}
		lastFitSignal.current = fitSignal;
		const state = controller.current;
		if (state !== undefined) {
			state.follow = true;
			fitCamera(state);
		}
	}, [fitSignal]);

	const nodes = useMemo(() => navigationNodes(structure), [structure]);
	const rootKey =
		structure?.root === null || structure?.root === undefined
			? undefined
			: entityIdKey(structure.root);
	const navigationKey =
		selectedKey ??
		activeKey ??
		rootKey ??
		(nodes[0] === undefined ? undefined : entityIdKey(nodes[0].id));
	const navigationIndex = nodes.findIndex(
		(node) => entityIdKey(node.id) === navigationKey,
	);
	const navigationNode = nodes[navigationIndex];
	const navigate = (event: ReactKeyboardEvent<HTMLDivElement>) => {
		if (nodes.length === 0) return;
		let nextIndex: number | undefined;
		switch (event.key) {
			case "ArrowLeft":
			case "ArrowUp":
				nextIndex = Math.max(0, navigationIndex - 1);
				break;
			case "ArrowRight":
			case "ArrowDown":
				nextIndex = Math.min(nodes.length - 1, navigationIndex + 1);
				break;
			case "Home":
				nextIndex = 0;
				break;
			case "End":
				nextIndex = nodes.length - 1;
				break;
		}
		const next = nextIndex === undefined ? undefined : nodes[nextIndex];
		if (next !== undefined) {
			event.preventDefault();
			onSelect(entityIdKey(next.id));
		}
	};
	const navigationLabel =
		navigationNode === undefined
			? undefined
			: `${navigationNode.role}; ${
					navigationNode.keys.length === 0
						? "no keys"
						: `keys ${navigationNode.keys.join(", ")}`
				}; ${
					navigationNode.links.length === 0
						? "no links"
						: navigationNode.links
								.map((link) => `${link.role} to ${entityIdKey(link.target)}`)
								.join(", ")
				}`;
	const navigationItemId =
		navigationNode === undefined
			? undefined
			: `structure-navigation-${entityIdKey(navigationNode.id).replaceAll(":", "-")}`;

	return (
		<div
			aria-activedescendant={navigationItemId}
			aria-label="Structure navigation"
			className="pixi-host"
			data-testid="pixi-host"
			onKeyDown={navigate}
			ref={host}
			role="listbox"
			tabIndex={0}
		>
			{navigationNode !== undefined && (
				<div
					aria-label={navigationLabel}
					aria-posinset={navigationIndex + 1}
					aria-selected="true"
					aria-setsize={nodes.length}
					className="visually-hidden"
					id={navigationItemId}
					role="option"
					tabIndex={-1}
				/>
			)}
		</div>
	);
}
