import { describe, expect, it } from "vitest";
import {
	ibfsDistanceLabel,
	isIbfsAlgorithm,
	projectIbfsView,
} from "./flow-ibfs-view";
import { decodeFlowCurrentSceneV9 } from "./flow-scene";

function ibfsScene() {
	const value = {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "8",
		event_count: "12",
		solve_status: "running",
		model: { kind: "max-flow", source: "s", sink: "t" },
		graph: {
			nodes: ["s", "a", "b", "t"].map((id) => ({ id, supply: "0" })),
			edges: [
				{ id: "sa", from: "s", to: "a", lower: "0", capacity: "2", cost: "0" },
				{ id: "ab", from: "a", to: "b", lower: "0", capacity: "1", cost: "0" },
				{ id: "bt", from: "b", to: "t", lower: "0", capacity: "2", cost: "0" },
			],
		},
		algorithm: { id: "ibfs", config: {} },
		run_profile: "trace",
		trace_granularity: "operation",
		trace_steps: {
			phase_unit: "one search-tree growth, augmentation, or adoption phase",
			phase_availability: { availability: "available" },
			operation_unit:
				"one tree growth, path augmentation, orphan adoption, or distance repair",
			operation_availability: { availability: "available" },
			detail: {
				availability: "unavailable",
				reason:
					"This vision-oriented trace aggregates primitive tree-edge scans.",
			},
			primary_work: {
				metric_ordinal: 2,
				unit: "residual-arc inspections",
				abstraction: "primitive",
				visualization: "edge-field",
			},
		},
		edge_states: [
			{ edge_id: "sa", flow: "1" },
			{ edge_id: "ab", flow: "1" },
			{ edge_id: "bt", flow: "1" },
		],
		residual_arcs: [
			{
				edge_id: "sa",
				direction: "forward",
				from: "s",
				to: "a",
				capacity: "1",
				cost: "0",
				active: true,
			},
			{
				edge_id: "sa",
				direction: "reverse",
				from: "a",
				to: "s",
				capacity: "1",
				cost: "0",
				active: false,
			},
			{
				edge_id: "ab",
				direction: "forward",
				from: "a",
				to: "b",
				capacity: "0",
				cost: "0",
				active: true,
			},
			{
				edge_id: "ab",
				direction: "reverse",
				from: "b",
				to: "a",
				capacity: "1",
				cost: "0",
				active: false,
			},
			{
				edge_id: "bt",
				direction: "forward",
				from: "b",
				to: "t",
				capacity: "1",
				cost: "0",
				active: true,
			},
			{
				edge_id: "bt",
				direction: "reverse",
				from: "t",
				to: "b",
				capacity: "1",
				cost: "0",
				active: false,
			},
		],
		node_trace_states: [
			{ node_id: "a", label: "1", search_ordinal: 0 },
			{ node_id: "b", label: "-2", search_ordinal: 1 },
			{ node_id: "s", label: "0" },
			{ node_id: "t", label: "-1" },
		],
		pseudoflow_forest: {
			arcs: [
				{ edge_id: "sa", direction: "forward" },
				{ edge_id: "bt", direction: "reverse" },
			],
			strong_nodes: [],
		},
		trace_event: {
			event_id: "8",
			parent_phase_id: "6",
			catalog_id: "ibfs.augment-shortest-path",
			minimum_granularity: "operation",
			pseudocode_line: "ibfs:augment-and-create-orphans",
			patch_count: 4,
			entity_refs: [],
			detail: { label: "bottleneck", value: "1" },
		},
		trace_event_semantics: {
			role: "mutate",
			work_deltas: [{ unit: "published-transition", count: "1" }],
			aggregation_count: "1",
			work_progress: {
				detail_completed: "0",
				detail_total: "0",
				primary_completed: "0",
				primary_total: "0",
			},
			changed_entity_refs: [],
		},
		metrics: Array.from({ length: 16 }, () => "0"),
	};
	return decodeFlowCurrentSceneV9(
		new TextEncoder().encode(JSON.stringify(value)),
	);
}

describe("projectIbfsView", () => {
	it("shares the typed two-tree projection with Boykov–Kolmogorov", () => {
		const scene = ibfsScene();
		scene.algorithm.id = "boykov-kolmogorov";
		if (scene.trace_event === undefined) throw new Error("missing trace event");
		scene.trace_event.catalog_id = "boykov-kolmogorov.augment";
		const order = ["s", "t", "a", "b"];
		for (const state of scene.node_trace_states) {
			state.search_ordinal = order.indexOf(state.node_id);
		}
		const view = projectIbfsView(scene);
		expect(isIbfsAlgorithm("boykov-kolmogorov")).toBe(true);
		expect(view?.nodes.get("a")?.orphan).toBe(true);
		expect(view?.nodes.get("b")?.orphan).toBe(true);
		expect(view?.sourceForestArcKeys).toContain("sa:forward");
		expect(view?.sinkForestArcKeys).toContain("bt:reverse");
	});

	it("decodes signed S/T distances, tree directions, and orphan queue", () => {
		const view = projectIbfsView(ibfsScene());
		expect(view).toBeDefined();
		expect(view?.sourceForestArcKeys).toEqual(new Set(["sa:forward"]));
		expect(view?.sinkForestArcKeys).toEqual(new Set(["bt:reverse"]));
		expect(view?.nodes.get("a")).toMatchObject({
			side: "source",
			distance: 1,
			orphan: true,
		});
		expect(view?.nodes.get("b")).toMatchObject({
			side: "sink",
			distance: 1,
			orphan: true,
		});
		const sinkTreeNode = view?.nodes.get("b");
		expect(sinkTreeNode).toBeDefined();
		if (sinkTreeNode === undefined) throw new Error("missing sink-tree node");
		expect(ibfsDistanceLabel(sinkTreeNode)).toBe("T · dₜ 1");
	});

	it("rejects a forest edge that crosses signed tree partitions", () => {
		const scene = ibfsScene();
		scene.pseudoflow_forest = {
			arcs: [{ edge_id: "ab", direction: "forward" }],
			strong_nodes: [],
		};
		expect(() => projectIbfsView(scene)).toThrow(
			"IBFS forest arc violates the signed-distance tree",
		);
	});

	it.each([
		["ibfs.remove-source-orphan", "a"],
		["ibfs.remove-sink-orphan", "b"],
	])("keeps %s focus visible after the node leaves its tree", (eventId, nodeId) => {
		const scene = ibfsScene();
		for (const state of scene.node_trace_states) {
			delete state.search_ordinal;
			if (state.node_id === nodeId) {
				state.search_ordinal = 0;
				delete state.label;
			}
		}
		if (scene.trace_event === undefined) throw new Error("missing trace event");
		scene.trace_event.catalog_id = eventId;
		if (scene.pseudoflow_forest === undefined)
			throw new Error("missing forest");
		scene.pseudoflow_forest.arcs = scene.pseudoflow_forest.arcs.filter(
			(arc) => arc.edge_id !== (nodeId === "a" ? "sa" : "bt"),
		);

		const view = projectIbfsView(scene);
		expect(view?.nodes.has(nodeId)).toBe(false);
		expect(view?.repairFocusNodeIds).toContain(nodeId);
	});

	it("rejects a source-tree parent edge with zero residual capacity", () => {
		const scene = ibfsScene();
		const parentArc = scene.residual_arcs.find(
			(arc) => arc.edge_id === "sa" && arc.direction === "forward",
		);
		if (parentArc === undefined) throw new Error("missing source parent arc");
		parentArc.capacity = "0";
		expect(() => projectIbfsView(scene)).toThrow(
			"IBFS forest parent arc is not residual",
		);
	});

	it("rejects two forest parents assigned to the same child", () => {
		const scene = ibfsScene();
		scene.graph.edges.push({
			id: "sa2",
			from: "s",
			to: "a",
			lower: "0",
			capacity: "1",
			cost: "0",
		});
		scene.edge_states.push({ edge_id: "sa2", flow: "0" });
		scene.residual_arcs.push(
			{
				edge_id: "sa2",
				direction: "forward",
				from: "s",
				to: "a",
				capacity: "1",
				cost: "0",
				active: false,
				fixed: false,
			},
			{
				edge_id: "sa2",
				direction: "reverse",
				from: "a",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
				fixed: false,
			},
		);
		scene.pseudoflow_forest?.arcs.push({
			edge_id: "sa2",
			direction: "forward",
		});
		expect(() => projectIbfsView(scene)).toThrow(
			"IBFS forest assigns more than one parent to a child",
		);
	});
});
