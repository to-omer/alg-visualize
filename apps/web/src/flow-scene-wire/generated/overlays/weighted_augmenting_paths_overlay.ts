// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowWeightedAugmentingPathsOverlayV1 } from "../FlowWeightedAugmentingPathsOverlayV1.js";
import { assertFlowSceneDefinition } from "../../schema-validator";

export const FIELD = "weighted_augmenting_paths_overlay" as const;
export const DEFINITION = "FlowWeightedAugmentingPathsOverlayV1" as const;

export function decodeStructure(value: unknown): FlowWeightedAugmentingPathsOverlayV1 {
	assertFlowSceneDefinition(value, DEFINITION, FIELD);
	return value as FlowWeightedAugmentingPathsOverlayV1;
}
