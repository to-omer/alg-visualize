type CanvasHeaderProps = {
	algorithmLabel: string;
	entityCount: number;
	entryCount: number;
	fit: () => void;
	itemCursor: number;
	lodLabel: string;
	revisionLabel: string;
	revisionStatus: "current" | "legacy-derived";
	rootKey: string | undefined;
	seekProgress: { cursor: number; target: number } | undefined;
	trackingAvailable: boolean;
	setTracking: (tracking: boolean) => void;
	tracking: boolean;
};

export function CanvasHeader({
	algorithmLabel,
	entityCount,
	entryCount,
	fit,
	itemCursor,
	lodLabel,
	revisionLabel,
	revisionStatus,
	rootKey,
	seekProgress,
	trackingAvailable,
	setTracking,
	tracking,
}: CanvasHeaderProps) {
	return (
		<div className="canvas-heading">
			<div>
				<p className="eyebrow">LIVE STRUCTURE</p>
				<h2>{algorithmLabel}</h2>
			</div>
			<div className="canvas-meta">
				<span className="entry-count">{entryCount} entries</span>
				<span>{entityCount} entities</span>
				<span data-testid="root-key">root {rootKey ?? "—"}</span>
				<span className="lod-chip" data-testid="lod-indicator">
					{lodLabel}
				</span>
				<span
					className="lod-chip"
					data-testid="revision-status"
					title={
						revisionStatus === "legacy-derived"
							? "Imported declarations are preserved; this build produces current trace and projection revisions."
							: undefined
					}
				>
					{revisionLabel}
				</span>
				<span className="live-chip" data-testid="seek-progress">
					{seekProgress === undefined
						? `item ${itemCursor}`
						: `seeking ${Math.floor((seekProgress.cursor * 100) / seekProgress.target)}%`}
				</span>
				<button type="button" className="canvas-tool" onClick={fit}>
					Fit tree
				</button>
				<button
					type="button"
					aria-pressed={tracking}
					className="canvas-tool"
					disabled={!trackingAvailable}
					onClick={() => setTracking(!tracking)}
				>
					Follow execution
				</button>
			</div>
		</div>
	);
}
