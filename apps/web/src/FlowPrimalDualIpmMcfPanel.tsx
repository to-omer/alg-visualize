import {
	type PointerEvent as ReactPointerEvent,
	useEffect,
	useRef,
	useState,
} from "react";

import {
	flowScopedDomId,
	flowScopedSvgUrl,
	useFlowDomIdScope,
} from "./flow-dom-id";
import { buildFlowPanelEdgeRoutes } from "./flow-panel-edge-routing";
import type {
	FlowCurrentSceneV9,
	FlowPrimalDualIpmMcfOverlayV1,
} from "./flow-scene";

type Point = { x: number; y: number };
type FlowGraph = FlowCurrentSceneV9["graph"];
type PositionGraph = Readonly<{
	nodes: readonly Readonly<{ id: string }>[];
	edges: readonly Readonly<{
		id: string;
		from: string;
		to: string;
	}>[];
}>;
type PositionOverlay = Readonly<{
	nodes: readonly Readonly<{
		auxiliary_id: string;
		kind: string;
		original_edge_id?: string;
	}>[];
}>;

const WIDTH = 920;
const HEIGHT = 410;

function revealFocusedArcLabel(target: Element, viewport: HTMLElement): void {
	const arc = target.closest<SVGGElement>(".flow-ipm-arc");
	const label = arc?.querySelector<SVGGElement>(".flow-ipm-arc-label");
	if (label === undefined || label === null) return;
	const labelBounds = label.getBoundingClientRect();
	const viewportBounds = viewport.getBoundingClientRect();
	const labelCenter = labelBounds.left + labelBounds.width / 2;
	const viewportCenter = viewportBounds.left + viewportBounds.width / 2;
	viewport.scrollLeft += labelCenter - viewportCenter;
}

function compactInteger(value: string): string {
	const negative = value.startsWith("-");
	const digits = negative ? value.slice(1) : value;
	if (digits.length <= 11) return value;
	return `${negative ? "−" : ""}${digits.slice(0, 4)}…${digits.slice(-3)} · 10^${digits.length - 1}`;
}

function magnitudeBand(value: bigint, maximum: bigint): number {
	const magnitude = value < 0n ? -value : value;
	if (magnitude === 0n || maximum === 0n) return 0;
	return Math.max(
		1,
		Math.min(4, Number((magnitude * 4n + maximum - 1n) / maximum)),
	);
}

function originalPositions(nodeIds: readonly string[]): Map<string, Point> {
	const positions = new Map<string, Point>();
	if (nodeIds.length === 1) {
		positions.set(`node:${nodeIds[0]}`, { x: WIDTH / 2, y: HEIGHT / 2 });
		return positions;
	}
	const center = { x: WIDTH / 2, y: HEIGHT / 2 };
	const radius = { x: 335, y: 128 };
	for (const [index, id] of nodeIds.entries()) {
		const angle = Math.PI + (index * Math.PI * 2) / nodeIds.length;
		positions.set(`node:${id}`, {
			x: center.x + Math.cos(angle) * radius.x,
			y: center.y + Math.sin(angle) * radius.y,
		});
	}
	return positions;
}

function compareText(left: string, right: string): number {
	return left < right ? -1 : left > right ? 1 : 0;
}

function unorderedEndpointKey(from: string, to: string): string {
	return from < to ? `${from}\u0000${to}` : `${to}\u0000${from}`;
}

function clampAuxiliaryPoint(point: Point): Point {
	const margin = 54;
	return {
		x: Math.min(WIDTH - margin, Math.max(margin, point.x)),
		y: Math.min(HEIGHT - margin, Math.max(margin, point.y)),
	};
}

/** Stable geometric placement: unrelated edges never move when overlay order changes. */
export function buildPrimalDualIpmAuxiliaryPositions(
	graph: PositionGraph,
	overlay: PositionOverlay,
): Map<string, Point> {
	const positions = originalPositions(graph.nodes.map((node) => node.id));
	const edgeById = new Map(graph.edges.map((edge) => [edge.id, edge]));
	const capacityNodes = overlay.nodes.filter(
		(node) => node.kind === "capacity",
	);
	const groups = new Map<string, typeof capacityNodes>();
	for (const node of capacityNodes) {
		const edge = edgeById.get(node.original_edge_id ?? "");
		if (edge === undefined) continue;
		const key = unorderedEndpointKey(edge.from, edge.to);
		const group = groups.get(key);
		if (group === undefined) groups.set(key, [node]);
		else group.push(node);
	}
	for (const group of groups.values()) {
		const ordered = [...group].sort(
			(left, right) =>
				compareText(
					left.original_edge_id ?? "",
					right.original_edge_id ?? "",
				) || compareText(left.auxiliary_id, right.auxiliary_id),
		);
		for (const [index, node] of ordered.entries()) {
			const edge = edgeById.get(node.original_edge_id ?? "");
			const from =
				edge === undefined ? undefined : positions.get(`node:${edge.from}`);
			const to =
				edge === undefined ? undefined : positions.get(`node:${edge.to}`);
			if (edge === undefined || from === undefined || to === undefined)
				continue;
			if (edge.from === edge.to) {
				const angle =
					-Math.PI / 2 + (index * Math.PI * 2) / Math.max(ordered.length, 1);
				positions.set(
					node.auxiliary_id,
					clampAuxiliaryPoint({
						x: from.x + Math.cos(angle) * 78,
						y: from.y + Math.sin(angle) * 64,
					}),
				);
				continue;
			}
			const low = edge.from < edge.to ? edge.from : edge.to;
			const high = edge.from < edge.to ? edge.to : edge.from;
			const lowPosition = positions.get(`node:${low}`);
			const highPosition = positions.get(`node:${high}`);
			if (lowPosition === undefined || highPosition === undefined) continue;
			const dx = highPosition.x - lowPosition.x;
			const dy = highPosition.y - lowPosition.y;
			const length = Math.max(Math.hypot(dx, dy), 1);
			const normal = { x: -dy / length, y: dx / length };
			const midpoint = {
				x: (lowPosition.x + highPosition.x) / 2,
				y: (lowPosition.y + highPosition.y) / 2,
			};
			const outwardDot =
				normal.x * (midpoint.x - WIDTH / 2) +
				normal.y * (midpoint.y - HEIGHT / 2);
			const outwardSign =
				Math.abs(outwardDot) > 1
					? Math.sign(outwardDot)
					: compareText(low, high) <= 0
						? 1
						: -1;
			const spread = Math.min(46, 150 / Math.max(ordered.length - 1, 1));
			const lane =
				outwardSign * 26 + (index - (ordered.length - 1) / 2) * spread;
			positions.set(
				node.auxiliary_id,
				clampAuxiliaryPoint({
					x: midpoint.x + normal.x * lane,
					y: midpoint.y + normal.y * lane,
				}),
			);
		}
	}
	return positions;
}

function phaseDescription(
	stage: FlowPrimalDualIpmMcfOverlayV1["stage"],
): string {
	if (
		["ready", "normalize-input", "build-capacity-reduction"].includes(stage)
	) {
		return "Expand finite capacities into capacity nodes and upper/lower arcs";
	}
	if (
		["initialize-central-point", "build-minor", "decrease-mu"].includes(stage)
	) {
		return "Update the integral central path and sticky minor";
	}
	if (
		[
			"inspect-forest-subset",
			"build-low-stretch-forest",
			"sample-fundamental-cycle",
			"centering-cycle-update",
			"centered",
		].includes(stage)
	) {
		return "Sample a weighted fundamental cycle from the minimum-condition forest";
	}
	if (
		["proxy-reached", "crossover-grow-cut", "restore-original-dual"].includes(
			stage,
		)
	) {
		return "Cross through the proxy to the original-cost dual using nested cuts";
	}
	return "Recover and verify the integral optimum on the zero-reduced-cost network";
}

export function FlowPrimalDualIpmMcfPanel({
	graph,
	overlay,
}: {
	graph: FlowGraph;
	overlay: FlowPrimalDualIpmMcfOverlayV1;
}) {
	const idScope = useFlowDomIdScope("flow-ipm-mcf");
	const titleId = flowScopedDomId(idScope, "title");
	const descriptionId = flowScopedDomId(idScope, "description");
	const arrowId = flowScopedDomId(idScope, "arrow");
	const cycleArrowId = flowScopedDomId(idScope, "cycle-arrow");
	const graphViewportRef = useRef<HTMLDivElement>(null);
	const [hoveredArcId, setHoveredArcId] = useState<string>();
	useEffect(() => {
		const viewport = graphViewportRef.current;
		if (viewport === null) return undefined;
		const handleFocus = (event: FocusEvent) => {
			if (event.target instanceof Element) {
				revealFocusedArcLabel(event.target, viewport);
			}
		};
		viewport.addEventListener("focusin", handleFocus);
		return () => viewport.removeEventListener("focusin", handleFocus);
	}, []);
	const positions = buildPrimalDualIpmAuxiliaryPositions(graph, overlay);
	const forestCandidateCount = overlay.arcs.filter(
		(arc) => arc.forest_candidate,
	).length;
	const forestSubsetLabel =
		overlay.forest_subset_serial === undefined
			? undefined
			: `CANDIDATE SUBSET #${overlay.forest_subset_serial} · ${forestCandidateCount === 0 ? "∅" : `${forestCandidateCount} AUX ARC${forestCandidateCount === 1 ? "" : "S"}`}`;
	const labelledArcIds = overlay.arcs
		.filter(
			(arc, index) =>
				arc.auxiliary_id === overlay.sampled_arc ||
				arc.forest_candidate ||
				arc.deleted ||
				arc.contracted ||
				(overlay.sampled_arc === undefined && index === 0),
		)
		.map((arc) => arc.auxiliary_id);
	const labelledArcIdSet = new Set(labelledArcIds);
	const arcRoutes = buildFlowPanelEdgeRoutes(
		overlay.arcs.map((arc) => ({
			id: arc.auxiliary_id,
			from: arc.from,
			to: arc.to,
		})),
		positions,
		{
			width: WIDTH,
			height: HEIGHT,
			paddingX: 38,
			paddingY: 38,
			laneSpacing: 48,
			nodeRadius: 32,
			markerClearance: 12,
			labelWidth: 144,
			labelHeight: 36,
			labelEdgeIds: labelledArcIds,
		},
	);
	const maximumFlow = overlay.arcs.reduce(
		(maximum, arc) => (BigInt(arc.flow) > maximum ? BigInt(arc.flow) : maximum),
		0n,
	);
	const maximumSlack = overlay.arcs.reduce(
		(maximum, arc) =>
			BigInt(arc.slack) > maximum ? BigInt(arc.slack) : maximum,
		0n,
	);
	const maximumResistance = overlay.arcs.reduce((maximum, arc) => {
		const resistance = BigInt(arc.resistance ?? "0");
		return resistance > maximum ? resistance : maximum;
	}, 0n);
	const maximumPotential = overlay.nodes.reduce((maximum, node) => {
		const potential = BigInt(node.potential);
		const magnitude = potential < 0n ? -potential : potential;
		return magnitude > maximum ? magnitude : maximum;
	}, 0n);
	const proxyThresholdNumerator =
		BigInt(overlay.beta) * BigInt(overlay.gamma) * 4n;
	const condition = overlay.tree_condition_number;
	const handleArcPointerMove = (event: ReactPointerEvent<SVGSVGElement>) => {
		const target = event.target;
		const hitArc =
			target instanceof Element
				? target.closest<SVGGElement>(".flow-ipm-arc")
				: null;
		const hitArcId = hitArc?.dataset.ipmArc;
		const matrix = event.currentTarget.getScreenCTM();
		if (matrix === null) {
			setHoveredArcId(hitArcId);
			return;
		}
		const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(
			matrix.inverse(),
		);
		let resolvedArcId = hitArcId;
		let nearestMidpointDistanceSquared = Number.POSITIVE_INFINITY;
		for (const arc of event.currentTarget.querySelectorAll<SVGGElement>(
			".flow-ipm-arc",
		)) {
			const path = arc.querySelector<SVGPathElement>(".flow-ipm-primary-arc");
			const arcId = arc.dataset.ipmArc;
			if (path === null || arcId === undefined) continue;
			const midpoint = path.getPointAtLength(path.getTotalLength() / 2);
			const dx = point.x - midpoint.x;
			const dy = point.y - midpoint.y;
			const distanceSquared = dx * dx + dy * dy;
			if (distanceSquared < nearestMidpointDistanceSquared) {
				nearestMidpointDistanceSquared = distanceSquared;
				resolvedArcId = arcId;
			}
		}
		// Away from an arc midpoint, ordinary SVG hit-testing is unambiguous and
		// should win. Near a crossing or a node label drawn above the path, the
		// closest midpoint resolves the arc the pointer visually targets instead
		// of whichever SVG child happened to be painted last.
		setHoveredArcId(
			nearestMidpointDistanceSquared <= 36 * 36 ? resolvedArcId : hitArcId,
		);
	};

	return (
		<figure
			className="flow-ipm-mcf-panel"
			data-testid="flow-ipm-mcf-panel"
			data-ipm-stage={overlay.stage}
			data-ipm-seed={overlay.seed}
			data-ipm-sampled-arc={overlay.sampled_arc}
			data-ipm-forest-subset={overlay.forest_subset_serial}
		>
			<header className="flow-ipm-mcf-header">
				<div>
					<p className="flow-ipm-eyebrow">
						INTEGER PRIMAL–DUAL AUXILIARY GRAPH
					</p>
					<strong>{overlay.stage}</strong>
					<small>{phaseDescription(overlay.stage)}</small>
				</div>
				<dl>
					<div>
						<dt>μ</dt>
						<dd title={overlay.mu}>{compactInteger(overlay.mu)}</dd>
					</div>
					<div>
						<dt>centrality / μ</dt>
						<dd>{`${compactInteger(overlay.centrality_numerator)} / ${compactInteger(overlay.mu)}`}</dd>
					</div>
					<div>
						<dt>proxy / 4βγ⁄81</dt>
						<dd
							title={`${overlay.proxy_gap} / (${proxyThresholdNumerator}/81)`}
						>{`${compactInteger(overlay.proxy_gap)} / ${compactInteger(proxyThresholdNumerator.toString())}⁄81`}</dd>
					</div>
					<div>
						<dt>grid β · γ</dt>
						<dd>{`${compactInteger(overlay.beta)} · ${compactInteger(overlay.gamma)}`}</dd>
					</div>
					<div>
						<dt>tree condition</dt>
						<dd>
							{condition === undefined
								? "—"
								: `${compactInteger(condition.numerator)} / ${compactInteger(condition.denominator)}`}
						</dd>
					</div>
				</dl>
			</header>
			<div ref={graphViewportRef} className="flow-ipm-mcf-graph-wrap">
				<svg
					viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
					role="img"
					aria-labelledby={`${titleId} ${descriptionId}`}
					onPointerMove={handleArcPointerMove}
					onPointerLeave={() => setHoveredArcId(undefined)}
				>
					<title id={titleId}>
						Auxiliary graph for the integral primal-dual interior-point method
					</title>
					<desc id={descriptionId}>
						Edge width shows primal coordinate x, color shows dual slack s, and
						dash density shows resistance ceil(s/x). Magenta marks the current
						forest-subset candidate, cyan marks the tree, orange marks the
						sampled cycle, dotted edges are deleted, and violet arcs are
						contracted.
					</desc>
					<defs>
						<marker
							id={arrowId}
							markerUnits="userSpaceOnUse"
							viewBox="0 0 10 10"
							refX="9"
							refY="5"
							markerWidth="9"
							markerHeight="9"
							orient="auto-start-reverse"
						>
							<path d="M 0 0 L 10 5 L 0 10 z" className="flow-ipm-arrow" />
						</marker>
						<marker
							id={cycleArrowId}
							markerUnits="userSpaceOnUse"
							viewBox="0 0 10 10"
							refX="9"
							refY="5"
							markerWidth="8"
							markerHeight="8"
							orient="auto-start-reverse"
						>
							<path
								d="M 0 0 L 10 5 L 0 10 z"
								className="flow-ipm-cycle-arrow"
							/>
						</marker>
					</defs>
					{forestSubsetLabel !== undefined && (
						<g
							className="flow-ipm-forest-subset-badge"
							data-ipm-forest-subset={overlay.forest_subset_serial}
							transform="translate(14 14)"
						>
							<rect width="304" height="28" rx="7" />
							<text x="12" y="18">
								{forestSubsetLabel}
							</text>
						</g>
					)}
					<g className="flow-ipm-mcf-arcs">
						{overlay.arcs.map((arc) => {
							const route = arcRoutes.get(arc.auxiliary_id);
							if (route === undefined) return null;
							const path = route.d;
							const center = route.label;
							const flowWidth =
								maximumFlow === 0n
									? 1.8
									: 1.8 +
										Number((BigInt(arc.flow) * 4_200n) / maximumFlow) / 1_000;
							const slackBand = magnitudeBand(BigInt(arc.slack), maximumSlack);
							const resistanceBand = magnitudeBand(
								BigInt(arc.resistance ?? "0"),
								maximumResistance,
							);
							const labelVisible = labelledArcIdSet.has(arc.auxiliary_id);
							return (
								<g
									key={arc.auxiliary_id}
									className={`flow-ipm-arc flow-ipm-arc-${arc.kind} flow-ipm-slack-${slackBand} flow-ipm-resistance-${resistanceBand}${arc.in_minor ? " flow-ipm-arc-minor" : ""}${arc.in_tree ? " flow-ipm-arc-tree" : ""}${arc.forest_candidate ? " flow-ipm-arc-forest-candidate" : ""}${arc.deleted ? " flow-ipm-arc-deleted" : ""}${arc.contracted ? " flow-ipm-arc-contracted" : ""}${arc.active_cycle_sign === "0" ? "" : " flow-ipm-arc-cycle"}${overlay.sampled_arc === arc.auxiliary_id ? " flow-ipm-arc-sampled" : ""}${hoveredArcId === arc.auxiliary_id ? " flow-ipm-arc-hovered" : ""}`}
									tabIndex={0}
									aria-label={`${arc.auxiliary_id}: x ${arc.flow}; s ${arc.slack}`}
									data-ipm-arc={arc.auxiliary_id}
									data-ipm-kind={arc.kind}
									data-ipm-flow={arc.flow}
									data-ipm-slack={arc.slack}
									data-ipm-resistance={arc.resistance}
									data-ipm-cycle-sign={arc.active_cycle_sign}
									data-ipm-forest-candidate={arc.forest_candidate || undefined}
									data-ipm-forest-subset={
										arc.forest_candidate
											? overlay.forest_subset_serial
											: undefined
									}
									data-ipm-parallel-index={route.parallelIndex}
									data-ipm-parallel-count={route.parallelCount}
								>
									<title>{`${arc.auxiliary_id} · ${arc.kind} of ${arc.original_edge_id} · x ${arc.flow} · s ${arc.slack}${arc.resistance === undefined ? "" : ` · r ${arc.resistance}`}${arc.in_minor ? " · active minor" : ""}${arc.in_tree ? " · forest/tree" : ""}${arc.forest_candidate ? ` · candidate subset #${overlay.forest_subset_serial}` : ""}${arc.deleted ? " · sticky deleted" : ""}${arc.contracted ? " · sticky contracted" : ""}${arc.active_cycle_sign === "0" ? "" : ` · cycle ${arc.active_cycle_sign}`}`}</title>
									{arc.in_tree && (
										<path d={path} className="flow-ipm-tree-rail" />
									)}
									{arc.forest_candidate && (
										<path d={path} className="flow-ipm-forest-candidate-rail" />
									)}
									<path
										d={path}
										className="flow-ipm-primary-arc"
										style={{ strokeWidth: arc.contracted ? 5 : flowWidth }}
										markerEnd={flowScopedSvgUrl(idScope, "arrow")}
									/>
									{arc.active_cycle_sign !== "0" && (
										<path
											d={path}
											className="flow-ipm-cycle-rail"
											markerStart={
												arc.active_cycle_sign === "-1"
													? flowScopedSvgUrl(idScope, "cycle-arrow")
													: undefined
											}
											markerEnd={
												arc.active_cycle_sign === "1"
													? flowScopedSvgUrl(idScope, "cycle-arrow")
													: undefined
											}
										/>
									)}
									<path d={path} className="flow-ipm-focus-rail" />
									{arc.deleted && (
										<path
											d={`M ${center.x - 7} ${center.y - 7} L ${center.x + 7} ${center.y + 7} M ${center.x + 7} ${center.y - 7} L ${center.x - 7} ${center.y + 7}`}
											className="flow-ipm-delete-mark"
										/>
									)}
									{route.labelLeader !== undefined && (
										<line
											className={`flow-panel-edge-label-leader flow-ipm-arc-label-leader${labelVisible ? " flow-ipm-arc-label-leader-visible" : ""}`}
											x1={route.labelLeader.from.x}
											y1={route.labelLeader.from.y}
											x2={route.labelLeader.to.x}
											y2={route.labelLeader.to.y}
										/>
									)}
									<g
										className={`flow-ipm-arc-label flow-panel-edge-label${labelVisible ? " flow-ipm-arc-label-visible" : ""}`}
										transform={`translate(${center.x} ${center.y})`}
									>
										<rect x="-72" y="-18" width="144" height="36" rx="5" />
										<text
											y="-5"
											textAnchor="middle"
										>{`x ${compactInteger(arc.flow)}`}</text>
										<text
											y="10"
											textAnchor="middle"
											className="flow-ipm-arc-label-detail"
										>{`s ${compactInteger(arc.slack)}${route.parallelCount > 1 ? ` · arc ${route.parallelIndex}/${route.parallelCount}` : ""}`}</text>
									</g>
								</g>
							);
						})}
					</g>
					<g className="flow-ipm-mcf-nodes">
						{overlay.nodes.map((node) => {
							const position = positions.get(node.auxiliary_id);
							if (position === undefined) return null;
							const potentialBand = magnitudeBand(
								BigInt(node.potential),
								maximumPotential,
							);
							return (
								<g
									key={node.auxiliary_id}
									transform={`translate(${position.x} ${position.y})`}
									className={`flow-ipm-node flow-ipm-node-${node.kind} flow-ipm-potential-${potentialBand}${node.in_crossover_set ? " flow-ipm-node-crossover" : ""}`}
									data-ipm-node={node.auxiliary_id}
									data-ipm-component={node.component}
									data-ipm-crossover={node.in_crossover_set || undefined}
								>
									<title>{`${node.auxiliary_id} · π ${node.potential} · component ${node.component}${node.in_crossover_set ? " · nested crossover set" : ""}`}</title>
									{node.in_crossover_set && (
										<circle r="30" className="flow-ipm-crossover-halo" />
									)}
									{node.kind === "original" ? (
										<circle r="20" />
									) : (
										<path d="M 0 -21 L 21 0 L 0 21 L -21 0 Z" />
									)}
									<text
										y="1"
										textAnchor="middle"
										dominantBaseline="central"
										className="flow-ipm-node-id"
									>
										{node.original_node_id ?? `u(${node.original_edge_id})`}
									</text>
									<text
										y="36"
										textAnchor="middle"
										className="flow-ipm-node-detail"
									>{`π ${compactInteger(node.potential)} · C${node.component}`}</text>
								</g>
							);
						})}
					</g>
				</svg>
			</div>
			<figcaption className="flow-ipm-mcf-legend">
				<span>
					<i className="flow-ipm-legend-flow" />
					Width = primal x
				</span>
				<span>
					<i className="flow-ipm-legend-slack" />
					Blue → red = dual slack s
				</span>
				<span>
					<i className="flow-ipm-legend-resistance" />
					Dashes = ceil(s/x)
				</span>
				<span>
					<i className="flow-ipm-legend-forest-candidate" />
					Magenta = candidate subset
				</span>
				<span>
					<i className="flow-ipm-legend-tree" />
					Cyan = forest / crossover tree
				</span>
				<span>
					<i className="flow-ipm-legend-cycle" />
					Orange = sampled fundamental cycle
				</span>
				<span>
					<i className="flow-ipm-legend-delete" />× = sticky delete
				</span>
				<span>
					<i className="flow-ipm-legend-contract" />
					Violet = sticky contraction
				</span>
			</figcaption>
			<ul className="visually-hidden" aria-label="Exact auxiliary graph state">
				{overlay.arcs.map((arc) => (
					<li
						key={arc.auxiliary_id}
					>{`${arc.auxiliary_id}: ${arc.from} to ${arc.to}; primal x ${arc.flow}; dual slack s ${arc.slack}${arc.resistance === undefined ? "" : `; resistance ${arc.resistance}`}; kind ${arc.kind}${arc.in_tree ? "; in tree" : ""}${arc.forest_candidate ? `; candidate subset ${overlay.forest_subset_serial}` : ""}${arc.deleted ? "; deleted" : ""}${arc.contracted ? "; contracted" : ""}${arc.active_cycle_sign === "0" ? "" : `; cycle sign ${arc.active_cycle_sign}`}`}</li>
				))}
				{overlay.nodes.map((node) => (
					<li
						key={node.auxiliary_id}
					>{`${node.auxiliary_id}: potential ${node.potential}; component ${node.component}${node.in_crossover_set ? "; in crossover set" : ""}`}</li>
				))}
			</ul>
		</figure>
	);
}
