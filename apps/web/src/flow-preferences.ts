import {
	FLOW_GENERATOR_FAMILY_IDS,
	FLOW_GENERATOR_PICKER_GROUPS,
	type FlowGeneratorPickerGroup,
} from "./flow-generator-fixture";
import type { FlowGeneratorForm } from "./flow-generator-form";
import type { FlowWorkbenchProblemKind } from "./flow-workbench-problem";

export type FlowViewMode = "original" | "residual" | "both";
export type FlowPlaybackGranularity = "phase" | "operation" | "micro";

const STORAGE_KEY = "algorithm-workbench/flow-preferences/v2";
const VERSION = 2;

type FlowPreferencesV2 = Readonly<{
	version: 2;
	generator?: Partial<
		Record<FlowWorkbenchProblemKind, Partial<FlowGeneratorForm>>
	>;
	viewMode?: Partial<Record<FlowWorkbenchProblemKind, FlowViewMode>>;
	granularity?: Partial<
		Record<FlowWorkbenchProblemKind, FlowPlaybackGranularity>
	>;
	familyGroup?: Partial<
		Record<FlowWorkbenchProblemKind, FlowGeneratorPickerGroup>
	>;
}>;

export type FlowPreferenceSnapshot = Readonly<{
	generator: FlowGeneratorForm;
	viewMode: FlowViewMode;
	granularity: FlowPlaybackGranularity;
	familyGroup: FlowGeneratorPickerGroup;
}>;

export function validateRestoredFlowGenerator(
	candidate: FlowGeneratorForm,
	fallback: FlowGeneratorForm,
): FlowGeneratorForm {
	const numberFields = [
		"primary",
		"secondary",
		"tertiary",
		"quaternary",
		"assignmentNoise",
		"netgenTransshipmentSources",
		"netgenTransshipmentSinks",
		"netgenHighCostPercentage",
		"netgenCapacitatedPercentage",
	] as const satisfies readonly (keyof FlowGeneratorForm)[];
	const unsignedIntegerStringFields = [
		"seed",
		"gridgenTotalSupply",
		"netgenTotalSupply",
		"capacityMinimum",
		"capacityMaximum",
		"costMaximumJitter",
	] as const satisfies readonly (keyof FlowGeneratorForm)[];
	const signedIntegerStringFields = [
		"costMinimum",
		"costMaximum",
	] as const satisfies readonly (keyof FlowGeneratorForm)[];
	const canonicalUnsignedInteger = /^(?:0|[1-9][0-9]*)$/;
	const canonicalSignedInteger = /^(?:0|-?[1-9][0-9]*)$/;
	return numberFields.every(
		(field) =>
			Number.isSafeInteger(candidate[field]) && Number(candidate[field]) >= 0,
	) &&
		unsignedIntegerStringFields.every((field) =>
			canonicalUnsignedInteger.test(String(candidate[field])),
		) &&
		signedIntegerStringFields.every((field) =>
			canonicalSignedInteger.test(String(candidate[field])),
		) &&
		BigInt(candidate.seed) <= 18_446_744_073_709_551_615n &&
		candidate.primary > 0 &&
		candidate.netgenHighCostPercentage <= 100 &&
		candidate.netgenCapacitatedPercentage <= 100
		? candidate
		: fallback;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseStoredPreferences(
	raw: string | null,
): FlowPreferencesV2 | undefined {
	if (raw === null) return undefined;
	try {
		const value: unknown = JSON.parse(raw);
		if (!isRecord(value) || value.version !== VERSION) return undefined;
		return value as FlowPreferencesV2;
	} catch {
		return undefined;
	}
}

const GENERATOR_ENUM_VALUES = {
	assignmentShape: [
		"uniform",
		"equal",
		"block",
		"near-tie",
		"planted-optimum",
		"monge",
		"anti-monge",
		"sparse-allowed",
		"hall-deficient",
	],
	transportationShape: [
		"dense-uniform",
		"sparse-feasible",
		"unit-degenerate",
		"block",
		"near-tie",
		"monge",
		"cut-infeasible",
	],
	assignmentObjective: ["minimize", "maximize"],
	gridgraphPreset: ["readable", "square", "wide", "long", "custom"],
	washingtonPreset: ["readable", "wide", "tall", "dense", "custom"],
	washingtonMatchingPreset: ["readable", "sparse", "medium", "dense", "custom"],
	washingtonSquareMeshPreset: [
		"readable",
		"sparse",
		"medium",
		"dense",
		"custom",
	],
	netgenPreset: [
		"general-min-cost",
		"transportation",
		"assignment",
		"single-source-max-flow",
		"dense-transshipment",
		"custom",
	],
	capacityKind: [
		"unit",
		"constant",
		"uniform",
		"bimodal",
		"power-of-two-buckets",
	],
	costKind: ["zero", "constant", "uniform", "bimodal", "capacity-correlated"],
	costCorrelationDirection: ["positive", "negative"],
} as const satisfies Partial<
	Record<keyof FlowGeneratorForm, readonly string[]>
>;

function restoredGenerator(
	value: unknown,
	fallback: FlowGeneratorForm,
): FlowGeneratorForm {
	if (!isRecord(value)) return fallback;
	const candidate = { ...fallback };
	const candidateRecord = candidate as unknown as Record<string, unknown>;
	for (const key of Object.keys(fallback) as (keyof FlowGeneratorForm)[]) {
		const stored = value[key];
		if (typeof stored !== typeof fallback[key]) continue;
		const allowed = GENERATOR_ENUM_VALUES[
			key as keyof typeof GENERATOR_ENUM_VALUES
		] as readonly string[] | undefined;
		if (allowed !== undefined && !allowed.includes(stored as string)) continue;
		candidateRecord[key] = stored;
	}
	if (!FLOW_GENERATOR_FAMILY_IDS.includes(candidate.family)) return fallback;
	try {
		return validateRestoredFlowGenerator(candidate, fallback);
	} catch {
		return fallback;
	}
}

function storedViewMode(value: unknown, fallback: FlowViewMode): FlowViewMode {
	return value === "original" || value === "residual" || value === "both"
		? value
		: fallback;
}

function storedGranularity(
	value: unknown,
	fallback: FlowPlaybackGranularity,
): FlowPlaybackGranularity {
	return value === "phase" || value === "operation" || value === "micro"
		? value
		: fallback;
}

function storedFamilyGroup(value: unknown): FlowGeneratorPickerGroup {
	return typeof value === "string" &&
		FLOW_GENERATOR_PICKER_GROUPS.some((group) => group === value)
		? (value as FlowGeneratorPickerGroup)
		: "all";
}

export function readFlowPreferences(
	storage: Pick<Storage, "getItem"> | undefined,
	problem: FlowWorkbenchProblemKind,
	fallbackGenerator: FlowGeneratorForm,
): FlowPreferenceSnapshot {
	let stored: FlowPreferencesV2 | undefined;
	try {
		stored = parseStoredPreferences(storage?.getItem(STORAGE_KEY) ?? null);
	} catch {
		stored = undefined;
	}
	return {
		generator: restoredGenerator(
			stored?.generator?.[problem],
			fallbackGenerator,
		),
		viewMode: storedViewMode(stored?.viewMode?.[problem], "original"),
		granularity: storedGranularity(stored?.granularity?.[problem], "operation"),
		familyGroup: storedFamilyGroup(stored?.familyGroup?.[problem]),
	};
}

export function writeFlowPreferences(
	storage: Pick<Storage, "getItem" | "setItem"> | undefined,
	problem: FlowWorkbenchProblemKind,
	value: FlowPreferenceSnapshot,
): void {
	if (storage === undefined) return;
	try {
		const previous = parseStoredPreferences(storage.getItem(STORAGE_KEY));
		const next: FlowPreferencesV2 = {
			version: VERSION,
			generator: {
				...previous?.generator,
				[problem]: { ...value.generator },
			},
			viewMode: { ...previous?.viewMode, [problem]: value.viewMode },
			granularity: {
				...previous?.granularity,
				[problem]: value.granularity,
			},
			familyGroup: {
				...previous?.familyGroup,
				[problem]: value.familyGroup,
			},
		};
		storage.setItem(STORAGE_KEY, JSON.stringify(next));
	} catch {
		// Storage can be unavailable in private or policy-restricted contexts.
	}
}
