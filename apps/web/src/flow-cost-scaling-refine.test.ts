import { describe, expect, it } from "vitest";
import { projectFlowCostScalingRefineBoundary } from "./flow-cost-scaling-refine";
import type { FlowCurrentSceneV9 } from "./flow-scene";

const states = new Map([
	["a", { node_id: "a", label: "0" }],
	["b", { node_id: "b", label: "7" }],
	["c", { node_id: "c", label: "0" }],
]);

const arcs = [
	{
		edge_id: "ab",
		direction: "forward",
		from: "a",
		to: "b",
		capacity: "3",
		cost: "1",
		active: false,
	},
	{
		edge_id: "bc",
		direction: "forward",
		from: "b",
		to: "c",
		capacity: "2",
		cost: "0",
		active: false,
	},
	{
		edge_id: "ca",
		direction: "forward",
		from: "c",
		to: "a",
		capacity: "1",
		cost: "2",
		active: false,
	},
] satisfies FlowCurrentSceneV9["residual_arcs"];

function event(catalogId: string, epsilon = "4") {
	return {
		event_id: "1",
		catalog_id: catalogId,
		minimum_granularity: "phase",
		pseudocode_line: "cost-scaling:test",
		patch_count: 1,
		entity_refs: [],
		detail: { label: "epsilon", value: epsilon },
	} satisfies NonNullable<FlowCurrentSceneV9["trace_event"]>;
}

describe("cost-scaling refine projection", () => {
	it("shows only exact negative saturation candidates at phase start", () => {
		const projection = projectFlowCostScalingRefineBoundary(
			event("cost-scaling.start-refine"),
			arcs,
			states,
			3,
		);
		expect([...(projection?.arcs.entries() ?? [])]).toEqual([
			[
				"ab:forward",
				{ className: "negative", reducedCost: -3n, witness: true },
			],
		]);
	});

	it("anchors completion to the minimum reduced-cost certificate witness", () => {
		const projection = projectFlowCostScalingRefineBoundary(
			event("price-refinement.complete-refine", "3"),
			arcs,
			states,
			3,
		);
		expect(projection?.kind).toBe("complete");
		expect([...(projection?.arcs.entries() ?? [])]).toEqual([
			[
				"ab:forward",
				{ className: "certificate", reducedCost: -3n, witness: true },
			],
		]);
	});

	it("fails loudly when a source boundary omits prices or epsilon", () => {
		const { detail: _detail, ...missingEpsilon } = event(
			"arc-fixing.start-refine",
		);
		expect(() =>
			projectFlowCostScalingRefineBoundary(missingEpsilon, arcs, states, 3),
		).toThrow(/exact epsilon/);
		expect(() =>
			projectFlowCostScalingRefineBoundary(
				event("augment-relabel.start-refine"),
				arcs,
				new Map(),
				3,
			),
		).toThrow(/node price/);
	});
});
