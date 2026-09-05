import { FLOW_LEGACY_TRACE_STEP_DIGESTS } from "./flow-legacy-trace-step-digests";
import {
	assertFlowOverlaySemanticBindings,
	FLOW_OVERLAY_CONTRIBUTION_ENTRIES,
	type FlowOverlaySemanticBindings,
} from "./flow-overlay-contribution-registry";
import { assertFlowCurrentSceneV9Wire } from "./flow-scene-wire/decode-v9";
import type * as FlowSceneWire from "./flow-scene-wire/generated/types";

export const FLOW_METRIC_COUNT = 16;

const utf8Encoder = new TextEncoder();
const PRIMARY_WORK_COUNTER_SUFFIX = ".primary-work-unit";

function canonicalStableIds(items: { id: string }[]): string[] {
	return items
		.map((item) => ({ id: item.id, bytes: utf8Encoder.encode(item.id) }))
		.sort((left, right) => {
			const sharedLength = Math.min(left.bytes.length, right.bytes.length);
			for (let index = 0; index < sharedLength; index += 1) {
				const difference = (left.bytes[index] ?? 0) - (right.bytes[index] ?? 0);
				if (difference !== 0) {
					return difference;
				}
			}
			return left.bytes.length - right.bytes.length;
		})
		.map(({ id }) => id);
}

function canonicalNodeIds(nodes: FlowNodeV1[]): string[] {
	return canonicalStableIds(nodes);
}

function validateTraceStageIdentity(
	traceEvent: FlowTraceEventV1 | undefined,
	algorithmId: string,
	stage: string,
	context: string,
): void {
	if (
		traceEventRequiresStageIdentity(traceEvent) &&
		traceEvent.catalog_id !== `${algorithmId}.${stage}`
	) {
		throw new Error(`${context} trace event and stage disagree`);
	}
}

function traceEventRequiresStageIdentity(
	traceEvent: FlowTraceEventV1 | undefined,
): traceEvent is FlowTraceEventV1 {
	return traceEvent !== undefined;
}

function traceDetailKeepsSourceLabel(
	detail: FlowTraceEventV1["detail"],
	sourceLabel: string,
): boolean {
	if (detail === undefined) return false;
	return (
		detail.label === sourceLabel ||
		detail.label.includes(` · ${sourceLabel} ${detail.value} · units `)
	);
}

export type FlowPositionV1 = FlowSceneWire.FlowPositionV1;

export type FlowNodeV1 = FlowSceneWire.FlowNodeV1;

export type FlowEdgeV1 = FlowSceneWire.FlowEdgeV1;

export type FlowPlanarDartV1 = FlowSceneWire.FlowPlanarDartV1;

export type FlowPlanarEmbeddingV1 = FlowSceneWire.FlowPlanarEmbeddingV1;

export type FlowRationalV1 = FlowSceneWire.FlowRationalV1;

export type FlowProblemModelV1 = FlowSceneWire.FlowProblemModelV1;

/**
 * Project the graph's node supplies into the exact outflow-minus-inflow target
 * used by every linear min-cost-flow certificate. A fixed-flow request is an
 * additional source-to-sink demand; it does not erase the graph's supplies.
 */
export function projectLinearMcfRequiredDivergence(
	model: FlowProblemModelV1,
	nodes: readonly FlowNodeV1[],
): ReadonlyMap<string, bigint> {
	if (
		model.kind !== "fixed-flow-min-cost" &&
		model.kind !== "circulation" &&
		model.kind !== "transshipment"
	) {
		throw new Error("Required divergence needs a linear MCF model");
	}
	const required = new Map<string, bigint>();
	for (const node of nodes) {
		if (required.has(node.id)) {
			throw new Error("Linear MCF divergence contains a duplicate node");
		}
		required.set(node.id, BigInt(node.supply));
	}
	if (model.kind === "fixed-flow-min-cost") {
		if (model.source === model.sink) {
			throw new Error("Fixed-flow MCF source and sink must differ");
		}
		const sourceSupply = required.get(model.source);
		const sinkSupply = required.get(model.sink);
		if (sourceSupply === undefined || sinkSupply === undefined) {
			throw new Error("Fixed-flow MCF terminal is missing from the graph");
		}
		const amount = BigInt(model.required_flow);
		required.set(model.source, sourceSupply + amount);
		required.set(model.sink, sinkSupply - amount);
	}
	if ([...required.values()].reduce((sum, value) => sum + value, 0n) !== 0n) {
		throw new Error("Linear MCF required divergence is unbalanced");
	}
	return required;
}

export type FlowAlgorithmStepContractV1 =
	FlowSceneWire.FlowAlgorithmStepContractV1;

export type FlowEdgeStateV1 = FlowSceneWire.FlowEdgeStateV1;

export type FlowResidualArcStateV1 = FlowSceneWire.FlowResidualArcStateV1;

export type FlowResidualArcRefV1 = FlowSceneWire.FlowResidualArcRefV1;

export type FlowNodeTraceStateV1 = FlowSceneWire.FlowNodeTraceStateV1;

export type FlowPseudoflowForestV1 = FlowSceneWire.FlowPseudoflowForestV1;

export type FlowBinaryBlockingOverlayV1 =
	FlowSceneWire.FlowBinaryBlockingOverlayV1;

export type FlowEibfsNodeStateV1 = FlowSceneWire.FlowEibfsNodeStateV1;

export type FlowEibfsForestArcV1 = FlowSceneWire.FlowEibfsForestArcV1;

export type FlowEibfsOverlayV1 = FlowSceneWire.FlowEibfsOverlayV1;

export type FlowDynamicEibfsOverlayV1 = FlowSceneWire.FlowDynamicEibfsOverlayV1;

export type FlowCancelTightenNodeStateV1 =
	FlowSceneWire.FlowCancelTightenNodeStateV1;

export type FlowCancelTightenOverlayV1 =
	FlowSceneWire.FlowCancelTightenOverlayV1;

export type FlowRelaxedMndcNodeStateV1 =
	FlowSceneWire.FlowRelaxedMndcNodeStateV1;

export type FlowRelaxedMndcCycleV1 = FlowSceneWire.FlowRelaxedMndcCycleV1;

export type FlowRelaxedMndcOverlayV1 = FlowSceneWire.FlowRelaxedMndcOverlayV1;

export type FlowEnhancedCapacityScalingComponentV1 =
	FlowSceneWire.FlowEnhancedCapacityScalingComponentV1;

export type FlowEnhancedCapacityScalingNodeStateV1 =
	FlowSceneWire.FlowEnhancedCapacityScalingNodeStateV1;

export type FlowEnhancedCapacityScalingEdgeStateV1 =
	FlowSceneWire.FlowEnhancedCapacityScalingEdgeStateV1;

export type FlowEnhancedCapacityScalingOverlayV1 =
	FlowSceneWire.FlowEnhancedCapacityScalingOverlayV1;

export type FlowOrlinMcfArcRefV1 = FlowSceneWire.FlowOrlinMcfArcRefV1;

export type FlowOrlinMcfComponentV1 = FlowSceneWire.FlowOrlinMcfComponentV1;

export type FlowOrlinMcfNodeStateV1 = FlowSceneWire.FlowOrlinMcfNodeStateV1;

export type FlowOrlinMcfArcStateV1 = FlowSceneWire.FlowOrlinMcfArcStateV1;

export type FlowOrlinMcfOverlayV1 = FlowSceneWire.FlowOrlinMcfOverlayV1;

export type FlowOrlinMaxFlowNodeStateV1 =
	FlowSceneWire.FlowOrlinMaxFlowNodeStateV1;

export type FlowOrlinMaxFlowResidualArcStateV1 =
	FlowSceneWire.FlowOrlinMaxFlowResidualArcStateV1;

export type FlowOrlinMaxFlowCompactArcRefV1 =
	FlowSceneWire.FlowOrlinMaxFlowCompactArcRefV1;

export type FlowOrlinMaxFlowCompactArcStateV1 =
	FlowSceneWire.FlowOrlinMaxFlowCompactArcStateV1;

export type FlowOrlinMaxFlowOverlayV1 = FlowSceneWire.FlowOrlinMaxFlowOverlayV1;

export type FlowDualNetworkSimplexNodeStateV1 =
	FlowSceneWire.FlowDualNetworkSimplexNodeStateV1;

export type FlowDualNetworkSimplexEdgeStateV1 =
	FlowSceneWire.FlowDualNetworkSimplexEdgeStateV1;

export type FlowDualNetworkSimplexOverlayV1 =
	FlowSceneWire.FlowDualNetworkSimplexOverlayV1;

export type FlowPolynomialDualNodeStateV1 =
	FlowSceneWire.FlowPolynomialDualNodeStateV1;

export type FlowPolynomialDualEdgeStateV1 =
	FlowSceneWire.FlowPolynomialDualEdgeStateV1;

export type FlowPolynomialDualSimplexOverlayV1 =
	FlowSceneWire.FlowPolynomialDualSimplexOverlayV1;

export type FlowPolynomialPrimalResidualRefV1 =
	FlowSceneWire.FlowPolynomialPrimalResidualRefV1;

export type FlowPolynomialPrimalNodeStateV1 =
	FlowSceneWire.FlowPolynomialPrimalNodeStateV1;

export type FlowPolynomialPrimalEdgeStateV1 =
	FlowSceneWire.FlowPolynomialPrimalEdgeStateV1;

export type FlowPolynomialPrimalArtificialEdgeStateV1 =
	FlowSceneWire.FlowPolynomialPrimalArtificialEdgeStateV1;

export type FlowPolynomialPrimalSimplexOverlayV1 =
	FlowSceneWire.FlowPolynomialPrimalSimplexOverlayV1;

export type FlowDoubleScalingNodeStateV1 =
	FlowSceneWire.FlowDoubleScalingNodeStateV1;

export type FlowDoubleScalingArcRefV1 = FlowSceneWire.FlowDoubleScalingArcRefV1;

export type FlowDoubleScalingEdgeStateV1 =
	FlowSceneWire.FlowDoubleScalingEdgeStateV1;

export type FlowDoubleScalingOverlayV1 =
	FlowSceneWire.FlowDoubleScalingOverlayV1;

export type FlowConvexCostSegmentStateV1 =
	FlowSceneWire.FlowConvexCostSegmentStateV1;

export type FlowConvexCostEdgeStateV1 = FlowSceneWire.FlowConvexCostEdgeStateV1;

export type FlowConvexCostOverlayV1 = FlowSceneWire.FlowConvexCostOverlayV1;

export type FlowConvexNetworkSimplexArcRefV1 =
	FlowSceneWire.FlowConvexNetworkSimplexArcRefV1;

export type FlowConvexNetworkSimplexNodeStateV1 =
	FlowSceneWire.FlowConvexNetworkSimplexNodeStateV1;

export type FlowConvexNetworkSimplexEdgeStateV1 =
	FlowSceneWire.FlowConvexNetworkSimplexEdgeStateV1;

export type FlowConvexNetworkSimplexArtificialEdgeV1 =
	FlowSceneWire.FlowConvexNetworkSimplexArtificialEdgeV1;

export type FlowConvexNetworkSimplexOverlayV1 =
	FlowSceneWire.FlowConvexNetworkSimplexOverlayV1;

export type FlowPredictionAssistedEpsilonNodeStateV1 =
	FlowSceneWire.FlowPredictionAssistedEpsilonNodeStateV1;

export type FlowPredictionAssistedEpsilonEdgeStateV1 =
	FlowSceneWire.FlowPredictionAssistedEpsilonEdgeStateV1;

export type FlowPredictionAssistedEpsilonOverlayV1 =
	FlowSceneWire.FlowPredictionAssistedEpsilonOverlayV1;

export type FlowTardosFixedVariableV1 = FlowSceneWire.FlowTardosFixedVariableV1;

export type FlowTardosFrameworkOverlayV1 =
	FlowSceneWire.FlowTardosFrameworkOverlayV1;

export type FlowElectricalNodeStateV1 = FlowSceneWire.FlowElectricalNodeStateV1;

export type FlowElectricalEdgeStateV1 = FlowSceneWire.FlowElectricalEdgeStateV1;

export type FlowElectricalFlowOverlayV1 =
	FlowSceneWire.FlowElectricalFlowOverlayV1;

export type FlowAugmentingElectricalNodeStateV1 =
	FlowSceneWire.FlowAugmentingElectricalNodeStateV1;

export type FlowAugmentingElectricalEdgeStateV1 =
	FlowSceneWire.FlowAugmentingElectricalEdgeStateV1;

export type FlowAugmentingElectricalOverlayV1 =
	FlowSceneWire.FlowAugmentingElectricalOverlayV1;

export type FlowInteriorPointNodeStateV1 =
	FlowSceneWire.FlowInteriorPointNodeStateV1;

export type FlowInteriorPointEdgeStateV1 =
	FlowSceneWire.FlowInteriorPointEdgeStateV1;

export type FlowInteriorPointMaxFlowOverlayV1 =
	FlowSceneWire.FlowInteriorPointMaxFlowOverlayV1;

export type FlowMinimumRatioCycleNodeStateV1 =
	FlowSceneWire.FlowMinimumRatioCycleNodeStateV1;

export type FlowMinimumRatioCycleEdgeStateV1 =
	FlowSceneWire.FlowMinimumRatioCycleEdgeStateV1;

export type FlowMinimumRatioCycleOverlayV1 =
	FlowSceneWire.FlowMinimumRatioCycleOverlayV1;

export type FlowMinimumRatioCycleMcfNodeStateV1 =
	FlowSceneWire.FlowMinimumRatioCycleMcfNodeStateV1;

export type FlowMinimumRatioCycleMcfEdgeStateV1 =
	FlowSceneWire.FlowMinimumRatioCycleMcfEdgeStateV1;

export type FlowMinimumRatioCycleMcfOverlayV1 =
	FlowSceneWire.FlowMinimumRatioCycleMcfOverlayV1;

export type FlowRandomizedAlmostLinearMcfNodeStateV1 =
	FlowSceneWire.FlowRandomizedAlmostLinearMcfNodeStateV1;

export type FlowRandomizedAlmostLinearMcfEdgeStateV1 =
	FlowSceneWire.FlowRandomizedAlmostLinearMcfEdgeStateV1;

export type FlowRandomizedAlmostLinearMcfOverlayV1 =
	FlowSceneWire.FlowRandomizedAlmostLinearMcfOverlayV1;

export type FlowFrameworkMcfEdgeStateV1 =
	FlowSceneWire.FlowFrameworkMcfEdgeStateV1;

export type FlowFrameworkMcfLevelStateV1 =
	FlowSceneWire.FlowFrameworkMcfLevelStateV1;

export type FlowFrameworkMcfFinalPointNodeV1 =
	FlowSceneWire.FlowFrameworkMcfFinalPointNodeV1;

export type FlowFrameworkMcfFinalPointEdgeV1 =
	FlowSceneWire.FlowFrameworkMcfFinalPointEdgeV1;

export type FlowFrameworkMcfOverlayV1 = FlowSceneWire.FlowFrameworkMcfOverlayV1;

export type FlowFrameworkMcfDynamicOperationV1 =
	FlowSceneWire.FlowFrameworkMcfDynamicOperationV1;

export type FlowWeightedAugmentingNodeStateV1 =
	FlowSceneWire.FlowWeightedAugmentingNodeStateV1;

export type FlowWeightedAugmentingEdgeStateV1 =
	FlowSceneWire.FlowWeightedAugmentingEdgeStateV1;

export type FlowWeightedAugmentingResidualArcStateV1 =
	FlowSceneWire.FlowWeightedAugmentingResidualArcStateV1;

export type FlowWeightedAugmentingPathsOverlayV1 =
	FlowSceneWire.FlowWeightedAugmentingPathsOverlayV1;

export type FlowWeightedPushRelabelShortcutNodeStateV1 =
	FlowSceneWire.FlowWeightedPushRelabelShortcutNodeStateV1;

export type FlowWeightedPushRelabelShortcutEdgeStateV1 =
	FlowSceneWire.FlowWeightedPushRelabelShortcutEdgeStateV1;

export type FlowWeightedPushRelabelShortcutResidualArcStateV1 =
	FlowSceneWire.FlowWeightedPushRelabelShortcutResidualArcStateV1;

export type FlowWeightedPushRelabelShortcutArcRefV1 =
	FlowSceneWire.FlowWeightedPushRelabelShortcutArcRefV1;

export type FlowWeightedPushRelabelShortcutOverlayV1 =
	FlowSceneWire.FlowWeightedPushRelabelShortcutOverlayV1;

export type FlowRandomizedAlmostLinearNodeStateV1 =
	FlowSceneWire.FlowRandomizedAlmostLinearNodeStateV1;

export type FlowRandomizedAlmostLinearEdgeStateV1 =
	FlowSceneWire.FlowRandomizedAlmostLinearEdgeStateV1;

export type FlowRandomizedAlmostLinearOverlayV1 =
	FlowSceneWire.FlowRandomizedAlmostLinearOverlayV1;

export type FlowDeterministicAlmostLinearNodeStateV1 =
	FlowSceneWire.FlowDeterministicAlmostLinearNodeStateV1;

export type FlowDeterministicAlmostLinearEdgeStateV1 =
	FlowSceneWire.FlowDeterministicAlmostLinearEdgeStateV1;

export type FlowDeterministicAlmostLinearOverlayV1 =
	FlowSceneWire.FlowDeterministicAlmostLinearOverlayV1;

export type FlowElectricalIpmMcfNodeStateV1 =
	FlowSceneWire.FlowElectricalIpmMcfNodeStateV1;

export type FlowElectricalIpmMcfEdgeStateV1 =
	FlowSceneWire.FlowElectricalIpmMcfEdgeStateV1;

export type FlowElectricalIpmMcfOverlayV1 =
	FlowSceneWire.FlowElectricalIpmMcfOverlayV1;

export type FlowPrimalDualIpmMcfNodeStateV1 =
	FlowSceneWire.FlowPrimalDualIpmMcfNodeStateV1;

export type FlowPrimalDualIpmMcfArcStateV1 =
	FlowSceneWire.FlowPrimalDualIpmMcfArcStateV1;

export type FlowPrimalDualIpmMcfOverlayV1 =
	FlowSceneWire.FlowPrimalDualIpmMcfOverlayV1;

export type FlowParametricSegmentV1 = FlowSceneWire.FlowParametricSegmentV1;

export type FlowParametricBreakpointV1 =
	FlowSceneWire.FlowParametricBreakpointV1;

export type FlowParametricTraversalV1 = FlowSceneWire.FlowParametricTraversalV1;

export type FlowParametricOverlayV1 = FlowSceneWire.FlowParametricOverlayV1;

export type FlowParametricMetricsV1 = FlowSceneWire.FlowParametricMetricsV1;

export type FlowFeasibilityNodeRefV1 = FlowSceneWire.FlowFeasibilityNodeRefV1;

export type FlowFeasibilityArcRefV1 = FlowSceneWire.FlowFeasibilityArcRefV1;

export type FlowFeasibilityResidualArcRefV1 =
	FlowSceneWire.FlowFeasibilityResidualArcRefV1;

export type FlowFeasibilityNodeStateV1 =
	FlowSceneWire.FlowFeasibilityNodeStateV1;

export type FlowFeasibilityArcStateV1 = FlowSceneWire.FlowFeasibilityArcStateV1;

export type FlowFeasibilityMetricsV1 = FlowSceneWire.FlowFeasibilityMetricsV1;

export type FlowFeasibilityWorkSummaryV1 =
	FlowSceneWire.FlowFeasibilityWorkSummaryV1;

export type FlowFeasibilityDomainEdgeV1 =
	FlowSceneWire.FlowFeasibilityDomainEdgeV1;
export type FlowFeasibilityDomainNodeV1 =
	FlowSceneWire.FlowFeasibilityDomainNodeV1;
export type FlowFeasibilityDomainV1 = FlowSceneWire.FlowFeasibilityDomainV1;
export type FlowFeasibilityOverlayV2 = FlowSceneWire.FlowFeasibilityOverlayV2;
export type FlowFeasibilityRequestV1 = FlowSceneWire.FlowFeasibilityRequestV1;

export type FlowTraceEntityRefV1 = FlowSceneWire.FlowTraceEntityRefSceneV1;

export type FlowTraceEventV1 = FlowSceneWire.FlowTraceEventSceneV1;

export type FlowTraceEventSemanticsV1 = FlowSceneWire.FlowTraceEventSemanticsV1;

export type FlowOutcomeV1 = FlowSceneWire.FlowOutcomeV1;

export type FlowCurrentSceneV9 = FlowSceneWire.FlowCurrentSceneV9;

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(
	value: Record<string, unknown>,
	required: readonly string[],
	optional: readonly string[] = [],
): boolean {
	const allowed = new Set([...required, ...optional]);
	return (
		required.every((key) => Object.hasOwn(value, key)) &&
		Object.keys(value).every((key) => allowed.has(key))
	);
}

const canonicalU64 = /^(0|[1-9][0-9]*)$/;
const canonicalI64 = /^(0|[1-9][0-9]*|-[1-9][0-9]*)$/;
const canonicalI128 = canonicalI64;
const canonicalFiniteDecimal =
	/^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:e-?[1-9][0-9]*)?$/;

function decodeFiniteDecimal(value: unknown): string {
	if (
		typeof value !== "string" ||
		!canonicalFiniteDecimal.test(value) ||
		value === "-0" ||
		!Number.isFinite(Number(value))
	) {
		throw new Error("Flow scene contains an invalid finite decimal");
	}
	return value;
}

function decodeRationalWithDigitLimit(
	value: unknown,
	maximumDigits: number,
): FlowRationalV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["numerator", "denominator"]) ||
		typeof value.numerator !== "string" ||
		!canonicalI64.test(value.numerator) ||
		value.numerator.replace("-", "").length > maximumDigits ||
		typeof value.denominator !== "string" ||
		!canonicalU64.test(value.denominator) ||
		value.denominator === "0" ||
		value.denominator.length > maximumDigits
	) {
		throw new Error("Flow scene contains an invalid exact rational");
	}
	let left = BigInt(value.numerator);
	let right = BigInt(value.denominator);
	left = left < 0n ? -left : left;
	while (right !== 0n) {
		const remainder = left % right;
		left = right;
		right = remainder;
	}
	if (left !== 1n) {
		throw new Error("Flow scene rational is not normalized");
	}
	return { numerator: value.numerator, denominator: value.denominator };
}

function decodeRational(value: unknown): FlowRationalV1 {
	return decodeRationalWithDigitLimit(value, 128);
}

const FLOW_FRAMEWORK_MAX_RATIONAL_DECIMAL_DIGITS = 1234;

function decodeFlowFrameworkRational(value: unknown): FlowRationalV1 {
	return decodeRationalWithDigitLimit(
		value,
		FLOW_FRAMEWORK_MAX_RATIONAL_DECIMAL_DIGITS,
	);
}

function compareRational(left: FlowRationalV1, right: FlowRationalV1): number {
	const difference =
		BigInt(left.numerator) * BigInt(right.denominator) -
		BigInt(right.numerator) * BigInt(left.denominator);
	return difference < 0n ? -1 : difference > 0n ? 1 : 0;
}

function decodeEdgeState(value: unknown): FlowEdgeStateV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["edge_id", "flow"]) ||
		typeof value.edge_id !== "string" ||
		typeof value.flow !== "string" ||
		!canonicalU64.test(value.flow)
	) {
		throw new Error("Flow scene contains an invalid edge state");
	}
	return { edge_id: value.edge_id, flow: value.flow };
}

function decodeResidualArc(value: unknown): FlowResidualArcStateV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			["edge_id", "direction", "from", "to", "capacity", "cost", "active"],
			["fixed"],
		) ||
		typeof value.edge_id !== "string" ||
		(value.direction !== "forward" && value.direction !== "reverse") ||
		typeof value.from !== "string" ||
		typeof value.to !== "string" ||
		typeof value.capacity !== "string" ||
		!canonicalU64.test(value.capacity) ||
		typeof value.cost !== "string" ||
		!canonicalI128.test(value.cost) ||
		typeof value.active !== "boolean" ||
		(value.fixed !== undefined && typeof value.fixed !== "boolean")
	) {
		throw new Error("Flow scene contains an invalid residual arc");
	}
	return {
		edge_id: value.edge_id,
		direction: value.direction,
		from: value.from,
		to: value.to,
		capacity: value.capacity,
		cost: value.cost,
		active: value.active,
		fixed: value.fixed ?? false,
	};
}

function decodeNodeTraceState(value: unknown): FlowNodeTraceStateV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			["node_id"],
			["label", "search_ordinal", "remaining_divergence"],
		) ||
		typeof value.node_id !== "string" ||
		(value.label !== undefined &&
			(typeof value.label !== "string" || !canonicalI128.test(value.label))) ||
		(value.search_ordinal !== undefined &&
			(typeof value.search_ordinal !== "number" ||
				!Number.isInteger(value.search_ordinal) ||
				value.search_ordinal < 0 ||
				value.search_ordinal > 0xffff_ffff)) ||
		(value.remaining_divergence !== undefined &&
			(typeof value.remaining_divergence !== "string" ||
				!canonicalI128.test(value.remaining_divergence)))
	) {
		throw new Error("Flow scene contains an invalid node trace state");
	}
	return {
		node_id: value.node_id,
		...(value.label === undefined ? {} : { label: value.label as string }),
		...(value.search_ordinal === undefined
			? {}
			: { search_ordinal: value.search_ordinal as number }),
		...(value.remaining_divergence === undefined
			? {}
			: { remaining_divergence: value.remaining_divergence as string }),
	};
}

function decodePseudoflowForest(
	value: unknown,
): FlowPseudoflowForestV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["arcs", "strong_nodes"]) ||
		!Array.isArray(value.arcs) ||
		!Array.isArray(value.strong_nodes)
	) {
		throw new Error("Flow scene contains an invalid pseudoflow forest");
	}
	const arcs = value.arcs.map((arc) => {
		if (
			!isRecord(arc) ||
			!hasExactKeys(arc, ["edge_id", "direction"]) ||
			typeof arc.edge_id !== "string" ||
			(arc.direction !== "forward" && arc.direction !== "reverse")
		) {
			throw new Error("Flow scene contains an invalid pseudoflow forest arc");
		}
		return {
			edge_id: arc.edge_id,
			direction: arc.direction as "forward" | "reverse",
		};
	});
	if (!value.strong_nodes.every((node) => typeof node === "string")) {
		throw new Error("Flow scene contains invalid pseudoflow strong nodes");
	}
	return { arcs, strong_nodes: value.strong_nodes as string[] };
}

const FLOW_FEASIBILITY_STAGES = [
	"ready",
	"add-original-arc",
	"add-return-arc",
	"inspect-node-imbalance",
	"add-imbalance-arc",
	"initialize-source-height",
	"inspect-source-arc",
	"activate-node",
	"select-active-node",
	"inspect-discharge-arc",
	"inspect-relabel-arc",
	"push",
	"advance-current-arc",
	"relabel",
	"complete-discharge",
	"complete-routing",
	"inspect-cut-arc",
	"mark-reachable",
	"extract-original-flow",
	"feasible",
	"infeasible",
] as const satisfies readonly FlowFeasibilityOverlayV2["stage"][];

const FLOW_FEASIBILITY_USES = [
	"initial-flow",
	"precheck-only",
	"anchored-recovery",
] as const satisfies readonly FlowFeasibilityOverlayV2["use_kind"][];

const FLOW_FEASIBILITY_DOMAIN_KINDS = [
	"public-input",
	"node-aligned-transformation",
	"standalone-transformation",
] as const satisfies readonly FlowFeasibilityDomainV1["kind"][];

function decodeFeasibilityDomainNode(
	value: unknown,
): FlowFeasibilityDomainNodeV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["node_id"], ["public_node_id"]) ||
		typeof value.node_id !== "string" ||
		value.node_id.length === 0 ||
		(value.public_node_id !== undefined &&
			(typeof value.public_node_id !== "string" ||
				value.public_node_id.length === 0))
	) {
		throw new Error("Feasibility domain contains an invalid node declaration");
	}
	return {
		node_id: value.node_id,
		...(value.public_node_id === undefined
			? {}
			: { public_node_id: value.public_node_id }),
	};
}

function decodeFeasibilityDomainEdge(
	value: unknown,
	nodeIds: ReadonlySet<string>,
): FlowFeasibilityDomainEdgeV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			["edge_id", "from_node_id", "to_node_id", "lower", "capacity"],
			["public_route_edge_id"],
		) ||
		![value.edge_id, value.from_node_id, value.to_node_id].every(
			(field) => typeof field === "string" && field.length > 0,
		) ||
		!nodeIds.has(value.from_node_id as string) ||
		!nodeIds.has(value.to_node_id as string) ||
		typeof value.lower !== "string" ||
		!canonicalU64.test(value.lower) ||
		typeof value.capacity !== "string" ||
		!canonicalU64.test(value.capacity) ||
		BigInt(value.lower) > BigInt(value.capacity) ||
		(value.public_route_edge_id !== undefined &&
			(typeof value.public_route_edge_id !== "string" ||
				value.public_route_edge_id.length === 0))
	) {
		throw new Error("Feasibility domain contains an invalid edge declaration");
	}
	return {
		edge_id: value.edge_id as string,
		from_node_id: value.from_node_id as string,
		to_node_id: value.to_node_id as string,
		lower: value.lower,
		capacity: value.capacity,
		...(value.public_route_edge_id === undefined
			? {}
			: { public_route_edge_id: value.public_route_edge_id }),
	};
}

function decodeFeasibilityRequest(
	value: unknown,
	nodes: readonly FlowFeasibilityDomainNodeV1[],
): FlowFeasibilityRequestV1 {
	const nodeIds = new Set(nodes.map((node) => node.node_id));
	if (!isRecord(value) || typeof value.kind !== "string") {
		throw new Error("Feasibility domain contains an invalid request");
	}
	if (value.kind === "balance") {
		if (
			!hasExactKeys(value, ["kind", "required_divergences"]) ||
			!Array.isArray(value.required_divergences)
		) {
			throw new Error("Feasibility balance request is malformed");
		}
		const requiredDivergences = value.required_divergences.map((item) => {
			if (
				!isRecord(item) ||
				!hasExactKeys(item, ["node_id", "required_divergence"]) ||
				typeof item.node_id !== "string" ||
				typeof item.required_divergence !== "string" ||
				!canonicalI128.test(item.required_divergence)
			) {
				throw new Error(
					"Feasibility balance request contains an invalid target",
				);
			}
			return {
				node_id: item.node_id,
				required_divergence: item.required_divergence,
			};
		});
		if (
			requiredDivergences.length !== nodes.length ||
			requiredDivergences.some(
				(item, index) => item.node_id !== nodes[index]?.node_id,
			) ||
			requiredDivergences.reduce(
				(sum, item) => sum + BigInt(item.required_divergence),
				0n,
			) !== 0n
		) {
			throw new Error(
				"Feasibility balance request does not match its canonical domain",
			);
		}
		return { kind: "balance", required_divergences: requiredDivergences };
	}
	if (
		value.kind !== "max-flow-initial" ||
		!hasExactKeys(value, ["kind", "source_node_id", "sink_node_id"]) ||
		typeof value.source_node_id !== "string" ||
		typeof value.sink_node_id !== "string" ||
		value.source_node_id === value.sink_node_id ||
		!nodeIds.has(value.source_node_id) ||
		!nodeIds.has(value.sink_node_id)
	) {
		throw new Error("Feasibility maximum-flow request is malformed");
	}
	return {
		kind: "max-flow-initial",
		source_node_id: value.source_node_id,
		sink_node_id: value.sink_node_id,
	};
}

function decodeFeasibilityDomain(value: unknown): FlowFeasibilityDomainV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["kind", "nodes", "edges", "request"]) ||
		!FLOW_FEASIBILITY_DOMAIN_KINDS.includes(
			value.kind as FlowFeasibilityDomainV1["kind"],
		) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges)
	) {
		throw new Error("Flow scene contains an invalid feasibility domain");
	}
	const nodes = value.nodes.map(decodeFeasibilityDomainNode);
	const nodeIds = nodes.map((node) => node.node_id);
	if (
		new Set(nodeIds).size !== nodes.length ||
		canonicalStableIds(nodes.map((node) => ({ id: node.node_id }))).some(
			(nodeId, index) => nodeId !== nodeIds[index],
		)
	) {
		throw new Error("Feasibility domain nodes are not canonical and unique");
	}
	const nodeIdSet = new Set(nodeIds);
	const edges = value.edges.map((edge) =>
		decodeFeasibilityDomainEdge(edge, nodeIdSet),
	);
	const edgeIds = edges.map((edge) => edge.edge_id);
	if (
		new Set(edgeIds).size !== edges.length ||
		canonicalStableIds(edges.map((edge) => ({ id: edge.edge_id }))).some(
			(edgeId, index) => edgeId !== edgeIds[index],
		)
	) {
		throw new Error("Feasibility domain edges are not canonical and unique");
	}
	return {
		kind: value.kind as FlowFeasibilityDomainV1["kind"],
		nodes,
		edges,
		request: decodeFeasibilityRequest(value.request, nodes),
	};
}

function decodeFeasibilityNodeRef(value: unknown): FlowFeasibilityNodeRefV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["kind"], ["original_node_id"]) ||
		!(
			value.kind === "original" ||
			value.kind === "super-source" ||
			value.kind === "super-sink"
		)
	) {
		throw new Error("Feasibility overlay contains an invalid node reference");
	}
	if (value.kind === "original") {
		if (
			typeof value.original_node_id !== "string" ||
			value.original_node_id.length === 0
		) {
			throw new Error(
				"Feasibility original-node reference has no stable identity",
			);
		}
		return { kind: "original", original_node_id: value.original_node_id };
	}
	if (value.original_node_id !== undefined) {
		throw new Error(
			"Feasibility artificial-node reference aliases an original identity",
		);
	}
	return { kind: value.kind };
}

function feasibilityNodeKey(node: FlowFeasibilityNodeRefV1): string {
	return node.kind === "original"
		? `original\u0000${node.original_node_id}`
		: node.kind;
}

function sameFeasibilityNode(
	left: FlowFeasibilityNodeRefV1,
	right: FlowFeasibilityNodeRefV1,
): boolean {
	return feasibilityNodeKey(left) === feasibilityNodeKey(right);
}

function decodeFeasibilityArcRef(value: unknown): FlowFeasibilityArcRefV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			["kind"],
			["original_edge_id", "imbalance_node_id", "return_from", "return_to"],
		) ||
		!(
			value.kind === "original" ||
			value.kind === "lower-bound-return" ||
			value.kind === "from-super-source" ||
			value.kind === "to-super-sink"
		)
	) {
		throw new Error("Feasibility overlay contains an invalid arc reference");
	}
	const fields = [
		value.original_edge_id,
		value.imbalance_node_id,
		value.return_from,
		value.return_to,
	];
	if (
		fields.some(
			(field) =>
				field !== undefined &&
				(typeof field !== "string" || field.length === 0),
		)
	) {
		throw new Error("Feasibility arc identity contains an invalid ID");
	}
	const only = (...present: unknown[]) =>
		fields.filter((field) => field !== undefined).length === present.length &&
		present.every((field) => field !== undefined);
	if (value.kind === "original" && only(value.original_edge_id)) {
		return {
			kind: "original",
			original_edge_id: value.original_edge_id as string,
		};
	}
	if (
		value.kind === "lower-bound-return" &&
		only(value.return_from, value.return_to)
	) {
		return {
			kind: "lower-bound-return",
			return_from: value.return_from as string,
			return_to: value.return_to as string,
		};
	}
	if (
		(value.kind === "from-super-source" || value.kind === "to-super-sink") &&
		only(value.imbalance_node_id)
	) {
		return {
			kind: value.kind,
			imbalance_node_id: value.imbalance_node_id as string,
		};
	}
	throw new Error(
		"Feasibility arc identity does not match its structural role",
	);
}

function feasibilityArcKey(arc: FlowFeasibilityArcRefV1): string {
	switch (arc.kind) {
		case "original":
			return `original\u0000${arc.original_edge_id}`;
		case "lower-bound-return":
			return `return\u0000${arc.return_from}\u0000${arc.return_to}`;
		case "from-super-source":
			return `super-source\u0000${arc.imbalance_node_id}`;
		case "to-super-sink":
			return `super-sink\u0000${arc.imbalance_node_id}`;
	}
}

const FLOW_FEASIBILITY_METRIC_NAMES = [
	"original_edge_inspections",
	"original_node_inspections",
	"auxiliary_adjacency_inspections",
	"pushes",
	"relabels",
	"active_node_selections",
	"discharges",
	"cut_adjacency_inspections",
	"extracted_original_edges",
] as const satisfies readonly (keyof FlowFeasibilityMetricsV1)[];

function decodeFeasibilityMetrics(value: unknown): FlowFeasibilityMetricsV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, FLOW_FEASIBILITY_METRIC_NAMES) ||
		!FLOW_FEASIBILITY_METRIC_NAMES.every(
			(name) =>
				typeof value[name] === "string" &&
				canonicalU64.test(value[name] as string),
		)
	) {
		throw new Error("Feasibility work contains a noncanonical counter");
	}
	return value as FlowFeasibilityMetricsV1;
}

function decodeFeasibilityWorkSummary(
	value: unknown,
): FlowFeasibilityWorkSummaryV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["invocations", "metrics"]) ||
		typeof value.invocations !== "string" ||
		!canonicalU64.test(value.invocations) ||
		BigInt(value.invocations) === 0n
	) {
		throw new Error("Flow scene contains an invalid feasibility-work summary");
	}
	const metrics = decodeFeasibilityMetrics(value.metrics);
	if (
		!FLOW_FEASIBILITY_METRIC_NAMES.some((name) => BigInt(metrics[name]) > 0n)
	) {
		throw new Error("Feasibility invocation reports no source work");
	}
	return { invocations: value.invocations, metrics };
}

function decodeFeasibilityResidualArcRef(
	value: unknown,
): FlowFeasibilityResidualArcRefV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["arc", "direction"]) ||
		(value.direction !== "forward" && value.direction !== "reverse")
	) {
		throw new Error(
			"Feasibility overlay contains an invalid residual-arc reference",
		);
	}
	return {
		arc: decodeFeasibilityArcRef(value.arc),
		direction: value.direction,
	};
}

function decodeFeasibilityOverlay(
	value: unknown,
): FlowFeasibilityOverlayV2 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"revision",
				"use_kind",
				"domain",
				"stage",
				"nodes",
				"arcs",
				"active_queue",
				"total_required",
				"routed",
				"metrics",
			],
			["focus_node", "focus_arc"],
		) ||
		value.revision !== "flow-feasibility-overlay/2" ||
		!FLOW_FEASIBILITY_USES.includes(
			value.use_kind as FlowFeasibilityOverlayV2["use_kind"],
		) ||
		!FLOW_FEASIBILITY_STAGES.includes(
			value.stage as FlowFeasibilityOverlayV2["stage"],
		) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.arcs) ||
		!Array.isArray(value.active_queue) ||
		typeof value.total_required !== "string" ||
		!canonicalU64.test(value.total_required) ||
		typeof value.routed !== "string" ||
		!canonicalU64.test(value.routed) ||
		!isRecord(value.metrics)
	) {
		throw new Error("Flow scene contains an invalid feasibility overlay");
	}
	const domain = decodeFeasibilityDomain(value.domain);
	const metrics = decodeFeasibilityMetrics(value.metrics);
	const nodes = value.nodes.map((item): FlowFeasibilityNodeStateV1 => {
		if (
			!isRecord(item) ||
			!hasExactKeys(
				item,
				["node", "height", "excess", "current_arc", "active", "reachable"],
				["queue_position"],
			) ||
			![item.height, item.excess, item.current_arc].every(
				(field) => typeof field === "string" && canonicalU64.test(field),
			) ||
			typeof item.active !== "boolean" ||
			typeof item.reachable !== "boolean" ||
			(item.queue_position !== undefined &&
				(typeof item.queue_position !== "string" ||
					!canonicalU64.test(item.queue_position)))
		) {
			throw new Error("Feasibility overlay contains an invalid node state");
		}
		return {
			node: decodeFeasibilityNodeRef(item.node),
			height: item.height as string,
			excess: item.excess as string,
			current_arc: item.current_arc as string,
			active: item.active,
			reachable: item.reachable,
			...(item.queue_position === undefined
				? {}
				: { queue_position: item.queue_position as string }),
		};
	});
	const nodeByKey = new Map(
		nodes.map((node) => [feasibilityNodeKey(node.node), node]),
	);
	if (
		nodeByKey.size !== nodes.length ||
		nodes.filter((node) => node.node.kind === "super-source").length !== 1 ||
		nodes.filter((node) => node.node.kind === "super-sink").length !== 1
	) {
		throw new Error(
			"Feasibility overlay does not identify each node exactly once",
		);
	}
	const arcs = value.arcs.map((item): FlowFeasibilityArcStateV1 => {
		if (
			!isRecord(item) ||
			!hasExactKeys(
				item,
				[
					"arc",
					"from",
					"to",
					"capacity",
					"flow",
					"forward_residual",
					"reverse_residual",
					"focused",
				],
				["focused_direction"],
			) ||
			![
				item.capacity,
				item.flow,
				item.forward_residual,
				item.reverse_residual,
			].every(
				(field) => typeof field === "string" && canonicalU64.test(field),
			) ||
			typeof item.focused !== "boolean" ||
			(item.focused_direction !== undefined &&
				item.focused_direction !== "forward" &&
				item.focused_direction !== "reverse") ||
			item.focused !== (item.focused_direction !== undefined)
		) {
			throw new Error("Feasibility overlay contains an invalid arc state");
		}
		const arc = decodeFeasibilityArcRef(item.arc);
		const from = decodeFeasibilityNodeRef(item.from);
		const to = decodeFeasibilityNodeRef(item.to);
		const capacity = BigInt(item.capacity as string);
		const flow = BigInt(item.flow as string);
		if (
			!nodeByKey.has(feasibilityNodeKey(from)) ||
			!nodeByKey.has(feasibilityNodeKey(to)) ||
			flow > capacity ||
			BigInt(item.forward_residual as string) !== capacity - flow ||
			BigInt(item.reverse_residual as string) !== flow
		) {
			throw new Error("Feasibility arc state violates its residual invariant");
		}
		const endpointsMatch =
			(arc.kind === "original" &&
				from.kind === "original" &&
				to.kind === "original") ||
			(arc.kind === "lower-bound-return" &&
				from.kind === "original" &&
				to.kind === "original" &&
				arc.return_from === from.original_node_id &&
				arc.return_to === to.original_node_id) ||
			(arc.kind === "from-super-source" &&
				from.kind === "super-source" &&
				to.kind === "original" &&
				arc.imbalance_node_id === to.original_node_id) ||
			(arc.kind === "to-super-sink" &&
				from.kind === "original" &&
				to.kind === "super-sink" &&
				arc.imbalance_node_id === from.original_node_id);
		if (!endpointsMatch) {
			throw new Error("Feasibility arc endpoints do not match its typed role");
		}
		return {
			arc,
			from,
			to,
			capacity: item.capacity as string,
			flow: item.flow as string,
			forward_residual: item.forward_residual as string,
			reverse_residual: item.reverse_residual as string,
			focused: item.focused,
			...(item.focused_direction === undefined
				? {}
				: { focused_direction: item.focused_direction }),
		};
	});
	const arcByKey = new Map(
		arcs.map((arc) => [feasibilityArcKey(arc.arc), arc]),
	);
	if (arcByKey.size !== arcs.length) {
		throw new Error("Feasibility overlay repeats a logical arc");
	}
	const activeQueue = value.active_queue.map(decodeFeasibilityNodeRef);
	const queuedKeys = activeQueue.map(feasibilityNodeKey);
	if (
		new Set(queuedKeys).size !== queuedKeys.length ||
		queuedKeys.some((key) => !nodeByKey.has(key)) ||
		nodes.some((node) => {
			const position = queuedKeys.indexOf(feasibilityNodeKey(node.node));
			return (
				node.active !== position >= 0 ||
				(position < 0
					? node.queue_position !== undefined
					: node.queue_position !== `${position}`)
			);
		})
	) {
		throw new Error("Feasibility FIFO queue and node activity disagree");
	}
	const focusNode =
		value.focus_node === undefined
			? undefined
			: decodeFeasibilityNodeRef(value.focus_node);
	const focusArc =
		value.focus_arc === undefined
			? undefined
			: decodeFeasibilityResidualArcRef(value.focus_arc);
	if (
		(focusNode !== undefined &&
			!nodeByKey.has(feasibilityNodeKey(focusNode))) ||
		(focusArc !== undefined &&
			!arcByKey.has(feasibilityArcKey(focusArc.arc))) ||
		arcs.filter((arc) => arc.focused).length !==
			(focusArc === undefined ? 0 : 1) ||
		(focusArc !== undefined &&
			!arcs.some(
				(arc) =>
					feasibilityArcKey(arc.arc) === feasibilityArcKey(focusArc.arc) &&
					arc.focused_direction === focusArc.direction,
			))
	) {
		throw new Error("Feasibility focus does not identify one local entity");
	}
	const stage = value.stage as FlowFeasibilityOverlayV2["stage"];
	const focusShapeIsValid = (() => {
		switch (stage) {
			case "add-original-arc":
			case "add-return-arc":
			case "add-imbalance-arc":
			case "push":
			case "extract-original-flow":
				return focusNode === undefined && focusArc !== undefined;
			case "inspect-node-imbalance":
			case "activate-node":
			case "select-active-node":
			case "relabel":
			case "complete-discharge":
				return focusNode !== undefined && focusArc === undefined;
			case "initialize-source-height":
				return focusNode?.kind === "super-source" && focusArc === undefined;
			case "inspect-source-arc":
				return focusNode?.kind === "super-source" && focusArc !== undefined;
			case "inspect-discharge-arc":
			case "inspect-relabel-arc":
			case "advance-current-arc":
			case "inspect-cut-arc":
				return focusNode !== undefined && focusArc !== undefined;
			case "mark-reachable":
				return focusNode !== undefined;
			case "ready":
			case "complete-routing":
			case "feasible":
			case "infeasible":
				return focusNode === undefined && focusArc === undefined;
		}
	})();
	if (
		!focusShapeIsValid ||
		BigInt(value.routed) > BigInt(value.total_required)
	) {
		throw new Error("Feasibility stage and local focus disagree");
	}
	return {
		revision: "flow-feasibility-overlay/2",
		use_kind: value.use_kind as FlowFeasibilityOverlayV2["use_kind"],
		domain,
		stage,
		nodes,
		arcs,
		active_queue: activeQueue,
		...(focusNode === undefined ? {} : { focus_node: focusNode }),
		...(focusArc === undefined ? {} : { focus_arc: focusArc }),
		total_required: value.total_required,
		routed: value.routed,
		metrics,
	};
}

function decodeBinaryBlockingOverlay(
	value: unknown,
): FlowBinaryBlockingOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"stage",
			"upper_bound",
			"delta",
			"delivered",
			"nodes",
			"base_zero_arcs",
			"special_arcs",
			"admissible_arcs",
			"zero_admissible_arcs",
		]) ||
		!(["analyzing", "analyzed", "contracted", "complete"] as const).includes(
			value.stage as FlowBinaryBlockingOverlayV1["stage"],
		) ||
		![value.upper_bound, value.delta, value.delivered].every(
			(item) => typeof item === "string" && canonicalU64.test(item),
		) ||
		value.upper_bound === "0" ||
		value.delta === "0" ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.base_zero_arcs) ||
		!Array.isArray(value.special_arcs) ||
		!Array.isArray(value.admissible_arcs) ||
		!Array.isArray(value.zero_admissible_arcs)
	) {
		throw new Error("Flow scene contains an invalid binary blocking overlay");
	}
	const nodes = value.nodes.map((node) => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, ["node_id", "component"], ["distance"]) ||
			typeof node.node_id !== "string" ||
			typeof node.component !== "string" ||
			!canonicalU64.test(node.component) ||
			(node.distance !== undefined &&
				(typeof node.distance !== "string" ||
					!canonicalU64.test(node.distance)))
		) {
			throw new Error("Flow scene contains an invalid binary blocking node");
		}
		return {
			node_id: node.node_id,
			component: node.component,
			...(node.distance === undefined
				? {}
				: { distance: node.distance as string }),
		};
	});
	const decodeArcs = (items: unknown[]) =>
		items.map((arc) => {
			if (
				!isRecord(arc) ||
				!hasExactKeys(arc, ["edge_id", "direction"]) ||
				typeof arc.edge_id !== "string" ||
				(arc.direction !== "forward" && arc.direction !== "reverse")
			) {
				throw new Error("Flow scene contains an invalid binary blocking arc");
			}
			return {
				edge_id: arc.edge_id,
				direction: arc.direction as "forward" | "reverse",
			};
		});
	return {
		stage: value.stage as FlowBinaryBlockingOverlayV1["stage"],
		upper_bound: value.upper_bound as string,
		delta: value.delta as string,
		delivered: value.delivered as string,
		nodes,
		base_zero_arcs: decodeArcs(value.base_zero_arcs),
		special_arcs: decodeArcs(value.special_arcs),
		admissible_arcs: decodeArcs(value.admissible_arcs),
		zero_admissible_arcs: decodeArcs(value.zero_admissible_arcs),
	};
}

function decodeCancelTightenOverlay(
	value: unknown,
): FlowCancelTightenOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"epsilon",
				"phase",
				"nodes",
				"admissible_arcs",
				"active_cycle",
				"inspected_arcs",
			],
			["delta"],
		) ||
		!(
			[
				"ready",
				"initialize",
				"begin-phase",
				"inspect-cycle-arc",
				"select-cycle",
				"cancel-cycle",
				"inspect-rank-arc",
				"tighten",
				"optimal",
			] as const
		).includes(value.stage as FlowCancelTightenOverlayV1["stage"]) ||
		typeof value.phase !== "string" ||
		!canonicalU64.test(value.phase) ||
		(value.delta !== undefined &&
			(typeof value.delta !== "string" ||
				!canonicalU64.test(value.delta) ||
				value.delta === "0")) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.admissible_arcs) ||
		!Array.isArray(value.active_cycle) ||
		!Array.isArray(value.inspected_arcs)
	) {
		throw new Error(
			"Flow scene contains an invalid Cancel-and-Tighten overlay",
		);
	}
	const nodes = value.nodes.map((node) => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, ["node_id", "potential"], ["rank"]) ||
			typeof node.node_id !== "string" ||
			(node.rank !== undefined &&
				(typeof node.rank !== "string" || !canonicalU64.test(node.rank)))
		) {
			throw new Error("Flow scene contains an invalid Cancel-and-Tighten node");
		}
		return {
			node_id: node.node_id,
			potential: decodeRational(node.potential),
			...(node.rank === undefined ? {} : { rank: node.rank as string }),
		};
	});
	const decodeArc = (arc: unknown): FlowResidualArcRefV1 => {
		if (
			!isRecord(arc) ||
			!hasExactKeys(arc, ["edge_id", "direction"]) ||
			typeof arc.edge_id !== "string" ||
			(arc.direction !== "forward" && arc.direction !== "reverse")
		) {
			throw new Error(
				"Flow scene contains an invalid Cancel-and-Tighten residual arc",
			);
		}
		return { edge_id: arc.edge_id, direction: arc.direction };
	};
	const epsilon = decodeRational(value.epsilon);
	if (BigInt(epsilon.numerator) < 0n) {
		throw new Error("Cancel-and-Tighten epsilon cannot be negative");
	}
	return {
		stage: value.stage as FlowCancelTightenOverlayV1["stage"],
		epsilon,
		phase: value.phase,
		nodes,
		admissible_arcs: value.admissible_arcs.map(decodeArc),
		active_cycle: value.active_cycle.map(decodeArc),
		inspected_arcs: value.inspected_arcs.map(decodeArc),
		...(value.delta === undefined ? {} : { delta: value.delta as string }),
	};
}

function decodeRelaxedMndcOverlay(
	value: unknown,
): FlowRelaxedMndcOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			["stage", "epsilon", "phase", "nodes", "family", "inspected_arcs"],
			["assignment_value", "active_assignment_cell"],
		) ||
		!(
			[
				"ready",
				"initialize",
				"begin-phase",
				"inspect-residual-arc",
				"inspect-assignment-cell",
				"select-family",
				"cancel-family",
				"phase-optimal",
				"optimal",
			] as const
		).includes(value.stage as FlowRelaxedMndcOverlayV1["stage"]) ||
		typeof value.phase !== "string" ||
		!canonicalU64.test(value.phase) ||
		(value.assignment_value !== undefined &&
			(typeof value.assignment_value !== "string" ||
				!canonicalI128.test(value.assignment_value))) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.family) ||
		!Array.isArray(value.inspected_arcs)
	) {
		throw new Error("Flow scene contains an invalid relaxed-MNDC overlay");
	}
	const decodeArc = (arc: unknown): FlowResidualArcRefV1 => {
		if (
			!isRecord(arc) ||
			!hasExactKeys(arc, ["edge_id", "direction"]) ||
			typeof arc.edge_id !== "string" ||
			(arc.direction !== "forward" && arc.direction !== "reverse")
		) {
			throw new Error(
				"Flow scene contains an invalid relaxed-MNDC residual arc",
			);
		}
		return { edge_id: arc.edge_id, direction: arc.direction };
	};
	const nodes = value.nodes.map((node) => {
		if (
			!isRecord(node) ||
			!hasExactKeys(
				node,
				["node_id", "matched_node_id", "left_dual", "right_dual"],
				["selected_arc"],
			) ||
			typeof node.node_id !== "string" ||
			typeof node.matched_node_id !== "string" ||
			typeof node.left_dual !== "string" ||
			!canonicalI128.test(node.left_dual) ||
			typeof node.right_dual !== "string" ||
			!canonicalI128.test(node.right_dual)
		) {
			throw new Error("Flow scene contains an invalid relaxed-MNDC node");
		}
		return {
			node_id: node.node_id,
			matched_node_id: node.matched_node_id,
			left_dual: node.left_dual,
			right_dual: node.right_dual,
			...(node.selected_arc === undefined
				? {}
				: { selected_arc: decodeArc(node.selected_arc) }),
		};
	});
	const family = value.family.map((cycle) => {
		if (
			!isRecord(cycle) ||
			!hasExactKeys(cycle, ["transformed_cost", "arcs"], ["delta"]) ||
			typeof cycle.transformed_cost !== "string" ||
			!canonicalI128.test(cycle.transformed_cost) ||
			!Array.isArray(cycle.arcs) ||
			(cycle.delta !== undefined &&
				(typeof cycle.delta !== "string" ||
					!canonicalU64.test(cycle.delta) ||
					cycle.delta === "0"))
		) {
			throw new Error("Flow scene contains an invalid relaxed-MNDC cycle");
		}
		return {
			transformed_cost: cycle.transformed_cost,
			arcs: cycle.arcs.map(decodeArc),
			...(cycle.delta === undefined ? {} : { delta: cycle.delta }),
		};
	});
	let activeAssignmentCell:
		| { row_node_id: string; column_node_id: string }
		| undefined;
	if (value.active_assignment_cell !== undefined) {
		const cell = value.active_assignment_cell;
		if (
			!isRecord(cell) ||
			!hasExactKeys(cell, ["row_node_id", "column_node_id"]) ||
			typeof cell.row_node_id !== "string" ||
			typeof cell.column_node_id !== "string"
		) {
			throw new Error(
				"Flow scene contains an invalid relaxed-MNDC assignment cell",
			);
		}
		activeAssignmentCell = {
			row_node_id: cell.row_node_id,
			column_node_id: cell.column_node_id,
		};
	}
	const epsilon = decodeRational(value.epsilon);
	if (BigInt(epsilon.numerator) < 0n) {
		throw new Error("Relaxed-MNDC epsilon cannot be negative");
	}
	return {
		stage: value.stage as FlowRelaxedMndcOverlayV1["stage"],
		epsilon,
		phase: value.phase,
		...(value.assignment_value === undefined
			? {}
			: { assignment_value: value.assignment_value as string }),
		nodes,
		family,
		inspected_arcs: value.inspected_arcs.map(decodeArc),
		...(activeAssignmentCell === undefined
			? {}
			: { active_assignment_cell: activeAssignmentCell }),
	};
}

function decodeEnhancedCapacityScalingOverlay(
	value: unknown,
): FlowEnhancedCapacityScalingOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			["stage", "delta", "phase", "components", "nodes", "edges", "path"],
			["source_component", "sink_component", "contraction_arc", "augmentation"],
		) ||
		!(
			[
				"ready",
				"initialize",
				"complete-regeneration",
				"begin-phase",
				"contract",
				"inspect-residual-arc",
				"select-path",
				"augment",
				"complete-phase",
				"halve-scale",
				"recover-primal",
				"optimal",
			] as const
		).includes(value.stage as FlowEnhancedCapacityScalingOverlayV1["stage"]) ||
		typeof value.phase !== "string" ||
		!canonicalU64.test(value.phase) ||
		!Array.isArray(value.components) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges) ||
		!Array.isArray(value.path) ||
		(value.source_component !== undefined &&
			typeof value.source_component !== "string") ||
		(value.sink_component !== undefined &&
			typeof value.sink_component !== "string") ||
		(value.contraction_arc !== undefined &&
			typeof value.contraction_arc !== "string")
	) {
		throw new Error("Flow scene contains an invalid enhanced-scaling overlay");
	}
	const decodeArc = (arc: unknown): FlowResidualArcRefV1 => {
		if (
			!isRecord(arc) ||
			!hasExactKeys(arc, ["edge_id", "direction"]) ||
			typeof arc.edge_id !== "string" ||
			(arc.direction !== "forward" && arc.direction !== "reverse")
		) {
			throw new Error("Flow scene contains an invalid enhanced-scaling arc");
		}
		return { edge_id: arc.edge_id, direction: arc.direction };
	};
	const components = value.components.map((component) => {
		if (
			!isRecord(component) ||
			!hasExactKeys(component, ["component_id", "members", "excess"]) ||
			typeof component.component_id !== "string" ||
			!Array.isArray(component.members) ||
			!component.members.every((member) => typeof member === "string")
		) {
			throw new Error(
				"Flow scene contains an invalid enhanced-scaling component",
			);
		}
		return {
			component_id: component.component_id,
			members: component.members as string[],
			excess: decodeRational(component.excess),
		};
	});
	const nodes = value.nodes.map((node) => {
		if (
			!isRecord(node) ||
			!hasExactKeys(
				node,
				["node_id", "component_id", "potential"],
				["distance"],
			) ||
			typeof node.node_id !== "string" ||
			typeof node.component_id !== "string" ||
			typeof node.potential !== "string" ||
			!canonicalI128.test(node.potential) ||
			(node.distance !== undefined &&
				(typeof node.distance !== "string" ||
					!canonicalI128.test(node.distance)))
		) {
			throw new Error("Flow scene contains an invalid enhanced-scaling node");
		}
		return {
			node_id: node.node_id,
			component_id: node.component_id,
			potential: node.potential,
			...(node.distance === undefined ? {} : { distance: node.distance }),
		};
	});
	const edges = value.edges.map((edge) => {
		if (
			!isRecord(edge) ||
			!hasExactKeys(edge, [
				"edge_id",
				"virtual_flow",
				"reduced_cost",
				"internal",
				"strongly_feasible",
				"tight",
			]) ||
			typeof edge.edge_id !== "string" ||
			typeof edge.reduced_cost !== "string" ||
			!canonicalI128.test(edge.reduced_cost) ||
			typeof edge.internal !== "boolean" ||
			typeof edge.strongly_feasible !== "boolean" ||
			typeof edge.tight !== "boolean"
		) {
			throw new Error("Flow scene contains an invalid enhanced-scaling edge");
		}
		return {
			edge_id: edge.edge_id,
			virtual_flow: decodeRational(edge.virtual_flow),
			reduced_cost: edge.reduced_cost,
			internal: edge.internal,
			strongly_feasible: edge.strongly_feasible,
			tight: edge.tight,
		};
	});
	const delta = decodeRational(value.delta);
	const augmentation =
		value.augmentation === undefined
			? undefined
			: decodeRational(value.augmentation);
	if (
		BigInt(delta.numerator) < 0n ||
		edges.some((edge) => BigInt(edge.virtual_flow.numerator) < 0n) ||
		(augmentation !== undefined && BigInt(augmentation.numerator) <= 0n)
	) {
		throw new Error("Enhanced-scaling flow and delta must be nonnegative");
	}
	return {
		stage: value.stage as FlowEnhancedCapacityScalingOverlayV1["stage"],
		delta,
		phase: value.phase,
		components,
		nodes,
		edges,
		...(value.source_component === undefined
			? {}
			: { source_component: value.source_component }),
		...(value.sink_component === undefined
			? {}
			: { sink_component: value.sink_component }),
		path: value.path.map(decodeArc),
		...(value.contraction_arc === undefined
			? {}
			: { contraction_arc: value.contraction_arc }),
		...(augmentation === undefined ? {} : { augmentation }),
	};
}

function decodeOrlinMcfOverlay(
	value: unknown,
): FlowOrlinMcfOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"delta",
				"phase",
				"components",
				"nodes",
				"arcs",
				"path",
				"inspected_segment",
				"eliminated_capacity_nodes",
				"shortcut_arcs",
			],
			[
				"source_component",
				"sink_component",
				"inspection_serial",
				"contraction_arc",
				"augmentation",
			],
		) ||
		!(
			[
				"ready",
				"transform-capacities",
				"initialize-dual",
				"complete-regeneration",
				"begin-phase",
				"inspect-contractible-arc",
				"inspect-reachability-arc",
				"inspect-compressed-residual-arc",
				"inspect-compressed-arc",
				"contract",
				"select-compressed-path",
				"augment",
				"complete-phase",
				"halve-scale",
				"expand-dual",
				"recover-primal",
				"optimal",
			] as const
		).includes(value.stage as FlowOrlinMcfOverlayV1["stage"]) ||
		typeof value.phase !== "string" ||
		!canonicalU64.test(value.phase) ||
		typeof value.eliminated_capacity_nodes !== "string" ||
		!canonicalU64.test(value.eliminated_capacity_nodes) ||
		typeof value.shortcut_arcs !== "string" ||
		!canonicalU64.test(value.shortcut_arcs) ||
		!Array.isArray(value.components) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.arcs) ||
		!Array.isArray(value.path) ||
		!Array.isArray(value.inspected_segment) ||
		(value.inspection_serial !== undefined &&
			(typeof value.inspection_serial !== "string" ||
				!canonicalU64.test(value.inspection_serial))) ||
		(value.source_component !== undefined &&
			typeof value.source_component !== "string") ||
		(value.sink_component !== undefined &&
			typeof value.sink_component !== "string")
	) {
		throw new Error("Flow scene contains an invalid Orlin MCF overlay");
	}
	const decodeArcRef = (arc: unknown): FlowOrlinMcfArcRefV1 => {
		if (
			!isRecord(arc) ||
			!hasExactKeys(arc, ["edge_id", "branch", "direction"]) ||
			typeof arc.edge_id !== "string" ||
			(arc.branch !== "flow" && arc.branch !== "slack") ||
			(arc.direction !== "forward" && arc.direction !== "reverse")
		) {
			throw new Error("Flow scene contains an invalid Orlin MCF arc ref");
		}
		return {
			edge_id: arc.edge_id,
			branch: arc.branch,
			direction: arc.direction,
		};
	};
	const components = value.components.map((component) => {
		if (
			!isRecord(component) ||
			!hasExactKeys(component, ["component_id", "members", "excess"]) ||
			typeof component.component_id !== "string" ||
			!Array.isArray(component.members) ||
			!component.members.every((member) => typeof member === "string")
		) {
			throw new Error("Flow scene contains an invalid Orlin MCF component");
		}
		return {
			component_id: component.component_id,
			members: component.members as string[],
			excess: decodeRational(component.excess),
		};
	});
	const nodes = value.nodes.map((node) => {
		if (
			!isRecord(node) ||
			!hasExactKeys(
				node,
				["node_id", "kind", "component_id", "potential"],
				["capacity_edge_id", "distance"],
			) ||
			typeof node.node_id !== "string" ||
			(node.kind !== "original" && node.kind !== "capacity") ||
			typeof node.component_id !== "string" ||
			typeof node.potential !== "string" ||
			!canonicalI128.test(node.potential) ||
			(node.capacity_edge_id !== undefined &&
				typeof node.capacity_edge_id !== "string") ||
			(node.distance !== undefined &&
				(typeof node.distance !== "string" ||
					!canonicalI128.test(node.distance))) ||
			(node.kind === "original" && node.capacity_edge_id !== undefined) ||
			(node.kind === "capacity" && node.capacity_edge_id === undefined)
		) {
			throw new Error("Flow scene contains an invalid Orlin MCF node");
		}
		return {
			node_id: node.node_id,
			kind: node.kind as FlowOrlinMcfNodeStateV1["kind"],
			...(node.capacity_edge_id === undefined
				? {}
				: { capacity_edge_id: node.capacity_edge_id }),
			component_id: node.component_id,
			potential: node.potential,
			...(node.distance === undefined ? {} : { distance: node.distance }),
		};
	});
	const arcs = value.arcs.map((arc) => {
		if (
			!isRecord(arc) ||
			!hasExactKeys(arc, [
				"edge_id",
				"branch",
				"flow",
				"reduced_cost",
				"internal",
				"strongly_feasible",
				"tight",
			]) ||
			typeof arc.edge_id !== "string" ||
			(arc.branch !== "flow" && arc.branch !== "slack") ||
			typeof arc.reduced_cost !== "string" ||
			!canonicalI128.test(arc.reduced_cost) ||
			typeof arc.internal !== "boolean" ||
			typeof arc.strongly_feasible !== "boolean" ||
			typeof arc.tight !== "boolean"
		) {
			throw new Error("Flow scene contains an invalid Orlin MCF branch");
		}
		const flow = decodeRational(arc.flow);
		if (BigInt(flow.numerator) < 0n) {
			throw new Error("Orlin MCF transformed flow cannot be negative");
		}
		return {
			edge_id: arc.edge_id,
			branch: arc.branch as FlowOrlinMcfArcStateV1["branch"],
			flow,
			reduced_cost: arc.reduced_cost,
			internal: arc.internal,
			strongly_feasible: arc.strongly_feasible,
			tight: arc.tight,
		};
	});
	const delta = decodeRational(value.delta);
	const augmentation =
		value.augmentation === undefined
			? undefined
			: decodeRational(value.augmentation);
	if (
		BigInt(delta.numerator) < 0n ||
		(augmentation !== undefined && BigInt(augmentation.numerator) <= 0n)
	) {
		throw new Error("Orlin MCF delta and augmentation must be nonnegative");
	}
	return {
		stage: value.stage as FlowOrlinMcfOverlayV1["stage"],
		delta,
		phase: value.phase,
		components,
		nodes,
		arcs,
		...(value.source_component === undefined
			? {}
			: { source_component: value.source_component }),
		...(value.sink_component === undefined
			? {}
			: { sink_component: value.sink_component }),
		path: value.path.map(decodeArcRef),
		inspected_segment: value.inspected_segment.map(decodeArcRef),
		...(value.inspection_serial === undefined
			? {}
			: { inspection_serial: value.inspection_serial }),
		...(value.contraction_arc === undefined
			? {}
			: { contraction_arc: decodeArcRef(value.contraction_arc) }),
		...(augmentation === undefined ? {} : { augmentation }),
		eliminated_capacity_nodes: value.eliminated_capacity_nodes,
		shortcut_arcs: value.shortcut_arcs,
	};
}

function decodeOrlinMaxFlowOverlay(
	value: unknown,
): FlowOrlinMaxFlowOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"delta",
				"gamma",
				"nodes",
				"residual_arcs",
				"compact_arcs",
				"active_compact_path",
				"active_original_path",
				"threshold",
			],
			["phase_case"],
		) ||
		!(
			[
				"ready",
				"begin-improvement",
				"contract-abundant",
				"inspect-classification-arc",
				"classify",
				"select-case",
				"inspect-compact-construction-arc",
				"transfer-capacity",
				"build-subproblem",
				"augment-subproblem",
				"inspect-subproblem-arc",
				"complete-subproblem",
				"inspect-decomposition-arc",
				"inspect-lift-residual-arc",
				"lift-path",
				"expand-contraction",
				"inspect-expansion-residual-arc",
				"inspect-cut-residual-arc",
				"update-cut",
				"optimal",
			] as const
		).includes(value.stage as FlowOrlinMaxFlowOverlayV1["stage"]) ||
		typeof value.delta !== "string" ||
		!canonicalU64.test(value.delta) ||
		typeof value.threshold !== "string" ||
		!canonicalU64.test(value.threshold) ||
		(value.phase_case !== undefined &&
			value.phase_case !== "original-approximation" &&
			value.phase_case !== "compact-approximation" &&
			value.phase_case !== "compact-exact") ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.residual_arcs) ||
		!Array.isArray(value.compact_arcs) ||
		!Array.isArray(value.active_compact_path) ||
		!Array.isArray(value.active_original_path)
	) {
		throw new Error("Flow scene contains an invalid Orlin max-flow overlay");
	}
	const decodeResidualRef = (reference: unknown): FlowResidualArcRefV1 => {
		if (
			!isRecord(reference) ||
			!hasExactKeys(reference, ["edge_id", "direction"]) ||
			typeof reference.edge_id !== "string" ||
			(reference.direction !== "forward" && reference.direction !== "reverse")
		) {
			throw new Error("Flow scene contains an invalid Orlin residual ref");
		}
		return {
			edge_id: reference.edge_id,
			direction: reference.direction,
		};
	};
	const nodes = value.nodes.map((node): FlowOrlinMaxFlowNodeStateV1 => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, [
				"node_id",
				"component_id",
				"critical",
				"anti_potential",
				"source_side",
			]) ||
			typeof node.node_id !== "string" ||
			typeof node.component_id !== "string" ||
			typeof node.critical !== "boolean" ||
			typeof node.anti_potential !== "string" ||
			!canonicalI128.test(node.anti_potential) ||
			typeof node.source_side !== "boolean"
		) {
			throw new Error("Flow scene contains an invalid Orlin max-flow node");
		}
		return {
			node_id: node.node_id,
			component_id: node.component_id,
			critical: node.critical,
			anti_potential: node.anti_potential,
			source_side: node.source_side,
		};
	});
	const residualArcs = value.residual_arcs.map(
		(arc): FlowOrlinMaxFlowResidualArcStateV1 => {
			if (
				!isRecord(arc) ||
				!hasExactKeys(
					arc,
					[
						"edge_id",
						"direction",
						"capacity",
						"abundant",
						"anti_abundant",
						"small",
						"medium",
					],
					["inspection_serial"],
				) ||
				typeof arc.edge_id !== "string" ||
				(arc.direction !== "forward" && arc.direction !== "reverse") ||
				typeof arc.capacity !== "string" ||
				!canonicalU64.test(arc.capacity) ||
				typeof arc.abundant !== "boolean" ||
				typeof arc.anti_abundant !== "boolean" ||
				typeof arc.small !== "boolean" ||
				typeof arc.medium !== "boolean" ||
				(arc.inspection_serial !== undefined &&
					(typeof arc.inspection_serial !== "string" ||
						!canonicalU64.test(arc.inspection_serial) ||
						BigInt(arc.inspection_serial) === 0n)) ||
				(arc.abundant && arc.anti_abundant) ||
				(arc.small && arc.medium)
			) {
				throw new Error("Flow scene contains an invalid Orlin residual state");
			}
			return {
				edge_id: arc.edge_id,
				direction: arc.direction,
				capacity: arc.capacity,
				abundant: arc.abundant,
				anti_abundant: arc.anti_abundant,
				small: arc.small,
				medium: arc.medium,
				...(arc.inspection_serial === undefined
					? {}
					: { inspection_serial: arc.inspection_serial }),
			};
		},
	);
	const compactArcs = value.compact_arcs.map(
		(arc): FlowOrlinMaxFlowCompactArcStateV1 => {
			if (
				!isRecord(arc) ||
				!hasExactKeys(
					arc,
					[
						"ordinal",
						"from_component",
						"to_component",
						"kind",
						"capacity",
						"flow",
						"witness",
					],
					["inspection_serial"],
				) ||
				typeof arc.ordinal !== "string" ||
				!canonicalU64.test(arc.ordinal) ||
				typeof arc.from_component !== "string" ||
				typeof arc.to_component !== "string" ||
				(arc.kind !== "original" &&
					arc.kind !== "abundant-pseudo" &&
					arc.kind !== "transferred-pseudo") ||
				typeof arc.capacity !== "string" ||
				!canonicalU64.test(arc.capacity) ||
				typeof arc.flow !== "string" ||
				!canonicalU64.test(arc.flow) ||
				BigInt(arc.flow) > BigInt(arc.capacity) ||
				(arc.inspection_serial !== undefined &&
					(typeof arc.inspection_serial !== "string" ||
						!canonicalU64.test(arc.inspection_serial) ||
						BigInt(arc.inspection_serial) === 0n)) ||
				!Array.isArray(arc.witness)
			) {
				throw new Error("Flow scene contains an invalid Orlin compact arc");
			}
			return {
				ordinal: arc.ordinal,
				from_component: arc.from_component,
				to_component: arc.to_component,
				kind: arc.kind,
				capacity: arc.capacity,
				flow: arc.flow,
				witness: arc.witness.map(decodeResidualRef),
				...(arc.inspection_serial === undefined
					? {}
					: { inspection_serial: arc.inspection_serial }),
			};
		},
	);
	const activeCompactPath = value.active_compact_path.map(
		(reference): FlowOrlinMaxFlowCompactArcRefV1 => {
			if (
				!isRecord(reference) ||
				!hasExactKeys(reference, ["ordinal", "reverse"]) ||
				typeof reference.ordinal !== "string" ||
				!canonicalU64.test(reference.ordinal) ||
				typeof reference.reverse !== "boolean"
			) {
				throw new Error("Flow scene contains an invalid Orlin compact ref");
			}
			return { ordinal: reference.ordinal, reverse: reference.reverse };
		},
	);
	const gamma = decodeRational(value.gamma);
	if (BigInt(gamma.numerator) < 0n) {
		throw new Error("Orlin max-flow gamma cannot be negative");
	}
	return {
		stage: value.stage as FlowOrlinMaxFlowOverlayV1["stage"],
		delta: value.delta,
		gamma,
		...(value.phase_case === undefined
			? {}
			: {
					phase_case: value.phase_case as NonNullable<
						FlowOrlinMaxFlowOverlayV1["phase_case"]
					>,
				}),
		nodes,
		residual_arcs: residualArcs,
		compact_arcs: compactArcs,
		active_compact_path: activeCompactPath,
		active_original_path: value.active_original_path.map(decodeResidualRef),
		threshold: value.threshold,
	};
}

function decodeDualNetworkSimplexOverlay(
	value: unknown,
): FlowDualNetworkSimplexOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			["stage", "nodes", "edges", "cut_side"],
			["leaving_edge", "entering_edge", "inspected_edge", "pivot_price_delta"],
		) ||
		!(
			[
				"ready",
				"inspect-initial-arc",
				"initialize-dual-tree",
				"select-leaving",
				"inspect-entering-arc",
				"select-entering",
				"pivot",
				"optimal",
			] as const
		).includes(value.stage as FlowDualNetworkSimplexOverlayV1["stage"]) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges) ||
		!Array.isArray(value.cut_side) ||
		!value.cut_side.every((node) => typeof node === "string") ||
		(value.leaving_edge !== undefined &&
			typeof value.leaving_edge !== "string") ||
		(value.entering_edge !== undefined &&
			typeof value.entering_edge !== "string") ||
		(value.inspected_edge !== undefined &&
			typeof value.inspected_edge !== "string") ||
		(value.pivot_price_delta !== undefined &&
			(typeof value.pivot_price_delta !== "string" ||
				!canonicalI128.test(value.pivot_price_delta)))
	) {
		throw new Error("Flow scene contains an invalid dual-simplex overlay");
	}
	const nodes = value.nodes.map((node) => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, ["node_id", "potential", "initialized", "in_cut"]) ||
			typeof node.node_id !== "string" ||
			typeof node.potential !== "string" ||
			!canonicalI128.test(node.potential) ||
			typeof node.initialized !== "boolean" ||
			typeof node.in_cut !== "boolean"
		) {
			throw new Error("Flow scene contains an invalid dual-simplex node");
		}
		return {
			node_id: node.node_id,
			potential: node.potential,
			initialized: node.initialized,
			in_cut: node.in_cut,
		};
	});
	const edges = value.edges.map((edge) => {
		if (
			!isRecord(edge) ||
			!hasExactKeys(edge, [
				"edge_id",
				"basic_flow",
				"reduced_cost",
				"in_tree",
			]) ||
			typeof edge.edge_id !== "string" ||
			typeof edge.basic_flow !== "string" ||
			!canonicalI128.test(edge.basic_flow) ||
			typeof edge.reduced_cost !== "string" ||
			!canonicalI128.test(edge.reduced_cost) ||
			typeof edge.in_tree !== "boolean"
		) {
			throw new Error("Flow scene contains an invalid dual-simplex edge");
		}
		return {
			edge_id: edge.edge_id,
			basic_flow: edge.basic_flow,
			reduced_cost: edge.reduced_cost,
			in_tree: edge.in_tree,
		};
	});
	return {
		stage: value.stage as FlowDualNetworkSimplexOverlayV1["stage"],
		nodes,
		edges,
		cut_side: value.cut_side as string[],
		...(value.leaving_edge === undefined
			? {}
			: { leaving_edge: value.leaving_edge }),
		...(value.entering_edge === undefined
			? {}
			: { entering_edge: value.entering_edge }),
		...(value.inspected_edge === undefined
			? {}
			: { inspected_edge: value.inspected_edge }),
		...(value.pivot_price_delta === undefined
			? {}
			: { pivot_price_delta: value.pivot_price_delta }),
	};
}

function decodePolynomialDualResidualRef(value: unknown): FlowResidualArcRefV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["edge_id", "direction"]) ||
		typeof value.edge_id !== "string" ||
		(value.direction !== "forward" && value.direction !== "reverse")
	) {
		throw new Error("Flow scene contains an invalid polynomial-dual path arc");
	}
	return { edge_id: value.edge_id, direction: value.direction };
}

function decodePolynomialDualSimplexOverlay(
	value: unknown,
): FlowPolynomialDualSimplexOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"phase",
				"delta",
				"nodes",
				"edges",
				"augment_path",
				"bad_edges",
				"bad_nodes",
				"pivot_cut",
			],
			["active_node", "leaving_edge", "entering_edge", "pivot_price_delta"],
		) ||
		!(
			[
				"ready",
				"inspect-initial-arc",
				"initialize-tree",
				"initialize-pseudoflow",
				"begin-scale",
				"inspect-augmentation-arc",
				"select-active",
				"augment-to-root",
				"select-bad-arc",
				"inspect-entering-arc",
				"select-entering",
				"pivot-make-good",
				"finish-scale",
				"optimal",
			] as const
		).includes(value.stage as FlowPolynomialDualSimplexOverlayV1["stage"]) ||
		typeof value.phase !== "string" ||
		!canonicalU64.test(value.phase) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges) ||
		!Array.isArray(value.augment_path) ||
		!Array.isArray(value.bad_edges) ||
		!value.bad_edges.every((edge) => typeof edge === "string") ||
		!Array.isArray(value.bad_nodes) ||
		!value.bad_nodes.every((node) => typeof node === "string") ||
		!Array.isArray(value.pivot_cut) ||
		!value.pivot_cut.every((node) => typeof node === "string") ||
		(value.active_node !== undefined &&
			typeof value.active_node !== "string") ||
		(value.leaving_edge !== undefined &&
			typeof value.leaving_edge !== "string") ||
		(value.entering_edge !== undefined &&
			typeof value.entering_edge !== "string") ||
		(value.pivot_price_delta !== undefined &&
			(typeof value.pivot_price_delta !== "string" ||
				!canonicalI128.test(value.pivot_price_delta)))
	) {
		throw new Error("Flow scene contains an invalid polynomial-dual overlay");
	}
	const delta = decodeRational(value.delta);
	if (BigInt(delta.numerator) < 0n) {
		throw new Error("Polynomial-dual delta must be nonnegative");
	}
	const nodes = value.nodes.map((node): FlowPolynomialDualNodeStateV1 => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, [
				"node_id",
				"potential",
				"excess",
				"root",
				"active",
				"bad",
				"in_pivot_cut",
			]) ||
			typeof node.node_id !== "string" ||
			typeof node.potential !== "string" ||
			!canonicalI128.test(node.potential) ||
			typeof node.root !== "boolean" ||
			typeof node.active !== "boolean" ||
			typeof node.bad !== "boolean" ||
			typeof node.in_pivot_cut !== "boolean"
		) {
			throw new Error("Flow scene contains an invalid polynomial-dual node");
		}
		return {
			node_id: node.node_id,
			potential: node.potential,
			excess: decodeRational(node.excess),
			root: node.root,
			active: node.active,
			bad: node.bad,
			in_pivot_cut: node.in_pivot_cut,
		};
	});
	const edges = value.edges.map((edge): FlowPolynomialDualEdgeStateV1 => {
		if (
			!isRecord(edge) ||
			!hasExactKeys(
				edge,
				[
					"edge_id",
					"pseudoflow",
					"basic_flow",
					"reduced_cost",
					"in_tree",
					"bad",
					"in_augment_path",
				],
				["augment_direction"],
			) ||
			typeof edge.edge_id !== "string" ||
			typeof edge.basic_flow !== "string" ||
			!canonicalI128.test(edge.basic_flow) ||
			typeof edge.reduced_cost !== "string" ||
			!canonicalI128.test(edge.reduced_cost) ||
			typeof edge.in_tree !== "boolean" ||
			typeof edge.bad !== "boolean" ||
			typeof edge.in_augment_path !== "boolean" ||
			(edge.augment_direction !== undefined &&
				edge.augment_direction !== "forward" &&
				edge.augment_direction !== "reverse") ||
			(edge.augment_direction !== undefined) !== edge.in_augment_path
		) {
			throw new Error("Flow scene contains an invalid polynomial-dual edge");
		}
		return {
			edge_id: edge.edge_id,
			pseudoflow: decodeRational(edge.pseudoflow),
			basic_flow: edge.basic_flow,
			reduced_cost: edge.reduced_cost,
			in_tree: edge.in_tree,
			bad: edge.bad,
			in_augment_path: edge.in_augment_path,
			...(edge.augment_direction === undefined
				? {}
				: { augment_direction: edge.augment_direction }),
		};
	});
	return {
		stage: value.stage as FlowPolynomialDualSimplexOverlayV1["stage"],
		phase: value.phase,
		delta,
		nodes,
		edges,
		...(value.active_node === undefined
			? {}
			: { active_node: value.active_node }),
		augment_path: value.augment_path.map(decodePolynomialDualResidualRef),
		bad_edges: value.bad_edges as string[],
		bad_nodes: value.bad_nodes as string[],
		...(value.leaving_edge === undefined
			? {}
			: { leaving_edge: value.leaving_edge }),
		...(value.entering_edge === undefined
			? {}
			: { entering_edge: value.entering_edge }),
		pivot_cut: value.pivot_cut as string[],
		...(value.pivot_price_delta === undefined
			? {}
			: { pivot_price_delta: value.pivot_price_delta }),
	};
}

function decodePolynomialPrimalResidualRef(
	value: unknown,
): FlowPolynomialPrimalResidualRefV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["entity_id", "direction"], ["original_edge_id"]) ||
		typeof value.entity_id !== "string" ||
		(value.original_edge_id !== undefined &&
			typeof value.original_edge_id !== "string") ||
		(value.direction !== "forward" && value.direction !== "reverse")
	) {
		throw new Error("Flow scene contains an invalid polynomial-simplex arc");
	}
	return {
		entity_id: value.entity_id,
		...(value.original_edge_id === undefined
			? {}
			: { original_edge_id: value.original_edge_id }),
		direction: value.direction,
	};
}

function decodePolynomialPrimalNode(
	value: unknown,
): FlowPolynomialPrimalNodeStateV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["entity_id", "kind", "premultiplier", "flags"]) ||
		typeof value.entity_id !== "string" ||
		(value.kind !== "original" && value.kind !== "artificial-root") ||
		!Array.isArray(value.flags) ||
		!value.flags.every(
			(flag) =>
				flag === "eligible" ||
				flag === "awake" ||
				flag === "in-n-star" ||
				flag === "root",
		)
	) {
		throw new Error("Flow scene contains an invalid polynomial-simplex node");
	}
	return {
		entity_id: value.entity_id,
		kind: value.kind,
		premultiplier: decodeRational(value.premultiplier),
		flags: value.flags as FlowPolynomialPrimalNodeStateV1["flags"],
	};
}

function decodePolynomialPrimalEdge(
	value: unknown,
): FlowPolynomialPrimalEdgeStateV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"edge_id",
			"basis",
			"perturbed_flow",
			"unperturbed_basic_flow",
			"reduced_cost",
			"in_cycle",
			"entering",
			"leaving",
		]) ||
		typeof value.edge_id !== "string" ||
		!(["lower", "tree", "upper"] as const).includes(
			value.basis as FlowPolynomialPrimalEdgeStateV1["basis"],
		) ||
		typeof value.perturbed_flow !== "string" ||
		!canonicalI128.test(value.perturbed_flow) ||
		typeof value.unperturbed_basic_flow !== "string" ||
		!canonicalI128.test(value.unperturbed_basic_flow) ||
		typeof value.in_cycle !== "boolean" ||
		typeof value.entering !== "boolean" ||
		typeof value.leaving !== "boolean"
	) {
		throw new Error("Flow scene contains an invalid polynomial-simplex edge");
	}
	return {
		edge_id: value.edge_id,
		basis: value.basis as FlowPolynomialPrimalEdgeStateV1["basis"],
		perturbed_flow: value.perturbed_flow,
		unperturbed_basic_flow: value.unperturbed_basic_flow,
		reduced_cost: decodeRational(value.reduced_cost),
		in_cycle: value.in_cycle,
		entering: value.entering,
		leaving: value.leaving,
	};
}

function decodePolynomialPrimalArtificialEdge(
	value: unknown,
): FlowPolynomialPrimalArtificialEdgeStateV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"entity_id",
			"node_id",
			"basis",
			"perturbed_flow",
			"unperturbed_basic_flow",
			"in_cycle",
			"entering",
			"leaving",
		]) ||
		typeof value.entity_id !== "string" ||
		typeof value.node_id !== "string" ||
		!(["lower", "tree", "upper"] as const).includes(
			value.basis as FlowPolynomialPrimalArtificialEdgeStateV1["basis"],
		) ||
		typeof value.perturbed_flow !== "string" ||
		!canonicalI128.test(value.perturbed_flow) ||
		typeof value.unperturbed_basic_flow !== "string" ||
		!canonicalI128.test(value.unperturbed_basic_flow) ||
		typeof value.in_cycle !== "boolean" ||
		typeof value.entering !== "boolean" ||
		typeof value.leaving !== "boolean"
	) {
		throw new Error(
			"Flow scene contains an invalid polynomial-simplex artificial edge",
		);
	}
	return {
		entity_id: value.entity_id,
		node_id: value.node_id,
		basis: value.basis as FlowPolynomialPrimalArtificialEdgeStateV1["basis"],
		perturbed_flow: value.perturbed_flow,
		unperturbed_basic_flow: value.unperturbed_basic_flow,
		in_cycle: value.in_cycle,
		entering: value.entering,
		leaving: value.leaving,
	};
}

function decodePolynomialPrimalSimplexOverlay(
	value: unknown,
): FlowPolynomialPrimalSimplexOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"phase",
				"perturbation_scale",
				"nodes",
				"edges",
				"artificial_edges",
				"cycle",
			],
			["epsilon", "entering", "leaving_entity", "delta", "potential_shift"],
		) ||
		!(
			[
				"ready",
				"initialize-basis",
				"begin-scale",
				"inspect-residual",
				"select-admissible",
				"pivot",
				"modify-premultipliers",
				"finish-scale",
				"optimal",
			] as const
		).includes(value.stage as FlowPolynomialPrimalSimplexOverlayV1["stage"]) ||
		typeof value.phase !== "string" ||
		!canonicalU64.test(value.phase) ||
		typeof value.perturbation_scale !== "string" ||
		!canonicalU64.test(value.perturbation_scale) ||
		value.perturbation_scale === "0" ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges) ||
		!Array.isArray(value.artificial_edges) ||
		!Array.isArray(value.cycle) ||
		(value.leaving_entity !== undefined &&
			typeof value.leaving_entity !== "string")
	) {
		throw new Error(
			"Flow scene contains an invalid polynomial-simplex overlay",
		);
	}
	return {
		stage: value.stage as FlowPolynomialPrimalSimplexOverlayV1["stage"],
		phase: value.phase,
		...(value.epsilon === undefined
			? {}
			: { epsilon: decodeRational(value.epsilon) }),
		perturbation_scale: value.perturbation_scale,
		nodes: value.nodes.map(decodePolynomialPrimalNode),
		edges: value.edges.map(decodePolynomialPrimalEdge),
		artificial_edges: value.artificial_edges.map(
			decodePolynomialPrimalArtificialEdge,
		),
		...(value.entering === undefined
			? {}
			: { entering: decodePolynomialPrimalResidualRef(value.entering) }),
		...(value.leaving_entity === undefined
			? {}
			: { leaving_entity: value.leaving_entity }),
		cycle: value.cycle.map(decodePolynomialPrimalResidualRef),
		...(value.delta === undefined
			? {}
			: { delta: decodeRational(value.delta) }),
		...(value.potential_shift === undefined
			? {}
			: { potential_shift: decodeRational(value.potential_shift) }),
	};
}

function decodeDoubleScalingOverlay(
	value: unknown,
): FlowDoubleScalingOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"epsilon",
				"cost_multiplier",
				"delta",
				"cost_phase",
				"capacity_phase",
				"nodes",
				"edges",
				"admissible_arcs",
				"active_path",
			],
			["inspected_arc", "selected_root", "selected_deficit"],
		) ||
		!(
			[
				"ready",
				"initialize",
				"start-cost-phase",
				"start-capacity-phase",
				"select-root",
				"inspect-arc",
				"advance",
				"relabel",
				"retreat",
				"augment",
				"complete-cost-phase",
				"optimal",
			] as const
		).includes(value.stage as FlowDoubleScalingOverlayV1["stage"]) ||
		![
			value.epsilon,
			value.cost_multiplier,
			value.delta,
			value.cost_phase,
			value.capacity_phase,
		].every((field) => typeof field === "string" && canonicalU64.test(field)) ||
		BigInt(value.epsilon as string) < 1n ||
		BigInt(value.cost_multiplier as string) < 1n ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges) ||
		!Array.isArray(value.admissible_arcs) ||
		!Array.isArray(value.active_path) ||
		![value.selected_root, value.selected_deficit].every(
			(selected) =>
				selected === undefined ||
				(typeof selected === "string" && /^(node|edge):.+$/u.test(selected)),
		)
	) {
		throw new Error("Flow scene contains an invalid double-scaling overlay");
	}
	const nodes = value.nodes.map((node) => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, [
				"entity_id",
				"kind",
				"price",
				"imbalance",
				"cursor",
			]) ||
			typeof node.entity_id !== "string" ||
			(node.kind !== "original" && node.kind !== "edge") ||
			typeof node.price !== "string" ||
			!canonicalI128.test(node.price) ||
			typeof node.imbalance !== "string" ||
			!canonicalI128.test(node.imbalance) ||
			typeof node.cursor !== "string" ||
			!canonicalU64.test(node.cursor)
		) {
			throw new Error("Flow scene contains an invalid double-scaling node");
		}
		return {
			entity_id: node.entity_id,
			kind: node.kind as "original" | "edge",
			price: node.price,
			imbalance: node.imbalance,
			cursor: node.cursor,
		};
	});
	const edges = value.edges.map((edge) => {
		if (
			!isRecord(edge) ||
			!hasExactKeys(edge, ["edge_id", "flow_branch", "slack_branch"]) ||
			typeof edge.edge_id !== "string" ||
			typeof edge.flow_branch !== "string" ||
			!canonicalU64.test(edge.flow_branch) ||
			typeof edge.slack_branch !== "string" ||
			!canonicalU64.test(edge.slack_branch)
		) {
			throw new Error("Flow scene contains an invalid double-scaling edge");
		}
		return {
			edge_id: edge.edge_id,
			flow_branch: edge.flow_branch,
			slack_branch: edge.slack_branch,
		};
	});
	const decodeArc = (arc: unknown): FlowDoubleScalingArcRefV1 => {
		if (
			!isRecord(arc) ||
			!hasExactKeys(arc, ["edge_id", "branch", "direction"]) ||
			typeof arc.edge_id !== "string" ||
			(arc.branch !== "flow" && arc.branch !== "slack") ||
			(arc.direction !== "forward" && arc.direction !== "reverse")
		) {
			throw new Error("Flow scene contains an invalid double-scaling arc");
		}
		return {
			edge_id: arc.edge_id,
			branch: arc.branch,
			direction: arc.direction,
		};
	};
	return {
		stage: value.stage as FlowDoubleScalingOverlayV1["stage"],
		epsilon: value.epsilon as string,
		cost_multiplier: value.cost_multiplier as string,
		delta: value.delta as string,
		cost_phase: value.cost_phase as string,
		capacity_phase: value.capacity_phase as string,
		nodes,
		edges,
		admissible_arcs: value.admissible_arcs.map(decodeArc),
		active_path: value.active_path.map(decodeArc),
		...(value.inspected_arc === undefined
			? {}
			: { inspected_arc: decodeArc(value.inspected_arc) }),
		...(value.selected_root === undefined
			? {}
			: { selected_root: value.selected_root as string }),
		...(value.selected_deficit === undefined
			? {}
			: { selected_deficit: value.selected_deficit as string }),
	};
}

function decodeConvexCostOverlay(
	value: unknown,
): FlowConvexCostOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			["stage", "edges", "active_cycle"],
			["scale", "eligible_arcs"],
		) ||
		!(
			[
				"initialize",
				"select-minimum-mean-cycle",
				"cancel-cycle",
				"start-scale",
				"saturate-marginal",
				"inspect-marginal-arc",
				"shortest-path",
				"update-potentials",
				"augment",
				"complete-scale",
				"optimal",
			] as const
		).includes(value.stage as FlowConvexCostOverlayV1["stage"]) ||
		!Array.isArray(value.edges) ||
		!Array.isArray(value.active_cycle) ||
		(value.scale !== undefined &&
			(typeof value.scale !== "string" ||
				!canonicalU64.test(value.scale) ||
				BigInt(value.scale) === 0n)) ||
		(value.eligible_arcs !== undefined && !Array.isArray(value.eligible_arcs))
	) {
		throw new Error("Flow scene contains an invalid convex-cost overlay");
	}
	const edges = value.edges.map((edge) => {
		if (
			!isRecord(edge) ||
			!hasExactKeys(
				edge,
				["edge_id", "base_cost_at_zero", "flow", "total_cost", "segments"],
				["forward_marginal_cost", "reverse_marginal_cost"],
			) ||
			typeof edge.edge_id !== "string" ||
			![edge.base_cost_at_zero, edge.total_cost].every(
				(field) => typeof field === "string" && canonicalI128.test(field),
			) ||
			typeof edge.flow !== "string" ||
			!canonicalU64.test(edge.flow) ||
			![edge.forward_marginal_cost, edge.reverse_marginal_cost].every(
				(field) =>
					field === undefined ||
					(typeof field === "string" && canonicalI64.test(field)),
			) ||
			!Array.isArray(edge.segments)
		) {
			throw new Error("Flow scene contains an invalid convex-cost edge");
		}
		const segments = edge.segments.map((segment) => {
			if (
				!isRecord(segment) ||
				!hasExactKeys(segment, [
					"segment",
					"start_flow",
					"end_flow",
					"flow",
					"marginal_cost",
				]) ||
				![
					segment.segment,
					segment.start_flow,
					segment.end_flow,
					segment.flow,
				].every(
					(field) => typeof field === "string" && canonicalU64.test(field),
				) ||
				typeof segment.marginal_cost !== "string" ||
				!canonicalI64.test(segment.marginal_cost)
			) {
				throw new Error("Flow scene contains an invalid convex-cost segment");
			}
			return {
				segment: segment.segment as string,
				start_flow: segment.start_flow as string,
				end_flow: segment.end_flow as string,
				flow: segment.flow as string,
				marginal_cost: segment.marginal_cost,
			};
		});
		return {
			edge_id: edge.edge_id,
			base_cost_at_zero: edge.base_cost_at_zero as string,
			flow: edge.flow,
			total_cost: edge.total_cost as string,
			...(edge.forward_marginal_cost === undefined
				? {}
				: { forward_marginal_cost: edge.forward_marginal_cost as string }),
			...(edge.reverse_marginal_cost === undefined
				? {}
				: { reverse_marginal_cost: edge.reverse_marginal_cost as string }),
			segments,
		};
	});
	const decodeArc = (arc: unknown) => {
		if (
			!isRecord(arc) ||
			!hasExactKeys(arc, ["edge_id", "segment", "direction"]) ||
			typeof arc.edge_id !== "string" ||
			typeof arc.segment !== "string" ||
			!canonicalU64.test(arc.segment) ||
			(arc.direction !== "forward" && arc.direction !== "reverse")
		) {
			throw new Error("Flow scene contains an invalid convex-cost active arc");
		}
		return {
			edge_id: arc.edge_id,
			segment: arc.segment,
			direction: arc.direction as "forward" | "reverse",
		};
	};
	const active_cycle = value.active_cycle.map(decodeArc);
	const eligible_arcs = (value.eligible_arcs ?? []).map(decodeArc);
	return {
		stage: value.stage as FlowConvexCostOverlayV1["stage"],
		...(value.scale === undefined ? {} : { scale: value.scale as string }),
		edges,
		active_cycle,
		eligible_arcs,
	};
}

function decodeConvexSimplexArc(
	value: unknown,
): FlowConvexNetworkSimplexArcRefV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["entity_id", "direction"], ["segment"]) ||
		typeof value.entity_id !== "string" ||
		(value.direction !== "forward" && value.direction !== "reverse") ||
		(value.segment !== undefined &&
			(typeof value.segment !== "string" || !canonicalU64.test(value.segment)))
	) {
		throw new Error("Flow scene contains an invalid convex-simplex arc");
	}
	return {
		entity_id: value.entity_id,
		direction: value.direction,
		...(value.segment === undefined ? {} : { segment: value.segment }),
	};
}

function decodeConvexSimplexNode(
	value: unknown,
): FlowConvexNetworkSimplexNodeStateV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["entity_id", "potential"], ["parent"]) ||
		typeof value.entity_id !== "string" ||
		typeof value.potential !== "string" ||
		!canonicalI128.test(value.potential) ||
		(value.parent !== undefined && typeof value.parent !== "string")
	) {
		throw new Error("Flow scene contains an invalid convex-simplex node");
	}
	return {
		entity_id: value.entity_id,
		potential: value.potential,
		...(value.parent === undefined ? {} : { parent: value.parent }),
	};
}

function decodeConvexSimplexEdge(
	value: unknown,
): FlowConvexNetworkSimplexEdgeStateV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			["edge_id", "basis", "in_cycle", "entering", "leaving"],
			["active_segment"],
		) ||
		typeof value.edge_id !== "string" ||
		(value.basis !== "tree" && value.basis !== "breakpoint") ||
		[value.in_cycle, value.entering, value.leaving].some(
			(flag) => typeof flag !== "boolean",
		) ||
		(value.active_segment !== undefined &&
			(typeof value.active_segment !== "string" ||
				!canonicalU64.test(value.active_segment)))
	) {
		throw new Error("Flow scene contains an invalid convex-simplex edge");
	}
	return value as FlowConvexNetworkSimplexEdgeStateV1;
}

function decodeConvexSimplexArtificialEdge(
	value: unknown,
): FlowConvexNetworkSimplexArtificialEdgeV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"entity_id",
			"node_id",
			"source",
			"target",
			"flow",
			"basis",
			"in_cycle",
			"entering",
			"leaving",
		]) ||
		![value.entity_id, value.node_id, value.source, value.target].every(
			(field) => typeof field === "string",
		) ||
		typeof value.flow !== "string" ||
		!canonicalU64.test(value.flow) ||
		(value.basis !== "tree" && value.basis !== "breakpoint") ||
		[value.in_cycle, value.entering, value.leaving].some(
			(flag) => typeof flag !== "boolean",
		)
	) {
		throw new Error(
			"Flow scene contains an invalid convex-simplex artificial edge",
		);
	}
	return value as FlowConvexNetworkSimplexArtificialEdgeV1;
}

function decodeConvexNetworkSimplexOverlay(
	value: unknown,
): FlowConvexNetworkSimplexOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowConvexNetworkSimplexOverlayV1["stage"][] = [
		"initialize-basis",
		"price",
		"form-cycle",
		"cross-breakpoint",
		"exchange-basis",
		"flip-bound",
		"optimal",
	];
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"artificial_cost",
				"nodes",
				"edges",
				"artificial_edges",
				"cycle",
			],
			["entering", "leaving"],
		) ||
		!stages.includes(
			value.stage as FlowConvexNetworkSimplexOverlayV1["stage"],
		) ||
		typeof value.artificial_cost !== "string" ||
		!canonicalI128.test(value.artificial_cost) ||
		BigInt(value.artificial_cost) <= 0n ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges) ||
		!Array.isArray(value.artificial_edges) ||
		!Array.isArray(value.cycle)
	) {
		throw new Error("Flow scene contains an invalid convex-simplex overlay");
	}
	return {
		stage: value.stage as FlowConvexNetworkSimplexOverlayV1["stage"],
		artificial_cost: value.artificial_cost,
		nodes: value.nodes.map(decodeConvexSimplexNode),
		edges: value.edges.map(decodeConvexSimplexEdge),
		artificial_edges: value.artificial_edges.map(
			decodeConvexSimplexArtificialEdge,
		),
		cycle: value.cycle.map(decodeConvexSimplexArc),
		...(value.entering === undefined
			? {}
			: { entering: decodeConvexSimplexArc(value.entering) }),
		...(value.leaving === undefined
			? {}
			: { leaving: decodeConvexSimplexArc(value.leaving) }),
	};
}

function decodePredictionAssistedEpsilonNode(
	value: unknown,
): FlowPredictionAssistedEpsilonNodeStateV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"node_id",
			"raw_predicted_price",
			"predicted_price",
			"prediction_clipped",
			"price",
			"surplus",
			"active",
		]) ||
		typeof value.node_id !== "string" ||
		![
			value.raw_predicted_price,
			value.predicted_price,
			value.price,
			value.surplus,
		].every(
			(field) => typeof field === "string" && canonicalI128.test(field),
		) ||
		typeof value.prediction_clipped !== "boolean" ||
		typeof value.active !== "boolean"
	) {
		throw new Error("Flow scene contains an invalid prediction-assisted node");
	}
	return value as FlowPredictionAssistedEpsilonNodeStateV1;
}

function decodePredictionAssistedEpsilonEdge(
	value: unknown,
): FlowPredictionAssistedEpsilonEdgeStateV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["edge_id", "scaled_cost"]) ||
		typeof value.edge_id !== "string" ||
		typeof value.scaled_cost !== "string" ||
		!canonicalI128.test(value.scaled_cost)
	) {
		throw new Error("Flow scene contains an invalid prediction-assisted edge");
	}
	return value as FlowPredictionAssistedEpsilonEdgeStateV1;
}

function decodePredictionAssistedEpsilonOverlay(
	value: unknown,
): FlowPredictionAssistedEpsilonOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowPredictionAssistedEpsilonOverlayV1["stage"][] = [
		"preprocess-prediction",
		"begin-attempt",
		"initialize-scale",
		"select-surplus",
		"inspect-admissible-arc",
		"inspect-price-breakpoint-arc",
		"push",
		"raise-price",
		"complete-up-iteration",
		"complete-scale",
		"abort-attempt",
		"optimal",
	];
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"scaling_parameter",
				"attempt",
				"maximum_attempt",
				"exponent",
				"nodes",
				"edges",
			],
			[
				"scale_exponent",
				"certificate_aligned_prediction_error",
				"active_node",
				"active_arc",
			],
		) ||
		!stages.includes(
			value.stage as FlowPredictionAssistedEpsilonOverlayV1["stage"],
		) ||
		![
			value.scaling_parameter,
			value.attempt,
			value.maximum_attempt,
			value.exponent,
		].every((field) => typeof field === "string" && canonicalU64.test(field)) ||
		(value.scale_exponent !== undefined &&
			(typeof value.scale_exponent !== "string" ||
				!canonicalU64.test(value.scale_exponent))) ||
		(value.certificate_aligned_prediction_error !== undefined &&
			(typeof value.certificate_aligned_prediction_error !== "string" ||
				!canonicalI128.test(value.certificate_aligned_prediction_error) ||
				BigInt(value.certificate_aligned_prediction_error) < 0n)) ||
		(value.active_node !== undefined &&
			typeof value.active_node !== "string") ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges)
	) {
		throw new Error(
			"Flow scene contains an invalid prediction-assisted overlay",
		);
	}
	return {
		stage: value.stage as FlowPredictionAssistedEpsilonOverlayV1["stage"],
		scaling_parameter: value.scaling_parameter as string,
		attempt: value.attempt as string,
		maximum_attempt: value.maximum_attempt as string,
		exponent: value.exponent as string,
		nodes: value.nodes.map(decodePredictionAssistedEpsilonNode),
		edges: value.edges.map(decodePredictionAssistedEpsilonEdge),
		...(value.scale_exponent === undefined
			? {}
			: { scale_exponent: value.scale_exponent as string }),
		...(value.certificate_aligned_prediction_error === undefined
			? {}
			: {
					certificate_aligned_prediction_error:
						value.certificate_aligned_prediction_error as string,
				}),
		...(value.active_node === undefined
			? {}
			: { active_node: value.active_node as string }),
		...(value.active_arc === undefined
			? {}
			: { active_arc: decodePolynomialDualResidualRef(value.active_arc) }),
	};
}

function decodeTardosFixedVariable(value: unknown): FlowTardosFixedVariableV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"edge_id",
			"bound",
			"value",
			"direction",
			"reduced_cost",
		]) ||
		typeof value.edge_id !== "string" ||
		(value.bound !== "lower" && value.bound !== "upper") ||
		typeof value.value !== "string" ||
		!canonicalU64.test(value.value) ||
		(value.direction !== "forward" && value.direction !== "reverse") ||
		typeof value.reduced_cost !== "string" ||
		!canonicalI128.test(value.reduced_cost)
	) {
		throw new Error("Flow scene contains an invalid Tardos fixed variable");
	}
	return value as FlowTardosFixedVariableV1;
}

function decodeTardosFrameworkOverlay(
	value: unknown,
): FlowTardosFrameworkOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowTardosFrameworkOverlayV1["stage"][] = [
		"ready",
		"construct-feasible-flow",
		"measure-epsilon",
		"classify-fixed-variables",
		"complete",
	];
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"stage",
			"epsilon",
			"threshold",
			"determinant_bound",
			"nodes",
			"residual_arcs",
			"fixed_variables",
		]) ||
		!stages.includes(value.stage as FlowTardosFrameworkOverlayV1["stage"]) ||
		![value.epsilon, value.threshold].every(
			(field) =>
				typeof field === "string" &&
				canonicalI128.test(field) &&
				BigInt(field) >= 0n,
		) ||
		typeof value.determinant_bound !== "string" ||
		!canonicalU64.test(value.determinant_bound) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.residual_arcs) ||
		!Array.isArray(value.fixed_variables)
	) {
		throw new Error("Flow scene contains an invalid Tardos framework overlay");
	}
	const nodes = value.nodes.map((node) => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, ["node_id", "potential"]) ||
			typeof node.node_id !== "string" ||
			typeof node.potential !== "string" ||
			!canonicalI128.test(node.potential)
		) {
			throw new Error("Flow scene contains an invalid Tardos node state");
		}
		return { node_id: node.node_id, potential: node.potential };
	});
	const residualArcs = value.residual_arcs.map((arc) => {
		if (
			!isRecord(arc) ||
			!hasExactKeys(arc, [
				"edge_id",
				"direction",
				"capacity",
				"reduced_cost",
				"fixes_variable",
			]) ||
			typeof arc.edge_id !== "string" ||
			(arc.direction !== "forward" && arc.direction !== "reverse") ||
			typeof arc.capacity !== "string" ||
			!canonicalU64.test(arc.capacity) ||
			arc.capacity === "0" ||
			typeof arc.reduced_cost !== "string" ||
			!canonicalI128.test(arc.reduced_cost) ||
			typeof arc.fixes_variable !== "boolean"
		) {
			throw new Error("Flow scene contains an invalid Tardos residual state");
		}
		return arc as FlowTardosFrameworkOverlayV1["residual_arcs"][number];
	});
	return {
		stage: value.stage as FlowTardosFrameworkOverlayV1["stage"],
		epsilon: value.epsilon as string,
		threshold: value.threshold as string,
		determinant_bound: value.determinant_bound,
		nodes,
		residual_arcs: residualArcs,
		fixed_variables: value.fixed_variables.map(decodeTardosFixedVariable),
	};
}

function decodeElectricalFlowOverlay(
	value: unknown,
): FlowElectricalFlowOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowElectricalFlowOverlayV1["stage"][] = [
		"ready",
		"assemble-laplacian",
		"initialize-conjugate-gradient",
		"conjugate-gradient-iteration",
		"recover-currents",
		"check-exact-reference",
		"complete",
	];
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"target_current",
				"relative_tolerance",
				"iteration",
				"residual_l2",
				"effective_resistance",
				"total_energy",
				"converged",
				"nodes",
				"edges",
			],
			["exact_effective_resistance", "maximum_absolute_error"],
		) ||
		!stages.includes(value.stage as FlowElectricalFlowOverlayV1["stage"]) ||
		value.target_current !== "1" ||
		value.relative_tolerance !== "0.0000000001" ||
		typeof value.iteration !== "string" ||
		!canonicalU64.test(value.iteration) ||
		typeof value.converged !== "boolean" ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges)
	) {
		throw new Error("Flow scene contains an invalid electrical-flow overlay");
	}
	const residualL2 = decodeFiniteDecimal(value.residual_l2);
	const effectiveResistance = decodeFiniteDecimal(value.effective_resistance);
	const totalEnergy = decodeFiniteDecimal(value.total_energy);
	if (
		Number(residualL2) < 0 ||
		Number(totalEnergy) < 0 ||
		(value.maximum_absolute_error !== undefined &&
			Number(decodeFiniteDecimal(value.maximum_absolute_error)) < 0)
	) {
		throw new Error("Flow scene electrical magnitudes must be nonnegative");
	}
	const nodes = value.nodes.map((node): FlowElectricalNodeStateV1 => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, [
				"node_id",
				"potential",
				"residual",
				"search_direction",
				"grounded",
			]) ||
			typeof node.node_id !== "string" ||
			typeof node.grounded !== "boolean"
		) {
			throw new Error("Flow scene contains an invalid electrical node state");
		}
		return {
			node_id: node.node_id,
			potential: decodeFiniteDecimal(node.potential),
			residual: decodeFiniteDecimal(node.residual),
			search_direction: decodeFiniteDecimal(node.search_direction),
			grounded: node.grounded,
		};
	});
	const edges = value.edges.map((edge): FlowElectricalEdgeStateV1 => {
		if (
			!isRecord(edge) ||
			!hasExactKeys(edge, [
				"edge_id",
				"resistance",
				"conductance",
				"voltage_drop",
				"current",
				"congestion",
				"energy",
			]) ||
			typeof edge.edge_id !== "string" ||
			typeof edge.conductance !== "string" ||
			!canonicalU64.test(edge.conductance) ||
			edge.conductance === "0"
		) {
			throw new Error("Flow scene contains an invalid electrical edge state");
		}
		const congestion = decodeFiniteDecimal(edge.congestion);
		const energy = decodeFiniteDecimal(edge.energy);
		if (Number(congestion) < 0 || Number(energy) < 0) {
			throw new Error(
				"Flow scene electrical edge magnitudes must be nonnegative",
			);
		}
		return {
			edge_id: edge.edge_id,
			resistance: decodeRational(edge.resistance),
			conductance: edge.conductance,
			voltage_drop: decodeFiniteDecimal(edge.voltage_drop),
			current: decodeFiniteDecimal(edge.current),
			congestion,
			energy,
		};
	});
	return {
		stage: value.stage as FlowElectricalFlowOverlayV1["stage"],
		target_current: "1",
		relative_tolerance: "0.0000000001",
		iteration: value.iteration,
		residual_l2: residualL2,
		effective_resistance: effectiveResistance,
		total_energy: totalEnergy,
		converged: value.converged,
		nodes,
		edges,
		...(value.exact_effective_resistance === undefined
			? {}
			: {
					exact_effective_resistance: decodeRational(
						value.exact_effective_resistance,
					),
				}),
		...(value.maximum_absolute_error === undefined
			? {}
			: {
					maximum_absolute_error: decodeFiniteDecimal(
						value.maximum_absolute_error,
					),
				}),
	};
}

function decodeAugmentingElectricalOverlay(
	value: unknown,
): FlowAugmentingElectricalOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowAugmentingElectricalOverlayV1["stage"][] = [
		"ready",
		"build-directed-reduction",
		"add-preconditioning",
		"install-target-cut",
		"solve-electrical-direction",
		"boost-high-energy-arc",
		"augment-primal-dual",
		"fix-coupling",
		"collapse-boost-paths",
		"round-central-flow",
		"cleanup-augmenting-path",
		"extract-directed-flow",
		"cancel-extraction-cycle",
		"round-directed-flow",
		"check-certificate",
		"optimal",
	];
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"original_target",
				"transformed_target",
				"working_target",
				"current_value",
				"alpha",
				"remaining",
				"electrical_energy",
				"congestion_l3",
				"congestion_l4",
				"coupling_l2",
				"working_nodes",
				"working_edges",
				"active_working_path",
				"active_extraction_cycle",
				"nodes",
				"edges",
			],
			["active_working_edge", "active_pivot_node", "active_discrete_amount"],
		) ||
		!stages.includes(
			value.stage as FlowAugmentingElectricalOverlayV1["stage"],
		) ||
		![
			value.original_target,
			value.transformed_target,
			value.working_target,
			value.working_nodes,
			value.working_edges,
		].every((count) => typeof count === "string" && canonicalU64.test(count)) ||
		(value.active_working_edge !== undefined &&
			(typeof value.active_working_edge !== "string" ||
				!canonicalU64.test(value.active_working_edge))) ||
		(value.active_pivot_node !== undefined &&
			(typeof value.active_pivot_node !== "string" ||
				!canonicalU64.test(value.active_pivot_node))) ||
		(value.active_discrete_amount !== undefined &&
			(typeof value.active_discrete_amount !== "string" ||
				!canonicalU64.test(value.active_discrete_amount))) ||
		!Array.isArray(value.active_working_path) ||
		!Array.isArray(value.active_extraction_cycle) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges)
	) {
		throw new Error(
			"Flow scene contains an invalid augmenting-electrical overlay",
		);
	}
	const magnitudes = {
		current_value: decodeFiniteDecimal(value.current_value),
		alpha: decodeFiniteDecimal(value.alpha),
		remaining: decodeFiniteDecimal(value.remaining),
		electrical_energy: decodeFiniteDecimal(value.electrical_energy),
		congestion_l3: decodeFiniteDecimal(value.congestion_l3),
		congestion_l4: decodeFiniteDecimal(value.congestion_l4),
		coupling_l2: decodeFiniteDecimal(value.coupling_l2),
	};
	if (
		Number(magnitudes.current_value) < 0 ||
		Number(magnitudes.alpha) < 0 ||
		Number(magnitudes.alpha) > 1 + 1e-8 ||
		Number(magnitudes.remaining) < 0 ||
		Number(magnitudes.electrical_energy) < 0 ||
		Number(magnitudes.congestion_l3) < 0 ||
		Number(magnitudes.congestion_l4) < 0 ||
		Number(magnitudes.coupling_l2) < 0
	) {
		throw new Error(
			"Flow scene augmenting-electrical magnitudes must be nonnegative",
		);
	}
	const nodes = value.nodes.map((node): FlowAugmentingElectricalNodeStateV1 => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, [
				"node_id",
				"potential",
				"coupling_violation",
				"target_source_side",
			]) ||
			typeof node.node_id !== "string" ||
			typeof node.target_source_side !== "boolean"
		) {
			throw new Error(
				"Flow scene contains an invalid augmenting-electrical node state",
			);
		}
		const couplingViolation = decodeFiniteDecimal(node.coupling_violation);
		if (Number(couplingViolation) < 0) {
			throw new Error(
				"Flow scene augmenting-electrical coupling must be nonnegative",
			);
		}
		return {
			node_id: node.node_id,
			potential: decodeFiniteDecimal(node.potential),
			coupling_violation: couplingViolation,
			target_source_side: node.target_source_side,
		};
	});
	const edgeStates = value.edges.map(
		(edge): FlowAugmentingElectricalEdgeStateV1 => {
			if (
				!isRecord(edge) ||
				!hasExactKeys(
					edge,
					[
						"edge_id",
						"central_flow",
						"electrical_current",
						"forward_residual",
						"backward_residual",
						"congestion",
						"resistance",
						"boost_segments",
					],
					[
						"rounded_central_flow",
						"extraction_central_scaled",
						"extraction_toward_source",
						"extraction_out_of_sink",
						"final_flow",
					],
				) ||
				typeof edge.edge_id !== "string" ||
				typeof edge.boost_segments !== "string" ||
				!canonicalU64.test(edge.boost_segments) ||
				(edge.rounded_central_flow !== undefined &&
					(typeof edge.rounded_central_flow !== "string" ||
						!canonicalI64.test(edge.rounded_central_flow))) ||
				(edge.extraction_central_scaled !== undefined &&
					(typeof edge.extraction_central_scaled !== "string" ||
						!canonicalU64.test(edge.extraction_central_scaled))) ||
				(edge.extraction_toward_source !== undefined &&
					(typeof edge.extraction_toward_source !== "string" ||
						!canonicalU64.test(edge.extraction_toward_source))) ||
				(edge.extraction_out_of_sink !== undefined &&
					(typeof edge.extraction_out_of_sink !== "string" ||
						!canonicalU64.test(edge.extraction_out_of_sink))) ||
				(edge.final_flow !== undefined &&
					(typeof edge.final_flow !== "string" ||
						!canonicalU64.test(edge.final_flow)))
			) {
				throw new Error(
					"Flow scene contains an invalid augmenting-electrical edge state",
				);
			}
			const state = {
				edge_id: edge.edge_id,
				central_flow: decodeFiniteDecimal(edge.central_flow),
				electrical_current: decodeFiniteDecimal(edge.electrical_current),
				forward_residual: decodeFiniteDecimal(edge.forward_residual),
				backward_residual: decodeFiniteDecimal(edge.backward_residual),
				congestion: decodeFiniteDecimal(edge.congestion),
				resistance: decodeFiniteDecimal(edge.resistance),
				boost_segments: edge.boost_segments,
				...(edge.rounded_central_flow === undefined
					? {}
					: { rounded_central_flow: edge.rounded_central_flow }),
				...(edge.extraction_central_scaled === undefined
					? {}
					: { extraction_central_scaled: edge.extraction_central_scaled }),
				...(edge.extraction_toward_source === undefined
					? {}
					: { extraction_toward_source: edge.extraction_toward_source }),
				...(edge.extraction_out_of_sink === undefined
					? {}
					: { extraction_out_of_sink: edge.extraction_out_of_sink }),
				...(edge.final_flow === undefined
					? {}
					: { final_flow: edge.final_flow }),
			};
			if (
				Number(state.forward_residual) < 0 ||
				Number(state.backward_residual) < 0 ||
				Number(state.congestion) < 0 ||
				Number(state.resistance) < 0
			) {
				throw new Error(
					"Flow scene augmenting-electrical edge magnitudes must be nonnegative",
				);
			}
			return state;
		},
	);
	const activeWorkingPath = value.active_working_path.map(
		(arc): FlowSceneWire.FlowAugmentingElectricalWorkingArcV1 => {
			if (
				!isRecord(arc) ||
				!hasExactKeys(arc, [
					"edge",
					"direction",
					"from_node",
					"to_node",
					"flow_after",
				]) ||
				typeof arc.edge !== "string" ||
				!canonicalU64.test(arc.edge) ||
				typeof arc.from_node !== "string" ||
				typeof arc.to_node !== "string" ||
				typeof arc.flow_after !== "string" ||
				!canonicalI64.test(arc.flow_after) ||
				(arc.direction !== "forward" && arc.direction !== "reverse")
			) {
				throw new Error(
					"Flow scene contains an invalid augmenting-electrical working path",
				);
			}
			return {
				edge: arc.edge,
				direction: arc.direction,
				from_node: arc.from_node,
				to_node: arc.to_node,
				flow_after: arc.flow_after,
			};
		},
	);
	const activeExtractionCycle = value.active_extraction_cycle.map(
		(arc): FlowSceneWire.FlowAugmentingElectricalExtractionArcV1 => {
			if (
				!isRecord(arc) ||
				!hasExactKeys(arc, ["edge", "kind"]) ||
				typeof arc.edge !== "string" ||
				!canonicalU64.test(arc.edge) ||
				!(["central", "toward-source", "out-of-sink"] as const).includes(
					arc.kind as never,
				)
			) {
				throw new Error(
					"Flow scene contains an invalid augmenting-electrical extraction cycle",
				);
			}
			return {
				edge: arc.edge,
				kind: arc.kind as FlowSceneWire.FlowAugmentingElectricalExtractionArcKindV1,
			};
		},
	);
	if (
		(activeWorkingPath.length === 0 && activeExtractionCycle.length === 0) !==
			(value.active_discrete_amount === undefined) ||
		(activeWorkingPath.length > 0 && activeExtractionCycle.length > 0)
	) {
		throw new Error(
			"Flow scene augmenting-electrical cleanup path and amount must be published together",
		);
	}
	return {
		stage: value.stage as FlowAugmentingElectricalOverlayV1["stage"],
		original_target: value.original_target as string,
		transformed_target: value.transformed_target as string,
		working_target: value.working_target as string,
		...magnitudes,
		working_nodes: value.working_nodes as string,
		working_edges: value.working_edges as string,
		...(value.active_working_edge === undefined
			? {}
			: { active_working_edge: value.active_working_edge as string }),
		...(value.active_pivot_node === undefined
			? {}
			: { active_pivot_node: value.active_pivot_node as string }),
		active_working_path: activeWorkingPath,
		active_extraction_cycle: activeExtractionCycle,
		...(value.active_discrete_amount === undefined
			? {}
			: { active_discrete_amount: value.active_discrete_amount as string }),
		nodes,
		edges: edgeStates,
	};
}

function decodeInteriorPointMaxFlowOverlay(
	value: unknown,
): FlowInteriorPointMaxFlowOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowInteriorPointMaxFlowOverlayV1["stage"][] = [
		"ready",
		"enumerate-target-cut",
		"build-b-matching-reduction",
		"build-min-cost-reduction",
		"initialize-central-path",
		"solve-electrical-direction",
		"descent-step",
		"solve-centering-direction",
		"centering-step",
		"extract-fractional-flow",
		"round-integral-flow",
		"check-certificate",
		"optimal",
	];
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"target_value",
				"mu",
				"duality_gap",
				"centrality",
				"congestion_l4",
				"step_size",
				"electrical_energy",
				"b_matching_nodes",
				"b_matching_edges",
				"working_nodes",
				"working_edges",
				"nodes",
				"edges",
			],
			["active_working_edge"],
		) ||
		!stages.includes(
			value.stage as FlowInteriorPointMaxFlowOverlayV1["stage"],
		) ||
		![
			value.target_value,
			value.b_matching_nodes,
			value.b_matching_edges,
			value.working_nodes,
			value.working_edges,
		].every((count) => typeof count === "string" && canonicalU64.test(count)) ||
		(value.active_working_edge !== undefined &&
			(typeof value.active_working_edge !== "string" ||
				!canonicalU64.test(value.active_working_edge))) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges)
	) {
		throw new Error("Flow scene contains an invalid interior-point overlay");
	}
	const magnitudes = {
		mu: decodeFiniteDecimal(value.mu),
		duality_gap: decodeFiniteDecimal(value.duality_gap),
		centrality: decodeFiniteDecimal(value.centrality),
		congestion_l4: decodeFiniteDecimal(value.congestion_l4),
		step_size: decodeFiniteDecimal(value.step_size),
		electrical_energy: decodeFiniteDecimal(value.electrical_energy),
	};
	if (
		Object.values(magnitudes).some((magnitude) => Number(magnitude) < 0) ||
		Number(magnitudes.step_size) > 0.5 + 1e-8
	) {
		throw new Error("Flow scene interior-point magnitudes are invalid");
	}
	const nodes = value.nodes.map((node): FlowInteriorPointNodeStateV1 => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, ["node_id", "potential", "target_source_side"]) ||
			typeof node.node_id !== "string" ||
			typeof node.target_source_side !== "boolean"
		) {
			throw new Error("Flow scene contains an invalid interior-point node");
		}
		return {
			node_id: node.node_id,
			potential: decodeFiniteDecimal(node.potential),
			target_source_side: node.target_source_side,
		};
	});
	const edgeStates = value.edges.map((edge): FlowInteriorPointEdgeStateV1 => {
		if (
			!isRecord(edge) ||
			!hasExactKeys(
				edge,
				[
					"edge_id",
					"fractional_flow",
					"electrical_current",
					"slack",
					"measure",
					"resistance",
					"congestion",
					"normalized_away",
				],
				["final_flow"],
			) ||
			typeof edge.edge_id !== "string" ||
			typeof edge.normalized_away !== "boolean" ||
			(edge.final_flow !== undefined &&
				(typeof edge.final_flow !== "string" ||
					!canonicalU64.test(edge.final_flow)))
		) {
			throw new Error("Flow scene contains an invalid interior-point edge");
		}
		const decoded = {
			edge_id: edge.edge_id,
			fractional_flow: decodeFiniteDecimal(edge.fractional_flow),
			electrical_current: decodeFiniteDecimal(edge.electrical_current),
			slack: decodeFiniteDecimal(edge.slack),
			measure: decodeFiniteDecimal(edge.measure),
			resistance: decodeFiniteDecimal(edge.resistance),
			congestion: decodeFiniteDecimal(edge.congestion),
			normalized_away: edge.normalized_away,
			...(edge.final_flow === undefined ? {} : { final_flow: edge.final_flow }),
		};
		if (
			[
				decoded.fractional_flow,
				decoded.slack,
				decoded.measure,
				decoded.resistance,
				decoded.congestion,
			].some((magnitude) => Number(magnitude) < 0)
		) {
			throw new Error("Flow scene interior-point edge magnitudes are invalid");
		}
		return decoded;
	});
	return {
		stage: value.stage as FlowInteriorPointMaxFlowOverlayV1["stage"],
		target_value: value.target_value as string,
		...magnitudes,
		b_matching_nodes: value.b_matching_nodes as string,
		b_matching_edges: value.b_matching_edges as string,
		working_nodes: value.working_nodes as string,
		working_edges: value.working_edges as string,
		...(value.active_working_edge === undefined
			? {}
			: { active_working_edge: value.active_working_edge as string }),
		nodes,
		edges: edgeStates,
	};
}

function decodeMinimumRatioCycleOverlay(
	value: unknown,
): FlowMinimumRatioCycleOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowMinimumRatioCycleOverlayV1["stage"][] = [
		"ready",
		"map-gradient-length",
		"build-spanning-forest",
		"inspect-vector",
		"evaluate-cycle",
		"update-best",
		"verify-cycle-space",
		"check-exhaustive-oracle",
		"complete",
	];
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"selected_edge_count",
				"maximum_absolute_balance",
				"enumerated_vectors",
				"simple_cycles",
				"fundamental_cycles",
				"nodes",
				"edges",
			],
			["candidate_ratio", "best_ratio"],
		) ||
		!stages.includes(value.stage as FlowMinimumRatioCycleOverlayV1["stage"]) ||
		![
			value.selected_edge_count,
			value.maximum_absolute_balance,
			value.enumerated_vectors,
			value.simple_cycles,
			value.fundamental_cycles,
		].every((count) => typeof count === "string" && canonicalU64.test(count)) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges)
	) {
		throw new Error(
			"Flow scene contains an invalid minimum-ratio-cycle overlay",
		);
	}
	const nodes = value.nodes.map((node): FlowMinimumRatioCycleNodeStateV1 => {
		if (
			!isRecord(node) ||
			!hasExactKeys(
				node,
				[
					"node_id",
					"component",
					"depth",
					"candidate_balance",
					"on_candidate",
					"on_selected",
				],
				["parent_node_id"],
			) ||
			typeof node.node_id !== "string" ||
			typeof node.component !== "string" ||
			!canonicalU64.test(node.component) ||
			typeof node.depth !== "string" ||
			!canonicalU64.test(node.depth) ||
			typeof node.candidate_balance !== "string" ||
			!canonicalI64.test(node.candidate_balance) ||
			typeof node.on_candidate !== "boolean" ||
			typeof node.on_selected !== "boolean" ||
			(node.parent_node_id !== undefined &&
				typeof node.parent_node_id !== "string")
		) {
			throw new Error(
				"Flow scene contains an invalid minimum-ratio-cycle node",
			);
		}
		return {
			node_id: node.node_id,
			component: node.component,
			...(node.parent_node_id === undefined
				? {}
				: { parent_node_id: node.parent_node_id }),
			depth: node.depth,
			candidate_balance: node.candidate_balance,
			on_candidate: node.on_candidate,
			on_selected: node.on_selected,
		};
	});
	const edges = value.edges.map((edge): FlowMinimumRatioCycleEdgeStateV1 => {
		if (
			!isRecord(edge) ||
			!hasExactKeys(edge, [
				"edge_id",
				"gradient",
				"length",
				"tree_edge",
				"candidate_sign",
				"selected_sign",
				"numerator_contribution",
				"denominator_contribution",
			]) ||
			typeof edge.edge_id !== "string" ||
			typeof edge.gradient !== "string" ||
			!canonicalI64.test(edge.gradient) ||
			typeof edge.length !== "string" ||
			!canonicalU64.test(edge.length) ||
			edge.length === "0" ||
			typeof edge.tree_edge !== "boolean" ||
			!["-1", "0", "1"].includes(edge.candidate_sign as string) ||
			!["-1", "0", "1"].includes(edge.selected_sign as string) ||
			typeof edge.numerator_contribution !== "string" ||
			!canonicalI128.test(edge.numerator_contribution) ||
			typeof edge.denominator_contribution !== "string" ||
			!canonicalU64.test(edge.denominator_contribution)
		) {
			throw new Error(
				"Flow scene contains an invalid minimum-ratio-cycle edge",
			);
		}
		return {
			edge_id: edge.edge_id,
			gradient: edge.gradient,
			length: edge.length,
			tree_edge: edge.tree_edge,
			candidate_sign: edge.candidate_sign as "-1" | "0" | "1",
			selected_sign: edge.selected_sign as "-1" | "0" | "1",
			numerator_contribution: edge.numerator_contribution,
			denominator_contribution: edge.denominator_contribution,
		};
	});
	return {
		stage: value.stage as FlowMinimumRatioCycleOverlayV1["stage"],
		...(value.candidate_ratio === undefined
			? {}
			: { candidate_ratio: decodeRational(value.candidate_ratio) }),
		...(value.best_ratio === undefined
			? {}
			: { best_ratio: decodeRational(value.best_ratio) }),
		selected_edge_count: value.selected_edge_count as string,
		maximum_absolute_balance: value.maximum_absolute_balance as string,
		enumerated_vectors: value.enumerated_vectors as string,
		simple_cycles: value.simple_cycles as string,
		fundamental_cycles: value.fundamental_cycles as string,
		nodes,
		edges,
	};
}

function decodeMinimumRatioCycleMcfOverlay(
	value: unknown,
): FlowMinimumRatioCycleMcfOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowMinimumRatioCycleMcfOverlayV1["stage"][] = [
		"ready",
		"enumerate-feasible-set",
		"contract-fixed-face",
		"initialize-strict-interior",
		"evaluate-potential",
		"map-gradient-length",
		"build-spanning-forest",
		"inspect-vector",
		"evaluate-cycle",
		"update-best",
		"verify-cycle-space",
		"apply-source-step",
		"measure-potential-decrease",
		"check-dfs-oracle",
		"complete",
	];
	const decimals = [
		"alpha",
		"initial_cost",
		"current_cost",
		"cost_gap",
		"potential_before",
		"current_potential",
		"kappa",
		"eta",
		"weighted_step_norm",
		"potential_decrease",
		"guaranteed_decrease",
	] as const;
	const counts = [
		"selected_edge_count",
		"maximum_absolute_balance",
		"feasible_flows",
		"enumerated_vectors",
		"simple_cycles",
		"fundamental_cycles",
	] as const;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"alpha",
				"optimum_cost",
				"initial_cost",
				"current_cost",
				"cost_gap",
				"potential_before",
				"current_potential",
				"kappa",
				"eta",
				"weighted_step_norm",
				"potential_decrease",
				"guaranteed_decrease",
				"stationary",
				"selected_edge_count",
				"maximum_absolute_balance",
				"feasible_flows",
				"enumerated_vectors",
				"simple_cycles",
				"fundamental_cycles",
				"nodes",
				"edges",
			],
			["candidate_ratio", "best_ratio"],
		) ||
		!stages.includes(
			value.stage as FlowMinimumRatioCycleMcfOverlayV1["stage"],
		) ||
		typeof value.optimum_cost !== "string" ||
		!canonicalI128.test(value.optimum_cost) ||
		!decimals.every((key) => {
			try {
				decodeFiniteDecimal(value[key]);
				return true;
			} catch {
				return false;
			}
		}) ||
		!counts.every(
			(key) => typeof value[key] === "string" && canonicalU64.test(value[key]),
		) ||
		typeof value.stationary !== "boolean" ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges)
	) {
		throw new Error(
			"Flow scene contains an invalid minimum-ratio-cycle MCF overlay",
		);
	}
	const optionalDecimal = (raw: unknown): string | undefined =>
		raw === undefined ? undefined : decodeFiniteDecimal(raw);
	const candidateRatio = optionalDecimal(value.candidate_ratio);
	const bestRatio = optionalDecimal(value.best_ratio);
	const nodes = value.nodes.map((node): FlowMinimumRatioCycleMcfNodeStateV1 => {
		if (
			!isRecord(node) ||
			!hasExactKeys(
				node,
				[
					"node_id",
					"component",
					"depth",
					"candidate_balance",
					"on_candidate",
					"on_selected",
				],
				["parent_node_id"],
			) ||
			typeof node.node_id !== "string" ||
			typeof node.component !== "string" ||
			!canonicalU64.test(node.component) ||
			typeof node.depth !== "string" ||
			!canonicalU64.test(node.depth) ||
			typeof node.candidate_balance !== "string" ||
			!canonicalI64.test(node.candidate_balance) ||
			typeof node.on_candidate !== "boolean" ||
			typeof node.on_selected !== "boolean" ||
			(node.parent_node_id !== undefined &&
				typeof node.parent_node_id !== "string")
		) {
			throw new Error(
				"Flow scene contains an invalid minimum-ratio-cycle MCF node",
			);
		}
		return {
			node_id: node.node_id,
			component: node.component,
			...(node.parent_node_id === undefined
				? {}
				: { parent_node_id: node.parent_node_id }),
			depth: node.depth,
			candidate_balance: node.candidate_balance,
			on_candidate: node.on_candidate,
			on_selected: node.on_selected,
		};
	});
	const edges = value.edges.map((edge): FlowMinimumRatioCycleMcfEdgeStateV1 => {
		if (
			!isRecord(edge) ||
			!hasExactKeys(edge, [
				"edge_id",
				"fixed_on_face",
				"initial_flow",
				"updated_flow",
				"lower_slack",
				"upper_slack",
				"gradient",
				"length",
				"tree_edge",
				"candidate_sign",
				"selected_sign",
				"numerator_contribution",
				"denominator_contribution",
			]) ||
			typeof edge.edge_id !== "string" ||
			typeof edge.fixed_on_face !== "boolean" ||
			typeof edge.tree_edge !== "boolean" ||
			![
				"initial_flow",
				"updated_flow",
				"lower_slack",
				"upper_slack",
				"gradient",
				"length",
				"numerator_contribution",
				"denominator_contribution",
			].every((key) => {
				try {
					decodeFiniteDecimal(edge[key]);
					return true;
				} catch {
					return false;
				}
			}) ||
			!["-1", "0", "1"].includes(edge.candidate_sign as string) ||
			!["-1", "0", "1"].includes(edge.selected_sign as string)
		) {
			throw new Error(
				"Flow scene contains an invalid minimum-ratio-cycle MCF edge",
			);
		}
		return {
			edge_id: edge.edge_id,
			fixed_on_face: edge.fixed_on_face,
			initial_flow: edge.initial_flow as string,
			updated_flow: edge.updated_flow as string,
			lower_slack: edge.lower_slack as string,
			upper_slack: edge.upper_slack as string,
			gradient: edge.gradient as string,
			length: edge.length as string,
			tree_edge: edge.tree_edge,
			candidate_sign: edge.candidate_sign as "-1" | "0" | "1",
			selected_sign: edge.selected_sign as "-1" | "0" | "1",
			numerator_contribution: edge.numerator_contribution as string,
			denominator_contribution: edge.denominator_contribution as string,
		};
	});
	return {
		stage: value.stage as FlowMinimumRatioCycleMcfOverlayV1["stage"],
		alpha: value.alpha as string,
		optimum_cost: value.optimum_cost,
		initial_cost: value.initial_cost as string,
		current_cost: value.current_cost as string,
		cost_gap: value.cost_gap as string,
		potential_before: value.potential_before as string,
		current_potential: value.current_potential as string,
		...(candidateRatio === undefined
			? {}
			: { candidate_ratio: candidateRatio }),
		...(bestRatio === undefined ? {} : { best_ratio: bestRatio }),
		kappa: value.kappa as string,
		eta: value.eta as string,
		weighted_step_norm: value.weighted_step_norm as string,
		potential_decrease: value.potential_decrease as string,
		guaranteed_decrease: value.guaranteed_decrease as string,
		stationary: value.stationary,
		selected_edge_count: value.selected_edge_count as string,
		maximum_absolute_balance: value.maximum_absolute_balance as string,
		feasible_flows: value.feasible_flows as string,
		enumerated_vectors: value.enumerated_vectors as string,
		simple_cycles: value.simple_cycles as string,
		fundamental_cycles: value.fundamental_cycles as string,
		nodes,
		edges,
	};
}

function decodeWeightedAugmentingPathsOverlay(
	value: unknown,
): FlowWeightedAugmentingPathsOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowWeightedAugmentingPathsOverlayV1["stage"][] = [
		"ready",
		"begin-capacity-phase",
		"build-hierarchy",
		"certify-expansion",
		"assign-weights",
		"relabel-sweep",
		"augment-path",
		"finish-weighted-round",
		"finish-capacity-phase",
		"check-certificate",
		"optimal",
	];
	const counts = [
		"phase",
		"phase_count",
		"capacity_bit",
		"round",
		"height",
		"phi_numerator",
		"phi_denominator",
		"active_bottleneck",
		"hierarchy_cuts",
		"relabel_jumps",
		"augmentations",
		"augmented_units",
	] as const;
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"stage",
			...counts,
			"nodes",
			"edges",
			"residual_arcs",
			"active_path",
		]) ||
		!stages.includes(
			value.stage as FlowWeightedAugmentingPathsOverlayV1["stage"],
		) ||
		!counts.every(
			(key) => typeof value[key] === "string" && canonicalU64.test(value[key]),
		) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges) ||
		!Array.isArray(value.residual_arcs) ||
		!Array.isArray(value.active_path)
	) {
		throw new Error(
			"Flow scene contains an invalid weighted augmenting-path overlay",
		);
	}
	const nodes = value.nodes.map((node): FlowWeightedAugmentingNodeStateV1 => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, [
				"node_id",
				"component",
				"order",
				"label",
				"alive",
				"expansion_witness_side",
				"source_side",
			]) ||
			typeof node.node_id !== "string" ||
			typeof node.component !== "string" ||
			!canonicalU64.test(node.component) ||
			typeof node.order !== "string" ||
			!canonicalU64.test(node.order) ||
			typeof node.label !== "string" ||
			!canonicalU64.test(node.label) ||
			typeof node.alive !== "boolean" ||
			typeof node.expansion_witness_side !== "boolean" ||
			typeof node.source_side !== "boolean"
		) {
			throw new Error(
				"Flow scene contains an invalid weighted augmenting-path node",
			);
		}
		return {
			node_id: node.node_id,
			component: node.component,
			order: node.order,
			label: node.label,
			alive: node.alive,
			expansion_witness_side: node.expansion_witness_side,
			source_side: node.source_side,
		};
	});
	const edges = value.edges.map((edge): FlowWeightedAugmentingEdgeStateV1 => {
		if (
			!isRecord(edge) ||
			!hasExactKeys(edge, ["edge_id", "scaled_capacity", "flow"]) ||
			typeof edge.edge_id !== "string" ||
			typeof edge.scaled_capacity !== "string" ||
			!canonicalU64.test(edge.scaled_capacity) ||
			typeof edge.flow !== "string" ||
			!canonicalU64.test(edge.flow)
		) {
			throw new Error(
				"Flow scene contains an invalid weighted augmenting-path edge",
			);
		}
		return {
			edge_id: edge.edge_id,
			scaled_capacity: edge.scaled_capacity,
			flow: edge.flow,
		};
	});
	const residualArcs = value.residual_arcs.map(
		(arc): FlowWeightedAugmentingResidualArcStateV1 => {
			if (
				!isRecord(arc) ||
				!hasExactKeys(
					arc,
					[
						"edge_id",
						"direction",
						"from",
						"to",
						"capacity",
						"weight",
						"admissible",
						"active",
					],
					["hierarchy_kind"],
				) ||
				typeof arc.edge_id !== "string" ||
				!(["forward", "reverse"] as const).includes(
					arc.direction as "forward" | "reverse",
				) ||
				typeof arc.from !== "string" ||
				typeof arc.to !== "string" ||
				typeof arc.capacity !== "string" ||
				!canonicalU64.test(arc.capacity) ||
				typeof arc.weight !== "string" ||
				!canonicalU64.test(arc.weight) ||
				typeof arc.admissible !== "boolean" ||
				typeof arc.active !== "boolean" ||
				(arc.hierarchy_kind !== undefined &&
					arc.hierarchy_kind !== "dag" &&
					arc.hierarchy_kind !== "expanding")
			) {
				throw new Error(
					"Flow scene contains an invalid weighted augmenting-path residual arc",
				);
			}
			return {
				edge_id: arc.edge_id,
				direction: arc.direction as "forward" | "reverse",
				from: arc.from,
				to: arc.to,
				capacity: arc.capacity,
				...(arc.hierarchy_kind === undefined
					? {}
					: { hierarchy_kind: arc.hierarchy_kind as "dag" | "expanding" }),
				weight: arc.weight,
				admissible: arc.admissible,
				active: arc.active,
			};
		},
	);
	const activePath = value.active_path.map(
		(reference): FlowResidualArcRefV1 => {
			if (
				!isRecord(reference) ||
				!hasExactKeys(reference, ["edge_id", "direction"]) ||
				typeof reference.edge_id !== "string" ||
				(reference.direction !== "forward" && reference.direction !== "reverse")
			) {
				throw new Error(
					"Flow scene contains an invalid weighted augmenting-path path reference",
				);
			}
			return {
				edge_id: reference.edge_id,
				direction: reference.direction,
			};
		},
	);
	return {
		stage: value.stage as FlowWeightedAugmentingPathsOverlayV1["stage"],
		phase: value.phase as string,
		phase_count: value.phase_count as string,
		capacity_bit: value.capacity_bit as string,
		round: value.round as string,
		height: value.height as string,
		phi_numerator: value.phi_numerator as string,
		phi_denominator: value.phi_denominator as string,
		active_bottleneck: value.active_bottleneck as string,
		hierarchy_cuts: value.hierarchy_cuts as string,
		relabel_jumps: value.relabel_jumps as string,
		augmentations: value.augmentations as string,
		augmented_units: value.augmented_units as string,
		nodes,
		edges,
		residual_arcs: residualArcs,
		active_path: activePath,
	};
}

function decodeWeightedPushRelabelShortcutOverlay(
	value: unknown,
): FlowWeightedPushRelabelShortcutOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowWeightedPushRelabelShortcutOverlayV1["stage"][] = [
		"ready",
		"build-weak-hierarchy",
		"build-shortcut-graph",
		"assign-weights",
		"initialize-demand",
		"relabel-sweep",
		"relabel-checkpoint",
		"inspect-primitive-arc-checkpoint",
		"augment-path",
		"measure-short-flow",
		"compute-distance-layers",
		"select-sparse-cut",
		"completion-inspect-primitive-arc-checkpoint",
		"completion-relabel-checkpoint",
		"completion-augment-path",
		"completion-residual-round",
		"complete-residual-rounds",
		"check-certificate",
		"optimal",
	];
	const counts = [
		"hierarchy_levels",
		"psi_numerator",
		"psi_denominator",
		"height",
		"demand",
		"routed",
		"weighted_length",
		"weighted_length_units",
		"sparse_cut_level",
		"sparse_cut_capacity",
		"active_bottleneck",
		"relabel_steps",
		"augmentations",
		"shortcut_traversals",
		"residual_rounds",
		"completion_relabel_steps",
		"completion_augmentations",
	] as const;
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"stage",
			...counts,
			"nodes",
			"edges",
			"residual_arcs",
			"active_path",
			"inspected_arcs",
			"active_relabel_nodes",
		]) ||
		!stages.includes(
			value.stage as FlowWeightedPushRelabelShortcutOverlayV1["stage"],
		) ||
		!counts.every(
			(key) => typeof value[key] === "string" && canonicalU64.test(value[key]),
		) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges) ||
		!Array.isArray(value.residual_arcs) ||
		!Array.isArray(value.active_path) ||
		!Array.isArray(value.inspected_arcs) ||
		!Array.isArray(value.active_relabel_nodes) ||
		!value.active_relabel_nodes.every(
			(nodeId) => typeof nodeId === "string" && nodeId.length > 0,
		)
	) {
		throw new Error(
			"Flow scene contains an invalid weighted push-relabel shortcut overlay",
		);
	}
	const nodes = value.nodes.map(
		(node): FlowWeightedPushRelabelShortcutNodeStateV1 => {
			if (
				!isRecord(node) ||
				!hasExactKeys(node, [
					"node_id",
					"original",
					"component",
					"order",
					"label",
					"alive",
					"sparse_cut_side",
					"source_side",
				]) ||
				typeof node.node_id !== "string" ||
				typeof node.original !== "boolean" ||
				typeof node.component !== "string" ||
				!canonicalU64.test(node.component) ||
				typeof node.order !== "string" ||
				!canonicalU64.test(node.order) ||
				typeof node.label !== "string" ||
				!canonicalU64.test(node.label) ||
				typeof node.alive !== "boolean" ||
				typeof node.sparse_cut_side !== "boolean" ||
				typeof node.source_side !== "boolean"
			) {
				throw new Error(
					"Flow scene contains an invalid weighted push-relabel shortcut node",
				);
			}
			return {
				node_id: node.node_id,
				original: node.original,
				component: node.component,
				order: node.order,
				label: node.label,
				alive: node.alive,
				sparse_cut_side: node.sparse_cut_side,
				source_side: node.source_side,
			};
		},
	);
	const edges = value.edges.map(
		(edge): FlowWeightedPushRelabelShortcutEdgeStateV1 => {
			if (
				!isRecord(edge) ||
				!hasExactKeys(
					edge,
					["edge_id", "kind", "from", "to", "capacity", "flow", "weight"],
					["shortcut_component"],
				) ||
				typeof edge.edge_id !== "string" ||
				(edge.kind !== "original" && edge.kind !== "shortcut") ||
				typeof edge.from !== "string" ||
				typeof edge.to !== "string" ||
				typeof edge.capacity !== "string" ||
				!canonicalU64.test(edge.capacity) ||
				typeof edge.flow !== "string" ||
				!canonicalU64.test(edge.flow) ||
				typeof edge.weight !== "string" ||
				!canonicalU64.test(edge.weight) ||
				(edge.shortcut_component !== undefined &&
					(typeof edge.shortcut_component !== "string" ||
						!canonicalU64.test(edge.shortcut_component)))
			) {
				throw new Error(
					"Flow scene contains an invalid weighted push-relabel shortcut edge",
				);
			}
			return {
				edge_id: edge.edge_id,
				kind: edge.kind,
				from: edge.from,
				to: edge.to,
				capacity: edge.capacity,
				flow: edge.flow,
				weight: edge.weight,
				...(edge.shortcut_component === undefined
					? {}
					: { shortcut_component: edge.shortcut_component }),
			};
		},
	);
	const residualArcs = value.residual_arcs.map(
		(arc): FlowWeightedPushRelabelShortcutResidualArcStateV1 => {
			if (
				!isRecord(arc) ||
				!hasExactKeys(arc, [
					"edge_id",
					"direction",
					"from",
					"to",
					"capacity",
					"weight",
					"admissible",
					"active",
				]) ||
				typeof arc.edge_id !== "string" ||
				(arc.direction !== "forward" && arc.direction !== "reverse") ||
				typeof arc.from !== "string" ||
				typeof arc.to !== "string" ||
				typeof arc.capacity !== "string" ||
				!canonicalU64.test(arc.capacity) ||
				typeof arc.weight !== "string" ||
				!canonicalU64.test(arc.weight) ||
				typeof arc.admissible !== "boolean" ||
				typeof arc.active !== "boolean"
			) {
				throw new Error(
					"Flow scene contains an invalid weighted push-relabel shortcut residual arc",
				);
			}
			return {
				edge_id: arc.edge_id,
				direction: arc.direction,
				from: arc.from,
				to: arc.to,
				capacity: arc.capacity,
				weight: arc.weight,
				admissible: arc.admissible,
				active: arc.active,
			};
		},
	);
	const activePath = value.active_path.map(
		(reference): FlowWeightedPushRelabelShortcutArcRefV1 => {
			if (
				!isRecord(reference) ||
				!hasExactKeys(reference, ["edge_id", "direction"]) ||
				typeof reference.edge_id !== "string" ||
				(reference.direction !== "forward" && reference.direction !== "reverse")
			) {
				throw new Error(
					"Flow scene contains an invalid weighted push-relabel path reference",
				);
			}
			return {
				edge_id: reference.edge_id,
				direction: reference.direction,
			};
		},
	);
	const inspectedArcs = value.inspected_arcs.map(
		(reference): FlowWeightedPushRelabelShortcutArcRefV1 => {
			if (
				!isRecord(reference) ||
				!hasExactKeys(reference, ["edge_id", "direction"]) ||
				typeof reference.edge_id !== "string" ||
				(reference.direction !== "forward" && reference.direction !== "reverse")
			) {
				throw new Error(
					"Flow scene contains an invalid weighted push-relabel inspected-arc reference",
				);
			}
			return {
				edge_id: reference.edge_id,
				direction: reference.direction,
			};
		},
	);
	return {
		stage: value.stage as FlowWeightedPushRelabelShortcutOverlayV1["stage"],
		hierarchy_levels: value.hierarchy_levels as string,
		psi_numerator: value.psi_numerator as string,
		psi_denominator: value.psi_denominator as string,
		height: value.height as string,
		demand: value.demand as string,
		routed: value.routed as string,
		weighted_length: value.weighted_length as string,
		weighted_length_units: value.weighted_length_units as string,
		sparse_cut_level: value.sparse_cut_level as string,
		sparse_cut_capacity: value.sparse_cut_capacity as string,
		active_bottleneck: value.active_bottleneck as string,
		relabel_steps: value.relabel_steps as string,
		augmentations: value.augmentations as string,
		shortcut_traversals: value.shortcut_traversals as string,
		residual_rounds: value.residual_rounds as string,
		completion_relabel_steps: value.completion_relabel_steps as string,
		completion_augmentations: value.completion_augmentations as string,
		nodes,
		edges,
		residual_arcs: residualArcs,
		active_path: activePath,
		inspected_arcs: inspectedArcs,
		active_relabel_nodes: value.active_relabel_nodes as string[],
	};
}

function decodeRandomizedAlmostLinearMcfOverlay(
	value: unknown,
): FlowRandomizedAlmostLinearMcfOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowRandomizedAlmostLinearMcfOverlayV1["stage"][] = [
		"ready",
		"inspect-feasible-assignment",
		"enumerate-feasible-set",
		"sample-isolation-costs",
		"select-isolated-optimum",
		"initialize-relative-interior",
		"inspect-oracle-vector",
		"build-forest-pool",
		"sample-tree-chain",
		"refresh-gradient-length",
		"query-minimum-ratio-cycle",
		"potential-reduction-step",
		"detect-changed-coordinates",
		"rebuild-tree-chain",
		"construct-final-point",
		"round-nearest-integer",
		"check-certificate",
		"optimal",
	];
	const decimals = [
		"alpha",
		"epsilon",
		"kappa",
		"eta",
		"initial_cost",
		"current_cost",
		"potential",
	] as const;
	const unsigned = [
		"seed",
		"isolation_attempt",
		"isolation_scale",
		"failure_numerator",
		"failure_denominator",
		"forest_pool_size",
		"feasible_flows",
		"detected_coordinates",
		"rebuilds",
	] as const;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"seed",
				"alpha",
				"epsilon",
				"kappa",
				"eta",
				"initial_cost",
				"current_cost",
				"optimum_cost",
				"isolated_optimum_cost",
				"potential",
				"isolation_attempt",
				"isolation_scale",
				"failure_numerator",
				"failure_denominator",
				"forest_pool_size",
				"final_point_threshold",
				"exact_recovery",
				"feasible_flows",
				"detected_coordinates",
				"rebuilds",
				"nodes",
				"edges",
			],
			[
				"assignment_cursor",
				"assignment_serial",
				"oracle_vector_serial",
				"minimum_ratio",
				"sampled_forest_index",
				"final_point_gap",
				"final_point_mix",
			],
		) ||
		!stages.includes(
			value.stage as FlowRandomizedAlmostLinearMcfOverlayV1["stage"],
		) ||
		(value.assignment_cursor !== undefined &&
			typeof value.assignment_cursor !== "string") ||
		(value.assignment_serial !== undefined &&
			(typeof value.assignment_serial !== "string" ||
				!canonicalU64.test(value.assignment_serial))) ||
		(value.oracle_vector_serial !== undefined &&
			(typeof value.oracle_vector_serial !== "string" ||
				!canonicalU64.test(value.oracle_vector_serial))) ||
		!unsigned.every(
			(key) => typeof value[key] === "string" && canonicalU64.test(value[key]),
		) ||
		typeof value.optimum_cost !== "string" ||
		!canonicalI128.test(value.optimum_cost) ||
		typeof value.isolated_optimum_cost !== "string" ||
		!canonicalI128.test(value.isolated_optimum_cost) ||
		!decimals.every((key) => {
			try {
				decodeFiniteDecimal(value[key]);
				return true;
			} catch {
				return false;
			}
		}) ||
		(value.minimum_ratio !== undefined &&
			(() => {
				try {
					decodeFiniteDecimal(value.minimum_ratio);
					return false;
				} catch {
					return true;
				}
			})()) ||
		(value.sampled_forest_index !== undefined &&
			(typeof value.sampled_forest_index !== "string" ||
				!canonicalU64.test(value.sampled_forest_index))) ||
		typeof value.exact_recovery !== "boolean" ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges)
	) {
		throw new Error(
			"Flow scene contains an invalid randomized almost-linear MCF overlay",
		);
	}
	const nodes = value.nodes.map(
		(node): FlowRandomizedAlmostLinearMcfNodeStateV1 => {
			if (
				!isRecord(node) ||
				!hasExactKeys(
					node,
					[
						"node_id",
						"required_divergence",
						"component",
						"depth",
						"on_selected_cycle",
					],
					["parent_node_id"],
				) ||
				typeof node.node_id !== "string" ||
				typeof node.required_divergence !== "string" ||
				!canonicalI128.test(node.required_divergence) ||
				typeof node.component !== "string" ||
				!canonicalU64.test(node.component) ||
				typeof node.depth !== "string" ||
				!canonicalU64.test(node.depth) ||
				typeof node.on_selected_cycle !== "boolean" ||
				(node.parent_node_id !== undefined &&
					typeof node.parent_node_id !== "string")
			) {
				throw new Error(
					"Flow scene contains an invalid randomized almost-linear MCF node",
				);
			}
			return {
				node_id: node.node_id,
				required_divergence: node.required_divergence,
				component: node.component,
				...(node.parent_node_id === undefined
					? {}
					: { parent_node_id: node.parent_node_id }),
				depth: node.depth,
				on_selected_cycle: node.on_selected_cycle,
			};
		},
	);
	const edges = value.edges.map(
		(edge): FlowRandomizedAlmostLinearMcfEdgeStateV1 => {
			if (
				!isRecord(edge) ||
				!hasExactKeys(
					edge,
					[
						"edge_id",
						"fixed_on_face",
						"initial_flow",
						"current_flow",
						"stale_flow",
						"isolation_draw",
						"isolated_cost",
						"tree_edge",
						"candidate_sign",
						"selected_sign",
						"gradient",
						"length",
						"detected",
					],
					["final_point_flow", "final_flow", "isolated_optimum_flow"],
				) ||
				typeof edge.edge_id !== "string" ||
				typeof edge.fixed_on_face !== "boolean" ||
				typeof edge.tree_edge !== "boolean" ||
				typeof edge.detected !== "boolean" ||
				!["-1", "0", "1"].includes(edge.candidate_sign as string) ||
				!["-1", "0", "1"].includes(edge.selected_sign as string) ||
				![
					"initial_flow",
					"current_flow",
					"stale_flow",
					"gradient",
					"length",
				].every((key) => {
					try {
						decodeFiniteDecimal(edge[key]);
						return true;
					} catch {
						return false;
					}
				}) ||
				(edge.final_flow !== undefined &&
					(typeof edge.final_flow !== "string" ||
						!canonicalU64.test(edge.final_flow))) ||
				(edge.isolated_optimum_flow !== undefined &&
					(typeof edge.isolated_optimum_flow !== "string" ||
						!canonicalU64.test(edge.isolated_optimum_flow))) ||
				typeof edge.isolation_draw !== "string" ||
				!canonicalU64.test(edge.isolation_draw) ||
				typeof edge.isolated_cost !== "string" ||
				!canonicalI128.test(edge.isolated_cost)
			) {
				throw new Error(
					"Flow scene contains an invalid randomized almost-linear MCF edge",
				);
			}
			return {
				edge_id: edge.edge_id,
				fixed_on_face: edge.fixed_on_face,
				initial_flow: edge.initial_flow as string,
				current_flow: edge.current_flow as string,
				stale_flow: edge.stale_flow as string,
				...(edge.final_point_flow === undefined
					? {}
					: { final_point_flow: decodeRational(edge.final_point_flow) }),
				...(edge.final_flow === undefined
					? {}
					: { final_flow: edge.final_flow }),
				isolation_draw: edge.isolation_draw,
				isolated_cost: edge.isolated_cost,
				...(edge.isolated_optimum_flow === undefined
					? {}
					: { isolated_optimum_flow: edge.isolated_optimum_flow }),
				tree_edge: edge.tree_edge,
				candidate_sign: edge.candidate_sign as "-1" | "0" | "1",
				selected_sign: edge.selected_sign as "-1" | "0" | "1",
				gradient: edge.gradient as string,
				length: edge.length as string,
				detected: edge.detected,
			};
		},
	);
	const finalPointThreshold = decodeRational(value.final_point_threshold);
	const finalPointGap =
		value.final_point_gap === undefined
			? undefined
			: decodeRational(value.final_point_gap);
	const finalPointMix =
		value.final_point_mix === undefined
			? undefined
			: decodeRational(value.final_point_mix);
	return {
		stage: value.stage as FlowRandomizedAlmostLinearMcfOverlayV1["stage"],
		...(value.assignment_cursor === undefined
			? {}
			: { assignment_cursor: value.assignment_cursor }),
		...(value.assignment_serial === undefined
			? {}
			: { assignment_serial: value.assignment_serial as string }),
		...(value.oracle_vector_serial === undefined
			? {}
			: { oracle_vector_serial: value.oracle_vector_serial as string }),
		seed: value.seed as string,
		alpha: value.alpha as string,
		epsilon: value.epsilon as string,
		kappa: value.kappa as string,
		eta: value.eta as string,
		initial_cost: value.initial_cost as string,
		current_cost: value.current_cost as string,
		optimum_cost: value.optimum_cost,
		isolated_optimum_cost: value.isolated_optimum_cost,
		potential: value.potential as string,
		...(value.minimum_ratio === undefined
			? {}
			: { minimum_ratio: value.minimum_ratio as string }),
		isolation_attempt: value.isolation_attempt as string,
		isolation_scale: value.isolation_scale as string,
		failure_numerator: value.failure_numerator as string,
		failure_denominator: value.failure_denominator as string,
		forest_pool_size: value.forest_pool_size as string,
		...(value.sampled_forest_index === undefined
			? {}
			: { sampled_forest_index: value.sampled_forest_index as string }),
		...(finalPointGap === undefined ? {} : { final_point_gap: finalPointGap }),
		final_point_threshold: finalPointThreshold,
		...(finalPointMix === undefined ? {} : { final_point_mix: finalPointMix }),
		exact_recovery: value.exact_recovery,
		feasible_flows: value.feasible_flows as string,
		detected_coordinates: value.detected_coordinates as string,
		rebuilds: value.rebuilds as string,
		nodes,
		edges,
	};
}

function decodeFlowFrameworkMcfOverlay(
	value: unknown,
): FlowFrameworkMcfOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowFrameworkMcfOverlayV1["stage"][] = [
		"initialize-source-point",
		"periodic-reinitialize",
		"detect",
		"query-minimum-ratio-cycle",
		"source-progress",
		"round-fractional-flow",
		"check-certificate",
		"optimal",
	];
	const dynamicOperations: readonly FlowFrameworkMcfDynamicOperationV1[] = [
		"topology-stage-applied",
		"periodic-rebuilt",
		"cycle-queried-accepted",
		"cycle-queried-rejected",
		"level-shifted",
		"flow-applied",
		"query-returned",
		"detect-returned",
		"completed",
	];
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"iteration",
				"reinitialized",
				"potential_before",
				"potential_after",
				"gap_before",
				"gap_after",
				"exact_gap_before",
				"exact_gap_after",
				"stopping_gap",
				"accepted_ratio",
				"target_progress",
				"levels",
				"edges",
			],
			[
				"dynamic_operation",
				"dynamic_operation_serial",
				"termination",
				"optimum_cost",
				"final_point_nodes",
				"final_point_edges",
			],
		) ||
		!stages.includes(value.stage as FlowFrameworkMcfOverlayV1["stage"]) ||
		(value.dynamic_operation !== undefined &&
			!dynamicOperations.includes(
				value.dynamic_operation as FlowFrameworkMcfDynamicOperationV1,
			)) ||
		(value.dynamic_operation_serial !== undefined &&
			(typeof value.dynamic_operation_serial !== "string" ||
				!canonicalU64.test(value.dynamic_operation_serial))) ||
		typeof value.iteration !== "string" ||
		!canonicalU64.test(value.iteration) ||
		typeof value.reinitialized !== "boolean" ||
		!["potential_before", "potential_after", "gap_before", "gap_after"].every(
			(key) => {
				try {
					decodeFiniteDecimal(value[key]);
					return true;
				} catch {
					return false;
				}
			},
		) ||
		(value.termination !== undefined &&
			value.termination !== "source-additive-half-gap") ||
		(value.optimum_cost !== undefined &&
			(typeof value.optimum_cost !== "string" ||
				!canonicalI128.test(value.optimum_cost))) ||
		(value.final_point_nodes !== undefined &&
			!Array.isArray(value.final_point_nodes)) ||
		(value.final_point_edges !== undefined &&
			!Array.isArray(value.final_point_edges)) ||
		!Array.isArray(value.levels) ||
		!Array.isArray(value.edges)
	) {
		throw new Error(
			"Flow scene contains an invalid Flow Framework MCF overlay",
		);
	}
	const acceptedRatio = decodeFlowFrameworkRational(value.accepted_ratio);
	const targetProgress = decodeFlowFrameworkRational(value.target_progress);
	const exactGapBefore = decodeFlowFrameworkRational(value.exact_gap_before);
	const exactGapAfter = decodeFlowFrameworkRational(value.exact_gap_after);
	const stoppingGap = decodeFlowFrameworkRational(value.stopping_gap);
	const levels = value.levels.map((level): FlowFrameworkMcfLevelStateV1 => {
		if (
			!isRecord(level) ||
			!hasExactKeys(level, ["level", "active_branch", "passes"]) ||
			!["level", "active_branch", "passes"].every(
				(key) =>
					typeof level[key] === "string" && canonicalU64.test(level[key]),
			)
		) {
			throw new Error(
				"Flow scene contains an invalid Flow Framework MCF level",
			);
		}
		return {
			level: level.level as string,
			active_branch: level.active_branch as string,
			passes: level.passes as string,
		};
	});
	const edges = value.edges.map((edge): FlowFrameworkMcfEdgeStateV1 => {
		if (
			!isRecord(edge) ||
			!hasExactKeys(edge, [
				"edge_id",
				"flow",
				"cycle_coefficient",
				"selected",
			]) ||
			typeof edge.edge_id !== "string" ||
			typeof edge.selected !== "boolean"
		) {
			throw new Error("Flow scene contains an invalid Flow Framework MCF edge");
		}
		return {
			edge_id: edge.edge_id,
			flow: decodeFlowFrameworkRational(edge.flow),
			cycle_coefficient: decodeFlowFrameworkRational(edge.cycle_coefficient),
			selected: edge.selected,
		};
	});
	const finalPointNodes = value.final_point_nodes?.map(
		(node): FlowFrameworkMcfFinalPointNodeV1 => {
			if (
				!isRecord(node) ||
				!hasExactKeys(node, ["node_id", "required_divergence"]) ||
				typeof node.node_id !== "string" ||
				node.node_id.length === 0 ||
				typeof node.required_divergence !== "string" ||
				!canonicalI128.test(node.required_divergence)
			) {
				throw new Error(
					"Flow scene contains an invalid Flow Framework MCF final-point node",
				);
			}
			return {
				node_id: node.node_id,
				required_divergence: node.required_divergence,
			};
		},
	);
	const finalPointEdges = value.final_point_edges?.map(
		(edge): FlowFrameworkMcfFinalPointEdgeV1 => {
			if (
				!isRecord(edge) ||
				!hasExactKeys(
					edge,
					[
						"edge_id",
						"from",
						"to",
						"lower",
						"capacity",
						"cost",
						"flow",
						"auxiliary",
					],
					["rounded_flow"],
				) ||
				!["edge_id", "from", "to"].every(
					(key) => typeof edge[key] === "string" && edge[key].length > 0,
				) ||
				!["lower", "capacity"].every(
					(key) =>
						typeof edge[key] === "string" && canonicalU64.test(edge[key]),
				) ||
				typeof edge.cost !== "string" ||
				!canonicalI64.test(edge.cost) ||
				typeof edge.auxiliary !== "boolean" ||
				(edge.rounded_flow !== undefined &&
					(typeof edge.rounded_flow !== "string" ||
						!canonicalU64.test(edge.rounded_flow)))
			) {
				throw new Error(
					"Flow scene contains an invalid Flow Framework MCF final-point edge",
				);
			}
			return {
				edge_id: edge.edge_id as string,
				from: edge.from as string,
				to: edge.to as string,
				lower: edge.lower as string,
				capacity: edge.capacity as string,
				cost: edge.cost,
				flow: decodeFlowFrameworkRational(edge.flow),
				auxiliary: edge.auxiliary,
				...(edge.rounded_flow === undefined
					? {}
					: { rounded_flow: edge.rounded_flow as string }),
			};
		},
	);
	return {
		stage: value.stage as FlowFrameworkMcfOverlayV1["stage"],
		...(value.dynamic_operation === undefined
			? {}
			: {
					dynamic_operation:
						value.dynamic_operation as FlowFrameworkMcfDynamicOperationV1,
				}),
		...(value.dynamic_operation_serial === undefined
			? {}
			: { dynamic_operation_serial: value.dynamic_operation_serial as string }),
		iteration: value.iteration,
		reinitialized: value.reinitialized,
		potential_before: value.potential_before as string,
		potential_after: value.potential_after as string,
		gap_before: value.gap_before as string,
		gap_after: value.gap_after as string,
		exact_gap_before: exactGapBefore,
		exact_gap_after: exactGapAfter,
		stopping_gap: stoppingGap,
		accepted_ratio: acceptedRatio,
		target_progress: targetProgress,
		...(value.termination === undefined
			? {}
			: { termination: value.termination as "source-additive-half-gap" }),
		...(value.optimum_cost === undefined
			? {}
			: { optimum_cost: value.optimum_cost as string }),
		...(finalPointNodes === undefined
			? {}
			: { final_point_nodes: finalPointNodes }),
		...(finalPointEdges === undefined
			? {}
			: { final_point_edges: finalPointEdges }),
		levels,
		edges,
	};
}

function decodeRandomizedAlmostLinearOverlay(
	value: unknown,
): FlowRandomizedAlmostLinearOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowRandomizedAlmostLinearOverlayV1["stage"][] = [
		"ready",
		"build-return-edge-reduction",
		"build-initial-point",
		"enumerate-forest-pool",
		"sample-tree-chain",
		"inspect-fundamental-cycle",
		"query-minimum-ratio-cycle",
		"sampling-failure",
		"potential-reduction-step",
		"detect-changed-coordinates",
		"rebuild-tree-chain",
		"inspect-feasible-assignment",
		"enumerate-feasible-set",
		"sample-isolation-costs",
		"select-isolated-optimum",
		"construct-final-point",
		"round-nearest-integer",
		"check-certificate",
		"optimal",
	];
	const counts = [
		"seed",
		"random_draws",
		"forest_pool_size",
		"sample_count",
		"iteration",
		"rebuild_epoch",
		"return_capacity",
		"return_tree_memberships",
		"return_isolation_draw",
		"artificial_edges",
		"isolation_scale",
		"isolation_attempt",
		"target_value",
	] as const;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				...counts,
				"alpha",
				"potential",
				"cost_gap",
				"miss_probability",
				"isolation_failure_probability",
				"return_flow",
				"return_gradient",
				"return_length",
				"active_return_tree_edge",
				"active_return_sign",
				"artificial_flow",
				"final_point_threshold",
				"nodes",
				"edges",
			],
			[
				"selected_ratio",
				"exact_pool_ratio",
				"final_point_return_flow",
				"final_return_flow",
				"final_artificial_flow",
				"isolated_objective",
				"final_point_gap",
				"final_point_mix",
			],
		) ||
		!stages.includes(
			value.stage as FlowRandomizedAlmostLinearOverlayV1["stage"],
		) ||
		!counts.every(
			(key) => typeof value[key] === "string" && canonicalU64.test(value[key]),
		) ||
		!isRecord(value.miss_probability) ||
		!hasExactKeys(value.miss_probability, ["numerator", "denominator"]) ||
		typeof value.miss_probability.numerator !== "string" ||
		!canonicalU64.test(value.miss_probability.numerator) ||
		typeof value.miss_probability.denominator !== "string" ||
		!canonicalU64.test(value.miss_probability.denominator) ||
		value.miss_probability.denominator === "0" ||
		!isRecord(value.isolation_failure_probability) ||
		!hasExactKeys(value.isolation_failure_probability, [
			"numerator",
			"denominator",
		]) ||
		typeof value.isolation_failure_probability.numerator !== "string" ||
		!canonicalU64.test(value.isolation_failure_probability.numerator) ||
		typeof value.isolation_failure_probability.denominator !== "string" ||
		!canonicalU64.test(value.isolation_failure_probability.denominator) ||
		value.isolation_failure_probability.denominator === "0" ||
		typeof value.active_return_tree_edge !== "boolean" ||
		!["-1", "0", "1"].includes(value.active_return_sign as string) ||
		(value.selected_ratio !== undefined &&
			typeof value.selected_ratio !== "string") ||
		(value.exact_pool_ratio !== undefined &&
			typeof value.exact_pool_ratio !== "string") ||
		(value.final_point_return_flow !== undefined &&
			typeof value.final_point_return_flow !== "string") ||
		(value.final_return_flow !== undefined &&
			(typeof value.final_return_flow !== "string" ||
				!canonicalU64.test(value.final_return_flow))) ||
		(value.final_artificial_flow !== undefined &&
			(typeof value.final_artificial_flow !== "string" ||
				!canonicalU64.test(value.final_artificial_flow))) ||
		(value.isolated_objective !== undefined &&
			(typeof value.isolated_objective !== "string" ||
				!canonicalI128.test(value.isolated_objective))) ||
		(value.final_point_gap !== undefined &&
			typeof value.final_point_gap !== "string") ||
		(value.final_point_mix !== undefined &&
			typeof value.final_point_mix !== "string") ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges)
	) {
		throw new Error(
			"Flow scene contains an invalid randomized almost-linear overlay",
		);
	}
	const finite = {
		alpha: decodeFiniteDecimal(value.alpha),
		potential: decodeFiniteDecimal(value.potential),
		cost_gap: decodeFiniteDecimal(value.cost_gap),
		return_flow: decodeFiniteDecimal(value.return_flow),
		return_gradient: decodeFiniteDecimal(value.return_gradient),
		return_length: decodeFiniteDecimal(value.return_length),
		artificial_flow: decodeFiniteDecimal(value.artificial_flow),
		final_point_threshold: decodeFiniteDecimal(value.final_point_threshold),
		...(value.selected_ratio === undefined
			? {}
			: { selected_ratio: decodeFiniteDecimal(value.selected_ratio) }),
		...(value.exact_pool_ratio === undefined
			? {}
			: { exact_pool_ratio: decodeFiniteDecimal(value.exact_pool_ratio) }),
		...(value.final_point_return_flow === undefined
			? {}
			: {
					final_point_return_flow: decodeFiniteDecimal(
						value.final_point_return_flow,
					),
				}),
		...(value.final_point_gap === undefined
			? {}
			: { final_point_gap: decodeFiniteDecimal(value.final_point_gap) }),
		...(value.final_point_mix === undefined
			? {}
			: { final_point_mix: decodeFiniteDecimal(value.final_point_mix) }),
	};
	const nodes = value.nodes.map(
		(node): FlowRandomizedAlmostLinearNodeStateV1 => {
			if (
				!isRecord(node) ||
				!hasExactKeys(
					node,
					[
						"node_id",
						"tree_component",
						"source_side",
						"artificial_direction",
						"artificial_flow",
						"artificial_capacity",
						"artificial_tree_memberships",
						"active_artificial_tree_edge",
						"active_artificial_sign",
					],
					["tree_parent_node_id"],
				) ||
				typeof node.node_id !== "string" ||
				(node.tree_parent_node_id !== undefined &&
					typeof node.tree_parent_node_id !== "string") ||
				typeof node.tree_component !== "string" ||
				!canonicalU64.test(node.tree_component) ||
				typeof node.source_side !== "boolean" ||
				!["-1", "0", "1"].includes(node.artificial_direction as string) ||
				typeof node.artificial_tree_memberships !== "string" ||
				!canonicalU64.test(node.artificial_tree_memberships) ||
				typeof node.active_artificial_tree_edge !== "boolean" ||
				!["-1", "0", "1"].includes(node.active_artificial_sign as string)
			) {
				throw new Error("Flow scene contains an invalid randomized node state");
			}
			return {
				node_id: node.node_id,
				...(node.tree_parent_node_id === undefined
					? {}
					: { tree_parent_node_id: node.tree_parent_node_id }),
				tree_component: node.tree_component,
				source_side: node.source_side,
				artificial_direction: node.artificial_direction as "-1" | "0" | "1",
				artificial_flow: decodeFiniteDecimal(node.artificial_flow),
				artificial_capacity: decodeFiniteDecimal(node.artificial_capacity),
				artificial_tree_memberships: node.artificial_tree_memberships,
				active_artificial_tree_edge: node.active_artificial_tree_edge,
				active_artificial_sign: node.active_artificial_sign as "-1" | "0" | "1",
			};
		},
	);
	const edges = value.edges.map(
		(edge): FlowRandomizedAlmostLinearEdgeStateV1 => {
			if (
				!isRecord(edge) ||
				!hasExactKeys(
					edge,
					[
						"edge_id",
						"interior_flow",
						"gradient",
						"length",
						"sampled_tree_memberships",
						"active_tree_edge",
						"active_cycle_sign",
						"changed_coordinate",
						"isolation_draw",
					],
					["final_point_flow", "final_flow"],
				) ||
				typeof edge.edge_id !== "string" ||
				typeof edge.sampled_tree_memberships !== "string" ||
				!canonicalU64.test(edge.sampled_tree_memberships) ||
				typeof edge.active_tree_edge !== "boolean" ||
				!["-1", "0", "1"].includes(edge.active_cycle_sign as string) ||
				typeof edge.changed_coordinate !== "boolean" ||
				typeof edge.isolation_draw !== "string" ||
				!canonicalU64.test(edge.isolation_draw) ||
				(edge.final_point_flow !== undefined &&
					typeof edge.final_point_flow !== "string") ||
				(edge.final_flow !== undefined &&
					(typeof edge.final_flow !== "string" ||
						!canonicalU64.test(edge.final_flow)))
			) {
				throw new Error("Flow scene contains an invalid randomized edge state");
			}
			return {
				edge_id: edge.edge_id,
				interior_flow: decodeFiniteDecimal(edge.interior_flow),
				gradient: decodeFiniteDecimal(edge.gradient),
				length: decodeFiniteDecimal(edge.length),
				sampled_tree_memberships: edge.sampled_tree_memberships,
				active_tree_edge: edge.active_tree_edge,
				active_cycle_sign: edge.active_cycle_sign as "-1" | "0" | "1",
				changed_coordinate: edge.changed_coordinate,
				isolation_draw: edge.isolation_draw,
				...(edge.final_point_flow === undefined
					? {}
					: {
							final_point_flow: decodeFiniteDecimal(edge.final_point_flow),
						}),
				...(edge.final_flow === undefined
					? {}
					: { final_flow: edge.final_flow }),
			};
		},
	);
	return {
		stage: value.stage as FlowRandomizedAlmostLinearOverlayV1["stage"],
		seed: value.seed as string,
		random_draws: value.random_draws as string,
		...finite,
		miss_probability: {
			numerator: value.miss_probability.numerator,
			denominator: value.miss_probability.denominator,
		},
		isolation_failure_probability: {
			numerator: value.isolation_failure_probability.numerator,
			denominator: value.isolation_failure_probability.denominator,
		},
		forest_pool_size: value.forest_pool_size as string,
		sample_count: value.sample_count as string,
		iteration: value.iteration as string,
		rebuild_epoch: value.rebuild_epoch as string,
		return_capacity: value.return_capacity as string,
		return_tree_memberships: value.return_tree_memberships as string,
		active_return_tree_edge: value.active_return_tree_edge,
		active_return_sign: value.active_return_sign as "-1" | "0" | "1",
		return_isolation_draw: value.return_isolation_draw as string,
		...(value.final_return_flow === undefined
			? {}
			: { final_return_flow: value.final_return_flow }),
		artificial_edges: value.artificial_edges as string,
		...(value.final_artificial_flow === undefined
			? {}
			: { final_artificial_flow: value.final_artificial_flow }),
		isolation_scale: value.isolation_scale as string,
		isolation_attempt: value.isolation_attempt as string,
		...(value.isolated_objective === undefined
			? {}
			: { isolated_objective: value.isolated_objective }),
		target_value: value.target_value as string,
		nodes,
		edges,
	};
}

function decodeDeterministicAlmostLinearOverlay(
	value: unknown,
): FlowDeterministicAlmostLinearOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowDeterministicAlmostLinearOverlayV1["stage"][] = [
		"ready",
		"build-return-edge-reduction",
		"build-initial-point",
		"enumerate-forest-pool",
		"install-branch-record",
		"build-branch-collection",
		"build-core-graph",
		"build-spanner-embedding",
		"inspect-fundamental-cycle",
		"query-minimum-ratio-cycle",
		"query-failure",
		"shift-branch",
		"rebuild-deeper-levels",
		"potential-reduction-step",
		"detect-changed-coordinates",
		"scheduled-rebuild",
		"enumerate-feasible-set",
		"construct-final-point",
		"rounding-integral-edge",
		"rounding-link-fractional-edge",
		"rounding-cancel-fractional-cycle",
		"finish-flow-rounding",
		"check-certificate",
		"optimal",
	];
	const counts = [
		"forest_pool_size",
		"level_count",
		"branch_count",
		"built_branch_records",
		"fundamental_cycles",
		"core_vertices",
		"core_edges",
		"spanner_edges",
		"embedding_hops",
		"iteration",
		"rebuild_epoch",
		"return_capacity",
		"return_tree_level_mask",
		"artificial_edges",
		"target_value",
	] as const;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				...counts,
				"alpha",
				"potential",
				"cost_gap",
				"active_branches",
				"passes",
				"return_flow",
				"return_gradient",
				"return_length",
				"active_return_tree_edge",
				"active_return_sign",
				"rounding_return_forest_edge",
				"rounding_return_sign",
				"artificial_flow",
				"final_point_threshold",
				"nodes",
				"edges",
			],
			[
				"selected_ratio",
				"exact_pool_ratio",
				"selected_off_tree_edge",
				"selected_cycle_kind",
				"active_level",
				"final_point_return_flow",
				"rounding_return_flow",
				"final_return_flow",
				"final_artificial_flow",
				"final_point_gap",
				"final_point_mix",
				"rounding_processed_edge",
			],
		) ||
		!stages.includes(
			value.stage as FlowDeterministicAlmostLinearOverlayV1["stage"],
		) ||
		!counts.every(
			(key) => typeof value[key] === "string" && canonicalU64.test(value[key]),
		) ||
		!Array.isArray(value.active_branches) ||
		!value.active_branches.every(
			(item) => typeof item === "string" && canonicalU64.test(item),
		) ||
		!Array.isArray(value.passes) ||
		!value.passes.every(
			(item) => typeof item === "string" && canonicalU64.test(item),
		) ||
		(value.active_level !== undefined &&
			(typeof value.active_level !== "string" ||
				!canonicalU64.test(value.active_level))) ||
		(value.selected_off_tree_edge !== undefined &&
			(typeof value.selected_off_tree_edge !== "string" ||
				!canonicalU64.test(value.selected_off_tree_edge))) ||
		(value.selected_cycle_kind !== undefined &&
			!["tree", "spanner"].includes(value.selected_cycle_kind as string)) ||
		typeof value.active_return_tree_edge !== "boolean" ||
		!["-1", "0", "1"].includes(value.active_return_sign as string) ||
		typeof value.rounding_return_forest_edge !== "boolean" ||
		!["-1", "0", "1"].includes(value.rounding_return_sign as string) ||
		(value.rounding_processed_edge !== undefined &&
			typeof value.rounding_processed_edge !== "string") ||
		(value.final_return_flow !== undefined &&
			(typeof value.final_return_flow !== "string" ||
				!canonicalU64.test(value.final_return_flow))) ||
		(value.final_artificial_flow !== undefined &&
			(typeof value.final_artificial_flow !== "string" ||
				!canonicalU64.test(value.final_artificial_flow))) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges)
	) {
		throw new Error(
			"Flow scene contains an invalid deterministic almost-linear overlay",
		);
	}
	const finalPointThreshold = decodeRational(value.final_point_threshold);
	const finalPointReturnFlow =
		value.final_point_return_flow === undefined
			? undefined
			: decodeRational(value.final_point_return_flow);
	const roundingReturnFlow =
		value.rounding_return_flow === undefined
			? undefined
			: decodeRational(value.rounding_return_flow);
	const finalPointGap =
		value.final_point_gap === undefined
			? undefined
			: decodeRational(value.final_point_gap);
	const finalPointMix =
		value.final_point_mix === undefined
			? undefined
			: decodeRational(value.final_point_mix);
	const finite = {
		alpha: decodeFiniteDecimal(value.alpha),
		potential: decodeFiniteDecimal(value.potential),
		cost_gap: decodeFiniteDecimal(value.cost_gap),
		return_flow: decodeFiniteDecimal(value.return_flow),
		return_gradient: decodeFiniteDecimal(value.return_gradient),
		return_length: decodeFiniteDecimal(value.return_length),
		artificial_flow: decodeFiniteDecimal(value.artificial_flow),
		...(value.selected_ratio === undefined
			? {}
			: { selected_ratio: decodeFiniteDecimal(value.selected_ratio) }),
		...(value.exact_pool_ratio === undefined
			? {}
			: { exact_pool_ratio: decodeFiniteDecimal(value.exact_pool_ratio) }),
	};
	const nodes = value.nodes.map(
		(node): FlowDeterministicAlmostLinearNodeStateV1 => {
			if (
				!isRecord(node) ||
				!hasExactKeys(
					node,
					[
						"node_id",
						"forest_component",
						"source_side",
						"artificial_direction",
						"artificial_flow",
						"artificial_capacity",
						"artificial_tree_level_mask",
						"active_artificial_tree_edge",
						"active_artificial_sign",
					],
					["tree_parent_node_id"],
				) ||
				typeof node.node_id !== "string" ||
				(node.tree_parent_node_id !== undefined &&
					typeof node.tree_parent_node_id !== "string") ||
				typeof node.forest_component !== "string" ||
				!canonicalU64.test(node.forest_component) ||
				typeof node.source_side !== "boolean" ||
				!["-1", "0", "1"].includes(node.artificial_direction as string) ||
				typeof node.artificial_tree_level_mask !== "string" ||
				!canonicalU64.test(node.artificial_tree_level_mask) ||
				typeof node.active_artificial_tree_edge !== "boolean" ||
				!["-1", "0", "1"].includes(node.active_artificial_sign as string)
			) {
				throw new Error(
					"Flow scene contains an invalid deterministic node state",
				);
			}
			return {
				node_id: node.node_id,
				...(node.tree_parent_node_id === undefined
					? {}
					: { tree_parent_node_id: node.tree_parent_node_id }),
				forest_component: node.forest_component,
				source_side: node.source_side,
				artificial_direction: node.artificial_direction as "-1" | "0" | "1",
				artificial_flow: decodeFiniteDecimal(node.artificial_flow),
				artificial_capacity: decodeFiniteDecimal(node.artificial_capacity),
				artificial_tree_level_mask: node.artificial_tree_level_mask,
				active_artificial_tree_edge: node.active_artificial_tree_edge,
				active_artificial_sign: node.active_artificial_sign as "-1" | "0" | "1",
			};
		},
	);
	const edges = value.edges.map(
		(edge): FlowDeterministicAlmostLinearEdgeStateV1 => {
			if (
				!isRecord(edge) ||
				!hasExactKeys(
					edge,
					[
						"edge_id",
						"interior_flow",
						"gradient",
						"length",
						"tree_level_mask",
						"forest_level_mask",
						"active_tree_edge",
						"active_core_edge",
						"active_spanner_edge",
						"embedding_hops",
						"embedding_stretch",
						"active_cycle_sign",
						"changed_coordinate",
						"rounding_forest_edge",
						"rounding_cycle_sign",
					],
					["final_point_flow", "rounding_flow", "final_flow"],
				) ||
				typeof edge.edge_id !== "string" ||
				typeof edge.tree_level_mask !== "string" ||
				!canonicalU64.test(edge.tree_level_mask) ||
				typeof edge.forest_level_mask !== "string" ||
				!canonicalU64.test(edge.forest_level_mask) ||
				typeof edge.embedding_hops !== "string" ||
				!canonicalU64.test(edge.embedding_hops) ||
				typeof edge.active_tree_edge !== "boolean" ||
				typeof edge.active_core_edge !== "boolean" ||
				typeof edge.active_spanner_edge !== "boolean" ||
				!["-1", "0", "1"].includes(edge.active_cycle_sign as string) ||
				typeof edge.changed_coordinate !== "boolean" ||
				typeof edge.rounding_forest_edge !== "boolean" ||
				!["-1", "0", "1"].includes(edge.rounding_cycle_sign as string) ||
				(edge.final_flow !== undefined &&
					(typeof edge.final_flow !== "string" ||
						!canonicalU64.test(edge.final_flow)))
			) {
				throw new Error(
					"Flow scene contains an invalid deterministic edge state",
				);
			}
			const finalPointFlow =
				edge.final_point_flow === undefined
					? undefined
					: decodeRational(edge.final_point_flow);
			const roundingFlow =
				edge.rounding_flow === undefined
					? undefined
					: decodeRational(edge.rounding_flow);
			return {
				edge_id: edge.edge_id,
				interior_flow: decodeFiniteDecimal(edge.interior_flow),
				gradient: decodeFiniteDecimal(edge.gradient),
				length: decodeFiniteDecimal(edge.length),
				tree_level_mask: edge.tree_level_mask,
				forest_level_mask: edge.forest_level_mask,
				active_tree_edge: edge.active_tree_edge,
				active_core_edge: edge.active_core_edge,
				active_spanner_edge: edge.active_spanner_edge,
				embedding_hops: edge.embedding_hops,
				embedding_stretch: decodeFiniteDecimal(edge.embedding_stretch),
				active_cycle_sign: edge.active_cycle_sign as "-1" | "0" | "1",
				changed_coordinate: edge.changed_coordinate,
				...(finalPointFlow === undefined
					? {}
					: { final_point_flow: finalPointFlow }),
				...(roundingFlow === undefined ? {} : { rounding_flow: roundingFlow }),
				rounding_forest_edge: edge.rounding_forest_edge,
				rounding_cycle_sign: edge.rounding_cycle_sign as "-1" | "0" | "1",
				...(edge.final_flow === undefined
					? {}
					: { final_flow: edge.final_flow }),
			};
		},
	);
	return {
		stage: value.stage as FlowDeterministicAlmostLinearOverlayV1["stage"],
		...finite,
		...(value.selected_off_tree_edge === undefined
			? {}
			: { selected_off_tree_edge: value.selected_off_tree_edge }),
		...(value.selected_cycle_kind === undefined
			? {}
			: {
					selected_cycle_kind: value.selected_cycle_kind as "tree" | "spanner",
				}),
		forest_pool_size: value.forest_pool_size as string,
		level_count: value.level_count as string,
		branch_count: value.branch_count as string,
		built_branch_records: value.built_branch_records as string,
		active_branches: value.active_branches as string[],
		passes: value.passes as string[],
		...(value.active_level === undefined
			? {}
			: { active_level: value.active_level }),
		fundamental_cycles: value.fundamental_cycles as string,
		core_vertices: value.core_vertices as string,
		core_edges: value.core_edges as string,
		spanner_edges: value.spanner_edges as string,
		embedding_hops: value.embedding_hops as string,
		iteration: value.iteration as string,
		rebuild_epoch: value.rebuild_epoch as string,
		return_capacity: value.return_capacity as string,
		return_tree_level_mask: value.return_tree_level_mask as string,
		active_return_tree_edge: value.active_return_tree_edge,
		active_return_sign: value.active_return_sign as "-1" | "0" | "1",
		...(finalPointReturnFlow === undefined
			? {}
			: { final_point_return_flow: finalPointReturnFlow }),
		...(roundingReturnFlow === undefined
			? {}
			: { rounding_return_flow: roundingReturnFlow }),
		rounding_return_forest_edge: value.rounding_return_forest_edge,
		rounding_return_sign: value.rounding_return_sign as "-1" | "0" | "1",
		...(value.final_return_flow === undefined
			? {}
			: { final_return_flow: value.final_return_flow }),
		artificial_edges: value.artificial_edges as string,
		...(value.final_artificial_flow === undefined
			? {}
			: { final_artificial_flow: value.final_artificial_flow }),
		...(finalPointGap === undefined ? {} : { final_point_gap: finalPointGap }),
		final_point_threshold: finalPointThreshold,
		...(finalPointMix === undefined ? {} : { final_point_mix: finalPointMix }),
		...(value.rounding_processed_edge === undefined
			? {}
			: { rounding_processed_edge: value.rounding_processed_edge }),
		target_value: value.target_value as string,
		nodes,
		edges,
	};
}

function decodeElectricalIpmMcfOverlay(
	value: unknown,
): FlowElectricalIpmMcfOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowElectricalIpmMcfOverlayV1["stage"][] = [
		"ready",
		"normalize-lower-bounds",
		"isolation-attempt",
		"select-isolated-costs",
		"contract-fixed-face",
		"initialize-dual-interior",
		"assemble-electrical-laplacian",
		"solve-newton-direction",
		"damped-centering-step",
		"centered",
		"decrease-barrier",
		"approximate-flow",
		"round-nearest-integer",
		"check-certificate",
		"optimal",
	];
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"stage",
			"seed",
			"mu",
			"epsilon_3",
			"recovery_epsilon",
			"duality_gap_bound",
			"centrality_residual",
			"balance_residual",
			"step_size",
			"electrical_energy",
			"linear_residual",
			"barrier_objective",
			"isolation_scale",
			"perturbation_bound",
			"isolation_attempt",
			"isolated_optimum_cost",
			"isolated_gap",
			"nodes",
			"edges",
		]) ||
		!stages.includes(value.stage as FlowElectricalIpmMcfOverlayV1["stage"]) ||
		!canonicalU64.test(value.seed as string) ||
		!canonicalI128.test(value.isolation_scale as string) ||
		!canonicalU64.test(value.perturbation_bound as string) ||
		!canonicalU64.test(value.isolation_attempt as string) ||
		!canonicalI128.test(value.isolated_optimum_cost as string) ||
		!canonicalI128.test(value.isolated_gap as string) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.edges)
	) {
		throw new Error("Flow scene contains an invalid electrical IPM overlay");
	}
	const finiteKeys = [
		"mu",
		"epsilon_3",
		"recovery_epsilon",
		"duality_gap_bound",
		"centrality_residual",
		"balance_residual",
		"step_size",
		"electrical_energy",
		"linear_residual",
		"barrier_objective",
	] as const;
	const finite = Object.fromEntries(
		finiteKeys.map((key) => [key, decodeFiniteDecimal(value[key])]),
	) as Record<(typeof finiteKeys)[number], string>;
	const nodes = value.nodes.map((node): FlowElectricalIpmMcfNodeStateV1 => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, [
				"node_id",
				"potential",
				"potential_direction",
				"balance_residual",
				"anchored",
			]) ||
			typeof node.node_id !== "string" ||
			typeof node.anchored !== "boolean"
		) {
			throw new Error("Flow scene contains an invalid electrical IPM node");
		}
		return {
			node_id: node.node_id,
			potential: decodeFiniteDecimal(node.potential),
			potential_direction: decodeFiniteDecimal(node.potential_direction),
			balance_residual: decodeFiniteDecimal(node.balance_residual),
			anchored: node.anchored,
		};
	});
	const edges = value.edges.map((edge): FlowElectricalIpmMcfEdgeStateV1 => {
		if (
			!isRecord(edge) ||
			!hasExactKeys(
				edge,
				[
					"edge_id",
					"perturbation",
					"isolated_cost",
					"fixed_on_face",
					"face_lower",
					"face_upper",
					"fractional_flow",
					"upper_complement",
					"lower_slack",
					"upper_multiplier",
					"resistance",
					"conductance",
					"electrical_current",
					"lower_slack_direction",
					"upper_multiplier_direction",
				],
				["final_flow"],
			) ||
			typeof edge.edge_id !== "string" ||
			!canonicalU64.test(edge.perturbation as string) ||
			!canonicalI128.test(edge.isolated_cost as string) ||
			typeof edge.fixed_on_face !== "boolean" ||
			!canonicalU64.test(edge.face_lower as string) ||
			!canonicalU64.test(edge.face_upper as string) ||
			(edge.final_flow !== undefined &&
				!canonicalU64.test(edge.final_flow as string))
		) {
			throw new Error("Flow scene contains an invalid electrical IPM edge");
		}
		return {
			edge_id: edge.edge_id,
			perturbation: edge.perturbation as string,
			isolated_cost: edge.isolated_cost as string,
			fixed_on_face: edge.fixed_on_face,
			face_lower: edge.face_lower as string,
			face_upper: edge.face_upper as string,
			fractional_flow: decodeFiniteDecimal(edge.fractional_flow),
			upper_complement: decodeFiniteDecimal(edge.upper_complement),
			lower_slack: decodeFiniteDecimal(edge.lower_slack),
			upper_multiplier: decodeFiniteDecimal(edge.upper_multiplier),
			resistance: decodeFiniteDecimal(edge.resistance),
			conductance: decodeFiniteDecimal(edge.conductance),
			electrical_current: decodeFiniteDecimal(edge.electrical_current),
			lower_slack_direction: decodeFiniteDecimal(edge.lower_slack_direction),
			upper_multiplier_direction: decodeFiniteDecimal(
				edge.upper_multiplier_direction,
			),
			...(edge.final_flow === undefined
				? {}
				: { final_flow: edge.final_flow as string }),
		};
	});
	return {
		stage: value.stage as FlowElectricalIpmMcfOverlayV1["stage"],
		seed: value.seed as string,
		...finite,
		isolation_scale: value.isolation_scale as string,
		perturbation_bound: value.perturbation_bound as string,
		isolation_attempt: value.isolation_attempt as string,
		isolated_optimum_cost: value.isolated_optimum_cost as string,
		isolated_gap: value.isolated_gap as string,
		nodes,
		edges,
	};
}

function decodePrimalDualIpmMcfOverlay(
	value: unknown,
): FlowPrimalDualIpmMcfOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const stages: readonly FlowPrimalDualIpmMcfOverlayV1["stage"][] = [
		"ready",
		"normalize-input",
		"build-capacity-reduction",
		"initialize-central-point",
		"build-minor",
		"decrease-mu",
		"inspect-forest-subset",
		"build-low-stretch-forest",
		"sample-fundamental-cycle",
		"centering-cycle-update",
		"centered",
		"proxy-reached",
		"crossover-grow-cut",
		"restore-original-dual",
		"recover-admissible-flow",
		"check-certificate",
		"optimal",
	];
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"seed",
				"mu",
				"beta",
				"gamma",
				"proxy_gap",
				"centrality_numerator",
				"cycle_alpha",
				"nodes",
				"arcs",
			],
			["sampled_arc", "tree_condition_number", "forest_subset_serial"],
		) ||
		!stages.includes(value.stage as FlowPrimalDualIpmMcfOverlayV1["stage"]) ||
		!["seed", "beta", "gamma", "proxy_gap", "centrality_numerator"].every(
			(key) => typeof value[key] === "string" && canonicalU64.test(value[key]),
		) ||
		typeof value.mu !== "string" ||
		!canonicalI128.test(value.mu) ||
		typeof value.cycle_alpha !== "string" ||
		!canonicalI128.test(value.cycle_alpha) ||
		(value.sampled_arc !== undefined &&
			typeof value.sampled_arc !== "string") ||
		(value.forest_subset_serial !== undefined &&
			(typeof value.forest_subset_serial !== "string" ||
				!canonicalU64.test(value.forest_subset_serial) ||
				value.forest_subset_serial === "0")) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.arcs)
	) {
		throw new Error("Flow scene contains an invalid primal-dual IPM overlay");
	}
	let treeConditionNumber:
		| FlowPrimalDualIpmMcfOverlayV1["tree_condition_number"]
		| undefined;
	if (value.tree_condition_number !== undefined) {
		if (
			!isRecord(value.tree_condition_number) ||
			!hasExactKeys(value.tree_condition_number, [
				"numerator",
				"denominator",
			]) ||
			typeof value.tree_condition_number.numerator !== "string" ||
			!canonicalU64.test(value.tree_condition_number.numerator) ||
			typeof value.tree_condition_number.denominator !== "string" ||
			!canonicalU64.test(value.tree_condition_number.denominator) ||
			value.tree_condition_number.denominator === "0"
		) {
			throw new Error(
				"Flow scene contains an invalid IPM tree condition number",
			);
		}
		treeConditionNumber = {
			numerator: value.tree_condition_number.numerator,
			denominator: value.tree_condition_number.denominator,
		};
	}
	const nodes = value.nodes.map((node): FlowPrimalDualIpmMcfNodeStateV1 => {
		if (
			!isRecord(node) ||
			!hasExactKeys(
				node,
				["auxiliary_id", "kind", "potential", "component", "in_crossover_set"],
				["original_node_id", "original_edge_id"],
			) ||
			typeof node.auxiliary_id !== "string" ||
			!(["original", "capacity"] as const).includes(
				node.kind as "original" | "capacity",
			) ||
			typeof node.potential !== "string" ||
			!canonicalI128.test(node.potential) ||
			typeof node.component !== "string" ||
			!canonicalU64.test(node.component) ||
			typeof node.in_crossover_set !== "boolean" ||
			(node.original_node_id !== undefined &&
				typeof node.original_node_id !== "string") ||
			(node.original_edge_id !== undefined &&
				typeof node.original_edge_id !== "string")
		) {
			throw new Error("Flow scene contains an invalid primal-dual IPM node");
		}
		return {
			auxiliary_id: node.auxiliary_id,
			kind: node.kind as "original" | "capacity",
			...(node.original_node_id === undefined
				? {}
				: { original_node_id: node.original_node_id }),
			...(node.original_edge_id === undefined
				? {}
				: { original_edge_id: node.original_edge_id }),
			potential: node.potential,
			component: node.component,
			in_crossover_set: node.in_crossover_set,
		};
	});
	const arcs = value.arcs.map((arc): FlowPrimalDualIpmMcfArcStateV1 => {
		if (
			!isRecord(arc) ||
			!hasExactKeys(
				arc,
				[
					"auxiliary_id",
					"original_edge_id",
					"from",
					"to",
					"kind",
					"flow",
					"slack",
					"deleted",
					"contracted",
					"in_minor",
					"in_tree",
					"forest_candidate",
					"active_cycle_sign",
				],
				["resistance"],
			) ||
			!["auxiliary_id", "original_edge_id", "from", "to"].every(
				(key) => typeof arc[key] === "string",
			) ||
			!(["upper", "lower", "artificial"] as const).includes(
				arc.kind as "upper" | "lower" | "artificial",
			) ||
			typeof arc.flow !== "string" ||
			!canonicalU64.test(arc.flow) ||
			typeof arc.slack !== "string" ||
			!canonicalU64.test(arc.slack) ||
			(arc.resistance !== undefined &&
				(typeof arc.resistance !== "string" ||
					!canonicalU64.test(arc.resistance) ||
					arc.resistance === "0")) ||
			![
				"deleted",
				"contracted",
				"in_minor",
				"in_tree",
				"forest_candidate",
			].every((key) => typeof arc[key] === "boolean") ||
			!["-1", "0", "1"].includes(arc.active_cycle_sign as string)
		) {
			throw new Error("Flow scene contains an invalid primal-dual IPM arc");
		}
		return {
			auxiliary_id: arc.auxiliary_id as string,
			original_edge_id: arc.original_edge_id as string,
			from: arc.from as string,
			to: arc.to as string,
			kind: arc.kind as "upper" | "lower" | "artificial",
			flow: arc.flow,
			slack: arc.slack,
			...(arc.resistance === undefined ? {} : { resistance: arc.resistance }),
			deleted: arc.deleted as boolean,
			contracted: arc.contracted as boolean,
			in_minor: arc.in_minor as boolean,
			in_tree: arc.in_tree as boolean,
			forest_candidate: arc.forest_candidate as boolean,
			active_cycle_sign: arc.active_cycle_sign as "-1" | "0" | "1",
		};
	});
	return {
		stage: value.stage as FlowPrimalDualIpmMcfOverlayV1["stage"],
		seed: value.seed as string,
		mu: value.mu,
		beta: value.beta as string,
		gamma: value.gamma as string,
		proxy_gap: value.proxy_gap as string,
		centrality_numerator: value.centrality_numerator as string,
		...(value.sampled_arc === undefined
			? {}
			: { sampled_arc: value.sampled_arc }),
		cycle_alpha: value.cycle_alpha,
		...(treeConditionNumber === undefined
			? {}
			: { tree_condition_number: treeConditionNumber }),
		...(value.forest_subset_serial === undefined
			? {}
			: { forest_subset_serial: value.forest_subset_serial }),
		nodes,
		arcs,
	};
}

function decodeEibfsOverlay(value: unknown): FlowEibfsOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"phase_direction",
			"source_depth",
			"sink_depth",
			"nodes",
			"forest_arcs",
		]) ||
		(value.phase_direction !== "forward" &&
			value.phase_direction !== "reverse") ||
		typeof value.source_depth !== "string" ||
		!canonicalU64.test(value.source_depth) ||
		typeof value.sink_depth !== "string" ||
		!canonicalU64.test(value.sink_depth) ||
		!Array.isArray(value.nodes) ||
		!Array.isArray(value.forest_arcs)
	) {
		throw new Error("Flow scene contains an invalid EIBFS overlay");
	}
	const nodes = value.nodes.map((node): FlowEibfsNodeStateV1 => {
		if (
			!isRecord(node) ||
			!hasExactKeys(node, [
				"node_id",
				"source_label",
				"sink_label",
				"membership",
				"root_kind",
				"orphan",
				"imbalance",
			]) ||
			typeof node.node_id !== "string" ||
			typeof node.source_label !== "string" ||
			!canonicalU64.test(node.source_label) ||
			typeof node.sink_label !== "string" ||
			!canonicalU64.test(node.sink_label) ||
			!(["free", "source", "sink"] as const).includes(
				node.membership as "free" | "source" | "sink",
			) ||
			!(["none", "source", "sink", "excess", "deficit"] as const).includes(
				node.root_kind as "none" | "source" | "sink" | "excess" | "deficit",
			) ||
			typeof node.orphan !== "boolean" ||
			typeof node.imbalance !== "string" ||
			!canonicalI128.test(node.imbalance)
		) {
			throw new Error("Flow scene contains an invalid EIBFS node state");
		}
		return node as FlowEibfsNodeStateV1;
	});
	const forestArcs = value.forest_arcs.map((relation): FlowEibfsForestArcV1 => {
		if (
			!isRecord(relation) ||
			!hasExactKeys(relation, [
				"parent",
				"child",
				"side",
				"admissible_residual",
			]) ||
			typeof relation.parent !== "string" ||
			typeof relation.child !== "string" ||
			(relation.side !== "source" && relation.side !== "sink") ||
			!isRecord(relation.admissible_residual) ||
			!hasExactKeys(relation.admissible_residual, ["edge_id", "direction"]) ||
			typeof relation.admissible_residual.edge_id !== "string" ||
			(relation.admissible_residual.direction !== "forward" &&
				relation.admissible_residual.direction !== "reverse")
		) {
			throw new Error("Flow scene contains an invalid EIBFS forest arc");
		}
		return relation as FlowEibfsForestArcV1;
	});
	return {
		phase_direction: value.phase_direction,
		source_depth: value.source_depth,
		sink_depth: value.sink_depth,
		nodes,
		forest_arcs: forestArcs,
	};
}

function decodeDynamicEibfsOverlay(
	value: unknown,
): FlowDynamicEibfsOverlayV1 | undefined {
	if (value === undefined) return undefined;
	const requiredCounters = [
		"reused_forest_nodes",
		"updates_applied",
		"capacity_increases",
		"capacity_decreases",
		"no_op_updates",
		"over_capacity_repairs",
		"invalidated_parent_arcs",
		"promoted_roots",
		"repair_arc_scans",
		"state_transitions",
		"bridge_violations",
		"label_violations",
		"current_arc_violations",
		"boundary_violations",
		"repair_iterations",
		"certification_recoveries",
	] as const;
	const stages: readonly FlowDynamicEibfsOverlayV1["stage"][] = [
		"initial-solve",
		"apply-update",
		"repair-capacity",
		"repair-forest",
		"repair-violation",
		"continue-solve",
		"prefix-recovery",
		"prefix-certified",
		"resume-reusable-pseudoflow",
	];
	const violations: readonly NonNullable<
		FlowDynamicEibfsOverlayV1["violation"]
	>[] = ["over-capacity", "bridge", "label", "current-arc", "boundary"];
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			["stage", "update_index", "update_total", ...requiredCounters],
			[
				"changed_edge",
				"old_capacity",
				"new_capacity",
				"violation",
				"prefix_value",
			],
		) ||
		!stages.includes(value.stage as FlowDynamicEibfsOverlayV1["stage"]) ||
		typeof value.update_index !== "string" ||
		!canonicalU64.test(value.update_index) ||
		typeof value.update_total !== "string" ||
		!canonicalU64.test(value.update_total) ||
		requiredCounters.some(
			(field) =>
				typeof value[field] !== "string" || !canonicalU64.test(value[field]),
		) ||
		(value.changed_edge !== undefined &&
			typeof value.changed_edge !== "string") ||
		(value.old_capacity !== undefined &&
			(typeof value.old_capacity !== "string" ||
				!canonicalU64.test(value.old_capacity))) ||
		(value.new_capacity !== undefined &&
			(typeof value.new_capacity !== "string" ||
				!canonicalU64.test(value.new_capacity))) ||
		(value.violation !== undefined &&
			!violations.includes(
				value.violation as NonNullable<FlowDynamicEibfsOverlayV1["violation"]>,
			)) ||
		(value.prefix_value !== undefined &&
			(typeof value.prefix_value !== "string" ||
				!canonicalI128.test(value.prefix_value)))
	) {
		throw new Error("Flow scene contains an invalid Dynamic EIBFS overlay");
	}
	return value as FlowDynamicEibfsOverlayV1;
}

function decodeParametricSegment(value: unknown): FlowParametricSegmentV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"lower",
			"upper",
			"intercept",
			"slope",
			"minimal_source_side",
			"maximal_source_side",
		]) ||
		typeof value.intercept !== "string" ||
		!canonicalI64.test(value.intercept) ||
		typeof value.slope !== "string" ||
		!canonicalI64.test(value.slope) ||
		!Array.isArray(value.minimal_source_side) ||
		!value.minimal_source_side.every((node) => typeof node === "string") ||
		!Array.isArray(value.maximal_source_side) ||
		!value.maximal_source_side.every((node) => typeof node === "string")
	) {
		throw new Error("Flow scene contains an invalid parametric segment");
	}
	const lower = decodeRational(value.lower);
	const upper = decodeRational(value.upper);
	if (compareRational(lower, upper) > 0) {
		throw new Error("Flow scene parametric segment endpoints are decreasing");
	}
	return {
		lower,
		upper,
		intercept: value.intercept,
		slope: value.slope,
		minimal_source_side: value.minimal_source_side as string[],
		maximal_source_side: value.maximal_source_side as string[],
	};
}

function decodeParametricBreakpoint(
	value: unknown,
): FlowParametricBreakpointV1 {
	const sourceSideFields = [
		"before_source_side",
		"after_source_side",
		"exact_minimal_source_side",
		"exact_maximal_source_side",
		"entering_nodes",
	] as const;
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["parameter", ...sourceSideFields]) ||
		sourceSideFields.some(
			(field) =>
				!Array.isArray(value[field]) ||
				!(value[field] as unknown[]).every((node) => typeof node === "string"),
		)
	) {
		throw new Error("Flow scene contains an invalid parametric breakpoint");
	}
	return {
		parameter: decodeRational(value.parameter),
		before_source_side: value.before_source_side as string[],
		after_source_side: value.after_source_side as string[],
		exact_minimal_source_side: value.exact_minimal_source_side as string[],
		exact_maximal_source_side: value.exact_maximal_source_side as string[],
		entering_nodes: value.entering_nodes as string[],
	};
}

function decodeParametricMetrics(value: unknown): FlowParametricMetricsV1 {
	if (!isRecord(value)) {
		throw new Error("Flow scene contains invalid parametric metrics");
	}
	const pseudoflowFields = [
		"forest_initializations",
		"parameter_advances",
		"forest_reuses",
		"renormalization_pushes",
		"renormalization_splits",
		"mergers",
		"relabels",
		"free_run_races",
		"forward_race_wins",
		"reverse_race_wins",
		"cooperative_race_steps",
		"contraction_views",
		"smaller_child_restarts",
		"larger_child_continuations",
		"maximum_depth",
		"residual_arc_scans",
	] as const;
	const rerunFields = [
		"pseudoflow_runs",
		"oracle_runs",
		"static_residual_arc_scans",
		"intersections",
		"subproblems",
		"segments",
		"breakpoints",
		"simultaneous_breakpoints",
		"maximum_depth",
	] as const;
	const fields =
		value.implementation === "parametric-pseudoflow"
			? pseudoflowFields
			: value.implementation === "breakpoint-rerun"
				? rerunFields
				: undefined;
	if (
		fields === undefined ||
		!hasExactKeys(value, ["implementation", ...fields]) ||
		fields.some(
			(field) =>
				typeof value[field] !== "string" ||
				!canonicalU64.test(value[field] as string),
		)
	) {
		throw new Error("Flow scene contains invalid parametric metrics");
	}
	return value as FlowParametricMetricsV1;
}

function decodeParametricTraversal(
	value: unknown,
): FlowParametricTraversalV1 | undefined {
	if (value === undefined) return undefined;
	const optionalCounts = [
		"static_run_ordinal",
		"active_nodes",
		"left_active_nodes",
		"right_active_nodes",
	] as const;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"kind",
				"lower",
				"upper",
				"cold_static_rerun",
				"lower_source_side",
				"upper_source_side",
				"normalized_tree_reused",
				"labels_retained",
				"renormalization_pushes",
				"renormalization_splits",
			],
			[
				"probe",
				"orientation",
				"race_winner",
				"scale_denominator",
				...optionalCounts,
			],
		) ||
		typeof value.kind !== "string" ||
		value.kind.length === 0 ||
		(value.orientation !== undefined &&
			value.orientation !== "forward" &&
			value.orientation !== "reverse") ||
		(value.race_winner !== undefined &&
			value.race_winner !== "forward" &&
			value.race_winner !== "reverse") ||
		(value.kind === "initialize-forest" && value.orientation === undefined) ||
		typeof value.cold_static_rerun !== "boolean" ||
		(value.scale_denominator !== undefined &&
			(typeof value.scale_denominator !== "string" ||
				!canonicalU64.test(value.scale_denominator) ||
				value.scale_denominator === "0")) ||
		optionalCounts.some(
			(field) =>
				value[field] !== undefined &&
				(typeof value[field] !== "string" ||
					!canonicalU64.test(value[field] as string)),
		) ||
		!Array.isArray(value.lower_source_side) ||
		!value.lower_source_side.every((node) => typeof node === "string") ||
		!Array.isArray(value.upper_source_side) ||
		!value.upper_source_side.every((node) => typeof node === "string") ||
		typeof value.normalized_tree_reused !== "boolean" ||
		typeof value.labels_retained !== "boolean" ||
		typeof value.renormalization_pushes !== "string" ||
		!canonicalU64.test(value.renormalization_pushes) ||
		typeof value.renormalization_splits !== "string" ||
		!canonicalU64.test(value.renormalization_splits)
	) {
		throw new Error("Flow scene contains an invalid parametric traversal");
	}
	const lower = decodeRational(value.lower);
	const upper = decodeRational(value.upper);
	const probe =
		value.probe === undefined ? undefined : decodeRational(value.probe);
	if (
		compareRational(lower, upper) > 0 ||
		(probe !== undefined &&
			(compareRational(probe, lower) < 0 ||
				compareRational(probe, upper) > 0)) ||
		(value.cold_static_rerun && value.normalized_tree_reused)
	) {
		throw new Error("Flow scene parametric traversal is inconsistent");
	}
	return {
		kind: value.kind,
		lower,
		upper,
		...(probe === undefined ? {} : { probe }),
		...(value.orientation === undefined
			? {}
			: { orientation: value.orientation as "forward" | "reverse" }),
		...(value.race_winner === undefined
			? {}
			: { race_winner: value.race_winner as "forward" | "reverse" }),
		cold_static_rerun: value.cold_static_rerun,
		...(value.static_run_ordinal === undefined
			? {}
			: { static_run_ordinal: value.static_run_ordinal as string }),
		...(value.scale_denominator === undefined
			? {}
			: { scale_denominator: value.scale_denominator as string }),
		lower_source_side: value.lower_source_side as string[],
		upper_source_side: value.upper_source_side as string[],
		normalized_tree_reused: value.normalized_tree_reused,
		labels_retained: value.labels_retained,
		...(value.active_nodes === undefined
			? {}
			: { active_nodes: value.active_nodes as string }),
		...(value.left_active_nodes === undefined
			? {}
			: { left_active_nodes: value.left_active_nodes as string }),
		...(value.right_active_nodes === undefined
			? {}
			: { right_active_nodes: value.right_active_nodes as string }),
		renormalization_pushes: value.renormalization_pushes,
		renormalization_splits: value.renormalization_splits,
	};
}

function decodeParametricOverlay(
	value: unknown,
): FlowParametricOverlayV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"stage",
				"parameter",
				"edge_capacities",
				"visual_scale_max_capacity",
				"recorded_segments",
				"recorded_breakpoints",
			],
			["traversal"],
		) ||
		typeof value.stage !== "string" ||
		value.stage.length === 0 ||
		!Array.isArray(value.edge_capacities) ||
		!Array.isArray(value.recorded_segments) ||
		!Array.isArray(value.recorded_breakpoints)
	) {
		throw new Error("Flow scene contains an invalid parametric overlay");
	}
	const edgeCapacities = value.edge_capacities.map((item) => {
		if (
			!isRecord(item) ||
			!hasExactKeys(item, ["edge_id", "capacity"]) ||
			typeof item.edge_id !== "string"
		) {
			throw new Error(
				"Flow scene contains an invalid parametric edge capacity",
			);
		}
		const capacity = decodeRational(item.capacity);
		if (BigInt(capacity.numerator) < 0n) {
			throw new Error("Flow scene contains a negative parametric capacity");
		}
		return { edge_id: item.edge_id, capacity };
	});
	const visualScale = decodeRational(value.visual_scale_max_capacity);
	if (BigInt(visualScale.numerator) < 0n) {
		throw new Error("Flow scene contains a negative parametric visual scale");
	}
	const traversal = decodeParametricTraversal(value.traversal);
	return {
		stage: value.stage,
		parameter: decodeRational(value.parameter),
		edge_capacities: edgeCapacities,
		visual_scale_max_capacity: visualScale,
		recorded_segments: value.recorded_segments.map(decodeParametricSegment),
		recorded_breakpoints: value.recorded_breakpoints.map(
			decodeParametricBreakpoint,
		),
		...(traversal === undefined ? {} : { traversal }),
	};
}

function validateEibfsOverlay(
	overlay: FlowEibfsOverlayV1,
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	nodeTraceStates: FlowNodeTraceStateV1[],
	edgeById: Map<string, FlowEdgeV1>,
): void {
	if (model.kind !== "max-flow") {
		throw new Error("EIBFS overlay requires a max-flow model");
	}
	const nodeById = new Map(overlay.nodes.map((node) => [node.node_id, node]));
	const orderedNodeIds = canonicalNodeIds(nodes);
	if (
		nodeById.size !== overlay.nodes.length ||
		nodeById.size !== nodes.length ||
		overlay.nodes.some((node, index) => node.node_id !== orderedNodeIds[index])
	) {
		throw new Error("EIBFS node states do not match the graph");
	}
	const traceById = new Map(
		nodeTraceStates.map((state) => [state.node_id, state]),
	);
	const childParent = new Map<string, string>();
	const relationKeys = new Set<string>();
	for (const relation of overlay.forest_arcs) {
		const parent = nodeById.get(relation.parent);
		const child = nodeById.get(relation.child);
		const edge = edgeById.get(relation.admissible_residual.edge_id);
		const key = `${relation.child}\u0000${relation.admissible_residual.edge_id}\u0000${relation.admissible_residual.direction}`;
		if (
			parent === undefined ||
			child === undefined ||
			edge === undefined ||
			relation.parent === relation.child ||
			relationKeys.has(key) ||
			childParent.has(relation.child) ||
			parent.membership !== relation.side ||
			child.membership !== relation.side ||
			child.orphan ||
			child.root_kind !== "none"
		) {
			throw new Error("EIBFS overlay is not a rooted forest");
		}
		relationKeys.add(key);
		childParent.set(relation.child, relation.parent);
		const residualFrom =
			relation.admissible_residual.direction === "forward"
				? edge.from
				: edge.to;
		const residualTo =
			relation.admissible_residual.direction === "forward"
				? edge.to
				: edge.from;
		if (
			(relation.side === "source" &&
				(residualFrom !== relation.parent ||
					residualTo !== relation.child ||
					BigInt(child.source_label) !== BigInt(parent.source_label) + 1n)) ||
			(relation.side === "sink" &&
				(residualFrom !== relation.child ||
					residualTo !== relation.parent ||
					BigInt(child.sink_label) !== BigInt(parent.sink_label) + 1n))
		) {
			throw new Error("EIBFS forest arc is not admissible");
		}
	}
	for (const node of overlay.nodes) {
		const hasParent = childParent.has(node.node_id);
		const imbalance = BigInt(node.imbalance);
		const trace = traceById.get(node.node_id);
		const expectedLabel =
			node.membership === "free"
				? undefined
				: node.membership === "source"
					? node.source_label
					: (-BigInt(node.sink_label) - 1n).toString();
		if (
			trace === undefined ||
			trace.label !== expectedLabel ||
			(trace.remaining_divergence ?? "0") !== node.imbalance ||
			(node.membership === "free" &&
				(node.root_kind !== "none" || node.orphan || hasParent)) ||
			(node.orphan && (node.root_kind !== "none" || hasParent)) ||
			(!node.orphan &&
				node.membership !== "free" &&
				node.root_kind === "none" &&
				!hasParent) ||
			(node.root_kind !== "none" && hasParent) ||
			(node.root_kind === "source" &&
				(node.node_id !== model.source ||
					node.membership !== "source" ||
					node.source_label !== "0")) ||
			(node.root_kind === "sink" &&
				(node.node_id !== model.sink ||
					node.membership !== "sink" ||
					node.sink_label !== "0")) ||
			(node.root_kind === "excess" &&
				(node.membership !== "source" || imbalance <= 0n)) ||
			(node.root_kind === "deficit" &&
				(node.membership !== "sink" || imbalance >= 0n))
		) {
			throw new Error("EIBFS node root state is invalid");
		}
		const ancestry = new Set<string>();
		let ancestor: string | undefined = node.node_id;
		while (ancestor !== undefined) {
			if (ancestry.has(ancestor)) {
				throw new Error("EIBFS overlay contains a directed forest cycle");
			}
			ancestry.add(ancestor);
			ancestor = childParent.get(ancestor);
		}
	}
	if (
		nodeById.get(model.source)?.root_kind !== "source" ||
		nodeById.get(model.sink)?.root_kind !== "sink"
	) {
		throw new Error("EIBFS terminal roots are missing");
	}
}

function validateDynamicEibfsOverlay(
	overlay: FlowDynamicEibfsOverlayV1,
	model: FlowProblemModelV1,
	algorithmId: string,
	edgeById: ReadonlyMap<string, FlowEdgeV1>,
	eibfsOverlay: FlowEibfsOverlayV1 | undefined,
): void {
	const updateIndex = BigInt(overlay.update_index);
	const updateTotal = BigInt(overlay.update_total);
	if (
		algorithmId !== "dynamic-eibfs" ||
		model.kind !== "max-flow" ||
		updateTotal === 0n ||
		updateTotal > 256n ||
		updateIndex > updateTotal ||
		BigInt(overlay.updates_applied) !== updateIndex
	) {
		throw new Error("Dynamic EIBFS overlay does not match its Scenario");
	}
	if (updateIndex === 0n) {
		if (
			overlay.changed_edge !== undefined ||
			overlay.old_capacity !== undefined ||
			overlay.new_capacity !== undefined
		) {
			throw new Error("Dynamic EIBFS initial prefix contains an update");
		}
	} else {
		const edge =
			overlay.changed_edge === undefined
				? undefined
				: edgeById.get(overlay.changed_edge);
		if (
			edge === undefined ||
			overlay.old_capacity === undefined ||
			overlay.new_capacity === undefined ||
			edge.capacity !== overlay.new_capacity
		) {
			throw new Error("Dynamic EIBFS update does not match its current edge");
		}
	}

	const hasSearchForest = eibfsOverlay !== undefined;
	const withoutViolationOrValue =
		overlay.violation === undefined && overlay.prefix_value === undefined;
	let validStage = false;
	switch (overlay.stage) {
		case "initial-solve":
			validStage =
				updateIndex === 0n && hasSearchForest && withoutViolationOrValue;
			break;
		case "apply-update":
		case "repair-forest":
		case "continue-solve":
			validStage =
				updateIndex > 0n && hasSearchForest && withoutViolationOrValue;
			break;
		case "resume-reusable-pseudoflow":
			validStage = hasSearchForest && withoutViolationOrValue;
			break;
		case "repair-capacity":
			validStage =
				updateIndex > 0n &&
				hasSearchForest &&
				overlay.violation === "over-capacity" &&
				overlay.prefix_value === undefined;
			break;
		case "repair-violation":
			validStage =
				updateIndex > 0n &&
				hasSearchForest &&
				overlay.violation !== undefined &&
				overlay.violation !== "over-capacity" &&
				overlay.prefix_value === undefined;
			break;
		case "prefix-recovery":
			validStage = !hasSearchForest && withoutViolationOrValue;
			break;
		case "prefix-certified":
			validStage =
				!hasSearchForest &&
				overlay.violation === undefined &&
				overlay.prefix_value !== undefined;
			break;
	}
	if (!validStage) {
		throw new Error(
			`Dynamic EIBFS overlay stage is inconsistent: stage=${overlay.stage}, prefix=${overlay.update_index}/${overlay.update_total}, forest=${hasSearchForest}, violation=${overlay.violation ?? "none"}, value=${overlay.prefix_value ?? "none"}`,
		);
	}
}

function decodeTraceEntityRef(value: unknown): FlowTraceEntityRefV1 {
	if (!isRecord(value) || typeof value.kind !== "string") {
		throw new Error("Flow scene contains an invalid trace entity reference");
	}
	if (
		value.kind === "node" &&
		hasExactKeys(value, ["kind", "node_id"]) &&
		typeof value.node_id === "string"
	) {
		return { kind: "node", node_id: value.node_id };
	}
	if (
		value.kind === "edge" &&
		hasExactKeys(value, ["kind", "edge_id"]) &&
		typeof value.edge_id === "string"
	) {
		return { kind: "edge", edge_id: value.edge_id };
	}
	if (
		value.kind === "residual-arc" &&
		hasExactKeys(value, ["kind", "edge_id", "direction"]) &&
		typeof value.edge_id === "string" &&
		(value.direction === "forward" || value.direction === "reverse")
	) {
		return {
			kind: "residual-arc",
			edge_id: value.edge_id,
			direction: value.direction,
		};
	}
	throw new Error("Flow scene contains an invalid trace entity reference");
}

function decodeTraceEvent(value: unknown): FlowTraceEventV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			[
				"event_id",
				"catalog_id",
				"minimum_granularity",
				"pseudocode_line",
				"patch_count",
				"entity_refs",
			],
			["parent_phase_id", "detail"],
		) ||
		typeof value.event_id !== "string" ||
		!canonicalU64.test(value.event_id) ||
		value.event_id === "0" ||
		(value.parent_phase_id !== undefined &&
			(typeof value.parent_phase_id !== "string" ||
				!canonicalU64.test(value.parent_phase_id) ||
				value.parent_phase_id === "0")) ||
		typeof value.catalog_id !== "string" ||
		value.catalog_id.length === 0 ||
		!["phase", "operation", "micro"].includes(
			value.minimum_granularity as string,
		) ||
		typeof value.pseudocode_line !== "string" ||
		value.pseudocode_line.length === 0 ||
		typeof value.patch_count !== "number" ||
		!Number.isInteger(value.patch_count) ||
		value.patch_count < 0 ||
		value.patch_count > 65_536 ||
		!Array.isArray(value.entity_refs) ||
		value.entity_refs.length > 65_536
	) {
		throw new Error("Flow scene contains an invalid trace event");
	}
	if (
		value.detail !== undefined &&
		(!isRecord(value.detail) ||
			!hasExactKeys(value.detail, ["label", "value"], []) ||
			typeof value.detail.label !== "string" ||
			value.detail.label.length === 0 ||
			typeof value.detail.value !== "string" ||
			!canonicalFiniteDecimal.test(value.detail.value) ||
			value.detail.value === "-0" ||
			!Number.isFinite(Number(value.detail.value)))
	) {
		throw new Error("Flow scene contains an invalid trace event detail");
	}
	return {
		event_id: value.event_id,
		...(value.parent_phase_id === undefined
			? {}
			: { parent_phase_id: value.parent_phase_id as string }),
		catalog_id: value.catalog_id,
		minimum_granularity:
			value.minimum_granularity as FlowTraceEventV1["minimum_granularity"],
		pseudocode_line: value.pseudocode_line,
		patch_count: value.patch_count,
		entity_refs: value.entity_refs.map(decodeTraceEntityRef),
		...(value.detail === undefined
			? {}
			: {
					detail: {
						label: (value.detail as Record<string, unknown>).label as string,
						value: (value.detail as Record<string, unknown>).value as string,
					},
				}),
	};
}

function traceEntityIdentity(entity: FlowTraceEntityRefV1): string {
	return entity.kind === "node"
		? `node:${entity.node_id}`
		: entity.kind === "edge"
			? `edge:${entity.edge_id}`
			: `residual-arc:${entity.edge_id}:${entity.direction}`;
}

function traceEntityBelongsToGraph(
	entity: FlowTraceEntityRefV1,
	nodeIds: ReadonlySet<string>,
	edgeById: ReadonlyMap<string, unknown>,
): boolean {
	return entity.kind === "node"
		? nodeIds.has(entity.node_id)
		: edgeById.has(entity.edge_id);
}

function validateTraceEventSemantics(
	semantics: FlowTraceEventSemanticsV1 | undefined,
	event: FlowTraceEventV1 | undefined,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	traceSteps: FlowAlgorithmStepContractV1,
	metrics: readonly string[],
): void {
	if ((semantics === undefined) !== (event === undefined)) {
		throw new Error(
			"Flow trace event and its common semantic header must appear together",
		);
	}
	if (semantics === undefined || event === undefined) return;
	if (semantics.work_deltas.length === 0) {
		throw new Error("Flow trace semantic work deltas cannot be empty");
	}
	const units = new Set<string>();
	let largestDelta = 1n;
	for (const delta of semantics.work_deltas) {
		if (
			units.has(delta.unit) ||
			!canonicalU64.test(delta.count) ||
			delta.count === "0"
		) {
			throw new Error("Flow trace semantic work delta is invalid");
		}
		units.add(delta.unit);
		const count = BigInt(delta.count);
		if (count > largestDelta) largestDelta = count;
	}
	if (
		semantics.work_deltas[0]?.unit !== "published-transition" ||
		semantics.work_deltas[0]?.count !== "1" ||
		!canonicalU64.test(semantics.aggregation_count) ||
		semantics.aggregation_count === "0" ||
		BigInt(semantics.aggregation_count) !== largestDelta
	) {
		throw new Error("Flow trace semantic aggregation is invalid");
	}
	const progress = semantics.work_progress;
	if (
		!canonicalU64.test(progress.detail_completed) ||
		!canonicalU64.test(progress.detail_total) ||
		!canonicalU64.test(progress.primary_completed) ||
		!canonicalU64.test(progress.primary_total) ||
		BigInt(progress.detail_completed) > BigInt(progress.detail_total) ||
		BigInt(progress.primary_completed) > BigInt(progress.primary_total)
	) {
		throw new Error("Flow trace work progress is invalid");
	}
	const detailWork = semantics.work_deltas.find(
		(delta) => delta.unit === "detail-primitive",
	);
	const primaryWork = semantics.work_deltas.find(
		(delta) => delta.unit === "primary-work",
	);
	const primaryBlock = semantics.primary_work_block;
	const validPrimaryBlock = (() => {
		if (primaryWork === undefined) return primaryBlock === undefined;
		if (
			primaryBlock === undefined ||
			!canonicalU64.test(primaryBlock.first) ||
			!canonicalU64.test(primaryBlock.last) ||
			!canonicalU64.test(primaryBlock.total) ||
			primaryBlock.first === "0" ||
			primaryBlock.last === "0" ||
			primaryBlock.total === "0"
		) {
			return false;
		}
		const first = BigInt(primaryBlock.first);
		const last = BigInt(primaryBlock.last);
		const total = BigInt(primaryBlock.total);
		return (
			first <= last &&
			last <= total &&
			last - first + 1n === BigInt(primaryWork.count)
		);
	})();
	if (!validPrimaryBlock) {
		throw new Error(
			"Flow primary-work boundary must own one exact action-local block",
		);
	}
	if (
		(event.minimum_granularity === "micro") !== (detailWork !== undefined) ||
		(detailWork !== undefined && detailWork.count !== "1") ||
		(event.minimum_granularity === "micro" && progress.detail_completed === "0")
	) {
		throw new Error(
			"Flow Detail boundaries must own exactly one published detail primitive",
		);
	}
	const touched = event.entity_refs.map(traceEntityIdentity);
	const changed = semantics.changed_entity_refs.map(traceEntityIdentity);
	if (
		new Set(touched).size !== touched.length ||
		new Set(changed).size !== changed.length
	) {
		throw new Error(
			"Flow trace focus and changed entities must each be unique",
		);
	}
	const terminal = ["primitive-complete", "optimal", "infeasible"].includes(
		solveStatus,
	);
	if ((semantics.role === "certify") !== terminal) {
		throw new Error(
			"Flow trace certify role does not match the terminal status",
		);
	}
	if (
		terminal &&
		(progress.detail_completed !== progress.detail_total ||
			progress.primary_completed !== progress.primary_total ||
			metrics[traceSteps.primary_work.metric_ordinal] !==
				progress.primary_total)
	) {
		throw new Error(
			"Flow terminal trace work progress must complete its declared metric-backed totals",
		);
	}
}

function rejectSyntheticPrimaryWorkBoundary(
	event: FlowTraceEventV1 | undefined,
): void {
	if (event?.catalog_id.endsWith(PRIMARY_WORK_COUNTER_SUFFIX)) {
		throw new Error(
			"Flow trace contains a synthetic counter-only Detail boundary",
		);
	}
	if (event?.catalog_id.endsWith(".work-observation")) {
		throw new Error(
			"Flow trace contains a synthetic graphical work observation",
		);
	}
}

function decodeOutcome(value: unknown): FlowOutcomeV1 | undefined {
	if (value === undefined) return undefined;
	if (!isRecord(value) || typeof value.kind !== "string") {
		throw new Error("Flow scene contains an invalid outcome");
	}
	if (
		value.kind === "parametric-max-flow" &&
		hasExactKeys(value, ["kind", "segments", "breakpoints", "metrics"]) &&
		Array.isArray(value.segments) &&
		Array.isArray(value.breakpoints)
	) {
		return {
			kind: "parametric-max-flow",
			segments: value.segments.map(decodeParametricSegment),
			breakpoints: value.breakpoints.map(decodeParametricBreakpoint),
			metrics: decodeParametricMetrics(value.metrics),
		};
	}
	if (
		value.kind === "bipartite-matching" &&
		hasExactKeys(value, [
			"kind",
			"cardinality",
			"pairs",
			"cover_left",
			"cover_right",
		]) &&
		typeof value.cardinality === "string" &&
		canonicalU64.test(value.cardinality) &&
		Array.isArray(value.pairs) &&
		Array.isArray(value.cover_left) &&
		Array.isArray(value.cover_right) &&
		value.cover_left.every((node) => typeof node === "string") &&
		value.cover_right.every((node) => typeof node === "string")
	) {
		const pairs = value.pairs.map((pair) => {
			if (
				!isRecord(pair) ||
				!hasExactKeys(pair, ["edge_id", "left", "right"]) ||
				typeof pair.edge_id !== "string" ||
				typeof pair.left !== "string" ||
				typeof pair.right !== "string"
			) {
				throw new Error("Flow scene contains an invalid matching pair");
			}
			return {
				edge_id: pair.edge_id,
				left: pair.left,
				right: pair.right,
			};
		});
		const uniquePairEdges = new Set(pairs.map((pair) => pair.edge_id));
		const uniqueLeft = new Set(pairs.map((pair) => pair.left));
		const uniqueRight = new Set(pairs.map((pair) => pair.right));
		if (
			uniquePairEdges.size !== pairs.length ||
			uniqueLeft.size !== pairs.length ||
			uniqueRight.size !== pairs.length ||
			new Set(value.cover_left as string[]).size !== value.cover_left.length ||
			new Set(value.cover_right as string[]).size !== value.cover_right.length
		) {
			throw new Error(
				"Flow scene contains duplicate matching certificate identities",
			);
		}
		return {
			kind: "bipartite-matching",
			cardinality: value.cardinality,
			pairs,
			cover_left: value.cover_left as string[],
			cover_right: value.cover_right as string[],
		};
	}
	if (
		value.kind === "assignment" &&
		hasExactKeys(value, [
			"kind",
			"objective",
			"total_cost",
			"pairs",
			"agent_labels",
			"task_labels",
		]) &&
		["minimize", "maximize"].includes(value.objective as string) &&
		typeof value.total_cost === "string" &&
		canonicalI128.test(value.total_cost) &&
		Array.isArray(value.pairs) &&
		Array.isArray(value.agent_labels) &&
		Array.isArray(value.task_labels)
	) {
		const pairs = value.pairs.map((pair) => {
			if (
				!isRecord(pair) ||
				!hasExactKeys(pair, ["edge_id", "agent", "task", "cost"]) ||
				typeof pair.edge_id !== "string" ||
				typeof pair.agent !== "string" ||
				typeof pair.task !== "string" ||
				typeof pair.cost !== "string" ||
				!canonicalI64.test(pair.cost)
			) {
				throw new Error("Flow scene contains an invalid assignment pair");
			}
			return {
				edge_id: pair.edge_id,
				agent: pair.agent,
				task: pair.task,
				cost: pair.cost,
			};
		});
		const decodeLabels = (labels: unknown[]) =>
			labels.map((label) => {
				if (
					!isRecord(label) ||
					!hasExactKeys(label, ["node_id", "label"]) ||
					typeof label.node_id !== "string" ||
					typeof label.label !== "string" ||
					!canonicalI128.test(label.label)
				) {
					throw new Error("Flow scene contains an invalid assignment label");
				}
				return { node_id: label.node_id, label: label.label };
			});
		return {
			kind: "assignment",
			objective: value.objective as "minimize" | "maximize",
			total_cost: value.total_cost,
			pairs,
			agent_labels: decodeLabels(value.agent_labels),
			task_labels: decodeLabels(value.task_labels),
		};
	}
	if (
		value.kind === "assignment-infeasible" &&
		hasExactKeys(value, [
			"kind",
			"deficiency",
			"hall_agents",
			"neighbor_tasks",
		]) &&
		typeof value.deficiency === "string" &&
		canonicalU64.test(value.deficiency) &&
		value.deficiency !== "0" &&
		Array.isArray(value.hall_agents) &&
		Array.isArray(value.neighbor_tasks) &&
		value.hall_agents.every((node) => typeof node === "string") &&
		value.neighbor_tasks.every((node) => typeof node === "string")
	) {
		return {
			kind: "assignment-infeasible",
			deficiency: value.deficiency,
			hall_agents: value.hall_agents as string[],
			neighbor_tasks: value.neighbor_tasks as string[],
		};
	}
	if (
		value.kind === "max-flow" &&
		hasExactKeys(value, ["kind", "value", "cut_bound", "source_side"]) &&
		typeof value.value === "string" &&
		canonicalI128.test(value.value) &&
		typeof value.cut_bound === "string" &&
		canonicalI128.test(value.cut_bound) &&
		Array.isArray(value.source_side) &&
		value.source_side.every((node) => typeof node === "string")
	) {
		return {
			kind: "max-flow",
			value: value.value,
			cut_bound: value.cut_bound,
			source_side: value.source_side as string[],
		};
	}
	if (
		value.kind === "binary-blocking-flow" &&
		hasExactKeys(value, [
			"kind",
			"upper_bound",
			"delta",
			"delivered",
			"termination",
			"component_count",
			"nontrivial_component_count",
			"augmentation_operations",
		]) &&
		[
			value.upper_bound,
			value.delta,
			value.delivered,
			value.component_count,
			value.nontrivial_component_count,
			value.augmentation_operations,
		].every((item) => typeof item === "string" && canonicalU64.test(item)) &&
		value.upper_bound !== "0" &&
		value.delta !== "0" &&
		(value.termination === "blocking" || value.termination === "delta-reached")
	) {
		return value as FlowOutcomeV1;
	}
	if (
		value.kind === "tardos-framework" &&
		hasExactKeys(value, [
			"kind",
			"epsilon",
			"threshold",
			"determinant_bound",
			"fixed_variables",
		]) &&
		[value.epsilon, value.threshold].every(
			(item) =>
				typeof item === "string" &&
				canonicalI128.test(item) &&
				BigInt(item) >= 0n,
		) &&
		typeof value.determinant_bound === "string" &&
		canonicalU64.test(value.determinant_bound) &&
		Array.isArray(value.fixed_variables)
	) {
		return {
			kind: "tardos-framework",
			epsilon: value.epsilon as string,
			threshold: value.threshold as string,
			determinant_bound: value.determinant_bound,
			fixed_variables: value.fixed_variables.map(decodeTardosFixedVariable),
		};
	}
	if (
		value.kind === "electrical-flow" &&
		hasExactKeys(value, [
			"kind",
			"effective_resistance",
			"exact_effective_resistance",
			"total_energy",
			"residual_l2",
			"maximum_absolute_error",
			"iterations",
		]) &&
		typeof value.iterations === "string" &&
		canonicalU64.test(value.iterations)
	) {
		const effectiveResistance = decodeFiniteDecimal(value.effective_resistance);
		const totalEnergy = decodeFiniteDecimal(value.total_energy);
		const residualL2 = decodeFiniteDecimal(value.residual_l2);
		const maximumAbsoluteError = decodeFiniteDecimal(
			value.maximum_absolute_error,
		);
		if (
			Number(effectiveResistance) < 0 ||
			Number(totalEnergy) < 0 ||
			Number(residualL2) < 0 ||
			Number(maximumAbsoluteError) < 0
		) {
			throw new Error("Flow scene electrical outcome is not nonnegative");
		}
		return {
			kind: "electrical-flow",
			effective_resistance: effectiveResistance,
			exact_effective_resistance: decodeRational(
				value.exact_effective_resistance,
			),
			total_energy: totalEnergy,
			residual_l2: residualL2,
			maximum_absolute_error: maximumAbsoluteError,
			iterations: value.iterations,
		};
	}
	if (
		value.kind === "minimum-ratio-cycle" &&
		hasExactKeys(
			value,
			["kind", "cycle", "simple_cycles", "enumerated_vectors"],
			["ratio"],
		) &&
		Array.isArray(value.cycle) &&
		typeof value.simple_cycles === "string" &&
		canonicalU64.test(value.simple_cycles) &&
		typeof value.enumerated_vectors === "string" &&
		canonicalU64.test(value.enumerated_vectors)
	) {
		const cycle = value.cycle.map((arc) => {
			if (
				!isRecord(arc) ||
				!hasExactKeys(arc, ["edge_id", "sign"]) ||
				typeof arc.edge_id !== "string" ||
				!["-1", "1"].includes(arc.sign as string)
			) {
				throw new Error(
					"Flow scene contains an invalid minimum-ratio-cycle arc",
				);
			}
			return { edge_id: arc.edge_id, sign: arc.sign as "-1" | "1" };
		});
		if (new Set(cycle.map((arc) => arc.edge_id)).size !== cycle.length) {
			throw new Error("Flow scene minimum-ratio-cycle outcome repeats an edge");
		}
		return {
			kind: "minimum-ratio-cycle",
			...(value.ratio === undefined
				? {}
				: { ratio: decodeRational(value.ratio) }),
			cycle,
			simple_cycles: value.simple_cycles,
			enumerated_vectors: value.enumerated_vectors,
		};
	}
	if (
		value.kind === "minimum-ratio-cycle-mcf" &&
		hasExactKeys(
			value,
			[
				"kind",
				"cycle",
				"alpha",
				"kappa",
				"eta",
				"potential_decrease",
				"guaranteed_decrease",
				"stationary",
			],
			["ratio"],
		) &&
		Array.isArray(value.cycle) &&
		typeof value.stationary === "boolean"
	) {
		const cycle = value.cycle.map((arc) => {
			if (
				!isRecord(arc) ||
				!hasExactKeys(arc, ["edge_id", "sign"]) ||
				typeof arc.edge_id !== "string" ||
				!["-1", "1"].includes(arc.sign as string)
			) {
				throw new Error(
					"Flow scene contains an invalid minimum-ratio-cycle MCF arc",
				);
			}
			return { edge_id: arc.edge_id, sign: arc.sign as "-1" | "1" };
		});
		if (new Set(cycle.map((arc) => arc.edge_id)).size !== cycle.length) {
			throw new Error(
				"Flow scene minimum-ratio-cycle MCF outcome repeats an edge",
			);
		}
		return {
			kind: "minimum-ratio-cycle-mcf",
			...(value.ratio === undefined
				? {}
				: { ratio: decodeFiniteDecimal(value.ratio) }),
			cycle,
			alpha: decodeFiniteDecimal(value.alpha),
			kappa: decodeFiniteDecimal(value.kappa),
			eta: decodeFiniteDecimal(value.eta),
			potential_decrease: decodeFiniteDecimal(value.potential_decrease),
			guaranteed_decrease: decodeFiniteDecimal(value.guaranteed_decrease),
			stationary: value.stationary,
		};
	}
	if (
		value.kind === "min-cost-flow" &&
		hasExactKeys(value, ["kind", "total_cost", "potentials"]) &&
		typeof value.total_cost === "string" &&
		canonicalI128.test(value.total_cost) &&
		Array.isArray(value.potentials)
	) {
		const potentials = value.potentials.map((potential) => {
			if (
				!isRecord(potential) ||
				!hasExactKeys(potential, ["node_id", "potential"]) ||
				typeof potential.node_id !== "string" ||
				typeof potential.potential !== "string" ||
				!canonicalI128.test(potential.potential)
			) {
				throw new Error("Flow scene contains an invalid dual potential");
			}
			return {
				node_id: potential.node_id,
				potential: potential.potential,
			};
		});
		return {
			kind: "min-cost-flow",
			total_cost: value.total_cost,
			potentials,
		};
	}
	if (
		value.kind === "min-cost-max-flow" &&
		hasExactKeys(value, [
			"kind",
			"value",
			"cut_bound",
			"source_side",
			"total_cost",
			"potentials",
		]) &&
		typeof value.value === "string" &&
		canonicalI128.test(value.value) &&
		typeof value.cut_bound === "string" &&
		canonicalI128.test(value.cut_bound) &&
		Array.isArray(value.source_side) &&
		value.source_side.every((node) => typeof node === "string") &&
		typeof value.total_cost === "string" &&
		canonicalI128.test(value.total_cost) &&
		Array.isArray(value.potentials)
	) {
		const potentials = value.potentials.map((potential) => {
			if (
				!isRecord(potential) ||
				!hasExactKeys(potential, ["node_id", "potential"]) ||
				typeof potential.node_id !== "string" ||
				typeof potential.potential !== "string" ||
				!canonicalI128.test(potential.potential)
			) {
				throw new Error("Flow scene contains an invalid dual potential");
			}
			return {
				node_id: potential.node_id,
				potential: potential.potential,
			};
		});
		return {
			kind: "min-cost-max-flow",
			value: value.value,
			cut_bound: value.cut_bound,
			source_side: value.source_side as string[],
			total_cost: value.total_cost,
			potentials,
		};
	}
	if (
		value.kind === "infeasible" &&
		hasExactKeys(value, ["kind", "unsatisfied", "reachable_original_nodes"]) &&
		typeof value.unsatisfied === "string" &&
		canonicalU64.test(value.unsatisfied) &&
		value.unsatisfied !== "0" &&
		Array.isArray(value.reachable_original_nodes) &&
		value.reachable_original_nodes.every((node) => typeof node === "string")
	) {
		return {
			kind: "infeasible",
			unsatisfied: value.unsatisfied,
			reachable_original_nodes: value.reachable_original_nodes as string[],
		};
	}
	throw new Error("Flow scene contains an invalid outcome");
}

function flowOutcomeMatchesModel(
	model: FlowProblemModelV1,
	outcome: FlowOutcomeV1,
): boolean {
	switch (model.kind) {
		case "max-flow":
			return [
				"max-flow",
				"binary-blocking-flow",
				"electrical-flow",
				"minimum-ratio-cycle",
				"infeasible",
			].includes(outcome.kind);
		case "fixed-flow-min-cost":
		case "circulation":
		case "transshipment":
			return [
				"min-cost-flow",
				"tardos-framework",
				"minimum-ratio-cycle-mcf",
				"infeasible",
			].includes(outcome.kind);
		case "min-cost-max-flow":
			return ["min-cost-max-flow", "infeasible"].includes(outcome.kind);
		case "parametric-max-flow":
			return outcome.kind === "parametric-max-flow";
		case "bipartite-matching":
			return outcome.kind === "bipartite-matching";
		case "assignment":
			return (
				outcome.kind === "assignment" ||
				outcome.kind === "assignment-infeasible"
			);
		case "transportation":
		case "convex-cost-flow":
			return outcome.kind === "min-cost-flow" || outcome.kind === "infeasible";
		case "planar-max-flow":
			return outcome.kind === "max-flow" || outcome.kind === "infeasible";
	}
}

function decodePosition(value: unknown): FlowPositionV1 | undefined {
	if (value === undefined) return undefined;
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["x", "y"]) ||
		typeof value.x !== "string" ||
		!canonicalI64.test(value.x) ||
		typeof value.y !== "string" ||
		!canonicalI64.test(value.y)
	) {
		throw new Error("Flow scene contains an invalid node position");
	}
	return { x: value.x, y: value.y };
}

function decodeNode(value: unknown): FlowNodeV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["id", "supply"], ["position"]) ||
		typeof value.id !== "string" ||
		typeof value.supply !== "string" ||
		!canonicalI64.test(value.supply)
	) {
		throw new Error("Flow scene contains an invalid node");
	}
	const position = decodePosition(value.position);
	return {
		id: value.id,
		supply: value.supply,
		...(position === undefined ? {} : { position }),
	};
}

function decodeEdge(value: unknown): FlowEdgeV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(
			value,
			["id", "from", "to", "lower", "capacity", "cost"],
			["initial_flow", "convex_cost"],
		) ||
		typeof value.id !== "string" ||
		typeof value.from !== "string" ||
		typeof value.to !== "string" ||
		typeof value.lower !== "string" ||
		!canonicalU64.test(value.lower) ||
		typeof value.capacity !== "string" ||
		!canonicalU64.test(value.capacity) ||
		typeof value.cost !== "string" ||
		!canonicalI64.test(value.cost) ||
		(value.initial_flow !== undefined &&
			(typeof value.initial_flow !== "string" ||
				!canonicalU64.test(value.initial_flow)))
	) {
		throw new Error("Flow scene contains an invalid edge");
	}
	let convexCost: FlowEdgeV1["convex_cost"];
	if (value.convex_cost !== undefined) {
		if (
			!isRecord(value.convex_cost) ||
			!hasExactKeys(value.convex_cost, ["base_cost_at_zero", "segments"]) ||
			typeof value.convex_cost.base_cost_at_zero !== "string" ||
			!canonicalI128.test(value.convex_cost.base_cost_at_zero) ||
			!Array.isArray(value.convex_cost.segments)
		) {
			throw new Error("Flow scene contains an invalid convex edge objective");
		}
		convexCost = {
			base_cost_at_zero: value.convex_cost.base_cost_at_zero,
			segments: value.convex_cost.segments.map((segment) => {
				if (
					!isRecord(segment) ||
					!hasExactKeys(segment, ["end_flow", "marginal_cost"]) ||
					typeof segment.end_flow !== "string" ||
					!canonicalU64.test(segment.end_flow) ||
					typeof segment.marginal_cost !== "string" ||
					!canonicalI64.test(segment.marginal_cost)
				) {
					throw new Error("Flow scene contains an invalid convex edge segment");
				}
				return {
					end_flow: segment.end_flow,
					marginal_cost: segment.marginal_cost,
				};
			}),
		};
	}
	return {
		id: value.id,
		from: value.from,
		to: value.to,
		lower: value.lower,
		capacity: value.capacity,
		cost: value.cost,
		...(convexCost === undefined ? {} : { convex_cost: convexCost }),
		...(value.initial_flow === undefined
			? {}
			: { initial_flow: value.initial_flow as string }),
	};
}

function decodePlanarDart(value: unknown): FlowPlanarDartV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["edge_id", "direction"]) ||
		typeof value.edge_id !== "string" ||
		(value.direction !== "forward" && value.direction !== "reverse")
	) {
		throw new Error("Flow scene contains an invalid planar dart");
	}
	return { edge_id: value.edge_id, direction: value.direction };
}

function decodePlanarEmbedding(value: unknown): FlowPlanarEmbeddingV1 {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["rotations", "outer_face"], ["terminal_corners"]) ||
		!Array.isArray(value.rotations)
	) {
		throw new Error("Flow scene contains an invalid planar embedding");
	}
	const rotations = value.rotations.map((rotation) => {
		if (
			!isRecord(rotation) ||
			!hasExactKeys(rotation, ["node_id", "darts"]) ||
			typeof rotation.node_id !== "string" ||
			!Array.isArray(rotation.darts)
		) {
			throw new Error("Flow scene contains an invalid planar rotation");
		}
		return {
			node_id: rotation.node_id,
			darts: rotation.darts.map(decodePlanarDart),
		};
	});
	let terminalCorners: FlowPlanarEmbeddingV1["terminal_corners"];
	if (value.terminal_corners !== undefined) {
		if (
			!isRecord(value.terminal_corners) ||
			!hasExactKeys(value.terminal_corners, ["source", "sink"])
		) {
			throw new Error("Flow scene contains invalid planar terminal corners");
		}
		terminalCorners = {
			source: decodePlanarDart(value.terminal_corners.source),
			sink: decodePlanarDart(value.terminal_corners.sink),
		};
	}
	return {
		rotations,
		outer_face: decodePlanarDart(value.outer_face),
		...(terminalCorners === undefined
			? {}
			: { terminal_corners: terminalCorners }),
	};
}

export function decodeFlowProblemModelV1(value: unknown): FlowProblemModelV1 {
	if (!isRecord(value) || typeof value.kind !== "string") {
		throw new Error("Flow scene contains an invalid model");
	}
	if (
		value.kind === "circulation" ||
		value.kind === "transshipment" ||
		value.kind === "convex-cost-flow"
	) {
		if (!hasExactKeys(value, ["kind"]))
			throw new Error("Flow scene model has unknown fields");
		return { kind: value.kind };
	}
	if (
		value.kind === "bipartite-matching" &&
		(hasExactKeys(value, ["kind", "left", "right"]) ||
			hasExactKeys(value, ["kind", "left", "right", "flow_adapter"])) &&
		Array.isArray(value.left) &&
		Array.isArray(value.right) &&
		value.left.length > 0 &&
		value.right.length > 0 &&
		value.left.every((node) => typeof node === "string") &&
		value.right.every((node) => typeof node === "string")
	) {
		const left = value.left as string[];
		const right = value.right as string[];
		if (
			left.some((node, index) => {
				const previous = left[index - 1];
				return previous !== undefined && previous >= node;
			}) ||
			right.some((node, index) => {
				const previous = right[index - 1];
				return previous !== undefined && previous >= node;
			}) ||
			left.some((node) => right.includes(node))
		) {
			throw new Error("Flow scene contains noncanonical matching partitions");
		}
		let flowAdapter: { source: string; sink: string } | undefined;
		if (value.flow_adapter !== undefined) {
			if (
				!isRecord(value.flow_adapter) ||
				!hasExactKeys(value.flow_adapter, ["source", "sink"]) ||
				typeof value.flow_adapter.source !== "string" ||
				typeof value.flow_adapter.sink !== "string"
			) {
				throw new Error("Flow scene contains an invalid matching flow adapter");
			}
			flowAdapter = {
				source: value.flow_adapter.source,
				sink: value.flow_adapter.sink,
			};
		}
		return {
			kind: "bipartite-matching",
			left,
			right,
			...(flowAdapter === undefined ? {} : { flow_adapter: flowAdapter }),
		};
	}
	if (
		value.kind === "assignment" &&
		hasExactKeys(value, ["kind", "agents", "tasks", "objective"]) &&
		Array.isArray(value.agents) &&
		Array.isArray(value.tasks) &&
		value.agents.length > 0 &&
		value.tasks.length > 0 &&
		value.agents.every((node) => typeof node === "string") &&
		value.tasks.every((node) => typeof node === "string") &&
		["minimize", "maximize"].includes(value.objective as string)
	) {
		const agents = value.agents as string[];
		const tasks = value.tasks as string[];
		if (
			agents.some((node, index) => {
				const previous = agents[index - 1];
				return previous !== undefined && previous >= node;
			}) ||
			tasks.some((node, index) => {
				const previous = tasks[index - 1];
				return previous !== undefined && previous >= node;
			}) ||
			agents.some((node) => tasks.includes(node))
		) {
			throw new Error("Flow scene contains noncanonical assignment partitions");
		}
		return {
			kind: "assignment",
			agents,
			tasks,
			objective: value.objective as "minimize" | "maximize",
		};
	}
	if (
		value.kind === "transportation" &&
		hasExactKeys(value, ["kind", "origins", "destinations"]) &&
		Array.isArray(value.origins) &&
		Array.isArray(value.destinations) &&
		value.origins.length > 0 &&
		value.destinations.length > 0 &&
		value.origins.every((node) => typeof node === "string") &&
		value.destinations.every((node) => typeof node === "string")
	) {
		const origins = value.origins as string[];
		const destinations = value.destinations as string[];
		const noncanonical = (ids: string[]) =>
			ids.some((node, index) => {
				const previous = ids[index - 1];
				return previous !== undefined && previous >= node;
			});
		if (
			noncanonical(origins) ||
			noncanonical(destinations) ||
			origins.some((node) => destinations.includes(node))
		) {
			throw new Error(
				"Flow scene contains noncanonical transportation partitions",
			);
		}
		return { kind: "transportation", origins, destinations };
	}
	if (value.kind === "max-flow" || value.kind === "min-cost-max-flow") {
		if (
			!hasExactKeys(value, ["kind", "source", "sink"]) ||
			typeof value.source !== "string" ||
			typeof value.sink !== "string"
		) {
			throw new Error("Flow scene contains invalid terminals");
		}
		return { kind: value.kind, source: value.source, sink: value.sink };
	}
	if (
		value.kind === "parametric-max-flow" &&
		hasExactKeys(value, [
			"kind",
			"source",
			"sink",
			"parameter",
			"capacity_slopes",
		]) &&
		typeof value.source === "string" &&
		typeof value.sink === "string" &&
		isRecord(value.parameter) &&
		hasExactKeys(value.parameter, ["minimum", "maximum"]) &&
		Array.isArray(value.capacity_slopes)
	) {
		const minimum = decodeRational(value.parameter.minimum);
		const maximum = decodeRational(value.parameter.maximum);
		const capacitySlopes = value.capacity_slopes.map((coefficient) => {
			if (
				!isRecord(coefficient) ||
				!hasExactKeys(coefficient, ["edge_id", "slope"]) ||
				typeof coefficient.edge_id !== "string" ||
				typeof coefficient.slope !== "string" ||
				!canonicalI64.test(coefficient.slope) ||
				coefficient.slope === "0" ||
				coefficient.slope.replace("-", "").length > 128
			) {
				throw new Error(
					"Flow scene contains an invalid parametric coefficient",
				);
			}
			return { edge_id: coefficient.edge_id, slope: coefficient.slope };
		});
		if (
			compareRational(minimum, maximum) > 0 ||
			capacitySlopes.some((coefficient, index) => {
				const previous = capacitySlopes[index - 1];
				return (
					previous !== undefined && previous.edge_id >= coefficient.edge_id
				);
			})
		) {
			throw new Error("Flow scene contains a noncanonical parametric model");
		}
		return {
			kind: "parametric-max-flow",
			source: value.source,
			sink: value.sink,
			parameter: { minimum, maximum },
			capacity_slopes: capacitySlopes,
		};
	}
	if (
		value.kind === "planar-max-flow" &&
		hasExactKeys(value, ["kind", "source", "sink", "embedding"]) &&
		typeof value.source === "string" &&
		typeof value.sink === "string"
	) {
		return {
			kind: "planar-max-flow",
			source: value.source,
			sink: value.sink,
			embedding: decodePlanarEmbedding(value.embedding),
		};
	}
	if (
		value.kind === "fixed-flow-min-cost" &&
		hasExactKeys(value, ["kind", "source", "sink", "required_flow"]) &&
		typeof value.source === "string" &&
		typeof value.sink === "string" &&
		typeof value.required_flow === "string" &&
		canonicalU64.test(value.required_flow)
	) {
		return {
			kind: value.kind,
			source: value.source,
			sink: value.sink,
			required_flow: value.required_flow,
		};
	}
	throw new Error("Flow scene contains an unsupported model");
}

function validateBipartiteScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	outcome: FlowOutcomeV1 | undefined,
): void {
	if (model.kind !== "bipartite-matching") return;
	const left = new Set(model.left);
	const right = new Set(model.right);
	const adapter = model.flow_adapter;
	const expectedNodes = new Set([...model.left, ...model.right]);
	if (adapter !== undefined) {
		if (
			adapter.source === adapter.sink ||
			left.has(adapter.source) ||
			left.has(adapter.sink) ||
			right.has(adapter.source) ||
			right.has(adapter.sink)
		) {
			throw new Error("Flow scene contains invalid matching adapter terminals");
		}
		expectedNodes.add(adapter.source);
		expectedNodes.add(adapter.sink);
	}
	if (
		expectedNodes.size !== nodes.length ||
		nodes.some((node) => !expectedNodes.has(node.id))
	) {
		throw new Error("Flow scene matching partitions do not match graph nodes");
	}

	const flowByEdge = new Map(
		edgeStates.map((state) => [state.edge_id, state.flow]),
	);
	const compatibility = new Map<string, { edge: FlowEdgeV1; flow: string }>();
	const sourceEdges = new Map<string, string>();
	const sinkEdges = new Map<string, string>();
	for (const edge of edges) {
		if (
			edge.lower !== "0" ||
			edge.capacity !== "1" ||
			edge.cost !== "0" ||
			(edge.initial_flow !== undefined && edge.initial_flow !== "0")
		) {
			throw new Error("Flow scene matching graph contains a non-unit edge");
		}
		const flow = flowByEdge.get(edge.id);
		if (flow === undefined || (flow !== "0" && flow !== "1")) {
			throw new Error("Flow scene matching graph contains invalid flow");
		}
		if (left.has(edge.from) && right.has(edge.to)) {
			const pairKey = `${edge.from}\u0000${edge.to}`;
			if (compatibility.has(pairKey)) {
				throw new Error(
					"Flow scene matching graph contains duplicate compatibility",
				);
			}
			compatibility.set(pairKey, { edge, flow });
		} else if (
			adapter !== undefined &&
			edge.from === adapter.source &&
			left.has(edge.to)
		) {
			if (sourceEdges.has(edge.to)) {
				throw new Error(
					"Flow scene matching graph contains duplicate adapter edge",
				);
			}
			sourceEdges.set(edge.to, flow);
		} else if (
			adapter !== undefined &&
			edge.to === adapter.sink &&
			right.has(edge.from)
		) {
			if (sinkEdges.has(edge.from)) {
				throw new Error(
					"Flow scene matching graph contains duplicate adapter edge",
				);
			}
			sinkEdges.set(edge.from, flow);
		} else {
			throw new Error("Flow scene matching graph contains an unexpected edge");
		}
	}
	if (
		adapter !== undefined &&
		(sourceEdges.size !== left.size || sinkEdges.size !== right.size)
	) {
		throw new Error("Flow scene matching graph contains an incomplete adapter");
	}

	const matchedLeft = new Set<string>();
	const matchedRight = new Set<string>();
	const matchedEdgeIds = new Set<string>();
	for (const { edge, flow } of compatibility.values()) {
		if (flow === "0") continue;
		if (
			matchedLeft.has(edge.from) ||
			matchedRight.has(edge.to) ||
			(adapter !== undefined &&
				(sourceEdges.get(edge.from) !== "1" || sinkEdges.get(edge.to) !== "1"))
		) {
			throw new Error("Flow scene matching flow is not a valid matching");
		}
		matchedLeft.add(edge.from);
		matchedRight.add(edge.to);
		matchedEdgeIds.add(edge.id);
	}
	if (
		adapter !== undefined &&
		([...sourceEdges].some(
			([node, flow]) => (flow === "1") !== matchedLeft.has(node),
		) ||
			[...sinkEdges].some(
				([node, flow]) => (flow === "1") !== matchedRight.has(node),
			))
	) {
		throw new Error("Flow scene matching adapter flow is inconsistent");
	}

	if (outcome?.kind !== "bipartite-matching") return;
	const outcomeEdgeIds = new Set(outcome.pairs.map((pair) => pair.edge_id));
	if (
		outcomeEdgeIds.size !== matchedEdgeIds.size ||
		[...outcomeEdgeIds].some((edgeId) => !matchedEdgeIds.has(edgeId))
	) {
		throw new Error("Flow scene matching outcome does not match current flow");
	}
	const coverLeft = new Set(outcome.cover_left);
	const coverRight = new Set(outcome.cover_right);
	if (
		[...compatibility.values()].some(
			({ edge }) => !coverLeft.has(edge.from) && !coverRight.has(edge.to),
		)
	) {
		throw new Error(
			"Flow scene matching cover does not cover every compatibility edge",
		);
	}
}

function validateAssignmentScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	outcome: FlowOutcomeV1 | undefined,
): void {
	if (model.kind !== "assignment") return;
	const agents = new Set(model.agents);
	const tasks = new Set(model.tasks);
	const expectedNodes = new Set([...model.agents, ...model.tasks]);
	if (
		expectedNodes.size !== nodes.length ||
		nodes.some((node) => !expectedNodes.has(node.id) || node.supply !== "0")
	) {
		throw new Error(
			"Flow scene assignment partitions do not match graph nodes",
		);
	}
	const flowByEdge = new Map(
		edgeStates.map((state) => [state.edge_id, state.flow]),
	);
	const allowed = new Map<string, FlowEdgeV1>();
	const selectedEdges = new Set<string>();
	const selectedAgents = new Set<string>();
	const selectedTasks = new Set<string>();
	for (const edge of edges) {
		if (
			edge.lower !== "0" ||
			edge.capacity !== "1" ||
			(edge.initial_flow !== undefined && edge.initial_flow !== "0") ||
			!agents.has(edge.from) ||
			!tasks.has(edge.to)
		) {
			throw new Error("Flow scene assignment graph contains an invalid edge");
		}
		const key = `${edge.from}\u0000${edge.to}`;
		if (allowed.has(key)) {
			throw new Error("Flow scene assignment graph contains a duplicate pair");
		}
		allowed.set(key, edge);
		const flow = flowByEdge.get(edge.id);
		if (flow !== "0" && flow !== "1") {
			throw new Error("Flow scene assignment graph contains invalid flow");
		}
		if (flow === "1") {
			if (selectedAgents.has(edge.from) || selectedTasks.has(edge.to)) {
				throw new Error("Flow scene assignment flow is not injective");
			}
			selectedEdges.add(edge.id);
			selectedAgents.add(edge.from);
			selectedTasks.add(edge.to);
		}
	}
	if (outcome?.kind === "assignment") {
		if (
			outcome.objective !== model.objective ||
			outcome.pairs.length !== model.agents.length ||
			selectedAgents.size !== model.agents.length ||
			outcome.agent_labels.length !== model.agents.length ||
			outcome.task_labels.length !== model.tasks.length ||
			outcome.agent_labels.some(
				(label, index) => label.node_id !== model.agents[index],
			) ||
			outcome.task_labels.some(
				(label, index) => label.node_id !== model.tasks[index],
			)
		) {
			throw new Error("Flow scene assignment outcome has invalid dimensions");
		}
		const agentLabels = new Map(
			outcome.agent_labels.map((item) => [item.node_id, BigInt(item.label)]),
		);
		const taskLabels = new Map(
			outcome.task_labels.map((item) => [item.node_id, BigInt(item.label)]),
		);
		if (
			(model.objective === "minimize" &&
				outcome.task_labels.some((item) => BigInt(item.label) > 0n)) ||
			(model.objective === "maximize" &&
				outcome.task_labels.some((item) => BigInt(item.label) < 0n))
		) {
			throw new Error("Flow scene assignment task-label sign is invalid");
		}
		for (const edge of edges) {
			const labelSum =
				(agentLabels.get(edge.from) ?? 0n) + (taskLabels.get(edge.to) ?? 0n);
			const cost = BigInt(edge.cost);
			const feasible =
				model.objective === "minimize" ? labelSum <= cost : labelSum >= cost;
			if (!feasible || (selectedEdges.has(edge.id) && labelSum !== cost)) {
				throw new Error("Flow scene assignment dual certificate is invalid");
			}
		}
		const pairEdges = new Set<string>();
		let primal = 0n;
		for (const pair of outcome.pairs) {
			const edge = allowed.get(`${pair.agent}\u0000${pair.task}`);
			if (
				edge === undefined ||
				edge.id !== pair.edge_id ||
				edge.cost !== pair.cost ||
				!selectedEdges.has(pair.edge_id) ||
				pairEdges.has(pair.edge_id)
			) {
				throw new Error(
					"Flow scene assignment pairs do not match current flow",
				);
			}
			pairEdges.add(pair.edge_id);
			primal += BigInt(pair.cost);
		}
		const dual = [...agentLabels.values(), ...taskLabels.values()].reduce(
			(sum, label) => sum + label,
			0n,
		);
		if (
			pairEdges.size !== selectedEdges.size ||
			[...selectedEdges].some((edge) => !pairEdges.has(edge)) ||
			primal !== BigInt(outcome.total_cost) ||
			dual !== primal
		) {
			throw new Error("Flow scene assignment primal/dual objectives differ");
		}
	}
	if (outcome?.kind === "assignment-infeasible") {
		const canonical = (values: string[]) =>
			values.every((value, index) => {
				const previous = values[index - 1];
				return previous === undefined || previous < value;
			});
		if (
			outcome.hall_agents.length === 0 ||
			!canonical(outcome.hall_agents) ||
			!canonical(outcome.neighbor_tasks) ||
			outcome.hall_agents.some((agent) => !agents.has(agent)) ||
			outcome.neighbor_tasks.some((task) => !tasks.has(task))
		) {
			throw new Error("Flow scene assignment Hall identities are invalid");
		}
		const hall = new Set(outcome.hall_agents);
		const exactNeighbors = new Set(
			edges.filter((edge) => hall.has(edge.from)).map((edge) => edge.to),
		);
		if (
			exactNeighbors.size !== outcome.neighbor_tasks.length ||
			outcome.neighbor_tasks.some((task) => !exactNeighbors.has(task)) ||
			BigInt(outcome.deficiency) !==
				BigInt(outcome.hall_agents.length - outcome.neighbor_tasks.length) ||
			outcome.hall_agents.length <= outcome.neighbor_tasks.length
		) {
			throw new Error("Flow scene assignment Hall witness is invalid");
		}
	}
}

function validateTransportationScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	outcome: FlowOutcomeV1 | undefined,
): void {
	if (model.kind !== "transportation") return;
	const origins = new Set(model.origins);
	const destinations = new Set(model.destinations);
	const expectedNodes = new Set([...model.origins, ...model.destinations]);
	const supplyByNode = new Map(
		nodes.map((node) => [node.id, BigInt(node.supply)]),
	);
	if (
		expectedNodes.size !== nodes.length ||
		nodes.some((node) => !expectedNodes.has(node.id)) ||
		model.origins.some((node) => (supplyByNode.get(node) ?? 0n) <= 0n) ||
		model.destinations.some((node) => (supplyByNode.get(node) ?? 0n) >= 0n)
	) {
		throw new Error(
			"Flow scene transportation partitions do not match supplies",
		);
	}
	const totalSupply = model.origins.reduce(
		(sum, node) => sum + (supplyByNode.get(node) ?? 0n),
		0n,
	);
	const totalDemand = model.destinations.reduce(
		(sum, node) => sum - (supplyByNode.get(node) ?? 0n),
		0n,
	);
	if (totalSupply !== totalDemand) {
		throw new Error(
			"Flow scene transportation supply and demand are unbalanced",
		);
	}
	const pairs = new Set<string>();
	for (const edge of edges) {
		const supply = supplyByNode.get(edge.from);
		const demand = supplyByNode.get(edge.to);
		const requiredCapacity =
			supply === undefined || demand === undefined
				? -1n
				: supply < -demand
					? supply
					: -demand;
		const pair = `${edge.from}\u0000${edge.to}`;
		if (
			edge.lower !== "0" ||
			(edge.initial_flow !== undefined && edge.initial_flow !== "0") ||
			!origins.has(edge.from) ||
			!destinations.has(edge.to) ||
			BigInt(edge.capacity) < requiredCapacity ||
			pairs.has(pair)
		) {
			throw new Error(
				"Flow scene transportation graph contains an invalid route",
			);
		}
		pairs.add(pair);
	}
	if (outcome?.kind !== "min-cost-flow") return;
	const flowByEdge = new Map(
		edgeStates.map((state) => [state.edge_id, BigInt(state.flow)]),
	);
	const divergence = new Map(nodes.map((node) => [node.id, 0n]));
	let totalCost = 0n;
	for (const edge of edges) {
		const flow = flowByEdge.get(edge.id) ?? 0n;
		divergence.set(edge.from, (divergence.get(edge.from) ?? 0n) + flow);
		divergence.set(edge.to, (divergence.get(edge.to) ?? 0n) - flow);
		totalCost += flow * BigInt(edge.cost);
	}
	if (
		nodes.some(
			(node) => (divergence.get(node.id) ?? 0n) !== BigInt(node.supply),
		) ||
		totalCost !== BigInt(outcome.total_cost)
	) {
		throw new Error("Flow scene transportation optimum is primal-infeasible");
	}
	const potential = new Map(
		outcome.potentials.map((item) => [item.node_id, BigInt(item.potential)]),
	);
	for (const edge of edges) {
		const flow = flowByEdge.get(edge.id) ?? 0n;
		const reduced =
			BigInt(edge.cost) +
			(potential.get(edge.from) ?? 0n) -
			(potential.get(edge.to) ?? 0n);
		if (
			(flow < BigInt(edge.capacity) && reduced < 0n) ||
			(flow > 0n && reduced > 0n)
		) {
			throw new Error("Flow scene transportation dual certificate is invalid");
		}
	}
}

export function planarDartKey(dart: FlowPlanarDartV1): string {
	return `${dart.edge_id}\u0000${dart.direction}`;
}

function reversePlanarDartKey(dart: FlowPlanarDartV1): string {
	return `${dart.edge_id}\u0000${dart.direction === "forward" ? "reverse" : "forward"}`;
}

export type FlowPlanarTopology = Readonly<{
	faces: readonly (readonly FlowPlanarDartV1[])[];
	leftFaceByDart: ReadonlyMap<string, number>;
	outerFace: number;
}>;

export function derivePlanarTopology(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
): FlowPlanarTopology | undefined {
	if (model.kind !== "planar-max-flow") return undefined;
	const nodeIds = new Set(nodes.map((node) => node.id));
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	if (
		model.source === model.sink ||
		!nodeIds.has(model.source) ||
		!nodeIds.has(model.sink) ||
		nodes.length === 0 ||
		edges.length === 0 ||
		nodes.some((node) => node.supply !== "0") ||
		edges.some(
			(edge) =>
				edge.lower !== "0" ||
				(edge.initial_flow !== undefined && edge.initial_flow !== "0"),
		) ||
		edgeById.size !== edges.length
	) {
		throw new Error("Flow scene planar model violates max-flow semantics");
	}

	const expectedNodes = nodes
		.map((node) => node.id)
		.sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
	const { rotations } = model.embedding;
	if (
		rotations.length !== expectedNodes.length ||
		rotations.some(
			(rotation, index) =>
				rotation.node_id !== expectedNodes[index] ||
				rotation.darts.length === 0,
		)
	) {
		throw new Error("Flow scene planar rotations are not canonical");
	}

	const seenDarts = new Set<string>();
	const successor = new Map<string, FlowPlanarDartV1>();
	const dartTail = (dart: FlowPlanarDartV1): string | undefined => {
		const edge = edgeById.get(dart.edge_id);
		if (edge === undefined) return undefined;
		return dart.direction === "forward" ? edge.from : edge.to;
	};
	for (const rotation of rotations) {
		for (let index = 0; index < rotation.darts.length; index += 1) {
			const dart = rotation.darts[index];
			const next = rotation.darts[(index + 1) % rotation.darts.length];
			if (dart === undefined || next === undefined) {
				throw new Error("Flow scene planar rotation is empty");
			}
			const key = planarDartKey(dart);
			if (dartTail(dart) !== rotation.node_id || seenDarts.has(key)) {
				throw new Error("Flow scene planar rotation contains an invalid dart");
			}
			seenDarts.add(key);
			successor.set(key, next);
		}
	}
	const expectedDarts = edges.flatMap((edge) => [
		{ edge_id: edge.id, direction: "forward" as const },
		{ edge_id: edge.id, direction: "reverse" as const },
	]);
	if (
		seenDarts.size !== expectedDarts.length ||
		expectedDarts.some((dart) => !seenDarts.has(planarDartKey(dart)))
	) {
		throw new Error("Flow scene planar embedding omits a dart");
	}

	const adjacency = new Map(nodes.map((node) => [node.id, [] as string[]]));
	for (const edge of edges) {
		adjacency.get(edge.from)?.push(edge.to);
		adjacency.get(edge.to)?.push(edge.from);
	}
	const reached = new Set<string>();
	const first = expectedNodes[0];
	if (first !== undefined) {
		reached.add(first);
		const queue = [first];
		for (let cursor = 0; cursor < queue.length; cursor += 1) {
			const current = queue[cursor];
			if (current === undefined) continue;
			for (const neighbor of adjacency.get(current) ?? []) {
				if (reached.has(neighbor)) continue;
				reached.add(neighbor);
				queue.push(neighbor);
			}
		}
	}
	if (reached.size !== nodes.length) {
		throw new Error("Flow scene planar embedding graph is disconnected");
	}

	const leftFace = new Map<string, number>();
	const faces: FlowPlanarDartV1[][] = [];
	let faceCount = 0;
	for (const startDart of expectedDarts) {
		const start = planarDartKey(startDart);
		if (leftFace.has(start)) continue;
		let current = startDart;
		const boundary: FlowPlanarDartV1[] = [];
		for (let traversed = 0; ; traversed += 1) {
			if (traversed > expectedDarts.length) {
				throw new Error("Flow scene planar face permutation does not close");
			}
			const key = planarDartKey(current);
			if (leftFace.has(key)) {
				if (key !== start) {
					throw new Error("Flow scene planar faces overlap");
				}
				break;
			}
			leftFace.set(key, faceCount);
			boundary.push(current);
			const next = successor.get(reversePlanarDartKey(current));
			if (next === undefined) {
				throw new Error("Flow scene planar face successor is missing");
			}
			current = next;
		}
		faces.push(boundary);
		faceCount += 1;
	}
	if (nodes.length - edges.length + faceCount !== 2) {
		throw new Error("Flow scene rotation system is not planar");
	}

	const outerFace = leftFace.get(planarDartKey(model.embedding.outer_face));
	if (outerFace === undefined) {
		throw new Error("Flow scene planar outer-face anchor is invalid");
	}
	const corners = model.embedding.terminal_corners;
	if (
		corners !== undefined &&
		(dartTail(corners.source) !== model.source ||
			dartTail(corners.sink) !== model.sink ||
			leftFace.get(planarDartKey(corners.source)) !== outerFace ||
			leftFace.get(planarDartKey(corners.sink)) !== outerFace)
	) {
		throw new Error("Flow scene planar terminal corners are invalid");
	}
	return { faces, leftFaceByDart: leftFace, outerFace };
}

export function validatePlanarScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
): void {
	derivePlanarTopology(model, nodes, edges);
}

function rationalEqual(left: FlowRationalV1, right: FlowRationalV1): boolean {
	return compareRational(left, right) === 0;
}

function orderedStringsEqual(
	left: readonly string[],
	right: readonly string[],
): boolean {
	return (
		left.length === right.length &&
		left.every((value, index) => value === right[index])
	);
}

function parametricSegmentEqual(
	left: FlowParametricSegmentV1,
	right: FlowParametricSegmentV1,
): boolean {
	return (
		rationalEqual(left.lower, right.lower) &&
		rationalEqual(left.upper, right.upper) &&
		left.intercept === right.intercept &&
		left.slope === right.slope &&
		orderedStringsEqual(left.minimal_source_side, right.minimal_source_side) &&
		orderedStringsEqual(left.maximal_source_side, right.maximal_source_side)
	);
}

function parametricBreakpointEqual(
	left: FlowParametricBreakpointV1,
	right: FlowParametricBreakpointV1,
): boolean {
	return (
		rationalEqual(left.parameter, right.parameter) &&
		orderedStringsEqual(left.before_source_side, right.before_source_side) &&
		orderedStringsEqual(left.after_source_side, right.after_source_side) &&
		orderedStringsEqual(
			left.exact_minimal_source_side,
			right.exact_minimal_source_side,
		) &&
		orderedStringsEqual(
			left.exact_maximal_source_side,
			right.exact_maximal_source_side,
		) &&
		orderedStringsEqual(left.entering_nodes, right.entering_nodes)
	);
}

function orderedParametricAnalysisEqual<T>(
	left: readonly T[],
	right: readonly T[],
	equal: (left: T, right: T) => boolean,
): boolean {
	return (
		left.length === right.length &&
		left.every((value, index) => {
			const other = right[index];
			return other !== undefined && equal(value, other);
		})
	);
}

function validateParametricScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	residualArcs: FlowResidualArcStateV1[],
	nodeTraceStates: FlowNodeTraceStateV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	overlay: FlowParametricOverlayV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	if (model.kind !== "parametric-max-flow") {
		if (overlay !== undefined || outcome?.kind === "parametric-max-flow") {
			throw new Error("Parametric scene data requires its matching model");
		}
		return;
	}
	if (
		!["parametric-pseudoflow", "parametric-breakpoint-rerun"].includes(
			algorithmId,
		) ||
		overlay === undefined ||
		edgeStates.length !== 0 ||
		residualArcs.length !== 0 ||
		nodeTraceStates.length !== 0 ||
		model.source === model.sink
	) {
		throw new Error("Parametric scene has an incompatible execution boundary");
	}
	const nodeIds = new Set(nodes.map((node) => node.id));
	if (
		!nodeIds.has(model.source) ||
		!nodeIds.has(model.sink) ||
		nodes.some((node) => node.supply !== "0") ||
		edges.some(
			(edge) =>
				edge.lower !== "0" ||
				edge.cost !== "0" ||
				(edge.initial_flow !== undefined && edge.initial_flow !== "0"),
		)
	) {
		throw new Error("Parametric scene graph violates the plain max-flow model");
	}
	const slopeByEdge = new Map(
		model.capacity_slopes.map((coefficient) => [
			coefficient.edge_id,
			BigInt(coefficient.slope),
		]),
	);
	if (slopeByEdge.size !== model.capacity_slopes.length) {
		throw new Error("Parametric scene contains duplicate coefficients");
	}
	for (const coefficient of model.capacity_slopes) {
		const edge = edges.find(
			(candidate) => candidate.id === coefficient.edge_id,
		);
		const slope = BigInt(coefficient.slope);
		const leavesSource =
			edge?.from === model.source && edge.to !== model.source;
		const entersSink = edge?.to === model.sink && edge.from !== model.sink;
		if (
			edge === undefined ||
			(leavesSource && entersSink) ||
			(!leavesSource && !entersSink) ||
			(leavesSource && slope <= 0n) ||
			(entersSink && slope >= 0n)
		) {
			throw new Error(
				"Parametric scene coefficient is not monotone terminal data",
			);
		}
	}
	const evaluate = (edge: FlowEdgeV1, parameter: FlowRationalV1) => ({
		numerator:
			BigInt(edge.capacity) * BigInt(parameter.denominator) +
			(slopeByEdge.get(edge.id) ?? 0n) * BigInt(parameter.numerator),
		denominator: BigInt(parameter.denominator),
	});
	const equalsRaw = (
		value: FlowRationalV1,
		raw: { numerator: bigint; denominator: bigint },
	) =>
		BigInt(value.numerator) * raw.denominator ===
		raw.numerator * BigInt(value.denominator);
	const endpointCapacities = edges.flatMap((edge) => [
		evaluate(edge, model.parameter.minimum),
		evaluate(edge, model.parameter.maximum),
	]);
	if (endpointCapacities.some((capacity) => capacity.numerator < 0n)) {
		throw new Error("Parametric scene capacity is negative on its domain");
	}
	const maximumCapacity = endpointCapacities.reduce(
		(maximum, capacity) =>
			capacity.numerator * maximum.denominator >
			maximum.numerator * capacity.denominator
				? capacity
				: maximum,
		{ numerator: 0n, denominator: 1n },
	);
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	if (
		compareRational(overlay.parameter, model.parameter.minimum) < 0 ||
		compareRational(overlay.parameter, model.parameter.maximum) > 0 ||
		!equalsRaw(overlay.visual_scale_max_capacity, maximumCapacity) ||
		overlay.edge_capacities.length !== edges.length ||
		overlay.edge_capacities.some((capacity) => {
			const edge = edgeById.get(capacity.edge_id);
			return (
				edge === undefined ||
				!equalsRaw(capacity.capacity, evaluate(edge, overlay.parameter))
			);
		})
	) {
		throw new Error(
			"Parametric scene exact capacities or fixed scale disagree",
		);
	}
	const canonicalIds = canonicalNodeIds(nodes);
	const validateSourceSide = (sourceSide: string[]) => {
		const membership = new Set(sourceSide);
		return (
			membership.size === sourceSide.length &&
			membership.has(model.source) &&
			!membership.has(model.sink) &&
			sourceSide.every((node) => nodeIds.has(node)) &&
			canonicalIds
				.filter((node) => membership.has(node))
				.every((node, index) => sourceSide[index] === node)
		);
	};
	const validateSegment = (segment: FlowParametricSegmentV1) => {
		const minimal = new Set(segment.minimal_source_side);
		return (
			compareRational(segment.lower, model.parameter.minimum) >= 0 &&
			compareRational(segment.upper, model.parameter.maximum) <= 0 &&
			validateSourceSide(segment.minimal_source_side) &&
			validateSourceSide(segment.maximal_source_side) &&
			segment.minimal_source_side.every((node) =>
				new Set(segment.maximal_source_side).has(node),
			) &&
			minimal.size === segment.minimal_source_side.length
		);
	};
	const validateBreakpoint = (breakpoint: FlowParametricBreakpointV1) => {
		const before = new Set(breakpoint.before_source_side);
		const after = new Set(breakpoint.after_source_side);
		const exactMinimal = new Set(breakpoint.exact_minimal_source_side);
		const exactMaximal = new Set(breakpoint.exact_maximal_source_side);
		const entering = breakpoint.after_source_side.filter(
			(node) => !before.has(node),
		);
		return (
			compareRational(breakpoint.parameter, model.parameter.minimum) >= 0 &&
			compareRational(breakpoint.parameter, model.parameter.maximum) <= 0 &&
			validateSourceSide(breakpoint.before_source_side) &&
			validateSourceSide(breakpoint.after_source_side) &&
			validateSourceSide(breakpoint.exact_minimal_source_side) &&
			validateSourceSide(breakpoint.exact_maximal_source_side) &&
			[...before].every((node) => after.has(node)) &&
			[...exactMinimal].every((node) => exactMaximal.has(node)) &&
			entering.length === breakpoint.entering_nodes.length &&
			entering.every((node, index) => breakpoint.entering_nodes[index] === node)
		);
	};
	if (
		(overlay.traversal === undefined
			? overlay.stage !== (solveStatus === "optimal" ? "optimal" : "ready")
			: overlay.stage !== overlay.traversal.kind) ||
		overlay.recorded_segments.some((segment, index, segments) => {
			const previous = segments[index - 1];
			return (
				!validateSegment(segment) ||
				(previous !== undefined &&
					compareRational(previous.lower, segment.lower) >= 0)
			);
		}) ||
		overlay.recorded_breakpoints.some((breakpoint, index, breakpoints) => {
			const previous = breakpoints[index - 1];
			return (
				!validateBreakpoint(breakpoint) ||
				(previous !== undefined &&
					compareRational(previous.parameter, breakpoint.parameter) >= 0)
			);
		}) ||
		(overlay.traversal !== undefined &&
			((overlay.traversal.probe !== undefined &&
				!rationalEqual(overlay.parameter, overlay.traversal.probe)) ||
				(overlay.traversal.probe === undefined &&
					!rationalEqual(
						overlay.parameter,
						overlay.traversal.kind === "optimal"
							? overlay.traversal.upper
							: overlay.traversal.lower,
					))))
	) {
		throw new Error(
			"Parametric scene traversal or recorded analysis is invalid",
		);
	}
	if (outcome !== undefined && outcome.kind !== "parametric-max-flow") {
		throw new Error("Parametric scene contains a nonparametric outcome");
	}
	if (outcome?.kind === "parametric-max-flow") {
		const segments = outcome.segments;
		const breakpoints = outcome.breakpoints;
		const invalidCoverage =
			solveStatus !== "optimal" ||
			segments.length === 0 ||
			!rationalEqual(
				segments[0]?.lower ?? model.parameter.maximum,
				model.parameter.minimum,
			) ||
			!rationalEqual(
				segments[segments.length - 1]?.upper ?? model.parameter.minimum,
				model.parameter.maximum,
			) ||
			segments.some((segment, index) => {
				const previous = segments[index - 1];
				return (
					!validateSegment(segment) ||
					(previous !== undefined &&
						!rationalEqual(previous.upper, segment.lower))
				);
			}) ||
			breakpoints.length !== Math.max(0, segments.length - 1) ||
			breakpoints.some((breakpoint, index) => {
				const left = segments[index];
				const right = segments[index + 1];
				return (
					!validateBreakpoint(breakpoint) ||
					left === undefined ||
					right === undefined ||
					!rationalEqual(left.upper, breakpoint.parameter) ||
					!rationalEqual(right.lower, breakpoint.parameter) ||
					!orderedStringsEqual(
						left.minimal_source_side,
						breakpoint.before_source_side,
					) ||
					!orderedStringsEqual(
						right.minimal_source_side,
						breakpoint.after_source_side,
					)
				);
			});
		if (invalidCoverage) {
			throw new Error("Parametric optimum segment coverage is invalid");
		}
		if (
			!orderedParametricAnalysisEqual(
				overlay.recorded_segments,
				segments,
				parametricSegmentEqual,
			) ||
			!orderedParametricAnalysisEqual(
				overlay.recorded_breakpoints,
				breakpoints,
				parametricBreakpointEqual,
			)
		) {
			throw new Error(
				"Parametric recorded analysis disagrees with its optimum",
			);
		}
		if (
			(outcome.metrics.implementation === "parametric-pseudoflow") !==
			(algorithmId === "parametric-pseudoflow")
		) {
			throw new Error("Parametric outcome implementation is invalid");
		}
	}
}

function validateBinaryBlockingScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	residualArcs: FlowResidualArcStateV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowBinaryBlockingOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "binary-blocking-flow";
	if (!selected) {
		if (overlay !== undefined || outcome?.kind === "binary-blocking-flow") {
			throw new Error("Binary blocking projection uses the wrong algorithm");
		}
		return;
	}
	if (model.kind !== "max-flow") {
		throw new Error("Binary blocking primitive requires the max-flow model");
	}
	if (eventId === "0" && solveStatus === "ready") {
		if (overlay !== undefined || outcome !== undefined) {
			throw new Error("Ready binary blocking scene contains computed state");
		}
		return;
	}
	if (overlay === undefined) {
		throw new Error("Binary blocking scene is missing its structural overlay");
	}
	const orderedNodes = canonicalNodeIds(nodes);
	if (
		overlay.nodes.length !== orderedNodes.length ||
		overlay.nodes.some((node, index) => node.node_id !== orderedNodes[index]) ||
		BigInt(overlay.delta) > BigInt(overlay.upper_bound) ||
		(overlay.stage !== "complete" && overlay.delivered !== "0")
	) {
		throw new Error("Binary blocking node or phase projection is invalid");
	}
	const components = new Map<string, number>();
	for (const node of overlay.nodes) {
		components.set(node.component, (components.get(node.component) ?? 0) + 1);
	}
	const componentOrdinals = [...components.keys()]
		.map(BigInt)
		.sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
	if (
		componentOrdinals.some((component, index) => component !== BigInt(index))
	) {
		throw new Error("Binary blocking SCC ordinals are not canonical");
	}
	const edgeIds = new Set(edges.map((edge) => edge.id));
	const residualIds = new Set(
		residualArcs.map((arc) => `${arc.edge_id}\u0000${arc.direction}`),
	);
	const arcSet = (
		arcs: FlowBinaryBlockingOverlayV1["admissible_arcs"],
	): Set<string> => {
		const keys = arcs.map((arc) => `${arc.edge_id}\u0000${arc.direction}`);
		const unique = new Set(keys);
		if (
			unique.size !== keys.length ||
			arcs.some(
				(arc) =>
					!edgeIds.has(arc.edge_id) ||
					!residualIds.has(`${arc.edge_id}\u0000${arc.direction}`),
			)
		) {
			throw new Error("Binary blocking residual classification is invalid");
		}
		return unique;
	};
	const baseZero = arcSet(overlay.base_zero_arcs);
	const special = arcSet(overlay.special_arcs);
	const admissible = arcSet(overlay.admissible_arcs);
	const zeroAdmissible = arcSet(overlay.zero_admissible_arcs);
	if (
		[...zeroAdmissible].some((arc) => !admissible.has(arc)) ||
		[...zeroAdmissible].some((arc) => !baseZero.has(arc) && !special.has(arc))
	) {
		throw new Error("Binary blocking admissible classes are inconsistent");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const stageMatches =
			(overlay.stage === "analyzing" &&
				[
					"binary-blocking-flow.inspect-initial-cut-arc",
					"binary-blocking-flow.inspect-residual-arc",
					"binary-blocking-flow.build-reverse-zero-one-adjacency",
					"binary-blocking-flow.relax-binary-distance",
					"binary-blocking-flow.inspect-binary-length",
					"binary-blocking-flow.build-zero-scc-adjacency",
					"binary-blocking-flow.inspect-zero-scc-reverse-arc",
					"binary-blocking-flow.inspect-canonical-cut-arc",
				].includes(traceEvent.catalog_id)) ||
			(overlay.stage === "analyzed" &&
				traceEvent.catalog_id ===
					"binary-blocking-flow.analyze-binary-network") ||
			(overlay.stage === "contracted" &&
				[
					"binary-blocking-flow.contract-zero-scc",
					"binary-blocking-flow.inspect-contracted-arc",
					"binary-blocking-flow.build-lift-adjacency",
					"binary-blocking-flow.inspect-lift-arc",
					"binary-blocking-flow.apply-contracted-flow",
					"binary-blocking-flow.apply-lift-path",
				].includes(traceEvent.catalog_id)) ||
			(overlay.stage === "complete" &&
				traceEvent.catalog_id === "binary-blocking-flow.complete-primitive");
		if (!stageMatches) {
			throw new Error("Binary blocking event and stage disagree");
		}
	}
	const active = new Set(
		residualArcs
			.filter((arc) => arc.active)
			.map((arc) => `${arc.edge_id}\u0000${arc.direction}`),
	);
	const inspectionDetailLabel =
		traceEvent === undefined
			? undefined
			: (
					{
						"binary-blocking-flow.inspect-initial-cut-arc":
							"initial-cut-capacity",
						"binary-blocking-flow.inspect-residual-arc": "residual-capacity",
						"binary-blocking-flow.build-reverse-zero-one-adjacency":
							"base-binary-length",
						"binary-blocking-flow.relax-binary-distance": "candidate-distance",
						"binary-blocking-flow.inspect-binary-length": "binary-length",
						"binary-blocking-flow.build-zero-scc-adjacency":
							"scc-adjacency-target",
						"binary-blocking-flow.inspect-zero-scc-reverse-arc":
							"scc-reverse-target",
						"binary-blocking-flow.inspect-canonical-cut-arc":
							"canonical-cut-level",
						"binary-blocking-flow.inspect-contracted-arc":
							"contracted-residual-capacity",
						"binary-blocking-flow.build-lift-adjacency": "lift-component",
						"binary-blocking-flow.inspect-lift-arc": "lift-component",
					} as Record<string, string>
				)[traceEvent.catalog_id];
	const inspecting = inspectionDetailLabel !== undefined;
	const touchedResidualArcs = new Set(
		traceEvent?.entity_refs.flatMap((entity) =>
			entity.kind === "residual-arc"
				? [`${entity.edge_id}\u0000${entity.direction}`]
				: [],
		) ?? [],
	);
	const applyingFlow =
		traceEvent !== undefined &&
		[
			"binary-blocking-flow.apply-contracted-flow",
			"binary-blocking-flow.apply-lift-path",
		].includes(traceEvent.catalog_id);
	const expectedActive = inspecting
		? undefined
		: traceEvent?.catalog_id === "binary-blocking-flow.analyze-binary-network"
			? admissible
			: traceEvent?.catalog_id === "binary-blocking-flow.contract-zero-scc"
				? zeroAdmissible
				: undefined;
	if (
		(inspecting &&
			(active.size !== 1 ||
				[...active].some((arc) => !touchedResidualArcs.has(arc)) ||
				!traceDetailKeepsSourceLabel(
					traceEvent?.detail,
					inspectionDetailLabel,
				) ||
				(["base-binary-length", "binary-length"].includes(
					inspectionDetailLabel,
				) &&
					!["0", "1"].includes(traceEvent?.detail?.value ?? "")))) ||
		(expectedActive !== undefined &&
			(active.size !== expectedActive.size ||
				[...active].some((arc) => !expectedActive.has(arc)))) ||
		(applyingFlow &&
			(active.size === 0 ||
				active.size !== touchedResidualArcs.size ||
				[...active].some(
					(arc) => !touchedResidualArcs.has(arc) || !admissible.has(arc),
				))) ||
		(!inspecting &&
			!applyingFlow &&
			expectedActive === undefined &&
			[...active].some((arc) => !admissible.has(arc)))
	) {
		throw new Error("Binary blocking active residual projection is invalid");
	}
	if (outcome?.kind === "binary-blocking-flow") {
		const nontrivial = [...components.values()].filter(
			(count) => count > 1,
		).length;
		if (
			solveStatus !== "primitive-complete" ||
			overlay.stage !== "complete" ||
			outcome.upper_bound !== overlay.upper_bound ||
			outcome.delta !== overlay.delta ||
			outcome.delivered !== overlay.delivered ||
			BigInt(outcome.component_count) !== BigInt(components.size) ||
			BigInt(outcome.nontrivial_component_count) !== BigInt(nontrivial) ||
			(outcome.termination === "blocking"
				? BigInt(outcome.delivered) >= BigInt(outcome.delta)
				: outcome.delivered !== outcome.delta)
		) {
			throw new Error("Binary blocking primitive outcome is inconsistent");
		}
	} else if (solveStatus === "primitive-complete") {
		throw new Error(
			"Completed binary blocking primitive is missing its outcome",
		);
	}
}

function validateCancelTightenScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	residualArcs: FlowResidualArcStateV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowCancelTightenOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
): void {
	const selected = algorithmId === "cancel-and-tighten";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Cancel-and-Tighten overlay uses the wrong algorithm");
		}
		return;
	}
	if (
		model.kind !== "fixed-flow-min-cost" &&
		model.kind !== "circulation" &&
		model.kind !== "transshipment"
	) {
		throw new Error("Cancel-and-Tighten requires a minimum-cost-flow model");
	}
	if (eventId === "0" && solveStatus === "ready" && overlay === undefined)
		return;
	if (overlay === undefined) {
		throw new Error("Cancel-and-Tighten scene is missing its exact overlay");
	}
	const orderedNodes = canonicalNodeIds(nodes);
	if (
		overlay.nodes.length !== orderedNodes.length ||
		overlay.nodes.some((node, index) => node.node_id !== orderedNodes[index]) ||
		((overlay.stage === "ready" || overlay.stage === "initialize") &&
			overlay.phase !== "0") ||
		(!["ready", "initialize", "optimal"].includes(overlay.stage) &&
			overlay.phase === "0") ||
		(overlay.stage === "cancel-cycle") !== (overlay.delta !== undefined) ||
		(solveStatus === "optimal") !== (overlay.stage === "optimal")
	) {
		throw new Error("Cancel-and-Tighten boundary metadata is inconsistent");
	}
	const ranks = overlay.nodes.map((node) => node.rank);
	if (overlay.stage === "tighten") {
		const rankValues = ranks.map((rank) => {
			if (rank === undefined) {
				throw new Error(
					"Cancel-and-Tighten tighten boundary is missing a rank",
				);
			}
			return BigInt(rank);
		});
		const distinct = new Set(rankValues.map(String));
		if (
			distinct.size !== nodes.length ||
			rankValues.some((rank) => rank < 0n || rank >= BigInt(nodes.length))
		) {
			throw new Error("Cancel-and-Tighten ranks are not a permutation");
		}
	} else if (ranks.some((rank) => rank !== undefined)) {
		throw new Error(
			"Cancel-and-Tighten ranks leaked outside a tighten boundary",
		);
	}
	const prices = new Map(
		overlay.nodes.map((node) => [node.node_id, node.potential]),
	);
	const reducedNumerator = (arc: FlowResidualArcStateV1): bigint => {
		const from = prices.get(arc.from);
		const to = prices.get(arc.to);
		if (from === undefined || to === undefined) {
			throw new Error("Cancel-and-Tighten price references an unknown node");
		}
		const fromDenominator = BigInt(from.denominator);
		const toDenominator = BigInt(to.denominator);
		return (
			BigInt(arc.cost) * fromDenominator * toDenominator +
			BigInt(from.numerator) * toDenominator -
			BigInt(to.numerator) * fromDenominator
		);
	};
	const epsilonOptimal = residualArcs.every((arc) => {
		if (BigInt(arc.capacity) === 0n) return true;
		const from = prices.get(arc.from);
		const to = prices.get(arc.to);
		if (from === undefined || to === undefined) return false;
		const fromDenominator = BigInt(from.denominator);
		const toDenominator = BigInt(to.denominator);
		const epsilonDenominator = BigInt(overlay.epsilon.denominator);
		return (
			reducedNumerator(arc) * epsilonDenominator +
				BigInt(overlay.epsilon.numerator) * fromDenominator * toDenominator >=
			0n
		);
	});
	if (!epsilonOptimal) {
		throw new Error("Cancel-and-Tighten snapshot is not epsilon-optimal");
	}
	const residualById = new Map(
		residualArcs.map((arc) => [`${arc.edge_id}\u0000${arc.direction}`, arc]),
	);
	const arcKeys = (arcs: FlowResidualArcRefV1[]): string[] =>
		arcs.map((arc) => `${arc.edge_id}\u0000${arc.direction}`);
	const admissibleKeys = arcKeys(overlay.admissible_arcs);
	const expectedAdmissible = residualArcs
		.filter((arc) => BigInt(arc.capacity) > 0n && reducedNumerator(arc) < 0n)
		.map((arc) => `${arc.edge_id}\u0000${arc.direction}`)
		.sort();
	if (
		new Set(admissibleKeys).size !== admissibleKeys.length ||
		JSON.stringify(admissibleKeys) !== JSON.stringify(expectedAdmissible)
	) {
		throw new Error("Cancel-and-Tighten admissible residual set is incorrect");
	}
	// A tighten boundary stores the topological rank of the admissible graph
	// immediately before the price update. `admissible_arcs` is reconstructed
	// from the post-update residual state, where newly admissible arcs may run
	// against that old rank. The Rust trace checker validates the temporal
	// topological-order contract using both boundary snapshots; this decoder can
	// truthfully validate only the rank permutation carried by the current frame.
	const cycleKeys = arcKeys(overlay.active_cycle);
	if (
		new Set(cycleKeys).size !== cycleKeys.length ||
		cycleKeys.some((key) => !residualById.has(key)) ||
		(["select-cycle", "cancel-cycle"].includes(overlay.stage)
			? cycleKeys.length === 0
			: cycleKeys.length !== 0)
	) {
		throw new Error("Cancel-and-Tighten active cycle identity is invalid");
	}
	if (cycleKeys.length > 0) {
		const cycle = cycleKeys.map((key) => residualById.get(key));
		if (
			cycle.some((arc) => arc === undefined) ||
			cycle.some((arc, index) => {
				const next = cycle[(index + 1) % cycle.length];
				return arc?.to !== next?.from;
			}) ||
			(overlay.stage === "select-cycle" &&
				cycle.some(
					(arc) =>
						arc === undefined ||
						BigInt(arc.capacity) === 0n ||
						reducedNumerator(arc) >= 0n,
				))
		) {
			throw new Error("Cancel-and-Tighten active cycle is not admissible");
		}
	}
	const inspectedKeys = arcKeys(overlay.inspected_arcs);
	const inspectionStage = ["inspect-cycle-arc", "inspect-rank-arc"].includes(
		overlay.stage,
	);
	if (
		(inspectionStage
			? inspectedKeys.length !== 1
			: inspectedKeys.length !== 0) ||
		inspectedKeys.some((key) => {
			const arc = residualById.get(key);
			return arc === undefined || BigInt(arc.capacity) === 0n;
		})
	) {
		throw new Error(
			"Cancel-and-Tighten inspected residual identity is invalid",
		);
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const expectedCatalog = `cancel-and-tighten.${
			{
				initialize: "initialize",
				"begin-phase": "begin-phase",
				"inspect-cycle-arc": "inspect-cycle-residual-arc",
				"select-cycle": "select-admissible-cycle",
				"cancel-cycle": "cancel-admissible-cycle",
				"inspect-rank-arc": "inspect-ranking-residual-arc",
				tighten: "tighten",
				optimal: "optimal",
				ready: "ready",
			}[overlay.stage]
		}`;
		if (traceEvent.catalog_id !== expectedCatalog) {
			throw new Error("Cancel-and-Tighten event and stage disagree");
		}
	}
}

function validateRelaxedMndcScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	residualArcs: FlowResidualArcStateV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowRelaxedMndcOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
): void {
	const selected = algorithmId === "relaxed-most-negative-cycle";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Relaxed-MNDC overlay uses the wrong algorithm");
		}
		return;
	}
	if (
		model.kind !== "fixed-flow-min-cost" &&
		model.kind !== "circulation" &&
		model.kind !== "transshipment"
	) {
		throw new Error("Relaxed-MNDC requires a minimum-cost-flow model");
	}
	if (eventId === "0" && solveStatus === "ready" && overlay === undefined)
		return;
	if (overlay === undefined) {
		throw new Error("Relaxed-MNDC scene is missing its assignment overlay");
	}
	const orderedNodes = canonicalNodeIds(nodes);
	const assignmentStage = [
		"select-family",
		"cancel-family",
		"phase-optimal",
	].includes(overlay.stage);
	if (
		overlay.nodes.length !== orderedNodes.length ||
		overlay.nodes.some((node, index) => node.node_id !== orderedNodes[index]) ||
		new Set(overlay.nodes.map((node) => node.matched_node_id)).size !==
			orderedNodes.length ||
		overlay.nodes.some(
			(node) => !orderedNodes.includes(node.matched_node_id),
		) ||
		((overlay.stage === "ready" || overlay.stage === "initialize") &&
			overlay.phase !== "0") ||
		(!["ready", "initialize", "optimal"].includes(overlay.stage) &&
			overlay.phase === "0") ||
		assignmentStage !== (overlay.assignment_value !== undefined) ||
		(["select-family", "cancel-family"].includes(overlay.stage)
			? overlay.family.length === 0
			: overlay.family.length !== 0) ||
		(solveStatus === "optimal") !== (overlay.stage === "optimal")
	) {
		throw new Error("Relaxed-MNDC boundary metadata is inconsistent");
	}
	const residualById = new Map(
		residualArcs.map((arc) => [`${arc.edge_id}\u0000${arc.direction}`, arc]),
	);
	const inspectedKeys = overlay.inspected_arcs.map(
		(arc) => `${arc.edge_id}\u0000${arc.direction}`,
	);
	const inspectingResidual = overlay.stage === "inspect-residual-arc";
	const inspectingCell = overlay.stage === "inspect-assignment-cell";
	const inspectedKey = inspectedKeys[0];
	if (
		(inspectingResidual
			? inspectedKeys.length !== 1 ||
				inspectedKey === undefined ||
				BigInt(residualById.get(inspectedKey)?.capacity ?? "0") === 0n
			: inspectedKeys.length !== 0) ||
		(inspectingCell
			? overlay.active_assignment_cell === undefined
			: overlay.active_assignment_cell !== undefined) ||
		(overlay.active_assignment_cell !== undefined &&
			(!orderedNodes.includes(overlay.active_assignment_cell.row_node_id) ||
				!orderedNodes.includes(overlay.active_assignment_cell.column_node_id)))
	) {
		throw new Error("Relaxed-MNDC work focus is inconsistent");
	}
	const denominator = BigInt(overlay.epsilon.denominator);
	const numerator = BigInt(overlay.epsilon.numerator);
	const transformed = (arc: FlowResidualArcStateV1): bigint =>
		BigInt(arc.cost) * denominator + numerator;
	const leftDual = new Map(
		overlay.nodes.map((node) => [node.node_id, BigInt(node.left_dual)]),
	);
	const rightDual = new Map(
		overlay.nodes.map((node) => [node.node_id, BigInt(node.right_dual)]),
	);
	let assignmentValue = 0n;
	for (const node of overlay.nodes) {
		const selectedArc =
			node.selected_arc === undefined
				? undefined
				: residualById.get(
						`${node.selected_arc.edge_id}\u0000${node.selected_arc.direction}`,
					);
		if (
			(node.selected_arc === undefined &&
				node.matched_node_id !== node.node_id) ||
			(node.selected_arc !== undefined &&
				(selectedArc === undefined ||
					selectedArc.from !== node.node_id ||
					selectedArc.to !== node.matched_node_id)) ||
			(!assignmentStage &&
				(node.selected_arc !== undefined ||
					node.left_dual !== "0" ||
					node.right_dual !== "0"))
		) {
			throw new Error("Relaxed-MNDC assignment row is inconsistent");
		}
		const weight = selectedArc === undefined ? 0n : transformed(selectedArc);
		assignmentValue += weight;
		if (
			assignmentStage &&
			weight !==
				(leftDual.get(node.node_id) ?? 0n) +
					(rightDual.get(node.matched_node_id) ?? 0n)
		) {
			throw new Error("Relaxed-MNDC selected assignment edge is not tight");
		}
	}
	if (assignmentStage) {
		const declared = BigInt(overlay.assignment_value ?? "0");
		const dualValue = [...leftDual.values(), ...rightDual.values()].reduce(
			(total, value) => total + value,
			0n,
		);
		if (assignmentValue !== declared || dualValue !== declared) {
			throw new Error("Relaxed-MNDC assignment primal/dual value disagrees");
		}
		if (overlay.stage !== "cancel-family") {
			for (const node of overlay.nodes) {
				if (
					0n <
					(leftDual.get(node.node_id) ?? 0n) +
						(rightDual.get(node.node_id) ?? 0n)
				) {
					throw new Error(
						"Relaxed-MNDC identity assignment dual is infeasible",
					);
				}
			}
			for (const arc of residualArcs) {
				if (
					BigInt(arc.capacity) > 0n &&
					transformed(arc) <
						(leftDual.get(arc.from) ?? 0n) + (rightDual.get(arc.to) ?? 0n)
				) {
					throw new Error("Relaxed-MNDC assignment dual is infeasible");
				}
			}
		}
		if (
			(["select-family", "cancel-family"].includes(overlay.stage) &&
				declared >= 0n) ||
			(overlay.stage === "phase-optimal" && declared < 0n)
		) {
			throw new Error("Relaxed-MNDC assignment sign disagrees with its stage");
		}
	}
	const familyNodes = new Set<string>();
	let familyValue = 0n;
	for (const cycle of overlay.family) {
		const arcs = cycle.arcs.map((arc) =>
			residualById.get(`${arc.edge_id}\u0000${arc.direction}`),
		);
		if (
			arcs.length === 0 ||
			arcs.some((arc) => arc === undefined) ||
			arcs.some(
				(arc, index) => arc?.to !== arcs[(index + 1) % arcs.length]?.from,
			) ||
			arcs.some((arc) => arc !== undefined && familyNodes.has(arc.from))
		) {
			throw new Error(
				"Relaxed-MNDC family is not node-disjoint residual cycles",
			);
		}
		for (const arc of arcs) {
			if (arc !== undefined) familyNodes.add(arc.from);
		}
		const cost = arcs.reduce(
			(total, arc) => total + (arc === undefined ? 0n : transformed(arc)),
			0n,
		);
		if (cost !== BigInt(cycle.transformed_cost) || cost >= 0n) {
			throw new Error("Relaxed-MNDC cycle transformed cost is inconsistent");
		}
		if ((overlay.stage === "cancel-family") !== (cycle.delta !== undefined)) {
			throw new Error(
				"Relaxed-MNDC family bottleneck metadata is inconsistent",
			);
		}
		familyValue += cost;
	}
	if (
		overlay.family.length > 0 &&
		familyValue !== BigInt(overlay.assignment_value ?? "0")
	) {
		throw new Error("Relaxed-MNDC family does not decompose the assignment");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const expectedCatalog = `relaxed-most-negative-cycle.${
			{
				ready: "ready",
				initialize: "initialize",
				"begin-phase": "begin-phase",
				"inspect-residual-arc": "inspect-residual-arc",
				"inspect-assignment-cell": "inspect-assignment-cell",
				"select-family": "select-family",
				"cancel-family": "cancel-family",
				"phase-optimal": "phase-optimal",
				optimal: "optimal",
			}[overlay.stage]
		}`;
		if (traceEvent.catalog_id !== expectedCatalog) {
			throw new Error("Relaxed-MNDC event and stage disagree");
		}
	}
}

function validateEnhancedCapacityScalingScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowEnhancedCapacityScalingOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
): void {
	const selected = algorithmId === "enhanced-capacity-scaling";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Enhanced-scaling overlay uses the wrong algorithm");
		}
		return;
	}
	if (model.kind !== "transshipment") {
		throw new Error("Enhanced capacity scaling requires transshipment");
	}
	if (eventId === "0" && solveStatus === "ready" && overlay === undefined)
		return;
	if (overlay === undefined) {
		throw new Error("Enhanced-scaling scene is missing its quotient overlay");
	}
	const orderedNodes = canonicalNodeIds(nodes);
	const orderedEdges = canonicalStableIds(edges);
	const nodeStates = new Map(overlay.nodes.map((node) => [node.node_id, node]));
	const componentByNode = new Map<string, string>();
	const componentIds = new Set<string>();
	for (const component of overlay.components) {
		if (
			component.members.length === 0 ||
			component.component_id !== component.members[0] ||
			componentIds.has(component.component_id) ||
			component.members.some(
				(member, index) =>
					!orderedNodes.includes(member) ||
					componentByNode.has(member) ||
					(index > 0 &&
						orderedNodes.indexOf(component.members[index - 1] ?? "") >=
							orderedNodes.indexOf(member)),
			)
		) {
			throw new Error("Enhanced-scaling component partition is invalid");
		}
		componentIds.add(component.component_id);
		for (const member of component.members) {
			componentByNode.set(member, component.component_id);
		}
	}
	const distanceStage =
		overlay.stage === "select-path" || overlay.stage === "augment";
	const inspectionStage = overlay.stage === "inspect-residual-arc";
	if (
		overlay.nodes.length !== orderedNodes.length ||
		overlay.nodes.some((node, index) => node.node_id !== orderedNodes[index]) ||
		overlay.edges.length !== orderedEdges.length ||
		overlay.edges.some((edge, index) => edge.edge_id !== orderedEdges[index]) ||
		componentByNode.size !== orderedNodes.length ||
		overlay.nodes.some(
			(node) =>
				componentByNode.get(node.node_id) !== node.component_id ||
				distanceStage !== (node.distance !== undefined),
		) ||
		(overlay.source_component === undefined) !== !distanceStage ||
		(overlay.sink_component === undefined) !== !distanceStage ||
		(overlay.source_component !== undefined &&
			!componentIds.has(overlay.source_component)) ||
		(overlay.sink_component !== undefined &&
			!componentIds.has(overlay.sink_component)) ||
		(distanceStage
			? overlay.path.length === 0
			: inspectionStage
				? overlay.path.length !== 1
				: overlay.path.length !== 0) ||
		(overlay.stage === "contract") !==
			(overlay.contraction_arc !== undefined) ||
		(overlay.stage === "augment") !== (overlay.augmentation !== undefined) ||
		(overlay.augmentation !== undefined &&
			compareRational(overlay.augmentation, overlay.delta) !== 0) ||
		(solveStatus === "optimal") !== (overlay.stage === "optimal")
	) {
		throw new Error("Enhanced-scaling boundary metadata is inconsistent");
	}
	const originalEdgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const projectedEdgeById = new Map(
		overlay.edges.map((edge) => [edge.edge_id, edge]),
	);
	const thresholdNumerator =
		3n * BigInt(nodes.length) * BigInt(overlay.delta.numerator);
	const deltaDenominator = BigInt(overlay.delta.denominator);
	for (const edge of edges) {
		const state = projectedEdgeById.get(edge.id);
		const from = nodeStates.get(edge.from);
		const to = nodeStates.get(edge.to);
		if (state === undefined || from === undefined || to === undefined) {
			throw new Error("Enhanced-scaling edge projection is incomplete");
		}
		const flow = BigInt(state.virtual_flow.numerator);
		const flowDenominator = BigInt(state.virtual_flow.denominator);
		const reduced =
			BigInt(edge.cost) + BigInt(from.potential) - BigInt(to.potential);
		const internal = from.component_id === to.component_id;
		if (
			state.reduced_cost !== reduced.toString() ||
			state.internal !== internal ||
			state.tight !== (reduced === 0n) ||
			state.strongly_feasible !==
				(!internal &&
					flow * deltaDenominator >= thresholdNumerator * flowDenominator) ||
			(overlay.stage !== "ready" &&
				(reduced < 0n || (flow > 0n && reduced !== 0n)))
		) {
			throw new Error("Enhanced-scaling edge dual state is inconsistent");
		}
	}
	let cursor = overlay.source_component;
	for (const arc of overlay.path) {
		const edge = originalEdgeById.get(arc.edge_id);
		if (edge === undefined) {
			throw new Error("Enhanced-scaling path references an unknown edge");
		}
		const from = arc.direction === "forward" ? edge.from : edge.to;
		const to = arc.direction === "forward" ? edge.to : edge.from;
		const fromComponent = componentByNode.get(from);
		const toComponent = componentByNode.get(to);
		if (
			fromComponent === undefined ||
			toComponent === undefined ||
			(!inspectionStage &&
				(fromComponent === toComponent || cursor !== fromComponent))
		) {
			throw new Error("Enhanced-scaling path is not a quotient path");
		}
		if (!inspectionStage) cursor = toComponent;
	}
	if (distanceStage && cursor !== overlay.sink_component) {
		throw new Error("Enhanced-scaling path misses the active sink");
	}
	if (
		overlay.contraction_arc !== undefined &&
		!originalEdgeById.has(overlay.contraction_arc)
	) {
		throw new Error("Enhanced-scaling contraction edge is unknown");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const expectedCatalog = `enhanced-capacity-scaling.${overlay.stage}`;
		if (traceEvent.catalog_id !== expectedCatalog) {
			throw new Error("Enhanced-scaling event and stage disagree");
		}
	}
}

function validateOrlinMcfScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowOrlinMcfOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
): void {
	const selected = algorithmId === "orlin-mcf";
	if (!selected) {
		if (overlay !== undefined)
			throw new Error("Orlin MCF overlay uses the wrong algorithm");
		return;
	}
	if (
		![
			"circulation",
			"transshipment",
			"min-cost-flow",
			"fixed-flow-min-cost",
		].includes(model.kind)
	) {
		throw new Error("Orlin MCF requires a minimum-cost-flow model");
	}
	if (eventId === "0" && solveStatus === "ready" && overlay === undefined)
		return;
	if (overlay === undefined)
		throw new Error("Orlin MCF scene is missing its transformed overlay");

	const originalNodes = canonicalNodeIds(nodes);
	const variableEdges = [...edges]
		.filter((edge) => BigInt(edge.capacity) > BigInt(edge.lower))
		.sort((left, right) => left.id.localeCompare(right.id));
	const transformedNodes = [
		...originalNodes,
		...variableEdges.map((edge) => `capacity:${edge.id}`),
	];
	const rank = new Map(transformedNodes.map((node, index) => [node, index]));
	const nodeStateById = new Map(
		overlay.nodes.map((node) => [node.node_id, node]),
	);
	if (
		overlay.nodes.length !== transformedNodes.length ||
		overlay.nodes.some(
			(node, index) => node.node_id !== transformedNodes[index],
		) ||
		overlay.nodes
			.slice(0, originalNodes.length)
			.some(
				(node) =>
					node.kind !== "original" || node.capacity_edge_id !== undefined,
			) ||
		overlay.nodes
			.slice(originalNodes.length)
			.some(
				(node, index) =>
					node.kind !== "capacity" ||
					node.capacity_edge_id !== variableEdges[index]?.id,
			)
	) {
		throw new Error("Orlin MCF transformed-node projection is inconsistent");
	}
	const componentByNode = new Map<string, string>();
	const componentIds = new Set<string>();
	let previousComponentRank = -1;
	for (const component of overlay.components) {
		const firstRank = rank.get(component.members[0] ?? "") ?? -1;
		let previousMemberRank = -1;
		if (
			component.members.length === 0 ||
			component.component_id !== component.members[0] ||
			firstRank <= previousComponentRank ||
			componentIds.has(component.component_id) ||
			component.members.some((member) => {
				const memberRank = rank.get(member) ?? -1;
				const invalid =
					memberRank <= previousMemberRank || componentByNode.has(member);
				previousMemberRank = memberRank;
				return invalid;
			})
		) {
			throw new Error("Orlin MCF component partition is invalid");
		}
		previousComponentRank = firstRank;
		componentIds.add(component.component_id);
		for (const member of component.members)
			componentByNode.set(member, component.component_id);
	}
	const pathStage =
		overlay.stage === "select-compressed-path" || overlay.stage === "augment";
	const branchInspectionStage = [
		"inspect-contractible-arc",
		"inspect-reachability-arc",
		"inspect-compressed-residual-arc",
		"inspect-compressed-arc",
	].includes(overlay.stage);
	const distanceInspectionStage = overlay.stage === "inspect-compressed-arc";
	const distanceShapeInvalid = pathStage
		? overlay.nodes.some((node) => node.distance === undefined)
		: distanceInspectionStage
			? overlay.nodes.every((node) => node.distance === undefined)
			: overlay.nodes.some((node) => node.distance !== undefined);
	if (
		componentByNode.size !== transformedNodes.length ||
		overlay.nodes.some(
			(node) => componentByNode.get(node.node_id) !== node.component_id,
		) ||
		distanceShapeInvalid ||
		(overlay.source_component === undefined) !== !pathStage ||
		(overlay.sink_component === undefined) !== !pathStage ||
		(overlay.source_component !== undefined &&
			!componentIds.has(overlay.source_component)) ||
		(overlay.sink_component !== undefined &&
			!componentIds.has(overlay.sink_component)) ||
		(pathStage ? overlay.path.length === 0 : overlay.path.length !== 0) ||
		(overlay.stage === "contract") !==
			(overlay.contraction_arc !== undefined) ||
		(overlay.stage === "augment") !== (overlay.augmentation !== undefined) ||
		(overlay.augmentation !== undefined &&
			compareRational(overlay.augmentation, overlay.delta) !== 0) ||
		(solveStatus === "optimal") !== (overlay.stage === "optimal")
	) {
		throw new Error("Orlin MCF boundary metadata is inconsistent");
	}

	const edgeById = new Map(variableEdges.map((edge) => [edge.id, edge]));
	const expectedArcs = variableEdges.flatMap((edge) => [
		`${edge.id}:flow`,
		`${edge.id}:slack`,
	]);
	if (
		overlay.arcs.length !== expectedArcs.length ||
		overlay.arcs.some(
			(arc, index) => `${arc.edge_id}:${arc.branch}` !== expectedArcs[index],
		)
	) {
		throw new Error("Orlin MCF branch projection is incomplete");
	}
	const threshold =
		3n * BigInt(transformedNodes.length) * BigInt(overlay.delta.numerator);
	const deltaDenominator = BigInt(overlay.delta.denominator);
	for (const state of overlay.arcs) {
		const edge = edgeById.get(state.edge_id);
		if (edge === undefined)
			throw new Error("Orlin MCF branch references an unknown edge");
		const original = state.branch === "flow" ? edge.from : edge.to;
		const capacity = `capacity:${edge.id}`;
		const originalNode = nodeStateById.get(original);
		const capacityNode = nodeStateById.get(capacity);
		if (originalNode === undefined || capacityNode === undefined)
			throw new Error("Orlin MCF branch endpoint is absent");
		const reduced =
			(state.branch === "flow" ? BigInt(edge.cost) : 0n) +
			BigInt(originalNode.potential) -
			BigInt(capacityNode.potential);
		const internal = originalNode.component_id === capacityNode.component_id;
		const flow = BigInt(state.flow.numerator);
		const flowDenominator = BigInt(state.flow.denominator);
		const dualRequired = !["ready", "transform-capacities"].includes(
			overlay.stage,
		);
		if (
			state.reduced_cost !== reduced.toString() ||
			state.internal !== internal ||
			state.tight !== (reduced === 0n) ||
			state.strongly_feasible !==
				(!internal && flow * deltaDenominator >= threshold * flowDenominator) ||
			(dualRequired && (reduced < 0n || (flow > 0n && reduced !== 0n)))
		) {
			throw new Error("Orlin MCF branch dual state is inconsistent");
		}
	}

	const endpoints = (arc: FlowOrlinMcfArcRefV1): [string, string] => {
		const edge = edgeById.get(arc.edge_id);
		if (edge === undefined)
			throw new Error("Orlin MCF path references an unknown edge");
		const original = arc.branch === "flow" ? edge.from : edge.to;
		const capacity = `capacity:${edge.id}`;
		return arc.direction === "forward"
			? [original, capacity]
			: [capacity, original];
	};
	const inspectionValid =
		(overlay.inspected_segment.length === 1 ||
			overlay.inspected_segment.length === 2) &&
		overlay.inspection_serial !== undefined &&
		/^[1-9]\d*$/.test(overlay.inspection_serial) &&
		overlay.inspected_segment.every((arc) => {
			try {
				endpoints(arc);
				return true;
			} catch {
				return false;
			}
		});
	if (
		branchInspectionStage !== inspectionValid ||
		(!branchInspectionStage &&
			(overlay.inspected_segment.length !== 0 ||
				overlay.inspection_serial !== undefined))
	) {
		throw new Error("Orlin MCF inspection metadata is inconsistent");
	}
	let cursor = overlay.source_component;
	for (const arc of overlay.path) {
		const [from, to] = endpoints(arc);
		const fromComponent = componentByNode.get(from);
		const toComponent = componentByNode.get(to);
		if (
			fromComponent === undefined ||
			toComponent === undefined ||
			fromComponent === toComponent ||
			cursor !== fromComponent
		) {
			throw new Error("Orlin MCF path is not a transformed quotient path");
		}
		cursor = toComponent;
	}
	if (pathStage && cursor !== overlay.sink_component)
		throw new Error("Orlin MCF path misses the active sink");
	if (overlay.contraction_arc !== undefined) {
		const [from, to] = endpoints(overlay.contraction_arc);
		if (
			overlay.contraction_arc.direction !== "forward" ||
			componentByNode.get(from) !== componentByNode.get(to)
		) {
			throw new Error("Orlin MCF contraction branch is inconsistent");
		}
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const suffix: Record<FlowOrlinMcfOverlayV1["stage"], string> = {
			ready: "ready",
			"transform-capacities": "transform-capacities",
			"initialize-dual": "initialize-dual",
			"complete-regeneration": "complete-regeneration",
			"begin-phase": "begin-phase",
			"inspect-contractible-arc": "inspect-contractible-arc",
			"inspect-reachability-arc": "inspect-reachability-arc",
			"inspect-compressed-residual-arc": "inspect-compressed-residual-arc",
			"inspect-compressed-arc": "inspect-compressed-arc",
			contract: "contract",
			"select-compressed-path": "select-compressed-path",
			augment: "augment",
			"complete-phase": "complete-phase",
			"halve-scale": "halve-scale",
			"expand-dual": "expand-dual",
			"recover-primal": "recover-primal",
			optimal: "optimal",
		};
		if (traceEvent.catalog_id !== `orlin-mcf.${suffix[overlay.stage]}`)
			throw new Error("Orlin MCF event and stage disagree");
	}
}

function validateOrlinMaxFlowScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	residualArcs: FlowResidualArcStateV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowOrlinMaxFlowOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "orlin-max-flow";
	if (!selected) {
		if (overlay !== undefined)
			throw new Error("Orlin max-flow overlay uses the wrong algorithm");
		return;
	}
	if (model.kind !== "max-flow") {
		throw new Error("Orlin max flow requires a max-flow model");
	}
	if (eventId === "0" && solveStatus === "ready" && overlay === undefined)
		return;
	if (overlay === undefined) {
		throw new Error("Orlin max-flow scene is missing its compact overlay");
	}
	const orderedNodes = canonicalNodeIds(nodes);
	const orderedEdges = canonicalStableIds(edges);
	const componentFirst = new Map<string, string>();
	const componentState = new Map<string, { critical: boolean; phi: string }>();
	for (const node of overlay.nodes) {
		if (!componentFirst.has(node.component_id)) {
			componentFirst.set(node.component_id, node.node_id);
		}
		const state = { critical: node.critical, phi: node.anti_potential };
		const previous = componentState.get(node.component_id);
		if (
			(previous !== undefined &&
				(previous.critical !== state.critical || previous.phi !== state.phi)) ||
			componentFirst.get(node.component_id) !== node.component_id
		) {
			throw new Error("Orlin max-flow component projection is inconsistent");
		}
		componentState.set(node.component_id, state);
	}
	if (
		overlay.nodes.length !== orderedNodes.length ||
		overlay.nodes.some((node, index) => node.node_id !== orderedNodes[index])
	) {
		throw new Error("Orlin max-flow node order is inconsistent");
	}
	const residualByKey = new Map(
		residualArcs.map((arc) => [`${arc.edge_id}\u0000${arc.direction}`, arc]),
	);
	const expectedResidualOrder = orderedEdges.flatMap((edge) => [
		`${edge}\u0000forward`,
		`${edge}\u0000reverse`,
	]);
	const delta = BigInt(overlay.delta);
	if (
		overlay.residual_arcs.length !== expectedResidualOrder.length ||
		overlay.residual_arcs.some((arc, index) => {
			const key = `${arc.edge_id}\u0000${arc.direction}`;
			const generic = residualByKey.get(key);
			return (
				key !== expectedResidualOrder[index] ||
				generic === undefined ||
				arc.capacity !== generic.capacity ||
				(arc.abundant && BigInt(arc.capacity) < 2n * delta)
			);
		})
	) {
		throw new Error("Orlin max-flow residual classification is inconsistent");
	}
	const validResidualRef = (reference: FlowResidualArcRefV1): boolean =>
		residualByKey.has(`${reference.edge_id}\u0000${reference.direction}`);
	for (const [index, arc] of overlay.compact_arcs.entries()) {
		if (
			arc.ordinal !== index.toString() ||
			!componentState.has(arc.from_component) ||
			!componentState.has(arc.to_component) ||
			arc.from_component === arc.to_component ||
			BigInt(arc.flow) > BigInt(arc.capacity) ||
			arc.witness.length === 0 ||
			arc.witness.some((reference) => !validResidualRef(reference))
		) {
			throw new Error("Orlin max-flow compact arc is inconsistent");
		}
	}
	const activeCompactStage = [
		"augment-subproblem",
		"inspect-subproblem-arc",
		"inspect-decomposition-arc",
		"inspect-lift-residual-arc",
		"lift-path",
	].includes(overlay.stage);
	const activeOriginalStage =
		overlay.stage === "inspect-classification-arc" ||
		overlay.stage === "inspect-compact-construction-arc" ||
		overlay.stage === "transfer-capacity" ||
		overlay.stage === "inspect-lift-residual-arc" ||
		overlay.stage === "lift-path" ||
		overlay.stage === "expand-contraction" ||
		overlay.stage === "inspect-expansion-residual-arc" ||
		overlay.stage === "inspect-cut-residual-arc";
	const caseStage = [
		"select-case",
		"inspect-compact-construction-arc",
		"transfer-capacity",
		"build-subproblem",
		"augment-subproblem",
		"inspect-subproblem-arc",
		"complete-subproblem",
		"inspect-decomposition-arc",
		"inspect-lift-residual-arc",
		"lift-path",
		"expand-contraction",
		"inspect-expansion-residual-arc",
		"inspect-cut-residual-arc",
		"update-cut",
	].includes(overlay.stage);
	const inspectedCompact = overlay.compact_arcs
		.filter((arc) => arc.inspection_serial !== undefined)
		.map((arc) => arc.ordinal)
		.sort();
	const expectedInspectedCompact = overlay.active_compact_path
		.map((reference) => reference.ordinal)
		.sort();
	const inspectedResidual = overlay.residual_arcs
		.filter((arc) => arc.inspection_serial !== undefined)
		.map((arc) => `${arc.edge_id}\u0000${arc.direction}`)
		.sort();
	const expectedInspectedResidual = overlay.active_original_path
		.map((reference) => `${reference.edge_id}\u0000${reference.direction}`)
		.sort();
	const compactInspection =
		overlay.stage === "inspect-subproblem-arc" ||
		overlay.stage === "inspect-decomposition-arc";
	const residualInspection =
		overlay.stage === "inspect-classification-arc" ||
		overlay.stage === "inspect-compact-construction-arc" ||
		overlay.stage === "inspect-lift-residual-arc" ||
		overlay.stage === "inspect-expansion-residual-arc" ||
		overlay.stage === "inspect-cut-residual-arc";
	if (
		caseStage !== (overlay.phase_case !== undefined) ||
		(!activeCompactStage && overlay.active_compact_path.length !== 0) ||
		overlay.active_compact_path.some(
			(reference) =>
				BigInt(reference.ordinal) >= BigInt(overlay.compact_arcs.length),
		) ||
		(!activeOriginalStage && overlay.active_original_path.length !== 0) ||
		overlay.active_original_path.some(
			(reference) => !validResidualRef(reference),
		) ||
		overlay.active_original_path.some((reference) => {
			const residual = residualByKey.get(
				`${reference.edge_id}\u0000${reference.direction}`,
			);
			return residual?.active !== true;
		}) ||
		(compactInspection
			? inspectedCompact.join("|") !== expectedInspectedCompact.join("|")
			: inspectedCompact.length !== 0) ||
		(residualInspection
			? inspectedResidual.join("|") !== expectedInspectedResidual.join("|")
			: inspectedResidual.length !== 0) ||
		(solveStatus === "optimal") !== (overlay.stage === "optimal") ||
		(overlay.stage === "optimal" && delta !== 0n)
	) {
		throw new Error("Orlin max-flow boundary metadata is inconsistent");
	}
	if (
		traceEventRequiresStageIdentity(traceEvent) &&
		traceEvent.catalog_id !== `orlin-max-flow.${overlay.stage}`
	) {
		throw new Error("Orlin max-flow event and stage disagree");
	}
	if (overlay.stage === "optimal") {
		if (outcome?.kind !== "max-flow") {
			throw new Error("Orlin max-flow optimal boundary lacks its certificate");
		}
		const sourceSide = overlay.nodes
			.filter((node) => node.source_side)
			.map((node) => node.node_id);
		if (
			sourceSide.length !== outcome.source_side.length ||
			sourceSide.some((node, index) => node !== outcome.source_side[index])
		) {
			throw new Error("Orlin max-flow source cut disagrees with its outcome");
		}
	}
}

function electricalClose(left: number, right: number): boolean {
	return (
		Math.abs(left - right) <=
		1e-8 * (1 + Math.max(Math.abs(left), Math.abs(right)))
	);
}

function electricalRationalNumber(value: FlowRationalV1): number {
	const result = Number(value.numerator) / Number(value.denominator);
	if (!Number.isFinite(result)) {
		throw new Error("Electrical-flow rational is not finitely projectable");
	}
	return result;
}

function validateElectricalFlowScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	residualArcs: FlowResidualArcStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowElectricalFlowOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "electrical-flow";
	if (!selected) {
		if (overlay !== undefined || outcome?.kind === "electrical-flow") {
			throw new Error("Electrical-flow state uses the wrong algorithm");
		}
		return;
	}
	if (model.kind !== "max-flow" || Object.keys(config).length !== 0) {
		throw new Error("Electrical flow requires an unconfigured max-flow model");
	}
	if (
		nodes.length < 2 ||
		nodes.length > 24 ||
		edges.length === 0 ||
		edges.length > 96 ||
		model.source === model.sink ||
		nodes.every((node) => node.id !== model.source) ||
		nodes.every((node) => node.id !== model.sink) ||
		nodes.some((node) => node.supply !== "0") ||
		edges.some(
			(edge) =>
				edge.lower !== "0" ||
				edge.cost !== "0" ||
				BigInt(edge.capacity) === 0n ||
				BigInt(edge.capacity) > 1_000_000n ||
				edge.from === edge.to,
		)
	) {
		throw new Error("Electrical-flow graph is outside its admitted domain");
	}
	const adjacency = new Map(nodes.map((node) => [node.id, [] as string[]]));
	for (const edge of edges) {
		adjacency.get(edge.from)?.push(edge.to);
		adjacency.get(edge.to)?.push(edge.from);
	}
	const reached = new Set<string>();
	const queue = [model.source];
	while (queue.length > 0) {
		const node = queue.shift();
		if (node === undefined || reached.has(node)) continue;
		reached.add(node);
		queue.push(...(adjacency.get(node) ?? []));
	}
	if (reached.size !== nodes.length) {
		throw new Error("Electrical-flow resistor graph is disconnected");
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined)
		) {
			return;
		}
		throw new Error("Electrical-flow scene is missing its numerical overlay");
	}
	if (
		edgeStates.length !== edges.length ||
		edgeStates.some((state) => state.flow !== "0") ||
		residualArcs.length !== 0
	) {
		throw new Error(
			"Electrical flow must not masquerade as a feasible max flow",
		);
	}

	const nodeOrder = canonicalNodeIds(nodes);
	const edgeOrder = canonicalStableIds(edges);
	if (
		overlay.nodes.length !== nodeOrder.length ||
		overlay.nodes.some((state, index) => state.node_id !== nodeOrder[index]) ||
		overlay.edges.length !== edgeOrder.length ||
		overlay.edges.some((state, index) => state.edge_id !== edgeOrder[index]) ||
		overlay.nodes.filter((state) => state.grounded).length !== 1 ||
		overlay.nodes.find((state) => state.grounded)?.node_id !== model.sink
	) {
		throw new Error(
			"Electrical-flow stable identities or ground are inconsistent",
		);
	}
	const nodeState = new Map(
		overlay.nodes.map((state) => [state.node_id, state]),
	);
	const potentials = new Map(
		overlay.nodes.map((state) => [state.node_id, Number(state.potential)]),
	);
	const ground = nodeState.get(model.sink);
	if (
		ground === undefined ||
		Number(ground.potential) !== 0 ||
		Number(ground.residual) !== 0 ||
		Number(ground.search_direction) !== 0
	) {
		throw new Error("Electrical-flow grounded state is not exactly zero");
	}

	const recovered = [
		"recover-currents",
		"check-exact-reference",
		"complete",
	].includes(overlay.stage);
	const exactChecked = ["check-exact-reference", "complete"].includes(
		overlay.stage,
	);
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const divergence = new Map(nodes.map((node) => [node.id, 0]));
	let recomputedEnergy = 0;
	for (const state of overlay.edges) {
		const edge = edgeById.get(state.edge_id);
		if (edge === undefined) {
			throw new Error("Electrical-flow edge projection is incomplete");
		}
		const capacity = BigInt(edge.capacity);
		const conductance = capacity * capacity;
		if (
			state.conductance !== conductance.toString() ||
			state.resistance.numerator !== "1" ||
			state.resistance.denominator !== conductance.toString()
		) {
			throw new Error("Electrical-flow resistance mapping is inconsistent");
		}
		const voltage = Number(state.voltage_drop);
		const current = Number(state.current);
		const congestion = Number(state.congestion);
		const energy = Number(state.energy);
		if (!recovered) {
			if (voltage !== 0 || current !== 0 || congestion !== 0 || energy !== 0) {
				throw new Error(
					"Electrical edge values appear before current recovery",
				);
			}
			continue;
		}
		const expectedVoltage =
			(potentials.get(edge.from) ?? Number.NaN) -
			(potentials.get(edge.to) ?? Number.NaN);
		const expectedCurrent = Number(conductance) * expectedVoltage;
		const expectedCongestion = Math.abs(current) / Number(capacity);
		const expectedEnergy = (current * current) / Number(conductance);
		if (
			!electricalClose(voltage, expectedVoltage) ||
			!electricalClose(current, expectedCurrent) ||
			!electricalClose(congestion, expectedCongestion) ||
			!electricalClose(energy, expectedEnergy)
		) {
			throw new Error("Electrical-flow Ohm or energy law is inconsistent");
		}
		divergence.set(edge.from, (divergence.get(edge.from) ?? 0) + current);
		divergence.set(edge.to, (divergence.get(edge.to) ?? 0) - current);
		recomputedEnergy += energy;
	}
	const residualL2 = Number(overlay.residual_l2);
	const effectiveResistance = Number(overlay.effective_resistance);
	const totalEnergy = Number(overlay.total_energy);
	if (recovered) {
		for (const node of nodes) {
			const expected =
				node.id === model.source ? 1 : node.id === model.sink ? -1 : 0;
			if (!electricalClose(divergence.get(node.id) ?? Number.NaN, expected)) {
				throw new Error("Electrical-flow KCL certificate is inconsistent");
			}
		}
		const terminalDrop =
			(potentials.get(model.source) ?? Number.NaN) -
			(potentials.get(model.sink) ?? Number.NaN);
		if (
			!electricalClose(recomputedEnergy, totalEnergy) ||
			!electricalClose(terminalDrop, effectiveResistance) ||
			!electricalClose(totalEnergy, effectiveResistance)
		) {
			throw new Error(
				"Electrical-flow energy/effective-resistance identity failed",
			);
		}
	} else if (totalEnergy !== 0 || effectiveResistance !== 0) {
		throw new Error(
			"Electrical-flow terminal quantities appear before recovery",
		);
	}

	if (exactChecked) {
		if (
			overlay.exact_effective_resistance === undefined ||
			overlay.maximum_absolute_error === undefined ||
			!electricalClose(
				electricalRationalNumber(overlay.exact_effective_resistance),
				effectiveResistance,
			) ||
			Number(overlay.maximum_absolute_error) >
				1e-8 *
					(1 +
						Math.abs(
							electricalRationalNumber(overlay.exact_effective_resistance),
						))
		) {
			throw new Error("Electrical-flow exact reference certificate failed");
		}
	} else if (
		overlay.exact_effective_resistance !== undefined ||
		overlay.maximum_absolute_error !== undefined
	) {
		throw new Error("Electrical exact witness appears before its check stage");
	}

	const preIteration = [
		"ready",
		"assemble-laplacian",
		"initialize-conjugate-gradient",
	].includes(overlay.stage);
	if (
		(overlay.stage === "ready" && overlay.iteration !== "0") ||
		(preIteration && overlay.converged) ||
		(recovered && !overlay.converged) ||
		(overlay.converged && residualL2 > Number(overlay.relative_tolerance))
	) {
		throw new Error("Electrical-flow convergence metadata is inconsistent");
	}
	const expectedStatus: Record<
		FlowElectricalFlowOverlayV1["stage"],
		FlowCurrentSceneV9["solve_status"]
	> = {
		ready: "ready",
		"assemble-laplacian": "running",
		"initialize-conjugate-gradient": "running",
		"conjugate-gradient-iteration": "running",
		"recover-currents": "running",
		"check-exact-reference": "running",
		complete: "primitive-complete",
	};
	if (
		solveStatus !== "resource-limit" &&
		(solveStatus !== expectedStatus[overlay.stage] ||
			(overlay.stage === "ready" && eventId !== "0"))
	) {
		throw new Error("Electrical-flow stage and solve status disagree");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const catalog: Record<FlowElectricalFlowOverlayV1["stage"], string[]> = {
			ready: ["electrical-flow.ready"],
			"assemble-laplacian": ["electrical-flow.assemble-laplacian"],
			"initialize-conjugate-gradient": ["electrical-flow.initialize-cg"],
			"conjugate-gradient-iteration": [
				"electrical-flow.matrix-scalar-product",
				"electrical-flow.cg-iteration",
			],
			"recover-currents": ["electrical-flow.recover-currents"],
			"check-exact-reference": ["electrical-flow.check-exact-reference"],
			complete: ["electrical-flow.complete-primitive"],
		};
		if (!catalog[overlay.stage].includes(traceEvent.catalog_id)) {
			throw new Error("Electrical-flow trace event and stage disagree");
		}
	}
	if (overlay.stage === "complete") {
		if (
			outcome?.kind !== "electrical-flow" ||
			outcome.effective_resistance !== overlay.effective_resistance ||
			outcome.total_energy !== overlay.total_energy ||
			outcome.residual_l2 !== overlay.residual_l2 ||
			outcome.maximum_absolute_error !== overlay.maximum_absolute_error ||
			outcome.iterations !== overlay.iteration ||
			overlay.exact_effective_resistance === undefined ||
			!rationalEqual(
				outcome.exact_effective_resistance,
				overlay.exact_effective_resistance,
			)
		) {
			throw new Error("Electrical-flow primitive outcome is inconsistent");
		}
	} else if (outcome?.kind === "electrical-flow") {
		throw new Error("Electrical-flow outcome appears before completion");
	}
}

function validateAugmentingElectricalScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowAugmentingElectricalOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "augmenting-electrical-flow";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Augmenting-electrical state uses the wrong algorithm");
		}
		return;
	}
	if (model.kind !== "max-flow" || Object.keys(config).length !== 0) {
		throw new Error(
			"Augmenting electrical flow requires an unconfigured max-flow model",
		);
	}
	if (
		nodes.length < 2 ||
		nodes.length > 10 ||
		edges.length > 12 ||
		model.source === model.sink ||
		nodes.every((node) => node.id !== model.source) ||
		nodes.every((node) => node.id !== model.sink) ||
		nodes.some((node) => node.supply !== "0") ||
		edges.some(
			(edge) =>
				edge.lower !== "0" ||
				edge.cost !== "0" ||
				BigInt(edge.capacity) === 0n ||
				BigInt(edge.capacity) > 64n ||
				edge.from === edge.to,
		)
	) {
		throw new Error(
			"Augmenting-electrical graph is outside its bounded admitted domain",
		);
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined)
		) {
			return;
		}
		throw new Error(
			"Augmenting-electrical scene is missing its numerical overlay",
		);
	}
	const nodeOrder = canonicalNodeIds(nodes);
	const edgeOrder = canonicalStableIds(edges);
	if (
		overlay.nodes.length !== nodeOrder.length ||
		overlay.nodes.some((state, index) => state.node_id !== nodeOrder[index]) ||
		overlay.edges.length !== edgeOrder.length ||
		overlay.edges.some((state, index) => state.edge_id !== edgeOrder[index]) ||
		edgeStates.length !== edgeOrder.length
	) {
		throw new Error("Augmenting-electrical stable identities are inconsistent");
	}
	const targetSide = new Set(
		overlay.nodes
			.filter((node) => node.target_source_side)
			.map((node) => node.node_id),
	);
	const targetCutInstalled = ![
		"ready",
		"build-directed-reduction",
		"add-preconditioning",
	].includes(overlay.stage);
	if (
		targetCutInstalled &&
		(!targetSide.has(model.source) || targetSide.has(model.sink))
	) {
		throw new Error(
			"Augmenting-electrical target cut has invalid terminal membership",
		);
	}
	if (!targetCutInstalled && targetSide.size !== 0) {
		throw new Error(
			"Augmenting-electrical target cut appears before its install boundary",
		);
	}
	let cutCapacity = 0n;
	let capacitySum = 0n;
	let maximumCapacity = 0n;
	for (const edge of edges) {
		const capacity = BigInt(edge.capacity);
		capacitySum += capacity;
		if (capacity > maximumCapacity) maximumCapacity = capacity;
		if (targetSide.has(edge.from) && !targetSide.has(edge.to)) {
			cutCapacity += capacity;
		}
	}
	const originalTarget = BigInt(overlay.original_target);
	const transformedTarget = BigInt(overlay.transformed_target);
	const workingTarget = BigInt(overlay.working_target);
	const expectedTransformed = capacitySum + 2n * originalTarget;
	const expectedWorking =
		expectedTransformed + BigInt(edges.length) * 6n * maximumCapacity;
	if (targetCutInstalled && cutCapacity !== originalTarget) {
		throw new Error("Augmenting-electrical target cut value is inconsistent");
	}
	const ready = overlay.stage === "ready";
	const reductionOnly = overlay.stage === "build-directed-reduction";
	if (
		(ready && (transformedTarget !== 0n || workingTarget !== 0n)) ||
		(reductionOnly &&
			(transformedTarget !== expectedTransformed || workingTarget !== 0n)) ||
		(!ready &&
			!reductionOnly &&
			(transformedTarget !== expectedTransformed ||
				workingTarget !== expectedWorking))
	) {
		throw new Error(
			"Augmenting-electrical transformed targets are inconsistent",
		);
	}
	const workingNodes = BigInt(overlay.working_nodes);
	const workingEdges = BigInt(overlay.working_edges);
	if (
		workingNodes > 192n ||
		workingEdges > 384n ||
		(overlay.active_working_edge !== undefined &&
			BigInt(overlay.active_working_edge) >= workingEdges)
	) {
		throw new Error(
			"Augmenting-electrical working graph exceeds its public bounds",
		);
	}
	const current = Number(overlay.current_value);
	const remaining = Number(overlay.remaining);
	const alpha = Number(overlay.alpha);
	if (
		workingTarget > 0n &&
		(!electricalClose(current + remaining, Number(workingTarget)) ||
			!electricalClose(current, alpha * Number(workingTarget)))
	) {
		throw new Error("Augmenting-electrical progress scalars are inconsistent");
	}
	const terminalFlows = [
		"round-directed-flow",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const roundedCentralFlows = [
		"round-central-flow",
		"cleanup-augmenting-path",
		"extract-directed-flow",
		"cancel-extraction-cycle",
		"round-directed-flow",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const extractionVisible = [
		"extract-directed-flow",
		"cancel-extraction-cycle",
		"round-directed-flow",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const graphEdgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const graphNodeIds = new Set(nodes.map((node) => node.id));
	const genericFlowById = new Map(
		edgeStates.map((state) => [state.edge_id, state.flow]),
	);
	for (const state of overlay.edges) {
		const edge = graphEdgeById.get(state.edge_id);
		if (edge === undefined) {
			throw new Error("Augmenting-electrical edge projection is incomplete");
		}
		if (
			(ready && state.boost_segments !== "0") ||
			(!ready && state.boost_segments === "0") ||
			roundedCentralFlows !== (state.rounded_central_flow !== undefined) ||
			extractionVisible !==
				(state.extraction_central_scaled !== undefined &&
					state.extraction_toward_source !== undefined &&
					state.extraction_out_of_sink !== undefined) ||
			terminalFlows !== (state.final_flow !== undefined) ||
			(state.final_flow !== undefined &&
				(BigInt(state.final_flow) > BigInt(edge.capacity) ||
					genericFlowById.get(state.edge_id) !== state.final_flow)) ||
			(!terminalFlows && genericFlowById.get(state.edge_id) !== "0")
		) {
			throw new Error(
				"Augmenting-electrical edge flow or boost metadata is inconsistent",
			);
		}
	}
	if (
		overlay.active_working_path.length > 0 &&
		(overlay.active_working_path.some(
			(arc) =>
				!graphNodeIds.has(arc.from_node) || !graphNodeIds.has(arc.to_node),
		) ||
			overlay.active_working_path.some(
				(arc, index, path) =>
					index > 0 && path[index - 1]?.to_node !== arc.from_node,
			) ||
			overlay.active_working_path[0]?.from_node !== model.source ||
			overlay.active_working_path.at(-1)?.to_node !== model.sink)
	) {
		throw new Error(
			"Augmenting-electrical cleanup path is not a connected source-to-sink path",
		);
	}
	const expectedStatus: Record<
		FlowAugmentingElectricalOverlayV1["stage"],
		FlowCurrentSceneV9["solve_status"]
	> = {
		ready: "ready",
		"build-directed-reduction": "running",
		"add-preconditioning": "running",
		"install-target-cut": "running",
		"solve-electrical-direction": "running",
		"boost-high-energy-arc": "running",
		"augment-primal-dual": "running",
		"fix-coupling": "running",
		"collapse-boost-paths": "running",
		"round-central-flow": "running",
		"cleanup-augmenting-path": "running",
		"extract-directed-flow": "running",
		"cancel-extraction-cycle": "running",
		"round-directed-flow": "running",
		"check-certificate": "running",
		optimal: "optimal",
	};
	if (
		solveStatus !== "resource-limit" &&
		(solveStatus !== expectedStatus[overlay.stage] ||
			(overlay.stage === "ready" && eventId !== "0"))
	) {
		throw new Error("Augmenting-electrical stage and solve status disagree");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const catalogs: Record<
			FlowAugmentingElectricalOverlayV1["stage"],
			readonly string[]
		> = {
			ready: ["augmenting-electrical-flow.ready"],
			"build-directed-reduction": [
				"augmenting-electrical-flow.build-directed-reduction",
			],
			"add-preconditioning": ["augmenting-electrical-flow.add-preconditioning"],
			"install-target-cut": ["augmenting-electrical-flow.install-target-cut"],
			"solve-electrical-direction": [
				"augmenting-electrical-flow.solve-direction",
				"augmenting-electrical-flow.elimination-pivot",
				"augmenting-electrical-flow.resolve-after-boost",
			],
			"boost-high-energy-arc": ["augmenting-electrical-flow.boost-high-energy"],
			"augment-primal-dual": ["augmenting-electrical-flow.augment-primal-dual"],
			"fix-coupling": ["augmenting-electrical-flow.fix-coupling"],
			"collapse-boost-paths": [
				"augmenting-electrical-flow.collapse-boost-paths",
			],
			"round-central-flow": ["augmenting-electrical-flow.round-central-flow"],
			"cleanup-augmenting-path": [
				"augmenting-electrical-flow.cleanup-augmenting-path",
			],
			"extract-directed-flow": [
				"augmenting-electrical-flow.extract-directed-reduction",
			],
			"cancel-extraction-cycle": [
				"augmenting-electrical-flow.cancel-extraction-cycle",
			],
			"round-directed-flow": ["augmenting-electrical-flow.round-directed-flow"],
			"check-certificate": ["augmenting-electrical-flow.check-certificate"],
			optimal: ["augmenting-electrical-flow.optimal"],
		};
		if (!catalogs[overlay.stage].includes(traceEvent.catalog_id)) {
			throw new Error("Augmenting-electrical trace event and stage disagree");
		}
	}
	if (overlay.stage === "optimal") {
		if (
			outcome?.kind !== "max-flow" ||
			outcome.value !== overlay.original_target
		) {
			throw new Error(
				"Augmenting-electrical optimal boundary lacks its certificate",
			);
		}
	} else if (outcome !== undefined) {
		throw new Error("Augmenting-electrical outcome appears before completion");
	}
}

function validateInteriorPointMaxFlowScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowInteriorPointMaxFlowOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "interior-point-max-flow";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Interior-point overlay uses the wrong algorithm");
		}
		return;
	}
	if (model.kind !== "max-flow" || Object.keys(config).length !== 0) {
		throw new Error(
			"Interior-point flow requires an unconfigured max-flow model",
		);
	}
	if (
		nodes.length < 2 ||
		nodes.length > 8 ||
		edges.length > 10 ||
		model.source === model.sink ||
		nodes.every((node) => node.id !== model.source) ||
		nodes.every((node) => node.id !== model.sink) ||
		nodes.some((node) => node.supply !== "0") ||
		edges.some(
			(edge) =>
				edge.lower !== "0" ||
				edge.capacity !== "1" ||
				edge.cost !== "0" ||
				edge.from === edge.to,
		)
	) {
		throw new Error("Interior-point graph is outside its bounded unit domain");
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined)
		) {
			return;
		}
		throw new Error("Interior-point scene is missing its central-path overlay");
	}
	const nodeOrder = canonicalNodeIds(nodes);
	const edgeOrder = canonicalStableIds(edges);
	if (
		overlay.nodes.length !== nodeOrder.length ||
		overlay.nodes.some((state, index) => state.node_id !== nodeOrder[index]) ||
		overlay.edges.length !== edgeOrder.length ||
		overlay.edges.some((state, index) => state.edge_id !== edgeOrder[index]) ||
		edgeStates.length !== edgeOrder.length
	) {
		throw new Error("Interior-point stable identities are inconsistent");
	}
	const ready = overlay.stage === "ready";
	const targetInstalled = !ready;
	const bBuilt = !["ready", "enumerate-target-cut"].includes(overlay.stage);
	const workingBuilt = ![
		"ready",
		"enumerate-target-cut",
		"build-b-matching-reduction",
	].includes(overlay.stage);
	const initialized = ![
		"ready",
		"enumerate-target-cut",
		"build-b-matching-reduction",
		"build-min-cost-reduction",
	].includes(overlay.stage);
	const terminalFlows = [
		"round-integral-flow",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const target = BigInt(overlay.target_value);
	const targetSide = new Set(
		overlay.nodes
			.filter((node) => node.target_source_side)
			.map((node) => node.node_id),
	);
	if (targetInstalled) {
		if (!targetSide.has(model.source) || targetSide.has(model.sink)) {
			throw new Error("Interior-point target cut has invalid terminals");
		}
		const cut = edges.reduce(
			(sum, edge) =>
				targetSide.has(edge.from) && !targetSide.has(edge.to)
					? sum + BigInt(edge.capacity)
					: sum,
			0n,
		);
		if (cut !== target) {
			throw new Error("Interior-point target cut value is inconsistent");
		}
	} else if (target !== 0n || targetSide.size !== 0) {
		throw new Error("Interior-point ready state contains a target cut");
	}
	const relevant = edges.filter(
		(edge) => edge.to !== model.source && edge.from !== model.sink,
	).length;
	const expectedBNodes = BigInt(2 * relevant + 2 * (nodes.length - 2) + 2);
	const expectedBEdges = BigInt(3 * relevant + nodes.length - 2);
	const normalizedIndegree = new Map(nodes.map((node) => [node.id, 0]));
	const normalizedOutdegree = new Map(nodes.map((node) => [node.id, 0]));
	for (const edge of edges) {
		if (edge.to !== model.source && edge.from !== model.sink) {
			normalizedOutdegree.set(
				edge.from,
				(normalizedOutdegree.get(edge.from) ?? 0) + 1,
			);
			normalizedIndegree.set(
				edge.to,
				(normalizedIndegree.get(edge.to) ?? 0) + 1,
			);
		}
	}
	const sourceDemand =
		BigInt(normalizedOutdegree.get(model.source) ?? 0) - target;
	const sinkDemand = BigInt(normalizedIndegree.get(model.sink) ?? 0) - target;
	if (sourceDemand < 0n || sinkDemand < 0n) {
		throw new Error("Interior-point target exceeds terminal degree");
	}
	let expectedDirectArcs = 0n;
	for (const edge of edges) {
		if (edge.to === model.source || edge.from === model.sink) {
			continue;
		}
		const tailDemand =
			edge.from === model.source
				? sourceDemand
				: BigInt(normalizedOutdegree.get(edge.from) ?? 0);
		const headDemand =
			edge.to === model.sink
				? sinkDemand
				: BigInt(normalizedIndegree.get(edge.to) ?? 0);
		expectedDirectArcs +=
			1n + (tailDemand > 0n ? 1n : 0n) + (headDemand > 0n ? 1n : 0n);
	}
	for (const node of nodes) {
		if (node.id !== model.source && node.id !== model.sink) {
			expectedDirectArcs += BigInt(
				Math.min(
					normalizedIndegree.get(node.id) ?? 0,
					normalizedOutdegree.get(node.id) ?? 0,
				),
			);
		}
	}
	const expectedWorkingEdges = 2n * expectedBNodes + expectedDirectArcs;
	const bNodes = BigInt(overlay.b_matching_nodes);
	const bEdges = BigInt(overlay.b_matching_edges);
	const workingNodes = BigInt(overlay.working_nodes);
	const workingEdges = BigInt(overlay.working_edges);
	if (
		(!bBuilt && (bNodes !== 0n || bEdges !== 0n)) ||
		(bBuilt && (bNodes !== expectedBNodes || bEdges !== expectedBEdges)) ||
		(!workingBuilt && (workingNodes !== 0n || workingEdges !== 0n)) ||
		(workingBuilt &&
			(workingNodes !== expectedBNodes + 1n ||
				workingEdges !== expectedWorkingEdges ||
				workingNodes > 64n ||
				workingEdges > 192n)) ||
		(overlay.active_working_edge !== undefined &&
			BigInt(overlay.active_working_edge) >= workingEdges)
	) {
		throw new Error("Interior-point reduction sizes are inconsistent");
	}
	const mu = Number(overlay.mu);
	const gap = Number(overlay.duality_gap);
	const centrality = Number(overlay.centrality);
	if (
		(!initialized && (mu !== 0 || gap !== 0 || centrality !== 0)) ||
		(initialized && (mu <= 0 || gap <= 0)) ||
		(["descent-step", "solve-centering-direction"].includes(overlay.stage)
			? centrality > 3 / 400 + 1e-6
			: initialized && centrality > 1 / 400 + 1e-6) ||
		([
			"extract-fractional-flow",
			...["round-integral-flow", "check-certificate", "optimal"],
		].includes(overlay.stage) &&
			gap > 0.5 + 1e-6)
	) {
		throw new Error("Interior-point centrality or duality gap is inconsistent");
	}
	const graphEdgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const genericFlowById = new Map(
		edgeStates.map((state) => [state.edge_id, state.flow]),
	);
	for (const state of overlay.edges) {
		const edge = graphEdgeById.get(state.edge_id);
		if (edge === undefined) {
			throw new Error("Interior-point edge projection is incomplete");
		}
		const normalizedAway = edge.to === model.source || edge.from === model.sink;
		const positiveValues = [
			Number(state.fractional_flow),
			Number(state.slack),
			Number(state.measure),
			Number(state.resistance),
		];
		if (
			state.normalized_away !== normalizedAway ||
			(!workingBuilt && positiveValues.some((value) => value !== 0)) ||
			(workingBuilt &&
				!normalizedAway &&
				positiveValues.some((value) => value <= 0)) ||
			terminalFlows !== (state.final_flow !== undefined) ||
			(state.final_flow !== undefined &&
				(BigInt(state.final_flow) > 1n ||
					genericFlowById.get(state.edge_id) !== state.final_flow)) ||
			(!terminalFlows && genericFlowById.get(state.edge_id) !== "0")
		) {
			throw new Error(
				"Interior-point edge state or terminal flow is inconsistent",
			);
		}
	}
	const expectedStatus: Record<
		FlowInteriorPointMaxFlowOverlayV1["stage"],
		FlowCurrentSceneV9["solve_status"]
	> = {
		ready: "ready",
		"enumerate-target-cut": "running",
		"build-b-matching-reduction": "running",
		"build-min-cost-reduction": "running",
		"initialize-central-path": "running",
		"solve-electrical-direction": "running",
		"descent-step": "running",
		"solve-centering-direction": "running",
		"centering-step": "running",
		"extract-fractional-flow": "running",
		"round-integral-flow": "running",
		"check-certificate": "running",
		optimal: "optimal",
	};
	if (
		solveStatus !== "resource-limit" &&
		(solveStatus !== expectedStatus[overlay.stage] ||
			(overlay.stage === "ready" && eventId !== "0"))
	) {
		throw new Error("Interior-point stage and solve status disagree");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const catalog: Record<
			FlowInteriorPointMaxFlowOverlayV1["stage"],
			string[]
		> = {
			ready: ["interior-point-max-flow.ready"],
			"enumerate-target-cut": ["interior-point-max-flow.enumerate-target-cut"],
			"build-b-matching-reduction": [
				"interior-point-max-flow.build-b-matching-reduction",
			],
			"build-min-cost-reduction": [
				"interior-point-max-flow.build-min-cost-reduction",
			],
			"initialize-central-path": [
				"interior-point-max-flow.initialize-zero-centered",
			],
			"solve-electrical-direction": [
				"interior-point-max-flow.elimination-pivot",
				"interior-point-max-flow.solve-associated-electrical",
			],
			"descent-step": ["interior-point-max-flow.descent"],
			"solve-centering-direction": [
				"interior-point-max-flow.solve-centering-electrical",
			],
			"centering-step": ["interior-point-max-flow.center"],
			"extract-fractional-flow": [
				"interior-point-max-flow.extract-fractional-flow",
			],
			"round-integral-flow": ["interior-point-max-flow.round-integral-flow"],
			"check-certificate": ["interior-point-max-flow.check-certificate"],
			optimal: ["interior-point-max-flow.optimal"],
		};
		if (!catalog[overlay.stage].includes(traceEvent.catalog_id)) {
			throw new Error("Interior-point trace event and stage disagree");
		}
	}
	if (overlay.stage === "optimal") {
		if (
			outcome?.kind !== "max-flow" ||
			outcome.value !== overlay.target_value
		) {
			throw new Error("Interior-point optimal boundary lacks its certificate");
		}
	} else if (outcome !== undefined) {
		throw new Error("Interior-point outcome appears before completion");
	}
}

type MinimumRatioForest = {
	treeEdges: Set<string>;
	componentByNode: Map<string, number>;
	parentByNode: Map<string, string | undefined>;
	depthByNode: Map<string, number>;
	componentCount: number;
};

function buildMinimumRatioForest(
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
): MinimumRatioForest {
	const nodeOrder = canonicalNodeIds(nodes);
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const parent = new Map(nodeOrder.map((node) => [node, node]));
	const find = (node: string): string => {
		const current = parent.get(node);
		if (current === undefined)
			throw new Error("Ratio forest has an unknown node");
		if (current !== node) parent.set(node, find(current));
		return parent.get(node) as string;
	};
	const treeEdges = new Set<string>();
	for (const edgeId of canonicalStableIds(edges)) {
		const edge = edgeById.get(edgeId);
		if (edge === undefined) throw new Error("Ratio forest has an unknown edge");
		const left = find(edge.from);
		const right = find(edge.to);
		if (left !== right) {
			parent.set(left < right ? right : left, left < right ? left : right);
			treeEdges.add(edge.id);
		}
	}
	const adjacency = new Map(nodeOrder.map((node) => [node, [] as string[]]));
	for (const edge of edges) {
		if (treeEdges.has(edge.id)) {
			adjacency.get(edge.from)?.push(edge.to);
			adjacency.get(edge.to)?.push(edge.from);
		}
	}
	for (const neighbors of adjacency.values()) neighbors.sort();
	const componentByNode = new Map<string, number>();
	const parentByNode = new Map<string, string | undefined>();
	const depthByNode = new Map<string, number>();
	const seen = new Set<string>();
	let componentCount = 0;
	for (const root of nodeOrder) {
		if (seen.has(root)) continue;
		seen.add(root);
		componentByNode.set(root, componentCount);
		parentByNode.set(root, undefined);
		depthByNode.set(root, 0);
		const queue = [root];
		for (let cursor = 0; cursor < queue.length; cursor += 1) {
			const node = queue[cursor] as string;
			for (const next of adjacency.get(node) ?? []) {
				if (!seen.has(next)) {
					seen.add(next);
					componentByNode.set(next, componentCount);
					parentByNode.set(next, node);
					depthByNode.set(next, (depthByNode.get(node) ?? 0) + 1);
					queue.push(next);
				}
			}
		}
		componentCount += 1;
	}
	return {
		treeEdges,
		componentByNode,
		parentByNode,
		depthByNode,
		componentCount,
	};
}

function analyzeMinimumRatioSigns(
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	signs: number[],
): { numerator: bigint; denominator: bigint } | undefined {
	if (signs.length !== edges.length || signs.every((sign) => sign === 0)) {
		return undefined;
	}
	const balance = new Map(nodes.map((node) => [node.id, 0]));
	const degree = new Map(nodes.map((node) => [node.id, 0]));
	const adjacency = new Map(nodes.map((node) => [node.id, [] as string[]]));
	let numerator = 0n;
	let denominator = 0n;
	for (let index = 0; index < edges.length; index += 1) {
		const edge = edges[index] as FlowEdgeV1;
		const sign = signs[index] as number;
		if (![0, -1, 1].includes(sign)) return undefined;
		if (sign === 0) continue;
		balance.set(edge.from, (balance.get(edge.from) ?? 0) - sign);
		balance.set(edge.to, (balance.get(edge.to) ?? 0) + sign);
		degree.set(edge.from, (degree.get(edge.from) ?? 0) + 1);
		degree.set(edge.to, (degree.get(edge.to) ?? 0) + 1);
		adjacency.get(edge.from)?.push(edge.to);
		adjacency.get(edge.to)?.push(edge.from);
		numerator += BigInt(edge.cost) * BigInt(sign);
		denominator += BigInt(edge.capacity);
	}
	if (
		[...balance.values()].some((value) => value !== 0) ||
		[...degree.values()].some((value) => value !== 0 && value !== 2)
	) {
		return undefined;
	}
	const start = nodes.find((node) => (degree.get(node.id) ?? 0) !== 0)?.id;
	if (start === undefined) return undefined;
	const seen = new Set([start]);
	const stack = [start];
	while (stack.length > 0) {
		const node = stack.pop() as string;
		for (const next of adjacency.get(node) ?? []) {
			if (!seen.has(next)) {
				seen.add(next);
				stack.push(next);
			}
		}
	}
	if (
		nodes.some((node) => (degree.get(node.id) ?? 0) !== 0 && !seen.has(node.id))
	) {
		return undefined;
	}
	return { numerator, denominator };
}

function normalizedMinimumRatio(
	numerator: bigint,
	denominator: bigint,
): FlowRationalV1 {
	let left = numerator < 0n ? -numerator : numerator;
	let right = denominator;
	while (right !== 0n) {
		const remainder = left % right;
		left = right;
		right = remainder;
	}
	const divisor = left === 0n ? denominator : left;
	return {
		numerator: (numerator / divisor).toString(),
		denominator: (denominator / divisor).toString(),
	};
}

function exactMinimumRatio(
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
): FlowRationalV1 | undefined {
	const vectorCount = 3 ** edges.length;
	let best: FlowRationalV1 | undefined;
	for (let code = 1; code < vectorCount; code += 1) {
		let remaining = code;
		const signs = new Array<number>(edges.length).fill(0);
		for (let index = 0; index < edges.length; index += 1) {
			const digit = remaining % 3;
			signs[index] = digit === 0 ? 0 : digit === 1 ? 1 : -1;
			remaining = Math.floor(remaining / 3);
		}
		if (signs.find((sign) => sign !== 0) !== 1) continue;
		const cycle = analyzeMinimumRatioSigns(nodes, edges, signs);
		if (cycle === undefined) continue;
		const oriented = normalizedMinimumRatio(
			cycle.numerator > 0n ? -cycle.numerator : cycle.numerator,
			cycle.denominator,
		);
		if (best === undefined || compareRational(oriented, best) < 0)
			best = oriented;
	}
	return best;
}

function validateMinimumRatioCycleScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	residualArcs: FlowResidualArcStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowMinimumRatioCycleOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "minimum-ratio-cycle-max-flow";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Minimum-ratio-cycle overlay uses the wrong algorithm");
		}
		return;
	}
	if (model.kind !== "max-flow" || Object.keys(config).length !== 0) {
		throw new Error(
			"Minimum-ratio-cycle requires an unconfigured max-flow model",
		);
	}
	if (
		nodes.length < 2 ||
		nodes.length > 8 ||
		edges.length < 1 ||
		edges.length > 11 ||
		nodes.some((node) => node.supply !== "0") ||
		edges.some(
			(edge) =>
				edge.lower !== "0" ||
				BigInt(edge.capacity) <= 0n ||
				BigInt(edge.capacity) > 1_000_000n ||
				BigInt(edge.cost) < -1_000_000n ||
				BigInt(edge.cost) > 1_000_000n ||
				edge.from === edge.to,
		)
	) {
		throw new Error("Minimum-ratio-cycle graph is outside its bounded domain");
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined)
		) {
			return;
		}
		throw new Error("Minimum-ratio-cycle scene is missing its typed overlay");
	}
	const nodeOrder = canonicalNodeIds(nodes);
	const edgeOrder = canonicalStableIds(edges);
	if (
		overlay.nodes.length !== nodeOrder.length ||
		overlay.nodes.some((node, index) => node.node_id !== nodeOrder[index]) ||
		overlay.edges.length !== edgeOrder.length ||
		overlay.edges.some((edge, index) => edge.edge_id !== edgeOrder[index]) ||
		edgeStates.length !== edgeOrder.length ||
		edgeStates.some(
			(state, index) =>
				state.edge_id !== edgeOrder[index] || state.flow !== "0",
		) ||
		residualArcs.length !== 0
	) {
		throw new Error("Minimum-ratio-cycle stable identities are inconsistent");
	}
	const forestBuilt = !["ready", "map-gradient-length"].includes(overlay.stage);
	const candidateVisible = ["evaluate-cycle", "update-best"].includes(
		overlay.stage,
	);
	const vectorInspection = overlay.stage === "inspect-vector";
	const vectorCount = 3n ** BigInt(edges.length);
	if (
		BigInt(overlay.enumerated_vectors) >= vectorCount ||
		BigInt(overlay.simple_cycles) > BigInt(overlay.enumerated_vectors) ||
		BigInt(overlay.selected_edge_count) > BigInt(edges.length) ||
		(overlay.stage === "complete" &&
			BigInt(overlay.enumerated_vectors) !== vectorCount - 1n) ||
		candidateVisible !== (overlay.candidate_ratio !== undefined)
	) {
		throw new Error("Minimum-ratio-cycle bounded counters are inconsistent");
	}
	const forest = buildMinimumRatioForest(nodes, edges);
	const expectedFundamental = BigInt(
		edges.length - nodes.length + forest.componentCount,
	);
	if (
		(forestBuilt &&
			BigInt(overlay.fundamental_cycles) !== expectedFundamental) ||
		(!forestBuilt && overlay.fundamental_cycles !== "0")
	) {
		throw new Error("Minimum-ratio-cycle forest dimension is inconsistent");
	}
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const orderedEdges = edgeOrder.map((edgeId) => {
		const edge = edgeById.get(edgeId);
		if (edge === undefined)
			throw new Error("Minimum-ratio-cycle edge is missing");
		return edge;
	});
	const candidateSigns: number[] = [];
	const selectedSigns: number[] = [];
	const candidateBalance = new Map(nodeOrder.map((node) => [node, 0]));
	const candidateIncident = new Set<string>();
	const selectedIncident = new Set<string>();
	for (const state of overlay.edges) {
		const edge = edgeById.get(state.edge_id);
		if (edge === undefined)
			throw new Error("Minimum-ratio-cycle edge is missing");
		const candidateSign = Number(state.candidate_sign);
		const selectedSign = Number(state.selected_sign);
		candidateSigns.push(candidateSign);
		selectedSigns.push(selectedSign);
		if (
			state.gradient !== edge.cost ||
			state.length !== edge.capacity ||
			state.tree_edge !== (forestBuilt && forest.treeEdges.has(edge.id)) ||
			(!candidateVisible && !vectorInspection && candidateSign !== 0) ||
			BigInt(state.numerator_contribution) !==
				BigInt(edge.cost) * BigInt(candidateSign) ||
			BigInt(state.denominator_contribution) !==
				(candidateSign === 0 ? 0n : BigInt(edge.capacity))
		) {
			throw new Error("Minimum-ratio-cycle edge objective is inconsistent");
		}
		candidateBalance.set(
			edge.from,
			(candidateBalance.get(edge.from) ?? 0) - candidateSign,
		);
		candidateBalance.set(
			edge.to,
			(candidateBalance.get(edge.to) ?? 0) + candidateSign,
		);
		if (candidateSign !== 0) {
			candidateIncident.add(edge.from);
			candidateIncident.add(edge.to);
		}
		if (selectedSign !== 0) {
			selectedIncident.add(edge.from);
			selectedIncident.add(edge.to);
		}
	}
	const maximumBalance = Math.max(
		0,
		...[...candidateBalance.values()].map(Math.abs),
	);
	if (
		BigInt(overlay.maximum_absolute_balance) !== BigInt(maximumBalance) ||
		selectedSigns.filter((sign) => sign !== 0).length !==
			Number(overlay.selected_edge_count)
	) {
		throw new Error("Minimum-ratio-cycle incidence summary is inconsistent");
	}
	for (let index = 0; index < overlay.nodes.length; index += 1) {
		const state = overlay.nodes[index] as FlowMinimumRatioCycleNodeStateV1;
		const nodeId = nodeOrder[index] as string;
		const expectedComponent = forestBuilt
			? forest.componentByNode.get(nodeId)
			: index;
		const expectedParent = forestBuilt
			? forest.parentByNode.get(nodeId)
			: undefined;
		const expectedDepth = forestBuilt ? forest.depthByNode.get(nodeId) : 0;
		if (
			Number(state.component) !== expectedComponent ||
			state.parent_node_id !== expectedParent ||
			Number(state.depth) !== expectedDepth ||
			Number(state.candidate_balance) !== candidateBalance.get(nodeId) ||
			state.on_candidate !== candidateIncident.has(nodeId) ||
			state.on_selected !== selectedIncident.has(nodeId)
		) {
			throw new Error("Minimum-ratio-cycle node/forest state is inconsistent");
		}
	}
	const candidate = analyzeMinimumRatioSigns(
		nodes,
		orderedEdges,
		candidateSigns,
	);
	const selectedCycle = analyzeMinimumRatioSigns(
		nodes,
		orderedEdges,
		selectedSigns,
	);
	const candidateRatio =
		candidate === undefined
			? undefined
			: normalizedMinimumRatio(candidate.numerator, candidate.denominator);
	const selectedRatio =
		selectedCycle === undefined
			? undefined
			: normalizedMinimumRatio(
					selectedCycle.numerator,
					selectedCycle.denominator,
				);
	if (
		(candidateVisible && candidate !== undefined && candidate.numerator > 0n) ||
		(selectedCycle !== undefined && selectedCycle.numerator > 0n) ||
		(candidateVisible &&
			(candidateRatio === undefined) !==
				(overlay.candidate_ratio === undefined)) ||
		(candidateVisible &&
			candidateRatio !== undefined &&
			compareRational(
				candidateRatio,
				overlay.candidate_ratio as FlowRationalV1,
			) !== 0) ||
		(selectedRatio === undefined) !== (overlay.best_ratio === undefined) ||
		(selectedRatio !== undefined &&
			compareRational(selectedRatio, overlay.best_ratio as FlowRationalV1) !==
				0) ||
		(["verify-cycle-space", "check-exhaustive-oracle", "complete"].includes(
			overlay.stage,
		) &&
			(maximumBalance !== 0 || candidateSigns.some((sign) => sign !== 0)))
	) {
		throw new Error("Minimum-ratio-cycle ratio or circulation is inconsistent");
	}
	const expectedStatus: Record<
		FlowMinimumRatioCycleOverlayV1["stage"],
		FlowCurrentSceneV9["solve_status"]
	> = {
		ready: "ready",
		"map-gradient-length": "running",
		"build-spanning-forest": "running",
		"inspect-vector": "running",
		"evaluate-cycle": "running",
		"update-best": "running",
		"verify-cycle-space": "running",
		"check-exhaustive-oracle": "running",
		complete: "primitive-complete",
	};
	if (
		solveStatus !== "resource-limit" &&
		(solveStatus !== expectedStatus[overlay.stage] ||
			(overlay.stage === "ready" && eventId !== "0"))
	) {
		throw new Error("Minimum-ratio-cycle stage and solve status disagree");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const catalog: Record<FlowMinimumRatioCycleOverlayV1["stage"], string> = {
			ready: "minimum-ratio-cycle-max-flow.ready",
			"map-gradient-length": "minimum-ratio-cycle-max-flow.map-gradient-length",
			"build-spanning-forest":
				"minimum-ratio-cycle-max-flow.build-spanning-forest",
			"inspect-vector":
				"minimum-ratio-cycle-max-flow.inspect-vector-checkpoint",
			"evaluate-cycle": "minimum-ratio-cycle-max-flow.evaluate-cycle",
			"update-best": "minimum-ratio-cycle-max-flow.update-best",
			"verify-cycle-space": "minimum-ratio-cycle-max-flow.verify-cycle-space",
			"check-exhaustive-oracle":
				"minimum-ratio-cycle-max-flow.check-dfs-oracle",
			complete: "minimum-ratio-cycle-max-flow.complete-primitive",
		};
		if (traceEvent.catalog_id !== catalog[overlay.stage]) {
			throw new Error("Minimum-ratio-cycle trace event and stage disagree");
		}
	}
	if (overlay.stage === "complete") {
		const exact = exactMinimumRatio(nodes, orderedEdges);
		if (
			(exact === undefined) !== (overlay.best_ratio === undefined) ||
			(exact !== undefined &&
				compareRational(exact, overlay.best_ratio as FlowRationalV1) !== 0) ||
			outcome?.kind !== "minimum-ratio-cycle" ||
			(outcome.ratio === undefined) !== (overlay.best_ratio === undefined) ||
			(outcome.ratio !== undefined &&
				compareRational(outcome.ratio, overlay.best_ratio as FlowRationalV1) !==
					0) ||
			outcome.simple_cycles !== overlay.simple_cycles ||
			outcome.enumerated_vectors !== overlay.enumerated_vectors ||
			outcome.cycle.length !== Number(overlay.selected_edge_count) ||
			outcome.cycle.some((arc) => {
				const state = overlay.edges.find(
					(edge) => edge.edge_id === arc.edge_id,
				);
				return state === undefined || state.selected_sign !== arc.sign;
			})
		) {
			throw new Error("Minimum-ratio-cycle completion lacks its exact outcome");
		}
	} else if (outcome !== undefined) {
		throw new Error("Minimum-ratio-cycle outcome appears before completion");
	}
}

function minimumRatioCycleMcfStageRank(
	stage: FlowMinimumRatioCycleMcfOverlayV1["stage"],
): number {
	return [
		"ready",
		"enumerate-feasible-set",
		"contract-fixed-face",
		"initialize-strict-interior",
		"evaluate-potential",
		"map-gradient-length",
		"build-spanning-forest",
		"inspect-vector",
		"evaluate-cycle",
		"update-best",
		"verify-cycle-space",
		"apply-source-step",
		"measure-potential-decrease",
		"check-dfs-oracle",
		"complete",
	].indexOf(stage);
}

function analyzeMinimumRatioMcfSigns(
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	states: FlowMinimumRatioCycleMcfEdgeStateV1[],
	signs: number[],
): { numerator: number; denominator: number } | undefined {
	if (signs.length !== edges.length || signs.every((sign) => sign === 0)) {
		return undefined;
	}
	const balance = new Map(nodes.map((node) => [node.id, 0]));
	const degree = new Map(nodes.map((node) => [node.id, 0]));
	const adjacency = new Map(nodes.map((node) => [node.id, [] as string[]]));
	let numerator = 0;
	let denominator = 0;
	for (let index = 0; index < edges.length; index += 1) {
		const edge = edges[index] as FlowEdgeV1;
		const state = states[index] as FlowMinimumRatioCycleMcfEdgeStateV1;
		const sign = signs[index] as number;
		if (![0, -1, 1].includes(sign) || (state.fixed_on_face && sign !== 0)) {
			return undefined;
		}
		if (sign === 0) continue;
		balance.set(edge.from, (balance.get(edge.from) ?? 0) - sign);
		balance.set(edge.to, (balance.get(edge.to) ?? 0) + sign);
		degree.set(edge.from, (degree.get(edge.from) ?? 0) + 1);
		degree.set(edge.to, (degree.get(edge.to) ?? 0) + 1);
		adjacency.get(edge.from)?.push(edge.to);
		adjacency.get(edge.to)?.push(edge.from);
		numerator += Number(state.gradient) * sign;
		denominator += Number(state.length);
	}
	if (
		[...balance.values()].some((value) => value !== 0) ||
		[...degree.values()].some((value) => value !== 0 && value !== 2) ||
		!(denominator > 0)
	) {
		return undefined;
	}
	const start = nodes.find((node) => (degree.get(node.id) ?? 0) !== 0)?.id;
	if (start === undefined) return undefined;
	const seen = new Set([start]);
	const stack = [start];
	while (stack.length > 0) {
		const node = stack.pop() as string;
		for (const next of adjacency.get(node) ?? []) {
			if (!seen.has(next)) {
				seen.add(next);
				stack.push(next);
			}
		}
	}
	if (
		nodes.some((node) => (degree.get(node.id) ?? 0) !== 0 && !seen.has(node.id))
	) {
		return undefined;
	}
	return { numerator, denominator };
}

function exactMinimumRatioMcf(
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	states: FlowMinimumRatioCycleMcfEdgeStateV1[],
): number | undefined {
	const active = states
		.map((state, index) => ({ state, index }))
		.filter(({ state }) => !state.fixed_on_face)
		.map(({ index }) => index);
	let best: number | undefined;
	for (let code = 1; code < 3 ** active.length; code += 1) {
		let remaining = code;
		const signs = new Array<number>(edges.length).fill(0);
		for (const index of active) {
			const digit = remaining % 3;
			signs[index] = digit === 0 ? 0 : digit === 1 ? 1 : -1;
			remaining = Math.floor(remaining / 3);
		}
		if (signs.find((sign) => sign !== 0) !== 1) continue;
		const cycle = analyzeMinimumRatioMcfSigns(nodes, edges, states, signs);
		if (cycle === undefined) continue;
		const ratio = -Math.abs(cycle.numerator) / cycle.denominator;
		if (best === undefined || ratio < best) best = ratio;
	}
	return best;
}

function validateMinimumRatioCycleMcfScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	residualArcs: FlowResidualArcStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowMinimumRatioCycleMcfOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "minimum-ratio-cycle-mcf";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error(
				"Minimum-ratio-cycle MCF overlay uses the wrong algorithm",
			);
		}
		return;
	}
	if (
		!(
			["fixed-flow-min-cost", "circulation", "transshipment"] as const
		).includes(
			model.kind as "fixed-flow-min-cost" | "circulation" | "transshipment",
		) ||
		Object.keys(config).length !== 0
	) {
		throw new Error(
			"Minimum-ratio-cycle MCF requires an unconfigured MCF model",
		);
	}
	if (
		nodes.length < 2 ||
		nodes.length > 6 ||
		edges.length < 1 ||
		edges.length > 8 ||
		edges.some(
			(edge) =>
				edge.from === edge.to ||
				BigInt(edge.capacity) > 8n ||
				BigInt(edge.cost) < -32n ||
				BigInt(edge.cost) > 32n,
		)
	) {
		throw new Error(
			"Minimum-ratio-cycle MCF graph is outside its bounded domain",
		);
	}
	const assignments = edges.reduce(
		(product, edge) =>
			product * (BigInt(edge.capacity) - BigInt(edge.lower) + 1n),
		1n,
	);
	if (assignments > 100_000n) {
		throw new Error("Minimum-ratio-cycle MCF assignment guard is exceeded");
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined)
		) {
			return;
		}
		throw new Error(
			"Minimum-ratio-cycle MCF scene is missing its typed overlay",
		);
	}
	const nodeOrder = canonicalNodeIds(nodes);
	const edgeOrder = canonicalStableIds(edges);
	if (
		overlay.nodes.length !== nodeOrder.length ||
		overlay.nodes.some((node, index) => node.node_id !== nodeOrder[index]) ||
		overlay.edges.length !== edgeOrder.length ||
		overlay.edges.some((edge, index) => edge.edge_id !== edgeOrder[index]) ||
		edgeStates.length !== edgeOrder.length ||
		edgeStates.some(
			(state, index) =>
				state.edge_id !== edgeOrder[index] || state.flow !== "0",
		) ||
		residualArcs.length !== 0
	) {
		throw new Error(
			"Minimum-ratio-cycle MCF stable identities are inconsistent",
		);
	}
	const graphEdgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const orderedEdges = edgeOrder.map((edgeId) => {
		const edge = graphEdgeById.get(edgeId);
		if (edge === undefined)
			throw new Error("Minimum-ratio-cycle MCF edge is missing");
		return edge;
	});
	const potentialVisible = minimumRatioCycleMcfStageRank(overlay.stage) >= 4;
	const mapped =
		!overlay.stationary && minimumRatioCycleMcfStageRank(overlay.stage) >= 5;
	const forestBuilt =
		!overlay.stationary && minimumRatioCycleMcfStageRank(overlay.stage) >= 6;
	const applied =
		!overlay.stationary && minimumRatioCycleMcfStageRank(overlay.stage) >= 11;
	const measured =
		!overlay.stationary && minimumRatioCycleMcfStageRank(overlay.stage) >= 12;
	const candidateVisible = ["evaluate-cycle", "update-best"].includes(
		overlay.stage,
	);
	const vectorInspection = overlay.stage === "inspect-vector";
	if (candidateVisible !== (overlay.candidate_ratio !== undefined)) {
		throw new Error(
			"Minimum-ratio-cycle MCF candidate visibility is inconsistent",
		);
	}
	const alpha = Number(overlay.alpha);
	const optimum = Number(BigInt(overlay.optimum_cost));
	const initialCost = Number(overlay.initial_cost);
	const currentCost = Number(overlay.current_cost);
	const gap = Number(overlay.cost_gap);
	const kappa = Number(overlay.kappa);
	const eta = Number(overlay.eta);
	const weighted = Number(overlay.weighted_step_norm);
	const decrease = Number(overlay.potential_decrease);
	const guaranteed = Number(overlay.guaranteed_decrease);
	if (
		!(alpha > 0 && alpha < 1) ||
		gap < 0 ||
		kappa < 0 ||
		kappa > 0.99 ||
		eta < 0 ||
		weighted < 0 ||
		decrease < 0 ||
		guaranteed < 0 ||
		BigInt(overlay.feasible_flows) === 0n ||
		BigInt(overlay.enumerated_vectors) > 6_561n ||
		BigInt(overlay.simple_cycles) > BigInt(overlay.enumerated_vectors)
	) {
		throw new Error("Minimum-ratio-cycle MCF scalar summary is inconsistent");
	}
	const fixed = overlay.edges.map((state) => state.fixed_on_face);
	const activeCount = fixed.filter((value) => !value).length;
	const maximumCapacity = Math.max(
		2,
		...orderedEdges.map((edge) => Number(edge.capacity)),
	);
	const expectedAlpha =
		1 /
		(1_000 * Math.log(Math.max(2, Math.max(1, activeCount) * maximumCapacity)));
	if (!electricalClose(alpha, expectedAlpha)) {
		throw new Error("Minimum-ratio-cycle MCF alpha is inconsistent");
	}
	const activeEdges = orderedEdges.filter((_, index) => !fixed[index]);
	const forest = buildMinimumRatioForest(nodes, activeEdges);
	const expectedFundamental = BigInt(
		activeEdges.length - nodes.length + forest.componentCount,
	);
	if (
		(forestBuilt &&
			BigInt(overlay.fundamental_cycles) !== expectedFundamental) ||
		(!forestBuilt && overlay.fundamental_cycles !== "0")
	) {
		throw new Error("Minimum-ratio-cycle MCF forest dimension is inconsistent");
	}
	const initial: number[] = [];
	const updated: number[] = [];
	const gradients: number[] = [];
	const lengths: number[] = [];
	const candidateSigns: number[] = [];
	const selectedSigns: number[] = [];
	let candidateNumerator = 0;
	let candidateDenominator = 0;
	for (let index = 0; index < overlay.edges.length; index += 1) {
		const state = overlay.edges[index] as FlowMinimumRatioCycleMcfEdgeStateV1;
		const edge = orderedEdges[index] as FlowEdgeV1;
		const before = Number(state.initial_flow);
		const after = Number(state.updated_flow);
		const lower = Number(state.lower_slack);
		const upper = Number(state.upper_slack);
		const gradient = Number(state.gradient);
		const length = Number(state.length);
		const candidateSign = Number(state.candidate_sign);
		const selectedSign = Number(state.selected_sign);
		if (
			before < Number(edge.lower) ||
			before > Number(edge.capacity) ||
			after < Number(edge.lower) - 1e-9 ||
			after > Number(edge.capacity) + 1e-9 ||
			!electricalClose(lower, before - Number(edge.lower)) ||
			!electricalClose(upper, Number(edge.capacity) - before) ||
			(state.fixed_on_face && (candidateSign !== 0 || selectedSign !== 0)) ||
			(!candidateVisible && !vectorInspection && candidateSign !== 0) ||
			(!forestBuilt && state.tree_edge) ||
			(forestBuilt && state.tree_edge !== forest.treeEdges.has(edge.id))
		) {
			throw new Error("Minimum-ratio-cycle MCF edge geometry is inconsistent");
		}
		if (mapped && !state.fixed_on_face) {
			const expectedLength = upper ** (-1 - alpha) + lower ** (-1 - alpha);
			const expectedGradient =
				(20 * Math.max(1, activeCount) * Number(edge.cost)) /
					(initialCost - optimum) +
				alpha * (upper ** (-1 - alpha) - lower ** (-1 - alpha));
			if (
				lower <= 0 ||
				upper <= 0 ||
				!electricalClose(length, expectedLength) ||
				!electricalClose(gradient, expectedGradient)
			) {
				throw new Error("Minimum-ratio-cycle MCF source map is inconsistent");
			}
		} else if (
			(!mapped || state.fixed_on_face) &&
			(!electricalClose(gradient, 0) || !electricalClose(length, 0))
		) {
			throw new Error("Minimum-ratio-cycle MCF unmapped edge is nonzero");
		}
		const numerator = Number(state.numerator_contribution);
		const denominator = Number(state.denominator_contribution);
		if (
			!electricalClose(numerator, gradient * candidateSign) ||
			!electricalClose(denominator, candidateSign === 0 ? 0 : length)
		) {
			throw new Error("Minimum-ratio-cycle MCF contributions are inconsistent");
		}
		initial.push(before);
		updated.push(after);
		gradients.push(gradient);
		lengths.push(length);
		candidateSigns.push(candidateSign);
		selectedSigns.push(selectedSign);
		candidateNumerator += numerator;
		candidateDenominator += denominator;
	}
	const recomputedInitialCost = orderedEdges.reduce(
		(sum, edge, index) => sum + Number(edge.cost) * (initial[index] as number),
		0,
	);
	const recomputedCurrentCost = orderedEdges.reduce(
		(sum, edge, index) => sum + Number(edge.cost) * (updated[index] as number),
		0,
	);
	if (
		!electricalClose(initialCost, recomputedInitialCost) ||
		!electricalClose(currentCost, recomputedCurrentCost) ||
		!electricalClose(gap, currentCost - optimum)
	) {
		throw new Error("Minimum-ratio-cycle MCF objective gap is inconsistent");
	}
	const sourcePotential = (flow: number[]): number => {
		const objective = orderedEdges.reduce(
			(sum, edge, index) => sum + Number(edge.cost) * (flow[index] as number),
			0,
		);
		const objectiveGap = objective - optimum;
		if (!(objectiveGap > 0)) return Number.NaN;
		const barriers = orderedEdges.reduce(
			(sum, edge, index) =>
				fixed[index]
					? sum
					: sum +
						(Number(edge.capacity) - (flow[index] as number)) ** -alpha +
						((flow[index] as number) - Number(edge.lower)) ** -alpha,
			0,
		);
		return 20 * Math.max(1, activeCount) * Math.log(objectiveGap) + barriers;
	};
	if (overlay.stationary) {
		if (
			gap > 1e-9 ||
			overlay.best_ratio !== undefined ||
			selectedSigns.some((sign) => sign !== 0) ||
			!electricalClose(kappa, 0) ||
			!electricalClose(eta, 0)
		) {
			throw new Error(
				"Minimum-ratio-cycle MCF stationary state is inconsistent",
			);
		}
	} else {
		const beforePotential = sourcePotential(initial);
		if (
			(potentialVisible &&
				!electricalClose(Number(overlay.potential_before), beforePotential)) ||
			(!potentialVisible &&
				(Number(overlay.potential_before) !== 0 ||
					Number(overlay.current_potential) !== 0))
		) {
			throw new Error("Minimum-ratio-cycle MCF potential is inconsistent");
		}
	}
	if (
		candidateVisible &&
		(!(candidateDenominator > 0) ||
			!electricalClose(
				Number(overlay.candidate_ratio),
				candidateNumerator / candidateDenominator,
			))
	) {
		throw new Error("Minimum-ratio-cycle MCF candidate ratio is inconsistent");
	}
	const candidateBalance = new Map(nodeOrder.map((node) => [node, 0]));
	const selectedBalance = new Map(nodeOrder.map((node) => [node, 0]));
	const candidateIncident = new Set<string>();
	const selectedIncident = new Set<string>();
	for (let index = 0; index < orderedEdges.length; index += 1) {
		const edge = orderedEdges[index] as FlowEdgeV1;
		const candidateSign = candidateSigns[index] as number;
		const selectedSign = selectedSigns[index] as number;
		candidateBalance.set(
			edge.from,
			(candidateBalance.get(edge.from) ?? 0) - candidateSign,
		);
		candidateBalance.set(
			edge.to,
			(candidateBalance.get(edge.to) ?? 0) + candidateSign,
		);
		selectedBalance.set(
			edge.from,
			(selectedBalance.get(edge.from) ?? 0) - selectedSign,
		);
		selectedBalance.set(
			edge.to,
			(selectedBalance.get(edge.to) ?? 0) + selectedSign,
		);
		if (candidateSign !== 0) {
			candidateIncident.add(edge.from);
			candidateIncident.add(edge.to);
		}
		if (selectedSign !== 0) {
			selectedIncident.add(edge.from);
			selectedIncident.add(edge.to);
		}
	}
	const maximumBalance = Math.max(
		0,
		...[...candidateBalance.values()].map(Math.abs),
	);
	if (
		BigInt(overlay.maximum_absolute_balance) !== BigInt(maximumBalance) ||
		Number(overlay.selected_edge_count) !==
			selectedSigns.filter((sign) => sign !== 0).length ||
		(selectedSigns.some((sign) => sign !== 0) &&
			[...selectedBalance.values()].some((balance) => balance !== 0))
	) {
		throw new Error(
			"Minimum-ratio-cycle MCF incidence summary is inconsistent",
		);
	}
	for (let index = 0; index < overlay.nodes.length; index += 1) {
		const state = overlay.nodes[index] as FlowMinimumRatioCycleMcfNodeStateV1;
		const nodeId = nodeOrder[index] as string;
		const expectedComponent = forestBuilt
			? forest.componentByNode.get(nodeId)
			: index;
		const expectedParent = forestBuilt
			? forest.parentByNode.get(nodeId)
			: undefined;
		const expectedDepth = forestBuilt ? forest.depthByNode.get(nodeId) : 0;
		if (
			Number(state.component) !== expectedComponent ||
			state.parent_node_id !== expectedParent ||
			Number(state.depth) !== expectedDepth ||
			Number(state.candidate_balance) !== candidateBalance.get(nodeId) ||
			state.on_candidate !== candidateIncident.has(nodeId) ||
			state.on_selected !== selectedIncident.has(nodeId)
		) {
			throw new Error("Minimum-ratio-cycle MCF node state is inconsistent");
		}
	}
	const selectedCycle = analyzeMinimumRatioMcfSigns(
		nodes,
		orderedEdges,
		overlay.edges,
		selectedSigns,
	);
	if (
		(selectedCycle === undefined) !== (overlay.best_ratio === undefined) ||
		(selectedCycle !== undefined &&
			!electricalClose(
				Number(overlay.best_ratio),
				selectedCycle.numerator / selectedCycle.denominator,
			))
	) {
		throw new Error("Minimum-ratio-cycle MCF selected ratio is inconsistent");
	}
	if (applied && !overlay.stationary && selectedCycle !== undefined) {
		const ratio = selectedCycle.numerator / selectedCycle.denominator;
		const expectedKappa = Math.min(-ratio, 0.99);
		const expectedEta =
			(expectedKappa * expectedKappa) / (50 * -selectedCycle.numerator);
		if (
			!electricalClose(kappa, expectedKappa) ||
			!electricalClose(eta, expectedEta) ||
			!electricalClose(weighted, eta * selectedCycle.denominator) ||
			updated.some(
				(value, index) =>
					!electricalClose(
						value,
						(initial[index] as number) + eta * (selectedSigns[index] as number),
					),
			)
		) {
			throw new Error("Minimum-ratio-cycle MCF source step is inconsistent");
		}
	} else if (
		!applied &&
		(updated.some(
			(value, index) => !electricalClose(value, initial[index] as number),
		) ||
			kappa !== 0 ||
			eta !== 0 ||
			weighted !== 0 ||
			guaranteed !== 0)
	) {
		throw new Error("Minimum-ratio-cycle MCF step appears before its boundary");
	}
	if (measured && !overlay.stationary) {
		const afterPotential = sourcePotential(updated);
		const beforePotential = Number(overlay.potential_before);
		if (
			!electricalClose(Number(overlay.current_potential), afterPotential) ||
			!electricalClose(decrease, beforePotential - afterPotential) ||
			!electricalClose(guaranteed, (kappa * kappa) / 500) ||
			decrease + 1e-8 * Math.max(1, Math.abs(beforePotential)) < guaranteed ||
			weighted > kappa / 25 + 1e-8
		) {
			throw new Error("Minimum-ratio-cycle MCF progress bound is inconsistent");
		}
	} else if (decrease !== 0) {
		throw new Error("Minimum-ratio-cycle MCF decrease appears too early");
	}
	const expectedStatus: Record<
		FlowMinimumRatioCycleMcfOverlayV1["stage"],
		FlowCurrentSceneV9["solve_status"]
	> = {
		ready: "ready",
		"enumerate-feasible-set": "running",
		"contract-fixed-face": "running",
		"initialize-strict-interior": "running",
		"evaluate-potential": "running",
		"map-gradient-length": "running",
		"build-spanning-forest": "running",
		"inspect-vector": "running",
		"evaluate-cycle": "running",
		"update-best": "running",
		"verify-cycle-space": "running",
		"apply-source-step": "running",
		"measure-potential-decrease": "running",
		"check-dfs-oracle": "running",
		complete: "primitive-complete",
	};
	if (
		solveStatus !== "resource-limit" &&
		(solveStatus !== expectedStatus[overlay.stage] ||
			(overlay.stage === "ready" && eventId !== "0"))
	) {
		throw new Error("Minimum-ratio-cycle MCF stage and solve status disagree");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const catalog: Record<FlowMinimumRatioCycleMcfOverlayV1["stage"], string> =
			{
				ready: "minimum-ratio-cycle-mcf.ready",
				"enumerate-feasible-set":
					"minimum-ratio-cycle-mcf.enumerate-feasible-set",
				"contract-fixed-face": "minimum-ratio-cycle-mcf.contract-fixed-face",
				"initialize-strict-interior":
					"minimum-ratio-cycle-mcf.initialize-strict-interior",
				"evaluate-potential": "minimum-ratio-cycle-mcf.evaluate-potential",
				"map-gradient-length": "minimum-ratio-cycle-mcf.map-gradient-length",
				"build-spanning-forest":
					"minimum-ratio-cycle-mcf.build-spanning-forest",
				"inspect-vector": "minimum-ratio-cycle-mcf.inspect-vector-checkpoint",
				"evaluate-cycle": "minimum-ratio-cycle-mcf.evaluate-cycle",
				"update-best": "minimum-ratio-cycle-mcf.update-best",
				"verify-cycle-space": "minimum-ratio-cycle-mcf.verify-cycle-space",
				"apply-source-step": "minimum-ratio-cycle-mcf.apply-source-step",
				"measure-potential-decrease":
					"minimum-ratio-cycle-mcf.measure-potential-decrease",
				"check-dfs-oracle": "minimum-ratio-cycle-mcf.check-dfs-oracle",
				complete: "minimum-ratio-cycle-mcf.complete-primitive",
			};
		if (traceEvent.catalog_id !== catalog[overlay.stage]) {
			throw new Error("Minimum-ratio-cycle MCF trace event and stage disagree");
		}
	}
	if (overlay.stage === "complete") {
		const exact = exactMinimumRatioMcf(nodes, orderedEdges, overlay.edges);
		if (
			(exact === undefined) !== (overlay.best_ratio === undefined) ||
			(exact !== undefined &&
				!electricalClose(exact, Number(overlay.best_ratio))) ||
			outcome?.kind !== "minimum-ratio-cycle-mcf" ||
			(outcome.ratio === undefined) !== (overlay.best_ratio === undefined) ||
			(outcome.ratio !== undefined &&
				!electricalClose(Number(outcome.ratio), Number(overlay.best_ratio))) ||
			outcome.alpha !== overlay.alpha ||
			outcome.kappa !== overlay.kappa ||
			outcome.eta !== overlay.eta ||
			outcome.potential_decrease !== overlay.potential_decrease ||
			outcome.guaranteed_decrease !== overlay.guaranteed_decrease ||
			outcome.stationary !== overlay.stationary ||
			outcome.cycle.length !== Number(overlay.selected_edge_count) ||
			outcome.cycle.some((arc) => {
				const state = overlay.edges.find(
					(edge) => edge.edge_id === arc.edge_id,
				);
				return state === undefined || state.selected_sign !== arc.sign;
			})
		) {
			throw new Error("Minimum-ratio-cycle MCF completion lacks its outcome");
		}
	} else if (outcome !== undefined) {
		throw new Error(
			"Minimum-ratio-cycle MCF outcome appears before completion",
		);
	}
}

function validateRandomizedAlmostLinearMcfScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowRandomizedAlmostLinearMcfOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected =
		algorithmId === "randomized-almost-linear-mcf-oracle-demonstrator";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error(
				"Randomized almost-linear MCF overlay uses the wrong algorithm",
			);
		}
		return;
	}
	if (
		!(
			["fixed-flow-min-cost", "circulation", "transshipment"] as const
		).includes(
			model.kind as "fixed-flow-min-cost" | "circulation" | "transshipment",
		) ||
		Object.keys(config).length !== 0
	) {
		throw new Error(
			"Randomized almost-linear MCF requires an unconfigured MCF model",
		);
	}
	if (
		nodes.length < 1 ||
		nodes.length > 6 ||
		edges.length < 1 ||
		edges.length > 8 ||
		edges.some(
			(edge) =>
				edge.from === edge.to ||
				BigInt(edge.capacity) > 8n ||
				BigInt(edge.cost) < -32n ||
				BigInt(edge.cost) > 32n,
		) ||
		edges.reduce(
			(product, edge) =>
				product * (BigInt(edge.capacity) - BigInt(edge.lower) + 1n),
			1n,
		) > 100_000n
	) {
		throw new Error(
			"Randomized almost-linear MCF graph is outside its bounded domain",
		);
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined)
		) {
			return;
		}
		throw new Error(
			"Randomized almost-linear MCF scene is missing its typed overlay",
		);
	}
	const nodeOrder = canonicalNodeIds(nodes);
	const edgeOrder = canonicalStableIds(edges);
	const assignmentCheckpoint = overlay.stage === "inspect-feasible-assignment";
	const assignmentSerial =
		overlay.assignment_serial === undefined
			? undefined
			: BigInt(overlay.assignment_serial);
	const oracleCheckpoint = overlay.stage === "inspect-oracle-vector";
	const oracleVectorSerial =
		overlay.oracle_vector_serial === undefined
			? undefined
			: BigInt(overlay.oracle_vector_serial);
	if (
		overlay.nodes.length !== nodeOrder.length ||
		overlay.nodes.some((node, index) => node.node_id !== nodeOrder[index]) ||
		overlay.edges.length !== edgeOrder.length ||
		overlay.edges.some((edge, index) => edge.edge_id !== edgeOrder[index]) ||
		edgeStates.length !== edgeOrder.length ||
		edgeStates.some((edge, index) => edge.edge_id !== edgeOrder[index])
	) {
		throw new Error(
			"Randomized almost-linear MCF stable identities are inconsistent",
		);
	}
	if (
		assignmentCheckpoint !== (overlay.assignment_cursor !== undefined) ||
		assignmentCheckpoint !== (assignmentSerial !== undefined) ||
		(assignmentSerial !== undefined &&
			(assignmentSerial === 0n ||
				(assignmentSerial & (assignmentSerial - 1n)) !== 0n)) ||
		(assignmentCheckpoint && overlay.assignment_cursor !== edgeOrder.at(-1)) ||
		oracleCheckpoint !== (oracleVectorSerial !== undefined) ||
		(oracleVectorSerial !== undefined && oracleVectorSerial === 0n)
	) {
		throw new Error(
			"Randomized almost-linear MCF checkpoint identity is inconsistent",
		);
	}
	const graphEdgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const scale = BigInt(overlay.isolation_scale);
	const requiredByNode = projectLinearMcfRequiredDivergence(model, nodes);
	if (
		overlay.nodes.some(
			(node) =>
				BigInt(node.required_divergence) !== requiredByNode.get(node.node_id),
		) ||
		[...requiredByNode.values()].reduce((sum, value) => sum + value, 0n) !== 0n
	) {
		throw new Error(
			"Randomized almost-linear MCF required divergence is inconsistent",
		);
	}
	type ExactRational = { numerator: bigint; denominator: bigint };
	const exact = (value: FlowRationalV1): ExactRational => ({
		numerator: BigInt(value.numerator),
		denominator: BigInt(value.denominator),
	});
	const add = (left: ExactRational, right: ExactRational): ExactRational => ({
		numerator:
			left.numerator * right.denominator + right.numerator * left.denominator,
		denominator: left.denominator * right.denominator,
	});
	const subtract = (
		left: ExactRational,
		right: ExactRational,
	): ExactRational => ({
		numerator:
			left.numerator * right.denominator - right.numerator * left.denominator,
		denominator: left.denominator * right.denominator,
	});
	const multiplyInteger = (
		value: ExactRational,
		factor: bigint,
	): ExactRational => ({
		numerator: value.numerator * factor,
		denominator: value.denominator,
	});
	const equal = (left: ExactRational, right: ExactRational): boolean =>
		left.numerator * right.denominator === right.numerator * left.denominator;
	const lessThan = (left: ExactRational, right: ExactRational): boolean =>
		left.numerator * right.denominator < right.numerator * left.denominator;
	const zero: ExactRational = { numerator: 0n, denominator: 1n };
	const finalPointReady = [
		"construct-final-point",
		"round-nearest-integer",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const roundedReady = [
		"round-nearest-integer",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const maximumCapacity = edges.reduce(
		(maximum, edge) =>
			BigInt(edge.capacity) > maximum ? BigInt(edge.capacity) : maximum,
		1n,
	);
	const m = BigInt(edges.length);
	const expectedThreshold: ExactRational = {
		numerator: 1n,
		denominator:
			12n * m * m * m * maximumCapacity * maximumCapacity * maximumCapacity,
	};
	const threshold = exact(overlay.final_point_threshold);
	const expectedScale = 4n * m * m * maximumCapacity * maximumCapacity;
	const gap =
		overlay.final_point_gap === undefined
			? undefined
			: exact(overlay.final_point_gap);
	const mix =
		overlay.final_point_mix === undefined
			? undefined
			: exact(overlay.final_point_mix);
	if (
		scale !== expectedScale ||
		!equal(threshold, expectedThreshold) ||
		finalPointReady !== (gap !== undefined) ||
		finalPointReady !== (mix !== undefined) ||
		overlay.exact_recovery !== roundedReady ||
		(gap !== undefined && (lessThan(gap, zero) || lessThan(threshold, gap))) ||
		(mix !== undefined &&
			(!lessThan(zero, mix) ||
				lessThan({ numerator: 1n, denominator: 4n }, mix)))
	) {
		throw new Error(
			"Randomized almost-linear MCF final-point header is inconsistent",
		);
	}
	const isolationVisible = ![
		"ready",
		"inspect-feasible-assignment",
		"enumerate-feasible-set",
	].includes(overlay.stage);
	const isolatedOptimumVisible = ![
		"ready",
		"inspect-feasible-assignment",
		"enumerate-feasible-set",
		"sample-isolation-costs",
	].includes(overlay.stage);
	const pointDivergence = new Map(nodes.map((node) => [node.id, { ...zero }]));
	let scaledPointCost = { ...zero };
	const isolatedDivergence = new Map(nodes.map((node) => [node.id, 0n]));
	let isolatedOriginalCost = 0n;
	let isolatedPerturbedCost = 0n;
	for (const state of overlay.edges) {
		const edge = graphEdgeById.get(state.edge_id);
		if (edge === undefined) {
			throw new Error("Randomized almost-linear MCF edge is missing");
		}
		const lower = Number(edge.lower);
		const capacity = Number(edge.capacity);
		const initial = Number(state.initial_flow);
		const current = Number(state.current_flow);
		const stale = Number(state.stale_flow);
		const point =
			state.final_point_flow === undefined
				? undefined
				: exact(state.final_point_flow);
		const finalFlow =
			state.final_flow === undefined ? undefined : BigInt(state.final_flow);
		const isolatedOptimumFlow =
			state.isolated_optimum_flow === undefined
				? undefined
				: BigInt(state.isolated_optimum_flow);
		if (
			initial < lower ||
			initial > capacity ||
			current < lower - 1e-9 ||
			current > capacity + 1e-9 ||
			stale < lower - 1e-9 ||
			stale > capacity + 1e-9 ||
			finalPointReady !== (point !== undefined) ||
			roundedReady !== (finalFlow !== undefined) ||
			(finalFlow !== undefined && finalFlow > BigInt(edge.capacity)) ||
			(overlay.stage !== "inspect-oracle-vector" &&
				state.candidate_sign !== "0") ||
			Number(state.length) < 0 ||
			(isolationVisible &&
				BigInt(state.isolated_cost) !==
					scale * BigInt(edge.cost) + BigInt(state.isolation_draw)) ||
			isolatedOptimumVisible !== (isolatedOptimumFlow !== undefined) ||
			(isolatedOptimumFlow !== undefined &&
				isolatedOptimumFlow > BigInt(edge.capacity))
		) {
			throw new Error(
				"Randomized almost-linear MCF edge state is inconsistent",
			);
		}
		if (isolatedOptimumFlow !== undefined) {
			isolatedDivergence.set(
				edge.from,
				(isolatedDivergence.get(edge.from) ?? 0n) + isolatedOptimumFlow,
			);
			isolatedDivergence.set(
				edge.to,
				(isolatedDivergence.get(edge.to) ?? 0n) - isolatedOptimumFlow,
			);
			isolatedOriginalCost += BigInt(edge.cost) * isolatedOptimumFlow;
			isolatedPerturbedCost +=
				BigInt(state.isolated_cost) * isolatedOptimumFlow;
		}
		if (point !== undefined) {
			if (
				point.numerator < BigInt(edge.lower) * point.denominator ||
				point.numerator > BigInt(edge.capacity) * point.denominator
			) {
				throw new Error(
					"Randomized almost-linear MCF final point violates a bound",
				);
			}
			const quotient = point.numerator / point.denominator;
			const remainder = point.numerator % point.denominator;
			const nearest =
				remainder * 2n >= point.denominator ? quotient + 1n : quotient;
			const distanceNumerator =
				point.numerator >= nearest * point.denominator
					? point.numerator - nearest * point.denominator
					: nearest * point.denominator - point.numerator;
			if (
				distanceNumerator * 4n >= point.denominator ||
				(finalFlow !== undefined && finalFlow !== nearest)
			) {
				throw new Error(
					"Randomized almost-linear MCF nearest-integer recovery is inconsistent",
				);
			}
			pointDivergence.set(
				edge.from,
				add(pointDivergence.get(edge.from) as ExactRational, point),
			);
			pointDivergence.set(
				edge.to,
				subtract(pointDivergence.get(edge.to) as ExactRational, point),
			);
			scaledPointCost = add(
				scaledPointCost,
				multiplyInteger(point, BigInt(state.isolated_cost)),
			);
		}
	}
	if (
		isolatedOptimumVisible &&
		(isolatedOriginalCost !== BigInt(overlay.optimum_cost) ||
			isolatedPerturbedCost !== BigInt(overlay.isolated_optimum_cost) ||
			overlay.nodes.some(
				(node) =>
					isolatedDivergence.get(node.node_id) !==
					BigInt(node.required_divergence),
			))
	) {
		throw new Error(
			"Randomized almost-linear MCF isolated optimum is inconsistent",
		);
	}
	if (finalPointReady) {
		const measuredGap = subtract(scaledPointCost, {
			numerator: BigInt(overlay.isolated_optimum_cost),
			denominator: 1n,
		});
		measuredGap.denominator *= scale;
		if (
			gap === undefined ||
			!equal(gap, measuredGap) ||
			overlay.nodes.some(
				(node) =>
					!equal(pointDivergence.get(node.node_id) as ExactRational, {
						numerator: BigInt(node.required_divergence),
						denominator: 1n,
					}),
			)
		) {
			throw new Error(
				"Randomized almost-linear MCF final point is infeasible or inaccurate",
			);
		}
	}
	if (roundedReady) {
		for (let index = 0; index < edgeStates.length; index += 1) {
			if (
				(edgeStates[index] as FlowEdgeStateV1).flow !==
				(overlay.edges[index] as FlowRandomizedAlmostLinearMcfEdgeStateV1)
					.final_flow
			) {
				throw new Error(
					"Randomized almost-linear MCF rounded flow projection is inconsistent",
				);
			}
		}
	}
	if (BigInt(overlay.failure_denominator) === 0n) {
		throw new Error("Randomized almost-linear MCF failure bound is invalid");
	}
	validateTraceStageIdentity(
		traceEvent,
		"randomized-almost-linear-mcf-oracle-demonstrator",
		overlay.stage,
		"Randomized almost-linear MCF",
	);
	if (overlay.stage === "optimal") {
		const recomputedCost = overlay.edges.reduce((sum, state) => {
			const edge = graphEdgeById.get(state.edge_id) as FlowEdgeV1;
			return sum + BigInt(edge.cost) * BigInt(state.final_flow as string);
		}, 0n);
		if (
			solveStatus !== "optimal" ||
			outcome?.kind !== "min-cost-flow" ||
			BigInt(outcome.total_cost) !== recomputedCost ||
			recomputedCost !== BigInt(overlay.optimum_cost)
		) {
			throw new Error(
				"Randomized almost-linear MCF completion lacks its certificate",
			);
		}
	} else if (outcome !== undefined) {
		throw new Error(
			"Randomized almost-linear MCF outcome appears before completion",
		);
	} else if (solveStatus !== "ready" && solveStatus !== "running") {
		throw new Error(
			"Randomized almost-linear MCF stage and solve status disagree",
		);
	}
}

function validateFlowFrameworkMcfScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowFrameworkMcfOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "deterministic-almost-linear-mcf";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Flow Framework MCF overlay uses the wrong algorithm");
		}
		return;
	}
	if (
		!(
			["fixed-flow-min-cost", "circulation", "transshipment"] as const
		).includes(
			model.kind as "fixed-flow-min-cost" | "circulation" | "transshipment",
		) ||
		Object.keys(config).length !== 0
	) {
		throw new Error("Flow Framework MCF requires an unconfigured MCF model");
	}
	if (
		nodes.length < 2 ||
		nodes.length > 6 ||
		edges.length < 1 ||
		edges.length > 8 ||
		edges.some((edge) => {
			const lower = BigInt(edge.lower);
			const capacity = BigInt(edge.capacity);
			const cost = BigInt(edge.cost);
			return (
				edge.from === edge.to ||
				lower >= capacity ||
				capacity > 8n ||
				cost < -32n ||
				cost > 32n
			);
		}) ||
		edges.reduce(
			(product, edge) =>
				product * (BigInt(edge.capacity) - BigInt(edge.lower) + 1n),
			1n,
		) > 100_000n
	) {
		throw new Error("Flow Framework MCF graph is outside its bounded domain");
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined)
		) {
			return;
		}
		throw new Error("Flow Framework MCF scene is missing its typed overlay");
	}
	const dynamicOperationValid = (() => {
		switch (overlay.stage) {
			case "initialize-source-point":
			case "round-fractional-flow":
			case "check-certificate":
			case "optimal":
				return overlay.dynamic_operation === undefined;
			case "periodic-reinitialize":
				return (
					overlay.dynamic_operation === undefined ||
					overlay.dynamic_operation === "topology-stage-applied" ||
					overlay.dynamic_operation === "periodic-rebuilt"
				);
			case "detect":
				return overlay.dynamic_operation === "detect-returned";
			case "query-minimum-ratio-cycle":
				return (
					overlay.dynamic_operation === "cycle-queried-accepted" ||
					overlay.dynamic_operation === "cycle-queried-rejected" ||
					overlay.dynamic_operation === "level-shifted" ||
					overlay.dynamic_operation === "query-returned"
				);
			case "source-progress":
				return (
					overlay.dynamic_operation === "flow-applied" ||
					overlay.dynamic_operation === "completed"
				);
		}
	})();
	const dynamicOperationSerial =
		overlay.dynamic_operation_serial === undefined
			? undefined
			: BigInt(overlay.dynamic_operation_serial);
	if (
		!dynamicOperationValid ||
		(overlay.dynamic_operation === undefined) !==
			(dynamicOperationSerial === undefined) ||
		dynamicOperationSerial === 0n
	) {
		throw new Error(
			"Flow Framework MCF dynamic operation identity is inconsistent",
		);
	}
	const iteration = BigInt(overlay.iteration);
	const initial = overlay.stage === "initialize-source-point";
	const progressVisible =
		iteration > 0n &&
		[
			"query-minimum-ratio-cycle",
			"source-progress",
			"round-fractional-flow",
			"check-certificate",
			"optimal",
		].includes(overlay.stage);
	const zero: FlowRationalV1 = { numerator: "0", denominator: "1" };
	const half: FlowRationalV1 = { numerator: "1", denominator: "2" };
	const finalPointVisible = [
		"round-fractional-flow",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	if (
		Number(overlay.gap_before) < 0 ||
		Number(overlay.gap_after) < 0 ||
		compareRational(overlay.exact_gap_before, zero) < 0 ||
		compareRational(overlay.exact_gap_after, zero) < 0 ||
		compareRational(overlay.stopping_gap, half) !== 0 ||
		(iteration === 0n
			? compareRational(overlay.exact_gap_before, overlay.exact_gap_after) !== 0
			: compareRational(overlay.exact_gap_after, overlay.exact_gap_before) >=
				0) ||
		(finalPointVisible &&
			compareRational(overlay.exact_gap_after, overlay.stopping_gap) > 0) ||
		initial !== (iteration === 0n) ||
		overlay.reinitialized !== iteration > 1n ||
		(progressVisible
			? compareRational(overlay.accepted_ratio, zero) <= 0 ||
				compareRational(overlay.target_progress, zero) <= 0
			: compareRational(overlay.accepted_ratio, zero) !== 0 ||
				compareRational(overlay.target_progress, zero) !== 0) ||
		(overlay.stage === "optimal") !==
			(overlay.termination === "source-additive-half-gap") ||
		(overlay.stage !== "optimal" && overlay.termination !== undefined)
	) {
		throw new Error(
			"Flow Framework MCF scalar or stopping state is inconsistent",
		);
	}
	if (
		overlay.levels.length !== 2 ||
		overlay.levels.some(
			(level, index) =>
				BigInt(level.level) !== BigInt(index) ||
				BigInt(level.active_branch) >= 2n,
		)
	) {
		throw new Error("Flow Framework MCF level state is inconsistent");
	}
	const edgeOrder = canonicalStableIds(edges);
	if (
		overlay.edges.length !== edgeOrder.length ||
		overlay.edges.some((edge, index) => edge.edge_id !== edgeOrder[index]) ||
		edgeStates.length !== edgeOrder.length ||
		edgeStates.some((edge, index) => edge.edge_id !== edgeOrder[index])
	) {
		throw new Error(
			"Flow Framework MCF stable edge identities are inconsistent",
		);
	}
	const graphEdgeById = new Map(edges.map((edge) => [edge.id, edge]));
	type Fraction = { numerator: bigint; denominator: bigint };
	const divergence = new Map(
		nodes.map((node) => [node.id, { numerator: 0n, denominator: 1n }]),
	);
	const add = (
		left: Fraction,
		right: FlowRationalV1,
		sign: bigint,
	): Fraction => ({
		numerator:
			left.numerator * BigInt(right.denominator) +
			sign * BigInt(right.numerator) * left.denominator,
		denominator: left.denominator * BigInt(right.denominator),
	});
	const finalPointPresent =
		overlay.optimum_cost !== undefined &&
		overlay.final_point_nodes !== undefined &&
		overlay.final_point_edges !== undefined;
	const roundedVisible = ["check-certificate", "optimal"].includes(
		overlay.stage,
	);
	if (finalPointVisible !== finalPointPresent) {
		throw new Error(
			"Flow Framework MCF final-point proof appears at the wrong stage",
		);
	}
	if (finalPointPresent) {
		const optimumCost = BigInt(overlay.optimum_cost as string);
		const finalNodes =
			overlay.final_point_nodes as FlowFrameworkMcfFinalPointNodeV1[];
		const finalEdges =
			overlay.final_point_edges as FlowFrameworkMcfFinalPointEdgeV1[];
		if (
			finalNodes.length === 0 ||
			finalEdges.length === 0 ||
			finalEdges.length > 12
		) {
			throw new Error(
				"Flow Framework MCF final-point proof is empty or oversized",
			);
		}
		const expectedRequired = projectLinearMcfRequiredDivergence(model, nodes);
		const required = new Map<string, bigint>();
		const pointDivergence = new Map<string, Fraction>();
		const roundedDivergence = new Map<string, bigint>();
		for (const node of finalNodes) {
			if (required.has(node.node_id)) {
				throw new Error("Flow Framework MCF final-point node is duplicated");
			}
			required.set(node.node_id, BigInt(node.required_divergence));
			pointDivergence.set(node.node_id, {
				numerator: 0n,
				denominator: 1n,
			});
			roundedDivergence.set(node.node_id, 0n);
		}
		for (const node of nodes) {
			if (required.get(node.id) !== expectedRequired.get(node.id)) {
				throw new Error(
					"Flow Framework MCF final-point divergence anchor is inconsistent",
				);
			}
		}
		const augmentedIds = new Set<string>();
		const originalIds = new Set<string>();
		let exactCost: Fraction = { numerator: 0n, denominator: 1n };
		let roundedCost = 0n;
		for (const state of finalEdges) {
			const lower = BigInt(state.lower);
			const capacity = BigInt(state.capacity);
			const cost = BigInt(state.cost);
			if (
				augmentedIds.has(state.edge_id) ||
				lower > capacity ||
				!required.has(state.from) ||
				!required.has(state.to) ||
				compareRational(state.flow, {
					numerator: state.lower,
					denominator: "1",
				}) < 0 ||
				compareRational(state.flow, {
					numerator: state.capacity,
					denominator: "1",
				}) > 0 ||
				(state.rounded_flow !== undefined) !== roundedVisible
			) {
				throw new Error("Flow Framework MCF final-point edge is inconsistent");
			}
			augmentedIds.add(state.edge_id);
			pointDivergence.set(
				state.from,
				add(pointDivergence.get(state.from) as Fraction, state.flow, 1n),
			);
			pointDivergence.set(
				state.to,
				add(pointDivergence.get(state.to) as Fraction, state.flow, -1n),
			);
			exactCost = {
				numerator:
					exactCost.numerator * BigInt(state.flow.denominator) +
					cost * BigInt(state.flow.numerator) * exactCost.denominator,
				denominator: exactCost.denominator * BigInt(state.flow.denominator),
			};
			if (state.rounded_flow !== undefined) {
				const rounded = BigInt(state.rounded_flow);
				if (
					rounded < lower ||
					rounded > capacity ||
					(state.auxiliary && rounded !== 0n)
				) {
					throw new Error(
						"Flow Framework MCF augmented rounding is inconsistent",
					);
				}
				roundedDivergence.set(
					state.from,
					(roundedDivergence.get(state.from) as bigint) + rounded,
				);
				roundedDivergence.set(
					state.to,
					(roundedDivergence.get(state.to) as bigint) - rounded,
				);
				roundedCost += rounded * cost;
			}
			if (!state.auxiliary) {
				const original = graphEdgeById.get(state.edge_id);
				const visible = overlay.edges.find(
					(edge) => edge.edge_id === state.edge_id,
				);
				if (
					original === undefined ||
					visible === undefined ||
					originalIds.has(state.edge_id) ||
					state.from !== original.from ||
					state.to !== original.to ||
					state.lower !== original.lower ||
					state.capacity !== original.capacity ||
					state.cost !== original.cost ||
					(overlay.stage === "optimal"
						? state.rounded_flow !== visible.flow.numerator ||
							visible.flow.denominator !== "1"
						: compareRational(state.flow, visible.flow) !== 0)
				) {
					throw new Error(
						"Flow Framework MCF original final-point projection is inconsistent",
					);
				}
				originalIds.add(state.edge_id);
			}
		}
		const exactGapNumerator =
			exactCost.numerator - optimumCost * exactCost.denominator;
		if (
			originalIds.size !== edges.length ||
			exactGapNumerator * BigInt(overlay.exact_gap_after.denominator) !==
				BigInt(overlay.exact_gap_after.numerator) * exactCost.denominator ||
			[...pointDivergence].some(([node, value]) => {
				const expected = required.get(node) as bigint;
				return value.numerator !== expected * value.denominator;
			}) ||
			(roundedVisible &&
				(roundedCost !== optimumCost ||
					roundedCost * exactCost.denominator > exactCost.numerator ||
					[...roundedDivergence].some(
						([node, value]) => value !== required.get(node),
					)))
		) {
			throw new Error(
				"Flow Framework MCF exact final-point or rounding proof is inconsistent",
			);
		}
	}
	for (const state of overlay.edges) {
		const edge = graphEdgeById.get(state.edge_id);
		if (edge === undefined) {
			throw new Error("Flow Framework MCF edge is missing");
		}
		const lower: FlowRationalV1 = {
			numerator: edge.lower,
			denominator: "1",
		};
		const capacity: FlowRationalV1 = {
			numerator: edge.capacity,
			denominator: "1",
		};
		if (
			state.selected !== (state.cycle_coefficient.numerator !== "0") ||
			compareRational(state.flow, lower) < 0 ||
			compareRational(state.flow, capacity) > 0
		) {
			throw new Error("Flow Framework MCF edge state is inconsistent");
		}
		divergence.set(
			edge.from,
			add(divergence.get(edge.from) as Fraction, state.cycle_coefficient, 1n),
		);
		divergence.set(
			edge.to,
			add(divergence.get(edge.to) as Fraction, state.cycle_coefficient, -1n),
		);
		const generic = edgeStates.find(
			(candidate) => candidate.edge_id === edge.id,
		);
		if (
			generic === undefined ||
			(overlay.stage === "optimal"
				? compareRational(state.flow, {
						numerator: generic.flow,
						denominator: "1",
					}) !== 0
				: generic.flow !== edge.lower)
		) {
			throw new Error(
				"Flow Framework MCF generic flow projection is inconsistent",
			);
		}
	}
	if ([...divergence.values()].some((value) => value.numerator !== 0n)) {
		throw new Error("Flow Framework MCF cycle is not a circulation");
	}
	validateTraceStageIdentity(
		traceEvent,
		"deterministic-almost-linear-mcf",
		overlay.stage,
		"Flow Framework MCF",
	);
	if (overlay.stage === "optimal") {
		const recomputedCost = overlay.edges.reduce((sum, state) => {
			const edge = graphEdgeById.get(state.edge_id) as FlowEdgeV1;
			return sum + BigInt(edge.cost) * BigInt(state.flow.numerator);
		}, 0n);
		if (
			solveStatus !== "optimal" ||
			outcome?.kind !== "min-cost-flow" ||
			BigInt(outcome.total_cost) !== recomputedCost
		) {
			throw new Error("Flow Framework MCF completion lacks its certificate");
		}
	} else if (outcome !== undefined) {
		throw new Error("Flow Framework MCF outcome appears before certification");
	} else if (solveStatus !== "running") {
		throw new Error("Flow Framework MCF stage and solve status disagree");
	}
}

function validateWeightedAugmentingPathsScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowWeightedAugmentingPathsOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "weighted-augmenting-paths";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error(
				"Weighted augmenting-path overlay uses the wrong algorithm",
			);
		}
		return;
	}
	if (model.kind !== "max-flow" || Object.keys(config).length !== 0) {
		throw new Error(
			"Weighted augmenting paths require an unconfigured max-flow model",
		);
	}
	if (
		nodes.length < 2 ||
		nodes.length > 8 ||
		edges.length < 1 ||
		edges.length > 12 ||
		model.source === model.sink ||
		nodes.every((node) => node.id !== model.source) ||
		nodes.every((node) => node.id !== model.sink) ||
		nodes.some((node) => node.supply !== "0") ||
		edges.some(
			(edge) =>
				edge.lower !== "0" ||
				BigInt(edge.capacity) <= 0n ||
				BigInt(edge.capacity) > 64n ||
				edge.from === edge.to,
		)
	) {
		throw new Error(
			"Weighted augmenting-path graph is outside its bounded domain",
		);
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined)
		) {
			return;
		}
		throw new Error(
			"Weighted augmenting-path scene is missing its source overlay",
		);
	}
	const nodeOrder = canonicalNodeIds(nodes);
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const orderedEdges = canonicalStableIds(edges).map(
		(id) => edgeById.get(id) as FlowEdgeV1,
	);
	if (
		overlay.nodes.length !== nodeOrder.length ||
		overlay.nodes.some((state, index) => state.node_id !== nodeOrder[index]) ||
		overlay.edges.length !== orderedEdges.length ||
		overlay.edges.some(
			(state, index) => state.edge_id !== orderedEdges[index]?.id,
		) ||
		overlay.residual_arcs.length !== orderedEdges.length * 2 ||
		edgeStates.length !== orderedEdges.length
	) {
		throw new Error(
			"Weighted augmenting-path stable identities are inconsistent",
		);
	}
	const phase = BigInt(overlay.phase);
	const phaseCount = BigInt(overlay.phase_count);
	const bit = BigInt(overlay.capacity_bit);
	const height = BigInt(overlay.height);
	const phiNumerator = BigInt(overlay.phi_numerator);
	const phiDenominator = BigInt(overlay.phi_denominator);
	const bottleneck = BigInt(overlay.active_bottleneck);
	if (
		phaseCount <= 0n ||
		phase < 0n ||
		phase >= phaseCount ||
		bit < 0n ||
		bit >= phaseCount ||
		phiDenominator <= 0n ||
		(bottleneck === 0n) !== (overlay.active_path.length === 0) ||
		(overlay.stage === "augment-path") !== bottleneck > 0n
	) {
		throw new Error(
			"Weighted augmenting-path phase or active path is malformed",
		);
	}
	if (overlay.stage === "ready") {
		const readyEdges = overlay.edges.every(
			(state, index) =>
				state.scaled_capacity === "0" &&
				state.flow === "0" &&
				edgeStates[index]?.flow === "0",
		);
		const readyNodes = overlay.nodes.every(
			(state) =>
				state.component === "0" &&
				state.order === "0" &&
				state.label === "0" &&
				state.alive &&
				!state.expansion_witness_side &&
				!state.source_side,
		);
		const readyResidual = overlay.residual_arcs.every((arc, index) => {
			const edge = orderedEdges[Math.floor(index / 2)];
			const forward = index % 2 === 0;
			return (
				edge !== undefined &&
				arc.edge_id === edge.id &&
				arc.direction === (forward ? "forward" : "reverse") &&
				arc.from === (forward ? edge.from : edge.to) &&
				arc.to === (forward ? edge.to : edge.from) &&
				arc.capacity === "0" &&
				arc.hierarchy_kind === undefined &&
				arc.weight === "0" &&
				!arc.admissible &&
				!arc.active
			);
		});
		if (
			phase !== 0n ||
			bit !== phaseCount - 1n ||
			height !== 0n ||
			phiNumerator !== 0n ||
			phiDenominator !== 1n ||
			bottleneck !== 0n ||
			overlay.active_path.length !== 0 ||
			!readyEdges ||
			!readyNodes ||
			!readyResidual ||
			eventId !== "0" ||
			(solveStatus !== "ready" && solveStatus !== "resource-limit") ||
			traceEvent !== undefined ||
			outcome !== undefined
		) {
			throw new Error(
				"Weighted augmenting-path ready snapshot is inconsistent",
			);
		}
		return;
	}
	const hierarchyVisible = overlay.residual_arcs.some(
		(arc) => arc.hierarchy_kind !== undefined,
	);
	const requiresHierarchy = [
		"build-hierarchy",
		"certify-expansion",
		"assign-weights",
		"relabel-sweep",
		"augment-path",
		"finish-weighted-round",
	].includes(overlay.stage);
	const requiresHeight = [
		"assign-weights",
		"relabel-sweep",
		"augment-path",
		"finish-weighted-round",
	].includes(overlay.stage);
	const terminalBoundary = [
		"finish-capacity-phase",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const heightVisible = height > 0n;
	if (
		hierarchyVisible !== phiNumerator > 0n ||
		(requiresHierarchy && !hierarchyVisible) ||
		(requiresHeight && !heightVisible) ||
		(!requiresHeight && !terminalBoundary && heightVisible) ||
		(terminalBoundary && hierarchyVisible !== heightVisible)
	) {
		throw new Error(
			"Weighted augmenting-path hierarchy publication is inconsistent",
		);
	}
	const nodeState = new Map(
		overlay.nodes.map((state) => [state.node_id, state]),
	);
	const seenOrders = new Set<string>();
	for (const state of overlay.nodes) {
		const component = BigInt(state.component);
		const order = BigInt(state.order);
		const label = BigInt(state.label);
		if (
			(hierarchyVisible &&
				(component < 0n ||
					component >= BigInt(nodes.length) ||
					order <= 0n ||
					order > BigInt(nodes.length) ||
					seenOrders.has(state.order))) ||
			(!hierarchyVisible && (component !== 0n || order !== 0n)) ||
			(!state.alive && label <= 9n * height) ||
			(state.alive && height > 0n && label > 9n * height)
		) {
			throw new Error("Weighted augmenting-path node state is inconsistent");
		}
		if (hierarchyVisible) seenOrders.add(state.order);
	}
	const genericFlow = new Map(
		edgeStates.map((state) => [state.edge_id, state.flow]),
	);
	const activeKeys = overlay.active_path.map(
		(reference) => `${reference.edge_id}\u0000${reference.direction}`,
	);
	if (new Set(activeKeys).size !== activeKeys.length) {
		throw new Error(
			"Weighted augmenting-path path repeats a residual direction",
		);
	}
	const residualByKey = new Map<
		string,
		FlowWeightedAugmentingResidualArcStateV1
	>();
	for (let index = 0; index < orderedEdges.length; index += 1) {
		const edge = orderedEdges[index] as FlowEdgeV1;
		const state = overlay.edges[index] as FlowWeightedAugmentingEdgeStateV1;
		const scaled = BigInt(state.scaled_capacity);
		const flow = BigInt(state.flow);
		const shift = phaseCount - phase - 1n;
		const expectedScaled = BigInt(edge.capacity) >> shift;
		if (
			scaled !== expectedScaled ||
			flow < 0n ||
			flow > scaled ||
			genericFlow.get(edge.id) !== state.flow
		) {
			throw new Error("Weighted augmenting-path prefix edge is inconsistent");
		}
		for (let directionIndex = 0; directionIndex < 2; directionIndex += 1) {
			const arc = overlay.residual_arcs[
				index * 2 + directionIndex
			] as FlowWeightedAugmentingResidualArcStateV1;
			const forward = directionIndex === 0;
			const direction = forward ? "forward" : "reverse";
			const expectedFrom = forward ? edge.from : edge.to;
			const expectedTo = forward ? edge.to : edge.from;
			const capacity = forward ? scaled - flow : flow;
			const weight = BigInt(arc.weight);
			const fromState = nodeState.get(
				expectedFrom,
			) as FlowWeightedAugmentingNodeStateV1;
			const toState = nodeState.get(
				expectedTo,
			) as FlowWeightedAugmentingNodeStateV1;
			const active = activeKeys.includes(`${edge.id}\u0000${direction}`);
			if (
				arc.edge_id !== edge.id ||
				arc.direction !== direction ||
				arc.from !== expectedFrom ||
				arc.to !== expectedTo ||
				BigInt(arc.capacity) !== capacity ||
				arc.active !== active ||
				(arc.admissible && capacity === 0n) ||
				hierarchyVisible !== (arc.hierarchy_kind !== undefined) ||
				hierarchyVisible !== weight > 0n
			) {
				throw new Error(
					"Weighted augmenting-path residual projection is inconsistent",
				);
			}
			if (hierarchyVisible) {
				const expectedWeight =
					BigInt(fromState.order) >= BigInt(toState.order)
						? BigInt(fromState.order) - BigInt(toState.order)
						: BigInt(toState.order) - BigInt(fromState.order);
				const sameComponent = fromState.component === toState.component;
				if (
					weight !== expectedWeight ||
					weight <= 0n ||
					(arc.hierarchy_kind === "expanding") !== sameComponent ||
					(arc.hierarchy_kind === "dag" &&
						arc.direction === "forward" &&
						capacity > 0n &&
						BigInt(fromState.order) >= BigInt(toState.order))
				) {
					throw new Error(
						"Weighted augmenting-path hierarchy weight is inconsistent",
					);
				}
			}
			residualByKey.set(`${edge.id}\u0000${direction}`, arc);
		}
	}
	const positiveAdjacency = new Map(
		nodeOrder.map((node) => [node, [] as string[]]),
	);
	for (const arc of overlay.residual_arcs) {
		if (BigInt(arc.capacity) > 0n) {
			positiveAdjacency.get(arc.from)?.push(arc.to);
		}
	}
	const reachable = new Set<string>([model.source]);
	const queue = [model.source];
	while (queue.length > 0) {
		const node = queue.shift() as string;
		for (const next of positiveAdjacency.get(node) ?? []) {
			if (!reachable.has(next)) {
				reachable.add(next);
				queue.push(next);
			}
		}
	}
	if (
		overlay.nodes.some(
			(state) => state.source_side !== reachable.has(state.node_id),
		)
	) {
		throw new Error(
			"Weighted augmenting-path source-side projection is inconsistent",
		);
	}
	const divergence = new Map(nodeOrder.map((node) => [node, 0n]));
	for (let index = 0; index < orderedEdges.length; index += 1) {
		const edge = orderedEdges[index] as FlowEdgeV1;
		const flow = BigInt(
			(overlay.edges[index] as FlowWeightedAugmentingEdgeStateV1).flow,
		);
		divergence.set(edge.from, (divergence.get(edge.from) ?? 0n) + flow);
		divergence.set(edge.to, (divergence.get(edge.to) ?? 0n) - flow);
	}
	const value = divergence.get(model.source) ?? 0n;
	if (
		value < 0n ||
		divergence.get(model.sink) !== -value ||
		[...divergence].some(
			([node, balance]) =>
				node !== model.source && node !== model.sink && balance !== 0n,
		)
	) {
		throw new Error(
			"Weighted augmenting-path prefix flow violates conservation",
		);
	}
	let pathNode = model.source;
	for (const key of activeKeys) {
		const arc = residualByKey.get(key);
		const edgeOrdinal =
			arc === undefined
				? -1
				: orderedEdges.findIndex((edge) => edge.id === arc.edge_id);
		const edgeState =
			edgeOrdinal < 0
				? undefined
				: (overlay.edges[edgeOrdinal] as
						| FlowWeightedAugmentingEdgeStateV1
						| undefined);
		const fromState = arc === undefined ? undefined : nodeState.get(arc.from);
		const toState = arc === undefined ? undefined : nodeState.get(arc.to);
		const capacity = arc === undefined ? 0n : BigInt(arc.capacity);
		const weight = arc === undefined ? 0n : BigInt(arc.weight);
		const flow = edgeState === undefined ? 0n : BigInt(edgeState.flow);
		const scaled =
			edgeState === undefined ? 0n : BigInt(edgeState.scaled_capacity);
		const flowBefore =
			arc?.direction === "forward" ? flow - bottleneck : flow + bottleneck;
		if (
			arc === undefined ||
			edgeState === undefined ||
			fromState === undefined ||
			toState === undefined ||
			arc.from !== pathNode ||
			!arc.active ||
			arc.admissible !== capacity > 0n ||
			BigInt(fromState.label) - BigInt(toState.label) < 2n * weight ||
			flowBefore < 0n ||
			flowBefore > scaled
		) {
			throw new Error("Weighted augmenting-path active path is invalid");
		}
		pathNode = arc.to;
	}
	if (activeKeys.length > 0 && pathNode !== model.sink) {
		throw new Error("Weighted augmenting path does not connect source to sink");
	}
	if (
		overlay.stage === "certify-expansion" ||
		overlay.stage === "assign-weights"
	) {
		type Ratio = { numerator: bigint; denominator: bigint };
		let exact: Ratio | undefined;
		for (const component of new Set(
			overlay.nodes.map((state) => state.component),
		)) {
			const members = overlay.nodes
				.filter((state) => state.component === component)
				.map((state) => state.node_id);
			if (members.length < 2) continue;
			for (let mask = 1; mask < 2 ** members.length - 1; mask += 1) {
				const side = new Set(
					members.filter((_, index) => (mask & (1 << index)) !== 0),
				);
				let outgoing = 0n;
				let incoming = 0n;
				let sideVolume = 0n;
				let otherVolume = 0n;
				for (const arc of overlay.residual_arcs) {
					if (
						arc.hierarchy_kind !== "expanding" ||
						(nodeState.get(arc.from) as FlowWeightedAugmentingNodeStateV1)
							.component !== component
					)
						continue;
					const capacity = BigInt(arc.capacity);
					const fromSide = side.has(arc.from);
					const toSide = side.has(arc.to);
					if (fromSide && !toSide) outgoing += capacity;
					if (!fromSide && toSide) incoming += capacity;
					if (fromSide) sideVolume += capacity;
					else otherVolume += capacity;
					if (toSide) sideVolume += capacity;
					else otherVolume += capacity;
				}
				const denominator = sideVolume < otherVolume ? sideVolume : otherVolume;
				if (denominator === 0n) continue;
				const numerator = outgoing < incoming ? outgoing : incoming;
				if (
					exact === undefined ||
					numerator * exact.denominator < exact.numerator * denominator
				)
					exact = { numerator, denominator };
			}
		}
		const expected = exact ?? { numerator: 1n, denominator: 1n };
		if (
			phiNumerator * expected.denominator !==
			expected.numerator * phiDenominator
		) {
			throw new Error(
				"Weighted augmenting-path phi certificate is inconsistent",
			);
		}
	}
	if (
		["finish-capacity-phase", "check-certificate", "optimal"].includes(
			overlay.stage,
		) &&
		reachable.has(model.sink)
	) {
		throw new Error(
			"Weighted augmenting-path phase retains an augmenting path",
		);
	}
	const expectedStatus: Record<
		FlowWeightedAugmentingPathsOverlayV1["stage"],
		FlowCurrentSceneV9["solve_status"]
	> = {
		ready: "ready",
		"begin-capacity-phase": "running",
		"build-hierarchy": "running",
		"certify-expansion": "running",
		"assign-weights": "running",
		"relabel-sweep": "running",
		"augment-path": "running",
		"finish-weighted-round": "running",
		"finish-capacity-phase": "running",
		"check-certificate": "running",
		optimal: "optimal",
	};
	if (
		solveStatus !== "resource-limit" &&
		solveStatus !== expectedStatus[overlay.stage]
	) {
		throw new Error("Weighted augmenting-path stage and solve status disagree");
	}
	validateTraceStageIdentity(
		traceEvent,
		"weighted-augmenting-paths",
		overlay.stage,
		"Weighted augmenting-path",
	);
	if (overlay.stage === "optimal") {
		const sourceSide = overlay.nodes
			.filter((node) => node.source_side)
			.map((node) => node.node_id);
		const sourceSet = new Set(sourceSide);
		const cut = orderedEdges.reduce(
			(sum, edge) =>
				sourceSet.has(edge.from) && !sourceSet.has(edge.to)
					? sum + BigInt(edge.capacity)
					: sum,
			0n,
		);
		if (
			phase !== phaseCount - 1n ||
			overlay.edges.some(
				(state, index) =>
					state.scaled_capacity !== orderedEdges[index]?.capacity,
			) ||
			outcome?.kind !== "max-flow" ||
			BigInt(outcome.value) !== value ||
			BigInt(outcome.cut_bound) !== cut ||
			cut !== value ||
			outcome.source_side.length !== sourceSide.length ||
			outcome.source_side.some((node, index) => node !== sourceSide[index])
		) {
			throw new Error(
				"Weighted augmenting-path optimum lacks its exact certificate",
			);
		}
	} else if (outcome !== undefined) {
		throw new Error(
			"Weighted augmenting-path outcome appears before certification",
		);
	}
}

function validateWeightedPushRelabelShortcutScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowWeightedPushRelabelShortcutOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "weighted-push-relabel";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error(
				"Weighted push-relabel shortcut overlay uses the wrong algorithm",
			);
		}
		return;
	}
	if (model.kind !== "max-flow" || Object.keys(config).length !== 0) {
		throw new Error(
			"Weighted push-relabel shortcuts require an unconfigured max-flow model",
		);
	}
	if (
		nodes.length < 2 ||
		nodes.length > 8 ||
		edges.length < 1 ||
		edges.length > 12 ||
		model.source === model.sink ||
		nodes.every((node) => node.id !== model.source) ||
		nodes.every((node) => node.id !== model.sink) ||
		nodes.some((node) => node.supply !== "0") ||
		edges.some(
			(edge) =>
				edge.lower !== "0" ||
				BigInt(edge.capacity) <= 0n ||
				BigInt(edge.capacity) > 64n ||
				edge.from === edge.to,
		)
	) {
		throw new Error(
			"Weighted push-relabel shortcut graph is outside its bounded domain",
		);
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined)
		) {
			return;
		}
		throw new Error(
			"Weighted push-relabel scene is missing its shortcut overlay",
		);
	}
	const nodeOrder = canonicalNodeIds(nodes);
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const orderedEdges = canonicalStableIds(edges).map(
		(id) => edgeById.get(id) as FlowEdgeV1,
	);
	if (overlay.stage === "ready") {
		if (
			overlay.hierarchy_levels !== "0" ||
			overlay.height !== "0" ||
			overlay.demand !== "0" ||
			overlay.nodes.length !== nodeOrder.length ||
			overlay.nodes.some(
				(state, index) => !state.original || state.node_id !== nodeOrder[index],
			) ||
			overlay.edges.length !== 0 ||
			overlay.residual_arcs.length !== 0 ||
			overlay.active_path.length !== 0
		) {
			throw new Error("Weighted push-relabel ready overlay is inconsistent");
		}
		return;
	}
	if (
		overlay.hierarchy_levels !== "1" ||
		overlay.psi_numerator !== "1" ||
		overlay.psi_denominator !== "1" ||
		BigInt(overlay.height) <= 0n ||
		BigInt(overlay.demand) <= 0n ||
		BigInt(overlay.routed) > BigInt(overlay.demand) ||
		BigInt(overlay.weighted_length_units) !==
			(BigInt(overlay.routed) > 0n ? BigInt(overlay.routed) : 1n) ||
		overlay.nodes.length < nodeOrder.length ||
		overlay.edges.length < orderedEdges.length ||
		overlay.residual_arcs.length !== overlay.edges.length * 2 ||
		edgeStates.length !== orderedEdges.length
	) {
		throw new Error("Weighted push-relabel shortcut shape is inconsistent");
	}
	const nodeById = new Map(
		overlay.nodes.map((state, index) => [state.node_id, { state, index }]),
	);
	if (nodeById.size !== overlay.nodes.length) {
		throw new Error("Weighted push-relabel shortcut node IDs are not unique");
	}
	const componentSizes = new Map<string, number>();
	for (let index = 0; index < overlay.nodes.length; index += 1) {
		const state = overlay.nodes[
			index
		] as FlowWeightedPushRelabelShortcutNodeStateV1;
		const label = BigInt(state.label);
		if (
			state.alive !== label <= 9n * BigInt(overlay.height) ||
			(state.original && BigInt(state.order) === 0n) ||
			(!state.original &&
				(BigInt(state.order) !== 0n || !state.node_id.startsWith("shortcut:")))
		) {
			throw new Error("Weighted push-relabel node label/order is invalid");
		}
		if (index < nodeOrder.length) {
			if (!state.original || state.node_id !== nodeOrder[index]) {
				throw new Error("Weighted push-relabel original-node identity drifted");
			}
			componentSizes.set(
				state.component,
				(componentSizes.get(state.component) ?? 0) + 1,
			);
		} else if (state.original) {
			throw new Error("Weighted push-relabel Steiner node is mislabeled");
		}
	}
	const originalStates = overlay.edges.slice(0, orderedEdges.length);
	if (
		originalStates.some((state, index) => {
			const edge = orderedEdges[index] as FlowEdgeV1;
			const fromOrder = BigInt(
				(nodeById.get(edge.from)?.state.order as string | undefined) ?? "0",
			);
			const toOrder = BigInt(
				(nodeById.get(edge.to)?.state.order as string | undefined) ?? "0",
			);
			return (
				state.kind !== "original" ||
				state.edge_id !== edge.id ||
				state.from !== edge.from ||
				state.to !== edge.to ||
				state.capacity !== edge.capacity ||
				state.shortcut_component !== undefined ||
				BigInt(state.flow) > BigInt(state.capacity) ||
				BigInt(state.weight) !==
					(fromOrder >= toOrder ? fromOrder - toOrder : toOrder - fromOrder) ||
				edgeStates[index]?.edge_id !== state.edge_id ||
				edgeStates[index]?.flow !== state.flow
			);
		})
	) {
		throw new Error(
			"Weighted push-relabel original-edge projection is inconsistent",
		);
	}
	const edgeByAugmentedId = new Map(
		overlay.edges.map((state) => [state.edge_id, state]),
	);
	if (edgeByAugmentedId.size !== overlay.edges.length) {
		throw new Error("Weighted push-relabel augmented edge IDs are not unique");
	}
	for (
		let index = orderedEdges.length;
		index < overlay.edges.length;
		index += 1
	) {
		const state = overlay.edges[
			index
		] as FlowWeightedPushRelabelShortcutEdgeStateV1;
		const component = state.shortcut_component;
		const from = nodeById.get(state.from)?.state;
		const to = nodeById.get(state.to)?.state;
		if (
			state.kind !== "shortcut" ||
			!state.edge_id.startsWith("shortcut-edge:") ||
			component === undefined ||
			from === undefined ||
			to === undefined ||
			from.component !== component ||
			to.component !== component ||
			from.original === to.original ||
			BigInt(state.capacity) <= 0n ||
			BigInt(state.flow) > BigInt(state.capacity) ||
			BigInt(state.weight) !== BigInt(componentSizes.get(component) ?? 0)
		) {
			throw new Error("Weighted push-relabel shortcut star is inconsistent");
		}
	}
	const residualByKey = new Map<
		string,
		FlowWeightedPushRelabelShortcutResidualArcStateV1
	>();
	for (let index = 0; index < overlay.edges.length; index += 1) {
		const edge = overlay.edges[
			index
		] as FlowWeightedPushRelabelShortcutEdgeStateV1;
		for (let directionIndex = 0; directionIndex < 2; directionIndex += 1) {
			const arc = overlay.residual_arcs[
				index * 2 + directionIndex
			] as FlowWeightedPushRelabelShortcutResidualArcStateV1;
			const forward = directionIndex === 0;
			const direction = forward ? "forward" : "reverse";
			const capacity = forward
				? BigInt(edge.capacity) - BigInt(edge.flow)
				: BigInt(edge.flow);
			const key = `${arc.edge_id}\u0000${arc.direction}`;
			if (
				arc.edge_id !== edge.edge_id ||
				arc.direction !== direction ||
				arc.from !== (forward ? edge.from : edge.to) ||
				arc.to !== (forward ? edge.to : edge.from) ||
				BigInt(arc.capacity) !== capacity ||
				arc.weight !== edge.weight ||
				(arc.admissible && capacity === 0n) ||
				residualByKey.has(key)
			) {
				throw new Error(
					"Weighted push-relabel residual identity/capacity is inconsistent",
				);
			}
			residualByKey.set(key, arc);
		}
	}
	const activePathKeys = overlay.active_path.map(
		(reference) => `${reference.edge_id}\u0000${reference.direction}`,
	);
	const inspectedKeys = overlay.inspected_arcs.map(
		(reference) => `${reference.edge_id}\u0000${reference.direction}`,
	);
	const activeKeys = new Set([...activePathKeys, ...inspectedKeys]);
	const augmentStage =
		overlay.stage === "augment-path" ||
		overlay.stage === "completion-augment-path";
	const inspectionStage =
		overlay.stage === "inspect-primitive-arc-checkpoint" ||
		overlay.stage === "completion-inspect-primitive-arc-checkpoint";
	const relabelStage =
		overlay.stage === "relabel-checkpoint" ||
		overlay.stage === "completion-relabel-checkpoint";
	if (
		activeKeys.size !== activePathKeys.length + inspectedKeys.length ||
		[...activeKeys].some((key) => !residualByKey.has(key)) ||
		(BigInt(overlay.active_bottleneck) === 0n) !==
			(activePathKeys.length === 0) ||
		augmentStage !== BigInt(overlay.active_bottleneck) > 0n ||
		inspectionStage !== inspectedKeys.length > 0 ||
		relabelStage !== (overlay.active_relabel_nodes.length === 1) ||
		overlay.active_relabel_nodes.some((node) => !nodeById.has(node)) ||
		overlay.residual_arcs.some(
			(arc) =>
				arc.active !== activeKeys.has(`${arc.edge_id}\u0000${arc.direction}`),
		)
	) {
		throw new Error("Weighted push-relabel active path flags are inconsistent");
	}
	let pathNode = model.source;
	for (const key of activePathKeys) {
		const arc = residualByKey.get(key);
		const edge =
			arc === undefined ? undefined : edgeByAugmentedId.get(arc.edge_id);
		const from = arc === undefined ? undefined : nodeById.get(arc.from)?.state;
		const to = arc === undefined ? undefined : nodeById.get(arc.to)?.state;
		const residualCapacity =
			arc === undefined ? undefined : BigInt(arc.capacity);
		if (
			arc === undefined ||
			edge === undefined ||
			from === undefined ||
			to === undefined ||
			arc.from !== pathNode ||
			(!arc.admissible && residualCapacity !== 0n) ||
			from.alive === false
		) {
			throw new Error(
				`Weighted push-relabel active path is not admissible at ${key}: expected tail ${pathNode}, actual ${arc?.from ?? "missing"} → ${arc?.to ?? "missing"}, residual ${arc?.capacity ?? "missing"}, persistent admissible ${arc?.admissible ?? false}, alive ${from?.alive ?? false}`,
			);
		}
		pathNode = arc.to;
	}
	if (activePathKeys.length > 0 && pathNode !== model.sink) {
		throw new Error("Weighted push-relabel active path misses the sink");
	}
	const completed = [
		"complete-residual-rounds",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const divergence = new Map(overlay.nodes.map((node) => [node.node_id, 0n]));
	const flowStates = completed ? originalStates : overlay.edges;
	for (const edge of flowStates) {
		const flow = BigInt(edge.flow);
		divergence.set(edge.from, (divergence.get(edge.from) ?? 0n) + flow);
		divergence.set(edge.to, (divergence.get(edge.to) ?? 0n) - flow);
	}
	const value = divergence.get(model.source) ?? 0n;
	if (
		value < 0n ||
		divergence.get(model.sink) !== -value ||
		[...divergence].some(
			([node, balance]) =>
				node !== model.source && node !== model.sink && balance !== 0n,
		)
	) {
		throw new Error(
			"Weighted push-relabel displayed flow violates conservation",
		);
	}
	validateTraceStageIdentity(
		traceEvent,
		"weighted-push-relabel",
		overlay.stage,
		"Weighted push-relabel",
	);
	if (overlay.stage === "optimal") {
		const sourceSide = overlay.nodes
			.filter((node) => node.original && node.source_side)
			.map((node) => node.node_id);
		const sourceSet = new Set(sourceSide);
		const cut = orderedEdges.reduce(
			(sum, edge) =>
				sourceSet.has(edge.from) && !sourceSet.has(edge.to)
					? sum + BigInt(edge.capacity)
					: sum,
			0n,
		);
		if (
			solveStatus !== "optimal" ||
			outcome?.kind !== "max-flow" ||
			BigInt(outcome.value) !== value ||
			BigInt(outcome.cut_bound) !== cut ||
			cut !== value ||
			outcome.source_side.length !== sourceSide.length ||
			outcome.source_side.some((node, index) => node !== sourceSide[index])
		) {
			throw new Error(
				"Weighted push-relabel optimum lacks its exact certificate",
			);
		}
	} else if (outcome !== undefined || solveStatus === "optimal") {
		throw new Error(
			"Weighted push-relabel outcome appears before certification",
		);
	}
}

function validateRandomizedAlmostLinearScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowRandomizedAlmostLinearOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected =
		algorithmId === "randomized-almost-linear-max-flow-oracle-demonstrator";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error(
				"Randomized almost-linear overlay uses the wrong algorithm",
			);
		}
		return;
	}
	if (model.kind !== "max-flow" || Object.keys(config).length !== 0) {
		throw new Error(
			"Randomized almost-linear flow requires an unconfigured max-flow model",
		);
	}
	if (
		nodes.length < 2 ||
		nodes.length > 8 ||
		edges.length < 1 ||
		edges.length > 10 ||
		model.source === model.sink ||
		nodes.every((node) => node.id !== model.source) ||
		nodes.every((node) => node.id !== model.sink) ||
		nodes.some((node) => node.supply !== "0") ||
		edges.some(
			(edge) =>
				edge.lower !== "0" ||
				BigInt(edge.capacity) <= 0n ||
				edge.from === edge.to,
		) ||
		edges.reduce(
			(product, edge) => product * (BigInt(edge.capacity) + 1n),
			1n,
		) > 100_000n
	) {
		throw new Error(
			"Randomized almost-linear graph is outside its bounded domain",
		);
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined)
		) {
			return;
		}
		throw new Error(
			"Randomized almost-linear scene is missing its tree-chain overlay",
		);
	}

	const nodeOrder = canonicalNodeIds(nodes);
	const edgeOrder = canonicalStableIds(edges);
	if (
		overlay.nodes.length !== nodeOrder.length ||
		overlay.nodes.some((state, index) => state.node_id !== nodeOrder[index]) ||
		overlay.edges.length !== edgeOrder.length ||
		overlay.edges.some((state, index) => state.edge_id !== edgeOrder[index]) ||
		edgeStates.length !== edgeOrder.length
	) {
		throw new Error(
			"Randomized almost-linear stable identities are inconsistent",
		);
	}

	const maximumCapacity = edges.reduce(
		(maximum, edge) =>
			BigInt(edge.capacity) > maximum ? BigInt(edge.capacity) : maximum,
		0n,
	);
	const expectedReturnCapacity = BigInt(edges.length) * maximumCapacity;
	const returnCapacity = BigInt(overlay.return_capacity);
	const alpha = Number(overlay.alpha);
	const potential = Number(overlay.potential);
	const costGap = Number(overlay.cost_gap);
	const returnFlow = Number(overlay.return_flow);
	const returnGradient = Number(overlay.return_gradient);
	const returnLength = Number(overlay.return_length);
	const returnTreeMemberships = BigInt(overlay.return_tree_memberships);
	const forestPool = BigInt(overlay.forest_pool_size);
	const sampleCount = BigInt(overlay.sample_count);
	const iteration = BigInt(overlay.iteration);
	const rebuildEpoch = BigInt(overlay.rebuild_epoch);
	const randomDraws = BigInt(overlay.random_draws);
	const probabilityNumerator = BigInt(overlay.miss_probability.numerator);
	const probabilityDenominator = BigInt(overlay.miss_probability.denominator);
	const isolationScale = BigInt(overlay.isolation_scale);
	const isolationAttempt = BigInt(overlay.isolation_attempt);
	const isolationProbabilityNumerator = BigInt(
		overlay.isolation_failure_probability.numerator,
	);
	const isolationProbabilityDenominator = BigInt(
		overlay.isolation_failure_probability.denominator,
	);
	const reductionEdges = BigInt(edges.length + 1);
	const expectedIsolationScale =
		4n *
		reductionEdges *
		reductionEdges *
		expectedReturnCapacity *
		expectedReturnCapacity;
	const finalPointThreshold = Number(overlay.final_point_threshold);
	if (
		alpha <= 0 ||
		alpha >= 1 ||
		!Number.isFinite(potential) ||
		costGap <= 0 ||
		returnCapacity !== expectedReturnCapacity ||
		returnFlow <= 0 ||
		returnFlow >= Number(returnCapacity) ||
		!Number.isFinite(returnGradient) ||
		returnLength <= 0 ||
		returnTreeMemberships > sampleCount ||
		(overlay.active_return_tree_edge && returnTreeMemberships === 0n) ||
		sampleCount < 4n ||
		sampleCount > 6n ||
		forestPool > 250_000n ||
		iteration > 8n ||
		rebuildEpoch > 6n ||
		probabilityNumerator > probabilityDenominator ||
		isolationScale !== expectedIsolationScale ||
		isolationProbabilityNumerator > isolationProbabilityDenominator ||
		finalPointThreshold <= 0
	) {
		throw new Error(
			"Randomized almost-linear source scalar or resource bound is inconsistent",
		);
	}

	const forestReady = ![
		"ready",
		"build-return-edge-reduction",
		"build-initial-point",
	].includes(overlay.stage);
	if (
		forestReady !== forestPool > 0n ||
		(overlay.stage === "enumerate-forest-pool"
			? randomDraws !== 0n
			: forestReady !== randomDraws > 0n)
	) {
		throw new Error(
			"Randomized almost-linear forest population or draw count is inconsistent",
		);
	}
	const isolationReady = [
		"sample-isolation-costs",
		"select-isolated-optimum",
		"construct-final-point",
		"round-nearest-integer",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const finalPointReady = [
		"construct-final-point",
		"round-nearest-integer",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const terminal = [
		"round-nearest-integer",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	if (
		isolationReady !== isolationAttempt > 0n ||
		isolationReady !== (overlay.isolated_objective !== undefined) ||
		isolationReady !== BigInt(overlay.return_isolation_draw) > 0n ||
		(isolationReady
			? isolationProbabilityNumerator !== 1n ||
				isolationProbabilityDenominator !== 2n ** isolationAttempt
			: isolationProbabilityNumerator !== 1n ||
				isolationProbabilityDenominator !== 1n) ||
		finalPointReady !== (overlay.final_point_return_flow !== undefined) ||
		finalPointReady !== (overlay.final_point_gap !== undefined) ||
		finalPointReady !== (overlay.final_point_mix !== undefined) ||
		(overlay.final_point_gap !== undefined &&
			(Number(overlay.final_point_gap) < 0 ||
				Number(overlay.final_point_gap) > finalPointThreshold)) ||
		(overlay.final_point_mix !== undefined &&
			(Number(overlay.final_point_mix) < 0 ||
				Number(overlay.final_point_mix) > 0.25)) ||
		terminal !== (overlay.final_return_flow !== undefined) ||
		terminal !== (overlay.final_artificial_flow !== undefined) ||
		terminal !== overlay.edges.every((edge) => edge.final_flow !== undefined)
	) {
		throw new Error(
			"Randomized almost-linear isolation, final-point, or rounding fields are inconsistent",
		);
	}

	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const genericFlowById = new Map(
		edgeStates.map((state) => [state.edge_id, state.flow]),
	);
	const divergence = new Map<string, number>(
		nodeOrder.map((node) => [node, 0]),
	);
	divergence.set("__artificial_star__", 0);
	const addDivergence = (node: string, delta: number): void => {
		divergence.set(node, (divergence.get(node) ?? 0) + delta);
	};
	let artificialEdges = 0n;
	let artificialFlow = 0;
	for (const state of overlay.nodes) {
		const direction = Number(state.artificial_direction);
		const activeSign = Number(state.active_artificial_sign);
		const flow = Number(state.artificial_flow);
		const capacity = Number(state.artificial_capacity);
		const treeMemberships = BigInt(state.artificial_tree_memberships);
		const component = BigInt(state.tree_component);
		const validParent =
			state.tree_parent_node_id === undefined ||
			state.tree_parent_node_id === "__artificial_star__" ||
			nodeOrder.includes(state.tree_parent_node_id);
		if (
			component > BigInt(nodes.length) ||
			!validParent ||
			(direction === 0 &&
				(flow !== 0 ||
					capacity !== 0 ||
					activeSign !== 0 ||
					treeMemberships !== 0n ||
					state.active_artificial_tree_edge)) ||
			treeMemberships > sampleCount ||
			(state.active_artificial_tree_edge && treeMemberships === 0n) ||
			(direction !== 0 && !(flow > 0 && flow < capacity))
		) {
			throw new Error(
				"Randomized almost-linear artificial-star state is inconsistent",
			);
		}
		if (direction !== 0) {
			artificialEdges += 1n;
			artificialFlow += flow;
			const from = direction > 0 ? "__artificial_star__" : state.node_id;
			const to = direction > 0 ? state.node_id : "__artificial_star__";
			addDivergence(from, activeSign);
			addDivergence(to, -activeSign);
		}
	}
	if (
		artificialEdges !== BigInt(overlay.artificial_edges) ||
		!electricalClose(artificialFlow, Number(overlay.artificial_flow)) ||
		Number(overlay.artificial_flow) < 0
	) {
		throw new Error(
			"Randomized almost-linear artificial-star summary is inconsistent",
		);
	}

	for (const state of overlay.edges) {
		const edge = edgeById.get(state.edge_id);
		if (edge === undefined) {
			throw new Error("Randomized almost-linear edge projection is incomplete");
		}
		const interiorFlow = Number(state.interior_flow);
		const gradient = Number(state.gradient);
		const length = Number(state.length);
		const memberships = BigInt(state.sampled_tree_memberships);
		const sign = Number(state.active_cycle_sign);
		const isolationDraw = BigInt(state.isolation_draw);
		const finalPointFlow =
			state.final_point_flow === undefined
				? undefined
				: Number(state.final_point_flow);
		const assignmentInspection =
			overlay.stage === "inspect-feasible-assignment";
		if (
			(assignmentInspection
				? interiorFlow < 0 || interiorFlow > Number(edge.capacity)
				: interiorFlow <= 0 || interiorFlow >= Number(edge.capacity)) ||
			!Number.isFinite(gradient) ||
			length <= 0 ||
			memberships > sampleCount ||
			isolationReady !== isolationDraw > 0n ||
			finalPointReady !== (finalPointFlow !== undefined) ||
			(finalPointFlow !== undefined &&
				(finalPointFlow < 0 || finalPointFlow > Number(edge.capacity))) ||
			terminal !== (state.final_flow !== undefined) ||
			(terminal &&
				(finalPointFlow === undefined ||
					Math.round(finalPointFlow) !== Number(state.final_flow))) ||
			(state.final_flow !== undefined &&
				(BigInt(state.final_flow) > BigInt(edge.capacity) ||
					genericFlowById.get(state.edge_id) !== state.final_flow)) ||
			(!terminal && genericFlowById.get(state.edge_id) !== "0")
		) {
			throw new Error(
				"Randomized almost-linear edge coordinate or final flow is inconsistent",
			);
		}
		addDivergence(edge.from, sign);
		addDivergence(edge.to, -sign);
	}
	const returnSign = Number(overlay.active_return_sign);
	addDivergence(model.sink, returnSign);
	addDivergence(model.source, -returnSign);
	if ([...divergence.values()].some((value) => value !== 0)) {
		throw new Error(
			"Randomized almost-linear active direction is not a circulation",
		);
	}

	const expectedStatus: Record<
		FlowRandomizedAlmostLinearOverlayV1["stage"],
		FlowCurrentSceneV9["solve_status"]
	> = {
		ready: "ready",
		"build-return-edge-reduction": "running",
		"build-initial-point": "running",
		"enumerate-forest-pool": "running",
		"sample-tree-chain": "running",
		"inspect-fundamental-cycle": "running",
		"query-minimum-ratio-cycle": "running",
		"sampling-failure": "running",
		"potential-reduction-step": "running",
		"detect-changed-coordinates": "running",
		"rebuild-tree-chain": "running",
		"inspect-feasible-assignment": "running",
		"enumerate-feasible-set": "running",
		"sample-isolation-costs": "running",
		"select-isolated-optimum": "running",
		"construct-final-point": "running",
		"round-nearest-integer": "running",
		"check-certificate": "running",
		optimal: "optimal",
	};
	if (
		solveStatus !== "resource-limit" &&
		(solveStatus !== expectedStatus[overlay.stage] ||
			(overlay.stage === "ready" && eventId !== "0"))
	) {
		throw new Error("Randomized almost-linear stage and solve status disagree");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const catalog: Record<
			FlowRandomizedAlmostLinearOverlayV1["stage"],
			string
		> = {
			ready: "randomized-almost-linear-max-flow-oracle-demonstrator.ready",
			"build-return-edge-reduction":
				"randomized-almost-linear-max-flow-oracle-demonstrator.return-edge",
			"build-initial-point":
				"randomized-almost-linear-max-flow-oracle-demonstrator.initial-point",
			"enumerate-forest-pool":
				"randomized-almost-linear-max-flow-oracle-demonstrator.forest-pool",
			"sample-tree-chain":
				"randomized-almost-linear-max-flow-oracle-demonstrator.sample-chain",
			"inspect-fundamental-cycle":
				"randomized-almost-linear-max-flow-oracle-demonstrator.inspect-fundamental-cycle",
			"query-minimum-ratio-cycle":
				"randomized-almost-linear-max-flow-oracle-demonstrator.query-cycle",
			"sampling-failure":
				"randomized-almost-linear-max-flow-oracle-demonstrator.sampling-failure",
			"potential-reduction-step":
				"randomized-almost-linear-max-flow-oracle-demonstrator.potential-step",
			"detect-changed-coordinates":
				"randomized-almost-linear-max-flow-oracle-demonstrator.detect",
			"rebuild-tree-chain":
				"randomized-almost-linear-max-flow-oracle-demonstrator.rebuild",
			"inspect-feasible-assignment":
				"randomized-almost-linear-max-flow-oracle-demonstrator.inspect-feasible-assignment",
			"enumerate-feasible-set":
				"randomized-almost-linear-max-flow-oracle-demonstrator.enumerate-feasible-set",
			"sample-isolation-costs":
				"randomized-almost-linear-max-flow-oracle-demonstrator.sample-isolation-costs",
			"select-isolated-optimum":
				"randomized-almost-linear-max-flow-oracle-demonstrator.select-isolated-optimum",
			"construct-final-point":
				"randomized-almost-linear-max-flow-oracle-demonstrator.construct-final-point",
			"round-nearest-integer":
				"randomized-almost-linear-max-flow-oracle-demonstrator.round-nearest-integer",
			"check-certificate":
				"randomized-almost-linear-max-flow-oracle-demonstrator.check-certificate",
			optimal: "randomized-almost-linear-max-flow-oracle-demonstrator.optimal",
		};
		if (traceEvent.catalog_id !== catalog[overlay.stage]) {
			throw new Error(
				"Randomized almost-linear trace event and stage disagree",
			);
		}
	}

	if (terminal) {
		const target = BigInt(overlay.target_value);
		if (
			overlay.final_return_flow === undefined ||
			BigInt(overlay.final_return_flow) !== target ||
			overlay.final_point_return_flow === undefined ||
			Math.round(Number(overlay.final_point_return_flow)) !== Number(target) ||
			overlay.final_artificial_flow !== "0"
		) {
			throw new Error(
				"Randomized almost-linear return or artificial rounding is inconsistent",
			);
		}
		const exactDivergence = new Map(nodeOrder.map((node) => [node, 0n]));
		for (const state of overlay.edges) {
			const edge = edgeById.get(state.edge_id) as FlowEdgeV1;
			const flow = BigInt(state.final_flow as string);
			exactDivergence.set(
				edge.from,
				(exactDivergence.get(edge.from) ?? 0n) + flow,
			);
			exactDivergence.set(edge.to, (exactDivergence.get(edge.to) ?? 0n) - flow);
		}
		for (const [node, balance] of exactDivergence) {
			const expected =
				node === model.source ? target : node === model.sink ? -target : 0n;
			if (balance !== expected) {
				throw new Error(
					"Randomized almost-linear rounded flow violates conservation",
				);
			}
		}
		const sourceSide = overlay.nodes
			.filter((node) => node.source_side)
			.map((node) => node.node_id);
		const sourceSet = new Set(sourceSide);
		const cut = edges.reduce(
			(sum, edge) =>
				sourceSet.has(edge.from) && !sourceSet.has(edge.to)
					? sum + BigInt(edge.capacity)
					: sum,
			0n,
		);
		if (
			!sourceSet.has(model.source) ||
			sourceSet.has(model.sink) ||
			cut !== target
		) {
			throw new Error(
				"Randomized almost-linear rounded flow lacks a matching minimum cut",
			);
		}
		if (overlay.stage === "optimal") {
			if (
				outcome?.kind !== "max-flow" ||
				BigInt(outcome.value) !== target ||
				BigInt(outcome.cut_bound) !== target ||
				outcome.source_side.length !== sourceSide.length ||
				outcome.source_side.some((node, index) => node !== sourceSide[index])
			) {
				throw new Error(
					"Randomized almost-linear optimum lacks its exact certificate",
				);
			}
		} else if (outcome !== undefined) {
			throw new Error(
				"Randomized almost-linear outcome appears before certification",
			);
		}
	} else if (outcome !== undefined) {
		throw new Error(
			"Randomized almost-linear outcome appears before source final-point rounding",
		);
	}
}

function validateDeterministicAlmostLinearScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowDeterministicAlmostLinearOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected =
		algorithmId === "deterministic-almost-linear-max-flow-oracle-demonstrator";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error(
				"Deterministic almost-linear overlay uses the wrong algorithm",
			);
		}
		return;
	}
	if (model.kind !== "max-flow" || Object.keys(config).length !== 0) {
		throw new Error(
			"Deterministic almost-linear flow requires an unconfigured max-flow model",
		);
	}
	if (
		nodes.length < 2 ||
		nodes.length > 7 ||
		edges.length < 1 ||
		edges.length > 8 ||
		model.source === model.sink ||
		nodes.every((node) => node.id !== model.source) ||
		nodes.every((node) => node.id !== model.sink) ||
		nodes.some((node) => node.supply !== "0") ||
		edges.some(
			(edge) =>
				edge.lower !== "0" ||
				BigInt(edge.capacity) <= 0n ||
				edge.from === edge.to,
		)
	) {
		throw new Error(
			"Deterministic almost-linear graph is outside its bounded domain",
		);
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined)
		) {
			return;
		}
		throw new Error(
			"Deterministic almost-linear scene is missing its shifted-tree-chain overlay",
		);
	}
	const nodeOrder = canonicalNodeIds(nodes);
	const edgeOrder = canonicalStableIds(edges);
	if (
		overlay.nodes.length !== nodeOrder.length ||
		overlay.nodes.some((state, index) => state.node_id !== nodeOrder[index]) ||
		overlay.edges.length !== edgeOrder.length ||
		overlay.edges.some((state, index) => state.edge_id !== edgeOrder[index]) ||
		edgeStates.length !== edgeOrder.length
	) {
		throw new Error(
			"Deterministic almost-linear stable identities are inconsistent",
		);
	}
	const maximumCapacity = edges.reduce(
		(maximum, edge) =>
			BigInt(edge.capacity) > maximum ? BigInt(edge.capacity) : maximum,
		0n,
	);
	if (
		edges.reduce(
			(product, edge) => product * (BigInt(edge.capacity) + 1n),
			1n,
		) > 100_000n
	) {
		throw new Error(
			"Deterministic almost-linear feasible-face oracle exceeds its assignment budget",
		);
	}
	const maskLimit = 1n << BigInt(overlay.level_count);
	const zero: FlowRationalV1 = { numerator: "0", denominator: "1" };
	const half: FlowRationalV1 = { numerator: "1", denominator: "2" };
	const quarter: FlowRationalV1 = { numerator: "1", denominator: "4" };
	const selectedFields = [
		overlay.selected_ratio,
		overlay.selected_off_tree_edge,
		overlay.selected_cycle_kind,
	];
	const recordsPerCollection =
		BigInt(overlay.level_count) * BigInt(overlay.branch_count);
	const recordWindowStart =
		BigInt(overlay.rebuild_epoch) * recordsPerCollection;
	const recordWindowEnd = recordWindowStart + recordsPerCollection;
	const builtBranchRecords = BigInt(overlay.built_branch_records);
	const branchRecordProgressValid =
		builtBranchRecords >= recordWindowStart &&
		builtBranchRecords <= recordWindowEnd &&
		(overlay.stage === "install-branch-record"
			? (() => {
					const branchCount = BigInt(overlay.branch_count);
					if (branchCount === 0n || builtBranchRecords <= recordWindowStart)
						return false;
					const ordinal = builtBranchRecords - recordWindowStart - 1n;
					const expectedLevel = ordinal / branchCount;
					const expectedBranch = ordinal % branchCount;
					return (
						overlay.active_level === expectedLevel.toString() &&
						overlay.active_branches[Number(expectedLevel)] ===
							expectedBranch.toString()
					);
				})()
			: overlay.stage === "build-branch-collection"
				? builtBranchRecords === recordWindowEnd
				: true);
	if (
		Number(overlay.alpha) <= 0 ||
		Number(overlay.alpha) >= 1 ||
		!Number.isFinite(Number(overlay.potential)) ||
		Number(overlay.cost_gap) <= 0 ||
		compareRational(overlay.final_point_threshold, half) !== 0 ||
		BigInt(overlay.return_capacity) !==
			BigInt(edges.length) * maximumCapacity ||
		Number(overlay.return_flow) <= 0 ||
		Number(overlay.return_flow) >= Number(overlay.return_capacity) ||
		Number(overlay.return_length) <= 0 ||
		BigInt(overlay.return_tree_level_mask) >= maskLimit ||
		overlay.level_count !== "2" ||
		overlay.branch_count !== "3" ||
		!branchRecordProgressValid ||
		overlay.active_branches.length !== 2 ||
		overlay.active_branches.some((branch) => BigInt(branch) >= 3n) ||
		overlay.passes.length !== 2 ||
		overlay.passes.some((passes) => BigInt(passes) > 2n) ||
		(overlay.active_level !== undefined &&
			BigInt(overlay.active_level) >= 2n) ||
		BigInt(overlay.spanner_edges) > BigInt(overlay.core_edges) ||
		BigInt(overlay.core_vertices) > BigInt(nodes.length + 1) ||
		selectedFields.some(
			(field) =>
				(field === undefined) !== (overlay.selected_ratio === undefined),
		) ||
		(overlay.selected_ratio !== undefined &&
			Number(overlay.selected_ratio) >= 0 &&
			overlay.stage !== "inspect-fundamental-cycle")
	) {
		throw new Error(
			"Deterministic almost-linear branch/core contract is inconsistent",
		);
	}
	const geometryVisible = [
		"install-branch-record",
		"build-core-graph",
		"build-spanner-embedding",
		"query-minimum-ratio-cycle",
		"query-failure",
		"shift-branch",
		"rebuild-deeper-levels",
		"potential-reduction-step",
		"detect-changed-coordinates",
		"enumerate-feasible-set",
		"construct-final-point",
		"rounding-integral-edge",
		"rounding-link-fractional-edge",
		"rounding-cancel-fractional-cycle",
		"finish-flow-rounding",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	if (
		geometryVisible !== BigInt(overlay.core_vertices) > 0n ||
		BigInt(overlay.spanner_edges) > BigInt(overlay.core_edges) ||
		BigInt(overlay.core_vertices) > BigInt(nodes.length + 1)
	) {
		throw new Error(
			"Deterministic almost-linear core/spanner visibility is inconsistent",
		);
	}
	const finalPointReady = [
		"construct-final-point",
		"rounding-integral-edge",
		"rounding-link-fractional-edge",
		"rounding-cancel-fractional-cycle",
		"finish-flow-rounding",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const roundingReady = [
		"rounding-integral-edge",
		"rounding-link-fractional-edge",
		"rounding-cancel-fractional-cycle",
		"finish-flow-rounding",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const terminal = [
		"finish-flow-rounding",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const roundingOperation = [
		"rounding-integral-edge",
		"rounding-link-fractional-edge",
		"rounding-cancel-fractional-cycle",
	].includes(overlay.stage);
	const processedEdgeValid =
		overlay.rounding_processed_edge === undefined ||
		edgeOrder.includes(overlay.rounding_processed_edge) ||
		overlay.rounding_processed_edge.startsWith("deterministic-rounding-return");
	if (
		finalPointReady !== (overlay.final_point_gap !== undefined) ||
		finalPointReady !== (overlay.final_point_mix !== undefined) ||
		finalPointReady !== (overlay.final_point_return_flow !== undefined) ||
		roundingReady !== (overlay.rounding_return_flow !== undefined) ||
		(!roundingReady &&
			(overlay.rounding_return_forest_edge ||
				overlay.rounding_return_sign !== "0")) ||
		(overlay.final_point_gap !== undefined &&
			(compareRational(overlay.final_point_gap, zero) < 0 ||
				compareRational(
					overlay.final_point_gap,
					overlay.final_point_threshold,
				) >= 0)) ||
		(overlay.final_point_mix !== undefined &&
			(compareRational(overlay.final_point_mix, zero) <= 0 ||
				compareRational(overlay.final_point_mix, quarter) > 0)) ||
		terminal !== (overlay.final_return_flow !== undefined) ||
		terminal !== (overlay.final_artificial_flow !== undefined) ||
		terminal !== overlay.edges.every((edge) => edge.final_flow !== undefined) ||
		roundingOperation !== (overlay.rounding_processed_edge !== undefined) ||
		!processedEdgeValid
	) {
		throw new Error(
			"Deterministic almost-linear final-point publication is inconsistent",
		);
	}
	const nodeIds = new Set(nodeOrder);
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const genericFlowById = new Map(
		edgeStates.map((state) => [state.edge_id, state.flow]),
	);
	const activeDivergence = new Map<string, number>(
		nodeOrder.map((node) => [node, 0]),
	);
	type Fraction = { numerator: bigint; denominator: bigint };
	const exactZero = (): Fraction => ({ numerator: 0n, denominator: 1n });
	const addExact = (
		left: Fraction,
		right: FlowRationalV1,
		sign: bigint,
	): Fraction => ({
		numerator:
			left.numerator * BigInt(right.denominator) +
			sign * BigInt(right.numerator) * left.denominator,
		denominator: left.denominator * BigInt(right.denominator),
	});
	const finalPointDivergence = new Map<string, Fraction>(
		nodeOrder.map((node) => [node, exactZero()]),
	);
	const roundingDivergence = new Map<string, Fraction>(
		nodeOrder.map((node) => [node, exactZero()]),
	);
	const roundingCycleDivergence = new Map<string, number>(
		nodeOrder.map((node) => [node, 0]),
	);
	const roundingForestEndpoints: [string, string][] = [];
	activeDivergence.set("__artificial_star__", 0);
	const addActiveDivergence = (node: string, delta: number): void => {
		activeDivergence.set(node, (activeDivergence.get(node) ?? 0) + delta);
	};
	const addRoundingCycleDivergence = (node: string, delta: number): void => {
		roundingCycleDivergence.set(
			node,
			(roundingCycleDivergence.get(node) ?? 0) + delta,
		);
	};
	let projectedArtificialEdges = 0n;
	let projectedArtificialFlow = 0;
	for (const state of overlay.nodes) {
		const direction = Number(state.artificial_direction);
		const activeSign = Number(state.active_artificial_sign);
		if (
			BigInt(state.artificial_tree_level_mask) >= maskLimit ||
			BigInt(state.forest_component) > BigInt(nodes.length) ||
			(state.tree_parent_node_id !== undefined &&
				state.tree_parent_node_id !== "__artificial_star__" &&
				!nodeIds.has(state.tree_parent_node_id)) ||
			(state.active_artificial_tree_edge &&
				state.artificial_tree_level_mask === "0") ||
			(state.artificial_direction === "0" &&
				(state.artificial_flow !== "0" ||
					state.artificial_capacity !== "0" ||
					state.active_artificial_sign !== "0")) ||
			(state.artificial_direction !== "0" &&
				(Number(state.artificial_flow) <= 0 ||
					Number(state.artificial_flow) >= Number(state.artificial_capacity)))
		) {
			throw new Error(
				"Deterministic almost-linear node/tree state is inconsistent",
			);
		}
		if (direction !== 0) {
			projectedArtificialEdges += 1n;
			projectedArtificialFlow += Number(state.artificial_flow);
			const from = direction > 0 ? "__artificial_star__" : state.node_id;
			const to = direction > 0 ? state.node_id : "__artificial_star__";
			addActiveDivergence(from, activeSign);
			addActiveDivergence(to, -activeSign);
		}
	}
	if (
		projectedArtificialEdges !== BigInt(overlay.artificial_edges) ||
		!electricalClose(
			projectedArtificialFlow,
			Number(overlay.artificial_flow),
		) ||
		Number(overlay.artificial_flow) < 0
	) {
		throw new Error(
			"Deterministic almost-linear artificial-star summary is inconsistent",
		);
	}
	for (const state of overlay.edges) {
		const edge = edgeById.get(state.edge_id);
		const capacityRational: FlowRationalV1 | undefined =
			edge === undefined
				? undefined
				: { numerator: edge.capacity, denominator: "1" };
		const finalPointFlow = state.final_point_flow;
		const roundingFlow = state.rounding_flow;
		const roundingIntegral =
			roundingFlow !== undefined &&
			BigInt(roundingFlow.numerator) % BigInt(roundingFlow.denominator) === 0n;
		if (
			edge === undefined ||
			capacityRational === undefined ||
			Number(state.interior_flow) <= 0 ||
			Number(state.interior_flow) >= Number(edge.capacity) ||
			Number(state.length) <= 0 ||
			Number(state.embedding_stretch) < 0 ||
			BigInt(state.tree_level_mask) >= maskLimit ||
			BigInt(state.forest_level_mask) >= maskLimit ||
			(BigInt(state.forest_level_mask) & ~BigInt(state.tree_level_mask)) !==
				0n ||
			(state.active_tree_edge && state.tree_level_mask === "0") ||
			(state.active_spanner_edge && !state.active_core_edge) ||
			(state.embedding_hops === "0" && state.embedding_stretch !== "0") ||
			finalPointReady !== (finalPointFlow !== undefined) ||
			roundingReady !== (roundingFlow !== undefined) ||
			(finalPointFlow !== undefined &&
				(compareRational(finalPointFlow, zero) < 0 ||
					compareRational(finalPointFlow, capacityRational) > 0)) ||
			(roundingFlow !== undefined &&
				(compareRational(roundingFlow, zero) < 0 ||
					compareRational(roundingFlow, capacityRational) > 0)) ||
			(state.rounding_forest_edge && roundingIntegral) ||
			(!roundingReady &&
				(state.rounding_forest_edge || state.rounding_cycle_sign !== "0")) ||
			(overlay.stage !== "rounding-cancel-fractional-cycle" &&
				state.rounding_cycle_sign !== "0") ||
			(terminal &&
				(roundingFlow === undefined ||
					!roundingIntegral ||
					BigInt(roundingFlow.numerator) / BigInt(roundingFlow.denominator) !==
						BigInt(genericFlowById.get(state.edge_id) ?? "0"))) ||
			terminal !== (state.final_flow !== undefined) ||
			(state.final_flow !== undefined &&
				(BigInt(state.final_flow) > BigInt(edge.capacity) ||
					genericFlowById.get(state.edge_id) !== state.final_flow)) ||
			(!terminal && genericFlowById.get(state.edge_id) !== "0")
		) {
			throw new Error(
				"Deterministic almost-linear edge/core state is inconsistent",
			);
		}
		const sign = Number(state.active_cycle_sign);
		addActiveDivergence(edge.from, sign);
		addActiveDivergence(edge.to, -sign);
		if (finalPointFlow !== undefined) {
			finalPointDivergence.set(
				edge.from,
				addExact(
					finalPointDivergence.get(edge.from) as Fraction,
					finalPointFlow,
					1n,
				),
			);
			finalPointDivergence.set(
				edge.to,
				addExact(
					finalPointDivergence.get(edge.to) as Fraction,
					finalPointFlow,
					-1n,
				),
			);
		}
		if (roundingFlow !== undefined) {
			roundingDivergence.set(
				edge.from,
				addExact(
					roundingDivergence.get(edge.from) as Fraction,
					roundingFlow,
					1n,
				),
			);
			roundingDivergence.set(
				edge.to,
				addExact(
					roundingDivergence.get(edge.to) as Fraction,
					roundingFlow,
					-1n,
				),
			);
		}
		if (state.rounding_forest_edge) {
			roundingForestEndpoints.push([edge.from, edge.to]);
		}
		const roundingSign = Number(state.rounding_cycle_sign);
		addRoundingCycleDivergence(edge.from, roundingSign);
		addRoundingCycleDivergence(edge.to, -roundingSign);
	}
	const returnSign = Number(overlay.active_return_sign);
	addActiveDivergence(model.sink, returnSign);
	addActiveDivergence(model.source, -returnSign);
	if (overlay.final_point_return_flow !== undefined) {
		const returnCapacityRational: FlowRationalV1 = {
			numerator: overlay.return_capacity,
			denominator: "1",
		};
		if (
			compareRational(overlay.final_point_return_flow, zero) < 0 ||
			compareRational(overlay.final_point_return_flow, returnCapacityRational) >
				0
		) {
			throw new Error(
				"Deterministic almost-linear final-point return flow is out of bounds",
			);
		}
		finalPointDivergence.set(
			model.sink,
			addExact(
				finalPointDivergence.get(model.sink) as Fraction,
				overlay.final_point_return_flow,
				1n,
			),
		);
		finalPointDivergence.set(
			model.source,
			addExact(
				finalPointDivergence.get(model.source) as Fraction,
				overlay.final_point_return_flow,
				-1n,
			),
		);
		const expectedGap: FlowRationalV1 = {
			numerator: (
				BigInt(overlay.target_value) *
					BigInt(overlay.final_point_return_flow.denominator) -
				BigInt(overlay.final_point_return_flow.numerator)
			).toString(),
			denominator: overlay.final_point_return_flow.denominator,
		};
		if (
			overlay.final_point_gap === undefined ||
			compareRational(overlay.final_point_gap, expectedGap) !== 0
		) {
			throw new Error(
				"Deterministic almost-linear final-point gap does not match the return edge",
			);
		}
	}
	if (overlay.rounding_return_flow !== undefined) {
		const returnCapacityRational: FlowRationalV1 = {
			numerator: overlay.return_capacity,
			denominator: "1",
		};
		const roundingReturnIntegral =
			BigInt(overlay.rounding_return_flow.numerator) %
				BigInt(overlay.rounding_return_flow.denominator) ===
			0n;
		if (
			compareRational(overlay.rounding_return_flow, zero) < 0 ||
			compareRational(overlay.rounding_return_flow, returnCapacityRational) >
				0 ||
			(terminal &&
				(!roundingReturnIntegral ||
					BigInt(overlay.rounding_return_flow.numerator) /
						BigInt(overlay.rounding_return_flow.denominator) !==
						BigInt(overlay.target_value))) ||
			(overlay.rounding_return_forest_edge && roundingReturnIntegral)
		) {
			throw new Error(
				"Deterministic almost-linear rounded return flow is inconsistent",
			);
		}
		roundingDivergence.set(
			model.sink,
			addExact(
				roundingDivergence.get(model.sink) as Fraction,
				overlay.rounding_return_flow,
				1n,
			),
		);
		roundingDivergence.set(
			model.source,
			addExact(
				roundingDivergence.get(model.source) as Fraction,
				overlay.rounding_return_flow,
				-1n,
			),
		);
		if (overlay.rounding_return_forest_edge) {
			roundingForestEndpoints.push([model.sink, model.source]);
		}
	}
	const roundingReturnSign = Number(overlay.rounding_return_sign);
	addRoundingCycleDivergence(model.sink, roundingReturnSign);
	addRoundingCycleDivergence(model.source, -roundingReturnSign);
	if ([...activeDivergence.values()].some((value) => value !== 0)) {
		throw new Error(
			"Deterministic almost-linear active direction is not a circulation",
		);
	}
	const roundingCycleVisible =
		overlay.stage === "rounding-cancel-fractional-cycle";
	if (
		(finalPointReady &&
			[...finalPointDivergence.values()].some(
				(value) => value.numerator !== 0n,
			)) ||
		(roundingReady &&
			[...roundingDivergence.values()].some(
				(value) => value.numerator !== 0n,
			)) ||
		[...roundingCycleDivergence.values()].some((value) => value !== 0) ||
		roundingCycleVisible !==
			(overlay.rounding_return_sign !== "0" ||
				overlay.edges.some((edge) => edge.rounding_cycle_sign !== "0")) ||
		roundingReturnSign < 0
	) {
		throw new Error(
			"Deterministic almost-linear fractional circulation or rounding cycle is inconsistent",
		);
	}
	const forestParent = new Map(nodeOrder.map((node) => [node, node]));
	const findForestRoot = (node: string): string => {
		let root = node;
		while (forestParent.get(root) !== root) {
			root = forestParent.get(root) as string;
		}
		return root;
	};
	for (const [left, right] of roundingForestEndpoints) {
		const leftRoot = findForestRoot(left);
		const rightRoot = findForestRoot(right);
		if (leftRoot === rightRoot) {
			throw new Error(
				"Deterministic almost-linear fractional support is not a forest",
			);
		}
		forestParent.set(rightRoot, leftRoot);
	}
	if (
		terminal !== (overlay.final_return_flow !== undefined) ||
		terminal !== (overlay.final_artificial_flow !== undefined) ||
		terminal !== overlay.edges.every((edge) => edge.final_flow !== undefined) ||
		(terminal &&
			(overlay.final_return_flow !== overlay.target_value ||
				overlay.final_artificial_flow !== "0")) ||
		overlay.edges.some((state, index) => {
			const projected = edgeStates[index]?.flow;
			return terminal
				? state.final_flow !== projected
				: state.final_flow !== undefined || projected !== "0";
		})
	) {
		throw new Error("Deterministic almost-linear rounded flow is inconsistent");
	}
	const expectedStatus =
		overlay.stage === "ready"
			? "ready"
			: overlay.stage === "optimal"
				? "optimal"
				: "running";
	if (
		solveStatus !== "resource-limit" &&
		(solveStatus !== expectedStatus ||
			(overlay.stage === "ready" && eventId !== "0"))
	) {
		throw new Error(
			"Deterministic almost-linear stage and solve status disagree",
		);
	}
	const catalog: Record<
		FlowDeterministicAlmostLinearOverlayV1["stage"],
		string
	> = {
		ready: "deterministic-almost-linear-max-flow-oracle-demonstrator.ready",
		"build-return-edge-reduction":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.return-edge",
		"build-initial-point":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.initial-point",
		"enumerate-forest-pool":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.forest-pool",
		"install-branch-record":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.install-branch-record",
		"build-branch-collection":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.build-branches",
		"build-core-graph":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.build-core",
		"build-spanner-embedding":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.build-spanner",
		"inspect-fundamental-cycle":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.inspect-fundamental-cycle",
		"query-minimum-ratio-cycle":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.query-cycle",
		"query-failure":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.query-failure",
		"shift-branch":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.shift-branch",
		"rebuild-deeper-levels":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.rebuild-deeper",
		"potential-reduction-step":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.potential-step",
		"detect-changed-coordinates":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.detect",
		"scheduled-rebuild":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.scheduled-rebuild",
		"enumerate-feasible-set":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.enumerate-feasible-set",
		"construct-final-point":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.construct-final-point",
		"rounding-integral-edge":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.round-integral-edge",
		"rounding-link-fractional-edge":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.link-fractional-edge",
		"rounding-cancel-fractional-cycle":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.cancel-fractional-cycle",
		"finish-flow-rounding":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.finish-flow-rounding",
		"check-certificate":
			"deterministic-almost-linear-max-flow-oracle-demonstrator.check-certificate",
		optimal: "deterministic-almost-linear-max-flow-oracle-demonstrator.optimal",
	};
	if (
		traceEventRequiresStageIdentity(traceEvent) &&
		traceEvent.catalog_id !== catalog[overlay.stage]
	) {
		throw new Error(
			"Deterministic almost-linear trace event and stage disagree",
		);
	}
	if (terminal) {
		const target = BigInt(overlay.target_value);
		if (
			overlay.final_return_flow === undefined ||
			BigInt(overlay.final_return_flow) !== target ||
			overlay.final_artificial_flow !== "0"
		) {
			throw new Error(
				"Deterministic almost-linear return or artificial rounding is inconsistent",
			);
		}
		const exactDivergence = new Map(nodeOrder.map((node) => [node, 0n]));
		for (const state of overlay.edges) {
			const edge = edgeById.get(state.edge_id) as FlowEdgeV1;
			const flow = BigInt(state.final_flow as string);
			exactDivergence.set(
				edge.from,
				(exactDivergence.get(edge.from) ?? 0n) + flow,
			);
			exactDivergence.set(edge.to, (exactDivergence.get(edge.to) ?? 0n) - flow);
		}
		for (const [node, balance] of exactDivergence) {
			const expected =
				node === model.source ? target : node === model.sink ? -target : 0n;
			if (balance !== expected) {
				throw new Error(
					"Deterministic almost-linear rounded flow violates conservation",
				);
			}
		}
		const sourceSide = overlay.nodes
			.filter((node) => node.source_side)
			.map((node) => node.node_id);
		const sourceSet = new Set(sourceSide);
		const cut = edges.reduce(
			(sum, edge) =>
				sourceSet.has(edge.from) && !sourceSet.has(edge.to)
					? sum + BigInt(edge.capacity)
					: sum,
			0n,
		);
		if (
			!sourceSet.has(model.source) ||
			sourceSet.has(model.sink) ||
			cut !== target
		) {
			throw new Error(
				"Deterministic almost-linear rounded flow lacks a matching minimum cut",
			);
		}
	}
	if (overlay.stage === "optimal") {
		const sourceSide = overlay.nodes
			.filter((node) => node.source_side)
			.map((node) => node.node_id);
		if (
			outcome?.kind !== "max-flow" ||
			outcome.value !== overlay.target_value ||
			outcome.cut_bound !== overlay.target_value ||
			new Set(outcome.source_side).size !== outcome.source_side.length ||
			outcome.source_side.length !== sourceSide.length ||
			outcome.source_side.some((node, index) => node !== sourceSide[index])
		) {
			throw new Error(
				"Deterministic almost-linear completion lacks its certificate",
			);
		}
	} else if (outcome !== undefined) {
		throw new Error(
			"Deterministic almost-linear outcome appears before optimality",
		);
	}
}

function validateElectricalIpmMcfScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowElectricalIpmMcfOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "electrical-flow-interior-point-mcf";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Electrical IPM MCF overlay uses the wrong algorithm");
		}
		return;
	}
	if (
		!(
			["fixed-flow-min-cost", "circulation", "transshipment"] as const
		).includes(
			model.kind as "fixed-flow-min-cost" | "circulation" | "transshipment",
		) ||
		Object.keys(config).length !== 0
	) {
		throw new Error(
			"Electrical IPM MCF requires an unconfigured balance model",
		);
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined) ||
			(solveStatus === "infeasible" && outcome?.kind === "infeasible")
		) {
			return;
		}
		throw new Error("Electrical IPM MCF scene is missing its typed overlay");
	}
	if (
		nodes.length > 6 ||
		edges.length > 5 ||
		edges.some(
			(edge) =>
				BigInt(edge.capacity) > 8n ||
				(BigInt(edge.cost) < 0n ? -BigInt(edge.cost) : BigInt(edge.cost)) > 32n,
		) ||
		overlay.nodes.length !== nodes.length ||
		overlay.edges.length !== edges.length
	) {
		throw new Error("Electrical IPM MCF scene exceeds its bounded domain");
	}
	const mu = Number(overlay.mu);
	const epsilon3 = Number(overlay.epsilon_3);
	const recovery = Number(overlay.recovery_epsilon);
	const gap = Number(overlay.duality_gap_bound);
	const centrality = Number(overlay.centrality_residual);
	const balance = Number(overlay.balance_residual);
	const step = Number(overlay.step_size);
	const energy = Number(overlay.electrical_energy);
	const linearResidual = Number(overlay.linear_residual);
	if (
		[
			mu,
			epsilon3,
			recovery,
			gap,
			centrality,
			balance,
			energy,
			linearResidual,
		].some((value) => value < 0) ||
		step < 0 ||
		step > 1
	) {
		throw new Error("Electrical IPM MCF scalar domain is inconsistent");
	}
	const isolated = !["ready", "normalize-lower-bounds"].includes(overlay.stage);
	const faceContracted = ![
		"ready",
		"normalize-lower-bounds",
		"isolation-attempt",
		"select-isolated-costs",
	].includes(overlay.stage);
	const initialized = ![
		"ready",
		"normalize-lower-bounds",
		"isolation-attempt",
		"select-isolated-costs",
		"contract-fixed-face",
	].includes(overlay.stage);
	const rounded = [
		"round-nearest-integer",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const recoveryBoundary = [
		"approximate-flow",
		"round-nearest-integer",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const scale = BigInt(overlay.isolation_scale);
	const perturbationBound = BigInt(overlay.perturbation_bound);
	const attempt = BigInt(overlay.isolation_attempt);
	const isolatedGap = BigInt(overlay.isolated_gap);
	const isolatedOptimumCost = BigInt(overlay.isolated_optimum_cost);
	if (
		isolated !== (scale > 0n && perturbationBound > 0n && attempt > 0n) ||
		(overlay.stage !== "isolation-attempt" && isolated && isolatedGap <= 0n) ||
		(initialized && (epsilon3 <= 0 || epsilon3 >= 1 || recovery <= 0)) ||
		(overlay.stage === "centered" && centrality > 2.01e-7) ||
		(recoveryBoundary && gap > recovery * (1 + 1e-8))
	) {
		throw new Error("Electrical IPM MCF phase aggregate is inconsistent");
	}
	const nodeOrder = canonicalNodeIds(nodes);
	for (const [index, state] of overlay.nodes.entries()) {
		if (state.node_id !== nodeOrder[index]) {
			throw new Error("Electrical IPM MCF node projection drifted");
		}
	}
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const genericFlow = new Map(
		edgeStates.map((state) => [state.edge_id, state.flow]),
	);
	let workingEdges = 0;
	let roundedIsolatedCost = 0n;
	const projectedEdgeIds = new Set<string>();
	for (const state of overlay.edges) {
		const edge = edgeById.get(state.edge_id);
		const perturbation = BigInt(state.perturbation);
		const isolatedCost = BigInt(state.isolated_cost);
		const faceLower = BigInt(state.face_lower);
		const faceUpper = BigInt(state.face_upper);
		const fractional = Number(state.fractional_flow);
		const complement = Number(state.upper_complement);
		const slack = Number(state.lower_slack);
		const multiplier = Number(state.upper_multiplier);
		const resistance = Number(state.resistance);
		const conductance = Number(state.conductance);
		if (
			edge === undefined ||
			projectedEdgeIds.has(state.edge_id) ||
			faceLower < BigInt(edge.lower) ||
			faceUpper > BigInt(edge.capacity) ||
			faceLower > faceUpper ||
			(faceContracted && state.fixed_on_face !== (faceLower === faceUpper)) ||
			(isolated &&
				(perturbation <= 0n ||
					perturbation > perturbationBound ||
					isolatedCost !== scale * BigInt(edge.cost) + perturbation)) ||
			(!isolated && perturbation !== 0n) ||
			complement < 0 ||
			resistance < 0 ||
			conductance < 0
		) {
			throw new Error("Electrical IPM MCF edge projection is inconsistent");
		}
		projectedEdgeIds.add(state.edge_id);
		if (faceContracted && !state.fixed_on_face) {
			workingEdges += 1;
			if (
				initialized &&
				(slack <= 0 ||
					multiplier <= 0 ||
					!electricalClose(fractional, Number(faceLower) + mu / slack) ||
					!electricalClose(complement, mu / multiplier))
			) {
				throw new Error("Electrical IPM MCF central estimate is inconsistent");
			}
			if (
				["assemble-electrical-laplacian", "solve-newton-direction"].includes(
					overlay.stage,
				) &&
				(!electricalClose(
					resistance,
					(slack * slack + multiplier * multiplier) / mu,
				) ||
					!electricalClose(conductance, 1 / resistance))
			) {
				throw new Error("Electrical IPM MCF resistance is inconsistent");
			}
		}
		const published = genericFlow.get(edge.id);
		if (
			published === undefined ||
			(rounded
				? state.final_flow === undefined || state.final_flow !== published
				: state.final_flow !== undefined || published !== edge.lower)
		) {
			throw new Error("Electrical IPM MCF rounded flow projection drifted");
		}
		if (rounded) {
			const finalFlow = BigInt(state.final_flow as string);
			if (Math.abs(fractional - Number(finalFlow)) > 1 / 3 + 1e-8) {
				throw new Error(
					"Electrical IPM MCF recovery coordinate is not roundable",
				);
			}
			roundedIsolatedCost += isolatedCost * finalFlow;
		}
	}
	if (
		!electricalClose(gap, 2 * workingEdges * mu) ||
		(rounded && roundedIsolatedCost !== isolatedOptimumCost)
	) {
		throw new Error("Electrical IPM MCF gap identity is inconsistent");
	}
	if (rounded) {
		const required = projectLinearMcfRequiredDivergence(model, nodes);
		const divergence = new Map(nodeOrder.map((node) => [node, 0n]));
		let totalCost = 0n;
		for (const edge of edges) {
			const flow = BigInt(genericFlow.get(edge.id) as string);
			if (flow < BigInt(edge.lower) || flow > BigInt(edge.capacity)) {
				throw new Error("Electrical IPM MCF rounded flow violates a bound");
			}
			divergence.set(edge.from, (divergence.get(edge.from) ?? 0n) + flow);
			divergence.set(edge.to, (divergence.get(edge.to) ?? 0n) - flow);
			totalCost += flow * BigInt(edge.cost);
		}
		for (const node of nodes) {
			if (divergence.get(node.id) !== required.get(node.id)) {
				throw new Error("Electrical IPM MCF rounded flow violates balance");
			}
		}
		if (
			overlay.stage === "optimal" &&
			(outcome?.kind !== "min-cost-flow" ||
				BigInt(outcome.total_cost) !== totalCost)
		) {
			throw new Error("Electrical IPM MCF optimum lacks its exact objective");
		}
	} else if (outcome !== undefined) {
		throw new Error("Electrical IPM MCF outcome appears before rounding");
	}
	const expectedStatus =
		overlay.stage === "ready"
			? "ready"
			: overlay.stage === "optimal"
				? "optimal"
				: "running";
	if (
		solveStatus !== expectedStatus ||
		(overlay.stage === "ready" && eventId !== "0")
	) {
		throw new Error("Electrical IPM MCF stage and solve status disagree");
	}
	const catalog: Partial<
		Record<FlowElectricalIpmMcfOverlayV1["stage"], string>
	> = {
		"normalize-lower-bounds":
			"electrical-flow-interior-point-mcf.normalize-lower-bounds",
		"isolation-attempt": "electrical-flow-interior-point-mcf.isolation-attempt",
		"select-isolated-costs":
			"electrical-flow-interior-point-mcf.select-isolated-costs",
		"contract-fixed-face":
			"electrical-flow-interior-point-mcf.contract-fixed-face",
		"initialize-dual-interior":
			"electrical-flow-interior-point-mcf.initialize-dual",
		"assemble-electrical-laplacian":
			"electrical-flow-interior-point-mcf.assemble-laplacian",
		"solve-newton-direction":
			"electrical-flow-interior-point-mcf.newton-centering-iteration",
		"damped-centering-step":
			"electrical-flow-interior-point-mcf.newton-centering-iteration",
		centered: "electrical-flow-interior-point-mcf.centered",
		"decrease-barrier": "electrical-flow-interior-point-mcf.decrease-barrier",
		"approximate-flow": "electrical-flow-interior-point-mcf.approximate-flow",
		"round-nearest-integer":
			"electrical-flow-interior-point-mcf.round-nearest-integer",
		"check-certificate": "electrical-flow-interior-point-mcf.check-certificate",
		optimal: "electrical-flow-interior-point-mcf.optimal",
	};
	if (
		traceEventRequiresStageIdentity(traceEvent) &&
		traceEvent.catalog_id !== catalog[overlay.stage]
	) {
		throw new Error("Electrical IPM MCF trace event and stage disagree");
	}
}

function validatePrimalDualIpmMcfScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowPrimalDualIpmMcfOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "primal-dual-interior-point-mcf";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Primal-dual IPM overlay uses the wrong algorithm");
		}
		return;
	}
	if (
		!(
			["fixed-flow-min-cost", "circulation", "transshipment"] as const
		).includes(
			model.kind as "fixed-flow-min-cost" | "circulation" | "transshipment",
		) ||
		Object.keys(config).length !== 0
	) {
		throw new Error(
			"Primal-dual IPM requires an unconfigured balance-flow model",
		);
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready" && outcome === undefined) ||
			(solveStatus === "resource-limit" && outcome === undefined)
		) {
			return;
		}
		throw new Error("Primal-dual IPM scene is missing its auxiliary graph");
	}
	if (
		nodes.length > 6 ||
		edges.length > 5 ||
		edges.some(
			(edge) =>
				BigInt(edge.capacity) > 32n ||
				(BigInt(edge.cost) < 0n ? -BigInt(edge.cost) : BigInt(edge.cost)) > 32n,
		) ||
		overlay.arcs.length > 15 ||
		BigInt(overlay.beta) <= 0n ||
		BigInt(overlay.gamma) <= 0n ||
		BigInt(overlay.proxy_gap) < 0n ||
		BigInt(overlay.centrality_numerator) < 0n
	) {
		throw new Error(
			"Primal-dual IPM graph or integer grid is outside its domain",
		);
	}
	const preAuxiliary = ["ready", "normalize-input"].includes(overlay.stage);
	const initialized = ![
		"ready",
		"normalize-input",
		"build-capacity-reduction",
	].includes(overlay.stage);
	if (
		preAuxiliary !==
			(overlay.arcs.length === 0 && overlay.nodes.length === nodes.length) ||
		(!preAuxiliary &&
			(overlay.nodes.length < nodes.length ||
				overlay.nodes.length > nodes.length + edges.length)) ||
		BigInt(overlay.mu) < 0n ||
		initialized !== BigInt(overlay.mu) > 0n
	) {
		throw new Error("Primal-dual IPM transformation stage is inconsistent");
	}
	const proxyReached = [
		"proxy-reached",
		"crossover-grow-cut",
		"restore-original-dual",
		"recover-admissible-flow",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	if (
		proxyReached &&
		BigInt(overlay.proxy_gap) * 81n >=
			BigInt(overlay.beta) * BigInt(overlay.gamma) * 4n
	) {
		throw new Error("Primal-dual IPM proxy inequality is false");
	}
	if (
		["centered", "proxy-reached"].includes(overlay.stage) &&
		BigInt(overlay.centrality_numerator) * 8n >= BigInt(overlay.mu)
	) {
		throw new Error("Primal-dual IPM centrality inequality is false");
	}

	const nodeOrder = canonicalNodeIds(nodes);
	const nodeIds = new Set(nodeOrder);
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const auxiliaryIds = new Set<string>();
	const capacityEdges = new Set<string>();
	for (const [index, state] of overlay.nodes.entries()) {
		if (auxiliaryIds.has(state.auxiliary_id)) {
			throw new Error("Primal-dual IPM auxiliary node identity is duplicated");
		}
		auxiliaryIds.add(state.auxiliary_id);
		if (state.kind === "original") {
			if (
				index >= nodeOrder.length ||
				state.original_node_id !== nodeOrder[index] ||
				state.original_edge_id !== undefined ||
				state.auxiliary_id !== `node:${state.original_node_id}`
			) {
				throw new Error("Primal-dual IPM original-node projection drifted");
			}
		} else if (
			state.original_node_id !== undefined ||
			state.original_edge_id === undefined ||
			!edgeById.has(state.original_edge_id) ||
			capacityEdges.has(state.original_edge_id) ||
			state.auxiliary_id !== `capacity:${state.original_edge_id}`
		) {
			throw new Error("Primal-dual IPM capacity-node projection drifted");
		} else {
			capacityEdges.add(state.original_edge_id);
		}
	}
	if (
		overlay.nodes
			.slice(0, nodeOrder.length)
			.some((state) => state.kind !== "original") ||
		new Set(
			overlay.nodes
				.filter((state) => state.kind === "original")
				.map((state) => state.original_node_id),
		).size !== nodeIds.size
	) {
		throw new Error("Primal-dual IPM original nodes are incomplete");
	}
	const crossoverStage = [
		"crossover-grow-cut",
		"restore-original-dual",
		"recover-admissible-flow",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const resistanceAvailable = ![
		"ready",
		"normalize-input",
		"build-capacity-reduction",
		"initialize-central-point",
	].includes(overlay.stage);
	const inspectingForest = overlay.stage === "inspect-forest-subset";
	const forestSerialValid =
		overlay.forest_subset_serial !== undefined &&
		/^[1-9]\d*$/.test(overlay.forest_subset_serial);
	if (inspectingForest !== forestSerialValid) {
		throw new Error(
			"Primal-dual IPM forest inspection metadata is inconsistent",
		);
	}
	const auxiliaryArcIds = new Set<string>();
	const componentByAuxiliary = new Map(
		overlay.nodes.map((node) => [node.auxiliary_id, node.component]),
	);
	const cycleDivergence = new Map(
		overlay.nodes.map((node) => [node.component, 0]),
	);
	let projectedProxyGap = 0n;
	let projectedCentrality = 0n;
	for (const [index, arc] of overlay.arcs.entries()) {
		const flow = BigInt(arc.flow);
		const slack = BigInt(arc.slack);
		if (
			arc.auxiliary_id !== `aux:${index}` ||
			auxiliaryArcIds.has(arc.auxiliary_id) ||
			!edgeById.has(arc.original_edge_id) ||
			!auxiliaryIds.has(arc.from) ||
			!auxiliaryIds.has(arc.to) ||
			(arc.kind !== "artificial" && !capacityEdges.has(arc.original_edge_id)) ||
			(arc.deleted && arc.contracted) ||
			(arc.in_minor && (arc.deleted || arc.contracted)) ||
			(arc.in_tree && !arc.in_minor && !crossoverStage) ||
			(arc.forest_candidate && (!inspectingForest || !arc.in_minor)) ||
			(arc.active_cycle_sign !== "0" && !arc.in_minor) ||
			(initialized && flow === 0n) ||
			(initialized && !crossoverStage && slack === 0n) ||
			(arc.resistance !== undefined &&
				(!arc.in_minor || BigInt(arc.resistance) <= 0n)) ||
			(resistanceAvailable && arc.in_minor && arc.resistance === undefined)
		) {
			throw new Error("Primal-dual IPM auxiliary arc is inconsistent");
		}
		auxiliaryArcIds.add(arc.auxiliary_id);
		if (arc.in_minor) {
			projectedProxyGap += flow * slack;
			const deviation = flow * slack - BigInt(overlay.mu);
			projectedCentrality += deviation < 0n ? -deviation : deviation;
		}
		const sign = Number(arc.active_cycle_sign);
		const fromComponent = componentByAuxiliary.get(arc.from) as string;
		const toComponent = componentByAuxiliary.get(arc.to) as string;
		cycleDivergence.set(
			fromComponent,
			(cycleDivergence.get(fromComponent) ?? 0) + sign,
		);
		cycleDivergence.set(
			toComponent,
			(cycleDivergence.get(toComponent) ?? 0) - sign,
		);
	}
	const invalidCycle = [...cycleDivergence.values()].some(
		(value) => value !== 0,
	);
	const invalidSample =
		overlay.sampled_arc !== undefined &&
		!auxiliaryArcIds.has(overlay.sampled_arc);
	if (
		(!crossoverStage &&
			(projectedProxyGap !== BigInt(overlay.proxy_gap) ||
				projectedCentrality !== BigInt(overlay.centrality_numerator))) ||
		invalidCycle ||
		invalidSample
	) {
		throw new Error("Primal-dual IPM aggregate state is inconsistent");
	}
	if (overlay.tree_condition_number !== undefined) {
		let left = BigInt(overlay.tree_condition_number.numerator);
		let right = BigInt(overlay.tree_condition_number.denominator);
		while (right !== 0n) {
			const remainder = left % right;
			left = right;
			right = remainder;
		}
		if (left !== 1n) {
			throw new Error("Primal-dual IPM tree condition number is not reduced");
		}
	}

	const terminal = [
		"recover-admissible-flow",
		"check-certificate",
		"optimal",
	].includes(overlay.stage);
	const genericFlowById = new Map(
		edgeStates.map((state) => [state.edge_id, state.flow]),
	);
	if (
		edgeStates.length !== edges.length ||
		edges.some((edge) => {
			const flow = genericFlowById.get(edge.id);
			return (
				flow === undefined ||
				(terminal
					? BigInt(flow) < BigInt(edge.lower) ||
						BigInt(flow) > BigInt(edge.capacity)
					: flow !== edge.lower)
			);
		})
	) {
		throw new Error("Primal-dual IPM original-flow projection is inconsistent");
	}
	if (terminal) {
		const required = projectLinearMcfRequiredDivergence(model, nodes);
		const divergence = new Map(nodeOrder.map((node) => [node, 0n]));
		let totalCost = 0n;
		for (const edge of edges) {
			const flow = BigInt(genericFlowById.get(edge.id) as string);
			divergence.set(edge.from, (divergence.get(edge.from) ?? 0n) + flow);
			divergence.set(edge.to, (divergence.get(edge.to) ?? 0n) - flow);
			totalCost += flow * BigInt(edge.cost);
		}
		for (const node of nodes) {
			if (divergence.get(node.id) !== required.get(node.id)) {
				throw new Error("Primal-dual IPM recovered flow violates balance");
			}
		}
		if (
			overlay.stage === "optimal" &&
			(outcome?.kind !== "min-cost-flow" ||
				BigInt(outcome.total_cost) !== totalCost)
		) {
			throw new Error("Primal-dual IPM optimum lacks its exact objective");
		}
	} else if (outcome !== undefined) {
		throw new Error("Primal-dual IPM outcome appears before recovery");
	}
	if (overlay.stage !== "optimal" && outcome !== undefined) {
		throw new Error("Primal-dual IPM outcome appears before certification");
	}
	const expectedStatus =
		overlay.stage === "ready"
			? "ready"
			: overlay.stage === "optimal"
				? "optimal"
				: "running";
	if (
		solveStatus !== expectedStatus ||
		(overlay.stage === "ready" && eventId !== "0")
	) {
		throw new Error("Primal-dual IPM stage and solve status disagree");
	}
	const catalog: Partial<
		Record<FlowPrimalDualIpmMcfOverlayV1["stage"], string>
	> = {
		"normalize-input": "primal-dual-interior-point-mcf.normalize-input",
		"build-capacity-reduction":
			"primal-dual-interior-point-mcf.build-capacity-reduction",
		"initialize-central-point":
			"primal-dual-interior-point-mcf.initialize-central-point",
		"build-minor": "primal-dual-interior-point-mcf.build-minor",
		"decrease-mu": "primal-dual-interior-point-mcf.decrease-mu",
		"inspect-forest-subset":
			"primal-dual-interior-point-mcf.inspect-forest-subset",
		"build-low-stretch-forest":
			"primal-dual-interior-point-mcf.build-low-stretch-forest",
		"sample-fundamental-cycle":
			"primal-dual-interior-point-mcf.sample-fundamental-cycle",
		"centering-cycle-update":
			"primal-dual-interior-point-mcf.centering-cycle-update",
		centered: "primal-dual-interior-point-mcf.centered",
		"proxy-reached": "primal-dual-interior-point-mcf.proxy-reached",
		"crossover-grow-cut": "primal-dual-interior-point-mcf.crossover-grow-cut",
		"restore-original-dual":
			"primal-dual-interior-point-mcf.restore-original-dual",
		"recover-admissible-flow":
			"primal-dual-interior-point-mcf.recover-admissible-flow",
		"check-certificate": "primal-dual-interior-point-mcf.check-certificate",
		optimal: "primal-dual-interior-point-mcf.optimal",
	};
	if (
		traceEventRequiresStageIdentity(traceEvent) &&
		traceEvent.catalog_id !== catalog[overlay.stage]
	) {
		throw new Error("Primal-dual IPM trace event and stage disagree");
	}
}

function validateDualNetworkSimplexScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowDualNetworkSimplexOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
): void {
	const selected = algorithmId === "dual-network-simplex";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Dual-simplex overlay uses the wrong algorithm");
		}
		return;
	}
	if (model.kind !== "transshipment") {
		throw new Error("Dual network simplex requires transshipment");
	}
	if (eventId === "0" && solveStatus === "ready" && overlay === undefined)
		return;
	if (overlay === undefined) {
		throw new Error("Dual-simplex scene is missing its tree-basis overlay");
	}
	const orderedNodes = canonicalNodeIds(nodes);
	const orderedEdges = canonicalStableIds(edges);
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const nodeStates = new Map(overlay.nodes.map((node) => [node.node_id, node]));
	const edgeStates = new Map(overlay.edges.map((edge) => [edge.edge_id, edge]));
	const cutSet = new Set(overlay.cut_side);
	const canonicalCut = orderedNodes.filter((node) => cutSet.has(node));
	if (
		overlay.nodes.length !== orderedNodes.length ||
		overlay.nodes.some((node, index) => node.node_id !== orderedNodes[index]) ||
		overlay.edges.length !== orderedEdges.length ||
		overlay.edges.some((edge, index) => edge.edge_id !== orderedEdges[index]) ||
		cutSet.size !== overlay.cut_side.length ||
		canonicalCut.some((node, index) => node !== overlay.cut_side[index]) ||
		overlay.nodes.some((node) => node.in_cut !== cutSet.has(node.node_id))
	) {
		throw new Error("Dual-simplex stable identities or cut are inconsistent");
	}
	const initializedCount = overlay.nodes.filter(
		(node) => node.initialized,
	).length;
	if (
		(overlay.stage === "ready" && initializedCount !== 0) ||
		(overlay.stage === "inspect-initial-arc" && initializedCount === 0) ||
		(!["ready", "inspect-initial-arc"].includes(overlay.stage) &&
			initializedCount !== nodes.length)
	) {
		throw new Error("Dual-simplex tentative prices are inconsistent");
	}
	let treeCount = 0;
	const treeAdjacency = new Map(
		orderedNodes.map((node) => [node, [] as string[]]),
	);
	for (const edge of edges) {
		const state = edgeStates.get(edge.id);
		const from = nodeStates.get(edge.from);
		const to = nodeStates.get(edge.to);
		if (state === undefined || from === undefined || to === undefined) {
			throw new Error("Dual-simplex edge projection is incomplete");
		}
		const reduced =
			BigInt(edge.cost) + BigInt(from.potential) - BigInt(to.potential);
		if (
			state.reduced_cost !== reduced.toString() ||
			(!state.in_tree && state.basic_flow !== "0") ||
			(!["ready", "inspect-initial-arc"].includes(overlay.stage) &&
				(reduced < 0n || (state.in_tree && reduced !== 0n)))
		) {
			throw new Error("Dual-simplex basic flow or dual slack is inconsistent");
		}
		if (state.in_tree) {
			treeCount += 1;
			treeAdjacency.get(edge.from)?.push(edge.to);
			treeAdjacency.get(edge.to)?.push(edge.from);
		}
	}
	const expectedTreeCount = overlay.stage === "ready" ? 0 : nodes.length - 1;
	if (
		(overlay.stage === "inspect-initial-arc" && treeCount >= nodes.length) ||
		(overlay.stage !== "inspect-initial-arc" && treeCount !== expectedTreeCount)
	) {
		throw new Error("Dual-simplex basis has the wrong cardinality");
	}
	if (!["ready", "inspect-initial-arc"].includes(overlay.stage)) {
		const reached = new Set<string>();
		const queue = [orderedNodes[0] ?? ""];
		while (queue.length > 0) {
			const node = queue.shift();
			if (node === undefined || reached.has(node)) continue;
			reached.add(node);
			queue.push(...(treeAdjacency.get(node) ?? []));
		}
		if (reached.size !== orderedNodes.length) {
			throw new Error("Dual-simplex basis is not a spanning tree");
		}
	}
	const leaving =
		overlay.leaving_edge === undefined
			? undefined
			: edgeStates.get(overlay.leaving_edge);
	const entering =
		overlay.entering_edge === undefined
			? undefined
			: edgeStates.get(overlay.entering_edge);
	const inspected =
		overlay.inspected_edge === undefined
			? undefined
			: edgeStates.get(overlay.inspected_edge);
	if (
		(leaving === undefined) !== (overlay.leaving_edge === undefined) ||
		(entering === undefined) !== (overlay.entering_edge === undefined) ||
		(inspected === undefined) !== (overlay.inspected_edge === undefined) ||
		(overlay.pivot_price_delta !== undefined &&
			BigInt(overlay.pivot_price_delta) < 0n)
	) {
		throw new Error("Dual-simplex selected edge is unknown");
	}
	const noSelection =
		overlay.cut_side.length === 0 &&
		leaving === undefined &&
		entering === undefined &&
		inspected === undefined &&
		overlay.pivot_price_delta === undefined;
	const validSelection =
		overlay.stage === "ready" ||
		overlay.stage === "initialize-dual-tree" ||
		overlay.stage === "optimal"
			? noSelection
			: overlay.stage === "inspect-initial-arc"
				? overlay.cut_side.length === 0 &&
					leaving === undefined &&
					entering === undefined &&
					inspected !== undefined &&
					overlay.pivot_price_delta === undefined
				: overlay.stage === "select-leaving"
					? overlay.cut_side.length > 0 &&
						leaving?.in_tree === true &&
						BigInt(leaving.basic_flow) < 0n &&
						entering === undefined &&
						inspected === undefined &&
						overlay.pivot_price_delta === undefined
					: overlay.stage === "inspect-entering-arc"
						? overlay.cut_side.length > 0 &&
							leaving?.in_tree === true &&
							inspected !== undefined &&
							(entering === undefined || entering.in_tree === false) &&
							(entering === undefined) ===
								(overlay.pivot_price_delta === undefined)
						: overlay.stage === "select-entering"
							? overlay.cut_side.length > 0 &&
								leaving?.in_tree === true &&
								entering?.in_tree === false &&
								inspected === undefined &&
								overlay.pivot_price_delta === entering.reduced_cost
							: overlay.cut_side.length > 0 &&
								leaving?.in_tree === false &&
								entering?.in_tree === true &&
								inspected === undefined &&
								entering.reduced_cost === "0" &&
								overlay.pivot_price_delta !== undefined;
	if (
		!validSelection ||
		(solveStatus === "optimal") !== (overlay.stage === "optimal")
	) {
		throw new Error("Dual-simplex pivot boundary metadata is inconsistent");
	}
	if (overlay.cut_side.length > 0) {
		const leavingEdge = edgeById.get(overlay.leaving_edge ?? "");
		const enteringEdge = edgeById.get(overlay.entering_edge ?? "");
		if (
			leavingEdge === undefined ||
			!cutSet.has(leavingEdge.to) ||
			cutSet.has(leavingEdge.from) ||
			(enteringEdge !== undefined &&
				(!cutSet.has(enteringEdge.from) || cutSet.has(enteringEdge.to)))
		) {
			throw new Error("Dual-simplex head-side cut orientation is inconsistent");
		}
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const expectedCatalog = `dual-network-simplex.${overlay.stage}`;
		if (traceEvent.catalog_id !== expectedCatalog) {
			throw new Error("Dual-simplex event and stage disagree");
		}
	}
}

function validatePolynomialDualSimplexScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowPolynomialDualSimplexOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
): void {
	const selected = algorithmId === "polynomial-dual-network-simplex";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Polynomial-dual overlay uses the wrong algorithm");
		}
		return;
	}
	if (model.kind !== "transshipment") {
		throw new Error("Polynomial dual network simplex requires transshipment");
	}
	if (eventId === "0" && solveStatus === "ready" && overlay === undefined)
		return;
	if (overlay === undefined) {
		throw new Error("Polynomial-dual scene is missing its scaling overlay");
	}
	const orderedNodes = canonicalNodeIds(nodes);
	const orderedEdges = canonicalStableIds(edges);
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const nodeStates = new Map(overlay.nodes.map((node) => [node.node_id, node]));
	const edgeStates = new Map(overlay.edges.map((edge) => [edge.edge_id, edge]));
	const badEdges = new Set(overlay.bad_edges);
	const badNodes = new Set(overlay.bad_nodes);
	const pivotCut = new Set(overlay.pivot_cut);
	const preTree =
		overlay.stage === "ready" || overlay.stage === "inspect-initial-arc";
	if (
		overlay.nodes.length !== orderedNodes.length ||
		overlay.nodes.some((node, index) => node.node_id !== orderedNodes[index]) ||
		overlay.edges.length !== orderedEdges.length ||
		overlay.edges.some((edge, index) => edge.edge_id !== orderedEdges[index]) ||
		badEdges.size !== overlay.bad_edges.length ||
		badNodes.size !== overlay.bad_nodes.length ||
		pivotCut.size !== overlay.pivot_cut.length ||
		[...badEdges].some((edge) => !edgeStates.has(edge)) ||
		[...badNodes, ...pivotCut].some((node) => !nodeStates.has(node)) ||
		overlay.nodes.filter((node) => node.root).length !== 1 ||
		overlay.nodes[0]?.root !== true ||
		overlay.nodes.filter((node) => node.active).length !==
			Number(overlay.active_node !== undefined) ||
		overlay.nodes.some(
			(node) =>
				node.active !== (node.node_id === overlay.active_node) ||
				node.bad !== badNodes.has(node.node_id) ||
				node.in_pivot_cut !== pivotCut.has(node.node_id),
		)
	) {
		throw new Error("Polynomial-dual stable identities are inconsistent");
	}
	const treeAdjacency = new Map(
		orderedNodes.map((node) => [node, [] as { node: string; edge: string }[]]),
	);
	let treeCount = 0;
	for (const edge of edges) {
		const state = edgeStates.get(edge.id);
		const from = nodeStates.get(edge.from);
		const to = nodeStates.get(edge.to);
		if (state === undefined || from === undefined || to === undefined) {
			throw new Error("Polynomial-dual edge projection is incomplete");
		}
		const pseudoflow = BigInt(state.pseudoflow.numerator);
		const reduced =
			BigInt(edge.cost) + BigInt(from.potential) - BigInt(to.potential);
		if (
			pseudoflow < 0n ||
			state.reduced_cost !== reduced.toString() ||
			state.bad !== badEdges.has(edge.id) ||
			(!state.in_tree &&
				(pseudoflow !== 0n || BigInt(state.basic_flow) !== 0n)) ||
			(!preTree && (reduced < 0n || (state.in_tree && reduced !== 0n)))
		) {
			throw new Error(
				"Polynomial-dual tree flow or dual slack is inconsistent",
			);
		}
		if (state.in_tree) {
			treeCount += 1;
			treeAdjacency.get(edge.from)?.push({ node: edge.to, edge: edge.id });
			treeAdjacency.get(edge.to)?.push({ node: edge.from, edge: edge.id });
		}
	}
	if (
		(preTree && treeCount !== 0) ||
		(!preTree && treeCount !== nodes.length - 1)
	) {
		throw new Error("Polynomial-dual tree has the wrong cardinality");
	}
	const parent = new Map<string, string>();
	const parentEdge = new Map<string, string>();
	if (!preTree) {
		const root = orderedNodes[0] ?? "";
		parent.set(root, root);
		const queue = [root];
		while (queue.length > 0) {
			const node = queue.shift();
			if (node === undefined) continue;
			for (const adjacent of treeAdjacency.get(node) ?? []) {
				if (parent.has(adjacent.node)) continue;
				parent.set(adjacent.node, node);
				parentEdge.set(adjacent.node, adjacent.edge);
				queue.push(adjacent.node);
			}
		}
		if (parent.size !== orderedNodes.length) {
			throw new Error("Polynomial-dual basis is not a spanning tree");
		}
		const expectedBadEdges = new Set<string>();
		for (const node of orderedNodes.slice(1)) {
			const edgeId = parentEdge.get(node);
			const parentNode = parent.get(node);
			const edge = edges.find((candidate) => candidate.id === edgeId);
			const state = edgeId === undefined ? undefined : edgeStates.get(edgeId);
			if (
				edge !== undefined &&
				state !== undefined &&
				edge.from === parentNode &&
				edge.to === node &&
				BigInt(state.pseudoflow.numerator) === 0n
			) {
				expectedBadEdges.add(edge.id);
			}
		}
		const expectedBadNodes = new Set<string>();
		for (const node of orderedNodes) {
			let cursor = node;
			while (parent.get(cursor) !== cursor) {
				const edge = parentEdge.get(cursor);
				if (edge === undefined) break;
				if (expectedBadEdges.has(edge)) {
					expectedBadNodes.add(node);
					break;
				}
				cursor = parent.get(cursor) ?? cursor;
			}
		}
		if (
			expectedBadEdges.size !== badEdges.size ||
			[...expectedBadEdges].some((edge) => !badEdges.has(edge)) ||
			expectedBadNodes.size !== badNodes.size ||
			[...expectedBadNodes].some((node) => !badNodes.has(node))
		) {
			throw new Error("Polynomial-dual bad subtree projection is inconsistent");
		}
	}
	const activeStage =
		overlay.stage === "select-active" || overlay.stage === "augment-to-root";
	const inspectionStage =
		overlay.stage === "inspect-augmentation-arc" ||
		overlay.stage === "inspect-entering-arc";
	const hasActive = overlay.active_node !== undefined;
	const pathByEdge = new Map(
		overlay.augment_path.map((arc) => [arc.edge_id, arc.direction]),
	);
	if (
		(!inspectionStage && activeStage !== hasActive) ||
		hasActive !== overlay.augment_path.length > 0 ||
		pathByEdge.size !== overlay.augment_path.length ||
		overlay.augment_path.some(
			(arc) => edgeStates.get(arc.edge_id)?.in_tree !== true,
		) ||
		overlay.edges.some(
			(edge) =>
				edge.in_augment_path !== pathByEdge.has(edge.edge_id) ||
				edge.augment_direction !== pathByEdge.get(edge.edge_id),
		)
	) {
		throw new Error("Polynomial-dual active path grammar is inconsistent");
	}
	if (hasActive) {
		let cursor = overlay.active_node;
		for (const reference of overlay.augment_path) {
			const edge = edgeById.get(reference.edge_id);
			if (edge === undefined || cursor === undefined) {
				throw new Error("Polynomial-dual active path edge is absent");
			}
			if (reference.direction === "forward" && edge.from === cursor) {
				cursor = edge.to;
			} else if (reference.direction === "reverse" && edge.to === cursor) {
				cursor = edge.from;
			} else {
				throw new Error(
					"Polynomial-dual active path direction is inconsistent",
				);
			}
		}
		if (cursor !== orderedNodes[0]) {
			throw new Error("Polynomial-dual active path does not reach the root");
		}
	}
	const leaving =
		overlay.leaving_edge === undefined
			? undefined
			: edgeStates.get(overlay.leaving_edge);
	const entering =
		overlay.entering_edge === undefined
			? undefined
			: edgeStates.get(overlay.entering_edge);
	const stageHasLeaving = [
		"select-bad-arc",
		"select-entering",
		"pivot-make-good",
	].includes(overlay.stage);
	const stageHasEntering = ["select-entering", "pivot-make-good"].includes(
		overlay.stage,
	);
	const hasLeaving = leaving !== undefined;
	const hasEntering = entering !== undefined;
	if (
		(!inspectionStage && stageHasLeaving !== hasLeaving) ||
		(!inspectionStage && stageHasEntering !== hasEntering) ||
		(hasEntering && !hasLeaving) ||
		hasLeaving !== overlay.pivot_cut.length > 0 ||
		hasEntering !== (overlay.pivot_price_delta !== undefined) ||
		(overlay.pivot_price_delta !== undefined &&
			BigInt(overlay.pivot_price_delta) < 0n) ||
		(["select-bad-arc", "select-entering"].includes(overlay.stage) &&
			(leaving?.in_tree !== true || entering?.in_tree === true)) ||
		(overlay.stage === "pivot-make-good" &&
			(leaving?.in_tree !== false || entering?.in_tree !== true)) ||
		(solveStatus === "optimal") !== (overlay.stage === "optimal")
	) {
		throw new Error("Polynomial-dual Make-Good selection is inconsistent");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const suffix = {
			ready: "ready",
			"inspect-initial-arc": "inspect-initial-arc",
			"initialize-tree": "initialize-dual-tree",
			"initialize-pseudoflow": "initialize-pseudoflow",
			"begin-scale": "begin-delta-scale",
			"inspect-augmentation-arc": "inspect-augmentation-arc",
			"select-active": "select-active-node",
			"augment-to-root": "augment-to-root",
			"select-bad-arc": "select-bad-subtree",
			"inspect-entering-arc": "inspect-entering-arc",
			"select-entering": "select-entering-arc",
			"pivot-make-good": "pivot-make-good",
			"finish-scale": "finish-delta-scale",
			optimal: "optimal",
		}[overlay.stage];
		if (traceEvent.catalog_id !== `polynomial-dual-network-simplex.${suffix}`) {
			throw new Error("Polynomial-dual event and stage disagree");
		}
	}
}

function validatePolynomialPrimalSelection(
	overlay: FlowPolynomialPrimalSimplexOverlayV1,
	entityIds: Set<string>,
): void {
	const validateReference = (
		reference: FlowPolynomialPrimalResidualRefV1,
	): boolean =>
		entityIds.has(reference.entity_id) &&
		(reference.entity_id.startsWith("artificial:")
			? reference.original_edge_id === undefined
			: reference.original_edge_id === reference.entity_id);
	if (
		(overlay.entering !== undefined && !validateReference(overlay.entering)) ||
		overlay.cycle.some((reference) => !validateReference(reference)) ||
		(overlay.leaving_entity !== undefined &&
			!entityIds.has(overlay.leaving_entity))
	) {
		throw new Error("Polynomial-simplex selected arc identity is invalid");
	}
	const selection =
		overlay.stage === "select-admissible" || overlay.stage === "pivot";
	if (
		selection !==
			(overlay.entering !== undefined && overlay.cycle.length > 0) ||
		(overlay.stage === "pivot") !== (overlay.delta !== undefined) ||
		(overlay.stage === "modify-premultipliers") !==
			(overlay.potential_shift !== undefined) ||
		(!selection && overlay.leaving_entity !== undefined)
	) {
		throw new Error("Polynomial-simplex selection grammar is invalid");
	}
	const cycle = new Set(overlay.cycle.map((reference) => reference.entity_id));
	for (const state of [...overlay.edges, ...overlay.artificial_edges]) {
		const entityId = "edge_id" in state ? state.edge_id : state.entity_id;
		if (
			state.in_cycle !== cycle.has(entityId) ||
			state.entering !== (overlay.entering?.entity_id === entityId) ||
			state.leaving !== (overlay.leaving_entity === entityId)
		) {
			throw new Error("Polynomial-simplex edge flags disagree with selection");
		}
	}
}

function polynomialReducedCostMatches(
	edge: FlowEdgeV1,
	state: FlowPolynomialPrimalEdgeStateV1,
	potentialByNode: Map<string, FlowRationalV1>,
): boolean {
	const from = potentialByNode.get(edge.from);
	const to = potentialByNode.get(edge.to);
	if (from === undefined || to === undefined) return false;
	const fromDenominator = BigInt(from.denominator);
	const toDenominator = BigInt(to.denominator);
	const common = fromDenominator * toDenominator;
	const expectedNumerator =
		BigInt(edge.cost) * common -
		BigInt(from.numerator) * toDenominator +
		BigInt(to.numerator) * fromDenominator;
	return (
		BigInt(state.reduced_cost.numerator) * common ===
		expectedNumerator * BigInt(state.reduced_cost.denominator)
	);
}

function validatePolynomialPrimalSimplexScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowPolynomialPrimalSimplexOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
): void {
	const selected = algorithmId === "polynomial-primal-network-simplex";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Polynomial-simplex overlay uses the wrong algorithm");
		}
		return;
	}
	if (
		model.kind !== "fixed-flow-min-cost" &&
		model.kind !== "min-cost-max-flow" &&
		model.kind !== "circulation" &&
		model.kind !== "transshipment"
	) {
		throw new Error("Polynomial primal simplex requires linear min-cost flow");
	}
	if (eventId === "0" && solveStatus === "ready" && overlay === undefined)
		return;
	if (overlay === undefined) {
		throw new Error("Polynomial-simplex scene is missing its exact overlay");
	}
	const orderedNodes = canonicalNodeIds(nodes);
	const orderedEdges = canonicalStableIds(edges);
	const expectedNodeIds = [...orderedNodes, "artificial-root"];
	if (
		overlay.perturbation_scale !== String(nodes.length + 1) ||
		overlay.nodes.length !== expectedNodeIds.length ||
		overlay.nodes.some(
			(state, index) =>
				state.entity_id !== expectedNodeIds[index] ||
				state.kind !==
					(index === orderedNodes.length ? "artificial-root" : "original") ||
				new Set(state.flags).size !== state.flags.length,
		) ||
		overlay.edges.length !== orderedEdges.length ||
		overlay.edges.some(
			(state, index) => state.edge_id !== orderedEdges[index],
		) ||
		overlay.artificial_edges.length !== orderedNodes.length ||
		overlay.artificial_edges.some(
			(state, index) =>
				state.node_id !== orderedNodes[index] ||
				state.entity_id !== `artificial:${orderedNodes[index]}`,
		)
	) {
		throw new Error("Polynomial-simplex extended identities are inconsistent");
	}
	const rootCount = overlay.nodes.filter((state) =>
		state.flags.includes("root"),
	).length;
	const activeScale =
		overlay.stage === "begin-scale" ||
		overlay.stage === "select-admissible" ||
		overlay.stage === "pivot" ||
		overlay.stage === "modify-premultipliers" ||
		overlay.stage === "finish-scale";
	if (
		rootCount !== 1 ||
		(activeScale && overlay.epsilon === undefined) ||
		((overlay.stage === "ready" || overlay.stage === "initialize-basis") &&
			overlay.epsilon !== undefined)
	) {
		throw new Error("Polynomial-simplex scale or root state is inconsistent");
	}
	const nodeStateById = new Map(
		overlay.nodes.map((state) => [state.entity_id, state]),
	);
	const potentialByNode = new Map(
		overlay.nodes.map((state) => [state.entity_id, state.premultiplier]),
	);
	const edgeStateById = new Map(
		overlay.edges.map((state) => [state.edge_id, state]),
	);
	const adjacency = new Map(
		expectedNodeIds.map((node) => [node, [] as string[]]),
	);
	let treeCount = 0;
	const scale = BigInt(overlay.perturbation_scale);
	for (const edge of edges) {
		const state = edgeStateById.get(edge.id);
		if (
			state === undefined ||
			!polynomialReducedCostMatches(edge, state, potentialByNode)
		) {
			throw new Error("Polynomial-simplex reduced cost is inconsistent");
		}
		const flow = BigInt(state.perturbed_flow);
		const capacity = (BigInt(edge.capacity) - BigInt(edge.lower)) * scale;
		const validBasis =
			state.basis === "lower"
				? flow === 0n
				: state.basis === "upper"
					? flow === capacity
					: flow > 0n && flow < capacity;
		if (!validBasis) {
			throw new Error("Polynomial-simplex original basis bound is invalid");
		}
		if (state.basis === "tree") {
			treeCount += 1;
			adjacency.get(edge.from)?.push(edge.to);
			adjacency.get(edge.to)?.push(edge.from);
		}
	}
	for (const state of overlay.artificial_edges) {
		const flow = BigInt(state.perturbed_flow);
		if ((state.basis === "lower" && flow !== 0n) || flow < 0n) {
			throw new Error("Polynomial-simplex artificial basis bound is invalid");
		}
		if (state.basis === "tree") {
			treeCount += 1;
			adjacency.get(state.node_id)?.push("artificial-root");
			adjacency.get("artificial-root")?.push(state.node_id);
		}
	}
	if (treeCount !== nodes.length) {
		throw new Error("Polynomial-simplex basis has the wrong cardinality");
	}
	const reached = new Set<string>();
	const queue = ["artificial-root"];
	while (queue.length > 0) {
		const node = queue.shift();
		if (node === undefined || reached.has(node)) continue;
		reached.add(node);
		queue.push(...(adjacency.get(node) ?? []));
	}
	if (
		reached.size !== expectedNodeIds.length ||
		nodeStateById.size !== expectedNodeIds.length
	) {
		throw new Error(
			"Polynomial-simplex basis is not an extended spanning tree",
		);
	}
	const entityIds = new Set([
		...orderedEdges,
		...orderedNodes.map((node) => `artificial:${node}`),
	]);
	validatePolynomialPrimalSelection(overlay, entityIds);
	if ((solveStatus === "optimal") !== (overlay.stage === "optimal")) {
		throw new Error("Polynomial-simplex optimal status is inconsistent");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const suffix = {
			"initialize-basis": "initialize-perturbed-basis",
			"begin-scale": "begin-epsilon-scale",
			"inspect-residual": "inspect-extended-arc",
			"select-admissible": "select-admissible-arc",
			pivot: "pivot-fundamental-cycle",
			"modify-premultipliers": "modify-epsilon-premultipliers",
			"finish-scale": "finish-epsilon-scale",
			optimal: "optimal",
			ready: "ready",
		}[overlay.stage];
		if (
			traceEvent.catalog_id !== `polynomial-primal-network-simplex.${suffix}`
		) {
			throw new Error("Polynomial-simplex event and stage disagree");
		}
	}
}

function validateDoubleScalingScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowDoubleScalingOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
): void {
	const selected = algorithmId === "double-scaling";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Double-scaling overlay uses the wrong algorithm");
		}
		return;
	}
	if (
		model.kind !== "fixed-flow-min-cost" &&
		model.kind !== "circulation" &&
		model.kind !== "transshipment"
	) {
		throw new Error("Double scaling requires a minimum-cost-flow model");
	}
	if (eventId === "0" && solveStatus === "ready" && overlay === undefined)
		return;
	if (overlay === undefined) {
		throw new Error("Double-scaling scene is missing its exact overlay");
	}
	const orderedNodes = canonicalNodeIds(nodes);
	const variableEdges = edges.filter(
		(edge) => BigInt(edge.capacity) > BigInt(edge.lower),
	);
	const expectedNodeIdentities = [
		...orderedNodes.map((entity_id) => ({ entity_id, kind: "original" })),
		...variableEdges.map((edge) => ({ entity_id: edge.id, kind: "edge" })),
	];
	if (
		overlay.nodes.length !== expectedNodeIdentities.length ||
		overlay.nodes.some(
			(node, index) =>
				node.entity_id !== expectedNodeIdentities[index]?.entity_id ||
				node.kind !== expectedNodeIdentities[index]?.kind,
		) ||
		overlay.edges.length !== variableEdges.length ||
		overlay.edges.some(
			(edge, index) => edge.edge_id !== variableEdges[index]?.id,
		) ||
		((overlay.stage === "ready" || overlay.stage === "initialize") &&
			overlay.cost_phase !== "0") ||
		(!["ready", "initialize"].includes(overlay.stage) &&
			overlay.cost_phase === "0") ||
		(overlay.stage === "start-cost-phase" && overlay.capacity_phase !== "0") ||
		([
			"ready",
			"initialize",
			"start-cost-phase",
			"complete-cost-phase",
			"optimal",
		].includes(overlay.stage)
			? overlay.delta !== "0"
			: overlay.delta === "0") ||
		(solveStatus === "optimal") !== (overlay.stage === "optimal")
	) {
		throw new Error("Double-scaling boundary metadata is inconsistent");
	}
	const activeStages = new Set([
		"select-root",
		"inspect-arc",
		"advance",
		"relabel",
		"retreat",
		"augment",
	]);
	if (
		activeStages.has(overlay.stage) !== (overlay.selected_root !== undefined) ||
		(overlay.stage === "augment") !==
			(overlay.selected_deficit !== undefined) ||
		(overlay.stage === "inspect-arc") !==
			(overlay.inspected_arc !== undefined) ||
		(overlay.stage === "augment" && overlay.active_path.length === 0) ||
		(!activeStages.has(overlay.stage) && overlay.active_path.length !== 0)
	) {
		throw new Error("Double-scaling path selection metadata is inconsistent");
	}
	const knownNodeRefs = new Set([
		...orderedNodes.map((id) => `node:${id}`),
		...variableEdges.map((edge) => `edge:${edge.id}`),
	]);
	if (
		[overlay.selected_root, overlay.selected_deficit].some(
			(selected) => selected !== undefined && !knownNodeRefs.has(selected),
		)
	) {
		throw new Error("Double-scaling selected node does not exist");
	}
	const prices = new Map(
		overlay.nodes.map((node) => [
			`${node.kind}:${node.entity_id}`,
			BigInt(node.price),
		]),
	);
	const branchByEdge = new Map(
		overlay.edges.map((edge) => [edge.edge_id, edge]),
	);
	const costMultiplier = BigInt(overlay.cost_multiplier);
	const epsilon = BigInt(overlay.epsilon);
	const expectedAdmissible: string[] = [];
	const knownArcKeys = new Set<string>();
	for (const edge of variableEdges) {
		const branchState = branchByEdge.get(edge.id);
		const rightPrice = prices.get(`edge:${edge.id}`);
		const fromPrice = prices.get(`original:${edge.from}`);
		const toPrice = prices.get(`original:${edge.to}`);
		if (
			branchState === undefined ||
			rightPrice === undefined ||
			fromPrice === undefined ||
			toPrice === undefined
		) {
			throw new Error("Double-scaling transformed price is incomplete");
		}
		const width = BigInt(edge.capacity) - BigInt(edge.lower);
		const flow = BigInt(branchState.flow_branch);
		const slack = BigInt(branchState.slack_branch);
		if (flow < 0n || slack < 0n) {
			throw new Error("Double-scaling transformed flow cannot be negative");
		}
		if (["complete-cost-phase", "optimal"].includes(overlay.stage)) {
			const originalFlow = edgeStates.find(
				(state) => state.edge_id === edge.id,
			)?.flow;
			if (
				flow + slack !== width ||
				originalFlow === undefined ||
				BigInt(originalFlow) !== BigInt(edge.lower) + flow
			) {
				throw new Error(
					"Double-scaling completed flow mapping is inconsistent",
				);
			}
		}
		for (const [branch, leftPrice, cost, branchFlow] of [
			["flow", fromPrice, BigInt(edge.cost) * costMultiplier, flow],
			["slack", toPrice, 0n, slack],
		] as const) {
			const forwardKey = `${edge.id}\u0000${branch}\u0000forward`;
			const reverseKey = `${edge.id}\u0000${branch}\u0000reverse`;
			knownArcKeys.add(forwardKey);
			knownArcKeys.add(reverseKey);
			const forwardReduced = cost + leftPrice - rightPrice;
			const reverseReduced = -cost + rightPrice - leftPrice;
			if (
				forwardReduced < -epsilon ||
				(branchFlow > 0n && reverseReduced < -epsilon)
			) {
				throw new Error("Double-scaling snapshot is not epsilon-optimal");
			}
			if (forwardReduced < 0n) expectedAdmissible.push(forwardKey);
			if (branchFlow > 0n && reverseReduced < 0n)
				expectedAdmissible.push(reverseKey);
		}
	}
	const arcKey = (arc: FlowDoubleScalingArcRefV1) =>
		`${arc.edge_id}\u0000${arc.branch}\u0000${arc.direction}`;
	const admissibleKeys = overlay.admissible_arcs.map(arcKey).sort();
	expectedAdmissible.sort();
	const pathKeys = overlay.active_path.map(arcKey);
	const inspectedKey =
		overlay.inspected_arc === undefined
			? undefined
			: arcKey(overlay.inspected_arc);
	if (
		new Set(admissibleKeys).size !== admissibleKeys.length ||
		JSON.stringify(admissibleKeys) !== JSON.stringify(expectedAdmissible) ||
		new Set(pathKeys).size !== pathKeys.length ||
		pathKeys.some((key) => !knownArcKeys.has(key)) ||
		(inspectedKey !== undefined && !knownArcKeys.has(inspectedKey))
	) {
		throw new Error("Double-scaling transformed arc set is incorrect");
	}
	const imbalanceSum = overlay.nodes.reduce(
		(sum, node) => sum + BigInt(node.imbalance),
		0n,
	);
	if (
		imbalanceSum !== 0n ||
		(["complete-cost-phase", "optimal"].includes(overlay.stage) &&
			overlay.nodes.some((node) => node.imbalance !== "0"))
	) {
		throw new Error("Double-scaling imbalance vector is inconsistent");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const expectedCatalog = `double-scaling.${
			{
				ready: "invalid-ready",
				initialize: "initialize-transportation",
				"start-cost-phase": "start-cost-phase",
				"start-capacity-phase": "start-capacity-phase",
				"select-root": "select-large-excess-root",
				"inspect-arc": "inspect-transformed-residual-arc",
				advance: "advance-admissible-path",
				relabel: "relabel-dead-end-tip",
				retreat: "retreat-inadmissible-predecessor",
				augment: "augment-exact-delta",
				"complete-cost-phase": "complete-cost-phase",
				optimal: "optimal",
			}[overlay.stage]
		}`;
		if (traceEvent.catalog_id !== expectedCatalog) {
			throw new Error("Double-scaling event and stage disagree");
		}
	}
}

function validateConvexCostScene(
	model: FlowProblemModelV1,
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowConvexCostOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const expanded = algorithmId === "segment-expanded-convex-mcf";
	const scaling = algorithmId === "convex-cost-scaling";
	const simplex = algorithmId === "convex-network-simplex";
	const selected = expanded || scaling || simplex;
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Convex-cost overlay uses the wrong algorithm");
		}
		return;
	}
	if (model.kind !== "convex-cost-flow") {
		throw new Error("Convex-cost algorithms require a convex-cost model");
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready") ||
			solveStatus === "resource-limit" ||
			solveStatus === "infeasible"
		) {
			return;
		}
		throw new Error("Convex-cost scene is missing its exact overlay");
	}
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const flowByEdge = new Map(
		edgeStates.map((state) => [state.edge_id, BigInt(state.flow)]),
	);
	const expectedOrder = canonicalStableIds(edges);
	const expandedInspection =
		expanded &&
		traceEvent?.catalog_id ===
			"segment-expanded-convex-mcf.inspect-residual-arc";
	const inspectedResidualRefs = expandedInspection
		? traceEvent.entity_refs.filter(
				(reference) => reference.kind === "residual-arc",
			)
		: [];
	const activeStage = expanded
		? ["select-minimum-mean-cycle", "cancel-cycle"].includes(overlay.stage)
		: [
				"saturate-marginal",
				"inspect-marginal-arc",
				"update-potentials",
				"augment",
			].includes(overlay.stage);
	if (
		overlay.edges.length !== edges.length ||
		overlay.edges.some(
			(edge, index) => edge.edge_id !== expectedOrder[index],
		) ||
		(solveStatus === "optimal") !== (overlay.stage === "optimal") ||
		(activeStage
			? overlay.active_cycle.length === 0
			: overlay.stage === "shortest-path"
				? false
				: overlay.active_cycle.length !== 0) ||
		(expanded &&
			(overlay.scale !== undefined ||
				(overlay.eligible_arcs ?? []).length !== 0)) ||
		(scaling && overlay.scale === undefined)
	) {
		throw new Error("Convex-cost boundary metadata is inconsistent");
	}
	let objective = 0n;
	const expectedEligible = new Set<string>();
	const segmentByKey = new Map<
		string,
		{ flow: bigint; length: bigint; lower: bigint; marginal: bigint }
	>();
	for (const state of overlay.edges) {
		const edge = edgeById.get(state.edge_id);
		const aggregateFlow = flowByEdge.get(state.edge_id);
		if (edge === undefined || aggregateFlow === undefined) {
			throw new Error("Convex-cost edge identity is unknown");
		}
		const declared = edge.convex_cost ?? {
			base_cost_at_zero: "0",
			segments:
				BigInt(edge.capacity) === 0n
					? []
					: [{ end_flow: edge.capacity, marginal_cost: edge.cost }],
		};
		if (edge.convex_cost !== undefined && edge.cost !== "0") {
			throw new Error("Convex edge cannot also declare a linear cost");
		}
		if (
			state.base_cost_at_zero !== declared.base_cost_at_zero ||
			state.flow !== aggregateFlow.toString() ||
			state.segments.length !== declared.segments.length
		) {
			throw new Error("Convex-cost edge state disagrees with its declaration");
		}
		let start = 0n;
		let occupied = 0n;
		let edgeCost = BigInt(declared.base_cost_at_zero);
		let previousMarginal: bigint | undefined;
		let partial = false;
		let expectedForward: bigint | undefined;
		let expectedReverse: bigint | undefined;
		for (const [index, segment] of state.segments.entries()) {
			const source = declared.segments[index];
			if (source === undefined) {
				throw new Error("Convex-cost segment declaration is incomplete");
			}
			const end = BigInt(segment.end_flow);
			const flow = BigInt(segment.flow);
			const marginal = BigInt(segment.marginal_cost);
			const length = end - start;
			const lowerOffset = BigInt(edge.lower) - start;
			const lower = lowerOffset > 0n ? lowerOffset : 0n;
			const segmentLower = lower < length ? lower : length;
			const isForwardBoundary = expectedForward === undefined && flow < length;
			if (
				segment.segment !== String(index) ||
				segment.start_flow !== start.toString() ||
				segment.end_flow !== source.end_flow ||
				segment.marginal_cost !== source.marginal_cost ||
				end <= start ||
				flow < segmentLower ||
				flow > length ||
				(partial && flow !== 0n) ||
				(previousMarginal !== undefined && marginal < previousMarginal)
			) {
				throw new Error("Convex-cost segment partition is inconsistent");
			}
			if (flow < length) {
				partial = true;
				if (expectedForward === undefined) expectedForward = marginal;
			}
			if (flow > segmentLower) expectedReverse = marginal;
			occupied += flow;
			edgeCost += flow * marginal;
			segmentByKey.set(`${edge.id}\u0000${index}`, {
				flow,
				length,
				lower: segmentLower,
				marginal,
			});
			if (scaling && overlay.scale !== undefined) {
				const scale = BigInt(overlay.scale);
				if (isForwardBoundary && length - flow >= scale) {
					expectedEligible.add(`${edge.id}\u0000${index}\u0000forward`);
				}
			}
			start = end;
			previousMarginal = marginal;
		}
		if (
			start !== BigInt(edge.capacity) ||
			occupied !== aggregateFlow ||
			state.total_cost !== edgeCost.toString() ||
			state.forward_marginal_cost !== expectedForward?.toString() ||
			state.reverse_marginal_cost !== expectedReverse?.toString()
		) {
			throw new Error("Convex-cost marginal summary is inconsistent");
		}
		objective += edgeCost;
		if (
			scaling &&
			overlay.scale !== undefined &&
			expectedReverse !== undefined
		) {
			const reverseIndex = state.segments.findLastIndex(
				(segment) => BigInt(segment.flow) > 0n,
			);
			if (reverseIndex >= 0) {
				const reverse = segmentByKey.get(`${edge.id}\u0000${reverseIndex}`);
				if (
					reverse !== undefined &&
					reverse.flow - reverse.lower >= BigInt(overlay.scale)
				) {
					expectedEligible.add(`${edge.id}\u0000${reverseIndex}\u0000reverse`);
				}
			}
		}
	}
	const activeKeys = new Set<string>();
	const activeEndpoints: { from: string; to: string }[] = [];
	const flowMutationStage =
		traceEvent !== undefined &&
		["cancel-cycle", "saturate-marginal", "augment"].includes(overlay.stage);
	const changedDelta =
		flowMutationStage &&
		traceEvent?.detail !== undefined &&
		traceDetailKeepsSourceLabel(traceEvent.detail, "delta") &&
		canonicalU64.test(traceEvent.detail.value)
			? BigInt(traceEvent.detail.value)
			: undefined;
	if (flowMutationStage && (changedDelta ?? 0n) <= 0n) {
		throw new Error("Convex-cost flow change is missing its exact delta");
	}
	for (const arc of overlay.active_cycle) {
		const key = `${arc.edge_id}\u0000${arc.segment}`;
		const segment = segmentByKey.get(key);
		const edge = edgeById.get(arc.edge_id);
		const directedKey = `${key}\u0000${arc.direction}`;
		const residualAtBoundary =
			segment !== undefined &&
			(traceEvent === undefined
				? true
				: flowMutationStage
					? arc.direction === "forward"
						? segment.flow >= segment.lower + (changedDelta ?? 0n)
						: segment.flow + (changedDelta ?? 0n) <= segment.length
					: arc.direction === "forward"
						? segment.flow < segment.length
						: segment.flow > segment.lower);
		if (
			edge === undefined ||
			segment === undefined ||
			activeKeys.has(directedKey) ||
			!residualAtBoundary
		) {
			throw new Error("Convex-cost active marginal arc is not residual");
		}
		activeKeys.add(directedKey);
		activeEndpoints.push(
			arc.direction === "forward"
				? { from: edge.from, to: edge.to }
				: { from: edge.to, to: edge.from },
		);
	}
	const disconnectedActiveWalk = activeEndpoints.some(
		(endpoint, index) =>
			index > 0 && activeEndpoints[index - 1]?.to !== endpoint.from,
	);
	const unclosedExpandedCycle =
		traceEvent !== undefined &&
		expanded &&
		activeEndpoints.length > 0 &&
		activeEndpoints.at(-1)?.to !== activeEndpoints[0]?.from &&
		!expandedInspection;
	if (
		disconnectedActiveWalk ||
		unclosedExpandedCycle ||
		(expandedInspection &&
			(activeEndpoints.length !== 1 ||
				!traceDetailKeepsSourceLabel(traceEvent?.detail, "marginal-arc-cost") ||
				!canonicalI128.test(traceEvent?.detail?.value ?? "") ||
				inspectedResidualRefs.length !== 1 ||
				inspectedResidualRefs[0]?.edge_id !==
					overlay.active_cycle[0]?.edge_id ||
				inspectedResidualRefs[0]?.direction !==
					overlay.active_cycle[0]?.direction))
	) {
		throw new Error("Convex-cost active marginal walk is inconsistent");
	}
	const eligibleKeys = new Set(
		(overlay.eligible_arcs ?? []).map(
			(arc) => `${arc.edge_id}\u0000${arc.segment}\u0000${arc.direction}`,
		),
	);
	if (
		eligibleKeys.size !== (overlay.eligible_arcs ?? []).length ||
		eligibleKeys.size !== expectedEligible.size ||
		[...eligibleKeys].some((key) => !expectedEligible.has(key))
	) {
		throw new Error("Convex-cost eligible marginal set is inconsistent");
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const eventMatches = expanded
			? (
					{
						initialize: ["segment-expanded-convex-mcf.start-selector"],
						"select-minimum-mean-cycle": [
							"segment-expanded-convex-mcf.inspect-residual-arc",
							"segment-expanded-convex-mcf.select-minimum-mean-cycle",
						],
						"cancel-cycle": ["segment-expanded-convex-mcf.cancel-cycle"],
						optimal: ["segment-expanded-convex-mcf.optimal"],
					} as Record<string, string[]>
				)[overlay.stage]?.includes(traceEvent.catalog_id)
			: scaling
				? traceEvent.catalog_id ===
					(
						{
							initialize: "convex-cost-scaling.initialize-marginal-residual",
							"start-scale": "convex-cost-scaling.start-delta-scale",
							"saturate-marginal":
								"convex-cost-scaling.saturate-negative-eligible-marginal",
							"inspect-marginal-arc":
								"convex-cost-scaling.inspect-marginal-residual-arc",
							"shortest-path":
								"convex-cost-scaling.shortest-marginal-residual-path",
							"update-potentials":
								"convex-cost-scaling.update-reduced-cost-potentials",
							augment: "convex-cost-scaling.augment-to-breakpoint",
							"complete-scale": "convex-cost-scaling.complete-delta-scale",
							optimal: "convex-cost-scaling.certify-expanded-oracle",
						} as Record<string, string>
					)[overlay.stage]
				: true;
		if (eventMatches === false) {
			throw new Error("Convex-cost event and stage disagree");
		}
	}
	if (outcome?.kind === "min-cost-flow") {
		if (outcome.total_cost !== objective.toString()) {
			throw new Error("Convex-cost native objective is inconsistent");
		}
		const potential = new Map(
			outcome.potentials.map((item) => [item.node_id, BigInt(item.potential)]),
		);
		for (const state of overlay.edges) {
			const edge = edgeById.get(state.edge_id);
			const from = edge === undefined ? undefined : potential.get(edge.from);
			const to = edge === undefined ? undefined : potential.get(edge.to);
			if (edge === undefined || from === undefined || to === undefined) {
				throw new Error("Convex-cost dual potential is incomplete");
			}
			if (
				(state.forward_marginal_cost !== undefined &&
					BigInt(state.forward_marginal_cost) + from - to < 0n) ||
				(state.reverse_marginal_cost !== undefined &&
					-BigInt(state.reverse_marginal_cost) + to - from < 0n)
			) {
				throw new Error("Convex-cost marginal residual dual is infeasible");
			}
		}
	}
}

function validatePredictionAssistedEpsilonScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowPredictionAssistedEpsilonOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
): void {
	const selected = algorithmId === "prediction-assisted-epsilon-relaxation";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Prediction-assisted overlay uses the wrong algorithm");
		}
		return;
	}
	if (
		model.kind !== "fixed-flow-min-cost" &&
		model.kind !== "circulation" &&
		model.kind !== "transshipment"
	) {
		throw new Error("Prediction-assisted epsilon relaxation requires MCF");
	}
	const configKeys = Object.keys(config).sort();
	const configuredPredictions = config.predicted_potentials;
	const configuredScaling = config.scaling_parameter;
	const nodeOrder = canonicalNodeIds(nodes);
	if (
		configKeys.length !== 2 ||
		configKeys[0] !== "predicted_potentials" ||
		configKeys[1] !== "scaling_parameter" ||
		!isRecord(configuredPredictions) ||
		Object.keys(configuredPredictions).length !== nodeOrder.length ||
		typeof configuredScaling !== "number" ||
		!Number.isInteger(configuredScaling) ||
		configuredScaling < 2 ||
		configuredScaling > 4 ||
		nodeOrder.some((node) => {
			const prediction = configuredPredictions[node];
			return typeof prediction !== "string" || !canonicalI128.test(prediction);
		})
	) {
		throw new Error("Prediction-assisted configuration is not closed");
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready") ||
			solveStatus === "resource-limit" ||
			solveStatus === "infeasible"
		) {
			return;
		}
		throw new Error("Prediction-assisted scene is missing its overlay");
	}
	if (
		(solveStatus === "optimal") !== (overlay.stage === "optimal") ||
		overlay.nodes.length !== nodes.length ||
		overlay.edges.length !== edges.length ||
		BigInt(overlay.scaling_parameter) !== BigInt(configuredScaling) ||
		BigInt(overlay.maximum_attempt) < 1n ||
		BigInt(overlay.attempt) > BigInt(overlay.maximum_attempt) ||
		BigInt(overlay.exponent) > BigInt(overlay.maximum_attempt) ||
		overlay.attempt !== overlay.exponent ||
		(overlay.attempt === "0") !== (overlay.stage === "preprocess-prediction") ||
		(overlay.scale_exponent !== undefined &&
			BigInt(overlay.scale_exponent) >= BigInt(overlay.exponent))
	) {
		throw new Error("Prediction-assisted attempt boundary is inconsistent");
	}
	validatePredictionAssistedStageShape(overlay);
	const edgeOrder = canonicalStableIds(edges);
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const flowById = new Map(
		edgeStates.map((state) => [state.edge_id, BigInt(state.flow)]),
	);
	const rawPredictions = nodeOrder.map((node) =>
		BigInt((configuredPredictions as Record<string, unknown>)[node] as string),
	);
	const minimumPrediction = rawPredictions.reduce((left, right) =>
		left < right ? left : right,
	);
	const maximumCost = edges.reduce((maximum, edge) => {
		const cost = BigInt(edge.cost);
		const magnitude = cost < 0n ? -cost : cost;
		return maximum > magnitude ? maximum : magnitude;
	}, 0n);
	const predictionUpper = BigInt(Math.max(0, nodes.length - 1)) * maximumCost;
	const required = projectLinearMcfRequiredDivergence(model, nodes);
	const actual = new Map(nodeOrder.map((node) => [node, 0n]));
	for (const edge of edges) {
		const flow = flowById.get(edge.id);
		if (flow === undefined) {
			throw new Error("Prediction-assisted flow state is incomplete");
		}
		actual.set(edge.from, (actual.get(edge.from) ?? 0n) + flow);
		actual.set(edge.to, (actual.get(edge.to) ?? 0n) - flow);
	}
	const price = new Map<string, bigint>();
	let activeCount = 0;
	for (const [index, state] of overlay.nodes.entries()) {
		const node = nodeOrder[index];
		const rawPrediction = rawPredictions[index];
		if (
			node === undefined ||
			rawPrediction === undefined ||
			state.node_id !== node
		) {
			throw new Error("Prediction-assisted node order is inconsistent");
		}
		const shifted = rawPrediction - minimumPrediction;
		const clipped = shifted > predictionUpper;
		const predicted = clipped ? predictionUpper : shifted;
		if (
			state.raw_predicted_price !==
				(configuredPredictions as Record<string, unknown>)[node] ||
			BigInt(state.predicted_price) !== predicted ||
			state.prediction_clipped !== clipped ||
			BigInt(state.surplus) !==
				(required.get(node) ?? 0n) - (actual.get(node) ?? 0n) ||
			state.active !== (overlay.active_node === node)
		) {
			throw new Error("Prediction-assisted node values are inconsistent");
		}
		price.set(node, BigInt(state.price));
		if (state.active) activeCount += 1;
	}
	if (activeCount !== (overlay.active_node === undefined ? 0 : 1)) {
		throw new Error("Prediction-assisted active node is inconsistent");
	}
	const scaleExponent =
		overlay.scale_exponent === undefined
			? undefined
			: BigInt(overlay.scale_exponent);
	const divisor =
		scaleExponent === undefined
			? 1n
			: BigInt(configuredScaling) ** scaleExponent;
	const scaledCost = new Map<string, bigint>();
	for (const [index, state] of overlay.edges.entries()) {
		const id = edgeOrder[index];
		const edge = id === undefined ? undefined : edgeById.get(id);
		if (edge === undefined || state.edge_id !== id) {
			throw new Error("Prediction-assisted edge order is inconsistent");
		}
		const cost = BigInt(state.scaled_cost);
		if (
			scaleExponent !== undefined &&
			cost !==
				floorBigIntDivision(
					BigInt(edge.cost) * BigInt(nodes.length + 1),
					divisor,
				)
		) {
			throw new Error("Prediction-assisted scaled cost is inconsistent");
		}
		scaledCost.set(id, cost);
	}
	if (
		overlay.stage !== "preprocess-prediction" &&
		overlay.stage !== "begin-attempt"
	) {
		validatePredictionAssistedEpsilonCs(edges, flowById, price, scaledCost);
	}
	if (overlay.stage === "initialize-scale") {
		for (const edge of edges) {
			const flow = flowById.get(edge.id) ?? 0n;
			const reduced =
				(price.get(edge.from) ?? 0n) -
				(price.get(edge.to) ?? 0n) -
				(scaledCost.get(edge.id) ?? 0n);
			if (
				(flow < BigInt(edge.capacity) && reduced === 1n) ||
				(flow > BigInt(edge.lower) && reduced === -1n)
			) {
				throw new Error(
					"Prediction-assisted initial admissible graph is not empty",
				);
			}
		}
	}
	if (overlay.active_arc !== undefined) {
		const edge = edgeById.get(overlay.active_arc.edge_id);
		if (edge === undefined) {
			throw new Error("Prediction-assisted active residual is unknown");
		}
		const reduced =
			(price.get(edge.from) ?? 0n) -
			(price.get(edge.to) ?? 0n) -
			(scaledCost.get(edge.id) ?? 0n);
		const residualReduced =
			overlay.active_arc.direction === "forward" ? reduced : -reduced;
		const flow = flowById.get(edge.id) ?? 0n;
		const residualExists =
			overlay.active_arc.direction === "forward"
				? flow < BigInt(edge.capacity)
				: flow > BigInt(edge.lower);
		const inspection = [
			"inspect-admissible-arc",
			"inspect-price-breakpoint-arc",
		].includes(overlay.stage);
		if (
			(inspection && !residualExists) ||
			(overlay.stage === "push" && residualReduced !== 1n)
		) {
			throw new Error("Prediction-assisted active residual is invalid");
		}
	}
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const suffix: Record<
			FlowPredictionAssistedEpsilonOverlayV1["stage"],
			string
		> = {
			"preprocess-prediction": "preprocess-prediction",
			"begin-attempt": "begin-exponent-attempt",
			"initialize-scale": "initialize-scaled-epsilon-cs",
			"select-surplus": "select-positive-surplus",
			"inspect-admissible-arc": "inspect-admissible-arc",
			"inspect-price-breakpoint-arc": "inspect-price-breakpoint-arc",
			push: "push-epsilon-balanced-arc",
			"raise-price": "raise-price",
			"complete-up-iteration": "complete-up-iteration",
			"complete-scale": "complete-scale",
			"abort-attempt": "abort-exponent-attempt",
			optimal: "certify-optimum",
		};
		if (
			traceEvent.catalog_id !==
			`prediction-assisted-epsilon-relaxation.${suffix[overlay.stage]}`
		) {
			throw new Error("Prediction-assisted event and stage disagree");
		}
	}
}

function validatePredictionAssistedStageShape(
	overlay: FlowPredictionAssistedEpsilonOverlayV1,
): void {
	const activeNode = [
		"select-surplus",
		"inspect-admissible-arc",
		"inspect-price-breakpoint-arc",
		"push",
		"raise-price",
		"complete-up-iteration",
	].includes(overlay.stage);
	const scaled =
		overlay.stage !== "preprocess-prediction" &&
		overlay.stage !== "begin-attempt";
	if (
		activeNode !== (overlay.active_node !== undefined) ||
		["inspect-admissible-arc", "inspect-price-breakpoint-arc", "push"].includes(
			overlay.stage,
		) !==
			(overlay.active_arc !== undefined) ||
		scaled !== (overlay.scale_exponent !== undefined) ||
		(overlay.stage === "optimal") !==
			(overlay.certificate_aligned_prediction_error !== undefined)
	) {
		throw new Error("Prediction-assisted stage fields are inconsistent");
	}
}

function validatePredictionAssistedEpsilonCs(
	edges: FlowEdgeV1[],
	flowById: Map<string, bigint>,
	price: Map<string, bigint>,
	scaledCost: Map<string, bigint>,
): void {
	for (const edge of edges) {
		const flow = flowById.get(edge.id) ?? 0n;
		const reduced =
			(price.get(edge.from) ?? 0n) -
			(price.get(edge.to) ?? 0n) -
			(scaledCost.get(edge.id) ?? 0n);
		if (
			(flow < BigInt(edge.capacity) && reduced > 1n) ||
			(flow > BigInt(edge.lower) && reduced < -1n)
		) {
			throw new Error("Prediction-assisted epsilon-CS is inconsistent");
		}
	}
}

function validateTardosFrameworkScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	config: Record<string, unknown>,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	overlay: FlowTardosFrameworkOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
	outcome: FlowOutcomeV1 | undefined,
): void {
	const selected = algorithmId === "tardos-framework";
	if (!selected) {
		if (overlay !== undefined || outcome?.kind === "tardos-framework") {
			throw new Error("Tardos framework state uses the wrong algorithm");
		}
		return;
	}
	if (
		model.kind !== "fixed-flow-min-cost" &&
		model.kind !== "circulation" &&
		model.kind !== "transshipment"
	) {
		throw new Error("Tardos framework primitive requires a linear MCF model");
	}
	const nodeOrder = canonicalNodeIds(nodes);
	const configuredPotentials = config.potentials;
	if (
		Object.keys(config).length !== 1 ||
		!Object.hasOwn(config, "potentials") ||
		!isRecord(configuredPotentials) ||
		Object.keys(configuredPotentials).length !== nodeOrder.length ||
		nodeOrder.some((node) => {
			const potential = configuredPotentials[node];
			return (
				!Object.hasOwn(configuredPotentials, node) ||
				typeof potential !== "string" ||
				!canonicalI128.test(potential)
			);
		})
	) {
		throw new Error("Tardos framework configuration is not closed");
	}
	if (overlay === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready") ||
			solveStatus === "resource-limit" ||
			solveStatus === "infeasible"
		) {
			return;
		}
		throw new Error("Tardos framework scene is missing its exact overlay");
	}
	const stageStatus: Record<
		FlowTardosFrameworkOverlayV1["stage"],
		FlowCurrentSceneV9["solve_status"]
	> = {
		ready: "ready",
		"construct-feasible-flow": "running",
		"measure-epsilon": "running",
		"classify-fixed-variables": "running",
		complete: "primitive-complete",
	};
	if (
		overlay.determinant_bound !== "1" ||
		overlay.nodes.length !== nodes.length ||
		solveStatus !== stageStatus[overlay.stage] ||
		(overlay.stage === "ready" && eventId !== "0")
	) {
		throw new Error("Tardos framework boundary metadata is inconsistent");
	}
	const potentialByNode = new Map<string, bigint>();
	for (const [index, state] of overlay.nodes.entries()) {
		const node = nodeOrder[index];
		const configured =
			node === undefined ? undefined : configuredPotentials[node];
		if (
			node === undefined ||
			state.node_id !== node ||
			state.potential !== configured
		) {
			throw new Error("Tardos framework node labels are inconsistent");
		}
		potentialByNode.set(node, BigInt(state.potential));
	}

	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const flowByEdge = new Map(
		edgeStates.map((state) => [state.edge_id, BigInt(state.flow)]),
	);
	if (overlay.stage !== "ready") {
		const required = projectLinearMcfRequiredDivergence(model, nodes);
		const actual = new Map(nodeOrder.map((node) => [node, 0n]));
		for (const edge of edges) {
			const flow = flowByEdge.get(edge.id);
			if (flow === undefined) {
				throw new Error("Tardos framework flow state is incomplete");
			}
			actual.set(edge.from, (actual.get(edge.from) ?? 0n) + flow);
			actual.set(edge.to, (actual.get(edge.to) ?? 0n) - flow);
		}
		if (nodeOrder.some((node) => actual.get(node) !== required.get(node))) {
			throw new Error("Tardos framework boundary is not primal-feasible");
		}
	}

	const measured = [
		"measure-epsilon",
		"classify-fixed-variables",
		"complete",
	].includes(overlay.stage);
	if (!measured) {
		if (
			overlay.epsilon !== "0" ||
			overlay.threshold !== "0" ||
			overlay.residual_arcs.length !== 0 ||
			overlay.fixed_variables.length !== 0
		) {
			throw new Error("Tardos framework unpublished theorem state is visible");
		}
	} else {
		const expectedResidual: {
			edge_id: string;
			direction: "forward" | "reverse";
			capacity: bigint;
			reduced_cost: bigint;
		}[] = [];
		for (const id of canonicalStableIds(edges)) {
			const edge = edgeById.get(id);
			const flow = flowByEdge.get(id);
			if (edge === undefined || flow === undefined) {
				throw new Error("Tardos framework edge order is incomplete");
			}
			const fromPotential = potentialByNode.get(edge.from);
			const toPotential = potentialByNode.get(edge.to);
			if (fromPotential === undefined || toPotential === undefined) {
				throw new Error("Tardos framework potential is missing");
			}
			if (flow < BigInt(edge.capacity)) {
				expectedResidual.push({
					edge_id: id,
					direction: "forward",
					capacity: BigInt(edge.capacity) - flow,
					reduced_cost: BigInt(edge.cost) + fromPotential - toPotential,
				});
			}
			if (flow > BigInt(edge.lower)) {
				expectedResidual.push({
					edge_id: id,
					direction: "reverse",
					capacity: flow - BigInt(edge.lower),
					reduced_cost: -BigInt(edge.cost) + toPotential - fromPotential,
				});
			}
		}
		const minimumReduced = expectedResidual.reduce<bigint | undefined>(
			(minimum, arc) =>
				minimum === undefined || arc.reduced_cost < minimum
					? arc.reduced_cost
					: minimum,
			undefined,
		);
		const epsilon =
			minimumReduced !== undefined && minimumReduced < 0n
				? -minimumReduced
				: 0n;
		const threshold = BigInt(nodes.length) * epsilon;
		const classified =
			overlay.stage === "classify-fixed-variables" ||
			overlay.stage === "complete";
		const scannedResidual = expectedResidual.slice(
			0,
			overlay.residual_arcs.length,
		);
		const scannedMinimum = scannedResidual.reduce<bigint | undefined>(
			(minimum, arc) =>
				minimum === undefined || arc.reduced_cost < minimum
					? arc.reduced_cost
					: minimum,
			undefined,
		);
		const scannedEpsilon =
			scannedMinimum !== undefined && scannedMinimum < 0n
				? -scannedMinimum
				: 0n;
		const expectedEpsilon = classified ? epsilon : scannedEpsilon;
		const projectedThreshold = BigInt(overlay.threshold);
		const thresholdIsValid = classified
			? projectedThreshold === threshold
			: overlay.residual_arcs.length === expectedResidual.length
				? projectedThreshold === 0n || projectedThreshold === threshold
				: projectedThreshold === 0n;
		if (
			overlay.residual_arcs.length > expectedResidual.length ||
			(classified &&
				overlay.residual_arcs.length !== expectedResidual.length) ||
			BigInt(overlay.epsilon) !== expectedEpsilon ||
			!thresholdIsValid
		) {
			throw new Error("Tardos framework epsilon measurement is inconsistent");
		}
		const expectedFixed: {
			index: number;
			fixed: FlowTardosFixedVariableV1;
		}[] = [];
		for (const [index, expected] of expectedResidual.entries()) {
			if (expected.reduced_cost <= threshold) continue;
			const edge = edgeById.get(expected.edge_id);
			const flow = flowByEdge.get(expected.edge_id);
			const bound = expected.direction === "forward" ? "lower" : "upper";
			const boundValue =
				edge === undefined
					? undefined
					: BigInt(bound === "lower" ? edge.lower : edge.capacity);
			if (edge === undefined || flow === undefined || flow !== boundValue) {
				throw new Error("Tardos fixed variable is not at its proved bound");
			}
			expectedFixed.push({
				index,
				fixed: {
					edge_id: expected.edge_id,
					bound,
					value: flow.toString(),
					direction: expected.direction,
					reduced_cost: expected.reduced_cost.toString(),
				},
			});
		}
		if (
			(!classified && overlay.fixed_variables.length !== 0) ||
			overlay.fixed_variables.length > expectedFixed.length ||
			(overlay.stage === "complete" &&
				overlay.fixed_variables.length !== expectedFixed.length) ||
			overlay.fixed_variables.some((actual, index) => {
				const expected = expectedFixed[index]?.fixed;
				return (
					expected === undefined ||
					actual.edge_id !== expected.edge_id ||
					actual.bound !== expected.bound ||
					actual.value !== expected.value ||
					actual.direction !== expected.direction ||
					actual.reduced_cost !== expected.reduced_cost
				);
			})
		) {
			throw new Error("Tardos framework fixed-variable certificate differs");
		}
		const fixedPrefix = expectedFixed.slice(0, overlay.fixed_variables.length);
		for (const [index, actual] of overlay.residual_arcs.entries()) {
			const expected = expectedResidual[index];
			const fixes =
				classified &&
				fixedPrefix.some((candidate) => candidate.index === index);
			if (
				expected === undefined ||
				actual === undefined ||
				actual.edge_id !== expected.edge_id ||
				actual.direction !== expected.direction ||
				BigInt(actual.capacity) !== expected.capacity ||
				BigInt(actual.reduced_cost) !== expected.reduced_cost ||
				actual.fixes_variable !== (classified && fixes)
			) {
				throw new Error("Tardos framework residual scan is inconsistent");
			}
		}
	}

	if (traceEventRequiresStageIdentity(traceEvent)) {
		const expectedCatalog: Partial<
			Record<FlowTardosFrameworkOverlayV1["stage"], readonly string[]>
		> = {
			"construct-feasible-flow": ["construct-feasible-flow"],
			"measure-epsilon": ["scan-residual-arc", "measure-epsilon"],
			"classify-fixed-variables": [
				"inspect-fixed-variable",
				"classify-fixed-variables",
			],
			complete: ["complete-primitive"],
		};
		const suffixes = expectedCatalog[overlay.stage];
		if (
			suffixes === undefined ||
			!suffixes.some(
				(suffix) => traceEvent.catalog_id === `tardos-framework.${suffix}`,
			)
		) {
			throw new Error("Tardos framework event and stage disagree");
		}
	}
	if (overlay.stage === "complete") {
		if (
			outcome?.kind !== "tardos-framework" ||
			outcome.epsilon !== overlay.epsilon ||
			outcome.threshold !== overlay.threshold ||
			outcome.determinant_bound !== overlay.determinant_bound ||
			outcome.fixed_variables.length !== overlay.fixed_variables.length ||
			outcome.fixed_variables.some((fixed, index) => {
				const expected = overlay.fixed_variables[index];
				return (
					expected === undefined ||
					fixed.edge_id !== expected.edge_id ||
					fixed.bound !== expected.bound ||
					fixed.value !== expected.value ||
					fixed.direction !== expected.direction ||
					fixed.reduced_cost !== expected.reduced_cost
				);
			})
		) {
			throw new Error("Tardos framework outcome is inconsistent");
		}
	} else if (outcome?.kind === "tardos-framework") {
		throw new Error("Tardos framework outcome appears before completion");
	}
}

function floorBigIntDivision(numerator: bigint, denominator: bigint): bigint {
	const quotient = numerator / denominator;
	return numerator % denominator < 0n ? quotient - 1n : quotient;
}

function validateConvexNetworkSimplexScene(
	model: FlowProblemModelV1,
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	algorithmId: string,
	solveStatus: FlowCurrentSceneV9["solve_status"],
	eventId: string,
	convex: FlowConvexCostOverlayV1 | undefined,
	overlay: FlowConvexNetworkSimplexOverlayV1 | undefined,
	traceEvent: FlowTraceEventV1 | undefined,
): void {
	const selected = algorithmId === "convex-network-simplex";
	if (!selected) {
		if (overlay !== undefined) {
			throw new Error("Convex-simplex overlay uses the wrong algorithm");
		}
		return;
	}
	if (model.kind !== "convex-cost-flow") {
		throw new Error("Convex network simplex requires a convex-cost model");
	}
	if (overlay === undefined || convex === undefined) {
		if (
			(eventId === "0" && solveStatus === "ready") ||
			solveStatus === "resource-limit" ||
			solveStatus === "infeasible"
		) {
			return;
		}
		throw new Error("Convex network simplex is missing its compact overlay");
	}
	if (
		(solveStatus === "optimal") !== (overlay.stage === "optimal") ||
		overlay.nodes.length !== nodes.length + 1 ||
		overlay.edges.length !== edges.length ||
		overlay.artificial_edges.length !== nodes.length
	) {
		throw new Error("Convex-simplex boundary shape is inconsistent");
	}
	const tree = validateConvexSimplexBasis(
		nodes,
		edges,
		edgeStates,
		convex,
		overlay,
	);
	validateConvexSimplexParents(nodes, overlay, tree);
	validateConvexSimplexSelection(nodes, edges, convex, overlay);
	if (traceEventRequiresStageIdentity(traceEvent)) {
		const suffix: Record<FlowConvexNetworkSimplexOverlayV1["stage"], string> = {
			"initialize-basis": "initialize-compact-basis",
			price: "price-forward-backward",
			"form-cycle": "form-fundamental-cycle",
			"cross-breakpoint": "cross-segment-breakpoint",
			"exchange-basis": "exchange-basis",
			"flip-bound": "flip-entering-bound",
			optimal: "certify-expanded-oracle",
		};
		if (
			traceEvent.catalog_id !==
			`convex-network-simplex.${suffix[overlay.stage]}`
		) {
			throw new Error("Convex-simplex event and stage disagree");
		}
	}
}

function validateConvexSimplexBasis(
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	edgeStates: FlowEdgeStateV1[],
	convex: FlowConvexCostOverlayV1,
	overlay: FlowConvexNetworkSimplexOverlayV1,
): Map<number, Set<number>> {
	const nodeOrder = canonicalNodeIds(nodes);
	const edgeOrder = canonicalStableIds(edges);
	const nodeIndex = new Map(nodeOrder.map((id, index) => [id, index]));
	nodeIndex.set("artificial-root", nodes.length);
	const potential = new Map<string, bigint>();
	for (const [index, state] of overlay.nodes.entries()) {
		const expected =
			index === nodes.length ? "artificial-root" : nodeOrder[index];
		if (
			state.entity_id !== expected ||
			(index === nodes.length) !== (state.parent === undefined) ||
			(state.parent !== undefined && !nodeIndex.has(state.parent))
		) {
			throw new Error("Convex-simplex extended node is inconsistent");
		}
		potential.set(state.entity_id, BigInt(state.potential));
	}
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const flowById = new Map(
		edgeStates.map((state) => [state.edge_id, BigInt(state.flow)]),
	);
	const convexById = new Map(
		convex.edges.map((state) => [state.edge_id, state]),
	);
	const tree = new Map(
		Array.from({ length: nodes.length + 1 }, (_, index) => [
			index,
			new Set<number>(),
		]),
	);
	for (const [index, state] of overlay.edges.entries()) {
		const edge = edgeById.get(state.edge_id);
		const flow = flowById.get(state.edge_id);
		const costState = convexById.get(state.edge_id);
		if (
			state.edge_id !== edgeOrder[index] ||
			edge === undefined ||
			flow === undefined ||
			costState === undefined
		) {
			throw new Error("Convex-simplex original edge identity is inconsistent");
		}
		const piece =
			state.active_segment === undefined
				? undefined
				: costState.segments.find(
						(segment) => segment.segment === state.active_segment,
					);
		const variable = BigInt(edge.capacity) > BigInt(edge.lower);
		const contains =
			piece !== undefined && BigInt(piece.start_flow) < BigInt(edge.lower)
				? flow >= BigInt(edge.lower) && flow <= BigInt(piece.end_flow)
				: piece !== undefined &&
					flow >= BigInt(piece.start_flow) &&
					flow <= BigInt(piece.end_flow);
		const breakpoint =
			piece !== undefined &&
			(flow === BigInt(edge.lower) ||
				flow === BigInt(edge.capacity) ||
				flow === BigInt(piece.start_flow) ||
				flow === BigInt(piece.end_flow));
		const intermediate = overlay.stage === "cross-breakpoint" && state.in_cycle;
		if (
			variable !== (piece !== undefined) ||
			(!variable && flow !== BigInt(edge.lower)) ||
			(variable &&
				(!contains ||
					(state.basis === "breakpoint" && !breakpoint && !intermediate))) ||
			(state.basis === "tree" && edge.from === edge.to)
		) {
			throw new Error("Convex-simplex compact edge state is inconsistent");
		}
		if (state.basis === "tree") {
			if (overlay.stage !== "cross-breakpoint") {
				const reduced =
					BigInt(piece?.marginal_cost ?? "0") +
					(potential.get(edge.from) ?? 0n) -
					(potential.get(edge.to) ?? 0n);
				if (reduced !== 0n) {
					throw new Error("Convex-simplex tree potential is not tight");
				}
			}
			connectConvexSimplexTree(tree, nodeIndex, edge.from, edge.to);
		}
	}
	validateConvexSimplexArtificialEdges(
		nodes,
		overlay,
		potential,
		nodeIndex,
		tree,
	);
	return tree;
}

function validateConvexSimplexArtificialEdges(
	nodes: FlowNodeV1[],
	overlay: FlowConvexNetworkSimplexOverlayV1,
	potential: Map<string, bigint>,
	nodeIndex: Map<string, number>,
	tree: Map<number, Set<number>>,
): void {
	const nodeOrder = canonicalNodeIds(nodes);
	for (const [index, state] of overlay.artificial_edges.entries()) {
		const node = nodeOrder[index];
		const endpoints = new Set([state.source, state.target]);
		if (
			node === undefined ||
			state.entity_id !== `artificial:${node}` ||
			state.node_id !== node ||
			endpoints.size !== 2 ||
			!endpoints.has(node) ||
			!endpoints.has("artificial-root")
		) {
			throw new Error("Convex-simplex artificial edge is inconsistent");
		}
		if (state.basis === "tree") {
			const reduced =
				BigInt(overlay.artificial_cost) +
				(potential.get(state.source) ?? 0n) -
				(potential.get(state.target) ?? 0n);
			if (reduced !== 0n) {
				throw new Error("Convex-simplex artificial tree edge is not tight");
			}
			connectConvexSimplexTree(tree, nodeIndex, state.source, state.target);
		}
	}
}

function connectConvexSimplexTree(
	tree: Map<number, Set<number>>,
	nodeIndex: Map<string, number>,
	leftId: string,
	rightId: string,
): void {
	const left = nodeIndex.get(leftId);
	const right = nodeIndex.get(rightId);
	if (left === undefined || right === undefined || left === right) {
		throw new Error("Convex-simplex tree endpoint is invalid");
	}
	tree.get(left)?.add(right);
	tree.get(right)?.add(left);
}

function validateConvexSimplexParents(
	nodes: FlowNodeV1[],
	overlay: FlowConvexNetworkSimplexOverlayV1,
	tree: Map<number, Set<number>>,
): void {
	const root = nodes.length;
	const edgeCount =
		[...tree.values()].reduce((sum, adjacent) => sum + adjacent.size, 0) / 2;
	if (edgeCount !== nodes.length) {
		throw new Error("Convex-simplex basis does not have tree cardinality");
	}
	const entityIndex = new Map(
		overlay.nodes.map((state, index) => [state.entity_id, index]),
	);
	for (let node = 0; node < root; node += 1) {
		const parent = overlay.nodes[node]?.parent;
		const parentIndex =
			parent === undefined ? undefined : entityIndex.get(parent);
		if (parentIndex === undefined || !tree.get(node)?.has(parentIndex)) {
			throw new Error("Convex-simplex parent is not a tree neighbor");
		}
		let cursor = node;
		for (let step = 0; step <= root && cursor !== root; step += 1) {
			const next = overlay.nodes[cursor]?.parent;
			cursor = next === undefined ? -1 : (entityIndex.get(next) ?? -1);
		}
		if (cursor !== root) {
			throw new Error("Convex-simplex parent chain does not reach the root");
		}
	}
}

function validateConvexSimplexSelection(
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	convex: FlowConvexCostOverlayV1,
	overlay: FlowConvexNetworkSimplexOverlayV1,
): void {
	const allRefs = [
		...overlay.cycle,
		...(overlay.entering === undefined ? [] : [overlay.entering]),
		...(overlay.leaving === undefined ? [] : [overlay.leaving]),
	];
	for (const reference of allRefs) {
		convexSimplexArcEndpoints(nodes, edges, convex, overlay, reference);
	}
	for (const [index, reference] of overlay.cycle.entries()) {
		const [, target] = convexSimplexArcEndpoints(
			nodes,
			edges,
			convex,
			overlay,
			reference,
		);
		const [nextSource] = convexSimplexArcEndpoints(
			nodes,
			edges,
			convex,
			overlay,
			overlay.cycle[
				(index + 1) % overlay.cycle.length
			] as FlowConvexNetworkSimplexArcRefV1,
		);
		if (target !== nextSource) {
			throw new Error("Convex-simplex fundamental cycle is discontinuous");
		}
	}
	const hasCycle = overlay.cycle.length > 0;
	const selectionValid =
		overlay.stage === "initialize-basis" || overlay.stage === "optimal"
			? !hasCycle &&
				overlay.entering === undefined &&
				overlay.leaving === undefined
			: overlay.stage === "price"
				? !hasCycle && overlay.leaving === undefined
				: overlay.stage === "form-cycle"
					? hasCycle &&
						overlay.entering !== undefined &&
						overlay.leaving === undefined
					: hasCycle &&
						overlay.entering !== undefined &&
						overlay.leaving !== undefined;
	if (!selectionValid) {
		throw new Error("Convex-simplex selection does not match its stage");
	}
	const cycleEntities = new Set(overlay.cycle.map((arc) => arc.entity_id));
	for (const state of [...overlay.edges, ...overlay.artificial_edges]) {
		const entity = "edge_id" in state ? state.edge_id : state.entity_id;
		if (
			state.in_cycle !== cycleEntities.has(entity) ||
			state.entering !== (overlay.entering?.entity_id === entity) ||
			state.leaving !== (overlay.leaving?.entity_id === entity)
		) {
			throw new Error("Convex-simplex selection flags are inconsistent");
		}
	}
}

function convexSimplexArcEndpoints(
	nodes: FlowNodeV1[],
	edges: FlowEdgeV1[],
	convex: FlowConvexCostOverlayV1,
	overlay: FlowConvexNetworkSimplexOverlayV1,
	reference: FlowConvexNetworkSimplexArcRefV1,
): [string, string] {
	const edgeIndex = edges.findIndex((edge) => edge.id === reference.entity_id);
	let endpoints: [string, string];
	if (edgeIndex >= 0) {
		const edge = edges[edgeIndex] as FlowEdgeV1;
		if (
			reference.segment === undefined ||
			!convex.edges[edgeIndex]?.segments.some(
				(segment) => segment.segment === reference.segment,
			)
		) {
			throw new Error("Convex-simplex original arc segment is unknown");
		}
		endpoints = [edge.from, edge.to];
	} else {
		const artificial = overlay.artificial_edges.find(
			(edge) => edge.entity_id === reference.entity_id,
		);
		if (artificial === undefined || reference.segment !== undefined) {
			throw new Error("Convex-simplex artificial arc is unknown");
		}
		endpoints = [artificial.source, artificial.target];
	}
	const knownNodes = new Set([
		...nodes.map((node) => node.id),
		"artificial-root",
	]);
	if (!knownNodes.has(endpoints[0]) || !knownNodes.has(endpoints[1])) {
		throw new Error("Convex-simplex arc endpoint is unknown");
	}
	return reference.direction === "forward"
		? endpoints
		: [endpoints[1], endpoints[0]];
}

function publicFeasibilityBalance(
	model: FlowProblemModelV1,
	nodes: readonly FlowNodeV1[],
): ReadonlyMap<string, bigint> {
	if (
		model.kind === "fixed-flow-min-cost" ||
		model.kind === "circulation" ||
		model.kind === "transshipment"
	) {
		return projectLinearMcfRequiredDivergence(model, nodes);
	}
	if (model.kind !== "transportation" && model.kind !== "convex-cost-flow") {
		throw new Error(
			"Public model does not define a balance-feasibility request",
		);
	}
	const required = new Map(nodes.map((node) => [node.id, BigInt(node.supply)]));
	if (
		required.size !== nodes.length ||
		[...required.values()].reduce((sum, value) => sum + value, 0n) !== 0n
	) {
		throw new Error("Public balance-feasibility request is not canonical");
	}
	return required;
}

function validateFeasibilityDomainAgainstPublicScene(
	overlay: FlowFeasibilityOverlayV2,
	model: FlowProblemModelV1,
	nodes: readonly FlowNodeV1[],
	edges: readonly FlowEdgeV1[],
): void {
	const domain = overlay.domain;
	const publicNodeById = new Map(nodes.map((node) => [node.id, node]));
	const publicEdgeById = new Map(edges.map((edge) => [edge.id, edge]));
	const everyNodeAligned = domain.nodes.every(
		(node) =>
			node.public_node_id === node.node_id && publicNodeById.has(node.node_id),
	);
	for (const node of domain.nodes) {
		if (
			node.public_node_id !== undefined &&
			(node.public_node_id !== node.node_id ||
				!publicNodeById.has(node.public_node_id))
		) {
			throw new Error("Feasibility node projection is not identity-safe");
		}
	}
	for (const edge of domain.edges) {
		if (edge.public_route_edge_id === undefined) continue;
		const publicEdge = publicEdgeById.get(edge.public_route_edge_id);
		if (
			publicEdge === undefined ||
			edge.public_route_edge_id !== edge.edge_id ||
			publicEdge.from !== edge.from_node_id ||
			publicEdge.to !== edge.to_node_id
		) {
			throw new Error("Feasibility public route projection is invalid");
		}
	}
	const publicNodeIds = canonicalNodeIds([...nodes]);
	const exactPublicNodes =
		domain.nodes.length === nodes.length &&
		domain.nodes.every(
			(node, index) =>
				node.node_id === publicNodeIds[index] &&
				node.public_node_id === node.node_id,
		);
	const publicEdges = canonicalStableIds([...edges]).map((edgeId) => {
		const edge = publicEdgeById.get(edgeId);
		if (edge === undefined) {
			throw new Error("Feasibility public edge identity is missing");
		}
		return edge;
	});
	const exactPublicEdges =
		domain.edges.length === publicEdges.length &&
		domain.edges.every((edge, index) => {
			const publicEdge = publicEdges[index];
			return (
				publicEdge !== undefined &&
				edge.edge_id === publicEdge.id &&
				edge.from_node_id === publicEdge.from &&
				edge.to_node_id === publicEdge.to &&
				edge.lower === publicEdge.lower &&
				edge.capacity === publicEdge.capacity &&
				edge.public_route_edge_id === publicEdge.id
			);
		});
	const exactPublicDomain = exactPublicNodes && exactPublicEdges;
	if (
		(domain.kind === "public-input") !==
			(exactPublicDomain && overlay.use_kind !== "anchored-recovery") ||
		(domain.kind === "node-aligned-transformation") !==
			(overlay.use_kind === "anchored-recovery" && everyNodeAligned) ||
		(domain.kind === "standalone-transformation") !==
			(overlay.use_kind === "anchored-recovery" && !everyNodeAligned)
	) {
		throw new Error("Feasibility domain relationship is misclassified");
	}
	if (
		(overlay.use_kind === "anchored-recovery") ===
		(domain.kind === "public-input")
	) {
		throw new Error("Feasibility use and domain relationship disagree");
	}
	if (domain.kind !== "public-input") return;
	const request = domain.request;
	if (request.kind === "balance") {
		const expected = publicFeasibilityBalance(model, nodes);
		if (
			request.required_divergences.some(
				(item) =>
					expected.get(item.node_id) !== BigInt(item.required_divergence),
			)
		) {
			throw new Error("Feasibility request disagrees with the public model");
		}
		return;
	}
	const terminals =
		model.kind === "max-flow" ||
		model.kind === "planar-max-flow" ||
		model.kind === "min-cost-max-flow"
			? { source: model.source, sink: model.sink }
			: undefined;
	if (
		terminals === undefined ||
		request.source_node_id !== terminals.source ||
		request.sink_node_id !== terminals.sink
	) {
		throw new Error(
			"Feasibility maximum-flow request disagrees with the model",
		);
	}
}

function validateFeasibilityOverlayAgainstScene(
	overlay: FlowFeasibilityOverlayV2,
	model: FlowProblemModelV1,
	nodes: readonly FlowNodeV1[],
	edges: readonly FlowEdgeV1[],
	edgeStates: readonly FlowEdgeStateV1[],
	traceEvent: FlowTraceEventV1 | undefined,
): void {
	validateFeasibilityDomainAgainstPublicScene(overlay, model, nodes, edges);
	const canonicalNodes = overlay.domain.nodes.map((node) => node.node_id);
	const canonicalEdges = overlay.domain.edges;
	if (
		overlay.nodes.length !== canonicalNodes.length + 2 ||
		overlay.nodes.some((state, index) =>
			index < canonicalNodes.length
				? state.node.kind !== "original" ||
					state.node.original_node_id !== canonicalNodes[index]
				: index === canonicalNodes.length
					? state.node.kind !== "super-source"
					: state.node.kind !== "super-sink",
		)
	) {
		throw new Error(
			"Feasibility overlay node order does not match the canonical graph",
		);
	}
	const required =
		overlay.domain.request.kind === "balance"
			? new Map(
					overlay.domain.request.required_divergences.map((item) => [
						item.node_id,
						BigInt(item.required_divergence),
					]),
				)
			: new Map(canonicalNodes.map((nodeId) => [nodeId, 0n]));
	const lowerDivergence = new Map(canonicalNodes.map((nodeId) => [nodeId, 0n]));
	const expectedArcs: Array<
		Readonly<{
			key: string;
			from: FlowFeasibilityNodeRefV1;
			to: FlowFeasibilityNodeRefV1;
			capacity: bigint;
		}>
	> = [];
	for (const edge of canonicalEdges) {
		const lower = BigInt(edge.lower);
		lowerDivergence.set(
			edge.from_node_id,
			(lowerDivergence.get(edge.from_node_id) ?? 0n) + lower,
		);
		lowerDivergence.set(
			edge.to_node_id,
			(lowerDivergence.get(edge.to_node_id) ?? 0n) - lower,
		);
		expectedArcs.push({
			key: `original\u0000${edge.edge_id}`,
			from: { kind: "original", original_node_id: edge.from_node_id },
			to: { kind: "original", original_node_id: edge.to_node_id },
			capacity: BigInt(edge.capacity) - lower,
		});
	}
	if (overlay.domain.request.kind === "max-flow-initial") {
		const capacitySum = canonicalEdges.reduce(
			(sum, edge) => sum + BigInt(edge.capacity),
			0n,
		);
		expectedArcs.push({
			key: `return\u0000${overlay.domain.request.sink_node_id}\u0000${overlay.domain.request.source_node_id}`,
			from: {
				kind: "original",
				original_node_id: overlay.domain.request.sink_node_id,
			},
			to: {
				kind: "original",
				original_node_id: overlay.domain.request.source_node_id,
			},
			capacity: capacitySum,
		});
	}
	for (const nodeId of canonicalNodes) {
		const shifted =
			(required.get(nodeId) ?? 0n) - (lowerDivergence.get(nodeId) ?? 0n);
		if (shifted > 0n) {
			expectedArcs.push({
				key: `super-source\u0000${nodeId}`,
				from: { kind: "super-source" },
				to: { kind: "original", original_node_id: nodeId },
				capacity: shifted,
			});
		} else if (shifted < 0n) {
			expectedArcs.push({
				key: `super-sink\u0000${nodeId}`,
				from: { kind: "original", original_node_id: nodeId },
				to: { kind: "super-sink" },
				capacity: -shifted,
			});
		}
	}
	if (overlay.arcs.length > expectedArcs.length) {
		throw new Error("Feasibility overlay contains an unexpected auxiliary arc");
	}
	for (const [index, arc] of overlay.arcs.entries()) {
		const expected = expectedArcs[index];
		if (
			expected === undefined ||
			feasibilityArcKey(arc.arc) !== expected.key ||
			!sameFeasibilityNode(arc.from, expected.from) ||
			!sameFeasibilityNode(arc.to, expected.to) ||
			BigInt(arc.capacity) !== expected.capacity
		) {
			throw new Error(
				"Feasibility auxiliary topology is not a canonical construction prefix",
			);
		}
	}
	const arcByKey = new Map(
		overlay.arcs.map((arc) => [feasibilityArcKey(arc.arc), arc]),
	);
	const stateByEdge = new Map(
		edgeStates.map((state) => [state.edge_id, state]),
	);
	if (overlay.use_kind === "initial-flow") {
		for (const edge of canonicalEdges) {
			const state = stateByEdge.get(edge.edge_id);
			const auxiliary = arcByKey.get(`original\u0000${edge.edge_id}`);
			if (
				state === undefined ||
				BigInt(state.flow) !==
					BigInt(edge.lower) + BigInt(auxiliary?.flow ?? "0")
			) {
				throw new Error(
					"Feasibility original-flow projection disagrees with its auxiliary arc",
				);
			}
		}
	}
	const materializedRequired = overlay.arcs.reduce(
		(total, arc) =>
			arc.arc.kind === "from-super-source"
				? total + BigInt(arc.capacity)
				: total,
		0n,
	);
	if (BigInt(overlay.total_required) !== materializedRequired) {
		throw new Error(
			"Feasibility required amount does not match its constructed super arcs",
		);
	}
	const routedIntoSink = overlay.arcs.reduce(
		(total, arc) =>
			arc.arc.kind === "to-super-sink" ? total + BigInt(arc.flow) : total,
		0n,
	);
	const routingPublished = [
		"complete-routing",
		"inspect-cut-arc",
		"mark-reachable",
		"extract-original-flow",
		"feasible",
		"infeasible",
	].includes(overlay.stage);
	if (
		(routingPublished && BigInt(overlay.routed) !== routedIntoSink) ||
		(!routingPublished && overlay.routed !== "0") ||
		(overlay.stage === "feasible" &&
			overlay.routed !== overlay.total_required) ||
		(overlay.stage === "infeasible" &&
			BigInt(overlay.routed) >= BigInt(overlay.total_required))
	) {
		throw new Error(
			"Feasibility routed amount does not match the auxiliary flow",
		);
	}
	const adjacencyCount = new Map(
		overlay.nodes.map((state) => [feasibilityNodeKey(state.node), 0]),
	);
	for (const arc of overlay.arcs) {
		const from = feasibilityNodeKey(arc.from);
		const to = feasibilityNodeKey(arc.to);
		adjacencyCount.set(from, (adjacencyCount.get(from) ?? 0) + 1);
		adjacencyCount.set(to, (adjacencyCount.get(to) ?? 0) + 1);
	}
	if (
		overlay.nodes.some(
			(state) =>
				BigInt(state.current_arc) >
				BigInt(adjacencyCount.get(feasibilityNodeKey(state.node)) ?? 0),
		) ||
		overlay.active_queue.some((node) => node.kind !== "original")
	) {
		throw new Error("Feasibility current-arc or FIFO state is out of range");
	}
	if (overlay.focus_arc !== undefined && overlay.focus_node !== undefined) {
		const focused = arcByKey.get(feasibilityArcKey(overlay.focus_arc.arc));
		if (focused === undefined) {
			throw new Error("Feasibility focused arc is absent");
		}
		const tail =
			overlay.focus_arc.direction === "forward" ? focused.from : focused.to;
		const head =
			overlay.focus_arc.direction === "forward" ? focused.to : focused.from;
		const expectedNode = overlay.stage === "mark-reachable" ? head : tail;
		if (!sameFeasibilityNode(overlay.focus_node, expectedNode)) {
			throw new Error(
				"Feasibility focused node is not incident in the inspected direction",
			);
		}
	}
	if (
		overlay.focus_arc !== undefined &&
		["add-original-arc", "add-imbalance-arc", "extract-original-flow"].includes(
			overlay.stage,
		) &&
		overlay.focus_arc.direction !== "forward"
	) {
		throw new Error("Feasibility construction focused a reverse auxiliary arc");
	}
	const expectedCatalogId = `feasibility.${overlay.stage}`;
	// The catalog ID is the cross-language stage identity. Pseudocode text is
	// source-owned, so its namespace is checked without duplicating the Rust
	// wording table in the decoder.
	if (
		traceEvent === undefined ||
		traceEvent.catalog_id !== expectedCatalogId ||
		!traceEvent.pseudocode_line.startsWith("feasibility:")
	) {
		throw new Error(
			"Feasibility overlay stage and source event identity disagree",
		);
	}
	const sourceMetrics = overlay.metrics;
	if (
		BigInt(sourceMetrics.original_edge_inspections) !==
			BigInt(
				overlay.arcs.filter((arc) => arc.arc.kind === "original").length,
			) ||
		BigInt(sourceMetrics.original_node_inspections) >
			BigInt(overlay.domain.nodes.length) ||
		BigInt(sourceMetrics.extracted_original_edges) >
			BigInt(overlay.domain.edges.length)
	) {
		throw new Error(
			"Feasibility source-work counters do not match the rendered construction",
		);
	}
}

export function decodeFlowCurrentSceneV9(
	payload: Uint8Array,
): FlowCurrentSceneV9 {
	const source = new TextDecoder("utf-8", { fatal: true }).decode(payload);
	const value: unknown = JSON.parse(source);
	assertFlowCurrentSceneV9Wire(value);
	return validateFlowCurrentSceneV9Semantics(value);
}

/**
 * Domain-level bindings layered on top of the generated structural decoders.
 * This object is intentionally total: every generated overlay either binds a
 * validator or is explicitly exempted with a reason in the contribution
 * registry. Missing and unknown bindings fail closed at module initialization.
 */
const FLOW_OVERLAY_SEMANTIC_VALIDATORS = {
	augmenting_electrical_overlay: decodeAugmentingElectricalOverlay,
	binary_blocking_overlay: decodeBinaryBlockingOverlay,
	cancel_tighten_overlay: decodeCancelTightenOverlay,
	convex_cost_overlay: decodeConvexCostOverlay,
	convex_network_simplex_overlay: decodeConvexNetworkSimplexOverlay,
	deterministic_almost_linear_overlay: decodeDeterministicAlmostLinearOverlay,
	double_scaling_overlay: decodeDoubleScalingOverlay,
	dual_network_simplex_overlay: decodeDualNetworkSimplexOverlay,
	dynamic_eibfs_overlay: decodeDynamicEibfsOverlay,
	eibfs_overlay: decodeEibfsOverlay,
	electrical_flow_overlay: decodeElectricalFlowOverlay,
	electrical_ipm_mcf_overlay: decodeElectricalIpmMcfOverlay,
	enhanced_capacity_scaling_overlay: decodeEnhancedCapacityScalingOverlay,
	feasibility_overlay: decodeFeasibilityOverlay,
	flow_framework_mcf_overlay: decodeFlowFrameworkMcfOverlay,
	interior_point_max_flow_overlay: decodeInteriorPointMaxFlowOverlay,
	minimum_ratio_cycle_mcf_overlay: decodeMinimumRatioCycleMcfOverlay,
	minimum_ratio_cycle_overlay: decodeMinimumRatioCycleOverlay,
	orlin_max_flow_overlay: decodeOrlinMaxFlowOverlay,
	orlin_mcf_overlay: decodeOrlinMcfOverlay,
	parametric_overlay: decodeParametricOverlay,
	polynomial_dual_simplex_overlay: decodePolynomialDualSimplexOverlay,
	polynomial_primal_simplex_overlay: decodePolynomialPrimalSimplexOverlay,
	prediction_assisted_epsilon_overlay: decodePredictionAssistedEpsilonOverlay,
	primal_dual_ipm_mcf_overlay: decodePrimalDualIpmMcfOverlay,
	randomized_almost_linear_mcf_overlay: decodeRandomizedAlmostLinearMcfOverlay,
	randomized_almost_linear_overlay: decodeRandomizedAlmostLinearOverlay,
	relaxed_mndc_overlay: decodeRelaxedMndcOverlay,
	tardos_framework_overlay: decodeTardosFrameworkOverlay,
	weighted_augmenting_paths_overlay: decodeWeightedAugmentingPathsOverlay,
	weighted_push_relabel_shortcut_overlay:
		decodeWeightedPushRelabelShortcutOverlay,
} satisfies FlowOverlaySemanticBindings;

assertFlowOverlaySemanticBindings(FLOW_OVERLAY_SEMANTIC_VALIDATORS);

function validateGeneratedFlowOverlaySemantics(
	value: FlowCurrentSceneV9,
): void {
	for (const contribution of FLOW_OVERLAY_CONTRIBUTION_ENTRIES) {
		const overlay = value[contribution.field];
		if (overlay === undefined) continue;
		const validator = FLOW_OVERLAY_SEMANTIC_VALIDATORS[contribution.field];
		if (contribution.semantic.kind === "structural-exemption") {
			if (validator !== null) {
				throw new Error(
					`Overlay semantic exemption ${contribution.field} is misconfigured`,
				);
			}
			continue;
		}
		if (validator === null) {
			throw new Error(
				`Overlay semantic validator ${contribution.field} is missing`,
			);
		}
		validator(overlay);
	}
}

/**
 * Validates arithmetic, graph, and algorithm invariants after the generated
 * Rust-owned structural contract has narrowed the untrusted payload.
 *
 * Structural fields are deliberately not reconstructed here: a newly generated
 * additive field must survive decoding without another root-level edit.
 */
function validateFlowCurrentSceneV9Semantics(
	value: FlowCurrentSceneV9,
): FlowCurrentSceneV9 {
	// `fixed` was introduced additively with a serde default. Preserve the public
	// decoded-scene invariant without reconstructing the generated root DTO.
	for (const arc of value.residual_arcs) arc.fixed ??= false;
	if (
		!canonicalU64.test(value.event_id) ||
		!canonicalU64.test(value.event_count) ||
		BigInt(value.event_id) > BigInt(value.event_count) ||
		value.metrics.length !== FLOW_METRIC_COUNT ||
		!value.metrics.every((metric) => canonicalU64.test(metric)) ||
		value.trace_steps.primary_work.metric_ordinal >= FLOW_METRIC_COUNT ||
		value.trace_steps.primary_work.unit.trim().length === 0 ||
		value.trace_steps.phase_unit.trim().length === 0 ||
		(value.trace_steps.phase_availability.availability === "unavailable" &&
			value.trace_steps.phase_availability.reason.trim().length === 0) ||
		value.trace_steps.operation_unit.trim().length === 0 ||
		(value.trace_steps.operation_availability.availability === "unavailable" &&
			value.trace_steps.operation_availability.reason.trim().length === 0) ||
		(value.trace_steps.detail.availability === "available"
			? value.trace_steps.detail.unit.trim().length === 0
			: value.trace_steps.detail.reason.trim().length === 0)
	) {
		throw new Error("Flow scene contract is invalid");
	}
	const nodes = value.graph.nodes.map(decodeNode);
	const edges = value.graph.edges.map(decodeEdge);
	const edgeStates = value.edge_states.map(decodeEdgeState);
	const residualArcs = value.residual_arcs.map(decodeResidualArc);
	const nodeTraceStates = value.node_trace_states.map(decodeNodeTraceState);
	const model = decodeFlowProblemModelV1(value.model);
	const pseudoflowForest = decodePseudoflowForest(value.pseudoflow_forest);
	validateGeneratedFlowOverlaySemantics(value);
	const eibfsOverlay = value.eibfs_overlay;
	const dynamicEibfsOverlay = value.dynamic_eibfs_overlay;
	const parametricOverlay = value.parametric_overlay;
	const feasibilityOverlay = value.feasibility_overlay;
	const feasibilityWork = decodeFeasibilityWorkSummary(value.feasibility_work);
	if (
		feasibilityWork !== undefined &&
		(value.run_profile !== "fast" ||
			value.solve_status === "ready" ||
			feasibilityOverlay !== undefined)
	) {
		throw new Error(
			"Feasibility-work summary does not belong to this execution boundary",
		);
	}
	const binaryBlockingOverlay = value.binary_blocking_overlay;
	const cancelTightenOverlay = value.cancel_tighten_overlay;
	const relaxedMndcOverlay = value.relaxed_mndc_overlay;
	const enhancedCapacityScalingOverlay =
		value.enhanced_capacity_scaling_overlay;
	const orlinMcfOverlay = value.orlin_mcf_overlay;
	const orlinMaxFlowOverlay = value.orlin_max_flow_overlay;
	const electricalFlowOverlay = value.electrical_flow_overlay;
	const augmentingElectricalOverlay = value.augmenting_electrical_overlay;
	const interiorPointMaxFlowOverlay = value.interior_point_max_flow_overlay;
	const minimumRatioCycleOverlay = value.minimum_ratio_cycle_overlay;
	const minimumRatioCycleMcfOverlay = value.minimum_ratio_cycle_mcf_overlay;
	const randomizedAlmostLinearMcfOverlay =
		value.randomized_almost_linear_mcf_overlay;
	const flowFrameworkMcfOverlay = value.flow_framework_mcf_overlay;
	const weightedAugmentingPathsOverlay =
		value.weighted_augmenting_paths_overlay;
	const weightedPushRelabelShortcutOverlay =
		value.weighted_push_relabel_shortcut_overlay;
	const randomizedAlmostLinearOverlay = value.randomized_almost_linear_overlay;
	const deterministicAlmostLinearOverlay =
		value.deterministic_almost_linear_overlay;
	const primalDualIpmMcfOverlay = value.primal_dual_ipm_mcf_overlay;
	const electricalIpmMcfOverlay = value.electrical_ipm_mcf_overlay;
	const dualNetworkSimplexOverlay = value.dual_network_simplex_overlay;
	const polynomialDualSimplexOverlay = value.polynomial_dual_simplex_overlay;
	const polynomialPrimalSimplexOverlay =
		value.polynomial_primal_simplex_overlay;
	const doubleScalingOverlay = value.double_scaling_overlay;
	const convexCostOverlay = value.convex_cost_overlay;
	const convexNetworkSimplexOverlay = value.convex_network_simplex_overlay;
	const predictionAssistedEpsilonOverlay =
		value.prediction_assisted_epsilon_overlay;
	const tardosFrameworkOverlay = value.tardos_framework_overlay;
	const traceEvent = decodeTraceEvent(value.trace_event);
	if (feasibilityOverlay !== undefined) {
		validateFeasibilityOverlayAgainstScene(
			feasibilityOverlay,
			model,
			nodes,
			edges,
			edgeStates,
			traceEvent,
		);
		if (
			pseudoflowForest !== undefined ||
			FLOW_OVERLAY_CONTRIBUTION_ENTRIES.some(
				({ field }) =>
					field !== "feasibility_overlay" && value[field] !== undefined,
			)
		) {
			throw new Error(
				"Feasibility boundary contains stale algorithm visualization state",
			);
		}
	}
	rejectSyntheticPrimaryWorkBoundary(traceEvent);
	validateTraceEventSemantics(
		value.trace_event_semantics,
		traceEvent,
		value.solve_status,
		value.trace_steps,
		value.metrics,
	);
	// A feasibility event belongs to its exact auxiliary kernel, not to the
	// selected parent algorithm's stage machine. Specialized validators still
	// receive an input boundary below so they check model compatibility without
	// demanding or accepting stale algorithm overlays.
	const algorithmStateTraceEvent =
		feasibilityOverlay === undefined ? traceEvent : undefined;
	const algorithmStateEventId =
		feasibilityOverlay === undefined ? value.event_id : "0";
	const algorithmStateSolveStatus =
		feasibilityOverlay === undefined ? value.solve_status : "ready";
	const terminalCertification = value.trace_event_semantics?.role === "certify";
	if (
		!terminalCertification &&
		((traceEvent?.minimum_granularity === "phase" &&
			value.trace_steps.phase_availability.availability === "unavailable") ||
			(traceEvent?.minimum_granularity === "operation" &&
				value.trace_steps.operation_availability.availability ===
					"unavailable") ||
			(traceEvent?.minimum_granularity === "micro" &&
				value.trace_steps.detail.availability === "unavailable"))
	) {
		throw new Error(
			"Flow trace event uses a boundary disabled by its step contract",
		);
	}
	const plainResourceLimitBoundary =
		pseudoflowForest === undefined &&
		FLOW_OVERLAY_CONTRIBUTION_ENTRIES.every(
			({ field }) => value[field] === undefined,
		) &&
		traceEvent === undefined &&
		value.outcome === undefined &&
		value.solve_status === "resource-limit" &&
		value.resource_limit_reason !== undefined &&
		value.event_id === "1" &&
		value.event_count === "1";
	if (
		(value.solve_status === "resource-limit") !==
			(value.resource_limit_reason !== undefined) ||
		(value.solve_status === "resource-limit" && !plainResourceLimitBoundary)
	) {
		throw new Error("Flow resource-limit boundary or reason is inconsistent");
	}
	const nodeIds = new Set(nodes.map((node) => node.id));
	const edgeById = new Map(edges.map((edge) => [edge.id, edge]));
	if (
		value.trace_event_semantics?.changed_entity_refs.some(
			(entity) => !traceEntityBelongsToGraph(entity, nodeIds, edgeById),
		)
	) {
		throw new Error("Flow scene changed entity does not match its graph");
	}
	const structuralOverlayCount =
		Number(pseudoflowForest !== undefined) +
		FLOW_OVERLAY_CONTRIBUTION_ENTRIES.filter(
			({ field, sceneGroup }) =>
				sceneGroup === "exclusive-forest" && value[field] !== undefined,
		).length;
	const validConvexPair =
		structuralOverlayCount === 2 &&
		convexCostOverlay !== undefined &&
		convexNetworkSimplexOverlay !== undefined;
	if (structuralOverlayCount > 1 && !validConvexPair) {
		throw new Error("Flow scene contains conflicting forest overlays");
	}
	if (pseudoflowForest !== undefined) {
		const forestArcIds = new Set<string>();
		const transportationEdgeIds = new Set<string>();
		const transportationChildren = new Set<string>();
		const componentParent = new Map(nodes.map((node) => [node.id, node.id]));
		const findComponent = (node: string): string => {
			let current = node;
			while (componentParent.get(current) !== current) {
				const next = componentParent.get(current);
				if (next === undefined) {
					throw new Error(
						"Transportation basis forest contains an unknown node",
					);
				}
				current = next;
			}
			return current;
		};
		for (const arc of pseudoflowForest.arcs) {
			const key = `${arc.edge_id}:${arc.direction}`;
			const edge = edgeById.get(arc.edge_id);
			if (edge === undefined || forestArcIds.has(key)) {
				throw new Error("Pseudoflow forest does not match the graph");
			}
			forestArcIds.add(key);
			if (model.kind === "transportation") {
				const from = arc.direction === "forward" ? edge.from : edge.to;
				const to = arc.direction === "forward" ? edge.to : edge.from;
				const fromRoot = findComponent(from);
				const toRoot = findComponent(to);
				if (
					transportationEdgeIds.has(arc.edge_id) ||
					transportationChildren.has(to) ||
					fromRoot === toRoot
				) {
					throw new Error(
						"Transportation basis overlay is not a rooted forest",
					);
				}
				transportationEdgeIds.add(arc.edge_id);
				transportationChildren.add(to);
				componentParent.set(toRoot, fromRoot);
			}
		}
		const strongNodes = new Set(pseudoflowForest.strong_nodes);
		if (
			strongNodes.size !== pseudoflowForest.strong_nodes.length ||
			pseudoflowForest.strong_nodes.some((node) => !nodeIds.has(node))
		) {
			throw new Error("Pseudoflow strong nodes do not match the graph");
		}
	}
	if (eibfsOverlay !== undefined) {
		if (
			value.algorithm.id !== "eibfs" &&
			value.algorithm.id !== "dynamic-eibfs"
		) {
			throw new Error("EIBFS overlay does not match the selected algorithm");
		}
		if (
			value.solve_status === "optimal" ||
			value.outcome !== undefined ||
			algorithmStateTraceEvent?.catalog_id ===
				"eibfs.begin-feasible-flow-recovery" ||
			algorithmStateTraceEvent?.catalog_id ===
				"eibfs.cancel-same-cut-positive-flow" ||
			algorithmStateTraceEvent?.catalog_id === "eibfs.optimal-feasible-flow"
		) {
			throw new Error(
				"EIBFS pseudoflow overlay cannot accompany recovered flow",
			);
		}
		validateEibfsOverlay(eibfsOverlay, model, nodes, nodeTraceStates, edgeById);
	}
	if (dynamicEibfsOverlay !== undefined) {
		validateDynamicEibfsOverlay(
			dynamicEibfsOverlay,
			model,
			value.algorithm.id,
			edgeById,
			eibfsOverlay,
		);
	} else if (
		value.algorithm.id === "dynamic-eibfs" &&
		value.solve_status !== "resource-limit" &&
		!(value.solve_status === "ready" && value.event_id === "0")
	) {
		throw new Error("Dynamic EIBFS scene is missing its update overlay");
	}
	if (model.kind !== "parametric-max-flow") {
		const stateIds = new Set<string>();
		for (const state of edgeStates) {
			const edge = edgeById.get(state.edge_id);
			const temporaryOverCapacity =
				dynamicEibfsOverlay?.stage === "apply-update" &&
				dynamicEibfsOverlay.changed_edge === state.edge_id;
			if (
				edge === undefined ||
				stateIds.has(state.edge_id) ||
				BigInt(state.flow) < BigInt(edge.lower) ||
				(BigInt(state.flow) > BigInt(edge.capacity) && !temporaryOverCapacity)
			) {
				throw new Error("Flow scene edge state does not match its graph edge");
			}
			stateIds.add(state.edge_id);
		}
		if (stateIds.size !== edgeById.size) {
			throw new Error("Flow scene does not contain exactly one state per edge");
		}
		const flowByEdge = new Map(
			edgeStates.map((state) => [state.edge_id, BigInt(state.flow)]),
		);
		const residualIds = new Set<string>();
		const residualFixedByEdge = new Map<string, boolean>();
		for (const arc of residualArcs) {
			const edge = edgeById.get(arc.edge_id);
			const flow = flowByEdge.get(arc.edge_id);
			const id = `${arc.edge_id}\u0000${arc.direction}`;
			if (edge === undefined || flow === undefined || residualIds.has(id)) {
				throw new Error(
					"Flow scene residual arc does not match its graph edge",
				);
			}
			residualIds.add(id);
			const forward = arc.direction === "forward";
			const expectedFrom = forward ? edge.from : edge.to;
			const expectedTo = forward ? edge.to : edge.from;
			const expectedCapacity = forward
				? BigInt(edge.capacity) > flow
					? BigInt(edge.capacity) - flow
					: 0n
				: flow - BigInt(edge.lower);
			const expectedCost = forward ? BigInt(edge.cost) : -BigInt(edge.cost);
			if (
				arc.from !== expectedFrom ||
				arc.to !== expectedTo ||
				BigInt(arc.capacity) !== expectedCapacity ||
				BigInt(arc.cost) !== expectedCost
			) {
				throw new Error("Flow scene residual values are inconsistent");
			}
			if (
				residualFixedByEdge.has(arc.edge_id) &&
				residualFixedByEdge.get(arc.edge_id) !== arc.fixed
			) {
				throw new Error(
					"Flow scene residual directions disagree on fixed state",
				);
			}
			residualFixedByEdge.set(arc.edge_id, arc.fixed === true);
		}
		if (
			value.algorithm.id !== "electrical-flow" &&
			value.algorithm.id !== "minimum-ratio-cycle-max-flow" &&
			value.algorithm.id !== "minimum-ratio-cycle-mcf" &&
			residualIds.size !== edges.length * 2
		) {
			throw new Error(
				"Flow scene must contain both residual directions per edge",
			);
		}
		if (
			eibfsOverlay?.forest_arcs.some((relation) => {
				const residual = residualArcs.find(
					(arc) =>
						arc.edge_id === relation.admissible_residual.edge_id &&
						arc.direction === relation.admissible_residual.direction,
				);
				const disappearingChangedParent =
					dynamicEibfsOverlay?.stage === "apply-update" &&
					dynamicEibfsOverlay.changed_edge ===
						relation.admissible_residual.edge_id;
				return (
					residual === undefined ||
					(BigInt(residual.capacity) <= 0n && !disappearingChangedParent)
				);
			})
		) {
			throw new Error("EIBFS parent residual is not positive");
		}
		const nodeTraceIds = new Set<string>();
		const orderedNodeIds = canonicalNodeIds(nodes);
		for (const [index, state] of nodeTraceStates.entries()) {
			if (
				!nodeIds.has(state.node_id) ||
				nodeTraceIds.has(state.node_id) ||
				state.node_id !== orderedNodeIds[index]
			) {
				throw new Error(
					"Flow scene node trace state does not match its graph node",
				);
			}
			nodeTraceIds.add(state.node_id);
		}
		if (nodeTraceIds.size !== nodeIds.size) {
			throw new Error("Flow scene must contain one trace state per node");
		}
	}
	if (
		(traceEvent !== undefined && traceEvent.event_id !== value.event_id) ||
		(value.event_id === "0" && traceEvent !== undefined) ||
		(value.run_profile === "trace" &&
			(value.solve_status === "running" ||
				value.solve_status === "primitive-complete" ||
				value.solve_status === "optimal" ||
				value.solve_status === "infeasible") &&
			BigInt(value.event_id) > 0n &&
			traceEvent === undefined &&
			!(
				parametricOverlay !== undefined && value.solve_status !== "infeasible"
			)) ||
		(traceEvent?.parent_phase_id !== undefined &&
			BigInt(traceEvent.parent_phase_id) >= BigInt(traceEvent.event_id)) ||
		traceEvent?.entity_refs.some(
			(entity) => !traceEntityBelongsToGraph(entity, nodeIds, edgeById),
		)
	) {
		throw new Error(
			"Flow scene trace event does not match its current boundary",
		);
	}
	const outcome = decodeOutcome(value.outcome);
	if (outcome !== undefined && !flowOutcomeMatchesModel(model, outcome)) {
		throw new Error("Flow scene outcome does not match its problem model");
	}
	validateBipartiteScene(model, nodes, edges, edgeStates, outcome);
	validateAssignmentScene(model, nodes, edges, edgeStates, outcome);
	validateTransportationScene(model, nodes, edges, edgeStates, outcome);
	validatePlanarScene(model, nodes, edges);
	if (!plainResourceLimitBoundary) {
		validateParametricScene(
			model,
			nodes,
			edges,
			edgeStates,
			residualArcs,
			nodeTraceStates,
			value.algorithm.id,
			algorithmStateSolveStatus,
			parametricOverlay,
			outcome,
		);
		validateBinaryBlockingScene(
			model,
			nodes,
			edges,
			residualArcs,
			value.algorithm.id,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			binaryBlockingOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateCancelTightenScene(
			model,
			nodes,
			residualArcs,
			value.algorithm.id,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			cancelTightenOverlay,
			algorithmStateTraceEvent,
		);
		validateRelaxedMndcScene(
			model,
			nodes,
			residualArcs,
			value.algorithm.id,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			relaxedMndcOverlay,
			algorithmStateTraceEvent,
		);
		validateEnhancedCapacityScalingScene(
			model,
			nodes,
			edges,
			value.algorithm.id,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			enhancedCapacityScalingOverlay,
			algorithmStateTraceEvent,
		);
		validateOrlinMcfScene(
			model,
			nodes,
			edges,
			value.algorithm.id,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			orlinMcfOverlay,
			algorithmStateTraceEvent,
		);
		validateOrlinMaxFlowScene(
			model,
			nodes,
			edges,
			residualArcs,
			value.algorithm.id,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			orlinMaxFlowOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateElectricalFlowScene(
			model,
			nodes,
			edges,
			edgeStates,
			residualArcs,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			electricalFlowOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateAugmentingElectricalScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			augmentingElectricalOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateInteriorPointMaxFlowScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			interiorPointMaxFlowOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateMinimumRatioCycleScene(
			model,
			nodes,
			edges,
			edgeStates,
			residualArcs,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			minimumRatioCycleOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateMinimumRatioCycleMcfScene(
			model,
			nodes,
			edges,
			edgeStates,
			residualArcs,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			minimumRatioCycleMcfOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateRandomizedAlmostLinearMcfScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			randomizedAlmostLinearMcfOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateFlowFrameworkMcfScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			flowFrameworkMcfOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateWeightedAugmentingPathsScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			weightedAugmentingPathsOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateWeightedPushRelabelShortcutScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			weightedPushRelabelShortcutOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateRandomizedAlmostLinearScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			randomizedAlmostLinearOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateDeterministicAlmostLinearScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			deterministicAlmostLinearOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validatePrimalDualIpmMcfScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			primalDualIpmMcfOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateElectricalIpmMcfScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			electricalIpmMcfOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateDualNetworkSimplexScene(
			model,
			nodes,
			edges,
			value.algorithm.id,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			dualNetworkSimplexOverlay,
			algorithmStateTraceEvent,
		);
		validatePolynomialDualSimplexScene(
			model,
			nodes,
			edges,
			value.algorithm.id,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			polynomialDualSimplexOverlay,
			algorithmStateTraceEvent,
		);
		validatePolynomialPrimalSimplexScene(
			model,
			nodes,
			edges,
			value.algorithm.id,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			polynomialPrimalSimplexOverlay,
			algorithmStateTraceEvent,
		);
		validateDoubleScalingScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			doubleScalingOverlay,
			algorithmStateTraceEvent,
		);
		validateConvexCostScene(
			model,
			edges,
			edgeStates,
			value.algorithm.id,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			convexCostOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
		validateConvexNetworkSimplexScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			convexCostOverlay,
			convexNetworkSimplexOverlay,
			algorithmStateTraceEvent,
		);
		validatePredictionAssistedEpsilonScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			predictionAssistedEpsilonOverlay,
			algorithmStateTraceEvent,
		);
		validateTardosFrameworkScene(
			model,
			nodes,
			edges,
			edgeStates,
			value.algorithm.id,
			value.algorithm.config,
			algorithmStateSolveStatus,
			algorithmStateEventId,
			tardosFrameworkOverlay,
			algorithmStateTraceEvent,
			outcome,
		);
	}
	if (
		((value.solve_status === "ready" || value.solve_status === "running") &&
			outcome !== undefined) ||
		(value.solve_status === "optimal" && outcome === undefined) ||
		(value.solve_status === "primitive-complete" &&
			outcome?.kind !== "binary-blocking-flow" &&
			outcome?.kind !== "tardos-framework" &&
			outcome?.kind !== "electrical-flow" &&
			outcome?.kind !== "minimum-ratio-cycle" &&
			outcome?.kind !== "minimum-ratio-cycle-mcf") ||
		(outcome?.kind === "binary-blocking-flow" &&
			value.solve_status !== "primitive-complete") ||
		(outcome?.kind === "tardos-framework" &&
			value.solve_status !== "primitive-complete") ||
		(outcome?.kind === "electrical-flow" &&
			value.solve_status !== "primitive-complete") ||
		(outcome?.kind === "minimum-ratio-cycle" &&
			value.solve_status !== "primitive-complete") ||
		(outcome?.kind === "minimum-ratio-cycle-mcf" &&
			value.solve_status !== "primitive-complete") ||
		(outcome?.kind === "assignment" && value.solve_status !== "optimal") ||
		(outcome?.kind === "assignment-infeasible" &&
			value.solve_status !== "infeasible") ||
		(outcome?.kind === "infeasible" && value.solve_status !== "infeasible") ||
		(value.solve_status === "infeasible" &&
			outcome?.kind !== "infeasible" &&
			outcome?.kind !== "assignment-infeasible") ||
		(outcome?.kind === "max-flow" &&
			(new Set(outcome.source_side).size !== outcome.source_side.length ||
				outcome.source_side.some((node) => !nodeIds.has(node)))) ||
		(outcome?.kind === "min-cost-flow" &&
			(new Set(outcome.potentials.map((item) => item.node_id)).size !==
				outcome.potentials.length ||
				outcome.potentials.length !== nodeIds.size ||
				outcome.potentials.some((item) => !nodeIds.has(item.node_id)))) ||
		(outcome?.kind === "min-cost-max-flow" &&
			(new Set(outcome.source_side).size !== outcome.source_side.length ||
				outcome.source_side.some((node) => !nodeIds.has(node)) ||
				new Set(outcome.potentials.map((item) => item.node_id)).size !==
					outcome.potentials.length ||
				outcome.potentials.length !== nodeIds.size ||
				outcome.potentials.some((item) => !nodeIds.has(item.node_id)))) ||
		(outcome?.kind === "bipartite-matching" &&
			(model.kind !== "bipartite-matching" ||
				BigInt(outcome.cardinality) !== BigInt(outcome.pairs.length) ||
				outcome.cover_left.length + outcome.cover_right.length !==
					outcome.pairs.length ||
				outcome.cover_left.some((node) => !model.left.includes(node)) ||
				outcome.cover_right.some((node) => !model.right.includes(node)) ||
				outcome.pairs.some((pair) => {
					const edge = edgeById.get(pair.edge_id);
					return (
						edge === undefined ||
						edge.from !== pair.left ||
						edge.to !== pair.right ||
						!model.left.includes(pair.left) ||
						!model.right.includes(pair.right)
					);
				}))) ||
		(outcome?.kind === "assignment" && model.kind !== "assignment") ||
		(outcome?.kind === "assignment-infeasible" &&
			model.kind !== "assignment") ||
		(model.kind === "assignment" &&
			outcome !== undefined &&
			outcome.kind !== "assignment" &&
			outcome.kind !== "assignment-infeasible") ||
		(model.kind === "transportation" &&
			outcome !== undefined &&
			outcome.kind !== "min-cost-flow" &&
			outcome.kind !== "infeasible") ||
		(model.kind === "planar-max-flow" &&
			outcome !== undefined &&
			outcome.kind !== "max-flow" &&
			outcome.kind !== "infeasible") ||
		(model.kind === "convex-cost-flow" &&
			outcome !== undefined &&
			outcome.kind !== "min-cost-flow" &&
			outcome.kind !== "infeasible") ||
		(outcome?.kind === "infeasible" &&
			(new Set(outcome.reachable_original_nodes).size !==
				outcome.reachable_original_nodes.length ||
				outcome.reachable_original_nodes.some((node) => !nodeIds.has(node))))
	) {
		throw new Error(
			"Flow scene outcome does not match its graph or solve status",
		);
	}
	return value;
}

export type LegacyFlowSceneMigrationCatalog = readonly Readonly<{
	id: string;
	trace_steps: FlowAlgorithmStepContractV1;
}>[];

function canonicalJson(value: unknown): string {
	if (Array.isArray(value)) {
		return `[${value.map(canonicalJson).join(",")}]`;
	}
	if (isRecord(value)) {
		return `{${Object.keys(value)
			.sort()
			.map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
			.join(",")}}`;
	}
	return JSON.stringify(value) ?? "undefined";
}

async function sha256Hex(value: string): Promise<string> {
	const digest = await crypto.subtle.digest(
		"SHA-256",
		utf8Encoder.encode(value),
	);
	return Array.from(new Uint8Array(digest), (byte) =>
		byte.toString(16).padStart(2, "0"),
	).join("");
}

async function legacyTraceStepsFromCatalog(
	value: Record<string, unknown>,
	catalog: LegacyFlowSceneMigrationCatalog,
): Promise<FlowAlgorithmStepContractV1 | undefined> {
	if (!isRecord(value.algorithm) || typeof value.algorithm.id !== "string") {
		return undefined;
	}
	const algorithmId = value.algorithm.id;
	const matches = catalog.filter((entry) => entry.id === algorithmId);
	const canonicalTraceSteps = canonicalJson(value.trace_steps);
	const legacyCatalogTraceSteps = (() => {
		const traceSteps = matches[0]?.trace_steps;
		if (traceSteps === undefined) return undefined;
		const { visualization: _visualization, ...legacyPrimaryWork } =
			traceSteps.primary_work;
		return {
			...traceSteps,
			primary_work: legacyPrimaryWork,
		};
	})();
	const trustedDigest = FLOW_LEGACY_TRACE_STEP_DIGESTS.get(algorithmId);
	if (
		matches.length !== 1 ||
		trustedDigest === undefined ||
		legacyCatalogTraceSteps === undefined ||
		canonicalTraceSteps !== canonicalJson(legacyCatalogTraceSteps) ||
		(await sha256Hex(canonicalTraceSteps)) !== trustedDigest
	) {
		return undefined;
	}
	return matches[0]?.trace_steps;
}

export async function migrateFlowCurrentSceneV7(
	payload: Uint8Array,
	catalog: LegacyFlowSceneMigrationCatalog,
): Promise<FlowCurrentSceneV9> {
	const source = new TextDecoder("utf-8", { fatal: true }).decode(payload);
	const value: unknown = JSON.parse(source);
	const traceSteps = isRecord(value)
		? await legacyTraceStepsFromCatalog(value, catalog)
		: undefined;
	if (
		!isRecord(value) ||
		value.result_schema_version !== 7 ||
		value.frame_revision !== "flow-scene/7" ||
		Object.hasOwn(value, "parametric_overlay") ||
		(isRecord(value.model) && value.model.kind === "parametric-max-flow") ||
		(isRecord(value.outcome) && value.outcome.kind === "parametric-max-flow") ||
		traceSteps === undefined ||
		value.trace_event !== undefined
	) {
		throw new Error("Flow scene V7 migration input is invalid");
	}
	return decodeFlowCurrentSceneV9(
		new TextEncoder().encode(
			JSON.stringify({
				...value,
				result_schema_version: 9,
				frame_revision: "flow-scene/9",
				trace_steps: traceSteps,
			}),
		),
	);
}

export async function migrateFlowCurrentSceneV6(
	payload: Uint8Array,
	catalog: LegacyFlowSceneMigrationCatalog,
): Promise<FlowCurrentSceneV9> {
	const source = new TextDecoder("utf-8", { fatal: true }).decode(payload);
	const value: unknown = JSON.parse(source);
	if (
		!isRecord(value) ||
		value.result_schema_version !== 6 ||
		value.frame_revision !== "flow-scene/6" ||
		Object.hasOwn(value, "dynamic_eibfs_overlay")
	) {
		throw new Error("Flow scene V6 migration input is invalid");
	}
	return await migrateFlowCurrentSceneV7(
		new TextEncoder().encode(
			JSON.stringify({
				...value,
				result_schema_version: 7,
				frame_revision: "flow-scene/7",
			}),
		),
		catalog,
	);
}
