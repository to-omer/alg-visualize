import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { FlowGraphOverlayContributionLayer } from "./FlowGraphOverlayContributionLayer";
import {
	FlowGraphOverlayStatusLayer,
	FlowGraphSourceOperationLayer,
} from "./FlowGraphOverlayFeatureLayers";
import { FlowOverlayContributionStatus } from "./FlowOverlayContributionStatus";
import { FlowOverlayRegistryInspector } from "./FlowOverlayRegistryInspector";
import type { FlowOverlayPresentation } from "./flow-overlay-presentation";

function genericPresentation(): FlowOverlayPresentation {
	const overlay = "binary_blocking_overlay" as const;
	const status = {
		overlay,
		title: "Future generic overlay",
		items: [{ label: "Stage (stage)", value: "scanning" }],
	};
	return {
		overlays: {},
		renderData: {} as FlowOverlayPresentation["renderData"],
		activeFields: [overlay],
		marks: [],
		nodeMarksById: new Map(),
		edgeMarksById: new Map(),
		residualArcMarksByKey: new Map(),
		annotations: [],
		legendEntries: [
			{
				overlay,
				label: "Future generic overlay",
				description: "Contribution-owned fallback rendering",
			},
		],
		inspectorSections: [
			{
				overlay,
				title: "Future generic overlay",
				rows: [{ field: "stage", label: "Stage (stage)", value: "scanning" }],
			},
		],
		statusEntries: [status],
		genericStatusEntries: [status],
		genericNodeDecorations: [
			{
				overlay,
				kind: "node",
				entityId: "s",
				roles: ["nodes[0]"],
				accent: "teal",
			},
		],
		genericEdgeDecorations: [
			{
				overlay,
				kind: "edge",
				entityId: "e",
				roles: ["arcs[0]"],
				accent: "teal",
			},
		],
		genericResidualArcDecorations: [
			{
				overlay,
				kind: "residual-arc",
				entityId: "e",
				direction: "reverse",
				roles: ["residual_arcs[0]"],
				accent: "violet",
			},
		],
		accessibleDescriptions: ["Future generic overlay scanning"],
	};
}

describe("generic overlay contribution components", () => {
	it("renders the exact source operation and measured position without a generic work label", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Source operation</title>
				<FlowGraphSourceOperationLayer
					state={
						{
							context: {
								traceEvent: {
									catalog_id:
										"randomized-almost-linear-mcf-oracle-demonstrator.inspect-oracle-vector",
								},
								traceEventSemantics: {
									work_deltas: [{ unit: "primary-work", count: "2" }],
									aggregation_count: "2",
									work_progress: {
										detail_completed: "19",
										detail_total: "40",
										primary_completed: "26",
										primary_total: "80",
									},
									primary_work_block: {
										first: "7",
										last: "8",
										total: "26",
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			'data-source-operation="randomized-almost-linear-mcf-oracle-demonstrator.inspect-oracle-vector"',
		);
		expect(svg).toContain('data-source-operation-position="7:8:26"');
		expect(svg).toContain('data-source-operation-progress="19:40:26:80"');
		expect(svg).toContain("INSPECT ORACLE VECTOR · 26/80 · BATCH 7–8/26");
		expect(svg).not.toContain("SET WORK");
	});

	it("renders SVG decoration, status, legend, and inspector with no field switch", () => {
		const presentation = genericPresentation();
		const status = renderToStaticMarkup(
			<FlowOverlayContributionStatus presentation={presentation} />,
		);
		const inspector = renderToStaticMarkup(
			<FlowOverlayRegistryInspector presentation={presentation} />,
		);
		const svg = renderToStaticMarkup(
			<svg>
				<title>Generic overlay contribution</title>
				<FlowGraphOverlayContributionLayer
					state={
						{
							plan: { overlayPresentation: presentation },
							layout: {
								routes: new Map([
									[
										"e",
										{
											path: "M 0 0 L 10 10",
											reversePath: "M 10 10 L 0 0",
										},
									],
								]),
							},
							positions: new Map([["s", { x: 20, y: 30 }]]),
						} as never
					}
				/>
			</svg>,
		);

		expect(status).toContain('role="status"');
		expect(status).toContain("data-overlay-contribution-status");
		expect(inspector).toContain("Contribution-owned fallback rendering");
		expect(inspector).toContain("Stage (stage): scanning");
		expect(inspector).toContain('data-overlay-field="stage"');
		expect(inspector).toContain('data-overlay-value="scanning"');
		expect(svg).toContain('data-overlay-entity-kind="edge"');
		expect(svg).toContain('d="M 0 0 L 10 10"');
		expect(svg).toContain('data-overlay-entity-kind="residual-arc"');
		expect(svg).toContain('data-overlay-residual-direction="reverse"');
		expect(svg).toContain('d="M 10 10 L 0 0"');
		expect(svg).toContain('data-overlay-entity-kind="node"');
		expect(svg).toContain('cx="20"');
	});

	it("renders the exact Cancel-and-Tighten phase as a local graph badge", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Cancel-and-Tighten stage</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									cancelTighten: {
										stage: "begin-phase",
										phase: "3",
										epsilon: { numerator: "7", denominator: "2" },
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain('data-cancel-tighten-stage="begin-phase"');
		expect(svg).toContain('data-cancel-tighten-phase="3"');
		expect(svg).toContain('data-cancel-tighten-epsilon="7/2"');
		expect(svg).toContain("PHASE 3 · CANCEL ADMISSIBLE CYCLES · ε 7/2");
	});

	it("renders electrical CG convergence without global node focus", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Electrical convergence</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									electricalFlow: {
										stage: "conjugate-gradient-iteration",
										iteration: "7",
										residual_l2: "0.00031",
										relative_tolerance: "0.00001",
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			'data-electrical-stage="conjugate-gradient-iteration"',
		);
		expect(svg).toContain('data-electrical-iteration="7"');
		expect(svg).toContain(
			"CONJUGATE GRADIENT ITERATION · CG 7 · ‖r‖₂ 0.00031 / 0.00001",
		);
	});

	it("renders each augmenting-electrical structural boundary in the graph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Augmenting electrical reduction</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									augmentingElectrical: {
										stage: "install-target-cut",
										working_nodes: "12",
										working_edges: "54",
										current_value: "3",
										working_target: "29",
										remaining: "26",
										active_working_path: [],
										active_extraction_cycle: [],
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			'data-augmenting-electrical-stage="install-target-cut"',
		);
		expect(svg).toContain('data-augmenting-electrical-work="12:54"');
		expect(svg).toContain(
			"INSTALL TARGET CUT · WORK 12V/54E · FLOW 3/29 · REM 26",
		);
	});

	it("renders an auxiliary working-node elimination pivot without inventing an original-node focus", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Augmenting electrical pivot</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									augmentingElectrical: {
										stage: "solve-electrical-direction",
										working_nodes: "29",
										working_edges: "144",
										current_value: "3",
										working_target: "41",
										remaining: "38",
										active_pivot_node: "17",
										active_working_path: [],
										active_extraction_cycle: [],
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain('data-augmenting-electrical-pivot="17"');
		expect(svg).toContain("PIVOT w17/29");
	});

	it("renders the exact transformed cleanup path and augmentation amount", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Augmenting electrical cleanup</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									augmentingElectrical: {
										stage: "cleanup-augmenting-path",
										working_nodes: "12",
										working_edges: "54",
										current_value: "29",
										working_target: "31",
										remaining: "2",
										active_working_path: [
											{
												edge: "3",
												direction: "forward",
												from_node: "s",
												to_node: "v",
												flow_after: "5",
											},
											{
												edge: "11",
												direction: "reverse",
												from_node: "v",
												to_node: "t",
												flow_after: "-2",
											},
										],
										active_extraction_cycle: [],
										active_discrete_amount: "2",
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			'data-augmenting-electrical-path="forward:3,reverse:11"',
		);
		expect(svg).toContain('data-augmenting-electrical-amount="2"');
		expect(svg).toContain("PUSH 2 · 2 ARCS");
		expect(svg).toContain("Push 2 on s→v [w3, x=5]; v→t [w11, x=-2]");
	});

	it("renders the exact directed-reduction extraction cycle", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Augmenting electrical extraction cycle</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									augmentingElectrical: {
										stage: "cancel-extraction-cycle",
										working_nodes: "8",
										working_edges: "21",
										current_value: "21",
										working_target: "21",
										remaining: "0",
										active_working_path: [],
										active_extraction_cycle: [
											{ edge: "2", kind: "central" },
											{ edge: "4", kind: "toward-source" },
										],
										active_discrete_amount: "3",
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			'data-augmenting-electrical-cycle="central:2,toward-source:4"',
		);
		expect(svg).toContain("CANCEL 3 · 2 ARCS");
		expect(svg).toContain("Cancel 3 on central [e2]; toward-source [e4]");
	});

	it("renders IPM central-path progress without selecting every edge", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Interior-point convergence</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									interiorPointMaxFlow: {
										stage: "solve-electrical-direction",
										mu: "0.125",
										duality_gap: "0.5",
										electrical_energy: "18.75",
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			'data-interior-point-stage="solve-electrical-direction"',
		);
		expect(svg).toContain('data-interior-point-gap="0.5"');
		expect(svg).toContain(
			"SOLVE ELECTRICAL DIRECTION · μ 0.125 · GAP 0.5 · E 18.75",
		);
	});

	it("renders a minimum-ratio ternary vector checkpoint without broad edge focus", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Minimum-ratio vector checkpoint</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									minimumRatioCycle: {
										stage: "inspect-vector",
										enumerated_vectors: "128",
										candidate_ratio: {
											numerator: "-7",
											denominator: "3",
										},
										best_ratio: {
											numerator: "-5",
											denominator: "2",
										},
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain('data-minimum-ratio-stage="inspect-vector"');
		expect(svg).toContain('data-minimum-ratio-vector="128"');
		expect(svg).toContain(
			"INSPECT VECTOR · VECTOR 128 · CAND -7/3 · BEST -5/2",
		);
	});

	it("renders randomized return-edge and initial-point state inside the graph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Randomized almost-linear initial point</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									randomizedAlmostLinear: {
										stage: "build-initial-point",
										return_flow: "9.5",
										return_capacity: "32",
										artificial_flow: "14.25",
										iteration: "0",
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			'data-randomized-almost-linear-stage="build-initial-point"',
		);
		expect(svg).toContain(
			"BUILD INITIAL POINT · RETURN 9.5/32 · ARTIFICIAL 14.25 · ITER 0",
		);
	});

	it("renders deterministic transformed-core state inside the graph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Deterministic almost-linear core</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									deterministicAlmostLinear: {
										stage: "build-core-graph",
										return_flow: "7.5",
										return_capacity: "40",
										artificial_flow: "12",
										core_vertices: "5",
										core_edges: "8",
										active_branches: ["0", "0", "0"],
										passes: ["0", "0", "0"],
										fundamental_cycles: "0",
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			'data-deterministic-almost-linear-stage="build-core-graph"',
		);
		expect(svg).toContain(
			"BUILD CORE GRAPH · RETURN 7.5/40 · ARTIFICIAL 12 · CORE 5V/8E",
		);
	});

	it("renders deterministic cycle-chain identity inside the graph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Deterministic almost-linear cycle</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									deterministicAlmostLinear: {
										stage: "inspect-fundamental-cycle",
										return_flow: "7.5",
										return_capacity: "40",
										artificial_flow: "12",
										core_vertices: "5",
										core_edges: "8",
										active_level: "1",
										active_branches: ["0", "2", "1"],
										passes: ["3", "1", "0"],
										selected_off_tree_edge: "6",
										fundamental_cycles: "17",
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			"INSPECT FUNDAMENTAL CYCLE · EVAL 17 · CHAIN L1 B0/2/1 P3/1/0 · OFF w6",
		);
		expect(svg).toContain(
			'data-deterministic-almost-linear-chain="1:0,2,1:3,1,0"',
		);
	});

	it("renders weighted augmenting phase and source work inside the graph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Weighted augmenting phase</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									weightedAugmentingPaths: {
										stage: "finish-capacity-phase",
										phase: "2",
										phase_count: "5",
										capacity_bit: "2",
										round: "7",
										relabel_jumps: "41",
										augmentations: "9",
									},
								},
							},
							context: {},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			"FINISH CAPACITY PHASE · PHASE 3/5 · BIT b2 · ROUND 7 · RELABEL 41 · PATH 9",
		);
	});

	it("renders weighted push-relabel lifecycle inside the graph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Weighted push-relabel lifecycle</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									weightedPushRelabelShortcut: {
										stage: "assign-weights",
										hierarchy_levels: "1",
										height: "28",
										routed: "3",
										demand: "64",
										relabel_steps: "12",
										augmentations: "2",
										residual_rounds: "0",
									},
								},
							},
							context: {},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			"ASSIGN WEIGHTS · HIERARCHY 1L · h 28 · ROUTED 3/64 · RELABEL 12 · PATH 2 · ROUND 0",
		);
	});

	it("renders dynamic EIBFS source action and update progress inside the graph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Dynamic EIBFS stage</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									eibfs: {
										phase_direction: "forward",
										source_depth: "3",
										sink_depth: "2",
									},
									dynamicEibfs: {
										stage: "repair-forest",
										update_index: "2",
										update_total: "4",
										repair_arc_scans: "17",
									},
								},
							},
							context: {
								traceEvent: {
									catalog_id: "dynamic-eibfs.inspect-retained-parent",
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			"INSPECT RETAINED PARENT · REPAIR FOREST · UPDATE 2/4 · S3/T2 · REPAIR SCAN 17",
		);
	});

	it.each([
		["capacity-scaling-mcf.start-scaling-phase", "CAPACITY PHASE START · Δ 8"],
		[
			"capacity-scaling-mcf.complete-scaling-phase",
			"CAPACITY PHASE COMPLETE · Δ 8",
		],
		["excess-scaling-mcf.start-excess-phase", "EXCESS PHASE START · Δ 4"],
		["excess-scaling-mcf.complete-excess-phase", "EXCESS PHASE COMPLETE · Δ 4"],
	])("renders source scaling state %s inside the graph", (catalogId, label) => {
		const scale = catalogId.startsWith("capacity") ? "8" : "4";
		const svg = renderToStaticMarkup(
			<svg>
				<title>Scaling phase state</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: { overlayViews: {} },
							context: {
								traceEvent: {
									catalog_id: catalogId,
									detail: { label: "scale", value: scale },
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(`data-scaling-stage="${catalogId}"`);
		expect(svg).toContain(`data-scaling-scale="${scale}"`);
		expect(svg).toContain(label);
	});

	it("renders exact parametric traversal stage and interval inside the graph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Parametric traversal state</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									parametric: {
										stage: "solve-intersection",
										parameter: { numerator: "3", denominator: "2" },
										traversal: {
											lower: { numerator: "1", denominator: "1" },
											upper: { numerator: "2", denominator: "1" },
										},
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain('data-parametric-stage="solve-intersection"');
		expect(svg).toContain('data-parametric-parameter="3/2"');
		expect(svg).toContain("SOLVE INTERSECTION · λ 3/2 · RANGE [1, 2]");
	});

	it.each([
		[
			"excess-scaling-push-relabel.scale-phase",
			{ label: "delta", value: "8" },
			[],
			"Δ PHASE · Δ 8",
		],
		[
			"excess-scaling-push-relabel.select-scaled-active",
			{ label: "excess", value: "5" },
			[{ kind: "node", node_id: "l002n0003" }],
			"SELECT L2·3 · EXCESS 5",
		],
	])("renders exact excess-scaling state %s inside the graph", (catalogId, detail, entityRefs, label) => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Excess-scaling state</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: { overlayViews: {} },
							context: {
								traceEvent: {
									catalog_id: catalogId,
									detail,
									entity_refs: entityRefs,
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(`data-excess-scaling-stage="${catalogId}"`);
		expect(svg).toContain(label);
	});

	it("renders Hassin's settled dual face and exact Dijkstra progress in the graph", () => {
		const metrics = Array.from({ length: 16 }, () => "0");
		metrics[5] = "7";
		metrics[15] = "3";
		const svg = renderToStaticMarkup(
			<svg>
				<title>Hassin dual settlement</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: { overlayViews: {} },
							context: {
								metrics,
								traceEvent: {
									catalog_id: "hassin-st-planar.settle-dual-face",
									detail: { label: "dual-distance", value: "11" },
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(
			'data-hassin-stage="hassin-st-planar.settle-dual-face"',
		);
		expect(svg).toContain('data-hassin-detail="dual-distance:11"');
		expect(svg).toContain("SETTLE DUAL FACE 3/7 · DIST 11");
	});

	it("renders exact enhanced-scaling phase state inside the graph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Enhanced scaling phase</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									enhancedCapacityScaling: {
										stage: "augment",
										phase: "3",
										delta: { numerator: "5", denominator: "2" },
										augmentation: {
											numerator: "3",
											denominator: "2",
										},
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain('data-enhanced-scaling-stage="augment"');
		expect(svg).toContain('data-enhanced-scaling-phase="3"');
		expect(svg).toContain('data-enhanced-scaling-delta="5/2"');
		expect(svg).toContain("PHASE 3 · AUGMENT · Δ 5/2 · PUSH 3/2");
	});

	it("renders exact Orlin MCF phase state inside the transformed graph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Orlin MCF phase</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									orlinMcf: {
										stage: "halve-scale",
										phase: "4",
										delta: { numerator: "1", denominator: "8" },
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain('data-orlin-mcf-stage="halve-scale"');
		expect(svg).toContain('data-orlin-mcf-phase="4"');
		expect(svg).toContain('data-orlin-mcf-delta="1/8"');
		expect(svg).toContain("PHASE 4 · SCALE HALVED · Δ 1/8");
	});

	it.each([
		["cost-scaling.start-refine", "REFINE START · ε 16"],
		["generalized-cost-scaling.complete-refine", "REFINE COMPLETE · ε 16"],
	])("renders cost-scaling source stage %s inside the graph", (catalogId, label) => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Cost refine stage</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: { overlayViews: {} },
							context: {
								traceEvent: {
									catalog_id: catalogId,
									detail: { label: "epsilon", value: "16" },
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(`data-cost-refine-stage="${catalogId}"`);
		expect(svg).toContain('data-cost-refine-epsilon="16"');
		expect(svg).toContain(label);
	});

	it.each([
		[
			"price-refinement.start-potential-only-attempt",
			"epsilon",
			"8",
			"PRICE-ONLY ATTEMPT · ε 8",
		],
		[
			"price-refinement.complete-relaxation-round",
			"round",
			"3",
			"RELAXATION ROUND COMPLETE · ROUND 3",
		],
		[
			"price-refinement.succeed-without-flow-change",
			"flow-changes",
			"0",
			"PRICE CERTIFIED · FLOW CHANGES 0",
		],
		[
			"price-refinement.fail-and-rollback-prices",
			"negative-cycle",
			"1",
			"ROLL BACK PRICES · NEGATIVE CYCLE 1",
		],
	])("renders price-refinement source state %s inside the graph", (catalogId, detailLabel, detailValue, label) => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Price-refinement state</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: { overlayViews: {} },
							context: {
								traceEvent: {
									catalog_id: catalogId,
									detail: {
										label: detailLabel,
										value: detailValue,
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain(`data-price-refinement-stage="${catalogId}"`);
		expect(svg).toContain(
			`data-price-refinement-detail="${detailLabel}:${detailValue}"`,
		);
		expect(svg).toContain(label);
	});

	it("renders exact polynomial primal scale state inside the graph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Polynomial primal state</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									polynomialPrimalSimplex: {
										stage: "pivot",
										phase: "2",
										epsilon: { numerator: "3", denominator: "2" },
										delta: { numerator: "5", denominator: "1" },
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain('data-polynomial-primal-stage="pivot"');
		expect(svg).toContain('data-polynomial-primal-phase="2"');
		expect(svg).toContain("PHASE 2 · PIVOT FUNDAMENTAL CYCLE · ε 3/2 · Δ 5");
	});

	it("renders exact polynomial dual scale state inside the graph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Polynomial dual state</title>
				<FlowGraphOverlayStatusLayer
					state={
						{
							renderData: {
								overlayViews: {
									polynomialDualSimplex: {
										stage: "finish-scale",
										phase: "4",
										delta: { numerator: "1", denominator: "8" },
									},
								},
							},
						} as never
					}
				/>
			</svg>,
		);
		expect(svg).toContain('data-polynomial-dual-stage="finish-scale"');
		expect(svg).toContain('data-polynomial-dual-phase="4"');
		expect(svg).toContain('data-polynomial-dual-delta="1/8"');
		expect(svg).toContain("PHASE 4 · SCALE COMPLETE · Δ 1/8");
	});
});
