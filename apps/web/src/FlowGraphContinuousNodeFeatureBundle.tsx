import type { ReactNode } from "react";
import { FlowGraphOverlayOwnedLeaves } from "./FlowGraphOverlayOwnedLeaves";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import type { FlowSceneV9OverlayField } from "./flow-scene-wire/generated/overlays";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;

function OwnedContinuousNodeFeature({
	state,
	nodeId,
	overlay,
	sourceRole,
	children,
}: Readonly<{
	state: FlowEntityGraphState;
	nodeId: string;
	overlay: FlowSceneV9OverlayField;
	sourceRole: string;
	children: ReactNode;
}>) {
	return (
		<FlowGraphOverlayOwnedLeaves
			state={state}
			bundle="node-continuous"
			entity={{ kind: "node", id: nodeId }}
			owners={[{ overlay, role: sourceRole }]}
		>
			{children}
		</FlowGraphOverlayOwnedLeaves>
	);
}

/** Electrical, IPM, minimum-ratio, and almost-linear node glyphs. */
export function FlowGraphContinuousNodeFeatureBundle({
	state,
	nodeId,
	enabled,
}: Readonly<{
	state: FlowEntityGraphState;
	nodeId: string;
	enabled: boolean;
}>) {
	if (!enabled) return null;
	const data = state.renderData;
	const interiorPointNode = data.interiorPointNodeById.get(nodeId);
	const interiorPointPotentialBand =
		interiorPointNode === undefined
			? undefined
			: data.interiorPointPotentialBand(Number(interiorPointNode.potential));
	const minimumRatioNode = data.minimumRatioNodeById.get(nodeId);
	const minimumRatioCertified =
		data.overlayViews.minimumRatioCycle?.stage === "complete";
	const randomizedNode = data.randomizedAlmostLinearNodeById.get(nodeId);
	const randomizedCycleIsCandidate =
		data.overlayViews.randomizedAlmostLinear?.stage ===
		"inspect-fundamental-cycle";
	const randomizedCycleIsSelected =
		data.overlayViews.randomizedAlmostLinear?.stage ===
		"query-minimum-ratio-cycle";
	const randomizedCycleIsVisible =
		randomizedCycleIsCandidate ||
		randomizedCycleIsSelected ||
		data.overlayViews.randomizedAlmostLinear?.stage ===
			"potential-reduction-step";
	const deterministicNode = data.deterministicAlmostLinearNodeById.get(nodeId);
	const deterministicCycleIsCandidate =
		data.overlayViews.deterministicAlmostLinear?.stage ===
		"inspect-fundamental-cycle";
	const deterministicCycleIsSelected =
		data.overlayViews.deterministicAlmostLinear?.stage ===
		"query-minimum-ratio-cycle";
	const deterministicCycleIsVisible =
		deterministicCycleIsCandidate ||
		deterministicCycleIsSelected ||
		data.overlayViews.deterministicAlmostLinear?.stage ===
			"potential-reduction-step";
	const augmentingNode = data.augmentingElectricalNodeById.get(nodeId);
	const augmentingPotentialBand =
		augmentingNode === undefined
			? undefined
			: data.augmentingPotentialBand(Number(augmentingNode.potential));
	const augmentingActivePathNode =
		data.overlayViews.augmentingElectrical?.active_working_path.some(
			(arc) => arc.from_node === nodeId || arc.to_node === nodeId,
		) === true;
	const augmentingCouplingRatio =
		augmentingNode === undefined ||
		data.maximumAugmentingCoupling <= Number.EPSILON
			? 0
			: Math.abs(Number(augmentingNode.coupling_violation)) /
				data.maximumAugmentingCoupling;
	const electricalNode = data.electricalNodeById.get(nodeId);
	const electricalPotentialBand =
		electricalNode === undefined
			? undefined
			: data.electricalPotentialBand(Number(electricalNode.potential));
	const electricalStage = data.overlayViews.electricalFlow?.stage;
	const electricalCertificateOwner = state.terminal?.source === nodeId;
	const electricalIpmMcfNode = data.electricalIpmMcfNodeById.get(nodeId);
	const electricalIpmMcfPotentialBand =
		electricalIpmMcfNode === undefined
			? undefined
			: data.electricalIpmMcfPotentialBand(
					Number(electricalIpmMcfNode.potential),
				);
	const electricalIpmMcfDirectionMagnitude =
		electricalIpmMcfNode === undefined ||
		data.maximumElectricalIpmMcfPotentialDirection <= Number.EPSILON
			? 0
			: Math.abs(Number(electricalIpmMcfNode.potential_direction)) /
				data.maximumElectricalIpmMcfPotentialDirection;

	return (
		<>
			{electricalIpmMcfNode !== undefined && (
				<OwnedContinuousNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="electrical_ipm_mcf_overlay"
					sourceRole="nodes.potential-and-newton-direction"
				>
					<circle
						className={`flow-eipm-main-potential-ring flow-eipm-main-potential-ring-${electricalIpmMcfPotentialBand}`}
						r="38"
					/>
					{electricalIpmMcfDirectionMagnitude > 0 && (
						<circle
							className={`flow-eipm-main-direction-ring flow-eipm-main-direction-${Number(electricalIpmMcfNode.potential_direction) < 0 ? "negative" : "positive"}`}
							r="43"
							style={{
								strokeWidth: 1.5 + electricalIpmMcfDirectionMagnitude * 3.5,
								strokeDashoffset:
									Number(electricalIpmMcfNode.potential_direction) * 3,
							}}
						/>
					)}
					{Number(electricalIpmMcfNode.balance_residual) !== 0 && (
						<circle className="flow-eipm-main-balance-ring" r="48" />
					)}
					{electricalIpmMcfNode.anchored && (
						<text
							className="flow-eipm-main-anchor-glyph"
							x="25"
							y="-22"
							textAnchor="middle"
						>
							⏚
						</text>
					)}
				</OwnedContinuousNodeFeature>
			)}
			{interiorPointNode !== undefined && (
				<OwnedContinuousNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="interior_point_max_flow_overlay"
					sourceRole="nodes.potential-and-target-cut"
				>
					<circle
						className={`flow-interior-point-potential-ring flow-interior-point-potential-ring-${interiorPointPotentialBand}`}
						r="38"
					/>
					<circle
						className={`flow-interior-point-target-ring flow-interior-point-target-${interiorPointNode.target_source_side ? "source" : "sink"}`}
						r="44"
					/>
				</OwnedContinuousNodeFeature>
			)}
			{minimumRatioNode !== undefined && (
				<OwnedContinuousNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="minimum_ratio_cycle_overlay"
					sourceRole="nodes.tree-component-and-cycle-membership"
				>
					<circle className="flow-minimum-ratio-component-ring" r="38" />
					{minimumRatioNode.on_candidate && (
						<circle className="flow-minimum-ratio-candidate-ring" r="43" />
					)}
					{minimumRatioNode.on_selected && (
						<circle className="flow-minimum-ratio-selected-ring" r="48" />
					)}
					{minimumRatioCertified &&
						minimumRatioNode.parent_node_id === undefined && (
							<circle className="flow-minimum-ratio-certificate-root" r="53" />
						)}
				</OwnedContinuousNodeFeature>
			)}
			{randomizedNode !== undefined && (
				<OwnedContinuousNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="randomized_almost_linear_overlay"
					sourceRole="nodes.sampled-tree-and-rounding-state"
				>
					<circle
						className="flow-randomized-component-mark"
						cx="-24"
						cy="-24"
						r="3.5"
					/>
					<path
						className={`flow-randomized-cut-ring flow-randomized-cut-${randomizedNode.source_side ? "source" : "sink"}`}
						d={
							randomizedNode.source_side
								? "M -29 -8 A 30 30 0 0 1 -8 -29"
								: "M 8 -29 A 30 30 0 0 1 29 -8"
						}
					/>
					{randomizedNode.artificial_direction !== "0" && (
						<path
							className="flow-randomized-artificial-mark"
							d="M 24 -30 L 30 -24 L 24 -18 L 18 -24 Z"
						/>
					)}
					{randomizedCycleIsVisible &&
						randomizedNode.active_artificial_sign !== "0" && (
							<circle
								className={`flow-randomized-active-cycle-ring${randomizedCycleIsCandidate ? " flow-randomized-candidate-cycle-ring" : ""}${randomizedCycleIsSelected ? " flow-randomized-selected-cycle-ring" : ""}`}
								r="36"
							/>
						)}
				</OwnedContinuousNodeFeature>
			)}
			{deterministicNode !== undefined && (
				<OwnedContinuousNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="deterministic_almost_linear_overlay"
					sourceRole="nodes.deterministic-forest-and-rounding-state"
				>
					<circle
						className="flow-deterministic-component-mark"
						cx="-24"
						cy="-24"
						r="3.5"
					/>
					<path
						className={`flow-deterministic-cut-ring flow-deterministic-cut-${deterministicNode.source_side ? "source" : "sink"}`}
						d={
							deterministicNode.source_side
								? "M -29 -8 A 30 30 0 0 1 -8 -29"
								: "M 8 -29 A 30 30 0 0 1 29 -8"
						}
					/>
					{deterministicNode.artificial_direction !== "0" && (
						<path
							className="flow-deterministic-artificial-mark"
							d="M 24 -30 L 30 -24 L 24 -18 L 18 -24 Z"
						/>
					)}
					{deterministicCycleIsVisible &&
						deterministicNode.active_artificial_sign !== "0" && (
							<circle
								className={`flow-deterministic-active-cycle-ring${deterministicCycleIsCandidate ? " flow-deterministic-candidate-cycle-ring" : ""}${deterministicCycleIsSelected ? " flow-deterministic-selected-cycle-ring" : ""}`}
								r="36"
							/>
						)}
				</OwnedContinuousNodeFeature>
			)}
			{augmentingNode !== undefined && (
				<OwnedContinuousNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="augmenting_electrical_overlay"
					sourceRole="nodes.embedding-and-coupling-state"
				>
					<circle
						className={`flow-augmenting-potential-ring flow-augmenting-potential-ring-${augmentingPotentialBand}${augmentingActivePathNode ? " flow-augmenting-node-active" : ""}`}
						r="38"
					/>
					<circle
						className={`flow-augmenting-target-cut-ring flow-augmenting-target-cut-${augmentingNode.target_source_side ? "source" : "sink"}${augmentingActivePathNode ? " flow-augmenting-node-active" : ""}`}
						r="43"
					/>
					{augmentingCouplingRatio >= 0.25 && (
						<circle
							className={`flow-augmenting-coupling-ring${augmentingActivePathNode ? " flow-augmenting-node-active" : ""}`}
							r="48"
						/>
					)}
				</OwnedContinuousNodeFeature>
			)}
			{electricalNode !== undefined && (
				<OwnedContinuousNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="electrical_flow_overlay"
					sourceRole="nodes.potential-and-residual"
				>
					<circle
						className={`flow-electrical-potential-ring flow-electrical-potential-ring-${electricalPotentialBand}`}
						r="38"
					/>
					{Number(electricalNode.residual) !== 0 && (
						<circle className="flow-electrical-residual-ring" r="44" />
					)}
					{electricalNode.grounded && (
						<text
							className="flow-electrical-ground-glyph"
							x="25"
							y="-22"
							textAnchor="middle"
						>
							⏚
						</text>
					)}
					{electricalCertificateOwner &&
						electricalStage === "check-exact-reference" && (
							<g className="flow-electrical-certificate-badge flow-electrical-certificate-check">
								<circle cx="27" cy="-27" r="9" />
								<text
									x="27"
									y="-27"
									textAnchor="middle"
									dominantBaseline="central"
								>
									Δ
								</text>
							</g>
						)}
					{electricalCertificateOwner && electricalStage === "complete" && (
						<g className="flow-electrical-certificate-badge flow-electrical-certificate-complete">
							<circle cx="27" cy="-27" r="9" />
							<text
								x="27"
								y="-27"
								textAnchor="middle"
								dominantBaseline="central"
							>
								✓
							</text>
						</g>
					)}
				</OwnedContinuousNodeFeature>
			)}
		</>
	);
}
