import { analyzeFlowGraphShape, type FlowGraphShape } from "./flow-graph-shape";
import {
	decodeFlowProblemModelV1,
	type FlowEdgeV1,
	type FlowNodeV1,
	validatePlanarScene,
} from "./flow-scene";

export type FlowGraphRequirement =
	| "no-self-loops"
	| "zero-flow-feasible"
	| "positive-capacity"
	| "non-empty-edges"
	| "zero-cost"
	| "distinct-terminals"
	| "underlying-connected"
	| "unit-capacity"
	| "unit-network"
	| "bipartite"
	| "balanced-bipartite"
	| "transportation-network"
	| "planar-embedding"
	| "strongly-connected"
	| "nonbinding-transshipment-capacities";

export type FlowInitialConstruction =
	| "zero-feasible"
	| "zero-pseudoflow-with-imbalance"
	| "any-feasible"
	| "dual-feasible"
	| "epsilon-optimal"
	| "source-defined"
	| "project-oracle-constructed";

export type FlowInitialOptimality =
	| "none"
	| "optimal-for-every-partial-value"
	| "dual-feasible"
	| "epsilon-optimal"
	| "source-defined"
	| "project-oracle-constructed";

export type FlowNegativeCyclePolicy =
	| "not-applicable"
	| "require-absent-anywhere"
	| "resolve-internally"
	| "source-defined";

export type FlowInitialOracleDependency =
	| "none"
	| "project-exact-max-flow-scalar-target"
	| "project-exact-min-cost-scalar-optimum"
	| "project-optimum-vector-initial-state"
	| "project-isolation-face-optimum-facts"
	| "project-feasible-face-initial-state-and-scalar-optimum";

export type FlowAlgorithmStepContract = {
	phase_unit: string;
	phase_availability: FlowStepAvailability;
	operation_unit: string;
	operation_availability: FlowStepAvailability;
	detail:
		| { availability: "available"; unit: string }
		| { availability: "unavailable"; reason: string };
	primary_work: {
		metric_ordinal: number;
		unit: string;
		abstraction: "primitive" | "iteration" | "oracle-call";
		visualization: "edge-field" | "candidate-field" | "numeric-field";
	};
};

export type FlowStepAvailability =
	| { availability: "available" }
	| { availability: "unavailable"; reason: string };

export type FlowAlgorithmAdmissionContract = Readonly<{
	min_nodes: number | null;
	min_edges: number | null;
	max_nodes: number | null;
	max_edges: number | null;
	max_capacity: string | null;
	max_absolute_cost: string | null;
	max_assignment_space: string | null;
	max_capacity_state_space: string | null;
	strict_interior_required: boolean;
	min_dynamic_capacity_updates: number | null;
	max_dynamic_capacity_updates: number | null;
	capacity_updates_only: boolean;
}>;

export type FlowAlgorithmCatalogEntry = {
	id: string;
	title: string;
	aliases: string[];
	search_terms: string[];
	kind: "solver" | "variant" | "heuristic" | "primitive";
	family: string;
	trace_steps: FlowAlgorithmStepContract;
	problems: string[];
	models: FlowModelKind[];
	runtime_route:
		| "max-flow"
		| "min-cost-flow"
		| "min-cost-max-flow"
		| "parametric-max-flow"
		| "bipartite-matching"
		| "assignment"
		| "transportation"
		| "planar-max-flow"
		| "convex-cost-flow";
	graph_requirements: FlowGraphRequirement[];
	initial_construction: FlowInitialConstruction;
	initial_optimality: FlowInitialOptimality;
	initial_oracle_dependency: FlowInitialOracleDependency;
	negative_cycle_policy: FlowNegativeCyclePolicy;
	terminal_oracle_dependency: "none" | "project-optimum-vector-final-point";
	exact: boolean;
	randomized: boolean;
	complexity: string;
	source_id: string;
	initial_band: { max_nodes: number; max_edges: number };
	admission_contract: FlowAlgorithmAdmissionContract;
	status: "planned" | "source-blocked" | "executable";
	implementation_scope:
		| "source-complete"
		| "bounded-oracle-guided"
		| "source-component"
		| "project-oracle-demonstrator"
		| "external-completion"
		| "precomputed-optimum-projection";
};

export type FlowModelKind =
	| "max-flow"
	| "parametric-max-flow"
	| "fixed-flow-min-cost"
	| "min-cost-max-flow"
	| "circulation"
	| "transshipment"
	| "convex-cost-flow"
	| "bipartite-matching"
	| "assignment"
	| "transportation"
	| "planar-max-flow";

export type FlowGraphAdmissionFacts = Readonly<{
	maximumCapacity: bigint;
	maximumAbsoluteCost: bigint;
	assignmentSpace: bigint;
	capacityStateSpace: bigint;
	strictInterior: boolean;
}>;

const FLOW_MODEL_KINDS = [
	"max-flow",
	"parametric-max-flow",
	"fixed-flow-min-cost",
	"min-cost-max-flow",
	"circulation",
	"transshipment",
	"convex-cost-flow",
	"bipartite-matching",
	"assignment",
	"transportation",
	"planar-max-flow",
] as const satisfies readonly FlowModelKind[];

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(
	value: Record<string, unknown>,
	expected: string[],
): boolean {
	const actual = Object.keys(value).sort();
	return (
		actual.length === expected.length &&
		[...expected].sort().every((key, index) => actual[index] === key)
	);
}

const ENTRY_KEYS = [
	"admission_contract",
	"aliases",
	"complexity",
	"exact",
	"family",
	"graph_requirements",
	"id",
	"initial_band",
	"initial_construction",
	"initial_optimality",
	"initial_oracle_dependency",
	"implementation_scope",
	"kind",
	"models",
	"negative_cycle_policy",
	"terminal_oracle_dependency",
	"trace_steps",
	"problems",
	"randomized",
	"runtime_route",
	"search_terms",
	"source_id",
	"status",
	"title",
];

function isNonEmptyString(value: unknown): value is string {
	return typeof value === "string" && value.trim().length > 0;
}

function isStepContract(value: unknown): value is FlowAlgorithmStepContract {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"phase_unit",
			"phase_availability",
			"operation_unit",
			"operation_availability",
			"detail",
			"primary_work",
		]) ||
		!isNonEmptyString(value.phase_unit) ||
		!isNonEmptyString(value.operation_unit) ||
		!isStepAvailability(value.phase_availability) ||
		!isStepAvailability(value.operation_availability) ||
		!isRecord(value.detail) ||
		!isRecord(value.primary_work) ||
		!hasExactKeys(value.primary_work, [
			"metric_ordinal",
			"unit",
			"abstraction",
			"visualization",
		]) ||
		!Number.isSafeInteger(value.primary_work.metric_ordinal) ||
		(value.primary_work.metric_ordinal as number) < 0 ||
		(value.primary_work.metric_ordinal as number) >= 16 ||
		!isNonEmptyString(value.primary_work.unit) ||
		!["primitive", "iteration", "oracle-call"].includes(
			value.primary_work.abstraction as string,
		) ||
		!["edge-field", "candidate-field", "numeric-field"].includes(
			value.primary_work.visualization as string,
		)
	) {
		return false;
	}
	if (value.detail.availability === "available") {
		return (
			hasExactKeys(value.detail, ["availability", "unit"]) &&
			isNonEmptyString(value.detail.unit)
		);
	}
	return (
		value.detail.availability === "unavailable" &&
		hasExactKeys(value.detail, ["availability", "reason"]) &&
		isNonEmptyString(value.detail.reason)
	);
}

function isStepAvailability(value: unknown): value is FlowStepAvailability {
	if (!isRecord(value)) return false;
	if (value.availability === "available") {
		return hasExactKeys(value, ["availability"]);
	}
	return (
		value.availability === "unavailable" &&
		hasExactKeys(value, ["availability", "reason"]) &&
		isNonEmptyString(value.reason)
	);
}

const ADMISSION_CONTRACT_KEYS = [
	"capacity_updates_only",
	"max_absolute_cost",
	"max_assignment_space",
	"max_capacity",
	"max_capacity_state_space",
	"max_dynamic_capacity_updates",
	"max_edges",
	"max_nodes",
	"min_dynamic_capacity_updates",
	"min_edges",
	"min_nodes",
	"strict_interior_required",
];

function isNullablePositiveSafeInteger(value: unknown): boolean {
	return (
		value === null || (Number.isSafeInteger(value) && (value as number) > 0)
	);
}

function isNullableCanonicalPositiveInteger(value: unknown): boolean {
	return (
		value === null ||
		(typeof value === "string" && /^[1-9][0-9]*$/u.test(value))
	);
}

function isAdmissionContract(
	value: unknown,
): value is FlowAlgorithmAdmissionContract {
	if (!isRecord(value) || !hasExactKeys(value, ADMISSION_CONTRACT_KEYS)) {
		return false;
	}
	const minimumUpdates = value.min_dynamic_capacity_updates;
	const maximumUpdates = value.max_dynamic_capacity_updates;
	if (
		!isNullablePositiveSafeInteger(value.min_nodes) ||
		!isNullablePositiveSafeInteger(value.min_edges) ||
		!isNullablePositiveSafeInteger(value.max_nodes) ||
		!isNullablePositiveSafeInteger(value.max_edges) ||
		!isNullableCanonicalPositiveInteger(value.max_capacity) ||
		!isNullableCanonicalPositiveInteger(value.max_absolute_cost) ||
		!isNullableCanonicalPositiveInteger(value.max_assignment_space) ||
		!isNullableCanonicalPositiveInteger(value.max_capacity_state_space) ||
		typeof value.strict_interior_required !== "boolean" ||
		!isNullablePositiveSafeInteger(minimumUpdates) ||
		!isNullablePositiveSafeInteger(maximumUpdates) ||
		typeof value.capacity_updates_only !== "boolean"
	) {
		return false;
	}
	if (
		minimumUpdates !== null &&
		maximumUpdates !== null &&
		(minimumUpdates as number) > (maximumUpdates as number)
	) {
		return false;
	}
	return !value.capacity_updates_only || minimumUpdates !== null;
}

function decodeEntry(value: unknown): FlowAlgorithmCatalogEntry {
	if (!isRecord(value) || !hasExactKeys(value, ENTRY_KEYS)) {
		throw new Error("Flow algorithm catalog entry has an invalid shape");
	}
	if (
		typeof value.id !== "string" ||
		!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value.id) ||
		typeof value.title !== "string" ||
		!Array.isArray(value.aliases) ||
		!value.aliases.every((alias) => typeof alias === "string") ||
		!Array.isArray(value.search_terms) ||
		!value.search_terms.every((term) => typeof term === "string") ||
		!["solver", "variant", "heuristic", "primitive"].includes(
			value.kind as string,
		) ||
		typeof value.family !== "string" ||
		!Array.isArray(value.problems) ||
		value.problems.length === 0 ||
		!value.problems.every((problem) => typeof problem === "string") ||
		!Array.isArray(value.models) ||
		value.models.length === 0 ||
		!value.models.every((model) =>
			FLOW_MODEL_KINDS.includes(model as FlowModelKind),
		) ||
		!isStepContract(value.trace_steps) ||
		![
			"max-flow",
			"min-cost-flow",
			"min-cost-max-flow",
			"parametric-max-flow",
			"bipartite-matching",
			"assignment",
			"transportation",
			"planar-max-flow",
			"convex-cost-flow",
		].includes(value.runtime_route as string) ||
		!Array.isArray(value.graph_requirements) ||
		!value.graph_requirements.every(
			(requirement) =>
				typeof requirement === "string" &&
				[
					"no-self-loops",
					"zero-flow-feasible",
					"positive-capacity",
					"non-empty-edges",
					"zero-cost",
					"distinct-terminals",
					"underlying-connected",
					"unit-capacity",
					"unit-network",
					"bipartite",
					"balanced-bipartite",
					"transportation-network",
					"planar-embedding",
					"strongly-connected",
					"nonbinding-transshipment-capacities",
				].includes(requirement),
		) ||
		![
			"zero-feasible",
			"zero-pseudoflow-with-imbalance",
			"any-feasible",
			"dual-feasible",
			"epsilon-optimal",
			"source-defined",
			"project-oracle-constructed",
		].includes(value.initial_construction as string) ||
		![
			"none",
			"optimal-for-every-partial-value",
			"dual-feasible",
			"epsilon-optimal",
			"source-defined",
			"project-oracle-constructed",
		].includes(value.initial_optimality as string) ||
		![
			"none",
			"project-exact-max-flow-scalar-target",
			"project-exact-min-cost-scalar-optimum",
			"project-optimum-vector-initial-state",
			"project-isolation-face-optimum-facts",
			"project-feasible-face-initial-state-and-scalar-optimum",
		].includes(value.initial_oracle_dependency as string) ||
		![
			"not-applicable",
			"require-absent-anywhere",
			"resolve-internally",
			"source-defined",
		].includes(value.negative_cycle_policy as string) ||
		!["none", "project-optimum-vector-final-point"].includes(
			value.terminal_oracle_dependency as string,
		) ||
		typeof value.exact !== "boolean" ||
		typeof value.randomized !== "boolean" ||
		typeof value.complexity !== "string" ||
		typeof value.source_id !== "string" ||
		!["planned", "source-blocked", "executable"].includes(
			value.status as string,
		) ||
		![
			"source-complete",
			"bounded-oracle-guided",
			"source-component",
			"project-oracle-demonstrator",
			"external-completion",
			"precomputed-optimum-projection",
		].includes(value.implementation_scope as string) ||
		!isRecord(value.initial_band) ||
		!hasExactKeys(value.initial_band, ["max_nodes", "max_edges"]) ||
		!Number.isSafeInteger(value.initial_band.max_nodes) ||
		(value.initial_band.max_nodes as number) <= 0 ||
		!Number.isSafeInteger(value.initial_band.max_edges) ||
		(value.initial_band.max_edges as number) <= 0 ||
		!isAdmissionContract(value.admission_contract)
	) {
		throw new Error("Flow algorithm catalog entry contains an invalid value");
	}
	return value as FlowAlgorithmCatalogEntry;
}

export function decodeFlowAlgorithmCatalog(
	source: string,
): FlowAlgorithmCatalogEntry[] {
	const value: unknown = JSON.parse(source);
	if (!Array.isArray(value) || value.length === 0) {
		throw new Error("Flow algorithm catalog must be a non-empty array");
	}
	const entries = value.map(decodeEntry);
	const ids = new Set(entries.map((entry) => entry.id));
	if (ids.size !== entries.length) {
		throw new Error("Flow algorithm catalog contains duplicate IDs");
	}
	return entries;
}

export function modelProblemKind(
	modelKind: FlowModelKind,
):
	| "max-flow"
	| "min-cost-flow"
	| "bipartite-matching"
	| "assignment"
	| "transportation"
	| "planar-max-flow"
	| "parametric-max-flow"
	| "convex-cost-flow" {
	switch (modelKind) {
		case "max-flow":
			return "max-flow";
		case "parametric-max-flow":
			return "parametric-max-flow";
		case "fixed-flow-min-cost":
		case "min-cost-max-flow":
		case "circulation":
		case "transshipment":
			return "min-cost-flow";
		case "convex-cost-flow":
			return "convex-cost-flow";
		case "bipartite-matching":
			return "bipartite-matching";
		case "assignment":
			return "assignment";
		case "transportation":
			return "transportation";
		case "planar-max-flow":
			return "planar-max-flow";
	}
}

export function isFlowAlgorithmCompatible(
	entry: FlowAlgorithmCatalogEntry,
	modelKind: FlowModelKind,
): boolean {
	return entry.models.includes(modelKind);
}

export type FlowGraphRequirementStatus =
	| "satisfied"
	| "unsatisfied"
	| "unverifiable";

export function flowGraphRequirementStatus(
	requirement: FlowGraphRequirement,
	shape: FlowGraphShape | undefined,
): FlowGraphRequirementStatus {
	if (shape === undefined) return "unverifiable";
	switch (requirement) {
		case "no-self-loops":
			return shape.noSelfLoops ? "satisfied" : "unsatisfied";
		case "zero-flow-feasible":
			return shape.zeroFlowFeasible ? "satisfied" : "unsatisfied";
		case "positive-capacity":
			return shape.positiveCapacity ? "satisfied" : "unsatisfied";
		case "non-empty-edges":
			return shape.nonEmptyEdges ? "satisfied" : "unsatisfied";
		case "zero-cost":
			return shape.zeroCost ? "satisfied" : "unsatisfied";
		case "distinct-terminals":
			return shape.distinctTerminals ? "satisfied" : "unsatisfied";
		case "underlying-connected":
			return shape.underlyingConnected ? "satisfied" : "unsatisfied";
		case "unit-capacity":
			return shape.unitCapacity ? "satisfied" : "unsatisfied";
		case "unit-network":
			return shape.unitNetwork ? "satisfied" : "unsatisfied";
		case "bipartite":
			return shape.bipartite ? "satisfied" : "unsatisfied";
		case "balanced-bipartite":
			return shape.balancedBipartite ? "satisfied" : "unsatisfied";
		case "transportation-network":
			return shape.transportationNetwork ? "satisfied" : "unsatisfied";
		case "planar-embedding":
			return shape.planarEmbedding === "unavailable"
				? "unverifiable"
				: "satisfied";
		case "strongly-connected":
			return shape.stronglyConnected ? "satisfied" : "unsatisfied";
		case "nonbinding-transshipment-capacities":
			return shape.nonbindingTransshipmentCapacities
				? "satisfied"
				: "unsatisfied";
	}
}

export function flowAlgorithmShapeReport(
	entry: FlowAlgorithmCatalogEntry,
	shape: FlowGraphShape | undefined,
): ReadonlyArray<
	Readonly<{
		requirement: FlowGraphRequirement;
		status: FlowGraphRequirementStatus;
	}>
> {
	return entry.graph_requirements.map((requirement) => ({
		requirement,
		status: flowGraphRequirementStatus(requirement, shape),
	}));
}

export type FlowAlgorithmSelectionReason =
	| "ready"
	| "invalid-model"
	| "incompatible"
	| "planned"
	| "source-blocked"
	| "node-limit"
	| "edge-limit"
	| "no-self-loops-required"
	| "zero-flow-feasible-required"
	| "positive-capacity-required"
	| "non-empty-edges-required"
	| "zero-cost-required"
	| "distinct-terminals-required"
	| "underlying-connected-required"
	| "unit-capacity-required"
	| "unit-network-required"
	| "bipartite-required"
	| "balanced-bipartite-required"
	| "transportation-network-required"
	| "planar-embedding-required"
	| "strongly-connected-required"
	| "nonbinding-transshipment-capacities-required"
	| "negative-residual-cycle-absent-required"
	| "capacity-updates-required"
	| "capacity-updates-only-required"
	| "dynamic-update-limit"
	| "kernel-node-minimum"
	| "kernel-edge-minimum"
	| "kernel-node-limit"
	| "kernel-edge-limit"
	| "kernel-capacity-limit"
	| "kernel-cost-limit"
	| "kernel-state-space-limit"
	| "strict-interior-required"
	| "graph-shape-unverifiable";

export function flowAlgorithmSelectionReason(
	entry: FlowAlgorithmCatalogEntry,
	modelKind: FlowModelKind | undefined,
	nodeCount?: number,
	edgeCount?: number,
	graphShape?: FlowGraphShape,
	dynamicUpdates?: Readonly<{ count: number; capacityOnly: boolean }>,
	admissionFacts?: FlowGraphAdmissionFacts,
): FlowAlgorithmSelectionReason {
	if (modelKind === undefined) return "invalid-model";
	if (!isFlowAlgorithmCompatible(entry, modelKind)) return "incompatible";
	if (entry.status !== "executable") return entry.status;
	const admission = entry.admission_contract;
	if (admission.min_dynamic_capacity_updates !== null) {
		if (
			dynamicUpdates === undefined ||
			dynamicUpdates.count < admission.min_dynamic_capacity_updates
		) {
			return "capacity-updates-required";
		}
	}
	if (
		admission.capacity_updates_only &&
		dynamicUpdates !== undefined &&
		!dynamicUpdates.capacityOnly
	) {
		return "capacity-updates-only-required";
	}
	if (
		admission.max_dynamic_capacity_updates !== null &&
		dynamicUpdates !== undefined &&
		dynamicUpdates.count > admission.max_dynamic_capacity_updates
	) {
		return "dynamic-update-limit";
	}
	if (nodeCount !== undefined && nodeCount > entry.initial_band.max_nodes) {
		return "node-limit";
	}
	if (edgeCount !== undefined && edgeCount > entry.initial_band.max_edges) {
		return "edge-limit";
	}
	if (entry.negative_cycle_policy === "require-absent-anywhere") {
		if (
			graphShape === undefined ||
			graphShape.lowerBoundResidualNegativeCycle === "unavailable"
		) {
			return "graph-shape-unverifiable";
		}
		if (graphShape.lowerBoundResidualNegativeCycle === "present") {
			return "negative-residual-cycle-absent-required";
		}
	}
	for (const { requirement, status } of flowAlgorithmShapeReport(
		entry,
		graphShape,
	)) {
		if (status === "unverifiable") return "graph-shape-unverifiable";
		if (status === "satisfied") continue;
		switch (requirement) {
			case "no-self-loops":
				return "no-self-loops-required";
			case "zero-flow-feasible":
				return "zero-flow-feasible-required";
			case "positive-capacity":
				return "positive-capacity-required";
			case "non-empty-edges":
				return "non-empty-edges-required";
			case "zero-cost":
				return "zero-cost-required";
			case "distinct-terminals":
				return "distinct-terminals-required";
			case "underlying-connected":
				return "underlying-connected-required";
			case "unit-capacity":
				return "unit-capacity-required";
			case "unit-network":
				return "unit-network-required";
			case "bipartite":
				return "bipartite-required";
			case "balanced-bipartite":
				return "balanced-bipartite-required";
			case "transportation-network":
				return "transportation-network-required";
			case "planar-embedding":
				return "planar-embedding-required";
			case "strongly-connected":
				return "strongly-connected-required";
			case "nonbinding-transshipment-capacities":
				return "nonbinding-transshipment-capacities-required";
		}
	}
	if (
		nodeCount !== undefined &&
		edgeCount !== undefined &&
		admissionFacts !== undefined
	) {
		const kernelReason = flowAlgorithmKernelAdmissionReason(
			entry.admission_contract,
			nodeCount,
			edgeCount,
			admissionFacts,
		);
		if (kernelReason !== "ready") return kernelReason;
	}
	return "ready";
}

export function flowAlgorithmSelectionReasonMessage(
	entry: FlowAlgorithmCatalogEntry,
	reason: FlowAlgorithmSelectionReason,
): string {
	switch (reason) {
		case "ready":
			return "Available for the current graph";
		case "invalid-model":
			return "Model unavailable";
		case "incompatible":
			return "Incompatible model";
		case "planned":
			return "Planned";
		case "source-blocked":
			return "Source pending";
		case "node-limit":
			return "Node limit exceeded";
		case "edge-limit":
			return "Edge limit exceeded";
		case "no-self-loops-required":
			return "Remove self-loops";
		case "zero-flow-feasible-required":
			return "Set all supplies and lower bounds to zero";
		case "positive-capacity-required":
			return "Every edge needs positive capacity";
		case "non-empty-edges-required":
			return "Add at least one edge";
		case "zero-cost-required":
			return "Set every edge cost to zero";
		case "distinct-terminals-required":
			return "Use distinct source and sink nodes";
		case "underlying-connected-required":
			return "Connect the underlying graph";
		case "unit-capacity-required":
			return "Unit capacities required";
		case "unit-network-required":
			return "Unit network required";
		case "bipartite-required":
			return "Bipartite graph required";
		case "balanced-bipartite-required":
			return "Balanced bipartite graph required";
		case "transportation-network-required":
			return "Transportation network required";
		case "planar-embedding-required":
			return "Planar embedding required";
		case "strongly-connected-required":
			return "Positive-capacity edges must be strongly connected";
		case "nonbinding-transshipment-capacities-required":
			return "Each residual capacity range must cover the required flow";
		case "negative-residual-cycle-absent-required":
			return "Remove negative-cost cycles from the lower-bound residual graph";
		case "capacity-updates-required":
			return "Add capacity updates to the JSON input";
		case "capacity-updates-only-required":
			return "Only capacity-changing updates are supported";
		case "dynamic-update-limit":
			return `At most ${entry.admission_contract.max_dynamic_capacity_updates} capacity updates`;
		case "kernel-node-minimum":
			return "This bounded kernel needs more nodes";
		case "kernel-edge-minimum":
			return "This bounded kernel needs at least one edge";
		case "kernel-node-limit":
			return "Bounded-kernel node limit exceeded";
		case "kernel-edge-limit":
			return "Bounded-kernel edge limit exceeded";
		case "kernel-capacity-limit":
			return "Capacity exceeds this bounded kernel";
		case "kernel-cost-limit":
			return "Cost magnitude exceeds this bounded kernel";
		case "kernel-state-space-limit":
			return "Exact bounded state space is too large";
		case "strict-interior-required":
			return "No flow satisfies the required balances strictly inside every edge bound; add cut capacity or reduce the required flow";
		case "graph-shape-unverifiable":
			return "Graph structure cannot be verified";
	}
}

type FlowAlgorithmKernelAdmissionReason =
	| "ready"
	| "kernel-node-minimum"
	| "kernel-edge-minimum"
	| "kernel-node-limit"
	| "kernel-edge-limit"
	| "kernel-capacity-limit"
	| "kernel-cost-limit"
	| "kernel-state-space-limit"
	| "strict-interior-required";

function boundedKernelReason(
	nodeCount: number,
	edgeCount: number,
	facts: FlowGraphAdmissionFacts,
	bounds: FlowAlgorithmAdmissionContract,
): FlowAlgorithmKernelAdmissionReason {
	if (bounds.min_nodes !== null && nodeCount < bounds.min_nodes) {
		return "kernel-node-minimum";
	}
	if (bounds.min_edges !== null && edgeCount < bounds.min_edges) {
		return "kernel-edge-minimum";
	}
	if (bounds.max_nodes !== null && nodeCount > bounds.max_nodes) {
		return "kernel-node-limit";
	}
	if (bounds.max_edges !== null && edgeCount > bounds.max_edges) {
		return "kernel-edge-limit";
	}
	if (
		bounds.max_capacity !== null &&
		facts.maximumCapacity > BigInt(bounds.max_capacity)
	) {
		return "kernel-capacity-limit";
	}
	if (
		bounds.max_absolute_cost !== null &&
		facts.maximumAbsoluteCost > BigInt(bounds.max_absolute_cost)
	) {
		return "kernel-cost-limit";
	}
	if (
		bounds.max_assignment_space !== null &&
		facts.assignmentSpace > BigInt(bounds.max_assignment_space)
	) {
		return "kernel-state-space-limit";
	}
	if (
		bounds.max_capacity_state_space !== null &&
		facts.capacityStateSpace > BigInt(bounds.max_capacity_state_space)
	) {
		return "kernel-state-space-limit";
	}
	if (bounds.strict_interior_required && !facts.strictInterior) {
		return "strict-interior-required";
	}
	return "ready";
}

export function flowAlgorithmKernelAdmissionReason(
	contract: FlowAlgorithmAdmissionContract,
	nodeCount: number,
	edgeCount: number,
	facts: FlowGraphAdmissionFacts,
): FlowAlgorithmKernelAdmissionReason {
	return boundedKernelReason(nodeCount, edgeCount, facts, contract);
}

export function filterFlowAlgorithmCatalog(
	entries: FlowAlgorithmCatalogEntry[],
	query: string,
): FlowAlgorithmCatalogEntry[] {
	const terms = query
		.trim()
		.toLocaleLowerCase()
		.split(/\s+/u)
		.filter((term) => term.length > 0);
	if (terms.length === 0) return entries;
	return entries.filter((entry) => {
		const searchable = [
			entry.id,
			entry.title,
			entry.family,
			entry.kind,
			entry.source_id,
			entry.complexity,
			...entry.aliases,
			...entry.search_terms,
			...entry.graph_requirements,
		]
			.join("\n")
			.toLocaleLowerCase();
		return terms.every((term) => searchable.includes(term));
	});
}

export type FlowAlgorithmCompatibilityFilter =
	| "workspace"
	| "model-compatible"
	| "runnable-now"
	| "all";
export type FlowAlgorithmKindFilter = "all" | FlowAlgorithmCatalogEntry["kind"];
export type FlowAlgorithmRandomnessFilter =
	| "all"
	| "deterministic"
	| "randomized";

export type FlowAlgorithmCatalogFilters = Readonly<{
	compatibility: FlowAlgorithmCompatibilityFilter;
	family: string;
	kind: FlowAlgorithmKindFilter;
	randomness: FlowAlgorithmRandomnessFilter;
}>;

export const DEFAULT_FLOW_ALGORITHM_CATALOG_FILTERS: FlowAlgorithmCatalogFilters =
	Object.freeze({
		compatibility: "workspace",
		family: "all",
		kind: "all",
		randomness: "all",
	});

export type FlowAlgorithmFilterContext = Readonly<{
	workspaceProblem: "max-flow" | "min-cost-flow";
	modelKind: FlowModelKind | undefined;
	nodeCount?: number;
	edgeCount?: number;
	graphShape?: FlowGraphShape;
	dynamicUpdates?: Readonly<{ count: number; capacityOnly: boolean }>;
	admissionFacts?: FlowGraphAdmissionFacts;
}>;

/** Applies the closed catalog facets after the full-text query. */
export function filterFlowAlgorithmCatalogByFacets(
	entries: FlowAlgorithmCatalogEntry[],
	query: string,
	filters: FlowAlgorithmCatalogFilters,
	context: FlowAlgorithmFilterContext,
): FlowAlgorithmCatalogEntry[] {
	return filterFlowAlgorithmCatalog(entries, query).filter((entry) => {
		if (filters.family !== "all" && entry.family !== filters.family) {
			return false;
		}
		if (filters.kind !== "all" && entry.kind !== filters.kind) return false;
		if (filters.randomness === "deterministic" && entry.randomized) {
			return false;
		}
		if (filters.randomness === "randomized" && !entry.randomized) {
			return false;
		}
		if (filters.compatibility === "all") return true;
		if (filters.compatibility === "workspace") {
			const maxFlowModels = new Set<FlowModelKind>([
				"max-flow",
				"parametric-max-flow",
				"planar-max-flow",
				"bipartite-matching",
			]);
			const currentIsMaxFlow = context.workspaceProblem === "max-flow";
			return entry.models.some(
				(model) => maxFlowModels.has(model) === currentIsMaxFlow,
			);
		}
		if (
			context.modelKind === undefined ||
			!isFlowAlgorithmCompatible(entry, context.modelKind)
		) {
			return false;
		}
		if (filters.compatibility === "model-compatible") return true;
		return (
			flowAlgorithmSelectionReason(
				entry,
				context.modelKind,
				context.nodeCount,
				context.edgeCount,
				context.graphShape,
				context.dynamicUpdates,
				context.admissionFacts,
			) === "ready"
		);
	});
}

// Admission limits cross the wire as u64 decimal strings. Once a product is
// larger than every encodable threshold, its exact value cannot affect any
// selection decision; keeping it saturated prevents million-digit BigInts.
export const FLOW_ADMISSION_PRODUCT_SATURATION = 1n << 64n;

function saturatingAdmissionProduct(left: bigint, right: bigint): bigint {
	if (left >= FLOW_ADMISSION_PRODUCT_SATURATION) {
		return FLOW_ADMISSION_PRODUCT_SATURATION;
	}
	const product = left * right;
	return product >= FLOW_ADMISSION_PRODUCT_SATURATION
		? FLOW_ADMISSION_PRODUCT_SATURATION
		: product;
}

export function flowGraphAdmissionFacts(
	graph: Record<string, unknown>,
	model?: Record<string, unknown>,
): FlowGraphAdmissionFacts | undefined {
	if (!Array.isArray(graph.edges)) return undefined;
	let maximumCapacity = 0n;
	let maximumAbsoluteCost = 0n;
	let assignmentSpace = 1n;
	let capacityStateSpace = 1n;
	let everyEdgeHasInterior = true;
	try {
		for (const value of graph.edges) {
			if (
				!isRecord(value) ||
				typeof value.lower !== "string" ||
				typeof value.capacity !== "string" ||
				typeof value.cost !== "string"
			) {
				return undefined;
			}
			const lower = BigInt(value.lower);
			const capacity = BigInt(value.capacity);
			const cost = BigInt(value.cost);
			if (lower < 0n || capacity < lower) return undefined;
			if (capacity > maximumCapacity) maximumCapacity = capacity;
			const absoluteCost = cost < 0n ? -cost : cost;
			if (absoluteCost > maximumAbsoluteCost) {
				maximumAbsoluteCost = absoluteCost;
			}
			assignmentSpace = saturatingAdmissionProduct(
				assignmentSpace,
				capacity - lower + 1n,
			);
			capacityStateSpace = saturatingAdmissionProduct(
				capacityStateSpace,
				capacity + 1n,
			);
			everyEdgeHasInterior &&= lower < capacity;
		}
	} catch {
		return undefined;
	}
	const strictInterior =
		model === undefined
			? everyEdgeHasInterior
			: everyEdgeHasInterior && strictInteriorCutConditionsHold(model, graph);
	return {
		maximumCapacity,
		maximumAbsoluteCost,
		assignmentSpace,
		capacityStateSpace,
		strictInterior,
	};
}

function strictInteriorCutConditionsHold(
	model: Record<string, unknown>,
	graph: Record<string, unknown>,
): boolean {
	if (!Array.isArray(graph.nodes) || !Array.isArray(graph.edges)) return false;
	// The only current strict-interior kernel admits at most six nodes. Avoid an
	// exponential structural probe before that kernel's ordinary size admission
	// rejects larger editor inputs.
	if (graph.nodes.length > 6) return false;
	const indexById = new Map<string, number>();
	const target: bigint[] = [];
	try {
		for (const [index, value] of graph.nodes.entries()) {
			if (
				!isRecord(value) ||
				typeof value.id !== "string" ||
				(value.supply !== undefined && typeof value.supply !== "string") ||
				indexById.has(value.id)
			) {
				return false;
			}
			indexById.set(value.id, index);
			target.push(BigInt(value.supply ?? "0"));
		}
		if (model.kind === "fixed-flow-min-cost") {
			if (
				typeof model.source !== "string" ||
				typeof model.sink !== "string" ||
				typeof model.required_flow !== "string"
			) {
				return false;
			}
			const source = indexById.get(model.source);
			const sink = indexById.get(model.sink);
			if (source === undefined || sink === undefined || source === sink)
				return false;
			const required = BigInt(model.required_flow);
			if (required < 0n) return false;
			target[source] = (target[source] ?? 0n) + required;
			target[sink] = (target[sink] ?? 0n) - required;
		} else if (model.kind !== "circulation" && model.kind !== "transshipment") {
			return false;
		}
		if (target.reduce((total, value) => total + value, 0n) !== 0n) return false;
		const edges = graph.edges.map((value) => {
			if (
				!isRecord(value) ||
				typeof value.from !== "string" ||
				typeof value.to !== "string" ||
				typeof value.lower !== "string" ||
				typeof value.capacity !== "string"
			) {
				throw new Error("invalid edge");
			}
			const from = indexById.get(value.from);
			const to = indexById.get(value.to);
			if (from === undefined || to === undefined)
				throw new Error("unknown endpoint");
			return {
				from,
				to,
				lower: BigInt(value.lower),
				capacity: BigInt(value.capacity),
			};
		});
		const subsetCount = 1 << graph.nodes.length;
		for (let subset = 1; subset < subsetCount - 1; subset += 1) {
			let balance = 0n;
			for (const [node, value] of target.entries()) {
				if ((subset & (1 << node)) !== 0) balance += value;
			}
			let lowerOut = 0n;
			let upperOut = 0n;
			let lowerIn = 0n;
			let upperIn = 0n;
			let crossesCut = false;
			for (const edge of edges) {
				const fromInside = (subset & (1 << edge.from)) !== 0;
				const toInside = (subset & (1 << edge.to)) !== 0;
				if (fromInside === toInside) continue;
				crossesCut = true;
				if (fromInside) {
					lowerOut += edge.lower;
					upperOut += edge.capacity;
				} else {
					lowerIn += edge.lower;
					upperIn += edge.capacity;
				}
			}
			if (!crossesCut) {
				if (balance !== 0n) return false;
				continue;
			}
			if (balance <= lowerOut - upperIn || balance >= upperOut - lowerIn) {
				return false;
			}
		}
		return true;
	} catch {
		return false;
	}
}

function flowGraphShape(
	model: Record<string, unknown>,
	graph: Record<string, unknown>,
): FlowGraphShape | undefined {
	if (!Array.isArray(graph.nodes) || !Array.isArray(graph.edges))
		return undefined;
	// Every current descriptor band is at or below this ceiling. Larger graphs
	// fail size admission first, so structural analysis must not block the editor.
	if (graph.nodes.length > 2_000 || graph.edges.length > 20_000) {
		return undefined;
	}
	const nodes: FlowNodeV1[] = [];
	for (const value of graph.nodes) {
		if (
			!isRecord(value) ||
			typeof value.id !== "string" ||
			typeof value.supply !== "string"
		) {
			return undefined;
		}
		nodes.push({ id: value.id, supply: value.supply });
	}
	const edges: FlowEdgeV1[] = [];
	for (const value of graph.edges) {
		if (
			!isRecord(value) ||
			typeof value.id !== "string" ||
			typeof value.from !== "string" ||
			typeof value.to !== "string" ||
			typeof value.lower !== "string" ||
			typeof value.capacity !== "string" ||
			typeof value.cost !== "string"
		) {
			return undefined;
		}
		edges.push({
			id: value.id,
			from: value.from,
			to: value.to,
			lower: value.lower,
			capacity: value.capacity,
			cost: value.cost,
		});
	}
	let terminals:
		| Readonly<{ source: string; sink: string; requiredFlow?: bigint }>
		| undefined;
	if (
		model.kind === "max-flow" ||
		model.kind === "planar-max-flow" ||
		model.kind === "min-cost-max-flow"
	) {
		if (typeof model.source !== "string" || typeof model.sink !== "string") {
			return undefined;
		}
		terminals = { source: model.source, sink: model.sink };
	} else if (model.kind === "fixed-flow-min-cost") {
		if (
			typeof model.source !== "string" ||
			typeof model.sink !== "string" ||
			typeof model.required_flow !== "string"
		) {
			return undefined;
		}
		try {
			const requiredFlow = BigInt(model.required_flow);
			if (requiredFlow < 0n) return undefined;
			terminals = {
				source: model.source,
				sink: model.sink,
				requiredFlow,
			};
		} catch {
			return undefined;
		}
	}
	const matchingAdapter =
		model.kind === "bipartite-matching" && isRecord(model.flow_adapter)
			? typeof model.flow_adapter.source === "string" &&
				typeof model.flow_adapter.sink === "string"
				? {
						source: model.flow_adapter.source,
						sink: model.flow_adapter.sink,
					}
				: undefined
			: undefined;
	const stringList = (value: unknown): readonly string[] | undefined =>
		Array.isArray(value) && value.every((item) => typeof item === "string")
			? value
			: undefined;
	const transportationPartitions =
		model.kind === "transportation"
			? {
					origins: stringList(model.origins) ?? [],
					destinations: stringList(model.destinations) ?? [],
				}
			: undefined;
	const basicShape = analyzeFlowGraphShape(
		nodes,
		edges,
		terminals ?? matchingAdapter,
		transportationPartitions,
	);
	if (model.kind !== "planar-max-flow") return basicShape;
	try {
		const planarModel = decodeFlowProblemModelV1(model);
		validatePlanarScene(planarModel, nodes, edges);
		return { ...basicShape, planarEmbedding: "verified" };
	} catch {
		return basicShape;
	}
}

export function flowScenarioSelection(source: string):
	| {
			modelKind: FlowModelKind;
			algorithmId: string;
			nodeCount: number;
			edgeCount: number;
			graphShape?: FlowGraphShape;
			admissionFacts?: FlowGraphAdmissionFacts;
			dynamicUpdates: { count: number; capacityOnly: boolean };
	  }
	| undefined {
	try {
		const value: unknown = JSON.parse(source);
		if (!isRecord(value) || !isRecord(value.payload)) return undefined;
		const { model, algorithm, graph, updates } = value.payload;
		if (
			!isRecord(model) ||
			!isRecord(algorithm) ||
			!isRecord(graph) ||
			!Array.isArray(graph.nodes) ||
			!Array.isArray(graph.edges) ||
			![
				"max-flow",
				"parametric-max-flow",
				"fixed-flow-min-cost",
				"min-cost-max-flow",
				"circulation",
				"transshipment",
				"convex-cost-flow",
				"bipartite-matching",
				"assignment",
				"transportation",
				"planar-max-flow",
			].includes(model.kind as string) ||
			typeof algorithm.id !== "string"
		) {
			return undefined;
		}
		const shape = flowGraphShape(model, graph);
		const admissionFacts = flowGraphAdmissionFacts(graph, model);
		const dynamicUpdates = Array.isArray(updates)
			? {
					count: updates.length,
					capacityOnly: updates.every(
						(update) => isRecord(update) && update.kind === "set-capacity",
					),
				}
			: { count: 0, capacityOnly: updates === undefined };
		return {
			modelKind: model.kind as FlowModelKind,
			algorithmId: algorithm.id,
			nodeCount: graph.nodes.length,
			edgeCount: graph.edges.length,
			dynamicUpdates,
			...(shape === undefined ? {} : { graphShape: shape }),
			...(admissionFacts === undefined ? {} : { admissionFacts }),
		};
	} catch {
		return undefined;
	}
}
