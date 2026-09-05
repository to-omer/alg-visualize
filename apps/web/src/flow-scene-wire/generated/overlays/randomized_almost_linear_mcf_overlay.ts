// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowRandomizedAlmostLinearMcfOverlayV1 } from "../FlowRandomizedAlmostLinearMcfOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "randomized_almost_linear_mcf_overlay" as const;
export const DEFINITION = "FlowRandomizedAlmostLinearMcfOverlayV1" as const;

export function decodeStructure(value: unknown): FlowRandomizedAlmostLinearMcfOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowRandomizedAlmostLinearMcfOverlayV1;
}
