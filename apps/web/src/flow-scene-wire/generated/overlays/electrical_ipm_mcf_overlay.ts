// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowElectricalIpmMcfOverlayV1 } from "../FlowElectricalIpmMcfOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "electrical_ipm_mcf_overlay" as const;
export const DEFINITION = "FlowElectricalIpmMcfOverlayV1" as const;

export function decodeStructure(value: unknown): FlowElectricalIpmMcfOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowElectricalIpmMcfOverlayV1;
}
