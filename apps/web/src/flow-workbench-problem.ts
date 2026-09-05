import type { FlowGeneratorTargetProblem } from "./flow-generator-family-registry";
import {
	type FlowGeneratorFamilyId,
	type FlowGeneratorFixture,
	flowGeneratorFamilyModel,
} from "./flow-generator-fixture";
import type { FlowGeneratorForm, NetgenPresetId } from "./flow-generator-form";
import {
	classifyNetgenForm,
	netgenProblemForPreset,
} from "./flow-netgen-classification";
import type { FlowCurrentSceneV9 } from "./flow-scene";

export type FlowWorkbenchProblemKind = "max-flow" | "min-cost-flow";

const MAX_FLOW_MODELS = new Set([
	"max-flow",
	"parametric-max-flow",
	"planar-max-flow",
	"bipartite-matching",
]);

const MIN_COST_FLOW_MODELS = new Set([
	"fixed-flow-min-cost",
	"min-cost-max-flow",
	"circulation",
	"transshipment",
	"assignment",
	"transportation",
	"convex-cost-flow",
]);

const MIN_COST_TO_MAX_FLOW_TOPOLOGIES = new Set<FlowGeneratorFamilyId>([
	"cycle",
	"goldberg-mesh-circulation",
	"goto-torus",
	"gridgen-grid",
	"gridgraph-grid",
	"torus",
]);

export function flowModelKindWorkbenchProblem(
	modelKind: string,
): FlowWorkbenchProblemKind | undefined {
	if (MAX_FLOW_MODELS.has(modelKind)) return "max-flow";
	if (MIN_COST_FLOW_MODELS.has(modelKind)) return "min-cost-flow";
	return undefined;
}

export function flowModelWorkbenchProblem(
	model: FlowCurrentSceneV9["model"],
): FlowWorkbenchProblemKind {
	const problem = flowModelKindWorkbenchProblem(model.kind);
	if (problem === undefined) {
		throw new Error(`Unsupported validated flow model: ${model.kind}`);
	}
	return problem;
}

/**
 * Reads only the model discriminator needed to keep the two workspaces
 * separate. Full validation remains owned by the Rust engine.
 */
export function flowInputWorkbenchProblem(
	input: string,
	format: "json" | "dsl",
): FlowWorkbenchProblemKind | undefined {
	const modelKind = flowInputModelKind(input, format);
	return modelKind === undefined
		? undefined
		: flowModelKindWorkbenchProblem(modelKind);
}

/**
 * Extracts only the untrusted model discriminator. The caller's workbench
 * policy decides whether that model may cross the ingestion boundary.
 */
export function flowInputModelKind(
	input: string,
	format: "json" | "dsl",
): string | undefined {
	if (format === "dsl") {
		return /^\s*model\s+(\S+)/m.exec(input)?.[1];
	}
	try {
		const document: unknown = JSON.parse(input);
		if (typeof document !== "object" || document === null) return undefined;
		const payload = Reflect.get(document, "payload");
		if (typeof payload !== "object" || payload === null) return undefined;
		const model = Reflect.get(payload, "model");
		if (typeof model !== "object" || model === null) return undefined;
		const kind = Reflect.get(model, "kind");
		return typeof kind === "string" ? kind : undefined;
	} catch {
		return undefined;
	}
}

export function flowGeneratorFamilySupportsProblem(
	family: FlowGeneratorFamilyId,
	problem: FlowWorkbenchProblemKind,
): boolean {
	// NETGEN is a topology family with both native transshipment presets and a
	// native single-source max-flow mode. Its current form decides the model.
	if (family === "netgen-skeleton") return true;
	const nativeProblem = flowModelKindWorkbenchProblem(
		flowGeneratorFamilyModel(family),
	);
	if (nativeProblem === problem) return true;
	if (problem === "min-cost-flow" && nativeProblem === "max-flow") return true;
	return problem === "max-flow" && MIN_COST_TO_MAX_FLOW_TOPOLOGIES.has(family);
}

export function flowGeneratorFormMatchesProblem(
	form: FlowGeneratorForm,
	problem: FlowWorkbenchProblemKind,
): boolean {
	return flowGeneratorFamilySupportsProblem(form.family, problem);
}

export function flowGeneratorTargetProblem(
	form: FlowGeneratorForm,
	problem: FlowWorkbenchProblemKind,
): FlowGeneratorTargetProblem {
	const nativeProblem =
		form.family === "netgen-skeleton"
			? classifyNetgenForm(form) === "max-flow"
				? "max-flow"
				: "min-cost-flow"
			: flowModelKindWorkbenchProblem(flowGeneratorFamilyModel(form.family));
	if (nativeProblem === problem) return "native";
	if (!flowGeneratorFamilySupportsProblem(form.family, problem)) {
		throw new Error(
			`${form.family} cannot generate a ${flowProblemTitle(problem)} scenario`,
		);
	}
	return problem === "max-flow" ? "max-flow" : "fixed-flow-min-cost";
}

export function flowGeneratorFormAdaptationLabel(
	form: FlowGeneratorForm,
	problem: FlowWorkbenchProblemKind,
): string | undefined {
	const target = flowGeneratorTargetProblem(form, problem);
	if (target === "native") return undefined;
	return target === "max-flow"
		? "Topology adapted to source/sink Max Flow"
		: "Topology adapted to fixed-flow Min-Cost Flow";
}

export function flowFixtureAdaptationLabel(
	fixture: FlowGeneratorFixture,
	problem: FlowWorkbenchProblemKind,
): string | undefined {
	const nativeProblem = flowModelKindWorkbenchProblem(fixture.model);
	if (nativeProblem === problem) return undefined;
	if (!flowGeneratorFamilySupportsProblem(fixture.family_id, problem))
		return undefined;
	return problem === "max-flow"
		? "Topology adapted to source/sink Max Flow"
		: "Topology adapted to fixed-flow Min-Cost Flow";
}

export function flowGeneratorFormDisabledReason(
	form: FlowGeneratorForm,
	problem: FlowWorkbenchProblemKind,
): string | undefined {
	if (flowGeneratorFormMatchesProblem(form, problem)) return undefined;
	return `This family is unavailable for ${flowProblemTitle(problem)}`;
}

export function flowNetgenPresetDisabledReason(
	preset: NetgenPresetId,
	problem: FlowWorkbenchProblemKind,
): string | undefined {
	const presetProblem = netgenProblemForPreset(preset);
	if (presetProblem === undefined || presetProblem === problem)
		return undefined;
	return `Preset belongs to ${flowProblemTitle(presetProblem)}`;
}

export function flowFixtureMatchesProblem(
	fixture: FlowGeneratorFixture,
	problem: FlowWorkbenchProblemKind,
): boolean {
	return flowGeneratorFamilySupportsProblem(fixture.family_id, problem);
}

/**
 * Keeps incompatible generator families discoverable without implying that
 * their canonical problem model can be changed by the picker.
 */
export function flowFixtureDisabledReason(
	fixture: FlowGeneratorFixture,
	problem: FlowWorkbenchProblemKind,
): string | undefined {
	if (flowFixtureMatchesProblem(fixture, problem)) return undefined;
	const fixtureProblem = flowModelKindWorkbenchProblem(fixture.model);
	if (fixtureProblem === undefined) return "Unsupported problem model";
	return `Generates ${flowProblemTitle(fixtureProblem)} scenarios`;
}

/** Canonical fixture presets do not inherit a family's alternative modes. */
export function flowFixturePresetDisabledReason(
	fixture: FlowGeneratorFixture,
	problem: FlowWorkbenchProblemKind,
): string | undefined {
	const fixtureProblem = flowModelKindWorkbenchProblem(fixture.model);
	if (fixtureProblem === problem) return undefined;
	if (fixtureProblem === undefined) return "Unsupported problem model";
	return `Preset belongs to ${flowProblemTitle(fixtureProblem)}`;
}

export function flowProblemTitle(problem: FlowWorkbenchProblemKind): string {
	return problem === "max-flow" ? "Max Flow" : "Min-Cost Flow";
}
