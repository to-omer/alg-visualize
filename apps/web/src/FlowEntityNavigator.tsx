import { useEffect, useId, useMemo, useState } from "react";
import {
	buildFlowEntityNavigatorModel,
	describeFlowEntity,
	type FlowEntitySelection,
	searchFlowEntityNavigatorModel,
} from "./flow-entity-navigator";
import type { FlowRenderPlan } from "./flow-render-plan";
import type { FlowCurrentSceneV9 } from "./flow-scene";

type Props = {
	scene: FlowCurrentSceneV9 | undefined;
	plan: FlowRenderPlan | undefined;
	selection: FlowEntitySelection | undefined;
	onSelectionChange: (selection: FlowEntitySelection | undefined) => void;
};

type EntityKindFilter = "all" | FlowEntitySelection["kind"];

function sameSelection(
	left: FlowEntitySelection | undefined,
	right: FlowEntitySelection,
): boolean {
	if (left?.kind !== right.kind || left.id !== right.id) return false;
	if (left.kind === "residual-arc" && right.kind === "residual-arc") {
		return left.edgeId === right.edgeId && left.direction === right.direction;
	}
	if (left.kind === "aggregate" && right.kind === "aggregate") {
		return left.aggregateKind === right.aggregateKind;
	}
	return true;
}

export function FlowEntityNavigator({
	scene,
	plan,
	selection,
	onSelectionChange,
}: Props) {
	const searchId = useId();
	const kindFilterId = useId();
	const [query, setQuery] = useState("");
	const [kindFilter, setKindFilter] = useState<EntityKindFilter>("all");
	const model = useMemo(
		() =>
			scene === undefined
				? undefined
				: buildFlowEntityNavigatorModel(scene, plan),
		[plan, scene],
	);
	const filteredModel = useMemo(
		() =>
			model === undefined
				? undefined
				: {
						entries:
							kindFilter === "all"
								? model.entries
								: model.entries.filter(
										(entry) => entry.selection.kind === kindFilter,
									),
					},
		[kindFilter, model],
	);
	const matches = useMemo(
		() =>
			filteredModel === undefined
				? []
				: searchFlowEntityNavigatorModel(filteredModel, query, 12),
		[filteredModel, query],
	);
	const selectedIndex = matches.findIndex((match) =>
		sameSelection(selection, match.selection),
	);
	const description = useMemo(
		() =>
			scene === undefined
				? undefined
				: describeFlowEntity(scene, selection, plan),
		[plan, scene, selection],
	);
	useEffect(() => {
		if (selection !== undefined && description === undefined) {
			onSelectionChange(undefined);
		}
	}, [description, onSelectionChange, selection]);

	return (
		<section className="flow-entity-navigator" aria-label="Entity navigator">
			<div className="flow-entity-heading">
				<h2>Entity navigator</h2>
				{selection !== undefined && (
					<button
						type="button"
						className="quiet-button"
						onClick={() => onSelectionChange(undefined)}
					>
						Clear
					</button>
				)}
			</div>
			<div className="flow-entity-search-controls">
				<label htmlFor={searchId}>
					Node / edge ID or endpoint
					<input
						id={searchId}
						type="search"
						value={query}
						onChange={(event) => setQuery(event.target.value)}
						placeholder="e.g. sa, s, t"
						disabled={scene === undefined}
					/>
				</label>
				<label htmlFor={kindFilterId}>
					Type
					<select
						id={kindFilterId}
						value={kindFilter}
						onChange={(event) =>
							setKindFilter(event.target.value as EntityKindFilter)
						}
						disabled={scene === undefined}
					>
						<option value="all">All</option>
						<option value="node">Node</option>
						<option value="edge">Original edge</option>
						<option value="residual-arc">Residual arc</option>
						<option value="aggregate">LOD aggregate</option>
					</select>
				</label>
			</div>
			<fieldset className="flow-entity-step-controls">
				<legend className="visually-hidden">Entity selection step</legend>
				<button
					type="button"
					className="quiet-button"
					disabled={selectedIndex <= 0}
					onClick={() => {
						const previous = matches[selectedIndex - 1];
						if (previous !== undefined) onSelectionChange(previous.selection);
					}}
				>
					Previous
				</button>
				<span aria-live="polite">
					{selectedIndex >= 0
						? `${selectedIndex + 1} / ${matches.length}`
						: `${matches.length} items`}
				</span>
				<button
					type="button"
					className="quiet-button"
					disabled={
						matches.length === 0 || selectedIndex === matches.length - 1
					}
					onClick={() => {
						const next = matches[selectedIndex + 1];
						if (next !== undefined) onSelectionChange(next.selection);
					}}
				>
					Next
				</button>
			</fieldset>
			<nav className="flow-entity-results" aria-label="Entity search results">
				{matches.map((match) => {
					const selected = sameSelection(selection, match.selection);
					return (
						<button
							type="button"
							className="flow-entity-result"
							aria-pressed={selected}
							key={`${match.selection.kind}:${match.selection.id}`}
							onClick={() => onSelectionChange(match.selection)}
						>
							<strong>{match.label}</strong>
							<small>{match.context}</small>
						</button>
					);
				})}
			</nav>
			{description !== undefined && (
				<div className="flow-entity-detail" aria-live="polite">
					<h3>{description.heading}</h3>
					<dl className="flow-entity-properties">
						{description.rows.map((row) => (
							<div className="flow-entity-property" key={row.label}>
								<dt>{row.label}</dt>
								<dd>{row.value}</dd>
							</div>
						))}
					</dl>
				</div>
			)}
		</section>
	);
}
