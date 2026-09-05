import { describe, expect, it } from "vitest";
import {
	applyNetgenPreset,
	classifyNetgenForm,
	DEFAULT_FLOW_GENERATOR,
} from "./flow-generator-family-registry";
import { FLOW_GENERATOR_FAMILY_IDS } from "./flow-generator-fixture";
import {
	flowFixtureDisabledReason,
	flowGeneratorFamilySupportsProblem,
	flowGeneratorFormAdaptationLabel,
	flowGeneratorFormDisabledReason,
	flowGeneratorFormMatchesProblem,
	flowGeneratorTargetProblem,
	flowInputModelKind,
	flowInputWorkbenchProblem,
	flowModelKindWorkbenchProblem,
	flowNetgenPresetDisabledReason,
} from "./flow-workbench-problem";

describe("flow workbench problem routing", () => {
	it("keeps max-flow-like models apart from cost-flow models", () => {
		expect(flowModelKindWorkbenchProblem("max-flow")).toBe("max-flow");
		expect(flowModelKindWorkbenchProblem("planar-max-flow")).toBe("max-flow");
		expect(flowModelKindWorkbenchProblem("bipartite-matching")).toBe(
			"max-flow",
		);
		expect(flowModelKindWorkbenchProblem("fixed-flow-min-cost")).toBe(
			"min-cost-flow",
		);
		expect(flowModelKindWorkbenchProblem("transportation")).toBe(
			"min-cost-flow",
		);
		expect(flowModelKindWorkbenchProblem("typo")).toBeUndefined();
	});

	it("reads only the JSON and DSL model discriminator", () => {
		expect(
			flowInputModelKind(
				JSON.stringify({ payload: { model: { kind: "min-cost-max-flow" } } }),
				"json",
			),
		).toBe("min-cost-max-flow");
		expect(
			flowInputModelKind(
				"# comment\nmodel parametric-max-flow source=s sink=t\n",
				"dsl",
			),
		).toBe("parametric-max-flow");
		expect(
			flowInputWorkbenchProblem(
				JSON.stringify({ payload: { model: { kind: "min-cost-max-flow" } } }),
				"json",
			),
		).toBe("min-cost-flow");
		expect(
			flowInputWorkbenchProblem(
				"# comment\nmodel parametric-max-flow source=s sink=t\n",
				"dsl",
			),
		).toBe("max-flow");
		expect(flowInputWorkbenchProblem("{", "json")).toBeUndefined();
		expect(flowInputWorkbenchProblem("node s", "dsl")).toBeUndefined();
		expect(
			flowInputWorkbenchProblem("model typo source=s sink=t", "dsl"),
		).toBeUndefined();
		expect(
			flowInputWorkbenchProblem(
				JSON.stringify({ payload: { model: { kind: "typo" } } }),
				"json",
			),
		).toBeUndefined();
	});

	it("shares topology families while keeping model-specific tables scoped", () => {
		expect(flowGeneratorFamilySupportsProblem("layered-dag", "max-flow")).toBe(
			true,
		);
		expect(
			flowGeneratorFamilySupportsProblem("netgen-skeleton", "max-flow"),
		).toBe(true);
		expect(
			flowGeneratorFamilySupportsProblem("netgen-skeleton", "min-cost-flow"),
		).toBe(true);
		expect(
			flowGeneratorFamilySupportsProblem("planar-triangulated", "max-flow"),
		).toBe(true);
		expect(
			flowGeneratorFamilySupportsProblem("layered-dag", "min-cost-flow"),
		).toBe(true);
		expect(flowGeneratorFamilySupportsProblem("cycle", "max-flow")).toBe(true);
		expect(
			flowGeneratorFamilySupportsProblem("assignment-matrix", "max-flow"),
		).toBe(false);
	});

	it("keeps the cross-workspace family policy closed over all 50 families", () => {
		expect(
			FLOW_GENERATOR_FAMILY_IDS.filter(
				(family) => !flowGeneratorFamilySupportsProblem(family, "max-flow"),
			),
		).toEqual(["assignment-matrix", "transportation-table"]);
		expect(
			FLOW_GENERATOR_FAMILY_IDS.filter(
				(family) =>
					!flowGeneratorFamilySupportsProblem(family, "min-cost-flow"),
			),
		).toEqual([]);
	});

	it("keeps cross-problem families discoverable with a stable disabled reason", () => {
		const fixture = {
			family_id: "assignment-matrix",
			model: "assignment",
		} as Parameters<typeof flowFixtureDisabledReason>[0];
		expect(flowFixtureDisabledReason(fixture, "min-cost-flow")).toBeUndefined();
		expect(flowFixtureDisabledReason(fixture, "max-flow")).toBe(
			"Generates Min-Cost Flow scenarios",
		);
	});

	it("uses the Rust-compatible NETGEN classifier for presets and custom forms", () => {
		const maxFlow = applyNetgenPreset(
			DEFAULT_FLOW_GENERATOR,
			"single-source-max-flow",
		);
		expect(flowGeneratorFormMatchesProblem(maxFlow, "max-flow")).toBe(true);
		expect(flowGeneratorFormMatchesProblem(maxFlow, "min-cost-flow")).toBe(
			true,
		);
		expect(
			flowGeneratorFormDisabledReason(maxFlow, "min-cost-flow"),
		).toBeUndefined();
		expect(flowGeneratorTargetProblem(maxFlow, "min-cost-flow")).toBe(
			"fixed-flow-min-cost",
		);
		expect(
			flowGeneratorFormAdaptationLabel(maxFlow, "max-flow"),
		).toBeUndefined();
		expect(flowGeneratorFormAdaptationLabel(maxFlow, "min-cost-flow")).toBe(
			"Topology adapted to fixed-flow Min-Cost Flow",
		);

		const oneByOneAssignment = {
			...maxFlow,
			primary: 2,
			secondary: 1,
			tertiary: 1,
			quaternary: 1,
			netgenTotalSupply: "1",
		};
		expect(classifyNetgenForm(oneByOneAssignment)).toBe("assignment");
		expect(
			flowGeneratorFormMatchesProblem(oneByOneAssignment, "max-flow"),
		).toBe(true);
		expect(
			flowGeneratorFormMatchesProblem(oneByOneAssignment, "min-cost-flow"),
		).toBe(true);
		expect(flowGeneratorTargetProblem(oneByOneAssignment, "max-flow")).toBe(
			"max-flow",
		);
		expect(
			flowGeneratorFormAdaptationLabel(oneByOneAssignment, "max-flow"),
		).toBe("Topology adapted to source/sink Max Flow");

		expect(
			flowNetgenPresetDisabledReason("single-source-max-flow", "min-cost-flow"),
		).toBe("Preset belongs to Max Flow");
		expect(flowNetgenPresetDisabledReason("general-min-cost", "max-flow")).toBe(
			"Preset belongs to Min-Cost Flow",
		);
		expect(
			flowNetgenPresetDisabledReason("custom", "max-flow"),
		).toBeUndefined();
	});
});
