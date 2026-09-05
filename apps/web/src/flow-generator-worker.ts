import init, {
	canonical_flow_scenario_json,
	generate_flow_graph_json,
} from "../../../packages/wasm/visualizer_engine.js";
import { buildGeneratedFlowScenario } from "./flow-generator-scenario";
import {
	FLOW_GENERATOR_REQUEST_ERROR,
	FLOW_GENERATOR_SIZE_ERROR,
	type FlowGeneratedStats,
	type FlowGeneratorWorkerResponse,
	flowGeneratorRequestFitsBudget,
	flowGeneratorRequestJobId,
	isFlowGeneratorWorkerRequest,
	MAX_FLOW_GENERATOR_TRANSFER_BYTES,
} from "./flow-generator-worker-protocol";

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, keys: string[]): boolean {
	const actual = Object.keys(value).sort();
	const expected = [...keys].sort();
	return (
		actual.length === expected.length &&
		actual.every((key, index) => key === expected[index])
	);
}

function decodeStats(value: unknown): FlowGeneratedStats {
	if (
		!isRecord(value) ||
		!hasExactKeys(value, [
			"node_count",
			"edge_count",
			"minimum_capacity",
			"maximum_capacity",
			"minimum_cost",
			"maximum_cost",
		]) ||
		!Number.isSafeInteger(value.node_count) ||
		(value.node_count as number) < 0 ||
		!Number.isSafeInteger(value.edge_count) ||
		(value.edge_count as number) < 0 ||
		![
			value.minimum_capacity,
			value.maximum_capacity,
			value.minimum_cost,
			value.maximum_cost,
		].every((item) => typeof item === "string")
	) {
		throw new Error("Generated flow statistics are invalid");
	}
	return value as FlowGeneratedStats;
}

function decodeCandidate(source: string): {
	graph: Record<string, unknown>;
	suggestedModel: Record<string, unknown>;
	provenance: Record<string, unknown>;
	stats: FlowGeneratedStats;
} {
	const value: unknown = JSON.parse(source);
	if (
		!isRecord(value) ||
		!hasExactKeys(value, ["graph", "suggested_model", "provenance", "stats"]) ||
		!isRecord(value.graph) ||
		!isRecord(value.suggested_model) ||
		!isRecord(value.provenance)
	) {
		throw new Error("Generated flow candidate has an invalid shape");
	}
	return {
		graph: value.graph,
		suggestedModel: value.suggested_model,
		provenance: value.provenance,
		stats: decodeStats(value.stats),
	};
}

function post(response: FlowGeneratorWorkerResponse): void {
	self.postMessage(response);
}

function phase(
	jobId: number,
	stage: "initializing" | "materializing" | "validating",
	completedPhases: 0 | 1 | 2,
): void {
	post({ kind: "progress", jobId, stage, completedPhases, totalPhases: 3 });
}

function yieldToHost(): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, 0));
}

self.addEventListener("message", (event: MessageEvent<unknown>) => {
	const request = event.data;
	if (!isFlowGeneratorWorkerRequest(request)) {
		const jobId = flowGeneratorRequestJobId(request);
		if (jobId !== undefined) {
			post({ kind: "error", jobId, message: FLOW_GENERATOR_REQUEST_ERROR });
		}
		return;
	}
	if (
		!flowGeneratorRequestFitsBudget(request, MAX_FLOW_GENERATOR_TRANSFER_BYTES)
	) {
		post({
			kind: "error",
			jobId: request.jobId,
			message: FLOW_GENERATOR_SIZE_ERROR,
		});
		return;
	}
	void (async () => {
		try {
			phase(request.jobId, "initializing", 0);
			await init();
			await yieldToHost();
			phase(request.jobId, "materializing", 1);
			await yieldToHost();
			const candidate = decodeCandidate(generate_flow_graph_json(request.spec));
			phase(request.jobId, "validating", 2);
			await yieldToHost();
			const scenario = buildGeneratedFlowScenario(
				JSON.parse(request.scenario),
				candidate,
				{
					recommendedRunProfile: request.recommendedRunProfile,
					...(request.recommendedAlgorithmId === undefined
						? {}
						: {
								recommendedAlgorithmId: request.recommendedAlgorithmId,
							}),
				},
			);
			const canonical = canonical_flow_scenario_json(JSON.stringify(scenario));
			post({
				kind: "complete",
				jobId: request.jobId,
				scenario: JSON.stringify(JSON.parse(canonical), null, 2),
				stats: candidate.stats,
			});
		} catch (error) {
			post({
				kind: "error",
				jobId: request.jobId,
				message: error instanceof Error ? error.message : String(error),
			});
		}
	})();
});
