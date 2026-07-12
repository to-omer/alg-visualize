import { readFile } from "node:fs/promises";

import { expect, test } from "./browser-test";

const ALGORITHMS = [
	"avl",
	"wbt",
	"aa",
	"llrb",
	"treap",
	"zip",
	"splay",
	"scapegoat",
	"skip-list",
	"b-tree",
	"veb",
	"x-fast",
	"y-fast",
] as const;

for (const algorithm of ALGORITHMS) {
	test(`${algorithm} loads and commits through the production WASM Worker`, async ({
		page,
	}) => {
		await page.goto("/");
		await page.getByLabel("Structure", { exact: true }).selectOption(algorithm);
		await expect(page.getByTestId("engine-status")).toHaveText("idle");
		await expect(page.getByLabel("Structure", { exact: true })).toHaveValue(
			algorithm,
		);
		await page.getByRole("button", { name: "Load", exact: true }).click();
		await expect(page.getByTestId("engine-status")).toHaveText("ready");
		await expect(page.getByText("3 entries", { exact: true })).toBeVisible();
		await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 4");

		await page.getByRole("button", { name: "Next step" }).click();
		await expect(page.getByTestId("timeline-readout")).toHaveText("1 / 4");
		for (
			let event = 0;
			event < 256 &&
			(await page.getByText("4 entries", { exact: true }).count()) === 0;
			event += 1
		) {
			await page.getByRole("button", { name: "Next step" }).click();
		}
		await expect(page.getByText("4 entries", { exact: true })).toBeVisible();
		await expect(page.getByTestId("pixi-host")).toHaveAttribute(
			"data-renderer",
			"webgl",
		);
	});
}

test("algorithm parameters are validated and persisted through the Worker", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByLabel("Structure", { exact: true }).selectOption("b-tree");
	await expect(page.getByTestId("engine-status")).toHaveText("idle");
	await page.getByRole("button", { name: "Parameters", exact: true }).click();
	await page.getByLabel("min degree", { exact: true }).fill("4");
	await page.getByRole("button", { name: "Apply", exact: true }).click();
	await expect(page.getByRole("dialog")).toBeHidden();
	await expect(page.getByTestId("engine-status")).toHaveText("idle");
	const downloadPromise = page.waitForEvent("download");
	await page.getByRole("button", { name: "Export", exact: true }).click();
	const download = await downloadPromise;
	const path = await download.path();
	expect(path).not.toBeNull();
	const exported = JSON.parse(await readFile(path as string, "utf8")) as {
		payload: { algorithm: unknown };
	};
	expect(exported.payload.algorithm).toEqual({
		id: "b-tree",
		config: { min_degree: 4 },
	});
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await expect(page.getByRole("heading", { name: "B-tree" })).toBeVisible();
});

test("an exported Scenario imports through the file boundary and restores its algorithm", async ({
	page,
}) => {
	await page.goto("/");
	const downloadPromise = page.waitForEvent("download");
	await page.getByRole("button", { name: "Export", exact: true }).click();
	const download = await downloadPromise;
	const path = await download.path();
	if (path === null) {
		throw new Error("export did not produce a local file");
	}

	await page.getByLabel("Structure", { exact: true }).selectOption("splay");
	await expect(page.getByLabel("Structure", { exact: true })).toHaveValue(
		"splay",
	);
	await page.locator('input[type="file"]').setInputFiles(path);
	await expect(page.getByTestId("engine-status")).toHaveText("idle");
	await expect(page.getByLabel("Structure", { exact: true })).toHaveValue(
		"avl",
	);
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 4");
});

test("legacy derived revisions stay declared until an explicit edit upgrades them", async ({
	page,
}) => {
	await page.goto("/");
	const downloadPromise = page.waitForEvent("download");
	await page.getByRole("button", { name: "Export", exact: true }).click();
	const download = await downloadPromise;
	const path = await download.path();
	if (path === null) throw new Error("export did not produce a local file");
	const legacy = (await readFile(path, "utf8")).replace(
		'"ordered-map-trace/3"',
		'"ordered-map-trace/2"',
	);
	await page.locator('input[type="file"]').setInputFiles({
		name: "legacy-scenario.json",
		mimeType: "application/json",
		buffer: Buffer.from(legacy),
	});
	await expect(page.getByTestId("revision-status")).toHaveText(
		"Legacy input · current trace",
	);
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await expect(page.getByTestId("revision-status")).toHaveText(
		"Legacy input · current trace",
	);

	await page
		.getByRole("textbox", { name: "Scenario JSON" })
		.fill(`${legacy}\n`);
	await expect(page.getByTestId("revision-status")).toHaveText(
		"Edited · not loaded",
	);
	const editedDownloadPromise = page.waitForEvent("download");
	await page.getByRole("button", { name: "Export", exact: true }).click();
	const editedDownload = await editedDownloadPromise;
	const editedPath = await editedDownload.path();
	if (editedPath === null)
		throw new Error("edited export did not produce a file");
	const editedExport = await readFile(editedPath, "utf8");
	expect(editedExport).toContain('"trace_revision":"ordered-map-trace/3"');
	await expect(page.getByTestId("revision-status")).toHaveText(
		"Current revisions",
	);
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await expect(page.getByTestId("revision-status")).toHaveText(
		"Current revisions",
	);
	await expect(
		page.getByRole("textbox", { name: "Scenario JSON" }),
	).toContainText("ordered-map-trace/3");
});

test("invalid Scenario is rejected without replacing the last committed scene", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	const before = await page
		.getByText("3 entries", { exact: true })
		.textContent();

	await page
		.getByTestId("scenario-editor")
		.getByRole("textbox")
		.fill("{ invalid json");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("invalid");
	expect(await page.getByText("3 entries", { exact: true }).textContent()).toBe(
		before,
	);
	await page.getByRole("button", { name: "Next step" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("1 / 4");
});

test("Run trace validates edited input instead of replaying a stale scene", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await page
		.getByTestId("scenario-editor")
		.getByRole("textbox")
		.fill('{"payload": {x');
	await expect(page.getByTestId("revision-status")).toHaveText(
		"Edited · not loaded",
	);
	await page.getByRole("button", { name: "Run trace", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("invalid");
	await expect(page.getByText("3 entries", { exact: true })).toBeVisible();
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 4");
});

test("DSL diagnostics preserve Rust UTF-16 positions in the editor", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await page.getByRole("textbox", { name: "Operations DSL" }).fill("  🦀 1");
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("invalid");
	const editor = page
		.getByRole("textbox", { name: "Operations DSL" })
		.locator("xpath=ancestor::*[@data-testid='scenario-editor'][1]");
	await expect(editor).toHaveAttribute(
		"data-diagnostic-code",
		"UNKNOWN_OPERATION",
	);
	await expect(editor).toHaveAttribute("data-diagnostic-line", "1");
	await expect(editor).toHaveAttribute("data-diagnostic-column", "3");

	await page.getByRole("textbox", { name: "Operations DSL" }).fill("get 1");
	await expect(editor).not.toHaveAttribute("data-diagnostic-code", /.+/);
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("idle");
});

test("strict DSL and the single-operation form prepare a runnable Scenario", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("idle");

	await page
		.getByRole("textbox", { name: "Initial DSL" })
		.fill('# comments are accepted\ninsert 10 "ten"\ninsert 4 "four"');
	await page.getByRole("textbox", { name: "Operations DSL" }).fill("get 10");
	await page
		.getByRole("combobox", { name: "Operation", exact: true })
		.selectOption("insert");
	await page.getByRole("textbox", { name: "Operation key" }).fill("7");
	await page.getByRole("textbox", { name: "Operation value" }).fill("seven");
	await page.getByRole("button", { name: "Append", exact: true }).click();
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("idle");

	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await expect(page.getByText("2 entries", { exact: true })).toBeVisible();
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 2");
});

test("DSL changes are validated against the selected algorithm before preparation", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByLabel("Structure", { exact: true }).selectOption("veb");
	await page.getByRole("button", { name: "Parameters", exact: true }).click();
	await page.getByLabel("word bits", { exact: true }).fill("4");
	await page.getByRole("button", { name: "Apply", exact: true }).click();
	await expect(page.getByRole("dialog")).toBeHidden();
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await page
		.getByRole("textbox", { name: "Initial DSL" })
		.fill('insert 16 "outside"');
	await page.getByRole("textbox", { name: "Operations DSL" }).fill("get 0");
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("invalid");
	await expect(
		page.getByText("invalid Scenario value: key exceeds algorithm universe", {
			exact: true,
		}),
	).toBeVisible();
});

test("both generators validate their complete Scenario against the word universe", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByLabel("Structure", { exact: true }).selectOption("veb");
	await page.getByRole("button", { name: "Parameters", exact: true }).click();
	await page.getByLabel("word bits", { exact: true }).fill("4");
	await page.getByRole("button", { name: "Apply", exact: true }).click();
	await expect(page.getByRole("dialog")).toBeHidden();

	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("initial");
	await page.getByLabel("Count", { exact: true }).fill("1");
	await page.getByLabel("Key minimum", { exact: true }).fill("16");
	await page.getByLabel("Key maximum", { exact: true }).fill("16");
	await page.getByLabel("insert overwrite", { exact: true }).fill("0");
	await page
		.getByTestId("generator-dialog")
		.getByRole("button", { name: "Generate", exact: true })
		.click();
	await expect(page.getByTestId("generator-dialog")).toBeVisible();
	await expect(page.getByRole("alert")).toHaveText(
		"invalid Scenario value: key exceeds algorithm universe",
	);

	await page.getByLabel("Key minimum", { exact: true }).fill("0");
	await page.getByLabel("Key maximum", { exact: true }).fill("0");
	await page
		.getByTestId("generator-dialog")
		.getByRole("button", { name: "Generate", exact: true })
		.click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();

	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("operations");
	await page.getByLabel("Count", { exact: true }).fill("1");
	await page.getByLabel("Key minimum", { exact: true }).fill("16");
	await page.getByLabel("Key maximum", { exact: true }).fill("16");
	for (const label of ["remove", "get", "lower_bound"]) {
		await page.getByLabel(label, { exact: true }).fill("0");
	}
	await page.getByLabel("insert", { exact: true }).fill("1");
	await page.getByLabel("insert overwrite", { exact: true }).fill("0");
	await page
		.getByTestId("generator-dialog")
		.getByRole("button", { name: "Generate", exact: true })
		.click();
	await expect(page.getByTestId("generator-dialog")).toBeVisible();
	await expect(page.getByRole("alert")).toHaveText(
		"invalid Scenario value: key exceeds algorithm universe",
	);
});

test("generator validation stays visible beside the fields and can be corrected", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	for (const label of ["insert", "remove", "get", "lower_bound"]) {
		await page.getByLabel(label, { exact: true }).fill("0");
	}
	await page
		.getByTestId("generator-dialog")
		.getByRole("button", { name: "Generate", exact: true })
		.click();
	await expect(page.getByTestId("generator-dialog")).toBeVisible();
	await expect(page.getByRole("alert")).toHaveText(
		"invalid generator setting: operation weight sum must be positive",
	);

	await page.getByLabel("insert", { exact: true }).fill("1");
	await page
		.getByTestId("generator-dialog")
		.getByRole("button", { name: "Generate", exact: true })
		.click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();
	await expect(page.getByTestId("engine-status")).toHaveText("idle");
});

test("atomic steps stay inside an operation while Shift moves the operation boundary", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("atomic");
	await page.getByRole("button", { name: "Next step" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("1 / 4");
	const firstStep = await page.getByText(/^step \d+\/\d+$/).textContent();

	await page.getByRole("button", { name: "Next step" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("1 / 4");
	expect(await page.getByText(/^step \d+\/\d+$/).textContent()).not.toBe(
		firstStep,
	);

	await page.getByTestId("pixi-host").click({ position: { x: 12, y: 12 } });
	await page.keyboard.press("Shift+ArrowRight");
	await expect(page.getByTestId("timeline-readout")).toHaveText("2 / 4");
});

test("Pause stops inside the final operation instead of replaying from the start", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("atomic");
	await page.getByTestId("pixi-host").click({ position: { x: 12, y: 12 } });
	for (const cursor of [1, 2, 3]) {
		await page.keyboard.press("Shift+ArrowRight");
		await expect(page.getByTestId("timeline-readout")).toHaveText(
			`${cursor} / 4`,
		);
	}
	await page.getByRole("button", { name: "Run trace", exact: true }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("4 / 4");
	const pause = page.getByRole("button", { name: "Pause", exact: true });
	await expect(pause).toBeVisible();
	const stepBeforePause = await page.getByText(/^step \d+\/\d+$/).textContent();
	await pause.click();
	await page.evaluate(
		() =>
			new Promise<void>((resolve) => {
				let frames = 0;
				const observe = () => {
					frames += 1;
					if (frames >= 12) resolve();
					else requestAnimationFrame(observe);
				};
				requestAnimationFrame(observe);
			}),
	);
	await expect(
		page.getByRole("button", { name: "Run trace", exact: true }),
	).toBeVisible();
	expect(await page.getByText(/^step \d+\/\d+$/).textContent()).toBe(
		stepBeforePause,
	);
	await expect(page.getByTestId("timeline-readout")).toHaveText("4 / 4");
});

test("a double rotation exposes and reverses each intermediate tree", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await page
		.getByRole("textbox", { name: "Initial DSL" })
		.fill('insert 3 "three"\ninsert 1 "one"');
	await page
		.getByRole("textbox", { name: "Operations DSL" })
		.fill('insert 2 "two"');
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("idle");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await expect(page.getByTestId("root-key")).toHaveText("root 3");
	await expect(page.getByText("2 entries", { exact: true })).toBeVisible();

	const next = page.getByRole("button", { name: "Next step" });
	await next.click();
	await expect(page.getByText("Compare 2", { exact: true })).toBeVisible();
	await expect(page.getByRole("heading", { name: "Key 3" })).toBeVisible();
	await expect(page.getByTestId("query-operand")).toHaveText("QUERY2");
	await expect(page.getByText("2 entries", { exact: true })).toBeVisible();
	await next.click();
	const traversal = await page.evaluate(
		() =>
			new Promise<{
				maxRenderedSegments: number;
				progress: number[];
			}>((resolve) => {
				const samples: number[] = [];
				let maxRenderedSegments = 0;
				const sampleRenderedEdge = () => {
					const host = document.querySelector<HTMLElement>(
						'[data-testid="pixi-host"]',
					);
					maxRenderedSegments = Math.max(
						maxRenderedSegments,
						Number(host?.dataset.renderedTraversalSegmentCount),
					);
				};
				const sample = () => {
					const value = Number(
						document.querySelector<HTMLElement>('[data-testid="pixi-host"]')
							?.dataset.traversalProgress,
					);
					samples.push(value);
					sampleRenderedEdge();
					if (samples.length === 8) {
						resolve({
							maxRenderedSegments,
							progress: samples,
						});
					} else requestAnimationFrame(sample);
				};
				requestAnimationFrame(sample);
			}),
	);
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-current-edge-count",
		"1",
	);
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-traversal-edge",
		/^node:.+>node:/,
	);
	expect(traversal.progress[0]).toBeLessThan(
		traversal.progress[traversal.progress.length - 1] ?? 0,
	);
	expect(traversal.progress[traversal.progress.length - 1]).toBeLessThan(1);
	expect(traversal.maxRenderedSegments).toBeGreaterThan(0);
	expect(await page.getByText(/^Follow a link/, { exact: false }).count()).toBe(
		0,
	);
	await expect(page.getByText("Compare 2", { exact: true })).toBeVisible();
	await expect(page.getByRole("heading", { name: "Key 1" })).toBeVisible();
	await expect(page.getByTestId("query-operand")).toHaveText("QUERY2");
	await next.click();
	await expect(page.getByText("Insert 2", { exact: true })).toBeVisible();
	await expect(page.getByText("3 entries", { exact: true })).toBeVisible();
	await next.click();
	await expect(
		page.getByText("Update metadata", { exact: true }),
	).toBeVisible();
	await next.click();
	await expect(
		page.getByText("Update metadata", { exact: true }),
	).toBeVisible();

	await next.click();
	await expect(page.getByText("Rotate left", { exact: true })).toBeVisible();
	await expect(page.getByTestId("root-key")).toHaveText("root 3");
	await expect(
		page.getByText("rotations", { exact: true }).locator("..").getByText("1"),
	).toBeVisible();

	await next.click();
	await expect(page.getByText("Rotate right", { exact: true })).toBeVisible();
	await expect(page.getByTestId("root-key")).toHaveText("root 2");
	await expect(
		page.getByText("rotations", { exact: true }).locator("..").getByText("2"),
	).toBeVisible();

	await page.getByRole("button", { name: "Previous step" }).click();
	await expect(page.getByText("Rotate left", { exact: true })).toBeVisible();
	await expect(page.getByTestId("root-key")).toHaveText("root 3");
	await expect(
		page.getByText("rotations", { exact: true }).locator("..").getByText("1"),
	).toBeVisible();
});

test("B-tree cascades expose every split and merge as a separate animation", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByLabel("Structure", { exact: true }).selectOption("b-tree");
	await page.getByRole("button", { name: "Parameters", exact: true }).click();
	await page.getByLabel("min degree", { exact: true }).fill("2");
	await page.getByRole("button", { name: "Apply", exact: true }).click();
	await expect(page.getByRole("dialog")).toBeHidden();
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	const initial = page.getByRole("textbox", { name: "Initial DSL" });
	const operations = page.getByRole("textbox", { name: "Operations DSL" });
	const apply = page.getByRole("button", { name: "Apply DSL", exact: true });
	const load = page.getByRole("button", { name: "Load", exact: true });
	const next = page.getByRole("button", { name: "Next step" });
	const host = page.getByTestId("pixi-host");
	const topology = (root: string[], edges: [string[], string, string[]][]) => ({
		edges: edges.sort((left, right) =>
			JSON.stringify(left).localeCompare(JSON.stringify(right)),
		),
		root,
	});

	const observeCascade = async (
		initialSize: number,
		operation: string,
		title: string,
		metricName: "splits" | "merges",
		expectedTopologies: ReturnType<typeof topology>[],
	) => {
		await initial.fill(
			Array.from(
				{ length: initialSize },
				(_, key) => `insert ${key} "${key}"`,
			).join("\n"),
		);
		await operations.fill(operation);
		await apply.click();
		await expect(page.getByTestId("engine-status")).toHaveText("idle");
		await load.click();
		await expect(page.getByTestId("engine-status")).toHaveText("ready");
		await page
			.getByRole("combobox", { name: "Playback granularity" })
			.selectOption("atomic");
		let observed = 0;
		const metric = page
			.getByText(metricName, { exact: true })
			.locator("..")
			.locator("dd");
		let previousMetric = Number(await metric.textContent());
		for (let step = 0; step < 256 && observed < 2; step += 1) {
			await expect(next).toBeEnabled();
			const stepReadout = page.getByText(/^step \d+\/\d+$/);
			const previousStep = (await stepReadout.count())
				? await stepReadout.textContent()
				: null;
			await next.click();
			if (previousStep === null) {
				await expect(stepReadout).toBeVisible();
			} else {
				await expect
					.poll(async () => stepReadout.textContent())
					.not.toBe(previousStep);
			}
			const currentMetric = Number(await metric.textContent());
			if (currentMetric !== previousMetric) {
				expect(currentMetric).toBe(previousMetric + 1);
				previousMetric = currentMetric;
				observed += 1;
				await expect(page.getByText(title, { exact: true })).toBeVisible();
				await expect(host).toHaveAttribute(
					"data-mutation-node-count",
					/[1-9]\d*/,
				);
				await expect(host).toHaveAttribute(
					"data-structural-transition",
					"active",
				);
				await expect(host).toHaveAttribute(
					"data-structural-transition",
					"settled",
				);
				const encodedTopology = await host.getAttribute("data-btree-topology");
				expect(encodedTopology).not.toBeNull();
				expect(JSON.parse(encodedTopology as string)).toEqual(
					expectedTopologies[observed - 1],
				);
			}
		}
		expect(observed).toBe(2);
	};

	await observeCascade(17, 'insert 17 "17"', "Split structure", "splits", [
		topology(
			["7"],
			[
				[["1"], "child-0", ["0"]],
				[["1"], "child-1", ["2"]],
				[["3"], "child-0", ["1"]],
				[["3"], "child-1", ["5"]],
				[["5"], "child-0", ["4"]],
				[["5"], "child-1", ["6"]],
				[["7"], "child-0", ["3"]],
				[["7"], "child-1", ["11"]],
				[["9"], "child-0", ["8"]],
				[["9"], "child-1", ["10"]],
				[["11"], "child-0", ["9"]],
				[["11"], "child-1", ["13"]],
				[["13"], "child-0", ["12"]],
				[["13"], "child-1", ["14", "15", "16"]],
			],
		),
		topology(
			["7"],
			[
				[["1"], "child-0", ["0"]],
				[["1"], "child-1", ["2"]],
				[["3"], "child-0", ["1"]],
				[["3"], "child-1", ["5"]],
				[["5"], "child-0", ["4"]],
				[["5"], "child-1", ["6"]],
				[["7"], "child-0", ["3"]],
				[["7"], "child-1", ["11"]],
				[["9"], "child-0", ["8"]],
				[["9"], "child-1", ["10"]],
				[["11"], "child-0", ["9"]],
				[["11"], "child-1", ["13", "15"]],
				[["13", "15"], "child-0", ["12"]],
				[["13", "15"], "child-1", ["14"]],
				[["13", "15"], "child-2", ["16"]],
			],
		),
	]);
	await observeCascade(9, "remove 0", "Merge structures", "merges", [
		topology(
			[],
			[
				[[], "child-0", ["1", "3", "5"]],
				[["1", "3", "5"], "child-0", ["0"]],
				[["1", "3", "5"], "child-1", ["2"]],
				[["1", "3", "5"], "child-2", ["4"]],
				[["1", "3", "5"], "child-3", ["6", "7", "8"]],
			],
		),
		topology(
			[],
			[
				[[], "child-0", ["3", "5"]],
				[["3", "5"], "child-0", ["0", "1", "2"]],
				[["3", "5"], "child-1", ["4"]],
				[["3", "5"], "child-2", ["6", "7", "8"]],
			],
		),
	]);
});

test("overwrite and remove state become visible only at their owning events", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await page
		.getByRole("textbox", { name: "Initial DSL" })
		.fill('insert 1 "before"');
	await page
		.getByRole("textbox", { name: "Operations DSL" })
		.fill('insert 1 "after"\nremove 1');
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("atomic");

	const next = page.getByRole("button", { name: "Next step" });
	const value = page
		.getByText("Value", { exact: true })
		.locator("..")
		.locator("dd");
	await next.click();
	await expect(page.getByText("Compare 1", { exact: true })).toBeVisible();
	await expect(value).toHaveText("before");
	for (let step = 0; step < 12; step += 1) {
		if (await page.getByText("Overwrite 1", { exact: true }).isVisible()) {
			break;
		}
		await expect(value).toHaveText("before");
		await next.click();
	}
	await expect(page.getByText("Overwrite 1", { exact: true })).toBeVisible();
	await expect(value).toHaveText("after");
	await expect(page.getByText("1 entries", { exact: true })).toBeVisible();

	for (let step = 0; step < 16; step += 1) {
		if (await page.getByText("Remove 1", { exact: true }).isVisible()) {
			break;
		}
		await expect(page.getByText("1 entries", { exact: true })).toBeVisible();
		await next.click();
	}
	await expect(page.getByText("Remove 1", { exact: true })).toBeVisible();
	await expect(page.getByText("0 entries", { exact: true })).toBeVisible();
	await expect(page.getByRole("heading", { name: "No entry" })).toBeVisible();

	await page.getByRole("button", { name: "Previous step" }).click();
	await expect(page.getByText("Compare 1", { exact: true })).toBeVisible();
	await expect(value).toHaveText("after");
	await expect(page.getByText("1 entries", { exact: true })).toBeVisible();
});

test("an explicit node selection overrides the last query result in Inspector", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await page.getByTestId("pixi-host").click({ position: { x: 12, y: 12 } });
	await page.keyboard.press("Shift+ArrowRight");
	await expect(page.getByTestId("timeline-readout")).toHaveText("1 / 4");
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("atomic");
	const next = page.getByRole("button", { name: "Next step" });
	for (let step = 0; step < 32; step += 1) {
		await next.click();
		if (await page.getByText("Operation result", { exact: true }).isVisible())
			break;
	}
	await expect(page.getByRole("heading", { name: "Key 12" })).toBeVisible();
	const host = page.getByTestId("pixi-host");
	await host.click({
		position: {
			x: Number(await host.getAttribute("data-root-screen-x")),
			y: Number(await host.getAttribute("data-root-screen-y")),
		},
	});
	await expect(page.getByRole("heading", { name: "Key 8" })).toBeVisible();
	await expect(
		page.getByText("Value", { exact: true }).locator("..").locator("dd"),
	).toHaveText("root");
});

test("vEB traversal keeps auxiliary identities and colors the declared edge", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByLabel("Structure", { exact: true }).selectOption("veb");
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await page
		.getByRole("textbox", { name: "Initial DSL" })
		.fill(
			'insert 1 "one"\ninsert 2 "two"\ninsert 7 "seven"\ninsert 15 "fifteen"',
		);
	await page.getByRole("textbox", { name: "Operations DSL" }).fill("get 7");
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("atomic");

	const next = page.getByRole("button", { name: "Next step" });
	const host = page.getByTestId("pixi-host");
	await next.click();

	await expect(host).toHaveAttribute("data-current-edge-count", "1");
	await expect(host).toHaveAttribute(
		"data-traversal-edge",
		/^auxiliary:\d+:\d+>auxiliary:\d+:\d+$/,
	);
	await expect(host).toHaveAttribute("data-active-key", /^auxiliary:/);
	await expect(host).toHaveAttribute("data-visited-node-count", "3");
});

test("remove colors the deleted node and disappearing edge from before-state", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await page
		.getByRole("textbox", { name: "Initial DSL" })
		.fill('insert 4 "root"\ninsert 6 "child"');
	await page.getByRole("textbox", { name: "Operations DSL" }).fill("remove 6");
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("atomic");

	const next = page.getByRole("button", { name: "Next step" });
	for (let step = 0; step < 32; step += 1) {
		await next.click();
		if (await page.getByText("Remove 6", { exact: true }).isVisible()) {
			break;
		}
	}

	const host = page.getByTestId("pixi-host");
	await expect(page.getByText("Remove 6", { exact: true })).toBeVisible();
	await expect(host).toHaveAttribute("data-mutation-node-count", "2");
	await expect(host).toHaveAttribute("data-current-edge-count", "1");
});

test("batched detail keeps a removed node visible for its mutation event", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await page
		.getByRole("textbox", { name: "Initial DSL" })
		.fill(
			Array.from({ length: 501 }, (_, key) => `insert ${key} "${key}"`).join(
				"\n",
			),
		);
	await page
		.getByRole("textbox", { name: "Operations DSL" })
		.fill("remove 500");
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("atomic");

	const host = page.getByTestId("pixi-host");
	await expect(host).toHaveAttribute("data-render-strategy", "batched");
	const next = page.getByRole("button", { name: "Next step" });
	for (let step = 0; step < 64; step += 1) {
		await next.click();
		if (await page.getByText("Remove 500", { exact: true }).isVisible()) {
			break;
		}
	}

	await expect(page.getByText("Remove 500", { exact: true })).toBeVisible();
	await expect(host).toHaveAttribute("data-dense-mutation-ghost-count", "1");
});

test("transport controls expose both timeline boundaries", async ({ page }) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await expect(page.getByRole("button", { name: "First item" })).toBeDisabled();
	await expect(
		page.getByRole("button", { name: "Previous step" }),
	).toBeDisabled();

	await page.getByRole("button", { name: "Last item" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("4 / 4");
	await expect(page.getByRole("button", { name: "Last item" })).toBeDisabled();
	await expect(page.getByRole("button", { name: "Next step" })).toBeDisabled();

	await page.getByRole("button", { name: "First item" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 4");
});

test("rapid timeline input settles on the latest requested item", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");

	await page.getByLabel("Timeline position").evaluate((element) => {
		const slider = element as HTMLInputElement;
		const setNativeValue = Object.getOwnPropertyDescriptor(
			HTMLInputElement.prototype,
			"value",
		)?.set;
		if (setNativeValue === undefined) {
			throw new Error("range input has no native value setter");
		}
		for (const value of ["1", "3", "2"]) {
			setNativeValue.call(slider, value);
			slider.dispatchEvent(new Event("input", { bubbles: true }));
		}
	});
	await expect(page.getByTestId("timeline-readout")).toHaveText("2 / 4");
});

test("a completed trace can replay from the beginning", async ({ page }) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await page.getByRole("button", { name: "Last item" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("4 / 4");
	await expect(
		page.getByRole("button", { name: "Replay trace", exact: true }),
	).toBeVisible();

	await page.getByRole("button", { name: "Replay trace", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("playing");
	await expect
		.poll(async () => {
			const text = await page.getByTestId("timeline-readout").textContent();
			return Number(text?.split("/")[0]?.trim());
		})
		.toBeGreaterThan(0);
});

test("a committed operation synchronizes trace explanation, active entity, and pseudocode", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await expect(page.getByTestId("seek-index-progress")).toBeHidden();
	await expect(
		page.getByText("Ready to inspect", { exact: true }),
	).toBeVisible();

	await page.getByRole("button", { name: "Next step" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("1 / 4");
	await expect(page.getByText("Compare 6", { exact: true })).toBeVisible();
	await expect(page.getByRole("heading", { name: "Key 8" })).toBeVisible();
	const pseudocode = page.getByRole("region", {
		name: "Synchronized pseudocode",
	});
	await expect(pseudocode.getByText("3", { exact: true })).toBeVisible();
	await expect(
		pseudocode.getByText("order ← compare(key, node.key)", { exact: true }),
	).toBeVisible();
	await expect(
		pseudocode.getByText("event 1 · compare", { exact: true }),
	).toBeVisible();
});

test("Rust generator materializes weighted operations and persisted provenance", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeVisible();
	await page.getByLabel("Count", { exact: true }).fill("20");
	await page.getByLabel("insert", { exact: true }).fill("0");
	await page.getByLabel("remove", { exact: true }).fill("0");
	await page.getByLabel("get", { exact: true }).fill("3");
	await page.getByLabel("lower_bound", { exact: true }).fill("1");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();
	await expect(page.getByTestId("engine-status")).toHaveText("idle");

	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 20");

	const downloadPromise = page.waitForEvent("download");
	await page.getByRole("button", { name: "Export", exact: true }).click();
	const download = await downloadPromise;
	const path = await download.path();
	expect(path).not.toBeNull();
	const canonical = await readFile(path as string, "utf8");
	const generatedScenario = JSON.parse(canonical) as {
		payload: {
			operations: {
				items: { op: string }[];
				provenance?: {
					generator_revision: string;
					payload: {
						digest_algorithm: string;
						materialized_digest: string;
						spec: { count: number; seed: string };
						stats: {
							item_count: number;
							insert_count: number;
							remove_count: number;
							get_count: number;
							lower_bound_count: number;
						};
					};
				};
			};
		};
	};
	const provenance = generatedScenario.payload.operations.provenance;
	if (provenance === undefined) {
		throw new Error("generated operations are missing provenance");
	}
	expect({
		generatorRevision: provenance.generator_revision,
		digestAlgorithm: provenance.payload.digest_algorithm,
		digestIsLowercaseSha256:
			provenance.payload.materialized_digest.match(/^[0-9a-f]{64}$/) !== null,
		seed: provenance.payload.spec.seed,
		requestedCount: provenance.payload.spec.count,
		materializedCount: provenance.payload.stats.item_count,
		operationCounts: {
			insert: provenance.payload.stats.insert_count,
			remove: provenance.payload.stats.remove_count,
			get: provenance.payload.stats.get_count,
			lowerBound: provenance.payload.stats.lower_bound_count,
		},
		materializedKinds: [
			...new Set(
				generatedScenario.payload.operations.items.map((item) => item.op),
			),
		].sort(),
	}).toEqual({
		generatorRevision: "ordered-map-generator/1",
		digestAlgorithm: "sha256",
		digestIsLowercaseSha256: true,
		seed: "42",
		requestedCount: 20,
		materializedCount: 20,
		operationCounts: { insert: 0, remove: 0, get: 15, lowerBound: 5 },
		materializedKinds: ["get", "lower_bound"],
	});
	expect(canonical).not.toContain("\n");

	await page.getByRole("textbox", { name: "Scenario JSON" }).fill(canonical);
	const editedDownloadPromise = page.waitForEvent("download");
	await page.getByRole("button", { name: "Export", exact: true }).click();
	const editedDownload = await editedDownloadPromise;
	const editedPath = await editedDownload.path();
	expect(editedPath).not.toBeNull();
	const editedScenario = JSON.parse(
		await readFile(editedPath as string, "utf8"),
	) as { payload: { operations: Record<string, unknown> } };
	expect(editedScenario.payload.operations).not.toHaveProperty("provenance");
});

test("100,000-operation seeks yield through observable progress", async ({
	page,
}) => {
	test.setTimeout(120_000);
	await page.goto("/");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page.getByLabel("Count", { exact: true }).fill("100000");
	await page.getByLabel("Key maximum", { exact: true }).fill("200000");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 100000");

	await page.evaluate(() => {
		const state = window as unknown as { __seekProgressSeen: boolean };
		state.__seekProgressSeen = false;
		const target = document.querySelector('[data-testid="seek-progress"]');
		new MutationObserver(() => {
			if (target?.textContent?.includes("seeking")) {
				state.__seekProgressSeen = true;
			}
		}).observe(target as Node, {
			characterData: true,
			childList: true,
			subtree: true,
		});
	});

	await page.getByLabel("Timeline position").focus();
	await page.keyboard.press("End");
	await expect
		.poll(() =>
			page.evaluate(
				() =>
					(window as unknown as { __seekProgressSeen: boolean })
						.__seekProgressSeen,
			),
		)
		.toBe(true);
	const cancellationDownload = page.waitForEvent("download");
	await page.getByRole("button", { name: "Export", exact: true }).click();
	await cancellationDownload;
	await expect(page.getByTestId("engine-status")).toHaveText("paused");
	await page.getByRole("button", { name: "Next step" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("1 / 100000");
	await page.getByRole("button", { name: "First item" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 100000");
	await page.evaluate(() => {
		(window as unknown as { __seekProgressSeen: boolean }).__seekProgressSeen =
			false;
	});

	const forwardStarted = Date.now();
	await page.getByLabel("Timeline position").focus();
	await page.keyboard.press("End");
	await expect(page.getByTestId("timeline-readout")).toHaveText(
		"100000 / 100000",
	);
	expect(
		await page.evaluate(
			() =>
				(window as unknown as { __seekProgressSeen: boolean })
					.__seekProgressSeen,
		),
	).toBe(true);
	expect(Date.now() - forwardStarted).toBeLessThan(10_000);

	const backwardStarted = Date.now();
	await page.getByRole("button", { name: "First item" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 100000");
	expect(Date.now() - backwardStarted).toBeLessThan(2_000);
});
