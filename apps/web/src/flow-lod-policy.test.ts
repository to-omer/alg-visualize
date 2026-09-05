import { describe, expect, it } from "vitest";
import {
	chooseFlowCanvasLod,
	constrainFlowCanvasLodToBaseline,
	constrainFlowCanvasLodToRenderLimits,
} from "./flow-lod-policy";

const viewport = { width: 900, height: 540 };

describe("flow LOD policy", () => {
	it("uses entity capacity at the reference viewport and zoom", () => {
		expect(
			chooseFlowCanvasLod({
				current: undefined,
				zoom: 1,
				viewport,
				entityCounts: { nodes: 50, edges: 64 },
			}),
		).toBe("detail");
		expect(
			chooseFlowCanvasLod({
				current: undefined,
				zoom: 1,
				viewport,
				entityCounts: { nodes: 51, edges: 65 },
			}),
		).toBe("structure");
		expect(
			chooseFlowCanvasLod({
				current: undefined,
				zoom: 1,
				viewport,
				entityCounts: { nodes: 601, edges: 1_201 },
			}),
		).toBe("overview");
	});

	it("uses viewport area and zoom to estimate visible density", () => {
		const counts = { nodes: 55, edges: 70 };
		expect(
			chooseFlowCanvasLod({
				current: undefined,
				zoom: 1,
				viewport: { width: 450, height: 270 },
				entityCounts: counts,
			}),
		).toBe("structure");
		expect(
			chooseFlowCanvasLod({
				current: undefined,
				zoom: 2,
				viewport,
				entityCounts: counts,
			}),
		).toBe("detail");
	});

	it("excludes xMidYMid meet letterboxes from viewport capacity", () => {
		const counts = { nodes: 55, edges: 70 };
		for (const letterboxed of [
			{ width: 900, height: 900 },
			{ width: 1_800, height: 540 },
		]) {
			expect(
				chooseFlowCanvasLod({
					current: undefined,
					zoom: 1,
					viewport: letterboxed,
					entityCounts: counts,
				}),
			).toBe("structure");
		}
		expect(
			chooseFlowCanvasLod({
				current: undefined,
				zoom: 1,
				viewport: { width: 1_800, height: 1_080 },
				entityCounts: counts,
			}),
		).toBe("detail");
	});

	it("holds the current level inside both hysteresis bands", () => {
		const detailBoundary = { nodes: 55, edges: 70 };
		expect(
			chooseFlowCanvasLod({
				current: "detail",
				zoom: 1,
				viewport,
				entityCounts: detailBoundary,
			}),
		).toBe("detail");
		expect(
			chooseFlowCanvasLod({
				current: "structure",
				zoom: 1,
				viewport,
				entityCounts: detailBoundary,
			}),
		).toBe("structure");

		const overviewBoundary = { nodes: 660, edges: 1_320 };
		expect(
			chooseFlowCanvasLod({
				current: "structure",
				zoom: 1,
				viewport,
				entityCounts: overviewBoundary,
			}),
		).toBe("structure");
		expect(
			chooseFlowCanvasLod({
				current: "overview",
				zoom: 1,
				viewport,
				entityCounts: overviewBoundary,
			}),
		).toBe("overview");
	});

	it("crosses a boundary only after leaving its hysteresis band", () => {
		expect(
			chooseFlowCanvasLod({
				current: "detail",
				zoom: 1,
				viewport,
				entityCounts: { nodes: 58, edges: 74 },
			}),
		).toBe("structure");
		expect(
			chooseFlowCanvasLod({
				current: "structure",
				zoom: 1,
				viewport,
				entityCounts: { nodes: 42, edges: 54 },
			}),
		).toBe("detail");
		expect(
			chooseFlowCanvasLod({
				current: "overview",
				zoom: 1,
				viewport,
				entityCounts: { nodes: 509, edges: 1_019 },
			}),
		).toBe("structure");
	});

	it("keeps absolute render limits even at extreme zoom", () => {
		expect(
			chooseFlowCanvasLod({
				current: "detail",
				zoom: 100,
				viewport,
				entityCounts: { nodes: 59, edges: 75 },
			}),
		).toBe("structure");
		expect(
			chooseFlowCanvasLod({
				current: "structure",
				zoom: 100,
				viewport,
				entityCounts: { nodes: 2_501, edges: 12_001 },
			}),
		).toBe("overview");
	});

	it("constrains stale requested levels before render-plan allocation", () => {
		expect(
			constrainFlowCanvasLodToRenderLimits("detail", {
				nodes: 59,
				edges: 75,
			}),
		).toBe("structure");
		expect(
			constrainFlowCanvasLodToRenderLimits("detail", {
				nodes: 2_501,
				edges: 12_001,
			}),
		).toBe("overview");
		expect(
			constrainFlowCanvasLodToRenderLimits("structure", {
				nodes: 2_501,
				edges: 12_001,
			}),
		).toBe("overview");
	});

	it("never promotes a scene above its spatial-safety baseline", () => {
		expect(constrainFlowCanvasLodToBaseline("detail", "overview")).toBe(
			"overview",
		);
		expect(constrainFlowCanvasLodToBaseline("structure", "overview")).toBe(
			"overview",
		);
		expect(constrainFlowCanvasLodToBaseline("overview", "structure")).toBe(
			"overview",
		);
		expect(constrainFlowCanvasLodToBaseline("structure", "detail")).toBe(
			"structure",
		);
	});

	it("fails closed for invalid measurements", () => {
		expect(
			chooseFlowCanvasLod({
				current: undefined,
				zoom: Number.NaN,
				viewport,
				entityCounts: { nodes: 1, edges: 1 },
			}),
		).toBe("overview");
		expect(
			chooseFlowCanvasLod({
				current: "structure",
				zoom: 0,
				viewport,
				entityCounts: { nodes: -1, edges: 1 },
			}),
		).toBe("overview");
	});
});
