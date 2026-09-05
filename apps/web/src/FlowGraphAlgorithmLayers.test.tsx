import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import {
	FlowGraphAlmostLinearCertificateLayer,
	FlowGraphBlockingFlowLevelLayer,
	FlowGraphDynamicEibfsCertificateLayer,
	FlowGraphEibfsNoNextLevelLayer,
} from "./FlowGraphAlgorithmLayers";
import { FlowGraphIdScopeProvider } from "./flow-dom-id";

function state(catalogId: string) {
	return {
		context: { traceEvent: { catalog_id: catalogId } },
		visualization: {
			nodeTraceStates: new Map([
				["s", { label: "0" }],
				["a", { label: "1" }],
				["b", { label: "0" }],
				["t", { label: "2" }],
			]),
		},
		visibleResidualArcs: [
			{
				edge_id: "sa",
				direction: "forward",
				from: "s",
				to: "a",
				capacity: "4",
			},
			{
				edge_id: "ab",
				direction: "reverse",
				from: "b",
				to: "a",
				capacity: "2",
			},
			{
				edge_id: "at",
				direction: "forward",
				from: "a",
				to: "t",
				capacity: "1",
			},
			{
				edge_id: "sb",
				direction: "forward",
				from: "s",
				to: "b",
				capacity: "7",
			},
			{
				edge_id: "zero",
				direction: "forward",
				from: "a",
				to: "t",
				capacity: "0",
			},
		],
		layout: {
			routes: new Map(
				["sa", "ab", "at", "sb", "zero"].map((edge) => [
					edge,
					{
						path: `M 0 0 L 10 ${edge.length}`,
						reversePath: `M 10 ${edge.length} L 0 0`,
					},
				]),
			),
		},
		originalVisuals: ["sa", "ab", "at", "sb", "zero"].map((edge, index) => ({
			edge: { id: edge },
			railWidth: 3 + index,
		})),
	} as never;
}

function render(catalogId: string): string {
	return renderToStaticMarkup(
		<FlowGraphIdScopeProvider scope="dinic-level-test">
			<svg>
				<title>Dinic blocking-flow level network</title>
				<FlowGraphBlockingFlowLevelLayer state={state(catalogId)} />
			</svg>
		</FlowGraphIdScopeProvider>,
	);
}

describe("Dinic blocking-flow level-network projection", () => {
	it("paints exactly the positive residual arcs that advance one level", () => {
		const svg = render("dinic.blocking-flow");
		expect(svg).toContain('data-level-network-arcs="3"');
		expect(svg).toContain('data-level-network-edge="sa"');
		expect(svg).toContain('data-level-network-edge="ab"');
		expect(svg).toContain('data-level-network-direction="reverse"');
		expect(svg).toContain('data-level-network-edge="at"');
		expect(svg).not.toContain('data-level-network-edge="sb"');
		expect(svg).not.toContain('data-level-network-edge="zero"');
		expect(svg).toContain(
			'marker-end="url(#dinic-level-test-flow-arrow-level-network)"',
		);
	});

	it.each([
		"unit-capacity-dinic.blocking-flow",
		"unit-network-dinic.blocking-flow",
	])("uses the same source-semantic projection for %s", (catalogId) => {
		expect(render(catalogId)).toContain(
			`data-level-network-event="${catalogId}"`,
		);
	});

	it("does not decorate adjacent source operations", () => {
		expect(render("dinic.publish-level-graph")).not.toContain(
			"flow-level-network",
		);
	});
});

function almostLinearCertificateState(
	variant: "randomized" | "deterministic",
	stage: "check-certificate" | "optimal",
) {
	const overlay = {
		stage,
		target_value: "5",
		nodes: [
			{ node_id: "s", source_side: true },
			{ node_id: "a", source_side: true },
			{ node_id: "t", source_side: false },
		],
	};
	return {
		renderData: {
			overlayViews: {
				randomizedAlmostLinear: variant === "randomized" ? overlay : undefined,
				deterministicAlmostLinear:
					variant === "deterministic" ? overlay : undefined,
			},
		},
		plan: {
			context: { model: { kind: "max-flow", source: "s", sink: "t" } },
			nodes: [{ id: "s" }, { id: "a" }, { id: "t" }],
			edges: [
				{ id: "sa", from: "s", to: "a", capacity: "8" },
				{ id: "at", from: "a", to: "t", capacity: "5" },
			],
			overlayPresentation: {
				activeFields: [
					variant === "randomized"
						? "randomized_almost_linear_overlay"
						: "deterministic_almost_linear_overlay",
				],
			},
		},
		layout: {
			routes: new Map([
				[
					"at",
					{
						path: "M 100 100 L 300 100",
						reversePath: "M 300 100 L 100 100",
					},
				],
			]),
		},
	} as never;
}

describe("almost-linear max-flow certificate projection", () => {
	it.each([
		"randomized",
		"deterministic",
	] as const)("paints only exact cut edges for %s", (variant) => {
		const checking = renderToStaticMarkup(
			<svg>
				<title>Almost-linear certificate check</title>
				<FlowGraphAlmostLinearCertificateLayer
					state={almostLinearCertificateState(variant, "check-certificate")}
					variant={variant}
				/>
			</svg>,
		);
		const verified = renderToStaticMarkup(
			<svg>
				<title>Verified almost-linear certificate</title>
				<FlowGraphAlmostLinearCertificateLayer
					state={almostLinearCertificateState(variant, "optimal")}
					variant={variant}
				/>
			</svg>,
		);
		expect(checking).toContain(
			'data-overlay-role="' +
				`${variant}_almost_linear_overlay:certificate-cut-check"`,
		);
		expect(checking).toContain('data-overlay-entity-id="at"');
		expect(checking).not.toContain('data-overlay-entity-id="sa"');
		expect(verified).toContain(
			'data-overlay-role="' +
				`${variant}_almost_linear_overlay:certificate-cut-verified"`,
		);
	});
});

function dynamicEibfsCertificateState(
	stage: "prefix-certified" | "prefix-recovery",
	value = "5",
) {
	return {
		context: {
			traceEvent: {
				catalog_id:
					stage === "prefix-certified"
						? "dynamic-eibfs.prefix-certified"
						: "eibfs.begin-feasible-flow-recovery",
				entity_refs: [
					{ kind: "node", node_id: "s" },
					{ kind: "node", node_id: "a" },
				],
			},
		},
		renderData: {
			overlayViews: {
				dynamicEibfs: {
					stage,
					update_index: "3",
					update_total: "3",
					prefix_value: stage === "prefix-certified" ? value : undefined,
				},
			},
		},
		plan: {
			context: { model: { kind: "max-flow", source: "s", sink: "t" } },
			edges: [
				{ id: "sa", from: "s", to: "a", capacity: "8" },
				{ id: "at-1", from: "a", to: "t", capacity: "2" },
				{ id: "at-2", from: "a", to: "t", capacity: "3" },
			],
			overlayPresentation: { activeFields: ["dynamic_eibfs_overlay"] },
		},
		layout: {
			routes: new Map([
				["at-1", { path: "M 100 92 Q 200 72 300 92" }],
				["at-2", { path: "M 100 108 Q 200 128 300 108" }],
			]),
		},
		positions: new Map([
			["s", { x: 30, y: 100 }],
			["a", { x: 100, y: 100 }],
			["t", { x: 300, y: 100 }],
		]),
	} as never;
}

describe("Dynamic EIBFS prefix certificate projection", () => {
	it("paints every parallel edge in the exact source-side cut", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Dynamic EIBFS certified prefix</title>
				<FlowGraphDynamicEibfsCertificateLayer
					state={dynamicEibfsCertificateState("prefix-certified")}
				/>
			</svg>,
		);
		expect(svg).toContain('data-dynamic-eibfs-cut-edges="at-1|at-2"');
		expect(svg).toContain('data-overlay-entity-id="at-1"');
		expect(svg).toContain('data-overlay-entity-id="at-2"');
		expect(svg).not.toContain('data-overlay-entity-id="sa"');
		expect(svg).toContain(
			'data-overlay-role="dynamic_eibfs_overlay:prefix-certificate.exact-cut-edge"',
		);
	});

	it("does not claim a certificate while the clone is still recovering", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Dynamic EIBFS recovering prefix</title>
				<FlowGraphDynamicEibfsCertificateLayer
					state={dynamicEibfsCertificateState("prefix-recovery")}
				/>
			</svg>,
		);
		expect(svg).not.toContain("flow-dynamic-eibfs-certificate");
	});
});

describe("EIBFS empty next-frontier projection", () => {
	it("anchors an explicit empty-level result to the exhausted search side", () => {
		const state = {
			context: {
				traceEvent: { catalog_id: "eibfs.no-next-level" },
			},
			renderData: {
				overlayViews: {
					eibfs: {
						phase_direction: "forward",
						source_depth: "2",
						sink_depth: "1",
						nodes: [
							{
								node_id: "s",
								membership: "source",
								source_label: "0",
								sink_label: "0",
							},
						],
					},
					dynamicEibfs: { stage: "continue-solve" },
				},
			},
			plan: {
				context: { model: { kind: "max-flow", source: "s", sink: "t" } },
				overlayPresentation: {
					activeFields: ["eibfs_overlay", "dynamic_eibfs_overlay"],
				},
			},
			positions: new Map([
				["s", { x: 100, y: 100 }],
				["t", { x: 800, y: 100 }],
			]),
		} as never;
		const svg = renderToStaticMarkup(
			<svg>
				<title>EIBFS empty next frontier</title>
				<FlowGraphEibfsNoNextLevelLayer state={state} />
			</svg>,
		);
		expect(svg).toContain('data-eibfs-empty-side="source"');
		expect(svg).toContain('data-eibfs-empty-depth="2"');
		expect(svg).toContain('data-overlay-entity-id="s"');
		expect(svg).toContain(
			'data-overlay-contributions="eibfs_overlay dynamic_eibfs_overlay"',
		);
		expect(svg).toContain("S · d2 · NEXT ∅");
	});

	it("does not decorate ordinary EIBFS boundaries", () => {
		const state = {
			context: { traceEvent: { catalog_id: "eibfs.complete-phase" } },
		} as never;
		expect(
			renderToStaticMarkup(
				<svg>
					<title>EIBFS phase completion</title>
					<FlowGraphEibfsNoNextLevelLayer state={state} />
				</svg>,
			),
		).not.toContain("flow-eibfs-no-next-level");
	});
});
