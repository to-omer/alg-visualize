import * as Tooltip from "@radix-ui/react-tooltip";

import type {
	CanonicalEntry,
	Metrics,
	StructureNode,
	TraceEvent,
} from "./engine-types";
import { COMPLEXITY, INVARIANTS, PSEUDOCODE, visibleValue } from "./pedagogy";
import type { AlgorithmId } from "./scenario";

type InspectorPanelProps = {
	algorithm: AlgorithmId;
	entry: Pick<CanonicalEntry, "key" | "value"> | undefined;
	event: TraceEvent | undefined;
	inspectionKind: "empty" | "event" | "selection";
	metrics: Metrics | undefined;
	node: StructureNode | undefined;
};

export function InspectorPanel({
	algorithm,
	entry,
	event,
	inspectionKind,
	metrics,
	node,
}: InspectorPanelProps) {
	const heading =
		node !== undefined && (node.id.kind === "auxiliary" || node.keys.length > 1)
			? `${node.id.kind === "auxiliary" ? "Auxiliary" : "Node"} ${node.keys.join(" · ") || node.role}`
			: entry === undefined
				? "No entry"
				: `Key ${entry.key}`;
	const pseudocode = event === undefined ? undefined : PSEUDOCODE[event.kind];
	const metricRows =
		metrics === undefined
			? []
			: [
					["comparisons", metrics.comparisons],
					["node visits", metrics.node_visits],
					["bit tests", metrics.bit_tests],
					["rotations", metrics.rotations],
					["recolors", metrics.recolors],
					["splits", metrics.splits],
					["merges", metrics.merges],
					["rebuild items", metrics.rebuild_items],
					["allocations", metrics.allocations],
					["frees", metrics.frees],
				];

	return (
		<Tooltip.Provider delayDuration={250}>
			<aside className="inspector-panel panel">
				<div className="panel-heading">
					<div>
						<p className="eyebrow">
							{inspectionKind === "selection"
								? "SELECTION"
								: inspectionKind === "event"
									? "CURRENT EVENT"
									: "INSPECTOR"}
						</p>
						<h2>{heading}</h2>
					</div>
					<Tooltip.Root>
						<Tooltip.Trigger asChild>
							<button
								type="button"
								className="icon-button"
								aria-label="Inspector help"
							>
								?
							</button>
						</Tooltip.Trigger>
						<Tooltip.Portal>
							<Tooltip.Content className="tooltip" sideOffset={8}>
								Click a node to inspect it. Drag to pan, scroll to zoom, and use
								Fit tree to return to the full structure.
							</Tooltip.Content>
						</Tooltip.Portal>
					</Tooltip.Root>
				</div>
				<dl className="property-list">
					{node !== undefined && node.keys.length > 1 && (
						<div className="property-row-wide">
							<dt>Node entries</dt>
							<dd className="value-preview" data-testid="node-entry-list">
								{node.keys.join(" · ")}
							</dd>
						</div>
					)}
					<div className="property-row-wide">
						<dt>Value</dt>
						<dd className="value-preview" title={entry?.value}>
							{visibleValue(entry?.value)}
						</dd>
					</div>
					<div className="property-row-wide">
						<dt>Complexity</dt>
						<dd>{COMPLEXITY[algorithm]}</dd>
					</div>
					{node?.metadata.map(([name, value]) => (
						<div key={name}>
							<dt>{name}</dt>
							<dd className="accent-text">{value}</dd>
						</div>
					))}
					{metricRows.map(([name, value]) => (
						<div key={name}>
							<dt>{name}</dt>
							<dd>{value}</dd>
						</div>
					))}
				</dl>
				<div className="invariant-block">
					<p className="eyebrow">INVARIANTS</p>
					<ul>
						{INVARIANTS[algorithm].map((invariant) => (
							<li key={invariant}>{invariant}</li>
						))}
					</ul>
				</div>
				<section className="code-block" aria-label="Synchronized pseudocode">
					<span>{pseudocode?.line ?? "—"}</span>
					<div>
						<code>{pseudocode?.code ?? "awaiting_event()"}</code>
						<small>
							event {event?.catalog_id ?? "—"} · {event?.kind ?? "ready"}
						</small>
					</div>
				</section>
			</aside>
		</Tooltip.Provider>
	);
}
