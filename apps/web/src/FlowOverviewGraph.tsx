import type { CSSProperties, MouseEvent as ReactMouseEvent } from "react";
import {
	FlowGraphMarkers,
	type RmfgenFrameGroup,
	RmfgenFrameGroups,
} from "./FlowEntityGraph";
import {
	FlowGraphIdScopeProvider,
	flowScopedDomId,
	flowScopedSvgUrl,
	useFlowDomIdScope,
} from "./flow-dom-id";
import type { FlowEntitySelection } from "./flow-entity-navigator";
import { shouldRenderFlowEventEntityEmphasis } from "./flow-event-highlight";
import type {
	FlowOverviewOriginalEdge,
	FlowOverviewRenderPlan,
} from "./flow-render-plan";
import { FLOW_LOD_LIMITS } from "./flow-render-plan";
import {
	absoluteBigInt,
	capacityRailWidth,
	costMagnitudeIntensity,
	flowFillStrokeWidth,
} from "./flow-visual-scales";
import { flowWorkbenchPolicy } from "./flow-workbench-policy";
import type { FlowWorkbenchProblemKind } from "./flow-workbench-problem";
import type { FlowCanvasSvgBinding } from "./use-flow-canvas-viewport";

function overviewEdgeCostMagnitude(edge: FlowOverviewOriginalEdge): bigint {
	const minimumMagnitude = absoluteBigInt(edge.minimumCost);
	const maximumMagnitude = absoluteBigInt(edge.maximumCost);
	return minimumMagnitude > maximumMagnitude
		? minimumMagnitude
		: maximumMagnitude;
}

export function flowOverviewAccessibleDescription({
	problemKind,
	hasBalances,
	hasSupernode,
	hasFrames,
	hasFixedEdges,
}: Readonly<{
	problemKind: FlowWorkbenchProblemKind;
	hasBalances: boolean;
	hasSupernode: boolean;
	hasFrames: boolean;
	hasFixedEdges: boolean;
}>): string {
	const sentences = [
		"Spatially aggregated flow-network overview.",
		"Circle labels show cluster size; neutral rail width encodes total capacity and the blue inner fill encodes current flow.",
	];
	if (flowWorkbenchPolicy(problemKind).showsCost) {
		sentences.push(
			"The outer cost rail uses color and dash for cost sign, with continuous intensity for the largest absolute cost in each aggregate.",
		);
	}
	if (hasBalances) {
		sentences.push(
			"Solid outer rings mark supply clusters, dashed rings mark demand clusters, and signed counts show their members.",
		);
	}
	if (hasSupernode) {
		sentences.push(
			"Clusters containing a GRIDGEN supernode are labeled super and use a dotted ring.",
		);
	}
	if (hasFrames) {
		sentences.push("Dashed background frames show RMFGEN frame boundaries.");
	}
	if (hasFixedEdges) {
		sentences.push(
			"A long-dashed overlay marks aggregates containing fixed arcs.",
		);
	}
	return sentences.join(" ");
}

export function FlowOverviewGraph({
	plan,
	problemKind,
	viewMode,
	frameGroups,
	selection,
	onSelectionChange,
	canvasBinding,
}: {
	plan: FlowOverviewRenderPlan;
	problemKind: FlowWorkbenchProblemKind;
	viewMode: "original" | "residual" | "both";
	frameGroups: readonly RmfgenFrameGroup[];
	selection: FlowEntitySelection | undefined;
	onSelectionChange: (selection: FlowEntitySelection) => void;
	canvasBinding: FlowCanvasSvgBinding;
}) {
	const idScope = useFlowDomIdScope("flow-overview-graph");
	const titleId = flowScopedDomId(idScope, "title");
	const descriptionId = flowScopedDomId(idScope, "description");
	const showsCost = flowWorkbenchPolicy(problemKind).showsCost;
	const maxCapacity = plan.originalEdges.reduce(
		(maximum, edge) => (edge.capacity > maximum ? edge.capacity : maximum),
		1n,
	);
	const maxResidualCapacity = plan.residualArcs.reduce(
		(maximum, arc) => (arc.capacity > maximum ? arc.capacity : maximum),
		1n,
	);
	const maxAbsoluteCost = plan.originalEdges.reduce((maximum, edge) => {
		const magnitude = overviewEdgeCostMagnitude(edge);
		return magnitude > maximum ? magnitude : maximum;
	}, 0n);
	const residualArcs = [...plan.residualArcs].sort(
		(left, right) =>
			Number(
				left.activeCount > 0 || left.traceCount > 0 || left.changeCount > 0,
			) -
			Number(
				right.activeCount > 0 || right.traceCount > 0 || right.changeCount > 0,
			),
	);
	const hasBalances = plan.clusters.some(
		(cluster) => cluster.balance !== "none",
	);
	const hasSupernode = plan.clusters.some(
		(cluster) => cluster.containsSupernode,
	);
	const hasFixedEdges = plan.originalEdges.some((edge) => edge.fixedCount > 0);
	const renderTouchedResidualAggregates = shouldRenderFlowEventEntityEmphasis({
		level: "overview",
		kind: "edge",
		signal: "touch",
		memberCount: plan.residualArcs.filter((arc) => arc.traceCount > 0).length,
		totalCount: plan.residualArcs.length,
		structureLimit: FLOW_LOD_LIMITS.structureEdgeEventLabels,
	});
	const renderChangedResidualAggregates = shouldRenderFlowEventEntityEmphasis({
		level: "overview",
		kind: "edge",
		signal: "change",
		memberCount: plan.residualArcs.filter((arc) => arc.changeCount > 0).length,
		totalCount: plan.residualArcs.length,
		structureLimit: FLOW_LOD_LIMITS.structureEdgeEventLabels,
	});
	const renderTouchedOriginalAggregates = shouldRenderFlowEventEntityEmphasis({
		level: "overview",
		kind: "edge",
		signal: "touch",
		memberCount: plan.originalEdges.filter((edge) => edge.traceCount > 0)
			.length,
		totalCount: plan.originalEdges.length,
		structureLimit: FLOW_LOD_LIMITS.structureEdgeEventLabels,
	});
	const renderChangedOriginalAggregates = shouldRenderFlowEventEntityEmphasis({
		level: "overview",
		kind: "edge",
		signal: "change",
		memberCount: plan.originalEdges.filter((edge) => edge.changeCount > 0)
			.length,
		totalCount: plan.originalEdges.length,
		structureLimit: FLOW_LOD_LIMITS.structureEdgeEventLabels,
	});
	const renderTouchedClusters = shouldRenderFlowEventEntityEmphasis({
		level: "overview",
		kind: "node",
		signal: "touch",
		memberCount: plan.clusters.filter((cluster) => cluster.traceCount > 0)
			.length,
		totalCount: plan.clusters.length,
		structureLimit: FLOW_LOD_LIMITS.structureNodeEventLabels,
	});
	const renderChangedClusters = shouldRenderFlowEventEntityEmphasis({
		level: "overview",
		kind: "node",
		signal: "change",
		memberCount: plan.clusters.filter((cluster) => cluster.changeCount > 0)
			.length,
		totalCount: plan.clusters.length,
		structureLimit: FLOW_LOD_LIMITS.structureNodeEventLabels,
	});
	const selectAggregate = (event: ReactMouseEvent<SVGSVGElement>) => {
		if (!(event.target instanceof Element)) return;
		const entity = event.target.closest(
			"[data-cluster-id], [data-aggregate-id]",
		);
		if (entity === null || !event.currentTarget.contains(entity)) return;
		const clusterId = entity.getAttribute("data-cluster-id");
		if (clusterId !== null) {
			onSelectionChange({
				kind: "aggregate",
				aggregateKind: "cluster",
				id: clusterId,
			});
			return;
		}
		const id = entity.getAttribute("data-aggregate-id");
		const aggregateKind = entity.getAttribute("data-aggregate-kind");
		if (
			id !== null &&
			(aggregateKind === "original-edge" || aggregateKind === "residual-arc")
		) {
			onSelectionChange({ kind: "aggregate", aggregateKind, id });
		}
	};

	return (
		<FlowGraphIdScopeProvider scope={idScope}>
			{/* biome-ignore lint/a11y/useKeyWithClickEvents: The adjacent entity navigator provides the complete keyboard interaction. */}
			<svg
				{...canvasBinding}
				className="flow-graph flow-graph-overview"
				role="img"
				aria-labelledby={titleId}
				aria-describedby={descriptionId}
				data-flow-lod="overview"
				data-rendered-nodes={plan.clusters.length}
				data-rendered-original-edges={plan.originalEdges.length}
				data-rendered-residual-arcs={plan.residualArcs.length}
				onClick={selectAggregate}
			>
				<title id={titleId}>Validated flow-network overview</title>
				<desc id={descriptionId}>
					{flowOverviewAccessibleDescription({
						problemKind,
						hasBalances,
						hasSupernode,
						hasFrames: frameGroups.length > 0,
						hasFixedEdges,
					})}
				</desc>
				<FlowGraphMarkers />
				<RmfgenFrameGroups groups={frameGroups} />

				{(viewMode !== "original" ||
					residualArcs.some(
						(arc) => arc.traceCount > 0 || arc.changeCount > 0,
					)) &&
					residualArcs.map((arc) => {
						if (
							viewMode === "original" &&
							arc.traceCount === 0 &&
							arc.changeCount === 0
						)
							return null;
						const rawTouched = arc.traceCount > 0;
						const rawChanged = arc.changeCount > 0;
						const touched = renderTouchedResidualAggregates && rawTouched;
						const changed = renderChangedResidualAggregates && rawChanged;
						const width =
							2 + Number((arc.capacity * 3_000n) / maxResidualCapacity) / 1_000;
						const visible =
							arc.capacity > 0n || arc.activeCount > 0 || arc.fixedCount > 0;
						return (
							<g
								key={arc.id}
								data-aggregate-id={arc.id}
								data-aggregate-kind="residual-arc"
								data-aggregate-count={arc.arcCount}
								data-event-touch={rawTouched || undefined}
								data-event-change={rawChanged || undefined}
								data-event-identities={
									rawTouched ? arc.traceIdentities.join("|") : undefined
								}
								data-changed-identities={
									rawChanged ? arc.changedIdentities.join("|") : undefined
								}
								className={`flow-residual-arc flow-residual-${arc.direction}${arc.from === arc.to ? " flow-overview-internal" : ""}${arc.fixedCount > 0 ? " flow-residual-fixed" : ""}${arc.activeCount > 0 ? " flow-residual-active" : ""}${visible ? "" : " flow-residual-zero"}${selection?.kind === "aggregate" && selection.aggregateKind === "residual-arc" && selection.id === arc.id ? " flow-entity-selected" : ""}`}
							>
								<title>{`${arc.arcCount} residual arcs · residual capacity ${arc.capacity}${arc.fixedCount > 0 ? ` · ${arc.fixedCount} fixed` : ""}${rawTouched ? ` · ${arc.traceCount} touched by this event` : ""}${rawChanged ? ` · ${arc.changeCount} changed by this event` : ""}`}</title>
								{changed && (
									<path
										d={arc.route.path}
										className="flow-event-change-edge-outline"
										strokeWidth={width + 12}
									/>
								)}
								{touched && (
									<path
										d={arc.route.path}
										className="flow-event-touch-edge-outline"
										strokeWidth={width + 8}
										markerEnd={flowScopedSvgUrl(
											idScope,
											"flow-arrow-residual-active",
										)}
									/>
								)}
								<path
									d={arc.route.path}
									strokeWidth={width}
									markerEnd={
										arc.activeCount > 0 || touched
											? flowScopedSvgUrl(idScope, "flow-arrow-residual-active")
											: flowScopedSvgUrl(idScope, "flow-arrow-residual")
									}
								/>
							</g>
						);
					})}

				{viewMode !== "residual" &&
					plan.originalEdges.map((edge) => {
						const rawTouched = edge.traceCount > 0;
						const rawChanged = edge.changeCount > 0;
						const touched = renderTouchedOriginalAggregates && rawTouched;
						const changed = renderChangedOriginalAggregates && rawChanged;
						const railWidth = capacityRailWidth(edge.capacity, maxCapacity);
						const flowWidth = flowFillStrokeWidth(
							edge.flow,
							edge.capacity,
							railWidth,
						);
						const costRange =
							edge.minimumCost === edge.maximumCost
								? edge.minimumCost.toString()
								: `${edge.minimumCost}…${edge.maximumCost}`;
						const costIntensity = costMagnitudeIntensity(
							overviewEdgeCostMagnitude(edge),
							maxAbsoluteCost,
						);
						const selected =
							selection?.kind === "aggregate" &&
							selection.aggregateKind === "original-edge" &&
							selection.id === edge.id;
						return (
							<g
								key={edge.id}
								data-aggregate-id={edge.id}
								data-aggregate-kind="original-edge"
								data-aggregate-count={edge.edgeCount}
								data-event-touch={rawTouched || undefined}
								data-event-change={rawChanged || undefined}
								data-event-identities={
									rawTouched ? edge.traceIdentities.join("|") : undefined
								}
								data-changed-identities={
									rawChanged ? edge.changedIdentities.join("|") : undefined
								}
								data-cost-intensity={showsCost ? costIntensity : undefined}
								className={`flow-original-edge flow-overview-original-edge${edge.from === edge.to ? " flow-overview-internal" : ""}${selected ? " flow-entity-selected" : ""}`}
							>
								<title>{`${edge.edgeCount} edges · flow ${edge.flow} / capacity ${edge.capacity}${showsCost ? ` · cost ${costRange}` : ""}${edge.fixedCount > 0 ? ` · ${edge.fixedCount} fixed` : ""}${rawTouched ? ` · ${edge.traceCount} touched by this event` : ""}${rawChanged ? ` · ${edge.changeCount} changed by this event` : ""}`}</title>
								{edge.activeCount > 0 && (
									<path
										d={edge.route.path}
										className="flow-active-outline"
										strokeWidth={railWidth + (showsCost ? 8 : 5)}
									/>
								)}
								{changed && (
									<path
										d={edge.route.path}
										className="flow-event-change-edge-outline"
										strokeWidth={railWidth + (showsCost ? 14 : 11)}
									/>
								)}
								{touched && (
									<path
										d={edge.route.path}
										className="flow-event-touch-edge-outline"
										strokeWidth={railWidth + (showsCost ? 10 : 7)}
									/>
								)}
								{selected && (
									<path
										d={edge.route.path}
										className="flow-selection-outline"
										strokeWidth={railWidth + (showsCost ? 11 : 8)}
									/>
								)}
								{showsCost && (
									<path
										d={edge.route.path}
										className={`flow-cost-rail flow-cost-${edge.costKind}`}
										data-flow-channel="cost"
										style={
											{
												"--flow-cost-intensity": costIntensity,
												"--flow-cost-mix": `${Math.round(42 + costIntensity * 58)}%`,
												strokeWidth: railWidth + 4,
											} as CSSProperties
										}
										strokeWidth={railWidth + 4}
									/>
								)}
								<path
									d={edge.route.path}
									className="flow-capacity-rail"
									data-flow-channel="capacity"
									style={{ strokeWidth: railWidth }}
									strokeWidth={railWidth}
									markerEnd={flowScopedSvgUrl(idScope, "flow-arrow-capacity")}
								/>
								<path
									d={edge.route.path}
									className="flow-flow-line"
									data-flow-channel="flow"
									style={{ strokeWidth: flowWidth }}
									strokeWidth={flowWidth}
									markerEnd={
										flowWidth > 0
											? flowScopedSvgUrl(idScope, "flow-arrow-fill")
											: undefined
									}
								/>
								{edge.cutCount > 0 && (
									<path
										d={edge.route.path}
										className="flow-algorithm-edge-overlay flow-algorithm-edge-overlay-danger"
										strokeWidth={railWidth + 6}
									/>
								)}
								{edge.fixedCount > 0 && (
									<path
										d={edge.route.path}
										className="flow-algorithm-edge-overlay flow-algorithm-edge-overlay-fixed"
										strokeWidth={railWidth + 7}
									/>
								)}
								<path d={edge.route.path} className="flow-edge-hit-target" />
							</g>
						);
					})}

				{plan.clusters.map((cluster) => {
					const rawTouched = cluster.traceCount > 0;
					const rawChanged = cluster.changeCount > 0;
					const touched = renderTouchedClusters && rawTouched;
					const changed = renderChangedClusters && rawChanged;
					const radius = Math.min(
						33,
						14 + Math.log2(cluster.memberCount + 1) * 3,
					);
					const terminalClass =
						cluster.terminal === "source"
							? " flow-node-source"
							: cluster.terminal === "sink"
								? " flow-node-sink"
								: cluster.terminal === "both"
									? " flow-node-terminal-mixed"
									: "";
					const label = cluster.containsSupernode
						? "super"
						: (cluster.terminalLabel ?? `×${cluster.memberCount}`);
					const balanceSummary = [
						cluster.supplyCount > 0 ? `+${cluster.supplyCount}` : undefined,
						cluster.demandCount > 0 ? `−${cluster.demandCount}` : undefined,
					]
						.filter((value) => value !== undefined)
						.join(" / ");
					const clusterAnnotation = cluster.containsSupernode
						? `×${cluster.memberCount}${balanceSummary.length > 0 ? ` · ${balanceSummary}` : ""}`
						: balanceSummary;
					return (
						<g
							key={cluster.id}
							data-cluster-id={cluster.id}
							data-cluster-count={cluster.memberCount}
							data-event-touch={rawTouched || undefined}
							data-event-change={rawChanged || undefined}
							data-event-identities={
								cluster.traceCount > 0
									? cluster.traceIdentities.join("|")
									: undefined
							}
							data-changed-identities={
								cluster.changeCount > 0
									? cluster.changedIdentities.join("|")
									: undefined
							}
							className={`flow-overview-cluster flow-overview-source-${cluster.sourceSide}${selection?.kind === "aggregate" && selection.aggregateKind === "cluster" && selection.id === cluster.id ? " flow-entity-selected" : ""}`}
							transform={`translate(${cluster.x} ${cluster.y})`}
						>
							<title>{`${cluster.memberCount} nodes${cluster.containsSupernode ? " · contains GRIDGEN supernode" : ""}${cluster.supplyCount > 0 ? ` · ${cluster.supplyCount} supply` : ""}${cluster.demandCount > 0 ? ` · ${cluster.demandCount} demand` : ""}${cluster.balance !== "none" ? ` · net balance ${cluster.netBalance > 0n ? "+" : ""}${cluster.netBalance}` : ""}${cluster.traceCount > 0 ? ` · ${cluster.traceCount} touched by this event` : ""}${cluster.changeCount > 0 ? ` · ${cluster.changeCount} changed by this event` : ""}`}</title>
							<circle className={`flow-node${terminalClass}`} r={radius} />
							{cluster.containsSupernode && (
								<circle className="flow-supernode-ring" r={radius + 6} />
							)}
							{cluster.terminal !== "none" && (
								<circle className="flow-terminal-ring" r={radius + 6} />
							)}
							{cluster.supplyCount > 0 && (
								<circle
									className="flow-balance-ring flow-balance-ring-supply"
									r={radius + (cluster.terminal === "none" ? 6 : 11)}
								/>
							)}
							{cluster.demandCount > 0 && (
								<circle
									className="flow-balance-ring flow-balance-ring-demand"
									r={
										radius +
										(cluster.terminal !== "none" || cluster.supplyCount > 0
											? 11
											: 6)
									}
								/>
							)}
							<text
								className="flow-node-label"
								textAnchor="middle"
								dominantBaseline="central"
							>
								{label}
							</text>
							{cluster.terminal !== "none" && cluster.memberCount > 1 && (
								<text
									className="flow-node-trace"
									textAnchor="middle"
									y={radius + 17}
								>
									+{cluster.memberCount - 1} nodes
								</text>
							)}
							{cluster.terminal === "none" && clusterAnnotation.length > 0 && (
								<text
									className="flow-node-trace"
									textAnchor="middle"
									y={radius + (cluster.balance === "mixed" ? 22 : 17)}
								>
									{clusterAnnotation}
								</text>
							)}
							{touched && (
								<circle
									r={radius + 12}
									className="flow-event-touch-node-ring"
								/>
							)}
							{changed && (
								<circle
									r={radius + 8}
									className="flow-event-change-node-ring"
								/>
							)}
						</g>
					);
				})}
			</svg>
		</FlowGraphIdScopeProvider>
	);
}
