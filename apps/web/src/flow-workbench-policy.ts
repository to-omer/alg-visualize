import type { FlowGeneratorFamilyId } from "./flow-generator-fixture";
import {
	DEFAULT_FLOW_GENERATOR,
	DEFAULT_MIN_COST_FLOW_GENERATOR,
	type FlowGeneratorForm,
} from "./flow-generator-form";
import { defaultFlowScenario } from "./flow-scenario";
import {
	type FlowWorkbenchProblemKind,
	flowGeneratorFamilySupportsProblem,
	flowGeneratorFormMatchesProblem,
	flowModelKindWorkbenchProblem,
	flowProblemTitle,
} from "./flow-workbench-problem";

export type FlowWorkbenchPolicy = Readonly<{
	problem: FlowWorkbenchProblemKind;
	acceptsGeneratorFamily: (family: FlowGeneratorFamilyId) => boolean;
	acceptsModel: (modelKind: string) => boolean;
	defaultDsl: () => string;
	defaultGenerator: () => FlowGeneratorForm;
	defaultScenario: () => string;
	normalizeGenerator: (form: FlowGeneratorForm) => FlowGeneratorForm;
	restoreGenerator: (form: FlowGeneratorForm) => FlowGeneratorForm;
	showsCost: boolean;
}>;

function defaultFlowDsl(problem: FlowWorkbenchProblemKind): string {
	const isMaxFlow = problem === "max-flow";
	return `# Flow DSL
model ${isMaxFlow ? "max-flow source=s sink=t" : "fixed-flow-min-cost source=s sink=t required-flow=10"}
node s x=90 y=270
node a x=290 y=130
node b x=290 y=390
node c x=560 y=130
node d x=560 y=390
node t x=810 y=270
edge sa s -> a capacity=12 cost=${isMaxFlow ? 0 : 2}
edge sb s -> b capacity=8 cost=${isMaxFlow ? 0 : -1}
edge ac a -> c capacity=9 cost=${isMaxFlow ? 0 : 1}
edge ad a -> d capacity=4 cost=${isMaxFlow ? 0 : -2}
edge bc b -> c capacity=3 cost=${isMaxFlow ? 0 : 3}
edge bd b -> d capacity=7 cost=0
edge ct c -> t capacity=10 cost=${isMaxFlow ? 0 : 2}
edge dt d -> t capacity=11 cost=${isMaxFlow ? 0 : -1}
algorithm ${isMaxFlow ? "edmonds-karp" : "successive-shortest-path"}
profile trace
granularity operation
seed 0
`;
}

function maxFlowGenerator(form: FlowGeneratorForm): FlowGeneratorForm {
	if (form.family === "netgen-skeleton") {
		return {
			...form,
			costKind: "constant",
			costMinimum: "1",
			costMaximum: "1",
		};
	}
	return {
		...form,
		costKind: "zero",
		costMinimum: "0",
		costMaximum: "0",
	};
}

function restoreFlowGenerator(
	form: FlowGeneratorForm,
	problem: FlowWorkbenchProblemKind,
	normalize: (candidate: FlowGeneratorForm) => FlowGeneratorForm,
	fallback: () => FlowGeneratorForm,
): FlowGeneratorForm {
	return flowGeneratorFormMatchesProblem(form, problem)
		? normalize(form)
		: fallback();
}

function maxFlowDefaultGenerator(): FlowGeneratorForm {
	return maxFlowGenerator(DEFAULT_FLOW_GENERATOR);
}

function minCostFlowDefaultGenerator(): FlowGeneratorForm {
	return { ...DEFAULT_MIN_COST_FLOW_GENERATOR };
}

const FLOW_WORKBENCH_POLICIES: Readonly<
	Record<FlowWorkbenchProblemKind, FlowWorkbenchPolicy>
> = {
	"max-flow": {
		problem: "max-flow",
		acceptsGeneratorFamily: (family) =>
			flowGeneratorFamilySupportsProblem(family, "max-flow"),
		acceptsModel: (modelKind) =>
			flowModelKindWorkbenchProblem(modelKind) === "max-flow",
		defaultDsl: () => defaultFlowDsl("max-flow"),
		defaultGenerator: maxFlowDefaultGenerator,
		defaultScenario: () => defaultFlowScenario("max-flow"),
		normalizeGenerator: maxFlowGenerator,
		restoreGenerator: (form) =>
			restoreFlowGenerator(
				form,
				"max-flow",
				maxFlowGenerator,
				maxFlowDefaultGenerator,
			),
		showsCost: false,
	},
	"min-cost-flow": {
		problem: "min-cost-flow",
		acceptsGeneratorFamily: (family) =>
			flowGeneratorFamilySupportsProblem(family, "min-cost-flow"),
		acceptsModel: (modelKind) =>
			flowModelKindWorkbenchProblem(modelKind) === "min-cost-flow",
		defaultDsl: () => defaultFlowDsl("min-cost-flow"),
		defaultGenerator: minCostFlowDefaultGenerator,
		defaultScenario: () => defaultFlowScenario("min-cost-flow"),
		normalizeGenerator: (form) => form,
		restoreGenerator: (form) =>
			restoreFlowGenerator(
				form,
				"min-cost-flow",
				(candidate) => candidate,
				minCostFlowDefaultGenerator,
			),
		showsCost: true,
	},
};

export function flowWorkbenchPolicy(
	problem: FlowWorkbenchProblemKind,
): FlowWorkbenchPolicy {
	return FLOW_WORKBENCH_POLICIES[problem];
}

/** Returns the known opposite workspace, or undefined when ingestion may proceed. */
export function rejectedFlowWorkbenchModel(
	policy: FlowWorkbenchPolicy,
	modelKind: string,
): FlowWorkbenchProblemKind | undefined {
	const actualProblem = flowModelKindWorkbenchProblem(modelKind);
	return actualProblem !== undefined && !policy.acceptsModel(modelKind)
		? actualProblem
		: undefined;
}

export function flowWorkbenchModelRejectionMessage(
	policy: FlowWorkbenchPolicy,
	modelKind: string,
): string | undefined {
	const actualProblem = rejectedFlowWorkbenchModel(policy, modelKind);
	return actualProblem === undefined
		? undefined
		: `This input belongs in the ${flowProblemTitle(actualProblem)} workspace.`;
}
