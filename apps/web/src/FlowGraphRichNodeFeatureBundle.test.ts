import { describe, expect, it } from "vitest";
import { placeFlowNodeTraceCallouts } from "./FlowGraphRichNodeFeatureBundle";

describe("placeFlowNodeTraceCallouts", () => {
	it("keeps dense electrical labels in bounds and gives each a leader", () => {
		const inputs = [
			["s", 58, 270],
			["a", 260, 180],
			["b", 460, 180],
			["c", 260, 360],
			["d", 460, 360],
			["t", 842, 270],
		].map(([id, x, y], ordinal) => ({
			id: String(id),
			position: { x: Number(x), y: Number(y) },
			label: "y -1.23e-5 · γ 9.88e4 · S*",
			ordinal,
		}));
		const placements = placeFlowNodeTraceCallouts(inputs);
		expect(placements.size).toBe(inputs.length);
		const boxes: {
			left: number;
			right: number;
			top: number;
			bottom: number;
		}[] = [];
		for (const input of inputs) {
			const placement = placements.get(input.id);
			expect(placement).toBeDefined();
			if (placement === undefined) continue;
			const absoluteX = input.position.x + placement.x;
			const absoluteY = input.position.y + placement.y;
			expect(absoluteX).toBeGreaterThanOrEqual(12);
			expect(absoluteX).toBeLessThanOrEqual(888);
			expect(absoluteY).toBeGreaterThanOrEqual(23);
			expect(absoluteY).toBeLessThanOrEqual(523);
			expect(
				Math.hypot(placement.leaderStart.x, placement.leaderStart.y),
			).toBeCloseTo(30);
			const width = Math.min(300, Math.max(70, input.label.length * 11.5));
			boxes.push({
				left: absoluteX - width / 2 - 4,
				right: absoluteX + width / 2 + 4,
				top: absoluteY - 24,
				bottom: absoluteY + 8,
			});
		}
		for (let left = 0; left < boxes.length; left += 1) {
			for (let right = left + 1; right < boxes.length; right += 1) {
				const leftBox = boxes[left];
				const rightBox = boxes[right];
				if (leftBox === undefined || rightBox === undefined) {
					throw new Error("trace callout collision fixture is incomplete");
				}
				const width = Math.max(
					0,
					Math.min(leftBox.right, rightBox.right) -
						Math.max(leftBox.left, rightBox.left),
				);
				const height = Math.max(
					0,
					Math.min(leftBox.bottom, rightBox.bottom) -
						Math.max(leftBox.top, rightBox.top),
				);
				expect(width * height).toBe(0);
			}
		}
	});

	it("is deterministic for an identical visible annotation set", () => {
		const initial = [
			{
				id: "a",
				position: { x: 260, y: 270 },
				label: "d 8 · #2",
				ordinal: 2,
			},
			{
				id: "b",
				position: { x: 360, y: 270 },
				label: "d 9 · #3",
				ordinal: 3,
			},
		] as const;
		const before = placeFlowNodeTraceCallouts(initial);
		const after = placeFlowNodeTraceCallouts([...initial]);
		expect(after).toEqual(before);
	});

	it("routes a callout around a reserved edge annotation", () => {
		const input = {
			id: "o1",
			position: { x: 450, y: 270 },
			label: "u 8 · #0",
			ordinal: 0,
		};
		const reserved = {
			left: 400,
			right: 500,
			top: 205,
			bottom: 240,
		};
		const placement = placeFlowNodeTraceCallouts([input], {
			reserved: [reserved],
		}).get(input.id);
		expect(placement).toBeDefined();
		if (placement === undefined) return;
		const width = Math.min(300, Math.max(70, input.label.length * 11.5));
		const x = input.position.x + placement.x;
		const y = input.position.y + placement.y;
		const callout = {
			left: x - width / 2 - 4,
			right: x + width / 2 + 4,
			top: y - 24,
			bottom: y + 8,
		};
		expect(overlapAreaForTest(callout, reserved)).toBe(0);
	});

	it("routes a callout around graph nodes that have no callout of their own", () => {
		const input = {
			id: "o1",
			position: { x: 450, y: 270 },
			label: "u 8 · #0",
			ordinal: 0,
		};
		const neighbor = { x: 450, y: 204 };
		const placement = placeFlowNodeTraceCallouts([input], {
			nodePositions: [input.position, neighbor],
		}).get(input.id);
		expect(placement).toBeDefined();
		if (placement === undefined) return;
		const width = Math.min(300, Math.max(70, input.label.length * 11.5));
		const x = input.position.x + placement.x;
		const y = input.position.y + placement.y;
		const callout = {
			left: x - width / 2 - 4,
			right: x + width / 2 + 4,
			top: y - 24,
			bottom: y + 8,
		};
		const neighborBounds = {
			left: neighbor.x - 34,
			right: neighbor.x + 34,
			top: neighbor.y - 34,
			bottom: neighbor.y + 34,
		};
		expect(overlapAreaForTest(callout, neighborBounds)).toBe(0);
	});

	it("never overlaps simultaneous callouts on the dense 40-node grid", () => {
		const inputs = [
			["v0001", 155, 110],
			["v0003", 355, 110],
			["v0005", 555, 110],
			["v0011", 355, 210],
			["v0013", 555, 210],
			["v0021", 355, 310],
		].map(([id, x, y], ordinal) => ({
			id: String(id),
			position: { x: Number(x), y: Number(y) },
			label: `C ${id} · Φ -120 · K · T`,
			ordinal,
		}));
		const placements = placeFlowNodeTraceCallouts(inputs);
		expect(placements.size).toBe(inputs.length);
		const boxes = inputs.flatMap((input) => {
			const placement = placements.get(input.id);
			if (placement === undefined) return [];
			const width = Math.min(300, Math.max(70, input.label.length * 11.5));
			const x = input.position.x + placement.x;
			const y = input.position.y + placement.y;
			return [
				{
					left: x - width / 2 - 4,
					right: x + width / 2 + 4,
					top: y - 24,
					bottom: y + 8,
				},
			];
		});
		for (let left = 0; left < boxes.length; left += 1) {
			for (let right = left + 1; right < boxes.length; right += 1) {
				const a = boxes[left];
				const b = boxes[right];
				if (a === undefined || b === undefined) continue;
				expect(overlapAreaForTest(a, b)).toBe(0);
			}
		}
	});
});

function overlapAreaForTest(
	left: Readonly<{ left: number; right: number; top: number; bottom: number }>,
	right: Readonly<{ left: number; right: number; top: number; bottom: number }>,
): number {
	return (
		Math.max(
			0,
			Math.min(left.right, right.right) - Math.max(left.left, right.left),
		) *
		Math.max(
			0,
			Math.min(left.bottom, right.bottom) - Math.max(left.top, right.top),
		)
	);
}
