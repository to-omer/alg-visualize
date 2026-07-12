import { describe, expect, it } from "vitest";

import { fitsUtf8Budget } from "./utf8-budget";

describe("UTF-8 input budget", () => {
	it.each([
		[["abc", "de"], 5, true],
		[["abc", "de"], 4, false],
		[["é"], 2, true],
		[["é"], 1, false],
		[["😀"], 4, true],
		[["😀"], 3, false],
		[["\ud800"], 3, true],
		[["\ud800"], 2, false],
	] as const)("counts %j against %d bytes", (values, limit, expected) => {
		expect(fitsUtf8Budget(values, limit)).toBe(expected);
	});

	it("stops across the combined document boundary", () => {
		expect(fitsUtf8Budget(["éé", "a"], 5)).toBe(true);
		expect(fitsUtf8Budget(["éé", "a"], 4)).toBe(false);
	});
});
