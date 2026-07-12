import { PLAYBACK_SPEEDS, type PlaybackGranularity } from "./playback";

type TimelineProps = {
	cursor: number;
	detailCount: number;
	detailCursor: number;
	disabled: boolean;
	granularity: PlaybackGranularity;
	itemCount: number;
	seekCoverage: number;
	onGranularityChange: (granularity: PlaybackGranularity) => void;
	onSeek: (target: number) => void;
	onSpeedChange: (speed: number) => void;
	onStepBackward: () => void;
	onStepForward: () => void;
	speed: number;
};

export function Timeline({
	cursor,
	detailCount,
	detailCursor,
	disabled,
	granularity,
	itemCount,
	seekCoverage,
	onGranularityChange,
	onSeek,
	onSpeedChange,
	onStepBackward,
	onStepForward,
	speed,
}: TimelineProps) {
	const atFirstStep = cursor === 0 && detailCursor === 0;
	const atLastStep =
		cursor >= itemCount &&
		(detailCount === 0 || detailCursor >= detailCount - 1);

	return (
		<footer className="timeline-panel">
			<button
				type="button"
				className="transport-button"
				aria-label="First item"
				disabled={disabled || cursor === 0}
				onClick={() => onSeek(0)}
			>
				↤
			</button>
			<button
				type="button"
				className="transport-button"
				aria-label="Previous step"
				disabled={disabled || atFirstStep}
				onClick={onStepBackward}
			>
				←
			</button>
			<input
				aria-label="Timeline position"
				disabled={disabled}
				max={itemCount}
				min="0"
				onChange={(event) => onSeek(Number(event.target.value))}
				onKeyDown={(event) => {
					if (event.key === "Home") {
						event.preventDefault();
						onSeek(0);
					} else if (event.key === "End") {
						event.preventDefault();
						onSeek(itemCount);
					}
				}}
				type="range"
				value={cursor}
			/>
			<span className="timeline-readout">
				<span data-testid="timeline-readout">
					{cursor} / {itemCount}
				</span>
				{detailCount > 0 && (
					<small>
						step {detailCursor + 1}/{detailCount}
					</small>
				)}
				{itemCount > 0 && seekCoverage < itemCount && (
					<small data-testid="seek-index-progress">
						index {Math.floor((seekCoverage * 100) / itemCount)}%
					</small>
				)}
			</span>
			<label className="timeline-option">
				<span>Steps</span>
				<select
					aria-label="Playback granularity"
					disabled={disabled}
					value={granularity}
					onChange={(event) =>
						onGranularityChange(event.target.value as PlaybackGranularity)
					}
				>
					<option value="semantic">Semantic</option>
					<option value="atomic">Atomic</option>
				</select>
			</label>
			<label className="timeline-speed">
				<span>Speed</span>
				<select
					aria-label="Playback speed"
					disabled={disabled}
					value={speed}
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
				disabled={disabled || atLastStep}
				onClick={onStepForward}
			>
				→
			</button>
			<button
				type="button"
				className="transport-button"
				aria-label="Last item"
				disabled={disabled || cursor >= itemCount}
				onClick={() => onSeek(itemCount)}
			>
				↦
			</button>
		</footer>
	);
}
