// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowDeterministicAlmostLinearOverlayV1 } from "../FlowDeterministicAlmostLinearOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "deterministic_almost_linear_overlay" as const;
export const DEFINITION = "FlowDeterministicAlmostLinearOverlayV1" as const;

export function decodeStructure(value: unknown): FlowDeterministicAlmostLinearOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowDeterministicAlmostLinearOverlayV1;
}
