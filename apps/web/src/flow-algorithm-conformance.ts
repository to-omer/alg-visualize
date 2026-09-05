import type { FlowAlgorithmCatalogEntry } from "./flow-algorithm-catalog";

export const FLOW_ALGORITHM_CONFORMANCE_REVISION =
	"flow-algorithm-conformance/2" as const;

export type FlowConfirmedSourceRecord = {
	source_id: string;
	kind: string;
	fixed_source: string;
	catalog_scope_and_claims: string;
	implementation_note: string;
	reviewed: string;
};

export type FlowCheckerContractKind =
	| "independent-max-flow-certificate"
	| "independent-min-cost-flow-certificate"
	| "independent-min-cost-max-flow-certificate"
	| "independent-bipartite-matching-certificate"
	| "independent-assignment-certificate"
	| "independent-convex-cost-certificate"
	| "source-defined-invariant"
	| "project-oracle-demonstrator-invariant";

export type FlowNumericSafetyContractKind =
	| "aggregate-safe-wide-arithmetic"
	| "bounded-kernel-checked-arithmetic"
	| "structural-domain-proof";

export type FlowWorkLimitContract = {
	source_termination_argument: boolean;
	checked_runtime_work_ceiling: boolean;
	catalog_admission_ceiling: boolean;
};

export type FlowAlgorithmConformanceContract = {
	schema_revision: typeof FLOW_ALGORITHM_CONFORMANCE_REVISION;
	algorithm_id: string;
	algorithm_anchor: string;
	kind: FlowAlgorithmCatalogEntry["kind"];
	status: FlowAlgorithmCatalogEntry["status"];
	implementation_scope: FlowAlgorithmCatalogEntry["implementation_scope"];
	runtime_route: FlowAlgorithmCatalogEntry["runtime_route"];
	models: FlowAlgorithmCatalogEntry["models"];
	graph_requirements: FlowAlgorithmCatalogEntry["graph_requirements"];
	initial_construction: FlowAlgorithmCatalogEntry["initial_construction"];
	initial_optimality: FlowAlgorithmCatalogEntry["initial_optimality"];
	initial_oracle_dependency: FlowAlgorithmCatalogEntry["initial_oracle_dependency"];
	negative_cycle_policy: FlowAlgorithmCatalogEntry["negative_cycle_policy"];
	terminal_oracle_dependency: FlowAlgorithmCatalogEntry["terminal_oracle_dependency"];
	exact: boolean;
	randomized: boolean;
	complexity: string;
	initial_band: { max_nodes: number; max_edges: number };
	checker_contract_kind: FlowCheckerContractKind;
	numeric_safety_contract_kind: FlowNumericSafetyContractKind;
	work_limit_contract: FlowWorkLimitContract;
	compatible_generator_fixture_ids: string[];
	source: FlowConfirmedSourceRecord;
};

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(
	value: Record<string, unknown>,
	expected: readonly string[],
): boolean {
	const actual = Object.keys(value).sort();
	const sortedExpected = [...expected].sort();
	return (
		actual.length === sortedExpected.length &&
		actual.every((key, index) => key === sortedExpected[index])
	);
}

function canonicalId(value: unknown): value is string {
	return typeof value === "string" && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value);
}

const CONTRACT_KEYS = [
	"algorithm_anchor",
	"algorithm_id",
	"checker_contract_kind",
	"compatible_generator_fixture_ids",
	"complexity",
	"exact",
	"graph_requirements",
	"implementation_scope",
	"initial_band",
	"initial_construction",
	"initial_optimality",
	"initial_oracle_dependency",
	"kind",
	"models",
	"negative_cycle_policy",
	"numeric_safety_contract_kind",
	"randomized",
	"runtime_route",
	"schema_revision",
	"source",
	"status",
	"terminal_oracle_dependency",
	"work_limit_contract",
] as const;

const SOURCE_KEYS = [
	"catalog_scope_and_claims",
	"fixed_source",
	"implementation_note",
	"kind",
	"reviewed",
	"source_id",
] as const;

function sameStrings(
	left: readonly string[],
	right: readonly string[],
): boolean {
	return (
		left.length === right.length &&
		left.every((value, index) => value === right[index])
	);
}

function decodeContract(
	value: unknown,
	descriptor: FlowAlgorithmCatalogEntry,
): FlowAlgorithmConformanceContract {
	if (!isRecord(value) || !hasExactKeys(value, CONTRACT_KEYS)) {
		throw new Error("Flow conformance contract has an invalid shape");
	}
	if (
		value.schema_revision !== FLOW_ALGORITHM_CONFORMANCE_REVISION ||
		value.algorithm_id !== descriptor.id ||
		value.algorithm_anchor !== descriptor.title ||
		value.kind !== descriptor.kind ||
		value.status !== descriptor.status ||
		value.implementation_scope !== descriptor.implementation_scope ||
		value.runtime_route !== descriptor.runtime_route ||
		!Array.isArray(value.models) ||
		!value.models.every((model) => typeof model === "string") ||
		!sameStrings(value.models, descriptor.models) ||
		!Array.isArray(value.graph_requirements) ||
		!value.graph_requirements.every((requirement) =>
			descriptor.graph_requirements.includes(
				requirement as FlowAlgorithmCatalogEntry["graph_requirements"][number],
			),
		) ||
		!sameStrings(
			value.graph_requirements as string[],
			descriptor.graph_requirements,
		) ||
		value.initial_construction !== descriptor.initial_construction ||
		value.initial_optimality !== descriptor.initial_optimality ||
		value.initial_oracle_dependency !== descriptor.initial_oracle_dependency ||
		value.negative_cycle_policy !== descriptor.negative_cycle_policy ||
		value.terminal_oracle_dependency !==
			descriptor.terminal_oracle_dependency ||
		value.exact !== descriptor.exact ||
		value.randomized !== descriptor.randomized ||
		value.complexity !== descriptor.complexity ||
		!isRecord(value.initial_band) ||
		!hasExactKeys(value.initial_band, ["max_edges", "max_nodes"]) ||
		value.initial_band.max_nodes !== descriptor.initial_band.max_nodes ||
		value.initial_band.max_edges !== descriptor.initial_band.max_edges ||
		!CHECKER_CONTRACT_KINDS.includes(
			value.checker_contract_kind as FlowCheckerContractKind,
		) ||
		!NUMERIC_SAFETY_CONTRACT_KINDS.includes(
			value.numeric_safety_contract_kind as FlowNumericSafetyContractKind,
		) ||
		!isRecord(value.work_limit_contract) ||
		!hasExactKeys(value.work_limit_contract, [
			"catalog_admission_ceiling",
			"checked_runtime_work_ceiling",
			"source_termination_argument",
		]) ||
		value.work_limit_contract.source_termination_argument !==
			(descriptor.implementation_scope === "source-complete" ||
				descriptor.implementation_scope === "bounded-oracle-guided") ||
		typeof value.work_limit_contract.checked_runtime_work_ceiling !==
			"boolean" ||
		value.work_limit_contract.catalog_admission_ceiling !== true
	) {
		throw new Error("Flow conformance contract disagrees with the catalog");
	}
	if (
		!Array.isArray(value.compatible_generator_fixture_ids) ||
		!value.compatible_generator_fixture_ids.every(canonicalId) ||
		new Set(value.compatible_generator_fixture_ids).size !==
			value.compatible_generator_fixture_ids.length ||
		!value.compatible_generator_fixture_ids.every(
			(fixture, index, fixtures) => {
				const previous = fixtures[index - 1];
				return index === 0 || (previous !== undefined && previous < fixture);
			},
		) ||
		!isRecord(value.source) ||
		!hasExactKeys(value.source, SOURCE_KEYS) ||
		!canonicalId(value.source.source_id) ||
		value.source.source_id !== descriptor.source_id ||
		typeof value.source.kind !== "string" ||
		typeof value.source.fixed_source !== "string" ||
		value.source.fixed_source.length === 0 ||
		typeof value.source.catalog_scope_and_claims !== "string" ||
		value.source.catalog_scope_and_claims.length === 0 ||
		typeof value.source.implementation_note !== "string" ||
		value.source.implementation_note.length === 0 ||
		typeof value.source.reviewed !== "string" ||
		!/^[0-9]{4}-[0-9]{2}-[0-9]{2}$/.test(value.source.reviewed)
	) {
		throw new Error("Flow conformance source or fixture binding is invalid");
	}
	return value as FlowAlgorithmConformanceContract;
}

const CHECKER_CONTRACT_KINDS = [
	"independent-max-flow-certificate",
	"independent-min-cost-flow-certificate",
	"independent-min-cost-max-flow-certificate",
	"independent-bipartite-matching-certificate",
	"independent-assignment-certificate",
	"independent-convex-cost-certificate",
	"source-defined-invariant",
	"project-oracle-demonstrator-invariant",
] as const satisfies readonly FlowCheckerContractKind[];

const NUMERIC_SAFETY_CONTRACT_KINDS = [
	"aggregate-safe-wide-arithmetic",
	"bounded-kernel-checked-arithmetic",
	"structural-domain-proof",
] as const satisfies readonly FlowNumericSafetyContractKind[];

export function decodeFlowAlgorithmConformanceContracts(
	source: string,
	catalog: readonly FlowAlgorithmCatalogEntry[],
): FlowAlgorithmConformanceContract[] {
	const value: unknown = JSON.parse(source);
	if (!Array.isArray(value) || value.length !== catalog.length) {
		throw new Error(
			"Flow conformance manifest must cover the complete catalog",
		);
	}
	return value.map((contract, index) =>
		decodeContract(contract, catalog[index] as FlowAlgorithmCatalogEntry),
	);
}
