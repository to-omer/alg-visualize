import type { ReactNode } from "react";
import { FlowGraphOverlayOwnedLeaves } from "./FlowGraphOverlayOwnedLeaves";
import { flowScopedSvgUrl, useFlowGraphIdScope } from "./flow-dom-id";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import type { FlowSceneV9OverlayField } from "./flow-scene-wire/generated/overlays";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
type OriginalVisual = FlowEntityGraphState["originalVisuals"][number];

function OwnedTreeChainEdgeFeature({
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
			bundle="original-edge-tree-chain"
			entity={{ kind: "edge", id: visual.edge.id }}
			owners={[{ overlay, role: sourceRole }]}
		>
			{children}
		</FlowGraphOverlayOwnedLeaves>
	);
}

export function FlowGraphTreeChainEdgeFeatureBundle({
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
	const overlayViews = state.renderData.overlayViews;
	const randomizedCycleIsCandidate =
		overlayViews.randomizedAlmostLinear?.stage === "inspect-fundamental-cycle";
	const randomizedCycleIsSelected =
		overlayViews.randomizedAlmostLinear?.stage === "query-minimum-ratio-cycle";
	const randomizedCycleIsVisible =
		randomizedCycleIsCandidate ||
		randomizedCycleIsSelected ||
		overlayViews.randomizedAlmostLinear?.stage === "potential-reduction-step";
	const randomizedInspectsFeasibleAssignment =
		overlayViews.randomizedAlmostLinear?.stage ===
		"inspect-feasible-assignment";
	const randomizedShowsChangedCoordinates =
		overlayViews.randomizedAlmostLinear?.stage === "detect-changed-coordinates";
	const deterministicCycleIsCandidate =
		overlayViews.deterministicAlmostLinear?.stage ===
		"inspect-fundamental-cycle";
	const deterministicCycleIsSelected =
		overlayViews.deterministicAlmostLinear?.stage ===
		"query-minimum-ratio-cycle";
	const deterministicCycleIsVisible =
		deterministicCycleIsCandidate ||
		deterministicCycleIsSelected ||
		overlayViews.deterministicAlmostLinear?.stage ===
			"potential-reduction-step";
	const deterministicShowsChangedCoordinates =
		overlayViews.deterministicAlmostLinear?.stage ===
		"detect-changed-coordinates";
	const deterministicSpannerIsPublished =
		overlayViews.deterministicAlmostLinear?.stage !== "build-core-graph";
	const minimumRatioCertified =
		overlayViews.minimumRatioCycle?.stage === "complete";
	return (
		<>
			{visual.minimumRatioState !== undefined && (
				<OwnedTreeChainEdgeFeature
					state={state}
					visual={visual}
					overlay="minimum_ratio_cycle_overlay"
					sourceRole="edges.length-gradient-and-cycle-state"
				>
					<path
						d={visual.geometry.path}
						className="flow-minimum-ratio-length"
						strokeWidth={visual.minimumRatioLengthWidth}
					/>
					<path
						d={visual.geometry.path}
						className={`flow-minimum-ratio-gradient flow-minimum-ratio-gradient-${BigInt(visual.minimumRatioState.gradient) < 0n ? "negative" : BigInt(visual.minimumRatioState.gradient) > 0n ? "positive" : "zero"} flow-minimum-ratio-gradient-magnitude-${visual.minimumRatioGradientBand}`}
						strokeWidth={Math.max(
							2,
							Number(visual.minimumRatioLengthWidth) - 4,
						)}
					/>
					{visual.minimumRatioState.tree_edge && (
						<path
							d={visual.geometry.path}
							className="flow-minimum-ratio-tree"
							strokeWidth={Number(visual.minimumRatioLengthWidth) + 5}
						/>
					)}
					{visual.minimumRatioState.candidate_sign !== "0" && (
						<path
							d={
								visual.minimumRatioState.candidate_sign === "-1"
									? visual.geometry.reversePath
									: visual.geometry.path
							}
							className="flow-minimum-ratio-candidate"
							data-min-ratio-direction={
								visual.minimumRatioState.candidate_sign === "-1"
									? "reverse"
									: "forward"
							}
							strokeWidth={Number(visual.minimumRatioLengthWidth) + 4}
							markerEnd={flowScopedSvgUrl(
								idScope,
								"flow-arrow-minimum-ratio-candidate",
							)}
						/>
					)}
					{visual.minimumRatioState.selected_sign !== "0" && (
						<path
							d={
								visual.minimumRatioState.selected_sign === "-1"
									? visual.geometry.reversePath
									: visual.geometry.path
							}
							className="flow-minimum-ratio-selected"
							data-min-ratio-direction={
								visual.minimumRatioState.selected_sign === "-1"
									? "reverse"
									: "forward"
							}
							strokeWidth={Number(visual.minimumRatioLengthWidth) + 8}
							markerEnd={flowScopedSvgUrl(
								idScope,
								"flow-arrow-minimum-ratio-selected",
							)}
						/>
					)}
					{minimumRatioCertified &&
						visual.minimumRatioState.selected_sign !== "0" && (
							<path
								d={
									visual.minimumRatioState.selected_sign === "-1"
										? visual.geometry.reversePath
										: visual.geometry.path
								}
								className="flow-minimum-ratio-certified"
								strokeWidth={Number(visual.minimumRatioLengthWidth) + 13}
							/>
						)}
				</OwnedTreeChainEdgeFeature>
			)}
			{visual.randomizedAlmostLinearState !== undefined && (
				<OwnedTreeChainEdgeFeature
					state={state}
					visual={visual}
					overlay="randomized_almost_linear_overlay"
					sourceRole="edges.sampled-tree-chain-and-cycle-state"
				>
					<path
						d={visual.geometry.path}
						className="flow-randomized-length-rail"
						strokeWidth={visual.randomizedLengthWidth}
					/>
					<path
						d={visual.geometry.path}
						className={`flow-randomized-gradient-line flow-randomized-gradient-${Number(visual.randomizedAlmostLinearState.gradient) < 0 ? "negative" : Number(visual.randomizedAlmostLinearState.gradient) > 0 ? "positive" : "zero"} flow-randomized-gradient-band-${visual.randomizedGradientBand}`}
						strokeWidth={Math.max(2, Number(visual.randomizedLengthWidth) - 4)}
					/>
					{BigInt(visual.randomizedAlmostLinearState.sampled_tree_memberships) >
						0n && (
						<path
							d={visual.geometry.path}
							className="flow-randomized-sampled-membership"
							data-randomized-memberships={
								visual.randomizedAlmostLinearState.sampled_tree_memberships
							}
							strokeWidth={
								Number(visual.randomizedLengthWidth) +
								Math.min(
									5,
									Number(
										visual.randomizedAlmostLinearState.sampled_tree_memberships,
									),
								)
							}
						/>
					)}
					{visual.randomizedAlmostLinearState.active_tree_edge && (
						<path
							d={visual.geometry.path}
							className="flow-randomized-active-tree"
							strokeWidth={Number(visual.randomizedLengthWidth) + 6}
						/>
					)}
					{randomizedShowsChangedCoordinates &&
						visual.randomizedAlmostLinearState.changed_coordinate && (
							<path
								d={visual.geometry.path}
								className="flow-randomized-changed-coordinate"
								strokeWidth={Number(visual.randomizedLengthWidth) + 11}
							/>
						)}
					{randomizedInspectsFeasibleAssignment &&
						Number(visual.randomizedAlmostLinearState.interior_flow) > 0 && (
							<path
								d={visual.geometry.path}
								className="flow-randomized-feasible-assignment"
								data-randomized-assignment-flow={
									visual.randomizedAlmostLinearState.interior_flow
								}
								strokeWidth={
									2.5 +
									7 *
										(Number(visual.randomizedAlmostLinearState.interior_flow) /
											Number(visual.edge.capacity))
								}
							>
								<title>{`${visual.edge.id} · enumerated feasible assignment ${visual.randomizedAlmostLinearState.interior_flow}/${visual.edge.capacity}`}</title>
							</path>
						)}
					{randomizedCycleIsVisible &&
						visual.randomizedAlmostLinearState.active_cycle_sign !== "0" && (
							<path
								d={
									visual.randomizedAlmostLinearState.active_cycle_sign === "-1"
										? visual.geometry.reversePath
										: visual.geometry.path
								}
								className={`flow-randomized-active-cycle${randomizedCycleIsCandidate ? " flow-randomized-candidate-cycle" : ""}${randomizedCycleIsSelected ? " flow-randomized-selected-cycle" : ""}`}
								data-randomized-cycle-direction={
									visual.randomizedAlmostLinearState.active_cycle_sign === "-1"
										? "reverse"
										: "forward"
								}
								strokeWidth={Number(visual.randomizedLengthWidth) + 8}
								markerEnd={flowScopedSvgUrl(
									idScope,
									randomizedCycleIsCandidate
										? "flow-arrow-randomized-almost-linear-candidate-cycle"
										: "flow-arrow-randomized-almost-linear-cycle",
								)}
							/>
						)}
				</OwnedTreeChainEdgeFeature>
			)}
			{visual.deterministicAlmostLinearState !== undefined && (
				<OwnedTreeChainEdgeFeature
					state={state}
					visual={visual}
					overlay="deterministic_almost_linear_overlay"
					sourceRole="edges.hierarchy-spanner-embedding-and-rounding-state"
				>
					<path
						d={visual.geometry.path}
						className="flow-deterministic-length-rail"
						strokeWidth={visual.deterministicLengthWidth}
					/>
					<path
						d={visual.geometry.path}
						className={`flow-deterministic-gradient-line flow-deterministic-gradient-${Number(visual.deterministicAlmostLinearState.gradient) < 0 ? "negative" : Number(visual.deterministicAlmostLinearState.gradient) > 0 ? "positive" : "zero"} flow-deterministic-gradient-band-${visual.deterministicGradientBand}`}
						strokeWidth={Math.max(
							2,
							Number(visual.deterministicLengthWidth) - 4,
						)}
					/>
					{visual.deterministicAlmostLinearState.tree_level_mask !== "0" && (
						<path
							d={visual.geometry.path}
							className="flow-deterministic-tree-chain"
							data-deterministic-tree-level-mask={
								visual.deterministicAlmostLinearState.tree_level_mask
							}
							strokeWidth={Number(visual.deterministicLengthWidth) + 5}
						/>
					)}
					{visual.deterministicAlmostLinearState.forest_level_mask !== "0" && (
						<path
							d={visual.geometry.path}
							className="flow-deterministic-partial-forest"
							data-deterministic-forest-level-mask={
								visual.deterministicAlmostLinearState.forest_level_mask
							}
							strokeWidth={Number(visual.deterministicLengthWidth) + 8}
						/>
					)}
					{visual.deterministicAlmostLinearState.active_core_edge && (
						<path
							d={visual.geometry.path}
							className="flow-deterministic-core-edge"
							strokeWidth={Number(visual.deterministicLengthWidth) + 10}
						/>
					)}
					{deterministicSpannerIsPublished &&
						visual.deterministicAlmostLinearState.active_spanner_edge && (
							<path
								d={visual.geometry.path}
								className="flow-deterministic-spanner-edge"
								strokeWidth={Number(visual.deterministicLengthWidth) + 6}
							/>
						)}
					{deterministicSpannerIsPublished &&
						visual.deterministicAlmostLinearState.active_core_edge &&
						!visual.deterministicAlmostLinearState.active_spanner_edge &&
						BigInt(visual.deterministicAlmostLinearState.embedding_hops) >
							0n && (
							<path
								d={visual.geometry.path}
								className="flow-deterministic-embedding-summary"
								data-deterministic-embedding-hops={
									visual.deterministicAlmostLinearState.embedding_hops
								}
								data-deterministic-embedding-stretch={
									visual.deterministicAlmostLinearState.embedding_stretch
								}
								strokeWidth={Number(visual.deterministicLengthWidth) + 3}
							/>
						)}
					{visual.deterministicAlmostLinearState.active_tree_edge && (
						<path
							d={visual.geometry.path}
							className="flow-deterministic-active-tree"
							strokeWidth={Number(visual.deterministicLengthWidth) + 7}
						/>
					)}
					{deterministicShowsChangedCoordinates &&
						visual.deterministicAlmostLinearState.changed_coordinate && (
							<path
								d={visual.geometry.path}
								className="flow-deterministic-changed-coordinate"
								strokeWidth={Number(visual.deterministicLengthWidth) + 13}
							/>
						)}
					{deterministicCycleIsVisible &&
						visual.deterministicAlmostLinearState.active_cycle_sign !== "0" && (
							<path
								d={
									visual.deterministicAlmostLinearState.active_cycle_sign ===
									"-1"
										? visual.geometry.reversePath
										: visual.geometry.path
								}
								className={`flow-deterministic-active-cycle${deterministicCycleIsCandidate ? " flow-deterministic-candidate-cycle" : ""}${deterministicCycleIsSelected ? " flow-deterministic-selected-cycle" : ""}`}
								data-deterministic-cycle-direction={
									visual.deterministicAlmostLinearState.active_cycle_sign ===
									"-1"
										? "reverse"
										: "forward"
								}
								data-deterministic-cycle-kind={
									overlayViews.deterministicAlmostLinear?.selected_cycle_kind
								}
								strokeWidth={Number(visual.deterministicLengthWidth) + 9}
								markerEnd={flowScopedSvgUrl(
									idScope,
									deterministicCycleIsCandidate
										? "flow-arrow-deterministic-almost-linear-candidate-cycle"
										: "flow-arrow-deterministic-almost-linear-cycle",
								)}
							/>
						)}
					{visual.deterministicAlmostLinearState.rounding_forest_edge && (
						<path
							d={visual.geometry.path}
							className="flow-deterministic-rounding-forest"
							strokeWidth={Number(visual.deterministicLengthWidth) + 11}
						/>
					)}
					{visual.deterministicAlmostLinearState.rounding_cycle_sign !==
						"0" && (
						<path
							d={
								visual.deterministicAlmostLinearState.rounding_cycle_sign ===
								"-1"
									? visual.geometry.reversePath
									: visual.geometry.path
							}
							className="flow-deterministic-rounding-cycle"
							data-deterministic-rounding-direction={
								visual.deterministicAlmostLinearState.rounding_cycle_sign ===
								"-1"
									? "reverse"
									: "forward"
							}
							strokeWidth={Number(visual.deterministicLengthWidth) + 12}
							markerEnd={flowScopedSvgUrl(
								idScope,
								"flow-arrow-deterministic-almost-linear-cycle",
							)}
						/>
					)}
				</OwnedTreeChainEdgeFeature>
			)}
		</>
	);
}
