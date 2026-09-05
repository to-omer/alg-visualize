// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowBinaryBlockingOverlayV1 } from "../FlowBinaryBlockingOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "binary_blocking_overlay" as const;
export const DEFINITION = "FlowBinaryBlockingOverlayV1" as const;

export function decodeStructure(value: unknown): FlowBinaryBlockingOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowBinaryBlockingOverlayV1;
}
