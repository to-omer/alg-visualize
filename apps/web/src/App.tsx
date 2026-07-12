import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { AlgorithmSettingsDialog } from "./AlgorithmSettingsDialog";
import { processEngineResponse } from "./engine-session-response";
import type {
	EngineRequest,
	EngineResponse,
	InputDiagnostic,
} from "./engine-types";
import { entityIdKey } from "./engine-types";
import {
	DEFAULT_GENERATOR,
	GeneratorDialog,
	type GeneratorForm,
} from "./GeneratorDialog";
import { generatorRequestSpec } from "./generator-request";
import { MAX_DETAIL_ENTITIES } from "./graph-layout";
import type { InputMode } from "./InputPanel";
import { eventActiveKey, traceDescription } from "./pedagogy";
import {
	cursorForRawEvent,
	type PlaybackGranularity,
	tracePositions,
} from "./playback";
import {
	ALGORITHMS,
	type AlgorithmId,
	defaultAlgorithmConfig,
} from "./scenario";
import { useVisualizerStore } from "./store";
import { getStructureNodeByKey } from "./structure-index";
import { Timeline } from "./Timeline";
import { TopBar } from "./TopBar";
import type { TraceReplayController } from "./trace-replay";
import { buildTracePresentation } from "./trace-visualization";
import { useEngineWorker } from "./use-engine-worker";
import { useKeyboardControls } from "./use-keyboard-controls";
import { useRendererRecovery } from "./use-renderer-recovery";
import { fitsUtf8Budget } from "./utf8-budget";
import { VisualizationWorkspace } from "./VisualizationWorkspace";

function useSynchronousCursor(initial: number) {
	const [, requestRender] = useState(0);
	const current = useRef(initial);
	const setCurrent = useCallback((next: number) => {
		current.current = next;
		requestRender((version) => version + 1);
	}, []);
	return [current.current, setCurrent] as const;
}

export function App() {
	const [dialogOpen, setDialogOpen] = useState(false);
	const [settingsOpen, setSettingsOpen] = useState(false);
	const [errorSource, setErrorSource] = useState<
		"input" | "renderer" | "worker"
	>("input");
	const [generator, setGenerator] = useState<GeneratorForm>(DEFAULT_GENERATOR);
	const [algorithmConfig, setAlgorithmConfig] = useState<
		Record<string, unknown>
	>(defaultAlgorithmConfig("avl"));
	const [inputMode, setInputMode] = useState<InputMode>("json");
	const [initialDsl, setInitialDsl] = useState("");
	const [operationsDsl, setOperationsDsl] = useState("");
	const [inputDiagnostic, setInputDiagnostic] = useState<InputDiagnostic>();
	const [playbackSpeed, setPlaybackSpeed] = useState(1);
	const [granularity, setGranularity] =
		useState<PlaybackGranularity>("semantic");
	const [detailCursor, setDetailCursor] = useSynchronousCursor(0);
	const [publishedRawEventIndex, setPublishedRawEventIndex] =
		useSynchronousCursor(-1);
	const setPublishedDetailCursor = useCallback(
		(next: number) => {
			setDetailCursor(next);
			setPublishedRawEventIndex(-1);
		},
		[setDetailCursor, setPublishedRawEventIndex],
	);
	const [fitSignal, setFitSignal] = useState(0);
	const [selectedEntityKey, setSelectedEntityKey] = useState<string>();
	const [trackExecution, setTrackExecution] = useState(false);
	const [seekProgress, setSeekProgress] = useState<{
		cursor: number;
		target: number;
	}>();
	const [seekCoverage, setSeekCoverage] = useState(0);
	const [revisionStatus, setRevisionStatus] = useState<
		"current" | "legacy-derived"
	>("current");
	const generation = useRef(0);
	const requestPending = useRef(false);
	const playWhenReady = useRef(false);
	const playAfterSeek = useRef(false);
	const scenarioEdited = useRef(false);
	const advancePlayback = useRef<() => void>(() => undefined);
	const replayMovementPending = useRef(false);
	const traceReplay = useRef<TraceReplayController | undefined>(undefined);
	const granularityRef = useRef<PlaybackGranularity>(granularity);
	granularityRef.current = granularity;
	const {
		algorithm,
		clearFrame,
		cursor,
		error,
		frame,
		itemCount,
		scenario,
		setAlgorithm,
		setError,
		setFrame,
		setItemCount,
		setScenario,
		setStatus,
		status,
	} = useVisualizerStore();
	const fatalError = error !== undefined && errorSource !== "input";
	const reportEngineError = useCallback(
		(message: string, source: "renderer" | "worker" = "worker") => {
			requestPending.current = false;
			playWhenReady.current = false;
			playAfterSeek.current = false;
			setSeekProgress(undefined);
			setError(message);
			setErrorSource(source);
			setStatus("error");
		},
		[setError, setStatus],
	);

	const handleEngineResponse = useCallback(
		(response: EngineResponse) =>
			processEngineResponse(response, {
				clearFrame,
				generation,
				granularity: granularityRef,
				onPrepared: () => {
					setDialogOpen(false);
					setSettingsOpen(false);
				},
				playAfterSeek,
				playWhenReady,
				reportEngineError,
				requestPending,
				scenarioEdited,
				setAlgorithm,
				setAlgorithmConfig,
				setDetailCursor: setPublishedDetailCursor,
				setError,
				setErrorSource,
				setFrame,
				setInitialDsl,
				setInputDiagnostic,
				setInputMode,
				setItemCount,
				setOperationsDsl,
				setRevisionStatus,
				setScenario,
				setSeekCoverage,
				setSeekProgress,
				setSelectedEntityKey,
				setStatus,
				traceReplay,
			}),
		[
			clearFrame,
			reportEngineError,
			setAlgorithm,
			setPublishedDetailCursor,
			setError,
			setFrame,
			setItemCount,
			setScenario,
			setStatus,
		],
	);
	const { post: rawPost, postTransfer: rawPostTransfer } = useEngineWorker(
		handleEngineResponse,
		reportEngineError,
	);
	const post = useCallback(
		(request: EngineRequest) => {
			if (!fatalError) rawPost(request);
		},
		[fatalError, rawPost],
	);
	const postTransfer = useCallback(
		(request: EngineRequest, transfer: Transferable[]) => {
			if (!fatalError) rawPostTransfer(request, transfer);
		},
		[fatalError, rawPostTransfer],
	);

	const loadScenario = useCallback(
		(autoplay: boolean) => {
			if (fatalError) return;
			generation.current += 1;
			requestPending.current = true;
			playWhenReady.current = autoplay;
			playAfterSeek.current = false;
			setError(undefined);
			setStatus("loading");
			post({
				kind: "create",
				generation: generation.current,
				scenario,
				discardProvenance: scenarioEdited.current,
			});
		},
		[fatalError, post, scenario, setError, setStatus],
	);

	useEffect(() => {
		if (status !== "playing") {
			return;
		}
		const timer = window.setInterval(() => {
			advancePlayback.current();
		}, 520 / playbackSpeed);
		return () => window.clearInterval(timer);
	}, [playbackSpeed, status]);

	const seek = useCallback(
		(target: number, autoplay = false) => {
			if (fatalError || frame === undefined || replayMovementPending.current) {
				return;
			}
			generation.current += 1;
			requestPending.current = true;
			playAfterSeek.current = autoplay;
			setStatus("paused");
			post({ kind: "seek", generation: generation.current, target });
		},
		[fatalError, frame, post, setStatus],
	);

	const detailPositions = useMemo(
		() => tracePositions(frame?.trace, granularity, frame?.patches),
		[frame?.trace, frame?.patches, granularity],
	);
	const publishedDetailCursor = useMemo(() => {
		return cursorForRawEvent(detailPositions, publishedRawEventIndex);
	}, [detailPositions, publishedRawEventIndex]);

	useEffect(() => {
		setDetailCursor(
			Math.min(detailCursor, Math.max(0, detailPositions.length - 1)),
		);
	}, [detailCursor, detailPositions.length, setDetailCursor]);

	useEffect(() => {
		if (requestPending.current) return;
		const target = detailPositions[detailCursor];
		const controller = traceReplay.current;
		if (
			target === undefined ||
			controller === undefined ||
			controller.currentRawEventIndex() === target
		) {
			setPublishedRawEventIndex(target ?? -1);
			replayMovementPending.current = false;
			return;
		}
		replayMovementPending.current = true;
		let cancelled = false;
		void controller.moveToAsync(target).then(
			(nextFrame) => {
				if (!cancelled) {
					replayMovementPending.current = false;
					setPublishedRawEventIndex(target);
					setFrame(nextFrame);
				}
			},
			(replayError: unknown) => {
				if (cancelled) return;
				replayMovementPending.current = false;
				reportEngineError(
					replayError instanceof Error
						? replayError.message
						: "Trace replay failed",
				);
			},
		);
		return () => {
			cancelled = true;
		};
	}, [
		detailCursor,
		detailPositions,
		reportEngineError,
		setFrame,
		setPublishedRawEventIndex,
	]);

	const stepForward = useCallback(() => {
		if (fatalError || requestPending.current || replayMovementPending.current) {
			return;
		}
		if (detailCursor < detailPositions.length - 1) {
			replayMovementPending.current = true;
			setDetailCursor(detailCursor + 1);
			return;
		}
		if (cursor >= itemCount) {
			setStatus("paused");
			return;
		}
		requestPending.current = true;
		post({ kind: "next", generation: generation.current });
	}, [
		cursor,
		detailCursor,
		detailPositions.length,
		fatalError,
		itemCount,
		post,
		setDetailCursor,
		setStatus,
	]);

	const stepBackward = useCallback(() => {
		if (fatalError || requestPending.current || replayMovementPending.current)
			return;
		if (detailCursor > 0) {
			replayMovementPending.current = true;
			setDetailCursor(detailCursor - 1);
			setStatus("paused");
			return;
		}
		seek(Math.max(0, cursor - 1));
	}, [cursor, detailCursor, fatalError, seek, setDetailCursor, setStatus]);

	advancePlayback.current = stepForward;

	const { handleContextState: handleCanvasContext, handleRendererError } =
		useRendererRecovery({
			cursor,
			frameAvailable: frame !== undefined,
			generation,
			post,
			reportEngineError,
			requestPending,
			setStatus,
		});

	useEffect(() => {
		if (
			selectedEntityKey !== undefined &&
			frame !== undefined &&
			getStructureNodeByKey(frame.structure, selectedEntityKey) === undefined
		) {
			setSelectedEntityKey(undefined);
		}
	}, [frame, selectedEntityKey]);

	useKeyboardControls({
		cursor,
		fatalError,
		frameAvailable: frame !== undefined,
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
	});

	const changeAlgorithm = (next: AlgorithmId) => {
		if (fatalError) return;
		generation.current += 1;
		requestPending.current = true;
		setError(undefined);
		setStatus("loading");
		post({
			kind: "set-algorithm",
			generation: generation.current,
			scenario,
			algorithm: next,
			config: defaultAlgorithmConfig(next),
		});
	};

	const applyAlgorithmConfig = (config: Record<string, unknown>) => {
		if (fatalError) return;
		generation.current += 1;
		requestPending.current = true;
		setError(undefined);
		setStatus("loading");
		post({
			kind: "set-algorithm",
			generation: generation.current,
			scenario,
			algorithm,
			config,
		});
	};

	const changeScenario = (next: string) => {
		setScenario(next);
		scenarioEdited.current = true;
		if (frame !== undefined) {
			setStatus("idle");
		}
	};

	const generate = () => {
		if (fatalError) return;
		generation.current += 1;
		requestPending.current = true;
		setError(undefined);
		setStatus("loading");
		post({
			kind: "generate",
			generation: generation.current,
			scenario,
			spec: generatorRequestSpec(generator),
			stream: generator.stream,
		});
	};

	const switchInputMode = (mode: InputMode) => {
		if (fatalError) return;
		if (mode === inputMode) {
			return;
		}
		if (mode === "json") {
			setInputMode("json");
			return;
		}
		generation.current += 1;
		requestPending.current = true;
		setError(undefined);
		setStatus("loading");
		post({ kind: "format-dsl", generation: generation.current, scenario });
	};

	const applyDsl = () => {
		if (fatalError) return;
		if (!fitsUtf8Budget([initialDsl, operationsDsl], 64 * 1024 * 1024)) {
			const message = "manual input exceeds the combined 64 MiB limit";
			setInputDiagnostic({
				stream: "operations",
				code: "DSL_BYTE_LIMIT",
				line: 1,
				column: 1,
				message,
			});
			setError(`${message} (1:1)`);
			setErrorSource("input");
			setStatus("error");
			return;
		}
		generation.current += 1;
		requestPending.current = true;
		setError(undefined);
		setStatus("loading");
		post({
			kind: "prepare-dsl",
			generation: generation.current,
			scenario,
			initialDsl,
			operationsDsl,
		});
	};

	const importScenario = async (file: File | undefined) => {
		if (fatalError) return;
		if (file === undefined) {
			return;
		}
		if (file.size > 64 * 1024 * 1024) {
			setError("Scenario exceeds the 64 MiB import limit");
			setErrorSource("input");
			setStatus("error");
			return;
		}
		const bytes = await file.arrayBuffer();
		generation.current += 1;
		requestPending.current = true;
		setError(undefined);
		setStatus("loading");
		postTransfer(
			{
				kind: "import-scenario",
				generation: generation.current,
				byteLength: bytes.byteLength,
				bytes,
			} satisfies EngineRequest,
			[bytes],
		);
	};

	const exportScenario = () => {
		if (fatalError) return;
		generation.current += 1;
		requestPending.current = true;
		setError(undefined);
		if (frame !== undefined) {
			setStatus("paused");
		}
		post({
			kind: "export-scenario",
			generation: generation.current,
			scenario,
			discardProvenance: scenarioEdited.current,
		});
	};

	const traceEventIndex =
		publishedRawEventIndex < 0 ? undefined : publishedRawEventIndex;
	const lastTrace =
		traceEventIndex === undefined ? undefined : frame?.trace?.[traceEventIndex];
	const displayedStructure = frame?.structure;
	const rootNode =
		displayedStructure?.root === null || displayedStructure?.root === undefined
			? undefined
			: getStructureNodeByKey(
					displayedStructure,
					entityIdKey(displayedStructure.root),
				);
	const rootKey = rootNode?.keys[0];
	const description = traceDescription(lastTrace);
	const tracePresentation = useMemo(
		() =>
			buildTracePresentation(
				displayedStructure,
				frame?.trace,
				traceEventIndex,
				frame?.patches,
			),
		[displayedStructure, frame?.trace, frame?.patches, traceEventIndex],
	);
	const traceActiveKey =
		tracePresentation.traversalEdge?.target ?? eventActiveKey(lastTrace);
	const activeKey = selectedEntityKey ?? traceActiveKey;
	const transitionMs = Math.max(90, Math.min(520, 480 / playbackSpeed));
	const selectedNode =
		frame === undefined || activeKey === undefined
			? undefined
			: getStructureNodeByKey(frame.structure, activeKey);
	const selectedNodeEntry = selectedNode?.entries[0];
	const selectedEntry =
		selectedEntityKey !== undefined && selectedNodeEntry !== undefined
			? frame?.canonical.entries.find(
					(entry) =>
						entry.id.index === selectedNodeEntry.index &&
						entry.id.generation === selectedNodeEntry.generation,
				)
			: frame?.result?.kind === "found"
				? frame.result
				: selectedNodeEntry !== undefined
					? frame?.canonical.entries.find(
							(entry) =>
								entry.id.index === selectedNodeEntry.index &&
								entry.id.generation === selectedNodeEntry.generation,
						)
					: lastTrace?.key !== null && lastTrace?.key !== undefined
						? frame?.canonical.entries.find(
								(entry) => entry.key === lastTrace.key,
							)
						: undefined;
	const metrics = frame?.canonical.metrics;
	const algorithmLabel =
		ALGORITHMS.find(([id]) => id === algorithm)?.[1] ?? algorithm;
	const lodLabel =
		(displayedStructure?.nodes.length ?? 0) > MAX_DETAIL_ENTITIES
			? "Summary view"
			: "Detail view";
	const statusLabel = error === undefined ? status : "invalid";
	const revisionLabel = scenarioEdited.current
		? "Edited · not loaded"
		: revisionStatus === "legacy-derived"
			? "Legacy input · current trace"
			: "Current revisions";
	const errorGuidance =
		errorSource === "worker"
			? "ページを再読み込みしてください。解消しない場合は、対応ブラウザで開き直します。"
			: errorSource === "renderer"
				? "再生を停止しました。ページを再読み込みし、WebGL が有効なブラウザで開き直します。"
				: "Scenario の該当箇所を修正して、もう一度読み込みます。";
	const openDialog = (dialog: "generator" | "settings") => {
		if (fatalError) {
			return;
		}
		setError(undefined);
		if (status === "playing" || status === "error") {
			setStatus(frame === undefined ? "idle" : "paused");
		}
		if (dialog === "generator") {
			setDialogOpen(true);
		} else {
			setSettingsOpen(true);
		}
	};

	return (
		<div className="app-shell">
			<TopBar
				algorithm={algorithm}
				atEnd={
					itemCount > 0 &&
					cursor >= itemCount &&
					(detailPositions.length === 0 ||
						publishedDetailCursor >= detailPositions.length - 1)
				}
				disabled={fatalError}
				onAlgorithmChange={changeAlgorithm}
				onExport={exportScenario}
				onGenerate={() => openDialog("generator")}
				onImport={(file) => void importScenario(file)}
				onLoad={() => loadScenario(false)}
				onParameters={() => openDialog("settings")}
				onRun={() => {
					if (status === "playing") {
						setStatus("paused");
					} else if (frame === undefined || scenarioEdited.current) {
						loadScenario(true);
					} else if (
						itemCount > 0 &&
						cursor >= itemCount &&
						(detailPositions.length === 0 ||
							publishedDetailCursor >= detailPositions.length - 1)
					) {
						seek(0, true);
					} else {
						setStatus("playing");
					}
				}}
				playing={status === "playing"}
			/>

			<VisualizationWorkspace
				canvas={{
					activeKey: traceActiveKey,
					fitSignal,
					onContextState: handleCanvasContext,
					onError: handleRendererError,
					onSelect: setSelectedEntityKey,
					onTrackExecutionChange: setTrackExecution,
					presentation: tracePresentation,
					selectedKey: selectedEntityKey,
					structure: displayedStructure,
					trackExecution,
					transitionMs,
				}}
				caption={{
					detail: description.detail,
					error,
					guidance: errorGuidance,
					title: description.title,
				}}
				empty={frame === undefined}
				header={{
					algorithmLabel,
					entityCount: displayedStructure?.nodes.length ?? 0,
					entryCount: frame?.canonical.entries.length ?? 0,
					fit: () => {
						setTrackExecution(false);
						setFitSignal((current) => current + 1);
					},
					itemCursor: cursor,
					lodLabel,
					revisionLabel,
					revisionStatus,
					rootKey,
					seekProgress,
					trackingAvailable: frame !== undefined,
					setTracking: setTrackExecution,
					tracking: trackExecution,
				}}
				input={{
					diagnostic: inputDiagnostic,
					engineDisabled: fatalError,
					initialDsl,
					mode: inputMode,
					onApplyDsl: applyDsl,
					onInitialDslChange: (value) => {
						setInputDiagnostic(undefined);
						setInitialDsl(value);
					},
					onModeChange: switchInputMode,
					onOperationsDslChange: (value) => {
						setInputDiagnostic(undefined);
						setOperationsDsl(value);
					},
					onScenarioChange: changeScenario,
					operationsDsl,
					scenario,
					statusLabel,
				}}
				inspector={{
					algorithm,
					entry: selectedEntry,
					event: lastTrace,
					inspectionKind:
						selectedEntityKey !== undefined
							? "selection"
							: traceActiveKey !== undefined
								? "event"
								: "empty",
					metrics,
					node: selectedNode,
				}}
				query={{
					key: tracePresentation.queryKey,
					ownedByNode: tracePresentation.queryNodeKey !== undefined,
				}}
			/>

			<Timeline
				cursor={cursor}
				detailCount={detailPositions.length}
				detailCursor={publishedDetailCursor}
				disabled={fatalError}
				granularity={granularity}
				itemCount={itemCount}
				seekCoverage={seekCoverage}
				onGranularityChange={(next) => {
					if (!requestPending.current && !replayMovementPending.current) {
						setDetailCursor(
							cursorForRawEvent(
								tracePositions(frame?.trace, next, frame?.patches),
								publishedRawEventIndex,
							),
						);
						setGranularity(next);
					}
				}}
				onSeek={seek}
				onSpeedChange={setPlaybackSpeed}
				onStepBackward={stepBackward}
				onStepForward={stepForward}
				speed={playbackSpeed}
			/>

			<GeneratorDialog
				error={dialogOpen ? error : undefined}
				form={generator}
				onChange={setGenerator}
				onGenerate={generate}
				onOpenChange={setDialogOpen}
				open={dialogOpen}
			/>
			<AlgorithmSettingsDialog
				algorithm={algorithm}
				config={algorithmConfig}
				error={settingsOpen ? error : undefined}
				onApply={applyAlgorithmConfig}
				onOpenChange={setSettingsOpen}
				open={settingsOpen}
			/>
		</div>
	);
}
