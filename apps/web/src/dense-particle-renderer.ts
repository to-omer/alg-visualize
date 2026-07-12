import { Particle, Rectangle } from "pixi.js";

import { entityIdKey, type StructureSnapshot } from "./engine-types";
import type { GraphLayout, Position } from "./graph-layout";
import type { CanvasController, NodeEmphasis } from "./pixi-controller";
import { getStructureNodeByKey } from "./structure-index";
import type { TracePresentation } from "./trace-visualization";

type EmphasisIndex = {
	compare: ReadonlySet<string>;
	mutation: ReadonlySet<string>;
	visited: ReadonlySet<string>;
};

const emphasisIndexCache = new WeakMap<TracePresentation, EmphasisIndex>();

function emphasisIndex(presentation: TracePresentation): EmphasisIndex {
	const cached = emphasisIndexCache.get(presentation);
	if (cached !== undefined) return cached;
	const index = {
		compare: new Set(presentation.compareKeys),
		mutation: new Set(presentation.mutationKeys),
		visited: new Set(presentation.visitedKeys),
	};
	emphasisIndexCache.set(presentation, index);
	return index;
}

export function nodeEmphasis(
	presentation: TracePresentation,
	key: string,
): NodeEmphasis {
	const index = emphasisIndex(presentation);
	if (index.mutation.has(key)) return "mutation";
	if (index.compare.has(key)) return "compare";
	return index.visited.has(key) ? "visited" : "normal";
}

export function particleTint(
	emphasis: NodeEmphasis,
	auxiliary: boolean,
): number {
	switch (emphasis) {
		case "mutation":
			return 0xff8a65;
		case "compare":
			return 0xffc15c;
		case "visited":
			return 0x53c7c5;
		case "normal":
			return auxiliary ? 0x56656c : 0x69767c;
	}
}

export function updateDenseParticles(
	controller: CanvasController,
	host: HTMLDivElement,
	structure: StructureSnapshot,
	layout: GraphLayout,
	presentation: TracePresentation,
	reuseLayout: boolean,
	previousDenseMode: boolean,
) {
	if (!controller.denseMode) {
		host.dataset.denseMutationGhostCount = "0";
		controller.denseCurrentPositions.clear();
		controller.denseEmphasizedKeys.clear();
		controller.denseParticleContainer.particleChildren = [];
		controller.denseParticleContainer.update();
		controller.denseParticles.clear();
		controller.denseTargetPositions.clear();
		return;
	}

	const previousPositions = controller.denseCurrentPositions;
	const previousParticles = controller.denseParticles;
	const emphasized = new Set([
		...presentation.visitedKeys,
		...presentation.compareKeys,
		...presentation.mutationKeys,
	]);
	const rebuildParticles =
		!reuseLayout ||
		!previousDenseMode ||
		Number(host.dataset.denseMutationGhostCount ?? "0") > 0;
	if (rebuildParticles) {
		const currentPositions = new Map<string, Position>();
		for (const [key, target] of layout.positions) {
			currentPositions.set(key, previousPositions.get(key) ?? { ...target });
		}
		const particles = new Map<string, Particle>();
		for (const node of layout.nodes) {
			const key = entityIdKey(node.id);
			const emphasis = nodeEmphasis(presentation, key);
			const particle =
				previousParticles.get(key) ??
				new Particle({
					anchorX: 0.5,
					anchorY: 0.5,
					texture: controller.denseTexture,
					tint: particleTint(emphasis, node.id.kind === "auxiliary"),
					x: 0,
					y: 0,
				});
			particle.tint = particleTint(emphasis, node.id.kind === "auxiliary");
			particles.set(key, particle);
		}
		let mutationGhostCount = 0;
		for (const key of presentation.mutationKeys) {
			if (particles.has(key)) continue;
			const particle = previousParticles.get(key);
			const position = previousPositions.get(key);
			if (particle === undefined || position === undefined) continue;
			particle.tint = particleTint("mutation", false);
			particles.set(key, particle);
			currentPositions.set(key, position);
			mutationGhostCount += 1;
		}
		controller.denseCurrentPositions = currentPositions;
		controller.denseParticles = particles;
		host.dataset.denseMutationGhostCount = String(mutationGhostCount);
		controller.denseParticleContainer.particleChildren = [
			...particles.values(),
		];
	} else {
		const changedEmphasis = new Set([
			...controller.denseEmphasizedKeys,
			...emphasized,
		]);
		for (const key of changedEmphasis) {
			const particle = previousParticles.get(key);
			const node = getStructureNodeByKey(structure, key);
			if (particle !== undefined && node !== undefined) {
				particle.tint = particleTint(
					nodeEmphasis(presentation, key),
					node.id.kind === "auxiliary",
				);
			}
		}
		host.dataset.denseMutationGhostCount = "0";
	}
	controller.denseTargetPositions = layout.positions;
	controller.denseEmphasizedKeys = emphasized;
	controller.denseParticleContainer.boundsArea = new Rectangle(
		0,
		0,
		Math.max(1, controller.app.screen.width),
		Math.max(1, controller.app.screen.height),
	);
	controller.denseParticleContainer.update();
}
