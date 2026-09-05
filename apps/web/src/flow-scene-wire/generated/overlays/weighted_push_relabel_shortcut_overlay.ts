// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowWeightedPushRelabelShortcutOverlayV1 } from "../FlowWeightedPushRelabelShortcutOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "weighted_push_relabel_shortcut_overlay" as const;
export const DEFINITION = "FlowWeightedPushRelabelShortcutOverlayV1" as const;

export function decodeStructure(value: unknown): FlowWeightedPushRelabelShortcutOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowWeightedPushRelabelShortcutOverlayV1;
}
