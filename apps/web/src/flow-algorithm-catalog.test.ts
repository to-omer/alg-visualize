import { describe, expect, it } from "vitest";
import {
	DEFAULT_FLOW_ALGORITHM_CATALOG_FILTERS,
	decodeFlowAlgorithmCatalog,
	FLOW_ADMISSION_PRODUCT_SATURATION,
	type FlowAlgorithmCatalogEntry,
	filterFlowAlgorithmCatalog,
	filterFlowAlgorithmCatalogByFacets,
	flowAlgorithmSelectionReason,
	flowAlgorithmShapeReport,
	flowGraphAdmissionFacts,
	flowScenarioSelection,
	isFlowAlgorithmCompatible,
	modelProblemKind,
} from "./flow-algorithm-catalog";

function entry(id = "edmonds-karp"): FlowAlgorithmCatalogEntry {
	return {
		id,
		title: "Edmonds–Karp",
		aliases: ["Edmonds-Karp"],
		search_terms: [],
		kind: "variant",
		family: "augmenting-path",
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
		problems: ["max-flow"],
		models: ["max-flow"],
		runtime_route: "max-flow",
		graph_requirements: [],
		initial_construction: "zero-feasible",
		initial_optimality: "none",
		initial_oracle_dependency: "none",
		negative_cycle_policy: "not-applicable",
		terminal_oracle_dependency: "none",
		exact: true,
		randomized: false,
		complexity: "O(n m^2)",
		source_id: "edmonds-karp-1972",
		initial_band: { max_nodes: 2000, max_edges: 20000 },
		admission_contract: {
			min_nodes: null,
			min_edges: null,
			max_nodes: null,
			max_edges: null,
			max_capacity: null,
			max_absolute_cost: null,
			max_assignment_space: null,
			max_capacity_state_space: null,
			strict_interior_required: false,
			min_dynamic_capacity_updates: null,
			max_dynamic_capacity_updates: null,
			capacity_updates_only: false,
		},
		status: "executable",
		implementation_scope: "source-complete",
	};
}

describe("flow algorithm catalog contract", () => {
	it("strictly decodes a closed unique descriptor list", () => {
		expect(decodeFlowAlgorithmCatalog(JSON.stringify([entry()]))).toEqual([
			entry(),
		]);
		expect(() =>
			decodeFlowAlgorithmCatalog(
				JSON.stringify([{ ...entry(), future: true }]),
			),
		).toThrow(/invalid shape/);
		expect(() =>
			decodeFlowAlgorithmCatalog(JSON.stringify([entry(), entry()])),
		).toThrow(/duplicate/);
		expect(() =>
			decodeFlowAlgorithmCatalog(
				JSON.stringify([{ ...entry(), graph_requirements: ["future"] }]),
			),
		).toThrow(/invalid value/);
		expect(() =>
			decodeFlowAlgorithmCatalog(
				JSON.stringify([{ ...entry(), implementation_scope: "future" }]),
			),
		).toThrow(/invalid value/);
		expect(() =>
			decodeFlowAlgorithmCatalog(
				JSON.stringify([{ ...entry(), runtime_route: "future" }]),
			),
		).toThrow(/invalid value/);
		expect(() =>
			decodeFlowAlgorithmCatalog(
				JSON.stringify([
					{
						...entry(),
						trace_steps: {
							...entry().trace_steps,
							detail: { availability: "available", unit: "" },
						},
					},
				]),
			),
		).toThrow(/invalid value/);
		expect(() =>
			decodeFlowAlgorithmCatalog(
				JSON.stringify([
					{
						...entry(),
						admission_contract: {
							...entry().admission_contract,
							max_capacity: "01",
						},
					},
				]),
			),
		).toThrow(/invalid value/);
		for (const field of [
			"initial_construction",
			"initial_optimality",
			"initial_oracle_dependency",
			"negative_cycle_policy",
		] as const) {
			expect(() =>
				decodeFlowAlgorithmCatalog(
					JSON.stringify([{ ...entry(), [field]: "future" }]),
				),
			).toThrow(/invalid value/);
		}
	});

	it("reports and enforces graph-shape requirements independently of status", () => {
		const unitCapacity = {
			...entry("unit-capacity-dinic"),
			graph_requirements: ["unit-capacity"],
		} satisfies FlowAlgorithmCatalogEntry;
		const selection = flowScenarioSelection(
			JSON.stringify({
				payload: {
					model: { kind: "max-flow", source: "s", sink: "t" },
					algorithm: { id: "edmonds-karp" },
					graph: {
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
					},
				},
			}),
		);
		expect(selection?.graphShape?.unitCapacity).toBe(false);
		expect(
			flowAlgorithmShapeReport(unitCapacity, selection?.graphShape),
		).toEqual([{ requirement: "unit-capacity", status: "unsatisfied" }]);
		expect(
			flowAlgorithmSelectionReason(
				unitCapacity,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
			),
		).toBe("unit-capacity-required");
		expect(
			flowAlgorithmSelectionReason(
				{ ...unitCapacity, status: "planned" },
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
			),
		).toBe("planned");
	});

	it("maps advanced max-flow input contracts to typed selection failures", () => {
		const invalid = flowScenarioSelection(
			JSON.stringify({
				payload: {
					model: { kind: "max-flow", source: "s", sink: "s" },
					algorithm: { id: "electrical-flow" },
					graph: {
						nodes: [
							{ id: "s", supply: "0" },
							{ id: "t", supply: "0" },
							{ id: "isolated", supply: "0" },
						],
						edges: [
							{
								id: "st",
								from: "s",
								to: "t",
								lower: "0",
								capacity: "0",
								cost: "2",
							},
						],
					},
				},
			}),
		);
		const checks = [
			["positive-capacity", "positive-capacity-required"],
			["zero-cost", "zero-cost-required"],
			["distinct-terminals", "distinct-terminals-required"],
			["underlying-connected", "underlying-connected-required"],
		] as const;
		for (const [requirement, expected] of checks) {
			const descriptor = {
				...entry(`contract-${requirement}`),
				graph_requirements: [requirement],
			} satisfies FlowAlgorithmCatalogEntry;
			expect(
				flowAlgorithmSelectionReason(
					descriptor,
					invalid?.modelKind,
					invalid?.nodeCount,
					invalid?.edgeCount,
					invalid?.graphShape,
				),
			).toBe(expected);
		}

		const empty = flowScenarioSelection(
			JSON.stringify({
				payload: {
					model: { kind: "max-flow", source: "s", sink: "t" },
					algorithm: { id: "electrical-flow" },
					graph: {
						nodes: [
							{ id: "s", supply: "0" },
							{ id: "t", supply: "0" },
						],
						edges: [],
					},
				},
			}),
		);
		const nonEmpty = {
			...entry("contract-non-empty"),
			graph_requirements: ["non-empty-edges"],
		} satisfies FlowAlgorithmCatalogEntry;
		expect(
			flowAlgorithmSelectionReason(
				nonEmpty,
				empty?.modelKind,
				empty?.nodeCount,
				empty?.edgeCount,
				empty?.graphShape,
			),
		).toBe("non-empty-edges-required");
	});

	it("rejects self-loops before selecting the bounded deterministic MCF lab", () => {
		const boundedLab = {
			...entry("deterministic-almost-linear-mcf"),
			models: ["transshipment"],
			problems: ["min-cost-flow"],
			graph_requirements: ["no-self-loops"],
		} satisfies FlowAlgorithmCatalogEntry;
		const selection = flowScenarioSelection(
			JSON.stringify({
				payload: {
					model: { kind: "transshipment" },
					algorithm: { id: boundedLab.id },
					graph: {
						nodes: [{ id: "a", supply: "0" }],
						edges: [
							{
								id: "loop",
								from: "a",
								to: "a",
								lower: "0",
								capacity: "1",
								cost: "0",
							},
						],
					},
				},
			}),
		);
		expect(selection?.graphShape?.noSelfLoops).toBe(false);
		expect(
			flowAlgorithmSelectionReason(
				boundedLab,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
			),
		).toBe("no-self-loops-required");
	});

	it("blocks zero-flow SSAP when a lower bound or supply is nonzero", () => {
		const ssap = {
			...entry("successive-shortest-augmenting-path"),
			problems: ["min-cost-flow"],
			models: ["min-cost-max-flow"],
			graph_requirements: ["zero-flow-feasible"],
		} satisfies FlowAlgorithmCatalogEntry;
		const selection = flowScenarioSelection(
			JSON.stringify({
				payload: {
					model: { kind: "min-cost-max-flow", source: "s", sink: "t" },
					algorithm: { id: ssap.id },
					graph: {
						nodes: [
							{ id: "s", supply: "0" },
							{ id: "t", supply: "0" },
						],
						edges: [
							{
								id: "st",
								from: "s",
								to: "t",
								lower: "1",
								capacity: "2",
								cost: "0",
							},
						],
					},
				},
			}),
		);
		expect(selection?.graphShape?.zeroFlowFeasible).toBe(false);
		expect(
			flowAlgorithmSelectionReason(
				ssap,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
			),
		).toBe("zero-flow-feasible-required");
	});

	it("admits enhanced scaling only for strongly connected nonbinding transshipment encodings", () => {
		const enhanced = {
			...entry("enhanced-capacity-scaling"),
			title: "Enhanced Capacity Scaling · Uncapacitated Transshipment",
			family: "scaling",
			problems: ["min-cost-flow"],
			models: ["transshipment"],
			graph_requirements: [
				"strongly-connected",
				"nonbinding-transshipment-capacities",
			],
			initial_construction: "zero-pseudoflow-with-imbalance",
			initial_optimality: "dual-feasible",
			negative_cycle_policy: "require-absent-anywhere",
			source_id: "orlin-1993",
		} satisfies FlowAlgorithmCatalogEntry;
		const scenario = {
			payload: {
				model: { kind: "transshipment" },
				algorithm: { id: enhanced.id },
				graph: {
					nodes: [
						{ id: "a", supply: "3" },
						{ id: "b", supply: "-3" },
					],
					edges: [
						{
							id: "ab",
							from: "a",
							to: "b",
							lower: "0",
							capacity: "3",
							cost: "1",
						},
						{
							id: "ba",
							from: "b",
							to: "a",
							lower: "0",
							capacity: "3",
							cost: "2",
						},
					],
				},
			},
		};
		const selection = flowScenarioSelection(JSON.stringify(scenario));
		expect(selection?.graphShape).toMatchObject({
			stronglyConnected: true,
			nonbindingTransshipmentCapacities: true,
		});
		expect(
			flowAlgorithmSelectionReason(
				enhanced,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
			),
		).toBe("ready");
		const dualSimplex = {
			...enhanced,
			id: "dual-network-simplex",
			title: "Dual Network Simplex · Uncapacitated Transshipment",
			family: "simplex",
			initial_construction: "dual-feasible",
			source_id: "orlin-plotkin-tardos-1993-dual-simplex",
		} satisfies FlowAlgorithmCatalogEntry;
		expect(
			flowAlgorithmSelectionReason(
				dualSimplex,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
			),
		).toBe("ready");

		const narrow = structuredClone(scenario);
		const narrowReturn = narrow.payload.graph.edges[1];
		if (narrowReturn === undefined) throw new Error("fixture edge is missing");
		narrowReturn.capacity = "2";
		const narrowSelection = flowScenarioSelection(JSON.stringify(narrow));
		expect(
			flowAlgorithmSelectionReason(
				enhanced,
				narrowSelection?.modelKind,
				narrowSelection?.nodeCount,
				narrowSelection?.edgeCount,
				narrowSelection?.graphShape,
			),
		).toBe("nonbinding-transshipment-capacities-required");

		const oneWay = structuredClone(scenario);
		oneWay.payload.graph.edges.pop();
		const oneWaySelection = flowScenarioSelection(JSON.stringify(oneWay));
		expect(
			flowAlgorithmSelectionReason(
				enhanced,
				oneWaySelection?.modelKind,
				oneWaySelection?.nodeCount,
				oneWaySelection?.edgeCount,
				oneWaySelection?.graphShape,
			),
		).toBe("strongly-connected-required");
	});

	it("disables excess scaling when fixed-flow demand makes capacities binding", () => {
		const excessScaling = {
			...entry("excess-scaling-mcf"),
			title: "Excess Scaling · Transshipment",
			family: "scaling",
			problems: ["min-cost-flow"],
			models: ["fixed-flow-min-cost", "circulation", "transshipment"],
			runtime_route: "min-cost-flow",
			graph_requirements: ["nonbinding-transshipment-capacities"],
			initial_construction: "zero-pseudoflow-with-imbalance",
			initial_optimality: "dual-feasible",
			negative_cycle_policy: "require-absent-anywhere",
		} satisfies FlowAlgorithmCatalogEntry;
		const scenario = {
			payload: {
				model: {
					kind: "fixed-flow-min-cost",
					source: "s",
					sink: "t",
					required_flow: "5",
				},
				algorithm: { id: excessScaling.id },
				graph: {
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
				},
			},
		};
		const narrow = flowScenarioSelection(JSON.stringify(scenario));
		expect(narrow?.graphShape?.nonbindingTransshipmentCapacities).toBe(false);
		expect(
			flowAlgorithmSelectionReason(
				excessScaling,
				narrow?.modelKind,
				narrow?.nodeCount,
				narrow?.edgeCount,
				narrow?.graphShape,
			),
		).toBe("nonbinding-transshipment-capacities-required");

		const wideScenario = structuredClone(scenario);
		const wideEdge = wideScenario.payload.graph.edges[0];
		if (wideEdge === undefined) throw new Error("fixture edge is missing");
		wideEdge.capacity = "5";
		wideScenario.payload.graph.edges.push({
			id: "loop",
			from: "s",
			to: "s",
			lower: "2",
			capacity: "7",
			cost: "0",
		});
		const wide = flowScenarioSelection(JSON.stringify(wideScenario));
		expect(wide?.graphShape?.nonbindingTransshipmentCapacities).toBe(true);
		expect(
			flowAlgorithmSelectionReason(
				excessScaling,
				wide?.modelKind,
				wide?.nodeCount,
				wide?.edgeCount,
				wide?.graphShape,
			),
		).toBe("ready");

		const negativeCycleScenario = structuredClone(wideScenario);
		negativeCycleScenario.payload.graph.edges.push({
			id: "return",
			from: "t",
			to: "s",
			lower: "0",
			capacity: "5",
			cost: "-1",
		});
		const negativeCycle = flowScenarioSelection(
			JSON.stringify(negativeCycleScenario),
		);
		expect(negativeCycle?.graphShape?.lowerBoundResidualNegativeCycle).toBe(
			"present",
		);
		expect(
			flowAlgorithmSelectionReason(
				excessScaling,
				negativeCycle?.modelKind,
				negativeCycle?.nodeCount,
				negativeCycle?.edgeCount,
				negativeCycle?.graphShape,
			),
		).toBe("negative-residual-cycle-absent-required");
	});

	it("does not invent reverse residual capacity at an edge lower bound", () => {
		const excessScaling = {
			...entry("excess-scaling-mcf"),
			models: ["transshipment"],
			problems: ["min-cost-flow"],
			runtime_route: "min-cost-flow",
			graph_requirements: ["nonbinding-transshipment-capacities"],
			initial_construction: "zero-pseudoflow-with-imbalance",
			initial_optimality: "dual-feasible",
			negative_cycle_policy: "require-absent-anywhere",
		} satisfies FlowAlgorithmCatalogEntry;
		const selection = flowScenarioSelection(
			JSON.stringify({
				payload: {
					model: { kind: "transshipment" },
					algorithm: { id: excessScaling.id },
					graph: {
						nodes: [
							{ id: "a", supply: "1" },
							{ id: "b", supply: "-1" },
						],
						edges: [
							{
								id: "fixed",
								from: "a",
								to: "b",
								lower: "1",
								capacity: "1",
								cost: "2",
							},
							{
								id: "optional",
								from: "a",
								to: "b",
								lower: "0",
								capacity: "1",
								cost: "1",
							},
						],
					},
				},
			}),
		);
		expect(selection?.graphShape).toMatchObject({
			nonbindingTransshipmentCapacities: true,
			lowerBoundResidualNegativeCycle: "absent",
		});
		expect(
			flowAlgorithmSelectionReason(
				excessScaling,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
			),
		).toBe("ready");
	});

	it("maps every public base model to the catalog compatibility class", () => {
		expect(modelProblemKind("max-flow")).toBe("max-flow");
		expect(modelProblemKind("parametric-max-flow")).toBe("parametric-max-flow");
		expect(modelProblemKind("convex-cost-flow")).toBe("convex-cost-flow");
		expect(modelProblemKind("planar-max-flow")).toBe("planar-max-flow");
		expect(modelProblemKind("bipartite-matching")).toBe("bipartite-matching");
		expect(modelProblemKind("assignment")).toBe("assignment");
		for (const kind of [
			"fixed-flow-min-cost",
			"min-cost-max-flow",
			"circulation",
			"transshipment",
		] as const) {
			expect(modelProblemKind(kind)).toBe("min-cost-flow");
		}
	});

	it("requires a verified combinatorial embedding for planar max-flow", () => {
		const planar = {
			...entry("hassin-st-planar"),
			title: "Hassin st-planar",
			models: ["planar-max-flow"],
			graph_requirements: ["planar-embedding"],
			source_id: "hassin-1981-st-planar-max-flow",
		} satisfies FlowAlgorithmCatalogEntry;
		const scenario = {
			payload: {
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
				algorithm: { id: planar.id },
				graph: {
					nodes: ["a", "b", "c"].map((id) => ({ id, supply: "0" })),
					edges: [
						{
							id: "ab",
							from: "a",
							to: "b",
							lower: "0",
							capacity: "5",
							cost: "0",
						},
						{
							id: "ac",
							from: "a",
							to: "c",
							lower: "0",
							capacity: "2",
							cost: "0",
						},
						{
							id: "bc",
							from: "b",
							to: "c",
							lower: "0",
							capacity: "3",
							cost: "0",
						},
					],
				},
			},
		};
		const selection = flowScenarioSelection(JSON.stringify(scenario));
		expect(selection?.modelKind).toBe("planar-max-flow");
		expect(selection?.graphShape?.planarEmbedding).toBe("verified");
		expect(
			flowAlgorithmSelectionReason(
				planar,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
			),
		).toBe("ready");

		const broken = structuredClone(scenario);
		broken.payload.model.embedding.rotations[0]?.darts.pop();
		const brokenSelection = flowScenarioSelection(JSON.stringify(broken));
		expect(brokenSelection?.graphShape?.planarEmbedding).toBe("unavailable");
		expect(
			flowAlgorithmSelectionReason(
				planar,
				brokenSelection?.modelKind,
				brokenSelection?.nodeCount,
				brokenSelection?.edgeCount,
				brokenSelection?.graphShape,
			),
		).toBe("graph-shape-unverifiable");
	});

	it("selects Hungarian and Auction only for a native assignment model", () => {
		const hungarian = {
			...entry("hungarian"),
			title: "Hungarian",
			family: "assignment",
			problems: ["assignment"],
			models: ["assignment"],
			graph_requirements: ["bipartite"],
			complexity:
				"O(a^2 t), hence O(n^3), for a agents and t tasks with a <= t",
			source_id: "kuhn-tomizawa-edmonds-karp-hungarian",
		} satisfies FlowAlgorithmCatalogEntry;
		const selection = flowScenarioSelection(
			JSON.stringify({
				payload: {
					model: {
						kind: "assignment",
						agents: ["a0", "a1"],
						tasks: ["t0", "t1", "t2"],
						objective: "minimize",
					},
					algorithm: { id: "hungarian" },
					graph: {
						nodes: ["a0", "a1", "t0", "t1", "t2"].map((id) => ({
							id,
							supply: "0",
						})),
						edges: [
							{
								id: "e00",
								from: "a0",
								to: "t0",
								lower: "0",
								capacity: "1",
								cost: "4",
							},
						],
					},
				},
			}),
		);
		expect(selection?.modelKind).toBe("assignment");
		expect(selection?.graphShape?.bipartite).toBe(true);
		expect(
			flowAlgorithmSelectionReason(
				hungarian,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
			),
		).toBe("ready");
		expect(isFlowAlgorithmCompatible(hungarian, "bipartite-matching")).toBe(
			false,
		);
		const auction = {
			...hungarian,
			id: "auction",
			title: "Auction",
			complexity:
				"source O(n A log(nC)) for its particular scaled symmetric implementation; this bounded rectangular implementation resets to equal prices per scale",
			source_id: "bertsekas-auction-1988",
		} satisfies FlowAlgorithmCatalogEntry;
		expect(
			flowAlgorithmSelectionReason(
				auction,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
			),
		).toBe("ready");
		expect(isFlowAlgorithmCompatible(auction, "bipartite-matching")).toBe(
			false,
		);
	});

	it("keeps sparse transportation models selectable with an isolated destination", () => {
		const transportation = {
			...entry("transportation-simplex"),
			title: "Transportation Simplex",
			family: "transportation",
			problems: ["transportation"],
			models: ["transportation"],
			graph_requirements: ["bipartite", "transportation-network"],
			source_id: "ye-bland-transportation-simplex",
		} satisfies FlowAlgorithmCatalogEntry;
		const selection = flowScenarioSelection(
			JSON.stringify({
				payload: {
					model: {
						kind: "transportation",
						origins: ["o0"],
						destinations: ["d0"],
					},
					algorithm: { id: transportation.id },
					graph: {
						nodes: [
							{ id: "d0", supply: "-1" },
							{ id: "o0", supply: "1" },
						],
						edges: [],
					},
				},
			}),
		);
		expect(selection?.graphShape?.transportationNetwork).toBe(true);
		expect(
			flowAlgorithmSelectionReason(
				transportation,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
			),
		).toBe("ready");
	});

	it("selects Hopcroft–Karp only for an explicit bipartite-matching model", () => {
		const hopcroftKarp = {
			...entry("hopcroft-karp"),
			title: "Hopcroft–Karp",
			problems: ["bipartite-matching"],
			models: ["bipartite-matching"],
			graph_requirements: ["bipartite"],
			complexity: "O((m+n) sqrt(n))",
			source_id: "hopcroft-karp-1973",
		} satisfies FlowAlgorithmCatalogEntry;
		const selection = flowScenarioSelection(
			JSON.stringify({
				payload: {
					model: {
						kind: "bipartite-matching",
						left: ["l0", "l1"],
						right: ["r0", "r1"],
						flow_adapter: { source: "s", sink: "t" },
					},
					algorithm: { id: "hopcroft-karp" },
					graph: {
						nodes: ["s", "l0", "l1", "r0", "r1", "t"].map((id) => ({
							id,
							supply: "0",
						})),
						edges: [
							{
								id: "a0",
								from: "s",
								to: "l0",
								lower: "0",
								capacity: "1",
								cost: "0",
							},
							{
								id: "b0",
								from: "l0",
								to: "r0",
								lower: "0",
								capacity: "1",
								cost: "0",
							},
							{
								id: "c0",
								from: "r0",
								to: "t",
								lower: "0",
								capacity: "1",
								cost: "0",
							},
						],
					},
				},
			}),
		);
		expect(selection?.modelKind).toBe("bipartite-matching");
		expect(selection?.graphShape?.bipartite).toBe(true);
		expect(
			flowAlgorithmSelectionReason(
				hopcroftKarp,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
			),
		).toBe("ready");
		expect(isFlowAlgorithmCompatible(hopcroftKarp, "max-flow")).toBe(false);
	});

	it("separates executable status from model compatibility", () => {
		const maximumFlow = entry();
		expect(isFlowAlgorithmCompatible(maximumFlow, "max-flow")).toBe(true);
		expect(isFlowAlgorithmCompatible(maximumFlow, "fixed-flow-min-cost")).toBe(
			false,
		);
		expect(flowAlgorithmSelectionReason(maximumFlow, "max-flow")).toBe("ready");
		expect(
			flowAlgorithmSelectionReason(maximumFlow, "fixed-flow-min-cost"),
		).toBe("incompatible");
		expect(flowAlgorithmSelectionReason(maximumFlow, "max-flow", 2001, 1)).toBe(
			"node-limit",
		);
		expect(
			flowAlgorithmSelectionReason(maximumFlow, "max-flow", 2, 20001),
		).toBe("edge-limit");
		expect(
			flowAlgorithmSelectionReason(
				{ ...maximumFlow, status: "planned" },
				"max-flow",
			),
		).toBe("planned");
		expect(
			flowAlgorithmSelectionReason(
				{ ...maximumFlow, status: "source-blocked" },
				"max-flow",
			),
		).toBe("source-blocked");
		expect(flowAlgorithmSelectionReason(maximumFlow, undefined)).toBe(
			"invalid-model",
		);
	});

	it("enforces the dedicated Orlin finite-capacity admission band", () => {
		const orlin = {
			...entry("orlin-mcf"),
			title: "Orlin MCF · Finite-Capacity Transformation",
			family: "strongly-polynomial",
			problems: ["min-cost-flow"],
			models: [
				"fixed-flow-min-cost",
				"min-cost-max-flow",
				"circulation",
				"transshipment",
			],
			initial_construction: "source-defined",
			initial_optimality: "source-defined",
			negative_cycle_policy: "source-defined",
			source_id: "orlin-1993",
			initial_band: { max_nodes: 32, max_edges: 96 },
		} satisfies FlowAlgorithmCatalogEntry;
		expect(flowAlgorithmSelectionReason(orlin, "transshipment", 32, 96)).toBe(
			"ready",
		);
		expect(flowAlgorithmSelectionReason(orlin, "transshipment", 33, 96)).toBe(
			"node-limit",
		);
		expect(flowAlgorithmSelectionReason(orlin, "transshipment", 32, 97)).toBe(
			"edge-limit",
		);
	});

	it("disables only bounded kernels when current numeric values exceed their exact domain", () => {
		const selection = flowScenarioSelection(
			JSON.stringify({
				payload: {
					model: {
						kind: "fixed-flow-min-cost",
						source: "s",
						sink: "t",
						required_flow: "3",
					},
					algorithm: { id: "successive-shortest-path" },
					graph: {
						nodes: [
							{ id: "s", supply: "0" },
							{ id: "a", supply: "0" },
							{ id: "t", supply: "0" },
						],
						edges: [
							{
								id: "sa",
								from: "s",
								to: "a",
								lower: "0",
								capacity: "12",
								cost: "2",
							},
							{
								id: "at",
								from: "a",
								to: "t",
								lower: "0",
								capacity: "8",
								cost: "-4",
							},
						],
					},
				},
			}),
		);
		expect(selection?.admissionFacts).toEqual({
			maximumCapacity: 12n,
			maximumAbsoluteCost: 4n,
			assignmentSpace: 117n,
			capacityStateSpace: 117n,
			strictInterior: true,
		});
		const general = {
			...entry("successive-shortest-path"),
			models: ["fixed-flow-min-cost" as const],
			runtime_route: "min-cost-flow" as const,
		};
		const bounded = {
			...general,
			id: "deterministic-almost-linear-mcf",
			initial_band: { max_nodes: 6, max_edges: 8 },
			admission_contract: {
				...general.admission_contract,
				min_nodes: 2,
				min_edges: 1,
				max_capacity: "8",
				max_absolute_cost: "32",
				max_assignment_space: "100000",
				strict_interior_required: true,
			},
		};
		expect(
			flowAlgorithmSelectionReason(
				general,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
				selection?.dynamicUpdates,
				selection?.admissionFacts,
			),
		).toBe("ready");
		expect(
			flowAlgorithmSelectionReason(
				bounded,
				selection?.modelKind,
				selection?.nodeCount,
				selection?.edgeCount,
				selection?.graphShape,
				selection?.dynamicUpdates,
				selection?.admissionFacts,
			),
		).toBe("kernel-capacity-limit");
	});

	it("reports bounded state-space and strict-interior failures before selection", () => {
		const facts = {
			maximumCapacity: 8n,
			maximumAbsoluteCost: 32n,
			assignmentSpace: 100_001n,
			capacityStateSpace: 100_001n,
			strictInterior: false,
		};
		const mcf = {
			...entry("deterministic-almost-linear-mcf"),
			models: ["circulation" as const],
			runtime_route: "min-cost-flow" as const,
			initial_band: { max_nodes: 6, max_edges: 8 },
			admission_contract: {
				...entry().admission_contract,
				min_nodes: 2,
				min_edges: 1,
				max_capacity: "8",
				max_absolute_cost: "32",
				max_assignment_space: "100000",
				strict_interior_required: true,
			},
		};
		expect(
			flowAlgorithmSelectionReason(
				mcf,
				"circulation",
				4,
				5,
				undefined,
				undefined,
				facts,
			),
		).toBe("kernel-state-space-limit");
		expect(
			flowAlgorithmSelectionReason(
				mcf,
				"circulation",
				4,
				5,
				undefined,
				undefined,
				{ ...facts, assignmentSpace: 81n, capacityStateSpace: 81n },
			),
		).toBe("strict-interior-required");
	});

	it("enforces solver-owned node and edge maxima before dispatch", () => {
		const bounded = {
			...entry("minimum-ratio-cycle-mcf"),
			models: ["circulation" as const],
			runtime_route: "min-cost-flow" as const,
			initial_band: { max_nodes: 64, max_edges: 512 },
			admission_contract: {
				...entry().admission_contract,
				max_nodes: 6,
				max_edges: 8,
			},
		};
		const facts = {
			maximumCapacity: 1n,
			maximumAbsoluteCost: 0n,
			assignmentSpace: 2n,
			capacityStateSpace: 2n,
			strictInterior: true,
		};
		expect(
			flowAlgorithmSelectionReason(
				bounded,
				"circulation",
				7,
				8,
				undefined,
				undefined,
				facts,
			),
		).toBe("kernel-node-limit");
		expect(
			flowAlgorithmSelectionReason(
				bounded,
				"circulation",
				6,
				9,
				undefined,
				undefined,
				facts,
			),
		).toBe("kernel-edge-limit");
	});

	it("saturates 100,000-edge admission products at the u64 contract ceiling", () => {
		const edge = {
			id: "e",
			from: "s",
			to: "t",
			lower: "0",
			capacity: "18446744073709551615",
			cost: "-18446744073709551615",
		};
		const started = performance.now();
		const facts = flowGraphAdmissionFacts({
			edges: Array.from({ length: 100_000 }, () => edge),
		});
		const elapsed = performance.now() - started;
		expect(facts).toEqual({
			maximumCapacity: 18_446_744_073_709_551_615n,
			maximumAbsoluteCost: 18_446_744_073_709_551_615n,
			assignmentSpace: FLOW_ADMISSION_PRODUCT_SATURATION,
			capacityStateSpace: FLOW_ADMISSION_PRODUCT_SATURATION,
			strictInterior: true,
		});
		expect(elapsed).toBeLessThan(1_000);
	});

	it("requires a balance-feasible strict interior rather than positive edge widths alone", () => {
		const model = { kind: "transshipment" };
		const graph = {
			nodes: [
				{ id: "s", supply: "3" },
				{ id: "a", supply: "0" },
				{ id: "t", supply: "-3" },
			],
			edges: [
				{
					id: "sa",
					from: "s",
					to: "a",
					lower: "0",
					capacity: "3",
					cost: "0",
				},
				{
					id: "at",
					from: "a",
					to: "t",
					lower: "0",
					capacity: "3",
					cost: "0",
				},
			],
		};
		expect(flowGraphAdmissionFacts(graph, model)?.strictInterior).toBe(false);
		const relaxed = {
			...graph,
			edges: graph.edges.map((edge) => ({ ...edge, capacity: "4" })),
		};
		expect(flowGraphAdmissionFacts(relaxed, model)?.strictInterior).toBe(true);
	});

	it("does not enumerate strict-interior cuts beyond the owning kernel band", () => {
		const nodes = Array.from({ length: 20 }, (_, index) => ({
			id: `n${index}`,
			supply: "0",
		}));
		const edges = Array.from({ length: 80 }, (_, index) => ({
			id: `e${index}`,
			from: `n${index % nodes.length}`,
			to: `n${(index + 1) % nodes.length}`,
			lower: "0",
			capacity: "4",
			cost: "0",
		}));
		const started = performance.now();
		const facts = flowGraphAdmissionFacts(
			{ nodes, edges },
			{ kind: "transshipment" },
		);
		expect(performance.now() - started).toBeLessThan(100);
		expect(facts?.strictInterior).toBe(false);
	});

	it("admits Dynamic EIBFS only for a bounded capacity-only update sequence", () => {
		const dynamic = {
			...entry("dynamic-eibfs"),
			admission_contract: {
				...entry().admission_contract,
				min_dynamic_capacity_updates: 1,
				max_dynamic_capacity_updates: 256,
				capacity_updates_only: true,
			},
		};
		expect(flowAlgorithmSelectionReason(dynamic, "max-flow")).toBe(
			"capacity-updates-required",
		);
		expect(
			flowAlgorithmSelectionReason(dynamic, "max-flow", 4, 5, undefined, {
				count: 3,
				capacityOnly: true,
			}),
		).toBe("ready");
		expect(
			flowAlgorithmSelectionReason(dynamic, "max-flow", 4, 5, undefined, {
				count: 3,
				capacityOnly: false,
			}),
		).toBe("capacity-updates-only-required");
		expect(
			flowAlgorithmSelectionReason(dynamic, "max-flow", 4, 5, undefined, {
				count: 257,
				capacityOnly: true,
			}),
		).toBe("dynamic-update-limit");
	});

	it("searches canonical names, aliases, discovery terms, families, complexity, and sources", () => {
		const entries = [
			entry(),
			{
				...entry("bellman-ford-ssp"),
				title: "Bellman–Ford SSP",
				aliases: ["BF SSP"],
				search_terms: ["successive shortest path family"],
				family: "shortest-path",
				complexity: "O(F n m)",
				source_id: "jewell-1958-ssp",
			},
		];
		expect(filterFlowAlgorithmCatalog(entries, "BF SSP")).toEqual([entries[1]]);
		expect(
			filterFlowAlgorithmCatalog(entries, "successive shortest path family"),
		).toEqual([entries[1]]);
		expect(filterFlowAlgorithmCatalog(entries, "jewell")).toEqual([entries[1]]);
		expect(filterFlowAlgorithmCatalog(entries, "")).toBe(entries);
	});

	it("combines query, model compatibility, family, kind, and randomness facets", () => {
		const entries = [
			entry(),
			{
				...entry("randomized-max-flow"),
				title: "Randomized max flow",
				family: "research",
				kind: "primitive" as const,
				randomized: true,
			},
			{
				...entry("min-cost-solver"),
				title: "Min-cost solver",
				family: "shortest-path",
				models: ["fixed-flow-min-cost" as const],
				runtime_route: "min-cost-flow" as const,
			},
		];
		expect(
			filterFlowAlgorithmCatalogByFacets(
				entries,
				"flow",
				DEFAULT_FLOW_ALGORITHM_CATALOG_FILTERS,
				{ workspaceProblem: "max-flow", modelKind: "max-flow" },
			).map((candidate) => candidate.id),
		).toEqual(["randomized-max-flow"]);
		expect(
			filterFlowAlgorithmCatalogByFacets(
				entries,
				"",
				DEFAULT_FLOW_ALGORITHM_CATALOG_FILTERS,
				{
					workspaceProblem: "max-flow",
					modelKind: "fixed-flow-min-cost",
				},
			).map((candidate) => candidate.id),
		).toEqual(["edmonds-karp", "randomized-max-flow"]);
		expect(
			filterFlowAlgorithmCatalogByFacets(
				entries,
				"",
				DEFAULT_FLOW_ALGORITHM_CATALOG_FILTERS,
				{ workspaceProblem: "max-flow", modelKind: undefined },
			).map((candidate) => candidate.id),
		).toEqual(["edmonds-karp", "randomized-max-flow"]);
		expect(
			filterFlowAlgorithmCatalogByFacets(
				entries,
				"",
				{
					compatibility: "all",
					family: "research",
					kind: "primitive",
					randomness: "randomized",
				},
				{ workspaceProblem: "max-flow", modelKind: "max-flow" },
			).map((candidate) => candidate.id),
		).toEqual(["randomized-max-flow"]);
		expect(
			filterFlowAlgorithmCatalogByFacets(
				[{ ...entry(), initial_band: { max_nodes: 1, max_edges: 1 } }],
				"",
				{
					...DEFAULT_FLOW_ALGORITHM_CATALOG_FILTERS,
					compatibility: "runnable-now",
				},
				{
					workspaceProblem: "max-flow",
					modelKind: "max-flow",
					nodeCount: 2,
					edgeCount: 1,
				},
			),
		).toEqual([]);
	});

	it("reads the model and selected algorithm without accepting malformed JSON", () => {
		expect(
			flowScenarioSelection(
				JSON.stringify({
					payload: {
						model: { kind: "circulation" },
						algorithm: { id: "bellman-ford-ssp" },
						graph: { nodes: [{ id: "a" }], edges: [] },
					},
				}),
			),
		).toEqual({
			modelKind: "circulation",
			algorithmId: "bellman-ford-ssp",
			nodeCount: 1,
			edgeCount: 0,
			admissionFacts: {
				maximumCapacity: 0n,
				maximumAbsoluteCost: 0n,
				assignmentSpace: 1n,
				capacityStateSpace: 1n,
				strictInterior: true,
			},
			dynamicUpdates: { count: 0, capacityOnly: true },
		});
		expect(flowScenarioSelection("{")).toBeUndefined();
		expect(
			flowScenarioSelection(
				JSON.stringify({ payload: { model: { kind: "future" } } }),
			),
		).toBeUndefined();
	});

	it("uses exact runtime models rather than the broader problem family", () => {
		const bellmanFord: FlowAlgorithmCatalogEntry = {
			...entry("bellman-ford-ssp"),
			problems: ["min-cost-flow"],
			models: ["fixed-flow-min-cost", "circulation", "transshipment"],
		};
		expect(isFlowAlgorithmCompatible(bellmanFord, "circulation")).toBe(true);
		expect(isFlowAlgorithmCompatible(bellmanFord, "min-cost-max-flow")).toBe(
			false,
		);
	});
});
