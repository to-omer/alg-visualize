export type EnginePluginContractV1 = {
	plugin_id: string;
	plugin_ordinal: number;
	result_revision_name: string;
	result_schema_version: number;
	metrics_revision_name: string;
	trace_revision_name: string;
	accepted_frame_revisions: string[];
};

export type EngineContractV1 = {
	contract_schema_version: 1;
	accepted_transport_versions: number[];
	plugins: EnginePluginContractV1[];
};

export const EXPECTED_ENGINE_CONTRACT_V1: EngineContractV1 = {
	contract_schema_version: 1,
	accepted_transport_versions: [5, 6],
	plugins: [
		{
			plugin_id: "ordered-map",
			plugin_ordinal: 1,
			result_revision_name: "ordered-map-result/1",
			result_schema_version: 1,
			metrics_revision_name: "ordered-map-metrics/1",
			trace_revision_name: "ordered-map-trace/3",
			accepted_frame_revisions: ["scene-frame/5"],
		},
		{
			plugin_id: "flow",
			plugin_ordinal: 2,
			result_revision_name: "flow-result/9",
			result_schema_version: 9,
			metrics_revision_name: "flow-metrics/6",
			trace_revision_name: "flow-trace/9",
			accepted_frame_revisions: ["flow-scene/9"],
		},
	],
};

export class EngineContractValidationError extends Error {
	constructor(message: string) {
		super(message);
		this.name = "EngineContractValidationError";
	}
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function assertExactKeys(
	value: Record<string, unknown>,
	expected: readonly string[],
	path: string,
) {
	const actual = Object.keys(value).sort();
	const sortedExpected = [...expected].sort();
	if (
		actual.length !== sortedExpected.length ||
		actual.some((key, index) => key !== sortedExpected[index])
	) {
		throw new EngineContractValidationError(
			`${path} has missing or unknown fields`,
		);
	}
}

function nonEmptyString(value: unknown, path: string): string {
	if (typeof value !== "string" || value.length === 0) {
		throw new EngineContractValidationError(`${path} must be non-empty`);
	}
	return value;
}

function boundedInteger(
	value: unknown,
	path: string,
	minimum: number,
	maximum: number,
): number {
	if (
		typeof value !== "number" ||
		!Number.isInteger(value) ||
		value < minimum ||
		value > maximum
	) {
		throw new EngineContractValidationError(`${path} is outside its range`);
	}
	return value;
}

function uniqueStrings(value: unknown, path: string): string[] {
	if (!Array.isArray(value) || value.length === 0) {
		throw new EngineContractValidationError(`${path} must be non-empty`);
	}
	const strings = value.map((item, index) =>
		nonEmptyString(item, `${path}[${index}]`),
	);
	if (new Set(strings).size !== strings.length) {
		throw new EngineContractValidationError(`${path} contains duplicates`);
	}
	return strings;
}

function parsePlugin(value: unknown, index: number): EnginePluginContractV1 {
	const path = `plugins[${index}]`;
	if (!isRecord(value)) {
		throw new EngineContractValidationError(`${path} must be an object`);
	}
	assertExactKeys(
		value,
		[
			"plugin_id",
			"plugin_ordinal",
			"result_revision_name",
			"result_schema_version",
			"metrics_revision_name",
			"trace_revision_name",
			"accepted_frame_revisions",
		],
		path,
	);
	return {
		plugin_id: nonEmptyString(value.plugin_id, `${path}.plugin_id`),
		plugin_ordinal: boundedInteger(
			value.plugin_ordinal,
			`${path}.plugin_ordinal`,
			1,
			0xffff_ffff,
		),
		result_revision_name: nonEmptyString(
			value.result_revision_name,
			`${path}.result_revision_name`,
		),
		result_schema_version: boundedInteger(
			value.result_schema_version,
			`${path}.result_schema_version`,
			1,
			0xffff_ffff,
		),
		metrics_revision_name: nonEmptyString(
			value.metrics_revision_name,
			`${path}.metrics_revision_name`,
		),
		trace_revision_name: nonEmptyString(
			value.trace_revision_name,
			`${path}.trace_revision_name`,
		),
		accepted_frame_revisions: uniqueStrings(
			value.accepted_frame_revisions,
			`${path}.accepted_frame_revisions`,
		),
	};
}

export function parseEngineContractV1(value: unknown): EngineContractV1 {
	if (!isRecord(value)) {
		throw new EngineContractValidationError(
			"engine contract must be an object",
		);
	}
	assertExactKeys(
		value,
		["contract_schema_version", "accepted_transport_versions", "plugins"],
		"engine contract",
	);
	if (value.contract_schema_version !== 1) {
		throw new EngineContractValidationError(
			"engine contract schema is unsupported",
		);
	}
	if (
		!Array.isArray(value.accepted_transport_versions) ||
		value.accepted_transport_versions.length === 0
	) {
		throw new EngineContractValidationError(
			"accepted transport versions must be non-empty",
		);
	}
	const transports = value.accepted_transport_versions.map((item, index) =>
		boundedInteger(item, `accepted_transport_versions[${index}]`, 1, 0xffff),
	);
	if (new Set(transports).size !== transports.length) {
		throw new EngineContractValidationError(
			"accepted transport versions contain duplicates",
		);
	}
	if (!Array.isArray(value.plugins) || value.plugins.length === 0) {
		throw new EngineContractValidationError("plugins must be non-empty");
	}
	const plugins = value.plugins.map(parsePlugin);
	const ordinals = plugins.map((plugin) => plugin.plugin_ordinal);
	const ids = plugins.map((plugin) => plugin.plugin_id);
	if (
		new Set(ordinals).size !== ordinals.length ||
		new Set(ids).size !== ids.length
	) {
		throw new EngineContractValidationError(
			"plugin IDs and ordinals must be unique",
		);
	}
	if (
		ordinals.some((ordinal, index) => {
			const previous = ordinals[index - 1];
			return previous !== undefined && ordinal <= previous;
		})
	) {
		throw new EngineContractValidationError(
			"plugin ordinals must be strictly increasing",
		);
	}
	return {
		contract_schema_version: 1,
		accepted_transport_versions: transports,
		plugins,
	};
}

export function assertExpectedEngineContractV1(
	value: unknown,
): EngineContractV1 {
	const actual = parseEngineContractV1(value);
	if (JSON.stringify(actual) !== JSON.stringify(EXPECTED_ENGINE_CONTRACT_V1)) {
		throw new EngineContractValidationError(
			"engine and frontend contracts do not match",
		);
	}
	return actual;
}
