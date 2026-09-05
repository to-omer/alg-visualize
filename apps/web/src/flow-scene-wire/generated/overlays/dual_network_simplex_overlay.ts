// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowDualNetworkSimplexOverlayV1 } from "../FlowDualNetworkSimplexOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "dual_network_simplex_overlay" as const;
export const DEFINITION = "FlowDualNetworkSimplexOverlayV1" as const;

export function decodeStructure(value: unknown): FlowDualNetworkSimplexOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowDualNetworkSimplexOverlayV1;
}
