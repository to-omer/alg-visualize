// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowInteriorPointMaxFlowOverlayV1 } from "../FlowInteriorPointMaxFlowOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "interior_point_max_flow_overlay" as const;
export const DEFINITION = "FlowInteriorPointMaxFlowOverlayV1" as const;

export function decodeStructure(value: unknown): FlowInteriorPointMaxFlowOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowInteriorPointMaxFlowOverlayV1;
}
