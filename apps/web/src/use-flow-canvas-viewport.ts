import {
	type MouseEventHandler,
	type PointerEventHandler,
	type RefCallback,
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import {
	type FlowCanvasPoint,
	type FlowCanvasSize,
	type FlowCanvasViewportState,
	fitFlowCanvasViewport,
	flowCanvasWheelDeltaPixels,
	panFlowCanvasViewport,
	pinchFlowCanvasViewport,
	wheelZoomFlowCanvasViewport,
	zoomInFlowCanvasViewport,
	zoomOutFlowCanvasViewport,
} from "./flow-canvas-viewport";

const DEFAULT_CANVAS_SIZE: FlowCanvasSize = Object.freeze({
	width: 1_000,
	height: 540,
});
const POINTER_CAPTURE_THRESHOLD_PX = 3;

export type FlowCanvasSvgBinding = Readonly<{
	ref: RefCallback<SVGSVGElement>;
	viewBox: string;
	"data-flow-zoom": string;
	onPointerDown: PointerEventHandler<SVGSVGElement>;
	onPointerMove: PointerEventHandler<SVGSVGElement>;
	onPointerLeave: PointerEventHandler<SVGSVGElement>;
	onPointerUp: PointerEventHandler<SVGSVGElement>;
	onPointerCancel: PointerEventHandler<SVGSVGElement>;
	onClickCapture: MouseEventHandler<SVGSVGElement>;
}>;

export type FlowCanvasViewportController = Readonly<{
	viewport: FlowCanvasViewportState;
	size: FlowCanvasSize;
	svgBinding: FlowCanvasSvgBinding;
	fit: () => void;
	zoomIn: () => void;
	zoomOut: () => void;
}>;

function localPoint(
	element: SVGSVGElement,
	clientX: number,
	clientY: number,
): FlowCanvasPoint {
	const rect = element.getBoundingClientRect();
	return { x: clientX - rect.left, y: clientY - rect.top };
}

/** Adapts the pure, bounded viewport transforms to one SVG element. */
export function useFlowCanvasViewport(
	resetKey: string,
): FlowCanvasViewportController {
	const [element, setElement] = useState<SVGSVGElement | null>(null);
	const [size, setSize] = useState<FlowCanvasSize>(DEFAULT_CANVAS_SIZE);
	const [viewport, setViewport] = useState(fitFlowCanvasViewport);
	const pointers = useRef(new Map<number, FlowCanvasPoint>());
	const pointerOrigins = useRef(new Map<number, FlowCanvasPoint>());
	const gestureMoved = useRef(false);
	const suppressNextClick = useRef(false);
	const suppressClickTimer = useRef<number | undefined>(undefined);
	const previousResetKey = useRef(resetKey);
	const clearClickSuppression = useCallback(() => {
		if (suppressClickTimer.current !== undefined) {
			window.clearTimeout(suppressClickTimer.current);
			suppressClickTimer.current = undefined;
		}
		suppressNextClick.current = false;
	}, []);

	useEffect(() => {
		if (previousResetKey.current === resetKey) return;
		previousResetKey.current = resetKey;
		setViewport(fitFlowCanvasViewport());
		pointers.current.clear();
		pointerOrigins.current.clear();
		gestureMoved.current = false;
		clearClickSuppression();
	}, [clearClickSuppression, resetKey]);

	useEffect(
		() => () => {
			if (suppressClickTimer.current !== undefined) {
				window.clearTimeout(suppressClickTimer.current);
			}
		},
		[],
	);

	useEffect(() => {
		if (element === null) return;
		const updateSize = () => {
			const rect = element.getBoundingClientRect();
			if (rect.width > 0 && rect.height > 0) {
				setSize({ width: rect.width, height: rect.height });
			}
		};
		updateSize();
		const observer = new ResizeObserver(updateSize);
		observer.observe(element);
		return () => observer.disconnect();
	}, [element]);

	useEffect(() => {
		if (element === null) return;
		const onWheel = (event: WheelEvent) => {
			event.preventDefault();
			const anchor = localPoint(element, event.clientX, event.clientY);
			const deltaY = flowCanvasWheelDeltaPixels(
				event.deltaY,
				event.deltaMode,
				size.height,
			);
			setViewport((current) =>
				wheelZoomFlowCanvasViewport(current, deltaY, anchor, size),
			);
		};
		element.addEventListener("wheel", onWheel, { passive: false });
		return () => element.removeEventListener("wheel", onWheel);
	}, [element, size]);

	const onPointerDown = useCallback<PointerEventHandler<SVGSVGElement>>(
		(event) => {
			if (event.pointerType === "mouse" && event.button !== 0) return;
			if (pointers.current.size === 0) {
				gestureMoved.current = false;
				clearClickSuppression();
			}
			const point = localPoint(
				event.currentTarget,
				event.clientX,
				event.clientY,
			);
			pointers.current.set(event.pointerId, point);
			pointerOrigins.current.set(event.pointerId, point);
			if (pointers.current.size >= 2) gestureMoved.current = true;
		},
		[clearClickSuppression],
	);

	const onPointerMove = useCallback<PointerEventHandler<SVGSVGElement>>(
		(event) => {
			const previousPoint = pointers.current.get(event.pointerId);
			if (previousPoint === undefined) return;
			const currentPoint = localPoint(
				event.currentTarget,
				event.clientX,
				event.clientY,
			);
			const origin = pointerOrigins.current.get(event.pointerId);
			const crossedDragThreshold =
				origin !== undefined &&
				(Math.abs(currentPoint.x - origin.x) >= POINTER_CAPTURE_THRESHOLD_PX ||
					Math.abs(currentPoint.y - origin.y) >= POINTER_CAPTURE_THRESHOLD_PX);
			if (crossedDragThreshold) gestureMoved.current = true;
			if (
				gestureMoved.current &&
				!event.currentTarget.hasPointerCapture(event.pointerId)
			) {
				event.currentTarget.setPointerCapture(event.pointerId);
			}
			const previousPointers = [...pointers.current.values()];
			pointers.current.set(event.pointerId, currentPoint);
			const currentPointers = [...pointers.current.values()];
			if (currentPointers.length === 2 && previousPointers.length === 2) {
				setViewport((current) => {
					const next = pinchFlowCanvasViewport(
						current,
						previousPointers as [FlowCanvasPoint, FlowCanvasPoint],
						currentPointers as [FlowCanvasPoint, FlowCanvasPoint],
						size,
					);
					return next;
				});
				return;
			}
			if (currentPointers.length === 1) {
				if (!gestureMoved.current) return;
				setViewport((current) => {
					const next = panFlowCanvasViewport(
						current,
						{
							x: currentPoint.x - previousPoint.x,
							y: currentPoint.y - previousPoint.y,
						},
						size,
					);
					return next;
				});
			}
		},
		[size],
	);

	const releasePointer = useCallback<PointerEventHandler<SVGSVGElement>>(
		(event) => {
			pointers.current.delete(event.pointerId);
			pointerOrigins.current.delete(event.pointerId);
			if (event.currentTarget.hasPointerCapture(event.pointerId)) {
				event.currentTarget.releasePointerCapture(event.pointerId);
			}
			if (pointers.current.size === 0 && gestureMoved.current) {
				suppressNextClick.current = true;
				suppressClickTimer.current = window.setTimeout(() => {
					suppressNextClick.current = false;
					suppressClickTimer.current = undefined;
				}, 0);
			}
		},
		[],
	);
	const onClickCapture = useCallback<MouseEventHandler<SVGSVGElement>>(
		(event) => {
			if (!suppressNextClick.current) return;
			event.preventDefault();
			event.stopPropagation();
			clearClickSuppression();
		},
		[clearClickSuppression],
	);
	const onPointerLeave = useCallback<PointerEventHandler<SVGSVGElement>>(
		(event) => {
			if (event.currentTarget.hasPointerCapture(event.pointerId)) return;
			releasePointer(event);
		},
		[releasePointer],
	);

	const fit = useCallback(() => setViewport(fitFlowCanvasViewport()), []);
	const zoomIn = useCallback(
		() => setViewport((current) => zoomInFlowCanvasViewport(current, size)),
		[size],
	);
	const zoomOut = useCallback(
		() => setViewport((current) => zoomOutFlowCanvasViewport(current, size)),
		[size],
	);
	const viewBox = viewport.viewBox;
	const svgBinding = useMemo<FlowCanvasSvgBinding>(
		() => ({
			ref: setElement,
			viewBox: `${viewBox.x} ${viewBox.y} ${viewBox.width} ${viewBox.height}`,
			"data-flow-zoom": viewport.zoom.toFixed(3),
			onPointerDown,
			onPointerMove,
			onPointerLeave,
			onPointerUp: releasePointer,
			onPointerCancel: releasePointer,
			onClickCapture,
		}),
		[
			onClickCapture,
			onPointerDown,
			onPointerLeave,
			onPointerMove,
			releasePointer,
			viewBox.height,
			viewBox.width,
			viewBox.x,
			viewBox.y,
			viewport.zoom,
		],
	);

	return { viewport, size, svgBinding, fit, zoomIn, zoomOut };
}
