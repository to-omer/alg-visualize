import { FlowGraphOverlayOwnedLeaves } from "./FlowGraphOverlayOwnedLeaves";
import { flowScopedSvgUrl, useFlowGraphIdScope } from "./flow-dom-id";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import type { FlowEntitySelection } from "./flow-entity-navigator";
import {
	flowMinimumMeanResidualScan,
	flowPolynomialPrimalScan,
	flowPrimitiveArcInspection,
	flowRelaxationArcScan,
	ordinaryFlowEventEntityRefs,
} from "./flow-event-highlight";
import { isOriginalEdgeSelected } from "./flow-graph-entity-selection";
import {
	FLOW_NODE_RADIUS,
	FLOW_VIEWBOX_HEIGHT,
	FLOW_VIEWBOX_WIDTH,
	type FlowEdgeRoute,
} from "./flow-layout";
import { formatFlowRational } from "./flow-parametric-view";
import { costMagnitudeBand } from "./flow-visual-scales";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
type FlowGraphLayerProps = Readonly<{
	state: FlowEntityGraphState;
	selection: FlowEntitySelection | undefined;
	hoveredEdgeId: string | undefined;
}>;

export function flowCycleAdjustmentLabel(
	adjustment: "add" | "subtract",
	catalogId: string | undefined,
): string {
	const degenerate =
		catalogId === "transportation-simplex.degenerate-pivot" ||
		catalogId === "modi.degenerate-loop-adjustment";
	if (degenerate) return adjustment === "add" ? "+0" : "−0";
	return adjustment === "add" ? "+θ" : "−θ";
}

function routeWithNodeClearLabel(
	route: FlowEdgeRoute,
	positions: ReadonlyMap<string, Readonly<{ x: number; y: number }>>,
	labelText: string,
): FlowEdgeRoute {
	const initialCenter = {
		x: route.label.x,
		y: route.label.y + route.labelYOffset,
	};
	const localOffsets = [-88, -66, -44, -22, 0, 22, 44, 66, 88].flatMap((y) =>
		[-120, -80, -40, 0, 40, 80, 120].map((x) => ({ x, y })),
	);
	localOffsets.sort((left, right) => {
		const leftDistance = left.x ** 2 + left.y ** 2;
		const rightDistance = right.x ** 2 + right.y ** 2;
		if (leftDistance !== rightDistance) return leftDistance - rightDistance;
		return Math.abs(left.x) - Math.abs(right.x);
	});
	const labelBoxWidth = Math.max(
		route.labelBoxWidth,
		18 + labelText.length * 6,
	);
	const labelHeight = Math.max(route.labelHeight, 20);
	const halfWidth = labelBoxWidth / 2;
	const halfHeight = labelHeight / 2;
	const localCenters = localOffsets.map((offset) => ({
		x: initialCenter.x + offset.x,
		y: initialCenter.y + offset.y,
	}));
	const railX = Math.max(
		10 + halfWidth,
		Math.min(FLOW_VIEWBOX_WIDTH - 10 - halfWidth, initialCenter.x),
	);
	const centers = [
		...localCenters,
		{ x: railX, y: 10 + halfHeight },
		{ x: railX, y: FLOW_VIEWBOX_HEIGHT - 10 - halfHeight },
	];
	const nodeClearance = FLOW_NODE_RADIUS + 5;
	for (const center of centers) {
		if (
			center.x - halfWidth < 10 ||
			center.x + halfWidth > FLOW_VIEWBOX_WIDTH - 10 ||
			center.y - halfHeight < 10 ||
			center.y + halfHeight > FLOW_VIEWBOX_HEIGHT - 10
		)
			continue;
		const overlapsNode = [...positions.values()].some((node) => {
			const closestX = Math.max(
				center.x - halfWidth,
				Math.min(node.x, center.x + halfWidth),
			);
			const closestY = Math.max(
				center.y - halfHeight,
				Math.min(node.y, center.y + halfHeight),
			);
			return (
				(closestX - node.x) ** 2 + (closestY - node.y) ** 2 < nodeClearance ** 2
			);
		});
		if (overlapsNode) continue;
		return {
			...route,
			labelBoxWidth,
			labelHeight,
			label: {
				x: center.x,
				y: center.y - route.labelYOffset,
			},
		};
	}
	throw new Error(`No node-clear annotation position for edge ${route.edgeId}`);
}

function FlowEdgeAnnotationConnector({
	geometry,
}: Readonly<{
	geometry: FlowEdgeRoute;
}>) {
	const anchorX = geometry.labelAnchor.x - geometry.label.x;
	const anchorY =
		geometry.labelAnchor.y - (geometry.label.y + geometry.labelYOffset);
	return (
		<g className="flow-edge-label-connector">
			<line
				className="flow-edge-label-leader-halo"
				x1={anchorX}
				y1={anchorY}
				x2="0"
				y2="0"
			/>
			<line
				className="flow-edge-label-leader"
				x1={anchorX}
				y1={anchorY}
				x2="0"
				y2="0"
			/>
			{geometry.parallelCount === 1 && (
				<circle
					className="flow-edge-label-anchor"
					cx={anchorX}
					cy={anchorY}
					r="3"
				/>
			)}
		</g>
	);
}

function FlowEdgeRouteLaneToken({
	geometry,
	focused,
}: Readonly<{
	geometry: FlowEdgeRoute;
	focused: boolean;
}>) {
	return (
		<g
			className={`flow-edge-route-lane-token${focused ? " flow-edge-route-lane-token-focused" : ""}`}
			data-edge-id={geometry.edgeId}
			data-route-lane-token={`${geometry.parallelIndex}/${geometry.parallelCount}`}
			transform={`translate(${geometry.laneToken.x} ${geometry.laneToken.y})`}
		>
			<title>{`Edge ${geometry.edgeId}, parallel lane ${geometry.parallelIndex} of ${geometry.parallelCount}`}</title>
			<path
				className="flow-edge-route-lane-token-shape"
				d="M -11 -7 L 3 -7 L 11 0 L 3 7 L -11 7 Z"
				transform={`rotate(${geometry.laneTokenAngle})`}
			/>
			<text textAnchor="middle" dominantBaseline="central">
				{geometry.parallelIndex}
			</text>
		</g>
	);
}

export function FlowGraphEdgeAnnotationLayers({
	state,
	selection,
	hoveredEdgeId,
}: FlowGraphLayerProps) {
	const idScope = useFlowGraphIdScope();
	const plan = state.plan;
	const viewMode = state.viewMode;
	const context = state.context;
	const degenerateTransportationPivot =
		context.traceEvent?.catalog_id ===
			"transportation-simplex.degenerate-pivot" ||
		context.traceEvent?.catalog_id === "modi.degenerate-loop-adjustment";
	const layout = state.layout;
	const positions = state.positions;
	const orlinMaxCompactVisuals = state.orlinMaxCompactVisuals;
	const planarDual = state.planarDual;
	const maxResidualCapacity = state.maxResidualCapacity;
	const _terminal = state.terminal;
	const originalVisuals = state.originalVisuals;
	const _pseudoflow = state.visualization.features.pseudoflow;
	const _orlinMaxFlow = state.visualization.features.orlinMaxFlow;
	const _polynomialPrimalSimplex =
		state.visualization.features.polynomialPrimalSimplex;
	const overlayViews = state.renderData.overlayViews;
	const maximumConvexMarginalMagnitude =
		state.renderData.maximumConvexMarginalMagnitude;
	const orlinMaxActiveOriginalKeys =
		state.renderData.orlinMaxActiveOriginalKeys;
	const orlinMaxActiveCompactCount = orlinMaxCompactVisuals.filter(
		(visual) => visual.activeReverse !== undefined,
	).length;
	const orlinMaxInspectedEdgeIds = new Set(
		overlayViews.orlinMaxFlow?.residual_arcs.flatMap((arc) =>
			arc.inspection_serial === undefined ? [] : [arc.edge_id],
		) ?? [],
	);
	const tardosFixedByEdge = state.renderData.tardosFixedByEdge;
	const showsCost = !new Set([
		"max-flow",
		"parametric-max-flow",
		"planar-max-flow",
		"bipartite-matching",
	]).has(context.model.kind);
	const compactLabels = originalVisuals.length > 16;
	const touchedEdgeIds = new Set(
		ordinaryFlowEventEntityRefs(context).flatMap((entity) =>
			entity.kind === "edge" || entity.kind === "residual-arc"
				? [entity.edge_id]
				: [],
		),
	);
	const parametricScan =
		context.traceEvent?.catalog_id ===
			"parametric-pseudoflow.inspect-residual-arc" &&
		context.traceEvent.detail !== undefined
			? {
					ordinal: context.traceEvent.detail.value,
					orientation:
						overlayViews.parametric?.traversal?.orientation === "reverse"
							? "REV"
							: "FWD",
				}
			: undefined;
	const polynomialPrimalScan = flowPolynomialPrimalScan(context);
	const minimumMeanScan = flowMinimumMeanResidualScan(context);
	const relaxationScan = flowRelaxationArcScan(context);
	const primitiveInspection =
		polynomialPrimalScan === undefined &&
		minimumMeanScan === undefined &&
		relaxationScan === undefined
			? flowPrimitiveArcInspection(context)
			: undefined;
	return (
		<>
			{viewMode !== "residual" &&
				plan.level === "detail" &&
				originalVisuals
					.filter((visual) => visual.geometry.parallelCount > 1)
					.map((visual) => (
						<FlowEdgeRouteLaneToken
							key={`route-token:${visual.edge.id}`}
							geometry={visual.geometry}
							focused={
								visual.edge.id === hoveredEdgeId ||
								isOriginalEdgeSelected(selection, visual.edge.id) ||
								visual.active ||
								touchedEdgeIds.has(visual.edge.id)
							}
						/>
					))}
			{viewMode !== "residual" &&
				originalVisuals
					.filter((visual) => {
						if (orlinMaxInspectedEdgeIds.has(visual.edge.id)) return false;
						const explicitlyFocused =
							visual.edge.id === hoveredEdgeId ||
							isOriginalEdgeSelected(selection, visual.edge.id);
						const sourceScanFocused =
							(polynomialPrimalScan !== undefined &&
								polynomialPrimalScan.target.kind !== "node" &&
								polynomialPrimalScan.target.edge_id === visual.edge.id) ||
							(minimumMeanScan !== undefined &&
								minimumMeanScan.target.edge_id === visual.edge.id) ||
							(relaxationScan !== undefined &&
								relaxationScan.target.edge_id === visual.edge.id) ||
							(primitiveInspection !== undefined &&
								primitiveInspection.target.edge_id === visual.edge.id);
						const eventFocused =
							visual.active ||
							visual.crossesCut ||
							touchedEdgeIds.has(visual.edge.id);
						return (
							explicitlyFocused ||
							sourceScanFocused ||
							(visual.geometry.labelCollisionFree &&
								(eventFocused ||
									(!compactLabels &&
										visual.geometry.parallelCount === 1 &&
										plan.edgeLabelIds.has(visual.edge.id))))
						);
					})
					.map((visual) => (
						<g
							key={`label:${visual.edge.id}`}
							data-edge-label-for={visual.edge.id}
							data-parallel-index={visual.geometry.parallelIndex}
							data-parallel-count={visual.geometry.parallelCount}
							data-label-collision-free={visual.geometry.labelCollisionFree}
							className={`${visual.crossesCut ? "flow-edge-cut" : ""}${isOriginalEdgeSelected(selection, visual.edge.id) ? " flow-entity-selected" : ""}${visual.active || touchedEdgeIds.has(visual.edge.id) ? " flow-edge-label-focused" : ""} flow-edge-label-group`}
							transform={`translate(${visual.geometry.label.x} ${visual.geometry.label.y + visual.geometry.labelYOffset})`}
						>
							<FlowEdgeAnnotationConnector geometry={visual.geometry} />
							<rect
								className="flow-edge-label-bg"
								x={-visual.geometry.labelBoxWidth / 2}
								y={-visual.geometry.labelHeight / 2}
								width={visual.geometry.labelBoxWidth}
								height={visual.geometry.labelHeight}
								rx="4"
							/>
							{visual.geometry.parallelCount > 1 && (
								<g
									className="flow-edge-parallel-badge"
									transform={`translate(${-visual.geometry.labelBoxWidth / 2 + 5} 0)`}
								>
									<rect x="0" y="-8" width="34" height="16" rx="8" />
									<text
										x="17"
										y="0"
										textAnchor="middle"
										dominantBaseline="central"
									>
										{`${visual.geometry.parallelIndex}/${visual.geometry.parallelCount}`}
									</text>
								</g>
							)}
							<text
								className="flow-edge-label"
								x={visual.geometry.parallelCount > 1 ? 20 : 0}
								textAnchor="middle"
								dominantBaseline="central"
								y={
									visual.convexState === undefined &&
									visual.predictionState === undefined &&
									visual.electricalState === undefined &&
									visual.augmentingElectricalState === undefined &&
									visual.electricalIpmMcfState === undefined &&
									visual.interiorPointState === undefined &&
									visual.minimumRatioState === undefined &&
									visual.randomizedAlmostLinearState === undefined &&
									visual.deterministicAlmostLinearState === undefined
										? showsCost
											? -5
											: 0
										: -4
								}
							>
								{visual.deterministicAlmostLinearState !== undefined ? (
									`x ${visual.deterministicAlmostLinearState.interior_flow} · g ${Number(visual.deterministicAlmostLinearState.gradient) > 0 ? "+" : ""}${visual.deterministicAlmostLinearState.gradient} · ℓ ${visual.deterministicAlmostLinearState.length} · T/F ${visual.deterministicAlmostLinearState.tree_level_mask}/${visual.deterministicAlmostLinearState.forest_level_mask}${visual.deterministicAlmostLinearState.active_core_edge ? " · core" : ""}${visual.deterministicAlmostLinearState.active_spanner_edge ? " · span" : ""}${BigInt(visual.deterministicAlmostLinearState.embedding_hops) === 0n ? "" : ` · emb ${visual.deterministicAlmostLinearState.embedding_hops}h/${visual.deterministicAlmostLinearState.embedding_stretch}`}${visual.deterministicAlmostLinearState.active_cycle_sign === "0" ? "" : ` · Δ${visual.deterministicAlmostLinearState.active_cycle_sign} ${overlayViews.deterministicAlmostLinear?.selected_cycle_kind ?? "tree"}`}${visual.deterministicAlmostLinearState.changed_coordinate ? " · DETECT" : ""}${visual.deterministicAlmostLinearState.final_point_flow === undefined ? "" : ` · x* ${formatFlowRational(visual.deterministicAlmostLinearState.final_point_flow)}`}${visual.deterministicAlmostLinearState.rounding_flow === undefined ? "" : ` · xᵣ ${formatFlowRational(visual.deterministicAlmostLinearState.rounding_flow)}`}${visual.deterministicAlmostLinearState.rounding_forest_edge ? " · Fᵣ" : ""}${visual.deterministicAlmostLinearState.rounding_cycle_sign === "0" ? "" : ` · Δᵣ${visual.deterministicAlmostLinearState.rounding_cycle_sign}`}${visual.deterministicAlmostLinearState.final_flow === undefined ? "" : ` · f ${visual.deterministicAlmostLinearState.final_flow}`}`
								) : visual.randomizedAlmostLinearState !== undefined ? (
									`x ${visual.randomizedAlmostLinearState.interior_flow} · g ${Number(visual.randomizedAlmostLinearState.gradient) > 0 ? "+" : ""}${visual.randomizedAlmostLinearState.gradient} · ℓ ${visual.randomizedAlmostLinearState.length} · T ${visual.randomizedAlmostLinearState.sampled_tree_memberships}/${overlayViews.randomizedAlmostLinear?.sample_count ?? "0"}${visual.randomizedAlmostLinearState.active_tree_edge ? " · active T" : ""}${visual.randomizedAlmostLinearState.active_cycle_sign === "0" ? "" : ` · Δ ${visual.randomizedAlmostLinearState.active_cycle_sign}`}${visual.randomizedAlmostLinearState.changed_coordinate ? " · DETECT" : ""}${visual.randomizedAlmostLinearState.final_flow === undefined ? "" : ` · f ${visual.randomizedAlmostLinearState.final_flow}`}`
								) : visual.minimumRatioState !== undefined ? (
									`g ${BigInt(visual.minimumRatioState.gradient) > 0n ? "+" : ""}${visual.minimumRatioState.gradient} · ℓ ${visual.minimumRatioState.length} · z ${visual.minimumRatioState.candidate_sign} · z* ${visual.minimumRatioState.selected_sign} · ΔN ${visual.minimumRatioState.numerator_contribution} · ΔD ${visual.minimumRatioState.denominator_contribution}`
								) : visual.electricalIpmMcfState !== undefined ? (
									`x̂ ${visual.electricalIpmMcfState.fractional_flow} · s ${visual.electricalIpmMcfState.lower_slack} · R ${visual.electricalIpmMcfState.resistance} · i ${visual.electricalIpmMcfState.electrical_current}${visual.electricalIpmMcfState.fixed_on_face ? " · FIXED" : ""}${visual.electricalIpmMcfState.final_flow === undefined ? "" : ` · f ${visual.electricalIpmMcfState.final_flow}`}`
								) : visual.interiorPointState !== undefined ? (
									visual.interiorPointState.normalized_away ? (
										"terminal-normalized away"
									) : (
										`x ${visual.interiorPointState.fractional_flow} · ŝ ${visual.interiorPointState.slack} · f̂ ${visual.interiorPointState.electrical_current} · r ${visual.interiorPointState.resistance} · ρ ${visual.interiorPointState.congestion}${visual.interiorPointState.final_flow === undefined ? "" : ` · f ${visual.interiorPointState.final_flow}`}`
									)
								) : visual.augmentingElectricalState !== undefined ? (
									visual.augmentingElectricalState.extraction_central_scaled !==
									undefined ? (
										`DIRECTED×2 ${visual.augmentingElectricalState.extraction_central_scaled} · HEAD→SOURCE ${visual.augmentingElectricalState.extraction_toward_source} · SINK→TAIL ${visual.augmentingElectricalState.extraction_out_of_sink}${visual.augmentingElectricalState.final_flow === undefined ? "" : ` · FLOW ${visual.augmentingElectricalState.final_flow}`}`
									) : visual.augmentingElectricalState.rounded_central_flow ===
										undefined ? (
										`x ${visual.augmentingElectricalState.central_flow} · f̂ ${visual.augmentingElectricalState.electrical_current} · ρ ${visual.augmentingElectricalState.congestion} · r ${visual.augmentingElectricalState.resistance} · β ${visual.augmentingElectricalState.boost_segments}${visual.augmentingElectricalState.final_flow === undefined ? "" : ` · f ${visual.augmentingElectricalState.final_flow}`}`
									) : (
										`CENTRAL ${visual.augmentingElectricalState.central_flow} → INTEGER ${visual.augmentingElectricalState.rounded_central_flow}${visual.augmentingElectricalState.final_flow === undefined ? "" : ` · DIRECTED ${visual.augmentingElectricalState.final_flow}`}`
									)
								) : visual.electricalState !== undefined ? (
									`I ${visual.electricalState.current} · ρ ${visual.electricalState.congestion} · E ${visual.electricalState.energy} · R ${formatFlowRational(visual.electricalState.resistance)}`
								) : visual.polynomialDualState !== undefined ? (
									`x̃ ${formatFlowRational(visual.polynomialDualState.pseudoflow)} · xᴮ ${visual.polynomialDualState.basic_flow} · c̄ ${visual.polynomialDualState.reduced_cost}${visual.polynomialDualState.in_tree ? " · T" : ""}${visual.polynomialDualState.bad ? " · BAD" : ""}`
								) : visual.polynomialPrimalState !== undefined ? (
									`x̃ ${visual.polynomialPrimalState.perturbed_flow}/${overlayViews.polynomialPrimalSimplex?.perturbation_scale ?? "1"} · c̄ ${formatFlowRational(visual.polynomialPrimalState.reduced_cost)} · ${visual.polynomialPrimalState.basis === "tree" ? "T" : visual.polynomialPrimalState.basis === "lower" ? "L" : "U"}`
								) : visual.dualSimplexState !== undefined ? (
									`xᴮ ${visual.dualSimplexState.basic_flow} · c̄ ${visual.dualSimplexState.reduced_cost}${visual.dualSimplexState.in_tree ? " · T" : ""}`
								) : visual.orlinMcfState?.flow !== undefined &&
									visual.orlinMcfState.slack !== undefined ? (
									`F ${formatFlowRational(visual.orlinMcfState.flow.flow)} · S ${formatFlowRational(visual.orlinMcfState.slack.flow)} · c̄ ${visual.orlinMcfState.flow.reduced_cost}/${visual.orlinMcfState.slack.reduced_cost}`
								) : visual.doubleScalingState !== undefined ? (
									`F ${visual.doubleScalingState.flow_branch} · S ${visual.doubleScalingState.slack_branch}${visual.doubleScalingInspectedArc === undefined ? "" : ` · SCAN ${context.traceEvent?.detail?.value ?? "?"} ${visual.doubleScalingInspectedArc.branch.toUpperCase()} ${visual.doubleScalingInspectedArc.direction === "forward" ? "FWD" : "REV"}`}`
								) : visual.enhancedScalingState !== undefined ? (
									`x̃ ${formatFlowRational(visual.enhancedScalingState.virtual_flow)} · c̄ ${visual.enhancedScalingState.reduced_cost}`
								) : visual.predictionState !== undefined ? (
									`${visual.flow}/${visual.capacityLabel} · c ${visual.signedCost} · cₜ ${visual.predictionState.scaled_cost}`
								) : visual.convexState !== undefined ? (
									`${visual.flow}/${visual.capacityLabel} · φ ${visual.convexState.total_cost} · μ+ ${visual.convexState.forward_marginal_cost ?? "—"} · μ− ${visual.convexState.reverse_marginal_cost ?? "—"}${visual.convexSimplexState === undefined ? "" : ` · ${visual.convexSimplexState.basis === "tree" ? "T" : "BP"}${visual.convexSimplexState.active_segment === undefined ? "" : `:${visual.convexSimplexState.active_segment}`}`}`
								) : visual.parametricCapacityBand === undefined ? (
									<>
										<tspan>{`FLOW ${visual.flow}  ·  CAP ${visual.capacityLabel}`}</tspan>
										{showsCost && (
											<tspan
												x="0"
												dy="12"
												className={`flow-edge-cost-chip flow-edge-cost-chip-${visual.costKind}`}
											>
												{`COST ${visual.signedCost}`}
											</tspan>
										)}
									</>
								) : (
									`u(λ) ${visual.capacityLabel}${parametricScan !== undefined && touchedEdgeIds.has(visual.edge.id) ? ` · SCAN ${parametricScan.ordinal} ${parametricScan.orientation}` : ""}`
								)}
								{tardosFixedByEdge.has(visual.edge.id)
									? ` · fix ${tardosFixedByEdge.get(visual.edge.id)?.bound === "lower" ? "L" : "U"}=${tardosFixedByEdge.get(visual.edge.id)?.value} · c̄ ${tardosFixedByEdge.get(visual.edge.id)?.reduced_cost}`
									: ""}
								{visual.fixed ? " · FIX" : ""}
								{visual.matched
									? context.model.kind === "assignment"
										? " · ASSIGN"
										: " · MATCH"
									: ""}
							</text>
							{polynomialPrimalScan !== undefined &&
								polynomialPrimalScan.target.kind !== "node" &&
								polynomialPrimalScan.target.edge_id === visual.edge.id && (
									<g
										className="flow-polynomial-scan-badge"
										data-polynomial-primal-scan={polynomialPrimalScan.ordinal}
										transform="translate(0 18)"
									>
										<rect x="-48" y="-7" width="96" height="14" rx="7" />
										<text textAnchor="middle" dominantBaseline="central">
											{polynomialPrimalScan.caption}
										</text>
									</g>
								)}
							{minimumMeanScan !== undefined &&
								minimumMeanScan.target.edge_id === visual.edge.id && (
									<g
										className="flow-minimum-mean-scan-badge"
										data-minimum-mean-scan={minimumMeanScan.ordinal}
										transform="translate(0 24)"
									>
										<rect x="-74" y="-7" width="148" height="14" rx="7" />
										<text textAnchor="middle" dominantBaseline="central">
											{minimumMeanScan.caption}
										</text>
									</g>
								)}
							{relaxationScan !== undefined &&
								relaxationScan.target.edge_id === visual.edge.id && (
									<g
										className="flow-relaxation-scan-badge"
										data-relaxation-scan={relaxationScan.ordinal}
										transform="translate(0 24)"
									>
										<rect x="-66" y="-7" width="132" height="14" rx="7" />
										<text textAnchor="middle" dominantBaseline="central">
											{relaxationScan.caption}
										</text>
									</g>
								)}
							{primitiveInspection !== undefined &&
								primitiveInspection.target.edge_id === visual.edge.id && (
									<g
										className="flow-primitive-inspection-badge"
										data-primitive-inspection={primitiveInspection.completed}
										data-primitive-inspection-total={primitiveInspection.total}
										transform="translate(0 24)"
									>
										<rect x="-66" y="-7" width="132" height="14" rx="7" />
										<text textAnchor="middle" dominantBaseline="central">
											{primitiveInspection.caption}
										</text>
									</g>
								)}
							{visual.convexState !== undefined && visual.capacity > 0n && (
								<FlowGraphOverlayOwnedLeaves
									state={state}
									bundle="original-edge-discrete-overlay"
									entity={{ kind: "edge", id: visual.edge.id }}
									owners={[
										{
											overlay: "convex_cost_overlay",
											role: "edges.segments",
										},
									]}
								>
									<g
										className="flow-convex-segment-rail"
										aria-label={`${visual.edge.id} convex-cost segments`}
									>
										{visual.convexState.segments.map((segment) => {
											const railWidth = visual.geometry.labelWidth + 142;
											const start = BigInt(segment.start_flow);
											const end = BigInt(segment.end_flow);
											const used = BigInt(segment.flow);
											const x =
												-railWidth / 2 +
												Number(
													(start * BigInt(railWidth * 1_000)) / visual.capacity,
												) /
													1_000;
											const width =
												Number(
													((end - start) * BigInt(railWidth * 1_000)) /
														visual.capacity,
												) / 1_000;
											const usedWidth =
												Number(
													(used * BigInt(railWidth * 1_000)) / visual.capacity,
												) / 1_000;
											const marginal = BigInt(segment.marginal_cost);
											const kind =
												marginal < 0n
													? "negative"
													: marginal > 0n
														? "positive"
														: "zero";
											const band = costMagnitudeBand(
												marginal,
												maximumConvexMarginalMagnitude,
											);
											return (
												<g
													key={`${visual.edge.id}:segment:${segment.segment}`}
													data-convex-segment={segment.segment}
													data-segment-flow={segment.flow}
													data-segment-marginal={segment.marginal_cost}
													data-convex-simplex-active={
														visual.convexSimplexState?.active_segment ===
															segment.segment || undefined
													}
													className={`flow-convex-segment flow-convex-segment-${kind} flow-convex-magnitude-${band}${visual.convexSimplexState?.active_segment === segment.segment ? " flow-convex-segment-active-simplex" : ""}`}
												>
													<title>{`segment ${segment.segment} · [${segment.start_flow}, ${segment.end_flow}] · used ${segment.flow} · marginal ${segment.marginal_cost}`}</title>
													<rect
														className="flow-convex-segment-capacity"
														x={x}
														y="10"
														width={Math.max(0.75, width)}
														height="7"
													/>
													<rect
														className="flow-convex-segment-used"
														x={x}
														y="10"
														width={Math.max(0, Math.min(width, usedWidth))}
														height="7"
													/>
												</g>
											);
										})}
									</g>
								</FlowGraphOverlayOwnedLeaves>
							)}
						</g>
					))}

			{viewMode !== "residual" &&
				originalVisuals
					.filter(
						(
							visual,
						): visual is typeof visual & {
							cycleAdjustment: "add" | "subtract";
						} => visual.cycleAdjustment !== undefined,
					)
					.map((visual) => (
						<text
							key={`cycle-sign:${visual.edge.id}`}
							x={visual.geometry.label.x}
							y={visual.geometry.label.y - 18}
							className={`flow-cycle-sign flow-cycle-sign-${visual.cycleAdjustment}${degenerateTransportationPivot ? " flow-cycle-sign-degenerate" : ""}`}
							data-cycle-theta={degenerateTransportationPivot ? "0" : undefined}
							textAnchor="middle"
						>
							{flowCycleAdjustmentLabel(
								visual.cycleAdjustment,
								context.traceEvent?.catalog_id,
							)}
						</text>
					))}

			{viewMode !== "residual" &&
				overlayViews.orlinMaxFlow?.residual_arcs.map((arc) => {
					const route = layout.routes.get(arc.edge_id);
					if (route === undefined) return null;
					const key = `${arc.edge_id}:${arc.direction}`;
					const active = orlinMaxActiveOriginalKeys.has(key);
					const sourceInspection = arc.inspection_serial !== undefined;
					const explicitlyFocused =
						arc.edge_id === hoveredEdgeId ||
						isOriginalEdgeSelected(selection, arc.edge_id);
					const renderClassification =
						plan.level === "detail" || active || sourceInspection;
					const renderClassificationLabel =
						sourceInspection ||
						explicitlyFocused ||
						(active && orlinMaxActiveOriginalKeys.size === 1);
					const className = arc.abundant
						? "abundant"
						: arc.anti_abundant
							? "anti-abundant"
							: arc.medium
								? "medium"
								: arc.small
									? "small"
									: undefined;
					if (
						!renderClassification ||
						(className === undefined && !active) ||
						(viewMode === "both" && !active)
					)
						return null;
					const path =
						arc.direction === "forward" ? route.path : route.reversePath;
					const symbol =
						className === "abundant"
							? "A"
							: className === "anti-abundant"
								? "Ā"
								: className === "medium"
									? "M"
									: className === "small"
										? "S"
										: "↯";
					const labelText = `${symbol}${arc.inspection_serial === undefined ? "" : ` #${arc.inspection_serial}`}`;
					const labelRoute = renderClassificationLabel
						? routeWithNodeClearLabel(route, positions, labelText)
						: route;
					return (
						<g
							key={`orlin-max-original:${key}`}
							className={`flow-orlin-max-original-class flow-orlin-max-original-${className ?? "active"}${active ? " flow-orlin-max-original-active" : ""}`}
							data-orlin-max-original={key}
							data-orlin-max-class={className}
							data-orlin-max-scan={arc.inspection_serial}
						>
							<title>{`${key} · residual ${arc.capacity} · ${className ?? "active lifted path"}${arc.inspection_serial === undefined ? "" : ` · source scan #${arc.inspection_serial}`}`}</title>
							<path
								d={path}
								strokeWidth={
									2 +
									Number(
										(BigInt(arc.capacity) * 4_000n) / maxResidualCapacity,
									) /
										1_000
								}
								markerEnd={flowScopedSvgUrl(
									idScope,
									"flow-arrow-residual-active",
								)}
							/>
							{renderClassificationLabel && (
								<g
									className="flow-orlin-max-original-label"
									data-orlin-max-label-owner={key}
									transform={`translate(${labelRoute.label.x} ${labelRoute.label.y + labelRoute.labelYOffset})`}
								>
									<FlowEdgeAnnotationConnector geometry={labelRoute} />
									<rect
										className="flow-orlin-max-original-label-bg"
										x={-labelRoute.labelBoxWidth / 2}
										y={-labelRoute.labelHeight / 2}
										width={labelRoute.labelBoxWidth}
										height={labelRoute.labelHeight}
										rx="4"
									/>
									<text textAnchor="middle" dominantBaseline="central">
										{labelText}
									</text>
								</g>
							)}
						</g>
					);
				})}

			{orlinMaxCompactVisuals
				.filter(
					(visual) =>
						plan.level === "detail" ||
						visual.activeReverse !== undefined ||
						visual.arc.inspection_serial !== undefined ||
						visual.arc.kind !== "original",
				)
				.map((visual) => {
					const active = visual.activeReverse !== undefined;
					const explicitlyFocused = visual.arc.witness.some(
						(reference) =>
							reference.edge_id === hoveredEdgeId ||
							isOriginalEdgeSelected(selection, reference.edge_id),
					);
					const renderLabel =
						visual.arc.inspection_serial !== undefined ||
						explicitlyFocused ||
						(active && orlinMaxActiveCompactCount === 1);
					const activePath = visual.activeReverse
						? visual.reversePath
						: visual.path;
					const symbol =
						visual.arc.kind === "original"
							? "O"
							: visual.arc.kind === "abundant-pseudo"
								? "P"
								: "T";
					return (
						<g
							key={`orlin-max-compact:${visual.arc.ordinal}`}
							className={`flow-orlin-max-compact flow-orlin-max-compact-${visual.arc.kind}${active ? " flow-orlin-max-compact-active" : ""}`}
							data-orlin-max-compact-ordinal={visual.arc.ordinal}
							data-orlin-max-compact-kind={visual.arc.kind}
							data-orlin-max-compact-capacity={visual.arc.capacity}
							data-orlin-max-compact-flow={visual.arc.flow}
							data-orlin-max-compact-active={active || undefined}
							data-orlin-max-scan={visual.arc.inspection_serial}
						>
							<title>{`compact ${visual.arc.ordinal} · ${visual.arc.from_component}→${visual.arc.to_component} · ${visual.arc.kind} · flow ${visual.arc.flow}/${visual.arc.capacity}${visual.arc.inspection_serial === undefined ? "" : ` · source scan #${visual.arc.inspection_serial}`} · witness ${visual.arc.witness.map((reference) => `${reference.edge_id}:${reference.direction}`).join(" → ")}`}</title>
							<path
								d={visual.path}
								className="flow-orlin-max-compact-capacity"
								strokeWidth={visual.capacityWidth}
								markerEnd={flowScopedSvgUrl(idScope, "flow-arrow-residual")}
							/>
							<path
								d={visual.path}
								className="flow-orlin-max-compact-flow"
								strokeWidth={visual.flowWidth}
							/>
							{active && (
								<path
									d={activePath}
									className="flow-orlin-max-compact-path"
									strokeWidth={visual.capacityWidth + 3}
									markerEnd={flowScopedSvgUrl(
										idScope,
										"flow-arrow-residual-active",
									)}
								/>
							)}
							{renderLabel && (
								<text
									x={visual.label.x}
									y={visual.label.y - 7}
									textAnchor="middle"
								>
									{`${symbol} ${visual.arc.flow}/${visual.arc.capacity}${visual.arc.inspection_serial === undefined ? "" : ` · #${visual.arc.inspection_serial}`}`}
								</text>
							)}
						</g>
					);
				})}

			{viewMode !== "residual" &&
				planarDual?.faces.map((face) => (
					<g
						key={`dual-face:${face.index}`}
						className={`flow-planar-dual-face flow-planar-dual-face-${face.role}${face.active ? " flow-planar-dual-face-active" : ""}`}
						data-planar-dual-face={face.index}
						transform={`translate(${face.x} ${face.y})`}
					>
						<title>{`${face.id}${face.distance === undefined ? "" : ` · dual distance ${face.distance}`}`}</title>
						<circle r={face.active ? 18 : 15} />
						<text textAnchor="middle" dominantBaseline="central">
							{face.id}
						</text>
						{face.distance !== undefined && (
							<text
								className="flow-planar-dual-distance"
								textAnchor="middle"
								y="29"
							>
								d={face.distance}
							</text>
						)}
					</g>
				))}
		</>
	);
}
