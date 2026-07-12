import {
	entityIdKey,
	type StructureNode,
	type StructureSnapshot,
} from "./engine-types";
import { getStructureNodeByKey } from "./structure-index";

export type CanvasMode = "detail" | "summary";

export type Position = { x: number; y: number };

export type GraphLayout = {
	edges: [string, string][];
	mode: CanvasMode;
	nodes: StructureNode[];
	positions: Map<string, Position>;
};

export const MAX_DETAIL_ENTITIES = 8_000;
const MAX_SUMMARY_CORE_ENTITIES = 1_000;
const MAX_SUMMARY_ENTITIES = 2_000;

function compareDecimal(left: string, right: string): number {
	const normalizedLeft = left.replace(/^0+(?=\d)/, "");
	const normalizedRight = right.replace(/^0+(?=\d)/, "");
	return (
		normalizedLeft.length - normalizedRight.length ||
		normalizedLeft.localeCompare(normalizedRight)
	);
}

function compareNodes(left: StructureNode, right: StructureNode): number {
	const leftKey = left.keys[0];
	const rightKey = right.keys[0];
	if (leftKey !== undefined && rightKey !== undefined) {
		const keyOrder = compareDecimal(leftKey, rightKey);
		if (keyOrder !== 0) {
			return keyOrder;
		}
	} else if (leftKey !== undefined) {
		return -1;
	} else if (rightKey !== undefined) {
		return 1;
	}
	return entityIdKey(left.id).localeCompare(entityIdKey(right.id));
}

function usesOrderedTreeLayout(nodes: readonly StructureNode[]): boolean {
	return nodes.every(
		(node) => node.role === "binary-node" || node.role === "yfast-bucket-node",
	);
}

function graphSubset(
	structure: StructureSnapshot,
	importantKeys: readonly string[],
	mode: CanvasMode,
): { depth: Map<string, number>; nodes: StructureNode[] } {
	const boundedNodes = structure.nodes;
	const root = structure.root === null ? boundedNodes[0]?.id : structure.root;
	if (root === undefined) {
		return { depth: new Map(), nodes: [] };
	}

	const limit =
		mode === "detail" ? MAX_DETAIL_ENTITIES : MAX_SUMMARY_CORE_ENTITIES;
	const queue: { depth: number; key: string }[] = [
		{ depth: 0, key: entityIdKey(root) },
	];
	let queueIndex = 0;
	const depth = new Map<string, number>();
	const nodes: StructureNode[] = [];
	while (queueIndex < queue.length && nodes.length < limit) {
		const item = queue[queueIndex];
		queueIndex += 1;
		if (item === undefined || depth.has(item.key)) {
			continue;
		}
		const node = getStructureNodeByKey(structure, item.key);
		if (node === undefined) {
			continue;
		}
		depth.set(item.key, item.depth);
		nodes.push(node);
		const neighbors = node.links
			.map((link) => entityIdKey(link.target))
			.filter(
				(target) => getStructureNodeByKey(structure, target) !== undefined,
			);
		for (const target of neighbors) {
			queue.push({ depth: item.depth + 1, key: target });
		}
	}

	const fallbackDepth = Math.max(0, ...depth.values()) + 1;
	if (mode === "detail") {
		for (const node of boundedNodes) {
			const key = entityIdKey(node.id);
			if (!depth.has(key)) {
				depth.set(key, fallbackDepth);
				nodes.push(node);
			}
		}
	} else {
		const sampleCount = Math.min(MAX_SUMMARY_ENTITIES, boundedNodes.length);
		for (
			let sample = 0;
			sample < sampleCount && nodes.length < MAX_SUMMARY_ENTITIES;
			sample += 1
		) {
			const index = Math.floor((sample * boundedNodes.length) / sampleCount);
			const node = boundedNodes[index];
			if (node === undefined) {
				continue;
			}
			const key = entityIdKey(node.id);
			if (!depth.has(key)) {
				depth.set(key, fallbackDepth);
				nodes.push(node);
			}
		}
		const protectedKeys = new Set(
			[...new Set(importantKeys)].slice(0, MAX_SUMMARY_ENTITIES),
		);
		for (const importantKey of protectedKeys) {
			if (depth.has(importantKey)) {
				continue;
			}
			const importantNode = getStructureNodeByKey(structure, importantKey);
			if (importantNode === undefined) {
				continue;
			}
			if (nodes.length === MAX_SUMMARY_ENTITIES) {
				const removableIndex = nodes.findLastIndex(
					(node) => !protectedKeys.has(entityIdKey(node.id)),
				);
				const removed =
					removableIndex < 0 ? undefined : nodes.splice(removableIndex, 1)[0];
				if (removed !== undefined) {
					depth.delete(entityIdKey(removed.id));
				}
			}
			depth.set(importantKey, fallbackDepth);
			nodes.push(importantNode);
		}
	}
	return { depth, nodes };
}

export function layoutGraph(
	structure: StructureSnapshot,
	width: number,
	height: number,
	importantKeys: readonly string[],
): GraphLayout {
	const mode =
		structure.nodes.length <= MAX_DETAIL_ENTITIES ? "detail" : "summary";
	const graph = graphSubset(structure, importantKeys, mode);
	const positions = new Map<string, Position>();
	const keyed = graph.nodes
		.filter((node) => node.keys.length > 0)
		.sort((left, right) =>
			compareDecimal(left.keys[0] ?? "0", right.keys[0] ?? "0"),
		);
	const keyRank = new Map(
		keyed.map((node, index) => [entityIdKey(node.id), index]),
	);
	const levels = new Map<number, StructureNode[]>();
	for (const node of graph.nodes) {
		const nodeDepth = graph.depth.get(entityIdKey(node.id)) ?? 0;
		const level = levels.get(nodeDepth) ?? [];
		level.push(node);
		levels.set(nodeDepth, level);
	}
	const maximumDepth = Math.max(0, ...levels.keys());
	const orderedTree = usesOrderedTreeLayout(graph.nodes);
	const levelGap = 36;
	const levelSpans = new Map<number, number>();
	for (const [nodeDepth, nodes] of levels) {
		levelSpans.set(
			nodeDepth,
			nodes.reduce((span, node) => span + nodeVisualWidth(node), 0) +
				Math.max(0, nodes.length - 1) * levelGap,
		);
	}
	const widestLevel = Math.max(0, ...levelSpans.values());
	const layoutWidth =
		mode === "detail"
			? Math.max(width, orderedTree ? keyed.length * 72 : widestLevel + 72)
			: Math.max(width, 1);
	const layoutHeight =
		mode === "detail"
			? Math.max(height, (maximumDepth + 2) * 90)
			: Math.max(height, 1);
	for (const [nodeDepth, nodes] of levels) {
		nodes.sort(compareNodes);
		let levelX = (layoutWidth - (levelSpans.get(nodeDepth) ?? layoutWidth)) / 2;
		for (const [index, node] of nodes.entries()) {
			const rank = keyRank.get(entityIdKey(node.id));
			const visualWidth = nodeVisualWidth(node);
			const hierarchicalX = levelX + visualWidth / 2;
			levelX += visualWidth + levelGap;
			positions.set(entityIdKey(node.id), {
				x:
					orderedTree && rank !== undefined
						? ((rank + 1) / (keyed.length + 1)) * layoutWidth
						: mode === "detail"
							? hierarchicalX
							: ((index + 1) / (nodes.length + 1)) * layoutWidth,
				y: ((nodeDepth + 1) / (maximumDepth + 2)) * layoutHeight,
			});
		}
	}
	const edges: [string, string][] = [];
	for (const node of graph.nodes) {
		const source = entityIdKey(node.id);
		for (const link of node.links) {
			const target = entityIdKey(link.target);
			if (positions.has(target) && link.role !== "previous") {
				edges.push([source, target]);
			}
		}
	}
	return { edges, mode, nodes: graph.nodes, positions };
}

export function nodeLabel(node: StructureNode): string {
	if (node.keys.length > 0) {
		if (node.role.startsWith("btree-")) {
			const visible =
				node.keys.length <= 12
					? node.keys
					: [...node.keys.slice(0, 8), "…", ...node.keys.slice(-3)];
			return visible.join("  │  ");
		}
		if (node.role.startsWith("veb-")) {
			const role = node.role.replace("veb-", "");
			return `${role}  ${node.keys.join(" · ")}`;
		}
		return node.keys.join(" · ");
	}
	return node.role
		.replace(/^yfast-representative-/, "rep ")
		.replace(/^(xfast|veb)-/, "")
		.slice(0, 18);
}

export function nodeVisualWidth(node: StructureNode): number {
	if (node.role.startsWith("btree-")) {
		return Math.min(560, Math.max(76, nodeLabel(node).length * 7 + 28));
	}
	if (node.id.kind === "auxiliary") {
		return Math.min(220, Math.max(64, nodeLabel(node).length * 6 + 24));
	}
	return 58;
}
