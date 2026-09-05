import type { ReactNode } from "react";
import {
	FlowGraphOverlayOwnedLeaves,
	type FlowOverlayLeafOwner,
} from "./FlowGraphOverlayOwnedLeaves";
import { flowScopedSvgUrl, useFlowGraphIdScope } from "./flow-dom-id";
import {
	type projectFlowEntityGraphState,
	rationalMagnitudeStrokeWidth,
} from "./flow-entity-graph-state";
import { flowCapacityScalingPhaseBoundary } from "./flow-event-highlight";
import {
	orlinMcfBelowGateWitness,
	orlinMcfPhaseGateStatus,
} from "./flow-graph-rational-scales";
import { formatFlowRational } from "./flow-parametric-view";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
type OriginalVisual = FlowEntityGraphState["originalVisuals"][number];

function OwnedDiscreteEdgeUnderlay({
	state,
	visual,
	owners,
	sourceRole,
	children,
}: Readonly<{
	state: FlowEntityGraphState;
	visual: OriginalVisual;
	owners: readonly FlowOverlayLeafOwner[];
	sourceRole?: string | undefined;
	children: ReactNode;
}>) {
	return (
		<FlowGraphOverlayOwnedLeaves
			state={state}
			bundle="original-edge-discrete-underlay"
			entity={{ kind: "edge", id: visual.edge.id }}
			owners={owners.map((owner) => ({
				...owner,
				role: sourceRole ?? owner.role,
			}))}
		>
			{children}
		</FlowGraphOverlayOwnedLeaves>
	);
}

export function FlowGraphDiscreteEdgeUnderlayBundle({
	state,
	visual,
	enabled,
}: Readonly<{
	state: FlowEntityGraphState;
	visual: OriginalVisual;
	enabled: boolean;
}>) {
	const idScope = useFlowGraphIdScope();
	if (!enabled) return null;
	const positions = state.positions;
	const viewMode = state.viewMode;
	const overlayViews = state.renderData.overlayViews;
	const maximumOrlinMcfBranchFlow = state.renderData.maximumOrlinMcfBranchFlow;
	const orlinMcfCapacityNodeByEdge =
		state.renderData.orlinMcfCapacityNodeByEdge;
	const orlinMcfComponentById = state.renderData.orlinMcfComponentById;
	const orlinMcfContractionKey = state.renderData.orlinMcfContractionKey;
	const orlinMcfPathArcKeys = state.renderData.orlinMcfPathArcKeys;
	const orlinBelowGateWitness =
		overlayViews.orlinMcf?.stage === "begin-phase"
			? orlinMcfBelowGateWitness(
					overlayViews.orlinMcf.components,
					overlayViews.orlinMcf.delta,
				)
			: undefined;
	const orlinMax = overlayViews.orlinMaxFlow;
	const orlinMaxTail = state.renderData.orlinMaxNodeById.get(visual.edge.from);
	const orlinMaxHead = state.renderData.orlinMaxNodeById.get(visual.edge.to);
	const orlinImprovementDirection = (() => {
		if (
			orlinMax?.stage !== "begin-improvement" ||
			orlinMaxTail === undefined ||
			orlinMaxHead === undefined ||
			orlinMaxTail.source_side === orlinMaxHead.source_side
		) {
			return undefined;
		}
		return orlinMaxTail.source_side ? "forward" : "reverse";
	})();
	const orlinImprovementResidual =
		orlinImprovementDirection === undefined
			? undefined
			: state.renderData.orlinMaxResidualByKey.get(
					`${visual.edge.id}:${orlinImprovementDirection}`,
				);
	const cancelTighten = overlayViews.cancelTighten;
	const cancelTightenPhaseDirections =
		cancelTighten?.stage === "begin-phase"
			? (["forward", "reverse"] as const).filter((direction) =>
					state.renderData.cancelTightenAdmissibleArcKeys.has(
						`${visual.edge.id}:${direction}`,
					),
				)
			: [];
	const scalingPhase = flowCapacityScalingPhaseBoundary(state.context);
	const scalingPhaseArcs =
		scalingPhase === undefined
			? []
			: state.context.residualArcs.filter(
					(arc) =>
						arc.edge_id === visual.edge.id &&
						BigInt(arc.capacity) >= scalingPhase.scale,
				);
	const eibfsOwners: FlowOverlayLeafOwner[] = [];
	if (overlayViews.eibfs !== undefined) {
		eibfsOwners.push({
			overlay: "eibfs_overlay",
			role: "forest.parent-arc",
		});
	}
	if (overlayViews.dynamicEibfs !== undefined) {
		eibfsOwners.push({
			overlay: "dynamic_eibfs_overlay",
			role: "repaired-forest.parent-arc",
		});
	}
	return (
		<>
			{scalingPhase !== undefined &&
				scalingPhaseArcs.map((arc) => (
					<path
						key={`scaling-phase:${visual.edge.id}:${arc.direction}`}
						className={`flow-capacity-scaling-phase-arc flow-capacity-scaling-phase-arc-${scalingPhase.boundary}`}
						data-capacity-scaling-variant={scalingPhase.variant}
						data-capacity-scaling-boundary={scalingPhase.boundary}
						data-capacity-scaling-direction={arc.direction}
						data-capacity-scaling-scale={scalingPhase.scaleLabel}
						data-capacity-scaling-residual={arc.capacity}
						d={
							arc.direction === "reverse"
								? visual.geometry.reversePath
								: visual.geometry.path
						}
						strokeWidth={visual.railWidth + 6}
						markerEnd={flowScopedSvgUrl(idScope, "flow-arrow-capacity-scaling")}
					>
						<title>{`${visual.edge.id}:${arc.direction} · residual ${arc.capacity} ≥ Δ ${scalingPhase.scaleLabel} · ${scalingPhase.boundary} ${scalingPhase.variant}-scaling phase`}</title>
					</path>
				))}
			{cancelTightenPhaseDirections.map((direction) => (
				<FlowGraphOverlayOwnedLeaves
					key={`cancel-phase:${visual.edge.id}:${direction}`}
					state={state}
					bundle="original-edge-discrete-underlay"
					entity={{ kind: "residual-arc", id: visual.edge.id, direction }}
					owners={[
						{
							overlay: "cancel_tighten_overlay",
							role: "admissible_arcs.phase-frontier",
						},
					]}
				>
					<path
						className="flow-cancel-tighten-phase-arc"
						data-cancel-tighten-phase={cancelTighten?.phase}
						data-cancel-tighten-direction={direction}
						d={
							direction === "reverse"
								? visual.geometry.reversePath
								: visual.geometry.path
						}
						strokeWidth={visual.railWidth + 5}
						markerEnd={flowScopedSvgUrl(idScope, "flow-arrow-residual-active")}
					>
						<title>{`${visual.edge.id}:${direction} · admissible at cancel/tighten phase ${cancelTighten?.phase} · ε ${cancelTighten === undefined ? "—" : formatFlowRational(cancelTighten.epsilon)}`}</title>
					</path>
				</FlowGraphOverlayOwnedLeaves>
			))}
			{orlinMax?.stage === "begin-improvement" &&
				orlinImprovementDirection !== undefined &&
				orlinImprovementResidual !== undefined &&
				BigInt(orlinImprovementResidual.capacity) > 0n && (
					<OwnedDiscreteEdgeUnderlay
						state={state}
						visual={visual}
						owners={[
							{
								overlay: "orlin_max_flow_overlay",
								role: "begin-improvement.residual-cut-gap",
							},
						]}
					>
						<path
							className="flow-orlin-max-improvement-cut"
							data-orlin-max-improvement-delta={orlinMax.delta}
							data-orlin-max-improvement-direction={orlinImprovementDirection}
							data-orlin-max-improvement-residual={
								orlinImprovementResidual.capacity
							}
							d={
								orlinImprovementDirection === "reverse"
									? visual.geometry.reversePath
									: visual.geometry.path
							}
							strokeWidth={visual.railWidth + 7}
							markerEnd={flowScopedSvgUrl(
								idScope,
								"flow-arrow-orlin-improvement",
							)}
						>
							<title>{`${visual.edge.id}:${orlinImprovementDirection} · improvement-phase Δ gate · residual ${orlinImprovementResidual.capacity} · current cut gap ${orlinMax.delta}`}</title>
						</path>
					</OwnedDiscreteEdgeUnderlay>
				)}
			{visual.orlinMcfState?.flow !== undefined &&
				visual.orlinMcfState.slack !== undefined &&
				(() => {
					const tail = positions.get(visual.edge.from);
					const head = positions.get(visual.edge.to);
					const capacityNode = orlinMcfCapacityNodeByEdge.get(visual.edge.id);
					if (
						tail === undefined ||
						head === undefined ||
						capacityNode === undefined
					)
						return null;
					const center = visual.geometry.label;
					const capacityComponent = orlinMcfComponentById.get(
						capacityNode.component_id,
					);
					const activeRole =
						overlayViews.orlinMcf?.source_component ===
						capacityNode.component_id
							? "source"
							: overlayViews.orlinMcf?.sink_component ===
									capacityNode.component_id
								? "sink"
								: undefined;
					const phaseGate =
						overlayViews.orlinMcf?.stage === "begin-phase" &&
						capacityComponent !== undefined
							? orlinMcfPhaseGateStatus(
									capacityComponent.excess,
									overlayViews.orlinMcf.delta,
								)
							: undefined;
					const phaseGateRole =
						phaseGate === "excess" || phaseGate === "deficit"
							? phaseGate
							: orlinBelowGateWitness?.component_id ===
									capacityNode.component_id
								? "below"
								: undefined;
					const renderBranch = (
						branch: "flow" | "slack",
						endpoint: { x: number; y: number },
					) => {
						const state = visual.orlinMcfState?.[branch];
						if (state === undefined) return null;
						const selected = overlayViews.orlinMcf?.path.find(
							(arc) => arc.edge_id === visual.edge.id && arc.branch === branch,
						);
						const inspected = overlayViews.orlinMcf?.inspected_segment.find(
							(arc) => arc.edge_id === visual.edge.id && arc.branch === branch,
						);
						const activeArc = selected ?? inspected;
						const key = `${visual.edge.id}:${branch}:${activeArc?.direction ?? "forward"}`;
						const contracted = orlinMcfContractionKey === key;
						const reverse = activeArc?.direction === "reverse";
						const inspectionSerial = overlayViews.orlinMcf?.inspection_serial;
						const inspectionLabel =
							inspected === undefined || inspectionSerial === undefined
								? undefined
								: `${branch === "flow" ? "F" : "S"} · #${inspectionSerial} · ${inspected.direction === "forward" ? "→" : "←"}`;
						const badgeX = (center.x + endpoint.x) / 2;
						const badgeY = (center.y + endpoint.y) / 2 - 12;
						const badgeWidth =
							inspectionLabel === undefined
								? 0
								: Math.max(54, inspectionLabel.length * 5.2 + 10);
						return (
							<g key={branch}>
								<line
									className={`flow-orlin-branch flow-orlin-branch-${branch}${state.tight ? " flow-orlin-branch-tight" : " flow-orlin-branch-slack-cost"}${state.internal ? " flow-orlin-branch-internal" : ""}${state.strongly_feasible ? " flow-orlin-branch-strong" : ""}${selected === undefined ? "" : " flow-orlin-branch-active"}${inspected === undefined ? "" : " flow-orlin-branch-inspected"}${contracted ? " flow-orlin-branch-contract" : ""}`}
									data-orlin-branch={branch}
									data-orlin-path={orlinMcfPathArcKeys.has(key) || undefined}
									data-orlin-contract={contracted || undefined}
									data-orlin-inspected={inspected?.direction}
									data-orlin-scan={
										inspected === undefined ? undefined : inspectionSerial
									}
									data-orlin-flow={formatFlowRational(state.flow)}
									data-orlin-reduced-cost={state.reduced_cost}
									x1={reverse ? center.x : endpoint.x}
									y1={reverse ? center.y : endpoint.y}
									x2={reverse ? endpoint.x : center.x}
									y2={reverse ? endpoint.y : center.y}
									strokeWidth={rationalMagnitudeStrokeWidth(
										state.flow,
										maximumOrlinMcfBranchFlow,
									)}
									markerEnd={flowScopedSvgUrl(
										idScope,
										activeArc !== undefined || contracted
											? "flow-arrow-residual-active"
											: "flow-arrow-residual",
									)}
								>
									<title>{`${branch === "flow" ? "F · costed flow" : "S · zero-cost slack"} · x̃ ${formatFlowRational(state.flow)} · c̄ ${state.reduced_cost}${state.tight ? " · tight" : ""}${state.strongly_feasible ? " · 3nΔ contraction candidate" : ""}${selected === undefined ? "" : ` · compressed path ${selected.direction}`}${inspected === undefined ? "" : ` · inspected ${inspected.direction} scan ${inspectionSerial}`}`}</title>
								</line>
								{inspectionLabel !== undefined && (
									<g
										className="flow-orlin-inspection-badge"
										data-orlin-scan={inspectionSerial}
										data-orlin-inspection-label={inspectionLabel}
										transform={`translate(${badgeX - badgeWidth / 2} ${badgeY})`}
									>
										<rect width={badgeWidth} height="17" rx="5" />
										<text
											x={badgeWidth / 2}
											y="8.5"
											textAnchor="middle"
											dominantBaseline="central"
										>
											{inspectionLabel}
										</text>
									</g>
								)}
							</g>
						);
					};
					return (
						<OwnedDiscreteEdgeUnderlay
							state={state}
							visual={visual}
							owners={[
								{
									overlay: "orlin_mcf_overlay",
									role: "transformed-flow-and-slack-branches",
								},
							]}
						>
							<g className="flow-orlin-transformation">
								{renderBranch("flow", tail)}
								{renderBranch("slack", head)}
								<g
									className={`flow-orlin-capacity-node${activeRole === undefined ? "" : ` flow-orlin-capacity-node-${activeRole}`}`}
									data-orlin-capacity-node={visual.edge.id}
									data-orlin-component={capacityNode.component_id}
									data-orlin-potential={capacityNode.potential}
									data-orlin-distance={capacityNode.distance}
									data-orlin-mcf-role={activeRole}
									transform={`translate(${center.x} ${center.y})`}
								>
									<title>{`capacity:${visual.edge.id} · component ${capacityNode.component_id} · excess ${capacityComponent === undefined ? "—" : formatFlowRational(capacityComponent.excess)} · π ${capacityNode.potential}${capacityNode.distance === undefined ? "" : ` · d ${capacityNode.distance}`}${activeRole === undefined ? "" : ` · ${activeRole}`}`}</title>
									<rect x="-10" y="-10" width="20" height="20" rx="3" />
									<text textAnchor="middle" dominantBaseline="central">
										κ
									</text>
									{phaseGateRole !== undefined &&
										capacityComponent !== undefined && (
											<g
												className={`flow-orlin-phase-gate flow-orlin-phase-gate-${phaseGateRole}`}
												data-orlin-capacity-phase-gate={phaseGateRole}
												data-orlin-phase-gate-excess={`${capacityComponent.excess.numerator}/${capacityComponent.excess.denominator}`}
												data-orlin-phase-gate-delta={`${overlayViews.orlinMcf?.delta.numerator}/${overlayViews.orlinMcf?.delta.denominator}`}
												transform="translate(0 -23)"
											>
												<title>
													{phaseGateRole === "below"
														? `No active quotient component: this capacity component witnesses maximum |excess| ${formatFlowRational(capacityComponent.excess)} below 3Δ/4`
														: `Active capacity component: excess ${phaseGateRole === "excess" ? "≥ 3Δ/4" : "≤ −3Δ/4"}`}
												</title>
												<rect x="-31" y="-8" width="62" height="16" rx="8" />
												<text textAnchor="middle" dominantBaseline="central">
													{phaseGateRole === "below"
														? "MAX <¾Δ"
														: phaseGateRole === "excess"
															? "+≥¾Δ"
															: "−≥¾Δ"}
												</text>
											</g>
										)}
								</g>
							</g>
						</OwnedDiscreteEdgeUnderlay>
					);
				})()}
			{viewMode === "original" && visual.ibfsTreeSide !== undefined && (
				<path
					d={
						visual.ibfsTreeDirection === "reverse"
							? visual.geometry.reversePath
							: visual.geometry.path
					}
					className={`flow-ibfs-tree-overlay flow-ibfs-tree-overlay-${visual.ibfsTreeSide}`}
					data-ibfs-tree-direction={visual.ibfsTreeDirection}
					strokeWidth={visual.railWidth + 5}
					markerEnd={flowScopedSvgUrl(
						idScope,
						`flow-arrow-ibfs-${visual.ibfsTreeSide}`,
					)}
				/>
			)}
			{viewMode === "original" && visual.eibfsTreeSide !== undefined && (
				<OwnedDiscreteEdgeUnderlay
					state={state}
					visual={visual}
					owners={eibfsOwners}
				>
					<path
						d={
							visual.eibfsTreeDirection === "reverse"
								? visual.geometry.reversePath
								: visual.geometry.path
						}
						className={`flow-eibfs-tree-overlay flow-eibfs-tree-overlay-${visual.eibfsTreeSide}`}
						data-eibfs-tree-direction={visual.eibfsTreeDirection}
						data-eibfs-tree-parent={visual.eibfsTreeParent}
						data-eibfs-tree-child={visual.eibfsTreeChild}
						strokeWidth={visual.railWidth + 5}
						markerEnd={flowScopedSvgUrl(
							idScope,
							`flow-arrow-eibfs-${visual.eibfsTreeSide}`,
						)}
					/>
				</OwnedDiscreteEdgeUnderlay>
			)}
			{visual.polynomialDualState?.in_tree && (
				<OwnedDiscreteEdgeUnderlay
					state={state}
					visual={visual}
					owners={[
						{
							overlay: "polynomial_dual_simplex_overlay",
							role: "edges.auxiliary-tree-basis",
						},
					]}
				>
					<path
						d={visual.geometry.path}
						className="flow-polynomial-dual-tree-overlay"
						strokeWidth={visual.railWidth + 5}
					/>
				</OwnedDiscreteEdgeUnderlay>
			)}
		</>
	);
}
