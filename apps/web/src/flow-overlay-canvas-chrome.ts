import type { FlowOverlayViews } from "./flow-overlay-contribution-registry";

/** Keeps overlay-specific canvas framing out of the workspace shell. */
export function flowOverlayCanvasClassName(
	views: FlowOverlayViews | undefined,
	parametric: boolean,
): string {
	return [
		"flow-canvas-panel",
		views?.dynamicEibfs === undefined ? undefined : "flow-canvas-panel-dynamic",
		parametric ? "flow-canvas-panel-parametric" : undefined,
		views?.electricalFlow === undefined &&
		views?.augmentingElectrical === undefined &&
		views?.interiorPointMaxFlow === undefined
			? undefined
			: "flow-canvas-panel-electrical",
		views?.minimumRatioCycle === undefined
			? undefined
			: "flow-canvas-panel-minimum-ratio",
		views?.minimumRatioCycleMcf === undefined
			? undefined
			: "flow-canvas-panel-minimum-ratio-mcf",
		views?.randomizedAlmostLinearMcf === undefined
			? undefined
			: "flow-canvas-panel-randomized-almost-linear-mcf-oracle-demonstrator",
		views?.flowFrameworkMcf === undefined
			? undefined
			: "flow-canvas-panel-deterministic-almost-linear-mcf",
		views?.randomizedAlmostLinear === undefined
			? undefined
			: "flow-canvas-panel-randomized-almost-linear",
		views?.deterministicAlmostLinear === undefined
			? undefined
			: "flow-canvas-panel-deterministic-almost-linear",
		views?.primalDualIpmMcf === undefined &&
		views?.electricalIpmMcf === undefined
			? undefined
			: "flow-canvas-panel-ipm-mcf",
	]
		.filter((className) => className !== undefined)
		.join(" ");
}

export function flowOverlayCanvasMessage(
	views: FlowOverlayViews | undefined,
	level: "detail" | "structure" | "overview" | undefined,
): string | undefined {
	if (views?.electricalFlow !== undefined) {
		return "The outer rail shows capacity u. The teal or violet inner stroke shows signed current I, its width shows congestion |I|/u, and its glow shows energy I²R. Node rings encode potential φ; ⏚ marks the grounded sink.";
	}
	if (views?.orlinMaxFlow !== undefined) {
		return "A/Ā/M/S identify residual classes, K/C identify critical or compactible components, and O/P/T identify original, abundant-pseudo, or transferred-pseudo compact arcs. Outer width encodes capacity; inner width encodes flow.";
	}
	if (views?.orlinMcf !== undefined) {
		return "The center diamond is the capacity node created from a finite-capacity edge. Its F and S branches show flow and unused capacity. Width, tightness, 3nΔ candidates, and the compressed path are shown together.";
	}
	if (views?.tardosFramework !== undefined) {
		return "Input potential π prices every residual direction. The view shows ε=max(0,−min c̄) and the exact threshold nε. Only an orange double stroke certifies that an original variable is fixed at L or U.";
	}
	if (views?.predictionAssistedEpsilon !== undefined) {
		return "Violet prediction rings, magenta clipping rings, current price pₜ, scaled cost cₜ, orange ε-balanced arcs, and the T ladder are shown together.";
	}
	if (views?.convexCost !== undefined) {
		return "The outer rail shows capacity. The inner stroke shows flow and the next forward marginal cost. The segmented rail shows each cost segment's width, slope, and utilization. φ is current cost; μ+ and μ− are forward and reverse residual marginal costs.";
	}
	if (level === "overview") {
		return "All nodes and edges are aggregated into spatial clusters. Stroke width, color, dash pattern, and intensity preserve aggregate capacity, flow, cost sign, and absolute cost.";
	}
	return undefined;
}
