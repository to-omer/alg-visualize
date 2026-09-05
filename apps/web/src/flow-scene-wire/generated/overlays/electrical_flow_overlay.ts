// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowElectricalFlowOverlayV1 } from "../FlowElectricalFlowOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "electrical_flow_overlay" as const;
export const DEFINITION = "FlowElectricalFlowOverlayV1" as const;

export function decodeStructure(value: unknown): FlowElectricalFlowOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowElectricalFlowOverlayV1;
}
