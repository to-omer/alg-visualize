// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowPrimalDualIpmMcfOverlayV1 } from "../FlowPrimalDualIpmMcfOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "primal_dual_ipm_mcf_overlay" as const;
export const DEFINITION = "FlowPrimalDualIpmMcfOverlayV1" as const;

export function decodeStructure(value: unknown): FlowPrimalDualIpmMcfOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowPrimalDualIpmMcfOverlayV1;
}
