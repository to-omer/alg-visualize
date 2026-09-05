import type { FlowGeneratorRunProfile } from "./flow-generator-worker-protocol";

export type FlowGeneratedCandidate = Readonly<{
	graph: Readonly<Record<string, unknown>>;
	suggestedModel: Readonly<Record<string, unknown>>;
	provenance: Readonly<Record<string, unknown>>;
}>;

type GeneratedScenarioOptions = Readonly<{
	recommendedAlgorithmId?: string;
	recommendedRunProfile: FlowGeneratorRunProfile;
}>;

const NETWORK_SIMPLEX_DEFAULT_FAMILIES = new Set([
	"goldberg-mesh-circulation",
	"gridgraph-grid",
	"goto-torus",
	"netgen-skeleton",
]);

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function selectedAlgorithm(
	payload: Readonly<Record<string, unknown>>,
	candidate: FlowGeneratedCandidate,
	recommendedAlgorithmId: string | undefined,
): unknown {
	if (recommendedAlgorithmId !== undefined) {
		return { id: recommendedAlgorithmId, config: {} };
	}
	if (candidate.provenance.family_id === "vision-segmentation-grid") {
		return { id: "boykov-kolmogorov", config: {} };
	}
	if (
		NETWORK_SIMPLEX_DEFAULT_FAMILIES.has(
			String(candidate.provenance.family_id),
		) &&
		(candidate.suggestedModel.kind === "circulation" ||
			candidate.suggestedModel.kind === "transshipment")
	) {
		return { id: "primal-network-simplex", config: {} };
	}
	switch (candidate.suggestedModel.kind) {
		case "max-flow":
			return { id: "edmonds-karp", config: {} };
		case "fixed-flow-min-cost":
			return { id: "successive-shortest-path", config: {} };
		case "min-cost-max-flow":
			return { id: "successive-shortest-augmenting-path", config: {} };
		case "circulation":
			return { id: "simple-cycle-canceling", config: {} };
		case "transshipment":
			return { id: "cost-scaling", config: {} };
		case "bipartite-matching":
			return { id: "hopcroft-karp", config: {} };
		case "assignment":
			return { id: "hungarian", config: {} };
		case "transportation":
			return { id: "transportation-simplex", config: {} };
		case "planar-max-flow":
			return { id: "hassin-st-planar", config: {} };
		default:
			return payload.algorithm;
	}
}

/**
 * Builds the generated Scenario as a fresh value. Validation precedes every
 * write so a rejected profile or malformed Scenario cannot partially mutate
 * the editor's active Scenario.
 */
export function buildGeneratedFlowScenario(
	scenario: unknown,
	candidate: FlowGeneratedCandidate,
	options: GeneratedScenarioOptions,
): Record<string, unknown> {
	if (
		!isRecord(scenario) ||
		scenario.plugin !== "flow" ||
		!isRecord(scenario.payload)
	) {
		throw new Error("Flow generation requires a flow Scenario");
	}
	if (
		options.recommendedRunProfile !== "trace" &&
		options.recommendedRunProfile !== "fast"
	) {
		throw new Error("Flow generator run profile must be trace or fast");
	}

	const payload = scenario.payload;
	const nextPayload: Record<string, unknown> = {
		...payload,
		graph: candidate.graph,
		model: candidate.suggestedModel,
		algorithm: selectedAlgorithm(
			payload,
			candidate,
			options.recommendedAlgorithmId,
		),
		run_profile: options.recommendedRunProfile,
		generator_provenance: candidate.provenance,
	};
	Reflect.deleteProperty(nextPayload, "updates");
	return { ...scenario, payload: nextPayload };
}
