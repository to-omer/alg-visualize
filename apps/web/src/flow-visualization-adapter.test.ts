import { describe, expect, it } from "vitest";

import {
	decodeFlowCurrentSceneV9,
	type FlowCurrentSceneV9,
} from "./flow-scene";
import { buildFlowEntityVisualization } from "./flow-visualization-adapter";

function baseScene(algorithmId = "edmonds-karp"): FlowCurrentSceneV9 {
	const graph = {
		nodes: ["s", "a", "b", "t"].map((id) => ({ id, supply: "0" })),
		edges: [
			{ id: "sa", from: "s", to: "a", lower: "0", capacity: "2", cost: "0" },
			{ id: "ab", from: "a", to: "b", lower: "0", capacity: "1", cost: "0" },
			{ id: "bt", from: "b", to: "t", lower: "0", capacity: "2", cost: "0" },
		],
	};
	return {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "0",
		event_count: "0",
		solve_status: "ready",
		model: { kind: "max-flow", source: "s", sink: "t" },
		graph,
		algorithm: { id: algorithmId, config: {} },
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
		edge_states: graph.edges.map((edge) => ({ edge_id: edge.id, flow: "0" })),
		residual_arcs: [],
		node_trace_states: [],
		metrics: Array.from(
			{ length: 16 },
			() => "0",
		) as FlowCurrentSceneV9["metrics"],
	};
}

describe("flow entity visualization adapter", () => {
	it.each([
		["dynamic-tree-blocking-flow", "dynamicTreeBlocking"],
		["dynamic-tree-push-relabel", "dynamicTreePushRelabel"],
		["distance-directed-augmenting-path", "distanceDirected"],
		["cost-scaling-push-relabel", "costScaling"],
		["dynamic-tree-network-simplex", "dynamicTreeNetworkSimplex"],
		["convex-cost-scaling", "convexCost"],
		["prediction-assisted-epsilon-relaxation", "predictionAssisted"],
	] as const)("classifies %s as feature %s", (algorithmId, feature) => {
		const visual = buildFlowEntityVisualization(baseScene(algorithmId));
		expect(visual.features[feature]).toBe(true);
	});

	it("projects cut, matching, and optional initial-flow annotations", () => {
		const cutScene = baseScene("warm-start-push-relabel");
		cutScene.outcome = {
			kind: "max-flow",
			value: "1",
			cut_bound: "1",
			source_side: ["s", "a"],
		};
		const firstEdge = cutScene.graph.edges[0];
		if (firstEdge === undefined) throw new Error("missing first fixture edge");
		cutScene.graph.edges[0] = {
			...firstEdge,
			initial_flow: "1",
		};
		const cutVisual = buildFlowEntityVisualization(cutScene);
		expect(cutVisual.sourceSide).toEqual(new Set(["s", "a"]));
		expect(cutVisual.predictedOriginalEdges).toEqual(new Set(["sa"]));

		const matchingScene = baseScene("hungarian");
		matchingScene.model = {
			kind: "assignment",
			agents: ["s", "a"],
			tasks: ["b", "t"],
			objective: "minimize",
		};
		matchingScene.edge_states = [
			{ edge_id: "sa", flow: "0" },
			{ edge_id: "ab", flow: "1" },
			{ edge_id: "bt", flow: "0" },
		];
		expect(
			buildFlowEntityVisualization(matchingScene).matchedOriginalEdges,
		).toEqual(new Set(["ab"]));
	});

	it("builds IBFS forest indexes from optional forest wire state", () => {
		const scene = baseScene("ibfs");
		scene.node_trace_states = [
			{ node_id: "s", label: "0" },
			{ node_id: "a", label: "1", search_ordinal: 0 },
			{ node_id: "b", label: "-2", search_ordinal: 1 },
			{ node_id: "t", label: "-1" },
		];
		scene.residual_arcs = [
			{
				edge_id: "sa",
				direction: "forward",
				from: "s",
				to: "a",
				capacity: "1",
				cost: "0",
				active: true,
				fixed: false,
			},
			{
				edge_id: "bt",
				direction: "forward",
				from: "b",
				to: "t",
				capacity: "1",
				cost: "0",
				active: false,
				fixed: false,
			},
			{
				edge_id: "bt",
				direction: "reverse",
				from: "t",
				to: "b",
				capacity: "1",
				cost: "0",
				active: false,
				fixed: false,
			},
		];
		scene.pseudoflow_forest = {
			arcs: [
				{ edge_id: "sa", direction: "forward" },
				{ edge_id: "bt", direction: "reverse" },
			],
			strong_nodes: [],
		};
		const visual = buildFlowEntityVisualization(scene);
		expect(visual.ibfsForestByEdge.get("sa")?.side).toBe("source");
		expect(visual.ibfsForestByEdge.get("bt")?.side).toBe("sink");
	});

	it("builds EIBFS edge indexes from its typed optional overlay", () => {
		const raw = baseScene("eibfs") as unknown as Record<string, unknown>;
		raw.node_trace_states = [
			{ node_id: "a", label: "1", remaining_divergence: "0" },
			{ node_id: "b", label: "-2", remaining_divergence: "0" },
			{ node_id: "s", label: "0", remaining_divergence: "-1" },
			{ node_id: "t", label: "-1", remaining_divergence: "1" },
		];
		raw.residual_arcs = [
			{
				edge_id: "sa",
				direction: "forward",
				from: "s",
				to: "a",
				capacity: "2",
				cost: "0",
				active: false,
			},
			{
				edge_id: "sa",
				direction: "reverse",
				from: "a",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
			{
				edge_id: "ab",
				direction: "forward",
				from: "a",
				to: "b",
				capacity: "1",
				cost: "0",
				active: false,
			},
			{
				edge_id: "ab",
				direction: "reverse",
				from: "b",
				to: "a",
				capacity: "0",
				cost: "0",
				active: false,
			},
			{
				edge_id: "bt",
				direction: "forward",
				from: "b",
				to: "t",
				capacity: "2",
				cost: "0",
				active: false,
			},
			{
				edge_id: "bt",
				direction: "reverse",
				from: "t",
				to: "b",
				capacity: "0",
				cost: "0",
				active: false,
			},
		];
		raw.eibfs_overlay = {
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
					imbalance: "-1",
				},
				{
					node_id: "t",
					source_label: "0",
					sink_label: "0",
					membership: "sink",
					root_kind: "sink",
					orphan: false,
					imbalance: "1",
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
		};
		const scene = decodeFlowCurrentSceneV9(
			new TextEncoder().encode(JSON.stringify(raw)),
		);
		const visual = buildFlowEntityVisualization(scene);
		expect(visual.eibfsForestByEdge.get("sa")).toMatchObject({
			side: "source",
			parent: "s",
			child: "a",
		});
		expect(visual.eibfsForestByEdge.get("bt")?.direction).toBe("reverse");
	});
});
