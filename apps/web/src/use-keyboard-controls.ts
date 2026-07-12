import type { Dispatch, SetStateAction } from "react";
import { useEffect } from "react";

import { PLAYBACK_SPEEDS } from "./playback";
import type { PlaybackStatus } from "./store";

type KeyboardControls = {
	cursor: number;
	fatalError: boolean;
	frameAvailable: boolean;
	itemCount: number;
	loadScenario: (autoplay: boolean) => void;
	seek: (target: number) => void;
	setFitSignal: Dispatch<SetStateAction<number>>;
	setPlaybackSpeed: Dispatch<SetStateAction<number>>;
	setStatus: (status: PlaybackStatus) => void;
	setTrackExecution: Dispatch<SetStateAction<boolean>>;
	status: PlaybackStatus;
	stepBackward: () => void;
	stepForward: () => void;
};

export function useKeyboardControls({
	cursor,
	fatalError,
	frameAvailable,
	itemCount,
	loadScenario,
	seek,
	setFitSignal,
	setPlaybackSpeed,
	setStatus,
	setTrackExecution,
	status,
	stepBackward,
	stepForward,
}: KeyboardControls) {
	useEffect(() => {
		const changeSpeed = (offset: -1 | 1) => {
			setPlaybackSpeed((current) => {
				const index = PLAYBACK_SPEEDS.indexOf(
					current as (typeof PLAYBACK_SPEEDS)[number],
				);
				return (
					PLAYBACK_SPEEDS[
						Math.max(0, Math.min(PLAYBACK_SPEEDS.length - 1, index + offset))
					] ?? 1
				);
			});
		};
		const onKeyDown = (event: KeyboardEvent) => {
			if (fatalError) return;
			const target = event.target as HTMLElement | null;
			if (
				target?.closest(
					"input, select, textarea, button, [contenteditable=true], .cm-editor",
				) !== null
			) {
				return;
			}
			if (event.code === "Space") {
				event.preventDefault();
				if (!frameAvailable) loadScenario(true);
				else setStatus(status === "playing" ? "paused" : "playing");
			} else if (event.key === "ArrowLeft") {
				if (event.shiftKey) seek(Math.max(0, cursor - 1));
				else stepBackward();
			} else if (event.key === "ArrowRight") {
				if (event.shiftKey) seek(Math.min(itemCount, cursor + 1));
				else stepForward();
			} else if (event.key === "Home") {
				seek(0);
			} else if (event.key === "End") {
				seek(itemCount);
			} else if (event.key.toLowerCase() === "f") {
				setTrackExecution(false);
				setFitSignal((current) => current + 1);
			} else if (event.key === "+" || event.key === "=") {
				changeSpeed(1);
			} else if (event.key === "-" || event.key === "_") {
				changeSpeed(-1);
			}
		};
		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [
		cursor,
		fatalError,
		frameAvailable,
		itemCount,
		loadScenario,
		seek,
		setFitSignal,
		setPlaybackSpeed,
		setStatus,
		setTrackExecution,
		status,
		stepBackward,
		stepForward,
	]);
}
