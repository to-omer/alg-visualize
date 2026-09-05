import { FlowGraphOverlayOwnedLeaves } from "./FlowGraphOverlayOwnedLeaves";
import { flowScopedSvgUrl, useFlowGraphIdScope } from "./flow-dom-id";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import {
	FLOW_NODE_RADIUS,
	FLOW_VIEWBOX_HEIGHT,
	FLOW_VIEWBOX_WIDTH,
	type FlowPoint,
} from "./flow-layout";
import { flowNodeCanvasLabel } from "./flow-node-display-label";
import type {
	FlowFeasibilityArcRefV1,
	FlowFeasibilityArcStateV1,
	FlowFeasibilityDomainEdgeV1,
	FlowFeasibilityNodeRefV1,
	FlowFeasibilityOverlayV2,
} from "./flow-scene";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;

function nodeKey(node: FlowFeasibilityNodeRefV1): string {
	return node.kind === "original"
		? `original:${node.original_node_id}`
		: `artificial:${node.kind}`;
}

function arcKey(arc: FlowFeasibilityArcRefV1): string {
	switch (arc.kind) {
		case "original":
			return `original:${arc.original_edge_id}`;
		case "lower-bound-return":
			return `return:${arc.return_from}:${arc.return_to}`;
		case "from-super-source":
			return `from-super-source:${arc.imbalance_node_id}`;
		case "to-super-sink":
			return `to-super-sink:${arc.imbalance_node_id}`;
	}
}

function chooseArtificialPosition(
	side: "left" | "right",
	positions: ReadonlyMap<string, FlowPoint>,
): FlowPoint {
	const x = side === "left" ? 44 : FLOW_VIEWBOX_WIDTH - 44;
	// The zoom controls occupy the upper-right corner of the SVG. Keep the
	// artificial sink out of that chrome while still maximizing graph clearance.
	const candidateRows =
		side === "left"
			? [52, 140, 270, 400, FLOW_VIEWBOX_HEIGHT - 52]
			: [140, 270, 400, FLOW_VIEWBOX_HEIGHT - 52];
	const candidates = candidateRows.map((y) => ({ x, y }));
	return candidates.reduce(
		(best, candidate) => {
			const clearance = [...positions.values()].reduce(
				(minimum, position) =>
					Math.min(
						minimum,
						(candidate.x - position.x) ** 2 + (candidate.y - position.y) ** 2,
					),
				Number.POSITIVE_INFINITY,
			);
			return clearance > best.clearance ? { candidate, clearance } : best;
		},
		{ candidate: candidates[0] as FlowPoint, clearance: -1 },
	).candidate;
}

function pointForNode(
	domainPositions: ReadonlyMap<string, FlowPoint>,
	node: FlowFeasibilityNodeRefV1,
	artificial: Readonly<{ source: FlowPoint; sink: FlowPoint }>,
): FlowPoint | undefined {
	if (node.kind === "super-source") return artificial.source;
	if (node.kind === "super-sink") return artificial.sink;
	return domainPositions.get(node.original_node_id ?? "");
}

function spreadRows(
	nodeIds: readonly string[],
	x: number,
): Map<string, FlowPoint> {
	const result = new Map<string, FlowPoint>();
	const top = 82;
	const bottom = FLOW_VIEWBOX_HEIGHT - 62;
	for (const [index, nodeId] of nodeIds.entries()) {
		result.set(nodeId, {
			x,
			y:
				nodeIds.length === 1
					? (top + bottom) / 2
					: top + ((bottom - top) * index) / (nodeIds.length - 1),
		});
	}
	return result;
}

function standaloneDomainPositions(
	overlay: FlowFeasibilityOverlayV2,
): Map<string, FlowPoint> {
	const incoming = new Map(
		overlay.domain.nodes.map((node) => [node.node_id, 0]),
	);
	const outgoing = new Map(incoming);
	for (const edge of overlay.domain.edges) {
		outgoing.set(edge.from_node_id, (outgoing.get(edge.from_node_id) ?? 0) + 1);
		incoming.set(edge.to_node_id, (incoming.get(edge.to_node_id) ?? 0) + 1);
	}
	const left = overlay.domain.nodes
		.filter(
			(node) =>
				(outgoing.get(node.node_id) ?? 0) > 0 &&
				(incoming.get(node.node_id) ?? 0) === 0,
		)
		.map((node) => node.node_id);
	const right = overlay.domain.nodes
		.filter(
			(node) =>
				(incoming.get(node.node_id) ?? 0) > 0 &&
				(outgoing.get(node.node_id) ?? 0) === 0,
		)
		.map((node) => node.node_id);
	const assigned = new Set([...left, ...right]);
	const middle = overlay.domain.nodes
		.map((node) => node.node_id)
		.filter((nodeId) => !assigned.has(nodeId));
	const result = spreadRows(left, 210);
	for (const [nodeId, point] of spreadRows(middle, 450)) {
		result.set(nodeId, point);
	}
	for (const [nodeId, point] of spreadRows(right, 690)) {
		result.set(nodeId, point);
	}
	return result;
}

function feasibilityDomainPositions(
	state: FlowEntityGraphState,
	overlay: FlowFeasibilityOverlayV2,
): Map<string, FlowPoint> {
	if (overlay.domain.kind === "standalone-transformation") {
		return standaloneDomainPositions(overlay);
	}
	const result = new Map<string, FlowPoint>();
	for (const node of overlay.domain.nodes) {
		const position = state.positions.get(node.public_node_id ?? "");
		if (position === undefined) {
			throw new Error(
				`Feasibility domain node ${node.node_id} has no validated public position`,
			);
		}
		result.set(node.node_id, position);
	}
	return result;
}

type FeasibilityArcGeometry = Readonly<{
	path: string;
	reversePath: string;
	label: FlowPoint;
}>;

function curvedGeometry(
	from: FlowPoint,
	to: FlowPoint,
	fromRadius: number,
	toRadius: number,
	bend: number,
): FeasibilityArcGeometry {
	const dx = to.x - from.x;
	const dy = to.y - from.y;
	const distance = Math.max(1, Math.hypot(dx, dy));
	const unitX = dx / distance;
	const unitY = dy / distance;
	const normalX = -unitY;
	const normalY = unitX;
	const start = {
		x: from.x + unitX * fromRadius,
		y: from.y + unitY * fromRadius,
	};
	const end = {
		x: to.x - unitX * toRadius,
		y: to.y - unitY * toRadius,
	};
	const label = {
		x: (start.x + end.x) / 2 + normalX * bend,
		y: (start.y + end.y) / 2 + normalY * bend,
	};
	return {
		path: `M ${start.x} ${start.y} Q ${label.x} ${label.y} ${end.x} ${end.y}`,
		reversePath: `M ${end.x} ${end.y} Q ${label.x} ${label.y} ${start.x} ${start.y}`,
		label,
	};
}

function transformedEdgeBend(
	domainPositions: ReadonlyMap<string, FlowPoint>,
	fromNodeId: string,
	toNodeId: string,
	lane: number,
): number {
	const from = domainPositions.get(fromNodeId);
	const to = domainPositions.get(toNodeId);
	if (from === undefined || to === undefined) return lane * 14;
	const dx = to.x - from.x;
	const dy = to.y - from.y;
	const distance = Math.max(1, Math.hypot(dx, dy));
	const nearbyOffsets = [...domainPositions.entries()].flatMap(
		([nodeId, position]) => {
			if (nodeId === fromNodeId || nodeId === toNodeId) return [];
			const offsetX = position.x - from.x;
			const offsetY = position.y - from.y;
			const projection = (offsetX * dx + offsetY * dy) / (distance * distance);
			if (projection <= 0.08 || projection >= 0.92) return [];
			const perpendicular = (-dy * offsetX + dx * offsetY) / distance;
			return Math.abs(perpendicular) < FLOW_NODE_RADIUS + 28
				? [perpendicular]
				: [];
		},
	);
	if (nearbyOffsets.length === 0) {
		return Math.max(-58, Math.min(58, lane * 14));
	}
	const occupiedSide = nearbyOffsets.reduce((sum, offset) => sum + offset, 0);
	const away = occupiedSide >= 0 ? -1 : 1;
	return Math.max(
		-150,
		Math.min(150, away * (112 + Math.abs(lane) * 18) + lane * 14),
	);
}

function geometryForArc(
	state: FlowEntityGraphState,
	domainEdgeById: ReadonlyMap<string, FlowFeasibilityDomainEdgeV1>,
	domainPositions: ReadonlyMap<string, FlowPoint>,
	arc: FlowFeasibilityArcStateV1,
	artificial: Readonly<{ source: FlowPoint; sink: FlowPoint }>,
	ordinal: number,
	parallelCount: number,
): FeasibilityArcGeometry {
	if (arc.arc.kind === "original") {
		const edge = domainEdgeById.get(arc.arc.original_edge_id ?? "");
		if (edge === undefined) {
			throw new Error("Feasibility arc has no exact input-domain edge");
		}
		if (edge.public_route_edge_id !== undefined) {
			const route = state.layout.routes.get(edge.public_route_edge_id);
			if (route === undefined) {
				throw new Error(
					"Feasibility public route projection has no layout route",
				);
			}
			return {
				path: route.path,
				reversePath: route.reversePath,
				label: route.label,
			};
		}
		const from = domainPositions.get(edge.from_node_id);
		const to = domainPositions.get(edge.to_node_id);
		if (from === undefined || to === undefined) {
			throw new Error("Feasibility transformed edge has no domain geometry");
		}
		const lane = ordinal - (parallelCount - 1) / 2;
		return curvedGeometry(
			from,
			to,
			FLOW_NODE_RADIUS - 5,
			FLOW_NODE_RADIUS - 2,
			transformedEdgeBend(
				domainPositions,
				edge.from_node_id,
				edge.to_node_id,
				lane,
			),
		);
	}
	const from = pointForNode(domainPositions, arc.from, artificial);
	const to = pointForNode(domainPositions, arc.to, artificial);
	if (from === undefined || to === undefined) {
		throw new Error("Feasibility auxiliary arc has no domain geometry");
	}
	const fromRadius = arc.from.kind === "original" ? FLOW_NODE_RADIUS + 9 : 30;
	const toRadius = arc.to.kind === "original" ? FLOW_NODE_RADIUS + 13 : 34;
	const lane = ordinal - (parallelCount - 1) / 2;
	const bend =
		arc.arc.kind === "lower-bound-return"
			? from.y + to.y < FLOW_VIEWBOX_HEIGHT
				? -94
				: 94
			: Math.max(-54, Math.min(54, lane * 13));
	return curvedGeometry(from, to, fromRadius, toRadius, bend);
}

function nodeLabel(node: FlowFeasibilityNodeRefV1): string {
	if (node.kind === "super-source") return "SUPER SOURCE";
	if (node.kind === "super-sink") return "SUPER SINK";
	return flowNodeCanvasLabel(node.original_node_id ?? "");
}

function stageLabel(stage: string): string {
	return stage.replaceAll("-", " ").toUpperCase();
}

function feasibilityUseLabel(
	useKind: FlowFeasibilityOverlayV2["use_kind"],
): string {
	switch (useKind) {
		case "initial-flow":
			return "INITIAL FLOW";
		case "precheck-only":
			return "PRECHECK";
		case "anchored-recovery":
			return "RECOVERY";
	}
}

function feasibilityArcLabel(arc: FlowFeasibilityArcStateV1, point: FlowPoint) {
	const label = `FLOW ${arc.flow}   CAPACITY ${arc.capacity}`;
	const width = Math.max(132, Math.min(236, label.length * 6.4 + 20));
	const x = Math.max(
		10 + width / 2,
		Math.min(FLOW_VIEWBOX_WIDTH - 10 - width / 2, point.x),
	);
	const y = Math.max(43, Math.min(FLOW_VIEWBOX_HEIGHT - 40, point.y));
	return (
		<g
			className="flow-feasibility-arc-label"
			transform={`translate(${x - width / 2} ${y - 13})`}
		>
			<rect width={width} height="26" rx="7" />
			<text x={width / 2} y="13" dominantBaseline="central" textAnchor="middle">
				{label}
			</text>
		</g>
	);
}

/**
 * Renders the actual lower-bound feasibility network without pretending its
 * artificial vertices and arcs are part of the input graph.
 */
export function FlowGraphFeasibilityLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.feasibility;
	const idScope = useFlowGraphIdScope();
	if (overlay === undefined) return null;
	const domainPositions = feasibilityDomainPositions(state, overlay);
	const transformed = overlay.domain.kind !== "public-input";
	const artificial = {
		source: chooseArtificialPosition("left", domainPositions),
		sink: chooseArtificialPosition("right", domainPositions),
	};
	const maximumCapacity = overlay.arcs.reduce((maximum, arc) => {
		const capacity = BigInt(arc.capacity);
		return capacity > maximum ? capacity : maximum;
	}, 1n);
	const auxiliaryArcs = overlay.arcs.filter(
		(arc) => arc.arc.kind !== "original",
	);
	const focusedOriginal = overlay.arcs.find(
		(arc) => arc.arc.kind === "original" && arc.focused,
	);
	const originalArcs = transformed
		? overlay.arcs.filter((arc) => arc.arc.kind === "original")
		: focusedOriginal === undefined
			? []
			: [focusedOriginal];
	const domainEdgeById = new Map(
		overlay.domain.edges.map((edge) => [edge.edge_id, edge]),
	);
	const laneGroups = new Map<string, string[]>();
	for (const edge of overlay.domain.edges) {
		const groupKey = `original\u0000${edge.from_node_id}\u0000${edge.to_node_id}`;
		const identities = laneGroups.get(groupKey) ?? [];
		identities.push(edge.edge_id);
		laneGroups.set(groupKey, identities);
	}
	for (const arc of auxiliaryArcs) {
		const groupKey = `auxiliary\u0000${nodeKey(arc.from)}\u0000${nodeKey(arc.to)}`;
		const identities = laneGroups.get(groupKey) ?? [];
		identities.push(arcKey(arc.arc));
		laneGroups.set(groupKey, identities);
	}
	const arcLane = (arc: FlowFeasibilityArcStateV1) => {
		const domainEdge =
			arc.arc.kind === "original"
				? domainEdgeById.get(arc.arc.original_edge_id ?? "")
				: undefined;
		const groupKey =
			domainEdge === undefined
				? `auxiliary\u0000${nodeKey(arc.from)}\u0000${nodeKey(arc.to)}`
				: `original\u0000${domainEdge.from_node_id}\u0000${domainEdge.to_node_id}`;
		const peers = laneGroups.get(groupKey) ?? [];
		const identity = domainEdge?.edge_id ?? arcKey(arc.arc);
		const ordinal = peers.indexOf(identity);
		if (ordinal < 0) {
			throw new Error("Feasibility arc is absent from its domain lane set");
		}
		return { ordinal, count: peers.length };
	};
	const renderedArcs = [
		...auxiliaryArcs.map((arc) => {
			const lane = arcLane(arc);
			return {
				arc,
				geometry: geometryForArc(
					state,
					domainEdgeById,
					domainPositions,
					arc,
					artificial,
					lane.ordinal,
					lane.count,
				),
				context: true,
			};
		}),
		...originalArcs.map((arc) => {
			const lane = arcLane(arc);
			return {
				arc,
				geometry: geometryForArc(
					state,
					domainEdgeById,
					domainPositions,
					arc,
					artificial,
					lane.ordinal,
					lane.count,
				),
				context: transformed,
			};
		}),
	];
	const focusedNodeKey =
		overlay.focus_node === undefined ? undefined : nodeKey(overlay.focus_node);
	const visibleArtificialNodeKeys = new Set(
		overlay.arcs
			.flatMap((arc) => [arc.from, arc.to])
			.filter((node) => node.kind !== "original")
			.map(nodeKey),
	);
	if (BigInt(overlay.total_required) > 0n) {
		for (const node of overlay.nodes) {
			if (node.node.kind !== "original") {
				visibleArtificialNodeKeys.add(nodeKey(node.node));
			}
		}
	}
	if (
		overlay.focus_node !== undefined &&
		overlay.focus_node.kind !== "original"
	) {
		visibleArtificialNodeKeys.add(nodeKey(overlay.focus_node));
	}
	const visibleOriginalNodeStates = overlay.nodes.filter(
		(node) =>
			node.node.kind === "original" &&
			(node.active ||
				node.reachable ||
				BigInt(node.excess) > 0n ||
				nodeKey(node.node) === focusedNodeKey),
	);
	const stage = stageLabel(overlay.stage);
	const status = `${feasibilityUseLabel(overlay.use_kind)}${transformed ? " · INTERNAL NETWORK" : ""} · ${stage} · ROUTED ${overlay.routed} / ${overlay.total_required}`;
	const statusWidth = Math.max(210, Math.min(480, status.length * 6.6 + 22));

	return (
		<g
			className="flow-feasibility-layer"
			data-feasibility-use={overlay.use_kind}
			data-feasibility-domain={overlay.domain.kind}
			data-feasibility-domain-node-count={overlay.domain.nodes.length}
			data-feasibility-rendered-original-arc-count={originalArcs.length}
			data-feasibility-stage={overlay.stage}
			data-feasibility-routed={`${overlay.routed}:${overlay.total_required}`}
		>
			{overlay.domain.kind === "standalone-transformation" && (
				<g className="flow-feasibility-internal-backdrop">
					<rect x="132" y="48" width="636" height="444" rx="18" />
					<text x="151" y="71">
						ALGORITHM-OWNED RECOVERY NETWORK
					</text>
				</g>
			)}
			<g
				className="flow-feasibility-status"
				transform={`translate(${FLOW_VIEWBOX_WIDTH / 2 - statusWidth / 2} 12)`}
			>
				<rect width={statusWidth} height="25" rx="7" />
				<text
					x={statusWidth / 2}
					y="12.5"
					dominantBaseline="central"
					textAnchor="middle"
				>
					{status}
				</text>
			</g>

			{renderedArcs.map(({ arc, geometry, context }) => {
				const capacity = BigInt(arc.capacity);
				const flow = BigInt(arc.flow);
				const capacityWidth =
					3 + Number((capacity * 4_000n) / maximumCapacity) / 1_000;
				const flowWidth =
					flow === 0n || capacity === 0n
						? 0
						: 1.4 +
							Number(
								(flow * BigInt(Math.round((capacityWidth - 1.4) * 1_000))) /
									capacity,
							) /
								1_000;
				const focusDirection = arc.focused_direction;
				const residualDirection =
					focusDirection === "reverse"
						? ("reverse" as const)
						: ("forward" as const);
				const focusPath =
					residualDirection === "reverse"
						? geometry.reversePath
						: geometry.path;
				const identity = arcKey(arc.arc);
				const originalEdgeId =
					arc.arc.kind === "original" ? arc.arc.original_edge_id : undefined;
				const publicOriginal =
					overlay.domain.kind === "public-input" &&
					originalEdgeId !== undefined;
				const entityKind = !publicOriginal
					? arc.focused
						? ("auxiliary-residual-arc" as const)
						: ("auxiliary-edge" as const)
					: arc.focused
						? ("residual-arc" as const)
						: ("edge" as const);
				return (
					<FlowGraphOverlayOwnedLeaves
						key={identity}
						state={state}
						bundle="feasibility"
						entity={{
							kind: entityKind,
							id: publicOriginal ? originalEdgeId : identity,
							...(arc.focused ? { direction: residualDirection } : {}),
						}}
						owners={[
							{
								overlay: "feasibility_overlay",
								role: context ? `arcs.${arc.arc.kind}` : "focus_arc.original",
							},
						]}
					>
						<g
							className={`flow-feasibility-arc${arc.focused ? " is-focused" : ""}`}
							data-feasibility-arc={identity}
							data-feasibility-arc-kind={arc.arc.kind}
							data-feasibility-from={nodeKey(arc.from)}
							data-feasibility-to={nodeKey(arc.to)}
							data-feasibility-flow={`${arc.flow}:${arc.capacity}`}
							data-feasibility-focus={arc.focused_direction}
						>
							<title>{`${arc.arc.kind}: ${nodeLabel(arc.from)} to ${nodeLabel(arc.to)}; flow ${arc.flow} of capacity ${arc.capacity}${arc.focused ? `; inspecting ${arc.focused_direction} residual direction` : ""}`}</title>
							<path
								className="flow-feasibility-arc-underlay"
								d={geometry.path}
							/>
							{context && (
								<path
									className="flow-feasibility-capacity"
									d={geometry.path}
									style={{ strokeWidth: capacityWidth }}
									markerEnd={flowScopedSvgUrl(
										idScope,
										"flow-arrow-feasibility-capacity",
									)}
								/>
							)}
							{context && flowWidth > 0 && (
								<path
									className="flow-feasibility-flow"
									d={geometry.path}
									style={{ strokeWidth: flowWidth }}
									markerEnd={flowScopedSvgUrl(
										idScope,
										"flow-arrow-feasibility-flow",
									)}
								/>
							)}
							{arc.focused && (
								<path
									className="flow-feasibility-focus"
									d={focusPath}
									markerEnd={flowScopedSvgUrl(
										idScope,
										"flow-arrow-feasibility-focus",
									)}
								/>
							)}
							{arc.focused && feasibilityArcLabel(arc, geometry.label)}
						</g>
					</FlowGraphOverlayOwnedLeaves>
				);
			})}

			{overlay.domain.kind === "standalone-transformation" &&
				overlay.nodes
					.filter((node) => node.node.kind === "original")
					.map((node) => {
						const nodeId = node.node.original_node_id ?? "";
						const position = domainPositions.get(nodeId);
						if (position === undefined) {
							throw new Error(
								"Standalone feasibility node has no layout point",
							);
						}
						const focused = nodeKey(node.node) === focusedNodeKey;
						return (
							<FlowGraphOverlayOwnedLeaves
								key={`domain:${nodeId}`}
								state={state}
								bundle="feasibility"
								entity={{ kind: "auxiliary-node", id: `domain:${nodeId}` }}
								owners={[
									{
										overlay: "feasibility_overlay",
										role: "domain.nodes",
									},
								]}
							>
								<g
									className={`flow-feasibility-domain-node${focused ? " is-focused" : ""}`}
									data-feasibility-domain-node={nodeId}
								>
									<title>{`${nodeId}; internal recovery node; height ${node.height}; excess ${node.excess}`}</title>
									<circle cx={position.x} cy={position.y} r="20" />
									<text
										x={position.x}
										y={position.y}
										dominantBaseline="central"
										textAnchor="middle"
									>
										{flowNodeCanvasLabel(nodeId)}
									</text>
								</g>
							</FlowGraphOverlayOwnedLeaves>
						);
					})}

			{overlay.nodes
				.filter(
					(node) =>
						node.node.kind !== "original" &&
						visibleArtificialNodeKeys.has(nodeKey(node.node)),
				)
				.map((node) => {
					const position = pointForNode(domainPositions, node.node, artificial);
					if (position === undefined) {
						throw new Error("Feasibility terminal has no layout point");
					}
					const source = node.node.kind === "super-source";
					const identity = nodeKey(node.node);
					const focused = identity === focusedNodeKey;
					const points = [
						`${position.x - 25},${position.y}`,
						`${position.x - 13},${position.y - 22}`,
						`${position.x + 13},${position.y - 22}`,
						`${position.x + 25},${position.y}`,
						`${position.x + 13},${position.y + 22}`,
						`${position.x - 13},${position.y + 22}`,
					].join(" ");
					return (
						<FlowGraphOverlayOwnedLeaves
							key={identity}
							state={state}
							bundle="feasibility"
							entity={{ kind: "auxiliary-node", id: identity }}
							owners={[
								{
									overlay: "feasibility_overlay",
									role: `nodes.${node.node.kind}`,
								},
							]}
						>
							<g
								className={`flow-feasibility-terminal ${source ? "is-source" : "is-sink"}${focused ? " is-focused" : ""}`}
								data-feasibility-node={identity}
								data-feasibility-height={node.height}
								data-feasibility-excess={node.excess}
							>
								<title>{`${nodeLabel(node.node)}; height ${node.height}; excess ${node.excess}`}</title>
								<polygon
									className="flow-feasibility-terminal-shape"
									points={points}
								/>
								<text
									className="flow-feasibility-terminal-role"
									x={position.x}
									y={position.y - 4}
									dominantBaseline="central"
									textAnchor="middle"
								>
									{source ? "SS" : "ST"}
								</text>
								<text
									className="flow-feasibility-terminal-value"
									x={position.x}
									y={position.y + 10}
									dominantBaseline="central"
									textAnchor="middle"
								>
									{`EX ${node.excess}`}
								</text>
							</g>
						</FlowGraphOverlayOwnedLeaves>
					);
				})}

			{visibleOriginalNodeStates.map((node) => {
				const position = domainPositions.get(node.node.original_node_id ?? "");
				if (position === undefined) {
					throw new Error("Feasibility node state has no layout point");
				}
				const identity = nodeKey(node.node);
				const focused = identity === focusedNodeKey;
				const queueLabel =
					node.queue_position === undefined
						? undefined
						: `FIFO ${BigInt(node.queue_position) + 1n}`;
				const valueLabel =
					queueLabel ??
					(node.reachable
						? "CUT SIDE"
						: BigInt(node.excess) > 0n
							? `EXCESS ${node.excess}`
							: `HEIGHT ${node.height}`);
				const width = Math.max(60, valueLabel.length * 6.2 + 15);
				const labelX = Math.max(
					8,
					Math.min(FLOW_VIEWBOX_WIDTH - width - 8, position.x + 22),
				);
				const labelY = Math.max(42, position.y - 42);
				return (
					<FlowGraphOverlayOwnedLeaves
						key={identity}
						state={state}
						bundle="feasibility"
						entity={
							overlay.domain.kind === "standalone-transformation"
								? { kind: "auxiliary-node", id: identity }
								: {
										kind: "node",
										id: node.node.original_node_id ?? identity,
									}
						}
						owners={[
							{
								overlay: "feasibility_overlay",
								role: focused
									? "focus_node"
									: node.active
										? "active_queue"
										: node.reachable
											? "cut_reachable"
											: "node_excess",
							},
						]}
					>
						<g
							className={`flow-feasibility-node-state${focused ? " is-focused" : ""}${node.active ? " is-active" : ""}${node.reachable ? " is-reachable" : ""}`}
							data-feasibility-node={node.node.original_node_id}
							data-feasibility-queue-position={node.queue_position}
							data-feasibility-excess={node.excess}
							data-feasibility-height={node.height}
						>
							<title>{`${nodeLabel(node.node)}; ${valueLabel.toLowerCase()}; height ${node.height}; excess ${node.excess}`}</title>
							<circle
								className="flow-feasibility-node-ring"
								cx={position.x}
								cy={position.y}
								r={FLOW_NODE_RADIUS + 8}
							/>
							<path
								className="flow-feasibility-node-leader"
								d={`M ${position.x + 19} ${position.y - 23} L ${labelX} ${labelY + 10}`}
							/>
							<rect
								className="flow-feasibility-node-badge"
								x={labelX}
								y={labelY}
								width={width}
								height="20"
								rx="6"
							/>
							<text
								className="flow-feasibility-node-badge-label"
								x={labelX + width / 2}
								y={labelY + 10}
								dominantBaseline="central"
								textAnchor="middle"
							>
								{valueLabel}
							</text>
						</g>
					</FlowGraphOverlayOwnedLeaves>
				);
			})}
		</g>
	);
}
