import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { FlowGraphOptimizationNodeFeatureBundle } from "./FlowGraphOptimizationNodeFeatureBundle";
import { FlowGraphAuxiliaryCellLayer } from "./FlowGraphOverlayFeatureLayers";

function activePlan() {
	return {
		overlayPresentation: { activeFields: ["relaxed_mndc_overlay"] },
	};
}

describe("Relaxed most-negative-cycle graph features", () => {
	it("shows the exact source-published split-node construction without a node ring", () => {
		const state = {
			plan: activePlan(),
			context: {
				traceEvent: { entity_refs: [] },
				traceEventSemantics: {
					changed_entity_refs: [{ kind: "node", node_id: "v" }],
				},
			},
			renderData: {
				overlayViews: {
					relaxedMndc: { stage: "initialize" },
				},
				tardosNodeById: new Map(),
				predictionNodeById: new Map(),
				binaryNodeById: new Map(),
				relaxedMndcFamilyNodeBand: new Map(),
				enhancedScalingNodeById: new Map(),
				orlinMcfOriginalNodeById: new Map(),
				dualSimplexNodeById: new Map(),
				polynomialDualNodeById: new Map(),
				polynomialPrimalNodeById: new Map(),
				doubleScalingOriginalNodeById: new Map(),
			},
		} as never;

		const svg = renderToStaticMarkup(
			<svg>
				<title>Split-node construction</title>
				<FlowGraphOptimizationNodeFeatureBundle
					state={state}
					nodeId="v"
					supernode={false}
					enabled
				/>
			</svg>,
		);

		expect(svg).toContain("flow-mndc-split-node-glyph");
		expect(svg).toContain('data-overlay-contribution="relaxed_mndc_overlay"');
		expect(svg).toContain('data-overlay-entity-kind="node"');
		expect(svg).toContain('data-overlay-entity-id="v"');
		expect(svg).toContain(
			'data-overlay-role="relaxed_mndc_overlay:trace_event.changed-entity.split-node-copies"',
		);
		expect(svg).not.toContain("flow-mndc-family-ring");
	});

	it("binds an assignment-cell scan to its published row/column relation", () => {
		const state = {
			plan: activePlan(),
			positions: new Map([
				["row", { x: 120, y: 180 }],
				["column", { x: 360, y: 180 }],
			]),
			context: {
				model: { kind: "min-cost-flow" },
				traceEvent: {
					catalog_id: "relaxed-most-negative-cycle.inspect-assignment-cell",
					pseudocode_line: "inspect_assignment_cell()",
					entity_refs: [
						{ kind: "node", node_id: "row" },
						{ kind: "node", node_id: "column" },
					],
					detail: { label: "assignment cell scan", value: "7" },
				},
				traceEventSemantics: {
					work_progress: { primary_total: "24" },
				},
			},
		} as never;

		const svg = renderToStaticMarkup(
			<svg>
				<title>Assignment cell scan</title>
				<FlowGraphAuxiliaryCellLayer state={state} />
			</svg>,
		);

		expect(svg).toContain('data-auxiliary-cell="assignment"');
		expect(svg).toContain('data-matrix-cell-progress="7:24"');
		expect(svg).toContain('data-overlay-contribution="relaxed_mndc_overlay"');
		expect(svg).toContain('data-overlay-entity-kind="auxiliary-edge"');
		expect(svg).toContain(
			'data-overlay-entity-id="assignment-cell:row:column"',
		);
		expect(svg).toContain(
			'data-overlay-role="relaxed_mndc_overlay:active_assignment_cell"',
		);
	});
});
