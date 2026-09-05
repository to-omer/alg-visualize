import { describe, expect, it } from "vitest";
import { flowEventCaption } from "./flow-event-caption";
import type { FlowCurrentSceneV9 } from "./flow-scene";

function scene(
	catalogId: string,
	detail?: { label: string; value: string },
	granularity: "phase" | "operation" | "micro" = "operation",
): FlowCurrentSceneV9 {
	return {
		trace_event: {
			event_id: "1",
			catalog_id: catalogId,
			minimum_granularity: granularity,
			pseudocode_line: "test",
			patch_count: 1,
			entity_refs: [
				{ kind: "residual-arc", edge_id: "sa", direction: "forward" },
			],
			...(detail === undefined ? {} : { detail }),
		},
		residual_arcs: [
			{
				edge_id: "sa",
				direction: "forward",
				from: "s",
				to: "a",
				capacity: "4",
				cost: "2",
				active: true,
				fixed: false,
			},
		],
	} as FlowCurrentSceneV9;
}

describe("flow event captions", () => {
	it("describes the concrete edge instead of exposing a catalog ID", () => {
		expect(flowEventCaption(scene("edmonds-karp.inspect-residual-arc"))).toBe(
			"Inspect residual edge s → a",
		);
	});

	it("names the visually active residual instead of an earlier touched arc", () => {
		const value = scene("edmonds-karp.inspect-residual-arc");
		value.trace_event?.entity_refs.unshift({
			kind: "residual-arc",
			edge_id: "older",
			direction: "forward",
		});
		value.residual_arcs.unshift({
			edge_id: "older",
			direction: "forward",
			from: "b",
			to: "c",
			capacity: "2",
			cost: "0",
			active: false,
			fixed: false,
		});
		expect(flowEventCaption(value)).toBe("Inspect residual edge s → a");
	});

	it("separates a precommit bottleneck from a committed update", () => {
		expect(
			flowEventCaption(
				scene("successive-shortest-path.bottleneck", {
					label: "bottleneck",
					value: "4",
				}),
			),
		).toBe("Bottleneck = 4");
		expect(
			flowEventCaption(
				scene("successive-shortest-path.augment", {
					label: "amount",
					value: "4",
				}),
			),
		).toBe("Commit +4 flow");
	});

	it("describes every path-construction boundary as Detail work", () => {
		const detail = { label: "path-edges", value: "2" };
		expect(
			flowEventCaption(scene("edmonds-karp.reconstruct-path", detail, "micro")),
		).toBe("Build path prefix · 2 edges");
		expect(
			flowEventCaption(scene("edmonds-karp.reconstruct-path", detail)),
		).toBe("Build path prefix · 2 edges");
	});

	it("keeps source-specific stages and their measurements visible", () => {
		const value = scene(
			"weighted-augmenting-paths.relabel-sweep",
			{ label: "relabel jumps", value: "12" },
			"operation",
		);
		value.trace_event_semantics = {
			role: "mutate",
			aggregation_count: "12",
			work_deltas: [{ unit: "published-transition", count: "1" }],
			work_progress: {
				detail_completed: "0",
				detail_total: "0",
				primary_completed: "12",
				primary_total: "12",
			},
			changed_entity_refs: [],
		};
		expect(flowEventCaption(value)).toBe(
			"Update · Relabel sweep · Relabel jumps = 12",
		);
	});

	it("describes feasibility construction and local auxiliary work directly", () => {
		const construction = scene(
			"feasibility.add-original-arc",
			{ label: "capacity", value: "4" },
			"phase",
		);
		construction.feasibility_overlay = {
			stage: "add-original-arc",
			routed: "0",
			total_required: "0",
			arcs: [
				{
					from: { kind: "original", original_node_id: "s" },
					to: { kind: "original", original_node_id: "a" },
					focused: true,
					focused_direction: "forward",
				},
			],
		} as never;
		expect(flowEventCaption(construction)).toBe(
			"Shift lower bound on s → a · residual capacity 4",
		);

		const push = scene(
			"feasibility.push",
			{ label: "amount", value: "3" },
			"micro",
		);
		push.feasibility_overlay = {
			stage: "push",
			routed: "0",
			total_required: "5",
			arcs: [
				{
					from: { kind: "super-source" },
					to: { kind: "original", original_node_id: "s" },
					focused: true,
					focused_direction: "forward",
				},
			],
		} as never;
		expect(flowEventCaption(push)).toBe(
			"Push 3 on auxiliary residual arc SS → s",
		);
	});

	it("presents weighted push-relabel checkpoints as graph actions", () => {
		expect(
			flowEventCaption(
				scene(
					"weighted-push-relabel.inspect-primitive-arc-checkpoint",
					{ label: "bidirectional inspection checkpoint", value: "37" },
					"micro",
				),
			),
		).toBe("Inspect residual edge s → a");

		const relabel = scene(
			"weighted-push-relabel.relabel-checkpoint",
			{ label: "relabel checkpoint", value: "9" },
			"micro",
		);
		relabel.trace_event?.entity_refs.splice(0, 1, {
			kind: "node",
			node_id: "a",
		});
		expect(flowEventCaption(relabel)).toBe("Relabel vertex a");

		expect(
			flowEventCaption(
				scene("weighted-push-relabel.complete-residual-rounds", {
					label: "residual rounds",
					value: "2",
				}),
			),
		).toBe("Complete exact residual flow · 2 rounds");
	});

	it("does not collapse different framework stages into one algorithm caption", () => {
		const query = scene(
			"deterministic-almost-linear-mcf.query-minimum-ratio-cycle",
			{ label: "minimum-ratio cycle", value: "3" },
			"phase",
		);
		const progress = scene(
			"deterministic-almost-linear-mcf.source-progress",
			{ label: "source progress", value: "3" },
			"phase",
		);
		expect(flowEventCaption(query)).not.toBe(flowEventCaption(progress));
		expect(flowEventCaption(query)).toContain("Query minimum ratio cycle");
		expect(flowEventCaption(progress)).toContain("Source progress");
	});
});
