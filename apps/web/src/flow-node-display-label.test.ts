import { describe, expect, it } from "vitest";
import { flowNodeCanvasLabel } from "./flow-node-display-label";

describe("flow node canvas labels", () => {
	it("shortens layered generator IDs and preserves short stable IDs", () => {
		expect(flowNodeCanvasLabel("l000n0000")).toBe("L0·0");
		expect(flowNodeCanvasLabel("l012n0042")).toBe("L12·42");
		expect(flowNodeCanvasLabel("source")).toBe("source");
	});

	it("compacts arbitrary long IDs without splitting Unicode code points", () => {
		expect(flowNodeCanvasLabel("source-node")).toBe("sour…ode");
		expect(flowNodeCanvasLabel("zz-pad-node-00004")).toBe("zz-p…004");
		expect(flowNodeCanvasLabel("A😀BCDEFGH")).toBe("A😀BC…FGH");
	});
});
