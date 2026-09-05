import { readFile } from "node:fs/promises";

import { describe, expect, it } from "vitest";

import {
	assertExpectedEngineContractV1,
	EngineContractValidationError,
	parseEngineContractV1,
} from "./engine-contract";

const fixtureUrl = new URL(
	"../../../fixtures/contracts/engine-contract-v1.json",
	import.meta.url,
);

async function fixtureContract(): Promise<Record<string, unknown>> {
	const fixture = JSON.parse(await readFile(fixtureUrl, "utf8")) as {
		contract: Record<string, unknown>;
	};
	return fixture.contract;
}

describe("EngineContractV1", () => {
	it("accepts the exact Rust-authority fixture", async () => {
		const contract = await fixtureContract();

		expect(assertExpectedEngineContractV1(contract)).toEqual(contract);
	});

	it("rejects unknown fields before session creation", async () => {
		const contract = { ...(await fixtureContract()), future: true };

		expect(() => parseEngineContractV1(contract)).toThrowError(
			EngineContractValidationError,
		);
	});

	it("rejects reused ordinals and mismatched frame revisions", async () => {
		const contract = await fixtureContract();
		const plugins = structuredClone(contract.plugins) as Array<
			Record<string, unknown>
		>;
		const secondPlugin = plugins[1];
		expect(secondPlugin).toBeDefined();
		if (secondPlugin === undefined)
			throw new Error("fixture plugin is missing");
		secondPlugin.plugin_ordinal = 1;
		expect(() => parseEngineContractV1({ ...contract, plugins })).toThrowError(
			/unique/,
		);

		const mismatched = structuredClone(contract);
		const mismatchedPlugin = (
			mismatched.plugins as Array<Record<string, unknown>>
		)[1];
		expect(mismatchedPlugin).toBeDefined();
		if (mismatchedPlugin === undefined)
			throw new Error("fixture plugin is missing");
		const frameRevisions =
			mismatchedPlugin.accepted_frame_revisions as string[];
		frameRevisions[0] = "flow-scene/1";
		expect(() => assertExpectedEngineContractV1(mismatched)).toThrowError(
			/do not match/,
		);
	});
});
