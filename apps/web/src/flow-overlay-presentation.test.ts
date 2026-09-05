import { describe, expect, it } from "vitest";
import { FLOW_OVERLAY_CONTRIBUTIONS } from "./flow-overlay-contribution-registry";
import {
	buildFlowOverlayPresentation,
	createFlowOverlayPresenter,
	FLOW_OVERLAY_PRESENTERS,
} from "./flow-overlay-presentation";
import type { FlowCurrentSceneV9 } from "./flow-scene";
import { FLOW_SCENE_V9_OVERLAY_DECODERS } from "./flow-scene-wire/generated/overlays";

function binaryBlockingScene(): FlowCurrentSceneV9 {
	return {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "0",
		event_count: "0",
		solve_status: "ready",
		model: { kind: "max-flow", source: "s", sink: "t" },
		graph: {
			nodes: [
				{ id: "s", supply: "0" },
				{ id: "t", supply: "0" },
			],
			edges: [],
		},
		algorithm: { id: "binary-blocking-flow", config: {} },
		run_profile: "trace",
		trace_granularity: "operation",
		trace_steps: {
			phase_unit: "one binary-length phase",
			phase_availability: { availability: "available" },
			operation_unit: "one blocking-flow operation",
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
		metrics: Array.from(
			{ length: 16 },
			() => "0",
		) as FlowCurrentSceneV9["metrics"],
		binary_blocking_overlay: {
			stage: "analyzed",
			upper_bound: "4",
			delta: "2",
			delivered: "0",
			nodes: [
				{ node_id: "s", distance: "1", component: "0" },
				{ node_id: "t", distance: "0", component: "1" },
			],
			base_zero_arcs: [],
			special_arcs: [],
			admissible_arcs: [],
			zero_admissible_arcs: [],
		},
	};
}

function convexScalingScene(
	stage: "initialize" | "start-scale" | "complete-scale" | "optimal",
): FlowCurrentSceneV9 {
	const scene = binaryBlockingScene();
	delete scene.binary_blocking_overlay;
	scene.graph.edges.push({
		id: "e",
		from: "s",
		to: "t",
		lower: "0",
		capacity: "4",
		cost: "0",
	});
	scene.convex_cost_overlay = {
		stage,
		scale: "2",
		edges: [],
		active_cycle: [],
		eligible_arcs: [{ edge_id: "e", segment: "0", direction: "forward" }],
	};
	return scene;
}

describe("flow overlay presentation", () => {
	it("stays total over the Rust-generated overlay registry", () => {
		expect(FLOW_OVERLAY_PRESENTERS.map(({ field }) => field)).toEqual(
			FLOW_SCENE_V9_OVERLAY_DECODERS.map(([field]) => field),
		);
		for (const presenter of FLOW_OVERLAY_PRESENTERS) {
			const contribution = presenter.present({ stage: "active" } as never);
			expect(contribution.legend.overlay).toBe(presenter.field);
			expect(contribution.inspector.overlay).toBe(presenter.field);
			expect(contribution.status.overlay).toBe(presenter.field);
			expect(contribution.accessibleDescription).toContain(
				presenter.definition.title,
			);
		}
	});

	it("projects active overlays into marks, inspector rows, and SVG lookups", () => {
		const presentation = buildFlowOverlayPresentation(binaryBlockingScene());

		expect(presentation.activeFields).toEqual(["binary_blocking_overlay"]);
		expect(presentation.marks).toEqual([
			expect.objectContaining({ kind: "node", entityId: "s" }),
			expect.objectContaining({ kind: "node", entityId: "t" }),
		]);
		expect(presentation.nodeMarksById.get("s")).toEqual([
			expect.objectContaining({
				overlay: "binary_blocking_overlay",
				kind: "node",
			}),
		]);
		expect(presentation.edgeMarksById.size).toBe(0);
		expect(presentation.inspectorSections).toEqual([
			expect.objectContaining({
				overlay: "binary_blocking_overlay",
				rows: expect.arrayContaining([
					{ field: "stage", label: "Stage (stage)", value: "analyzed" },
					{
						field: "referenced_nodes",
						label: "Referenced nodes",
						value: "2",
					},
				]),
			}),
		]);
		expect(presentation.renderData.binaryNodeById.get("s")?.distance).toBe("1");
		expect(presentation.renderData.overlayViews.binaryBlocking?.delta).toBe(
			"2",
		);
	});

	it("opens the exact convex delta-eligible network only during a live scale", () => {
		const initialize = buildFlowOverlayPresentation(
			convexScalingScene("initialize"),
		);
		const start = buildFlowOverlayPresentation(
			convexScalingScene("start-scale"),
		);
		const complete = buildFlowOverlayPresentation(
			convexScalingScene("complete-scale"),
		);
		const optimal = buildFlowOverlayPresentation(convexScalingScene("optimal"));

		expect(initialize.renderData.convexEligibleDirectionsByEdge.size).toBe(0);
		expect(start.renderData.convexEligibleDirectionsByEdge.get("e")).toEqual(
			new Set(["forward"]),
		);
		expect(complete.renderData.convexEligibleDirectionsByEdge.size).toBe(0);
		expect(optimal.renderData.convexEligibleDirectionsByEdge.size).toBe(0);
	});

	it("projects Dynamic EIBFS's scalar changed-edge identity onto the graph", () => {
		const presenter = createFlowOverlayPresenter(
			FLOW_OVERLAY_CONTRIBUTIONS.dynamic_eibfs_overlay,
		);
		const contribution = presenter.present({
			stage: "apply-update",
			update_index: "1",
			update_total: "1",
			changed_edge: "sa",
			reused_forest_nodes: "2",
			updates_applied: "1",
			capacity_increases: "1",
			capacity_decreases: "0",
			no_op_updates: "0",
			over_capacity_repairs: "0",
			invalidated_parent_arcs: "0",
			promoted_roots: "0",
			repair_arc_scans: "3",
			state_transitions: "1",
			bridge_violations: "0",
			label_violations: "0",
			current_arc_violations: "0",
			boundary_violations: "0",
		} as never);

		expect(contribution.marks).toContainEqual({
			overlay: "dynamic_eibfs_overlay",
			kind: "edge",
			entityId: "sa",
			role: "changed_edge",
		});
	});

	it("composes generic node, residual arc, status, legend, inspector, and a11y output", () => {
		const scene = binaryBlockingScene();
		scene.graph.edges.push({
			id: "e",
			from: "s",
			to: "t",
			lower: "0",
			capacity: "4",
			cost: "0",
		});
		scene.binary_blocking_overlay?.admissible_arcs.push({
			edge_id: "e",
			direction: "forward",
		});
		const generic = createFlowOverlayPresenter({
			...FLOW_OVERLAY_CONTRIBUTIONS.binary_blocking_overlay,
			presentation: { kind: "generic", accent: "amber" },
		});

		const presentation = buildFlowOverlayPresentation(scene, [generic]);

		expect(presentation.genericStatusEntries).toEqual([
			expect.objectContaining({
				overlay: "binary_blocking_overlay",
				items: expect.arrayContaining([
					{ label: "Stage (stage)", value: "analyzed" },
				]),
			}),
		]);
		expect(presentation.genericNodeDecorations).toEqual(
			expect.arrayContaining([
				expect.objectContaining({ entityId: "s", accent: "amber" }),
			]),
		);
		expect(presentation.genericEdgeDecorations).toEqual([]);
		expect(presentation.genericResidualArcDecorations).toEqual([
			expect.objectContaining({
				entityId: "e",
				direction: "forward",
				accent: "amber",
			}),
		]);
		expect(presentation.legendEntries[0]?.description).toContain("0/1 lengths");
		expect(presentation.inspectorSections[0]?.rows).toEqual(
			expect.arrayContaining([
				{
					field: "referenced_edges",
					label: "Referenced edges",
					value: "1",
				},
			]),
		);
		expect(presentation.accessibleDescriptions[0]).toContain(
			"Referenced nodes 2, edges 1",
		);
	});
});
