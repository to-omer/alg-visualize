import { useEffect, useRef } from "react";
import type { FlowEntitySelection } from "./flow-entity-navigator";
import type { FlowCurrentSceneV9 } from "./flow-scene";

type FlowMobileEdgeSelectionProps = Readonly<{
	scene: FlowCurrentSceneV9;
	selection: FlowEntitySelection | undefined;
	showsCost: boolean;
	onSelectionChange: (selection: FlowEntitySelection) => void;
}>;

export function FlowMobileEdgeSelection({
	scene,
	selection,
	showsCost,
	onSelectionChange,
}: FlowMobileEdgeSelectionProps) {
	const laneList = useRef<HTMLFieldSetElement>(null);
	const selectedEdgeId = selection?.kind === "edge" ? selection.id : undefined;
	useEffect(() => {
		if (selectedEdgeId === undefined) return;
		laneList.current
			?.querySelector<HTMLElement>('[aria-pressed="true"]')
			?.scrollIntoView({ block: "nearest", inline: "center" });
	}, [selectedEdgeId]);
	if (selection?.kind !== "edge") return null;
	const edge = scene.graph.edges.find(
		(candidate) => candidate.id === selection.id,
	);
	if (edge === undefined) return null;
	const state = scene.edge_states.find(
		(candidate) => candidate.edge_id === edge.id,
	);
	const parallel = scene.graph.edges
		.filter(
			(candidate) => candidate.from === edge.from && candidate.to === edge.to,
		)
		.sort((left, right) => left.id.localeCompare(right.id));
	const selectedIndex = parallel.findIndex(
		(candidate) => candidate.id === edge.id,
	);
	return (
		<section
			className="flow-mobile-edge-selection"
			aria-label="Selected edge details"
		>
			<div className="flow-mobile-edge-selection-summary">
				<strong className="flow-mobile-edge-selection-id">{`${edge.id} · ${edge.from} → ${edge.to}`}</strong>
				<span className="flow-mobile-edge-selection-values">{`FLOW ${state?.flow ?? edge.initial_flow} / CAP ${edge.capacity}${showsCost ? ` · COST ${edge.cost}` : ""}`}</span>
			</div>
			{parallel.length > 1 && (
				<fieldset ref={laneList} className="flow-mobile-parallel-lanes">
					<legend className="visually-hidden">
						{`Parallel lanes ${edge.from} to ${edge.to}`}
					</legend>
					{parallel.map((candidate, index) => (
						<button
							type="button"
							key={candidate.id}
							className="flow-mobile-parallel-lane"
							aria-label={`Select lane ${index + 1} of ${parallel.length}, edge ${candidate.id}`}
							aria-pressed={index === selectedIndex}
							onClick={() =>
								onSelectionChange({ kind: "edge", id: candidate.id })
							}
						>
							{`${index + 1}/${parallel.length}`}
						</button>
					))}
				</fieldset>
			)}
		</section>
	);
}
