export type FlowGoldbergRaoMetricRow = { label: string; value: string };

export function projectGoldbergRaoMetrics(
	metrics: readonly string[],
): FlowGoldbergRaoMetricRow[] {
	if (metrics.length !== 16) {
		throw new Error("Goldberg–Rao metrics require the flow-metrics/6 vector");
	}
	return [
		{
			label: "0–1 distance searches / gap phases",
			value: `${metrics[0]} / ${metrics[1]}`,
		},
		{
			label: "Residual scans / state transitions",
			value: `${metrics[2]} / ${metrics[15]}`,
		},
		{
			label: "Binary updates / augmented units",
			value: `${metrics[3]} / ${metrics[14]}`,
		},
		{
			label: "Canonical cuts / gap replacements",
			value: `${metrics[4]} / ${metrics[10]}`,
		},
		{
			label: "Blocking / delta-limited updates",
			value: `${metrics[6]} / ${metrics[12]}`,
		},
		{
			label: "Base-zero / special arc observations",
			value: `${metrics[7]} / ${metrics[8]}`,
		},
		{
			label: "Nontrivial SCCs / contracted augmentations",
			value: `${metrics[9]} / ${metrics[11]}`,
		},
		{ label: "Component lift paths", value: metrics[13] ?? "0" },
	];
}
