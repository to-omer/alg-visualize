// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowParametricOverlayV1 } from "../FlowParametricOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "parametric_overlay" as const;
export const DEFINITION = "FlowParametricOverlayV1" as const;

export function decodeStructure(value: unknown): FlowParametricOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowParametricOverlayV1;
}
