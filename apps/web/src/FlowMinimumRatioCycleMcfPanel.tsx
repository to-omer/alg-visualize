import type { CSSProperties } from "react";
import {
	flowScopedDomId,
	flowScopedSvgUrl,
	useFlowDomIdScope,
} from "./flow-dom-id";
import { buildFlowPanelEdgeRoutes } from "./flow-panel-edge-routing";
import type {
	FlowCurrentSceneV9,
	FlowMinimumRatioCycleMcfEdgeStateV1,
	FlowMinimumRatioCycleMcfOverlayV1,
} from "./flow-scene";

type Point = { x: number; y: number };
type FlowGraph = FlowCurrentSceneV9["graph"];

const WIDTH = 940;
const HEIGHT = 370;
const PAD_X = 92;
const PAD_Y = 70;

function compact(raw: string): string {
	const value = Number(raw);
	if (!Number.isFinite(value)) return raw;
	if (value === 0) return "0";
	const magnitude = Math.abs(value);
	if (magnitude >= 1_000 || magnitude < 0.001) {
		return value.toExponential(2).replace("e+", "e");
	}
	return value
		.toPrecision(4)
		.replace(/(\.\d*?[1-9])0+$/u, "$1")
		.replace(/\.0+$/u, "");
}

function phaseDescription(
	stage: FlowMinimumRatioCycleMcfOverlayV1["stage"],
): string {
	if (
		[
			"ready",
			"enumerate-feasible-set",
			"contract-fixed-face",
			"initialize-strict-interior",
		].includes(stage)
	) {
		return "Construct a strict relative-interior point from the exact feasible face";
	}
	if (["evaluate-potential", "map-gradient-length"].includes(stage)) {
		return "Evaluate Φ, gradient g, and length ℓ of the α-power barrier from the source formulas";
	}
	if (
		[
			"build-spanning-forest",
			"evaluate-cycle",
			"update-best",
			"verify-cycle-space",
		].includes(stage)
	) {
		return "Enumerate the active cycle space exactly and minimize gᵀδ / ‖Lδ‖₁";
	}
	if (["apply-source-step", "measure-potential-decrease"].includes(stage)) {
		return "Update with ηgᵀδ = −κ²/50 and measure Φ decrease ≥ κ²/500";
	}
	return "One-step primitive checked against an independent DFS oracle; this is not a complete MCF solver";
}

function normalizedPositions(graph: FlowGraph): Map<string, Point> {
	const raw = graph.nodes.map((node, index) => ({
		id: node.id,
		x: Number(node.position?.x ?? index),
		y: Number(node.position?.y ?? index % 2),
	}));
	if (
		raw.length === 0 ||
		!raw.every((point) => Number.isFinite(point.x) && Number.isFinite(point.y))
	) {
		return new Map();
	}
	if (raw.length === 1) {
		return new Map([[raw[0]?.id ?? "", { x: WIDTH / 2, y: HEIGHT / 2 }]]);
	}
	const minX = Math.min(...raw.map((point) => point.x));
	const maxX = Math.max(...raw.map((point) => point.x));
	const minY = Math.min(...raw.map((point) => point.y));
	const maxY = Math.max(...raw.map((point) => point.y));
	const spanX = maxX - minX;
	const spanY = maxY - minY;
	return new Map(
		raw.map((point, index) => {
			const angle = Math.PI + (index * Math.PI * 2) / raw.length;
			return [
				point.id,
				{
					x:
						spanX === 0
							? WIDTH / 2 + Math.cos(angle) * (WIDTH / 2 - PAD_X)
							: PAD_X + ((point.x - minX) / spanX) * (WIDTH - PAD_X * 2),
					y:
						spanY === 0
							? HEIGHT / 2 + Math.sin(angle) * 98
							: PAD_Y + ((point.y - minY) / spanY) * (HEIGHT - PAD_Y * 2),
				},
			] as const;
		}),
	);
}

function band(value: number, maximum: number): number {
	if (!(value > 0) || !(maximum > 0)) return 0;
	return Math.max(1, Math.min(4, Math.ceil((value / maximum) * 4)));
}

function edgeTitle(edge: FlowMinimumRatioCycleMcfEdgeStateV1): string {
	return `${edge.edge_id} · f ${edge.initial_flow} → ${edge.updated_flow} · lower/upper slack ${edge.lower_slack}/${edge.upper_slack} · g ${edge.gradient} · ℓ ${edge.length} · candidate ${edge.candidate_sign} · selected ${edge.selected_sign}${edge.fixed_on_face ? " · fixed feasible-face coordinate" : ""}`;
}

export function FlowMinimumRatioCycleMcfPanel({
	graph,
	overlay,
}: {
	graph: FlowGraph;
	overlay: FlowMinimumRatioCycleMcfOverlayV1;
}) {
	const idScope = useFlowDomIdScope("flow-mrcmcf");
	const titleId = flowScopedDomId(idScope, "title");
	const descriptionId = flowScopedDomId(idScope, "description");
	const arrowId = flowScopedDomId(idScope, "arrow");
	const positions = normalizedPositions(graph);
	const edgeById = new Map(graph.edges.map((edge) => [edge.id, edge]));
	const edgeRoutes = buildFlowPanelEdgeRoutes(graph.edges, positions, {
		width: WIDTH,
		height: HEIGHT,
		paddingX: 70,
		paddingY: 48,
		labelWidth: 160,
		labelHeight: 40,
	});
	const maximumPressure = Math.max(
		0,
		...overlay.edges.map((edge) => {
			const length = Number(edge.length);
			return length > 0 ? Math.abs(Number(edge.gradient)) / length : 0;
		}),
	);
	const measured = Math.max(Number(overlay.potential_decrease), 0);
	const guaranteed = Math.max(Number(overlay.guaranteed_decrease), 0);
	const guaranteeShare = measured > 0 ? Math.min(1, guaranteed / measured) : 0;
	return (
		<figure
			className="flow-mrcmcf-panel"
			data-testid="flow-minimum-ratio-cycle-mcf-panel"
			data-mrcmcf-stage={overlay.stage}
			data-mrcmcf-ratio={overlay.best_ratio}
			data-mrcmcf-candidate-ratio={overlay.candidate_ratio}
			data-mrcmcf-eta={overlay.eta}
		>
			<header className="flow-mrcmcf-header">
				<div className="flow-mrcmcf-title">
					<p>α-POWER POTENTIAL · EXACT CYCLE ORACLE</p>
					<strong>{overlay.stage}</strong>
					<small>{phaseDescription(overlay.stage)}</small>
				</div>
				<dl>
					<div>
						<dt>cᵀf / F*</dt>
						<dd>{`${compact(overlay.current_cost)} / ${overlay.optimum_cost}`}</dd>
					</div>
					<div>
						<dt>gap / α</dt>
						<dd>{`${compact(overlay.cost_gap)} / ${compact(overlay.alpha)}`}</dd>
					</div>
					<div>
						<dt>ratio / κ</dt>
						<dd>{`${compact(overlay.best_ratio ?? "0")} / ${compact(overlay.kappa)}`}</dd>
					</div>
					<div>
						<dt>η / ‖Lηδ‖₁</dt>
						<dd>{`${compact(overlay.eta)} / ${compact(overlay.weighted_step_norm)}`}</dd>
					</div>
					<div>
						<dt>cycles / vectors</dt>
						<dd>{`${overlay.simple_cycles} / ${overlay.enumerated_vectors}`}</dd>
					</div>
					<div>
						<dt>feasible / dim</dt>
						<dd>{`${overlay.feasible_flows} / ${overlay.fundamental_cycles}`}</dd>
					</div>
				</dl>
			</header>
			<fieldset
				className="flow-mrcmcf-progress"
				aria-label="Source potential decrease"
			>
				<div>
					<span>Φ before</span>
					<strong>{compact(overlay.potential_before)}</strong>
				</div>
				<div className="flow-mrcmcf-progress-track">
					<i
						className="flow-mrcmcf-progress-measured"
						style={{ width: measured > 0 ? "100%" : "0%" }}
					/>
					<i
						className="flow-mrcmcf-progress-guarantee"
						style={{ left: `${guaranteeShare * 100}%` }}
					/>
				</div>
				<div>
					<span>decrease / guarantee</span>
					<strong>{`${compact(overlay.potential_decrease)} / ${compact(overlay.guaranteed_decrease)}`}</strong>
				</div>
			</fieldset>
			<div className="flow-mrcmcf-graph-wrap">
				<svg
					viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
					role="img"
					aria-labelledby={`${titleId} ${descriptionId}`}
				>
					<title id={titleId}>Minimum-ratio-cycle MCF source step</title>
					<desc id={descriptionId}>
						Edge width shows updated flow, blue-to-orange color shows
						upper/lower slack bias, saturation shows gradient/length pressure,
						dashes mark the spanning forest, and glow marks the selected cycle.
						Labels also give the flow change and g/ℓ numerically.
					</desc>
					<defs>
						<marker
							id={arrowId}
							markerUnits="userSpaceOnUse"
							viewBox="0 0 10 10"
							refX="9"
							refY="5"
							markerWidth="7"
							markerHeight="7"
							orient="auto-start-reverse"
						>
							<path d="M 0 0 L 10 5 L 0 10 z" className="flow-mrcmcf-arrow" />
						</marker>
					</defs>
					<g className="flow-mrcmcf-edges">
						{overlay.edges.map((state) => {
							const edge = edgeById.get(state.edge_id);
							if (edge === undefined) return null;
							const geometry = edgeRoutes.get(edge.id);
							if (geometry === undefined) return null;
							const capacity = Math.max(Number(edge.capacity), 1);
							const flow = Math.max(0, Number(state.updated_flow));
							const width = 2.5 + Math.sqrt(flow / capacity) * 7;
							const lower = Math.max(0, Number(state.lower_slack));
							const upper = Math.max(0, Number(state.upper_slack));
							const slackBias =
								upper + lower === 0 ? 0.5 : upper / (upper + lower);
							const pressure =
								Number(state.length) > 0
									? Math.abs(Number(state.gradient)) / Number(state.length)
									: 0;
							const pressureBand = band(pressure, maximumPressure);
							const selected = state.selected_sign !== "0";
							return (
								<g
									key={state.edge_id}
									className={`flow-mrcmcf-edge flow-mrcmcf-pressure-${pressureBand}${state.tree_edge ? " flow-mrcmcf-edge-tree" : ""}${state.fixed_on_face ? " flow-mrcmcf-edge-fixed" : ""}${state.candidate_sign !== "0" ? " flow-mrcmcf-edge-candidate" : ""}${selected ? " flow-mrcmcf-edge-selected" : ""}`}
									data-mrcmcf-edge={state.edge_id}
									data-mrcmcf-flow={state.updated_flow}
									data-mrcmcf-gradient={state.gradient}
									data-mrcmcf-length={state.length}
									data-mrcmcf-selected-sign={state.selected_sign}
									style={
										{
											"--mrcmcf-slack-bias": slackBias,
										} as CSSProperties
									}
								>
									<title>{edgeTitle(state)}</title>
									{selected && (
										<path
											d={geometry.d}
											className="flow-mrcmcf-selected-rail"
											style={{ strokeWidth: width + 8 }}
										/>
									)}
									{state.fixed_on_face && (
										<path d={geometry.d} className="flow-mrcmcf-fixed-rail" />
									)}
									<path
										d={geometry.d}
										className="flow-mrcmcf-edge-line"
										style={{ strokeWidth: width }}
										markerEnd={
											state.selected_sign === "-1"
												? undefined
												: flowScopedSvgUrl(idScope, "arrow")
										}
										markerStart={
											state.selected_sign === "-1"
												? flowScopedSvgUrl(idScope, "arrow")
												: undefined
										}
									/>
									{geometry.labelLeader !== undefined && (
										<line
											className="flow-panel-edge-label-leader"
											x1={geometry.labelLeader.from.x}
											y1={geometry.labelLeader.from.y}
											x2={geometry.labelLeader.to.x}
											y2={geometry.labelLeader.to.y}
										/>
									)}
									<g
										className="flow-mrcmcf-edge-label flow-panel-edge-label"
										transform={`translate(${geometry.label.x} ${geometry.label.y})`}
									>
										<rect x="-80" y="-20" width="160" height="40" rx="5" />
										<text y="-6" textAnchor="middle">
											{`${state.selected_sign === "1" ? "→ " : state.selected_sign === "-1" ? "← " : ""}${state.edge_id} · ${compact(state.initial_flow)}→${compact(state.updated_flow)}`}
										</text>
										<text
											y="10"
											textAnchor="middle"
											className="flow-mrcmcf-edge-label-detail"
										>{`g ${compact(state.gradient)} · ℓ ${compact(state.length)}`}</text>
									</g>
								</g>
							);
						})}
					</g>
					<g className="flow-mrcmcf-nodes">
						{overlay.nodes.map((state) => {
							const point = positions.get(state.node_id);
							if (point === undefined) return null;
							return (
								<g
									key={state.node_id}
									transform={`translate(${point.x} ${point.y})`}
									className={`flow-mrcmcf-node${state.on_selected ? " flow-mrcmcf-node-selected" : ""}${state.on_candidate ? " flow-mrcmcf-node-candidate" : ""}`}
									data-mrcmcf-node={state.node_id}
									data-mrcmcf-component={state.component}
									data-mrcmcf-balance={state.candidate_balance}
								>
									<title>{`${state.node_id} · component ${state.component} · depth ${state.depth} · candidate balance ${state.candidate_balance}${state.parent_node_id === undefined ? "" : ` · parent ${state.parent_node_id}`}`}</title>
									{state.on_selected && (
										<circle r="29" className="flow-mrcmcf-node-selected-ring" />
									)}
									<circle r="22" className="flow-mrcmcf-node-disc" />
									<text
										y="1"
										textAnchor="middle"
										className="flow-mrcmcf-node-id"
									>
										{state.node_id}
									</text>
									<text
										y="39"
										textAnchor="middle"
										className="flow-mrcmcf-node-detail"
									>
										{`C${state.component} · d${state.depth}${state.candidate_balance === "0" ? "" : ` · b ${state.candidate_balance}`}`}
									</text>
								</g>
							);
						})}
					</g>
				</svg>
			</div>
			<figcaption className="flow-mrcmcf-legend">
				<span>
					<i className="flow-mrcmcf-legend-flow" />
					Width = updated flow / capacity
				</span>
				<span>
					<i className="flow-mrcmcf-legend-slack" />
					Blue ↔ orange = upper/lower slack bias
				</span>
				<span>
					<i className="flow-mrcmcf-legend-pressure" />
					Saturation = |g| / ℓ
				</span>
				<span>
					<i className="flow-mrcmcf-legend-tree" />
					Dashes = active spanning forest
				</span>
				<span>
					<i className="flow-mrcmcf-legend-selected" />
					Glow + arrow = selected δ
				</span>
				<span>
					<i className="flow-mrcmcf-legend-fixed" />
					Double gray stroke = fixed face
				</span>
			</figcaption>
			<ul
				className="visually-hidden"
				aria-label="Exact minimum-ratio-cycle state"
			>
				{overlay.edges.map((state) => (
					<li key={state.edge_id}>{edgeTitle(state)}</li>
				))}
				{overlay.nodes.map((state) => (
					<li
						key={state.node_id}
					>{`${state.node_id}: component ${state.component}; depth ${state.depth}; candidate balance ${state.candidate_balance}${state.parent_node_id === undefined ? "" : `; parent ${state.parent_node_id}`}`}</li>
				))}
			</ul>
		</figure>
	);
}
