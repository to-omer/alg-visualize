import {
	CONVEX_COST_SCALING_ALGORITHM,
	isArcFixingAlgorithm,
	isCancelAndTightenAlgorithm,
	isDistanceDirectedAlgorithm,
	isDoubleScalingAlgorithm,
	isDualNetworkSimplexAlgorithm,
	isDynamicTreeNetworkSimplexAlgorithm,
	isDynamicTreePushRelabelAlgorithm,
	isEnhancedCapacityScalingAlgorithm,
	isNetworkSimplexAlgorithm,
	isOrlinMaxFlowAlgorithm,
	isOrlinMcfAlgorithm,
	isPolynomialDualSimplexAlgorithm,
	isPolynomialPrimalSimplexAlgorithm,
	isRelaxedMndcAlgorithm,
	isRootwardForestAlgorithm,
	isTransportationAlgorithm,
	isWarmStartPushRelabelAlgorithm,
} from "./flow-algorithm-presentation";
import { isEibfsAlgorithm } from "./flow-eibfs-view";
import { isIbfsAlgorithm } from "./flow-ibfs-view";
import type { FlowOverlayPresentation } from "./flow-overlay-presentation";
import { isGridgenScene } from "./flow-render-plan";
import type { FlowCurrentSceneV9 } from "./flow-scene";

type FlowInspectorLegendProps = Readonly<{
	scene: FlowCurrentSceneV9 | undefined;
	presentation: FlowOverlayPresentation | undefined;
}>;

export function FlowInspectorLegendSection(props: FlowInspectorLegendProps) {
	const scene = props.scene;
	const currentOverlayViews = props.presentation?.renderData.overlayViews;
	const showsCost = !new Set([
		"max-flow",
		"parametric-max-flow",
		"planar-max-flow",
		"bipartite-matching",
	]).has(scene?.model.kind ?? "max-flow");
	return (
		<details className="flow-legend">
			<summary>Visual encoding details</summary>
			<div className="flow-legend-content">
				<h2 className="visually-hidden">Edge visual encoding</h2>
				<div>
					<span className="legend-capacity" aria-hidden="true" />
					<p>
						<strong>
							{scene?.model.kind === "parametric-max-flow"
								? "Current capacity u(λ)"
								: "Capacity"}
						</strong>
						<small>
							{scene?.model.kind === "parametric-max-flow"
								? "Width uses one fixed scale across all λ values"
								: "Neutral outer-rail width"}
						</small>
					</p>
				</div>
				<div>
					<span className="legend-flow" aria-hidden="true" />
					<p>
						<strong>Current flow</strong>
						<small>Light inner-line width relative to capacity</small>
					</p>
				</div>
				{scene?.model.kind === "parametric-max-flow" ? (
					<>
						<div>
							<span className="legend-parametric-tie" aria-hidden="true" />
							<p>
								<strong>Tied cut nodes</strong>
								<small>
									Difference between minimum and maximum source-side cuts
								</small>
							</p>
						</div>
						<div>
							<span className="legend-parametric-current" aria-hidden="true" />
							<p>
								<strong>Current λ</strong>
								<small>Orange dashed line in the F(λ) chart</small>
							</p>
						</div>
					</>
				) : currentOverlayViews?.deterministicAlmostLinear !== undefined ? (
					<>
						<div>
							<span
								className="legend-deterministic-gradient"
								aria-hidden="true"
							/>
							<p>
								<strong>Source length ℓ / gradient g</strong>
								<small>
									Outer width = ℓ · teal = negative · orange = positive ·
									intensity = |g|
								</small>
							</p>
						</div>
						<div>
							<span
								className="legend-deterministic-tree-chain"
								aria-hidden="true"
							/>
							<p>
								<strong>Tree chain / partial forest</strong>
								<small>
									Teal dash-dot = level membership · long green dash =
									pre-contraction forest
								</small>
							</p>
						</div>
						<div>
							<span className="legend-deterministic-core" aria-hidden="true" />
							<p>
								<strong>Core / spanner / embedding</strong>
								<small>
									Orange dashes = core · solid gold = spanner · violet dots =
									embedding summary
								</small>
							</p>
						</div>
						<div>
							<span className="legend-deterministic-cycle" aria-hidden="true" />
							<p>
								<strong>Query cycle / Detect</strong>
								<small>
									Magenta arrows = ± direction · yellow glow = changed
									coordinate
								</small>
							</p>
						</div>
						<div>
							<span
								className="legend-deterministic-rounding"
								aria-hidden="true"
							/>
							<p>
								<strong>Fractional forest / rounding cycle</strong>
								<small>
									Double cyan dashes = forest · pink arrows = cost-nonincreasing
									cycle
								</small>
							</p>
						</div>
						<div>
							<span
								className="legend-deterministic-auxiliary"
								aria-hidden="true"
							/>
							<p>
								<strong>Return t→s / artificial v*</strong>
								<small>
									mU return edge and strict-interior initialization edges · ends
									with an additive-1/2 final point and Kang–Payor rounding
								</small>
							</p>
						</div>
					</>
				) : currentOverlayViews?.randomizedAlmostLinear !== undefined ? (
					<>
						<div>
							<span className="legend-randomized-gradient" aria-hidden="true" />
							<p>
								<strong>Source length ℓ / gradient g</strong>
								<small>
									Outer width = ℓ · blue = negative · orange = positive ·
									intensity = |g|
								</small>
							</p>
						</div>
						<div>
							<span
								className="legend-randomized-tree-chain"
								aria-hidden="true"
							/>
							<p>
								<strong>Sampled tree chain</strong>
								<small>
									Cyan dashes = membership count · long green dash = queried
									tree
								</small>
							</p>
						</div>
						<div>
							<span className="legend-randomized-cycle" aria-hidden="true" />
							<p>
								<strong>Min-ratio cycle / Detect</strong>
								<small>
									Violet arrows = ± direction · yellow glow = changed coordinate
								</small>
							</p>
						</div>
						<div>
							<span
								className="legend-randomized-auxiliary"
								aria-hidden="true"
							/>
							<p>
								<strong>Return t→s / artificial v*</strong>
								<small>
									mU return edge and strict-interior initialization edges · ends
									with the isolation final point and nearest-integer rounding
								</small>
							</p>
						</div>
					</>
				) : currentOverlayViews?.minimumRatioCycle !== undefined ? (
					<>
						<div>
							<span
								className="legend-minimum-ratio-gradient"
								aria-hidden="true"
							/>
							<p>
								<strong>Length ℓ / gradient g</strong>
								<small>
									Outer width = ℓ · blue = negative · orange = positive ·
									intensity = |g|
								</small>
							</p>
						</div>
						<div>
							<span
								className="legend-minimum-ratio-forest"
								aria-hidden="true"
							/>
							<p>
								<strong>Deterministic spanning forest</strong>
								<small>Dashed blue · verifies m−n+c fundamental cycles</small>
							</p>
						</div>
						<div>
							<span className="legend-minimum-ratio-cycle" aria-hidden="true" />
							<p>
								<strong>Candidate z / best z*</strong>
								<small>
									Orange = evaluating · violet = minimum ratio · arrow = ±1
									direction
								</small>
							</p>
						</div>
					</>
				) : currentOverlayViews?.interiorPointMaxFlow !== undefined ? (
					<>
						<div>
							<span className="legend-interior-fractional" aria-hidden="true" />
							<p>
								<strong>Central-path fractional flow x</strong>
								<small>
									Outer rail = unit capacity · pale-green inner width =
									fractional flow
								</small>
							</p>
						</div>
						<div>
							<span className="legend-interior-dual" aria-hidden="true" />
							<p>
								<strong>Electrical direction f̂ / dual values</strong>
								<small>
									Cyan / dashed violet = direction · gold = slack · dots =
									resistance
								</small>
							</p>
						</div>
						<div>
							<span className="legend-interior-cut" aria-hidden="true" />
							<p>
								<strong>Congestion ρ / target cut</strong>
								<small>
									Amber → red glow = congestion · double node ring = exact cut
								</small>
							</p>
						</div>
					</>
				) : currentOverlayViews?.augmentingElectrical !== undefined ? (
					<>
						<div>
							<span className="legend-augmenting-central" aria-hidden="true" />
							<p>
								<strong>Central flow x / capacity rail</strong>
								<small>
									Outer rail = original capacity · pale green = transformed
									fractional central flow
								</small>
							</p>
						</div>
						<div>
							<span
								className="legend-augmenting-direction"
								aria-hidden="true"
							/>
							<p>
								<strong>Electrical direction f̂ / congestion ρ</strong>
								<small>
									Cyan = edge direction · dashed violet = reverse · amber → red
									glow = congestion
								</small>
							</p>
						</div>
						<div>
							<span className="legend-augmenting-boost" aria-hidden="true" />
							<p>
								<strong>Boost path / target cut</strong>
								<small>
									Outer dashes = explicit expansion · double node ring = exact
									cut side
								</small>
							</p>
						</div>
						{currentOverlayViews.augmentingElectrical.edges.some(
							(edge) => edge.rounded_central_flow !== undefined,
						) && (
							<div>
								<span
									className="legend-augmenting-rounded"
									aria-hidden="true"
								/>
								<p>
									<strong>Rounded integer central flow</strong>
									<small>
										Blue dashes = rounded value · bright only where rounding
										changed the continuous flow
									</small>
								</p>
							</div>
						)}
						{currentOverlayViews.augmentingElectrical.edges.some(
							(edge) => edge.extraction_central_scaled !== undefined,
						) && (
							<div>
								<span
									className="legend-augmenting-extraction"
									aria-hidden="true"
								/>
								<p>
									<strong>Directed-reduction extraction</strong>
									<small>
										Blue = doubled directed flow · amber = auxiliary arcs ·
										violet = cycle being canceled
									</small>
								</p>
							</div>
						)}
					</>
				) : currentOverlayViews?.electricalFlow !== undefined ? (
					<>
						<div>
							<span className="legend-electrical-current" aria-hidden="true" />
							<p>
								<strong>Signed current I</strong>
								<small>
									Teal = edge direction · dashed violet = reverse · width =
									congestion |I|/u
								</small>
							</p>
						</div>
						<div>
							<span className="legend-electrical-energy" aria-hidden="true" />
							<p>
								<strong>Edge energy I²R</strong>
								<small>
									Amber glow intensity · numeric labels include E and R
								</small>
							</p>
						</div>
						<div>
							<span
								className="legend-electrical-potential"
								aria-hidden="true"
							/>
							<p>
								<strong>Node potential φ / ground</strong>
								<small>Cool → warm ring · ⏚ is the sink at φ=0</small>
							</p>
						</div>
					</>
				) : currentOverlayViews?.tardosFramework !== undefined ? (
					<>
						<div>
							<span className="legend-tardos-reduced" aria-hidden="true" />
							<p>
								<strong>Residual reduced cost c̄</strong>
								<small>
									Solid teal = positive · dashed violet = negative · dotted gray
									= 0 · width = |c̄|
								</small>
							</p>
						</div>
						<div>
							<span className="legend-tardos-threshold" aria-hidden="true" />
							<p>
								<strong>Exact threshold c̄ &gt; nε</strong>
								<small>
									Double orange stroke = residual direction that certifies a
									fixed value
								</small>
							</p>
						</div>
						<div>
							<span className="legend-tardos-fixed" aria-hidden="true" />
							<p>
								<strong>Original variable fixed at L / U</strong>
								<small>
									Solid = lower bound L · dashed = upper bound U · label gives
									fixed value
								</small>
							</p>
						</div>
					</>
				) : currentOverlayViews?.predictionAssistedEpsilon !== undefined ? (
					<>
						<div>
							<span className="legend-prediction-cost" aria-hidden="true" />
							<p>
								<strong>Cost cₜ at scale t</strong>
								<small>
									Solid teal = positive · dashed violet = negative · dotted gray
									= 0 · width = absolute value
								</small>
							</p>
						</div>
						<div>
							<span className="legend-prediction-node" aria-hidden="true" />
							<p>
								<strong>Prediction / clipped / active</strong>
								<small>Violet dots / broken magenta / double orange ring</small>
							</p>
						</div>
						<div>
							<span className="legend-prediction-attempt" aria-hidden="true" />
							<p>
								<strong>Remark 1 T ladder</strong>
								<small>
									Hatched × = aborted exponential trial · solid = current or
									successful
								</small>
							</p>
						</div>
					</>
				) : currentOverlayViews?.convexCost !== undefined ? (
					<>
						<div>
							<span className="legend-convex-marginal" aria-hidden="true" />
							<p>
								<strong>Marginal cost and segment utilization</strong>
								<small>
									Cool = negative · gray = 0 · warm = positive · fill width =
									segment usage
								</small>
							</p>
						</div>
						{scene?.algorithm.id === CONVEX_COST_SCALING_ALGORITHM && (
							<div>
								<span className="legend-convex-eligible" aria-hidden="true" />
								<p>
									<strong>Δ-eligible marginal segment</strong>
									<small>
										Dashed violet · residual capacity at least current Δ
									</small>
								</p>
							</div>
						)}
						<div>
							<span
								className="legend-convex-cycle-forward"
								aria-hidden="true"
							/>
							<p>
								<strong>
									{scene?.algorithm.id === CONVEX_COST_SCALING_ALGORITHM
										? "Selected path · forward"
										: "Improving cycle · forward"}
								</strong>
								<small>
									Solid orange · increases flow in the selected segment
								</small>
							</p>
						</div>
						<div>
							<span
								className="legend-convex-cycle-reverse"
								aria-hidden="true"
							/>
							<p>
								<strong>
									{scene?.algorithm.id === CONVEX_COST_SCALING_ALGORITHM
										? "Selected path · reverse"
										: "Improving cycle · reverse"}
								</strong>
								<small>
									Dashed orange · returns flow through the selected segment
								</small>
							</p>
						</div>
					</>
				) : showsCost ? (
					<>
						<div>
							<span className="legend-positive" aria-hidden="true" />
							<p>
								<strong>Positive cost</strong>
								<small>Solid amber; continuous intensity shows |cost|</small>
							</p>
						</div>
						<div>
							<span className="legend-negative" aria-hidden="true" />
							<p>
								<strong>Negative cost</strong>
								<small>Dashed cyan; continuous intensity shows |cost|</small>
							</p>
						</div>
						<div>
							<span className="legend-zero" aria-hidden="true" />
							<p>
								<strong>Zero cost</strong>
								<small>Short dotted neutral rail</small>
							</p>
						</div>
						<div>
							<span className="legend-mixed" aria-hidden="true" />
							<p>
								<strong>Mixed aggregate cost</strong>
								<small>Long-short dash in Overview</small>
							</p>
						</div>
					</>
				) : null}
				{isArcFixingAlgorithm(scene?.algorithm.id) && (
					<div>
						<span className="legend-fixed" aria-hidden="true" />
						<p>
							<strong>Temporarily fixed</strong>
							<small>
								Wide long dashes with low saturation · excluded from refine
								search
							</small>
						</p>
					</div>
				)}
				<div>
					<span className="legend-terminal" aria-hidden="true" />
					<p>
						<strong>Source and sink</strong>
						<small>Double rings</small>
					</p>
				</div>
				{scene?.graph.nodes.some((node) => BigInt(node.supply) > 0n) && (
					<div>
						<span className="legend-supply" aria-hidden="true" />
						<p>
							<strong>Supply node</strong>
							<small>Solid ring · positive balance</small>
						</p>
					</div>
				)}
				{scene?.graph.nodes.some((node) => BigInt(node.supply) < 0n) && (
					<div>
						<span className="legend-demand" aria-hidden="true" />
						<p>
							<strong>Demand node</strong>
							<small>Dashed ring · negative balance</small>
						</p>
					</div>
				)}
				{scene?.outcome?.kind === "infeasible" && (
					<div>
						<span className="legend-infeasible-cut" aria-hidden="true" />
						<p>
							<strong>Infeasible-cut witness</strong>
							<small>Dotted orange nodes · boundary routes</small>
						</p>
					</div>
				)}
				{scene !== undefined && isGridgenScene(scene) && (
					<div>
						<span className="legend-super" aria-hidden="true" />
						<p>
							<strong>GRIDGEN supernode</strong>
							<small>Dotted ring · certifies feasibility</small>
						</p>
					</div>
				)}
				{isIbfsAlgorithm(scene?.algorithm.id) && (
					<>
						<div>
							<span className="legend-ibfs-source" aria-hidden="true" />
							<p>
								<strong>S tree · dₛ</strong>
								<small>
									{scene?.algorithm.id === "boykov-kolmogorov"
										? "Solid teal · reused source tree"
										: "Solid teal · S distance labels"}
								</small>
							</p>
						</div>
						<div>
							<span className="legend-ibfs-sink" aria-hidden="true" />
							<p>
								<strong>T tree · dₜ</strong>
								<small>
									{scene?.algorithm.id === "boykov-kolmogorov"
										? "Dashed indigo · reused sink tree"
										: "Dashed indigo · T distance labels"}
								</small>
							</p>
						</div>
						<div>
							<span className="legend-ibfs-orphan" aria-hidden="true" />
							<p>
								<strong>Orphan / frontier</strong>
								<small>Dashed orange ring / outer dotted ring</small>
							</p>
						</div>
					</>
				)}
				{isEibfsAlgorithm(scene?.algorithm.id) && (
					<>
						<div>
							<span className="legend-eibfs-source" aria-hidden="true" />
							<p>
								<strong>S forest · dₛ</strong>
								<small>
									Solid teal · residual arc directed away from the root
								</small>
							</p>
						</div>
						<div>
							<span className="legend-eibfs-sink" aria-hidden="true" />
							<p>
								<strong>T forest · dₜ</strong>
								<small>
									Dashed indigo · residual arc directed toward the root
								</small>
							</p>
						</div>
						<div>
							<span className="legend-eibfs-root" aria-hidden="true">
								±
							</span>
							<p>
								<strong>Root · imbalance</strong>
								<small>±∞ = terminal · ± = finite excess/deficit root</small>
							</p>
						</div>
						<div>
							<span className="legend-eibfs-orphan" aria-hidden="true" />
							<p>
								<strong>Orphan / frontier / repair</strong>
								<small>Dashed orange / outer dots / rounded frame</small>
							</p>
						</div>
					</>
				)}
				{!isIbfsAlgorithm(scene?.algorithm.id) &&
					!isEibfsAlgorithm(scene?.algorithm.id) &&
					!isWarmStartPushRelabelAlgorithm(scene?.algorithm.id) &&
					(scene?.pseudoflow_forest !== undefined ||
						isNetworkSimplexAlgorithm(scene?.algorithm.id) ||
						isTransportationAlgorithm(scene?.algorithm.id)) && (
						<div>
							<span className="legend-forest" aria-hidden="true" />
							<p>
								<strong>
									{isDynamicTreeNetworkSimplexAlgorithm(scene?.algorithm.id)
										? "Directional link-cut basis tree"
										: isNetworkSimplexAlgorithm(scene?.algorithm.id) ||
												isTransportationAlgorithm(scene?.algorithm.id)
											? "Basis tree"
											: isDistanceDirectedAlgorithm(scene?.algorithm.id)
												? "Exact shortest-path in-tree"
												: isRootwardForestAlgorithm(scene?.algorithm.id)
													? "Represented tree"
													: "Normalization forest"}
								</strong>
								<small>
									{isTransportationAlgorithm(scene?.algorithm.id)
										? "Dashed blue · dotted ring at each basis-component root"
										: isDynamicTreeNetworkSimplexAlgorithm(scene?.algorithm.id)
											? "Dashed blue shared by directional forests · dotted ring on the artificial-root side"
											: isNetworkSimplexAlgorithm(scene?.algorithm.id)
												? "Dashed blue · dotted ring on the artificial-root side"
												: isDistanceDirectedAlgorithm(scene?.algorithm.id)
													? "Dashed blue · parent edge from each node toward the sink"
													: isRootwardForestAlgorithm(scene?.algorithm.id)
														? isDynamicTreePushRelabelAlgorithm(
																scene?.algorithm.id,
															)
															? "Dashed blue toward a lower-height root · dotted root ring"
															: "Dashed blue toward the sink · dotted root ring"
														: "Dashed blue · dotted root ring"}
								</small>
							</p>
						</div>
					)}
				{isWarmStartPushRelabelAlgorithm(scene?.algorithm.id) && (
					<div>
						<span className="legend-warm-prediction" aria-hidden="true" />
						<p>
							<strong>Predicted flow / maintained cut</strong>
							<small>Dotted rail / red S side with highlighted boundary</small>
						</p>
					</div>
				)}
				{isCancelAndTightenAlgorithm(scene?.algorithm.id) && (
					<>
						<div>
							<span className="legend-cancel-admissible" aria-hidden="true" />
							<p>
								<strong>Admissible residual</strong>
								<small>Dashed violet · strictly negative reduced cost</small>
							</p>
						</div>
						<div>
							<span className="legend-cancel-cycle" aria-hidden="true" />
							<p>
								<strong>Cancel cycle</strong>
								<small>Solid orange · cycle augmented to bottleneck Δ</small>
							</p>
						</div>
					</>
				)}
				{isRelaxedMndcAlgorithm(scene?.algorithm.id) && (
					<>
						<div>
							<span className="legend-mndc-assignment" aria-hidden="true" />
							<p>
								<strong>Split-node assignment</strong>
								<small>Dashed violet · tight left-to-right match</small>
							</p>
						</div>
						<div>
							<span className="legend-mndc-family" aria-hidden="true" />
							<p>
								<strong>Node-disjoint cycle family</strong>
								<small>
									Color-coded heavy strokes and rings · cancel each cycle to its
									bottleneck
								</small>
							</p>
						</div>
					</>
				)}
				{isEnhancedCapacityScalingAlgorithm(scene?.algorithm.id) && (
					<>
						<div>
							<span className="legend-enhanced-component" aria-hidden="true" />
							<p>
								<strong>Contracted component</strong>
								<small>
									Translucent rounded outline · shows C and exact excess
								</small>
							</p>
						</div>
						<div>
							<span className="legend-enhanced-strong" aria-hidden="true" />
							<p>
								<strong>Tight / strongly feasible</strong>
								<small>Solid teal / dotted violet · x̃ ≥ 3nΔ</small>
							</p>
						</div>
						<div>
							<span className="legend-enhanced-path" aria-hidden="true" />
							<p>
								<strong>Quotient shortest path</strong>
								<small>
									Solid orange · sends exact Δ from excess to deficit
								</small>
							</p>
						</div>
					</>
				)}
				{isOrlinMcfAlgorithm(scene?.algorithm.id) && (
					<>
						<div>
							<span className="legend-orlin-capacity" aria-hidden="true" />
							<p>
								<strong>Finite-capacity node κ</strong>
								<small>
									Diamond · splits an original edge into flow/slack branches
								</small>
							</p>
						</div>
						<div>
							<span className="legend-orlin-flow" aria-hidden="true" />
							<p>
								<strong>Flow / slack branches</strong>
								<small>Solid F / dashed S · width = exact pseudoflow</small>
							</p>
						</div>
						<div>
							<span className="legend-orlin-path" aria-hidden="true" />
							<p>
								<strong>Compressed shortest path</strong>
								<small>
									Orange · eliminated capacity-node routes shown on their
									original two branches
								</small>
							</p>
						</div>
					</>
				)}
				{isOrlinMaxFlowAlgorithm(scene?.algorithm.id) && (
					<>
						<div>
							<span className="legend-orlin-max-abundant" aria-hidden="true" />
							<p>
								<strong>A / Ā residual</strong>
								<small>
									Solid teal = abundant / long-dashed violet = anti-abundant
								</small>
							</p>
						</div>
						<div>
							<span
								className="legend-orlin-max-small-medium"
								aria-hidden="true"
							/>
							<p>
								<strong>S / M residual</strong>
								<small>Dotted gray = small / double yellow = medium</small>
							</p>
						</div>
						<div>
							<span className="legend-orlin-max-component" aria-hidden="true" />
							<p>
								<strong>K / C component</strong>
								<small>
									Solid = critical / dotted = compactible · frame = contraction
								</small>
							</p>
						</div>
						<div>
							<span className="legend-orlin-max-compact" aria-hidden="true" />
							<p>
								<strong>Compact O / P / T</strong>
								<small>original / abundant pseudo / transferred pseudo</small>
							</p>
						</div>
					</>
				)}
				{isDualNetworkSimplexAlgorithm(scene?.algorithm.id) && (
					<>
						<div>
							<span className="legend-dual-tree" aria-hidden="true" />
							<p>
								<strong>Dual-feasible tree basis</strong>
								<small>Heavy teal · tree arcs have c̄ = 0</small>
							</p>
						</div>
						<div>
							<span className="legend-dual-leaving" aria-hidden="true" />
							<p>
								<strong>Leaving / infeasible basic</strong>
								<small>Dashed red · signed basic flow xᴮ &lt; 0</small>
							</p>
						</div>
						<div>
							<span className="legend-dual-entering" aria-hidden="true" />
							<p>
								<strong>Entering / head-side cut H</strong>
								<small>Double orange stroke / blue node ring</small>
							</p>
						</div>
					</>
				)}
				{isPolynomialDualSimplexAlgorithm(scene?.algorithm.id) && (
					<>
						<div>
							<span
								className="legend-polynomial-dual-tree"
								aria-hidden="true"
							/>
							<p>
								<strong>Dual-feasible tree / pseudoflow</strong>
								<small>
									Heavy teal outer stroke / weighted dots · separate from actual
									flow
								</small>
							</p>
						</div>
						<div>
							<span
								className="legend-polynomial-dual-path"
								aria-hidden="true"
							/>
							<p>
								<strong>Exact-Δ active-to-root path</strong>
								<small>
									Long violet dashes and arrows · residual orientation
								</small>
							</p>
						</div>
						<div>
							<span
								className="legend-polynomial-dual-make-good"
								aria-hidden="true"
							/>
							<p>
								<strong>Bad / leaving / entering</strong>
								<small>
									Short red dashes / double orange · distinguishable without
									color
								</small>
							</p>
						</div>
					</>
				)}
				{isPolynomialPrimalSimplexAlgorithm(scene?.algorithm.id) && (
					<>
						<div>
							<span
								className="legend-polynomial-tree-bounds"
								aria-hidden="true"
							/>
							<p>
								<strong>Tree / lower / upper basis</strong>
								<small>Heavy solid teal / dashed gray / dotted gray</small>
							</p>
						</div>
						<div>
							<span className="legend-polynomial-cycle" aria-hidden="true" />
							<p>
								<strong>Admissible fundamental cycle</strong>
								<small>Long violet dashes · follows residual orientation</small>
							</p>
						</div>
						<div>
							<span
								className="legend-polynomial-enter-leave"
								aria-hidden="true"
							/>
							<p>
								<strong>Entering / leaving</strong>
								<small>Double orange / short red dashes</small>
							</p>
						</div>
						<div>
							<span
								className="legend-polynomial-node-state"
								aria-hidden="true"
							/>
							<p>
								<strong>Eligible / awake / N*</strong>
								<small>
									Concentric solid / dotted / long-dashed node rings
								</small>
							</p>
						</div>
					</>
				)}
				{isDoubleScalingAlgorithm(scene?.algorithm.id) && (
					<>
						<div>
							<span className="legend-double-admissible" aria-hidden="true" />
							<p>
								<strong>Transformed admissible</strong>
								<small>
									Dashed violet · negative reduced cost on a flow/slack branch
								</small>
							</p>
						</div>
						<div>
							<span className="legend-double-path" aria-hidden="true" />
							<p>
								<strong>Exact-Δ path</strong>
								<small>
									Solid orange · distinguishes costed flow from zero-cost slack
								</small>
							</p>
						</div>
					</>
				)}
				<div>
					<span className="legend-active" aria-hidden="true" />
					<p>
						<strong>
							{isNetworkSimplexAlgorithm(scene?.algorithm.id) ||
							isTransportationAlgorithm(scene?.algorithm.id)
								? "Current fundamental cycle"
								: "Current residual edge"}
						</strong>
						<small>
							{isTransportationAlgorithm(scene?.algorithm.id)
								? "Solid +θ increases; dashed −θ decreases"
								: "Violet focus outline; base data colors remain unchanged"}
						</small>
					</p>
				</div>
			</div>
		</details>
	);
}
