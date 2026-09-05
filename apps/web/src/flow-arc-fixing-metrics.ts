export type ArcFixingMetricProjection = Readonly<{
	fixingPasses: string;
	arcsUnfixed: string;
	residualArcScans: string;
	arcsFixed: string;
	fixIns: string;
	refinePhases: string;
	recoveries: string;
	relabels: string;
	fixedArcSkips: string;
	currentArcAdvances: string;
	initialSaturations: string;
	pushes: string;
	saturatingPushes: string;
	nonsaturatingPushes: string;
	discharges: string;
	activeVertexSelections: string;
}>;

/**
 * Gives the Arc Fixing wire counters domain names at one audited boundary.
 * The scene decoder guarantees an exact 16-counter tuple before this runs.
 */
export function projectArcFixingMetrics(
	metrics: readonly string[],
): ArcFixingMetricProjection {
	if (metrics.length !== 16) {
		throw new Error("Arc Fixing requires exactly 16 metrics");
	}
	const [
		fixingPasses,
		arcsUnfixed,
		residualArcScans,
		arcsFixed,
		fixIns,
		refinePhases,
		recoveries,
		relabels,
		fixedArcSkips,
		currentArcAdvances,
		initialSaturations,
		pushes,
		saturatingPushes,
		nonsaturatingPushes,
		discharges,
		activeVertexSelections,
	] = metrics;
	if (
		fixingPasses === undefined ||
		arcsUnfixed === undefined ||
		residualArcScans === undefined ||
		arcsFixed === undefined ||
		fixIns === undefined ||
		refinePhases === undefined ||
		recoveries === undefined ||
		relabels === undefined ||
		fixedArcSkips === undefined ||
		currentArcAdvances === undefined ||
		initialSaturations === undefined ||
		pushes === undefined ||
		saturatingPushes === undefined ||
		nonsaturatingPushes === undefined ||
		discharges === undefined ||
		activeVertexSelections === undefined
	) {
		throw new Error("Arc Fixing metrics are incomplete");
	}
	return {
		fixingPasses,
		arcsUnfixed,
		residualArcScans,
		arcsFixed,
		fixIns,
		refinePhases,
		recoveries,
		relabels,
		fixedArcSkips,
		currentArcAdvances,
		initialSaturations,
		pushes,
		saturatingPushes,
		nonsaturatingPushes,
		discharges,
		activeVertexSelections,
	};
}
