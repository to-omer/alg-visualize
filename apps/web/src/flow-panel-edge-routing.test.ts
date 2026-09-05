import { describe, expect, it } from "vitest";

import { buildFlowPanelEdgeRoutes } from "./flow-panel-edge-routing";

const options = {
	width: 940,
	height: 370,
	paddingX: 70,
	paddingY: 48,
};

describe("flow algorithm-panel edge routing", () => {
	it("separates opposite and parallel directions with stable lane identity", () => {
		const routes = buildFlowPanelEdgeRoutes(
			[
				{ id: "forward-a", from: "u", to: "v" },
				{ id: "forward-b", from: "u", to: "v" },
				{ id: "reverse", from: "v", to: "u" },
			],
			new Map([
				["u", { x: 100, y: 185 }],
				["v", { x: 840, y: 185 }],
			]),
			options,
		);
		expect(new Set([...routes.values()].map((route) => route.d)).size).toBe(3);
		expect(routes.get("forward-a")?.parallelIndex).toBe(1);
		expect(routes.get("forward-b")?.parallelIndex).toBe(2);
		expect(routes.get("reverse")?.parallelIndex).toBe(1);
	});

	it("keeps dense self-loop controls and labels inside every corner", () => {
		for (const center of [
			{ x: 92, y: 68 },
			{ x: 848, y: 68 },
			{ x: 92, y: 302 },
			{ x: 848, y: 302 },
		]) {
			const routes = buildFlowPanelEdgeRoutes(
				Array.from({ length: 64 }, (_, index) => ({
					id: `loop-${index.toString().padStart(2, "0")}`,
					from: "v",
					to: "v",
				})),
				new Map([["v", center]]),
				options,
			);
			expect(new Set([...routes.values()].map((route) => route.d)).size).toBe(
				64,
			);
			for (const route of routes.values()) {
				const values = route.d.match(/-?\d+(?:\.\d+)?/gu)?.map(Number) ?? [];
				for (let index = 0; index < values.length; index += 2) {
					expect(values[index]).toBeGreaterThanOrEqual(0);
					expect(values[index]).toBeLessThanOrEqual(options.width);
					expect(values[index + 1]).toBeGreaterThanOrEqual(0);
					expect(values[index + 1]).toBeLessThanOrEqual(options.height);
				}
				expect(route.label.x).toBeGreaterThanOrEqual(options.paddingX);
				expect(route.label.x).toBeLessThanOrEqual(
					options.width - options.paddingX,
				);
				expect(route.label.y).toBeGreaterThanOrEqual(options.paddingY);
				expect(route.label.y).toBeLessThanOrEqual(
					options.height - options.paddingY,
				);
			}
		}
	});

	it("fits every admitted parallel lane without collapsing paths at a boundary", () => {
		for (const [from, to] of [
			[
				{ x: 100, y: 68 },
				{ x: 840, y: 68 },
			],
			[
				{ x: 100, y: 302 },
				{ x: 840, y: 302 },
			],
			[
				{ x: 92, y: 68 },
				{ x: 92, y: 302 },
			],
			[
				{ x: 848, y: 68 },
				{ x: 848, y: 302 },
			],
		] as const) {
			const edges = [
				...Array.from({ length: 7 }, (_, index) => ({
					id: `forward-${index}`,
					from: "u",
					to: "v",
				})),
				{ id: "reverse", from: "v", to: "u" },
			];
			const routes = buildFlowPanelEdgeRoutes(
				edges,
				new Map<string, { x: number; y: number }>([
					["u", from],
					["v", to],
				]),
				options,
			);
			expect(routes.size).toBe(edges.length);
			expect(new Set([...routes.values()].map((route) => route.d)).size).toBe(
				edges.length,
			);
			for (const route of routes.values()) {
				const values = route.d.match(/-?\d+(?:\.\d+)?/gu)?.map(Number) ?? [];
				for (let index = 0; index < values.length; index += 2) {
					expect(values[index]).toBeGreaterThanOrEqual(0);
					expect(values[index]).toBeLessThanOrEqual(options.width);
					expect(values[index + 1]).toBeGreaterThanOrEqual(0);
					expect(values[index + 1]).toBeLessThanOrEqual(options.height);
				}
				expect(route.label.x).toBeGreaterThanOrEqual(options.paddingX + 70);
				expect(route.label.x).toBeLessThanOrEqual(
					options.width - options.paddingX - 70,
				);
				expect(route.label.y).toBeGreaterThanOrEqual(options.paddingY + 20);
				expect(route.label.y).toBeLessThanOrEqual(
					options.height - options.paddingY - 20,
				);
				if (route.labelLeader !== undefined) {
					expect(
						Math.hypot(
							route.labelLeader.from.x - route.labelLeader.to.x,
							route.labelLeader.from.y - route.labelLeader.to.y,
						),
					).toBeGreaterThanOrEqual(4);
				}
			}
			const routeList = [...routes.values()];
			for (let left = 0; left < routeList.length; left += 1) {
				for (let right = left + 1; right < routeList.length; right += 1) {
					const first = routeList[left]?.label;
					const second = routeList[right]?.label;
					if (first === undefined || second === undefined) continue;
					expect(
						Math.abs(first.x - second.x) >= 144 ||
							Math.abs(first.y - second.y) >= 44,
					).toBe(true);
				}
			}
		}
	});

	it("ends arrow-bearing paths outside the target node disc", () => {
		const target = { x: 840, y: 185 };
		const route = buildFlowPanelEdgeRoutes(
			[{ id: "uv", from: "u", to: "v" }],
			new Map([
				["u", { x: 100, y: 185 }],
				["v", target],
			]),
			options,
		).get("uv");
		expect(route).toBeDefined();
		const values = route?.d.match(/-?\d+(?:\.\d+)?/gu)?.map(Number) ?? [];
		const endX = values.at(-2) ?? target.x;
		const endY = values.at(-1) ?? target.y;
		expect(Math.hypot(target.x - endX, target.y - endY)).toBeGreaterThanOrEqual(
			40,
		);
	});

	it("places a dense auxiliary graph's labels without overlap", () => {
		const positions = new Map<string, { x: number; y: number }>([
			["s", { x: 125, y: 205 }],
			["a", { x: 460, y: 77 }],
			["b", { x: 795, y: 205 }],
			["c", { x: 460, y: 333 }],
			["u0", { x: 300, y: 135 }],
			["u1", { x: 620, y: 135 }],
			["u2", { x: 300, y: 275 }],
			["u3", { x: 620, y: 275 }],
			["u4", { x: 460, y: 205 }],
		]);
		const edgeEndpoints = [
			["s", "u0"],
			["u0", "a"],
			["a", "u1"],
			["u1", "b"],
			["s", "u2"],
			["u2", "c"],
			["c", "u3"],
			["u3", "b"],
			["a", "u4"],
			["u4", "c"],
			["u0", "u4"],
			["u4", "u1"],
			["u2", "u4"],
			["u4", "u3"],
			["u1", "u3"],
		] as const;
		const edges = edgeEndpoints.map(([from, to], index) => ({
			id: `arc-${index}`,
			from,
			to,
		}));
		const labelOptions = {
			...options,
			width: 920,
			height: 410,
			paddingX: 38,
			paddingY: 38,
			nodeRadius: 32,
			markerClearance: 12,
			labelWidth: 144,
			labelHeight: 36,
			labelEdgeIds: ["arc-0", "arc-4", "arc-8", "arc-12"],
		};
		const routeMap = buildFlowPanelEdgeRoutes(edges, positions, labelOptions);
		const routes = labelOptions.labelEdgeIds.map((id) => routeMap.get(id));
		expect(routes).toHaveLength(labelOptions.labelEdgeIds.length);
		const collisions: unknown[] = [];
		for (let left = 0; left < routes.length; left += 1) {
			for (let right = left + 1; right < routes.length; right += 1) {
				const first = routes[left]?.label;
				const second = routes[right]?.label;
				if (first === undefined || second === undefined) continue;
				if (
					Math.abs(first.x - second.x) < 148 &&
					Math.abs(first.y - second.y) < 40
				) {
					collisions.push({ left, right, first, second });
				}
			}
		}
		expect(collisions).toEqual([]);
		const visibleBoxes = routes.flatMap((route) =>
			route === undefined
				? []
				: [
						{
							left: route.label.x - 72,
							right: route.label.x + 72,
							top: route.label.y - 18,
							bottom: route.label.y + 18,
						},
					],
		);
		for (const edge of edges) {
			if (labelOptions.labelEdgeIds.includes(edge.id)) continue;
			const route = routeMap.get(edge.id);
			expect(route, edge.id).toBeDefined();
			if (route === undefined) continue;
			const onDemand = {
				left: route.label.x - 72,
				right: route.label.x + 72,
				top: route.label.y - 18,
				bottom: route.label.y + 18,
			};
			for (const visible of visibleBoxes) {
				expect(
					onDemand.right + 4 <= visible.left ||
						visible.right + 4 <= onDemand.left ||
						onDemand.bottom + 4 <= visible.top ||
						visible.bottom + 4 <= onDemand.top,
					edge.id,
				).toBe(true);
			}
		}
	});

	it("keeps Unicode edge identities on deterministic lanes", () => {
		const edges = ["z", "ä", "é", "e\u0301", "😀"].map((id) => ({
			id,
			from: "u",
			to: "v",
		}));
		const positions = new Map([
			["u", { x: 100, y: 185 }],
			["v", { x: 840, y: 185 }],
		]);
		const forward = buildFlowPanelEdgeRoutes(edges, positions, options);
		const reversed = buildFlowPanelEdgeRoutes(
			[...edges].reverse(),
			positions,
			options,
		);
		for (const edge of edges) {
			expect(reversed.get(edge.id)).toEqual(forward.get(edge.id));
		}
	});
});
