// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowPolynomialPrimalSimplexOverlayV1 } from "../FlowPolynomialPrimalSimplexOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "polynomial_primal_simplex_overlay" as const;
export const DEFINITION = "FlowPolynomialPrimalSimplexOverlayV1" as const;

export function decodeStructure(value: unknown): FlowPolynomialPrimalSimplexOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowPolynomialPrimalSimplexOverlayV1;
}
