// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowFeasibilityOverlayV2 } from "../FlowFeasibilityOverlayV2.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "feasibility_overlay" as const;
export const DEFINITION = "FlowFeasibilityOverlayV2" as const;

export function decodeStructure(value: unknown): FlowFeasibilityOverlayV2 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowFeasibilityOverlayV2;
}
