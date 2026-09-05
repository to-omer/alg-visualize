import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { FlowGraphOptimizationNodeFeatureBundle } from "./FlowGraphOptimizationNodeFeatureBundle";

function renderImbalance(
	imbalance: string,
	stage = "initialize",
	delta = "0",
): string {
	const state = {
		plan: {
			overlayPresentation: { activeFields: ["double_scaling_overlay"] },
		},
		context: {
			traceEvent: { entity_refs: [] },
			traceEventSemantics: { changed_entity_refs: [] },
		},
		renderData: {
			overlayViews: { doubleScaling: { stage, delta } },
			tardosNodeById: new Map(),
			predictionNodeById: new Map(),
			binaryNodeById: new Map(),
			relaxedMndcFamilyNodeBand: new Map(),
			enhancedScalingNodeById: new Map(),
			enhancedScalingComponentById: new Map(),
			orlinMcfOriginalNodeById: new Map(),
			dualSimplexNodeById: new Map(),
			polynomialDualNodeById: new Map(),
			polynomialPrimalNodeById: new Map(),
			doubleScalingOriginalNodeById: new Map([
				["s", { entity_id: "s", price: "0", imbalance, cursor: "0" }],
			]),
		},
	} as never;
	return renderToStaticMarkup(
		<svg>
			<title>Double-scaling node state</title>
			<FlowGraphOptimizationNodeFeatureBundle
				state={state}
				nodeId="s"
				supernode={false}
				enabled
			/>
		</svg>,
	);
}

describe("Double-scaling graph features", () => {
	it("binds a compact excess marker to the exact overlay node", () => {
		const svg = renderImbalance("7");
		expect(svg).toContain("flow-double-scaling-imbalance-excess");
		expect(svg).toContain('data-double-scaling-imbalance="7"');
		expect(svg).toContain('data-overlay-contribution="double_scaling_overlay"');
		expect(svg).toContain(
			'data-overlay-role="double_scaling_overlay:nodes.imbalance-state"',
		);
		expect(svg).toContain('data-overlay-entity-kind="node"');
		expect(svg).toContain('data-overlay-entity-id="s"');
	});

	it("uses the opposite directional glyph for a deficit", () => {
		const svg = renderImbalance("-3");
		expect(svg).toContain("flow-double-scaling-imbalance-deficit");
		expect(svg).toContain("M -38 21 L -31 33 L -24 21 Z");
	});

	it("renders a quiet source-owned state mark for a balanced node", () => {
		const svg = renderImbalance("0");
		expect(svg).toContain("flow-double-scaling-imbalance-balanced");
		expect(svg).toContain(
			'data-overlay-role="double_scaling_overlay:nodes.imbalance-state"',
		);
	});

	it("marks only nodes admitted by the exact capacity-scale gate", () => {
		const eligible = renderImbalance("-8", "start-capacity-phase", "8");
		expect(eligible).toContain("flow-double-scaling-delta-gate-deficit");
		expect(eligible).toContain('data-double-scaling-delta="8"');
		expect(eligible).toContain(
			'data-overlay-role="double_scaling_overlay:nodes.delta-gate"',
		);

		expect(renderImbalance("7", "start-capacity-phase", "8")).not.toContain(
			"flow-double-scaling-delta-gate",
		);
		expect(renderImbalance("8", "start-cost-phase", "8")).not.toContain(
			"flow-double-scaling-delta-gate",
		);
	});
});
