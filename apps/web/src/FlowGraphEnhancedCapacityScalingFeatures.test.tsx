import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { FlowGraphEnhancedScalingComponentLayer } from "./FlowGraphAlgorithmLayers";
import { FlowGraphOptimizationNodeFeatureBundle } from "./FlowGraphOptimizationNodeFeatureBundle";
import { enhancedCapacityScalingGateStatus } from "./flow-graph-rational-scales";

function renderGate(
	stage: "initialize" | "begin-phase" | "halve-scale",
	nodeId: string,
	excess: string,
): string {
	const component = {
		component_id: "a",
		members: ["a", "a-child"],
		excess: { numerator: excess, denominator: "2" },
	};
	const state = {
		plan: {
			overlayPresentation: {
				activeFields: ["enhanced_capacity_scaling_overlay"],
			},
		},
		context: {
			traceEvent: { entity_refs: [] },
			traceEventSemantics: { changed_entity_refs: [] },
		},
		renderData: {
			overlayViews: {
				enhancedCapacityScaling: {
					stage,
					delta: { numerator: "4", denominator: "1" },
				},
			},
			tardosNodeById: new Map(),
			predictionNodeById: new Map(),
			binaryNodeById: new Map(),
			relaxedMndcFamilyNodeBand: new Map(),
			enhancedScalingNodeById: new Map([
				["a", { node_id: "a", component_id: "a" }],
				["a-child", { node_id: "a-child", component_id: "a" }],
			]),
			enhancedScalingComponentById: new Map([["a", component]]),
			orlinMcfOriginalNodeById: new Map(),
			dualSimplexNodeById: new Map(),
			polynomialDualNodeById: new Map(),
			polynomialPrimalNodeById: new Map(),
			doubleScalingOriginalNodeById: new Map(),
		},
	} as never;
	return renderToStaticMarkup(
		<svg>
			<title>Enhanced capacity-scaling phase gate</title>
			<FlowGraphOptimizationNodeFeatureBundle
				state={state}
				nodeId={nodeId}
				supernode={false}
				enabled
			/>
		</svg>,
	);
}

describe("Enhanced capacity-scaling graph features", () => {
	it("uses the kernel's exact three-quarter-delta component gate", () => {
		expect(
			enhancedCapacityScalingGateStatus(
				{ numerator: "15", denominator: "4" },
				{ numerator: "5", denominator: "1" },
			),
		).toBe("excess");
		expect(
			enhancedCapacityScalingGateStatus(
				{ numerator: "-15", denominator: "4" },
				{ numerator: "5", denominator: "1" },
			),
		).toBe("deficit");
		expect(
			enhancedCapacityScalingGateStatus(
				{ numerator: "14", denominator: "4" },
				{ numerator: "5", denominator: "1" },
			),
		).toBe("below");
	});

	it("marks only the representative node of an eligible quotient component", () => {
		const representative = renderGate("begin-phase", "a", "6");
		expect(representative).toContain("flow-enhanced-phase-gate-excess");
		expect(representative).toContain("+≥¾Δ");
		expect(representative).toContain(
			'data-overlay-contribution="enhanced_capacity_scaling_overlay"',
		);
		expect(representative).toContain(
			'data-overlay-role="enhanced_capacity_scaling_overlay:components.excess-three-quarter-delta-active"',
		);
		expect(renderGate("begin-phase", "a-child", "6")).not.toContain(
			"flow-enhanced-phase-gate",
		);
	});

	it("keeps deficit, next-scale, and inactive boundaries distinct", () => {
		const deficit = renderGate("begin-phase", "a", "-7");
		expect(deficit).toContain("flow-enhanced-phase-gate-deficit");
		expect(deficit).toContain("−≥¾Δ");

		const next = renderGate("halve-scale", "a", "6");
		expect(next).toContain("flow-enhanced-phase-gate-next");
		expect(next).toContain("NEXT ");
		expect(renderGate("initialize", "a", "6")).not.toContain(
			"flow-enhanced-phase-gate",
		);
	});

	it("publishes a quiet per-component rejection mark when no component reaches the gate", () => {
		const state = {
			plan: {
				overlayPresentation: {
					activeFields: ["enhanced_capacity_scaling_overlay"],
				},
			},
			enhancedScalingComponentBoxes: [
				{
					component_id: "a",
					members: ["a"],
					excess: { numerator: "1", denominator: "2" },
					x: 10,
					y: 20,
					width: 76,
					height: 76,
					activeRole: undefined,
				},
			],
			renderData: {
				overlayViews: {
					enhancedCapacityScaling: {
						stage: "begin-phase",
						delta: { numerator: "4", denominator: "1" },
					},
				},
			},
		} as never;
		const svg = renderToStaticMarkup(
			<svg>
				<title>Rejected quotient component</title>
				<FlowGraphEnhancedScalingComponentLayer state={state} />
			</svg>,
		);

		expect(svg).toContain("flow-enhanced-component-gate-below");
		expect(svg).toContain('data-enhanced-component-gate="below"');
		expect(svg).toContain(
			'data-overlay-role="enhanced_capacity_scaling_overlay:components.below-three-quarter-delta-active"',
		);
	});
});
