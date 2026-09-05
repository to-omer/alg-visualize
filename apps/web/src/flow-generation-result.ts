import type { FlowGeneratedStats } from "./flow-generator-worker-protocol";

export type FlowGenerationResultSummary = Readonly<{
	algorithmId: string;
	difficulty: string;
	digest: string;
	familyId: string;
	modelKind: string;
	seed: string;
	stats: FlowGeneratedStats;
}>;

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function canonicalUnsigned(value: unknown): value is string {
	return typeof value === "string" && /^(0|[1-9][0-9]*)$/u.test(value);
}

function canonicalInteger(value: unknown): value is string {
	return typeof value === "string" && /^(0|-?[1-9][0-9]*)$/u.test(value);
}

function validStats(stats: FlowGeneratedStats): boolean {
	return (
		Number.isSafeInteger(stats.node_count) &&
		stats.node_count >= 0 &&
		Number.isSafeInteger(stats.edge_count) &&
		stats.edge_count >= 0 &&
		canonicalUnsigned(stats.minimum_capacity) &&
		canonicalUnsigned(stats.maximum_capacity) &&
		canonicalInteger(stats.minimum_cost) &&
		canonicalInteger(stats.maximum_cost) &&
		BigInt(stats.minimum_capacity) <= BigInt(stats.maximum_capacity) &&
		BigInt(stats.minimum_cost) <= BigInt(stats.maximum_cost)
	);
}

const GENERATED_MODEL_KINDS = new Set([
	"assignment",
	"bipartite-matching",
	"circulation",
	"fixed-flow-min-cost",
	"max-flow",
	"min-cost-max-flow",
	"planar-max-flow",
	"transportation",
	"transshipment",
]);

function graphMatchesStats(graph: unknown, stats: FlowGeneratedStats): boolean {
	if (
		!isRecord(graph) ||
		!Array.isArray(graph.nodes) ||
		!Array.isArray(graph.edges)
	) {
		return false;
	}
	if (
		graph.nodes.length !== stats.node_count ||
		graph.edges.length !== stats.edge_count
	) {
		return false;
	}
	const capacities: bigint[] = [];
	const costs: bigint[] = [];
	for (const edge of graph.edges) {
		if (
			!isRecord(edge) ||
			!canonicalUnsigned(edge.capacity) ||
			!canonicalInteger(edge.cost)
		) {
			return false;
		}
		capacities.push(BigInt(edge.capacity));
		costs.push(BigInt(edge.cost));
	}
	const minimumCapacity =
		capacities.length === 0
			? 0n
			: capacities.reduce((minimum, value) =>
					value < minimum ? value : minimum,
				);
	const maximumCapacity =
		capacities.length === 0
			? 0n
			: capacities.reduce((maximum, value) =>
					value > maximum ? value : maximum,
				);
	const minimumCost =
		costs.length === 0
			? 0n
			: costs.reduce((minimum, value) => (value < minimum ? value : minimum));
	const maximumCost =
		costs.length === 0
			? 0n
			: costs.reduce((maximum, value) => (value > maximum ? value : maximum));
	return (
		minimumCapacity.toString() === stats.minimum_capacity &&
		maximumCapacity.toString() === stats.maximum_capacity &&
		minimumCost.toString() === stats.minimum_cost &&
		maximumCost.toString() === stats.maximum_cost
	);
}

/**
 * Extracts only reviewed provenance fields from the Worker-validated Scenario.
 * A malformed candidate fails closed rather than producing a reassuring summary.
 */
export function buildFlowGenerationResultSummary(
	scenario: string,
	stats: FlowGeneratedStats,
): FlowGenerationResultSummary {
	let decoded: unknown;
	try {
		decoded = JSON.parse(scenario);
	} catch {
		throw new Error("Generated Flow Scenario is not valid JSON");
	}
	if (!isRecord(decoded) || !isRecord(decoded.payload) || !validStats(stats)) {
		throw new Error("Generated Flow summary is invalid");
	}
	const payload = decoded.payload;
	const provenance = payload.generator_provenance;
	const algorithm = payload.algorithm;
	const model = payload.model;
	if (
		!isRecord(provenance) ||
		!isRecord(algorithm) ||
		!isRecord(model) ||
		typeof provenance.family_id !== "string" ||
		!/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(provenance.family_id) ||
		!canonicalUnsigned(provenance.seed) ||
		typeof provenance.materialized_sha256 !== "string" ||
		!/^[0-9a-f]{64}$/u.test(provenance.materialized_sha256) ||
		typeof provenance.difficulty !== "string" ||
		!new Set(["ordinary", "stress", "verified-worst-case"]).has(
			provenance.difficulty,
		) ||
		typeof algorithm.id !== "string" ||
		!/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(algorithm.id) ||
		typeof model.kind !== "string" ||
		!GENERATED_MODEL_KINDS.has(model.kind)
	) {
		throw new Error("Generated Flow provenance is invalid");
	}
	if (!graphMatchesStats(payload.graph, stats)) {
		throw new Error(
			"Generated Flow statistics do not match the Scenario graph",
		);
	}
	return {
		algorithmId: algorithm.id,
		difficulty: provenance.difficulty,
		digest: provenance.materialized_sha256,
		familyId: provenance.family_id,
		modelKind: model.kind,
		seed: provenance.seed,
		stats,
	};
}
