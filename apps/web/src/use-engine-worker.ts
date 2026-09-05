import { useCallback, useEffect, useRef } from "react";

import type { EngineRequest, EngineResponse } from "./engine-types";

type EngineWorker = {
	post: (request: EngineRequest) => void;
	postTransfer: (request: EngineRequest, transfer: Transferable[]) => void;
};

export function useEngineWorker(
	onResponse: (response: EngineResponse) => boolean | Promise<boolean>,
	onFatalError: (message: string) => void,
	onAcknowledged?: (response: EngineResponse, accepted: boolean) => void,
): EngineWorker {
	const worker = useRef<Worker | undefined>(undefined);
	const responseHandler = useRef(onResponse);
	const fatalHandler = useRef(onFatalError);
	const acknowledgedHandler = useRef(onAcknowledged);
	responseHandler.current = onResponse;
	fatalHandler.current = onFatalError;
	acknowledgedHandler.current = onAcknowledged;

	useEffect(() => {
		const engine = new Worker(new URL("./engine-worker.ts", import.meta.url), {
			type: "module",
		});
		worker.current = engine;
		const onMessage = (event: MessageEvent<EngineResponse>) => {
			const response = event.data;
			const acknowledge = (accepted: boolean) => {
				if (response.kind === "commit") {
					engine.postMessage({
						kind: "commit-ack",
						generation: response.generation,
						accepted,
					} satisfies EngineRequest);
				} else if (response.kind === "ready" || response.kind === "seeked") {
					engine.postMessage({
						kind: "current-ack",
						generation: response.generation,
						accepted,
					} satisfies EngineRequest);
				} else if (
					response.kind === "flow-ready" ||
					response.kind === "flow-update"
				) {
					engine.postMessage({
						kind: "flow-current-ack",
						generation: response.generation,
						publicationId: response.publicationId,
						accepted,
					} satisfies EngineRequest);
				}
			};
			void Promise.resolve(responseHandler.current(response)).then(
				(accepted) => {
					acknowledge(accepted);
					acknowledgedHandler.current?.(response, accepted);
				},
				(error: unknown) => {
					acknowledge(false);
					acknowledgedHandler.current?.(response, false);
					fatalHandler.current(
						error instanceof Error
							? `Visualization response rejected: ${error.message}`
							: "Visualization response rejected",
					);
				},
			);
		};
		const onError = (event: ErrorEvent) => {
			event.preventDefault();
			fatalHandler.current(
				event.message.length > 0
					? `Visualization Worker failed: ${event.message}`
					: "Visualization Worker failed",
			);
		};
		const onMessageError = () => {
			fatalHandler.current(
				"Visualization Worker returned an unreadable message",
			);
		};
		engine.addEventListener("message", onMessage);
		engine.addEventListener("error", onError);
		engine.addEventListener("messageerror", onMessageError);
		return () => {
			engine.removeEventListener("message", onMessage);
			engine.removeEventListener("error", onError);
			engine.removeEventListener("messageerror", onMessageError);
			engine.terminate();
			worker.current = undefined;
		};
	}, []);

	const send = useCallback(
		(request: EngineRequest, transfer: Transferable[] = []) => {
			const engine = worker.current;
			if (engine === undefined) {
				fatalHandler.current("Visualization Worker is not available");
				return;
			}
			engine.postMessage(request, { transfer });
		},
		[],
	);

	return {
		post: useCallback((request) => send(request), [send]),
		postTransfer: send,
	};
}
