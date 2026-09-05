import { describe, expect, it } from "vitest";
import {
	capacityRailWidth,
	costMagnitudeIntensity,
	flowFillStrokeWidth,
	rationalCapacityRailWidth,
} from "./flow-visual-scales";

describe("flow visual scales", () => {
	it("maps cost magnitude continuously and symmetrically", () => {
		expect(costMagnitudeIntensity(0n, 10n)).toBe(0);
		expect(costMagnitudeIntensity(1n, 10n)).toBe(0.1);
		expect(costMagnitudeIntensity(-5n, 10n)).toBe(0.5);
		expect(costMagnitudeIntensity(10n, 10n)).toBe(1);
		expect(costMagnitudeIntensity(99n, 10n)).toBe(1);
	});

	it("keeps zero capacity distinct and a single positive value at full width", () => {
		expect(capacityRailWidth(0n, 0n)).toBe(4);
		expect(capacityRailWidth(0n, 1n)).toBe(4);
		expect(capacityRailWidth(1n, 1n)).toBe(10);
		expect(capacityRailWidth(7n, 7n)).toBe(10);
	});

	it("is monotone while preserving small capacities across a wide range", () => {
		const capacities = [0n, 1n, 4n, 1_000n, 1_000_000n];
		const widths = capacities.map((capacity) =>
			capacityRailWidth(capacity, 1_000_000n),
		);
		for (let index = 1; index < widths.length; index += 1) {
			expect(widths[index]).toBeGreaterThan(widths[index - 1] ?? -1);
		}
		expect(widths[0]).toBe(4);
		expect(widths.at(-1)).toBe(10);
	});

	it("remains finite and bounded over the full strict u64 domain", () => {
		const maximum = (1n << 64n) - 1n;
		for (const capacity of [0n, 1n, 1n << 32n, maximum]) {
			const width = capacityRailWidth(capacity, maximum);
			expect(Number.isFinite(width)).toBe(true);
			expect(width).toBeGreaterThanOrEqual(4);
			expect(width).toBeLessThanOrEqual(10);
		}
		expect(capacityRailWidth(maximum, maximum)).toBe(10);
	});

	it("orders exact fractional capacities without lossy BigInt conversion", () => {
		const huge = BigInt("9".repeat(120));
		const maximum = { numerator: huge * 11n, denominator: 7n };
		const widths = [1n, 2n, huge, huge * 11n].map((numerator) =>
			rationalCapacityRailWidth({ numerator, denominator: 7n }, maximum),
		);
		for (let index = 1; index < widths.length; index += 1) {
			expect(widths[index]).toBeGreaterThan(widths[index - 1] ?? -1);
		}
		expect(widths.at(-1)).toBe(10);
	});

	it("clamps an inconsistent capacity above the fixed visual maximum", () => {
		expect(
			rationalCapacityRailWidth(
				{ numerator: 19n, denominator: 3n },
				{ numerator: 5n, denominator: 2n },
			),
		).toBe(10);
	});

	it("uses one bounded proportional flow fill scale across canvas LODs", () => {
		expect(flowFillStrokeWidth(0n, 10n, 10)).toBe(0);
		expect(flowFillStrokeWidth(5n, 10n, 10)).toBe(3.75);
		expect(flowFillStrokeWidth(10n, 10n, 10)).toBe(6);
		expect(flowFillStrokeWidth(50n, 10n, 5)).toBe(3.5);
		expect(flowFillStrokeWidth(4n, 0n, 10)).toBe(0);
	});
});
