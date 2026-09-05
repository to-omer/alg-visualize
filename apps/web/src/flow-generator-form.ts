import type { FlowGeneratorFamilyId } from "./flow-generator-fixture";

export type AssignmentMatrixShapeId =
	| "uniform"
	| "equal"
	| "block"
	| "near-tie"
	| "planted-optimum"
	| "monge"
	| "anti-monge"
	| "sparse-allowed"
	| "hall-deficient";

export type TransportationTableShapeId =
	| "dense-uniform"
	| "sparse-feasible"
	| "unit-degenerate"
	| "block"
	| "near-tie"
	| "monge"
	| "cut-infeasible";

export type GridgraphPresetId =
	| "readable"
	| "square"
	| "wide"
	| "long"
	| "custom";
export type WashingtonPresetId =
	| "readable"
	| "wide"
	| "tall"
	| "dense"
	| "custom";
export type WashingtonMatchingPresetId =
	| "readable"
	| "sparse"
	| "medium"
	| "dense"
	| "custom";
export type WashingtonSquareMeshPresetId = WashingtonMatchingPresetId;
export type NetgenPresetId =
	| "general-min-cost"
	| "transportation"
	| "assignment"
	| "single-source-max-flow"
	| "dense-transshipment"
	| "custom";

export type FlowGeneratorForm = {
	family: FlowGeneratorFamilyId;
	seed: string;
	primary: number;
	secondary: number;
	tertiary: number;
	quaternary: number;
	toggle: boolean;
	assignmentShape: AssignmentMatrixShapeId;
	transportationShape: TransportationTableShapeId;
	assignmentObjective: "minimize" | "maximize";
	assignmentNoise: number;
	gridgenTotalSupply: string;
	gridgraphPreset: GridgraphPresetId;
	washingtonPreset: WashingtonPresetId;
	washingtonMatchingPreset: WashingtonMatchingPresetId;
	washingtonSquareMeshPreset: WashingtonSquareMeshPresetId;
	netgenPreset: NetgenPresetId;
	netgenTotalSupply: string;
	netgenTransshipmentSources: number;
	netgenTransshipmentSinks: number;
	netgenHighCostPercentage: number;
	netgenCapacitatedPercentage: number;
	capacityKind:
		| "unit"
		| "constant"
		| "uniform"
		| "bimodal"
		| "power-of-two-buckets";
	capacityMinimum: string;
	capacityMaximum: string;
	costKind: "zero" | "constant" | "uniform" | "bimodal" | "capacity-correlated";
	costMinimum: string;
	costMaximum: string;
	costCorrelationDirection: "positive" | "negative";
	costMaximumJitter: string;
};

export const DEFAULT_FLOW_GENERATOR: FlowGeneratorForm = {
	family: "layered-dag",
	seed: "42",
	primary: 5,
	secondary: 4,
	tertiary: 2,
	quaternary: 5,
	toggle: false,
	assignmentShape: "planted-optimum",
	transportationShape: "sparse-feasible",
	assignmentObjective: "minimize",
	assignmentNoise: 3,
	gridgenTotalSupply: "20",
	gridgraphPreset: "readable",
	washingtonPreset: "readable",
	washingtonMatchingPreset: "readable",
	washingtonSquareMeshPreset: "readable",
	netgenPreset: "general-min-cost",
	netgenTotalSupply: "60",
	netgenTransshipmentSources: 1,
	netgenTransshipmentSinks: 1,
	netgenHighCostPercentage: 75,
	netgenCapacitatedPercentage: 65,
	capacityKind: "uniform",
	capacityMinimum: "3",
	capacityMaximum: "12",
	costKind: "uniform",
	costMinimum: "-3",
	costMaximum: "5",
	costCorrelationDirection: "positive",
	costMaximumJitter: "1",
};

export const DEFAULT_MIN_COST_FLOW_GENERATOR: FlowGeneratorForm = {
	...DEFAULT_FLOW_GENERATOR,
};
