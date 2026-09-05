import type { FlowOverlayLeafOwner } from "./FlowGraphOverlayOwnedLeaves";
import { FlowGraphOverlayOwnedLeaves } from "./FlowGraphOverlayOwnedLeaves";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import { flowCapacityScalingPhaseBoundary } from "./flow-event-highlight";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
export type FlowGraphNodeKind =
	| "source"
	| "sink"
	| "supply"
	| "demand"
	| "normal";

/** Search-forest, cover, terminal, and balance node glyphs. */
export function FlowGraphSearchNodeFeatureBundle({
	state,
	nodeId,
	kind,
	nodeBalance,
	overlayEnabled,
}: Readonly<{
	state: FlowEntityGraphState;
	nodeId: string;
	kind: FlowGraphNodeKind;
	nodeBalance: bigint;
	overlayEnabled: boolean;
}>) {
	const ibfsNode = state.visualization.ibfsView?.nodes.get(nodeId);
	const eibfsNode = state.visualization.eibfsView?.nodes.get(nodeId);
	const ibfsRepairFocus =
		state.visualization.ibfsView?.repairFocusNodeIds.has(nodeId) === true;
	const eibfsRepairFocus =
		state.visualization.eibfsView?.repairFocusNodeIds.has(nodeId) === true;
	const transportation = state.visualization.features.transportation;
	const traceState = state.visualization.nodeTraceStates.get(nodeId);
	const scalingPhase = flowCapacityScalingPhaseBoundary(state.context);
	const scalingRemaining =
		traceState?.remaining_divergence === undefined
			? undefined
			: BigInt(traceState.remaining_divergence);
	const scalingRole =
		scalingPhase === undefined || scalingRemaining === undefined
			? undefined
			: scalingRemaining >= scalingPhase.scale
				? "excess"
				: scalingRemaining <= -scalingPhase.scale
					? "deficit"
					: undefined;
	const hopcroftLayer =
		state.context.algorithmId === "hopcroft-karp" &&
		state.context.traceEvent?.catalog_id === "hopcroft-karp.level-bfs" &&
		traceState?.label !== undefined
			? Number(BigInt(traceState.label) % 5n)
			: undefined;
	const eibfsOwners: FlowOverlayLeafOwner[] = [];
	if (state.renderData.overlayViews.eibfs !== undefined) {
		eibfsOwners.push({
			overlay: "eibfs_overlay",
			role: "nodes.bidirectional-search-forest",
		});
	}
	if (state.renderData.overlayViews.dynamicEibfs !== undefined) {
		eibfsOwners.push({
			overlay: "dynamic_eibfs_overlay",
			role: "nodes.reused-and-repaired-search-forest",
		});
	}

	return (
		<>
			{scalingPhase !== undefined &&
				scalingRemaining !== undefined &&
				scalingRole !== undefined && (
					<g
						className={`flow-capacity-scaling-node-gate flow-capacity-scaling-node-gate-${scalingRole} flow-capacity-scaling-node-gate-${scalingPhase.boundary}`}
						data-capacity-scaling-node-role={scalingRole}
						data-capacity-scaling-node-remaining={scalingRemaining.toString()}
						data-capacity-scaling-node-scale={scalingPhase.scaleLabel}
					>
						<title>{`${nodeId} · remaining ${scalingRemaining} · ${scalingRole} is eligible at Δ ${scalingPhase.scaleLabel}`}</title>
						<path
							d={
								scalingRole === "excess"
									? "M -32 -17 A 36 36 0 0 1 -17 -32"
									: "M 17 -32 A 36 36 0 0 1 32 -17"
							}
						/>
						<text
							x={scalingRole === "excess" ? -36 : 36}
							y="-35"
							textAnchor="middle"
						>
							{scalingRole === "excess" ? "Δ+" : "Δ−"}
						</text>
					</g>
				)}
			{hopcroftLayer !== undefined && (
				<circle
					className={`flow-hopcroft-layer-dot flow-hopcroft-layer-${hopcroftLayer}`}
					data-hopcroft-layer={traceState?.label}
					cx="-20"
					cy="-20"
					r="5"
				/>
			)}
			{state.hasForestOverlay &&
				(kind === "normal" || transportation) &&
				!state.forestChildIds.has(nodeId) && (
					<circle className="flow-branch-root-ring" r="35" />
				)}
			{(kind === "source" || kind === "sink") && (
				<circle className="flow-terminal-ring" r="35" />
			)}
			{overlayEnabled && ibfsNode !== undefined && (
				<circle
					className={`flow-ibfs-tree-ring flow-ibfs-tree-ring-${ibfsNode.side}`}
					r="37"
				/>
			)}
			{overlayEnabled && ibfsNode?.orphan && (
				<circle className="flow-ibfs-orphan-ring" r="42" />
			)}
			{overlayEnabled && ibfsNode?.frontier && (
				<circle className="flow-ibfs-frontier-ring" r="47" />
			)}
			{overlayEnabled && ibfsRepairFocus && (
				<rect
					className="flow-ibfs-repair-focus"
					x="-34"
					y="-34"
					width="68"
					height="68"
					rx="10"
				/>
			)}
			{overlayEnabled && eibfsNode !== undefined && eibfsOwners.length > 0 && (
				<FlowGraphOverlayOwnedLeaves
					state={state}
					bundle="node-search"
					entity={{ kind: "node", id: nodeId }}
					owners={eibfsOwners}
				>
					{eibfsNode.membership !== "free" ? (
						<circle
							className={`flow-eibfs-membership-ring flow-eibfs-membership-ring-${eibfsNode.membership}`}
							r="37"
						/>
					) : (
						<circle className="flow-eibfs-free-ring" r="37" />
					)}
					{eibfsNode.root_kind !== "none" && (
						<circle className="flow-eibfs-root-ring" r="42" />
					)}
					{eibfsNode.orphan && (
						<circle className="flow-eibfs-orphan-ring" r="43" />
					)}
					{eibfsNode.frontier && (
						<circle className="flow-eibfs-frontier-ring" r="48" />
					)}
					{eibfsRepairFocus && (
						<rect
							className="flow-eibfs-repair-focus"
							x="-35"
							y="-35"
							width="70"
							height="70"
							rx="11"
						/>
					)}
				</FlowGraphOverlayOwnedLeaves>
			)}
			{state.visualization.matchingCoverNodes.has(nodeId) && (
				<circle className="flow-matching-cover-ring" r="36" />
			)}
			{state.visualization.assignmentHallNodes.has(nodeId) && (
				<circle className="flow-assignment-hall-ring" r="36" />
			)}
			{nodeBalance > 0n && (
				<circle
					className="flow-balance-ring flow-balance-ring-supply"
					r={kind === "source" || kind === "sink" ? 41 : 35}
				/>
			)}
			{nodeBalance < 0n && (
				<circle
					className="flow-balance-ring flow-balance-ring-demand"
					r={kind === "source" || kind === "sink" ? 41 : 35}
				/>
			)}
		</>
	);
}
