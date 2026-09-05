import { describe, expect, it } from "vitest";

import { projectEibfsMetrics } from "./flow-eibfs-metrics";

describe("projectEibfsMetrics", () => {
	it("names all 16 counters without collapsing pseudoflow and recovery work", () => {
		const rows = projectEibfsMetrics(
			Array.from({ length: 16 }, (_, index) => index.toString()),
		);
		expect(rows).toEqual([
			{ label: "Phases · forward / reverse", value: "0 · 1 / 5" },
			{ label: "Residual / adoption scans", value: "2 / 9" },
			{ label: "Bridge / tree-path pushes", value: "3 / 4" },
			{ label: "Tree attachments / removals", value: "6 / 8" },
			{ label: "Orphans created / visited", value: "10 / 11" },
			{ label: "Relabels / side migrations", value: "7 / 13" },
			{
				label: "Saturated tree arcs / recovery paths",
				value: "12 / 14",
			},
			{ label: "State transitions", value: "15" },
		]);
	});

	it("rejects metrics from a different revision", () => {
		expect(() => projectEibfsMetrics(["0"])).toThrow(
			"EIBFS metrics require the flow-metrics/6 vector",
		);
	});

	it("names Dynamic EIBFS reuse and all five repair channels", () => {
		const rows = projectEibfsMetrics(
			Array.from({ length: 16 }, (_, index) => index.toString()),
			true,
		);
		expect(rows).toEqual([
			{ label: "Updates · increases / decreases", value: "0 · 1 / 5" },
			{ label: "Repair scans / iterations", value: "2 / 13" },
			{ label: "Bridge / label violations", value: "3 / 4" },
			{ label: "Current-arc / boundary violations", value: "7 / 9" },
			{ label: "Reused nodes / invalidated parents", value: "6 / 8" },
			{ label: "Over-capacity repairs / promoted roots", value: "10 / 11" },
			{ label: "No-ops / certification recoveries", value: "12 / 14" },
			{ label: "Reusable state transitions", value: "15" },
		]);
	});
});
