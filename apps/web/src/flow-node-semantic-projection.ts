import type { FlowGraphNodeKind } from "./FlowGraphSearchNodeFeatureBundle";
import {
	BLOCKING_PRIMAL_DUAL_LEVEL_EVENTS,
	isMinimumMeanCycleCancelingAlgorithm,
	isNetworkSimplexOptimalEvent,
	isPotentialDijkstraSspAlgorithm,
	isPriceCoordinateRelaxationAlgorithm,
	MPM_POTENTIAL_EVENTS,
	POTENTIAL_DIJKSTRA_PRICE_EVENTS,
} from "./flow-algorithm-presentation";
import { eibfsNodeLabel } from "./flow-eibfs-view";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import { ibfsDistanceLabel } from "./flow-ibfs-view";
import { formatFlowRational } from "./flow-parametric-view";
import type { FlowNodeV1 } from "./flow-scene";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;

/**
 * Shortens an exact decimal for the fixed-size SVG canvas.
 *
 * The unabridged value remains in the node's accessible title and Inspector;
 * this formatter is only for the spatially constrained callout.
 */
export function compactFlowCanvasNumber(value: string): string {
	const parsed = Number(value);
	if (!Number.isFinite(parsed)) return value;
	if (parsed === 0) return "0";
	const magnitude = Math.abs(parsed);
	if (magnitude >= 10_000 || magnitude < 0.001) {
		const [coefficient, exponent] = parsed.toExponential(2).split("e");
		return `${Number(coefficient)}e${Number(exponent)}`;
	}
	return Number(parsed.toPrecision(4)).toString();
}

export function orlinMcfPotentialLabel(
	stage: string | undefined,
	potential: string,
): string {
	return stage === "ready" || stage === "transform-capacities"
		? "π …"
		: `π ${potential}`;
}

function isPotentialDijkstraPriceEvent(catalogId: string | undefined): boolean {
	return (
		catalogId !== undefined && POTENTIAL_DIJKSTRA_PRICE_EVENTS.has(catalogId)
	);
}

function isBlockingPrimalDualLevelEvent(
	catalogId: string | undefined,
): boolean {
	return (
		catalogId !== undefined && BLOCKING_PRIMAL_DUAL_LEVEL_EVENTS.has(catalogId)
	);
}

function isCostScalingPriceEvent(catalogId: string | undefined): boolean {
	return (
		catalogId?.endsWith(".relabel") === true ||
		catalogId?.endsWith(".relabel-tip") === true ||
		catalogId?.endsWith(".start-refine") === true ||
		catalogId?.endsWith(".complete-refine") === true ||
		catalogId?.endsWith(".relax-price") === true ||
		catalogId?.endsWith(".fail-and-rollback-prices") === true ||
		catalogId?.endsWith(".optimal") === true
	);
}

function isMpmPotentialEvent(catalogId: string | undefined): boolean {
	return catalogId !== undefined && MPM_POTENTIAL_EVENTS.has(catalogId);
}

/**
 * Capacity/excess scaling reuses the trace label field for two different
 * quantities.  A shortest-path boundary publishes reduced distances; after
 * the dual update, the same slots publish the resulting node potentials.
 * Keeping the symbols distinct is essential when the first update has
 * numerically identical distances and potentials.
 */
export function capacityScalingTraceLabelPrefix(
	catalogId: string | undefined,
): "d̄" | "π" | undefined {
	if (
		catalogId === undefined ||
		(!catalogId.startsWith("capacity-scaling-mcf.") &&
			!catalogId.startsWith("excess-scaling-mcf."))
	) {
		return undefined;
	}
	return catalogId.endsWith(".inspect-residual-arc") ||
		catalogId.endsWith(".shortest-eligible-path") ||
		catalogId.endsWith(".shortest-large-excess-path") ||
		catalogId.endsWith(".no-eligible-deficit") ||
		catalogId.endsWith(".no-reachable-large-deficit")
		? "d̄"
		: "π";
}

/** Projects algorithm state into the accessible copy and compact node labels. */
export function projectFlowNodeSemanticState(
	state: FlowEntityGraphState,
	node: FlowNodeV1,
	kind: FlowGraphNodeKind,
	supernode: boolean,
) {
	const { context, visualization } = state;
	const data = state.renderData;
	const views = data.overlayViews;
	const nodeBalance = BigInt(node.supply);
	const potential = visualization.potentials.get(node.id);
	const traceState = visualization.nodeTraceStates.get(node.id);
	const ibfsNode = visualization.ibfsView?.nodes.get(node.id);
	const eibfsNode = visualization.eibfsView?.nodes.get(node.id);
	const eibfsStage = visualization.eibfsStage;
	const features = visualization.features;
	const binaryNode = data.binaryNodeById.get(node.id);
	const cancelTightenNode = data.cancelTightenNodeById.get(node.id);
	const relaxedMndcNode = data.relaxedMndcNodeById.get(node.id);
	const enhancedScalingNode = data.enhancedScalingNodeById.get(node.id);
	const enhancedScalingComponent =
		enhancedScalingNode === undefined
			? undefined
			: data.enhancedScalingComponentById.get(enhancedScalingNode.component_id);
	const enhancedScalingRole =
		enhancedScalingNode === undefined
			? undefined
			: views.enhancedCapacityScaling?.source_component ===
					enhancedScalingNode.component_id
				? "source"
				: views.enhancedCapacityScaling?.sink_component ===
						enhancedScalingNode.component_id
					? "sink"
					: undefined;
	const orlinMcfNode = data.orlinMcfOriginalNodeById.get(node.id);
	const orlinMcfComponent =
		orlinMcfNode === undefined
			? undefined
			: data.orlinMcfComponentById.get(orlinMcfNode.component_id);
	const orlinMcfRole =
		orlinMcfNode === undefined
			? undefined
			: views.orlinMcf?.source_component === orlinMcfNode.component_id
				? "source"
				: views.orlinMcf?.sink_component === orlinMcfNode.component_id
					? "sink"
					: undefined;
	const orlinMaxNode = data.orlinMaxNodeById.get(node.id);
	const electricalNode = data.electricalNodeById.get(node.id);
	const electricalNodePotentialBand =
		electricalNode === undefined
			? undefined
			: data.electricalPotentialBand(Number(electricalNode.potential));
	const augmentingElectricalNode = data.augmentingElectricalNodeById.get(
		node.id,
	);
	const augmentingElectricalPotentialBand =
		augmentingElectricalNode === undefined
			? undefined
			: data.augmentingPotentialBand(
					Number(augmentingElectricalNode.potential),
				);
	const interiorPointNode = data.interiorPointNodeById.get(node.id);
	const interiorPointNodePotentialBand =
		interiorPointNode === undefined
			? undefined
			: data.interiorPointPotentialBand(Number(interiorPointNode.potential));
	const electricalIpmMcfNode = data.electricalIpmMcfNodeById.get(node.id);
	const electricalIpmMcfNodePotentialBand =
		electricalIpmMcfNode === undefined
			? undefined
			: data.electricalIpmMcfPotentialBand(
					Number(electricalIpmMcfNode.potential),
				);
	const minimumRatioNode = data.minimumRatioNodeById.get(node.id);
	const randomizedAlmostLinearNode = data.randomizedAlmostLinearNodeById.get(
		node.id,
	);
	const deterministicAlmostLinearNode =
		data.deterministicAlmostLinearNodeById.get(node.id);
	const dualSimplexNode = data.dualSimplexNodeById.get(node.id);
	const polynomialDualNode = data.polynomialDualNodeById.get(node.id);
	const polynomialPrimalNode = data.polynomialPrimalNodeById.get(node.id);
	const convexSimplexNode = data.convexSimplexNodeById.get(node.id);
	const predictionNode = data.predictionNodeById.get(node.id);
	const tardosNode = data.tardosNodeById.get(node.id);
	const polynomialPrimalEligible =
		polynomialPrimalNode?.flags.includes("eligible") === true;
	const polynomialPrimalAwake =
		polynomialPrimalNode?.flags.includes("awake") === true;
	const polynomialPrimalNStar =
		polynomialPrimalNode?.flags.includes("in-n-star") === true;
	const polynomialPrimalRoot =
		polynomialPrimalNode?.flags.includes("root") === true;
	const doubleScalingNode = data.doubleScalingOriginalNodeById.get(node.id);
	const doubleScalingRole =
		views.doubleScaling?.selected_deficit === `node:${node.id}`
			? "deficit"
			: views.doubleScaling?.selected_root === `node:${node.id}`
				? "root"
				: undefined;
	const eibfsRepairFocus =
		visualization.eibfsView?.repairFocusNodeIds.has(node.id) === true;
	const potentialDijkstra = isPotentialDijkstraSspAlgorithm(
		context.algorithmId,
	);
	const priceCoordinateRelaxation = isPriceCoordinateRelaxationAlgorithm(
		context.algorithmId,
	);
	const priceAlgorithm =
		potentialDijkstra ||
		features.costScaling ||
		features.outOfKilter ||
		priceCoordinateRelaxation ||
		features.hungarian ||
		features.auction ||
		features.convexCost;
	const certificatePriceAlgorithm =
		potentialDijkstra ||
		features.costScaling ||
		features.hungarian ||
		features.convexCost;
	const minimumMeanCycleCanceling = isMinimumMeanCycleCancelingAlgorithm(
		context.algorithmId,
	);
	const capacityScalingLabelPrefix = capacityScalingTraceLabelPrefix(
		context.traceEvent?.catalog_id,
	);
	const certifiedPotentialReplacesTraceLabel =
		potential !== undefined &&
		((certificatePriceAlgorithm &&
			(isPotentialDijkstraPriceEvent(context.traceEvent?.catalog_id) ||
				isCostScalingPriceEvent(context.traceEvent?.catalog_id) ||
				(features.hungarian &&
					context.traceEvent?.catalog_id === "hungarian.optimal") ||
				(features.convexCost &&
					[
						"segment-expanded-convex-mcf.optimal",
						"convex-cost-scaling.certify-expanded-oracle",
						"convex-network-simplex.certify-expanded-oracle",
					].includes(context.traceEvent?.catalog_id ?? "")))) ||
			(features.networkSimplex &&
				isNetworkSimplexOptimalEvent(context.traceEvent?.catalog_id)));
	const solverPriceReplacesCertifiedPotential =
		(features.outOfKilter ||
			priceCoordinateRelaxation ||
			features.transportation) &&
		traceState?.label !== undefined;
	const labelPrefix =
		capacityScalingLabelPrefix ??
		(features.networkSimplex
			? "π"
			: features.transportation && context.model.kind === "transportation"
				? context.model.destinations.includes(node.id)
					? "v"
					: "u"
				: features.hungarian
					? context.model.kind === "assignment" &&
						context.model.tasks.includes(node.id)
						? "v"
						: "u"
					: features.auction
						? context.model.kind === "assignment" &&
							context.model.tasks.includes(node.id)
							? "pₛ"
							: "βₛ"
						: features.epsilonRelaxation
							? "p̂"
							: features.relaxation
								? "π"
								: priceAlgorithm
									? potentialDijkstra
										? isBlockingPrimalDualLevelEvent(
												context.traceEvent?.catalog_id,
											)
											? "ℓ"
											: isPotentialDijkstraPriceEvent(
														context.traceEvent?.catalog_id,
													)
												? "π"
												: "d̄"
										: "π"
									: minimumMeanCycleCanceling
										? "Dₙ"
										: features.pushRelabel
											? "h"
											: features.pseudoflow
												? "ℓ"
												: isMpmPotentialEvent(context.traceEvent?.catalog_id)
													? "p"
													: "d");
	const convexSimplexParentLabel =
		convexSimplexNode?.parent === "artificial-root"
			? "R*"
			: convexSimplexNode?.parent;
	const traceLabel = [
		deterministicAlmostLinearNode !== undefined
			? `C${deterministicAlmostLinearNode.forest_component} · ↑ ${deterministicAlmostLinearNode.tree_parent_node_id ?? "root"} · ${deterministicAlmostLinearNode.source_side ? "S*" : "T*"}${deterministicAlmostLinearNode.artificial_tree_level_mask === "0" ? "" : ` · L ${deterministicAlmostLinearNode.artificial_tree_level_mask}`}`
			: electricalIpmMcfNode !== undefined
				? `y ${compactFlowCanvasNumber(electricalIpmMcfNode.potential)} · Δy ${compactFlowCanvasNumber(electricalIpmMcfNode.potential_direction)}${electricalIpmMcfNode.anchored ? " · GND" : ""}`
				: minimumRatioNode !== undefined
					? `C${minimumRatioNode.component} · d ${minimumRatioNode.depth} · b ${minimumRatioNode.candidate_balance}${minimumRatioNode.parent_node_id === undefined ? " · root" : ` · ↑ ${minimumRatioNode.parent_node_id}`}`
					: interiorPointNode !== undefined
						? `y ${compactFlowCanvasNumber(interiorPointNode.potential)} · ${interiorPointNode.target_source_side ? "S*" : "T*"}`
						: augmentingElectricalNode !== undefined
							? `y ${compactFlowCanvasNumber(augmentingElectricalNode.potential)} · γ ${compactFlowCanvasNumber(augmentingElectricalNode.coupling_violation)}${augmentingElectricalNode.target_source_side ? " · S*" : " · T*"}`
							: electricalNode !== undefined
								? `φ ${compactFlowCanvasNumber(electricalNode.potential)}${electricalNode.grounded ? " · GND" : Number(electricalNode.residual) === 0 ? "" : ` · r ${compactFlowCanvasNumber(electricalNode.residual)}`}`
								: tardosNode !== undefined
									? `π ${tardosNode.potential}`
									: predictionNode !== undefined
										? `p̂ ${predictionNode.prediction_clipped ? "clip→" : ""}${predictionNode.predicted_price} · pₜ ${predictionNode.price}`
										: convexSimplexNode !== undefined
											? `π ${convexSimplexNode.potential}${convexSimplexParentLabel === undefined ? " · R*" : ` · ↑ ${convexSimplexParentLabel}`}`
											: polynomialDualNode !== undefined
												? `π ${polynomialDualNode.potential} · ẽ ${formatFlowRational(polynomialDualNode.excess)}${polynomialDualNode.root ? " · R" : ""}${polynomialDualNode.active ? " · A" : ""}${polynomialDualNode.bad ? " · BAD" : ""}${polynomialDualNode.in_pivot_cut ? " · H" : ""}`
												: polynomialPrimalNode !== undefined
													? `q ${formatFlowRational(polynomialPrimalNode.premultiplier)}${polynomialPrimalEligible ? " · E" : ""}${polynomialPrimalAwake ? " · A" : ""}${polynomialPrimalNStar ? " · N*" : ""}${polynomialPrimalRoot ? " · R" : ""}`
													: orlinMaxNode !== undefined
														? `C ${orlinMaxNode.component_id} · Φ ${orlinMaxNode.anti_potential} · ${orlinMaxNode.critical ? "K" : "C"} · ${orlinMaxNode.source_side ? "S" : "T"}`
														: orlinMcfNode !== undefined &&
																orlinMcfComponent !== undefined
															? `C ${orlinMcfNode.component_id} · e ${formatFlowRational(orlinMcfComponent.excess)} · ${orlinMcfPotentialLabel(views.orlinMcf?.stage, orlinMcfNode.potential)}${orlinMcfNode.distance === undefined ? "" : ` · d ${orlinMcfNode.distance}`}`
															: enhancedScalingNode !== undefined &&
																	enhancedScalingComponent !== undefined
																? `C ${enhancedScalingNode.component_id} · e ${formatFlowRational(enhancedScalingComponent.excess)} · π ${enhancedScalingNode.potential}${enhancedScalingNode.distance === undefined ? "" : ` · d ${enhancedScalingNode.distance}`}`
																: dualSimplexNode !== undefined
																	? dualSimplexNode.initialized
																		? `π ${dualSimplexNode.potential}${dualSimplexNode.in_cut ? " · H" : ""}`
																		: "π …"
																	: doubleScalingNode !== undefined
																		? `π̂ ${doubleScalingNode.price} · e ${doubleScalingNode.imbalance} · a ${doubleScalingNode.cursor}`
																		: relaxedMndcNode !== undefined &&
																				views.relaxedMndc?.assignment_value !==
																					undefined
																			? `σL ${relaxedMndcNode.left_dual} · σR ${relaxedMndcNode.right_dual} · ↦ ${relaxedMndcNode.matched_node_id}`
																			: cancelTightenNode !== undefined
																				? `π ${formatFlowRational(cancelTightenNode.potential)}${cancelTightenNode.rank === undefined ? "" : ` · ℓ ${cancelTightenNode.rank}`}`
																				: binaryNode !== undefined
																					? `d ${binaryNode.distance ?? "∞"} · C${binaryNode.component}`
																					: eibfsNode !== undefined
																						? eibfsNodeLabel(eibfsNode)
																						: eibfsStage !== undefined
																							? undefined
																							: traceState?.label ===
																										undefined ||
																									certifiedPotentialReplacesTraceLabel
																								? undefined
																								: ibfsNode === undefined
																									? `${labelPrefix} ${traceState.label}`
																									: ibfsDistanceLabel(ibfsNode),
		traceState?.search_ordinal === undefined
			? undefined
			: `#${traceState.search_ordinal}`,
		eibfsNode !== undefined ||
		traceState?.remaining_divergence === undefined ||
		traceState.remaining_divergence === "0"
			? undefined
			: `${eibfsStage !== undefined ? "e" : features.networkSimplex ? "bᵃ" : features.epsilonRelaxation || features.predictionAssisted ? "g" : features.relaxation ? "d" : features.pushRelabel || features.blockingPreflow || features.pseudoflow || features.costScaling || features.transportation ? "e" : features.capacityScaling || features.blockingPrimalDual ? "b′" : "Δ"} ${traceState.remaining_divergence}`,
	]
		.filter((value) => value !== undefined)
		.join(" · ");

	const title = [
		supernode
			? "super · GRIDGEN feasibility connector"
			: nodeBalance === 0n
				? `${node.id}${ibfsNode === undefined ? "" : ` · ${ibfsDistanceLabel(ibfsNode)}${ibfsNode.orphan ? " · orphan" : ibfsNode.frontier ? " · frontier" : ""}`}${eibfsNode === undefined ? "" : ` · ${eibfsNodeLabel(eibfsNode)} · root ${eibfsNode.root_kind}${eibfsNode.orphan ? " · orphan" : eibfsNode.frontier ? " · frontier" : ""}${eibfsRepairFocus ? " · repair focus" : ""}`}${binaryNode === undefined ? "" : ` · binary distance ${binaryNode.distance ?? "unreachable"} · component ${binaryNode.component}`}${cancelTightenNode === undefined ? "" : ` · exact price ${formatFlowRational(cancelTightenNode.potential)}${cancelTightenNode.rank === undefined ? "" : ` · topological rank ${cancelTightenNode.rank}`}`}${relaxedMndcNode === undefined ? "" : ` · split assignment ${relaxedMndcNode.node_id}→${relaxedMndcNode.matched_node_id} · dual ${relaxedMndcNode.left_dual}/${relaxedMndcNode.right_dual}`}${doubleScalingNode === undefined ? "" : ` · scaled price ${doubleScalingNode.price} · imbalance ${doubleScalingNode.imbalance} · current arc ${doubleScalingNode.cursor}${doubleScalingRole === undefined ? "" : ` · ${doubleScalingRole}`}`}`
				: `${node.id} · balance ${nodeBalance > 0n ? "+" : ""}${node.supply}`,
		enhancedScalingNode === undefined || enhancedScalingComponent === undefined
			? ""
			: ` · component ${enhancedScalingNode.component_id} · exact excess ${formatFlowRational(enhancedScalingComponent.excess)} · dual price ${enhancedScalingNode.potential}${enhancedScalingNode.distance === undefined ? "" : ` · distance ${enhancedScalingNode.distance}`}${enhancedScalingRole === undefined ? "" : ` · selected ${enhancedScalingRole}`}`,
		orlinMcfNode === undefined || orlinMcfComponent === undefined
			? ""
			: ` · transformed component ${orlinMcfNode.component_id} · exact excess ${formatFlowRational(orlinMcfComponent.excess)} · dual price ${orlinMcfNode.potential}${orlinMcfNode.distance === undefined ? "" : ` · capped distance ${orlinMcfNode.distance}`}${orlinMcfRole === undefined ? "" : ` · selected ${orlinMcfRole}`}`,
		dualSimplexNode === undefined
			? ""
			: dualSimplexNode.initialized
				? ` · dual price ${dualSimplexNode.potential}${dualSimplexNode.in_cut ? " · head-side cut H" : ""}`
				: " · dual price not reached yet",
		polynomialDualNode === undefined
			? ""
			: ` · dual price ${polynomialDualNode.potential} · exact auxiliary excess ${formatFlowRational(polynomialDualNode.excess)}${polynomialDualNode.root ? " · root" : ""}${polynomialDualNode.active ? " · active" : ""}${polynomialDualNode.bad ? " · below bad arc" : ""}${polynomialDualNode.in_pivot_cut ? " · pivot cut" : ""}`,
		polynomialPrimalNode === undefined
			? ""
			: ` · exact premultiplier ${formatFlowRational(polynomialPrimalNode.premultiplier)}${polynomialPrimalEligible ? " · eligible" : ""}${polynomialPrimalAwake ? " · awake" : ""}${polynomialPrimalNStar ? " · in N*" : ""}${polynomialPrimalRoot ? " · rooted tree root" : ""}`,
		convexSimplexNode === undefined
			? ""
			: ` · compact-basis potential ${convexSimplexNode.potential}${convexSimplexNode.parent === undefined ? " · artificial root" : ` · parent ${convexSimplexNode.parent}`}`,
		predictionNode === undefined
			? ""
			: ` · raw prediction ${predictionNode.raw_predicted_price} · Algorithm 1 prediction ${predictionNode.predicted_price}${predictionNode.prediction_clipped ? " · clipped" : ""} · current price ${predictionNode.price} · surplus ${predictionNode.surplus}${predictionNode.active ? " · active positive-surplus node" : ""}`,
		electricalNode === undefined
			? ""
			: ` · electrical potential ${electricalNode.potential} · residual ${electricalNode.residual} · search direction ${electricalNode.search_direction}${electricalNode.grounded ? " · grounded sink" : ""}`,
		augmentingElectricalNode === undefined
			? ""
			: ` · dual embedding ${augmentingElectricalNode.potential} · coupling violation ${augmentingElectricalNode.coupling_violation} · target cut ${augmentingElectricalNode.target_source_side ? "source" : "sink"} side`,
		interiorPointNode === undefined
			? ""
			: ` · interior-point potential ${interiorPointNode.potential} · exact target cut ${interiorPointNode.target_source_side ? "source" : "sink"} side`,
		electricalIpmMcfNode === undefined
			? ""
			: ` · MCF dual potential ${electricalIpmMcfNode.potential} · Newton direction ${electricalIpmMcfNode.potential_direction} · balance residual ${electricalIpmMcfNode.balance_residual}${electricalIpmMcfNode.anchored ? " · gauge anchor" : ""}`,
		minimumRatioNode === undefined
			? ""
			: ` · minimum-ratio component ${minimumRatioNode.component} · parent ${minimumRatioNode.parent_node_id ?? "root"} · depth ${minimumRatioNode.depth} · candidate balance ${minimumRatioNode.candidate_balance}${minimumRatioNode.on_candidate ? " · candidate cycle" : ""}${minimumRatioNode.on_selected ? " · selected cycle" : ""}`,
		randomizedAlmostLinearNode === undefined
			? ""
			: ` · sampled-tree component ${randomizedAlmostLinearNode.tree_component} · parent ${randomizedAlmostLinearNode.tree_parent_node_id ?? "root"} · target cut ${randomizedAlmostLinearNode.source_side ? "source" : "sink"} side${randomizedAlmostLinearNode.artificial_direction === "0" ? "" : ` · artificial ${randomizedAlmostLinearNode.artificial_direction} ${randomizedAlmostLinearNode.artificial_flow}/${randomizedAlmostLinearNode.artificial_capacity}`}${randomizedAlmostLinearNode.active_artificial_sign === "0" ? "" : ` · active artificial sign ${randomizedAlmostLinearNode.active_artificial_sign}`}`,
		deterministicAlmostLinearNode === undefined
			? ""
			: ` · deterministic forest component ${deterministicAlmostLinearNode.forest_component} · tree parent ${deterministicAlmostLinearNode.tree_parent_node_id ?? "root"} · target cut ${deterministicAlmostLinearNode.source_side ? "source" : "sink"} side${deterministicAlmostLinearNode.artificial_direction === "0" ? "" : ` · artificial ${deterministicAlmostLinearNode.artificial_direction} ${deterministicAlmostLinearNode.artificial_flow}/${deterministicAlmostLinearNode.artificial_capacity} · tree levels ${deterministicAlmostLinearNode.artificial_tree_level_mask}`}${deterministicAlmostLinearNode.active_artificial_sign === "0" ? "" : ` · active artificial sign ${deterministicAlmostLinearNode.active_artificial_sign}`}`,
	].join("");

	const nodeClassName = `flow-node flow-node-${kind}${supernode ? " flow-node-super" : ""}${electricalNode === undefined ? "" : ` flow-node-electric-fill-${electricalNodePotentialBand}${electricalNode.grounded ? " flow-node-electric-ground" : ""}`}${augmentingElectricalNode === undefined ? "" : ` flow-node-augmenting-fill-${augmentingElectricalPotentialBand}`}${interiorPointNode === undefined ? "" : ` flow-node-interior-fill-${interiorPointNodePotentialBand}`}${electricalIpmMcfNode === undefined ? "" : ` flow-node-eipm-mcf-fill-${electricalIpmMcfNodePotentialBand}`}${minimumRatioNode === undefined ? "" : ` flow-node-minimum-ratio-fill-${Number(BigInt(minimumRatioNode.component) % 5n)}`}${randomizedAlmostLinearNode === undefined ? "" : ` flow-node-randomized-fill-${Number(BigInt(randomizedAlmostLinearNode.tree_component) % 5n)}`}${deterministicAlmostLinearNode === undefined ? "" : ` flow-node-deterministic-fill-${Number(BigInt(deterministicAlmostLinearNode.forest_component) % 5n)}`}`;

	return {
		eibfsNode,
		potential,
		solverPriceReplacesCertifiedPotential,
		traceLabel,
		title,
		nodeClassName,
	};
}
