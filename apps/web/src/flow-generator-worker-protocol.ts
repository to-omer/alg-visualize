import { fitsUtf8Budget } from "./utf8-budget";

export const MAX_FLOW_GENERATOR_TRANSFER_BYTES = 64 * 1024 * 1024;
export const FLOW_GENERATOR_SIZE_ERROR =
	"Flow generator request exceeds the 64 MiB UTF-8 limit";
export const FLOW_GENERATOR_REQUEST_ERROR =
	"Flow generator Worker request is invalid or incompatible";

export const FLOW_GENERATOR_RUN_PROFILES = ["trace", "fast"] as const;

export type FlowGeneratorRunProfile =
	(typeof FLOW_GENERATOR_RUN_PROFILES)[number];

export type FlowGeneratedStats = Readonly<{
	node_count: number;
	edge_count: number;
	minimum_capacity: string;
	maximum_capacity: string;
	minimum_cost: string;
	maximum_cost: string;
}>;

export type FlowGenerationStage =
	| "initializing"
	| "materializing"
	| "validating";

export type FlowGeneratorProgress = Readonly<{
	stage: FlowGenerationStage;
	completedPhases: 0 | 1 | 2;
	totalPhases: 3;
}>;

export type FlowGeneratorWorkerRequest = Readonly<{
	kind: "generate";
	jobId: number;
	scenario: string;
	spec: string;
	recommendedRunProfile: FlowGeneratorRunProfile;
	recommendedAlgorithmId?: string;
}>;

export type FlowGeneratorWorkerResponse =
	| Readonly<
			{
				kind: "progress";
				jobId: number;
			} & FlowGeneratorProgress
	  >
	| Readonly<{
			kind: "complete";
			jobId: number;
			scenario: string;
			stats: FlowGeneratedStats;
	  }>
	| Readonly<{
			kind: "error";
			jobId: number;
			message: string;
	  }>;

export function flowGeneratorRequestFitsBudget(
	request: Pick<
		FlowGeneratorWorkerRequest,
		"recommendedAlgorithmId" | "recommendedRunProfile" | "scenario" | "spec"
	>,
	maxBytes: number,
): boolean {
	return fitsUtf8Budget(
		[
			request.scenario,
			request.spec,
			request.recommendedAlgorithmId ?? "",
			request.recommendedRunProfile,
		],
		maxBytes,
	);
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function flowGeneratorRequestJobId(value: unknown): number | undefined {
	if (!isRecord(value) || !Number.isSafeInteger(value.jobId)) return undefined;
	const jobId = value.jobId as number;
	return jobId > 0 ? jobId : undefined;
}

function hasExactKeys(value: Record<string, unknown>, keys: string[]): boolean {
	const actual = Object.keys(value).sort();
	const expected = [...keys].sort();
	return (
		actual.length === expected.length &&
		actual.every((key, index) => key === expected[index])
	);
}

export function isFlowGeneratorWorkerRequest(
	value: unknown,
): value is FlowGeneratorWorkerRequest {
	if (!isRecord(value)) return false;
	const expectedKeys = [
		"jobId",
		"kind",
		"recommendedRunProfile",
		"scenario",
		"spec",
	];
	if (value.recommendedAlgorithmId !== undefined) {
		expectedKeys.push("recommendedAlgorithmId");
	}
	return (
		hasExactKeys(value, expectedKeys) &&
		value.kind === "generate" &&
		Number.isSafeInteger(value.jobId) &&
		(value.jobId as number) > 0 &&
		typeof value.scenario === "string" &&
		typeof value.spec === "string" &&
		(value.recommendedRunProfile === "trace" ||
			value.recommendedRunProfile === "fast") &&
		(value.recommendedAlgorithmId === undefined ||
			(typeof value.recommendedAlgorithmId === "string" &&
				value.recommendedAlgorithmId.length <= 96 &&
				/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value.recommendedAlgorithmId)))
	);
}
