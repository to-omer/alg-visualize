import type { StatePatchRecord, TraceEvent } from "./engine-types";

export type PlaybackGranularity = "semantic" | "atomic";

export const PLAYBACK_SPEEDS = [0.25, 0.5, 1, 2, 4, 8, 16, 32] as const;

export function cursorForRawEvent(
	positions: readonly number[],
	rawEventIndex: number,
): number {
	let closest = 0;
	for (
		let index = 1;
		index < positions.length &&
		(positions[index] ?? Number.POSITIVE_INFINITY) <= rawEventIndex;
		index += 1
	) {
		closest = index;
	}
	return closest;
}

export function tracePositions(
	trace: TraceEvent[] | undefined,
	granularity: PlaybackGranularity,
	patches: StatePatchRecord[] | undefined = undefined,
): number[] {
	if (trace === undefined || trace.length === 0) {
		return [];
	}
	if (granularity === "atomic") {
		return trace.flatMap((event, index) =>
			event.kind === "descend" ? [] : [index],
		);
	}
	const category = (event: TraceEvent) => {
		if (event.kind === "compare") {
			return "search";
		}
		if (event.kind === "update-metadata") {
			return "metadata";
		}
		return `${event.kind}:${event.catalog_id}`;
	};
	const positions: number[] = [];
	for (let index = 0; index < trace.length; index += 1) {
		const current = trace[index];
		const next = trace[index + 1];
		if (
			current !== undefined &&
			current.kind !== "descend" &&
			(current.kind === "compare" ||
				hasProjectionPatch(current, patches) ||
				next === undefined ||
				category(current) !== category(next))
		) {
			positions.push(index);
		}
	}
	return positions;
}

function hasProjectionPatch(
	event: TraceEvent,
	patches: StatePatchRecord[] | undefined,
): boolean {
	if (patches === undefined) {
		return false;
	}
	const end = event.patch_start + event.patch_count;
	for (let index = event.patch_start; index < end; index += 1) {
		const patch = patches[index];
		if (patch?.kind === "root" || patch?.kind === "node") {
			return true;
		}
	}
	return false;
}
