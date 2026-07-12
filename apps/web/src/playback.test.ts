import { describe, expect, it } from "vitest";

import type { StatePatchRecord, TraceEvent } from "./engine-types";
import { cursorForRawEvent, PLAYBACK_SPEEDS, tracePositions } from "./playback";

function event(catalog_id: number, kind: TraceEvent["kind"]): TraceEvent {
	return {
		catalog_id,
		kind,
		node: null,
		target: null,
		entry: null,
		key: null,
		patch_start: 0,
		patch_count: 0,
	};
}

const trace: TraceEvent[] = [
	event(1, "compare"),
	event(2, "descend"),
	event(1, "compare"),
	event(6, "rotate-left"),
	event(8, "update-metadata"),
	event(8, "update-metadata"),
	event(9, "result"),
];

describe("playback trace positions", () => {
	it("offers inspection through fast-forward playback speeds", () => {
		expect(PLAYBACK_SPEEDS).toEqual([0.25, 0.5, 1, 2, 4, 8, 16, 32]);
	});

	it("folds link traversal into the destination step at atomic granularity", () => {
		expect(tracePositions(trace, "atomic")).toEqual([0, 2, 3, 4, 5, 6]);
	});

	it("preserves each comparison while collapsing traversal and metadata work", () => {
		expect(tracePositions(trace, "semantic")).toEqual([0, 2, 3, 5, 6]);
	});

	it("never collapses an intermediate projection change", () => {
		const structural = [
			{ ...event(6, "rotate-left"), patch_start: 0, patch_count: 1 },
			{ ...event(6, "rotate-left"), patch_start: 1, patch_count: 1 },
			{ ...event(9, "result"), patch_start: 2 },
		];
		const patches: StatePatchRecord[] = [
			{ kind: "root", before: null, after: null },
			{ kind: "root", before: null, after: null },
		];

		expect(tracePositions(structural, "semantic", patches)).toEqual([0, 1, 2]);
	});

	it("maps granularity changes by raw event instead of reusing an ordinal", () => {
		const atomic = tracePositions(trace, "atomic");
		const semantic = tracePositions(trace, "semantic");

		expect(cursorForRawEvent(atomic, 5)).toBe(4);
		expect(cursorForRawEvent(semantic, 4)).toBe(2);
		expect(semantic[cursorForRawEvent(semantic, 4)]).toBe(3);
	});
});
