import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { FlowGraphOptimizationNodeFeatureBundle } from "./FlowGraphOptimizationNodeFeatureBundle";
import { polynomialDualScaleGateStatus } from "./flow-graph-rational-scales";

function renderGate(excess: string, root = false): string {
	const state = {
		plan: {
			overlayPresentation: {
				activeFields: ["polynomial_dual_simplex_overlay"],
			},
		},
		context: {
			traceEvent: { entity_refs: [] },
			traceEventSemantics: { changed_entity_refs: [] },
		},
		renderData: {
			overlayViews: {
				polynomialDualSimplex: {
					stage: "begin-scale",
					delta: { numerator: "3", denominator: "2" },
				},
			},
			tardosNodeById: new Map(),
			predictionNodeById: new Map(),
			binaryNodeById: new Map(),
			relaxedMndcFamilyNodeBand: new Map(),
			enhancedScalingNodeById: new Map(),
			enhancedScalingComponentById: new Map(),
			orlinMcfOriginalNodeById: new Map(),
			dualSimplexNodeById: new Map(),
			polynomialDualNodeById: new Map([
				[
					"v",
					{
						node_id: "v",
						potential: "0",
						excess: { numerator: excess, denominator: "4" },
						root,
						active: false,
						bad: false,
						in_pivot_cut: false,
					},
				],
			]),
			polynomialPrimalNodeById: new Map(),
			doubleScalingOriginalNodeById: new Map(),
		},
	} as never;
	return renderToStaticMarkup(
		<svg>
			<title>Polynomial dual scale gate</title>
			<FlowGraphOptimizationNodeFeatureBundle
				state={state}
				nodeId="v"
				supernode={false}
				enabled
			/>
		</svg>,
	);
}

describe("Polynomial-dual delta gate", () => {
	it("uses the source algorithm's strict rational comparison", () => {
		expect(
			polynomialDualScaleGateStatus(
				{ numerator: "6", denominator: "4" },
				{ numerator: "3", denominator: "2" },
			),
		).toBe("below");
		expect(
			polynomialDualScaleGateStatus(
				{ numerator: "7", denominator: "4" },
				{ numerator: "3", denominator: "2" },
			),
		).toBe("active");
	});

	it("paints exact active and rejected node states with ownership", () => {
		const active = renderGate("7");
		expect(active).toContain("flow-polynomial-dual-scale-gate-active");
		expect(active).toContain(
			'data-overlay-role="polynomial_dual_simplex_overlay:nodes.delta-gate.active"',
		);
		expect(active).toContain('data-overlay-entity-id="v"');

		expect(renderGate("6")).toContain("flow-polynomial-dual-scale-gate-below");
	});

	it("does not treat the fixed root as an active-search candidate", () => {
		expect(renderGate("7", true)).not.toContain(
			"flow-polynomial-dual-scale-gate",
		);
	});
});
