// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowEnhancedCapacityScalingOverlayV1 } from "../FlowEnhancedCapacityScalingOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "enhanced_capacity_scaling_overlay" as const;
export const DEFINITION = "FlowEnhancedCapacityScalingOverlayV1" as const;

export function decodeStructure(value: unknown): FlowEnhancedCapacityScalingOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowEnhancedCapacityScalingOverlayV1;
}
