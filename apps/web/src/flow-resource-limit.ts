import type { FlowCurrentSceneV9 } from "./flow-scene";

export function flowResourceLimitMessage(
	scene: FlowCurrentSceneV9 | undefined,
): string | undefined {
	if (scene?.solve_status !== "resource-limit") return undefined;
	switch (scene.resource_limit_reason) {
		case "input-admission":
			return "This input exceeds the selected implementation's published admission limits. No solver result was substituted.";
		case "runtime-work":
			return "The selected implementation reached its published work ceiling before certification. No solver result was substituted.";
		case "transformed-graph":
			return "The algorithm's transformed working graph reached its published size ceiling. No solver result was substituted.";
		case "trace-publication":
			return "The complete trace would exceed its published event or byte ceiling. No partial result was presented as a solution.";
		case "numerical-convergence":
			return "The bounded numerical iteration did not converge within its published limit. No solver result was substituted.";
		case "declared-ceiling":
			return "The selected implementation reached a published resource ceiling. No solver result was substituted.";
	}
}

export function flowResourceLimitResultLabel(
	scene: FlowCurrentSceneV9 | undefined,
): string | undefined {
	if (scene?.solve_status !== "resource-limit") return undefined;
	switch (scene.resource_limit_reason) {
		case "input-admission":
			return "Not run · input outside admission limits";
		case "runtime-work":
			return "No result · work ceiling reached";
		case "transformed-graph":
			return "No result · transformed graph ceiling reached";
		case "trace-publication":
			return "No result · trace publication ceiling reached";
		case "numerical-convergence":
			return "No result · convergence limit reached";
		case "declared-ceiling":
			return "No result · declared resource ceiling reached";
	}
}
