import { describe, expect, it } from "vitest";

import { traversalGradientSegments } from "./traversal-gradient";

describe("traversal edge gradient", () => {
	it("moves an ordered cyan-to-amber pulse from source toward target", () => {
		const segments = traversalGradientSegments(0.25);

		expect(segments).toHaveLength(6);
		expect(segments[0]).toEqual({
			alpha: 0.4989583333333333,
			color: 0x7ac6b0,
			end: 0.0625,
			start: 0,
		});
		expect(segments.at(-1)).toEqual({
			alpha: 0.9825892857142857,
			color: 0xfac36d,
			end: 0.375,
			start: 0.3125,
		});
		expect(
			segments.every(
				(segment, index) =>
					segment.start < segment.end &&
					(index === 0 ||
						(segments[index - 1]?.end ?? Number.NaN) === segment.start),
			),
		).toBe(true);
	});

	it("leaves the edge settled before motion and after completion", () => {
		expect(traversalGradientSegments(0)).toEqual([]);
		expect(traversalGradientSegments(1)).toEqual([]);
	});
});
