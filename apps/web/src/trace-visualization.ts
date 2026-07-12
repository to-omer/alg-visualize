import {
	type ArenaKey,
	entityIdKey,
	type StatePatchRecord,
	type StructureSnapshot,
	type TraceEvent,
	type TraceKind,
} from "./engine-types";

export type TracePresentation = {
	compareKeys: readonly string[];
	currentEdges: readonly string[];
	currentKeys: readonly string[];
	mutationKeys: readonly string[];
	queryKey: string | undefined;
	queryNodeKey: string | undefined;
	traversalEdge: { key: string; source: string; target: string } | undefined;
	visitedEdges: readonly string[];
	visitedKeys: readonly string[];
};

const MUTATION_KINDS = new Set<TraceKind>([
	"insert",
	"remove",
	"rotate-left",
	"rotate-right",
	"rebuild",
	"split",
	"merge",
	"move-entry",
]);

export function edgeKey(left: string, right: string): string {
	return left < right ? `${left}|${right}` : `${right}|${left}`;
}

function traceNodeKey(event: TraceEvent): string | undefined {
	return event.node === null ? undefined : entityIdKey(event.node);
}

function traceTargetKey(event: TraceEvent): string | undefined {
	return event.target === null ? undefined : entityIdKey(event.target);
}

function latestTraversalEdge(
	trace: readonly TraceEvent[],
	currentEventIndex: number,
): TracePresentation["traversalEdge"] {
	let index =
		trace[currentEventIndex]?.kind === "descend"
			? currentEventIndex
			: currentEventIndex - 1;
	while (index >= 0 && trace[index]?.kind === "descend") {
		const event = trace[index];
		if (event !== undefined) {
			const source = traceNodeKey(event);
			const target = traceTargetKey(event);
			if (source !== undefined && target !== undefined && source !== target) {
				return { key: edgeKey(source, target), source, target };
			}
		}
		index -= 1;
	}
	return undefined;
}

type OwnershipIndex = {
	entries: Map<string, string>;
	keys: Map<string, string>;
	links: Map<string, string[]>;
};

const ownershipCache = new WeakMap<StructureSnapshot, OwnershipIndex>();

function ownershipIndex(structure: StructureSnapshot): OwnershipIndex {
	const cached = ownershipCache.get(structure);
	if (cached !== undefined) {
		return cached;
	}
	const entries = new Map<string, string>();
	const keys = new Map<string, string>();
	const links = new Map<string, string[]>();
	for (const node of structure.nodes) {
		const owner = entityIdKey(node.id);
		for (const entry of node.entries) {
			entries.set(`${entry.index}:${entry.generation}`, owner);
		}
		for (const key of node.keys) {
			if (!keys.has(key)) {
				keys.set(key, owner);
			}
		}
		links.set(
			owner,
			node.links
				.filter((link) => link.role !== "previous")
				.map((link) => entityIdKey(link.target)),
		);
	}
	const index = { entries, keys, links };
	ownershipCache.set(structure, index);
	return index;
}

type VisitationState = {
	index: number;
	visitedEdges: Set<string>;
	visitedKeys: Set<string>;
};

const visitationCache = new WeakMap<
	readonly TraceEvent[],
	WeakMap<StructureSnapshot, VisitationState>
>();

function visitationThrough(
	structure: StructureSnapshot,
	trace: readonly TraceEvent[],
	currentEventIndex: number,
	owners: OwnershipIndex,
): { visitedEdges: Set<string>; visitedKeys: Set<string> } {
	let byStructure = visitationCache.get(trace);
	if (byStructure === undefined) {
		byStructure = new WeakMap();
		visitationCache.set(trace, byStructure);
	}
	let state = byStructure.get(structure);
	if (state === undefined || currentEventIndex < state.index) {
		state = {
			index: -1,
			visitedEdges: new Set(),
			visitedKeys: new Set(),
		};
		byStructure.set(structure, state);
	}
	for (let index = state.index + 1; index <= currentEventIndex; index += 1) {
		const event = trace[index];
		if (event === undefined) continue;
		const node = traceNodeKey(event);
		const target = traceTargetKey(event);
		const owner =
			event.entry === null
				? undefined
				: owners.entries.get(`${event.entry.index}:${event.entry.generation}`);
		const keyNode =
			event.kind === "compare" || event.key === null
				? undefined
				: owners.keys.get(event.key);
		addDefined(state.visitedKeys, node, target, owner, keyNode);
		if (node !== undefined && target !== undefined && node !== target) {
			state.visitedEdges.add(edgeKey(node, target));
		}
	}
	state.index = currentEventIndex;
	for (const source of state.visitedKeys) {
		for (const target of owners.links.get(source) ?? []) {
			if (state.visitedKeys.has(target)) {
				state.visitedEdges.add(edgeKey(source, target));
			}
		}
	}
	return {
		visitedEdges: new Set(state.visitedEdges),
		visitedKeys: new Set(state.visitedKeys),
	};
}

function addDefined(target: Set<string>, ...values: (string | undefined)[]) {
	for (const value of values) {
		if (value !== undefined) {
			target.add(value);
		}
	}
}

function structuralEdges(
	node: StatePatchRecord & { kind: "node" },
	side: "before" | "after",
): Set<string> {
	const value = node[side];
	if (value === null) {
		return new Set();
	}
	const source = entityIdKey(value.id);
	return new Set(
		value.links
			.filter((link) => link.role !== "previous")
			.map((link) => edgeKey(source, entityIdKey(link.target))),
	);
}

function mutationEffects(
	event: TraceEvent,
	patches: readonly StatePatchRecord[] | undefined,
): { edges: Set<string>; keys: Set<string> } {
	const edges = new Set<string>();
	const keys = new Set<string>();
	if (patches === undefined || !MUTATION_KINDS.has(event.kind)) {
		return { edges, keys };
	}
	const end = event.patch_start + event.patch_count;
	for (const patch of patches.slice(event.patch_start, end)) {
		if (patch.kind === "root") {
			addDefined(
				keys,
				patch.before === null ? undefined : entityIdKey(patch.before),
				patch.after === null ? undefined : entityIdKey(patch.after),
			);
			continue;
		}
		if (patch.kind !== "node") {
			continue;
		}
		keys.add(entityIdKey(patch.id));
		const before = structuralEdges(patch, "before");
		const after = structuralEdges(patch, "after");
		for (const candidate of before) {
			if (!after.has(candidate)) edges.add(candidate);
		}
		for (const candidate of after) {
			if (!before.has(candidate)) edges.add(candidate);
		}
	}
	return { edges, keys };
}

export function buildTracePresentation(
	structure: StructureSnapshot | undefined,
	trace: readonly TraceEvent[] | undefined,
	currentEventIndex: number | undefined,
	patches: readonly StatePatchRecord[] | undefined = undefined,
): TracePresentation {
	const empty: TracePresentation = {
		compareKeys: [],
		currentEdges: [],
		currentKeys: [],
		mutationKeys: [],
		queryKey: undefined,
		queryNodeKey: undefined,
		traversalEdge: undefined,
		visitedEdges: [],
		visitedKeys: [],
	};
	if (
		structure === undefined ||
		trace === undefined ||
		currentEventIndex === undefined ||
		currentEventIndex < 0 ||
		currentEventIndex >= trace.length
	) {
		return empty;
	}

	const current = trace[currentEventIndex];
	if (current === undefined) {
		return empty;
	}
	const owners = ownershipIndex(structure);
	const entryOwner = (entry: ArenaKey | null) =>
		entry === null
			? undefined
			: owners.entries.get(`${entry.index}:${entry.generation}`);
	const keyOwner = (key: string | null) =>
		key === null ? undefined : owners.keys.get(key);
	const { visitedEdges, visitedKeys } = visitationThrough(
		structure,
		trace,
		currentEventIndex,
		owners,
	);

	const node = traceNodeKey(current);
	const target = traceTargetKey(current);
	const owner = entryOwner(current.entry);
	const eventKeyNode =
		current.kind === "compare" ? undefined : keyOwner(current.key);
	const queryNode =
		current.kind === "compare" ? keyOwner(current.key) : undefined;
	const currentKeys = new Set<string>();
	addDefined(currentKeys, node, target, owner, eventKeyNode);
	if (current.kind === "compare") {
		addDefined(currentKeys, queryNode);
	}
	const effects = mutationEffects(current, patches);
	for (const key of effects.keys) currentKeys.add(key);
	const compareKeys =
		current.kind === "compare" ? [...currentKeys] : ([] as string[]);
	const mutationKeys = MUTATION_KINDS.has(current.kind) ? [...currentKeys] : [];
	const currentEdges = effects.edges;
	const traversalEdge = latestTraversalEdge(trace, currentEventIndex);

	if (
		(current.kind === "rotate-left" || current.kind === "rotate-right") &&
		node !== undefined &&
		owner !== undefined &&
		node !== owner
	) {
		currentEdges.add(edgeKey(node, owner));
	}
	if (traversalEdge !== undefined) {
		currentEdges.add(traversalEdge.key);
		visitedEdges.add(traversalEdge.key);
	}

	return {
		compareKeys,
		currentEdges: [...currentEdges],
		currentKeys: [...currentKeys],
		mutationKeys,
		queryKey:
			current.kind === "compare" ? (current.key ?? undefined) : undefined,
		queryNodeKey: queryNode,
		traversalEdge,
		visitedEdges: [...visitedEdges],
		visitedKeys: [...visitedKeys],
	};
}
