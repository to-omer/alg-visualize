import { describe, expect, it } from "vitest";

import type { StructureNode, StructureSnapshot } from "./engine-types";
import { layoutGraph, nodeLabel, nodeVisualWidth } from "./graph-layout";

function chain(length: number): StructureSnapshot {
	const nodes = Array.from({ length }, (_, index) => ({
		id: {
			kind: "node" as const,
			id: { index, generation: 1 },
		},
		role: "binary-node",
		entries: [{ index, generation: 1 }],
		keys: [String(index)],
		links:
			index + 1 < length
				? [
						{
							slot: 0,
							role: "right",
							target: {
								kind: "node" as const,
								id: { index: index + 1, generation: 1 },
							},
						},
					]
				: [],
		metadata: [],
	}));
	return { root: nodes[0]?.id ?? null, nodes };
}

describe("graph layout", () => {
	it("keeps the complete tree visible through one thousand entities", () => {
		const structure = chain(1_000);
		const layout = layoutGraph(structure, 1_000, 600, []);

		expect(layout.mode).toBe("detail");
		expect(layout.nodes).toHaveLength(1_000);
		expect(layout.positions.size).toBe(1_000);
		const first = layout.positions.get("node:0:1");
		const second = layout.positions.get("node:1:1");
		if (first === undefined || second === undefined) {
			throw new Error("detail layout omitted a deterministic chain position");
		}
		expect(second.x - first.x).toBeGreaterThan(60);
		expect(layoutGraph(chain(8_001), 1_000, 600, []).mode).toBe("summary");
	});

	it("uses a bounded summary while retaining every emphasized entity", () => {
		const structure = chain(10_000);
		const importantKeys = ["node:4999:1", "node:9999:1"];
		const layout = layoutGraph(structure, 1_000, 600, importantKeys);

		expect(layout.mode).toBe("summary");
		expect(layout.nodes).toHaveLength(2_000);
		for (const importantKey of importantKeys) {
			expect(layout.positions.has(importantKey)).toBe(true);
		}
	});

	it("keeps summary output bounded when an operation touches many entities", () => {
		const structure = chain(10_000);
		const importantKeys = Array.from(
			{ length: 2_500 },
			(_, index) => `node:${index}:1`,
		);
		const layout = layoutGraph(structure, 1_000, 600, importantKeys);

		expect(layout.nodes).toHaveLength(2_000);
		expect(layout.positions.has("node:1999:1")).toBe(true);
	});

	it("is deterministic for the same scene and viewport", () => {
		const structure = chain(20);
		const first = layoutGraph(structure, 1_000, 600, []);
		const second = layoutGraph(structure, 1_000, 600, []);

		expect([...first.positions]).toEqual([...second.positions]);
		expect(first.edges).toEqual(second.edges);
	});

	it("assigns distinct positions to vEB nodes sharing one depth", () => {
		const auxiliary = (index: number, keys: string[]): StructureNode => ({
			id: { kind: "auxiliary", id: { index, generation: 1 } },
			role: index === 0 ? "veb-root" : "veb-cluster",
			entries: [],
			keys,
			links: [],
			metadata: [["word-bits", "8"]],
		});
		const root = auxiliary(0, ["1", "9"]);
		root.links = [1, 2, 3].map((index) => ({
			slot: index,
			role: `cluster-${index}`,
			target: { kind: "auxiliary", id: { index, generation: 1 } },
		}));
		const structure = {
			root: root.id,
			nodes: [root, auxiliary(1, []), auxiliary(2, ["1"]), auxiliary(3, ["1"])],
		};

		const layout = layoutGraph(structure, 1_000, 600, []);
		const clusterXs = [1, 2, 3].map(
			(index) => layout.positions.get(`auxiliary:${index}:1`)?.x,
		);
		expect(new Set(clusterXs).size).toBe(3);
	});

	it("renders a B-tree node as a readable multi-entry unit", () => {
		const node: StructureNode = {
			id: { kind: "node", id: { index: 0, generation: 1 } },
			role: "btree-internal",
			entries: [0, 1, 2, 3].map((index) => ({ index, generation: 1 })),
			keys: ["10", "20", "30", "40"],
			links: [],
			metadata: [["leaf", "0"]],
		};

		expect(nodeLabel(node)).toBe("10  │  20  │  30  │  40");
		expect(nodeVisualWidth(node)).toBeGreaterThan(180);
	});
});
