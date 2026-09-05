import { describe, expect, it } from "vitest";

import { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import { buildFlowRenderPlan } from "./flow-render-plan";
import type { FlowCurrentSceneV9 } from "./flow-scene";

function scene(): FlowCurrentSceneV9 {
	return {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "1",
		event_count: "2",
		solve_status: "ready",
		model: { kind: "max-flow", source: "s", sink: "t" },
		graph: {
			nodes: [
				{ id: "s", supply: "0", position: { x: "80", y: "240" } },
				{ id: "t", supply: "0", position: { x: "880", y: "240" } },
			],
			edges: [
				{
					id: "e0",
					from: "s",
					to: "t",
					lower: "0",
					capacity: "7",
					cost: "3",
				},
			],
		},
		algorithm: { id: "edmonds-karp", config: {} },
		run_profile: "trace",
		trace_granularity: "operation",
		trace_steps: {
			phase_unit: "one residual-path search phase",
			phase_availability: { availability: "available" },
			operation_unit: "one completed augmentation",
			operation_availability: { availability: "available" },
			detail: {
				availability: "available",
				unit: "one residual-arc inspection",
			},
			primary_work: {
				metric_ordinal: 2,
				unit: "residual-arc inspections",
				abstraction: "primitive",
				visualization: "edge-field",
			},
		},
		edge_states: [{ edge_id: "e0", flow: "4" }],
		residual_arcs: [
			{
				edge_id: "e0",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "3",
				cost: "3",
				active: true,
				fixed: false,
			},
		],
		node_trace_states: [],
		metrics: Array.from(
			{ length: 16 },
			() => "0",
		) as FlowCurrentSceneV9["metrics"],
	};
}

describe("flow entity graph state", () => {
	it("projects scene data into typed geometry without renderer DTO reads", () => {
		const render = buildFlowRenderPlan(scene());
		if (render.kind !== "entities") throw new Error("expected entity plan");

		const state = projectFlowEntityGraphState({
			render,
			viewMode: "both",
			frameGroups: [],
		});

		expect(state.positions.get("s")).toEqual({ x: 80, y: 240 });
		expect(state.visibleResidualArcs).toHaveLength(1);
		expect(state.originalVisuals).toEqual([
			expect.objectContaining({
				flow: 4n,
				capacity: 7n,
				costKind: "positive",
			}),
		]);
	});
});
