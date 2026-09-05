import { describe, expect, it } from "vitest";

import {
	assertFlowAlgorithmConfig,
	defaultFlowAlgorithmConfig,
	flowScenarioNodeIds,
} from "./flow-algorithm-config";
import { defaultFlowScenario } from "./flow-scenario";

const NODE_IDS = ["s", "a", "b", "c", "d", "t"] as const;
const zeroPotentials = () =>
	Object.fromEntries(NODE_IDS.map((node) => [node, "0"]));

describe("flow algorithm config boundary", () => {
	it("accepts the two closed non-empty runtime configs", () => {
		expect(() =>
			assertFlowAlgorithmConfig(
				"tardos-framework",
				{ potentials: zeroPotentials() },
				NODE_IDS,
			),
		).not.toThrow();
		expect(() =>
			assertFlowAlgorithmConfig(
				"prediction-assisted-epsilon-relaxation",
				{
					predicted_potentials: zeroPotentials(),
					scaling_parameter: 2,
				},
				NODE_IDS,
			),
		).not.toThrow();
	});

	it.each([
		["missing node", { potentials: { ...zeroPotentials(), t: undefined } }],
		["extra field", { potentials: zeroPotentials(), unexpected: true }],
		["noncanonical", { potentials: { ...zeroPotentials(), a: "+0" } }],
		[
			"outside i128",
			{ potentials: { ...zeroPotentials(), a: (1n << 127n).toString() } },
		],
	] as const)("rejects malformed Tardos config: %s", (_, config) => {
		expect(() =>
			assertFlowAlgorithmConfig("tardos-framework", config, NODE_IDS),
		).toThrow();
	});

	it.each([
		1,
		1.5,
		5,
		"2",
	])("rejects prediction scaling parameter %p", (scaling_parameter) => {
		expect(() =>
			assertFlowAlgorithmConfig(
				"prediction-assisted-epsilon-relaxation",
				{
					predicted_potentials: zeroPotentials(),
					scaling_parameter,
				},
				NODE_IDS,
			),
		).toThrow(/integer 2, 3, or 4/);
	});

	it("keeps every other executable algorithm on the empty-config contract", () => {
		expect(() =>
			assertFlowAlgorithmConfig("dinic", { potentials: {} }, NODE_IDS),
		).toThrow(/empty config/);
		expect(() =>
			assertFlowAlgorithmConfig("dinic", {}, NODE_IDS),
		).not.toThrow();
	});

	it("extracts the exact graph-node coverage from a Flow Scenario", () => {
		expect(flowScenarioNodeIds(defaultFlowScenario())).toEqual(NODE_IDS);
	});

	it("builds node-complete defaults from the edited Scenario rather than the displayed scene", () => {
		const editedNodeIds = ["draft-source", "draft-middle", "draft-sink"];
		expect(
			defaultFlowAlgorithmConfig("tardos-framework", editedNodeIds),
		).toEqual({
			potentials: {
				"draft-source": "0",
				"draft-middle": "0",
				"draft-sink": "0",
			},
		});
		expect(
			defaultFlowAlgorithmConfig(
				"prediction-assisted-epsilon-relaxation",
				editedNodeIds,
			),
		).toEqual({
			predicted_potentials: {
				"draft-source": "0",
				"draft-middle": "0",
				"draft-sink": "0",
			},
			scaling_parameter: 2,
		});
		expect(defaultFlowAlgorithmConfig("dinic", editedNodeIds)).toEqual({});
	});
});
