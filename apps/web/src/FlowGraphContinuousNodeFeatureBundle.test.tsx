import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { FlowGraphContinuousNodeFeatureBundle } from "./FlowGraphContinuousNodeFeatureBundle";

function state() {
	return {
		plan: {
			overlayPresentation: {
				activeFields: ["augmenting_electrical_overlay"],
			},
		},
		renderData: {
			overlayViews: {
				augmentingElectrical: {
					active_working_path: [{ from_node: "s", to_node: "t" }],
				},
				electricalFlow: undefined,
			},
			augmentingElectricalNodeById: new Map([
				[
					"s",
					{
						potential: "-4",
						coupling_violation: "1",
						target_source_side: true,
					},
				],
				[
					"a",
					{
						potential: "-2",
						coupling_violation: "0.00000005",
						target_source_side: false,
					},
				],
			]),
			maximumAugmentingCoupling: 1,
			augmentingPotentialBand: () => 2,
			interiorPointNodeById: new Map(),
			minimumRatioNodeById: new Map(),
			randomizedAlmostLinearNodeById: new Map(),
			deterministicAlmostLinearNodeById: new Map(),
			electricalNodeById: new Map(),
			electricalIpmMcfNodeById: new Map(),
		},
	} as never;
}

function render(nodeId: string): string {
	return renderToStaticMarkup(
		<svg>
			<title>Augmenting-electrical node channels</title>
			<FlowGraphContinuousNodeFeatureBundle
				state={state()}
				nodeId={nodeId}
				enabled
			/>
		</svg>,
	);
}

describe("augmenting-electrical node channels", () => {
	it("keeps persistent embedding and cut channels subordinate off the active path", () => {
		const svg = render("a");
		expect(svg).toContain("flow-augmenting-potential-ring");
		expect(svg).toContain("flow-augmenting-target-cut-ring");
		expect(svg).not.toContain("flow-augmenting-coupling-ring");
		expect(svg).not.toContain("flow-augmenting-node-active");
	});

	it("emphasizes only path nodes and material relative coupling violations", () => {
		const svg = render("s");
		expect(svg).toContain("flow-augmenting-coupling-ring");
		expect(svg.match(/flow-augmenting-node-active/gu)).toHaveLength(3);
	});
});
