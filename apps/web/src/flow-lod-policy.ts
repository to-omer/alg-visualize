export type FlowCanvasLod = "detail" | "structure" | "overview";

export type FlowLodEntityCounts = Readonly<{
	nodes: number;
	edges: number;
}>;

export type FlowLodViewport = Readonly<{
	width: number;
	height: number;
}>;

export type FlowLodPolicyInput = Readonly<{
	current: FlowCanvasLod | undefined;
	zoom: number;
	viewport: FlowLodViewport;
	entityCounts: FlowLodEntityCounts;
}>;

export type FlowLodPolicy = Readonly<{
	referenceViewport: FlowLodViewport;
	detailCapacity: FlowLodEntityCounts;
	structureCapacity: FlowLodEntityCounts;
	absoluteDetailLimit: FlowLodEntityCounts;
	absoluteStructureLimit: FlowLodEntityCounts;
	minimumViewportScale: number;
	maximumViewportScale: number;
	maximumZoomContribution: number;
	hysteresis: number;
}>;

export const DEFAULT_FLOW_LOD_POLICY: FlowLodPolicy = Object.freeze({
	referenceViewport: Object.freeze({ width: 900, height: 540 }),
	detailCapacity: Object.freeze({ nodes: 50, edges: 64 }),
	structureCapacity: Object.freeze({ nodes: 600, edges: 1_200 }),
	absoluteDetailLimit: Object.freeze({ nodes: 58, edges: 74 }),
	absoluteStructureLimit: Object.freeze({ nodes: 2_500, edges: 12_000 }),
	minimumViewportScale: 0.25,
	maximumViewportScale: 4,
	maximumZoomContribution: 64,
	hysteresis: 0.15,
});

const FLOW_LOD_RANK: Readonly<Record<FlowCanvasLod, number>> = Object.freeze({
	detail: 0,
	structure: 1,
	overview: 2,
});

/**
 * Keeps scene-specific spatial safety decisions authoritative while still
 * allowing viewport pressure to reduce detail further.
 */
export function constrainFlowCanvasLodToBaseline(
	requested: FlowCanvasLod,
	baseline: FlowCanvasLod,
): FlowCanvasLod {
	return FLOW_LOD_RANK[requested] < FLOW_LOD_RANK[baseline]
		? baseline
		: requested;
}

function finitePositive(value: number): boolean {
	return Number.isFinite(value) && value > 0;
}

function validCount(value: number): boolean {
	return Number.isSafeInteger(value) && value >= 0;
}

function within(
	counts: FlowLodEntityCounts,
	limits: FlowLodEntityCounts,
): boolean {
	return counts.nodes <= limits.nodes && counts.edges <= limits.edges;
}

function pressure(
	counts: FlowLodEntityCounts,
	capacity: FlowLodEntityCounts,
	scale: number,
): number {
	return Math.max(
		counts.nodes / (capacity.nodes * scale),
		counts.edges / (capacity.edges * scale),
	);
}

function validPolicy(policy: FlowLodPolicy): boolean {
	return (
		finitePositive(policy.referenceViewport.width) &&
		finitePositive(policy.referenceViewport.height) &&
		finitePositive(policy.detailCapacity.nodes) &&
		finitePositive(policy.detailCapacity.edges) &&
		finitePositive(policy.structureCapacity.nodes) &&
		finitePositive(policy.structureCapacity.edges) &&
		finitePositive(policy.absoluteDetailLimit.nodes) &&
		finitePositive(policy.absoluteDetailLimit.edges) &&
		finitePositive(policy.absoluteStructureLimit.nodes) &&
		finitePositive(policy.absoluteStructureLimit.edges) &&
		policy.detailCapacity.nodes <= policy.structureCapacity.nodes &&
		policy.detailCapacity.edges <= policy.structureCapacity.edges &&
		policy.detailCapacity.nodes <= policy.absoluteDetailLimit.nodes &&
		policy.detailCapacity.edges <= policy.absoluteDetailLimit.edges &&
		policy.absoluteDetailLimit.nodes <= policy.absoluteStructureLimit.nodes &&
		policy.absoluteDetailLimit.edges <= policy.absoluteStructureLimit.edges &&
		finitePositive(policy.minimumViewportScale) &&
		finitePositive(policy.maximumViewportScale) &&
		policy.maximumViewportScale >= policy.minimumViewportScale &&
		finitePositive(policy.maximumZoomContribution) &&
		Number.isFinite(policy.hysteresis) &&
		policy.hysteresis > 0 &&
		policy.hysteresis < 1
	);
}

function inputIsValid(input: FlowLodPolicyInput): boolean {
	return (
		finitePositive(input.zoom) &&
		finitePositive(input.viewport.width) &&
		finitePositive(input.viewport.height) &&
		validCount(input.entityCounts.nodes) &&
		validCount(input.entityCounts.edges)
	);
}

function capacityScale(
	input: FlowLodPolicyInput,
	policy: FlowLodPolicy,
): number {
	// SVG uses xMidYMid meet. Only the rendered graph rectangle, not its
	// letterbox, contributes space for readable entities.
	const fitScale = Math.min(
		input.viewport.width / policy.referenceViewport.width,
		input.viewport.height / policy.referenceViewport.height,
	);
	const viewportScale = Math.min(
		policy.maximumViewportScale,
		Math.max(policy.minimumViewportScale, fitScale * fitScale),
	);
	const zoomContribution = Math.min(
		policy.maximumZoomContribution,
		input.zoom * input.zoom,
	);
	return viewportScale * zoomContribution;
}

/** Applies hard allocation limits even when a caller carries stale LOD state. */
export function constrainFlowCanvasLodToRenderLimits(
	requested: FlowCanvasLod,
	entityCounts: FlowLodEntityCounts,
	policy: FlowLodPolicy = DEFAULT_FLOW_LOD_POLICY,
): FlowCanvasLod {
	if (
		!validPolicy(policy) ||
		!validCount(entityCounts.nodes) ||
		!validCount(entityCounts.edges)
	) {
		return "overview";
	}
	if (requested === "overview") return requested;
	if (requested === "structure") {
		return within(entityCounts, policy.absoluteStructureLimit)
			? requested
			: "overview";
	}
	if (within(entityCounts, policy.absoluteDetailLimit)) return requested;
	return within(entityCounts, policy.absoluteStructureLimit)
		? "structure"
		: "overview";
}

/**
 * Chooses render detail from visible density. Invalid inputs retain an existing
 * level, or select Overview when no safe previous level exists.
 */
export function chooseFlowCanvasLod(
	input: FlowLodPolicyInput,
	policy: FlowLodPolicy = DEFAULT_FLOW_LOD_POLICY,
): FlowCanvasLod {
	if (!validPolicy(policy) || !inputIsValid(input)) {
		return "overview";
	}
	const scale = capacityScale(input, policy);
	const detailPressure = pressure(
		input.entityCounts,
		policy.detailCapacity,
		scale,
	);
	const structurePressure = pressure(
		input.entityCounts,
		policy.structureCapacity,
		scale,
	);
	const detailAllowed = within(input.entityCounts, policy.absoluteDetailLimit);
	const structureAllowed = within(
		input.entityCounts,
		policy.absoluteStructureLimit,
	);
	const enterThreshold = 1 - policy.hysteresis;
	const leaveThreshold = 1 + policy.hysteresis;

	if (input.current === "detail") {
		if (detailAllowed && detailPressure <= leaveThreshold) return "detail";
		return structureAllowed && structurePressure <= leaveThreshold
			? "structure"
			: "overview";
	}
	if (input.current === "structure") {
		if (detailAllowed && detailPressure < enterThreshold) return "detail";
		return structureAllowed && structurePressure <= leaveThreshold
			? "structure"
			: "overview";
	}
	if (input.current === "overview") {
		if (detailAllowed && detailPressure < enterThreshold) return "detail";
		return structureAllowed && structurePressure < enterThreshold
			? "structure"
			: "overview";
	}
	if (detailAllowed && detailPressure <= 1) return "detail";
	if (structureAllowed && structurePressure <= 1) return "structure";
	return "overview";
}
