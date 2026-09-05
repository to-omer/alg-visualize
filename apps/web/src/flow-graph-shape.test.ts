import { describe, expect, it } from "vitest";
import {
	analyzeFlowGraphShape,
	directedFlowBipartition,
} from "./flow-graph-shape";
import type { FlowEdgeV1, FlowNodeV1 } from "./flow-scene";

function node(id: string, supply = "0"): FlowNodeV1 {
	return { id, supply };
}

function edge(
	id: string,
	from: string,
	to: string,
	capacity = "1",
): FlowEdgeV1 {
	return { id, from, to, lower: "0", capacity, cost: "0" };
}

describe("flow graph-shape analysis", () => {
	it("recognizes unit networks and rejects one high-degree internal layer", () => {
		const nodes = [node("s"), node("a"), node("b"), node("t")];
		const unit = analyzeFlowGraphShape(
			nodes,
			[
				edge("sa", "s", "a"),
				edge("sb", "s", "b"),
				edge("at", "a", "t"),
				edge("bt", "b", "t"),
			],
			{ source: "s", sink: "t" },
		);
		expect(unit.unitCapacity).toBe(true);
		expect(unit.unitNetwork).toBe(true);

		const nonUnit = analyzeFlowGraphShape(
			[...nodes, node("c")],
			[
				edge("sa", "s", "a"),
				edge("sb", "s", "b"),
				edge("ac", "a", "c"),
				edge("bc", "b", "c"),
				edge("at", "a", "t"),
				edge("bt", "b", "t"),
			],
			{ source: "s", sink: "t" },
		);
		expect(nonUnit.unitCapacity).toBe(true);
		expect(nonUnit.unitNetwork).toBe(false);
	});

	it("recognizes the exact zero-flow start required by classical SSAP", () => {
		const nodes = [node("s"), node("t")];
		const zero = analyzeFlowGraphShape(nodes, [edge("st", "s", "t", "3")]);
		expect(zero.zeroFlowFeasible).toBe(true);

		const lower = {
			...edge("st", "s", "t", "3"),
			lower: "1",
		};
		expect(analyzeFlowGraphShape(nodes, [lower]).zeroFlowFeasible).toBe(false);
		expect(
			analyzeFlowGraphShape(
				[node("s", "1"), node("t", "-1")],
				[edge("st", "s", "t", "3")],
			).zeroFlowFeasible,
		).toBe(false);
	});

	it("reports advanced max-flow admission facts without vacuous readiness", () => {
		const nodes = [node("s"), node("a"), node("t")];
		const connected = [edge("sa", "s", "a", "2"), edge("at", "a", "t")];
		expect(
			analyzeFlowGraphShape(nodes, connected, { source: "s", sink: "t" }),
		).toMatchObject({
			positiveCapacity: true,
			nonEmptyEdges: true,
			zeroCost: true,
			distinctTerminals: true,
			underlyingConnected: true,
		});

		expect(
			analyzeFlowGraphShape(nodes, [edge("zero", "s", "t", "0")])
				.positiveCapacity,
		).toBe(false);
		expect(analyzeFlowGraphShape(nodes, []).nonEmptyEdges).toBe(false);
		expect(
			analyzeFlowGraphShape(nodes, [{ ...edge("priced", "s", "t"), cost: "3" }])
				.zeroCost,
		).toBe(false);
		expect(
			analyzeFlowGraphShape(nodes, connected, { source: "s", sink: "s" })
				.distinctTerminals,
		).toBe(false);
		expect(
			analyzeFlowGraphShape(nodes, [edge("st", "s", "t")]).underlyingConnected,
		).toBe(false);
	});

	it("orients disconnected bipartite components deterministically", () => {
		const nodes = [node("l2"), node("r2"), node("r1"), node("l1")];
		const edges = [edge("first", "l1", "r1"), edge("second", "l2", "r2")];
		const first = directedFlowBipartition(nodes, edges);
		const second = directedFlowBipartition(
			[...nodes].reverse(),
			[...edges].reverse(),
		);

		expect(first?.directionCoherence).toBe(1);
		expect([...(first?.left ?? [])].sort()).toEqual(["l1", "l2"]);
		expect([...(first?.right ?? [])].sort()).toEqual(["r1", "r2"]);
		expect([...(first?.left ?? [])].sort()).toEqual(
			[...(second?.left ?? [])].sort(),
		);
	});

	it("checks balance signs for transportation and never infers a planar embedding", () => {
		const shape = analyzeFlowGraphShape(
			[node("factory", "3"), node("shop", "-3")],
			[edge("route", "factory", "shop", "7")],
		);
		expect(shape).toMatchObject({
			bipartite: true,
			balancedBipartite: true,
			transportationNetwork: true,
			planarEmbedding: "unavailable",
			unitCapacity: false,
		});
	});

	it("uses declared transportation partitions for isolated origins and destinations", () => {
		const nodes = [node("d0", "-1"), node("o0", "1")];
		const partitions = { origins: ["o0"], destinations: ["d0"] };
		const isolated = analyzeFlowGraphShape(nodes, [], undefined, partitions);
		expect(isolated.bipartite).toBe(true);
		expect(isolated.transportationNetwork).toBe(true);

		const routed = analyzeFlowGraphShape(
			nodes,
			[edge("route", "o0", "d0", "1")],
			undefined,
			partitions,
		);
		expect(routed.transportationNetwork).toBe(true);
		expect(
			analyzeFlowGraphShape(
				nodes,
				[edge("reverse", "d0", "o0", "1")],
				undefined,
				partitions,
			).transportationNetwork,
		).toBe(false);
	});

	it("rejects odd cycles, self-loops, and dangling endpoints", () => {
		const triangle = [
			edge("ab", "a", "b"),
			edge("bc", "b", "c"),
			edge("ca", "c", "a"),
		];
		expect(
			analyzeFlowGraphShape([node("a"), node("b"), node("c")], triangle)
				.bipartite,
		).toBe(false);
		const loopShape = analyzeFlowGraphShape(
			[node("a")],
			[edge("loop", "a", "a")],
		);
		expect(loopShape.bipartite).toBe(false);
		expect(loopShape.noSelfLoops).toBe(false);
		expect(analyzeFlowGraphShape([node("a")], []).noSelfLoops).toBe(true);
		expect(
			analyzeFlowGraphShape([node("a")], [edge("bad", "a", "missing")])
				.unitCapacity,
		).toBe(false);
	});

	it("checks positive-width strong connectivity in both directions", () => {
		const nodes = [node("a"), node("b"), node("c")];
		const ab = edge("ab", "a", "b", "5");
		const bc = edge("bc", "b", "c", "5");
		const cycle = [ab, bc, edge("ca", "c", "a", "5")];
		expect(analyzeFlowGraphShape(nodes, cycle).stronglyConnected).toBe(true);
		expect(
			analyzeFlowGraphShape(nodes, cycle.slice(0, 2)).stronglyConnected,
		).toBe(false);

		const zeroWidthReturn = {
			...edge("ca", "c", "a", "2"),
			lower: "2",
		};
		expect(
			analyzeFlowGraphShape(nodes, [ab, bc, zeroWidthReturn]).stronglyConnected,
		).toBe(false);
	});

	it("checks nonbinding widths after shifting lower-bound divergence", () => {
		const nodes = [node("a", "5"), node("b", "-5")];
		const wideForward = {
			...edge("ab", "a", "b", "7"),
			lower: "2",
		};
		const wideReturn = edge("ba", "b", "a", "3");
		const shape = analyzeFlowGraphShape(nodes, [wideForward, wideReturn]);
		// The lower bound already transports two units, so the shifted positive
		// supply is three and both residual widths are nonbinding.
		expect(shape.nonbindingTransshipmentCapacities).toBe(true);

		const narrowReturn = edge("ba", "b", "a", "2");
		expect(
			analyzeFlowGraphShape(nodes, [wideForward, narrowReturn])
				.nonbindingTransshipmentCapacities,
		).toBe(false);
	});

	it("includes fixed terminal flow in the nonbinding-capacity requirement", () => {
		const nodes = [node("s"), node("t")];
		const terminals = { source: "s", sink: "t", requiredFlow: 5n };
		expect(
			analyzeFlowGraphShape(nodes, [edge("narrow", "s", "t", "1")], terminals)
				.nonbindingTransshipmentCapacities,
		).toBe(false);
		expect(
			analyzeFlowGraphShape(nodes, [edge("wide", "s", "t", "5")], terminals)
				.nonbindingTransshipmentCapacities,
		).toBe(true);
		expect(
			analyzeFlowGraphShape(nodes, [edge("zero", "s", "t", "0")])
				.nonbindingTransshipmentCapacities,
		).toBe(true);
	});

	it("cancels a lower-bound self-loop before measuring required width", () => {
		const nodes = [node("s", "5"), node("t", "-5")];
		const loop = {
			...edge("loop", "s", "s", "7"),
			lower: "2",
		};
		expect(
			analyzeFlowGraphShape(nodes, [loop, edge("st", "s", "t", "5")])
				.nonbindingTransshipmentCapacities,
		).toBe(true);
	});

	it("detects negative cycles in every lower-bound residual component", () => {
		const nodes = [node("a"), node("b"), node("isolated")];
		const negativeCycle = [
			{ ...edge("ab", "a", "b"), cost: "-2" },
			{ ...edge("ba", "b", "a"), cost: "1" },
		];
		expect(
			analyzeFlowGraphShape(nodes, negativeCycle)
				.lowerBoundResidualNegativeCycle,
		).toBe("present");
		expect(
			analyzeFlowGraphShape(nodes, negativeCycle.slice(0, 1))
				.lowerBoundResidualNegativeCycle,
		).toBe("absent");

		const fixedPositive = {
			...edge("fixed-positive", "a", "b", "1"),
			lower: "1",
			cost: "2",
		};
		const returnToForward = {
			...edge("forward", "a", "b", "1"),
			cost: "1",
		};
		expect(
			analyzeFlowGraphShape(nodes, [fixedPositive, returnToForward])
				.lowerBoundResidualNegativeCycle,
		).toBe("absent");
	});
});
