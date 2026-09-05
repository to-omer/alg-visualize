import { describe, expect, it } from "vitest";

import { buildFlowGenerationResultSummary } from "./flow-generation-result";
import {
	flowWorkbenchModelRejectionMessage,
	flowWorkbenchPolicy,
} from "./flow-workbench-policy";

const stats = {
	node_count: 8,
	edge_count: 13,
	minimum_capacity: "1",
	maximum_capacity: "8",
	minimum_cost: "-3",
	maximum_cost: "5",
} as const;

function scenario(overrides: Record<string, unknown> = {}): string {
	return JSON.stringify({
		plugin: "flow",
		payload: {
			algorithm: { id: "dinic", config: {} },
			model: { kind: "max-flow", source: "s", sink: "t" },
			graph: {
				nodes: Array.from({ length: 8 }, (_, index) => ({
					id: index === 0 ? "s" : index === 7 ? "t" : `n${index}`,
				})),
				edges: [
					{ capacity: "1", cost: "-3" },
					{ capacity: "2", cost: "0" },
					{ capacity: "3", cost: "1" },
					{ capacity: "4", cost: "2" },
					{ capacity: "5", cost: "3" },
					{ capacity: "6", cost: "4" },
					{ capacity: "7", cost: "5" },
					{ capacity: "8", cost: "0" },
					{ capacity: "1", cost: "0" },
					{ capacity: "2", cost: "0" },
					{ capacity: "3", cost: "0" },
					{ capacity: "4", cost: "0" },
					{ capacity: "5", cost: "0" },
				],
			},
			generator_provenance: {
				family_id: "dinic-worst-case",
				seed: "42",
				materialized_sha256: "a".repeat(64),
				difficulty: "verified-worst-case",
				...overrides,
			},
		},
	});
}

describe("flow generation result summary", () => {
	it("extracts the exact realized range and reproducibility fields", () => {
		expect(buildFlowGenerationResultSummary(scenario(), stats)).toEqual({
			algorithmId: "dinic",
			difficulty: "verified-worst-case",
			digest: "a".repeat(64),
			familyId: "dinic-worst-case",
			modelKind: "max-flow",
			seed: "42",
			stats,
		});
	});

	it("accepts explicit fixed-flow and legacy max-flow min-cost targets", () => {
		const decoded = JSON.parse(scenario()) as {
			payload: { model: Record<string, unknown> };
		};
		decoded.payload.model = {
			kind: "fixed-flow-min-cost",
			source: "s",
			sink: "t",
			required_flow: "8",
		};
		expect(
			buildFlowGenerationResultSummary(JSON.stringify(decoded), stats)
				.modelKind,
		).toBe("fixed-flow-min-cost");
		decoded.payload.model = {
			kind: "min-cost-max-flow",
			source: "s",
			sink: "t",
		};
		expect(
			buildFlowGenerationResultSummary(JSON.stringify(decoded), stats)
				.modelKind,
		).toBe("min-cost-max-flow");
	});

	it("rejects a generated result at the opposite workspace ingestion boundary", () => {
		const summary = buildFlowGenerationResultSummary(scenario(), stats);
		expect(
			flowWorkbenchModelRejectionMessage(
				flowWorkbenchPolicy("min-cost-flow"),
				summary.modelKind,
			),
		).toBe("This input belongs in the Max Flow workspace.");
	});

	it("fails closed for malformed provenance or inconsistent ranges", () => {
		expect(() =>
			buildFlowGenerationResultSummary(scenario({ seed: "042" }), stats),
		).toThrow(/provenance/);
		expect(() =>
			buildFlowGenerationResultSummary(scenario(), {
				...stats,
				minimum_cost: "6",
			}),
		).toThrow(/summary/);
		expect(() => buildFlowGenerationResultSummary("{", stats)).toThrow(
			/not valid JSON/,
		);
	});

	it("rejects statistics that do not describe the materialized graph", () => {
		expect(() =>
			buildFlowGenerationResultSummary(scenario(), {
				...stats,
				edge_count: 12,
			}),
		).toThrow(/do not match/);
		expect(() =>
			buildFlowGenerationResultSummary(scenario(), {
				...stats,
				maximum_capacity: "9",
			}),
		).toThrow(/do not match/);
	});
});
