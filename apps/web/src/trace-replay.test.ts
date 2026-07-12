import { describe, expect, it } from "vitest";

import type {
	CanonicalEntry,
	CommitFrame,
	CurrentFrame,
	Metrics,
	StructureNode,
} from "./engine-types";
import { TraceReplayController, TraceReplayError } from "./trace-replay";

const zeroMetrics: Metrics = {
	comparisons: "0",
	node_visits: "0",
	bit_tests: "0",
	rotations: "0",
	recolors: "0",
	splits: "0",
	merges: "0",
	rebuild_items: "0",
	allocations: "0",
	frees: "0",
};

const id = (index: number) => ({ index, generation: 1 });
const entity = (index: number) => ({ kind: "node" as const, id: id(index) });
const entry = (index: number, key: string): CanonicalEntry => ({
	id: id(index),
	key,
	value: key,
});
const node = (
	index: number,
	key: string,
	links: StructureNode["links"],
): StructureNode => ({
	id: entity(index),
	role: "binary-node",
	entries: [id(index)],
	keys: [key],
	links,
	metadata: [["height", links.length === 0 ? "1" : "2"]],
});

const beforeRoot = node(0, "1", [
	{ slot: 1, role: "right", target: entity(1) },
]);
const beforePivot = node(1, "2", []);
const afterRoot = node(0, "1", []);
const afterPivot = node(1, "2", [{ slot: 0, role: "left", target: entity(0) }]);

const base: CurrentFrame = {
	itemIndex: 0,
	itemCount: 1,
	structure: { root: entity(0), nodes: [beforeRoot, beforePivot] },
	canonical: {
		entries: [entry(0, "1"), entry(1, "2")],
		metrics: zeroMetrics,
	},
};

function commit(): CommitFrame {
	return {
		baseItemIndex: 0,
		itemIndex: 1,
		itemCount: 1,
		initialBuild: false,
		result: { kind: "found", entry: id(1), key: "2", value: "2" },
		trace: [
			{
				catalog_id: 6,
				kind: "rotate-left",
				node: entity(0),
				target: entity(1),
				entry: id(1),
				key: "2",
				patch_start: 0,
				patch_count: 4,
			},
		],
		patches: [
			{ kind: "root", before: entity(0), after: entity(1) },
			{
				kind: "node",
				id: entity(0),
				before: beforeRoot,
				after: afterRoot,
			},
			{
				kind: "node",
				id: entity(1),
				before: beforePivot,
				after: afterPivot,
			},
			{
				kind: "metric",
				ordinal: "rotations",
				before: "0",
				after: "1",
			},
		],
	};
}

describe("trace replay controller", () => {
	it("treats a missing map record as the wire null used by insertion", () => {
		const insertedNode = node(0, "1", []);
		const insertedEntry = entry(0, "1");
		const empty: CurrentFrame = {
			itemIndex: 0,
			itemCount: 1,
			structure: { root: null, nodes: [] },
			canonical: { entries: [], metrics: zeroMetrics },
		};
		const inserted: CommitFrame = {
			baseItemIndex: 0,
			itemIndex: 1,
			itemCount: 1,
			initialBuild: false,
			result: { kind: "inserted", entry: id(0) },
			trace: [
				{
					catalog_id: 3,
					kind: "insert",
					node: entity(0),
					target: null,
					entry: id(0),
					key: "1",
					patch_start: 0,
					patch_count: 4,
				},
				{
					catalog_id: 9,
					kind: "result",
					node: null,
					target: null,
					entry: null,
					key: "1",
					patch_start: 4,
					patch_count: 0,
				},
			],
			patches: [
				{ kind: "root", before: null, after: entity(0) },
				{ kind: "node", id: entity(0), before: null, after: insertedNode },
				{ kind: "entry", id: id(0), before: null, after: insertedEntry },
				{
					kind: "metric",
					ordinal: "allocations",
					before: "0",
					after: "2",
				},
			],
		};

		const controller = new TraceReplayController(empty, inserted);
		const atInsert = controller.moveTo(0);
		expect(atInsert.structure).toEqual({
			root: entity(0),
			nodes: [insertedNode],
		});
		expect(atInsert.result).toBeUndefined();
		expect(controller.moveTo(1).result).toEqual(inserted.result);
		expect(controller.moveTo(-1).canonical).toEqual(empty.canonical);
	});

	it("moves a structural event forward and backward exactly", () => {
		const controller = new TraceReplayController(base, commit());

		expect(controller.moveTo(0).structure).toEqual({
			root: entity(1),
			nodes: [afterRoot, afterPivot],
		});
		expect(controller.moveTo(0).canonical.metrics.rotations).toBe("1");
		expect(controller.moveTo(-1).structure).toEqual(base.structure);
		expect(controller.moveTo(-1).canonical).toEqual(base.canonical);
	});

	it("transfers the indexed final boundary to the next operation", async () => {
		const first = await TraceReplayController.create(base, commit());
		const boundary = first.moveTo(0);
		const nextBase: CurrentFrame = { ...boundary, itemCount: 2 };
		const next: CommitFrame = {
			baseItemIndex: 1,
			itemIndex: 2,
			itemCount: 2,
			initialBuild: false,
			result: { kind: "miss" },
			trace: [
				{
					catalog_id: 9,
					kind: "result",
					node: null,
					target: null,
					entry: null,
					key: "9",
					patch_start: 0,
					patch_count: 0,
				},
			],
			patches: [],
		};

		const second = await TraceReplayController.create(nextBase, next, first);
		expect(second.moveTo(0).structure).toEqual(boundary.structure);
		expect(() => first.moveTo(-1)).toThrowError(
			new TraceReplayError("trace state has moved to the next operation"),
		);
	});

	it("does not transfer state while a queued replay movement owns it", async () => {
		const first = await TraceReplayController.create(base, commit());
		const boundary = first.moveTo(0);
		const movement = first.moveToAsync(-1);
		const nextBase: CurrentFrame = { ...boundary, itemCount: 2 };
		const next: CommitFrame = {
			baseItemIndex: 1,
			itemIndex: 2,
			itemCount: 2,
			initialBuild: false,
			result: { kind: "miss" },
			trace: [
				{
					catalog_id: 9,
					kind: "result",
					node: null,
					target: null,
					entry: null,
					key: "9",
					patch_start: 0,
					patch_count: 0,
				},
			],
			patches: [],
		};

		const second = await TraceReplayController.create(nextBase, next, first);
		await movement;
		expect(second.moveTo(0).structure).toEqual(boundary.structure);
		expect(first.moveTo(0).structure).toEqual(boundary.structure);
	});

	it("yields while validating a commit with many replay records", async () => {
		const count = 1_025;
		const largeCommit: CommitFrame = {
			baseItemIndex: 0,
			itemIndex: 1,
			itemCount: 1,
			initialBuild: false,
			result: { kind: "miss" },
			trace: Array.from({ length: count }, (_, index) => ({
				catalog_id: 7,
				kind: "update-metadata" as const,
				node: entity(0),
				target: null,
				entry: null,
				key: null,
				patch_start: index,
				patch_count: 1,
			})),
			patches: Array.from({ length: count }, (_, index) => ({
				kind: "metric" as const,
				ordinal: "comparisons" as const,
				before: String(index),
				after: String(index + 1),
			})),
		};
		let settled = false;
		const replay = TraceReplayController.create(base, largeCommit).then(
			(controller) => {
				settled = true;
				return controller;
			},
		);

		await Promise.resolve();
		expect(settled).toBe(false);
		await expect(replay).resolves.toBeInstanceOf(TraceReplayController);
	});

	it("keeps the previous replay usable while a large commit is prepared", async () => {
		const first = await TraceReplayController.create(base, commit());
		const boundary = first.moveTo(0);
		const count = 1_025;
		const largeCommit: CommitFrame = {
			baseItemIndex: 1,
			itemIndex: 2,
			itemCount: 2,
			initialBuild: false,
			result: { kind: "miss" },
			trace: Array.from({ length: count }, (_, index) => ({
				catalog_id: 7,
				kind: "update-metadata" as const,
				node: entity(1),
				target: null,
				entry: null,
				key: null,
				patch_start: index,
				patch_count: 1,
			})),
			patches: Array.from({ length: count }, (_, index) => ({
				kind: "metric" as const,
				ordinal: "comparisons" as const,
				before: String(index),
				after: String(index + 1),
			})),
		};
		const pending = TraceReplayController.create(
			{ ...boundary, itemCount: 2 },
			largeCommit,
			first,
		);

		expect(first.moveTo(-1).structure).toEqual(base.structure);
		await expect(pending).resolves.toBeInstanceOf(TraceReplayController);
	});

	it("reuses the projection snapshot across observation-only events", () => {
		const observed: CommitFrame = {
			baseItemIndex: 0,
			itemIndex: 1,
			itemCount: 1,
			initialBuild: false,
			result: { kind: "miss" },
			trace: [
				{
					catalog_id: 1,
					kind: "compare",
					node: entity(0),
					target: null,
					entry: id(0),
					key: "9",
					patch_start: 0,
					patch_count: 1,
				},
				{
					catalog_id: 2,
					kind: "descend",
					node: entity(0),
					target: entity(1),
					entry: null,
					key: "9",
					patch_start: 1,
					patch_count: 1,
				},
			],
			patches: [
				{
					kind: "metric",
					ordinal: "comparisons",
					before: "0",
					after: "1",
				},
				{
					kind: "metric",
					ordinal: "node-visits",
					before: "0",
					after: "1",
				},
			],
		};
		const controller = new TraceReplayController(base, observed);
		const compare = controller.moveTo(0);
		const descend = controller.moveTo(1);
		expect(descend.structure).toBe(compare.structure);
		expect(descend.canonical).not.toBe(compare.canonical);
		expect(descend.canonical.entries).toBe(compare.canonical.entries);
	});

	it.each([
		["inserted", { kind: "inserted", entry: id(2) }],
		["overwritten", { kind: "overwritten", entry: id(1), previous: "before" }],
		["removed", { kind: "removed", entry: id(1), value: "removed" }],
		["miss", { kind: "miss" }],
		["found", { kind: "found", entry: id(1), key: "2", value: "2" }],
	] as const)("does not publish the %s result before its result event", (_, result) => {
		const recorded = commit();
		recorded.result = result;
		recorded.trace = [
			...recorded.trace,
			{
				catalog_id: 9,
				kind: "result",
				node: null,
				target: null,
				entry: null,
				key: "2",
				patch_start: recorded.patches.length,
				patch_count: 0,
			},
		];

		const controller = new TraceReplayController(base, recorded);
		expect(controller.moveTo(0).result).toBeUndefined();
		expect(controller.moveTo(1).result).toEqual(result);
		expect(controller.moveTo(0).result).toBeUndefined();
	});

	it("rejects stale patch input without publishing a controller", () => {
		const corrupted = commit();
		corrupted.patches[3] = {
			kind: "metric",
			ordinal: "rotations",
			before: "9",
			after: "10",
		};

		expect(() => new TraceReplayController(base, corrupted)).toThrowError(
			new TraceReplayError("metric patch precondition mismatch"),
		);
	});

	it("rejects a patch that is omitted from the trace spans", () => {
		const corrupted = commit();
		const event = corrupted.trace[0];
		if (event === undefined) throw new Error("fixture event is missing");
		corrupted.trace[0] = { ...event, patch_count: 3 };

		expect(() => new TraceReplayController(base, corrupted)).toThrowError(
			new TraceReplayError("trace patch spans do not cover all patches"),
		);
	});

	it("rejects a transaction that strands a primary node", () => {
		const corrupted = commit();
		const rotation = corrupted.trace[0];
		if (rotation === undefined) {
			throw new Error("fixture rotation is missing");
		}
		corrupted.trace[0] = {
			...rotation,
			patch_count: 1,
		};
		corrupted.patches = [
			{
				kind: "node",
				id: entity(0),
				before: beforeRoot,
				after: afterRoot,
			},
		];
		expect(() => new TraceReplayController(base, corrupted)).toThrowError(
			new TraceReplayError("primary node is unreachable from root"),
		);
	});

	it("rejects an affected component that forms a detached cycle", () => {
		const middle = node(1, "2", [
			{ slot: 1, role: "right", target: entity(2) },
		]);
		const leaf = node(2, "3", []);
		const cycleBase: CurrentFrame = {
			itemIndex: 0,
			itemCount: 1,
			structure: {
				root: entity(0),
				nodes: [
					node(0, "1", [{ slot: 1, role: "right", target: entity(1) }]),
					middle,
					leaf,
				],
			},
			canonical: {
				entries: [entry(0, "1"), entry(1, "2"), entry(2, "3")],
				metrics: zeroMetrics,
			},
		};
		const rootAfter = node(0, "1", []);
		const leafAfter = node(2, "3", [
			{ slot: 0, role: "left", target: entity(1) },
		]);
		const corrupted: CommitFrame = {
			baseItemIndex: 0,
			itemIndex: 1,
			itemCount: 1,
			initialBuild: false,
			result: { kind: "miss" },
			trace: [
				{
					catalog_id: 8,
					kind: "rebuild",
					node: entity(1),
					target: null,
					entry: id(1),
					key: "2",
					patch_start: 0,
					patch_count: 2,
				},
			],
			patches: [
				{
					kind: "node",
					id: entity(0),
					before: cycleBase.structure.nodes[0] ?? null,
					after: rootAfter,
				},
				{
					kind: "node",
					id: entity(2),
					before: leaf,
					after: leafAfter,
				},
			],
		};

		expect(() => new TraceReplayController(cycleBase, corrupted)).toThrowError(
			new TraceReplayError("primary node is unreachable from root"),
		);
	});

	it.each([
		["reroot", entity(1)],
		["null root", null],
	] as const)("rejects a root-only %s patch that strands nodes", (_, nextRoot) => {
		const corrupted: CommitFrame = {
			baseItemIndex: 0,
			itemIndex: 1,
			itemCount: 1,
			initialBuild: false,
			result: { kind: "miss" },
			trace: [
				{
					catalog_id: 8,
					kind: "rebuild",
					node: nextRoot,
					target: null,
					entry: null,
					key: null,
					patch_start: 0,
					patch_count: 1,
				},
			],
			patches: [{ kind: "root", before: entity(0), after: nextRoot }],
		};

		expect(() => new TraceReplayController(base, corrupted)).toThrowError(
			new TraceReplayError("primary node is unreachable from root"),
		);
	});
});
