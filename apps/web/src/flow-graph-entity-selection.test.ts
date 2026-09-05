import { describe, expect, it } from "vitest";
import type { FlowEntitySelection } from "./flow-entity-navigator";
import {
	isOriginalEdgeSelected,
	isResidualArcSelected,
} from "./flow-graph-entity-selection";

describe("graph entity selection", () => {
	const originalEdge: FlowEntitySelection = { kind: "edge", id: "edge-1" };
	const forwardArc: FlowEntitySelection = {
		kind: "residual-arc",
		id: "edge-1:forward",
		edgeId: "edge-1",
		direction: "forward",
	};

	it("keeps original-edge and residual-arc selection domains separate", () => {
		expect(isOriginalEdgeSelected(originalEdge, "edge-1")).toBe(true);
		expect(isOriginalEdgeSelected(forwardArc, "edge-1")).toBe(false);
		expect(isResidualArcSelected(originalEdge, "edge-1", "forward")).toBe(
			false,
		);
		expect(isResidualArcSelected(forwardArc, "edge-1", "forward")).toBe(true);
	});

	it("requires both edge id and direction for a residual arc", () => {
		expect(isResidualArcSelected(forwardArc, "edge-2", "forward")).toBe(false);
		expect(isResidualArcSelected(forwardArc, "edge-1", "reverse")).toBe(false);
	});
});
