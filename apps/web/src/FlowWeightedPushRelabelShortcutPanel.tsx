import type { CSSProperties } from "react";
import {
	flowScopedDomId,
	flowScopedSvgUrl,
	useFlowDomIdScope,
} from "./flow-dom-id";
import { buildFlowPanelEdgeRoutes } from "./flow-panel-edge-routing";
import type { FlowWeightedPushRelabelShortcutOverlayV1 } from "./flow-scene";

type Point = { x: number; y: number };

const WIDTH = 980;
const HEIGHT = 390;

function compact(value: string): string {
	const parsed = BigInt(value);
	if (parsed < 10_000n) return value;
	return `${value.slice(0, 3)}e${value.length - 1}`;
}

const STAGE_PRESENTATION = {
	ready: {
		title: "Ready",
		copy: "Validate the bounded max-flow instance before constructing shortcuts",
	},
	"build-weak-hierarchy": {
		title: "Build weak hierarchy",
		copy: "Freeze residual SCCs as a one-level weak hierarchy",
	},
	"build-shortcut-graph": {
		title: "Build shortcut graph",
		copy: "Place a Steiner root in each SCC and add bidirectional star shortcuts",
	},
	"assign-weights": {
		title: "Assign weighted lengths",
		copy: "Weight original edges by |τ(u)−τ(v)| and shortcuts by SCC size",
	},
	"initialize-demand": {
		title: "Initialize demand",
		copy: "Initialize bounded demand and height h for measuring short flows",
	},
	"relabel-sweep": {
		title: "Relabel sweep",
		copy: "Sweep weighted labels until an admissible route can advance",
	},
	"relabel-checkpoint": {
		title: "Relabel one vertex",
		copy: "Show the exact vertex whose weighted label changes at this source checkpoint",
	},
	"inspect-primitive-arc-checkpoint": {
		title: "Inspect residual arc",
		copy: "Show the exact augmented residual direction inspected before the next path decision",
	},
	"augment-path": {
		title: "Augment weighted path",
		copy: "Push the bottleneck along the displayed admissible weighted path",
	},
	"measure-short-flow": {
		title: "Measure short flow",
		copy: "Measure routed units and weighted path length before selecting a cut",
	},
	"compute-distance-layers": {
		title: "Build distance layers",
		copy: "Compute modified residual distance layers used by the sparse-cut rule",
	},
	"select-sparse-cut": {
		title: "Select sparse cut",
		copy: "Choose a sparse cut from the modified residual distance layers",
	},
	"completion-inspect-primitive-arc-checkpoint": {
		title: "Inspect exact residual arc",
		copy: "Show the original residual direction examined inside exact completion",
	},
	"completion-relabel-checkpoint": {
		title: "Relabel exact-flow vertex",
		copy: "Show the original vertex whose label changes inside exact completion",
	},
	"completion-augment-path": {
		title: "Augment exact residual path",
		copy: "Apply one concrete original-residual path to the exact flow",
	},
	"completion-residual-round": {
		title: "Finish residual round",
		copy: "Publish the exact flow after one original-residual kernel call",
	},
	"complete-residual-rounds": {
		title: "Complete exact flow",
		copy: "Run weighted push–relabel on the original residual graph to complete the exact flow",
	},
	"check-certificate": {
		title: "Verify certificate",
		copy: "Independently verify a cut certificate equal to the maximum-flow value",
	},
	optimal: {
		title: "Optimal",
		copy: "Publish the maximum flow together with its equal-capacity cut certificate",
	},
} satisfies Record<
	FlowWeightedPushRelabelShortcutOverlayV1["stage"],
	Readonly<{ title: string; copy: string }>
>;

function positions(
	overlay: FlowWeightedPushRelabelShortcutOverlayV1,
): Map<string, Point> {
	const originals = overlay.nodes
		.filter((node) => node.original)
		.sort((left, right) => {
			const leftOrder = BigInt(left.order);
			const rightOrder = BigInt(right.order);
			if (leftOrder !== rightOrder) return leftOrder < rightOrder ? -1 : 1;
			return left.node_id < right.node_id
				? -1
				: left.node_id > right.node_id
					? 1
					: 0;
		});
	const result = new Map<string, Point>();
	for (let index = 0; index < originals.length; index += 1) {
		const node = originals[index];
		if (node === undefined) continue;
		result.set(node.node_id, {
			x: 84 + (index / Math.max(1, originals.length - 1)) * (WIDTH - 168),
			y: 245 + (index % 2 === 0 ? -24 : 24),
		});
	}
	for (const root of overlay.nodes.filter((node) => !node.original)) {
		const members = overlay.nodes
			.filter((node) => node.original && node.component === root.component)
			.map((node) => result.get(node.node_id))
			.filter((point): point is Point => point !== undefined);
		const x =
			members.length === 0
				? WIDTH / 2
				: members.reduce((sum, point) => sum + point.x, 0) / members.length;
		result.set(root.node_id, { x, y: 82 });
	}
	return result;
}

export function FlowWeightedPushRelabelShortcutPanel({
	overlay,
}: {
	overlay: FlowWeightedPushRelabelShortcutOverlayV1;
}) {
	const idScope = useFlowDomIdScope("flow-wpr");
	const titleId = flowScopedDomId(idScope, "title");
	const descriptionId = flowScopedDomId(idScope, "description");
	const arrowId = flowScopedDomId(idScope, "arrow");
	const glowId = flowScopedDomId(idScope, "glow");
	const nodePositions = positions(overlay);
	const arcRoutes = buildFlowPanelEdgeRoutes(
		overlay.residual_arcs.map((arc, index) => ({
			id: `${arc.edge_id}:${arc.direction}:${index}`,
			from: arc.from,
			to: arc.to,
		})),
		nodePositions,
		{
			width: WIDTH,
			height: HEIGHT,
			paddingX: 54,
			paddingY: 46,
			nodeRadius: 31,
			markerClearance: 11,
			labelWidth: 110,
			labelHeight: 22,
		},
	);
	const edgeById = new Map(overlay.edges.map((edge) => [edge.edge_id, edge]));
	const maximumResidual = overlay.residual_arcs.reduce((maximum, arc) => {
		const capacity = BigInt(arc.capacity);
		return capacity > maximum ? capacity : maximum;
	}, 1n);
	const componentMembers = new Map<string, string[]>();
	for (const node of overlay.nodes.filter((candidate) => candidate.original)) {
		const members = componentMembers.get(node.component) ?? [];
		members.push(node.node_id);
		componentMembers.set(node.component, members);
	}
	const stagePresentation = STAGE_PRESENTATION[overlay.stage];
	return (
		<figure
			className="flow-wpr-panel"
			data-testid="flow-weighted-push-relabel-panel"
			data-wpr-stage={overlay.stage}
			data-wpr-shortcuts={
				overlay.edges.filter((edge) => edge.kind === "shortcut").length
			}
			data-wpr-active-path={overlay.active_path.length}
			data-wpr-inspected-arcs={overlay.inspected_arcs.length}
			data-wpr-relabel-nodes={overlay.active_relabel_nodes.length}
		>
			<header className="flow-wpr-header">
				<div>
					<p className="flow-wpr-kicker">
						WEIGHTED PUSH–RELABEL · STEINER SHORTCUT GRAPH
					</p>
					<strong className="flow-wpr-title">{stagePresentation.title}</strong>
					<small className="flow-wpr-subtitle">{stagePresentation.copy}</small>
				</div>
				<dl>
					<div className="flow-wpr-stat">
						<dt>hierarchy / ψ</dt>
						<dd>{`${overlay.hierarchy_levels}L · ${overlay.psi_numerator}/${overlay.psi_denominator}`}</dd>
					</div>
					<div className="flow-wpr-stat">
						<dt>height h</dt>
						<dd>{compact(overlay.height)}</dd>
					</div>
					<div className="flow-wpr-stat">
						<dt>routed / demand</dt>
						<dd>{`${overlay.routed} / ${overlay.demand}`}</dd>
					</div>
					<div className="flow-wpr-stat">
						<dt>weighted avg</dt>
						<dd>{`${overlay.weighted_length}/${overlay.weighted_length_units}`}</dd>
					</div>
					<div className="flow-wpr-stat">
						<dt>relabel / path</dt>
						<dd>{`${overlay.relabel_steps} / ${overlay.augmentations}`}</dd>
					</div>
					<div className="flow-wpr-stat">
						<dt>sparse cut</dt>
						<dd>{`L${overlay.sparse_cut_level} · c${overlay.sparse_cut_capacity}`}</dd>
					</div>
				</dl>
			</header>
			<ol
				className="flow-wpr-stage-rail"
				aria-label="Weighted push-relabel lifecycle"
			>
				<li
					className={
						overlay.stage.includes("shortcut") ||
						overlay.stage.includes("hierarchy")
							? "is-active"
							: ""
					}
				>
					shortcut
				</li>
				<li
					className={
						overlay.stage.includes("relabel") ||
						overlay.stage.includes("inspect-primitive-arc-checkpoint") ||
						overlay.stage.endsWith("augment-path")
							? "is-active"
							: ""
					}
				>
					push / relabel
				</li>
				<li
					className={
						overlay.stage.includes("distance") ||
						overlay.stage.includes("sparse")
							? "is-active"
							: ""
					}
				>
					distance cut
				</li>
				<li
					className={
						[
							"completion-inspect-primitive-arc-checkpoint",
							"completion-relabel-checkpoint",
							"completion-augment-path",
							"completion-residual-round",
							"complete-residual-rounds",
							"check-certificate",
							"optimal",
						].includes(overlay.stage)
							? "is-active"
							: ""
					}
				>
					exact certificate
				</li>
			</ol>
			<div className="flow-wpr-graph-shell">
				<svg
					viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
					role="img"
					aria-labelledby={`${titleId} ${descriptionId}`}
					style={
						{
							"--flow-wpr-glow": flowScopedSvgUrl(idScope, "glow"),
						} as CSSProperties
					}
				>
					<title id={titleId}>Weighted push-relabel shortcut graph</title>
					<desc id={descriptionId}>
						Thicker strokes mean more residual capacity. Solid blue marks
						original edges, dashed cyan marks Steiner shortcuts, amber marks
						admissible edges, and white glow marks the active path. Circles are
						original nodes; diamonds are SCC shortcut roots.
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
						<filter id={glowId} x="-50%" y="-50%" width="200%" height="200%">
							<feGaussianBlur stdDeviation="4" result="blur" />
							<feMerge>
								<feMergeNode in="blur" />
								<feMergeNode in="SourceGraphic" />
							</feMerge>
						</filter>
					</defs>
					<g className="flow-wpr-components">
						{[...componentMembers].map(([component, members]) => {
							const points = members
								.map((member) => nodePositions.get(member))
								.filter((point): point is Point => point !== undefined);
							if (points.length === 0) return null;
							const root = nodePositions.get(`shortcut:${component}`);
							const minX =
								Math.min(...points.map((point) => point.x), root?.x ?? WIDTH) -
								50;
							const maxX =
								Math.max(...points.map((point) => point.x), root?.x ?? 0) + 50;
							const minY =
								Math.min(...points.map((point) => point.y), root?.y ?? HEIGHT) -
								46;
							const maxY =
								Math.max(...points.map((point) => point.y), root?.y ?? 0) + 46;
							return (
								<g key={component}>
									<rect
										x={minX}
										y={minY}
										width={maxX - minX}
										height={maxY - minY}
										rx="28"
									/>
									<text x={minX + 13} y={minY + 21}>{`SCC ${component}`}</text>
								</g>
							);
						})}
					</g>
					<g className="flow-wpr-arcs">
						{overlay.residual_arcs.map((arc, index) => {
							const inspected = overlay.inspected_arcs.some(
								(candidate) =>
									candidate.edge_id === arc.edge_id &&
									candidate.direction === arc.direction,
							);
							if (BigInt(arc.capacity) === 0n && !arc.active && !inspected)
								return null;
							const edge = edgeById.get(arc.edge_id);
							if (edge === undefined) return null;
							const shortcut = edge.kind === "shortcut";
							const geometry = arcRoutes.get(
								`${arc.edge_id}:${arc.direction}:${index}`,
							);
							if (geometry === undefined) return null;
							const width =
								2 +
								Number((BigInt(arc.capacity) * 520n) / maximumResidual) / 100;
							const className = [
								shortcut ? "is-shortcut" : "is-original",
								arc.direction === "reverse" ? "is-reverse" : "",
								arc.admissible ? "is-admissible" : "",
								arc.active ? "is-active" : "",
								inspected ? "is-inspected" : "",
							]
								.filter(Boolean)
								.join(" ");
							return (
								<g
									key={`${arc.edge_id}-${arc.direction}`}
									className={className}
									data-testid={
										inspected
											? "flow-wpr-inspected-arc"
											: arc.active
												? "flow-wpr-active-arc"
												: undefined
									}
								>
									<title>{`${arc.edge_id} ${arc.direction} · residual ${arc.capacity} · weight ${arc.weight}${arc.admissible ? " · admissible" : ""}`}</title>
									<path
										d={geometry.d}
										style={{ "--wpr-width": `${width}px` } as CSSProperties}
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
					<g className="flow-wpr-nodes">
						{overlay.nodes.map((node) => {
							const point = nodePositions.get(node.node_id);
							if (point === undefined) return null;
							const className = [
								node.original ? "is-original" : "is-shortcut-root",
								node.alive ? "is-alive" : "is-dead",
								node.sparse_cut_side ? "is-sparse-side" : "",
								node.source_side ? "is-source-side" : "",
								overlay.active_relabel_nodes.includes(node.node_id)
									? "is-relabel-target"
									: "",
							]
								.filter(Boolean)
								.join(" ");
							return (
								<g
									key={node.node_id}
									className={className}
									transform={`translate(${point.x} ${point.y})`}
									data-testid={
										overlay.active_relabel_nodes.includes(node.node_id)
											? "flow-wpr-relabel-node"
											: undefined
									}
								>
									<title>{`${node.node_id} · SCC ${node.component} · τ ${node.order} · weighted label ${node.label}`}</title>
									{node.original ? (
										<circle r="28" />
									) : (
										<rect
											x="-22"
											y="-22"
											width="44"
											height="44"
											rx="6"
											transform="rotate(45)"
										/>
									)}
									<text className="flow-wpr-node-id" y="-2">
										{node.original ? node.node_id : `★${node.component}`}
									</text>
									<text className="flow-wpr-node-meta" y="17">
										{node.original
											? `τ${node.order} · h${compact(node.label)}`
											: `w=${componentMembers.get(node.component)?.length ?? 0}`}
									</text>
								</g>
							);
						})}
					</g>
				</svg>
			</div>
			<footer className="flow-wpr-footer">
				<ul
					className="flow-wpr-legend"
					aria-label="Weighted push-relabel legend"
				>
					<li className="is-original">original residual</li>
					<li className="is-shortcut">Steiner shortcut</li>
					<li className="is-admissible">admissible</li>
					<li className="is-active">active path</li>
					<li className="is-inspected">inspected arc</li>
					<li className="is-relabel">relabel target</li>
					<li className="is-sparse">sparse-cut side</li>
				</ul>
				<div className="flow-wpr-path-readout">
					<span>active weighted path</span>
					<strong data-testid="flow-wpr-active-path-readout">
						{overlay.active_path.length === 0
							? "—"
							: overlay.active_path
									.map(
										(arc) =>
											`${arc.edge_id}${arc.direction === "forward" ? "→" : "←"}`,
									)
									.join("  ")}
					</strong>
					<small>{`shortcut traversals ${overlay.shortcut_traversals} · residual rounds ${overlay.residual_rounds} · completion augments ${overlay.completion_augmentations}`}</small>
				</div>
			</footer>
		</figure>
	);
}
