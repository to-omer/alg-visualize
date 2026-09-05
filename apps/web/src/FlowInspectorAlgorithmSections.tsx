import {
	CAPACITY_SCALING_MCF_ALGORITHM,
	CONVEX_COST_SCALING_ALGORITHM,
	isArcFixingAlgorithm,
	isAuctionAlgorithm,
	isAugmentRelabelMcfAlgorithm,
	isBinaryBlockingAlgorithm,
	isBlockingPreflowAlgorithm,
	isBlockingPrimalDualAlgorithm,
	isBorradaileKleinPlanarAlgorithm,
	isCancelAndTightenAlgorithm,
	isCapacityScalingMcfAlgorithm,
	isConvexCostAlgorithm,
	isCostScalingAlgorithm,
	isCycleCancelingAlgorithm,
	isDistanceDirectedAlgorithm,
	isDoubleScalingAlgorithm,
	isDualNetworkSimplexAlgorithm,
	isDynamicTreeBlockingAlgorithm,
	isDynamicTreeNetworkSimplexAlgorithm,
	isDynamicTreePushRelabelAlgorithm,
	isEnhancedCapacityScalingAlgorithm,
	isEpsilonRelaxationAlgorithm,
	isExcessScalingMcfAlgorithm,
	isExcessScalingPushRelabelAlgorithm,
	isGoldbergRaoAlgorithm,
	isHassinStPlanarAlgorithm,
	isHopcroftKarpAlgorithm,
	isHungarianAlgorithm,
	isMinimumMeanCycleCancelingAlgorithm,
	isNetworkSimplexAlgorithm,
	isOrlinMaxFlowAlgorithm,
	isOrlinMcfAlgorithm,
	isOutOfKilterAlgorithm,
	isPartialAugmentRelabelAlgorithm,
	isPolynomialDualSimplexAlgorithm,
	isPolynomialPrimalSimplexAlgorithm,
	isPotentialDijkstraSspAlgorithm,
	isPriceRefinementAlgorithm,
	isPseudoflowAlgorithm,
	isPushRelabelAlgorithm,
	isRelabelHeuristicAlgorithm,
	isRelaxationAlgorithm,
	isRelaxedMndcAlgorithm,
	isSimpleCycleCancelingAlgorithm,
	isSynchronousPushRelabelAlgorithm,
	isTransportationAlgorithm,
	isWarmStartPushRelabelAlgorithm,
	PSEUDOFLOW_SIMPLEX_ALGORITHM,
} from "./flow-algorithm-presentation";
import { isEibfsAlgorithm } from "./flow-eibfs-view";
import { isIbfsAlgorithm } from "./flow-ibfs-view";
import type { FlowOverlayPresentation } from "./flow-overlay-presentation";
import { projectFlowParametricCut } from "./flow-parametric-view";
import type { FlowCurrentSceneV9 } from "./flow-scene";
import type { FlowInspectorViewModel } from "./use-flow-inspector-view-model";

type FlowInspectorSectionContext = Readonly<{
	scene: FlowCurrentSceneV9 | undefined;
	presentation: FlowOverlayPresentation | undefined;
}>;

type FlowInspectorScenarioProps = FlowInspectorSectionContext &
	FlowInspectorViewModel["scenario"];

type FlowInspectorAlgorithmDetailProps = FlowInspectorSectionContext &
	FlowInspectorViewModel["algorithmDetail"];

export function FlowInspectorScenarioSection(
	props: FlowInspectorScenarioProps,
) {
	const scene = props.scene;
	const currentParametricMetricRows = props.currentParametricMetricRows;
	const arcFixingMetrics = props.arcFixingMetrics;
	const ibfsMetricRows = props.ibfsMetricRows;
	const eibfsMetricRows = props.eibfsMetricRows;
	const goldbergRaoMetricRows = props.goldbergRaoMetricRows;
	const binaryBlockingMetricRows = props.binaryBlockingMetricRows;
	const distanceDirectedMetricRows = props.distanceDirectedMetricRows;
	const currentOverlayViews = props.presentation?.renderData.overlayViews;
	return (
		<dl className="property-list">
			{currentParametricMetricRows.map((row) => (
				<div key={`parametric-metric:${row.label}`}>
					<dt>{row.label}</dt>
					<dd>{row.value}</dd>
				</div>
			))}
			<div>
				<dt>
					{scene?.model.kind === "parametric-max-flow"
						? "Published intervals"
						: isHassinStPlanarAlgorithm(scene?.algorithm.id)
							? "Positive-flow edges"
							: isBorradaileKleinPlanarAlgorithm(scene?.algorithm.id)
								? "Leftmost augmentations"
								: isHopcroftKarpAlgorithm(scene?.algorithm.id)
									? "Disjoint augmentations"
									: isHungarianAlgorithm(scene?.algorithm.id)
										? "Alternating augmentations"
										: isAuctionAlgorithm(scene?.algorithm.id)
											? "Awards"
											: isPartialAugmentRelabelAlgorithm(scene?.algorithm.id)
												? "Path augmentations"
												: isAugmentRelabelMcfAlgorithm(scene?.algorithm.id)
													? "Path augmentations"
													: isWarmStartPushRelabelAlgorithm(scene?.algorithm.id)
														? "Recovery paths"
														: isPseudoflowAlgorithm(scene?.algorithm.id)
															? "Recovery paths"
															: isPushRelabelAlgorithm(scene?.algorithm.id) ||
																	isBlockingPreflowAlgorithm(
																		scene?.algorithm.id,
																	)
																? "Pushes"
																: isCycleCancelingAlgorithm(scene?.algorithm.id)
																	? "Canceled cycles"
																	: isOutOfKilterAlgorithm(scene?.algorithm.id)
																		? "Breakthroughs"
																		: isRelaxationAlgorithm(scene?.algorithm.id)
																			? "Balanced augmentations"
																			: isEpsilonRelaxationAlgorithm(
																						scene?.algorithm.id,
																					)
																				? "Admissible pushes"
																				: isArcFixingAlgorithm(
																							scene?.algorithm.id,
																						)
																					? "Arcs fixed"
																					: isPriceRefinementAlgorithm(
																								scene?.algorithm.id,
																							)
																						? "Refines skipped"
																						: isCostScalingAlgorithm(
																									scene?.algorithm.id,
																								)
																							? "Initial saturations"
																							: isNetworkSimplexAlgorithm(
																										scene?.algorithm.id,
																									)
																								? "Pivots"
																								: isGoldbergRaoAlgorithm(
																											scene?.algorithm.id,
																										)
																									? "Binary updates"
																									: isBinaryBlockingAlgorithm(
																												scene?.algorithm.id,
																											)
																										? "Primitive updates"
																										: isDistanceDirectedAlgorithm(
																													scene?.algorithm.id,
																												)
																											? "Tree augmentations"
																											: "Augmentations"}
				</dt>
				<dd>
					{scene?.outcome?.kind === "parametric-max-flow"
						? `${scene.outcome.segments.length} / ${scene.outcome.breakpoints.length} breakpoints`
						: isHassinStPlanarAlgorithm(scene?.algorithm.id)
							? (scene?.metrics[11] ?? "—")
							: isHopcroftKarpAlgorithm(scene?.algorithm.id) ||
									isHungarianAlgorithm(scene?.algorithm.id)
								? (scene?.metrics[3] ?? "—")
								: isPartialAugmentRelabelAlgorithm(scene?.algorithm.id)
									? (scene?.metrics[3] ?? "—")
									: isWarmStartPushRelabelAlgorithm(scene?.algorithm.id)
										? (scene?.metrics[4] ?? "—")
										: isPseudoflowAlgorithm(scene?.algorithm.id)
											? (scene?.metrics[3] ?? "—")
											: isPushRelabelAlgorithm(scene?.algorithm.id) ||
													isBlockingPreflowAlgorithm(scene?.algorithm.id)
												? (scene?.metrics[11] ?? "—")
												: isArcFixingAlgorithm(scene?.algorithm.id)
													? (arcFixingMetrics?.arcsFixed ?? "—")
													: (scene?.metrics[3] ?? "—")}
				</dd>
			</div>
			<div>
				<dt>
					{scene?.model.kind === "parametric-max-flow"
						? "Current traversal / retained state"
						: isHassinStPlanarAlgorithm(scene?.algorithm.id)
							? "Dual arc scans / Dijkstra"
							: isBorradaileKleinPlanarAlgorithm(scene?.algorithm.id)
								? "Dual + rotation scans / tree searches"
								: isCostScalingAlgorithm(scene?.algorithm.id)
									? isArcFixingAlgorithm(scene?.algorithm.id)
										? "Residual scans / fixed skips"
										: isPriceRefinementAlgorithm(scene?.algorithm.id)
											? "Residual scans / price scans"
											: isAugmentRelabelMcfAlgorithm(scene?.algorithm.id)
												? "Residual scans / retreats"
												: "Residual scans / advances"
									: isHopcroftKarpAlgorithm(scene?.algorithm.id)
										? "Edge scans / BFS"
										: isHungarianAlgorithm(scene?.algorithm.id)
											? "Cell scans / dual updates"
											: isAuctionAlgorithm(scene?.algorithm.id)
												? "Edge scans / price raises"
												: isNetworkSimplexAlgorithm(scene?.algorithm.id)
													? "Pricing scans / searches"
													: isBlockingPrimalDualAlgorithm(scene?.algorithm.id)
														? "Slack searches / equality BFS"
														: isPotentialDijkstraSspAlgorithm(
																	scene?.algorithm.id,
																)
															? "Dijkstra runs"
															: isSimpleCycleCancelingAlgorithm(
																		scene?.algorithm.id,
																	)
																? "BF relaxations"
																: isMinimumMeanCycleCancelingAlgorithm(
																			scene?.algorithm.id,
																		)
																	? "Karp DP rounds"
																	: isOutOfKilterAlgorithm(scene?.algorithm.id)
																		? "Residual scans / label searches"
																		: isRelaxationAlgorithm(scene?.algorithm.id)
																			? "Arc scans / label scans"
																			: isEpsilonRelaxationAlgorithm(
																						scene?.algorithm.id,
																					)
																				? "Arc scans / price rises"
																				: isIbfsAlgorithm(scene?.algorithm.id)
																					? "Residual / adoption scans"
																					: isEibfsAlgorithm(
																								scene?.algorithm.id,
																							)
																						? "Residual / adoption scans"
																						: "BFS / relax"}
				</dt>
				<dd>
					{scene?.model.kind === "parametric-max-flow"
						? `${currentOverlayViews?.parametric?.traversal?.kind ?? "initial boundary"} / ${currentOverlayViews?.parametric?.traversal?.normalized_tree_reused ? "tree" : "restart"}`
						: scene === undefined
							? "—"
							: isHassinStPlanarAlgorithm(scene.algorithm.id)
								? `${scene.metrics[2]} / ${scene.metrics[4]}`
								: isBorradaileKleinPlanarAlgorithm(scene.algorithm.id)
									? `${scene.metrics[2]} / ${scene.metrics[4]}`
									: isHopcroftKarpAlgorithm(scene.algorithm.id)
										? `${scene.metrics[2]} / ${scene.metrics[0]}`
										: isHungarianAlgorithm(scene.algorithm.id)
											? `${scene.metrics[2]} / ${scene.metrics[1]}`
											: isAuctionAlgorithm(scene.algorithm.id)
												? `${scene.metrics[2]} / ${scene.metrics[1]}`
												: isCostScalingAlgorithm(scene.algorithm.id)
													? isArcFixingAlgorithm(scene.algorithm.id)
														? `${arcFixingMetrics?.residualArcScans} / ${arcFixingMetrics?.fixedArcSkips}`
														: `${scene.metrics[2]} / ${scene.metrics[8]}`
													: isNetworkSimplexAlgorithm(scene.algorithm.id)
														? `${scene.metrics[2]} / ${scene.metrics[4]}`
														: isBlockingPrimalDualAlgorithm(scene.algorithm.id)
															? `${scene.metrics[4]} / ${scene.metrics[0]}`
															: isOutOfKilterAlgorithm(scene.algorithm.id)
																? `${scene.metrics[2]} / ${scene.metrics[0]}`
																: isRelaxationAlgorithm(scene.algorithm.id)
																	? `${scene.metrics[2]} / ${scene.metrics[0]}`
																	: isEpsilonRelaxationAlgorithm(
																				scene.algorithm.id,
																			)
																		? `${scene.metrics[2]} / ${scene.metrics[7]}`
																		: isPotentialDijkstraSspAlgorithm(
																					scene.algorithm.id,
																				)
																			? scene.metrics[4]
																			: isCycleCancelingAlgorithm(
																						scene.algorithm.id,
																					)
																				? scene.metrics[1]
																				: isIbfsAlgorithm(scene.algorithm.id)
																					? `${scene.metrics[2]} / ${scene.metrics[9]}`
																					: isEibfsAlgorithm(scene.algorithm.id)
																						? `${scene.metrics[2]} / ${scene.metrics[9]}`
																						: `${scene.metrics[0]} / ${scene.metrics[1]}`}
				</dd>
			</div>
			{isHassinStPlanarAlgorithm(scene?.algorithm.id) && (
				<div>
					<dt>Dual faces / settled faces</dt>
					<dd>{`${scene?.metrics[5]} / ${scene?.metrics[15]}`}</dd>
				</div>
			)}
			{isBorradaileKleinPlanarAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Dual faces / preprocessing</dt>
						<dd>{`${scene?.metrics[5]} / ${scene?.metrics[6]}`}</dd>
					</div>
					<div>
						<dt>Saturated path darts / discovered vertices</dt>
						<dd>{`${scene?.metrics[12]} / ${scene?.metrics[15]}`}</dd>
					</div>
				</>
			)}
			{ibfsMetricRows.map((row) => (
				<div key={`ibfs-metric:${row.label}`}>
					<dt>{row.label}</dt>
					<dd>{row.value}</dd>
				</div>
			))}
			{eibfsMetricRows.map((row) => (
				<div key={`eibfs-metric:${row.label}`}>
					<dt>{row.label}</dt>
					<dd>{row.value}</dd>
				</div>
			))}
			{goldbergRaoMetricRows.map((row) => (
				<div key={`goldberg-rao-metric:${row.label}`}>
					<dt>{row.label}</dt>
					<dd>{row.value}</dd>
				</div>
			))}
			{binaryBlockingMetricRows.map((row) => (
				<div key={`binary-blocking-metric:${row.label}`}>
					<dt>{row.label}</dt>
					<dd>{row.value}</dd>
				</div>
			))}
			{distanceDirectedMetricRows.map((row) => (
				<div key={`distance-directed-metric:${row.label}`}>
					<dt>{row.label}</dt>
					<dd>{row.value}</dd>
				</div>
			))}
			{isHopcroftKarpAlgorithm(scene?.algorithm.id) && (
				<div>
					<dt>Phases / DFS roots</dt>
					<dd>{`${scene?.metrics[6]} / ${scene?.metrics[4]}`}</dd>
				</div>
			)}
			{isHungarianAlgorithm(scene?.algorithm.id) && (
				<div>
					<dt>Agent searches / predecessor updates</dt>
					<dd>{`${scene?.metrics[0]} / ${scene?.metrics[4]}`}</dd>
				</div>
			)}
			{isAuctionAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Bids / scaling phases</dt>
						<dd>{`${scene?.metrics[15]} / ${scene?.metrics[5]}`}</dd>
					</div>
					<div>
						<dt>Feasibility searches</dt>
						<dd>{scene?.metrics[0]}</dd>
					</div>
					<div>
						<dt>Feasibility augmentations / evictions</dt>
						<dd>{`${scene?.metrics[4]} / ${scene?.metrics[6]}`}</dd>
					</div>
				</>
			)}
		</dl>
	);
}

export function FlowInspectorAlgorithmDetailSection(
	props: FlowInspectorAlgorithmDetailProps,
) {
	const scene = props.scene;
	const fixedEdgeIds = props.fixedEdgeIds;
	const arcFixingMetrics = props.arcFixingMetrics;
	const selectedMeanLabel = props.selectedMeanLabel;
	const networkSimplexArtificialBalance = props.networkSimplexArtificialBalance;
	const currentOverlayViews = props.presentation?.renderData.overlayViews;
	return (
		<dl className="property-list">
			{isCycleCancelingAlgorithm(scene?.algorithm.id) && (
				<div>
					<dt>
						{isMinimumMeanCycleCancelingAlgorithm(scene?.algorithm.id)
							? "Mean-cycle searches"
							: "Cycle searches"}
					</dt>
					<dd>{scene?.metrics[4]}</dd>
				</div>
			)}
			{selectedMeanLabel !== undefined && (
				<div>
					<dt>Selected mean</dt>
					<dd>{selectedMeanLabel}</dd>
				</div>
			)}
			{isPotentialDijkstraSspAlgorithm(scene?.algorithm.id) && (
				<div>
					<dt>Settled nodes / price updates</dt>
					<dd>{`${scene?.metrics[15]} / ${scene?.metrics[7]}`}</dd>
				</div>
			)}
			{scene !== undefined &&
				[
					"ford-fulkerson",
					"dfs-ford-fulkerson",
					"widest-augmenting-path",
					"capacity-scaling-augmenting-path",
				].includes(scene.algorithm.id) && (
					<div>
						<dt>Path searches</dt>
						<dd>{scene.metrics[4]}</dd>
					</div>
				)}
			{(scene?.algorithm.id === "capacity-scaling-augmenting-path" ||
				scene?.algorithm.id === "distance-directed-scaling-augmenting-path" ||
				isExcessScalingPushRelabelAlgorithm(scene?.algorithm.id) ||
				isCapacityScalingMcfAlgorithm(scene?.algorithm.id) ||
				isCostScalingAlgorithm(scene?.algorithm.id)) && (
				<div>
					<dt>Scaling phases</dt>
					<dd>{scene?.metrics[5]}</dd>
				</div>
			)}
			{scene?.algorithm.id === CAPACITY_SCALING_MCF_ALGORITHM && (
				<div>
					<dt>Phase-boundary saturations</dt>
					<dd>{scene?.metrics[12]}</dd>
				</div>
			)}
			{isExcessScalingMcfAlgorithm(scene?.algorithm.id) && (
				<div>
					<dt>Exact Δ augmentations</dt>
					<dd>{scene?.metrics[3]}</dd>
				</div>
			)}
			{isCancelAndTightenAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Phases / tightenings</dt>
						<dd>{`${scene?.metrics[5]} / ${scene?.metrics[7]}`}</dd>
					</div>
					<div>
						<dt>Cycle searches / cancellations</dt>
						<dd>{`${scene?.metrics[4]} / ${scene?.metrics[3]}`}</dd>
					</div>
				</>
			)}
			{isRelaxedMndcAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>ε phases / assignment solves</dt>
						<dd>{`${scene?.metrics[5]} / ${scene?.metrics[0]}`}</dd>
					</div>
					<div>
						<dt>Hungarian augmentations / cell scans</dt>
						<dd>{`${scene?.metrics[1]} / ${scene?.metrics[2]}`}</dd>
					</div>
					<div>
						<dt>Families / canceled cycles / arcs</dt>
						<dd>{`${scene?.metrics[4]} / ${scene?.metrics[3]} / ${scene?.metrics[6]}`}</dd>
					</div>
					<div>
						<dt>Residual scans / dropped zero cycles</dt>
						<dd>{`${scene?.metrics[7]} / ${scene?.metrics[8]}`}</dd>
					</div>
				</>
			)}
			{isEnhancedCapacityScalingAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Phases / complete regenerations</dt>
						<dd>{`${scene?.metrics[0]} / ${scene?.metrics[1]}`}</dd>
					</div>
					<div>
						<dt>Shortest paths / exact-Δ augmentations</dt>
						<dd>{`${scene?.metrics[4]} / ${scene?.metrics[3]}`}</dd>
					</div>
					<div>
						<dt>Contractions / augmented arcs</dt>
						<dd>{`${scene?.metrics[5]} / ${scene?.metrics[6]}`}</dd>
					</div>
					<div>
						<dt>Arc scans / price updates / recovery paths</dt>
						<dd>{`${scene?.metrics[2]} / ${scene?.metrics[7]} / ${scene?.metrics[8]}`}</dd>
					</div>
				</>
			)}
			{isOrlinMcfAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Capacity nodes / phases / regenerations</dt>
						<dd>{`${scene?.metrics[0]} / ${scene?.metrics[1]} / ${scene?.metrics[2]}`}</dd>
					</div>
					<div>
						<dt>Contractions / shortest paths / augmentations</dt>
						<dd>{`${scene?.metrics[3]} / ${scene?.metrics[4]} / ${scene?.metrics[7]}`}</dd>
					</div>
					<div>
						<dt>Eliminated capacity nodes / shortcut arcs</dt>
						<dd>{`${scene?.metrics[5]} / ${scene?.metrics[6]}`}</dd>
					</div>
					<div>
						<dt>Augmented arcs / price updates / scans / recovery</dt>
						<dd>{`${scene?.metrics[8]} / ${scene?.metrics[9]} / ${scene?.metrics[10]} / ${scene?.metrics[11]}`}</dd>
					</div>
				</>
			)}
			{isOrlinMaxFlowAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Improvement phases / contractions / cut updates</dt>
						<dd>{`${scene?.metrics[0]} / ${scene?.metrics[2]} / ${scene?.metrics[13]}`}</dd>
					</div>
					<div>
						<dt>Critical observations / compact networks</dt>
						<dd>{`${scene?.metrics[3]} / ${scene?.metrics[4]}`}</dd>
					</div>
					<div>
						<dt>Capacity transfers / units / pseudo-arcs</dt>
						<dd>{`${scene?.metrics[5]} / ${scene?.metrics[6]} / ${scene?.metrics[7]}`}</dd>
					</div>
					<div>
						<dt>Approx / exact subproblems / augmentations</dt>
						<dd>{`${scene?.metrics[8]} / ${scene?.metrics[9]} / ${scene?.metrics[10]}`}</dd>
					</div>
					<div>
						<dt>Lift / expansion paths / residual scans</dt>
						<dd>{`${scene?.metrics[11]} / ${scene?.metrics[12]} / ${scene?.metrics[14]}`}</dd>
					</div>
				</>
			)}
			{isDualNetworkSimplexAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Pivots / zero-price pivots</dt>
						<dd>{`${scene?.metrics[0]} / ${scene?.metrics[4]}`}</dd>
					</div>
					<div>
						<dt>Leaving searches / entering scans</dt>
						<dd>{`${scene?.metrics[1]} / ${scene?.metrics[2]}`}</dd>
					</div>
					<div>
						<dt>Shortest-path scans / tree rebuilds</dt>
						<dd>{`${scene?.metrics[3]} / ${scene?.metrics[5]}`}</dd>
					</div>
					<div>
						<dt>Cut-side price updates</dt>
						<dd>{scene?.metrics[6]}</dd>
					</div>
				</>
			)}
			{isPolynomialDualSimplexAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Scaling phases / augmentations / pivots</dt>
						<dd>{`${scene?.metrics[0]} / ${scene?.metrics[1]} / ${scene?.metrics[2]}`}</dd>
					</div>
					<div>
						<dt>Active / bad-arc searches</dt>
						<dd>{`${scene?.metrics[3]} / ${scene?.metrics[4]}`}</dd>
					</div>
					<div>
						<dt>Total / initial / augmentation / pricing arc scans</dt>
						<dd>{`${scene?.metrics[5]} / ${scene?.metrics[6]} / ${scene?.metrics[7]} / ${scene?.metrics[8]}`}</dd>
					</div>
					<div>
						<dt>Tree rebuilds / zero-price pivots / price updates</dt>
						<dd>{`${scene?.metrics[9]} / ${scene?.metrics[10]} / ${scene?.metrics[11]}`}</dd>
					</div>
				</>
			)}
			{isPolynomialPrimalSimplexAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Scaling phases / primal pivots</dt>
						<dd>{`${scene?.metrics[0]} / ${scene?.metrics[1]}`}</dd>
					</div>
					<div>
						<dt>Admissible searches / arc scans</dt>
						<dd>{`${scene?.metrics[2]} / ${scene?.metrics[3]}`}</dd>
					</div>
					<div>
						<dt>Premultiplier updates / nodes / reawakened</dt>
						<dd>{`${scene?.metrics[4]} / ${scene?.metrics[5]} / ${scene?.metrics[6]}`}</dd>
					</div>
					<div>
						<dt>Basis exchanges / bound flips / cycle scans</dt>
						<dd>{`${scene?.metrics[7]} / ${scene?.metrics[8]} / ${scene?.metrics[9]}`}</dd>
					</div>
					<div>
						<dt>Optimality searches / scans / tree rebuilds</dt>
						<dd>{`${scene?.metrics[10]} / ${scene?.metrics[11]} / ${scene?.metrics[12]}`}</dd>
					</div>
				</>
			)}
			{isDoubleScalingAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Cost / capacity phases</dt>
						<dd>{`${scene?.metrics[0]} / ${scene?.metrics[1]}`}</dd>
					</div>
					<div>
						<dt>Searches / exact-Δ augmentations</dt>
						<dd>{`${scene?.metrics[4]} / ${scene?.metrics[3]}`}</dd>
					</div>
					<div>
						<dt>Advance / relabel / retreat</dt>
						<dd>{`${scene?.metrics[6]} / ${scene?.metrics[5]} / ${scene?.metrics[7]}`}</dd>
					</div>
				</>
			)}
			{isCostScalingAlgorithm(scene?.algorithm.id) &&
				!isAugmentRelabelMcfAlgorithm(scene?.algorithm.id) &&
				!isPriceRefinementAlgorithm(scene?.algorithm.id) && (
					<>
						<div>
							<dt>Pushes · saturating / non-sat</dt>
							<dd>
								{arcFixingMetrics === undefined
									? `${scene?.metrics[11]} · ${scene?.metrics[12]} / ${scene?.metrics[13]}`
									: `${arcFixingMetrics.pushes} · ${arcFixingMetrics.saturatingPushes} / ${arcFixingMetrics.nonsaturatingPushes}`}
							</dd>
						</div>
						<div>
							<dt>Relabels</dt>
							<dd>{arcFixingMetrics?.relabels ?? scene?.metrics[7]}</dd>
						</div>
						<div>
							<dt>Discharges / selections</dt>
							<dd>
								{arcFixingMetrics === undefined
									? `${scene?.metrics[14]} / ${scene?.metrics[15]}`
									: `${arcFixingMetrics.discharges} / ${arcFixingMetrics.activeVertexSelections}`}
							</dd>
						</div>
					</>
				)}
			{isPriceRefinementAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Attempts · success / failure</dt>
						<dd>{`${scene?.metrics[4]} · ${scene?.metrics[3]} / ${scene?.metrics[6]}`}</dd>
					</div>
					<div>
						<dt>Price rounds / relaxations</dt>
						<dd>{`${scene?.metrics[1]} / ${scene?.metrics[9]}`}</dd>
					</div>
					<div>
						<dt>Fallback saturations / pushes</dt>
						<dd>{`${scene?.metrics[10]} / ${scene?.metrics[11]}`}</dd>
					</div>
					<div>
						<dt>Fallback relabels / discharges</dt>
						<dd>{`${scene?.metrics[7]} / ${scene?.metrics[14]}`}</dd>
					</div>
				</>
			)}
			{isArcFixingAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Fixing passes / currently fixed</dt>
						<dd>{`${arcFixingMetrics?.fixingPasses} / ${fixedEdgeIds.size}`}</dd>
					</div>
					<div>
						<dt>Unfixed / fix-ins</dt>
						<dd>{`${arcFixingMetrics?.arcsUnfixed} / ${arcFixingMetrics?.fixIns}`}</dd>
					</div>
					<div>
						<dt>Restricted-set recoveries</dt>
						<dd>{arcFixingMetrics?.recoveries}</dd>
					</div>
				</>
			)}
			{isOutOfKilterAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Price updates</dt>
						<dd>{scene?.metrics[7]}</dd>
					</div>
					<div>
						<dt>Selected out-of-kilter arcs</dt>
						<dd>{scene?.metrics[15]}</dd>
					</div>
				</>
			)}
			{isRelaxationAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Root iterations / labeled nodes</dt>
						<dd>{`${scene?.metrics[4]} / ${scene?.metrics[14]}`}</dd>
					</div>
					<div>
						<dt>Price adjustments</dt>
						<dd>{scene?.metrics[7]}</dd>
					</div>
					<div>
						<dt>Augmented flow units</dt>
						<dd>{scene?.metrics[9]}</dd>
					</div>
					<div>
						<dt>Boundary flow updates</dt>
						<dd>{scene?.metrics[11]}</dd>
					</div>
				</>
			)}
			{isEpsilonRelaxationAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Complete up iterations</dt>
						<dd>{scene?.metrics[4]}</dd>
					</div>
					<div>
						<dt>Price rises</dt>
						<dd>{scene?.metrics[7]}</dd>
					</div>
					<div>
						<dt>Pushes · saturating / non-sat</dt>
						<dd>{`${scene?.metrics[11]} · ${scene?.metrics[12]} / ${scene?.metrics[13]}`}</dd>
					</div>
					<div>
						<dt>Pushed flow units</dt>
						<dd>{scene?.metrics[9]}</dd>
					</div>
				</>
			)}
			{isAugmentRelabelMcfAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Arc pushes · saturating / non-sat</dt>
						<dd>{`${scene?.metrics[11]} · ${scene?.metrics[12]} / ${scene?.metrics[13]}`}</dd>
					</div>
					<div>
						<dt>Relabels</dt>
						<dd>{scene?.metrics[7]}</dd>
					</div>
					<div>
						<dt>Path searches / advances</dt>
						<dd>{`${scene?.metrics[4]} / ${scene?.metrics[9]}`}</dd>
					</div>
					<div>
						<dt>Deficit / length-limit endpoints</dt>
						<dd>{`${scene?.metrics[6]} / ${scene?.metrics[10]}`}</dd>
					</div>
					<div>
						<dt>Active-root selections</dt>
						<dd>{scene?.metrics[15]}</dd>
					</div>
				</>
			)}
			{isNetworkSimplexAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Nondegenerate / degenerate pivots</dt>
						<dd>{`${scene?.metrics[12]} / ${scene?.metrics[13]}`}</dd>
					</div>
					<div>
						<dt>Basis exchanges / bound flips</dt>
						<dd>{`${scene?.metrics[11]} / ${scene?.metrics[10]}`}</dd>
					</div>
					<div>
						<dt>Cycle scans / potential rebuilds</dt>
						<dd>{`${scene?.metrics[8]} / ${scene?.metrics[7]}`}</dd>
					</div>
					<div>
						<dt>Artificial balances</dt>
						<dd>
							{networkSimplexArtificialBalance === undefined
								? "—"
								: `${networkSimplexArtificialBalance.nodes} nodes · |bᵃ| ${networkSimplexArtificialBalance.absolute}`}
						</dd>
					</div>
				</>
			)}
			{isDynamicTreeNetworkSimplexAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Directional forest rebuilds / validations</dt>
						<dd>{`${scene?.metrics[0]} / ${scene?.metrics[6]}`}</dd>
					</div>
					<div>
						<dt>Path minima / lazy path updates</dt>
						<dd>{`${scene?.metrics[1]} / ${scene?.metrics[5]}`}</dd>
					</div>
					<div>
						<dt>Link-Cut links / cuts</dt>
						<dd>{`${scene?.metrics[9]} / ${scene?.metrics[14]}`}</dd>
					</div>
					<div>
						<dt>Explicit rooted-tree rebuilds</dt>
						<dd>{scene?.metrics[15]}</dd>
					</div>
				</>
			)}
			{isTransportationAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Pricing searches / route scans</dt>
						<dd>{`${scene?.metrics[4]} / ${scene?.metrics[2]}`}</dd>
					</div>
					<div>
						<dt>Positive / degenerate pivots</dt>
						<dd>{`${scene?.metrics[10]} / ${scene?.metrics[8]}`}</dd>
					</div>
					<div>
						<dt>Basis extensions / exchanges</dt>
						<dd>{`${scene?.metrics[5]} / ${scene?.metrics[6]}`}</dd>
					</div>
					<div>
						<dt>Potential rebuilds / structure scans</dt>
						<dd>{`${scene?.metrics[7]} / ${scene?.metrics[9]}`}</dd>
					</div>
				</>
			)}
			{scene !== undefined &&
				[
					"dinic",
					"unit-capacity-dinic",
					"unit-network-dinic",
					"dynamic-tree-blocking-flow",
					"karzanov-preflow",
					"mpm",
					"blocking-flow-primal-dual",
				].includes(scene.algorithm.id) && (
					<div>
						<dt>Blocking phases</dt>
						<dd>{scene.metrics[6]}</dd>
					</div>
				)}
			{scene !== undefined &&
				["shortest-augmenting-path", "isap"].includes(scene.algorithm.id) && (
					<div>
						<dt>Relabels / retreats</dt>
						<dd>{`${scene.metrics[7]} / ${scene.metrics[8]}`}</dd>
					</div>
				)}
			{scene?.algorithm.id === "isap" && (
				<div>
					<dt>Reverse BFS / gaps</dt>
					<dd>{`${scene.metrics[9]} / ${scene.metrics[10]}`}</dd>
				</div>
			)}
			{isDynamicTreeBlockingAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Tree links / cuts</dt>
						<dd>{`${scene?.metrics[11]} / ${scene?.metrics[12]}`}</dd>
					</div>
					<div>
						<dt>Root-path minima / updates</dt>
						<dd>{`${scene?.metrics[4]} / ${scene?.metrics[13]}`}</dd>
					</div>
					<div>
						<dt>Dead-root prunes</dt>
						<dd>{scene?.metrics[8]}</dd>
					</div>
					<div>
						<dt>Kernel transitions</dt>
						<dd>{scene?.metrics[15]}</dd>
					</div>
				</>
			)}
			{isDynamicTreePushRelabelAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Tree size limit k / gate rejects</dt>
						<dd>{`${scene?.metrics[1]} / ${scene?.metrics[10]}`}</dd>
					</div>
					<div>
						<dt>Tree links / cuts</dt>
						<dd>{`${scene?.metrics[5]} / ${scene?.metrics[6]}`}</dd>
					</div>
					<div>
						<dt>Root-path sends / size queries</dt>
						<dd>{`${scene?.metrics[3]} / ${scene?.metrics[4]}`}</dd>
					</div>
					<div>
						<dt>Final materializations / queue adds</dt>
						<dd>{`${scene?.metrics[8]} / ${scene?.metrics[9]}`}</dd>
					</div>
				</>
			)}
			{isPartialAugmentRelabelAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Path searches / retreats</dt>
						<dd>{`${scene?.metrics[4]} / ${scene?.metrics[8]}`}</dd>
					</div>
					<div>
						<dt>Arc pushes</dt>
						<dd>{scene?.metrics[11]}</dd>
					</div>
					<div>
						<dt>Saturating / non-sat</dt>
						<dd>{`${scene?.metrics[12]} / ${scene?.metrics[13]}`}</dd>
					</div>
					<div>
						<dt>Relabels / selections</dt>
						<dd>{`${scene?.metrics[7]} / ${scene?.metrics[15]}`}</dd>
					</div>
				</>
			)}
			{isPushRelabelAlgorithm(scene?.algorithm.id) &&
				!isPartialAugmentRelabelAlgorithm(scene?.algorithm.id) && (
					<>
						{!isSynchronousPushRelabelAlgorithm(scene?.algorithm.id) && (
							<div>
								<dt>Saturating / non-sat</dt>
								<dd>{`${scene?.metrics[12]} / ${scene?.metrics[13]}`}</dd>
							</div>
						)}
						<div>
							<dt>Relabels</dt>
							<dd>{scene?.metrics[7]}</dd>
						</div>
						{isSynchronousPushRelabelAlgorithm(scene?.algorithm.id) && (
							<>
								<div>
									<dt>Logical rounds / global relabels</dt>
									<dd>{`${scene?.metrics[1]} / ${scene?.metrics[9]}`}</dd>
								</div>
								<div>
									<dt>Active visits / ownership deferrals</dt>
									<dd>{`${scene?.metrics[15]} / ${scene?.metrics[8]}`}</dd>
								</div>
								<div>
									<dt>Recovery paths</dt>
									<dd>{scene?.metrics[3]}</dd>
								</div>
							</>
						)}
						{isRelabelHeuristicAlgorithm(scene?.algorithm.id) && (
							<div>
								<dt>Global / gap relabels</dt>
								<dd>{`${scene?.metrics[9]} / ${scene?.metrics[10]}`}</dd>
							</div>
						)}
						{isExcessScalingPushRelabelAlgorithm(scene?.algorithm.id) ? (
							<div>
								<dt>Scaled selections</dt>
								<dd>{scene?.metrics[15]}</dd>
							</div>
						) : !isSynchronousPushRelabelAlgorithm(scene?.algorithm.id) ? (
							<div>
								<dt>Discharges / selections</dt>
								<dd>{`${scene?.metrics[14]} / ${scene?.metrics[15]}`}</dd>
							</div>
						) : null}
					</>
				)}
			{isWarmStartPushRelabelAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Prediction error η</dt>
						<dd>{scene?.metrics[1]}</dd>
					</div>
					<div>
						<dt>Cut saturation / imbalance error</dt>
						<dd>{`${scene?.metrics[5]} / ${scene?.metrics[6]}`}</dd>
					</div>
					<div>
						<dt>Auxiliary solves / cut transfers</dt>
						<dd>{`${scene?.metrics[0]} / ${scene?.metrics[8]}`}</dd>
					</div>
					<div>
						<dt>Recovery paths / predicted edges</dt>
						<dd>{`${scene?.metrics[4]} / ${scene?.metrics[9]}`}</dd>
					</div>
					<div>
						<dt>Gap relabels / auxiliary pushes</dt>
						<dd>{`${scene?.metrics[10]} / ${scene?.metrics[11]}`}</dd>
					</div>
				</>
			)}
			{isBlockingPreflowAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<dt>Saturating / non-sat</dt>
						<dd>{`${scene?.metrics[12]} / ${scene?.metrics[13]}`}</dd>
					</div>
					{scene?.algorithm.id === "karzanov-preflow" ? (
						<div>
							<dt>Balancing iterations</dt>
							<dd>{scene.metrics[14]}</dd>
						</div>
					) : (
						<div>
							<dt>Vertex eliminations</dt>
							<dd>{scene?.metrics[15]}</dd>
						</div>
					)}
				</>
			)}
			{isPseudoflowAlgorithm(scene?.algorithm.id) && (
				<>
					{scene?.algorithm.id === PSEUDOFLOW_SIMPLEX_ALGORITHM ? (
						<>
							<div>
								<dt>Pivots / relabels</dt>
								<dd>{`${scene.metrics[15]} / ${scene.metrics[7]}`}</dd>
							</div>
							<div>
								<dt>Cycle arcs / degenerate pivots</dt>
								<dd>{`${scene.metrics[4]} / ${scene.metrics[14]}`}</dd>
							</div>
							<div>
								<dt>Internal / entering leaves</dt>
								<dd>{`${scene.metrics[8]} / ${scene.metrics[9]}`}</dd>
							</div>
							<div>
								<dt>Strong / weak root leaves</dt>
								<dd>{`${scene.metrics[5]} / ${scene.metrics[6]}`}</dd>
							</div>
							<div>
								<dt>Pivot / recovery pushes</dt>
								<dd>{scene.metrics[11]}</dd>
							</div>
						</>
					) : (
						<>
							<div>
								<dt>Normalization / recovery pushes</dt>
								<dd>{scene?.metrics[11]}</dd>
							</div>
							<div>
								<dt>Relabels / mergers</dt>
								<dd>{`${scene?.metrics[7]} / ${scene?.metrics[15]}`}</dd>
							</div>
						</>
					)}
					<div>
						<dt>Saturating / non-sat</dt>
						<dd>{`${scene?.metrics[12]} / ${scene?.metrics[13]}`}</dd>
					</div>
				</>
			)}
			<div>
				<dt>
					{scene?.model.kind === "parametric-max-flow"
						? "Current tied-cut nodes"
						: isHassinStPlanarAlgorithm(scene?.algorithm.id)
							? "Active dual-crossing edge"
							: isBorradaileKleinPlanarAlgorithm(scene?.algorithm.id)
								? "Active leftmost path"
								: isNetworkSimplexAlgorithm(scene?.algorithm.id) ||
										isTransportationAlgorithm(scene?.algorithm.id)
									? "Active basic cycle"
									: isWarmStartPushRelabelAlgorithm(scene?.algorithm.id)
										? "Active warm-start repair path"
										: scene?.algorithm.id === PSEUDOFLOW_SIMPLEX_ALGORITHM
											? "Active normalized-basis cycle"
											: isConvexCostAlgorithm(scene?.algorithm.id)
												? scene?.algorithm.id === CONVEX_COST_SCALING_ALGORITHM
													? "Active marginal path"
													: "Active marginal cycle"
												: "Active path"}
				</dt>
				<dd>
					{scene?.model.kind === "parametric-max-flow"
						? (projectFlowParametricCut(scene)?.tiedNodes.join(" · ") ?? "—") ||
							"none"
						: currentOverlayViews?.convexCost !== undefined
							? currentOverlayViews.convexCost.active_cycle.length
							: (scene?.residual_arcs.filter((arc) => arc.active).length ??
								"—")}
				</dd>
			</div>
		</dl>
	);
}
