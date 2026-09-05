import {
	eibfsLifecycleStage,
	eibfsNodeLabel,
	projectEibfsView,
} from "./flow-eibfs-view";
import { ibfsDistanceLabel, projectIbfsView } from "./flow-ibfs-view";
import type { FlowRenderPlan } from "./flow-render-plan";
import type { FlowCurrentSceneV9, FlowResidualArcStateV1 } from "./flow-scene";

export type FlowAggregateKind = "cluster" | "original-edge" | "residual-arc";

export type FlowEntitySelection =
	| { kind: "node"; id: string }
	| { kind: "edge"; id: string }
	| {
			kind: "residual-arc";
			id: string;
			edgeId: string;
			direction: FlowResidualArcStateV1["direction"];
	  }
	| { kind: "aggregate"; id: string; aggregateKind: FlowAggregateKind };

export type FlowEntityMatch = {
	selection: FlowEntitySelection;
	label: string;
	context: string;
};

export type FlowEntityDescription = {
	heading: string;
	rows: { label: string; value: string }[];
};

type FlowEntityNavigatorEntry = FlowEntityMatch & {
	searchId: string;
	searchText: string;
};

export type FlowEntityNavigatorModel = Readonly<{
	entries: readonly FlowEntityNavigatorEntry[];
}>;

const SELECTION_KIND_ORDER: Readonly<
	Record<FlowEntitySelection["kind"], number>
> = Object.freeze({
	node: 0,
	edge: 1,
	"residual-arc": 2,
	aggregate: 3,
});

export function flowResidualArcSelectionId(
	edgeId: string,
	direction: FlowResidualArcStateV1["direction"],
): string {
	return JSON.stringify([edgeId, direction]);
}

function overviewEntries(
	plan: FlowRenderPlan | undefined,
): FlowEntityNavigatorEntry[] {
	if (plan?.kind !== "overview") return [];
	return [
		...plan.clusters.map(
			(cluster): FlowEntityNavigatorEntry => ({
				selection: {
					kind: "aggregate",
					aggregateKind: "cluster",
					id: cluster.id,
				},
				label: cluster.id,
				context: `aggregate cluster · ${cluster.memberCount} nodes`,
				searchId: cluster.id.toLowerCase(),
				searchText:
					`${cluster.id} cluster aggregate ${cluster.memberCount} nodes`.toLowerCase(),
			}),
		),
		...plan.originalEdges.map(
			(edge): FlowEntityNavigatorEntry => ({
				selection: {
					kind: "aggregate",
					aggregateKind: "original-edge",
					id: edge.id,
				},
				label: `${edge.from} → ${edge.to}`,
				context: `original aggregate · ${edge.edgeCount} edges`,
				searchId: edge.id.toLowerCase(),
				searchText:
					`${edge.id} ${edge.from} ${edge.to} original aggregate ${edge.edgeCount} edges`.toLowerCase(),
			}),
		),
		...plan.residualArcs.map(
			(arc): FlowEntityNavigatorEntry => ({
				selection: {
					kind: "aggregate",
					aggregateKind: "residual-arc",
					id: arc.id,
				},
				label: `${arc.from} ⇢ ${arc.to}`,
				context: `residual aggregate · ${arc.arcCount} arcs`,
				searchId: arc.id.toLowerCase(),
				searchText:
					`${arc.id} ${arc.from} ${arc.to} residual aggregate ${arc.arcCount} arcs`.toLowerCase(),
			}),
		),
	];
}

/** Builds the searchable projection once per scene/render-plan revision. */
export function buildFlowEntityNavigatorModel(
	scene: FlowCurrentSceneV9,
	plan?: FlowRenderPlan,
): FlowEntityNavigatorModel {
	const entries: FlowEntityNavigatorEntry[] = [];
	for (const node of scene.graph.nodes) {
		entries.push({
			selection: { kind: "node", id: node.id },
			label: node.id,
			context: node.supply === "0" ? "node" : `node · supply ${node.supply}`,
			searchId: node.id.toLowerCase(),
			searchText: `${node.id} node ${node.supply}`.toLowerCase(),
		});
	}
	for (const edge of scene.graph.edges) {
		entries.push({
			selection: { kind: "edge", id: edge.id },
			label: edge.id,
			context: `original edge · ${edge.from} → ${edge.to}`,
			searchId: edge.id.toLowerCase(),
			searchText:
				`${edge.id} ${edge.from} ${edge.to} original edge`.toLowerCase(),
		});
	}
	for (const arc of scene.residual_arcs) {
		entries.push({
			selection: {
				kind: "residual-arc",
				id: flowResidualArcSelectionId(arc.edge_id, arc.direction),
				edgeId: arc.edge_id,
				direction: arc.direction,
			},
			label: `${arc.edge_id}:${arc.direction}`,
			context: `residual arc · ${arc.from} → ${arc.to}`,
			searchId: `${arc.edge_id}:${arc.direction}`.toLowerCase(),
			searchText:
				`${arc.edge_id} ${arc.direction} ${arc.from} ${arc.to} residual arc`.toLowerCase(),
		});
	}
	entries.push(...overviewEntries(plan));
	return { entries };
}

function matchRank(
	id: string,
	haystack: string,
	query: string,
): number | undefined {
	if (query.length === 0) return 3;
	if (id === query) return 0;
	if (id.startsWith(query)) return 1;
	if (haystack.includes(query)) return 2;
	return undefined;
}

export function searchFlowEntities(
	scene: FlowCurrentSceneV9,
	rawQuery: string,
	limit = 32,
	plan?: FlowRenderPlan,
): FlowEntityMatch[] {
	return searchFlowEntityNavigatorModel(
		buildFlowEntityNavigatorModel(scene, plan),
		rawQuery,
		limit,
	);
}

export function searchFlowEntityNavigatorModel(
	model: FlowEntityNavigatorModel,
	rawQuery: string,
	limit = 32,
): FlowEntityMatch[] {
	if (!Number.isSafeInteger(limit) || limit <= 0) return [];
	const query = rawQuery.trim().toLowerCase();
	const ranked: (FlowEntityMatch & { rank: number })[] = [];
	for (const entry of model.entries) {
		const rank = matchRank(entry.searchId, entry.searchText, query);
		if (rank !== undefined) {
			ranked.push({
				rank,
				selection: entry.selection,
				label: entry.label,
				context: entry.context,
			});
		}
	}
	ranked.sort((left, right) => {
		if (left.rank !== right.rank) return left.rank - right.rank;
		if (left.selection.kind !== right.selection.kind) {
			return (
				SELECTION_KIND_ORDER[left.selection.kind] -
				SELECTION_KIND_ORDER[right.selection.kind]
			);
		}
		return left.selection.id < right.selection.id
			? -1
			: left.selection.id > right.selection.id
				? 1
				: 0;
	});
	return ranked.slice(0, limit).map(({ rank: _rank, ...match }) => match);
}

function describeResidualArc(
	scene: FlowCurrentSceneV9,
	selection: Extract<FlowEntitySelection, { kind: "residual-arc" }>,
): FlowEntityDescription | undefined {
	if (
		selection.id !==
		flowResidualArcSelectionId(selection.edgeId, selection.direction)
	) {
		return undefined;
	}
	const arc = scene.residual_arcs.find(
		(candidate) =>
			candidate.edge_id === selection.edgeId &&
			candidate.direction === selection.direction,
	);
	if (arc === undefined) return undefined;
	return {
		heading: `Residual arc ${arc.edge_id}:${arc.direction}`,
		rows: [
			{ label: "Endpoints", value: `${arc.from} → ${arc.to}` },
			{ label: "Direction", value: arc.direction },
			{ label: "Residual capacity", value: arc.capacity },
			{ label: "Unit cost", value: arc.cost },
			{ label: "Active", value: arc.active ? "yes" : "no" },
			{ label: "Fixed", value: arc.fixed ? "yes" : "no" },
		],
	};
}

function describeAggregate(
	plan: FlowRenderPlan | undefined,
	selection: Extract<FlowEntitySelection, { kind: "aggregate" }>,
): FlowEntityDescription | undefined {
	if (plan?.kind !== "overview") return undefined;
	if (selection.aggregateKind === "cluster") {
		const cluster = plan.clusters.find(
			(candidate) => candidate.id === selection.id,
		);
		if (cluster === undefined) return undefined;
		return {
			heading: `Aggregate cluster ${cluster.id}`,
			rows: [
				{ label: "Nodes", value: cluster.memberCount.toString() },
				{ label: "Source side", value: cluster.sourceSide },
				{ label: "Terminal", value: cluster.terminal },
				{ label: "Balance", value: cluster.balance },
				{ label: "Net balance", value: cluster.netBalance.toString() },
				{ label: "Trace nodes", value: cluster.traceCount.toString() },
			],
		};
	}
	if (selection.aggregateKind === "original-edge") {
		const edge = plan.originalEdges.find(
			(candidate) => candidate.id === selection.id,
		);
		if (edge === undefined) return undefined;
		return {
			heading: `Original edge aggregate ${edge.from} → ${edge.to}`,
			rows: [
				{ label: "Original edges", value: edge.edgeCount.toString() },
				{ label: "Flow / capacity", value: `${edge.flow} / ${edge.capacity}` },
				{
					label: "Cost range",
					value: `${edge.minimumCost}…${edge.maximumCost}`,
				},
				{ label: "Active edges", value: edge.activeCount.toString() },
				{ label: "Fixed edges", value: edge.fixedCount.toString() },
				{ label: "Cut edges", value: edge.cutCount.toString() },
			],
		};
	}
	const arc = plan.residualArcs.find(
		(candidate) => candidate.id === selection.id,
	);
	if (arc === undefined) return undefined;
	return {
		heading: `Residual arc aggregate ${arc.from} → ${arc.to}`,
		rows: [
			{ label: "Residual arcs", value: arc.arcCount.toString() },
			{ label: "Capacity", value: arc.capacity.toString() },
			{ label: "Direction", value: arc.direction },
			{ label: "Active arcs", value: arc.activeCount.toString() },
			{ label: "Fixed arcs", value: arc.fixedCount.toString() },
		],
	};
}

function terminalRole(scene: FlowCurrentSceneV9, nodeId: string): string {
	if (
		scene.model.kind === "max-flow" ||
		scene.model.kind === "planar-max-flow" ||
		scene.model.kind === "fixed-flow-min-cost" ||
		scene.model.kind === "min-cost-max-flow"
	) {
		if (scene.model.source === nodeId) return "source";
		if (scene.model.sink === nodeId) return "sink";
	}
	if (
		scene.model.kind === "bipartite-matching" &&
		scene.model.flow_adapter !== undefined
	) {
		if (scene.model.flow_adapter.source === nodeId) return "adapter source";
		if (scene.model.flow_adapter.sink === nodeId) return "adapter sink";
	}
	return "internal";
}

export function describeFlowEntity(
	scene: FlowCurrentSceneV9,
	selection: FlowEntitySelection | undefined,
	plan?: FlowRenderPlan,
): FlowEntityDescription | undefined {
	if (selection === undefined) return undefined;
	if (selection.kind === "residual-arc") {
		return describeResidualArc(scene, selection);
	}
	if (selection.kind === "aggregate") {
		return describeAggregate(plan, selection);
	}
	const ibfsView = projectIbfsView(scene);
	const eibfsView = projectEibfsView(scene);
	const eibfsStage = eibfsLifecycleStage(scene);
	if (selection.kind === "node") {
		const node = scene.graph.nodes.find(
			(candidate) => candidate.id === selection.id,
		);
		if (node === undefined) return undefined;
		const trace = scene.node_trace_states.find(
			(candidate) => candidate.node_id === node.id,
		);
		const ibfsNode = ibfsView?.nodes.get(node.id);
		const ibfsRepairFocus = ibfsView?.repairFocusNodeIds.has(node.id) === true;
		const eibfsNode = eibfsView?.nodes.get(node.id);
		const eibfsRepairFocus =
			eibfsView?.repairFocusNodeIds.has(node.id) === true;
		const potential =
			scene.outcome?.kind === "min-cost-flow" ||
			scene.outcome?.kind === "min-cost-max-flow"
				? scene.outcome.potentials.find((item) => item.node_id === node.id)
						?.potential
				: scene.outcome?.kind === "assignment"
					? [...scene.outcome.agent_labels, ...scene.outcome.task_labels].find(
							(item) => item.node_id === node.id,
						)?.label
					: undefined;
		const cutSide =
			scene.outcome?.kind === "max-flow" ||
			scene.outcome?.kind === "min-cost-max-flow"
				? scene.outcome.source_side.includes(node.id)
					? "source side"
					: "sink side"
				: scene.outcome?.kind === "infeasible"
					? scene.outcome.reachable_original_nodes.includes(node.id)
						? "reachable witness side"
						: "unreachable witness side"
					: "not computed";
		const matchingRole =
			scene.model.kind === "bipartite-matching"
				? scene.model.left.includes(node.id)
					? "left"
					: scene.model.right.includes(node.id)
						? "right"
						: "flow adapter"
				: undefined;
		const assignmentRole =
			scene.model.kind === "assignment"
				? scene.model.agents.includes(node.id)
					? "agent"
					: "task"
				: undefined;
		const transportationRole =
			scene.model.kind === "transportation"
				? scene.model.origins.includes(node.id)
					? "origin"
					: "destination"
				: undefined;
		const inMinimumCover =
			scene.outcome?.kind === "bipartite-matching" &&
			(scene.outcome.cover_left.includes(node.id) ||
				scene.outcome.cover_right.includes(node.id));
		const hallRole =
			scene.outcome?.kind === "assignment-infeasible"
				? scene.outcome.hall_agents.includes(node.id)
					? "deficient agent set S"
					: scene.outcome.neighbor_tasks.includes(node.id)
						? "exact neighborhood N(S)"
						: "outside witness"
				: undefined;
		const traceRows =
			eibfsStage !== undefined
				? [
						{
							label: "EIBFS membership",
							value:
								eibfsNode?.membership === "source"
									? "S forest"
									: eibfsNode?.membership === "sink"
										? "T forest"
										: "free",
						},
						{
							label: "Retained labels dₛ / dₜ",
							value:
								eibfsNode === undefined
									? "—"
									: `${eibfsNode.source_label} / ${eibfsNode.sink_label}`,
						},
						{
							label: "Root kind",
							value: eibfsNode?.root_kind ?? "none",
						},
						{
							label: "Pseudoflow imbalance e",
							value: eibfsNode?.imbalance ?? trace?.remaining_divergence ?? "0",
						},
						{
							label: "EIBFS state",
							value:
								eibfsNode === undefined
									? eibfsStage === "certified"
										? "certified feasible max flow · forest intentionally absent"
										: eibfsStage === "recovery"
											? "same-cut recovery · forest intentionally absent"
											: "ready · forest not initialized"
									: eibfsNode.orphan
										? "orphan · awaiting adoption"
										: eibfsRepairFocus
											? "repair focus"
											: eibfsNode.frontier
												? `active ${eibfsView?.phaseDirection} frontier`
												: eibfsNodeLabel(eibfsNode),
						},
						{
							label: "Queue ordinal",
							value: trace?.search_ordinal?.toString() ?? "—",
						},
					]
				: ibfsNode !== undefined || ibfsRepairFocus
					? [
							{
								label: "IBFS tree",
								value:
									ibfsNode === undefined
										? "outside both trees"
										: ibfsNode.side === "source"
											? "S tree"
											: "T tree",
							},
							{
								label: "IBFS distance",
								value:
									ibfsNode === undefined ? "—" : ibfsDistanceLabel(ibfsNode),
							},
							{
								label: "IBFS state",
								value:
									ibfsNode === undefined
										? "removed from tree · repair focus"
										: ibfsNode.orphan
											? "orphan · awaiting adoption"
											: ibfsRepairFocus
												? "adoption focus"
												: ibfsNode.frontier
													? "active growth frontier"
													: "tree member",
							},
							{
								label: "Queue ordinal",
								value: trace?.search_ordinal?.toString() ?? "—",
							},
						]
					: scene.algorithm.id === "transportation-simplex" ||
							scene.algorithm.id === "modi"
						? [
								{
									label:
										transportationRole === "origin"
											? "Row potential u"
											: "Column potential v",
									value: trace?.label ?? "—",
								},
								{
									label: "Remaining divergence",
									value: trace?.remaining_divergence ?? "—",
								},
								{
									label: "Pricing ordinal",
									value: trace?.search_ordinal?.toString() ?? "—",
								},
							]
						: scene.algorithm.id === "auction"
							? [
									{
										label:
											assignmentRole === "agent"
												? "Scaled net benefit βₛ"
												: "Scaled task price pₛ",
										value: trace?.label ?? "—",
									},
									{
										label: "Selection ordinal",
										value: trace?.search_ordinal?.toString() ?? "—",
									},
								]
							: scene.algorithm.id === "relaxation"
								? [
										{ label: "Source price π", value: trace?.label ?? "—" },
										{
											label: "Deficit d",
											value: trace?.remaining_divergence ?? "—",
										},
										{
											label: "Label ordinal",
											value: trace?.search_ordinal?.toString() ?? "—",
										},
									]
								: scene.algorithm.id === "epsilon-relaxation"
									? [
											{
												label: "Scaled source price p̂",
												value: trace?.label ?? "—",
											},
											{
												label: "Surplus g",
												value: trace?.remaining_divergence ?? "—",
											},
											{
												label: "Selection ordinal",
												value: trace?.search_ordinal?.toString() ?? "—",
											},
										]
									: [
											{
												label: "Trace",
												value:
													trace === undefined
														? "—"
														: [
																trace.label,
																trace.search_ordinal,
																trace.remaining_divergence,
															]
																.filter((value) => value !== undefined)
																.join(" · ") || "present",
											},
										];
		return {
			heading: `Node ${node.id}`,
			rows: [
				{ label: "Role", value: terminalRole(scene, node.id) },
				...(matchingRole === undefined
					? []
					: [
							{ label: "Partition", value: matchingRole },
							{
								label: "Minimum cover",
								value: inMinimumCover ? "included" : "not included",
							},
						]),
				...(assignmentRole === undefined
					? []
					: [{ label: "Assignment partition", value: assignmentRole }]),
				...(transportationRole === undefined
					? []
					: [
							{
								label: "Transportation partition",
								value: transportationRole,
							},
						]),
				...(hallRole === undefined
					? []
					: [{ label: "Hall witness", value: hallRole }]),
				{ label: "Supply", value: node.supply },
				{
					label: "Position",
					value:
						node.position === undefined
							? "automatic"
							: `${node.position.x}, ${node.position.y}`,
				},
				{ label: "Cut side", value: cutSide },
				{ label: "Certificate potential", value: potential ?? "—" },
				...traceRows,
			],
		};
	}

	const edge = scene.graph.edges.find(
		(candidate) => candidate.id === selection.id,
	);
	if (edge === undefined) return undefined;
	const flow = scene.edge_states.find(
		(state) => state.edge_id === edge.id,
	)?.flow;
	const forward = scene.residual_arcs.find(
		(arc) => arc.edge_id === edge.id && arc.direction === "forward",
	);
	const reverse = scene.residual_arcs.find(
		(arc) => arc.edge_id === edge.id && arc.direction === "reverse",
	);
	const ibfsForwardKey = `${edge.id}:forward`;
	const ibfsReverseKey = `${edge.id}:reverse`;
	const ibfsTreeRole = ibfsView?.sourceForestArcKeys.has(ibfsForwardKey)
		? "S tree · forward parent→child"
		: ibfsView?.sourceForestArcKeys.has(ibfsReverseKey)
			? "S tree · reverse parent→child"
			: ibfsView?.sinkForestArcKeys.has(ibfsForwardKey)
				? "T tree · forward parent→child"
				: ibfsView?.sinkForestArcKeys.has(ibfsReverseKey)
					? "T tree · reverse parent→child"
					: "not a current tree edge";
	const eibfsForestArc = eibfsView?.forestArcs.find(
		(relation) => relation.admissible_residual.edge_id === edge.id,
	);
	const eibfsTreeRole =
		eibfsForestArc === undefined
			? "not a current forest edge"
			: `${eibfsForestArc.side === "source" ? "S" : "T"} forest · ${eibfsForestArc.parent} → ${eibfsForestArc.child}`;
	const eibfsResidualRole =
		eibfsForestArc === undefined
			? "—"
			: `${eibfsForestArc.admissible_residual.edge_id}:${eibfsForestArc.admissible_residual.direction} · ${eibfsForestArc.side === "source" ? "parent→child" : "child→parent"}`;
	const crossesCut =
		scene.outcome?.kind === "max-flow" ||
		scene.outcome?.kind === "min-cost-max-flow"
			? scene.outcome.source_side.includes(edge.from) &&
				!scene.outcome.source_side.includes(edge.to)
			: scene.outcome?.kind === "infeasible"
				? scene.outcome.reachable_original_nodes.includes(edge.from) !==
					scene.outcome.reachable_original_nodes.includes(edge.to)
				: false;
	const basis = scene.pseudoflow_forest?.arcs.some(
		(arc) => arc.edge_id === edge.id,
	);
	const cycleAdjustment = forward?.active
		? "+θ (increase)"
		: reverse?.active
			? "−θ (decrease)"
			: "not on active cycle";
	const matchingPair =
		scene.outcome?.kind === "bipartite-matching"
			? scene.outcome.pairs.find((pair) => pair.edge_id === edge.id)
			: undefined;
	const assignmentPair =
		scene.outcome?.kind === "assignment"
			? scene.outcome.pairs.find((pair) => pair.edge_id === edge.id)
			: undefined;
	return {
		heading: `Edge ${edge.id}`,
		rows: [
			{ label: "Endpoints", value: `${edge.from} → ${edge.to}` },
			{
				label: "Flow / capacity",
				value: `${flow ?? edge.lower} / ${edge.capacity}`,
			},
			{ label: "Lower bound", value: edge.lower },
			{ label: "Unit cost", value: edge.cost },
			...(scene.model.kind === "bipartite-matching"
				? [
						{
							label: "Matching",
							value:
								matchingPair === undefined
									? "unmatched / adapter"
									: `${matchingPair.left} ↔ ${matchingPair.right}`,
						},
					]
				: []),
			...(scene.model.kind === "assignment"
				? [
						{
							label: "Assignment",
							value:
								assignmentPair === undefined
									? "not selected"
									: `${assignmentPair.agent} → ${assignmentPair.task}`,
						},
					]
				: []),
			...(scene.model.kind === "transportation"
				? [
						{ label: "Basis", value: basis ? "basic route" : "nonbasic route" },
						{ label: "Closed-loop change", value: cycleAdjustment },
					]
				: []),
			...(ibfsView === undefined
				? []
				: [{ label: "IBFS forest", value: ibfsTreeRole }]),
			...(eibfsView === undefined
				? eibfsStage === undefined
					? []
					: [
							{
								label: "EIBFS stage",
								value:
									eibfsStage === "certified"
										? "certified feasible flow · no forest"
										: eibfsStage === "recovery"
											? "same-cut recovery path · no forest"
											: "ready · no forest",
							},
						]
				: [
						{ label: "EIBFS forest", value: eibfsTreeRole },
						{ label: "Admissible residual", value: eibfsResidualRole },
					]),
			{ label: "Residual +", value: forward?.capacity ?? "—" },
			{ label: "Residual −", value: reverse?.capacity ?? "—" },
			{
				label: "Active",
				value: forward?.active || reverse?.active ? "yes" : "no",
			},
			{
				label:
					scene.outcome?.kind === "infeasible"
						? "Witness boundary"
						: "Minimum cut",
				value: crossesCut ? "crosses cut" : "no",
			},
		],
	};
}
