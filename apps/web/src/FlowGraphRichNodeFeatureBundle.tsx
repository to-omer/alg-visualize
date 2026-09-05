import { FlowGraphContinuousNodeFeatureBundle } from "./FlowGraphContinuousNodeFeatureBundle";
import { FlowGraphOptimizationNodeFeatureBundle } from "./FlowGraphOptimizationNodeFeatureBundle";
import { FlowGraphOverlayOwnedLeaves } from "./FlowGraphOverlayOwnedLeaves";
import { FlowGraphRichNodeFrame } from "./FlowGraphRichNodeFrame";
import {
	type FlowGraphNodeKind,
	FlowGraphSearchNodeFeatureBundle,
} from "./FlowGraphSearchNodeFeatureBundle";
import { eibfsRootGlyph } from "./flow-eibfs-view";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import type { FlowEntitySelection } from "./flow-entity-navigator";
import {
	ordinaryFlowEventEntityRefs,
	shouldRenderFlowEventEntityEmphasis,
} from "./flow-event-highlight";
import { isOriginalEdgeSelected } from "./flow-graph-entity-selection";
import {
	FLOW_VIEWBOX_HEIGHT,
	FLOW_VIEWBOX_WIDTH,
	type FlowPoint,
} from "./flow-layout";
import { flowNodeCanvasLabel } from "./flow-node-display-label";
import { projectFlowNodeSemanticState } from "./flow-node-semantic-projection";
import { buildActiveFlowOverlayFeatureBundles } from "./flow-overlay-contribution-registry";
import { formatFlowRational } from "./flow-parametric-view";
import { FLOW_LOD_LIMITS } from "./flow-render-plan";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
type FlowGraphLayerProps = Readonly<{
	state: FlowEntityGraphState;
	selection: FlowEntitySelection | undefined;
}>;

type FlowNodeTracePlacement = Readonly<{
	x: number;
	y: number;
	leaderStart: FlowPoint;
	leaderEnd: FlowPoint;
}>;

type FlowNodeTraceInput = Readonly<{
	id: string;
	position: FlowPoint;
	label: string;
	ordinal: number;
}>;

type FlowTraceRect = Readonly<{
	left: number;
	right: number;
	top: number;
	bottom: number;
}>;

const FLOW_TRACE_CALLOUT_MARGIN = 12;

function overlapArea(left: FlowTraceRect, right: FlowTraceRect): number {
	const width = Math.max(
		0,
		Math.min(left.right, right.right) - Math.max(left.left, right.left),
	);
	const height = Math.max(
		0,
		Math.min(left.bottom, right.bottom) - Math.max(left.top, right.top),
	);
	return width * height;
}

/** Places compact trace callouts without detaching them from their nodes. */
export function placeFlowNodeTraceCallouts(
	inputs: readonly FlowNodeTraceInput[],
	options: Readonly<{
		reserved?: readonly FlowTraceRect[];
		nodePositions?: readonly FlowPoint[];
	}> = {},
): ReadonlyMap<string, FlowNodeTracePlacement> {
	const placements = new Map<string, FlowNodeTracePlacement>();
	const occupied: FlowTraceRect[] = [...(options.reserved ?? [])];
	const nodeBounds = (
		options.nodePositions ?? inputs.map(({ position }) => position)
	).map((position) => ({
		left: position.x - 34,
		right: position.x + 34,
		top: position.y - 34,
		bottom: position.y + 34,
	}));
	// Geometry must not depend on the current event focus. Reordering focused
	// nodes first made unrelated callouts jump between candidates on every
	// Detail step. Focus still receives its visual priority at render time;
	// placement stays deterministic for an unchanged node-state projection.
	const ordered = [...inputs].sort(
		(left, right) =>
			left.ordinal - right.ordinal || left.id.localeCompare(right.id),
	);
	for (const input of ordered) {
		// Reserve the phone breakpoint's 20-unit monospace glyph width and height;
		// desktop text is smaller, so this is a deliberately safe collision box.
		const width = Math.min(300, Math.max(70, input.label.length * 11.5));
		const verticalFirst = input.ordinal % 2 === 0;
		const nearCandidates = (
			verticalFirst
				? [
						[0, -48],
						[0, 55],
						[48, -38],
						[-48, 55],
						[-48, -38],
						[48, 55],
					]
				: [
						[0, 55],
						[0, -48],
						[-48, 55],
						[48, -38],
						[48, 55],
						[-48, -38],
					]
		) as readonly (readonly [number, number])[];
		const candidates = [
			...nearCandidates,
			[0, -78],
			[0, 85],
			[-48, -68],
			[48, -68],
			[-48, 82],
			[48, 82],
			[-82, -48],
			[82, -48],
			[-82, 58],
			[82, 58],
			[0, -108],
			[0, 115],
			[-96, -82],
			[96, -82],
			[-96, 92],
			[96, 92],
			[0, -138],
			[0, 145],
			[-140, -112],
			[140, -112],
			[-140, 122],
			[140, 122],
			[0, -168],
			[0, 175],
		] as readonly (readonly [number, number])[];
		const minimumX = FLOW_TRACE_CALLOUT_MARGIN + width / 2;
		const maximumX = FLOW_VIEWBOX_WIDTH - FLOW_TRACE_CALLOUT_MARGIN - width / 2;
		const globalColumns = Array.from(
			{ length: 7 },
			(_, index) => minimumX + ((maximumX - minimumX) * index) / 6,
		);
		const globalRows = Array.from(
			{ length: 15 },
			(_, index) => FLOW_TRACE_CALLOUT_MARGIN + 24 + index * 36,
		).filter((y) => y <= FLOW_VIEWBOX_HEIGHT - FLOW_TRACE_CALLOUT_MARGIN - 8);
		const exhaustiveCandidates = globalRows.flatMap((absoluteY) =>
			globalColumns.map(
				(absoluteX) =>
					[absoluteX - input.position.x, absoluteY - input.position.y] as const,
			),
		);
		let best:
			| Readonly<{
					x: number;
					y: number;
					rect: FlowTraceRect;
					score: number;
			  }>
			| undefined;
		for (const [offsetX, offsetY] of [...candidates, ...exhaustiveCandidates]) {
			const absoluteX = Math.max(
				FLOW_TRACE_CALLOUT_MARGIN + width / 2,
				Math.min(
					FLOW_VIEWBOX_WIDTH - FLOW_TRACE_CALLOUT_MARGIN - width / 2,
					input.position.x + offsetX,
				),
			);
			const absoluteY = Math.max(
				FLOW_TRACE_CALLOUT_MARGIN + 24,
				Math.min(
					FLOW_VIEWBOX_HEIGHT - FLOW_TRACE_CALLOUT_MARGIN - 8,
					input.position.y + offsetY,
				),
			);
			const rect = {
				left: absoluteX - width / 2 - 4,
				right: absoluteX + width / 2 + 4,
				top: absoluteY - 24,
				bottom: absoluteY + 8,
			};
			const occupiedOverlap = occupied.reduce(
				(sum, other) => sum + overlapArea(rect, other),
				0,
			);
			const nodeOverlap = nodeBounds.reduce(
				(sum, other) => sum + overlapArea(rect, other),
				0,
			);
			if (occupiedOverlap > 0 || nodeOverlap > 0) continue;
			const score = Math.hypot(
				absoluteX - input.position.x,
				absoluteY - input.position.y,
			);
			if (best === undefined || score < best.score) {
				best = { x: absoluteX, y: absoluteY, rect, score };
			}
		}
		if (best === undefined) continue;
		occupied.push(best.rect);
		const localX = best.x - input.position.x;
		const localY = best.y - input.position.y;
		const distance = Math.max(1, Math.hypot(localX, localY));
		placements.set(input.id, {
			x: localX,
			y: localY,
			leaderStart: {
				x: (localX * 30) / distance,
				y: (localY * 30) / distance,
			},
			leaderEnd: {
				x: localX * 0.72,
				y: localY * 0.72,
			},
		});
	}
	return placements;
}

export function FlowGraphNodeLayer({ state, selection }: FlowGraphLayerProps) {
	const plan = state.plan;
	const activeBundles = buildActiveFlowOverlayFeatureBundles(
		plan.overlayPresentation.activeFields,
	);
	const context = state.context;
	const positions = state.positions;
	const terminal = state.terminal;
	const gridgen = state.gridgen;
	const touchedNodeIds = new Set(
		ordinaryFlowEventEntityRefs(context).flatMap((entity) =>
			entity.kind === "node" ? [entity.node_id] : [],
		),
	);
	const changedNodeIds = new Set(
		context.traceEventSemantics?.changed_entity_refs.flatMap((entity) =>
			entity.kind === "node" ? [entity.node_id] : [],
		) ?? [],
	);
	const emphasizeTouchedNodes = shouldRenderFlowEventEntityEmphasis({
		level: plan.level,
		kind: "node",
		signal: "touch",
		memberCount: touchedNodeIds.size,
		totalCount: plan.nodes.length,
		structureLimit: FLOW_LOD_LIMITS.structureNodeEventLabels,
	});
	const emphasizeChangedNodes = shouldRenderFlowEventEntityEmphasis({
		level: plan.level,
		kind: "node",
		signal: "change",
		memberCount: changedNodeIds.size,
		totalCount: plan.nodes.length,
		structureLimit: FLOW_LOD_LIMITS.structureNodeEventLabels,
	});
	const nodeViews = plan.nodes.flatMap((node, ordinal) => {
		const position = positions.get(node.id);
		if (position === undefined) return [];
		const nodeBalance = BigInt(node.supply);
		const supernode = gridgen && node.id === "super";
		const kind: FlowGraphNodeKind =
			terminal?.source === node.id
				? "source"
				: terminal?.sink === node.id
					? "sink"
					: nodeBalance > 0n
						? "supply"
						: nodeBalance < 0n
							? "demand"
							: "normal";
		const semantic = projectFlowNodeSemanticState(state, node, kind, supernode);
		const rawEventTouched = touchedNodeIds.has(node.id);
		const rawEventChanged = changedNodeIds.has(node.id);
		const eventTouched = emphasizeTouchedNodes && rawEventTouched;
		const eventChanged = emphasizeChangedNodes && rawEventChanged;
		const selected = selection?.kind === "node" && selection.id === node.id;
		const showLabel =
			plan.nodeLabelIds.has(node.id) || eventTouched || selected;
		return [
			{
				node,
				ordinal,
				position,
				nodeBalance,
				supernode,
				kind,
				semantic,
				eventTouched,
				eventChanged,
				rawEventTouched,
				rawEventChanged,
				showLabel,
				selected,
			},
		];
	});
	const labeledTraceViews = nodeViews.filter(
		(view) => view.showLabel && view.semantic.traceLabel.length > 0,
	);
	const structureTraceNodeIds = new Set(
		[
			...labeledTraceViews.filter((view) => view.selected),
			...labeledTraceViews.filter((view) => view.eventChanged),
			...labeledTraceViews.filter((view) => view.eventTouched),
			...labeledTraceViews.filter(
				(view) => view.kind === "source" || view.kind === "sink",
			),
		]
			.filter(
				(view, index, candidates) =>
					candidates.findIndex(
						(candidate) => candidate.node.id === view.node.id,
					) === index,
			)
			.slice(0, FLOW_LOD_LIMITS.structureNodeTraceCallouts)
			.map((view) => view.node.id),
	);
	const traceViews = labeledTraceViews.filter(
		(view) =>
			plan.level === "detail" || structureTraceNodeIds.has(view.node.id),
	);
	const priorityTraceNodeId =
		traceViews.find((view) => view.selected)?.node.id ?? traceViews[0]?.node.id;
	const traceInputs = traceViews.map((view) => ({
		id: view.node.id,
		position: view.position,
		label: view.semantic.traceLabel,
		ordinal: view.ordinal,
	}));
	const compactEdgeLabels = state.originalVisuals.length > 16;
	const visibleEdgeLabelObstacles = state.originalVisuals.flatMap((visual) => {
		const selected = isOriginalEdgeSelected(selection, visual.edge.id);
		const eventFocused =
			visual.active ||
			visual.crossesCut ||
			ordinaryFlowEventEntityRefs(context).some(
				(entity) =>
					(entity.kind === "edge" || entity.kind === "residual-arc") &&
					entity.edge_id === visual.edge.id,
			);
		const stableContextLabel =
			!compactEdgeLabels &&
			visual.geometry.parallelCount === 1 &&
			plan.edgeLabelIds.has(visual.edge.id);
		if (
			!selected &&
			(!visual.geometry.labelCollisionFree ||
				(!eventFocused && !stableContextLabel))
		) {
			return [];
		}
		const centerY = visual.geometry.label.y + visual.geometry.labelYOffset;
		const clearance = 4;
		return [
			{
				left:
					visual.geometry.label.x -
					visual.geometry.labelBoxWidth / 2 -
					clearance,
				right:
					visual.geometry.label.x +
					visual.geometry.labelBoxWidth / 2 +
					clearance,
				top: centerY - visual.geometry.labelHeight / 2 - clearance,
				bottom: centerY + visual.geometry.labelHeight / 2 + clearance,
			},
		];
	});
	// Placement is a pure function of the current scene. Remembering the
	// previous frame made the same event render differently after Previous →
	// Next; stable ordering already prevents movement while inputs are unchanged.
	const tracePlacements = placeFlowNodeTraceCallouts(traceInputs, {
		reserved: visibleEdgeLabelObstacles,
		nodePositions: [...positions.values()],
	});
	return (
		<>
			{nodeViews.map((view) => {
				const {
					node,
					position,
					nodeBalance,
					supernode,
					kind,
					semantic,
					eventTouched,
					eventChanged,
					rawEventTouched,
					rawEventChanged,
					showLabel,
				} = view;
				const tracePlacement = tracePlacements.get(node.id);
				const showTraceCallout = tracePlacement !== undefined;
				const priorityTrace = node.id === priorityTraceNodeId;
				const cancelTightenNode = state.renderData.cancelTightenNodeById.get(
					node.id,
				);
				const relaxedMndcNode = state.renderData.relaxedMndcNodeById.get(
					node.id,
				);
				const tardosNode = state.renderData.tardosNodeById.get(node.id);
				const displayedPotential =
					cancelTightenNode !== undefined
						? formatFlowRational(cancelTightenNode.potential)
						: relaxedMndcNode !== undefined
							? `${relaxedMndcNode.left_dual} · ${relaxedMndcNode.right_dual}`
							: semantic.potential;
				const potentialLabel = (
					<text
						className={`flow-node-potential${relaxedMndcNode === undefined ? "" : " flow-node-assignment-duals"}`}
						textAnchor="middle"
						y={
							semantic.traceLabel.length > 0 &&
							showTraceCallout &&
							(tracePlacement?.y ?? 0) > 0
								? "78"
								: "51"
						}
					>
						{relaxedMndcNode !== undefined ? (
							<>
								L {relaxedMndcNode.left_dual} · R {relaxedMndcNode.right_dual}
							</>
						) : (
							<>
								{context.model.kind === "assignment"
									? context.model.tasks.includes(node.id)
										? "v"
										: "u"
									: "π"}{" "}
								{displayedPotential}
							</>
						)}
					</text>
				);
				const traceCallout =
					tracePlacement !== undefined && semantic.traceLabel.length > 0 ? (
						<g
							className={`flow-node-trace-callout${priorityTrace ? " flow-node-trace-priority" : ""}`}
							data-node-trace-for={node.id}
						>
							<line
								className="flow-node-trace-leader"
								x1={tracePlacement.leaderStart.x}
								y1={tracePlacement.leaderStart.y}
								x2={tracePlacement.leaderEnd.x}
								y2={tracePlacement.leaderEnd.y}
							/>
							<text
								className="flow-node-trace"
								textAnchor="middle"
								x={tracePlacement.x}
								y={tracePlacement.y}
							>
								{semantic.traceLabel}
							</text>
						</g>
					) : undefined;
				return (
					<FlowGraphRichNodeFrame
						key={node.id}
						state={state}
						node={node}
						position={position}
						kind={kind}
						selection={selection}
						traceCalloutExpected={
							plan.level === "structure"
								? structureTraceNodeIds.has(node.id)
								: priorityTrace
						}
						eventTouched={rawEventTouched}
						eventChanged={rawEventChanged}
						renderEventTouched={eventTouched}
						renderEventChanged={eventChanged}
					>
						<title>{semantic.title}</title>
						<circle className={semantic.nodeClassName} r="29" />
						<FlowGraphContinuousNodeFeatureBundle
							state={state}
							nodeId={node.id}
							enabled={activeBundles.has("node-continuous")}
						/>
						<FlowGraphOptimizationNodeFeatureBundle
							state={state}
							nodeId={node.id}
							supernode={supernode}
							enabled={activeBundles.has("node-optimization") || supernode}
						/>
						<FlowGraphSearchNodeFeatureBundle
							state={state}
							nodeId={node.id}
							kind={kind}
							nodeBalance={nodeBalance}
							overlayEnabled={
								activeBundles.has("node-search") ||
								state.visualization.ibfsView !== undefined
							}
						/>
						{showLabel && (
							<text
								className="flow-node-label"
								textAnchor="middle"
								dominantBaseline="central"
								y={nodeBalance === 0n ? undefined : "-7"}
							>
								{flowNodeCanvasLabel(node.id)}
							</text>
						)}
						{showLabel && nodeBalance !== 0n && (
							<text className="flow-node-balance" textAnchor="middle" y="13">
								{nodeBalance > 0n ? "+" : ""}
								{node.supply}
							</text>
						)}
						{showLabel &&
							semantic.eibfsNode !== undefined &&
							eibfsRootGlyph(semantic.eibfsNode) !== undefined && (
								<text
									className={`flow-eibfs-root-glyph flow-eibfs-root-glyph-${semantic.eibfsNode.root_kind}`}
									textAnchor="middle"
									x="25"
									y="-24"
								>
									{eibfsRootGlyph(semantic.eibfsNode)}
								</text>
							)}
						{traceCallout !== undefined && tardosNode !== undefined ? (
							<FlowGraphOverlayOwnedLeaves
								state={state}
								bundle="node-optimization"
								entity={{ kind: "node", id: node.id }}
								owners={[
									{
										overlay: "tardos_framework_overlay",
										role: "nodes.potential",
									},
								]}
							>
								{traceCallout}
							</FlowGraphOverlayOwnedLeaves>
						) : (
							traceCallout
						)}
						{showLabel &&
							displayedPotential !== undefined &&
							!semantic.solverPriceReplacesCertifiedPotential &&
							(cancelTightenNode !== undefined ? (
								<FlowGraphOverlayOwnedLeaves
									state={state}
									bundle="node-optimization"
									entity={{ kind: "node", id: node.id }}
									owners={[
										{
											overlay: "cancel_tighten_overlay",
											role: "nodes.potential",
										},
									]}
								>
									{potentialLabel}
								</FlowGraphOverlayOwnedLeaves>
							) : relaxedMndcNode !== undefined ? (
								<FlowGraphOverlayOwnedLeaves
									state={state}
									bundle="node-optimization"
									entity={{ kind: "node", id: node.id }}
									owners={[
										{
											overlay: "relaxed_mndc_overlay",
											role: "nodes.assignment-duals",
										},
									]}
								>
									{potentialLabel}
								</FlowGraphOverlayOwnedLeaves>
							) : (
								potentialLabel
							))}
						<circle className="flow-node-hit-target" r="42" />
					</FlowGraphRichNodeFrame>
				);
			})}
		</>
	);
}
