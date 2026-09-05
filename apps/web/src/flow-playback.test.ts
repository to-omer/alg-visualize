import { describe, expect, it } from "vitest";
import {
	createFlowBoundaryInventory,
	flowAdjacentVisibleBoundary,
	flowEffectivePlaybackGranularity,
	flowKnownRawPrefixEnd,
	flowSceneVisibleAtGranularity,
	flowVisibleBoundaryPositions,
	recordFlowBoundary,
	resetFlowBoundaryInventory,
} from "./flow-playback";
import type { FlowCurrentSceneV9 } from "./flow-scene";

function scene(
	granularity: "phase" | "operation" | "micro",
	status = "running",
) {
	const value = {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "1",
		event_count: "4",
		solve_status: status,
		model: { kind: "max-flow", source: "s", sink: "t" },
		graph: {
			nodes: [
				{ id: "s", supply: "0" },
				{ id: "t", supply: "0" },
			],
			edges: [],
		},
		algorithm: { id: "edmonds-karp", config: {} },
		run_profile: "trace",
		trace_granularity: "operation",
		edge_states: [],
		residual_arcs: [],
		node_trace_states: [],
		metrics: [],
		trace_event: {
			event_id: "1",
			catalog_id: "test.event",
			minimum_granularity: granularity,
			entity_refs: [],
			patch_count: 0,
			pseudocode_line: "test",
		},
	};
	return value as unknown as FlowCurrentSceneV9;
}

describe("flow playback visibility", () => {
	const available = { availability: "available" } as const;
	const detailAvailable = {
		availability: "available",
		unit: "one detailed primitive",
	} as const;
	const primaryWork = {
		metric_ordinal: 2,
		unit: "residual-arc inspections",
		abstraction: "primitive",
		visualization: "edge-field",
	} as const;
	const unavailable = (reason: string) =>
		({ availability: "unavailable", reason }) as const;

	it("keeps the preference while choosing the nearest supported boundary", () => {
		expect(
			flowEffectivePlaybackGranularity("micro", {
				phase_unit: "one phase",
				phase_availability: available,
				operation_unit: "one operation",
				operation_availability: available,
				detail: unavailable("Detail is not recorded."),
				primary_work: primaryWork,
			}),
		).toBe("operation");
		expect(
			flowEffectivePlaybackGranularity("operation", {
				phase_unit: "one phase",
				phase_availability: available,
				operation_unit: "one operation",
				operation_availability: unavailable("Operations are aggregated."),
				detail: detailAvailable,
				primary_work: primaryWork,
			}),
		).toBe("phase");
		expect(
			flowEffectivePlaybackGranularity("phase", {
				phase_unit: "one phase",
				phase_availability: unavailable("This trace has no phase boundary."),
				operation_unit: "one operation",
				operation_availability: available,
				detail: unavailable("Detail is not recorded."),
				primary_work: primaryWork,
			}),
		).toBe("operation");
	});

	it("rejects an endpoint that records no boundary kind", () => {
		expect(() =>
			flowEffectivePlaybackGranularity("operation", {
				phase_unit: "one phase",
				phase_availability: unavailable("No phase."),
				operation_unit: "one operation",
				operation_availability: unavailable("No operation."),
				detail: unavailable("No detail."),
				primary_work: primaryWork,
			}),
		).toThrow("records no playback boundary kind");
	});

	it("uses operation as semantic playback and exposes all events in micro", () => {
		expect(flowSceneVisibleAtGranularity(scene("phase"), "operation")).toBe(
			true,
		);
		expect(flowSceneVisibleAtGranularity(scene("operation"), "operation")).toBe(
			true,
		);
		expect(flowSceneVisibleAtGranularity(scene("micro"), "operation")).toBe(
			false,
		);
		expect(flowSceneVisibleAtGranularity(scene("micro"), "micro")).toBe(true);
	});

	it("always exposes terminal boundaries", () => {
		expect(
			flowSceneVisibleAtGranularity(scene("micro", "optimal"), "phase"),
		).toBe(true);
	});

	it("builds stable semantic and phase ordinals from raw boundaries", () => {
		const boundaries = new Map([
			[1, "phase"],
			[2, "micro"],
			[3, "operation"],
			[4, "micro"],
			[5, "phase"],
		] as const);
		expect(flowVisibleBoundaryPositions(boundaries, "operation", 6)).toEqual([
			0, 1, 3, 5, 6,
		]);
		expect(flowVisibleBoundaryPositions(boundaries, "phase", 6)).toEqual([
			0, 1, 5, 6,
		]);
	});

	it("jumps between indexed semantic boundaries in either direction", () => {
		const positions = [0, 1, 5, 16];
		expect(flowAdjacentVisibleBoundary(positions, 1, 1)).toBe(5);
		expect(flowAdjacentVisibleBoundary(positions, 3, 1)).toBe(5);
		expect(flowAdjacentVisibleBoundary(positions, 5, -1)).toBe(1);
		expect(flowAdjacentVisibleBoundary(positions, 3, -1)).toBe(1);
		expect(flowAdjacentVisibleBoundary(positions, 16, 1)).toBeUndefined();
		expect(flowAdjacentVisibleBoundary(positions, 0, -1)).toBeUndefined();
	});

	it("reports only the contiguous raw inventory prefix as ordinal-safe", () => {
		expect(
			flowKnownRawPrefixEnd(
				new Map([
					[1, "phase"],
					[2, "micro"],
					[4, "operation"],
				]),
				6,
			),
		).toBe(2);
		expect(
			flowKnownRawPrefixEnd(
				new Map([
					[1, "phase"],
					[2, "micro"],
					[3, "operation"],
				]),
				3,
			),
		).toBe(3);
	});

	it("maintains semantic boundary indexes and the raw prefix incrementally", () => {
		const inventory = createFlowBoundaryInventory();
		recordFlowBoundary(inventory, 0, 6, undefined);
		recordFlowBoundary(inventory, 1, 6, "phase");
		recordFlowBoundary(inventory, 3, 6, "operation");
		recordFlowBoundary(inventory, 2, 6, "micro");
		recordFlowBoundary(inventory, 5, 6, "phase");
		expect(inventory.prefixEnd).toBe(3);
		expect(inventory.phasePositions).toEqual([0, 1, 5, 6]);
		expect(inventory.operationPositions).toEqual([0, 1, 3, 5, 6]);

		recordFlowBoundary(inventory, 4, 6, "micro");
		recordFlowBoundary(inventory, 6, 6, "phase");
		expect(inventory.prefixEnd).toBe(6);
		expect(inventory.minimumByRawPosition.size).toBe(6);

		resetFlowBoundaryInventory(inventory);
		expect(inventory).toEqual({
			minimumByRawPosition: new Map(),
			phasePositions: [0],
			operationPositions: [0],
			extent: 0,
			prefixEnd: 0,
		});
	});
});
