import { Container, Graphics, Text } from "pixi.js";
import { encodeSmallBtreeTopology } from "./btree-topology";
import { nodeEmphasis, updateDenseParticles } from "./dense-particle-renderer";
import {
	entityIdKey,
	type StructureNode,
	type StructureSnapshot,
} from "./engine-types";
import {
	layoutGraph,
	MAX_DETAIL_ENTITIES,
	nodeLabel,
	nodeVisualWidth,
} from "./graph-layout";
import {
	BATCHED_DETAIL_EXIT_THRESHOLD,
	BATCHED_DETAIL_THRESHOLD,
	type CanvasController,
	currentPosition,
	directedEdgeKey,
	displayResolution,
	fitCamera,
	type NodeEmphasis,
	type NodeView,
	sameEdges,
} from "./pixi-controller";
import { getStructureNodeByKey } from "./structure-index";
import type { TracePresentation } from "./trace-visualization";

function drawNode(
	view: Pick<
		NodeView,
		"auxiliary" | "label" | "multiEntry" | "shape" | "width"
	>,
	emphasis: NodeEmphasis,
	selected: boolean,
) {
	const palette = {
		normal: { fill: 0x252b2e, stroke: 0x657178, text: 0xe9edef },
		visited: { fill: 0x174c4d, stroke: 0x53c7c5, text: 0xe9ffff },
		compare: { fill: 0xd78b2c, stroke: 0xffcf76, text: 0x17120b },
		mutation: { fill: 0xcf633d, stroke: 0xffa37f, text: 0x1b0d08 },
	}[emphasis];
	view.shape.clear();
	if (view.multiEntry) {
		view.shape
			.roundRect(-view.width / 2, -20, view.width, 40, 7)
			.fill(palette.fill)
			.stroke({
				color: selected ? 0xf4fbff : palette.stroke,
				width: selected ? 4 : 2,
			});
	} else if (view.auxiliary) {
		view.shape
			.roundRect(-view.width / 2, -17, view.width, 34, 6)
			.fill(emphasis === "normal" ? 0x20282b : palette.fill)
			.stroke({ color: palette.stroke, width: selected ? 3 : 1.5 });
	} else {
		view.shape
			.circle(0, 0, emphasis === "normal" ? 24 : 27)
			.fill(palette.fill)
			.stroke({
				color: selected ? 0xf4fbff : palette.stroke,
				width: selected ? 4 : 2,
			});
	}
	if (view.label !== undefined) {
		view.label.style.fill = palette.text;
	}
}

function createNodeLabel(node: StructureNode): Text {
	const label = new Text({
		resolution: displayResolution(),
		text: nodeLabel(node),
		style: {
			fill: 0xe9edef,
			fontFamily: "ui-monospace, monospace",
			fontSize: node.role.startsWith("btree-")
				? 11
				: node.id.kind === "auxiliary"
					? 10
					: 12,
			fontWeight: "600",
		},
	});
	label.anchor.set(0.5);
	return label;
}

function createNodeView(
	controller: CanvasController,
	node: StructureNode,
	key: string,
	showLabel: boolean,
): NodeView {
	const auxiliary = node.id.kind === "auxiliary";
	const multiEntry = node.role.startsWith("btree-");
	const container = new Container();
	container.alpha = 0;
	container.cursor = "pointer";
	container.eventMode = "static";
	container.on("pointertap", () => controller.onSelect(key));
	const shape = new Graphics();
	const label = showLabel ? createNodeLabel(node) : undefined;
	container.addChild(shape);
	if (label !== undefined) {
		container.addChild(label);
	}
	const view = {
		auxiliary,
		container,
		exiting: false,
		emphasis: nodeEmphasis(controller.presentation, key),
		label,
		multiEntry,
		selected: key === controller.selectedKey,
		shape,
		target: { x: 0, y: 0 },
		width: nodeVisualWidth(node),
	};
	drawNode(view, view.emphasis, view.selected);
	return view;
}

export function updateScene(
	controller: CanvasController,
	host: HTMLDivElement,
	structure: StructureSnapshot | undefined,
	activeKey: string | undefined,
	selectedKey: string | undefined,
	presentation: TracePresentation,
	transitionMs: number,
) {
	const traversalChanged =
		controller.presentation.traversalEdge !== presentation.traversalEdge;
	controller.activeKey = activeKey;
	if (activeKey === undefined) {
		delete host.dataset.activeKey;
	} else {
		host.dataset.activeKey = activeKey;
	}
	controller.selectedKey = selectedKey;
	controller.presentation = presentation;
	if (traversalChanged) {
		controller.traversalProgress =
			presentation.traversalEdge === undefined || controller.reducedMotion
				? 1
				: 0;
	}
	controller.rootKey =
		structure?.root === null || structure?.root === undefined
			? undefined
			: entityIdKey(structure.root);
	controller.motionDurationMs = transitionMs;
	const activeNode =
		structure === undefined || activeKey === undefined
			? undefined
			: getStructureNodeByKey(structure, activeKey);
	controller.activeLabel =
		activeNode === undefined ? undefined : nodeLabel(activeNode);
	for (const [key, view] of controller.nodeViews) {
		view.exiting = true;
		const emphasis = nodeEmphasis(presentation, key);
		if (view.emphasis !== emphasis) {
			view.emphasis = emphasis;
			drawNode(view, emphasis, view.selected);
		}
	}
	if (structure === undefined) {
		controller.layout = undefined;
		controller.layoutStructure = undefined;
		host.dataset.denseMutationGhostCount = "0";
		controller.edgePairs = [];
		controller.exitingEdgePairs = [];
		controller.enteringEdgeKeys.clear();
		controller.edges.clear();
		controller.denseCurrentPositions.clear();
		controller.denseEmphasizedKeys.clear();
		controller.denseParticleContainer.particleChildren = [];
		controller.denseParticleContainer.update();
		controller.denseParticles.clear();
		controller.denseTargetPositions.clear();
		controller.summaryNodes.clear();
		controller.summaryPositions.clear();
		host.dataset.entityCount = "0";
		host.dataset.renderedCount = "0";
		host.dataset.structuralTransition = "settled";
		delete host.dataset.btreeTopology;
		return;
	}
	const btreeTopology = encodeSmallBtreeTopology(structure);
	if (btreeTopology === undefined) {
		delete host.dataset.btreeTopology;
	} else {
		host.dataset.btreeTopology = btreeTopology;
	}
	const sceneStartedAt = performance.now();
	const layoutStartedAt = performance.now();
	const importantKeys = [
		...new Set([
			...presentation.currentKeys,
			...(selectedKey === undefined ? [] : [selectedKey]),
			...presentation.visitedKeys,
		]),
	];
	const importantSignature = importantKeys.join("\u0000");
	const width = controller.app.screen.width;
	const height = controller.app.screen.height;
	const detailMode = structure.nodes.length <= MAX_DETAIL_ENTITIES;
	const reuseLayout =
		controller.layout !== undefined &&
		controller.layoutStructure === structure &&
		controller.layoutWidth === width &&
		controller.layoutHeight === height &&
		(detailMode || controller.layoutImportantSignature === importantSignature);
	const layout =
		reuseLayout && controller.layout !== undefined
			? controller.layout
			: layoutGraph(structure, width, height, importantKeys);
	if (!reuseLayout) {
		controller.layout = layout;
		controller.layoutStructure = structure;
		controller.layoutWidth = width;
		controller.layoutHeight = height;
		controller.layoutImportantSignature = importantSignature;
	}
	host.dataset.layoutReused = String(reuseLayout);
	const previousMode = controller.mode;
	const previousDenseMode = controller.denseMode;
	controller.mode = layout.mode;
	host.dataset.layoutMs = (performance.now() - layoutStartedAt).toFixed(3);
	const structureChanged = !sameEdges(controller.edgePairs, layout.edges);
	if (structureChanged && controller.edgePairs.length > 0) {
		controller.edgeTransitionPositions = new Map();
		for (const [source, target] of controller.edgePairs) {
			for (const key of [source, target]) {
				const position = currentPosition(controller, key);
				if (position !== undefined) {
					controller.edgeTransitionPositions.set(key, { ...position });
				}
			}
		}
		const nextKeys = new Set(layout.edges.map(directedEdgeKey));
		const previousKeys = new Set(controller.edgePairs.map(directedEdgeKey));
		controller.exitingEdgePairs = controller.edgePairs.filter(
			(pair) => !nextKeys.has(directedEdgeKey(pair)),
		);
		controller.enteringEdgeKeys = new Set(
			layout.edges
				.filter((pair) => !previousKeys.has(directedEdgeKey(pair)))
				.map(directedEdgeKey),
		);
		controller.edgeProgress = controller.reducedMotion ? 1 : 0;
	} else if (structureChanged) {
		controller.exitingEdgePairs = [];
		controller.enteringEdgeKeys.clear();
		controller.edgeProgress = 1;
	}
	controller.edgePairs = layout.edges;
	controller.edgesDirty = true;
	controller.summaryPositions =
		layout.mode === "summary" ? layout.positions : new Map();
	controller.summaryDirty = true;
	controller.denseMode =
		layout.mode === "detail" &&
		(layout.nodes.length > BATCHED_DETAIL_THRESHOLD ||
			(previousDenseMode &&
				layout.nodes.length >= BATCHED_DETAIL_EXIT_THRESHOLD));
	const projectionModeChanged =
		previousMode !== controller.mode ||
		previousDenseMode !== controller.denseMode;
	if (layout.mode === "detail") {
		controller.summaryNodes.clear();
	}
	updateDenseParticles(
		controller,
		host,
		structure,
		layout,
		presentation,
		reuseLayout,
		previousDenseMode,
	);
	if (layout.mode === "detail" && !controller.denseMode) {
		const showAllLabels = layout.nodes.length <= BATCHED_DETAIL_THRESHOLD;
		for (const node of layout.nodes) {
			const key = entityIdKey(node.id);
			const showLabel = showAllLabels || key === activeKey;
			let view = controller.nodeViews.get(key);
			if (view === undefined) {
				view = createNodeView(controller, node, key, showLabel);
				const target = layout.positions.get(key) ?? { x: 0, y: 0 };
				view.container.position.set(target.x, target.y);
				controller.world.addChild(view.container);
				controller.nodeViews.set(key, view);
			}
			view.exiting = false;
			if (showLabel && view.label === undefined) {
				view.label = createNodeLabel(node);
				view.container.addChild(view.label);
			} else if (!showLabel && view.label !== undefined) {
				view.label.destroy();
				view.label = undefined;
			}
			if (view.label !== undefined) {
				view.label.text = nodeLabel(node);
			}
			const width = nodeVisualWidth(node);
			const multiEntry = node.role.startsWith("btree-");
			const geometryChanged =
				view.width !== width || view.multiEntry !== multiEntry;
			view.width = width;
			view.multiEntry = multiEntry;
			const emphasis = nodeEmphasis(presentation, key);
			const selected = key === selectedKey;
			if (
				geometryChanged ||
				view.emphasis !== emphasis ||
				view.selected !== selected
			) {
				view.emphasis = emphasis;
				view.selected = selected;
				drawNode(view, emphasis, selected);
			}
			view.target = layout.positions.get(key) ?? view.target;
		}
	}
	host.dataset.entityCount = String(structure.nodes.length);
	host.dataset.renderedCount = String(layout.nodes.length);
	host.dataset.multiEntryNodeCount = String(
		layout.nodes.filter((node) => node.role.startsWith("btree-")).length,
	);
	host.dataset.maxNodeWidth = Math.max(
		0,
		...layout.nodes.map(nodeVisualWidth),
	).toFixed(3);
	if (layout.positions.size <= 500) {
		const positions = [...layout.positions.values()];
		let minimumDistance = Number.POSITIVE_INFINITY;
		for (let left = 0; left < positions.length; left += 1) {
			const from = positions[left];
			if (from === undefined) {
				continue;
			}
			for (let right = left + 1; right < positions.length; right += 1) {
				const to = positions[right];
				if (to !== undefined) {
					minimumDistance = Math.min(
						minimumDistance,
						Math.hypot(to.x - from.x, to.y - from.y),
					);
				}
			}
		}
		host.dataset.minimumNodeDistance = Number.isFinite(minimumDistance)
			? minimumDistance.toFixed(3)
			: "0.000";
	} else {
		delete host.dataset.minimumNodeDistance;
	}
	host.dataset.mode = layout.mode;
	host.dataset.renderStrategy = controller.denseMode ? "batched" : layout.mode;
	host.dataset.compareNodeCount = String(presentation.compareKeys.length);
	host.dataset.currentEdgeCount = String(presentation.currentEdges.length);
	host.dataset.traversalProgress = controller.traversalProgress.toFixed(3);
	if (presentation.traversalEdge === undefined) {
		delete host.dataset.traversalEdge;
	} else {
		host.dataset.traversalEdge = `${presentation.traversalEdge.source}>${presentation.traversalEdge.target}`;
	}
	host.dataset.enteringEdgeCount = String(controller.enteringEdgeKeys.size);
	host.dataset.exitingEdgeCount = String(controller.exitingEdgePairs.length);
	host.dataset.mutationNodeCount = String(presentation.mutationKeys.length);
	host.dataset.visitedEdgeCount = String(presentation.visitedEdges.length);
	host.dataset.visitedNodeCount = String(presentation.visitedKeys.length);
	host.dataset.structuralTransition =
		controller.edgeProgress < 1 ? "active" : "settled";
	if (projectionModeChanged) {
		controller.follow = true;
	}
	if (controller.trackExecution) {
		controller.follow = false;
	} else if (controller.follow || !controller.cameraInitialized) {
		delete controller.host.dataset.trackedKey;
		fitCamera(controller);
	} else {
		delete controller.host.dataset.trackedKey;
	}
	host.dataset.sceneUpdateMs = (performance.now() - sceneStartedAt).toFixed(3);
}
