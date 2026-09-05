// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowRelaxedMndcOverlayV1 } from "../FlowRelaxedMndcOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "relaxed_mndc_overlay" as const;
export const DEFINITION = "FlowRelaxedMndcOverlayV1" as const;

export function decodeStructure(value: unknown): FlowRelaxedMndcOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowRelaxedMndcOverlayV1;
}
