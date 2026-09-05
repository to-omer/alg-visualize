import { describe, expect, it } from "vitest";
import {
	buildGeneratedFlowScenario,
	type FlowGeneratedCandidate,
} from "./flow-generator-scenario";

const CANDIDATE: FlowGeneratedCandidate = {
	graph: { nodes: [{ id: "s" }, { id: "t" }], edges: [] },
	suggestedModel: { kind: "max-flow", source: "s", sink: "t" },
	provenance: { family_id: "path", materialized_sha256: "abc" },
};

function scenario() {
	return {
		schema_version: 1,
		plugin: "flow",
		payload: {
			algorithm: { id: "edmonds-karp", config: {} },
			run_profile: "trace",
			graph: { nodes: [], edges: [] },
			model: { kind: "max-flow", source: "old-s", sink: "old-t" },
			updates: [{ edge: "stale" }],
		},
	};
}

describe("generated flow Scenario construction", () => {
	for (const recommendedRunProfile of ["trace", "fast"] as const) {
		it(`applies the required ${recommendedRunProfile} run profile without mutating the existing Scenario`, () => {
			const existing = scenario();
			const before = structuredClone(existing);
			const generated = buildGeneratedFlowScenario(existing, CANDIDATE, {
				recommendedAlgorithmId: "dinic",
				recommendedRunProfile,
			});

			expect(existing).toEqual(before);
			expect(generated).not.toBe(existing);
			expect(generated.payload).not.toBe(existing.payload);
			expect(generated).toMatchObject({
				plugin: "flow",
				payload: {
					algorithm: { id: "dinic", config: {} },
					graph: CANDIDATE.graph,
					model: CANDIDATE.suggestedModel,
					run_profile: recommendedRunProfile,
					generator_provenance: CANDIDATE.provenance,
				},
			});
			expect(
				(generated.payload as Record<string, unknown>).updates,
			).toBeUndefined();
		});
	}

	it("keeps failure atomic when the run profile is invalid", () => {
		const existing = scenario();
		const before = structuredClone(existing);
		expect(() =>
			buildGeneratedFlowScenario(existing, CANDIDATE, {
				recommendedRunProfile: "result-only" as "trace",
			}),
		).toThrow(/trace or fast/);
		expect(existing).toEqual(before);
	});

	it("preserves model-specific algorithm selection when no recommendation is supplied", () => {
		const generated = buildGeneratedFlowScenario(
			scenario(),
			{
				...CANDIDATE,
				suggestedModel: { kind: "assignment" },
			},
			{ recommendedRunProfile: "trace" },
		);
		expect(generated).toMatchObject({
			payload: { algorithm: { id: "hungarian", config: {} } },
		});
	});

	it("selects a compatible starter algorithm when generation adapts the problem model", () => {
		const fixedFlowMinCost = buildGeneratedFlowScenario(
			{
				...scenario(),
				payload: {
					...scenario().payload,
					algorithm: { id: "hungarian", config: {} },
				},
			},
			{
				...CANDIDATE,
				suggestedModel: {
					kind: "fixed-flow-min-cost",
					source: "s",
					sink: "t",
					required_flow: "8",
				},
			},
			{ recommendedRunProfile: "trace" },
		);
		expect(fixedFlowMinCost).toMatchObject({
			payload: {
				algorithm: { id: "successive-shortest-path", config: {} },
			},
		});

		const minCostMaxFlow = buildGeneratedFlowScenario(
			scenario(),
			{
				...CANDIDATE,
				suggestedModel: {
					kind: "min-cost-max-flow",
					source: "s",
					sink: "t",
				},
			},
			{ recommendedRunProfile: "trace" },
		);
		expect(minCostMaxFlow).toMatchObject({
			payload: {
				algorithm: {
					id: "successive-shortest-augmenting-path",
					config: {},
				},
			},
		});

		const maxFlow = buildGeneratedFlowScenario(
			{
				...scenario(),
				payload: {
					...scenario().payload,
					algorithm: { id: "successive-shortest-path", config: {} },
				},
			},
			CANDIDATE,
			{ recommendedRunProfile: "trace" },
		);
		expect(maxFlow).toMatchObject({
			payload: { algorithm: { id: "edmonds-karp", config: {} } },
		});
	});

	it.each([
		["circulation", "simple-cycle-canceling"],
		["transshipment", "cost-scaling"],
	] as const)("rebuilds %s algorithm configuration for the generated node set", (modelKind, expectedAlgorithmId) => {
		const generated = buildGeneratedFlowScenario(
			{
				...scenario(),
				payload: {
					...scenario().payload,
					algorithm: {
						id: "tardos-framework",
						config: {
							potentials: { "old-s": "0", "old-t": "0" },
						},
					},
				},
			},
			{
				...CANDIDATE,
				graph: {
					nodes: [
						{ id: "new-a", supply: "3" },
						{ id: "new-b", supply: "0" },
						{ id: "new-c", supply: "-3" },
					],
					edges: [],
				},
				suggestedModel: { kind: modelKind },
			},
			{ recommendedRunProfile: "trace" },
		);

		expect(generated).toMatchObject({
			payload: {
				algorithm: { id: expectedAlgorithmId, config: {} },
				model: { kind: modelKind },
			},
		});
	});

	it.each([
		["goldberg-mesh-circulation", "circulation"],
		["gridgraph-grid", "transshipment"],
		["goto-torus", "transshipment"],
		["netgen-skeleton", "transshipment"],
	] as const)("uses the trace-admitted network-simplex default for %s", (familyId, modelKind) => {
		const generated = buildGeneratedFlowScenario(
			scenario(),
			{
				...CANDIDATE,
				provenance: { ...CANDIDATE.provenance, family_id: familyId },
				suggestedModel: { kind: modelKind },
			},
			{ recommendedRunProfile: "trace" },
		);

		expect(generated).toMatchObject({
			payload: {
				algorithm: { id: "primal-network-simplex", config: {} },
			},
		});
	});

	it("keeps the NETGEN network-simplex default scoped to native min-cost models", () => {
		const generated = buildGeneratedFlowScenario(
			scenario(),
			{
				...CANDIDATE,
				provenance: {
					...CANDIDATE.provenance,
					family_id: "netgen-skeleton",
				},
				suggestedModel: { kind: "max-flow", source: "s", sink: "t" },
			},
			{ recommendedRunProfile: "trace" },
		);

		expect(generated).toMatchObject({
			payload: { algorithm: { id: "edmonds-karp", config: {} } },
		});
	});
});
