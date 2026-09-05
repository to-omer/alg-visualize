import { describe, expect, it } from "vitest";

import {
	decodeFlowCurrentSceneV9,
	type LegacyFlowSceneMigrationCatalog,
	migrateFlowCurrentSceneV6,
	migrateFlowCurrentSceneV7,
	projectLinearMcfRequiredDivergence,
} from "./flow-scene";

function validScene(): Record<string, unknown> {
	return {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "0",
		event_count: "0",
		solve_status: "ready",
		model: { kind: "max-flow", source: "s", sink: "t" },
		graph: {
			nodes: [
				{ id: "s", supply: "0", position: { x: "0", y: "0" } },
				{ id: "t", supply: "0" },
			],
			edges: [
				{
					id: "st",
					from: "s",
					to: "t",
					lower: "0",
					capacity: "18446744073709551615",
					cost: "-7",
				},
			],
		},
		algorithm: { id: "edmonds-karp", config: {} },
		run_profile: "trace",
		trace_granularity: "operation",
		trace_steps: {
			phase_unit:
				"one boundary that starts or completes a residual-path search",
			phase_availability: { availability: "available" },
			operation_unit: "one completed flow augmentation",
			operation_availability: { availability: "available" },
			detail: {
				availability: "available",
				unit: "one residual-arc inspection, augmenting-path prefix extension, or bottleneck computation",
			},
			primary_work: {
				metric_ordinal: 2,
				unit: "residual-arc inspections",
				abstraction: "primitive",
				visualization: "edge-field",
			},
		},
		edge_states: [{ edge_id: "st", flow: "0" }],
		residual_arcs: [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "18446744073709551615",
				cost: "-7",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "7",
				active: false,
			},
		],
		node_trace_states: [{ node_id: "s" }, { node_id: "t" }],
		metrics: Array.from({ length: 16 }, () => "0"),
	};
}

function legacyMigrationCatalog(
	scene: Record<string, unknown>,
	algorithmId = "edmonds-karp",
): LegacyFlowSceneMigrationCatalog {
	return [
		{
			id: algorithmId,
			trace_steps: structuredClone(
				scene.trace_steps,
			) as LegacyFlowSceneMigrationCatalog[number]["trace_steps"],
		},
	];
}

function flowFrameworkMcfScene(
	stage: "query" | "optimal" = "query",
): Record<string, unknown> {
	const optimal = stage === "optimal";
	const value = validScene();
	value.event_id = optimal ? "10" : "1";
	value.event_count = "10";
	value.solve_status = optimal ? "optimal" : "running";
	value.model = { kind: "transshipment" };
	value.algorithm = { id: "deterministic-almost-linear-mcf", config: {} };
	value.graph = {
		nodes: [
			{ id: "m", supply: "0" },
			{ id: "s", supply: "3" },
			{ id: "t", supply: "-3" },
		],
		edges: [
			{ id: "a", from: "s", to: "m", lower: "0", capacity: "3", cost: "1" },
			{ id: "b", from: "m", to: "t", lower: "0", capacity: "3", cost: "1" },
			{
				id: "expensive",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "3",
				cost: "5",
			},
		],
	};
	const finalFlows = ["3", "3", "0"];
	value.edge_states = ["a", "b", "expensive"].map((edgeId, index) => ({
		edge_id: edgeId,
		flow: optimal ? (finalFlows[index] as string) : "0",
	}));
	const graph = value.graph as {
		edges: {
			id: string;
			from: string;
			to: string;
			capacity: string;
			cost: string;
		}[];
	};
	value.residual_arcs = graph.edges.flatMap((edge, index) => {
		const flow = optimal ? BigInt(finalFlows[index] as string) : 0n;
		return [
			{
				edge_id: edge.id,
				direction: "forward",
				from: edge.from,
				to: edge.to,
				capacity: (BigInt(edge.capacity) - flow).toString(),
				cost: edge.cost,
				active: false,
			},
			{
				edge_id: edge.id,
				direction: "reverse",
				from: edge.to,
				to: edge.from,
				capacity: flow.toString(),
				cost: (-BigInt(edge.cost)).toString(),
				active: false,
			},
		];
	});
	value.node_trace_states = ["m", "s", "t"].map((nodeId) => ({
		node_id: nodeId,
	}));
	value.trace_event = {
		event_id: value.event_id,
		catalog_id: optimal
			? "deterministic-almost-linear-mcf.optimal"
			: "deterministic-almost-linear-mcf.query-minimum-ratio-cycle",
		minimum_granularity: "phase",
		pseudocode_line: optimal
			? "publish after the source additive-half gate and checked rounding"
			: "Query the topology-aware shifted tree chain",
		patch_count: optimal ? 1 : 4,
		entity_refs: optimal
			? []
			: ["a", "b", "expensive"].map((edgeId) => ({
					kind: "edge",
					edge_id: edgeId,
				})),
		detail: {
			label: optimal ? "termination" : "minimum-ratio cycle",
			value: optimal ? "1" : "3",
		},
	};
	value.flow_framework_mcf_overlay = {
		stage: optimal ? "optimal" : "query-minimum-ratio-cycle",
		...(optimal
			? {}
			: {
					dynamic_operation: "cycle-queried-accepted",
					dynamic_operation_serial: "1",
				}),
		iteration: optimal ? "3" : "1",
		reinitialized: optimal,
		potential_before: "96.25",
		potential_after: "94.20",
		gap_before: optimal ? "0.75" : "4.5",
		gap_after: optimal ? "0.25" : "4.3",
		exact_gap_before: {
			numerator: optimal ? "3" : "9",
			denominator: optimal ? "4" : "2",
		},
		exact_gap_after: {
			numerator: optimal ? "1" : "43",
			denominator: optimal ? "4" : "10",
		},
		stopping_gap: { numerator: "1", denominator: "2" },
		accepted_ratio: { numerator: "10", denominator: "1" },
		target_progress: { numerator: "2", denominator: "1" },
		...(optimal ? { termination: "source-additive-half-gap" } : {}),
		...(optimal
			? {
					optimum_cost: "6",
					final_point_nodes: [
						{ node_id: "m", required_divergence: "0" },
						{ node_id: "s", required_divergence: "3" },
						{ node_id: "t", required_divergence: "-3" },
						{ node_id: "aux", required_divergence: "0" },
					],
					final_point_edges: [
						{
							edge_id: "a",
							from: "s",
							to: "m",
							lower: "0",
							capacity: "3",
							cost: "1",
							flow: { numerator: "35", denominator: "12" },
							auxiliary: false,
							rounded_flow: "3",
						},
						{
							edge_id: "b",
							from: "m",
							to: "t",
							lower: "0",
							capacity: "3",
							cost: "1",
							flow: { numerator: "35", denominator: "12" },
							auxiliary: false,
							rounded_flow: "3",
						},
						{
							edge_id: "expensive",
							from: "s",
							to: "t",
							lower: "0",
							capacity: "3",
							cost: "5",
							flow: { numerator: "1", denominator: "12" },
							auxiliary: false,
							rounded_flow: "0",
						},
						{
							edge_id: "aux-s",
							from: "aux",
							to: "s",
							lower: "0",
							capacity: "3",
							cost: "100",
							flow: { numerator: "0", denominator: "1" },
							auxiliary: true,
							rounded_flow: "0",
						},
					],
				}
			: {}),
		levels: [
			{ level: "0", active_branch: "0", passes: "0" },
			{ level: "1", active_branch: "0", passes: "0" },
		],
		edges: [
			{
				edge_id: "a",
				flow: {
					numerator: optimal ? "3" : "3",
					denominator: optimal ? "1" : "2",
				},
				cycle_coefficient: { numerator: optimal ? "0" : "1", denominator: "1" },
				selected: !optimal,
			},
			{
				edge_id: "b",
				flow: {
					numerator: optimal ? "3" : "3",
					denominator: optimal ? "1" : "2",
				},
				cycle_coefficient: { numerator: optimal ? "0" : "1", denominator: "1" },
				selected: !optimal,
			},
			{
				edge_id: "expensive",
				flow: {
					numerator: optimal ? "0" : "3",
					denominator: optimal ? "1" : "2",
				},
				cycle_coefficient: {
					numerator: optimal ? "0" : "-1",
					denominator: "1",
				},
				selected: !optimal,
			},
		],
	};
	if (optimal) {
		value.outcome = {
			kind: "min-cost-flow",
			total_cost: "6",
			potentials: [
				{ node_id: "m", potential: "1" },
				{ node_id: "s", potential: "0" },
				{ node_id: "t", potential: "2" },
			],
		};
	}
	return value;
}

function randomizedAlmostLinearMcfScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "15";
	value.event_count = "15";
	value.solve_status = "optimal";
	value.model = { kind: "transshipment" };
	value.algorithm = {
		id: "randomized-almost-linear-mcf-oracle-demonstrator",
		config: {},
	};
	value.graph = {
		nodes: [
			{ id: "s", supply: "1" },
			{ id: "t", supply: "-1" },
		],
		edges: [
			{ id: "st", from: "s", to: "t", lower: "0", capacity: "2", cost: "3" },
		],
	};
	value.edge_states = [{ edge_id: "st", flow: "1" }];
	value.residual_arcs = [
		{
			edge_id: "st",
			direction: "forward",
			from: "s",
			to: "t",
			capacity: "1",
			cost: "3",
			active: false,
		},
		{
			edge_id: "st",
			direction: "reverse",
			from: "t",
			to: "s",
			capacity: "1",
			cost: "-3",
			active: false,
		},
	];
	value.node_trace_states = [
		{ node_id: "s", label: "0", remaining_divergence: "0" },
		{ node_id: "t", label: "1", remaining_divergence: "0" },
	];
	value.trace_event = {
		event_id: "15",
		parent_phase_id: "5",
		catalog_id: "randomized-almost-linear-mcf-oracle-demonstrator.optimal",
		minimum_granularity: "phase",
		pseudocode_line: "publish the certified original-cost optimum",
		patch_count: 1,
		entity_refs: [{ kind: "edge", edge_id: "st" }],
		detail: { label: "optimum cost", value: "3" },
	};
	value.randomized_almost_linear_mcf_overlay = {
		stage: "optimal",
		seed: "7",
		alpha: "0.001",
		epsilon: "0.000001",
		kappa: "0",
		eta: "0",
		initial_cost: "3",
		current_cost: "3",
		optimum_cost: "3",
		isolated_optimum_cost: "49",
		potential: "0",
		isolation_attempt: "1",
		isolation_scale: "16",
		failure_numerator: "1",
		failure_denominator: "2",
		forest_pool_size: "1",
		sampled_forest_index: "0",
		final_point_gap: { numerator: "0", denominator: "1" },
		final_point_threshold: { numerator: "1", denominator: "96" },
		final_point_mix: { numerator: "1", denominator: "4" },
		exact_recovery: true,
		feasible_flows: "1",
		detected_coordinates: "0",
		rebuilds: "1",
		nodes: [
			{
				node_id: "s",
				required_divergence: "1",
				component: "0",
				depth: "0",
				on_selected_cycle: false,
			},
			{
				node_id: "t",
				required_divergence: "-1",
				component: "0",
				parent_node_id: "s",
				depth: "1",
				on_selected_cycle: false,
			},
		],
		edges: [
			{
				edge_id: "st",
				fixed_on_face: true,
				initial_flow: "1",
				current_flow: "1",
				stale_flow: "1",
				final_point_flow: { numerator: "1", denominator: "1" },
				final_flow: "1",
				isolation_draw: "1",
				isolated_cost: "49",
				isolated_optimum_flow: "1",
				tree_edge: true,
				candidate_sign: "0",
				selected_sign: "0",
				gradient: "0",
				length: "0",
				detected: false,
			},
		],
	};
	value.outcome = {
		kind: "min-cost-flow",
		total_cost: "3",
		potentials: [
			{ node_id: "s", potential: "0" },
			{ node_id: "t", potential: "3" },
		],
	};
	return value;
}

function convexNetworkSimplexScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "1";
	value.event_count = "1";
	value.solve_status = "optimal";
	value.model = { kind: "convex-cost-flow" };
	value.algorithm = { id: "convex-network-simplex", config: {} };
	value.graph = {
		nodes: [
			{ id: "s", supply: "2" },
			{ id: "t", supply: "-2" },
		],
		edges: [
			{
				id: "st",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "3",
				cost: "0",
				convex_cost: {
					base_cost_at_zero: "0",
					segments: [{ end_flow: "3", marginal_cost: "2" }],
				},
			},
		],
	};
	value.edge_states = [{ edge_id: "st", flow: "2" }];
	value.residual_arcs = [
		{
			edge_id: "st",
			direction: "forward",
			from: "s",
			to: "t",
			capacity: "1",
			cost: "0",
			active: false,
		},
		{
			edge_id: "st",
			direction: "reverse",
			from: "t",
			to: "s",
			capacity: "2",
			cost: "0",
			active: false,
		},
	];
	value.node_trace_states = [
		{ node_id: "s", label: "0", remaining_divergence: "0" },
		{ node_id: "t", label: "2", remaining_divergence: "0" },
	];
	value.trace_event = {
		event_id: "1",
		catalog_id: "convex-network-simplex.certify-expanded-oracle",
		minimum_granularity: "operation",
		pseudocode_line: "convex-network-simplex:compare-expanded-oracle",
		patch_count: 1,
		entity_refs: [],
		detail: { label: "total-cost", value: "4" },
	};
	value.convex_cost_overlay = {
		stage: "optimal",
		edges: [
			{
				edge_id: "st",
				base_cost_at_zero: "0",
				flow: "2",
				total_cost: "4",
				forward_marginal_cost: "2",
				reverse_marginal_cost: "2",
				segments: [
					{
						segment: "0",
						start_flow: "0",
						end_flow: "3",
						flow: "2",
						marginal_cost: "2",
					},
				],
			},
		],
		active_cycle: [],
		eligible_arcs: [],
	};
	value.convex_network_simplex_overlay = {
		stage: "optimal",
		artificial_cost: "10",
		nodes: [
			{ entity_id: "s", potential: "0", parent: "t" },
			{ entity_id: "t", potential: "2", parent: "artificial-root" },
			{ entity_id: "artificial-root", potential: "12" },
		],
		edges: [
			{
				edge_id: "st",
				basis: "tree",
				active_segment: "0",
				in_cycle: false,
				entering: false,
				leaving: false,
			},
		],
		artificial_edges: [
			{
				entity_id: "artificial:s",
				node_id: "s",
				source: "s",
				target: "artificial-root",
				flow: "0",
				basis: "breakpoint",
				in_cycle: false,
				entering: false,
				leaving: false,
			},
			{
				entity_id: "artificial:t",
				node_id: "t",
				source: "t",
				target: "artificial-root",
				flow: "0",
				basis: "tree",
				in_cycle: false,
				entering: false,
				leaving: false,
			},
		],
		cycle: [],
	};
	value.outcome = {
		kind: "min-cost-flow",
		total_cost: "4",
		potentials: [
			{ node_id: "s", potential: "0" },
			{ node_id: "t", potential: "2" },
		],
	};
	return value;
}

function parametricScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "1";
	value.event_count = "2";
	value.solve_status = "running";
	value.model = {
		kind: "parametric-max-flow",
		source: "s",
		sink: "t",
		parameter: {
			minimum: { numerator: "0", denominator: "1" },
			maximum: { numerator: "2", denominator: "1" },
		},
		capacity_slopes: [
			{ edge_id: "at", slope: "-1" },
			{ edge_id: "sa", slope: "1" },
		],
	};
	value.graph = {
		nodes: ["a", "s", "t"].map((id) => ({ id, supply: "0" })),
		edges: [
			{
				id: "at",
				from: "a",
				to: "t",
				lower: "0",
				capacity: "3",
				cost: "0",
			},
			{
				id: "sa",
				from: "s",
				to: "a",
				lower: "0",
				capacity: "1",
				cost: "0",
			},
		],
	};
	value.algorithm = { id: "parametric-pseudoflow", config: {} };
	value.edge_states = [];
	value.residual_arcs = [];
	value.node_trace_states = [];
	value.parametric_overlay = {
		stage: "ready",
		parameter: { numerator: "1", denominator: "1" },
		edge_capacities: [
			{
				edge_id: "at",
				capacity: { numerator: "2", denominator: "1" },
			},
			{
				edge_id: "sa",
				capacity: { numerator: "2", denominator: "1" },
			},
		],
		visual_scale_max_capacity: { numerator: "3", denominator: "1" },
		recorded_segments: [
			{
				lower: { numerator: "0", denominator: "1" },
				upper: { numerator: "1", denominator: "1" },
				intercept: "1",
				slope: "1",
				minimal_source_side: ["s"],
				maximal_source_side: ["s"],
			},
		],
		recorded_breakpoints: [
			{
				parameter: { numerator: "1", denominator: "1" },
				before_source_side: ["s"],
				after_source_side: ["a", "s"],
				exact_minimal_source_side: ["s"],
				exact_maximal_source_side: ["a", "s"],
				entering_nodes: ["a"],
			},
		],
	};
	return value;
}

function matchingScene(): Record<string, unknown> {
	const value = validScene();
	const edges = [
		{
			id: "b00",
			from: "l0",
			to: "r0",
			lower: "0",
			capacity: "1",
			cost: "0",
		},
		{
			id: "b01",
			from: "l0",
			to: "r1",
			lower: "0",
			capacity: "1",
			cost: "0",
		},
		{
			id: "b10",
			from: "l1",
			to: "r0",
			lower: "0",
			capacity: "1",
			cost: "0",
		},
	];
	const flows = new Map([
		["b00", "0"],
		["b01", "1"],
		["b10", "1"],
	]);
	value.solve_status = "optimal";
	value.model = {
		kind: "bipartite-matching",
		left: ["l0", "l1"],
		right: ["r0", "r1"],
	};
	value.graph = {
		nodes: ["l0", "l1", "r0", "r1"].map((id) => ({ id, supply: "0" })),
		edges,
	};
	value.algorithm = { id: "hopcroft-karp", config: {} };
	value.run_profile = "fast";
	value.edge_states = edges.map((edge) => ({
		edge_id: edge.id,
		flow: flows.get(edge.id),
	}));
	value.residual_arcs = edges.flatMap((edge) => {
		const flow = flows.get(edge.id);
		if (flow === undefined) throw new Error("matching fixture flow is missing");
		return [
			{
				edge_id: edge.id,
				direction: "forward",
				from: edge.from,
				to: edge.to,
				capacity: flow === "1" ? "0" : "1",
				cost: "0",
				active: false,
			},
			{
				edge_id: edge.id,
				direction: "reverse",
				from: edge.to,
				to: edge.from,
				capacity: flow,
				cost: "0",
				active: false,
			},
		];
	});
	value.node_trace_states = ["l0", "l1", "r0", "r1"].map((node_id) => ({
		node_id,
	}));
	value.outcome = {
		kind: "bipartite-matching",
		cardinality: "2",
		pairs: [
			{ edge_id: "b01", left: "l0", right: "r1" },
			{ edge_id: "b10", left: "l1", right: "r0" },
		],
		cover_left: ["l0", "l1"],
		cover_right: [],
	};
	return value;
}

function assignmentScene(): Record<string, unknown> {
	const value = validScene();
	const edges = [
		{ id: "e00", from: "a0", to: "t0", lower: "0", capacity: "1", cost: "4" },
		{ id: "e01", from: "a0", to: "t1", lower: "0", capacity: "1", cost: "1" },
		{ id: "e10", from: "a1", to: "t0", lower: "0", capacity: "1", cost: "2" },
		{ id: "e11", from: "a1", to: "t1", lower: "0", capacity: "1", cost: "3" },
		{ id: "e12", from: "a1", to: "t2", lower: "0", capacity: "1", cost: "0" },
	];
	const selected = new Set(["e01", "e12"]);
	value.solve_status = "optimal";
	value.model = {
		kind: "assignment",
		agents: ["a0", "a1"],
		tasks: ["t0", "t1", "t2"],
		objective: "minimize",
	};
	value.graph = {
		nodes: ["a0", "a1", "t0", "t1", "t2"].map((id) => ({ id, supply: "0" })),
		edges,
	};
	value.algorithm = { id: "hungarian", config: {} };
	value.run_profile = "fast";
	value.edge_states = edges.map((edge) => ({
		edge_id: edge.id,
		flow: selected.has(edge.id) ? "1" : "0",
	}));
	value.residual_arcs = edges.flatMap((edge) => {
		const flow = selected.has(edge.id) ? "1" : "0";
		return [
			{
				edge_id: edge.id,
				direction: "forward",
				from: edge.from,
				to: edge.to,
				capacity: flow === "1" ? "0" : "1",
				cost: edge.cost,
				active: false,
			},
			{
				edge_id: edge.id,
				direction: "reverse",
				from: edge.to,
				to: edge.from,
				capacity: flow,
				cost: (-BigInt(edge.cost)).toString(),
				active: false,
			},
		];
	});
	value.node_trace_states = ["a0", "a1", "t0", "t1", "t2"].map((node_id) => ({
		node_id,
	}));
	value.outcome = {
		kind: "assignment",
		objective: "minimize",
		total_cost: "1",
		pairs: [
			{ edge_id: "e01", agent: "a0", task: "t1", cost: "1" },
			{ edge_id: "e12", agent: "a1", task: "t2", cost: "0" },
		],
		agent_labels: [
			{ node_id: "a0", label: "1" },
			{ node_id: "a1", label: "0" },
		],
		task_labels: [
			{ node_id: "t0", label: "0" },
			{ node_id: "t1", label: "0" },
			{ node_id: "t2", label: "0" },
		],
	};
	return value;
}

function transportationScene(): Record<string, unknown> {
	const value = validScene();
	value.solve_status = "optimal";
	value.model = {
		kind: "transportation",
		origins: ["o0"],
		destinations: ["d0"],
	};
	value.graph = {
		nodes: [
			{ id: "d0", supply: "-2" },
			{ id: "o0", supply: "2" },
		],
		edges: [
			{
				id: "route",
				from: "o0",
				to: "d0",
				lower: "0",
				capacity: "2",
				cost: "3",
			},
		],
	};
	value.algorithm = { id: "transportation-simplex", config: {} };
	value.run_profile = "fast";
	value.edge_states = [{ edge_id: "route", flow: "2" }];
	value.residual_arcs = [
		{
			edge_id: "route",
			direction: "forward",
			from: "o0",
			to: "d0",
			capacity: "0",
			cost: "3",
			active: false,
		},
		{
			edge_id: "route",
			direction: "reverse",
			from: "d0",
			to: "o0",
			capacity: "2",
			cost: "-3",
			active: false,
		},
	];
	value.node_trace_states = [{ node_id: "d0" }, { node_id: "o0" }];
	value.outcome = {
		kind: "min-cost-flow",
		total_cost: "6",
		potentials: [
			{ node_id: "d0", potential: "3" },
			{ node_id: "o0", potential: "0" },
		],
	};
	return value;
}

function transportationForestScene(): Record<string, unknown> {
	const value = transportationScene();
	value.model = {
		kind: "transportation",
		origins: ["o0", "o1"],
		destinations: ["d0", "d1"],
	};
	value.graph = {
		nodes: [
			{ id: "o0", supply: "1" },
			{ id: "o1", supply: "1" },
			{ id: "d0", supply: "-1" },
			{ id: "d1", supply: "-1" },
		],
		edges: [
			{ id: "r00", from: "o0", to: "d0", lower: "0", capacity: "1", cost: "0" },
			{ id: "r01", from: "o0", to: "d1", lower: "0", capacity: "1", cost: "0" },
			{ id: "r10", from: "o1", to: "d0", lower: "0", capacity: "1", cost: "0" },
			{ id: "r11", from: "o1", to: "d1", lower: "0", capacity: "1", cost: "0" },
		],
	};
	value.edge_states = [
		{ edge_id: "r00", flow: "1" },
		{ edge_id: "r01", flow: "0" },
		{ edge_id: "r10", flow: "0" },
		{ edge_id: "r11", flow: "1" },
	];
	value.residual_arcs = (
		value.graph as { edges: { id: string; from: string; to: string }[] }
	).edges.flatMap((edge) => {
		const flow = edge.id === "r00" || edge.id === "r11" ? "1" : "0";
		return [
			{
				edge_id: edge.id,
				direction: "forward",
				from: edge.from,
				to: edge.to,
				capacity: flow === "1" ? "0" : "1",
				cost: "0",
				active: false,
			},
			{
				edge_id: edge.id,
				direction: "reverse",
				from: edge.to,
				to: edge.from,
				capacity: flow,
				cost: "0",
				active: false,
			},
		];
	});
	value.node_trace_states = ["d0", "d1", "o0", "o1"].map((node_id) => ({
		node_id,
	}));
	value.outcome = {
		kind: "min-cost-flow",
		total_cost: "0",
		potentials: ["d0", "d1", "o0", "o1"].map((node_id) => ({
			node_id,
			potential: "0",
		})),
	};
	value.pseudoflow_forest = {
		arcs: [
			{ edge_id: "r00", direction: "forward" },
			{ edge_id: "r10", direction: "reverse" },
			{ edge_id: "r11", direction: "forward" },
		],
		strong_nodes: [],
	};
	return value;
}

function planarTriangleScene(): Record<string, unknown> {
	const value = validScene();
	const edges = [
		{ id: "ab", from: "a", to: "b", lower: "0", capacity: "5", cost: "0" },
		{ id: "ac", from: "a", to: "c", lower: "0", capacity: "2", cost: "0" },
		{ id: "bc", from: "b", to: "c", lower: "0", capacity: "3", cost: "0" },
	];
	value.model = {
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
	};
	value.graph = {
		nodes: ["a", "b", "c"].map((id) => ({ id, supply: "0" })),
		edges,
	};
	value.algorithm = { id: "hassin-st-planar", config: {} };
	value.edge_states = edges.map((edge) => ({ edge_id: edge.id, flow: "0" }));
	value.residual_arcs = edges.flatMap((edge) => [
		{
			edge_id: edge.id,
			direction: "forward",
			from: edge.from,
			to: edge.to,
			capacity: edge.capacity,
			cost: "0",
			active: false,
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
	]);
	value.node_trace_states = ["a", "b", "c"].map((node_id) => ({ node_id }));
	return value;
}

function nonPlanarK33Scene(): Record<string, unknown> {
	const value = validScene();
	const edges = Array.from({ length: 3 }, (_, row) =>
		Array.from({ length: 3 }, (_, column) => ({
			id: `e${row}${column}`,
			from: `a${row}`,
			to: `b${column}`,
			lower: "0",
			capacity: "1",
			cost: "0",
		})),
	).flat();
	value.model = {
		kind: "planar-max-flow",
		source: "a0",
		sink: "b0",
		embedding: {
			rotations: [
				...[0, 1, 2].map((row) => ({
					node_id: `a${row}`,
					darts: [0, 1, 2].map((column) => ({
						edge_id: `e${row}${column}`,
						direction: "forward",
					})),
				})),
				...[0, 1, 2].map((column) => ({
					node_id: `b${column}`,
					darts: [0, 1, 2].map((row) => ({
						edge_id: `e${row}${column}`,
						direction: "reverse",
					})),
				})),
			],
			outer_face: { edge_id: "e00", direction: "forward" },
		},
	};
	value.graph = {
		nodes: ["a0", "a1", "a2", "b0", "b1", "b2"].map((id) => ({
			id,
			supply: "0",
		})),
		edges,
	};
	value.edge_states = edges.map((edge) => ({ edge_id: edge.id, flow: "0" }));
	value.residual_arcs = edges.flatMap((edge) => [
		{
			edge_id: edge.id,
			direction: "forward",
			from: edge.from,
			to: edge.to,
			capacity: "1",
			cost: "0",
			active: false,
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
	]);
	value.node_trace_states = ["a0", "a1", "a2", "b0", "b1", "b2"].map(
		(node_id) => ({ node_id }),
	);
	return value;
}

function hallDeficientAssignmentScene(): Record<string, unknown> {
	const value = assignmentScene();
	const graph = value.graph as {
		nodes: { id: string; supply: string }[];
		edges: {
			id: string;
			from: string;
			to: string;
			lower: string;
			capacity: string;
			cost: string;
		}[];
	};
	graph.nodes = ["a0", "a1", "a2", "t0", "t1", "t2"].map((id) => ({
		id,
		supply: "0",
	}));
	graph.edges = [
		{ id: "e00", from: "a0", to: "t0", lower: "0", capacity: "1", cost: "1" },
		{ id: "e10", from: "a1", to: "t0", lower: "0", capacity: "1", cost: "2" },
		{ id: "e21", from: "a2", to: "t1", lower: "0", capacity: "1", cost: "0" },
		{ id: "e22", from: "a2", to: "t2", lower: "0", capacity: "1", cost: "0" },
	];
	value.model = {
		kind: "assignment",
		agents: ["a0", "a1", "a2"],
		tasks: ["t0", "t1", "t2"],
		objective: "minimize",
	};
	value.solve_status = "infeasible";
	value.edge_states = graph.edges.map((edge) => ({
		edge_id: edge.id,
		flow: "0",
	}));
	value.residual_arcs = graph.edges.flatMap((edge) => [
		{
			edge_id: edge.id,
			direction: "forward",
			from: edge.from,
			to: edge.to,
			capacity: "1",
			cost: edge.cost,
			active: false,
		},
		{
			edge_id: edge.id,
			direction: "reverse",
			from: edge.to,
			to: edge.from,
			capacity: "0",
			cost: (-BigInt(edge.cost)).toString(),
			active: false,
		},
	]);
	value.node_trace_states = graph.nodes.map((node) => ({ node_id: node.id }));
	value.outcome = {
		kind: "assignment-infeasible",
		deficiency: "1",
		hall_agents: ["a0", "a1"],
		neighbor_tasks: ["t0"],
	};
	return value;
}

function tardosFrameworkScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "4";
	value.event_count = "4";
	value.solve_status = "primitive-complete";
	value.model = { kind: "transshipment" };
	value.algorithm = {
		id: "tardos-framework",
		config: { potentials: { a: "0", s: "0", t: "0" } },
	};
	value.graph = {
		nodes: [
			{ id: "a", supply: "0" },
			{ id: "s", supply: "2" },
			{ id: "t", supply: "-2" },
		],
		edges: [
			{
				id: "cheap-1",
				from: "s",
				to: "a",
				lower: "0",
				capacity: "2",
				cost: "1",
			},
			{
				id: "cheap-2",
				from: "a",
				to: "t",
				lower: "0",
				capacity: "2",
				cost: "1",
			},
			{
				id: "expensive",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "2",
				cost: "20",
			},
		],
	};
	value.edge_states = [
		{ edge_id: "cheap-1", flow: "2" },
		{ edge_id: "cheap-2", flow: "2" },
		{ edge_id: "expensive", flow: "0" },
	];
	value.residual_arcs = [
		{
			edge_id: "cheap-1",
			direction: "forward",
			from: "s",
			to: "a",
			capacity: "0",
			cost: "1",
			active: false,
		},
		{
			edge_id: "cheap-1",
			direction: "reverse",
			from: "a",
			to: "s",
			capacity: "2",
			cost: "-1",
			active: false,
		},
		{
			edge_id: "cheap-2",
			direction: "forward",
			from: "a",
			to: "t",
			capacity: "0",
			cost: "1",
			active: false,
		},
		{
			edge_id: "cheap-2",
			direction: "reverse",
			from: "t",
			to: "a",
			capacity: "2",
			cost: "-1",
			active: false,
		},
		{
			edge_id: "expensive",
			direction: "forward",
			from: "s",
			to: "t",
			capacity: "2",
			cost: "20",
			active: false,
		},
		{
			edge_id: "expensive",
			direction: "reverse",
			from: "t",
			to: "s",
			capacity: "0",
			cost: "-20",
			active: false,
		},
	];
	value.node_trace_states = [
		{ node_id: "a", label: "0" },
		{ node_id: "s", label: "0" },
		{ node_id: "t", label: "0" },
	];
	value.trace_event = {
		event_id: "4",
		catalog_id: "tardos-framework.complete-primitive",
		minimum_granularity: "phase",
		pseudocode_line: "tardos-framework:return-fixed-variables",
		patch_count: 1,
		entity_refs: [{ kind: "edge", edge_id: "expensive" }],
	};
	const fixed = {
		edge_id: "expensive",
		bound: "lower",
		value: "0",
		direction: "forward",
		reduced_cost: "20",
	};
	value.tardos_framework_overlay = {
		stage: "complete",
		epsilon: "1",
		threshold: "3",
		determinant_bound: "1",
		nodes: [
			{ node_id: "a", potential: "0" },
			{ node_id: "s", potential: "0" },
			{ node_id: "t", potential: "0" },
		],
		residual_arcs: [
			{
				edge_id: "cheap-1",
				direction: "reverse",
				capacity: "2",
				reduced_cost: "-1",
				fixes_variable: false,
			},
			{
				edge_id: "cheap-2",
				direction: "reverse",
				capacity: "2",
				reduced_cost: "-1",
				fixes_variable: false,
			},
			{
				edge_id: "expensive",
				direction: "forward",
				capacity: "2",
				reduced_cost: "20",
				fixes_variable: true,
			},
		],
		fixed_variables: [fixed],
	};
	value.outcome = {
		kind: "tardos-framework",
		epsilon: "1",
		threshold: "3",
		determinant_bound: "1",
		fixed_variables: [fixed],
	};
	return value;
}

function electricalFlowScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "6";
	value.event_count = "6";
	value.solve_status = "primitive-complete";
	value.algorithm = { id: "electrical-flow", config: {} };
	value.graph = {
		nodes: [
			{ id: "s", supply: "0" },
			{ id: "t", supply: "0" },
		],
		edges: [
			{
				id: "st",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "2",
				cost: "0",
			},
		],
	};
	value.edge_states = [{ edge_id: "st", flow: "0" }];
	value.residual_arcs = [];
	value.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
	value.trace_event = {
		event_id: "6",
		parent_phase_id: "1",
		catalog_id: "electrical-flow.complete-primitive",
		minimum_granularity: "phase",
		pseudocode_line: "publish minimum-energy primitive certificate",
		patch_count: 1,
		entity_refs: [{ kind: "edge", edge_id: "st" }],
		detail: { label: "maximum absolute error", value: "0" },
	};
	value.electrical_flow_overlay = {
		stage: "complete",
		target_current: "1",
		relative_tolerance: "0.0000000001",
		iteration: "1",
		residual_l2: "0",
		effective_resistance: "0.25",
		total_energy: "0.25",
		exact_effective_resistance: { numerator: "1", denominator: "4" },
		maximum_absolute_error: "0",
		converged: true,
		nodes: [
			{
				node_id: "s",
				potential: "0.25",
				residual: "0",
				search_direction: "0",
				grounded: false,
			},
			{
				node_id: "t",
				potential: "0",
				residual: "0",
				search_direction: "0",
				grounded: true,
			},
		],
		edges: [
			{
				edge_id: "st",
				resistance: { numerator: "1", denominator: "4" },
				conductance: "4",
				voltage_drop: "0.25",
				current: "1",
				congestion: "0.5",
				energy: "0.25",
			},
		],
	};
	value.outcome = {
		kind: "electrical-flow",
		effective_resistance: "0.25",
		exact_effective_resistance: { numerator: "1", denominator: "4" },
		total_energy: "0.25",
		residual_l2: "0",
		maximum_absolute_error: "0",
		iterations: "1",
	};
	return value;
}

function augmentingElectricalScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "15";
	value.event_count = "15";
	value.solve_status = "optimal";
	value.algorithm = { id: "augmenting-electrical-flow", config: {} };
	value.graph = {
		nodes: [
			{ id: "s", supply: "0" },
			{ id: "t", supply: "0" },
		],
		edges: [
			{
				id: "st",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "2",
				cost: "0",
			},
		],
	};
	value.edge_states = [{ edge_id: "st", flow: "2" }];
	value.residual_arcs = [
		{
			edge_id: "st",
			direction: "forward",
			from: "s",
			to: "t",
			capacity: "0",
			cost: "0",
			active: false,
		},
		{
			edge_id: "st",
			direction: "reverse",
			from: "t",
			to: "s",
			capacity: "2",
			cost: "0",
			active: false,
		},
	];
	value.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
	value.trace_event = {
		event_id: "15",
		parent_phase_id: "1",
		catalog_id: "augmenting-electrical-flow.optimal",
		minimum_granularity: "phase",
		pseudocode_line: "publish the certified directed maximum flow",
		patch_count: 1,
		entity_refs: [{ kind: "edge", edge_id: "st" }],
		detail: { label: "original max flow", value: "2" },
	};
	value.augmenting_electrical_overlay = {
		stage: "optimal",
		original_target: "2",
		transformed_target: "6",
		working_target: "18",
		current_value: "18",
		alpha: "1",
		remaining: "0",
		electrical_energy: "0",
		congestion_l3: "0",
		congestion_l4: "0",
		coupling_l2: "0",
		working_nodes: "2",
		working_edges: "3",
		active_working_path: [],
		active_extraction_cycle: [],
		nodes: [
			{
				node_id: "s",
				potential: "0",
				coupling_violation: "0",
				target_source_side: true,
			},
			{
				node_id: "t",
				potential: "0",
				coupling_violation: "0",
				target_source_side: false,
			},
		],
		edges: [
			{
				edge_id: "st",
				central_flow: "0",
				electrical_current: "0",
				forward_residual: "0",
				backward_residual: "2",
				congestion: "0",
				resistance: "0",
				boost_segments: "1",
				rounded_central_flow: "0",
				extraction_central_scaled: "4",
				extraction_toward_source: "0",
				extraction_out_of_sink: "0",
				final_flow: "2",
			},
		],
	};
	value.outcome = {
		kind: "max-flow",
		value: "2",
		cut_bound: "2",
		source_side: ["s"],
	};
	return value;
}

function interiorPointMaxFlowScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "12";
	value.event_count = "12";
	value.solve_status = "optimal";
	value.algorithm = { id: "interior-point-max-flow", config: {} };
	value.graph = {
		nodes: [
			{ id: "s", supply: "0" },
			{ id: "t", supply: "0" },
		],
		edges: [
			{
				id: "st",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "1",
				cost: "0",
			},
		],
	};
	value.edge_states = [{ edge_id: "st", flow: "1" }];
	value.residual_arcs = [
		{
			edge_id: "st",
			direction: "forward",
			from: "s",
			to: "t",
			capacity: "0",
			cost: "0",
			active: false,
		},
		{
			edge_id: "st",
			direction: "reverse",
			from: "t",
			to: "s",
			capacity: "1",
			cost: "0",
			active: false,
		},
	];
	value.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
	value.trace_event = {
		event_id: "12",
		parent_phase_id: "1",
		catalog_id: "interior-point-max-flow.optimal",
		minimum_granularity: "phase",
		pseudocode_line: "publish the certified unit-capacity maximum flow",
		patch_count: 1,
		entity_refs: [{ kind: "edge", edge_id: "st" }],
		detail: { label: "target flow", value: "1" },
	};
	value.interior_point_max_flow_overlay = {
		stage: "optimal",
		target_value: "1",
		mu: "0.01",
		duality_gap: "0.1",
		centrality: "0",
		congestion_l4: "0",
		step_size: "0",
		electrical_energy: "0",
		b_matching_nodes: "4",
		b_matching_edges: "3",
		working_nodes: "5",
		working_edges: "9",
		nodes: [
			{ node_id: "s", potential: "0", target_source_side: true },
			{ node_id: "t", potential: "0", target_source_side: false },
		],
		edges: [
			{
				edge_id: "st",
				fractional_flow: "0.99",
				electrical_current: "0",
				slack: "0.1",
				measure: "1",
				resistance: "0.10101010101010101",
				congestion: "0",
				normalized_away: false,
				final_flow: "1",
			},
		],
	};
	value.outcome = {
		kind: "max-flow",
		value: "1",
		cut_bound: "1",
		source_side: ["s"],
	};
	return value;
}

function minimumRatioCycleScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "7";
	value.event_count = "7";
	value.solve_status = "primitive-complete";
	value.algorithm = { id: "minimum-ratio-cycle-max-flow", config: {} };
	value.graph = {
		nodes: [
			{ id: "s", supply: "0" },
			{ id: "t", supply: "0" },
		],
		edges: [
			{
				id: "a",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "2",
				cost: "7",
			},
			{
				id: "b",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "3",
				cost: "-3",
			},
		],
	};
	value.edge_states = [
		{ edge_id: "a", flow: "0" },
		{ edge_id: "b", flow: "0" },
	];
	value.residual_arcs = [];
	value.node_trace_states = [
		{ node_id: "s", label: "0", remaining_divergence: "0" },
		{ node_id: "t", label: "0", remaining_divergence: "0" },
	];
	value.trace_event = {
		event_id: "7",
		parent_phase_id: "2",
		catalog_id: "minimum-ratio-cycle-max-flow.complete-primitive",
		minimum_granularity: "phase",
		pseudocode_line:
			"publish the primitive certificate without a max-flow claim",
		patch_count: 1,
		entity_refs: [{ kind: "edge", edge_id: "a" }],
		detail: { label: "simple cycles", value: "1" },
	};
	value.minimum_ratio_cycle_overlay = {
		stage: "complete",
		best_ratio: { numerator: "-2", denominator: "1" },
		selected_edge_count: "2",
		maximum_absolute_balance: "0",
		enumerated_vectors: "8",
		simple_cycles: "1",
		fundamental_cycles: "1",
		nodes: [
			{
				node_id: "s",
				component: "0",
				depth: "0",
				candidate_balance: "0",
				on_candidate: false,
				on_selected: true,
			},
			{
				node_id: "t",
				component: "0",
				parent_node_id: "s",
				depth: "1",
				candidate_balance: "0",
				on_candidate: false,
				on_selected: true,
			},
		],
		edges: [
			{
				edge_id: "a",
				gradient: "7",
				length: "2",
				tree_edge: true,
				candidate_sign: "0",
				selected_sign: "-1",
				numerator_contribution: "0",
				denominator_contribution: "0",
			},
			{
				edge_id: "b",
				gradient: "-3",
				length: "3",
				tree_edge: false,
				candidate_sign: "0",
				selected_sign: "1",
				numerator_contribution: "0",
				denominator_contribution: "0",
			},
		],
	};
	value.outcome = {
		kind: "minimum-ratio-cycle",
		ratio: { numerator: "-2", denominator: "1" },
		cycle: [
			{ edge_id: "a", sign: "-1" },
			{ edge_id: "b", sign: "1" },
		],
		simple_cycles: "1",
		enumerated_vectors: "8",
	};
	return value;
}

function minimumRatioCycleMcfScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "13";
	value.event_count = "13";
	value.solve_status = "primitive-complete";
	value.model = {
		kind: "fixed-flow-min-cost",
		source: "s",
		sink: "t",
		required_flow: "1",
	};
	value.algorithm = { id: "minimum-ratio-cycle-mcf", config: {} };
	value.graph = {
		nodes: [
			{ id: "s", supply: "0" },
			{ id: "t", supply: "0" },
		],
		edges: [
			{
				id: "cheap",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "2",
				cost: "1",
			},
			{
				id: "dear",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "2",
				cost: "4",
			},
		],
	};
	value.edge_states = [
		{ edge_id: "cheap", flow: "0" },
		{ edge_id: "dear", flow: "0" },
	];
	value.residual_arcs = [];
	value.node_trace_states = [
		{ node_id: "s", label: "0", remaining_divergence: "0" },
		{ node_id: "t", label: "0", remaining_divergence: "0" },
	];
	value.trace_event = {
		event_id: "13",
		parent_phase_id: "5",
		catalog_id: "minimum-ratio-cycle-mcf.complete-primitive",
		minimum_granularity: "phase",
		pseudocode_line:
			"publish one checked progress primitive without a terminal MCF claim",
		patch_count: 1,
		entity_refs: [{ kind: "edge", edge_id: "cheap" }],
		detail: { label: "source steps", value: "1" },
	};
	value.minimum_ratio_cycle_mcf_overlay = {
		stage: "complete",
		alpha: "0.0007213475204444818",
		optimum_cost: "1",
		initial_cost: "2.5",
		current_cost: "2.499264925",
		cost_gap: "1.4992649249999999",
		potential_before: "20.21901969740447",
		current_potential: "20.199412893047402",
		best_ratio: "-14.995471605326076",
		kappa: "0.99",
		eta: "0.000245025",
		weighted_step_norm: "0.0013071946328808881",
		potential_decrease: "0.01960680435706763",
		guaranteed_decrease: "0.0019602",
		stationary: false,
		selected_edge_count: "2",
		maximum_absolute_balance: "0",
		feasible_flows: "2",
		enumerated_vectors: "8",
		simple_cycles: "1",
		fundamental_cycles: "1",
		nodes: [
			{
				node_id: "s",
				component: "0",
				depth: "0",
				candidate_balance: "0",
				on_candidate: false,
				on_selected: true,
			},
			{
				node_id: "t",
				component: "0",
				parent_node_id: "s",
				depth: "1",
				candidate_balance: "0",
				on_candidate: false,
				on_selected: true,
			},
		],
		edges: [
			{
				edge_id: "cheap",
				fixed_on_face: false,
				initial_flow: "0.5",
				updated_flow: "0.500245025",
				lower_slack: "0.5",
				upper_slack: "1.5",
				gradient: "26.665704007811673",
				length: "2.6674719577204122",
				tree_edge: true,
				candidate_sign: "0",
				selected_sign: "1",
				numerator_contribution: "0",
				denominator_contribution: "0",
			},
			{
				edge_id: "dear",
				fixed_on_face: false,
				initial_flow: "0.5",
				updated_flow: "0.499754975",
				lower_slack: "0.5",
				upper_slack: "1.5",
				gradient: "106.66570400781167",
				length: "2.6674719577204122",
				tree_edge: false,
				candidate_sign: "0",
				selected_sign: "-1",
				numerator_contribution: "0",
				denominator_contribution: "0",
			},
		],
	};
	value.outcome = {
		kind: "minimum-ratio-cycle-mcf",
		ratio: "-14.995471605326076",
		cycle: [
			{ edge_id: "cheap", sign: "1" },
			{ edge_id: "dear", sign: "-1" },
		],
		alpha: "0.0007213475204444818",
		kappa: "0.99",
		eta: "0.000245025",
		potential_decrease: "0.01960680435706763",
		guaranteed_decrease: "0.0019602",
		stationary: false,
	};
	return value;
}

function randomizedAlmostLinearScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "12";
	value.event_count = "12";
	value.solve_status = "optimal";
	value.algorithm = {
		id: "randomized-almost-linear-max-flow-oracle-demonstrator",
		config: {},
	};
	value.graph = {
		nodes: [
			{ id: "s", supply: "0" },
			{ id: "t", supply: "0" },
		],
		edges: [
			{
				id: "st",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "3",
				cost: "0",
			},
		],
	};
	value.edge_states = [{ edge_id: "st", flow: "3" }];
	value.residual_arcs = [
		{
			edge_id: "st",
			direction: "forward",
			from: "s",
			to: "t",
			capacity: "0",
			cost: "0",
			active: false,
		},
		{
			edge_id: "st",
			direction: "reverse",
			from: "t",
			to: "s",
			capacity: "3",
			cost: "0",
			active: false,
		},
	];
	value.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
	value.trace_event = {
		event_id: "12",
		parent_phase_id: "3",
		catalog_id: "randomized-almost-linear-max-flow-oracle-demonstrator.optimal",
		minimum_granularity: "phase",
		pseudocode_line:
			"publish certified maximum flow without claiming almost-linear runtime",
		patch_count: 1,
		entity_refs: [{ kind: "edge", edge_id: "st" }],
		detail: { label: "maximum flow", value: "3" },
	};
	value.randomized_almost_linear_overlay = {
		stage: "optimal",
		seed: "4849066491903931220",
		random_draws: "6",
		alpha: "0.1",
		potential: "12.5",
		cost_gap: "0.25",
		selected_ratio: "-0.5",
		exact_pool_ratio: "-0.5",
		miss_probability: { numerator: "0", denominator: "1" },
		forest_pool_size: "2",
		sample_count: "4",
		iteration: "2",
		rebuild_epoch: "1",
		return_flow: "1.5",
		return_capacity: "3",
		return_gradient: "-1",
		return_length: "2",
		return_tree_memberships: "1",
		active_return_tree_edge: true,
		active_return_sign: "0",
		return_isolation_draw: "1",
		final_point_return_flow: "3",
		final_return_flow: "3",
		artificial_edges: "0",
		artificial_flow: "0",
		final_artificial_flow: "0",
		isolation_scale: "144",
		isolation_attempt: "1",
		isolation_failure_probability: { numerator: "1", denominator: "2" },
		isolated_objective: "-426",
		final_point_threshold: "0.00038580246913580245",
		final_point_gap: "0",
		final_point_mix: "0",
		target_value: "3",
		nodes: [
			{
				node_id: "s",
				tree_component: "0",
				source_side: true,
				artificial_direction: "0",
				artificial_flow: "0",
				artificial_capacity: "0",
				artificial_tree_memberships: "0",
				active_artificial_tree_edge: false,
				active_artificial_sign: "0",
			},
			{
				node_id: "t",
				tree_parent_node_id: "s",
				tree_component: "0",
				source_side: false,
				artificial_direction: "0",
				artificial_flow: "0",
				artificial_capacity: "0",
				artificial_tree_memberships: "0",
				active_artificial_tree_edge: false,
				active_artificial_sign: "0",
			},
		],
		edges: [
			{
				edge_id: "st",
				interior_flow: "1.5",
				gradient: "1",
				length: "2",
				sampled_tree_memberships: "3",
				active_tree_edge: false,
				active_cycle_sign: "0",
				changed_coordinate: false,
				isolation_draw: "1",
				final_point_flow: "3",
				final_flow: "3",
			},
		],
	};
	value.outcome = {
		kind: "max-flow",
		value: "3",
		cut_bound: "3",
		source_side: ["s"],
	};
	return value;
}

function weightedAugmentingPathsScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "14";
	value.event_count = "14";
	value.solve_status = "optimal";
	value.algorithm = { id: "weighted-augmenting-paths", config: {} };
	value.graph = {
		nodes: [
			{ id: "s", supply: "0" },
			{ id: "t", supply: "0" },
		],
		edges: [
			{
				id: "st",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "2",
				cost: "0",
			},
		],
	};
	value.edge_states = [{ edge_id: "st", flow: "2" }];
	value.residual_arcs = [
		{
			edge_id: "st",
			direction: "forward",
			from: "s",
			to: "t",
			capacity: "0",
			cost: "0",
			active: false,
		},
		{
			edge_id: "st",
			direction: "reverse",
			from: "t",
			to: "s",
			capacity: "2",
			cost: "0",
			active: false,
		},
	];
	value.node_trace_states = ["s", "t"].map((node_id) => ({
		node_id,
		label: "0",
		remaining_divergence: "0",
	}));
	value.trace_event = {
		event_id: "14",
		catalog_id: "weighted-augmenting-paths.optimal",
		minimum_granularity: "operation",
		pseudocode_line: "independently certify the final maximum flow and cut",
		patch_count: 1,
		entity_refs: [{ kind: "edge", edge_id: "st" }],
		detail: { label: "maximum flow", value: "2" },
	};
	value.weighted_augmenting_paths_overlay = {
		stage: "optimal",
		phase: "1",
		phase_count: "2",
		capacity_bit: "0",
		round: "0",
		height: "0",
		phi_numerator: "0",
		phi_denominator: "1",
		active_bottleneck: "0",
		hierarchy_cuts: "0",
		relabel_jumps: "1",
		augmentations: "1",
		augmented_units: "1",
		nodes: [
			{
				node_id: "s",
				component: "0",
				order: "0",
				label: "0",
				alive: true,
				expansion_witness_side: false,
				source_side: true,
			},
			{
				node_id: "t",
				component: "0",
				order: "0",
				label: "0",
				alive: true,
				expansion_witness_side: false,
				source_side: false,
			},
		],
		edges: [{ edge_id: "st", scaled_capacity: "2", flow: "2" }],
		residual_arcs: [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "0",
				weight: "0",
				admissible: false,
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "2",
				weight: "0",
				admissible: false,
				active: false,
			},
		],
		active_path: [],
	};
	value.outcome = {
		kind: "max-flow",
		value: "2",
		cut_bound: "2",
		source_side: ["s"],
	};
	return value;
}

function weightedPushRelabelShortcutScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "23";
	value.event_count = "23";
	value.solve_status = "optimal";
	value.model = { kind: "max-flow", source: "s", sink: "t" };
	value.algorithm = { id: "weighted-push-relabel", config: {} };
	value.graph = {
		nodes: ["a", "b", "s", "t"].map((id) => ({ id, supply: "0" })),
		edges: [
			{ id: "ab", from: "a", to: "b", lower: "0", capacity: "7", cost: "0" },
			{ id: "at", from: "a", to: "t", lower: "0", capacity: "4", cost: "0" },
			{ id: "ba", from: "b", to: "a", lower: "0", capacity: "7", cost: "0" },
			{ id: "bt", from: "b", to: "t", lower: "0", capacity: "6", cost: "0" },
			{ id: "sa", from: "s", to: "a", lower: "0", capacity: "7", cost: "0" },
			{ id: "sb", from: "s", to: "b", lower: "0", capacity: "4", cost: "0" },
		],
	};
	const originalEdges = [
		{
			edge_id: "ab",
			from: "a",
			to: "b",
			capacity: "7",
			flow: "2",
			weight: "1",
		},
		{
			edge_id: "at",
			from: "a",
			to: "t",
			capacity: "4",
			flow: "4",
			weight: "2",
		},
		{
			edge_id: "ba",
			from: "b",
			to: "a",
			capacity: "7",
			flow: "0",
			weight: "1",
		},
		{
			edge_id: "bt",
			from: "b",
			to: "t",
			capacity: "6",
			flow: "6",
			weight: "1",
		},
		{
			edge_id: "sa",
			from: "s",
			to: "a",
			capacity: "7",
			flow: "6",
			weight: "1",
		},
		{
			edge_id: "sb",
			from: "s",
			to: "b",
			capacity: "4",
			flow: "4",
			weight: "2",
		},
	].map((edge) => ({ ...edge, kind: "original" as const }));
	const shortcutEdges = [
		{ edge_id: "shortcut-edge:1:a:shortcut:1", from: "a", to: "shortcut:1" },
		{ edge_id: "shortcut-edge:1:shortcut:1:a", from: "shortcut:1", to: "a" },
		{ edge_id: "shortcut-edge:1:b:shortcut:1", from: "b", to: "shortcut:1" },
		{ edge_id: "shortcut-edge:1:shortcut:1:b", from: "shortcut:1", to: "b" },
	].map((edge) => ({
		...edge,
		kind: "shortcut" as const,
		capacity: "7",
		flow: "0",
		weight: "2",
		shortcut_component: "1",
	}));
	const augmentedEdges = [...originalEdges, ...shortcutEdges];
	value.edge_states = originalEdges.map(({ edge_id, flow }) => ({
		edge_id,
		flow,
	}));
	value.residual_arcs = originalEdges.flatMap((edge) => [
		{
			edge_id: edge.edge_id,
			direction: "forward",
			from: edge.from,
			to: edge.to,
			capacity: (BigInt(edge.capacity) - BigInt(edge.flow)).toString(),
			cost: "0",
			active: false,
		},
		{
			edge_id: edge.edge_id,
			direction: "reverse",
			from: edge.to,
			to: edge.from,
			capacity: edge.flow,
			cost: "0",
			active: false,
		},
	]);
	value.node_trace_states = ["a", "b", "s", "t"].map((node_id) => ({
		node_id,
		label: "0",
		remaining_divergence: "0",
	}));
	value.trace_event = {
		event_id: "23",
		catalog_id: "weighted-push-relabel.optimal",
		minimum_granularity: "operation",
		pseudocode_line: "independently certify the repaired maximum flow and cut",
		patch_count: 1,
		entity_refs: [{ kind: "node", node_id: "s" }],
		detail: { label: "maximum flow", value: "10" },
	};
	value.weighted_push_relabel_shortcut_overlay = {
		stage: "optimal",
		hierarchy_levels: "1",
		psi_numerator: "1",
		psi_denominator: "1",
		height: "4096",
		demand: "448",
		routed: "10",
		weighted_length: "18",
		weighted_length_units: "10",
		sparse_cut_level: "2",
		sparse_cut_capacity: "10",
		active_bottleneck: "0",
		relabel_steps: "18",
		augmentations: "3",
		shortcut_traversals: "2",
		residual_rounds: "2",
		completion_relabel_steps: "24",
		completion_augmentations: "3",
		nodes: [
			{
				node_id: "a",
				original: true,
				component: "1",
				order: "2",
				label: "0",
				alive: true,
				sparse_cut_side: true,
				source_side: true,
			},
			{
				node_id: "b",
				original: true,
				component: "1",
				order: "3",
				label: "0",
				alive: true,
				sparse_cut_side: true,
				source_side: true,
			},
			{
				node_id: "s",
				original: true,
				component: "0",
				order: "1",
				label: "0",
				alive: true,
				sparse_cut_side: true,
				source_side: true,
			},
			{
				node_id: "t",
				original: true,
				component: "2",
				order: "4",
				label: "0",
				alive: true,
				sparse_cut_side: false,
				source_side: false,
			},
			{
				node_id: "shortcut:1",
				original: false,
				component: "1",
				order: "0",
				label: "0",
				alive: true,
				sparse_cut_side: true,
				source_side: false,
			},
		],
		edges: augmentedEdges,
		residual_arcs: augmentedEdges.flatMap((edge) => [
			{
				edge_id: edge.edge_id,
				direction: "forward",
				from: edge.from,
				to: edge.to,
				capacity: (BigInt(edge.capacity) - BigInt(edge.flow)).toString(),
				weight: edge.weight,
				admissible: false,
				active: false,
			},
			{
				edge_id: edge.edge_id,
				direction: "reverse",
				from: edge.to,
				to: edge.from,
				capacity: edge.flow,
				weight: edge.weight,
				admissible: false,
				active: false,
			},
		]),
		active_path: [],
		inspected_arcs: [],
		active_relabel_nodes: [],
	};
	value.outcome = {
		kind: "max-flow",
		value: "10",
		cut_bound: "10",
		source_side: ["a", "b", "s"],
	};
	return value;
}

function deterministicAlmostLinearScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "18";
	value.event_count = "18";
	value.solve_status = "optimal";
	value.algorithm = {
		id: "deterministic-almost-linear-max-flow-oracle-demonstrator",
		config: {},
	};
	value.graph = {
		nodes: [
			{ id: "s", supply: "0" },
			{ id: "t", supply: "0" },
		],
		edges: [
			{
				id: "st",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "3",
				cost: "0",
			},
		],
	};
	value.edge_states = [{ edge_id: "st", flow: "3" }];
	value.residual_arcs = [
		{
			edge_id: "st",
			direction: "forward",
			from: "s",
			to: "t",
			capacity: "0",
			cost: "0",
			active: false,
		},
		{
			edge_id: "st",
			direction: "reverse",
			from: "t",
			to: "s",
			capacity: "3",
			cost: "0",
			active: false,
		},
	];
	value.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
	value.trace_event = {
		event_id: "18",
		parent_phase_id: "17",
		catalog_id:
			"deterministic-almost-linear-max-flow-oracle-demonstrator.optimal",
		minimum_granularity: "phase",
		pseudocode_line:
			"publish certified maximum flow without claiming the paper runtime",
		patch_count: 1,
		entity_refs: [{ kind: "edge", edge_id: "st" }],
		detail: { label: "maximum flow", value: "3" },
	};
	value.deterministic_almost_linear_overlay = {
		stage: "optimal",
		alpha: "0.1",
		potential: "12.5",
		cost_gap: "0.25",
		selected_ratio: "-0.5",
		exact_pool_ratio: "-0.5",
		selected_off_tree_edge: "0",
		selected_cycle_kind: "tree",
		forest_pool_size: "2",
		level_count: "2",
		branch_count: "3",
		built_branch_records: "6",
		active_branches: ["0", "0"],
		passes: ["0", "0"],
		active_level: "0",
		fundamental_cycles: "1",
		core_vertices: "2",
		core_edges: "1",
		spanner_edges: "1",
		embedding_hops: "0",
		iteration: "2",
		rebuild_epoch: "1",
		return_flow: "1.5",
		return_capacity: "3",
		return_gradient: "-1",
		return_length: "2",
		return_tree_level_mask: "1",
		active_return_tree_edge: true,
		active_return_sign: "0",
		final_point_return_flow: { numerator: "3", denominator: "1" },
		rounding_return_flow: { numerator: "3", denominator: "1" },
		rounding_return_forest_edge: false,
		rounding_return_sign: "0",
		final_return_flow: "3",
		artificial_edges: "0",
		artificial_flow: "0",
		final_artificial_flow: "0",
		final_point_gap: { numerator: "0", denominator: "1" },
		final_point_threshold: { numerator: "1", denominator: "2" },
		final_point_mix: { numerator: "1", denominator: "4" },
		target_value: "3",
		nodes: [
			{
				node_id: "s",
				forest_component: "0",
				source_side: true,
				artificial_direction: "0",
				artificial_flow: "0",
				artificial_capacity: "0",
				artificial_tree_level_mask: "0",
				active_artificial_tree_edge: false,
				active_artificial_sign: "0",
			},
			{
				node_id: "t",
				tree_parent_node_id: "s",
				forest_component: "0",
				source_side: false,
				artificial_direction: "0",
				artificial_flow: "0",
				artificial_capacity: "0",
				artificial_tree_level_mask: "0",
				active_artificial_tree_edge: false,
				active_artificial_sign: "0",
			},
		],
		edges: [
			{
				edge_id: "st",
				interior_flow: "1.5",
				gradient: "1",
				length: "2",
				tree_level_mask: "1",
				forest_level_mask: "1",
				active_tree_edge: true,
				active_core_edge: true,
				active_spanner_edge: true,
				embedding_hops: "0",
				embedding_stretch: "0",
				active_cycle_sign: "0",
				changed_coordinate: false,
				final_point_flow: { numerator: "3", denominator: "1" },
				rounding_flow: { numerator: "3", denominator: "1" },
				rounding_forest_edge: false,
				rounding_cycle_sign: "0",
				final_flow: "3",
			},
		],
	};
	value.outcome = {
		kind: "max-flow",
		value: "3",
		cut_bound: "3",
		source_side: ["s"],
	};
	return value;
}

function primalDualIpmMcfScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "16";
	value.event_count = "16";
	value.solve_status = "optimal";
	value.model = { kind: "transshipment" };
	value.algorithm = { id: "primal-dual-interior-point-mcf", config: {} };
	value.graph = {
		nodes: [
			{ id: "s", supply: "2" },
			{ id: "t", supply: "-2" },
		],
		edges: [
			{
				id: "st",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "2",
				cost: "1",
			},
		],
	};
	value.edge_states = [{ edge_id: "st", flow: "2" }];
	value.residual_arcs = [
		{
			edge_id: "st",
			direction: "forward",
			from: "s",
			to: "t",
			capacity: "0",
			cost: "1",
			active: false,
		},
		{
			edge_id: "st",
			direction: "reverse",
			from: "t",
			to: "s",
			capacity: "2",
			cost: "-1",
			active: false,
		},
	];
	value.node_trace_states = [
		{ node_id: "s", label: "0", remaining_divergence: "0" },
		{ node_id: "t", label: "1", remaining_divergence: "0" },
	];
	value.trace_event = {
		event_id: "16",
		catalog_id: "primal-dual-interior-point-mcf.optimal",
		minimum_granularity: "phase",
		pseudocode_line: "integer-ipm:publish-certified-optimum",
		patch_count: 1,
		entity_refs: [{ kind: "edge", edge_id: "st" }],
		detail: { label: "certificate checks", value: "1" },
	};
	value.primal_dual_ipm_mcf_overlay = {
		stage: "optimal",
		seed: "7640891576956012809",
		mu: "1",
		beta: "256",
		gamma: "32768",
		proxy_gap: "0",
		centrality_numerator: "0",
		cycle_alpha: "0",
		nodes: [
			{
				auxiliary_id: "node:s",
				kind: "original",
				original_node_id: "s",
				potential: "0",
				component: "0",
				in_crossover_set: true,
			},
			{
				auxiliary_id: "node:t",
				kind: "original",
				original_node_id: "t",
				potential: "1",
				component: "1",
				in_crossover_set: true,
			},
			{
				auxiliary_id: "capacity:st",
				kind: "capacity",
				original_edge_id: "st",
				potential: "1",
				component: "2",
				in_crossover_set: true,
			},
		],
		arcs: [
			{
				auxiliary_id: "aux:0",
				original_edge_id: "st",
				from: "node:s",
				to: "capacity:st",
				kind: "upper",
				flow: "1",
				slack: "0",
				deleted: true,
				contracted: false,
				in_minor: false,
				in_tree: false,
				forest_candidate: false,
				active_cycle_sign: "0",
			},
			{
				auxiliary_id: "aux:1",
				original_edge_id: "st",
				from: "node:t",
				to: "capacity:st",
				kind: "lower",
				flow: "1",
				slack: "0",
				deleted: false,
				contracted: true,
				in_minor: false,
				in_tree: true,
				forest_candidate: false,
				active_cycle_sign: "0",
			},
			{
				auxiliary_id: "aux:2",
				original_edge_id: "st",
				from: "node:t",
				to: "node:s",
				kind: "artificial",
				flow: "1",
				slack: "1",
				deleted: true,
				contracted: false,
				in_minor: false,
				in_tree: false,
				forest_candidate: false,
				active_cycle_sign: "0",
			},
		],
	};
	value.outcome = {
		kind: "min-cost-flow",
		total_cost: "2",
		potentials: [
			{ node_id: "s", potential: "0" },
			{ node_id: "t", potential: "1" },
		],
	};
	return value;
}

function electricalIpmMcfScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "15";
	value.event_count = "15";
	value.solve_status = "optimal";
	value.model = { kind: "transshipment" };
	value.algorithm = {
		id: "electrical-flow-interior-point-mcf",
		config: {},
	};
	value.graph = {
		nodes: [
			{ id: "s", supply: "1" },
			{ id: "t", supply: "-1" },
		],
		edges: [
			{
				id: "cheap",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "1",
				cost: "0",
			},
			{
				id: "dear",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "1",
				cost: "1",
			},
		],
	};
	value.edge_states = [
		{ edge_id: "cheap", flow: "1" },
		{ edge_id: "dear", flow: "0" },
	];
	value.residual_arcs = [
		{
			edge_id: "cheap",
			direction: "forward",
			from: "s",
			to: "t",
			capacity: "0",
			cost: "0",
			active: false,
		},
		{
			edge_id: "cheap",
			direction: "reverse",
			from: "t",
			to: "s",
			capacity: "1",
			cost: "0",
			active: false,
		},
		{
			edge_id: "dear",
			direction: "forward",
			from: "s",
			to: "t",
			capacity: "1",
			cost: "1",
			active: false,
		},
		{
			edge_id: "dear",
			direction: "reverse",
			from: "t",
			to: "s",
			capacity: "0",
			cost: "-1",
			active: false,
		},
	];
	value.node_trace_states = [
		{ node_id: "s", label: "1", remaining_divergence: "0" },
		{ node_id: "t", label: "0", remaining_divergence: "0" },
	];
	value.trace_event = {
		event_id: "15",
		catalog_id: "electrical-flow-interior-point-mcf.optimal",
		minimum_granularity: "phase",
		pseudocode_line: "electrical-ipm:publish-certified-optimum",
		patch_count: 1,
		entity_refs: [{ kind: "edge", edge_id: "cheap" }],
		detail: { label: "certificate checks", value: "1" },
	};
	value.electrical_ipm_mcf_overlay = {
		stage: "optimal",
		seed: "13503953896175478587",
		mu: "0.001",
		epsilon_3: "0.02",
		recovery_epsilon: "0.01",
		duality_gap_bound: "0.004",
		centrality_residual: "0",
		balance_residual: "0",
		step_size: "1",
		electrical_energy: "0",
		linear_residual: "0",
		barrier_objective: "-1",
		isolation_scale: "16",
		perturbation_bound: "4",
		isolation_attempt: "1",
		isolated_optimum_cost: "1",
		isolated_gap: "17",
		nodes: [
			{
				node_id: "s",
				potential: "0",
				potential_direction: "0",
				balance_residual: "0",
				anchored: true,
			},
			{
				node_id: "t",
				potential: "0",
				potential_direction: "0",
				balance_residual: "0",
				anchored: false,
			},
		],
		edges: [
			{
				edge_id: "cheap",
				perturbation: "1",
				isolated_cost: "1",
				fixed_on_face: false,
				face_lower: "0",
				face_upper: "1",
				fractional_flow: "0.9999",
				upper_complement: "0.0001",
				lower_slack: "0.001000100010001",
				upper_multiplier: "10",
				resistance: "0",
				conductance: "0",
				electrical_current: "0",
				lower_slack_direction: "0",
				upper_multiplier_direction: "0",
				final_flow: "1",
			},
			{
				edge_id: "dear",
				perturbation: "2",
				isolated_cost: "18",
				fixed_on_face: false,
				face_lower: "0",
				face_upper: "1",
				fractional_flow: "0.0001",
				upper_complement: "0.9999",
				lower_slack: "10",
				upper_multiplier: "0.001000100010001",
				resistance: "0",
				conductance: "0",
				electrical_current: "0",
				lower_slack_direction: "0",
				upper_multiplier_direction: "0",
				final_flow: "0",
			},
		],
	};
	value.outcome = {
		kind: "min-cost-flow",
		total_cost: "0",
		potentials: [
			{ node_id: "s", potential: "0" },
			{ node_id: "t", potential: "0" },
		],
	};
	return value;
}

function encode(value: unknown): Uint8Array {
	const encoded = structuredClone(value);
	if (
		typeof encoded === "object" &&
		encoded !== null &&
		!Array.isArray(encoded) &&
		"trace_event" in encoded &&
		encoded.trace_event !== undefined &&
		!("trace_event_semantics" in encoded)
	) {
		const terminal = ["primitive-complete", "optimal", "infeasible"].includes(
			String("solve_status" in encoded ? encoded.solve_status : ""),
		);
		const traceEvent = encoded.trace_event as {
			entity_refs?: unknown[];
			minimum_granularity?: string;
		};
		const isDetail = traceEvent.minimum_granularity === "micro";
		Object.assign(encoded, {
			trace_event_semantics: {
				role: terminal ? "certify" : "mutate",
				work_deltas: [
					{ unit: "published-transition", count: "1" },
					...(isDetail ? [{ unit: "detail-primitive", count: "1" }] : []),
				],
				aggregation_count: "1",
				work_progress: {
					detail_completed: isDetail ? "1" : "0",
					detail_total: isDetail ? "1" : "0",
					primary_completed: "0",
					primary_total: "0",
				},
				changed_entity_refs: traceEvent.entity_refs ?? [],
			},
		});
	}
	return new TextEncoder().encode(JSON.stringify(encoded));
}

function encodeRaw(value: unknown): Uint8Array {
	return new TextEncoder().encode(JSON.stringify(value));
}

function validTraceScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "1";
	value.event_count = "1";
	value.solve_status = "running";
	value.trace_granularity = "micro";
	value.trace_event = {
		event_id: "1",
		catalog_id: "edmonds-karp.scan-residual-arc",
		minimum_granularity: "micro",
		pseudocode_line: "Inspect one residual arc",
		patch_count: 1,
		entity_refs: [
			{ kind: "residual-arc", edge_id: "st", direction: "forward" },
		],
		detail: { label: "scan ordinal", value: "1" },
	};
	value.trace_event_semantics = {
		role: "observe",
		work_deltas: [
			{ unit: "published-transition", count: "1" },
			{ unit: "detail-primitive", count: "1" },
			{ unit: "primary-work", count: "1" },
		],
		aggregation_count: "1",
		work_progress: {
			detail_completed: "1",
			detail_total: "1",
			primary_completed: "1",
			primary_total: "1",
		},
		primary_work_block: { first: "1", last: "1", total: "1" },
		changed_entity_refs: [],
	};
	return value;
}

function feasibilityConstructionScene(): Record<string, unknown> {
	const value = validScene();
	value.event_id = "1";
	value.event_count = "10";
	value.solve_status = "running";
	value.model = {
		kind: "fixed-flow-min-cost",
		source: "s",
		sink: "t",
		required_flow: "2",
	};
	value.algorithm = { id: "cost-scaling", config: {} };
	value.graph = {
		// Scenario declaration order is intentionally not the kernel's canonical
		// stable-ID order. The overlay must follow FlowNetwork's order.
		nodes: [
			{ id: "t", supply: "0" },
			{ id: "s", supply: "0", position: { x: "0", y: "0" } },
		],
		edges: [
			{
				id: "st",
				from: "s",
				to: "t",
				lower: "0",
				capacity: "2",
				cost: "1",
			},
		],
	};
	value.edge_states = [{ edge_id: "st", flow: "0" }];
	value.residual_arcs = [
		{
			edge_id: "st",
			direction: "forward",
			from: "s",
			to: "t",
			capacity: "2",
			cost: "1",
			active: true,
		},
		{
			edge_id: "st",
			direction: "reverse",
			from: "t",
			to: "s",
			capacity: "0",
			cost: "-1",
			active: false,
		},
	];
	value.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
	value.trace_event = {
		event_id: "1",
		catalog_id: "feasibility.add-original-arc",
		minimum_granularity: "micro",
		pseudocode_line: "feasibility:shift-one-lower-bounded-edge",
		patch_count: 1,
		entity_refs: [
			{ kind: "residual-arc", edge_id: "st", direction: "forward" },
		],
		detail: { label: "capacity", value: "2" },
	};
	value.trace_event_semantics = {
		role: "mutate",
		work_deltas: [
			{ unit: "published-transition", count: "1" },
			{ unit: "detail-primitive", count: "1" },
			{ unit: "residual-arc-scan", count: "1" },
		],
		aggregation_count: "1",
		work_progress: {
			detail_completed: "1",
			detail_total: "10",
			primary_completed: "0",
			primary_total: "10",
		},
		changed_entity_refs: [],
	};
	const original = (nodeId: string) => ({
		kind: "original",
		original_node_id: nodeId,
	});
	const nodeState = (node: Record<string, string>) => ({
		node,
		height: "0",
		excess: "0",
		current_arc: "0",
		active: false,
		reachable: false,
	});
	value.feasibility_overlay = {
		revision: "flow-feasibility-overlay/2",
		use_kind: "initial-flow",
		domain: {
			kind: "public-input",
			nodes: [
				{ node_id: "s", public_node_id: "s" },
				{ node_id: "t", public_node_id: "t" },
			],
			edges: [
				{
					edge_id: "st",
					from_node_id: "s",
					to_node_id: "t",
					lower: "0",
					capacity: "2",
					public_route_edge_id: "st",
				},
			],
			request: {
				kind: "balance",
				required_divergences: [
					{ node_id: "s", required_divergence: "2" },
					{ node_id: "t", required_divergence: "-2" },
				],
			},
		},
		stage: "add-original-arc",
		nodes: [
			nodeState(original("s")),
			nodeState(original("t")),
			nodeState({ kind: "super-source" }),
			nodeState({ kind: "super-sink" }),
		],
		arcs: [
			{
				arc: { kind: "original", original_edge_id: "st" },
				from: original("s"),
				to: original("t"),
				capacity: "2",
				flow: "0",
				forward_residual: "2",
				reverse_residual: "0",
				focused: true,
				focused_direction: "forward",
			},
		],
		active_queue: [],
		focus_arc: {
			arc: { kind: "original", original_edge_id: "st" },
			direction: "forward",
		},
		total_required: "0",
		routed: "0",
		metrics: {
			original_edge_inspections: "1",
			original_node_inspections: "0",
			auxiliary_adjacency_inspections: "0",
			pushes: "0",
			relabels: "0",
			active_node_selections: "0",
			discharges: "0",
			cut_adjacency_inspections: "0",
			extracted_original_edges: "0",
		},
	};
	return value;
}

describe("projectLinearMcfRequiredDivergence", () => {
	const nodes = [
		{ id: "s", supply: "2" },
		{ id: "a", supply: "-2" },
		{ id: "t", supply: "0" },
	];

	it("adds fixed-flow demand without discarding existing supplies", () => {
		const required = projectLinearMcfRequiredDivergence(
			{
				kind: "fixed-flow-min-cost",
				source: "s",
				sink: "t",
				required_flow: "3",
			},
			nodes,
		);
		expect(Object.fromEntries(required)).toEqual({ s: 5n, a: -2n, t: -3n });
	});

	it("preserves supplies for circulation and rejects malformed domains", () => {
		expect(
			Object.fromEntries(
				projectLinearMcfRequiredDivergence({ kind: "circulation" }, nodes),
			),
		).toEqual({ s: 2n, a: -2n, t: 0n });
		expect(() =>
			projectLinearMcfRequiredDivergence(
				{
					kind: "fixed-flow-min-cost",
					source: "missing",
					sink: "t",
					required_flow: "1",
				},
				nodes,
			),
		).toThrow("terminal is missing");
		expect(() =>
			projectLinearMcfRequiredDivergence(
				{ kind: "max-flow", source: "s", sink: "t" },
				nodes,
			),
		).toThrow("linear MCF model");
		expect(() =>
			projectLinearMcfRequiredDivergence({ kind: "transshipment" }, [
				{ id: "s", supply: "1" },
				{ id: "t", supply: "0" },
			]),
		).toThrow("unbalanced");
	});
});

describe("decodeFlowCurrentSceneV9", () => {
	it("accepts only canonical fast-profile feasibility-work summaries", () => {
		const fast = feasibilityConstructionScene();
		fast.event_id = "0";
		fast.event_count = "0";
		fast.run_profile = "fast";
		fast.solve_status = "running";
		delete fast.trace_event;
		delete fast.trace_event_semantics;
		delete fast.feasibility_overlay;
		fast.metrics = Array.from({ length: 16 }, () => "0");
		fast.feasibility_work = {
			invocations: "1",
			metrics: {
				original_edge_inspections: "1",
				original_node_inspections: "2",
				auxiliary_adjacency_inspections: "9",
				pushes: "2",
				relabels: "1",
				active_node_selections: "1",
				discharges: "1",
				cut_adjacency_inspections: "0",
				extracted_original_edges: "1",
			},
		};
		expect(decodeFlowCurrentSceneV9(encodeRaw(fast)).feasibility_work).toEqual(
			fast.feasibility_work,
		);

		for (const mutate of [
			(value: Record<string, unknown>) => {
				value.run_profile = "trace";
			},
			(value: Record<string, unknown>) => {
				value.solve_status = "ready";
			},
			(value: Record<string, unknown>) => {
				(value.feasibility_work as { invocations: string }).invocations = "0";
			},
			(value: Record<string, unknown>) => {
				const metrics = (
					value.feasibility_work as { metrics: Record<string, string> }
				).metrics;
				for (const key of Object.keys(metrics)) metrics[key] = "0";
			},
			(value: Record<string, unknown>) => {
				(
					value.feasibility_work as { metrics: Record<string, string> }
				).metrics.pushes = "01";
			},
			(value: Record<string, unknown>) => {
				value.feasibility_overlay =
					feasibilityConstructionScene().feasibility_overlay;
			},
		] as const) {
			const invalid = structuredClone(fast);
			mutate(invalid);
			expect(() => decodeFlowCurrentSceneV9(encodeRaw(invalid))).toThrow();
		}
	});

	it("validates the source-canonical feasibility construction independently", () => {
		const valid = feasibilityConstructionScene();
		const decoded = decodeFlowCurrentSceneV9(encodeRaw(valid));
		expect(
			decoded.feasibility_overlay?.nodes.map((state) =>
				state.node.kind === "original"
					? state.node.original_node_id
					: state.node.kind,
			),
		).toEqual(["s", "t", "super-source", "super-sink"]);

		const composedForSsp = feasibilityConstructionScene();
		composedForSsp.algorithm = {
			id: "successive-shortest-path",
			config: {},
		};
		expect(
			decodeFlowCurrentSceneV9(encodeRaw(composedForSsp)).feasibility_overlay,
		).toBeDefined();

		const wrongOrder = feasibilityConstructionScene();
		const wrongOrderOverlay = wrongOrder.feasibility_overlay as {
			nodes: unknown[];
		};
		[wrongOrderOverlay.nodes[0], wrongOrderOverlay.nodes[1]] = [
			wrongOrderOverlay.nodes[1],
			wrongOrderOverlay.nodes[0],
		];
		expect(() => decodeFlowCurrentSceneV9(encodeRaw(wrongOrder))).toThrowError(
			/node order does not match the canonical graph/u,
		);

		const wrongTopology = feasibilityConstructionScene();
		const wrongArc = (
			wrongTopology.feasibility_overlay as {
				arcs: { capacity: string; forward_residual: string }[];
			}
		).arcs[0] as { capacity: string; forward_residual: string };
		wrongArc.capacity = "1";
		wrongArc.forward_residual = "1";
		expect(() =>
			decodeFlowCurrentSceneV9(encodeRaw(wrongTopology)),
		).toThrowError(/topology is not a canonical construction prefix/u);

		const wrongCounter = feasibilityConstructionScene();
		(
			wrongCounter.feasibility_overlay as {
				metrics: { original_edge_inspections: string };
			}
		).metrics.original_edge_inspections = "2";
		expect(() =>
			decodeFlowCurrentSceneV9(encodeRaw(wrongCounter)),
		).toThrowError(/source-work counters do not match/u);

		const wrongUse = feasibilityConstructionScene();
		(
			wrongUse.feasibility_overlay as {
				use_kind: string;
			}
		).use_kind = "implicit-fallback";
		expect(() => decodeFlowCurrentSceneV9(encodeRaw(wrongUse))).toThrowError(
			/feasibility_overlay\.use_kind/u,
		);
	});

	it("keeps precheck-only auxiliary flow separate from public flow", () => {
		const precheck = feasibilityConstructionScene();
		const precheckOverlay = precheck.feasibility_overlay as {
			use_kind: string;
			arcs: Array<{
				flow: string;
				forward_residual: string;
				reverse_residual: string;
			}>;
		};
		precheckOverlay.use_kind = "precheck-only";
		const auxiliaryArc = precheckOverlay.arcs[0];
		if (auxiliaryArc === undefined) throw new Error("fixture arc is missing");
		auxiliaryArc.flow = "1";
		auxiliaryArc.forward_residual = "1";
		auxiliaryArc.reverse_residual = "1";

		expect(
			decodeFlowCurrentSceneV9(encodeRaw(precheck)).feasibility_overlay
				?.use_kind,
		).toBe("precheck-only");

		precheckOverlay.use_kind = "initial-flow";
		expect(() => decodeFlowCurrentSceneV9(encodeRaw(precheck))).toThrowError(
			/original-flow projection disagrees/u,
		);
	});

	it("versions feasibility payloads and admits public transportation requests", () => {
		const stale = feasibilityConstructionScene();
		delete (stale.feasibility_overlay as { revision?: string }).revision;
		expect(() => decodeFlowCurrentSceneV9(encodeRaw(stale))).toThrowError(
			/feasibility_overlay: missing revision/u,
		);

		const transportation = feasibilityConstructionScene();
		transportation.model = {
			kind: "transportation",
			origins: ["s"],
			destinations: ["t"],
		};
		const graph = transportation.graph as {
			nodes: Array<{ id: string; supply: string }>;
		};
		for (const node of graph.nodes) {
			node.supply = node.id === "s" ? "2" : "-2";
		}
		expect(
			decodeFlowCurrentSceneV9(encodeRaw(transportation)).feasibility_overlay
				?.domain.request.kind,
		).toBe("balance");
	});

	it("validates node-aligned transformed feasibility topology without projecting its flow", () => {
		const transformed = feasibilityConstructionScene();
		const overlay = transformed.feasibility_overlay as {
			use_kind: string;
			domain: {
				kind: string;
				edges: Array<{ capacity: string; public_route_edge_id?: string }>;
			};
			arcs: Array<{
				capacity: string;
				forward_residual: string;
			}>;
		};
		overlay.use_kind = "anchored-recovery";
		overlay.domain.kind = "node-aligned-transformation";
		const domainEdge = overlay.domain.edges[0];
		const arc = overlay.arcs[0];
		if (domainEdge === undefined || arc === undefined) {
			throw new Error("transformed feasibility fixture is incomplete");
		}
		domainEdge.capacity = "1";
		arc.capacity = "1";
		arc.forward_residual = "1";
		expect(
			decodeFlowCurrentSceneV9(encodeRaw(transformed)).feasibility_overlay
				?.domain.kind,
		).toBe("node-aligned-transformation");
	});

	it("treats anchored feasibility as an auxiliary boundary without stale parent state", () => {
		const auxiliary = feasibilityConstructionScene();
		auxiliary.algorithm = { id: "orlin-mcf", config: {} };
		const overlay = auxiliary.feasibility_overlay as {
			use_kind: string;
			domain: {
				kind: string;
				edges: Array<{ capacity: string; public_route_edge_id?: string }>;
			};
			arcs: Array<{
				capacity: string;
				forward_residual: string;
			}>;
		};
		overlay.use_kind = "anchored-recovery";
		overlay.domain.kind = "node-aligned-transformation";
		const domainEdge = overlay.domain.edges[0];
		const arc = overlay.arcs[0];
		if (domainEdge === undefined || arc === undefined) {
			throw new Error("auxiliary feasibility fixture is incomplete");
		}
		domainEdge.capacity = "1";
		arc.capacity = "1";
		arc.forward_residual = "1";

		expect(
			decodeFlowCurrentSceneV9(encodeRaw(auxiliary)).feasibility_overlay
				?.use_kind,
		).toBe("anchored-recovery");

		const stale = structuredClone(auxiliary);
		stale.tardos_framework_overlay =
			tardosFrameworkScene().tardos_framework_overlay;
		expect(() => decodeFlowCurrentSceneV9(encodeRaw(stale))).toThrowError(
			/stale algorithm visualization state/u,
		);
	});

	it("bypasses specialized admission only for a plain resource boundary", () => {
		const electrical = validScene();
		electrical.event_id = "1";
		electrical.event_count = "1";
		electrical.solve_status = "resource-limit";
		electrical.resource_limit_reason = "input-admission";
		electrical.algorithm = { id: "electrical-flow", config: {} };
		const electricalGraph = electrical.graph as {
			edges: Record<string, unknown>[];
		};
		electricalGraph.edges[0] = {
			...(electricalGraph.edges[0] as Record<string, unknown>),
			capacity: "1000001",
			cost: "0",
		};
		const electricalState = electrical.edge_states as Record<string, unknown>[];
		electricalState[0] = { edge_id: "st", flow: "0" };
		const electricalResidual = electrical.residual_arcs as Record<
			string,
			unknown
		>[];
		electricalResidual[0] = {
			...electricalResidual[0],
			capacity: "1000001",
			cost: "0",
		};
		electricalResidual[1] = {
			...electricalResidual[1],
			cost: "0",
		};
		expect(decodeFlowCurrentSceneV9(encode(electrical)).solve_status).toBe(
			"resource-limit",
		);
		const electricalReady = structuredClone(electrical);
		electricalReady.event_id = "0";
		electricalReady.event_count = "0";
		electricalReady.solve_status = "ready";
		delete electricalReady.resource_limit_reason;
		expect(() => decodeFlowCurrentSceneV9(encode(electricalReady))).toThrow();

		const minimumRatio = structuredClone(electrical);
		minimumRatio.algorithm = {
			id: "minimum-ratio-cycle-max-flow",
			config: {},
		};
		expect(decodeFlowCurrentSceneV9(encode(minimumRatio)).solve_status).toBe(
			"resource-limit",
		);

		const parametric = parametricScene();
		parametric.event_id = "1";
		parametric.event_count = "1";
		parametric.solve_status = "resource-limit";
		parametric.resource_limit_reason = "trace-publication";
		delete parametric.parametric_overlay;
		expect(decodeFlowCurrentSceneV9(encode(parametric)).solve_status).toBe(
			"resource-limit",
		);

		const executionStateAtResourceBoundary = electricalFlowScene();
		executionStateAtResourceBoundary.event_id = "1";
		executionStateAtResourceBoundary.event_count = "1";
		executionStateAtResourceBoundary.solve_status = "resource-limit";
		executionStateAtResourceBoundary.resource_limit_reason = "runtime-work";
		delete executionStateAtResourceBoundary.trace_event;
		delete executionStateAtResourceBoundary.outcome;
		expect(() =>
			decodeFlowCurrentSceneV9(encode(executionStateAtResourceBoundary)),
		).toThrow("Flow resource-limit boundary or reason is inconsistent");

		const missingReason = structuredClone(electrical);
		delete missingReason.resource_limit_reason;
		expect(() => decodeFlowCurrentSceneV9(encode(missingReason))).toThrow(
			"Flow resource-limit boundary or reason is inconsistent",
		);

		const reasonOnReady = validScene();
		reasonOnReady.resource_limit_reason = "declared-ceiling";
		expect(() => decodeFlowCurrentSceneV9(encode(reasonOnReady))).toThrow(
			"Flow resource-limit boundary or reason is inconsistent",
		);
	});

	it("validates exact Flow Framework MCF query and source final-point boundaries", () => {
		const query = decodeFlowCurrentSceneV9(encode(flowFrameworkMcfScene()));
		expect(query.solve_status).toBe("running");
		expect(query.flow_framework_mcf_overlay).toEqual(
			expect.objectContaining({
				stage: "query-minimum-ratio-cycle",
				dynamic_operation: "cycle-queried-accepted",
				dynamic_operation_serial: "1",
				accepted_ratio: { numerator: "10", denominator: "1" },
			}),
		);
		expect(
			query.flow_framework_mcf_overlay?.edges.filter((edge) => edge.selected),
		).toHaveLength(3);

		const optimal = decodeFlowCurrentSceneV9(
			encode(flowFrameworkMcfScene("optimal")),
		);
		expect(optimal.solve_status).toBe("optimal");
		expect(optimal.outcome).toEqual(
			expect.objectContaining({ kind: "min-cost-flow", total_cost: "6" }),
		);
		expect(optimal.flow_framework_mcf_overlay?.termination).toBe(
			"source-additive-half-gap",
		);
		expect(optimal.flow_framework_mcf_overlay?.optimum_cost).toBe("6");
		expect(
			optimal.flow_framework_mcf_overlay?.final_point_edges?.filter(
				(edge) => edge.auxiliary,
			),
		).toEqual([expect.objectContaining({ rounded_flow: "0" })]);
	});

	it("admits the Flow Framework 4096-bit rational band without widening other scenes", () => {
		const extended = flowFrameworkMcfScene();
		(
			extended.flow_framework_mcf_overlay as {
				exact_gap_before: { numerator: string; denominator: string };
			}
		).exact_gap_before = {
			numerator: `1${"0".repeat(128)}`,
			denominator: "1",
		};
		expect(() => decodeFlowCurrentSceneV9(encode(extended))).not.toThrow();

		const oversized = flowFrameworkMcfScene();
		(
			oversized.flow_framework_mcf_overlay as {
				exact_gap_before: { numerator: string; denominator: string };
			}
		).exact_gap_before = {
			numerator: "1".repeat(1235),
			denominator: "1",
		};
		expect(() => decodeFlowCurrentSceneV9(encode(oversized))).toThrowError(
			/invalid exact rational/,
		);
	});

	it("rejects nonnormalized, noncirculating, and uncertified Flow Framework MCF state", () => {
		const wrongGate = flowFrameworkMcfScene();
		(
			wrongGate.flow_framework_mcf_overlay as {
				stopping_gap: { numerator: string; denominator: string };
			}
		).stopping_gap = { numerator: "1", denominator: "3" };
		expect(() => decodeFlowCurrentSceneV9(encode(wrongGate))).toThrowError(
			/scalar or stopping state is inconsistent/,
		);

		const nonnormalized = flowFrameworkMcfScene();
		const nonnormalizedEdge = (
			nonnormalized.flow_framework_mcf_overlay as {
				edges: { flow: { numerator: string; denominator: string } }[];
			}
		).edges[0];
		if (nonnormalizedEdge === undefined)
			throw new Error("missing Flow Framework edge");
		nonnormalizedEdge.flow = { numerator: "6", denominator: "4" };
		expect(() => decodeFlowCurrentSceneV9(encode(nonnormalized))).toThrowError(
			/rational is not normalized/,
		);

		const noncirculating = flowFrameworkMcfScene();
		const cycleEdge = (
			noncirculating.flow_framework_mcf_overlay as {
				edges: {
					cycle_coefficient: { numerator: string; denominator: string };
				}[];
			}
		).edges[2];
		if (cycleEdge === undefined)
			throw new Error("missing Flow Framework cycle edge");
		cycleEdge.cycle_coefficient = { numerator: "-2", denominator: "1" };
		expect(() => decodeFlowCurrentSceneV9(encode(noncirculating))).toThrowError(
			/not a circulation/,
		);

		const missingDynamicSerial = flowFrameworkMcfScene();
		delete (
			missingDynamicSerial.flow_framework_mcf_overlay as {
				dynamic_operation_serial?: string;
			}
		).dynamic_operation_serial;
		expect(() =>
			decodeFlowCurrentSceneV9(encode(missingDynamicSerial)),
		).toThrowError(/dynamic operation identity is inconsistent/);

		const wrongDynamicOperation = flowFrameworkMcfScene();
		(
			wrongDynamicOperation.flow_framework_mcf_overlay as {
				dynamic_operation: string;
			}
		).dynamic_operation = "flow-applied";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(wrongDynamicOperation)),
		).toThrowError(/dynamic operation identity is inconsistent/);

		const uncertified = flowFrameworkMcfScene("optimal");
		delete (
			uncertified.flow_framework_mcf_overlay as {
				termination?: string;
			}
		).termination;
		expect(() => decodeFlowCurrentSceneV9(encode(uncertified))).toThrowError(
			/scalar or stopping state is inconsistent/,
		);

		const changedPoint = flowFrameworkMcfScene("optimal");
		const changedPointEdge = (
			changedPoint.flow_framework_mcf_overlay as {
				final_point_edges: {
					flow: { numerator: string; denominator: string };
				}[];
			}
		).final_point_edges[0];
		if (changedPointEdge === undefined)
			throw new Error("missing final-point edge");
		changedPointEdge.flow = { numerator: "17", denominator: "6" };
		expect(() => decodeFlowCurrentSceneV9(encode(changedPoint))).toThrowError(
			/exact final-point or rounding proof is inconsistent|original final-point projection is inconsistent|final-point divergence anchor is inconsistent/,
		);

		const nonzeroAuxiliary = flowFrameworkMcfScene("optimal");
		const auxiliaryEdge = (
			nonzeroAuxiliary.flow_framework_mcf_overlay as {
				final_point_edges: { auxiliary: boolean; rounded_flow: string }[];
			}
		).final_point_edges.find((edge) => edge.auxiliary);
		if (auxiliaryEdge === undefined) throw new Error("missing auxiliary edge");
		auxiliaryEdge.rounded_flow = "1";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(nonzeroAuxiliary)),
		).toThrowError(/augmented rounding is inconsistent/);
	});

	it("accepts exact parametric overlays without generic flow snapshots", () => {
		const decoded = decodeFlowCurrentSceneV9(
			new TextEncoder().encode(JSON.stringify(parametricScene())),
		);
		expect(decoded.model.kind).toBe("parametric-max-flow");
		expect(decoded.edge_states).toEqual([]);
		expect(decoded.parametric_overlay?.recorded_breakpoints).toEqual([
			expect.objectContaining({
				parameter: { numerator: "1", denominator: "1" },
				entering_nodes: ["a"],
			}),
		]);
		const reordered = parametricScene();
		(
			reordered.parametric_overlay as {
				edge_capacities: unknown[];
			}
		).edge_capacities.reverse();
		expect(() =>
			decodeFlowCurrentSceneV9(
				new TextEncoder().encode(JSON.stringify(reordered)),
			),
		).not.toThrow();
	});

	it("compares terminal parametric certificates semantically across key order", () => {
		const terminal = parametricScene();
		terminal.event_id = "2";
		terminal.solve_status = "optimal";
		const overlay = terminal.parametric_overlay as {
			stage: string;
			parameter: Record<string, string>;
			edge_capacities: Record<string, unknown>[];
			recorded_segments: Record<string, unknown>[];
			recorded_breakpoints: Record<string, unknown>[];
		};
		overlay.stage = "optimal";
		overlay.parameter = { numerator: "2", denominator: "1" };
		overlay.edge_capacities = [
			{
				edge_id: "at",
				capacity: { numerator: "1", denominator: "1" },
			},
			{
				edge_id: "sa",
				capacity: { numerator: "3", denominator: "1" },
			},
		];
		overlay.recorded_segments.push({
			lower: { numerator: "1", denominator: "1" },
			upper: { numerator: "2", denominator: "1" },
			intercept: "2",
			slope: "0",
			minimal_source_side: ["a", "s"],
			maximal_source_side: ["a", "s"],
		});
		const metrics = Object.fromEntries(
			[
				"forest_initializations",
				"parameter_advances",
				"forest_reuses",
				"renormalization_pushes",
				"renormalization_splits",
				"mergers",
				"relabels",
				"free_run_races",
				"forward_race_wins",
				"reverse_race_wins",
				"cooperative_race_steps",
				"contraction_views",
				"smaller_child_restarts",
				"larger_child_continuations",
				"maximum_depth",
				"residual_arc_scans",
			].map((key) => [key, "0"]),
		);
		terminal.outcome = {
			kind: "parametric-max-flow",
			segments: overlay.recorded_segments.map((segment) => ({
				maximal_source_side: segment.maximal_source_side,
				minimal_source_side: segment.minimal_source_side,
				slope: segment.slope,
				intercept: segment.intercept,
				upper: segment.upper,
				lower: segment.lower,
			})),
			breakpoints: overlay.recorded_breakpoints.map((breakpoint) => ({
				entering_nodes: breakpoint.entering_nodes,
				exact_maximal_source_side: breakpoint.exact_maximal_source_side,
				exact_minimal_source_side: breakpoint.exact_minimal_source_side,
				after_source_side: breakpoint.after_source_side,
				before_source_side: breakpoint.before_source_side,
				parameter: breakpoint.parameter,
			})),
			metrics: { implementation: "parametric-pseudoflow", ...metrics },
		};

		expect(() => decodeFlowCurrentSceneV9(encode(terminal))).not.toThrow();
	});

	it("uses the domain upper endpoint for an optimal traversal without inventing a probe", () => {
		const terminal = parametricScene();
		const overlay = terminal.parametric_overlay as {
			stage: string;
			parameter: { numerator: string; denominator: string };
			edge_capacities: {
				edge_id: string;
				capacity: { numerator: string; denominator: string };
			}[];
			traversal?: Record<string, unknown>;
		};
		overlay.stage = "optimal";
		overlay.parameter = { numerator: "2", denominator: "1" };
		overlay.edge_capacities = [
			{
				edge_id: "at",
				capacity: { numerator: "1", denominator: "1" },
			},
			{
				edge_id: "sa",
				capacity: { numerator: "3", denominator: "1" },
			},
		];
		overlay.traversal = {
			kind: "optimal",
			lower: { numerator: "0", denominator: "1" },
			upper: { numerator: "2", denominator: "1" },
			cold_static_rerun: false,
			lower_source_side: [],
			upper_source_side: [],
			normalized_tree_reused: false,
			labels_retained: false,
			renormalization_pushes: "0",
			renormalization_splits: "0",
		};

		const decoded = decodeFlowCurrentSceneV9(
			new TextEncoder().encode(JSON.stringify(terminal)),
		);
		expect(decoded.parametric_overlay?.parameter).toEqual({
			numerator: "2",
			denominator: "1",
		});
		expect(decoded.parametric_overlay?.traversal?.probe).toBeUndefined();
	});

	it("rejects an initialize-forest traversal without its source orientation", () => {
		const initialized = parametricScene();
		const overlay = initialized.parametric_overlay as {
			stage: string;
			traversal?: Record<string, unknown>;
		};
		overlay.stage = "initialize-forest";
		overlay.traversal = {
			kind: "initialize-forest",
			lower: { numerator: "0", denominator: "1" },
			upper: { numerator: "0", denominator: "1" },
			probe: { numerator: "0", denominator: "1" },
			cold_static_rerun: false,
			lower_source_side: [],
			upper_source_side: [],
			normalized_tree_reused: false,
			labels_retained: false,
			renormalization_pushes: "0",
			renormalization_splits: "0",
		};

		expect(() => decodeFlowCurrentSceneV9(encode(initialized))).toThrowError(
			/invalid parametric traversal/,
		);
	});

	it("rejects a nonnormalized parameter and a drifting fixed visual scale", () => {
		const nonnormalized = parametricScene();
		(
			nonnormalized.parametric_overlay as {
				parameter: { numerator: string; denominator: string };
			}
		).parameter = { numerator: "2", denominator: "2" };
		expect(() =>
			decodeFlowCurrentSceneV9(
				new TextEncoder().encode(JSON.stringify(nonnormalized)),
			),
		).toThrowError(/rational is not normalized/);

		const driftingScale = parametricScene();
		(
			driftingScale.parametric_overlay as {
				visual_scale_max_capacity: { numerator: string; denominator: string };
			}
		).visual_scale_max_capacity = { numerator: "2", denominator: "1" };
		expect(() =>
			decodeFlowCurrentSceneV9(
				new TextEncoder().encode(JSON.stringify(driftingScale)),
			),
		).toThrowError(/fixed scale disagree/);
	});
	it("migrates a strict static V6 fixture only through the explicit boundary", async () => {
		const legacy = validScene();
		const contract = legacyMigrationCatalog(legacy);
		delete (legacy.trace_steps as { primary_work: { visualization?: string } })
			.primary_work.visualization;
		legacy.result_schema_version = 6;
		legacy.frame_revision = "flow-scene/6";
		expect(() => decodeFlowCurrentSceneV9(encode(legacy))).toThrow(
			/wrong constant/,
		);
		const migrated = await migrateFlowCurrentSceneV6(encode(legacy), contract);
		expect(migrated.result_schema_version).toBe(9);
		expect(migrated.frame_revision).toBe("flow-scene/9");

		legacy.future = true;
		await expect(
			migrateFlowCurrentSceneV6(encode(legacy), contract),
		).rejects.toThrow(/unknown field future/);
	});

	it("migrates V7 only from unknown input through its dedicated boundary", async () => {
		const base = validScene();
		const contract = legacyMigrationCatalog(base);
		delete (base.trace_steps as { primary_work: { visualization?: string } })
			.primary_work.visualization;
		const legacy: unknown = {
			...base,
			result_schema_version: 7,
			frame_revision: "flow-scene/7",
		};
		expect(() => decodeFlowCurrentSceneV9(encode(legacy))).toThrow(
			/wrong constant/,
		);
		const migrated = await migrateFlowCurrentSceneV7(encode(legacy), contract);
		expect(migrated.result_schema_version).toBe(9);
		expect(migrated.frame_revision).toBe("flow-scene/9");
	});

	it("binds legacy work contracts to the internal revision and rejects unverifiable events", async () => {
		const electrical = validScene();
		electrical.algorithm = { id: "electrical-flow", config: {} };
		electrical.trace_steps = {
			...(electrical.trace_steps as Record<string, unknown>),
			primary_work: {
				metric_ordinal: 3,
				unit: "CG iterations",
				abstraction: "iteration",
			},
		};
		(
			(electrical.graph as { edges: Record<string, unknown>[] })
				.edges[0] as Record<string, unknown>
		).capacity = "3";
		(
			(electrical.graph as { edges: Record<string, unknown>[] })
				.edges[0] as Record<string, unknown>
		).cost = "0";
		const electricalResidual = electrical.residual_arcs as Record<
			string,
			unknown
		>[];
		electricalResidual[0] = {
			...electricalResidual[0],
			capacity: "3",
			cost: "0",
		};
		electricalResidual[1] = {
			...electricalResidual[1],
			cost: "0",
		};
		electrical.result_schema_version = 7;
		electrical.frame_revision = "flow-scene/7";
		const contract = legacyMigrationCatalog(electrical, "electrical-flow");
		const electricalContract = contract[0];
		expect(electricalContract).toBeDefined();
		if (electricalContract === undefined) {
			throw new Error("electrical migration contract is missing");
		}
		await expect(
			migrateFlowCurrentSceneV7(encode(electrical), contract),
		).rejects.toThrowError(/migration input is invalid/);
		await expect(
			migrateFlowCurrentSceneV7(encode(electrical), [
				{ ...electricalContract, id: "edmonds-karp" },
			]),
		).rejects.toThrowError(/migration input is invalid/);
		await expect(
			migrateFlowCurrentSceneV7(encode(electrical), [
				{
					...electricalContract,
					trace_steps: {
						...electricalContract.trace_steps,
						primary_work: {
							metric_ordinal: 2,
							unit: "residual-arc inspections",
							abstraction: "primitive",
							visualization: "edge-field",
						},
					},
				},
			]),
		).rejects.toThrowError(/migration input is invalid/);

		electrical.trace_event = {
			event_id: "1",
			catalog_id: "electrical-flow.cg-iteration",
			minimum_granularity: "operation",
			pseudocode_line: "electrical-flow:cg-iteration",
			patch_count: 1,
			entity_refs: [],
		};
		await expect(
			migrateFlowCurrentSceneV7(encode(electrical), contract),
		).rejects.toThrowError(/migration input is invalid/);
	});

	it("retains exact decimal graph values", () => {
		const scene = decodeFlowCurrentSceneV9(encode(validScene()));

		expect(scene.graph.edges[0]?.capacity).toBe("18446744073709551615");
		expect(scene.graph.edges[0]?.cost).toBe("-7");
		expect(scene.metrics).toHaveLength(16);
	});

	it("uses Rust-compatible UTF-8 byte order for canonical node traces", () => {
		const bmpNode = "\u{e000}";
		const supplementaryNode = "\u{10000}";
		const value = validScene();
		value.model = {
			kind: "max-flow",
			source: bmpNode,
			sink: supplementaryNode,
		};
		const graph = value.graph as {
			nodes: { id: string; supply: string }[];
			edges: { from: string; to: string }[];
		};
		graph.nodes = [
			{ id: supplementaryNode, supply: "0" },
			{ id: bmpNode, supply: "0" },
		];
		if (graph.edges[0] === undefined)
			throw new Error("fixture edge is missing");
		graph.edges[0].from = bmpNode;
		graph.edges[0].to = supplementaryNode;
		const residualArcs = value.residual_arcs as {
			from: string;
			to: string;
		}[];
		if (residualArcs[0] === undefined || residualArcs[1] === undefined) {
			throw new Error("fixture residual arcs are missing");
		}
		residualArcs[0].from = bmpNode;
		residualArcs[0].to = supplementaryNode;
		residualArcs[1].from = supplementaryNode;
		residualArcs[1].to = bmpNode;
		value.node_trace_states = [bmpNode, supplementaryNode].map((node_id) => ({
			node_id,
		}));

		expect(
			decodeFlowCurrentSceneV9(encode(value)).node_trace_states.map(
				(state) => state.node_id,
			),
		).toEqual([bmpNode, supplementaryNode]);

		value.node_trace_states = [supplementaryNode, bmpNode].map((node_id) => ({
			node_id,
		}));
		expect(() => decodeFlowCurrentSceneV9(encode(value))).toThrowError(
			/node trace state does not match/,
		);
	});

	it("rejects the pre-bipartite-matching scene revision", () => {
		const legacy = validScene();
		legacy.result_schema_version = 3;
		legacy.frame_revision = "flow-scene/3";

		expect(() => decodeFlowCurrentSceneV9(encode(legacy))).toThrow(
			/wrong constant/,
		);
	});

	it("defaults the additive fixed overlay to false and preserves explicit true", () => {
		const legacyCompatible = decodeFlowCurrentSceneV9(encode(validScene()));
		expect(legacyCompatible.residual_arcs.map((arc) => arc.fixed)).toEqual([
			false,
			false,
		]);

		const fixed = validScene();
		const arcs = fixed.residual_arcs as Record<string, unknown>[];
		if (arcs[0] === undefined || arcs[1] === undefined) {
			throw new Error("fixture arcs are missing");
		}
		arcs[0].fixed = true;
		arcs[1].fixed = true;
		expect(
			decodeFlowCurrentSceneV9(encode(fixed)).residual_arcs.map(
				(arc) => arc.fixed,
			),
		).toEqual([true, true]);

		const inconsistent = validScene();
		const inconsistentArcs = inconsistent.residual_arcs as Record<
			string,
			unknown
		>[];
		if (inconsistentArcs[0] === undefined) {
			throw new Error("fixture forward arc is missing");
		}
		inconsistentArcs[0].fixed = true;
		expect(() => decodeFlowCurrentSceneV9(encode(inconsistent))).toThrow(
			"Flow scene residual directions disagree on fixed state",
		);
	});

	it("accepts a certified max-flow outcome and rejects edge-state drift", () => {
		const valid = validScene();
		valid.event_id = "1";
		valid.event_count = "1";
		valid.solve_status = "optimal";
		valid.run_profile = "fast";
		valid.edge_states = [{ edge_id: "st", flow: "9" }];
		valid.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "18446744073709551606",
				cost: "-7",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "9",
				cost: "7",
				active: false,
			},
		];
		valid.outcome = {
			kind: "max-flow",
			value: "9",
			cut_bound: "9",
			source_side: ["s"],
		};
		expect(decodeFlowCurrentSceneV9(encode(valid)).outcome).toEqual(
			valid.outcome,
		);

		const wrongModel = structuredClone(valid);
		wrongModel.model = { kind: "transshipment" };
		expect(() => decodeFlowCurrentSceneV9(encode(wrongModel))).toThrowError(
			/outcome does not match its problem model/,
		);

		valid.edge_states = [{ edge_id: "st", flow: "18446744073709551616" }];
		expect(() => decodeFlowCurrentSceneV9(encode(valid))).toThrowError(
			/edge state does not match/,
		);
	});

	it("accepts a checked binary blocking primitive without treating it as max flow", () => {
		const valid = validScene();
		valid.event_id = "1";
		valid.event_count = "1";
		valid.solve_status = "primitive-complete";
		valid.run_profile = "fast";
		valid.algorithm = { id: "binary-blocking-flow", config: {} };
		valid.edge_states = [{ edge_id: "st", flow: "1" }];
		valid.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "18446744073709551614",
				cost: "-7",
				active: true,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "1",
				cost: "7",
				active: false,
			},
		];
		valid.binary_blocking_overlay = {
			stage: "complete",
			upper_bound: "18446744073709551615",
			delta: "1",
			delivered: "1",
			nodes: [
				{ node_id: "s", distance: "0", component: "0" },
				{ node_id: "t", distance: "0", component: "1" },
			],
			base_zero_arcs: [{ edge_id: "st", direction: "forward" }],
			special_arcs: [],
			admissible_arcs: [{ edge_id: "st", direction: "forward" }],
			zero_admissible_arcs: [{ edge_id: "st", direction: "forward" }],
		};
		valid.outcome = {
			kind: "binary-blocking-flow",
			upper_bound: "18446744073709551615",
			delta: "1",
			delivered: "1",
			termination: "delta-reached",
			component_count: "2",
			nontrivial_component_count: "0",
			augmentation_operations: "1",
		};
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("primitive-complete");
		expect(decoded.outcome).toEqual(valid.outcome);
		expect(decoded.binary_blocking_overlay?.zero_admissible_arcs).toHaveLength(
			1,
		);

		const counterBoundary = validScene();
		counterBoundary.algorithm = { id: "binary-blocking-flow", config: {} };
		counterBoundary.run_profile = "trace";
		counterBoundary.solve_status = "running";
		counterBoundary.event_id = "1";
		counterBoundary.event_count = "2";
		const counterMetrics = counterBoundary.metrics as string[];
		counterMetrics[2] = "1";
		counterBoundary.trace_event = {
			event_id: "1",
			catalog_id:
				"binary-blocking-flow.inspect-binary-length.primary-work-unit",
			minimum_granularity: "micro",
			pseudocode_line: "binary-blocking-flow:inspect-one-positive-residual-arc",
			patch_count: 0,
			entity_refs: [],
			detail: {
				label: "residual-arc inspections · units 1–1 of 1",
				value: "1",
			},
		};
		counterBoundary.trace_event_semantics = {
			role: "observe",
			work_deltas: [
				{ unit: "published-transition", count: "1" },
				{ unit: "detail-primitive", count: "1" },
				{ unit: "primary-work", count: "1" },
			],
			aggregation_count: "1",
			work_progress: {
				detail_completed: "1",
				detail_total: "1",
				primary_completed: "1",
				primary_total: "1",
			},
			primary_work_block: { first: "1", last: "1", total: "1" },
			changed_entity_refs: [],
		};
		expect(() =>
			decodeFlowCurrentSceneV9(encode(counterBoundary)),
		).toThrowError(/synthetic counter-only Detail/u);

		const analysisWork = validScene();
		analysisWork.algorithm = { id: "binary-blocking-flow", config: {} };
		analysisWork.solve_status = "running";
		analysisWork.event_id = "1";
		analysisWork.event_count = "2";
		const analysisMetrics = analysisWork.metrics as string[];
		analysisMetrics[2] = "1";
		analysisWork.trace_event = {
			event_id: "1",
			catalog_id: "binary-blocking-flow.analyze-binary-network",
			minimum_granularity: "micro",
			pseudocode_line: "binary-blocking-flow:analyze-binary-network",
			patch_count: 1,
			entity_refs: [{ kind: "edge", edge_id: "st" }],
			detail: {
				label: "work unit",
				value: "1",
			},
		};
		analysisWork.trace_event_semantics = {
			role: "observe",
			work_deltas: [
				{ unit: "published-transition", count: "1" },
				{ unit: "detail-primitive", count: "1" },
				{ unit: "primary-work", count: "1" },
			],
			aggregation_count: "1",
			work_progress: {
				detail_completed: "1",
				detail_total: "2",
				primary_completed: "1",
				primary_total: "2",
			},
			primary_work_block: { first: "1", last: "1", total: "2" },
			changed_entity_refs: [],
		};
		expect(() => decodeFlowCurrentSceneV9(encode(analysisWork))).toThrowError(
			/missing its structural overlay/u,
		);

		const inspected = structuredClone(valid);
		inspected.solve_status = "running";
		inspected.run_profile = "trace";
		inspected.event_id = "2";
		inspected.event_count = "4";
		delete inspected.outcome;
		const inspectedOverlay = inspected.binary_blocking_overlay as {
			stage: string;
			delivered: string;
			base_zero_arcs: unknown[];
			special_arcs: unknown[];
			admissible_arcs: unknown[];
			zero_admissible_arcs: unknown[];
		};
		inspectedOverlay.stage = "analyzing";
		inspectedOverlay.delivered = "0";
		inspectedOverlay.base_zero_arcs = [];
		inspectedOverlay.special_arcs = [];
		inspectedOverlay.admissible_arcs = [];
		inspectedOverlay.zero_admissible_arcs = [];
		inspected.trace_event = {
			event_id: "2",
			parent_phase_id: "1",
			catalog_id: "binary-blocking-flow.inspect-binary-length",
			minimum_granularity: "micro",
			pseudocode_line: "binary-blocking-flow:inspect-one-positive-residual-arc",
			patch_count: 1,
			entity_refs: [
				{ kind: "edge", edge_id: "st" },
				{ kind: "residual-arc", edge_id: "st", direction: "forward" },
			],
			detail: { label: "binary-length", value: "0" },
		};
		expect(() => decodeFlowCurrentSceneV9(encode(inspected))).not.toThrow();
		(
			inspected.trace_event as {
				catalog_id: string;
			}
		).catalog_id = "binary-blocking-flow.contract-zero-scc";
		expect(() => decodeFlowCurrentSceneV9(encode(inspected))).toThrowError(
			/event and stage disagree/,
		);

		(valid.outcome as Record<string, unknown>).termination = "blocking";
		expect(() => decodeFlowCurrentSceneV9(encode(valid))).toThrowError(
			/primitive outcome is inconsistent/,
		);

		(valid.outcome as Record<string, unknown>).termination = "delta-reached";
		valid.algorithm = { id: "edmonds-karp", config: {} };
		expect(() => decodeFlowCurrentSceneV9(encode(valid))).toThrowError(
			/wrong algorithm/,
		);
	});

	it("accepts a combined minimum-cost maximum-flow outcome", () => {
		const valid = validScene();
		valid.solve_status = "optimal";
		valid.model = { kind: "min-cost-max-flow", source: "s", sink: "t" };
		valid.algorithm = {
			id: "successive-shortest-augmenting-path",
			config: {},
		};
		valid.outcome = {
			kind: "min-cost-max-flow",
			value: "0",
			cut_bound: "0",
			source_side: ["s"],
			total_cost: "0",
			potentials: [
				{ node_id: "s", potential: "0" },
				{ node_id: "t", potential: "-7" },
			],
		};
		expect(decodeFlowCurrentSceneV9(encode(valid)).outcome).toEqual(
			valid.outcome,
		);
	});

	it("accepts a native matching with a matching-size minimum vertex cover", () => {
		const valid = matchingScene();
		expect(decodeFlowCurrentSceneV9(encode(valid)).outcome).toEqual(
			valid.outcome,
		);
	});

	it("rejects matching pairs, flows, and covers that disagree", () => {
		const wrongPair = matchingScene();
		(
			wrongPair.outcome as {
				pairs: { edge_id: string; left: string; right: string }[];
			}
		).pairs[0] = { edge_id: "b00", left: "l0", right: "r1" };
		expect(() => decodeFlowCurrentSceneV9(encode(wrongPair))).toThrowError(
			/matching outcome does not match current flow/,
		);

		const uncovered = matchingScene();
		(uncovered.outcome as { cover_left: string[] }).cover_left = ["l1"];
		(uncovered.outcome as { cover_right: string[] }).cover_right = ["r1"];
		expect(() => decodeFlowCurrentSceneV9(encode(uncovered))).toThrowError(
			/cover does not cover every compatibility edge/,
		);

		const duplicatePartition = matchingScene();
		(duplicatePartition.model as { left: string[] }).left = ["l0", "l0"];
		expect(() =>
			decodeFlowCurrentSceneV9(encode(duplicatePartition)),
		).toThrowError(/noncanonical matching partitions/);
	});

	it("accepts an exact assignment primal-dual certificate", () => {
		const valid = assignmentScene();
		expect(decodeFlowCurrentSceneV9(encode(valid)).outcome).toEqual(
			valid.outcome,
		);
	});

	it("accepts transportation primal-dual data and rejects route drift", () => {
		const valid = transportationScene();
		expect(decodeFlowCurrentSceneV9(encode(valid)).model.kind).toBe(
			"transportation",
		);

		const binding = transportationScene();
		(
			(binding.graph as { edges: Record<string, unknown>[] })
				.edges[0] as Record<string, unknown>
		).capacity = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(binding))).toThrowError(
			/invalid route|graph edge/,
		);

		const badDual = transportationScene();
		(
			(badDual.outcome as { potentials: { potential: string }[] })
				.potentials[0] as { potential: string }
		).potential = "2";
		expect(() => decodeFlowCurrentSceneV9(encode(badDual))).toThrowError(
			/dual certificate is invalid/,
		);
	});

	it("accepts a complete planar rotation system and rejects certificate drift", () => {
		const valid = planarTriangleScene();
		const model = decodeFlowCurrentSceneV9(encode(valid)).model;
		expect(model.kind).toBe("planar-max-flow");
		if (model.kind !== "planar-max-flow")
			throw new Error("planar fixture drift");
		expect(model.embedding.rotations).toHaveLength(3);

		const noncanonical = planarTriangleScene();
		(
			noncanonical.model as { embedding: { rotations: unknown[] } }
		).embedding.rotations.reverse();
		expect(() => decodeFlowCurrentSceneV9(encode(noncanonical))).toThrowError(
			/noncanonical|rotations are not canonical/,
		);

		const missing = planarTriangleScene();
		(
			(missing.model as { embedding: { rotations: { darts: unknown[] }[] } })
				.embedding.rotations[0] as { darts: unknown[] }
		).darts.pop();
		expect(() => decodeFlowCurrentSceneV9(encode(missing))).toThrowError(
			/omits a dart/,
		);

		const wrongFace = planarTriangleScene();
		(
			wrongFace.model as {
				embedding: {
					terminal_corners: { source: unknown; sink: unknown };
				};
			}
		).embedding.terminal_corners = {
			source: { edge_id: "ab", direction: "forward" },
			sink: { edge_id: "ac", direction: "reverse" },
		};
		expect(() => decodeFlowCurrentSceneV9(encode(wrongFace))).toThrowError(
			/terminal corners are invalid/,
		);

		const unknown = planarTriangleScene();
		(unknown.model as { embedding: Record<string, unknown> }).embedding.future =
			true;
		expect(() => decodeFlowCurrentSceneV9(encode(unknown))).toThrowError(
			/structure mismatch.*model/,
		);
	});

	it("independently rejects a non-planar K3,3 rotation system", () => {
		expect(() =>
			decodeFlowCurrentSceneV9(encode(nonPlanarK33Scene())),
		).toThrowError(/rotation system is not planar/);
	});

	it("rejects assignment pair, dual, and objective drift", () => {
		const wrongPair = assignmentScene();
		(
			wrongPair.outcome as {
				pairs: { edge_id: string; agent: string; task: string; cost: string }[];
			}
		).pairs[0] = { edge_id: "e00", agent: "a0", task: "t1", cost: "1" };
		expect(() => decodeFlowCurrentSceneV9(encode(wrongPair))).toThrowError(
			/pairs do not match current flow/,
		);

		const badDual = assignmentScene();
		(
			badDual.outcome as { task_labels: { node_id: string; label: string }[] }
		).task_labels[0] = { node_id: "t0", label: "1" };
		expect(() => decodeFlowCurrentSceneV9(encode(badDual))).toThrowError(
			/task-label sign is invalid/,
		);

		const badObjective = assignmentScene();
		(badObjective.outcome as { total_cost: string }).total_cost = "2";
		expect(() => decodeFlowCurrentSceneV9(encode(badObjective))).toThrowError(
			/primal\/dual objectives differ/,
		);
	});

	it("accepts an exact Hall witness and rejects a non-neighbor omission", () => {
		const valid = hallDeficientAssignmentScene();
		expect(decodeFlowCurrentSceneV9(encode(valid)).outcome).toEqual(
			valid.outcome,
		);

		const omitted = hallDeficientAssignmentScene();
		(omitted.outcome as { neighbor_tasks: string[] }).neighbor_tasks = [];
		expect(() => decodeFlowCurrentSceneV9(encode(omitted))).toThrowError(
			/Hall witness is invalid/,
		);
	});

	it("rejects unknown fields and malformed exact numbers", () => {
		const unknown = validScene();
		unknown.future = true;
		expect(() => decodeFlowCurrentSceneV9(encode(unknown))).toThrowError(
			/unknown field future/,
		);

		const invalid = validScene();
		const graph = invalid.graph as { edges: Record<string, unknown>[] };
		if (graph.edges[0] === undefined)
			throw new Error("fixture edge is missing");
		graph.edges[0].capacity = "01";
		expect(() => decodeFlowCurrentSceneV9(encode(invalid))).toThrowError(
			/invalid edge/,
		);

		const emptyStepMeaning = validScene();
		(
			emptyStepMeaning.trace_steps as { operation_unit: string }
		).operation_unit = "";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(emptyStepMeaning)),
		).toThrowError(/contract is invalid/);
	});

	it("requires one valid common semantic header for every trace event", () => {
		const valid = validTraceScene();
		expect(() => decodeFlowCurrentSceneV9(encodeRaw(valid))).not.toThrow();
		const workObservation = structuredClone(valid);
		(
			workObservation.trace_event as { catalog_id: string; patch_count: number }
		).catalog_id = "edmonds-karp.scan-residual-arc.work-observation";
		(workObservation.trace_event as { patch_count: number }).patch_count = 0;
		expect(() =>
			decodeFlowCurrentSceneV9(encodeRaw(workObservation)),
		).toThrowError(/synthetic graphical work observation/u);

		const missing = validTraceScene();
		delete missing.trace_event_semantics;
		expect(() => decodeFlowCurrentSceneV9(encodeRaw(missing))).toThrowError(
			/common semantic header must appear together/,
		);

		const changedButUntouched = validTraceScene();
		(
			changedButUntouched.trace_event_semantics as {
				changed_entity_refs: unknown[];
			}
		).changed_entity_refs = [{ kind: "node", node_id: "s" }];
		expect(() =>
			decodeFlowCurrentSceneV9(encodeRaw(changedButUntouched)),
		).not.toThrow();

		for (const unknown of [
			{ kind: "node", node_id: "missing" },
			{ kind: "edge", edge_id: "missing" },
			{ kind: "residual-arc", edge_id: "missing", direction: "forward" },
		]) {
			const unknownChanged = validTraceScene();
			(
				unknownChanged.trace_event_semantics as {
					changed_entity_refs: unknown[];
				}
			).changed_entity_refs = [unknown];
			expect(() =>
				decodeFlowCurrentSceneV9(encodeRaw(unknownChanged)),
			).toThrowError(/changed entity does not match its graph/u);
		}

		const duplicateChanged = validTraceScene();
		(
			duplicateChanged.trace_event_semantics as {
				changed_entity_refs: unknown[];
			}
		).changed_entity_refs = [
			{ kind: "node", node_id: "s" },
			{ kind: "node", node_id: "s" },
		];
		expect(() =>
			decodeFlowCurrentSceneV9(encodeRaw(duplicateChanged)),
		).toThrowError(/focus and changed entities must each be unique/);

		const duplicateFocus = validTraceScene();
		(duplicateFocus.trace_event as { entity_refs: unknown[] }).entity_refs = [
			{ kind: "node", node_id: "s" },
			{ kind: "node", node_id: "s" },
		];
		expect(() =>
			decodeFlowCurrentSceneV9(encodeRaw(duplicateFocus)),
		).toThrowError(/focus and changed entities must each be unique/);

		const wrongAggregation = validTraceScene();
		(
			wrongAggregation.trace_event_semantics as {
				work_deltas: { unit: string; count: string }[];
			}
		).work_deltas[1] = { unit: "residual-arc-scan", count: "3" };
		expect(() =>
			decodeFlowCurrentSceneV9(encodeRaw(wrongAggregation)),
		).toThrowError(/semantic aggregation is invalid/);

		const wrongRole = validTraceScene();
		(wrongRole.trace_event_semantics as { role: string }).role = "certify";
		expect(() => decodeFlowCurrentSceneV9(encodeRaw(wrongRole))).toThrowError(
			/certify role does not match/,
		);

		const disabledBoundary = validTraceScene();
		(
			disabledBoundary.trace_steps as {
				detail: { availability: string; reason?: string };
			}
		).detail = {
			availability: "unavailable",
			reason: "This fixture intentionally disables Detail.",
		};
		expect(() =>
			decodeFlowCurrentSceneV9(encodeRaw(disabledBoundary)),
		).toThrowError(/boundary disabled by its step contract/);

		const terminalOnDisabledBoundary = validTraceScene();
		terminalOnDisabledBoundary.solve_status = "optimal";
		terminalOnDisabledBoundary.edge_states = [{ edge_id: "st", flow: "9" }];
		terminalOnDisabledBoundary.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "18446744073709551606",
				cost: "-7",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "9",
				cost: "7",
				active: false,
			},
		];
		terminalOnDisabledBoundary.outcome = {
			kind: "max-flow",
			value: "9",
			cut_bound: "9",
			source_side: ["s"],
		};
		(terminalOnDisabledBoundary.metrics as string[])[2] = "1";
		(
			terminalOnDisabledBoundary.trace_steps as {
				detail: { availability: string; reason?: string };
			}
		).detail = {
			availability: "unavailable",
			reason: "Terminal certification remains globally reachable.",
		};
		(
			terminalOnDisabledBoundary.trace_event_semantics as { role: string }
		).role = "certify";
		expect(() =>
			decodeFlowCurrentSceneV9(encodeRaw(terminalOnDisabledBoundary)),
		).not.toThrow();

		const incompleteTerminal = structuredClone(terminalOnDisabledBoundary);
		(
			incompleteTerminal.trace_event_semantics as {
				work_progress: {
					detail_completed: string;
					detail_total: string;
					primary_completed: string;
					primary_total: string;
				};
			}
		).work_progress = {
			detail_completed: "1",
			detail_total: "999",
			primary_completed: "0",
			primary_total: "999",
		};
		expect(() =>
			decodeFlowCurrentSceneV9(encodeRaw(incompleteTerminal)),
		).toThrowError(/must complete its declared metric-backed totals/);

		const mismatchedMetric = structuredClone(terminalOnDisabledBoundary);
		(mismatchedMetric.metrics as string[])[2] = "2";
		expect(() =>
			decodeFlowCurrentSceneV9(encodeRaw(mismatchedMetric)),
		).toThrowError(/must complete its declared metric-backed totals/);

		const infeasibleWithoutEvent = validScene();
		infeasibleWithoutEvent.event_id = "1";
		infeasibleWithoutEvent.event_count = "1";
		infeasibleWithoutEvent.solve_status = "infeasible";
		expect(() =>
			decodeFlowCurrentSceneV9(encodeRaw(infeasibleWithoutEvent)),
		).toThrowError(/trace event does not match/);
	});

	it("rejects metric-vector length or model-field drift", () => {
		const metrics = validScene();
		metrics.metrics = ["0"];
		expect(() => decodeFlowCurrentSceneV9(encode(metrics))).toThrowError(
			/metrics: array is too short/,
		);

		const model = validScene();
		model.model = {
			kind: "max-flow",
			source: "s",
			sink: "t",
			future: true,
		};
		expect(() => decodeFlowCurrentSceneV9(encode(model))).toThrowError(
			/structure mismatch.*model/,
		);
	});

	it("rejects residual-value drift and a missing current trace event", () => {
		const residual = validScene();
		const arcs = residual.residual_arcs as Record<string, unknown>[];
		if (arcs[0] === undefined) throw new Error("fixture arc is missing");
		arcs[0].capacity = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(residual))).toThrowError(
			/residual values are inconsistent/,
		);

		const trace = validScene();
		trace.event_id = "1";
		trace.event_count = "2";
		trace.solve_status = "running";
		expect(() => decodeFlowCurrentSceneV9(encode(trace))).toThrowError(
			/trace event does not match/,
		);
	});

	it("keeps exact algorithm-specific trace detail and rejects noncanonical values", () => {
		const traced = validScene();
		traced.event_id = "1";
		traced.event_count = "2";
		traced.solve_status = "running";
		traced.trace_event = {
			event_id: "1",
			catalog_id: "capacity-scaling-augmenting-path.search",
			minimum_granularity: "phase",
			pseudocode_line:
				"capacity-scaling-augmenting-path:search-delta-eligible-path",
			patch_count: 2,
			entity_refs: [],
			detail: { label: "delta", value: "8" },
		};
		expect(
			decodeFlowCurrentSceneV9(encode(traced)).trace_event?.detail,
		).toEqual({
			label: "delta",
			value: "8",
		});

		(traced.trace_event as { detail: { value: string } }).detail.value = "08";
		expect(() => decodeFlowCurrentSceneV9(encode(traced))).toThrowError(
			/invalid trace event detail/,
		);
	});

	it("validates prediction preprocessing, clipping, and closed configuration", () => {
		const valid = validScene();
		valid.model = {
			kind: "fixed-flow-min-cost",
			source: "s",
			sink: "t",
			required_flow: "0",
		};
		valid.algorithm = {
			id: "prediction-assisted-epsilon-relaxation",
			config: {
				predicted_potentials: { s: "-100", t: "100" },
				scaling_parameter: 2,
			},
		};
		valid.event_id = "1";
		valid.event_count = "4";
		valid.solve_status = "running";
		valid.trace_event = {
			event_id: "1",
			catalog_id:
				"prediction-assisted-epsilon-relaxation.preprocess-prediction",
			minimum_granularity: "phase",
			pseudocode_line:
				"prediction-assisted-epsilon-relaxation:shift-and-clip-prediction",
			patch_count: 1,
			entity_refs: [],
		};
		valid.prediction_assisted_epsilon_overlay = {
			stage: "preprocess-prediction",
			scaling_parameter: "2",
			attempt: "0",
			maximum_attempt: "5",
			exponent: "0",
			nodes: [
				{
					node_id: "s",
					raw_predicted_price: "-100",
					predicted_price: "0",
					prediction_clipped: false,
					price: "0",
					surplus: "0",
					active: false,
				},
				{
					node_id: "t",
					raw_predicted_price: "100",
					predicted_price: "7",
					prediction_clipped: true,
					price: "7",
					surplus: "0",
					active: false,
				},
			],
			edges: [{ edge_id: "st", scaled_cost: "-7" }],
		};

		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(
			decoded.prediction_assisted_epsilon_overlay?.nodes[1]?.prediction_clipped,
		).toBe(true);

		const corruptClip = structuredClone(valid);
		const corruptClipNode = (
			corruptClip.prediction_assisted_epsilon_overlay as {
				nodes: { prediction_clipped: boolean }[];
			}
		).nodes[1];
		if (corruptClipNode === undefined) throw new Error("missing clipped node");
		corruptClipNode.prediction_clipped = false;
		expect(() => decodeFlowCurrentSceneV9(encode(corruptClip))).toThrowError(
			/node values are inconsistent/,
		);

		const openConfig = structuredClone(valid);
		(
			openConfig.algorithm as { config: Record<string, unknown> }
		).config.fallback = true;
		expect(() => decodeFlowCurrentSceneV9(encode(openConfig))).toThrowError(
			/configuration is not closed/,
		);
	});

	it("validates Tardos epsilon, strict fixing threshold, and closed labels", () => {
		const valid = tardosFrameworkScene();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("primitive-complete");
		expect(decoded.tardos_framework_overlay?.fixed_variables).toEqual([
			expect.objectContaining({ edge_id: "expensive", bound: "lower" }),
		]);

		const measured = structuredClone(valid);
		measured.event_id = "2";
		measured.solve_status = "running";
		delete measured.outcome;
		const measuredEvent = measured.trace_event as {
			event_id: string;
			catalog_id: string;
		};
		measuredEvent.event_id = "2";
		measuredEvent.catalog_id = "tardos-framework.measure-epsilon";
		const measuredOverlay = measured.tardos_framework_overlay as {
			stage: string;
			residual_arcs: { fixes_variable: boolean }[];
			fixed_variables: unknown[];
		};
		measuredOverlay.stage = "measure-epsilon";
		for (const arc of measuredOverlay.residual_arcs) {
			arc.fixes_variable = false;
		}
		measuredOverlay.fixed_variables = [];
		expect(() => decodeFlowCurrentSceneV9(encode(measured))).not.toThrow();

		const scanned = structuredClone(measured);
		const scannedEvent = scanned.trace_event as {
			catalog_id: string;
			minimum_granularity: string;
		};
		scannedEvent.catalog_id = "tardos-framework.scan-residual-arc";
		scannedEvent.minimum_granularity = "micro";
		const scannedOverlay = scanned.tardos_framework_overlay as {
			epsilon: string;
			threshold: string;
			residual_arcs: unknown[];
		};
		scannedOverlay.epsilon = "1";
		scannedOverlay.threshold = "0";
		scannedOverlay.residual_arcs = scannedOverlay.residual_arcs.slice(0, 1);
		expect(() => decodeFlowCurrentSceneV9(encode(scanned))).not.toThrow();

		const inspected = structuredClone(valid);
		inspected.event_id = "6";
		inspected.event_count = "8";
		inspected.solve_status = "running";
		delete inspected.outcome;
		const inspectedEvent = inspected.trace_event as {
			event_id: string;
			catalog_id: string;
			minimum_granularity: string;
		};
		inspectedEvent.event_id = "6";
		inspectedEvent.catalog_id = "tardos-framework.inspect-fixed-variable";
		inspectedEvent.minimum_granularity = "micro";
		(inspected.tardos_framework_overlay as { stage: string }).stage =
			"classify-fixed-variables";
		expect(() => decodeFlowCurrentSceneV9(encode(inspected))).not.toThrow();

		const badThreshold = structuredClone(valid);
		(badThreshold.tardos_framework_overlay as { threshold: string }).threshold =
			"20";
		expect(() => decodeFlowCurrentSceneV9(encode(badThreshold))).toThrowError(
			/epsilon measurement is inconsistent/,
		);

		const falseWitness = structuredClone(valid);
		const witness = (
			falseWitness.tardos_framework_overlay as {
				fixed_variables: { direction: string }[];
			}
		).fixed_variables[0];
		if (witness === undefined) throw new Error("fixture witness is missing");
		witness.direction = "reverse";
		expect(() => decodeFlowCurrentSceneV9(encode(falseWitness))).toThrowError(
			/fixed-variable certificate differs/,
		);

		const openConfig = structuredClone(valid);
		(openConfig.algorithm as { config: Record<string, unknown> }).config.extra =
			true;
		expect(() => decodeFlowCurrentSceneV9(encode(openConfig))).toThrowError(
			/configuration is not closed/,
		);
	});

	it("validates electrical KCL, Ohm, energy, and exact-reference certificates", () => {
		const valid = electricalFlowScene();
		const ready = structuredClone(valid);
		ready.event_id = "0";
		ready.event_count = "0";
		ready.solve_status = "ready";
		delete ready.trace_event;
		delete ready.outcome;
		delete ready.electrical_flow_overlay;
		ready.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "2",
				cost: "0",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
		];
		expect(() => decodeFlowCurrentSceneV9(encode(ready))).not.toThrow();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("primitive-complete");
		expect(decoded.electrical_flow_overlay).toEqual(
			expect.objectContaining({
				stage: "complete",
				effective_resistance: "0.25",
				total_energy: "0.25",
				converged: true,
			}),
		);
		expect(decoded.outcome).toEqual(
			expect.objectContaining({
				kind: "electrical-flow",
				exact_effective_resistance: { numerator: "1", denominator: "4" },
			}),
		);

		const badCurrent = structuredClone(valid);
		const badCurrentEdge = (
			badCurrent.electrical_flow_overlay as {
				edges: { current: string }[];
			}
		).edges[0];
		if (badCurrentEdge === undefined)
			throw new Error("missing electrical edge");
		badCurrentEdge.current = "0.9";
		expect(() => decodeFlowCurrentSceneV9(encode(badCurrent))).toThrowError(
			/Ohm or energy law/,
		);

		const badEnergy = structuredClone(valid);
		(
			badEnergy.electrical_flow_overlay as {
				total_energy: string;
			}
		).total_energy = "0.2";
		expect(() => decodeFlowCurrentSceneV9(encode(badEnergy))).toThrowError(
			/energy\/effective-resistance identity/,
		);

		const badGround = structuredClone(valid);
		const badGroundNode = (
			badGround.electrical_flow_overlay as {
				nodes: { grounded: boolean }[];
			}
		).nodes[0];
		if (badGroundNode === undefined) throw new Error("missing electrical node");
		badGroundNode.grounded = true;
		expect(() => decodeFlowCurrentSceneV9(encode(badGround))).toThrowError(
			/stable identities or ground/,
		);

		const badExact = structuredClone(valid);
		(
			badExact.electrical_flow_overlay as {
				exact_effective_resistance: { numerator: string; denominator: string };
			}
		).exact_effective_resistance = { numerator: "1", denominator: "5" };
		expect(() => decodeFlowCurrentSceneV9(encode(badExact))).toThrowError(
			/exact reference certificate/,
		);

		const nonFinite = structuredClone(valid);
		const nonFiniteNode = (
			nonFinite.electrical_flow_overlay as {
				nodes: { potential: string }[];
			}
		).nodes[0];
		if (nonFiniteNode === undefined) throw new Error("missing electrical node");
		nonFiniteNode.potential = "NaN";
		expect(() => decodeFlowCurrentSceneV9(encode(nonFinite))).toThrowError(
			/invalid finite decimal/,
		);

		const wrongAlgorithm = structuredClone(valid);
		wrongAlgorithm.algorithm = { id: "edmonds-karp", config: {} };
		wrongAlgorithm.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "2",
				cost: "0",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
		];
		expect(() => decodeFlowCurrentSceneV9(encode(wrongAlgorithm))).toThrowError(
			/wrong algorithm/,
		);
	});

	it("validates augmenting-electrical reductions, target cut, decimals, and final flow", () => {
		const valid = augmentingElectricalScene();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("optimal");
		expect(decoded.augmenting_electrical_overlay).toEqual(
			expect.objectContaining({
				stage: "optimal",
				original_target: "2",
				transformed_target: "6",
				working_target: "18",
			}),
		);

		const cleanup = structuredClone(valid);
		cleanup.event_id = "14";
		cleanup.solve_status = "running";
		cleanup.edge_states = [{ edge_id: "st", flow: "0" }];
		cleanup.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "2",
				cost: "0",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
		];
		(cleanup.trace_event as { catalog_id: string; event_id: string }) = {
			...(cleanup.trace_event as { catalog_id: string; event_id: string }),
			catalog_id: "augmenting-electrical-flow.cleanup-augmenting-path",
			event_id: "14",
		};
		const cleanupOverlay = cleanup.augmenting_electrical_overlay as {
			active_discrete_amount?: string;
			active_working_path: Array<{
				direction: "forward" | "reverse";
				edge: string;
				flow_after: string;
				from_node: string;
				to_node: string;
			}>;
			edges: Array<{
				extraction_central_scaled?: string;
				extraction_out_of_sink?: string;
				extraction_toward_source?: string;
				final_flow?: string;
			}>;
			stage: string;
		};
		cleanupOverlay.stage = "cleanup-augmenting-path";
		cleanupOverlay.active_working_path = [
			{
				edge: "0",
				direction: "forward",
				from_node: "s",
				to_node: "t",
				flow_after: "2",
			},
		];
		cleanupOverlay.active_discrete_amount = "2";
		delete cleanupOverlay.edges[0]?.final_flow;
		delete cleanupOverlay.edges[0]?.extraction_central_scaled;
		delete cleanupOverlay.edges[0]?.extraction_toward_source;
		delete cleanupOverlay.edges[0]?.extraction_out_of_sink;
		delete cleanup.outcome;
		expect(() => decodeFlowCurrentSceneV9(encode(cleanup))).not.toThrow();

		const disconnectedCleanup = structuredClone(cleanup);
		const disconnectedArc = (
			disconnectedCleanup.augmenting_electrical_overlay as {
				active_working_path: { to_node: string }[];
			}
		).active_working_path[0];
		if (disconnectedArc === undefined) {
			throw new Error("missing disconnected cleanup test arc");
		}
		disconnectedArc.to_node = "s";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(disconnectedCleanup)),
		).toThrowError(/not a connected source-to-sink path/);

		const eliminationPivot = structuredClone(valid);
		eliminationPivot.event_id = "5";
		eliminationPivot.solve_status = "running";
		eliminationPivot.edge_states = [{ edge_id: "st", flow: "0" }];
		eliminationPivot.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "2",
				cost: "0",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
		];
		(eliminationPivot.trace_event as { catalog_id: string; event_id: string }) =
			{
				...(eliminationPivot.trace_event as {
					catalog_id: string;
					event_id: string;
				}),
				catalog_id: "augmenting-electrical-flow.elimination-pivot",
				event_id: "5",
			};
		(
			eliminationPivot.augmenting_electrical_overlay as {
				edges: Array<{
					extraction_central_scaled?: string;
					extraction_out_of_sink?: string;
					extraction_toward_source?: string;
					final_flow?: string;
					rounded_central_flow?: string;
				}>;
				stage: string;
			}
		).stage = "solve-electrical-direction";
		delete (
			eliminationPivot.augmenting_electrical_overlay as {
				edges: {
					final_flow?: string;
					rounded_central_flow?: string;
				}[];
			}
		).edges[0]?.final_flow;
		delete (
			eliminationPivot.augmenting_electrical_overlay as {
				edges: { rounded_central_flow?: string }[];
			}
		).edges[0]?.rounded_central_flow;
		const eliminationEdge = (
			eliminationPivot.augmenting_electrical_overlay as {
				edges: Array<{
					extraction_central_scaled?: string;
					extraction_out_of_sink?: string;
					extraction_toward_source?: string;
				}>;
			}
		).edges[0];
		if (eliminationEdge === undefined) {
			throw new Error("missing elimination edge state");
		}
		delete eliminationEdge.extraction_central_scaled;
		delete eliminationEdge.extraction_toward_source;
		delete eliminationEdge.extraction_out_of_sink;
		delete eliminationPivot.outcome;
		expect(() =>
			decodeFlowCurrentSceneV9(encode(eliminationPivot)),
		).not.toThrow();

		const missingRoundedFlow = structuredClone(valid);
		delete (
			missingRoundedFlow.augmenting_electrical_overlay as {
				edges: { rounded_central_flow?: string }[];
			}
		).edges[0]?.rounded_central_flow;
		expect(() =>
			decodeFlowCurrentSceneV9(encode(missingRoundedFlow)),
		).toThrowError(/edge flow or boost metadata is inconsistent/);

		const partialExtraction = structuredClone(valid);
		delete (
			partialExtraction.augmenting_electrical_overlay as {
				edges: { extraction_toward_source?: string }[];
			}
		).edges[0]?.extraction_toward_source;
		expect(() =>
			decodeFlowCurrentSceneV9(encode(partialExtraction)),
		).toThrowError(/edge flow or boost metadata is inconsistent/);

		const reductionOnly = structuredClone(eliminationPivot);
		reductionOnly.event_id = "1";
		(reductionOnly.trace_event as {
			catalog_id: string;
			event_id: string;
		}) = {
			...(reductionOnly.trace_event as {
				catalog_id: string;
				event_id: string;
			}),
			catalog_id: "augmenting-electrical-flow.build-directed-reduction",
			event_id: "1",
		};
		delete (
			reductionOnly.trace_event as {
				parent_phase_id?: string;
			}
		).parent_phase_id;
		const reductionOverlay = reductionOnly.augmenting_electrical_overlay as {
			stage: string;
			working_target: string;
			current_value: string;
			alpha: string;
			remaining: string;
			nodes: { target_source_side: boolean }[];
		};
		reductionOverlay.stage = "build-directed-reduction";
		reductionOverlay.working_target = "0";
		reductionOverlay.current_value = "0";
		reductionOverlay.alpha = "0";
		reductionOverlay.remaining = "0";
		for (const node of reductionOverlay.nodes) {
			node.target_source_side = false;
		}
		expect(() => decodeFlowCurrentSceneV9(encode(reductionOnly))).not.toThrow();
		const firstReductionNode = reductionOverlay.nodes[0];
		if (firstReductionNode === undefined) {
			throw new Error("missing augmenting-electrical reduction node");
		}
		firstReductionNode.target_source_side = true;
		expect(() => decodeFlowCurrentSceneV9(encode(reductionOnly))).toThrowError(
			/target cut appears before its install boundary/,
		);

		const ready = structuredClone(valid);
		ready.event_id = "0";
		ready.event_count = "0";
		ready.solve_status = "ready";
		ready.edge_states = [{ edge_id: "st", flow: "0" }];
		ready.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "2",
				cost: "0",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
		];
		delete ready.trace_event;
		delete ready.outcome;
		delete ready.augmenting_electrical_overlay;
		expect(() => decodeFlowCurrentSceneV9(encode(ready))).not.toThrow();

		const badCut = structuredClone(valid);
		const badCutSource = (
			badCut.augmenting_electrical_overlay as {
				nodes: { target_source_side: boolean }[];
			}
		).nodes[0];
		if (badCutSource === undefined) throw new Error("missing cut node");
		badCutSource.target_source_side = false;
		expect(() => decodeFlowCurrentSceneV9(encode(badCut))).toThrowError(
			/target cut has invalid terminal membership/,
		);

		const badWorkingTarget = structuredClone(valid);
		(
			badWorkingTarget.augmenting_electrical_overlay as {
				working_target: string;
			}
		).working_target = "17";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(badWorkingTarget)),
		).toThrowError(/transformed targets are inconsistent/);

		const badFlow = structuredClone(valid);
		const badFlowEdge = (
			badFlow.augmenting_electrical_overlay as {
				edges: { final_flow?: string }[];
			}
		).edges[0];
		if (badFlowEdge === undefined) throw new Error("missing final-flow edge");
		badFlowEdge.final_flow = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(badFlow))).toThrowError(
			/edge flow or boost metadata is inconsistent/,
		);

		const nonFinite = structuredClone(valid);
		const nonFiniteNode = (
			nonFinite.augmenting_electrical_overlay as {
				nodes: { potential: string }[];
			}
		).nodes[0];
		if (nonFiniteNode === undefined) throw new Error("missing potential node");
		nonFiniteNode.potential = "Infinity";
		expect(() => decodeFlowCurrentSceneV9(encode(nonFinite))).toThrowError(
			/invalid finite decimal/,
		);

		const wrongAlgorithm = structuredClone(valid);
		wrongAlgorithm.algorithm = { id: "edmonds-karp", config: {} };
		expect(() => decodeFlowCurrentSceneV9(encode(wrongAlgorithm))).toThrowError(
			/wrong algorithm/,
		);
	});

	it("validates interior-point reductions, centrality, unit domain, and b-matching-recovered flow", () => {
		const valid = interiorPointMaxFlowScene();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("optimal");
		expect(decoded.interior_point_max_flow_overlay).toEqual(
			expect.objectContaining({
				stage: "optimal",
				target_value: "1",
				b_matching_nodes: "4",
				working_edges: "9",
			}),
		);

		const ready = structuredClone(valid);
		ready.event_id = "0";
		ready.event_count = "0";
		ready.solve_status = "ready";
		ready.edge_states = [{ edge_id: "st", flow: "0" }];
		ready.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "1",
				cost: "0",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
		];
		delete ready.trace_event;
		delete ready.outcome;
		delete ready.interior_point_max_flow_overlay;
		expect(() => decodeFlowCurrentSceneV9(encode(ready))).not.toThrow();

		const badCut = structuredClone(valid);
		const sourceState = (
			badCut.interior_point_max_flow_overlay as {
				nodes: { target_source_side: boolean }[];
			}
		).nodes[0];
		if (sourceState === undefined) throw new Error("missing source state");
		sourceState.target_source_side = false;
		expect(() => decodeFlowCurrentSceneV9(encode(badCut))).toThrowError(
			/target cut has invalid terminals/,
		);

		const badReduction = structuredClone(valid);
		(
			badReduction.interior_point_max_flow_overlay as {
				working_edges: string;
			}
		).working_edges = "8";
		expect(() => decodeFlowCurrentSceneV9(encode(badReduction))).toThrowError(
			/reduction sizes are inconsistent/,
		);

		const badCentrality = structuredClone(valid);
		(
			badCentrality.interior_point_max_flow_overlay as {
				centrality: string;
			}
		).centrality = "0.1";
		expect(() => decodeFlowCurrentSceneV9(encode(badCentrality))).toThrowError(
			/centrality or duality gap is inconsistent/,
		);

		const badFlow = structuredClone(valid);
		const edge = (
			badFlow.interior_point_max_flow_overlay as {
				edges: { final_flow?: string }[];
			}
		).edges[0];
		if (edge === undefined) throw new Error("missing edge state");
		edge.final_flow = "0";
		expect(() => decodeFlowCurrentSceneV9(encode(badFlow))).toThrowError(
			/edge state or terminal flow is inconsistent/,
		);

		const nonFinite = structuredClone(valid);
		const node = (
			nonFinite.interior_point_max_flow_overlay as {
				nodes: { potential: string }[];
			}
		).nodes[0];
		if (node === undefined) throw new Error("missing node state");
		node.potential = "NaN";
		expect(() => decodeFlowCurrentSceneV9(encode(nonFinite))).toThrowError(
			/invalid finite decimal/,
		);

		const nonUnit = structuredClone(valid);
		const graphEdge = (nonUnit.graph as { edges: { cost: string }[] }).edges[0];
		if (graphEdge === undefined) throw new Error("missing graph edge");
		graphEdge.cost = "1";
		const [forward, reverse] = nonUnit.residual_arcs as {
			cost: string;
		}[];
		if (forward === undefined || reverse === undefined) {
			throw new Error("missing residual pair");
		}
		forward.cost = "1";
		reverse.cost = "-1";
		expect(() => decodeFlowCurrentSceneV9(encode(nonUnit))).toThrowError(
			/bounded unit domain/,
		);
	});

	it("validates minimum-ratio-cycle mapping, forest, exact ratio, and cycle-space certificate", () => {
		const valid = minimumRatioCycleScene();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("primitive-complete");
		expect(decoded.minimum_ratio_cycle_overlay).toEqual(
			expect.objectContaining({
				stage: "complete",
				best_ratio: { numerator: "-2", denominator: "1" },
				fundamental_cycles: "1",
			}),
		);

		const ready = structuredClone(valid);
		ready.event_id = "0";
		ready.event_count = "0";
		ready.solve_status = "ready";
		delete ready.trace_event;
		delete ready.outcome;
		delete ready.minimum_ratio_cycle_overlay;
		expect(() => decodeFlowCurrentSceneV9(encode(ready))).not.toThrow();

		const badRatio = structuredClone(valid);
		(
			badRatio.minimum_ratio_cycle_overlay as {
				best_ratio: { numerator: string; denominator: string };
			}
		).best_ratio = { numerator: "-1", denominator: "1" };
		expect(() => decodeFlowCurrentSceneV9(encode(badRatio))).toThrowError(
			/ratio or circulation is inconsistent/,
		);

		const badForest = structuredClone(valid);
		const forestEdge = (
			badForest.minimum_ratio_cycle_overlay as {
				edges: { tree_edge: boolean }[];
			}
		).edges[0];
		if (forestEdge === undefined) throw new Error("missing forest edge");
		forestEdge.tree_edge = false;
		expect(() => decodeFlowCurrentSceneV9(encode(badForest))).toThrowError(
			/edge objective is inconsistent/,
		);

		const badBalance = structuredClone(valid);
		const balanceNode = (
			badBalance.minimum_ratio_cycle_overlay as {
				nodes: { candidate_balance: string }[];
			}
		).nodes[0];
		if (balanceNode === undefined) throw new Error("missing balance node");
		balanceNode.candidate_balance = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(badBalance))).toThrowError(
			/node\/forest state is inconsistent/,
		);

		const badOutcome = structuredClone(valid);
		const outcomeArc = (badOutcome.outcome as { cycle: { sign: string }[] })
			.cycle[0];
		if (outcomeArc === undefined) throw new Error("missing outcome arc");
		outcomeArc.sign = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(badOutcome))).toThrowError(
			/completion lacks its exact outcome/,
		);

		const badLength = structuredClone(valid);
		const graphEdge = (badLength.graph as { edges: { capacity: string }[] })
			.edges[0];
		if (graphEdge === undefined) throw new Error("missing graph edge");
		graphEdge.capacity = "0";
		expect(() => decodeFlowCurrentSceneV9(encode(badLength))).toThrowError(
			/bounded domain/,
		);
	});

	it("validates weighted augmenting-path prefixes, hierarchy weights, phi, cut, and outcome", () => {
		const valid = weightedAugmentingPathsScene();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("optimal");
		expect(decoded.weighted_augmenting_paths_overlay).toEqual(
			expect.objectContaining({
				stage: "optimal",
				phase: "1",
				phase_count: "2",
				active_path: [],
			}),
		);

		const ready = structuredClone(valid);
		ready.event_id = "0";
		ready.event_count = "0";
		ready.solve_status = "ready";
		ready.edge_states = [{ edge_id: "st", flow: "0" }];
		ready.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "2",
				cost: "0",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
		];
		ready.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
		delete ready.trace_event;
		delete ready.outcome;
		delete ready.weighted_augmenting_paths_overlay;
		expect(() => decodeFlowCurrentSceneV9(encode(ready))).not.toThrow();

		const assigned = structuredClone(valid);
		assigned.event_id = "5";
		assigned.solve_status = "running";
		assigned.edge_states = [{ edge_id: "st", flow: "0" }];
		assigned.residual_arcs = structuredClone(ready.residual_arcs);
		delete assigned.outcome;
		(assigned.trace_event as Record<string, unknown>).event_id = "5";
		(assigned.trace_event as Record<string, unknown>).catalog_id =
			"weighted-augmenting-paths.assign-weights";
		(assigned.trace_event as Record<string, unknown>).detail = {
			label: "height",
			value: "2",
		};
		const assignedOverlay = assigned.weighted_augmenting_paths_overlay as {
			stage: string;
			phase: string;
			capacity_bit: string;
			height: string;
			phi_numerator: string;
			nodes: {
				component: string;
				order: string;
				source_side: boolean;
			}[];
			edges: { scaled_capacity: string; flow: string }[];
			residual_arcs: {
				capacity: string;
				hierarchy_kind?: string;
				weight: string;
			}[];
		};
		assignedOverlay.stage = "assign-weights";
		assignedOverlay.phase = "0";
		assignedOverlay.capacity_bit = "1";
		assignedOverlay.height = "2";
		assignedOverlay.phi_numerator = "1";
		const assignedSource = assignedOverlay.nodes[0];
		const assignedSink = assignedOverlay.nodes[1];
		const assignedEdge = assignedOverlay.edges[0];
		const assignedForward = assignedOverlay.residual_arcs[0];
		const assignedReverse = assignedOverlay.residual_arcs[1];
		if (
			assignedSource === undefined ||
			assignedSink === undefined ||
			assignedEdge === undefined ||
			assignedForward === undefined ||
			assignedReverse === undefined
		) {
			throw new Error("weighted augmenting-path fixture is incomplete");
		}
		assignedSource.component = "0";
		assignedSource.order = "1";
		assignedSource.source_side = true;
		assignedSink.component = "1";
		assignedSink.order = "2";
		assignedSink.source_side = true;
		assignedEdge.scaled_capacity = "1";
		assignedEdge.flow = "0";
		assignedForward.capacity = "1";
		assignedForward.hierarchy_kind = "dag";
		assignedForward.weight = "1";
		assignedReverse.capacity = "0";
		assignedReverse.hierarchy_kind = "dag";
		assignedReverse.weight = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(assigned))).not.toThrow();

		const badScaled = structuredClone(valid);
		const badScaledEdge = (
			badScaled.weighted_augmenting_paths_overlay as {
				edges: { scaled_capacity: string }[];
			}
		).edges[0];
		if (badScaledEdge === undefined) throw new Error("missing prefix edge");
		badScaledEdge.scaled_capacity = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(badScaled))).toThrowError(
			/prefix edge is inconsistent/,
		);

		const badResidual = structuredClone(valid);
		const badResidualArc = (
			badResidual.weighted_augmenting_paths_overlay as {
				residual_arcs: { capacity: string }[];
			}
		).residual_arcs[1];
		if (badResidualArc === undefined) throw new Error("missing residual arc");
		badResidualArc.capacity = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(badResidual))).toThrowError(
			/residual projection is inconsistent/,
		);

		const badWeight = structuredClone(assigned);
		const badWeightArc = (
			badWeight.weighted_augmenting_paths_overlay as {
				residual_arcs: { weight: string }[];
			}
		).residual_arcs[0];
		if (badWeightArc === undefined) throw new Error("missing weighted arc");
		badWeightArc.weight = "2";
		expect(() => decodeFlowCurrentSceneV9(encode(badWeight))).toThrowError(
			/hierarchy weight is inconsistent/,
		);

		const badPhi = structuredClone(assigned);
		(
			badPhi.weighted_augmenting_paths_overlay as { phi_numerator: string }
		).phi_numerator = "2";
		expect(() => decodeFlowCurrentSceneV9(encode(badPhi))).toThrowError(
			/phi certificate is inconsistent/,
		);

		const badOutcome = structuredClone(valid);
		(badOutcome.outcome as { value: string }).value = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(badOutcome))).toThrowError(
			/outcome is inconsistent|maximum-flow outcome|exact certificate/,
		);
	});

	it("validates weighted push-relabel shortcut stars, residuals, and exact cut", () => {
		const valid = weightedPushRelabelShortcutScene();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("optimal");
		expect(decoded.weighted_push_relabel_shortcut_overlay).toEqual(
			expect.objectContaining({
				stage: "optimal",
				hierarchy_levels: "1",
				psi_numerator: "1",
				shortcut_traversals: "2",
				active_path: [],
			}),
		);

		const ready = structuredClone(valid);
		ready.event_id = "0";
		ready.event_count = "0";
		ready.solve_status = "ready";
		ready.edge_states = (
			ready.edge_states as { edge_id: string; flow: string }[]
		).map((state) => ({ ...state, flow: "0" }));
		ready.residual_arcs = (
			ready.graph as {
				edges: { id: string; from: string; to: string; capacity: string }[];
			}
		).edges.flatMap((edge) => [
			{
				edge_id: edge.id,
				direction: "forward",
				from: edge.from,
				to: edge.to,
				capacity: edge.capacity,
				cost: "0",
				active: false,
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
		]);
		delete ready.trace_event;
		delete ready.outcome;
		delete ready.weighted_push_relabel_shortcut_overlay;
		expect(() => decodeFlowCurrentSceneV9(encode(ready))).not.toThrow();

		const badOriginalWeight = structuredClone(valid);
		const original = (
			badOriginalWeight.weighted_push_relabel_shortcut_overlay as {
				edges: { weight: string }[];
			}
		).edges[0];
		if (original === undefined) throw new Error("missing original edge");
		original.weight = "2";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(badOriginalWeight)),
		).toThrowError(/original-edge projection is inconsistent/);

		const badShortcutWeight = structuredClone(valid);
		const shortcut = (
			badShortcutWeight.weighted_push_relabel_shortcut_overlay as {
				edges: { weight: string }[];
			}
		).edges[6];
		if (shortcut === undefined) throw new Error("missing shortcut edge");
		shortcut.weight = "3";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(badShortcutWeight)),
		).toThrowError(/shortcut star is inconsistent/);

		const badResidual = structuredClone(valid);
		const residual = (
			badResidual.weighted_push_relabel_shortcut_overlay as {
				residual_arcs: { capacity: string }[];
			}
		).residual_arcs[13];
		if (residual === undefined) throw new Error("missing shortcut residual");
		residual.capacity = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(badResidual))).toThrowError(
			/residual identity\/capacity is inconsistent/,
		);

		const inspected = structuredClone(valid);
		inspected.event_id = "5";
		inspected.solve_status = "running";
		delete inspected.outcome;
		inspected.trace_event = {
			event_id: "5",
			catalog_id: "weighted-push-relabel.inspect-primitive-arc-checkpoint",
			minimum_granularity: "micro",
			pseudocode_line: "inspect one primitive residual arc",
			patch_count: 1,
			entity_refs: [
				{ kind: "residual-arc", edge_id: "ab", direction: "forward" },
			],
			detail: { label: "residual capacity", value: "5" },
		};
		const inspectedOverlay =
			inspected.weighted_push_relabel_shortcut_overlay as {
				stage: string;
				inspected_arcs: {
					edge_id: string;
					direction: "forward" | "reverse";
				}[];
				residual_arcs: {
					edge_id: string;
					direction: string;
					active: boolean;
				}[];
			};
		inspectedOverlay.stage = "inspect-primitive-arc-checkpoint";
		inspectedOverlay.inspected_arcs = [{ edge_id: "ab", direction: "forward" }];
		const inspectedArc = inspectedOverlay.residual_arcs.find(
			(arc) => arc.edge_id === "ab" && arc.direction === "forward",
		);
		if (inspectedArc === undefined) throw new Error("missing inspected arc");
		inspectedArc.active = true;
		expect(() => decodeFlowCurrentSceneV9(encode(inspected))).not.toThrow();

		const persistentPath = structuredClone(valid);
		persistentPath.event_id = "17";
		persistentPath.solve_status = "running";
		delete persistentPath.outcome;
		persistentPath.trace_event = {
			event_id: "17",
			catalog_id: "weighted-push-relabel.completion-augment-path",
			minimum_granularity: "operation",
			pseudocode_line: "augment one exact original-residual path",
			patch_count: 2,
			entity_refs: [
				{ kind: "residual-arc", edge_id: "sb", direction: "forward" },
				{ kind: "residual-arc", edge_id: "bt", direction: "forward" },
			],
			detail: { label: "bottleneck", value: "1" },
		};
		const persistentOverlay =
			persistentPath.weighted_push_relabel_shortcut_overlay as {
				stage: string;
				active_bottleneck: string;
				active_path: { edge_id: string; direction: "forward" | "reverse" }[];
				nodes: { node_id: string; order: string; label: string }[];
				edges: {
					edge_id: string;
					kind: "original" | "shortcut";
					from: string;
					to: string;
					capacity: string;
					flow: string;
					weight: string;
				}[];
				residual_arcs: {
					edge_id: string;
					direction: "forward" | "reverse";
					capacity: string;
					weight: string;
					admissible: boolean;
					active: boolean;
				}[];
			};
		persistentOverlay.stage = "completion-augment-path";
		persistentOverlay.active_bottleneck = "1";
		persistentOverlay.active_path = [
			{ edge_id: "sb", direction: "forward" },
			{ edge_id: "bt", direction: "forward" },
		];
		const orderByNode = new Map([
			["a", "2"],
			["b", "5"],
			["s", "1"],
			["t", "6"],
		]);
		const labelByNode = new Map([
			["a", "6"],
			["b", "6"],
			["s", "12"],
			["t", "0"],
		]);
		for (const node of persistentOverlay.nodes) {
			node.order = orderByNode.get(node.node_id) ?? node.order;
			node.label = labelByNode.get(node.node_id) ?? node.label;
		}
		const flowByEdge = new Map([
			["sb", "3"],
			["bt", "5"],
		]);
		for (const edge of persistentOverlay.edges) {
			edge.flow = flowByEdge.get(edge.edge_id) ?? edge.flow;
			if (edge.kind === "original") {
				const from = BigInt(orderByNode.get(edge.from) ?? "0");
				const to = BigInt(orderByNode.get(edge.to) ?? "0");
				edge.weight = (from >= to ? from - to : to - from).toString();
			}
		}
		const augmentedById = new Map(
			persistentOverlay.edges.map((edge) => [edge.edge_id, edge]),
		);
		const activeKeys = new Set(["sb:forward", "bt:forward"]);
		for (const arc of persistentOverlay.residual_arcs) {
			const edge = augmentedById.get(arc.edge_id);
			if (edge === undefined) throw new Error("missing augmented edge");
			arc.capacity =
				arc.direction === "forward"
					? (BigInt(edge.capacity) - BigInt(edge.flow)).toString()
					: edge.flow;
			arc.weight = edge.weight;
			arc.active = activeKeys.has(`${arc.edge_id}:${arc.direction}`);
			arc.admissible = arc.active && BigInt(arc.capacity) > 0n;
		}
		persistentPath.edge_states = (
			persistentPath.edge_states as { edge_id: string; flow: string }[]
		).map((state) => ({
			...state,
			flow: flowByEdge.get(state.edge_id) ?? state.flow,
		}));
		persistentPath.residual_arcs = (
			persistentPath.graph as {
				edges: {
					id: string;
					from: string;
					to: string;
					capacity: string;
				}[];
			}
		).edges.flatMap((edge) => {
			const flow = BigInt(
				flowByEdge.get(edge.id) ?? augmentedById.get(edge.id)?.flow ?? "0",
			);
			return [
				{
					edge_id: edge.id,
					direction: "forward",
					from: edge.from,
					to: edge.to,
					capacity: (BigInt(edge.capacity) - flow).toString(),
					cost: "0",
					active: activeKeys.has(`${edge.id}:forward`),
				},
				{
					edge_id: edge.id,
					direction: "reverse",
					from: edge.to,
					to: edge.from,
					capacity: flow.toString(),
					cost: "0",
					active: false,
				},
			];
		});
		expect(() =>
			decodeFlowCurrentSceneV9(encode(persistentPath)),
		).not.toThrow();

		const unflaggedInspection = structuredClone(inspected);
		const unflaggedArc = (
			unflaggedInspection.weighted_push_relabel_shortcut_overlay as {
				residual_arcs: {
					edge_id: string;
					direction: string;
					active: boolean;
				}[];
			}
		).residual_arcs.find(
			(arc) => arc.edge_id === "ab" && arc.direction === "forward",
		);
		if (unflaggedArc === undefined) throw new Error("missing inspected arc");
		unflaggedArc.active = false;
		expect(() =>
			decodeFlowCurrentSceneV9(encode(unflaggedInspection)),
		).toThrowError(/active path flags are inconsistent/);

		const badOutcome = structuredClone(valid);
		(badOutcome.outcome as { cut_bound: string }).cut_bound = "11";
		expect(() => decodeFlowCurrentSceneV9(encode(badOutcome))).toThrowError(
			/exact certificate/,
		);
	});

	it("validates minimum-ratio-cycle MCF potential, source map, step, and progress bound", () => {
		const valid = minimumRatioCycleMcfScene();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("primitive-complete");
		expect(decoded.minimum_ratio_cycle_mcf_overlay).toEqual(
			expect.objectContaining({
				stage: "complete",
				best_ratio: "-14.995471605326076",
				fundamental_cycles: "1",
				stationary: false,
			}),
		);

		const ready = structuredClone(valid);
		ready.event_id = "0";
		ready.event_count = "0";
		ready.solve_status = "ready";
		delete ready.trace_event;
		delete ready.outcome;
		delete ready.minimum_ratio_cycle_mcf_overlay;
		expect(() => decodeFlowCurrentSceneV9(encode(ready))).not.toThrow();

		const badGradient = structuredClone(valid);
		const gradientEdge = (
			badGradient.minimum_ratio_cycle_mcf_overlay as {
				edges: { gradient: string }[];
			}
		).edges[0];
		if (gradientEdge === undefined) throw new Error("missing gradient edge");
		gradientEdge.gradient = "27";
		expect(() => decodeFlowCurrentSceneV9(encode(badGradient))).toThrowError(
			/source map is inconsistent/,
		);

		const badStep = structuredClone(valid);
		const stepEdge = (
			badStep.minimum_ratio_cycle_mcf_overlay as {
				edges: { updated_flow: string }[];
			}
		).edges[0];
		if (stepEdge === undefined) throw new Error("missing step edge");
		stepEdge.updated_flow = "0.6";
		expect(() => decodeFlowCurrentSceneV9(encode(badStep))).toThrowError(
			/objective gap is inconsistent|source step is inconsistent/,
		);

		const badDecrease = structuredClone(valid);
		(
			badDecrease.minimum_ratio_cycle_mcf_overlay as {
				potential_decrease: string;
			}
		).potential_decrease = "0.0001";
		expect(() => decodeFlowCurrentSceneV9(encode(badDecrease))).toThrowError(
			/progress bound is inconsistent/,
		);

		const badRatio = structuredClone(valid);
		(
			badRatio.minimum_ratio_cycle_mcf_overlay as { best_ratio: string }
		).best_ratio = "-1";
		expect(() => decodeFlowCurrentSceneV9(encode(badRatio))).toThrowError(
			/selected ratio is inconsistent/,
		);

		const badOutcome = structuredClone(valid);
		const outcomeArc = (badOutcome.outcome as { cycle: { sign: string }[] })
			.cycle[0];
		if (outcomeArc === undefined) throw new Error("missing outcome arc");
		outcomeArc.sign = "-1";
		expect(() => decodeFlowCurrentSceneV9(encode(badOutcome))).toThrowError(
			/completion lacks its outcome/,
		);
	});

	it("accepts a stationary active MCF face without inventing source-map or forest state", () => {
		const stationary = minimumRatioCycleMcfScene();
		stationary.event_id = "5";
		stationary.event_count = "5";
		(stationary.trace_event as Record<string, unknown>).event_id = "5";
		(stationary.trace_event as Record<string, unknown>).parent_phase_id = "4";
		(stationary.trace_event as Record<string, unknown>).detail = {
			label: "source steps",
			value: "0",
		};
		const graph = stationary.graph as { edges: { cost: string }[] };
		const secondEdge = graph.edges[1];
		if (secondEdge === undefined) throw new Error("missing second edge");
		secondEdge.cost = "1";
		const overlay = stationary.minimum_ratio_cycle_mcf_overlay as {
			alpha: string;
			initial_cost: string;
			current_cost: string;
			cost_gap: string;
			potential_before: string;
			current_potential: string;
			best_ratio?: string;
			kappa: string;
			eta: string;
			weighted_step_norm: string;
			potential_decrease: string;
			guaranteed_decrease: string;
			stationary: boolean;
			enumerated_vectors: string;
			simple_cycles: string;
			fundamental_cycles: string;
			selected_edge_count: string;
			nodes: {
				component: string;
				parent_node_id?: string;
				depth: string;
				on_selected: boolean;
			}[];
			edges: {
				updated_flow: string;
				gradient: string;
				length: string;
				tree_edge: boolean;
				selected_sign: string;
			}[];
		};
		overlay.initial_cost = "1";
		overlay.current_cost = "1";
		overlay.cost_gap = "0";
		overlay.potential_before = "0";
		overlay.current_potential = "0";
		delete overlay.best_ratio;
		overlay.kappa = "0";
		overlay.eta = "0";
		overlay.weighted_step_norm = "0";
		overlay.potential_decrease = "0";
		overlay.guaranteed_decrease = "0";
		overlay.stationary = true;
		overlay.enumerated_vectors = "0";
		overlay.simple_cycles = "0";
		overlay.fundamental_cycles = "0";
		overlay.selected_edge_count = "0";
		for (const [index, node] of overlay.nodes.entries()) {
			node.component = String(index);
			delete node.parent_node_id;
			node.depth = "0";
			node.on_selected = false;
		}
		for (const edge of overlay.edges) {
			edge.updated_flow = "0.5";
			edge.gradient = "0";
			edge.length = "0";
			edge.tree_edge = false;
			edge.selected_sign = "0";
		}
		stationary.outcome = {
			kind: "minimum-ratio-cycle-mcf",
			cycle: [],
			alpha: overlay.alpha,
			kappa: "0",
			eta: "0",
			potential_decrease: "0",
			guaranteed_decrease: "0",
			stationary: true,
		};
		const decoded = decodeFlowCurrentSceneV9(encode(stationary));
		expect(decoded.minimum_ratio_cycle_mcf_overlay).toEqual(
			expect.objectContaining({ stationary: true, fundamental_cycles: "0" }),
		);
	});

	it("validates randomized tree-chain isolation, source rounding, and certificate", () => {
		const valid = randomizedAlmostLinearScene();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("optimal");
		expect(decoded.randomized_almost_linear_overlay).toEqual(
			expect.objectContaining({
				stage: "optimal",
				forest_pool_size: "2",
				sample_count: "4",
				final_return_flow: "3",
			}),
		);

		const ready = structuredClone(valid);
		ready.event_id = "0";
		ready.event_count = "0";
		ready.solve_status = "ready";
		ready.edge_states = [{ edge_id: "st", flow: "0" }];
		ready.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "3",
				cost: "0",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
		];
		delete ready.trace_event;
		delete ready.outcome;
		delete ready.randomized_almost_linear_overlay;
		expect(() => decodeFlowCurrentSceneV9(encode(ready))).not.toThrow();

		const badProbability = structuredClone(valid);
		(
			badProbability.randomized_almost_linear_overlay as {
				miss_probability: { numerator: string; denominator: string };
			}
		).miss_probability = { numerator: "2", denominator: "1" };
		expect(() => decodeFlowCurrentSceneV9(encode(badProbability))).toThrowError(
			/source scalar or resource bound is inconsistent/,
		);

		const badCirculation = structuredClone(valid);
		const signedEdge = (
			badCirculation.randomized_almost_linear_overlay as {
				edges: { active_cycle_sign: string }[];
			}
		).edges[0];
		if (signedEdge === undefined) throw new Error("missing randomized edge");
		signedEdge.active_cycle_sign = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(badCirculation))).toThrowError(
			/active direction is not a circulation/,
		);

		const badReturnCapacity = structuredClone(valid);
		(
			badReturnCapacity.randomized_almost_linear_overlay as {
				return_capacity: string;
			}
		).return_capacity = "4";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(badReturnCapacity)),
		).toThrowError(/source scalar or resource bound is inconsistent/);

		const badIsolationScale = structuredClone(valid);
		(
			badIsolationScale.randomized_almost_linear_overlay as {
				isolation_scale: string;
			}
		).isolation_scale = "143";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(badIsolationScale)),
		).toThrowError(/source scalar or resource bound is inconsistent/);

		const badIsolationProbability = structuredClone(valid);
		(
			badIsolationProbability.randomized_almost_linear_overlay as {
				isolation_failure_probability: {
					numerator: string;
					denominator: string;
				};
			}
		).isolation_failure_probability = { numerator: "1", denominator: "4" };
		expect(() =>
			decodeFlowCurrentSceneV9(encode(badIsolationProbability)),
		).toThrowError(
			/isolation, final-point, or rounding fields are inconsistent/,
		);

		const badArtificialSummary = structuredClone(valid);
		(
			badArtificialSummary.randomized_almost_linear_overlay as {
				artificial_flow: string;
			}
		).artificial_flow = "1";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(badArtificialSummary)),
		).toThrowError(/artificial-star summary is inconsistent/);

		const badFinalFlow = structuredClone(valid);
		const finalEdge = (
			badFinalFlow.randomized_almost_linear_overlay as {
				edges: { final_flow: string }[];
			}
		).edges[0];
		if (finalEdge === undefined)
			throw new Error("missing randomized final edge");
		finalEdge.final_flow = "2";
		expect(() => decodeFlowCurrentSceneV9(encode(badFinalFlow))).toThrowError(
			/edge coordinate or final flow is inconsistent/,
		);

		const badOutcome = structuredClone(valid);
		(badOutcome.outcome as { source_side: string[] }).source_side = ["t"];
		expect(() => decodeFlowCurrentSceneV9(encode(badOutcome))).toThrowError(
			/optimum lacks its exact certificate/,
		);
	});

	it("validates randomized MCF exact final-point feasibility, gap, and nearest rounding", () => {
		const valid = randomizedAlmostLinearMcfScene();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("optimal");
		expect(decoded.randomized_almost_linear_mcf_overlay).toEqual(
			expect.objectContaining({
				stage: "optimal",
				final_point_gap: { numerator: "0", denominator: "1" },
				final_point_threshold: { numerator: "1", denominator: "96" },
				exact_recovery: true,
			}),
		);

		const badPoint = structuredClone(valid);
		const pointEdge = (
			badPoint.randomized_almost_linear_mcf_overlay as {
				edges: {
					final_point_flow: { numerator: string; denominator: string };
				}[];
			}
		).edges[0];
		if (pointEdge === undefined) throw new Error("missing randomized MCF edge");
		pointEdge.final_point_flow = { numerator: "3", denominator: "2" };
		expect(() => decodeFlowCurrentSceneV9(encode(badPoint))).toThrowError(
			/nearest-integer recovery|final point is infeasible or inaccurate/,
		);

		const badGap = structuredClone(valid);
		(
			badGap.randomized_almost_linear_mcf_overlay as {
				final_point_gap: { numerator: string; denominator: string };
			}
		).final_point_gap = { numerator: "1", denominator: "96" };
		expect(() => decodeFlowCurrentSceneV9(encode(badGap))).toThrowError(
			/final point is infeasible or inaccurate/,
		);

		const badThreshold = structuredClone(valid);
		(
			badThreshold.randomized_almost_linear_mcf_overlay as {
				final_point_threshold: { numerator: string; denominator: string };
			}
		).final_point_threshold = { numerator: "1", denominator: "95" };
		expect(() => decodeFlowCurrentSceneV9(encode(badThreshold))).toThrowError(
			/final-point header is inconsistent/,
		);

		const badRounded = structuredClone(valid);
		const roundedEdge = (
			badRounded.randomized_almost_linear_mcf_overlay as {
				edges: { final_flow: string }[];
			}
		).edges[0];
		if (roundedEdge === undefined) throw new Error("missing rounded MCF edge");
		roundedEdge.final_flow = "0";
		expect(() => decodeFlowCurrentSceneV9(encode(badRounded))).toThrowError(
			/nearest-integer recovery is inconsistent/,
		);

		const missingIsolatedOptimum = structuredClone(valid);
		delete (
			missingIsolatedOptimum.randomized_almost_linear_mcf_overlay as {
				edges: { isolated_optimum_flow?: string }[];
			}
		).edges[0]?.isolated_optimum_flow;
		expect(() =>
			decodeFlowCurrentSceneV9(encode(missingIsolatedOptimum)),
		).toThrowError(
			/isolated optimum is inconsistent|edge state is inconsistent/,
		);
	});

	it("validates deterministic shifted tree chains, additive-half final point, flow rounding, and certificate", () => {
		const valid = deterministicAlmostLinearScene();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("optimal");
		expect(decoded.deterministic_almost_linear_overlay).toEqual(
			expect.objectContaining({
				stage: "optimal",
				active_branches: ["0", "0"],
				passes: ["0", "0"],
				core_edges: "1",
				spanner_edges: "1",
				final_point_gap: { numerator: "0", denominator: "1" },
				final_point_threshold: { numerator: "1", denominator: "2" },
				final_return_flow: "3",
			}),
		);

		const ready = structuredClone(valid);
		ready.event_id = "0";
		ready.event_count = "0";
		ready.solve_status = "ready";
		ready.edge_states = [{ edge_id: "st", flow: "0" }];
		ready.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "3",
				cost: "0",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
		];
		delete ready.trace_event;
		delete ready.outcome;
		delete ready.deterministic_almost_linear_overlay;
		expect(() => decodeFlowCurrentSceneV9(encode(ready))).not.toThrow();

		const unknownField = structuredClone(valid);
		(
			unknownField.deterministic_almost_linear_overlay as Record<
				string,
				unknown
			>
		).random_seed = "forbidden";
		expect(() => decodeFlowCurrentSceneV9(encode(unknownField))).toThrowError(
			/unknown field random_seed/,
		);

		const badPass = structuredClone(valid);
		(
			badPass.deterministic_almost_linear_overlay as { passes: string[] }
		).passes = ["0", "3"];
		expect(() => decodeFlowCurrentSceneV9(encode(badPass))).toThrowError(
			/branch\/core contract is inconsistent/,
		);

		const badForestMask = structuredClone(valid);
		const forestEdge = (
			badForestMask.deterministic_almost_linear_overlay as {
				edges: { tree_level_mask: string; forest_level_mask: string }[];
			}
		).edges[0];
		if (forestEdge === undefined)
			throw new Error("missing deterministic forest edge");
		forestEdge.tree_level_mask = "1";
		forestEdge.forest_level_mask = "2";
		expect(() => decodeFlowCurrentSceneV9(encode(badForestMask))).toThrowError(
			/edge\/core state is inconsistent/,
		);

		const badSpanner = structuredClone(valid);
		const spannerEdge = (
			badSpanner.deterministic_almost_linear_overlay as {
				edges: { active_core_edge: boolean; active_spanner_edge: boolean }[];
			}
		).edges[0];
		if (spannerEdge === undefined)
			throw new Error("missing deterministic spanner edge");
		spannerEdge.active_core_edge = false;
		spannerEdge.active_spanner_edge = true;
		expect(() => decodeFlowCurrentSceneV9(encode(badSpanner))).toThrowError(
			/edge\/core state is inconsistent/,
		);

		const badCirculation = structuredClone(valid);
		const cycleEdge = (
			badCirculation.deterministic_almost_linear_overlay as {
				edges: { active_cycle_sign: string }[];
			}
		).edges[0];
		if (cycleEdge === undefined)
			throw new Error("missing deterministic cycle edge");
		cycleEdge.active_cycle_sign = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(badCirculation))).toThrowError(
			/active direction is not a circulation/,
		);

		const badFinalFlow = structuredClone(valid);
		const finalEdge = (
			badFinalFlow.deterministic_almost_linear_overlay as {
				edges: { final_flow: string }[];
			}
		).edges[0];
		if (finalEdge === undefined)
			throw new Error("missing deterministic final edge");
		finalEdge.final_flow = "2";
		expect(() => decodeFlowCurrentSceneV9(encode(badFinalFlow))).toThrowError(
			/edge\/core state is inconsistent/,
		);

		const badThreshold = structuredClone(valid);
		(
			badThreshold.deterministic_almost_linear_overlay as {
				final_point_threshold: { numerator: string; denominator: string };
			}
		).final_point_threshold = { numerator: "1", denominator: "3" };
		expect(() => decodeFlowCurrentSceneV9(encode(badThreshold))).toThrowError(
			/branch\/core contract is inconsistent/,
		);

		const badGap = structuredClone(valid);
		(
			badGap.deterministic_almost_linear_overlay as {
				final_point_gap: { numerator: string; denominator: string };
			}
		).final_point_gap = { numerator: "1", denominator: "4" };
		expect(() => decodeFlowCurrentSceneV9(encode(badGap))).toThrowError(
			/final-point gap does not match the return edge/,
		);

		const badRoundedFlow = structuredClone(valid);
		const roundedEdge = (
			badRoundedFlow.deterministic_almost_linear_overlay as {
				edges: {
					rounding_flow: { numerator: string; denominator: string };
				}[];
			}
		).edges[0];
		if (roundedEdge === undefined)
			throw new Error("missing deterministic rounded edge");
		roundedEdge.rounding_flow = { numerator: "5", denominator: "2" };
		expect(() => decodeFlowCurrentSceneV9(encode(badRoundedFlow))).toThrowError(
			/edge\/core state is inconsistent/,
		);

		const badOutcome = structuredClone(valid);
		(badOutcome.outcome as { source_side: string[] }).source_side = [];
		expect(() => decodeFlowCurrentSceneV9(encode(badOutcome))).toThrowError(
			/completion lacks its certificate/,
		);
	});

	it("validates integer primal-dual IPM auxiliary graphs, sticky minors, and crossover recovery", () => {
		const valid = primalDualIpmMcfScene();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("optimal");
		expect(decoded.primal_dual_ipm_mcf_overlay).toEqual(
			expect.objectContaining({
				stage: "optimal",
				beta: "256",
				gamma: "32768",
				proxy_gap: "0",
			}),
		);
		expect(decoded.primal_dual_ipm_mcf_overlay?.nodes).toHaveLength(3);
		expect(decoded.primal_dual_ipm_mcf_overlay?.arcs).toHaveLength(3);

		const ready = structuredClone(valid);
		ready.event_id = "0";
		ready.event_count = "0";
		ready.solve_status = "ready";
		ready.edge_states = [{ edge_id: "st", flow: "0" }];
		ready.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "2",
				cost: "1",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "-1",
				active: false,
			},
		];
		delete ready.trace_event;
		delete ready.outcome;
		delete ready.primal_dual_ipm_mcf_overlay;
		expect(() => decodeFlowCurrentSceneV9(encode(ready))).not.toThrow();

		const unknownField = structuredClone(valid);
		(
			unknownField.primal_dual_ipm_mcf_overlay as Record<string, unknown>
		).floating_point_mu = 1;
		expect(() => decodeFlowCurrentSceneV9(encode(unknownField))).toThrowError(
			/unknown field floating_point_mu/,
		);

		const badProxy = structuredClone(valid);
		(badProxy.primal_dual_ipm_mcf_overlay as { proxy_gap: string }).proxy_gap =
			"500000";
		expect(() => decodeFlowCurrentSceneV9(encode(badProxy))).toThrowError(
			/proxy inequality is false/,
		);

		const badEndpoint = structuredClone(valid);
		const endpointArc = (
			badEndpoint.primal_dual_ipm_mcf_overlay as {
				arcs: { from: string }[];
			}
		).arcs[0];
		if (endpointArc === undefined) throw new Error("missing IPM arc");
		endpointArc.from = "node:unknown";
		expect(() => decodeFlowCurrentSceneV9(encode(badEndpoint))).toThrowError(
			/auxiliary arc is inconsistent/,
		);

		const badStickyState = structuredClone(valid);
		const stickyArc = (
			badStickyState.primal_dual_ipm_mcf_overlay as {
				arcs: { deleted: boolean; contracted: boolean }[];
			}
		).arcs[0];
		if (stickyArc === undefined) throw new Error("missing sticky IPM arc");
		stickyArc.contracted = true;
		expect(() => decodeFlowCurrentSceneV9(encode(badStickyState))).toThrowError(
			/auxiliary arc is inconsistent/,
		);

		const badForestSerial = structuredClone(valid);
		(
			badForestSerial.primal_dual_ipm_mcf_overlay as {
				forest_subset_serial: string;
			}
		).forest_subset_serial = "1";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(badForestSerial)),
		).toThrowError(/forest inspection metadata is inconsistent/);

		const badForestCandidate = structuredClone(valid);
		const candidateArc = (
			badForestCandidate.primal_dual_ipm_mcf_overlay as {
				arcs: { forest_candidate: boolean }[];
			}
		).arcs[0];
		if (candidateArc === undefined)
			throw new Error("missing candidate IPM arc");
		candidateArc.forest_candidate = true;
		expect(() =>
			decodeFlowCurrentSceneV9(encode(badForestCandidate)),
		).toThrowError(/auxiliary arc is inconsistent/);

		const badCatalog = structuredClone(valid);
		(badCatalog.trace_event as { catalog_id: string }).catalog_id =
			"primal-dual-interior-point-mcf.check-certificate";
		expect(() => decodeFlowCurrentSceneV9(encode(badCatalog))).toThrowError(
			/trace event and stage disagree/,
		);

		const badOutcome = structuredClone(valid);
		(badOutcome.outcome as { total_cost: string }).total_cost = "3";
		expect(() => decodeFlowCurrentSceneV9(encode(badOutcome))).toThrowError(
			/exact objective|minimum-cost certificate/,
		);
	});

	it("validates electrical IPM isolation, central estimates, and rounded recovery", () => {
		const valid = electricalIpmMcfScene();
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.solve_status).toBe("optimal");
		expect(decoded.electrical_ipm_mcf_overlay).toEqual(
			expect.objectContaining({
				stage: "optimal",
				isolation_scale: "16",
				isolated_optimum_cost: "1",
				isolated_gap: "17",
			}),
		);
		expect(decoded.electrical_ipm_mcf_overlay?.edges).toHaveLength(2);

		const reorderedGraph = structuredClone(valid);
		(
			reorderedGraph.graph as {
				edges: unknown[];
			}
		).edges.reverse();
		expect(() =>
			decodeFlowCurrentSceneV9(encode(reorderedGraph)),
		).not.toThrow();

		const ready = structuredClone(valid);
		ready.event_id = "0";
		ready.event_count = "0";
		ready.solve_status = "ready";
		ready.edge_states = [
			{ edge_id: "cheap", flow: "0" },
			{ edge_id: "dear", flow: "0" },
		];
		ready.residual_arcs = [
			{
				edge_id: "cheap",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "1",
				cost: "0",
				active: false,
			},
			{
				edge_id: "cheap",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
			{
				edge_id: "dear",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "1",
				cost: "1",
				active: false,
			},
			{
				edge_id: "dear",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "-1",
				active: false,
			},
		];
		delete ready.trace_event;
		delete ready.outcome;
		delete ready.electrical_ipm_mcf_overlay;
		expect(() => decodeFlowCurrentSceneV9(encode(ready))).not.toThrow();

		const unknownField = structuredClone(valid);
		(
			unknownField.electrical_ipm_mcf_overlay as Record<string, unknown>
		).fallback_flow = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(unknownField))).toThrowError(
			/unknown field fallback_flow/,
		);

		const badIsolation = structuredClone(valid);
		const isolatedEdge = (
			badIsolation.electrical_ipm_mcf_overlay as {
				edges: { isolated_cost: string }[];
			}
		).edges[0];
		if (isolatedEdge === undefined) throw new Error("missing isolated edge");
		isolatedEdge.isolated_cost = "2";
		expect(() => decodeFlowCurrentSceneV9(encode(badIsolation))).toThrowError(
			/edge projection is inconsistent/,
		);

		const badCentralEstimate = structuredClone(valid);
		const centralEdge = (
			badCentralEstimate.electrical_ipm_mcf_overlay as {
				edges: { fractional_flow: string }[];
			}
		).edges[0];
		if (centralEdge === undefined) throw new Error("missing central edge");
		centralEdge.fractional_flow = "0.5";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(badCentralEstimate)),
		).toThrowError(/central estimate is inconsistent/);

		const badFinal = structuredClone(valid);
		const finalEdge = (
			badFinal.electrical_ipm_mcf_overlay as {
				edges: { final_flow: string }[];
			}
		).edges[0];
		if (finalEdge === undefined) throw new Error("missing final edge");
		finalEdge.final_flow = "0";
		expect(() => decodeFlowCurrentSceneV9(encode(badFinal))).toThrowError(
			/rounded flow projection drifted/,
		);

		const badCatalog = structuredClone(valid);
		(badCatalog.trace_event as { catalog_id: string }).catalog_id =
			"electrical-flow-interior-point-mcf.check-certificate";
		expect(() => decodeFlowCurrentSceneV9(encode(badCatalog))).toThrowError(
			/trace event and stage disagree/,
		);
	});

	it("validates exact Cancel-and-Tighten prices and the admissible residual set", () => {
		const valid = validScene();
		valid.model = {
			kind: "fixed-flow-min-cost",
			source: "s",
			sink: "t",
			required_flow: "0",
		};
		valid.algorithm = { id: "cancel-and-tighten", config: {} };
		valid.event_id = "1";
		valid.event_count = "3";
		valid.solve_status = "running";
		valid.trace_event = {
			event_id: "1",
			catalog_id: "cancel-and-tighten.initialize",
			minimum_granularity: "phase",
			pseudocode_line: "cancel-and-tighten:initialize-exact-epsilon-state",
			patch_count: 1,
			entity_refs: [],
		};
		valid.cancel_tighten_overlay = {
			stage: "initialize",
			epsilon: { numerator: "7", denominator: "1" },
			phase: "0",
			nodes: ["s", "t"].map((node_id) => ({
				node_id,
				potential: { numerator: "0", denominator: "1" },
			})),
			admissible_arcs: [{ edge_id: "st", direction: "forward" }],
			active_cycle: [],
			inspected_arcs: [],
		};
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.cancel_tighten_overlay?.epsilon).toEqual({
			numerator: "7",
			denominator: "1",
		});
		const inspectionTrace = valid.trace_event as {
			catalog_id: string;
			minimum_granularity: string;
		};
		inspectionTrace.catalog_id =
			"cancel-and-tighten.inspect-cycle-residual-arc";
		inspectionTrace.minimum_granularity = "micro";
		const inspectionOverlay = valid.cancel_tighten_overlay as {
			stage: string;
			phase: string;
			inspected_arcs: { edge_id: string; direction: string }[];
		};
		inspectionOverlay.stage = "inspect-cycle-arc";
		inspectionOverlay.phase = "1";
		inspectionOverlay.inspected_arcs = [
			{ edge_id: "st", direction: "forward" },
		];
		expect(
			decodeFlowCurrentSceneV9(encode(valid)).cancel_tighten_overlay
				?.inspected_arcs,
		).toEqual([{ edge_id: "st", direction: "forward" }]);

		(
			valid.cancel_tighten_overlay as {
				admissible_arcs: { edge_id: string; direction: string }[];
			}
		).admissible_arcs = [];
		expect(() => decodeFlowCurrentSceneV9(encode(valid))).toThrowError(
			/admissible residual set is incorrect/,
		);
	});

	it("validates relaxed-MNDC assignment permutations and exact dual evidence", () => {
		const valid = validScene();
		valid.model = {
			kind: "fixed-flow-min-cost",
			source: "s",
			sink: "t",
			required_flow: "0",
		};
		valid.algorithm = { id: "relaxed-most-negative-cycle", config: {} };
		valid.event_id = "1";
		valid.event_count = "2";
		valid.solve_status = "running";
		valid.trace_event = {
			event_id: "1",
			catalog_id: "relaxed-most-negative-cycle.phase-optimal",
			minimum_granularity: "phase",
			pseudocode_line: "relaxed-mndc:certify-shifted-negative-cycle-absence",
			patch_count: 1,
			entity_refs: [],
			detail: { label: "assignment value", value: "0" },
		};
		valid.relaxed_mndc_overlay = {
			stage: "phase-optimal",
			epsilon: { numerator: "7", denominator: "1" },
			phase: "1",
			assignment_value: "0",
			nodes: ["s", "t"].map((node_id) => ({
				node_id,
				matched_node_id: node_id,
				left_dual: "0",
				right_dual: "0",
			})),
			family: [],
			inspected_arcs: [],
		};
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.relaxed_mndc_overlay?.assignment_value).toBe("0");

		const overlay = valid.relaxed_mndc_overlay as {
			nodes: { matched_node_id: string; left_dual: string }[];
		};
		const sourceNode = overlay.nodes[0];
		const sinkNode = overlay.nodes[1];
		expect(sourceNode).toBeDefined();
		expect(sinkNode).toBeDefined();
		if (sourceNode === undefined || sinkNode === undefined) return;
		sinkNode.matched_node_id = "s";
		expect(() => decodeFlowCurrentSceneV9(encode(valid))).toThrowError(
			/metadata is inconsistent/,
		);
		sinkNode.matched_node_id = "t";
		sourceNode.left_dual = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(valid))).toThrowError(
			/selected assignment edge is not tight/,
		);
	});

	it("validates exact enhanced-scaling quotient paths, partitions, and dual state", () => {
		const valid = validScene();
		valid.model = { kind: "transshipment" };
		valid.algorithm = { id: "enhanced-capacity-scaling", config: {} };
		valid.event_id = "1";
		valid.event_count = "4";
		valid.solve_status = "running";
		valid.graph = {
			nodes: [
				{ id: "s", supply: "1" },
				{ id: "t", supply: "-1" },
			],
			edges: [
				{
					id: "st",
					from: "s",
					to: "t",
					lower: "0",
					capacity: "2",
					cost: "0",
				},
			],
		};
		valid.edge_states = [{ edge_id: "st", flow: "0" }];
		valid.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "2",
				cost: "0",
				active: true,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
		];
		valid.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
		valid.trace_event = {
			event_id: "1",
			catalog_id: "enhanced-capacity-scaling.select-path",
			minimum_granularity: "operation",
			pseudocode_line: "orlin-ecs:shortest-quotient-path",
			patch_count: 2,
			entity_refs: [
				{ kind: "residual-arc", edge_id: "st", direction: "forward" },
			],
			detail: { label: "delta numerator", value: "1" },
		};
		valid.enhanced_capacity_scaling_overlay = {
			stage: "select-path",
			delta: { numerator: "1", denominator: "1" },
			phase: "1",
			components: [
				{
					component_id: "s",
					members: ["s"],
					excess: { numerator: "1", denominator: "1" },
				},
				{
					component_id: "t",
					members: ["t"],
					excess: { numerator: "-1", denominator: "1" },
				},
			],
			nodes: [
				{ node_id: "s", component_id: "s", potential: "0", distance: "0" },
				{ node_id: "t", component_id: "t", potential: "0", distance: "0" },
			],
			edges: [
				{
					edge_id: "st",
					virtual_flow: { numerator: "0", denominator: "1" },
					reduced_cost: "0",
					internal: false,
					strongly_feasible: false,
					tight: true,
				},
			],
			source_component: "s",
			sink_component: "t",
			path: [{ edge_id: "st", direction: "forward" }],
		};

		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.enhanced_capacity_scaling_overlay?.delta).toEqual({
			numerator: "1",
			denominator: "1",
		});
		expect(decoded.enhanced_capacity_scaling_overlay?.path).toHaveLength(1);

		const corruptDual = structuredClone(valid);
		const corruptDualEdge = (
			corruptDual.enhanced_capacity_scaling_overlay as {
				edges: { reduced_cost: string }[];
			}
		).edges[0];
		if (corruptDualEdge === undefined)
			throw new Error("fixture edge is missing");
		corruptDualEdge.reduced_cost = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptDual))).toThrowError(
			/edge dual state is inconsistent/,
		);

		const corruptPartition = structuredClone(valid);
		const corruptComponent = (
			corruptPartition.enhanced_capacity_scaling_overlay as {
				components: { members: string[] }[];
			}
		).components[1];
		if (corruptComponent === undefined)
			throw new Error("fixture component is missing");
		corruptComponent.members = ["s", "t"];
		expect(() =>
			decodeFlowCurrentSceneV9(encode(corruptPartition)),
		).toThrowError(/component partition is invalid/);

		const corruptPath = structuredClone(valid);
		const corruptArc = (
			corruptPath.enhanced_capacity_scaling_overlay as {
				path: { direction: string }[];
			}
		).path[0];
		if (corruptArc === undefined) throw new Error("fixture arc is missing");
		corruptArc.direction = "reverse";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptPath))).toThrowError(
			/not a quotient path/,
		);
	});

	it("validates Orlin finite-capacity nodes, branch duals, and expanded compressed paths", () => {
		const valid = validScene();
		valid.event_id = "1";
		valid.event_count = "4";
		valid.solve_status = "running";
		valid.model = { kind: "transshipment" };
		valid.algorithm = { id: "orlin-mcf", config: {} };
		valid.graph = {
			nodes: [
				{ id: "s", supply: "1" },
				{ id: "t", supply: "-1" },
			],
			edges: [
				{
					id: "st",
					from: "s",
					to: "t",
					lower: "0",
					capacity: "2",
					cost: "1",
				},
			],
		};
		valid.edge_states = [{ edge_id: "st", flow: "0" }];
		valid.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "2",
				cost: "1",
				active: true,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "-1",
				active: false,
			},
		];
		valid.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
		valid.trace_event = {
			event_id: "1",
			catalog_id: "orlin-mcf.select-compressed-path",
			minimum_granularity: "operation",
			pseudocode_line: "eliminate capacity nodes and run shortest path",
			patch_count: 1,
			entity_refs: [
				{ kind: "residual-arc", edge_id: "st", direction: "forward" },
			],
			detail: { label: "compressed shortcuts", value: "0" },
		};
		valid.orlin_mcf_overlay = {
			stage: "select-compressed-path",
			delta: { numerator: "1", denominator: "1" },
			phase: "1",
			components: [
				{
					component_id: "s",
					members: ["s"],
					excess: { numerator: "1", denominator: "1" },
				},
				{
					component_id: "t",
					members: ["t"],
					excess: { numerator: "1", denominator: "1" },
				},
				{
					component_id: "capacity:st",
					members: ["capacity:st"],
					excess: { numerator: "-2", denominator: "1" },
				},
			],
			nodes: [
				{
					node_id: "s",
					kind: "original",
					component_id: "s",
					potential: "0",
					distance: "0",
				},
				{
					node_id: "t",
					kind: "original",
					component_id: "t",
					potential: "1",
					distance: "1",
				},
				{
					node_id: "capacity:st",
					kind: "capacity",
					capacity_edge_id: "st",
					component_id: "capacity:st",
					potential: "1",
					distance: "1",
				},
			],
			arcs: [
				{
					edge_id: "st",
					branch: "flow",
					flow: { numerator: "0", denominator: "1" },
					reduced_cost: "0",
					internal: false,
					strongly_feasible: false,
					tight: true,
				},
				{
					edge_id: "st",
					branch: "slack",
					flow: { numerator: "0", denominator: "1" },
					reduced_cost: "0",
					internal: false,
					strongly_feasible: false,
					tight: true,
				},
			],
			source_component: "s",
			sink_component: "capacity:st",
			path: [{ edge_id: "st", branch: "flow", direction: "forward" }],
			inspected_segment: [],
			eliminated_capacity_nodes: "0",
			shortcut_arcs: "0",
		};

		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.orlin_mcf_overlay?.nodes).toHaveLength(3);
		expect(decoded.orlin_mcf_overlay?.path[0]?.branch).toBe("flow");

		const inspection = structuredClone(valid);
		inspection.trace_event = {
			...(inspection.trace_event as Record<string, unknown>),
			catalog_id: "orlin-mcf.inspect-compressed-arc",
			minimum_granularity: "micro",
			detail: { label: "residual arc scan", value: "1" },
		};
		const inspectionOverlay = inspection.orlin_mcf_overlay as {
			stage: string;
			nodes: { distance?: string }[];
			path: unknown[];
			inspected_segment: unknown[];
			inspection_serial?: string;
			source_component?: string;
			sink_component?: string;
		};
		inspectionOverlay.stage = "inspect-compressed-arc";
		inspectionOverlay.path = [];
		inspectionOverlay.inspected_segment = [
			{ edge_id: "st", branch: "flow", direction: "forward" },
		];
		inspectionOverlay.inspection_serial = "1";
		delete inspectionOverlay.source_component;
		delete inspectionOverlay.sink_component;
		expect(
			decodeFlowCurrentSceneV9(encode(inspection)).orlin_mcf_overlay?.stage,
		).toBe("inspect-compressed-arc");

		const missingInspectionDistance = structuredClone(inspection);
		const inspectionNodes = (
			missingInspectionDistance.orlin_mcf_overlay as {
				nodes: { distance?: string }[];
			}
		).nodes;
		for (const node of inspectionNodes) delete node.distance;
		expect(() =>
			decodeFlowCurrentSceneV9(encode(missingInspectionDistance)),
		).toThrowError(/boundary metadata is inconsistent/);

		const corruptDual = structuredClone(valid);
		const corruptDualArc = (
			corruptDual.orlin_mcf_overlay as {
				arcs: { reduced_cost: string }[];
			}
		).arcs[0];
		if (corruptDualArc === undefined) throw new Error("fixture arc is missing");
		corruptDualArc.reduced_cost = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptDual))).toThrowError(
			/branch dual state is inconsistent/,
		);

		const corruptNode = structuredClone(valid);
		const corruptCapacityNode = (
			corruptNode.orlin_mcf_overlay as {
				nodes: { capacity_edge_id?: string }[];
			}
		).nodes[2];
		if (corruptCapacityNode === undefined)
			throw new Error("fixture capacity node is missing");
		corruptCapacityNode.capacity_edge_id = "unknown";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptNode))).toThrowError(
			/transformed-node projection is inconsistent/,
		);

		const corruptPath = structuredClone(valid);
		const corruptPathArc = (
			corruptPath.orlin_mcf_overlay as {
				path: { branch: string }[];
			}
		).path[0];
		if (corruptPathArc === undefined)
			throw new Error("fixture path is missing");
		corruptPathArc.branch = "slack";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptPath))).toThrowError(
			/not a transformed quotient path/,
		);
	});

	it("validates Orlin max-flow quotient identities, residual classes, and phase metadata", () => {
		const valid = validScene();
		valid.event_id = "1";
		valid.event_count = "4";
		valid.solve_status = "running";
		valid.algorithm = { id: "orlin-max-flow", config: {} };
		valid.graph = {
			nodes: [
				{ id: "s", supply: "0" },
				{ id: "t", supply: "0" },
			],
			edges: [
				{
					id: "st",
					from: "s",
					to: "t",
					lower: "0",
					capacity: "10",
					cost: "0",
				},
			],
		};
		valid.edge_states = [{ edge_id: "st", flow: "0" }];
		valid.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "10",
				cost: "0",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "0",
				active: false,
			},
		];
		valid.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
		valid.trace_event = {
			event_id: "1",
			catalog_id: "orlin-max-flow.select-case",
			minimum_granularity: "phase",
			pseudocode_line: "select original, Δ-compact, or (Δ,Γ)-compact case",
			patch_count: 1,
			entity_refs: [],
			detail: { label: "delta", value: "10" },
		};
		valid.orlin_max_flow_overlay = {
			stage: "select-case",
			delta: "10",
			gamma: { numerator: "0", denominator: "1" },
			phase_case: "original-approximation",
			nodes: [
				{
					node_id: "s",
					component_id: "s",
					critical: true,
					anti_potential: "-10",
					source_side: true,
				},
				{
					node_id: "t",
					component_id: "t",
					critical: true,
					anti_potential: "10",
					source_side: false,
				},
			],
			residual_arcs: [
				{
					edge_id: "st",
					direction: "forward",
					capacity: "10",
					abundant: false,
					anti_abundant: true,
					small: false,
					medium: true,
				},
				{
					edge_id: "st",
					direction: "reverse",
					capacity: "0",
					abundant: false,
					anti_abundant: false,
					small: false,
					medium: false,
				},
			],
			compact_arcs: [],
			active_compact_path: [],
			active_original_path: [],
			threshold: "0",
		};

		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.orlin_max_flow_overlay?.phase_case).toBe(
			"original-approximation",
		);
		expect(decoded.orlin_max_flow_overlay?.residual_arcs).toHaveLength(2);

		const inspected = structuredClone(valid);
		inspected.trace_event = {
			event_id: "1",
			catalog_id: "orlin-max-flow.inspect-lift-residual-arc",
			minimum_granularity: "micro",
			pseudocode_line: "inspect one original residual lift route",
			patch_count: 1,
			entity_refs: [
				{ kind: "residual-arc", edge_id: "st", direction: "forward" },
			],
			detail: { label: "arc scans", value: "17" },
		};
		const inspectedOverlay = inspected.orlin_max_flow_overlay as {
			stage: string;
			active_original_path: { edge_id: string; direction: string }[];
			residual_arcs: { inspection_serial?: string }[];
		};
		inspectedOverlay.stage = "inspect-lift-residual-arc";
		inspectedOverlay.active_original_path = [
			{ edge_id: "st", direction: "forward" },
		];
		const inspectedResidual = inspectedOverlay.residual_arcs[0];
		if (inspectedResidual === undefined)
			throw new Error("fixture residual is missing");
		inspectedResidual.inspection_serial = "17";
		const activeResidual = (
			inspected.residual_arcs as { active: boolean }[]
		)[0];
		if (activeResidual === undefined)
			throw new Error("fixture generic residual is missing");
		activeResidual.active = true;
		expect(
			decodeFlowCurrentSceneV9(encode(inspected)).orlin_max_flow_overlay
				?.residual_arcs[0]?.inspection_serial,
		).toBe("17");

		const classificationInspection = structuredClone(inspected);
		const classificationTrace = classificationInspection.trace_event as {
			catalog_id: string;
			pseudocode_line: string;
		};
		classificationTrace.catalog_id =
			"orlin-max-flow.inspect-classification-arc";
		classificationTrace.pseudocode_line =
			"inspect one residual or quotient arc for phase classification";
		const classificationOverlay =
			classificationInspection.orlin_max_flow_overlay as {
				stage: string;
				phase_case?: string;
			};
		classificationOverlay.stage = "inspect-classification-arc";
		delete classificationOverlay.phase_case;
		expect(
			decodeFlowCurrentSceneV9(encode(classificationInspection))
				.orlin_max_flow_overlay?.stage,
		).toBe("inspect-classification-arc");

		const cutInspection = structuredClone(inspected);
		const cutTrace = cutInspection.trace_event as {
			catalog_id: string;
			pseudocode_line: string;
		};
		cutTrace.catalog_id = "orlin-max-flow.inspect-cut-residual-arc";
		cutTrace.pseudocode_line =
			"inspect one residual arc for the next source cut";
		(
			cutInspection.orlin_max_flow_overlay as {
				stage: string;
			}
		).stage = "inspect-cut-residual-arc";
		expect(
			decodeFlowCurrentSceneV9(encode(cutInspection)).orlin_max_flow_overlay
				?.stage,
		).toBe("inspect-cut-residual-arc");

		for (const [stage, catalogId, pseudocode] of [
			[
				"inspect-compact-construction-arc",
				"orlin-max-flow.inspect-compact-construction-arc",
				"inspect one quotient arc while building the compact network",
			],
			[
				"inspect-expansion-residual-arc",
				"orlin-max-flow.inspect-expansion-residual-arc",
				"inspect one residual arc while expanding a contraction",
			],
		] as const) {
			const inspection = structuredClone(inspected);
			const traceEvent = inspection.trace_event as {
				catalog_id: string;
				pseudocode_line: string;
			};
			traceEvent.catalog_id = catalogId;
			traceEvent.pseudocode_line = pseudocode;
			(
				inspection.orlin_max_flow_overlay as {
					stage: string;
				}
			).stage = stage;
			expect(
				decodeFlowCurrentSceneV9(encode(inspection)).orlin_max_flow_overlay
					?.stage,
			).toBe(stage);
		}

		const staleInspection = structuredClone(valid);
		const staleResidual = (
			staleInspection.orlin_max_flow_overlay as {
				residual_arcs: { inspection_serial?: string }[];
			}
		).residual_arcs[0];
		if (staleResidual === undefined)
			throw new Error("fixture residual is missing");
		staleResidual.inspection_serial = "17";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(staleInspection)),
		).toThrowError(/boundary metadata is inconsistent/);

		const corruptResidual = structuredClone(valid);
		const residual = (
			corruptResidual.orlin_max_flow_overlay as {
				residual_arcs: { capacity: string }[];
			}
		).residual_arcs[0];
		if (residual === undefined) throw new Error("fixture residual is missing");
		residual.capacity = "9";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(corruptResidual)),
		).toThrowError(/residual classification is inconsistent/);

		const corruptComponent = structuredClone(valid);
		const node = (
			corruptComponent.orlin_max_flow_overlay as {
				nodes: { component_id: string }[];
			}
		).nodes[0];
		if (node === undefined) throw new Error("fixture node is missing");
		node.component_id = "t";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(corruptComponent)),
		).toThrowError(/component projection is inconsistent/);

		const corruptPhase = structuredClone(valid);
		delete (
			corruptPhase.orlin_max_flow_overlay as {
				phase_case?: string;
			}
		).phase_case;
		expect(() => decodeFlowCurrentSceneV9(encode(corruptPhase))).toThrowError(
			/boundary metadata is inconsistent/,
		);
	});

	it("validates dual-simplex tree, signed basic flow, head cut, and price delta", () => {
		const valid = validScene();
		valid.model = { kind: "transshipment" };
		valid.algorithm = { id: "dual-network-simplex", config: {} };
		valid.event_id = "1";
		valid.event_count = "3";
		valid.solve_status = "running";
		valid.graph = {
			nodes: [
				{ id: "s", supply: "-1" },
				{ id: "t", supply: "1" },
			],
			edges: [
				{
					id: "st",
					from: "s",
					to: "t",
					lower: "0",
					capacity: "2",
					cost: "1",
				},
				{
					id: "ts",
					from: "t",
					to: "s",
					lower: "0",
					capacity: "2",
					cost: "2",
				},
			],
		};
		valid.edge_states = [
			{ edge_id: "st", flow: "0" },
			{ edge_id: "ts", flow: "0" },
		];
		valid.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "2",
				cost: "1",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "-1",
				active: false,
			},
			{
				edge_id: "ts",
				direction: "forward",
				from: "t",
				to: "s",
				capacity: "2",
				cost: "2",
				active: false,
			},
			{
				edge_id: "ts",
				direction: "reverse",
				from: "s",
				to: "t",
				capacity: "0",
				cost: "-2",
				active: false,
			},
		];
		valid.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
		valid.trace_event = {
			event_id: "1",
			catalog_id: "dual-network-simplex.select-entering",
			minimum_granularity: "operation",
			pseudocode_line: "dual-ns:minimum-reduced-cost-cut-arc",
			patch_count: 2,
			entity_refs: [
				{ kind: "edge", edge_id: "st" },
				{ kind: "edge", edge_id: "ts" },
			],
			detail: { label: "price delta", value: "3" },
		};
		valid.dual_network_simplex_overlay = {
			stage: "select-entering",
			nodes: [
				{ node_id: "s", potential: "0", initialized: true, in_cut: false },
				{ node_id: "t", potential: "1", initialized: true, in_cut: true },
			],
			edges: [
				{ edge_id: "st", basic_flow: "-1", reduced_cost: "0", in_tree: true },
				{ edge_id: "ts", basic_flow: "0", reduced_cost: "3", in_tree: false },
			],
			cut_side: ["t"],
			leaving_edge: "st",
			entering_edge: "ts",
			pivot_price_delta: "3",
		};

		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.dual_network_simplex_overlay?.leaving_edge).toBe("st");
		expect(decoded.dual_network_simplex_overlay?.edges[0]?.basic_flow).toBe(
			"-1",
		);

		const corruptDual = structuredClone(valid);
		const corruptDualEdge = (
			corruptDual.dual_network_simplex_overlay as {
				edges: { reduced_cost: string }[];
			}
		).edges[1];
		if (corruptDualEdge === undefined)
			throw new Error("fixture dual edge is missing");
		corruptDualEdge.reduced_cost = "2";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptDual))).toThrowError(
			/basic flow or dual slack is inconsistent/,
		);

		const corruptCut = structuredClone(valid);
		(
			corruptCut.dual_network_simplex_overlay as {
				nodes: { in_cut: boolean }[];
				cut_side: string[];
			}
		).cut_side = ["s"];
		(
			corruptCut.dual_network_simplex_overlay as {
				nodes: { in_cut: boolean }[];
			}
		).nodes.forEach((node, index) => {
			node.in_cut = index === 0;
		});
		expect(() => decodeFlowCurrentSceneV9(encode(corruptCut))).toThrowError(
			/head-side cut orientation is inconsistent/,
		);

		const corruptBasis = structuredClone(valid);
		const corruptNonTreeEdge = (
			corruptBasis.dual_network_simplex_overlay as {
				edges: { basic_flow: string }[];
			}
		).edges[1];
		if (corruptNonTreeEdge === undefined)
			throw new Error("fixture non-tree edge is missing");
		corruptNonTreeEdge.basic_flow = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptBasis))).toThrowError(
			/basic flow or dual slack is inconsistent/,
		);
	});

	it("validates polynomial-dual pseudoflow, bad subtree, and Make-Good pivot", () => {
		const valid = validScene();
		valid.model = { kind: "transshipment" };
		valid.algorithm = { id: "polynomial-dual-network-simplex", config: {} };
		valid.event_id = "2";
		valid.event_count = "4";
		valid.solve_status = "running";
		valid.graph = {
			nodes: [
				{ id: "s", supply: "-1" },
				{ id: "t", supply: "1" },
			],
			edges: [
				{
					id: "st",
					from: "s",
					to: "t",
					lower: "0",
					capacity: "2",
					cost: "1",
				},
				{
					id: "ts",
					from: "t",
					to: "s",
					lower: "0",
					capacity: "2",
					cost: "2",
				},
			],
		};
		valid.edge_states = [
			{ edge_id: "st", flow: "0" },
			{ edge_id: "ts", flow: "0" },
		];
		valid.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "2",
				cost: "1",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "0",
				cost: "-1",
				active: false,
			},
			{
				edge_id: "ts",
				direction: "forward",
				from: "t",
				to: "s",
				capacity: "2",
				cost: "2",
				active: false,
			},
			{
				edge_id: "ts",
				direction: "reverse",
				from: "s",
				to: "t",
				capacity: "0",
				cost: "-2",
				active: false,
			},
		];
		valid.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
		valid.trace_event = {
			event_id: "2",
			parent_phase_id: "1",
			catalog_id: "polynomial-dual-network-simplex.select-entering-arc",
			minimum_granularity: "operation",
			pseudocode_line: "make-good:min-reduced-cost-cut-arc",
			patch_count: 2,
			entity_refs: [
				{ kind: "edge", edge_id: "st" },
				{ kind: "edge", edge_id: "ts" },
			],
			detail: { label: "price delta", value: "3" },
		};
		valid.polynomial_dual_simplex_overlay = {
			stage: "select-entering",
			phase: "1",
			delta: { numerator: "1", denominator: "2" },
			nodes: [
				{
					node_id: "s",
					potential: "0",
					excess: { numerator: "0", denominator: "1" },
					root: true,
					active: false,
					bad: false,
					in_pivot_cut: false,
				},
				{
					node_id: "t",
					potential: "1",
					excess: { numerator: "1", denominator: "2" },
					root: false,
					active: false,
					bad: true,
					in_pivot_cut: true,
				},
			],
			edges: [
				{
					edge_id: "st",
					pseudoflow: { numerator: "0", denominator: "1" },
					basic_flow: "-1",
					reduced_cost: "0",
					in_tree: true,
					bad: true,
					in_augment_path: false,
				},
				{
					edge_id: "ts",
					pseudoflow: { numerator: "0", denominator: "1" },
					basic_flow: "0",
					reduced_cost: "3",
					in_tree: false,
					bad: false,
					in_augment_path: false,
				},
			],
			augment_path: [],
			bad_edges: ["st"],
			bad_nodes: ["t"],
			leaving_edge: "st",
			entering_edge: "ts",
			pivot_cut: ["t"],
			pivot_price_delta: "3",
		};

		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.polynomial_dual_simplex_overlay?.delta).toEqual({
			numerator: "1",
			denominator: "2",
		});
		expect(decoded.polynomial_dual_simplex_overlay?.bad_nodes).toEqual(["t"]);

		const corruptBad = structuredClone(valid);
		const corruptBadEdge = (
			corruptBad.polynomial_dual_simplex_overlay as {
				edges: { pseudoflow: { numerator: string } }[];
			}
		).edges[0];
		if (corruptBadEdge === undefined)
			throw new Error("fixture edge is missing");
		corruptBadEdge.pseudoflow.numerator = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptBad))).toThrowError(
			/bad subtree projection is inconsistent/,
		);

		const corruptReduced = structuredClone(valid);
		const corruptReducedEdge = (
			corruptReduced.polynomial_dual_simplex_overlay as {
				edges: { reduced_cost: string }[];
			}
		).edges[1];
		if (corruptReducedEdge === undefined)
			throw new Error("fixture edge is missing");
		corruptReducedEdge.reduced_cost = "2";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptReduced))).toThrowError(
			/tree flow or dual slack is inconsistent/,
		);

		const active = structuredClone(valid);
		const activeOverlay = active.polynomial_dual_simplex_overlay as {
			stage: string;
			active_node?: string;
			augment_path: { edge_id: string; direction: string }[];
			nodes: { active: boolean; in_pivot_cut: boolean }[];
			edges: {
				in_augment_path: boolean;
				augment_direction?: string;
			}[];
			leaving_edge?: string;
			entering_edge?: string;
			pivot_cut: string[];
			pivot_price_delta?: string;
		};
		activeOverlay.stage = "select-active";
		activeOverlay.active_node = "t";
		activeOverlay.augment_path = [{ edge_id: "st", direction: "reverse" }];
		const activeNode = activeOverlay.nodes[1];
		const activeEdge = activeOverlay.edges[0];
		const activeArc = activeOverlay.augment_path[0];
		if (
			activeNode === undefined ||
			activeEdge === undefined ||
			activeArc === undefined
		) {
			throw new Error("active path fixture is incomplete");
		}
		activeNode.active = true;
		activeNode.in_pivot_cut = false;
		activeEdge.in_augment_path = true;
		activeEdge.augment_direction = "reverse";
		delete activeOverlay.leaving_edge;
		delete activeOverlay.entering_edge;
		activeOverlay.pivot_cut = [];
		delete activeOverlay.pivot_price_delta;
		const activeTraceEvent = active.trace_event as
			| { catalog_id: string }
			| null
			| undefined;
		if (activeTraceEvent === undefined || activeTraceEvent === null) {
			throw new Error("fixture event missing");
		}
		activeTraceEvent.catalog_id =
			"polynomial-dual-network-simplex.select-active-node";
		expect(() => decodeFlowCurrentSceneV9(encode(active))).not.toThrow();
		activeArc.direction = "forward";
		activeEdge.augment_direction = "forward";
		expect(() => decodeFlowCurrentSceneV9(encode(active))).toThrowError(
			/active path direction is inconsistent/,
		);
	});

	it("validates polynomial-simplex epsilon, extended tree, and selected cycle", () => {
		const valid = validScene();
		valid.model = { kind: "transshipment" };
		valid.algorithm = {
			id: "polynomial-primal-network-simplex",
			config: {},
		};
		valid.event_id = "2";
		valid.event_count = "3";
		valid.solve_status = "running";
		valid.graph = {
			nodes: [
				{ id: "s", supply: "1" },
				{ id: "t", supply: "-1" },
			],
			edges: [
				{
					id: "st",
					from: "s",
					to: "t",
					lower: "0",
					capacity: "2",
					cost: "1",
				},
				{
					id: "ts",
					from: "t",
					to: "s",
					lower: "0",
					capacity: "2",
					cost: "-3",
				},
			],
		};
		valid.edge_states = [
			{ edge_id: "st", flow: "1" },
			{ edge_id: "ts", flow: "0" },
		];
		valid.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "1",
				cost: "1",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "1",
				cost: "-1",
				active: false,
			},
			{
				edge_id: "ts",
				direction: "forward",
				from: "t",
				to: "s",
				capacity: "2",
				cost: "-3",
				active: false,
			},
			{
				edge_id: "ts",
				direction: "reverse",
				from: "s",
				to: "t",
				capacity: "0",
				cost: "3",
				active: false,
			},
		];
		valid.node_trace_states = [{ node_id: "s" }, { node_id: "t" }];
		valid.trace_event = {
			event_id: "2",
			parent_phase_id: "1",
			catalog_id: "polynomial-primal-network-simplex.select-admissible-arc",
			minimum_granularity: "operation",
			pseudocode_line: "orlin-pns:select-admissible-arc",
			patch_count: 1,
			entity_refs: [
				{ kind: "residual-arc", edge_id: "ts", direction: "forward" },
			],
		};
		valid.polynomial_primal_simplex_overlay = {
			stage: "select-admissible",
			phase: "1",
			epsilon: { numerator: "4", denominator: "1" },
			perturbation_scale: "3",
			nodes: [
				{
					entity_id: "s",
					kind: "original",
					premultiplier: { numerator: "0", denominator: "1" },
					flags: ["eligible", "awake", "in-n-star", "root"],
				},
				{
					entity_id: "t",
					kind: "original",
					premultiplier: { numerator: "-1", denominator: "1" },
					flags: ["eligible", "awake", "in-n-star"],
				},
				{
					entity_id: "artificial-root",
					kind: "artificial-root",
					premultiplier: { numerator: "0", denominator: "1" },
					flags: ["awake", "in-n-star"],
				},
			],
			edges: [
				{
					edge_id: "st",
					basis: "tree",
					perturbed_flow: "1",
					unperturbed_basic_flow: "1",
					reduced_cost: { numerator: "0", denominator: "1" },
					in_cycle: true,
					entering: false,
					leaving: false,
				},
				{
					edge_id: "ts",
					basis: "lower",
					perturbed_flow: "0",
					unperturbed_basic_flow: "0",
					reduced_cost: { numerator: "-2", denominator: "1" },
					in_cycle: true,
					entering: true,
					leaving: false,
				},
			],
			artificial_edges: [
				{
					entity_id: "artificial:s",
					node_id: "s",
					basis: "tree",
					perturbed_flow: "2",
					unperturbed_basic_flow: "0",
					in_cycle: false,
					entering: false,
					leaving: false,
				},
				{
					entity_id: "artificial:t",
					node_id: "t",
					basis: "lower",
					perturbed_flow: "0",
					unperturbed_basic_flow: "0",
					in_cycle: false,
					entering: false,
					leaving: false,
				},
			],
			entering: {
				entity_id: "ts",
				original_edge_id: "ts",
				direction: "forward",
			},
			cycle: [
				{
					entity_id: "ts",
					original_edge_id: "ts",
					direction: "forward",
				},
				{
					entity_id: "st",
					original_edge_id: "st",
					direction: "forward",
				},
			],
		};

		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.polynomial_primal_simplex_overlay?.cycle).toHaveLength(2);
		expect(decoded.polynomial_primal_simplex_overlay?.nodes[2]?.kind).toBe(
			"artificial-root",
		);

		const corruptReducedCost = structuredClone(valid);
		const corruptReducedEdge = (
			corruptReducedCost.polynomial_primal_simplex_overlay as {
				edges: { reduced_cost: { numerator: string } }[];
			}
		).edges[1];
		if (corruptReducedEdge === undefined)
			throw new Error("fixture reduced-cost edge is missing");
		corruptReducedEdge.reduced_cost.numerator = "-1";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(corruptReducedCost)),
		).toThrowError(/reduced cost is inconsistent/);

		const corruptTree = structuredClone(valid);
		const corruptArtificial = (
			corruptTree.polynomial_primal_simplex_overlay as {
				artificial_edges: { basis: string }[];
			}
		).artificial_edges[0];
		if (corruptArtificial === undefined)
			throw new Error("fixture artificial edge is missing");
		corruptArtificial.basis = "lower";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptTree))).toThrowError(
			/artificial basis bound is invalid|wrong cardinality/,
		);

		const corruptCycle = structuredClone(valid);
		const corruptCycleEdge = (
			corruptCycle.polynomial_primal_simplex_overlay as {
				edges: { in_cycle: boolean }[];
			}
		).edges[0];
		if (corruptCycleEdge === undefined)
			throw new Error("fixture cycle edge is missing");
		corruptCycleEdge.in_cycle = false;
		expect(() => decodeFlowCurrentSceneV9(encode(corruptCycle))).toThrowError(
			/edge flags disagree/,
		);
	});

	it("validates exact double-scaling transportation branches and reduced costs", () => {
		const valid = validScene();
		valid.model = {
			kind: "fixed-flow-min-cost",
			source: "s",
			sink: "t",
			required_flow: "0",
		};
		valid.algorithm = { id: "double-scaling", config: {} };
		valid.event_id = "1";
		valid.event_count = "3";
		valid.solve_status = "running";
		valid.trace_event = {
			event_id: "1",
			catalog_id: "double-scaling.initialize-transportation",
			minimum_granularity: "phase",
			pseudocode_line: "double-scaling:build-transportation-network",
			patch_count: 1,
			entity_refs: [],
		};
		valid.double_scaling_overlay = {
			stage: "initialize",
			epsilon: "32",
			cost_multiplier: "4",
			delta: "0",
			cost_phase: "0",
			capacity_phase: "0",
			nodes: [
				{
					entity_id: "s",
					kind: "original",
					price: "0",
					imbalance: "0",
					cursor: "0",
				},
				{
					entity_id: "t",
					kind: "original",
					price: "0",
					imbalance: "18446744073709551615",
					cursor: "0",
				},
				{
					entity_id: "st",
					kind: "edge",
					price: "0",
					imbalance: "-18446744073709551615",
					cursor: "0",
				},
			],
			edges: [{ edge_id: "st", flow_branch: "0", slack_branch: "0" }],
			admissible_arcs: [
				{ edge_id: "st", branch: "flow", direction: "forward" },
			],
			active_path: [],
		};
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.double_scaling_overlay?.epsilon).toBe("32");
		expect(decoded.double_scaling_overlay?.nodes).toHaveLength(3);

		(
			valid.double_scaling_overlay as {
				admissible_arcs: unknown[];
			}
		).admissible_arcs = [];
		expect(() => decodeFlowCurrentSceneV9(encode(valid))).toThrowError(
			/transformed arc set is incorrect/,
		);
	});

	it("recomputes convex segment occupancy marginal costs and native objective", () => {
		const valid = validScene();
		valid.model = { kind: "convex-cost-flow" };
		valid.algorithm = { id: "segment-expanded-convex-mcf", config: {} };
		valid.event_id = "1";
		valid.event_count = "1";
		valid.solve_status = "optimal";
		valid.graph = {
			nodes: [
				{ id: "s", supply: "2" },
				{ id: "t", supply: "-2" },
			],
			edges: [
				{
					id: "st",
					from: "s",
					to: "t",
					lower: "0",
					capacity: "3",
					cost: "0",
					convex_cost: {
						base_cost_at_zero: "-4",
						segments: [
							{ end_flow: "1", marginal_cost: "2" },
							{ end_flow: "3", marginal_cost: "5" },
						],
					},
				},
			],
		};
		valid.edge_states = [{ edge_id: "st", flow: "2" }];
		valid.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "1",
				cost: "0",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "2",
				cost: "0",
				active: false,
			},
		];
		valid.node_trace_states = [
			{ node_id: "s", label: "0" },
			{ node_id: "t", label: "5" },
		];
		valid.trace_event = {
			event_id: "1",
			catalog_id: "segment-expanded-convex-mcf.optimal",
			minimum_granularity: "phase",
			pseudocode_line: "segment-expanded-convex-mcf:certify-marginal-residual",
			patch_count: 0,
			entity_refs: [],
		};
		valid.convex_cost_overlay = {
			stage: "optimal",
			edges: [
				{
					edge_id: "st",
					base_cost_at_zero: "-4",
					flow: "2",
					total_cost: "3",
					forward_marginal_cost: "5",
					reverse_marginal_cost: "5",
					segments: [
						{
							segment: "0",
							start_flow: "0",
							end_flow: "1",
							flow: "1",
							marginal_cost: "2",
						},
						{
							segment: "1",
							start_flow: "1",
							end_flow: "3",
							flow: "1",
							marginal_cost: "5",
						},
					],
				},
			],
			active_cycle: [],
		};
		valid.outcome = {
			kind: "min-cost-flow",
			total_cost: "3",
			potentials: [
				{ node_id: "s", potential: "0" },
				{ node_id: "t", potential: "5" },
			],
		};
		const decoded = decodeFlowCurrentSceneV9(encode(valid));
		expect(decoded.convex_cost_overlay?.edges[0]?.segments).toHaveLength(2);
		expect(decoded.outcome).toMatchObject({ total_cost: "3" });

		const canceled = structuredClone(valid);
		canceled.solve_status = "running";
		delete canceled.outcome;
		canceled.trace_event = {
			event_id: "1",
			catalog_id: "segment-expanded-convex-mcf.cancel-cycle",
			minimum_granularity: "operation",
			pseudocode_line: "segment-expanded-convex-mcf:cancel-expanded-cycle",
			patch_count: 1,
			entity_refs: [
				{ kind: "residual-arc", edge_id: "st", direction: "forward" },
			],
			detail: { label: "delta", value: "1" },
		};
		if (canceled.convex_cost_overlay === undefined) {
			throw new Error("fixture convex overlay is missing");
		}
		const canceledOverlay = canceled.convex_cost_overlay as {
			stage: string;
			active_cycle: { edge_id: string; segment: string; direction: string }[];
		};
		canceledOverlay.stage = "cancel-cycle";
		canceledOverlay.active_cycle = [
			{ edge_id: "st", segment: "1", direction: "forward" },
			{ edge_id: "st", segment: "1", direction: "reverse" },
		];
		expect(() => decodeFlowCurrentSceneV9(encode(canceled))).not.toThrow();
		const canceledEvent = canceled.trace_event as {
			detail?: { label: string; value: string };
		};
		if (canceledEvent.detail === undefined) {
			throw new Error("fixture cancellation detail is missing");
		}
		canceledEvent.detail.value = "2";
		expect(() => decodeFlowCurrentSceneV9(encode(canceled))).toThrowError(
			/active marginal arc is not residual/,
		);

		const initialized = structuredClone(valid);
		initialized.solve_status = "running";
		delete initialized.outcome;
		initialized.trace_event = {
			event_id: "1",
			catalog_id: "segment-expanded-convex-mcf.start-selector",
			minimum_granularity: "phase",
			pseudocode_line: "segment-expanded-convex-mcf:start-expanded-selector",
			patch_count: 0,
			entity_refs: [],
		};
		(
			initialized.convex_cost_overlay as {
				stage: string;
			}
		).stage = "initialize";
		expect(() => decodeFlowCurrentSceneV9(encode(initialized))).not.toThrow();

		const inspected = structuredClone(valid);
		inspected.solve_status = "running";
		inspected.event_id = "2";
		inspected.event_count = "2";
		delete inspected.outcome;
		inspected.trace_event = {
			event_id: "2",
			parent_phase_id: "1",
			catalog_id: "segment-expanded-convex-mcf.inspect-residual-arc",
			minimum_granularity: "micro",
			pseudocode_line:
				"segment-expanded-convex-mcf:inspect-marginal-residual-arc",
			patch_count: 1,
			entity_refs: [
				{ kind: "residual-arc", edge_id: "st", direction: "forward" },
			],
			detail: { label: "marginal-arc-cost", value: "5" },
		};
		const inspectedOverlay = inspected.convex_cost_overlay as {
			stage: string;
			active_cycle: { edge_id: string; segment: string; direction: string }[];
		};
		inspectedOverlay.stage = "select-minimum-mean-cycle";
		inspectedOverlay.active_cycle = [
			{ edge_id: "st", segment: "1", direction: "forward" },
		];
		expect(() => decodeFlowCurrentSceneV9(encode(inspected))).not.toThrow();
		const inspectedScaleAwareSource = structuredClone(inspected);
		(
			inspectedScaleAwareSource.trace_event as {
				detail: { label: string; value: string };
			}
		).detail.label =
			"expanded marginal residual-arc inspections · marginal-arc-cost 5 · units 1–1 of 1";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(inspectedScaleAwareSource)),
		).not.toThrow();
		const mismatchedInspectionFocus = structuredClone(inspected);
		(
			mismatchedInspectionFocus.trace_event as {
				entity_refs: {
					kind: string;
					edge_id: string;
					direction: string;
				}[];
			}
		).entity_refs[0] = {
			kind: "residual-arc",
			edge_id: "st",
			direction: "reverse",
		};
		expect(() =>
			decodeFlowCurrentSceneV9(encode(mismatchedInspectionFocus)),
		).toThrowError(/active marginal walk is inconsistent/);

		const inspectedWorkUnit = structuredClone(inspected);
		inspectedWorkUnit.trace_event = {
			...(inspectedWorkUnit.trace_event as NonNullable<
				typeof inspectedWorkUnit.trace_event
			>),
			catalog_id:
				"segment-expanded-convex-mcf.inspect-residual-arc.primary-work-unit",
			minimum_granularity: "micro",
			patch_count: 0,
			entity_refs: [
				{ kind: "residual-arc", edge_id: "st", direction: "forward" },
			],
			detail: {
				label: "dynamic-programming rounds · unit 1 of 2",
				value: "1",
			},
		};
		inspectedWorkUnit.trace_event_semantics = {
			role: "observe",
			work_deltas: [
				{ unit: "published-transition", count: "1" },
				{ unit: "detail-primitive", count: "1" },
				{ unit: "primary-work", count: "1" },
			],
			aggregation_count: "1",
			work_progress: {
				detail_completed: "1",
				detail_total: "2",
				primary_completed: "1",
				primary_total: "2",
			},
			primary_work_block: { first: "1", last: "1", total: "2" },
			changed_entity_refs: [],
		};
		expect(() =>
			decodeFlowCurrentSceneV9(encode(inspectedWorkUnit)),
		).toThrowError(/synthetic counter-only Detail/u);
		const workObservation = structuredClone(inspectedWorkUnit);
		(
			workObservation.trace_event as {
				catalog_id: string;
			}
		).catalog_id =
			"segment-expanded-convex-mcf.inspect-residual-arc.work-observation";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(workObservation)),
		).toThrowError(/synthetic graphical work observation/u);
		const inspectedWorkBlock = structuredClone(inspectedWorkUnit);
		(
			inspectedWorkBlock.trace_event as {
				catalog_id: string;
			}
		).catalog_id = "segment-expanded-convex-mcf.inspect-residual-arc";
		(
			inspectedWorkBlock.trace_event as {
				detail: { label: string; value: string };
			}
		).detail = {
			label: "marginal-arc-cost",
			value: "5",
		};
		(
			inspectedWorkBlock.trace_event_semantics as {
				work_deltas: { unit: string; count: string }[];
				aggregation_count: string;
			}
		).work_deltas[2] = { unit: "primary-work", count: "3" };
		(
			inspectedWorkBlock.trace_event_semantics as {
				aggregation_count: string;
			}
		).aggregation_count = "3";
		(
			inspectedWorkBlock.trace_event_semantics as {
				primary_work_block: { first: string; last: string; total: string };
			}
		).primary_work_block = { first: "1", last: "3", total: "3" };
		expect(() =>
			decodeFlowCurrentSceneV9(encode(inspectedWorkBlock)),
		).not.toThrow();
		const forgedWorkBlock = structuredClone(inspectedWorkBlock);
		(
			forgedWorkBlock.trace_event_semantics as {
				primary_work_block: { first: string; last: string; total: string };
			}
		).primary_work_block.last = "5";
		expect(() =>
			decodeFlowCurrentSceneV9(encode(forgedWorkBlock)),
		).toThrowError(
			/primary-work boundary must own one exact action-local block/,
		);
		const forgedWorkUnit = structuredClone(inspectedWorkUnit);
		(
			forgedWorkUnit.trace_event_semantics as {
				work_deltas: { unit: string; count: string }[];
			}
		).work_deltas = [
			{ unit: "published-transition", count: "1" },
			{ unit: "detail-primitive", count: "1" },
		];
		expect(() => decodeFlowCurrentSceneV9(encode(forgedWorkUnit))).toThrowError(
			/synthetic counter-only Detail/u,
		);

		const canceledWorkUnit = structuredClone(canceled);
		canceledWorkUnit.trace_event = {
			...(canceledWorkUnit.trace_event as NonNullable<
				typeof canceledWorkUnit.trace_event
			>),
			catalog_id: "segment-expanded-convex-mcf.cancel-cycle.primary-work-unit",
			minimum_granularity: "micro",
			patch_count: 0,
			entity_refs: [],
			detail: {
				label: "dynamic-programming rounds · unit 1 of 2",
				value: "1",
			},
		};
		canceledWorkUnit.trace_event_semantics =
			inspectedWorkUnit.trace_event_semantics;
		expect(() =>
			decodeFlowCurrentSceneV9(encode(canceledWorkUnit)),
		).toThrowError(/synthetic counter-only Detail/u);

		(
			inspected.trace_event as {
				catalog_id: string;
			}
		).catalog_id = "segment-expanded-convex-mcf.select-minimum-mean-cycle";
		expect(() => decodeFlowCurrentSceneV9(encode(inspected))).toThrowError(
			/active marginal walk is inconsistent/,
		);

		const corruptedEdge = (
			valid.convex_cost_overlay as {
				edges: { total_cost: string }[];
			}
		).edges[0];
		if (corruptedEdge === undefined) throw new Error("fixture edge is missing");
		corruptedEdge.total_cost = "4";
		expect(() => decodeFlowCurrentSceneV9(encode(valid))).toThrowError(
			/marginal summary is inconsistent/,
		);
	});

	it("recomputes native convex Δ-eligible marginal boundaries", () => {
		const valid = validScene();
		valid.model = { kind: "convex-cost-flow" };
		valid.algorithm = { id: "convex-cost-scaling", config: {} };
		valid.event_id = "1";
		valid.event_count = "1";
		valid.solve_status = "optimal";
		valid.graph = {
			nodes: [
				{ id: "s", supply: "2" },
				{ id: "t", supply: "-2" },
			],
			edges: [
				{
					id: "st",
					from: "s",
					to: "t",
					lower: "0",
					capacity: "3",
					cost: "0",
					convex_cost: {
						base_cost_at_zero: "-4",
						segments: [
							{ end_flow: "1", marginal_cost: "2" },
							{ end_flow: "3", marginal_cost: "5" },
						],
					},
				},
			],
		};
		valid.edge_states = [{ edge_id: "st", flow: "2" }];
		valid.residual_arcs = [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "1",
				cost: "0",
				active: false,
			},
			{
				edge_id: "st",
				direction: "reverse",
				from: "t",
				to: "s",
				capacity: "2",
				cost: "0",
				active: false,
			},
		];
		valid.node_trace_states = [
			{ node_id: "s", label: "0", remaining_divergence: "0" },
			{ node_id: "t", label: "5", remaining_divergence: "0" },
		];
		valid.trace_event = {
			event_id: "1",
			catalog_id: "convex-cost-scaling.certify-expanded-oracle",
			minimum_granularity: "operation",
			pseudocode_line: "convex-cost-scaling:compare-expanded-oracle",
			patch_count: 1,
			entity_refs: [],
			detail: { label: "total-cost", value: "3" },
		};
		valid.convex_cost_overlay = {
			stage: "optimal",
			scale: "1",
			edges: [
				{
					edge_id: "st",
					base_cost_at_zero: "-4",
					flow: "2",
					total_cost: "3",
					forward_marginal_cost: "5",
					reverse_marginal_cost: "5",
					segments: [
						{
							segment: "0",
							start_flow: "0",
							end_flow: "1",
							flow: "1",
							marginal_cost: "2",
						},
						{
							segment: "1",
							start_flow: "1",
							end_flow: "3",
							flow: "1",
							marginal_cost: "5",
						},
					],
				},
			],
			active_cycle: [],
			eligible_arcs: [
				{ edge_id: "st", segment: "1", direction: "forward" },
				{ edge_id: "st", segment: "1", direction: "reverse" },
			],
		};
		valid.outcome = {
			kind: "min-cost-flow",
			total_cost: "3",
			potentials: [
				{ node_id: "s", potential: "0" },
				{ node_id: "t", potential: "5" },
			],
		};
		expect(() => decodeFlowCurrentSceneV9(encode(valid))).not.toThrow();

		const saturationWork = structuredClone(valid);
		saturationWork.solve_status = "running";
		delete saturationWork.outcome;
		const saturationMetrics = saturationWork.metrics as string[];
		saturationMetrics[2] = "1";
		saturationWork.trace_event = {
			event_id: "1",
			catalog_id: "convex-cost-scaling.saturate-negative-eligible-marginal",
			minimum_granularity: "micro",
			pseudocode_line: "convex-cost-scaling:saturate-negative-marginal",
			patch_count: 1,
			entity_refs: [
				{ kind: "residual-arc", edge_id: "st", direction: "forward" },
			],
			detail: { label: "delta", value: "1" },
		};
		saturationWork.trace_event_semantics = {
			role: "mutate",
			work_deltas: [
				{ unit: "published-transition", count: "1" },
				{ unit: "detail-primitive", count: "1" },
				{ unit: "primary-work", count: "1" },
			],
			aggregation_count: "1",
			work_progress: {
				detail_completed: "1",
				detail_total: "2",
				primary_completed: "1",
				primary_total: "2",
			},
			primary_work_block: { first: "1", last: "1", total: "2" },
			changed_entity_refs: [
				{ kind: "residual-arc", edge_id: "st", direction: "forward" },
			],
		};
		(
			saturationWork.convex_cost_overlay as {
				stage: string;
				active_cycle: { edge_id: string; segment: string; direction: string }[];
			}
		).stage = "saturate-marginal";
		(
			saturationWork.convex_cost_overlay as {
				active_cycle: { edge_id: string; segment: string; direction: string }[];
			}
		).active_cycle = [{ edge_id: "st", segment: "0", direction: "forward" }];
		expect(() =>
			decodeFlowCurrentSceneV9(encode(saturationWork)),
		).not.toThrow();

		const corrupt = structuredClone(valid);
		if (corrupt.convex_cost_overlay === undefined) {
			throw new Error("fixture convex scaling overlay is missing");
		}
		(
			corrupt.convex_cost_overlay as {
				eligible_arcs: unknown[];
			}
		).eligible_arcs.pop();
		expect(() => decodeFlowCurrentSceneV9(encode(corrupt))).toThrowError(
			/eligible marginal set is inconsistent/,
		);
	});

	it("validates Pasche compact convex-simplex basis and oracle boundary", () => {
		const decoded = decodeFlowCurrentSceneV9(
			encode(convexNetworkSimplexScene()),
		);
		expect(decoded.convex_network_simplex_overlay).toMatchObject({
			stage: "optimal",
			artificial_cost: "10",
		});
		expect(
			decoded.convex_network_simplex_overlay?.edges.filter(
				(edge) => edge.basis === "tree",
			),
		).toHaveLength(1);
	});

	it("rejects corrupt convex-simplex tree, segment, stage, and flags", () => {
		const corruptParent = convexNetworkSimplexScene();
		const corruptParentNode = (
			corruptParent.convex_network_simplex_overlay as {
				nodes: { parent?: string }[];
			}
		).nodes[0];
		if (corruptParentNode === undefined) throw new Error("missing parent node");
		corruptParentNode.parent = "artificial-root";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptParent))).toThrowError(
			/extended node is inconsistent|parent is not a tree neighbor/,
		);

		const corruptSegment = convexNetworkSimplexScene();
		const corruptSegmentEdge = (
			corruptSegment.convex_network_simplex_overlay as {
				edges: { active_segment?: string }[];
			}
		).edges[0];
		if (corruptSegmentEdge === undefined) throw new Error("missing edge state");
		corruptSegmentEdge.active_segment = "1";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptSegment))).toThrowError(
			/compact edge state is inconsistent/,
		);

		const corruptStage = convexNetworkSimplexScene();
		(
			corruptStage.trace_event as {
				catalog_id: string;
			}
		).catalog_id = "convex-network-simplex.price-forward-backward";
		expect(() => decodeFlowCurrentSceneV9(encode(corruptStage))).toThrowError(
			/event and stage disagree/,
		);

		const corruptFlags = convexNetworkSimplexScene();
		const corruptFlagEdge = (
			corruptFlags.convex_network_simplex_overlay as {
				edges: { in_cycle: boolean }[];
			}
		).edges[0];
		if (corruptFlagEdge === undefined) throw new Error("missing flag edge");
		corruptFlagEdge.in_cycle = true;
		expect(() => decodeFlowCurrentSceneV9(encode(corruptFlags))).toThrowError(
			/selection flags are inconsistent/,
		);
	});

	it("rejects a discontinuous convex-simplex fundamental cycle", () => {
		const formCycle = convexNetworkSimplexScene();
		formCycle.solve_status = "running";
		delete formCycle.outcome;
		(
			formCycle.convex_cost_overlay as {
				stage: string;
			}
		).stage = "initialize";
		const overlay = formCycle.convex_network_simplex_overlay as {
			stage: string;
			edges: { in_cycle: boolean }[];
			artificial_edges: { in_cycle: boolean; entering: boolean }[];
			entering?: { entity_id: string; direction: string };
			cycle: { entity_id: string; segment?: string; direction: string }[];
		};
		overlay.stage = "form-cycle";
		const originalState = overlay.edges[0];
		const artificialS = overlay.artificial_edges[0];
		const artificialT = overlay.artificial_edges[1];
		if (
			originalState === undefined ||
			artificialS === undefined ||
			artificialT === undefined
		) {
			throw new Error("missing cycle fixture state");
		}
		originalState.in_cycle = true;
		artificialS.in_cycle = true;
		artificialS.entering = true;
		artificialT.in_cycle = true;
		artificialT.entering = false;
		overlay.entering = {
			entity_id: "artificial:s",
			direction: "reverse",
		};
		overlay.cycle = [
			{ entity_id: "st", segment: "0", direction: "forward" },
			{ entity_id: "artificial:t", direction: "forward" },
			{ entity_id: "artificial:s", direction: "reverse" },
		];
		(
			formCycle.trace_event as {
				catalog_id: string;
			}
		).catalog_id = "convex-network-simplex.form-fundamental-cycle";
		expect(() => decodeFlowCurrentSceneV9(encode(formCycle))).not.toThrow();

		overlay.cycle[1] = {
			entity_id: "artificial:t",
			direction: "reverse",
		};
		expect(() => decodeFlowCurrentSceneV9(encode(formCycle))).toThrowError(
			/fundamental cycle is discontinuous/,
		);
	});

	it("validates pseudoflow forest identities and strong-node membership", () => {
		const valid = validScene();
		valid.pseudoflow_forest = {
			arcs: [{ edge_id: "st", direction: "forward" }],
			strong_nodes: ["s"],
		};
		expect(decodeFlowCurrentSceneV9(encode(valid)).pseudoflow_forest).toEqual(
			valid.pseudoflow_forest,
		);

		(valid.pseudoflow_forest as { strong_nodes: string[] }).strong_nodes = [
			"missing",
		];
		expect(() => decodeFlowCurrentSceneV9(encode(valid))).toThrowError(
			/strong nodes do not match/,
		);

		(valid.pseudoflow_forest as { strong_nodes: string[] }).strong_nodes = [
			"s",
		];
		(
			valid.pseudoflow_forest as {
				arcs: { edge_id: string; direction: string }[];
			}
		).arcs.push({ edge_id: "st", direction: "forward" });
		expect(() => decodeFlowCurrentSceneV9(encode(valid))).toThrowError(
			/forest does not match/,
		);
	});

	it("requires transportation basis overlays to be rooted acyclic forests", () => {
		const valid = transportationForestScene();
		expect(
			decodeFlowCurrentSceneV9(encode(valid)).pseudoflow_forest?.arcs,
		).toHaveLength(3);

		const duplicateChild = transportationForestScene();
		(
			duplicateChild.pseudoflow_forest as {
				arcs: { edge_id: string; direction: string }[];
			}
		).arcs.push({ edge_id: "r01", direction: "forward" });
		expect(() => decodeFlowCurrentSceneV9(encode(duplicateChild))).toThrowError(
			/rooted forest/,
		);

		const directedCycle = transportationForestScene();
		(
			directedCycle.pseudoflow_forest as {
				arcs: { edge_id: string; direction: string }[];
			}
		).arcs.push({ edge_id: "r01", direction: "reverse" });
		expect(() => decodeFlowCurrentSceneV9(encode(directedCycle))).toThrowError(
			/rooted forest/,
		);

		const repeatedRoute = transportationForestScene();
		(
			repeatedRoute.pseudoflow_forest as {
				arcs: { edge_id: string; direction: string }[];
			}
		).arcs.push({ edge_id: "r00", direction: "reverse" });
		expect(() => decodeFlowCurrentSceneV9(encode(repeatedRoute))).toThrowError(
			/rooted forest/,
		);
	});
});
