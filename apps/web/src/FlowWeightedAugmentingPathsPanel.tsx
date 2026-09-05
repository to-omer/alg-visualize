import type { CSSProperties } from "react";
import {
	flowScopedDomId,
	flowScopedSvgUrl,
	useFlowDomIdScope,
} from "./flow-dom-id";
import { buildFlowPanelEdgeRoutes } from "./flow-panel-edge-routing";
import type { FlowWeightedAugmentingPathsOverlayV1 } from "./flow-scene";

type Point = { x: number; y: number };

const WIDTH = 980;
const HEIGHT = 390;

function compact(value: string): string {
	const parsed = BigInt(value);
	if (parsed < 10_000n) return value;
	const digits = value.length;
	return `${value.slice(0, 3)}e${digits - 1}`;
}

function phaseCopy(
	stage: FlowWeightedAugmentingPathsOverlayV1["stage"],
): string {
	if (stage === "ready" || stage === "begin-capacity-phase") {
		return "Restore capacities from the highest bit and double the previous phase's flow";
	}
	if (stage === "build-hierarchy" || stage === "certify-expansion") {
		return "Form residual SCCs X₁ and the condensation DAG D, then certify φ for every cut";
	}
	if (stage === "assign-weights") {
		return "Assign w(u,v)=|τu−τv| from the hierarchy-respecting order τ";
	}
	if (stage === "relabel-sweep" || stage === "augment-path") {
		return "Relabel to weight multiples and augment an admissible path in one step";
	}
	if (stage === "finish-weighted-round") {
		return "Close the weighted round once the source is dead and classify the residual problem";
	}
	return "Independently verify each capacity prefix's residual cut and the final max-flow/min-cut certificate";
}

function orderedPositions(
	overlay: FlowWeightedAugmentingPathsOverlayV1,
): Map<string, Point> {
	const ordered = [...overlay.nodes].sort((left, right) => {
		const leftOrder = BigInt(left.order);
		const rightOrder = BigInt(right.order);
		if (leftOrder !== rightOrder) return leftOrder < rightOrder ? -1 : 1;
		return left.node_id < right.node_id
			? -1
			: left.node_id > right.node_id
				? 1
				: 0;
	});
	const components = [...new Set(ordered.map((node) => node.component))];
	const componentRow = new Map(
		components.map((component, index) => [component, index]),
	);
	return new Map(
		ordered.map((node, index) => {
			const order = Number(node.order);
			const x =
				order > 0
					? 82 + ((order - 1) / Math.max(1, ordered.length - 1)) * (WIDTH - 164)
					: 82 + (index / Math.max(1, ordered.length - 1)) * (WIDTH - 164);
			const row = componentRow.get(node.component) ?? 0;
			const y =
				components.length <= 1
					? HEIGHT / 2 + (index % 2 === 0 ? -42 : 42)
					: 82 + (row / Math.max(1, components.length - 1)) * (HEIGHT - 164);
			return [node.node_id, { x, y }] as const;
		}),
	);
}

export function FlowWeightedAugmentingPathsPanel({
	overlay,
}: {
	overlay: FlowWeightedAugmentingPathsOverlayV1;
}) {
	const idScope = useFlowDomIdScope("flow-wap");
	const titleId = flowScopedDomId(idScope, "title");
	const descriptionId = flowScopedDomId(idScope, "description");
	const arrowId = flowScopedDomId(idScope, "arrow");
	const glowId = flowScopedDomId(idScope, "glow");
	const positions = orderedPositions(overlay);
	const arcRoutes = buildFlowPanelEdgeRoutes(
		overlay.residual_arcs.map((arc, index) => ({
			id: `${arc.edge_id}:${arc.direction}:${index}`,
			from: arc.from,
			to: arc.to,
		})),
		positions,
		{
			width: WIDTH,
			height: HEIGHT,
			paddingX: 54,
			paddingY: 48,
			nodeRadius: 30,
			markerClearance: 11,
			labelWidth: 104,
			labelHeight: 22,
		},
	);
	const maximumResidual = overlay.residual_arcs.reduce((maximum, arc) => {
		const capacity = BigInt(arc.capacity);
		return capacity > maximum ? capacity : maximum;
	}, 1n);
	const phase = Number(overlay.phase);
	const phaseCount = Number(overlay.phase_count);
	const componentGroups = new Map<string, string[]>();
	for (const node of overlay.nodes) {
		const members = componentGroups.get(node.component) ?? [];
		members.push(node.node_id);
		componentGroups.set(node.component, members);
	}
	return (
		<figure
			className="flow-wap-panel"
			data-testid="flow-weighted-augmenting-paths-panel"
			data-wap-stage={overlay.stage}
			data-wap-phase={overlay.phase}
			data-wap-bit={overlay.capacity_bit}
			data-wap-active-path={overlay.active_path.length}
		>
			<header className="flow-wap-header">
				<div>
					<p className="flow-wap-kicker">
						DIRECTED EXPANDER HIERARCHY · WEIGHTED PATH LISTING
					</p>
					<strong className="flow-wap-title">{overlay.stage}</strong>
					<small className="flow-wap-subtitle">
						{phaseCopy(overlay.stage)}
					</small>
				</div>
				<dl>
					<div className="flow-wap-stat">
						<dt>prefix / bit</dt>
						<dd>{`${phase + 1}/${phaseCount} · b${overlay.capacity_bit}`}</dd>
					</div>
					<div className="flow-wap-stat">
						<dt>φ exact</dt>
						<dd>{`${overlay.phi_numerator}/${overlay.phi_denominator}`}</dd>
					</div>
					<div className="flow-wap-stat">
						<dt>height / round</dt>
						<dd>{`${compact(overlay.height)} / ${overlay.round}`}</dd>
					</div>
					<div className="flow-wap-stat">
						<dt>cuts / relabel</dt>
						<dd>{`${overlay.hierarchy_cuts} / ${overlay.relabel_jumps}`}</dd>
					</div>
					<div className="flow-wap-stat">
						<dt>paths / units</dt>
						<dd>{`${overlay.augmentations} / ${overlay.augmented_units}`}</dd>
					</div>
					<div className="flow-wap-stat">
						<dt>bottleneck</dt>
						<dd>{overlay.active_bottleneck}</dd>
					</div>
				</dl>
			</header>
			<div
				className="flow-wap-bitrail"
				role="progressbar"
				aria-label="Capacity scaling bit phase"
				aria-valuemin={1}
				aria-valuemax={phaseCount}
				aria-valuenow={phase + 1}
			>
				{Array.from(
					{ length: phaseCount },
					(_, index) => `b${phaseCount - index - 1}`,
				).map((bitLabel, index) => (
					<i
						key={bitLabel}
						className={
							index < phase ? "is-done" : index === phase ? "is-active" : ""
						}
					>
						<span>{bitLabel}</span>
					</i>
				))}
			</div>
			<div className="flow-wap-graph-shell">
				<svg
					viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
					role="img"
					aria-labelledby={`${titleId} ${descriptionId}`}
					style={
						{
							"--flow-wap-glow": flowScopedSvgUrl(idScope, "glow"),
						} as CSSProperties
					}
				>
					<title id={titleId}>Weighted augmenting paths hierarchy</title>
					<desc id={descriptionId}>
						Thicker strokes mean more residual capacity. Solid blue marks
						expanding edges within an SCC, dashed violet marks DAG edges, amber
						marks admissible edges, and white glow marks the active path. Node
						values show τ order and the weighted label.
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
							<path d="M 0 0 L 10 5 L 0 10 z" fill="context-stroke" />
						</marker>
						<filter id={glowId} x="-40%" y="-40%" width="180%" height="180%">
							<feGaussianBlur stdDeviation="3.5" result="blur" />
							<feMerge>
								<feMergeNode in="blur" />
								<feMergeNode in="SourceGraphic" />
							</feMerge>
						</filter>
					</defs>
					<g className="flow-wap-components">
						{[...componentGroups].map(([component, members]) => {
							const points = members
								.map((node) => positions.get(node))
								.filter((point): point is Point => point !== undefined);
							if (points.length === 0) return null;
							const minX = Math.min(...points.map((point) => point.x)) - 54;
							const maxX = Math.max(...points.map((point) => point.x)) + 54;
							const minY = Math.min(...points.map((point) => point.y)) - 52;
							const maxY = Math.max(...points.map((point) => point.y)) + 52;
							return (
								<g key={`component-${component}`}>
									<rect
										x={minX}
										y={minY}
										width={maxX - minX}
										height={maxY - minY}
										rx="28"
									/>
									<text
										x={minX + 14}
										y={minY + 22}
									>{`X₁ · C${component}`}</text>
								</g>
							);
						})}
					</g>
					<g className="flow-wap-arcs">
						{overlay.residual_arcs.map((arc, index) => {
							if (BigInt(arc.capacity) === 0n && !arc.active) return null;
							const geometry = arcRoutes.get(
								`${arc.edge_id}:${arc.direction}:${index}`,
							);
							if (geometry === undefined) return null;
							const capacity = BigInt(arc.capacity);
							const width =
								2.2 + Number((capacity * 500n) / maximumResidual) / 100;
							const className = [
								arc.hierarchy_kind === "dag" ? "is-dag" : "is-expanding",
								arc.admissible ? "is-admissible" : "",
								arc.active ? "is-active" : "",
								arc.direction === "reverse" ? "is-reverse" : "",
							]
								.filter(Boolean)
								.join(" ");
							return (
								<g
									key={`${arc.edge_id}-${arc.direction}`}
									className={className}
									data-testid={arc.active ? "flow-wap-active-arc" : undefined}
								>
									<title>{`${arc.edge_id} ${arc.direction} · residual ${arc.capacity} · w ${arc.weight} · ${arc.hierarchy_kind ?? "unassigned"}${arc.admissible ? " · admissible" : ""}`}</title>
									<path
										d={geometry.d}
										style={{ "--wap-width": `${width}px` } as CSSProperties}
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
									<text
										className="flow-panel-edge-label"
										x={geometry.label.x}
										y={geometry.label.y}
									>{`r${arc.capacity} · w${arc.weight}`}</text>
								</g>
							);
						})}
					</g>
					<g className="flow-wap-nodes">
						{overlay.nodes.map((node) => {
							const point = positions.get(node.node_id);
							if (point === undefined) return null;
							const className = [
								node.alive ? "is-alive" : "is-dead",
								node.expansion_witness_side ? "is-witness" : "",
								node.source_side ? "is-source-side" : "",
							]
								.filter(Boolean)
								.join(" ");
							return (
								<g
									key={node.node_id}
									className={className}
									transform={`translate(${point.x} ${point.y})`}
								>
									<title>{`${node.node_id} · component ${node.component} · τ ${node.order} · label ${node.label} · ${node.alive ? "alive" : "dead"}`}</title>
									<circle r="27" />
									<text className="flow-wap-node-id" y="-2">
										{node.node_id}
									</text>
									<text
										className="flow-wap-node-meta"
										y="17"
									>{`τ${node.order} · ℓ${compact(node.label)}`}</text>
								</g>
							);
						})}
					</g>
				</svg>
			</div>
			<footer className="flow-wap-footer">
				<ul
					className="flow-wap-legend"
					aria-label="Weighted augmenting paths legend"
				>
					<li className="is-expanding">X₁ expanding</li>
					<li className="is-dag">D DAG</li>
					<li className="is-admissible">admissible</li>
					<li className="is-active">active path</li>
					<li className="is-witness">φ witness side</li>
				</ul>
				<div className="flow-wap-path-readout">
					<span className="flow-wap-path-label">active residual path</span>
					<strong
						className="flow-wap-path-value"
						data-testid="flow-wap-active-path-readout"
					>
						{overlay.active_path.length === 0
							? "—"
							: overlay.active_path
									.map(
										(arc) =>
											`${arc.edge_id}${arc.direction === "forward" ? "→" : "←"}`,
									)
									.join("  ")}
					</strong>
				</div>
			</footer>
		</figure>
	);
}
