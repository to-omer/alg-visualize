// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowConvexCostOverlayV1 } from "../FlowConvexCostOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "convex_cost_overlay" as const;
export const DEFINITION = "FlowConvexCostOverlayV1" as const;

export function decodeStructure(value: unknown): FlowConvexCostOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowConvexCostOverlayV1;
}
