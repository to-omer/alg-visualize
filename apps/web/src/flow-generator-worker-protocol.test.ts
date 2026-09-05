import { describe, expect, it } from "vitest";
import {
	FLOW_GENERATOR_REQUEST_ERROR,
	flowGeneratorRequestFitsBudget,
	flowGeneratorRequestJobId,
	isFlowGeneratorWorkerRequest,
} from "./flow-generator-worker-protocol";

describe("flow generator transfer budget", () => {
	it("counts the combined UTF-8 bytes of the Scenario, spec, and recommendation", () => {
		const request = {
			scenario: "é",
			spec: "😀",
			recommendedAlgorithmId: "dinic",
			recommendedRunProfile: "trace" as const,
		};
		expect(flowGeneratorRequestFitsBudget(request, 16)).toBe(true);
		expect(flowGeneratorRequestFitsBudget(request, 15)).toBe(false);
	});

	it("rejects invalid byte ceilings", () => {
		const request = {
			scenario: "{}",
			spec: "{}",
			recommendedRunProfile: "fast" as const,
		};
		expect(flowGeneratorRequestFitsBudget(request, -1)).toBe(false);
		expect(flowGeneratorRequestFitsBudget(request, Number.NaN)).toBe(false);
	});
});

describe("flow generator Worker request", () => {
	const request = {
		kind: "generate",
		jobId: 7,
		scenario: "{}",
		spec: "{}",
		recommendedRunProfile: "trace",
		recommendedAlgorithmId: "dinic",
	};

	it("accepts the exact closed trace and fast request variants", () => {
		expect(isFlowGeneratorWorkerRequest(request)).toBe(true);
		expect(
			isFlowGeneratorWorkerRequest({
				kind: "generate",
				jobId: 8,
				scenario: "{}",
				spec: "{}",
				recommendedRunProfile: "fast",
			}),
		).toBe(true);
	});

	it("fails closed for unknown, missing, or invalid profile fields", () => {
		const { recommendedRunProfile: _missing, ...missingProfile } = request;
		expect(isFlowGeneratorWorkerRequest({ ...request, unexpected: true })).toBe(
			false,
		);
		expect(isFlowGeneratorWorkerRequest(missingProfile)).toBe(false);
		expect(
			isFlowGeneratorWorkerRequest({
				...request,
				recommendedRunProfile: "result-only",
			}),
		).toBe(false);
	});

	it("recovers only a valid job id for a closed error response", () => {
		expect(flowGeneratorRequestJobId({ jobId: 17, kind: "old-generate" })).toBe(
			17,
		);
		expect(flowGeneratorRequestJobId({ jobId: 0 })).toBeUndefined();
		expect(flowGeneratorRequestJobId({ jobId: 1.5 })).toBeUndefined();
		expect(flowGeneratorRequestJobId(null)).toBeUndefined();
		expect(FLOW_GENERATOR_REQUEST_ERROR).toMatch(/invalid|incompatible/u);
	});
});
