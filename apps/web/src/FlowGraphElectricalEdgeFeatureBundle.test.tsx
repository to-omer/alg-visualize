import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { FlowGraphElectricalEdgeFeatureBundle } from "./FlowGraphElectricalEdgeFeatureBundle";
import { FlowGraphIdScopeProvider } from "./flow-dom-id";

function renderRoundedEdge(centralFlow: string, roundedFlow: string): string {
	const state = {
		plan: {
			overlayPresentation: {
				activeFields: ["augmenting_electrical_overlay"],
			},
		},
		renderData: { overlayViews: { electricalFlow: undefined } },
	} as never;
	const visual = {
		edge: { id: "e" },
		capacity: 8n,
		railWidth: 9,
		augmentingCentralWidth: 4,
		augmentingCongestionBand: 0,
		geometry: {
			path: "M 10 20 L 90 20",
			reversePath: "M 90 20 L 10 20",
		},
		augmentingElectricalState: {
			boost_segments: "1",
			central_flow: centralFlow,
			electrical_current: "0",
			congestion: "0",
			rounded_central_flow: roundedFlow,
		},
	} as never;
	return renderToStaticMarkup(
		<FlowGraphIdScopeProvider scope="rounded-flow-test">
			<svg>
				<title>Rounded central-flow edge</title>
				<FlowGraphElectricalEdgeFeatureBundle
					state={state}
					visual={visual}
					enabled
				/>
			</svg>
		</FlowGraphIdScopeProvider>,
	);
}

function renderElectricalRecovery(stage: string): string {
	const edgeState = {
		edge_id: "e",
		fixed_on_face: false,
		electrical_current: "0",
	};
	const state = {
		plan: {
			overlayPresentation: {
				activeFields: ["electrical_ipm_mcf_overlay"],
			},
		},
		renderData: {
			overlayViews: {
				electricalFlow: undefined,
				electricalIpmMcf: {
					stage,
					duality_gap_bound: "0.00004",
					recovery_epsilon: "0.00005",
					edges: [edgeState],
				},
			},
		},
	} as never;
	const visual = {
		edge: { id: "e" },
		capacity: 8n,
		railWidth: 9,
		geometry: {
			path: "M 10 20 L 90 20",
			reversePath: "M 90 20 L 10 20",
			routeMidpoint: { x: 50, y: 20 },
		},
		electricalIpmMcfState: edgeState,
		electricalIpmMcfResistanceBand: 0,
		electricalIpmMcfSlackBand: 0,
		electricalIpmMcfFractionalWidth: 2,
		electricalIpmMcfCurrentWidth: 0,
	} as never;
	return renderToStaticMarkup(
		<FlowGraphIdScopeProvider scope="electrical-recovery-test">
			<svg>
				<title>Electrical IPM exact recovery</title>
				<FlowGraphElectricalEdgeFeatureBundle
					state={state}
					visual={visual}
					enabled
				/>
			</svg>
		</FlowGraphIdScopeProvider>,
	);
}

describe("augmenting electrical rounded-flow projection", () => {
	it("paints an exact changed continuous-to-integer transition on its edge", () => {
		const svg = renderRoundedEdge("2.625", "3");
		expect(svg).toContain("flow-augmenting-rounded-flow-changed");
		expect(svg).toContain(
			'data-overlay-role="augmenting_electrical_overlay:edges.rounded-central-flow"',
		);
		expect(svg).toContain("Rounded central flow on e: 2.625 → 3");
		expect(svg).toContain(
			'marker-end="url(#rounded-flow-test-flow-arrow-augmenting-electrical-central)"',
		);
		expect(svg).toContain(
			'marker-end="url(#rounded-flow-test-flow-arrow-augmenting-electrical-rounded)"',
		);
	});

	it("keeps already-integral coordinates visually subordinate", () => {
		const svg = renderRoundedEdge("2", "2");
		expect(svg).toContain("flow-augmenting-rounded-flow-stable");
		expect(svg).not.toContain("flow-augmenting-rounded-flow-changed");
	});

	it("does not imply a direction for an exact zero rounded result", () => {
		const svg = renderRoundedEdge("0.625", "0");
		expect(svg).toContain(
			'marker-end="url(#rounded-flow-test-flow-arrow-augmenting-electrical-central)"',
		);
		expect(svg).not.toContain("flow-arrow-augmenting-electrical-rounded");
	});
});

describe("electrical MCF exact-recovery projection", () => {
	it("attaches the proved 2mμ ≤ ε threshold to the rounded edge domain", () => {
		const reducing = renderElectricalRecovery("decrease-barrier");
		const approximate = renderElectricalRecovery("approximate-flow");

		expect(reducing).not.toContain("flow-eipm-main-recovery-badge");
		expect(approximate).toContain("flow-eipm-main-recovery-badge");
		expect(approximate).toContain("2mμ ≤ ε · ROUND");
		expect(approximate).toContain(
			'data-overlay-role="electrical_ipm_mcf_overlay:edges.exact-recovery-threshold"',
		);
		expect(approximate).toContain("duality gap 0.00004 ≤ epsilon 0.00005");
		expect(approximate).not.toBe(reducing);
	});
});
