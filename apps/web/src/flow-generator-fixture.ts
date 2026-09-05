export const FLOW_GENERATOR_FAMILY_IDS = [
	"arborescence",
	"assignment-matrix",
	"bipartite-random",
	"cherkassky-goldberg-ak-stress",
	"clustered-directed",
	"complete-dag",
	"cycle",
	"diamond-chain",
	"dinic-worst-case",
	"erdos-renyi-directed",
	"glover-dense-acyclic-stress",
	"goldberg-mesh-circulation",
	"goto-torus",
	"grid-2d",
	"grid-3d",
	"gridgen-grid",
	"gridgraph-grid",
	"hall-tight-bipartite",
	"ladder",
	"layered-dag",
	"multi-source-sink",
	"netgen-skeleton",
	"parallel-paths",
	"path",
	"planar-triangulated",
	"planted-bottleneck",
	"preferential-attachment-directed",
	"random-dag",
	"random-geometric",
	"random-regular-directed",
	"rmfgen-frames",
	"strongly-connected",
	"torus",
	"transportation-table",
	"vision-segmentation-grid",
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
	"watts-strogatz-fixed",
	"zadeh-phase-chain-stress",
] as const;

export type FlowGeneratorFamilyId = (typeof FLOW_GENERATOR_FAMILY_IDS)[number];

export type FlowGeneratorFixtureModel =
	| "max-flow"
	| "circulation"
	| "transshipment"
	| "bipartite-matching"
	| "assignment"
	| "transportation"
	| "planar-max-flow";

/**
 * Canonical family-to-model contract mirrored from the Rust fixture registry.
 * Keeping this exhaustive makes a newly added family fail TypeScript checking
 * until its workspace ownership has been decided explicitly.
 */
export const FLOW_GENERATOR_FAMILY_MODELS = Object.freeze({
	arborescence: "max-flow",
	"assignment-matrix": "assignment",
	"bipartite-random": "max-flow",
	"cherkassky-goldberg-ak-stress": "max-flow",
	"clustered-directed": "max-flow",
	"complete-dag": "max-flow",
	cycle: "circulation",
	"diamond-chain": "max-flow",
	"dinic-worst-case": "max-flow",
	"erdos-renyi-directed": "max-flow",
	"glover-dense-acyclic-stress": "max-flow",
	"goldberg-mesh-circulation": "circulation",
	"goto-torus": "transshipment",
	"grid-2d": "max-flow",
	"grid-3d": "max-flow",
	"gridgen-grid": "transshipment",
	"gridgraph-grid": "transshipment",
	"hall-tight-bipartite": "bipartite-matching",
	ladder: "max-flow",
	"layered-dag": "max-flow",
	"multi-source-sink": "max-flow",
	"netgen-skeleton": "transshipment",
	"parallel-paths": "max-flow",
	path: "max-flow",
	"planar-triangulated": "planar-max-flow",
	"planted-bottleneck": "max-flow",
	"preferential-attachment-directed": "max-flow",
	"random-dag": "max-flow",
	"random-geometric": "max-flow",
	"random-regular-directed": "max-flow",
	"rmfgen-frames": "max-flow",
	"strongly-connected": "max-flow",
	torus: "circulation",
	"transportation-table": "transportation",
	"vision-segmentation-grid": "max-flow",
	"waissi-setubal-acyclic-dense": "max-flow",
	"waissi-transit-one-way-grid": "max-flow",
	"waissi-transit-two-way-grid": "max-flow",
	"washington-basic-line": "max-flow",
	"washington-cheriyan-stress": "max-flow",
	"washington-dinic-phase-stress": "max-flow",
	"washington-double-exponential-line": "max-flow",
	"washington-exponential-line": "max-flow",
	"washington-goldberg-fifo-stress": "max-flow",
	"washington-matching": "bipartite-matching",
	"washington-mesh": "max-flow",
	"washington-random-level": "max-flow",
	"washington-square-mesh": "max-flow",
	"watts-strogatz-fixed": "max-flow",
	"zadeh-phase-chain-stress": "max-flow",
} as const satisfies Readonly<
	Record<FlowGeneratorFamilyId, FlowGeneratorFixtureModel>
>);

export function flowGeneratorFamilyModel(
	family: FlowGeneratorFamilyId,
): FlowGeneratorFixtureModel {
	return FLOW_GENERATOR_FAMILY_MODELS[family];
}

const FAMILY_NAME_WORDS: Readonly<Record<string, string>> = {
	"2d": "2D",
	"3d": "3D",
	ak: "AK",
	dag: "DAG",
	dinic: "Dinic",
	erdos: "Erdős",
	fifo: "FIFO",
	glover: "Glover",
	goldberg: "Goldberg",
	goto: "GOTO",
	gridgen: "GRIDGEN",
	gridgraph: "GRIDGRAPH",
	netgen: "NETGEN",
	renyi: "Rényi",
	rmfgen: "RMFGEN",
	setubal: "Setubal",
	waissi: "Waissi",
};

export function flowGeneratorFamilyDisplayName(
	family: FlowGeneratorFamilyId,
): string {
	return family
		.split("-")
		.map(
			(word) =>
				FAMILY_NAME_WORDS[word] ??
				`${word.slice(0, 1).toUpperCase()}${word.slice(1)}`,
		)
		.join(" ");
}

export type FlowGeneratorFixturePreset = Readonly<{
	purpose: "trace" | "fast" | "boundary";
	label: string;
	recommended_run_profile: "trace" | "fast";
	spec: Readonly<{
		generator_revision: "flow-generator/27";
		seed: string;
		family: Readonly<Record<string, unknown>> & {
			readonly family_id: FlowGeneratorFamilyId;
		};
		capacity: Readonly<Record<string, unknown>>;
		cost: Readonly<Record<string, unknown>>;
		target_problem?: "max-flow" | "fixed-flow-min-cost";
	}>;
	expects_strict_difficulty_certificate: boolean;
	expected_counters: readonly Readonly<{
		algorithm_id: string;
		metric_id: string;
		exact_value: string;
		evidence:
			| "strict-certificate"
			| "finite-regression"
			| "structural-identity";
	}>[];
}>;

export type FlowGeneratorAlgorithmCompatibility = Readonly<{
	algorithm_id: string;
	state: "recommended" | "compatible" | "incompatible";
	reason: string;
}>;

export type FlowGeneratorFixture = Readonly<{
	family_id: FlowGeneratorFamilyId;
	title: string;
	purpose: string;
	model: FlowGeneratorFixtureModel;
	layout_class:
		| "linear-layered"
		| "radial-cyclic"
		| "grid-local"
		| "grid-periodic"
		| "partitioned"
		| "hierarchical"
		| "clustered"
		| "dense-spatial"
		| "benchmark-gadget";
	picker_group:
		| "structural"
		| "random"
		| "special"
		| "benchmark"
		| "stress"
		| "worst-case";
	origin: string;
	sampling: "deterministic" | "randomized";
	difficulty: "ordinary" | "stress" | "verified-worst-case";
	source_id: string;
	tags: readonly string[];
	presets: readonly FlowGeneratorFixturePreset[];
	algorithm_compatibility: readonly FlowGeneratorAlgorithmCompatibility[];
	default_algorithm_id: string;
	admission_note: string;
}>;

export type FlowGeneratorPickerGroup =
	| "all"
	| FlowGeneratorFixture["picker_group"];

export const FLOW_GENERATOR_PICKER_GROUPS = [
	"all",
	"structural",
	"random",
	"special",
	"benchmark",
	"stress",
	"worst-case",
] as const satisfies readonly FlowGeneratorPickerGroup[];

export const FLOW_GENERATOR_PICKER_GROUP_LABELS: Readonly<
	Record<FlowGeneratorPickerGroup, string>
> = Object.freeze({
	all: "All",
	structural: "Structural",
	random: "Random",
	special: "Specialized",
	benchmark: "Benchmark",
	stress: "Stress",
	"worst-case": "Verified worst case",
});

/**
 * Searches the closed generator manifest without inventing aliases outside the
 * reviewed fixture metadata. Whitespace-separated terms are combined with AND.
 */
export function filterFlowGeneratorFixtures(
	fixtures: readonly FlowGeneratorFixture[],
	query: string,
	group: FlowGeneratorPickerGroup = "all",
): FlowGeneratorFixture[] {
	const terms = query
		.trim()
		.toLocaleLowerCase()
		.split(/\s+/u)
		.filter((term) => term.length > 0);
	return fixtures.filter((fixture) => {
		if (group !== "all" && fixture.picker_group !== group) return false;
		if (terms.length === 0) return true;
		const searchable = [
			fixture.family_id,
			fixture.title,
			fixture.purpose,
			fixture.model,
			fixture.layout_class,
			fixture.picker_group,
			FLOW_GENERATOR_PICKER_GROUP_LABELS[fixture.picker_group],
			fixture.origin,
			fixture.sampling,
			fixture.difficulty,
			fixture.source_id,
			fixture.default_algorithm_id,
			fixture.admission_note,
			...fixture.tags,
		]
			.join("\n")
			.toLocaleLowerCase();
		return terms.every((term) => searchable.includes(term));
	});
}

const FAMILY_ID_SET = new Set<string>(FLOW_GENERATOR_FAMILY_IDS);
const MODELS = new Set([
	"max-flow",
	"circulation",
	"transshipment",
	"bipartite-matching",
	"assignment",
	"transportation",
	"planar-max-flow",
]);
const LAYOUT_CLASSES = new Set([
	"linear-layered",
	"radial-cyclic",
	"grid-local",
	"grid-periodic",
	"partitioned",
	"hierarchical",
	"clustered",
	"dense-spatial",
	"benchmark-gadget",
]);
const PICKER_GROUPS = new Set([
	"structural",
	"random",
	"special",
	"benchmark",
	"stress",
	"worst-case",
]);
const COMPATIBILITY_STATES = new Set([
	"recommended",
	"compatible",
	"incompatible",
]);
const COUNTER_EVIDENCE = new Set([
	"strict-certificate",
	"finite-regression",
	"structural-identity",
]);

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(
	value: Record<string, unknown>,
	keys: readonly string[],
): boolean {
	const actual = Object.keys(value).sort();
	const expected = [...keys].sort();
	return (
		actual.length === expected.length &&
		actual.every((key, index) => key === expected[index])
	);
}

function nonemptyString(value: unknown): value is string {
	return typeof value === "string" && value.length > 0;
}

function canonicalSlug(value: unknown): value is string {
	return (
		typeof value === "string" &&
		value.length <= 96 &&
		/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value)
	);
}

function decodeCounter(
	value: unknown,
): FlowGeneratorFixturePreset["expected_counters"][number] {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"algorithm_id",
			"metric_id",
			"exact_value",
			"evidence",
		]) ||
		!canonicalSlug(value.algorithm_id) ||
		!nonemptyString(value.metric_id) ||
		typeof value.exact_value !== "string" ||
		!/^(0|[1-9][0-9]*)$/.test(value.exact_value) ||
		typeof value.evidence !== "string" ||
		!COUNTER_EVIDENCE.has(value.evidence)
	) {
		throw new Error("Flow generator fixture counter is invalid");
	}
	return value as FlowGeneratorFixturePreset["expected_counters"][number];
}

function decodePreset(
	value: unknown,
	familyId: FlowGeneratorFamilyId,
	expectedPurpose: FlowGeneratorFixturePreset["purpose"],
): FlowGeneratorFixturePreset {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"purpose",
			"label",
			"recommended_run_profile",
			"spec",
			"expects_strict_difficulty_certificate",
			"expected_counters",
		]) ||
		value.purpose !== expectedPurpose ||
		!nonemptyString(value.label) ||
		(value.recommended_run_profile !== "trace" &&
			value.recommended_run_profile !== "fast") ||
		typeof value.expects_strict_difficulty_certificate !== "boolean" ||
		!Array.isArray(value.expected_counters) ||
		!isRecord(value.spec) ||
		!hasExactKeys(value.spec, [
			"generator_revision",
			"seed",
			"family",
			"capacity",
			"cost",
		]) ||
		value.spec.generator_revision !== "flow-generator/27" ||
		typeof value.spec.seed !== "string" ||
		!/^(0|[1-9][0-9]*)$/.test(value.spec.seed) ||
		!isRecord(value.spec.family) ||
		value.spec.family.family_id !== familyId ||
		!isRecord(value.spec.capacity) ||
		!isRecord(value.spec.cost)
	) {
		throw new Error(`Flow generator fixture preset is invalid: ${familyId}`);
	}
	return {
		purpose: expectedPurpose,
		label: value.label,
		recommended_run_profile:
			value.recommended_run_profile as FlowGeneratorFixturePreset["recommended_run_profile"],
		spec: value.spec as FlowGeneratorFixturePreset["spec"],
		expects_strict_difficulty_certificate:
			value.expects_strict_difficulty_certificate,
		expected_counters: value.expected_counters.map(decodeCounter),
	};
}

function decodeCompatibility(
	value: unknown,
): FlowGeneratorAlgorithmCompatibility {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["algorithm_id", "state", "reason"]) ||
		!canonicalSlug(value.algorithm_id) ||
		typeof value.state !== "string" ||
		!COMPATIBILITY_STATES.has(value.state) ||
		!nonemptyString(value.reason)
	) {
		throw new Error("Flow generator algorithm compatibility is invalid");
	}
	return value as FlowGeneratorAlgorithmCompatibility;
}

function decodeFixture(
	value: unknown,
	expectedFamilyId: FlowGeneratorFamilyId,
): FlowGeneratorFixture {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"family_id",
			"title",
			"purpose",
			"model",
			"layout_class",
			"picker_group",
			"origin",
			"sampling",
			"difficulty",
			"source_id",
			"tags",
			"presets",
			"algorithm_compatibility",
			"default_algorithm_id",
			"admission_note",
		]) ||
		value.family_id !== expectedFamilyId ||
		!nonemptyString(value.title) ||
		!nonemptyString(value.purpose) ||
		typeof value.model !== "string" ||
		!MODELS.has(value.model) ||
		value.model !== flowGeneratorFamilyModel(expectedFamilyId) ||
		typeof value.layout_class !== "string" ||
		!LAYOUT_CLASSES.has(value.layout_class) ||
		typeof value.picker_group !== "string" ||
		!PICKER_GROUPS.has(value.picker_group) ||
		!nonemptyString(value.origin) ||
		(value.sampling !== "deterministic" && value.sampling !== "randomized") ||
		(value.difficulty !== "ordinary" &&
			value.difficulty !== "stress" &&
			value.difficulty !== "verified-worst-case") ||
		!nonemptyString(value.source_id) ||
		!Array.isArray(value.tags) ||
		!value.tags.every(nonemptyString) ||
		!Array.isArray(value.presets) ||
		value.presets.length !== 3 ||
		!Array.isArray(value.algorithm_compatibility) ||
		value.algorithm_compatibility.length === 0 ||
		!canonicalSlug(value.default_algorithm_id) ||
		!nonemptyString(value.admission_note)
	) {
		throw new Error(`Flow generator fixture is invalid: ${expectedFamilyId}`);
	}
	const compatibility = value.algorithm_compatibility.map(decodeCompatibility);
	if (
		new Set(compatibility.map((entry) => entry.algorithm_id)).size !==
		compatibility.length
	) {
		throw new Error(
			`Flow generator compatibility IDs repeat: ${expectedFamilyId}`,
		);
	}
	if (!compatibility.some((entry) => entry.state === "recommended")) {
		throw new Error(
			`Flow generator fixture has no recommendation: ${expectedFamilyId}`,
		);
	}
	if (
		!compatibility.some(
			(entry) =>
				entry.algorithm_id === value.default_algorithm_id &&
				entry.state === "recommended",
		)
	) {
		throw new Error(
			`Flow generator default algorithm is not recommended: ${expectedFamilyId}`,
		);
	}
	const presets = value.presets as unknown[];
	return {
		family_id: expectedFamilyId,
		title: value.title,
		purpose: value.purpose,
		model: value.model as FlowGeneratorFixture["model"],
		layout_class: value.layout_class as FlowGeneratorFixture["layout_class"],
		picker_group: value.picker_group as FlowGeneratorFixture["picker_group"],
		origin: value.origin,
		sampling: value.sampling,
		difficulty: value.difficulty,
		source_id: value.source_id,
		tags: value.tags,
		presets: ["trace", "fast", "boundary"].map((purpose, index) =>
			decodePreset(
				presets[index],
				expectedFamilyId,
				purpose as FlowGeneratorFixturePreset["purpose"],
			),
		),
		algorithm_compatibility: compatibility,
		default_algorithm_id: value.default_algorithm_id,
		admission_note: value.admission_note,
	};
}

export function decodeFlowGeneratorFixtureManifest(
	source: string,
): FlowGeneratorFixture[] {
	const value: unknown = JSON.parse(source);
	if (
		!Array.isArray(value) ||
		value.length !== FLOW_GENERATOR_FAMILY_IDS.length
	) {
		throw new Error(
			"Flow generator fixture manifest must contain exactly 50 families",
		);
	}
	const fixtures = FLOW_GENERATOR_FAMILY_IDS.map((familyId, index) =>
		decodeFixture(value[index], familyId),
	);
	if (!fixtures.every((fixture) => FAMILY_ID_SET.has(fixture.family_id))) {
		throw new Error(
			"Flow generator fixture manifest contains an unknown family",
		);
	}
	return fixtures;
}

export function flowGeneratorFixtureKind(
	fixture: FlowGeneratorFixture,
):
	| "Structural"
	| "Random"
	| "Specialized"
	| "Benchmark"
	| "Stress"
	| "Worst case" {
	switch (fixture.picker_group) {
		case "structural":
			return "Structural";
		case "random":
			return "Random";
		case "special":
			return "Specialized";
		case "benchmark":
			return "Benchmark";
		case "stress":
			return "Stress";
		case "worst-case":
			return "Worst case";
	}
}
