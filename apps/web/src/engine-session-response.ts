import type { MutableRefObject } from "react";

import { decodeFrame } from "./engine-frame-decoder";
import type {
	CurrentFrame,
	EngineFrame,
	EngineResponse,
	InputDiagnostic,
} from "./engine-types";
import { type PlaybackGranularity, tracePositions } from "./playback";
import type { AlgorithmId } from "./scenario";
import { type PlaybackStatus, useVisualizerStore } from "./store";
import { TraceReplayController } from "./trace-replay";

type EngineSessionResponseContext = {
	clearFrame: () => void;
	generation: MutableRefObject<number>;
	granularity: MutableRefObject<PlaybackGranularity>;
	onPrepared: () => void;
	playAfterSeek: MutableRefObject<boolean>;
	playWhenReady: MutableRefObject<boolean>;
	reportEngineError: (message: string) => void;
	requestPending: MutableRefObject<boolean>;
	scenarioEdited: MutableRefObject<boolean>;
	setAlgorithm: (algorithm: AlgorithmId) => void;
	setAlgorithmConfig: (config: Record<string, unknown>) => void;
	setDetailCursor: (cursor: number) => void;
	setError: (error: string | undefined) => void;
	setErrorSource: (source: "input" | "renderer" | "worker") => void;
	setFrame: (frame: EngineFrame) => void;
	setInitialDsl: (value: string) => void;
	setInputDiagnostic: (diagnostic: InputDiagnostic | undefined) => void;
	setInputMode: (mode: "dsl" | "json") => void;
	setItemCount: (count: number) => void;
	setOperationsDsl: (value: string) => void;
	setRevisionStatus: (status: "current" | "legacy-derived") => void;
	setScenario: (scenario: string) => void;
	setSeekCoverage: (coverage: number) => void;
	setSeekProgress: (
		progress: { cursor: number; target: number } | undefined,
	) => void;
	setSelectedEntityKey: (key: string | undefined) => void;
	setStatus: (status: PlaybackStatus) => void;
	traceReplay: MutableRefObject<TraceReplayController | undefined>;
};

export async function processEngineResponse(
	response: EngineResponse,
	context: EngineSessionResponseContext,
): Promise<boolean> {
	try {
		if (response.generation !== context.generation.current) return false;
		switch (response.kind) {
			case "ready": {
				context.traceReplay.current = undefined;
				context.setSeekCoverage(0);
				const decoded = await decodeFrame(response.packet, "current");
				if (response.generation !== context.generation.current) return false;
				context.requestPending.current = false;
				context.setSeekProgress(undefined);
				context.setDetailCursor(0);
				context.setFrame(decoded);
				context.setItemCount(response.itemCount);
				context.setAlgorithm(response.algorithm as AlgorithmId);
				context.setAlgorithmConfig(response.algorithmConfig);
				context.setRevisionStatus(response.revisionStatus);
				context.setSelectedEntityKey(undefined);
				if (response.scenario !== undefined) {
					context.setScenario(response.scenario);
					context.scenarioEdited.current = false;
				}
				context.setStatus(context.playWhenReady.current ? "playing" : "ready");
				context.playWhenReady.current = false;
				context.playAfterSeek.current = false;
				break;
			}
			case "commit": {
				const decoded = await decodeFrame(response.packet, "commit");
				if (response.generation !== context.generation.current) return false;
				const baseFrame = contextFrame();
				if (baseFrame === undefined) {
					throw new Error("Commit arrived without a base frame");
				}
				const base: CurrentFrame = {
					itemIndex: baseFrame.itemIndex,
					itemCount: baseFrame.itemCount,
					structure: baseFrame.structure,
					canonical: baseFrame.canonical,
				};
				const replay = await TraceReplayController.create(
					base,
					decoded,
					context.traceReplay.current,
				);
				if (response.generation !== context.generation.current) return false;
				context.traceReplay.current = replay;
				const firstEvent = tracePositions(
					decoded.trace,
					context.granularity.current,
					decoded.patches,
				)[0];
				context.setDetailCursor(0);
				context.setFrame(replay.moveTo(firstEvent ?? -1));
				context.requestPending.current = false;
				break;
			}
			case "seeked": {
				const decoded = await decodeFrame(response.packet, "current");
				if (response.generation !== context.generation.current) return false;
				context.requestPending.current = false;
				context.traceReplay.current = undefined;
				context.setDetailCursor(0);
				context.setFrame(decoded);
				context.setSeekProgress(undefined);
				context.setStatus(context.playAfterSeek.current ? "playing" : "paused");
				context.playAfterSeek.current = false;
				break;
			}
			case "seek-progress":
				context.setSeekProgress({
					cursor: response.cursor,
					target: response.target,
				});
				context.setStatus("loading");
				break;
			case "index-progress":
			case "index-ready":
				context.setSeekCoverage(response.coverage);
				break;
			case "index-error":
				console.warn("Seek index disabled:", response.message);
				break;
			case "ended":
				context.requestPending.current = false;
				context.setStatus("paused");
				break;
			case "scenario-prepared":
				context.requestPending.current = false;
				context.playAfterSeek.current = false;
				context.setScenario(response.scenario);
				context.setRevisionStatus(response.revisionStatus);
				context.setInputDiagnostic(undefined);
				if (response.algorithm !== undefined) {
					context.setAlgorithm(response.algorithm as AlgorithmId);
				}
				if (response.algorithmConfig !== undefined) {
					context.setAlgorithmConfig(response.algorithmConfig);
				}
				context.scenarioEdited.current = false;
				context.setDetailCursor(0);
				context.traceReplay.current = undefined;
				context.clearFrame();
				context.setStatus("idle");
				context.onPrepared();
				break;
			case "dsl-formatted":
				context.requestPending.current = false;
				context.setInitialDsl(response.initialDsl);
				context.setOperationsDsl(response.operationsDsl);
				context.setInputMode("dsl");
				context.setStatus("idle");
				break;
			case "input-diagnostic":
				context.requestPending.current = false;
				context.setInputDiagnostic({
					stream: response.stream,
					code: response.code,
					line: response.line,
					column: response.column,
					message: response.message,
				});
				context.setError(
					`${response.message} (${response.line}:${response.column})`,
				);
				context.setErrorSource("input");
				context.setStatus("error");
				break;
			case "scenario-exported":
				context.requestPending.current = false;
				context.setScenario(response.scenario);
				context.setRevisionStatus(response.revisionStatus);
				context.scenarioEdited.current = false;
				downloadScenario(response.canonical);
				break;
			case "error":
				if (response.source === "engine") {
					context.reportEngineError(response.message);
					break;
				}
				context.requestPending.current = false;
				context.playWhenReady.current = false;
				context.playAfterSeek.current = false;
				context.setSeekProgress(undefined);
				context.setError(response.message);
				context.setErrorSource("input");
				context.setStatus("error");
				break;
		}
		return true;
	} catch (error: unknown) {
		if (response.generation !== context.generation.current) return false;
		context.reportEngineError(
			error instanceof Error
				? error.message
				: "The Worker returned an invalid response",
		);
		return false;
	}
}

function contextFrame(): EngineFrame | undefined {
	// Delayed lookup is required because packet decoding yields to the event loop.
	return useVisualizerStore.getState().frame;
}

function downloadScenario(canonical: string) {
	const url = URL.createObjectURL(
		new Blob([canonical], { type: "application/json" }),
	);
	const anchor = document.createElement("a");
	anchor.href = url;
	anchor.download = "ordered-map-scenario.json";
	anchor.click();
	URL.revokeObjectURL(url);
}
