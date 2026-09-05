import { describe, expect, it } from "vitest";

import {
	feasibilityWorkRows,
	parametricMetricRows,
} from "./flow-inspector-summary";
import type { FlowCurrentSceneV9 } from "./flow-scene";

function scene(
	metrics: Extract<
		NonNullable<FlowCurrentSceneV9["outcome"]>,
		{ kind: "parametric-max-flow" }
	>["metrics"],
): FlowCurrentSceneV9 {
	return {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "0",
		event_count: "0",
		solve_status: "optimal",
		model: {
			kind: "parametric-max-flow",
			source: "s",
			sink: "t",
			parameter: {
				minimum: { numerator: "0", denominator: "1" },
				maximum: { numerator: "1", denominator: "1" },
			},
			capacity_slopes: [],
		},
		graph: {
			nodes: [
				{ id: "s", supply: "0" },
				{ id: "t", supply: "0" },
			],
			edges: [],
		},
		algorithm: { id: "parametric-pseudoflow", config: {} },
		run_profile: "fast",
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
		outcome: {
			kind: "parametric-max-flow",
			segments: [],
			breakpoints: [],
			metrics,
		},
		metrics: Array.from(
			{ length: 16 },
			() => "0",
		) as FlowCurrentSceneV9["metrics"],
	};
}

describe("flow inspector summary", () => {
	it("keeps Fast feasibility work separate and compact", () => {
		const value = scene({
			implementation: "breakpoint-rerun",
			pseudoflow_runs: "0",
			oracle_runs: "0",
			static_residual_arc_scans: "0",
			intersections: "0",
			subproblems: "0",
			segments: "0",
			breakpoints: "0",
			simultaneous_breakpoints: "0",
			maximum_depth: "0",
		});
		value.feasibility_work = {
			invocations: "2",
			metrics: {
				original_edge_inspections: "10",
				original_node_inspections: "8",
				auxiliary_adjacency_inspections: "24",
				pushes: "5",
				relabels: "3",
				active_node_selections: "4",
				discharges: "4",
				cut_adjacency_inspections: "7",
				extracted_original_edges: "10",
			},
		};
		expect(feasibilityWorkRows(value)).toEqual([
			{
				label: "Feasibility prepass",
				value: "2 runs · 10 input-edge scans · 8 input-node scans",
			},
			{
				label: "Auxiliary routing work",
				value:
					"24 adjacency scans · 5 pushes · 3 relabels · 4 active selections · 4 discharges",
			},
			{
				label: "Feasibility certificate work",
				value: "7 cut scans · 10 extracted-edge checks",
			},
		]);
		expect(feasibilityWorkRows(undefined)).toEqual([]);
	});

	it.each([
		[
			"parametric-pseudoflow",
			{
				implementation: "parametric-pseudoflow",
				forest_initializations: "2",
				parameter_advances: "3",
				forest_reuses: "1",
				renormalization_pushes: "4",
				renormalization_splits: "5",
				mergers: "6",
				relabels: "7",
				free_run_races: "8",
				forward_race_wins: "9",
				reverse_race_wins: "10",
				cooperative_race_steps: "11",
				contraction_views: "12",
				smaller_child_restarts: "13",
				larger_child_continuations: "14",
				maximum_depth: "15",
				residual_arc_scans: "160",
			},
			8,
		],
		[
			"breakpoint-rerun",
			{
				implementation: "breakpoint-rerun",
				pseudoflow_runs: "2",
				oracle_runs: "3",
				static_residual_arc_scans: "40",
				intersections: "4",
				subproblems: "5",
				segments: "6",
				breakpoints: "7",
				simultaneous_breakpoints: "8",
				maximum_depth: "9",
			},
			6,
		],
	] as const)("projects %s metrics", (_implementation, metrics, expectedRows) => {
		expect(parametricMetricRows(scene(metrics))).toHaveLength(expectedRows);
	});
});
