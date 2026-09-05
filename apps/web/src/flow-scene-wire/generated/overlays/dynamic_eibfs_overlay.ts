// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowDynamicEibfsOverlayV1 } from "../FlowDynamicEibfsOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "dynamic_eibfs_overlay" as const;
export const DEFINITION = "FlowDynamicEibfsOverlayV1" as const;

export function decodeStructure(value: unknown): FlowDynamicEibfsOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowDynamicEibfsOverlayV1;
}
