import type { FlowOverlayPresentation } from "./flow-overlay-presentation";

/** Compact, source-owned status projection for every active overlay. */
export function FlowOverlayContributionStatus({
	presentation,
}: Readonly<{ presentation: FlowOverlayPresentation | undefined }>) {
	if (presentation === undefined || presentation.statusEntries.length === 0) {
		return null;
	}

	return (
		<div
			className="flow-overlay-contribution-status"
			role="status"
			aria-label="Algorithm overlay status"
		>
			{presentation.statusEntries.map((entry) => (
				<section
					key={entry.overlay}
					data-overlay-contribution-status={entry.overlay}
				>
					<strong>{entry.title}</strong>
					{entry.items.length === 0 ? (
						<span>Showing runtime state</span>
					) : (
						entry.items.map((item) => (
							<span key={item.label}>{`${item.label}: ${item.value}`}</span>
						))
					)}
				</section>
			))}
		</div>
	);
}
