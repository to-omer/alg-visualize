import { describe, expect, it } from "vitest";
import { orlinMcfPotentialLabel } from "./flow-node-semantic-projection";

describe("Orlin MCF potential lifecycle", () => {
	it("distinguishes allocated placeholders from initialized dual prices", () => {
		expect(orlinMcfPotentialLabel("ready", "0")).toBe("π …");
		expect(orlinMcfPotentialLabel("transform-capacities", "0")).toBe("π …");
		expect(orlinMcfPotentialLabel("initialize-dual", "0")).toBe("π 0");
		expect(orlinMcfPotentialLabel("begin-phase", "-7")).toBe("π -7");
	});
});
