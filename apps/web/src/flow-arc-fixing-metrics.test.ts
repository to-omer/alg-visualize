import { describe, expect, it } from "vitest";

import { projectArcFixingMetrics } from "./flow-arc-fixing-metrics";

describe("projectArcFixingMetrics", () => {
	it("assigns every wire index to its audited Arc Fixing counter", () => {
		expect(
			projectArcFixingMetrics(
				Array.from({ length: 16 }, (_, index) => `${101 + index}`),
			),
		).toEqual({
			fixingPasses: "101",
			arcsUnfixed: "102",
			residualArcScans: "103",
			arcsFixed: "104",
			fixIns: "105",
			refinePhases: "106",
			recoveries: "107",
			relabels: "108",
			fixedArcSkips: "109",
			currentArcAdvances: "110",
			initialSaturations: "111",
			pushes: "112",
			saturatingPushes: "113",
			nonsaturatingPushes: "114",
			discharges: "115",
			activeVertexSelections: "116",
		});
	});

	it("rejects a non-canonical counter tuple", () => {
		expect(() => projectArcFixingMetrics(["1", "2"])).toThrow(
			"Arc Fixing requires exactly 16 metrics",
		);
	});
});
