import { describe, expect, it } from "vitest";

import { primaryWorkProgressValue } from "./flow-detail-density";

describe("flow exact work progress", () => {
	it("scales exact decimal ratios without losing large integer precision", () => {
		expect(primaryWorkProgressValue("0", "0")).toBe(0);
		expect(primaryWorkProgressValue("1", "4")).toBe(250);
		expect(primaryWorkProgressValue("7083294", "7083294")).toBe(1000);
		expect(
			primaryWorkProgressValue(
				"170141183460469231731687303715884105727",
				"340282366920938463463374607431768211454",
			),
		).toBe(500);
	});

	it("rejects progress outside the exact measured total", () => {
		expect(() => primaryWorkProgressValue("2", "1")).toThrow(RangeError);
	});
});
