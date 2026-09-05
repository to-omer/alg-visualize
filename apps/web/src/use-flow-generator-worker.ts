import { useCallback, useEffect, useRef } from "react";
import {
	FLOW_GENERATOR_SIZE_ERROR,
	type FlowGeneratorWorkerRequest,
	type FlowGeneratorWorkerResponse,
	flowGeneratorRequestFitsBudget,
	MAX_FLOW_GENERATOR_TRANSFER_BYTES,
} from "./flow-generator-worker-protocol";

type FlowGeneratorWorkerController = Readonly<{
	start: (request: FlowGeneratorWorkerRequest) => void;
	cancel: () => boolean;
}>;

export function useFlowGeneratorWorker(
	onResponse: (response: FlowGeneratorWorkerResponse) => void,
): FlowGeneratorWorkerController {
	const workerRef = useRef<Worker | undefined>(undefined);
	const responseHandler = useRef(onResponse);
	responseHandler.current = onResponse;

	const cancel = useCallback(() => {
		const worker = workerRef.current;
		if (worker === undefined) return false;
		worker.terminate();
		workerRef.current = undefined;
		return true;
	}, []);

	useEffect(
		() => () => {
			cancel();
		},
		[cancel],
	);

	const start = useCallback(
		(request: FlowGeneratorWorkerRequest) => {
			cancel();
			if (
				!flowGeneratorRequestFitsBudget(
					request,
					MAX_FLOW_GENERATOR_TRANSFER_BYTES,
				)
			) {
				responseHandler.current({
					kind: "error",
					jobId: request.jobId,
					message: FLOW_GENERATOR_SIZE_ERROR,
				});
				return;
			}
			const worker = new Worker(
				new URL("./flow-generator-worker.ts", import.meta.url),
				{ type: "module" },
			);
			workerRef.current = worker;
			const finish = (response: FlowGeneratorWorkerResponse) => {
				if (workerRef.current !== worker) return;
				responseHandler.current(response);
				if (response.kind !== "progress") cancel();
			};
			worker.addEventListener(
				"message",
				(event: MessageEvent<FlowGeneratorWorkerResponse>) =>
					finish(event.data),
			);
			worker.addEventListener("error", (event) => {
				event.preventDefault();
				finish({
					kind: "error",
					jobId: request.jobId,
					message:
						event.message.length > 0
							? `Flow generator Worker failed: ${event.message}`
							: "Flow generator Worker failed",
				});
			});
			worker.addEventListener("messageerror", () => {
				finish({
					kind: "error",
					jobId: request.jobId,
					message: "Flow generator Worker returned an unreadable message",
				});
			});
			worker.postMessage(request);
		},
		[cancel],
	);

	return { start, cancel };
}
