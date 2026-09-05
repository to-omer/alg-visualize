import type { FlowOverlayPresentation } from "./flow-overlay-presentation";
import type { FlowCurrentSceneV9 } from "./flow-scene";

export type FlowInspectorMetricRow = Readonly<{
	label: string;
	value: string;
}>;

/** Keeps Fast-only feasibility work visible without mixing it into primary metrics. */
export function feasibilityWorkRows(
	scene: FlowCurrentSceneV9 | undefined,
): readonly FlowInspectorMetricRow[] {
	const work = scene?.feasibility_work;
	if (work === undefined) return [];
	return [
		{
			label: "Feasibility prepass",
			value: `${work.invocations} run${work.invocations === "1" ? "" : "s"} · ${work.metrics.original_edge_inspections} input-edge scans · ${work.metrics.original_node_inspections} input-node scans`,
		},
		{
			label: "Auxiliary routing work",
			value: `${work.metrics.auxiliary_adjacency_inspections} adjacency scans · ${work.metrics.pushes} pushes · ${work.metrics.relabels} relabels · ${work.metrics.active_node_selections} active selections · ${work.metrics.discharges} discharges`,
		},
		{
			label: "Feasibility certificate work",
			value: `${work.metrics.cut_adjacency_inspections} cut scans · ${work.metrics.extracted_original_edges} extracted-edge checks`,
		},
	];
}

/** Projects algorithm-specific wire metrics into stable inspector rows. */
export function parametricMetricRows(
	scene: FlowCurrentSceneV9 | undefined,
): readonly FlowInspectorMetricRow[] {
	if (scene?.outcome?.kind !== "parametric-max-flow") return [];
	const { metrics } = scene.outcome;
	return metrics.implementation === "parametric-pseudoflow"
		? [
				{
					label: "Forest initialization / reuse",
					value: `${metrics.forest_initializations} / ${metrics.forest_reuses}`,
				},
				{
					label: "Parameter advances",
					value: metrics.parameter_advances,
				},
				{
					label: "Renormalization push / split",
					value: `${metrics.renormalization_pushes} / ${metrics.renormalization_splits}`,
				},
				{
					label: "Merge / relabel",
					value: `${metrics.mergers} / ${metrics.relabels}`,
				},
				{
					label: "Free-run races · forward / reverse",
					value: `${metrics.free_run_races} · ${metrics.forward_race_wins} / ${metrics.reverse_race_wins}`,
				},
				{
					label: "Contractions · restart / continue",
					value: `${metrics.contraction_views} · ${metrics.smaller_child_restarts} / ${metrics.larger_child_continuations}`,
				},
				{
					label: "Normalized-forest residual-arc scans",
					value: metrics.residual_arc_scans,
				},
				{ label: "Maximum recursion depth", value: metrics.maximum_depth },
			]
		: [
				{
					label: "Cold pseudoflow / cut-oracle runs",
					value: `${metrics.pseudoflow_runs} / ${metrics.oracle_runs}`,
				},
				{
					label: "Cold-solver residual-arc scans",
					value: metrics.static_residual_arc_scans,
				},
				{
					label: "Intersections / subproblems",
					value: `${metrics.intersections} / ${metrics.subproblems}`,
				},
				{
					label: "Segments / breakpoints",
					value: `${metrics.segments} / ${metrics.breakpoints}`,
				},
				{
					label: "Simultaneous breakpoints",
					value: metrics.simultaneous_breakpoints,
				},
				{ label: "Maximum recursion depth", value: metrics.maximum_depth },
			];
}

export function buildFlowInspectorSummaries(
	scene: FlowCurrentSceneV9,
	presentation: FlowOverlayPresentation,
) {
	const overlayViews = presentation.renderData.overlayViews;
	const predictionAssistedSummary =
		overlayViews.predictionAssistedEpsilon === undefined
			? undefined
			: {
					overlay: overlayViews.predictionAssistedEpsilon,
					clipped: overlayViews.predictionAssistedEpsilon.nodes.filter(
						(node) => node.prediction_clipped,
					).length,
					metrics: scene.metrics,
				};
	const tardosSummary =
		overlayViews.tardosFramework === undefined
			? undefined
			: {
					overlay: overlayViews.tardosFramework,
					positive: overlayViews.tardosFramework.residual_arcs.filter(
						(arc) => BigInt(arc.reduced_cost) > 0n,
					).length,
					negative: overlayViews.tardosFramework.residual_arcs.filter(
						(arc) => BigInt(arc.reduced_cost) < 0n,
					).length,
					metrics: scene.metrics,
				};
	const electricalSummary =
		overlayViews.electricalFlow === undefined
			? undefined
			: {
					overlay: overlayViews.electricalFlow,
					maximumCongestion: overlayViews.electricalFlow.edges.reduce(
						(maximum, edge) => Math.max(maximum, Number(edge.congestion)),
						0,
					),
					metrics: scene.metrics,
				};
	const augmentingElectricalSummary =
		overlayViews.augmentingElectrical === undefined
			? undefined
			: {
					overlay: overlayViews.augmentingElectrical,
					maximumCongestion: overlayViews.augmentingElectrical.edges.reduce(
						(maximum, edge) => Math.max(maximum, Number(edge.congestion)),
						0,
					),
					boostedRoots: overlayViews.augmentingElectrical.edges.filter(
						(edge) => BigInt(edge.boost_segments) > 1n,
					).length,
					metrics: scene.metrics,
				};
	const interiorPointSummary =
		overlayViews.interiorPointMaxFlow === undefined
			? undefined
			: {
					overlay: overlayViews.interiorPointMaxFlow,
					maximumCongestion: overlayViews.interiorPointMaxFlow.edges.reduce(
						(maximum, edge) => Math.max(maximum, Number(edge.congestion)),
						0,
					),
					normalizedEdges: overlayViews.interiorPointMaxFlow.edges.filter(
						(edge) => edge.normalized_away,
					).length,
					metrics: scene.metrics,
				};
	const minimumRatioSummary =
		overlayViews.minimumRatioCycle === undefined
			? undefined
			: {
					overlay: overlayViews.minimumRatioCycle,
					treeEdges: overlayViews.minimumRatioCycle.edges.filter(
						(edge) => edge.tree_edge,
					).length,
					candidateEdges: overlayViews.minimumRatioCycle.edges.filter(
						(edge) => edge.candidate_sign !== "0",
					).length,
					selectedEdges: overlayViews.minimumRatioCycle.edges.filter(
						(edge) => edge.selected_sign !== "0",
					).length,
					metrics: scene.metrics,
				};
	const randomizedAlmostLinearSummary =
		overlayViews.randomizedAlmostLinear === undefined
			? undefined
			: {
					overlay: overlayViews.randomizedAlmostLinear,
					treeEdges: overlayViews.randomizedAlmostLinear.edges.filter(
						(edge) => edge.active_tree_edge,
					).length,
					cycleEdges: overlayViews.randomizedAlmostLinear.edges.filter(
						(edge) => edge.active_cycle_sign !== "0",
					).length,
					changedCoordinates: overlayViews.randomizedAlmostLinear.edges.filter(
						(edge) => edge.changed_coordinate,
					).length,
					metrics: scene.metrics,
				};
	const deterministicAlmostLinearSummary =
		overlayViews.deterministicAlmostLinear === undefined
			? undefined
			: {
					overlay: overlayViews.deterministicAlmostLinear,
					treeEdges: overlayViews.deterministicAlmostLinear.edges.filter(
						(edge) => edge.tree_level_mask !== "0",
					).length,
					forestEdges: overlayViews.deterministicAlmostLinear.edges.filter(
						(edge) => edge.forest_level_mask !== "0",
					).length,
					coreEdges: overlayViews.deterministicAlmostLinear.edges.filter(
						(edge) => edge.active_core_edge,
					).length,
					spannerEdges: overlayViews.deterministicAlmostLinear.edges.filter(
						(edge) => edge.active_spanner_edge,
					).length,
					embeddedEdges: overlayViews.deterministicAlmostLinear.edges.filter(
						(edge) => BigInt(edge.embedding_hops) > 0n,
					).length,
					cycleEdges: overlayViews.deterministicAlmostLinear.edges.filter(
						(edge) => edge.active_cycle_sign !== "0",
					).length,
					changedCoordinates:
						overlayViews.deterministicAlmostLinear.edges.filter(
							(edge) => edge.changed_coordinate,
						).length,
					roundingForestEdges:
						overlayViews.deterministicAlmostLinear.edges.filter(
							(edge) => edge.rounding_forest_edge,
						).length,
					roundingCycleEdges:
						overlayViews.deterministicAlmostLinear.edges.filter(
							(edge) => edge.rounding_cycle_sign !== "0",
						).length,
					metrics: scene.metrics,
				};
	const convexCostSummary =
		overlayViews.convexCost === undefined
			? undefined
			: {
					stage: overlayViews.convexCost.stage,
					scale: overlayViews.convexCost.scale,
					edgeCount: overlayViews.convexCost.edges.length,
					segmentCount: overlayViews.convexCost.edges.reduce(
						(total, edge) => total + edge.segments.length,
						0,
					),
					activeCycleLength: overlayViews.convexCost.active_cycle.length,
					eligibleArcCount: (overlayViews.convexCost.eligible_arcs ?? [])
						.length,
					totalCost: overlayViews.convexCost.edges
						.reduce((total, edge) => total + BigInt(edge.total_cost), 0n)
						.toString(),
				};
	const convexSimplexSummary =
		overlayViews.convexNetworkSimplex === undefined
			? undefined
			: {
					stage: overlayViews.convexNetworkSimplex.stage,
					originalTreeEdges: overlayViews.convexNetworkSimplex.edges.filter(
						(edge) => edge.basis === "tree",
					).length,
					artificialTreeEdges:
						overlayViews.convexNetworkSimplex.artificial_edges.filter(
							(edge) => edge.basis === "tree",
						).length,
					cycle: overlayViews.convexNetworkSimplex.cycle,
					entering: overlayViews.convexNetworkSimplex.entering,
					leaving: overlayViews.convexNetworkSimplex.leaving,
					metrics: scene.metrics,
				};

	return {
		predictionAssistedSummary,
		tardosSummary,
		electricalSummary,
		augmentingElectricalSummary,
		interiorPointSummary,
		minimumRatioSummary,
		randomizedAlmostLinearSummary,
		deterministicAlmostLinearSummary,
		convexCostSummary,
		convexSimplexSummary,
	};
}

export type FlowInspectorSummaries = ReturnType<
	typeof buildFlowInspectorSummaries
>;
