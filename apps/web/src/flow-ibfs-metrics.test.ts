import { describe, expect, it } from "vitest";

import {
	projectBoykovKolmogorovMetrics,
	projectIbfsMetrics,
} from "./flow-ibfs-metrics";

describe("projectIbfsMetrics", () => {
	it("projects every IBFS counter group from flow-metrics/6", () => {
		expect(
			projectIbfsMetrics(
				Array.from({ length: 16 }, (_, index) => `${101 + index}`),
			),
		).toEqual([
			{ label: "Passes · forward / reverse", value: "101 · 102 / 106" },
			{ label: "Residual / adoption scans", value: "103 / 110" },
			{ label: "Augmentations / path arcs", value: "104 / 105" },
			{ label: "Tree attachments / removals", value: "107 / 109" },
			{ label: "Orphans created / visited", value: "111 / 112" },
			{ label: "Same-level / relabeled", value: "114 / 108" },
			{ label: "Saturated tree arcs", value: "113" },
			{ label: "Active scans / transitions", value: "115 / 116" },
		]);
	});

	it("rejects a noncanonical counter vector", () => {
		expect(() => projectIbfsMetrics(["1"])).toThrow(
			"IBFS metrics require the flow-metrics/6 vector",
		);
	});
});

describe("projectBoykovKolmogorovMetrics", () => {
	it("names retained-tree and orphan work without IBFS pass terminology", () => {
		expect(
			projectBoykovKolmogorovMetrics([
				"6",
				"3",
				"12",
				"3",
				"7",
				"2",
				"3",
				"2",
				"3",
				"5",
				"3",
				"1",
				"3",
				"1",
				"2",
				"14",
			]),
		).toEqual([
			{ label: "Active visits / passive vertices", value: "6 / 3" },
			{ label: "Growth / adoption scans", value: "7 / 5" },
			{ label: "Augmentations / path arcs", value: "3 / 7" },
			{ label: "Tree attachments / reactivations", value: "2 / 2" },
			{ label: "Orphans created / visited", value: "3 / 3" },
			{ label: "Adopted / made free", value: "1 / 2" },
			{ label: "State transitions", value: "14" },
		]);
	});

	it("rejects a noncanonical counter vector", () => {
		expect(() => projectBoykovKolmogorovMetrics(["1"])).toThrow(
			"Boykov–Kolmogorov metrics require flow-metrics/6",
		);
	});
});
