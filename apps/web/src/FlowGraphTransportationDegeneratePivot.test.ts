import { describe, expect, it } from "vitest";

import { flowCycleAdjustmentLabel } from "./FlowGraphEdgeAnnotationFeatureBundle";

describe("transportation degenerate cycle labels", () => {
	it.each([
		"transportation-simplex.degenerate-pivot",
		"modi.degenerate-loop-adjustment",
	])("publishes zero movement at %s", (catalogId) => {
		expect(flowCycleAdjustmentLabel("add", catalogId)).toBe("+0");
		expect(flowCycleAdjustmentLabel("subtract", catalogId)).toBe("−0");
	});

	it("keeps theta on a nondegenerate cycle boundary", () => {
		expect(
			flowCycleAdjustmentLabel("add", "transportation-simplex.augment-cycle"),
		).toBe("+θ");
		expect(flowCycleAdjustmentLabel("subtract", "modi.form-closed-loop")).toBe(
			"−θ",
		);
	});
});
