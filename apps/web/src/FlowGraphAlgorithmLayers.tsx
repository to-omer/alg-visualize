import { FlowGraphOverlayOwnedLeaves } from "./FlowGraphOverlayOwnedLeaves";
import {
	BLOCKING_PRIMAL_DUAL_LEVEL_EVENTS,
	MPM_POTENTIAL_EVENTS,
	POTENTIAL_DIJKSTRA_PRICE_EVENTS,
} from "./flow-algorithm-presentation";
import { flowScopedSvgUrl, useFlowGraphIdScope } from "./flow-dom-id";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import type { FlowEntitySelection } from "./flow-entity-navigator";
import { enhancedCapacityScalingGateStatus } from "./flow-graph-rational-scales";
import { formatFlowRational } from "./flow-parametric-view";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
type FlowGraphLayerProps = Readonly<{
	state: FlowEntityGraphState;
	selection: FlowEntitySelection | undefined;
}>;

const DINIC_BLOCKING_FLOW_START_EVENTS = new Set([
	"dinic.blocking-flow",
	"unit-capacity-dinic.blocking-flow",
	"unit-network-dinic.blocking-flow",
]);

function exactDinicLevel(
	state: FlowEntityGraphState,
	nodeId: string,
): bigint | undefined {
	const label = state.visualization.nodeTraceStates.get(nodeId)?.label;
	if (label === undefined) return undefined;
	if (!/^(?:0|[1-9][0-9]*)$/u.test(label)) {
		throw new Error("Dinic level labels must be canonical unsigned integers");
	}
	return BigInt(label);
}

/**
 * Shows the residual level network that becomes the blocking-flow search
 * domain at the exact source boundary where Dinic enters the phase.
 *
 * The previous boundary has only published the vertex levels. Painting the
 * admissible directed arcs here makes the control transition graph-visible
 * without pretending that every levelled vertex is a local mutation.
 */
export function FlowGraphBlockingFlowLevelLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const idScope = useFlowGraphIdScope();
	const catalogId = state.context.traceEvent?.catalog_id;
	if (
		catalogId === undefined ||
		!DINIC_BLOCKING_FLOW_START_EVENTS.has(catalogId)
	) {
		return null;
	}
	const railWidthByEdge = new Map(
		state.originalVisuals.map((visual) => [visual.edge.id, visual.railWidth]),
	);
	const admissibleArcs = state.visibleResidualArcs.flatMap((arc) => {
		if (BigInt(arc.capacity) === 0n) return [];
		const fromLevel = exactDinicLevel(state, arc.from);
		const toLevel = exactDinicLevel(state, arc.to);
		if (
			fromLevel === undefined ||
			toLevel === undefined ||
			toLevel !== fromLevel + 1n
		) {
			return [];
		}
		const route = state.layout.routes.get(arc.edge_id);
		if (route === undefined) {
			throw new Error("Dinic level arc has no routed original edge");
		}
		const railWidth = railWidthByEdge.get(arc.edge_id);
		if (railWidth === undefined) {
			throw new Error("Dinic level arc has no original-edge visual");
		}
		return [
			{
				arc,
				fromLevel,
				toLevel,
				path: arc.direction === "forward" ? route.path : route.reversePath,
				width: railWidth + 6,
			},
		];
	});
	if (admissibleArcs.length === 0) {
		throw new Error("Dinic blocking-flow phase has no admissible level arc");
	}

	return (
		<g
			className="flow-level-network"
			data-level-network-event={catalogId}
			data-level-network-arcs={admissibleArcs.length}
		>
			{admissibleArcs.map(({ arc, fromLevel, toLevel, path, width }) => (
				<path
					key={`${arc.edge_id}:${arc.direction}`}
					className="flow-level-network-arc"
					data-level-network-edge={arc.edge_id}
					data-level-network-direction={arc.direction}
					data-level-network-range={`${fromLevel}:${toLevel}`}
					d={path}
					strokeWidth={width}
					markerEnd={flowScopedSvgUrl(idScope, "flow-arrow-level-network")}
				>
					<title>{`${arc.edge_id} · ${arc.direction} residual level ${fromLevel} → ${toLevel} · blocking-flow admissible`}</title>
				</path>
			))}
		</g>
	);
}

function _isPotentialDijkstraPriceEvent(
	catalogId: string | undefined,
): boolean {
	return (
		catalogId !== undefined && POTENTIAL_DIJKSTRA_PRICE_EVENTS.has(catalogId)
	);
}

function _isBlockingPrimalDualLevelEvent(
	catalogId: string | undefined,
): boolean {
	return (
		catalogId !== undefined && BLOCKING_PRIMAL_DUAL_LEVEL_EVENTS.has(catalogId)
	);
}

function _isCostScalingPriceEvent(catalogId: string | undefined): boolean {
	return (
		catalogId === "cost-scaling.relabel" ||
		catalogId === "cost-scaling.refine-start" ||
		catalogId === "cost-scaling.refine-complete" ||
		catalogId === "cost-scaling.complete"
	);
}

function _isMpmPotentialEvent(catalogId: string | undefined): boolean {
	return catalogId !== undefined && MPM_POTENTIAL_EVENTS.has(catalogId);
}

function ForestPoolMarker({
	state,
	variant,
	position,
	count,
}: Readonly<{
	state: FlowEntityGraphState;
	variant: "randomized" | "deterministic";
	position: Readonly<{ x: number; y: number }>;
	count: string;
}>) {
	const horizontalDirection = position.x <= 450 ? 1 : -1;
	const card = {
		x: position.x + horizontalDirection * 82,
		y: position.y,
	};
	const cardEdgeX = card.x - horizontalDirection * 57;
	const owner =
		variant === "randomized"
			? "randomized_almost_linear_overlay"
			: "deterministic_almost_linear_overlay";
	return (
		<FlowGraphOverlayOwnedLeaves
			state={state}
			bundle="original-edge-tree-chain"
			entity={{ kind: "auxiliary-node", id: `${variant}-forest-pool` }}
			owners={[{ overlay: owner, role: "enumerated-forest-pool" }]}
		>
			<g
				className={`flow-forest-pool-marker flow-forest-pool-marker-${variant}`}
				data-forest-pool-size={count}
			>
				<title>{`${count} spanning forests enumerated for the augmented circulation graph`}</title>
				<line
					className="flow-forest-pool-leader"
					x1={position.x + horizontalDirection * 21}
					y1={position.y}
					x2={cardEdgeX}
					y2={card.y}
				/>
				<g transform={`translate(${card.x} ${card.y})`}>
					<rect x="-57" y="-18" width="114" height="36" rx="8" />
					<path
						className="flow-forest-pool-tree-back"
						d="M -43 9 L -43 -7 M -43 -2 L -50 -9 M -43 1 L -35 -8"
					/>
					<path
						className="flow-forest-pool-tree-front"
						d="M -36 10 L -36 -6 M -36 -1 L -43 -8 M -36 2 L -28 -7"
					/>
					<text x="-18" y="1" dominantBaseline="central">
						{`${count} forests`}
					</text>
				</g>
			</g>
		</FlowGraphOverlayOwnedLeaves>
	);
}

type AlmostLinearCertificateVariant = "randomized" | "deterministic";

/**
 * Projects the independently checked max-flow certificate onto the original
 * graph. Only edges crossing the exact source-side cut are painted: the
 * certificate should not turn every vertex into a competing highlight.
 */
export function FlowGraphAlmostLinearCertificateLayer({
	state,
	variant,
}: Readonly<{
	state: FlowEntityGraphState;
	variant: AlmostLinearCertificateVariant;
}>) {
	const overlay =
		variant === "randomized"
			? state.renderData.overlayViews.randomizedAlmostLinear
			: state.renderData.overlayViews.deterministicAlmostLinear;
	if (
		overlay === undefined ||
		!(overlay.stage === "check-certificate" || overlay.stage === "optimal")
	) {
		return null;
	}
	const model = state.plan.context.model;
	if (model.kind !== "max-flow") {
		throw new Error(`${variant} almost-linear certificate requires max flow`);
	}
	const sourceSide = new Set(
		overlay.nodes
			.filter((node) => node.source_side)
			.map((node) => node.node_id),
	);
	if (!sourceSide.has(model.source) || sourceSide.has(model.sink)) {
		throw new Error(
			`${variant} almost-linear certificate has no separating cut`,
		);
	}
	const cutEdges = state.plan.edges.filter(
		(edge) => sourceSide.has(edge.from) && !sourceSide.has(edge.to),
	);
	if (cutEdges.length === 0 && BigInt(overlay.target_value) !== 0n) {
		throw new Error(
			`${variant} almost-linear nonzero certificate has no cut edge`,
		);
	}
	const verified = overlay.stage === "optimal";
	const overlayField =
		variant === "randomized"
			? "randomized_almost_linear_overlay"
			: "deterministic_almost_linear_overlay";
	return (
		<g
			className={`flow-almost-linear-certificate flow-almost-linear-certificate-${variant}`}
			data-almost-linear-certificate-stage={overlay.stage}
			data-almost-linear-certificate-cut-edges={cutEdges
				.map((edge) => edge.id)
				.join("|")}
		>
			{cutEdges.map((edge) => {
				const route = state.layout.routes.get(edge.id);
				if (route === undefined) {
					throw new Error(
						`${variant} almost-linear certificate geometry is missing for ${edge.id}`,
					);
				}
				return (
					<FlowGraphOverlayOwnedLeaves
						key={edge.id}
						state={state}
						bundle="original-edge-tree-chain"
						entity={{ kind: "edge", id: edge.id }}
						owners={[
							{
								overlay: overlayField,
								role: verified
									? "certificate-cut-verified"
									: "certificate-cut-check",
							},
						]}
					>
						<path
							className={`flow-almost-linear-certificate-cut flow-almost-linear-certificate-cut-${verified ? "verified" : "check"}`}
							d={route.path}
						>
							<title>{`${edge.id} · exact source-side cut · capacity ${edge.capacity} · ${verified ? "maximum flow certified" : "checking flow/cut equality"}`}</title>
						</path>
					</FlowGraphOverlayOwnedLeaves>
				);
			})}
		</g>
	);
}

/**
 * Turns Dynamic EIBFS's independently checked prefix certificate into an
 * exact graph-local mark. The source event publishes the source side as node
 * identities; only original arcs leaving that set belong to the cut. This is
 * deliberately edge-local so a large source side does not flash every node.
 */
export function FlowGraphDynamicEibfsCertificateLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const dynamicEibfs = state.renderData.overlayViews.dynamicEibfs;
	const traceEvent = state.context.traceEvent;
	if (
		dynamicEibfs?.stage !== "prefix-certified" ||
		traceEvent?.catalog_id !== "dynamic-eibfs.prefix-certified"
	) {
		return null;
	}
	const model = state.plan.context.model;
	if (model.kind !== "max-flow") {
		throw new Error("Dynamic EIBFS certificate requires max flow");
	}
	const sourceSide = new Set(
		traceEvent.entity_refs.flatMap((entity) =>
			entity.kind === "node" ? [entity.node_id] : [],
		),
	);
	if (!sourceSide.has(model.source) || sourceSide.has(model.sink)) {
		throw new Error("Dynamic EIBFS prefix certificate has no separating cut");
	}
	const cutEdges = state.plan.edges.filter(
		(edge) => sourceSide.has(edge.from) && !sourceSide.has(edge.to),
	);
	const prefixValue = BigInt(dynamicEibfs.prefix_value ?? "0");
	if (cutEdges.length === 0 && prefixValue !== 0n) {
		throw new Error("Dynamic EIBFS nonzero prefix certificate has no cut edge");
	}
	const zeroCutSourcePosition =
		cutEdges.length === 0 ? state.positions.get(model.source) : undefined;
	if (cutEdges.length === 0 && zeroCutSourcePosition === undefined) {
		throw new Error("Dynamic EIBFS zero-cut source geometry is missing");
	}
	return (
		<g
			className="flow-dynamic-eibfs-certificate"
			data-dynamic-eibfs-prefix={dynamicEibfs.update_index}
			data-dynamic-eibfs-prefix-value={dynamicEibfs.prefix_value}
			data-dynamic-eibfs-cut-edges={cutEdges.map((edge) => edge.id).join("|")}
		>
			{cutEdges.map((edge) => {
				const route = state.layout.routes.get(edge.id);
				if (route === undefined) {
					throw new Error(
						`Dynamic EIBFS certificate geometry is missing for ${edge.id}`,
					);
				}
				return (
					<FlowGraphOverlayOwnedLeaves
						key={edge.id}
						state={state}
						bundle="original-edge-discrete-underlay"
						entity={{ kind: "edge", id: edge.id }}
						owners={[
							{
								overlay: "dynamic_eibfs_overlay",
								role: "prefix-certificate.exact-cut-edge",
							},
						]}
					>
						<path className="flow-dynamic-eibfs-certificate-cut" d={route.path}>
							<title>{`${edge.id} · prefix ${dynamicEibfs.update_index}/${dynamicEibfs.update_total} exact source-side cut · capacity ${edge.capacity} · certified maximum flow ${dynamicEibfs.prefix_value}`}</title>
						</path>
					</FlowGraphOverlayOwnedLeaves>
				);
			})}
			{cutEdges.length === 0 && (
				<FlowGraphOverlayOwnedLeaves
					state={state}
					bundle="node-search"
					entity={{ kind: "node", id: model.source }}
					owners={[
						{
							overlay: "dynamic_eibfs_overlay",
							role: "prefix-certificate.zero-cut-source",
						},
					]}
				>
					<circle
						className="flow-dynamic-eibfs-zero-cut"
						cx={zeroCutSourcePosition?.x}
						cy={zeroCutSourcePosition?.y}
						r="42"
					>
						<title>{`Prefix ${dynamicEibfs.update_index}/${dynamicEibfs.update_total} has a certified zero-capacity source cut`}</title>
					</circle>
				</FlowGraphOverlayOwnedLeaves>
			)}
		</g>
	);
}

/**
 * `no-next-level` changes no flow or forest edge by design. Make that exact
 * negative search result visible at the exhausted side's current boundary,
 * rather than relying on a detached stage badge. When the requested depth is
 * already empty, the corresponding terminal is the natural anchor.
 */
export function FlowGraphEibfsNoNextLevelLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	if (state.context.traceEvent?.catalog_id !== "eibfs.no-next-level") {
		return null;
	}
	const eibfs = state.renderData.overlayViews.eibfs;
	if (eibfs === undefined) {
		throw new Error("EIBFS no-next-level event omitted its search forest");
	}
	const model = state.plan.context.model;
	if (model.kind !== "max-flow") {
		throw new Error("EIBFS no-next-level visualization requires max flow");
	}
	const side = eibfs.phase_direction === "forward" ? "source" : "sink";
	const depth = side === "source" ? eibfs.source_depth : eibfs.sink_depth;
	const boundaryNodes = eibfs.nodes.filter(
		(node) =>
			node.membership === side &&
			(side === "source"
				? node.source_label === depth
				: node.sink_label === depth),
	);
	const anchorId =
		boundaryNodes[0]?.node_id ??
		(side === "source" ? model.source : model.sink);
	const anchor = state.positions.get(anchorId);
	if (anchor === undefined) {
		throw new Error(
			`EIBFS exhausted-boundary geometry is missing for ${anchorId}`,
		);
	}
	const horizontal = anchor.x <= 450 ? 1 : -1;
	const vertical = anchor.y >= 92 ? -1 : 1;
	const label = `${side === "source" ? "S" : "T"} · d${depth} · NEXT ∅`;
	const labelWidth = Math.max(104, label.length * 6.2 + 20);
	const labelCenter = {
		x: anchor.x + horizontal * (52 + labelWidth / 2),
		y: anchor.y + vertical * 50,
	};
	const owners = [
		{ overlay: "eibfs_overlay" as const, role: "empty-next-frontier" },
		...(state.renderData.overlayViews.dynamicEibfs === undefined
			? []
			: [
					{
						overlay: "dynamic_eibfs_overlay" as const,
						role: "reused-search.empty-next-frontier",
					},
				]),
	];
	return (
		<FlowGraphOverlayOwnedLeaves
			state={state}
			bundle="node-search"
			entity={{ kind: "node", id: anchorId }}
			owners={owners}
		>
			<g
				className="flow-eibfs-no-next-level"
				data-eibfs-empty-side={side}
				data-eibfs-empty-depth={depth}
				data-eibfs-empty-boundary-nodes={boundaryNodes
					.map((node) => node.node_id)
					.join("|")}
			>
				<title>{`${side === "source" ? "Source" : "Sink"} forest depth ${depth} produced no next level; search terminates here`}</title>
				<circle cx={anchor.x} cy={anchor.y} r="49" />
				<line
					x1={anchor.x + horizontal * 34}
					y1={anchor.y + vertical * 29}
					x2={labelCenter.x - horizontal * (labelWidth / 2 + 4)}
					y2={labelCenter.y}
				/>
				<rect
					x={labelCenter.x - labelWidth / 2}
					y={labelCenter.y - 12}
					width={labelWidth}
					height="24"
					rx="6"
				/>
				<text
					x={labelCenter.x}
					y={labelCenter.y}
					dominantBaseline="central"
					textAnchor="middle"
				>
					{label}
				</text>
			</g>
		</FlowGraphOverlayOwnedLeaves>
	);
}

function DeterministicFundamentalCycleScan({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.deterministicAlmostLinear;
	if (overlay === undefined || overlay.stage !== "inspect-fundamental-cycle") {
		return null;
	}
	const selectedText = overlay.selected_off_tree_edge;
	if (
		selectedText === undefined ||
		!/^(?:0|[1-9][0-9]*)$/u.test(selectedText)
	) {
		throw new Error(
			"deterministic fundamental-cycle scan has no canonical off-tree edge",
		);
	}
	const selected = BigInt(selectedText);
	const originalCount = BigInt(state.plan.edges.length);
	const anchor = (() => {
		if (selected < originalCount) {
			const edge = state.plan.edges[Number(selected)];
			if (edge === undefined) {
				throw new Error("deterministic selected original edge is missing");
			}
			const route = state.layout.routes.get(edge.id);
			if (route === undefined) {
				throw new Error(
					`deterministic selected edge geometry is missing for ${edge.id}`,
				);
			}
			return {
				position: route.routeMidpoint,
				entity: { kind: "edge", id: edge.id } as const,
			};
		}
		if (selected === originalCount) {
			if (state.deterministicReturnGeometry === undefined) {
				throw new Error(
					"deterministic selected return-edge geometry is missing",
				);
			}
			return {
				position: state.deterministicReturnGeometry.midpoint,
				entity: {
					kind: "auxiliary-edge",
					id: "deterministic-return:t-to-s",
				} as const,
			};
		}
		const artificialNodes = overlay.nodes.filter(
			(node) => node.artificial_direction !== "0",
		);
		const ordinal = selected - originalCount - 1n;
		if (ordinal >= BigInt(artificialNodes.length)) {
			throw new Error("deterministic selected artificial edge is out of range");
		}
		const node = artificialNodes[Number(ordinal)];
		const nodePosition =
			node === undefined ? undefined : state.positions.get(node.node_id);
		const star = state.deterministicArtificialStarPosition;
		if (
			node === undefined ||
			nodePosition === undefined ||
			star === undefined
		) {
			throw new Error(
				"deterministic selected artificial-edge geometry is missing",
			);
		}
		return {
			position: {
				x: (nodePosition.x + star.x) / 2,
				y: (nodePosition.y + star.y) / 2,
			},
			entity: {
				kind: "auxiliary-edge",
				id: `deterministic-artificial:${node.node_id}`,
			} as const,
		};
	})();
	if (!/^(?:0|[1-9][0-9]*)$/u.test(overlay.fundamental_cycles)) {
		throw new Error("deterministic fundamental-cycle count is not canonical");
	}
	const evaluations = BigInt(overlay.fundamental_cycles);
	if (evaluations === 0n) {
		throw new Error("deterministic fundamental-cycle scan has no evaluation");
	}
	const logarithmicRadius =
		9 + Math.min(19, Math.log2(Number(evaluations) + 1) * 1.7);
	return (
		<FlowGraphOverlayOwnedLeaves
			state={state}
			bundle="original-edge-tree-chain"
			entity={anchor.entity}
			owners={[
				{
					overlay: "deterministic_almost_linear_overlay",
					role: "fundamental-cycle-evaluation-sweep",
				},
			]}
		>
			<circle
				className="flow-deterministic-cycle-scan"
				data-deterministic-cycle-evaluations={overlay.fundamental_cycles}
				data-deterministic-selected-work-edge={selectedText}
				cx={anchor.position.x}
				cy={anchor.position.y}
				r={logarithmicRadius}
			>
				<title>{`Candidate-cycle search on work edge w${selectedText} · ${overlay.fundamental_cycles} exact fundamental cycles evaluated · each published ring expansion doubles the explored set`}</title>
			</circle>
		</FlowGraphOverlayOwnedLeaves>
	);
}

export function FlowGraphEnhancedScalingComponentLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.enhancedCapacityScaling;
	const gateStage =
		overlay?.stage === "begin-phase"
			? "active"
			: overlay?.stage === "halve-scale"
				? "next"
				: undefined;
	return state.enhancedScalingComponentBoxes.map((component) => {
		const gateStatus =
			overlay !== undefined && gateStage !== undefined
				? enhancedCapacityScalingGateStatus(component.excess, overlay.delta)
				: undefined;
		const gatePath =
			gateStatus === "excess"
				? "M 0 -6 L 6 5 H -6 Z"
				: gateStatus === "deficit"
					? "M -6 -5 H 6 L 0 6 Z"
					: "M 0 -5 L 5 0 L 0 5 L -5 0 Z";
		return (
			<g
				key={`enhanced-component:${component.component_id}`}
				className={`flow-enhanced-component${component.members.length > 1 ? " flow-enhanced-component-contracted" : ""}${component.activeRole === undefined ? "" : ` flow-enhanced-component-${component.activeRole}`}`}
				data-enhanced-component={component.component_id}
				data-enhanced-component-size={component.members.length}
				data-enhanced-component-excess={formatFlowRational(component.excess)}
			>
				<title>{`component ${component.component_id} · ${component.members.join(", ")} · excess ${formatFlowRational(component.excess)}`}</title>
				<rect
					x={component.x}
					y={component.y}
					width={component.width}
					height={component.height}
					rx="24"
				/>
				<text x={component.x + 9} y={component.y + 17}>
					{`C ${component.component_id} · e ${formatFlowRational(component.excess)}`}
				</text>
				{gateStatus !== undefined && gateStage !== undefined && (
					<FlowGraphOverlayOwnedLeaves
						state={state}
						bundle="node-optimization"
						entity={{ kind: "node", id: component.component_id }}
						owners={[
							{
								overlay: "enhanced_capacity_scaling_overlay",
								role: `components.${gateStatus}-three-quarter-delta-${gateStage}`,
							},
						]}
					>
						<g
							className={`flow-enhanced-component-gate flow-enhanced-component-gate-${gateStatus} flow-enhanced-component-gate-${gateStage}`}
							data-enhanced-component-gate={gateStatus}
							data-enhanced-component-gate-stage={gateStage}
							transform={`translate(${component.x + component.width - 13} ${component.y + 13})`}
						>
							<title>{`${gateStage === "active" ? "Current" : "Next"} 3Δ/4 component scan: ${gateStatus === "excess" ? "eligible excess" : gateStatus === "deficit" ? "eligible deficit" : "below threshold"}`}</title>
							<path d={gatePath} />
						</g>
					</FlowGraphOverlayOwnedLeaves>
				)}
			</g>
		);
	});
}

export function FlowGraphAlgorithmUnderlays({
	state,
	selection,
}: FlowGraphLayerProps) {
	const idScope = useFlowGraphIdScope();
	const viewMode = state.viewMode;
	const positions = state.positions;
	const orlinMaxComponentBoxes = state.orlinMaxComponentBoxes;
	const planarDual = state.planarDual;
	const randomizedReturnGeometry = state.randomizedReturnGeometry;
	const randomizedArtificialStarPosition =
		state.randomizedArtificialStarPosition;
	const deterministicReturnGeometry = state.deterministicReturnGeometry;
	const deterministicArtificialStarPosition =
		state.deterministicArtificialStarPosition;
	const randomizedSampleCount = state.visualization.randomizedSampleCount;
	const overlayViews = state.renderData.overlayViews;
	const randomizedCycleIsCandidate =
		overlayViews.randomizedAlmostLinear?.stage === "inspect-fundamental-cycle";
	const randomizedCycleIsSelected =
		overlayViews.randomizedAlmostLinear?.stage === "query-minimum-ratio-cycle";
	const randomizedCycleIsVisible =
		randomizedCycleIsCandidate ||
		randomizedCycleIsSelected ||
		overlayViews.randomizedAlmostLinear?.stage === "potential-reduction-step";
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
	return (
		<>
			<DeterministicFundamentalCycleScan state={state} />
			<FlowGraphDynamicEibfsCertificateLayer state={state} />
			<FlowGraphAlmostLinearCertificateLayer
				state={state}
				variant="randomized"
			/>
			<FlowGraphAlmostLinearCertificateLayer
				state={state}
				variant="deterministic"
			/>
			<FlowGraphEnhancedScalingComponentLayer state={state} />
			{orlinMaxComponentBoxes
				.filter(
					(component) =>
						state.plan.level === "detail" ||
						component.members.length > 1 ||
						(selection?.kind === "node" &&
							component.members.includes(selection.id)),
				)
				.map((component) => (
					<g
						key={`orlin-max-component:${component.componentId}`}
						className={`flow-orlin-max-component${component.members.length > 1 ? " flow-orlin-max-component-contracted" : ""}${component.state.critical ? " flow-orlin-max-component-critical" : " flow-orlin-max-component-compactible"}${component.state.source_side ? " flow-orlin-max-component-source-side" : ""}`}
						data-orlin-max-component={component.componentId}
						data-orlin-max-component-size={component.members.length}
						data-orlin-max-critical={component.state.critical || undefined}
						data-orlin-max-phi={component.state.anti_potential}
					>
						<title>{`component ${component.componentId} · ${component.members.join(", ")} · ${component.state.critical ? "critical" : "compactible"} · Φ ${component.state.anti_potential}${component.state.source_side ? " · source side S" : " · sink side T"}`}</title>
						<rect
							x={component.x}
							y={component.y}
							width={component.width}
							height={component.height}
							rx="22"
						/>
						<text x={component.x + 9} y={component.y + 17}>
							{`C ${component.componentId} · Φ ${component.state.anti_potential} · ${component.state.critical ? "K" : "C"}`}
						</text>
					</g>
				))}

			{viewMode !== "residual" &&
				planarDual?.edges.map((edge) => {
					const from = planarDual.faces[edge.fromFace];
					const to = planarDual.faces[edge.toFace];
					if (from === undefined || to === undefined) return null;
					const active = edge.activeDirection !== undefined;
					const label = `${edge.forwardLength} → / 0 ←`;
					const labelWidth = Math.max(40, label.length * 5.2 + 8);
					return (
						<g
							key={`dual-edge:${edge.edgeId}`}
							className={`flow-planar-dual-edge${active ? " flow-planar-dual-edge-active" : ""}`}
							data-planar-dual-edge={edge.edgeId}
						>
							<title>{`Dual arc crossing ${edge.edgeId} · forward ${edge.forwardLength} / reverse ${edge.reverseLength}${active ? ` · active ${edge.activeDirection}` : ""}`}</title>
							<line
								x1={from.x}
								y1={from.y}
								x2={to.x}
								y2={to.y}
								markerStart={flowScopedSvgUrl(
									idScope,
									`flow-arrow-dual${active ? "-active" : ""}`,
								)}
								markerEnd={flowScopedSvgUrl(
									idScope,
									`flow-arrow-dual${active ? "-active" : ""}`,
								)}
							/>
							<line
								className="flow-planar-dual-label-tether"
								x1={edge.labelAnchorX}
								y1={edge.labelAnchorY}
								x2={edge.labelX}
								y2={edge.labelY - 4}
							/>
							<rect
								className="flow-planar-dual-label-bg"
								x={edge.labelX - labelWidth / 2}
								y={edge.labelY - 14}
								width={labelWidth}
								height={16}
								rx={4}
							/>
							<text
								data-planar-dual-label-for={edge.edgeId}
								x={edge.labelX}
								y={edge.labelY - 3}
								textAnchor="middle"
							>
								{label}
							</text>
						</g>
					);
				})}
			{viewMode !== "residual" &&
				overlayViews.randomizedAlmostLinear !== undefined &&
				randomizedReturnGeometry !== undefined && (
					<g
						className={`flow-randomized-return-edge${overlayViews.randomizedAlmostLinear.active_return_sign === "0" ? "" : " flow-randomized-return-edge-active"}`}
						data-testid="flow-randomized-return-edge"
						data-randomized-return-flow={
							overlayViews.randomizedAlmostLinear.return_flow
						}
						data-randomized-return-capacity={
							overlayViews.randomizedAlmostLinear.return_capacity
						}
						data-randomized-return-gradient={
							overlayViews.randomizedAlmostLinear.return_gradient
						}
						data-randomized-return-length={
							overlayViews.randomizedAlmostLinear.return_length
						}
						data-randomized-return-tree-memberships={
							overlayViews.randomizedAlmostLinear.return_tree_memberships
						}
						data-randomized-return-active-tree={
							overlayViews.randomizedAlmostLinear.active_return_tree_edge ||
							undefined
						}
						data-randomized-return-sign={
							overlayViews.randomizedAlmostLinear.active_return_sign
						}
						data-randomized-return-final={
							overlayViews.randomizedAlmostLinear.final_return_flow
						}
						data-randomized-return-isolation-draw={
							overlayViews.randomizedAlmostLinear.return_isolation_draw
						}
						data-randomized-return-final-point={
							overlayViews.randomizedAlmostLinear.final_point_return_flow
						}
					>
						<title>{`return t→s · interior ${overlayViews.randomizedAlmostLinear.return_flow}/${overlayViews.randomizedAlmostLinear.return_capacity} · cost −1 · gradient ${overlayViews.randomizedAlmostLinear.return_gradient} · length ${overlayViews.randomizedAlmostLinear.return_length} · sampled trees ${overlayViews.randomizedAlmostLinear.return_tree_memberships}/${overlayViews.randomizedAlmostLinear.sample_count}${overlayViews.randomizedAlmostLinear.active_return_tree_edge ? " · active tree" : ""}${overlayViews.randomizedAlmostLinear.active_return_sign === "0" ? "" : ` · active cycle sign ${overlayViews.randomizedAlmostLinear.active_return_sign}`}${overlayViews.randomizedAlmostLinear.return_isolation_draw === "0" ? "" : ` · isolation z ${overlayViews.randomizedAlmostLinear.return_isolation_draw}`}${overlayViews.randomizedAlmostLinear.final_point_return_flow === undefined ? "" : ` · final point ${overlayViews.randomizedAlmostLinear.final_point_return_flow}`}${overlayViews.randomizedAlmostLinear.final_return_flow === undefined ? "" : ` · rounded ${overlayViews.randomizedAlmostLinear.final_return_flow}`}`}</title>
						<path
							d={randomizedReturnGeometry.path}
							className="flow-randomized-return-rail"
							data-overlay-contribution="randomized_almost_linear_overlay"
							data-overlay-feature-bundle="original-edge-tree-chain"
							data-overlay-entity-kind="auxiliary-edge"
							data-overlay-entity-id="randomized-return:t-to-s"
							data-overlay-role="return-edge-reduction"
							markerEnd={flowScopedSvgUrl(
								idScope,
								"flow-arrow-randomized-almost-linear-return",
							)}
						/>
						<path
							d={randomizedReturnGeometry.path}
							className="flow-randomized-return-flow"
							data-overlay-contribution="randomized_almost_linear_overlay"
							data-overlay-feature-bundle="original-edge-tree-chain"
							data-overlay-entity-kind="auxiliary-edge"
							data-overlay-entity-id="randomized-return:t-to-s"
							data-overlay-role="return-edge-flow"
						/>
						{BigInt(
							overlayViews.randomizedAlmostLinear.return_tree_memberships,
						) > 0n && (
							<path
								d={randomizedReturnGeometry.path}
								className="flow-randomized-sampled-membership"
							/>
						)}
						{overlayViews.randomizedAlmostLinear.active_return_tree_edge && (
							<path
								d={randomizedReturnGeometry.path}
								className="flow-randomized-active-tree"
							/>
						)}
						{randomizedCycleIsVisible &&
							overlayViews.randomizedAlmostLinear.active_return_sign !==
								"0" && (
								<path
									d={
										overlayViews.randomizedAlmostLinear.active_return_sign ===
										"-1"
											? randomizedReturnGeometry.reversePath
											: randomizedReturnGeometry.path
									}
									className={`flow-randomized-return-cycle flow-randomized-active-cycle${randomizedCycleIsCandidate ? " flow-randomized-candidate-cycle" : ""}${randomizedCycleIsSelected ? " flow-randomized-selected-cycle" : ""}`}
									markerEnd={flowScopedSvgUrl(
										idScope,
										randomizedCycleIsCandidate
											? "flow-arrow-randomized-almost-linear-candidate-cycle"
											: "flow-arrow-randomized-almost-linear-cycle",
									)}
								/>
							)}
						<g
							className="flow-randomized-return-label"
							transform={`translate(${randomizedReturnGeometry.label.x} ${randomizedReturnGeometry.label.y})`}
						>
							<rect x="-92" y="-13" width="184" height="26" rx="5" />
							<text textAnchor="middle" dominantBaseline="central">
								{`t→s · ${overlayViews.randomizedAlmostLinear.final_return_flow ?? overlayViews.randomizedAlmostLinear.return_flow}/${overlayViews.randomizedAlmostLinear.return_capacity} · g ${overlayViews.randomizedAlmostLinear.return_gradient} · ℓ ${overlayViews.randomizedAlmostLinear.return_length}`}
							</text>
						</g>
					</g>
				)}
			{viewMode !== "residual" &&
				overlayViews.randomizedAlmostLinear !== undefined &&
				randomizedArtificialStarPosition !== undefined && (
					<>
						{overlayViews.randomizedAlmostLinear.stage ===
							"enumerate-forest-pool" && (
							<ForestPoolMarker
								state={state}
								variant="randomized"
								position={randomizedArtificialStarPosition}
								count={overlayViews.randomizedAlmostLinear.forest_pool_size}
							/>
						)}
						<g
							className="flow-randomized-artificial-layer"
							data-testid="flow-randomized-artificial-star"
							data-randomized-artificial-edges={
								overlayViews.randomizedAlmostLinear.artificial_edges
							}
							data-randomized-artificial-flow={
								overlayViews.randomizedAlmostLinear.artificial_flow
							}
							data-randomized-artificial-final={
								overlayViews.randomizedAlmostLinear.final_artificial_flow
							}
						>
							{overlayViews.randomizedAlmostLinear.nodes.map((node) => {
								if (node.artificial_direction === "0") return null;
								const position = positions.get(node.node_id);
								if (position === undefined) return null;
								const direction = Number(node.artificial_direction);
								const activeSign = Number(node.active_artificial_sign);
								const storedFrom =
									direction > 0 ? randomizedArtificialStarPosition : position;
								const storedTo =
									direction > 0 ? position : randomizedArtificialStarPosition;
								const activeFrom = activeSign < 0 ? storedTo : storedFrom;
								const activeTo = activeSign < 0 ? storedFrom : storedTo;
								return (
									<g
										key={`randomized-artificial:${node.node_id}`}
										className={`flow-randomized-artificial-edge${activeSign === 0 ? "" : " flow-randomized-artificial-edge-active"}`}
										data-randomized-artificial-node={node.node_id}
										data-randomized-artificial-direction={
											node.artificial_direction
										}
										data-randomized-artificial-active-sign={
											node.active_artificial_sign
										}
										data-randomized-artificial-tree-memberships={
											node.artificial_tree_memberships
										}
										data-randomized-artificial-active-tree={
											node.active_artificial_tree_edge || undefined
										}
									>
										<title>{`artificial ${direction > 0 ? "v*→" : "→v*"}${node.node_id} · ${node.artificial_flow}/${node.artificial_capacity} · sampled trees ${node.artificial_tree_memberships}/${randomizedSampleCount}${node.active_artificial_tree_edge ? " · active tree" : ""}${activeSign === 0 ? "" : ` · active sign ${activeSign}`}`}</title>
										<line
											x1={storedFrom.x}
											y1={storedFrom.y}
											x2={storedTo.x}
											y2={storedTo.y}
											markerEnd={flowScopedSvgUrl(
												idScope,
												"flow-arrow-randomized-almost-linear-artificial",
											)}
										/>
										{BigInt(node.artificial_tree_memberships) > 0n && (
											<line
												x1={storedFrom.x}
												y1={storedFrom.y}
												x2={storedTo.x}
												y2={storedTo.y}
												className="flow-randomized-sampled-membership"
											/>
										)}
										{node.active_artificial_tree_edge && (
											<line
												x1={storedFrom.x}
												y1={storedFrom.y}
												x2={storedTo.x}
												y2={storedTo.y}
												className="flow-randomized-active-tree"
											/>
										)}
										{randomizedCycleIsVisible && activeSign !== 0 && (
											<line
												x1={activeFrom.x}
												y1={activeFrom.y}
												x2={activeTo.x}
												y2={activeTo.y}
												className={`flow-randomized-artificial-cycle flow-randomized-active-cycle${randomizedCycleIsCandidate ? " flow-randomized-candidate-cycle" : ""}${randomizedCycleIsSelected ? " flow-randomized-selected-cycle" : ""}`}
												markerEnd={flowScopedSvgUrl(
													idScope,
													randomizedCycleIsCandidate
														? "flow-arrow-randomized-almost-linear-candidate-cycle"
														: "flow-arrow-randomized-almost-linear-cycle",
												)}
											/>
										)}
									</g>
								);
							})}
							<g
								className="flow-randomized-artificial-star"
								transform={`translate(${randomizedArtificialStarPosition.x} ${randomizedArtificialStarPosition.y})`}
							>
								<title>{`artificial star v* · aggregate strict-interior flow ${overlayViews.randomizedAlmostLinear.artificial_flow}${overlayViews.randomizedAlmostLinear.final_artificial_flow === undefined ? "" : ` · rounded ${overlayViews.randomizedAlmostLinear.final_artificial_flow}`}`}</title>
								<path d="M0,-18 L4,-6 L17,-6 L7,2 L11,15 L0,8 L-11,15 L-7,2 L-17,-6 L-4,-6 Z" />
								<text y="29" textAnchor="middle">
									v*
								</text>
							</g>
						</g>
					</>
				)}
			{viewMode !== "residual" &&
				overlayViews.deterministicAlmostLinear !== undefined &&
				deterministicReturnGeometry !== undefined && (
					<g
						className={`flow-deterministic-return-edge${overlayViews.deterministicAlmostLinear.active_return_sign === "0" ? "" : " flow-deterministic-return-edge-active"}`}
						data-testid="flow-deterministic-return-edge"
						data-deterministic-return-flow={
							overlayViews.deterministicAlmostLinear.return_flow
						}
						data-deterministic-return-capacity={
							overlayViews.deterministicAlmostLinear.return_capacity
						}
						data-deterministic-return-gradient={
							overlayViews.deterministicAlmostLinear.return_gradient
						}
						data-deterministic-return-length={
							overlayViews.deterministicAlmostLinear.return_length
						}
						data-deterministic-return-tree-level-mask={
							overlayViews.deterministicAlmostLinear.return_tree_level_mask
						}
						data-deterministic-return-active-tree={
							overlayViews.deterministicAlmostLinear.active_return_tree_edge ||
							undefined
						}
						data-deterministic-return-sign={
							overlayViews.deterministicAlmostLinear.active_return_sign
						}
						data-deterministic-return-final-point={
							overlayViews.deterministicAlmostLinear.final_point_return_flow ===
							undefined
								? undefined
								: formatFlowRational(
										overlayViews.deterministicAlmostLinear
											.final_point_return_flow,
									)
						}
						data-deterministic-return-rounding={
							overlayViews.deterministicAlmostLinear.rounding_return_flow ===
							undefined
								? undefined
								: formatFlowRational(
										overlayViews.deterministicAlmostLinear.rounding_return_flow,
									)
						}
						data-deterministic-return-rounding-forest={
							overlayViews.deterministicAlmostLinear
								.rounding_return_forest_edge || undefined
						}
						data-deterministic-return-rounding-sign={
							overlayViews.deterministicAlmostLinear.rounding_return_sign
						}
						data-deterministic-return-final={
							overlayViews.deterministicAlmostLinear.final_return_flow
						}
					>
						<title>{`return t→s · interior ${overlayViews.deterministicAlmostLinear.return_flow}/${overlayViews.deterministicAlmostLinear.return_capacity} · cost −1 · gradient ${overlayViews.deterministicAlmostLinear.return_gradient} · length ${overlayViews.deterministicAlmostLinear.return_length} · tree levels ${overlayViews.deterministicAlmostLinear.return_tree_level_mask}${overlayViews.deterministicAlmostLinear.active_return_tree_edge ? " · active tree" : ""}${overlayViews.deterministicAlmostLinear.active_return_sign === "0" ? "" : ` · active IPM cycle sign ${overlayViews.deterministicAlmostLinear.active_return_sign}`}${overlayViews.deterministicAlmostLinear.final_point_return_flow === undefined ? "" : ` · final point ${formatFlowRational(overlayViews.deterministicAlmostLinear.final_point_return_flow)}`}${overlayViews.deterministicAlmostLinear.rounding_return_flow === undefined ? "" : ` · rounding ${formatFlowRational(overlayViews.deterministicAlmostLinear.rounding_return_flow)}`}${overlayViews.deterministicAlmostLinear.rounding_return_forest_edge ? " · fractional forest" : ""}${overlayViews.deterministicAlmostLinear.rounding_return_sign === "0" ? "" : ` · rounding cycle sign ${overlayViews.deterministicAlmostLinear.rounding_return_sign}`}${overlayViews.deterministicAlmostLinear.final_return_flow === undefined ? "" : ` · rounded ${overlayViews.deterministicAlmostLinear.final_return_flow}`}`}</title>
						<path
							d={deterministicReturnGeometry.path}
							className="flow-deterministic-return-rail"
							data-overlay-contribution="deterministic_almost_linear_overlay"
							data-overlay-feature-bundle="original-edge-tree-chain"
							data-overlay-entity-kind="auxiliary-edge"
							data-overlay-entity-id="deterministic-return:t-to-s"
							data-overlay-role="return-edge-reduction"
							markerEnd={flowScopedSvgUrl(
								idScope,
								"flow-arrow-deterministic-almost-linear-return",
							)}
						/>
						<path
							d={deterministicReturnGeometry.path}
							className="flow-deterministic-return-flow"
							data-overlay-contribution="deterministic_almost_linear_overlay"
							data-overlay-feature-bundle="original-edge-tree-chain"
							data-overlay-entity-kind="auxiliary-edge"
							data-overlay-entity-id="deterministic-return:t-to-s"
							data-overlay-role="return-edge-flow"
						/>
						{overlayViews.deterministicAlmostLinear.return_tree_level_mask !==
							"0" && (
							<path
								d={deterministicReturnGeometry.path}
								className="flow-deterministic-tree-chain"
							/>
						)}
						{overlayViews.deterministicAlmostLinear.active_return_tree_edge && (
							<path
								d={deterministicReturnGeometry.path}
								className="flow-deterministic-active-tree"
							/>
						)}
						{deterministicCycleIsVisible &&
							overlayViews.deterministicAlmostLinear.active_return_sign !==
								"0" && (
								<path
									d={
										overlayViews.deterministicAlmostLinear
											.active_return_sign === "-1"
											? deterministicReturnGeometry.reversePath
											: deterministicReturnGeometry.path
									}
									className={`flow-deterministic-return-cycle flow-deterministic-active-cycle${deterministicCycleIsCandidate ? " flow-deterministic-candidate-cycle" : ""}${deterministicCycleIsSelected ? " flow-deterministic-selected-cycle" : ""}`}
									markerEnd={flowScopedSvgUrl(
										idScope,
										deterministicCycleIsCandidate
											? "flow-arrow-deterministic-almost-linear-candidate-cycle"
											: "flow-arrow-deterministic-almost-linear-cycle",
									)}
								/>
							)}
						{overlayViews.deterministicAlmostLinear
							.rounding_return_forest_edge && (
							<path
								d={deterministicReturnGeometry.path}
								className="flow-deterministic-rounding-forest"
							/>
						)}
						{overlayViews.deterministicAlmostLinear.rounding_return_sign !==
							"0" && (
							<path
								d={
									overlayViews.deterministicAlmostLinear
										.rounding_return_sign === "-1"
										? deterministicReturnGeometry.reversePath
										: deterministicReturnGeometry.path
								}
								className="flow-deterministic-rounding-cycle"
								markerEnd={flowScopedSvgUrl(
									idScope,
									"flow-arrow-deterministic-almost-linear-cycle",
								)}
							/>
						)}
						<g
							className="flow-deterministic-return-label"
							transform={`translate(${deterministicReturnGeometry.label.x} ${deterministicReturnGeometry.label.y})`}
						>
							<rect x="-96" y="-13" width="192" height="26" rx="5" />
							<text textAnchor="middle" dominantBaseline="central">
								{`t→s · ${overlayViews.deterministicAlmostLinear.final_return_flow ?? (overlayViews.deterministicAlmostLinear.rounding_return_flow === undefined ? overlayViews.deterministicAlmostLinear.return_flow : formatFlowRational(overlayViews.deterministicAlmostLinear.rounding_return_flow))}/${overlayViews.deterministicAlmostLinear.return_capacity} · L ${overlayViews.deterministicAlmostLinear.return_tree_level_mask}`}
							</text>
						</g>
					</g>
				)}
			{viewMode !== "residual" &&
				overlayViews.deterministicAlmostLinear !== undefined &&
				deterministicArtificialStarPosition !== undefined && (
					<>
						{overlayViews.deterministicAlmostLinear.stage ===
							"enumerate-forest-pool" && (
							<ForestPoolMarker
								state={state}
								variant="deterministic"
								position={deterministicArtificialStarPosition}
								count={overlayViews.deterministicAlmostLinear.forest_pool_size}
							/>
						)}
						<g
							className="flow-deterministic-artificial-layer"
							data-testid="flow-deterministic-artificial-star"
							data-deterministic-artificial-edges={
								overlayViews.deterministicAlmostLinear.artificial_edges
							}
							data-deterministic-artificial-flow={
								overlayViews.deterministicAlmostLinear.artificial_flow
							}
							data-deterministic-artificial-final={
								overlayViews.deterministicAlmostLinear.final_artificial_flow
							}
						>
							{overlayViews.deterministicAlmostLinear.nodes.map((node) => {
								if (node.artificial_direction === "0") return null;
								const position = positions.get(node.node_id);
								if (position === undefined) return null;
								const direction = Number(node.artificial_direction);
								const activeSign = Number(node.active_artificial_sign);
								const storedFrom =
									direction > 0
										? deterministicArtificialStarPosition
										: position;
								const storedTo =
									direction > 0
										? position
										: deterministicArtificialStarPosition;
								const activeFrom = activeSign < 0 ? storedTo : storedFrom;
								const activeTo = activeSign < 0 ? storedFrom : storedTo;
								return (
									<g
										key={`deterministic-artificial:${node.node_id}`}
										className={`flow-deterministic-artificial-edge${activeSign === 0 ? "" : " flow-deterministic-artificial-edge-active"}`}
										data-deterministic-artificial-node={node.node_id}
										data-deterministic-artificial-direction={
											node.artificial_direction
										}
										data-deterministic-artificial-active-sign={
											node.active_artificial_sign
										}
										data-deterministic-artificial-tree-level-mask={
											node.artificial_tree_level_mask
										}
										data-deterministic-artificial-active-tree={
											node.active_artificial_tree_edge || undefined
										}
									>
										<title>{`artificial ${direction > 0 ? "v*→" : "→v*"}${node.node_id} · ${node.artificial_flow}/${node.artificial_capacity} · tree levels ${node.artificial_tree_level_mask}${node.active_artificial_tree_edge ? " · active tree" : ""}${activeSign === 0 ? "" : ` · active sign ${activeSign}`}`}</title>
										<line
											x1={storedFrom.x}
											y1={storedFrom.y}
											x2={storedTo.x}
											y2={storedTo.y}
											markerEnd={flowScopedSvgUrl(
												idScope,
												"flow-arrow-deterministic-almost-linear-artificial",
											)}
										/>
										{node.artificial_tree_level_mask !== "0" && (
											<line
												x1={storedFrom.x}
												y1={storedFrom.y}
												x2={storedTo.x}
												y2={storedTo.y}
												className="flow-deterministic-tree-chain"
											/>
										)}
										{node.active_artificial_tree_edge && (
											<line
												x1={storedFrom.x}
												y1={storedFrom.y}
												x2={storedTo.x}
												y2={storedTo.y}
												className="flow-deterministic-active-tree"
											/>
										)}
										{deterministicCycleIsVisible && activeSign !== 0 && (
											<line
												x1={activeFrom.x}
												y1={activeFrom.y}
												x2={activeTo.x}
												y2={activeTo.y}
												className={`flow-deterministic-artificial-cycle flow-deterministic-active-cycle${deterministicCycleIsCandidate ? " flow-deterministic-candidate-cycle" : ""}${deterministicCycleIsSelected ? " flow-deterministic-selected-cycle" : ""}`}
												markerEnd={flowScopedSvgUrl(
													idScope,
													deterministicCycleIsCandidate
														? "flow-arrow-deterministic-almost-linear-candidate-cycle"
														: "flow-arrow-deterministic-almost-linear-cycle",
												)}
											/>
										)}
									</g>
								);
							})}
							<g
								className="flow-deterministic-artificial-star"
								transform={`translate(${deterministicArtificialStarPosition.x} ${deterministicArtificialStarPosition.y})`}
							>
								<title>{`artificial star v* · aggregate strict-interior flow ${overlayViews.deterministicAlmostLinear.artificial_flow}${overlayViews.deterministicAlmostLinear.final_artificial_flow === undefined ? "" : ` · rounded ${overlayViews.deterministicAlmostLinear.final_artificial_flow}`}`}</title>
								<path d="M0,-19 L5,-6 L18,-6 L7,2 L11,16 L0,8 L-11,16 L-7,2 L-18,-6 L-5,-6 Z" />
								<text y="30" textAnchor="middle">
									v*
								</text>
							</g>
						</g>
					</>
				)}
		</>
	);
}

export function FlowGraphAlgorithmMidLayers({ state }: FlowGraphLayerProps) {
	const idScope = useFlowGraphIdScope();
	const viewMode = state.viewMode;
	const positions = state.positions;
	const convexSimplexRootPosition = state.convexSimplexRootPosition;
	const overlayViews = state.renderData.overlayViews;
	const convexSimplexNodeById = state.renderData.convexSimplexNodeById;
	return (
		<>
			<FlowGraphEibfsNoNextLevelLayer state={state} />
			{viewMode !== "residual" && convexSimplexRootPosition !== undefined && (
				<g
					className="flow-convex-simplex-artificial-layer"
					data-convex-simplex-artificial-root="artificial-root"
				>
					{overlayViews.convexNetworkSimplex?.artificial_edges.map((edge) => {
						const source =
							edge.source === "artificial-root"
								? convexSimplexRootPosition
								: positions.get(edge.source);
						const target =
							edge.target === "artificial-root"
								? convexSimplexRootPosition
								: positions.get(edge.target);
						if (source === undefined || target === undefined) return null;
						const cycleRef = overlayViews.convexNetworkSimplex?.cycle.find(
							(reference) => reference.entity_id === edge.entity_id,
						);
						const pathSource =
							cycleRef?.direction === "reverse" ? target : source;
						const pathTarget =
							cycleRef?.direction === "reverse" ? source : target;
						return (
							<g
								key={edge.entity_id}
								className={`flow-convex-simplex-artificial flow-convex-simplex-artificial-${edge.basis}${edge.in_cycle ? " flow-convex-simplex-artificial-cycle" : ""}${edge.entering ? " flow-convex-simplex-artificial-entering" : ""}${edge.leaving ? " flow-convex-simplex-artificial-leaving" : ""}`}
								data-convex-simplex-artificial-edge={edge.entity_id}
								data-convex-simplex-basis={edge.basis}
								data-convex-simplex-cycle-direction={cycleRef?.direction}
								data-convex-simplex-entering={edge.entering || undefined}
								data-convex-simplex-leaving={edge.leaving || undefined}
								data-convex-simplex-role={
									edge.entering
										? "entering"
										: edge.leaving
											? "leaving"
											: undefined
								}
							>
								<title>{`${edge.entity_id} · artificial Big-M ${overlayViews.convexNetworkSimplex?.artificial_cost} · flow ${edge.flow} · ${edge.basis}${cycleRef === undefined ? "" : ` · cycle ${cycleRef.direction}`}${edge.entering ? " · entering" : ""}${edge.leaving ? " · leaving" : ""}`}</title>
								<line
									x1={pathSource.x}
									y1={pathSource.y}
									x2={pathTarget.x}
									y2={pathTarget.y}
									markerEnd={
										edge.in_cycle
											? flowScopedSvgUrl(idScope, "flow-arrow-residual-active")
											: flowScopedSvgUrl(idScope, "flow-arrow-forest")
									}
								/>
							</g>
						);
					})}
					<g
						className="flow-convex-simplex-root"
						transform={`translate(${convexSimplexRootPosition.x} ${convexSimplexRootPosition.y})`}
						data-convex-simplex-potential={
							convexSimplexNodeById.get("artificial-root")?.potential
						}
					>
						<title>{`artificial root · π ${convexSimplexNodeById.get("artificial-root")?.potential ?? "—"} · Big-M ${overlayViews.convexNetworkSimplex?.artificial_cost}`}</title>
						<circle r="24" />
						<text textAnchor="middle" dominantBaseline="central">
							R*
						</text>
					</g>
				</g>
			)}
		</>
	);
}
