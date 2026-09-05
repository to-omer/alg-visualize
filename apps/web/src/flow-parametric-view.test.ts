import { describe, expect, it } from "vitest";
import {
	compareFlowRational,
	projectFlowParametricChart,
} from "./flow-parametric-view";
import type { FlowCurrentSceneV9 } from "./flow-scene";

function scene(): FlowCurrentSceneV9 {
	return {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "7",
		event_count: "11",
		solve_status: "running",
		model: {
			kind: "parametric-max-flow",
			source: "s",
			sink: "t",
			parameter: {
				minimum: { numerator: "-1", denominator: "2" },
				maximum: { numerator: "3", denominator: "2" },
			},
			capacity_slopes: [{ edge_id: "st", slope: "1" }],
		},
		graph: {
			nodes: [
				{ id: "s", supply: "0" },
				{ id: "t", supply: "0" },
			],
			edges: [
				{
					id: "st",
					from: "s",
					to: "t",
					lower: "0",
					capacity: "2",
					cost: "0",
				},
			],
		},
		algorithm: { id: "parametric-pseudoflow", config: {} },
		run_profile: "trace",
		trace_granularity: "operation",
		trace_steps: {
			phase_unit: "one parameter traversal phase",
			phase_availability: { availability: "available" },
			operation_unit: "one breakpoint traversal operation",
			operation_availability: { availability: "available" },
			detail: { availability: "unavailable", reason: "No detailed events." },
			primary_work: {
				metric_ordinal: 2,
				unit: "residual-arc inspections",
				abstraction: "primitive",
				visualization: "edge-field",
			},
		},
		edge_states: [],
		residual_arcs: [],
		node_trace_states: [],
		parametric_overlay: {
			stage: "ready",
			parameter: { numerator: "1", denominator: "2" },
			edge_capacities: [
				{
					edge_id: "st",
					capacity: { numerator: "5", denominator: "2" },
				},
			],
			visual_scale_max_capacity: { numerator: "7", denominator: "2" },
			recorded_segments: [
				{
					lower: { numerator: "-1", denominator: "2" },
					upper: { numerator: "1", denominator: "2" },
					intercept: "2",
					slope: "1",
					minimal_source_side: ["s"],
					maximal_source_side: ["s", "a"],
				},
				{
					lower: { numerator: "1", denominator: "2" },
					upper: { numerator: "3", denominator: "2" },
					intercept: "3",
					slope: "-1",
					minimal_source_side: ["s", "a"],
					maximal_source_side: ["s", "a"],
				},
			],
			recorded_breakpoints: [
				{
					parameter: { numerator: "1", denominator: "2" },
					before_source_side: ["s"],
					after_source_side: ["s", "a"],
					exact_minimal_source_side: ["s"],
					exact_maximal_source_side: ["s", "a"],
					entering_nodes: ["a"],
				},
			],
		},
		metrics: Array.from(
			{ length: 16 },
			() => "0",
		) as FlowCurrentSceneV9["metrics"],
	};
}

describe("parametric flow chart projection", () => {
	it("keeps exact breakpoint order and exposes tied cut intervals", () => {
		const projection = projectFlowParametricChart(scene());
		expect(projection).toBeDefined();
		expect(projection?.currentX).toBe(0.5);
		expect(projection?.segments).toHaveLength(2);
		expect(projection?.segments[0]?.tied).toBe(true);
		expect(projection?.segments[1]?.tied).toBe(false);
		expect(projection?.breakpoints).toEqual([
			expect.objectContaining({
				parameterLabel: "1/2",
				x: 0.5,
				enteringNodes: ["a"],
				tied: true,
			}),
		]);
	});

	it("compares huge adjacent rationals without converting them to Number", () => {
		const huge = "9".repeat(120);
		expect(
			compareFlowRational(
				{ numerator: huge, denominator: "7" },
				{ numerator: `${BigInt(huge) + 1n}`, denominator: "7" },
			),
		).toBe(-1);
	});
});
