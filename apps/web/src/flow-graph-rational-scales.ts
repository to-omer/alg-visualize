import type { FlowOrlinMcfComponentV1, FlowRationalV1 } from "./flow-scene";
import { absoluteBigInt } from "./flow-visual-scales";

export function exactRational(value: FlowRationalV1): {
	numerator: bigint;
	denominator: bigint;
} {
	return {
		numerator: BigInt(value.numerator),
		denominator: BigInt(value.denominator),
	};
}

/** Exact 3Δ/4 quotient-component gate used by enhanced capacity scaling. */
export function enhancedCapacityScalingGateStatus(
	excess: FlowRationalV1,
	delta: FlowRationalV1,
): "excess" | "deficit" | "below" {
	return threeQuarterDeltaGateStatus(excess, delta);
}

/** Exact signed 3Δ/4 gate shared by quotient-capacity scaling phases. */
export function threeQuarterDeltaGateStatus(
	excess: FlowRationalV1,
	delta: FlowRationalV1,
): "excess" | "deficit" | "below" {
	const excessNumerator = BigInt(excess.numerator);
	const excessDenominator = BigInt(excess.denominator);
	const deltaNumerator = BigInt(delta.numerator);
	const deltaDenominator = BigInt(delta.denominator);
	const scaledExcess = 4n * excessNumerator * deltaDenominator;
	const scaledDelta = 3n * deltaNumerator * excessDenominator;
	if (scaledExcess >= scaledDelta) return "excess";
	if (scaledExcess <= -scaledDelta) return "deficit";
	return "below";
}

export function orlinMcfPhaseGateStatus(
	excess: FlowRationalV1,
	delta: FlowRationalV1,
): "excess" | "deficit" | "below" {
	return threeQuarterDeltaGateStatus(excess, delta);
}

/**
 * Returns one exact maximum-|excess| witness only when every quotient
 * component is below Orlin's 3Δ/4 phase gate.
 */
export function orlinMcfBelowGateWitness(
	components: readonly FlowOrlinMcfComponentV1[],
	delta: FlowRationalV1,
): FlowOrlinMcfComponentV1 | undefined {
	if (
		components.length === 0 ||
		components.some(
			(component) =>
				orlinMcfPhaseGateStatus(component.excess, delta) !== "below",
		)
	) {
		return undefined;
	}
	return components.reduce((best, component) => {
		const componentMagnitude =
			absoluteBigInt(BigInt(component.excess.numerator)) *
			BigInt(best.excess.denominator);
		const bestMagnitude =
			absoluteBigInt(BigInt(best.excess.numerator)) *
			BigInt(component.excess.denominator);
		if (componentMagnitude > bestMagnitude) return component;
		if (
			componentMagnitude === bestMagnitude &&
			component.component_id < best.component_id
		) {
			return component;
		}
		return best;
	});
}

/** Exact strict e(v) > Δ active-node gate used by Scaling-Simplex. */
export function polynomialDualScaleGateStatus(
	excess: FlowRationalV1,
	delta: FlowRationalV1,
): "active" | "below" {
	const left = BigInt(excess.numerator) * BigInt(delta.denominator);
	const right = BigInt(delta.numerator) * BigInt(excess.denominator);
	return left > right ? "active" : "below";
}

export function rationalMagnitudeStrokeWidth(
	value: FlowRationalV1,
	maximum: FlowRationalV1,
): number {
	const numerator = absoluteBigInt(BigInt(value.numerator));
	const maximumNumerator = absoluteBigInt(BigInt(maximum.numerator));
	if (numerator === 0n || maximumNumerator === 0n) return 2;
	const scaled =
		(numerator * BigInt(maximum.denominator) * 4_000n) /
		(BigInt(value.denominator) * maximumNumerator);
	return 2 + Number(scaled) / 1_000;
}

export function rationalCapacityBand(
	value: FlowRationalV1,
	maximum: FlowRationalV1,
): number {
	const left = BigInt(value.numerator) * BigInt(maximum.denominator);
	const right = BigInt(maximum.numerator) * BigInt(value.denominator);
	if (right <= 0n) return 0;
	const scaled = (left * 4n + right - 1n) / right;
	return Math.max(1, Math.min(4, Number(scaled)));
}
