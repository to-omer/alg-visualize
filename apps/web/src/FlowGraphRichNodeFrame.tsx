import type { ReactNode } from "react";
import type { FlowGraphNodeKind } from "./FlowGraphSearchNodeFeatureBundle";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import type { FlowEntitySelection } from "./flow-entity-navigator";
import { formatFlowRational } from "./flow-parametric-view";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
type FlowGraphNode = FlowEntityGraphState["plan"]["nodes"][number];

export function FlowGraphRichNodeFrame({
	state,
	node,
	position,
	kind,
	selection,
	traceCalloutExpected,
	eventTouched,
	eventChanged,
	renderEventTouched,
	renderEventChanged,
	children,
}: Readonly<{
	state: FlowEntityGraphState;
	node: FlowGraphNode;
	position: { x: number; y: number };
	kind: FlowGraphNodeKind;
	selection: FlowEntitySelection | undefined;
	traceCalloutExpected: boolean;
	eventTouched: boolean;
	eventChanged: boolean;
	renderEventTouched: boolean;
	renderEventChanged: boolean;
	children: ReactNode;
}>) {
	const plan = state.plan;
	const forestChildIds = state.forestChildIds;
	const hasForestOverlay = state.hasForestOverlay;
	const parametricCut = state.visualization.parametricCut;
	const sourceSide = state.visualization.sourceSide;
	const infeasibleReachable = state.visualization.infeasibleReachable;
	const ibfsView = state.visualization.ibfsView;
	const eibfsView = state.visualization.eibfsView;
	const strongNodeIds = state.visualization.strongNodeIds;
	const transportation = state.visualization.features.transportation;
	const data = state.renderData;
	const overlayViews = data.overlayViews;
	const ibfsNode = ibfsView?.nodes.get(node.id);
	const eibfsNode = eibfsView?.nodes.get(node.id);
	const binaryNode = data.binaryNodeById.get(node.id);
	const cancelTightenNode = data.cancelTightenNodeById.get(node.id);
	const relaxedMndcNode = data.relaxedMndcNodeById.get(node.id);
	const relaxedMndcFamilyBand = data.relaxedMndcFamilyNodeBand.get(node.id);
	const enhancedScalingNode = data.enhancedScalingNodeById.get(node.id);
	const enhancedScalingComponent =
		enhancedScalingNode === undefined
			? undefined
			: data.enhancedScalingComponentById.get(enhancedScalingNode.component_id);
	const enhancedScalingRole =
		enhancedScalingNode === undefined
			? undefined
			: overlayViews.enhancedCapacityScaling?.source_component ===
					enhancedScalingNode.component_id
				? "source"
				: overlayViews.enhancedCapacityScaling?.sink_component ===
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
			: overlayViews.orlinMcf?.source_component === orlinMcfNode.component_id
				? "source"
				: overlayViews.orlinMcf?.sink_component === orlinMcfNode.component_id
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
	const electricalIpmMcfPotentialBand =
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
		overlayViews.doubleScaling?.selected_deficit === `node:${node.id}`
			? "deficit"
			: overlayViews.doubleScaling?.selected_root === `node:${node.id}`
				? "root"
				: undefined;
	const exactEventIdentity =
		state.context.traceEvent?.entity_refs.some(
			(entity) => entity.kind === "node" && entity.node_id === node.id,
		) === true;
	const exactChangedIdentity =
		state.context.traceEventSemantics?.changed_entity_refs.some(
			(entity) => entity.kind === "node" && entity.node_id === node.id,
		) === true;
	const excessScalingSelected =
		state.context.traceEvent?.catalog_id ===
			"excess-scaling-push-relabel.select-scaled-active" && exactEventIdentity;
	const binaryComponentBand =
		binaryNode === undefined
			? undefined
			: Number(BigInt(binaryNode.component) % 4n);
	const ibfsRepairFocus = ibfsView?.repairFocusNodeIds.has(node.id) === true;
	const eibfsRepairFocus = eibfsView?.repairFocusNodeIds.has(node.id) === true;
	const selectedNode = selection?.kind === "node" && selection.id === node.id;
	const renderOrlinMaxRole =
		orlinMaxNode !== undefined &&
		(plan.level === "detail" ||
			renderEventTouched ||
			renderEventChanged ||
			selectedNode);
	return (
		<g
			data-node-id={node.id}
			data-trace-callout-expected={traceCalloutExpected || undefined}
			data-event-touch={eventTouched || undefined}
			data-event-change={eventChanged || undefined}
			data-event-identities={exactEventIdentity ? `node:${node.id}` : undefined}
			data-excess-scaling-selected={excessScalingSelected || undefined}
			data-changed-identities={
				exactChangedIdentity ? `node:${node.id}` : undefined
			}
			data-overlay-marks={plan.overlayPresentation.nodeMarksById
				.get(node.id)
				?.map(({ overlay, role }) => `${overlay}:${role}`)
				.join("|")}
			data-ibfs-side={ibfsNode?.side}
			data-ibfs-distance={ibfsNode?.distance}
			data-ibfs-orphan={ibfsNode?.orphan || undefined}
			data-ibfs-frontier={ibfsNode?.frontier || undefined}
			data-ibfs-repair-focus={ibfsRepairFocus || undefined}
			data-eibfs-membership={eibfsNode?.membership}
			data-eibfs-source-label={eibfsNode?.source_label}
			data-eibfs-sink-label={eibfsNode?.sink_label}
			data-eibfs-root-kind={eibfsNode?.root_kind}
			data-eibfs-imbalance={eibfsNode?.imbalance}
			data-eibfs-orphan={eibfsNode?.orphan || undefined}
			data-eibfs-frontier={eibfsNode?.frontier || undefined}
			data-eibfs-repair-focus={eibfsRepairFocus || undefined}
			data-binary-distance={binaryNode?.distance ?? "unreachable"}
			data-binary-component={binaryNode?.component}
			data-tardos-potential={tardosNode?.potential}
			data-cancel-tighten-potential={
				cancelTightenNode === undefined
					? undefined
					: formatFlowRational(cancelTightenNode.potential)
			}
			data-cancel-tighten-rank={cancelTightenNode?.rank}
			data-relaxed-mndc-left-dual={relaxedMndcNode?.left_dual}
			data-relaxed-mndc-right-dual={relaxedMndcNode?.right_dual}
			data-relaxed-mndc-match={relaxedMndcNode?.matched_node_id}
			data-relaxed-mndc-cycle={relaxedMndcFamilyBand}
			data-enhanced-scaling-component={enhancedScalingNode?.component_id}
			data-enhanced-scaling-excess={
				enhancedScalingComponent === undefined
					? undefined
					: formatFlowRational(enhancedScalingComponent.excess)
			}
			data-enhanced-scaling-potential={enhancedScalingNode?.potential}
			data-enhanced-scaling-distance={enhancedScalingNode?.distance}
			data-enhanced-scaling-role={enhancedScalingRole}
			data-orlin-mcf-component={orlinMcfNode?.component_id}
			data-orlin-mcf-excess={
				orlinMcfComponent === undefined
					? undefined
					: formatFlowRational(orlinMcfComponent.excess)
			}
			data-orlin-mcf-potential={orlinMcfNode?.potential}
			data-orlin-mcf-distance={orlinMcfNode?.distance}
			data-orlin-mcf-role={orlinMcfRole}
			data-orlin-max-component={orlinMaxNode?.component_id}
			data-orlin-max-critical={orlinMaxNode?.critical || undefined}
			data-orlin-max-phi={orlinMaxNode?.anti_potential}
			data-orlin-max-side={
				orlinMaxNode === undefined
					? undefined
					: orlinMaxNode.source_side
						? "source"
						: "sink"
			}
			data-electrical-potential={electricalNode?.potential}
			data-electrical-residual={electricalNode?.residual}
			data-electrical-search-direction={electricalNode?.search_direction}
			data-electrical-grounded={electricalNode?.grounded || undefined}
			data-electrical-potential-band={electricalNodePotentialBand}
			data-augmenting-potential={augmentingElectricalNode?.potential}
			data-augmenting-coupling={augmentingElectricalNode?.coupling_violation}
			data-augmenting-target-side={
				augmentingElectricalNode === undefined
					? undefined
					: augmentingElectricalNode.target_source_side
						? "source"
						: "sink"
			}
			data-augmenting-potential-band={augmentingElectricalPotentialBand}
			data-ipm-potential={interiorPointNode?.potential}
			data-ipm-target-side={
				interiorPointNode === undefined
					? undefined
					: interiorPointNode.target_source_side
						? "source"
						: "sink"
			}
			data-ipm-potential-band={interiorPointNodePotentialBand}
			data-eipm-mcf-potential={electricalIpmMcfNode?.potential}
			data-eipm-mcf-potential-direction={
				electricalIpmMcfNode?.potential_direction
			}
			data-eipm-mcf-balance-residual={electricalIpmMcfNode?.balance_residual}
			data-eipm-mcf-anchored={electricalIpmMcfNode?.anchored || undefined}
			data-eipm-mcf-potential-band={electricalIpmMcfPotentialBand}
			data-min-ratio-component={minimumRatioNode?.component}
			data-min-ratio-parent={minimumRatioNode?.parent_node_id}
			data-min-ratio-depth={minimumRatioNode?.depth}
			data-min-ratio-balance={minimumRatioNode?.candidate_balance}
			data-min-ratio-candidate={minimumRatioNode?.on_candidate || undefined}
			data-min-ratio-selected={minimumRatioNode?.on_selected || undefined}
			data-randomized-tree-component={
				randomizedAlmostLinearNode?.tree_component
			}
			data-randomized-tree-parent={
				randomizedAlmostLinearNode?.tree_parent_node_id
			}
			data-randomized-source-side={
				randomizedAlmostLinearNode === undefined
					? undefined
					: randomizedAlmostLinearNode.source_side
						? "source"
						: "sink"
			}
			data-randomized-artificial-direction={
				randomizedAlmostLinearNode?.artificial_direction
			}
			data-randomized-artificial-flow={
				randomizedAlmostLinearNode?.artificial_flow
			}
			data-randomized-artificial-capacity={
				randomizedAlmostLinearNode?.artificial_capacity
			}
			data-randomized-artificial-active-sign={
				randomizedAlmostLinearNode?.active_artificial_sign
			}
			data-deterministic-forest-component={
				deterministicAlmostLinearNode?.forest_component
			}
			data-deterministic-tree-parent={
				deterministicAlmostLinearNode?.tree_parent_node_id
			}
			data-deterministic-source-side={
				deterministicAlmostLinearNode === undefined
					? undefined
					: deterministicAlmostLinearNode.source_side
						? "source"
						: "sink"
			}
			data-deterministic-artificial-direction={
				deterministicAlmostLinearNode?.artificial_direction
			}
			data-deterministic-artificial-flow={
				deterministicAlmostLinearNode?.artificial_flow
			}
			data-deterministic-artificial-capacity={
				deterministicAlmostLinearNode?.artificial_capacity
			}
			data-deterministic-artificial-tree-mask={
				deterministicAlmostLinearNode?.artificial_tree_level_mask
			}
			data-deterministic-artificial-active-sign={
				deterministicAlmostLinearNode?.active_artificial_sign
			}
			data-dual-simplex-potential={
				dualSimplexNode?.initialized ? dualSimplexNode.potential : undefined
			}
			data-dual-simplex-initialized={dualSimplexNode?.initialized || undefined}
			data-dual-simplex-cut={dualSimplexNode?.in_cut || undefined}
			data-polynomial-dual-potential={polynomialDualNode?.potential}
			data-polynomial-dual-excess={
				polynomialDualNode === undefined
					? undefined
					: formatFlowRational(polynomialDualNode.excess)
			}
			data-polynomial-dual-root={polynomialDualNode?.root || undefined}
			data-polynomial-dual-active={polynomialDualNode?.active || undefined}
			data-polynomial-dual-bad={polynomialDualNode?.bad || undefined}
			data-polynomial-dual-pivot-cut={
				polynomialDualNode?.in_pivot_cut || undefined
			}
			data-polynomial-primal-premultiplier={
				polynomialPrimalNode === undefined
					? undefined
					: formatFlowRational(polynomialPrimalNode.premultiplier)
			}
			data-polynomial-primal-eligible={polynomialPrimalEligible || undefined}
			data-polynomial-primal-awake={polynomialPrimalAwake || undefined}
			data-polynomial-primal-n-star={polynomialPrimalNStar || undefined}
			data-polynomial-primal-root={polynomialPrimalRoot || undefined}
			data-double-scaling-price={doubleScalingNode?.price}
			data-double-scaling-imbalance={doubleScalingNode?.imbalance}
			data-double-scaling-cursor={doubleScalingNode?.cursor}
			data-double-scaling-role={doubleScalingRole}
			data-convex-simplex-potential={convexSimplexNode?.potential}
			data-convex-simplex-parent={convexSimplexNode?.parent}
			data-prediction-raw-price={predictionNode?.raw_predicted_price}
			data-prediction-price={predictionNode?.predicted_price}
			data-prediction-clipped={predictionNode?.prediction_clipped || undefined}
			data-prediction-current-price={predictionNode?.price}
			data-prediction-surplus={predictionNode?.surplus}
			data-prediction-active={predictionNode?.active || undefined}
			data-parametric-cut-tie={parametricCut.tie.has(node.id) || undefined}
			className={`flow-node-frame flow-node-frame-${kind}${sourceSide.has(node.id) ? " flow-node-source-side" : ""}${parametricCut.tie.has(node.id) ? " flow-node-parametric-tie" : ""}${infeasibleReachable.has(node.id) ? " flow-node-infeasible-reachable" : ""}${strongNodeIds.has(node.id) ? " flow-node-strong-branch" : ""}${hasForestOverlay && (kind === "normal" || transportation) && !forestChildIds.has(node.id) ? " flow-node-branch-root" : ""}${ibfsNode?.side === "source" ? " flow-node-ibfs-source-tree" : ibfsNode?.side === "sink" ? " flow-node-ibfs-sink-tree" : ""}${ibfsNode?.orphan ? " flow-node-ibfs-orphan" : ""}${ibfsNode?.frontier ? " flow-node-ibfs-frontier" : ""}${ibfsRepairFocus ? " flow-node-ibfs-repair-focus" : ""}${eibfsNode?.membership === "source" ? " flow-node-eibfs-source-forest" : eibfsNode?.membership === "sink" ? " flow-node-eibfs-sink-forest" : eibfsNode !== undefined ? " flow-node-eibfs-free" : ""}${eibfsNode !== undefined && eibfsNode.root_kind !== "none" ? " flow-node-eibfs-root" : ""}${eibfsNode?.orphan ? " flow-node-eibfs-orphan" : ""}${eibfsNode?.frontier ? " flow-node-eibfs-frontier" : ""}${eibfsRepairFocus ? " flow-node-eibfs-repair-focus" : ""}${binaryComponentBand === undefined ? "" : ` flow-node-binary-component flow-node-binary-component-${binaryComponentBand}`}${relaxedMndcFamilyBand === undefined ? "" : ` flow-node-mndc-family flow-node-mndc-family-${relaxedMndcFamilyBand % 4}`}${enhancedScalingNode === undefined ? "" : " flow-node-enhanced-component"}${enhancedScalingRole === undefined ? "" : ` flow-node-enhanced-${enhancedScalingRole}`}${orlinMcfNode === undefined ? "" : " flow-node-orlin-component"}${orlinMcfRole === undefined ? "" : ` flow-node-orlin-${orlinMcfRole}`}${!renderOrlinMaxRole ? "" : ` flow-node-orlin-max${orlinMaxNode.critical ? " flow-node-orlin-max-critical" : " flow-node-orlin-max-compactible"}${orlinMaxNode.source_side ? " flow-node-orlin-max-source-side" : ""}`}${electricalNode === undefined ? "" : ` flow-node-electrical flow-node-electrical-potential-${electricalNodePotentialBand}${electricalNode.grounded ? " flow-node-electrical-grounded" : ""}${Number(electricalNode.residual) === 0 ? "" : " flow-node-electrical-residual"}`}${interiorPointNode === undefined ? "" : ` flow-node-interior-point flow-node-interior-potential-${interiorPointNodePotentialBand} flow-node-interior-target-${interiorPointNode.target_source_side ? "source" : "sink"}`}${minimumRatioNode === undefined ? "" : ` flow-node-minimum-ratio flow-node-minimum-ratio-component-${Number(BigInt(minimumRatioNode.component) % 5n)}${minimumRatioNode.on_candidate ? " flow-node-minimum-ratio-candidate" : ""}${minimumRatioNode.on_selected ? " flow-node-minimum-ratio-selected" : ""}`}${deterministicAlmostLinearNode === undefined ? "" : ` flow-node-deterministic flow-node-deterministic-component-${Number(BigInt(deterministicAlmostLinearNode.forest_component) % 5n)} flow-node-deterministic-cut-${deterministicAlmostLinearNode.source_side ? "source" : "sink"}${deterministicAlmostLinearNode.artificial_direction === "0" ? "" : " flow-node-deterministic-artificial"}${deterministicAlmostLinearNode.active_artificial_sign === "0" ? "" : " flow-node-deterministic-cycle"}`}${dualSimplexNode?.in_cut ? " flow-node-dual-cut" : ""}${polynomialDualNode?.root ? " flow-node-polynomial-dual-root" : ""}${polynomialDualNode?.active ? " flow-node-polynomial-dual-active" : ""}${polynomialDualNode?.bad ? " flow-node-polynomial-dual-bad" : ""}${polynomialDualNode?.in_pivot_cut ? " flow-node-polynomial-dual-cut" : ""}${polynomialPrimalEligible ? " flow-node-polynomial-eligible" : ""}${polynomialPrimalAwake ? " flow-node-polynomial-awake" : ""}${polynomialPrimalNStar ? " flow-node-polynomial-n-star" : ""}${polynomialPrimalRoot ? " flow-node-polynomial-root" : ""}${doubleScalingRole === "root" ? " flow-node-double-root" : doubleScalingRole === "deficit" ? " flow-node-double-deficit" : ""}${predictionNode === undefined ? "" : " flow-node-prediction"}${predictionNode?.prediction_clipped ? " flow-node-prediction-clipped" : ""}${predictionNode?.active ? " flow-node-prediction-active" : ""}${selectedNode ? " flow-entity-selected" : ""}`}
			transform={`translate(${position.x} ${position.y})`}
		>
			{children}
			{renderOrlinMaxRole && (
				<circle
					className="flow-orlin-max-node-state-mark"
					data-overlay-contribution="orlin_max_flow_overlay"
					data-overlay-feature-bundle="node-optimization"
					data-overlay-entity-kind="node"
					data-overlay-entity-id={node.id}
					data-overlay-role={
						orlinMaxNode.critical
							? "critical-component-member"
							: "compactible-component-member"
					}
					data-overlay-source-side={orlinMaxNode.source_side || undefined}
					cx="27"
					cy="-27"
					r="4"
				>
					<title>{`${node.id}: component ${orlinMaxNode.component_id}, ${orlinMaxNode.critical ? "critical" : "compactible"}, anti-potential ${orlinMaxNode.anti_potential}${orlinMaxNode.source_side ? ", source side" : ", sink side"}`}</title>
				</circle>
			)}
			{excessScalingSelected && (
				<path
					className="flow-excess-scaling-selected-node"
					d="M -39 -27 V -39 H -27 M 27 -39 H 39 V -27 M 39 27 V 39 H 27 M -27 39 H -39 V 27"
				/>
			)}
			{renderEventChanged && (
				<circle r="44" className="flow-event-change-node-ring" />
			)}
			{renderEventTouched && (
				<circle r="48" className="flow-event-touch-node-ring" />
			)}
		</g>
	);
}
