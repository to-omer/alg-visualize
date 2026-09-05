export type FlowDistanceDirectedMetricRow = {
	label: string;
	value: string;
};

export function projectDistanceDirectedMetrics(
	metrics: readonly string[],
): FlowDistanceDirectedMetricRow[] {
	if (metrics.length !== 16) {
		throw new Error(
			"Distance-directed metrics require the flow-metrics/6 vector",
		);
	}
	return [
		{
			label: "Exact-tree BFS / scaling phases",
			value: `${metrics[9]} / ${metrics[5]}`,
		},
		{
			label: "Tree repairs / invalid parents",
			value: `${metrics[4]} / ${metrics[8]}`,
		},
		{
			label: "Parent replacements / relabels",
			value: `${metrics[11]} / ${metrics[7]}`,
		},
		{
			label: "Deleted nodes / cascaded children",
			value: `${metrics[10]} / ${metrics[13]}`,
		},
		{
			label: "Saturated tree arcs / current-arc advances",
			value: `${metrics[12]} / ${metrics[1]}`,
		},
		{
			label: "Residual scans / state transitions",
			value: `${metrics[2]} / ${metrics[15]}`,
		},
	];
}
