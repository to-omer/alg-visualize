import {
	flowScopedDomId,
	flowScopedSvgUrl,
	useFlowDomIdScope,
} from "./flow-dom-id";
import { buildFlowPanelEdgeRoutes } from "./flow-panel-edge-routing";
import type {
	FlowCurrentSceneV9,
	FlowElectricalIpmMcfEdgeStateV1,
	FlowElectricalIpmMcfOverlayV1,
} from "./flow-scene";

type Point = { x: number; y: number };
type FlowGraph = FlowCurrentSceneV9["graph"];

const WIDTH = 940;
const HEIGHT = 360;
const PAD_X = 92;
const PAD_Y = 68;

function compactFloat(raw: string): string {
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

function compactInteger(raw: string): string {
	const negative = raw.startsWith("-");
	const digits = negative ? raw.slice(1) : raw;
	if (digits.length <= 9) return raw;
	return `${negative ? "−" : ""}${digits.slice(0, 3)}…${digits.slice(-2)}`;
}

function phaseDescription(
	stage: FlowElectricalIpmMcfOverlayV1["stage"],
): string {
	if (["ready", "normalize-lower-bounds"].includes(stage)) {
		return "Move lower bounds into balances and enumerate the finite integral feasible set";
	}
	if (["isolation-attempt", "select-isolated-costs"].includes(stage)) {
		return "Isolate a unique optimum with Qc+r and independently verify optimality for the original costs";
	}
	if (["contract-fixed-face", "initialize-dual-interior"].includes(stage)) {
		return "Contract fixed coordinates of the feasible face and initialize the dual interior";
	}
	if (
		[
			"assemble-electrical-laplacian",
			"solve-newton-direction",
			"damped-centering-step",
			"centered",
		].includes(stage)
	) {
		return "Solve the Newton direction on the resistance network and recenter while preserving positivity";
	}
	if (["decrease-barrier", "approximate-flow"].includes(stage)) {
		return "Reduce μ with short steps until the gap is small enough for rounding";
	}
	return "Round to nearest integers and exactly verify balance, capacity, and cost optimality";
}

function normalizedPositions(graph: FlowGraph): Map<string, Point> {
	const raw = graph.nodes.map((node, index) => ({
		id: node.id,
		x: Number(node.position?.x ?? index),
		y: Number(node.position?.y ?? index % 2),
	}));
	const finite = raw.every(
		(point) => Number.isFinite(point.x) && Number.isFinite(point.y),
	);
	if (!finite || raw.length === 0) return new Map();
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
			const radialAngle = Math.PI + (index * Math.PI * 2) / raw.length;
			const x =
				spanX === 0
					? WIDTH / 2 + Math.cos(radialAngle) * (WIDTH / 2 - PAD_X)
					: PAD_X + ((point.x - minX) / spanX) * (WIDTH - PAD_X * 2);
			const y =
				spanY === 0
					? HEIGHT / 2 + Math.sin(radialAngle) * 94
					: PAD_Y + ((point.y - minY) / spanY) * (HEIGHT - PAD_Y * 2);
			return [point.id, { x, y }] as const;
		}),
	);
}

function positiveBand(value: number, maximum: number): number {
	if (!(value > 0) || !(maximum > 0)) return 0;
	const normalized = Math.log1p(value) / Math.log1p(maximum);
	return Math.max(1, Math.min(4, Math.ceil(normalized * 4)));
}

function absoluteMaximum(values: readonly string[]): number {
	return values.reduce(
		(maximum, raw) => Math.max(maximum, Math.abs(Number(raw))),
		0,
	);
}

function edgeTitle(edge: FlowElectricalIpmMcfEdgeStateV1): string {
	return `${edge.edge_id} · x̂ ${edge.fractional_flow} · lower slack ${edge.lower_slack} · R ${edge.resistance} · G ${edge.conductance} · current ${edge.electrical_current}${edge.fixed_on_face ? " · fixed feasible-face coordinate" : ""}${edge.final_flow === undefined ? "" : ` · rounded ${edge.final_flow}`}`;
}

export function FlowElectricalIpmMcfPanel({
	graph,
	overlay,
}: {
	graph: FlowGraph;
	overlay: FlowElectricalIpmMcfOverlayV1;
}) {
	const idScope = useFlowDomIdScope("flow-eipm");
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
		laneSpacing: 29,
		labelWidth: 140,
		labelHeight: 36,
	});
	const maximumSlack = absoluteMaximum(
		overlay.edges.map((edge) => edge.lower_slack),
	);
	const maximumResistance = absoluteMaximum(
		overlay.edges.map((edge) => edge.resistance),
	);
	const maximumCurrent = absoluteMaximum(
		overlay.edges.map((edge) => edge.electrical_current),
	);
	const maximumPotential = absoluteMaximum(
		overlay.nodes.map((node) => node.potential),
	);
	return (
		<figure
			className="flow-eipm-panel"
			data-testid="flow-electrical-ipm-mcf-panel"
			data-eipm-stage={overlay.stage}
			data-eipm-seed={overlay.seed}
			data-eipm-attempt={overlay.isolation_attempt}
			data-eipm-isolated-optimum={overlay.isolated_optimum_cost}
			data-eipm-mu={overlay.mu}
		>
			<header className="flow-eipm-header">
				<div className="flow-eipm-title">
					<p>ELECTRICAL NEWTON SYSTEM · EXACT RECOVERY</p>
					<strong>{overlay.stage}</strong>
					<small>{phaseDescription(overlay.stage)}</small>
				</div>
				<dl>
					<div>
						<dt>μ / gap</dt>
						<dd>{`${compactFloat(overlay.mu)} / ${compactFloat(overlay.duality_gap_bound)}`}</dd>
					</div>
					<div>
						<dt>central / balance</dt>
						<dd>{`${compactFloat(overlay.centrality_residual)} / ${compactFloat(overlay.balance_residual)}`}</dd>
					</div>
					<div>
						<dt>energy / solve residual</dt>
						<dd>{`${compactFloat(overlay.electrical_energy)} / ${compactFloat(overlay.linear_residual)}`}</dd>
					</div>
					<div>
						<dt>step α / ε₃</dt>
						<dd>{`${compactFloat(overlay.step_size)} / ${compactFloat(overlay.epsilon_3)}`}</dd>
					</div>
					<div>
						<dt>attempt / isolated objective / gap</dt>
						<dd>{`${overlay.isolation_attempt} / ${overlay.isolated_optimum_cost} / ${overlay.isolated_gap}`}</dd>
					</div>
					<div>
						<dt>Q / perturb ≤</dt>
						<dd
							title={`${overlay.isolation_scale} / ${overlay.perturbation_bound}`}
						>{`${compactInteger(overlay.isolation_scale)} / ${compactInteger(overlay.perturbation_bound)}`}</dd>
					</div>
				</dl>
			</header>
			<div className="flow-eipm-graph-wrap">
				<svg
					viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
					role="img"
					aria-labelledby={`${titleId} ${descriptionId}`}
				>
					<title id={titleId}>
						Resistance network for the electrical-flow interior-point method
					</title>
					<desc id={descriptionId}>
						Edge width shows fractional flow, blue-to-red color shows lower
						slack, dash density shows resistance, and glow shows Newton
						electrical current. Fixed feasible-face coordinates use double gray
						strokes; the gauge anchor uses a double circle.
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
							<path d="M 0 0 L 10 5 L 0 10 z" className="flow-eipm-arrow" />
						</marker>
					</defs>
					<g className="flow-eipm-edges">
						{overlay.edges.map((state) => {
							const edge = edgeById.get(state.edge_id);
							if (edge === undefined) return null;
							const geometry = edgeRoutes.get(edge.id);
							if (geometry === undefined) return null;
							const lower = Number(state.face_lower);
							const upper = Number(state.face_upper);
							const fractional = Number(state.fractional_flow);
							const faceSpan = Math.max(upper - lower, 0);
							const normalizedFlow =
								faceSpan === 0
									? 0.45
									: Math.max(0, Math.min(1, (fractional - lower) / faceSpan));
							const width = 2 + normalizedFlow * 6;
							const slackBand = positiveBand(
								Math.abs(Number(state.lower_slack)),
								maximumSlack,
							);
							const resistanceBand = positiveBand(
								Number(state.resistance),
								maximumResistance,
							);
							const current = Math.abs(Number(state.electrical_current));
							const currentWidth =
								maximumCurrent === 0 ? 0 : 3 + (current / maximumCurrent) * 8;
							return (
								<g
									key={state.edge_id}
									className={`flow-eipm-edge flow-eipm-slack-${slackBand} flow-eipm-resistance-${resistanceBand}${state.fixed_on_face ? " flow-eipm-edge-fixed" : ""}${current > 0 ? " flow-eipm-edge-current" : ""}${state.final_flow === undefined ? "" : " flow-eipm-edge-rounded"}`}
									data-eipm-edge={state.edge_id}
									data-eipm-flow={state.fractional_flow}
									data-eipm-perturbation={state.perturbation}
									data-eipm-isolated-cost={state.isolated_cost}
									data-eipm-slack={state.lower_slack}
									data-eipm-resistance={state.resistance}
									data-eipm-current={state.electrical_current}
									data-eipm-fixed={state.fixed_on_face || undefined}
								>
									<title>{edgeTitle(state)}</title>
									{current > 0 && (
										<path
											d={geometry.d}
											className="flow-eipm-current-rail"
											style={{ strokeWidth: currentWidth }}
										/>
									)}
									{state.fixed_on_face && (
										<path d={geometry.d} className="flow-eipm-fixed-rail" />
									)}
									<path
										d={geometry.d}
										className="flow-eipm-edge-line"
										style={{ strokeWidth: state.fixed_on_face ? 3 : width }}
										markerEnd={flowScopedSvgUrl(idScope, "arrow")}
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
										className="flow-eipm-edge-label flow-panel-edge-label"
										transform={`translate(${geometry.label.x} ${geometry.label.y})`}
									>
										<rect x="-70" y="-18" width="140" height="36" rx="5" />
										<text
											y="-5"
											textAnchor="middle"
										>{`${state.fixed_on_face ? "◆ " : ""}${state.edge_id} · x̂ ${compactFloat(state.fractional_flow)}`}</text>
										<text
											y="9"
											textAnchor="middle"
											className="flow-eipm-edge-label-detail"
										>{`s ${compactFloat(state.lower_slack)} · R ${compactFloat(state.resistance)} · i ${compactFloat(state.electrical_current)}`}</text>
									</g>
								</g>
							);
						})}
					</g>
					<g className="flow-eipm-nodes">
						{overlay.nodes.map((state) => {
							const point = positions.get(state.node_id);
							if (point === undefined) return null;
							const potential = Number(state.potential);
							const band =
								maximumPotential === 0
									? 2
									: Math.max(
											0,
											Math.min(
												4,
												Math.round(
													((potential / maximumPotential + 1) / 2) * 4,
												),
											),
										);
							return (
								<g
									key={state.node_id}
									transform={`translate(${point.x} ${point.y})`}
									className={`flow-eipm-node flow-eipm-potential-${band}${state.anchored ? " flow-eipm-node-anchor" : ""}`}
									data-eipm-node={state.node_id}
									data-eipm-potential={state.potential}
									data-eipm-anchor={state.anchored || undefined}
								>
									<title>{`${state.node_id} · potential ${state.potential} · Newton direction ${state.potential_direction} · balance residual ${state.balance_residual}${state.anchored ? " · gauge anchor" : ""}`}</title>
									{state.anchored && (
										<circle r="27" className="flow-eipm-anchor-ring" />
									)}
									<circle r="21" className="flow-eipm-node-disc" />
									<text y="1" textAnchor="middle" className="flow-eipm-node-id">
										{state.node_id}
									</text>
									<text
										y="38"
										textAnchor="middle"
										className="flow-eipm-node-detail"
									>{`y ${compactFloat(state.potential)} · Δy ${compactFloat(state.potential_direction)}`}</text>
								</g>
							);
						})}
					</g>
				</svg>
			</div>
			<figcaption className="flow-eipm-legend">
				<span>
					<i className="flow-eipm-legend-flow" />
					Width = fractional x̂
				</span>
				<span>
					<i className="flow-eipm-legend-slack" />
					Blue → red = lower slack
				</span>
				<span>
					<i className="flow-eipm-legend-resistance" />
					Dash density = resistance R
				</span>
				<span>
					<i className="flow-eipm-legend-current" />
					Glow = Newton current
				</span>
				<span>
					<i className="flow-eipm-legend-fixed" />◆ / double stroke = fixed face
				</span>
				<span>
					<i className="flow-eipm-legend-anchor" />◎ = gauge anchor
				</span>
			</figcaption>
			<ul className="visually-hidden" aria-label="Exact electrical state">
				{overlay.edges.map((state) => (
					<li key={state.edge_id}>{edgeTitle(state)}</li>
				))}
				{overlay.nodes.map((state) => (
					<li
						key={state.node_id}
					>{`${state.node_id}: potential ${state.potential}; Newton direction ${state.potential_direction}; balance residual ${state.balance_residual}${state.anchored ? "; gauge anchor" : ""}`}</li>
				))}
			</ul>
		</figure>
	);
}
