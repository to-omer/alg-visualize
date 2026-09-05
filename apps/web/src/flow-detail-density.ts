const WORK_PROGRESS_RESOLUTION = 1000n;

/**
 * Maps an exact decimal counter ratio to a bounded visual progress value.
 * Exact counters remain in the text readout and are never converted to a
 * floating-point value for algorithm or navigation decisions.
 */
export function primaryWorkProgressValue(
	completedText: string,
	totalText: string,
): number {
	const completed = BigInt(completedText);
	const total = BigInt(totalText);
	if (total === 0n) return 0;
	if (completed < 0n || completed > total) {
		throw new RangeError("primary work progress is outside its exact total");
	}
	return Number((completed * WORK_PROGRESS_RESOLUTION) / total);
}

export const PRIMARY_WORK_PROGRESS_MAX = Number(WORK_PROGRESS_RESOLUTION);
