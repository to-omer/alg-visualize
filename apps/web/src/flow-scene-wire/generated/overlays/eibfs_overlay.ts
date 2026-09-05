// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowEibfsOverlayV1 } from "../FlowEibfsOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "eibfs_overlay" as const;
export const DEFINITION = "FlowEibfsOverlayV1" as const;

export function decodeStructure(value: unknown): FlowEibfsOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowEibfsOverlayV1;
}
