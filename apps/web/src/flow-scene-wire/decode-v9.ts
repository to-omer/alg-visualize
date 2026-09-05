import type { FlowCurrentSceneV9 } from "./generated/FlowCurrentSceneV9";
import { FLOW_SCENE_V9_OVERLAY_DECODERS } from "./generated/overlays";
import { assertFlowSceneV9Root } from "./schema-validator";

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * Verifies the Rust-generated scene V9 wire shape, then composes the generated
 * overlay decoders. Arithmetic and graph semantics remain in `flow-scene.ts`.
 */
export function assertFlowCurrentSceneV9Wire(
	value: unknown,
): asserts value is FlowCurrentSceneV9 {
	assertFlowSceneV9Root(value);
	if (!isRecord(value)) {
		throw new Error("Flow scene V9 wire value is not an object");
	}
	for (const [field, decode] of FLOW_SCENE_V9_OVERLAY_DECODERS) {
		if (Object.hasOwn(value, field)) {
			decode(value[field]);
		}
	}
}
