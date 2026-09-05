import { describe, expect, it } from "vitest";

import {
	eibfsNodeLabel,
	eibfsRootGlyph,
	projectEibfsView,
} from "./flow-eibfs-view";
import { decodeFlowCurrentSceneV9 } from "./flow-scene";

type MutableEibfsFixture = ReturnType<typeof eibfsFixture>;

function required<T>(value: T | undefined, label: string): T {
	if (value === undefined) throw new Error(`missing fixture ${label}`);
	return value;
}

function overlayNode(value: MutableEibfsFixture, nodeId: string) {
	return required(
		value.eibfs_overlay.nodes.find((node) => node.node_id === nodeId),
		`overlay node ${nodeId}`,
	);
}

function traceNode(value: MutableEibfsFixture, nodeId: string) {
	return required(
		value.node_trace_states.find((node) => node.node_id === nodeId),
		`trace node ${nodeId}`,
	);
}

function eibfsFixture() {
	const edges = [
		{ id: "sa", from: "s", to: "a", lower: "0", capacity: "2", cost: "0" },
		{ id: "ax", from: "a", to: "x", lower: "0", capacity: "2", cost: "0" },
		{ id: "xb", from: "x", to: "b", lower: "0", capacity: "2", cost: "0" },
		{ id: "bt", from: "b", to: "t", lower: "0", capacity: "2", cost: "0" },
	];
	return {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "8",
		event_count: "12",
		solve_status: "running",
		model: { kind: "max-flow", source: "s", sink: "t" },
		graph: {
			nodes: ["s", "a", "x", "b", "t"].map((id) => ({
				id,
				supply: "0",
			})),
			edges,
		},
		algorithm: { id: "eibfs", config: {} },
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
		edge_states: edges.map((edge) => ({ edge_id: edge.id, flow: "0" })),
		residual_arcs: edges.flatMap((edge) => [
			{
				edge_id: edge.id,
				direction: "forward",
				from: edge.from,
				to: edge.to,
				capacity: edge.capacity,
				cost: edge.cost,
				active: false,
			},
			{
				edge_id: edge.id,
				direction: "reverse",
				from: edge.to,
				to: edge.from,
				capacity: "0",
				cost: edge.cost,
				active: false,
			},
		]),
		node_trace_states: [
			{
				node_id: "a",
				label: "1",
				search_ordinal: 0,
				remaining_divergence: "0",
			},
			{ node_id: "b", label: "-2", remaining_divergence: "0" },
			{ node_id: "s", label: "0", remaining_divergence: "-2" },
			{ node_id: "t", label: "-1", remaining_divergence: "0" },
			{ node_id: "x", label: "3", remaining_divergence: "2" },
		],
		eibfs_overlay: {
			phase_direction: "forward",
			source_depth: "1",
			sink_depth: "1",
			nodes: [
				{
					node_id: "a",
					source_label: "1",
					sink_label: "2",
					membership: "source",
					root_kind: "none",
					orphan: false,
					imbalance: "0",
				},
				{
					node_id: "b",
					source_label: "2",
					sink_label: "1",
					membership: "sink",
					root_kind: "none",
					orphan: false,
					imbalance: "0",
				},
				{
					node_id: "s",
					source_label: "0",
					sink_label: "0",
					membership: "source",
					root_kind: "source",
					orphan: false,
					imbalance: "-2",
				},
				{
					node_id: "t",
					source_label: "0",
					sink_label: "0",
					membership: "sink",
					root_kind: "sink",
					orphan: false,
					imbalance: "0",
				},
				{
					node_id: "x",
					source_label: "3",
					sink_label: "3",
					membership: "source",
					root_kind: "excess",
					orphan: false,
					imbalance: "2",
				},
			],
			forest_arcs: [
				{
					parent: "s",
					child: "a",
					side: "source",
					admissible_residual: { edge_id: "sa", direction: "forward" },
				},
				{
					parent: "t",
					child: "b",
					side: "sink",
					admissible_residual: { edge_id: "bt", direction: "forward" },
				},
			],
		},
		trace_event: {
			event_id: "8",
			parent_phase_id: "7",
			catalog_id: "eibfs.grow-forward",
			minimum_granularity: "operation",
			pseudocode_line: "eibfs:grow-forward",
			patch_count: 3,
			entity_refs: [{ kind: "node", node_id: "a" }],
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
			changed_entity_refs: [{ kind: "node", node_id: "a" }],
		},
		metrics: Array.from({ length: 16 }, () => "0"),
	};
}

function decode(value: MutableEibfsFixture) {
	value.trace_event_semantics.role = [
		"primitive-complete",
		"optimal",
		"infeasible",
	].includes(value.solve_status)
		? "certify"
		: "mutate";
	return decodeFlowCurrentSceneV9(
		new TextEncoder().encode(JSON.stringify(value)),
	);
}

function dynamicOverlay(
	stage: "apply-update" | "prefix-certified",
): Record<string, unknown> {
	return {
		stage,
		update_index: "1",
		update_total: "2",
		changed_edge: "sa",
		old_capacity: "2",
		new_capacity: "0",
		reused_forest_nodes: "4",
		updates_applied: "1",
		capacity_increases: "0",
		capacity_decreases: "1",
		no_op_updates: "0",
		over_capacity_repairs: "0",
		invalidated_parent_arcs: "0",
		promoted_roots: "0",
		repair_arc_scans: "3",
		state_transitions: "9",
		bridge_violations: "0",
		label_violations: "0",
		current_arc_violations: "0",
		boundary_violations: "0",
		repair_iterations: "0",
		certification_recoveries: "0",
		...(stage === "prefix-certified" ? { prefix_value: "0" } : {}),
	};
}

describe("EIBFS scene and view", () => {
	it("keeps the certified initial prefix reusable before update one", () => {
		const value = eibfsFixture();
		value.algorithm.id = "dynamic-eibfs";
		const overlay = dynamicOverlay("apply-update");
		overlay.stage = "resume-reusable-pseudoflow";
		overlay.update_index = "0";
		overlay.updates_applied = "0";
		delete overlay.changed_edge;
		delete overlay.old_capacity;
		delete overlay.new_capacity;
		(value as unknown as Record<string, unknown>).dynamic_eibfs_overlay =
			overlay;
		expect(decode(value).dynamic_eibfs_overlay?.stage).toBe(
			"resume-reusable-pseudoflow",
		);
	});

	it("admits only the typed Dynamic EIBFS capacity-update boundary", () => {
		const value = eibfsFixture();
		value.algorithm.id = "dynamic-eibfs";
		(value as unknown as Record<string, unknown>).dynamic_eibfs_overlay =
			dynamicOverlay("apply-update");
		const edge = required(
			value.graph.edges.find((candidate) => candidate.id === "sa"),
			"changed edge",
		);
		edge.capacity = "0";
		const state = required(
			value.edge_states.find((candidate) => candidate.edge_id === "sa"),
			"changed edge state",
		);
		state.flow = "2";
		const forward = required(
			value.residual_arcs.find(
				(candidate) =>
					candidate.edge_id === "sa" && candidate.direction === "forward",
			),
			"changed forward residual",
		);
		const reverse = required(
			value.residual_arcs.find(
				(candidate) =>
					candidate.edge_id === "sa" && candidate.direction === "reverse",
			),
			"changed reverse residual",
		);
		forward.capacity = "0";
		reverse.capacity = "2";

		const scene = decode(value);
		expect(scene.dynamic_eibfs_overlay?.stage).toBe("apply-update");
		expect(projectEibfsView(scene)?.sourceForestArcKeys).toContain(
			"sa:forward",
		);
		(
			(value as unknown as Record<string, unknown>)
				.dynamic_eibfs_overlay as Record<string, unknown>
		).stage = "repair-forest";
		expect(() => decode(value)).toThrow("edge state does not match");

		(
			(value as unknown as Record<string, unknown>)
				.dynamic_eibfs_overlay as Record<string, unknown>
		).stage = "apply-update";
		(
			(value as unknown as Record<string, unknown>)
				.dynamic_eibfs_overlay as Record<string, unknown>
		).changed_edge = "ax";
		expect(() => decode(value)).toThrow("current edge");
	});

	it("projects separate forests, retained labels, frontier, and finite roots", () => {
		const view = projectEibfsView(decode(eibfsFixture()));
		expect(view).toBeDefined();
		expect(view?.phaseDirection).toBe("forward");
		expect(view?.sourceForestArcKeys).toEqual(new Set(["sa:forward"]));
		expect(view?.sinkForestArcKeys).toEqual(new Set(["bt:forward"]));
		expect(view?.nodes.get("a")).toMatchObject({
			membership: "source",
			frontier: true,
		});
		expect(view?.forestArcs.find((arc) => arc.side === "sink")).toMatchObject({
			parent: "t",
			child: "b",
			displayDirection: "reverse",
		});
		const excessRoot = view?.nodes.get("x");
		expect(excessRoot).toBeDefined();
		if (excessRoot === undefined) throw new Error("missing excess root");
		expect(eibfsNodeLabel(excessRoot)).toBe("S · dₛ 3 · e +2");
		expect(eibfsRootGlyph(excessRoot)).toBe("+");
	});

	it("rejects a forest relation that crosses memberships", () => {
		const value = eibfsFixture();
		overlayNode(value, "a").membership = "sink";
		expect(() => decode(value)).toThrow("EIBFS overlay is not a rooted forest");
	});

	it("rejects two parents for one child", () => {
		const value = eibfsFixture();
		const relation = required(
			value.eibfs_overlay.forest_arcs[0],
			"source relation",
		);
		value.eibfs_overlay.forest_arcs.push({
			...relation,
			admissible_residual: { ...relation.admissible_residual },
		});
		expect(() => decode(value)).toThrow("EIBFS overlay is not a rooted forest");
	});

	it("rejects a T-forest residual directed parent to child", () => {
		const value = eibfsFixture();
		required(
			value.eibfs_overlay.forest_arcs[1],
			"sink relation",
		).admissible_residual.direction = "reverse";
		expect(() => decode(value)).toThrow("EIBFS forest arc is not admissible");
	});

	it("rejects a finite excess root with the wrong imbalance sign", () => {
		const value = eibfsFixture();
		overlayNode(value, "x").imbalance = "-1";
		expect(() => decode(value)).toThrow("EIBFS node root state is invalid");
	});

	it("rejects a parent relation whose residual capacity is zero", () => {
		const value = eibfsFixture();
		required(value.edge_states[0], "sa flow").flow = "2";
		required(value.residual_arcs[0], "sa forward residual").capacity = "0";
		required(value.residual_arcs[1], "sa reverse residual").capacity = "2";
		expect(() => decode(value)).toThrow(
			"EIBFS parent residual is not positive",
		);
	});

	it("rejects generic and EIBFS forest overlays in one frame", () => {
		const value = eibfsFixture() as MutableEibfsFixture & {
			pseudoflow_forest?: { arcs: never[]; strong_nodes: never[] };
		};
		value.pseudoflow_forest = { arcs: [], strong_nodes: [] };
		expect(() => decode(value)).toThrow("conflicting forest overlays");
	});

	it("rejects overlay drift from generic labels and divergence", () => {
		const labelDrift = eibfsFixture();
		traceNode(labelDrift, "a").label = "2";
		expect(() => decode(labelDrift)).toThrow(
			"EIBFS node root state is invalid",
		);

		const divergenceDrift = eibfsFixture();
		traceNode(divergenceDrift, "x").remaining_divergence = "1";
		expect(() => decode(divergenceDrift)).toThrow(
			"EIBFS node root state is invalid",
		);

		const missingNonzeroDivergence = eibfsFixture();
		delete (
			traceNode(missingNonzeroDivergence, "x") as {
				remaining_divergence?: string;
			}
		).remaining_divergence;
		expect(() => decode(missingNonzeroDivergence)).toThrow(
			"EIBFS node root state is invalid",
		);
	});

	it("decodes an omitted all-zero divergence vector as canonical zero", () => {
		const value = eibfsFixture();
		overlayNode(value, "s").imbalance = "0";
		const x = overlayNode(value, "x");
		x.membership = "free";
		x.root_kind = "none";
		x.imbalance = "0";
		delete (traceNode(value, "x") as { label?: string }).label;
		for (const node of value.node_trace_states) {
			delete (node as { remaining_divergence?: string }).remaining_divergence;
		}

		expect(decode(value).eibfs_overlay?.nodes).toHaveLength(5);
	});

	it("requires canonical node order and excludes orphans from the frontier", () => {
		const reversed = eibfsFixture();
		reversed.eibfs_overlay.nodes.reverse();
		expect(() => decode(reversed)).toThrow(
			"EIBFS node states do not match the graph",
		);

		const orphan = eibfsFixture();
		const a = overlayNode(orphan, "a");
		a.orphan = true;
		orphan.eibfs_overlay.forest_arcs.shift();
		expect(projectEibfsView(decode(orphan))?.nodes.get("a")?.frontier).toBe(
			false,
		);
	});

	it("rejects a pseudoflow overlay at recovery or certified boundaries", () => {
		const recovery = eibfsFixture();
		recovery.trace_event.catalog_id = "eibfs.begin-feasible-flow-recovery";
		expect(() => decode(recovery)).toThrow(
			"EIBFS pseudoflow overlay cannot accompany recovered flow",
		);

		const certified = eibfsFixture();
		certified.solve_status = "optimal";
		expect(() => decode(certified)).toThrow(
			"EIBFS pseudoflow overlay cannot accompany recovered flow",
		);
	});

	it("rejects a directed cycle even when every child has one parent", () => {
		const value = eibfsFixture();
		const a = overlayNode(value, "a");
		const x = overlayNode(value, "x");
		a.root_kind = "none";
		x.root_kind = "none";
		a.source_label = "4";
		x.source_label = "3";
		value.eibfs_overlay.forest_arcs = [];
		// A cycle cannot also satisfy strict +1 tree labels. The decoder must reject
		// it at or before the explicit cycle check rather than accepting a non-forest.
		value.eibfs_overlay.forest_arcs.push(
			{
				parent: "a",
				child: "x",
				side: "source",
				admissible_residual: { edge_id: "ax", direction: "forward" },
			},
			{
				parent: "x",
				child: "a",
				side: "source",
				admissible_residual: { edge_id: "ax", direction: "reverse" },
			},
		);
		expect(() => decode(value)).toThrow(/EIBFS (forest arc|overlay)/);
	});
});
