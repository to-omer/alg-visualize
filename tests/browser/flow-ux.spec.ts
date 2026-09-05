import { readFileSync } from "node:fs";
import type { Locator, Page } from "@playwright/test";
import { expect, test } from "./browser-test";
import { FLOW_BROWSER_ALGORITHM_IDS } from "./flow-browser-coverage";

type FlowProblem = "Max Flow" | "Min-Cost Flow";

type FlowGeneratorFaultMode = "error" | "hold" | "reject-create";

function hasDarwinPixelBaseline(testInfo: {
	project: Readonly<{ name: string }>;
}): boolean {
	return testInfo.project.name === "chromium" && process.platform === "darwin";
}

declare global {
	interface Window {
		__flowEnginePostCount?: number;
		__flowEngineWorker?: Worker;
		__flowGeneratorFaultMode?: FlowGeneratorFaultMode;
		__flowLastEngineGeneration?: number;
		__flowSeekReplacementTarget?: number;
		__flowTextEncoderConstructionCount?: number;
	}
}

function activeFlowWorkspace(page: Page): Locator {
	return page.locator("[data-workspace-id]:not([hidden])");
}

function inspectorOverview(inspector: Locator): Locator {
	return inspector
		.locator("[data-trace-catalog-id]")
		.locator("..")
		.locator("..");
}

function flowWorkspace(page: Page, problem: FlowProblem): Locator {
	return page.locator(
		`[data-workspace-id="${problem === "Max Flow" ? "max-flow" : "min-cost-flow"}"]`,
	);
}

function generatorFamily(dialog: Locator, familyId: string): Locator {
	return dialog.locator(`[data-family-id="${familyId}"]`);
}

async function selectGeneratorFamily(
	dialog: Locator,
	familyId: string,
): Promise<void> {
	if ((await dialog.locator(`[data-family-id="${familyId}"]`).count()) === 0) {
		await dialog.getByRole("button", { name: "Change shape" }).click();
	}
	const option = generatorFamily(dialog, familyId);
	await expect(option).toHaveAttribute("data-selection-state", "available");
	await option.click();
	await expect(option).toBeHidden();
}

async function openFlow(page: Page, problem: FlowProblem): Promise<void> {
	await page.goto("/");
	await page.getByRole("button", { name: problem, exact: true }).click();
	const workspace = flowWorkspace(page, problem);
	await expect(
		workspace.getByRole("heading", { name: problem, level: 1 }),
	).toBeVisible();
	await expect(
		workspace.getByRole("img", { name: "Validated flow network" }),
	).toBeVisible();
	await expect(workspace.getByText("Validated", { exact: true })).toHaveText(
		"Validated",
	);
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/^\{\n {2}"payload"/,
	);
}

async function revealFlowScenarioEditor(page: Page): Promise<Locator> {
	const workspace = activeFlowWorkspace(page);
	const editor = workspace.getByRole("textbox", {
		name: "Flow Scenario JSON",
	});
	if (!(await editor.isVisible())) {
		await workspace.getByRole("button", { name: "Input", exact: true }).click();
	}
	await expect(editor).toBeVisible();
	return editor;
}

async function closeFlowInputPanel(page: Page): Promise<void> {
	const close = activeFlowWorkspace(page).getByRole("button", {
		name: "Close input panel",
	});
	if (await close.isVisible()) await close.click();
}

async function selectFlowAlgorithm(page: Page, algorithmId: string) {
	const workspace = activeFlowWorkspace(page);
	await workspace
		.getByRole("button", { name: "Algorithm", exact: true })
		.click();
	const dialog = page.getByRole("dialog", { name: "Flow algorithms" });
	const row = dialog.locator(`[data-algorithm-id="${algorithmId}"]`);
	await expect(row).toHaveAttribute("data-selection-reason", "ready");
	const select = row.getByRole("button");
	if (await select.isDisabled()) {
		await expect(select).toHaveText("Current");
		await dialog.getByRole("button", { name: "Close" }).click();
	} else {
		await select.click();
	}
	await expect(dialog).toBeHidden();
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		new RegExp(`"id": "${algorithmId}"`),
	);
}

async function computeTrace(page: Page): Promise<void> {
	const workspace = activeFlowWorkspace(page);
	const runTrace = workspace.getByRole("button", { name: "Run trace" });
	const nextStep = workspace.getByRole("button", { name: "Next step" });
	await expect(nextStep).toBeEnabled();
	await nextStep.click();
	await expect(workspace.getByTestId("flow-timeline-readout")).toHaveText(
		/^Raw [1-9][0-9]* \/ [1-9][0-9]*$/,
		{ timeout: 60_000 },
	);
	await expect(runTrace).toBeVisible({ timeout: 60_000 });
	await expect(workspace.locator(".flow-status")).toHaveText("Validated");
	await page.evaluate(() => {
		if (document.activeElement instanceof HTMLElement) {
			document.activeElement.blur();
		}
	});
	// The outermost first/last controls are intentionally omitted on narrow
	// screens. From the first published boundary, one semantic back-step reaches
	// the immutable raw base in every viewport.
	await workspace.getByRole("button", { name: "Previous step" }).click();
	await expect(workspace.getByTestId("flow-timeline-readout")).toHaveText(
		/^Raw 0 \/ [1-9][0-9]*$/,
	);
}

async function selectMicroSteps(page: Page): Promise<void> {
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("micro");
	await expect(
		page.getByRole("slider", { name: "Raw trace position" }),
	).toBeVisible();
}

async function assertReadableNodeTraceCallouts(
	page: Page,
	expectedMaximum?: number,
): Promise<void> {
	const graph = activeFlowWorkspace(page).getByRole("img", {
		name: "Validated flow network",
	});
	const callouts = graph.locator(".flow-node-trace-callout:visible");
	const count = await callouts.count();
	expect(count).toBeGreaterThan(0);
	if (expectedMaximum !== undefined)
		expect(count).toBeLessThanOrEqual(expectedMaximum);
	const expectedOwnerIds = await graph
		.locator('[data-trace-callout-expected="true"]:visible')
		.evaluateAll((nodes) =>
			nodes
				.map((node) => node.getAttribute("data-node-id"))
				.filter((id): id is string => id !== null)
				.sort(),
		);
	const actualOwnerIds = await callouts.evaluateAll((items) =>
		items
			.map((item) => item.getAttribute("data-node-trace-for"))
			.filter((id): id is string => id !== null)
			.sort(),
	);
	if (expectedMaximum === 1) {
		expect(
			expectedOwnerIds,
			"the phone callout must belong to a source-selected Structure candidate",
		).toContain(actualOwnerIds[0]);
	} else if ((await graph.getAttribute("data-flow-lod")) === "structure") {
		expect(
			actualOwnerIds,
			"every source-selected Structure callout candidate must remain visible",
		).toEqual(expectedOwnerIds);
	}
	await expect(callouts.locator(".flow-node-trace-leader")).toHaveCount(count);
	const geometryErrors = await callouts.evaluateAll((items) => {
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
		return items.flatMap((item, index) => {
			const owner = item.closest<SVGGElement>("[data-node-id]");
			const ownerId = owner?.getAttribute("data-node-id");
			const declaredOwner = item.getAttribute("data-node-trace-for");
			const line = item.querySelector<SVGLineElement>(
				".flow-node-trace-leader",
			);
			const label = item.querySelector<SVGTextElement>(".flow-node-trace");
			if (owner === null || line === null || label === null)
				return ["incomplete"];
			const style = getComputedStyle(line);
			const x1 = line.x1.baseVal.value;
			const y1 = line.y1.baseVal.value;
			const x2 = line.x2.baseVal.value;
			const y2 = line.y2.baseVal.value;
			const labelX = label.x.baseVal[0]?.value ?? 0;
			const labelY = label.y.baseVal[0]?.value ?? 0;
			const labelBox = label.getBoundingClientRect();
			const otherLabelCollision = labels.some(
				(other, otherIndex) =>
					otherIndex !== index &&
					overlapArea(labelBox, other.getBoundingClientRect()) > 1,
			);
			const obstacleCollision = obstacles.some(
				(obstacle) =>
					overlapArea(labelBox, obstacle.getBoundingClientRect()) > 1,
			);
			const errors: string[] = [];
			if (ownerId === null || ownerId !== declaredOwner) errors.push("owner");
			if (
				style.stroke === "none" ||
				style.visibility === "hidden" ||
				style.display === "none" ||
				Number(style.strokeOpacity) === 0 ||
				Number(style.opacity) === 0
			)
				errors.push("leader-paint");
			if (Math.abs(Math.hypot(x1, y1) - 30) > 0.6) errors.push("leader-origin");
			if (Math.hypot(x2 - x1, y2 - y1) < 3) errors.push("leader-length");
			if (
				Math.hypot(x2 - labelX, y2 - labelY) >=
				Math.hypot(x1 - labelX, y1 - labelY)
			)
				errors.push("leader-direction");
			if (otherLabelCollision) errors.push("callout-collision");
			if (obstacleCollision) errors.push("graph-collision");
			return errors.map((error) => `${declaredOwner ?? "unknown"}:${error}`);
		});
	});
	expect(
		geometryErrors,
		"callout ownership, leader, and collision geometry",
	).toEqual([]);
	const graphBounds = await graph.boundingBox();
	if (graphBounds === null) throw new Error("Flow graph has no browser bounds");
	const boxes = await callouts
		.locator(".flow-node-trace")
		.evaluateAll((labels) =>
			labels.map((label) => {
				const bounds = label.getBoundingClientRect();
				return {
					left: bounds.left,
					right: bounds.right,
					top: bounds.top,
					bottom: bounds.bottom,
				};
			}),
		);
	for (const box of boxes) {
		expect(box.left).toBeGreaterThanOrEqual(graphBounds.x - 1);
		expect(box.right).toBeLessThanOrEqual(
			graphBounds.x + graphBounds.width + 1,
		);
		expect(box.top).toBeGreaterThanOrEqual(graphBounds.y - 1);
		expect(box.bottom).toBeLessThanOrEqual(
			graphBounds.y + graphBounds.height + 1,
		);
	}
	for (let left = 0; left < boxes.length; left += 1) {
		for (let right = left + 1; right < boxes.length; right += 1) {
			const leftBox = boxes[left];
			const rightBox = boxes[right];
			if (leftBox === undefined || rightBox === undefined) {
				throw new Error("Node callout browser fixture is incomplete");
			}
			const overlapWidth = Math.max(
				0,
				Math.min(leftBox.right, rightBox.right) -
					Math.max(leftBox.left, rightBox.left),
			);
			const overlapHeight = Math.max(
				0,
				Math.min(leftBox.bottom, rightBox.bottom) -
					Math.max(leftBox.top, rightBox.top),
			);
			expect(overlapWidth * overlapHeight).toBeLessThanOrEqual(1);
		}
	}
}

async function assertOrlinOriginalLabelsClearNodes(
	graph: Locator,
): Promise<void> {
	const labels = graph.locator(
		".flow-orlin-max-original-label[data-orlin-max-label-owner]:visible",
	);
	expect(await labels.count()).toBeLessThanOrEqual(1);
	const errors = await labels.evaluateAll((visibleLabels) => {
		const overlapArea = (left: DOMRect, right: DOMRect) =>
			Math.max(
				0,
				Math.min(left.right, right.right) - Math.max(left.left, right.left),
			) *
			Math.max(
				0,
				Math.min(left.bottom, right.bottom) - Math.max(left.top, right.top),
			);
		const nodes = Array.from(
			document.querySelectorAll<SVGCircleElement>(
				".flow-graph .flow-node-frame[data-node-id] > circle.flow-node",
			),
		).map((node) => node.getBoundingClientRect());
		return visibleLabels.flatMap((label) => {
			const owner = label.getAttribute("data-orlin-max-label-owner");
			const ownerGroup = label.closest<SVGGElement>(
				"[data-orlin-max-original]",
			);
			const text = label.querySelector<SVGTextElement>("text");
			const background = label.querySelector<SVGRectElement>("rect");
			const leader = label.querySelector<SVGLineElement>(
				".flow-edge-label-leader",
			);
			const path = ownerGroup?.querySelector<SVGPathElement>(":scope > path");
			if (
				owner === null ||
				ownerGroup?.getAttribute("data-orlin-max-original") !== owner ||
				text === null ||
				background === null ||
				leader === null ||
				path === undefined ||
				path === null
			)
				return ["missing-owner-or-geometry"];
			const textBox = text.getBoundingClientRect();
			const backgroundBox = background.getBoundingClientRect();
			const pathStyle = getComputedStyle(path);
			const labelErrors: string[] = [];
			const labelText = text.textContent?.trim() ?? "";
			if (
				labelText.length === 0 ||
				textBox.width < Math.max(4, labelText.length * 3) ||
				textBox.height < 5
			)
				labelErrors.push("unreadable-text");
			if (
				backgroundBox.left > textBox.left ||
				backgroundBox.right < textBox.right ||
				backgroundBox.top > textBox.top ||
				backgroundBox.bottom < textBox.bottom
			)
				labelErrors.push("background-does-not-enclose-text");
			if (nodes.some((node) => overlapArea(textBox, node) > 0))
				labelErrors.push("label-overlaps-node");
			if (
				path.getTotalLength() <= 0 ||
				pathStyle.stroke === "none" ||
				Number(pathStyle.strokeOpacity) === 0 ||
				!path.getAttribute("marker-end")?.startsWith("url(")
			)
				labelErrors.push("owner-path-is-not-painted-and-directed");
			return labelErrors;
		});
	});
	expect(errors).toEqual([]);
}

async function stepUntilCaption(
	page: Page,
	prefix: string,
	limit = 120,
): Promise<void> {
	const next = page.getByRole("button", { name: "Next step" });
	const readout = page.getByTestId("flow-timeline-readout");
	for (let index = 0; index < limit; index += 1) {
		const eventAction = page.locator(".flow-event-action");
		const caption =
			(await eventAction.count()) === 0
				? ""
				: ((await eventAction.textContent())?.trim() ?? "");
		if (caption.startsWith(prefix)) return;
		const before = (await readout.textContent()) ?? "";
		await expect(next).toBeEnabled();
		await next.click();
		await expect(readout).not.toHaveText(before);
	}
	throw new Error(`No trace event starts with ${JSON.stringify(prefix)}`);
}

async function stepUntilTraceCatalog(
	page: Page,
	catalogSuffix: string,
	limit = 160,
): Promise<void> {
	const workspace = activeFlowWorkspace(page);
	const next = workspace.getByRole("button", { name: "Next step" });
	const traceValue = workspace
		.getByLabel("Flow scene inspector")
		.locator("dd[data-trace-catalog-id]");
	const readout = workspace.getByTestId("flow-timeline-readout");
	for (let index = 0; index < limit; index += 1) {
		const catalogId =
			(await traceValue.count()) === 0
				? null
				: await traceValue.getAttribute("data-trace-catalog-id");
		if (catalogId?.endsWith(catalogSuffix) === true) return;
		const before = (await readout.textContent()) ?? "";
		await expect(next).toBeEnabled();
		await next.click();
		await expect(readout).not.toHaveText(before);
	}
	throw new Error(`No trace event contains ${JSON.stringify(catalogSuffix)}`);
}

async function stepBackwardUntilTraceCatalog(
	page: Page,
	catalogSuffix: string,
	limit = 32,
): Promise<void> {
	const workspace = activeFlowWorkspace(page);
	const previous = workspace.getByRole("button", { name: "Previous step" });
	const traceValue = workspace
		.getByLabel("Flow scene inspector")
		.locator("dd[data-trace-catalog-id]");
	const readout = workspace.getByTestId("flow-timeline-readout");
	for (let index = 0; index < limit; index += 1) {
		const catalogId =
			(await traceValue.count()) === 0
				? null
				: await traceValue.getAttribute("data-trace-catalog-id");
		if (catalogId?.endsWith(catalogSuffix) === true) return;
		const before = (await readout.textContent()) ?? "";
		await expect(previous).toBeEnabled();
		await previous.click();
		await expect(readout).not.toHaveText(before);
	}
	throw new Error(
		`No reverse trace event contains ${JSON.stringify(catalogSuffix)}`,
	);
}

async function stepBackwardUntilVisible(
	page: Page,
	target: Locator,
	limit = 16,
): Promise<void> {
	const workspace = activeFlowWorkspace(page);
	const previous = workspace.getByRole("button", { name: "Previous step" });
	const readout = workspace.getByTestId("flow-timeline-readout");
	for (let index = 0; index < limit; index += 1) {
		if (await target.isVisible()) return;
		const before = (await readout.textContent()) ?? "";
		await expect(previous).toBeEnabled();
		await previous.click();
		await expect(readout).not.toHaveText(before);
	}
	throw new Error("Target did not become visible while stepping backward");
}

async function stepUntilBoundary(
	page: Page,
	boundary: "Detail" | "Operation" | "Phase",
	limit = 160,
): Promise<void> {
	const workspace = activeFlowWorkspace(page);
	const next = workspace.getByRole("button", { name: "Next step" });
	const readout = workspace.getByTestId("flow-timeline-readout");
	const boundaryRow = workspace
		.getByLabel("Flow scene inspector")
		.locator("dt", { hasText: /^Boundary$/u })
		.locator("..");
	for (let index = 0; index < limit; index += 1) {
		const before = (await readout.textContent()) ?? "";
		await expect(next).toBeEnabled();
		await next.click();
		await expect(readout).not.toHaveText(before);
		await expect(boundaryRow).toBeVisible();
		if (((await boundaryRow.textContent()) ?? "").includes(boundary)) return;
	}
	throw new Error(`No ${boundary} boundary appeared within ${limit} steps`);
}

async function selectNavigatorResultByKeyboard(
	page: Page,
	kind: "node" | "edge" | "residual-arc" | "aggregate",
	query: string,
): Promise<Locator> {
	const navigator = page.getByRole("region", { name: "Entity navigator" });
	await navigator
		.getByRole("combobox", { name: "Type", exact: true })
		.selectOption(kind);
	await navigator.getByRole("searchbox").fill(query);
	const result = navigator.locator(".flow-entity-result").first();
	await expect(result).toBeVisible();
	await result.focus();
	await page.keyboard.press("Enter");
	await expect(result).toHaveAttribute("aria-pressed", "true");
	return result;
}

async function collectCaptions(page: Page, limit = 120): Promise<string[]> {
	const captions: string[] = [];
	const next = page.getByRole("button", { name: "Next step" });
	const readout = page.getByTestId("flow-timeline-readout");
	for (let index = 0; index < limit; index += 1) {
		const before = (await readout.textContent()) ?? "";
		const match = /^Raw (\d+) \/ (\d+)$/.exec(before);
		if (match === null || match[1] === match[2]) break;
		await expect(next).toBeEnabled();
		await next.click();
		await expect(readout).not.toHaveText(before);
		const caption = await page.locator(".flow-event-action").textContent();
		if (caption !== null) captions.push(caption.trim());
	}
	return captions;
}

async function generateReadableDefault(page: Page): Promise<void> {
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	const dialog = page.getByRole("dialog", { name: /Generate .* graph/ });
	await expect(dialog).toBeVisible();
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });
	await expect(page.locator(".flow-generation-details")).toContainText(
		/Generated [1-9][0-9]* nodes · [1-9][0-9]* edges/,
	);
}

type StableGraphSnapshot = Readonly<{
	edges: Record<string, string>;
	nodes: Record<string, string>;
}>;

type CanonicalCapacityStyle = Readonly<{
	filter: string;
	linecap: string;
	opacity: string;
	stroke: string;
	strokeDasharray: string;
	strokeWidth: string;
}>;

async function canonicalCapacityStyle(
	edge: Locator,
): Promise<CanonicalCapacityStyle> {
	const rail = edge.locator(
		':scope > .flow-capacity-rail[data-flow-channel="capacity"]',
	);
	await expect(rail).toHaveCount(1);
	return rail.evaluate((element) => {
		const style = getComputedStyle(element);
		return {
			filter: style.filter,
			linecap: style.strokeLinecap,
			opacity: style.opacity,
			stroke: style.stroke,
			strokeDasharray: style.strokeDasharray,
			strokeWidth: style.strokeWidth,
		};
	});
}

async function canonicalDataChannelStyles(
	edge: Locator,
): Promise<Record<string, CanonicalCapacityStyle>> {
	return edge.evaluate((element) =>
		Object.fromEntries(
			[
				...element.querySelectorAll<SVGPathElement>(
					":scope > path[data-flow-channel]",
				),
			].map((path) => {
				const channel = path.dataset.flowChannel;
				if (channel === undefined) throw new Error("Flow channel is missing");
				const style = getComputedStyle(path);
				return [
					channel,
					{
						filter: style.filter,
						linecap: style.strokeLinecap,
						opacity: style.opacity,
						stroke: style.stroke,
						strokeDasharray: style.strokeDasharray,
						strokeWidth: style.strokeWidth,
					},
				] as const;
			}),
		),
	);
}

async function stableGraphSnapshot(
	graph: Locator,
): Promise<StableGraphSnapshot> {
	return graph.evaluate((element) => {
		const styleProjection = (target: Element | null) => {
			if (!(target instanceof SVGElement)) return "missing";
			const style = getComputedStyle(target);
			return [
				target.getAttribute("class") ?? "",
				target.getAttribute("d") ?? "",
				target.getAttribute("transform") ?? "",
				target.getAttribute("stroke-width") ?? "",
				style.stroke,
				style.strokeWidth,
				style.strokeDasharray,
				style.fill,
				style.opacity,
				style.filter,
			].join("|");
		};
		const project = (
			selector: string,
			idAttribute: "data-edge-id" | "data-node-id",
			children: readonly string[],
		) =>
			Object.fromEntries(
				[...element.querySelectorAll<SVGGElement>(selector)]
					.map((group) => {
						const id = group.getAttribute(idAttribute);
						if (id === null) throw new Error(`${idAttribute} is missing`);
						return [
							id,
							[
								group.getAttribute("class") ?? "",
								group.getAttribute("transform") ?? "",
								...children.map((child) =>
									styleProjection(group.querySelector(child)),
								),
								...[...group.querySelectorAll<SVGElement>("*")].map((child) => {
									const attributes = [...child.attributes]
										.filter(
											(attribute) =>
												attribute.name !== "data-event-touch" &&
												attribute.name !== "data-event-change",
										)
										.map((attribute) => `${attribute.name}=${attribute.value}`)
										.sort()
										.join(";");
									return `${child.tagName}|${attributes}|${child.textContent ?? ""}|${styleProjection(child)}`;
								}),
							].join("||"),
						] as const;
					})
					.sort(([left], [right]) => left.localeCompare(right)),
			);
		return {
			edges: project(".flow-original-edge", "data-edge-id", [
				":scope > .flow-cost-rail",
				":scope > .flow-capacity-rail",
				":scope > .flow-flow-line",
				":scope > .flow-edge-hit-target",
			]),
			nodes: project("[data-node-id]", "data-node-id", [
				":scope > .flow-node",
				":scope > .flow-node-label",
			]),
		};
	});
}

async function retainedFlowState(page: Page): Promise<{
	cursor: string;
	graph: StableGraphSnapshot;
	scenario: string;
	selection: string[];
}> {
	const workspace = activeFlowWorkspace(page);
	const graph = workspace.getByRole("img", { name: "Validated flow network" });
	return {
		cursor:
			(await workspace.getByTestId("flow-timeline-readout").textContent()) ??
			"",
		graph: await stableGraphSnapshot(graph),
		scenario: await workspace
			.getByRole("textbox", { name: "Flow Scenario JSON" })
			.inputValue(),
		selection: await graph
			.locator(".flow-entity-selected")
			.evaluateAll((items) =>
				items
					.map((item) =>
						[
							item.getAttribute("data-node-id") ?? "",
							item.getAttribute("data-edge-id") ?? "",
							item.getAttribute("data-residual-direction") ?? "",
						].join(":"),
					)
					.filter((identity) => identity !== "::")
					.sort(),
			),
	};
}

function expectUntouchedEntitiesStable(
	before: StableGraphSnapshot,
	after: StableGraphSnapshot,
	touchedEdges: ReadonlySet<string>,
	touchedNodes: ReadonlySet<string>,
): void {
	for (const [id, state] of Object.entries(before.edges)) {
		if (!touchedEdges.has(id))
			expect(after.edges[id], `edge ${id}`).toBe(state);
	}
	for (const [id, state] of Object.entries(before.nodes)) {
		if (!touchedNodes.has(id))
			expect(after.nodes[id], `node ${id}`).toBe(state);
	}
}

async function eventTouchProjection(graph: Locator): Promise<{
	edges: string[];
	nodes: string[];
}> {
	return {
		edges: await graph
			.locator('.flow-original-edge[data-event-touch="true"]')
			.evaluateAll((items) =>
				items.flatMap((item) => item.getAttribute("data-edge-id") ?? []).sort(),
			),
		nodes: await graph
			.locator('[data-node-id][data-event-touch="true"]')
			.evaluateAll((items) =>
				items.flatMap((item) => item.getAttribute("data-node-id") ?? []).sort(),
			),
	};
}

async function eventChangeProjection(graph: Locator): Promise<{
	edges: string[];
	nodes: string[];
}> {
	return {
		edges: await graph
			.locator('.flow-original-edge[data-event-change="true"]')
			.evaluateAll((items) =>
				items.flatMap((item) => item.getAttribute("data-edge-id") ?? []).sort(),
			),
		nodes: await graph
			.locator('[data-node-id][data-event-change="true"]')
			.evaluateAll((items) =>
				items.flatMap((item) => item.getAttribute("data-node-id") ?? []).sort(),
			),
	};
}

test("Max Flow and Min-Cost Flow are separate English workspaces", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	const maxWorkspace = flowWorkspace(page, "Max Flow");
	const maxEditor = maxWorkspace.getByRole("textbox", {
		name: "Flow Scenario JSON",
	});
	const maxScenario = await maxEditor.inputValue();
	expect(maxScenario).toContain('"kind": "max-flow"');
	const editedMaxScenario = `${maxScenario}\n`;
	await maxEditor.fill(editedMaxScenario);
	await expect(
		maxWorkspace.getByRole("group", { name: "Visual encoding key" }),
	).not.toContainText("Cost");

	await page
		.getByRole("button", { name: "Min-Cost Flow", exact: true })
		.click();
	await expect(
		page.getByRole("heading", { name: "Min-Cost Flow", level: 1 }),
	).toBeVisible();
	const minWorkspace = flowWorkspace(page, "Min-Cost Flow");
	const minEditor = minWorkspace.getByRole("textbox", {
		name: "Flow Scenario JSON",
	});
	await expect(minEditor).toHaveValue(/^\{\n {2}"payload"/);
	const minScenario = await minEditor.inputValue();
	expect(minScenario).not.toBe(maxScenario);
	const editedMinScenario = `${minScenario}\n`;
	await minEditor.fill(editedMinScenario);
	await expect(
		minWorkspace.getByRole("group", { name: "Visual encoding key" }),
	).toContainText("Cost");

	await page.getByRole("button", { name: "Max Flow", exact: true }).click();
	await expect(
		maxWorkspace.getByRole("textbox", { name: "Flow Scenario JSON" }),
	).toHaveValue(editedMaxScenario);
	await page
		.getByRole("button", { name: "Min-Cost Flow", exact: true })
		.click();
	await expect(
		minWorkspace.getByRole("textbox", { name: "Flow Scenario JSON" }),
	).toHaveValue(editedMinScenario);
});

test("the public 93-algorithm catalog is closed and accessible", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	await page.getByRole("button", { name: "Algorithm", exact: true }).click();
	const dialog = page.getByRole("dialog", { name: "Flow algorithms" });
	await expect(dialog).toBeVisible();
	await dialog.getByLabel("Compatibility").selectOption("all");
	const rows = await dialog
		.locator("[data-algorithm-id]")
		.evaluateAll((items) =>
			items.map((item) => {
				const titleId = item.getAttribute("aria-labelledby");
				const title =
					titleId === null ? null : document.getElementById(titleId);
				const button = item.querySelector("button");
				return {
					id: item.getAttribute("data-algorithm-id"),
					title: title?.textContent?.trim() ?? "",
					buttonText: button?.textContent?.trim() ?? "",
				};
			}),
		);
	expect(rows.map((row) => row.id).sort()).toEqual(
		[...FLOW_BROWSER_ALGORITHM_IDS].sort(),
	);
	expect(rows).toHaveLength(93);
	for (const row of rows) {
		expect(row.title, `${row.id} title`).not.toBe("");
		expect(row.buttonText, `${row.id} action`).not.toBe("");
	}
});

test("all 93 endpoints publish enabled Detail playback and preserve the user preference", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	const workspace = activeFlowWorkspace(page);
	const playback = workspace.getByRole("combobox", {
		name: "Playback granularity",
	});
	await expect(playback.locator('option[value="micro"]')).toBeEnabled();
	await playback.selectOption("micro");

	await workspace
		.getByRole("button", { name: "Algorithm", exact: true })
		.click();
	const dialog = page.getByRole("dialog", { name: "Flow algorithms" });
	await dialog.getByLabel("Compatibility").selectOption("all");
	const stepContracts = dialog.locator(".flow-algorithm-step-contract");
	await expect(stepContracts).toHaveCount(93);
	const implementationDetails = dialog.locator(".flow-algorithm-metadata");
	await expect(implementationDetails).toHaveCount(93);
	expect(
		await implementationDetails.evaluateAll((details) =>
			details.every((detail) => !detail.hasAttribute("open")),
		),
	).toBe(true);
	await expect(
		implementationDetails.locator("summary", {
			hasText: "Implementation details",
		}),
	).toHaveCount(93);
	const firstImplementationDetails = implementationDetails.first();
	const implementationSummary = firstImplementationDetails.getByText(
		"Implementation details",
		{ exact: true },
	);
	await implementationSummary.click();
	await expect(firstImplementationDetails).toHaveAttribute("open", "");
	await expect(firstImplementationDetails).toContainText(
		"Complexity / implementation claim",
	);
	await implementationSummary.click();
	const stepSummaries = dialog.locator(".flow-algorithm-step-status");
	await expect(stepSummaries).toHaveCount(93);
	expect(
		await stepSummaries.evaluateAll((summaries) =>
			summaries.every((summary) =>
				(summary.getAttribute("aria-label") ?? "").startsWith(
					"Step support: Detail",
				),
			),
		),
	).toBe(true);
	await expect(
		stepContracts.getByText("No detail", { exact: true }),
	).toHaveCount(0);
	const detailDefinitions = await stepContracts.evaluateAll((contracts) =>
		contracts.map((contract) => {
			const detail = [...contract.querySelectorAll("small")].find(
				(item) =>
					item.querySelector("strong")?.textContent?.trim() === "Detail",
			);
			return detail?.textContent?.trim() ?? "";
		}),
	);
	expect(detailDefinitions).toHaveLength(93);
	for (const detail of detailDefinitions) {
		expect(detail).toMatch(/^Detail .+/u);
		expect(detail).not.toContain("Unavailable");
	}
	const incompatibleRow = dialog.locator(
		'[data-algorithm-id="successive-shortest-path"]',
	);
	await expect(incompatibleRow).not.toHaveAttribute("aria-disabled");
	await expect(
		incompatibleRow.locator(".flow-algorithm-select"),
	).toBeDisabled();
	const incompatibleDisclosure = incompatibleRow.locator(
		".flow-algorithm-step-contract",
	);
	await incompatibleDisclosure.locator("summary").focus();
	await page.keyboard.press("Enter");
	await expect(incompatibleDisclosure).toHaveAttribute("open", "");

	const goldbergRao = dialog.locator('[data-algorithm-id="goldberg-rao"]');
	await expect(goldbergRao).toHaveAttribute("data-selection-reason", "ready");
	const goldbergStepContract = goldbergRao.locator(
		".flow-algorithm-step-contract",
	);
	await goldbergStepContract.locator("summary").click();
	await expect(goldbergStepContract).toHaveAttribute("open", "");
	await expect(goldbergStepContract).toContainText(
		"one positive residual arc inspected while building binary lengths and reverse 0–1 distances",
	);
	await goldbergRao.getByRole("button").click();
	await expect(dialog).toBeHidden();
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/"id": "goldberg-rao"/,
	);
	await expect(playback.locator('option[value="micro"]')).toBeEnabled();
	await expect(playback.locator('option[value="micro"]')).toHaveText("Detail");
	await expect(playback).toHaveValue("micro");
	await computeTrace(page);
	await selectMicroSteps(page);
	await stepUntilBoundary(page, "Detail");
	await expect(
		workspace
			.getByLabel("Flow scene inspector")
			.locator("dt", { hasText: "Boundary" })
			.locator(".."),
	).toContainText("Detail");
	await expect(
		workspace.getByTestId("flow-timeline-visible-readout"),
	).toHaveText(/^Event [1-9][0-9]*\/[1-9][0-9]*$/);
	await expect(workspace.getByTestId("flow-timeline-work-readout")).toHaveText(
		/^Detail [1-9][0-9]*\/[1-9][0-9]* · (?:Primitive|Iteration|Oracle call) (?:0|[1-9][0-9]*)\/[1-9][0-9]*$/,
	);
	await expect(playback).toHaveAccessibleName(
		/Detail visits every trace event, including Operation and Phase boundaries/,
	);

	await workspace
		.getByRole("button", { name: "Algorithm", exact: true })
		.click();
	const restoredDialog = page.getByRole("dialog", { name: "Flow algorithms" });
	const currentRow = restoredDialog.locator(
		'[data-algorithm-id="goldberg-rao"]',
	);
	await expect(currentRow.locator(".flow-algorithm-current")).toHaveText(
		"Current",
	);
	await expect(currentRow).not.toHaveAttribute("aria-disabled");
	await expect(currentRow.locator(".flow-algorithm-select")).toBeDisabled();
	const currentDisclosure = currentRow.locator(".flow-algorithm-step-contract");
	await currentDisclosure.locator("summary").focus();
	await page.keyboard.press("Enter");
	await expect(currentDisclosure).toHaveAttribute("open", "");
	await restoredDialog
		.locator('[data-algorithm-id="edmonds-karp"]')
		.getByRole("button")
		.click();
	await expect(restoredDialog).toBeHidden();
	await expect(playback).toHaveValue("micro");
});

test("Phase and Operation controls disclose real availability", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	let workspace = activeFlowWorkspace(page);
	let playback = workspace.getByRole("combobox", {
		name: "Playback granularity",
	});
	await selectFlowAlgorithm(page, "binary-blocking-flow");
	await expect(playback.locator('option[value="phase"]')).toHaveAttribute(
		"disabled",
		"",
	);
	await expect(playback.locator('option[value="phase"]')).toHaveAttribute(
		"aria-disabled",
		"true",
	);
	await expect(playback.locator('option[value="phase"]')).toHaveText(
		"Phase — unavailable",
	);
	await expect(playback).toHaveAccessibleName(
		/Phase playback unavailable: Primary work owns the former phase event/,
	);
	await selectFlowAlgorithm(page, "warm-start-push-relabel");
	await expect(playback.locator('option[value="operation"]')).toHaveAttribute(
		"disabled",
		"",
	);
	await expect(playback.locator('option[value="operation"]')).toHaveAttribute(
		"aria-disabled",
		"true",
	);
	await expect(playback.locator('option[value="operation"]')).toHaveText(
		"Operation — unavailable",
	);
	await expect(playback).toHaveAccessibleName(
		/Operation playback unavailable: This trace publishes Phase boundaries/,
	);

	await selectFlowAlgorithm(page, "edmonds-karp");
	await playback.selectOption("phase");
	await expect(playback).toHaveValue("phase");
	await expect(playback.locator('option[value="phase"]')).toBeEnabled();
	await expect(playback.locator('option[value="phase"]')).toHaveText("Phase");
	await computeTrace(page);
	await stepUntilBoundary(page, "Phase");

	await openFlow(page, "Min-Cost Flow");
	workspace = activeFlowWorkspace(page);
	playback = workspace.getByRole("combobox", {
		name: "Playback granularity",
	});
	await selectFlowAlgorithm(page, "blocking-flow-primal-dual");
	await playback.selectOption("operation");
	await expect(playback).toHaveValue("operation");
	await expect(playback.locator('option[value="operation"]')).toBeEnabled();
	await expect(playback.locator('option[value="operation"]')).toHaveText(
		"Operation",
	);
	await computeTrace(page);
	await stepUntilBoundary(page, "Operation");
});

test("augmenting-path Detail playback reveals a selected path one edge at a time", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	for (const [algorithmId, catalogSuffix] of [
		["ford-fulkerson", ".extend-path-prefix"],
		["dinic", ".extend-level-path-prefix"],
	] as const) {
		await selectFlowAlgorithm(page, algorithmId);
		await computeTrace(page);
		await selectMicroSteps(page);
		await stepUntilTraceCatalog(page, catalogSuffix);
		const graph = activeFlowWorkspace(page).getByRole("img", {
			name: "Validated flow network",
		});
		await expect(
			graph.locator(".flow-original-edge.flow-edge-active"),
		).toHaveCount(1);
		await activeFlowWorkspace(page)
			.getByRole("button", { name: "Next step" })
			.click();
		await stepUntilTraceCatalog(page, catalogSuffix);
		await expect(
			graph.locator(".flow-original-edge.flow-edge-active"),
		).toHaveCount(2);
		await expect(
			activeFlowWorkspace(page).locator(".flow-event-action"),
		).toContainText(/Extend|prefix/i);
	}
});

test("rich numerical state stays readable and locally scrollable on phones", async ({
	page,
}) => {
	test.slow();
	test.setTimeout(300_000);
	await page.setViewportSize({ width: 1440, height: 960 });
	await openFlow(page, "Min-Cost Flow");
	const manifest = JSON.parse(
		readFileSync(
			new URL("../../fixtures/flow-representative-audit.json", import.meta.url),
			"utf8",
		),
	) as Readonly<{
		cases: readonly Readonly<{
			algorithm_id: string;
			label: string;
			middle_detail: Readonly<{ event: number }>;
			scenario: unknown;
		}>[];
	}>;
	for (const [algorithmId, panelTestId, labelSelector, graphSelector] of [
		[
			"primal-dual-interior-point-mcf",
			"flow-ipm-mcf-panel",
			".flow-ipm-arc-label text, .flow-ipm-node-detail",
			".flow-ipm-mcf-graph-wrap",
		],
		[
			"electrical-flow-interior-point-mcf",
			"flow-electrical-ipm-mcf-panel",
			".flow-eipm-edge-label text, .flow-eipm-node-detail",
			".flow-eipm-graph-wrap",
		],
		[
			"minimum-ratio-cycle-mcf",
			"flow-minimum-ratio-cycle-mcf-panel",
			".flow-mrcmcf-edge-label text, .flow-mrcmcf-node-detail",
			".flow-mrcmcf-graph-wrap",
		],
	] as const) {
		await page.setViewportSize({ width: 1440, height: 960 });
		const representative = manifest.cases.find(
			(candidate) =>
				candidate.algorithm_id === algorithmId &&
				candidate.label ===
					(algorithmId === "primal-dual-interior-point-mcf"
						? "ipm-path-2"
						: "canonical"),
		);
		if (representative === undefined) {
			throw new Error(
				`${algorithmId} has no required numerical representative`,
			);
		}
		const editor = await revealFlowScenarioEditor(page);
		await editor.fill(JSON.stringify(representative.scenario, null, 2));
		await activeFlowWorkspace(page)
			.getByRole("button", { name: "Load", exact: true })
			.click();
		await expect(activeFlowWorkspace(page).locator(".flow-status")).toHaveText(
			"Validated",
		);
		await computeTrace(page);
		const workspace = activeFlowWorkspace(page);
		await selectMicroSteps(page);
		await workspace
			.locator(".flow-timeline input[type='range']")
			.fill(String(representative.middle_detail.event));
		const desktopDisclosureSummary = workspace
			.locator(".flow-rich-status-disclosure")
			.getByLabel("Show algorithm state details");
		const desktopZoomControls = workspace.locator(".flow-canvas-controls");
		await expect(desktopDisclosureSummary).toBeVisible();
		await expect(desktopZoomControls).toBeVisible();
		const [summaryBounds, zoomBounds] = await Promise.all([
			desktopDisclosureSummary.boundingBox(),
			desktopZoomControls.boundingBox(),
		]);
		if (summaryBounds === null || zoomBounds === null)
			throw new Error(`${algorithmId} desktop controls have no layout bounds`);
		const controlsOverlap = !(
			summaryBounds.x + summaryBounds.width <= zoomBounds.x ||
			summaryBounds.x >= zoomBounds.x + zoomBounds.width ||
			summaryBounds.y + summaryBounds.height <= zoomBounds.y ||
			summaryBounds.y >= zoomBounds.y + zoomBounds.height
		);
		expect(
			controlsOverlap,
			`${algorithmId} algorithm-state disclosure must not cover canvas zoom`,
		).toBe(false);
		await page.setViewportSize({ width: 390, height: 844 });
		const disclosure = workspace.locator(".flow-rich-status-disclosure");
		const summary = disclosure.getByLabel("Show algorithm state details");
		await summary.click();
		const panel = workspace.getByTestId(panelTestId);
		await expect(panel).toBeVisible();
		const readability = await panel.evaluate(
			(element, selectors) => {
				const labels = [
					...element.querySelectorAll<SVGTextElement>(selectors.label),
				].filter((label) => label.getBoundingClientRect().height > 0);
				const graph = element.querySelector<HTMLElement>(selectors.graph);
				return {
					labelCount: labels.length,
					minimumLabelHeight: Math.min(
						...labels.map((label) => label.getBoundingClientRect().height),
					),
					exactItems: element.querySelectorAll(".visually-hidden li").length,
					graphClientWidth: graph?.clientWidth ?? 0,
					graphScrollWidth: graph?.scrollWidth ?? 0,
				};
			},
			{ label: labelSelector, graph: graphSelector },
		);
		expect(
			readability.labelCount,
			`${algorithmId} visible exact labels`,
		).toBeGreaterThan(0);
		expect(
			readability.minimumLabelHeight,
			`${algorithmId} minimum rendered exact-label height`,
		).toBeGreaterThanOrEqual(10);
		expect(
			readability.exactItems,
			`${algorithmId} exact accessible items`,
		).toBeGreaterThan(0);
		expect(readability.graphClientWidth).toBeGreaterThan(0);
		expect(readability.graphScrollWidth).toBeGreaterThan(
			readability.graphClientWidth,
		);
		expect(
			await page.evaluate(
				() =>
					document.documentElement.scrollWidth -
					document.documentElement.clientWidth,
			),
		).toBeLessThanOrEqual(1);
		if (algorithmId === "primal-dual-interior-point-mcf") {
			const arcs = panel.locator(".flow-ipm-arc");
			const arcCount = await arcs.count();
			expect(arcCount).toBeGreaterThanOrEqual(2);
			const onDemandArcs = panel.locator(
				".flow-ipm-arc:has(> .flow-ipm-arc-label:not(.flow-ipm-arc-label-visible))",
			);
			const onDemandCount = await onDemandArcs.count();
			const focusArcs: Locator[] = [];
			const onDemandArc = onDemandArcs.first();
			if (onDemandCount >= 2) {
				await onDemandArc.focus();
				const focusedLabel = onDemandArc.locator(".flow-ipm-arc-label");
				await expect(focusedLabel).toBeVisible();
				const focusedReadability = await panel.evaluate((element) => {
					const focused = element.querySelector<SVGGElement>(
						".flow-ipm-arc:focus > .flow-ipm-arc-label",
					);
					if (focused === null) return undefined;
					const focusedBounds = focused.getBoundingClientRect();
					const visibleBounds = [
						...element.querySelectorAll<SVGGElement>(
							".flow-ipm-arc-label-visible",
						),
					].map((label) => label.getBoundingClientRect());
					return {
						height: focusedBounds.height,
						insideViewport:
							focusedBounds.left >=
								(element
									.querySelector(".flow-ipm-mcf-graph-wrap")
									?.getBoundingClientRect().left ?? Number.POSITIVE_INFINITY) &&
							focusedBounds.right <=
								(element
									.querySelector(".flow-ipm-mcf-graph-wrap")
									?.getBoundingClientRect().right ?? Number.NEGATIVE_INFINITY),
						collides: visibleBounds.some(
							(bounds) =>
								focusedBounds.right + 1 > bounds.left &&
								bounds.right + 1 > focusedBounds.left &&
								focusedBounds.bottom + 1 > bounds.top &&
								bounds.bottom + 1 > focusedBounds.top,
						),
					};
				});
				expect(focusedReadability).toBeDefined();
				expect(focusedReadability?.height).toBeGreaterThanOrEqual(10);
				expect(focusedReadability?.insideViewport).toBe(true);
				expect(focusedReadability?.collides).toBe(false);
				const secondOnDemandArc = panel
					.locator(
						".flow-ipm-arc:has(> .flow-ipm-arc-label:not(.flow-ipm-arc-label-visible))",
					)
					.nth(1);
				await expect(secondOnDemandArc).toHaveCount(1);
				const secondOnDemandPath = secondOnDemandArc.locator(
					":scope > path:not(.flow-ipm-tree-rail):not(.flow-ipm-cycle-rail):not(.flow-ipm-focus-rail)",
				);
				const secondOnDemandArcId =
					await secondOnDemandArc.getAttribute("data-ipm-arc");
				expect(secondOnDemandArcId).not.toBeNull();
				await secondOnDemandPath.evaluate((path) => {
					const graph = path.closest(".flow-ipm-mcf-graph-wrap");
					if (!(graph instanceof HTMLElement)) {
						throw new Error("IPM graph viewport is missing");
					}
					const pathBounds = path.getBoundingClientRect();
					const graphBounds = graph.getBoundingClientRect();
					graph.scrollLeft +=
						pathBounds.left +
						pathBounds.width / 2 -
						(graphBounds.left + graphBounds.width / 2);
				});
				await expect(secondOnDemandPath).toBeInViewport();
				const hoverPoint = await secondOnDemandPath.evaluate((path) => {
					if (!(path instanceof SVGPathElement)) {
						throw new Error("IPM arc path is not an SVG path");
					}
					const point = path.getPointAtLength(path.getTotalLength() / 2);
					const matrix = path.getScreenCTM();
					if (matrix === null) {
						throw new Error("IPM arc has no screen transform");
					}
					return {
						x: matrix.a * point.x + matrix.c * point.y + matrix.e,
						y: matrix.b * point.x + matrix.d * point.y + matrix.f,
					};
				});
				await page.mouse.move(hoverPoint.x, hoverPoint.y);
				await expect(panel.locator(".flow-ipm-arc-hovered")).toHaveAttribute(
					"data-ipm-arc",
					secondOnDemandArcId ?? "",
				);
				await expect(
					secondOnDemandArc.locator(".flow-ipm-arc-label"),
				).toBeVisible();
				await expect(focusedLabel).toBeHidden();
				expect(
					await panel.evaluate(
						(element) =>
							[
								...element.querySelectorAll<SVGGElement>(
									".flow-ipm-arc-label:not(.flow-ipm-arc-label-visible)",
								),
							].filter((label) => label.getBoundingClientRect().height > 0)
								.length,
					),
				).toBe(1);
				focusArcs.push(arcs.first(), arcs.nth(1), onDemandArc);
			} else {
				// Small representatives can place every exact label without a
				// collision. Hiding one in that case would reduce useful detail.
				expect(onDemandCount).toBe(0);
				const allFit = await panel.evaluate((element) => {
					const labels = [
						...element.querySelectorAll<SVGGElement>(
							".flow-ipm-arc-label-visible",
						),
					];
					const bounds = labels.map((label) => label.getBoundingClientRect());
					let collisions = 0;
					for (let left = 0; left < bounds.length; left += 1) {
						for (let right = left + 1; right < bounds.length; right += 1) {
							const a = bounds[left];
							const b = bounds[right];
							if (
								a !== undefined &&
								b !== undefined &&
								a.right + 1 > b.left &&
								b.right + 1 > a.left &&
								a.bottom + 1 > b.top &&
								b.bottom + 1 > a.top
							) {
								collisions += 1;
							}
						}
					}
					return {
						arcCount: element.querySelectorAll(".flow-ipm-arc").length,
						labelCount: labels.length,
						collisions,
					};
				});
				expect(allFit.labelCount).toBe(allFit.arcCount);
				expect(allFit.collisions).toBe(0);
				for (const index of new Set([0, 1, arcCount - 1])) {
					focusArcs.push(arcs.nth(index));
				}
			}
			await page.emulateMedia({ colorScheme: "light" });
			const lightThemeContrast = await panel.evaluate((element) => {
				const parseRgb = (value: string): readonly [number, number, number] => {
					const channels = value.match(/[0-9.]+/gu)?.map(Number) ?? [];
					return [channels[0] ?? 0, channels[1] ?? 0, channels[2] ?? 0];
				};
				const luminance = (color: readonly [number, number, number]) => {
					const linearized = color.map((channel) => {
						const normalized = channel / 255;
						return normalized <= 0.04045
							? normalized / 12.92
							: ((normalized + 0.055) / 1.055) ** 2.4;
					});
					const red = linearized[0] ?? 0;
					const green = linearized[1] ?? 0;
					const blue = linearized[2] ?? 0;
					return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
				};
				const white = luminance([255, 255, 255]);
				return [
					...element.querySelectorAll<SVGTextElement>(
						".flow-ipm-arc-label-visible text, .flow-ipm-node-detail",
					),
				].map((label) => {
					const style = getComputedStyle(label);
					const foreground = luminance(parseRgb(style.fill));
					return {
						contrast: (white + 0.05) / (foreground + 0.05),
						paintOrder: style.paintOrder,
						stroke: style.stroke,
					};
				});
			});
			expect(lightThemeContrast.length).toBeGreaterThan(0);
			expect(
				Math.min(...lightThemeContrast.map(({ contrast }) => contrast)),
			).toBeGreaterThanOrEqual(4.5);
			expect(
				lightThemeContrast.every(
					({ paintOrder, stroke }) =>
						stroke === "none" || paintOrder.startsWith("stroke"),
				),
			).toBe(true);
			await page.emulateMedia({ colorScheme: "dark", forcedColors: "active" });
			await page.mouse.move(0, 0);
			for (const focusArc of focusArcs) {
				await expect(focusArc).toHaveCount(1);
				await focusArc.focus();
				const focusPath = focusArc.locator(":scope > .flow-ipm-focus-rail");
				await expect(focusPath).toHaveCSS("stroke-width", "14px");
				await expect(focusPath).toHaveCSS(
					"stroke-dasharray",
					/3px(?:, )?2px|3 2/,
				);
				expect(
					await focusArc.evaluate((arcElement) => {
						const rail = arcElement.querySelector(".flow-ipm-focus-rail");
						if (rail === null) return false;
						return [...arcElement.children]
							.filter((child) =>
								child.matches("path:not(.flow-ipm-delete-mark)"),
							)
							.every(
								(child) =>
									child === rail ||
									Boolean(
										child.compareDocumentPosition(rail) &
											Node.DOCUMENT_POSITION_FOLLOWING,
									),
							);
					}),
				).toBe(true);
			}
		}
		if (algorithmId !== "electrical-flow-interior-point-mcf") {
			await page.emulateMedia({ colorScheme: "dark", forcedColors: "active" });
			const forcedColorText = await panel.evaluate((element, selector) => {
				const canvasText = getComputedStyle(document.documentElement).color;
				const values = [...element.querySelectorAll<SVGTextElement>(selector)]
					.filter((label) => label.getBoundingClientRect().height > 0)
					.map((label) => ({
						fill: getComputedStyle(label).fill,
						paintOrder: getComputedStyle(label).paintOrder,
					}));
				return { canvasText, values };
			}, labelSelector);
			expect(forcedColorText.values.length).toBeGreaterThan(0);
			expect(
				forcedColorText.values.every(
					(value) =>
						value.fill === forcedColorText.canvasText &&
						value.paintOrder.includes("stroke"),
				),
			).toBe(true);
		}
		await page.emulateMedia({ colorScheme: "dark", forcedColors: "none" });
		await summary.click();
	}
});

test("eight representative solver families project the same common step semantics", async ({
	page,
}) => {
	test.slow();
	const representatives: readonly [FlowProblem, readonly string[]][] = [
		[
			"Max Flow",
			["edmonds-karp", "ford-fulkerson", "dinic", "generic-push-relabel"],
		],
		[
			"Min-Cost Flow",
			[
				"successive-shortest-path",
				"cost-scaling",
				"primal-network-simplex",
				"simple-cycle-canceling",
			],
		],
	];
	for (const [problem, algorithmIds] of representatives) {
		await openFlow(page, problem);
		for (const algorithmId of algorithmIds) {
			await selectFlowAlgorithm(page, algorithmId);
			await computeTrace(page);
			const workspace = activeFlowWorkspace(page);
			await workspace
				.getByRole("combobox", { name: "Playback granularity" })
				.selectOption("operation");
			const nextStep = workspace.getByRole("button", { name: "Next step" });
			const readout = workspace.getByTestId("flow-timeline-readout");
			let before = (await readout.textContent()) ?? "";
			await nextStep.click();
			await expect(readout).not.toHaveText(before);
			await expect(workspace.locator(".flow-event-action")).not.toBeEmpty();
			const changed = workspace.locator('[data-event-change="true"]');
			const touched = workspace.locator('[data-event-touch="true"]');
			for (
				let attempt = 0;
				attempt < 16 &&
				(await changed.count()) === 0 &&
				(await touched.count()) === 0;
				attempt += 1
			) {
				await expect(nextStep).toBeEnabled();
				before = (await readout.textContent()) ?? "";
				await nextStep.click();
				await expect(readout).not.toHaveText(before);
				await expect(workspace.locator(".flow-event-action")).not.toBeEmpty();
			}
			expect(
				(await changed.count()) + (await touched.count()),
				`${algorithmId} visible semantic projection`,
			).toBeGreaterThan(0);
			await expect(workspace.locator(".flow-work-observation")).toHaveCount(0);
			await expect(workspace.getByTestId("flow-step-evidence")).toHaveAttribute(
				"data-evidence-kind",
				"source-event",
			);
			const inspector = workspace.getByLabel("Flow scene inspector");
			const overview = inspectorOverview(inspector);
			await expect(
				overview.locator("dt", { hasText: "Boundary" }).locator(".."),
			).toContainText(/Detail|Phase|Operation/);
			await expect(
				overview.locator("dt", { hasText: "Effect" }).locator(".."),
			).toContainText(
				/Read state|Select structure|Change working state|Commit flow|Certify result/,
			);
			await expect(
				inspector.locator("dt", { hasText: "Work delta" }).locator(".."),
			).toContainText(/\+[1-9][0-9]*|published transition/);
			await expect(
				inspector.locator("dt", { hasText: "Touched" }).locator(".."),
			).toBeVisible();
			await expect(
				inspector.locator("dt", { hasText: "Changed" }).locator(".."),
			).toBeVisible();
			for (const [label, entities, identityAttribute] of [
				["Touched", touched, "data-event-identities"],
				["Changed", changed, "data-changed-identities"],
			] as const) {
				const expectedIdentities = (
					(await inspector
						.locator("dt", { hasText: new RegExp(`^${label}$`, "u") })
						.locator("..")
						.locator("dd")
						.getAttribute("data-event-identities")) ?? ""
				)
					.split("|")
					.filter(Boolean)
					.sort();
				const renderedIdentities = await entities.evaluateAll(
					(items, attribute) =>
						[
							...new Set(
								items.flatMap((item) =>
									(item.getAttribute(attribute) ?? "")
										.split("|")
										.filter(Boolean),
								),
							),
						].sort(),
					identityAttribute,
				);
				expect(
					renderedIdentities,
					`${algorithmId} exact ${label.toLowerCase()} projection`,
				).toEqual(expectedIdentities);
			}
		}
	}
});

test("generator shares generic topologies and retains model-specific rows as disabled choices", async ({
	page,
}) => {
	await page.setViewportSize({ width: 390, height: 844 });
	await openFlow(page, "Min-Cost Flow");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	let dialog = page.getByRole("dialog", {
		name: "Generate Min-Cost Flow graph",
	});
	await expect
		.poll(() =>
			dialog.evaluate(
				(element) => element.scrollWidth <= element.clientWidth + 1,
			),
		)
		.toBe(true);
	await expect
		.poll(() =>
			page.evaluate(
				() => document.documentElement.scrollWidth <= window.innerWidth,
			),
		)
		.toBe(true);
	await expect(dialog.locator("[data-family-id]")).toHaveCount(0);
	await expect(dialog.locator(".flow-generator-shape-selector")).toContainText(
		"Layered DAG",
	);
	await expect(
		dialog.getByRole("spinbutton", { name: "Internal layers" }),
	).toHaveValue("5");
	await expect(
		dialog.getByRole("spinbutton", { name: "Layer width" }),
	).toHaveValue("4");
	await expect(dialog.getByRole("spinbutton", { name: "Fanout" })).toHaveValue(
		"2",
	);
	await expect(
		dialog.getByRole("button", { name: "Readable trace" }),
	).toBeHidden();
	await dialog.getByRole("button", { name: "Change shape" }).click();
	const familyRows = dialog.locator("[data-family-id]");
	await expect(familyRows).toHaveCount(50);
	await expect(
		dialog.locator('[data-selection-state="available"]'),
	).toHaveCount(50);
	const layered = generatorFamily(dialog, "layered-dag");
	await expect(layered).toHaveAttribute("aria-disabled", "false");
	await expect(layered).toHaveAccessibleDescription(
		"Topology adapted to fixed-flow Min-Cost Flow",
	);
	const assignmentFamily = generatorFamily(dialog, "assignment-matrix");
	await expect(assignmentFamily).toHaveAttribute("aria-disabled", "false");
	await selectGeneratorFamily(dialog, "netgen-skeleton");
	await dialog.getByText("Presets & generator notes", { exact: true }).click();
	const maxFlowPresetInMin = dialog.locator(
		'[data-netgen-preset="single-source-max-flow"]',
	);
	await expect(maxFlowPresetInMin).toHaveAttribute("aria-disabled", "true");
	await expect(maxFlowPresetInMin).toHaveAccessibleDescription(
		"Preset belongs to Max Flow",
	);
	await maxFlowPresetInMin.focus();
	await expect(maxFlowPresetInMin).toBeFocused();
	await page.keyboard.press("Enter");
	await expect(maxFlowPresetInMin).toHaveAttribute("aria-pressed", "false");
	await dialog.getByRole("button", { name: "Cancel", exact: true }).click();

	await page.setViewportSize({ width: 1440, height: 960 });
	await page.getByRole("button", { name: "Max Flow", exact: true }).click();
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	dialog = page.getByRole("dialog", { name: "Generate Max Flow graph" });
	await expect(dialog.locator(".flow-generator-shape-selector")).toContainText(
		"Layered DAG",
	);
	await expect(
		dialog.getByRole("spinbutton", { name: "Internal layers" }),
	).toHaveValue("5");
	await expect(
		dialog.getByRole("spinbutton", { name: "Layer width" }),
	).toHaveValue("4");
	await expect(dialog.getByRole("spinbutton", { name: "Fanout" })).toHaveValue(
		"2",
	);
	await dialog.getByRole("button", { name: "Change shape" }).click();
	await expect(dialog.locator("[data-family-id]")).toHaveCount(50);
	await expect(
		dialog.locator('[data-selection-state="available"]'),
	).toHaveCount(48);
	await expect(dialog.locator('[data-selection-state="disabled"]')).toHaveCount(
		2,
	);
	expect(
		await dialog
			.locator('[data-selection-state="disabled"]')
			.evaluateAll((families) =>
				families.map((family) => family.getAttribute("data-family-id")).sort(),
			),
	).toEqual(["assignment-matrix", "transportation-table"]);
	const adaptedReason = generatorFamily(dialog, "cycle").locator(
		":scope > small",
	);
	await expect(adaptedReason).toContainText(
		"Topology adapted to source/sink Max Flow",
	);
	const adaptedReasonBox = await adaptedReason.boundingBox();
	expect(adaptedReasonBox?.width ?? 0).toBeGreaterThan(100);
	expect(adaptedReasonBox?.height ?? Number.POSITIVE_INFINITY).toBeLessThan(40);
	const assignment = generatorFamily(dialog, "assignment-matrix");
	await expect(assignment).toHaveAttribute("aria-disabled", "true");
	await expect(assignment).toHaveAccessibleDescription(
		"Generates Min-Cost Flow scenarios",
	);
	await assignment.focus();
	await expect(assignment).toBeFocused();
	await page.keyboard.press("Enter");
	await expect(assignment).toHaveAttribute("aria-pressed", "false");
	const netgen = generatorFamily(dialog, "netgen-skeleton");
	await expect(netgen).toHaveAttribute("data-selection-state", "available");
	await netgen.click();
	await dialog.getByText("Presets & generator notes", { exact: true }).click();
	await expect(
		dialog.locator('[data-netgen-preset="single-source-max-flow"]'),
	).toHaveAttribute("aria-pressed", "true");
	const minCostPresetInMax = dialog.locator(
		'[data-netgen-preset="general-min-cost"]',
	);
	await expect(minCostPresetInMax).toHaveAttribute("aria-disabled", "true");
	await expect(minCostPresetInMax).toHaveAccessibleDescription(
		"Preset belongs to Min-Cost Flow",
	);
	const canonicalPreset = dialog.getByRole("button", {
		name: "Readable trace",
	});
	await expect(canonicalPreset).toHaveAttribute("aria-disabled", "true");
	await expect(canonicalPreset).toHaveAccessibleDescription(
		/Canonical benchmark presets are unavailable: Preset belongs to Min-Cost Flow/,
	);
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });
	const maxWorkspace = activeFlowWorkspace(page);
	await expect(
		maxWorkspace.getByRole("heading", { name: "Max Flow", level: 2 }),
	).toBeVisible();
	await expect(maxWorkspace.locator(".flow-scenario-editor")).toHaveValue(
		/"kind": "max-flow"/,
	);
	await expect(maxWorkspace.locator(".flow-scenario-editor")).not.toHaveValue(
		/"cost": "1"/,
	);
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	dialog = page.getByRole("dialog", { name: "Generate Max Flow graph" });
	await dialog.getByLabel("Minimum cost c1").fill("0");
	await expect(
		dialog.getByRole("button", { name: "Generate & load" }),
	).toBeEnabled();
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });
	await expect(maxWorkspace.locator(".flow-scenario-editor")).toHaveValue(
		/"kind": "max-flow"/,
	);

	await page
		.getByRole("button", { name: "Min-Cost Flow", exact: true })
		.click();
	await page.getByRole("button", { name: "Algorithm", exact: true }).click();
	dialog = page.getByRole("dialog", { name: "Flow algorithms" });
	await expect(dialog.locator("[data-algorithm-id]")).toHaveCount(44);
	const incompatible = dialog.locator(
		'[data-algorithm-id="transportation-simplex"]',
	);
	await expect(incompatible).toHaveAttribute(
		"data-selection-reason",
		"incompatible",
	);
	await expect(incompatible).toHaveAttribute("tabindex", "0");
	await expect(incompatible).toHaveAccessibleDescription("Incompatible model");
	await incompatible.focus();
	await expect(incompatible).toBeFocused();
	await dialog
		.getByRole("combobox", { name: "Compatibility" })
		.selectOption("all");
	await expect(dialog.locator("[data-algorithm-id]")).toHaveCount(93);
});

test("generator shape keyboard focus stays above the mobile actions and returns to its trigger", async ({
	page,
}, testInfo) => {
	await page.setViewportSize({ width: 320, height: 568 });
	await openFlow(page, "Max Flow");
	const trigger = activeFlowWorkspace(page).getByRole("button", {
		name: "Generate",
		exact: true,
	});
	await trigger.focus();
	await page.keyboard.press("Enter");
	let dialog = page.getByRole("dialog", { name: "Generate Max Flow graph" });
	await dialog.getByRole("button", { name: "Change shape" }).click();

	const options = dialog.locator("[data-family-id]");
	const cycleIndex = await options.evaluateAll((elements) =>
		elements.findIndex(
			(element) => element.getAttribute("data-family-id") === "cycle",
		),
	);
	expect(cycleIndex).toBeGreaterThan(0);
	await options.nth(cycleIndex - 1).focus();
	await page.keyboard.press(
		testInfo.project.name === "webkit" && process.platform === "darwin"
			? "Alt+Tab"
			: "Tab",
	);
	const cycle = generatorFamily(dialog, "cycle");
	await expect(cycle).toBeFocused();
	await expect
		.poll(() =>
			cycle.evaluate((element) => {
				const option = element.getBoundingClientRect();
				const scrollRegion = element
					.closest(".flow-generator-dialog-scroll-region")
					?.getBoundingClientRect();
				const actions = element
					.closest("[role=dialog]")
					?.querySelector(".dialog-actions")
					?.getBoundingClientRect();
				return (
					scrollRegion !== undefined &&
					actions !== undefined &&
					option.top >= scrollRegion.top &&
					option.bottom <= scrollRegion.bottom &&
					option.bottom <= actions.top
				);
			}),
		)
		.toBe(true);

	await page.keyboard.press("Escape");
	await expect(dialog).toBeHidden();
	await expect(trigger).toBeFocused();

	await page.keyboard.press("Enter");
	dialog = page.getByRole("dialog", { name: "Generate Max Flow graph" });
	const cancel = dialog.getByRole("button", { name: "Cancel", exact: true });
	await cancel.focus();
	await page.keyboard.press("Enter");
	await expect(dialog).toBeHidden();
	await expect(trigger).toBeFocused();
});

test("algorithm catalog returns keyboard focus after every close path", async ({
	page,
}) => {
	await page.setViewportSize({ width: 390, height: 844 });
	await openFlow(page, "Max Flow");
	const workspace = activeFlowWorkspace(page);
	const trigger = workspace.getByRole("button", {
		name: "Algorithm",
		exact: true,
	});

	await trigger.focus();
	await page.keyboard.press("Enter");
	let dialog = page.getByRole("dialog", { name: "Flow algorithms" });
	await expect(dialog).toBeVisible();
	await page.keyboard.press("Escape");
	await expect(dialog).toBeHidden();
	await expect(trigger).toBeFocused();

	await page.keyboard.press("Enter");
	dialog = page.getByRole("dialog", { name: "Flow algorithms" });
	const close = dialog.getByRole("button", { name: "Close", exact: true });
	await close.focus();
	await page.keyboard.press("Enter");
	await expect(dialog).toBeHidden();
	await expect(trigger).toBeFocused();

	await page.keyboard.press("Enter");
	dialog = page.getByRole("dialog", { name: "Flow algorithms" });
	const dinic = dialog.locator('[data-algorithm-id="dinic"]');
	await expect(dinic).toHaveAttribute("data-selection-reason", "ready");
	await dinic.getByRole("button", { name: "Select", exact: true }).focus();
	await page.keyboard.press("Enter");
	await expect(dialog).toBeHidden({ timeout: 30_000 });
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/"id": "dinic"/,
		{ timeout: 30_000 },
	);
	await expect(trigger).toBeEnabled({ timeout: 30_000 });
	await expect(trigger).toBeFocused();
});

test("excess scaling rejects negative residual cycles before publishing ready", async ({
	page,
}) => {
	await openFlow(page, "Min-Cost Flow");
	const workspace = activeFlowWorkspace(page);
	const editor = await revealFlowScenarioEditor(page);
	const scenario = JSON.parse(await editor.inputValue()) as {
		payload: Record<string, unknown>;
	};
	scenario.payload.model = { kind: "transshipment" };
	scenario.payload.algorithm = { id: "excess-scaling-mcf", config: {} };
	scenario.payload.graph = {
		nodes: [
			{ id: "a", supply: "0" },
			{ id: "b", supply: "0" },
		],
		edges: [
			{
				id: "ab",
				from: "a",
				to: "b",
				lower: "0",
				capacity: "1",
				cost: "-2",
			},
			{
				id: "ba",
				from: "b",
				to: "a",
				lower: "0",
				capacity: "1",
				cost: "1",
			},
		],
	};
	await editor.fill(JSON.stringify(scenario, null, 2));

	await workspace
		.getByRole("button", { name: "Algorithm", exact: true })
		.click();
	const dialog = page.getByRole("dialog", { name: "Flow algorithms" });
	const excess = dialog.locator('[data-algorithm-id="excess-scaling-mcf"]');
	await expect(excess).toHaveAttribute(
		"data-selection-reason",
		"negative-residual-cycle-absent-required",
	);
	await expect(excess.getByRole("button")).toBeDisabled();
	await expect(excess).toHaveAccessibleDescription(
		"Currently selected; Remove negative-cost cycles from the lower-bound residual graph",
	);
	await dialog.getByRole("button", { name: "Close", exact: true }).click();

	await workspace.getByRole("button", { name: "Load", exact: true }).click();
	await expect(
		workspace.getByText("Input error", { exact: true }),
	).toBeVisible();
	await expect(
		workspace.getByText("Engine error", { exact: true }),
	).toBeHidden();
	await expect(
		workspace.getByText(
			"Selected flow algorithm is not runnable: Remove negative-cost cycles from the lower-bound residual graph",
			{ exact: true },
		),
	).toBeVisible();
	await expect(
		workspace.getByRole("button", { name: "Algorithm", exact: true }),
	).toBeEnabled();
});

test("binding fixed-flow capacities reject excess scaling before engine construction", async ({
	page,
}) => {
	await openFlow(page, "Min-Cost Flow");
	const workspace = activeFlowWorkspace(page);
	await expect(
		workspace
			.locator(".flow-inspector-panel .property-list")
			.getByText("successive-shortest-path", { exact: true }),
	).toBeVisible();
	const acceptedSceneHeading = await workspace
		.getByRole("heading", { level: 2, name: /Min-Cost Flow/ })
		.innerText();
	const editor = await revealFlowScenarioEditor(page);
	const scenario = JSON.parse(await editor.inputValue()) as {
		payload: Record<string, unknown>;
	};
	scenario.payload.model = {
		kind: "fixed-flow-min-cost",
		source: "s",
		sink: "t",
		required_flow: "2",
	};
	scenario.payload.algorithm = { id: "excess-scaling-mcf", config: {} };
	scenario.payload.graph = {
		nodes: [
			{ id: "s", supply: "0" },
			{ id: "a", supply: "0" },
			{ id: "b", supply: "0" },
			{ id: "t", supply: "0" },
		],
		edges: [
			{ id: "sa", from: "s", to: "a", capacity: "1", cost: "1" },
			{ id: "at", from: "a", to: "t", capacity: "1", cost: "1" },
			{ id: "sb", from: "s", to: "b", capacity: "1", cost: "2" },
			{ id: "bt", from: "b", to: "t", capacity: "1", cost: "2" },
		],
	};
	await editor.fill(JSON.stringify(scenario, null, 2));

	await workspace.getByRole("button", { name: "Load", exact: true }).click();
	await expect(workspace.locator(".flow-status")).toHaveText("Input error");
	await expect(
		workspace.getByText("Engine error", { exact: true }),
	).toBeHidden();
	await expect(
		workspace.getByText(
			"Selected flow algorithm is not runnable: Each residual capacity range must cover the required flow",
			{ exact: true },
		),
	).toBeVisible();
	await expect(
		workspace.getByRole("button", { name: "Algorithm", exact: true }),
	).toBeEnabled();
	await expect(
		workspace.getByRole("button", { name: "Run trace", exact: true }),
	).toBeDisabled();
	await expect(
		workspace.getByRole("heading", {
			level: 2,
			name: acceptedSceneHeading,
		}),
	).toBeVisible();
	await expect(
		workspace
			.locator(".flow-inspector-panel .property-list")
			.getByText("successive-shortest-path", { exact: true }),
	).toBeVisible();
});

test("generation replaces a previously runnable excess-scaling selection with the generated model default", async ({
	page,
}) => {
	test.setTimeout(180_000);
	await openFlow(page, "Min-Cost Flow");
	const workspace = activeFlowWorkspace(page);
	const editor = await revealFlowScenarioEditor(page);
	const scenario = JSON.parse(await editor.inputValue()) as {
		payload: Record<string, unknown>;
	};
	scenario.payload.model = {
		kind: "fixed-flow-min-cost",
		source: "s",
		sink: "t",
		required_flow: "2",
	};
	scenario.payload.algorithm = { id: "excess-scaling-mcf", config: {} };
	scenario.payload.graph = {
		nodes: [
			{ id: "s", supply: "0" },
			{ id: "t", supply: "0" },
		],
		edges: [
			{ id: "cheap", from: "s", to: "t", capacity: "2", cost: "1" },
			{
				id: "expensive",
				from: "s",
				to: "t",
				capacity: "2",
				cost: "3",
			},
		],
	};
	await editor.fill(JSON.stringify(scenario, null, 2));
	await workspace.getByRole("button", { name: "Load", exact: true }).click();
	await expect(workspace.locator(".flow-status")).toHaveText("Validated", {
		timeout: 30_000,
	});
	await expect(
		workspace
			.locator(".flow-inspector-panel .property-list")
			.getByText("excess-scaling-mcf", { exact: true }),
	).toBeVisible();

	await workspace
		.getByRole("button", { name: "Generate", exact: true })
		.click();
	const dialog = page.getByRole("dialog", {
		name: "Generate Min-Cost Flow graph",
	});
	await selectGeneratorFamily(dialog, "layered-dag");
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });
	await expect(workspace.locator(".flow-status")).toHaveText("Validated", {
		timeout: 30_000,
	});
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/"id": "successive-shortest-path"/,
	);
	await expect(
		workspace
			.locator(".flow-inspector-panel .property-list")
			.getByText("successive-shortest-path", { exact: true }),
	).toBeVisible();
	await expect(
		workspace.getByText("Engine error", { exact: true }),
	).toBeHidden();

	await workspace
		.getByRole("button", { name: "Run trace", exact: true })
		.click();
	await expect(workspace.getByTestId("flow-timeline-readout")).toHaveText(
		/Raw [1-9][0-9]* \/ [1-9][0-9]*/,
		{ timeout: 60_000 },
	);
	const pause = workspace.getByRole("button", { name: "Pause", exact: true });
	if (await pause.isVisible()) await pause.click();
	const readout = workspace.getByTestId("flow-timeline-readout");
	const extent = Number(
		(await readout.textContent())?.match(/^Raw [0-9]+ \/ ([0-9]+)$/)?.[1],
	);
	expect(
		extent,
		"the default fixed-flow graph must retain a nontrivial complexity-faithful trace",
	).toBeGreaterThan(1_000);
	await workspace.getByRole("button", { name: "Last event" }).click();
	await expect(readout).toHaveText(`Raw ${extent} / ${extent}`, {
		timeout: 60_000,
	});
	const summary = workspace.locator(".flow-inspector-panel .property-list");
	await expect(summary.getByText(/^Total cost -?[0-9]+$/)).toBeVisible();
	await expect(
		summary.getByText("No negative cycle", { exact: true }),
	).toBeVisible();
	await expect(
		workspace.getByText("No result · trace publication ceiling reached", {
			exact: true,
		}),
	).toBeHidden();
});

test("generated NETGEN precheck stays overlay-only and completes its network-simplex trace", async ({
	page,
}) => {
	test.setTimeout(180_000);
	await openFlow(page, "Min-Cost Flow");
	const workspace = activeFlowWorkspace(page);
	await workspace
		.getByRole("button", { name: "Generate", exact: true })
		.click();
	const dialog = page.getByRole("dialog", {
		name: "Generate Min-Cost Flow graph",
	});
	await selectGeneratorFamily(dialog, "netgen-skeleton");
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });
	await expect(workspace.locator(".flow-status")).toHaveText("Validated", {
		timeout: 30_000,
	});
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/"id": "primal-network-simplex"/,
	);

	await workspace
		.getByRole("button", { name: "Run trace", exact: true })
		.click();
	const graph = workspace.getByRole("img", { name: "Validated flow network" });
	const feasibility = graph.locator('[data-feasibility-use="precheck-only"]');
	await expect(feasibility).toBeVisible({ timeout: 60_000 });
	await expect(feasibility.locator(".flow-feasibility-status")).toContainText(
		"PRECHECK",
	);
	await expect(
		workspace.getByText("Engine error", { exact: true }),
	).toBeHidden();

	const speed = workspace.getByRole("combobox", { name: "Playback speed" });
	await expect(speed).toBeEnabled();
	await speed.selectOption("32");
	await expect(speed).toHaveValue("32");
	await expect(workspace.getByTestId("flow-timeline-readout")).toHaveText(
		/^Raw ([1-9][0-9]*) \/ \1$/,
		{ timeout: 90_000 },
	);
	await expect(workspace.locator(".flow-status")).toHaveText("Validated");
	await expect(
		workspace.getByText("Engine error", { exact: true }),
	).toBeHidden();
	await expect(
		workspace
			.locator(".flow-inspector-panel .property-list")
			.getByText(/Total cost [0-9-]+/),
	).toBeVisible();
});

test("transformed feasibility boundaries isolate their exact internal graph", async ({
	page,
}) => {
	test.setTimeout(180_000);
	await openFlow(page, "Min-Cost Flow");
	const workspace = activeFlowWorkspace(page);
	const editor = await revealFlowScenarioEditor(page);
	const scenario = JSON.parse(await editor.inputValue()) as {
		payload: Record<string, unknown>;
	};
	const load = async (payload: Record<string, unknown>) => {
		scenario.payload = {
			...scenario.payload,
			...payload,
			run_profile: "trace",
			trace_granularity: "operation",
			algorithm_seed: "0",
		};
		await editor.fill(JSON.stringify(scenario, null, 2));
		await workspace.getByRole("button", { name: "Load", exact: true }).click();
		await expect(workspace.locator(".flow-status")).toHaveText("Validated", {
			timeout: 30_000,
		});
		await expect(
			workspace.getByText("Engine error", { exact: true }),
		).toBeHidden();
		await computeTrace(page);
	};
	const graph = workspace.getByRole("img", { name: "Validated flow network" });

	await load({
		model: { kind: "transshipment" },
		graph: {
			nodes: [
				{ id: "s", supply: "3" },
				{ id: "m", supply: "0" },
				{ id: "t", supply: "-3" },
			],
			edges: [
				{ id: "a", from: "s", to: "m", capacity: "3", cost: "1" },
				{ id: "b", from: "m", to: "t", capacity: "3", cost: "2" },
				{
					id: "expensive",
					from: "s",
					to: "t",
					capacity: "3",
					cost: "9",
				},
			],
		},
		algorithm: { id: "orlin-mcf", config: {} },
	});
	await workspace.getByRole("button", { name: "Last event" }).click();
	const standalone = graph.locator(
		'[data-feasibility-use="anchored-recovery"][data-feasibility-domain="standalone-transformation"]',
	);
	await stepBackwardUntilVisible(page, standalone, 8);
	await expect(graph).toHaveAttribute(
		"data-active-overlays",
		"feasibility_overlay",
	);
	await expect(graph.locator("[data-node-id]")).toHaveCount(0);
	await expect(graph.locator("[data-edge-id]")).toHaveCount(0);
	await expect(graph.locator("[data-edge-label-for]")).toHaveCount(0);
	await expect(
		standalone.locator("[data-feasibility-domain-node]"),
	).toHaveCount(6);
	expect(
		await standalone.locator("[data-feasibility-arc]").count(),
	).toBeGreaterThan(0);

	await load({
		model: { kind: "transshipment" },
		graph: {
			nodes: [
				{ id: "a", supply: "5" },
				{ id: "b", supply: "-4" },
				{ id: "c", supply: "-1" },
			],
			edges: [
				{ id: "ab", from: "a", to: "b", capacity: "20", cost: "0" },
				{ id: "ac", from: "a", to: "c", capacity: "20", cost: "4" },
				{ id: "ba", from: "b", to: "a", capacity: "20", cost: "4" },
				{ id: "bc", from: "b", to: "c", capacity: "20", cost: "0" },
				{ id: "ca", from: "c", to: "a", capacity: "20", cost: "4" },
				{ id: "cb", from: "c", to: "b", capacity: "20", cost: "4" },
			],
		},
		algorithm: { id: "enhanced-capacity-scaling", config: {} },
	});
	await workspace.getByRole("button", { name: "Last event" }).click();
	const aligned = graph.locator(
		'[data-feasibility-use="anchored-recovery"][data-feasibility-domain="node-aligned-transformation"]',
	);
	await stepBackwardUntilVisible(page, aligned, 8);
	await expect(graph).toHaveAttribute(
		"data-active-overlays",
		"feasibility_overlay",
	);
	await expect(graph.locator("[data-node-id]")).toHaveCount(3);
	await expect(graph.locator("[data-edge-label-for]")).toHaveCount(0);
	await expect(
		graph.locator(".flow-feasibility-public-edge-context"),
	).toBeVisible();

	await load({
		model: { kind: "convex-cost-flow" },
		graph: {
			nodes: [
				{ id: "s", supply: "3" },
				{ id: "m", supply: "0" },
				{ id: "t", supply: "-3" },
			],
			edges: [
				{
					id: "direct",
					from: "s",
					to: "t",
					capacity: "3",
					cost: "0",
					convex_cost: {
						base_cost_at_zero: "7",
						segments: [
							{ end_flow: "1", marginal_cost: "0" },
							{ end_flow: "3", marginal_cost: "5" },
						],
					},
				},
				{
					id: "sm",
					from: "s",
					to: "m",
					lower: "1",
					capacity: "3",
					cost: "1",
				},
				{
					id: "mt",
					from: "m",
					to: "t",
					lower: "1",
					capacity: "3",
					cost: "1",
				},
			],
		},
		algorithm: { id: "segment-expanded-convex-mcf", config: {} },
	});
	await workspace.getByRole("button", { name: "Next step" }).click();
	await expect(aligned).toBeVisible();
	await expect(aligned.locator(".flow-feasibility-terminal")).toHaveCount(0);
	await expect(graph.locator("[data-edge-label-for]")).toHaveCount(0);
	await expect(aligned.locator(".flow-feasibility-arc.is-focused")).toHaveCount(
		1,
	);
	const midpointDistance = await aligned
		.locator(".flow-feasibility-focus")
		.evaluate((path) => {
			if (!(path instanceof SVGPathElement)) return 0;
			const middle = path.getPointAtLength(path.getTotalLength() / 2);
			const node = path.ownerSVGElement?.querySelector<SVGCircleElement>(
				'[data-node-id="m"] circle.flow-node',
			);
			if (node === null || node === undefined) return 0;
			return Math.hypot(
				middle.x - node.cx.baseVal.value,
				middle.y - node.cy.baseVal.value,
			);
		});
	expect(midpointDistance).toBeGreaterThan(45);
});

test("Orlin structure density keeps a 40-node compact network attributable", async ({
	page,
}) => {
	test.setTimeout(240_000);
	await openFlow(page, "Max Flow");
	const workspace = activeFlowWorkspace(page);
	await workspace
		.getByRole("button", { name: "Generate", exact: true })
		.click();
	const dialog = page.getByRole("dialog", { name: "Generate Max Flow graph" });
	await selectGeneratorFamily(dialog, "dinic-worst-case");
	await dialog.getByRole("spinbutton", { name: "Vertices" }).fill("40");
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/"nodes": 40/,
	);
	await selectFlowAlgorithm(page, "orlin-max-flow");
	await computeTrace(page);
	await selectMicroSteps(page);
	await stepUntilTraceCatalog(page, "inspect-subproblem-arc", 120);

	const graph = workspace.getByRole("img", { name: "Validated flow network" });
	await expect(graph).toHaveAttribute("data-flow-lod", "structure");
	await expect(graph.locator(".flow-node-frame[data-node-id]")).toHaveCount(40);
	await expect(graph.locator(".flow-original-edge[data-edge-id]")).toHaveCount(
		77,
	);
	const compactArcs = graph.locator(".flow-orlin-max-compact");
	expect(await compactArcs.count()).toBeGreaterThan(0);
	await expect(
		graph.locator(
			'.flow-orlin-max-compact-original:not([data-orlin-max-compact-active="true"]):not([data-orlin-max-scan])',
		),
	).toHaveCount(0);
	expect(
		await graph
			.locator(".flow-orlin-max-compact-original[data-orlin-max-scan]")
			.count(),
	).toBeGreaterThan(0);
	expect(
		await graph.locator(".flow-orlin-max-original-class:visible").count(),
	).toBeLessThanOrEqual(1);
	expect(
		await graph.locator(".flow-event-touch-node-ring:visible").count(),
	).toBeLessThanOrEqual(2);
	await assertReadableNodeTraceCallouts(page, 6);

	await stepUntilTraceCatalog(page, "inspect-lift-residual-arc", 400);
	const localLabel = graph.locator(
		".flow-orlin-max-original-label[data-orlin-max-label-owner]:visible",
	);
	await expect(localLabel).toHaveCount(1);
	await assertOrlinOriginalLabelsClearNodes(graph);
	await page.setViewportSize({ width: 390, height: 844 });
	await expect(localLabel).toHaveCount(1);
	await assertOrlinOriginalLabelsClearNodes(graph);
	await page.setViewportSize({ width: 1440, height: 900 });
	await expect(localLabel).toHaveCount(1);
	await assertOrlinOriginalLabelsClearNodes(graph);

	await workspace
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("operation");
	await workspace.getByRole("button", { name: "Last event" }).click();
	await stepBackwardUntilTraceCatalog(page, "lift-path", 80);
	const lateLift = graph.locator(
		".flow-orlin-max-original-class.flow-orlin-max-original-active",
	);
	expect(
		await lateLift.count(),
		"the stress case must exercise a long lifted original path",
	).toBeGreaterThan(20);
	const lateLiftPaintErrors = await lateLift
		.locator(":scope > path")
		.evaluateAll((paths) =>
			paths.flatMap((path) => {
				if (!(path instanceof SVGPathElement)) return ["not-an-svg-path"];
				const style = getComputedStyle(path);
				return path.getTotalLength() > 0 &&
					style.stroke !== "none" &&
					Number(style.strokeOpacity) > 0 &&
					path.getAttribute("marker-end")?.startsWith("url(")
					? []
					: ["inactive-or-undirected-lift-path"];
			}),
		);
	expect(lateLiftPaintErrors).toEqual([]);
	expect(
		await lateLift.locator("text:visible").count(),
		"Structure keeps the path but labels only an attributable local target",
	).toBeLessThanOrEqual(1);

	await selectMicroSteps(page);
	const readout = workspace.getByTestId("flow-timeline-readout");
	const extentMatch = /^Raw \d+ \/ (\d+)$/.exec(
		(await readout.textContent())?.trim() ?? "",
	);
	if (extentMatch === null)
		throw new Error("Orlin trace extent is unavailable");
	const extent = Number(extentMatch[1]);
	await workspace.getByRole("button", { name: "First event" }).click();
	await expect(readout).toHaveText(`Raw 0 / ${extent}`);
	const next = workspace.getByRole("button", { name: "Next step" });
	let attributedLabelEvents = 0;
	for (let raw = 1; raw <= extent; raw += 1) {
		await next.click();
		await expect(readout).toHaveText(`Raw ${raw} / ${extent}`);
		if (await localLabel.isVisible()) {
			attributedLabelEvents += 1;
			await assertOrlinOriginalLabelsClearNodes(graph);
		}
	}
	expect(attributedLabelEvents).toBeGreaterThan(0);
});

test("generic generator shapes materialize canonically in both flow workspaces", async ({
	page,
}) => {
	await openFlow(page, "Min-Cost Flow");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	let dialog = page.getByRole("dialog", {
		name: "Generate Min-Cost Flow graph",
	});
	await selectGeneratorFamily(dialog, "layered-dag");
	await expect(dialog.locator(".flow-generator-shape-selector")).toContainText(
		"Topology adapted to fixed-flow Min-Cost Flow",
	);
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });
	let workspace = activeFlowWorkspace(page);
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/"kind": "fixed-flow-min-cost"/,
	);
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/"target_problem": "fixed-flow-min-cost"/,
	);
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/"id": "successive-shortest-path"/,
	);
	await expect(
		workspace.getByText("Engine error", { exact: true }),
	).toBeHidden();
	expect(
		await workspace
			.getByRole("img", { name: "Validated flow network" })
			.locator("[data-node-id]")
			.count(),
	).toBeGreaterThan(2);
	await workspace
		.getByRole("button", { name: "Run trace", exact: true })
		.click();
	await expect(workspace.getByTestId("flow-timeline-readout")).toHaveText(
		/Raw [1-9][0-9]* \/ [1-9][0-9]*/,
		{ timeout: 60_000 },
	);
	const pauseGeneratedTrace = workspace.getByRole("button", {
		name: "Pause",
		exact: true,
	});
	if (await pauseGeneratedTrace.isVisible()) await pauseGeneratedTrace.click();
	await expect(workspace.locator(".flow-status")).toHaveText("Validated");
	await workspace
		.getByRole("button", { name: "Algorithm", exact: true })
		.click();
	const algorithmDialog = page.getByRole("dialog", { name: "Flow algorithms" });
	const workspaceAlgorithms = algorithmDialog.locator("[data-algorithm-id]");
	await expect(workspaceAlgorithms).toHaveCount(44);
	const readyCount = await algorithmDialog
		.locator('[data-algorithm-id][data-selection-reason="ready"]')
		.count();
	const incompatibleCount = await algorithmDialog
		.locator('[data-algorithm-id][data-selection-reason="incompatible"]')
		.count();
	expect(
		readyCount,
		"a generated fixed-flow model should leave more ordinary MCF methods ready than model-incompatible",
	).toBeGreaterThan(incompatibleCount);
	expect(
		incompatibleCount * 2,
		"model-specific methods may remain incompatible, but they must not be the majority",
	).toBeLessThan(await workspaceAlgorithms.count());
	await algorithmDialog
		.getByRole("combobox", { name: "Compatibility" })
		.selectOption("all");
	for (const algorithmId of [
		"successive-shortest-path",
		"simple-cycle-canceling",
		"cost-scaling",
		"primal-network-simplex",
	]) {
		await expect(
			algorithmDialog.locator(`[data-algorithm-id="${algorithmId}"]`),
		).toHaveAttribute("data-selection-reason", "ready");
	}
	const excessScaling = algorithmDialog.locator(
		'[data-algorithm-id="excess-scaling-mcf"]',
	);
	await expect(excessScaling).toHaveAttribute(
		"data-selection-reason",
		"nonbinding-transshipment-capacities-required",
	);
	await expect(excessScaling).toHaveAccessibleDescription(
		"Each residual capacity range must cover the required flow",
	);
	await expect(excessScaling.locator(".flow-algorithm-select")).toBeDisabled();
	await algorithmDialog.getByRole("button", { name: "Close" }).click();

	const generatedEditor = workspace.locator(".flow-scenario-editor");
	const generatedScenario = JSON.parse(await generatedEditor.inputValue()) as {
		payload: { algorithm: { id: string; config: Record<string, unknown> } };
	};
	generatedScenario.payload.algorithm = {
		id: "excess-scaling-mcf",
		config: {},
	};
	await generatedEditor.fill(JSON.stringify(generatedScenario, null, 2));
	await expect(
		workspace.getByRole("button", { name: "Run trace", exact: true }),
	).toBeDisabled();
	await workspace.getByRole("button", { name: "Load", exact: true }).click();
	await expect(workspace.locator(".flow-status")).toHaveText("Input error");
	await expect(
		workspace.getByText(
			"Selected flow algorithm is not runnable: Each residual capacity range must cover the required flow",
			{ exact: true },
		),
	).toBeVisible();
	await expect(
		workspace.getByText("Engine error", { exact: true }),
	).toBeHidden();

	await page.getByRole("button", { name: "Max Flow", exact: true }).click();
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	dialog = page.getByRole("dialog", { name: "Generate Max Flow graph" });
	await selectGeneratorFamily(dialog, "cycle");
	await expect(dialog.locator(".flow-generator-shape-selector")).toContainText(
		"Topology adapted to source/sink Max Flow",
	);
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });
	workspace = activeFlowWorkspace(page);
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/"kind": "max-flow"/,
	);
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/"target_problem": "max-flow"/,
	);
});

test("bounded kernels stay discoverable but disabled when the current graph exceeds their numeric domain", async ({
	page,
}) => {
	await openFlow(page, "Min-Cost Flow");
	const workspace = activeFlowWorkspace(page);
	await workspace
		.getByRole("button", { name: "Algorithm", exact: true })
		.click();
	const dialog = page.getByRole("dialog", { name: "Flow algorithms" });

	const bounded = dialog.locator(
		'[data-algorithm-id="deterministic-almost-linear-mcf"]',
	);
	await expect(bounded).toHaveAttribute(
		"data-selection-reason",
		"kernel-capacity-limit",
	);
	await expect(bounded).not.toHaveAttribute("aria-disabled");
	await expect(bounded).toHaveAccessibleDescription(
		"Capacity exceeds this bounded kernel",
	);
	await expect(
		bounded.getByText("Graph outside limits", { exact: true }),
	).toBeVisible();
	const boundedAction = bounded.locator(".flow-algorithm-select");
	await expect(boundedAction).toBeDisabled();
	await expect(boundedAction).toHaveText(
		"Capacity exceeds this bounded kernel",
	);
	await bounded.focus();
	await expect(bounded).toBeFocused();
	await page.keyboard.press("Enter");
	await expect(dialog).toBeVisible();

	const generic = dialog.locator(
		'[data-algorithm-id="successive-shortest-path"]',
	);
	await expect(generic).toHaveAttribute("data-selection-reason", "ready");
	await expect(generic.getByText("Available", { exact: true })).toBeVisible();
	await expect(generic.getByRole("button", { name: "Current" })).toBeDisabled();
});

test("bounded augmenting-electrical limits disable catalog selection and reject direct JSON before execution", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	const workspace = activeFlowWorkspace(page);
	await workspace
		.getByRole("button", { name: "Algorithm", exact: true })
		.click();
	const dialog = page.getByRole("dialog", { name: "Flow algorithms" });
	const bounded = dialog.locator(
		'[data-algorithm-id="augmenting-electrical-flow"]',
	);
	await expect(bounded).toHaveAttribute("data-selection-reason", "node-limit");
	await expect(
		bounded.getByText("Graph outside limits", { exact: true }),
	).toBeVisible();
	await expect(bounded.getByRole("button")).toBeDisabled();
	await expect(bounded.getByRole("button")).toHaveText("Node limit exceeded");
	await dialog.getByRole("button", { name: "Close" }).click();

	const editor = workspace.getByRole("textbox", {
		name: "Flow Scenario JSON",
	});
	const scenario = JSON.parse(await editor.inputValue()) as {
		payload: { algorithm: { config: Record<string, never>; id: string } };
	};
	scenario.payload.algorithm = {
		config: {},
		id: "augmenting-electrical-flow",
	};
	await editor.fill(JSON.stringify(scenario, null, 2));
	await workspace.getByRole("button", { name: "Load", exact: true }).click();
	await expect(workspace.locator(".flow-status")).toHaveText("Input error");
	await expect(
		workspace.getByText(
			"Selected flow algorithm is not runnable: Node limit exceeded",
			{ exact: true },
		),
	).toBeVisible();
	await expect(
		workspace.getByRole("button", { name: "Algorithm", exact: true }),
	).toBeEnabled();
	await expect(
		workspace.getByRole("button", { name: "Generate", exact: true }),
	).toBeEnabled();
	await expect(
		workspace.getByRole("button", { name: "Load", exact: true }),
	).toBeEnabled();
	await expect(
		workspace.getByRole("button", { name: "Run trace", exact: true }),
	).toBeDisabled();
	await expect(
		workspace.getByText("Engine error", { exact: true }),
	).toBeHidden();
});

test("sub-900px workspace navigation exposes all problem workspaces before flow is mounted", async ({
	page,
}) => {
	await page.goto("/");
	for (const width of [320, 641, 768, 899]) {
		await page.setViewportSize({ width, height: 844 });
		const navigation = page.getByRole("navigation", {
			name: "Algorithm workspace",
		});
		const tabGeometry = await navigation
			.getByRole("button")
			.evaluateAll((buttons) =>
				buttons.map((button) => {
					const rect = button.getBoundingClientRect();
					return {
						label: button.textContent?.trim() ?? "",
						left: rect.left,
						right: rect.right,
						viewportWidth: document.documentElement.clientWidth,
					};
				}),
			);
		expect(tabGeometry.map(({ label }) => label)).toEqual([
			"Ordered Map",
			"Max Flow",
			"Min-Cost Flow",
		]);
		for (const tab of tabGeometry) {
			expect(
				tab.left,
				`${width}px ${tab.label} left edge`,
			).toBeGreaterThanOrEqual(0);
			expect(
				tab.right,
				`${width}px ${tab.label} right edge`,
			).toBeLessThanOrEqual(tab.viewportWidth + 1);
		}
	}
});

test("algorithm workspace scope remains stable for invalid and opposite-model drafts", async ({
	page,
}) => {
	await openFlow(page, "Min-Cost Flow");
	const minCostDraft = await flowWorkspace(page, "Min-Cost Flow")
		.locator(".flow-scenario-editor")
		.inputValue();
	expect(minCostDraft).toContain('"kind": "fixed-flow-min-cost"');
	await page.getByRole("button", { name: "Max Flow", exact: true }).click();
	const maxWorkspace = flowWorkspace(page, "Max Flow");
	await expect(
		maxWorkspace.getByRole("heading", { name: "Max Flow", level: 1 }),
	).toBeVisible();
	await expect(
		maxWorkspace.getByText("Validated", { exact: true }),
	).toBeVisible();
	const editor = maxWorkspace.locator(".flow-scenario-editor");
	await editor.fill(minCostDraft);
	await expect(editor).toHaveValue(/"kind": "fixed-flow-min-cost"/);
	await expect(maxWorkspace.getByText("Edited", { exact: true })).toBeVisible();
	await maxWorkspace.getByRole("button", { name: "Algorithm" }).click();
	let dialog = page.getByRole("dialog", { name: "Flow algorithms" });
	await expect(dialog.locator("[data-algorithm-id]")).toHaveCount(49);
	await expect(
		dialog.locator('[data-algorithm-id="edmonds-karp"]'),
	).toHaveAttribute("data-selection-reason", "incompatible");
	await dialog.getByRole("button", { name: "Close", exact: true }).click();

	await editor.fill("{");
	await expect(editor).toHaveValue("{");
	await maxWorkspace.getByRole("button", { name: "Algorithm" }).click();
	dialog = page.getByRole("dialog", { name: "Flow algorithms" });
	await expect(dialog.locator("[data-algorithm-id]")).toHaveCount(49);
	await expect(
		dialog.locator('[data-algorithm-id="edmonds-karp"]'),
	).toHaveAttribute("data-selection-reason", "invalid-model");
});

test("node-complete algorithm config follows the edited Scenario node identities", async ({
	page,
}) => {
	await openFlow(page, "Min-Cost Flow");
	const editor = await revealFlowScenarioEditor(page);
	const draft = JSON.parse(await editor.inputValue()) as {
		payload: {
			model: { source: string; sink: string };
			graph: {
				nodes: Array<{ id: string }>;
				edges: Array<{ from: string; to: string }>;
			};
		};
	};
	const renamed = new Map(
		draft.payload.graph.nodes.map((node, index) => [
			node.id,
			`draft-node-${index}`,
		]),
	);
	for (const node of draft.payload.graph.nodes) {
		node.id = renamed.get(node.id) ?? node.id;
	}
	for (const edge of draft.payload.graph.edges) {
		edge.from = renamed.get(edge.from) ?? edge.from;
		edge.to = renamed.get(edge.to) ?? edge.to;
	}
	draft.payload.model.source =
		renamed.get(draft.payload.model.source) ?? draft.payload.model.source;
	draft.payload.model.sink =
		renamed.get(draft.payload.model.sink) ?? draft.payload.model.sink;
	await editor.fill(JSON.stringify(draft, null, 2));

	await selectFlowAlgorithm(page, "tardos-framework");
	const configured = JSON.parse(await editor.inputValue()) as {
		payload: { algorithm: { config: { potentials: Record<string, string> } } };
	};
	expect(configured.payload.algorithm.config.potentials).toEqual(
		Object.fromEntries([...renamed.values()].map((nodeId) => [nodeId, "0"])),
	);
	await expect(activeFlowWorkspace(page).locator(".flow-status")).toHaveText(
		"Validated",
	);
});

test("input byte measurement runs only when the edited input changes", async ({
	page,
}) => {
	await page.addInitScript(() => {
		const NativeTextEncoder = window.TextEncoder;
		window.__flowTextEncoderConstructionCount = 0;
		class CountingTextEncoder extends NativeTextEncoder {
			constructor() {
				super();
				window.__flowTextEncoderConstructionCount =
					(window.__flowTextEncoderConstructionCount ?? 0) + 1;
			}
		}
		Object.defineProperty(window, "TextEncoder", {
			configurable: true,
			value: CountingTextEncoder,
		});
	});
	await openFlow(page, "Max Flow");
	const workspace = activeFlowWorkspace(page);
	const baseline = await page.evaluate(
		() => window.__flowTextEncoderConstructionCount ?? -1,
	);
	expect(baseline).toBeGreaterThanOrEqual(1);

	for (const view of ["Residual", "Both", "Original"]) {
		await workspace.getByRole("button", { name: view, exact: true }).click();
	}
	expect(
		await page.evaluate(() => window.__flowTextEncoderConstructionCount ?? -1),
	).toBe(baseline);

	const editor = await revealFlowScenarioEditor(page);
	await editor.fill(`${await editor.inputValue()}\n`);
	await expect
		.poll(() =>
			page.evaluate(() => window.__flowTextEncoderConstructionCount ?? -1),
		)
		.toBeGreaterThan(baseline);
});

test("dense NETGEN uses progressive edge disclosure and view-specific layers", async ({
	page,
}) => {
	test.setTimeout(90_000);
	await openFlow(page, "Min-Cost Flow");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	const dialog = page.getByRole("dialog", {
		name: "Generate Min-Cost Flow graph",
	});
	await selectGeneratorFamily(dialog, "netgen-skeleton");
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });

	const graph = page.getByRole("img", { name: "Validated flow network" });
	await expect(page.locator(".flow-lod-level")).toHaveText("Structure");
	await expect(graph.locator(".flow-original-edge")).toHaveCount(80);
	await expect(
		graph.locator('.flow-original-edge[data-edge-detail="context"]'),
	).toHaveCount(80);
	await expect(
		graph.locator('.flow-original-edge [data-flow-channel="capacity"]'),
	).toHaveCount(80);
	await expect(page.locator(".flow-edge-count")).toHaveText(
		"80/80 edges shown",
	);
	expect(
		await graph
			.locator(
				'.flow-original-edge.flow-edge-context [data-flow-channel="capacity"]',
			)
			.first()
			.evaluate((element) => Number(getComputedStyle(element).opacity)),
	).toBeGreaterThanOrEqual(0.4);
	await expect(
		graph.locator('.flow-original-edge [data-flow-channel="cost"]'),
	).toHaveCount(0);
	await expect(
		graph.locator('.flow-original-edge [data-flow-channel="flow"]'),
	).toHaveCount(0);
	await expect(graph.locator("[data-edge-label-for]")).toHaveCount(0);
	await expect(graph.locator(".flow-residual-arc")).toHaveCount(0);

	const selected = graph.locator('.flow-original-edge[data-edge-id="e000027"]');
	const stablePathBefore = await selected
		.locator('[data-flow-channel="capacity"]')
		.getAttribute("d");
	await selectNavigatorResultByKeyboard(page, "edge", "e000027");
	await expect(selected).toHaveAttribute("data-edge-detail", "expanded");
	await expect(selected.locator('[data-flow-channel="cost"]')).toHaveCount(1);
	await expect(selected.locator('[data-flow-channel="flow"]')).toHaveCount(1);
	const selectedLabel = graph.locator('[data-edge-label-for="e000027"]');
	await expect(selectedLabel).toHaveCount(1);
	await expect(selectedLabel.locator(".flow-edge-label-leader")).toHaveCount(1);
	await expect(selectedLabel.locator(".flow-edge-label-anchor")).toHaveCount(1);
	expect(
		await graph
			.locator("defs .flow-arrow-capacity")
			.evaluate((element) =>
				Number.parseFloat(getComputedStyle(element).strokeWidth),
			),
	).toBeGreaterThanOrEqual(2);
	await expect(
		selected.locator('[data-flow-channel="capacity"]'),
	).toHaveAttribute("d", stablePathBefore ?? "");
	await selectNavigatorResultByKeyboard(page, "edge", "e000028");
	await expect(selected).toHaveAttribute("data-edge-detail", "context");
	await expect(
		selected.locator('[data-flow-channel="capacity"]'),
	).toHaveAttribute("d", stablePathBefore ?? "");

	await page.getByRole("button", { name: "Residual", exact: true }).click();
	await expect(graph.locator(".flow-original-edge")).toHaveCount(0);
	await expect(graph.locator(".flow-residual-arc")).toHaveCount(81);
	await expect(
		graph.locator('.flow-residual-arc[data-edge-detail="expanded"]'),
	).toHaveCount(2);

	await page.getByRole("button", { name: "Both", exact: true }).click();
	await expect(graph.locator(".flow-original-edge")).toHaveCount(80);
	await expect(graph.locator(".flow-residual-arc")).toHaveCount(2);

	await computeTrace(page);
	await selectMicroSteps(page);
	let sawMeasuredSourceAction = false;
	let sawLocalSourceFocus = false;
	// Every Detail click is a real solver-published source boundary. Only the
	// identities owned by the previous or current event may change appearance.
	for (let step = 0; step < 24; step += 1) {
		const before = await stableGraphSnapshot(graph);
		const previouslyTouched = await eventTouchProjection(graph);
		const previouslyChanged = await eventChangeProjection(graph);
		const beforeReadout = await page
			.getByTestId("flow-timeline-readout")
			.textContent();
		const next = page.getByRole("button", { name: "Next step" });
		if (await next.isDisabled()) break;
		await next.click();
		await expect(page.getByTestId("flow-timeline-readout")).not.toHaveText(
			beforeReadout ?? "",
		);
		const touched = await eventTouchProjection(graph);
		const changed = await eventChangeProjection(graph);
		const after = await stableGraphSnapshot(graph);
		expectUntouchedEntitiesStable(
			before,
			after,
			new Set([
				...previouslyTouched.edges,
				...previouslyChanged.edges,
				...touched.edges,
				...changed.edges,
			]),
			new Set([
				...previouslyTouched.nodes,
				...previouslyChanged.nodes,
				...touched.nodes,
				...changed.nodes,
			]),
		);
		const evidence = page.getByTestId("flow-step-evidence");
		await expect(evidence).toHaveAttribute(
			"data-evidence-kind",
			"source-event",
		);
		const traceValue = page
			.getByLabel("Flow scene inspector")
			.locator("dd[data-trace-catalog-id]");
		await expect(traceValue).toHaveCount(1);
		const catalogId = await traceValue.getAttribute("data-trace-catalog-id");
		expect(catalogId?.endsWith(".primary-work-unit")).toBe(false);
		expect(catalogId?.endsWith(".work-observation")).toBe(false);
		sawLocalSourceFocus ||= touched.edges.length + touched.nodes.length > 0;
		await expect(graph.locator(".flow-work-observation")).toHaveCount(0);
		const workDelta =
			(await page
				.getByLabel("Flow scene inspector")
				.locator("dt", { hasText: /^Work delta$/u })
				.locator("..")
				.textContent()) ?? "";
		if (/\+[1-9][0-9]*/u.test(workDelta)) sawMeasuredSourceAction = true;
	}
	expect(sawMeasuredSourceAction).toBe(true);
	expect(sawLocalSourceFocus).toBe(true);

	// Operation playback skips finer source boundaries and reaches the next
	// meaningful graph mutation without weakening Detail granularity.
	await page.getByRole("button", { name: "First event" }).click();
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("operation");
	let sawChangedStructureBoundary = false;
	for (let step = 0; step < 24 && !sawChangedStructureBoundary; step += 1) {
		const before = await stableGraphSnapshot(graph);
		const previouslyTouched = await eventTouchProjection(graph);
		const previouslyChanged = await eventChangeProjection(graph);
		const beforeReadout = await page
			.getByTestId("flow-timeline-readout")
			.textContent();
		const next = page.getByRole("button", { name: "Next step" });
		if (await next.isDisabled()) break;
		await next.click();
		await expect(page.getByTestId("flow-timeline-readout")).not.toHaveText(
			beforeReadout ?? "",
		);
		const touched = await eventTouchProjection(graph);
		const changed = await eventChangeProjection(graph);
		expectUntouchedEntitiesStable(
			before,
			await stableGraphSnapshot(graph),
			new Set([
				...previouslyTouched.edges,
				...previouslyChanged.edges,
				...touched.edges,
				...changed.edges,
			]),
			new Set([
				...previouslyTouched.nodes,
				...previouslyChanged.nodes,
				...touched.nodes,
				...changed.nodes,
			]),
		);
		sawChangedStructureBoundary =
			changed.edges.length + changed.nodes.length > 0;
	}
	expect(sawChangedStructureBoundary).toBe(true);
});

test("solver-published Detail steps move local graph focus without touching unrelated entities", async ({
	page,
}) => {
	test.slow();
	test.setTimeout(120_000);
	await page.setViewportSize({ width: 1440, height: 960 });
	await openFlow(page, "Max Flow");
	const manifest = JSON.parse(
		readFileSync(
			new URL("../../fixtures/flow-representative-audit.json", import.meta.url),
			"utf8",
		),
	) as Readonly<{
		cases: readonly Readonly<{
			algorithm_id: string;
			label: string;
			scenario: unknown;
		}>[];
	}>;
	const representative = manifest.cases.find(
		(candidate) =>
			candidate.algorithm_id === "ford-fulkerson" &&
			candidate.label === "canonical",
	);
	if (representative === undefined) {
		throw new Error("ford-fulkerson has no canonical representative");
	}
	const editor = await revealFlowScenarioEditor(page);
	await editor.fill(JSON.stringify(representative.scenario, null, 2));
	await activeFlowWorkspace(page)
		.getByRole("button", { name: "Load", exact: true })
		.click();
	await expect(activeFlowWorkspace(page).locator(".flow-status")).toHaveText(
		"Validated",
	);
	await closeFlowInputPanel(page);
	await computeTrace(page);
	await page.setViewportSize({ width: 390, height: 844 });
	await selectMicroSteps(page);
	const graph = activeFlowWorkspace(page).getByRole("img", {
		name: "Validated flow network",
	});
	const workspace = activeFlowWorkspace(page);
	const next = workspace.getByRole("button", { name: "Next step" });
	const readout = workspace.getByTestId("flow-timeline-readout");
	const localFocuses = new Set<string>();
	for (let step = 0; step < 32 && localFocuses.size < 3; step += 1) {
		const before = await stableGraphSnapshot(graph);
		const previous = await eventTouchProjection(graph);
		const previousChanged = await eventChangeProjection(graph);
		const beforeReadout = (await readout.textContent()) ?? "";
		await expect(next).toBeEnabled();
		await next.click();
		await expect(readout).not.toHaveText(beforeReadout);
		await expect(workspace.getByTestId("flow-step-evidence")).toHaveAttribute(
			"data-evidence-kind",
			"source-event",
		);
		const catalogId = await workspace
			.getByLabel("Flow scene inspector")
			.locator("dd[data-trace-catalog-id]")
			.getAttribute("data-trace-catalog-id");
		expect(catalogId?.endsWith(".work-observation")).toBe(false);
		expect(catalogId?.endsWith(".primary-work-unit")).toBe(false);
		const touched = await eventTouchProjection(graph);
		const changed = await eventChangeProjection(graph);
		expectUntouchedEntitiesStable(
			before,
			await stableGraphSnapshot(graph),
			new Set([
				...previous.edges,
				...previousChanged.edges,
				...touched.edges,
				...changed.edges,
			]),
			new Set([
				...previous.nodes,
				...previousChanged.nodes,
				...touched.nodes,
				...changed.nodes,
			]),
		);
		if (touched.edges.length + touched.nodes.length > 0) {
			const focus = `e:${touched.edges.join(",")}|n:${touched.nodes.join(",")}`;
			localFocuses.add(focus);
			expect(touched.edges.length).toBeLessThan(
				await graph.locator(".flow-original-edge").count(),
			);
			expect(touched.nodes.length).toBeLessThan(
				await graph.locator("[data-node-id]").count(),
			);
		}
	}
	expect(localFocuses.size).toBeGreaterThanOrEqual(3);
	await expect(graph.locator(".flow-work-observation")).toHaveCount(0);
});

test("complete DAG keeps every edge visible or count-preserving in Overview", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	let dialog = page.getByRole("dialog", {
		name: "Generate Max Flow graph",
	});
	await selectGeneratorFamily(dialog, "complete-dag");
	await dialog.getByText("Presets & generator notes", { exact: true }).click();
	await dialog.getByRole("button", { name: "Readable trace" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });

	const workspace = activeFlowWorkspace(page);
	await page.setViewportSize({ width: 390, height: 844 });
	await expect(workspace.locator(".flow-generation-details")).toBeVisible();
	expect(
		await workspace.locator(".canvas-meta").evaluate((meta) => {
			const boxes = [...meta.children]
				.filter((child) => {
					const style = getComputedStyle(child);
					return style.display !== "none" && style.visibility !== "hidden";
				})
				.map((child) => child.getBoundingClientRect())
				.filter((box) => box.width > 0 && box.height > 0);
			return boxes.every((left, index) =>
				boxes
					.slice(index + 1)
					.every(
						(right) =>
							left.right <= right.left + 0.5 ||
							right.right <= left.left + 0.5 ||
							left.bottom <= right.top + 0.5 ||
							right.bottom <= left.top + 0.5,
					),
			);
		}),
	).toBe(true);
	await page.setViewportSize({ width: 1440, height: 900 });
	let graph = workspace.getByRole("img", {
		name: "Validated flow network",
	});
	await expect(workspace.locator(".flow-lod-level")).toHaveText("Structure");
	await expect(graph.locator(".flow-original-edge")).toHaveCount(28);
	await expect(
		graph.locator('.flow-original-edge [data-flow-channel="capacity"]'),
	).toHaveCount(28);
	await expect(workspace.locator(".flow-edge-count")).toHaveText(
		"28/28 edges shown",
	);
	const nodeRows = await graph.locator("[data-node-id]").evaluateAll((nodes) =>
		nodes.map((node) => {
			const matrix = (node as SVGGElement).transform.baseVal.consolidate()
				?.matrix;
			return matrix?.f ?? Number.NaN;
		}),
	);
	expect(
		new Set(nodeRows.map((y) => y.toFixed(2))).size,
	).toBeGreaterThanOrEqual(4);
	expect(Math.max(...nodeRows) - Math.min(...nodeRows)).toBeGreaterThan(200);
	expect(
		await graph
			.locator(
				'.flow-original-edge.flow-edge-context [data-flow-channel="capacity"]',
			)
			.first()
			.evaluate((element) => Number(getComputedStyle(element).opacity)),
	).toBeGreaterThanOrEqual(0.4);

	await workspace
		.getByRole("button", { name: "Generate", exact: true })
		.click();
	dialog = page.getByRole("dialog", {
		name: "Generate Max Flow graph",
	});
	await selectGeneratorFamily(dialog, "complete-dag");
	await dialog.getByText("Presets & generator notes", { exact: true }).click();
	await dialog.getByRole("button", { name: "Standard comparison" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });

	await expect(workspace.locator(".flow-lod-level")).toHaveText("Overview");
	await expect(workspace.locator(".flow-edge-count")).toHaveText(
		/^780 edges → [1-9][0-9]* bundles$/u,
	);
	graph = workspace.getByRole("img", {
		name: "Validated flow-network overview",
	});
	const representedEdges = await graph
		.locator('[data-aggregate-kind="original-edge"][data-aggregate-count]')
		.evaluateAll((bundles) =>
			bundles.reduce(
				(total, bundle) =>
					total + Number(bundle.getAttribute("data-aggregate-count") ?? "0"),
				0,
			),
		);
	expect(representedEdges).toBe(780);
});

test("dense Max Flow NETGEN uses a readable semantic Structure layout", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	const dialog = page.getByRole("dialog", {
		name: "Generate Max Flow graph",
	});
	await selectGeneratorFamily(dialog, "netgen-skeleton");
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });

	const workspace = activeFlowWorkspace(page);
	await expect(workspace.locator(".flow-lod-level")).toHaveText("Structure");
	const graph = workspace.getByRole("img", {
		name: "Validated flow network",
	});
	await expect(graph.locator("[data-node-id]")).toHaveCount(30);
	const positions = await graph
		.locator("[data-node-id]")
		.evaluateAll((nodes) =>
			nodes.map((node) => node.getAttribute("transform")),
		);
	expect(new Set(positions).size).toBe(positions.length);
	await workspace.getByRole("button", { name: "Run trace" }).click();
	await expect(workspace.getByTestId("flow-timeline-readout")).toHaveText(
		/^Raw [1-9][0-9]* \/ [1-9][0-9]*$/,
		{ timeout: 60_000 },
	);
	await expect(graph.locator('[data-event-touch="true"]')).not.toHaveCount(0);
});

test("single-lane NETGEN keeps the practical 59-node boundary individually readable", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	const dialog = page.getByRole("dialog", {
		name: "Generate Max Flow graph",
	});
	await selectGeneratorFamily(dialog, "netgen-skeleton");
	await dialog.locator('[data-netgen-preset="custom"]').click();
	await dialog.getByLabel("Nodes N").fill("59");
	await dialog.getByLabel("Sources S").fill("1");
	await dialog.getByLabel("Sinks T").fill("1");
	await dialog.getByLabel("Edges M").fill("70");
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });

	const workspace = activeFlowWorkspace(page);
	await expect(workspace.locator(".flow-lod-level")).toHaveText("Structure");
	const graph = workspace.getByRole("img", {
		name: "Validated flow network",
	});
	const nodes = graph.locator("[data-node-id]");
	await expect(nodes).toHaveCount(59);
	const minimumVisibleGap = await nodes.evaluateAll((elements) => {
		const points = elements.map((element) => {
			const group = element as SVGGElement;
			const matrix = group.transform.baseVal.consolidate()?.matrix;
			const radius = Number(
				group.querySelector("circle.flow-node")?.getAttribute("r") ?? "0",
			);
			return {
				x: matrix?.e ?? Number.NaN,
				y: matrix?.f ?? Number.NaN,
				radius,
			};
		});
		let minimum = Number.POSITIVE_INFINITY;
		for (const [index, point] of points.entries()) {
			for (const other of points.slice(index + 1)) {
				minimum = Math.min(
					minimum,
					Math.hypot(point.x - other.x, point.y - other.y) -
						point.radius -
						other.radius,
				);
			}
		}
		return minimum;
	});
	expect(minimumVisibleGap).toBeGreaterThanOrEqual(3.9);
	await workspace.getByRole("button", { name: "Run trace" }).click();
	const readout = workspace.getByTestId("flow-timeline-readout");
	await expect(readout).toHaveText("Raw 123 / 123", { timeout: 60_000 });
	await expect(
		workspace.getByRole("button", { name: "Next step" }),
	).toBeDisabled();
	await expect(
		workspace.getByRole("button", { name: "Last event" }),
	).toBeDisabled();
	await expect(workspace.locator(".flow-event-action")).toHaveText(
		"Optimality certificate verified",
	);
	await expect(
		workspace.getByText("Max flow 100", { exact: true }),
	).toBeVisible();
	await expect(workspace.getByText("cut = 100", { exact: true })).toBeVisible();
});

test("generation applies immediately and produces a useful default trace", async ({
	page,
}, testInfo) => {
	test.setTimeout(120_000);
	await openFlow(page, "Max Flow");
	await generateReadableDefault(page);
	const graph = page.getByRole("img", { name: "Validated flow network" });
	await expect(graph.locator(".flow-original-edge")).toHaveCount(40);
	await expect(graph.locator("[data-node-id]")).toHaveCount(22);
	const firstLayeredNode = graph.locator('[data-node-id="l000n0000"]');
	await expect(firstLayeredNode.locator(".flow-node-label")).toHaveText("L0·0");
	await expect(firstLayeredNode.locator("title")).toContainText("l000n0000");
	const clippedNodes = await graph.evaluate((svg) => {
		const viewport = {
			left: 0,
			right: window.innerWidth,
			top: 0,
			bottom: window.innerHeight,
		};
		for (let element: Element | null = svg; element !== null; ) {
			const style = getComputedStyle(element);
			const clipsX =
				element === svg || /^(auto|clip|hidden|scroll)$/.test(style.overflowX);
			const clipsY =
				element === svg || /^(auto|clip|hidden|scroll)$/.test(style.overflowY);
			if (clipsX || clipsY) {
				const bounds = element.getBoundingClientRect();
				if (clipsX) {
					viewport.left = Math.max(viewport.left, bounds.left);
					viewport.right = Math.min(viewport.right, bounds.right);
				}
				if (clipsY) {
					viewport.top = Math.max(viewport.top, bounds.top);
					viewport.bottom = Math.min(viewport.bottom, bounds.bottom);
				}
			}
			element = element.parentElement;
		}
		return [...svg.querySelectorAll<SVGGElement>("[data-node-id]")].flatMap(
			(group) => {
				const body = group.querySelector<SVGCircleElement>(
					":scope > .flow-node",
				);
				if (body === null) return [];
				const bounds = body.getBoundingClientRect();
				const clipped =
					bounds.left < viewport.left - 0.5 ||
					bounds.right > viewport.right + 0.5 ||
					bounds.top < viewport.top - 0.5 ||
					bounds.bottom > viewport.bottom + 0.5;
				return clipped
					? [
							{
								id: group.getAttribute("data-node-id") ?? "<missing>",
								transform: group.getAttribute("transform"),
								viewBox: svg.getAttribute("viewBox"),
								ancestors: (() => {
									const entries = [];
									for (
										let element: Element | null = svg;
										element !== null && entries.length < 6;
										element = element.parentElement
									) {
										const rect = element.getBoundingClientRect();
										entries.push({
											name: `${element.tagName}.${element.getAttribute("class") ?? ""}`,
											left: Math.round(rect.left),
											right: Math.round(rect.right),
											width: Math.round(rect.width),
											overflowX: getComputedStyle(element).overflowX,
										});
									}
									return entries;
								})(),
								bounds: {
									left: Math.round(bounds.left),
									right: Math.round(bounds.right),
									top: Math.round(bounds.top),
									bottom: Math.round(bounds.bottom),
								},
								viewport: {
									left: Math.round(viewport.left),
									right: Math.round(viewport.right),
									top: Math.round(viewport.top),
									bottom: Math.round(viewport.bottom),
								},
							},
						]
					: [];
			},
		);
	});
	expect(clippedNodes).toEqual([]);
	if (hasDarwinPixelBaseline(testInfo)) {
		await expect(page.locator(".flow-shell")).toHaveScreenshot(
			"max-flow-generated-default-1440.png",
			{ animations: "disabled", maxDiffPixels: 400 },
		);
	}
	await selectFlowAlgorithm(page, "dinic");
	const workspace = activeFlowWorkspace(page);
	await workspace.getByRole("button", { name: "Run trace" }).click();
	const speed = workspace.getByRole("combobox", { name: "Playback speed" });
	const granularity = workspace.getByRole("combobox", {
		name: /Playback granularity/,
	});
	await expect(workspace.locator(".flow-status")).toHaveText("Tracing");
	await expect(speed).toBeEnabled();
	await expect(granularity).toBeEnabled();
	await speed.selectOption("32");
	await granularity.selectOption("micro");
	await expect(speed).toHaveValue("32");
	await expect(granularity).toHaveValue("micro");
	await expect(workspace.getByTestId("flow-timeline-readout")).toHaveText(
		/^Raw ([1-9][0-9]*) \/ \1$/,
		{ timeout: 60_000 },
	);
	await expect(
		workspace.getByRole("button", { name: "Next step" }),
	).toBeDisabled();
	await expect(workspace.locator(".flow-event-action")).toHaveText(
		"Optimality certificate verified",
	);
	await workspace.getByRole("button", { name: "First event" }).click();
	await expect(page.getByTestId("flow-timeline-readout")).toHaveText(
		/^Raw 0 \/ ([5-9][0-9]|[1-9][0-9]{2,})$/,
	);
	await selectMicroSteps(page);
	await stepUntilTraceCatalog(page, "dinic.inspect-residual-arc", 240);
	await expect(workspace.getByTestId("flow-step-evidence")).toHaveAttribute(
		"data-evidence-kind",
		"source-event",
	);
	const touched = await eventTouchProjection(graph);
	expect(touched.edges.length + touched.nodes.length).toBeGreaterThan(0);
	expect(touched.edges.length).toBeLessThan(
		await graph.locator(".flow-original-edge").count(),
	);
	expect(touched.nodes.length).toBeLessThan(
		await graph.locator("[data-node-id]").count(),
	);
	await expect(graph.locator(".flow-work-observation")).toHaveCount(0);
});

test("retained flow workspaces keep IDs and accessible references document-unique", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	await page
		.getByRole("button", { name: "Min-Cost Flow", exact: true })
		.click();
	const maxWorkspace = flowWorkspace(page, "Max Flow");
	const minCostWorkspace = flowWorkspace(page, "Min-Cost Flow");
	await expect(maxWorkspace).toBeHidden();
	await expect(minCostWorkspace).toBeVisible();
	await expect(maxWorkspace.locator("svg[role='img']")).toHaveCount(1);
	await expect(
		minCostWorkspace.getByRole("img", { name: "Validated flow network" }),
	).toBeVisible();

	const duplicateFlowIds = await page.evaluate(() => {
		const counts = new Map<string, number>();
		for (const element of document.querySelectorAll<HTMLElement>(
			"[data-workspace-id='max-flow'] [id], [data-workspace-id='min-cost-flow'] [id]",
		)) {
			counts.set(element.id, (counts.get(element.id) ?? 0) + 1);
		}
		return [...counts]
			.filter(([, count]) => count !== 1)
			.sort(([left], [right]) => left.localeCompare(right));
	});
	expect(duplicateFlowIds).toEqual([]);

	const visibleLabels = minCostWorkspace.locator(
		".flow-entity-navigator label[for]:visible",
	);
	const visibleLabelCount = await visibleLabels.count();
	expect(visibleLabelCount).toBeGreaterThan(0);
	for (let index = 0; index < visibleLabelCount; index += 1) {
		const label = visibleLabels.nth(index);
		const targetId = await label.getAttribute("for");
		expect(targetId).not.toBeNull();
		const target = minCostWorkspace.locator(
			`[id=${JSON.stringify(targetId ?? "")}]`,
		);
		await expect(target).toHaveCount(1);
		await expect(target).toBeVisible();
		expect(
			await target.evaluate((element) =>
				element.matches("button, input, select, textarea"),
			),
		).toBe(true);
		await label.click();
		expect(
			await target.evaluate((element) => document.activeElement === element),
		).toBe(true);
	}

	const graphReferences = await minCostWorkspace
		.getByRole("img", { name: "Validated flow network" })
		.evaluate((graph) => {
			const workspace = graph.closest<HTMLElement>("[data-workspace-id]");
			if (workspace === null) throw new Error("flow graph has no workspace");
			return ["aria-labelledby", "aria-describedby"].flatMap((attribute) =>
				(graph.getAttribute(attribute) ?? "")
					.split(/\s+/u)
					.filter((id) => id.length > 0)
					.map((id) => {
						const matches = [
							...document.querySelectorAll<HTMLElement>("[id]"),
						].filter((element) => element.id === id);
						return {
							attribute,
							id,
							globalCount: matches.length,
							insideActiveWorkspace:
								matches.length === 1 && workspace.contains(matches[0] ?? null),
						};
					}),
			);
		});
	expect(graphReferences).toHaveLength(2);
	for (const reference of graphReferences) {
		expect(
			reference.globalCount,
			`${reference.attribute}:${reference.id}`,
		).toBe(1);
		expect(
			reference.insideActiveWorkspace,
			`${reference.attribute}:${reference.id}`,
		).toBe(true);
	}

	const maxDescription = maxWorkspace.locator("svg[role='img'] > desc");
	await expect(maxDescription).toContainText(
		"Outer edge width shows capacity; inner width shows current flow; arrow markers show edge direction.",
	);
	await expect(maxDescription).toContainText(
		"The minimum cut is highlighted after optimization.",
	);
	await expect(maxDescription).not.toContainText("signed unit cost");

	const minCostDescription = minCostWorkspace.locator("svg[role='img'] > desc");
	await expect(minCostDescription).toContainText(
		"Outer edge width shows capacity; inner width shows current flow; arrow markers show edge direction.",
	);
	await expect(minCostDescription).toContainText(
		"signed unit cost, continuous intensity shows absolute cost magnitude",
	);
	await expect(minCostDescription).not.toContainText("minimum cut");
});

test("complete problem-specific generator and playback preferences survive workspace switches and reloads", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	await page.getByRole("button", { name: "Both", exact: true }).click();
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("micro");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	let dialog = page.getByRole("dialog", { name: "Generate Max Flow graph" });
	await expect(
		dialog.getByRole("button", { name: "New random seed" }),
	).toHaveCount(0);
	await selectGeneratorFamily(dialog, "grid-2d");
	await dialog.getByLabel("Rows", { exact: true }).fill("4");
	await dialog.getByLabel("Columns", { exact: true }).fill("7");
	await dialog.getByLabel("Diagonal edges", { exact: true }).check();
	await dialog.getByLabel("Seed", { exact: true }).fill("123456");
	await dialog.getByRole("button", { name: "Change shape" }).click();
	await dialog
		.getByRole("combobox", { name: "Category", exact: true })
		.selectOption("structural");
	await dialog.getByRole("button", { name: "Cancel", exact: true }).click();

	await page
		.getByRole("button", { name: "Min-Cost Flow", exact: true })
		.click();
	await page.getByRole("button", { name: "Residual", exact: true }).click();
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("phase");
	await page.reload();
	await page
		.getByRole("button", { name: "Min-Cost Flow", exact: true })
		.click();
	await expect(
		page.getByRole("button", { name: "Residual", exact: true }),
	).toHaveAttribute("aria-pressed", "true");
	await expect(
		page.getByRole("combobox", { name: "Playback granularity" }),
	).toHaveValue("phase");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	dialog = page.getByRole("dialog", { name: "Generate Min-Cost Flow graph" });
	await selectGeneratorFamily(dialog, "transportation-table");
	await dialog
		.getByLabel("Transportation table shape", { exact: true })
		.selectOption("near-tie");
	await dialog.getByLabel("Origins", { exact: true }).fill("3");
	await dialog.getByLabel("Destinations", { exact: true }).fill("4");
	await dialog.getByLabel("Total shipment B", { exact: true }).fill("24");
	await dialog.getByLabel("Seed", { exact: true }).fill("654321");
	await dialog.getByRole("button", { name: "Change shape" }).click();
	await dialog
		.getByRole("combobox", { name: "Category", exact: true })
		.selectOption("special");
	await dialog.getByRole("button", { name: "Cancel", exact: true }).click();

	await page.getByRole("button", { name: "Max Flow", exact: true }).click();
	await expect(
		page.getByRole("button", { name: "Both", exact: true }),
	).toHaveAttribute("aria-pressed", "true");
	await expect(
		page.getByRole("combobox", { name: "Playback granularity" }),
	).toHaveValue("micro");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	dialog = page.getByRole("dialog", { name: "Generate Max Flow graph" });
	await dialog.getByRole("button", { name: "Change shape" }).click();
	await expect(generatorFamily(dialog, "grid-2d")).toHaveAttribute(
		"aria-pressed",
		"true",
	);
	await expect(dialog.getByLabel("Rows", { exact: true })).toHaveValue("4");
	await expect(dialog.getByLabel("Columns", { exact: true })).toHaveValue("7");
	await expect(
		dialog.getByLabel("Diagonal edges", { exact: true }),
	).toBeChecked();
	await expect(dialog.getByLabel("Seed", { exact: true })).toHaveValue(
		"123456",
	);
	await expect(
		dialog.getByRole("combobox", { name: "Category", exact: true }),
	).toHaveValue("structural");
	await dialog.getByRole("button", { name: "Cancel", exact: true }).click();

	await page.reload();
	await page
		.getByRole("button", { name: "Min-Cost Flow", exact: true })
		.click();
	await expect(
		page.getByRole("button", { name: "Residual", exact: true }),
	).toHaveAttribute("aria-pressed", "true");
	await expect(
		page.getByRole("combobox", { name: "Playback granularity" }),
	).toHaveValue("phase");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	dialog = page.getByRole("dialog", { name: "Generate Min-Cost Flow graph" });
	await dialog.getByRole("button", { name: "Change shape" }).click();
	await expect(generatorFamily(dialog, "transportation-table")).toHaveAttribute(
		"aria-pressed",
		"true",
	);
	await expect(
		dialog.getByLabel("Transportation table shape", { exact: true }),
	).toHaveValue("near-tie");
	await expect(dialog.getByLabel("Origins", { exact: true })).toHaveValue("3");
	await expect(dialog.getByLabel("Destinations", { exact: true })).toHaveValue(
		"4",
	);
	await expect(
		dialog.getByLabel("Total shipment B", { exact: true }),
	).toHaveValue("24");
	await expect(dialog.getByLabel("Seed", { exact: true })).toHaveValue(
		"654321",
	);
	await expect(
		dialog.getByRole("combobox", { name: "Category", exact: true }),
	).toHaveValue("special");
});

test("invalid publication can be corrected without locking the workbench", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	const editor = page.getByRole("textbox", { name: "Flow Scenario JSON" });
	const validScenario = await editor.inputValue();
	await editor.fill("{");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.locator(".flow-message-error")).toBeVisible();
	await expect(
		page.getByRole("button", { name: "Load", exact: true }),
	).toBeEnabled();
	await expect(
		page.getByRole("button", { name: "Generate", exact: true }),
	).toBeEnabled();

	await editor.fill(validScenario);
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.locator(".flow-message-error")).toBeHidden();
	await expect(page.getByText("Validated", { exact: true })).toBeVisible();
});

test("a Flow engine failure is fatal and cannot be retried through stale Worker state", async ({
	page,
}) => {
	await page.addInitScript(() => {
		const NativeWorker = window.Worker;
		class ObservableFlowWorker extends NativeWorker {
			constructor(scriptURL: string | URL, options?: WorkerOptions) {
				super(scriptURL, options);
				window.__flowEngineWorker = this;
				window.__flowEnginePostCount = 0;
				const postMessage = this.postMessage.bind(this);
				this.postMessage = ((message: unknown, transfer?: Transferable[]) => {
					window.__flowEnginePostCount =
						(window.__flowEnginePostCount ?? 0) + 1;
					if (
						typeof message === "object" &&
						message !== null &&
						"generation" in message &&
						typeof message.generation === "number"
					) {
						window.__flowLastEngineGeneration = message.generation;
					}
					postMessage(message, transfer ?? []);
				}) as Worker["postMessage"];
			}
		}
		window.Worker = ObservableFlowWorker;
	});
	await openFlow(page, "Max Flow");
	await page.evaluate(() => {
		const worker = window.__flowEngineWorker;
		const generation = window.__flowLastEngineGeneration;
		if (worker === undefined || generation === undefined) {
			throw new Error("Flow Worker observation was not installed");
		}
		worker.dispatchEvent(
			new MessageEvent("message", {
				data: {
					kind: "error",
					generation,
					requestKind: "next",
					message: "injected Flow runtime invariant failure",
					source: "engine",
				},
			}),
		);
	});

	const workspace = activeFlowWorkspace(page);
	await expect(workspace.locator(".flow-status")).toHaveText("Engine error");
	await expect(
		workspace.getByText("injected Flow runtime invariant failure", {
			exact: true,
		}),
	).toBeVisible();
	for (const name of ["Algorithm", "Generate", "Load", "Run trace"]) {
		await expect(
			workspace.getByRole("button", { name, exact: true }),
		).toBeDisabled();
	}
	await expect(
		workspace.getByRole("button", { name: "Next step" }),
	).toBeDisabled();
	await expect(workspace.getByRole("slider")).toBeDisabled();
	const postsBefore = await page.evaluate(
		() => window.__flowEnginePostCount ?? -1,
	);
	await workspace.evaluate((root) => {
		for (const name of ["Algorithm", "Generate", "Load"]) {
			const button = [...root.querySelectorAll("button")].find(
				(candidate) => candidate.textContent?.trim() === name,
			);
			if (button instanceof HTMLButtonElement) {
				button.disabled = false;
				button.click();
			}
		}
	});
	expect(await page.evaluate(() => window.__flowEnginePostCount ?? -1)).toBe(
		postsBefore,
	);
});

test("a replacement seek resumes after the superseded publication is rejected", async ({
	page,
}) => {
	await page.addInitScript(() => {
		const NativeWorker = window.Worker;
		class FlowSeekReplacementWorker extends NativeWorker {
			constructor(scriptURL: string | URL, options?: WorkerOptions) {
				super(scriptURL, options);
				const add = this.addEventListener.bind(this);
				this.addEventListener = ((
					type: string,
					listener: EventListenerOrEventListenerObject | null,
					options?: AddEventListenerOptions | boolean,
				) => {
					if (listener === null) return;
					if (type !== "message") {
						add(type, listener, options);
						return;
					}
					add(
						type,
						(event: Event) => {
							const deliver = () => {
								if (typeof listener === "function") listener.call(this, event);
								else listener.handleEvent(event);
							};
							const data = (event as MessageEvent<unknown>).data;
							const replacement = window.__flowSeekReplacementTarget;
							if (
								replacement !== undefined &&
								typeof data === "object" &&
								data !== null &&
								"kind" in data &&
								data.kind === "flow-update" &&
								"seekRequestSerial" in data &&
								typeof data.seekRequestSerial === "number"
							) {
								delete window.__flowSeekReplacementTarget;
								deliver();
								const slider = document.querySelector<HTMLInputElement>(
									'[data-workspace-id]:not([hidden]) input[aria-label="Raw trace position"]',
								);
								if (slider === null)
									throw new Error("Flow raw slider is missing");
								slider.disabled = false;
								const setter = Object.getOwnPropertyDescriptor(
									HTMLInputElement.prototype,
									"value",
								)?.set;
								setter?.call(slider, String(replacement));
								slider.dispatchEvent(new Event("input", { bubbles: true }));
								return;
							}
							deliver();
						},
						options,
					);
				}) as Worker["addEventListener"];
			}
		}
		window.Worker = FlowSeekReplacementWorker;
	});
	await openFlow(page, "Max Flow");
	await computeTrace(page);
	await selectMicroSteps(page);
	const workspace = activeFlowWorkspace(page);
	const slider = workspace.getByRole("slider", { name: "Raw trace position" });
	const extent = Number(await slider.getAttribute("max"));
	expect(extent).toBeGreaterThanOrEqual(3);
	await page.evaluate(() => {
		window.__flowSeekReplacementTarget = 2;
	});
	await slider.fill("1");
	await expect(workspace.getByTestId("flow-timeline-readout")).toHaveText(
		`Raw 2 / ${extent}`,
	);
	await expect(workspace.locator(".flow-status")).toHaveText("Validated");
});

test("generator cancellation and failure retain the exact validated flow state", async ({
	page,
}) => {
	await page.addInitScript(() => {
		const NativeWorker = window.Worker;
		class FaultInjectingGeneratorWorker extends NativeWorker {
			constructor(scriptURL: string | URL, options?: WorkerOptions) {
				super(scriptURL, options);
				const postMessage = this.postMessage.bind(this);
				this.postMessage = ((message: unknown, transfer?: Transferable[]) => {
					const request =
						typeof message === "object" && message !== null
							? (message as Record<string, unknown>)
							: undefined;
					const mode = window.__flowGeneratorFaultMode;
					if (
						mode === "reject-create" &&
						request?.kind === "create" &&
						typeof request.generation === "number" &&
						typeof request.scenario === "string" &&
						request.scenario.includes('"generator_provenance"')
					) {
						queueMicrotask(() => {
							this.dispatchEvent(
								new MessageEvent("message", {
									data: {
										kind: "error",
										generation: request.generation,
										requestKind: "create",
										message: "injected generated session rejection",
										source: "input",
									},
								}),
							);
						});
						return;
					}
					if (
						(mode === "error" || mode === "hold") &&
						request?.kind === "generate" &&
						typeof request.jobId === "number"
					) {
						if (mode === "error") {
							queueMicrotask(() => {
								this.dispatchEvent(
									new MessageEvent("message", {
										data: {
											kind: "error",
											jobId: request.jobId,
											message: "injected flow generator failure",
										},
									}),
								);
							});
						}
						return;
					}
					postMessage(message, transfer ?? []);
				}) as Worker["postMessage"];
			}
		}
		window.Worker = FaultInjectingGeneratorWorker;
	});

	await openFlow(page, "Min-Cost Flow");
	await computeTrace(page);
	await selectMicroSteps(page);
	await stepUntilCaption(page, "Relax residual edge", 180);
	await selectNavigatorResultByKeyboard(page, "edge", "sa");
	const before = await retainedFlowState(page);
	expect(before.cursor).toMatch(/^Raw [1-9][0-9]* \/ [1-9][0-9]*$/);
	expect(before.selection).toEqual([":sa:"]);

	await page.evaluate(() => {
		window.__flowGeneratorFaultMode = "hold";
	});
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	let dialog = page.getByRole("dialog", {
		name: "Generate Min-Cost Flow graph",
	});
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await dialog.getByRole("button", { name: "Cancel generation" }).click();
	await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
	await expect(dialog).toBeHidden();
	expect(await retainedFlowState(page)).toEqual(before);

	await page.evaluate(() => {
		window.__flowGeneratorFaultMode = "error";
	});
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	dialog = page.getByRole("dialog", { name: "Generate Min-Cost Flow graph" });
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(
		dialog.getByText("injected flow generator failure", { exact: true }),
	).toBeVisible();
	await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
	await expect(dialog).toBeHidden();
	expect(await retainedFlowState(page)).toEqual(before);

	await page.evaluate(() => {
		window.__flowGeneratorFaultMode = "reject-create";
	});
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	dialog = page.getByRole("dialog", { name: "Generate Min-Cost Flow graph" });
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(
		dialog.getByText("injected generated session rejection", { exact: true }),
	).toBeVisible({ timeout: 30_000 });
	await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
	await expect(dialog).toBeHidden();
	expect(await retainedFlowState(page)).toEqual(before);
});

test("Max Flow Detail steps expose search, bottleneck, and commit separately", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	await computeTrace(page);
	await selectMicroSteps(page);
	const captions = await collectCaptions(page);
	expect(
		captions.some((caption) => caption.startsWith("Inspect residual edge")),
	).toBe(true);
	expect(
		captions.some((caption) => caption.startsWith("Build path prefix")),
	).toBe(true);
	expect(captions.some((caption) => caption.startsWith("Bottleneck ="))).toBe(
		true,
	);
	expect(captions.some((caption) => caption.startsWith("Commit +"))).toBe(true);
	expect(
		captions.findIndex((caption) => caption.startsWith("Bottleneck =")),
	).toBeLessThan(
		captions.findIndex((caption) => caption.startsWith("Commit +")),
	);
});

test("Min-Cost Flow Detail steps expose relaxations before augmentation", async ({
	page,
}) => {
	await openFlow(page, "Min-Cost Flow");
	await computeTrace(page);
	await selectMicroSteps(page);
	const captions = await collectCaptions(page, 180);
	expect(
		captions.some((caption) => caption.startsWith("Relax residual edge")),
	).toBe(true);
	expect(
		captions.some((caption) => caption.startsWith("Build path prefix")),
	).toBe(true);
	expect(captions.some((caption) => caption.startsWith("Bottleneck ="))).toBe(
		true,
	);
	expect(captions.some((caption) => caption.startsWith("Commit +"))).toBe(true);
});

test("timeline modes and global keyboard controls share one navigation contract", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	await computeTrace(page);
	await selectMicroSteps(page);
	const readout = page.getByTestId("flow-timeline-readout");
	await page.keyboard.press("ArrowRight");
	await expect(readout).not.toHaveText(/^Raw 0 \/ /);
	const afterGlobalStep = await readout.textContent();

	const editor = page.getByRole("textbox", { name: "Flow Scenario JSON" });
	await editor.focus();
	await page.keyboard.press("ArrowRight");
	await expect(readout).toHaveText(afterGlobalStep ?? "");

	await page.getByRole("button", { name: "First event" }).click();
	await expect(readout).toHaveText(/^Raw 0 \/ /);
	await page.evaluate(() => {
		if (document.activeElement instanceof HTMLElement) {
			document.activeElement.blur();
		}
		return new Promise<void>((resolve) => {
			requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
		});
	});
	await page.keyboard.press("End");
	await expect(readout).not.toHaveText(/^Raw 0 \/ /);
	await page.keyboard.press("Home");
	await expect(readout).toHaveText(/^Raw 0 \/ /);

	await stepUntilCaption(page, "Inspect residual edge");
	const rawBeforeModeChange = await readout.textContent();
	const graphBeforeModeChange = await stableGraphSnapshot(
		page.getByRole("img", { name: "Validated flow network" }),
	);
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("operation");
	await expect(readout).toHaveText(rawBeforeModeChange ?? "");
	await expect(page.getByTestId("flow-timeline-visible-readout")).toHaveText(
		/^Raw [1-9][0-9]* \/ [1-9][0-9]* · next Operation$/,
	);
	expect(
		await stableGraphSnapshot(
			page.getByRole("img", { name: "Validated flow network" }),
		),
	).toEqual(graphBeforeModeChange);
	const rawBetweenOperations = page.getByRole("slider", {
		name: "Raw trace position before the next Operation boundary",
	});
	await expect(rawBetweenOperations).toBeVisible();
	const rawBeforeExactSeek = Number(
		(await readout.textContent())?.match(/^Raw ([0-9]+) \/ /)?.[1],
	);
	expect(rawBeforeExactSeek).toBeGreaterThan(0);
	await rawBetweenOperations.fill(String(rawBeforeExactSeek + 1));
	await expect(readout).toHaveText(
		new RegExp(`^Raw ${rawBeforeExactSeek + 1} / `),
	);
	const visibleTimeline = page.getByRole("slider", {
		name: "Visible trace position",
	});
	await expect(visibleTimeline).toBeHidden();
	await page.getByRole("button", { name: "Next step" }).click();
	await expect(page.getByTestId("flow-timeline-visible-readout")).toHaveText(
		/^Operation [1-9][0-9]* \/ \?$/,
	);
	await expect(visibleTimeline).toBeVisible();
	await expect(visibleTimeline).toBeEnabled();
	await expect(visibleTimeline).toHaveAttribute(
		"aria-valuetext",
		/^Operation [1-9][0-9]* of \? · raw [1-9][0-9]* of [1-9][0-9]*$/,
	);
	await visibleTimeline.fill("1");
	await expect(readout).not.toHaveText(/^Raw 0 \/ /);
	const rawExtentBeforeTerminalJump = Number(
		(await readout.textContent())?.match(/\/ ([0-9]+)$/)?.[1],
	);
	await page.getByRole("button", { name: "Last event" }).click();
	await expect(readout).toHaveText(
		`Raw ${rawExtentBeforeTerminalJump} / ${rawExtentBeforeTerminalJump}`,
	);
	await expect(page.getByTestId("flow-timeline-visible-readout")).toHaveText(
		/^Operation \? \/ \? · raw [1-9][0-9]* \/ [1-9][0-9]*$/,
	);
	await expect(
		page.getByRole("slider", {
			name: "Raw trace position while the Operation ordinal is unknown",
		}),
	).toBeVisible();

	const granularity = page.getByRole("combobox", {
		name: "Playback granularity",
	});
	await granularity.selectOption("micro");
	await page.getByRole("button", { name: "First event" }).click();
	const next = page.getByRole("button", { name: "Next step" });
	await expect(readout).toHaveText(/^Raw 0 \/ [1-9][0-9]*$/);
	const extentMatch = (await readout.textContent())?.match(
		/^Raw 0 \/ ([0-9]+)$/,
	);
	const rawExtent = Number(extentMatch?.[1]);
	expect(rawExtent).toBeGreaterThan(0);
	expect(rawExtent).toBeLessThanOrEqual(200);
	for (let step = 1; step <= rawExtent; step += 1) {
		await expect(next).toBeEnabled();
		await next.click();
		await expect(readout).toHaveText(`Raw ${step} / ${rawExtent}`);
	}
	await expect(next).toBeDisabled();
	await granularity.selectOption("operation");
	await expect(page.getByTestId("flow-timeline-visible-readout")).toHaveText(
		/^Operation [1-9][0-9]* \/ [1-9][0-9]*$/,
	);
	await expect(visibleTimeline).toHaveAttribute(
		"aria-valuetext",
		/^Operation [1-9][0-9]* of [1-9][0-9]* · raw [1-9][0-9]* of [1-9][0-9]*$/,
	);
});

test("global keyboard controls affect only the active retained flow workspace", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	await computeTrace(page);
	const maxWorkspace = flowWorkspace(page, "Max Flow");
	const maxReadout = maxWorkspace.getByTestId("flow-timeline-readout");
	const maxGraph = maxWorkspace.getByRole("img", {
		name: "Validated flow network",
	});
	const maxNodeCount = await maxGraph.locator("[data-node-id]").count();
	const maxEdgeCount = await maxGraph.locator(".flow-original-edge").count();
	await page.keyboard.press("ArrowRight");
	await expect(maxReadout).not.toHaveText(/^Raw 0 \/ /);
	const retainedMaxReadout = await maxReadout.textContent();
	expect(retainedMaxReadout).not.toBeNull();

	await page
		.getByRole("button", { name: "Min-Cost Flow", exact: true })
		.click();
	await computeTrace(page);
	const minCostWorkspace = flowWorkspace(page, "Min-Cost Flow");
	await expect(
		minCostWorkspace.getByTestId("flow-timeline-readout"),
	).toHaveText(/^Raw 0 \/ [1-9][0-9]*$/);

	await page.keyboard.press("ArrowRight");
	await expect(
		minCostWorkspace.getByTestId("flow-timeline-readout"),
	).not.toHaveText(/^Raw 0 \/ /);
	await expect(maxReadout).toHaveText(retainedMaxReadout ?? "");

	await page.getByRole("button", { name: "Max Flow", exact: true }).click();
	await expect(maxReadout).toBeVisible();
	await expect(maxReadout).toHaveText(retainedMaxReadout ?? "");
	await expect(maxGraph.locator("[data-node-id]")).toHaveCount(maxNodeCount);
	await expect(maxGraph.locator(".flow-original-edge")).toHaveCount(
		maxEdgeCount,
	);
});

test("event references touch only their exact edge projection and preserve every other stable entity", async ({
	page,
}) => {
	await openFlow(page, "Min-Cost Flow");
	await computeTrace(page);
	await selectMicroSteps(page);
	const graph = page.getByRole("img", { name: "Validated flow network" });
	const initial = await stableGraphSnapshot(graph);
	const next = page.getByRole("button", { name: "Next step" });
	await stepUntilTraceCatalog(page, ".relax", 180);
	await expect(page.locator(".flow-event-action")).toContainText(
		"Relax residual edge",
	);
	const relaxationReadout =
		(await page.getByTestId("flow-timeline-readout").textContent()) ?? "";
	const relaxationMatch = /^Raw (\d+) \/ (\d+)$/.exec(relaxationReadout);
	const relaxationIndex =
		relaxationMatch === null ? undefined : Number(relaxationMatch[1]);
	const traceExtent =
		relaxationMatch === null ? undefined : Number(relaxationMatch[2]);
	expect(relaxationIndex).toBeGreaterThan(0);
	expect(traceExtent).toBeGreaterThan(relaxationIndex ?? Number.MAX_VALUE);
	await page.getByRole("button", { name: "Previous step" }).click();
	await expect(page.getByTestId("flow-timeline-readout")).not.toHaveText(
		relaxationReadout,
	);
	const beforeRelaxation = await stableGraphSnapshot(graph);
	await next.click();
	await expect(page.getByTestId("flow-timeline-readout")).toHaveText(
		relaxationReadout,
	);

	// The canonical default fixture's first published relaxation lowers a through
	// s->a. The source event names the exact residual arc and the changed target;
	// it does not widen that focus to the arc's unchanged source endpoint.
	const expectedTouchedEdges = new Set(["sa"]);
	const expectedTouchedNodes = new Set(["a"]);
	expect(await eventTouchProjection(graph)).toEqual({
		edges: [...expectedTouchedEdges],
		nodes: [...expectedTouchedNodes].sort(),
	});
	expect(await eventChangeProjection(graph)).toEqual({
		edges: [],
		nodes: ["a"],
	});
	const inspector = page.getByLabel("Flow scene inspector");
	const overview = inspectorOverview(inspector);
	await expect(
		overview.locator("dt", { hasText: "Boundary" }).locator(".."),
	).toContainText("Detail");
	await expect(
		overview.locator("dt", { hasText: "Effect" }).locator(".."),
	).toContainText("Change working state");
	await expect(
		inspector.locator("dt", { hasText: "Work delta" }).locator(".."),
	).toContainText(/\+1|1 published transition/);
	await expect(
		inspector.locator("dt", { hasText: "Touched" }).locator(".."),
	).toContainText(/a.*sa:forward|sa:forward.*a/);
	await expect(
		inspector.locator("dt", { hasText: "Changed" }).locator("..").locator("dd"),
	).toHaveText("a");
	const relaxation = await stableGraphSnapshot(graph);
	expectUntouchedEntitiesStable(
		beforeRelaxation,
		relaxation,
		expectedTouchedEdges,
		expectedTouchedNodes,
	);

	const relaxReadout =
		(await page.getByTestId("flow-timeline-readout").textContent()) ?? "";
	await page.getByRole("button", { name: "Previous step" }).click();
	await expect(page.getByTestId("flow-timeline-readout")).not.toHaveText(
		relaxReadout,
	);
	const reversedRelaxation = await stableGraphSnapshot(graph);
	expect(await eventTouchProjection(graph)).toEqual({
		edges: ["sa"],
		nodes: [],
	});
	expectUntouchedEntitiesStable(
		beforeRelaxation as StableGraphSnapshot,
		reversedRelaxation,
		expectedTouchedEdges,
		expectedTouchedNodes,
	);
	const previousReadout =
		(await page.getByTestId("flow-timeline-readout").textContent()) ?? "";
	await next.click();
	await expect(page.getByTestId("flow-timeline-readout")).not.toHaveText(
		previousReadout,
	);
	expect(await stableGraphSnapshot(graph)).toEqual(relaxation);

	const rawPosition = page.getByRole("slider", {
		name: "Raw trace position",
	});
	await rawPosition.fill("0");
	await expect(page.getByTestId("flow-timeline-readout")).toHaveText(
		`Raw 0 / ${traceExtent}`,
	);
	expect(await stableGraphSnapshot(graph)).toEqual(initial);
	await rawPosition.fill(String(relaxationIndex));
	await expect(page.getByTestId("flow-timeline-readout")).toHaveText(
		`Raw ${relaxationIndex} / ${traceExtent}`,
	);
	expect(await eventTouchProjection(graph)).toEqual({
		edges: ["sa"],
		nodes: ["a"],
	});
	expect(await eventChangeProjection(graph)).toEqual({
		edges: [],
		nodes: ["a"],
	});
	expect(await stableGraphSnapshot(graph)).toEqual(relaxation);

	await expect
		.poll(() =>
			graph
				.locator('[data-event-touch="true"]')
				.evaluateAll((items) =>
					items
						.flatMap((item) => item.getAnimations({ subtree: true }))
						.every(
							(animation) =>
								animation.playState === "finished" ||
								animation.playState === "idle",
						),
				),
		)
		.toBe(true);
});

test("cut, selection, and legacy algorithm classes never mutate canonical data channels", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	await computeTrace(page);
	const graph = page.getByRole("img", { name: "Validated flow network" });
	await page.keyboard.press("End");
	await expect(page.getByTestId("flow-timeline-readout")).toHaveText(
		/^Raw ([1-9][0-9]*) \/ \1$/,
	);

	const cutEdge = graph.locator(".flow-original-edge.flow-edge-cut").first();
	await expect(cutEdge).toBeVisible();
	const edgeId = await cutEdge.getAttribute("data-edge-id");
	expect(edgeId).not.toBeNull();
	await expect(
		cutEdge.locator(
			':scope > .flow-algorithm-edge-overlay[data-algorithm-edge-role="strong"]',
		),
	).toBeVisible();
	const cutStyle = await canonicalCapacityStyle(cutEdge);

	await cutEdge
		.locator(":scope > .flow-edge-hit-target")
		.click({ force: true });
	await expect(cutEdge).toHaveClass(/flow-entity-selected/);
	expect(await canonicalCapacityStyle(cutEdge)).toEqual(cutStyle);

	await page.keyboard.press("Home");
	await expect(page.getByTestId("flow-timeline-readout")).toHaveText(
		/^Raw 0 \/ [1-9][0-9]*$/,
	);
	const baselineEdge = graph.locator(
		`.flow-original-edge[data-edge-id="${edgeId}"]`,
	);
	await expect(baselineEdge).not.toHaveClass(/flow-edge-cut/);
	expect(await canonicalCapacityStyle(baselineEdge)).toEqual(cutStyle);

	await openFlow(page, "Min-Cost Flow");
	const minCostGraph = page.getByRole("img", {
		name: "Validated flow network",
	});
	const compositeEdge = minCostGraph.locator(".flow-original-edge").first();
	await expect(
		compositeEdge.locator(':scope > [data-flow-channel="cost"]'),
	).toBeVisible();
	const beforeComposite = await canonicalDataChannelStyles(compositeEdge);
	await compositeEdge.evaluate((element) => {
		element.classList.add(
			"flow-edge-dual-tree",
			"flow-edge-dual-entering",
			"flow-edge-polynomial-tree",
			"flow-edge-polynomial-entering",
			"flow-edge-convex-simplex-tree",
			"flow-edge-convex-simplex-entering",
			"flow-edge-convex-active-forward",
			"flow-edge-convex-active-reverse",
			"flow-edge-matched",
			"flow-edge-fixed",
			"flow-edge-cut",
			"flow-entity-selected",
		);
	});
	expect(await canonicalDataChannelStyles(compositeEdge)).toEqual(
		beforeComposite,
	);
});

test("dense graph labels reveal on edge hover and remain available by keyboard selection", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	await generateReadableDefault(page);
	const graph = page.getByRole("img", { name: "Validated flow network" });
	const allEdges = graph.locator(".flow-original-edge");
	const visibleLabels = graph.locator(".flow-edge-label-group");
	expect(await visibleLabels.count()).toBeLessThan(await allEdges.count());
	const labeledIds = new Set(
		await visibleLabels.evaluateAll((items) =>
			items.flatMap((item) => item.getAttribute("data-edge-label-for") ?? []),
		),
	);
	let hiddenEdge: Locator | undefined;
	let hiddenId: string | undefined;
	for (let index = 0; index < (await allEdges.count()); index += 1) {
		const candidate = allEdges.nth(index);
		const id = await candidate.getAttribute("data-edge-id");
		if (id !== null && !labeledIds.has(id)) {
			hiddenEdge = candidate;
			hiddenId = id;
			break;
		}
	}
	expect(hiddenEdge).toBeDefined();
	await hiddenEdge?.locator(".flow-edge-hit-target").hover();
	await expect(
		graph.locator(`[data-edge-label-for="${hiddenId}"]`),
	).toBeVisible();

	const navigator = page.getByRole("region", { name: "Entity navigator" });
	await navigator.getByRole("searchbox").fill(hiddenId ?? "");
	const result = navigator.locator(".flow-entity-result").first();
	await result.focus();
	await page.keyboard.press("Enter");
	await expect(result).toHaveAttribute("aria-pressed", "true");
	await expect(
		graph.locator(`[data-edge-label-for="${hiddenId}"]`),
	).toBeVisible();
});

test("parallel edges use distinct lanes, tethered labels, and lane badges", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	const editor = page.getByRole("textbox", { name: "Flow Scenario JSON" });
	const scenario = JSON.parse(await editor.inputValue()) as {
		payload: {
			graph: {
				edges: Array<{
					capacity: string;
					cost: string;
					from: string;
					id: string;
					lower: string;
					to: string;
				}>;
			};
		};
	};
	const baseEdge = scenario.payload.graph.edges[0];
	expect(baseEdge).toBeDefined();
	if (baseEdge === undefined) throw new Error("Max Flow fixture has no edge");
	scenario.payload.graph.edges.push(
		{ ...baseEdge, id: "parallel-a", capacity: "5" },
		{ ...baseEdge, id: "parallel-b", capacity: "11" },
		{ ...baseEdge, id: "parallel-c", capacity: "17" },
	);
	await editor.fill(JSON.stringify(scenario, null, 2));
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.locator(".flow-status")).toHaveText("Validated");

	const graph = page.getByRole("img", { name: "Validated flow network" });
	const lanes = graph.locator('.flow-original-edge[data-parallel-count="4"]');
	await expect(lanes).toHaveCount(4);
	const lanePaths = await lanes
		.locator('[data-flow-channel="capacity"]')
		.evaluateAll((paths) => paths.map((path) => path.getAttribute("d")));
	expect(new Set(lanePaths).size).toBe(4);

	const tokens = graph.locator(".flow-edge-route-lane-token");
	await expect(tokens).toHaveCount(4);
	expect(
		(
			await tokens.evaluateAll((elements) =>
				elements.map((element) => ({
					edge: element.getAttribute("data-edge-id"),
					lane: element.getAttribute("data-route-lane-token"),
				})),
			)
		).sort((left, right) => (left.lane ?? "").localeCompare(right.lane ?? "")),
	).toEqual([
		{ edge: "parallel-a", lane: "1/4" },
		{ edge: "parallel-b", lane: "2/4" },
		{ edge: "parallel-c", lane: "3/4" },
		{ edge: baseEdge.id, lane: "4/4" },
	]);
	await expect(
		graph.locator('[data-parallel-count="4"][data-edge-label-for]'),
	).toHaveCount(0);

	await graph
		.locator(
			'.flow-edge-route-lane-token[data-edge-id="parallel-b"][data-route-lane-token="2/4"]',
		)
		.click();
	const selected = graph.locator(
		'.flow-original-edge[data-edge-id="parallel-b"]',
	);
	await expect(selected).toHaveClass(/flow-entity-selected/);
	await expect(selected).toHaveAttribute("data-parallel-index", "2");
	const label = graph.locator('[data-edge-label-for="parallel-b"]');
	await expect(label).toBeVisible();
	await expect(label.locator(".flow-edge-label-leader-halo")).toHaveCount(1);
	await expect(label.locator(".flow-edge-label-leader")).toHaveCount(1);
	expect(
		await label.locator(".flow-edge-label-leader").evaluate((element) => {
			const line = element as SVGLineElement;
			return Math.hypot(
				line.x1.baseVal.value - line.x2.baseVal.value,
				line.y1.baseVal.value - line.y2.baseVal.value,
			);
		}),
	).toBeGreaterThanOrEqual(30);
	await expect(label.locator(".flow-edge-parallel-badge")).toHaveText("2/4");
	const markerStyle = await graph
		.locator("defs .flow-arrow-capacity")
		.evaluate((element) => {
			const style = getComputedStyle(element);
			return {
				fill: style.fill,
				stroke: style.stroke,
				strokeWidth: Number.parseFloat(style.strokeWidth),
			};
		});
	expect(markerStyle.strokeWidth).toBeGreaterThanOrEqual(2);
	expect(markerStyle.fill).not.toBe(markerStyle.stroke);

	const keyboardResult = await selectNavigatorResultByKeyboard(
		page,
		"edge",
		"parallel-c",
	);
	await expect(keyboardResult).toHaveAccessibleName(
		`parallel-c original edge · ${baseEdge.from} → ${baseEdge.to}`,
	);
	await expect(
		graph.locator('.flow-original-edge[data-edge-id="parallel-c"]'),
	).toHaveClass(/flow-entity-selected/);
	await expect(selected).not.toHaveClass(/flow-entity-selected/);
	await expect(
		graph.locator('[data-parallel-count="4"][data-edge-label-for]'),
	).toHaveCount(1);
	await expect(
		graph
			.locator('[data-edge-label-for="parallel-c"]')
			.locator(".flow-edge-parallel-badge"),
	).toHaveText("3/4");

	await page.setViewportSize({ width: 390, height: 844 });
	expect(
		await graph
			.locator(".flow-edge-label-group")
			.evaluateAll((labels) =>
				labels.every((label) => getComputedStyle(label).display === "none"),
			),
	).toBe(true);
	const mobileSelection = page.getByRole("region", {
		name: "Selected edge details",
	});
	await expect(mobileSelection).toBeVisible();
	await expect(mobileSelection).toContainText("parallel-c · s → a");
	await expect(mobileSelection).toContainText("FLOW 0 / CAP 17");
	const laneButtons = mobileSelection.getByRole("button");
	await expect(laneButtons).toHaveCount(4);
	for (let index = 0; index < 4; index += 1) {
		const bounds = await laneButtons.nth(index).boundingBox();
		expect(bounds?.width ?? 0).toBeGreaterThanOrEqual(44);
		expect(bounds?.height ?? 0).toBeGreaterThanOrEqual(44);
	}
	await mobileSelection
		.getByRole("button", {
			name: `Select lane 4 of 4, edge ${baseEdge.id}`,
		})
		.click();
	await expect(
		graph.locator(`.flow-original-edge[data-edge-id="${baseEdge.id}"]`),
	).toHaveClass(/flow-entity-selected/);
	await expect(mobileSelection).toContainText(`4/4`);
	expect(
		await graph
			.locator(".flow-edge-hit-target")
			.first()
			.evaluate((element) => {
				const style = getComputedStyle(element);
				return {
					strokeWidth: Number.parseFloat(style.strokeWidth),
					vectorEffect: style.vectorEffect,
				};
			}),
	).toEqual({ strokeWidth: 44, vectorEffect: "non-scaling-stroke" });
});

test("large parallel groups stay bounded in Structure and reveal any selected lane", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	const editor = page.getByRole("textbox", { name: "Flow Scenario JSON" });
	const scenario = JSON.parse(await editor.inputValue()) as {
		payload: {
			graph: {
				edges: Array<{
					capacity: string;
					cost: string;
					from: string;
					id: string;
					lower: string;
					to: string;
				}>;
			};
		};
	};
	const baseEdge = scenario.payload.graph.edges[0];
	if (baseEdge === undefined) throw new Error("Max Flow fixture has no edge");
	for (let index = 0; index < 12; index += 1) {
		scenario.payload.graph.edges.push({
			...baseEdge,
			id: `parallel-${index.toString().padStart(2, "0")}`,
			capacity: String(index + 1),
		});
	}
	await editor.fill(JSON.stringify(scenario, null, 2));
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.locator(".flow-status")).toHaveText("Validated");
	await expect(page.locator(".flow-lod-level")).toHaveText("Structure");

	const graph = page.getByRole("img", { name: "Validated flow network" });
	const parallelEdges = graph.locator(
		'.flow-original-edge[data-parallel-count="13"]',
	);
	await expect(parallelEdges).toHaveCount(13);
	await expect(graph.locator(".flow-edge-route-lane-token")).toHaveCount(0);
	expect(
		await parallelEdges
			.locator('[data-flow-channel="capacity"]')
			.evaluateAll((paths) =>
				paths.every((path) => {
					const bounds = (path as SVGGraphicsElement).getBBox();
					return (
						bounds.x >= 0 &&
						bounds.y >= 0 &&
						bounds.x + bounds.width <= 900 &&
						bounds.y + bounds.height <= 540
					);
				}),
			),
	).toBe(true);

	await selectNavigatorResultByKeyboard(page, "edge", baseEdge.id);
	const selectedLabel = graph.locator(`[data-edge-label-for="${baseEdge.id}"]`);
	await expect(selectedLabel.locator(".flow-edge-parallel-badge")).toHaveText(
		"13/13",
	);
	expect(
		await selectedLabel
			.locator(".flow-edge-label-leader")
			.evaluate((element) => {
				const line = element as SVGLineElement;
				return Math.hypot(
					line.x1.baseVal.value - line.x2.baseVal.value,
					line.y1.baseVal.value - line.y2.baseVal.value,
				);
			}),
	).toBeGreaterThanOrEqual(37.9);
});

test("390px keeps a selected lane reachable in a 64-edge parallel group", async ({
	page,
}) => {
	await page.setViewportSize({ width: 390, height: 844 });
	await openFlow(page, "Max Flow");
	const editor = await revealFlowScenarioEditor(page);
	const scenario = JSON.parse(await editor.inputValue()) as {
		payload: {
			graph: {
				edges: Array<{
					capacity: string;
					cost: string;
					from: string;
					id: string;
					lower: string;
					to: string;
				}>;
			};
		};
	};
	const baseEdge = scenario.payload.graph.edges[0];
	if (baseEdge === undefined) throw new Error("Max Flow fixture has no edge");
	for (let index = 0; index < 63; index += 1) {
		scenario.payload.graph.edges.push({
			...baseEdge,
			id: `parallel-${index.toString().padStart(2, "0")}`,
			capacity: String(index + 1),
		});
	}
	await editor.fill(JSON.stringify(scenario, null, 2));
	await closeFlowInputPanel(page);
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.locator(".flow-status")).toHaveText("Validated");
	await activeFlowWorkspace(page)
		.getByRole("button", { name: "Inspector", exact: true })
		.click();
	await selectNavigatorResultByKeyboard(page, "edge", "parallel-62");
	await activeFlowWorkspace(page)
		.getByRole("button", { name: "Close inspector panel" })
		.click();

	const selection = page.getByRole("region", {
		name: "Selected edge details",
	});
	await expect(selection).toContainText("parallel-62 · s → a");
	await expect(selection).toContainText("FLOW 0 / CAP 63");
	const lanes = selection.locator(".flow-mobile-parallel-lanes");
	await expect(lanes.getByRole("button")).toHaveCount(64);
	const selectedLane = lanes.getByRole("button", {
		name: "Select lane 63 of 64, edge parallel-62",
	});
	await expect(selectedLane).toHaveAttribute("aria-pressed", "true");
	expect(
		await lanes.evaluate(
			(element) => element.scrollWidth > element.clientWidth,
		),
	).toBe(true);
	expect(
		await selectedLane.evaluate((element) => {
			const lane = element.getBoundingClientRect();
			const list = element.parentElement?.getBoundingClientRect();
			return (
				list !== undefined && lane.left >= list.left && lane.right <= list.right
			);
		}),
	).toBe(true);
	expect(
		await selection.evaluate((element) => {
			const bounds = element.getBoundingClientRect();
			return bounds.left >= 0 && bounds.right <= window.innerWidth;
		}),
	).toBe(true);
});

test("loading a different graph clears selection even when edge IDs are reused", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	const graph = page.getByRole("img", { name: "Validated flow network" });
	await selectNavigatorResultByKeyboard(page, "edge", "sa");
	await expect(
		graph.locator('.flow-original-edge[data-edge-id="sa"]'),
	).toHaveClass(/flow-entity-selected/);

	const editor = page.getByRole("textbox", { name: "Flow Scenario JSON" });
	const scenario = JSON.parse(await editor.inputValue()) as {
		payload: { graph: { edges: Array<{ id: string; capacity: string }> } };
	};
	const reused = scenario.payload.graph.edges.find((edge) => edge.id === "sa");
	if (reused === undefined) throw new Error("Default edge sa is missing");
	reused.capacity = String(Number(reused.capacity) + 1);
	await editor.fill(JSON.stringify(scenario, null, 2));
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.locator(".flow-status")).toHaveText("Validated");
	await expect(
		graph.locator('.flow-original-edge[data-edge-id="sa"]'),
	).not.toHaveClass(/flow-entity-selected/);
	await expect(
		page
			.getByRole("region", { name: "Entity navigator" })
			.locator('.flow-entity-result[aria-pressed="true"]'),
	).toHaveCount(0);
});

test("electrical overlays keep the graph viewport readable", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	const editor = page.getByRole("textbox", { name: "Flow Scenario JSON" });
	const scenario = JSON.parse(await editor.inputValue()) as {
		payload: {
			algorithm: { config: Record<string, never>; id: string };
			algorithm_seed: string;
			graph: {
				edges: Array<Record<string, string>>;
				nodes: Array<Record<string, string>>;
			};
			model: Record<string, string>;
		};
	};
	scenario.payload.algorithm = {
		config: {},
		id: "augmenting-electrical-flow",
	};
	scenario.payload.algorithm_seed = "0";
	scenario.payload.graph = {
		edges: [
			{ id: "sa", from: "s", to: "a", lower: "0", capacity: "8", cost: "0" },
			{ id: "at", from: "a", to: "t", lower: "0", capacity: "8", cost: "0" },
			{ id: "sb", from: "s", to: "b", lower: "0", capacity: "1", cost: "0" },
			{ id: "bt", from: "b", to: "t", lower: "0", capacity: "1", cost: "0" },
		],
		nodes: ["s", "a", "b", "t"].map((id) => ({ id, supply: "0" })),
	};
	scenario.payload.model = { kind: "max-flow", source: "s", sink: "t" };
	await editor.fill(JSON.stringify(scenario, null, 2));
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.locator(".flow-status")).toHaveText("Validated");
	await computeTrace(page);
	await selectMicroSteps(page);

	const workspace = activeFlowWorkspace(page);
	const initialReadout = await workspace
		.getByTestId("flow-timeline-readout")
		.textContent();
	const total = initialReadout?.match(/^Raw 0 \/ ([1-9][0-9]*)$/)?.[1];
	expect(total).toBeDefined();
	expect(Number(total)).toBeGreaterThan(5);
	await workspace.locator(".flow-timeline input[type='range']").fill("5");
	await expect(workspace.getByTestId("flow-timeline-readout")).toHaveText(
		`Raw 5 / ${total}`,
	);
	const viewport = await workspace
		.locator(".flow-canvas-viewport")
		.boundingBox();
	expect(viewport?.height ?? 0).toBeGreaterThanOrEqual(400);
	const firstNode = await workspace
		.getByRole("img", { name: "Validated flow network" })
		.locator(".flow-node")
		.first()
		.boundingBox();
	expect(firstNode?.height ?? 0).toBeGreaterThanOrEqual(34);
	await assertReadableNodeTraceCallouts(page);
});

test("native min-cost generators replace node-indexed algorithm configuration", async ({
	page,
}) => {
	test.setTimeout(90_000);
	await openFlow(page, "Min-Cost Flow");
	let workspace = activeFlowWorkspace(page);
	await selectFlowAlgorithm(page, "tardos-framework");
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/"potentials": \{/,
	);

	await workspace
		.getByRole("button", { name: "Generate", exact: true })
		.click();
	let dialog = page.getByRole("dialog", {
		name: "Generate Min-Cost Flow graph",
	});
	await selectGeneratorFamily(dialog, "cycle");
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });
	workspace = activeFlowWorkspace(page);
	const circulationEditor = workspace.locator(".flow-scenario-editor");
	await expect(circulationEditor).toHaveValue(/"kind": "circulation"/);
	await expect(circulationEditor).toHaveValue(/"id": "simple-cycle-canceling"/);
	await expect(circulationEditor).not.toHaveValue(/"potentials": \{/);
	await expect(workspace.locator(".flow-status")).toHaveText("Validated");
	await expect(
		workspace.getByText("Engine error", { exact: true }),
	).toBeHidden();
	await computeTrace(page);

	await selectFlowAlgorithm(page, "prediction-assisted-epsilon-relaxation");
	await expect(workspace.locator(".flow-scenario-editor")).toHaveValue(
		/"predicted_potentials": \{/,
	);
	await workspace
		.getByRole("button", { name: "Generate", exact: true })
		.click();
	dialog = page.getByRole("dialog", {
		name: "Generate Min-Cost Flow graph",
	});
	await selectGeneratorFamily(dialog, "goto-torus");
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });
	workspace = activeFlowWorkspace(page);
	const transshipmentEditor = workspace.locator(".flow-scenario-editor");
	await expect(transshipmentEditor).toHaveValue(/"kind": "transshipment"/);
	await expect(transshipmentEditor).toHaveValue(
		/"id": "primal-network-simplex"/,
	);
	await expect(transshipmentEditor).not.toHaveValue(
		/"predicted_potentials": \{/,
	);
	await expect(workspace.locator(".flow-status")).toHaveText("Validated");
	await expect(
		workspace.getByText("Engine error", { exact: true }),
	).toBeHidden();
	await computeTrace(page);
});

test("phone electrical traces keep one linked node callout in bounds", async ({
	page,
}) => {
	await page.setViewportSize({ width: 390, height: 844 });
	await openFlow(page, "Max Flow");
	const editor = await revealFlowScenarioEditor(page);
	const baseline = JSON.parse(await editor.inputValue()) as {
		payload: {
			algorithm: { config: Record<string, never>; id: string };
			algorithm_seed: string;
			graph: {
				edges: Array<Record<string, string>>;
				nodes: Array<Record<string, string>>;
			};
			model: Record<string, string>;
		};
	};
	baseline.payload.algorithm_seed = "0";
	baseline.payload.graph = {
		edges: [
			{ id: "sa", from: "s", to: "a", lower: "0", capacity: "1", cost: "0" },
			{ id: "at", from: "a", to: "t", lower: "0", capacity: "1", cost: "0" },
			{ id: "sb", from: "s", to: "b", lower: "0", capacity: "1", cost: "0" },
			{ id: "bt", from: "b", to: "t", lower: "0", capacity: "1", cost: "0" },
		],
		nodes: ["s", "a", "b", "t"].map((id) => ({ id, supply: "0" })),
	};
	baseline.payload.model = { kind: "max-flow", source: "s", sink: "t" };
	await closeFlowInputPanel(page);

	for (const algorithmId of [
		"electrical-flow",
		"augmenting-electrical-flow",
		"interior-point-max-flow",
	]) {
		baseline.payload.algorithm = { config: {}, id: algorithmId };
		const activeEditor = await revealFlowScenarioEditor(page);
		await activeEditor.fill(JSON.stringify(baseline, null, 2));
		await closeFlowInputPanel(page);
		await page.getByRole("button", { name: "Load", exact: true }).click();
		await expect(page.locator(".flow-status")).toHaveText("Validated");
		await computeTrace(page);
		await selectMicroSteps(page);
		const next = activeFlowWorkspace(page).getByRole("button", {
			name: "Next step",
		});
		const callouts = activeFlowWorkspace(page).locator(
			".flow-node-trace-callout:visible",
		);
		const expectedCalloutOwners = activeFlowWorkspace(page).locator(
			'[data-trace-callout-expected="true"]:visible',
		);
		for (
			let step = 0;
			step < 40 &&
			((await callouts.count()) === 0 ||
				(await expectedCalloutOwners.count()) === 0);
			step += 1
		) {
			await expect(next).toBeEnabled();
			await next.click();
		}
		await assertReadableNodeTraceCallouts(page, 1);
	}
});

test("node, original edge, residual arc, and LOD aggregate have keyboard-selectable accessible names", async ({
	page,
}) => {
	await openFlow(page, "Max Flow");
	await page.getByRole("button", { name: "Both", exact: true }).click();
	const graph = page.getByRole("img", { name: "Validated flow network" });

	const nodeResult = await selectNavigatorResultByKeyboard(page, "node", "s");
	await expect(nodeResult).toHaveAccessibleName("s node");
	await expect(graph.locator('[data-node-id="s"]')).toHaveClass(
		/flow-entity-selected/,
	);

	const originalResult = await selectNavigatorResultByKeyboard(
		page,
		"edge",
		"sa",
	);
	await expect(originalResult).toHaveAccessibleName("sa original edge · s → a");
	await expect(
		graph.locator('.flow-original-edge[data-edge-id="sa"]'),
	).toHaveClass(/flow-entity-selected/);

	const residualResult = await selectNavigatorResultByKeyboard(
		page,
		"residual-arc",
		"sa:reverse",
	);
	await expect(residualResult).toHaveAccessibleName(
		"sa:reverse residual arc · a → s",
	);
	await expect(
		graph.locator(
			'.flow-residual-arc[data-edge-id="sa"][data-residual-direction="reverse"]',
		),
	).toHaveClass(/flow-entity-selected/);

	await page.getByRole("button", { name: "Generate", exact: true }).click();
	const dialog = page.getByRole("dialog", { name: "Generate Max Flow graph" });
	await selectGeneratorFamily(dialog, "grid-2d");
	await dialog.getByLabel("Rows", { exact: true }).fill("25");
	await dialog.getByLabel("Columns", { exact: true }).fill("25");
	await dialog.getByRole("button", { name: "Generate & load" }).click();
	await expect(dialog).toBeHidden({ timeout: 30_000 });
	await expect(page.locator(".flow-lod-overview")).toHaveText("Overview");

	const aggregateResult = await selectNavigatorResultByKeyboard(
		page,
		"aggregate",
		"cluster",
	);
	await expect(aggregateResult).toHaveAccessibleName(
		/cluster:[^ ]+ aggregate cluster · [1-9][0-9]* nodes/,
	);
	const overviewGraph = activeFlowWorkspace(page).getByRole("img", {
		name: "Validated flow-network overview",
	});
	await expect(
		overviewGraph.locator("[data-cluster-id].flow-entity-selected"),
	).toHaveCount(1);
});

test("grayscale preserves compound cost, residual, selection, and current-path channels", async ({
	page,
}) => {
	await openFlow(page, "Min-Cost Flow");
	const editor = page.getByRole("textbox", { name: "Flow Scenario JSON" });
	const scenario = JSON.parse(await editor.inputValue()) as {
		payload: {
			graph: {
				edges: Array<Record<string, string>>;
			};
		};
	};
	// The added a→s original edge deliberately shares endpoints and direction
	// with sa's reverse residual arc. They must remain separate visual entities.
	scenario.payload.graph.edges.push({
		id: "as",
		from: "a",
		to: "s",
		lower: "0",
		capacity: "5",
		cost: "4",
	});
	await editor.fill(JSON.stringify(scenario, null, 2));
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByText("Validated", { exact: true })).toBeVisible();
	await page.getByRole("button", { name: "Both", exact: true }).click();
	await computeTrace(page);
	await selectMicroSteps(page);
	await stepUntilCaption(page, "Relax residual edge", 180);
	const reverseResult = await selectNavigatorResultByKeyboard(
		page,
		"residual-arc",
		"sa:reverse",
	);
	await expect(reverseResult).toHaveAccessibleName(
		"sa:reverse residual arc · a → s",
	);

	const graph = page.getByRole("img", { name: "Validated flow network" });
	const originalOpposite = graph.locator(
		'.flow-original-edge[data-edge-id="as"]',
	);
	const reverseResidual = graph.locator(
		'.flow-residual-arc[data-edge-id="sa"][data-residual-direction="reverse"]',
	);
	await expect(originalOpposite).toHaveCount(1);
	await expect(reverseResidual).toHaveCount(1);
	await expect(
		originalOpposite.locator(":scope > .flow-capacity-rail"),
	).toHaveCount(1);
	await expect(reverseResidual).toHaveClass(/flow-residual-reverse/);
	expect(
		await originalOpposite
			.locator(":scope > .flow-capacity-rail")
			.getAttribute("d"),
	).not.toBe(
		await reverseResidual.locator(":scope > path").last().getAttribute("d"),
	);

	const residualResult = await selectNavigatorResultByKeyboard(
		page,
		"residual-arc",
		"sa:forward",
	);
	await expect(residualResult).toHaveAccessibleName(
		"sa:forward residual arc · s → a",
	);

	await graph.evaluate((element) => {
		(element as SVGSVGElement).style.filter = "grayscale(1)";
	});
	await expect(graph).toHaveCSS("filter", "grayscale(1)");

	const negativeCost = graph.locator(
		'.flow-original-edge[data-edge-id="sb"] > .flow-cost-negative',
	);
	const currentOriginal = graph.locator(
		'.flow-original-edge[data-edge-id="sa"]',
	);
	const selectedResidual = graph.locator(
		'.flow-residual-arc.flow-entity-selected[data-edge-id="sa"][data-residual-direction="forward"]',
	);
	await expect(negativeCost).toHaveCSS(
		"stroke-dasharray",
		/7px(?:, )?4px|8px(?:, )?4px|7 4|8 4/,
	);
	await expect(
		currentOriginal.locator(":scope > .flow-active-outline"),
	).toHaveCSS("stroke-dasharray", "none");
	await expect(
		selectedResidual.locator(":scope > .flow-residual-selection-outline"),
	).toHaveCSS("stroke-dasharray", /2px(?:, )?3px|2 3/);
	await expect(
		selectedResidual.locator(":scope > .flow-residual-active-outline"),
	).toHaveCSS("stroke-dasharray", /10px(?:, )?3px|10 3/);
	await expect(
		currentOriginal.locator(":scope > [data-flow-channel='capacity']"),
	).toHaveCount(1);
	await expect(
		currentOriginal.locator(":scope > [data-flow-channel='flow']"),
	).toHaveCount(1);
});

test("visual help opens on hover and focus and dismisses with Escape", async ({
	page,
}) => {
	await openFlow(page, "Min-Cost Flow");
	const help = page.getByRole("button", { name: "Visual encoding help" });
	const tooltip = page.getByRole("tooltip");
	await help.hover();
	await expect(tooltip).toBeVisible();
	await expect(tooltip).toContainText("Outer width is capacity");
	await page.mouse.move(0, 0);
	await expect(tooltip).toBeHidden();
	await help.focus();
	await expect(tooltip).toBeVisible();
	await page.keyboard.press("Escape");
	await expect(tooltip).toBeHidden();
});

test("390px keeps primary stepping usable and exposes touched endpoint labels", async ({
	page,
}, testInfo) => {
	await page.setViewportSize({ width: 390, height: 844 });
	await openFlow(page, "Max Flow");
	await computeTrace(page);
	await selectMicroSteps(page);
	const slider = page.getByRole("slider", { name: "Raw trace position" });
	const sliderBox = await slider.boundingBox();
	expect(sliderBox?.width ?? 0).toBeGreaterThanOrEqual(80);
	for (const name of ["Previous step", "Next step"]) {
		const box = await page.getByRole("button", { name }).boundingBox();
		expect(box?.width ?? 0).toBeGreaterThanOrEqual(44);
		expect(box?.height ?? 0).toBeGreaterThanOrEqual(44);
	}
	for (const control of [
		page.getByTestId("flow-timeline-readout"),
		page.getByRole("combobox", { name: "Playback granularity" }),
	]) {
		expect(
			await control.evaluate((element) => {
				const bounds = element.getBoundingClientRect();
				return bounds.top >= 0 && bounds.bottom <= window.innerHeight;
			}),
		).toBe(true);
	}
	await page.getByRole("button", { name: "Next step" }).click();
	const touchedNodes = page.locator('[data-node-id][data-event-touch="true"]');
	for (let index = 0; index < (await touchedNodes.count()); index += 1) {
		await expect(
			touchedNodes.nth(index).locator(".flow-node-label"),
		).toBeVisible();
	}
	if (hasDarwinPixelBaseline(testInfo)) {
		await expect(page.locator(".flow-shell")).toHaveScreenshot(
			"max-flow-390-micro.png",
			{ animations: "disabled" },
		);
	}
});

test("320px phone and 768px tablet keep generation and stepping operable without horizontal overflow", async ({
	page,
}) => {
	for (const viewport of [
		{ width: 320, height: 568 },
		{ width: 768, height: 1024 },
	]) {
		await page.setViewportSize(viewport);
		await openFlow(page, "Max Flow");
		await expect
			.poll(() =>
				page.evaluate(
					() => document.documentElement.scrollWidth <= window.innerWidth,
				),
			)
			.toBe(true);
		for (const name of ["Algorithm", "Generate", "Load", "Run trace"]) {
			await expect(
				page.getByRole("button", { name, exact: true }),
			).toBeVisible();
		}
		await computeTrace(page);
		await selectMicroSteps(page);
		const slider = page.getByRole("slider", { name: "Raw trace position" });
		await slider.scrollIntoViewIfNeeded();
		expect((await slider.boundingBox())?.width ?? 0).toBeGreaterThanOrEqual(80);
		for (const name of ["Previous step", "Next step"]) {
			const control = page.getByRole("button", { name });
			await control.scrollIntoViewIfNeeded();
			const box = await control.boundingBox();
			expect(box?.width ?? 0).toBeGreaterThanOrEqual(44);
			expect(box?.height ?? 0).toBeGreaterThanOrEqual(44);
		}
		await page.getByRole("button", { name: "Next step" }).click();
		await expect(page.getByTestId("flow-timeline-readout")).not.toHaveText(
			/^Raw 0 \/ /,
		);
	}
});

test("leaving the drawer breakpoint clears modal state and inert background", async ({
	page,
}) => {
	await page.setViewportSize({ width: 900, height: 800 });
	await openFlow(page, "Max Flow");
	const workspace = activeFlowWorkspace(page);
	const input = workspace.locator(".flow-mobile-panel-controls button").first();
	await input.click();
	await expect(input).toHaveAttribute("aria-expanded", "true");
	await expect(
		workspace.getByRole("button", { name: "Close input panel" }),
	).toBeVisible();
	expect(
		await page.evaluate(() =>
			[
				document.querySelector(".workspace-switcher"),
				document.querySelector(
					"[data-workspace-id]:not([hidden]) .flow-topbar",
				),
				document.querySelector(
					"[data-workspace-id]:not([hidden]) .flow-canvas-panel",
				),
				document.querySelector(
					"[data-workspace-id]:not([hidden]) .flow-inspector-panel",
				),
			].every((element) => element instanceof HTMLElement && element.inert),
		),
	).toBe(true);

	await page.setViewportSize({ width: 1200, height: 800 });
	await expect(input).toHaveAttribute("aria-expanded", "false");
	await expect
		.poll(() =>
			page.evaluate(() =>
				[
					document.querySelector(".workspace-switcher"),
					document.querySelector(
						"[data-workspace-id]:not([hidden]) .flow-topbar",
					),
					document.querySelector(
						"[data-workspace-id]:not([hidden]) .flow-canvas-panel",
					),
					document.querySelector(
						"[data-workspace-id]:not([hidden]) .flow-inspector-panel",
					),
				].every((element) => element instanceof HTMLElement && !element.inert),
			),
		)
		.toBe(true);
	await workspace
		.getByRole("button", { name: "Algorithm", exact: true })
		.click();
	await expect(
		page.getByRole("dialog", { name: "Flow algorithms" }),
	).toBeVisible();
});

test("200% zoom-equivalent reflow and color-vision simulation preserve non-color channels", async ({
	page,
}, testInfo) => {
	await page.setViewportSize({ width: 640, height: 900 });
	await openFlow(page, "Min-Cost Flow");
	await expect
		.poll(() =>
			page.evaluate(
				() => document.documentElement.scrollWidth <= window.innerWidth,
			),
		)
		.toBe(true);
	for (const name of ["Algorithm", "Generate", "Load", "Run trace"]) {
		await expect(page.getByRole("button", { name, exact: true })).toBeVisible();
	}

	const graph = page.getByRole("img", { name: "Validated flow network" });
	await graph.evaluate((graphElement) => {
		const namespace = "http://www.w3.org/2000/svg";
		const definitions = document.createElementNS(namespace, "defs");
		const filter = document.createElementNS(namespace, "filter");
		filter.id = "flow-min-cost-flow-deuteranopia-simulation";
		const matrix = document.createElementNS(namespace, "feColorMatrix");
		matrix.setAttribute("type", "matrix");
		matrix.setAttribute(
			"values",
			"0.367 0.861 -0.228 0 0 0.280 0.673 0.047 0 0 -0.012 0.043 0.969 0 0 0 0 0 1 0",
		);
		filter.append(matrix);
		definitions.append(filter);
		graphElement.prepend(definitions);
		graphElement.style.filter =
			"url(#flow-min-cost-flow-deuteranopia-simulation)";
	});
	await expect(graph.locator(".flow-cost-positive").first()).toHaveCSS(
		"stroke-dasharray",
		"none",
	);
	await expect(graph.locator(".flow-cost-negative").first()).toHaveCSS(
		"stroke-dasharray",
		/8px(?:, )?4px|8 4/,
	);
	await expect(graph.locator(".flow-cost-zero").first()).toHaveCSS(
		"stroke-dasharray",
		/2px(?:, )?5px|2 5/,
	);
	const firstEdge = graph.locator(".flow-original-edge").first();
	expect(
		Number(
			await firstEdge
				.locator(":scope > .flow-cost-rail")
				.getAttribute("stroke-width"),
		),
	).toBeGreaterThan(
		Number(
			await firstEdge
				.locator(":scope > .flow-capacity-rail")
				.getAttribute("stroke-width"),
		),
	);
	if (hasDarwinPixelBaseline(testInfo)) {
		await expect(page.locator(".flow-shell")).toHaveScreenshot(
			"min-cost-flow-640-deuteranopia-200-zoom.png",
			{ animations: "disabled" },
		);
	}
});

test("responsive, reduced-motion, and forced-color views preserve non-color channels", async ({
	page,
}, testInfo) => {
	await page.setViewportSize({ width: 1024, height: 768 });
	await page.emulateMedia({ reducedMotion: "reduce", forcedColors: "active" });
	await openFlow(page, "Min-Cost Flow");
	const graph = page.getByRole("img", { name: "Validated flow network" });
	await expect(graph.locator(".flow-cost-negative").first()).toHaveCSS(
		"stroke-dasharray",
		/8px(?:, )?4px|8 4/,
	);
	await expect(graph.locator(".flow-cost-zero").first()).toHaveCSS(
		"stroke-dasharray",
		/2px(?:, )?5px|2 5/,
	);
	if (hasDarwinPixelBaseline(testInfo)) {
		await expect(page.locator(".flow-shell")).toHaveScreenshot(
			"min-cost-flow-1024-forced-colors.png",
			{ animations: "disabled" },
		);
	}
	await computeTrace(page);
	await selectMicroSteps(page);
	await stepUntilCaption(page, "Relax residual edge", 180);
	const touched = graph.locator("[data-event-touch='true']");
	expect(await touched.count()).toBeGreaterThan(0);
	const reducedMotion = await touched.evaluateAll((items) => {
		const seconds = (value: string) =>
			Math.max(
				...value.split(",").map((part) => {
					const duration = part.trim();
					return duration.endsWith("ms")
						? Number.parseFloat(duration) / 1_000
						: Number.parseFloat(duration);
				}),
			);
		const elements = items.flatMap((item) => [
			item,
			...item.querySelectorAll("*"),
		]);
		const styles = elements.map((element) => getComputedStyle(element));
		return {
			animationDurationSeconds: Math.max(
				...styles.map((style) => seconds(style.animationDuration)),
			),
			hasInfiniteAnimation: styles.some((style) =>
				style.animationIterationCount
					.split(",")
					.some((count) => count.trim() === "infinite"),
			),
			runningAnimationCount: items
				.flatMap((item) => item.getAnimations({ subtree: true }))
				.filter((animation) => animation.playState === "running").length,
			transitionDurationSeconds: Math.max(
				...styles.map((style) => seconds(style.transitionDuration)),
			),
		};
	});
	expect(reducedMotion.hasInfiniteAnimation).toBe(false);
	expect(reducedMotion.runningAnimationCount).toBe(0);
	expect(reducedMotion.animationDurationSeconds).toBeLessThanOrEqual(0.000_01);
	expect(reducedMotion.transitionDurationSeconds).toBeLessThanOrEqual(0.000_01);
});

test("1440px visual baseline keeps the canvas dominant", async ({
	page,
}, testInfo) => {
	await openFlow(page, "Max Flow");
	await computeTrace(page);
	await selectMicroSteps(page);
	await page.getByRole("button", { name: "Next step" }).click();
	const canvas = page.getByRole("region", { name: "Flow graph visualization" });
	const canvasBox = await canvas.boundingBox();
	expect(canvasBox?.width ?? 0).toBeGreaterThan(600);
	expect(canvasBox?.height ?? 0).toBeGreaterThan(500);
	if (hasDarwinPixelBaseline(testInfo)) {
		await expect(page.locator(".flow-shell")).toHaveScreenshot(
			"max-flow-1440-micro.png",
			{ animations: "disabled" },
		);
	}
});
