import { describe, expect, it } from "vitest";
import {
	flowResourceLimitMessage,
	flowResourceLimitResultLabel,
} from "./flow-resource-limit";
import type { FlowCurrentSceneV9 } from "./flow-scene";

function limited(
	reason: NonNullable<FlowCurrentSceneV9["resource_limit_reason"]>,
): FlowCurrentSceneV9 {
	return {
		solve_status: "resource-limit",
		resource_limit_reason: reason,
	} as FlowCurrentSceneV9;
}

describe("flow resource-limit presentation", () => {
	it.each([
		["input-admission", "admission limits", "outside admission limits"],
		["runtime-work", "work ceiling", "work ceiling reached"],
		["transformed-graph", "working graph", "transformed graph ceiling"],
		["trace-publication", "complete trace", "trace publication ceiling"],
		["numerical-convergence", "did not converge", "convergence limit"],
		["declared-ceiling", "resource ceiling", "resource ceiling"],
	] as const)("explains %s without claiming a result", (reason, messageFragment, resultFragment) => {
		const scene = limited(reason);
		expect(flowResourceLimitMessage(scene)).toContain(messageFragment);
		expect(flowResourceLimitMessage(scene)).toMatch(/No .*result|No partial/u);
		expect(flowResourceLimitResultLabel(scene)).toContain(resultFragment);
	});

	it("does not add a resource message to a normal scene", () => {
		const scene = { solve_status: "ready" } as FlowCurrentSceneV9;
		expect(flowResourceLimitMessage(scene)).toBeUndefined();
		expect(flowResourceLimitResultLabel(scene)).toBeUndefined();
	});
});
