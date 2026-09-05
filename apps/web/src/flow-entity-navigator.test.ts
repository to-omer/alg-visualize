import { describe, expect, it } from "vitest";
import {
	buildFlowEntityNavigatorModel,
	describeFlowEntity,
	flowResidualArcSelectionId,
	searchFlowEntities,
	searchFlowEntityNavigatorModel,
} from "./flow-entity-navigator";
import type { FlowOverviewRenderPlan } from "./flow-render-plan";
import type { FlowCurrentSceneV9 } from "./flow-scene";

function scene(): FlowCurrentSceneV9 {
	return {
		result_schema_version: 9,
		frame_revision: "flow-scene/9",
		event_id: "2",
		event_count: "2",
		solve_status: "optimal",
		model: { kind: "max-flow", source: "s", sink: "t" },
		graph: {
			nodes: [
				{ id: "s", supply: "0", position: { x: "10", y: "20" } },
				{ id: "alpha", supply: "0" },
				{ id: "t", supply: "0" },
			],
			edges: [
				{
					id: "sa",
					from: "s",
					to: "alpha",
					lower: "0",
					capacity: "7",
					cost: "-2",
				},
				{
					id: "at",
					from: "alpha",
					to: "t",
					lower: "0",
					capacity: "5",
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
		edge_states: [
			{ edge_id: "sa", flow: "5" },
			{ edge_id: "at", flow: "5" },
		],
		residual_arcs: [
			{
				edge_id: "sa",
				direction: "forward",
				from: "s",
				to: "alpha",
				capacity: "2",
				cost: "-2",
				active: true,
				fixed: false,
			},
			{
				edge_id: "sa",
				direction: "reverse",
				from: "alpha",
				to: "s",
				capacity: "5",
				cost: "2",
				active: false,
				fixed: false,
			},
		],
		node_trace_states: [{ node_id: "alpha", label: "2" }],
		outcome: {
			kind: "max-flow",
			value: "5",
			cut_bound: "5",
			source_side: ["s", "alpha"],
		},
		metrics: Array.from(
			{ length: 16 },
			() => "0",
		) as FlowCurrentSceneV9["metrics"],
	};
}

function overviewPlan(): FlowOverviewRenderPlan {
	const route = {
		edgeId: "aggregate",
		from: "cluster:0:0",
		to: "cluster:0:1",
		path: "M 0 0 L 1 1",
		reversePath: "M 1 1 L 0 0",
		label: { x: 0.5, y: 0.5 },
		labelWidth: 20,
		labelBoxWidth: 20,
		labelHeight: 24,
		labelYOffset: 0,
		labelCollisionFree: true,
		labelAnchor: { x: 0.5, y: 0.5 },
		laneToken: { x: 0.5, y: 0.5 },
		laneTokenAngle: 0,
		routeMidpoint: { x: 0.5, y: 0.5 },
		parallelIndex: 1,
		parallelCount: 1,
		residualForwardLabel: { x: 0.5, y: 0.25 },
		residualReverseLabel: { x: 0.5, y: 0.75 },
		selfLoop: false,
	};
	return {
		kind: "overview",
		level: "overview",
		grid: { columns: 2, rows: 1 },
		clusters: [
			{
				id: "cluster:0:0",
				x: 100,
				y: 100,
				memberCount: 2,
				sourceSide: "all",
				terminal: "source",
				terminalLabel: "s",
				balance: "none",
				supplyCount: 0,
				demandCount: 0,
				netBalance: 0n,
				containsSupernode: false,
				traceCount: 1,
				traceIdentities: ["node:s"],
				changeCount: 0,
				changedIdentities: [],
			},
		],
		originalEdges: [
			{
				id: "overview-original:source-to-sink",
				from: "cluster:0:0",
				to: "cluster:0:1",
				route,
				edgeCount: 3,
				capacity: 12n,
				flow: 5n,
				costKind: "mixed",
				minimumCost: -2n,
				maximumCost: 3n,
				activeCount: 1,
				fixedCount: 0,
				cutCount: 2,
				traceCount: 1,
				traceIdentities: ["edge:sa"],
				changeCount: 0,
				changedIdentities: [],
			},
		],
		residualArcs: [
			{
				id: "overview-residual:sink-to-source",
				from: "cluster:0:1",
				to: "cluster:0:0",
				route,
				arcCount: 2,
				capacity: 7n,
				direction: "mixed",
				activeCount: 1,
				fixedCount: 1,
				traceCount: 1,
				traceIdentities: ["residual-arc:sa:reverse"],
				changeCount: 0,
				changedIdentities: [],
			},
		],
	};
}

describe("flow entity navigator", () => {
	it("ranks exact IDs before endpoint matches and bounds the result", () => {
		const matches = searchFlowEntities(scene(), "sa", 2);
		expect(matches).toEqual([
			{
				selection: { kind: "edge", id: "sa" },
				label: "sa",
				context: "original edge · s → alpha",
			},
			{
				selection: {
					kind: "residual-arc",
					id: flowResidualArcSelectionId("sa", "forward"),
					edgeId: "sa",
					direction: "forward",
				},
				label: "sa:forward",
				context: "residual arc · s → alpha",
			},
		]);
		expect(
			searchFlowEntities(scene(), "alpha").map((match) => match.label),
		).toEqual(["alpha", "at", "sa", "sa:forward", "sa:reverse"]);
		expect(searchFlowEntities(scene(), "", 2)).toHaveLength(2);
	});

	it("indexes nodes, original edges, residual arcs, and overview aggregates", () => {
		const model = buildFlowEntityNavigatorModel(scene(), overviewPlan());
		expect(
			searchFlowEntityNavigatorModel(model, "residual").map((match) => ({
				kind: match.selection.kind,
				label: match.label,
			})),
		).toEqual([
			{ kind: "residual-arc", label: "sa:forward" },
			{ kind: "residual-arc", label: "sa:reverse" },
			{ kind: "aggregate", label: "cluster:0:1 ⇢ cluster:0:0" },
		]);
		expect(searchFlowEntityNavigatorModel(model, "cluster:0:0", 1)).toEqual([
			{
				selection: {
					kind: "aggregate",
					aggregateKind: "cluster",
					id: "cluster:0:0",
				},
				label: "cluster:0:0",
				context: "aggregate cluster · 2 nodes",
			},
		]);
	});

	it("describes a selected residual arc and rejects a forged selection ID", () => {
		const current = scene();
		const selection = {
			kind: "residual-arc" as const,
			id: flowResidualArcSelectionId("sa", "forward"),
			edgeId: "sa",
			direction: "forward" as const,
		};
		expect(describeFlowEntity(current, selection)).toEqual({
			heading: "Residual arc sa:forward",
			rows: [
				{ label: "Endpoints", value: "s → alpha" },
				{ label: "Direction", value: "forward" },
				{ label: "Residual capacity", value: "2" },
				{ label: "Unit cost", value: "-2" },
				{ label: "Active", value: "yes" },
				{ label: "Fixed", value: "no" },
			],
		});
		expect(
			describeFlowEntity(current, { ...selection, id: "forged" }),
		).toBeUndefined();
	});

	it("describes overview aggregate state only against its owning plan", () => {
		const selection = {
			kind: "aggregate" as const,
			aggregateKind: "original-edge" as const,
			id: "overview-original:source-to-sink",
		};
		expect(describeFlowEntity(scene(), selection)).toBeUndefined();
		expect(describeFlowEntity(scene(), selection, overviewPlan())).toEqual({
			heading: "Original edge aggregate cluster:0:0 → cluster:0:1",
			rows: [
				{ label: "Original edges", value: "3" },
				{ label: "Flow / capacity", value: "5 / 12" },
				{ label: "Cost range", value: "-2…3" },
				{ label: "Active edges", value: "1" },
				{ label: "Fixed edges", value: "0" },
				{ label: "Cut edges", value: "2" },
			],
		});
	});

	it("describes exact node, edge, residual, and cut state", () => {
		const node = describeFlowEntity(scene(), { kind: "node", id: "alpha" });
		expect(node?.heading).toBe("Node alpha");
		expect(node?.rows).toContainEqual({
			label: "Cut side",
			value: "source side",
		});
		expect(node?.rows).toContainEqual({ label: "Trace", value: "2" });

		const edge = describeFlowEntity(scene(), { kind: "edge", id: "sa" });
		expect(edge?.heading).toBe("Edge sa");
		expect(edge?.rows).toContainEqual({
			label: "Flow / capacity",
			value: "5 / 7",
		});
		expect(edge?.rows).toContainEqual({ label: "Residual +", value: "2" });
		expect(edge?.rows).toContainEqual({ label: "Active", value: "yes" });
		expect(
			describeFlowEntity(scene(), { kind: "edge", id: "missing" }),
		).toBeUndefined();
	});

	it("separates Relaxation source prices from certificate potentials", () => {
		const relaxation = scene();
		relaxation.model = {
			kind: "fixed-flow-min-cost",
			source: "s",
			sink: "t",
			required_flow: "2",
		};
		relaxation.algorithm = { id: "relaxation", config: {} };
		relaxation.node_trace_states = [
			{
				node_id: "s",
				label: "0",
				search_ordinal: 1,
				remaining_divergence: "-2",
			},
		];
		relaxation.outcome = {
			kind: "min-cost-flow",
			total_cost: "10",
			potentials: [
				{ node_id: "s", potential: "-5" },
				{ node_id: "alpha", potential: "0" },
				{ node_id: "t", potential: "0" },
			],
		};

		expect(
			describeFlowEntity(relaxation, { kind: "node", id: "s" })?.rows,
		).toEqual([
			{ label: "Role", value: "source" },
			{ label: "Supply", value: "0" },
			{ label: "Position", value: "10, 20" },
			{ label: "Cut side", value: "not computed" },
			{ label: "Certificate potential", value: "-5" },
			{ label: "Source price π", value: "0" },
			{ label: "Deficit d", value: "-2" },
			{ label: "Label ordinal", value: "1" },
		]);
	});

	it("labels Epsilon-Relaxation scaled prices and surplus independently", () => {
		const epsilonRelaxation = scene();
		epsilonRelaxation.algorithm = { id: "epsilon-relaxation", config: {} };
		epsilonRelaxation.node_trace_states = [
			{
				node_id: "s",
				label: "16",
				search_ordinal: 0,
				remaining_divergence: "2",
			},
		];

		expect(
			describeFlowEntity(epsilonRelaxation, { kind: "node", id: "s" })?.rows,
		).toEqual([
			{ label: "Role", value: "source" },
			{ label: "Supply", value: "0" },
			{ label: "Position", value: "10, 20" },
			{ label: "Cut side", value: "source side" },
			{ label: "Certificate potential", value: "—" },
			{ label: "Scaled source price p̂", value: "16" },
			{ label: "Surplus g", value: "2" },
			{ label: "Selection ordinal", value: "0" },
		]);
	});

	it("describes matching partitions, minimum-cover membership, and matched pairs", () => {
		const matching = scene();
		matching.model = {
			kind: "bipartite-matching",
			left: ["l0"],
			right: ["r0"],
		};
		matching.graph = {
			nodes: [
				{ id: "l0", supply: "0" },
				{ id: "r0", supply: "0" },
			],
			edges: [
				{
					id: "pair",
					from: "l0",
					to: "r0",
					lower: "0",
					capacity: "1",
					cost: "0",
				},
			],
		};
		matching.algorithm = { id: "hopcroft-karp", config: {} };
		matching.edge_states = [{ edge_id: "pair", flow: "1" }];
		matching.residual_arcs = [
			{
				edge_id: "pair",
				direction: "forward",
				from: "l0",
				to: "r0",
				capacity: "0",
				cost: "0",
				active: false,
				fixed: false,
			},
			{
				edge_id: "pair",
				direction: "reverse",
				from: "r0",
				to: "l0",
				capacity: "1",
				cost: "0",
				active: false,
				fixed: false,
			},
		];
		matching.node_trace_states = [{ node_id: "l0" }, { node_id: "r0" }];
		matching.outcome = {
			kind: "bipartite-matching",
			cardinality: "1",
			pairs: [{ edge_id: "pair", left: "l0", right: "r0" }],
			cover_left: ["l0"],
			cover_right: [],
		};

		const left = describeFlowEntity(matching, { kind: "node", id: "l0" });
		expect(left?.rows).toContainEqual({ label: "Partition", value: "left" });
		expect(left?.rows).toContainEqual({
			label: "Minimum cover",
			value: "included",
		});
		const edge = describeFlowEntity(matching, { kind: "edge", id: "pair" });
		expect(edge?.rows).toContainEqual({
			label: "Matching",
			value: "l0 ↔ r0",
		});
	});

	it("describes assignment dual labels, selected pairs, and Hall membership", () => {
		const assignment = scene();
		assignment.model = {
			kind: "assignment",
			agents: ["a0"],
			tasks: ["t0"],
			objective: "minimize",
		};
		assignment.graph = {
			nodes: [
				{ id: "a0", supply: "0" },
				{ id: "t0", supply: "0" },
			],
			edges: [
				{
					id: "assign",
					from: "a0",
					to: "t0",
					lower: "0",
					capacity: "1",
					cost: "7",
				},
			],
		};
		assignment.algorithm = { id: "hungarian", config: {} };
		assignment.edge_states = [{ edge_id: "assign", flow: "1" }];
		assignment.residual_arcs = [
			{
				edge_id: "assign",
				direction: "forward",
				from: "a0",
				to: "t0",
				capacity: "0",
				cost: "7",
				active: false,
				fixed: false,
			},
			{
				edge_id: "assign",
				direction: "reverse",
				from: "t0",
				to: "a0",
				capacity: "1",
				cost: "-7",
				active: false,
				fixed: false,
			},
		];
		assignment.node_trace_states = [{ node_id: "a0" }, { node_id: "t0" }];
		assignment.outcome = {
			kind: "assignment",
			objective: "minimize",
			total_cost: "7",
			pairs: [{ edge_id: "assign", agent: "a0", task: "t0", cost: "7" }],
			agent_labels: [{ node_id: "a0", label: "7" }],
			task_labels: [{ node_id: "t0", label: "0" }],
		};

		const agent = describeFlowEntity(assignment, { kind: "node", id: "a0" });
		expect(agent?.rows).toContainEqual({
			label: "Assignment partition",
			value: "agent",
		});
		expect(agent?.rows).toContainEqual({
			label: "Certificate potential",
			value: "7",
		});
		const edge = describeFlowEntity(assignment, { kind: "edge", id: "assign" });
		expect(edge?.rows).toContainEqual({
			label: "Assignment",
			value: "a0 → t0",
		});

		assignment.solve_status = "infeasible";
		assignment.graph.edges = [];
		assignment.edge_states = [];
		assignment.residual_arcs = [];
		assignment.outcome = {
			kind: "assignment-infeasible",
			deficiency: "1",
			hall_agents: ["a0"],
			neighbor_tasks: [],
		};
		const hall = describeFlowEntity(assignment, { kind: "node", id: "a0" });
		expect(hall?.rows).toContainEqual({
			label: "Hall witness",
			value: "deficient agent set S",
		});
	});

	it("exposes transportation partition, basis, cycle sign, prices, and cut witness", () => {
		const transportation = scene();
		transportation.solve_status = "infeasible";
		transportation.model = {
			kind: "transportation",
			origins: ["o0"],
			destinations: ["d0"],
		};
		transportation.graph = {
			nodes: [
				{ id: "o0", supply: "2" },
				{ id: "d0", supply: "-2" },
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
		transportation.algorithm = { id: "modi", config: {} };
		transportation.edge_states = [{ edge_id: "route", flow: "2" }];
		transportation.residual_arcs = [
			{
				edge_id: "route",
				direction: "forward",
				from: "o0",
				to: "d0",
				capacity: "0",
				cost: "3",
				active: false,
				fixed: false,
			},
			{
				edge_id: "route",
				direction: "reverse",
				from: "d0",
				to: "o0",
				capacity: "2",
				cost: "-3",
				active: true,
				fixed: false,
			},
		];
		transportation.node_trace_states = [
			{ node_id: "o0", label: "0", search_ordinal: 1 },
			{ node_id: "d0", label: "3", remaining_divergence: "0" },
		];
		transportation.pseudoflow_forest = {
			arcs: [{ edge_id: "route", direction: "forward" }],
			strong_nodes: [],
		};
		transportation.outcome = {
			kind: "infeasible",
			unsatisfied: "2",
			reachable_original_nodes: ["o0"],
		};

		const origin = describeFlowEntity(transportation, {
			kind: "node",
			id: "o0",
		});
		expect(origin?.rows).toContainEqual({
			label: "Transportation partition",
			value: "origin",
		});
		expect(origin?.rows).toContainEqual({
			label: "Cut side",
			value: "reachable witness side",
		});
		expect(origin?.rows).toContainEqual({
			label: "Row potential u",
			value: "0",
		});
		const edge = describeFlowEntity(transportation, {
			kind: "edge",
			id: "route",
		});
		expect(edge?.rows).toContainEqual({
			label: "Basis",
			value: "basic route",
		});
		expect(edge?.rows).toContainEqual({
			label: "Closed-loop change",
			value: "−θ (decrease)",
		});
		expect(edge?.rows).toContainEqual({
			label: "Witness boundary",
			value: "crosses cut",
		});
	});

	it("names Auction scaled values and selection ordinals by partition", () => {
		const auction = scene();
		auction.model = {
			kind: "assignment",
			agents: ["a0"],
			tasks: ["t0"],
			objective: "maximize",
		};
		auction.graph.nodes = [
			{ id: "a0", supply: "0" },
			{ id: "t0", supply: "0" },
		];
		auction.graph.edges = [];
		auction.edge_states = [];
		auction.residual_arcs = [];
		auction.algorithm = { id: "auction", config: {} };
		auction.node_trace_states = [
			{ node_id: "a0", label: "-3", search_ordinal: 0 },
			{ node_id: "t0", label: "12", search_ordinal: 1 },
		];
		delete auction.outcome;

		expect(
			describeFlowEntity(auction, { kind: "node", id: "a0" })?.rows,
		).toContainEqual({ label: "Scaled net benefit βₛ", value: "-3" });
		expect(
			describeFlowEntity(auction, { kind: "node", id: "a0" })?.rows,
		).toContainEqual({ label: "Selection ordinal", value: "0" });
		expect(
			describeFlowEntity(auction, { kind: "node", id: "t0" })?.rows,
		).toContainEqual({ label: "Scaled task price pₛ", value: "12" });
	});

	it("explains IBFS tree side, signed distance, orphan queue, and parent edge", () => {
		const ibfs = scene();
		ibfs.algorithm = { id: "ibfs", config: {} };
		ibfs.node_trace_states = [
			{ node_id: "s", label: "0" },
			{ node_id: "alpha", label: "1", search_ordinal: 0 },
			{ node_id: "t", label: "-1" },
		];
		ibfs.pseudoflow_forest = {
			arcs: [{ edge_id: "sa", direction: "forward" }],
			strong_nodes: [],
		};
		ibfs.trace_event = {
			event_id: "2",
			parent_phase_id: "1",
			catalog_id: "ibfs.augment-shortest-path",
			minimum_granularity: "operation",
			pseudocode_line: "ibfs:augment-and-create-orphans",
			patch_count: 1,
			entity_refs: [],
			detail: { label: "bottleneck", value: "1" },
		};

		const node = describeFlowEntity(ibfs, { kind: "node", id: "alpha" });
		expect(node?.rows).toContainEqual({ label: "IBFS tree", value: "S tree" });
		expect(node?.rows).toContainEqual({
			label: "IBFS distance",
			value: "S · dₛ 1",
		});
		expect(node?.rows).toContainEqual({
			label: "IBFS state",
			value: "orphan · awaiting adoption",
		});
		const edge = describeFlowEntity(ibfs, { kind: "edge", id: "sa" });
		expect(edge?.rows).toContainEqual({
			label: "IBFS forest",
			value: "S tree · forward parent→child",
		});
	});

	it("explains an IBFS removal focus after the node becomes free", () => {
		const ibfs = scene();
		ibfs.algorithm = { id: "ibfs", config: {} };
		ibfs.node_trace_states = [
			{ node_id: "s", label: "0" },
			{ node_id: "alpha", search_ordinal: 0 },
			{ node_id: "t", label: "-1" },
		];
		ibfs.pseudoflow_forest = { arcs: [], strong_nodes: [] };
		ibfs.trace_event = {
			event_id: "3",
			parent_phase_id: "1",
			catalog_id: "ibfs.remove-source-orphan",
			minimum_granularity: "operation",
			pseudocode_line: "ibfs:remove-orphan-beyond-current-boundary",
			patch_count: 1,
			entity_refs: [],
			detail: { label: "distance", value: "1" },
		};

		const node = describeFlowEntity(ibfs, { kind: "node", id: "alpha" });
		expect(node?.rows).toContainEqual({
			label: "IBFS tree",
			value: "outside both trees",
		});
		expect(node?.rows).toContainEqual({
			label: "IBFS state",
			value: "removed from tree · repair focus",
		});
	});

	it("explains EIBFS membership, retained labels, imbalance, and residual orientation", () => {
		const eibfs = scene();
		eibfs.algorithm = { id: "eibfs", config: {} };
		eibfs.node_trace_states = [
			{ node_id: "s", label: "0" },
			{ node_id: "alpha", label: "-2", search_ordinal: 0 },
			{ node_id: "t", label: "0" },
		];
		eibfs.eibfs_overlay = {
			phase_direction: "forward",
			source_depth: "1",
			sink_depth: "0",
			nodes: [
				{
					node_id: "s",
					source_label: "0",
					sink_label: "0",
					membership: "source",
					root_kind: "source",
					orphan: false,
					imbalance: "0",
				},
				{
					node_id: "alpha",
					source_label: "1",
					sink_label: "2",
					membership: "source",
					root_kind: "none",
					orphan: false,
					imbalance: "0",
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
			],
			forest_arcs: [
				{
					parent: "s",
					child: "alpha",
					side: "source",
					admissible_residual: { edge_id: "sa", direction: "forward" },
				},
			],
		};
		eibfs.trace_event = {
			event_id: "2",
			parent_phase_id: "1",
			catalog_id: "eibfs.adopt-source",
			minimum_granularity: "operation",
			pseudocode_line: "eibfs:adopt-source",
			patch_count: 2,
			entity_refs: [{ kind: "node", node_id: "alpha" }],
		};

		const node = describeFlowEntity(eibfs, {
			kind: "node",
			id: "alpha",
		});
		expect(node?.rows).toContainEqual({
			label: "EIBFS membership",
			value: "S forest",
		});
		expect(node?.rows).toContainEqual({
			label: "Retained labels dₛ / dₜ",
			value: "1 / 2",
		});
		expect(node?.rows).toContainEqual({
			label: "Pseudoflow imbalance e",
			value: "0",
		});
		expect(node?.rows).toContainEqual({
			label: "EIBFS state",
			value: "repair focus",
		});

		const edge = describeFlowEntity(eibfs, { kind: "edge", id: "sa" });
		expect(edge?.rows).toContainEqual({
			label: "EIBFS forest",
			value: "S forest · s → alpha",
		});
		expect(edge?.rows).toContainEqual({
			label: "Admissible residual",
			value: "sa:forward · parent→child",
		});
		expect(edge?.rows.some((row) => row.label === "IBFS forest")).toBe(false);
	});
});
