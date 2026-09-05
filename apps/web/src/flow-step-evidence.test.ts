import { describe, expect, it } from "vitest";
import type { FlowCurrentSceneV9 } from "./flow-scene";
import { projectFlowStepEvidence } from "./flow-step-evidence";

function scene(
	catalogId: string,
	abstraction: "primitive" | "iteration" | "oracle-call",
): FlowCurrentSceneV9 {
	return {
		graph: {
			nodes: [
				{ id: "s", position: { x: 0, y: 0 }, supply: "0" },
				{ id: "t", position: { x: 1, y: 0 }, supply: "0" },
			],
			edges: [
				{
					id: "st",
					from: "s",
					to: "t",
					capacity: "4",
					cost: "0",
				},
			],
		},
		trace_steps: {
			phase_unit: "one phase",
			phase_availability: { availability: "available" },
			operation_unit: "one operation",
			operation_availability: { availability: "available" },
			detail: { availability: "available", unit: "one source action" },
			primary_work: {
				metric_ordinal: 2,
				unit:
					abstraction === "oracle-call"
						? "cycle queries"
						: abstraction === "iteration"
							? "relabel sweeps"
							: "residual-arc inspections",
				abstraction,
				visualization:
					abstraction === "oracle-call"
						? "candidate-field"
						: abstraction === "iteration"
							? "numeric-field"
							: "edge-field",
			},
		},
		trace_event: {
			event_id: "4",
			catalog_id: catalogId,
			minimum_granularity: "micro",
			pseudocode_line: "inspect the selected residual structure",
			patch_count: 1,
			entity_refs:
				abstraction === "oracle-call"
					? []
					: [{ kind: "residual-arc", edge_id: "st", direction: "forward" }],
			detail: {
				label: "amount",
				value: "4",
			},
		},
		trace_event_semantics: {
			role: "commit",
			work_deltas: [
				{ unit: "published-transition", count: "1" },
				{ unit: "detail-primitive", count: "1" },
				{ unit: "primary-work", count: "3" },
			],
			aggregation_count: "3",
			work_progress: {
				detail_completed: "4",
				detail_total: "20",
				primary_completed: "6",
				primary_total: "12",
			},
			primary_work_block: { first: "1", last: "3", total: "3" },
			changed_entity_refs: [{ kind: "edge", edge_id: "st" }],
		},
		residual_arcs: [
			{
				edge_id: "st",
				direction: "forward",
				from: "s",
				to: "t",
				capacity: "4",
				cost: "0",
				active: true,
			},
		],
	} as unknown as FlowCurrentSceneV9;
}

describe("flow step evidence", () => {
	it("keeps aggregate iteration work on the source action", () => {
		const evidence = projectFlowStepEvidence(
			scene("weighted-augmenting-paths.relabel-sweep", "iteration"),
		);
		expect(evidence?.work).toBe(
			"3 relabel sweeps · units 1–3 of 3 · total 6/12",
		);
		expect(evidence?.focus).toBe("residual s → t");
		expect(evidence?.effect).toBe(
			"Publishes this source event after 3 measured work units",
		);
	});

	it("describes an oracle source action without inventing a graph entity", () => {
		const evidence = projectFlowStepEvidence(
			scene("minimum-ratio-cycle-mcf.evaluate-cycle", "oracle-call"),
		);
		expect(evidence?.focus).toBe("Oracle subproblem and returned witness");
		expect(evidence?.effect).toBe(
			"Publishes this source event after 3 measured work units",
		);
	});

	it("describes the exact effect of a source transition", () => {
		const evidence = projectFlowStepEvidence(
			scene("successive-shortest-path.augment", "primitive"),
		);
		expect(evidence?.action).toBe("Commit +4 flow");
		expect(evidence?.focus).toBe("residual s → t");
		expect(evidence?.observation).toBe("Amount = 4 · measured units 1–3 of 3");
		expect(evidence?.effect).toBe(
			"Publishes this source event after 3 measured work units",
		);
	});

	it("describes an exact observation without inventing a persistent effect", () => {
		const value = scene("edmonds-karp.inspect-residual-arc", "primitive");
		if (
			value.trace_event === undefined ||
			value.trace_event_semantics === undefined
		) {
			throw new Error("fixture source event missing");
		}
		value.trace_event.patch_count = 0;
		value.trace_event_semantics.role = "observe";
		value.trace_event_semantics.changed_entity_refs = [];
		value.trace_event_semantics.primary_work_block = {
			first: "1",
			last: "1",
			total: "3",
		};
		const primaryDelta = value.trace_event_semantics.work_deltas.find(
			(delta) => delta.unit === "primary-work",
		);
		if (primaryDelta === undefined) throw new Error("primary delta missing");
		primaryDelta.count = "1";
		value.trace_event_semantics.aggregation_count = "1";
		const evidence = projectFlowStepEvidence(value);
		expect(evidence?.action).toBe("Inspect residual edge s → t");
		expect(evidence?.focus).toBe("residual s → t");
		expect(evidence?.effect).toBe(
			"Completes 1 measured work unit without changing algorithm state",
		);
	});

	it("shows exact kernel work when a boundary has no primary-work delta", () => {
		const value = scene("feasibility.add-original-arc", "primitive");
		if (value.trace_event_semantics === undefined)
			throw new Error("fixture event semantics missing");
		value.trace_event_semantics.work_deltas = [
			{ unit: "published-transition", count: "1" },
			{ unit: "residual-arc-scan", count: "1" },
		];
		delete value.trace_event_semantics.primary_work_block;
		expect(projectFlowStepEvidence(value)?.work).toBe(
			"1 Residual Arc Scan · step 4/20",
		);
	});

	it("prefers exact kernel work to an enclosing Detail label", () => {
		const value = scene("feasibility.push", "primitive");
		if (value.trace_event_semantics === undefined)
			throw new Error("fixture event semantics missing");
		value.trace_event_semantics.work_deltas = [
			{ unit: "published-transition", count: "1" },
			{ unit: "detail-primitive", count: "1" },
			{ unit: "push", count: "1" },
		];
		delete value.trace_event_semantics.primary_work_block;
		expect(projectFlowStepEvidence(value)?.work).toBe("1 Push · step 4/20");
	});

	it("keeps catalog primary work ahead of a secondary exact counter", () => {
		const value = scene("feasibility.inspect-discharge-arc", "primitive");
		if (value.trace_event_semantics === undefined)
			throw new Error("fixture event semantics missing");
		value.trace_event_semantics.work_deltas.push({
			unit: "residual-arc-scan",
			count: "1",
		});
		expect(projectFlowStepEvidence(value)?.work).toBe(
			"3 residual-arc inspections · units 1–3 of 3 · total 6/12",
		);
	});

	it("explains a source event without a scalar measurement", () => {
		const value = scene("unit-capacity-dinic.blocking-flow", "iteration");
		if (value.trace_event === undefined)
			throw new Error("fixture event missing");
		delete value.trace_event.detail;
		expect(projectFlowStepEvidence(value)?.observation).toBe(
			"Current witness is highlighted in the graph",
		);
		value.trace_event.entity_refs = [];
		expect(projectFlowStepEvidence(value)?.observation).toBe(
			"Iteration result is shown in the algorithm state",
		);
	});

	it("shortens catalog-shaped pseudocode without rewriting real expressions", () => {
		const value = scene("successive-shortest-path.augment", "primitive");
		if (value.trace_event === undefined)
			throw new Error("fixture event missing");
		value.trace_event.pseudocode_line =
			"successive-shortest-path:commit-residual-path";
		expect(projectFlowStepEvidence(value)?.pseudocode).toBe(
			"commit_residual_path()",
		);
		value.trace_event.pseudocode_line = "while excess(v) > 0";
		expect(projectFlowStepEvidence(value)?.pseudocode).toBe(
			"while excess(v) > 0",
		);
	});

	it("does not fabricate evidence for the input boundary", () => {
		expect(projectFlowStepEvidence(undefined)).toBeUndefined();
		const input = scene("successive-shortest-path.augment", "primitive");
		delete input.trace_event;
		expect(projectFlowStepEvidence(input)).toBeUndefined();
	});
});
