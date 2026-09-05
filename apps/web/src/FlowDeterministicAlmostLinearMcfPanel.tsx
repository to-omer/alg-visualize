import { type CSSProperties, useId } from "react";
import { buildFlowPanelEdgeRoutes } from "./flow-panel-edge-routing";
import type {
	FlowCurrentSceneV9,
	FlowFrameworkMcfOverlayV1,
	FlowRationalV1,
} from "./flow-scene";

type Point = { x: number; y: number };
type FlowGraph = FlowCurrentSceneV9["graph"];

const WIDTH = 900;
const HEIGHT = 390;
const PAD_X = 82;
const PAD_Y = 68;

function rationalNumber(value: FlowRationalV1): number {
	const numerator = BigInt(value.numerator);
	const denominator = BigInt(value.denominator);
	const scaled = (numerator * 1_000_000n) / denominator;
	return Number(scaled) / 1_000_000;
}

function rationalText(value: FlowRationalV1): string {
	const numeratorDigits = value.numerator.replace("-", "").length;
	const denominatorDigits = value.denominator.length;
	if (value.denominator === "1" && numeratorDigits <= 24)
		return value.numerator;
	const approximate = rationalNumber(value);
	const fraction =
		numeratorDigits <= 24 && denominatorDigits <= 24
			? `${value.numerator}/${value.denominator}`
			: `exact ${numeratorDigits}d/${denominatorDigits}d`;
	return `${fraction} ≈ ${compact(String(approximate))}`;
}

function compact(raw: string): string {
	const value = Number(raw);
	if (!Number.isFinite(value)) return raw;
	if (value === 0) return "0";
	if (Math.abs(value) >= 10_000 || Math.abs(value) < 0.001) {
		return value.toExponential(2).replace("e+", "e");
	}
	return value.toPrecision(4).replace(/\.0+$/u, "");
}

function phase(stage: FlowFrameworkMcfOverlayV1["stage"]): string {
	if (stage === "initialize-source-point") return "source point";
	if (stage === "periodic-reinitialize" || stage === "detect") {
		return "dynamic epoch";
	}
	if (stage === "query-minimum-ratio-cycle") return "cycle oracle";
	if (stage === "source-progress") return "source progress";
	if (stage === "round-fractional-flow") return "source final point";
	return "certified rounding";
}

function stageCopy(stage: FlowFrameworkMcfOverlayV1["stage"]): string {
	switch (stage) {
		case "initialize-source-point":
			return "Build an initial point away from capacity bounds using the paper's midpoint/auxiliary-star construction";
		case "periodic-reinitialize":
			return "Absorb the current exact flow into a new base point and rebuild the two-level dynamic epoch";
		case "detect":
			return "Detect returns changed stale coordinates, identifying which g and ℓ values to refresh before Query";
		case "query-minimum-ratio-cycle":
			return "Topology-aware Algorithm 2 selects an improving circulation and returns a positive improvement ratio";
		case "source-progress":
			return "Atomically apply the source-scaled step −η⟨g,Δ⟩ = κ²/50";
		case "round-fractional-flow":
			return "Confirm exact gap ≤ 1/2, then use Kang–Payor rounding to build an integral flow that preserves divergence and does not increase cost";
		case "check-certificate":
			return "Independently verify capacity, conservation, and residual optimality on the original graph";
		case "optimal":
			return "Publish the optimum after the source's additive-1/2 final point, Kang–Payor rounding, and all independent checks pass";
	}
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
							? WIDTH / 2 + Math.cos(angle) * 285
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

export function FlowDeterministicAlmostLinearMcfPanel({
	graph,
	overlay,
}: {
	graph: FlowGraph;
	overlay: FlowFrameworkMcfOverlayV1;
}) {
	const identity = useId().replaceAll(":", "");
	const titleId = `${identity}-title`;
	const descriptionId = `${identity}-description`;
	const encodingDescriptionId = `${identity}-encoding`;
	const markerId = `${identity}-arrow`;
	const glowId = `${identity}-glow`;
	const nodePositions = positions(graph);
	const stateByEdge = new Map(
		overlay.edges.map((edge) => [edge.edge_id, edge]),
	);
	const cycleNodes = new Set<string>();
	for (const edge of graph.edges) {
		if (stateByEdge.get(edge.id)?.selected === true) {
			cycleNodes.add(edge.from);
			cycleNodes.add(edge.to);
		}
	}
	const maximumCapacity = Math.max(
		1,
		...graph.edges.map((edge) => Number(edge.capacity)),
	);
	const edgeRoutes = buildFlowPanelEdgeRoutes(graph.edges, nodePositions, {
		width: WIDTH,
		height: HEIGHT,
		paddingX: 58,
		paddingY: 48,
		nodeRadius: 29,
		markerClearance: 11,
		labelWidth: 130,
		labelHeight: 44,
	});
	const lifecycle = [
		"source point",
		"dynamic epoch",
		"cycle oracle",
		"source progress",
		"source final point",
		"certified rounding",
	];
	const auxiliaryEdges =
		overlay.final_point_edges?.filter((edge) => edge.auxiliary) ?? [];

	return (
		<figure
			className="flow-dalmcf-panel"
			aria-labelledby={titleId}
			aria-describedby={descriptionId}
			data-testid="flow-deterministic-almost-linear-mcf-panel"
			data-dalmcf-stage={overlay.stage}
			data-dalmcf-phase={phase(overlay.stage)}
			data-dalmcf-iteration={overlay.iteration}
			data-dalmcf-reinitialized={overlay.reinitialized ? "true" : "false"}
			data-dalmcf-optimum-cost={overlay.optimum_cost ?? "pending"}
			data-dalmcf-auxiliary-edges={String(auxiliaryEdges.length)}
		>
			<header className="flow-dalmcf-header" aria-live="polite">
				<div className="flow-dalmcf-title">
					<p className="flow-dalmcf-kicker">
						FLOW FRAMEWORK MCF · BOUNDED SOURCE COORDINATOR
					</p>
					<strong id={titleId}>{overlay.stage}</strong>
					<small id={descriptionId}>{stageCopy(overlay.stage)}</small>
				</div>
				<dl>
					{[
						["iteration", overlay.iteration],
						[
							"gap",
							`${compact(overlay.gap_before)} → ${compact(overlay.gap_after)}`,
						],
						[
							"exact source gap",
							`${rationalText(overlay.exact_gap_before)} → ${rationalText(overlay.exact_gap_after)}`,
						],
						["final-point gate", `≤ ${rationalText(overlay.stopping_gap)}`],
						["scalar F* oracle", overlay.optimum_cost ?? "hidden until gate"],
						[
							"augmented proof",
							overlay.final_point_edges === undefined
								? "hidden until gate"
								: `${overlay.final_point_edges.length} edges · ${auxiliaryEdges.length} auxiliary`,
						],
						[
							"potential",
							`${compact(overlay.potential_before)} → ${compact(overlay.potential_after)}`,
						],
						["accepted ratio", rationalText(overlay.accepted_ratio)],
						["κ²/50", rationalText(overlay.target_progress)],
						["epoch", overlay.reinitialized ? "reinitialized" : "current"],
					].map(([label, value]) => (
						<div key={label} className="flow-dalmcf-stat">
							<dt>{label}</dt>
							<dd>{value}</dd>
						</div>
					))}
				</dl>
			</header>
			<ol
				className="flow-dalmcf-rail"
				aria-label="Flow Framework MCF lifecycle"
			>
				{lifecycle.map((label) => (
					<li
						key={label}
						className={phase(overlay.stage) === label ? "is-active" : ""}
					>
						{label}
					</li>
				))}
			</ol>
			<div className="flow-dalmcf-body">
				<div className="flow-dalmcf-graph-wrap">
					<svg
						viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
						role="img"
						aria-label="Bounded Flow Framework minimum-cost-flow state"
						aria-describedby={`${descriptionId} ${encodingDescriptionId}`}
						style={
							{
								"--flow-dalmcf-glow": `url(#${glowId})`,
							} as CSSProperties
						}
					>
						<title>Bounded Flow Framework minimum-cost-flow state</title>
						<desc id={encodingDescriptionId}>
							Outer width shows capacity; amber, blue, and gray outer strokes
							show positive, negative, and zero cost. Inner width shows exact
							fractional flow. The violet glow and +/− label show the selected
							circulation coefficient.
						</desc>
						<defs>
							<marker
								id={markerId}
								markerUnits="userSpaceOnUse"
								viewBox="0 0 10 10"
								refX="9"
								refY="5"
								markerWidth="7"
								markerHeight="7"
								orient="auto"
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
						<g className="flow-dalmcf-edges">
							{graph.edges.map((edge) => {
								const state = stateByEdge.get(edge.id);
								const path = edgeRoutes.get(edge.id);
								if (state === undefined || path === undefined) {
									return null;
								}
								const coefficient = rationalNumber(state.cycle_coefficient);
								const capacityWidth =
									2.2 + (Number(edge.capacity) / maximumCapacity) * 7;
								const flowFraction =
									rationalNumber(state.flow) /
									Math.max(1, Number(edge.capacity));
								const flowWidth =
									1.2 +
									Math.max(0, Math.min(1, flowFraction)) *
										Math.max(1, capacityWidth - 1.2);
								const classes = [
									state.selected ? "is-cycle" : "",
									Number(edge.cost) > 0
										? "cost-positive"
										: Number(edge.cost) < 0
											? "cost-negative"
											: "cost-zero",
									coefficient > 0
										? "is-positive"
										: coefficient < 0
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
										data-cycle-sign={
											coefficient > 0 ? "1" : coefficient < 0 ? "-1" : "0"
										}
									>
										<path
											className="flow-dalmcf-edge-halo"
											d={path.d}
											style={
												{
													"--edge-width": `${capacityWidth + 8}px`,
													filter: state.selected
														? `url(#${glowId})`
														: undefined,
												} as CSSProperties
											}
										/>
										<path
											className="flow-dalmcf-cost-rail"
											d={path.d}
											style={
												{
													"--edge-width": `${capacityWidth}px`,
												} as CSSProperties
											}
										/>
										<path
											className="flow-dalmcf-edge"
											d={path.d}
											markerEnd={`url(#${markerId})`}
											style={
												{ "--edge-width": `${flowWidth}px` } as CSSProperties
											}
										>
											<title>{`${edge.id} · flow ${rationalText(state.flow)}/${edge.capacity} · cost ${edge.cost} · cycle ${rationalText(state.cycle_coefficient)}`}</title>
										</path>
										{state.selected ? (
											<path className="flow-dalmcf-cycle-rail" d={path.d} />
										) : null}
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
											className="flow-dalmcf-edge-label flow-panel-edge-label"
											transform={`translate(${path.label.x} ${path.label.y})`}
										>
											<text y="-8">{`${rationalText(state.flow)} / ${edge.capacity}`}</text>
											<text y="6">{`cost ${edge.cost}`}</text>
											<text y="20" className="flow-dalmcf-edge-badges">
												{state.selected
													? `Δ ${coefficient > 0 ? "+" : "−"}${rationalText({ ...state.cycle_coefficient, numerator: state.cycle_coefficient.numerator.replace("-", "") })}`
													: ""}
											</text>
										</g>
									</g>
								);
							})}
						</g>
						<g className="flow-dalmcf-nodes">
							{graph.nodes.map((node) => {
								const point = nodePositions.get(node.id);
								if (point === undefined) return null;
								return (
									<g
										key={node.id}
										transform={`translate(${point.x} ${point.y})`}
										className={cycleNodes.has(node.id) ? "is-cycle" : ""}
										data-node-id={node.id}
									>
										<circle r="25" />
										<text y="4">{node.id}</text>
										<text
											className="flow-dalmcf-node-meta"
											y="40"
										>{`b ${node.supply}`}</text>
									</g>
								);
							})}
						</g>
					</svg>
				</div>
				<aside
					className="flow-dalmcf-levels"
					aria-label="Dynamic Algorithm 2 levels"
				>
					<header>
						<strong>DYNAMIC LEVELS</strong>
						<span>
							{overlay.reinitialized ? "fresh epoch" : "current epoch"}
						</span>
					</header>
					{overlay.levels.map((level) => (
						<section
							key={level.level}
							data-level={level.level}
							data-branch={level.active_branch}
							data-passes={level.passes}
						>
							<div className="flow-dalmcf-level-heading">
								<b>{`L${level.level}`}</b>
								<span>{`branch ${level.active_branch}`}</span>
							</div>
							<div
								className="flow-dalmcf-branch-dots"
								role="img"
								aria-label={`Level ${level.level}, active branch ${level.active_branch}`}
							>
								{[0, 1].map((branch) => (
									<i
										key={branch}
										className={
											Number(level.active_branch) === branch ? "is-active" : ""
										}
									/>
								))}
							</div>
							<dl>
								<div className="flow-dalmcf-level-metric">
									<dt>active branch</dt>
									<dd>{level.active_branch}</dd>
								</div>
								<div className="flow-dalmcf-level-metric">
									<dt>wrapped passes</dt>
									<dd>{level.passes}</dd>
								</div>
							</dl>
						</section>
					))}
					<footer>{`iteration ${overlay.iteration} · ${overlay.stage}`}</footer>
				</aside>
			</div>
			<footer className="flow-dalmcf-legend">
				<span>
					<i className="capacity" /> Capacity = outer width
				</span>
				<span>
					<i className="flow" /> Exact flow = inner width
				</span>
				<span>
					<i className="cost" /> Cost ±/0 = outer color
				</span>
				<span>
					<i className="positive" /> cycle +
				</span>
				<span>
					<i className="negative" /> cycle −
				</span>
				<span>
					<i className="cycle" /> accepted cycle
				</span>
				<small>
					{overlay.termination === "source-additive-half-gap"
						? "SOURCE FINAL POINT · exact gap ≤ 1/2 · Kang–Payor + independent certificate"
						: "bounded source coordinator · the stronger CKLPPS (mU)^−10 loop threshold and m^(1+o(1)) runtime are not claimed"}
				</small>
			</footer>
			<ul className="visually-hidden">
				{graph.edges.map((edge) => {
					const state = stateByEdge.get(edge.id);
					if (state === undefined) return null;
					return (
						<li
							key={edge.id}
						>{`${edge.id}: ${edge.from} to ${edge.to}, flow ${rationalText(state.flow)}/${edge.capacity}, cost ${edge.cost}, cycle coefficient ${rationalText(state.cycle_coefficient)}`}</li>
					);
				})}
			</ul>
		</figure>
	);
}
