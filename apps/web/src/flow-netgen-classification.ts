import type { FlowGeneratorForm, NetgenPresetId } from "./flow-generator-form";

export type NetgenProblemKind =
	| "assignment"
	| "transportation"
	| "transshipment"
	| "max-flow";

function canonicalTotalSupply(value: string): bigint | undefined {
	if (!/^(0|[1-9][0-9]*)$/.test(value)) return undefined;
	try {
		const parsed = BigInt(value);
		return parsed <= 1_000_000_000n ? parsed : undefined;
	} catch {
		return undefined;
	}
}

/** Mirrors Rust's NETGEN problem classifier, including assignment priority. */
export function classifyNetgenForm(form: FlowGeneratorForm): NetgenProblemKind {
	const totalSupply = canonicalTotalSupply(form.netgenTotalSupply);
	const terminalCount = form.secondary + form.tertiary;
	if (
		Number.isSafeInteger(form.secondary) &&
		terminalCount === form.primary &&
		form.secondary === form.tertiary &&
		form.netgenTransshipmentSources === 0 &&
		form.netgenTransshipmentSinks === 0 &&
		totalSupply === BigInt(form.secondary)
	) {
		return "assignment";
	}
	if (form.costMinimum === "1" && form.costMaximum === "1") return "max-flow";
	if (
		terminalCount === form.primary &&
		form.netgenTransshipmentSources === 0 &&
		form.netgenTransshipmentSinks === 0
	) {
		return "transportation";
	}
	return "transshipment";
}

export function netgenProblemForPreset(
	preset: NetgenPresetId,
): "max-flow" | "min-cost-flow" | undefined {
	if (preset === "custom") return undefined;
	return preset === "single-source-max-flow" ? "max-flow" : "min-cost-flow";
}
