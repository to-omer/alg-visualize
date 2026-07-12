import { describe, expect, it } from "vitest";

import type {
	StatePatchRecord,
	StructureSnapshot,
	TraceEvent,
} from "./engine-types";
import { buildTracePresentation, edgeKey } from "./trace-visualization";

const structure: StructureSnapshot = {
	root: { kind: "node", id: { index: 1, generation: 1 } },
	nodes: [
		{
			id: { kind: "node", id: { index: 1, generation: 1 } },
			role: "tree-node",
			entries: [{ index: 11, generation: 1 }],
			keys: ["4"],
			links: [
				{
					slot: 1,
					role: "right",
					target: { kind: "node", id: { index: 2, generation: 1 } },
				},
			],
			metadata: [],
		},
		{
			id: { kind: "node", id: { index: 2, generation: 1 } },
			role: "tree-node",
			entries: [{ index: 12, generation: 1 }],
			keys: ["6"],
			links: [],
			metadata: [],
		},
	],
};

function event(overrides: Partial<TraceEvent>): TraceEvent {
	return {
		catalog_id: 0,
		kind: "compare",
		node: { kind: "node", id: { index: 1, generation: 1 } },
		target: null,
		entry: { index: 11, generation: 1 },
		key: "6",
		patch_start: 0,
		patch_count: 0,
		...overrides,
	};
}

describe("trace presentation", () => {
	it("highlights both sides of a comparison", () => {
		const result = buildTracePresentation(structure, [event({})], 0);

		expect(result.compareKeys).toEqual(["node:1:1", "node:2:1"]);
		expect(result.queryKey).toBe("6");
		expect(result.queryNodeKey).toBe("node:2:1");
	});

	it("keeps a missing query explicit without inventing a tree node", () => {
		const result = buildTracePresentation(structure, [event({ key: "9" })], 0);

		expect(result.compareKeys).toEqual(["node:1:1"]);
		expect(result.queryKey).toBe("9");
		expect(result.queryNodeKey).toBeUndefined();
	});

	it("accumulates only the visited prefix and colors the traversed edge", () => {
		const trace = [
			event({ key: "9" }),
			event({
				kind: "descend",
				target: { kind: "node", id: { index: 2, generation: 1 } },
				entry: null,
				key: "right",
			}),
			event({
				node: { kind: "node", id: { index: 2, generation: 1 } },
				entry: null,
				key: "9",
			}),
		];
		const result = buildTracePresentation(structure, trace, 2);

		expect(result.visitedKeys).toEqual(["node:1:1", "node:2:1"]);
		expect(result.currentEdges).toEqual([edgeKey("node:1:1", "node:2:1")]);
		expect(result.visitedEdges).toEqual([edgeKey("node:1:1", "node:2:1")]);
		expect(result.traversalEdge).toEqual({
			key: edgeKey("node:1:1", "node:2:1"),
			source: "node:1:1",
			target: "node:2:1",
		});
	});

	it("identifies both rotation participants and their structural edge", () => {
		const result = buildTracePresentation(
			structure,
			[
				event({
					kind: "rotate-left",
					entry: { index: 12, generation: 1 },
					key: "6",
				}),
			],
			0,
		);

		expect(result.mutationKeys).toEqual(["node:1:1", "node:2:1"]);
		expect(result.currentEdges).toEqual([edgeKey("node:1:1", "node:2:1")]);
	});

	it("retains a node touched by an earlier mutation in the operation trail", () => {
		const result = buildTracePresentation(
			structure,
			[
				event({ kind: "insert", node: null, entry: null, key: "6" }),
				event({ kind: "update-metadata", key: "4" }),
			],
			1,
		);

		expect(result.visitedKeys).toContain("node:2:1");
	});

	it("preserves auxiliary identity and uses the declared descent endpoint", () => {
		const auxiliary = {
			kind: "auxiliary" as const,
			id: { index: 8, generation: 2 },
		};
		const child = {
			kind: "auxiliary" as const,
			id: { index: 9, generation: 2 },
		};
		const result = buildTracePresentation(
			{
				root: auxiliary,
				nodes: [
					{
						id: auxiliary,
						role: "veb-root",
						entries: [],
						keys: ["1"],
						links: [{ slot: 0, role: "cluster", target: child }],
						metadata: [],
					},
					{
						id: child,
						role: "veb-cluster",
						entries: [],
						keys: ["1"],
						links: [],
						metadata: [],
					},
				],
			},
			[
				event({
					kind: "descend",
					node: auxiliary,
					target: child,
					entry: null,
				}),
			],
			0,
		);

		expect(result.currentKeys).toEqual(["auxiliary:8:2", "auxiliary:9:2"]);
		expect(result.currentEdges).toEqual([
			edgeKey("auxiliary:8:2", "auxiliary:9:2"),
		]);
	});

	it("folds a descent run into its destination using the latest valid edge", () => {
		const validDescent = event({
			kind: "descend",
			target: { kind: "node", id: { index: 2, generation: 1 } },
			entry: null,
		});
		const bookkeepingDescent = event({
			kind: "descend",
			node: { kind: "node", id: { index: 2, generation: 1 } },
			target: null,
			entry: null,
		});
		const result = buildTracePresentation(
			structure,
			[validDescent, bookkeepingDescent, event({ kind: "result" })],
			2,
		);

		expect(result.traversalEdge).toEqual({
			key: edgeKey("node:1:1", "node:2:1"),
			source: "node:1:1",
			target: "node:2:1",
		});
	});

	it("colors removed nodes and disappearing edges from patch before-state", () => {
		const removedNode = structure.nodes[1];
		const retainedBefore = structure.nodes[0];
		if (removedNode === undefined || retainedBefore === undefined) {
			throw new Error("fixture nodes are missing");
		}
		const retainedAfter = { ...retainedBefore, links: [] };
		const patches: StatePatchRecord[] = [
			{
				kind: "node",
				id: retainedBefore.id,
				before: retainedBefore,
				after: retainedAfter,
			},
			{
				kind: "node",
				id: removedNode.id,
				before: removedNode,
				after: null,
			},
			{
				kind: "entry",
				id: { index: 12, generation: 1 },
				before: { id: { index: 12, generation: 1 }, key: "6", value: "six" },
				after: null,
			},
		];
		const removal = event({
			kind: "remove",
			node: null,
			target: null,
			entry: { index: 12, generation: 1 },
			key: "6",
			patch_count: patches.length,
		});
		const result = buildTracePresentation(
			{ root: structure.root, nodes: [retainedAfter] },
			[removal],
			0,
			patches,
		);

		expect(result.mutationKeys).toEqual(["node:1:1", "node:2:1"]);
		expect(result.currentEdges).toEqual([edgeKey("node:1:1", "node:2:1")]);
	});
});
