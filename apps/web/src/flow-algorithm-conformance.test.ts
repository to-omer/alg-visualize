import { describe, expect, it } from "vitest";
import type { FlowAlgorithmCatalogEntry } from "./flow-algorithm-catalog";
import {
	decodeFlowAlgorithmConformanceContracts,
	FLOW_ALGORITHM_CONFORMANCE_REVISION,
} from "./flow-algorithm-conformance";

const descriptor: FlowAlgorithmCatalogEntry = {
	id: "edmonds-karp",
	title: "Edmonds–Karp",
	aliases: ["Edmonds-Karp"],
	search_terms: [],
	kind: "variant",
	family: "augmenting-path",
	trace_steps: {
		phase_unit: "one residual-path search phase",
		phase_availability: { availability: "available" },
		operation_unit: "one completed augmentation",
		operation_availability: { availability: "available" },
		detail: { availability: "available", unit: "one residual-arc inspection" },
		primary_work: {
			metric_ordinal: 2,
			unit: "residual-arc inspections",
			abstraction: "primitive",
			visualization: "edge-field",
		},
	},
	problems: ["max-flow"],
	models: ["max-flow"],
	runtime_route: "max-flow",
	graph_requirements: [],
	initial_construction: "zero-feasible",
	initial_optimality: "none",
	initial_oracle_dependency: "none",
	negative_cycle_policy: "not-applicable",
	terminal_oracle_dependency: "none",
	exact: true,
	randomized: false,
	complexity: "O(n m^2)",
	source_id: "edmonds-karp-1972",
	initial_band: { max_nodes: 2_000, max_edges: 20_000 },
	admission_contract: {
		min_nodes: null,
		min_edges: null,
		max_nodes: null,
		max_edges: null,
		max_capacity: null,
		max_absolute_cost: null,
		max_assignment_space: null,
		max_capacity_state_space: null,
		strict_interior_required: false,
		min_dynamic_capacity_updates: null,
		max_dynamic_capacity_updates: null,
		capacity_updates_only: false,
	},
	status: "executable",
	implementation_scope: "source-complete",
};

function contract() {
	return {
		schema_revision: FLOW_ALGORITHM_CONFORMANCE_REVISION,
		algorithm_id: descriptor.id,
		algorithm_anchor: descriptor.title,
		kind: descriptor.kind,
		status: descriptor.status,
		implementation_scope: descriptor.implementation_scope,
		runtime_route: descriptor.runtime_route,
		models: descriptor.models,
		graph_requirements: descriptor.graph_requirements,
		initial_construction: descriptor.initial_construction,
		initial_optimality: descriptor.initial_optimality,
		initial_oracle_dependency: descriptor.initial_oracle_dependency,
		negative_cycle_policy: descriptor.negative_cycle_policy,
		terminal_oracle_dependency: descriptor.terminal_oracle_dependency,
		exact: descriptor.exact,
		randomized: descriptor.randomized,
		complexity: descriptor.complexity,
		initial_band: descriptor.initial_band,
		checker_contract_kind: "independent-max-flow-certificate",
		numeric_safety_contract_kind: "aggregate-safe-wide-arithmetic",
		work_limit_contract: {
			source_termination_argument: true,
			checked_runtime_work_ceiling: false,
			catalog_admission_ceiling: true,
		},
		compatible_generator_fixture_ids: ["path"],
		source: {
			source_id: descriptor.source_id,
			kind: "primary-paper",
			fixed_source: "fixed citation",
			catalog_scope_and_claims: "Algorithm B and its invariant",
			implementation_note: "independent implementation",
			reviewed: "2026-07-19",
		},
	};
}

describe("flow algorithm conformance manifest", () => {
	it("requires total catalog-ordered descriptor contracts", () => {
		expect(
			decodeFlowAlgorithmConformanceContracts(JSON.stringify([contract()]), [
				descriptor,
			]),
		).toHaveLength(1);
		expect(() =>
			decodeFlowAlgorithmConformanceContracts("[]", [descriptor]),
		).toThrow(/complete catalog/);
	});

	it("rejects contract drift, source drift, and unknown fields", () => {
		expect(() =>
			decodeFlowAlgorithmConformanceContracts(
				JSON.stringify([
					{ ...contract(), numeric_safety_contract_kind: "unchecked" },
				]),
				[descriptor],
			),
		).toThrow(/disagrees with the catalog/);
		expect(() =>
			decodeFlowAlgorithmConformanceContracts(
				JSON.stringify([
					{
						...contract(),
						work_limit_contract: { catalog_admission_ceiling: false },
					},
				]),
				[descriptor],
			),
		).toThrow(/disagrees with the catalog/);
		expect(() =>
			decodeFlowAlgorithmConformanceContracts(
				JSON.stringify([
					{
						...contract(),
						source: { ...contract().source, source_id: "wrong-source" },
					},
				]),
				[descriptor],
			),
		).toThrow(/source or fixture/);
		expect(() =>
			decodeFlowAlgorithmConformanceContracts(
				JSON.stringify([{ ...contract(), future: true }]),
				[descriptor],
			),
		).toThrow(/invalid shape/);
	});
});
