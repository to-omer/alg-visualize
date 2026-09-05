import { describe, expect, it } from "vitest";

import {
	decodeFlowGeneratorFixtureManifest,
	FLOW_GENERATOR_FAMILY_IDS,
	FLOW_GENERATOR_PICKER_GROUPS,
	filterFlowGeneratorFixtures,
	flowGeneratorFamilyModel,
	flowGeneratorFixtureKind,
} from "./flow-generator-fixture";

function fixture(familyId: (typeof FLOW_GENERATOR_FAMILY_IDS)[number]) {
	return {
		family_id: familyId,
		title: familyId,
		purpose: `purpose:${familyId}`,
		model: flowGeneratorFamilyModel(familyId),
		layout_class: "linear-layered",
		picker_group: familyId === "dinic-worst-case" ? "worst-case" : "structural",
		origin: "project-synthetic",
		sampling: "deterministic",
		difficulty:
			familyId === "dinic-worst-case" ? "verified-worst-case" : "ordinary",
		source_id: "fixture-source",
		tags: [familyId],
		presets: ["trace", "fast", "boundary"].map((purpose) => ({
			purpose,
			label: purpose,
			recommended_run_profile: purpose === "trace" ? "trace" : "fast",
			spec: {
				generator_revision: "flow-generator/27",
				seed: "42",
				family: { family_id: familyId, nodes: 8 },
				capacity: { kind: "unit" },
				cost: { kind: "zero" },
			},
			expects_strict_difficulty_certificate: familyId === "dinic-worst-case",
			expected_counters:
				familyId === "dinic-worst-case"
					? [
							{
								algorithm_id: "dinic",
								metric_id: "bfs-runs",
								exact_value: "8",
								evidence: "strict-certificate",
							},
						]
					: [],
		})),
		algorithm_compatibility: [
			{
				algorithm_id: "dinic",
				state: "recommended",
				reason: "fixture recommendation",
			},
		],
		default_algorithm_id: "dinic",
		admission_note: "preset-specific admission",
	};
}

function manifest() {
	return FLOW_GENERATOR_FAMILY_IDS.map(fixture);
}

function required<Value>(value: Value | undefined): Value {
	if (value === undefined) throw new Error("test fixture is incomplete");
	return value;
}

describe("flow generator fixture manifest", () => {
	it("decodes exactly 50 ordered families and preserves strict evidence", () => {
		const decoded = decodeFlowGeneratorFixtureManifest(
			JSON.stringify(manifest()),
		);
		expect(decoded).toHaveLength(50);
		expect(decoded.map((entry) => entry.family_id)).toEqual(
			FLOW_GENERATOR_FAMILY_IDS,
		);
		const worst = decoded.find(
			(entry) => entry.family_id === "dinic-worst-case",
		);
		const strictCounter = required(
			required(required(worst).presets[0]).expected_counters[0],
		);
		expect(strictCounter.evidence).toBe("strict-certificate");
		expect(flowGeneratorFixtureKind(required(worst))).toBe("Worst case");
	});

	it("uses the explicit picker group instead of randomized attributes", () => {
		const structural = fixture("path");
		structural.sampling = "randomized";
		const decoded = decodeFlowGeneratorFixtureManifest(
			JSON.stringify(
				manifest().map((entry) =>
					entry.family_id === "path" ? structural : entry,
				),
			),
		);
		expect(
			flowGeneratorFixtureKind(
				required(decoded.find((entry) => entry.family_id === "path")),
			),
		).toBe("Structural");
	});

	it("combines closed group and metadata search filters", () => {
		const decoded = decodeFlowGeneratorFixtureManifest(
			JSON.stringify(manifest()),
		);
		expect(FLOW_GENERATOR_PICKER_GROUPS).toEqual([
			"all",
			"structural",
			"random",
			"special",
			"benchmark",
			"stress",
			"worst-case",
		]);
		expect(
			filterFlowGeneratorFixtures(decoded, "DINIC worst", "worst-case").map(
				(entry) => entry.family_id,
			),
		).toEqual(["dinic-worst-case"]);
		expect(filterFlowGeneratorFixtures(decoded, "dinic", "stress")).toEqual([]);
		expect(filterFlowGeneratorFixtures(decoded, "", "all")).toEqual(decoded);
	});

	it("rejects reordered or missing families", () => {
		const reordered = manifest();
		const first = required(reordered[0]);
		reordered[0] = required(reordered[1]);
		reordered[1] = first;
		expect(() =>
			decodeFlowGeneratorFixtureManifest(JSON.stringify(reordered)),
		).toThrow(/arborescence/);
		expect(() =>
			decodeFlowGeneratorFixtureManifest(JSON.stringify(manifest().slice(1))),
		).toThrow(/exactly 50/);
	});

	it("rejects a family whose declared model disagrees with the canonical contract", () => {
		const malformed = manifest();
		required(malformed[6]).model = "max-flow";
		expect(() =>
			decodeFlowGeneratorFixtureManifest(JSON.stringify(malformed)),
		).toThrow(/cycle/);
	});

	it("rejects malformed counter evidence and repeated algorithm IDs", () => {
		const malformed = manifest();
		required(
			required(required(malformed[8]).presets[0]).expected_counters[0],
		).exact_value = "08";
		expect(() =>
			decodeFlowGeneratorFixtureManifest(JSON.stringify(malformed)),
		).toThrow(/counter/);

		const repeated = manifest();
		const firstFixture = required(repeated[0]);
		firstFixture.algorithm_compatibility.push({
			...required(firstFixture.algorithm_compatibility[0]),
		});
		expect(() =>
			decodeFlowGeneratorFixtureManifest(JSON.stringify(repeated)),
		).toThrow(/repeat/);
	});

	it("rejects a default descriptor that is not recommended", () => {
		const malformed = manifest();
		required(malformed[0]).default_algorithm_id = "edmonds-karp";
		expect(() =>
			decodeFlowGeneratorFixtureManifest(JSON.stringify(malformed)),
		).toThrow(/default algorithm/);
	});
});
