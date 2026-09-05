import { describe, expect, it } from "vitest";

import { assertFlowCurrentSceneV9Wire } from "./flow-scene-wire/decode-v9";
import { FLOW_SCENE_V9_OVERLAY_DECODERS } from "./flow-scene-wire/generated/overlays";

function minimalWireScene(): Record<string, unknown> {
	return {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "0",
		event_count: "0",
		solve_status: "ready",
		model: { kind: "circulation" },
		graph: { nodes: [], edges: [] },
		algorithm: { id: "simple-cycle-canceling", config: {} },
		run_profile: "trace",
		trace_granularity: "phase",
		trace_steps: {
			phase_unit: "one negative-cycle search phase",
			phase_availability: { availability: "available" },
			operation_unit: "one complete cycle cancellation",
			operation_availability: { availability: "available" },
			detail: {
				availability: "unavailable",
				reason: "This trace records complete cycle operations only.",
			},
			primary_work: {
				metric_ordinal: 1,
				unit: "Bellman–Ford relaxation passes",
				abstraction: "primitive",
				visualization: "edge-field",
			},
		},
		edge_states: [],
		residual_arcs: [],
		node_trace_states: [],
		metrics: Array.from({ length: 16 }, () => "0"),
	};
}

describe("Rust-generated flow scene V9 wire contract", () => {
	it("accepts the current root shape and rejects unknown root fields", () => {
		const valid = minimalWireScene();
		expect(() => assertFlowCurrentSceneV9Wire(valid)).not.toThrow();

		const unknown = { ...valid, future: true };
		expect(() => assertFlowCurrentSceneV9Wire(unknown)).toThrowError(
			/unknown field future/,
		);
	});

	it("composes every generated overlay module and rejects overlay drift", () => {
		const fields = FLOW_SCENE_V9_OVERLAY_DECODERS.map(([field]) => field);
		expect(fields.length).toBeGreaterThan(0);
		expect(new Set(fields).size).toBe(fields.length);
		expect(fields.every((field) => field.endsWith("_overlay"))).toBe(true);
		const scene = {
			...minimalWireScene(),
			binary_blocking_overlay: {
				stage: "analyzed",
				upper_bound: "1",
				delta: "1",
				delivered: "0",
				nodes: [],
				base_zero_arcs: [],
				special_arcs: [],
				admissible_arcs: [],
				zero_admissible_arcs: [],
			},
		};
		expect(() => assertFlowCurrentSceneV9Wire(scene)).not.toThrow();
		(
			scene.binary_blocking_overlay as Record<string, unknown>
		).future_overlay_field = true;
		expect(() => assertFlowCurrentSceneV9Wire(scene)).toThrowError(
			/unknown field future_overlay_field/,
		);
	});

	it("enforces Rust-owned literal unions and serialized optional fields", () => {
		const wrongDirection = {
			...minimalWireScene(),
			residual_arcs: [
				{
					edge_id: "e",
					direction: "sideways",
					from: "s",
					to: "t",
					capacity: "1",
					cost: "0",
					active: false,
				},
			],
		};
		expect(() => assertFlowCurrentSceneV9Wire(wrongDirection)).toThrowError(
			/unknown enum value/,
		);

		const omittedEmptyEligibleArcs = {
			...minimalWireScene(),
			convex_cost_overlay: {
				stage: "initialize",
				edges: [],
				active_cycle: [],
			},
		};
		expect(() =>
			assertFlowCurrentSceneV9Wire(omittedEmptyEligibleArcs),
		).not.toThrow();

		const nullableOutcome = {
			...minimalWireScene(),
			outcome: {
				kind: "minimum-ratio-cycle",
				ratio: null,
				cycle: [],
				simple_cycles: "0",
				enumerated_vectors: "0",
			},
		};
		expect(() => assertFlowCurrentSceneV9Wire(nullableOutcome)).toThrowError(
			/expected one union member/,
		);
	});

	it("rejects legacy revisions at the V9 structural boundary", () => {
		const legacy = {
			...minimalWireScene(),
			result_schema_version: 7,
			frame_revision: "flow-scene/7",
		};
		expect(() => assertFlowCurrentSceneV9Wire(legacy)).toThrowError(
			/wrong constant/,
		);
	});
});
