import { describe, expect, it, vi } from "vitest";

import {
	automaticFlowLod,
	buildFlowRenderPlan,
	FLOW_LOD_LIMITS,
	flowLodForScene,
	isGridgraphScene,
	isWashingtonRandomLevelScene,
} from "./flow-render-plan";
import type {
	FlowCurrentSceneV9,
	FlowEdgeV1,
	FlowNodeV1,
	FlowResidualArcStateV1,
} from "./flow-scene";

function scene(
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	residualArcs: FlowResidualArcStateV1[] = [],
): FlowCurrentSceneV9 {
	return {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "0",
		event_count: "0",
		solve_status: "ready",
		model: {
			kind: "max-flow",
			source: nodes[0]?.id ?? "s",
			sink: nodes.at(-1)?.id ?? "t",
		},
		graph: { nodes, edges },
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
		edge_states: edges.map((edge) => ({ edge_id: edge.id, flow: "0" })),
		residual_arcs: residualArcs,
		node_trace_states: [],
		metrics: Array.from(
			{ length: 16 },
			() => "0",
		) as FlowCurrentSceneV9["metrics"],
	};
}

function nodes(count: number): FlowNodeV1[] {
	return Array.from({ length: count }, (_, index) => ({
		id: `n${index.toString().padStart(5, "0")}`,
		supply: "0",
		position: {
			x: (70 + ((index * 83) % 760)).toString(),
			y: (70 + ((index * 47) % 400)).toString(),
		},
	}));
}

function edges(count: number, nodeCount: number): FlowEdgeV1[] {
	return Array.from({ length: count }, (_, index) => ({
		id: `e${index.toString().padStart(6, "0")}`,
		from: `n${(index % nodeCount).toString().padStart(5, "0")}`,
		to: `n${((index * 17 + 1) % nodeCount).toString().padStart(5, "0")}`,
		lower: "0",
		capacity: "1",
		cost: (index % 3 === 0 ? -1 : index % 3 === 1 ? 0 : 1).toString(),
	}));
}

function goldbergMeshScene(): FlowCurrentSceneV9 {
	const meshNodes = Array.from({ length: 12 }, (_, index) => {
		const row = Math.floor(index / 4);
		const column = index % 4;
		return {
			id: `m${row.toString().padStart(4, "0")}c${column.toString().padStart(4, "0")}`,
			supply: "0",
			position: {
				x: (72 + column * 252).toString(),
				y: (58 + row * 212).toString(),
			},
		};
	});
	let edgeOrdinal = 0;
	const meshEdges = meshNodes.flatMap((node, index) => {
		const row = Math.floor(index / 4);
		const column = index % 4;
		const right = meshNodes[row * 4 + ((column + 1) % 4)];
		const down = meshNodes[((row + 1) % 3) * 4 + column];
		if (right === undefined || down === undefined) {
			throw new Error("mesh neighbor is missing");
		}
		return [right, down].flatMap((neighbor) => {
			const forwardId = `e${edgeOrdinal.toString().padStart(6, "0")}`;
			edgeOrdinal += 1;
			const reverseId = `e${edgeOrdinal.toString().padStart(6, "0")}`;
			edgeOrdinal += 1;
			return [
				{
					id: forwardId,
					from: node.id,
					to: neighbor.id,
					lower: "0",
					capacity: "10",
					cost: "3",
				},
				{
					id: reverseId,
					from: neighbor.id,
					to: node.id,
					lower: "0",
					capacity: "7",
					cost: "-3",
				},
			];
		});
	});
	const current = scene(meshNodes, meshEdges);
	current.model = { kind: "circulation" };
	return current;
}

describe("flow render plan", () => {
	it("selects the three LOD levels at their explicit boundaries", () => {
		expect(automaticFlowLod(50, 64)).toBe("detail");
		expect(automaticFlowLod(51, 64)).toBe("structure");
		expect(automaticFlowLod(50, 65)).toBe("structure");
		expect(automaticFlowLod(600, 1_200)).toBe("structure");
		expect(automaticFlowLod(601, 1_200)).toBe("overview");
		expect(automaticFlowLod(600, 1_201)).toBe("overview");
	});

	it("moves overfull parallel groups to Structure without dropping edges", () => {
		const graphNodes = nodes(2);
		const parallelEdges = Array.from({ length: 10 }, (_, index) => ({
			id: `parallel-${index.toString().padStart(2, "0")}`,
			from: graphNodes[0]?.id ?? "n00000",
			to: graphNodes[1]?.id ?? "n00001",
			lower: "0",
			capacity: "1",
			cost: "0",
		}));
		const current = scene(graphNodes, parallelEdges);

		expect(automaticFlowLod(2, 10)).toBe("detail");
		expect(flowLodForScene(current)).toBe("structure");
		const plan = buildFlowRenderPlan(current);
		expect(plan.kind).toBe("entities");
		if (plan.kind !== "entities") throw new Error("expected entity plan");
		expect(plan.edges).toHaveLength(10);
	});

	it("honors safe viewport overrides without bypassing dense detail limits", () => {
		const graphNodes = nodes(90);
		const current = scene(graphNodes, edges(90, graphNodes.length));
		expect(buildFlowRenderPlan(current).level).toBe("structure");
		expect(buildFlowRenderPlan(current, "detail").level).toBe("structure");
		expect(buildFlowRenderPlan(current, "overview").level).toBe("overview");

		const smallNodes = nodes(20);
		const small = scene(smallNodes, edges(20, smallNodes.length));
		expect(buildFlowRenderPlan(small, "detail").level).toBe("detail");
	});

	it("keeps dense signed mesh pairs while distributing a bounded label sample", () => {
		const current = goldbergMeshScene();
		expect(automaticFlowLod(12, 48)).toBe("detail");
		expect(flowLodForScene(current)).toBe("structure");

		const plan = buildFlowRenderPlan(current);
		expect(plan.kind).toBe("entities");
		if (plan.kind !== "entities") throw new Error("expected entity plan");
		expect(plan.edges).toHaveLength(48);
		expect(plan.edgeLabelIds.size).toBe(12);
		expect(plan.edgeLabelIds.has("e000000")).toBe(true);
		expect(plan.edgeLabelIds.has("e000044")).toBe(true);
		expect(plan.edgeLabelIds.has("e000001")).toBe(false);
	});

	it("keeps dense assignment rails but labels the current assignment first", () => {
		const agentNodes = Array.from({ length: 6 }, (_, index) => ({
			id: `a${index}`,
			supply: "0",
		}));
		const taskNodes = Array.from({ length: 8 }, (_, index) => ({
			id: `t${index}`,
			supply: "0",
		}));
		const assignmentEdges = agentNodes.flatMap((agent, agentIndex) =>
			taskNodes.slice(0, 5).map((task, taskIndex) => ({
				id: `e${agentIndex}${taskIndex}`,
				from: agent.id,
				to: task.id,
				lower: "0",
				capacity: "1",
				cost: (agentIndex + taskIndex).toString(),
			})),
		);
		const current = scene([...agentNodes, ...taskNodes], assignmentEdges);
		current.model = {
			kind: "assignment",
			agents: agentNodes.map((node) => node.id),
			tasks: taskNodes.map((node) => node.id),
			objective: "minimize",
		};
		current.algorithm = { id: "hungarian", config: {} };
		current.edge_states = assignmentEdges.map((edge, index) => ({
			edge_id: edge.id,
			flow: index % 5 === 0 ? "1" : "0",
		}));

		const plan = buildFlowRenderPlan(current);
		expect(plan.kind).toBe("entities");
		if (plan.kind !== "entities") throw new Error("expected entity plan");
		expect(plan.level).toBe("detail");
		expect(plan.edges).toHaveLength(30);
		expect(plan.edgeLabelIds.size).toBe(6);
		for (const edge of current.edge_states.filter(
			(state) => state.flow === "1",
		)) {
			expect(plan.edgeLabelIds.has(edge.edge_id)).toBe(true);
		}
	});

	it("keeps the admitted 40 by 40 transportation table as labeled structure", () => {
		const origins = Array.from({ length: 40 }, (_, index) => ({
			id: `o${index.toString().padStart(2, "0")}`,
			supply: "40",
		}));
		const destinations = Array.from({ length: 40 }, (_, index) => ({
			id: `d${index.toString().padStart(2, "0")}`,
			supply: "-40",
		}));
		const routes = origins.flatMap((origin, row) =>
			destinations.map((destination, column) => ({
				id: `r${row.toString().padStart(2, "0")}${column
					.toString()
					.padStart(2, "0")}`,
				from: origin.id,
				to: destination.id,
				lower: "0",
				capacity: "40",
				cost: (row + column).toString(),
			})),
		);
		const current = scene([...origins, ...destinations], routes);
		current.model = {
			kind: "transportation",
			origins: origins.map((node) => node.id),
			destinations: destinations.map((node) => node.id),
		};
		current.algorithm = { id: "transportation-simplex", config: {} };

		expect(automaticFlowLod(80, 1_600)).toBe("overview");
		expect(flowLodForScene(current)).toBe("structure");
		const plan = buildFlowRenderPlan(current);
		expect(plan.kind).toBe("entities");
		if (plan.kind !== "entities") throw new Error("expected entity plan");
		expect(plan.nodes).toHaveLength(80);
		expect(plan.edges).toHaveLength(1_600);
		expect(plan.nodeLabelIds.size).toBe(80);
		expect(plan.edgeLabelIds.size).toBe(FLOW_LOD_LIMITS.structureEdgeLabels);
	});

	it("keeps the admitted IBFS grid band as inspectable tree structure", () => {
		const ibfsNodes = nodes(225);
		const ibfsEdges = edges(1_500, ibfsNodes.length);
		const current = scene(ibfsNodes, ibfsEdges);
		current.algorithm = { id: "ibfs", config: {} };
		current.node_trace_states = ibfsNodes.map((node, index) => ({
			node_id: node.id,
			...(index < 200
				? { label: index.toString() }
				: index === ibfsNodes.length - 1
					? { label: "-1" }
					: {}),
			...(index === 223 ? { search_ordinal: 0 } : {}),
		}));
		current.trace_event = {
			event_id: "1",
			catalog_id: "ibfs.remove-source-orphan",
			minimum_granularity: "operation",
			pseudocode_line: "ibfs:remove-orphan-beyond-current-boundary",
			patch_count: 1,
			entity_refs: [{ kind: "node", node_id: "n00222" }],
		};
		expect(automaticFlowLod(225, 1_500)).toBe("overview");
		expect(flowLodForScene(current)).toBe("structure");
		const plan = buildFlowRenderPlan(current);
		expect(plan.kind).toBe("entities");
		if (plan.kind !== "entities") throw new Error("expected entity plan");
		expect(plan.nodes).toHaveLength(225);
		expect(plan.edges).toHaveLength(1_500);
		expect(plan.nodeLabelIds).toContain("n00223");
		expect(plan.nodeLabelIds).toContain("n00222");
		expect(plan.nodeLabelIds).toContain("n00000");
		expect(plan.nodeLabelIds).toContain("n00224");
	});

	it("keeps admitted EIBFS forests inspectable and prioritizes roots, orphans, and parent arcs", () => {
		const eibfsNodes = nodes(225);
		const eibfsEdges = edges(1_500, eibfsNodes.length);
		const current = scene(eibfsNodes, eibfsEdges);
		current.algorithm = { id: "eibfs", config: {} };
		current.node_trace_states = eibfsNodes.map((node, index) => ({
			node_id: node.id,
			...(index === 0 || index === 149 || index === 222
				? { label: index === 222 ? "7" : "0" }
				: index === 59
					? { label: "1" }
					: index === 223
						? { label: "-9", search_ordinal: 0 }
						: index === 224
							? { label: "-1" }
							: {}),
			remaining_divergence: index === 149 || index === 222 ? "1" : "0",
		}));
		current.eibfs_overlay = {
			phase_direction: "reverse",
			source_depth: "7",
			sink_depth: "8",
			nodes: eibfsNodes.map((node, index) => ({
				node_id: node.id,
				source_label: index === 222 ? "7" : index === 59 ? "1" : "0",
				sink_label: index === 223 ? "8" : "0",
				membership:
					index === 0 || index === 59 || index === 149 || index === 222
						? ("source" as const)
						: index === 223 || index === 224
							? ("sink" as const)
							: ("free" as const),
				root_kind:
					index === 0
						? ("source" as const)
						: index === 224
							? ("sink" as const)
							: index === 149 || index === 222
								? ("excess" as const)
								: ("none" as const),
				orphan: index === 223,
				imbalance: index === 149 || index === 222 ? "1" : "0",
			})),
			forest_arcs: [
				{
					parent: "n00149",
					child: "n00059",
					side: "source",
					admissible_residual: {
						edge_id: "e001499",
						direction: "forward",
					},
				},
			],
		};

		expect(automaticFlowLod(225, 1_500)).toBe("overview");
		expect(flowLodForScene(current)).toBe("structure");
		const plan = buildFlowRenderPlan(current);
		expect(plan.kind).toBe("entities");
		if (plan.kind !== "entities") throw new Error("expected entity plan");
		expect(plan.nodeLabelIds).toContain("n00222");
		expect(plan.nodeLabelIds).toContain("n00223");
		expect(plan.nodeLabelIds).toContain("n00000");
		expect(plan.nodeLabelIds).toContain("n00224");
		expect(plan.edgeLabelIds).toContain("e001499");
	});

	it("bounds transportation detail labels while preserving current simplex state", () => {
		const origins = Array.from({ length: 6 }, (_, index) => ({
			id: `o${index}`,
			supply: "7",
		}));
		const destinations = Array.from({ length: 7 }, (_, index) => ({
			id: `d${index}`,
			supply: "-6",
		}));
		const routes = origins.flatMap((origin, row) =>
			destinations.map((destination, column) => ({
				id: `r${row}${column}`,
				from: origin.id,
				to: destination.id,
				lower: "0",
				capacity: "42",
				cost: (row * 7 + column).toString(),
			})),
		);
		const current = scene([...origins, ...destinations], routes, [
			{
				edge_id: "r65",
				direction: "forward",
				from: "o5",
				to: "d5",
				capacity: "42",
				cost: "40",
				active: true,
				fixed: false,
			},
		]);
		current.model = {
			kind: "transportation",
			origins: origins.map((node) => node.id),
			destinations: destinations.map((node) => node.id),
		};
		current.algorithm = { id: "transportation-simplex", config: {} };
		current.edge_states = routes.map((route) => ({
			edge_id: route.id,
			flow: route.id === "r00" || route.id === "r11" ? "6" : "0",
		}));
		current.pseudoflow_forest = {
			arcs: routes
				.filter((route) => route.from === "o0" || route.to === "d0")
				.map((route) => ({
					edge_id: route.id,
					direction: "forward",
				})),
			strong_nodes: ["o0"],
		};
		current.trace_event = {
			event_id: "1",
			catalog_id: "transportation-simplex.form-fundamental-cycle",
			minimum_granularity: "operation",
			pseudocode_line: "form the fundamental cycle",
			patch_count: 0,
			entity_refs: [{ kind: "edge", edge_id: "r66" }],
		};

		const plan = buildFlowRenderPlan(current);
		expect(plan.kind).toBe("entities");
		if (plan.kind !== "entities") throw new Error("expected entity plan");
		expect(plan.level).toBe("detail");
		expect(plan.edges).toHaveLength(42);
		expect(plan.edgeLabelIds.size).toBeLessThanOrEqual(18);
		expect(plan.edgeLabelIds.has("r65")).toBe(true);
		expect(plan.edgeLabelIds.has("r66")).toBe(true);
		expect(plan.edgeLabelIds.has("r00")).toBe(true);
		for (const basis of current.pseudoflow_forest.arcs) {
			expect(plan.edgeLabelIds.has(basis.edge_id)).toBe(true);
		}
	});

	it("does not classify an unrelated circulation from one mesh-like node ID", () => {
		const unrelatedNodes = nodes(30);
		const firstNode = unrelatedNodes[0];
		if (firstNode === undefined) throw new Error("missing first node");
		unrelatedNodes[0] = { ...firstNode, id: "m0000c0000" };
		const unrelatedEdges = edges(60, unrelatedNodes.length).map((edge) => ({
			...edge,
			from: edge.from === "n00000" ? "m0000c0000" : edge.from,
			to: edge.to === "n00000" ? "m0000c0000" : edge.to,
		}));
		const current = scene(unrelatedNodes, unrelatedEdges);
		current.model = { kind: "circulation" };

		expect(automaticFlowLod(30, 60)).toBe("detail");
		expect(flowLodForScene(current)).toBe("detail");
	});

	it("keeps the maximum Goldberg mesh inside bounded Overview aggregation", () => {
		const meshNodes = Array.from({ length: 1_024 }, (_, index) => ({
			id:
				index === 0
					? "m0000c0000"
					: `m${Math.floor(index / 32)
							.toString()
							.padStart(4, "0")}c${(index % 32).toString().padStart(4, "0")}`,
			supply: "0",
			position: {
				x: (72 + ((index % 32) * 756) / 31).toString(),
				y: (58 + (Math.floor(index / 32) * 424) / 31).toString(),
			},
		}));
		const meshEdges = Array.from({ length: 32_768 }, (_, index) => ({
			id: `e${index.toString().padStart(6, "0")}`,
			from: meshNodes[index % meshNodes.length]?.id ?? "missing",
			to: meshNodes[(index + 1) % meshNodes.length]?.id ?? "missing",
			lower: "0",
			capacity: "1",
			cost: index % 2 === 0 ? "1" : "-1",
		}));
		const current = scene(meshNodes, meshEdges);
		current.model = { kind: "circulation" };

		expect(flowLodForScene(current)).toBe("overview");
		expect(buildFlowRenderPlan(current, "detail").level).toBe("overview");
		const plan = buildFlowRenderPlan(current);
		if (plan.kind !== "overview") throw new Error("expected overview plan");
		expect(
			plan.originalEdges.reduce((sum, edge) => sum + edge.edgeCount, 0),
		).toBe(32_768);
		expect(
			plan.originalEdges.length + plan.residualArcs.length,
		).toBeLessThanOrEqual(FLOW_LOD_LIMITS.overviewAggregateMarks);
	});

	it("keeps trace and active labels inside the bounded structure label budget", () => {
		const graphNodes = nodes(200);
		const graphEdges = edges(200, graphNodes.length);
		const current = scene(graphNodes, graphEdges, [
			{
				edge_id: graphEdges[199]?.id ?? "missing",
				direction: "forward",
				from: graphEdges[199]?.from ?? "missing",
				to: graphEdges[199]?.to ?? "missing",
				capacity: "1",
				cost: "0",
				active: true,
				fixed: false,
			},
		]);
		current.node_trace_states = [{ node_id: graphNodes[199]?.id ?? "missing" }];
		current.trace_event = {
			event_id: "1",
			catalog_id: "test",
			minimum_granularity: "operation",
			pseudocode_line: "test",
			patch_count: 0,
			entity_refs: [
				{ kind: "edge", edge_id: graphEdges[198]?.id ?? "missing" },
			],
		};
		const plan = buildFlowRenderPlan(current);

		expect(plan.kind).toBe("entities");
		if (plan.kind !== "entities") throw new Error("expected entity plan");
		expect(plan.level).toBe("structure");
		expect(plan.nodeLabelIds.size).toBeLessThanOrEqual(
			FLOW_LOD_LIMITS.structureNodeLabels +
				FLOW_LOD_LIMITS.structureNodeEventLabels,
		);
		expect(plan.edgeLabelIds.size).toBeLessThanOrEqual(
			FLOW_LOD_LIMITS.structureEdgeLabels +
				FLOW_LOD_LIMITS.structureEdgeEventLabels,
		);
		expect(plan.nodeLabelIds.has(graphNodes[199]?.id ?? "missing")).toBe(true);
		expect(plan.edgeLabelIds.has(graphEdges[199]?.id ?? "missing")).toBe(true);
		expect(plan.edgeLabelIds.has(graphEdges[198]?.id ?? "missing")).toBe(true);

		const next = structuredClone(current);
		next.residual_arcs = [
			{
				edge_id: graphEdges[197]?.id ?? "missing",
				direction: "forward",
				from: graphEdges[197]?.from ?? "missing",
				to: graphEdges[197]?.to ?? "missing",
				capacity: "1",
				cost: "0",
				active: true,
				fixed: false,
			},
		];
		if (next.trace_event === undefined) throw new Error("missing trace event");
		next.trace_event.entity_refs = [
			{ kind: "edge", edge_id: graphEdges[196]?.id ?? "missing" },
		];
		const nextPlan = buildFlowRenderPlan(next);
		if (nextPlan.kind !== "entities") throw new Error("expected entity plan");
		for (const edge of graphEdges.slice(
			0,
			FLOW_LOD_LIMITS.structureEdgeLabels,
		)) {
			expect(plan.edgeLabelIds.has(edge.id)).toBe(true);
			expect(nextPlan.edgeLabelIds.has(edge.id)).toBe(true);
		}
		expect(nextPlan.edgeLabelIds.has(graphEdges[197]?.id ?? "missing")).toBe(
			true,
		);
		expect(nextPlan.edgeLabelIds.has(graphEdges[196]?.id ?? "missing")).toBe(
			true,
		);
	});

	it("prioritizes fixed arcs in Structure and preserves their counts in Overview", () => {
		const structureNodes = nodes(200);
		const structureEdges = edges(200, structureNodes.length);
		const fixedEdge = structureEdges[197];
		if (fixedEdge === undefined) throw new Error("missing fixed edge");
		const structureScene = scene(structureNodes, structureEdges, [
			{
				edge_id: fixedEdge.id,
				direction: "forward",
				from: fixedEdge.from,
				to: fixedEdge.to,
				capacity: "0",
				cost: fixedEdge.cost,
				active: false,
				fixed: true,
			},
		]);
		const structurePlan = buildFlowRenderPlan(structureScene);
		if (structurePlan.kind !== "entities") {
			throw new Error("expected Structure entity plan");
		}
		expect(structurePlan.level).toBe("structure");
		expect(structurePlan.edgeLabelIds.has(fixedEdge.id)).toBe(true);

		const overviewNodes = nodes(601);
		const overviewEdges = [
			{
				id: "fixed",
				from: overviewNodes[0]?.id ?? "missing",
				to: overviewNodes[1]?.id ?? "missing",
				lower: "0",
				capacity: "2",
				cost: "-3",
			},
			{
				id: "ordinary",
				from: overviewNodes[0]?.id ?? "missing",
				to: overviewNodes[1]?.id ?? "missing",
				lower: "0",
				capacity: "5",
				cost: "4",
			},
		];
		const overviewScene = scene(overviewNodes, overviewEdges, [
			{
				edge_id: "fixed",
				direction: "forward",
				from: overviewEdges[0]?.from ?? "missing",
				to: overviewEdges[0]?.to ?? "missing",
				capacity: "2",
				cost: "-3",
				active: false,
				fixed: true,
			},
			{
				edge_id: "fixed",
				direction: "reverse",
				from: overviewEdges[0]?.to ?? "missing",
				to: overviewEdges[0]?.from ?? "missing",
				capacity: "0",
				cost: "3",
				active: false,
				fixed: true,
			},
		]);
		const overviewPlan = buildFlowRenderPlan(overviewScene);
		if (overviewPlan.kind !== "overview") {
			throw new Error("expected Overview plan");
		}
		expect(
			overviewPlan.originalEdges.reduce(
				(sum, edge) => sum + edge.fixedCount,
				0,
			),
		).toBe(1);
		expect(
			overviewPlan.residualArcs.reduce((sum, arc) => sum + arc.fixedCount, 0),
		).toBe(2);
	});

	it("summarizes the hard-cap graph without dropping capacity or entities", () => {
		const graphNodes = nodes(10_000);
		const graphEdges = edges(100_000, graphNodes.length);
		const plan = buildFlowRenderPlan(scene(graphNodes, graphEdges));

		expect(plan.kind).toBe("overview");
		if (plan.kind !== "overview") throw new Error("expected overview plan");
		expect(plan.clusters.length).toBeGreaterThan(0);
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.memberCount, 0),
		).toBe(10_000);
		expect(
			plan.originalEdges.reduce((sum, edge) => sum + edge.edgeCount, 0),
		).toBe(100_000);
		expect(
			plan.originalEdges.reduce((sum, edge) => sum + edge.capacity, 0n),
		).toBe(100_000n);
		expect(
			plan.originalEdges.length + plan.residualArcs.length,
		).toBeLessThanOrEqual(FLOW_LOD_LIMITS.overviewAggregateMarks);
		expect(plan.clusters.length).toBeLessThanOrEqual(
			plan.grid.columns * plan.grid.rows,
		);
	});

	it("keeps an exact cost range when an overview route mixes cost signs", () => {
		const graphNodes = nodes(601);
		const graphEdges: FlowEdgeV1[] = [
			{
				id: "negative",
				from: graphNodes[0]?.id ?? "missing",
				to: graphNodes[1]?.id ?? "missing",
				lower: "0",
				capacity: "2",
				cost: "-3",
			},
			{
				id: "positive",
				from: graphNodes[0]?.id ?? "missing",
				to: graphNodes[1]?.id ?? "missing",
				lower: "0",
				capacity: "5",
				cost: "4",
			},
		];
		const plan = buildFlowRenderPlan(scene(graphNodes, graphEdges));
		if (plan.kind !== "overview") throw new Error("expected overview plan");
		const mixed = plan.originalEdges.find((edge) => edge.costKind === "mixed");

		expect(mixed).toMatchObject({
			edgeCount: 2,
			capacity: 7n,
			minimumCost: -3n,
			maximumCost: 4n,
		});
	});

	it("promotes dense GRIDGEN structure and preserves balances and supernode identity", () => {
		const gridNodes = Array.from({ length: 10 * 50 }, (_, index) => {
			const row = Math.floor(index / 50);
			const column = index % 50;
			return {
				id: `g${row.toString().padStart(4, "0")}c${column.toString().padStart(4, "0")}`,
				supply: index === 0 ? "7" : index === 499 ? "-7" : "0",
				position: {
					x: (40 + (720 * column) / 49).toString(),
					y: (40 + (460 * row) / 9).toString(),
				},
			};
		});
		const current = scene(
			[
				...gridNodes,
				{ id: "super", supply: "0", position: { x: "880", y: "270" } },
			],
			[],
		);
		current.model = { kind: "transshipment" };

		expect(automaticFlowLod(501, 0)).toBe("structure");
		expect(flowLodForScene(current)).toBe("overview");
		const plan = buildFlowRenderPlan(current);
		if (plan.kind !== "overview") throw new Error("expected overview plan");
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.supplyCount, 0),
		).toBe(1);
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.demandCount, 0),
		).toBe(1);
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.netBalance, 0n),
		).toBe(0n);
		expect(plan.clusters.some((cluster) => cluster.containsSupernode)).toBe(
			true,
		);
	});

	it("promotes a dense GOTO torus while preserving its supply and demand", () => {
		const gotoNodes = Array.from({ length: 4 * 16 }, (_, index) => {
			const row = Math.floor(index / 16);
			const column = index % 16;
			return {
				id: `t${row.toString().padStart(4, "0")}c${column.toString().padStart(4, "0")}`,
				supply: index === 0 ? "11" : index === 63 ? "-11" : "0",
				position: {
					x: (52 + (708 * column) / 15).toString(),
					y: (48 + (444 * row) / 3).toString(),
				},
			};
		});
		const gotoEdges = Array.from({ length: 512 }, (_, index) => ({
			id: `e${index.toString().padStart(6, "0")}`,
			from: gotoNodes[index % gotoNodes.length]?.id ?? "missing",
			to: gotoNodes[(index * 17 + 1) % gotoNodes.length]?.id ?? "missing",
			lower: "0",
			capacity: "1",
			cost: (index % 9).toString(),
		}));
		const current = scene(gotoNodes, gotoEdges);
		current.model = { kind: "transshipment" };

		expect(automaticFlowLod(64, 512)).toBe("structure");
		expect(flowLodForScene(current)).toBe("overview");
		const plan = buildFlowRenderPlan(current);
		if (plan.kind !== "overview") throw new Error("expected overview plan");
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.supplyCount, 0),
		).toBe(1);
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.demandCount, 0),
		).toBe(1);
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.netBalance, 0n),
		).toBe(0n);
	});

	it("keeps readable GRIDGRAPH detailed and promotes a crowded square", () => {
		const gridgraphScene = (rows: number, columns: number) => {
			const gridNodes = Array.from({ length: rows * columns }, (_, index) => {
				const row = Math.floor(index / columns);
				const column = index % columns;
				return {
					id: `q${row.toString().padStart(4, "0")}c${column.toString().padStart(4, "0")}`,
					supply: "0",
					position: {
						x: (168 + (564 * column) / (columns - 1)).toString(),
						y: (68 + (404 * row) / (rows - 1)).toString(),
					},
				};
			});
			const graphNodes: FlowNodeV1[] = [
				{ id: "s", supply: "19", position: { x: "68", y: "270" } },
				...gridNodes,
				{ id: "t", supply: "-19", position: { x: "832", y: "270" } },
			];
			const edgeCount = 2 * rows * columns + rows - columns;
			const graphEdges = Array.from({ length: edgeCount }, (_, index) => ({
				id: `e${index.toString().padStart(6, "0")}`,
				from: graphNodes[index % graphNodes.length]?.id ?? "missing",
				to: graphNodes[(index + 1) % graphNodes.length]?.id ?? "missing",
				lower: "0",
				capacity: "7",
				cost: "11",
			}));
			const current = scene(graphNodes, graphEdges);
			current.model = { kind: "transshipment" };
			return current;
		};

		const readable = gridgraphScene(6, 8);
		expect(isGridgraphScene(readable)).toBe(true);
		expect(flowLodForScene(readable)).toBe("structure");
		expect(flowLodForScene(gridgraphScene(2, 39))).toBe("overview");
		expect(flowLodForScene(gridgraphScene(7, 11))).toBe("overview");
		expect(flowLodForScene(gridgraphScene(48, 16))).toBe("overview");
		expect(flowLodForScene(gridgraphScene(16, 48))).toBe("overview");

		const square = gridgraphScene(24, 24);
		expect(automaticFlowLod(578, 1_152)).toBe("structure");
		expect(flowLodForScene(square)).toBe("overview");
		const plan = buildFlowRenderPlan(square);
		if (plan.kind !== "overview")
			throw new Error("expected GRIDGRAPH overview plan");
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.supplyCount, 0),
		).toBe(1);
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.demandCount, 0),
		).toBe(1);
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.netBalance, 0n),
		).toBe(0n);
	});

	it("keeps readable Washington levels detailed and promotes crowded shapes", () => {
		const washingtonScene = (rows: number, columns: number) => {
			const levelNodes = Array.from({ length: rows * columns }, (_, index) => {
				const column = Math.floor(index / rows);
				const row = index % rows;
				return {
					id: `w${column.toString().padStart(4, "0")}r${row.toString().padStart(4, "0")}`,
					supply: "0",
					position: {
						x: (168 + (564 * column) / (columns - 1)).toString(),
						y: (68 + (404 * row) / (rows - 1)).toString(),
					},
				};
			});
			const graphNodes: FlowNodeV1[] = [
				{ id: "s", supply: "0", position: { x: "68", y: "270" } },
				...levelNodes,
				{ id: "t", supply: "0", position: { x: "832", y: "270" } },
			];
			const edgeCount = 3 * rows * columns - rows;
			const graphEdges = Array.from({ length: edgeCount }, (_, index) => ({
				id: `e${index.toString().padStart(6, "0")}`,
				from: graphNodes[index % graphNodes.length]?.id ?? "missing",
				to: graphNodes[(index + 1) % graphNodes.length]?.id ?? "missing",
				lower: "0",
				capacity: ((index % 100) + 1).toString(),
				cost: "0",
			}));
			return scene(graphNodes, graphEdges);
		};

		const readable = washingtonScene(6, 8);
		expect(isWashingtonRandomLevelScene(readable)).toBe(true);
		expect(flowLodForScene(readable)).toBe("structure");
		expect(flowLodForScene(washingtonScene(6, 24))).toBe("overview");
		expect(flowLodForScene(washingtonScene(24, 6))).toBe("overview");
		expect(flowLodForScene(washingtonScene(32, 32))).toBe("overview");

		const densePlan = buildFlowRenderPlan(washingtonScene(32, 32));
		if (densePlan.kind !== "overview")
			throw new Error("expected Washington overview plan");
		expect(
			densePlan.clusters.reduce((sum, cluster) => sum + cluster.memberCount, 0),
		).toBe(1_026);
		expect(
			densePlan.originalEdges.reduce((sum, edge) => sum + edge.edgeCount, 0),
		).toBe(3_040);
	});

	it("uses actual NETGEN lane spacing to avoid terminal and chain overlap", () => {
		const netgenNodes = (
			sourceCount: number,
			sinkCount: number,
			middleCount: number,
		) => [
			...Array.from({ length: sourceCount }, (_, index) => ({
				id: `s${index.toString().padStart(4, "0")}`,
				supply: sinkCount.toString(),
				position: {
					x: "60",
					y: Math.floor(
						50 + (440 * (index + 1)) / (sourceCount + 1),
					).toString(),
				},
			})),
			...Array.from({ length: middleCount }, (_, index) => ({
				id: `x${index.toString().padStart(4, "0")}`,
				supply: "0",
				position: {
					x: (180 + ((index * 97) % 620)).toString(),
					y: (48 + ((index * 71) % 445)).toString(),
				},
			})),
			...Array.from({ length: sinkCount }, (_, index) => ({
				id: `t${index.toString().padStart(4, "0")}`,
				supply: (-sourceCount).toString(),
				position: {
					x: "940",
					y: Math.floor(50 + (440 * (index + 1)) / (sinkCount + 1)).toString(),
				},
			})),
		];
		const netgenEdges = (graphNodes: FlowNodeV1[], count: number) =>
			Array.from({ length: count }, (_, index) => ({
				id: `e${index.toString().padStart(6, "0")}`,
				from: graphNodes[index % graphNodes.length]?.id ?? "missing",
				to: graphNodes[(index * 17 + 1) % graphNodes.length]?.id ?? "missing",
				lower: "0",
				capacity: "30",
				cost: (index % 11 === 0 ? -5 : index % 20).toString(),
			}));

		const transportationNodes = netgenNodes(12, 12, 0);
		const transportation = scene(
			transportationNodes,
			netgenEdges(transportationNodes, 96),
		);
		transportation.model = { kind: "transshipment" };
		expect(automaticFlowLod(24, 96)).toBe("structure");
		expect(flowLodForScene(transportation)).toBe("overview");

		const fiveTerminalNodes = netgenNodes(5, 5, 0);
		const fiveTerminal = scene(
			fiveTerminalNodes,
			netgenEdges(fiveTerminalNodes, 25),
		);
		fiveTerminal.model = { kind: "transshipment" };
		expect(flowLodForScene(fiveTerminal)).toBe("detail");

		const sixTerminalNodes = netgenNodes(6, 6, 0);
		const sixTerminal = scene(
			sixTerminalNodes,
			netgenEdges(sixTerminalNodes, 36),
		);
		sixTerminal.model = { kind: "transshipment" };
		expect(flowLodForScene(sixTerminal)).toBe("overview");

		const longChainNodes: FlowNodeV1[] = [
			{ id: "s0000", supply: "1", position: { x: "60", y: "270" } },
			...Array.from({ length: 78 }, (_, index) => ({
				id: `x${index.toString().padStart(4, "0")}`,
				supply: "0",
				position: {
					x: (160 + (680 * (index + 1)) / 79).toString(),
					y: "270",
				},
			})),
			{ id: "t0000", supply: "-1", position: { x: "940", y: "270" } },
		];
		const longChain = scene(longChainNodes, netgenEdges(longChainNodes, 80));
		longChain.model = {
			kind: "max-flow",
			source: "s0000",
			sink: "t0000",
		};
		expect(automaticFlowLod(80, 80)).toBe("structure");
		expect(flowLodForScene(longChain)).toBe("overview");

		const focusedNodes: FlowNodeV1[] = [
			{ id: "s0000", supply: "0", position: { x: "60", y: "270" } },
			...Array.from({ length: 28 }, (_, index) => ({
				id: `x${index.toString().padStart(4, "0")}`,
				supply: "0",
				position: {
					x: (150 + index * 25).toString(),
					y: "270",
				},
			})),
			{ id: "t0000", supply: "0", position: { x: "940", y: "270" } },
		];
		const focusedEdges = [
			...focusedNodes.slice(1, -1).flatMap((node, index) => [
				{
					id: `source-${index}`,
					from: "s0000",
					to: node.id,
					lower: "0",
					capacity: "10",
					cost: "0",
				},
				{
					id: `sink-${index}`,
					from: node.id,
					to: "t0000",
					lower: "0",
					capacity: "10",
					cost: "0",
				},
			]),
			...netgenEdges(focusedNodes, 44).map((edge, index) => ({
				...edge,
				id: `extra-${index}`,
			})),
		];
		const focused = scene(focusedNodes, focusedEdges);
		focused.model = {
			kind: "max-flow",
			source: "s0000",
			sink: "t0000",
		};
		expect(focusedEdges).toHaveLength(100);
		expect(flowLodForScene(focused)).toBe("structure");

		const maxFlowBoundary = (count: number) => {
			const graphNodes: FlowNodeV1[] = [
				{ id: "s0000", supply: "0", position: { x: "60", y: "270" } },
				...Array.from({ length: count - 2 }, (_, index) => ({
					id: `x${index.toString().padStart(4, "0")}`,
					supply: "0",
					position: { x: String(150 + index * 10), y: "270" },
				})),
				{ id: "t0000", supply: "0", position: { x: "940", y: "270" } },
			];
			const current = scene(
				graphNodes,
				netgenEdges(graphNodes, Math.max(70, count)),
			);
			current.model = {
				kind: "max-flow",
				source: "s0000",
				sink: "t0000",
			};
			return current;
		};
		expect(flowLodForScene(maxFlowBoundary(20))).toBe("structure");
		expect(flowLodForScene(maxFlowBoundary(21))).toBe("structure");
		expect(flowLodForScene(maxFlowBoundary(58))).toBe("structure");
		expect(flowLodForScene(maxFlowBoundary(59))).toBe("structure");
		expect(flowLodForScene(maxFlowBoundary(79))).toBe("structure");
		expect(flowLodForScene(maxFlowBoundary(80))).toBe("overview");

		const completeDagNodes: FlowNodeV1[] = Array.from(
			{ length: 40 },
			(_, index) => ({
				id:
					index === 0
						? "s"
						: index === 39
							? "t"
							: `v${index.toString().padStart(2, "0")}`,
				supply: "0",
				position: {
					x: String(80 + (760 * index) / 39),
					y: String(80 + (index % 5) * 95),
				},
			}),
		);
		const completeDagEdges = completeDagNodes.flatMap((from, fromIndex) =>
			completeDagNodes.slice(fromIndex + 1).map((to, offset) => ({
				id: `dag-${fromIndex}-${fromIndex + offset + 1}`,
				from: from.id,
				to: to.id,
				lower: "0",
				capacity: "1",
				cost: "0",
			})),
		);
		const completeDag = scene(completeDagNodes, completeDagEdges);
		completeDag.model = { kind: "max-flow", source: "s", sink: "t" };
		expect(completeDag.graph.edges).toHaveLength(780);
		expect(automaticFlowLod(40, 780)).toBe("structure");
		expect(flowLodForScene(completeDag)).toBe("overview");
		const completeDagPlan = buildFlowRenderPlan(completeDag);
		if (completeDagPlan.kind !== "overview")
			throw new Error("expected dense complete DAG overview plan");
		expect(
			completeDagPlan.originalEdges.reduce(
				(sum, edge) => sum + edge.edgeCount,
				0,
			),
		).toBe(780);

		const denseNodes = netgenNodes(4, 8, 28);
		const dense = scene(denseNodes, netgenEdges(denseNodes, 1_000));
		dense.model = { kind: "transshipment" };
		expect(automaticFlowLod(40, 1_000)).toBe("structure");
		expect(flowLodForScene(dense)).toBe("overview");
		const plan = buildFlowRenderPlan(dense);
		if (plan.kind !== "overview")
			throw new Error("expected NETGEN overview plan");
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.supplyCount, 0),
		).toBe(4);
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.demandCount, 0),
		).toBe(8);
		expect(
			plan.clusters.reduce((sum, cluster) => sum + cluster.netBalance, 0n),
		).toBe(0n);

		dense.node_trace_states = dense.graph.nodes.map((node) => ({
			node_id: node.id,
		}));
		dense.trace_event = {
			event_id: "1",
			catalog_id: "test.touch-one",
			minimum_granularity: "operation",
			pseudocode_line: "test:touch-one",
			patch_count: 1,
			entity_refs: [{ kind: "node", node_id: dense.graph.nodes[0]?.id ?? "" }],
		};
		dense.trace_event_semantics = {
			role: "mutate",
			work_deltas: [
				{ unit: "published-transition", count: "1" },
				{ unit: "detail-primitive", count: "1" },
			],
			aggregation_count: "1",
			work_progress: {
				detail_completed: "1",
				detail_total: "1",
				primary_completed: "0",
				primary_total: "0",
			},
			changed_entity_refs: [
				{ kind: "node", node_id: dense.graph.nodes[1]?.id ?? "" },
			],
		};
		const touchedPlan = buildFlowRenderPlan(dense);
		if (touchedPlan.kind !== "overview")
			throw new Error("expected NETGEN overview plan");
		expect(
			touchedPlan.clusters.reduce(
				(sum, cluster) => sum + cluster.traceCount,
				0,
			),
		).toBe(1);
		expect(
			touchedPlan.clusters.flatMap((cluster) => cluster.traceIdentities),
		).toEqual([`node:${dense.graph.nodes[0]?.id ?? ""}`]);
		expect(
			touchedPlan.clusters.reduce(
				(sum, cluster) => sum + cluster.changeCount,
				0,
			),
		).toBe(1);
		expect(
			touchedPlan.clusters.flatMap((cluster) => cluster.changedIdentities),
		).toEqual([`node:${dense.graph.nodes[1]?.id ?? ""}`]);
	});

	it("skips pairwise NETGEN spacing after count-based Overview is certain", () => {
		const graphNodes: FlowNodeV1[] = [
			{ id: "s0000", supply: "0", position: { x: "60", y: "270" } },
			...Array.from({ length: 9_998 }, (_, index) => ({
				id: `x${index.toString().padStart(4, "0")}`,
				supply: "0",
				position: { x: String(150 + (index % 600)), y: "270" },
			})),
			{ id: "t0000", supply: "0", position: { x: "940", y: "270" } },
		];
		const current = scene(graphNodes, []);
		current.model = {
			kind: "max-flow",
			source: "s0000",
			sink: "t0000",
		};
		const hypot = vi.spyOn(Math, "hypot");
		try {
			expect(flowLodForScene(current)).toBe("overview");
			expect(hypot).not.toHaveBeenCalled();
		} finally {
			hypot.mockRestore();
		}
	});
});
