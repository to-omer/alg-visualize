import { describe, expect, it } from "vitest";

import { projectGoldbergRaoMetrics } from "./flow-goldberg-rao-metrics";

describe("projectGoldbergRaoMetrics", () => {
	it("names every source-specific counter without claiming dynamic-tree work", () => {
		const rows = projectGoldbergRaoMetrics(
			Array.from({ length: 16 }, (_, index) => index.toString()),
		);
		expect(rows).toEqual([
			{ label: "0–1 distance searches / gap phases", value: "0 / 1" },
			{ label: "Residual scans / state transitions", value: "2 / 15" },
			{ label: "Binary updates / augmented units", value: "3 / 14" },
			{ label: "Canonical cuts / gap replacements", value: "4 / 10" },
			{ label: "Blocking / delta-limited updates", value: "6 / 12" },
			{
				label: "Base-zero / special arc observations",
				value: "7 / 8",
			},
			{
				label: "Nontrivial SCCs / contracted augmentations",
				value: "9 / 11",
			},
			{ label: "Component lift paths", value: "13" },
		]);
	});

	it("rejects a metric vector from another revision", () => {
		expect(() => projectGoldbergRaoMetrics(["0"])).toThrow(
			"Goldberg–Rao metrics require the flow-metrics/6 vector",
		);
	});
});
