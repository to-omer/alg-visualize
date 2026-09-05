import { useEffect, useId, useRef, useState } from "react";

import {
	PRIMARY_WORK_PROGRESS_MAX,
	primaryWorkProgressValue,
} from "./flow-detail-density";
import type { FlowPlaybackGranularity } from "./flow-preferences";
import type { FlowCurrentSceneV9 } from "./flow-scene";
import { PLAYBACK_SPEEDS } from "./playback";

type Props = Readonly<{
	cursor: number;
	extent: number;
	navigationDisabled: boolean;
	optionsDisabled: boolean;
	canStepForward: boolean;
	granularity: FlowPlaybackGranularity;
	currentBoundary: FlowPlaybackGranularity | undefined;
	traceSteps: FlowCurrentSceneV9["trace_steps"] | undefined;
	workProgress:
		| NonNullable<FlowCurrentSceneV9["trace_event_semantics"]>["work_progress"]
		| undefined;
	modeNotice: string | undefined;
	visibleBoundaryPositions: readonly number[];
	boundaryInventoryComplete: boolean;
	boundaryInventoryPrefixEnd: number;
	speed: number;
	onGranularityChange: (value: FlowPlaybackGranularity) => void;
	onSeek: (target: number) => void;
	onRawSeek: (target: number) => void;
	onSpeedChange: (value: number) => void;
	onStepBackward: () => void;
	onStepForward: () => void;
}>;

function boundaryOrdinalAtOrBefore(
	positions: readonly number[],
	cursor: number,
): number {
	let low = 0;
	let high = positions.length;
	while (low < high) {
		const middle = low + Math.floor((high - low) / 2);
		if ((positions[middle] ?? Number.POSITIVE_INFINITY) <= cursor) {
			low = middle + 1;
		} else {
			high = middle;
		}
	}
	return Math.max(0, low - 1);
}

export function FlowTimeline({
	cursor,
	extent,
	navigationDisabled,
	optionsDisabled,
	canStepForward,
	granularity,
	currentBoundary,
	traceSteps,
	workProgress,
	modeNotice,
	visibleBoundaryPositions,
	boundaryInventoryComplete,
	boundaryInventoryPrefixEnd,
	speed,
	onGranularityChange,
	onSeek,
	onRawSeek,
	onSpeedChange,
	onStepBackward,
	onStepForward,
}: Props) {
	const modeNoticeId = useId();
	const modeNoticeWrapRef = useRef<HTMLSpanElement>(null);
	const [openModeNotice, setOpenModeNotice] = useState<string>();
	const [modeNoticeDismissed, setModeNoticeDismissed] = useState(false);
	const modeNoticeOpen =
		modeNotice !== undefined && openModeNotice === modeNotice;
	useEffect(() => {
		if (!modeNoticeOpen) return;
		const closeOnOutsidePointer = (event: PointerEvent) => {
			if (
				event.target instanceof Node &&
				!modeNoticeWrapRef.current?.contains(event.target)
			) {
				setOpenModeNotice(undefined);
			}
		};
		document.addEventListener("pointerdown", closeOnOutsidePointer, true);
		return () =>
			document.removeEventListener("pointerdown", closeOnOutsidePointer, true);
	}, [modeNoticeOpen]);
	const phaseAvailable =
		traceSteps?.phase_availability.availability === "available";
	const operationAvailable =
		traceSteps?.operation_availability.availability === "available";
	const detailedAvailable = traceSteps?.detail.availability === "available";
	const stepModeHelp =
		granularity === "phase"
			? (traceSteps?.phase_unit ?? "Phase boundaries only")
			: granularity === "micro" &&
					traceSteps?.detail.availability === "available"
				? `${traceSteps.detail.unit}. Detail visits every trace event, including Operation and Phase boundaries; Inspector shows the current event classification.`
				: (traceSteps?.operation_unit ?? "One complete operation");
	const detailedAvailabilityHelp =
		traceSteps?.detail.availability === "unavailable"
			? ` Detail playback unavailable: ${traceSteps.detail.reason}`
			: "";
	const operationAvailabilityHelp =
		traceSteps?.operation_availability.availability === "unavailable"
			? ` Operation playback unavailable: ${traceSteps.operation_availability.reason}`
			: "";
	const phaseAvailabilityHelp =
		traceSteps?.phase_availability.availability === "unavailable"
			? ` Phase playback unavailable: ${traceSteps.phase_availability.reason}`
			: "";
	const playbackHelp = `${stepModeHelp}${detailedAvailabilityHelp}${operationAvailabilityHelp}${phaseAvailabilityHelp}`;
	const visibleBoundaryOrdinal =
		granularity === "micro"
			? cursor
			: boundaryOrdinalAtOrBefore(visibleBoundaryPositions, cursor);
	const visibleBoundaryExtent = Math.max(
		0,
		visibleBoundaryPositions.length - 1,
	);
	const cursorIsVisibleBoundary =
		granularity === "micro" ||
		visibleBoundaryPositions[visibleBoundaryOrdinal] === cursor;
	const visibleBoundaryOrdinalKnown =
		boundaryInventoryComplete || cursor <= boundaryInventoryPrefixEnd;
	const modeLabel = granularity === "phase" ? "Phase" : "Operation";
	const visibleBoundaryTotal = boundaryInventoryComplete
		? String(visibleBoundaryExtent)
		: "?";
	const currentBoundaryLabel =
		currentBoundary === "phase"
			? "Phase"
			: currentBoundary === "operation"
				? "Operation"
				: currentBoundary === "micro"
					? "Detail"
					: undefined;
	const detailReadout =
		workProgress === undefined || traceSteps === undefined
			? `Event ${cursor}/${extent} · ${
					cursor === 0
						? "Initial"
						: currentBoundaryLabel === undefined
							? "Pending"
							: currentBoundaryLabel
				}`
			: `Detail ${workProgress.detail_completed}/${workProgress.detail_total} · ${
					traceSteps.primary_work.abstraction === "oracle-call"
						? "Oracle call"
						: traceSteps.primary_work.abstraction === "iteration"
							? "Iteration"
							: "Primitive"
				} ${workProgress.primary_completed}/${workProgress.primary_total}`;
	const workProgressValue =
		workProgress === undefined
			? undefined
			: primaryWorkProgressValue(
					workProgress.primary_completed,
					workProgress.primary_total,
				);
	const visibleReadout =
		granularity === "micro"
			? `Event ${cursor}/${extent}`
			: cursorIsVisibleBoundary && visibleBoundaryOrdinalKnown
				? `${modeLabel} ${visibleBoundaryOrdinal} / ${visibleBoundaryTotal}`
				: cursorIsVisibleBoundary
					? `${modeLabel} ? / ${visibleBoundaryTotal} · raw ${cursor} / ${extent}`
					: `Raw ${cursor} / ${extent} · next ${modeLabel}`;
	return (
		<footer className="timeline-panel flow-timeline">
			<button
				type="button"
				className="transport-button"
				aria-label="First event"
				disabled={navigationDisabled || cursor === 0}
				onClick={() => onSeek(0)}
			>
				↤
			</button>
			<button
				type="button"
				className="transport-button"
				aria-label="Previous step"
				disabled={navigationDisabled || cursor === 0}
				onClick={onStepBackward}
			>
				←
			</button>
			{granularity === "micro" ||
			!cursorIsVisibleBoundary ||
			!visibleBoundaryOrdinalKnown ? (
				<input
					type="range"
					aria-label={
						granularity === "micro"
							? "Raw trace position"
							: cursorIsVisibleBoundary && !visibleBoundaryOrdinalKnown
								? `Raw trace position while the ${modeLabel} ordinal is unknown`
								: `Raw trace position before the next ${modeLabel} boundary`
					}
					aria-valuetext={
						granularity === "micro"
							? `${detailReadout.replaceAll("/", " of ")} · raw event ${cursor} of ${extent}`
							: `Raw ${cursor} of ${extent}${cursorIsVisibleBoundary ? ` · ${modeLabel} ordinal unknown` : ` · next ${modeLabel}`}`
					}
					min="0"
					max={extent}
					value={cursor}
					disabled={navigationDisabled || extent === 0}
					onChange={(event) => onRawSeek(Number(event.target.value))}
					onKeyDown={(event) => {
						if (event.key === "Home") {
							event.preventDefault();
							onRawSeek(0);
						} else if (event.key === "End") {
							event.preventDefault();
							onRawSeek(extent);
						}
					}}
				/>
			) : (
				<input
					type="range"
					aria-label="Visible trace position"
					aria-valuetext={`${modeLabel} ${visibleBoundaryOrdinal} of ${visibleBoundaryTotal} · raw ${cursor} of ${extent}`}
					min="0"
					max={Math.max(0, visibleBoundaryPositions.length - 1)}
					value={visibleBoundaryOrdinal}
					disabled={navigationDisabled || visibleBoundaryPositions.length <= 1}
					onChange={(event) => {
						const target = visibleBoundaryPositions[Number(event.target.value)];
						if (target !== undefined) onSeek(target);
					}}
				/>
			)}
			<span
				className="timeline-readout"
				title={`Raw trace ${cursor} / ${extent}`}
			>
				<span data-testid="flow-timeline-visible-readout">
					{visibleReadout}
				</span>
				{workProgress !== undefined && (
					<>
						<small data-testid="flow-timeline-work-readout">
							{detailReadout}
						</small>
						<progress
							className="flow-primary-work-progress"
							aria-label="Measured primary work progress"
							aria-valuetext={`${traceSteps?.primary_work.unit ?? "Primary work"} ${workProgress.primary_completed} of ${workProgress.primary_total}`}
							max={PRIMARY_WORK_PROGRESS_MAX}
							value={workProgressValue ?? 0}
						/>
					</>
				)}
				<span className="visually-hidden" data-testid="flow-timeline-readout">
					Raw {cursor} / {extent}
				</span>
			</span>
			<div
				className="timeline-option"
				data-has-notice={modeNotice !== undefined || undefined}
			>
				<label className="timeline-option-control">
					<span>Move by</span>
					<select
						aria-label={`Playback granularity. ${playbackHelp}`}
						title={playbackHelp}
						value={granularity}
						disabled={optionsDisabled}
						onChange={(event) =>
							onGranularityChange(event.target.value as FlowPlaybackGranularity)
						}
					>
						<option
							value="operation"
							disabled={!operationAvailable}
							aria-disabled={!operationAvailable}
						>
							{operationAvailable ? "Operation" : "Operation — unavailable"}
						</option>
						<option
							value="micro"
							disabled={!detailedAvailable}
							aria-disabled={!detailedAvailable}
						>
							{detailedAvailable ? "Detail" : "Detail — unavailable"}
						</option>
						<option
							value="phase"
							disabled={!phaseAvailable}
							aria-disabled={!phaseAvailable}
						>
							{phaseAvailable ? "Phase" : "Phase — unavailable"}
						</option>
					</select>
				</label>
				{traceSteps?.detail.availability === "unavailable" && (
					<span className="visually-hidden">
						Detail playback unavailable. {traceSteps.detail.reason}
					</span>
				)}
				{modeNotice !== undefined && (
					<span ref={modeNoticeWrapRef} className="timeline-mode-notice-wrap">
						<button
							type="button"
							className="timeline-mode-notice"
							aria-label={modeNotice}
							aria-describedby={modeNoticeId}
							aria-expanded={modeNoticeOpen}
							onFocus={() => setModeNoticeDismissed(false)}
							onBlur={() => {
								setOpenModeNotice(undefined);
								setModeNoticeDismissed(false);
							}}
							onMouseLeave={() => setModeNoticeDismissed(false)}
							onClick={() => {
								setModeNoticeDismissed(false);
								setOpenModeNotice(modeNoticeOpen ? undefined : modeNotice);
							}}
							onKeyDown={(event) => {
								if (event.key === "Escape") {
									setOpenModeNotice(undefined);
									setModeNoticeDismissed(true);
								}
							}}
						>
							<span aria-hidden="true">!</span>
						</button>
						<span
							id={modeNoticeId}
							className="timeline-mode-tooltip"
							role="tooltip"
							data-open={modeNoticeOpen || undefined}
							data-dismissed={modeNoticeDismissed || undefined}
						>
							{modeNotice}
						</span>
					</span>
				)}
			</div>
			<label className="timeline-speed">
				<span>Speed</span>
				<select
					aria-label="Playback speed"
					value={speed}
					disabled={optionsDisabled}
					onChange={(event) => onSpeedChange(Number(event.target.value))}
				>
					{PLAYBACK_SPEEDS.map((value) => (
						<option key={value} value={value}>
							{value}×
						</option>
					))}
				</select>
			</label>
			<button
				type="button"
				className="transport-button"
				aria-label="Next step"
				disabled={navigationDisabled || !canStepForward}
				onClick={onStepForward}
			>
				→
			</button>
			<button
				type="button"
				className="transport-button"
				aria-label="Last event"
				disabled={navigationDisabled || cursor >= extent}
				onClick={() => onSeek(extent)}
			>
				↦
			</button>
		</footer>
	);
}
