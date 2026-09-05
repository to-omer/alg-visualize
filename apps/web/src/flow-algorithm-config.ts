const PREDICTION_ASSISTED_EPSILON_ALGORITHM =
	"prediction-assisted-epsilon-relaxation";
const TARDOS_FRAMEWORK_ALGORITHM = "tardos-framework";
const I128_MINIMUM = -(1n << 127n);
const I128_MAXIMUM = (1n << 127n) - 1n;
const CANONICAL_SIGNED_DECIMAL = /^(?:0|-[1-9][0-9]*|[1-9][0-9]*)$/;

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

function isCanonicalI128(value: unknown): value is string {
	if (typeof value !== "string" || !CANONICAL_SIGNED_DECIMAL.test(value)) {
		return false;
	}
	const parsed = BigInt(value);
	return parsed >= I128_MINIMUM && parsed <= I128_MAXIMUM;
}

function assertNodeCompletePotentials(
	value: unknown,
	nodeIds: readonly string[],
	label: string,
): asserts value is Record<string, string> {
	if (!isRecord(value)) {
		throw new Error(`${label} must be an object`);
	}
	const canonicalNodes = new Set(nodeIds);
	if (canonicalNodes.size !== nodeIds.length || !hasExactKeys(value, nodeIds)) {
		throw new Error(
			`${label} must contain every canonical graph node exactly once`,
		);
	}
	if (!Object.values(value).every(isCanonicalI128)) {
		throw new Error(`${label} must contain canonical i128 strings`);
	}
}

/**
 * Mirrors the closed algorithm-config boundary enforced by the WASM runtime.
 * The worker checks it before mutating the selected Scenario so malformed
 * algorithm-specific configuration cannot cross the JS/WASM boundary.
 */
export function assertFlowAlgorithmConfig(
	algorithmId: string,
	config: Record<string, unknown>,
	nodeIds: readonly string[],
): void {
	if (algorithmId === TARDOS_FRAMEWORK_ALGORITHM) {
		if (!hasExactKeys(config, ["potentials"])) {
			throw new Error("Tardos framework requires exactly potentials");
		}
		assertNodeCompletePotentials(
			config.potentials,
			nodeIds,
			"Tardos framework potentials",
		);
		return;
	}
	if (algorithmId === PREDICTION_ASSISTED_EPSILON_ALGORITHM) {
		if (!hasExactKeys(config, ["predicted_potentials", "scaling_parameter"])) {
			throw new Error(
				"prediction-assisted epsilon relaxation requires exactly predicted_potentials and scaling_parameter",
			);
		}
		assertNodeCompletePotentials(
			config.predicted_potentials,
			nodeIds,
			"predicted_potentials",
		);
		if (
			!Number.isInteger(config.scaling_parameter) ||
			typeof config.scaling_parameter !== "number" ||
			config.scaling_parameter < 2 ||
			config.scaling_parameter > 4
		) {
			throw new Error("scaling_parameter must be the integer 2, 3, or 4");
		}
		return;
	}
	if (Object.keys(config).length !== 0) {
		throw new Error("This executable flow algorithm requires an empty config");
	}
}

/** Builds the node-complete default config for the exact Scenario being edited. */
export function defaultFlowAlgorithmConfig(
	algorithmId: string,
	nodeIds: readonly string[],
): Record<string, unknown> {
	if (algorithmId === TARDOS_FRAMEWORK_ALGORITHM) {
		return {
			potentials: Object.fromEntries(nodeIds.map((nodeId) => [nodeId, "0"])),
		};
	}
	if (algorithmId === PREDICTION_ASSISTED_EPSILON_ALGORITHM) {
		return {
			predicted_potentials: Object.fromEntries(
				nodeIds.map((nodeId) => [nodeId, "0"]),
			),
			scaling_parameter: 2,
		};
	}
	return {};
}

/** Extracts the canonical graph-node boundary needed by config validation. */
export function flowScenarioNodeIds(source: string): readonly string[] {
	const value: unknown = JSON.parse(source);
	if (
		!isRecord(value) ||
		!isRecord(value.payload) ||
		!isRecord(value.payload.graph) ||
		!Array.isArray(value.payload.graph.nodes)
	) {
		throw new Error("Flow algorithm config requires a valid graph node list");
	}
	const nodeIds = value.payload.graph.nodes.map((node) =>
		isRecord(node) && typeof node.id === "string" ? node.id : undefined,
	);
	if (
		nodeIds.some((node): node is undefined => node === undefined) ||
		new Set(nodeIds).size !== nodeIds.length
	) {
		throw new Error("Flow algorithm config requires unique graph node ids");
	}
	return nodeIds as string[];
}
