import type {
	FlowCurrentSceneV9,
	FlowEibfsForestArcV1,
	FlowEibfsNodeStateV1,
} from "./flow-scene";

export type FlowEibfsNodeView = FlowEibfsNodeStateV1 & {
	frontier: boolean;
	repairFocus: boolean;
};

export type FlowEibfsForestArcView = FlowEibfsForestArcV1 & {
	residualKey: string;
	displayDirection: "forward" | "reverse";
};

export type FlowEibfsView = {
	phaseDirection: "forward" | "reverse";
	sourceDepth: string;
	sinkDepth: string;
	nodes: ReadonlyMap<string, FlowEibfsNodeView>;
	forestArcs: readonly FlowEibfsForestArcView[];
	sourceForestArcKeys: ReadonlySet<string>;
	sinkForestArcKeys: ReadonlySet<string>;
	repairFocusNodeIds: ReadonlySet<string>;
};

export type FlowEibfsLifecycleStage =
	| "ready"
	| "search"
	| "recovery"
	| "certified";

export function isEibfsAlgorithm(algorithmId: string | undefined): boolean {
	return algorithmId === "eibfs" || algorithmId === "dynamic-eibfs";
}

export function eibfsLifecycleStage(
	scene: FlowCurrentSceneV9,
): FlowEibfsLifecycleStage | undefined {
	if (!isEibfsAlgorithm(scene.algorithm.id)) return undefined;
	if (scene.dynamic_eibfs_overlay !== undefined) {
		switch (scene.dynamic_eibfs_overlay.stage) {
			case "prefix-recovery":
				return "recovery";
			case "prefix-certified":
				return "certified";
			default:
				return "search";
		}
	}
	if (scene.eibfs_overlay !== undefined) return "search";
	if (scene.solve_status === "optimal" || scene.outcome !== undefined) {
		return "certified";
	}
	if (
		scene.trace_event?.catalog_id === "eibfs.begin-feasible-flow-recovery" ||
		scene.trace_event?.catalog_id === "eibfs.cancel-same-cut-positive-flow"
	) {
		return "recovery";
	}
	return "ready";
}

function isRepairEvent(catalogId: string | undefined): boolean {
	return (
		catalogId?.startsWith("eibfs.drain-") === true ||
		catalogId?.startsWith("eibfs.adopt-") === true ||
		catalogId?.startsWith("eibfs.relabel-") === true ||
		catalogId?.startsWith("eibfs.remove-") === true ||
		catalogId?.startsWith("eibfs.migrate-") === true ||
		catalogId?.startsWith("dynamic-eibfs.repair-") === true
	);
}

function opposite(direction: "forward" | "reverse"): "forward" | "reverse" {
	return direction === "forward" ? "reverse" : "forward";
}

export function projectEibfsView(
	scene: FlowCurrentSceneV9,
): FlowEibfsView | undefined {
	if (!isEibfsAlgorithm(scene.algorithm.id)) return undefined;
	if (scene.model.kind !== "max-flow") {
		throw new Error("EIBFS visualization requires a max-flow model");
	}
	const overlay = scene.eibfs_overlay;
	if (overlay === undefined) return undefined;
	const ordered = scene.node_trace_states
		.filter((node) => node.search_ordinal !== undefined)
		.sort(
			(left, right) => (left.search_ordinal ?? 0) - (right.search_ordinal ?? 0),
		)
		.map((node) => node.node_id);
	const repairFocusNodeIds = new Set(
		isRepairEvent(scene.trace_event?.catalog_id) ? ordered.slice(0, 1) : [],
	);
	const nodes = new Map<string, FlowEibfsNodeView>();
	for (const node of overlay.nodes) {
		const frontier =
			!node.orphan &&
			((overlay.phase_direction === "forward" &&
				node.membership === "source" &&
				node.source_label === overlay.source_depth) ||
				(overlay.phase_direction === "reverse" &&
					node.membership === "sink" &&
					node.sink_label === overlay.sink_depth));
		nodes.set(node.node_id, {
			...node,
			frontier,
			repairFocus: repairFocusNodeIds.has(node.node_id),
		});
	}
	const forestArcs = overlay.forest_arcs.map((relation) => {
		const direction = relation.admissible_residual.direction;
		return {
			...relation,
			residualKey: `${relation.admissible_residual.edge_id}:${direction}`,
			displayDirection:
				relation.side === "source" ? direction : opposite(direction),
		};
	});
	return {
		phaseDirection: overlay.phase_direction,
		sourceDepth: overlay.source_depth,
		sinkDepth: overlay.sink_depth,
		nodes,
		forestArcs,
		sourceForestArcKeys: new Set(
			forestArcs
				.filter((relation) => relation.side === "source")
				.map((relation) => relation.residualKey),
		),
		sinkForestArcKeys: new Set(
			forestArcs
				.filter((relation) => relation.side === "sink")
				.map((relation) => relation.residualKey),
		),
		repairFocusNodeIds,
	};
}

export function eibfsNodeLabel(node: FlowEibfsNodeView): string {
	if (node.membership === "free") return "F";
	const distance =
		node.membership === "source" ? node.source_label : node.sink_label;
	const prefix = node.membership === "source" ? "S · dₛ" : "T · dₜ";
	const imbalance = BigInt(node.imbalance);
	const exactImbalance =
		imbalance === 0n ? "" : ` · e ${imbalance > 0n ? "+" : ""}${imbalance}`;
	return `${prefix} ${distance}${exactImbalance}`;
}

export function eibfsRootGlyph(node: FlowEibfsNodeView): string | undefined {
	if (node.root_kind === "source") return "+∞";
	if (node.root_kind === "sink") return "−∞";
	if (node.root_kind === "excess") return "+";
	if (node.root_kind === "deficit") return "−";
	return undefined;
}
