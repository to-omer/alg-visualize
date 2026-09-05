import { describe, expect, it } from "vitest";

import { FLOW_GENERATOR_PREVIEW_LAYOUTS } from "./FlowGeneratorShapePreview";

describe("flow generator shape preview", () => {
	it("covers the closed nine-class layout inventory", () => {
		expect(FLOW_GENERATOR_PREVIEW_LAYOUTS).toEqual([
			"linear-layered",
			"radial-cyclic",
			"grid-local",
			"grid-periodic",
			"partitioned",
			"hierarchical",
			"clustered",
			"dense-spatial",
			"benchmark-gadget",
		]);
	});
});
