import { describe, expect, it } from "vitest";

import {
	buildBorradaileKleinPlanarDualOverlay,
	buildHassinPlanarDualOverlay,
} from "./flow-planar-dual";
import { decodeFlowCurrentSceneV9 } from "./flow-scene";

function triangleScene() {
	const edges = [
		{ id: "ab", from: "a", to: "b", lower: "0", capacity: "5", cost: "0" },
		{ id: "ac", from: "a", to: "c", lower: "0", capacity: "2", cost: "0" },
		{ id: "bc", from: "b", to: "c", lower: "0", capacity: "3", cost: "0" },
	];
	const value = {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "3",
		event_count: "6",
		solve_status: "running",
		model: {
			kind: "planar-max-flow",
			source: "a",
			sink: "c",
			embedding: {
				rotations: [
					{
						node_id: "a",
						darts: [
							{ edge_id: "ab", direction: "forward" },
							{ edge_id: "ac", direction: "forward" },
						],
					},
					{
						node_id: "b",
						darts: [
							{ edge_id: "ab", direction: "reverse" },
							{ edge_id: "bc", direction: "forward" },
						],
					},
					{
						node_id: "c",
						darts: [
							{ edge_id: "bc", direction: "reverse" },
							{ edge_id: "ac", direction: "reverse" },
						],
					},
				],
				outer_face: { edge_id: "ab", direction: "reverse" },
				terminal_corners: {
					source: { edge_id: "ac", direction: "forward" },
					sink: { edge_id: "bc", direction: "reverse" },
				},
			},
		},
		graph: {
			nodes: ["a", "b", "c"].map((id) => ({ id, supply: "0" })),
			edges,
		},
		algorithm: { id: "hassin-st-planar", config: {} },
		run_profile: "trace",
		trace_granularity: "operation",
		trace_steps: {
			phase_unit: "one structure-specific search phase",
			phase_availability: { availability: "available" },
			operation_unit: "one planar-dual settlement or augmentation operation",
			operation_availability: { availability: "available" },
			detail: {
				availability: "unavailable",
				reason:
					"This planar-dual trace records complete structural operations only.",
			},
			primary_work: {
				metric_ordinal: 2,
				unit: "residual-arc inspections",
				abstraction: "primitive",
				visualization: "edge-field",
			},
		},
		edge_states: edges.map((edge) => ({ edge_id: edge.id, flow: "0" })),
		residual_arcs: edges.flatMap((edge) => [
			{
				edge_id: edge.id,
				direction: "forward",
				from: edge.from,
				to: edge.to,
				capacity: edge.capacity,
				cost: "0",
				active: edge.id === "bc",
			},
			{
				edge_id: edge.id,
				direction: "reverse",
				from: edge.to,
				to: edge.from,
				capacity: "0",
				cost: "0",
				active: false,
			},
		]),
		node_trace_states: ["a", "b", "c"].map((node_id) => ({ node_id })),
		trace_event: {
			event_id: "3",
			catalog_id: "hassin-st-planar.settle-dual-face",
			minimum_granularity: "operation",
			pseudocode_line: "hassin-st-planar:dijkstra-settle-face",
			patch_count: 1,
			entity_refs: [],
			detail: { label: "dual-distance", value: "3" },
		},
		trace_event_semantics: {
			role: "mutate",
			work_deltas: [{ unit: "published-transition", count: "1" }],
			aggregation_count: "1",
			work_progress: {
				detail_completed: "0",
				detail_total: "0",
				primary_completed: "8",
				primary_total: "8",
			},
			changed_entity_refs: [],
		},
		metrics: [
			"0",
			"0",
			"8",
			"0",
			"1",
			"3",
			"0",
			"0",
			"0",
			"0",
			"0",
			"0",
			"0",
			"0",
			"0",
			"3",
		],
	};
	return decodeFlowCurrentSceneV9(
		new TextEncoder().encode(JSON.stringify(value)),
	);
}

describe("Hassin split-dual overlay", () => {
	it("materializes the split dual at the split event, not in the ready frame", () => {
		const scene = triangleScene();
		scene.solve_status = "ready";
		delete scene.trace_event;
		expect(buildHassinPlanarDualOverlay(scene, new Map())).toBeUndefined();

		scene.solve_status = "running";
		scene.trace_event = {
			event_id: "1",
			catalog_id: "hassin-st-planar.split-outer-face",
			minimum_granularity: "phase",
			pseudocode_line: "hassin-st-planar:split-terminal-face",
			patch_count: 0,
			entity_refs: [],
		};
		const positions = new Map([
			["a", { x: 100, y: 100 }],
			["b", { x: 320, y: 80 }],
			["c", { x: 300, y: 280 }],
		]);
		expect(buildHassinPlanarDualOverlay(scene, positions)).toBeDefined();
	});

	it("splits the common face and maps the active primal crossing to its dual face", () => {
		const scene = triangleScene();
		const positions = new Map([
			["a", { x: 100, y: 100 }],
			["b", { x: 320, y: 80 }],
			["c", { x: 300, y: 280 }],
		]);
		const overlay = buildHassinPlanarDualOverlay(scene, positions);
		expect(overlay).toBeDefined();
		expect(overlay?.faces).toHaveLength(3);
		expect(overlay?.edges).toHaveLength(3);
		expect(overlay?.faces.filter((face) => face.active)).toEqual([
			expect.objectContaining({ role: "sink-side", distance: "3" }),
		]);
		expect(overlay?.edges.find((edge) => edge.edgeId === "bc")).toEqual(
			expect.objectContaining({
				forwardLength: "3",
				reverseLength: "0",
				activeDirection: "forward",
			}),
		);
		const source = overlay?.faces.find((face) => face.role === "source-side");
		const sink = overlay?.faces.find((face) => face.role === "sink-side");
		expect(
			Math.hypot(
				(source?.x ?? 0) - (sink?.x ?? 0),
				(source?.y ?? 0) - (sink?.y ?? 0),
			),
		).toBeGreaterThanOrEqual(42);
		for (let first = 0; first < (overlay?.faces.length ?? 0); first += 1) {
			for (
				let second = first + 1;
				second < (overlay?.faces.length ?? 0);
				second += 1
			) {
				const left = overlay?.faces[first];
				const right = overlay?.faces[second];
				expect(
					Math.hypot(
						(left?.x ?? 0) - (right?.x ?? 0),
						(left?.y ?? 0) - (right?.y ?? 0),
					),
				).toBeGreaterThanOrEqual(42);
			}
		}
	});

	it("does not claim a dual overlay for another algorithm", () => {
		const scene = triangleScene();
		scene.algorithm.id = "edmonds-karp";
		expect(buildHassinPlanarDualOverlay(scene, new Map())).toBeUndefined();
	});

	it("keeps dual-arc labels clear of graph annotations when a free candidate exists", () => {
		const scene = triangleScene();
		const positions = new Map([
			["a", { x: 100, y: 100 }],
			["b", { x: 320, y: 80 }],
			["c", { x: 300, y: 280 }],
		]);
		const obstacles = [
			{ left: 170, right: 250, top: 65, bottom: 105, weight: 1_000_000 },
			{ left: 190, right: 270, top: 110, bottom: 150, weight: 10_000 },
		];
		const overlay = buildHassinPlanarDualOverlay(scene, positions, obstacles);
		expect(overlay).toBeDefined();
		for (const edge of overlay?.edges ?? []) {
			const label = `${edge.forwardLength} → / 0 ←`;
			const width = Math.max(40, label.length * 5.2 + 8);
			const box = {
				left: edge.labelX - width / 2,
				right: edge.labelX + width / 2,
				top: edge.labelY - 12,
				bottom: edge.labelY + 2,
			};
			for (const obstacle of obstacles) {
				expect(
					Math.max(
						0,
						Math.min(box.right, obstacle.right) -
							Math.max(box.left, obstacle.left),
					) *
						Math.max(
							0,
							Math.min(box.bottom, obstacle.bottom) -
								Math.max(box.top, obstacle.top),
						),
				).toBe(0);
			}
		}
	});
});

describe("Borradaile–Klein unsplit-dual overlay", () => {
	it("keeps the designated infinite face unsplit and separates coincident face centers", () => {
		const scene = triangleScene();
		scene.algorithm.id = "borradaile-klein-planar";
		const overlay = buildBorradaileKleinPlanarDualOverlay(
			scene,
			new Map([
				["a", { x: 100, y: 100 }],
				["b", { x: 320, y: 80 }],
				["c", { x: 300, y: 280 }],
			]),
		);
		expect(overlay?.kind).toBe("borradaile-klein-unsplit");
		expect(overlay?.faces).toHaveLength(2);
		expect(overlay?.edges).toHaveLength(3);
		expect(overlay?.faces.find((face) => face.id === "f∞")?.role).toBe(
			"source-side",
		);
		const [first, second] = overlay?.faces ?? [];
		expect(
			Math.hypot(
				(first?.x ?? 0) - (second?.x ?? 0),
				(first?.y ?? 0) - (second?.y ?? 0),
			),
		).toBeGreaterThanOrEqual(42);
		expect(
			overlay?.edges.every((edge) => edge.activeDirection === undefined),
		).toBe(true);
	});
});
