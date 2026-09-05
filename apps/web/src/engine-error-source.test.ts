import { describe, expect, it } from "vitest";

import { engineRequestErrorSource } from "./engine-error-source";
import { PacketValidationError } from "./packet";

describe("engine error source", () => {
	it.each([
		"next",
		"seek",
		"commit-ack",
		"current-ack",
		"flow-current-ack",
	] as const)("classifies %s as runtime", (kind) => {
		expect(engineRequestErrorSource(kind)).toBe("engine");
	});

	it.each([
		"get-flow-catalog",
		"get-flow-generator-fixtures",
		"create",
		"prepare-dsl",
		"generate",
		"format-dsl",
		"import-scenario",
		"export-scenario",
		"set-algorithm",
	] as const)("classifies %s as input/control", (kind) => {
		expect(engineRequestErrorSource(kind)).toBe("input");
	});

	it.each([
		"next",
		"seek",
	] as const)("keeps a recoverable %s resource limit editable", (kind) => {
		expect(
			engineRequestErrorSource(
				kind,
				new Error("ordered-map resource limit exceeded: visual entity count"),
			),
		).toBe("input");
		expect(
			engineRequestErrorSource(
				kind,
				new PacketValidationError("limits", "producer packet limit"),
			),
		).toBe("input");
		expect(
			engineRequestErrorSource(
				kind,
				new Error("frame JSON byte limit exceeded"),
			),
		).toBe("input");
	});

	it("keeps corrupt runtime failures fatal", () => {
		expect(
			engineRequestErrorSource(
				"next",
				new Error("ordered-map structure is corrupt"),
			),
		).toBe("engine");
	});
});
