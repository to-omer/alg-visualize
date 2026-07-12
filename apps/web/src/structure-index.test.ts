import { describe, expect, it } from "vitest";

import type { StructureSnapshot } from "./engine-types";
import { getStructureNode, getStructureNodeByKey } from "./structure-index";

const structure: StructureSnapshot = {
	root: { kind: "node", id: { index: 0, generation: 1 } },
	nodes: [
		{
			id: { kind: "node", id: { index: 0, generation: 1 } },
			role: "primary",
			entries: [],
			keys: ["7"],
			links: [],
			metadata: [],
		},
		{
			id: { kind: "auxiliary", id: { index: 0, generation: 2 } },
			role: "summary",
			entries: [],
			keys: [],
			links: [],
			metadata: [],
		},
	],
};

describe("structure index", () => {
	it("keeps node and auxiliary identity spaces separate", () => {
		expect(getStructureNodeByKey(structure, "node:0:1")?.role).toBe("primary");
		expect(getStructureNodeByKey(structure, "auxiliary:0:2")?.role).toBe(
			"summary",
		);
	});

	it("rejects stale generations and malformed identity keys", () => {
		expect(
			getStructureNode(structure, {
				kind: "node",
				id: { index: 0, generation: 2 },
			}),
		).toBeUndefined();
		expect(getStructureNodeByKey(structure, "node:0:1:extra")).toBeUndefined();
	});
});
