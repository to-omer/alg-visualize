import type { CSSProperties, ReactNode } from "react";

import { flowScopedSvgUrl, useFlowGraphIdScope } from "./flow-dom-id";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import { rationalMagnitudeStrokeWidth } from "./flow-graph-rational-scales";
import {
	FLOW_NODE_RADIUS,
	FLOW_VIEWBOX_HEIGHT,
	FLOW_VIEWBOX_WIDTH,
} from "./flow-layout";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
type Point = Readonly<{ x: number; y: number }>;
type OverlayField =
	| "augmenting_electrical_overlay"
	| "flow_framework_mcf_overlay"
	| "interior_point_max_flow_overlay"
	| "minimum_ratio_cycle_mcf_overlay"
	| "primal_dual_ipm_mcf_overlay"
	| "randomized_almost_linear_mcf_overlay"
	| "weighted_augmenting_paths_overlay"
	| "weighted_push_relabel_shortcut_overlay";

const FEATURE_BUNDLE = "advanced-algorithm";

function canonicalUnsignedInteger(value: string, label: string): bigint {
	if (!/^(0|[1-9][0-9]*)$/u.test(value)) {
		throw new Error(`${label} must be a canonical unsigned integer`);
	}
	return BigInt(value);
}

function visiblePivotOrdinals(active: bigint, count: bigint): bigint[] {
	if (active >= count) {
		throw new Error("augmenting-electrical pivot must identify a working node");
	}
	const maximumVisible = 22n;
	if (count <= maximumVisible) {
		return Array.from({ length: Number(count) }, (_, index) => BigInt(index));
	}
	let start = active > maximumVisible / 2n ? active - maximumVisible / 2n : 0n;
	if (start + maximumVisible > count) start = count - maximumVisible;
	return Array.from(
		{ length: Number(maximumVisible) },
		(_, index) => start + BigInt(index),
	);
}

function compareText(left: string, right: string): number {
	return left < right ? -1 : left > right ? 1 : 0;
}

function boundedInteger(value: string, maximum: number): number {
	try {
		return Number(BigInt(value) % BigInt(maximum));
	} catch {
		return 0;
	}
}

function compactFiniteScalar(value: string): string {
	const numeric = Number(value);
	if (!Number.isFinite(numeric)) {
		throw new Error(`advanced algorithm scalar is not finite: ${value}`);
	}
	if (numeric === 0) return "0";
	const magnitude = Math.abs(numeric);
	if (magnitude >= 1_000 || magnitude < 0.001) {
		return numeric.toExponential(2).replace("e+", "e");
	}
	return numeric
		.toPrecision(4)
		.replace(/(\.\d*?[1-9])0+$/u, "$1")
		.replace(/\.0+$/u, "");
}

function compactExactInteger(value: string): string {
	const exact = BigInt(value).toString();
	const negative = exact.startsWith("-");
	const digits = negative ? exact.slice(1) : exact;
	if (digits.length <= 9) return exact;
	return `${negative ? "−" : ""}${digits.slice(0, 4)}…${digits.slice(-3)}`;
}

function requiredAdvancedValue<T>(value: T | undefined, label: string): T {
	if (value === undefined) {
		throw new Error(`advanced algorithm state is missing ${label}`);
	}
	return value;
}

function decimalFlowStrokeWidth(flow: string, capacity: string): number {
	const numericFlow = Number(flow);
	const numericCapacity = Number(capacity);
	if (
		!Number.isFinite(numericFlow) ||
		!Number.isFinite(numericCapacity) ||
		numericFlow < 0 ||
		numericCapacity < 0 ||
		numericFlow > numericCapacity
	) {
		throw new Error(`invalid bounded flow coordinate ${flow}/${capacity}`);
	}
	if (numericCapacity === 0) return 1.5;
	return 2 + (5 * numericFlow) / numericCapacity;
}

function rationalFlowStrokeWidth(
	flow: Readonly<{ numerator: string; denominator: string }>,
	capacity: string,
): number {
	const numerator = BigInt(flow.numerator);
	const denominator = BigInt(flow.denominator);
	const capacityValue = BigInt(capacity);
	if (
		denominator <= 0n ||
		numerator < 0n ||
		capacityValue < 0n ||
		numerator > capacityValue * denominator
	) {
		throw new Error(
			`invalid exact bounded flow coordinate ${flow.numerator}/${flow.denominator} over ${capacity}`,
		);
	}
	if (capacityValue === 0n) return 1.5;
	const scaled = (numerator * 5_000n) / (capacityValue * denominator);
	return 2 + Number(scaled) / 1_000;
}

function compactRational(
	value: Readonly<{ numerator: string; denominator: string }>,
): string {
	return value.denominator === "1"
		? compactExactInteger(value.numerator)
		: `${compactExactInteger(value.numerator)}/${compactExactInteger(value.denominator)}`;
}

function directedLinePath(from: Point, to: Point, clearance = 12): string {
	const dx = to.x - from.x;
	const dy = to.y - from.y;
	const distance = Math.max(1, Math.hypot(dx, dy));
	const unitX = dx / distance;
	const unitY = dy / distance;
	return `M ${from.x + unitX * clearance} ${from.y + unitY * clearance} L ${to.x - unitX * clearance} ${to.y - unitY * clearance}`;
}

function AdvancedPath({
	contribution,
	entityKind,
	entityId,
	visualRole,
	path,
	width,
	markerEnd,
	residualDirection,
	title,
}: Readonly<{
	contribution: OverlayField;
	entityKind:
		| "edge"
		| "residual-arc"
		| "auxiliary-edge"
		| "auxiliary-residual-arc";
	entityId: string;
	visualRole: string;
	path: string;
	width: number;
	markerEnd?: string | undefined;
	residualDirection?: "forward" | "reverse" | undefined;
	title: string;
}>) {
	return (
		<path
			className="flow-advanced-algorithm-path"
			data-overlay-contribution={contribution}
			data-overlay-feature-bundle={FEATURE_BUNDLE}
			data-overlay-entity-kind={entityKind}
			data-overlay-entity-id={entityId}
			data-overlay-residual-direction={residualDirection}
			data-overlay-role={visualRole}
			d={path}
			markerEnd={markerEnd}
			style={{ "--flow-advanced-width": width } as CSSProperties}
		>
			<title>{title}</title>
		</path>
	);
}

function AdvancedNodeMark({
	contribution,
	entityKind = "node",
	entityId,
	visualRole,
	position,
	shape = "dot",
	originalNodeId,
	originalEdgeId,
	title,
}: Readonly<{
	contribution: OverlayField;
	entityKind?: "node" | "auxiliary-node";
	entityId: string;
	visualRole: string;
	position: Point;
	shape?: "dot" | "ring" | "capacity" | "steiner";
	originalNodeId?: string | undefined;
	originalEdgeId?: string | undefined;
	title: string;
}>) {
	if (shape === "ring") {
		return (
			<circle
				className="flow-advanced-algorithm-node"
				data-overlay-contribution={contribution}
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind={entityKind}
				data-overlay-entity-id={entityId}
				data-overlay-original-node-id={originalNodeId}
				data-overlay-original-edge-id={originalEdgeId}
				data-overlay-role={visualRole}
				cx={position.x}
				cy={position.y}
				r={FLOW_NODE_RADIUS + 6}
			>
				<title>{title}</title>
			</circle>
		);
	}
	if (shape === "capacity" || shape === "steiner") {
		const size = shape === "capacity" ? 9 : 10;
		return (
			<path
				className="flow-advanced-algorithm-node"
				data-overlay-contribution={contribution}
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind={entityKind}
				data-overlay-entity-id={entityId}
				data-overlay-original-node-id={originalNodeId}
				data-overlay-original-edge-id={originalEdgeId}
				data-overlay-role={visualRole}
				d={
					shape === "capacity"
						? `M ${position.x - size} ${position.y - size} h ${size * 2} v ${size * 2} h ${-size * 2} Z`
						: `M ${position.x} ${position.y - size} L ${position.x + size} ${position.y} L ${position.x} ${position.y + size} L ${position.x - size} ${position.y} Z`
				}
			>
				<title>{title}</title>
			</path>
		);
	}
	return (
		<circle
			className="flow-advanced-algorithm-node"
			data-overlay-contribution={contribution}
			data-overlay-feature-bundle={FEATURE_BUNDLE}
			data-overlay-entity-kind={entityKind}
			data-overlay-entity-id={entityId}
			data-overlay-original-node-id={originalNodeId}
			data-overlay-original-edge-id={originalEdgeId}
			data-overlay-role={visualRole}
			cx={position.x + FLOW_NODE_RADIUS * 0.72}
			cy={position.y - FLOW_NODE_RADIUS * 0.72}
			r="5"
		>
			<title>{title}</title>
		</circle>
	);
}

function AdvancedAnchoredBadge({
	contribution,
	entityKind,
	entityId,
	visualRole,
	position,
	label,
	title,
	tone,
	originalNodeId,
	originalEdgeId,
}: Readonly<{
	contribution: OverlayField;
	entityKind: "edge" | "auxiliary-edge" | "auxiliary-node";
	entityId: string;
	visualRole: string;
	position: Point;
	label: string;
	title: string;
	tone: "amber" | "teal";
	originalNodeId?: string | undefined;
	originalEdgeId?: string | undefined;
}>) {
	const badgeX = Math.max(92, Math.min(FLOW_VIEWBOX_WIDTH - 92, position.x));
	const above = position.y >= 76;
	const badgeY = above ? position.y - 48 : position.y + 48;
	const ownership = {
		"data-overlay-contribution": contribution,
		"data-overlay-feature-bundle": FEATURE_BUNDLE,
		"data-overlay-entity-kind": entityKind,
		"data-overlay-entity-id": entityId,
		"data-overlay-original-node-id": originalNodeId,
		"data-overlay-original-edge-id": originalEdgeId,
		"data-overlay-role": visualRole,
	} as const;
	return (
		<g
			className={`flow-advanced-anchored-badge flow-advanced-anchored-badge-${tone}`}
			aria-label={title}
		>
			<title>{title}</title>
			<line
				{...ownership}
				className="flow-advanced-anchored-badge-leader"
				x1={position.x}
				y1={position.y}
				x2={badgeX}
				y2={badgeY + (above ? 13 : -13)}
			/>
			<circle
				{...ownership}
				className="flow-advanced-anchored-badge-anchor"
				cx={position.x}
				cy={position.y}
				r="4"
			/>
			<rect
				{...ownership}
				className="flow-advanced-anchored-badge-box"
				x={badgeX - 82}
				y={badgeY - 13}
				width="164"
				height="26"
				rx="13"
			/>
			<text
				{...ownership}
				className="flow-advanced-anchored-badge-label"
				x={badgeX}
				y={badgeY}
				textAnchor="middle"
				dominantBaseline="central"
			>
				{label}
			</text>
		</g>
	);
}

function FrameworkMcfLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.flowFrameworkMcf;
	const idScope = useFlowGraphIdScope();
	if (overlay === undefined) return null;
	const maximumFlow = overlay.edges.reduce(
		(maximum, edge) => {
			const left = BigInt(edge.flow.numerator) * BigInt(maximum.denominator);
			const right = BigInt(maximum.numerator) * BigInt(edge.flow.denominator);
			return left > right ? edge.flow : maximum;
		},
		{ numerator: "0", denominator: "1" },
	);
	const anchorEdge =
		overlay.edges.find((edge) => edge.selected) ?? overlay.edges[0];
	if (anchorEdge === undefined) {
		throw new Error("Flow Framework MCF overlay has no graph-edge anchor");
	}
	const anchorRoute = state.layout.routes.get(anchorEdge.edge_id);
	if (anchorRoute === undefined) {
		throw new Error(
			`Flow Framework MCF route is missing for ${anchorEdge.edge_id}`,
		);
	}
	const operationBadge = (() => {
		const operation = overlay.dynamic_operation;
		if (operation !== undefined) {
			const serial = compactExactInteger(
				requiredAdvancedValue(
					overlay.dynamic_operation_serial,
					"dynamic operation serial",
				),
			);
			const definition = (() => {
				switch (operation) {
					case "topology-stage-applied":
						return ["TOPOLOGY", "applied a topology stage", "amber"] as const;
					case "periodic-rebuilt":
						return [
							"REBUILD",
							"rebuilt the periodic hierarchy",
							"amber",
						] as const;
					case "cycle-queried-accepted":
						return ["QUERY ✓", "accepted the queried cycle", "teal"] as const;
					case "cycle-queried-rejected":
						return ["QUERY ×", "rejected the queried cycle", "amber"] as const;
					case "level-shifted":
						return [
							"SHIFT",
							"shifted the active hierarchy level",
							"amber",
						] as const;
					case "flow-applied":
						return [
							"APPLY FLOW",
							"applied the exact cycle flow",
							"teal",
						] as const;
					case "query-returned":
						return [
							"RETURN CYCLE",
							"returned the selected cycle",
							"teal",
						] as const;
					case "detect-returned":
						return [
							"DETECT",
							"returned the detected coordinate",
							"teal",
						] as const;
					case "completed":
						return [
							"RETURN FLOW",
							"completed the dynamic flow call",
							"teal",
						] as const;
				}
			})();
			return {
				label: `DYN #${serial} · ${definition[0]}`,
				role: `dynamic-${operation}`,
				title: `Dynamic operation ${overlay.dynamic_operation_serial} ${definition[1]}; badge anchor is graph edge ${anchorEdge.edge_id}`,
				tone: definition[2],
			};
		}
		switch (overlay.stage) {
			case "initialize-source-point":
				return {
					label: `START · GAP ${compactRational(overlay.exact_gap_after)}`,
					role: "initialize-gap",
					title: `Initialized the source point with exact gap ${compactRational(overlay.exact_gap_after)}; badge anchor is graph edge ${anchorEdge.edge_id}`,
					tone: "amber" as const,
				};
			case "periodic-reinitialize":
				return {
					label: `ITER ${compactExactInteger(overlay.iteration)} · REINIT`,
					role: "outer-reinitialize",
					title: `Started source iteration ${overlay.iteration} with periodic reinitialization; badge anchor is graph edge ${anchorEdge.edge_id}`,
					tone: "amber" as const,
				};
			case "round-fractional-flow":
				return {
					label: `ROUND · GAP ${compactRational(overlay.exact_gap_after)}`,
					role: "round-final-point",
					title: `Rounded the exact final point at gap ${compactRational(overlay.exact_gap_after)}; badge anchor is graph edge ${anchorEdge.edge_id}`,
					tone: "teal" as const,
				};
			case "check-certificate":
				return {
					label: `CHECK · COST ${compactExactInteger(requiredAdvancedValue(overlay.optimum_cost, "certificate cost"))}`,
					role: "check-rounded-certificate",
					title: `Checked rounded flow against optimum cost ${overlay.optimum_cost}; badge anchor is graph edge ${anchorEdge.edge_id}`,
					tone: "amber" as const,
				};
			case "optimal":
				return {
					label: `CERTIFIED · ${compactExactInteger(requiredAdvancedValue(overlay.optimum_cost, "certified cost"))}`,
					role: "verified-optimum",
					title: `Verified the exact minimum-cost certificate with cost ${overlay.optimum_cost}; badge anchor is graph edge ${anchorEdge.edge_id}`,
					tone: "teal" as const,
				};
			case "detect":
			case "query-minimum-ratio-cycle":
			case "source-progress":
				throw new Error(
					`Flow Framework MCF ${overlay.stage} stage lacks its exact dynamic operation`,
				);
		}
	})();
	const levelCount = Math.max(1, overlay.levels.length);
	return (
		<g
			className="flow-advanced-algorithm-layer"
			data-advanced-overlay="flow-framework-mcf"
			data-advanced-stage={overlay.stage}
		>
			{overlay.edges.map((edge) => {
				const route = state.layout.routes.get(edge.edge_id);
				if (route === undefined) {
					throw new Error(
						`Flow Framework MCF route is missing for ${edge.edge_id}`,
					);
				}
				const selectedQuery =
					edge.selected && overlay.stage === "query-minimum-ratio-cycle";
				const selectedDetection = edge.selected && overlay.stage === "detect";
				const role = (() => {
					if (selectedQuery) return "selected-cycle";
					if (selectedDetection) return "detected-coordinate";
					switch (overlay.stage) {
						case "source-progress":
							return edge.flow.numerator === "0"
								? "source-progress-flow-zero"
								: "source-progress-flow";
						case "periodic-reinitialize":
							return "rebuilt-coordinate";
						case "round-fractional-flow":
							return "final-point-flow";
						case "check-certificate":
							return "certificate-flow-check";
						case "optimal":
							return "certificate-flow-verified";
						case "initialize-source-point":
						case "detect":
						case "query-minimum-ratio-cycle":
							return "fractional-flow";
					}
				})();
				return (
					<AdvancedPath
						key={edge.edge_id}
						contribution="flow_framework_mcf_overlay"
						entityKind="edge"
						entityId={edge.edge_id}
						visualRole={role}
						path={route.path}
						width={
							selectedQuery || selectedDetection
								? 6
								: rationalMagnitudeStrokeWidth(edge.flow, maximumFlow)
						}
						markerEnd={
							selectedQuery || selectedDetection
								? flowScopedSvgUrl(idScope, "flow-arrow-advanced-active")
								: undefined
						}
						title={`${edge.edge_id}: fractional flow ${edge.flow.numerator}/${edge.flow.denominator}; cycle coefficient ${edge.cycle_coefficient.numerator}/${edge.cycle_coefficient.denominator}`}
					/>
				);
			})}
			<AdvancedAnchoredBadge
				contribution="flow_framework_mcf_overlay"
				entityKind="edge"
				entityId={anchorEdge.edge_id}
				visualRole={operationBadge.role}
				position={anchorRoute.routeMidpoint}
				label={operationBadge.label}
				title={operationBadge.title}
				tone={operationBadge.tone}
			/>
			<text
				className="flow-advanced-algorithm-level-caption"
				data-overlay-contribution="flow_framework_mcf_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind="hierarchy-summary"
				data-overlay-entity-id="dynamic-levels"
				data-overlay-role="hierarchy-caption"
				x="16"
				y="12"
			>
				DYNAMIC LEVELS
			</text>
			{overlay.levels.map((level, index) => {
				const width = 150 / levelCount;
				const x = 16 + index * width;
				return (
					<g key={level.level}>
						<text
							className="flow-advanced-algorithm-level-label"
							data-overlay-contribution="flow_framework_mcf_overlay"
							data-overlay-feature-bundle={FEATURE_BUNDLE}
							data-overlay-entity-kind="hierarchy-level"
							data-overlay-entity-id={level.level}
							data-overlay-role="hierarchy-label"
							x={x + Math.max(4, width - 3) / 2}
							y="24"
						>
							{`L${level.level}`}
						</text>
						<rect
							className="flow-advanced-algorithm-level"
							data-overlay-contribution="flow_framework_mcf_overlay"
							data-overlay-feature-bundle={FEATURE_BUNDLE}
							data-overlay-entity-kind="hierarchy-level"
							data-overlay-entity-id={level.level}
							data-overlay-role="hierarchy-pass"
							data-level-branch={level.active_branch}
							x={x}
							y="29"
							width={Math.max(4, width - 3)}
							height={6 + Math.min(10, boundedInteger(level.passes, 11))}
							rx="2"
						>
							<title>{`Hierarchy level ${level.level}: branch ${level.active_branch}, ${level.passes} completed passes`}</title>
						</rect>
					</g>
				);
			})}
		</g>
	);
}

function MinimumRatioMcfLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.minimumRatioCycleMcf;
	const idScope = useFlowGraphIdScope();
	if (overlay === undefined) return null;
	const completionAnchor =
		overlay.stage === "complete" ? overlay.nodes[0] : undefined;
	const completionAnchorPosition =
		completionAnchor === undefined
			? undefined
			: state.positions.get(completionAnchor.node_id);
	let fixedFaceBadge:
		| Readonly<{
				edgeId: string;
				position: Point;
				active: number;
				fixed: number;
		  }>
		| undefined;
	if (overlay.stage === "contract-fixed-face") {
		const anchor = overlay.edges[0];
		if (anchor === undefined) {
			throw new Error("minimum-ratio fixed-face audit has no edge coordinate");
		}
		const route = state.layout.routes.get(anchor.edge_id);
		if (route === undefined) {
			throw new Error(
				`minimum-ratio fixed-face route is missing for ${anchor.edge_id}`,
			);
		}
		const fixed = overlay.edges.filter((edge) => edge.fixed_on_face).length;
		fixedFaceBadge = {
			edgeId: anchor.edge_id,
			position: route.routeMidpoint,
			active: overlay.edges.length - fixed,
			fixed,
		};
	}
	let sourceComputationBadge:
		| Readonly<{
				edgeId: string;
				label: string;
				position: Point;
				role: string;
				title: string;
				tone: "amber" | "teal";
		  }>
		| undefined;
	const sourceBadgeDefinition = (() => {
		switch (overlay.stage) {
			case "evaluate-potential":
				return {
					label: `Φ ${compactFiniteScalar(overlay.current_potential)}`,
					role: "source-potential",
					title: `Exact α-power source potential Φ = ${overlay.current_potential}`,
					tone: "amber" as const,
				};
			case "evaluate-cycle":
				if (overlay.candidate_ratio === undefined) {
					throw new Error(
						"evaluated minimum-ratio cycle has no candidate ratio",
					);
				}
				return {
					label: `RATIO ${compactFiniteScalar(overlay.candidate_ratio)}`,
					role: "candidate-ratio-summary",
					title: `Exact candidate cycle ratio ${overlay.candidate_ratio}`,
					tone: "amber" as const,
				};
			case "update-best":
				if (overlay.best_ratio === undefined) {
					throw new Error("minimum-ratio best update has no selected ratio");
				}
				return {
					label: `BEST ${compactFiniteScalar(overlay.best_ratio)}`,
					role: "best-ratio-summary",
					title: `New exact minimum ratio ${overlay.best_ratio}`,
					tone: "teal" as const,
				};
			case "verify-cycle-space":
				return {
					label: `CYCLE SPACE · ${overlay.fundamental_cycles}D`,
					role: "cycle-space-certificate",
					title: `Exact cycle-space enumeration verified ${overlay.simple_cycles} simple cycles from ${overlay.enumerated_vectors} ternary vectors in dimension ${overlay.fundamental_cycles}`,
					tone: "teal" as const,
				};
			case "measure-potential-decrease":
				return {
					label: `ΔΦ ${compactFiniteScalar(overlay.potential_decrease)} ≥ ${compactFiniteScalar(overlay.guaranteed_decrease)}`,
					role: "potential-decrease-certificate",
					title: `Measured source potential decrease ${overlay.potential_decrease}; guaranteed decrease ${overlay.guaranteed_decrease}`,
					tone: "teal" as const,
				};
			case "check-dfs-oracle":
				return {
					label: "DFS = SOURCE",
					role: "dfs-oracle-check",
					title:
						"Independent exact DFS cycle enumeration agrees with the source minimum-ratio cycle",
					tone: "teal" as const,
				};
			default:
				return undefined;
		}
	})();
	if (sourceBadgeDefinition !== undefined) {
		const anchor =
			overlay.edges.find((edge) => edge.candidate_sign !== "0") ??
			overlay.edges.find((edge) => edge.selected_sign !== "0") ??
			overlay.edges.find((edge) => edge.tree_edge) ??
			overlay.edges.find((edge) => !edge.fixed_on_face) ??
			overlay.edges[0];
		if (anchor === undefined) {
			throw new Error("minimum-ratio source computation has no edge anchor");
		}
		const route = state.layout.routes.get(anchor.edge_id);
		if (route === undefined) {
			throw new Error(
				`minimum-ratio source route is missing for ${anchor.edge_id}`,
			);
		}
		sourceComputationBadge = {
			...sourceBadgeDefinition,
			edgeId: anchor.edge_id,
			position: route.routeMidpoint,
		};
	}
	return (
		<g
			className="flow-advanced-algorithm-layer"
			data-advanced-overlay="minimum-ratio-cycle-mcf"
			data-advanced-stage={overlay.stage}
		>
			{overlay.edges.map((edge) => {
				const route = state.layout.routes.get(edge.edge_id);
				if (route === undefined) return null;
				const selected = edge.selected_sign !== "0";
				const candidate = edge.candidate_sign !== "0";
				const role = selected
					? "selected-cycle"
					: candidate
						? "candidate-cycle"
						: edge.tree_edge
							? "cycle-basis-tree"
							: edge.fixed_on_face
								? "fixed-coordinate"
								: "gradient-coordinate";
				const reverse = selected
					? edge.selected_sign === "-1"
					: candidate && edge.candidate_sign === "-1";
				return (
					<AdvancedPath
						key={edge.edge_id}
						contribution="minimum_ratio_cycle_mcf_overlay"
						entityKind="edge"
						entityId={edge.edge_id}
						visualRole={role}
						path={reverse ? route.reversePath : route.path}
						width={selected ? 6 : candidate ? 4.5 : edge.tree_edge ? 2.8 : 1.5}
						markerEnd={
							selected || candidate
								? flowScopedSvgUrl(
										idScope,
										selected
											? "flow-arrow-advanced-active"
											: "flow-arrow-advanced-candidate",
									)
								: undefined
						}
						title={`${edge.edge_id}: gradient ${edge.gradient}, length ${edge.length}, candidate ${edge.candidate_sign}, selected ${edge.selected_sign}`}
					/>
				);
			})}
			{overlay.nodes.flatMap((node) => {
				if (!node.on_candidate && !node.on_selected) return [];
				const position = state.positions.get(node.node_id);
				if (position === undefined) return [];
				return [
					<AdvancedNodeMark
						key={node.node_id}
						contribution="minimum_ratio_cycle_mcf_overlay"
						entityId={node.node_id}
						visualRole={node.on_selected ? "selected-cycle" : "candidate-cycle"}
						position={position}
						shape={node.on_selected ? "ring" : "dot"}
						title={`${node.node_id}: component ${node.component}, depth ${node.depth}, candidate balance ${node.candidate_balance}`}
					/>,
				];
			})}
			{completionAnchor !== undefined &&
				completionAnchorPosition !== undefined && (
					<AdvancedNodeMark
						contribution="minimum_ratio_cycle_mcf_overlay"
						entityId={completionAnchor.node_id}
						visualRole="dfs-oracle-certified"
						position={completionAnchorPosition}
						title={`${completionAnchor.node_id}: exact DFS oracle agrees; one-step minimum-ratio-cycle primitive is certified complete`}
					/>
				)}
			{fixedFaceBadge !== undefined && (
				<AdvancedAnchoredBadge
					contribution="minimum_ratio_cycle_mcf_overlay"
					entityKind="edge"
					entityId={fixedFaceBadge.edgeId}
					visualRole="fixed-face-summary"
					position={fixedFaceBadge.position}
					label={`${fixedFaceBadge.active} ACTIVE · ${fixedFaceBadge.fixed} FIXED`}
					title={`Exact feasible-face contraction retained ${fixedFaceBadge.active} active edge coordinates and fixed ${fixedFaceBadge.fixed}`}
					tone="amber"
				/>
			)}
			{sourceComputationBadge !== undefined && (
				<AdvancedAnchoredBadge
					contribution="minimum_ratio_cycle_mcf_overlay"
					entityKind="edge"
					entityId={sourceComputationBadge.edgeId}
					visualRole={sourceComputationBadge.role}
					position={sourceComputationBadge.position}
					label={sourceComputationBadge.label}
					title={sourceComputationBadge.title}
					tone={sourceComputationBadge.tone}
				/>
			)}
		</g>
	);
}

function primalDualAuxiliaryPositions(
	state: FlowEntityGraphState,
): ReadonlyMap<string, Point> {
	const overlay = state.renderData.overlayViews.primalDualIpmMcf;
	const result = new Map<string, Point>();
	if (overlay === undefined) return result;
	for (const node of overlay.nodes) {
		if (node.kind !== "original" || node.original_node_id === undefined)
			continue;
		const position = state.positions.get(node.original_node_id);
		if (position !== undefined) result.set(node.auxiliary_id, position);
	}
	const groups = new Map<string, typeof overlay.nodes>();
	for (const node of overlay.nodes) {
		if (node.kind !== "capacity" || node.original_edge_id === undefined)
			continue;
		const group = groups.get(node.original_edge_id) ?? [];
		group.push(node);
		groups.set(node.original_edge_id, group);
	}
	for (const [edgeId, nodes] of groups) {
		const route = state.layout.routes.get(edgeId);
		const edge = state.plan.edges.find((candidate) => candidate.id === edgeId);
		const from =
			edge === undefined ? undefined : state.positions.get(edge.from);
		const to = edge === undefined ? undefined : state.positions.get(edge.to);
		if (route === undefined || from === undefined || to === undefined) continue;
		const dx = to.x - from.x;
		const dy = to.y - from.y;
		const distance = Math.max(1, Math.hypot(dx, dy));
		const normal = { x: -dy / distance, y: dx / distance };
		for (const [index, node] of [...nodes]
			.sort((left, right) => compareText(left.auxiliary_id, right.auxiliary_id))
			.entries()) {
			const lane = 24 + index * 18;
			result.set(node.auxiliary_id, {
				x: route.routeMidpoint.x + normal.x * lane,
				y: route.routeMidpoint.y + normal.y * lane,
			});
		}
	}
	return result;
}

function PrimalDualIpmMcfLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.primalDualIpmMcf;
	const idScope = useFlowGraphIdScope();
	if (overlay === undefined) return null;
	const positions = primalDualAuxiliaryPositions(state);
	let forestSubsetBadge:
		| Readonly<{
				count: number;
				entityId: string;
				entityKind: "auxiliary-edge" | "auxiliary-node";
				originalEdgeId?: string | undefined;
				originalNodeId?: string | undefined;
				position: Point;
				serial: string;
		  }>
		| undefined;
	if (overlay.stage === "inspect-forest-subset") {
		if (overlay.forest_subset_serial === undefined) {
			throw new Error("forest-subset inspection has no source ordinal");
		}
		canonicalUnsignedInteger(
			overlay.forest_subset_serial,
			"primal-dual forest-subset ordinal",
		);
		const candidates = overlay.arcs.filter((arc) => arc.forest_candidate);
		const anchorArc = candidates[0];
		if (anchorArc !== undefined) {
			const from = positions.get(anchorArc.from);
			const to = positions.get(anchorArc.to);
			if (from === undefined || to === undefined) {
				throw new Error(
					`forest-subset geometry is missing for ${anchorArc.auxiliary_id}`,
				);
			}
			forestSubsetBadge = {
				count: candidates.length,
				entityId: anchorArc.auxiliary_id,
				entityKind: "auxiliary-edge",
				originalEdgeId: anchorArc.original_edge_id,
				position: { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 },
				serial: overlay.forest_subset_serial,
			};
		} else {
			const anchorNode = overlay.nodes[0];
			const position =
				anchorNode === undefined
					? undefined
					: positions.get(anchorNode.auxiliary_id);
			if (anchorNode === undefined || position === undefined) {
				throw new Error(
					"empty forest-subset inspection has no auxiliary anchor",
				);
			}
			forestSubsetBadge = {
				count: 0,
				entityId: anchorNode.auxiliary_id,
				entityKind: "auxiliary-node",
				originalEdgeId: anchorNode.original_edge_id,
				originalNodeId: anchorNode.original_node_id,
				position,
				serial: overlay.forest_subset_serial,
			};
		}
	}
	let kernelStageBadge:
		| Readonly<{
				entityId: string;
				entityKind: "auxiliary-edge" | "auxiliary-node";
				label: string;
				originalEdgeId?: string | undefined;
				originalNodeId?: string | undefined;
				position: Point;
				role: string;
				title: string;
				tone: "amber" | "teal";
		  }>
		| undefined;
	const kernelStageDefinition = (() => {
		const deleted = overlay.arcs.filter((arc) => arc.deleted).length;
		const contracted = overlay.arcs.filter((arc) => arc.contracted).length;
		const active = overlay.arcs.filter((arc) => arc.in_minor).length;
		switch (overlay.stage) {
			case "build-minor":
				return {
					label: `MINOR ${active} · D${deleted}/C${contracted}`,
					role: "minor-summary",
					title: `Sticky minor contains ${active} active auxiliary arcs, ${deleted} primal deletions, and ${contracted} dual contractions`,
					tone: "amber" as const,
				};
			case "decrease-mu":
				return {
					label: `μ ${compactExactInteger(overlay.mu)}`,
					role: "barrier-decrease",
					title: `Exact integral central-path parameter decreased to μ = ${overlay.mu}`,
					tone: "amber" as const,
				};
			case "centering-cycle-update":
				return {
					label: `CYCLE α ${compactExactInteger(overlay.cycle_alpha)}`,
					role: "centering-cycle-correction",
					title: `Rounded fundamental-cycle correction α = ${overlay.cycle_alpha}; exact centrality numerator ${overlay.centrality_numerator}`,
					tone: "amber" as const,
				};
			case "centered":
				return {
					label: `CENTERED · Δ ${compactExactInteger(overlay.centrality_numerator)}`,
					role: "centered-certificate",
					title: `Minor recentered with exact one-norm centrality numerator ${overlay.centrality_numerator} at μ = ${overlay.mu}`,
					tone: "teal" as const,
				};
			case "proxy-reached":
				return {
					label: `PROXY GAP ${compactExactInteger(overlay.proxy_gap)}`,
					role: "proxy-threshold",
					title: `Exact proxy gap ${overlay.proxy_gap} satisfies 81·gap < 4·β·γ with β = ${overlay.beta} and γ = ${overlay.gamma}`,
					tone: "teal" as const,
				};
			case "restore-original-dual":
				return {
					label: "ORIGINAL-COST DUAL",
					role: "original-dual-restored",
					title:
						"Original-cost dual potentials and nonnegative reduced slacks restored on the crossover tree",
					tone: "teal" as const,
				};
			case "recover-admissible-flow":
				return {
					label: "ADMISSIBLE FLOW",
					role: "admissible-flow-recovered",
					title:
						"Integral admissible flow recovered on the zero-reduced-cost auxiliary network and lifted to original edges",
					tone: "teal" as const,
				};
			case "check-certificate":
				return {
					label: "FLOW + DUAL CHECK",
					role: "minimum-cost-certificate-check",
					title:
						"Checking original-edge feasibility, flow balance, and minimum-cost dual certificate",
					tone: "amber" as const,
				};
			case "optimal":
				return {
					label: "OPTIMAL ✓",
					role: "minimum-cost-certificate-accepted",
					title:
						"Original integral flow and minimum-cost dual certificate are accepted",
					tone: "teal" as const,
				};
			default:
				return undefined;
		}
	})();
	if (kernelStageDefinition !== undefined) {
		const anchorArc =
			overlay.arcs.find((arc) => arc.auxiliary_id === overlay.sampled_arc) ??
			overlay.arcs.find((arc) => arc.active_cycle_sign !== "0") ??
			overlay.arcs.find((arc) => arc.in_tree) ??
			overlay.arcs.find((arc) => arc.in_minor) ??
			overlay.arcs[0];
		if (anchorArc !== undefined) {
			const from = positions.get(anchorArc.from);
			const to = positions.get(anchorArc.to);
			if (from === undefined || to === undefined) {
				throw new Error(
					`primal-dual stage geometry is missing for ${anchorArc.auxiliary_id}`,
				);
			}
			kernelStageBadge = {
				...kernelStageDefinition,
				entityId: anchorArc.auxiliary_id,
				entityKind: "auxiliary-edge",
				originalEdgeId: anchorArc.original_edge_id,
				position: { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 },
			};
		} else {
			const anchorNode = overlay.nodes[0];
			const position =
				anchorNode === undefined
					? undefined
					: positions.get(anchorNode.auxiliary_id);
			if (anchorNode === undefined || position === undefined) {
				throw new Error("primal-dual stage has no auxiliary graph anchor");
			}
			kernelStageBadge = {
				...kernelStageDefinition,
				entityId: anchorNode.auxiliary_id,
				entityKind: "auxiliary-node",
				originalEdgeId: anchorNode.original_edge_id,
				originalNodeId: anchorNode.original_node_id,
				position,
			};
		}
	}
	return (
		<g
			className="flow-advanced-algorithm-layer"
			data-advanced-overlay="primal-dual-ipm-mcf"
			data-advanced-stage={overlay.stage}
			data-primal-dual-native-node-ids={overlay.nodes
				.map((node) => node.auxiliary_id)
				.join("|")}
			data-primal-dual-native-edge-ids={overlay.arcs
				.map((arc) => arc.auxiliary_id)
				.join("|")}
		>
			{overlay.arcs.map((arc) => {
				const from = positions.get(arc.from);
				const to = positions.get(arc.to);
				if (from === undefined || to === undefined) return null;
				const active = arc.active_cycle_sign !== "0";
				const sampled = arc.auxiliary_id === overlay.sampled_arc;
				const role = active
					? "centering-cycle"
					: sampled
						? "sampled-arc"
						: arc.forest_candidate
							? "forest-candidate"
							: arc.in_tree
								? "low-stretch-tree"
								: arc.deleted
									? "deleted-arc"
									: arc.contracted
										? "contracted-arc"
										: arc.in_minor
											? "active-minor"
											: "auxiliary-arc";
				return (
					<AdvancedPath
						key={arc.auxiliary_id}
						contribution="primal_dual_ipm_mcf_overlay"
						entityKind="auxiliary-edge"
						entityId={arc.auxiliary_id}
						visualRole={role}
						path={directedLinePath(from, to, 11)}
						width={active ? 6 : sampled ? 5 : arc.in_tree ? 3 : 1.5}
						markerEnd={
							active || sampled || arc.forest_candidate
								? flowScopedSvgUrl(
										idScope,
										active || sampled
											? "flow-arrow-advanced-active"
											: "flow-arrow-advanced-tree",
									)
								: undefined
						}
						title={`${arc.auxiliary_id}: ${arc.kind} arc, flow ${arc.flow}, slack ${arc.slack}${arc.resistance === undefined ? "" : `, resistance ${arc.resistance}`}`}
					/>
				);
			})}
			{overlay.nodes.flatMap((node) => {
				const position = positions.get(node.auxiliary_id);
				if (position === undefined) return [];
				return [
					<AdvancedNodeMark
						key={node.auxiliary_id}
						contribution="primal_dual_ipm_mcf_overlay"
						entityKind="auxiliary-node"
						entityId={node.auxiliary_id}
						originalNodeId={node.original_node_id}
						originalEdgeId={node.original_edge_id}
						visualRole={
							node.kind === "capacity"
								? "capacity-node"
								: node.in_crossover_set
									? "crossover-set"
									: "auxiliary-original"
						}
						position={position}
						shape={
							node.kind === "capacity"
								? "capacity"
								: node.in_crossover_set
									? "ring"
									: "dot"
						}
						title={`${node.auxiliary_id}: ${node.kind}, potential ${node.potential}, component ${node.component}`}
					/>,
				];
			})}
			{forestSubsetBadge !== undefined && (
				<AdvancedAnchoredBadge
					contribution="primal_dual_ipm_mcf_overlay"
					entityKind={forestSubsetBadge.entityKind}
					entityId={forestSubsetBadge.entityId}
					originalEdgeId={forestSubsetBadge.originalEdgeId}
					originalNodeId={forestSubsetBadge.originalNodeId}
					visualRole="forest-subset-enumeration"
					position={forestSubsetBadge.position}
					label={`FOREST #${forestSubsetBadge.serial} · ${forestSubsetBadge.count === 0 ? "∅" : `${forestSubsetBadge.count} ARC${forestSubsetBadge.count === 1 ? "" : "S"}`}`}
					title={`Exact forest subset ${forestSubsetBadge.serial}: ${forestSubsetBadge.count} candidate auxiliary arcs`}
					tone="teal"
				/>
			)}
			{kernelStageBadge !== undefined && (
				<AdvancedAnchoredBadge
					contribution="primal_dual_ipm_mcf_overlay"
					entityKind={kernelStageBadge.entityKind}
					entityId={kernelStageBadge.entityId}
					originalEdgeId={kernelStageBadge.originalEdgeId}
					originalNodeId={kernelStageBadge.originalNodeId}
					visualRole={kernelStageBadge.role}
					position={kernelStageBadge.position}
					label={kernelStageBadge.label}
					title={kernelStageBadge.title}
					tone={kernelStageBadge.tone}
				/>
			)}
		</g>
	);
}

function RandomizedMcfLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.randomizedAlmostLinearMcf;
	const idScope = useFlowGraphIdScope();
	if (overlay === undefined) return null;
	type EdgeState = (typeof overlay.edges)[number];
	const graphEdges = new Map(state.plan.edges.map((edge) => [edge.id, edge]));
	const routeFor = (edge: EdgeState) => {
		const route = state.layout.routes.get(edge.edge_id);
		if (route === undefined) {
			throw new Error(
				`randomized almost-linear MCF geometry is missing for ${edge.edge_id}`,
			);
		}
		return route;
	};
	const graphEdgeFor = (edge: EdgeState) => {
		const graphEdge = graphEdges.get(edge.edge_id);
		if (graphEdge === undefined) {
			throw new Error(
				`randomized almost-linear MCF graph edge is missing for ${edge.edge_id}`,
			);
		}
		return graphEdge;
	};
	const firstEdge = requiredAdvancedValue(
		overlay.edges[0],
		"randomized MCF edge state",
	);
	const pickEdge = (predicate: (edge: EdgeState) => boolean): EdgeState =>
		overlay.edges.find(predicate) ?? firstEdge;
	const maximumIsolationDraw = overlay.edges.reduce((maximum, edge) => {
		const value = BigInt(edge.isolation_draw);
		return value > maximum ? value : maximum;
	}, 0n);
	const maximumLength = overlay.edges.reduce(
		(maximum, edge) => Math.max(maximum, Math.abs(Number(edge.length))),
		0,
	);
	const isolationWidth = (edge: EdgeState): number => {
		if (maximumIsolationDraw === 0n) return 1.5;
		return (
			2 +
			Number((BigInt(edge.isolation_draw) * 4_000n) / maximumIsolationDraw) /
				1_000
		);
	};
	const gradientWidth = (edge: EdgeState): number =>
		maximumLength === 0
			? 1.5
			: 2 + (4 * Math.abs(Number(edge.length))) / maximumLength;
	const displayedFlowWidth = (edge: EdgeState): number => {
		const capacity = graphEdgeFor(edge).capacity;
		switch (overlay.stage) {
			case "sample-isolation-costs":
				return isolationWidth(edge);
			case "select-isolated-optimum":
				return decimalFlowStrokeWidth(
					requiredAdvancedValue(
						edge.isolated_optimum_flow,
						`${edge.edge_id} isolated optimum flow`,
					),
					capacity,
				);
			case "refresh-gradient-length":
				return gradientWidth(edge);
			case "inspect-oracle-vector":
				return edge.candidate_sign === "0" ? 1.5 : 4.5;
			case "build-forest-pool":
			case "sample-tree-chain":
				return edge.tree_edge ? 4 : 1.5;
			case "query-minimum-ratio-cycle":
				return edge.selected_sign === "0" ? gradientWidth(edge) : 6;
			case "detect-changed-coordinates":
			case "rebuild-tree-chain":
				return edge.detected ? 4.5 : gradientWidth(edge);
			case "construct-final-point":
				return rationalFlowStrokeWidth(
					requiredAdvancedValue(
						edge.final_point_flow,
						`${edge.edge_id} final-point flow`,
					),
					capacity,
				);
			case "round-nearest-integer":
			case "check-certificate":
			case "optimal":
				return decimalFlowStrokeWidth(
					requiredAdvancedValue(
						edge.final_flow,
						`${edge.edge_id} rounded flow`,
					),
					capacity,
				);
			case "ready":
				return 1.5;
			case "inspect-feasible-assignment":
			case "enumerate-feasible-set":
			case "initialize-relative-interior":
			case "potential-reduction-step":
				return decimalFlowStrokeWidth(edge.current_flow, capacity);
		}
	};
	const visualRole = (edge: EdgeState): string => {
		switch (overlay.stage) {
			case "ready":
				return "isolated-coordinate";
			case "inspect-feasible-assignment":
				return Number(edge.current_flow) === 0
					? "feasible-assignment-zero"
					: "feasible-assignment-flow";
			case "enumerate-feasible-set":
				return edge.fixed_on_face
					? "fixed-coordinate"
					: "feasible-face-coordinate";
			case "sample-isolation-costs":
				return "isolation-draw-coordinate";
			case "select-isolated-optimum":
				return edge.isolated_optimum_flow === "0"
					? "isolated-optimum-zero"
					: "isolated-optimum-flow";
			case "initialize-relative-interior":
				return "relative-interior-flow";
			case "inspect-oracle-vector":
				return edge.candidate_sign === "0"
					? "oracle-vector-zero"
					: "candidate-cycle";
			case "build-forest-pool":
				return edge.tree_edge ? "sampled-tree" : "forest-pool-context";
			case "sample-tree-chain":
				return edge.tree_edge ? "sampled-tree-active" : "tree-chain-context";
			case "refresh-gradient-length":
				return Number(edge.gradient) < 0
					? "gradient-negative"
					: Number(edge.gradient) > 0
						? "gradient-positive"
						: "gradient-zero";
			case "query-minimum-ratio-cycle":
				return edge.selected_sign === "0"
					? "gradient-context"
					: "selected-cycle";
			case "potential-reduction-step":
				return edge.selected_sign === "0"
					? "potential-flow-context"
					: "potential-step-cycle";
			case "detect-changed-coordinates":
				return edge.detected ? "detected-coordinate" : "gradient-context";
			case "rebuild-tree-chain":
				return edge.detected
					? "rebuilt-coordinate"
					: edge.tree_edge
						? "sampled-tree-active"
						: "tree-chain-context";
			case "construct-final-point":
				return "final-point-flow";
			case "round-nearest-integer":
				return "rounded-flow";
			case "check-certificate":
				return "certificate-flow-check";
			case "optimal":
				return "certificate-flow-verified";
		}
	};
	type Badge = Readonly<{
		edge: EdgeState;
		label: string;
		role: string;
		title: string;
		tone: "amber" | "teal";
	}>;
	let badge: Badge | undefined;
	const selectedCycleNodesVisible =
		overlay.stage === "query-minimum-ratio-cycle" ||
		overlay.stage === "potential-reduction-step";
	switch (overlay.stage) {
		case "ready":
			break;
		case "inspect-feasible-assignment": {
			const cursor = requiredAdvancedValue(
				overlay.assignment_cursor,
				"randomized MCF assignment cursor",
			);
			const edge = requiredAdvancedValue(
				overlay.edges.find((candidate) => candidate.edge_id === cursor),
				`randomized MCF assignment cursor edge ${cursor}`,
			);
			const serial = requiredAdvancedValue(
				overlay.assignment_serial,
				"randomized MCF assignment serial",
			);
			badge = {
				edge,
				label: `ASSIGN #${serial} · f=${compactFiniteScalar(edge.current_flow)}`,
				role: "feasible-assignment-checkpoint",
				title: `Exact feasible-assignment checkpoint ${serial}; cursor ${edge.edge_id}; coordinate ${edge.current_flow}/${graphEdgeFor(edge).capacity}`,
				tone: "amber",
			};
			break;
		}
		case "enumerate-feasible-set": {
			const edge = pickEdge((candidate) => !candidate.fixed_on_face);
			badge = {
				edge,
				label: `FACE · ${overlay.feasible_flows} FLOWS`,
				role: "feasible-face-barycenter",
				title: `Exact feasible face contains ${overlay.feasible_flows} integer flows; ${edge.edge_id} barycenter coordinate ${edge.current_flow}`,
				tone: "teal",
			};
			break;
		}
		case "sample-isolation-costs": {
			const edge = overlay.edges.reduce((best, candidate) =>
				BigInt(candidate.isolation_draw) > BigInt(best.isolation_draw)
					? candidate
					: best,
			);
			badge = {
				edge,
				label: `z=${edge.isolation_draw} · TRY #${overlay.isolation_attempt}`,
				role: "isolation-draw-witness",
				title: `${edge.edge_id} received isolation draw ${edge.isolation_draw}; isolated unit cost ${edge.isolated_cost}; attempt ${overlay.isolation_attempt}`,
				tone: "amber",
			};
			break;
		}
		case "select-isolated-optimum": {
			const edge = pickEdge(
				(candidate) => candidate.isolated_optimum_flow !== "0",
			);
			const flow = requiredAdvancedValue(
				edge.isolated_optimum_flow,
				`${edge.edge_id} isolated optimum flow`,
			);
			badge = {
				edge,
				label: `ISO f=${flow} · C*=${compactExactInteger(overlay.isolated_optimum_cost)}`,
				role: "isolated-optimum-witness",
				title: `Unique isolated optimum; ${edge.edge_id} carries ${flow}; exact perturbed objective ${overlay.isolated_optimum_cost}`,
				tone: "teal",
			};
			break;
		}
		case "initialize-relative-interior": {
			const edge = pickEdge((candidate) => !candidate.fixed_on_face);
			badge = {
				edge,
				label: `INTERIOR · f=${compactFiniteScalar(edge.current_flow)}`,
				role: "relative-interior-witness",
				title: `Feasible-face barycenter initializes the relative interior; ${edge.edge_id} flow ${edge.current_flow}; objective ${overlay.initial_cost}`,
				tone: "teal",
			};
			break;
		}
		case "inspect-oracle-vector": {
			const edge = pickEdge((candidate) => candidate.candidate_sign !== "0");
			const serial = requiredAdvancedValue(
				overlay.oracle_vector_serial,
				"randomized MCF oracle-vector serial",
			);
			const active = overlay.edges.filter(
				(candidate) => candidate.candidate_sign !== "0",
			).length;
			badge = {
				edge,
				label: `ORACLE #${serial} · ${active} ACTIVE`,
				role: "oracle-vector-checkpoint",
				title: `Exact signed-vector checkpoint ${serial}; ${active} nonzero coordinates; ${edge.edge_id} sign ${edge.candidate_sign}`,
				tone: "amber",
			};
			break;
		}
		case "build-forest-pool": {
			const edge = pickEdge((candidate) => candidate.tree_edge);
			const treeEdges = overlay.edges.filter(
				(candidate) => candidate.tree_edge,
			).length;
			badge = {
				edge,
				label: `POOL ${overlay.forest_pool_size} · ${treeEdges} TREE`,
				role: "forest-pool-witness",
				title: `Bounded forest pool size ${overlay.forest_pool_size}; published representative contains ${treeEdges} tree edges; anchor ${edge.edge_id}`,
				tone: "teal",
			};
			break;
		}
		case "sample-tree-chain": {
			const edge = pickEdge((candidate) => candidate.tree_edge);
			const sample = requiredAdvancedValue(
				overlay.sampled_forest_index,
				"randomized MCF sampled forest index",
			);
			badge = {
				edge,
				label: `TREE ${sample} / ${overlay.forest_pool_size}`,
				role: "sampled-tree-chain-witness",
				title: `Seeded tree-chain sample index ${sample} from pool ${overlay.forest_pool_size}; member edge ${edge.edge_id}`,
				tone: "teal",
			};
			break;
		}
		case "refresh-gradient-length": {
			const edge = overlay.edges.reduce((best, candidate) =>
				Math.abs(Number(candidate.length)) > Math.abs(Number(best.length))
					? candidate
					: best,
			);
			badge = {
				edge,
				label: `g=${compactFiniteScalar(edge.gradient)} · ℓ=${compactFiniteScalar(edge.length)}`,
				role: "gradient-length-witness",
				title: `${edge.edge_id} refreshed source gradient ${edge.gradient} and length ${edge.length}; alpha ${overlay.alpha}`,
				tone: "amber",
			};
			break;
		}
		case "query-minimum-ratio-cycle": {
			const edge = pickEdge((candidate) => candidate.selected_sign !== "0");
			badge = {
				edge,
				label:
					overlay.minimum_ratio === undefined
						? "RATIO · STATIONARY"
						: `RATIO ${compactFiniteScalar(overlay.minimum_ratio)}`,
				role: "minimum-ratio-query-witness",
				title: `${edge.edge_id} selected sign ${edge.selected_sign}; exact published ratio ${overlay.minimum_ratio ?? "stationary"}`,
				tone: "amber",
			};
			break;
		}
		case "potential-reduction-step": {
			const edge = pickEdge((candidate) => candidate.selected_sign !== "0");
			badge = {
				edge,
				label: `η=${compactFiniteScalar(overlay.eta)} · Φ=${compactFiniteScalar(overlay.potential)}`,
				role: "potential-reduction-witness",
				title: `Source potential reduction uses eta ${overlay.eta}; resulting potential ${overlay.potential}; ${edge.edge_id} flow ${edge.current_flow}`,
				tone: "amber",
			};
			break;
		}
		case "detect-changed-coordinates": {
			const edge = pickEdge((candidate) => candidate.detected);
			badge = {
				edge,
				label: `DETECT · ${overlay.detected_coordinates} COORD`,
				role: "detect-witness",
				title: `Lazy Detect refreshed ${overlay.detected_coordinates} coordinates; anchor ${edge.edge_id}; current ${edge.current_flow}, stale ${edge.stale_flow}`,
				tone: "amber",
			};
			break;
		}
		case "rebuild-tree-chain": {
			const edge = pickEdge(
				(candidate) => candidate.detected || candidate.tree_edge,
			);
			badge = {
				edge,
				label: `REBUILD #${overlay.rebuilds}`,
				role: "tree-chain-rebuild-witness",
				title: `Tree-chain rebuild ${overlay.rebuilds}; anchored to ${edge.edge_id}${edge.detected ? " refreshed by Detect" : " in the sampled tree"}`,
				tone: "teal",
			};
			break;
		}
		case "construct-final-point": {
			const edge = pickEdge(
				(candidate) => candidate.final_point_flow?.numerator !== "0",
			);
			const point = requiredAdvancedValue(
				edge.final_point_flow,
				`${edge.edge_id} final-point flow`,
			);
			badge = {
				edge,
				label: `x̂=${compactRational(point)} · GAP≤τ`,
				role: "final-point-witness",
				title: `${edge.edge_id} exact final-point coordinate ${point.numerator}/${point.denominator}; gap ${compactRational(requiredAdvancedValue(overlay.final_point_gap, "randomized MCF final-point gap"))}; threshold ${compactRational(overlay.final_point_threshold)}`,
				tone: "teal",
			};
			break;
		}
		case "round-nearest-integer": {
			const edge = pickEdge((candidate) => candidate.final_flow !== "0");
			const point = requiredAdvancedValue(
				edge.final_point_flow,
				`${edge.edge_id} final-point flow`,
			);
			const rounded = requiredAdvancedValue(
				edge.final_flow,
				`${edge.edge_id} rounded flow`,
			);
			badge = {
				edge,
				label: `ROUND ${compactRational(point)} → ${rounded}`,
				role: "nearest-integer-witness",
				title: `${edge.edge_id} nearest-integer recovery rounds exact coordinate ${point.numerator}/${point.denominator} to ${rounded}`,
				tone: "teal",
			};
			break;
		}
		case "check-certificate": {
			const edge = pickEdge((candidate) => candidate.final_flow !== "0");
			badge = {
				edge,
				label: `CHECK · c·f=${compactExactInteger(overlay.optimum_cost)}`,
				role: "certificate-check-witness",
				title: `Independent min-cost-flow certificate checks the rounded flow; optimum original cost ${overlay.optimum_cost}; anchored to ${edge.edge_id}`,
				tone: "amber",
			};
			break;
		}
		case "optimal": {
			const edge = pickEdge((candidate) => candidate.final_flow !== "0");
			badge = {
				edge,
				label: `CERTIFIED · c·f=${compactExactInteger(overlay.optimum_cost)}`,
				role: "certificate-verified-witness",
				title: `Certified original-cost optimum ${overlay.optimum_cost}; ${edge.edge_id} final flow ${edge.final_flow}`,
				tone: "teal",
			};
			break;
		}
	}
	return (
		<g
			className="flow-advanced-algorithm-layer"
			data-advanced-overlay="randomized-almost-linear-mcf"
			data-advanced-stage={overlay.stage}
		>
			{overlay.edges.map((edge) => {
				const route = routeFor(edge);
				const selected = edge.selected_sign !== "0";
				const candidate = edge.candidate_sign !== "0";
				const reverse =
					overlay.stage === "potential-reduction-step" && selected
						? edge.selected_sign === "-1"
						: overlay.stage === "query-minimum-ratio-cycle" && selected
							? edge.selected_sign === "-1"
							: overlay.stage === "inspect-oracle-vector" && candidate
								? edge.candidate_sign === "-1"
								: false;
				return (
					<AdvancedPath
						key={edge.edge_id}
						contribution="randomized_almost_linear_mcf_overlay"
						entityKind="edge"
						entityId={edge.edge_id}
						visualRole={visualRole(edge)}
						path={reverse ? route.reversePath : route.path}
						width={displayedFlowWidth(edge)}
						markerEnd={
							reverse ||
							(overlay.stage === "query-minimum-ratio-cycle" && selected) ||
							(overlay.stage === "potential-reduction-step" && selected) ||
							(overlay.stage === "inspect-oracle-vector" && candidate)
								? flowScopedSvgUrl(
										idScope,
										selected
											? "flow-arrow-advanced-active"
											: "flow-arrow-advanced-candidate",
									)
								: undefined
						}
						title={`${edge.edge_id}: current ${edge.current_flow}, isolated optimum ${edge.isolated_optimum_flow ?? "not selected"}, stale ${edge.stale_flow}, isolation draw ${edge.isolation_draw}, gradient ${edge.gradient}, length ${edge.length}, selected ${edge.selected_sign}, final point ${edge.final_point_flow === undefined ? "not built" : `${edge.final_point_flow.numerator}/${edge.final_point_flow.denominator}`}, rounded ${edge.final_flow ?? "not rounded"}`}
					/>
				);
			})}
			{overlay.nodes.flatMap((node) => {
				if (!node.on_selected_cycle || !selectedCycleNodesVisible) return [];
				const position = state.positions.get(node.node_id);
				if (position === undefined) return [];
				return [
					<AdvancedNodeMark
						key={node.node_id}
						contribution="randomized_almost_linear_mcf_overlay"
						entityId={node.node_id}
						visualRole="selected-cycle"
						position={position}
						shape="ring"
						title={`${node.node_id}: selected ratio cycle, tree depth ${node.depth}`}
					/>,
				];
			})}
			{badge !== undefined && (
				<AdvancedAnchoredBadge
					contribution="randomized_almost_linear_mcf_overlay"
					entityKind="edge"
					entityId={badge.edge.edge_id}
					visualRole={badge.role}
					position={routeFor(badge.edge).routeMidpoint}
					label={badge.label}
					title={badge.title}
					tone={badge.tone}
				/>
			)}
		</g>
	);
}

function WeightedAugmentingPathsLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.weightedAugmentingPaths;
	const idScope = useFlowGraphIdScope();
	if (overlay === undefined) return null;
	const weightsAssigned = !(
		overlay.stage === "ready" ||
		overlay.stage === "begin-capacity-phase" ||
		overlay.stage === "build-hierarchy" ||
		overlay.stage === "certify-expansion"
	);
	let phaseCertificate:
		| Readonly<{
				bit: string;
				check: string;
				phase: string;
				phaseCount: string;
				sink: Point;
				sinkId: string;
				source: Point;
				sourceId: string;
		  }>
		| undefined;
	if (overlay.stage === "finish-capacity-phase") {
		const model = state.plan.context.model;
		if (model.kind !== "max-flow") {
			throw new Error(
				"weighted capacity certificate requires max-flow terminals",
			);
		}
		const source = state.positions.get(model.source);
		const sink = state.positions.get(model.sink);
		const sourceState = overlay.nodes.find(
			(node) => node.node_id === model.source,
		);
		const sinkState = overlay.nodes.find((node) => node.node_id === model.sink);
		if (
			source === undefined ||
			sink === undefined ||
			sourceState?.source_side !== true ||
			sinkState?.source_side !== false
		) {
			throw new Error(
				"weighted capacity certificate has no separating residual cut",
			);
		}
		const check = state.plan.context.metrics[10];
		canonicalUnsignedInteger(check, "weighted residual-cut check counter");
		canonicalUnsignedInteger(overlay.phase, "weighted capacity phase");
		canonicalUnsignedInteger(
			overlay.phase_count,
			"weighted capacity phase count",
		);
		canonicalUnsignedInteger(overlay.capacity_bit, "weighted capacity bit");
		phaseCertificate = {
			bit: overlay.capacity_bit,
			check,
			phase: overlay.phase,
			phaseCount: overlay.phase_count,
			sink,
			sinkId: model.sink,
			source,
			sourceId: model.source,
		};
	}
	const maximumLabel = overlay.nodes.reduce(
		(maximum, node) => Math.max(maximum, boundedInteger(node.label, 10_000)),
		1,
	);
	return (
		<g
			className="flow-advanced-algorithm-layer"
			data-advanced-overlay="weighted-augmenting-paths"
			data-advanced-stage={overlay.stage}
		>
			{overlay.residual_arcs.flatMap((arc) => {
				const route = state.layout.routes.get(arc.edge_id);
				if (route === undefined) return [];
				if (
					BigInt(arc.capacity) === 0n &&
					arc.hierarchy_kind === undefined &&
					!arc.admissible &&
					!arc.active
				)
					return [];
				const role = arc.active
					? "active-path"
					: arc.admissible
						? "admissible-arc"
						: arc.hierarchy_kind === "expanding"
							? "expanding-arc"
							: arc.hierarchy_kind === "dag"
								? "hierarchy-dag"
								: "weighted-residual";
				const assignedWeight = canonicalUnsignedInteger(
					arc.weight,
					`weighted residual ${arc.edge_id}:${arc.direction} weight`,
				);
				const visibleWeight = Number(
					assignedWeight > 255n ? 255n : assignedWeight,
				);
				const weightedWidth =
					1.5 + Math.min(2.6, Math.log2(visibleWeight + 1) * 0.62);
				return [
					<AdvancedPath
						key={`${arc.edge_id}:${arc.direction}`}
						contribution="weighted_augmenting_paths_overlay"
						entityKind="residual-arc"
						entityId={arc.edge_id}
						residualDirection={arc.direction}
						visualRole={role}
						path={arc.direction === "forward" ? route.path : route.reversePath}
						width={
							arc.active
								? 6
								: arc.admissible
									? 4
									: weightsAssigned
										? weightedWidth
										: 1.5
						}
						markerEnd={
							arc.active || arc.admissible
								? flowScopedSvgUrl(
										idScope,
										arc.active
											? "flow-arrow-advanced-active"
											: "flow-arrow-advanced-candidate",
									)
								: undefined
						}
						title={`${arc.edge_id} ${arc.direction}: residual ${arc.capacity}, weight ${arc.weight}, ${role}`}
					/>,
				];
			})}
			{overlay.nodes.flatMap((node) => {
				const position = state.positions.get(node.node_id);
				if (position === undefined) return [];
				const role = !node.alive
					? "dead-node"
					: node.expansion_witness_side
						? "expansion-witness"
						: node.source_side
							? "source-side"
							: "weighted-label";
				const label = boundedInteger(node.label, 10_000);
				const height = 4 + (label / maximumLabel) * 13;
				return [
					<line
						key={node.node_id}
						className="flow-advanced-algorithm-node-meter"
						data-overlay-contribution="weighted_augmenting_paths_overlay"
						data-overlay-feature-bundle={FEATURE_BUNDLE}
						data-overlay-entity-kind="node"
						data-overlay-entity-id={node.node_id}
						data-overlay-role={role}
						x1={position.x + FLOW_NODE_RADIUS * 0.72}
						y1={position.y + FLOW_NODE_RADIUS * 0.72}
						x2={position.x + FLOW_NODE_RADIUS * 0.72}
						y2={position.y + FLOW_NODE_RADIUS * 0.72 - height}
					>
						<title>{`${node.node_id}: weighted label ${node.label}, order ${node.order}, component ${node.component}, ${role}`}</title>
					</line>,
				];
			})}
			{phaseCertificate !== undefined && (
				<WeightedCapacityPhaseCertificate certificate={phaseCertificate} />
			)}
		</g>
	);
}

function WeightedCapacityPhaseCertificate({
	certificate,
}: Readonly<{
	certificate: Readonly<{
		bit: string;
		check: string;
		phase: string;
		phaseCount: string;
		sink: Point;
		sinkId: string;
		source: Point;
		sourceId: string;
	}>;
}>) {
	const dx = certificate.sink.x - certificate.source.x;
	const dy = certificate.sink.y - certificate.source.y;
	const length = Math.max(1, Math.hypot(dx, dy));
	const ux = dx / length;
	const uy = dy / length;
	const midpoint = {
		x: (certificate.source.x + certificate.sink.x) / 2,
		y: (certificate.source.y + certificate.sink.y) / 2,
	};
	const sourceStart = {
		x: certificate.source.x + ux * (FLOW_NODE_RADIUS + 7),
		y: certificate.source.y + uy * (FLOW_NODE_RADIUS + 7),
	};
	const sinkStart = {
		x: certificate.sink.x - ux * (FLOW_NODE_RADIUS + 7),
		y: certificate.sink.y - uy * (FLOW_NODE_RADIUS + 7),
	};
	const gap = 16;
	return (
		<g
			className="flow-weighted-capacity-certificate"
			data-weighted-capacity-check={certificate.check}
		>
			<line
				className="flow-weighted-capacity-certificate-half"
				data-overlay-contribution="weighted_augmenting_paths_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind="node"
				data-overlay-entity-id={certificate.sourceId}
				data-overlay-role="residual-cut-source-half"
				x1={sourceStart.x}
				y1={sourceStart.y}
				x2={midpoint.x - ux * gap}
				y2={midpoint.y - uy * gap}
			/>
			<line
				className="flow-weighted-capacity-certificate-half"
				data-overlay-contribution="weighted_augmenting_paths_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind="node"
				data-overlay-entity-id={certificate.sinkId}
				data-overlay-role="residual-cut-sink-half"
				x1={sinkStart.x}
				y1={sinkStart.y}
				x2={midpoint.x + ux * gap}
				y2={midpoint.y + uy * gap}
			/>
			<path
				className="flow-weighted-capacity-certificate-break"
				data-overlay-contribution="weighted_augmenting_paths_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind="node"
				data-overlay-entity-id={certificate.sourceId}
				data-overlay-role="residual-cut-separator"
				d={`M ${midpoint.x - 7} ${midpoint.y - 7} L ${midpoint.x + 7} ${midpoint.y + 7} M ${midpoint.x - 7} ${midpoint.y + 7} L ${midpoint.x + 7} ${midpoint.y - 7}`}
			/>
			<text
				className="flow-weighted-capacity-certificate-label"
				data-overlay-contribution="weighted_augmenting_paths_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind="node"
				data-overlay-entity-id={certificate.sourceId}
				data-overlay-role="residual-cut-check"
				x={midpoint.x}
				y={midpoint.y - 18}
				textAnchor="middle"
			>
				{`NO s→t PATH · PHASE ${BigInt(certificate.phase) + 1n}/${certificate.phaseCount} · BIT ${certificate.bit} · CUT #${certificate.check}`}
			</text>
		</g>
	);
}

function weightedPushRelabelPositions(
	state: FlowEntityGraphState,
): ReadonlyMap<string, Point> {
	const overlay = state.renderData.overlayViews.weightedPushRelabelShortcut;
	const result = new Map(state.positions);
	if (overlay === undefined) return result;
	for (const root of overlay.nodes.filter((node) => !node.original)) {
		const members = overlay.nodes
			.filter((node) => node.original && node.component === root.component)
			.flatMap((node) => {
				const position = state.positions.get(node.node_id);
				return position === undefined ? [] : [position];
			});
		const center =
			members.length === 0
				? { x: 450, y: 270 }
				: {
						x:
							members.reduce((total, point) => total + point.x, 0) /
							members.length,
						y:
							members.reduce((total, point) => total + point.y, 0) /
							members.length,
					};
		const direction = boundedInteger(root.component, 2) === 0 ? -1 : 1;
		result.set(root.node_id, {
			x: Math.max(34, Math.min(866, center.x)),
			y: Math.max(36, Math.min(504, center.y + direction * 68)),
		});
	}
	return result;
}

function WeightedPushRelabelLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.weightedPushRelabelShortcut;
	const idScope = useFlowGraphIdScope();
	if (overlay === undefined) return null;
	const positions = weightedPushRelabelPositions(state);
	const edgeById = new Map(overlay.edges.map((edge) => [edge.edge_id, edge]));
	const nodeById = new Map(overlay.nodes.map((node) => [node.node_id, node]));
	const inspected = new Set(
		overlay.inspected_arcs.map((arc) => `${arc.edge_id}:${arc.direction}`),
	);
	const activePath = new Set(
		overlay.active_path.map((arc) => `${arc.edge_id}:${arc.direction}`),
	);
	const relabelNodes = new Set(overlay.active_relabel_nodes);
	const exactCompletion =
		overlay.stage.startsWith("completion-") ||
		overlay.stage === "complete-residual-rounds" ||
		overlay.stage === "check-certificate" ||
		overlay.stage === "optimal";
	const shortcutGraphVisible =
		overlay.stage !== "ready" &&
		overlay.stage !== "build-weak-hierarchy" &&
		!exactCompletion;
	const weightsAssigned = !(
		overlay.stage === "ready" ||
		overlay.stage === "build-weak-hierarchy" ||
		overlay.stage === "build-shortcut-graph"
	);
	const inspectionCount = canonicalUnsignedInteger(
		state.plan.context.metrics[3],
		"weighted push-relabel primitive-arc inspection counter",
	);
	const inspectionRadius = 3.25 + Math.log2(Number(inspectionCount) + 1) * 0.45;
	const terminalDemand =
		overlay.stage === "initialize-demand" &&
		state.plan.context.model.kind === "max-flow"
			? {
					demand: overlay.demand,
					sink: state.plan.context.model.sink,
					source: state.plan.context.model.source,
				}
			: undefined;
	const shortFlowMeasure =
		overlay.stage === "measure-short-flow" &&
		state.plan.context.model.kind === "max-flow"
			? {
					demand: overlay.demand,
					routed: overlay.routed,
					sinkId: state.plan.context.model.sink,
					sourceId: state.plan.context.model.source,
					weightedLength: overlay.weighted_length,
					weightedUnits: overlay.weighted_length_units,
				}
			: undefined;
	const certificateRole =
		overlay.stage === "complete-residual-rounds"
			? "certificate-cut-candidate"
			: overlay.stage === "check-certificate"
				? "certificate-cut-check"
				: overlay.stage === "optimal"
					? "certificate-cut-verified"
					: undefined;
	return (
		<g
			className="flow-advanced-algorithm-layer"
			data-advanced-overlay="weighted-push-relabel"
			data-advanced-stage={overlay.stage}
			data-weighted-native-node-ids={overlay.nodes
				.map((node) => node.node_id)
				.join("|")}
			data-weighted-native-edge-ids={overlay.edges
				.map((edge) => edge.edge_id)
				.join("|")}
		>
			{overlay.residual_arcs.flatMap((arc) => {
				const edge = edgeById.get(arc.edge_id);
				const key = `${arc.edge_id}:${arc.direction}`;
				const isInspected = inspected.has(key);
				const isActivePath = activePath.has(key);
				if (edge?.kind === "shortcut" && !shortcutGraphVisible) return [];
				if (
					BigInt(arc.capacity) === 0n &&
					!isActivePath &&
					!arc.admissible &&
					!isInspected &&
					edge?.kind !== "shortcut"
				)
					return [];
				const originalRoute = state.layout.routes.get(arc.edge_id);
				const from = positions.get(arc.from);
				const to = positions.get(arc.to);
				const path =
					originalRoute !== undefined
						? arc.direction === "forward"
							? originalRoute.path
							: originalRoute.reversePath
						: from !== undefined && to !== undefined
							? directedLinePath(from, to, 13)
							: undefined;
				if (path === undefined) return [];
				const role = isInspected
					? "inspected-arc"
					: isActivePath
						? "active-path"
						: arc.admissible
							? "admissible-arc"
							: edge?.kind === "shortcut"
								? "shortcut-arc"
								: "weighted-residual";
				const weight = canonicalUnsignedInteger(
					arc.weight,
					`weighted push-relabel ${key} weight`,
				);
				const visibleWeight = Number(weight > 255n ? 255n : weight);
				const weightedWidth =
					1.5 + Math.min(2.6, Math.log2(visibleWeight + 1) * 0.62);
				const leaves: ReactNode[] = [
					<AdvancedPath
						key={key}
						contribution="weighted_push_relabel_shortcut_overlay"
						entityKind={
							edge?.kind === "shortcut"
								? "auxiliary-residual-arc"
								: "residual-arc"
						}
						entityId={arc.edge_id}
						residualDirection={arc.direction}
						visualRole={role}
						path={path}
						width={
							isInspected
								? 5
								: isActivePath
									? 6
									: arc.admissible
										? 4
										: weightsAssigned
											? weightedWidth
											: 1.5
						}
						markerEnd={
							isActivePath || isInspected || arc.admissible
								? flowScopedSvgUrl(
										idScope,
										isActivePath || isInspected
											? "flow-arrow-advanced-active"
											: "flow-arrow-advanced-candidate",
									)
								: undefined
						}
						title={`${key}: residual ${arc.capacity}, weight ${arc.weight}, ${role}`}
					/>,
				];
				if (isInspected) {
					const midpoint =
						originalRoute?.routeMidpoint ??
						(from !== undefined && to !== undefined
							? { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 }
							: undefined);
					if (
						midpoint === undefined ||
						from === undefined ||
						to === undefined
					) {
						throw new Error(
							`weighted inspection geometry is missing for ${key}`,
						);
					}
					const distance = Math.max(
						1,
						Math.hypot(to.x - from.x, to.y - from.y),
					);
					const side = arc.direction === "forward" ? 1 : -1;
					const normal = {
						x: (-(to.y - from.y) / distance) * side,
						y: ((to.x - from.x) / distance) * side,
					};
					const center = {
						x: midpoint.x + normal.x * 8,
						y: midpoint.y + normal.y * 8,
					};
					leaves.push(
						<circle
							key={`${key}:inspection-progress`}
							className="flow-weighted-inspection-progress"
							data-overlay-contribution="weighted_push_relabel_shortcut_overlay"
							data-overlay-feature-bundle={FEATURE_BUNDLE}
							data-overlay-entity-kind={
								edge?.kind === "shortcut"
									? "auxiliary-residual-arc"
									: "residual-arc"
							}
							data-overlay-entity-id={arc.edge_id}
							data-overlay-residual-direction={arc.direction}
							data-overlay-role="inspection-progress"
							cx={center.x}
							cy={center.y}
							r={inspectionRadius}
						/>,
						<text
							key={`${key}:inspection-count`}
							className="flow-weighted-local-label"
							data-overlay-contribution="weighted_push_relabel_shortcut_overlay"
							data-overlay-feature-bundle={FEATURE_BUNDLE}
							data-overlay-entity-kind={
								edge?.kind === "shortcut"
									? "auxiliary-residual-arc"
									: "residual-arc"
							}
							data-overlay-entity-id={arc.edge_id}
							data-overlay-residual-direction={arc.direction}
							data-overlay-role="inspection-count"
							x={center.x + normal.x * (inspectionRadius + 5)}
							y={center.y + normal.y * (inspectionRadius + 5) + 3}
							textAnchor="middle"
						>
							{`i${inspectionCount}`}
						</text>,
					);
				}
				return leaves;
			})}
			{overlay.nodes.flatMap((node) => {
				const position = positions.get(node.node_id);
				if (position === undefined) return [];
				if (!node.original) {
					if (!shortcutGraphVisible) return [];
					return [
						<AdvancedNodeMark
							key={node.node_id}
							contribution="weighted_push_relabel_shortcut_overlay"
							entityKind="auxiliary-node"
							entityId={node.node_id}
							visualRole="steiner-root"
							position={position}
							shape="steiner"
							title={`${node.node_id}: Steiner root for component ${node.component}`}
						/>,
					];
				}
				if (!relabelNodes.has(node.node_id)) return [];
				const label = canonicalUnsignedInteger(
					node.label,
					`weighted relabel label for ${node.node_id}`,
				);
				const meterLength = 8 + Math.log2(Number(label) + 1) * 1.35;
				return [
					<AdvancedNodeMark
						key={node.node_id}
						contribution="weighted_push_relabel_shortcut_overlay"
						entityId={node.node_id}
						visualRole="active-relabel"
						position={position}
						shape="ring"
						title={`${node.node_id}: relabel to weighted height ${node.label}`}
					/>,
					<line
						key={`${node.node_id}:label-meter`}
						className="flow-weighted-relabel-meter"
						data-overlay-contribution="weighted_push_relabel_shortcut_overlay"
						data-overlay-feature-bundle={FEATURE_BUNDLE}
						data-overlay-entity-kind="node"
						data-overlay-entity-id={node.node_id}
						data-overlay-role="active-relabel-level"
						x1={position.x + FLOW_NODE_RADIUS + 7}
						y1={position.y}
						x2={position.x + FLOW_NODE_RADIUS + 7 + meterLength}
						y2={position.y}
					/>,
					<text
						key={`${node.node_id}:label-value`}
						className="flow-weighted-local-label"
						data-overlay-contribution="weighted_push_relabel_shortcut_overlay"
						data-overlay-feature-bundle={FEATURE_BUNDLE}
						data-overlay-entity-kind="node"
						data-overlay-entity-id={node.node_id}
						data-overlay-role="active-relabel-value"
						x={position.x + FLOW_NODE_RADIUS + 10 + meterLength}
						y={position.y + 3}
					>
						{`ℓ${node.label}`}
					</text>,
				];
			})}
			{terminalDemand !== undefined &&
				[terminalDemand.source, terminalDemand.sink].map((nodeId, index) => {
					const position = positions.get(nodeId);
					if (position === undefined) {
						throw new Error(`weighted demand terminal ${nodeId} is missing`);
					}
					const source = index === 0;
					return (
						<g key={`demand:${nodeId}`}>
							<AdvancedNodeMark
								contribution="weighted_push_relabel_shortcut_overlay"
								entityId={nodeId}
								visualRole={source ? "demand-source" : "demand-sink"}
								position={position}
								shape="ring"
								title={`${nodeId}: ${source ? "supply" : "demand"} ${terminalDemand.demand}`}
							/>
							<text
								className="flow-weighted-local-label"
								data-overlay-contribution="weighted_push_relabel_shortcut_overlay"
								data-overlay-feature-bundle={FEATURE_BUNDLE}
								data-overlay-entity-kind="node"
								data-overlay-entity-id={nodeId}
								data-overlay-role={
									source ? "demand-source-value" : "demand-sink-value"
								}
								x={position.x}
								y={position.y - FLOW_NODE_RADIUS - 12}
								textAnchor="middle"
							>
								{`${source ? "+" : "−"}${terminalDemand.demand}`}
							</text>
						</g>
					);
				})}
			{shortFlowMeasure !== undefined && (
				<WeightedShortFlowMeasure
					measure={shortFlowMeasure}
					positions={positions}
				/>
			)}
			{overlay.stage === "select-sparse-cut" &&
				overlay.residual_arcs.flatMap((arc) => {
					const fromState = nodeById.get(arc.from);
					const toState = nodeById.get(arc.to);
					if (
						fromState?.sparse_cut_side !== true ||
						toState?.sparse_cut_side !== false
					)
						return [];
					const edge = edgeById.get(arc.edge_id);
					const route = state.layout.routes.get(arc.edge_id);
					const from = positions.get(arc.from);
					const to = positions.get(arc.to);
					const path =
						route !== undefined
							? arc.direction === "forward"
								? route.path
								: route.reversePath
							: from !== undefined && to !== undefined
								? directedLinePath(from, to, 13)
								: undefined;
					if (path === undefined) {
						throw new Error(
							`weighted sparse-cut geometry is missing for ${arc.edge_id}`,
						);
					}
					return [
						<AdvancedPath
							key={`sparse-cut:${arc.edge_id}:${arc.direction}`}
							contribution="weighted_push_relabel_shortcut_overlay"
							entityKind={
								edge?.kind === "shortcut"
									? "auxiliary-residual-arc"
									: "residual-arc"
							}
							entityId={arc.edge_id}
							residualDirection={arc.direction}
							visualRole="sparse-cut-boundary"
							path={path}
							width={6}
							markerEnd={flowScopedSvgUrl(
								idScope,
								"flow-arrow-advanced-candidate",
							)}
							title={`${arc.from} → ${arc.to}: selected L${overlay.sparse_cut_level} residual cut, capacity ${arc.capacity}`}
						/>,
					];
				})}
			{certificateRole !== undefined &&
				overlay.edges.flatMap((edge) => {
					if (edge.kind !== "original") return [];
					const from = nodeById.get(edge.from);
					const to = nodeById.get(edge.to);
					if (from?.source_side !== true || to?.source_side !== false)
						return [];
					const route = state.layout.routes.get(edge.edge_id);
					if (route === undefined) {
						throw new Error(
							`weighted certificate route is missing for ${edge.edge_id}`,
						);
					}
					return [
						<AdvancedPath
							key={`certificate:${edge.edge_id}`}
							contribution="weighted_push_relabel_shortcut_overlay"
							entityKind="edge"
							entityId={edge.edge_id}
							visualRole={certificateRole}
							path={route.path}
							width={7}
							title={`${edge.edge_id}: ${certificateRole}, saturated ${edge.flow}/${edge.capacity}`}
						/>,
					];
				})}
		</g>
	);
}

function WeightedShortFlowMeasure({
	measure,
	positions,
}: Readonly<{
	measure: Readonly<{
		demand: string;
		routed: string;
		sinkId: string;
		sourceId: string;
		weightedLength: string;
		weightedUnits: string;
	}>;
	positions: ReadonlyMap<string, Point>;
}>) {
	const source = positions.get(measure.sourceId);
	const sink = positions.get(measure.sinkId);
	if (source === undefined || sink === undefined) {
		throw new Error("weighted short-flow measurement terminals are missing");
	}
	const demand = canonicalUnsignedInteger(
		measure.demand,
		"weighted short-flow demand",
	);
	const routed = canonicalUnsignedInteger(
		measure.routed,
		"weighted short-flow routed units",
	);
	const weightedLength = canonicalUnsignedInteger(
		measure.weightedLength,
		"weighted short-flow length",
	);
	const weightedUnits = canonicalUnsignedInteger(
		measure.weightedUnits,
		"weighted short-flow length units",
	);
	if (demand === 0n || routed > demand || weightedUnits === 0n) {
		throw new Error("weighted short-flow measurement is inconsistent");
	}
	const dx = sink.x - source.x;
	const dy = sink.y - source.y;
	const distance = Math.max(1, Math.hypot(dx, dy));
	const unitX = dx / distance;
	const unitY = dy / distance;
	const normalX = -unitY;
	const normalY = unitX;
	const start = {
		x: source.x + unitX * (FLOW_NODE_RADIUS + 7),
		y: source.y + unitY * (FLOW_NODE_RADIUS + 7),
	};
	const end = {
		x: sink.x - unitX * (FLOW_NODE_RADIUS + 7),
		y: sink.y - unitY * (FLOW_NODE_RADIUS + 7),
	};
	const bend = Math.min(54, Math.max(28, distance * 0.08));
	const control = {
		x: (start.x + end.x) / 2 + normalX * bend,
		y: (start.y + end.y) / 2 + normalY * bend,
	};
	const path = `M ${start.x} ${start.y} Q ${control.x} ${control.y} ${end.x} ${end.y}`;
	const progress = Number((routed * 10_000n) / demand) / 100;
	const average = `${weightedLength}/${weightedUnits}`;
	return (
		<g
			className="flow-weighted-short-flow-measure"
			data-weighted-short-flow-progress={progress}
		>
			<path
				className="flow-weighted-short-flow-rail"
				data-overlay-contribution="weighted_push_relabel_shortcut_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind="node"
				data-overlay-entity-id={measure.sourceId}
				data-overlay-role="short-flow-measure-rail"
				d={path}
				pathLength="100"
			/>
			<path
				className="flow-weighted-short-flow-routed"
				data-overlay-contribution="weighted_push_relabel_shortcut_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind="node"
				data-overlay-entity-id={measure.sourceId}
				data-overlay-role="short-flow-measure-routed"
				d={path}
				pathLength="100"
				strokeDasharray={`${progress} ${100 - progress}`}
			/>
			<text
				className="flow-weighted-local-label"
				data-overlay-contribution="weighted_push_relabel_shortcut_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind="node"
				data-overlay-entity-id={measure.sourceId}
				data-overlay-role="short-flow-measure-value"
				x={control.x}
				y={control.y - 8}
				textAnchor="middle"
			>
				{`SHORT FLOW ${measure.routed}/${measure.demand} · AVG ${average}`}
			</text>
		</g>
	);
}

/**
 * One row of the explicit reduced Laplacian used by the dense electrical solve.
 *
 * The source publishes a working-node ordinal for every Gaussian-elimination
 * pivot. Original-node ordinals are tied back to their canonical graph node;
 * later ordinals are boost vertices and remain visibly distinct auxiliary
 * equations. The cumulative source counter makes repeated pivots of the same
 * equation visibly distinct without inventing a simulated matrix value.
 */
function AugmentingElectricalPivotLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.augmentingElectrical;
	if (overlay?.active_pivot_node === undefined) return null;

	const active = canonicalUnsignedInteger(
		overlay.active_pivot_node,
		"augmenting-electrical active pivot",
	);
	const workingNodeCount = canonicalUnsignedInteger(
		overlay.working_nodes,
		"augmenting-electrical working-node count",
	);
	const pivotSerial = state.plan.context.metrics[2];
	canonicalUnsignedInteger(
		pivotSerial,
		"augmenting-electrical elimination-pivot counter",
	);
	const ordinals = visiblePivotOrdinals(active, workingNodeCount);
	const originalNodeCount = BigInt(overlay.nodes.length);
	const entityFor = (ordinal: bigint) => {
		if (ordinal >= originalNodeCount) {
			return {
				kind: "auxiliary-node" as const,
				id: `working-node:${ordinal}`,
			};
		}
		const node = overlay.nodes[Number(ordinal)];
		if (node === undefined) {
			throw new Error("augmenting-electrical original pivot node is missing");
		}
		return { kind: "node" as const, id: node.node_id };
	};
	const activeEntity = entityFor(active);
	const activeOriginalPosition =
		activeEntity.kind === "node"
			? state.positions.get(activeEntity.id)
			: undefined;

	const stripLeft = 118;
	const stripRight = FLOW_VIEWBOX_WIDTH - 36;
	const gap = 4;
	const cellWidth = Math.min(
		27,
		(stripRight - stripLeft - gap * Math.max(0, ordinals.length - 1)) /
			Math.max(1, ordinals.length),
	);
	const stripWidth =
		ordinals.length * cellWidth + Math.max(0, ordinals.length - 1) * gap;
	const startX = stripLeft + (stripRight - stripLeft - stripWidth) / 2;
	const stripY = FLOW_VIEWBOX_HEIGHT - 31;
	const visibleEntityIds = ordinals.map((ordinal) => entityFor(ordinal).id);
	const activeDescription =
		activeEntity.kind === "node"
			? `canonical node ${activeEntity.id}`
			: `boost vertex w${active}`;

	return (
		<g
			className="flow-advanced-algorithm-layer flow-augmenting-elimination-layer"
			data-advanced-overlay="augmenting-electrical-elimination"
			data-advanced-stage={overlay.stage}
			data-augmenting-working-node-ids={visibleEntityIds.join("|")}
			data-augmenting-active-pivot-entity-id={activeEntity.id}
		>
			<text
				className="flow-augmenting-elimination-caption"
				data-overlay-contribution="augmenting_electrical_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind={activeEntity.kind}
				data-overlay-entity-id={activeEntity.id}
				data-overlay-role="elimination-progress"
				x="24"
				y={stripY + 13}
			>
				{`PIVOT #${pivotSerial}`}
				<title>{`Dense elimination pivot ${pivotSerial} processes working equation w${active} of ${workingNodeCount}: ${activeDescription}`}</title>
			</text>
			<line
				className="flow-augmenting-elimination-baseline"
				data-overlay-contribution="augmenting_electrical_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind={activeEntity.kind}
				data-overlay-entity-id={activeEntity.id}
				data-overlay-role="working-system"
				x1={startX - 7}
				x2={startX + stripWidth + 7}
				y1={stripY + 10}
				y2={stripY + 10}
			/>
			{ordinals.map((ordinal, index) => {
				const entity = entityFor(ordinal);
				const selected = ordinal === active;
				const x = startX + index * (cellWidth + gap);
				return (
					<g key={ordinal.toString()}>
						<rect
							className={`flow-augmenting-elimination-cell${selected ? " flow-augmenting-elimination-cell-active" : ""}`}
							data-overlay-contribution="augmenting_electrical_overlay"
							data-overlay-feature-bundle={FEATURE_BUNDLE}
							data-overlay-entity-kind={entity.kind}
							data-overlay-entity-id={entity.id}
							data-overlay-role={
								selected ? "active-pivot-equation" : "working-equation"
							}
							x={x}
							y={stripY}
							width={cellWidth}
							height="20"
							rx="3"
						>
							<title>{`w${ordinal}: ${entity.kind === "node" ? `canonical node ${entity.id}` : "boost-vertex equation"}${selected ? `; active dense-elimination pivot #${pivotSerial}` : ""}`}</title>
						</rect>
						{cellWidth >= 18 && (
							<text
								className="flow-augmenting-elimination-cell-label"
								data-overlay-contribution="augmenting_electrical_overlay"
								data-overlay-feature-bundle={FEATURE_BUNDLE}
								data-overlay-entity-kind={entity.kind}
								data-overlay-entity-id={entity.id}
								data-overlay-role={
									selected ? "active-pivot-label" : "working-equation-label"
								}
								x={x + cellWidth / 2}
								y={stripY + 14}
								textAnchor="middle"
							>
								{`w${ordinal}`}
							</text>
						)}
					</g>
				);
			})}
			{activeOriginalPosition !== undefined && (
				<>
					<AdvancedNodeMark
						contribution="augmenting_electrical_overlay"
						entityId={activeEntity.id}
						visualRole="active-pivot-owner"
						position={activeOriginalPosition}
						shape="ring"
						title={`w${active}: dense-elimination pivot #${pivotSerial} is attached to ${activeEntity.id}`}
					/>
					<text
						className="flow-augmenting-elimination-node-label"
						data-overlay-contribution="augmenting_electrical_overlay"
						data-overlay-feature-bundle={FEATURE_BUNDLE}
						data-overlay-entity-kind="node"
						data-overlay-entity-id={activeEntity.id}
						data-overlay-role="active-pivot-owner-label"
						x={activeOriginalPosition.x}
						y={activeOriginalPosition.y - FLOW_NODE_RADIUS - 12}
						textAnchor="middle"
					>
						{`w${active}`}
					</text>
				</>
			)}
		</g>
	);
}

function AugmentingElectricalPreconditionerLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const idScope = useFlowGraphIdScope();
	const overlay = state.renderData.overlayViews.augmentingElectrical;
	if (overlay?.stage !== "add-preconditioning") return null;
	const model = state.plan.context.model;
	if (model.kind !== "max-flow") {
		throw new Error(
			"augmenting-electrical preconditioning requires max-flow terminals",
		);
	}
	const source = state.positions.get(model.source);
	const sink = state.positions.get(model.sink);
	if (source === undefined || sink === undefined) {
		throw new Error(
			"augmenting-electrical preconditioning terminals are missing",
		);
	}
	const workingEdges = canonicalUnsignedInteger(
		overlay.working_edges,
		"augmenting-electrical working-edge count",
	);
	const directedReductionEdges = BigInt(state.plan.edges.length) * 3n;
	if (workingEdges <= directedReductionEdges) {
		throw new Error("augmenting-electrical preconditioner bank is empty");
	}
	const preconditionerCount = workingEdges - directedReductionEdges;
	const midpointX = (source.x + sink.x) / 2;
	const controlY = Math.max(32, Math.min(source.y, sink.y) - 94);
	const path = `M ${source.x} ${source.y - FLOW_NODE_RADIUS - 4} Q ${midpointX} ${controlY} ${sink.x} ${sink.y - FLOW_NODE_RADIUS - 4}`;

	return (
		<g
			className="flow-advanced-algorithm-layer flow-augmenting-preconditioner-layer"
			data-advanced-overlay="augmenting-electrical-preconditioner"
			data-advanced-stage={overlay.stage}
			data-augmenting-preconditioner-count={preconditionerCount.toString()}
		>
			<AdvancedPath
				contribution="augmenting_electrical_overlay"
				entityKind="auxiliary-edge"
				entityId="preconditioner-bank"
				visualRole="preconditioner-bank"
				path={path}
				width={5}
				markerEnd={flowScopedSvgUrl(idScope, "flow-arrow-advanced-candidate")}
				title={`${preconditionerCount} symmetric source-to-sink preconditioner arcs; working target ${overlay.working_target}`}
			/>
			<text
				className="flow-augmenting-preconditioner-label"
				data-overlay-contribution="augmenting_electrical_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind="auxiliary-edge"
				data-overlay-entity-id="preconditioner-bank"
				data-overlay-role="preconditioner-count"
				data-preconditioner-count={preconditionerCount.toString()}
				data-working-target={overlay.working_target}
				x={midpointX}
				y={controlY + 10}
				textAnchor="middle"
			>
				{`${preconditionerCount}× PRECONDITIONER · TARGET ${overlay.working_target}`}
			</text>
		</g>
	);
}

function cleanupArcGeometry(
	from: Point,
	to: Point,
	index: number,
): Readonly<{ label: Point; path: string }> {
	const dx = to.x - from.x;
	const dy = to.y - from.y;
	const distance = Math.hypot(dx, dy);
	if (distance < 1) {
		const radius = FLOW_NODE_RADIUS + 14 + index * 3;
		return {
			path: `M ${from.x + FLOW_NODE_RADIUS * 0.7} ${from.y - FLOW_NODE_RADIUS * 0.7} C ${from.x + radius} ${from.y - radius} ${from.x - radius} ${from.y - radius} ${from.x - FLOW_NODE_RADIUS * 0.7} ${from.y - FLOW_NODE_RADIUS * 0.7}`,
			label: { x: from.x, y: from.y - radius },
		};
	}
	const ux = dx / distance;
	const uy = dy / distance;
	const nx = -uy;
	const ny = ux;
	const clearance = FLOW_NODE_RADIUS + 5;
	const bend = (index % 2 === 0 ? 1 : -1) * (9 + (index % 3) * 4);
	const start = {
		x: from.x + ux * clearance,
		y: from.y + uy * clearance,
	};
	const end = {
		x: to.x - ux * clearance,
		y: to.y - uy * clearance,
	};
	const label = {
		x: (start.x + end.x) / 2 + nx * bend,
		y: (start.y + end.y) / 2 + ny * bend,
	};
	return {
		path: `M ${start.x} ${start.y} Q ${label.x} ${label.y} ${end.x} ${end.y}`,
		label,
	};
}

/** Exact residual path used to complete the rounded transformed flow. */
function AugmentingElectricalCleanupLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const idScope = useFlowGraphIdScope();
	const overlay = state.renderData.overlayViews.augmentingElectrical;
	if (
		overlay?.stage !== "cleanup-augmenting-path" ||
		overlay.active_working_path.length === 0
	)
		return null;
	if (overlay.active_discrete_amount === undefined) {
		throw new Error("augmenting-electrical cleanup amount is missing");
	}
	const cleanupSerial = state.plan.context.metrics[8];
	canonicalUnsignedInteger(
		cleanupSerial,
		"augmenting-electrical cleanup-augmentation counter",
	);
	const workingEdgeIds = overlay.active_working_path.map(
		(arc) => `working-edge:${arc.edge}`,
	);
	const summary = overlay.active_working_path
		.map((arc) => `w${arc.edge} x=${arc.flow_after}`)
		.join(" · ");
	const firstArc = overlay.active_working_path[0];
	if (firstArc === undefined) return null;

	return (
		<g
			className="flow-advanced-algorithm-layer flow-augmenting-cleanup-layer"
			data-advanced-overlay="augmenting-electrical-cleanup"
			data-advanced-stage={overlay.stage}
			data-augmenting-working-edge-ids={workingEdgeIds.join("|")}
			data-augmenting-cleanup-serial={cleanupSerial}
			data-augmenting-cleanup-amount={overlay.active_discrete_amount}
		>
			{overlay.active_working_path.map((arc, index) => {
				const from = state.positions.get(arc.from_node);
				const to = state.positions.get(arc.to_node);
				if (from === undefined || to === undefined) {
					throw new Error(
						`augmenting-electrical cleanup arc w${arc.edge} has a missing endpoint`,
					);
				}
				const geometry = cleanupArcGeometry(from, to, index);
				const identity = `working-edge:${arc.edge}`;
				return (
					<g key={identity}>
						<AdvancedPath
							contribution="augmenting_electrical_overlay"
							entityKind="auxiliary-residual-arc"
							entityId={identity}
							visualRole="cleanup-working-arc"
							path={geometry.path}
							width={5}
							markerEnd={flowScopedSvgUrl(idScope, "flow-arrow-advanced-tree")}
							residualDirection={arc.direction}
							title={`Cleanup #${cleanupSerial}, arc ${index + 1}/${overlay.active_working_path.length}: ${arc.from_node} → ${arc.to_node} on working edge ${arc.edge}; push ${overlay.active_discrete_amount}; flow after ${arc.flow_after}`}
						/>
						<circle
							className="flow-augmenting-cleanup-order"
							data-overlay-contribution="augmenting_electrical_overlay"
							data-overlay-feature-bundle={FEATURE_BUNDLE}
							data-overlay-entity-kind="auxiliary-residual-arc"
							data-overlay-entity-id={identity}
							data-overlay-residual-direction={arc.direction}
							data-overlay-role="cleanup-path-order"
							cx={geometry.label.x}
							cy={geometry.label.y}
							r="8"
						/>
						<text
							className="flow-augmenting-cleanup-order-label"
							data-overlay-contribution="augmenting_electrical_overlay"
							data-overlay-feature-bundle={FEATURE_BUNDLE}
							data-overlay-entity-kind="auxiliary-residual-arc"
							data-overlay-entity-id={identity}
							data-overlay-residual-direction={arc.direction}
							data-overlay-role="cleanup-path-order-label"
							x={geometry.label.x}
							y={geometry.label.y + 3}
							textAnchor="middle"
						>
							{index + 1}
						</text>
					</g>
				);
			})}
			<text
				className="flow-augmenting-cleanup-summary"
				data-overlay-contribution="augmenting_electrical_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind="auxiliary-residual-arc"
				data-overlay-entity-id={`working-edge:${firstArc.edge}`}
				data-overlay-residual-direction={firstArc.direction}
				data-overlay-role="cleanup-path-summary"
				x={FLOW_VIEWBOX_WIDTH / 2}
				y={FLOW_VIEWBOX_HEIGHT - 15}
				textAnchor="middle"
			>
				{`CLEANUP #${cleanupSerial} · PUSH ${overlay.active_discrete_amount} · ${summary}`}
			</text>
		</g>
	);
}

function extractionStrokeWidth(value: string): number {
	const amount = canonicalUnsignedInteger(
		value,
		"augmenting-electrical extraction amount",
	);
	const bounded = amount > 63n ? 63 : Number(amount);
	return 1.5 + Math.log2(bounded + 1) * 0.9;
}

function AugmentingElectricalExtractionLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const idScope = useFlowGraphIdScope();
	const overlay = state.renderData.overlayViews.augmentingElectrical;
	if (
		overlay === undefined ||
		!(
			overlay.stage === "extract-directed-flow" ||
			overlay.stage === "cancel-extraction-cycle"
		)
	)
		return null;
	const model = state.plan.context.model;
	if (model.kind !== "max-flow") {
		throw new Error(
			"augmenting-electrical extraction requires max-flow terminals",
		);
	}
	const source = state.positions.get(model.source);
	const sink = state.positions.get(model.sink);
	if (source === undefined || sink === undefined) {
		throw new Error("augmenting-electrical extraction terminals are missing");
	}
	const activeArcs = new Set(
		overlay.active_extraction_cycle.map((arc) => `${arc.edge}:${arc.kind}`),
	);
	const extractionByEdgeId = new Map(
		overlay.edges.map((edge, ordinal) => [edge.edge_id, { edge, ordinal }]),
	);
	if (extractionByEdgeId.size !== overlay.edges.length) {
		throw new Error(
			"augmenting-electrical extraction contains duplicate edge identities",
		);
	}

	return (
		<g
			className="flow-advanced-algorithm-layer flow-augmenting-extraction-layer"
			data-advanced-overlay="augmenting-electrical-extraction"
			data-advanced-stage={overlay.stage}
			data-augmenting-extraction-edge-ids={overlay.edges
				.flatMap((edge) => [
					`extraction:${edge.edge_id}:toward-source`,
					`extraction:${edge.edge_id}:out-of-sink`,
				])
				.join("|")}
		>
			{state.plan.edges.flatMap((edge, routeIndex) => {
				const projected = extractionByEdgeId.get(edge.id);
				if (projected === undefined) {
					throw new Error(
						`augmenting-electrical extraction is missing edge ${edge.id}`,
					);
				}
				const { edge: extraction, ordinal } = projected;
				const central = extraction.extraction_central_scaled;
				const towardSource = extraction.extraction_toward_source;
				const outOfSink = extraction.extraction_out_of_sink;
				if (
					central === undefined ||
					towardSource === undefined ||
					outOfSink === undefined
				) {
					throw new Error(
						"augmenting-electrical extraction amounts are missing",
					);
				}
				const route = state.layout.routes.get(edge.id);
				const tail = state.positions.get(edge.from);
				const head = state.positions.get(edge.to);
				if (route === undefined || tail === undefined || head === undefined) {
					throw new Error(
						`augmenting-electrical extraction geometry is missing for ${edge.id}`,
					);
				}
				const extractionOrdinal = ordinal.toString();
				const centralActive = activeArcs.has(`${extractionOrdinal}:central`);
				const towardActive = activeArcs.has(
					`${extractionOrdinal}:toward-source`,
				);
				const sinkActive = activeArcs.has(`${extractionOrdinal}:out-of-sink`);
				const leaves: ReactNode[] = [
					<AdvancedPath
						key={`central:${edge.id}`}
						contribution="augmenting_electrical_overlay"
						entityKind="edge"
						entityId={edge.id}
						visualRole={
							centralActive
								? "extraction-cycle-active"
								: central === "0"
									? "extraction-central-zero"
									: "extraction-central"
						}
						path={route.path}
						width={extractionStrokeWidth(central)}
						markerEnd={
							centralActive
								? flowScopedSvgUrl(idScope, "flow-arrow-advanced-active")
								: undefined
						}
						title={`${edge.id}: recovered central reduction amount 2f=${central}${centralActive ? "; active cancellation cycle" : ""}`}
					/>,
				];
				if (towardSource !== "0" || towardActive) {
					const identity = `extraction:${edge.id}:toward-source`;
					leaves.push(
						<AdvancedPath
							key={identity}
							contribution="augmenting_electrical_overlay"
							entityKind="auxiliary-edge"
							entityId={identity}
							visualRole={
								towardActive
									? "extraction-cycle-active"
									: "extraction-auxiliary"
							}
							path={cleanupArcGeometry(head, source, routeIndex).path}
							width={extractionStrokeWidth(towardSource)}
							markerEnd={flowScopedSvgUrl(
								idScope,
								towardActive
									? "flow-arrow-advanced-active"
									: "flow-arrow-advanced-candidate",
							)}
							title={`${edge.id}: head ${edge.to} → source ${model.source}, auxiliary extraction amount ${towardSource}${towardActive ? "; active cancellation cycle" : ""}`}
						/>,
					);
				}
				if (outOfSink !== "0" || sinkActive) {
					const identity = `extraction:${edge.id}:out-of-sink`;
					leaves.push(
						<AdvancedPath
							key={identity}
							contribution="augmenting_electrical_overlay"
							entityKind="auxiliary-edge"
							entityId={identity}
							visualRole={
								sinkActive ? "extraction-cycle-active" : "extraction-auxiliary"
							}
							path={cleanupArcGeometry(sink, tail, routeIndex + 1).path}
							width={extractionStrokeWidth(outOfSink)}
							markerEnd={flowScopedSvgUrl(
								idScope,
								sinkActive
									? "flow-arrow-advanced-active"
									: "flow-arrow-advanced-candidate",
							)}
							title={`${edge.id}: sink ${model.sink} → tail ${edge.from}, auxiliary extraction amount ${outOfSink}${sinkActive ? "; active cancellation cycle" : ""}`}
						/>,
					);
				}
				return leaves;
			})}
		</g>
	);
}

function AugmentingElectricalCertificateLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.augmentingElectrical;
	if (
		overlay === undefined ||
		!(overlay.stage === "check-certificate" || overlay.stage === "optimal")
	)
		return null;
	const sourceSide = new Set(
		overlay.nodes
			.filter((node) => node.target_source_side)
			.map((node) => node.node_id),
	);
	const cutEdges = state.plan.edges.filter(
		(edge) => sourceSide.has(edge.from) && !sourceSide.has(edge.to),
	);
	const verified = overlay.stage === "optimal";
	const owner = cutEdges[0];
	const model = state.plan.context.model;
	if (model.kind !== "max-flow") {
		throw new Error(
			"augmenting-electrical certificate requires max-flow terminals",
		);
	}
	return (
		<g
			className="flow-advanced-algorithm-layer flow-augmenting-certificate-layer"
			data-advanced-overlay="augmenting-electrical-certificate"
			data-advanced-stage={overlay.stage}
			data-augmenting-certificate-cut-edges={cutEdges
				.map((edge) => edge.id)
				.join("|")}
		>
			{cutEdges.map((edge) => {
				const route = state.layout.routes.get(edge.id);
				if (route === undefined) {
					throw new Error(
						`augmenting-electrical certificate geometry is missing for ${edge.id}`,
					);
				}
				return (
					<AdvancedPath
						key={edge.id}
						contribution="augmenting_electrical_overlay"
						entityKind="edge"
						entityId={edge.id}
						visualRole={
							verified ? "certificate-cut-verified" : "certificate-cut-check"
						}
						path={route.path}
						width={6}
						title={`${edge.id}: exact source-side cut edge; capacity ${edge.capacity}; ${verified ? "maximum flow certified" : "independent checker evaluating flow/cut equality"}`}
					/>
				);
			})}
			<text
				className={`flow-augmenting-certificate-label flow-augmenting-certificate-label-${verified ? "verified" : "check"}`}
				data-overlay-contribution="augmenting_electrical_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind={owner === undefined ? "node" : "edge"}
				data-overlay-entity-id={owner?.id ?? model.source}
				data-overlay-role={
					verified ? "certificate-accepted" : "certificate-check"
				}
				x={FLOW_VIEWBOX_WIDTH / 2}
				y={FLOW_VIEWBOX_HEIGHT - 15}
				textAnchor="middle"
			>
				{`${verified ? "CERTIFIED" : "CHECK"} · FLOW = CUT = ${overlay.original_target}${cutEdges.length === 0 ? " · EMPTY CUT" : ` · ${cutEdges.length} CUT EDGE${cutEdges.length === 1 ? "" : "S"}`}`}
			</text>
		</g>
	);
}

const INTERIOR_POINT_REDUCTION_STAGES = Object.freeze([
	"build-b-matching-reduction",
	"build-min-cost-reduction",
	"initialize-central-path",
	"solve-electrical-direction",
	"descent-step",
	"solve-centering-direction",
	"centering-step",
	"extract-fractional-flow",
] as const);

function InteriorPointReductionLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.interiorPointMaxFlow;
	if (
		overlay === undefined ||
		!INTERIOR_POINT_REDUCTION_STAGES.includes(
			overlay.stage as (typeof INTERIOR_POINT_REDUCTION_STAGES)[number],
		)
	)
		return null;
	const showsWorkingReduction = overlay.stage !== "build-b-matching-reduction";
	const showsInitializedCentralPath = ![
		"build-b-matching-reduction",
		"build-min-cost-reduction",
	].includes(overlay.stage);
	const model = state.plan.context.model;
	if (model.kind !== "max-flow") return null;
	const edgeStateById = new Map(
		overlay.edges.map((edge) => [edge.edge_id, edge]),
	);
	return (
		<g
			className="flow-advanced-algorithm-layer"
			data-advanced-overlay="interior-point-max-flow-reduction"
			data-advanced-stage={overlay.stage}
		>
			{state.plan.edges.map((edge) => {
				if (edgeStateById.get(edge.id)?.normalized_away !== false) return null;
				const route = state.layout.routes.get(edge.id);
				if (route === undefined) return null;
				return (
					<AdvancedPath
						key={`b-matching:${edge.id}`}
						contribution="interior_point_max_flow_overlay"
						entityKind="edge"
						entityId={edge.id}
						visualRole={
							showsInitializedCentralPath
								? "initialized-central-path"
								: showsWorkingReduction
									? "working-hub-coupling"
									: "b-matching-direct"
						}
						path={route.path}
						width={
							showsInitializedCentralPath ? 6 : showsWorkingReduction ? 5 : 3
						}
						title={
							showsInitializedCentralPath
								? `${edge.id}: initialized zero-centered primal/slack pair`
								: showsWorkingReduction
									? `${edge.id}: direct b-matching arc plus its G_b hub coupling`
									: `${edge.id}: p_e to q_e direct b-matching arc`
						}
					/>
				);
			})}
			{state.plan.nodes.flatMap((node) => {
				const position = state.positions.get(node.id);
				if (position === undefined) return [];
				const marks: ReactNode[] = [];
				if (node.id !== model.source) {
					marks.push(
						<AdvancedNodeMark
							key={`b-matching-p:${node.id}`}
							contribution="interior_point_max_flow_overlay"
							entityId={node.id}
							visualRole="b-matching-p"
							position={{ x: position.x - 43, y: position.y }}
							shape="capacity"
							title={`${node.id}: P-side reduction vertex`}
						/>,
					);
				}
				if (node.id !== model.sink) {
					marks.push(
						<AdvancedNodeMark
							key={`b-matching-q:${node.id}`}
							contribution="interior_point_max_flow_overlay"
							entityId={node.id}
							visualRole={
								showsWorkingReduction ? "working-reduction-q" : "b-matching-q"
							}
							position={{ x: position.x + 43, y: position.y }}
							shape="steiner"
							title={
								showsWorkingReduction
									? `${node.id}: Q-side vertex coupled through the G_b hub`
									: `${node.id}: Q-side reduction vertex`
							}
						/>,
					);
				}
				return marks;
			})}
		</g>
	);
}

function InteriorPointCertificateLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const overlay = state.renderData.overlayViews.interiorPointMaxFlow;
	if (
		overlay === undefined ||
		!(overlay.stage === "check-certificate" || overlay.stage === "optimal")
	)
		return null;
	const model = state.plan.context.model;
	if (model.kind !== "max-flow") {
		throw new Error("interior-point certificate requires max-flow terminals");
	}
	const sourceSide = new Set(
		overlay.nodes
			.filter((node) => node.target_source_side)
			.map((node) => node.node_id),
	);
	if (!sourceSide.has(model.source) || sourceSide.has(model.sink)) {
		throw new Error("interior-point certificate has no separating cut");
	}
	const cutEdges = state.plan.edges.filter(
		(edge) => sourceSide.has(edge.from) && !sourceSide.has(edge.to),
	);
	const verified = overlay.stage === "optimal";
	const owner = cutEdges[0];
	return (
		<g
			className="flow-advanced-algorithm-layer flow-augmenting-certificate-layer"
			data-advanced-overlay="interior-point-max-flow-certificate"
			data-advanced-stage={overlay.stage}
			data-interior-point-certificate-cut-edges={cutEdges
				.map((edge) => edge.id)
				.join("|")}
		>
			{cutEdges.map((edge) => {
				const route = state.layout.routes.get(edge.id);
				if (route === undefined) {
					throw new Error(
						`interior-point certificate geometry is missing for ${edge.id}`,
					);
				}
				return (
					<AdvancedPath
						key={edge.id}
						contribution="interior_point_max_flow_overlay"
						entityKind="edge"
						entityId={edge.id}
						visualRole={
							verified ? "certificate-cut-verified" : "certificate-cut-check"
						}
						path={route.path}
						width={6}
						title={`${edge.id}: exact source-side cut edge; capacity ${edge.capacity}; ${verified ? "maximum flow certified" : "independent checker evaluating flow/cut equality"}`}
					/>
				);
			})}
			<text
				className={`flow-augmenting-certificate-label flow-augmenting-certificate-label-${verified ? "verified" : "check"}`}
				data-overlay-contribution="interior_point_max_flow_overlay"
				data-overlay-feature-bundle={FEATURE_BUNDLE}
				data-overlay-entity-kind={owner === undefined ? "node" : "edge"}
				data-overlay-entity-id={owner?.id ?? model.source}
				data-overlay-role={
					verified ? "certificate-accepted" : "certificate-check"
				}
				x={FLOW_VIEWBOX_WIDTH / 2}
				y={FLOW_VIEWBOX_HEIGHT - 15}
				textAnchor="middle"
			>
				{`${verified ? "CERTIFIED" : "CHECK"} · FLOW = CUT = ${overlay.target_value}${cutEdges.length === 0 ? " · EMPTY CUT" : ` · ${cutEdges.length} CUT EDGE${cutEdges.length === 1 ? "" : "S"}`}`}
			</text>
		</g>
	);
}

/**
 * Main-canvas projections for algorithm states whose native working graph is
 * richer than the ordinary capacity network. Context is deliberately subtle;
 * the current primitive, candidate, or mutation receives the vivid stroke.
 */
export function FlowGraphAdvancedAlgorithmFeatureBundle({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	return (
		<>
			<AugmentingElectricalPreconditionerLayer state={state} />
			<AugmentingElectricalPivotLayer state={state} />
			<AugmentingElectricalCleanupLayer state={state} />
			<AugmentingElectricalExtractionLayer state={state} />
			<AugmentingElectricalCertificateLayer state={state} />
			<InteriorPointReductionLayer state={state} />
			<InteriorPointCertificateLayer state={state} />
			<FrameworkMcfLayer state={state} />
			<MinimumRatioMcfLayer state={state} />
			<PrimalDualIpmMcfLayer state={state} />
			<RandomizedMcfLayer state={state} />
			<WeightedAugmentingPathsLayer state={state} />
			<WeightedPushRelabelLayer state={state} />
		</>
	);
}
