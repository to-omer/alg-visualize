import { describe, expect, it } from "vitest";
import {
	flowWorkbenchModelRejectionMessage,
	flowWorkbenchPolicy,
} from "./flow-workbench-policy";

describe("flowWorkbenchPolicy", () => {
	it("keeps Max Flow cost-free across JSON, DSL, and generators", () => {
		const policy = flowWorkbenchPolicy("max-flow");
		expect(policy.showsCost).toBe(false);
		expect(policy.defaultScenario()).toContain('"kind": "max-flow"');
		expect(policy.defaultDsl()).toContain("model max-flow source=s sink=t");
		expect(policy.defaultGenerator()).toMatchObject({
			costKind: "zero",
			costMinimum: "0",
			costMaximum: "0",
		});
		expect(
			policy.normalizeGenerator({
				...policy.defaultGenerator(),
				family: "netgen-skeleton",
				netgenPreset: "single-source-max-flow",
			}),
		).toMatchObject({
			costKind: "constant",
			costMinimum: "1",
			costMaximum: "1",
		});
	});

	it("keeps Min-Cost Flow defaults cost-aware", () => {
		const policy = flowWorkbenchPolicy("min-cost-flow");
		expect(policy.showsCost).toBe(true);
		expect(policy.defaultScenario()).toContain('"kind": "fixed-flow-min-cost"');
		expect(policy.defaultDsl()).toContain("model fixed-flow-min-cost");
		expect(policy.defaultGenerator()).toMatchObject({
			family: "layered-dag",
			primary: 5,
			secondary: 4,
			tertiary: 2,
			quaternary: 5,
			costKind: "uniform",
			costMinimum: "-3",
			costMaximum: "5",
		});
	});

	it("owns model admission while adapting generic topology families", () => {
		const maxFlow = flowWorkbenchPolicy("max-flow");
		const minCostFlow = flowWorkbenchPolicy("min-cost-flow");

		expect(maxFlow.acceptsModel("max-flow")).toBe(true);
		expect(maxFlow.acceptsModel("bipartite-matching")).toBe(true);
		expect(maxFlow.acceptsModel("transshipment")).toBe(false);
		expect(maxFlow.acceptsGeneratorFamily("layered-dag")).toBe(true);
		expect(maxFlow.acceptsGeneratorFamily("netgen-skeleton")).toBe(true);
		expect(minCostFlow.acceptsModel("transportation")).toBe(true);
		expect(minCostFlow.acceptsModel("planar-max-flow")).toBe(false);
		expect(minCostFlow.acceptsGeneratorFamily("transportation-table")).toBe(
			true,
		);
		expect(minCostFlow.acceptsGeneratorFamily("layered-dag")).toBe(true);
	});

	it("rejects a cross-workspace model with the shared ingestion message", () => {
		expect(
			flowWorkbenchModelRejectionMessage(
				flowWorkbenchPolicy("max-flow"),
				"transshipment",
			),
		).toBe("This input belongs in the Min-Cost Flow workspace.");
		expect(
			flowWorkbenchModelRejectionMessage(
				flowWorkbenchPolicy("min-cost-flow"),
				"max-flow",
			),
		).toBe("This input belongs in the Max Flow workspace.");
		expect(
			flowWorkbenchModelRejectionMessage(
				flowWorkbenchPolicy("max-flow"),
				"unsupported-model",
			),
		).toBeUndefined();
	});

	it("preserves adaptable restored families and only resets incompatible models", () => {
		const maxFlow = flowWorkbenchPolicy("max-flow");
		const minCostFlow = flowWorkbenchPolicy("min-cost-flow");
		expect(maxFlow.restoreGenerator(minCostFlow.defaultGenerator())).toEqual(
			maxFlow.normalizeGenerator(minCostFlow.defaultGenerator()),
		);
		expect(minCostFlow.restoreGenerator(maxFlow.defaultGenerator())).toEqual(
			maxFlow.defaultGenerator(),
		);
		expect(
			maxFlow.restoreGenerator({
				...minCostFlow.defaultGenerator(),
				family: "assignment-matrix",
			}),
		).toEqual(maxFlow.defaultGenerator());

		const compatible = {
			...maxFlow.defaultGenerator(),
			seed: "99",
			costKind: "uniform" as const,
			costMinimum: "-7",
			costMaximum: "9",
		};
		expect(maxFlow.restoreGenerator(compatible)).toMatchObject({
			family: compatible.family,
			seed: "99",
			costKind: "zero",
			costMinimum: "0",
			costMaximum: "0",
		});
	});
});
