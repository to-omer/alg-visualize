import {
	FLOW_OVERLAY_CONTRIBUTION_ENTRIES,
	type FlowOverlayContributionDefinition,
} from "./flow-overlay-contribution-registry";
import {
	buildFlowOverlayRenderData,
	type FlowOverlayRenderData,
} from "./flow-overlay-render-data";
import type { FlowCurrentSceneV9 } from "./flow-scene";
import type { FlowSceneV9OverlayField } from "./flow-scene-wire/generated/overlays";

export type FlowOverlaySnapshot = Readonly<
	Pick<FlowCurrentSceneV9, FlowSceneV9OverlayField>
>;

type FlowOverlayEntityReference =
	| Readonly<{ kind: "node" | "edge"; entityId: string }>
	| Readonly<{
			kind: "residual-arc";
			entityId: string;
			direction: "forward" | "reverse";
	  }>;

export type FlowOverlayEntityMark = FlowOverlayEntityReference &
	Readonly<{
		overlay: FlowSceneV9OverlayField;
		role: string;
	}>;

export type FlowOverlayAnnotation = Readonly<{
	overlay: FlowSceneV9OverlayField;
	label: string;
	value: string;
}>;

export type FlowOverlayLegendEntry = Readonly<{
	overlay: FlowSceneV9OverlayField;
	label: string;
	description: string;
}>;

export type FlowOverlayInspectorSection = Readonly<{
	overlay: FlowSceneV9OverlayField;
	title: string;
	rows: readonly Readonly<{ field: string; label: string; value: string }>[];
}>;

export type FlowOverlayStatusEntry = Readonly<{
	overlay: FlowSceneV9OverlayField;
	title: string;
	items: readonly Readonly<{ label: string; value: string }>[];
}>;

export type FlowOverlayEntityDecoration = FlowOverlayEntityReference &
	Readonly<{
		overlay: FlowSceneV9OverlayField;
		roles: readonly string[];
		accent: "teal" | "violet" | "amber";
	}>;

export type FlowOverlayPresentation = Readonly<{
	overlays: FlowOverlaySnapshot;
	renderData: FlowOverlayRenderData;
	activeFields: readonly FlowSceneV9OverlayField[];
	marks: readonly FlowOverlayEntityMark[];
	nodeMarksById: ReadonlyMap<string, readonly FlowOverlayEntityMark[]>;
	edgeMarksById: ReadonlyMap<string, readonly FlowOverlayEntityMark[]>;
	residualArcMarksByKey: ReadonlyMap<string, readonly FlowOverlayEntityMark[]>;
	annotations: readonly FlowOverlayAnnotation[];
	legendEntries: readonly FlowOverlayLegendEntry[];
	inspectorSections: readonly FlowOverlayInspectorSection[];
	statusEntries: readonly FlowOverlayStatusEntry[];
	genericStatusEntries: readonly FlowOverlayStatusEntry[];
	genericNodeDecorations: readonly FlowOverlayEntityDecoration[];
	genericEdgeDecorations: readonly FlowOverlayEntityDecoration[];
	genericResidualArcDecorations: readonly FlowOverlayEntityDecoration[];
	accessibleDescriptions: readonly string[];
}>;

function indexMarks(
	marks: readonly FlowOverlayEntityMark[],
	kind: FlowOverlayEntityMark["kind"],
): ReadonlyMap<string, readonly FlowOverlayEntityMark[]> {
	const index = new Map<string, FlowOverlayEntityMark[]>();
	for (const mark of marks) {
		if (mark.kind !== kind) continue;
		const key =
			mark.kind === "residual-arc"
				? `${mark.entityId}:${mark.direction}`
				: mark.entityId;
		const entityMarks = index.get(key) ?? [];
		entityMarks.push(mark);
		index.set(key, entityMarks);
	}
	return index;
}

type OverlayValue = NonNullable<FlowCurrentSceneV9[FlowSceneV9OverlayField]>;

export type FlowOverlayPresenter = Readonly<{
	field: FlowSceneV9OverlayField;
	definition: FlowOverlayContributionDefinition;
	present: (overlay: OverlayValue) => Readonly<{
		marks: readonly FlowOverlayEntityMark[];
		annotations: readonly FlowOverlayAnnotation[];
		legend: FlowOverlayLegendEntry;
		inspector: FlowOverlayInspectorSection;
		status: FlowOverlayStatusEntry;
		genericDecorations: readonly FlowOverlayEntityDecoration[];
		accessibleDescription: string;
	}>;
}>;

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

const JAPANESE_FIELD_LABELS: Readonly<Record<string, string>> = {
	stage: "Stage",
	phase: "Phase",
	iteration: "Iteration",
	delta: "Flow scale Δ",
	epsilon: "Cost tolerance ε",
	scale: "Scale",
	upper_bound: "Upper bound",
	delivered: "Delivered flow",
	active_node: "Active node",
	active_arc: "Active residual arc",
};

function inspectorLabel(field: string): string {
	const japanese = JAPANESE_FIELD_LABELS[field];
	return japanese === undefined
		? `State value (${field})`
		: `${japanese} (${field})`;
}

function scalarText(value: unknown): string | undefined {
	return typeof value === "string" ||
		typeof value === "number" ||
		typeof value === "boolean"
		? String(value)
		: undefined;
}

function collectMarks(
	field: FlowSceneV9OverlayField,
	value: unknown,
	path: string,
	marks: FlowOverlayEntityMark[],
	visited: Set<object>,
): void {
	if (Array.isArray(value)) {
		value.forEach((item, index) => {
			collectMarks(field, item, `${path}[${index}]`, marks, visited);
		});
		return;
	}
	if (!isRecord(value) || visited.has(value)) return;
	visited.add(value);
	if (typeof value.node_id === "string") {
		marks.push({
			overlay: field,
			kind: "node",
			entityId: value.node_id,
			role: path,
		});
	}
	if (typeof value.edge_id === "string") {
		const direction = value.direction;
		marks.push(
			direction === "forward" || direction === "reverse"
				? {
						overlay: field,
						kind: "residual-arc",
						entityId: value.edge_id,
						direction,
						role: path,
					}
				: {
						overlay: field,
						kind: "edge",
						entityId: value.edge_id,
						role: path,
					},
		);
	}
	for (const [key, child] of Object.entries(value)) {
		if (key === "node_id" || key === "edge_id") continue;
		collectMarks(
			field,
			child,
			path === "" ? key : `${path}.${key}`,
			marks,
			visited,
		);
	}
}

/**
 * Projects scalar entity identities whose wire schema intentionally stores a
 * stable ID instead of an `{ edge_id }` / `{ node_id }` reference object.
 * Keep this list closed and schema-specific: an arbitrary string must never be
 * guessed to be a graph identity.
 */
function collectScalarIdentityMarks(
	field: FlowSceneV9OverlayField,
	overlay: OverlayValue,
	marks: FlowOverlayEntityMark[],
): void {
	if (field !== "dynamic_eibfs_overlay") return;
	const changedEdge = (overlay as { changed_edge?: unknown }).changed_edge;
	if (typeof changedEdge !== "string") return;
	marks.push({
		overlay: field,
		kind: "edge",
		entityId: changedEdge,
		role: "changed_edge",
	});
}

export function projectFlowOverlayContribution(
	definition: FlowOverlayContributionDefinition,
	overlay: OverlayValue,
): ReturnType<FlowOverlayPresenter["present"]> {
	const field = definition.field;
	const title = `${definition.title} (${field})`;
	const rows = Object.entries(overlay).flatMap(([label, value]) => {
		const text = scalarText(value);
		return text === undefined
			? []
			: [{ field: label, label: inspectorLabel(label), value: text }];
	});
	const configuredStatusItems = definition.statusFields.flatMap((label) => {
		const text = scalarText((overlay as Record<string, unknown>)[label]);
		return text === undefined
			? []
			: [{ label: inspectorLabel(label), value: text }];
	});
	const statusItems =
		configuredStatusItems.length > 0
			? configuredStatusItems
			: rows.slice(0, 3).map(({ label, value }) => ({ label, value }));
	const annotations = statusItems.map(({ label, value }) => ({
		overlay: field,
		label,
		value,
	}));
	const marks: FlowOverlayEntityMark[] = [];
	collectMarks(field, overlay, "", marks, new Set());
	collectScalarIdentityMarks(field, overlay, marks);
	const nodeReferences = marks.filter(({ kind }) => kind === "node").length;
	const edgeReferences = marks.length - nodeReferences;
	const inspectorRows = [
		...rows,
		...(nodeReferences > 0
			? [
					{
						field: "referenced_nodes",
						label: "Referenced nodes",
						value: String(nodeReferences),
					},
				]
			: []),
		...(edgeReferences > 0
			? [
					{
						field: "referenced_edges",
						label: "Referenced edges",
						value: String(edgeReferences),
					},
				]
			: []),
	];
	const genericDecorations: FlowOverlayEntityDecoration[] = [];
	if (definition.presentation.kind === "generic") {
		const grouped = new Map<string, FlowOverlayEntityMark[]>();
		for (const mark of marks) {
			const key = `${mark.kind}:${mark.entityId}:${mark.kind === "residual-arc" ? mark.direction : ""}`;
			const entityMarks = grouped.get(key) ?? [];
			entityMarks.push(mark);
			grouped.set(key, entityMarks);
		}
		for (const entityMarks of grouped.values()) {
			const first = entityMarks[0];
			if (first === undefined) continue;
			genericDecorations.push(
				first.kind === "residual-arc"
					? {
							overlay: field,
							kind: first.kind,
							entityId: first.entityId,
							direction: first.direction,
							roles: [...new Set(entityMarks.map(({ role }) => role))],
							accent: definition.presentation.accent,
						}
					: {
							overlay: field,
							kind: first.kind,
							entityId: first.entityId,
							roles: [...new Set(entityMarks.map(({ role }) => role))],
							accent: definition.presentation.accent,
						},
			);
		}
	}
	const statusText = statusItems
		.map(({ label, value }) => `${label}: ${value}`)
		.join("、");
	const referenceText = `Referenced nodes ${nodeReferences}, edges ${edgeReferences}`;
	return {
		marks,
		annotations,
		legend: {
			overlay: field,
			label: title,
			description: definition.description,
		},
		inspector: { overlay: field, title, rows: inspectorRows },
		status: { overlay: field, title: definition.title, items: statusItems },
		genericDecorations,
		accessibleDescription: `${definition.title}。${definition.description}${statusText === "" ? "" : ` ${statusText}。`} ${referenceText}。`,
	};
}

export function createFlowOverlayPresenter(
	definition: FlowOverlayContributionDefinition,
): FlowOverlayPresenter {
	return {
		field: definition.field,
		definition,
		present: (overlay) => projectFlowOverlayContribution(definition, overlay),
	};
}

/**
 * Ordered adapter view of the total contribution registry. Root React modules
 * consume only this projection and never switch on generated overlay fields.
 */
export const FLOW_OVERLAY_PRESENTERS: readonly FlowOverlayPresenter[] =
	FLOW_OVERLAY_CONTRIBUTION_ENTRIES.map(createFlowOverlayPresenter);

export function buildFlowOverlayPresentation(
	scene: FlowCurrentSceneV9,
	presenters: readonly FlowOverlayPresenter[] = FLOW_OVERLAY_PRESENTERS,
): FlowOverlayPresentation {
	const activeFields: FlowSceneV9OverlayField[] = [];
	const marks: FlowOverlayEntityMark[] = [];
	const annotations: FlowOverlayAnnotation[] = [];
	const legendEntries: FlowOverlayLegendEntry[] = [];
	const inspectorSections: FlowOverlayInspectorSection[] = [];
	const statusEntries: FlowOverlayStatusEntry[] = [];
	const genericStatusEntries: FlowOverlayStatusEntry[] = [];
	const genericNodeDecorations: FlowOverlayEntityDecoration[] = [];
	const genericEdgeDecorations: FlowOverlayEntityDecoration[] = [];
	const genericResidualArcDecorations: FlowOverlayEntityDecoration[] = [];
	const accessibleDescriptions: string[] = [];
	const overlays = Object.fromEntries(
		presenters.flatMap(({ field, definition, present }) => {
			const overlay = scene[field] as OverlayValue | undefined;
			if (overlay === undefined) return [];
			activeFields.push(field);
			const contribution = present(overlay);
			marks.push(...contribution.marks);
			annotations.push(...contribution.annotations);
			legendEntries.push(contribution.legend);
			inspectorSections.push(contribution.inspector);
			statusEntries.push(contribution.status);
			if (definition.presentation.kind === "generic") {
				genericStatusEntries.push(contribution.status);
			}
			genericNodeDecorations.push(
				...contribution.genericDecorations.filter(
					({ kind }) => kind === "node",
				),
			);
			genericEdgeDecorations.push(
				...contribution.genericDecorations.filter(
					({ kind }) => kind === "edge",
				),
			);
			genericResidualArcDecorations.push(
				...contribution.genericDecorations.filter(
					({ kind }) => kind === "residual-arc",
				),
			);
			accessibleDescriptions.push(contribution.accessibleDescription);
			return [[field, overlay] as const];
		}),
	) as FlowOverlaySnapshot;
	return {
		overlays,
		renderData: buildFlowOverlayRenderData(scene, overlays),
		activeFields,
		marks,
		nodeMarksById: indexMarks(marks, "node"),
		edgeMarksById: indexMarks(marks, "edge"),
		residualArcMarksByKey: indexMarks(marks, "residual-arc"),
		annotations,
		legendEntries,
		inspectorSections,
		statusEntries,
		genericStatusEntries,
		genericNodeDecorations,
		genericEdgeDecorations,
		genericResidualArcDecorations,
		accessibleDescriptions,
	};
}
