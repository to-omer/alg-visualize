// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowRandomizedAlmostLinearOverlayV1 } from "../FlowRandomizedAlmostLinearOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "randomized_almost_linear_overlay" as const;
export const DEFINITION = "FlowRandomizedAlmostLinearOverlayV1" as const;

export function decodeStructure(value: unknown): FlowRandomizedAlmostLinearOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowRandomizedAlmostLinearOverlayV1;
}
