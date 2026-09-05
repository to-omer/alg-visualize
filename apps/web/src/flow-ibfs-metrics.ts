export type FlowIbfsMetricRow = {
	label: string;
	value: string;
};

export function projectIbfsMetrics(
	metrics: readonly string[],
): FlowIbfsMetricRow[] {
	if (metrics.length !== 16) {
		throw new Error("IBFS metrics require the flow-metrics/6 vector");
	}
	return [
		{
			label: "Passes · forward / reverse",
			value: `${metrics[0]} · ${metrics[1]} / ${metrics[5]}`,
		},
		{
			label: "Residual / adoption scans",
			value: `${metrics[2]} / ${metrics[9]}`,
		},
		{
			label: "Augmentations / path arcs",
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
			label: "Same-level / relabeled",
			value: `${metrics[13]} / ${metrics[7]}`,
		},
		{ label: "Saturated tree arcs", value: metrics[12] ?? "0" },
		{
			label: "Active scans / transitions",
			value: `${metrics[14]} / ${metrics[15]}`,
		},
	];
}

export function projectBoykovKolmogorovMetrics(
	metrics: readonly string[],
): FlowIbfsMetricRow[] {
	if (metrics.length !== 16) {
		throw new Error("Boykov–Kolmogorov metrics require flow-metrics/6");
	}
	const totalScans = BigInt(metrics[2] ?? "0");
	const adoptionScans = BigInt(metrics[9] ?? "0");
	const growthScans = totalScans - adoptionScans;
	return [
		{
			label: "Active visits / passive vertices",
			value: `${metrics[0]} / ${metrics[1]}`,
		},
		{
			label: "Growth / adoption scans",
			value: `${growthScans} / ${adoptionScans}`,
		},
		{
			label: "Augmentations / path arcs",
			value: `${metrics[3]} / ${metrics[4]}`,
		},
		{
			label: "Tree attachments / reactivations",
			value: `${metrics[5]} / ${metrics[14]}`,
		},
		{
			label: "Orphans created / visited",
			value: `${metrics[10]} / ${metrics[8]}`,
		},
		{
			label: "Adopted / made free",
			value: `${metrics[11]} / ${metrics[7]}`,
		},
		{ label: "State transitions", value: metrics[15] ?? "0" },
	];
}
