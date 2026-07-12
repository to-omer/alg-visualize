import { describe, expect, it } from "vitest";

import type { TraceEvent } from "./engine-types";
import { eventActiveKey, traceDescription, visibleValue } from "./pedagogy";

describe("pedagogy presentation", () => {
	it("escapes line and bidi controls without changing ordinary Unicode", () => {
		expect(visibleValue("A\n\u202E🦀")).toBe("A\\u{A}\\u{202E}🦀");
	});

	it("keeps stable English identifiers inside Japanese explanations", () => {
		const event: TraceEvent = {
			catalog_id: 4,
			kind: "overwrite",
			node: null,
			target: null,
			entry: { index: 3, generation: 1 },
			key: "7",
			patch_start: 0,
			patch_count: 0,
		};
		const description = traceDescription(event);

		expect(description.title).toBe("Overwrite 7");
		expect(description.detail).toContain("EntryId");
		expect(description.detail).toContain("value payload");
	});

	it("does not select an active node when the serialized Rust Option is null", () => {
		const event: TraceEvent = {
			catalog_id: 9,
			kind: "result",
			node: null,
			target: null,
			entry: null,
			key: "12",
			patch_start: 0,
			patch_count: 0,
		};

		expect(eventActiveKey(event)).toBeUndefined();
	});

	it("does not render a serialized null key as user-facing text", () => {
		const event: TraceEvent = {
			catalog_id: 6,
			kind: "rotate-left",
			node: { kind: "auxiliary", id: { index: 2, generation: 0 } },
			target: null,
			entry: null,
			key: null,
			patch_start: 0,
			patch_count: 0,
		};

		expect(traceDescription(event)).toEqual({
			title: "Rotate left",
			detail: "right child を親の位置へ上げ、BST 順序を保って再接続します。",
		});
		expect(eventActiveKey(event)).toBe("auxiliary:2:0");
	});
});
