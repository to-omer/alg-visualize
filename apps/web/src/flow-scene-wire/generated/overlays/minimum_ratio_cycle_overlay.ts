// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowMinimumRatioCycleOverlayV1 } from "../FlowMinimumRatioCycleOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "minimum_ratio_cycle_overlay" as const;
export const DEFINITION = "FlowMinimumRatioCycleOverlayV1" as const;

export function decodeStructure(value: unknown): FlowMinimumRatioCycleOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowMinimumRatioCycleOverlayV1;
}
