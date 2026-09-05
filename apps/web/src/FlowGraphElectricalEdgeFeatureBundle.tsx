import type { ReactNode } from "react";
import { FlowGraphOverlayOwnedLeaves } from "./FlowGraphOverlayOwnedLeaves";
import { flowScopedSvgUrl, useFlowGraphIdScope } from "./flow-dom-id";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import type { FlowSceneV9OverlayField } from "./flow-scene-wire/generated/overlays";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
type OriginalVisual = FlowEntityGraphState["originalVisuals"][number];

function roundedFlowStrokeWidth(flow: string, capacity: bigint): number {
	const signed = BigInt(flow);
	const magnitude = signed < 0n ? -signed : signed;
	if (capacity <= 0n) return 2;
	const bounded = magnitude > capacity ? capacity : magnitude;
	return 2 + Number((bounded * 5_000n) / capacity) / 1_000;
}

function OwnedElectricalEdgeFeature({
	state,
	visual,
	overlay,
	sourceRole,
	children,
}: Readonly<{
	state: FlowEntityGraphState;
	visual: OriginalVisual;
	overlay: FlowSceneV9OverlayField;
	sourceRole: string;
	children: ReactNode;
}>) {
	return (
		<FlowGraphOverlayOwnedLeaves
			state={state}
			bundle="original-edge-electrical"
			entity={{ kind: "edge", id: visual.edge.id }}
			owners={[{ overlay, role: sourceRole }]}
		>
			{children}
		</FlowGraphOverlayOwnedLeaves>
	);
}

export function FlowGraphElectricalEdgeFeatureBundle({
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
	const electricalStage = state.renderData.overlayViews.electricalFlow?.stage;
	const electricalIpmMcf = state.renderData.overlayViews.electricalIpmMcf;
	const recoveryAnchor =
		electricalIpmMcf?.stage === "approximate-flow"
			? (electricalIpmMcf.edges.find((edge) => !edge.fixed_on_face) ??
				electricalIpmMcf.edges[0])
			: undefined;
	const augmentingRoundedFlow =
		visual.augmentingElectricalState?.rounded_central_flow;
	const augmentingRoundedChanged =
		augmentingRoundedFlow !== undefined &&
		Math.abs(
			Number(visual.augmentingElectricalState?.central_flow) -
				Number(augmentingRoundedFlow),
		) > 1e-9;
	return (
		<>
			{visual.electricalIpmMcfState !== undefined && (
				<OwnedElectricalEdgeFeature
					state={state}
					visual={visual}
					overlay="electrical_ipm_mcf_overlay"
					sourceRole="edges.centrality-and-newton-direction"
				>
					<path
						d={visual.geometry.path}
						className={`flow-eipm-main-resistance flow-eipm-main-resistance-${visual.electricalIpmMcfResistanceBand}`}
						strokeWidth={visual.railWidth + 7}
					/>
					<path
						d={visual.geometry.path}
						className={`flow-eipm-main-slack flow-eipm-main-slack-${visual.electricalIpmMcfSlackBand}`}
						strokeWidth={visual.railWidth + 3}
					/>
					{visual.electricalIpmMcfState.fixed_on_face && (
						<path
							d={visual.geometry.path}
							className="flow-eipm-main-fixed"
							strokeWidth={Math.max(3, visual.railWidth - 1)}
						/>
					)}
					<path
						d={visual.geometry.path}
						className="flow-eipm-main-fractional"
						strokeWidth={visual.electricalIpmMcfFractionalWidth}
					/>
					{visual.electricalIpmMcfCurrentWidth > 0 && (
						<path
							d={
								Number(visual.electricalIpmMcfState.electrical_current) < 0
									? visual.geometry.reversePath
									: visual.geometry.path
							}
							className={`flow-eipm-main-current flow-eipm-main-current-${Number(visual.electricalIpmMcfState.electrical_current) < 0 ? "reverse" : "forward"}`}
							strokeWidth={visual.electricalIpmMcfCurrentWidth}
							markerEnd={flowScopedSvgUrl(
								idScope,
								`flow-arrow-interior-point-${Number(visual.electricalIpmMcfState.electrical_current) < 0 ? "reverse" : "forward"}`,
							)}
						/>
					)}
				</OwnedElectricalEdgeFeature>
			)}
			{visual.electricalIpmMcfState !== undefined &&
				recoveryAnchor?.edge_id === visual.edge.id &&
				electricalIpmMcf !== undefined && (
					<OwnedElectricalEdgeFeature
						state={state}
						visual={visual}
						overlay="electrical_ipm_mcf_overlay"
						sourceRole="edges.exact-recovery-threshold"
					>
						<g
							className="flow-eipm-main-recovery-badge"
							transform={`translate(${visual.geometry.routeMidpoint.x} ${visual.geometry.routeMidpoint.y})`}
							aria-label={`Exact recovery is ready: duality gap ${electricalIpmMcf.duality_gap_bound} is at most epsilon ${electricalIpmMcf.recovery_epsilon}`}
						>
							<title>{`Exact recovery is ready: duality gap ${electricalIpmMcf.duality_gap_bound} ≤ epsilon ${electricalIpmMcf.recovery_epsilon}; nearest-integer candidate matches the isolated optimum`}</title>
							<line x1="0" y1="0" x2="0" y2="-29" />
							<circle cx="0" cy="0" r="4" />
							<rect x="-49" y="-49" width="98" height="22" rx="11" />
							<text
								x="0"
								y="-38"
								textAnchor="middle"
								dominantBaseline="central"
							>
								2mμ ≤ ε · ROUND
							</text>
						</g>
					</OwnedElectricalEdgeFeature>
				)}
			{visual.interiorPointState !== undefined &&
				(visual.interiorPointState.normalized_away ? (
					<OwnedElectricalEdgeFeature
						state={state}
						visual={visual}
						overlay="interior_point_max_flow_overlay"
						sourceRole="edges.terminal-normalization"
					>
						<path
							d={visual.geometry.path}
							className="flow-interior-point-normalized-line"
							strokeWidth={Math.max(3, visual.railWidth - 2)}
						/>
					</OwnedElectricalEdgeFeature>
				) : (
					<OwnedElectricalEdgeFeature
						state={state}
						visual={visual}
						overlay="interior_point_max_flow_overlay"
						sourceRole="edges.centrality-and-electrical-direction"
					>
						<path
							d={visual.geometry.path}
							className={`flow-interior-point-resistance flow-interior-point-resistance-${visual.interiorPointResistanceBand}`}
							strokeWidth={visual.railWidth + 6}
						/>
						<path
							d={visual.geometry.path}
							className={`flow-interior-point-slack flow-interior-point-slack-${visual.interiorPointSlackBand}`}
							strokeWidth={visual.railWidth + 2}
						/>
						<path
							d={
								Number(visual.interiorPointState.electrical_current) < 0
									? visual.geometry.reversePath
									: visual.geometry.path
							}
							className={`flow-interior-point-congestion flow-interior-point-congestion-${visual.interiorPointCongestionBand}`}
							strokeWidth={
								5 +
								Math.min(10, Number(visual.interiorPointState.congestion) * 5)
							}
						/>
						<path
							d={visual.geometry.path}
							className="flow-interior-point-fractional"
							strokeWidth={visual.interiorPointFractionalWidth}
						/>
						<path
							d={
								Number(visual.interiorPointState.electrical_current) < 0
									? visual.geometry.reversePath
									: visual.geometry.path
							}
							className={`flow-interior-point-current flow-interior-point-current-${Number(visual.interiorPointState.electrical_current) < 0 ? "reverse" : Number(visual.interiorPointState.electrical_current) > 0 ? "forward" : "zero"}`}
							strokeWidth={
								2 +
								Math.min(7, Number(visual.interiorPointState.congestion) * 4)
							}
							markerEnd={
								Number(visual.interiorPointState.electrical_current) === 0
									? undefined
									: flowScopedSvgUrl(
											idScope,
											`flow-arrow-interior-point-${Number(visual.interiorPointState.electrical_current) < 0 ? "reverse" : "forward"}`,
										)
							}
						/>
					</OwnedElectricalEdgeFeature>
				))}
			{visual.augmentingElectricalState !== undefined && (
				<OwnedElectricalEdgeFeature
					state={state}
					visual={visual}
					overlay="augmenting_electrical_overlay"
					sourceRole="edges.central-flow-and-electrical-correction"
				>
					{BigInt(visual.augmentingElectricalState.boost_segments) > 1n && (
						<path
							d={visual.geometry.path}
							className="flow-augmenting-boost-rail"
							strokeWidth={visual.railWidth + 7}
						/>
					)}
					<path
						d={
							Number(visual.augmentingElectricalState.central_flow) < 0
								? visual.geometry.reversePath
								: visual.geometry.path
						}
						className="flow-augmenting-central-flow"
						strokeWidth={visual.augmentingCentralWidth}
						markerEnd={
							Number(visual.augmentingElectricalState.central_flow) === 0
								? undefined
								: flowScopedSvgUrl(
										idScope,
										"flow-arrow-augmenting-electrical-central",
									)
						}
					/>
					<path
						d={
							Number(visual.augmentingElectricalState.electrical_current) < 0
								? visual.geometry.reversePath
								: visual.geometry.path
						}
						className={`flow-augmenting-congestion-halo flow-augmenting-congestion-${visual.augmentingCongestionBand}`}
						strokeWidth={
							5 +
							Math.min(
								10,
								Number(visual.augmentingElectricalState.congestion) * 5,
							)
						}
					/>
					<path
						d={
							Number(visual.augmentingElectricalState.electrical_current) < 0
								? visual.geometry.reversePath
								: visual.geometry.path
						}
						className={`flow-augmenting-electrical-direction flow-augmenting-electrical-${Number(visual.augmentingElectricalState.electrical_current) < 0 ? "reverse" : Number(visual.augmentingElectricalState.electrical_current) > 0 ? "forward" : "zero"}`}
						strokeWidth={
							2 +
							Math.min(
								7,
								Number(visual.augmentingElectricalState.congestion) * 4,
							)
						}
						markerEnd={
							Number(visual.augmentingElectricalState.electrical_current) === 0
								? undefined
								: flowScopedSvgUrl(
										idScope,
										`flow-arrow-augmenting-electrical-${Number(visual.augmentingElectricalState.electrical_current) < 0 ? "reverse" : "forward"}`,
									)
						}
					/>
				</OwnedElectricalEdgeFeature>
			)}
			{visual.augmentingElectricalState !== undefined &&
				augmentingRoundedFlow !== undefined && (
					<OwnedElectricalEdgeFeature
						state={state}
						visual={visual}
						overlay="augmenting_electrical_overlay"
						sourceRole="edges.rounded-central-flow"
					>
						<path
							d={
								BigInt(augmentingRoundedFlow) < 0n ||
								(BigInt(augmentingRoundedFlow) === 0n &&
									Number(visual.augmentingElectricalState.central_flow) < 0)
									? visual.geometry.reversePath
									: visual.geometry.path
							}
							className={`flow-augmenting-rounded-flow flow-augmenting-rounded-flow-${augmentingRoundedChanged ? "changed" : "stable"}`}
							strokeWidth={roundedFlowStrokeWidth(
								augmentingRoundedFlow,
								visual.capacity,
							)}
							markerEnd={
								BigInt(augmentingRoundedFlow) === 0n
									? undefined
									: flowScopedSvgUrl(
											idScope,
											"flow-arrow-augmenting-electrical-rounded",
										)
							}
						>
							<title>{`Rounded central flow on ${visual.edge.id}: ${visual.augmentingElectricalState.central_flow} → ${augmentingRoundedFlow}`}</title>
						</path>
					</OwnedElectricalEdgeFeature>
				)}
			{visual.electricalState !== undefined && (
				<OwnedElectricalEdgeFeature
					state={state}
					visual={visual}
					overlay="electrical_flow_overlay"
					sourceRole="edges.energy-and-current"
				>
					{electricalStage !== undefined && electricalStage !== "ready" && (
						<path
							d={visual.geometry.path}
							className="flow-electrical-conductance-rail"
							strokeWidth={Math.max(2, visual.railWidth * 0.45)}
						/>
					)}
					<path
						d={
							Number(visual.electricalState.current) < 0
								? visual.geometry.reversePath
								: visual.geometry.path
						}
						className={`flow-electrical-energy-halo flow-electrical-energy-${visual.electricalEnergyBand}`}
						strokeWidth={
							4 + Math.min(8, Number(visual.electricalState.congestion) * 6)
						}
					/>
					<path
						d={
							Number(visual.electricalState.current) < 0
								? visual.geometry.reversePath
								: visual.geometry.path
						}
						className={`flow-electrical-current flow-electrical-current-${Number(visual.electricalState.current) < 0 ? "negative" : Number(visual.electricalState.current) > 0 ? "positive" : "zero"}`}
						strokeWidth={
							2 + Math.min(6, Number(visual.electricalState.congestion) * 5)
						}
						markerEnd={
							Number(visual.electricalState.current) === 0
								? undefined
								: flowScopedSvgUrl(
										idScope,
										`flow-arrow-electrical-${Number(visual.electricalState.current) < 0 ? "negative" : "positive"}`,
									)
						}
					/>
				</OwnedElectricalEdgeFeature>
			)}
		</>
	);
}
