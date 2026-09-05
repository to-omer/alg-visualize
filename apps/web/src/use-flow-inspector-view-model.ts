import { useMemo } from "react";
import {
	isArcFixingAlgorithm,
	isBinaryBlockingAlgorithm,
	isDistanceDirectedAlgorithm,
	isGoldbergRaoAlgorithm,
	isMinimumMeanCycleCancelingAlgorithm,
	isNetworkSimplexAlgorithm,
} from "./flow-algorithm-presentation";
import { projectArcFixingMetrics } from "./flow-arc-fixing-metrics";
import { projectDistanceDirectedMetrics } from "./flow-distance-directed-metrics";
import { projectEibfsMetrics } from "./flow-eibfs-metrics";
import { isEibfsAlgorithm } from "./flow-eibfs-view";
import { projectGoldbergRaoMetrics } from "./flow-goldberg-rao-metrics";
import {
	projectBoykovKolmogorovMetrics,
	projectIbfsMetrics,
} from "./flow-ibfs-metrics";
import { isIbfsAlgorithm } from "./flow-ibfs-view";
import {
	buildFlowInspectorSummaries,
	type FlowInspectorSummaries,
	parametricMetricRows,
} from "./flow-inspector-summary";
import type { FlowOverlayPresentation } from "./flow-overlay-presentation";
import { sumFlowRationals } from "./flow-parametric-view";
import type { FlowRenderPlan } from "./flow-render-plan";
import type { FlowCurrentSceneV9 } from "./flow-scene";

export type FlowInspectorViewModel = Readonly<{
	overview: Readonly<{
		capacityTotal: bigint | undefined;
		parametricCapacityTotal: string | undefined;
		activeResidualDescription: string | undefined;
		fixedEdgeDescription: string;
		labelOrderDescription: string | undefined;
		flowLodLabel: "Detail" | "Structure" | "Overview" | "—";
		predictionAssistedSummary: FlowInspectorSummaries["predictionAssistedSummary"];
		tardosSummary: FlowInspectorSummaries["tardosSummary"];
	}>;
	scenario: Readonly<{
		currentParametricMetricRows: ReturnType<typeof parametricMetricRows>;
		arcFixingMetrics: ReturnType<typeof projectArcFixingMetrics> | undefined;
		ibfsMetricRows: ReturnType<typeof projectIbfsMetrics>;
		eibfsMetricRows: ReturnType<typeof projectEibfsMetrics>;
		goldbergRaoMetricRows: ReturnType<typeof projectGoldbergRaoMetrics>;
		binaryBlockingMetricRows: ReturnType<typeof projectGoldbergRaoMetrics>;
		distanceDirectedMetricRows: ReturnType<
			typeof projectDistanceDirectedMetrics
		>;
	}>;
	algorithmDetail: Readonly<{
		fixedEdgeIds: ReadonlySet<string>;
		arcFixingMetrics: ReturnType<typeof projectArcFixingMetrics> | undefined;
		selectedMeanLabel: string | undefined;
		networkSimplexArtificialBalance:
			| Readonly<{ nodes: number; absolute: bigint }>
			| undefined;
	}>;
	continuous: Readonly<{
		electricalSummary: FlowInspectorSummaries["electricalSummary"];
		augmentingElectricalSummary: FlowInspectorSummaries["augmentingElectricalSummary"];
		interiorPointSummary: FlowInspectorSummaries["interiorPointSummary"];
		minimumRatioSummary: FlowInspectorSummaries["minimumRatioSummary"];
		randomizedAlmostLinearSummary: FlowInspectorSummaries["randomizedAlmostLinearSummary"];
		deterministicAlmostLinearSummary: FlowInspectorSummaries["deterministicAlmostLinearSummary"];
		convexCostSummary: FlowInspectorSummaries["convexCostSummary"];
		convexSimplexSummary: FlowInspectorSummaries["convexSimplexSummary"];
	}>;
}>;

function greatestCommonDivisor(left: bigint, right: bigint): bigint {
	let a = left < 0n ? -left : left;
	let b = right;
	while (b !== 0n) {
		const remainder = a % b;
		a = b;
		b = remainder;
	}
	return a === 0n ? 1n : a;
}

function selectedMinimumMeanLabel(
	scene: FlowCurrentSceneV9 | undefined,
): string | undefined {
	if (
		!isMinimumMeanCycleCancelingAlgorithm(scene?.algorithm.id) ||
		scene?.trace_event?.catalog_id !==
			"minimum-mean-cycle-canceling.select-minimum-mean-cycle" ||
		scene.trace_event.detail?.label !== "cycle-cost"
	) {
		return undefined;
	}
	const cycleLength = BigInt(
		scene.residual_arcs.filter((arc) => arc.active).length,
	);
	if (cycleLength === 0n) return undefined;
	const cycleCost = BigInt(scene.trace_event.detail.value);
	const divisor = greatestCommonDivisor(cycleCost, cycleLength);
	const numerator = cycleCost / divisor;
	const denominator = cycleLength / divisor;
	return denominator === 1n
		? numerator.toString()
		: `${numerator}/${denominator}`;
}

export function useFlowInspectorViewModel(
	scene: FlowCurrentSceneV9 | undefined,
	renderPlan: FlowRenderPlan | undefined,
	presentation: FlowOverlayPresentation | undefined,
): FlowInspectorViewModel {
	return useMemo(() => {
		const overlayViews = presentation?.renderData.overlayViews;
		const capacityTotal = scene?.graph.edges.reduce(
			(sum, edge) => sum + BigInt(edge.capacity),
			0n,
		);
		const parametricCapacityTotal =
			overlayViews?.parametric === undefined
				? undefined
				: sumFlowRationals(
						overlayViews.parametric.edge_capacities.map(
							(capacity) => capacity.capacity,
						),
					);
		const activeResidualDescription = scene?.residual_arcs
			.filter((arc) => arc.active)
			.map((arc) => `${arc.edge_id}:${arc.direction} ${arc.from}→${arc.to}`)
			.join(" · ");
		const fixedEdgeIds = new Set(
			scene?.residual_arcs
				.filter((arc) => arc.fixed)
				.map((arc) => arc.edge_id) ?? [],
		);
		const arcFixingMetrics =
			scene !== undefined && isArcFixingAlgorithm(scene.algorithm.id)
				? projectArcFixingMetrics(scene.metrics)
				: undefined;
		const summaries =
			scene === undefined || presentation === undefined
				? undefined
				: buildFlowInspectorSummaries(scene, presentation);
		const flowLodLabel =
			renderPlan?.level === "detail"
				? "Detail"
				: renderPlan?.level === "structure"
					? "Structure"
					: renderPlan?.level === "overview"
						? "Overview"
						: "—";

		return {
			overview: {
				capacityTotal,
				parametricCapacityTotal,
				activeResidualDescription,
				fixedEdgeDescription: [...fixedEdgeIds].sort().join(" · "),
				labelOrderDescription: scene?.node_trace_states
					.filter((node) => node.search_ordinal !== undefined)
					.sort(
						(left, right) =>
							(left.search_ordinal ?? 0) - (right.search_ordinal ?? 0),
					)
					.map((node) => `${node.node_id} #${node.search_ordinal}`)
					.join(" · "),
				flowLodLabel,
				predictionAssistedSummary: summaries?.predictionAssistedSummary,
				tardosSummary: summaries?.tardosSummary,
			},
			scenario: {
				currentParametricMetricRows: parametricMetricRows(scene),
				arcFixingMetrics,
				ibfsMetricRows:
					scene !== undefined && isIbfsAlgorithm(scene.algorithm.id)
						? scene.algorithm.id === "boykov-kolmogorov"
							? projectBoykovKolmogorovMetrics(scene.metrics)
							: projectIbfsMetrics(scene.metrics)
						: [],
				eibfsMetricRows:
					scene !== undefined && isEibfsAlgorithm(scene.algorithm.id)
						? projectEibfsMetrics(
								scene.metrics,
								scene.algorithm.id === "dynamic-eibfs",
							)
						: [],
				goldbergRaoMetricRows:
					scene !== undefined && isGoldbergRaoAlgorithm(scene.algorithm.id)
						? projectGoldbergRaoMetrics(scene.metrics)
						: [],
				binaryBlockingMetricRows:
					scene !== undefined && isBinaryBlockingAlgorithm(scene.algorithm.id)
						? projectGoldbergRaoMetrics(scene.metrics)
						: [],
				distanceDirectedMetricRows:
					scene !== undefined && isDistanceDirectedAlgorithm(scene.algorithm.id)
						? projectDistanceDirectedMetrics(scene.metrics)
						: [],
			},
			algorithmDetail: {
				fixedEdgeIds,
				arcFixingMetrics,
				selectedMeanLabel: selectedMinimumMeanLabel(scene),
				networkSimplexArtificialBalance:
					scene !== undefined && isNetworkSimplexAlgorithm(scene.algorithm.id)
						? scene.node_trace_states.reduce(
								(summary, state) => {
									const balance = BigInt(state.remaining_divergence ?? "0");
									if (balance !== 0n) summary.nodes += 1;
									summary.absolute += balance < 0n ? -balance : balance;
									return summary;
								},
								{ nodes: 0, absolute: 0n },
							)
						: undefined,
			},
			continuous: {
				electricalSummary: summaries?.electricalSummary,
				augmentingElectricalSummary: summaries?.augmentingElectricalSummary,
				interiorPointSummary: summaries?.interiorPointSummary,
				minimumRatioSummary: summaries?.minimumRatioSummary,
				randomizedAlmostLinearSummary: summaries?.randomizedAlmostLinearSummary,
				deterministicAlmostLinearSummary:
					summaries?.deterministicAlmostLinearSummary,
				convexCostSummary: summaries?.convexCostSummary,
				convexSimplexSummary: summaries?.convexSimplexSummary,
			},
		};
	}, [presentation, renderPlan?.level, scene]);
}
