export type FlowEibfsMetricRow = { label: string; value: string };

export function projectEibfsMetrics(
	metrics: readonly string[],
	dynamic = false,
): FlowEibfsMetricRow[] {
	if (metrics.length !== 16) {
		throw new Error("EIBFS metrics require the flow-metrics/6 vector");
	}
	if (dynamic) {
		return [
			{
				label: "Updates · increases / decreases",
				value: `${metrics[0]} · ${metrics[1]} / ${metrics[5]}`,
			},
			{
				label: "Repair scans / iterations",
				value: `${metrics[2]} / ${metrics[13]}`,
			},
			{
				label: "Bridge / label violations",
				value: `${metrics[3]} / ${metrics[4]}`,
			},
			{
				label: "Current-arc / boundary violations",
				value: `${metrics[7]} / ${metrics[9]}`,
			},
			{
				label: "Reused nodes / invalidated parents",
				value: `${metrics[6]} / ${metrics[8]}`,
			},
			{
				label: "Over-capacity repairs / promoted roots",
				value: `${metrics[10]} / ${metrics[11]}`,
			},
			{
				label: "No-ops / certification recoveries",
				value: `${metrics[12]} / ${metrics[14]}`,
			},
			{ label: "Reusable state transitions", value: metrics[15] ?? "0" },
		];
	}
	return [
		{
			label: "Phases · forward / reverse",
			value: `${metrics[0]} · ${metrics[1]} / ${metrics[5]}`,
		},
		{
			label: "Residual / adoption scans",
			value: `${metrics[2]} / ${metrics[9]}`,
		},
		{
			label: "Bridge / tree-path pushes",
			value: `${metrics[3]} / ${metrics[4]}`,
		},
		{
			label: "Tree attachments / removals",
			value: `${metrics[6]} / ${metrics[8]}`,
		},
		{
			label: "Orphans created / visited",
			value: `${metrics[10]} / ${metrics[11]}`,
		},
		{
			label: "Relabels / side migrations",
			value: `${metrics[7]} / ${metrics[13]}`,
		},
		{
			label: "Saturated tree arcs / recovery paths",
			value: `${metrics[12]} / ${metrics[14]}`,
		},
		{ label: "State transitions", value: metrics[15] ?? "0" },
	];
}
