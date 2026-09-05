// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowPolynomialDualSimplexOverlayV1 } from "../FlowPolynomialDualSimplexOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "polynomial_dual_simplex_overlay" as const;
export const DEFINITION = "FlowPolynomialDualSimplexOverlayV1" as const;

export function decodeStructure(value: unknown): FlowPolynomialDualSimplexOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowPolynomialDualSimplexOverlayV1;
}
