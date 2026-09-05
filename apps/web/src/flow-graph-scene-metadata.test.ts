import { describe, expect, it } from "vitest";
import { flowGraphModelAccessibleDescription } from "./flow-graph-scene-metadata";

describe("flowGraphModelAccessibleDescription", () => {
	it("describes Max Flow without a cost channel", () => {
		const description = flowGraphModelAccessibleDescription({
			kind: "max-flow",
			source: "s",
			sink: "t",
		});

		expect(description).toContain(
			"Outer edge width shows capacity; inner width shows current flow; arrow markers show edge direction.",
		);
		expect(description).toContain(
			"Leader lines connect visible annotations to their exact edge",
		);
		expect(description).toContain(
			"parallel edges use separated curved lanes with one-based arrow tokens",
		);
		expect(description).toContain(
			"The minimum cut is highlighted after optimization.",
		);
		expect(description).not.toContain("cost");
		expect(description).not.toContain("intensity");
	});

	it("describes Min-Cost Flow with continuous signed-cost encoding and no minimum cut", () => {
		const description = flowGraphModelAccessibleDescription({
			kind: "fixed-flow-min-cost",
			source: "s",
			sink: "t",
			required_flow: "10",
		});

		expect(description).toContain(
			"Outer edge width shows capacity; inner width shows current flow; arrow markers show edge direction.",
		);
		expect(description).toContain(
			"Color and dash pattern show signed unit cost, continuous intensity shows absolute cost magnitude",
		);
		expect(description).not.toContain("minimum cut");
		expect(description).not.toContain("four absolute-cost levels");
	});

	it("describes Min-Cost Max-Flow with both signed cost and its certified minimum cut", () => {
		const description = flowGraphModelAccessibleDescription({
			kind: "min-cost-max-flow",
			source: "s",
			sink: "t",
		});

		expect(description).toContain(
			"signed unit cost, continuous intensity shows absolute cost magnitude",
		);
		expect(description).toContain(
			"The minimum cut is highlighted after optimization.",
		);
	});

	it("does not claim a generic minimum cut for matching or min-cost models", () => {
		const descriptions = [
			flowGraphModelAccessibleDescription({
				kind: "bipartite-matching",
				left: ["a"],
				right: ["b"],
			}),
			flowGraphModelAccessibleDescription({ kind: "circulation" }),
			flowGraphModelAccessibleDescription({ kind: "transshipment" }),
			flowGraphModelAccessibleDescription({
				kind: "assignment",
				agents: ["a"],
				tasks: ["b"],
				objective: "minimize",
			}),
			flowGraphModelAccessibleDescription({
				kind: "transportation",
				origins: ["a"],
				destinations: ["b"],
			}),
			flowGraphModelAccessibleDescription({ kind: "convex-cost-flow" }),
		];

		for (const description of descriptions) {
			expect(description).not.toContain("minimum cut");
		}
		expect(descriptions[0]).not.toContain("cost");
		for (const description of descriptions.slice(1)) {
			expect(description).toContain("signed unit cost");
		}
	});
});
