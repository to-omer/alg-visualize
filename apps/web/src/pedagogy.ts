import { entityIdKey, type TraceEvent, type TraceKind } from "./engine-types";
import type { AlgorithmId } from "./scenario";

export const INVARIANTS: Record<AlgorithmId, string[]> = {
	avl: ["BST ordering", "Stored height", "|balance| ≤ 1"],
	wbt: ["BST ordering", "Stored subtree size", "Delta = 3 weight balance"],
	aa: ["BST ordering", "AA level rules", "No consecutive right horizontals"],
	llrb: ["BST ordering", "Left-leaning red links", "Equal black height"],
	treap: ["BST ordering", "Max-priority heap", "Stable priority on overwrite"],
	zip: ["BST ordering", "Max-rank heap", "Smaller key wins rank tie"],
	splay: ["BST ordering", "Stable entry identity", "Accessed node at root"],
	scapegoat: [
		"BST ordering",
		"Stored subtree size",
		"Exact rational threshold",
	],
	"skip-list": ["Ordered base level", "Ordered tower links", "Bounded height"],
	"b-tree": ["Sorted node entries", "Occupancy bounds", "Equal leaf depth"],
	veb: ["Min/max consistency", "Summary ↔ clusters", "Sparse materialization"],
	"x-fast": [
		"Prefix table coverage",
		"Ordered leaf list",
		"Jump range consistency",
	],
	"y-fast": [
		"Representative index",
		"Treap bucket heaps",
		"Sentinel remains structural",
	],
};

export const COMPLEXITY: Record<AlgorithmId, string> = {
	avl: "worst O(log n)",
	wbt: "worst O(log n)",
	aa: "worst O(log n)",
	llrb: "worst O(log n)",
	treap: "expected O(log n)",
	zip: "expected O(log n)",
	splay: "amortized O(log n)",
	scapegoat: "amortized O(log n)",
	"skip-list": "expected O(log n)",
	"b-tree": "O(t logₜ n) CPU",
	veb: "expected O(log log U)",
	"x-fast": "expected O(log log U) lookup",
	"y-fast": "expected O(log log U)",
};

export const PSEUDOCODE: Record<TraceKind, { line: number; code: string }> = {
	compare: { line: 3, code: "order ← compare(key, node.key)" },
	descend: { line: 4, code: "node ← child(node, order)" },
	insert: { line: 7, code: "attach(new_entry, search_slot)" },
	overwrite: { line: 8, code: "entry.value ← value" },
	remove: { line: 11, code: "detach(entry); repair(path)" },
	"rotate-left": { line: 14, code: "root ← rotate_left(root)" },
	"rotate-right": { line: 15, code: "root ← rotate_right(root)" },
	"update-metadata": { line: 17, code: "update_metadata(node)" },
	rebuild: { line: 20, code: "rebuild_in_order(subtree)" },
	split: { line: 23, code: "(left, pivot, right) ← split(node)" },
	merge: { line: 24, code: "node ← merge(left, right)" },
	"move-entry": { line: 26, code: "move(entry_id, destination)" },
	result: { line: 29, code: "return operation_result" },
};

export function traceDescription(event: TraceEvent | undefined): {
	title: string;
	detail: string;
} {
	if (event === undefined) {
		return {
			title: "Ready to inspect",
			detail:
				"Load a Scenario, then step or play to inspect its internal work.",
		};
	}
	const key = event.key == null ? "" : ` ${event.key}`;
	const descriptions: Record<TraceKind, [string, string]> = {
		compare: [
			`Compare${key}`,
			"Compare the search key with the current entry.",
		],
		descend: [
			`Follow a link${key}`,
			"Follow the structure link selected by the comparison.",
		],
		insert: [
			`Insert${key}`,
			"Create a stable EntryId and add its representation to live state.",
		],
		overwrite: [
			`Overwrite${key}`,
			"Keep the EntryId and random attributes; update only its value payload.",
		],
		remove: [
			`Remove${key}`,
			"Retire the logical entry and repair structural links and metadata.",
		],
		"rotate-left": [
			"Rotate left",
			"Promote the right child and reconnect it while preserving BST order.",
		],
		"rotate-right": [
			"Rotate right",
			"Promote the left child and reconnect it while preserving BST order.",
		],
		"update-metadata": [
			"Update metadata",
			"Synchronize derived height, size, level, color, and related metadata.",
		],
		rebuild: [
			"Rebuild subtree",
			"Rearrange into balanced order without changing NodeIds or EntryIds.",
		],
		split: [
			"Split structure",
			"Split one structural region without recreating EntryIds.",
		],
		merge: [
			"Merge structures",
			"Merge adjacent structural regions while preserving EntryIds.",
		],
		"move-entry": [
			"Move entry",
			"Move an existing EntryId to another physical node.",
		],
		result: [
			"Operation result",
			"Finalize the public result, such as a hit, miss, or previous value.",
		],
	};
	const [title, detail] = descriptions[event.kind];
	return { title, detail };
}

export function eventActiveKey(
	event: TraceEvent | undefined,
): string | undefined {
	return event === undefined || event.node === null
		? undefined
		: entityIdKey(event.node);
}

export function visibleValue(value: string | undefined): string {
	if (value === undefined) {
		return "—";
	}
	return [...value]
		.map((character) => {
			const code = character.codePointAt(0) ?? 0;
			if (
				code < 0x20 ||
				code === 0x7f ||
				(0x200b <= code && code <= 0x200f) ||
				(0x202a <= code && code <= 0x202e) ||
				(0x2060 <= code && code <= 0x2069)
			) {
				return `\\u{${code.toString(16).toUpperCase()}}`;
			}
			return character;
		})
		.join("");
}
