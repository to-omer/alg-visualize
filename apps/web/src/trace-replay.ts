import {
	type ArenaKey,
	type CanonicalEntry,
	type CanonicalSnapshot,
	type CommitFrame,
	type CurrentFrame,
	type EngineFrame,
	entityIdKey,
	type MetricOrdinal,
	type Metrics,
	type StatePatchRecord,
	type StructureEntityId,
	type StructureNode,
	type StructureSnapshot,
	type TraceEvent,
} from "./engine-types";

const METRIC_FIELD: Record<MetricOrdinal, keyof Metrics> = {
	comparisons: "comparisons",
	"node-visits": "node_visits",
	"bit-tests": "bit_tests",
	rotations: "rotations",
	recolors: "recolors",
	splits: "splits",
	merges: "merges",
	"rebuild-items": "rebuild_items",
	allocations: "allocations",
	frees: "frees",
};

const METRIC_ORDER = new Map(
	Object.keys(METRIC_FIELD).map((ordinal, index) => [ordinal, index]),
);

type Direction = "forward" | "reverse";
const PREVALIDATED_REPLAY = Symbol("prevalidated-replay");

type PreparedPatch = {
	entries: Map<string, CanonicalEntry | null>;
	metricChanges: [keyof Metrics, string][];
	nextRoot: StructureEntityId | null;
	nodes: Map<string, StructureNode | null>;
};

export class TraceReplayError extends Error {
	constructor(message: string) {
		super(message);
		this.name = "TraceReplayError";
	}
}

class IndexedTraceState {
	root: StructureEntityId | null;
	readonly nodes: Map<string, StructureNode>;
	readonly entries: Map<string, CanonicalEntry>;
	readonly metrics: Metrics;
	private readonly incoming = new Map<string, Set<string>>();
	private readonly entryOwners = new Map<string, Set<string>>();
	private readonly logicalKeys = new Map<string, string>();
	private structureCache: StructureSnapshot | undefined;
	private canonicalCache: CanonicalSnapshot | undefined;
	private entriesCache: CanonicalEntry[] | undefined;

	constructor(
		structure: StructureSnapshot,
		canonical: CanonicalSnapshot,
		populate = true,
	) {
		this.root = structure.root;
		this.nodes = new Map();
		this.entries = new Map();
		this.metrics = { ...canonical.metrics };
		if (!populate) return;
		for (const node of structure.nodes) {
			const key = entityIdKey(node.id);
			if (this.nodes.has(key)) {
				throw new TraceReplayError(
					"base state has duplicate structural identity",
				);
			}
			this.nodes.set(key, node);
			this.addNodeIndexes(key, node);
		}
		for (const entry of canonical.entries) {
			const key = arenaKey(entry.id);
			if (this.entries.has(key)) {
				throw new TraceReplayError("base state has duplicate entry identity");
			}
			this.entries.set(key, entry);
			if (this.logicalKeys.has(entry.key)) {
				throw new TraceReplayError("base state has duplicate canonical key");
			}
			this.logicalKeys.set(entry.key, key);
		}
	}

	static async create(
		structure: StructureSnapshot,
		canonical: CanonicalSnapshot,
	) {
		const state = new IndexedTraceState(structure, canonical, false);
		for (let start = 0; start < structure.nodes.length; start += 1_024) {
			for (
				let index = start;
				index < Math.min(start + 1_024, structure.nodes.length);
				index += 1
			) {
				const node = structure.nodes[index];
				if (node === undefined) continue;
				const key = entityIdKey(node.id);
				if (state.nodes.has(key)) {
					throw new TraceReplayError(
						"base state has duplicate structural identity",
					);
				}
				state.nodes.set(key, node);
				state.addNodeIndexes(key, node);
			}
			if (start + 1_024 < structure.nodes.length) {
				await yieldToMainThread();
			}
		}
		for (let start = 0; start < canonical.entries.length; start += 1_024) {
			for (
				let index = start;
				index < Math.min(start + 1_024, canonical.entries.length);
				index += 1
			) {
				const entry = canonical.entries[index];
				if (entry === undefined) continue;
				const key = arenaKey(entry.id);
				if (state.entries.has(key)) {
					throw new TraceReplayError("base state has duplicate entry identity");
				}
				state.entries.set(key, entry);
				if (state.logicalKeys.has(entry.key)) {
					throw new TraceReplayError("base state has duplicate canonical key");
				}
				state.logicalKeys.set(entry.key, key);
			}
			if (start + 1_024 < canonical.entries.length) {
				await yieldToMainThread();
			}
		}
		state.structureCache = structure;
		state.entriesCache = canonical.entries;
		state.canonicalCache = canonical;
		return state;
	}

	apply(records: readonly StatePatchRecord[], direction: Direction) {
		const prepared = this.preparePatch(records, direction);
		this.validateChangedReferences(
			prepared.nodes,
			prepared.entries,
			prepared.nextRoot,
		);
		this.commitPatch(prepared);
	}

	async applyAsync(records: readonly StatePatchRecord[], direction: Direction) {
		const prepared = await consumeStepsAsync(
			this.preparePatchSteps(records, direction),
		);
		await consumeStepsAsync(
			this.validateChangedReferenceSteps(
				prepared.nodes,
				prepared.entries,
				prepared.nextRoot,
			),
		);
		await consumeStepsAsync(this.commitPatchSteps(prepared));
	}

	private preparePatch(
		records: readonly StatePatchRecord[],
		direction: Direction,
	): PreparedPatch {
		return consumeSteps(this.preparePatchSteps(records, direction));
	}

	private *preparePatchSteps(
		records: readonly StatePatchRecord[],
		direction: Direction,
	): Generator<void, PreparedPatch> {
		const nodes = new Map<string, StructureNode | null>();
		const entries = new Map<string, CanonicalEntry | null>();
		let nextRoot = this.root;
		const metricChanges: [keyof Metrics, string][] = [];

		let previousOrder = "";
		let processed = 0;
		for (const record of records) {
			const currentOrder = patchOrderKey(record);
			if (previousOrder >= currentOrder) {
				throw new TraceReplayError("state patch order is not canonical");
			}
			previousOrder = currentOrder;
			switch (record.kind) {
				case "root": {
					const expected =
						direction === "forward" ? record.before : record.after;
					if (!sameEntity(this.root, expected)) {
						throw new TraceReplayError("root patch precondition mismatch");
					}
					nextRoot = direction === "forward" ? record.after : record.before;
					break;
				}
				case "node": {
					const key = entityIdKey(record.id);
					validateNodeIdentity(record.id, record.before);
					validateNodeIdentity(record.id, record.after);
					const expected =
						direction === "forward" ? record.before : record.after;
					const current = this.nodes.get(key);
					if (!sameNode(current, expected)) {
						throw new TraceReplayError(
							`node patch precondition mismatch for ${key}: ${nodeDifference(current, expected)}`,
						);
					}
					nodes.set(
						key,
						direction === "forward" ? record.after : record.before,
					);
					break;
				}
				case "entry": {
					const key = arenaKey(record.id);
					validateEntryIdentity(record.id, record.before);
					validateEntryIdentity(record.id, record.after);
					const expected =
						direction === "forward" ? record.before : record.after;
					if (!sameEntry(this.entries.get(key), expected)) {
						throw new TraceReplayError("entry patch precondition mismatch");
					}
					entries.set(
						key,
						direction === "forward" ? record.after : record.before,
					);
					break;
				}
				case "metric": {
					if (compareDecimal(record.after, record.before) <= 0) {
						throw new TraceReplayError("metric patch is not monotonic");
					}
					const field = METRIC_FIELD[record.ordinal];
					const expected =
						direction === "forward" ? record.before : record.after;
					if (this.metrics[field] !== expected) {
						throw new TraceReplayError("metric patch precondition mismatch");
					}
					metricChanges.push([
						field,
						direction === "forward" ? record.after : record.before,
					]);
					break;
				}
			}
			processed += 1;
			if (processed % 1_024 === 0) yield;
		}
		return { entries, metricChanges, nextRoot, nodes };
	}

	private commitPatch({
		entries,
		metricChanges,
		nextRoot,
		nodes,
	}: PreparedPatch) {
		consumeSteps(
			this.commitPatchSteps({ entries, metricChanges, nextRoot, nodes }),
		);
	}

	private *commitPatchSteps({
		entries,
		metricChanges,
		nextRoot,
		nodes,
	}: PreparedPatch): Generator<void, void> {
		const rootChanged = !sameEntity(nextRoot, this.root);
		this.root = nextRoot;
		let processed = 0;
		for (const [key, node] of nodes) {
			const before = this.nodes.get(key);
			if (before !== undefined) this.removeNodeIndexes(key, before);
			if (node === null) {
				this.nodes.delete(key);
			} else {
				this.nodes.set(key, node);
				this.addNodeIndexes(key, node);
			}
			processed += 1;
			if (processed % 1_024 === 0) yield;
		}
		for (const [key, entry] of entries) {
			const before = this.entries.get(key);
			if (before !== undefined) this.logicalKeys.delete(before.key);
			if (entry === null) {
				this.entries.delete(key);
			} else {
				this.entries.set(key, entry);
				this.logicalKeys.set(entry.key, key);
			}
			processed += 1;
			if (processed % 1_024 === 0) yield;
		}
		for (const [field, value] of metricChanges) {
			this.metrics[field] = value;
		}
		if (nodes.size > 0 || rootChanged) {
			this.structureCache = undefined;
		}
		if (entries.size > 0 || metricChanges.length > 0) {
			this.canonicalCache = undefined;
		}
		if (entries.size > 0) {
			this.entriesCache = undefined;
		}
	}

	structureSnapshot(): StructureSnapshot {
		this.structureCache ??= {
			root: this.root,
			nodes: [...this.nodes.values()].sort((left, right) =>
				compareEntity(left.id, right.id),
			),
		};
		return this.structureCache;
	}

	canonicalSnapshot(): CanonicalSnapshot {
		this.entriesCache ??= [...this.entries.values()].sort((left, right) =>
			compareDecimal(left.key, right.key),
		);
		this.canonicalCache ??= {
			entries: this.entriesCache,
			metrics: { ...this.metrics },
		};
		return this.canonicalCache;
	}

	private addNodeIndexes(source: string, node: StructureNode) {
		for (const link of node.links) {
			const target = entityIdKey(link.target);
			const sources = this.incoming.get(target) ?? new Set<string>();
			sources.add(source);
			this.incoming.set(target, sources);
		}
		for (const entry of node.entries) {
			const target = arenaKey(entry);
			const owners = this.entryOwners.get(target) ?? new Set<string>();
			owners.add(source);
			this.entryOwners.set(target, owners);
		}
	}

	private removeNodeIndexes(source: string, node: StructureNode) {
		for (const link of node.links) {
			removeIndexValue(this.incoming, entityIdKey(link.target), source);
		}
		for (const entry of node.entries) {
			removeIndexValue(this.entryOwners, arenaKey(entry), source);
		}
	}

	private validateChangedReferences(
		nodeChanges: ReadonlyMap<string, StructureNode | null>,
		entryChanges: ReadonlyMap<string, CanonicalEntry | null>,
		root: StructureEntityId | null,
	) {
		consumeSteps(
			this.validateChangedReferenceSteps(nodeChanges, entryChanges, root),
		);
	}

	private *validateChangedReferenceSteps(
		nodeChanges: ReadonlyMap<string, StructureNode | null>,
		entryChanges: ReadonlyMap<string, CanonicalEntry | null>,
		root: StructureEntityId | null,
	): Generator<void, void> {
		const nodeAfter = (key: string) =>
			nodeChanges.has(key) ? nodeChanges.get(key) : this.nodes.get(key);
		const entryAfter = (key: string) =>
			entryChanges.has(key) ? entryChanges.get(key) : this.entries.get(key);
		if (root !== null && nodeAfter(entityIdKey(root)) == null) {
			throw new TraceReplayError("root references a missing entity");
		}
		const affectedTargets = new Set<string>();
		if (!sameEntity(root, this.root)) {
			if (this.root !== null) affectedTargets.add(entityIdKey(this.root));
			if (root !== null) affectedTargets.add(entityIdKey(root));
		}
		const affectedEntries = new Set<string>();
		const incomingAfter = new Map<string, Set<string>>();
		const ownersAfter = new Map<string, Set<string>>();
		const incomingFor = (target: string) => {
			let sources = incomingAfter.get(target);
			if (sources === undefined) {
				sources = new Set(this.incoming.get(target) ?? []);
				incomingAfter.set(target, sources);
			}
			return sources;
		};
		const ownersFor = (entry: string) => {
			let owners = ownersAfter.get(entry);
			if (owners === undefined) {
				owners = new Set(this.entryOwners.get(entry) ?? []);
				ownersAfter.set(entry, owners);
			}
			return owners;
		};
		let processed = 0;
		for (const [source, after] of nodeChanges) {
			affectedTargets.add(source);
			const before = this.nodes.get(source);
			if (before !== undefined) {
				for (const link of before.links) {
					const target = entityIdKey(link.target);
					affectedTargets.add(target);
					incomingFor(target).delete(source);
				}
				for (const entry of before.entries) {
					const target = arenaKey(entry);
					affectedEntries.add(target);
					ownersFor(target).delete(source);
				}
			}
			if (after == null) continue;
			for (const link of after.links) {
				const target = entityIdKey(link.target);
				affectedTargets.add(target);
				incomingFor(target).add(source);
				if (nodeAfter(target) == null) {
					throw new TraceReplayError("link references a missing entity");
				}
			}
			for (const entry of after.entries) {
				const target = arenaKey(entry);
				affectedEntries.add(target);
				ownersFor(target).add(source);
				if (entryAfter(target) == null) {
					throw new TraceReplayError("node references a missing entry");
				}
			}
			processed += 1;
			if (processed % 1_024 === 0) yield;
		}
		const affectedPrimary = new Set<string>();
		for (const key of affectedTargets) {
			const node = nodeAfter(key);
			const incoming = incomingFor(key);
			if (node == null && incoming.size > 0) {
				throw new TraceReplayError("link references a missing entity");
			}
			if (node?.id.kind === "node") affectedPrimary.add(key);
			processed += 1;
			if (processed % 1_024 === 0) yield;
		}
		yield* this.validateAffectedReachabilitySteps(
			affectedPrimary,
			root,
			nodeAfter,
			incomingFor,
		);
		const changedLogicalKeys = new Map<string, string>();
		for (const [key, entry] of entryChanges) {
			affectedEntries.add(key);
			if (entry !== null) {
				const changedOwner = changedLogicalKeys.get(entry.key);
				const existingOwner = this.logicalKeys.get(entry.key);
				if (
					(changedOwner !== undefined && changedOwner !== key) ||
					(existingOwner !== undefined &&
						existingOwner !== key &&
						!entryChanges.has(existingOwner))
				) {
					throw new TraceReplayError("canonical state has a duplicate key");
				}
				changedLogicalKeys.set(entry.key, key);
			}
			processed += 1;
			if (processed % 1_024 === 0) yield;
		}
		for (const key of affectedEntries) {
			const entry = entryAfter(key);
			if (entry == null && ownersFor(key).size > 0) {
				throw new TraceReplayError("node references a missing entry");
			}
			processed += 1;
			if (processed % 1_024 === 0) yield;
		}
	}

	private *validateAffectedReachabilitySteps(
		affected: ReadonlySet<string>,
		root: StructureEntityId | null,
		nodeAfter: (key: string) => StructureNode | null | undefined,
		incomingFor: (key: string) => Set<string>,
	): Generator<void, void> {
		if (affected.size === 0) return;
		const rootKey = root === null ? undefined : entityIdKey(root);
		const closure = new Set<string>();
		const pending: string[] = [];
		let processed = 0;
		for (const key of affected) {
			closure.add(key);
			pending.push(key);
			processed += 1;
			if (processed % 1_024 === 0) yield;
		}
		const outgoing = new Map<string, Set<string>>();
		while (pending.length > 0) {
			const target = pending.pop();
			if (target === undefined) continue;
			for (const source of incomingFor(target)) {
				const targets = outgoing.get(source) ?? new Set<string>();
				targets.add(target);
				outgoing.set(source, targets);
				if (!closure.has(source) && nodeAfter(source) != null) {
					closure.add(source);
					pending.push(source);
				}
			}
			processed += 1;
			if (processed % 1_024 === 0) yield;
		}
		const reachable = new Set<string>();
		const forward = rootKey === undefined ? [] : [rootKey];
		while (forward.length > 0) {
			const source = forward.pop();
			if (source === undefined || reachable.has(source)) continue;
			reachable.add(source);
			for (const target of outgoing.get(source) ?? []) forward.push(target);
			processed += 1;
			if (processed % 1_024 === 0) yield;
		}
		for (const key of affected) {
			if (!reachable.has(key)) {
				throw new TraceReplayError("primary node is unreachable from root");
			}
			processed += 1;
			if (processed % 1_024 === 0) yield;
		}
	}
}

export class TraceReplayController {
	readonly commit: CommitFrame;
	private readonly state: IndexedTraceState;
	private movement: Promise<void> = Promise.resolve();
	private pendingMovements = 0;
	private rawEventIndex = -1;
	private released = false;

	constructor(
		base: CurrentFrame,
		commit: CommitFrame,
		state = new IndexedTraceState(base.structure, base.canonical),
		prevalidated?: typeof PREVALIDATED_REPLAY,
	) {
		if (commit.baseItemIndex !== base.itemIndex) {
			throw new TraceReplayError(
				"commit base item does not match current state",
			);
		}
		this.commit = commit;
		if (prevalidated === PREVALIDATED_REPLAY) {
			this.state = state;
			return;
		}
		let patchOffset = 0;
		for (const event of commit.trace) {
			if (event.patch_start !== patchOffset) {
				throw new TraceReplayError("trace patch spans are not contiguous");
			}
			const records = eventRecords(commit, event);
			state.apply(records, "forward");
			patchOffset += records.length;
		}
		if (patchOffset !== commit.patches.length) {
			throw new TraceReplayError("trace patch spans do not cover all patches");
		}
		for (let index = commit.trace.length - 1; index >= 0; index -= 1) {
			const event = commit.trace[index];
			if (event !== undefined) {
				state.apply(eventRecords(commit, event), "reverse");
			}
		}
		this.state = state;
	}

	static async create(
		base: CurrentFrame,
		commit: CommitFrame,
		previous?: TraceReplayController,
	) {
		if (commit.baseItemIndex !== base.itemIndex) {
			throw new TraceReplayError(
				"commit base item does not match current state",
			);
		}
		if (commit.patches.length >= 1_024 || commit.trace.length >= 1_024) {
			const isolated = await IndexedTraceState.create(
				base.structure,
				base.canonical,
			);
			await prepareReplayState(isolated, commit);
			return new TraceReplayController(
				base,
				commit,
				isolated,
				PREVALIDATED_REPLAY,
			);
		}
		const transferred = previous?.releaseFinalState(base.itemIndex);
		if (transferred !== undefined) {
			return new TraceReplayController(base, commit, transferred);
		}
		const state = await IndexedTraceState.create(
			base.structure,
			base.canonical,
		);
		return new TraceReplayController(base, commit, state);
	}

	currentRawEventIndex(): number {
		return this.rawEventIndex;
	}

	moveTo(rawEventIndex: number): EngineFrame {
		if (this.released) {
			throw new TraceReplayError("trace state has moved to the next operation");
		}
		if (rawEventIndex < -1 || rawEventIndex >= this.commit.trace.length) {
			throw new TraceReplayError("trace event index is outside its bounds");
		}
		while (this.rawEventIndex < rawEventIndex) {
			const next = this.rawEventIndex + 1;
			const event = this.commit.trace[next];
			if (event === undefined) {
				throw new TraceReplayError("trace event disappeared during replay");
			}
			this.state.apply(eventRecords(this.commit, event), "forward");
			this.rawEventIndex = next;
		}
		while (this.rawEventIndex > rawEventIndex) {
			const event = this.commit.trace[this.rawEventIndex];
			if (event === undefined) {
				throw new TraceReplayError(
					"trace event disappeared during reverse replay",
				);
			}
			this.state.apply(eventRecords(this.commit, event), "reverse");
			this.rawEventIndex -= 1;
		}
		return this.frame();
	}

	moveToAsync(rawEventIndex: number): Promise<EngineFrame> {
		this.pendingMovements += 1;
		const movement = this.movement.then(() =>
			this.moveToAsyncImmediately(rawEventIndex),
		);
		this.movement = movement.then(
			() => undefined,
			() => undefined,
		);
		return movement.finally(() => {
			this.pendingMovements -= 1;
		});
	}

	private async moveToAsyncImmediately(
		rawEventIndex: number,
	): Promise<EngineFrame> {
		if (this.released) {
			throw new TraceReplayError("trace state has moved to the next operation");
		}
		if (rawEventIndex < -1 || rawEventIndex >= this.commit.trace.length) {
			throw new TraceReplayError("trace event index is outside its bounds");
		}
		while (this.rawEventIndex < rawEventIndex) {
			const next = this.rawEventIndex + 1;
			const event = this.commit.trace[next];
			if (event === undefined) {
				throw new TraceReplayError("trace event disappeared during replay");
			}
			const records = eventRecords(this.commit, event);
			if (records.length >= 1_024)
				await this.state.applyAsync(records, "forward");
			else this.state.apply(records, "forward");
			this.rawEventIndex = next;
		}
		while (this.rawEventIndex > rawEventIndex) {
			const event = this.commit.trace[this.rawEventIndex];
			if (event === undefined) {
				throw new TraceReplayError(
					"trace event disappeared during reverse replay",
				);
			}
			const records = eventRecords(this.commit, event);
			if (records.length >= 1_024)
				await this.state.applyAsync(records, "reverse");
			else this.state.apply(records, "reverse");
			this.rawEventIndex -= 1;
		}
		return this.frame();
	}

	private releaseFinalState(itemIndex: number) {
		if (
			this.released ||
			this.pendingMovements > 0 ||
			this.commit.itemIndex !== itemIndex ||
			this.rawEventIndex !== this.commit.trace.length - 1
		) {
			return undefined;
		}
		this.released = true;
		return this.state;
	}

	frame(): EngineFrame {
		const event = this.commit.trace[this.rawEventIndex];
		const frame: EngineFrame = {
			...this.commit,
			structure: this.state.structureSnapshot(),
			canonical: this.state.canonicalSnapshot(),
		};
		if (event?.kind !== "result") {
			delete frame.result;
		}
		return frame;
	}
}

async function prepareReplayState(
	state: IndexedTraceState,
	commit: CommitFrame,
) {
	let patchOffset = 0;
	let recordsSinceYield = 0;
	let sliceStartedAt = performance.now();
	const yieldIfNeeded = async () => {
		if (recordsSinceYield < 1_024 && performance.now() - sliceStartedAt < 8) {
			return;
		}
		await yieldToMainThread();
		recordsSinceYield = 0;
		sliceStartedAt = performance.now();
	};
	for (const event of commit.trace) {
		if (event.patch_start !== patchOffset) {
			throw new TraceReplayError("trace patch spans are not contiguous");
		}
		const records = eventRecords(commit, event);
		if (records.length >= 1_024) await state.applyAsync(records, "forward");
		else state.apply(records, "forward");
		patchOffset += records.length;
		recordsSinceYield += records.length;
		await yieldIfNeeded();
	}
	if (patchOffset !== commit.patches.length) {
		throw new TraceReplayError("trace patch spans do not cover all patches");
	}
	for (let index = commit.trace.length - 1; index >= 0; index -= 1) {
		const event = commit.trace[index];
		if (event === undefined) continue;
		const records = eventRecords(commit, event);
		if (records.length >= 1_024) await state.applyAsync(records, "reverse");
		else state.apply(records, "reverse");
		recordsSinceYield += records.length;
		await yieldIfNeeded();
	}
}

function eventRecords(
	commit: CommitFrame,
	event: TraceEvent,
): readonly StatePatchRecord[] {
	const end = event.patch_start + event.patch_count;
	if (
		!Number.isSafeInteger(end) ||
		event.patch_start < 0 ||
		event.patch_count < 0 ||
		end > commit.patches.length
	) {
		throw new TraceReplayError("trace patch span is outside its bounds");
	}
	return commit.patches.slice(event.patch_start, end);
}

function yieldToMainThread() {
	return new Promise<void>((resolve) => setTimeout(resolve, 0));
}

function consumeSteps<T>(steps: Generator<void, T>): T {
	let step = steps.next();
	while (!step.done) step = steps.next();
	return step.value;
}

async function consumeStepsAsync<T>(steps: Generator<void, T>): Promise<T> {
	let step = steps.next();
	while (!step.done) {
		await yieldToMainThread();
		step = steps.next();
	}
	return step.value;
}

function removeIndexValue(
	index: Map<string, Set<string>>,
	key: string,
	value: string,
) {
	const values = index.get(key);
	if (values === undefined) return;
	values.delete(value);
	if (values.size === 0) index.delete(key);
}

function patchOrderKey(record: StatePatchRecord): string {
	switch (record.kind) {
		case "root":
			return "0";
		case "node":
			return `1:${entityOrderKey(record.id)}`;
		case "entry":
			return `2:${arenaOrderKey(record.id)}`;
		case "metric":
			return `3:${String(METRIC_ORDER.get(record.ordinal) ?? -1).padStart(2, "0")}`;
	}
}

function arenaKey(id: ArenaKey): string {
	return `${id.index}:${id.generation}`;
}

function arenaOrderKey(id: ArenaKey): string {
	return `${String(id.index).padStart(10, "0")}:${String(id.generation).padStart(10, "0")}`;
}

function entityOrderKey(id: StructureEntityId): string {
	return `${id.kind === "node" ? "0" : "1"}:${arenaOrderKey(id.id)}`;
}

function compareEntity(
	left: StructureEntityId,
	right: StructureEntityId,
): number {
	return entityOrderKey(left).localeCompare(entityOrderKey(right));
}

function compareDecimal(left: string, right: string): number {
	const normalizedLeft = left.replace(/^0+(?=\d)/, "");
	const normalizedRight = right.replace(/^0+(?=\d)/, "");
	return (
		normalizedLeft.length - normalizedRight.length ||
		normalizedLeft.localeCompare(normalizedRight)
	);
}

function sameArena(left: ArenaKey, right: ArenaKey): boolean {
	return left.index === right.index && left.generation === right.generation;
}

function sameEntity(
	left: StructureEntityId | null | undefined,
	right: StructureEntityId | null | undefined,
): boolean {
	return (
		left === right ||
		(left != null &&
			right != null &&
			left.kind === right.kind &&
			sameArena(left.id, right.id))
	);
}

function sameEntry(
	left: CanonicalEntry | null | undefined,
	right: CanonicalEntry | null | undefined,
): boolean {
	if (left == null || right == null) {
		return left == null && right == null;
	}
	return (
		left === right ||
		(sameArena(left.id, right.id) &&
			left.key === right.key &&
			left.value === right.value)
	);
}

function sameNode(
	left: StructureNode | null | undefined,
	right: StructureNode | null | undefined,
): boolean {
	if (left == null || right == null) {
		return left == null && right == null;
	}
	return (
		left === right ||
		(sameEntity(left.id, right.id) &&
			left.role === right.role &&
			left.entries.length === right.entries.length &&
			left.entries.every((entry, index) => {
				const candidate = right.entries[index];
				return candidate !== undefined && sameArena(entry, candidate);
			}) &&
			left.keys.length === right.keys.length &&
			left.keys.every((key, index) => key === right.keys[index]) &&
			left.links.length === right.links.length &&
			left.links.every((link, index) => {
				const candidate = right.links[index];
				return (
					candidate !== undefined &&
					link.slot === candidate.slot &&
					link.role === candidate.role &&
					sameEntity(link.target, candidate.target)
				);
			}) &&
			left.metadata.length === right.metadata.length &&
			left.metadata.every((metadata, index) => {
				const candidate = right.metadata[index];
				return (
					candidate !== undefined &&
					metadata[0] === candidate[0] &&
					metadata[1] === candidate[1]
				);
			}))
	);
}

function nodeDifference(
	current: StructureNode | null | undefined,
	expected: StructureNode | null | undefined,
): string {
	if (current == null || expected == null) {
		return `current=${current === undefined ? "missing" : "null"}, expected=${expected === undefined ? "missing" : "null"}`;
	}
	if (!sameEntity(current.id, expected.id)) return "identity differs";
	if (current.role !== expected.role) return "role differs";
	if (JSON.stringify(current.entries) !== JSON.stringify(expected.entries)) {
		return "entries differ";
	}
	if (JSON.stringify(current.keys) !== JSON.stringify(expected.keys)) {
		return "keys differ";
	}
	if (JSON.stringify(current.links) !== JSON.stringify(expected.links)) {
		return `links differ: current=${JSON.stringify(current.links)}, expected=${JSON.stringify(expected.links)}`;
	}
	if (JSON.stringify(current.metadata) !== JSON.stringify(expected.metadata)) {
		return `metadata differs: current=${JSON.stringify(current.metadata)}, expected=${JSON.stringify(expected.metadata)}`;
	}
	return "unknown field differs";
}

function validateNodeIdentity(
	id: StructureEntityId,
	node: StructureNode | null,
) {
	if (node !== null && !sameEntity(id, node.id)) {
		throw new TraceReplayError("node patch identity mismatch");
	}
}

function validateEntryIdentity(id: ArenaKey, entry: CanonicalEntry | null) {
	if (entry !== null && !sameArena(id, entry.id)) {
		throw new TraceReplayError("entry patch identity mismatch");
	}
}
