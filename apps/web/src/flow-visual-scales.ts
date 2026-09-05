export type ExactVisualRational = Readonly<{
	numerator: bigint;
	denominator: bigint;
}>;

export function absoluteBigInt(value: bigint): bigint {
	return value < 0n ? -value : value;
}

export function costMagnitudeBand(
	cost: bigint,
	maximumAbsoluteCost: bigint,
): number {
	const magnitude = absoluteBigInt(cost);
	if (magnitude === 0n || maximumAbsoluteCost === 0n) return 0;
	return Number(
		(magnitude * 4n + maximumAbsoluteCost - 1n) / maximumAbsoluteCost,
	);
}

/** Returns a continuous, bigint-safe cost intensity in the inclusive 0..1 range. */
export function costMagnitudeIntensity(
	cost: bigint,
	maximumAbsoluteCost: bigint,
): number {
	const magnitude = absoluteBigInt(cost);
	if (magnitude === 0n || maximumAbsoluteCost === 0n) return 0;
	const bounded =
		magnitude > maximumAbsoluteCost ? maximumAbsoluteCost : magnitude;
	return Number((bounded * 10_000n) / maximumAbsoluteCost) / 10_000;
}

function approximateLog2(value: bigint): number {
	if (value <= 0n) return Number.NEGATIVE_INFINITY;
	const bits = value.toString(2).length;
	const shift = Math.max(0, bits - 53);
	const leading = Number(value >> BigInt(shift));
	return Math.log2(leading) + shift;
}

function log2OnePlus(value: ExactVisualRational): number {
	if (value.numerator <= 0n) return 0;
	return (
		approximateLog2(value.numerator + value.denominator) -
		approximateLog2(value.denominator)
	);
}

function compareExactRational(
	left: ExactVisualRational,
	right: ExactVisualRational,
): number {
	const difference =
		left.numerator * right.denominator - right.numerator * left.denominator;
	return difference < 0n ? -1 : difference > 0n ? 1 : 0;
}

/** Maps an arbitrary-precision exact capacity to a bounded stable rail width. */
export function rationalCapacityRailWidth(
	capacity: ExactVisualRational,
	maximumCapacity: ExactVisualRational,
): number {
	if (capacity.numerator <= 0n) return 4;
	if (maximumCapacity.numerator <= 0n) return 4;
	const bounded =
		compareExactRational(capacity, maximumCapacity) > 0
			? maximumCapacity
			: capacity;
	const denominator = log2OnePlus(maximumCapacity);
	if (!Number.isFinite(denominator) || denominator <= 0) return 10;
	const ratio = log2OnePlus(bounded) / denominator;
	return 4 + Math.max(0, Math.min(1, ratio)) * 6;
}

/** Maps a strict u64 capacity to a bounded neutral-rail width. */
export function capacityRailWidth(
	capacity: bigint,
	maximumCapacity: bigint,
): number {
	return rationalCapacityRailWidth(
		{ numerator: capacity, denominator: 1n },
		{ numerator: maximumCapacity, denominator: 1n },
	);
}

/** Maps current flow to the same bounded inner-fill width used by every LOD. */
export function flowFillStrokeWidth(
	flow: bigint,
	capacity: bigint,
	capacityWidth: number,
): number {
	if (flow <= 0n || capacity <= 0n) return 0;
	const boundedFlow = flow > capacity ? capacity : flow;
	const proportionalWidth =
		1.5 + Number((boundedFlow * 4_500n) / capacity) / 1_000;
	return Math.max(0, Math.min(proportionalWidth, capacityWidth - 1.5));
}
