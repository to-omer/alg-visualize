// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowCancelTightenOverlayV1 } from "../FlowCancelTightenOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "cancel_tighten_overlay" as const;
export const DEFINITION = "FlowCancelTightenOverlayV1" as const;

export function decodeStructure(value: unknown): FlowCancelTightenOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowCancelTightenOverlayV1;
}
