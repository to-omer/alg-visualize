import {
	exactRational,
	rationalCapacityBand,
	rationalMagnitudeStrokeWidth,
} from "./flow-graph-rational-scales";
import type { FlowLayout } from "./flow-layout";
import { formatFlowRational } from "./flow-parametric-view";
import type { FlowEntityRenderPlan } from "./flow-render-plan";
import {
	absoluteBigInt,
	capacityRailWidth,
	costMagnitudeBand,
	costMagnitudeIntensity,
	flowFillStrokeWidth,
	rationalCapacityRailWidth,
} from "./flow-visual-scales";

/**
 * Projects the algorithm-specific state attached to each original edge.
 *
 * This is deliberately a feature projection boundary: SVG components consume
 * this result and never inspect generated overlay payloads directly.
 */
export function projectFlowOriginalEdgeFeatures(
	plan: FlowEntityRenderPlan,
	layout: FlowLayout,
	maxCapacity: bigint,
) {
	const { context, visualization } = plan;
	const renderData = plan.overlayPresentation.renderData;
	const overlayViews = renderData.overlayViews;
	const parametricCapacityByEdge = new Map(
		overlayViews.parametric?.edge_capacities.map((capacity) => [
			capacity.edge_id,
			capacity.capacity,
		]) ?? [],
	);
	const parametricMaximumCapacity =
		overlayViews.parametric?.visual_scale_max_capacity;
	const maxAbsoluteCost = plan.edges.reduce((maximum, edge) => {
		const magnitude = absoluteBigInt(BigInt(edge.cost));
		return magnitude > maximum ? magnitude : maximum;
	}, 0n);
	const transportation = visualization.features.transportation;

	return plan.edges.flatMap((edge) => {
		const geometry = layout.routes.get(edge.id);
		if (geometry === undefined) return [];
		const parametricCapacity = parametricCapacityByEdge.get(edge.id);
		const capacity = BigInt(edge.capacity);
		const flow = BigInt(
			visualization.edgeStates.get(edge.id)?.flow ?? edge.lower,
		);
		const enhancedScalingState = renderData.enhancedScalingEdgeById.get(
			edge.id,
		);
		const orlinMcfState = renderData.orlinMcfBranchesByEdge.get(edge.id);
		const dualSimplexState = renderData.dualSimplexEdgeById.get(edge.id);
		const polynomialDualState = renderData.polynomialDualEdgeById.get(edge.id);
		const polynomialPrimalState = renderData.polynomialPrimalEdgeById.get(
			edge.id,
		);
		const convexSimplexState = renderData.convexSimplexEdgeById.get(edge.id);
		const predictionState = renderData.predictionEdgeById.get(edge.id);
		const electricalState = renderData.electricalEdgeById.get(edge.id);
		const augmentingElectricalState =
			renderData.augmentingElectricalEdgeById.get(edge.id);
		const interiorPointState = renderData.interiorPointEdgeById.get(edge.id);
		const electricalIpmMcfState = renderData.electricalIpmMcfEdgeById.get(
			edge.id,
		);
		const minimumRatioState = renderData.minimumRatioEdgeById.get(edge.id);
		const randomizedAlmostLinearState =
			renderData.randomizedAlmostLinearEdgeById.get(edge.id);
		const deterministicAlmostLinearState =
			renderData.deterministicAlmostLinearEdgeById.get(edge.id);
		const railWidth =
			parametricCapacity === undefined ||
			parametricMaximumCapacity === undefined
				? capacityRailWidth(capacity, maxCapacity)
				: rationalCapacityRailWidth(
						exactRational(parametricCapacity),
						exactRational(parametricMaximumCapacity),
					);
		const rawFlowWidth =
			orlinMcfState?.flow !== undefined
				? rationalMagnitudeStrokeWidth(
						orlinMcfState.flow.flow,
						renderData.maximumOrlinMcfBranchFlow,
					)
				: polynomialPrimalState !== undefined
					? capacity === 0n
						? 2
						: 2 +
							Number(
								(absoluteBigInt(BigInt(polynomialPrimalState.perturbed_flow)) *
									4_000n) /
									(capacity *
										BigInt(
											overlayViews.polynomialPrimalSimplex
												?.perturbation_scale ?? "1",
										)),
							) /
								1_000
					: dualSimplexState !== undefined
						? capacity === 0n
							? 2
							: 2 +
								Number(
									(absoluteBigInt(BigInt(dualSimplexState.basic_flow)) *
										4_000n) /
										capacity,
								) /
									1_000
						: enhancedScalingState !== undefined
							? 2 +
								Number(
									(BigInt(enhancedScalingState.virtual_flow.numerator) *
										4_000n) /
										renderData.maximumEnhancedVirtualFlow,
								) /
									1_000
							: parametricCapacity !== undefined
								? Math.max(2, railWidth - 2)
								: flowFillStrokeWidth(flow, capacity, railWidth);
		const flowWidth = Math.max(0, Math.min(rawFlowWidth, railWidth - 1.5));
		const augmentingCentralWidth =
			augmentingElectricalState === undefined
				? undefined
				: 2 +
					Math.min(
						6,
						(Math.abs(Number(augmentingElectricalState.central_flow)) /
							Math.max(1, Number(capacity))) *
							6,
					);
		const interiorPointFractionalWidth =
			interiorPointState === undefined
				? undefined
				: 2 +
					Math.min(
						6,
						(Math.abs(Number(interiorPointState.fractional_flow)) /
							Math.max(1, Number(capacity))) *
							6,
					);
		const electricalIpmMcfFractionalWidth = (() => {
			if (electricalIpmMcfState === undefined) return undefined;
			const lower = Number(electricalIpmMcfState.face_lower);
			const upper = Number(electricalIpmMcfState.face_upper);
			const fractional = Number(electricalIpmMcfState.fractional_flow);
			const span = upper - lower;
			const normalized =
				span <= Number.EPSILON
					? 0.45
					: Math.max(0, Math.min(1, (fractional - lower) / span));
			return 2 + normalized * 6;
		})();
		const convexState = renderData.convexCostEdgeById.get(edge.id);
		const displayedMarginal =
			convexState?.forward_marginal_cost ??
			convexState?.reverse_marginal_cost ??
			edge.cost;
		const cost = BigInt(displayedMarginal);
		const costKind = cost < 0n ? "negative" : cost > 0n ? "positive" : "zero";
		const magnitudeBand = costMagnitudeBand(cost, maxAbsoluteCost);
		const costIntensity = costMagnitudeIntensity(cost, maxAbsoluteCost);
		const ibfsForestArc = visualization.ibfsForestByEdge.get(edge.id);
		const eibfsForestArc = visualization.eibfsForestByEdge.get(edge.id);

		return [
			{
				edge,
				geometry,
				flow,
				capacity,
				capacityLabel:
					parametricCapacity === undefined
						? edge.capacity
						: formatFlowRational(parametricCapacity),
				parametricCapacityBand:
					parametricCapacity === undefined ||
					parametricMaximumCapacity === undefined
						? undefined
						: rationalCapacityBand(
								parametricCapacity,
								parametricMaximumCapacity,
							),
				railWidth,
				flowWidth,
				costKind,
				magnitudeBand,
				costIntensity,
				signedCost: cost > 0n ? `+${edge.cost}` : edge.cost,
				crossesCut:
					context.outcome?.kind === "infeasible"
						? visualization.sourceSide.has(edge.from) !==
							visualization.sourceSide.has(edge.to)
						: visualization.sourceSide.has(edge.from) &&
							!visualization.sourceSide.has(edge.to),
				active:
					visualization.activeOriginalEdges.has(edge.id) ||
					polynomialDualState?.in_augment_path === true ||
					polynomialDualState?.bad === true ||
					overlayViews.polynomialDualSimplex?.leaving_edge === edge.id ||
					overlayViews.polynomialDualSimplex?.entering_edge === edge.id ||
					polynomialPrimalState?.entering === true ||
					polynomialPrimalState?.leaving === true ||
					polynomialPrimalState?.in_cycle === true ||
					convexSimplexState?.entering === true ||
					convexSimplexState?.leaving === true ||
					convexSimplexState?.in_cycle === true ||
					overlayViews.dualNetworkSimplex?.leaving_edge === edge.id ||
					overlayViews.dualNetworkSimplex?.entering_edge === edge.id ||
					renderData.enhancedScalingPathArcKeys.has(`${edge.id}:forward`) ||
					renderData.enhancedScalingPathArcKeys.has(`${edge.id}:reverse`) ||
					overlayViews.enhancedCapacityScaling?.contraction_arc === edge.id ||
					renderData.orlinMcfActiveEdges.has(edge.id) ||
					overlayViews.orlinMcf?.contraction_arc?.edge_id === edge.id ||
					renderData.doubleScalingActiveEdges.has(edge.id) ||
					renderData.convexActiveDirectionsByEdge.has(edge.id) ||
					overlayViews.predictionAssistedEpsilon?.active_arc?.edge_id ===
						edge.id,
				convexState,
				convexActiveDirections: renderData.convexActiveDirectionsByEdge.get(
					edge.id,
				),
				convexEligibleDirections: renderData.convexEligibleDirectionsByEdge.get(
					edge.id,
				),
				cancelTightenCycle: renderData.cancelTightenCycleEdges.has(edge.id),
				relaxedMndcCycle: renderData.relaxedMndcCycleByEdge.get(edge.id),
				enhancedScalingState,
				orlinMcfState,
				dualSimplexState,
				polynomialDualState,
				polynomialPrimalState,
				convexSimplexState,
				predictionState,
				electricalState,
				electricalEnergyBand:
					electricalState === undefined
						? undefined
						: renderData.electricalEnergyBand(Number(electricalState.energy)),
				augmentingElectricalState,
				augmentingCentralWidth,
				augmentingCongestionBand:
					augmentingElectricalState === undefined
						? undefined
						: renderData.augmentingCongestionBand(
								Number(augmentingElectricalState.congestion),
							),
				interiorPointState,
				electricalIpmMcfState,
				electricalIpmMcfFractionalWidth,
				electricalIpmMcfSlackBand:
					electricalIpmMcfState === undefined
						? undefined
						: renderData.interiorPointMagnitudeBand(
								Math.abs(Number(electricalIpmMcfState.lower_slack)),
								renderData.maximumElectricalIpmMcfSlack,
							),
				electricalIpmMcfResistanceBand:
					electricalIpmMcfState === undefined
						? undefined
						: renderData.interiorPointMagnitudeBand(
								Math.abs(Number(electricalIpmMcfState.resistance)),
								renderData.maximumElectricalIpmMcfResistance,
							),
				electricalIpmMcfCurrentWidth:
					electricalIpmMcfState === undefined ||
					renderData.maximumElectricalIpmMcfCurrent <= Number.EPSILON
						? 0
						: 3 +
							(Math.abs(Number(electricalIpmMcfState.electrical_current)) /
								renderData.maximumElectricalIpmMcfCurrent) *
								8,
				minimumRatioState,
				minimumRatioLengthWidth:
					minimumRatioState === undefined
						? undefined
						: 4 +
							Number(
								(BigInt(minimumRatioState.length) * 7_000n) /
									renderData.maximumMinimumRatioLength,
							) /
								1_000,
				minimumRatioGradientBand:
					minimumRatioState === undefined
						? undefined
						: costMagnitudeBand(
								BigInt(minimumRatioState.gradient),
								renderData.maximumMinimumRatioGradient,
							),
				randomizedAlmostLinearState,
				deterministicAlmostLinearState,
				randomizedLengthWidth:
					randomizedAlmostLinearState === undefined
						? undefined
						: 4 +
							(Math.abs(Number(randomizedAlmostLinearState.length)) /
								renderData.maximumRandomizedLength) *
								7,
				randomizedGradientBand:
					randomizedAlmostLinearState === undefined
						? undefined
						: renderData.randomizedMagnitudeBand(
								Number(randomizedAlmostLinearState.gradient),
								renderData.maximumRandomizedGradient,
							),
				deterministicLengthWidth:
					deterministicAlmostLinearState === undefined
						? undefined
						: 4 +
							(Math.abs(Number(deterministicAlmostLinearState.length)) /
								renderData.maximumDeterministicLength) *
								7,
				deterministicGradientBand:
					deterministicAlmostLinearState === undefined
						? undefined
						: renderData.randomizedMagnitudeBand(
								Number(deterministicAlmostLinearState.gradient),
								renderData.maximumDeterministicGradient,
							),
				interiorPointFractionalWidth,
				interiorPointCongestionBand:
					interiorPointState === undefined
						? undefined
						: renderData.interiorPointMagnitudeBand(
								Number(interiorPointState.congestion),
								renderData.maximumInteriorPointCongestion,
							),
				interiorPointSlackBand:
					interiorPointState === undefined
						? undefined
						: renderData.interiorPointMagnitudeBand(
								Number(interiorPointState.slack),
								renderData.maximumInteriorPointSlack,
							),
				interiorPointResistanceBand:
					interiorPointState === undefined
						? undefined
						: renderData.interiorPointMagnitudeBand(
								Number(interiorPointState.resistance),
								renderData.maximumInteriorPointResistance,
							),
				predictionActiveDirection:
					overlayViews.predictionAssistedEpsilon?.active_arc?.edge_id ===
					edge.id
						? overlayViews.predictionAssistedEpsilon.active_arc.direction
						: undefined,
				doubleScalingBranches: renderData.doubleScalingActiveBranches.get(
					edge.id,
				),
				doubleScalingInspectedArc:
					renderData.doubleScalingInspectedArc?.edge_id === edge.id
						? renderData.doubleScalingInspectedArc
						: undefined,
				doubleScalingState: renderData.doubleScalingEdgeById.get(edge.id),
				cycleAdjustment:
					transportation &&
					visualization.activeForwardOriginalEdges.has(edge.id)
						? "add"
						: transportation &&
								visualization.activeReverseOriginalEdges.has(edge.id)
							? "subtract"
							: undefined,
				fixed: visualization.fixedOriginalEdges.has(edge.id),
				matched: visualization.matchedOriginalEdges.has(edge.id),
				ibfsTreeSide: ibfsForestArc?.side,
				ibfsTreeDirection: ibfsForestArc?.direction,
				eibfsTreeSide: eibfsForestArc?.side,
				eibfsTreeDirection: eibfsForestArc?.direction,
				eibfsTreeParent: eibfsForestArc?.parent,
				eibfsTreeChild: eibfsForestArc?.child,
			},
		];
	});
}
