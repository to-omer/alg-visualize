import type { ReactNode } from "react";
import { FlowGraphOverlayContributionLayer } from "./FlowGraphOverlayContributionLayer";
import { FlowGraphOverlayOwnedLeaves } from "./FlowGraphOverlayOwnedLeaves";
import { FlowPredictionAttemptLadder } from "./FlowPredictionAttemptLadder";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import { flowAuxiliaryCellFocus } from "./flow-event-highlight";
import {
	FLOW_NODE_RADIUS,
	FLOW_VIEWBOX_HEIGHT,
	FLOW_VIEWBOX_WIDTH,
} from "./flow-layout";
import { flowNodeCanvasLabel } from "./flow-node-display-label";
import { formatFlowRational } from "./flow-parametric-view";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;

/** Composes registered fallback layers with the remaining rich SVG bundles. */
export function FlowGraphOverlayFeatureLayers({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const prediction = state.renderData.overlayViews.predictionAssistedEpsilon;
	return (
		<>
			{prediction !== undefined && (
				<FlowPredictionAttemptLadder overlay={prediction} />
			)}
			<FlowGraphOverlayContributionLayer state={state} />
		</>
	);
}

function compactMatrixNodeLabel(nodeId: string): string {
	const label = flowNodeCanvasLabel(nodeId);
	return label.length <= 16 ? label : `${label.slice(0, 13)}…`;
}

/**
 * Makes a source-published matrix cell visible as an auxiliary row/column relation.
 * The curved bracket is deliberately not arrowed: it is matrix work, not a
 * residual or original network arc.
 */
export function FlowGraphAuxiliaryCellLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const cell = flowAuxiliaryCellFocus(state.context);
	if (cell === undefined) return null;
	const row = state.positions.get(cell.rowNodeId);
	const column = state.positions.get(cell.columnNodeId);
	if (row === undefined || column === undefined) {
		throw new Error("Auxiliary matrix cell references an absent graph vertex");
	}
	const rowLabel = compactMatrixNodeLabel(cell.rowNodeId);
	const columnLabel = compactMatrixNodeLabel(cell.columnNodeId);
	const diagonal = cell.rowNodeId === cell.columnNodeId;
	const label = diagonal
		? cell.kind === "laplacian"
			? `L[${rowLabel}, ${rowLabel}] × d[${rowLabel}] · ${cell.completed}/${cell.total}`
			: `A[${rowLabel}, ${rowLabel}] · diagonal assignment cell · ${cell.completed}/${cell.total}`
		: cell.kind === "laplacian"
			? `ROW ${rowLabel} · L[${rowLabel}, ${columnLabel}] × d[${columnLabel}] · COL ${columnLabel} · ${cell.completed}/${cell.total}`
			: `ROW ${rowLabel} · A[${rowLabel}, ${columnLabel}] · COL ${columnLabel} · ${cell.completed}/${cell.total}`;
	const labelWidth = Math.max(170, Math.min(470, label.length * 6.2 + 24));
	const ownPublishedCell = (children: ReactNode) =>
		state.context.traceEvent?.catalog_id ===
		"relaxed-most-negative-cycle.inspect-assignment-cell" ? (
			<FlowGraphOverlayOwnedLeaves
				state={state}
				bundle="node-optimization"
				entity={{
					kind: "auxiliary-edge",
					id: `assignment-cell:${cell.rowNodeId}:${cell.columnNodeId}`,
				}}
				owners={[
					{
						overlay: "relaxed_mndc_overlay",
						role: "active_assignment_cell",
					},
				]}
			>
				{children}
			</FlowGraphOverlayOwnedLeaves>
		) : (
			children
		);

	if (diagonal) {
		const labelX = Math.max(
			14 + labelWidth / 2,
			Math.min(FLOW_VIEWBOX_WIDTH - 14 - labelWidth / 2, row.x),
		);
		const labelY = row.y > 92 ? row.y - 64 : row.y + 64;
		const anchorY = row.y + (labelY < row.y ? -1 : 1) * (FLOW_NODE_RADIUS + 8);
		return ownPublishedCell(
			<g
				className="flow-auxiliary-cell"
				data-auxiliary-cell={cell.kind}
				data-auxiliary-cell-shape="diagonal"
				data-matrix-row-node={cell.rowNodeId}
				data-matrix-column-node={cell.columnNodeId}
				data-matrix-cell-progress={`${cell.completed}:${cell.total}`}
			>
				<title>{`${cell.kind === "laplacian" ? "Grounded Laplacian" : "Assignment"} diagonal cell: row ${cell.rowNodeId}, column ${cell.columnNodeId}`}</title>
				<path
					className="flow-auxiliary-cell-underlay"
					d={`M ${row.x} ${anchorY} L ${row.x} ${labelY}`}
				/>
				<path
					className="flow-auxiliary-cell-link"
					d={`M ${row.x} ${anchorY} L ${row.x} ${labelY}`}
				/>
				<circle
					className="flow-auxiliary-cell-role"
					cx={row.x}
					cy={anchorY}
					r="8"
				/>
				<text
					className="flow-auxiliary-cell-role-label"
					x={row.x}
					y={anchorY}
					dominantBaseline="central"
					textAnchor="middle"
				>
					D
				</text>
				<g
					className="flow-auxiliary-cell-label"
					transform={`translate(${labelX - labelWidth / 2} ${labelY - 12})`}
				>
					<rect width={labelWidth} height="24" rx="6" />
					<text
						x={labelWidth / 2}
						y="12"
						dominantBaseline="central"
						textAnchor="middle"
					>
						{label}
					</text>
				</g>
			</g>,
		);
	}

	const dx = column.x - row.x;
	const dy = column.y - row.y;
	const distance = Math.max(1, Math.hypot(dx, dy));
	const unitX = dx / distance;
	const unitY = dy / distance;
	const normalX = -unitY;
	const normalY = unitX;
	const midpoint = { x: (row.x + column.x) / 2, y: (row.y + column.y) / 2 };
	const bend = Math.max(48, Math.min(88, distance * 0.2));
	const candidates = [
		{
			x: midpoint.x - normalX * bend,
			y: midpoint.y - normalY * bend,
		},
		{
			x: midpoint.x + normalX * bend,
			y: midpoint.y + normalY * bend,
		},
	] as const;
	const boundaryClearance = (point: { x: number; y: number }) =>
		Math.min(
			point.x - labelWidth / 2 - 14,
			FLOW_VIEWBOX_WIDTH - point.x - labelWidth / 2 - 14,
			point.y - 52,
			FLOW_VIEWBOX_HEIGHT - point.y - 34,
		);
	const control =
		boundaryClearance(candidates[0]) >= boundaryClearance(candidates[1])
			? candidates[0]
			: candidates[1];
	const labelX = Math.max(
		14 + labelWidth / 2,
		Math.min(FLOW_VIEWBOX_WIDTH - 14 - labelWidth / 2, control.x),
	);
	const labelY = Math.max(52, Math.min(FLOW_VIEWBOX_HEIGHT - 34, control.y));
	const radius = FLOW_NODE_RADIUS + 8;
	const start = { x: row.x + unitX * radius, y: row.y + unitY * radius };
	const end = {
		x: column.x - unitX * radius,
		y: column.y - unitY * radius,
	};
	const path = `M ${start.x} ${start.y} Q ${control.x} ${control.y} ${end.x} ${end.y}`;
	return ownPublishedCell(
		<g
			className="flow-auxiliary-cell"
			data-auxiliary-cell={cell.kind}
			data-auxiliary-cell-shape="off-diagonal"
			data-matrix-row-node={cell.rowNodeId}
			data-matrix-column-node={cell.columnNodeId}
			data-matrix-cell-progress={`${cell.completed}:${cell.total}`}
		>
			<title>{`${cell.kind === "laplacian" ? "Grounded Laplacian" : "Assignment"} cell: row ${cell.rowNodeId}, column ${cell.columnNodeId}`}</title>
			<path className="flow-auxiliary-cell-underlay" d={path} />
			<path className="flow-auxiliary-cell-link" d={path} />
			{[
				{ role: "R", point: start, nodeId: cell.rowNodeId },
				{ role: "C", point: end, nodeId: cell.columnNodeId },
			].map(({ role, point, nodeId }) => (
				<g
					key={`${role}:${nodeId}`}
					data-matrix-role={role}
					data-node-id={nodeId}
				>
					<circle
						className="flow-auxiliary-cell-role"
						cx={point.x}
						cy={point.y}
						r="8"
					/>
					<text
						className="flow-auxiliary-cell-role-label"
						x={point.x}
						y={point.y}
						dominantBaseline="central"
						textAnchor="middle"
					>
						{role}
					</text>
				</g>
			))}
			<g
				className="flow-auxiliary-cell-label"
				transform={`translate(${labelX - labelWidth / 2} ${labelY - 12})`}
			>
				<rect width={labelWidth} height="24" rx="6" />
				<text
					x={labelWidth / 2}
					y="12"
					dominantBaseline="central"
					textAnchor="middle"
				>
					{label}
				</text>
			</g>
		</g>,
	);
}

const CANCEL_TIGHTEN_STAGE_LABELS = Object.freeze({
	ready: "READY",
	initialize: "INITIALIZE PRICES",
	"begin-phase": "CANCEL ADMISSIBLE CYCLES",
	"inspect-cycle-arc": "SCAN FOR CYCLE",
	"select-cycle": "CYCLE SELECTED",
	"cancel-cycle": "SATURATE CYCLE",
	"inspect-rank-arc": "SCAN FOR RANKING",
	tighten: "TIGHTEN PRICES",
	optimal: "OPTIMAL",
});

const SCALING_PHASE_STAGE_LABELS = Object.freeze({
	"capacity-scaling-mcf.start-scaling-phase": "CAPACITY PHASE START",
	"capacity-scaling-mcf.complete-scaling-phase": "CAPACITY PHASE COMPLETE",
	"excess-scaling-mcf.start-excess-phase": "EXCESS PHASE START",
	"excess-scaling-mcf.complete-excess-phase": "EXCESS PHASE COMPLETE",
});

const GOLDBERG_RAO_STAGE_LABELS = Object.freeze({
	"goldberg-rao.initialize-cut-gap": {
		stage: "INITIALIZE CUT GAP",
		detail: "GAP",
		label: "gap-upper-bound",
	},
	"goldberg-rao.start-gap-phase": {
		stage: "BINARY-LENGTH PHASE START",
		detail: "Δ",
		label: "delta",
	},
	"goldberg-rao.binary-length-distance": {
		stage: "BUILD BINARY LENGTHS",
		detail: "ZERO ARCS",
		label: "zero-length-arcs",
	},
	"goldberg-rao.minimum-canonical-cut": {
		stage: "SELECT MINIMUM CANONICAL CUT",
		detail: "CAPACITY",
		label: "cut-capacity",
	},
	"goldberg-rao.mark-special-arcs": {
		stage: "MARK SPECIAL ARCS",
		detail: "ARCS",
		label: "special-arcs",
	},
	"goldberg-rao.contract-zero-scc": {
		stage: "CONTRACT ZERO-LENGTH SCCS",
		detail: "COMPONENTS",
		label: "components",
	},
	"goldberg-rao.blocking-or-delta-flow": {
		stage: "AUGMENT CONTRACTED DAG",
		detail: "FLOW",
		label: "delta-flow",
	},
	"goldberg-rao.lift-component-flow": {
		stage: "LIFT COMPONENT FLOW",
		detail: "ROUTES",
		label: "routing-paths",
	},
	"goldberg-rao.halve-cut-gap": {
		stage: "UPDATE CUT GAP",
		detail: "GAP",
		label: "gap-upper-bound",
	},
	"goldberg-rao.optimal": {
		stage: "OPTIMAL",
		detail: "FLOW",
		label: "flow-value",
	},
});

const HASSIN_STAGE_IDS = new Set([
	"hassin-st-planar.split-outer-face",
	"hassin-st-planar.settle-dual-face",
	"hassin-st-planar.reconstruct-primal-flow",
	"hassin-st-planar.optimal-dual-cut",
]);

const ENHANCED_SCALING_STAGE_LABELS = Object.freeze({
	ready: "READY",
	initialize: "INITIALIZE QUOTIENT",
	"complete-regeneration": "REGENERATION COMPLETE",
	"begin-phase": "PHASE START",
	contract: "CONTRACT STRONGLY FEASIBLE ARC",
	"inspect-residual-arc": "INSPECT RESIDUAL ARC",
	"select-path": "SELECT QUOTIENT PATH",
	augment: "AUGMENT",
	"complete-phase": "PHASE COMPLETE",
	"halve-scale": "SCALE HALVED",
	"recover-primal": "RECOVER PRIMAL FLOW",
	optimal: "OPTIMAL",
});

const ORLIN_MCF_STAGE_LABELS = Object.freeze({
	ready: "READY",
	"transform-capacities": "EXPAND FINITE CAPACITIES",
	"initialize-dual": "INITIALIZE DUAL PRICES",
	"complete-regeneration": "REGENERATE Δ",
	"begin-phase": "SCALING PHASE START",
	"inspect-contractible-arc": "TEST CONTRACTION ARC",
	"inspect-reachability-arc": "SCAN REACHABILITY ARC",
	"inspect-compressed-residual-arc": "CLASSIFY RESIDUAL ARC",
	"inspect-compressed-arc": "RELAX COMPRESSED ARC",
	contract: "CONTRACT 3nΔ ARC",
	"select-compressed-path": "SELECT QUOTIENT PATH",
	augment: "AUGMENT EXACT Δ",
	"complete-phase": "SCALING PHASE COMPLETE",
	"halve-scale": "SCALE HALVED",
	"expand-dual": "EXPAND DUAL PRICES",
	"recover-primal": "RECOVER PRIMAL FLOW",
	optimal: "OPTIMAL",
});

const COST_SCALING_REFINE_STAGE_LABELS = Object.freeze({
	"cost-scaling.start-refine": "REFINE START",
	"cost-scaling.complete-refine": "REFINE COMPLETE",
	"cost-scaling-push-relabel.start-refine": "REFINE START",
	"cost-scaling-push-relabel.complete-refine": "REFINE COMPLETE",
	"augment-relabel.start-refine": "REFINE START",
	"augment-relabel.complete-refine": "REFINE COMPLETE",
	"partial-augment-relabel-mcf.start-refine": "REFINE START",
	"partial-augment-relabel-mcf.complete-refine": "REFINE COMPLETE",
	"price-refinement.start-refine": "REFINE START",
	"price-refinement.complete-refine": "REFINE COMPLETE",
	"arc-fixing.start-refine": "REFINE START",
	"arc-fixing.complete-refine": "REFINE COMPLETE",
	"generalized-cost-scaling.start-refine": "REFINE START",
	"generalized-cost-scaling.complete-refine": "REFINE COMPLETE",
});

const PRICE_REFINEMENT_STAGE_LABELS = Object.freeze({
	"price-refinement.start-potential-only-attempt": "PRICE-ONLY ATTEMPT",
	"price-refinement.complete-relaxation-round": "RELAXATION ROUND COMPLETE",
	"price-refinement.succeed-without-flow-change": "PRICE CERTIFIED",
	"price-refinement.fail-and-rollback-prices": "ROLL BACK PRICES",
});

const PRICE_REFINEMENT_DETAIL_LABELS = Object.freeze({
	epsilon: "ε",
	round: "ROUND",
	"flow-changes": "FLOW CHANGES",
	"negative-cycle": "NEGATIVE CYCLE",
});

const POLYNOMIAL_PRIMAL_STAGE_LABELS = Object.freeze({
	ready: "READY",
	"initialize-basis": "INITIALIZE PERTURBED BASIS",
	"begin-scale": "SCALE START",
	"inspect-residual": "INSPECT EXTENDED ARC",
	"select-admissible": "SELECT ADMISSIBLE ARC",
	pivot: "PIVOT FUNDAMENTAL CYCLE",
	"modify-premultipliers": "MODIFY PREMULTIPLIERS",
	"finish-scale": "SCALE COMPLETE",
	optimal: "OPTIMAL",
});

const POLYNOMIAL_DUAL_STAGE_LABELS = Object.freeze({
	ready: "READY",
	"inspect-initial-arc": "INSPECT INITIAL-TREE ARC",
	"initialize-tree": "INITIALIZE TREE",
	"initialize-pseudoflow": "INITIALIZE PSEUDOFLOW",
	"begin-scale": "SCALE START",
	"inspect-augmentation-arc": "INSPECT AUGMENTATION ARC",
	"select-active": "SELECT ACTIVE NODE",
	"augment-to-root": "AUGMENT TO ROOT",
	"select-bad-arc": "SELECT BAD ARC",
	"inspect-entering-arc": "INSPECT PRICING ARC",
	"select-entering": "SELECT ENTERING ARC",
	"pivot-make-good": "MAKE-GOOD PIVOT",
	"finish-scale": "SCALE COMPLETE",
	optimal: "OPTIMAL",
});

function uppercaseFlowStage(stage: string): string {
	return stage.replaceAll("-", " ").toUpperCase();
}

/**
 * Names the exact source operation represented by the current boundary.
 * The compact chip is intentionally independent of typed overlay state: two
 * source operations can inspect the same graph state while still consuming
 * distinct measured work.
 */
export function FlowGraphSourceOperationLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const event = state.context.traceEvent;
	if (event === undefined) return null;
	const action = uppercaseFlowStage(
		event.catalog_id.split(".").at(-1) ?? event.catalog_id,
	);
	const semantics = state.context.traceEventSemantics;
	const block = semantics?.primary_work_block;
	const progress = semantics?.work_progress;
	const primaryDelta = semantics?.work_deltas.find(
		(delta) => delta.unit === "primary-work",
	);
	const position =
		progress === undefined
			? ""
			: primaryDelta === undefined
				? ` · STEP ${progress.detail_completed}/${progress.detail_total}`
				: ` · ${progress.primary_completed}/${progress.primary_total}`;
	const batch =
		block === undefined || semantics?.aggregation_count === "1"
			? ""
			: ` · BATCH ${block.first === block.last ? block.last : `${block.first}–${block.last}`}/${block.total}`;
	const measurement =
		event.detail === undefined
			? ""
			: ` · ${uppercaseFlowStage(event.detail.label)} ${event.detail.value}`;
	const label = `${action}${position}${batch}${measurement}`;
	const width = Math.max(130, Math.min(650, label.length * 6.4 + 22));
	return (
		<g
			className="flow-overlay-stage-badge flow-source-operation-badge"
			data-source-operation={event.catalog_id}
			data-source-operation-position={
				block === undefined
					? undefined
					: `${block.first}:${block.last}:${block.total}`
			}
			data-source-operation-progress={
				progress === undefined
					? undefined
					: `${progress.detail_completed}:${progress.detail_total}:${progress.primary_completed}:${progress.primary_total}`
			}
			transform="translate(14 505)"
		>
			<rect width={width} height="22" rx="6" />
			<text x="11" y="11" dominantBaseline="central">
				{label}
			</text>
		</g>
	);
}

/** Keeps scalar Cancel-and-Tighten phase work visible without broad focus rings. */
export function FlowGraphOverlayStatusLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const parametric = state.renderData.overlayViews.parametric;
	if (parametric !== undefined) {
		const traversal = parametric.traversal;
		const parameter = formatFlowRational(parametric.parameter);
		const interval =
			traversal === undefined
				? ""
				: ` · RANGE [${formatFlowRational(traversal.lower)}, ${formatFlowRational(traversal.upper)}]`;
		const staticRun =
			traversal?.static_run_ordinal === undefined
				? ""
				: ` · COLD RUN ${traversal.static_run_ordinal}`;
		const label = `${uppercaseFlowStage(parametric.stage)} · λ ${parameter}${interval}${staticRun}`;
		const width = Math.max(160, Math.min(540, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-parametric-stage-badge"
				data-overlay-contribution="parametric_overlay"
				data-parametric-stage={parametric.stage}
				data-parametric-parameter={parameter}
				data-parametric-range={
					traversal === undefined
						? undefined
						: `${formatFlowRational(traversal.lower)}:${formatFlowRational(traversal.upper)}`
				}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const electrical = state.renderData.overlayViews.electricalFlow;
	if (electrical !== undefined) {
		const stage = uppercaseFlowStage(electrical.stage);
		const label = `${stage} · CG ${electrical.iteration} · ‖r‖₂ ${electrical.residual_l2} / ${electrical.relative_tolerance}`;
		const width = Math.max(160, Math.min(450, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-electrical-stage-badge"
				data-overlay-contribution="electrical_flow_overlay"
				data-electrical-stage={electrical.stage}
				data-electrical-iteration={electrical.iteration}
				data-electrical-residual={electrical.residual_l2}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const augmentingElectrical =
		state.renderData.overlayViews.augmentingElectrical;
	if (augmentingElectrical !== undefined) {
		const stage = uppercaseFlowStage(augmentingElectrical.stage);
		if (
			(augmentingElectrical.active_working_path.length > 0 ||
				augmentingElectrical.active_extraction_cycle.length > 0) &&
			augmentingElectrical.active_discrete_amount === undefined
		) {
			throw new Error("Augmenting-electrical path omitted its discrete amount");
		}
		const pivot =
			augmentingElectrical.active_pivot_node === undefined
				? ""
				: ` · PIVOT w${augmentingElectrical.active_pivot_node}/${augmentingElectrical.working_nodes}`;
		const workingPath = augmentingElectrical.active_working_path;
		const extractionCycle = augmentingElectrical.active_extraction_cycle;
		const activeAction =
			workingPath.length > 0
				? ` · PUSH ${augmentingElectrical.active_discrete_amount} · ${workingPath.length} ${workingPath.length === 1 ? "ARC" : "ARCS"}`
				: extractionCycle.length > 0
					? ` · CANCEL ${augmentingElectrical.active_discrete_amount} · ${extractionCycle.length} ${extractionCycle.length === 1 ? "ARC" : "ARCS"}`
					: "";
		const label = `${stage}${pivot}${activeAction} · WORK ${augmentingElectrical.working_nodes}V/${augmentingElectrical.working_edges}E · FLOW ${augmentingElectrical.current_value}/${augmentingElectrical.working_target} · REM ${augmentingElectrical.remaining}`;
		const exactAction =
			workingPath.length > 0
				? `Push ${augmentingElectrical.active_discrete_amount} on ${workingPath.map((arc) => `${arc.from_node}→${arc.to_node} [w${arc.edge}, x=${arc.flow_after}]`).join("; ")}`
				: extractionCycle.length > 0
					? `Cancel ${augmentingElectrical.active_discrete_amount} on ${extractionCycle.map((arc) => `${arc.kind} [e${arc.edge}]`).join("; ")}`
					: undefined;
		const width = Math.max(160, Math.min(470, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-augmenting-electrical-stage-badge"
				data-overlay-contribution="augmenting_electrical_overlay"
				data-augmenting-electrical-stage={augmentingElectrical.stage}
				data-augmenting-electrical-work={`${augmentingElectrical.working_nodes}:${augmentingElectrical.working_edges}`}
				data-augmenting-electrical-progress={`${augmentingElectrical.current_value}:${augmentingElectrical.working_target}:${augmentingElectrical.remaining}`}
				data-augmenting-electrical-pivot={
					augmentingElectrical.active_pivot_node
				}
				data-augmenting-electrical-path={
					augmentingElectrical.active_working_path.length === 0
						? undefined
						: augmentingElectrical.active_working_path
								.map((arc) => `${arc.direction}:${arc.edge}`)
								.join(",")
				}
				data-augmenting-electrical-amount={
					augmentingElectrical.active_discrete_amount
				}
				data-augmenting-electrical-cycle={
					augmentingElectrical.active_extraction_cycle.length === 0
						? undefined
						: augmentingElectrical.active_extraction_cycle
								.map((arc) => `${arc.kind}:${arc.edge}`)
								.join(",")
				}
				transform="translate(14 14)"
			>
				<title>
					{exactAction === undefined ? label : `${label}. ${exactAction}`}
				</title>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const interiorPoint = state.renderData.overlayViews.interiorPointMaxFlow;
	if (interiorPoint !== undefined) {
		const stage = uppercaseFlowStage(interiorPoint.stage);
		const label = `${stage} · μ ${interiorPoint.mu} · GAP ${interiorPoint.duality_gap} · E ${interiorPoint.electrical_energy}`;
		const width = Math.max(160, Math.min(450, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-interior-point-stage-badge"
				data-overlay-contribution="interior_point_max_flow_overlay"
				data-interior-point-stage={interiorPoint.stage}
				data-interior-point-mu={interiorPoint.mu}
				data-interior-point-gap={interiorPoint.duality_gap}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const minimumRatio = state.renderData.overlayViews.minimumRatioCycle;
	if (minimumRatio !== undefined) {
		const stage = uppercaseFlowStage(minimumRatio.stage);
		const candidate =
			minimumRatio.candidate_ratio === undefined
				? ""
				: ` · CAND ${formatFlowRational(minimumRatio.candidate_ratio)}`;
		const best =
			minimumRatio.best_ratio === undefined
				? ""
				: ` · BEST ${formatFlowRational(minimumRatio.best_ratio)}`;
		const label = `${stage} · VECTOR ${minimumRatio.enumerated_vectors}${candidate}${best}`;
		const width = Math.max(160, Math.min(450, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-minimum-ratio-stage-badge"
				data-overlay-contribution="minimum_ratio_cycle_overlay"
				data-minimum-ratio-stage={minimumRatio.stage}
				data-minimum-ratio-vector={minimumRatio.enumerated_vectors}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const randomized = state.renderData.overlayViews.randomizedAlmostLinear;
	if (randomized !== undefined) {
		const stage = uppercaseFlowStage(randomized.stage);
		const label = `${stage} · RETURN ${randomized.return_flow}/${randomized.return_capacity} · ARTIFICIAL ${randomized.artificial_flow} · ITER ${randomized.iteration}`;
		const width = Math.max(160, Math.min(470, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-randomized-almost-linear-stage-badge"
				data-overlay-contribution="randomized_almost_linear_overlay"
				data-randomized-almost-linear-stage={randomized.stage}
				data-randomized-almost-linear-return={`${randomized.return_flow}:${randomized.return_capacity}`}
				data-randomized-almost-linear-artificial={randomized.artificial_flow}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const deterministic = state.renderData.overlayViews.deterministicAlmostLinear;
	if (deterministic !== undefined) {
		const stage = uppercaseFlowStage(deterministic.stage);
		const inspectingCycle =
			deterministic.stage === "inspect-fundamental-cycle" ||
			deterministic.stage === "query-minimum-ratio-cycle";
		const chain = inspectingCycle
			? ` · EVAL ${deterministic.fundamental_cycles} · CHAIN L${deterministic.active_level ?? "—"} B${deterministic.active_branches.join("/")} P${deterministic.passes.join("/")} · OFF w${deterministic.selected_off_tree_edge ?? "—"}`
			: "";
		const label = `${stage}${chain} · RETURN ${deterministic.return_flow}/${deterministic.return_capacity} · ARTIFICIAL ${deterministic.artificial_flow} · CORE ${deterministic.core_vertices}V/${deterministic.core_edges}E`;
		const width = Math.max(160, Math.min(680, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-deterministic-almost-linear-stage-badge"
				data-overlay-contribution="deterministic_almost_linear_overlay"
				data-deterministic-almost-linear-stage={deterministic.stage}
				data-deterministic-almost-linear-return={`${deterministic.return_flow}:${deterministic.return_capacity}`}
				data-deterministic-almost-linear-artificial={
					deterministic.artificial_flow
				}
				data-deterministic-almost-linear-chain={`${deterministic.active_level ?? ""}:${deterministic.active_branches.join(",")}:${deterministic.passes.join(",")}`}
				data-deterministic-almost-linear-off-tree={
					deterministic.selected_off_tree_edge
				}
				data-deterministic-almost-linear-cycle={
					deterministic.fundamental_cycles
				}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const weightedAugmenting =
		state.renderData.overlayViews.weightedAugmentingPaths;
	if (weightedAugmenting !== undefined) {
		const stage = uppercaseFlowStage(weightedAugmenting.stage);
		const phase = (BigInt(weightedAugmenting.phase) + 1n).toString();
		const label = `${stage} · PHASE ${phase}/${weightedAugmenting.phase_count} · BIT b${weightedAugmenting.capacity_bit} · ROUND ${weightedAugmenting.round} · RELABEL ${weightedAugmenting.relabel_jumps} · PATH ${weightedAugmenting.augmentations}`;
		const width = Math.max(160, Math.min(650, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-weighted-augmenting-stage-badge"
				data-overlay-contribution="weighted_augmenting_paths_overlay"
				data-weighted-augmenting-stage={weightedAugmenting.stage}
				data-weighted-augmenting-phase={`${weightedAugmenting.phase}:${weightedAugmenting.phase_count}:${weightedAugmenting.capacity_bit}`}
				data-weighted-augmenting-round={weightedAugmenting.round}
				data-weighted-augmenting-work={`${weightedAugmenting.relabel_jumps}:${weightedAugmenting.augmentations}`}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const weightedPushRelabel =
		state.renderData.overlayViews.weightedPushRelabelShortcut;
	if (weightedPushRelabel !== undefined) {
		const stage = uppercaseFlowStage(weightedPushRelabel.stage);
		const label = `${stage} · HIERARCHY ${weightedPushRelabel.hierarchy_levels}L · h ${weightedPushRelabel.height} · ROUTED ${weightedPushRelabel.routed}/${weightedPushRelabel.demand} · RELABEL ${weightedPushRelabel.relabel_steps} · PATH ${weightedPushRelabel.augmentations} · ROUND ${weightedPushRelabel.residual_rounds}`;
		const width = Math.max(160, Math.min(680, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-weighted-push-relabel-stage-badge"
				data-overlay-contribution="weighted_push_relabel_shortcut_overlay"
				data-weighted-push-relabel-stage={weightedPushRelabel.stage}
				data-weighted-push-relabel-hierarchy={
					weightedPushRelabel.hierarchy_levels
				}
				data-weighted-push-relabel-routing={`${weightedPushRelabel.routed}:${weightedPushRelabel.demand}`}
				data-weighted-push-relabel-work={`${weightedPushRelabel.relabel_steps}:${weightedPushRelabel.augmentations}:${weightedPushRelabel.residual_rounds}`}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const overlay = state.renderData.overlayViews.cancelTighten;
	if (overlay !== undefined) {
		const stage = CANCEL_TIGHTEN_STAGE_LABELS[overlay.stage];
		const phase = BigInt(overlay.phase);
		const label = `${phase > 0n ? `PHASE ${overlay.phase} · ` : ""}${stage} · ε ${formatFlowRational(overlay.epsilon)}${overlay.delta === undefined ? "" : ` · Δ ${overlay.delta}`}`;
		const width = Math.max(160, Math.min(390, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-cancel-tighten-stage-badge"
				data-overlay-contribution="cancel_tighten_overlay"
				data-cancel-tighten-stage={overlay.stage}
				data-cancel-tighten-phase={overlay.phase}
				data-cancel-tighten-epsilon={formatFlowRational(overlay.epsilon)}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const enhanced = state.renderData.overlayViews.enhancedCapacityScaling;
	if (enhanced !== undefined) {
		const phase = BigInt(enhanced.phase);
		const stage = ENHANCED_SCALING_STAGE_LABELS[enhanced.stage];
		const delta = formatFlowRational(enhanced.delta);
		const augmentation =
			enhanced.augmentation === undefined
				? ""
				: ` · PUSH ${formatFlowRational(enhanced.augmentation)}`;
		const label = `${phase > 0n ? `PHASE ${enhanced.phase} · ` : ""}${stage} · Δ ${delta}${augmentation}`;
		const width = Math.max(160, Math.min(430, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-enhanced-scaling-stage-badge"
				data-overlay-contribution="enhanced_capacity_scaling_overlay"
				data-enhanced-scaling-stage={enhanced.stage}
				data-enhanced-scaling-phase={enhanced.phase}
				data-enhanced-scaling-delta={delta}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const orlin = state.renderData.overlayViews.orlinMcf;
	if (orlin !== undefined) {
		const phase = BigInt(orlin.phase);
		const stage = ORLIN_MCF_STAGE_LABELS[orlin.stage];
		const delta = formatFlowRational(orlin.delta);
		const augmentation =
			orlin.augmentation === undefined
				? ""
				: ` · PUSH ${formatFlowRational(orlin.augmentation)}`;
		const label = `${phase > 0n ? `PHASE ${orlin.phase} · ` : ""}${stage} · Δ ${delta}${augmentation}`;
		const width = Math.max(160, Math.min(450, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-orlin-mcf-stage-badge"
				data-overlay-contribution="orlin_mcf_overlay"
				data-orlin-mcf-stage={orlin.stage}
				data-orlin-mcf-phase={orlin.phase}
				data-orlin-mcf-delta={delta}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const polynomialPrimal =
		state.renderData.overlayViews.polynomialPrimalSimplex;
	if (polynomialPrimal !== undefined) {
		const phase = BigInt(polynomialPrimal.phase);
		const stage = POLYNOMIAL_PRIMAL_STAGE_LABELS[polynomialPrimal.stage];
		const epsilon =
			polynomialPrimal.epsilon === undefined
				? ""
				: ` · ε ${formatFlowRational(polynomialPrimal.epsilon)}`;
		const pivot =
			polynomialPrimal.delta === undefined
				? ""
				: ` · Δ ${formatFlowRational(polynomialPrimal.delta)}`;
		const shift =
			polynomialPrimal.potential_shift === undefined
				? ""
				: ` · SHIFT ${formatFlowRational(polynomialPrimal.potential_shift)}`;
		const label = `${phase > 0n ? `PHASE ${polynomialPrimal.phase} · ` : ""}${stage}${epsilon}${pivot}${shift}`;
		const width = Math.max(160, Math.min(450, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-polynomial-stage-badge"
				data-overlay-contribution="polynomial_primal_simplex_overlay"
				data-polynomial-primal-stage={polynomialPrimal.stage}
				data-polynomial-primal-phase={polynomialPrimal.phase}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const polynomialDual = state.renderData.overlayViews.polynomialDualSimplex;
	if (polynomialDual !== undefined) {
		const phase = BigInt(polynomialDual.phase);
		const stage = POLYNOMIAL_DUAL_STAGE_LABELS[polynomialDual.stage];
		const delta = formatFlowRational(polynomialDual.delta);
		const shift =
			polynomialDual.pivot_price_delta === undefined
				? ""
				: ` · SHIFT ${polynomialDual.pivot_price_delta}`;
		const label = `${phase > 0n ? `PHASE ${polynomialDual.phase} · ` : ""}${stage} · Δ ${delta}${shift}`;
		const width = Math.max(160, Math.min(450, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-polynomial-stage-badge"
				data-overlay-contribution="polynomial_dual_simplex_overlay"
				data-polynomial-dual-stage={polynomialDual.stage}
				data-polynomial-dual-phase={polynomialDual.phase}
				data-polynomial-dual-delta={delta}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const traceEvent = state.context.traceEvent;
	if (
		traceEvent?.catalog_id === "excess-scaling-push-relabel.scale-phase" &&
		traceEvent.detail?.label === "delta"
	) {
		const label = `Δ PHASE · Δ ${traceEvent.detail.value}`;
		const width = Math.max(160, label.length * 6.4 + 22);
		return (
			<g
				className="flow-overlay-stage-badge flow-excess-scaling-stage-badge"
				data-excess-scaling-stage={traceEvent.catalog_id}
				data-excess-scaling-delta={traceEvent.detail.value}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	if (
		traceEvent?.catalog_id ===
			"excess-scaling-push-relabel.select-scaled-active" &&
		traceEvent.detail?.label === "excess"
	) {
		const selectedNode = traceEvent.entity_refs.find(
			(entity) => entity.kind === "node",
		);
		if (selectedNode === undefined) {
			throw new Error("Excess-scaling selection omitted its active vertex");
		}
		const node = flowNodeCanvasLabel(selectedNode.node_id);
		const label = `SELECT ${node} · EXCESS ${traceEvent.detail.value}`;
		const width = Math.max(160, label.length * 6.4 + 22);
		return (
			<g
				className="flow-overlay-stage-badge flow-excess-scaling-stage-badge"
				data-excess-scaling-stage={traceEvent.catalog_id}
				data-excess-scaling-node={selectedNode.node_id}
				data-excess-scaling-excess={traceEvent.detail.value}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const eibfs = state.renderData.overlayViews.eibfs;
	const dynamicEibfs = state.renderData.overlayViews.dynamicEibfs;
	if (eibfs !== undefined) {
		const action =
			traceEvent === undefined
				? "READY"
				: uppercaseFlowStage(
						traceEvent.catalog_id.split(".").at(-1) ?? traceEvent.catalog_id,
					);
		const depth = `S${eibfs.source_depth}/T${eibfs.sink_depth}`;
		const label =
			dynamicEibfs === undefined
				? `${action} · ${uppercaseFlowStage(eibfs.phase_direction)} GROW · ${depth}`
				: `${action} · ${uppercaseFlowStage(dynamicEibfs.stage)} · UPDATE ${dynamicEibfs.update_index}/${dynamicEibfs.update_total} · ${depth} · REPAIR SCAN ${dynamicEibfs.repair_arc_scans}`;
		const width = Math.max(160, Math.min(650, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-eibfs-stage-badge"
				data-overlay-contribution={
					dynamicEibfs === undefined ? "eibfs_overlay" : "dynamic_eibfs_overlay"
				}
				data-eibfs-action={traceEvent?.catalog_id}
				data-eibfs-depth={`${eibfs.source_depth}:${eibfs.sink_depth}`}
				data-eibfs-direction={eibfs.phase_direction}
				data-dynamic-eibfs-stage={dynamicEibfs?.stage}
				data-dynamic-eibfs-update={
					dynamicEibfs === undefined
						? undefined
						: `${dynamicEibfs.update_index}:${dynamicEibfs.update_total}`
				}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const goldbergStage =
		traceEvent === undefined
			? undefined
			: GOLDBERG_RAO_STAGE_LABELS[
					traceEvent.catalog_id as keyof typeof GOLDBERG_RAO_STAGE_LABELS
				];
	if (
		traceEvent !== undefined &&
		goldbergStage !== undefined &&
		traceEvent.detail?.label === goldbergStage.label
	) {
		const label = `${goldbergStage.stage} · ${goldbergStage.detail} ${traceEvent.detail.value}`;
		const width = Math.max(160, Math.min(410, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-goldberg-rao-stage-badge"
				data-goldberg-rao-stage={traceEvent.catalog_id}
				data-goldberg-rao-detail={`${traceEvent.detail.label}:${traceEvent.detail.value}`}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	if (traceEvent !== undefined && HASSIN_STAGE_IDS.has(traceEvent.catalog_id)) {
		const dualFaces = state.context.metrics[5] ?? "0";
		const settledFaces = state.context.metrics[15] ?? "0";
		const label = (() => {
			switch (traceEvent.catalog_id) {
				case "hassin-st-planar.split-outer-face":
					return `SPLIT OUTER FACE · ${dualFaces} DUAL FACES`;
				case "hassin-st-planar.settle-dual-face":
					return `SETTLE DUAL FACE ${settledFaces}/${dualFaces} · DIST ${traceEvent.detail?.value ?? "—"}`;
				case "hassin-st-planar.reconstruct-primal-flow":
					return `RECONSTRUCT PRIMAL FLOW · ${state.context.metrics[11] ?? "0"} POSITIVE EDGES`;
				case "hassin-st-planar.optimal-dual-cut":
					return `DUAL CUT CERTIFIED · VALUE ${traceEvent.detail?.value ?? "—"}`;
				default:
					throw new Error("Unreachable Hassin stage");
			}
		})();
		const width = Math.max(160, Math.min(410, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-hassin-stage-badge"
				data-hassin-stage={traceEvent.catalog_id}
				data-hassin-detail={
					traceEvent.detail === undefined
						? undefined
						: `${traceEvent.detail.label}:${traceEvent.detail.value}`
				}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const refineStage =
		traceEvent === undefined
			? undefined
			: COST_SCALING_REFINE_STAGE_LABELS[
					traceEvent.catalog_id as keyof typeof COST_SCALING_REFINE_STAGE_LABELS
				];
	if (
		refineStage !== undefined &&
		traceEvent?.detail?.label === "epsilon" &&
		traceEvent.detail.value !== undefined
	) {
		const epsilon = traceEvent.detail.value;
		const label = `${refineStage} · ε ${epsilon}`;
		const width = Math.max(160, Math.min(390, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-cost-refine-stage-badge"
				data-cost-refine-stage={traceEvent.catalog_id}
				data-cost-refine-epsilon={epsilon}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const priceCatalogId = traceEvent?.catalog_id;
	const priceStage =
		priceCatalogId === undefined
			? undefined
			: PRICE_REFINEMENT_STAGE_LABELS[
					priceCatalogId as keyof typeof PRICE_REFINEMENT_STAGE_LABELS
				];
	const priceDetail = traceEvent?.detail;
	const priceDetailLabel =
		priceDetail === undefined
			? undefined
			: PRICE_REFINEMENT_DETAIL_LABELS[
					priceDetail.label as keyof typeof PRICE_REFINEMENT_DETAIL_LABELS
				];
	if (
		priceCatalogId !== undefined &&
		priceStage !== undefined &&
		priceDetail !== undefined &&
		priceDetailLabel !== undefined
	) {
		const label = `${priceStage} · ${priceDetailLabel} ${priceDetail.value}`;
		const width = Math.max(160, Math.min(430, label.length * 6.4 + 22));
		return (
			<g
				className="flow-overlay-stage-badge flow-price-refinement-stage-badge"
				data-price-refinement-stage={priceCatalogId}
				data-price-refinement-detail={`${priceDetail.label}:${priceDetail.value}`}
				transform="translate(14 14)"
			>
				<rect width={width} height="25" rx="6" />
				<text x="11" y="12.5" dominantBaseline="central">
					{label}
				</text>
			</g>
		);
	}
	const scalingStage =
		traceEvent === undefined
			? undefined
			: SCALING_PHASE_STAGE_LABELS[
					traceEvent.catalog_id as keyof typeof SCALING_PHASE_STAGE_LABELS
				];
	if (
		scalingStage === undefined ||
		traceEvent?.detail?.label !== "scale" ||
		traceEvent.detail.value === undefined
	)
		return null;
	const scale = traceEvent.detail.value;
	const label = `${scalingStage} · Δ ${scale}`;
	const width = Math.max(160, Math.min(390, label.length * 6.4 + 22));
	return (
		<g
			className="flow-overlay-stage-badge flow-scaling-stage-badge"
			data-scaling-stage={traceEvent.catalog_id}
			data-scaling-scale={scale}
			transform="translate(14 14)"
		>
			<rect width={width} height="25" rx="6" />
			<text x="11" y="12.5" dominantBaseline="central">
				{label}
			</text>
		</g>
	);
}
