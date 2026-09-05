import type { EngineRequest } from "./engine-types";

export type FlowEngineErrorOwner =
	| "algorithm-catalog"
	| "generator-fixtures"
	| "workspace";

/** Routes an engine failure from immutable request identity, never UI timing. */
export function flowEngineErrorOwner(
	requestKind: EngineRequest["kind"],
): FlowEngineErrorOwner {
	switch (requestKind) {
		case "get-flow-catalog":
		case "set-algorithm":
			return "algorithm-catalog";
		case "get-flow-generator-fixtures":
			return "generator-fixtures";
		default:
			return "workspace";
	}
}
