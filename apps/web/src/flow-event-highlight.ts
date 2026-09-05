import type { FlowEntityRenderContext, FlowLod } from "./flow-render-plan";

/**
 * Returns exactly the identities published by the current source event.
 * Rendering code must not widen this set from aggregate counters or inferred
 * candidate sets, because doing so makes untouched graph entities flicker.
 */
export function ordinaryFlowEventEntityRefs(
	context: FlowEntityRenderContext,
): NonNullable<FlowEntityRenderContext["traceEvent"]>["entity_refs"] {
	return context.traceEvent?.entity_refs ?? [];
}

export type FlowAuxiliaryCellFocus = Readonly<{
	kind: "laplacian" | "assignment";
	rowNodeId: string;
	columnNodeId: string;
	completed: string;
	total: string;
}>;

export type FlowCapacityScalingPhaseBoundary = Readonly<{
	variant: "capacity" | "excess";
	boundary: "start" | "complete";
	scale: bigint;
	scaleLabel: string;
}>;

const CAPACITY_SCALING_PHASE_BOUNDARIES = Object.freeze({
	"capacity-scaling-mcf.start-scaling-phase": {
		variant: "capacity",
		boundary: "start",
	},
	"capacity-scaling-mcf.complete-scaling-phase": {
		variant: "capacity",
		boundary: "complete",
	},
	"excess-scaling-mcf.start-excess-phase": {
		variant: "excess",
		boundary: "start",
	},
	"excess-scaling-mcf.complete-excess-phase": {
		variant: "excess",
		boundary: "complete",
	},
} as const);

/**
 * Returns an exact capacity/excess-scaling phase boundary and its published Δ.
 * A matching producer event with an invalid scale is a contract violation and
 * must surface instead of silently dropping the graph feature.
 */
export function flowCapacityScalingPhaseBoundary(
	context: FlowEntityRenderContext,
): FlowCapacityScalingPhaseBoundary | undefined {
	const event = context.traceEvent;
	if (event === undefined) return undefined;
	const descriptor =
		CAPACITY_SCALING_PHASE_BOUNDARIES[
			event.catalog_id as keyof typeof CAPACITY_SCALING_PHASE_BOUNDARIES
		];
	if (descriptor === undefined) return undefined;
	if (event.detail?.label !== "scale") {
		throw new Error(`${event.catalog_id} did not publish its exact scale`);
	}
	const scale = BigInt(event.detail.value);
	if (scale <= 0n) {
		throw new Error(`${event.catalog_id} published a nonpositive scale`);
	}
	return {
		...descriptor,
		scale,
		scaleLabel: event.detail.value,
	};
}

/**
 * Projects one source-published auxiliary matrix cell onto its row and column
 * vertices.
 *
 * This is intentionally a closed producer-specific projection. Two unrelated
 * graph vertices must never become a generic Micro focus merely because an
 * algorithm happened to publish both of them.
 */
export function flowAuxiliaryCellFocus(
	context: FlowEntityRenderContext,
): FlowAuxiliaryCellFocus | undefined {
	const event = context.traceEvent;
	const kind = (() => {
		if (
			event?.catalog_id === "electrical-flow.matrix-scalar-product" &&
			event.detail?.label === "matrix scalar products"
		) {
			return "laplacian" as const;
		}
		if (
			event?.catalog_id ===
				"relaxed-most-negative-cycle.inspect-assignment-cell" &&
			event.detail?.label === "assignment cell scan"
		) {
			return "assignment" as const;
		}
		if (
			event?.catalog_id === "hungarian.inspect-cell" &&
			event.detail?.label === "cell-scans"
		) {
			return "assignment" as const;
		}
		return undefined;
	})();
	if (event === undefined || kind === undefined) return undefined;
	const detail = event.detail;
	if (detail === undefined) return undefined;
	const nodes = event.entity_refs.flatMap((entity) =>
		entity.kind === "node" ? [entity.node_id] : [],
	);
	if (
		nodes.length === 0 ||
		nodes.length > 2 ||
		nodes.length !== event.entity_refs.length
	) {
		return undefined;
	}
	const coordinates = (() => {
		if (event.catalog_id !== "hungarian.inspect-cell") {
			const row = nodes[0];
			return row === undefined ? undefined : ([row, nodes[1] ?? row] as const);
		}
		const model = context.model;
		if (model.kind !== "assignment") return undefined;
		const rows = nodes.filter((node) => model.agents.includes(node));
		const columns = nodes.filter((node) => model.tasks.includes(node));
		const row = rows[0];
		const column = columns[0];
		return rows.length === 1 &&
			columns.length === 1 &&
			row !== undefined &&
			column !== undefined
			? ([row, column] as const)
			: undefined;
	})();
	if (coordinates === undefined) return undefined;
	const total = context.traceEventSemantics?.work_progress.primary_total;
	if (total === undefined) return undefined;
	return {
		kind,
		rowNodeId: coordinates[0],
		columnNodeId: coordinates[1],
		completed: detail.value,
		total,
	};
}

export type FlowPrimitiveArcInspection = Readonly<{
	caption: string;
	completed: string;
	total: string;
	target: Extract<
		NonNullable<FlowEntityRenderContext["traceEvent"]>["entity_refs"][number],
		{ kind: "edge" | "residual-arc" }
	>;
}>;

function isInspectionBoundary(
	catalogId: string,
	pseudocodeLine: string,
): boolean {
	return /(?:^|[.:-])(inspect|scan)(?:[.:-]|$)/.test(
		`${catalogId}:${pseudocodeLine}`,
	);
}

/**
 * Projects one counted, source-published arc inspection onto its exact target.
 *
 * The global primary-work position is used instead of inventing a display-only
 * ordinal. This makes repeated visits to the same arc visibly distinct while
 * retaining the implementation's measured complexity counter.
 */
export function flowPrimitiveArcInspection(
	context: FlowEntityRenderContext,
): FlowPrimitiveArcInspection | undefined {
	const event = context.traceEvent;
	const semantics = context.traceEventSemantics;
	if (
		event === undefined ||
		semantics === undefined ||
		(semantics.role !== "select" && semantics.role !== "mutate") ||
		semantics.primary_work_block === undefined ||
		event.entity_refs.length !== 1 ||
		!isInspectionBoundary(event.catalog_id, event.pseudocode_line)
	) {
		return undefined;
	}
	const target = event.entity_refs[0];
	if (target === undefined || target.kind === "node") return undefined;
	const completed = semantics.work_progress.primary_completed;
	const total = semantics.work_progress.primary_total;
	let range = completed;
	try {
		const completedValue = BigInt(completed);
		const blockFirst = BigInt(semantics.primary_work_block.first);
		const blockLast = BigInt(semantics.primary_work_block.last);
		const deltaValue = blockLast - blockFirst + 1n;
		if (deltaValue <= 0n) return undefined;
		const first = completedValue - deltaValue + 1n;
		if (first < completedValue) range = `${first}–${completedValue}`;
	} catch {
		return undefined;
	}
	const direction =
		target.kind === "residual-arc"
			? ` · ${target.direction === "forward" ? "FWD" : "REV"}`
			: "";
	return {
		caption: `SCAN ${range}/${total}${direction}`,
		completed,
		total,
		target,
	};
}

/**
 * Structure views reserve per-entity rings for genuinely local actions.
 * Graph-wide source state remains available in the typed overlay and Inspector;
 * painting every member as a local change would erase the active primitive.
 */
export type FlowEventEntityEmphasisRequest = Readonly<{
	level: FlowLod;
	kind: "node" | "edge";
	signal: "touch" | "change";
	memberCount: number;
	totalCount: number;
	structureLimit: number;
}>;

/**
 * Decides whether exact event membership should also receive a vivid ring.
 *
 * `data-event-*` always retains the complete producer-published set.  The
 * painted ring is intentionally stricter: a BFS level, component, or batch
 * update covering most of the graph is global state, not a local primitive.
 * Broad edge touches remain paintable because they can be the path/cycle that
 * the event is explicitly asking the reader to follow.
 */
export function shouldRenderFlowEventEntityEmphasis({
	level,
	kind,
	signal,
	memberCount,
	totalCount,
	structureLimit,
}: FlowEventEntityEmphasisRequest): boolean {
	if (memberCount <= 0 || totalCount <= 0 || memberCount > structureLimit) {
		return false;
	}
	if (memberCount === 1) return true;
	if (memberCount >= totalCount) return false;
	if (kind === "edge" && signal === "touch") return true;

	const localityCap =
		kind === "node"
			? signal === "touch"
				? level === "detail"
					? 4
					: 3
				: level === "detail"
					? 3
					: 2
			: level === "detail"
				? 4
				: 3;
	return memberCount <= localityCap && memberCount * 3 <= totalCount;
}

const POLYNOMIAL_PRIMAL_SCAN_CAPTIONS = Object.freeze({
	"scale scan ordinal": "SCALE",
	"admissible scan ordinal": "ADMIT",
	"cycle scan ordinal": "CYCLE",
	"optimality scan ordinal": "OPT",
});

export type FlowPolynomialPrimalScan = Readonly<{
	caption: string;
	ordinal: string;
	target: NonNullable<
		FlowEntityRenderContext["traceEvent"]
	>["entity_refs"][number];
}>;

/** Projects one source-published extended-arc inspection onto its exact target. */
export function flowPolynomialPrimalScan(
	context: FlowEntityRenderContext,
): FlowPolynomialPrimalScan | undefined {
	const event = context.traceEvent;
	if (
		event?.catalog_id !==
			"polynomial-primal-network-simplex.inspect-extended-arc" ||
		event.detail === undefined
	) {
		return undefined;
	}
	const sourceLabel = Object.keys(POLYNOMIAL_PRIMAL_SCAN_CAPTIONS).find(
		(label) =>
			event.detail?.label === label ||
			event.detail?.label.includes(
				` · ${label} ${event.detail.value} · units `,
			),
	) as keyof typeof POLYNOMIAL_PRIMAL_SCAN_CAPTIONS | undefined;
	const target = event.entity_refs[0];
	if (sourceLabel === undefined || target === undefined) return undefined;
	return {
		caption: `${POLYNOMIAL_PRIMAL_SCAN_CAPTIONS[sourceLabel]} · #${event.detail.value}`,
		ordinal: event.detail.value,
		target,
	};
}

const MINIMUM_MEAN_SCAN_CAPTIONS = Object.freeze({
	"residual-inventory scan ordinal": "RESIDUAL SET",
	"karp-dp scan ordinal": "KARP DP",
	"tight-potential scan ordinal": "POTENTIAL",
	"tight-arc scan ordinal": "TIGHT ARC",
});

export type FlowMinimumMeanResidualScan = Readonly<{
	caption: string;
	ordinal: string;
	target: Extract<
		NonNullable<FlowEntityRenderContext["traceEvent"]>["entity_refs"][number],
		{ kind: "residual-arc" }
	>;
}>;

/** Anchors a repeated Karp-selector visit and its exact source ordinal. */
export function flowMinimumMeanResidualScan(
	context: FlowEntityRenderContext,
): FlowMinimumMeanResidualScan | undefined {
	const event = context.traceEvent;
	if (
		event?.catalog_id !== "minimum-mean-cycle-canceling.inspect-residual-arc" ||
		event.detail === undefined
	) {
		return undefined;
	}
	const sourceLabel = Object.keys(MINIMUM_MEAN_SCAN_CAPTIONS).find(
		(label) =>
			event.detail?.label === label ||
			event.detail?.label.includes(
				` · ${label} ${event.detail.value} · units `,
			),
	) as keyof typeof MINIMUM_MEAN_SCAN_CAPTIONS | undefined;
	const target = event.entity_refs[0];
	if (
		sourceLabel === undefined ||
		target === undefined ||
		target.kind !== "residual-arc"
	) {
		return undefined;
	}
	return {
		caption: `${MINIMUM_MEAN_SCAN_CAPTIONS[sourceLabel]} · #${event.detail.value} · ${target.direction === "forward" ? "FWD" : "REV"}`,
		ordinal: event.detail.value,
		target,
	};
}

const RELAXATION_SCAN_CAPTIONS = Object.freeze({
	"relaxation.scan-balanced-arcs": "BALANCED",
	"relaxation.scan-boundary-flow-arc": "BOUND FLOW",
	"relaxation.scan-price-cut-arc": "PRICE CUT",
});

export type FlowRelaxationArcScan = Readonly<{
	caption: string;
	ordinal: string;
	target: Extract<
		NonNullable<FlowEntityRenderContext["traceEvent"]>["entity_refs"][number],
		{ kind: "edge" | "residual-arc" }
	>;
}>;

/** Anchors one ordinary-network relaxation scan to the exact inspected arc. */
export function flowRelaxationArcScan(
	context: FlowEntityRenderContext,
): FlowRelaxationArcScan | undefined {
	const event = context.traceEvent;
	if (event === undefined || event.detail === undefined) return undefined;
	const caption =
		RELAXATION_SCAN_CAPTIONS[
			event.catalog_id as keyof typeof RELAXATION_SCAN_CAPTIONS
		];
	const target = event.entity_refs[0];
	if (
		caption === undefined ||
		!event.detail.label.includes("scan-ordinal") ||
		target === undefined ||
		target.kind === "node"
	) {
		return undefined;
	}
	const direction =
		target.kind === "residual-arc"
			? ` · ${target.direction === "forward" ? "FWD" : "REV"}`
			: "";
	return {
		caption: `${caption} · #${event.detail.value}${direction}`,
		ordinal: event.detail.value,
		target,
	};
}
