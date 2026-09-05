// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowOrlinMaxFlowOverlayV1 } from "../FlowOrlinMaxFlowOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "orlin_max_flow_overlay" as const;
export const DEFINITION = "FlowOrlinMaxFlowOverlayV1" as const;

export function decodeStructure(value: unknown): FlowOrlinMaxFlowOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowOrlinMaxFlowOverlayV1;
}
