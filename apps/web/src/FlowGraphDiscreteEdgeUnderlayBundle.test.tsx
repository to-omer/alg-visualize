import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { FlowGraphDiscreteEdgeUnderlayBundle } from "./FlowGraphDiscreteEdgeUnderlayBundle";
import { FlowGraphIdScopeProvider } from "./flow-dom-id";

function renderCancelPhase(stage: "initialize" | "begin-phase"): string {
	const state = {
		viewMode: "original",
		context: { traceEvent: undefined, residualArcs: [] },
		positions: new Map([
			["s", { x: 80, y: 100 }],
			["t", { x: 320, y: 100 }],
		]),
		plan: {
			overlayPresentation: { activeFields: ["cancel_tighten_overlay"] },
		},
		renderData: {
			overlayViews: {
				cancelTighten: {
					stage,
					epsilon: { numerator: "4", denominator: "1" },
					phase: stage === "begin-phase" ? "1" : "0",
				},
				orlinMaxFlow: undefined,
				eibfs: undefined,
				dynamicEibfs: undefined,
			},
			cancelTightenAdmissibleArcKeys: new Set(["e:reverse"]),
			maximumOrlinMcfBranchFlow: 1n,
			orlinMcfCapacityNodeByEdge: new Map(),
			orlinMcfComponentById: new Map(),
			orlinMcfContractionKey: undefined,
			orlinMcfPathArcKeys: new Set(),
			orlinMaxNodeById: new Map(),
			orlinMaxResidualByKey: new Map(),
		},
	} as never;
	const visual = {
		edge: { id: "e", from: "s", to: "t" },
		railWidth: 4,
		geometry: {
			path: "M 80 100 L 320 100",
			reversePath: "M 320 100 L 80 100",
		},
	} as never;
	return renderToStaticMarkup(
		<FlowGraphIdScopeProvider scope="cancel-phase-test">
			<svg>
				<title>Cancel and Tighten phase</title>
				<FlowGraphDiscreteEdgeUnderlayBundle
					state={state}
					visual={visual}
					enabled
				/>
			</svg>
		</FlowGraphIdScopeProvider>,
	);
}

function renderCapacityScalingPhase(boundary: "start" | "complete"): string {
	const catalogId =
		boundary === "start"
			? "capacity-scaling-mcf.start-scaling-phase"
			: "capacity-scaling-mcf.complete-scaling-phase";
	const state = {
		viewMode: "original",
		positions: new Map([
			["s", { x: 80, y: 100 }],
			["t", { x: 320, y: 100 }],
		]),
		context: {
			traceEvent: {
				catalog_id: catalogId,
				detail: { label: "scale", value: "4" },
			},
			residualArcs: [
				{ edge_id: "e", direction: "forward", capacity: "8" },
				{ edge_id: "e", direction: "reverse", capacity: "2" },
			],
		},
		plan: { overlayPresentation: { activeFields: [] } },
		renderData: {
			overlayViews: {
				cancelTighten: undefined,
				orlinMaxFlow: undefined,
				eibfs: undefined,
				dynamicEibfs: undefined,
			},
			cancelTightenAdmissibleArcKeys: new Set(),
			maximumOrlinMcfBranchFlow: 1n,
			orlinMcfCapacityNodeByEdge: new Map(),
			orlinMcfComponentById: new Map(),
			orlinMcfContractionKey: undefined,
			orlinMcfPathArcKeys: new Set(),
			orlinMaxNodeById: new Map(),
			orlinMaxResidualByKey: new Map(),
		},
	} as never;
	const visual = {
		edge: { id: "e", from: "s", to: "t" },
		railWidth: 4,
		geometry: {
			path: "M 80 100 L 320 100",
			reversePath: "M 320 100 L 80 100",
		},
	} as never;
	return renderToStaticMarkup(
		<FlowGraphIdScopeProvider scope="scaling-phase-test">
			<svg>
				<title>Capacity-scaling phase</title>
				<FlowGraphDiscreteEdgeUnderlayBundle
					state={state}
					visual={visual}
					enabled
				/>
			</svg>
		</FlowGraphIdScopeProvider>,
	);
}

describe("Cancel-and-Tighten phase frontier", () => {
	it("paints the exact directed admissible residual frontier at phase entry", () => {
		const svg = renderCancelPhase("begin-phase");
		expect(svg).toContain("flow-cancel-tighten-phase-arc");
		expect(svg).toContain('data-overlay-entity-kind="residual-arc"');
		expect(svg).toContain('data-overlay-entity-id="e"');
		expect(svg).toContain('data-overlay-residual-direction="reverse"');
		expect(svg).toContain(
			'data-overlay-role="cancel_tighten_overlay:admissible_arcs.phase-frontier"',
		);
		expect(svg).toContain('d="M 320 100 L 80 100"');
		expect(svg).toContain(
			'marker-end="url(#cancel-phase-test-flow-arrow-residual-active)"',
		);
	});

	it("does not imply an active phase while prices are only initialized", () => {
		expect(renderCancelPhase("initialize")).not.toContain(
			"flow-cancel-tighten-phase-arc",
		);
	});
});

describe("Capacity-scaling phase frontier", () => {
	it("draws only residual directions meeting the exact phase scale", () => {
		const svg = renderCapacityScalingPhase("start");
		expect(svg).toContain("flow-capacity-scaling-phase-arc-start");
		expect(svg).toContain('data-capacity-scaling-direction="forward"');
		expect(svg).not.toContain('data-capacity-scaling-direction="reverse"');
		expect(svg).toContain('data-capacity-scaling-scale="4"');
		expect(svg).toContain('data-capacity-scaling-residual="8"');
		expect(svg).toContain(
			'marker-end="url(#scaling-phase-test-flow-arrow-capacity-scaling)"',
		);
	});

	it("uses a distinct quiet boundary after completing the same scale", () => {
		const svg = renderCapacityScalingPhase("complete");
		expect(svg).toContain("flow-capacity-scaling-phase-arc-complete");
		expect(svg).toContain('data-capacity-scaling-boundary="complete"');
	});
});
