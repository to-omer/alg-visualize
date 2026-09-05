import { describe, expect, it } from "vitest";

import {
	buildFlowLayout,
	buildFlowNodePositions,
	FLOW_NODE_MIN_CENTER_SPACING,
} from "./flow-layout";
import type { FlowEdgeV1, FlowNodeV1 } from "./flow-scene";

function node(id: string, x?: string, y?: string, supply = "0"): FlowNodeV1 {
	return {
		id,
		supply,
		...(x === undefined || y === undefined ? {} : { position: { x, y } }),
	};
}

function edge(id: string, from: string, to: string): FlowEdgeV1 {
	return { id, from, to, lower: "0", capacity: "7", cost: "-2" };
}

describe("buildFlowLayout", () => {
	it("gives nodes without declared coordinates distinct deterministic positions", () => {
		const nodes = [node("c"), node("a"), node("b")];
		const first = buildFlowLayout(nodes, []);
		const second = buildFlowLayout([...nodes].reverse(), []);

		expect(
			new Set([...first.positions.values()].map(({ x, y }) => `${x}:${y}`)),
		).toHaveLength(3);
		expect([...first.positions]).toEqual([...second.positions]);
	});

	it("places terminal models in stable source-to-sink layers", () => {
		const nodes = [node("b"), node("t"), node("a"), node("s")];
		const edges = [
			edge("sa", "s", "a"),
			edge("ab", "a", "b"),
			edge("bt", "b", "t"),
		];
		const model = { kind: "max-flow", source: "s", sink: "t" } as const;
		const first = buildFlowLayout(nodes, edges, { model });
		const second = buildFlowLayout([...nodes].reverse(), [...edges].reverse(), {
			model,
		});
		const x = (id: string) => first.positions.get(id)?.x ?? Number.NaN;

		expect(x("s")).toBeLessThan(x("a"));
		expect(x("a")).toBeLessThan(x("b"));
		expect(x("b")).toBeLessThan(x("t"));
		expect([...first.positions]).toEqual([...second.positions]);
	});

	it("packs practical single-lane NETGEN boundaries without overlapping nodes", () => {
		const netgenNodes = (count: number): FlowNodeV1[] => [
			node("s0000", "60", "270"),
			...Array.from({ length: count - 2 }, (_, index) =>
				node(
					`x${index.toString().padStart(4, "0")}`,
					String(150 + index * 10),
					"270",
				),
			),
			node("t0000", "940", "270"),
		];
		const model = {
			kind: "max-flow",
			source: "s0000",
			sink: "t0000",
		} as const;

		for (const count of [2, 20, 21, 58, 59, 79]) {
			const positions = [
				...buildFlowNodePositions(netgenNodes(count), { model }).values(),
			];
			expect(new Set(positions.map(({ x, y }) => `${x}:${y}`))).toHaveLength(
				count,
			);
			let minimum = Number.POSITIVE_INFINITY;
			for (const [index, point] of positions.entries()) {
				for (const other of positions.slice(index + 1)) {
					minimum = Math.min(
						minimum,
						Math.hypot(point.x - other.x, point.y - other.y),
					);
				}
			}
			expect(minimum).toBeGreaterThanOrEqual(FLOW_NODE_MIN_CENTER_SPACING);
		}
	});

	it("packs dense generated max-flow rows regardless of node naming", () => {
		const nodes = [
			node("s", "40", "270"),
			...Array.from({ length: 38 }, (_, index) =>
				node(
					`v${String(index + 1).padStart(4, "0")}`,
					String(61 + index * 21),
					"270",
				),
			),
			node("t", "860", "270"),
		];
		const positions = [
			...buildFlowNodePositions(nodes, {
				model: { kind: "max-flow", source: "s", sink: "t" },
			}).values(),
		];

		expect(new Set(positions.map(({ x, y }) => `${x}:${y}`))).toHaveLength(
			nodes.length,
		);
		for (const [index, point] of positions.entries()) {
			for (const other of positions.slice(index + 1)) {
				expect(
					Math.hypot(point.x - other.x, point.y - other.y),
				).toBeGreaterThanOrEqual(FLOW_NODE_MIN_CENTER_SPACING);
			}
		}
	});

	it("unfolds a small complete DAG instead of projecting every edge onto one line", () => {
		const ids = [
			"s0000",
			"x0000",
			"x0001",
			"x0002",
			"x0003",
			"x0004",
			"x0005",
			"t0000",
		];
		const nodes = ids.map((id, index) =>
			node(id, String(60 + index * 110), "270"),
		);
		const edges = ids.flatMap((from, fromIndex) =>
			ids
				.slice(fromIndex + 1)
				.map((to, offset) =>
					edge(`e-${fromIndex}-${fromIndex + offset + 1}`, from, to),
				),
		);
		const layout = buildFlowLayout(nodes, edges, {
			model: { kind: "max-flow", source: "s0000", sink: "t0000" },
		});
		const positions = ids.map((id) => {
			const position = layout.positions.get(id);
			expect(position).toBeDefined();
			if (position === undefined) throw new Error(`missing position for ${id}`);
			return position;
		});

		expect(layout.routes).toHaveLength(28);
		expect(new Set(positions.map(({ y }) => y)).size).toBeGreaterThanOrEqual(4);
		expect(Math.max(...positions.map(({ y }) => y))).toBeGreaterThan(
			Math.min(...positions.map(({ y }) => y)) + 240,
		);
		for (let first = 0; first < positions.length - 2; first += 1) {
			for (let second = first + 1; second < positions.length - 1; second += 1) {
				for (let third = second + 1; third < positions.length; third += 1) {
					const a = positions[first];
					const b = positions[second];
					const c = positions[third];
					if (a === undefined || b === undefined || c === undefined) {
						throw new Error("complete DAG position index is missing");
					}
					const doubledArea =
						(b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
					expect(Math.abs(doubledArea)).toBeGreaterThan(0.01);
				}
			}
		}
	});

	it("places supply, transit, and demand columns while preserving explicit coordinates", () => {
		const layout = buildFlowLayout(
			[
				node("supply", undefined, undefined, "4"),
				node("transit"),
				node("demand", undefined, undefined, "-4"),
				node("pinned", "333", "123", "2"),
			],
			[],
			{ model: { kind: "transshipment" } },
		);

		expect(layout.positions.get("supply")?.x).toBeLessThan(
			layout.positions.get("transit")?.x ?? 0,
		);
		expect(layout.positions.get("transit")?.x).toBeLessThan(
			layout.positions.get("demand")?.x ?? 0,
		);
		expect(layout.positions.get("pinned")).toEqual({ x: 333, y: 123 });
	});

	it("wraps a practical transshipment transit set without overlapping nodes", () => {
		const nodes = [
			node("supply", undefined, undefined, "25"),
			...Array.from({ length: 25 }, (_, index) =>
				node(`transit-${String(index).padStart(2, "0")}`),
			),
			node("demand", undefined, undefined, "-25"),
		];
		const positions = buildFlowNodePositions(nodes, {
			model: { kind: "transshipment" },
		});
		const transitColumns = new Set(
			nodes
				.filter((candidate) => candidate.id.startsWith("transit-"))
				.flatMap((candidate) => {
					const position = positions.get(candidate.id);
					return position === undefined ? [] : [position.x];
				}),
		);

		expect(transitColumns.size).toBeGreaterThan(1);
		expect(positions.get("supply")?.x).toBeLessThan(
			Math.min(...transitColumns),
		);
		expect(Math.max(...transitColumns)).toBeLessThan(
			positions.get("demand")?.x ?? 0,
		);
		const points = [...positions.values()];
		for (const [index, point] of points.entries()) {
			for (const other of points.slice(index + 1)) {
				expect(
					Math.hypot(point.x - other.x, point.y - other.y),
				).toBeGreaterThanOrEqual(FLOW_NODE_MIN_CENTER_SPACING);
			}
		}
	});

	it("recognizes consistently directed bipartite graphs without flattening directed chains", () => {
		const bipartiteNodes = [node("l2"), node("r1"), node("l1"), node("r2")];
		const bipartiteEdges = [
			edge("l1r1", "l1", "r1"),
			edge("l1r2", "l1", "r2"),
			edge("l2r1", "l2", "r1"),
		];
		const bipartite = buildFlowLayout(bipartiteNodes, bipartiteEdges, {
			model: { kind: "circulation" },
		});
		const leftX = bipartite.positions.get("l1")?.x;
		const rightX = bipartite.positions.get("r1")?.x;
		expect(leftX).toBe(bipartite.positions.get("l2")?.x);
		expect(rightX).toBe(bipartite.positions.get("r2")?.x);
		expect(leftX ?? 0).toBeLessThan(rightX ?? 0);

		const chainNodes = [node("a"), node("b"), node("c"), node("d")];
		const chain = buildFlowLayout(
			chainNodes,
			[edge("ab", "a", "b"), edge("bc", "b", "c"), edge("cd", "c", "d")],
			{ model: { kind: "circulation" } },
		);
		expect(new Set([...chain.positions.values()].map(({ x }) => x)).size).toBe(
			3,
		);
	});

	it("uses declared assignment partitions even when tasks are isolated", () => {
		const nodes = [node("a1"), node("t2"), node("a0"), node("t0"), node("t1")];
		const layout = buildFlowLayout(nodes, [edge("e", "a0", "t0")], {
			model: {
				kind: "assignment",
				agents: ["a0", "a1"],
				tasks: ["t0", "t1", "t2"],
				objective: "minimize",
			},
		});
		expect(layout.positions.get("a0")?.x).toBe(layout.positions.get("a1")?.x);
		expect(layout.positions.get("t0")?.x).toBe(layout.positions.get("t1")?.x);
		expect(layout.positions.get("t1")?.x).toBe(layout.positions.get("t2")?.x);
		expect(layout.positions.get("a0")?.x ?? 0).toBeLessThan(
			layout.positions.get("t2")?.x ?? 0,
		);
	});

	it("assigns unique stable lanes to parallel and opposite original edges", () => {
		const nodes = [node("s", "100", "270"), node("t", "800", "270")];
		const edges = [
			edge("forward-b", "s", "t"),
			edge("reverse", "t", "s"),
			edge("forward-a", "s", "t"),
		];
		const first = buildFlowLayout(nodes, edges);
		const second = buildFlowLayout(nodes, [...edges].reverse());
		const paths = [...first.routes.values()].map((route) => route.path);

		expect(new Set(paths)).toHaveLength(3);
		expect([...first.routes]).toEqual([...second.routes]);
		expect(first.routes.get("forward-a")?.path).not.toBe(
			first.routes.get("reverse")?.reversePath,
		);
		expect(first.routes.get("forward-a")).toMatchObject({
			parallelIndex: 1,
			parallelCount: 2,
		});
		expect(first.routes.get("forward-b")).toMatchObject({
			parallelIndex: 2,
			parallelCount: 2,
		});
		expect(first.routes.get("reverse")).toMatchObject({
			parallelIndex: 1,
			parallelCount: 1,
		});
		for (const route of first.routes.values()) {
			expect(Number.isFinite(route.labelAnchor.x)).toBe(true);
			expect(Number.isFinite(route.labelAnchor.y)).toBe(true);
		}
	});

	it("keeps five parallel lane tokens individually readable", () => {
		const ids = ["p1", "p2", "p3", "p4", "p5"];
		const layout = buildFlowLayout(
			[node("s", "100", "270"), node("t", "800", "270")],
			ids.map((id) => edge(id, "s", "t")),
		);
		const routes = ids.map((id) => layout.routes.get(id));

		expect(routes.every((route) => route !== undefined)).toBe(true);
		expect(new Set(routes.map((route) => route?.path))).toHaveLength(5);
		for (let left = 0; left < routes.length; left += 1) {
			for (let right = left + 1; right < routes.length; right += 1) {
				const a = routes[left]?.laneToken;
				const b = routes[right]?.laneToken;
				if (a === undefined || b === undefined) continue;
				expect(Math.hypot(a.x - b.x, a.y - b.y)).toBeGreaterThanOrEqual(18);
			}
		}
	});

	it("keeps lane identity tokens stable when unrelated label metrics change", () => {
		const graphNodes = [
			node("s", "100", "270"),
			node("t", "800", "270"),
			node("a", "300", "90"),
			node("b", "600", "450"),
		];
		const graphEdges = [
			edge("p1", "s", "t"),
			edge("p2", "s", "t"),
			edge("p3", "s", "t"),
			edge("p4", "s", "t"),
			edge("crossing", "a", "b"),
		];
		const baseline = buildFlowLayout(graphNodes, graphEdges);
		const widened = buildFlowLayout(graphNodes, graphEdges, {
			labelMetrics: new Map([
				["crossing", { widthAddition: 700, height: 24, yOffset: 0 }],
			]),
		});

		expect(widened.routes.get("p4")?.laneToken).toEqual(
			baseline.routes.get("p4")?.laneToken,
		);
		expect(widened.routes.get("p4")?.laneTokenAngle).toBe(
			baseline.routes.get("p4")?.laneTokenAngle,
		);
	});

	it("bounds dense parallel route spread and keeps every midpoint in view", () => {
		for (const count of [9, 10, 11, 64]) {
			const layout = buildFlowLayout(
				[node("s", "100", "270"), node("t", "800", "270")],
				Array.from({ length: count }, (_, index) =>
					edge(`p${index.toString().padStart(2, "0")}`, "s", "t"),
				),
			);
			const midpoints = [...layout.routes.values()].map(
				(route) => route.routeMidpoint,
			);
			const spread =
				Math.max(...midpoints.map(({ y }) => y)) -
				Math.min(...midpoints.map(({ y }) => y));
			expect(spread, `${count} lane spread`).toBeLessThanOrEqual(160.001);
			expect(
				midpoints.every(
					(point) =>
						point.x >= 0 && point.x <= 900 && point.y >= 0 && point.y <= 540,
				),
				`${count} lane midpoint bounds`,
			).toBe(true);
		}
	});

	it("fits dense parallel curves inside every viewbox boundary", () => {
		const placements = [
			[node("s", "100", "68"), node("t", "800", "68")],
			[node("s", "100", "472"), node("t", "800", "472")],
			[node("s", "68", "100"), node("t", "68", "440")],
			[node("s", "832", "100"), node("t", "832", "440")],
			[node("s", "68", "68"), node("t", "832", "472")],
		] as const;
		for (const graphNodes of placements) {
			for (const count of [13, 64]) {
				const graphEdges = Array.from({ length: count }, (_, index) =>
					edge(`p${index.toString().padStart(2, "0")}`, "s", "t"),
				);
				if (count === 13) {
					graphEdges.push(
						...Array.from({ length: count }, (_, index) =>
							edge(`r${index.toString().padStart(2, "0")}`, "t", "s"),
						),
					);
				}
				const layout = buildFlowLayout(graphNodes, graphEdges);
				for (const route of layout.routes.values()) {
					const coordinates = route.path.match(/-?\d+(?:\.\d+)?/g)?.map(Number);
					expect(coordinates).toHaveLength(6);
					for (let index = 0; index < 6; index += 2) {
						expect(
							coordinates?.[index],
							`${count} lane x`,
						).toBeGreaterThanOrEqual(16);
						expect(coordinates?.[index], `${count} lane x`).toBeLessThanOrEqual(
							884,
						);
						expect(
							coordinates?.[index + 1],
							`${count} lane y`,
						).toBeGreaterThanOrEqual(16);
						expect(
							coordinates?.[index + 1],
							`${count} lane y`,
						).toBeLessThanOrEqual(524);
					}
				}
			}
		}
	});

	it("fans dense self-loops inward without clipping at viewbox corners", () => {
		const placements = [
			node("v", "68", "68"),
			node("v", "832", "68"),
			node("v", "68", "472"),
			node("v", "832", "472"),
		] as const;
		for (const graphNode of placements) {
			for (const count of [4, 9, 10, 13, 64]) {
				const layout = buildFlowLayout(
					[graphNode],
					Array.from({ length: count }, (_, index) =>
						edge(`loop-${index.toString().padStart(2, "0")}`, "v", "v"),
					),
				);
				const routes = [...layout.routes.values()];
				expect(new Set(routes.map((route) => route.path)).size).toBe(count);
				for (const route of routes) {
					const coordinates = route.path.match(/-?\d+(?:\.\d+)?/g)?.map(Number);
					expect(coordinates).toHaveLength(8);
					for (let index = 0; index < 8; index += 2) {
						expect(
							coordinates?.[index],
							`${count} loop x`,
						).toBeGreaterThanOrEqual(16);
						expect(coordinates?.[index], `${count} loop x`).toBeLessThanOrEqual(
							884,
						);
						expect(
							coordinates?.[index + 1],
							`${count} loop y`,
						).toBeGreaterThanOrEqual(16);
						expect(
							coordinates?.[index + 1],
							`${count} loop y`,
						).toBeLessThanOrEqual(524);
					}
					for (const point of [
						route.routeMidpoint,
						route.laneToken,
						route.label,
					]) {
						expect(point.x).toBeGreaterThanOrEqual(0);
						expect(point.x).toBeLessThanOrEqual(900);
						expect(point.y).toBeGreaterThanOrEqual(0);
						expect(point.y).toBeLessThanOrEqual(540);
					}
				}
			}
		}
	});

	it("gives on-demand labels a visible leader outside the preplaced label set", () => {
		const layout = buildFlowLayout(
			[node("s", "100", "270"), node("t", "800", "270")],
			Array.from({ length: 13 }, (_, index) =>
				edge(`p${index.toString().padStart(2, "0")}`, "s", "t"),
			),
			{ labelEdgeIds: new Set() },
		);
		for (const route of layout.routes.values()) {
			const renderedLabel = {
				x: route.label.x,
				y: route.label.y + route.labelYOffset,
			};
			expect(
				Math.hypot(
					renderedLabel.x - route.labelAnchor.x,
					renderedLabel.y - route.labelAnchor.y,
				),
			).toBeGreaterThanOrEqual(37.999);
		}
	});

	it("keeps vertically shifted Orlin annotations visibly connected to their route", () => {
		const edges = [
			edge("a", "s", "m"),
			edge("b", "m", "t"),
			edge("expensive", "s", "t"),
			edge("zz-pad-edge-00003", "s", "zz-pad-node-00003"),
		];
		const layout = buildFlowLayout(
			[
				node("s", undefined, undefined, "3"),
				node("m"),
				node("t", undefined, undefined, "-3"),
				node("zz-pad-node-00003"),
			],
			edges,
			{
				model: { kind: "transshipment" },
				labelEdgeIds: new Set(edges.map(({ id }) => id)),
				labelPriorityEdgeIds: ["zz-pad-edge-00003"],
				labelMetrics: new Map(
					edges.map(({ id }) => [
						id,
						{ widthAddition: 180, height: 35, yOffset: 32 },
					]),
				),
			},
		);
		for (const route of layout.routes.values()) {
			const renderedLabel = {
				x: route.label.x,
				y: route.label.y + route.labelYOffset,
			};
			expect(
				Math.hypot(
					renderedLabel.x - route.labelAnchor.x,
					renderedLabel.y - route.labelAnchor.y,
				),
				`${route.edgeId} annotation leader`,
			).toBeGreaterThanOrEqual(29.999);
		}
	});

	it("never places dense parallel labels directly on their routes", () => {
		const edges = Array.from({ length: 13 }, (_, index) =>
			edge(`p${index.toString().padStart(2, "0")}`, "s", "t"),
		);
		const layout = buildFlowLayout(
			[node("s", "100", "270"), node("t", "800", "270")],
			edges,
			{
				labelMetrics: new Map(
					edges.map((item) => [
						item.id,
						{ widthAddition: 104, height: 36, yOffset: 0 },
					]),
				),
			},
		);
		for (const route of layout.routes.values()) {
			const renderedLabel = {
				x: route.label.x,
				y: route.label.y + route.labelYOffset,
			};
			expect(
				Math.hypot(
					renderedLabel.x - route.labelAnchor.x,
					renderedLabel.y - route.labelAnchor.y,
				),
			).toBeGreaterThanOrEqual(37.999);
		}
	});

	it("renders individually addressable self-loop curves and reverse residual paths", () => {
		const layout = buildFlowLayout(
			[node("x", "450", "270")],
			[edge("loop-a", "x", "x"), edge("loop-b", "x", "x")],
		);
		const first = layout.routes.get("loop-a");
		const second = layout.routes.get("loop-b");

		expect(first?.selfLoop).toBe(true);
		expect(first?.path).toContain(" C ");
		expect(first?.reversePath).not.toBe(first?.path);
		expect(second?.path).not.toBe(first?.path);
		expect(first).toMatchObject({ parallelIndex: 1, parallelCount: 2 });
		expect(second).toMatchObject({ parallelIndex: 2, parallelCount: 2 });
		expect(Number.isFinite(first?.label.x)).toBe(true);
		expect(Number.isFinite(first?.label.y)).toBe(true);
	});

	it("keeps labels apart on the crossing fixture when a free candidate exists", () => {
		const layout = buildFlowLayout(
			[
				node("a", "290", "130"),
				node("b", "290", "390"),
				node("c", "560", "130"),
				node("d", "560", "390"),
			],
			[edge("ad", "a", "d"), edge("bc", "b", "c")],
		);
		const first = layout.routes.get("ad")?.label;
		const second = layout.routes.get("bc")?.label;
		if (first === undefined || second === undefined)
			throw new Error("missing route");

		expect(Math.hypot(first.x - second.x, first.y - second.y)).toBeGreaterThan(
			35,
		);
	});

	it("includes assignment badges in deterministic label collision boxes", () => {
		const nodes = [node("a0"), node("a1"), node("t0"), node("t1"), node("t2")];
		const edges = [0, 1].flatMap((agent) =>
			[0, 1, 2].map((task) =>
				edge(`e${agent}${task}`, `a${agent}`, `t${task}`),
			),
		);
		const layout = buildFlowLayout(nodes, edges, {
			model: {
				kind: "assignment",
				agents: ["a0", "a1"],
				tasks: ["t0", "t1", "t2"],
				objective: "minimize",
			},
			labelEdgeIds: new Set(edges.map((item) => item.id)),
			labelMetrics: new Map([
				["e01", { widthAddition: 50, height: 24, yOffset: 0 }],
				["e10", { widthAddition: 50, height: 24, yOffset: 0 }],
			]),
		});
		const boxes = edges.map((item) => {
			const route = layout.routes.get(item.id);
			if (route === undefined) throw new Error("missing assignment route");
			const width = route.labelBoxWidth;
			return {
				id: item.id,
				left: route.label.x - width / 2,
				right: route.label.x + width / 2,
				top: route.label.y - 12,
				bottom: route.label.y + 12,
			};
		});
		for (const [index, left] of boxes.entries()) {
			for (const right of boxes.slice(index + 1)) {
				expect(
					left.right + 6 < right.left ||
						right.right + 6 < left.left ||
						left.bottom + 6 < right.top ||
						right.bottom + 6 < left.top,
					`${left.id} overlaps ${right.id}`,
				).toBe(true);
			}
		}
	});

	it("does not cap collision boxes for u64-scale edge values", () => {
		const longEdge = {
			...edge("long", "s", "t"),
			capacity: "18446744073709551615",
			cost: "0",
		};
		const layout = buildFlowLayout([node("s"), node("t")], [longEdge], {
			labelEdgeIds: new Set(["long"]),
		});
		const route = layout.routes.get("long");
		expect(route?.labelWidth).toBeGreaterThan(350);
		expect(route?.labelBoxWidth).toBe(route?.labelWidth);
		expect(route?.labelHeight).toBe(24);
	});

	it("marks labels hidden when a short layered graph has no collision-free slot", () => {
		const ids = ["s", "v1", "v2", "v3", "v4", "v5", "v6", "t"];
		const nodes = ids.map((id, index) =>
			node(id, String(40 + index * 117), "270"),
		);
		const edges = [
			...ids
				.slice(0, -1)
				.map((from, index) =>
					edge(`chain-${index}`, from, ids[index + 1] ?? "t"),
				),
			...ids
				.slice(0, -2)
				.map((from, index) => edge(`shortcut-${index}`, from, "t")),
		];
		const layout = buildFlowLayout(nodes, edges, {
			labelEdgeIds: new Set(edges.map((item) => item.id)),
			model: { kind: "max-flow", source: "s", sink: "t" },
		});
		const visible = [...layout.routes.values()].filter(
			(route) => route.labelCollisionFree,
		);

		expect(visible.length).toBeGreaterThan(0);
		expect(visible.length).toBeLessThan(edges.length);
		for (const [index, left] of visible.entries()) {
			for (const right of visible.slice(index + 1)) {
				expect(
					Math.abs(left.label.y - right.label.y) >= 30 ||
						Math.abs(left.label.x - right.label.x) >=
							(left.labelWidth + right.labelWidth) / 2 + 6,
					`${left.edgeId} overlaps ${right.edgeId}`,
				).toBe(true);
			}
		}
	});

	it("places the current event label before stable context labels", () => {
		const nodes = [node("s"), node("t"), node("x"), node("y")];
		const edges = [
			edge("path", "s", "t"),
			edge("xy", "x", "y"),
			edge("yx", "y", "x"),
		];
		const layout = buildFlowLayout(nodes, edges, {
			labelEdgeIds: new Set(edges.map((item) => item.id)),
			labelPriorityEdgeIds: ["xy"],
			model: {
				kind: "fixed-flow-min-cost",
				source: "s",
				sink: "t",
				required_flow: "1",
			},
		});

		expect(layout.routes.get("xy")?.labelCollisionFree).toBe(true);
	});

	it("reserves an event label fallback before placing context labels", () => {
		const ids = ["s", "v1", "v2", "v3", "v4", "v5", "v6", "t"];
		const nodes = ids.map((id, index) =>
			node(id, String(40 + index * 117), "270"),
		);
		const edges = [
			...ids
				.slice(0, -1)
				.map((from, index) =>
					edge(`chain-${index}`, from, ids[index + 1] ?? "t"),
				),
			...ids
				.slice(0, -2)
				.map((from, index) => edge(`shortcut-${index}`, from, "t")),
		];
		const baseOptions = {
			labelEdgeIds: new Set(edges.map((item) => item.id)),
			model: { kind: "max-flow", source: "s", sink: "t" } as const,
		};
		const baseline = buildFlowLayout(nodes, edges, baseOptions);
		const hidden = [...baseline.routes.values()].find(
			(route) => !route.labelCollisionFree,
		);
		if (hidden === undefined) throw new Error("missing crowded event route");
		const prioritized = buildFlowLayout(nodes, edges, {
			...baseOptions,
			labelPriorityEdgeIds: [hidden.edgeId],
		});
		const eventRoute = prioritized.routes.get(hidden.edgeId);
		if (eventRoute === undefined) throw new Error("missing prioritized route");
		const eventBox = {
			left: eventRoute.label.x - eventRoute.labelBoxWidth / 2,
			right: eventRoute.label.x + eventRoute.labelBoxWidth / 2,
			top:
				eventRoute.label.y +
				eventRoute.labelYOffset -
				eventRoute.labelHeight / 2,
			bottom:
				eventRoute.label.y +
				eventRoute.labelYOffset +
				eventRoute.labelHeight / 2,
		};
		for (const route of prioritized.routes.values()) {
			if (route.edgeId === hidden.edgeId || !route.labelCollisionFree) continue;
			const contextBox = {
				left: route.label.x - route.labelBoxWidth / 2,
				right: route.label.x + route.labelBoxWidth / 2,
				top: route.label.y + route.labelYOffset - route.labelHeight / 2,
				bottom: route.label.y + route.labelYOffset + route.labelHeight / 2,
			};
			expect(
				eventBox.right + 6 < contextBox.left ||
					contextBox.right + 6 < eventBox.left ||
					eventBox.bottom + 6 < contextBox.top ||
					contextBox.bottom + 6 < eventBox.top,
				`${hidden.edgeId} overlaps ${route.edgeId}`,
			).toBe(true);
		}
	});
});
