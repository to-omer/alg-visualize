// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowFrameworkMcfOverlayV1 } from "../FlowFrameworkMcfOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "flow_framework_mcf_overlay" as const;
export const DEFINITION = "FlowFrameworkMcfOverlayV1" as const;

export function decodeStructure(value: unknown): FlowFrameworkMcfOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowFrameworkMcfOverlayV1;
}
