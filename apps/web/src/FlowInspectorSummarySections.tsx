import {
	CONVEX_COST_SCALING_ALGORITHM,
	CONVEX_NETWORK_SIMPLEX_ALGORITHM,
	isBinaryBlockingAlgorithm,
} from "./flow-algorithm-presentation";
import { flowEventCaption } from "./flow-event-caption";
import { feasibilityWorkRows } from "./flow-inspector-summary";
import type { FlowOverlayPresentation } from "./flow-overlay-presentation";
import { formatFlowRational } from "./flow-parametric-view";
import {
	flowResourceLimitMessage,
	flowResourceLimitResultLabel,
} from "./flow-resource-limit";
import type { FlowCurrentSceneV9, FlowTraceEntityRefV1 } from "./flow-scene";
import { flowTraceEntityIdentity } from "./flow-trace-entity-identity";
import type { FlowInspectorViewModel } from "./use-flow-inspector-view-model";

type FlowInspectorSectionContext = Readonly<{
	scene: FlowCurrentSceneV9 | undefined;
	presentation: FlowOverlayPresentation | undefined;
}>;

type FlowInspectorOverviewProps = FlowInspectorSectionContext &
	FlowInspectorViewModel["overview"];

type FlowInspectorContinuousProps = FlowInspectorSectionContext &
	FlowInspectorViewModel["continuous"];

function titleCaseKebab(value: string): string {
	return value
		.split("-")
		.map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
		.join(" ");
}

function workDeltaLabel(
	scene: FlowCurrentSceneV9,
	unit: NonNullable<
		FlowCurrentSceneV9["trace_event_semantics"]
	>["work_deltas"][number]["unit"],
): string {
	if (unit === "primary-work") return scene.trace_steps.primary_work.unit;
	if (unit === "detail-primitive") return "Detail primitive";
	return titleCaseKebab(unit);
}

function changedEntityLabel(entity: FlowTraceEntityRefV1): string {
	if (entity.kind === "node") return entity.node_id;
	if (entity.kind === "edge") return entity.edge_id;
	return `${entity.edge_id}:${entity.direction}`;
}

function touchedEntityLabel(
	scene: FlowCurrentSceneV9,
	entity: FlowTraceEntityRefV1,
): string {
	if (entity.kind === "node") return `node ${entity.node_id}`;
	const edge = scene.graph.edges.find(
		(candidate) => candidate.id === entity.edge_id,
	);
	const edgeLabel =
		entity.kind === "edge"
			? `edge ${entity.edge_id}`
			: `residual ${entity.edge_id}:${entity.direction}`;
	return edge === undefined
		? edgeLabel
		: `${edgeLabel} · endpoint nodes ${edge.from}, ${edge.to}`;
}

function roleLabel(
	role: NonNullable<FlowCurrentSceneV9["trace_event_semantics"]>["role"],
): string {
	switch (role) {
		case "observe":
			return "Read state";
		case "select":
			return "Select structure";
		case "mutate":
			return "Change working state";
		case "commit":
			return "Commit flow";
		case "certify":
			return "Certify result";
	}
}

export function FlowInspectorOverviewSection(
	props: FlowInspectorOverviewProps,
) {
	const scene = props.scene;
	const capacityTotal = props.capacityTotal;
	const parametricCapacityTotal = props.parametricCapacityTotal;
	const activeResidualDescription = props.activeResidualDescription;
	const fixedEdgeDescription = props.fixedEdgeDescription;
	const labelOrderDescription = props.labelOrderDescription;
	const flowLodLabel = props.flowLodLabel;
	const predictionAssistedSummary = props.predictionAssistedSummary;
	const tardosSummary = props.tardosSummary;
	const currentOverlayViews = props.presentation?.renderData.overlayViews;
	const resourceLimitResult = flowResourceLimitResultLabel(scene);
	const resourceLimitMessage = flowResourceLimitMessage(scene);
	const eventSemantics = scene?.trace_event_semantics;
	const feasibilityRows = feasibilityWorkRows(scene);
	const boundary = scene?.trace_event?.minimum_granularity;
	const primaryWorkDelta = eventSemantics?.work_deltas.find(
		(delta) => delta.unit === "primary-work",
	)?.count;
	const hasPrimaryWorkRange = eventSemantics?.primary_work_block !== undefined;
	const boundaryLabel =
		boundary === "micro"
			? "Detail"
			: boundary === "operation"
				? "Operation"
				: boundary === "phase"
					? "Phase"
					: undefined;
	const sourceBoundaryMeaning =
		boundary === "micro" &&
		scene?.trace_steps.detail.availability === "available"
			? scene.trace_steps.detail.unit
			: boundary === "operation"
				? scene?.trace_steps.operation_unit
				: boundary === "phase"
					? scene?.trace_steps.phase_unit
					: undefined;
	const workBoundaryMeaning =
		primaryWorkDelta !== undefined && hasPrimaryWorkRange
			? `${primaryWorkDelta} measured ${scene?.trace_steps.primary_work.unit ?? "primary work"} performed by this source event`
			: undefined;
	const boundaryMeaning = [sourceBoundaryMeaning, workBoundaryMeaning]
		.filter((value) => value !== undefined)
		.join(" · ");
	return (
		<dl className="property-list">
			<div>
				<dt>Algorithm</dt>
				<dd>{scene?.algorithm.id ?? "—"}</dd>
			</div>
			<div>
				<dt>Profile</dt>
				<dd>{scene?.run_profile ?? "—"}</dd>
			</div>
			<div>
				<dt>Frame</dt>
				<dd>{scene?.frame_revision ?? "—"}</dd>
			</div>
			<div>
				<dt>Render density</dt>
				<dd>{flowLodLabel}</dd>
			</div>
			<div>
				<dt>Event</dt>
				<dd>
					{scene === undefined
						? "—"
						: `${scene.event_id} / ${scene.event_count}`}
				</dd>
			</div>
			<div className="property-row-wide">
				<dt>Trace</dt>
				<dd
					data-trace-catalog-id={scene?.trace_event?.catalog_id}
					data-trace-boundary={scene?.trace_event?.minimum_granularity}
				>
					{scene?.model.kind === "parametric-max-flow" &&
					currentOverlayViews?.parametric !== undefined
						? `λ ${formatFlowRational(currentOverlayViews.parametric.parameter)} · ${currentOverlayViews.parametric.traversal?.kind ?? "initial boundary"}`
						: scene !== undefined && scene.trace_event !== undefined
							? flowEventCaption(scene)
							: "Input boundary"}
				</dd>
			</div>
			{scene?.trace_event !== undefined && (
				<div className="property-row-wide">
					<dt>Boundary</dt>
					<dd>{`${boundaryLabel} · ${boundaryMeaning}`}</dd>
				</div>
			)}
			{eventSemantics !== undefined && (
				<div>
					<dt>Effect</dt>
					<dd title={titleCaseKebab(eventSemantics.role)}>
						{roleLabel(eventSemantics.role)}
					</dd>
				</div>
			)}
			{eventSemantics !== undefined && (
				<div className="property-row-wide">
					<dt>Work delta</dt>
					<dd>
						{eventSemantics.work_deltas
							.filter((delta) => delta.unit !== "published-transition")
							.map(
								(delta) =>
									`${scene === undefined ? titleCaseKebab(delta.unit) : workDeltaLabel(scene, delta.unit)} +${delta.count}`,
							)
							.join(" · ") || "1 published transition"}
						{eventSemantics.aggregation_count === "1"
							? ""
							: ` · aggregates ${eventSemantics.aggregation_count}`}
					</dd>
				</div>
			)}
			{scene !== undefined && (
				<div className="property-row-wide">
					<dt>Primary work</dt>
					<dd>
						{`${scene.trace_steps.primary_work.unit}: ${
							eventSemantics?.work_progress.primary_completed ??
							scene.metrics[scene.trace_steps.primary_work.metric_ordinal]
						} / ${eventSemantics?.work_progress.primary_total ?? "—"} · ${titleCaseKebab(
							scene.trace_steps.primary_work.abstraction,
						)}`}
					</dd>
				</div>
			)}
			{feasibilityRows.map((row, index) => (
				<div
					key={row.label}
					className="property-row-wide"
					data-feasibility-work-summary={index === 0 || undefined}
				>
					<dt>{row.label}</dt>
					<dd>{row.value}</dd>
				</div>
			))}
			{eventSemantics !== undefined && (
				<div className="property-row-wide">
					<dt>Detail progress</dt>
					<dd>{`${eventSemantics.work_progress.detail_completed} / ${eventSemantics.work_progress.detail_total} meaningful boundaries`}</dd>
				</div>
			)}
			{eventSemantics !== undefined && (
				<div className="property-row-wide">
					<dt>Trace coverage</dt>
					<dd>{`${eventSemantics.work_progress.primary_total} exact ${scene?.trace_steps.primary_work.unit ?? "primary work units"} counted; the canvas shows only solver-published source boundaries`}</dd>
				</div>
			)}
			{scene !== undefined && eventSemantics !== undefined && (
				<div className="property-row-wide">
					<dt>Touched</dt>
					<dd
						data-event-identities={scene.trace_event?.entity_refs
							.map(flowTraceEntityIdentity)
							.join("|")}
					>
						{scene.trace_event?.entity_refs.length === 0
							? "No graph entity"
							: scene.trace_event?.entity_refs
									.map((entity) => touchedEntityLabel(scene, entity))
									.join(", ")}
					</dd>
				</div>
			)}
			{eventSemantics !== undefined && (
				<div className="property-row-wide">
					<dt>Changed</dt>
					<dd
						data-event-identities={eventSemantics.changed_entity_refs
							.map(flowTraceEntityIdentity)
							.join("|")}
					>
						{eventSemantics.changed_entity_refs.length === 0
							? "No graph entity"
							: eventSemantics.changed_entity_refs
									.map(changedEntityLabel)
									.join(", ")}
					</dd>
				</div>
			)}
			{scene?.trace_event !== undefined && (
				<div>
					<dt>Technical event</dt>
					<dd>
						{scene.trace_event.minimum_granularity} ·{" "}
						{scene.trace_event.patch_count} patches
					</dd>
				</div>
			)}
			{activeResidualDescription !== undefined &&
				activeResidualDescription.length > 0 && (
					<div>
						<dt>Active residuals</dt>
						<dd>{activeResidualDescription}</dd>
					</div>
				)}
			{fixedEdgeDescription.length > 0 && (
				<div>
					<dt>Fixed arcs</dt>
					<dd>{fixedEdgeDescription}</dd>
				</div>
			)}
			{labelOrderDescription !== undefined &&
				labelOrderDescription.length > 0 && (
					<div>
						<dt>Label order</dt>
						<dd>{labelOrderDescription}</dd>
					</div>
				)}
			{isBinaryBlockingAlgorithm(scene?.algorithm.id) && (
				<>
					<div>
						<span className="legend-binary-base-zero" aria-hidden="true" />
						<p>
							<strong>Base length 0</strong>
							<small>Long teal dashes · residual ≥ 3Δ</small>
						</p>
					</div>
					<div>
						<span className="legend-binary-special" aria-hidden="true" />
						<p>
							<strong>Special length 0</strong>
							<small>Violet dots · source-defined correction</small>
						</p>
					</div>
					<div>
						<span className="legend-binary-component" aria-hidden="true" />
						<p>
							<strong>Zero-length SCC</strong>
							<small>
								Four-color rings and C numbers · colors are visual aids
							</small>
						</p>
					</div>
				</>
			)}
			<div>
				<dt>
					{scene?.model.kind === "parametric-max-flow"
						? "Capacity sum at λ"
						: "Capacity sum"}
				</dt>
				<dd>{parametricCapacityTotal ?? capacityTotal?.toString() ?? "—"}</dd>
			</div>
			<div>
				<dt>Result</dt>
				<dd>
					{resourceLimitResult ??
						(scene?.outcome?.kind === "parametric-max-flow"
							? `${scene.outcome.segments.length} segments · ${scene.outcome.breakpoints.length} breakpoints`
							: scene?.outcome?.kind === "binary-blocking-flow"
								? `1 primitive · ${scene.outcome.delivered}/${scene.outcome.delta} flow`
								: scene?.outcome?.kind === "electrical-flow"
									? `Unit-current primitive · Rₑff ${scene.outcome.effective_resistance} · E ${scene.outcome.total_energy}`
									: scene?.outcome?.kind === "minimum-ratio-cycle"
										? `1 cycle primitive · ratio ${scene.outcome.ratio === undefined ? "—" : formatFlowRational(scene.outcome.ratio)}`
										: scene?.outcome?.kind === "minimum-ratio-cycle-mcf"
											? scene.outcome.stationary
												? "1 source progress primitive · stationary optimum face"
												: `1 source progress primitive · ratio ${scene.outcome.ratio ?? "—"} · ΔΦ ${scene.outcome.potential_decrease}`
											: scene?.outcome?.kind === "tardos-framework"
												? `1 variable-fixing primitive · ${scene.outcome.fixed_variables.length} variables fixed`
												: scene?.outcome?.kind === "max-flow"
													? `Max flow ${scene.outcome.value}`
													: scene?.outcome?.kind === "bipartite-matching"
														? `Maximum matching ${scene.outcome.cardinality}`
														: scene?.outcome?.kind === "assignment"
															? `${scene.outcome.objective === "minimize" ? "Minimum" : "Maximum"} assignment · total cost ${scene.outcome.total_cost}`
															: scene?.outcome?.kind === "assignment-infeasible"
																? `Perfect assignment infeasible · Hall deficiency ${scene.outcome.deficiency}`
																: scene?.outcome?.kind === "min-cost-max-flow"
																	? `Max flow ${scene.outcome.value} · total cost ${scene.outcome.total_cost}`
																	: scene?.outcome?.kind === "min-cost-flow"
																		? `Total cost ${scene.outcome.total_cost}`
																		: scene?.outcome?.kind === "infeasible"
																			? `Infeasible (${scene.outcome.unsatisfied})`
																			: "Not computed")}
				</dd>
			</div>
			<div>
				<dt>Certificate</dt>
				<dd>
					{resourceLimitMessage !== undefined
						? "No certificate produced"
						: scene?.outcome?.kind === "parametric-max-flow"
							? "Exact minimum and maximum source-side cuts verified over every interval"
							: scene?.outcome?.kind === "binary-blocking-flow"
								? `${scene.outcome.termination === "blocking" ? "admissible path blocked" : "Δ reached"} · primitive invariant verified · no max-flow claim`
								: scene?.outcome?.kind === "electrical-flow"
									? `KCL · Ohm · E=Rₑff · exact ${formatFlowRational(scene.outcome.exact_effective_resistance)} · error ${scene.outcome.maximum_absolute_error} · no max-flow claim`
									: scene?.outcome?.kind === "minimum-ratio-cycle"
										? "cycle space · exact ratio · matches DFS oracle · no max-flow claim"
										: scene?.outcome?.kind === "minimum-ratio-cycle-mcf"
											? scene.outcome.stationary
												? "Cost-flat face verified · no undefined gradient produced · no final MCF claim"
												: `cycle space · DFS oracle · ΔΦ≥κ²/500 (${scene.outcome.potential_decrease}≥${scene.outcome.guaranteed_decrease}) · no final MCF claim`
											: scene?.outcome?.kind === "tardos-framework"
												? `Δ(A)=${scene.outcome.determinant_bound} · fixed value for c̄>nε verified · no complete-optimization claim`
												: scene?.outcome?.kind === "max-flow"
													? `cut = ${scene.outcome.cut_bound}`
													: scene?.outcome?.kind === "bipartite-matching"
														? `minimum vertex cover = ${scene.outcome.cover_left.length + scene.outcome.cover_right.length}`
														: scene?.outcome?.kind === "assignment"
															? "dual feasible · selected edges tight · primal = dual"
															: scene?.outcome?.kind === "assignment-infeasible"
																? `Hall |S|=${scene.outcome.hall_agents.length} > |N(S)|=${scene.outcome.neighbor_tasks.length}`
																: scene?.outcome?.kind === "min-cost-max-flow"
																	? `cut = ${scene.outcome.cut_bound} · no negative cycle`
																	: scene?.outcome?.kind === "min-cost-flow"
																		? "No negative cycle"
																		: scene?.outcome?.kind === "infeasible"
																			? "Cut witness verified"
																			: "—"}
				</dd>
			</div>
			{currentOverlayViews?.binaryBlocking !== undefined && (
				<>
					<div>
						<dt>Binary phase</dt>
						<dd>{`${currentOverlayViews.binaryBlocking.stage} · F̂ ${currentOverlayViews.binaryBlocking.upper_bound} · Δ ${currentOverlayViews.binaryBlocking.delta}`}</dd>
					</div>
					<div>
						<dt>Arc classes</dt>
						<dd>{`base-0 ${currentOverlayViews.binaryBlocking.base_zero_arcs.length} · special ${currentOverlayViews.binaryBlocking.special_arcs.length} · admissible ${currentOverlayViews.binaryBlocking.admissible_arcs.length} · zero-admissible ${currentOverlayViews.binaryBlocking.zero_admissible_arcs.length}`}</dd>
					</div>
					<div>
						<dt>Zero-length SCCs</dt>
						<dd>
							{scene?.outcome?.kind === "binary-blocking-flow"
								? `${scene.outcome.component_count} total · ${scene.outcome.nontrivial_component_count} nontrivial · ${scene.outcome.augmentation_operations} lifted operations`
								: `${new Set(currentOverlayViews.binaryBlocking.nodes.map((node) => node.component)).size} components`}
						</dd>
					</div>
				</>
			)}
			{tardosSummary !== undefined && (
				<>
					<div>
						<dt>Tardos variable-fixing boundary</dt>
						<dd>{tardosSummary.overlay.stage}</dd>
					</div>
					<div>
						<dt>ε / strict threshold nε / Δ(A)</dt>
						<dd>{`${tardosSummary.overlay.epsilon} / ${tardosSummary.overlay.threshold} / ${tardosSummary.overlay.determinant_bound}`}</dd>
					</div>
					<div>
						<dt>Positive / negative residual costs</dt>
						<dd>{`${tardosSummary.positive} / ${tardosSummary.negative}`}</dd>
					</div>
					<div>
						<dt>Proved fixed variables</dt>
						<dd>
							{tardosSummary.overlay.fixed_variables.length === 0
								? "none at this boundary"
								: tardosSummary.overlay.fixed_variables
										.map(
											(fixed) =>
												`${fixed.edge_id}=${fixed.value} (${fixed.bound === "lower" ? "L" : "U"}, c̄ ${fixed.reduced_cost})`,
										)
										.join(" · ")}
						</dd>
					</div>
					<div>
						<dt>Feasibility runs / residual scans / fixes / transitions</dt>
						<dd>{`${tardosSummary.metrics[0]} / ${tardosSummary.metrics[2]} / ${tardosSummary.metrics[3]} / ${tardosSummary.metrics[15]}`}</dd>
					</div>
				</>
			)}
			{predictionAssistedSummary !== undefined && (
				<>
					<div>
						<dt>Prediction-assisted boundary</dt>
						<dd>{predictionAssistedSummary.overlay.stage}</dd>
					</div>
					<div>
						<dt>Attempt / maximum · T / t</dt>
						<dd>{`${predictionAssistedSummary.overlay.attempt}/${predictionAssistedSummary.overlay.maximum_attempt} · ${predictionAssistedSummary.overlay.exponent}/${predictionAssistedSummary.overlay.scale_exponent ?? "—"}`}</dd>
					</div>
					<div>
						<dt>Scaling c / clipped predictions</dt>
						<dd>{`${predictionAssistedSummary.overlay.scaling_parameter} / ${predictionAssistedSummary.clipped}`}</dd>
					</div>
					<div>
						<dt>Active node / ε-balanced arc</dt>
						<dd>{`${predictionAssistedSummary.overlay.active_node ?? "—"} / ${predictionAssistedSummary.overlay.active_arc === undefined ? "—" : `${predictionAssistedSummary.overlay.active_arc.edge_id}:${predictionAssistedSummary.overlay.active_arc.direction}`}`}</dd>
					</div>
					<div>
						<dt>Attempts / aborts / arc scans</dt>
						<dd>{`${predictionAssistedSummary.metrics[0]} / ${predictionAssistedSummary.metrics[1]} / ${predictionAssistedSummary.metrics[2]}`}</dd>
					</div>
					<div>
						<dt>Pushes / price raises / scales</dt>
						<dd>{`${predictionAssistedSummary.metrics[3]} / ${predictionAssistedSummary.metrics[5]} / ${predictionAssistedSummary.metrics[6]}`}</dd>
					</div>
					<div>
						<dt>Certified aligned prediction error</dt>
						<dd>
							{predictionAssistedSummary.overlay
								.certificate_aligned_prediction_error ?? "—"}
						</dd>
					</div>
				</>
			)}
			{currentOverlayViews?.cancelTighten !== undefined && (
				<>
					<div>
						<dt>Cancel / Tighten phase</dt>
						<dd>{`${currentOverlayViews.cancelTighten.stage} · phase ${currentOverlayViews.cancelTighten.phase}`}</dd>
					</div>
					<div>
						<dt>Exact ε</dt>
						<dd>
							{formatFlowRational(currentOverlayViews.cancelTighten.epsilon)}
						</dd>
					</div>
					<div>
						<dt>Admissible / active cycle</dt>
						<dd>{`${currentOverlayViews.cancelTighten.admissible_arcs.length} / ${currentOverlayViews.cancelTighten.active_cycle.length}${currentOverlayViews.cancelTighten.delta === undefined ? "" : ` · Δ ${currentOverlayViews.cancelTighten.delta}`}`}</dd>
					</div>
				</>
			)}
			{currentOverlayViews?.relaxedMndc !== undefined && (
				<>
					<div>
						<dt>Relaxed MNDC boundary</dt>
						<dd>{`${currentOverlayViews.relaxedMndc.stage} · phase ${currentOverlayViews.relaxedMndc.phase}`}</dd>
					</div>
					<div>
						<dt>Exact ε / assignment value</dt>
						<dd>{`${formatFlowRational(currentOverlayViews.relaxedMndc.epsilon)} / ${currentOverlayViews.relaxedMndc.assignment_value ?? "—"}`}</dd>
					</div>
					<div>
						<dt>Node-disjoint cycle family</dt>
						<dd>
							{currentOverlayViews.relaxedMndc.family.length === 0
								? "—"
								: currentOverlayViews.relaxedMndc.family
										.map(
											(cycle, index) =>
												`C${index + 1}: ${cycle.arcs.length} arcs · ĉ ${cycle.transformed_cost}${cycle.delta === undefined ? "" : ` · Δ ${cycle.delta}`}`,
										)
										.join(" / ")}
						</dd>
					</div>
					<div>
						<dt>Assignment arcs / identities</dt>
						<dd>{`${currentOverlayViews.relaxedMndc.nodes.filter((node) => node.selected_arc !== undefined).length} / ${currentOverlayViews.relaxedMndc.nodes.filter((node) => node.selected_arc === undefined).length}`}</dd>
					</div>
				</>
			)}
			{currentOverlayViews?.enhancedCapacityScaling !== undefined && (
				<>
					<div>
						<dt>Enhanced scaling boundary</dt>
						<dd>{`${currentOverlayViews.enhancedCapacityScaling.stage} · phase ${currentOverlayViews.enhancedCapacityScaling.phase}`}</dd>
					</div>
					<div>
						<dt>Exact Δ / quotient components</dt>
						<dd>{`${formatFlowRational(currentOverlayViews.enhancedCapacityScaling.delta)} / ${currentOverlayViews.enhancedCapacityScaling.components.length}`}</dd>
					</div>
					<div>
						<dt>Selected source / sink</dt>
						<dd>{`${currentOverlayViews.enhancedCapacityScaling.source_component ?? "—"} / ${currentOverlayViews.enhancedCapacityScaling.sink_component ?? "—"}`}</dd>
					</div>
					<div>
						<dt>Path / augmentation / contraction</dt>
						<dd>{`${currentOverlayViews.enhancedCapacityScaling.path.length} arcs / ${currentOverlayViews.enhancedCapacityScaling.augmentation === undefined ? "—" : formatFlowRational(currentOverlayViews.enhancedCapacityScaling.augmentation)} / ${currentOverlayViews.enhancedCapacityScaling.contraction_arc ?? "—"}`}</dd>
					</div>
				</>
			)}
			{currentOverlayViews?.orlinMcf !== undefined && (
				<>
					<div>
						<dt>Orlin finite-capacity boundary</dt>
						<dd>{`${currentOverlayViews.orlinMcf.stage} · phase ${currentOverlayViews.orlinMcf.phase}`}</dd>
					</div>
					<div>
						<dt>Exact Δ / transformed components</dt>
						<dd>{`${formatFlowRational(currentOverlayViews.orlinMcf.delta)} / ${currentOverlayViews.orlinMcf.components.length}`}</dd>
					</div>
					<div>
						<dt>Capacity nodes / branches</dt>
						<dd>{`${currentOverlayViews.orlinMcf.nodes.filter((node) => node.kind === "capacity").length} / ${currentOverlayViews.orlinMcf.arcs.length}`}</dd>
					</div>
					<div>
						<dt>Eliminated nodes / shortcuts</dt>
						<dd>{`${currentOverlayViews.orlinMcf.eliminated_capacity_nodes} / ${currentOverlayViews.orlinMcf.shortcut_arcs}`}</dd>
					</div>
					<div>
						<dt>Source / sink / expanded path</dt>
						<dd>{`${currentOverlayViews.orlinMcf.source_component ?? "—"} / ${currentOverlayViews.orlinMcf.sink_component ?? "—"} / ${currentOverlayViews.orlinMcf.path.length}`}</dd>
					</div>
				</>
			)}
			{currentOverlayViews?.orlinMaxFlow !== undefined && (
				<>
					<div>
						<dt>Orlin max-flow boundary</dt>
						<dd>{`${currentOverlayViews.orlinMaxFlow.stage} · phase ${scene?.metrics[0] ?? "—"} · ${currentOverlayViews.orlinMaxFlow.phase_case ?? "pending"}`}</dd>
					</div>
					<div>
						<dt>Exact Δ / Γ / threshold</dt>
						<dd>{`${currentOverlayViews.orlinMaxFlow.delta} / ${formatFlowRational(currentOverlayViews.orlinMaxFlow.gamma)} / ${currentOverlayViews.orlinMaxFlow.threshold}`}</dd>
					</div>
					<div>
						<dt>Components / critical / compactible</dt>
						<dd>{`${new Set(currentOverlayViews.orlinMaxFlow.nodes.map((node) => node.component_id)).size} / ${new Set(currentOverlayViews.orlinMaxFlow.nodes.filter((node) => node.critical).map((node) => node.component_id)).size} / ${new Set(currentOverlayViews.orlinMaxFlow.nodes.filter((node) => !node.critical).map((node) => node.component_id)).size}`}</dd>
					</div>
					<div>
						<dt>A / Ā / small / medium residuals</dt>
						<dd>{`${currentOverlayViews.orlinMaxFlow.residual_arcs.filter((arc) => arc.abundant).length} / ${currentOverlayViews.orlinMaxFlow.residual_arcs.filter((arc) => arc.anti_abundant).length} / ${currentOverlayViews.orlinMaxFlow.residual_arcs.filter((arc) => arc.small).length} / ${currentOverlayViews.orlinMaxFlow.residual_arcs.filter((arc) => arc.medium).length}`}</dd>
					</div>
					<div>
						<dt>Compact O / P / T / active path</dt>
						<dd>{`${currentOverlayViews.orlinMaxFlow.compact_arcs.filter((arc) => arc.kind === "original").length} / ${currentOverlayViews.orlinMaxFlow.compact_arcs.filter((arc) => arc.kind === "abundant-pseudo").length} / ${currentOverlayViews.orlinMaxFlow.compact_arcs.filter((arc) => arc.kind === "transferred-pseudo").length} / ${currentOverlayViews.orlinMaxFlow.active_compact_path.length}`}</dd>
					</div>
				</>
			)}
		</dl>
	);
}

export function FlowInspectorContinuousSection(
	props: FlowInspectorContinuousProps,
) {
	const scene = props.scene;
	const electricalSummary = props.electricalSummary;
	const augmentingElectricalSummary = props.augmentingElectricalSummary;
	const interiorPointSummary = props.interiorPointSummary;
	const minimumRatioSummary = props.minimumRatioSummary;
	const randomizedAlmostLinearSummary = props.randomizedAlmostLinearSummary;
	const deterministicAlmostLinearSummary =
		props.deterministicAlmostLinearSummary;
	const convexCostSummary = props.convexCostSummary;
	const convexSimplexSummary = props.convexSimplexSummary;
	const currentOverlayViews = props.presentation?.renderData.overlayViews;
	return (
		<dl className="property-list">
			{electricalSummary !== undefined && (
				<>
					<div>
						<dt>Electrical primitive boundary</dt>
						<dd>{`${electricalSummary.overlay.stage} · CG iteration ${electricalSummary.overlay.iteration}`}</dd>
					</div>
					<div>
						<dt>Residual L2 / tolerance</dt>
						<dd>{`${electricalSummary.overlay.residual_l2} / ${electricalSummary.overlay.relative_tolerance}${electricalSummary.overlay.converged ? " · converged" : ""}`}</dd>
					</div>
					<div>
						<dt>Energy / effective resistance</dt>
						<dd>{`${electricalSummary.overlay.total_energy} / ${electricalSummary.overlay.effective_resistance}`}</dd>
					</div>
					<div>
						<dt>Exact R / maximum error</dt>
						<dd>{`${electricalSummary.overlay.exact_effective_resistance === undefined ? "—" : formatFlowRational(electricalSummary.overlay.exact_effective_resistance)} / ${electricalSummary.overlay.maximum_absolute_error ?? "—"}`}</dd>
					</div>
					<div>
						<dt>Maximum congestion</dt>
						<dd>{electricalSummary.maximumCongestion.toString()}</dd>
					</div>
					<div>
						<dt>Assembly / dimension / CG / matvec</dt>
						<dd>{`${electricalSummary.metrics[0]} / ${electricalSummary.metrics[1]} / ${electricalSummary.metrics[2]} / ${electricalSummary.metrics[3]}`}</dd>
					</div>
					<div>
						<dt>Edge scans / exact pivots / checks / transitions</dt>
						<dd>{`${electricalSummary.metrics[4]} / ${electricalSummary.metrics[5]} / ${electricalSummary.metrics[6]} / ${electricalSummary.metrics[7]}`}</dd>
					</div>
				</>
			)}
			{augmentingElectricalSummary !== undefined && (
				<>
					<div>
						<dt>Augmenting-electrical boundary</dt>
						<dd>{`${augmentingElectricalSummary.overlay.stage} · α ${augmentingElectricalSummary.overlay.alpha}`}</dd>
					</div>
					<div>
						<dt>Current / remaining / target</dt>
						<dd>{`${augmentingElectricalSummary.overlay.current_value} / ${augmentingElectricalSummary.overlay.remaining} / ${augmentingElectricalSummary.overlay.working_target}`}</dd>
					</div>
					<div>
						<dt>Energy / congestion L3 / L4</dt>
						<dd>{`${augmentingElectricalSummary.overlay.electrical_energy} / ${augmentingElectricalSummary.overlay.congestion_l3} / ${augmentingElectricalSummary.overlay.congestion_l4}`}</dd>
					</div>
					<div>
						<dt>Coupling L2 / max edge congestion</dt>
						<dd>{`${augmentingElectricalSummary.overlay.coupling_l2} / ${augmentingElectricalSummary.maximumCongestion}`}</dd>
					</div>
					<div>
						<dt>Working graph / visibly boosted roots</dt>
						<dd>{`${augmentingElectricalSummary.overlay.working_nodes}v / ${augmentingElectricalSummary.overlay.working_edges}e / ${augmentingElectricalSummary.boostedRoots}`}</dd>
					</div>
					<div>
						<dt>Cuts / solves / pivots / progress</dt>
						<dd>{`${augmentingElectricalSummary.metrics[0]} / ${augmentingElectricalSummary.metrics[1]} / ${augmentingElectricalSummary.metrics[2]} / ${augmentingElectricalSummary.metrics[3]}`}</dd>
					</div>
					<div>
						<dt>Fixes / boosts / boost vertices / rounding</dt>
						<dd>{`${augmentingElectricalSummary.metrics[4]} / ${augmentingElectricalSummary.metrics[5]} / ${augmentingElectricalSummary.metrics[6]} / ${augmentingElectricalSummary.metrics[7]}`}</dd>
					</div>
					<div>
						<dt>Cleanup / extraction cycles / checks / transitions</dt>
						<dd>{`${augmentingElectricalSummary.metrics[8]} / ${augmentingElectricalSummary.metrics[9]} / ${augmentingElectricalSummary.metrics[10]} / ${augmentingElectricalSummary.metrics[11]}`}</dd>
					</div>
				</>
			)}
			{interiorPointSummary !== undefined && (
				<>
					<div>
						<dt>Interior-point boundary</dt>
						<dd>{interiorPointSummary.overlay.stage}</dd>
					</div>
					<div>
						<dt>μ / duality gap / centrality</dt>
						<dd>{`${interiorPointSummary.overlay.mu} / ${interiorPointSummary.overlay.duality_gap} / ${interiorPointSummary.overlay.centrality}`}</dd>
					</div>
					<div>
						<dt>Step / congestion L4 / energy</dt>
						<dd>{`${interiorPointSummary.overlay.step_size} / ${interiorPointSummary.overlay.congestion_l4} / ${interiorPointSummary.overlay.electrical_energy}`}</dd>
					</div>
					<div>
						<dt>B-matching Ḡ / min-cost Gᵦ</dt>
						<dd>{`${interiorPointSummary.overlay.b_matching_nodes}v/${interiorPointSummary.overlay.b_matching_edges}e · ${interiorPointSummary.overlay.working_nodes}v/${interiorPointSummary.overlay.working_edges}a`}</dd>
					</div>
					<div>
						<dt>Target / max ρ / normalized edges</dt>
						<dd>{`${interiorPointSummary.overlay.target_value} / ${interiorPointSummary.maximumCongestion} / ${interiorPointSummary.normalizedEdges}`}</dd>
					</div>
					<div>
						<dt>Cuts / electrical / pivots / progress</dt>
						<dd>{`${interiorPointSummary.metrics[0]} / ${interiorPointSummary.metrics[5]} / ${interiorPointSummary.metrics[6]} / ${interiorPointSummary.metrics[7]}`}</dd>
					</div>
					<div>
						<dt>Center / rounding / checks / transitions</dt>
						<dd>{`${interiorPointSummary.metrics[8]} / ${interiorPointSummary.metrics[9]} / ${interiorPointSummary.metrics[10]} / ${interiorPointSummary.metrics[11]}`}</dd>
					</div>
				</>
			)}
			{minimumRatioSummary !== undefined && (
				<>
					<div>
						<dt>Minimum-ratio primitive boundary</dt>
						<dd>{minimumRatioSummary.overlay.stage}</dd>
					</div>
					<div>
						<dt>Candidate / best exact ratio</dt>
						<dd>{`${minimumRatioSummary.overlay.candidate_ratio === undefined ? "—" : formatFlowRational(minimumRatioSummary.overlay.candidate_ratio)} / ${minimumRatioSummary.overlay.best_ratio === undefined ? "—" : formatFlowRational(minimumRatioSummary.overlay.best_ratio)}`}</dd>
					</div>
					<div>
						<dt>Forest / candidate / selected edges</dt>
						<dd>{`${minimumRatioSummary.treeEdges} / ${minimumRatioSummary.candidateEdges} / ${minimumRatioSummary.selectedEdges}`}</dd>
					</div>
					<div>
						<dt>Vectors / simple / fundamental cycles</dt>
						<dd>{`${minimumRatioSummary.overlay.enumerated_vectors} / ${minimumRatioSummary.overlay.simple_cycles} / ${minimumRatioSummary.overlay.fundamental_cycles}`}</dd>
					</div>
					<div>
						<dt>Comparisons / updates / DFS expansions</dt>
						<dd>{`${minimumRatioSummary.metrics[4]} / ${minimumRatioSummary.metrics[5]} / ${minimumRatioSummary.metrics[6]}`}</dd>
					</div>
					<div>
						<dt>Balance∞ / checks / transitions</dt>
						<dd>{`${minimumRatioSummary.overlay.maximum_absolute_balance} / ${minimumRatioSummary.metrics[7]} / ${minimumRatioSummary.metrics[8]}`}</dd>
					</div>
				</>
			)}
			{randomizedAlmostLinearSummary !== undefined && (
				<>
					<div>
						<dt>Randomized tree-chain boundary</dt>
						<dd>{randomizedAlmostLinearSummary.overlay.stage}</dd>
					</div>
					<div>
						<dt>Seed / draws / finite forest population</dt>
						<dd>{`${randomizedAlmostLinearSummary.overlay.seed} / ${randomizedAlmostLinearSummary.overlay.random_draws} / ${randomizedAlmostLinearSummary.overlay.forest_pool_size}`}</dd>
					</div>
					<div>
						<dt>Samples / exact miss probability</dt>
						<dd>{`${randomizedAlmostLinearSummary.overlay.sample_count} / ${randomizedAlmostLinearSummary.overlay.miss_probability.numerator}/${randomizedAlmostLinearSummary.overlay.miss_probability.denominator}`}</dd>
					</div>
					<div>
						<dt>Sampled / exact-pool min ratio</dt>
						<dd>{`${randomizedAlmostLinearSummary.overlay.selected_ratio ?? "—"} / ${randomizedAlmostLinearSummary.overlay.exact_pool_ratio ?? "—"}`}</dd>
					</div>
					<div>
						<dt>Potential / gap / IPM step / rebuild</dt>
						<dd>{`${randomizedAlmostLinearSummary.overlay.potential} / ${randomizedAlmostLinearSummary.overlay.cost_gap} / ${randomizedAlmostLinearSummary.overlay.iteration} / ${randomizedAlmostLinearSummary.overlay.rebuild_epoch}`}</dd>
					</div>
					<div>
						<dt>Isolation attempt / failure probability / scale D</dt>
						<dd>{`${randomizedAlmostLinearSummary.overlay.isolation_attempt} / ${randomizedAlmostLinearSummary.overlay.isolation_failure_probability.numerator}/${randomizedAlmostLinearSummary.overlay.isolation_failure_probability.denominator} / ${randomizedAlmostLinearSummary.overlay.isolation_scale}`}</dd>
					</div>
					<div>
						<dt>Final-point gap / threshold / mix</dt>
						<dd>{`${randomizedAlmostLinearSummary.overlay.final_point_gap ?? "—"} / ${randomizedAlmostLinearSummary.overlay.final_point_threshold} / ${randomizedAlmostLinearSummary.overlay.final_point_mix ?? "—"}`}</dd>
					</div>
					<div>
						<dt>Active tree / cycle / detected coordinates</dt>
						<dd>{`${randomizedAlmostLinearSummary.treeEdges} / ${randomizedAlmostLinearSummary.cycleEdges} / ${randomizedAlmostLinearSummary.changedCoordinates}`}</dd>
					</div>
					<div>
						<dt>Cuts / subsets / pool / seeded draws</dt>
						<dd>{`${randomizedAlmostLinearSummary.metrics[0]} / ${randomizedAlmostLinearSummary.metrics[1]} / ${randomizedAlmostLinearSummary.metrics[2]} / ${randomizedAlmostLinearSummary.metrics[3]}`}</dd>
					</div>
					<div>
						<dt>Cycles / good queries / misses / source steps</dt>
						<dd>{`${randomizedAlmostLinearSummary.metrics[4]} / ${randomizedAlmostLinearSummary.metrics[5]} / ${randomizedAlmostLinearSummary.metrics[6]} / ${randomizedAlmostLinearSummary.metrics[7]}`}</dd>
					</div>
					<div>
						<dt>Detect / rebuild / assignments / feasible flows</dt>
						<dd>{`${randomizedAlmostLinearSummary.metrics[8]} / ${randomizedAlmostLinearSummary.metrics[9]} / ${randomizedAlmostLinearSummary.metrics[10]} / ${randomizedAlmostLinearSummary.metrics[11]}`}</dd>
					</div>
					<div>
						<dt>Isolation / rounded coordinates / checks / transitions</dt>
						<dd>{`${randomizedAlmostLinearSummary.metrics[12]} / ${randomizedAlmostLinearSummary.metrics[13]} / ${randomizedAlmostLinearSummary.metrics[14]} / ${randomizedAlmostLinearSummary.metrics[15]}`}</dd>
					</div>
				</>
			)}
			{deterministicAlmostLinearSummary !== undefined && (
				<>
					<div>
						<dt>Deterministic tree-chain boundary</dt>
						<dd>{deterministicAlmostLinearSummary.overlay.stage}</dd>
					</div>
					<div>
						<dt>Active branches / passes / level</dt>
						<dd>{`${deterministicAlmostLinearSummary.overlay.active_branches.join("/")} / ${deterministicAlmostLinearSummary.overlay.passes.join("/")} / ${deterministicAlmostLinearSummary.overlay.active_level ?? "—"}`}</dd>
					</div>
					<div>
						<dt>Tree / partial forest / core / spanner</dt>
						<dd>{`${deterministicAlmostLinearSummary.treeEdges} / ${deterministicAlmostLinearSummary.forestEdges} / ${deterministicAlmostLinearSummary.coreEdges} / ${deterministicAlmostLinearSummary.spannerEdges}`}</dd>
					</div>
					<div>
						<dt>Embedding / active cycle / detected coordinates</dt>
						<dd>{`${deterministicAlmostLinearSummary.embeddedEdges} / ${deterministicAlmostLinearSummary.cycleEdges} / ${deterministicAlmostLinearSummary.changedCoordinates}`}</dd>
					</div>
					<div>
						<dt>Final-point gap / threshold / mix</dt>
						<dd>{`${deterministicAlmostLinearSummary.overlay.final_point_gap === undefined ? "—" : formatFlowRational(deterministicAlmostLinearSummary.overlay.final_point_gap)} / ${formatFlowRational(deterministicAlmostLinearSummary.overlay.final_point_threshold)} / ${deterministicAlmostLinearSummary.overlay.final_point_mix === undefined ? "—" : formatFlowRational(deterministicAlmostLinearSummary.overlay.final_point_mix)}`}</dd>
					</div>
					<div>
						<dt>Rounding forest / cycle / processed edge</dt>
						<dd>{`${deterministicAlmostLinearSummary.roundingForestEdges + Number(deterministicAlmostLinearSummary.overlay.rounding_return_forest_edge)} / ${deterministicAlmostLinearSummary.roundingCycleEdges + Number(deterministicAlmostLinearSummary.overlay.rounding_return_sign !== "0")} / ${deterministicAlmostLinearSummary.overlay.rounding_processed_edge ?? "—"}`}</dd>
					</div>
					<div>
						<dt>Selected / exact-pool ratio and cycle kind</dt>
						<dd>{`${deterministicAlmostLinearSummary.overlay.selected_ratio ?? "—"} / ${deterministicAlmostLinearSummary.overlay.exact_pool_ratio ?? "—"} / ${deterministicAlmostLinearSummary.overlay.selected_cycle_kind ?? "—"}`}</dd>
					</div>
					<div>
						<dt>Potential / gap / IPM step / rebuild</dt>
						<dd>{`${deterministicAlmostLinearSummary.overlay.potential} / ${deterministicAlmostLinearSummary.overlay.cost_gap} / ${deterministicAlmostLinearSummary.overlay.iteration} / ${deterministicAlmostLinearSummary.overlay.rebuild_epoch}`}</dd>
					</div>
					<div>
						<dt>Cuts / subsets / pool / branch records</dt>
						<dd>{`${deterministicAlmostLinearSummary.metrics[0]} / ${deterministicAlmostLinearSummary.metrics[1]} / ${deterministicAlmostLinearSummary.metrics[2]} / ${deterministicAlmostLinearSummary.metrics[3]}`}</dd>
					</div>
					<div>
						<dt>Core builds / embeddings / cycles / good queries</dt>
						<dd>{`${deterministicAlmostLinearSummary.metrics[4]} / ${deterministicAlmostLinearSummary.metrics[5]} / ${deterministicAlmostLinearSummary.metrics[6]} / ${deterministicAlmostLinearSummary.metrics[7]}`}</dd>
					</div>
					<div>
						<dt>Failures / shifts / wraps / deeper rebuilds</dt>
						<dd>{`${deterministicAlmostLinearSummary.metrics[8]} / ${deterministicAlmostLinearSummary.metrics[9]} / ${deterministicAlmostLinearSummary.metrics[10]} / ${deterministicAlmostLinearSummary.metrics[11]}`}</dd>
					</div>
					<div>
						<dt>Potential / detect / scheduled / rounding cycles</dt>
						<dd>{`${deterministicAlmostLinearSummary.metrics[12]} / ${deterministicAlmostLinearSummary.metrics[13]} / ${deterministicAlmostLinearSummary.metrics[14]} / ${deterministicAlmostLinearSummary.metrics[15]}`}</dd>
					</div>
				</>
			)}
			{currentOverlayViews?.dualNetworkSimplex !== undefined && (
				<>
					<div>
						<dt>Dual simplex boundary</dt>
						<dd>{currentOverlayViews.dualNetworkSimplex.stage}</dd>
					</div>
					<div>
						<dt>Tree / negative basic arcs</dt>
						<dd>{`${currentOverlayViews.dualNetworkSimplex.edges.filter((edge) => edge.in_tree).length} / ${currentOverlayViews.dualNetworkSimplex.edges.filter((edge) => BigInt(edge.basic_flow) < 0n).length}`}</dd>
					</div>
					<div>
						<dt>Leaving / entering</dt>
						<dd>{`${currentOverlayViews.dualNetworkSimplex.leaving_edge ?? "—"} / ${currentOverlayViews.dualNetworkSimplex.entering_edge ?? "—"}`}</dd>
					</div>
					<div>
						<dt>Head-side cut / price Δ</dt>
						<dd>{`${currentOverlayViews.dualNetworkSimplex.cut_side.length === 0 ? "—" : currentOverlayViews.dualNetworkSimplex.cut_side.join(", ")} / ${currentOverlayViews.dualNetworkSimplex.pivot_price_delta ?? "—"}`}</dd>
					</div>
				</>
			)}
			{currentOverlayViews?.polynomialDualSimplex !== undefined && (
				<>
					<div>
						<dt>Scaling-Simplex boundary</dt>
						<dd>{`${currentOverlayViews.polynomialDualSimplex.stage} · phase ${currentOverlayViews.polynomialDualSimplex.phase}`}</dd>
					</div>
					<div>
						<dt>Exact Δ / active node</dt>
						<dd>{`${formatFlowRational(currentOverlayViews.polynomialDualSimplex.delta)} / ${currentOverlayViews.polynomialDualSimplex.active_node ?? "—"}`}</dd>
					</div>
					<div>
						<dt>Tree / bad arcs / bad nodes</dt>
						<dd>{`${currentOverlayViews.polynomialDualSimplex.edges.filter((edge) => edge.in_tree).length} / ${currentOverlayViews.polynomialDualSimplex.bad_edges.length} / ${currentOverlayViews.polynomialDualSimplex.bad_nodes.length}`}</dd>
					</div>
					<div>
						<dt>Active-to-root path</dt>
						<dd>
							{currentOverlayViews.polynomialDualSimplex.augment_path.length ===
							0
								? "—"
								: currentOverlayViews.polynomialDualSimplex.augment_path
										.map((arc) => `${arc.edge_id}:${arc.direction}`)
										.join(" → ")}
						</dd>
					</div>
					<div>
						<dt>Leaving / entering / price Δ</dt>
						<dd>{`${currentOverlayViews.polynomialDualSimplex.leaving_edge ?? "—"} / ${currentOverlayViews.polynomialDualSimplex.entering_edge ?? "—"} / ${currentOverlayViews.polynomialDualSimplex.pivot_price_delta ?? "—"}`}</dd>
					</div>
					<div>
						<dt>Pivot head-side cut</dt>
						<dd>
							{currentOverlayViews.polynomialDualSimplex.pivot_cut.length === 0
								? "—"
								: currentOverlayViews.polynomialDualSimplex.pivot_cut.join(
										", ",
									)}
						</dd>
					</div>
				</>
			)}
			{currentOverlayViews?.polynomialPrimalSimplex !== undefined && (
				<>
					<div>
						<dt>Polynomial primal boundary</dt>
						<dd>{`${currentOverlayViews.polynomialPrimalSimplex.stage} · phase ${currentOverlayViews.polynomialPrimalSimplex.phase}`}</dd>
					</div>
					<div>
						<dt>Exact ε / perturbation scale</dt>
						<dd>{`${currentOverlayViews.polynomialPrimalSimplex.epsilon === undefined ? "—" : formatFlowRational(currentOverlayViews.polynomialPrimalSimplex.epsilon)} / ${currentOverlayViews.polynomialPrimalSimplex.perturbation_scale}`}</dd>
					</div>
					<div>
						<dt>Original / artificial tree arcs</dt>
						<dd>{`${currentOverlayViews.polynomialPrimalSimplex.edges.filter((edge) => edge.basis === "tree").length} / ${currentOverlayViews.polynomialPrimalSimplex.artificial_edges.filter((edge) => edge.basis === "tree").length}`}</dd>
					</div>
					<div>
						<dt>Extended N* / eligible / awake nodes</dt>
						<dd>{`${currentOverlayViews.polynomialPrimalSimplex.nodes.filter((node) => node.flags.includes("in-n-star")).length} / ${currentOverlayViews.polynomialPrimalSimplex.nodes.filter((node) => node.flags.includes("eligible")).length} / ${currentOverlayViews.polynomialPrimalSimplex.nodes.filter((node) => node.flags.includes("awake")).length}`}</dd>
					</div>
					<div>
						<dt>Artificial root q / flags</dt>
						<dd>
							{currentOverlayViews.polynomialPrimalSimplex.nodes
								.filter((node) => node.kind === "artificial-root")
								.map(
									(node) =>
										`${formatFlowRational(node.premultiplier)} / ${node.flags.length === 0 ? "—" : node.flags.join(", ")}`,
								)
								.join("")}
						</dd>
					</div>
					<div>
						<dt>Entering / leaving / cycle</dt>
						<dd>{`${currentOverlayViews.polynomialPrimalSimplex.entering?.entity_id ?? "—"} / ${currentOverlayViews.polynomialPrimalSimplex.leaving_entity ?? "—"} / ${currentOverlayViews.polynomialPrimalSimplex.cycle.length} arcs`}</dd>
					</div>
					<div>
						<dt>Exact pivot Δ / potential shift</dt>
						<dd>{`${currentOverlayViews.polynomialPrimalSimplex.delta === undefined ? "—" : formatFlowRational(currentOverlayViews.polynomialPrimalSimplex.delta)} / ${currentOverlayViews.polynomialPrimalSimplex.potential_shift === undefined ? "—" : formatFlowRational(currentOverlayViews.polynomialPrimalSimplex.potential_shift)}`}</dd>
					</div>
				</>
			)}
			{currentOverlayViews?.doubleScaling !== undefined && (
				<>
					<div>
						<dt>Double-scaling boundary</dt>
						<dd>{`${currentOverlayViews.doubleScaling.stage} · cost ${currentOverlayViews.doubleScaling.cost_phase} · capacity ${currentOverlayViews.doubleScaling.capacity_phase}`}</dd>
					</div>
					<div>
						<dt>Exact ε̂ / Δ</dt>
						<dd>{`${currentOverlayViews.doubleScaling.epsilon} / ${currentOverlayViews.doubleScaling.delta} · multiplier ${currentOverlayViews.doubleScaling.cost_multiplier}`}</dd>
					</div>
					<div>
						<dt>Admissible / active path</dt>
						<dd>{`${currentOverlayViews.doubleScaling.admissible_arcs.length} / ${currentOverlayViews.doubleScaling.active_path.length}`}</dd>
					</div>
					<div>
						<dt>Selected root / deficit</dt>
						<dd>{`${currentOverlayViews.doubleScaling.selected_root ?? "—"} / ${currentOverlayViews.doubleScaling.selected_deficit ?? "—"}`}</dd>
					</div>
				</>
			)}
			{convexSimplexSummary !== undefined && (
				<>
					<div>
						<dt>Pasche compact basis</dt>
						<dd>{convexSimplexSummary.stage}</dd>
					</div>
					<div>
						<dt>Original / artificial tree arcs</dt>
						<dd>{`${convexSimplexSummary.originalTreeEdges} / ${convexSimplexSummary.artificialTreeEdges}`}</dd>
					</div>
					<div>
						<dt>Entering / Cunningham leaving</dt>
						<dd>{`${convexSimplexSummary.entering === undefined ? "—" : `${convexSimplexSummary.entering.entity_id}:${convexSimplexSummary.entering.direction}`} / ${convexSimplexSummary.leaving === undefined ? "—" : `${convexSimplexSummary.leaving.entity_id}:${convexSimplexSummary.leaving.direction}`}`}</dd>
					</div>
					<div>
						<dt>Ordered fundamental cycle</dt>
						<dd>
							{convexSimplexSummary.cycle.length === 0
								? "—"
								: convexSimplexSummary.cycle
										.map(
											(arc) =>
												`${arc.entity_id}${arc.segment === undefined ? "" : `[${arc.segment}]`}:${arc.direction}`,
										)
										.join(" → ")}
						</dd>
					</div>
					<div>
						<dt>Crossings / one final exchange</dt>
						<dd>{`${convexSimplexSummary.metrics[3]} / ${convexSimplexSummary.metrics[4]}`}</dd>
					</div>
					<div>
						<dt>Combined / multi-crossing pivots</dt>
						<dd>{`${convexSimplexSummary.metrics[1]} / ${convexSimplexSummary.metrics[6]} · degenerate ${convexSimplexSummary.metrics[9]}`}</dd>
					</div>
				</>
			)}
			{convexCostSummary !== undefined && (
				<>
					<div>
						<dt>Convex-cost boundary</dt>
						<dd>{`${convexCostSummary.stage}${convexCostSummary.scale === undefined ? "" : ` · Δ ${convexCostSummary.scale}`}`}</dd>
					</div>
					<div>
						<dt>Edges / marginal segments</dt>
						<dd>{`${convexCostSummary.edgeCount} / ${convexCostSummary.segmentCount}`}</dd>
					</div>
					<div>
						<dt>Current convex objective</dt>
						<dd>{convexCostSummary.totalCost}</dd>
					</div>
					<div>
						<dt>Eligible / active marginal arcs</dt>
						<dd>{`${convexCostSummary.eligibleArcCount} / ${convexCostSummary.activeCycleLength}`}</dd>
					</div>
					{scene?.algorithm.id === CONVEX_NETWORK_SIMPLEX_ALGORITHM ? (
						<>
							<div>
								<dt>Pricing searches / arc scans</dt>
								<dd>{`${scene.metrics[0]} / ${scene.metrics[2]}`}</dd>
							</div>
							<div>
								<dt>Cycle scans / tree rebuilds / flips</dt>
								<dd>{`${scene.metrics[7]} / ${scene.metrics[8]} / ${scene.metrics[5]}`}</dd>
							</div>
						</>
					) : scene?.algorithm.id === CONVEX_COST_SCALING_ALGORITHM ? (
						<>
							<div>
								<dt>Δ phases / Dijkstra / augment</dt>
								<dd>{`${scene.metrics[0]} / ${scene.metrics[1]} / ${scene.metrics[3]}`}</dd>
							</div>
							<div>
								<dt>Marginal scans / saturations / breakpoints</dt>
								<dd>{`${scene.metrics[2]} / ${scene.metrics[5]} / ${scene.metrics[6]}`}</dd>
							</div>
						</>
					) : (
						<>
							<div>
								<dt>Mean-cycle search / canceled</dt>
								<dd>{`${scene?.metrics[0] ?? "—"} / ${scene?.metrics[3] ?? "—"}`}</dd>
							</div>
							<div>
								<dt>DP rounds / residual scans</dt>
								<dd>{`${scene?.metrics[1] ?? "—"} / ${scene?.metrics[2] ?? "—"}`}</dd>
							</div>
						</>
					)}
				</>
			)}
		</dl>
	);
}
