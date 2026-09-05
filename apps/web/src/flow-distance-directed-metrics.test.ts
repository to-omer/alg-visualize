import { describe, expect, it } from "vitest";

import { projectDistanceDirectedMetrics } from "./flow-distance-directed-metrics";

describe("projectDistanceDirectedMetrics", () => {
	it("names every DD2 tree-repair counter in the stable metric vector", () => {
		const rows = projectDistanceDirectedMetrics(
			Array.from({ length: 16 }, (_, index) => index.toString()),
		);
		expect(rows).toEqual([
			{ label: "Exact-tree BFS / scaling phases", value: "9 / 5" },
			{ label: "Tree repairs / invalid parents", value: "4 / 8" },
			{ label: "Parent replacements / relabels", value: "11 / 7" },
			{ label: "Deleted nodes / cascaded children", value: "10 / 13" },
			{
				label: "Saturated tree arcs / current-arc advances",
				value: "12 / 1",
			},
			{ label: "Residual scans / state transitions", value: "2 / 15" },
		]);
	});

	it("rejects a metric vector from another revision", () => {
		expect(() => projectDistanceDirectedMetrics(["0"])).toThrow(
			"Distance-directed metrics require the flow-metrics/6 vector",
		);
	});
});
