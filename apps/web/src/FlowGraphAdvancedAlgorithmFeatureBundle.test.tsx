import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { FlowGraphAdvancedAlgorithmFeatureBundle } from "./FlowGraphAdvancedAlgorithmFeatureBundle";
import { FlowGraphIdScopeProvider } from "./flow-dom-id";

const route = {
	path: "M 100 100 L 300 100",
	reversePath: "M 300 100 L 100 100",
	routeMidpoint: { x: 200, y: 100 },
};

function stateWith(
	overlays: Record<string, unknown>,
	metrics: readonly string[] = Array.from({ length: 16 }, () => "0"),
	graph: {
		edges: Array<{ capacity: string; from: string; id: string; to: string }>;
		nodes: Array<{ id: string }>;
		positions: Map<string, { x: number; y: number }>;
		routes: Map<string, typeof route>;
	} = {
		edges: [{ id: "e", from: "s", to: "t", capacity: "8" }],
		nodes: [{ id: "s" }, { id: "t" }],
		positions: new Map([
			["s", { x: 100, y: 100 }],
			["t", { x: 300, y: 100 }],
		]),
		routes: new Map([["e", route]]),
	},
) {
	return {
		renderData: {
			overlayViews: {
				interiorPointMaxFlow: undefined,
				flowFrameworkMcf: undefined,
				minimumRatioCycleMcf: undefined,
				primalDualIpmMcf: undefined,
				randomizedAlmostLinearMcf: undefined,
				weightedAugmentingPaths: undefined,
				weightedPushRelabelShortcut: undefined,
				...overlays,
			},
		},
		layout: { routes: graph.routes },
		positions: graph.positions,
		plan: {
			context: {
				model: { kind: "max-flow", source: "s", sink: "t" },
				metrics,
			},
			nodes: graph.nodes,
			edges: graph.edges,
		},
	} as never;
}

function render(
	overlays: Record<string, unknown>,
	metrics?: readonly string[],
	graph?: Parameters<typeof stateWith>[2],
): string {
	return renderToStaticMarkup(
		<FlowGraphIdScopeProvider scope="advanced-test">
			<svg>
				<title>Advanced algorithm graph projection</title>
				<FlowGraphAdvancedAlgorithmFeatureBundle
					state={stateWith(overlays, metrics, graph)}
				/>
			</svg>
		</FlowGraphIdScopeProvider>,
	);
}

function weightedPushOverlay(
	stage: string,
	overrides: Record<string, unknown> = {},
) {
	return {
		stage,
		demand: "448",
		sparse_cut_level: "2",
		nodes: [
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
				component: "0",
				order: "2",
				label: "0",
				alive: true,
				sparse_cut_side: false,
				source_side: false,
			},
			{
				node_id: "shortcut:0",
				original: false,
				component: "0",
				order: "0",
				label: "0",
				alive: true,
				sparse_cut_side: false,
				source_side: false,
			},
		],
		edges: [
			{
				edge_id: "e",
				kind: "original",
				from: "s",
				to: "t",
				capacity: "8",
				flow: "8",
				weight: "4",
			},
			{
				edge_id: "shortcut-edge:0",
				kind: "shortcut",
				from: "s",
				to: "shortcut:0",
				capacity: "8",
				flow: "0",
				weight: "2",
				shortcut_component: "0",
			},
		],
		residual_arcs: [
			{
				edge_id: "e",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "8",
				weight: "4",
				admissible: false,
				active: false,
			},
			{
				edge_id: "shortcut-edge:0",
				direction: "forward",
				from: "s",
				to: "shortcut:0",
				capacity: "8",
				weight: "2",
				admissible: false,
				active: false,
			},
		],
		active_path: [],
		inspected_arcs: [],
		active_relabel_nodes: [],
		...overrides,
	};
}

describe("advanced algorithm main-canvas feature bundle", () => {
	it.each([
		[
			"augmenting_electrical_overlay",
			{
				augmentingElectrical: {
					stage: "solve-electrical-pivot",
					working_nodes: "2",
					active_pivot_node: "0",
					nodes: [{ node_id: "s" }, { node_id: "t" }],
				},
			},
		],
		[
			"interior_point_max_flow_overlay",
			{
				interiorPointMaxFlow: {
					stage: "build-b-matching-reduction",
					edges: [{ edge_id: "e", normalized_away: false }],
				},
			},
		],
		[
			"flow_framework_mcf_overlay",
			{
				flowFrameworkMcf: {
					stage: "source-progress",
					dynamic_operation: "flow-applied",
					dynamic_operation_serial: "3",
					iteration: "1",
					exact_gap_after: { numerator: "1", denominator: "2" },
					levels: [{ level: "0", active_branch: "query", passes: "2" }],
					edges: [
						{
							edge_id: "e",
							flow: { numerator: "1", denominator: "2" },
							cycle_coefficient: { numerator: "1", denominator: "1" },
							selected: true,
						},
					],
				},
			},
		],
		[
			"minimum_ratio_cycle_mcf_overlay",
			{
				minimumRatioCycleMcf: {
					stage: "apply-cycle",
					edges: [
						{
							edge_id: "e",
							fixed_on_face: false,
							gradient: "2",
							length: "3",
							tree_edge: true,
							candidate_sign: "1",
							selected_sign: "1",
						},
					],
					nodes: [
						{
							node_id: "s",
							component: "0",
							depth: "0",
							candidate_balance: "0",
							on_candidate: true,
							on_selected: true,
						},
					],
				},
			},
		],
		[
			"primal_dual_ipm_mcf_overlay",
			{
				primalDualIpmMcf: {
					stage: "sample-fundamental-cycle",
					sampled_arc: "a",
					nodes: [
						{
							auxiliary_id: "node:s",
							kind: "original",
							original_node_id: "s",
							potential: "0",
							component: "0",
							in_crossover_set: false,
						},
						{
							auxiliary_id: "capacity:e",
							kind: "capacity",
							original_edge_id: "e",
							potential: "1",
							component: "0",
							in_crossover_set: false,
						},
					],
					arcs: [
						{
							auxiliary_id: "a",
							original_edge_id: "e",
							from: "node:s",
							to: "capacity:e",
							kind: "upper",
							flow: "2",
							slack: "3",
							deleted: false,
							contracted: false,
							in_minor: true,
							in_tree: true,
							forest_candidate: true,
							active_cycle_sign: "1",
						},
					],
				},
			},
		],
		[
			"randomized_almost_linear_mcf_overlay",
			{
				randomizedAlmostLinearMcf: {
					stage: "query-minimum-ratio-cycle",
					edges: [
						{
							edge_id: "e",
							fixed_on_face: false,
							current_flow: "2",
							stale_flow: "1",
							isolation_draw: "7",
							isolated_cost: "19",
							tree_edge: true,
							candidate_sign: "1",
							selected_sign: "1",
							gradient: "-2",
							length: "3",
							detected: true,
						},
					],
					nodes: [{ node_id: "s", depth: "0", on_selected_cycle: true }],
				},
			},
		],
		[
			"weighted_augmenting_paths_overlay",
			{
				weightedAugmentingPaths: {
					stage: "augment-path",
					residual_arcs: [
						{
							edge_id: "e",
							direction: "forward",
							capacity: "4",
							weight: "2",
							hierarchy_kind: "dag",
							admissible: true,
							active: true,
						},
					],
					nodes: [
						{
							node_id: "s",
							component: "0",
							order: "1",
							label: "2",
							alive: true,
							expansion_witness_side: false,
							source_side: true,
						},
					],
				},
			},
		],
		[
			"weighted_push_relabel_shortcut_overlay",
			{
				weightedPushRelabelShortcut: {
					stage: "relabel-checkpoint",
					nodes: [
						{
							node_id: "s",
							original: true,
							component: "0",
							label: "2",
						},
						{
							node_id: "shortcut:0",
							original: false,
							component: "0",
							label: "0",
						},
					],
					edges: [
						{
							edge_id: "shortcut-edge",
							kind: "shortcut",
							from: "s",
							to: "shortcut:0",
						},
					],
					residual_arcs: [
						{
							edge_id: "shortcut-edge",
							direction: "forward",
							from: "s",
							to: "shortcut:0",
							capacity: "4",
							weight: "2",
							admissible: true,
							active: true,
						},
					],
					active_path: [],
					inspected_arcs: [{ edge_id: "shortcut-edge", direction: "forward" }],
					active_relabel_nodes: ["s"],
				},
			},
		],
	] as const)("paints %s on the graph instead of status alone", (field, overlays) => {
		const svg = render(overlays);
		expect(svg).toContain(`data-overlay-contribution="${field}"`);
		expect(svg).toContain('data-overlay-feature-bundle="advanced-algorithm"');
		expect(svg).toMatch(/data-overlay-entity-kind="[^"]+"/u);
		expect(svg).toMatch(/data-overlay-entity-id="[^"]+"/u);
		expect(svg).toMatch(/data-overlay-role="[^"]+"/u);
	});

	it("renders adjacent dynamic source operations as distinct edge-owned graph states", () => {
		const base = {
			stage: "source-progress",
			iteration: "2",
			exact_gap_after: { numerator: "7", denominator: "4" },
			levels: [
				{ level: "0", active_branch: "1", passes: "3" },
				{ level: "1", active_branch: "0", passes: "2" },
			],
			edges: [
				{
					edge_id: "e",
					flow: { numerator: "3", denominator: "4" },
					cycle_coefficient: { numerator: "1", denominator: "1" },
					selected: true,
				},
			],
		};
		const applied = render({
			flowFrameworkMcf: {
				...base,
				dynamic_operation: "flow-applied",
				dynamic_operation_serial: "17",
			},
		});
		const completed = render({
			flowFrameworkMcf: {
				...base,
				dynamic_operation: "completed",
				dynamic_operation_serial: "18",
			},
		});

		expect(applied).toContain("DYN #17 · APPLY FLOW");
		expect(applied).toContain('data-overlay-role="dynamic-flow-applied"');
		expect(applied).toContain('data-overlay-role="source-progress-flow"');
		expect(applied).not.toContain('data-overlay-role="selected-cycle"');
		expect(completed).toContain("DYN #18 · RETURN FLOW");
		expect(completed).toContain('data-overlay-role="dynamic-completed"');
		expect(completed).toContain('data-overlay-entity-id="e"');
		expect(completed).not.toBe(applied);
	});

	it("renders each feasible-assignment checkpoint as an exact edge-flow change", () => {
		const assignment = (serial: string, flow: string) => ({
			stage: "inspect-feasible-assignment",
			assignment_cursor: "e",
			assignment_serial: serial,
			edges: [
				{
					edge_id: "e",
					fixed_on_face: false,
					current_flow: flow,
					stale_flow: "0",
					isolation_draw: "0",
					isolated_cost: "0",
					tree_edge: false,
					candidate_sign: "0",
					selected_sign: "0",
					gradient: "0",
					length: "0",
					detected: false,
				},
			],
			nodes: [],
		});
		const first = render({
			randomizedAlmostLinearMcf: assignment("1", "0"),
		});
		const second = render({
			randomizedAlmostLinearMcf: assignment("2", "4"),
		});

		expect(first).toContain("ASSIGN #1 · f=0");
		expect(first).toContain('data-overlay-role="feasible-assignment-zero"');
		expect(second).toContain("ASSIGN #2 · f=4");
		expect(second).toContain('data-overlay-role="feasible-assignment-flow"');
		expect(second).toContain(
			'data-overlay-role="feasible-assignment-checkpoint"',
		);
		expect(second).toContain('data-overlay-entity-id="e"');
		expect(second).not.toBe(first);
	});

	it("shows the sampled isolation costs and the actual selected optimum as different edge states", () => {
		const edge = {
			edge_id: "e",
			fixed_on_face: false,
			current_flow: "2",
			stale_flow: "2",
			isolation_draw: "7",
			isolated_cost: "103",
			tree_edge: false,
			candidate_sign: "0",
			selected_sign: "0",
			gradient: "0",
			length: "0",
			detected: false,
		};
		const sampled = render({
			randomizedAlmostLinearMcf: {
				stage: "sample-isolation-costs",
				isolation_attempt: "1",
				edges: [edge],
				nodes: [],
			},
		});
		const selected = render({
			randomizedAlmostLinearMcf: {
				stage: "select-isolated-optimum",
				isolated_optimum_cost: "206",
				edges: [{ ...edge, isolated_optimum_flow: "2" }],
				nodes: [],
			},
		});

		expect(sampled).toContain('data-overlay-role="isolation-draw-coordinate"');
		expect(sampled).toContain("z=7 · TRY #1");
		expect(selected).toContain('data-overlay-role="isolated-optimum-flow"');
		expect(selected).toContain("ISO f=2 · C*=206");
		expect(selected).not.toBe(sampled);
	});

	it("attaches the minimum-ratio DFS completion certificate to one canonical node", () => {
		const overlay = {
			edges: [
				{
					edge_id: "e",
					fixed_on_face: false,
					gradient: "2",
					length: "3",
					tree_edge: true,
					candidate_sign: "0",
					selected_sign: "1",
				},
			],
			nodes: [
				{
					node_id: "s",
					component: "0",
					depth: "0",
					candidate_balance: "0",
					on_candidate: false,
					on_selected: true,
				},
			],
		};
		const checking = render({
			minimumRatioCycleMcf: { ...overlay, stage: "check-dfs-oracle" },
		});
		const complete = render({
			minimumRatioCycleMcf: { ...overlay, stage: "complete" },
		});

		expect(checking).not.toContain('data-overlay-role="dfs-oracle-certified"');
		expect(complete).toContain('data-overlay-role="dfs-oracle-certified"');
		expect(complete).not.toBe(checking);
	});

	it("anchors the exact fixed-face dimension to a real edge coordinate", () => {
		const overlay = {
			edges: [
				{
					edge_id: "e",
					fixed_on_face: false,
					gradient: "0",
					length: "0",
					tree_edge: false,
					candidate_sign: "0",
					selected_sign: "0",
				},
			],
			nodes: [],
		};
		const enumerated = render({
			minimumRatioCycleMcf: {
				...overlay,
				stage: "enumerate-feasible-set",
			},
		});
		const contracted = render({
			minimumRatioCycleMcf: {
				...overlay,
				stage: "contract-fixed-face",
			},
		});

		expect(enumerated).not.toContain('data-overlay-role="fixed-face-summary"');
		expect(contracted).toContain('data-overlay-role="fixed-face-summary"');
		expect(contracted).toContain('data-overlay-entity-id="e"');
		expect(contracted).toContain("1 ACTIVE · 0 FIXED");
		expect(contracted).not.toBe(enumerated);
	});

	it("anchors source potential and independent DFS results to an edge", () => {
		const overlay = {
			current_potential: "12.5",
			potential_decrease: "0.25",
			guaranteed_decrease: "0.125",
			fundamental_cycles: "1",
			simple_cycles: "2",
			enumerated_vectors: "8",
			edges: [
				{
					edge_id: "e",
					fixed_on_face: false,
					gradient: "0",
					length: "0",
					tree_edge: true,
					candidate_sign: "0",
					selected_sign: "1",
				},
			],
			nodes: [],
		};
		const potential = render({
			minimumRatioCycleMcf: { ...overlay, stage: "evaluate-potential" },
		});
		const dfs = render({
			minimumRatioCycleMcf: { ...overlay, stage: "check-dfs-oracle" },
		});

		expect(potential).toContain('data-overlay-role="source-potential"');
		expect(potential).toContain("Φ 12.5");
		expect(dfs).toContain('data-overlay-role="dfs-oracle-check"');
		expect(dfs).toContain("DFS = SOURCE");
		expect(dfs).not.toBe(potential);
	});

	it("changes the graph-bound forest-subset witness on every exact subset ordinal", () => {
		const overlay = {
			stage: "inspect-forest-subset",
			sampled_arc: undefined,
			nodes: [
				{
					auxiliary_id: "node:s",
					kind: "original",
					original_node_id: "s",
					potential: "0",
					component: "0",
					in_crossover_set: false,
				},
				{
					auxiliary_id: "capacity:e",
					kind: "capacity",
					original_edge_id: "e",
					potential: "1",
					component: "0",
					in_crossover_set: false,
				},
			],
			arcs: [
				{
					auxiliary_id: "a",
					original_edge_id: "e",
					from: "node:s",
					to: "capacity:e",
					kind: "upper",
					flow: "2",
					slack: "3",
					deleted: false,
					contracted: false,
					in_minor: true,
					in_tree: false,
					forest_candidate: true,
					active_cycle_sign: "0",
				},
			],
		};
		const first = render({
			primalDualIpmMcf: { ...overlay, forest_subset_serial: "1" },
		});
		const second = render({
			primalDualIpmMcf: { ...overlay, forest_subset_serial: "2" },
		});
		const empty = render({
			primalDualIpmMcf: {
				...overlay,
				forest_subset_serial: "3",
				arcs: overlay.arcs.map((arc) => ({
					...arc,
					forest_candidate: false,
				})),
			},
		});

		expect(first).toContain('data-overlay-role="forest-subset-enumeration"');
		expect(first).toContain('data-overlay-entity-id="a"');
		expect(first).toContain("FOREST #1 · 1 ARC");
		expect(second).toContain("FOREST #2 · 1 ARC");
		expect(second).not.toBe(first);
		expect(empty).toContain("FOREST #3 · ∅");
		expect(empty).toContain('data-overlay-entity-id="node:s"');
		expect(empty).toContain('data-overlay-original-node-id="s"');
	});

	it("publishes exact minor and barrier transitions on one auxiliary arc", () => {
		const overlay = {
			mu: "1024",
			beta: "8",
			gamma: "4",
			proxy_gap: "16",
			centrality_numerator: "2",
			cycle_alpha: "0",
			sampled_arc: undefined,
			nodes: [
				{
					auxiliary_id: "node:s",
					kind: "original",
					original_node_id: "s",
					potential: "0",
					component: "0",
					in_crossover_set: false,
				},
				{
					auxiliary_id: "capacity:e",
					kind: "capacity",
					original_edge_id: "e",
					potential: "1",
					component: "0",
					in_crossover_set: false,
				},
			],
			arcs: [
				{
					auxiliary_id: "a",
					original_edge_id: "e",
					from: "node:s",
					to: "capacity:e",
					kind: "upper",
					flow: "2",
					slack: "3",
					deleted: false,
					contracted: false,
					in_minor: true,
					in_tree: false,
					forest_candidate: false,
					active_cycle_sign: "0",
				},
			],
		};
		const minor = render({
			primalDualIpmMcf: { ...overlay, stage: "build-minor" },
		});
		const decreased = render({
			primalDualIpmMcf: { ...overlay, stage: "decrease-mu", mu: "512" },
		});

		expect(minor).toContain('data-overlay-role="minor-summary"');
		expect(minor).toContain("MINOR 1 · D0/C0");
		expect(decreased).toContain('data-overlay-role="barrier-decrease"');
		expect(decreased).toContain("μ 512");
		expect(decreased).not.toBe(minor);
	});

	it("publishes the zero-centered path as a distinct graph state", () => {
		const reduction = {
			interiorPointMaxFlow: {
				stage: "build-min-cost-reduction",
				edges: [{ edge_id: "e", normalized_away: false }],
			},
		};
		const initialized = {
			interiorPointMaxFlow: {
				...reduction.interiorPointMaxFlow,
				stage: "initialize-central-path",
			},
		};
		const reductionSvg = render(reduction);
		const initializedSvg = render(initialized);

		expect(reductionSvg).toContain('data-overlay-role="working-hub-coupling"');
		expect(initializedSvg).toContain(
			'data-overlay-role="initialized-central-path"',
		);
		expect(initializedSvg).not.toBe(reductionSvg);
	});

	it("retires the working reduction and turns the IPM cut check into a certificate", () => {
		const overlay = {
			edges: [{ edge_id: "e", normalized_away: false }],
			nodes: [
				{ node_id: "s", target_source_side: true },
				{ node_id: "t", target_source_side: false },
			],
			target_value: "8",
		};
		const rounded = render({
			interiorPointMaxFlow: { ...overlay, stage: "round-integral-flow" },
		});
		const checking = render({
			interiorPointMaxFlow: { ...overlay, stage: "check-certificate" },
		});
		const certified = render({
			interiorPointMaxFlow: { ...overlay, stage: "optimal" },
		});

		expect(rounded).not.toContain(
			'data-overlay-role="initialized-central-path"',
		);
		expect(checking).toContain('data-overlay-role="certificate-cut-check"');
		expect(checking).toContain("CHECK · FLOW = CUT = 8 · 1 CUT EDGE");
		expect(certified).toContain('data-overlay-role="certificate-cut-verified"');
		expect(certified).toContain("CERTIFIED · FLOW = CUT = 8 · 1 CUT EDGE");
		expect(certified).not.toBe(checking);
	});

	it("reveals the exact respecting-order edge weight at assign-weights", () => {
		const overlay = {
			residual_arcs: [
				{
					edge_id: "e",
					direction: "forward",
					capacity: "8",
					weight: "8",
					hierarchy_kind: "dag",
					admissible: false,
					active: false,
				},
			],
			nodes: [
				{
					node_id: "s",
					component: "0",
					order: "1",
					label: "0",
					alive: true,
					expansion_witness_side: false,
					source_side: true,
				},
			],
		};
		const certified = render({
			weightedAugmentingPaths: {
				...overlay,
				stage: "certify-expansion",
			},
		});
		const assigned = render({
			weightedAugmentingPaths: {
				...overlay,
				stage: "assign-weights",
			},
		});

		expect(certified).toContain("--flow-advanced-width:1.5");
		expect(assigned).not.toContain("--flow-advanced-width:1.5");
		expect(assigned).toContain("residual 8, weight 8, hierarchy-dag");
	});

	it("separates hierarchy, shortcut construction, and weight assignment", () => {
		const hierarchy = render({
			weightedPushRelabelShortcut: weightedPushOverlay("build-weak-hierarchy"),
		});
		const shortcut = render({
			weightedPushRelabelShortcut: weightedPushOverlay("build-shortcut-graph"),
		});
		const weighted = render({
			weightedPushRelabelShortcut: weightedPushOverlay("assign-weights"),
		});

		expect(hierarchy).not.toContain('data-overlay-role="steiner-root"');
		expect(hierarchy).not.toContain('data-overlay-role="shortcut-arc"');
		expect(shortcut).toContain('data-overlay-role="steiner-root"');
		expect(shortcut).toContain('data-overlay-role="shortcut-arc"');
		expect(shortcut).toContain("--flow-advanced-width:1.5");
		expect(weighted).not.toContain("--flow-advanced-width:1.5");
	});

	it("anchors literal relabel and inspection progress to their graph owners", () => {
		const relabeled = render({
			weightedPushRelabelShortcut: weightedPushOverlay("relabel-checkpoint", {
				nodes: weightedPushOverlay("ready").nodes.map((node) =>
					node.node_id === "s" ? { ...node, label: "17" } : node,
				),
				active_relabel_nodes: ["s"],
			}),
		});
		const metrics = Array.from({ length: 16 }, () => "0");
		metrics[3] = "129";
		const inspected = render(
			{
				weightedPushRelabelShortcut: weightedPushOverlay(
					"inspect-primitive-arc-checkpoint",
					{
						inspected_arcs: [{ edge_id: "e", direction: "forward" }],
					},
				),
			},
			metrics,
		);

		expect(relabeled).toContain('data-overlay-role="active-relabel-level"');
		expect(relabeled).toContain("ℓ17");
		expect(inspected).toContain('data-overlay-role="inspection-progress"');
		expect(inspected).toContain('data-overlay-role="inspection-count"');
		expect(inspected).toContain("i129");
	});

	it("shows terminal demand, the selected sparse cut, and certificate states locally", () => {
		const demand = render({
			weightedPushRelabelShortcut: weightedPushOverlay("initialize-demand"),
		});
		const sparseCut = render({
			weightedPushRelabelShortcut: weightedPushOverlay("select-sparse-cut", {
				residual_arcs: weightedPushOverlay("ready").residual_arcs.map(
					(arc, index) => (index === 0 ? { ...arc, capacity: "0" } : arc),
				),
			}),
		});
		const candidate = render({
			weightedPushRelabelShortcut: weightedPushOverlay(
				"complete-residual-rounds",
			),
		});
		const checking = render({
			weightedPushRelabelShortcut: weightedPushOverlay("check-certificate"),
		});
		const verified = render({
			weightedPushRelabelShortcut: weightedPushOverlay("optimal"),
		});

		expect(demand).toContain('data-overlay-role="demand-source"');
		expect(demand).toContain('data-overlay-role="demand-sink"');
		expect(demand).toContain("+448");
		expect(demand).toContain("−448");
		expect(sparseCut).toContain('data-overlay-role="sparse-cut-boundary"');
		expect(sparseCut).toContain("residual cut, capacity 0");
		expect(candidate).toContain(
			'data-overlay-role="certificate-cut-candidate"',
		);
		expect(checking).toContain('data-overlay-role="certificate-cut-check"');
		expect(verified).toContain('data-overlay-role="certificate-cut-verified"');
	});

	it("publishes a zero short-flow measurement on the terminal pair", () => {
		const measured = render({
			weightedPushRelabelShortcut: weightedPushOverlay("measure-short-flow", {
				routed: "0",
				weighted_length: "0",
				weighted_length_units: "1",
			}),
		});

		expect(measured).toContain('data-overlay-role="short-flow-measure-rail"');
		expect(measured).toContain('data-overlay-role="short-flow-measure-routed"');
		expect(measured).toContain("SHORT FLOW 0/448 · AVG 0/1");
	});

	it("makes every dense-elimination pivot and its graph owner visible", () => {
		const overlay = {
			augmentingElectrical: {
				stage: "solve-electrical-pivot",
				working_nodes: "3",
				active_pivot_node: "0",
				nodes: [{ node_id: "s" }, { node_id: "t" }],
			},
		};
		const fourth = render(overlay, ["0", "0", "4"]);
		const fifth = render(overlay, ["0", "0", "5"]);
		expect(fourth).toContain("PIVOT #4");
		expect(fifth).toContain("PIVOT #5");
		expect(fourth).not.toBe(fifth);
		expect(fourth).toContain(
			'data-overlay-entity-kind="node" data-overlay-entity-id="s" data-overlay-role="active-pivot-equation"',
		);
		expect(fourth).toContain('data-overlay-role="active-pivot-owner"');
	});

	it("keeps a boost-vertex pivot visibly auxiliary", () => {
		const svg = render(
			{
				augmentingElectrical: {
					stage: "solve-electrical-pivot",
					working_nodes: "3",
					active_pivot_node: "2",
					nodes: [{ node_id: "s" }, { node_id: "t" }],
				},
			},
			["0", "0", "7"],
		);
		expect(svg).toContain(
			'data-overlay-entity-kind="auxiliary-node" data-overlay-entity-id="working-node:2" data-overlay-role="active-pivot-equation"',
		);
		expect(svg).toContain(
			'data-augmenting-working-node-ids="s|t|working-node:2"',
		);
	});

	it("anchors the exact preconditioner bank between the max-flow terminals", () => {
		const svg = render({
			augmentingElectrical: {
				stage: "add-preconditioning",
				working_edges: "6",
				working_target: "11",
				nodes: [{ node_id: "s" }, { node_id: "t" }],
			},
		});
		expect(svg).toContain('data-overlay-entity-id="preconditioner-bank"');
		expect(svg).toContain('data-augmenting-preconditioner-count="3"');
		expect(svg).toContain("3× PRECONDITIONER · TARGET 11");
	});

	it("draws the exact cleanup path, order, and post-augmentation flow", () => {
		const metrics = Array.from({ length: 16 }, () => "0");
		metrics[8] = "4";
		const svg = render(
			{
				augmentingElectrical: {
					stage: "cleanup-augmenting-path",
					active_working_path: [
						{
							edge: "9",
							direction: "forward",
							from_node: "s",
							to_node: "t",
							flow_after: "7",
						},
					],
					active_discrete_amount: "2",
				},
			},
			metrics,
		);
		expect(svg).toContain('data-augmenting-cleanup-serial="4"');
		expect(svg).toContain('data-augmenting-working-edge-ids="working-edge:9"');
		expect(svg).toContain('data-overlay-role="cleanup-working-arc"');
		expect(svg).toContain("CLEANUP #4 · PUSH 2 · w9 x=7");
	});

	it("projects the three exact directed-reduction extraction components", () => {
		const svg = render({
			augmentingElectrical: {
				stage: "cancel-extraction-cycle",
				active_extraction_cycle: [{ edge: "0", kind: "toward-source" }],
				edges: [
					{
						edge_id: "e",
						extraction_central_scaled: "6",
						extraction_toward_source: "2",
						extraction_out_of_sink: "3",
					},
				],
			},
		});
		expect(svg).toContain(
			'data-augmenting-extraction-edge-ids="extraction:e:toward-source|extraction:e:out-of-sink"',
		);
		expect(svg).toContain("recovered central reduction amount 2f=6");
		expect(svg).toContain("head t → source s, auxiliary extraction amount 2");
		expect(svg).toContain('data-overlay-role="extraction-cycle-active"');
		expect(svg).toContain("sink t → tail s, auxiliary extraction amount 3");
	});

	it("joins canonical extraction ordinals to layout edges by stable identity", () => {
		const svg = render(
			{
				augmentingElectrical: {
					stage: "cancel-extraction-cycle",
					active_extraction_cycle: [{ edge: "0", kind: "central" }],
					edges: [
						{
							edge_id: "a-edge",
							extraction_central_scaled: "6",
							extraction_toward_source: "0",
							extraction_out_of_sink: "0",
						},
						{
							edge_id: "z-edge",
							extraction_central_scaled: "2",
							extraction_toward_source: "0",
							extraction_out_of_sink: "0",
						},
					],
				},
			},
			undefined,
			{
				edges: [
					{ id: "z-edge", from: "s", to: "m", capacity: "8" },
					{ id: "a-edge", from: "m", to: "t", capacity: "8" },
				],
				nodes: [{ id: "s" }, { id: "m" }, { id: "t" }],
				positions: new Map([
					["s", { x: 100, y: 100 }],
					["m", { x: 200, y: 100 }],
					["t", { x: 300, y: 100 }],
				]),
				routes: new Map([
					["z-edge", { ...route, path: "M 100 100 L 200 100" }],
					["a-edge", { ...route, path: "M 200 100 L 300 100" }],
				]),
			},
		);
		expect(svg).toContain(
			"a-edge: recovered central reduction amount 2f=6; active cancellation cycle",
		);
		expect(svg).toContain(
			"z-edge: recovered central reduction amount 2f=2</title>",
		);
	});

	it("turns an exact cut check into a distinct certified cut", () => {
		const overlay = {
			nodes: [
				{ node_id: "s", target_source_side: true },
				{ node_id: "t", target_source_side: false },
			],
			original_target: "8",
		};
		const checking = render({
			augmentingElectrical: { ...overlay, stage: "check-certificate" },
		});
		const certified = render({
			augmentingElectrical: { ...overlay, stage: "optimal" },
		});
		expect(checking).toContain('data-overlay-role="certificate-cut-check"');
		expect(checking).toContain("CHECK · FLOW = CUT = 8 · 1 CUT EDGE");
		expect(certified).toContain('data-overlay-role="certificate-cut-verified"');
		expect(certified).toContain("CERTIFIED · FLOW = CUT = 8 · 1 CUT EDGE");
		expect(certified).not.toBe(checking);
	});

	it("anchors a finished capacity-prefix cut to source and sink", () => {
		const metrics = Array.from({ length: 16 }, () => "0");
		metrics[10] = "5";
		const svg = render(
			{
				weightedAugmentingPaths: {
					stage: "finish-capacity-phase",
					phase: "2",
					phase_count: "4",
					capacity_bit: "1",
					residual_arcs: [],
					nodes: [
						{
							node_id: "s",
							label: "0",
							order: "1",
							component: "0",
							alive: true,
							expansion_witness_side: false,
							source_side: true,
						},
						{
							node_id: "t",
							label: "0",
							order: "2",
							component: "1",
							alive: true,
							expansion_witness_side: false,
							source_side: false,
						},
					],
				},
			},
			metrics,
		);
		expect(svg).toContain('data-weighted-capacity-check="5"');
		expect(svg).toContain("NO s→t PATH · PHASE 3/4 · BIT 1 · CUT #5");
		expect(svg).toContain('data-overlay-role="residual-cut-separator"');
	});

	it("keeps primal-dual auxiliary identity distinct from canonical graph identity", () => {
		const svg = render({
			primalDualIpmMcf: {
				stage: "sample-fundamental-cycle",
				nodes: [
					{
						auxiliary_id: "node:s",
						kind: "original",
						original_node_id: "s",
						potential: "0",
						component: "0",
						in_crossover_set: false,
					},
					{
						auxiliary_id: "capacity:e",
						kind: "capacity",
						original_edge_id: "e",
						potential: "0",
						component: "0",
						in_crossover_set: false,
					},
				],
				arcs: [],
			},
		});
		expect(svg).toContain(
			'data-overlay-entity-kind="auxiliary-node" data-overlay-entity-id="node:s" data-overlay-original-node-id="s"',
		);
		expect(svg).toContain(
			'data-overlay-entity-kind="auxiliary-node" data-overlay-entity-id="capacity:e" data-overlay-original-edge-id="e"',
		);
		expect(svg).not.toContain(
			'data-overlay-entity-kind="node" data-overlay-entity-id="node:s"',
		);
	});

	it("publishes residual edge identity and direction in separate attributes", () => {
		const svg = render({
			weightedAugmentingPaths: {
				stage: "augment-path",
				residual_arcs: [
					{
						edge_id: "e",
						direction: "reverse",
						capacity: "4",
						weight: "2",
						admissible: true,
						active: true,
					},
				],
				nodes: [],
			},
		});
		expect(svg).toContain(
			'data-overlay-entity-kind="residual-arc" data-overlay-entity-id="e" data-overlay-residual-direction="reverse"',
		);
		expect(svg).not.toContain('data-overlay-entity-id="e:reverse"');
	});
});
