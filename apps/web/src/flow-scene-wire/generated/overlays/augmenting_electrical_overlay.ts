// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowAugmentingElectricalOverlayV1 } from "../FlowAugmentingElectricalOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "augmenting_electrical_overlay" as const;
export const DEFINITION = "FlowAugmentingElectricalOverlayV1" as const;

export function decodeStructure(value: unknown): FlowAugmentingElectricalOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowAugmentingElectricalOverlayV1;
}
