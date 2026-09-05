import type { FlowOverlayPresentation } from "./flow-overlay-presentation";

type FlowOverlayRegistryInspectorProps = Readonly<{
	presentation: FlowOverlayPresentation | undefined;
}>;

/**
 * Generated-registry fallback for newly added overlays.
 * Rich algorithm panels may coexist with this collapsed structural summary.
 */
export function FlowOverlayRegistryInspector({
	presentation,
}: FlowOverlayRegistryInspectorProps) {
	if (presentation === undefined || presentation.activeFields.length === 0) {
		return null;
	}

	return (
		<details
			className="flow-overlay-registry-inspector"
			data-active-overlay-fields={presentation.activeFields.join("|")}
		>
			<summary>
				Overlay state <span>{presentation.activeFields.length}</span>
			</summary>
			<div className="flow-overlay-registry-content">
				{presentation.inspectorSections.map((section) => {
					const legend = presentation.legendEntries.find(
						(entry) => entry.overlay === section.overlay,
					);
					const status = presentation.statusEntries.find(
						(entry) => entry.overlay === section.overlay,
					);
					return (
						<section
							key={section.overlay}
							data-overlay-inspector={section.overlay}
						>
							<h3>{section.title}</h3>
							{legend !== undefined && <p>{legend.description}</p>}
							{status !== undefined && status.items.length > 0 && (
								<ul className="flow-overlay-registry-status">
									{status.items.map((item) => (
										<li key={item.label}>{`${item.label}: ${item.value}`}</li>
									))}
								</ul>
							)}
							{section.rows.length > 0 ? (
								<dl>
									{section.rows.map((row) => (
										<div
											key={row.field}
											data-overlay-field={row.field}
											data-overlay-value={row.value}
										>
											<dt>{row.label}</dt>
											<dd>{row.value}</dd>
										</div>
									))}
								</dl>
							) : (
								<p>No scalar values or references to display.</p>
							)}
						</section>
					);
				})}
			</div>
		</details>
	);
}
