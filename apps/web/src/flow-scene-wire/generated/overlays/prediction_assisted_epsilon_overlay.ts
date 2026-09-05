// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowPredictionAssistedEpsilonOverlayV1 } from "../FlowPredictionAssistedEpsilonOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "prediction_assisted_epsilon_overlay" as const;
export const DEFINITION = "FlowPredictionAssistedEpsilonOverlayV1" as const;

export function decodeStructure(value: unknown): FlowPredictionAssistedEpsilonOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowPredictionAssistedEpsilonOverlayV1;
}
