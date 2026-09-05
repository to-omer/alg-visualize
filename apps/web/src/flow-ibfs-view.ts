import type { FlowCurrentSceneV9 } from "./flow-scene";

export type FlowIbfsTreeSide = "source" | "sink";

export type FlowIbfsNodeView = {
	side: FlowIbfsTreeSide;
	distance: number;
	frontier: boolean;
	orphan: boolean;
	repairFocus: boolean;
};

export type FlowIbfsView = {
	nodes: ReadonlyMap<string, FlowIbfsNodeView>;
	repairFocusNodeIds: ReadonlySet<string>;
	sourceForestArcKeys: ReadonlySet<string>;
	sinkForestArcKeys: ReadonlySet<string>;
	sourceDepth: number;
	sinkDepth: number;
	shortestPathLength: string | undefined;
};

const TWO_TREE_ALGORITHMS = new Set(["ibfs", "boykov-kolmogorov"]);

export function isIbfsAlgorithm(algorithmId: string | undefined): boolean {
	return algorithmId !== undefined && TWO_TREE_ALGORITHMS.has(algorithmId);
}

function isRepairEvent(catalogId: string | undefined): boolean {
	return (
		catalogId?.startsWith("ibfs.adopt-") === true ||
		catalogId?.startsWith("ibfs.relabel-") === true ||
		catalogId?.startsWith("ibfs.remove-") === true ||
		catalogId?.startsWith("boykov-kolmogorov.adopt-") === true ||
		catalogId?.startsWith("boykov-kolmogorov.free-") === true
	);
}

function isFrontierEvent(catalogId: string | undefined): boolean {
	return (
		catalogId === "ibfs.start-forward-pass" ||
		catalogId === "ibfs.start-reverse-pass" ||
		catalogId === "ibfs.attach-source-tree" ||
		catalogId === "ibfs.attach-sink-tree" ||
		catalogId === "ibfs.connect-trees" ||
		catalogId === "boykov-kolmogorov.grow-source-tree" ||
		catalogId === "boykov-kolmogorov.grow-sink-tree" ||
		catalogId === "boykov-kolmogorov.connect-trees"
	);
}

export function projectIbfsView(
	scene: FlowCurrentSceneV9,
): FlowIbfsView | undefined {
	if (!isIbfsAlgorithm(scene.algorithm.id)) return undefined;
	if (scene.model.kind !== "max-flow") {
		throw new Error("IBFS visualization requires a max-flow model");
	}

	const eventId = scene.trace_event?.catalog_id;
	const ordered = scene.node_trace_states
		.filter((node) => node.search_ordinal !== undefined)
		.sort(
			(left, right) => (left.search_ordinal ?? 0) - (right.search_ordinal ?? 0),
		)
		.map((node) => node.node_id);
	const repairFocusNodeIds = new Set(
		isRepairEvent(eventId) ? ordered.slice(0, 1) : [],
	);
	const orphanNodes = new Set(
		eventId === "ibfs.augment-shortest-path"
			? ordered
			: eventId === "boykov-kolmogorov.augment"
				? ordered.slice(2)
				: eventId?.startsWith("boykov-kolmogorov.adopt-") === true
					? [ordered[0], ...ordered.slice(2)].filter(
							(node): node is string => node !== undefined,
						)
					: eventId?.startsWith("boykov-kolmogorov.free-") === true
						? ordered
						: isRepairEvent(eventId)
							? ordered.slice(1)
							: [],
	);
	const frontierNodes = new Set(isFrontierEvent(eventId) ? ordered : []);

	const nodes = new Map<string, FlowIbfsNodeView>();
	let sourceDepth = 0;
	let sinkDepth = 0;
	for (const state of scene.node_trace_states) {
		if (state.label === undefined) continue;
		let raw: bigint;
		try {
			raw = BigInt(state.label);
		} catch {
			throw new Error("IBFS contains a non-integral tree label");
		}
		const side: FlowIbfsTreeSide = raw >= 0n ? "source" : "sink";
		const exactDistance = raw >= 0n ? raw : -raw - 1n;
		const distance = Number(exactDistance);
		if (!Number.isSafeInteger(distance) || distance < 0) {
			throw new Error("IBFS tree distance is outside the visualizer domain");
		}
		if (side === "source") sourceDepth = Math.max(sourceDepth, distance);
		else sinkDepth = Math.max(sinkDepth, distance);
		nodes.set(state.node_id, {
			side,
			distance,
			frontier: frontierNodes.has(state.node_id),
			orphan: orphanNodes.has(state.node_id),
			repairFocus: repairFocusNodeIds.has(state.node_id),
		});
	}

	if (nodes.size > 0) {
		const source = nodes.get(scene.model.source);
		const sink = nodes.get(scene.model.sink);
		if (
			source?.side !== "source" ||
			source.distance !== 0 ||
			sink?.side !== "sink" ||
			sink.distance !== 0
		) {
			throw new Error("IBFS roots do not match the declared terminals");
		}
	}

	const residualByKey = new Map(
		scene.residual_arcs.map((arc) => [`${arc.edge_id}:${arc.direction}`, arc]),
	);
	const sourceForestArcKeys = new Set<string>();
	const sinkForestArcKeys = new Set<string>();
	const forestChildren = new Set<string>();
	for (const forestArc of scene.pseudoflow_forest?.arcs ?? []) {
		const key = `${forestArc.edge_id}:${forestArc.direction}`;
		const arc = residualByKey.get(key);
		if (arc === undefined) {
			throw new Error("IBFS forest references a missing residual direction");
		}
		const parent = nodes.get(arc.from);
		const child = nodes.get(arc.to);
		if (
			parent === undefined ||
			child === undefined ||
			parent.side !== child.side ||
			child.distance !== parent.distance + 1
		) {
			throw new Error("IBFS forest arc violates the signed-distance tree");
		}
		if (forestChildren.has(arc.to)) {
			throw new Error("IBFS forest assigns more than one parent to a child");
		}
		forestChildren.add(arc.to);
		const treeResidual =
			child.side === "source"
				? arc
				: residualByKey.get(
						`${forestArc.edge_id}:${forestArc.direction === "forward" ? "reverse" : "forward"}`,
					);
		if (treeResidual === undefined || BigInt(treeResidual.capacity) <= 0n) {
			throw new Error("IBFS forest parent arc is not residual");
		}
		if (child.side === "source") sourceForestArcKeys.add(key);
		else sinkForestArcKeys.add(key);
	}
	if ((scene.pseudoflow_forest?.strong_nodes.length ?? 0) !== 0) {
		throw new Error(
			"Static IBFS must not reuse pseudoflow strong-branch state",
		);
	}

	const shortestPathLength =
		scene.trace_event?.detail?.label === "shortest-path-length"
			? scene.trace_event.detail.value
			: undefined;
	return {
		nodes,
		repairFocusNodeIds,
		sourceForestArcKeys,
		sinkForestArcKeys,
		sourceDepth,
		sinkDepth,
		shortestPathLength,
	};
}

export function ibfsDistanceLabel(node: FlowIbfsNodeView): string {
	return node.side === "source"
		? `S · dₛ ${node.distance}`
		: `T · dₜ ${node.distance}`;
}
