import { createHash } from "node:crypto";
import {
	existsSync,
	mkdirSync,
	readdirSync,
	readFileSync,
	renameSync,
	statSync,
	writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Locator, Page } from "@playwright/test";
import { FLOW_OVERLAY_CONTRIBUTIONS } from "../../apps/web/src/flow-overlay-contribution-registry";
import { FLOW_LOD_LIMITS } from "../../apps/web/src/flow-render-plan";
import { FLOW_SCENE_V9_OVERLAY_DECODERS } from "../../apps/web/src/flow-scene-wire/generated/overlays/index";
import { expect, test } from "./browser-test";
import { FLOW_BROWSER_ALGORITHM_IDS } from "./flow-browser-coverage";

type AuditCase = Readonly<{
	algorithm_id: string;
	label: string;
	node_count: number;
	edge_count: number;
	event_count: number;
	phase_count: number;
	operation_count: number;
	detail_count: number;
	primary_work: string;
	primary_work_boundary_count: string;
	primary_work_unit: string;
	primary_work_abstraction: "primitive" | "iteration" | "oracle-call";
	maximum_primary_work_delta: string;
	first_detail: AuditBoundaryWitness;
	middle_detail: AuditBoundaryWitness;
	last_detail: AuditBoundaryWitness;
	first_primary_work: AuditBoundaryWitness;
	maximum_aggregation: AuditBoundaryWitness;
	maximum_primary_work: AuditBoundaryWitness;
	overlay_witnesses: Readonly<Record<string, AuditBoundaryWitness>>;
	scenario_digest: string;
	trace_digest: string;
	scenario: {
		payload: {
			model: { kind: string };
			graph: {
				edges: readonly Readonly<{ capacity: string; cost: string }>[];
			};
		};
	};
}>;

type AuditBoundaryWitness = Readonly<{
	event: number;
	catalog_id: string;
	primary_delta: string;
	primary_completed: string;
	detail_completed: string;
	aggregation: string;
	work_deltas: readonly Readonly<{ unit: string; count: string }>[];
	work_first: string | null;
	work_last: string | null;
	work_total: string | null;
	active_overlays: readonly string[];
	overlay_scalar_values: Readonly<
		Record<string, Readonly<Record<string, string>>>
	>;
	touched_identities: readonly string[];
	changed_identities: readonly string[];
}>;

type ScreenshotWitness = "early" | "middle" | "late";

type AuditManifest = Readonly<{
	schema_version: 17;
	algorithm_count: number;
	cases_per_algorithm: 3;
	complexity_growth: readonly Readonly<{
		algorithm_id: string;
		driver: "graph-entity-count" | "maximum-absolute-cost" | "maximum-capacity";
		controlled_family: string;
		controlled: boolean;
		control_contract: string;
		control_digest: string;
		smaller_driver: string;
		larger_driver: string;
		smaller_label: string;
		larger_label: string;
		smaller_primary_work: string;
		larger_primary_work: string;
		smaller_primary_boundary_count: string;
		larger_primary_boundary_count: string;
		smaller_event_count: number;
		larger_event_count: number;
		smaller_detail_count: number;
		larger_detail_count: number;
		smaller_node_count: number;
		larger_node_count: number;
		smaller_edge_count: number;
		larger_edge_count: number;
	}>[];
	cases: readonly AuditCase[];
}>;

const manifestPath = fileURLToPath(
	new URL("../../fixtures/flow-representative-audit.json", import.meta.url),
);
const manifest = JSON.parse(
	readFileSync(manifestPath, "utf8"),
) as AuditManifest;
const selectedAuditAlgorithm =
	process.env.FLOW_BROWSER_REPRESENTATIVE_ALGORITHM;
const representativeReleaseAudit =
	process.env.FLOW_BROWSER_REPRESENTATIVE_AUDIT === "1";
const representativePartialAudit =
	process.env.FLOW_BROWSER_REPRESENTATIVE_PARTIAL === "1";
const representativeDiagnosticAudit =
	process.env.FLOW_BROWSER_REPRESENTATIVE_DIAGNOSTIC === "1";
if (
	[
		representativeReleaseAudit,
		representativePartialAudit,
		representativeDiagnosticAudit,
	].filter(Boolean).length > 1
) {
	throw new Error(
		"Representative release, partial, and diagnostic audit modes are mutually exclusive",
	);
}
if (representativeReleaseAudit && selectedAuditAlgorithm !== undefined) {
	throw new Error(
		"The release representative audit cannot be filtered; use FLOW_BROWSER_REPRESENTATIVE_PARTIAL=1 for one-algorithm diagnostics",
	);
}
if (representativePartialAudit && selectedAuditAlgorithm === undefined) {
	throw new Error(
		"FLOW_BROWSER_REPRESENTATIVE_PARTIAL requires FLOW_BROWSER_REPRESENTATIVE_ALGORITHM",
	);
}
if (representativeDiagnosticAudit && selectedAuditAlgorithm !== undefined) {
	throw new Error(
		"The diagnostic representative audit always covers the complete catalog",
	);
}
if (
	selectedAuditAlgorithm !== undefined &&
	!FLOW_BROWSER_ALGORITHM_IDS.some(
		(algorithmId) => algorithmId === selectedAuditAlgorithm,
	)
) {
	throw new Error(
		`Unknown FLOW_BROWSER_REPRESENTATIVE_ALGORITHM: ${selectedAuditAlgorithm}`,
	);
}
const auditAlgorithmIds =
	selectedAuditAlgorithm === undefined
		? FLOW_BROWSER_ALGORITHM_IDS
		: [selectedAuditAlgorithm];
const representativeDebug =
	process.env.FLOW_BROWSER_REPRESENTATIVE_DEBUG === "1";
const representativeStartEventText =
	process.env.FLOW_BROWSER_REPRESENTATIVE_START_EVENT;
if (
	representativeStartEventText !== undefined &&
	(!representativePartialAudit ||
		!/^(0|[1-9][0-9]*)$/u.test(representativeStartEventText))
) {
	throw new Error(
		"FLOW_BROWSER_REPRESENTATIVE_START_EVENT is a partial-audit-only nonnegative integer",
	);
}
const representativeStartEvent = Number(representativeStartEventText ?? "0");
if (!Number.isSafeInteger(representativeStartEvent)) {
	throw new Error(
		"FLOW_BROWSER_REPRESENTATIVE_START_EVENT exceeds safe integer range",
	);
}
const advancedGraphOverlayFields = new Set([
	"flow_framework_mcf_overlay",
	"minimum_ratio_cycle_mcf_overlay",
	"primal_dual_ipm_mcf_overlay",
	"randomized_almost_linear_mcf_overlay",
	"weighted_augmenting_paths_overlay",
	"weighted_push_relabel_shortcut_overlay",
]);
const screenshotAuditDirectory = process.env.FLOW_BROWSER_SCREENSHOT_AUDIT_DIR;
if (!representativeReleaseAudit && screenshotAuditDirectory !== undefined) {
	throw new Error(
		"Only the unfiltered release audit may write retained screenshot artifacts",
	);
}
const screenshotAuditRecords: Array<{
	algorithm_id: string;
	case_label: string;
	witness: ScreenshotWitness;
	event: number;
	file: string;
	byte_size: number;
	sha256: string;
	graph_projection_sha256: string;
}> = [];
const casesByAlgorithm = new Map<string, AuditCase[]>();
for (const auditCase of manifest.cases) {
	const cases = casesByAlgorithm.get(auditCase.algorithm_id) ?? [];
	cases.push(auditCase);
	casesByAlgorithm.set(auditCase.algorithm_id, cases);
}

function largestVisualAuditCase(auditCases: readonly AuditCase[]): AuditCase {
	const first = auditCases[0];
	if (first === undefined) throw new Error("visual audit requires one case");
	return auditCases.slice(1).reduce((largest, candidate) => {
		if (candidate.edge_count !== largest.edge_count) {
			return candidate.edge_count > largest.edge_count ? candidate : largest;
		}
		return candidate.node_count > largest.node_count ? candidate : largest;
	}, first);
}

function screenshotCandidateWitnesses(
	auditCase: AuditCase,
): readonly AuditBoundaryWitness[] {
	return [
		auditCase.first_primary_work,
		auditCase.first_detail,
		auditCase.maximum_aggregation,
		auditCase.middle_detail,
		auditCase.maximum_primary_work,
		auditCase.last_detail,
	]
		.filter(
			(candidate, index, candidates) =>
				candidates.findIndex((item) => item.event === candidate.event) ===
				index,
		)
		.sort((left, right) => left.event - right.event);
}

function pngDimensions(
	bytes: Buffer,
): Readonly<{ width: number; height: number }> {
	const signature = "89504e470d0a1a0a";
	if (
		bytes.byteLength < 24 ||
		bytes.subarray(0, 8).toString("hex") !== signature ||
		bytes.subarray(12, 16).toString("ascii") !== "IHDR"
	) {
		throw new Error("visual audit artifact is not a canonical PNG");
	}
	const width = bytes.readUInt32BE(16);
	const height = bytes.readUInt32BE(20);
	if (width === 0 || height === 0) {
		throw new Error("visual audit PNG has an empty IHDR extent");
	}
	return { width, height };
}

const richPanelByAlgorithm = new Map<
	string,
	Readonly<{ testId: string; overlay: string }>
>([
	[
		"weighted-augmenting-paths",
		{
			testId: "flow-weighted-augmenting-paths-panel",
			overlay: "weighted_augmenting_paths_overlay",
		},
	],
	[
		"weighted-push-relabel",
		{
			testId: "flow-weighted-push-relabel-panel",
			overlay: "weighted_push_relabel_shortcut_overlay",
		},
	],
	[
		"primal-dual-interior-point-mcf",
		{
			testId: "flow-ipm-mcf-panel",
			overlay: "primal_dual_ipm_mcf_overlay",
		},
	],
	[
		"electrical-flow-interior-point-mcf",
		{
			testId: "flow-electrical-ipm-mcf-panel",
			overlay: "electrical_ipm_mcf_overlay",
		},
	],
	[
		"minimum-ratio-cycle-mcf",
		{
			testId: "flow-minimum-ratio-cycle-mcf-panel",
			overlay: "minimum_ratio_cycle_mcf_overlay",
		},
	],
	[
		"randomized-almost-linear-mcf-oracle-demonstrator",
		{
			testId: "flow-randomized-almost-linear-mcf-oracle-demonstrator-panel",
			overlay: "randomized_almost_linear_mcf_overlay",
		},
	],
	[
		"deterministic-almost-linear-mcf",
		{
			testId: "flow-deterministic-almost-linear-mcf-panel",
			overlay: "flow_framework_mcf_overlay",
		},
	],
] as const);

function workspace(page: Page): Locator {
	return page.locator("[data-workspace-id]:not([hidden])");
}

function renderedFlowGraph(page: Page): Locator {
	return workspace(page).locator('svg.flow-graph[role="img"]');
}

async function visibleGraphProjection(graph: Locator): Promise<string> {
	return graph.locator("*").evaluateAll((elements) => {
		const rounded = (value: number) => Math.round(value * 100) / 100;
		const geometryAttributes = [
			"d",
			"x",
			"y",
			"x1",
			"y1",
			"x2",
			"y2",
			"cx",
			"cy",
			"r",
			"rx",
			"ry",
			"width",
			"height",
			"points",
			"transform",
		] as const;
		return JSON.stringify(
			elements.flatMap((element) => {
				if (!(element instanceof SVGGraphicsElement)) return [];
				if (element.closest(".flow-overlay-stage-badge") !== null) return [];
				const style = getComputedStyle(element);
				const box = element.getBoundingClientRect();
				if (
					style.display === "none" ||
					style.visibility === "hidden" ||
					Number(style.opacity) === 0 ||
					(box.width <= 0 && box.height <= 0)
				) {
					return [];
				}
				const geometry = geometryAttributes.flatMap((attribute) => {
					const value = element.getAttribute(attribute);
					return value === null ? [] : [[attribute, value] as const];
				});
				const tag = element.tagName.toLowerCase();
				return [
					{
						tag,
						geometry,
						box: [
							rounded(box.x),
							rounded(box.y),
							rounded(box.width),
							rounded(box.height),
						],
						paint: [
							style.fill,
							style.fillOpacity,
							style.stroke,
							style.strokeOpacity,
							style.strokeWidth,
							style.strokeDasharray,
							style.opacity,
						],
						text:
							tag === "text" || tag === "tspan"
								? (element.textContent ?? "").trim()
								: "",
					},
				];
			}),
		);
	});
}

async function selectDistinctScreenshotWitnesses(
	page: Page,
	auditCase: AuditCase,
): Promise<
	readonly Readonly<{
		name: ScreenshotWitness;
		boundary: AuditBoundaryWitness;
		graphProjectionSha256: string;
	}>[]
> {
	const distinct: Array<{
		boundary: AuditBoundaryWitness;
		graphProjectionSha256: string;
	}> = [];
	const seenProjections = new Set<string>();
	for (const boundary of screenshotCandidateWitnesses(auditCase)) {
		await seekRawEvent(page, boundary.event, auditCase.event_count);
		const projection = await visibleGraphProjection(renderedFlowGraph(page));
		const graphProjectionSha256 = createHash("sha256")
			.update(projection)
			.digest("hex");
		if (seenProjections.has(graphProjectionSha256)) continue;
		seenProjections.add(graphProjectionSha256);
		distinct.push({ boundary, graphProjectionSha256 });
	}
	expect(
		distinct.length,
		`${auditCase.algorithm_id}/${auditCase.label} needs three visibly distinct screenshot witnesses`,
	).toBeGreaterThanOrEqual(3);
	const early = distinct[0];
	const late = distinct.at(-1);
	if (early === undefined || late === undefined || early === late) {
		throw new Error(
			`${auditCase.algorithm_id}/${auditCase.label} screenshot endpoints are incomplete`,
		);
	}
	const temporalMiddle = (early.boundary.event + late.boundary.event) / 2;
	const middle = distinct
		.slice(1, -1)
		.sort(
			(left, right) =>
				Math.abs(left.boundary.event - temporalMiddle) -
				Math.abs(right.boundary.event - temporalMiddle),
		)[0];
	if (middle === undefined) {
		throw new Error(
			`${auditCase.algorithm_id}/${auditCase.label} screenshot midpoint is incomplete`,
		);
	}
	return [
		{ name: "early", ...early },
		{ name: "middle", ...middle },
		{ name: "late", ...late },
	];
}

function problemFor(modelKind: string): "Max Flow" | "Min-Cost Flow" {
	return new Set([
		"max-flow",
		"parametric-max-flow",
		"bipartite-matching",
		"planar-max-flow",
	]).has(modelKind)
		? "Max Flow"
		: "Min-Cost Flow";
}

function measuredComplexityDriver(
	driver: AuditManifest["complexity_growth"][number]["driver"],
	auditCase: AuditCase,
): bigint {
	if (driver === "graph-entity-count") {
		return BigInt(auditCase.node_count + auditCase.edge_count);
	}
	const values = auditCase.scenario.payload.graph.edges.map((edge) => {
		if (driver === "maximum-capacity") return BigInt(edge.capacity);
		const cost = BigInt(edge.cost);
		return cost < 0n ? -cost : cost;
	});
	return values.reduce(
		(maximum, value) => (value > maximum ? value : maximum),
		0n,
	);
}

async function openProblem(
	page: Page,
	problem: "Max Flow" | "Min-Cost Flow",
): Promise<void> {
	await page.goto("/");
	await page.getByRole("button", { name: problem, exact: true }).click();
	await expect(
		workspace(page).getByRole("heading", { name: problem, level: 1 }),
	).toBeVisible();
	await expect(workspace(page).locator(".flow-status")).toHaveText(
		"Validated",
		{
			timeout: 30_000,
		},
	);
	await expect(renderedFlowGraph(page)).toBeVisible();
}

async function loadAuditCase(page: Page, auditCase: AuditCase): Promise<void> {
	const current = workspace(page);
	await current
		.getByRole("textbox", { name: "Flow Scenario JSON" })
		.fill(JSON.stringify(auditCase.scenario, null, 2));
	await expect(current.locator(".flow-status")).toHaveText("Edited");
	await current.getByRole("button", { name: "Load", exact: true }).click();
	await expect(current.locator(".flow-status")).toHaveText("Validated", {
		timeout: 30_000,
	});
	await expect(renderedFlowGraph(page)).toBeVisible();
	await expect(
		current
			.getByLabel("Flow scene inspector")
			.locator("dt", { hasText: "Algorithm" })
			.locator(".."),
	).toContainText(auditCase.algorithm_id);
}

async function prepareTrace(page: Page): Promise<number> {
	const current = workspace(page);
	await current.getByRole("button", { name: "Run trace" }).click();
	const rawReadout = current.getByTestId("flow-timeline-readout");
	await expect(rawReadout).toHaveText(/^Raw [1-9][0-9]* \/ [1-9][0-9]*$/, {
		timeout: 60_000,
	});
	const pause = current.getByRole("button", { name: "Pause", exact: true });
	if (await pause.isVisible()) await pause.click();
	await current.getByRole("button", { name: "First event" }).click();
	await expect(rawReadout).toHaveText(/^Raw 0 \/ [1-9][0-9]*$/);
	const match = /^Raw 0 \/ ([1-9][0-9]*)$/.exec(
		(await rawReadout.textContent()) ?? "",
	);
	if (match === null) throw new Error("Trace extent is unavailable");
	return Number(match[1]);
}

async function selectDetailBoundary(page: Page): Promise<void> {
	const select = workspace(page).getByRole("combobox", {
		name: "Playback granularity",
	});
	await expect(select.locator('option[value="micro"]')).toHaveAttribute(
		"aria-disabled",
		"false",
	);
	await select.selectOption("micro");
	await expect(select).toHaveValue("micro");
}

async function seekRawEvent(
	page: Page,
	event: number,
	eventCount: number,
): Promise<void> {
	const current = workspace(page);
	const slider = current.locator(".flow-timeline input[type='range']");
	await expect(slider).toBeEnabled({ timeout: 60_000 });
	await slider.fill(String(event));
	await expect(current.getByTestId("flow-timeline-readout")).toHaveText(
		`Raw ${event} / ${eventCount}`,
		{ timeout: 60_000 },
	);
	await expect(slider).toBeEnabled({ timeout: 60_000 });
}

async function assertDeclaredBoundaryAvailability(
	page: Page,
	auditCase: AuditCase,
): Promise<void> {
	const select = workspace(page).getByRole("combobox", {
		name: "Playback granularity",
	});
	const phase = select.locator('option[value="phase"]');
	const phaseAvailability = await phase.getAttribute("aria-disabled");
	expect(["true", "false"]).toContain(phaseAvailability);
	if (auditCase.phase_count > 0) {
		await expect(phase).toHaveAttribute("aria-disabled", "false");
	} else if (phaseAvailability === "true") {
		await expect(phase).toHaveAttribute("disabled", "");
		await expect(phase).toContainText("unavailable");
	}
	const operation = select.locator('option[value="operation"]');
	const operationAvailability = await operation.getAttribute("aria-disabled");
	expect(["true", "false"]).toContain(operationAvailability);
	if (auditCase.operation_count > 0) {
		await expect(operation).toHaveAttribute("aria-disabled", "false");
	} else if (operationAvailability === "true") {
		await expect(operation).toHaveAttribute("disabled", "");
		await expect(operation).toContainText("unavailable");
	}
}

async function stepToAuditedDetail(
	page: Page,
	auditCase: AuditCase,
): Promise<void> {
	const current = workspace(page);
	expect(auditCase.first_detail.event).toBeGreaterThan(0);
	expect(auditCase.first_detail.event).toBeLessThanOrEqual(
		auditCase.event_count,
	);
	const preceding = auditCase.first_detail.event - 1;
	await seekRawEvent(page, preceding, auditCase.event_count);
	const next = current.getByRole("button", { name: "Next step" });
	await expect(next).toBeEnabled();
	await next.click();
	await expect(current.getByTestId("flow-timeline-readout")).toHaveText(
		`Raw ${auditCase.first_detail.event} / ${auditCase.event_count}`,
	);
	await expect(
		current
			.getByLabel("Flow scene inspector")
			.locator("dt", { hasText: /^Boundary$/u })
			.locator(".."),
	).toContainText(/Phase|Operation|Detail/u);
	await assertAuditedWorkBoundary(page, auditCase, auditCase.first_detail);
}

async function assertDetailBackwardRoundTrip(
	page: Page,
	auditCase: AuditCase,
	witness: AuditBoundaryWitness,
): Promise<void> {
	const current = workspace(page);
	await seekRawEvent(page, witness.event, auditCase.event_count);
	await assertAuditedWorkBoundary(page, auditCase, witness);
	const witnessProjection = await visibleGraphProjection(
		renderedFlowGraph(page),
	);
	const previous = current.getByRole("button", { name: "Previous step" });
	await expect(previous).toBeEnabled();
	await previous.click();
	await expect(current.getByTestId("flow-timeline-readout")).not.toHaveText(
		`Raw ${witness.event} / ${auditCase.event_count}`,
	);
	const previousCatalogId =
		(await current
			.getByLabel("Flow scene inspector")
			.locator("[data-trace-catalog-id]")
			.getAttribute("data-trace-catalog-id")) ?? "input-boundary";
	expect(
		await visibleGraphProjection(renderedFlowGraph(page)),
		`${auditCase.algorithm_id}/${auditCase.label} previous source boundary ${previousCatalogId} must look different from ${witness.catalog_id}`,
	).not.toBe(witnessProjection);
	const next = current.getByRole("button", { name: "Next step" });
	await expect(next).toBeEnabled();
	await next.click();
	await assertAuditedWorkBoundary(page, auditCase, witness);
	expect(
		await visibleGraphProjection(renderedFlowGraph(page)),
		`${auditCase.algorithm_id}/${auditCase.label} backward/forward rendering must be deterministic`,
	).toBe(witnessProjection);
}

function titleCaseKebab(value: string): string {
	return value
		.split("-")
		.map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
		.join(" ");
}

function auditedWorkDeltaText(
	auditCase: AuditCase,
	witness: AuditBoundaryWitness,
): string {
	const deltas = witness.work_deltas
		.filter((delta) => delta.unit !== "published-transition")
		.map((delta) => {
			const label =
				delta.unit === "primary-work"
					? auditCase.primary_work_unit
					: delta.unit === "detail-primitive"
						? "Detail primitive"
						: titleCaseKebab(delta.unit);
			return `${label} +${delta.count}`;
		});
	const deltaText = deltas.join(" · ") || "1 published transition";
	return witness.aggregation === "1"
		? deltaText
		: `${deltaText} · aggregates ${witness.aggregation}`;
}

async function assertFeasibilityOverlayTopology(page: Page): Promise<void> {
	const topologyErrors = await renderedFlowGraph(page).evaluate((graph) => {
		const errors: string[] = [];
		const layer = graph.querySelector(".flow-feasibility-layer");
		if (layer === null) return ["missing-feasibility-layer"];
		const domainKind = layer.getAttribute("data-feasibility-domain");
		if (
			domainKind !== "public-input" &&
			domainKind !== "node-aligned-transformation" &&
			domainKind !== "standalone-transformation"
		) {
			errors.push(`invalid-domain:${domainKind ?? "missing"}`);
		}
		const publicDomain = domainKind === "public-input";
		if (
			layer.querySelector(
				'.flow-feasibility-status [data-overlay-contribution="feasibility_overlay"]',
			) !== null
		) {
			errors.push("status-badge-owns-graph-entity");
		}
		const originalNodes = new Set(
			[...graph.querySelectorAll("[data-node-id]")]
				.map((node) => node.getAttribute("data-node-id"))
				.filter((id): id is string => id !== null),
		);
		const originalEdges = new Set(
			[...graph.querySelectorAll("[data-edge-id]")]
				.map((edge) => edge.getAttribute("data-edge-id"))
				.filter((id): id is string => id !== null),
		);
		const endpointKeys = new Set([
			...[...originalNodes].map((id) => `original:${id}`),
			...[...layer.querySelectorAll("[data-feasibility-node]")]
				.map((node) => node.getAttribute("data-feasibility-node"))
				.filter((id): id is string => id?.startsWith("artificial:") === true),
		]);
		for (const node of layer.querySelectorAll(
			'[data-feasibility-node^="artificial:"]',
		)) {
			const id = node.getAttribute("data-feasibility-node");
			const owned = node.querySelector(
				'[data-overlay-contribution="feasibility_overlay"][data-overlay-entity-kind="auxiliary-node"]',
			);
			if (id === null || owned?.getAttribute("data-overlay-entity-id") !== id) {
				errors.push(`auxiliary-node:${id ?? "missing"}`);
			}
		}
		for (const arc of layer.querySelectorAll("[data-feasibility-arc]")) {
			const id = arc.getAttribute("data-feasibility-arc");
			const kind = arc.getAttribute("data-feasibility-arc-kind");
			const from = arc.getAttribute("data-feasibility-from");
			const to = arc.getAttribute("data-feasibility-to");
			const flowCapacity = arc.getAttribute("data-feasibility-flow") ?? "";
			const [flowText, capacityText, ...extra] = flowCapacity.split(":");
			if (
				id === null ||
				kind === null ||
				from === null ||
				to === null ||
				!endpointKeys.has(from) ||
				!endpointKeys.has(to) ||
				extra.length !== 0 ||
				flowText === undefined ||
				capacityText === undefined ||
				!/^(?:0|[1-9][0-9]*)$/u.test(flowText) ||
				!/^(?:0|[1-9][0-9]*)$/u.test(capacityText) ||
				BigInt(flowText) > BigInt(capacityText)
			) {
				errors.push(`arc-shape:${id ?? "missing"}`);
				continue;
			}
			const expectedIdentity = (() => {
				switch (kind) {
					case "original": {
						const edge = id.startsWith("original:")
							? id.slice("original:".length)
							: "";
						return edge.length > 0 && (!publicDomain || originalEdges.has(edge))
							? id
							: undefined;
					}
					case "lower-bound-return":
						return id ===
							`return:${from.slice("original:".length)}:${to.slice("original:".length)}`
							? id
							: undefined;
					case "from-super-source":
						return from === "artificial:super-source" &&
							id === `from-super-source:${to.slice("original:".length)}`
							? id
							: undefined;
					case "to-super-sink":
						return to === "artificial:super-sink" &&
							id === `to-super-sink:${from.slice("original:".length)}`
							? id
							: undefined;
					default:
						return undefined;
				}
			})();
			if (expectedIdentity === undefined) errors.push(`arc-identity:${id}`);
			const owner = arc.querySelector(
				'[data-overlay-contribution="feasibility_overlay"][data-overlay-entity-kind]',
			);
			if (owner === null) {
				errors.push(`arc-without-owned-leaf:${id}`);
				continue;
			}
			const ownerKind = owner.getAttribute("data-overlay-entity-kind");
			const ownerId = owner.getAttribute("data-overlay-entity-id");
			if (kind === "original" && publicDomain) {
				const edgeId = id.slice("original:".length);
				if (
					!(["edge", "residual-arc"] as string[]).includes(ownerKind ?? "") ||
					ownerId !== edgeId
				) {
					errors.push(`original-owner:${id}:${ownerKind}:${ownerId}`);
				}
			} else if (
				!(["auxiliary-edge", "auxiliary-residual-arc"] as string[]).includes(
					ownerKind ?? "",
				) ||
				ownerId !== id
			) {
				errors.push(`auxiliary-owner:${id}:${ownerKind}:${ownerId}`);
			}
		}
		const mutation = layer.cloneNode(true) as Element;
		for (const graphEntity of mutation.querySelectorAll(
			".flow-feasibility-arc, .flow-feasibility-terminal, .flow-feasibility-node-state",
		)) {
			graphEntity.remove();
		}
		if (mutation.querySelector(".flow-feasibility-status") === null) {
			errors.push("mutation-removed-status-badge");
		}
		if (
			mutation.querySelector(
				'[data-overlay-contribution="feasibility_overlay"]',
			) !== null
		) {
			errors.push("status-only-mutation-still-owns-graph-leaf");
		}
		return errors;
	});
	expect(
		topologyErrors,
		"feasibility auxiliary nodes/arcs preserve typed identities and endpoints",
	).toEqual([]);
}

async function assertOverlayWitness(
	page: Page,
	witness: AuditBoundaryWitness,
): Promise<void> {
	const current = workspace(page);
	const inspector = current.locator(".flow-overlay-registry-inspector");
	if (witness.active_overlays.length === 0) {
		await expect(inspector).toHaveCount(0);
		return;
	}
	await expect(inspector).toHaveCount(1);
	const active = await inspector
		.getAttribute("data-active-overlay-fields")
		.then((value) => (value ?? "").split("|").filter(Boolean).sort());
	expect(active).toEqual([...witness.active_overlays].sort());
	if ((await inspector.getAttribute("open")) === null) {
		await inspector.locator("summary").click();
	}
	for (const overlay of witness.active_overlays) {
		if (!(overlay in FLOW_OVERLAY_CONTRIBUTIONS)) {
			throw new Error(`Unknown active overlay ${overlay}`);
		}
		const contribution =
			FLOW_OVERLAY_CONTRIBUTIONS[
				overlay as keyof typeof FLOW_OVERLAY_CONTRIBUTIONS
			];
		const graphCapable = contribution.featureBundles.some(
			(bundle) => bundle !== "rich-status",
		);
		const section = inspector.locator(`[data-overlay-inspector="${overlay}"]`);
		await expect(section).toBeVisible();
		await expect(section.locator("h3")).not.toHaveText("");
		await expect(section.locator("p").first()).not.toHaveText("");
		const scalarValues = Object.entries(
			witness.overlay_scalar_values[overlay] ?? {},
		);
		expect(
			scalarValues.length,
			`${overlay} must expose at least one source-owned scalar stage/value`,
		).toBeGreaterThan(0);
		for (const [field, value] of scalarValues) {
			await expect(
				section.locator(`[data-overlay-field="${field}"]`),
				`${overlay}.${field} exact visible value`,
			).toHaveAttribute("data-overlay-value", value);
		}
		const statusContribution = current.locator(
			`[data-overlay-contribution-status="${overlay}"]`,
		);
		if (graphCapable) {
			const ownedLeaves = renderedFlowGraph(page).locator(
				`[data-overlay-contribution="${overlay}"][data-overlay-feature-bundle][data-overlay-entity-kind][data-overlay-entity-id][data-overlay-role]:not(.flow-overlay-stage-badge), [data-overlay-contributions~="${overlay}"][data-overlay-feature-bundle][data-overlay-entity-kind][data-overlay-entity-id][data-overlay-role]:not(.flow-overlay-stage-badge)`,
			);
			const declaredGraphBundles = contribution.featureBundles.filter(
				(bundle) => bundle !== "rich-status",
			);
			const leafAudit = await ownedLeaves.evaluateAll(
				(leaves, declaredBundles) => {
					const painted = (leaf: Element) => {
						if (!(leaf instanceof SVGGraphicsElement)) return false;
						const rect = leaf.getBoundingClientRect();
						const style = getComputedStyle(leaf);
						if (
							Math.max(rect.width, rect.height) <= 0 ||
							style.display === "none" ||
							style.visibility === "hidden" ||
							Number(style.opacity) === 0
						)
							return false;
						if (leaf instanceof SVGTextElement) {
							return (leaf.textContent?.trim().length ?? 0) > 0;
						}
						const openLine =
							leaf instanceof SVGLineElement ||
							leaf instanceof SVGPolylineElement ||
							(leaf instanceof SVGPathElement &&
								!/[zZ]/u.test(leaf.getAttribute("d") ?? ""));
						const paintedStroke =
							style.stroke !== "none" && Number(style.strokeOpacity) > 0;
						if (openLine) return paintedStroke;
						return (
							paintedStroke ||
							(style.fill !== "none" && Number(style.fillOpacity) > 0)
						);
					};
					return {
						paintedCount: leaves.filter(painted).length,
						bundleErrors: leaves.flatMap((leaf) => {
							const bundle = leaf.getAttribute("data-overlay-feature-bundle");
							return bundle !== null &&
								(declaredBundles as readonly string[]).includes(bundle)
								? []
								: [bundle ?? "missing"];
						}),
					};
				},
				declaredGraphBundles,
			);
			expect(
				leafAudit.paintedCount,
				`${overlay} must paint an exact source-owned entity leaf`,
			).toBeGreaterThan(0);
			expect(
				leafAudit.bundleErrors,
				`${overlay} owned leaves name a declared graph feature bundle`,
			).toEqual([]);
			if (overlay === "feasibility_overlay") {
				await assertFeasibilityOverlayTopology(page);
			}
			if (advancedGraphOverlayFields.has(overlay)) {
				const nativeLeaves = renderedFlowGraph(page).locator(
					`[data-overlay-contribution="${overlay}"][data-overlay-feature-bundle="advanced-algorithm"][data-overlay-entity-kind][data-overlay-entity-id][data-overlay-role]`,
				);
				const paintedNativeLeaves = await nativeLeaves.evaluateAll(
					(leaves) =>
						leaves.filter((leaf) => {
							if (!(leaf instanceof SVGGraphicsElement)) return false;
							const rect = leaf.getBoundingClientRect();
							const style = getComputedStyle(leaf);
							return (
								Math.max(rect.width, rect.height) > 0 &&
								style.display !== "none" &&
								style.visibility !== "hidden" &&
								Number(style.opacity) > 0 &&
								((style.stroke !== "none" && Number(style.strokeOpacity) > 0) ||
									(style.fill !== "none" && Number(style.fillOpacity) > 0))
							);
						}).length,
				);
				expect(
					paintedNativeLeaves,
					`${overlay} must paint its native working graph on the main canvas`,
				).toBeGreaterThan(0);
				const identityErrors = await nativeLeaves.evaluateAll((leaves) => {
					const graph = leaves[0]?.closest("svg");
					if (graph === null || graph === undefined) return ["missing-graph"];
					const nodeIds = new Set(
						[...graph.querySelectorAll("[data-node-id]")]
							.map((element) => element.getAttribute("data-node-id"))
							.filter((id): id is string => id !== null),
					);
					const edgeIds = new Set(
						[...graph.querySelectorAll("[data-edge-id]")]
							.map((element) => element.getAttribute("data-edge-id"))
							.filter((id): id is string => id !== null),
					);
					const sourceIdentitySet = (selector: string, attribute: string) =>
						new Set(
							(graph.querySelector(selector)?.getAttribute(attribute) ?? "")
								.split("|")
								.filter(Boolean),
						);
					const weightedNodes = sourceIdentitySet(
						'[data-advanced-overlay="weighted-push-relabel"]',
						"data-weighted-native-node-ids",
					);
					const weightedEdges = sourceIdentitySet(
						'[data-advanced-overlay="weighted-push-relabel"]',
						"data-weighted-native-edge-ids",
					);
					const primalDualNodes = sourceIdentitySet(
						'[data-advanced-overlay="primal-dual-ipm-mcf"]',
						"data-primal-dual-native-node-ids",
					);
					const primalDualEdges = sourceIdentitySet(
						'[data-advanced-overlay="primal-dual-ipm-mcf"]',
						"data-primal-dual-native-edge-ids",
					);
					const augmentingWorkingNodes = sourceIdentitySet(
						'[data-advanced-overlay="augmenting-electrical-elimination"]',
						"data-augmenting-working-node-ids",
					);
					const augmentingWorkingEdges = sourceIdentitySet(
						'[data-advanced-overlay="augmenting-electrical-cleanup"]',
						"data-augmenting-working-edge-ids",
					);
					const augmentingExtractionEdges = sourceIdentitySet(
						'[data-advanced-overlay="augmenting-electrical-extraction"]',
						"data-augmenting-extraction-edge-ids",
					);
					return leaves.flatMap((leaf) => {
						const kind = leaf.getAttribute("data-overlay-entity-kind");
						const id = leaf.getAttribute("data-overlay-entity-id");
						const contribution = leaf.getAttribute("data-overlay-contribution");
						if (id === null || kind === null) return ["missing-identity"];
						if (kind === "node" && !nodeIds.has(id)) return [`node:${id}`];
						if (kind === "edge" && !edgeIds.has(id)) return [`edge:${id}`];
						if (kind === "residual-arc") {
							const direction = leaf.getAttribute(
								"data-overlay-residual-direction",
							);
							if (!edgeIds.has(id)) return [`residual-edge:${id}`];
							if (direction !== "forward" && direction !== "reverse") {
								return [`residual-direction:${id}:${direction ?? "missing"}`];
							}
						}
						if (
							contribution === "augmenting_electrical_overlay" &&
							kind === "auxiliary-node" &&
							!augmentingWorkingNodes.has(id)
						) {
							return [`augmenting-working-node:${id}`];
						}
						if (
							contribution === "augmenting_electrical_overlay" &&
							kind === "auxiliary-residual-arc" &&
							!augmentingWorkingEdges.has(id)
						) {
							return [`augmenting-working-edge:${id}`];
						}
						if (
							contribution === "augmenting_electrical_overlay" &&
							kind === "auxiliary-edge" &&
							id !== "preconditioner-bank" &&
							!augmentingExtractionEdges.has(id)
						) {
							return [`augmenting-extraction-edge:${id}`];
						}
						if (
							contribution === "primal_dual_ipm_mcf_overlay" &&
							kind === "auxiliary-node"
						) {
							if (!primalDualNodes.has(id)) return [`aux-node:${id}`];
							const originalNode = leaf.getAttribute(
								"data-overlay-original-node-id",
							);
							const originalEdge = leaf.getAttribute(
								"data-overlay-original-edge-id",
							);
							if (id.startsWith("node:")) {
								return originalNode !== null && nodeIds.has(originalNode)
									? []
									: [`aux-node:${id}`];
							}
							if (id.startsWith("capacity:")) {
								return originalEdge !== null && edgeIds.has(originalEdge)
									? []
									: [`capacity-node:${id}`];
							}
							return [`auxiliary-node:${id}`];
						}
						if (
							contribution === "primal_dual_ipm_mcf_overlay" &&
							kind === "auxiliary-edge" &&
							!primalDualEdges.has(id)
						) {
							return [`auxiliary-edge:${id}`];
						}
						if (contribution === "weighted_push_relabel_shortcut_overlay") {
							if (kind === "auxiliary-node" && !weightedNodes.has(id)) {
								return [`weighted-node:${id}`];
							}
							if (
								(kind === "auxiliary-edge" ||
									kind === "auxiliary-residual-arc") &&
								!weightedEdges.has(id)
							) {
								return [`weighted-edge:${id}`];
							}
							if (kind === "auxiliary-residual-arc") {
								const direction = leaf.getAttribute(
									"data-overlay-residual-direction",
								);
								if (direction !== "forward" && direction !== "reverse") {
									return [`weighted-direction:${id}:${direction ?? "missing"}`];
								}
							}
						}
						return [];
					});
				});
				expect(
					identityErrors,
					`${overlay} leaf identities resolve to their native or canonical graphs`,
				).toEqual([]);
			}
		} else {
			expect(
				await statusContribution.count(),
				`${overlay} is status-only and must publish source-owned status`,
			).toBeGreaterThan(0);
		}
		const statusDisclosure = current.locator(".flow-rich-status-disclosure");
		const statusWasOpen =
			(await statusDisclosure.getAttribute("open")) !== null;
		if (!statusWasOpen) {
			await statusDisclosure.getByLabel("Show algorithm state details").click();
		}
		await expect(statusContribution).toBeVisible();
		const statusText = (await statusContribution.textContent()) ?? "";
		expect(
			Object.values(witness.overlay_scalar_values[overlay] ?? {}).some(
				(value) => statusText.includes(value),
			),
			`${overlay} status must render an exact source-owned scalar value`,
		).toBe(true);
		if (!statusWasOpen) {
			await statusDisclosure.getByLabel("Show algorithm state details").click();
		}
	}
	await inspector.locator("summary").click();
}

async function assertAuditedWorkBoundary(
	page: Page,
	auditCase: AuditCase,
	witness: AuditBoundaryWitness,
): Promise<void> {
	const current = workspace(page);
	await expect(current.getByTestId("flow-timeline-readout")).toHaveText(
		`Raw ${witness.event} / ${auditCase.event_count}`,
	);
	const inspector = current.getByLabel("Flow scene inspector");
	await expect(inspector.locator("[data-trace-catalog-id]")).toHaveAttribute(
		"data-trace-catalog-id",
		witness.catalog_id,
	);
	await expect(
		inspector
			.locator("dt", { hasText: /^Work delta$/u })
			.locator("..")
			.locator("dd"),
	).toHaveText(auditedWorkDeltaText(auditCase, witness));
	await expect(
		inspector
			.locator("dt", { hasText: /^Primary work$/u })
			.locator("..")
			.locator("dd"),
	).toHaveText(
		`${auditCase.primary_work_unit}: ${witness.primary_completed} / ${auditCase.primary_work} · ${titleCaseKebab(auditCase.primary_work_abstraction)}`,
	);
	await expect(
		inspector
			.locator("dt", { hasText: /^Detail progress$/u })
			.locator("..")
			.locator("dd"),
	).toHaveText(
		`${witness.detail_completed} / ${auditCase.detail_count} meaningful boundaries`,
	);
	const coverage = inspector
		.locator("dt", { hasText: /^Trace coverage$/u })
		.locator("..")
		.locator("dd");
	await expect(coverage).toHaveText(
		`${auditCase.primary_work} exact ${auditCase.primary_work_unit} counted; the canvas shows only solver-published source boundaries`,
	);
	await expect(
		current.getByRole("progressbar", {
			name: "Measured primary work progress",
		}),
	).toHaveAttribute(
		"aria-valuetext",
		`${auditCase.primary_work_unit} ${witness.primary_completed} of ${auditCase.primary_work}`,
	);
	if (BigInt(witness.primary_delta) > 0n) {
		const boundary = inspector
			.locator("dt", { hasText: /^Boundary$/u })
			.locator("..")
			.locator("dd");
		await expect(boundary).toContainText(
			`${witness.primary_delta} measured ${auditCase.primary_work_unit} performed by this source event`,
		);
		expect(witness.work_first).not.toBeNull();
		expect(witness.work_last).not.toBeNull();
		expect(witness.work_total).not.toBeNull();
	}
	await expect(current.locator(".flow-work-observation")).toHaveCount(0);
	const semanticPaintErrors = await renderedFlowGraph(page)
		.locator(
			".flow-transportation-optimality-mark, .flow-tardos-potential-anchor-complete circle, .flow-tardos-potential-anchor-complete text",
		)
		.evaluateAll((marks) => {
			if (marks.length === 0) return [];
			const graph = marks[0]?.closest("svg");
			if (!(graph instanceof SVGSVGElement)) return ["missing graph"];
			const namespace = "http://www.w3.org/2000/svg";
			const probe = document.createElementNS(namespace, "circle");
			probe.style.fill = "var(--success)";
			probe.style.stroke = "var(--success)";
			graph.append(probe);
			const expectedStyle = getComputedStyle(probe);
			const expected = {
				fill: expectedStyle.fill,
				stroke: expectedStyle.stroke,
			};
			probe.remove();
			const rootToken = getComputedStyle(document.documentElement)
				.getPropertyValue("--success")
				.trim();
			const defaultPaints = new Set([
				"",
				"none",
				"rgb(0, 0, 0)",
				"rgba(0, 0, 0, 0)",
			]);
			const errors: string[] = [];
			if (rootToken.length === 0) errors.push("undefined --success");
			if (
				defaultPaints.has(expected.fill) ||
				defaultPaints.has(expected.stroke)
			) {
				errors.push(
					`unresolved success paint ${expected.fill}/${expected.stroke}`,
				);
			}
			for (const [index, mark] of marks.entries()) {
				const property = mark instanceof SVGTextElement ? "fill" : "stroke";
				const actual = getComputedStyle(mark)[property];
				if (actual !== expected[property]) {
					errors.push(
						`${index}:${mark.tagName.toLowerCase()}:${property}:${actual} != ${expected[property]}`,
					);
				}
			}
			return errors;
		});
	expect(
		semanticPaintErrors,
		`${auditCase.algorithm_id}/${auditCase.label} semantic completion marks resolve the declared success color`,
	).toEqual([]);
	const transportationRouteOwnershipErrors = await renderedFlowGraph(page)
		.locator(".flow-transportation-optimality-mark")
		.evaluateAll((marks) =>
			marks.flatMap((mark, index) => {
				if (!(mark instanceof SVGPathElement)) {
					return [`${index}: completion certificate is not a route path`];
				}
				const edge = mark.closest(".flow-original-edge");
				const capacity = edge?.querySelector<SVGPathElement>(
					".flow-capacity-rail",
				);
				if (capacity === null || capacity === undefined) {
					return [
						`${index}: completion certificate has no owned capacity route`,
					];
				}
				return mark.getAttribute("d") === capacity.getAttribute("d")
					? []
					: [`${index}: completion certificate is detached from its route`];
			}),
		);
	expect(
		transportationRouteOwnershipErrors,
		`${auditCase.algorithm_id}/${auditCase.label} transportation completion marks stay on their owning routes`,
	).toEqual([]);
	for (const [selector, attribute, expected, kind] of [
		[
			'[data-event-touch="true"]',
			"data-event-identities",
			witness.touched_identities,
			"touched",
		],
		[
			'[data-event-change="true"]',
			"data-changed-identities",
			witness.changed_identities,
			"changed",
		],
	] as const) {
		const rendered = await current
			.locator(selector)
			.evaluateAll(
				(items, identityAttribute) =>
					[
						...new Set(
							items.flatMap((item) =>
								(item.getAttribute(identityAttribute) ?? "")
									.split("|")
									.filter(Boolean),
							),
						),
					].sort(),
				attribute,
			);
		expect(
			rendered,
			`${auditCase.algorithm_id}/${auditCase.label} ${kind} graph identities must match the source-produced manifest`,
		).toEqual([...expected].sort());
	}
	if (
		witness.catalog_id === "orlin-max-flow.inspect-subproblem-arc" ||
		witness.catalog_id === "orlin-max-flow.inspect-decomposition-arc" ||
		witness.catalog_id === "orlin-max-flow.inspect-lift-residual-arc"
	) {
		const scanTargets = renderedFlowGraph(page).locator(
			`[data-orlin-max-scan="${witness.primary_completed}"]:visible`,
		);
		await expect(
			scanTargets,
			"Orlin source scan ordinal must identify exactly one visible inspected arc",
		).toHaveCount(1);
		expect(
			await scanTargets.evaluate((target, serial) => {
				const visiblePaths = [...target.querySelectorAll("path")].filter(
					(path) => {
						const box = path.getBoundingClientRect();
						const style = getComputedStyle(path);
						return (
							Math.max(box.width, box.height) > 0 &&
							style.display !== "none" &&
							style.visibility !== "hidden" &&
							style.stroke !== "none" &&
							Number(style.opacity) > 0 &&
							Number(style.strokeOpacity) > 0
						);
					},
				);
				const markerResolved = visiblePaths.some((path) => {
					const marker = path.getAttribute("marker-end") ?? "";
					const match = marker.match(/^url\(["']?#([^"')]+)["']?\)$/u);
					return (
						match?.[1] !== undefined &&
						document.getElementById(match[1]) !== null
					);
				});
				const visibleText = [...target.querySelectorAll("text")].some(
					(label) =>
						label.getBoundingClientRect().width > 0 &&
						(label.textContent ?? "").includes(`#${serial}`),
				);
				return visiblePaths.length > 0 && markerResolved && visibleText;
			}, witness.primary_completed),
			"Orlin source scan must paint its exact arc, resolved arrow marker, and source serial",
		).toBe(true);
	}
	if (
		[
			"orlin-mcf.inspect-contractible-arc",
			"orlin-mcf.inspect-reachability-arc",
			"orlin-mcf.inspect-compressed-residual-arc",
			"orlin-mcf.inspect-compressed-arc",
		].includes(witness.catalog_id)
	) {
		const scanTargets = renderedFlowGraph(page).locator(
			`[data-orlin-scan="${witness.primary_completed}"]`,
		);
		expect(
			await scanTargets.count(),
			"Orlin MCF scan ordinal must be anchored to the exact transformed F/S branch",
		).toBeGreaterThan(0);
		expect(
			await scanTargets.evaluateAll(
				(targets, serial) =>
					targets.some((target) =>
						(target.textContent ?? "").includes(`#${serial}`),
					),
				witness.primary_completed,
			),
			"revisiting an Orlin MCF branch must advance visible graph text",
		).toBe(true);
	}
	if (
		witness.catalog_id === "minimum-mean-cycle-canceling.inspect-residual-arc"
	) {
		const scanTargets = renderedFlowGraph(page).locator(
			`[data-minimum-mean-scan="${witness.primary_completed}"]`,
		);
		expect(
			await scanTargets.count(),
			"minimum-mean source scan ordinal must be anchored to its residual arc",
		).toBeGreaterThan(0);
		expect(
			await scanTargets.evaluateAll(
				(targets, serial) =>
					targets.some((target) =>
						(target.textContent ?? "").includes(`#${serial}`),
					),
				witness.primary_completed,
			),
			"a repeated Karp scan must advance visible graph text",
		).toBe(true);
	}
	if (
		witness.catalog_id ===
		"polynomial-primal-network-simplex.inspect-extended-arc"
	) {
		const scanTargets = renderedFlowGraph(page).locator(
			`[data-polynomial-primal-scan="${witness.primary_completed}"]`,
		);
		expect(
			await scanTargets.count(),
			"polynomial-primal source scan ordinal must be anchored to its extended arc",
		).toBeGreaterThan(0);
		expect(
			await scanTargets.evaluateAll(
				(targets, serial) =>
					targets.some((target) =>
						(target.textContent ?? "").includes(`#${serial}`),
					),
				witness.primary_completed,
			),
			"a repeated extended-arc scan must advance visible graph text",
		).toBe(true);
	}
	if (
		[
			"relaxation.scan-balanced-arcs",
			"relaxation.scan-boundary-flow-arc",
			"relaxation.scan-price-cut-arc",
		].includes(witness.catalog_id)
	) {
		const scanTargets = renderedFlowGraph(page).locator(
			`[data-relaxation-scan="${witness.primary_completed}"]`,
		);
		expect(
			await scanTargets.count(),
			"relaxation source scan ordinal must be anchored to its inspected graph arc",
		).toBeGreaterThan(0);
		expect(
			await scanTargets.evaluateAll(
				(targets, serial) =>
					targets.some((target) =>
						(target.textContent ?? "").includes(`#${serial}`),
					),
				witness.primary_completed,
			),
			"a repeated relaxation scan must advance visible graph text",
		).toBe(true);
	}
	if (
		witness.catalog_id ===
		"primal-dual-interior-point-mcf.inspect-forest-subset"
	) {
		const subsetTargets = current
			.getByTestId("flow-ipm-mcf-panel")
			.locator(`[data-ipm-forest-subset="${witness.primary_completed}"]`);
		expect(
			await subsetTargets.count(),
			"the exact forest subset ordinal must be visible in the auxiliary graph",
		).toBeGreaterThan(0);
		expect(
			await subsetTargets.evaluateAll(
				(targets, serial) =>
					targets.some((target) =>
						(target.textContent ?? "").includes(`#${serial}`),
					),
				witness.primary_completed,
			),
			"even the empty candidate subset must advance visible graph text",
		).toBe(true);
	}
	await assertStepEvidence(page, auditCase, witness);
	await assertOverlayWitness(page, witness);
}

async function assertAdjacentSourceBoundaryMoves(
	page: Page,
	auditCase: AuditCase,
): Promise<void> {
	const witness = auditCase.first_primary_work;
	if (witness.event >= auditCase.event_count) return;
	const current = workspace(page);
	await seekRawEvent(page, witness.event, auditCase.event_count);
	await assertAuditedWorkBoundary(page, auditCase, witness);
	const graph = renderedFlowGraph(page);
	const beforeGraph = await visibleGraphProjection(graph);
	const beforeEvidence = await current
		.locator("[data-testid^='flow-step-']")
		.allTextContents();
	await current.getByRole("button", { name: "Next step" }).click();
	await expect(current.getByTestId("flow-timeline-readout")).toHaveText(
		`Raw ${witness.event + 1} / ${auditCase.event_count}`,
	);
	await expect(current.getByTestId("flow-step-evidence")).toHaveAttribute(
		"data-evidence-kind",
		"source-event",
	);
	const nextCatalog = await current
		.getByLabel("Flow scene inspector")
		.locator("[data-trace-catalog-id]")
		.getAttribute("data-trace-catalog-id");
	expect(nextCatalog).not.toMatch(/\.(?:primary-work-unit|work-observation)$/u);
	expect(
		await current.locator("[data-testid^='flow-step-']").allTextContents(),
		`${auditCase.algorithm_id}/${auditCase.label} adjacent source evidence`,
	).not.toEqual(beforeEvidence);
	expect(
		await visibleGraphProjection(graph),
		`${auditCase.algorithm_id}/${auditCase.label} adjacent source graph rendering`,
	).not.toBe(beforeGraph);
	await expect(current.locator(".flow-work-observation")).toHaveCount(0);
}

/**
 * Release-only exhaustive paint audit. It runs inside the browser so tens of
 * thousands of Worker/ACK transitions do not pay one Playwright round trip
 * apiece, while still comparing the computed production SVG after every raw
 * source boundary.
 */
async function assertEverySourceBoundaryMoves(
	page: Page,
	auditCase: AuditCase,
	startEvent = 0,
): Promise<void> {
	await page.evaluate(
		async ({ algorithmId, caseLabel, eventCount, debug, startEvent }) => {
			const activeWorkspace = () =>
				document.querySelector<HTMLElement>(
					"[data-workspace-id]:not([hidden])",
				);
			const current = activeWorkspace();
			if (current === null) throw new Error("Active flow workspace is missing");
			if (
				current.querySelector('[data-testid="flow-timeline-readout"]') ===
					null ||
				current.querySelector('svg.flow-graph[role="img"]') === null
			) {
				throw new Error("Flow timeline or graph is missing");
			}
			const readoutText = () =>
				(
					activeWorkspace()?.querySelector<HTMLElement>(
						'[data-testid="flow-timeline-readout"]',
					)?.textContent ?? ""
				).trim();
			const rounded = (value: number) => Math.round(value * 100) / 100;
			const geometryAttributes = [
				"d",
				"x",
				"y",
				"x1",
				"y1",
				"x2",
				"y2",
				"cx",
				"cy",
				"r",
				"rx",
				"ry",
				"width",
				"height",
				"points",
				"transform",
			] as const;
			const projection = () => {
				const graph = activeWorkspace()?.querySelector<SVGSVGElement>(
					'svg.flow-graph[role="img"]',
				);
				if (graph === null || graph === undefined) {
					throw new Error("Active flow graph disappeared during publication");
				}
				return JSON.stringify(
					[...graph.querySelectorAll("*")].flatMap((element) => {
						if (!(element instanceof SVGGraphicsElement)) return [];
						if (element.closest(".flow-overlay-stage-badge") !== null) {
							return [];
						}
						const style = getComputedStyle(element);
						const box = element.getBoundingClientRect();
						if (
							style.display === "none" ||
							style.visibility === "hidden" ||
							Number(style.opacity) === 0 ||
							(box.width <= 0 && box.height <= 0)
						)
							return [];
						const tag = element.tagName.toLowerCase();
						return [
							{
								tag,
								geometry: geometryAttributes.flatMap((attribute) => {
									const value = element.getAttribute(attribute);
									return value === null ? [] : [[attribute, value] as const];
								}),
								box: [
									rounded(box.x),
									rounded(box.y),
									rounded(box.width),
									rounded(box.height),
								],
								paint: [
									style.fill,
									style.fillOpacity,
									style.stroke,
									style.strokeOpacity,
									style.strokeWidth,
									style.strokeDasharray,
									style.opacity,
								],
								text:
									tag === "text" || tag === "tspan"
										? (element.textContent ?? "").trim()
										: "",
							},
						];
					}),
				);
			};
			const waitForEvent = (target: number) =>
				new Promise<void>((resolve, reject) => {
					const expected = `Raw ${target} / ${eventCount}`;
					if (readoutText() === expected) {
						resolve();
						return;
					}
					const timeout = window.setTimeout(() => {
						observer.disconnect();
						reject(new Error(`Timed out waiting for ${expected}`));
					}, 60_000);
					const observer = new MutationObserver(() => {
						if (readoutText() !== expected) return;
						window.clearTimeout(timeout);
						observer.disconnect();
						resolve();
					});
					observer.observe(document.body, {
						childList: true,
						characterData: true,
						subtree: true,
					});
				});
			const waitForButtonEnabled = (label: string) =>
				new Promise<HTMLButtonElement>((resolve, reject) => {
					const findEnabled = () => {
						const next = activeWorkspace()?.querySelector<HTMLButtonElement>(
							`button[aria-label="${label}"]`,
						);
						return next !== undefined && next !== null && !next.disabled
							? next
							: undefined;
					};
					const ready = findEnabled();
					if (ready !== undefined) {
						resolve(ready);
						return;
					}
					const timeout = window.setTimeout(() => {
						observer.disconnect();
						reject(
							new Error(
								`${algorithmId}/${caseLabel} timed out waiting for ${label}`,
							),
						);
					}, 60_000);
					const observer = new MutationObserver(() => {
						const next = findEnabled();
						if (next === undefined) return;
						window.clearTimeout(timeout);
						observer.disconnect();
						resolve(next);
					});
					observer.observe(document.body, {
						attributes: true,
						attributeFilter: ["disabled"],
						childList: true,
						subtree: true,
					});
				});
			let before = projection();
			for (let event = startEvent + 1; event <= eventCount; event += 1) {
				if (debug) console.info(`FLOW_AUDIT before ${event}/${eventCount}`);
				const next = await waitForButtonEnabled("Next step");
				const published = waitForEvent(event);
				next.click();
				await published;
				if (debug) console.info(`FLOW_AUDIT published ${event}/${eventCount}`);
				await new Promise<void>((resolve) =>
					requestAnimationFrame(() => resolve()),
				);
				const after = projection();
				if (debug) console.info(`FLOW_AUDIT painted ${event}/${eventCount}`);
				if (after === before) {
					const catalog = activeWorkspace()
						?.querySelector("[data-trace-catalog-id]")
						?.getAttribute("data-trace-catalog-id");
					throw new Error(
						`${algorithmId}/${caseLabel} raw ${event} (${catalog ?? "unknown"}) has no production SVG paint change`,
					);
				}
				before = after;
			}
			await waitForButtonEnabled("First event");
			if (debug) console.info("FLOW_AUDIT final ACK ready");
		},
		{
			algorithmId: auditCase.algorithm_id,
			caseLabel: auditCase.label,
			eventCount: auditCase.event_count,
			debug: representativeDebug,
			startEvent,
		},
	);
}

async function assertStepEvidence(
	page: Page,
	auditCase: AuditCase,
	witness: AuditBoundaryWitness,
): Promise<void> {
	const current = workspace(page);
	const evidence = current.getByTestId("flow-step-evidence");
	await expect(evidence).toBeVisible();
	await expect(evidence).toHaveAttribute("data-evidence-kind", "source-event");
	expect(witness.catalog_id).not.toMatch(
		/\.(?:primary-work-unit|work-observation)$/u,
	);
	const action = (await evidence.locator("h3").textContent())?.trim() ?? "";
	expect(
		action,
		`${auditCase.algorithm_id}/${auditCase.label} step action`,
	).not.toBe("");
	expect(
		action,
		`${auditCase.algorithm_id}/${auditCase.label} source-action caption`,
	).not.toMatch(/^Count\b/u);
	for (const field of [
		"flow-step-work",
		"flow-step-focus",
		"flow-step-observation",
		"flow-step-effect",
	] as const) {
		const text = (await current.getByTestId(field).textContent())?.trim() ?? "";
		expect(
			text,
			`${auditCase.algorithm_id}/${auditCase.label} ${field}`,
		).not.toBe("");
	}
	await expect(evidence.locator(".flow-step-pseudocode code")).not.toHaveText(
		"",
	);
	if (BigInt(witness.primary_delta) === 0n) {
		const exactDelta = witness.work_deltas.find(
			(delta) =>
				delta.unit !== "published-transition" &&
				delta.unit !== "detail-primitive" &&
				delta.unit !== "primary-work",
		);
		if (exactDelta !== undefined) {
			await expect(current.getByTestId("flow-step-work")).toHaveText(
				`${exactDelta.count} ${titleCaseKebab(exactDelta.unit)} · step ${witness.detail_completed}/${auditCase.detail_count}`,
			);
		}
		return;
	}
	await expect(current.getByTestId("flow-step-work")).toContainText(
		`${witness.primary_delta} ${auditCase.primary_work_unit}`,
	);
	await expect(current.getByTestId("flow-step-work")).toContainText(
		`units ${witness.work_first}–${witness.work_last} of ${witness.work_total}`,
	);
	await expect(current.getByTestId("flow-step-effect")).toContainText(
		`${witness.primary_delta} measured work`,
	);
}

async function assertEventPresentation(
	page: Page,
	algorithmId: string,
	caseLabel: string,
): Promise<void> {
	const current = workspace(page);
	const actionLocator = current.locator(".flow-event-action");
	await expect(
		actionLocator,
		`${algorithmId}/${caseLabel} event caption`,
	).toBeVisible({ timeout: 5_000 });
	const action = (await actionLocator.textContent())?.trim();
	expect(action, `${algorithmId}/${caseLabel} event caption`).toBeTruthy();
	expect(action?.length ?? 0).toBeLessThanOrEqual(180);
	expect(action, `${algorithmId}/${caseLabel} raw catalog caption`).not.toMatch(
		/^[a-z0-9-]+\.[a-z0-9-]+$/u,
	);

	const inspector = current.getByLabel("Flow scene inspector");
	const overview = inspector
		.locator("[data-trace-catalog-id]")
		.locator("..")
		.locator("..");
	const traceIdentity = inspector.locator("[data-trace-catalog-id]");
	for (const term of ["Boundary", "Effect", "Touched", "Changed"]) {
		await expect(
			overview
				.locator("dt", { hasText: new RegExp(`^${term}$`) })
				.locator(".."),
		).toBeVisible();
	}
	const stepEvidence = current.getByTestId("flow-step-evidence");
	await expect(stepEvidence).toBeVisible();
	for (const field of [
		"flow-step-work",
		"flow-step-focus",
		"flow-step-observation",
		"flow-step-effect",
	] as const) {
		await expect(current.getByTestId(field)).not.toHaveText("");
	}
	await expect(
		stepEvidence.locator(".flow-step-pseudocode code"),
	).not.toHaveText("");
	for (const term of ["Work delta", "Primary work", "Detail progress"]) {
		await expect(
			inspector
				.locator("dt", { hasText: new RegExp(`^${term}$`) })
				.locator(".."),
		).toBeVisible();
	}
	const touched = current.locator('[data-event-touch="true"]');
	const changed = current.locator('[data-event-change="true"]');
	const catalogId = await traceIdentity.getAttribute("data-trace-catalog-id");
	if ((await traceIdentity.getAttribute("data-trace-boundary")) === "micro") {
		const graph = renderedFlowGraph(page);
		const focusedNodes = graph.locator(
			'.flow-node-frame[data-node-id][data-event-touch="true"], .flow-overview-cluster[data-cluster-id][data-event-touch="true"]',
		);
		const focusedEdges = graph.locator(
			'.flow-original-edge[data-edge-id][data-event-touch="true"], .flow-overview-edge[data-aggregate-kind="original-edge"][data-event-touch="true"]',
		);
		const focusedNodeCount = await focusedNodes.count();
		const focusedEdgeCount = await focusedEdges.count();
		expect(
			focusedEdgeCount,
			`${algorithmId}/${caseLabel}/${catalogId} one Micro boundary focuses at most one ordinary edge`,
		).toBeLessThanOrEqual(1);
		const auxiliaryCellKind =
			catalogId === "electrical-flow.matrix-scalar-product"
				? "laplacian"
				: catalogId === "relaxed-most-negative-cycle.inspect-assignment-cell" ||
						(catalogId === "hungarian.inspect-cell" &&
							focusedEdgeCount === 0 &&
							focusedNodeCount === 2)
					? "assignment"
					: undefined;
		expect(
			focusedNodeCount,
			`${algorithmId}/${caseLabel}/${catalogId} one Micro boundary focuses one node, one edge's endpoints, or one declared auxiliary cell`,
		).toBeLessThanOrEqual(
			focusedEdgeCount === 0 && auxiliaryCellKind === undefined ? 1 : 2,
		);
		if (auxiliaryCellKind !== undefined) {
			const cell = graph.locator(
				`[data-auxiliary-cell="${auxiliaryCellKind}"]`,
			);
			await expect(
				cell,
				`${algorithmId}/${caseLabel}/${catalogId} exposes one row/column annotation`,
			).toHaveCount(1);
			const annotatedNodes = [
				await cell.getAttribute("data-matrix-row-node"),
				await cell.getAttribute("data-matrix-column-node"),
			]
				.filter((node): node is string => node !== null)
				.sort();
			const focusedNodeIds = await focusedNodes.evaluateAll((nodes) =>
				nodes
					.map((node) => node.getAttribute("data-node-id"))
					.filter((node): node is string => node !== null)
					.sort(),
			);
			expect(
				[...new Set(annotatedNodes)],
				`${algorithmId}/${caseLabel}/${catalogId} annotation endpoints match the focused matrix coordinates`,
			).toEqual([...new Set(focusedNodeIds)]);
			await expect(cell.locator(".flow-auxiliary-cell-label")).toContainText(
				/ROW|[LA]\[/u,
			);
		}
	}
	const touchedSummary =
		(await inspector
			.locator("dt", { hasText: /^Touched$/u })
			.locator("..")
			.locator("dd")
			.textContent()) ?? "";
	const changedSummary =
		(await inspector
			.locator("dt", { hasText: /^Changed$/u })
			.locator("..")
			.locator("dd")
			.textContent()) ?? "";
	const touchedSummaryRow = inspector
		.locator("dt", { hasText: /^Touched$/u })
		.locator("..");
	const expectedTouchedIdentities = (
		(await touchedSummaryRow
			.locator("dd")
			.getAttribute("data-event-identities")) ?? ""
	)
		.split("|")
		.filter(Boolean)
		.sort();
	expect(
		touchedSummary.includes("No graph entity"),
		`${algorithmId}/${caseLabel} Inspector touched summary`,
	).toBe(expectedTouchedIdentities.length === 0);
	expect(
		changedSummary.includes("No graph entity"),
		`${algorithmId}/${caseLabel} Inspector changed summary`,
	).toBe((await changed.count()) === 0);
	const renderedTouchedIdentities = await touched.evaluateAll((items) =>
		[
			...new Set(
				items.flatMap((item) =>
					(item.getAttribute("data-event-identities") ?? "")
						.split("|")
						.filter(Boolean),
				),
			),
		].sort(),
	);
	expect(
		renderedTouchedIdentities,
		`${algorithmId}/${caseLabel} DOM targets must exactly match source-event identities`,
	).toEqual(expectedTouchedIdentities);
	await expect(current.locator(".flow-work-observation")).toHaveCount(0);

	for (const [summaryRow, entities, attribute, kind] of [
		[
			inspector.locator("dt", { hasText: /^Changed$/u }).locator(".."),
			changed,
			"data-changed-identities",
			"changed",
		],
	] as const) {
		const expectedIdentities = (
			(await summaryRow.locator("dd").getAttribute("data-event-identities")) ??
			""
		)
			.split("|")
			.filter(Boolean)
			.sort();
		const renderedIdentities = await entities.evaluateAll(
			(items, identityAttribute) =>
				[
					...new Set(
						items.flatMap((item) =>
							(item.getAttribute(identityAttribute) ?? "")
								.split("|")
								.filter(Boolean),
						),
					),
				].sort(),
			attribute,
		);
		expect(
			renderedIdentities,
			`${algorithmId}/${caseLabel} ${kind} DOM targets must exactly match Inspector`,
		).toEqual(expectedIdentities);
	}
}

async function assertGraphGeometry(
	page: Page,
	auditCase: AuditCase,
): Promise<void> {
	const algorithmId = auditCase.algorithm_id;
	const caseLabel = auditCase.label;
	const current = workspace(page);
	const graph = renderedFlowGraph(page);
	const pathCount = await graph.locator("path").count();
	expect(
		pathCount,
		`${algorithmId}/${caseLabel} non-vacuous rendered path set`,
	).toBeGreaterThan(0);
	expect(
		await graph.locator("path").evaluateAll((paths) =>
			paths.every((path) => {
				const data = path.getAttribute("d") ?? "";
				return data.length > 0 && !/NaN|Infinity/u.test(data);
			}),
		),
		`${algorithmId}/${caseLabel} finite edge geometry`,
	).toBe(true);
	const stageBadgeContrasts = await graph
		.locator(".flow-overlay-stage-badge:visible")
		.evaluateAll((badges) => {
			const parseRgb = (color: string): [number, number, number] => {
				const colorSrgb = color.match(
					/^color\(srgb\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)/u,
				);
				if (colorSrgb !== null) {
					return [
						Number(colorSrgb[1] ?? 0) * 255,
						Number(colorSrgb[2] ?? 0) * 255,
						Number(colorSrgb[3] ?? 0) * 255,
					];
				}
				const channels = color
					.match(/[\d.]+/gu)
					?.slice(0, 3)
					.map(Number);
				if (channels === undefined || channels.length !== 3) {
					throw new Error(`expected RGB color, received ${color}`);
				}
				return [channels[0] ?? 0, channels[1] ?? 0, channels[2] ?? 0];
			};
			const linear = (channel: number) => {
				const normalized = channel / 255;
				return normalized <= 0.04045
					? normalized / 12.92
					: ((normalized + 0.055) / 1.055) ** 2.4;
			};
			const luminance = ([red, green, blue]: [number, number, number]) =>
				0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue);
			return badges.map((badge) => {
				const rect = badge.querySelector("rect");
				const text = badge.querySelector("text");
				if (rect === null || text === null) {
					throw new Error("stage badge requires one rect and one text label");
				}
				const background = luminance(parseRgb(getComputedStyle(rect).fill));
				const foreground = luminance(parseRgb(getComputedStyle(text).fill));
				return (
					(Math.max(background, foreground) + 0.05) /
					(Math.min(background, foreground) + 0.05)
				);
			});
		});
	expect(
		Math.min(...stageBadgeContrasts, Number.POSITIVE_INFINITY),
		`${algorithmId}/${caseLabel} stage badge WCAG AA text contrast`,
	).toBeGreaterThanOrEqual(4.5);
	const standaloneFeasibility = graph.locator(
		'.flow-feasibility-layer[data-feasibility-domain="standalone-transformation"]',
	);
	if ((await standaloneFeasibility.count()) > 0) {
		expect(
			await standaloneFeasibility.count(),
			`${algorithmId}/${caseLabel} one standalone transformation`,
		).toBe(1);
		const expectedDomainNodes = Number(
			await standaloneFeasibility.getAttribute(
				"data-feasibility-domain-node-count",
			),
		);
		const expectedOriginalArcs = Number(
			await standaloneFeasibility.getAttribute(
				"data-feasibility-rendered-original-arc-count",
			),
		);
		const domainNodes = standaloneFeasibility.locator(
			".flow-feasibility-domain-node[data-feasibility-domain-node]",
		);
		expect(
			await domainNodes.count(),
			`${algorithmId}/${caseLabel} every standalone-domain node is rendered`,
		).toBe(expectedDomainNodes);
		expect(
			await domainNodes.evaluateAll((nodes) => {
				const ids = nodes.map((node) =>
					node.getAttribute("data-feasibility-domain-node"),
				);
				const points = nodes.map((node) => {
					const circle = node.querySelector("circle");
					return `${circle?.getAttribute("cx")}:${circle?.getAttribute("cy")}`;
				});
				return (
					ids.every((id) => id !== null && id.length > 0) &&
					new Set(ids).size === ids.length &&
					new Set(points).size === points.length
				);
			}),
			`${algorithmId}/${caseLabel} standalone-domain node identities and positions`,
		).toBe(true);
		const originalArcs = standaloneFeasibility.locator(
			'.flow-feasibility-arc[data-feasibility-arc-kind="original"]',
		);
		expect(
			await originalArcs.count(),
			`${algorithmId}/${caseLabel} every published standalone-domain arc is rendered`,
		).toBe(expectedOriginalArcs);
		expect(
			await originalArcs.evaluateAll((arcs) => {
				const ids = arcs.map((arc) => arc.getAttribute("data-feasibility-arc"));
				return (
					ids.every((id) => id !== null && id.length > 0) &&
					new Set(ids).size === ids.length
				);
			}),
			`${algorithmId}/${caseLabel} standalone-domain arc identities`,
		).toBe(true);
		expect(
			await graph.locator(".flow-node-frame, .flow-original-edge").count(),
			`${algorithmId}/${caseLabel} public graph is not mixed into an identity-incompatible transformed domain`,
		).toBe(0);
		const overflow = await current.evaluate(
			(element) => element.scrollWidth - element.clientWidth,
		);
		expect(
			overflow,
			`${algorithmId}/${caseLabel} standalone transformation horizontal overflow`,
		).toBeLessThanOrEqual(2);
		return;
	}
	if ((await graph.getAttribute("data-flow-lod")) === "overview") {
		const clusters = graph.locator(
			".flow-overview-cluster[data-cluster-id][data-cluster-count]",
		);
		expect(
			await clusters.count(),
			`${algorithmId}/${caseLabel} non-empty overview clusters`,
		).toBeGreaterThan(0);
		expect(
			await clusters.evaluateAll((items) =>
				items.reduce(
					(total, item) =>
						total + Number(item.getAttribute("data-cluster-count") ?? 0),
					0,
				),
			),
			`${algorithmId}/${caseLabel} overview preserves every node`,
		).toBe(auditCase.node_count);
		const clusterTransforms = await clusters.evaluateAll((items) =>
			items.map((item) => item.getAttribute("transform") ?? ""),
		);
		expect(
			new Set(clusterTransforms).size,
			`${algorithmId}/${caseLabel} distinct overview cluster positions`,
		).toBe(clusterTransforms.length);
		const originalAggregates = graph.locator(
			'[data-aggregate-kind="original-edge"][data-aggregate-count]',
		);
		expect(
			await originalAggregates.evaluateAll((items) =>
				items.reduce(
					(total, item) =>
						total + Number(item.getAttribute("data-aggregate-count") ?? 0),
					0,
				),
			),
			`${algorithmId}/${caseLabel} overview preserves every original edge`,
		).toBe(auditCase.edge_count);
		const overflow = await current.evaluate(
			(element) => element.scrollWidth - element.clientWidth,
		);
		expect(
			overflow,
			`${algorithmId}/${caseLabel} overview workspace horizontal overflow`,
		).toBeLessThanOrEqual(2);
		return;
	}
	expect(
		await graph.locator(".flow-node-frame[data-node-id]").count(),
		`${algorithmId}/${caseLabel} every graph node is rendered`,
	).toBe(auditCase.node_count);
	expect(
		await graph.locator(".flow-original-edge[data-edge-id]").count(),
		`${algorithmId}/${caseLabel} every original edge is rendered`,
	).toBe(auditCase.edge_count);
	const nodeTransforms = await graph
		.locator(".flow-node-frame[data-node-id]")
		.evaluateAll((nodes) =>
			nodes.map((node) => node.getAttribute("transform") ?? ""),
		);
	expect(
		new Set(nodeTransforms).size,
		`${algorithmId}/${caseLabel} distinct node positions`,
	).toBe(nodeTransforms.length);
	expect(
		await graph
			.locator(".flow-original-edge[data-parallel-index][data-parallel-count]")
			.evaluateAll((edges) =>
				edges.every((edge) => {
					const index = Number(edge.getAttribute("data-parallel-index"));
					const count = Number(edge.getAttribute("data-parallel-count"));
					return (
						Number.isInteger(index) &&
						Number.isInteger(count) &&
						index >= 1 &&
						index <= count
					);
				}),
			),
		`${algorithmId}/${caseLabel} stable parallel lane metadata`,
	).toBe(true);
	const invalidAnnotations = await graph
		.locator(".flow-edge-label-group")
		.evaluateAll((labels) =>
			labels
				.map((label) => {
					const parallelCount = Number(
						label.getAttribute("data-parallel-count"),
					);
					const leader = label.querySelector(
						".flow-edge-label-leader",
					) as SVGLineElement | null;
					const leaderLength =
						leader === null
							? 0
							: Math.hypot(
									leader.x1.baseVal.value - leader.x2.baseVal.value,
									leader.y1.baseVal.value - leader.y2.baseVal.value,
								);
					return {
						edge: label.getAttribute("data-edge-label-for"),
						parallelCount,
						leaderLength,
						halos: label.querySelectorAll(".flow-edge-label-leader-halo")
							.length,
						anchors: label.querySelectorAll(".flow-edge-label-anchor").length,
						badges: label.querySelectorAll(".flow-edge-parallel-badge").length,
					};
				})
				.filter(
					({ parallelCount, leaderLength, halos, anchors, badges }) =>
						!(
							leaderLength >= 18 ||
							(parallelCount === 1 && leaderLength <= 1)
						) ||
						halos !== 1 ||
						anchors !== (parallelCount > 1 ? 0 : 1) ||
						badges !== (parallelCount > 1 ? 1 : 0),
				),
		);
	expect(
		invalidAnnotations,
		`${algorithmId}/${caseLabel} legible annotation leaders and lane badges`,
	).toEqual([]);
	const expectedLaneTokens =
		(await graph.getAttribute("data-flow-lod")) === "detail"
			? await graph
					.locator(
						'.flow-original-edge[data-parallel-count]:not([data-parallel-count="1"])',
					)
					.count()
			: 0;
	expect(
		await graph.locator(".flow-edge-route-lane-token").evaluateAll((tokens) =>
			tokens.every((token) => {
				const edgeId = token.getAttribute("data-edge-id");
				const lane = token.getAttribute("data-route-lane-token") ?? "";
				return (
					edgeId !== null &&
					/^\d+\/\d+$/u.test(lane) &&
					token.querySelectorAll("path, circle").length === 1 &&
					token.querySelectorAll("text").length === 1
				);
			}),
		),
		`${algorithmId}/${caseLabel} parallel lane token structure`,
	).toBe(true);
	expect(
		await graph.locator(".flow-edge-route-lane-token").count(),
		`${algorithmId}/${caseLabel} every detailed parallel edge has one token`,
	).toBe(expectedLaneTokens);
	if ((await graph.getAttribute("data-flow-lod")) === "structure") {
		const callouts = graph.locator(".flow-node-trace-callout:visible");
		const expectedCalloutOwners = await graph
			.locator('[data-trace-callout-expected="true"]:visible')
			.evaluateAll((nodes) =>
				nodes
					.map((node) => node.getAttribute("data-node-id"))
					.filter((id): id is string => id !== null)
					.sort(),
			);
		const actualCalloutOwners = await callouts.evaluateAll((items) =>
			items
				.map((item) => item.getAttribute("data-node-trace-for"))
				.filter((id): id is string => id !== null)
				.sort(),
		);
		expect(
			await callouts.count(),
			`${algorithmId}/${caseLabel} bounded structure trace callouts`,
		).toBeLessThanOrEqual(FLOW_LOD_LIMITS.structureNodeTraceCallouts);
		expect(
			actualCalloutOwners,
			`${algorithmId}/${caseLabel} every selected Structure trace candidate has a callout`,
		).toEqual(expectedCalloutOwners);
		await expect(
			callouts.locator(".flow-node-trace-leader"),
			`${algorithmId}/${caseLabel} every callout is linked to its node`,
		).toHaveCount(await callouts.count());
		expect(
			await callouts.evaluateAll((items) =>
				items.every(
					(item) =>
						item.getAttribute("data-node-trace-for") !== null &&
						item.getAttribute("data-node-trace-for") ===
							item.closest("[data-node-id]")?.getAttribute("data-node-id"),
				),
			),
			`${algorithmId}/${caseLabel} every callout names the node reached by its leader`,
		).toBe(true);
		const calloutGeometry = await callouts.evaluateAll((items) => {
			const overlapArea = (left: DOMRect, right: DOMRect) =>
				Math.max(
					0,
					Math.min(left.right, right.right) - Math.max(left.left, right.left),
				) *
				Math.max(
					0,
					Math.min(left.bottom, right.bottom) - Math.max(left.top, right.top),
				);
			const svg = items[0]?.closest("svg");
			const obstacles =
				svg === null || svg === undefined
					? []
					: [
							...svg.querySelectorAll<SVGGraphicsElement>(
								".flow-node-frame > circle.flow-node, .flow-edge-label-bg",
							),
						].filter((element) => {
							const box = element.getBoundingClientRect();
							const style = getComputedStyle(element);
							return (
								box.width > 0 &&
								box.height > 0 &&
								style.display !== "none" &&
								style.visibility !== "hidden"
							);
						});
			const labels = items.flatMap((item) => {
				const label = item.querySelector<SVGTextElement>(".flow-node-trace");
				return label === null ? [] : [label];
			});
			const errors = items.flatMap((item, index) => {
				const owner = item.closest("[data-node-id]");
				const ownerId = owner?.getAttribute("data-node-id");
				const declaredOwner = item.getAttribute("data-node-trace-for");
				const line = item.querySelector<SVGLineElement>(
					".flow-node-trace-leader",
				);
				const label = item.querySelector<SVGTextElement>(".flow-node-trace");
				if (line === null || label === null) return ["incomplete"];
				const x1 = line.x1.baseVal.value;
				const y1 = line.y1.baseVal.value;
				const x2 = line.x2.baseVal.value;
				const y2 = line.y2.baseVal.value;
				const labelX = label.x.baseVal[0]?.value ?? 0;
				const labelY = label.y.baseVal[0]?.value ?? 0;
				const style = getComputedStyle(line);
				const labelBox = label.getBoundingClientRect();
				const result: string[] = [];
				if (ownerId === null || ownerId !== declaredOwner) result.push("owner");
				if (
					style.stroke === "none" ||
					style.display === "none" ||
					style.visibility === "hidden" ||
					Number(style.strokeOpacity) === 0 ||
					Number(style.opacity) === 0
				)
					result.push("paint");
				if (Math.abs(Math.hypot(x1, y1) - 30) > 0.6) result.push("origin");
				if (Math.hypot(x2 - x1, y2 - y1) < 3) result.push("length");
				if (
					Math.hypot(x2 - labelX, y2 - labelY) >=
					Math.hypot(x1 - labelX, y1 - labelY)
				)
					result.push("direction");
				if (
					labels.some(
						(other, otherIndex) =>
							otherIndex !== index &&
							overlapArea(labelBox, other.getBoundingClientRect()) > 1,
					)
				)
					result.push("callout-collision");
				for (const obstacle of obstacles) {
					if (overlapArea(labelBox, obstacle.getBoundingClientRect()) <= 1)
						continue;
					const nodeId = obstacle
						.closest("[data-node-id]")
						?.getAttribute("data-node-id");
					const edgeId = obstacle
						.closest("[data-edge-label-for]")
						?.getAttribute("data-edge-label-for");
					result.push(
						`graph-collision:${nodeId === null || nodeId === undefined ? `edge:${edgeId ?? "unknown"}` : `node:${nodeId}`}`,
					);
				}
				return result.map((error) => `${declaredOwner ?? "unknown"}:${error}`);
			});
			return { count: labels.length, errors };
		});
		expect(
			calloutGeometry.errors,
			`${algorithmId}/${caseLabel} structure callout ownership, leader, and collision geometry`,
		).toEqual([]);
		expect(
			await graph
				.locator(
					".flow-orlin-max-component:not(.flow-orlin-max-component-contracted) text:visible",
				)
				.count(),
			`${algorithmId}/${caseLabel} no broad singleton-component annotations in structure density`,
		).toBe(0);
		if (algorithmId === "orlin-max-flow") {
			const inactiveOriginalCompactArcs = graph.locator(
				'.flow-orlin-max-compact-original:not([data-orlin-max-compact-active="true"]):not([data-orlin-max-scan]):visible',
			);
			await expect(
				inactiveOriginalCompactArcs,
				`${algorithmId}/${caseLabel} no duplicate inactive original compact-network arcs in structure density`,
			).toHaveCount(0);
		}
	}

	const labelOverlap = await graph
		.locator(".flow-edge-label-bg")
		.evaluateAll((labels) => {
			const boxes = labels
				.map((label) => {
					const group = label.closest("[data-edge-label-for]");
					return {
						edge: group?.getAttribute("data-edge-label-for") ?? "unknown",
						collisionFree:
							group?.getAttribute("data-label-collision-free") ?? "unknown",
						transform: group?.getAttribute("transform") ?? "none",
						box: label.getBoundingClientRect(),
					};
				})
				.filter(({ box }) => box.width > 0 && box.height > 0);
			const overlaps: string[] = [];
			for (let left = 0; left < boxes.length; left += 1) {
				for (let right = left + 1; right < boxes.length; right += 1) {
					const a = boxes[left];
					const b = boxes[right];
					if (
						a !== undefined &&
						b !== undefined &&
						Math.min(a.box.right, b.box.right) -
							Math.max(a.box.left, b.box.left) >
							3 &&
						Math.min(a.box.bottom, b.box.bottom) -
							Math.max(a.box.top, b.box.top) >
							3
					) {
						overlaps.push(
							`${a.edge} free=${a.collisionFree} ${a.transform} [${a.box.x.toFixed(1)},${a.box.y.toFixed(1)},${a.box.width.toFixed(1)},${a.box.height.toFixed(1)}] ↔ ${b.edge} free=${b.collisionFree} ${b.transform} [${b.box.x.toFixed(1)},${b.box.y.toFixed(1)},${b.box.width.toFixed(1)},${b.box.height.toFixed(1)}]`,
						);
					}
				}
			}
			return { labels: boxes.length, overlaps };
		});
	expect(
		labelOverlap.overlaps,
		`${algorithmId}/${caseLabel} visible edge-label collisions`,
	).toEqual([]);
	const dualLabelLayout = await graph
		.locator(".flow-planar-dual-edge text[data-planar-dual-label-for]")
		.evaluateAll((labels) => {
			const boxes = labels.map((label) => {
				const background = label
					.closest("[data-planar-dual-edge]")
					?.querySelector(".flow-planar-dual-label-bg");
				return (background ?? label).getBoundingClientRect();
			});
			const svg = labels[0]?.closest("svg") as SVGSVGElement | null | undefined;
			const obstacleBoxes = (selector: string) =>
				Array.from(svg?.querySelectorAll<Element>(selector) ?? [])
					.map((element) => element.getBoundingClientRect())
					.filter((box) => box.width > 0 && box.height > 0);
			const overlapCount = (obstacles: readonly DOMRect[]) =>
				boxes.reduce(
					(total, box) =>
						total +
						obstacles.filter(
							(obstacle) =>
								Math.min(box.right, obstacle.right) -
									Math.max(box.left, obstacle.left) >
									2 &&
								Math.min(box.bottom, obstacle.bottom) -
									Math.max(box.top, obstacle.top) >
									2,
						).length,
					0,
				);
			let overlaps = 0;
			for (let left = 0; left < boxes.length; left += 1) {
				for (let right = left + 1; right < boxes.length; right += 1) {
					const a = boxes[left];
					const b = boxes[right];
					if (
						a !== undefined &&
						b !== undefined &&
						Math.min(a.right, b.right) - Math.max(a.left, b.left) > 3 &&
						Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 3
					) {
						overlaps += 1;
					}
				}
			}
			const invalidAssociations = labels.filter((label) => {
				const edge = label.closest("[data-planar-dual-edge]");
				const tether = edge?.querySelector(
					".flow-planar-dual-label-tether",
				) as SVGLineElement | null;
				if (
					edge?.getAttribute("data-planar-dual-edge") !==
						label.getAttribute("data-planar-dual-label-for") ||
					tether === null
				) {
					return true;
				}
				return (
					Math.hypot(
						tether.x2.baseVal.value - tether.x1.baseVal.value,
						tether.y2.baseVal.value - tether.y1.baseVal.value,
					) < 8
				);
			}).length;
			return {
				count: labels.length,
				overlaps,
				invalidAssociations,
				nodeOverlaps: overlapCount(obstacleBoxes(".flow-node-frame")),
				faceOverlaps: overlapCount(
					obstacleBoxes(".flow-planar-dual-face circle"),
				),
				edgeLabelOverlaps: overlapCount(obstacleBoxes(".flow-edge-label-bg")),
			};
		});
	expect(
		dualLabelLayout.overlaps,
		`${algorithmId}/${caseLabel} planar-dual label collisions`,
	).toBe(0);
	expect(
		dualLabelLayout.invalidAssociations,
		`${algorithmId}/${caseLabel} planar-dual label-to-arc tethers`,
	).toBe(0);
	expect(
		dualLabelLayout.nodeOverlaps,
		`${algorithmId}/${caseLabel} planar-dual labels overlap nodes`,
	).toBe(0);
	expect(
		dualLabelLayout.faceOverlaps,
		`${algorithmId}/${caseLabel} planar-dual labels overlap faces`,
	).toBe(0);
	expect(
		dualLabelLayout.edgeLabelOverlaps,
		`${algorithmId}/${caseLabel} planar-dual labels overlap primal labels`,
	).toBe(0);
	const overflow = await current.evaluate(
		(element) => element.scrollWidth - element.clientWidth,
	);
	expect(
		overflow,
		`${algorithmId}/${caseLabel} workspace horizontal overflow`,
	).toBeLessThanOrEqual(2);
}

async function assertRichAlgorithmState(
	page: Page,
	algorithmId: string,
	caseLabel: string,
	options: Readonly<{
		keepOpen?: boolean;
		requireExpectedPanel?: boolean;
	}> = {},
): Promise<boolean> {
	const current = workspace(page);
	const disclosure = current.locator(".flow-rich-status-disclosure");
	const count = await disclosure.count();
	const expectedPanel = richPanelByAlgorithm.get(algorithmId)?.testId;
	const keepOpen = options.keepOpen ?? false;
	const requireExpectedPanel = options.requireExpectedPanel ?? true;
	if (count === 0) {
		expect(
			expectedPanel === undefined || !requireExpectedPanel,
			`${algorithmId}/${caseLabel} algorithm-state disclosure`,
		).toBe(true);
		return false;
	}
	expect(count, `${algorithmId}/${caseLabel} one rich-state disclosure`).toBe(
		1,
	);
	const summary = disclosure.getByLabel("Show algorithm state details");
	if ((await disclosure.getAttribute("open")) === null) await summary.click();
	await expect(disclosure).toHaveAttribute("open", "");
	const body = disclosure.locator(".flow-rich-status-body");
	await expect(body).toBeVisible();

	if (expectedPanel !== undefined && requireExpectedPanel) {
		await expect(current.getByTestId(expectedPanel)).toBeVisible();
	}
	const panels = body.locator('[data-testid$="panel"]');
	for (let index = 0; index < (await panels.count()); index += 1) {
		const panel = panels.nth(index);
		await expect(panel).toBeVisible();
		await panel.scrollIntoViewIfNeeded();
		const svgs = panel.locator('svg[role="img"]');
		expect(
			await svgs.count(),
			`${algorithmId}/${caseLabel} rich panel SVG`,
		).toBeGreaterThan(0);
		const invalidGeometry = await svgs.evaluateAll((items) =>
			items.flatMap((item, svgIndex) => {
				if (!(item instanceof SVGSVGElement)) return [`svg ${svgIndex}: type`];
				const viewBox = item.viewBox.baseVal;
				const svgMatrix = item.getCTM();
				const failures: string[] = [];
				if (svgMatrix === null) return [`svg ${svgIndex}: missing transform`];
				for (const path of item.querySelectorAll("path[d]")) {
					if (path.closest("defs") !== null) continue;
					if (!(path instanceof SVGGraphicsElement)) {
						failures.push(`svg ${svgIndex}: non-graphics path`);
						continue;
					}
					const data = path.getAttribute("d") ?? "";
					if (data.length === 0 || /NaN|Infinity/u.test(data)) {
						failures.push(`svg ${svgIndex}: non-finite path`);
						continue;
					}
					const bounds = path.getBBox();
					const pathMatrix = path.getCTM();
					if (pathMatrix === null) {
						failures.push(`svg ${svgIndex}: missing path transform`);
						continue;
					}
					const relative = svgMatrix.inverse().multiply(pathMatrix);
					const corners = [
						new DOMPoint(bounds.x, bounds.y),
						new DOMPoint(bounds.x + bounds.width, bounds.y),
						new DOMPoint(bounds.x, bounds.y + bounds.height),
						new DOMPoint(bounds.x + bounds.width, bounds.y + bounds.height),
					].map((point) => point.matrixTransform(relative));
					const left = Math.min(...corners.map((point) => point.x));
					const top = Math.min(...corners.map((point) => point.y));
					const right = Math.max(...corners.map((point) => point.x));
					const bottom = Math.max(...corners.map((point) => point.y));
					if (
						left < viewBox.x - 1 ||
						top < viewBox.y - 1 ||
						right > viewBox.x + viewBox.width + 1 ||
						bottom > viewBox.y + viewBox.height + 1
					) {
						failures.push(`svg ${svgIndex}: path outside viewBox`);
					}
					for (const attribute of ["marker-start", "marker-end"] as const) {
						const reference = path.getAttribute(attribute);
						const markerId = reference?.match(/^url\(#(.+)\)$/u)?.[1];
						if (
							markerId !== undefined &&
							item.querySelector(`#${CSS.escape(markerId)} marker`) === null &&
							item.querySelector(`marker#${CSS.escape(markerId)}`) === null
						) {
							failures.push(`svg ${svgIndex}: unresolved ${attribute}`);
						}
					}
				}
				return failures;
			}),
		);
		expect(
			invalidGeometry,
			`${algorithmId}/${caseLabel} rich-state path and marker geometry`,
		).toEqual([]);
		const annotationGeometry = await panel.evaluate((element) => {
			const boxes = [...element.querySelectorAll(".flow-panel-edge-label")]
				.map((label, index) => ({
					id:
						label.closest("[data-ipm-arc]")?.getAttribute("data-ipm-arc") ??
						label.closest("[data-eipm-edge]")?.getAttribute("data-eipm-edge") ??
						label
							.closest("[data-mrcmcf-edge]")
							?.getAttribute("data-mrcmcf-edge") ??
						`label-${index}`,
					box: label.getBoundingClientRect(),
				}))
				.filter(({ box }) => box.width > 0 && box.height > 0);
			const collisions = boxes.flatMap((left, leftIndex) =>
				boxes.flatMap((right, rightIndex) =>
					rightIndex > leftIndex &&
					Math.min(left.box.right, right.box.right) -
						Math.max(left.box.left, right.box.left) >
						2 &&
					Math.min(left.box.bottom, right.box.bottom) -
						Math.max(left.box.top, right.box.top) >
						2
						? [
								{
									left: { id: left.id, ...left.box.toJSON() },
									right: { id: right.id, ...right.box.toJSON() },
								},
							]
						: [],
				),
			);
			const invalidLeaders = [
				...element.querySelectorAll(".flow-panel-edge-label-leader"),
			].filter((leader) => {
				if (!(leader instanceof SVGLineElement)) return true;
				return (
					!Number.isFinite(leader.x1.baseVal.value) ||
					!Number.isFinite(leader.y1.baseVal.value) ||
					!Number.isFinite(leader.x2.baseVal.value) ||
					!Number.isFinite(leader.y2.baseVal.value) ||
					Math.hypot(
						leader.x1.baseVal.value - leader.x2.baseVal.value,
						leader.y1.baseVal.value - leader.y2.baseVal.value,
					) < 4
				);
			});
			return {
				collisions,
				invalidLeaderCount: invalidLeaders.length,
			};
		});
		expect(
			annotationGeometry.collisions,
			`${algorithmId}/${caseLabel} rich-state edge-label collisions`,
		).toEqual([]);
		expect(
			annotationGeometry.invalidLeaderCount,
			`${algorithmId}/${caseLabel} rich-state annotation leaders`,
		).toBe(0);
	}

	const overflow = await current.evaluate(
		(element) => element.scrollWidth - element.clientWidth,
	);
	expect(
		overflow,
		`${algorithmId}/${caseLabel} open algorithm-state horizontal overflow`,
	).toBeLessThanOrEqual(2);
	if (!keepOpen) await summary.click();
	return true;
}

async function auditInteriorTimelineSamples(
	page: Page,
	auditCase: AuditCase,
): Promise<void> {
	const algorithmId = auditCase.algorithm_id;
	const caseLabel = auditCase.label;
	const slider = workspace(page).locator(".flow-timeline input[type='range']");
	const maximum = Number(await slider.getAttribute("max"));
	const positions = new Set([
		Math.floor(maximum / 3),
		Math.floor((maximum * 2) / 3),
	]);
	for (const position of positions) {
		if (position <= 0 || position >= maximum) continue;
		await seekRawEvent(page, position, maximum);
		await expect(
			workspace(page).getByRole("button", { name: "Next step" }),
		).toBeEnabled({ timeout: 60_000 });
		await assertEventPresentation(page, algorithmId, caseLabel);
		await assertGraphGeometry(page, auditCase);
	}
}

test.describe("flow representative production-renderer audit", () => {
	test.skip(
		!representativeReleaseAudit &&
			!representativePartialAudit &&
			!representativeDiagnosticAudit,
		"Run through the dedicated flow-representative-browser-audit gate",
	);
	let releaseAuditFailed = false;
	test.beforeAll(() => {
		if (screenshotAuditDirectory === undefined) return;
		if (existsSync(screenshotAuditDirectory)) {
			throw new Error(
				`The release screenshot audit requires a fresh output directory; remove or choose a new path: ${screenshotAuditDirectory}`,
			);
		}
		mkdirSync(screenshotAuditDirectory, { recursive: true });
	});
	test.afterEach(({ page: _page }, testInfo) => {
		if (
			representativeReleaseAudit &&
			testInfo.status !== testInfo.expectedStatus
		) {
			releaseAuditFailed = true;
		}
	});

	expect(manifest.schema_version).toBe(17);
	expect(manifest.algorithm_count).toBe(FLOW_BROWSER_ALGORITHM_IDS.length);
	expect(casesByAlgorithm.size).toBe(FLOW_BROWSER_ALGORITHM_IDS.length);
	expect(manifest.complexity_growth).toHaveLength(
		FLOW_BROWSER_ALGORITHM_IDS.length,
	);
	const witnessedOverlayFields = new Set(
		manifest.cases.flatMap((auditCase) =>
			Object.keys(auditCase.overlay_witnesses),
		),
	);
	expect([...witnessedOverlayFields].sort()).toEqual(
		FLOW_SCENE_V9_OVERLAY_DECODERS.map(([field]) => field).sort(),
	);
	for (const algorithmId of FLOW_BROWSER_ALGORITHM_IDS) {
		const cases = casesByAlgorithm.get(algorithmId) ?? [];
		expect(
			cases.filter((auditCase) => BigInt(auditCase.primary_work) >= 12n).length,
			`${algorithmId} complexity-rich source-event representatives`,
		).toBeGreaterThanOrEqual(2);
		const growth = manifest.complexity_growth.find(
			(witness) => witness.algorithm_id === algorithmId,
		);
		expect(growth, `${algorithmId} complexity-growth witness`).toBeDefined();
		expect(
			growth?.controlled,
			`${algorithmId} complexity growth must stay within one declared input family`,
		).toBe(true);
		expect(
			growth?.control_contract,
			`${algorithmId} typed scale-pair contract`,
		).toMatch(/-v\d+$/u);
		expect(growth?.control_digest).toMatch(/^[0-9a-f]{64}$/u);
		const smaller = cases.find(
			(candidate) => candidate.label === growth?.smaller_label,
		);
		const larger = cases.find(
			(candidate) => candidate.label === growth?.larger_label,
		);
		expect(smaller, `${algorithmId} smaller complexity driver`).toBeDefined();
		expect(larger, `${algorithmId} larger complexity driver`).toBeDefined();
		if (growth === undefined || smaller === undefined || larger === undefined) {
			throw new Error(`${algorithmId} complexity witness is incomplete`);
		}
		expect(BigInt(growth?.smaller_primary_work ?? "0")).toBeLessThan(
			BigInt(growth?.larger_primary_work ?? "0"),
		);
		expect(BigInt(growth?.smaller_driver ?? "0")).toBe(
			measuredComplexityDriver(growth?.driver ?? "graph-entity-count", smaller),
		);
		expect(BigInt(growth?.larger_driver ?? "0")).toBe(
			measuredComplexityDriver(growth?.driver ?? "graph-entity-count", larger),
		);
		expect(BigInt(growth?.smaller_driver ?? "0")).toBeLessThan(
			BigInt(growth?.larger_driver ?? "0"),
		);
		expect(smaller?.primary_work).toBe(growth?.smaller_primary_work);
		expect(larger?.primary_work).toBe(growth?.larger_primary_work);
		expect(smaller?.event_count).toBe(growth?.smaller_event_count);
		expect(larger?.event_count).toBe(growth?.larger_event_count);
	}
	for (const representative of manifest.cases) {
		for (const [overlay, witness] of Object.entries(
			representative.overlay_witnesses,
		)) {
			expect(
				witness.active_overlays,
				`${representative.algorithm_id}/${representative.label} dedicated ${overlay} witness`,
			).toContain(overlay);
			expect(witness.overlay_scalar_values[overlay]).toBeDefined();
		}
		expect(
			BigInt(representative.primary_work_boundary_count),
			`${representative.algorithm_id}/${representative.label} source work actions`,
		).toBeGreaterThan(0n);
		for (const witness of [
			representative.first_primary_work,
			representative.maximum_primary_work,
		]) {
			expect(
				BigInt(witness.primary_delta),
				`${representative.algorithm_id}/${representative.label} positive primary-work witness`,
			).toBeGreaterThan(0n);
			expect(witness.catalog_id).not.toMatch(
				/\.(?:primary-work-unit|work-observation)$/u,
			);
			expect(witness.work_first).not.toBeNull();
			expect(witness.work_last).not.toBeNull();
			expect(witness.work_total).not.toBeNull();
			expect(witness.work_first).toBe("1");
			expect(witness.work_last).toBe(witness.work_total);
		}
		expect(
			BigInt(representative.maximum_primary_work.primary_delta),
			`${representative.algorithm_id}/${representative.label} maximum source-boundary primary work`,
		).toBe(BigInt(representative.maximum_primary_work_delta));
		expect(
			[
				representative.first_detail,
				representative.middle_detail,
				representative.last_detail,
				representative.first_primary_work,
				representative.maximum_primary_work,
			].every(
				(witness) =>
					!witness.catalog_id.endsWith(".primary-work-unit") &&
					!witness.catalog_id.endsWith(".work-observation"),
			),
			`${representative.algorithm_id}/${representative.label} contains no synthetic work boundaries`,
		).toBe(true);
		expect(
			createHash("sha256")
				.update(JSON.stringify(representative.scenario))
				.digest("hex"),
			`${representative.algorithm_id}/${representative.label} input digest`,
		).toBe(representative.scenario_digest);
	}

	test.afterAll(() => {
		if (!representativeReleaseAudit) return;
		if (releaseAuditFailed) return;
		expect(
			screenshotAuditDirectory,
			"the representative release gate must persist visual audit artifacts",
		).toBeDefined();
		expect(auditAlgorithmIds).toEqual(FLOW_BROWSER_ALGORITHM_IDS);
		expect(screenshotAuditRecords).toHaveLength(
			FLOW_BROWSER_ALGORITHM_IDS.length * 3,
		);
		const directory = screenshotAuditDirectory as string;
		for (const algorithmId of auditAlgorithmIds) {
			const auditCase = largestVisualAuditCase(
				casesByAlgorithm.get(algorithmId) ?? [],
			);
			const records = screenshotAuditRecords.filter(
				(record) => record.algorithm_id === algorithmId,
			);
			expect(records, `${algorithmId} screenshot records`).toHaveLength(3);
			expect(new Set(records.map((record) => record.witness))).toEqual(
				new Set<ScreenshotWitness>(["early", "middle", "late"]),
			);
			expect(
				records.every((record) => record.case_label === auditCase.label),
				`${algorithmId} screenshots use the largest representative graph`,
			).toBe(true);
			const candidateEvents = new Set(
				screenshotCandidateWitnesses(auditCase).map((witness) => witness.event),
			);
			expect(
				records.every((record) => candidateEvents.has(record.event)),
				`${algorithmId} screenshots come from audited source witnesses`,
			).toBe(true);
			expect(new Set(records.map((record) => record.event)).size).toBe(3);
			expect(
				new Set(records.map((record) => record.graph_projection_sha256)).size,
				`${algorithmId} screenshot graph projections`,
			).toBe(3);
			expect(
				new Set(records.map((record) => record.sha256)).size,
				`${algorithmId} screenshot images`,
			).toBe(3);
		}
		expect(
			new Set(screenshotAuditRecords.map((record) => record.sha256)).size,
			"every visual-audit image is an independent artifact",
		).toBe(screenshotAuditRecords.length);
		for (const record of screenshotAuditRecords) {
			expect(record.file).toBe(`${record.sha256}.png`);
			const path = join(directory, record.file);
			expect(existsSync(path), `${record.file} exists`).toBe(true);
			expect(statSync(path).size, `${record.file} byte size`).toBe(
				record.byte_size,
			);
			const bytes = readFileSync(path);
			expect(
				createHash("sha256").update(bytes).digest("hex"),
				`${record.file} content hash`,
			).toBe(record.sha256);
			const dimensions = pngDimensions(bytes);
			expect(dimensions.width).toBe(1600);
			expect(dimensions.height).toBe(1000);
		}
		const indexedPngs = screenshotAuditRecords
			.map((record) => record.file)
			.sort();
		const directoryEntries = readdirSync(directory, { withFileTypes: true });
		expect(
			directoryEntries.every((entry) => entry.isFile()),
			"visual-audit directory contains only regular artifact files",
		).toBe(true);
		expect(
			directoryEntries.map((entry) => entry.name).sort(),
			"visual-audit directory has no orphan or pending artifacts",
		).toEqual(indexedPngs);
		writeFileSync(
			join(directory, "visual-audit-index.json"),
			`${JSON.stringify(
				{
					schema_version: 2,
					manifest_sha256: createHash("sha256")
						.update(readFileSync(manifestPath))
						.digest("hex"),
					algorithm_count: auditAlgorithmIds.length,
					records: screenshotAuditRecords.sort((left, right) =>
						`${left.algorithm_id}:${left.witness}`.localeCompare(
							`${right.algorithm_id}:${right.witness}`,
						),
					),
				},
				null,
				2,
			)}\n`,
			"utf8",
		);
	});

	for (const algorithmId of auditAlgorithmIds) {
		const auditCases = casesByAlgorithm.get(algorithmId) ?? [];
		test(`@representative ${algorithmId} renders three distinct audited traces`, async ({
			page,
			browserName,
		}) => {
			test.skip(browserName !== "chromium", "full corpus uses one renderer");
			if (representativeDebug) {
				page.on("console", (message) => {
					if (message.text().startsWith("FLOW_AUDIT ")) {
						console.info(message.text());
					}
				});
			}
			test.setTimeout(
				representativeReleaseAudit ||
					representativePartialAudit ||
					representativeDiagnosticAudit
					? 3_600_000
					: 300_000,
			);
			expect(auditCases, `${algorithmId} representative count`).toHaveLength(3);
			expect(new Set(auditCases.map((item) => item.scenario_digest)).size).toBe(
				3,
			);
			expect(new Set(auditCases.map((item) => item.trace_digest)).size).toBe(3);
			const visualAuditCase = largestVisualAuditCase(auditCases);
			const richPanelExpectation = richPanelByAlgorithm.get(algorithmId);
			const debugProgress = (message: string) => {
				if (representativeDebug) console.info(`FLOW_TEST ${message}`);
			};

			await page.setViewportSize({ width: 1600, height: 1000 });
			await openProblem(
				page,
				problemFor(auditCases[0]?.scenario.payload.model.kind ?? ""),
			);
			await page.addStyleTag({
				content:
					"*,*::before,*::after{animation:none!important;transition:none!important;scroll-behavior:auto!important}",
			});
			for (const auditCase of auditCases) {
				debugProgress(`${auditCase.label} load`);
				await loadAuditCase(page, auditCase);
				const extent = await prepareTrace(page);
				debugProgress(`${auditCase.label} prepared`);
				expect(extent).toBe(auditCase.event_count);
				await assertDeclaredBoundaryAvailability(page, auditCase);
				await selectDetailBoundary(page);
				await stepToAuditedDetail(page, auditCase);
				await assertEventPresentation(page, algorithmId, auditCase.label);
				let richPanelValidated = richPanelExpectation === undefined;
				const firstDetailHasExpectedPanel =
					richPanelExpectation === undefined ||
					auditCase.first_detail.active_overlays.includes(
						richPanelExpectation.overlay,
					);
				await assertRichAlgorithmState(page, algorithmId, auditCase.label, {
					requireExpectedPanel: firstDetailHasExpectedPanel,
				});
				if (firstDetailHasExpectedPanel) richPanelValidated = true;
				const additionalWorkWitnesses = [
					auditCase.first_primary_work,
					auditCase.maximum_aggregation,
					auditCase.maximum_primary_work,
					...Object.values(auditCase.overlay_witnesses),
				]
					.filter(
						(witness): witness is AuditBoundaryWitness => witness !== null,
					)
					.filter(
						(witness, index, witnesses) =>
							witness.event !== auditCase.first_detail.event &&
							witnesses.findIndex(
								(candidate) => candidate.event === witness.event,
							) === index,
					);
				for (const witness of additionalWorkWitnesses) {
					await seekRawEvent(page, witness.event, auditCase.event_count);
					await assertAuditedWorkBoundary(page, auditCase, witness);
					if (
						richPanelExpectation !== undefined &&
						witness.active_overlays.includes(richPanelExpectation.overlay)
					) {
						await assertRichAlgorithmState(page, algorithmId, auditCase.label);
						richPanelValidated = true;
					}
				}
				expect(
					richPanelValidated,
					`${algorithmId}/${auditCase.label} algorithm-specific rich panel witness`,
				).toBe(true);
				await assertAdjacentSourceBoundaryMoves(page, auditCase);
				for (const witness of [
					auditCase.middle_detail,
					auditCase.last_detail,
				].filter(
					(candidate, index, witnesses) =>
						witnesses.findIndex(
							(witness) => witness.event === candidate.event,
						) === index,
				)) {
					await assertDetailBackwardRoundTrip(page, auditCase, witness);
				}
				debugProgress(`${auditCase.label} witnesses`);
				if (
					screenshotAuditDirectory !== undefined &&
					auditCase === visualAuditCase
				) {
					const selectedScreenshots = await selectDistinctScreenshotWitnesses(
						page,
						auditCase,
					);
					for (const {
						name,
						boundary: screenshotWitness,
						graphProjectionSha256,
					} of selectedScreenshots) {
						await seekRawEvent(
							page,
							screenshotWitness.event,
							auditCase.event_count,
						);
						await assertAuditedWorkBoundary(page, auditCase, screenshotWitness);
						await workspace(page)
							.getByTestId("flow-step-evidence")
							.scrollIntoViewIfNeeded();
						const richStateOpen = await assertRichAlgorithmState(
							page,
							algorithmId,
							auditCase.label,
							{
								keepOpen: true,
								requireExpectedPanel:
									richPanelExpectation === undefined ||
									screenshotWitness.active_overlays.includes(
										richPanelExpectation.overlay,
									),
							},
						);
						if (richStateOpen) {
							await workspace(page)
								.locator(".flow-rich-status-disclosure")
								.getByLabel("Show algorithm state details")
								.click();
						}
						const captureProjectionSha256 = createHash("sha256")
							.update(await visibleGraphProjection(renderedFlowGraph(page)))
							.digest("hex");
						expect(
							captureProjectionSha256,
							`${algorithmId}/${name} capture graph matches the selected witness`,
						).toBe(graphProjectionSha256);
						const pendingPath = join(
							screenshotAuditDirectory,
							`.pending-${algorithmId}-${name}`,
						);
						await page.screenshot({
							path: pendingPath,
							type: "png",
							animations: "disabled",
						});
						const bytes = readFileSync(pendingPath);
						const sha256 = createHash("sha256").update(bytes).digest("hex");
						const file = `${sha256}.png`;
						const screenshotPath = join(screenshotAuditDirectory, file);
						expect(
							existsSync(screenshotPath),
							`${algorithmId}/${name} must not duplicate an existing visual artifact`,
						).toBe(false);
						renameSync(pendingPath, screenshotPath);
						screenshotAuditRecords.push({
							algorithm_id: algorithmId,
							case_label: auditCase.label,
							witness: name,
							event: screenshotWitness.event,
							file,
							byte_size: bytes.byteLength,
							sha256,
							graph_projection_sha256: graphProjectionSha256,
						});
					}
				}
				debugProgress(`${auditCase.label} screenshots`);
				await workspace(page)
					.getByRole("button", { name: "First event" })
					.click();
				debugProgress(`${auditCase.label} reset-before-exhaustive`);
				if (
					representativeReleaseAudit ||
					representativePartialAudit ||
					representativeDiagnosticAudit
				) {
					const exhaustiveStart = Math.min(
						representativeStartEvent,
						auditCase.event_count,
					);
					if (exhaustiveStart > 0) {
						await seekRawEvent(page, exhaustiveStart, auditCase.event_count);
					}
					await expect(
						workspace(page).getByTestId("flow-timeline-readout"),
					).toHaveText(`Raw ${exhaustiveStart} / ${auditCase.event_count}`);
					await assertEverySourceBoundaryMoves(
						page,
						auditCase,
						exhaustiveStart,
					);
					await workspace(page)
						.getByRole("button", { name: "First event" })
						.click();
				}
				debugProgress(`${auditCase.label} exhaustive`);
				debugProgress(`${auditCase.label} reset-after-exhaustive`);
				const next = workspace(page).getByRole("button", {
					name: "Next step",
				});
				const visibleReadout = workspace(page).getByTestId(
					"flow-timeline-visible-readout",
				);
				for (let step = 0; step < Math.min(3, extent); step += 1) {
					await expect(next).toBeEnabled();
					const before = await visibleReadout.textContent();
					await next.click();
					await expect(visibleReadout).not.toHaveText(before ?? "");
					await assertEventPresentation(page, algorithmId, auditCase.label);
				}
				debugProgress(`${auditCase.label} sampled-steps`);
				await assertGraphGeometry(page, auditCase);
				await auditInteriorTimelineSamples(page, auditCase);
				debugProgress(`${auditCase.label} interior`);
				const lastEvent = workspace(page).getByRole("button", {
					name: "Last event",
				});
				if (await lastEvent.isEnabled()) {
					await lastEvent.click();
				}
				await expect(
					workspace(page).getByTestId("flow-timeline-readout"),
				).toHaveText(`Raw ${extent} / ${extent}`, { timeout: 60_000 });
				await expect(next).toBeDisabled();
				await expect(
					workspace(page)
						.getByLabel("Flow scene inspector")
						.locator("dt", { hasText: "Result" })
						.locator(".."),
				).not.toContainText("Not computed");
				const inspector = workspace(page).getByLabel("Flow scene inspector");
				await expect(
					inspector.locator("dt", { hasText: /^Primary work$/u }).locator(".."),
				).toContainText(
					`${auditCase.primary_work} / ${auditCase.primary_work}`,
				);
				await expect(
					inspector
						.locator("dt", { hasText: /^Detail progress$/u })
						.locator(".."),
				).toContainText(
					`${auditCase.detail_count} / ${auditCase.detail_count}`,
				);
				debugProgress(`${auditCase.label} complete`);
			}
		});
	}
});
