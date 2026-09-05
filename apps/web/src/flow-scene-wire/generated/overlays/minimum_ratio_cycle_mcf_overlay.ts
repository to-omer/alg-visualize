// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowMinimumRatioCycleMcfOverlayV1 } from "../FlowMinimumRatioCycleMcfOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "minimum_ratio_cycle_mcf_overlay" as const;
export const DEFINITION = "FlowMinimumRatioCycleMcfOverlayV1" as const;

export function decodeStructure(value: unknown): FlowMinimumRatioCycleMcfOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowMinimumRatioCycleMcfOverlayV1;
}
