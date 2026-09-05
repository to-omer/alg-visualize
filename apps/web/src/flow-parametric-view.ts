import type {
	FlowCurrentSceneV9,
	FlowParametricBreakpointV1,
	FlowParametricSegmentV1,
	FlowRationalV1,
} from "./flow-scene";

type RawRational = Readonly<{ numerator: bigint; denominator: bigint }>;

export type FlowParametricChartSegment = Readonly<{
	lowerLabel: string;
	upperLabel: string;
	formula: string;
	x1: number;
	x2: number;
	y1: number;
	y2: number;
	tied: boolean;
	minimalSourceSide: readonly string[];
	maximalSourceSide: readonly string[];
}>;

export type FlowParametricChartBreakpoint = Readonly<{
	parameterLabel: string;
	x: number;
	y: number;
	enteringNodes: readonly string[];
	tied: boolean;
}>;

export type FlowParametricChartProjection = Readonly<{
	domainMinimumLabel: string;
	domainMaximumLabel: string;
	valueMinimumLabel: string;
	valueMaximumLabel: string;
	currentParameterLabel: string;
	currentX: number;
	segments: readonly FlowParametricChartSegment[];
	breakpoints: readonly FlowParametricChartBreakpoint[];
}>;

export type FlowParametricCutProjection = Readonly<{
	minimalSourceSide: readonly string[];
	maximalSourceSide: readonly string[];
	tiedNodes: readonly string[];
}>;

function raw(value: FlowRationalV1): RawRational {
	return {
		numerator: BigInt(value.numerator),
		denominator: BigInt(value.denominator),
	};
}

function rawInteger(value: string): RawRational {
	return { numerator: BigInt(value), denominator: 1n };
}

function subtract(left: RawRational, right: RawRational): RawRational {
	return {
		numerator:
			left.numerator * right.denominator - right.numerator * left.denominator,
		denominator: left.denominator * right.denominator,
	};
}

function evaluate(
	segment: FlowParametricSegmentV1,
	parameter: FlowRationalV1,
): RawRational {
	const at = raw(parameter);
	return {
		numerator:
			BigInt(segment.intercept) * at.denominator +
			BigInt(segment.slope) * at.numerator,
		denominator: at.denominator,
	};
}

function compare(left: RawRational, right: RawRational): number {
	const difference =
		left.numerator * right.denominator - right.numerator * left.denominator;
	return difference < 0n ? -1 : difference > 0n ? 1 : 0;
}

function greatestCommonDivisor(left: bigint, right: bigint): bigint {
	let a = left < 0n ? -left : left;
	let b = right < 0n ? -right : right;
	while (b !== 0n) {
		const remainder = a % b;
		a = b;
		b = remainder;
	}
	return a;
}

function formatRaw(value: RawRational): string {
	const divisor = greatestCommonDivisor(value.numerator, value.denominator);
	const numerator = value.numerator / divisor;
	const denominator = value.denominator / divisor;
	return denominator === 1n ? `${numerator}` : `${numerator}/${denominator}`;
}

function equalNodeSets(
	left: readonly string[],
	right: readonly string[],
): boolean {
	return (
		left.length === right.length &&
		left.every((node, index) => node === right[index])
	);
}

function boundedRatio(
	value: RawRational,
	minimum: RawRational,
	maximum: RawRational,
): number {
	const range = subtract(maximum, minimum);
	if (range.numerator === 0n) return 0.5;
	const offset = subtract(value, minimum);
	const numerator = offset.numerator * range.denominator;
	const denominator = offset.denominator * range.numerator;
	if (numerator <= 0n) return 0;
	if (numerator >= denominator) return 1;
	const scale = 1_000_000_000n;
	return Number((numerator * scale) / denominator) / Number(scale);
}

function formula(segment: FlowParametricSegmentV1): string {
	if (segment.slope === "0") return segment.intercept;
	if (segment.intercept === "0") {
		return segment.slope === "1"
			? "λ"
			: segment.slope === "-1"
				? "−λ"
				: `${segment.slope}λ`;
	}
	const slope = BigInt(segment.slope);
	const slopeText =
		slope === 1n
			? "+ λ"
			: slope === -1n
				? "− λ"
				: `${slope > 0n ? "+" : "−"} ${slope < 0n ? -slope : slope}λ`;
	return `${segment.intercept} ${slopeText}`;
}

function segmentAt(
	segments: readonly FlowParametricSegmentV1[],
	parameter: FlowRationalV1,
): FlowParametricSegmentV1 | undefined {
	const target = raw(parameter);
	return segments.find(
		(segment) =>
			compare(raw(segment.lower), target) <= 0 &&
			compare(target, raw(segment.upper)) <= 0,
	);
}

function valueAtBreakpoint(
	breakpoint: FlowParametricBreakpointV1,
	segments: readonly FlowParametricSegmentV1[],
): RawRational {
	const segment = segmentAt(segments, breakpoint.parameter);
	return segment === undefined
		? rawInteger("0")
		: evaluate(segment, breakpoint.parameter);
}

export function formatFlowRational(value: FlowRationalV1): string {
	return value.denominator === "1"
		? value.numerator
		: `${value.numerator}/${value.denominator}`;
}

export function sumFlowRationals(values: readonly FlowRationalV1[]): string {
	const total = values.reduce<RawRational>((sum, value) => {
		const next = raw(value);
		const divisor = greatestCommonDivisor(sum.denominator, next.denominator);
		const leftScale = next.denominator / divisor;
		const rightScale = sum.denominator / divisor;
		return {
			numerator: sum.numerator * leftScale + next.numerator * rightScale,
			denominator: sum.denominator * leftScale,
		};
	}, rawInteger("0"));
	return formatRaw(total);
}

export function compareFlowRational(
	left: FlowRationalV1,
	right: FlowRationalV1,
): number {
	return compare(raw(left), raw(right));
}

export function projectFlowParametricCut(
	scene: FlowCurrentSceneV9,
): FlowParametricCutProjection | undefined {
	const overlay = scene.parametric_overlay;
	if (scene.model.kind !== "parametric-max-flow" || overlay === undefined) {
		return undefined;
	}
	const breakpoint = overlay.recorded_breakpoints.find(
		(candidate) =>
			compareFlowRational(candidate.parameter, overlay.parameter) === 0,
	);
	const segment = segmentAt(overlay.recorded_segments, overlay.parameter);
	const minimalSourceSide =
		breakpoint?.exact_minimal_source_side ?? segment?.minimal_source_side ?? [];
	const maximalSourceSide =
		breakpoint?.exact_maximal_source_side ?? segment?.maximal_source_side ?? [];
	const minimal = new Set(minimalSourceSide);
	return {
		minimalSourceSide,
		maximalSourceSide,
		tiedNodes: maximalSourceSide.filter((node) => !minimal.has(node)),
	};
}

/**
 * Projects exact parametric values into [0, 1] chart coordinates. All ordering
 * and ratio arithmetic stays in BigInt; Number conversion happens only after
 * the result has been bounded to nine decimal digits.
 */
export function projectFlowParametricChart(
	scene: FlowCurrentSceneV9,
): FlowParametricChartProjection | undefined {
	if (
		scene.model.kind !== "parametric-max-flow" ||
		scene.parametric_overlay === undefined
	) {
		return undefined;
	}
	const { model, parametric_overlay: overlay } = scene;
	const domainMinimum = raw(model.parameter.minimum);
	const domainMaximum = raw(model.parameter.maximum);
	const endpointValues = overlay.recorded_segments.flatMap((segment) => [
		evaluate(segment, segment.lower),
		evaluate(segment, segment.upper),
	]);
	const valueMinimum = endpointValues.reduce(
		(minimum, value) => (compare(value, minimum) < 0 ? value : minimum),
		endpointValues[0] ?? rawInteger("0"),
	);
	const valueMaximum = endpointValues.reduce(
		(maximum, value) => (compare(value, maximum) > 0 ? value : maximum),
		endpointValues[0] ?? rawInteger("1"),
	);
	const x = (parameter: FlowRationalV1) =>
		boundedRatio(raw(parameter), domainMinimum, domainMaximum);
	const y = (value: RawRational) =>
		1 - boundedRatio(value, valueMinimum, valueMaximum);

	return {
		domainMinimumLabel: formatFlowRational(model.parameter.minimum),
		domainMaximumLabel: formatFlowRational(model.parameter.maximum),
		valueMinimumLabel: formatRaw(valueMinimum),
		valueMaximumLabel: formatRaw(valueMaximum),
		currentParameterLabel: formatFlowRational(overlay.parameter),
		currentX: x(overlay.parameter),
		segments: overlay.recorded_segments.map((segment) => ({
			lowerLabel: formatFlowRational(segment.lower),
			upperLabel: formatFlowRational(segment.upper),
			formula: formula(segment),
			x1: x(segment.lower),
			x2: x(segment.upper),
			y1: y(evaluate(segment, segment.lower)),
			y2: y(evaluate(segment, segment.upper)),
			tied: !equalNodeSets(
				segment.minimal_source_side,
				segment.maximal_source_side,
			),
			minimalSourceSide: segment.minimal_source_side,
			maximalSourceSide: segment.maximal_source_side,
		})),
		breakpoints: overlay.recorded_breakpoints.map((breakpoint) => ({
			parameterLabel: formatFlowRational(breakpoint.parameter),
			x: x(breakpoint.parameter),
			y: y(valueAtBreakpoint(breakpoint, overlay.recorded_segments)),
			enteringNodes: breakpoint.entering_nodes,
			tied: !equalNodeSets(
				breakpoint.exact_minimal_source_side,
				breakpoint.exact_maximal_source_side,
			),
		})),
	};
}
