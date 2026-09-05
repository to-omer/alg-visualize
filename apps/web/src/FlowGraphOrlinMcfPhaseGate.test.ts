import { describe, expect, it } from "vitest";
import {
	orlinMcfBelowGateWitness,
	orlinMcfPhaseGateStatus,
} from "./flow-graph-rational-scales";

describe("Orlin MCF three-quarter delta phase gate", () => {
	it("uses the exact inclusive signed 3Δ/4 comparisons", () => {
		const delta = { numerator: "8", denominator: "3" };
		expect(
			orlinMcfPhaseGateStatus({ numerator: "2", denominator: "1" }, delta),
		).toBe("excess");
		expect(
			orlinMcfPhaseGateStatus({ numerator: "-2", denominator: "1" }, delta),
		).toBe("deficit");
		expect(
			orlinMcfPhaseGateStatus({ numerator: "199", denominator: "100" }, delta),
		).toBe("below");
	});

	it("publishes one deterministic maximum-magnitude witness only when all components miss the gate", () => {
		const delta = { numerator: "3", denominator: "2" };
		const components = [
			{
				component_id: "z",
				members: ["z"],
				excess: { numerator: "1", denominator: "1" },
			},
			{
				component_id: "a",
				members: ["a"],
				excess: { numerator: "-1", denominator: "1" },
			},
			{
				component_id: "m",
				members: ["m"],
				excess: { numerator: "0", denominator: "1" },
			},
		];
		expect(orlinMcfBelowGateWitness(components, delta)?.component_id).toBe("a");
		expect(
			orlinMcfBelowGateWitness(
				[
					...components,
					{
						component_id: "active",
						members: ["active"],
						excess: { numerator: "9", denominator: "8" },
					},
				],
				delta,
			),
		).toBeUndefined();
	});
});
