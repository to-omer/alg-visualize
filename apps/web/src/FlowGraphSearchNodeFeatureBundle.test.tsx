import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { FlowGraphSearchNodeFeatureBundle } from "./FlowGraphSearchNodeFeatureBundle";

function renderScalingNode(remaining: string): string {
	const state = {
		context: {
			algorithmId: "capacity-scaling-mcf",
			traceEvent: {
				catalog_id: "capacity-scaling-mcf.start-scaling-phase",
				detail: { label: "scale", value: "4" },
			},
		},
		visualization: {
			ibfsView: undefined,
			eibfsView: undefined,
			features: { transportation: false },
			nodeTraceStates: new Map([["v", { remaining_divergence: remaining }]]),
			matchingCoverNodes: new Set(),
			assignmentHallNodes: new Set(),
		},
		renderData: {
			overlayViews: { eibfs: undefined, dynamicEibfs: undefined },
		},
		hasForestOverlay: false,
		forestChildIds: new Set(),
	} as never;
	return renderToStaticMarkup(
		<svg>
			<title>Capacity-scaling node gate</title>
			<FlowGraphSearchNodeFeatureBundle
				state={state}
				nodeId="v"
				kind="normal"
				nodeBalance={0n}
				overlayEnabled={false}
			/>
		</svg>,
	);
}

describe("FlowGraphSearchNodeFeatureBundle capacity scale", () => {
	it("marks only a node whose remaining excess reaches Δ", () => {
		const eligible = renderScalingNode("7");
		expect(eligible).toContain("flow-capacity-scaling-node-gate-excess");
		expect(eligible).toContain('data-capacity-scaling-node-remaining="7"');
		expect(eligible).toContain('data-capacity-scaling-node-scale="4"');
		expect(eligible).toContain("Δ+");

		expect(renderScalingNode("3")).not.toContain(
			"flow-capacity-scaling-node-gate",
		);
	});

	it("keeps a Δ-eligible deficit distinct from excess", () => {
		const svg = renderScalingNode("-9");
		expect(svg).toContain("flow-capacity-scaling-node-gate-deficit");
		expect(svg).toContain("Δ−");
	});
});
