import {
	FLOW_GENERATOR_FAMILY_IDS,
	type FlowGeneratorFamilyId,
} from "./flow-generator-fixture";
import {
	type AssignmentMatrixShapeId,
	DEFAULT_FLOW_GENERATOR,
	type FlowGeneratorForm,
	type GridgraphPresetId,
	type NetgenPresetId,
	type TransportationTableShapeId,
	type WashingtonMatchingPresetId,
	type WashingtonPresetId,
	type WashingtonSquareMeshPresetId,
} from "./flow-generator-form";
import {
	classifyNetgenForm,
	type NetgenProblemKind,
} from "./flow-netgen-classification";

export type { FlowGeneratorFamilyId } from "./flow-generator-fixture";
export {
	type AssignmentMatrixShapeId,
	DEFAULT_FLOW_GENERATOR,
	type FlowGeneratorForm,
	type GridgraphPresetId,
	type NetgenPresetId,
	type TransportationTableShapeId,
	type WashingtonMatchingPresetId,
	type WashingtonPresetId,
	type WashingtonSquareMeshPresetId,
} from "./flow-generator-form";
export { classifyNetgenForm } from "./flow-netgen-classification";

const ASSIGNMENT_SHAPE_OPTIONS: readonly {
	id: AssignmentMatrixShapeId;
	label: string;
	detail: string;
}[] = [
	{
		id: "uniform",
		label: "Uniform random",
		detail: "exact density + uniform costs",
	},
	{ id: "equal", label: "Equal costs", detail: "observe tie-breaking" },
	{ id: "block", label: "Block", detail: "within- vs cross-group costs" },
	{ id: "near-tie", label: "Near tie", detail: "near-optimal alternatives" },
	{
		id: "planted-optimum",
		label: "Planted unique optimum",
		detail: "sparse allowed edges + known optimum",
	},
	{ id: "monge", label: "Monge", detail: "s·|i−j|" },
	{ id: "anti-monge", label: "Anti-Monge", detail: "−s·|i−j|" },
	{
		id: "sparse-allowed",
		label: "Fixed-degree sparse",
		detail: "not conditioned on feasibility",
	},
	{
		id: "hall-deficient",
		label: "Hall deficient",
		detail: "exact infeasibility witness",
	},
];

const TRANSPORTATION_SHAPE_OPTIONS: readonly {
	id: TransportationTableShapeId;
	label: string;
	detail: string;
}[] = [
	{
		id: "dense-uniform",
		label: "Dense random",
		detail: "all routes + uniform costs",
	},
	{
		id: "sparse-feasible",
		label: "Sparse feasible",
		detail: "always preserves feasible support",
	},
	{
		id: "unit-degenerate",
		label: "Unit degeneracy",
		detail: "unit balance + equal cost + zero basic cells",
	},
	{ id: "block", label: "Block", detail: "within- vs cross-group costs" },
	{ id: "near-tie", label: "Near tie", detail: "near-cost alternative routes" },
	{ id: "monge", label: "Monge", detail: "s·|i−j|" },
	{
		id: "cut-infeasible",
		label: "Infeasible supply-demand cut",
		detail: "generates an independent cut witness",
	},
];

const NETGEN_PRESET_OPTIONS: readonly {
	id: Exclude<NetgenPresetId, "custom">;
	label: string;
}[] = [
	{ id: "general-min-cost", label: "General min-cost · 3 supply / 4 demand" },
	{ id: "transportation", label: "Transportation · 12 × 12" },
	{ id: "assignment", label: "Assignment · 12 × 12" },
	{ id: "single-source-max-flow", label: "Single-source s–t max flow" },
	{ id: "dense-transshipment", label: "Dense transshipment" },
];

const GRIDGRAPH_PRESET_OPTIONS: readonly {
	id: Exclude<GridgraphPresetId, "custom">;
	label: string;
}[] = [
	{ id: "readable", label: "Readable · 6 rows × 8 columns" },
	{ id: "square", label: "GRID-SQUARE · 24 × 24" },
	{ id: "wide", label: "GRID-WIDE · 48 rows × 16 columns" },
	{ id: "long", label: "GRID-LONG · 16 rows × 48 columns" },
];

const WASHINGTON_PRESET_OPTIONS: readonly {
	id: Exclude<WashingtonPresetId, "custom">;
	label: string;
}[] = [
	{ id: "readable", label: "Readable · 6 vertices/level × 8 levels" },
	{ id: "wide", label: "Wide · 6 × 24" },
	{ id: "tall", label: "Tall · 24 × 6" },
	{ id: "dense", label: "Dense · 32 × 32" },
];

const WASHINGTON_MATCHING_PRESET_OPTIONS: readonly {
	id: Exclude<WashingtonMatchingPresetId, "custom">;
	label: string;
}[] = [
	{ id: "readable", label: "Readable · 12 × 12 / degree 3" },
	{ id: "sparse", label: "Sparse · 64 × 64 / degree 2" },
	{ id: "medium", label: "Medium · 96 × 96 / degree 8" },
	{ id: "dense", label: "Dense · 48 × 48 / degree 32" },
];

const WASHINGTON_SQUARE_MESH_PRESET_OPTIONS: readonly {
	id: Exclude<WashingtonSquareMeshPresetId, "custom">;
	label: string;
}[] = [
	{ id: "readable", label: "Readable · 6 × 6 / degree 3" },
	{ id: "sparse", label: "Sparse · 18 × 18 / degree 2" },
	{ id: "medium", label: "Medium · 24 × 24 / degree 6" },
	{ id: "dense", label: "Dense · 27 × 27 / degree 27" },
];

export function applyGridgraphPreset(
	form: FlowGeneratorForm,
	preset: Exclude<GridgraphPresetId, "custom">,
): FlowGeneratorForm {
	const dimensions: Record<
		Exclude<GridgraphPresetId, "custom">,
		readonly [rows: number, columns: number]
	> = {
		readable: [6, 8],
		square: [24, 24],
		wide: [48, 16],
		long: [16, 48],
	};
	const [rows, columns] = dimensions[preset];
	return {
		...form,
		family: "gridgraph-grid",
		gridgraphPreset: preset,
		primary: rows,
		secondary: columns,
		capacityKind: "unit",
		costKind: "zero",
		capacityMaximum: "1000",
		costMaximum: "10000",
	};
}

export function applyWashingtonPreset(
	form: FlowGeneratorForm,
	preset: Exclude<WashingtonPresetId, "custom">,
	family:
		| "washington-mesh"
		| "washington-random-level" = "washington-random-level",
): FlowGeneratorForm {
	const dimensions: Record<
		Exclude<WashingtonPresetId, "custom">,
		readonly [rows: number, columns: number]
	> = {
		readable: [6, 8],
		wide: [6, 24],
		tall: [24, 6],
		dense: [32, 32],
	};
	const [rows, columns] = dimensions[preset];
	return {
		...form,
		family,
		washingtonPreset: preset,
		primary: rows,
		secondary: columns,
		capacityKind: "unit",
		costKind: "zero",
		capacityMaximum: "100",
	};
}

export function applyWashingtonMatchingPreset(
	form: FlowGeneratorForm,
	preset: Exclude<WashingtonMatchingPresetId, "custom">,
): FlowGeneratorForm {
	const parameters: Record<
		Exclude<WashingtonMatchingPresetId, "custom">,
		readonly [partSize: number, degree: number]
	> = {
		readable: [12, 3],
		sparse: [64, 2],
		medium: [96, 8],
		dense: [48, 32],
	};
	const [partSize, degree] = parameters[preset];
	return {
		...form,
		family: "washington-matching",
		washingtonMatchingPreset: preset,
		primary: partSize,
		secondary: degree,
		capacityKind: "unit",
		costKind: "zero",
	};
}

export function applyWashingtonSquareMeshPreset(
	form: FlowGeneratorForm,
	preset: Exclude<WashingtonSquareMeshPresetId, "custom">,
): FlowGeneratorForm {
	const parameters: Record<
		Exclude<WashingtonSquareMeshPresetId, "custom">,
		readonly [dimension: number, degree: number]
	> = {
		readable: [6, 3],
		sparse: [18, 2],
		medium: [24, 6],
		dense: [27, 27],
	};
	const [dimension, degree] = parameters[preset];
	return {
		...form,
		family: "washington-square-mesh",
		washingtonSquareMeshPreset: preset,
		primary: dimension,
		secondary: degree,
		capacityKind: "unit",
		costKind: "zero",
		capacityMaximum: "100",
	};
}

export function applyNetgenPreset(
	form: FlowGeneratorForm,
	preset: Exclude<NetgenPresetId, "custom">,
): FlowGeneratorForm {
	const common = {
		...form,
		family: "netgen-skeleton" as const,
		netgenPreset: preset,
		capacityKind: "unit" as const,
		costKind: "zero" as const,
	};
	switch (preset) {
		case "general-min-cost":
			return {
				...common,
				primary: 24,
				secondary: 3,
				tertiary: 4,
				quaternary: 80,
				netgenTotalSupply: "60",
				netgenTransshipmentSources: 1,
				netgenTransshipmentSinks: 1,
				netgenHighCostPercentage: 75,
				netgenCapacitatedPercentage: 65,
				capacityMinimum: "2",
				capacityMaximum: "30",
				costMinimum: "-5",
				costMaximum: "20",
			};
		case "transportation":
			return {
				...common,
				primary: 24,
				secondary: 12,
				tertiary: 12,
				quaternary: 96,
				netgenTotalSupply: "120",
				netgenTransshipmentSources: 0,
				netgenTransshipmentSinks: 0,
				netgenHighCostPercentage: 40,
				netgenCapacitatedPercentage: 60,
				capacityMinimum: "2",
				capacityMaximum: "40",
				costMinimum: "0",
				costMaximum: "20",
			};
		case "assignment":
			return {
				...common,
				primary: 24,
				secondary: 12,
				tertiary: 12,
				quaternary: 96,
				netgenTotalSupply: "12",
				netgenTransshipmentSources: 0,
				netgenTransshipmentSinks: 0,
				netgenHighCostPercentage: 0,
				netgenCapacitatedPercentage: 0,
				capacityMinimum: "0",
				capacityMaximum: "0",
				costMinimum: "0",
				costMaximum: "20",
			};
		case "single-source-max-flow":
			return {
				...common,
				primary: 30,
				secondary: 1,
				tertiary: 1,
				quaternary: 100,
				netgenTotalSupply: "100",
				netgenTransshipmentSources: 0,
				netgenTransshipmentSinks: 0,
				netgenHighCostPercentage: 0,
				netgenCapacitatedPercentage: 80,
				capacityMinimum: "2",
				capacityMaximum: "50",
				costMinimum: "1",
				costMaximum: "1",
			};
		case "dense-transshipment":
			return {
				...common,
				primary: 40,
				secondary: 4,
				tertiary: 8,
				quaternary: 1_000,
				netgenTotalSupply: "200",
				netgenTransshipmentSources: 1,
				netgenTransshipmentSinks: 1,
				netgenHighCostPercentage: 85,
				netgenCapacitatedPercentage: 70,
				capacityMinimum: "2",
				capacityMaximum: "100",
				costMinimum: "-20",
				costMaximum: "100",
			};
	}
}

type Estimate = { nodes: bigint; edges: bigint };

function nonnegativeBigInt(value: number): bigint {
	return Number.isSafeInteger(value) && value >= 0 ? BigInt(value) : 0n;
}

export function estimateFlowGenerator(form: FlowGeneratorForm): Estimate {
	return flowGeneratorFamilyEntry(form.family).estimate(form);
}

function canonicalInteger(
	value: string,
	minimum: bigint,
	maximum: bigint,
): bigint | undefined {
	if (!/^(0|-?[1-9][0-9]*)$/.test(value)) return undefined;
	try {
		const parsed = BigInt(value);
		return parsed >= minimum && parsed <= maximum ? parsed : undefined;
	} catch {
		return undefined;
	}
}

function bigintOutside(
	value: bigint,
	minimum: bigint,
	maximum: bigint,
): boolean {
	return value < minimum || value > maximum;
}

function validateNetgenForm(form: FlowGeneratorForm): string | undefined {
	if (
		![
			form.netgenTransshipmentSources,
			form.netgenTransshipmentSinks,
			form.netgenHighCostPercentage,
			form.netgenCapacitatedPercentage,
		].every((value) => Number.isSafeInteger(value) && value >= 0)
	) {
		return "NETGEN counts and percentages must be nonnegative safe integers";
	}
	const nodes = BigInt(form.primary);
	const sources = BigInt(form.secondary);
	const sinks = BigInt(form.tertiary);
	const edges = BigInt(form.quaternary);
	const transshipmentSources = BigInt(form.netgenTransshipmentSources);
	const transshipmentSinks = BigInt(form.netgenTransshipmentSinks);
	if (nodes < 2n || nodes > 10_000n)
		return "NETGEN vertex count N must be between 2 and 10,000";
	if (sources < 1n || sinks < 1n || sources + sinks > nodes)
		return "Source and sink counts must each be at least 1 and total at most N";
	if (edges < nodes || edges > 100_000n)
		return "NETGEN edge count M must be between N and 100,000";
	if (transshipmentSources > sources || transshipmentSinks > sinks)
		return "Transshipment source and sink counts cannot exceed their terminal counts";
	if (
		form.netgenHighCostPercentage > 100 ||
		form.netgenCapacitatedPercentage > 100
	) {
		return "High-cost and capacitated percentages must be between 0% and 100%";
	}
	const totalSupply = canonicalInteger(
		form.netgenTotalSupply,
		1n,
		1_000_000_000n,
	);
	if (
		totalSupply === undefined ||
		totalSupply < (sources > sinks ? sources : sinks)
	)
		return "Total supply B must be between max(S,T) and 1,000,000,000";
	const minimumCost = canonicalInteger(
		form.costMinimum,
		-1_000_000_000n,
		1_000_000_000n,
	);
	const maximumCost = canonicalInteger(
		form.costMaximum,
		-1_000_000_000n,
		1_000_000_000n,
	);
	if (
		minimumCost === undefined ||
		maximumCost === undefined ||
		minimumCost > maximumCost
	)
		return "NETGEN costs must satisfy −1,000,000,000 ≤ c1 ≤ c2 ≤ 1,000,000,000";
	const minimumCapacity = canonicalInteger(
		form.capacityMinimum,
		0n,
		1_000_000_000n,
	);
	const maximumCapacity = canonicalInteger(
		form.capacityMaximum,
		0n,
		1_000_000_000n,
	);
	if (
		minimumCapacity === undefined ||
		maximumCapacity === undefined ||
		minimumCapacity > maximumCapacity
	) {
		return "NETGEN capacities must satisfy 0 ≤ u1 ≤ u2 ≤ 1,000,000,000";
	}

	const kind = classifyNetgenForm(form);
	if (kind === "max-flow" && (sources !== 1n || sinks !== 1n))
		return "A unit-cost NETGEN max-flow instance requires exactly one source and one sink";
	const middle = nodes - sources - sinks;
	const sinksPerSource =
		kind === "assignment"
			? 1n
			: (2n * sinks + sources - 1n) / sources < sinks
				? (2n * sinks + sources - 1n) / sources
				: sinks;
	const skeletonEdges =
		kind === "assignment" ? sources : middle + sources * sinksPerSource;
	if (edges < skeletonEdges)
		return `Edge count M must be at least the ${skeletonEdges.toString()} edges required by the feasible skeleton`;
	const pureSources = sources - transshipmentSources;
	const tailCount = nodes - sinks + transshipmentSinks;
	const headCount = nodes - pureSources;
	const allowedEdges =
		kind === "assignment"
			? sources * sinks
			: tailCount * headCount - (tailCount - pureSources);
	if (edges > allowedEdges)
		return `Edge count M cannot exceed the ${allowedEdges.toString()} allowed simple directed edges`;
	return undefined;
}

function validateFlowDistributions(
	form: FlowGeneratorForm,
): string | undefined {
	if (form.capacityKind === "power-of-two-buckets") {
		const minimumExponent = canonicalInteger(form.capacityMinimum, 0n, 63n);
		const maximumExponent = canonicalInteger(form.capacityMaximum, 0n, 63n);
		if (
			minimumExponent === undefined ||
			maximumExponent === undefined ||
			minimumExponent > maximumExponent
		) {
			return "Power-of-two bucket exponents must satisfy 0 ≤ min ≤ max ≤ 63";
		}
	} else if (form.capacityKind !== "unit") {
		const first = canonicalInteger(
			form.capacityMinimum,
			0n,
			18_446_744_073_709_551_615n,
		);
		const second =
			form.capacityKind === "constant"
				? first
				: canonicalInteger(
						form.capacityMaximum,
						0n,
						18_446_744_073_709_551_615n,
					);
		if (
			first === undefined ||
			second === undefined ||
			(form.capacityKind === "uniform" && first > second)
		) {
			return "Capacities must be canonical unsigned 64-bit integers; uniform ranges require min ≤ max";
		}
		if (form.capacityKind === "bimodal" && first === second) {
			return "Bimodal capacity atoms A and B must differ";
		}
	}
	if (form.costKind !== "zero") {
		const first = canonicalInteger(
			form.costMinimum,
			-9_223_372_036_854_775_808n,
			9_223_372_036_854_775_807n,
		);
		const second =
			form.costKind === "constant"
				? first
				: canonicalInteger(
						form.costMaximum,
						-9_223_372_036_854_775_808n,
						9_223_372_036_854_775_807n,
					);
		if (
			first === undefined ||
			second === undefined ||
			((form.costKind === "uniform" ||
				form.costKind === "capacity-correlated") &&
				first > second)
		) {
			return "Costs must be canonical signed 64-bit integers; ranges require min ≤ max";
		}
		if (form.costKind === "bimodal" && first === second) {
			return "Bimodal cost atoms A and B must differ";
		}
		if (
			form.costKind === "capacity-correlated" &&
			canonicalInteger(
				form.costMaximumJitter,
				0n,
				9_223_372_036_854_775_807n,
			) === undefined
		) {
			return "Maximum correlated-cost jitter must be a canonical nonnegative 64-bit integer";
		}
	}
	return undefined;
}

function validateFlowGeneratorFormFromRegistry(
	form: FlowGeneratorForm,
): string | undefined {
	if (
		![form.primary, form.secondary, form.tertiary, form.quaternary].every(
			(value) => Number.isSafeInteger(value) && value >= 0,
		)
	) {
		return "Shape parameters must be nonnegative safe integers";
	}
	if (
		canonicalInteger(form.seed, 0n, 18_446_744_073_709_551_615n) === undefined
	) {
		return "Seed must be a canonical unsigned 64-bit integer";
	}
	const a = BigInt(form.primary);
	const b = BigInt(form.secondary);
	const c = BigInt(form.tertiary);
	const d = BigInt(form.quaternary);
	const entry = flowGeneratorFamilyEntry(form.family);
	const familyError = entry.validate(form, a, b, c, d);
	if (familyError !== undefined) return familyError;
	const estimate = entry.estimate(form);
	if (estimate.nodes > 10_000n || estimate.edges > 100_000n) {
		return "This graph exceeds the 10,000-vertex or 100,000-edge limit";
	}
	return entry.skipDistributionValidation
		? undefined
		: validateFlowDistributions(form);
}

export function validateFlowGeneratorForm(
	form: FlowGeneratorForm,
): string | undefined {
	return validateFlowGeneratorFormFromRegistry(form);
}

function validateAssignmentMatrixForm(
	form: FlowGeneratorForm,
	agents: bigint,
	tasks: bigint,
	parameter: bigint,
	secondaryParameter: bigint,
): string | undefined {
	if (agents < 1n || tasks < 1n)
		return "Agent and task counts must be at least 1";
	if (agents + tasks > 2_000n)
		return "This graph exceeds the 2,000-vertex Hungarian limit";
	if (agents * agents * tasks > 20_000_000n)
		return "This matrix exceeds the conservative Hungarian scan limit a²t = 20,000,000";
	const minimumCost = canonicalInteger(
		form.costMinimum,
		-1_000_000_000n,
		1_000_000_000n,
	);
	const maximumCost = canonicalInteger(
		form.costMaximum,
		-1_000_000_000n,
		1_000_000_000n,
	);
	const intervalInvalid =
		minimumCost === undefined ||
		maximumCost === undefined ||
		minimumCost > maximumCost;
	switch (form.assignmentShape) {
		case "uniform":
			if (parameter > 1_000n) return "Density must be between 0 and 1,000‰";
			if (intervalInvalid)
				return "Costs must satisfy −1,000,000,000 ≤ min ≤ max ≤ 1,000,000,000";
			break;
		case "equal":
			if (minimumCost === undefined)
				return "Common cost must be a safe integer";
			break;
		case "block":
			if (parameter < 1n) return "Block count must be at least 1";
			if (intervalInvalid)
				return "Within-group and cross-group costs must be safe integers";
			break;
		case "near-tie":
			if (parameter < 1n) return "Near-tie gap must be at least 1";
			if (minimumCost === undefined) return "Base cost must be a safe integer";
			break;
		case "planted-optimum": {
			if (tasks < agents)
				return "A planted optimum requires at least as many tasks as agents";
			if (parameter < 1n || parameter > 1_000n)
				return "Density must be between 1 and 1,000‰";
			if (secondaryParameter < 1n)
				return "The gap between optimum and distractor edges must be at least 1";
			if (
				!Number.isSafeInteger(form.assignmentNoise) ||
				form.assignmentNoise < 0
			)
				return "Noise must be a nonnegative safe integer";
			if (minimumCost === undefined) return "Base cost must be a safe integer";
			const extent = secondaryParameter + BigInt(form.assignmentNoise);
			const realized =
				form.assignmentObjective === "minimize"
					? minimumCost + extent
					: minimumCost - extent;
			if (realized < -1_000_000_000n || realized > 1_000_000_000n)
				return "Keep base cost ± gap ± noise within ±1,000,000,000";
			break;
		}
		case "monge":
		case "anti-monge":
			if (parameter < 1n) return "Scale must be at least 1";
			break;
		case "sparse-allowed":
			if (parameter > tasks)
				return "Each agent's degree cannot exceed the task count";
			if (intervalInvalid)
				return "Costs must satisfy −1,000,000,000 ≤ min ≤ max ≤ 1,000,000,000";
			break;
		case "hall-deficient":
			if (parameter < 1n || parameter > agents)
				return "Hall-witness agent prefix must be between 1 and the agent count";
			if (secondaryParameter >= parameter || secondaryParameter > tasks)
				return "Hall-witness task neighborhood must be smaller than the agent prefix and no larger than the task count";
			if (intervalInvalid)
				return "Costs must satisfy −1,000,000,000 ≤ min ≤ max ≤ 1,000,000,000";
			break;
	}
	const estimate = estimateFlowGenerator(form);
	if (estimate.edges > 20_000n)
		return "This graph exceeds the 20,000-edge Hungarian limit";
	return undefined;
}

function validateTransportationTableForm(
	form: FlowGeneratorForm,
	origins: bigint,
	destinations: bigint,
	totalSupply: bigint,
	parameter: bigint,
): string | undefined {
	if (origins < 1n || destinations < 1n)
		return "Origin and destination counts must be at least 1";
	if (origins + destinations > 256n || origins * destinations > 2_048n)
		return "This table exceeds the transportation limit of 256 vertices or 2,048 routes";
	if (
		form.transportationShape === "unit-degenerate" &&
		(origins !== destinations || totalSupply !== origins)
	)
		return "Unit degeneracy requires origins = destinations = B";
	if (
		form.transportationShape === "cut-infeasible" &&
		(origins < 2n ||
			destinations < 2n ||
			totalSupply < (origins + 1n > destinations ? origins + 1n : destinations))
	)
		return "The infeasible-cut shape requires at least 2 vertices per partition and B ≥ max(origins+1,destinations)";
	if (
		totalSupply < (origins > destinations ? origins : destinations) ||
		totalSupply > 1_000_000_000n
	)
		return "Total shipment B must be between max(origin,destination) and 1,000,000,000";
	const minimumCost = canonicalInteger(
		form.costMinimum,
		-1_000_000_000n,
		1_000_000_000n,
	);
	const maximumCost = canonicalInteger(
		form.costMaximum,
		-1_000_000_000n,
		1_000_000_000n,
	);
	const intervalInvalid =
		minimumCost === undefined ||
		maximumCost === undefined ||
		minimumCost > maximumCost;
	switch (form.transportationShape) {
		case "dense-uniform":
		case "sparse-feasible":
		case "cut-infeasible":
			if (intervalInvalid)
				return "Costs must satisfy −1,000,000,000 ≤ min ≤ max ≤ 1,000,000,000";
			if (
				form.transportationShape === "sparse-feasible" &&
				(parameter < 1n || parameter > 1_000n)
			)
				return "Route density must be between 1 and 1,000‰";
			break;
		case "unit-degenerate":
			if (minimumCost === undefined)
				return "Common cost must be a safe integer";
			break;
		case "block":
			if (parameter < 1n) return "Block count must be at least 1";
			if (minimumCost === undefined || maximumCost === undefined)
				return "Within-group and cross-group costs must be safe integers";
			break;
		case "near-tie":
			if (parameter < 1n) return "Near-tie gap must be at least 1";
			if (minimumCost === undefined) return "Base cost must be a safe integer";
			if (minimumCost + parameter > 1_000_000_000n)
				return "Base cost + gap cannot exceed 1,000,000,000";
			break;
		case "monge":
			if (parameter < 1n) return "Monge scale must be at least 1";
			break;
	}
	return undefined;
}

export type FlowGeneratorFieldKey =
	| "seed"
	| "primary"
	| "secondary"
	| "tertiary"
	| "quaternary"
	| "assignmentNoise"
	| "gridgenTotalSupply"
	| "netgenTotalSupply"
	| "netgenTransshipmentSources"
	| "netgenTransshipmentSinks"
	| "netgenHighCostPercentage"
	| "netgenCapacitatedPercentage"
	| "costMinimum"
	| "costMaximum"
	| "capacityMinimum"
	| "capacityMaximum";

function netgenFieldInvalid(
	form: FlowGeneratorForm,
	field: FlowGeneratorFieldKey,
	a: bigint,
	b: bigint,
	c: bigint,
	d: bigint,
): boolean {
	const sourcesAndSinksInvalid = b < 1n || c < 1n || b + c > a;
	const kind = classifyNetgenForm(form);
	const middle = a - b - c;
	const transshipmentSourcesValid =
		Number.isSafeInteger(form.netgenTransshipmentSources) &&
		form.netgenTransshipmentSources >= 0;
	const transshipmentSinksValid =
		Number.isSafeInteger(form.netgenTransshipmentSinks) &&
		form.netgenTransshipmentSinks >= 0;
	const transshipmentSources = transshipmentSourcesValid
		? BigInt(form.netgenTransshipmentSources)
		: 0n;
	const transshipmentSinks = transshipmentSinksValid
		? BigInt(form.netgenTransshipmentSinks)
		: 0n;
	const sinksPerSource =
		kind === "assignment" || b === 0n
			? 1n
			: (2n * c + b - 1n) / b < c
				? (2n * c + b - 1n) / b
				: c;
	const skeletonEdges = kind === "assignment" ? b : middle + b * sinksPerSource;
	const pureSources = b - transshipmentSources;
	const tailCount = a - c + transshipmentSinks;
	const headCount = a - pureSources;
	const allowedEdges =
		kind === "assignment"
			? b * c
			: tailCount * headCount - (tailCount - pureSources);

	switch (field) {
		case "primary":
			return bigintOutside(a, 2n, 10_000n) || sourcesAndSinksInvalid;
		case "secondary":
		case "tertiary":
			return (
				sourcesAndSinksInvalid ||
				(kind === "max-flow" && (b !== 1n || c !== 1n))
			);
		case "quaternary":
			return (
				bigintOutside(d, a, 100_000n) || d < skeletonEdges || d > allowedEdges
			);
		case "netgenTotalSupply": {
			const total = canonicalInteger(
				form.netgenTotalSupply,
				1n,
				1_000_000_000n,
			);
			return total === undefined || total < (b > c ? b : c);
		}
		case "netgenTransshipmentSources":
			return !transshipmentSourcesValid || transshipmentSources > b;
		case "netgenTransshipmentSinks":
			return !transshipmentSinksValid || transshipmentSinks > c;
		case "netgenHighCostPercentage":
			return (
				!Number.isSafeInteger(form.netgenHighCostPercentage) ||
				form.netgenHighCostPercentage < 0 ||
				form.netgenHighCostPercentage > 100
			);
		case "netgenCapacitatedPercentage":
			return (
				!Number.isSafeInteger(form.netgenCapacitatedPercentage) ||
				form.netgenCapacitatedPercentage < 0 ||
				form.netgenCapacitatedPercentage > 100
			);
		case "costMinimum":
		case "costMaximum": {
			const minimum = canonicalInteger(
				form.costMinimum,
				-1_000_000_000n,
				1_000_000_000n,
			);
			const maximum = canonicalInteger(
				form.costMaximum,
				-1_000_000_000n,
				1_000_000_000n,
			);
			return (
				minimum === undefined || maximum === undefined || minimum > maximum
			);
		}
		case "capacityMinimum":
		case "capacityMaximum": {
			const minimum = canonicalInteger(
				form.capacityMinimum,
				0n,
				1_000_000_000n,
			);
			const maximum = canonicalInteger(
				form.capacityMaximum,
				0n,
				1_000_000_000n,
			);
			return (
				minimum === undefined || maximum === undefined || minimum > maximum
			);
		}
		default:
			return false;
	}
}

function assignmentMatrixFieldInvalid(
	form: FlowGeneratorForm,
	field: FlowGeneratorFieldKey,
	a: bigint,
	b: bigint,
	c: bigint,
	d: bigint,
): boolean {
	const admissionInvalid =
		a < 1n ||
		b < 1n ||
		a + b > 2_000n ||
		a * a * b > 20_000_000n ||
		flowGeneratorFamilyEntry("assignment-matrix").estimate(form).edges >
			20_000n;
	if (field === "primary") return a < 1n || admissionInvalid;
	if (field === "secondary")
		return (
			b < 1n ||
			admissionInvalid ||
			(form.assignmentShape === "planted-optimum" && b < a)
		);
	if (field === "tertiary") {
		switch (form.assignmentShape) {
			case "uniform":
				return c > 1_000n;
			case "planted-optimum":
				return c < 1n || c > 1_000n;
			case "block":
			case "near-tie":
			case "monge":
			case "anti-monge":
				return c < 1n;
			case "sparse-allowed":
				return c > b;
			case "hall-deficient":
				return c < 1n || c > a;
			case "equal":
				return false;
		}
	}
	if (field === "quaternary")
		return form.assignmentShape === "planted-optimum"
			? d < 1n
			: form.assignmentShape === "hall-deficient" && (d >= c || d > b);
	if (field === "assignmentNoise")
		return (
			form.assignmentShape === "planted-optimum" &&
			(!Number.isSafeInteger(form.assignmentNoise) || form.assignmentNoise < 0)
		);
	if (field === "costMinimum" || field === "costMaximum") {
		const minimum = canonicalInteger(
			form.costMinimum,
			-1_000_000_000n,
			1_000_000_000n,
		);
		const maximum = canonicalInteger(
			form.costMaximum,
			-1_000_000_000n,
			1_000_000_000n,
		);
		const usesInterval =
			form.assignmentShape === "uniform" ||
			form.assignmentShape === "block" ||
			form.assignmentShape === "sparse-allowed" ||
			form.assignmentShape === "hall-deficient";
		return (
			minimum === undefined ||
			(usesInterval && (maximum === undefined || minimum > maximum))
		);
	}
	return false;
}

function transportationTableFieldInvalid(
	form: FlowGeneratorForm,
	field: FlowGeneratorFieldKey,
	a: bigint,
	b: bigint,
	c: bigint,
	d: bigint,
): boolean {
	const admissionInvalid = a < 1n || b < 1n || a + b > 256n || a * b > 2_048n;
	const minimumTotal = a > b ? a : b;
	const unitMismatch =
		form.transportationShape === "unit-degenerate" && (a !== b || c !== a);
	const cutMismatch =
		form.transportationShape === "cut-infeasible" &&
		(a < 2n || b < 2n || c < (a + 1n > b ? a + 1n : b));
	if (field === "primary" || field === "secondary")
		return admissionInvalid || unitMismatch || cutMismatch;
	if (field === "tertiary")
		return (
			c < minimumTotal || c > 1_000_000_000n || unitMismatch || cutMismatch
		);
	if (field === "quaternary")
		return form.transportationShape === "sparse-feasible"
			? d < 1n || d > 1_000n
			: ["block", "near-tie", "monge"].includes(form.transportationShape) &&
					d < 1n;
	if (field === "costMinimum" || field === "costMaximum") {
		const minimum = canonicalInteger(
			form.costMinimum,
			-1_000_000_000n,
			1_000_000_000n,
		);
		const maximum = canonicalInteger(
			form.costMaximum,
			-1_000_000_000n,
			1_000_000_000n,
		);
		const usesInterval = [
			"dense-uniform",
			"sparse-feasible",
			"cut-infeasible",
		].includes(form.transportationShape);
		const orderingInvalid =
			usesInterval &&
			minimum !== undefined &&
			maximum !== undefined &&
			minimum > maximum;
		if (field === "costMinimum") {
			return (
				minimum === undefined ||
				orderingInvalid ||
				(form.transportationShape === "near-tie" &&
					minimum + d > 1_000_000_000n)
			);
		}
		return (
			((usesInterval || form.transportationShape === "block") &&
				maximum === undefined) ||
			orderingInvalid
		);
	}
	return false;
}

/**
 * Returns the invalid state for one generator input. Cross-field constraints mark
 * the fields that jointly participate in the constraint, never unrelated inputs.
 */
function flowGeneratorFieldInvalidFromRegistry(
	form: FlowGeneratorForm,
	field: FlowGeneratorFieldKey,
): boolean {
	if (field === "seed") {
		return (
			canonicalInteger(form.seed, 0n, 18_446_744_073_709_551_615n) === undefined
		);
	}
	if (
		field === "primary" ||
		field === "secondary" ||
		field === "tertiary" ||
		field === "quaternary"
	) {
		const value = form[field];
		if (!Number.isSafeInteger(value) || value < 0) return true;
	}
	if (
		![form.primary, form.secondary, form.tertiary, form.quaternary].every(
			(value) => Number.isSafeInteger(value) && value >= 0,
		)
	) {
		return false;
	}
	const handler = flowGeneratorFamilyEntry(form.family).fieldInvalid;
	return (
		handler?.(
			form,
			field,
			BigInt(form.primary),
			BigInt(form.secondary),
			BigInt(form.tertiary),
			BigInt(form.quaternary),
		) ?? false
	);
}

export function flowGeneratorFieldInvalid(
	form: FlowGeneratorForm,
	field: FlowGeneratorFieldKey,
): boolean {
	return flowGeneratorFieldInvalidFromRegistry(form, field);
}

function assignmentShapePayload(
	form: FlowGeneratorForm,
): Record<string, unknown> {
	switch (form.assignmentShape) {
		case "uniform":
			return {
				kind: "uniform",
				density_per_mille: form.tertiary,
				minimum_cost: Number(form.costMinimum),
				maximum_cost: Number(form.costMaximum),
			};
		case "equal":
			return { kind: "equal", cost: Number(form.costMinimum) };
		case "block":
			return {
				kind: "block",
				blocks: form.tertiary,
				within_cost: Number(form.costMinimum),
				between_cost: Number(form.costMaximum),
			};
		case "near-tie":
			return {
				kind: "near-tie",
				base_cost: Number(form.costMinimum),
				gap: form.tertiary,
			};
		case "planted-optimum":
			return {
				kind: "planted-optimum",
				density_per_mille: form.tertiary,
				base_cost: Number(form.costMinimum),
				gap: form.quaternary,
				noise: form.assignmentNoise,
			};
		case "monge":
		case "anti-monge":
			return { kind: form.assignmentShape, scale: form.tertiary };
		case "sparse-allowed":
			return {
				kind: "sparse-allowed",
				degree: form.tertiary,
				minimum_cost: Number(form.costMinimum),
				maximum_cost: Number(form.costMaximum),
			};
		case "hall-deficient":
			return {
				kind: "hall-deficient",
				witness_agents: form.tertiary,
				witness_tasks: form.quaternary,
				minimum_cost: Number(form.costMinimum),
				maximum_cost: Number(form.costMaximum),
			};
	}
}

function transportationShapePayload(
	form: FlowGeneratorForm,
): Record<string, unknown> {
	switch (form.transportationShape) {
		case "dense-uniform":
		case "cut-infeasible":
			return {
				kind: form.transportationShape,
				minimum_cost: Number(form.costMinimum),
				maximum_cost: Number(form.costMaximum),
			};
		case "sparse-feasible":
			return {
				kind: "sparse-feasible",
				density_per_mille: form.quaternary,
				minimum_cost: Number(form.costMinimum),
				maximum_cost: Number(form.costMaximum),
			};
		case "unit-degenerate":
			return { kind: "unit-degenerate", cost: Number(form.costMinimum) };
		case "block":
			return {
				kind: "block",
				blocks: form.quaternary,
				within_cost: Number(form.costMinimum),
				between_cost: Number(form.costMaximum),
			};
		case "near-tie":
			return {
				kind: "near-tie",
				base_cost: Number(form.costMinimum),
				gap: form.quaternary,
			};
		case "monge":
			return { kind: "monge", scale: form.quaternary };
	}
}

function capacityPayload(form: FlowGeneratorForm): Record<string, unknown> {
	switch (form.capacityKind) {
		case "unit":
			return { kind: "unit" };
		case "constant":
			return { kind: "constant", value: form.capacityMinimum };
		case "uniform":
			return {
				kind: "uniform",
				minimum: form.capacityMinimum,
				maximum: form.capacityMaximum,
			};
		case "bimodal":
			return {
				kind: "bimodal",
				first: form.capacityMinimum,
				second: form.capacityMaximum,
			};
		case "power-of-two-buckets":
			return {
				kind: "power-of-two-buckets",
				minimum_exponent: Number(form.capacityMinimum),
				maximum_exponent: Number(form.capacityMaximum),
			};
	}
}

function costPayload(form: FlowGeneratorForm): Record<string, unknown> {
	switch (form.costKind) {
		case "zero":
			return { kind: "zero" };
		case "constant":
			return { kind: "constant", value: form.costMinimum };
		case "uniform":
			return {
				kind: "uniform",
				minimum: form.costMinimum,
				maximum: form.costMaximum,
			};
		case "bimodal":
			return {
				kind: "bimodal",
				first: form.costMinimum,
				second: form.costMaximum,
			};
		case "capacity-correlated":
			return {
				kind: "capacity-correlated",
				minimum: form.costMinimum,
				maximum: form.costMaximum,
				direction: form.costCorrelationDirection,
				maximum_jitter: form.costMaximumJitter,
			};
	}
}

export type FlowGeneratorTargetProblem =
	| "native"
	| "max-flow"
	| "fixed-flow-min-cost";

export function encodeFlowGeneratorSpec(
	form: FlowGeneratorForm,
	targetProblem: FlowGeneratorTargetProblem = "native",
): string {
	const entry = flowGeneratorFamilyEntry(form.family);
	const fixedByConstruction = entry.fixedConstruction !== undefined;
	return JSON.stringify({
		generator_revision: "flow-generator/27",
		seed: form.seed,
		family: entry.payload(form),
		capacity: fixedByConstruction ? { kind: "unit" } : capacityPayload(form),
		cost: fixedByConstruction ? { kind: "zero" } : costPayload(form),
		...(targetProblem === "native" ? {} : { target_problem: targetProblem }),
	});
}

function assignmentParameterLabels(
	shape: AssignmentMatrixShapeId,
): readonly [string?, string?] {
	switch (shape) {
		case "uniform":
			return ["Allowed-edge density (‰)"];
		case "equal":
			return [];
		case "block":
			return ["Block count"];
		case "near-tie":
			return ["gap"];
		case "planted-optimum":
			return ["Allowed-edge density (‰)", "Gap from optimum"];
		case "monge":
		case "anti-monge":
			return ["scale s"];
		case "sparse-allowed":
			return ["Degree per agent"];
		case "hall-deficient":
			return ["Witness agent prefix", "Witness task neighborhood"];
	}
}

function assignmentCostLabels(
	shape: AssignmentMatrixShapeId,
): readonly [string?, string?] {
	switch (shape) {
		case "uniform":
		case "sparse-allowed":
		case "hall-deficient":
			return ["Minimum cost", "Maximum cost"];
		case "equal":
			return ["Common cost"];
		case "block":
			return ["Within-group cost", "Cross-group cost"];
		case "near-tie":
		case "planted-optimum":
			return ["Optimal-edge base cost"];
		case "monge":
		case "anti-monge":
			return [];
	}
}

function transportationParameterLabel(
	shape: TransportationTableShapeId,
): string | undefined {
	switch (shape) {
		case "sparse-feasible":
			return "Route density ‰ (preserve feasible support)";
		case "block":
			return "Block count";
		case "near-tie":
			return "gap";
		case "monge":
			return "scale";
		case "dense-uniform":
		case "unit-degenerate":
		case "cut-infeasible":
			return undefined;
	}
}

function transportationCostLabels(
	shape: TransportationTableShapeId,
): readonly [string, string?] {
	switch (shape) {
		case "dense-uniform":
		case "sparse-feasible":
		case "cut-infeasible":
			return ["Minimum cost", "Maximum cost"];
		case "unit-degenerate":
			return ["Common cost"];
		case "block":
			return ["Within-group cost", "Cross-group cost"];
		case "near-tie":
			return ["base cost"];
		case "monge":
			return [""];
	}
}

function capacityFieldLabels(
	kind: FlowGeneratorForm["capacityKind"],
): readonly [string, string?] {
	switch (kind) {
		case "unit":
			return [""];
		case "constant":
			return ["Capacity value"];
		case "uniform":
			return ["Minimum capacity", "Maximum capacity"];
		case "bimodal":
			return ["Capacity atom A", "Capacity atom B"];
		case "power-of-two-buckets":
			return ["Minimum exponent (2^k)", "Maximum exponent (2^k)"];
	}
}

function costFieldLabels(
	kind: FlowGeneratorForm["costKind"],
): readonly [string, string?] {
	switch (kind) {
		case "zero":
			return [""];
		case "constant":
			return ["Cost value"];
		case "uniform":
		case "capacity-correlated":
			return ["Minimum cost", "Maximum cost"];
		case "bimodal":
			return ["Cost atom A", "Cost atom B"];
	}
}

function applyAssignmentShape(
	form: FlowGeneratorForm,
	shape: AssignmentMatrixShapeId,
): FlowGeneratorForm {
	const common = {
		...form,
		family: "assignment-matrix" as const,
		assignmentShape: shape,
	};
	switch (shape) {
		case "uniform":
			return { ...common, tertiary: 700, costMinimum: "-9", costMaximum: "20" };
		case "equal":
			return { ...common, tertiary: 0, costMinimum: "5", costMaximum: "5" };
		case "block":
			return { ...common, tertiary: 2, costMinimum: "0", costMaximum: "12" };
		case "near-tie":
			return { ...common, tertiary: 1, costMinimum: "10", costMaximum: "10" };
		case "planted-optimum":
			return {
				...common,
				tertiary: 600,
				quaternary: 5,
				assignmentNoise: 3,
				costMinimum: "10",
				costMaximum: "10",
			};
		case "monge":
		case "anti-monge":
			return { ...common, tertiary: 2, costMinimum: "0", costMaximum: "0" };
		case "sparse-allowed":
			return { ...common, tertiary: 3, costMinimum: "-9", costMaximum: "20" };
		case "hall-deficient":
			return {
				...common,
				tertiary: 4,
				quaternary: 3,
				costMinimum: "0",
				costMaximum: "9",
			};
	}
}

function applyTransportationShape(
	form: FlowGeneratorForm,
	shape: TransportationTableShapeId,
): FlowGeneratorForm {
	const common = {
		...form,
		family: "transportation-table" as const,
		transportationShape: shape,
		primary: 6,
		secondary: 7,
		tertiary: 42,
		quaternary: 0,
		capacityKind: "unit" as const,
		costKind: "zero" as const,
	};
	switch (shape) {
		case "dense-uniform":
			return { ...common, costMinimum: "-5", costMaximum: "12" };
		case "sparse-feasible":
			return {
				...common,
				quaternary: 400,
				costMinimum: "-5",
				costMaximum: "12",
			};
		case "unit-degenerate":
			return {
				...common,
				secondary: 6,
				tertiary: 6,
				costMinimum: "3",
				costMaximum: "3",
			};
		case "block":
			return { ...common, quaternary: 2, costMinimum: "0", costMaximum: "12" };
		case "near-tie":
			return { ...common, quaternary: 1, costMinimum: "10", costMaximum: "10" };
		case "monge":
			return { ...common, quaternary: 2, costMinimum: "0", costMaximum: "0" };
		case "cut-infeasible":
			return {
				...common,
				primary: 4,
				secondary: 5,
				tertiary: 20,
				costMinimum: "0",
				costMaximum: "9",
			};
	}
}

export type FlowGeneratorUiFeature =
	| "assignment-shape"
	| "transportation-shape"
	| "netgen"
	| "gridgraph"
	| "washington-level"
	| "washington-matching"
	| "washington-square-mesh"
	| "grid-toggle"
	| "rmfgen"
	| "gridgen"
	| "goto-torus"
	| "vision-grid";

type FixedConstructionCopy = Readonly<{
	heading: string;
	detail: string;
	note: string;
}>;

type FlowGeneratorPresetKey =
	| "gridgraphPreset"
	| "netgenPreset"
	| "washingtonMatchingPreset"
	| "washingtonPreset"
	| "washingtonSquareMeshPreset";

export type FlowGeneratorFamilyDescriptor = Readonly<{
	id: FlowGeneratorFamilyId;
	features: ReadonlySet<FlowGeneratorUiFeature>;
	sourceBacked: boolean;
	statusId?: string;
	toggleLabel?: string;
	presetKey?: FlowGeneratorPresetKey;
	applyWashingtonLevelPreset?: (
		form: FlowGeneratorForm,
		preset: Exclude<WashingtonPresetId, "custom">,
	) => FlowGeneratorForm;
	/** Ordered schema rendered by the shared numeric-parameter control. */
	parameters: (
		form: FlowGeneratorForm,
	) => readonly FlowGeneratorParameterControl[];
	defaults: (form: FlowGeneratorForm) => FlowGeneratorForm;
	presets: readonly Readonly<{ id: string; label: string }>[];
	estimate: (form: FlowGeneratorForm) => Estimate;
	estimateIsUpperBound: (form: FlowGeneratorForm) => boolean;
	validation: (form: FlowGeneratorForm) => string | undefined;
	fieldInvalid: (
		form: FlowGeneratorForm,
		field: FlowGeneratorFieldKey,
	) => boolean;
	encode: (form: FlowGeneratorForm) => string;
	fixedConstruction: (
		form: FlowGeneratorForm,
	) => FixedConstructionCopy | undefined;
	customize: <Key extends keyof FlowGeneratorForm>(
		form: FlowGeneratorForm,
		key: Key,
		value: FlowGeneratorForm[Key],
	) => FlowGeneratorForm;
}>;

type FlowGeneratorFamilyEntryDeclaration = Readonly<{
	features: readonly FlowGeneratorUiFeature[];
	parameters: readonly FlowGeneratorParameter[];
	sourceBacked: boolean;
	presets: readonly Readonly<{ id: string; label: string }>[];
	presetKey?: FlowGeneratorPresetKey;
	applyWashingtonLevelPreset?: (
		form: FlowGeneratorForm,
		preset: Exclude<WashingtonPresetId, "custom">,
	) => FlowGeneratorForm;
	statusId?: string;
	toggleLabel?: string;
	defaults?: (form: FlowGeneratorForm) => FlowGeneratorForm;
	estimate: (form: FlowGeneratorForm) => Estimate;
	estimateIsUpperBound?: (form: FlowGeneratorForm) => boolean;
	payload: (form: FlowGeneratorForm) => Record<string, unknown>;
	skipDistributionValidation?: boolean;
	validate: (
		form: FlowGeneratorForm,
		a: bigint,
		b: bigint,
		c: bigint,
		d: bigint,
	) => string | undefined;
	fieldInvalid?: (
		form: FlowGeneratorForm,
		field: FlowGeneratorFieldKey,
		a: bigint,
		b: bigint,
		c: bigint,
		d: bigint,
	) => boolean;
	fixedConstruction?: (form: FlowGeneratorForm) => FixedConstructionCopy;
}>;

type FlowGeneratorParameter = Readonly<{
	label: string;
	minimum?: number;
	maximum?: number | ((form: FlowGeneratorForm) => number | undefined);
	step?: number;
}>;

export type FlowGeneratorParameterControl = Readonly<{
	field: "primary" | "secondary" | "tertiary" | "quaternary";
	label: string;
	minimum: number;
	maximum: number | undefined;
	step: number;
}>;

function parameter(
	label: string,
	minimum = 1,
	maximum?: number | ((form: FlowGeneratorForm) => number | undefined),
	step = 1,
): FlowGeneratorParameter {
	return {
		label,
		minimum,
		...(maximum === undefined ? {} : { maximum }),
		...(step === 1 ? {} : { step }),
	};
}

function estimateWith(
	calculate: (
		a: bigint,
		b: bigint,
		c: bigint,
		d: bigint,
		form: FlowGeneratorForm,
	) => Estimate,
): (form: FlowGeneratorForm) => Estimate {
	return (form) =>
		calculate(
			nonnegativeBigInt(form.primary),
			nonnegativeBigInt(form.secondary),
			nonnegativeBigInt(form.tertiary),
			nonnegativeBigInt(form.quaternary),
			form,
		);
}

function washingtonLineFieldInvalid(
	maximumDegree: (width: bigint) => bigint,
): NonNullable<FlowGeneratorFamilyEntryDeclaration["fieldInvalid"]> {
	return (_form, field, a, b, c) => {
		const nodeLimit = a * b + 2n > 2_000n;
		const edgeLimit = a * b * c + 2n * b > 20_000n;
		if (field === "primary") return a < 2n || nodeLimit || edgeLimit;
		if (field === "secondary") return b < 1n || nodeLimit || edgeLimit;
		return (
			field === "tertiary" && (c < 1n || c > maximumDegree(b) || edgeLimit)
		);
	};
}

function washingtonLevelFieldInvalid(
	form: FlowGeneratorForm,
	field: FlowGeneratorFieldKey,
	a: bigint,
	b: bigint,
): boolean {
	const shapeLimit = a * b + 2n > 2_000n;
	if (field === "primary") return bigintOutside(a, 3n, 1_000n) || shapeLimit;
	if (field === "secondary") return bigintOutside(b, 2n, 1_000n) || shapeLimit;
	return (
		field === "capacityMaximum" &&
		canonicalInteger(form.capacityMaximum, 1n, 100_000_000n) === undefined
	);
}

const GLOVER_DENSE_FIXED_COPY: FixedConstructionCopy = {
	heading: "Glover–Waissi dense DAG · source-claimed stress",
	detail:
		"All forward edges i<j · chain capacity 1+⌊(i−n/2)²⌋ · all other capacities 1 · cost 0",
	note: "Waissi's reference source implements these special capacities as the Glover et al. hard network. The source gives no general lower bound for a specified algorithm and tie-breaking policy, so no certificate is claimed. For n=12, stable-ID Dinic measures max flow 36, 12 BFS passes, 11 blocking-flow phases, and 36 augmentations.",
};

const WAISSI_AC_FIXED_COPY: FixedConstructionCopy = {
	heading: "First DIMACS AC · official dense benchmark",
	detail:
		"All forward edges i<j · n(n−1)/2 edges · capacities [1,1,000,000] · cost 0",
	note: "Materializes the complete dense DAG and capacity range from the official ac.c using the reproducible project RNG. Its sequence is not byte-identical to C random(), and this is not labeled a worst case.",
};

const WAISSI_TRANSIT_TWO_WAY_FIXED_COPY: FixedConstructionCopy = {
	heading: "Waissi two-way transit grid · official benchmark",
	detail:
		"d×d grid · bidirectional adjacency and terminal links · 4d² edges · capacities [1,U] · cost 0",
	note: "Materializes the complete square grid from official tr2-max.pas with the reproducible project capacity RNG. Its random sequence is not byte-identical to Turbo Pascal, and this is not labeled a worst case.",
};

const WAISSI_TRANSIT_ONE_WAY_FIXED_COPY: FixedConstructionCopy = {
	heading: "Waissi one-way transit grid · official benchmark",
	detail:
		"d×d grid · terminals fixed left-to-right · each horizontal or vertical street chooses one random direction · 2d² edges · capacities [1,U] · cost 0",
	note: "Materializes the complete square grid and integer direction rule from official tr1-max.pas using the reproducible project RNG. For odd U, one direction has probability ⌈U/2⌉/U and the other ⌊U/2⌋/U. Direction and capacity use independent streams, so the sequence is not byte-identical to Turbo Pascal. Some orientations have no s–t path and max flow 0.",
};

const GOLDBERG_MESH_FIXED_COPY: FixedConstructionCopy = {
	heading: "Goldberg mesh1 · signed-bound min-cost circulation",
	detail:
		"X×Y torus · distance-d capacity ⌊r/2^(d−1)⌋ · costs ±[0,999] · each signed link becomes a forward/reverse pair",
	note: "Converts each First DIMACS mesh1.c bound −a≤x≤b with cost cx into forward (b,c) and reverse (a,−c) edges without changing its meaning. Distance decay may produce zero-capacity edges. Negative-cost cycles are possible; compare with Cost Scaling or Cycle Canceling. Capacity and cost use separate project RNG streams, so output is not byte-identical and is not labeled a worst case.",
};

const DINIC_WORST_CASE_FIXED_COPY: FixedConstructionCopy = {
	heading: "Fixed paper construction",
	detail: "Chain-edge capacity n · sink-entry capacity 1 · cost 0",
	note: "Generates n−1 level graphs following the Waissi (1991) construction.",
};

const PLANTED_BOTTLENECK_FIXED_COPY: FixedConstructionCopy = {
	heading: "Fixed cut construction",
	detail: "Middle-cut capacity 1 · outer capacity = cut-edge count · cost 0",
	note: "Generator tests verify that max flow equals the requested cut-edge count.",
};

const HALL_TIGHT_FIXED_COPY: FixedConstructionCopy = {
	heading: "Fixed matching construction",
	detail: "All edges have capacity 1 · cost 0",
	note: "The neighborhood of each left prefix exactly matches an equal-sized right prefix while preserving a perfect matching.",
};

const RMFGEN_FIXED_COPY: FixedConstructionCopy = {
	heading: "Fixed RMFGEN structural contract",
	detail: "Inter-frame [c1,c2] · intra-frame c2·a² · cost 0",
	note: "Materializes the Goldfarb–Grigoriadis / GLPK structure with independent project RNG streams.",
};

const GRIDGEN_FIXED_COPY: FixedConstructionCopy = {
	heading: "Fixed GRIDGEN-derived structure",
	detail:
		"Regular edges use the requested ranges · super edges use capacity B / cost 2× the maximum grid cost",
	note: "Total edges are max(base edges, (rc+1)d). In directed mode, rows and columns alternate orientation; base edges may exceed the target average degree.",
};

const WASHINGTON_BASIC_LINE_FIXED_COPY: FixedConstructionCopy = {
	heading: "Washington Basic Line · primary-source-derived",
	detail:
		"n blocks × width m · distinct positive d offsets from each vertex · internal capacities [1,10^6] · terminal capacity 20,000,000 · cost 0",
	note: "Materializes First DIMACS function 6 with separate canonical topology and capacity RNG streams. Out-of-range tail offsets are dropped, so the displayed edge count is a safe upper bound. Output is not byte-identical to the original random order and is not labeled a worst case.",
};

const WASHINGTON_EXPONENTIAL_LINE_FIXED_COPY: FixedConstructionCopy = {
	heading: "Washington Exponential Line · primary-source-derived",
	detail:
		"Basic Line topology · internal capacity limit decays 10^6 → … → 2 by forward-distance band · terminal capacity 20,000,000 · cost 0",
	note: "Preserves the 20-entry integer Range table from First DIMACS function 7. This is not a general exponential distribution; tail clipping makes the realized edge count seed-dependent. It is not labeled a worst case.",
};

const WASHINGTON_DOUBLE_EXPONENTIAL_LINE_FIXED_COPY: FixedConstructionCopy = {
	heading: "Washington Double Exponential Line · primary-source-derived",
	detail:
		"Signed offsets −md..md · exclude zero and out-of-range offsets · distance-band capacities in both directions · terminal capacity 20,000,000 · cost 0",
	note: "Preserves C integer division and the Range table from First DIMACS function 8. Degree is restricted to the safe range to avoid the original out-of-bounds table access; no speculative correction is applied. Cycles are possible, but this is not labeled a worst case.",
};

const GOTO_FIXED_COPY: FixedConstructionCopy = {
	heading: "Fixed Goldberg GOTO-derived structure",
	detail:
		"Opened torus · one supply and one demand · return-path capacity equals total supply and may exceed U",
	note: "U limits regular random-edge capacity. The original random-boundary, fixed-direction, and return-cost defects are not carried forward; distance-based capacity decay is defined using the project RNG and integer power-of-two quantization.",
};

function washingtonDinicFixedCopy(
	form: FlowGeneratorForm,
): FixedConstructionCopy {
	const nodes = nonnegativeBigInt(form.primary);
	const blockingPhases = nodes >= 2n ? nodes - 1n : undefined;
	const maximumFlow = nodes === 2n ? 2n : nodes >= 3n ? nodes + 1n : undefined;
	return {
		heading: "First DIMACS function 9 · source-claimed stress",
		detail:
			"Every chain edge has capacity n · unit-capacity shortcuts from the first n−2 vertices to the sink · cost 0",
		note: `The official README states “n augmentation phases.” Stable-ID Dinic measures ${blockingPhases?.toString() ?? "—"} blocking-flow phases, ${nodes >= 2n ? nodes.toString() : "—"} BFS passes including termination, and max flow ${maximumFlow?.toString() ?? "—"}. Because the metrics differ, this is not labeled a verified worst case and carries no certificate.`,
	};
}

function washingtonFifoFixedCopy(
	form: FlowGeneratorForm,
): FixedConstructionCopy {
	const blockSize = nonnegativeBigInt(form.primary);
	const anchors: Record<string, readonly [pushes: bigint, relabels: bigint]> = {
		"2": [20n, 14n],
		"4": [63n, 39n],
		"8": [193n, 110n],
		"16": [573n, 326n],
		"32": [1_895n, 1_076n],
	};
	const measured = anchors[blockSize.toString()];
	return {
		heading: "First DIMACS function 10 · measured FIFO stress",
		detail:
			"source→hub→k unit bottlenecks→merge→k-edge tail chain · all other capacities k · cost 0",
		note: measured
			? `Stable-ID FIFO measures ${measured[0].toString()} pushes and ${measured[1].toString()} relabels; highest-label measures ${(5n * blockSize).toString()} pushes and ${(4n * blockSize).toString()} relabels. These are finite-size measurements, so no certificate is claimed.`
			: "The official source calls this a Goldberg bad case without naming a selection policy. Only FIFO shows strong growth here, so it is labeled source-claimed stress rather than a verified worst case.",
	};
}

function washingtonCheriyanFixedCopy(
	form: FlowGeneratorForm,
): FixedConstructionCopy {
	const n = nonnegativeBigInt(form.primary);
	const m = nonnegativeBigInt(form.secondary);
	const c = nonnegativeBigInt(form.tertiary);
	const anchor =
		n === 4n && m === 2n && c === 2n
			? "At these defaults, stable-ID measurements are generic push/relabel 46/34, FIFO 34/27, and global-relabel 29/4."
			: n === 8n && m === 4n && c === 2n
				? "At these settings, stable-ID measurements are generic push/relabel 222/136, FIFO 175/117, and global-relabel 141/26."
				: "Current stable-ID ordering does not degrade uniformly across all presets, so no general worst-case certificate is claimed.";
	return {
		heading: "First DIMACS function 11 · source-claimed legacy stress",
		detail:
			"Four mc-node chain gadgets · capacity-n entries every c vertices · n unit bridges · BIG capacity 1,000,000 · cost 0",
		note: `The official C display formula 4mc+n+6 disagrees with the actual allocation. Following its implementation yields 4mc+2n+7 vertices and 4m(c+1)+3n+3 edges; max flow in this practical range is 2nm = ${(2n * n * m).toString()}. ${anchor}`,
	};
}

function cherkasskyGoldbergAkFixedCopy(
	form: FlowGeneratorForm,
): FixedConstructionCopy {
	const size = nonnegativeBigInt(form.primary);
	const anchors: Record<string, readonly [pushes: bigint, relabels: bigint]> = {
		"2": [32n, 23n],
		"4": [65n, 52n],
		"8": [162n, 130n],
		"16": [432n, 350n],
		"32": [1_328n, 1_082n],
	};
	const fifo = anchors[size.toString()];
	return {
		heading: "Cherkassky–Goldberg AK · deterministic push–relabel stress",
		detail:
			"Two chain gadgets · 4k+6 vertices · 6k+7 edges · unit shortcuts / mirror arcs · terminal capacity 1,000,000 · cost 0",
		note: fifo
			? `Max flow is 2k+3 = ${(2n * size + 3n).toString()}. Finite-size stable-ID FIFO measurements are ${fifo[0].toString()} pushes and ${fifo[1].toString()} relabels. The paper's complexity claim concerns its evaluation implementation, so no worst-case certificate is claimed here.`
			: `Max flow is 2k+3 = ${(2n * size + 3n).toString()}. This independently implements the Cherkassky–Goldberg construction without porting its distributed code. No worst-case certificate is claimed across this implementation's selection policies.`,
	};
}

function zadehFixedCopy(form: FlowGeneratorForm): FixedConstructionCopy {
	const groupSize = nonnegativeBigInt(form.primary);
	const exactAugmentations =
		groupSize >= 4n ? (groupSize * groupSize * groupSize) / 4n : undefined;
	return {
		heading: "Fixed paper-inspired stress construction",
		detail:
			"3k vertices · complete-bipartite unit-capacity edges · phase chain · cost 0",
		note: `A project-specific phase chain regression-checks k³/4 finite-size stable-BFS augmentations (${exactAugmentations?.toString() ?? "—"} here). It is not an exact transcription of Zadeh's worst-case construction.`,
	};
}

function assignmentFixedCopy(form: FlowGeneratorForm): FixedConstructionCopy {
	const selected = ASSIGNMENT_SHAPE_OPTIONS.find(
		(option) => option.id === form.assignmentShape,
	);
	return {
		heading: `Native assignment · ${selected?.label ?? form.assignmentShape}`,
		detail: `agent ${form.primary} × task ${form.secondary} · ${form.assignmentObjective} · every edge has capacity 1`,
		note: `${selected?.detail ?? "project synthetic matrix"}. Before generation, checks the Hungarian execution envelope: a²t cell scans, 2,000 vertices, and 20,000 edges. It is not labeled a worst case for any specific algorithm.`,
	};
}

function transportationFixedCopy(
	form: FlowGeneratorForm,
): FixedConstructionCopy {
	const selected = TRANSPORTATION_SHAPE_OPTIONS.find(
		(option) => option.id === form.transportationShape,
	);
	return {
		heading: `Native transportation · ${selected?.label ?? form.transportationShape}`,
		detail: `origin ${form.primary} × destination ${form.secondary} · total shipment ${form.tertiary} · every route has capacity ${form.tertiary}`,
		note: `${selected?.detail ?? "project synthetic table"}. Fixes balanced equality, missing-route prohibition, and nonbinding capacity, then checks the 256-vertex / 2,048-route execution envelope before generation. Degenerate and near-tie variants are not labeled asymptotic worst cases.`,
	};
}

function gridgraphFixedCopy(form: FlowGeneratorForm): FixedConstructionCopy {
	const shape =
		form.gridgraphPreset === "square"
			? "GRID-SQUARE"
			: form.gridgraphPreset === "wide"
				? "GRID-WIDE"
				: form.gridgraphPreset === "long"
					? "GRID-LONG"
					: form.gridgraphPreset === "readable"
						? "Readable"
						: "Custom";
	return {
		heading: `GRIDGRAPH · primary-source-derived · ${shape}`,
		detail:
			"Right/down grid · grid capacities [1,U] · costs [1,C] · terminal capacity is the sum of incident grid capacities and may exceed U",
		note: "Following corrected ggraph1.f attribute generation, internal Dinic max flow sets supply and demand. The 48×16 and 16×48 project presets are reduced for visualization. Output is not byte-identical because neither the original RNG nor column-block order is copied.",
	};
}

function washingtonMatchingFixedCopy(
	form: FlowGeneratorForm,
): FixedConstructionCopy {
	const preset =
		WASHINGTON_MATCHING_PRESET_OPTIONS.find(
			(option) => option.id === form.washingtonMatchingPreset,
		)?.label ?? "Custom";
	return {
		heading: `Washington Matching · primary-source-derived · ${preset}`,
		detail:
			"n vertices per side · each left vertex connects to d distinct right vertices · all capacities, including source/sink edges, are 1 · cost 0",
		note: "Materializes First DIMACS function 4 using a uniform d-subset per left vertex and the canonical project RNG. The original RNG, rejection order, and adjacency-list output order are not copied. This matching family guarantees neither a perfect matching nor a worst case.",
	};
}

function washingtonSquareMeshFixedCopy(
	form: FlowGeneratorForm,
): FixedConstructionCopy {
	const preset =
		WASHINGTON_SQUARE_MESH_PRESET_OPTIONS.find(
			(option) => option.id === form.washingtonSquareMeshPreset,
		)?.label ?? "Custom";
	return {
		heading: `Washington Square Mesh · primary-source-derived · ${preset}`,
		detail:
			"d×d row-major grid · forward offsets d..d+degree−1 from each non-final-column vertex · tail clipping only · intermediate capacities [1,C] · terminal capacity 3C · cost 0",
		note: "Following First DIMACS function 5, row-end offsets do not wrap within the next column and can advance into the following column. This is not a standard 4-neighbor grid. The project capacity RNG makes output non-byte-identical, and this is not labeled a worst case.",
	};
}

function washingtonRandomLevelFixedCopy(
	form: FlowGeneratorForm,
): FixedConstructionCopy {
	const preset =
		WASHINGTON_PRESET_OPTIONS.find(
			(option) => option.id === form.washingtonPreset,
		)?.label ?? "Custom";
	return {
		heading: `Washington Random Level · primary-source-derived · ${preset}`,
		detail:
			"Each vertex connects to 3 distinct vertices in the next level · intermediate capacities [1,C] · terminal capacity 3C · cost 0",
		note: "Materializes First DIMACS function 2 with separate project topology and capacity RNG streams. The original RNG, single stream, and adjacency-list output order are not copied, so output is not byte-identical and is not labeled a worst case.",
	};
}

function washingtonMeshFixedCopy(
	form: FlowGeneratorForm,
): FixedConstructionCopy {
	const preset =
		WASHINGTON_PRESET_OPTIONS.find(
			(option) => option.id === form.washingtonPreset,
		)?.label ?? "Custom";
	return {
		heading: `Washington Mesh · primary-source-derived · ${preset}`,
		detail:
			"Three edges from each vertex to the previous, same, and next row of the next level · row wrap · intermediate capacities [1,C] · terminal capacity 3C · cost 0",
		note: "Materializes First DIMACS function 1 with fixed cylindrical topology and the project capacity RNG. Official function 3 dispatches to function 2 and its unreachable R2Level also has inconsistent indexing, so it is not exposed separately. The original RNG and adjacency-list order are not copied, and this is not labeled a worst case.",
	};
}

function netgenFixedCopy(form: FlowGeneratorForm): FixedConstructionCopy {
	const labels: Record<NetgenProblemKind, string> = {
		assignment: "assignment",
		transportation: "transportation",
		transshipment: "general transshipment",
		"max-flow": "single s–t max flow",
	};
	return {
		heading: `NETGEN independent primary-source-derived implementation · ${labels[classifyNetgenForm(form)]}`,
		detail:
			"Build a supply-feasible skeleton first, then uniformly sample the remaining distinct allowed edges",
		note: "Based on primary sources without copying the original code or RNG. Output is not byte-identical, and skeleton capacities may exceed u2 to preserve feasibility.",
	};
}

function declareFlowGeneratorFamily(
	parameters: readonly FlowGeneratorParameter[],
	overrides: Pick<
		FlowGeneratorFamilyEntryDeclaration,
		"estimate" | "payload" | "validate"
	> &
		Omit<
			Partial<FlowGeneratorFamilyEntryDeclaration>,
			"estimate" | "parameters" | "payload" | "validate"
		>,
): FlowGeneratorFamilyEntryDeclaration {
	return {
		features: [],
		parameters,
		presets: [],
		sourceBacked: false,
		...overrides,
	};
}

/**
 * Closed family inventory. Adding a family requires adding exactly one entry;
 * source provenance and UI capabilities are owned here, not inferred elsewhere.
 */
const FLOW_GENERATOR_FAMILY_ENTRY_DECLARATIONS = {
	arborescence: declareFlowGeneratorFamily(
		[parameter("Branching factor"), parameter("Depth")],
		{
			estimate: estimateWith((a, _b, _c, _d, form) => {
				let level = 1n;
				let nodes = 1n;
				for (
					let depth = 0;
					depth < Math.max(0, Math.trunc(form.secondary));
					depth += 1
				) {
					level *= a;
					nodes += level;
				}
				return { nodes, edges: nodes > 0n ? nodes - 1n : 0n };
			}),
			payload: (form) => ({
				family_id: "arborescence",
				branching: form.primary,
				depth: form.secondary,
			}),
			validate: (_form, a, b) =>
				a >= 1n && b >= 1n
					? undefined
					: "Branching factor and depth must be at least 1",
		},
	),
	"assignment-matrix": declareFlowGeneratorFamily(
		[parameter("Agent count"), parameter("Task count")],
		{
			estimate: estimateWith((a, b, c, d, form) => {
				const candidates = a * b;
				const edges =
					form.assignmentShape === "uniform"
						? (candidates * c) / 1_000n
						: form.assignmentShape === "planted-optimum"
							? (candidates * c + 999n) / 1_000n > a
								? (candidates * c + 999n) / 1_000n
								: a
							: form.assignmentShape === "sparse-allowed"
								? a * c
								: form.assignmentShape === "hall-deficient"
									? c * d + (a > c ? a - c : 0n) * b
									: candidates;
				return { nodes: a + b, edges };
			}),
			payload: (form) => ({
				family_id: "assignment-matrix",
				agents: form.primary,
				tasks: form.secondary,
				objective: form.assignmentObjective,
				shape: assignmentShapePayload(form),
			}),
			fixedConstruction: assignmentFixedCopy,
			validate: validateAssignmentMatrixForm,
			fieldInvalid: assignmentMatrixFieldInvalid,
			skipDistributionValidation: true,
			features: ["assignment-shape"],
			presets: ASSIGNMENT_SHAPE_OPTIONS,
			sourceBacked: true,
			defaults: (form) =>
				applyAssignmentShape(
					{ ...form, primary: 6, secondary: 8 },
					"planted-optimum",
				),
		},
	),
	"bipartite-random": declareFlowGeneratorFamily(
		[
			parameter("Left vertices"),
			parameter("Right vertices"),
			parameter("Cross-partition edges", 0),
		],
		{
			estimate: estimateWith((a, b, c) => ({
				nodes: 2n + a + b,
				edges: a + b + c,
			})),
			payload: (form) => ({
				family_id: "bipartite-random",
				left: form.primary,
				right: form.secondary,
				edge_count: form.tertiary,
			}),
			validate: (_form, a, b, c) =>
				a < 1n || b < 1n
					? "Left and right vertex counts must be at least 1"
					: c > a * b
						? "Cross-partition edge count cannot exceed L×R"
						: undefined,
		},
	),
	"cherkassky-goldberg-ak-stress": declareFlowGeneratorFamily(
		[parameter("size k", 2, 128)],
		{
			estimate: estimateWith((a) => ({
				nodes: 4n * a + 6n,
				edges: 6n * a + 7n,
			})),
			payload: (form) => ({
				family_id: "cherkassky-goldberg-ak-stress",
				size: form.primary,
			}),
			fixedConstruction: cherkasskyGoldbergAkFixedCopy,
			validate: (_form, a) =>
				a >= 2n && a <= 128n
					? undefined
					: "Cherkassky–Goldberg AK size k must be between 2 and 128",
			fieldInvalid: (_form, field, a) =>
				field === "primary" && bigintOutside(a, 2n, 128n),
			skipDistributionValidation: true,
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "cherkassky-goldberg-ak-stress",
				primary: 4,
				capacityKind: "unit",
				costKind: "zero",
			}),
		},
	),
	"clustered-directed": declareFlowGeneratorFamily(
		[
			parameter("Cluster count", 2),
			parameter("Vertices per cluster", 2),
			parameter("Bridge edges", 0),
		],
		{
			estimate: estimateWith((a, b, c) => ({ nodes: a * b, edges: a * b + c })),
			payload: (form) => ({
				family_id: "clustered-directed",
				clusters: form.primary,
				cluster_size: form.secondary,
				bridge_edges: form.tertiary,
			}),
			validate: (_form, a, b, c) => {
				const nodes = a * b;
				const crossCandidates = nodes * b * (a > 0n ? a - 1n : 0n);
				return a < 2n || b < 2n
					? "Cluster count and size must be at least 2"
					: c > crossCandidates
						? "Bridge-edge count exceeds all cross-cluster candidates"
						: undefined;
			},
		},
	),
	"complete-dag": declareFlowGeneratorFamily([parameter("Vertices", 2)], {
		estimate: estimateWith((a) => ({
			nodes: a,
			edges: (a * (a > 0n ? a - 1n : 0n)) / 2n,
		})),
		payload: (form) => ({ family_id: "complete-dag", nodes: form.primary }),
		validate: (_form, a) =>
			a >= 2n ? undefined : "Vertex count must be at least 2",
	}),
	cycle: declareFlowGeneratorFamily([parameter("Vertices", 2)], {
		estimate: estimateWith((a) => ({ nodes: a, edges: a })),
		payload: (form) => ({ family_id: "cycle", nodes: form.primary }),
		validate: (_form, a) =>
			a >= 2n ? undefined : "Vertex count must be at least 2",
	}),
	"diamond-chain": declareFlowGeneratorFamily([parameter("Stages")], {
		estimate: estimateWith((a) => ({ nodes: 3n * a + 1n, edges: 4n * a })),
		payload: (form) => ({ family_id: "diamond-chain", stages: form.primary }),
		validate: (_form, a) =>
			a >= 1n ? undefined : "Stage count must be at least 1",
	}),
	"dinic-worst-case": declareFlowGeneratorFamily([parameter("Vertices", 2)], {
		estimate: estimateWith((a) => ({
			nodes: a,
			edges: a >= 2n ? 2n * a - 3n : 0n,
		})),
		payload: (form) => ({ family_id: "dinic-worst-case", nodes: form.primary }),
		fixedConstruction: () => DINIC_WORST_CASE_FIXED_COPY,
		validate: (_form, a) =>
			a >= 2n ? undefined : "Vertex count must be at least 2",
		fieldInvalid: (_form, field, a) => field === "primary" && a < 2n,
		skipDistributionValidation: true,
		sourceBacked: true,
	}),
	"erdos-renyi-directed": declareFlowGeneratorFamily(
		[parameter("Vertices", 2), parameter("Edges m", 0)],
		{
			estimate: estimateWith((a, b) => ({ nodes: a, edges: b })),
			payload: (form) => ({
				family_id: "erdos-renyi-directed",
				nodes: form.primary,
				edge_count: form.secondary,
			}),
			validate: (_form, a, b) =>
				a < 2n
					? "Vertex count must be at least 2"
					: b > a * (a - 1n)
						? "Edge count cannot exceed n(n−1)"
						: undefined,
		},
	),
	"glover-dense-acyclic-stress": declareFlowGeneratorFamily(
		[parameter("Vertices", 2, 200)],
		{
			estimate: estimateWith((a) => ({
				nodes: a,
				edges: (a * (a > 0n ? a - 1n : 0n)) / 2n,
			})),
			payload: (form) => ({
				family_id: "glover-dense-acyclic-stress",
				nodes: form.primary,
			}),
			fixedConstruction: () => GLOVER_DENSE_FIXED_COPY,
			validate: (_form, a) =>
				a >= 2n && a <= 200n
					? undefined
					: "Glover–Waissi dense stress requires 2–200 vertices",
			fieldInvalid: (_form, field, a) =>
				field === "primary" && bigintOutside(a, 2n, 200n),
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "glover-dense-acyclic-stress",
				primary: 12,
				capacityKind: "unit",
				costKind: "zero",
			}),
		},
	),
	"goldberg-mesh-circulation": declareFlowGeneratorFamily(
		[
			parameter("Columns X", 3, 32),
			parameter("Rows Y", 3, 32),
			parameter("Horizontal degree XDEG", 0, (form) =>
				Math.min(8, Math.floor((form.primary - 1) / 2)),
			),
			parameter("Vertical degree YDEG", 0, (form) =>
				Math.min(8, Math.floor((form.secondary - 1) / 2)),
			),
		],
		{
			estimate: estimateWith((a, b, c, d) => ({
				nodes: a * b,
				edges: 2n * a * b * (c + d),
			})),
			payload: (form) => ({
				family_id: "goldberg-mesh-circulation",
				columns: form.primary,
				rows: form.secondary,
				horizontal_degree: form.tertiary,
				vertical_degree: form.quaternary,
			}),
			fixedConstruction: () => GOLDBERG_MESH_FIXED_COPY,
			validate: (_form, a, b, c, d) => {
				if (a < 3n || a > 32n || b < 3n || b > 32n)
					return "Goldberg Mesh columns X and rows Y must be between 3 and 32";
				const maxHorizontal = 8n < (a - 1n) / 2n ? 8n : (a - 1n) / 2n;
				const maxVertical = 8n < (b - 1n) / 2n ? 8n : (b - 1n) / 2n;
				if (c > maxHorizontal)
					return `Horizontal degree XDEG must be between 0 and ${maxHorizontal.toString()}`;
				if (d > maxVertical)
					return `Vertical degree YDEG must be between 0 and ${maxVertical.toString()}`;
				return c + d > 0n
					? undefined
					: "At least one of horizontal or vertical degree must be 1 or greater";
			},
			fieldInvalid: (_form, field, a, b, c, d) => {
				if (field === "primary") return bigintOutside(a, 3n, 32n);
				if (field === "secondary") return bigintOutside(b, 3n, 32n);
				if (bigintOutside(a, 3n, 32n) || bigintOutside(b, 3n, 32n))
					return false;
				const maxHorizontal = 8n < (a - 1n) / 2n ? 8n : (a - 1n) / 2n;
				const maxVertical = 8n < (b - 1n) / 2n ? 8n : (b - 1n) / 2n;
				if (field === "tertiary")
					return c > maxHorizontal || (c === 0n && d === 0n);
				return (
					field === "quaternary" && (d > maxVertical || (c === 0n && d === 0n))
				);
			},
			skipDistributionValidation: true,
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "goldberg-mesh-circulation",
				primary: 4,
				secondary: 3,
				tertiary: 1,
				quaternary: 1,
				capacityKind: "unit",
				costKind: "zero",
			}),
		},
	),
	"goto-torus": declareFlowGeneratorFamily(
		[parameter("Vertices N", 15, 10_000), parameter("Edges M", 90, 100_000)],
		{
			estimate: estimateWith((a, b) => ({ nodes: a, edges: b })),
			payload: (form) => ({
				family_id: "goto-torus",
				nodes: form.primary,
				edge_count: form.secondary,
				maximum_capacity: Number(form.capacityMaximum),
				maximum_cost: Number(form.costMaximum),
			}),
			fixedConstruction: () => GOTO_FIXED_COPY,
			validate: (form, a, b) => {
				if (a < 15n || a > 10_000n)
					return "GOTO vertex count N must be between 15 and 10,000";
				if (b < 6n * a || b * b * b > a * a * a * a * a)
					return "GOTO edge count M must satisfy 6N ≤ M ≤ N^(5/3)";
				const maximumCapacity = canonicalInteger(
					form.capacityMaximum,
					8n,
					1_000_000_000n,
				);
				const maximumCost = canonicalInteger(
					form.costMaximum,
					8n,
					1_000_000_000n,
				);
				return maximumCapacity === undefined || maximumCost === undefined
					? "GOTO random-edge capacity limit U and maximum cost C must be between 8 and 1,000,000,000"
					: undefined;
			},
			fieldInvalid: (form, field, a, b) => {
				if (field === "primary") return bigintOutside(a, 15n, 10_000n);
				if (field === "secondary")
					return b < 6n * a || b * b * b > a * a * a * a * a;
				if (field === "capacityMaximum")
					return (
						canonicalInteger(form.capacityMaximum, 8n, 1_000_000_000n) ===
						undefined
					);
				return (
					field === "costMaximum" &&
					canonicalInteger(form.costMaximum, 8n, 1_000_000_000n) === undefined
				);
			},
			skipDistributionValidation: true,
			features: ["goto-torus"],
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "goto-torus",
				primary: 15,
				secondary: 90,
				capacityMaximum: "8",
				costMaximum: "8",
			}),
		},
	),
	"grid-2d": declareFlowGeneratorFamily(
		[parameter("Rows"), parameter("Columns")],
		{
			estimate: estimateWith((a, b, _c, _d, form) => ({
				nodes: a * b,
				edges:
					a * (b > 0n ? b - 1n : 0n) +
					(a > 0n ? a - 1n : 0n) * b +
					(form.toggle ? (a > 0n ? a - 1n : 0n) * (b > 0n ? b - 1n : 0n) : 0n),
			})),
			payload: (form) => ({
				family_id: "grid-2d",
				rows: form.primary,
				columns: form.secondary,
				diagonals: form.toggle,
			}),
			validate: (_form, a, b) =>
				a < 1n || b < 1n || a * b < 2n
					? "A 2D grid requires at least 2 vertices in total"
					: undefined,
			features: ["grid-toggle"],
			toggleLabel: "Diagonal edges",
		},
	),
	"grid-3d": declareFlowGeneratorFamily(
		[parameter("Layers"), parameter("Rows"), parameter("Columns")],
		{
			estimate: estimateWith((a, b, c) => ({
				nodes: a * b * c,
				edges:
					a * b * (c > 0n ? c - 1n : 0n) +
					a * (b > 0n ? b - 1n : 0n) * c +
					(a > 0n ? a - 1n : 0n) * b * c,
			})),
			payload: (form) => ({
				family_id: "grid-3d",
				layers: form.primary,
				rows: form.secondary,
				columns: form.tertiary,
			}),
			validate: (_form, a, b, c) =>
				a < 1n || b < 1n || c < 1n || a * b * c < 2n
					? "A 3D grid requires each dimension to be at least 1 and at least 2 vertices in total"
					: undefined,
		},
	),
	"gridgen-grid": declareFlowGeneratorFamily(
		[
			parameter("Rows", 2, 1_000),
			parameter("Columns", 2, 1_000),
			parameter("terminal pairs"),
		],
		{
			estimate: estimateWith((a, b, c, d, form) => {
				const nodes = a * b + 1n;
				const links = a * (b > 0n ? b - 1n : 0n) + (a > 0n ? a - 1n : 0n) * b;
				const basicEdges = (form.toggle ? 2n : 1n) * links + 2n * c;
				return {
					nodes,
					edges: basicEdges > nodes * d ? basicEdges : nodes * d,
				};
			}),
			payload: (form) => ({
				family_id: "gridgen-grid",
				rows: form.primary,
				columns: form.secondary,
				terminal_pairs: form.tertiary,
				average_degree: form.quaternary,
				total_supply: Number(form.gridgenTotalSupply),
				two_way: form.toggle,
				minimum_capacity: Number(form.capacityMinimum),
				maximum_capacity: Number(form.capacityMaximum),
				minimum_cost: Number(form.costMinimum),
				maximum_cost: Number(form.costMaximum),
			}),
			fixedConstruction: () => GRIDGEN_FIXED_COPY,
			validate: (form, a, b, c, d) => {
				if (a < 2n || a > 1_000n || b < 2n || b > 1_000n)
					return "GRIDGEN rows and columns must be between 2 and 1,000";
				if (c < 1n || 2n * c > a * b)
					return "Terminal pairs must be at least 1 and no more than half the grid vertices";
				if (d < 1n || d > a * b)
					return "Average degree must be between 1 and the grid vertex count";
				const totalSupply = canonicalInteger(
					form.gridgenTotalSupply,
					1n,
					1_000_000_000n,
				);
				if (totalSupply === undefined || totalSupply < c)
					return "Total supply must be between the terminal-pair count and 1,000,000,000";
				const capacityMinimum = canonicalInteger(
					form.capacityMinimum,
					0n,
					1_000_000_000n,
				);
				const capacityMaximum = canonicalInteger(
					form.capacityMaximum,
					0n,
					1_000_000_000n,
				);
				const costMinimum = canonicalInteger(
					form.costMinimum,
					0n,
					1_000_000_000n,
				);
				const costMaximum = canonicalInteger(
					form.costMaximum,
					0n,
					1_000_000_000n,
				);
				return capacityMinimum === undefined ||
					capacityMaximum === undefined ||
					capacityMinimum > capacityMaximum ||
					costMinimum === undefined ||
					costMaximum === undefined ||
					costMinimum > costMaximum
					? "GRIDGEN capacity and cost ranges must satisfy 0 ≤ min ≤ max ≤ 1,000,000,000"
					: undefined;
			},
			fieldInvalid: (form, field, a, b, c, d) => {
				const terminalInvalid = c < 1n || 2n * c > a * b;
				if (field === "primary") return bigintOutside(a, 2n, 1_000n);
				if (field === "secondary") return bigintOutside(b, 2n, 1_000n);
				if (field === "tertiary") return terminalInvalid;
				if (field === "quaternary") return d < 1n || d > a * b;
				if (field === "gridgenTotalSupply") {
					const total = canonicalInteger(
						form.gridgenTotalSupply,
						1n,
						1_000_000_000n,
					);
					return total === undefined || total < c;
				}
				if (field === "capacityMinimum" || field === "capacityMaximum") {
					const minimum = canonicalInteger(
						form.capacityMinimum,
						0n,
						1_000_000_000n,
					);
					const maximum = canonicalInteger(
						form.capacityMaximum,
						0n,
						1_000_000_000n,
					);
					return (
						minimum === undefined || maximum === undefined || minimum > maximum
					);
				}
				if (field === "costMinimum" || field === "costMaximum") {
					const minimum = canonicalInteger(
						form.costMinimum,
						0n,
						1_000_000_000n,
					);
					const maximum = canonicalInteger(
						form.costMaximum,
						0n,
						1_000_000_000n,
					);
					return (
						minimum === undefined || maximum === undefined || minimum > maximum
					);
				}
				return false;
			},
			skipDistributionValidation: true,
			features: ["grid-toggle", "gridgen"],
			sourceBacked: true,
			toggleLabel: "Make grid links bidirectional",
			defaults: (form) => ({
				...form,
				family: "gridgen-grid",
				primary: 2,
				secondary: 2,
				tertiary: 1,
				quaternary: 1,
				toggle: true,
				gridgenTotalSupply: "1",
				capacityMinimum: "1",
				capacityMaximum: "12",
				costMinimum: "0",
				costMaximum: "9",
			}),
		},
	),
	"gridgraph-grid": declareFlowGeneratorFamily(
		[parameter("Rows W", 2, 1_000), parameter("Columns L", 3, 1_000)],
		{
			estimate: estimateWith((a, b) => ({
				nodes: a * b + 2n,
				edges: a * (b > 0n ? b - 1n : 0n) + (a > 0n ? a - 1n : 0n) * b + 2n * a,
			})),
			payload: (form) => ({
				family_id: "gridgraph-grid",
				rows: form.primary,
				columns: form.secondary,
				maximum_capacity: Number(form.capacityMaximum),
				maximum_cost: Number(form.costMaximum),
			}),
			fixedConstruction: gridgraphFixedCopy,
			validate: (form, a, b) => {
				if (a < 2n || a > 1_000n || b < 3n || b > 1_000n)
					return "GRIDGRAPH rows W must be 2–1,000 and columns L must be 3–1,000";
				if (a * b + 2n > 2_000n)
					return "GRIDGRAPH is limited to 2,000 vertices including source and sink so max flow can be certified during generation";
				const maximumCapacity = canonicalInteger(
					form.capacityMaximum,
					1n,
					1_000_000_000n,
				);
				const maximumCost = canonicalInteger(
					form.costMaximum,
					1n,
					1_000_000_000n,
				);
				return maximumCapacity === undefined || maximumCost === undefined
					? "GRIDGRAPH maximum capacity and cost must be between 1 and 1,000,000,000"
					: undefined;
			},
			fieldInvalid: (form, field, a, b) => {
				const shapeLimit = a * b + 2n > 2_000n;
				if (field === "primary")
					return bigintOutside(a, 2n, 1_000n) || shapeLimit;
				if (field === "secondary")
					return bigintOutside(b, 3n, 1_000n) || shapeLimit;
				if (field === "capacityMaximum")
					return (
						canonicalInteger(form.capacityMaximum, 1n, 1_000_000_000n) ===
						undefined
					);
				return (
					field === "costMaximum" &&
					canonicalInteger(form.costMaximum, 1n, 1_000_000_000n) === undefined
				);
			},
			skipDistributionValidation: true,
			features: ["gridgraph"],
			presets: GRIDGRAPH_PRESET_OPTIONS,
			presetKey: "gridgraphPreset",
			sourceBacked: true,
			statusId: "flow-generator-gridgraph-shape",
			defaults: (form) => applyGridgraphPreset(form, "readable"),
		},
	),
	"hall-tight-bipartite": declareFlowGeneratorFamily(
		[parameter("Vertices per side", 2), parameter("Tight prefix")],
		{
			estimate: estimateWith((a, b) => ({
				nodes: 2n + 2n * a,
				edges: 2n * a + b * b + (a > b ? (a - b) * a : 0n),
			})),
			payload: (form) => ({
				family_id: "hall-tight-bipartite",
				part_size: form.primary,
				tight_prefix: form.secondary,
			}),
			fixedConstruction: () => HALL_TIGHT_FIXED_COPY,
			validate: (_form, a, b) =>
				a < 2n
					? "Each side must have at least 2 vertices"
					: b < 1n || b >= a
						? "Tight prefix must be at least 1 and smaller than each side"
						: undefined,
			skipDistributionValidation: true,
			defaults: (form) => ({
				...form,
				family: "hall-tight-bipartite",
				primary: 8,
				secondary: 3,
			}),
		},
	),
	ladder: declareFlowGeneratorFamily([parameter("Columns", 2)], {
		estimate: estimateWith((a, _b, _c, _d, form) => ({
			nodes: 2n * a,
			edges:
				a +
				2n * (a > 0n ? a - 1n : 0n) +
				(form.toggle ? 2n * (a > 0n ? a - 1n : 0n) : 0n),
		})),
		payload: (form) => ({
			family_id: "ladder",
			columns: form.primary,
			cross_edges: form.toggle,
		}),
		validate: (_form, a) =>
			a >= 2n ? undefined : "Column count must be at least 2",
		features: ["grid-toggle"],
		toggleLabel: "Cross edges",
	}),
	"layered-dag": declareFlowGeneratorFamily(
		[
			parameter("Internal layers"),
			parameter("Layer width"),
			parameter("Fanout"),
		],
		{
			estimate: estimateWith((a, b, c) => ({
				nodes: 2n + a * b,
				edges: 2n * b + (a > 0n ? a - 1n : 0n) * b * c,
			})),
			payload: (form) => ({
				family_id: "layered-dag",
				layers: form.primary,
				width: form.secondary,
				fanout: form.tertiary,
			}),
			validate: (_form, a, b, c) =>
				a < 1n || b < 1n || c < 1n
					? "Layer count, width, and fanout must be at least 1"
					: c > b
						? "Fanout cannot exceed layer width"
						: undefined,
		},
	),
	"multi-source-sink": declareFlowGeneratorFamily(
		[
			parameter("Source vertices"),
			parameter("Intermediate vertices"),
			parameter("Sink vertices"),
		],
		{
			estimate: estimateWith((a, b, c) => ({
				nodes: 2n + a + b + c,
				edges: a + a * b + b * c + c,
			})),
			payload: (form) => ({
				family_id: "multi-source-sink",
				sources: form.primary,
				intermediate: form.secondary,
				sinks: form.tertiary,
			}),
			validate: (_form, a, b, c) =>
				a >= 1n && b >= 1n && c >= 1n
					? undefined
					: "Source, intermediate, and sink vertex counts must each be at least 1",
		},
	),
	"netgen-skeleton": declareFlowGeneratorFamily(
		[
			parameter("Nodes N", 2, 10_000),
			parameter("Sources S", 1, 10_000),
			parameter("Sinks T", 1, 10_000),
			parameter("Edges M", 1, 100_000),
		],
		{
			estimate: estimateWith((a, _b, _c, d) => ({ nodes: a, edges: d })),
			payload: (form) => ({
				family_id: "netgen-skeleton",
				nodes: form.primary,
				sources: form.secondary,
				sinks: form.tertiary,
				edge_count: form.quaternary,
				minimum_cost: Number(form.costMinimum),
				maximum_cost: Number(form.costMaximum),
				total_supply: Number(form.netgenTotalSupply),
				transshipment_sources: form.netgenTransshipmentSources,
				transshipment_sinks: form.netgenTransshipmentSinks,
				high_cost_percentage: form.netgenHighCostPercentage,
				capacitated_percentage: form.netgenCapacitatedPercentage,
				minimum_capacity: Number(form.capacityMinimum),
				maximum_capacity: Number(form.capacityMaximum),
			}),
			fixedConstruction: netgenFixedCopy,
			validate: (form) => validateNetgenForm(form),
			fieldInvalid: netgenFieldInvalid,
			skipDistributionValidation: true,
			features: ["netgen"],
			presets: NETGEN_PRESET_OPTIONS,
			presetKey: "netgenPreset",
			sourceBacked: true,
			statusId: "flow-generator-netgen-kind",
			defaults: (form) => applyNetgenPreset(form, "general-min-cost"),
		},
	),
	"parallel-paths": declareFlowGeneratorFamily(
		[parameter("Paths"), parameter("Internal vertices per path", 0)],
		{
			estimate: estimateWith((a, b) => ({
				nodes: 2n + a * b,
				edges: a * (b + 1n),
			})),
			payload: (form) => ({
				family_id: "parallel-paths",
				path_count: form.primary,
				internal_nodes: form.secondary,
			}),
			validate: (_form, a) =>
				a >= 1n ? undefined : "Path count must be at least 1",
		},
	),
	path: declareFlowGeneratorFamily([parameter("Vertices", 2)], {
		estimate: estimateWith((a) => ({
			nodes: a,
			edges: a > 0n ? a - 1n : 0n,
		})),
		payload: (form) => ({ family_id: "path", nodes: form.primary }),
		validate: (_form, a) =>
			a >= 2n ? undefined : "Vertex count must be at least 2",
	}),
	"planar-triangulated": declareFlowGeneratorFamily(
		[parameter("Boundary vertices", 3)],
		{
			estimate: estimateWith((a) => ({
				nodes: a,
				edges: a >= 2n ? 2n * a - 3n : 0n,
			})),
			payload: (form) => ({
				family_id: "planar-triangulated",
				nodes: form.primary,
			}),
			validate: (_form, a) =>
				a >= 3n ? undefined : "Boundary vertex count must be at least 3",
		},
	),
	"planted-bottleneck": declareFlowGeneratorFamily(
		[
			parameter("Left vertices"),
			parameter("Right vertices"),
			parameter("Cut edges"),
		],
		{
			estimate: estimateWith((a, b, c) => ({
				nodes: 2n + a + b,
				edges: a + b + c,
			})),
			payload: (form) => ({
				family_id: "planted-bottleneck",
				left: form.primary,
				right: form.secondary,
				cut_edges: form.tertiary,
			}),
			fixedConstruction: () => PLANTED_BOTTLENECK_FIXED_COPY,
			validate: (_form, a, b, c) =>
				a < 1n || b < 1n || c < 1n
					? "Left size, right size, and cut-edge count must be at least 1"
					: c > a * b
						? "Cut-edge count cannot exceed L×R"
						: undefined,
			skipDistributionValidation: true,
		},
	),
	"preferential-attachment-directed": declareFlowGeneratorFamily(
		[parameter("Vertices", 2), parameter("Existing vertices selected m")],
		{
			estimate: estimateWith((a, b) => {
				const seed = b + 1n;
				return {
					nodes: a,
					edges: (seed * b) / 2n + (a > seed ? (a - seed) * b : 0n),
				};
			}),
			payload: (form) => ({
				family_id: "preferential-attachment-directed",
				nodes: form.primary,
				attachment_count: form.secondary,
			}),
			validate: (_form, a, b) =>
				a < 2n
					? "Vertex count must be at least 2"
					: b < 1n || b >= a
						? "Selection count m must satisfy 1 ≤ m < n"
						: undefined,
			fieldInvalid: (_form, field, a, b) =>
				field === "primary"
					? a < 2n
					: field === "secondary" && (b < 1n || b >= a),
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "preferential-attachment-directed",
				primary: 30,
				secondary: 3,
			}),
		},
	),
	"random-dag": declareFlowGeneratorFamily(
		[parameter("Vertices", 2), parameter("Edges m", 0)],
		{
			estimate: estimateWith((a, b) => ({ nodes: a, edges: b })),
			payload: (form) => ({
				family_id: "random-dag",
				nodes: form.primary,
				edge_count: form.secondary,
			}),
			validate: (_form, a, b) =>
				a < 2n
					? "Vertex count must be at least 2"
					: b > (a * (a - 1n)) / 2n
						? "Edge count cannot exceed n(n−1)/2"
						: undefined,
		},
	),
	"random-geometric": declareFlowGeneratorFamily(
		[parameter("Vertices", 2), parameter("Connection radius")],
		{
			estimate: estimateWith((a) => ({
				nodes: a,
				edges: (a * (a > 0n ? a - 1n : 0n)) / 2n,
			})),
			payload: (form) => ({
				family_id: "random-geometric",
				nodes: form.primary,
				radius: form.secondary,
			}),
			validate: (form, a, b) =>
				a < 2n
					? "Vertex count must be at least 2"
					: b < 1n || b > 1_000n
						? "Connection radius must be between 1 and 1,000"
						: flowGeneratorFamilyEntry("random-geometric").estimate(form)
									.edges > 100_000n
							? "The geometric graph's safe candidate-edge bound exceeds 100,000"
							: undefined,
			fieldInvalid: (_form, field, a, b) =>
				field === "primary"
					? bigintOutside(a, 2n, 448n)
					: field === "secondary" && bigintOutside(b, 1n, 1_000n),
			sourceBacked: true,
			estimateIsUpperBound: () => true,
		},
	),
	"random-regular-directed": declareFlowGeneratorFamily(
		[parameter("Vertices", 2), parameter("In-degree = out-degree")],
		{
			estimate: estimateWith((a, b) => ({ nodes: a, edges: a * b })),
			payload: (form) => ({
				family_id: "random-regular-directed",
				nodes: form.primary,
				degree: form.secondary,
			}),
			validate: (_form, a, b) =>
				a < 2n
					? "Vertex count must be at least 2"
					: b < 1n || b >= a
						? "Degree must satisfy 1 ≤ degree < n"
						: undefined,
			defaults: (form) => ({
				...form,
				family: "random-regular-directed",
				primary: 20,
				secondary: 3,
			}),
		},
	),
	"rmfgen-frames": declareFlowGeneratorFamily(
		[parameter("frame size a", 2, 1_000), parameter("depth b", 1, 1_000)],
		{
			estimate: estimateWith((a, b) => ({
				nodes: a * a * b,
				edges:
					4n * b * a * (a > 0n ? a - 1n : 0n) + a * a * (b > 0n ? b - 1n : 0n),
			})),
			payload: (form) => ({
				family_id: "rmfgen-frames",
				frame_size: form.primary,
				depth: form.secondary,
				minimum_capacity: Number(form.capacityMinimum),
				maximum_capacity: Number(form.capacityMaximum),
			}),
			fixedConstruction: () => RMFGEN_FIXED_COPY,
			validate: (form, a, b) => {
				if (a < 2n || a > 1_000n || b < 1n || b > 1_000n)
					return "Frame size must be 2–1,000 and depth must be 1–1,000";
				const minimum = canonicalInteger(form.capacityMinimum, 0n, 1_000n);
				const maximum = canonicalInteger(form.capacityMaximum, 0n, 1_000n);
				return minimum === undefined ||
					maximum === undefined ||
					minimum > maximum
					? "RMFGEN capacities must satisfy 0 ≤ c1 ≤ c2 ≤ 1,000"
					: undefined;
			},
			fieldInvalid: (form, field, a, b) => {
				const estimate =
					flowGeneratorFamilyEntry("rmfgen-frames").estimate(form);
				const shapeLimit =
					estimate.nodes > 10_000n || estimate.edges > 100_000n;
				if (field === "primary")
					return bigintOutside(a, 2n, 1_000n) || shapeLimit;
				if (field === "secondary")
					return bigintOutside(b, 1n, 1_000n) || shapeLimit;
				if (field === "capacityMinimum" || field === "capacityMaximum") {
					const minimum = canonicalInteger(form.capacityMinimum, 0n, 1_000n);
					const maximum = canonicalInteger(form.capacityMaximum, 0n, 1_000n);
					return (
						minimum === undefined || maximum === undefined || minimum > maximum
					);
				}
				return false;
			},
			skipDistributionValidation: true,
			features: ["rmfgen"],
			sourceBacked: true,
		},
	),
	"strongly-connected": declareFlowGeneratorFamily(
		[parameter("Vertices", 3), parameter("Extra edges", 0)],
		{
			estimate: estimateWith((a, b) => ({ nodes: a, edges: a + b })),
			payload: (form) => ({
				family_id: "strongly-connected",
				nodes: form.primary,
				extra_edges: form.secondary,
			}),
			validate: (_form, a, b) =>
				a < 3n
					? "Vertex count must be at least 3"
					: b > a * (a - 2n)
						? "Extra-edge count cannot exceed n(n−2)"
						: undefined,
		},
	),
	torus: declareFlowGeneratorFamily(
		[parameter("Rows", 3), parameter("Columns", 3)],
		{
			estimate: estimateWith((a, b) => ({ nodes: a * b, edges: 2n * a * b })),
			payload: (form) => ({
				family_id: "torus",
				rows: form.primary,
				columns: form.secondary,
			}),
			validate: (_form, a, b) =>
				a >= 3n && b >= 3n
					? undefined
					: "Each torus dimension must be at least 3",
		},
	),
	"transportation-table": declareFlowGeneratorFamily(
		[
			parameter("Origins", 1, 255),
			parameter("Destinations", 1, 255),
			parameter("Total shipment B", 1, 1_000_000_000),
		],
		{
			estimate: estimateWith((a, b, _c, d, form) => {
				const candidates = a * b;
				const sparseDensity = (candidates * d + 999n) / 1_000n;
				const sparseSupport = a + b > 0n ? a + b - 1n : 0n;
				const edges =
					form.transportationShape === "cut-infeasible"
						? 1n + (a > 0n ? a - 1n : 0n) * b
						: form.transportationShape === "sparse-feasible"
							? sparseDensity > sparseSupport
								? sparseDensity
								: sparseSupport
							: candidates;
				return { nodes: a + b, edges };
			}),
			payload: (form) => ({
				family_id: "transportation-table",
				origins: form.primary,
				destinations: form.secondary,
				total_supply: form.tertiary,
				shape: transportationShapePayload(form),
			}),
			fixedConstruction: transportationFixedCopy,
			validate: validateTransportationTableForm,
			fieldInvalid: transportationTableFieldInvalid,
			skipDistributionValidation: true,
			features: ["transportation-shape"],
			presets: TRANSPORTATION_SHAPE_OPTIONS,
			sourceBacked: true,
			defaults: (form) => applyTransportationShape(form, "sparse-feasible"),
			estimateIsUpperBound: (form) =>
				form.transportationShape === "sparse-feasible",
		},
	),
	"vision-segmentation-grid": declareFlowGeneratorFamily(
		[parameter("Rows", 1, 254), parameter("Columns", 1, 254)],
		{
			estimate: estimateWith((a, b, _c, _d, form) => {
				const horizontalPairs = a * (b > 0n ? b - 1n : 0n);
				const verticalPairs = (a > 0n ? a - 1n : 0n) * b;
				const diagonalPairs = form.toggle
					? 2n * (a > 0n ? a - 1n : 0n) * (b > 0n ? b - 1n : 0n)
					: 0n;
				return {
					nodes: a * b + 2n,
					edges:
						2n * a * b + 2n * (horizontalPairs + verticalPairs + diagonalPairs),
				};
			}),
			payload: (form) => ({
				family_id: "vision-segmentation-grid",
				rows: form.primary,
				columns: form.secondary,
				eight_neighbor: form.toggle,
			}),
			validate: (form, a, b) => {
				const estimate = flowGeneratorFamilyEntry(
					"vision-segmentation-grid",
				).estimate(form);
				return a < 1n || b < 1n
					? "Image-segmentation grid rows and columns must be at least 1"
					: estimate.nodes > 256n || estimate.edges > 2_048n
						? "Stay within 256 vertices and 2,048 edges for the Boykov–Kolmogorov trace limit"
						: undefined;
			},
			fieldInvalid: (form, field, a, b) => {
				const estimate = flowGeneratorFamilyEntry(
					"vision-segmentation-grid",
				).estimate(form);
				const shapeInvalid =
					a < 1n || b < 1n || estimate.nodes > 256n || estimate.edges > 2_048n;
				return (field === "primary" || field === "secondary") && shapeInvalid;
			},
			features: ["grid-toggle", "vision-grid"],
			sourceBacked: true,
			toggleLabel: "8-neighbor (add diagonal n-links)",
			defaults: (form) => ({
				...form,
				family: "vision-segmentation-grid",
				primary: 6,
				secondary: 8,
				toggle: false,
				capacityKind: "uniform",
				capacityMinimum: "1",
				capacityMaximum: "12",
				costKind: "zero",
			}),
		},
	),
	"waissi-setubal-acyclic-dense": declareFlowGeneratorFamily(
		[parameter("Vertices", 2, 200)],
		{
			estimate: estimateWith((a) => ({
				nodes: a,
				edges: (a * (a > 0n ? a - 1n : 0n)) / 2n,
			})),
			payload: (form) => ({
				family_id: "waissi-setubal-acyclic-dense",
				nodes: form.primary,
			}),
			fixedConstruction: () => WAISSI_AC_FIXED_COPY,
			validate: (_form, a) =>
				a >= 2n && a <= 200n
					? undefined
					: "First DIMACS AC requires 2–200 vertices",
			fieldInvalid: (_form, field, a) =>
				field === "primary" && bigintOutside(a, 2n, 200n),
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "waissi-setubal-acyclic-dense",
				primary: 12,
				capacityKind: "unit",
				costKind: "zero",
			}),
		},
	),
	"waissi-transit-one-way-grid": declareFlowGeneratorFamily(
		[
			parameter("Side length d", 2, 44),
			parameter("Maximum capacity U", 1, 1_000_000_000),
		],
		{
			estimate: estimateWith((a) => ({
				nodes: a * a + 2n,
				edges: 2n * a * a,
			})),
			payload: (form) => ({
				family_id: "waissi-transit-one-way-grid",
				dimension: form.primary,
				maximum_capacity: form.secondary,
			}),
			fixedConstruction: () => WAISSI_TRANSIT_ONE_WAY_FIXED_COPY,
			validate: (_form, a, b) =>
				a >= 2n && a <= 44n && b >= 1n && b <= 1_000_000_000n
					? undefined
					: "Waissi one-way transit grid requires d=2–44 and U=1–1,000,000,000",
			fieldInvalid: (_form, field, a, b) =>
				field === "primary"
					? bigintOutside(a, 2n, 44n)
					: field === "secondary" && bigintOutside(b, 1n, 1_000_000_000n),
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "waissi-transit-one-way-grid",
				primary: 4,
				secondary: 100,
				capacityKind: "unit",
				costKind: "zero",
			}),
		},
	),
	"waissi-transit-two-way-grid": declareFlowGeneratorFamily(
		[
			parameter("Side length d", 2, 44),
			parameter("Maximum capacity U", 1, 1_000_000_000),
		],
		{
			estimate: estimateWith((a) => ({
				nodes: a * a + 2n,
				edges: 4n * a * a,
			})),
			payload: (form) => ({
				family_id: "waissi-transit-two-way-grid",
				dimension: form.primary,
				maximum_capacity: form.secondary,
			}),
			fixedConstruction: () => WAISSI_TRANSIT_TWO_WAY_FIXED_COPY,
			validate: (_form, a, b) =>
				a >= 2n && a <= 44n && b >= 1n && b <= 1_000_000_000n
					? undefined
					: "Waissi two-way transit grid requires d=2–44 and U=1–1,000,000,000",
			fieldInvalid: (_form, field, a, b) =>
				field === "primary"
					? bigintOutside(a, 2n, 44n)
					: field === "secondary" && bigintOutside(b, 1n, 1_000_000_000n),
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "waissi-transit-two-way-grid",
				primary: 4,
				secondary: 100,
				capacityKind: "unit",
				costKind: "zero",
			}),
		},
	),
	"washington-basic-line": declareFlowGeneratorFamily(
		[
			parameter("Blocks n", 2),
			parameter("Block width m"),
			parameter("offset degree d", 1, 20),
		],
		{
			estimate: estimateWith((a, b, c) => ({
				nodes: a * b + 2n,
				edges: a * b * c + 2n * b,
			})),
			payload: (form) => ({
				family_id: "washington-basic-line",
				levels: form.primary,
				width: form.secondary,
				degree: form.tertiary,
			}),
			fixedConstruction: () => WASHINGTON_BASIC_LINE_FIXED_COPY,
			validate: (_form, a, b, c) => {
				if (a < 2n || b < 1n)
					return "Washington Line requires at least 2 blocks and width at least 1";
				if (c < 1n || c > 20n)
					return "Washington Line degree must be between 1 and 20";
				if (a * b + 2n > 2_000n)
					return "Washington Line is limited to 2,000 vertices including source and sink";
				return a * b * c + 2n * b > 20_000n
					? "Washington Line exceeds the conservative 20,000-edge limit"
					: undefined;
			},
			fieldInvalid: washingtonLineFieldInvalid(() => 20n),
			skipDistributionValidation: true,
			sourceBacked: true,
			estimateIsUpperBound: () => true,
			defaults: (form) => ({
				...form,
				family: "washington-basic-line",
				primary: 8,
				secondary: 4,
				tertiary: 3,
				capacityKind: "unit",
				costKind: "zero",
			}),
		},
	),
	"washington-cheriyan-stress": declareFlowGeneratorFamily(
		[
			parameter("n · bridge width / gateway capacity", 1, 64),
			parameter("m · gadget entries", 1, 12),
			parameter("c · chain length between entries", 1, 10),
		],
		{
			estimate: estimateWith((a, b, c) => ({
				nodes: 4n * b * c + 2n * a + 7n,
				edges: 4n * b * (c + 1n) + 3n * a + 3n,
			})),
			payload: (form) => ({
				family_id: "washington-cheriyan-stress",
				bridge_width: form.primary,
				gadget_entries: form.secondary,
				chain_length: form.tertiary,
			}),
			fixedConstruction: washingtonCheriyanFixedCopy,
			validate: (_form, a, b, c) =>
				a >= 1n && a <= 64n && b >= 1n && b <= 12n && c >= 1n && c <= 10n
					? undefined
					: "Washington Cheriyan requires n=1–64, m=1–12, and c=1–10",
			fieldInvalid: (_form, field, a, b, c) =>
				field === "primary"
					? bigintOutside(a, 1n, 64n)
					: field === "secondary"
						? bigintOutside(b, 1n, 12n)
						: field === "tertiary" && bigintOutside(c, 1n, 10n),
			skipDistributionValidation: true,
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "washington-cheriyan-stress",
				primary: 4,
				secondary: 2,
				tertiary: 2,
				capacityKind: "unit",
				costKind: "zero",
			}),
		},
	),
	"washington-dinic-phase-stress": declareFlowGeneratorFamily(
		[parameter("Vertices", 2, 2_000)],
		{
			estimate: estimateWith((a) => ({
				nodes: a,
				edges: a >= 2n ? 2n * a - 3n : 0n,
			})),
			payload: (form) => ({
				family_id: "washington-dinic-phase-stress",
				nodes: form.primary,
			}),
			fixedConstruction: washingtonDinicFixedCopy,
			validate: (_form, a) =>
				a >= 2n && a <= 2_000n
					? undefined
					: "Washington Dinic phase stress requires 2–2,000 vertices",
			fieldInvalid: (_form, field, a) =>
				field === "primary" && bigintOutside(a, 2n, 2_000n),
			skipDistributionValidation: true,
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "washington-dinic-phase-stress",
				primary: 12,
				capacityKind: "unit",
				costKind: "zero",
			}),
		},
	),
	"washington-double-exponential-line": declareFlowGeneratorFamily(
		[
			parameter("Blocks n", 2),
			parameter("Block width m"),
			parameter("offset degree d", 1, 19),
		],
		{
			estimate: estimateWith((a, b, c) => ({
				nodes: a * b + 2n,
				edges: a * b * c + 2n * b,
			})),
			payload: (form) => ({
				family_id: "washington-double-exponential-line",
				levels: form.primary,
				width: form.secondary,
				degree: form.tertiary,
			}),
			fixedConstruction: () => WASHINGTON_DOUBLE_EXPONENTIAL_LINE_FIXED_COPY,
			validate: (_form, a, b, c) => {
				if (a < 2n || b < 1n)
					return "Washington Line requires at least 2 blocks and width at least 1";
				const maximumDegree = b === 1n ? 18n : 19n;
				if (c < 1n || c > maximumDegree)
					return `Washington Line degree must be between 1 and ${maximumDegree}`;
				if (a * b + 2n > 2_000n)
					return "Washington Line is limited to 2,000 vertices including source and sink";
				return a * b * c + 2n * b > 20_000n
					? "Washington Line exceeds the conservative 20,000-edge limit"
					: undefined;
			},
			fieldInvalid: washingtonLineFieldInvalid((width) =>
				width === 1n ? 18n : 19n,
			),
			skipDistributionValidation: true,
			sourceBacked: true,
			estimateIsUpperBound: () => true,
			defaults: (form) => ({
				...form,
				family: "washington-double-exponential-line",
				primary: 8,
				secondary: 4,
				tertiary: 3,
				capacityKind: "unit",
				costKind: "zero",
			}),
		},
	),
	"washington-exponential-line": declareFlowGeneratorFamily(
		[
			parameter("Blocks n", 2),
			parameter("Block width m"),
			parameter("offset degree d", 1, 20),
		],
		{
			estimate: estimateWith((a, b, c) => ({
				nodes: a * b + 2n,
				edges: a * b * c + 2n * b,
			})),
			payload: (form) => ({
				family_id: "washington-exponential-line",
				levels: form.primary,
				width: form.secondary,
				degree: form.tertiary,
			}),
			fixedConstruction: () => WASHINGTON_EXPONENTIAL_LINE_FIXED_COPY,
			validate: (_form, a, b, c) => {
				if (a < 2n || b < 1n)
					return "Washington Line requires at least 2 blocks and width at least 1";
				if (c < 1n || c > 20n)
					return "Washington Line degree must be between 1 and 20";
				if (a * b + 2n > 2_000n)
					return "Washington Line is limited to 2,000 vertices including source and sink";
				return a * b * c + 2n * b > 20_000n
					? "Washington Line exceeds the conservative 20,000-edge limit"
					: undefined;
			},
			fieldInvalid: washingtonLineFieldInvalid(() => 20n),
			skipDistributionValidation: true,
			sourceBacked: true,
			estimateIsUpperBound: () => true,
			defaults: (form) => ({
				...form,
				family: "washington-exponential-line",
				primary: 8,
				secondary: 4,
				tertiary: 3,
				capacityKind: "unit",
				costKind: "zero",
			}),
		},
	),
	"washington-goldberg-fifo-stress": declareFlowGeneratorFamily(
		[parameter("block size k", 2, 64)],
		{
			estimate: estimateWith((a) => ({
				nodes: 3n * a + 3n,
				edges: a >= 2n ? 4n * a + 1n : 0n,
			})),
			payload: (form) => ({
				family_id: "washington-goldberg-fifo-stress",
				block_size: form.primary,
			}),
			fixedConstruction: washingtonFifoFixedCopy,
			validate: (_form, a) =>
				a >= 2n && a <= 64n
					? undefined
					: "Washington FIFO stress block size must be between 2 and 64",
			fieldInvalid: (_form, field, a) =>
				field === "primary" && bigintOutside(a, 2n, 64n),
			skipDistributionValidation: true,
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "washington-goldberg-fifo-stress",
				primary: 4,
				capacityKind: "unit",
				costKind: "zero",
			}),
		},
	),
	"washington-matching": declareFlowGeneratorFamily(
		[
			parameter("Vertices per side n", 2, 999),
			parameter("Left-vertex degree d", 1, 999),
		],
		{
			estimate: estimateWith((a, b) => ({
				nodes: 2n * a + 2n,
				edges: a * (b + 2n),
			})),
			payload: (form) => ({
				family_id: "washington-matching",
				part_size: form.primary,
				degree: form.secondary,
			}),
			fixedConstruction: washingtonMatchingFixedCopy,
			validate: (_form, a, b) => {
				if (a < 2n || a > 999n)
					return "Washington Matching side size n must be between 2 and 999";
				if (b < 1n || b > a)
					return "Washington Matching degree d must be between 1 and n";
				return a * (b + 2n) > 20_000n
					? "Washington Matching requires n(d+2) ≤ 20,000 edges to keep traces practical"
					: undefined;
			},
			fieldInvalid: (_form, field, a, b) => {
				const edgeLimit = a * (b + 2n) > 20_000n;
				if (field === "primary") return bigintOutside(a, 2n, 999n) || edgeLimit;
				return field === "secondary" && (b < 1n || b > a || edgeLimit);
			},
			skipDistributionValidation: true,
			features: ["washington-matching"],
			presets: WASHINGTON_MATCHING_PRESET_OPTIONS,
			presetKey: "washingtonMatchingPreset",
			sourceBacked: true,
			statusId: "flow-generator-washington-matching-shape",
			defaults: (form) => applyWashingtonMatchingPreset(form, "readable"),
		},
	),
	"washington-mesh": declareFlowGeneratorFamily(
		[
			parameter("Vertices per level R", 3, 1_000),
			parameter("Levels L", 2, 1_000),
		],
		{
			estimate: estimateWith((a, b) => ({
				nodes: a * b + 2n,
				edges: 3n * a * b - a,
			})),
			payload: (form) => ({
				family_id: "washington-mesh",
				rows: form.primary,
				columns: form.secondary,
				maximum_capacity: Number(form.capacityMaximum),
			}),
			fixedConstruction: washingtonMeshFixedCopy,
			validate: (form, a, b) => {
				if (a < 3n || a > 1_000n || b < 2n || b > 1_000n)
					return "Washington Mesh requires 3–1,000 vertices per level and 2–1,000 levels";
				if (a * b + 2n > 2_000n)
					return "Washington Mesh is limited to 2,000 vertices including source and sink";
				return canonicalInteger(form.capacityMaximum, 1n, 100_000_000n) ===
					undefined
					? "Washington Mesh maximum capacity C must be between 1 and 100,000,000"
					: undefined;
			},
			fieldInvalid: washingtonLevelFieldInvalid,
			skipDistributionValidation: true,
			features: ["washington-level"],
			presets: WASHINGTON_PRESET_OPTIONS,
			presetKey: "washingtonPreset",
			applyWashingtonLevelPreset: (form, preset) =>
				applyWashingtonPreset(form, preset, "washington-mesh"),
			sourceBacked: true,
			statusId: "flow-generator-washington-shape",
			defaults: (form) =>
				applyWashingtonPreset(form, "readable", "washington-mesh"),
		},
	),
	"washington-random-level": declareFlowGeneratorFamily(
		[
			parameter("Vertices per level R", 3, 1_000),
			parameter("Levels L", 2, 1_000),
		],
		{
			estimate: estimateWith((a, b) => ({
				nodes: a * b + 2n,
				edges: 3n * a * b - a,
			})),
			payload: (form) => ({
				family_id: "washington-random-level",
				rows: form.primary,
				columns: form.secondary,
				maximum_capacity: Number(form.capacityMaximum),
			}),
			fixedConstruction: washingtonRandomLevelFixedCopy,
			validate: (form, a, b) => {
				if (a < 3n || a > 1_000n || b < 2n || b > 1_000n)
					return "Washington Random Level requires 3–1,000 vertices per level and 2–1,000 levels";
				if (a * b + 2n > 2_000n)
					return "Washington Random Level is limited to 2,000 vertices including source and sink";
				return canonicalInteger(form.capacityMaximum, 1n, 100_000_000n) ===
					undefined
					? "Washington Random Level maximum capacity C must be between 1 and 100,000,000"
					: undefined;
			},
			fieldInvalid: washingtonLevelFieldInvalid,
			skipDistributionValidation: true,
			features: ["washington-level"],
			presets: WASHINGTON_PRESET_OPTIONS,
			presetKey: "washingtonPreset",
			applyWashingtonLevelPreset: (form, preset) =>
				applyWashingtonPreset(form, preset, "washington-random-level"),
			sourceBacked: true,
			statusId: "flow-generator-washington-shape",
			defaults: (form) => applyWashingtonPreset(form, "readable"),
		},
	),
	"washington-square-mesh": declareFlowGeneratorFamily(
		[
			parameter("Vertices per side d", 2, 44),
			parameter("Forward degree", 1, 44),
		],
		{
			estimate: estimateWith((a, b) => ({
				nodes: a * a + 2n,
				edges: b * a * (a > 0n ? a - 1n : 0n) - (b * (b - 1n)) / 2n + 2n * a,
			})),
			payload: (form) => ({
				family_id: "washington-square-mesh",
				dimension: form.primary,
				degree: form.secondary,
				maximum_capacity: Number(form.capacityMaximum),
			}),
			fixedConstruction: washingtonSquareMeshFixedCopy,
			validate: (form, a, b) => {
				if (a < 2n || a > 44n)
					return "Washington Square Mesh side length d must be between 2 and 44";
				if (b < 1n || b > a)
					return "Washington Square Mesh degree must be between 1 and d";
				const edges = b * a * (a - 1n) - (b * (b - 1n)) / 2n + 2n * a;
				if (edges > 20_000n)
					return "Washington Square Mesh is limited to 20,000 edges to keep traces practical";
				return canonicalInteger(form.capacityMaximum, 1n, 100_000_000n) ===
					undefined
					? "Washington Square Mesh maximum capacity C must be between 1 and 100,000,000"
					: undefined;
			},
			fieldInvalid: (form, field, a, b) => {
				const edgeLimit =
					b * a * (a > 0n ? a - 1n : 0n) - (b * (b - 1n)) / 2n + 2n * a >
					20_000n;
				if (field === "primary") return bigintOutside(a, 2n, 44n) || edgeLimit;
				if (field === "secondary") return b < 1n || b > a || edgeLimit;
				return (
					field === "capacityMaximum" &&
					canonicalInteger(form.capacityMaximum, 1n, 100_000_000n) === undefined
				);
			},
			skipDistributionValidation: true,
			features: ["washington-square-mesh"],
			presets: WASHINGTON_SQUARE_MESH_PRESET_OPTIONS,
			presetKey: "washingtonSquareMeshPreset",
			sourceBacked: true,
			statusId: "flow-generator-washington-square-mesh-shape",
			defaults: (form) => applyWashingtonSquareMeshPreset(form, "readable"),
		},
	),
	"watts-strogatz-fixed": declareFlowGeneratorFamily(
		[
			parameter("Vertices", 4),
			parameter("Neighbor degree (even)", 2),
			parameter("Rewired edges", 0),
		],
		{
			estimate: estimateWith((a, b) => ({ nodes: a, edges: (a * b) / 2n })),
			payload: (form) => ({
				family_id: "watts-strogatz-fixed",
				nodes: form.primary,
				neighborhood: form.secondary,
				rewire_count: form.tertiary,
			}),
			validate: (_form, a, b, c) =>
				a < 4n
					? "Vertex count must be at least 4"
					: b < 2n || b >= a || b % 2n !== 0n
						? "Neighbor degree must be even and satisfy 2 ≤ degree < n"
						: c > (a * b) / 2n
							? "Rewired-edge count cannot exceed the initial edge count"
							: a * c > 5_000_000n
								? "Deterministic rewiring exceeds the 5,000,000-operation limit"
								: undefined,
			fieldInvalid: (_form, field, a, b, c) =>
				field === "primary"
					? a < 4n
					: field === "secondary"
						? b < 2n || b >= a || b % 2n !== 0n
						: field === "tertiary" && (c > (a * b) / 2n || a * c > 5_000_000n),
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "watts-strogatz-fixed",
				primary: 20,
				secondary: 4,
				tertiary: 8,
			}),
		},
	),
	"zadeh-phase-chain-stress": declareFlowGeneratorFamily(
		[parameter("Group size k (multiple of 4)", 4, 20, 4)],
		{
			estimate: estimateWith((a) => ({
				nodes: 3n * a,
				edges: a >= 4n ? (3n * a * a) / 2n + a - 2n : 0n,
			})),
			payload: (form) => ({
				family_id: "zadeh-phase-chain-stress",
				group_size: form.primary,
			}),
			fixedConstruction: zadehFixedCopy,
			validate: (_form, a) =>
				a >= 4n && a <= 20n && a % 4n === 0n
					? undefined
					: "Group size must be a multiple of 4 between 4 and 20",
			fieldInvalid: (_form, field, a) =>
				field === "primary" && (a < 4n || a > 20n || a % 4n !== 0n),
			skipDistributionValidation: true,
			sourceBacked: true,
			defaults: (form) => ({
				...form,
				family: "zadeh-phase-chain-stress",
				primary: 8,
			}),
		},
	),
} satisfies Record<FlowGeneratorFamilyId, FlowGeneratorFamilyEntryDeclaration>;

function flowGeneratorFamilyEntry(
	family: FlowGeneratorFamilyId,
): FlowGeneratorFamilyEntryDeclaration {
	return FLOW_GENERATOR_FAMILY_ENTRY_DECLARATIONS[family];
}

function createFlowGeneratorFamilyDescriptor(
	id: FlowGeneratorFamilyId,
): FlowGeneratorFamilyDescriptor {
	const declaration = FLOW_GENERATOR_FAMILY_ENTRY_DECLARATIONS[id];
	const { presetKey, statusId, toggleLabel, applyWashingtonLevelPreset } =
		declaration;
	return {
		id,
		features: new Set(declaration.features),
		sourceBacked: declaration.sourceBacked,
		...(statusId === undefined ? {} : { statusId }),
		...(toggleLabel === undefined ? {} : { toggleLabel }),
		...(presetKey === undefined ? {} : { presetKey }),
		...(applyWashingtonLevelPreset === undefined
			? {}
			: { applyWashingtonLevelPreset }),
		parameters: (form) => {
			const fields = [
				"primary",
				"secondary",
				"tertiary",
				"quaternary",
			] as const;
			return declaration.parameters.map((parameter, index) => ({
				field: fields[index] ?? "quaternary",
				label: parameter.label,
				minimum: parameter.minimum ?? 1,
				maximum:
					typeof parameter.maximum === "function"
						? parameter.maximum(form)
						: parameter.maximum,
				step: parameter.step ?? 1,
			}));
		},
		defaults: (form) => {
			const base = {
				...DEFAULT_FLOW_GENERATOR,
				family: id,
				seed: form.seed,
				primary: form.primary,
				capacityKind: form.capacityKind,
				capacityMinimum: form.capacityMinimum,
				capacityMaximum: form.capacityMaximum,
				costKind: form.costKind,
				costMinimum: form.costMinimum,
				costMaximum: form.costMaximum,
			};
			const next = declaration.defaults?.(base) ?? base;
			const primary = declaration.parameters[0];
			const maximum =
				typeof primary?.maximum === "function"
					? primary.maximum(next)
					: primary?.maximum;
			return {
				...next,
				primary: Math.max(
					primary?.minimum ?? 1,
					maximum === undefined
						? next.primary
						: Math.min(maximum, next.primary),
				),
			};
		},
		presets: declaration.presets,
		estimate: declaration.estimate,
		estimateIsUpperBound: declaration.estimateIsUpperBound ?? (() => false),
		validation: validateFlowGeneratorForm,
		fieldInvalid: flowGeneratorFieldInvalid,
		encode: encodeFlowGeneratorSpec,
		fixedConstruction: declaration.fixedConstruction ?? (() => undefined),
		customize: (form, key, value) => {
			const next = { ...form, [key]: value };
			if (presetKey === undefined || key === "seed" || key === presetKey) {
				return next;
			}
			return { ...next, [presetKey]: "custom" };
		},
	};
}

export const FLOW_GENERATOR_FAMILY_DESCRIPTORS = new Map(
	FLOW_GENERATOR_FAMILY_IDS.map(
		(family) => [family, createFlowGeneratorFamilyDescriptor(family)] as const,
	),
);

export function flowGeneratorFamilyDescriptor(
	family: FlowGeneratorFamilyId,
): FlowGeneratorFamilyDescriptor {
	const descriptor = FLOW_GENERATOR_FAMILY_DESCRIPTORS.get(family);
	if (descriptor === undefined) {
		throw new Error(`Flow generator family descriptor is missing: ${family}`);
	}
	return descriptor;
}

export {
	ASSIGNMENT_SHAPE_OPTIONS,
	applyAssignmentShape,
	applyTransportationShape,
	assignmentCostLabels,
	assignmentParameterLabels,
	canonicalInteger,
	capacityFieldLabels,
	costFieldLabels,
	GRIDGRAPH_PRESET_OPTIONS,
	NETGEN_PRESET_OPTIONS,
	nonnegativeBigInt,
	TRANSPORTATION_SHAPE_OPTIONS,
	transportationCostLabels,
	transportationParameterLabel,
	WASHINGTON_MATCHING_PRESET_OPTIONS,
	WASHINGTON_PRESET_OPTIONS,
	WASHINGTON_SQUARE_MESH_PRESET_OPTIONS,
};
