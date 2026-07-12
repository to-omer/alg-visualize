import { type MutableRefObject, useCallback, useEffect, useRef } from "react";

import type { EngineRequest } from "./engine-types";
import { type PlaybackStatus, useVisualizerStore } from "./store";

const CONTEXT_RECOVERY_ERROR =
	"WebGL context was not restored within five seconds";

type RendererRecoveryOptions = {
	cursor: number;
	frameAvailable: boolean;
	generation: MutableRefObject<number>;
	post: (request: EngineRequest) => void;
	reportEngineError: (message: string, source?: "renderer" | "worker") => void;
	requestPending: MutableRefObject<boolean>;
	setStatus: (status: PlaybackStatus) => void;
};

export function useRendererRecovery({
	cursor,
	frameAvailable,
	generation,
	post,
	reportEngineError,
	requestPending,
	setStatus,
}: RendererRecoveryOptions) {
	const timer = useRef<number | undefined>(undefined);
	const handleContextState = useCallback(
		(state: "lost" | "restored") => {
			if (state === "lost") {
				setStatus("paused");
				window.clearTimeout(timer.current);
				timer.current = window.setTimeout(() => {
					reportEngineError(CONTEXT_RECOVERY_ERROR, "renderer");
				}, 5_000);
				return;
			}
			window.clearTimeout(timer.current);
			if (useVisualizerStore.getState().error === CONTEXT_RECOVERY_ERROR) {
				return;
			}
			if (frameAvailable) {
				generation.current += 1;
				requestPending.current = true;
				post({ kind: "seek", generation: generation.current, target: cursor });
			}
		},
		[
			cursor,
			frameAvailable,
			generation,
			post,
			requestPending,
			reportEngineError,
			setStatus,
		],
	);
	const handleRendererError = useCallback(
		(message: string) => {
			reportEngineError(`WebGL renderer failed: ${message}`, "renderer");
		},
		[reportEngineError],
	);
	useEffect(() => () => window.clearTimeout(timer.current), []);
	return { handleContextState, handleRendererError };
}
