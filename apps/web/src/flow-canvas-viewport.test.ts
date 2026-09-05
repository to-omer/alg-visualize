import { describe, expect, it } from "vitest";
import {
	constrainFlowCanvasViewport,
	fitFlowCanvasViewport,
	flowCanvasWheelDeltaPixels,
	panFlowCanvasViewport,
	pinchFlowCanvasViewport,
	wheelZoomFlowCanvasViewport,
	zoomFlowCanvasViewport,
	zoomInFlowCanvasViewport,
	zoomOutFlowCanvasViewport,
} from "./flow-canvas-viewport";

const screen = { width: 900, height: 540 };

describe("flow canvas viewport", () => {
	it("fits the complete graph and uses a centered button zoom", () => {
		const fit = fitFlowCanvasViewport();
		expect(fit).toEqual({
			viewBox: { x: 0, y: 0, width: 900, height: 540 },
			zoom: 1,
		});
		const zoomed = zoomInFlowCanvasViewport(fit, screen);
		expect(zoomed).toEqual({
			viewBox: { x: 90, y: 54, width: 720, height: 432 },
			zoom: 1.25,
		});
		expect(zoomOutFlowCanvasViewport(zoomed, screen)).toEqual(fit);
	});

	it("keeps the world point under the wheel anchor and clamps maximum zoom", () => {
		const fit = fitFlowCanvasViewport();
		const topLeft = zoomFlowCanvasViewport(fit, 2, { x: 0, y: 0 }, screen);
		expect(topLeft).toEqual({
			viewBox: { x: 0, y: 0, width: 450, height: 270 },
			zoom: 2,
		});
		let current = topLeft;
		for (let index = 0; index < 100; index += 1) {
			current = wheelZoomFlowCanvasViewport(
				current,
				-240,
				{ x: 450, y: 270 },
				screen,
			);
		}
		expect(current.zoom).toBe(8);
		expect(current.viewBox).toEqual({
			x: 168.75,
			y: 101.25,
			width: 112.5,
			height: 67.5,
		});
	});

	it("normalizes all standard wheel delta modes and rejects unknown modes", () => {
		expect(flowCanvasWheelDeltaPixels(-120, 0, 540)).toBe(-120);
		expect(flowCanvasWheelDeltaPixels(-3, 1, 540)).toBe(-48);
		expect(flowCanvasWheelDeltaPixels(-1, 2, 540)).toBe(-540);
		expect(flowCanvasWheelDeltaPixels(-1, 3, 540)).toBeNaN();
		expect(flowCanvasWheelDeltaPixels(-1, 0, 0)).toBeNaN();
	});

	it("converts pointer movement to world pan and cannot leave graph bounds", () => {
		const zoomed = zoomFlowCanvasViewport(
			fitFlowCanvasViewport(),
			2,
			{ x: 450, y: 270 },
			screen,
		);
		expect(panFlowCanvasViewport(zoomed, { x: 100, y: 54 }, screen)).toEqual({
			viewBox: { x: 175, y: 108, width: 450, height: 270 },
			zoom: 2,
		});
		expect(
			panFlowCanvasViewport(zoomed, { x: 100_000, y: 100_000 }, screen).viewBox,
		).toEqual({ x: 0, y: 0, width: 450, height: 270 });
		expect(
			panFlowCanvasViewport(zoomed, { x: -100_000, y: -100_000 }, screen)
				.viewBox,
		).toEqual({ x: 450, y: 270, width: 450, height: 270 });
	});

	it("combines two-pointer centroid translation with pinch scaling", () => {
		const pinched = pinchFlowCanvasViewport(
			fitFlowCanvasViewport(),
			[
				{ x: 300, y: 270 },
				{ x: 600, y: 270 },
			],
			[
				{ x: 210, y: 270 },
				{ x: 810, y: 270 },
			],
			screen,
		);
		expect(pinched).toEqual({
			viewBox: { x: 195, y: 135, width: 450, height: 270 },
			zoom: 2,
		});
	});

	it("maps pointer anchors through xMidYMid meet letterboxing", () => {
		const squareScreen = { width: 900, height: 900 };
		const zoomed = zoomFlowCanvasViewport(
			fitFlowCanvasViewport(),
			2,
			{ x: 450, y: 180 },
			squareScreen,
		);
		expect(zoomed).toEqual({
			viewBox: { x: 225, y: 0, width: 450, height: 270 },
			zoom: 2,
		});
		expect(
			panFlowCanvasViewport(zoomed, { x: 0, y: -54 }, squareScreen),
		).toEqual({
			viewBox: { x: 225, y: 27, width: 450, height: 270 },
			zoom: 2,
		});
	});

	it("fails closed on malformed events and repairs malformed persisted state", () => {
		const fit = fitFlowCanvasViewport();
		expect(
			wheelZoomFlowCanvasViewport(fit, Number.NaN, { x: 0, y: 0 }, screen),
		).toBe(fit);
		expect(
			pinchFlowCanvasViewport(
				fit,
				[
					{ x: 4, y: 4 },
					{ x: 4, y: 4 },
				],
				[
					{ x: 2, y: 2 },
					{ x: 6, y: 6 },
				],
				screen,
			),
		).toBe(fit);
		expect(
			constrainFlowCanvasViewport({
				viewBox: {
					x: Number.POSITIVE_INFINITY,
					y: -100,
					width: 1,
					height: 1,
				},
				zoom: 99,
			}),
		).toEqual({
			viewBox: { x: 0, y: 0, width: 112.5, height: 67.5 },
			zoom: 8,
		});
	});

	it("preserves bounds, aspect ratio, and zoom limits across mixed gestures", () => {
		let current = fitFlowCanvasViewport();
		for (let index = 0; index < 40; index += 1) {
			current = zoomFlowCanvasViewport(
				current,
				index % 3 === 0 ? 1.7 : 0.8,
				{ x: (index * 83) % screen.width, y: (index * 47) % screen.height },
				screen,
			);
			current = panFlowCanvasViewport(
				current,
				{ x: index % 2 === 0 ? 97 : -131, y: index % 3 === 0 ? 53 : -71 },
				screen,
			);
			expect(current.zoom).toBeGreaterThanOrEqual(1);
			expect(current.zoom).toBeLessThanOrEqual(8);
			expect(current.viewBox.x).toBeGreaterThanOrEqual(0);
			expect(current.viewBox.y).toBeGreaterThanOrEqual(0);
			expect(current.viewBox.x + current.viewBox.width).toBeLessThanOrEqual(
				900,
			);
			expect(current.viewBox.y + current.viewBox.height).toBeLessThanOrEqual(
				540,
			);
			expect(current.viewBox.width / current.viewBox.height).toBeCloseTo(
				900 / 540,
				12,
			);
		}
	});
});
