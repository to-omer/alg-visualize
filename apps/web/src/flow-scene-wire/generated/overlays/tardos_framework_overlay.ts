// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowTardosFrameworkOverlayV1 } from "../FlowTardosFrameworkOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "tardos_framework_overlay" as const;
export const DEFINITION = "FlowTardosFrameworkOverlayV1" as const;

export function decodeStructure(value: unknown): FlowTardosFrameworkOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowTardosFrameworkOverlayV1;
}
