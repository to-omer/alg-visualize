import { describe, expect, it } from "vitest";

import { flowEngineErrorOwner } from "./flow-engine-error-owner";

describe("flow engine error ownership", () => {
	it("routes reverse-order failures by the request that produced each response", () => {
		const responses = [
			{ requestKind: "next" as const, message: "navigation failed" },
			{ requestKind: "get-flow-catalog" as const, message: "catalog failed" },
			{
				requestKind: "get-flow-generator-fixtures" as const,
				message: "fixtures failed",
			},
		];

		expect(
			responses.map((response) => ({
				message: response.message,
				owner: flowEngineErrorOwner(response.requestKind),
			})),
		).toEqual([
			{ message: "navigation failed", owner: "workspace" },
			{ message: "catalog failed", owner: "algorithm-catalog" },
			{ message: "fixtures failed", owner: "generator-fixtures" },
		]);
	});
});
