import type { CSSProperties, ReactNode } from "react";
import { FlowGraphDiscreteEdgeOverlayBundle } from "./FlowGraphDiscreteEdgeOverlayBundle";
import { FlowGraphDiscreteEdgeUnderlayBundle } from "./FlowGraphDiscreteEdgeUnderlayBundle";
import { FlowGraphElectricalEdgeFeatureBundle } from "./FlowGraphElectricalEdgeFeatureBundle";
import { FlowGraphOverlayOwnedLeaves } from "./FlowGraphOverlayOwnedLeaves";
import { FlowGraphTreeChainEdgeFeatureBundle } from "./FlowGraphTreeChainEdgeFeatureBundle";
import { flowScopedSvgUrl, useFlowGraphIdScope } from "./flow-dom-id";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import type { FlowEntitySelection } from "./flow-entity-navigator";
import {
	ordinaryFlowEventEntityRefs,
	shouldRenderFlowEventEntityEmphasis,
} from "./flow-event-highlight";
import { isOriginalEdgeSelected } from "./flow-graph-entity-selection";
import { buildActiveFlowOverlayFeatureBundles } from "./flow-overlay-contribution-registry";
import { formatFlowRational } from "./flow-parametric-view";
import { FLOW_LOD_LIMITS } from "./flow-render-plan";
import { costMagnitudeBand } from "./flow-visual-scales";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
type FlowGraphLayerProps = Readonly<{
	state: FlowEntityGraphState;
	selection: FlowEntitySelection | undefined;
	hoveredEdgeId: string | undefined;
}>;

export function isTransportationOptimalityCertificateRoute(
	algorithmId: string,
	catalogId: string | undefined,
	isBasisRoute: boolean,
): boolean {
	if (isBasisRoute) return false;
	return (
		(algorithmId === "transportation-simplex" &&
			catalogId === "transportation-simplex.optimal") ||
		(algorithmId === "modi" && catalogId === "modi.optimal")
	);
}

function ParametricCapacityLeaf({
	state,
	visual,
	children,
}: Readonly<{
	state: FlowEntityGraphState;
	visual: FlowEntityGraphState["originalVisuals"][number];
	children: ReactNode;
}>) {
	if (visual.parametricCapacityBand === undefined) return children;
	return (
		<FlowGraphOverlayOwnedLeaves
			state={state}
			bundle="original-edge-discrete-overlay"
			entity={{ kind: "edge", id: visual.edge.id }}
			owners={[
				{
					overlay: "parametric_overlay",
					role: "edge_capacities.at-current-parameter",
				},
			]}
		>
			{children}
		</FlowGraphOverlayOwnedLeaves>
	);
}

function ConvexMarginalCostLeaf({
	state,
	visual,
	children,
}: Readonly<{
	state: FlowEntityGraphState;
	visual: FlowEntityGraphState["originalVisuals"][number];
	children: ReactNode;
}>) {
	if (visual.convexState === undefined) return children;
	return (
		<FlowGraphOverlayOwnedLeaves
			state={state}
			bundle="original-edge-discrete-overlay"
			entity={{ kind: "edge", id: visual.edge.id }}
			owners={[
				{
					overlay: "convex_cost_overlay",
					role: "edges.current-marginal-cost",
				},
			]}
		>
			{children}
		</FlowGraphOverlayOwnedLeaves>
	);
}

export function FlowGraphOriginalLayer({
	state,
	selection,
	hoveredEdgeId,
}: FlowGraphLayerProps) {
	const idScope = useFlowGraphIdScope();
	const plan = state.plan;
	const viewMode = state.viewMode;
	const context = state.context;
	const originalVisuals = state.originalVisuals;
	const basisOriginalEdges = state.visualization.basisOriginalEdges;
	const predictedOriginalEdges = state.visualization.predictedOriginalEdges;
	const overlayViews = state.renderData.overlayViews;
	const parametricIntersectionSourceSide =
		overlayViews.parametric?.traversal?.kind === "solve-intersection"
			? new Set(overlayViews.parametric.traversal.lower_source_side)
			: undefined;
	const maximumConvexMarginalMagnitude =
		state.renderData.maximumConvexMarginalMagnitude;
	const tardosFixedByEdge = state.renderData.tardosFixedByEdge;
	const ordinaryEventEntityRefs = ordinaryFlowEventEntityRefs(context);
	const touchedEdgeIds = new Set(
		ordinaryEventEntityRefs.flatMap((entity) =>
			entity.kind === "edge" || entity.kind === "residual-arc"
				? [entity.edge_id]
				: [],
		),
	);
	const changedEdgeIds = new Set(
		context.traceEventSemantics?.changed_entity_refs.flatMap((entity) =>
			entity.kind === "edge" || entity.kind === "residual-arc"
				? [entity.edge_id]
				: [],
		) ?? [],
	);
	const touchedIdentitiesByEdge = new Map<string, string[]>();
	for (const entity of ordinaryEventEntityRefs) {
		if (entity.kind === "node") continue;
		const identities = touchedIdentitiesByEdge.get(entity.edge_id) ?? [];
		identities.push(
			entity.kind === "edge"
				? `edge:${entity.edge_id}`
				: `residual-arc:${entity.edge_id}:${entity.direction}`,
		);
		touchedIdentitiesByEdge.set(entity.edge_id, identities);
	}
	const changedIdentitiesByEdge = new Map<string, string[]>();
	for (const entity of context.traceEventSemantics?.changed_entity_refs ?? []) {
		if (entity.kind === "node") continue;
		const identities = changedIdentitiesByEdge.get(entity.edge_id) ?? [];
		identities.push(
			entity.kind === "edge"
				? `edge:${entity.edge_id}`
				: `residual-arc:${entity.edge_id}:${entity.direction}`,
		);
		changedIdentitiesByEdge.set(entity.edge_id, identities);
	}
	const activeBundles = buildActiveFlowOverlayFeatureBundles(
		plan.overlayPresentation.activeFields,
	);
	const showsCost = !new Set([
		"max-flow",
		"parametric-max-flow",
		"planar-max-flow",
		"bipartite-matching",
	]).has(context.model.kind);
	const emphasizeTouchedEdges = shouldRenderFlowEventEntityEmphasis({
		level: plan.level,
		kind: "edge",
		signal: "touch",
		memberCount: touchedEdgeIds.size,
		totalCount: originalVisuals.length,
		structureLimit: FLOW_LOD_LIMITS.structureEdgeEventLabels,
	});
	const emphasizeChangedEdges = shouldRenderFlowEventEntityEmphasis({
		level: plan.level,
		kind: "edge",
		signal: "change",
		memberCount: changedEdgeIds.size,
		totalCount: originalVisuals.length,
		structureLimit: FLOW_LOD_LIMITS.structureEdgeEventLabels,
	});
	return (
		<>
			{viewMode !== "residual" &&
				originalVisuals.map((visual) => {
					const parametricIntersectionCut =
						parametricIntersectionSourceSide !== undefined &&
						parametricIntersectionSourceSide.has(visual.edge.from) !==
							parametricIntersectionSourceSide.has(visual.edge.to);
					const selected = isOriginalEdgeSelected(selection, visual.edge.id);
					const rawTouched = touchedEdgeIds.has(visual.edge.id);
					const rawChanged = changedEdgeIds.has(visual.edge.id);
					const touched = emphasizeTouchedEdges && rawTouched;
					const changed = emphasizeChangedEdges && rawChanged;
					const expanded =
						plan.level === "detail" ||
						visual.active ||
						touched ||
						selected ||
						hoveredEdgeId === visual.edge.id ||
						visual.flow > 0n;
					return (
						<g
							key={visual.edge.id}
							data-edge-id={visual.edge.id}
							data-parallel-index={visual.geometry.parallelIndex}
							data-parallel-count={visual.geometry.parallelCount}
							data-event-touch={rawTouched || undefined}
							data-event-change={rawChanged || undefined}
							data-event-identities={touchedIdentitiesByEdge
								.get(visual.edge.id)
								?.join("|")}
							data-changed-identities={changedIdentitiesByEdge
								.get(visual.edge.id)
								?.join("|")}
							data-edge-detail={expanded ? "expanded" : "context"}
							data-overlay-marks={plan.overlayPresentation.edgeMarksById
								.get(visual.edge.id)
								?.map(({ overlay, role }) => `${overlay}:${role}`)
								.join("|")}
							data-convex-flow={visual.convexState?.flow}
							data-convex-total-cost={visual.convexState?.total_cost}
							data-convex-forward-marginal={
								visual.convexState?.forward_marginal_cost
							}
							data-convex-reverse-marginal={
								visual.convexState?.reverse_marginal_cost
							}
							data-convex-active-directions={
								visual.convexActiveDirections === undefined
									? undefined
									: [...visual.convexActiveDirections].sort().join(",")
							}
							data-convex-eligible-directions={
								visual.convexEligibleDirections === undefined
									? undefined
									: [...visual.convexEligibleDirections].sort().join(",")
							}
							data-enhanced-virtual-flow={
								visual.enhancedScalingState === undefined
									? undefined
									: formatFlowRational(visual.enhancedScalingState.virtual_flow)
							}
							data-enhanced-reduced-cost={
								visual.enhancedScalingState?.reduced_cost
							}
							data-enhanced-internal={
								visual.enhancedScalingState?.internal || undefined
							}
							data-enhanced-strongly-feasible={
								visual.enhancedScalingState?.strongly_feasible || undefined
							}
							data-orlin-mcf-flow={
								visual.orlinMcfState?.flow === undefined
									? undefined
									: formatFlowRational(visual.orlinMcfState.flow.flow)
							}
							data-orlin-mcf-slack={
								visual.orlinMcfState?.slack === undefined
									? undefined
									: formatFlowRational(visual.orlinMcfState.slack.flow)
							}
							data-orlin-mcf-flow-reduced-cost={
								visual.orlinMcfState?.flow?.reduced_cost
							}
							data-orlin-mcf-slack-reduced-cost={
								visual.orlinMcfState?.slack?.reduced_cost
							}
							data-dual-simplex-basic-flow={visual.dualSimplexState?.basic_flow}
							data-dual-simplex-reduced-cost={
								visual.dualSimplexState?.reduced_cost
							}
							data-dual-simplex-tree={
								visual.dualSimplexState?.in_tree || undefined
							}
							data-dual-simplex-role={
								overlayViews.dualNetworkSimplex?.inspected_edge ===
								visual.edge.id
									? "inspected"
									: overlayViews.dualNetworkSimplex?.leaving_edge ===
											visual.edge.id
										? "leaving"
										: overlayViews.dualNetworkSimplex?.entering_edge ===
												visual.edge.id
											? "entering"
											: undefined
							}
							data-polynomial-dual-pseudoflow={
								visual.polynomialDualState === undefined
									? undefined
									: formatFlowRational(visual.polynomialDualState.pseudoflow)
							}
							data-polynomial-dual-basic-flow={
								visual.polynomialDualState?.basic_flow
							}
							data-polynomial-dual-reduced-cost={
								visual.polynomialDualState?.reduced_cost
							}
							data-polynomial-dual-tree={
								visual.polynomialDualState?.in_tree || undefined
							}
							data-polynomial-dual-bad={
								visual.polynomialDualState?.bad || undefined
							}
							data-polynomial-dual-path-direction={
								visual.polynomialDualState?.augment_direction
							}
							data-polynomial-dual-role={
								overlayViews.polynomialDualSimplex?.leaving_edge ===
								visual.edge.id
									? "leaving"
									: overlayViews.polynomialDualSimplex?.entering_edge ===
											visual.edge.id
										? "entering"
										: undefined
							}
							data-polynomial-primal-basis={visual.polynomialPrimalState?.basis}
							data-polynomial-primal-perturbed-flow={
								visual.polynomialPrimalState?.perturbed_flow
							}
							data-polynomial-primal-unperturbed-flow={
								visual.polynomialPrimalState?.unperturbed_basic_flow
							}
							data-polynomial-primal-reduced-cost={
								visual.polynomialPrimalState === undefined
									? undefined
									: formatFlowRational(
											visual.polynomialPrimalState.reduced_cost,
										)
							}
							data-polynomial-primal-cycle={
								visual.polynomialPrimalState?.in_cycle || undefined
							}
							data-polynomial-primal-role={
								visual.polynomialPrimalState?.entering
									? "entering"
									: visual.polynomialPrimalState?.leaving
										? "leaving"
										: undefined
							}
							data-convex-simplex-basis={visual.convexSimplexState?.basis}
							data-convex-simplex-active-segment={
								visual.convexSimplexState?.active_segment
							}
							data-convex-simplex-cycle={
								visual.convexSimplexState?.in_cycle || undefined
							}
							data-convex-simplex-entering={
								visual.convexSimplexState?.entering || undefined
							}
							data-convex-simplex-leaving={
								visual.convexSimplexState?.leaving || undefined
							}
							data-convex-simplex-role={
								visual.convexSimplexState?.entering
									? "entering"
									: visual.convexSimplexState?.leaving
										? "leaving"
										: undefined
							}
							data-prediction-scaled-cost={visual.predictionState?.scaled_cost}
							data-prediction-active-direction={
								visual.predictionActiveDirection
							}
							data-tardos-fixed-bound={
								tardosFixedByEdge.get(visual.edge.id)?.bound
							}
							data-tardos-fixed-value={
								tardosFixedByEdge.get(visual.edge.id)?.value
							}
							data-tardos-fixed-reduced-cost={
								tardosFixedByEdge.get(visual.edge.id)?.reduced_cost
							}
							data-electrical-current={visual.electricalState?.current}
							data-electrical-voltage={visual.electricalState?.voltage_drop}
							data-electrical-congestion={visual.electricalState?.congestion}
							data-electrical-energy={visual.electricalState?.energy}
							data-electrical-conductance={visual.electricalState?.conductance}
							data-electrical-resistance={
								visual.electricalState === undefined
									? undefined
									: formatFlowRational(visual.electricalState.resistance)
							}
							data-electrical-direction={
								visual.electricalState === undefined
									? undefined
									: Number(visual.electricalState.current) < 0
										? "reverse"
										: Number(visual.electricalState.current) > 0
											? "forward"
											: "zero"
							}
							data-augmenting-central-flow={
								visual.augmentingElectricalState?.central_flow
							}
							data-augmenting-current={
								visual.augmentingElectricalState?.electrical_current
							}
							data-augmenting-congestion={
								visual.augmentingElectricalState?.congestion
							}
							data-augmenting-resistance={
								visual.augmentingElectricalState?.resistance
							}
							data-augmenting-boost-segments={
								visual.augmentingElectricalState?.boost_segments
							}
							data-augmenting-rounded-central-flow={
								visual.augmentingElectricalState?.rounded_central_flow
							}
							data-augmenting-extraction-central-scaled={
								visual.augmentingElectricalState?.extraction_central_scaled
							}
							data-augmenting-extraction-toward-source={
								visual.augmentingElectricalState?.extraction_toward_source
							}
							data-augmenting-extraction-out-of-sink={
								visual.augmentingElectricalState?.extraction_out_of_sink
							}
							data-augmenting-final-flow={
								visual.augmentingElectricalState?.final_flow
							}
							data-ipm-fractional={visual.interiorPointState?.fractional_flow}
							data-ipm-current={visual.interiorPointState?.electrical_current}
							data-ipm-current-direction={
								visual.interiorPointState === undefined
									? undefined
									: Number(visual.interiorPointState.electrical_current) < 0
										? "reverse"
										: Number(visual.interiorPointState.electrical_current) > 0
											? "forward"
											: "zero"
							}
							data-ipm-slack={visual.interiorPointState?.slack}
							data-ipm-resistance={visual.interiorPointState?.resistance}
							data-ipm-congestion={visual.interiorPointState?.congestion}
							data-ipm-normalized-away={
								visual.interiorPointState?.normalized_away || undefined
							}
							data-ipm-final-flow={visual.interiorPointState?.final_flow}
							data-eipm-mcf-fractional={
								visual.electricalIpmMcfState?.fractional_flow
							}
							data-eipm-mcf-slack={visual.electricalIpmMcfState?.lower_slack}
							data-eipm-mcf-resistance={
								visual.electricalIpmMcfState?.resistance
							}
							data-eipm-mcf-current={
								visual.electricalIpmMcfState?.electrical_current
							}
							data-eipm-mcf-fixed={
								visual.electricalIpmMcfState?.fixed_on_face || undefined
							}
							data-eipm-mcf-final-flow={
								visual.electricalIpmMcfState?.final_flow
							}
							data-min-ratio-gradient={visual.minimumRatioState?.gradient}
							data-min-ratio-length={visual.minimumRatioState?.length}
							data-min-ratio-tree={
								visual.minimumRatioState?.tree_edge || undefined
							}
							data-min-ratio-candidate-sign={
								visual.minimumRatioState?.candidate_sign
							}
							data-min-ratio-selected-sign={
								visual.minimumRatioState?.selected_sign
							}
							data-min-ratio-numerator={
								visual.minimumRatioState?.numerator_contribution
							}
							data-min-ratio-denominator={
								visual.minimumRatioState?.denominator_contribution
							}
							data-randomized-interior-flow={
								visual.randomizedAlmostLinearState?.interior_flow
							}
							data-randomized-gradient={
								visual.randomizedAlmostLinearState?.gradient
							}
							data-randomized-length={
								visual.randomizedAlmostLinearState?.length
							}
							data-randomized-tree-memberships={
								visual.randomizedAlmostLinearState?.sampled_tree_memberships
							}
							data-randomized-active-tree={
								visual.randomizedAlmostLinearState?.active_tree_edge ||
								undefined
							}
							data-randomized-cycle-sign={
								visual.randomizedAlmostLinearState?.active_cycle_sign
							}
							data-randomized-changed={
								visual.randomizedAlmostLinearState?.changed_coordinate ||
								undefined
							}
							data-randomized-final-flow={
								visual.randomizedAlmostLinearState?.final_flow
							}
							data-randomized-isolation-draw={
								visual.randomizedAlmostLinearState?.isolation_draw
							}
							data-randomized-final-point-flow={
								visual.randomizedAlmostLinearState?.final_point_flow
							}
							data-deterministic-interior-flow={
								visual.deterministicAlmostLinearState?.interior_flow
							}
							data-deterministic-gradient={
								visual.deterministicAlmostLinearState?.gradient
							}
							data-deterministic-length={
								visual.deterministicAlmostLinearState?.length
							}
							data-deterministic-tree-mask={
								visual.deterministicAlmostLinearState?.tree_level_mask
							}
							data-deterministic-forest-mask={
								visual.deterministicAlmostLinearState?.forest_level_mask
							}
							data-deterministic-core={
								visual.deterministicAlmostLinearState?.active_core_edge ||
								undefined
							}
							data-deterministic-spanner={
								visual.deterministicAlmostLinearState?.active_spanner_edge ||
								undefined
							}
							data-deterministic-embedding-hops={
								visual.deterministicAlmostLinearState?.embedding_hops
							}
							data-deterministic-stretch={
								visual.deterministicAlmostLinearState?.embedding_stretch
							}
							data-deterministic-cycle-sign={
								visual.deterministicAlmostLinearState?.active_cycle_sign
							}
							data-deterministic-changed={
								visual.deterministicAlmostLinearState?.changed_coordinate ||
								undefined
							}
							data-deterministic-final-point-flow={
								visual.deterministicAlmostLinearState?.final_point_flow ===
								undefined
									? undefined
									: formatFlowRational(
											visual.deterministicAlmostLinearState.final_point_flow,
										)
							}
							data-deterministic-rounding-flow={
								visual.deterministicAlmostLinearState?.rounding_flow ===
								undefined
									? undefined
									: formatFlowRational(
											visual.deterministicAlmostLinearState.rounding_flow,
										)
							}
							data-deterministic-rounding-forest={
								visual.deterministicAlmostLinearState?.rounding_forest_edge ||
								undefined
							}
							data-deterministic-rounding-sign={
								visual.deterministicAlmostLinearState?.rounding_cycle_sign
							}
							data-deterministic-final-flow={
								visual.deterministicAlmostLinearState?.final_flow
							}
							className={`flow-original-edge ${expanded ? "flow-edge-expanded" : "flow-edge-context"}${tardosFixedByEdge.has(visual.edge.id) ? ` flow-edge-tardos-fixed flow-edge-tardos-fixed-${tardosFixedByEdge.get(visual.edge.id)?.bound}` : ""}${visual.crossesCut ? " flow-edge-cut" : ""}${predictedOriginalEdges.has(visual.edge.id) ? " flow-edge-predicted" : ""}${visual.parametricCapacityBand === undefined ? "" : ` flow-edge-parametric flow-capacity-band-${visual.parametricCapacityBand}`}${visual.fixed ? " flow-edge-fixed" : ""}${visual.matched ? " flow-edge-matched" : ""}${visual.orlinMcfState === undefined ? "" : " flow-edge-orlin-mcf"}${visual.electricalState === undefined ? "" : ` flow-edge-electrical flow-edge-electrical-energy-${visual.electricalEnergyBand}`}${visual.augmentingElectricalState === undefined ? "" : ` flow-edge-augmenting-electrical flow-edge-augmenting-congestion-${visual.augmentingCongestionBand}${BigInt(visual.augmentingElectricalState.boost_segments) > 1n ? " flow-edge-augmenting-boosted" : ""}`}${visual.interiorPointState === undefined ? "" : ` flow-edge-interior-point flow-edge-interior-congestion-${visual.interiorPointCongestionBand} flow-edge-interior-slack-${visual.interiorPointSlackBand} flow-edge-interior-resistance-${visual.interiorPointResistanceBand}${visual.interiorPointState.normalized_away ? " flow-edge-interior-normalized-away" : ""}`}${visual.randomizedAlmostLinearState === undefined ? "" : ` flow-edge-randomized-almost-linear flow-edge-randomized-gradient-${Number(visual.randomizedAlmostLinearState.gradient) < 0 ? "negative" : Number(visual.randomizedAlmostLinearState.gradient) > 0 ? "positive" : "zero"} flow-edge-randomized-gradient-band-${visual.randomizedGradientBand}${visual.randomizedAlmostLinearState.active_tree_edge ? " flow-edge-randomized-active-tree" : ""}${visual.randomizedAlmostLinearState.active_cycle_sign === "0" ? "" : " flow-edge-randomized-active-cycle"}${visual.randomizedAlmostLinearState.changed_coordinate ? " flow-edge-randomized-changed" : ""}`}${visual.deterministicAlmostLinearState === undefined ? "" : ` flow-edge-deterministic-almost-linear${visual.deterministicAlmostLinearState.active_tree_edge ? " flow-edge-deterministic-tree" : ""}${visual.deterministicAlmostLinearState.forest_level_mask === "0" ? "" : " flow-edge-deterministic-forest"}${visual.deterministicAlmostLinearState.active_core_edge ? " flow-edge-deterministic-core" : ""}${visual.deterministicAlmostLinearState.active_spanner_edge ? " flow-edge-deterministic-spanner" : ""}${visual.deterministicAlmostLinearState.active_cycle_sign === "0" ? "" : " flow-edge-deterministic-cycle"}${visual.deterministicAlmostLinearState.changed_coordinate ? " flow-edge-deterministic-changed" : ""}${visual.deterministicAlmostLinearState.rounding_forest_edge ? " flow-edge-deterministic-rounding-forest" : ""}${visual.deterministicAlmostLinearState.rounding_cycle_sign === "0" ? "" : " flow-edge-deterministic-rounding-cycle"}`}${visual.convexState === undefined ? "" : ` flow-edge-convex flow-convex-marginal-${visual.costKind} flow-convex-magnitude-${costMagnitudeBand(BigInt(visual.convexState.forward_marginal_cost ?? visual.convexState.reverse_marginal_cost ?? "0"), maximumConvexMarginalMagnitude)}`}${visual.convexEligibleDirections?.has("forward") === true ? " flow-edge-convex-eligible-forward" : ""}${visual.convexEligibleDirections?.has("reverse") === true ? " flow-edge-convex-eligible-reverse" : ""}${visual.convexActiveDirections?.has("forward") === true ? " flow-edge-convex-active-forward" : ""}${visual.convexActiveDirections?.has("reverse") === true ? " flow-edge-convex-active-reverse" : ""}${visual.convexSimplexState === undefined ? "" : ` flow-edge-convex-simplex flow-edge-convex-simplex-${visual.convexSimplexState.basis}${visual.convexSimplexState.in_cycle ? " flow-edge-convex-simplex-cycle" : ""}${visual.convexSimplexState.entering ? " flow-edge-convex-simplex-entering" : ""}${visual.convexSimplexState.leaving ? " flow-edge-convex-simplex-leaving" : ""}`}${basisOriginalEdges.has(visual.edge.id) ? " flow-edge-basis" : ""}${visual.ibfsTreeSide === "source" ? " flow-edge-ibfs-source-tree" : visual.ibfsTreeSide === "sink" ? " flow-edge-ibfs-sink-tree" : ""}${visual.eibfsTreeSide === "source" ? " flow-edge-eibfs-source-tree" : visual.eibfsTreeSide === "sink" ? " flow-edge-eibfs-sink-tree" : ""}${visual.active ? " flow-edge-active" : ""}${visual.enhancedScalingState === undefined ? "" : ` flow-edge-enhanced${visual.enhancedScalingState.tight ? " flow-edge-enhanced-tight" : " flow-edge-enhanced-slack"}${visual.enhancedScalingState.internal ? " flow-edge-enhanced-internal" : ""}${visual.enhancedScalingState.strongly_feasible ? " flow-edge-enhanced-strong" : ""}${overlayViews.enhancedCapacityScaling?.contraction_arc === visual.edge.id ? " flow-edge-enhanced-contract" : ""}`}${visual.dualSimplexState === undefined ? "" : ` flow-edge-dual-simplex${visual.dualSimplexState.in_tree ? " flow-edge-dual-tree" : " flow-edge-dual-nontree"}${BigInt(visual.dualSimplexState.basic_flow) < 0n ? " flow-edge-dual-infeasible" : ""}${visual.dualSimplexState.reduced_cost === "0" ? " flow-edge-dual-tight" : " flow-edge-dual-slack"}${overlayViews.dualNetworkSimplex?.leaving_edge === visual.edge.id ? " flow-edge-dual-leaving" : ""}${overlayViews.dualNetworkSimplex?.entering_edge === visual.edge.id ? " flow-edge-dual-entering" : ""}${overlayViews.dualNetworkSimplex?.inspected_edge === visual.edge.id ? " flow-edge-dual-inspected" : ""}`}${visual.polynomialDualState === undefined ? "" : ` flow-edge-polynomial-dual${visual.polynomialDualState.in_tree ? " flow-edge-polynomial-dual-tree" : " flow-edge-polynomial-dual-nontree"}${visual.polynomialDualState.bad ? " flow-edge-polynomial-dual-bad" : ""}${visual.polynomialDualState.in_augment_path ? " flow-edge-polynomial-dual-path" : ""}${overlayViews.polynomialDualSimplex?.leaving_edge === visual.edge.id ? " flow-edge-polynomial-dual-leaving" : ""}${overlayViews.polynomialDualSimplex?.entering_edge === visual.edge.id ? " flow-edge-polynomial-dual-entering" : ""}`}${visual.polynomialPrimalState === undefined ? "" : ` flow-edge-polynomial-primal flow-edge-polynomial-${visual.polynomialPrimalState.basis}${visual.polynomialPrimalState.in_cycle ? " flow-edge-polynomial-cycle" : ""}${visual.polynomialPrimalState.entering ? " flow-edge-polynomial-entering" : ""}${visual.polynomialPrimalState.leaving ? " flow-edge-polynomial-leaving" : ""}`}${visual.doubleScalingBranches !== undefined ? " flow-edge-double-active" : ""}${visual.cancelTightenCycle ? " flow-edge-cancel-cycle" : ""}${visual.relaxedMndcCycle === undefined ? "" : ` flow-edge-mndc-cycle flow-edge-mndc-cycle-${visual.relaxedMndcCycle % 4}`}${visual.cycleAdjustment === "add" ? " flow-edge-cycle-add" : visual.cycleAdjustment === "subtract" ? " flow-edge-cycle-subtract" : ""}${selected ? " flow-entity-selected" : ""}`}
							data-relaxed-mndc-cycle={visual.relaxedMndcCycle}
							data-capacity-band={visual.parametricCapacityBand}
							data-parametric-intersection-cut={
								parametricIntersectionCut || undefined
							}
						>
							<title>{`${visual.edge.id} · ${visual.parametricCapacityBand === undefined ? `${visual.flow}/` : "capacity "}${visual.capacityLabel}${visual.electricalState === undefined ? "" : ` · resistor R ${formatFlowRational(visual.electricalState.resistance)} · conductance ${visual.electricalState.conductance} · voltage ${visual.electricalState.voltage_drop} · signed current ${visual.electricalState.current} · congestion ${visual.electricalState.congestion} · energy ${visual.electricalState.energy}`}${visual.augmentingElectricalState === undefined ? "" : ` · central flow ${visual.augmentingElectricalState.central_flow}${visual.augmentingElectricalState.rounded_central_flow === undefined ? "" : ` → rounded central flow ${visual.augmentingElectricalState.rounded_central_flow}`} · electrical direction ${visual.augmentingElectricalState.electrical_current} · residual +/− ${visual.augmentingElectricalState.forward_residual}/${visual.augmentingElectricalState.backward_residual} · congestion ${visual.augmentingElectricalState.congestion} · barrier resistance ${visual.augmentingElectricalState.resistance} · boost segments ${visual.augmentingElectricalState.boost_segments}${visual.augmentingElectricalState.final_flow === undefined ? "" : ` · extracted flow ${visual.augmentingElectricalState.final_flow}`}`}${visual.interiorPointState === undefined ? "" : ` · IPM fractional flow ${visual.interiorPointState.fractional_flow} · electrical direction ${visual.interiorPointState.electrical_current} · dual slack ${visual.interiorPointState.slack} · measure ${visual.interiorPointState.measure} · resistance ${visual.interiorPointState.resistance} · congestion ${visual.interiorPointState.congestion}${visual.interiorPointState.normalized_away ? " · terminal-normalized away" : ""}${visual.interiorPointState.final_flow === undefined ? "" : ` · TU-rounded flow ${visual.interiorPointState.final_flow}`}`}${visual.randomizedAlmostLinearState === undefined ? "" : ` · randomized interior ${visual.randomizedAlmostLinearState.interior_flow} · gradient ${visual.randomizedAlmostLinearState.gradient} · length ${visual.randomizedAlmostLinearState.length} · sampled memberships ${visual.randomizedAlmostLinearState.sampled_tree_memberships}${visual.randomizedAlmostLinearState.active_tree_edge ? " · active tree" : ""}${visual.randomizedAlmostLinearState.active_cycle_sign === "0" ? "" : ` · cycle sign ${visual.randomizedAlmostLinearState.active_cycle_sign}`}${visual.randomizedAlmostLinearState.changed_coordinate ? " · detected coordinate" : ""}${visual.randomizedAlmostLinearState.final_flow === undefined ? "" : ` · rounded flow ${visual.randomizedAlmostLinearState.final_flow}`}`}${visual.deterministicAlmostLinearState === undefined ? "" : ` · deterministic interior ${visual.deterministicAlmostLinearState.interior_flow} · gradient ${visual.deterministicAlmostLinearState.gradient} · length ${visual.deterministicAlmostLinearState.length} · tree/forest masks ${visual.deterministicAlmostLinearState.tree_level_mask}/${visual.deterministicAlmostLinearState.forest_level_mask}${visual.deterministicAlmostLinearState.active_core_edge ? " · core" : ""}${visual.deterministicAlmostLinearState.active_spanner_edge ? " · spanner" : ""}${BigInt(visual.deterministicAlmostLinearState.embedding_hops) === 0n ? "" : ` · embedding ${visual.deterministicAlmostLinearState.embedding_hops} hops · stretch ${visual.deterministicAlmostLinearState.embedding_stretch}`}${visual.deterministicAlmostLinearState.active_cycle_sign === "0" ? "" : ` · ${overlayViews.deterministicAlmostLinear?.selected_cycle_kind ?? "tree"} cycle sign ${visual.deterministicAlmostLinearState.active_cycle_sign}`}${visual.deterministicAlmostLinearState.changed_coordinate ? " · detected coordinate" : ""}${visual.deterministicAlmostLinearState.final_point_flow === undefined ? "" : ` · final point ${formatFlowRational(visual.deterministicAlmostLinearState.final_point_flow)}`}${visual.deterministicAlmostLinearState.rounding_flow === undefined ? "" : ` · rounding ${formatFlowRational(visual.deterministicAlmostLinearState.rounding_flow)}`}${visual.deterministicAlmostLinearState.rounding_forest_edge ? " · fractional forest" : ""}${visual.deterministicAlmostLinearState.rounding_cycle_sign === "0" ? "" : ` · rounding cycle sign ${visual.deterministicAlmostLinearState.rounding_cycle_sign}`}${visual.deterministicAlmostLinearState.final_flow === undefined ? "" : ` · rounded flow ${visual.deterministicAlmostLinearState.final_flow}`}`}${visual.convexState === undefined ? (showsCost ? ` · cost ${visual.signedCost}` : "") : ` · convex cost φ ${visual.convexState.total_cost} · base ${visual.convexState.base_cost_at_zero} · forward marginal ${visual.convexState.forward_marginal_cost ?? "none"} · reverse marginal ${visual.convexState.reverse_marginal_cost ?? "none"} · segments ${visual.convexState.segments.map((segment) => `[${segment.start_flow},${segment.end_flow}]:${segment.flow}@${segment.marginal_cost}`).join(" | ")}`}${visual.convexSimplexState === undefined ? "" : ` · compact ${visual.convexSimplexState.basis}${visual.convexSimplexState.active_segment === undefined ? " at breakpoint" : ` · active segment ${visual.convexSimplexState.active_segment}`}${visual.convexSimplexState.in_cycle ? " · fundamental cycle" : ""}${visual.convexSimplexState.entering ? " · entering" : ""}${visual.convexSimplexState.leaving ? " · Cunningham leaving" : ""}`}${visual.enhancedScalingState === undefined ? "" : ` · virtual ${formatFlowRational(visual.enhancedScalingState.virtual_flow)} · reduced cost ${visual.enhancedScalingState.reduced_cost}${visual.enhancedScalingState.internal ? " · internal" : ""}${visual.enhancedScalingState.strongly_feasible ? " · strongly feasible" : ""}`}${visual.dualSimplexState === undefined ? "" : ` · signed basic flow ${visual.dualSimplexState.basic_flow} · reduced cost ${visual.dualSimplexState.reduced_cost}${visual.dualSimplexState.in_tree ? " · tree basis" : " · non-tree"}`}${visual.polynomialDualState === undefined ? "" : ` · auxiliary pseudoflow ${formatFlowRational(visual.polynomialDualState.pseudoflow)} · signed basic flow ${visual.polynomialDualState.basic_flow} · reduced cost ${visual.polynomialDualState.reduced_cost}${visual.polynomialDualState.in_tree ? " · tree basis" : " · non-tree"}${visual.polynomialDualState.bad ? " · bad downward arc" : ""}${visual.polynomialDualState.in_augment_path ? ` · active path ${visual.polynomialDualState.augment_direction}` : ""}`}${visual.polynomialPrimalState === undefined ? "" : ` · perturbed flow ${visual.polynomialPrimalState.perturbed_flow}/${overlayViews.polynomialPrimalSimplex?.perturbation_scale ?? "1"} · unperturbed basic flow ${visual.polynomialPrimalState.unperturbed_basic_flow} · exact reduced cost ${formatFlowRational(visual.polynomialPrimalState.reduced_cost)} · ${visual.polynomialPrimalState.basis} basis${visual.polynomialPrimalState.in_cycle ? " · fundamental cycle" : ""}${visual.polynomialPrimalState.entering ? " · entering" : ""}${visual.polynomialPrimalState.leaving ? " · leaving" : ""}`}${predictedOriginalEdges.has(visual.edge.id) ? ` · predicted ${visual.edge.initial_flow}` : ""}${visual.eibfsTreeSide === undefined ? "" : ` · EIBFS ${visual.eibfsTreeSide === "source" ? "S" : "T"} forest ${visual.eibfsTreeParent}→${visual.eibfsTreeChild}`}${visual.cancelTightenCycle ? " · selected cancel cycle" : ""}${visual.relaxedMndcCycle === undefined ? "" : ` · MNDC cycle ${visual.relaxedMndcCycle + 1}`}${visual.doubleScalingState === undefined ? "" : ` · transformed flow/slack ${visual.doubleScalingState.flow_branch}/${visual.doubleScalingState.slack_branch}`}${visual.doubleScalingBranches === undefined ? "" : ` · active ${visual.doubleScalingBranches.join(", ")}`}${visual.convexEligibleDirections === undefined ? "" : ` · Δ-eligible marginal ${[...visual.convexEligibleDirections].sort().join("/")}`}${visual.convexActiveDirections === undefined ? "" : ` · active marginal ${[...visual.convexActiveDirections].sort().join("/")}`}${visual.cycleAdjustment === "add" ? " · +theta" : visual.cycleAdjustment === "subtract" ? " · -theta" : ""}${visual.matched ? (context.model.kind === "assignment" ? " · ASSIGN" : " · MATCH") : ""}${tardosFixedByEdge.has(visual.edge.id) ? ` · fixed ${tardosFixedByEdge.get(visual.edge.id)?.bound}=${tardosFixedByEdge.get(visual.edge.id)?.value} · witness c̄ ${tardosFixedByEdge.get(visual.edge.id)?.reduced_cost}` : ""}${visual.fixed ? " · FIX" : ""}`}</title>
							<FlowGraphDiscreteEdgeUnderlayBundle
								state={state}
								visual={visual}
								enabled={
									activeBundles.has("original-edge-discrete-underlay") ||
									state.visualization.ibfsView !== undefined ||
									state.visualization.features.capacityScaling
								}
							/>
							{parametricIntersectionCut && (
								<FlowGraphOverlayOwnedLeaves
									state={state}
									bundle="original-edge-discrete-overlay"
									entity={{ kind: "edge", id: visual.edge.id }}
									owners={[
										{
											overlay: "parametric_overlay",
											role: "traversal.solve-intersection.cut-boundary",
										},
									]}
								>
									<path
										d={visual.geometry.path}
										className="flow-parametric-intersection-cut"
										strokeWidth={visual.railWidth + 10}
									/>
								</FlowGraphOverlayOwnedLeaves>
							)}
							{visual.active && (
								<path
									d={visual.geometry.path}
									className="flow-active-outline"
									strokeWidth={visual.railWidth + (showsCost ? 8 : 5)}
								/>
							)}
							{changed && (
								<path
									d={visual.geometry.path}
									className="flow-event-change-edge-outline"
									strokeWidth={visual.railWidth + (showsCost ? 14 : 11)}
								/>
							)}
							{touched && (
								<path
									d={visual.geometry.path}
									className="flow-event-touch-edge-outline"
									strokeWidth={visual.railWidth + (showsCost ? 10 : 7)}
								/>
							)}
							{selected && (
								<path
									d={visual.geometry.path}
									className="flow-selection-outline"
									strokeWidth={visual.railWidth + (showsCost ? 11 : 8)}
								/>
							)}
							{expanded &&
								showsCost &&
								visual.parametricCapacityBand === undefined && (
									<ConvexMarginalCostLeaf state={state} visual={visual}>
										<path
											d={visual.geometry.path}
											className={`flow-cost-rail flow-cost-halo flow-cost-line flow-cost-${visual.costKind} flow-cost-magnitude-${visual.magnitudeBand}`}
											data-flow-channel="cost"
											style={
												{
													"--flow-cost-intensity": visual.costIntensity,
													"--flow-cost-mix": `${Math.round(42 + visual.costIntensity * 58)}%`,
													strokeWidth: visual.railWidth + 4,
												} as CSSProperties
											}
											strokeWidth={visual.railWidth + 4}
										/>
									</ConvexMarginalCostLeaf>
								)}
							<ParametricCapacityLeaf state={state} visual={visual}>
								<path
									d={visual.geometry.path}
									className={`flow-capacity-rail${!expanded && showsCost ? ` flow-context-cost flow-context-cost-${visual.costKind} flow-context-cost-magnitude-${visual.magnitudeBand}` : ""}`}
									data-flow-channel="capacity"
									data-context-cost={
										!expanded && showsCost ? visual.costKind : undefined
									}
									style={
										{
											"--flow-cost-mix": `${Math.round(42 + visual.costIntensity * 58)}%`,
											strokeWidth: expanded ? visual.railWidth : 1.65,
										} as CSSProperties
									}
									strokeWidth={expanded ? visual.railWidth : 1.65}
									markerEnd={flowScopedSvgUrl(
										idScope,
										expanded ? "flow-arrow-capacity" : "flow-arrow-context",
									)}
								/>
							</ParametricCapacityLeaf>
							{expanded && (
								<path
									d={visual.geometry.path}
									className="flow-flow-line"
									data-flow-channel="flow"
									data-cost-magnitude={visual.magnitudeBand}
									style={{ strokeWidth: visual.flowWidth }}
									strokeWidth={visual.flowWidth}
									markerEnd={
										visual.flowWidth > 0
											? flowScopedSvgUrl(idScope, "flow-arrow-fill")
											: undefined
									}
								/>
							)}
							<FlowGraphTreeChainEdgeFeatureBundle
								state={state}
								visual={visual}
								enabled={activeBundles.has("original-edge-tree-chain")}
							/>
							<FlowGraphElectricalEdgeFeatureBundle
								state={state}
								visual={visual}
								enabled={activeBundles.has("original-edge-electrical")}
							/>
							<FlowGraphDiscreteEdgeOverlayBundle
								state={state}
								visual={visual}
								enabled={
									visual.crossesCut ||
									activeBundles.has("original-edge-discrete-overlay")
								}
							/>
							{isTransportationOptimalityCertificateRoute(
								context.algorithmId,
								context.traceEvent?.catalog_id,
								basisOriginalEdges.has(visual.edge.id),
							) && (
								<path
									d={visual.geometry.path}
									className="flow-transportation-optimality-mark"
									data-transportation-optimality="nonnegative-reduced-cost"
									pathLength="100"
								>
									<title>{`${visual.edge.id}: nonbasic route certified with nonnegative reduced cost`}</title>
								</path>
							)}
							<path d={visual.geometry.path} className="flow-edge-hit-target" />
						</g>
					);
				})}
		</>
	);
}
