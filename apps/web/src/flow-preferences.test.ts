import { describe, expect, it } from "vitest";
import { DEFAULT_FLOW_GENERATOR } from "./flow-generator-form";
import {
	readFlowPreferences,
	validateRestoredFlowGenerator,
	writeFlowPreferences,
} from "./flow-preferences";
import { flowWorkbenchPolicy } from "./flow-workbench-policy";

function memoryStorage(initial: string | null = null) {
	let value = initial;
	return {
		getItem: () => value,
		setItem: (_key: string, next: string) => {
			value = next;
		},
		value: () => value,
	};
}

describe("flow preferences", () => {
	it("round-trips every generator field and enum choice without cross-problem contamination", () => {
		const storage = memoryStorage();
		const maxGenerator = {
			...DEFAULT_FLOW_GENERATOR,
			family: "grid-2d" as const,
			seed: "123456",
			primary: 4,
			secondary: 7,
			tertiary: 9,
			quaternary: 11,
			toggle: true,
			assignmentShape: "near-tie" as const,
			transportationShape: "block" as const,
			assignmentObjective: "maximize" as const,
			assignmentNoise: 9,
			gridgenTotalSupply: "37",
			gridgraphPreset: "wide" as const,
			washingtonPreset: "dense" as const,
			washingtonMatchingPreset: "medium" as const,
			washingtonSquareMeshPreset: "sparse" as const,
			netgenPreset: "custom" as const,
			netgenTotalSupply: "72",
			netgenTransshipmentSources: 2,
			netgenTransshipmentSinks: 3,
			netgenHighCostPercentage: 63,
			netgenCapacitatedPercentage: 57,
			capacityKind: "bimodal" as const,
			capacityMinimum: "2",
			capacityMaximum: "31",
			costKind: "capacity-correlated" as const,
			costMinimum: "-8",
			costMaximum: "13",
			costCorrelationDirection: "negative" as const,
			costMaximumJitter: "3",
		};
		const minGenerator = {
			...flowWorkbenchPolicy("min-cost-flow").defaultGenerator(),
			seed: "654321",
			netgenPreset: "custom" as const,
			netgenTotalSupply: "72",
			netgenTransshipmentSources: 2,
			netgenTransshipmentSinks: 2,
			netgenHighCostPercentage: 63,
			netgenCapacitatedPercentage: 57,
		};
		expect(
			validateRestoredFlowGenerator(maxGenerator, DEFAULT_FLOW_GENERATOR),
		).toBe(maxGenerator);
		expect(
			validateRestoredFlowGenerator(minGenerator, DEFAULT_FLOW_GENERATOR),
		).toBe(minGenerator);

		writeFlowPreferences(storage, "max-flow", {
			generator: maxGenerator,
			viewMode: "both",
			granularity: "micro",
			familyGroup: "structural",
		});
		writeFlowPreferences(storage, "min-cost-flow", {
			generator: minGenerator,
			viewMode: "residual",
			granularity: "phase",
			familyGroup: "special",
		});

		expect(
			readFlowPreferences(storage, "max-flow", DEFAULT_FLOW_GENERATOR),
		).toEqual({
			generator: maxGenerator,
			viewMode: "both",
			granularity: "micro",
			familyGroup: "structural",
		});
		expect(
			readFlowPreferences(storage, "min-cost-flow", DEFAULT_FLOW_GENERATOR),
		).toEqual({
			generator: minGenerator,
			viewMode: "residual",
			granularity: "phase",
			familyGroup: "special",
		});
	});

	it("persists shared generator size and problem-specific playback choices", () => {
		const storage = memoryStorage();
		writeFlowPreferences(storage, "max-flow", {
			generator: {
				...DEFAULT_FLOW_GENERATOR,
				primary: 6,
				seed: "123456",
			},
			viewMode: "both",
			granularity: "micro",
			familyGroup: "worst-case",
		});
		const restored = readFlowPreferences(
			storage,
			"max-flow",
			DEFAULT_FLOW_GENERATOR,
		);
		expect(restored.generator.primary).toBe(6);
		expect(restored.generator.seed).toBe("123456");
		expect(restored.generator.family).toBe("layered-dag");
		expect(restored.viewMode).toBe("both");
		expect(restored.granularity).toBe("micro");
		expect(restored.familyGroup).toBe("worst-case");
		const otherProblem = readFlowPreferences(storage, "min-cost-flow", {
			...DEFAULT_FLOW_GENERATOR,
			primary: 24,
		});
		expect(otherProblem.generator.primary).toBe(24);
		expect(otherProblem.familyGroup).toBe("all");
	});

	it("rejects corrupt and out-of-range values", () => {
		const corrupt = memoryStorage("{not-json");
		expect(
			readFlowPreferences(corrupt, "max-flow", DEFAULT_FLOW_GENERATOR),
		).toMatchObject({
			viewMode: "original",
			granularity: "operation",
			familyGroup: "all",
		});
		const invalid = memoryStorage(
			JSON.stringify({
				version: 2,
				generator: {
					"max-flow": { primary: 0, capacityMaximum: "NaN" },
				},
				viewMode: { "max-flow": "unknown" },
				familyGroup: { "max-flow": "unknown" },
			}),
		);
		const restored = readFlowPreferences(
			invalid,
			"max-flow",
			DEFAULT_FLOW_GENERATOR,
		);
		expect(restored.generator.primary).toBe(DEFAULT_FLOW_GENERATOR.primary);
		expect(restored.generator.capacityMaximum).toBe(
			DEFAULT_FLOW_GENERATOR.capacityMaximum,
		);
		expect(restored.familyGroup).toBe("all");
		const invalidEnums = memoryStorage(
			JSON.stringify({
				version: 2,
				generator: {
					"max-flow": {
						...DEFAULT_FLOW_GENERATOR,
						family: "unknown",
						capacityKind: "rainbow",
					},
				},
			}),
		);
		expect(
			readFlowPreferences(invalidEnums, "max-flow", DEFAULT_FLOW_GENERATOR)
				.generator,
		).toEqual(DEFAULT_FLOW_GENERATOR);
	});

	it("leaves family-specific validation to the lazy generator catalog", () => {
		const invalid = {
			...DEFAULT_FLOW_GENERATOR,
			secondary: 2,
			tertiary: 3,
		};
		expect(validateRestoredFlowGenerator(invalid, DEFAULT_FLOW_GENERATOR)).toBe(
			invalid,
		);
	});

	it("contains unavailable storage and rejects old versions", () => {
		const unavailable = {
			getItem: () => {
				throw new Error("storage disabled");
			},
			setItem: () => {
				throw new Error("storage disabled");
			},
		};
		expect(
			readFlowPreferences(unavailable, "max-flow", DEFAULT_FLOW_GENERATOR),
		).toMatchObject({ familyGroup: "all", viewMode: "original" });
		expect(() =>
			writeFlowPreferences(unavailable, "max-flow", {
				generator: DEFAULT_FLOW_GENERATOR,
				viewMode: "original",
				granularity: "operation",
				familyGroup: "all",
			}),
		).not.toThrow();
		const oldVersion = memoryStorage(JSON.stringify({ version: 1 }));
		expect(
			readFlowPreferences(oldVersion, "max-flow", DEFAULT_FLOW_GENERATOR),
		).toMatchObject({ familyGroup: "all", viewMode: "original" });
	});
});
