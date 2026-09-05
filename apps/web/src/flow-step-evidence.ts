import { flowEventCaption } from "./flow-event-caption";
import type { FlowCurrentSceneV9, FlowTraceEntityRefV1 } from "./flow-scene";

export type FlowStepEvidence = Readonly<{
	action: string;
	work: string;
	focus: string;
	observation: string;
	effect: string;
	pseudocode: string;
}>;

function titleCaseKebab(value: string): string {
	return value
		.split("-")
		.map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
		.join(" ");
}

function edgeEndpoints(
	scene: FlowCurrentSceneV9,
	entity: Extract<FlowTraceEntityRefV1, { kind: "edge" | "residual-arc" }>,
): string {
	const edge = scene.graph.edges.find(
		(candidate) => candidate.id === entity.edge_id,
	);
	if (entity.kind === "edge") {
		return edge === undefined
			? `edge ${entity.edge_id}`
			: `edge ${edge.from} → ${edge.to}`;
	}
	if (edge === undefined) {
		return `${entity.direction} residual ${entity.edge_id}`;
	}
	return entity.direction === "forward"
		? `residual ${edge.from} → ${edge.to}`
		: `reverse residual ${edge.to} → ${edge.from}`;
}

function focusLabel(
	scene: FlowCurrentSceneV9,
	entities: readonly FlowTraceEntityRefV1[],
): string {
	if (entities.length === 0) {
		const abstraction = scene.trace_steps.primary_work.abstraction;
		return abstraction === "oracle-call"
			? "Oracle subproblem and returned witness"
			: abstraction === "iteration"
				? "Algorithm working state"
				: "Current algorithm state";
	}
	const labels = [
		...new Set(
			entities.map((entity) =>
				entity.kind === "node"
					? `node ${entity.node_id}`
					: edgeEndpoints(scene, entity),
			),
		),
	];
	const visible = labels.slice(0, 3);
	const remaining = labels.length - visible.length;
	return `${visible.join(" · ")}${remaining > 0 ? ` · +${remaining} more` : ""}`;
}

function workLabel(scene: FlowCurrentSceneV9): string {
	const semantics = scene.trace_event_semantics;
	if (semantics === undefined) return "1 published transition";
	const primaryDelta = semantics.work_deltas.find(
		(delta) => delta.unit === "primary-work",
	);
	if (primaryDelta !== undefined) {
		const block = semantics.primary_work_block;
		return `${primaryDelta.count} ${scene.trace_steps.primary_work.unit}${
			block === undefined
				? ""
				: ` · units ${block.first}–${block.last} of ${block.total}`
		} · total ${semantics.work_progress.primary_completed}/${semantics.work_progress.primary_total}`;
	}
	const exactDelta = semantics.work_deltas.find(
		(delta) =>
			delta.unit !== "published-transition" &&
			delta.unit !== "detail-primitive" &&
			delta.unit !== "primary-work",
	);
	if (exactDelta !== undefined) {
		return `${exactDelta.count} ${titleCaseKebab(exactDelta.unit)} · step ${semantics.work_progress.detail_completed}/${semantics.work_progress.detail_total}`;
	}
	const detailDelta = semantics.work_deltas.find(
		(delta) => delta.unit === "detail-primitive",
	);
	if (detailDelta !== undefined) {
		const detailUnit =
			scene.trace_steps.detail.availability === "available"
				? scene.trace_steps.detail.unit
				: "Detail primitive";
		return `${detailDelta.count} ${detailUnit} · step ${semantics.work_progress.detail_completed}/${semantics.work_progress.detail_total}`;
	}
	return `1 published transition · step ${semantics.work_progress.detail_completed}/${semantics.work_progress.detail_total}`;
}

function observationLabel(scene: FlowCurrentSceneV9): string {
	const block = scene.trace_event_semantics?.primary_work_block;
	const detail = scene.trace_event?.detail;
	if (detail === undefined) {
		if ((scene.trace_event?.entity_refs.length ?? 0) > 0) {
			return "Current witness is highlighted in the graph";
		}
		switch (scene.trace_steps.primary_work.abstraction) {
			case "oracle-call":
				return "Oracle witness is shown in the algorithm state";
			case "iteration":
				return "Iteration result is shown in the algorithm state";
			case "primitive":
				return "Current result is shown in the algorithm state";
		}
	}
	const observation = `${titleCaseKebab(detail.label)} = ${detail.value}`;
	return block === undefined
		? observation
		: `${observation} · measured units ${block.first}–${block.last} of ${block.total}`;
}

function effectLabel(scene: FlowCurrentSceneV9): string {
	const semantics = scene.trace_event_semantics;
	if (semantics === undefined) return "No published effect";
	const localProgress = semantics.primary_work_block;
	const primaryDelta = semantics.work_deltas.find(
		(delta) => delta.unit === "primary-work",
	)?.count;
	if (localProgress !== undefined && primaryDelta !== undefined) {
		const measured = `${primaryDelta} measured work ${primaryDelta === "1" ? "unit" : "units"}`;
		if (semantics.role === "observe") {
			return `Completes ${measured} without changing algorithm state`;
		}
		if (semantics.role === "select") {
			return `Completes ${measured} and selects the highlighted structure`;
		}
		return `Publishes this source event after ${measured}`;
	}
	const changed = semantics.changed_entity_refs.length;
	switch (semantics.role) {
		case "observe":
			return "Reads state without changing the published scene";
		case "select":
			return "Selects the highlighted structure for the next decision";
		case "mutate":
			return changed === 0
				? "Changes algorithm working state"
				: `Changes working state and ${changed} graph ${changed === 1 ? "entity" : "entities"}`;
		case "commit":
			return changed === 0
				? "Commits the algorithm result"
				: `Commits flow changes on ${changed} graph ${changed === 1 ? "entity" : "entities"}`;
		case "certify":
			return "Publishes an independently checked terminal certificate";
	}
}

function pseudocodeLabel(line: string): string {
	const separator = line.lastIndexOf(":");
	const operation = separator < 0 ? line : line.slice(separator + 1);
	return /^[a-z][a-z0-9-]*$/u.test(operation)
		? `${operation.replaceAll("-", "_")}()`
		: line;
}

/** Projects one validated trace boundary into concise, user-visible evidence. */
export function projectFlowStepEvidence(
	scene: FlowCurrentSceneV9 | undefined,
): FlowStepEvidence | undefined {
	const event = scene?.trace_event;
	if (scene === undefined || event === undefined) return undefined;
	return {
		action: flowEventCaption(scene),
		work: workLabel(scene),
		focus: focusLabel(scene, event.entity_refs),
		observation: observationLabel(scene),
		effect: effectLabel(scene),
		pseudocode: pseudocodeLabel(event.pseudocode_line),
	};
}
