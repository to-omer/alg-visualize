import { describe, expect, it } from "vitest";

import {
	applyGridgraphPreset,
	applyNetgenPreset,
	applyWashingtonMatchingPreset,
	applyWashingtonPreset,
	applyWashingtonSquareMeshPreset,
	classifyNetgenForm,
	DEFAULT_FLOW_GENERATOR,
	encodeFlowGeneratorSpec,
	estimateFlowGenerator,
	FLOW_GENERATOR_FAMILY_DESCRIPTORS,
	flowGeneratorFamilyDescriptor,
	flowGeneratorFieldInvalid,
	validateFlowGeneratorForm,
} from "./FlowGeneratorDialog";
import { FLOW_GENERATOR_FAMILY_IDS } from "./flow-generator-fixture";

describe("flow generator form", () => {
	it("keeps a closed descriptor registry for all 50 generator families", () => {
		expect([...FLOW_GENERATOR_FAMILY_DESCRIPTORS.keys()]).toEqual(
			FLOW_GENERATOR_FAMILY_IDS,
		);
		expect(FLOW_GENERATOR_FAMILY_DESCRIPTORS.size).toBe(50);
		const invalidDefaults: string[] = [];
		const fixedConstructionFamilies: string[] = [];

		for (const family of FLOW_GENERATOR_FAMILY_IDS) {
			const descriptor = flowGeneratorFamilyDescriptor(family);
			const form = descriptor.defaults(DEFAULT_FLOW_GENERATOR);
			expect(descriptor.id).toBe(family);
			expect(form.family).toBe(family);
			const parameterControls = descriptor.parameters(form);
			expect(
				new Set(parameterControls.map((control) => control.field)).size,
			).toBe(parameterControls.length);
			expect(descriptor.estimate(form)).toEqual(estimateFlowGenerator(form));
			const validation = descriptor.validation(form);
			if (validation !== undefined)
				invalidDefaults.push(`${family}: ${validation}`);
			expect(descriptor.fieldInvalid(form, "seed")).toBe(false);
			expect(descriptor.encode(form)).toBe(encodeFlowGeneratorSpec(form));
			expect(JSON.parse(descriptor.encode(form)).family.family_id).toBe(family);
			if (descriptor.fixedConstruction(form) !== undefined) {
				fixedConstructionFamilies.push(family);
			}
			if (descriptor.features.has("washington-level")) {
				expect(descriptor.applyWashingtonLevelPreset).toBeDefined();
				expect(
					descriptor.applyWashingtonLevelPreset?.(form, "readable").family,
				).toBe(family);
			} else {
				expect(descriptor.applyWashingtonLevelPreset).toBeUndefined();
			}

			for (const control of parameterControls) {
				expect(control.label).not.toBe("");
				expect(Number.isFinite(control.minimum)).toBe(true);
				if (control.maximum !== undefined) {
					expect(Number.isFinite(control.maximum)).toBe(true);
					expect(control.minimum).toBeLessThanOrEqual(control.maximum);
				}
				expect(control.step).toBeGreaterThan(0);
			}

			const seedOnly = descriptor.customize(form, "seed", "43");
			expect(seedOnly.seed).toBe("43");
			if (descriptor.presetKey !== undefined) {
				expect(seedOnly[descriptor.presetKey]).toBe(form[descriptor.presetKey]);
				expect(
					descriptor.customize(form, "primary", form.primary + 1)[
						descriptor.presetKey
					],
				).toBe("custom");
				expect(descriptor.presets.length).toBeGreaterThan(0);
			}
		}
		expect(invalidDefaults).toEqual([]);
		expect(fixedConstructionFamilies).toEqual([
			"assignment-matrix",
			"cherkassky-goldberg-ak-stress",
			"dinic-worst-case",
			"glover-dense-acyclic-stress",
			"goldberg-mesh-circulation",
			"goto-torus",
			"gridgen-grid",
			"gridgraph-grid",
			"hall-tight-bipartite",
			"netgen-skeleton",
			"planted-bottleneck",
			"rmfgen-frames",
			"transportation-table",
			"waissi-setubal-acyclic-dense",
			"waissi-transit-one-way-grid",
			"waissi-transit-two-way-grid",
			"washington-basic-line",
			"washington-cheriyan-stress",
			"washington-dinic-phase-stress",
			"washington-double-exponential-line",
			"washington-exponential-line",
			"washington-goldberg-fifo-stress",
			"washington-matching",
			"washington-mesh",
			"washington-random-level",
			"washington-square-mesh",
			"zadeh-phase-chain-stress",
		]);
	});

	it("routes special-family field errors through each registry entry", () => {
		const invalidCases: readonly [
			family: (typeof FLOW_GENERATOR_FAMILY_IDS)[number],
			field: Parameters<typeof flowGeneratorFieldInvalid>[1],
			overrides: Partial<typeof DEFAULT_FLOW_GENERATOR>,
		][] = [
			["assignment-matrix", "tertiary", { tertiary: 1_001 }],
			["transportation-table", "tertiary", { tertiary: 1 }],
			["netgen-skeleton", "primary", { primary: 1 }],
			[
				"gridgen-grid",
				"gridgenTotalSupply",
				{ tertiary: 2, gridgenTotalSupply: "1" },
			],
			["gridgraph-grid", "primary", { primary: 1 }],
			["washington-matching", "secondary", { primary: 4, secondary: 5 }],
			["washington-square-mesh", "secondary", { primary: 4, secondary: 5 }],
			["washington-mesh", "primary", { primary: 2 }],
			[
				"washington-double-exponential-line",
				"tertiary",
				{ secondary: 1, tertiary: 19 },
			],
		];

		for (const [family, field, overrides] of invalidCases) {
			const descriptor = flowGeneratorFamilyDescriptor(family);
			const form = {
				...descriptor.defaults(DEFAULT_FLOW_GENERATOR),
				...overrides,
			};
			expect(descriptor.fieldInvalid(form, field), `${family}:${field}`).toBe(
				true,
			);
		}
	});

	it("computes exact shape estimates", () => {
		expect(estimateFlowGenerator(DEFAULT_FLOW_GENERATOR)).toEqual({
			nodes: 22n,
			edges: 40n,
		});
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "grid-2d",
				primary: 3,
				secondary: 4,
				toggle: true,
			}),
		).toEqual({ nodes: 12n, edges: 23n });
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "vision-segmentation-grid",
				primary: 3,
				secondary: 4,
				toggle: false,
			}),
		).toEqual({ nodes: 14n, edges: 58n });
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "vision-segmentation-grid",
				primary: 3,
				secondary: 4,
				toggle: true,
			}),
		).toEqual({ nodes: 14n, edges: 82n });
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "random-regular-directed",
				primary: 20,
				secondary: 3,
			}),
		).toEqual({ nodes: 20n, edges: 60n });
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "preferential-attachment-directed",
				primary: 30,
				secondary: 3,
			}),
		).toEqual({ nodes: 30n, edges: 84n });
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "planar-triangulated",
				primary: 9,
			}),
		).toEqual({ nodes: 9n, edges: 15n });
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "multi-source-sink",
				primary: 3,
				secondary: 4,
				tertiary: 2,
			}),
		).toEqual({ nodes: 11n, edges: 25n });
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "watts-strogatz-fixed",
				primary: 20,
				secondary: 4,
				tertiary: 8,
			}),
		).toEqual({ nodes: 20n, edges: 40n });
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "clustered-directed",
				primary: 3,
				secondary: 4,
				tertiary: 5,
			}),
		).toEqual({ nodes: 12n, edges: 17n });
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "hall-tight-bipartite",
				primary: 8,
				secondary: 3,
			}),
		).toEqual({ nodes: 18n, edges: 65n });
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "rmfgen-frames",
				primary: 3,
				secondary: 4,
			}),
		).toEqual({ nodes: 36n, edges: 123n });
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "gridgen-grid",
				primary: 3,
				secondary: 4,
				tertiary: 2,
				quaternary: 3,
				toggle: true,
			}),
		).toEqual({ nodes: 13n, edges: 39n });
		expect(
			estimateFlowGenerator(
				applyGridgraphPreset(DEFAULT_FLOW_GENERATOR, "readable"),
			),
		).toEqual({ nodes: 50n, edges: 94n });
		expect(
			estimateFlowGenerator({
				...DEFAULT_FLOW_GENERATOR,
				family: "goto-torus",
				primary: 32,
				secondary: 256,
			}),
		).toEqual({ nodes: 32n, edges: 256n });
		expect(
			estimateFlowGenerator(
				applyNetgenPreset(DEFAULT_FLOW_GENERATOR, "dense-transshipment"),
			),
		).toEqual({ nodes: 40n, edges: 1_000n });
	});

	it("encodes all visible values without hidden topology defaults", () => {
		const encoded = JSON.parse(encodeFlowGeneratorSpec(DEFAULT_FLOW_GENERATOR));
		expect(encoded.target_problem).toBeUndefined();
		expect(encoded).toMatchObject({
			generator_revision: "flow-generator/27",
			seed: "42",
			family: {
				family_id: "layered-dag",
				layers: 5,
				width: 4,
				fanout: 2,
			},
			capacity: { minimum: "3", maximum: "12" },
			cost: { minimum: "-3", maximum: "5" },
		});
		const geometric = {
			...DEFAULT_FLOW_GENERATOR,
			family: "random-geometric" as const,
			primary: 40,
			secondary: 120,
		};
		expect(JSON.parse(encodeFlowGeneratorSpec(geometric))).toMatchObject({
			family: {
				family_id: "random-geometric",
				nodes: 40,
				radius: 120,
			},
		});
		const multiTerminal = {
			...DEFAULT_FLOW_GENERATOR,
			family: "multi-source-sink" as const,
			primary: 3,
			secondary: 4,
			tertiary: 2,
		};
		expect(JSON.parse(encodeFlowGeneratorSpec(multiTerminal))).toMatchObject({
			family: {
				family_id: "multi-source-sink",
				sources: 3,
				intermediate: 4,
				sinks: 2,
			},
		});
		const smallWorld = {
			...DEFAULT_FLOW_GENERATOR,
			family: "watts-strogatz-fixed" as const,
			primary: 20,
			secondary: 4,
			tertiary: 8,
		};
		expect(JSON.parse(encodeFlowGeneratorSpec(smallWorld))).toMatchObject({
			family: {
				family_id: "watts-strogatz-fixed",
				nodes: 20,
				neighborhood: 4,
				rewire_count: 8,
			},
		});
		const visionGrid = {
			...DEFAULT_FLOW_GENERATOR,
			family: "vision-segmentation-grid" as const,
			primary: 3,
			secondary: 4,
			toggle: true,
			costKind: "zero" as const,
		};
		expect(validateFlowGeneratorForm(visionGrid)).toBeUndefined();
		expect(JSON.parse(encodeFlowGeneratorSpec(visionGrid))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "vision-segmentation-grid",
				rows: 3,
				columns: 4,
				eight_neighbor: true,
			},
			capacity: { kind: "uniform", minimum: "3", maximum: "12" },
			cost: { kind: "zero" },
		});
	});

	it("encodes an explicit cross-problem target instead of relabeling after generation", () => {
		expect(
			JSON.parse(
				encodeFlowGeneratorSpec(DEFAULT_FLOW_GENERATOR, "fixed-flow-min-cost"),
			),
		).toMatchObject({
			generator_revision: "flow-generator/27",
			target_problem: "fixed-flow-min-cost",
			family: { family_id: "layered-dag" },
		});
	});

	it("keeps vision graph-cut generation inside the complete BK trace band", () => {
		const valid = {
			...DEFAULT_FLOW_GENERATOR,
			family: "vision-segmentation-grid" as const,
			primary: 15,
			secondary: 15,
			toggle: false,
			costKind: "zero" as const,
		};
		expect(estimateFlowGenerator(valid)).toEqual({
			nodes: 227n,
			edges: 1_290n,
		});
		expect(validateFlowGeneratorForm(valid)).toBeUndefined();
		expect(flowGeneratorFieldInvalid(valid, "primary")).toBe(false);
		expect(flowGeneratorFieldInvalid(valid, "secondary")).toBe(false);

		const tooManyEdges = { ...valid, primary: 15, secondary: 16, toggle: true };
		expect(estimateFlowGenerator(tooManyEdges)).toEqual({
			nodes: 242n,
			edges: 2_218n,
		});
		expect(validateFlowGeneratorForm(tooManyEdges)).toMatch(/2,048 edges/);
		expect(flowGeneratorFieldInvalid(tooManyEdges, "primary")).toBe(true);
		expect(flowGeneratorFieldInvalid(tooManyEdges, "secondary")).toBe(true);
		expect(flowGeneratorFieldInvalid(tooManyEdges, "seed")).toBe(false);
	});

	it("encodes and validates every visible RMFGEN source parameter", () => {
		const rmfgen = {
			...DEFAULT_FLOW_GENERATOR,
			family: "rmfgen-frames" as const,
			primary: 3,
			secondary: 4,
			capacityMinimum: "2",
			capacityMaximum: "19",
			capacityKind: "bimodal" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(validateFlowGeneratorForm(rmfgen)).toBeUndefined();
		expect(JSON.parse(encodeFlowGeneratorSpec(rmfgen))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "rmfgen-frames",
				frame_size: 3,
				depth: 4,
				minimum_capacity: 2,
				maximum_capacity: 19,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(
			validateFlowGeneratorForm({ ...rmfgen, capacityMinimum: "20" }),
		).toMatch(/c1 ≤ c2/);
		expect(
			validateFlowGeneratorForm({ ...rmfgen, capacityMaximum: "1001" }),
		).toMatch(/1,000/);
		expect(validateFlowGeneratorForm({ ...rmfgen, primary: 1001 })).toMatch(
			/2–1,000/,
		);
		expect(validateFlowGeneratorForm({ ...rmfgen, secondary: 1001 })).toMatch(
			/1–1,000/,
		);
	});

	it("encodes and validates the GRIDGEN-derived source subset", () => {
		const gridgen = {
			...DEFAULT_FLOW_GENERATOR,
			family: "gridgen-grid" as const,
			primary: 3,
			secondary: 4,
			tertiary: 2,
			quaternary: 3,
			toggle: true,
			gridgenTotalSupply: "20",
			capacityMinimum: "3",
			capacityMaximum: "9",
			costMinimum: "2",
			costMaximum: "7",
			capacityKind: "power-of-two-buckets" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(validateFlowGeneratorForm(gridgen)).toBeUndefined();
		expect(JSON.parse(encodeFlowGeneratorSpec(gridgen))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "gridgen-grid",
				rows: 3,
				columns: 4,
				terminal_pairs: 2,
				average_degree: 3,
				total_supply: 20,
				two_way: true,
				minimum_capacity: 3,
				maximum_capacity: 9,
				minimum_cost: 2,
				maximum_cost: 7,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(validateFlowGeneratorForm({ ...gridgen, tertiary: 7 })).toMatch(
			/Terminal pairs/,
		);
		expect(validateFlowGeneratorForm({ ...gridgen, quaternary: 13 })).toMatch(
			/Average degree/,
		);
		expect(validateFlowGeneratorForm({ ...gridgen, primary: 1_001 })).toMatch(
			/between 2 and 1,000/,
		);
		expect(validateFlowGeneratorForm({ ...gridgen, secondary: 1_001 })).toMatch(
			/between 2 and 1,000/,
		);
		expect(
			validateFlowGeneratorForm({ ...gridgen, gridgenTotalSupply: "1" }),
		).toMatch(/Total supply/);
		expect(
			validateFlowGeneratorForm({ ...gridgen, capacityMinimum: "10" }),
		).toMatch(/capacity and cost ranges/);
		expect(
			validateFlowGeneratorForm({ ...gridgen, costMinimum: "-1" }),
		).toMatch(/capacity and cost ranges/);
	});

	it("encodes and validates readable, square, wide, and long GRIDGRAPH shapes", () => {
		const expected = {
			readable: { rows: 6, columns: 8, nodes: 50n, edges: 94n },
			square: { rows: 24, columns: 24, nodes: 578n, edges: 1_152n },
			wide: { rows: 48, columns: 16, nodes: 770n, edges: 1_568n },
			long: { rows: 16, columns: 48, nodes: 770n, edges: 1_504n },
		} as const;
		for (const preset of ["readable", "square", "wide", "long"] as const) {
			const form = applyGridgraphPreset(DEFAULT_FLOW_GENERATOR, preset);
			expect(validateFlowGeneratorForm(form), preset).toBeUndefined();
			expect(estimateFlowGenerator(form), preset).toEqual({
				nodes: expected[preset].nodes,
				edges: expected[preset].edges,
			});
			expect(JSON.parse(encodeFlowGeneratorSpec(form)), preset).toMatchObject({
				generator_revision: "flow-generator/27",
				family: {
					family_id: "gridgraph-grid",
					rows: expected[preset].rows,
					columns: expected[preset].columns,
					maximum_capacity: 1_000,
					maximum_cost: 10_000,
				},
				capacity: { kind: "unit" },
				cost: { kind: "zero" },
			});
		}

		const readable = applyGridgraphPreset(DEFAULT_FLOW_GENERATOR, "readable");
		expect(validateFlowGeneratorForm({ ...readable, primary: 1 })).toMatch(
			/rows W/,
		);
		expect(validateFlowGeneratorForm({ ...readable, secondary: 2 })).toMatch(
			/columns L must be 3–1,000/,
		);
		expect(
			validateFlowGeneratorForm({ ...readable, primary: 2, secondary: 999 }),
		).toBeUndefined();
		expect(
			validateFlowGeneratorForm({ ...readable, primary: 2, secondary: 1_000 }),
		).toMatch(/limited to 2,000 vertices/);
		expect(
			validateFlowGeneratorForm({ ...readable, primary: 50, secondary: 40 }),
		).toMatch(/limited to 2,000 vertices/);
		expect(
			validateFlowGeneratorForm({ ...readable, capacityMaximum: "0" }),
		).toMatch(/maximum capacity and cost/);
		expect(
			validateFlowGeneratorForm({ ...readable, costMaximum: "1000000001" }),
		).toMatch(/maximum capacity and cost/);
	});

	it("encodes and validates all Washington Random Level shapes", () => {
		const expected = {
			readable: { rows: 6, columns: 8, nodes: 50n, edges: 138n },
			wide: { rows: 6, columns: 24, nodes: 146n, edges: 426n },
			tall: { rows: 24, columns: 6, nodes: 146n, edges: 408n },
			dense: { rows: 32, columns: 32, nodes: 1_026n, edges: 3_040n },
		} as const;
		for (const preset of ["readable", "wide", "tall", "dense"] as const) {
			const form = applyWashingtonPreset(DEFAULT_FLOW_GENERATOR, preset);
			expect(validateFlowGeneratorForm(form), preset).toBeUndefined();
			expect(estimateFlowGenerator(form), preset).toEqual({
				nodes: expected[preset].nodes,
				edges: expected[preset].edges,
			});
			expect(JSON.parse(encodeFlowGeneratorSpec(form)), preset).toMatchObject({
				generator_revision: "flow-generator/27",
				family: {
					family_id: "washington-random-level",
					rows: expected[preset].rows,
					columns: expected[preset].columns,
					maximum_capacity: 100,
				},
				capacity: { kind: "unit" },
				cost: { kind: "zero" },
			});
		}

		const readable = applyWashingtonPreset(DEFAULT_FLOW_GENERATOR, "readable");
		expect(validateFlowGeneratorForm({ ...readable, primary: 2 })).toMatch(
			/3–1,000 vertices per level/,
		);
		expect(validateFlowGeneratorForm({ ...readable, secondary: 1 })).toMatch(
			/2–1,000 levels/,
		);
		expect(
			validateFlowGeneratorForm({ ...readable, primary: 3, secondary: 666 }),
		).toBeUndefined();
		expect(
			validateFlowGeneratorForm({ ...readable, primary: 3, secondary: 667 }),
		).toMatch(/limited to 2,000 vertices/);
		expect(
			validateFlowGeneratorForm({ ...readable, capacityMaximum: "0" }),
		).toMatch(/maximum capacity C/);
		expect(
			validateFlowGeneratorForm({
				...readable,
				capacityMaximum: "100000001",
			}),
		).toMatch(/maximum capacity C/);
	});

	it("encodes Washington function 4 Matching presets and strict practical limits", () => {
		const expected = {
			readable: { partSize: 12, degree: 3, nodes: 26n, edges: 60n },
			sparse: { partSize: 64, degree: 2, nodes: 130n, edges: 256n },
			medium: { partSize: 96, degree: 8, nodes: 194n, edges: 960n },
			dense: { partSize: 48, degree: 32, nodes: 98n, edges: 1_632n },
		} as const;
		for (const preset of ["readable", "sparse", "medium", "dense"] as const) {
			const form = applyWashingtonMatchingPreset(
				DEFAULT_FLOW_GENERATOR,
				preset,
			);
			expect(validateFlowGeneratorForm(form), preset).toBeUndefined();
			expect(estimateFlowGenerator(form), preset).toEqual({
				nodes: expected[preset].nodes,
				edges: expected[preset].edges,
			});
			expect(JSON.parse(encodeFlowGeneratorSpec(form)), preset).toMatchObject({
				generator_revision: "flow-generator/27",
				family: {
					family_id: "washington-matching",
					part_size: expected[preset].partSize,
					degree: expected[preset].degree,
				},
				capacity: { kind: "unit" },
				cost: { kind: "zero" },
			});
		}

		const readable = applyWashingtonMatchingPreset(
			DEFAULT_FLOW_GENERATOR,
			"readable",
		);
		expect(validateFlowGeneratorForm({ ...readable, primary: 1 })).toMatch(
			/side size n must be between 2 and 999/,
		);
		expect(validateFlowGeneratorForm({ ...readable, secondary: 0 })).toMatch(
			/degree d must be between 1 and n/,
		);
		expect(validateFlowGeneratorForm({ ...readable, secondary: 13 })).toMatch(
			/degree d must be between 1 and n/,
		);
		expect(
			validateFlowGeneratorForm({ ...readable, primary: 999, secondary: 18 }),
		).toBeUndefined();
		expect(
			validateFlowGeneratorForm({ ...readable, primary: 999, secondary: 19 }),
		).toMatch(/n\(d\+2\) ≤ 20,000/);
	});

	it("encodes Washington function 5 Square Mesh presets and clipped offset limits", () => {
		const expected = {
			readable: { dimension: 6, degree: 3, nodes: 38n, edges: 99n },
			sparse: { dimension: 18, degree: 2, nodes: 326n, edges: 647n },
			medium: { dimension: 24, degree: 6, nodes: 578n, edges: 3_345n },
			dense: { dimension: 27, degree: 27, nodes: 731n, edges: 18_657n },
		} as const;
		for (const preset of ["readable", "sparse", "medium", "dense"] as const) {
			const form = applyWashingtonSquareMeshPreset(
				DEFAULT_FLOW_GENERATOR,
				preset,
			);
			expect(validateFlowGeneratorForm(form), preset).toBeUndefined();
			expect(estimateFlowGenerator(form), preset).toEqual({
				nodes: expected[preset].nodes,
				edges: expected[preset].edges,
			});
			expect(JSON.parse(encodeFlowGeneratorSpec(form)), preset).toMatchObject({
				generator_revision: "flow-generator/27",
				family: {
					family_id: "washington-square-mesh",
					dimension: expected[preset].dimension,
					degree: expected[preset].degree,
					maximum_capacity: 100,
				},
				capacity: { kind: "unit" },
				cost: { kind: "zero" },
			});
		}

		const readable = applyWashingtonSquareMeshPreset(
			DEFAULT_FLOW_GENERATOR,
			"readable",
		);
		expect(validateFlowGeneratorForm({ ...readable, primary: 1 })).toMatch(
			/side length d must be between 2 and 44/,
		);
		expect(validateFlowGeneratorForm({ ...readable, secondary: 0 })).toMatch(
			/degree must be between 1 and d/,
		);
		expect(validateFlowGeneratorForm({ ...readable, secondary: 7 })).toMatch(
			/degree must be between 1 and d/,
		);
		expect(
			validateFlowGeneratorForm({ ...readable, primary: 44, secondary: 10 }),
		).toBeUndefined();
		expect(
			validateFlowGeneratorForm({ ...readable, primary: 44, secondary: 11 }),
		).toMatch(/limited to 20,000 edges/);
		expect(
			validateFlowGeneratorForm({ ...readable, capacityMaximum: "0" }),
		).toMatch(/maximum capacity C/);
	});

	it("encodes Washington function 1 Mesh with the same practical shape presets", () => {
		const expected = {
			readable: { rows: 6, columns: 8, nodes: 50n, edges: 138n },
			wide: { rows: 6, columns: 24, nodes: 146n, edges: 426n },
			tall: { rows: 24, columns: 6, nodes: 146n, edges: 408n },
			dense: { rows: 32, columns: 32, nodes: 1_026n, edges: 3_040n },
		} as const;
		for (const preset of ["readable", "wide", "tall", "dense"] as const) {
			const form = applyWashingtonPreset(
				DEFAULT_FLOW_GENERATOR,
				preset,
				"washington-mesh",
			);
			expect(validateFlowGeneratorForm(form), preset).toBeUndefined();
			expect(estimateFlowGenerator(form), preset).toEqual({
				nodes: expected[preset].nodes,
				edges: expected[preset].edges,
			});
			expect(JSON.parse(encodeFlowGeneratorSpec(form)), preset).toMatchObject({
				generator_revision: "flow-generator/27",
				family: {
					family_id: "washington-mesh",
					rows: expected[preset].rows,
					columns: expected[preset].columns,
					maximum_capacity: 100,
				},
				capacity: { kind: "unit" },
				cost: { kind: "zero" },
			});
		}

		const readable = applyWashingtonPreset(
			DEFAULT_FLOW_GENERATOR,
			"readable",
			"washington-mesh",
		);
		expect(validateFlowGeneratorForm({ ...readable, primary: 2 })).toMatch(
			/Washington Mesh.*3–1,000 vertices per level/,
		);
		expect(validateFlowGeneratorForm({ ...readable, secondary: 1 })).toMatch(
			/Washington Mesh.*2–1,000 levels/,
		);
		expect(
			validateFlowGeneratorForm({ ...readable, primary: 3, secondary: 667 }),
		).toMatch(/limited to 2,000 vertices/);
		expect(
			validateFlowGeneratorForm({ ...readable, capacityMaximum: "0" }),
		).toMatch(/Washington Mesh.*maximum capacity C/);
	});

	it("encodes the distinct Washington function 9 Dinic phase stress", () => {
		const form = {
			...DEFAULT_FLOW_GENERATOR,
			family: "washington-dinic-phase-stress" as const,
			primary: 8,
			capacityKind: "bimodal" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(validateFlowGeneratorForm(form)).toBeUndefined();
		expect(estimateFlowGenerator(form)).toEqual({ nodes: 8n, edges: 13n });
		expect(JSON.parse(encodeFlowGeneratorSpec(form))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "washington-dinic-phase-stress",
				nodes: 8,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(validateFlowGeneratorForm({ ...form, primary: 1 })).toMatch(
			/2–2,000/,
		);
		expect(validateFlowGeneratorForm({ ...form, primary: 2_001 })).toMatch(
			/2–2,000/,
		);
	});

	it("encodes Washington function 10 as a bounded FIFO push-relabel stress", () => {
		const form = {
			...DEFAULT_FLOW_GENERATOR,
			family: "washington-goldberg-fifo-stress" as const,
			primary: 8,
			capacityKind: "bimodal" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(validateFlowGeneratorForm(form)).toBeUndefined();
		expect(estimateFlowGenerator(form)).toEqual({ nodes: 27n, edges: 33n });
		expect(JSON.parse(encodeFlowGeneratorSpec(form))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "washington-goldberg-fifo-stress",
				block_size: 8,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(validateFlowGeneratorForm({ ...form, primary: 1 })).toMatch(
			/2 and 64/,
		);
		expect(validateFlowGeneratorForm({ ...form, primary: 65 })).toMatch(
			/2 and 64/,
		);
	});

	it("encodes Washington function 11 from its actual bounded construction", () => {
		const form = {
			...DEFAULT_FLOW_GENERATOR,
			family: "washington-cheriyan-stress" as const,
			primary: 8,
			secondary: 4,
			tertiary: 2,
			capacityKind: "bimodal" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(validateFlowGeneratorForm(form)).toBeUndefined();
		expect(estimateFlowGenerator(form)).toEqual({ nodes: 55n, edges: 75n });
		expect(JSON.parse(encodeFlowGeneratorSpec(form))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "washington-cheriyan-stress",
				bridge_width: 8,
				gadget_entries: 4,
				chain_length: 2,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(validateFlowGeneratorForm({ ...form, primary: 0 })).toMatch(
			/n=1–64/,
		);
		expect(validateFlowGeneratorForm({ ...form, primary: 65 })).toMatch(
			/n=1–64/,
		);
		expect(validateFlowGeneratorForm({ ...form, secondary: 13 })).toMatch(
			/m=1–12/,
		);
		expect(validateFlowGeneratorForm({ ...form, tertiary: 11 })).toMatch(
			/c=1–10/,
		);
	});

	it("encodes the deterministic AK push-relabel stress within its work ceiling", () => {
		const form = {
			...DEFAULT_FLOW_GENERATOR,
			family: "cherkassky-goldberg-ak-stress" as const,
			primary: 4,
			capacityKind: "bimodal" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(validateFlowGeneratorForm(form)).toBeUndefined();
		expect(estimateFlowGenerator(form)).toEqual({ nodes: 22n, edges: 31n });
		expect(JSON.parse(encodeFlowGeneratorSpec(form))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "cherkassky-goldberg-ak-stress",
				size: 4,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(validateFlowGeneratorForm({ ...form, primary: 1 })).toMatch(
			/2 and 128/,
		);
		expect(validateFlowGeneratorForm({ ...form, primary: 129 })).toMatch(
			/2 and 128/,
		);
	});

	it("encodes First DIMACS AC with its official dense DAG contract", () => {
		const form = {
			...DEFAULT_FLOW_GENERATOR,
			family: "waissi-setubal-acyclic-dense" as const,
			primary: 12,
			capacityKind: "bimodal" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(validateFlowGeneratorForm(form)).toBeUndefined();
		expect(estimateFlowGenerator(form)).toEqual({ nodes: 12n, edges: 66n });
		expect(JSON.parse(encodeFlowGeneratorSpec(form))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "waissi-setubal-acyclic-dense",
				nodes: 12,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(validateFlowGeneratorForm({ ...form, primary: 1 })).toMatch(/2–200/);
		expect(validateFlowGeneratorForm({ ...form, primary: 201 })).toMatch(
			/2–200/,
		);
	});

	it("encodes the Glover-Waissi dense source-claimed stress contract", () => {
		const form = {
			...DEFAULT_FLOW_GENERATOR,
			family: "glover-dense-acyclic-stress" as const,
			primary: 12,
			capacityKind: "bimodal" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(validateFlowGeneratorForm(form)).toBeUndefined();
		expect(estimateFlowGenerator(form)).toEqual({ nodes: 12n, edges: 66n });
		expect(JSON.parse(encodeFlowGeneratorSpec(form))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "glover-dense-acyclic-stress",
				nodes: 12,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(validateFlowGeneratorForm({ ...form, primary: 1 })).toMatch(/2–200/);
		expect(validateFlowGeneratorForm({ ...form, primary: 201 })).toMatch(
			/2–200/,
		);
	});

	it("encodes the Waissi two-way transit grid benchmark contract", () => {
		const form = {
			...DEFAULT_FLOW_GENERATOR,
			family: "waissi-transit-two-way-grid" as const,
			primary: 4,
			secondary: 100,
			capacityKind: "bimodal" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(validateFlowGeneratorForm(form)).toBeUndefined();
		expect(estimateFlowGenerator(form)).toEqual({ nodes: 18n, edges: 64n });
		expect(JSON.parse(encodeFlowGeneratorSpec(form))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "waissi-transit-two-way-grid",
				dimension: 4,
				maximum_capacity: 100,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(validateFlowGeneratorForm({ ...form, primary: 1 })).toMatch(
			/d=2–44/,
		);
		expect(validateFlowGeneratorForm({ ...form, primary: 45 })).toMatch(
			/d=2–44/,
		);
		expect(validateFlowGeneratorForm({ ...form, secondary: 0 })).toMatch(
			/U=1–1,000,000,000/,
		);
	});

	it("encodes the Waissi one-way transit grid benchmark contract", () => {
		const form = {
			...DEFAULT_FLOW_GENERATOR,
			family: "waissi-transit-one-way-grid" as const,
			primary: 4,
			secondary: 100,
			capacityKind: "bimodal" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(validateFlowGeneratorForm(form)).toBeUndefined();
		expect(estimateFlowGenerator(form)).toEqual({ nodes: 18n, edges: 32n });
		expect(JSON.parse(encodeFlowGeneratorSpec(form))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "waissi-transit-one-way-grid",
				dimension: 4,
				maximum_capacity: 100,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(validateFlowGeneratorForm({ ...form, primary: 1 })).toMatch(
			/d=2–44/,
		);
		expect(validateFlowGeneratorForm({ ...form, primary: 45 })).toMatch(
			/d=2–44/,
		);
		expect(validateFlowGeneratorForm({ ...form, secondary: 0 })).toMatch(
			/U=1–1,000,000,000/,
		);
	});

	it("encodes the Goldberg signed-bound mesh circulation contract", () => {
		const form = {
			...DEFAULT_FLOW_GENERATOR,
			family: "goldberg-mesh-circulation" as const,
			primary: 4,
			secondary: 3,
			tertiary: 1,
			quaternary: 1,
			capacityKind: "bimodal" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(validateFlowGeneratorForm(form)).toBeUndefined();
		expect(estimateFlowGenerator(form)).toEqual({ nodes: 12n, edges: 48n });
		expect(JSON.parse(encodeFlowGeneratorSpec(form))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "goldberg-mesh-circulation",
				columns: 4,
				rows: 3,
				horizontal_degree: 1,
				vertical_degree: 1,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(validateFlowGeneratorForm({ ...form, primary: 2 })).toMatch(
			/3 and 32/,
		);
		expect(validateFlowGeneratorForm({ ...form, secondary: 33 })).toMatch(
			/3 and 32/,
		);
		expect(
			validateFlowGeneratorForm({ ...form, primary: 6, tertiary: 3 }),
		).toMatch(/XDEG must be between 0 and 2/);
		expect(
			validateFlowGeneratorForm({ ...form, tertiary: 0, quaternary: 0 }),
		).toMatch(/At least one/);
		expect(
			estimateFlowGenerator({
				...form,
				primary: 32,
				secondary: 32,
				tertiary: 8,
				quaternary: 8,
			}),
		).toEqual({ nodes: 1_024n, edges: 32_768n });
	});

	it("attributes source-backed validation errors to only participating fields", () => {
		const goldberg = {
			...DEFAULT_FLOW_GENERATOR,
			family: "goldberg-mesh-circulation" as const,
			primary: 6,
			secondary: 5,
			tertiary: 3,
			quaternary: 1,
		};
		expect(flowGeneratorFieldInvalid(goldberg, "tertiary")).toBe(true);
		expect(flowGeneratorFieldInvalid(goldberg, "primary")).toBe(false);
		expect(flowGeneratorFieldInvalid(goldberg, "secondary")).toBe(false);
		expect(flowGeneratorFieldInvalid(goldberg, "quaternary")).toBe(false);
		expect(flowGeneratorFieldInvalid(goldberg, "seed")).toBe(false);

		const invalidSeed = { ...goldberg, tertiary: 2, seed: "01" };
		expect(flowGeneratorFieldInvalid(invalidSeed, "seed")).toBe(true);
		for (const field of [
			"primary",
			"secondary",
			"tertiary",
			"quaternary",
		] as const) {
			expect(flowGeneratorFieldInvalid(invalidSeed, field)).toBe(false);
		}

		const nonFiniteShape = {
			...goldberg,
			primary: Number.POSITIVE_INFINITY,
		};
		expect(flowGeneratorFieldInvalid(nonFiniteShape, "primary")).toBe(true);
		expect(flowGeneratorFieldInvalid(nonFiniteShape, "secondary")).toBe(false);
		const fractionalShape = { ...goldberg, primary: 6.5 };
		expect(flowGeneratorFieldInvalid(fractionalShape, "primary")).toBe(true);
		expect(() =>
			flowGeneratorFieldInvalid(fractionalShape, "secondary"),
		).not.toThrow();
		expect(flowGeneratorFieldInvalid(fractionalShape, "secondary")).toBe(false);

		const invalidNetgenCount = {
			...applyNetgenPreset(DEFAULT_FLOW_GENERATOR, "general-min-cost"),
			netgenTransshipmentSources: Number.NaN,
		};
		expect(
			flowGeneratorFieldInvalid(
				invalidNetgenCount,
				"netgenTransshipmentSources",
			),
		).toBe(true);
		expect(
			flowGeneratorFieldInvalid(invalidNetgenCount, "netgenTotalSupply"),
		).toBe(false);

		const invalidGotoCost = {
			...DEFAULT_FLOW_GENERATOR,
			family: "goto-torus" as const,
			primary: 30,
			secondary: 180,
			capacityMaximum: "100",
			costMaximum: "7",
		};
		expect(flowGeneratorFieldInvalid(invalidGotoCost, "costMaximum")).toBe(
			true,
		);
		expect(flowGeneratorFieldInvalid(invalidGotoCost, "primary")).toBe(false);

		const invalidLineDegree = {
			...DEFAULT_FLOW_GENERATOR,
			family: "washington-double-exponential-line" as const,
			primary: 6,
			secondary: 1,
			tertiary: 19,
		};
		expect(flowGeneratorFieldInvalid(invalidLineDegree, "tertiary")).toBe(true);
		expect(flowGeneratorFieldInvalid(invalidLineDegree, "primary")).toBe(false);
	});

	it("encodes the complete GOTO-derived parameter contract", () => {
		const readableDefault = flowGeneratorFamilyDescriptor(
			"goto-torus",
		).defaults(DEFAULT_FLOW_GENERATOR);
		expect(readableDefault).toMatchObject({
			family: "goto-torus",
			primary: 15,
			secondary: 90,
			capacityMaximum: "8",
			costMaximum: "8",
		});
		expect(validateFlowGeneratorForm(readableDefault)).toBeUndefined();

		const goto = {
			...DEFAULT_FLOW_GENERATOR,
			family: "goto-torus" as const,
			primary: 32,
			secondary: 256,
			capacityMaximum: "1000",
			costMaximum: "10000",
			capacityKind: "bimodal" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(validateFlowGeneratorForm(goto)).toBeUndefined();
		expect(JSON.parse(encodeFlowGeneratorSpec(goto))).toMatchObject({
			generator_revision: "flow-generator/27",
			family: {
				family_id: "goto-torus",
				nodes: 32,
				edge_count: 256,
				maximum_capacity: 1000,
				maximum_cost: 10000,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(validateFlowGeneratorForm({ ...goto, primary: 14 })).toMatch(
			/15 and 10,000/,
		);
		expect(validateFlowGeneratorForm({ ...goto, secondary: 191 })).toMatch(
			/6N/,
		);
		expect(validateFlowGeneratorForm({ ...goto, secondary: 323 })).toMatch(
			/N\^\(5\/3\)/,
		);
		expect(
			validateFlowGeneratorForm({ ...goto, capacityMaximum: "7" }),
		).toMatch(/capacity limit U/);
		expect(
			validateFlowGeneratorForm({ ...goto, costMaximum: "1000000001" }),
		).toMatch(/capacity limit U and maximum cost C/);
	});

	it("offers five valid NETGEN-derived problem classes and encodes all 13 parameters", () => {
		const cases = [
			["general-min-cost", "transshipment"],
			["transportation", "transportation"],
			["assignment", "assignment"],
			["single-source-max-flow", "max-flow"],
			["dense-transshipment", "transshipment"],
		] as const;
		for (const [preset, kind] of cases) {
			const form = applyNetgenPreset(DEFAULT_FLOW_GENERATOR, preset);
			expect(validateFlowGeneratorForm(form), preset).toBeUndefined();
			expect(classifyNetgenForm(form), preset).toBe(kind);
		}

		const general = applyNetgenPreset(
			DEFAULT_FLOW_GENERATOR,
			"general-min-cost",
		);
		expect(JSON.parse(encodeFlowGeneratorSpec(general))).toMatchObject({
			generator_revision: "flow-generator/27",
			seed: "42",
			family: {
				family_id: "netgen-skeleton",
				nodes: 24,
				sources: 3,
				sinks: 4,
				edge_count: 80,
				minimum_cost: -5,
				maximum_cost: 20,
				total_supply: 60,
				transshipment_sources: 1,
				transshipment_sinks: 1,
				high_cost_percentage: 75,
				capacitated_percentage: 65,
				minimum_capacity: 2,
				maximum_capacity: 30,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
	});

	it("mirrors NETGEN skeleton, eligibility, percentage, and max-flow boundaries", () => {
		const general = applyNetgenPreset(
			DEFAULT_FLOW_GENERATOR,
			"general-min-cost",
		);
		expect(
			validateFlowGeneratorForm({ ...general, netgenTotalSupply: "3" }),
		).toMatch(/max\(S,T\)/);
		expect(
			validateFlowGeneratorForm({ ...general, netgenTransshipmentSources: 4 }),
		).toMatch(/cannot exceed their terminal counts/);
		expect(
			validateFlowGeneratorForm({ ...general, netgenHighCostPercentage: 101 }),
		).toMatch(/between 0% and 100%/);
		expect(validateFlowGeneratorForm({ ...general, quaternary: 24 })).toMatch(
			/26 edges required by the feasible skeleton/,
		);
		expect(validateFlowGeneratorForm({ ...general, quaternary: 444 })).toMatch(
			/443 allowed simple directed edges/,
		);
		expect(
			validateFlowGeneratorForm({
				...general,
				costMinimum: "1",
				costMaximum: "1",
			}),
		).toMatch(/exactly one source and one sink/);
		expect(
			validateFlowGeneratorForm({ ...general, capacityMaximum: "1000000001" }),
		).toMatch(/NETGEN capacities/);
	});

	it("keeps sourced stress and worst-case attributes fixed and explicit", () => {
		const dinic = {
			...DEFAULT_FLOW_GENERATOR,
			family: "dinic-worst-case" as const,
			primary: 12,
		};
		expect(estimateFlowGenerator(dinic)).toEqual({ nodes: 12n, edges: 21n });
		expect(JSON.parse(encodeFlowGeneratorSpec(dinic))).toMatchObject({
			family: { family_id: "dinic-worst-case", nodes: 12 },
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});

		const zadeh = {
			...DEFAULT_FLOW_GENERATOR,
			family: "zadeh-phase-chain-stress" as const,
			primary: 12,
			capacityKind: "bimodal" as const,
			costKind: "capacity-correlated" as const,
		};
		expect(estimateFlowGenerator(zadeh)).toEqual({ nodes: 36n, edges: 226n });
		expect(JSON.parse(encodeFlowGeneratorSpec(zadeh))).toMatchObject({
			family: {
				family_id: "zadeh-phase-chain-stress",
				group_size: 12,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
	});

	it("rejects invalid family domains and integer attribute ranges before WASM", () => {
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "bipartite-random",
				primary: 3,
				secondary: 4,
				tertiary: 13,
			}),
		).toMatch(/L×R/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				capacityMinimum: "10",
				capacityMaximum: "2",
			}),
		).toMatch(/Capacities/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "dinic-worst-case",
				primary: 8,
				capacityMinimum: "not-used",
			}),
		).toBeUndefined();
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "random-geometric",
				primary: 449,
				secondary: 10,
			}),
		).toMatch(/safe candidate-edge bound/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "random-regular-directed",
				primary: 8,
				secondary: 8,
			}),
		).toMatch(/degree < n/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "planar-triangulated",
				primary: 2,
			}),
		).toMatch(/at least 3/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "multi-source-sink",
				primary: 2,
				secondary: 0,
				tertiary: 2,
			}),
		).toMatch(/vertex counts must each be at least 1/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "watts-strogatz-fixed",
				primary: 12,
				secondary: 5,
				tertiary: 3,
			}),
		).toMatch(/must be even/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "hall-tight-bipartite",
				primary: 8,
				secondary: 8,
			}),
		).toMatch(/smaller than each side/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "zadeh-phase-chain-stress",
				primary: 10,
			}),
		).toMatch(/multiple of 4/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "zadeh-phase-chain-stress",
				primary: 24,
			}),
		).toMatch(/between 4 and 20/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "zadeh-phase-chain-stress",
				primary: 12,
				capacityMinimum: "not-used",
			}),
		).toBeUndefined();
	});

	it("fixes attribute distributions for construction-owned families", () => {
		const bottleneck = {
			...DEFAULT_FLOW_GENERATOR,
			family: "planted-bottleneck" as const,
			primary: 4,
			secondary: 5,
			tertiary: 7,
			capacityMinimum: "999",
			costMinimum: "-999",
		};
		expect(JSON.parse(encodeFlowGeneratorSpec(bottleneck))).toMatchObject({
			family: {
				family_id: "planted-bottleneck",
				left: 4,
				right: 5,
				cut_edges: 7,
			},
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});
		expect(validateFlowGeneratorForm(bottleneck)).toBeUndefined();
	});

	it("encodes and validates every native assignment matrix shape", () => {
		const expectedEdges: Record<
			typeof DEFAULT_FLOW_GENERATOR.assignmentShape,
			bigint
		> = {
			uniform: 14n,
			equal: 20n,
			block: 20n,
			"near-tie": 20n,
			"planted-optimum": 12n,
			monge: 20n,
			"anti-monge": 20n,
			"sparse-allowed": 8n,
			"hall-deficient": 11n,
		};
		for (const assignmentShape of Object.keys(expectedEdges) as Array<
			keyof typeof expectedEdges
		>) {
			const form = {
				...DEFAULT_FLOW_GENERATOR,
				family: "assignment-matrix" as const,
				primary: 4,
				secondary: 5,
				tertiary:
					assignmentShape === "uniform"
						? 700
						: assignmentShape === "planted-optimum"
							? 600
							: assignmentShape === "hall-deficient"
								? 3
								: 2,
				quaternary:
					assignmentShape === "hall-deficient"
						? 2
						: assignmentShape === "planted-optimum"
							? 5
							: 0,
				assignmentShape,
				costMinimum: "-3",
				costMaximum: "9",
			};
			expect(validateFlowGeneratorForm(form), assignmentShape).toBeUndefined();
			expect(estimateFlowGenerator(form)).toEqual({
				nodes: 9n,
				edges: expectedEdges[assignmentShape],
			});
			const encoded = JSON.parse(encodeFlowGeneratorSpec(form));
			expect(encoded).toMatchObject({
				generator_revision: "flow-generator/27",
				family: {
					family_id: "assignment-matrix",
					agents: 4,
					tasks: 5,
					objective: "minimize",
					shape: { kind: assignmentShape },
				},
				capacity: { kind: "unit" },
				cost: { kind: "zero" },
			});
		}
	});

	it("rejects assignment infeasibility-shape mistakes and Hungarian admission overflow", () => {
		const base = {
			...DEFAULT_FLOW_GENERATOR,
			family: "assignment-matrix" as const,
			primary: 6,
			secondary: 8,
		};
		expect(
			validateFlowGeneratorForm({
				...base,
				assignmentShape: "planted-optimum",
				primary: 9,
				secondary: 8,
				tertiary: 600,
				quaternary: 5,
			}),
		).toMatch(/at least as many tasks as agents/);
		expect(
			validateFlowGeneratorForm({
				...base,
				assignmentShape: "hall-deficient",
				tertiary: 4,
				quaternary: 4,
			}),
		).toMatch(/smaller than the agent prefix/);
		expect(
			validateFlowGeneratorForm({
				...base,
				assignmentShape: "sparse-allowed",
				tertiary: 9,
			}),
		).toMatch(/degree/);
		expect(
			validateFlowGeneratorForm({
				...base,
				assignmentShape: "sparse-allowed",
				primary: 300,
				secondary: 300,
				tertiary: 1,
			}),
		).toMatch(/Hungarian scan limit/);
	});

	it("encodes all native transportation shapes with bounded estimates", () => {
		const expectedEdges: Record<
			typeof DEFAULT_FLOW_GENERATOR.transportationShape,
			bigint
		> = {
			"dense-uniform": 20n,
			"sparse-feasible": 8n,
			"unit-degenerate": 16n,
			block: 20n,
			"near-tie": 20n,
			monge: 20n,
			"cut-infeasible": 16n,
		};
		for (const transportationShape of Object.keys(expectedEdges) as Array<
			keyof typeof expectedEdges
		>) {
			const unit = transportationShape === "unit-degenerate";
			const form = {
				...DEFAULT_FLOW_GENERATOR,
				family: "transportation-table" as const,
				primary: 4,
				secondary: unit ? 4 : 5,
				tertiary: unit ? 4 : 20,
				quaternary:
					transportationShape === "sparse-feasible"
						? 400
						: ["block", "near-tie", "monge"].includes(transportationShape)
							? 2
							: 0,
				transportationShape,
				costMinimum: "-3",
				costMaximum: "9",
			};
			expect(
				validateFlowGeneratorForm(form),
				transportationShape,
			).toBeUndefined();
			expect(estimateFlowGenerator(form)).toEqual({
				nodes: unit ? 8n : 9n,
				edges: expectedEdges[transportationShape],
			});
			const encoded = JSON.parse(encodeFlowGeneratorSpec(form));
			expect(encoded).toMatchObject({
				generator_revision: "flow-generator/27",
				family: {
					family_id: "transportation-table",
					origins: 4,
					destinations: unit ? 4 : 5,
					total_supply: unit ? 4 : 20,
					shape: { kind: transportationShape },
				},
				capacity: { kind: "unit" },
				cost: { kind: "zero" },
			});
		}
	});

	it("rejects transportation admission, degeneracy, density, and cut drift", () => {
		const base = {
			...DEFAULT_FLOW_GENERATOR,
			family: "transportation-table" as const,
			primary: 4,
			secondary: 5,
			tertiary: 20,
			quaternary: 400,
			transportationShape: "sparse-feasible" as const,
			costMinimum: "0",
			costMaximum: "9",
		};
		expect(
			validateFlowGeneratorForm({ ...base, primary: 64, secondary: 64 }),
		).toMatch(/2,048 route/);
		expect(validateFlowGeneratorForm({ ...base, quaternary: 0 })).toMatch(
			/between 1 and 1,000‰/,
		);
		expect(
			validateFlowGeneratorForm({
				...base,
				transportationShape: "unit-degenerate",
			}),
		).toMatch(/origins = destinations = B/);
		expect(
			validateFlowGeneratorForm({
				...base,
				transportationShape: "cut-infeasible",
				primary: 1,
			}),
		).toMatch(/at least 2 vertices per partition/);
		const invalidBlockMaximum = {
			...base,
			transportationShape: "block" as const,
			quaternary: 2,
			costMaximum: "not-an-integer",
		};
		expect(validateFlowGeneratorForm(invalidBlockMaximum)).toMatch(
			/Within-group and cross-group costs/,
		);
		expect(flowGeneratorFieldInvalid(invalidBlockMaximum, "costMaximum")).toBe(
			true,
		);
		expect(flowGeneratorFieldInvalid(invalidBlockMaximum, "costMinimum")).toBe(
			false,
		);
	});

	it("encodes every exposed orthogonal capacity and cost distribution", () => {
		const unitZero = {
			...DEFAULT_FLOW_GENERATOR,
			capacityKind: "unit" as const,
			costKind: "zero" as const,
			capacityMinimum: "hidden-invalid",
			capacityMaximum: "hidden-invalid",
			costMinimum: "hidden-invalid",
			costMaximum: "hidden-invalid",
		};
		expect(validateFlowGeneratorForm(unitZero)).toBeUndefined();
		expect(JSON.parse(encodeFlowGeneratorSpec(unitZero))).toMatchObject({
			capacity: { kind: "unit" },
			cost: { kind: "zero" },
		});

		const constant = {
			...DEFAULT_FLOW_GENERATOR,
			capacityKind: "constant" as const,
			costKind: "constant" as const,
			capacityMinimum: "17",
			capacityMaximum: "hidden-invalid",
			costMinimum: "-9",
			costMaximum: "hidden-invalid",
		};
		expect(validateFlowGeneratorForm(constant)).toBeUndefined();
		expect(JSON.parse(encodeFlowGeneratorSpec(constant))).toMatchObject({
			capacity: { kind: "constant", value: "17" },
			cost: { kind: "constant", value: "-9" },
		});

		const bimodalCorrelated = {
			...DEFAULT_FLOW_GENERATOR,
			capacityKind: "bimodal" as const,
			capacityMinimum: "2",
			capacityMaximum: "64",
			costKind: "capacity-correlated" as const,
			costMinimum: "-12",
			costMaximum: "18",
			costCorrelationDirection: "negative" as const,
			costMaximumJitter: "3",
		};
		expect(validateFlowGeneratorForm(bimodalCorrelated)).toBeUndefined();
		expect(
			JSON.parse(encodeFlowGeneratorSpec(bimodalCorrelated)),
		).toMatchObject({
			capacity: { kind: "bimodal", first: "2", second: "64" },
			cost: {
				kind: "capacity-correlated",
				minimum: "-12",
				maximum: "18",
				direction: "negative",
				maximum_jitter: "3",
			},
		});

		const buckets = {
			...DEFAULT_FLOW_GENERATOR,
			capacityKind: "power-of-two-buckets" as const,
			capacityMinimum: "1",
			capacityMaximum: "12",
			costKind: "bimodal" as const,
			costMinimum: "-5",
			costMaximum: "8",
		};
		expect(validateFlowGeneratorForm(buckets)).toBeUndefined();
		expect(JSON.parse(encodeFlowGeneratorSpec(buckets))).toMatchObject({
			capacity: {
				kind: "power-of-two-buckets",
				minimum_exponent: 1,
				maximum_exponent: 12,
			},
			cost: { kind: "bimodal", first: "-5", second: "8" },
		});
	});

	it("rejects degenerate advanced distribution controls before WASM", () => {
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				capacityKind: "bimodal",
				capacityMinimum: "7",
				capacityMaximum: "7",
			}),
		).toMatch(/atoms A and B/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				capacityKind: "power-of-two-buckets",
				capacityMinimum: "0",
				capacityMaximum: "64",
			}),
		).toMatch(/exponents/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				costKind: "capacity-correlated",
				costMaximumJitter: "-1",
			}),
		).toMatch(/jitter/);
	});

	it("encodes Washington functions 6–8 with practical upper bounds", () => {
		for (const family of [
			"washington-basic-line",
			"washington-exponential-line",
			"washington-double-exponential-line",
		] as const) {
			const form = {
				...DEFAULT_FLOW_GENERATOR,
				family,
				primary: 8,
				secondary: 4,
				tertiary: 3,
			};
			expect(estimateFlowGenerator(form)).toEqual({ nodes: 34n, edges: 104n });
			expect(validateFlowGeneratorForm(form)).toBeUndefined();
			expect(JSON.parse(encodeFlowGeneratorSpec(form))).toMatchObject({
				generator_revision: "flow-generator/27",
				family: { family_id: family, levels: 8, width: 4, degree: 3 },
				capacity: { kind: "unit" },
				cost: { kind: "zero" },
			});
		}
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "washington-double-exponential-line",
				primary: 8,
				secondary: 1,
				tertiary: 19,
			}),
		).toMatch(/between 1 and 18/);
		expect(
			validateFlowGeneratorForm({
				...DEFAULT_FLOW_GENERATOR,
				family: "washington-basic-line",
				primary: 100,
				secondary: 20,
				tertiary: 10,
			}),
		).toMatch(/2,000 vertices/);
	});
});
