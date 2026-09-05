import type { FlowCurrentSceneV9 } from "./flow-scene";

const COST_SCALING_REFINE_START_IDS = new Set([
	"cost-scaling.start-refine",
	"cost-scaling-push-relabel.start-refine",
	"augment-relabel.start-refine",
	"partial-augment-relabel-mcf.start-refine",
	"price-refinement.start-refine",
	"arc-fixing.start-refine",
	"generalized-cost-scaling.start-refine",
]);

const COST_SCALING_REFINE_COMPLETE_IDS = new Set([
	"cost-scaling.complete-refine",
	"cost-scaling-push-relabel.complete-refine",
	"augment-relabel.complete-refine",
	"partial-augment-relabel-mcf.complete-refine",
	"price-refinement.complete-refine",
	"arc-fixing.complete-refine",
	"generalized-cost-scaling.complete-refine",
]);

export type FlowCostScalingRefineArc = Readonly<{
	className: "negative" | "frontier" | "certificate";
	reducedCost: bigint;
	witness: boolean;
}>;

export type FlowCostScalingRefineBoundary = Readonly<{
	kind: "start" | "complete";
	epsilon: bigint;
	arcs: ReadonlyMap<string, FlowCostScalingRefineArc>;
}>;

function residualArcKey(
	arc: FlowCurrentSceneV9["residual_arcs"][number],
): string {
	return `${arc.edge_id}:${arc.direction}`;
}

/**
 * Projects a cost-scaling ε boundary onto the exact residual graph.
 *
 * At refine start, every negative reduced-cost residual arc is about to be
 * saturated. If the set is empty, the minimum reduced-cost arc is retained as
 * the exact threshold witness. At refine completion, that minimum arc is the
 * visible witness for c̄ >= -ε. This keeps a global scalar attached to graph
 * structure without pretending that every node was locally touched.
 */
export function projectFlowCostScalingRefineBoundary(
	traceEvent: FlowCurrentSceneV9["trace_event"],
	residualArcs: FlowCurrentSceneV9["residual_arcs"],
	nodeTraceStates: ReadonlyMap<
		string,
		FlowCurrentSceneV9["node_trace_states"][number]
	>,
	nodeCount: number,
): FlowCostScalingRefineBoundary | undefined {
	if (traceEvent === undefined) return undefined;
	const kind = COST_SCALING_REFINE_START_IDS.has(traceEvent.catalog_id)
		? ("start" as const)
		: COST_SCALING_REFINE_COMPLETE_IDS.has(traceEvent.catalog_id)
			? ("complete" as const)
			: undefined;
	if (kind === undefined) return undefined;
	if (traceEvent.detail?.label !== "epsilon") {
		throw new Error(
			"Cost-scaling refine boundary is missing its exact epsilon",
		);
	}
	const epsilon = BigInt(traceEvent.detail.value);
	if (epsilon <= 0n || nodeCount <= 0) {
		throw new Error(
			"Cost-scaling refine boundary has an invalid epsilon domain",
		);
	}

	const multiplier = BigInt(nodeCount + 1);
	const candidates = residualArcs
		.filter((arc) => BigInt(arc.capacity) > 0n && arc.fixed !== true)
		.map((arc) => {
			const from = nodeTraceStates.get(arc.from)?.label;
			const to = nodeTraceStates.get(arc.to)?.label;
			if (from === undefined || to === undefined) {
				throw new Error("Cost-scaling refine boundary is missing a node price");
			}
			return {
				key: residualArcKey(arc),
				reducedCost: BigInt(arc.cost) * multiplier + BigInt(from) - BigInt(to),
			};
		})
		.sort((left, right) => left.key.localeCompare(right.key));
	if (candidates.length === 0) {
		throw new Error("Cost-scaling refine boundary has no residual capacity");
	}

	const witness = candidates.reduce((minimum, candidate) =>
		candidate.reducedCost < minimum.reducedCost ? candidate : minimum,
	);
	const negative = candidates.filter((candidate) => candidate.reducedCost < 0n);
	const rendered =
		kind === "start" && negative.length > 0 ? negative : [witness];
	return {
		kind,
		epsilon,
		arcs: new Map(
			rendered.map(
				(candidate) =>
					[
						candidate.key,
						{
							className:
								kind === "complete"
									? "certificate"
									: candidate.reducedCost < 0n
										? "negative"
										: "frontier",
							reducedCost: candidate.reducedCost,
							witness: candidate.key === witness.key,
						},
					] as const,
			),
		),
	};
}
