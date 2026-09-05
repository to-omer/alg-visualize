import type { CSSProperties } from "react";
import {
	flowScopedDomId,
	flowScopedSvgUrl,
	useFlowDomIdScope,
} from "./flow-dom-id";
import { buildFlowPanelEdgeRoutes } from "./flow-panel-edge-routing";
import type {
	FlowCurrentSceneV9,
	FlowRandomizedAlmostLinearMcfOverlayV1,
} from "./flow-scene";

type Point = { x: number; y: number };
type FlowGraph = FlowCurrentSceneV9["graph"];

const WIDTH = 980;
const HEIGHT = 390;
const PAD_X = 92;
const PAD_Y = 74;

function compact(raw: string): string {
	const value = Number(raw);
	if (!Number.isFinite(value)) return raw;
	if (value === 0) return "0";
	if (Math.abs(value) >= 10_000 || Math.abs(value) < 0.001) {
		return value.toExponential(2).replace("e+", "e");
	}
	return value.toPrecision(4).replace(/\.0+$/u, "");
}

function rational(
	value: { numerator: string; denominator: string } | undefined,
): string {
	if (value === undefined) return "—";
	return value.denominator === "1"
		? value.numerator
		: `${value.numerator}/${value.denominator}`;
}

function stageCopy(
	stage: FlowRandomizedAlmostLinearMcfOverlayV1["stage"],
): string {
	if (stage.includes("isolation")) {
		return "Sample independent zₑ values and uniquely isolate the optimum with D·cₑ+zₑ";
	}
	if (stage === "initialize-relative-interior") {
		return "Construct a strict relative-interior point from the feasible face's centroid";
	}
	if (stage.includes("forest") || stage === "sample-tree-chain") {
		return "Sample a tree chain from the seeded forest population";
	}
	if (
		stage === "inspect-oracle-vector" ||
		stage === "refresh-gradient-length" ||
		stage === "query-minimum-ratio-cycle" ||
		stage === "potential-reduction-step"
	) {
		return "Map g/ℓ for the α-power potential and advance with the exact bounded oracle";
	}
	if (
		stage === "detect-changed-coordinates" ||
		stage === "rebuild-tree-chain"
	) {
		return "Detect only coordinates with ℓₑ·|fₑ−f̃ₑ| ≥ ε, then rebuild the tree chain";
	}
	if (stage === "construct-final-point") {
		return "Place an exact feasible point within the source threshold and certify rounding radius below 1/4";
	}
	if (
		["round-nearest-integer", "check-certificate", "optimal"].includes(stage)
	) {
		return "Round each coordinate of the published rational point to the nearest integer and independently verify original-cost optimality";
	}
	return "Initialize the bounded executable replacement";
}

function positions(graph: FlowGraph): Map<string, Point> {
	const raw = graph.nodes.map((node, index) => ({
		id: node.id,
		x: Number(node.position?.x ?? index),
		y: Number(node.position?.y ?? index % 2),
	}));
	if (raw.length === 1) {
		return new Map([[raw[0]?.id ?? "", { x: WIDTH / 2, y: HEIGHT / 2 }]]);
	}
	const minX = Math.min(...raw.map((point) => point.x));
	const maxX = Math.max(...raw.map((point) => point.x));
	const minY = Math.min(...raw.map((point) => point.y));
	const maxY = Math.max(...raw.map((point) => point.y));
	return new Map(
		raw.map((point, index) => {
			const angle = (index * Math.PI * 2) / Math.max(1, raw.length);
			return [
				point.id,
				{
					x:
						maxX === minX
							? WIDTH / 2 + Math.cos(angle) * 310
							: PAD_X +
								((point.x - minX) / (maxX - minX)) * (WIDTH - 2 * PAD_X),
					y:
						maxY === minY
							? HEIGHT / 2 + Math.sin(angle) * 112
							: PAD_Y +
								((point.y - minY) / (maxY - minY)) * (HEIGHT - 2 * PAD_Y),
				},
			] as const;
		}),
	);
}

function phase(stage: FlowRandomizedAlmostLinearMcfOverlayV1["stage"]): string {
	if (stage.includes("isolation") || stage === "select-isolated-optimum")
		return "isolate";
	if (stage.includes("forest") || stage.includes("tree-chain"))
		return "tree chain";
	if (
		stage === "inspect-oracle-vector" ||
		stage.includes("gradient") ||
		stage.includes("ratio") ||
		stage.includes("potential") ||
		stage.includes("detect")
	) {
		return "IPM / Detect";
	}
	if (stage === "construct-final-point") return "final point";
	if (
		["round-nearest-integer", "check-certificate", "optimal"].includes(stage)
	) {
		return "exact recovery";
	}
	return "feasible face";
}

export function FlowRandomizedAlmostLinearMcfPanel({
	graph,
	overlay,
}: {
	graph: FlowGraph;
	overlay: FlowRandomizedAlmostLinearMcfOverlayV1;
}) {
	const idScope = useFlowDomIdScope("flow-ralmcf");
	const titleId = flowScopedDomId(idScope, "title");
	const descriptionId = flowScopedDomId(idScope, "description");
	const arrowId = flowScopedDomId(idScope, "arrow");
	const glowId = flowScopedDomId(idScope, "glow");
	const nodePositions = positions(graph);
	const stateByEdge = new Map(
		overlay.edges.map((edge) => [edge.edge_id, edge]),
	);
	const candidateNodeIds = new Set<string>();
	for (const edge of graph.edges) {
		if (stateByEdge.get(edge.id)?.candidate_sign !== "0") {
			candidateNodeIds.add(edge.from);
			candidateNodeIds.add(edge.to);
		}
	}
	const maximumCapacity = Math.max(
		1,
		...graph.edges.map((edge) => Number(edge.capacity)),
	);
	const edgeRoutes = buildFlowPanelEdgeRoutes(graph.edges, nodePositions, {
		width: WIDTH,
		height: HEIGHT,
		paddingX: 70,
		paddingY: 52,
		labelWidth: 130,
		labelHeight: 32,
	});
	return (
		<figure
			className="flow-ralmcf-panel"
			data-testid="flow-randomized-almost-linear-mcf-oracle-demonstrator-panel"
			data-ralmcf-stage={overlay.stage}
			data-ralmcf-phase={phase(overlay.stage)}
			data-ralmcf-detected={overlay.detected_coordinates}
			data-ralmcf-exact={overlay.exact_recovery ? "true" : "false"}
			data-ralmcf-final-gap={rational(overlay.final_point_gap)}
		>
			<header className="flow-ralmcf-header">
				<div>
					<p className="flow-ralmcf-kicker">
						RANDOMIZED MCF · ISOLATION + TREE CHAIN + α-IPM
					</p>
					<strong>{overlay.stage}</strong>
					<small>{stageCopy(overlay.stage)}</small>
				</div>
				<dl>
					<div className="flow-ralmcf-stat">
						<dt>cost / F*</dt>
						<dd>{`${compact(overlay.current_cost)} / ${overlay.optimum_cost}`}</dd>
					</div>
					<div className="flow-ralmcf-stat">
						<dt>α / ε</dt>
						<dd>{`${compact(overlay.alpha)} / ${compact(overlay.epsilon)}`}</dd>
					</div>
					<div className="flow-ralmcf-stat">
						<dt>ratio / η</dt>
						<dd>{`${compact(overlay.minimum_ratio ?? "0")} / ${compact(overlay.eta)}`}</dd>
					</div>
					<div className="flow-ralmcf-stat">
						<dt>isolation</dt>
						<dd>{`#${overlay.isolation_attempt} · ≤ ${overlay.failure_numerator}/${overlay.failure_denominator}`}</dd>
					</div>
					<div className="flow-ralmcf-stat">
						<dt>forest / sample</dt>
						<dd>{`${overlay.forest_pool_size} / ${overlay.sampled_forest_index ?? "—"}`}</dd>
					</div>
					<div className="flow-ralmcf-stat">
						<dt>Detect / rebuild</dt>
						<dd>{`${overlay.detected_coordinates} / ${overlay.rebuilds}`}</dd>
					</div>
					<div className="flow-ralmcf-stat">
						<dt>final gap / limit</dt>
						<dd>{`${rational(overlay.final_point_gap)} / ${rational(overlay.final_point_threshold)}`}</dd>
					</div>
				</dl>
			</header>
			<ol className="flow-ralmcf-rail" aria-label="Randomized MCF lifecycle">
				{[
					"feasible face",
					"isolate",
					"tree chain",
					"IPM / Detect",
					"final point",
					"exact recovery",
				].map((label) => (
					<li
						key={label}
						className={phase(overlay.stage) === label ? "is-active" : ""}
					>
						{label}
					</li>
				))}
			</ol>
			<div className="flow-ralmcf-graph-wrap">
				<svg
					viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
					role="img"
					aria-labelledby={`${titleId} ${descriptionId}`}
					style={
						{
							"--flow-ralmcf-glow": flowScopedSvgUrl(idScope, "glow"),
						} as CSSProperties
					}
				>
					<title id={titleId}>Randomized almost-linear minimum-cost flow</title>
					<desc id={descriptionId}>
						Stroke width shows capacity. Labels show flow/capacity and
						original/isolated cost. Dashes mark the sampled tree, violet marks
						the selected cycle, yellow glow marks a Detect refresh, and cyan or
						orange shows the gradient sign.
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
							<path d="M0 0L10 5L0 10z" fill="context-stroke" />
						</marker>
						<filter id={glowId}>
							<feGaussianBlur stdDeviation="3" result="blur" />
							<feMerge>
								<feMergeNode in="blur" />
								<feMergeNode in="SourceGraphic" />
							</feMerge>
						</filter>
					</defs>
					<g className="flow-ralmcf-edges">
						{graph.edges.map((edge) => {
							const state = stateByEdge.get(edge.id);
							const path = edgeRoutes.get(edge.id);
							if (state === undefined || path === undefined) return null;
							const visibleFlow =
								state.final_flow ??
								(state.final_point_flow === undefined
									? compact(state.current_flow)
									: rational(state.final_point_flow));
							const gradient = Number(state.gradient);
							const width = 2.2 + (Number(edge.capacity) / maximumCapacity) * 7;
							const classes = [
								state.tree_edge ? "is-tree" : "",
								state.selected_sign !== "0" ? "is-cycle" : "",
								state.detected ? "is-detected" : "",
								Number(edge.cost) > 0
									? "cost-positive"
									: Number(edge.cost) < 0
										? "cost-negative"
										: "cost-zero",
								gradient > 0
									? "is-positive"
									: gradient < 0
										? "is-negative"
										: "is-neutral",
							]
								.filter(Boolean)
								.join(" ");
							return (
								<g
									key={edge.id}
									className={classes}
									data-edge-id={edge.id}
									data-tree={state.tree_edge ? "true" : "false"}
									data-candidate-sign={state.candidate_sign}
									data-cycle-sign={state.selected_sign}
									data-detected={state.detected ? "true" : "false"}
								>
									<path
										className="flow-ralmcf-edge-halo"
										d={path.d}
										style={
											{ "--edge-width": `${width + 7}px` } as CSSProperties
										}
									/>
									<path
										className="flow-ralmcf-edge"
										d={path.d}
										markerEnd={flowScopedSvgUrl(idScope, "arrow")}
										style={{ "--edge-width": `${width}px` } as CSSProperties}
									>
										<title>{`${edge.id} · f ${visibleFlow}/${edge.capacity} · final point ${rational(state.final_point_flow)} · rounded ${state.final_flow ?? "—"} · c ${edge.cost} → ĉ ${state.isolated_cost} · z ${state.isolation_draw} · g ${state.gradient} · ℓ ${state.length}`}</title>
									</path>
									{state.candidate_sign !== "0" && (
										<path
											className="flow-ralmcf-oracle-candidate"
											d={path.d}
											markerStart={
												state.candidate_sign === "-1"
													? flowScopedSvgUrl(idScope, "arrow")
													: undefined
											}
											markerEnd={
												state.candidate_sign === "1"
													? flowScopedSvgUrl(idScope, "arrow")
													: undefined
											}
										>
											<title>{`Current ratio-oracle vector · sign ${state.candidate_sign}`}</title>
										</path>
									)}
									{path.labelLeader !== undefined && (
										<line
											className="flow-panel-edge-label-leader"
											x1={path.labelLeader.from.x}
											y1={path.labelLeader.from.y}
											x2={path.labelLeader.to.x}
											y2={path.labelLeader.to.y}
										/>
									)}
									<g
										className="flow-ralmcf-edge-label flow-panel-edge-label"
										transform={`translate(${path.label.x} ${path.label.y})`}
									>
										<text y="-5">{`${visibleFlow}/${edge.capacity}`}</text>
										<text y="11">{`c ${edge.cost} · z ${state.isolation_draw}`}</text>
									</g>
								</g>
							);
						})}
					</g>
					<g className="flow-ralmcf-nodes">
						{overlay.nodes.map((node) => {
							const point = nodePositions.get(node.node_id);
							if (point === undefined) return null;
							return (
								<g
									key={node.node_id}
									transform={`translate(${point.x} ${point.y})`}
									className={[
										node.on_selected_cycle ? "is-cycle" : "",
										candidateNodeIds.has(node.node_id)
											? "is-oracle-candidate"
											: "",
									]
										.filter(Boolean)
										.join(" ")}
									data-node-id={node.node_id}
								>
									<circle r="25" />
									<text y="4">{node.node_id}</text>
									<text
										className="flow-ralmcf-node-meta"
										y="40"
									>{`T${node.component} · d${node.depth} · b${node.required_divergence}`}</text>
								</g>
							);
						})}
					</g>
				</svg>
			</div>
			<footer className="flow-ralmcf-legend">
				<span>
					<i className="capacity" /> Capacity = stroke width
				</span>
				<span>
					<i className="positive" /> g &gt; 0
				</span>
				<span>
					<i className="negative" /> g &lt; 0
				</span>
				<span>
					<i className="tree" /> sampled tree
				</span>
				<span>
					<i className="candidate" /> current oracle vector
				</span>
				<span>
					<i className="cycle" /> selected cycle
				</span>
				<span>
					<i className="detected" /> Detect refresh
				</span>
				<small>{`seed ${overlay.seed} · D ${overlay.isolation_scale} · feasible ${overlay.feasible_flows} · isolated F ${overlay.isolated_optimum_cost} · mix ${rational(overlay.final_point_mix)}`}</small>
			</footer>
		</figure>
	);
}
