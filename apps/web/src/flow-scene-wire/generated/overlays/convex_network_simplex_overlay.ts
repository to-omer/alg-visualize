// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowConvexNetworkSimplexOverlayV1 } from "../FlowConvexNetworkSimplexOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "convex_network_simplex_overlay" as const;
export const DEFINITION = "FlowConvexNetworkSimplexOverlayV1" as const;

export function decodeStructure(value: unknown): FlowConvexNetworkSimplexOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowConvexNetworkSimplexOverlayV1;
}
