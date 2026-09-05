import {
	FlowGraphOverlayOwnedLeaves,
	type FlowOverlayLeafOwner,
} from "./FlowGraphOverlayOwnedLeaves";
import { projectFlowCostScalingRefineBoundary } from "./flow-cost-scaling-refine";
import { flowScopedSvgUrl, useFlowGraphIdScope } from "./flow-dom-id";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import type { FlowEntitySelection } from "./flow-entity-navigator";
import {
	flowMinimumMeanResidualScan,
	flowPrimitiveArcInspection,
	flowRelaxationArcScan,
	ordinaryFlowEventEntityRefs,
	shouldRenderFlowEventEntityEmphasis,
} from "./flow-event-highlight";
import { isResidualArcSelected } from "./flow-graph-entity-selection";
import { FLOW_LOD_LIMITS } from "./flow-render-plan";
import { costMagnitudeBand } from "./flow-visual-scales";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
type FlowGraphLayerProps = Readonly<{
	state: FlowEntityGraphState;
	selection: FlowEntitySelection | undefined;
	hoveredEdgeId: string | undefined;
}>;

export function FlowGraphResidualLayer({
	state,
	selection,
	hoveredEdgeId,
}: FlowGraphLayerProps) {
	const idScope = useFlowGraphIdScope();
	const plan = state.plan;
	const viewMode = state.viewMode;
	const layout = state.layout;
	const visibleResidualArcs = state.visibleResidualArcs;
	const maxResidualCapacity = state.maxResidualCapacity;
	const ibfsView = state.visualization.ibfsView;
	const eibfsView = state.visualization.eibfsView;
	const forestArcKeys = state.visualization.forestArcKeys;
	const distanceDirected = state.visualization.features.distanceDirected;
	const rootwardForest = state.visualization.features.rootwardForest;
	const _convexCost = state.visualization.features.convexCost;
	const _enhancedCapacityScaling =
		state.visualization.features.enhancedCapacityScaling;
	const _tardosFramework = state.visualization.features.tardosFramework;
	const overlayViews = state.renderData.overlayViews;
	const binaryAdmissibleArcKeys = state.renderData.binaryAdmissibleArcKeys;
	const binaryBaseZeroArcKeys = state.renderData.binaryBaseZeroArcKeys;
	const binarySpecialArcKeys = state.renderData.binarySpecialArcKeys;
	const binaryZeroAdmissibleArcKeys =
		state.renderData.binaryZeroAdmissibleArcKeys;
	const cancelTightenAdmissibleArcKeys =
		state.renderData.cancelTightenAdmissibleArcKeys;
	const cancelTightenCycleArcKeys = state.renderData.cancelTightenCycleArcKeys;
	const cancelTightenInspectedArcKeys =
		state.renderData.cancelTightenInspectedArcKeys;
	const convexActiveDirectionsByEdge =
		state.renderData.convexActiveDirectionsByEdge;
	const convexEligibleDirectionsByEdge =
		state.renderData.convexEligibleDirectionsByEdge;
	const doubleScalingActiveArcKeys =
		state.renderData.doubleScalingActiveArcKeys;
	const doubleScalingAdmissibleArcKeys =
		state.renderData.doubleScalingAdmissibleArcKeys;
	const doubleScalingInspectedArc = state.renderData.doubleScalingInspectedArc;
	const enhancedScalingPathArcKeys =
		state.renderData.enhancedScalingPathArcKeys;
	const maximumTardosReducedCost = state.renderData.maximumTardosReducedCost;
	const orlinMaxResidualByKey = state.renderData.orlinMaxResidualByKey;
	const predictionActiveArcKey = state.renderData.predictionActiveArcKey;
	const relaxedMndcAssignmentArcKeys =
		state.renderData.relaxedMndcAssignmentArcKeys;
	const relaxedMndcInspectedArcKeys =
		state.renderData.relaxedMndcInspectedArcKeys;
	const relaxedMndcCycleByArc = state.renderData.relaxedMndcCycleByArc;
	const tardosResidualByArc = state.renderData.tardosResidualByArc;
	const minimumMeanScan = flowMinimumMeanResidualScan(state.context);
	const relaxationScan = flowRelaxationArcScan(state.context);
	const primitiveInspection =
		minimumMeanScan === undefined && relaxationScan === undefined
			? flowPrimitiveArcInspection(state.context)
			: undefined;
	const costScalingRefine = projectFlowCostScalingRefineBoundary(
		state.context.traceEvent,
		state.context.residualArcs,
		state.visualization.nodeTraceStates,
		plan.nodes.length,
	);
	const touchedResidualArcKeys = new Set(
		ordinaryFlowEventEntityRefs(state.context).flatMap((entity) =>
			entity.kind === "residual-arc"
				? [`${entity.edge_id}:${entity.direction}`]
				: [],
		),
	);
	const changedResidualArcKeys = new Set(
		state.context.traceEventSemantics?.changed_entity_refs.flatMap((entity) =>
			entity.kind === "residual-arc"
				? [`${entity.edge_id}:${entity.direction}`]
				: [],
		) ?? [],
	);
	const emphasizeTouchedResidualArcs = shouldRenderFlowEventEntityEmphasis({
		level: plan.level,
		kind: "edge",
		signal: "touch",
		memberCount: touchedResidualArcKeys.size,
		totalCount: visibleResidualArcs.length,
		structureLimit: FLOW_LOD_LIMITS.structureEdgeEventLabels,
	});
	const emphasizeChangedResidualArcs = shouldRenderFlowEventEntityEmphasis({
		level: plan.level,
		kind: "edge",
		signal: "change",
		memberCount: changedResidualArcKeys.size,
		totalCount: visibleResidualArcs.length,
		structureLimit: FLOW_LOD_LIMITS.structureEdgeEventLabels,
	});
	return (
		<>
			{(viewMode !== "original" ||
				touchedResidualArcKeys.size > 0 ||
				costScalingRefine !== undefined) &&
				visibleResidualArcs.map((arc) => {
					const route = layout.routes.get(arc.edge_id);
					if (route === undefined) return null;
					const path =
						arc.direction === "forward" ? route.path : route.reversePath;
					const label =
						arc.direction === "forward"
							? route.residualForwardLabel
							: route.residualReverseLabel;
					const capacity = BigInt(arc.capacity);
					const width =
						2 + Number((capacity * 3_000n) / maxResidualCapacity) / 1_000;
					const forestKey = `${arc.edge_id}:${arc.direction}`;
					const costRefineArc = costScalingRefine?.arcs.get(forestKey);
					const minimumMeanScanForArc =
						minimumMeanScan?.target.edge_id === arc.edge_id &&
						minimumMeanScan.target.direction === arc.direction
							? minimumMeanScan
							: undefined;
					const relaxationScanForArc =
						relaxationScan?.target.edge_id === arc.edge_id &&
						(relaxationScan.target.kind === "edge"
							? arc.direction === "forward"
							: relaxationScan.target.direction === arc.direction)
							? relaxationScan
							: undefined;
					const primitiveInspectionForArc =
						primitiveInspection?.target.edge_id === arc.edge_id &&
						(primitiveInspection.target.kind === "edge"
							? arc.direction === "forward"
							: primitiveInspection.target.direction === arc.direction)
							? primitiveInspection
							: undefined;
					const orlinMaxResidual = orlinMaxResidualByKey.get(forestKey);
					const orlinMaxClass =
						orlinMaxResidual?.abundant === true
							? "abundant"
							: orlinMaxResidual?.anti_abundant === true
								? "anti-abundant"
								: orlinMaxResidual?.medium === true
									? "medium"
									: orlinMaxResidual?.small === true
										? "small"
										: undefined;
					const renderOrlinMaxClass =
						orlinMaxClass !== undefined &&
						(plan.level === "detail" ||
							orlinMaxResidual?.inspection_serial !== undefined ||
							state.renderData.orlinMaxActiveOriginalKeys.has(forestKey));
					const tardosResidual = tardosResidualByArc.get(forestKey);
					const tardosReducedCost =
						tardosResidual === undefined
							? undefined
							: BigInt(tardosResidual.reduced_cost);
					const tardosCostKind =
						tardosReducedCost === undefined
							? undefined
							: tardosReducedCost < 0n
								? "negative"
								: tardosReducedCost > 0n
									? "positive"
									: "zero";
					const tardosMagnitude =
						tardosReducedCost === undefined
							? undefined
							: costMagnitudeBand(tardosReducedCost, maximumTardosReducedCost);
					const forest = forestArcKeys.has(forestKey);
					const ibfsSourceTree =
						ibfsView?.sourceForestArcKeys.has(forestKey) === true;
					const ibfsSinkTree =
						ibfsView?.sinkForestArcKeys.has(forestKey) === true;
					const eibfsSourceTree =
						eibfsView?.sourceForestArcKeys.has(forestKey) === true;
					const eibfsSinkTree =
						eibfsView?.sinkForestArcKeys.has(forestKey) === true;
					const binaryBaseZero = binaryBaseZeroArcKeys.has(forestKey);
					const binarySpecial = binarySpecialArcKeys.has(forestKey);
					const binaryAdmissible = binaryAdmissibleArcKeys.has(forestKey);
					const binaryZeroAdmissible =
						binaryZeroAdmissibleArcKeys.has(forestKey);
					const binaryInspected =
						overlayViews.binaryBlocking?.stage === "analyzing" &&
						touchedResidualArcKeys.has(forestKey);
					const activeWorkingArc =
						arc.active && overlayViews.binaryBlocking?.stage !== "complete";
					const cancelTightenAdmissible =
						cancelTightenAdmissibleArcKeys.has(forestKey);
					const cancelTightenCycle = cancelTightenCycleArcKeys.has(forestKey);
					const cancelTightenInspected =
						cancelTightenInspectedArcKeys.has(forestKey);
					const relaxedMndcAssignment =
						relaxedMndcAssignmentArcKeys.has(forestKey);
					const relaxedMndcInspected =
						relaxedMndcInspectedArcKeys.has(forestKey);
					const relaxedMndcCycle = relaxedMndcCycleByArc.get(forestKey);
					const enhancedScalingPath = enhancedScalingPathArcKeys.has(forestKey);
					const enhancedScalingContract =
						arc.direction === "forward" &&
						overlayViews.enhancedCapacityScaling?.contraction_arc ===
							arc.edge_id;
					const doubleScalingAdmissible = doubleScalingAdmissibleArcKeys.has(
						`${arc.edge_id}:flow:${arc.direction}`,
					);
					const doubleScalingActive = doubleScalingActiveArcKeys.has(
						`${arc.edge_id}:flow:${arc.direction}`,
					);
					const doubleScalingInspectedBranch =
						doubleScalingInspectedArc?.edge_id === arc.edge_id &&
						doubleScalingInspectedArc.direction === arc.direction
							? doubleScalingInspectedArc.branch
							: undefined;
					const convexActive =
						convexActiveDirectionsByEdge
							.get(arc.edge_id)
							?.has(arc.direction) === true;
					const convexEligible =
						convexEligibleDirectionsByEdge
							.get(arc.edge_id)
							?.has(arc.direction) === true;
					const predictionActive =
						predictionActiveArcKey === `${arc.edge_id}:${arc.direction}`;
					const visible =
						capacity > 0n ||
						activeWorkingArc ||
						forest ||
						eibfsSourceTree ||
						eibfsSinkTree ||
						binaryBaseZero ||
						binarySpecial ||
						binaryAdmissible ||
						binaryInspected ||
						cancelTightenAdmissible ||
						cancelTightenCycle ||
						cancelTightenInspected ||
						relaxedMndcAssignment ||
						relaxedMndcInspected ||
						relaxedMndcCycle !== undefined ||
						enhancedScalingPath ||
						enhancedScalingContract ||
						doubleScalingAdmissible ||
						doubleScalingActive ||
						doubleScalingInspectedBranch !== undefined ||
						convexEligible ||
						convexActive ||
						predictionActive ||
						costRefineArc !== undefined ||
						minimumMeanScanForArc !== undefined ||
						relaxationScanForArc !== undefined ||
						primitiveInspectionForArc !== undefined ||
						tardosResidual !== undefined ||
						renderOrlinMaxClass ||
						arc.fixed;
					const selected = isResidualArcSelected(
						selection,
						arc.edge_id,
						arc.direction,
					);
					const rawTouched = touchedResidualArcKeys.has(forestKey);
					const rawChanged = changedResidualArcKeys.has(forestKey);
					const touched = emphasizeTouchedResidualArcs && rawTouched;
					const changed = emphasizeChangedResidualArcs && rawChanged;
					if (
						viewMode === "original" &&
						!touched &&
						costRefineArc === undefined
					) {
						return null;
					}
					const originalSelected =
						selection?.kind === "edge" && selection.id === arc.edge_id;
					const algorithmFocused =
						activeWorkingArc ||
						forest ||
						ibfsSourceTree ||
						ibfsSinkTree ||
						eibfsSourceTree ||
						eibfsSinkTree ||
						binaryBaseZero ||
						binarySpecial ||
						binaryAdmissible ||
						binaryZeroAdmissible ||
						binaryInspected ||
						cancelTightenAdmissible ||
						cancelTightenCycle ||
						cancelTightenInspected ||
						relaxedMndcAssignment ||
						relaxedMndcInspected ||
						relaxedMndcCycle !== undefined ||
						enhancedScalingPath ||
						enhancedScalingContract ||
						doubleScalingAdmissible ||
						doubleScalingActive ||
						doubleScalingInspectedBranch !== undefined ||
						convexEligible ||
						convexActive ||
						predictionActive ||
						costRefineArc !== undefined ||
						minimumMeanScanForArc !== undefined ||
						relaxationScanForArc !== undefined ||
						primitiveInspectionForArc !== undefined ||
						tardosResidual !== undefined ||
						renderOrlinMaxClass ||
						arc.fixed;
					const focused =
						algorithmFocused ||
						touched ||
						selected ||
						originalSelected ||
						hoveredEdgeId === arc.edge_id;
					if ((!visible && !focused) || (viewMode === "both" && !focused)) {
						return null;
					}
					const expanded =
						focused || (viewMode === "residual" && plan.level === "detail");
					const overlayOwners: FlowOverlayLeafOwner[] = [
						...(binaryBaseZero ||
						binarySpecial ||
						binaryAdmissible ||
						binaryZeroAdmissible ||
						binaryInspected
							? [
									{
										overlay: "binary_blocking_overlay" as const,
										role: binaryInspected
											? "trace_event.inspected-residual-arc"
											: "residual_arcs.binary-length-state",
									},
								]
							: []),
						...(cancelTightenAdmissible ||
						cancelTightenCycle ||
						cancelTightenInspected
							? [
									{
										overlay: "cancel_tighten_overlay" as const,
										role: cancelTightenInspected
											? "inspected_arcs"
											: cancelTightenCycle
												? "active_cycle"
												: "admissible_arcs",
									},
								]
							: []),
						...(relaxedMndcAssignment ||
						relaxedMndcInspected ||
						relaxedMndcCycle !== undefined
							? [
									{
										overlay: "relaxed_mndc_overlay" as const,
										role: relaxedMndcInspected
											? "inspected_arcs"
											: relaxedMndcCycle === undefined
												? "nodes.selected_arc"
												: "family.arcs",
									},
								]
							: []),
						...(enhancedScalingPath || enhancedScalingContract
							? [
									{
										overlay: "enhanced_capacity_scaling_overlay" as const,
										role: enhancedScalingContract ? "contraction_arc" : "path",
									},
								]
							: []),
						...(renderOrlinMaxClass
							? [
									{
										overlay: "orlin_max_flow_overlay" as const,
										role: "residual_arcs.capacity-class",
									},
								]
							: []),
						...(doubleScalingAdmissible ||
						doubleScalingActive ||
						doubleScalingInspectedBranch !== undefined
							? [
									{
										overlay: "double_scaling_overlay" as const,
										role: "residual_arcs.transformed-flow-branch",
									},
								]
							: []),
						...(convexEligible || convexActive
							? [
									{
										overlay: "convex_cost_overlay" as const,
										role: convexActive ? "active_cycle" : "eligible_arcs",
									},
								]
							: []),
						...(predictionActive
							? [
									{
										overlay: "prediction_assisted_epsilon_overlay" as const,
										role: "active_arc",
									},
								]
							: []),
						...(tardosResidual === undefined
							? []
							: [
									{
										overlay: "tardos_framework_overlay" as const,
										role: "residual_arcs.reduced-cost-class",
									},
								]),
					];
					return (
						<g
							key={`${arc.edge_id}:${arc.direction}`}
							data-edge-id={arc.edge_id}
							data-residual-direction={arc.direction}
							data-event-touch={rawTouched || undefined}
							data-event-change={rawChanged || undefined}
							data-event-identities={
								touched
									? `residual-arc:${arc.edge_id}:${arc.direction}`
									: undefined
							}
							data-changed-identities={
								changed
									? `residual-arc:${arc.edge_id}:${arc.direction}`
									: undefined
							}
							data-edge-detail={expanded ? "expanded" : "context"}
							className={`flow-residual-arc flow-residual-${arc.direction} ${expanded ? "flow-residual-expanded" : "flow-residual-context"}${forest ? " flow-residual-forest" : ""}${ibfsSourceTree ? " flow-residual-ibfs-source-tree" : ""}${ibfsSinkTree ? " flow-residual-ibfs-sink-tree" : ""}${eibfsSourceTree ? " flow-residual-eibfs-source-tree" : ""}${eibfsSinkTree ? " flow-residual-eibfs-sink-tree" : ""}${binaryBaseZero ? " flow-residual-binary-base-zero" : ""}${binarySpecial ? " flow-residual-binary-special" : ""}${binaryAdmissible ? " flow-residual-binary-admissible" : ""}${binaryZeroAdmissible ? " flow-residual-binary-zero-admissible" : ""}${binaryInspected ? " flow-residual-binary-inspected" : ""}${cancelTightenAdmissible ? " flow-residual-cancel-admissible" : ""}${cancelTightenCycle ? " flow-residual-cancel-cycle" : ""}${cancelTightenInspected ? " flow-residual-cancel-inspected" : ""}${relaxedMndcAssignment ? " flow-residual-mndc-assignment" : ""}${relaxedMndcInspected ? " flow-residual-mndc-inspected" : ""}${relaxedMndcCycle === undefined ? "" : ` flow-residual-mndc-cycle flow-residual-mndc-cycle-${relaxedMndcCycle % 4}`}${enhancedScalingPath ? " flow-residual-enhanced-path" : ""}${enhancedScalingContract ? " flow-residual-enhanced-contract" : ""}${!renderOrlinMaxClass ? "" : ` flow-residual-orlin-max flow-residual-orlin-max-${orlinMaxClass}`}${doubleScalingAdmissible ? " flow-residual-double-admissible" : ""}${doubleScalingActive ? " flow-residual-double-active" : ""}${doubleScalingInspectedBranch === undefined ? "" : ` flow-residual-double-inspected flow-residual-double-inspected-${doubleScalingInspectedBranch}`}${convexEligible ? " flow-residual-convex-eligible" : ""}${convexActive ? ` flow-residual-convex-active flow-residual-convex-active-${arc.direction}` : ""}${predictionActive ? " flow-residual-prediction-active" : ""}${costRefineArc === undefined ? "" : ` flow-residual-cost-refine flow-residual-cost-refine-${costRefineArc.className}`}${tardosResidual === undefined ? "" : ` flow-residual-tardos flow-residual-tardos-${tardosCostKind} flow-tardos-magnitude-${tardosMagnitude}${tardosResidual.fixes_variable ? " flow-residual-tardos-fixes" : ""}`}${arc.fixed ? " flow-residual-fixed" : ""}${activeWorkingArc ? " flow-residual-active" : ""}${selected ? " flow-entity-selected" : ""}`}
							data-cost-refine-class={costRefineArc?.className}
							data-cost-refine-epsilon={costScalingRefine?.epsilon.toString()}
							data-cost-refine-reduced-cost={costRefineArc?.reducedCost.toString()}
							data-double-scaling-inspected-branch={
								doubleScalingInspectedBranch
							}
							data-orlin-max-class={orlinMaxClass}
							data-orlin-max-abundant={orlinMaxResidual?.abundant || undefined}
							data-orlin-max-anti-abundant={
								orlinMaxResidual?.anti_abundant || undefined
							}
							data-orlin-max-small={orlinMaxResidual?.small || undefined}
							data-orlin-max-medium={orlinMaxResidual?.medium || undefined}
							data-orlin-max-scan={orlinMaxResidual?.inspection_serial}
							data-minimum-mean-scan={minimumMeanScanForArc?.ordinal}
							data-relaxation-scan={relaxationScanForArc?.ordinal}
							data-primitive-inspection={primitiveInspectionForArc?.completed}
							data-primitive-inspection-total={primitiveInspectionForArc?.total}
							data-prediction-active={predictionActive || undefined}
							data-tardos-reduced-cost={tardosResidual?.reduced_cost}
							data-tardos-fixes-variable={
								tardosResidual?.fixes_variable || undefined
							}
							data-tardos-threshold={overlayViews.tardosFramework?.threshold}
							data-convex-eligible={convexEligible || undefined}
							data-enhanced-scaling-path={enhancedScalingPath || undefined}
							data-enhanced-scaling-contract={
								enhancedScalingContract || undefined
							}
							data-relaxed-mndc-cycle={relaxedMndcCycle}
						>
							<title>{`${arc.edge_id}:${arc.direction} · residual ${arc.capacity}${rootwardForest && forest ? (distanceDirected ? " · exact shortest-path tree · child→sink" : " · represented tree · child→root") : eibfsSourceTree ? " · EIBFS S forest · parent→child" : eibfsSinkTree ? " · EIBFS T forest · child→parent" : ""}${binaryBaseZero ? " · base length 0" : ""}${binarySpecial ? " · special length 0" : ""}${binaryAdmissible ? " · admissible" : ""}${binaryZeroAdmissible ? " · zero-SCC arc" : ""}${binaryInspected ? " · inspecting binary length" : ""}${cancelTightenAdmissible ? " · negative reduced cost" : ""}${cancelTightenCycle ? " · selected cancel cycle" : ""}${cancelTightenInspected ? " · inspected now" : ""}${relaxedMndcAssignment ? " · split-node assignment" : ""}${relaxedMndcInspected ? " · split-graph scan now" : ""}${relaxedMndcCycle === undefined ? "" : ` · MNDC cycle ${relaxedMndcCycle + 1}`}${enhancedScalingPath ? " · Orlin quotient shortest path" : ""}${enhancedScalingContract ? " · contracted strongly feasible arc" : ""}${orlinMaxClass === undefined ? "" : ` · Orlin ${orlinMaxClass}`}${doubleScalingAdmissible ? " · transformed flow branch admissible" : ""}${doubleScalingActive ? " · active transformed flow branch" : ""}${doubleScalingInspectedBranch === undefined ? "" : ` · inspecting transformed ${doubleScalingInspectedBranch} branch`}${convexEligible ? ` · Δ=${overlayViews.convexCost?.scale} eligible marginal` : ""}${convexActive ? " · active marginal segment" : ""}${predictionActive ? " · ε-balanced prediction-assisted push" : ""}${costRefineArc === undefined ? "" : ` · scaled reduced cost ${costRefineArc.reducedCost} · epsilon ${costScalingRefine?.epsilon}`}${tardosResidual === undefined ? "" : ` · reduced cost ${tardosResidual.reduced_cost} · threshold ${overlayViews.tardosFramework?.threshold}${tardosResidual.fixes_variable ? " · fixes original variable" : ""}`}${minimumMeanScanForArc === undefined ? "" : ` · ${minimumMeanScanForArc.caption}`}${relaxationScanForArc === undefined ? "" : ` · ${relaxationScanForArc.caption}`}${primitiveInspectionForArc === undefined ? "" : ` · ${primitiveInspectionForArc.caption}`}${arc.fixed ? " · fixed" : ""}`}</title>
							{tardosResidual?.fixes_variable && (
								<>
									<path
										d={path}
										className="flow-tardos-threshold-outer"
										strokeWidth={9 + (tardosMagnitude ?? 0)}
									/>
									<path
										d={path}
										className="flow-tardos-threshold-gap"
										strokeWidth={6 + (tardosMagnitude ?? 0)}
									/>
								</>
							)}
							{selected && (
								<path
									d={path}
									className="flow-residual-selection-outline"
									strokeWidth={width + 10}
								/>
							)}
							{activeWorkingArc && (
								<path
									d={path}
									className="flow-residual-active-outline"
									strokeWidth={width + 6}
								/>
							)}
							{changed && (
								<path
									d={path}
									className="flow-event-change-edge-outline"
									strokeWidth={width + 12}
								/>
							)}
							{touched && (
								<path
									d={path}
									className="flow-event-touch-edge-outline"
									strokeWidth={width + 8}
									markerEnd={flowScopedSvgUrl(
										idScope,
										"flow-arrow-residual-active",
									)}
								/>
							)}
							{costRefineArc !== undefined && (
								<path
									d={path}
									className="flow-cost-refine-edge-outline"
									strokeWidth={width + 5}
								/>
							)}
							{overlayOwners.length === 0 ? (
								<path
									d={path}
									strokeWidth={
										expanded
											? tardosMagnitude === undefined
												? width
												: 2 + tardosMagnitude
											: 1.2
									}
									markerEnd={
										!expanded
											? undefined
											: activeWorkingArc ||
													touched ||
													cancelTightenCycle ||
													relaxedMndcCycle !== undefined ||
													enhancedScalingContract ||
													doubleScalingActive ||
													doubleScalingInspectedBranch !== undefined ||
													convexActive ||
													predictionActive
												? flowScopedSvgUrl(
														idScope,
														"flow-arrow-residual-active",
													)
												: ibfsSourceTree
													? flowScopedSvgUrl(idScope, "flow-arrow-ibfs-source")
													: ibfsSinkTree
														? flowScopedSvgUrl(idScope, "flow-arrow-ibfs-sink")
														: eibfsSourceTree
															? flowScopedSvgUrl(
																	idScope,
																	"flow-arrow-eibfs-source",
																)
															: eibfsSinkTree
																? flowScopedSvgUrl(
																		idScope,
																		"flow-arrow-eibfs-sink",
																	)
																: forest
																	? flowScopedSvgUrl(
																			idScope,
																			"flow-arrow-forest",
																		)
																	: flowScopedSvgUrl(
																			idScope,
																			"flow-arrow-residual",
																		)
									}
								/>
							) : (
								<FlowGraphOverlayOwnedLeaves
									state={state}
									bundle="original-edge-discrete-overlay"
									entity={{
										kind: "residual-arc",
										id: arc.edge_id,
										direction: arc.direction,
									}}
									owners={overlayOwners}
								>
									<path
										d={path}
										strokeWidth={
											expanded
												? tardosMagnitude === undefined
													? width
													: 2 + tardosMagnitude
												: 1.2
										}
										markerEnd={
											!expanded
												? undefined
												: activeWorkingArc ||
														touched ||
														cancelTightenCycle ||
														relaxedMndcCycle !== undefined ||
														enhancedScalingContract ||
														doubleScalingActive ||
														doubleScalingInspectedBranch !== undefined ||
														convexActive ||
														predictionActive
													? flowScopedSvgUrl(
															idScope,
															"flow-arrow-residual-active",
														)
													: ibfsSourceTree
														? flowScopedSvgUrl(
																idScope,
																"flow-arrow-ibfs-source",
															)
														: ibfsSinkTree
															? flowScopedSvgUrl(
																	idScope,
																	"flow-arrow-ibfs-sink",
																)
															: eibfsSourceTree
																? flowScopedSvgUrl(
																		idScope,
																		"flow-arrow-eibfs-source",
																	)
																: eibfsSinkTree
																	? flowScopedSvgUrl(
																			idScope,
																			"flow-arrow-eibfs-sink",
																		)
																	: forest
																		? flowScopedSvgUrl(
																				idScope,
																				"flow-arrow-forest",
																			)
																		: flowScopedSvgUrl(
																				idScope,
																				"flow-arrow-residual",
																			)
										}
									/>
								</FlowGraphOverlayOwnedLeaves>
							)}
							{expanded &&
								viewMode === "residual" &&
								(plan.edgeLabelIds.has(arc.edge_id) || focused) && (
									<text
										x={label.x}
										y={label.y}
										className="flow-residual-label"
										textAnchor="middle"
									>
										{arc.direction === "forward" ? "R+" : "R−"} {arc.capacity}
										{orlinMaxClass === undefined
											? ""
											: ` · ${orlinMaxClass === "abundant" ? "A" : orlinMaxClass === "anti-abundant" ? "Ā" : orlinMaxClass === "medium" ? "M" : "S"}`}
										{orlinMaxResidual?.inspection_serial === undefined
											? ""
											: ` · #${orlinMaxResidual.inspection_serial}`}
										{minimumMeanScanForArc === undefined
											? ""
											: ` · ${minimumMeanScanForArc.caption}`}
										{relaxationScanForArc === undefined
											? ""
											: ` · ${relaxationScanForArc.caption}`}
										{primitiveInspectionForArc === undefined
											? ""
											: ` · ${primitiveInspectionForArc.caption}`}
										{tardosResidual === undefined
											? ""
											: ` · c̄ ${tardosResidual.reduced_cost}${tardosResidual.fixes_variable ? " · FIX" : ""}`}
									</text>
								)}
							{costRefineArc?.witness && costScalingRefine !== undefined && (
								<g
									className="flow-cost-refine-witness"
									transform={`translate(${label.x} ${label.y - 15})`}
								>
									<line x1="0" y1="8" x2="0" y2="15" />
									<rect x="-48" y="-9" width="96" height="18" rx="5" />
									<text y="0.5" dominantBaseline="central" textAnchor="middle">
										c̄ {costRefineArc.reducedCost} · −ε{" "}
										{-costScalingRefine.epsilon}
									</text>
								</g>
							)}
							<path className="flow-edge-hit-target" d={path} />
						</g>
					);
				})}
		</>
	);
}
