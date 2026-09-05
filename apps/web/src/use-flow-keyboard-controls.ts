import { useEffect } from "react";
import { PLAYBACK_SPEEDS } from "./playback";

type FlowKeyboardControls = Readonly<{
	busy: boolean;
	canStepForward: boolean;
	cursor: number;
	enabled: boolean;
	extent: number;
	playing: boolean;
	onFit: () => void;
	onPause: () => void;
	onPlay: () => void;
	onSeek: (target: number) => void;
	onSpeedChange: (speed: number) => void;
	onStepBackward: () => void;
	onStepForward: () => void;
	speed: number;
}>;

function isEditableKeyboardTarget(target: EventTarget | null): boolean {
	return (
		target instanceof Element &&
		target.closest(
			"input, select, textarea, button, [contenteditable=true], .cm-editor",
		) !== null
	);
}

export function useFlowKeyboardControls({
	busy,
	canStepForward,
	cursor,
	enabled,
	extent,
	playing,
	onFit,
	onPause,
	onPlay,
	onSeek,
	onSpeedChange,
	onStepBackward,
	onStepForward,
	speed,
}: FlowKeyboardControls): void {
	useEffect(() => {
		if (!enabled) return;
		const changeSpeed = (offset: -1 | 1) => {
			const currentIndex = PLAYBACK_SPEEDS.indexOf(
				speed as (typeof PLAYBACK_SPEEDS)[number],
			);
			const fallbackIndex = PLAYBACK_SPEEDS.indexOf(1);
			const index = currentIndex < 0 ? fallbackIndex : currentIndex;
			const next =
				PLAYBACK_SPEEDS[
					Math.max(0, Math.min(PLAYBACK_SPEEDS.length - 1, index + offset))
				];
			if (next !== undefined) onSpeedChange(next);
		};
		const onKeyDown = (event: KeyboardEvent) => {
			if (isEditableKeyboardTarget(event.target)) return;
			if (event.code === "Space") {
				event.preventDefault();
				if (playing) onPause();
				else if (!busy && canStepForward) onPlay();
			} else if (event.key === "ArrowLeft" && !busy && cursor > 0) {
				event.preventDefault();
				onStepBackward();
			} else if (event.key === "ArrowRight" && !busy && canStepForward) {
				event.preventDefault();
				onStepForward();
			} else if (event.key === "Home" && !busy && cursor > 0) {
				event.preventDefault();
				onSeek(0);
			} else if (event.key === "End" && !busy && cursor < extent) {
				event.preventDefault();
				onSeek(extent);
			} else if (event.key.toLowerCase() === "f") {
				event.preventDefault();
				onFit();
			} else if (event.key === "+" || event.key === "=") {
				event.preventDefault();
				changeSpeed(1);
			} else if (event.key === "-" || event.key === "_") {
				event.preventDefault();
				changeSpeed(-1);
			}
		};
		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [
		busy,
		canStepForward,
		cursor,
		enabled,
		extent,
		onFit,
		onPause,
		onPlay,
		onSeek,
		onSpeedChange,
		onStepBackward,
		onStepForward,
		playing,
		speed,
	]);
}
