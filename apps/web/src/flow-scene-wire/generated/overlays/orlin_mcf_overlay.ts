// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowOrlinMcfOverlayV1 } from "../FlowOrlinMcfOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "orlin_mcf_overlay" as const;
export const DEFINITION = "FlowOrlinMcfOverlayV1" as const;

export function decodeStructure(value: unknown): FlowOrlinMcfOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowOrlinMcfOverlayV1;
}
