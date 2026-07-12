import { expect, test } from "./browser-test";

type BrowserQualityState = {
	cspViolations: string[];
	longTasks: { duration: number; startTime: number }[];
	resetLongTasks: () => void;
};

declare global {
	interface Window {
		__browserQuality: BrowserQualityState;
		__engineWorker?: Worker;
		__enginePostCount?: number;
		__heldReady?: () => void;
		__holdNextReady?: boolean;
		__lastEngineGeneration?: number;
		__readyDecodeRaceTriggered?: boolean;
		__rendererRecoveryAckCount?: number;
		__rendererRecoveryFrameCount?: number;
		__rendererRecoverySeekCount?: number;
		__seekDecodeRaceTriggered?: boolean;
	}
}

test.beforeEach(async ({ page }) => {
	await page.addInitScript(() => {
		const state: BrowserQualityState = {
			cspViolations: [],
			longTasks: [],
			resetLongTasks: () => undefined,
		};
		document.addEventListener("securitypolicyviolation", (event) => {
			state.cspViolations.push(
				`${event.violatedDirective}:${event.blockedURI}`,
			);
		});
		if (PerformanceObserver.supportedEntryTypes.includes("longtask")) {
			let epoch = 0;
			const observer = new PerformanceObserver((list) => {
				for (const entry of list.getEntries()) {
					if (entry.startTime >= epoch) {
						state.longTasks.push({
							duration: entry.duration,
							startTime: entry.startTime,
						});
					}
				}
			});
			observer.observe({ entryTypes: ["longtask"] });
			state.resetLongTasks = () => {
				epoch = performance.now();
				state.longTasks.length = 0;
				observer.takeRecords();
			};
		}
		window.__browserQuality = state;
	});
});

test("production CSP supports the selected UI stack", async ({ page }) => {
	await page.goto("/");
	await expect(
		page.getByRole("heading", { name: "Ordered Map" }),
	).toBeVisible();
	await expect(page.getByTestId("scenario-editor")).toBeVisible();
	await expect(page.getByTestId("structure-canvas")).toBeVisible();
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-renderer",
		"webgl",
	);

	await page.getByRole("button", { name: "Generate" }).click();
	await expect(page.getByTestId("generator-dialog")).toBeVisible();
	await page.getByRole("button", { name: "Cancel" }).click();

	expect(
		await page.evaluate(async () => {
			const module = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]);
			const result = await WebAssembly.instantiate(module);
			return result.instance instanceof WebAssembly.Instance;
		}),
	).toBe(true);
	expect(
		await page.evaluate(() => window.__browserQuality.cspViolations),
	).toEqual([]);
});

test("the WebGL structure has a keyboard and screen-reader navigation surface", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	const navigation = page.getByRole("listbox", {
		name: "Structure navigation",
	});
	await expect(page.getByTestId("structure-canvas")).toHaveAttribute(
		"aria-hidden",
		"true",
	);
	await expect(navigation).toHaveAttribute(
		"aria-activedescendant",
		/^structure-navigation-node-/,
	);

	await navigation.focus();
	await page.keyboard.press("Home");
	await expect(page.getByRole("heading", { name: "Key 3" })).toBeVisible();
	const firstActiveId = await navigation.getAttribute("aria-activedescendant");
	const activeOption = navigation.getByRole("option");
	await expect(activeOption).toHaveAccessibleName(/binary-node; keys 3;/);
	await expect(activeOption).toHaveAttribute("aria-posinset", "1");
	await expect(activeOption).toHaveAttribute("aria-setsize", "3");
	await page.keyboard.press("ArrowRight");
	await expect(page.getByRole("heading", { name: "Key 8" })).toBeVisible();
	expect(await navigation.getAttribute("aria-activedescendant")).not.toBe(
		firstActiveId,
	);
	await page.keyboard.press("End");
	await expect(page.getByRole("heading", { name: "Key 12" })).toBeVisible();
});

test("light-theme pseudocode metadata keeps WCAG AA text contrast", async ({
	page,
}) => {
	await page.emulateMedia({ colorScheme: "light" });
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await page.getByRole("button", { name: "Next step" }).click();
	const minimumContrast = await page
		.locator(".code-block")
		.evaluate((block) => {
			const luminance = (color: string) => {
				const channels = color
					.match(/[\d.]+/g)
					?.slice(0, 3)
					.map(Number)
					.map((channel) => channel / 255)
					.map((channel) =>
						channel <= 0.04045
							? channel / 12.92
							: ((channel + 0.055) / 1.055) ** 2.4,
					);
				if (channels === undefined || channels.length !== 3) return 0;
				const [red = 0, green = 0, blue = 0] = channels;
				return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
			};
			const background = luminance(getComputedStyle(block).backgroundColor);
			return Math.min(
				...[...block.querySelectorAll("span, small")].map((element) => {
					const foreground = luminance(getComputedStyle(element).color);
					return (
						(Math.max(background, foreground) + 0.05) /
						(Math.min(background, foreground) + 0.05)
					);
				}),
			);
		});
	expect(minimumContrast).toBeGreaterThanOrEqual(4.5);
});

test("a generation change after create decoding starts rejects the stale frame", async ({
	page,
}) => {
	await page.addInitScript(() => {
		window.__readyDecodeRaceTriggered = false;
		const NativeWorker = window.Worker;
		class DecodeRaceWorker extends NativeWorker {
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
							if (
								window.__readyDecodeRaceTriggered === false &&
								typeof data === "object" &&
								data !== null &&
								"kind" in data &&
								data.kind === "ready"
							) {
								window.__readyDecodeRaceTriggered = true;
								deliver();
								const select = document.querySelector<HTMLSelectElement>(
									'select[aria-label="Structure"]',
								);
								if (select === null)
									throw new Error("Structure selector is missing");
								select.value = "b-tree";
								select.dispatchEvent(new Event("change", { bubbles: true }));
								return;
							}
							deliver();
						},
						options,
					);
				}) as Worker["addEventListener"];
			}
		}
		window.Worker = DecodeRaceWorker;
	});
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("idle");
	await expect(page.getByLabel("Structure", { exact: true })).toHaveValue(
		"b-tree",
	);
	await expect(page.getByTestId("engine-status")).toHaveText("idle");

	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await page.getByRole("button", { name: "Next step" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("1 / 4");
});

test("a generation change after seek decoding starts keeps the old boundary", async ({
	page,
}) => {
	await page.addInitScript(() => {
		window.__seekDecodeRaceTriggered = false;
		const NativeWorker = window.Worker;
		class SeekDecodeRaceWorker extends NativeWorker {
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
							if (
								window.__seekDecodeRaceTriggered === false &&
								typeof data === "object" &&
								data !== null &&
								"kind" in data &&
								data.kind === "seeked"
							) {
								window.__seekDecodeRaceTriggered = true;
								deliver();
								const exportButton = [
									...document.querySelectorAll("button"),
								].find((button) => button.textContent?.trim() === "Export");
								if (exportButton === undefined)
									throw new Error("Export is missing");
								exportButton.click();
								return;
							}
							deliver();
						},
						options,
					);
				}) as Worker["addEventListener"];
			}
		}
		window.Worker = SeekDecodeRaceWorker;
	});
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	const download = page.waitForEvent("download");
	await page.getByRole("button", { name: "Last item" }).click();
	await download;
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 4");
	await page.getByRole("button", { name: "Next step" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("1 / 4");
});

test("minimum desktop layout keeps controls and inspector content readable", async ({
	page,
}) => {
	await page.setViewportSize({ width: 900, height: 700 });
	await page.goto("/");
	await page.getByRole("combobox", { name: "Structure" }).selectOption("veb");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");

	await expect(page.getByRole("button", { name: "Run trace" })).toBeVisible();
	await expect(page.getByRole("heading", { name: "No entry" })).toBeVisible();
	await expect(page.getByRole("button", { name: "Last item" })).toBeVisible();
	expect(
		await page
			.getByRole("combobox", { name: "Playback granularity" })
			.evaluate((select) => select.getBoundingClientRect().width),
	).toBeGreaterThanOrEqual(70);
	expect(
		await page.evaluate(
			() => document.documentElement.scrollWidth <= window.innerWidth,
		),
	).toBe(true);

	const propertyRows = page.locator(".property-list > div");
	for (let index = 0; index < (await propertyRows.count()); index += 1) {
		const row = propertyRows.nth(index);
		const [term, definition] = await Promise.all([
			row.locator("dt").boundingBox(),
			row.locator("dd").boundingBox(),
		]);
		expect(term).not.toBeNull();
		expect(definition).not.toBeNull();
		if (term === null || definition === null) {
			continue;
		}
		expect(
			await row.evaluate((element) =>
				Array.from(element.children).every(
					(child) =>
						child instanceof HTMLElement &&
						child.scrollWidth <= child.clientWidth,
				),
			),
		).toBe(true);
		expect(
			term.x + term.width + 4 <= definition.x ||
				term.y + term.height + 2 <= definition.y,
		).toBe(true);
	}

	const canvasHeading = page.locator(".canvas-heading");
	for (const name of ["Fit tree", "Follow execution"]) {
		const button = page.getByRole("button", { name, exact: true });
		await expect(button).toBeVisible();
		const [headingBox, buttonBox] = await Promise.all([
			canvasHeading.boundingBox(),
			button.boundingBox(),
		]);
		expect(headingBox).not.toBeNull();
		expect(buttonBox).not.toBeNull();
		if (headingBox !== null && buttonBox !== null) {
			expect(buttonBox.x).toBeGreaterThanOrEqual(headingBox.x);
			expect(buttonBox.x + buttonBox.width).toBeLessThanOrEqual(
				headingBox.x + headingBox.width,
			);
		}
	}
	expect(
		await canvasHeading.evaluate((heading) =>
			Array.from(heading.querySelectorAll<HTMLElement>(".canvas-meta > *"))
				.filter((element) => element.offsetParent !== null)
				.every(
					(element) =>
						element.scrollWidth <= element.clientWidth &&
						element.scrollHeight <= element.clientHeight,
				),
		),
	).toBe(true);
});

test("modal actions stay above persistent playback controls", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	const generate = page
		.getByTestId("generator-dialog")
		.getByRole("button", { name: "Generate", exact: true });
	await expect(generate).toBeVisible();
	await expect
		.poll(() =>
			generate.evaluate((button) => {
				const bounds = button.getBoundingClientRect();
				const target = document.elementFromPoint(
					bounds.left + bounds.width / 2,
					bounds.top + bounds.height / 2,
				);
				return target === button || button.contains(target);
			}),
		)
		.toBe(true);
});

test("WebGL context loss and restore are observable", async ({ page }) => {
	await page.addInitScript(() => {
		window.__rendererRecoveryAckCount = 0;
		window.__rendererRecoveryFrameCount = 0;
		window.__rendererRecoverySeekCount = 0;
		const NativeWorker = window.Worker;
		class RecoveryObservableWorker extends NativeWorker {
			constructor(scriptURL: string | URL, options?: WorkerOptions) {
				super(scriptURL, options);
				this.addEventListener("message", (event: MessageEvent<unknown>) => {
					const data = event.data;
					if (
						typeof data === "object" &&
						data !== null &&
						"kind" in data &&
						data.kind === "seeked"
					) {
						window.__rendererRecoveryFrameCount =
							(window.__rendererRecoveryFrameCount ?? 0) + 1;
					}
				});
				const post = this.postMessage.bind(this);
				this.postMessage = ((message: unknown, transfer?: Transferable[]) => {
					if (
						typeof message === "object" &&
						message !== null &&
						"kind" in message &&
						message.kind === "seek"
					) {
						window.__rendererRecoverySeekCount =
							(window.__rendererRecoverySeekCount ?? 0) + 1;
					}
					if (
						typeof message === "object" &&
						message !== null &&
						"kind" in message &&
						message.kind === "current-ack" &&
						"accepted" in message &&
						message.accepted === true
					) {
						window.__rendererRecoveryAckCount =
							(window.__rendererRecoveryAckCount ?? 0) + 1;
					}
					post(message, transfer ?? []);
				}) as Worker["postMessage"];
			}
		}
		window.Worker = RecoveryObservableWorker;
	});
	await page.goto("/");
	const host = page.getByTestId("pixi-host");
	await expect(host).toHaveAttribute("data-renderer", "webgl");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await page.getByRole("button", { name: "Last item" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("4 / 4");
	const before = {
		entities: await host.getAttribute("data-entity-count"),
		root: await page.getByTestId("root-key").textContent(),
		seeks: await page.evaluate(() => window.__rendererRecoverySeekCount ?? 0),
		frames: await page.evaluate(() => window.__rendererRecoveryFrameCount ?? 0),
		acks: await page.evaluate(() => window.__rendererRecoveryAckCount ?? 0),
	};

	const extensionAvailable = await page
		.getByTestId("structure-canvas")
		.evaluate((canvas) => {
			const context =
				(canvas as HTMLCanvasElement).getContext("webgl2") ??
				(canvas as HTMLCanvasElement).getContext("webgl");
			const extension = context?.getExtension("WEBGL_lose_context");
			if (extension === null || extension === undefined) {
				return false;
			}
			extension.loseContext();
			window.setTimeout(() => extension.restoreContext(), 100);
			return true;
		});

	expect(extensionAvailable).toBe(true);
	await expect(host).toHaveAttribute("data-context", "restored");
	await expect
		.poll(async () =>
			page.evaluate(() => window.__rendererRecoverySeekCount ?? 0),
		)
		.toBeGreaterThan(before.seeks);
	await expect
		.poll(async () =>
			page.evaluate(() => window.__rendererRecoveryFrameCount ?? 0),
		)
		.toBeGreaterThan(before.frames);
	await expect
		.poll(async () =>
			page.evaluate(() => window.__rendererRecoveryAckCount ?? 0),
		)
		.toBeGreaterThan(before.acks);
	await expect(page.getByTestId("timeline-readout")).toHaveText("4 / 4");
	await expect(host).toHaveAttribute(
		"data-entity-count",
		before.entities ?? "",
	);
	await expect(page.getByTestId("root-key")).toHaveText(before.root ?? "");
	expect(
		await page.evaluate(() => window.__browserQuality.cspViolations),
	).toEqual([]);
});

test("a context restored after the recovery deadline remains fatal", async ({
	page,
}) => {
	await page.clock.install();
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	const canvas = page.getByTestId("structure-canvas");
	await canvas.dispatchEvent("webglcontextlost");
	await page.clock.fastForward(5_001);

	await expect(page.getByTestId("engine-status")).toHaveText("invalid");
	await expect(
		page.getByText("WebGL context was not restored within five seconds", {
			exact: true,
		}),
	).toBeVisible();
	await canvas.dispatchEvent("webglcontextrestored");
	await expect(page.getByTestId("engine-status")).toHaveText("invalid");
	await expect(
		page.getByRole("button", { name: "Load", exact: true }),
	).toBeDisabled();
	await expect(page.getByRole("button", { name: "Next step" })).toBeDisabled();
});

test("reduced motion and camera controls preserve an observable final camera", async ({
	page,
}) => {
	await page.emulateMedia({ reducedMotion: "reduce" });
	await page.goto("/");
	const host = page.getByTestId("pixi-host");
	await expect(host).toHaveAttribute("data-renderer", "webgl");
	await expect(host).toHaveAttribute("data-motion", "reduced");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await expect(host).toHaveAttribute("data-follow", "true");

	const zoomBefore = Number(await host.getAttribute("data-zoom"));
	await host.hover();
	await page.mouse.wheel(0, -300);
	await expect(host).toHaveAttribute("data-follow", "false");
	await expect
		.poll(async () => Number(await host.getAttribute("data-zoom")))
		.toBeGreaterThan(zoomBefore);

	const bounds = await host.boundingBox();
	if (bounds === null) {
		throw new Error("renderer host has no layout box");
	}
	const cameraXBefore = Number(await host.getAttribute("data-camera-x"));
	await page.mouse.move(
		bounds.x + bounds.width / 2,
		bounds.y + bounds.height / 2,
	);
	await page.mouse.down();
	await page.mouse.move(
		bounds.x + bounds.width / 2 + 48,
		bounds.y + bounds.height / 2 + 24,
		{ steps: 5 },
	);
	await page.mouse.up();
	await expect
		.poll(async () => Number(await host.getAttribute("data-camera-x")))
		.toBeGreaterThan(cameraXBefore);

	await page.getByRole("button", { name: "Fit tree", exact: true }).click();
	await expect(host).toHaveAttribute("data-follow", "true");
	await expect(page.getByRole("heading", { name: "No entry" })).toBeVisible();
	await host.click({
		position: {
			x: Number(await host.getAttribute("data-root-screen-x")),
			y: Number(await host.getAttribute("data-root-screen-y")),
		},
	});
	await expect(page.getByRole("heading", { name: "Key 8" })).toBeVisible();
});

test("one hundred nodes keep readable labels at the fitted overview", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("initial");
	await page.getByLabel("Count", { exact: true }).fill("100");
	await page.getByLabel("Key maximum", { exact: true }).fill("500");
	await page.getByLabel("insert overwrite", { exact: true }).fill("0");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page.getByRole("button", { name: "Load", exact: true }).click();

	const host = page.getByTestId("pixi-host");
	await expect(host).toHaveAttribute("data-mode", "detail");
	await expect(host).toHaveAttribute("data-visible-label-count", "100");
});

test("a single node stays at a crisp native-scale overview", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await page
		.getByRole("textbox", { name: "Initial DSL" })
		.fill('insert 42 "only"');
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await page.getByRole("button", { name: "Load", exact: true }).click();

	const host = page.getByTestId("pixi-host");
	await expect(host).toHaveAttribute("data-visible-label-count", "1");
	expect(Number(await host.getAttribute("data-zoom"))).toBeLessThanOrEqual(
		1.75,
	);
	expect(
		Number(await host.getAttribute("data-render-resolution")),
	).toBeGreaterThanOrEqual(1);
});

test("B-tree nodes expose their complete local key set", async ({ page }) => {
	await page.goto("/");
	await page.getByLabel("Structure", { exact: true }).selectOption("b-tree");
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await page
		.getByRole("textbox", { name: "Initial DSL" })
		.fill(
			'insert 10 "ten"\ninsert 20 "twenty"\ninsert 30 "thirty"\ninsert 40 "forty"',
		);
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await page.getByRole("button", { name: "Load", exact: true }).click();

	const host = page.getByTestId("pixi-host");
	await expect(host).toHaveAttribute("data-multi-entry-node-count", "1");
	expect(
		Number(await host.getAttribute("data-max-node-width")),
	).toBeGreaterThan(180);
	const bounds = await host.boundingBox();
	if (bounds === null) {
		throw new Error("B-tree renderer has no layout box");
	}
	await host.click({ position: { x: bounds.width / 2, y: bounds.height / 2 } });
	await expect(page.getByTestId("node-entry-list")).toHaveText(
		"10 · 20 · 30 · 40",
	);
});

test("vEB projection assigns every materialized node a distinct position", async ({
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
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await page.getByRole("button", { name: "Load", exact: true }).click();

	const host = page.getByTestId("pixi-host");
	await expect(host).toHaveAttribute("data-mode", "detail");
	expect(Number(await host.getAttribute("data-entity-count"))).toBeGreaterThan(
		4,
	);
	expect(
		Number(await host.getAttribute("data-minimum-node-distance")),
	).toBeGreaterThan(30);
});

test("execution tracking follows the active event and yields to manual input", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	const host = page.getByTestId("pixi-host");
	await page.getByRole("button", { name: "Next step" }).click();
	await expect(page.getByText("Compare 6", { exact: true })).toBeVisible();
	await expect(host).toHaveAttribute("data-root-screen-x", /\d/);
	await expect(host).toHaveAttribute("data-root-screen-y", /\d/);
	await host.click({
		position: {
			x: Number(await host.getAttribute("data-root-screen-x")),
			y: Number(await host.getAttribute("data-root-screen-y")),
		},
	});
	await expect(page.getByRole("heading", { name: "Key 8" })).toBeVisible();
	const tracking = page.getByRole("button", { name: "Follow execution" });
	await tracking.click();
	await expect(tracking).toHaveAttribute("aria-pressed", "true");
	await expect(host).toHaveAttribute("data-track-execution", "true");
	await expect(host).toHaveAttribute("data-tracked-key", /^node:/);
	const firstTracked = await host.getAttribute("data-tracked-key");
	const zoomBeforeTracking = await host.getAttribute("data-zoom");

	await page.getByRole("button", { name: "Next step" }).click();
	const cameraSamples = await page.evaluate(
		() =>
			new Promise<{ camera: string; distance: number; zoom: string }[]>(
				(resolve) => {
					const samples: { camera: string; distance: number; zoom: string }[] =
						[];
					const sample = () => {
						const host = document.querySelector<HTMLElement>(
							'[data-testid="pixi-host"]',
						);
						samples.push({
							camera: `${host?.dataset.cameraX}:${host?.dataset.cameraY}`,
							distance: Math.hypot(
								Number(host?.dataset.activeScreenX) -
									(host?.clientWidth ?? 0) / 2,
								Number(host?.dataset.activeScreenY) -
									(host?.clientHeight ?? 0) / 2,
							),
							zoom: host?.dataset.zoom ?? "",
						});
						if (samples.length === 8) resolve(samples);
						else requestAnimationFrame(sample);
					};
					requestAnimationFrame(sample);
				},
			),
	);
	await expect(host).toHaveAttribute("data-active-key", /^node:/);
	await expect
		.poll(async () => await host.getAttribute("data-tracked-key"))
		.toBe(await host.getAttribute("data-active-key"));
	expect(await host.getAttribute("data-tracked-key")).not.toBe(firstTracked);
	expect(
		new Set(cameraSamples.map((sample) => sample.camera)).size,
	).toBeGreaterThan(2);
	expect(
		cameraSamples.every((sample) => sample.zoom === zoomBeforeTracking),
	).toBe(true);
	for (let index = 1; index < cameraSamples.length; index += 1) {
		expect(cameraSamples[index]?.distance ?? Infinity).toBeLessThanOrEqual(
			(cameraSamples[index - 1]?.distance ?? 0) + 0.5,
		);
	}
	await expect
		.poll(async () =>
			host.evaluate((element) =>
				Math.hypot(
					Number(element.dataset.activeScreenX) - element.clientWidth / 2,
					Number(element.dataset.activeScreenY) - element.clientHeight / 2,
				),
			),
		)
		.toBeLessThan(1);
	await expect(page.getByRole("heading", { name: "Key 8" })).toBeVisible();

	await host.hover();
	await page.mouse.wheel(0, -100);
	await expect(
		page.getByRole("button", { name: "Follow execution" }),
	).toHaveAttribute("aria-pressed", "false");
	await expect(host).toHaveAttribute("data-track-execution", "false");
});

test("a Worker bootstrap failure becomes a repair-oriented UI error", async ({
	page,
}) => {
	await page.route("**/assets/engine-worker-*.js", async (route) => {
		await route.fulfill({
			body: 'throw new Error("injected worker bootstrap failure");',
			contentType: "text/javascript",
			status: 200,
		});
	});
	await page.goto("/");
	await expect(page.getByTestId("engine-status")).toHaveText("invalid");
	await expect(
		page.getByText(
			/^Visualization Worker failed: (?:Uncaught )?Error: injected worker bootstrap failure$/,
		),
	).toBeVisible();
	await expect(
		page.getByText(
			"ページを再読み込みしてください。解消しない場合は、対応ブラウザで開き直します。",
			{ exact: true },
		),
	).toBeVisible();
	await expect(
		page.getByRole("button", { name: "Generate", exact: true }),
	).toBeDisabled();
	await expect(
		page.getByRole("button", { name: "Load", exact: true }),
	).toBeDisabled();
	await expect(page.getByTestId("engine-status")).toHaveText("invalid");
});

test("a WASM bootstrap failure is fatal instead of blaming Scenario input", async ({
	page,
}) => {
	await page.route("**/assets/visualizer_engine_bg-*.wasm", async (route) => {
		await route.fulfill({
			body: "unavailable",
			contentType: "application/wasm",
			status: 200,
		});
	});
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();

	await expect(page.getByTestId("engine-status")).toHaveText("invalid");
	await expect(
		page.getByText(/^WASM engine initialization failed:/),
	).toBeVisible();
	await expect(
		page.getByText(
			"ページを再読み込みしてください。解消しない場合は、対応ブラウザで開き直します。",
			{ exact: true },
		),
	).toBeVisible();
	await expect(
		page.getByRole("button", { name: "Generate", exact: true }),
	).toBeDisabled();
	await expect(
		page.getByRole("button", { name: "Load", exact: true }),
	).toBeDisabled();
});

test("a structured runtime engine error disables every engine-dependent action", async ({
	page,
}) => {
	await page.addInitScript(() => {
		const NativeWorker = window.Worker;
		class ObservableWorker extends NativeWorker {
			constructor(scriptURL: string | URL, options?: WorkerOptions) {
				super(scriptURL, options);
				window.__engineWorker = this;
				window.__enginePostCount = 0;
				const postMessage = this.postMessage.bind(this);
				this.postMessage = ((message: unknown, transfer?: Transferable[]) => {
					window.__enginePostCount = (window.__enginePostCount ?? 0) + 1;
					postMessage(message, transfer ?? []);
				}) as Worker["postMessage"];
				this.addEventListener("message", (event: MessageEvent<unknown>) => {
					const data = event.data;
					if (
						typeof data === "object" &&
						data !== null &&
						"generation" in data &&
						typeof data.generation === "number"
					) {
						window.__lastEngineGeneration = data.generation;
					}
				});
			}
		}
		window.Worker = ObservableWorker;
	});
	await page.goto("/");
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await expect(
		page.getByRole("textbox", { name: "Initial DSL" }),
	).toBeVisible();
	await page.evaluate(() => {
		const worker = window.__engineWorker;
		const generation = window.__lastEngineGeneration;
		if (worker === undefined || generation === undefined) {
			throw new Error("observable Worker was not installed");
		}
		worker.dispatchEvent(
			new MessageEvent("message", {
				data: {
					kind: "error",
					generation,
					message: "injected runtime serialization failure",
					source: "engine",
				},
			}),
		);
	});

	await expect(page.getByTestId("engine-status")).toHaveText("invalid");
	await expect(
		page.getByText("injected runtime serialization failure", { exact: true }),
	).toBeVisible();
	await expect(
		page.getByRole("button", { name: "Generate", exact: true }),
	).toBeDisabled();
	await expect(
		page.getByRole("button", { name: "Load", exact: true }),
	).toBeDisabled();
	for (const name of [
		"First item",
		"Previous step",
		"Next step",
		"Last item",
	]) {
		await expect(page.getByRole("button", { name })).toBeDisabled();
	}
	await expect(page.getByLabel("Timeline position")).toBeDisabled();
	await expect(page.getByLabel("Playback granularity")).toBeDisabled();
	await expect(page.getByLabel("Playback speed")).toBeDisabled();
	await expect(
		page.getByRole("button", { name: "JSON", exact: true }),
	).toBeDisabled();
	await expect(
		page.getByRole("button", { name: "DSL", exact: true }),
	).toBeDisabled();
	await expect(
		page.getByRole("button", { name: "Apply DSL", exact: true }),
	).toBeDisabled();
	const before = await page.evaluate(() => window.__enginePostCount ?? -1);
	await page.evaluate(() => {
		for (const element of document.querySelectorAll<HTMLElement>(
			'button, input[type="range"], select',
		)) {
			element.click();
			element.dispatchEvent(new Event("change", { bubbles: true }));
		}
	});
	await page.waitForTimeout(50);
	expect(await page.evaluate(() => window.__enginePostCount ?? -1)).toBe(
		before,
	);
});

test("10,000 entries load through automatic summary LOD without a long task", async ({
	page,
}) => {
	test.setTimeout(60_000);
	await page.goto("/");
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-renderer",
		"webgl",
	);
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("initial");
	await page.getByLabel("Count", { exact: true }).fill("10000");
	await page.getByLabel("Key maximum", { exact: true }).fill("20000");
	await page.getByLabel("insert overwrite", { exact: true }).fill("0");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();

	await page.evaluate(
		() =>
			new Promise<void>((resolve) =>
				requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
			),
	);

	await page.evaluate(() => window.__browserQuality.resetLongTasks());
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByText("10000 entries", { exact: true })).toBeVisible();
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-mode",
		"summary",
	);
	await expect(page.getByTestId("lod-indicator")).toHaveText("Summary view");
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-rendered-count",
		"2000",
	);
	const loadLongTasks = await page.evaluate(
		() => window.__browserQuality.longTasks,
	);
	expect(loadLongTasks).toEqual([]);
});

test("a 1,000-node tree stays detailed and responsive at 32× playback", async ({
	browserName,
	page,
}) => {
	test.skip(
		browserName !== "chromium",
		"Frame pacing is measured in the designated Chromium performance browser",
	);
	test.setTimeout(90_000);
	await page.goto("/");
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-renderer",
		"webgl",
	);

	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("initial");
	await page.getByLabel("Count", { exact: true }).fill("1000");
	await page.getByLabel("Key maximum", { exact: true }).fill("2000");
	await page.getByLabel("insert overwrite", { exact: true }).fill("0");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();

	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("operations");
	await page.getByLabel("Count", { exact: true }).fill("1000");
	await page.getByLabel("insert", { exact: true }).fill("0");
	await page.getByLabel("remove", { exact: true }).fill("1");
	await page.getByLabel("get", { exact: true }).fill("1");
	await page.getByLabel("lower_bound", { exact: true }).fill("0");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();

	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	const host = page.getByTestId("pixi-host");
	await expect(host).toHaveAttribute("data-mode", "detail");
	await expect(host).toHaveAttribute("data-rendered-count", "1000");
	await expect(page.getByTestId("lod-indicator")).toHaveText("Detail view");
	const hostHeight = await host.evaluate(
		(element) => element.getBoundingClientRect().height,
	);
	expect(Number(await host.getAttribute("data-layout-span-y"))).toBeGreaterThan(
		hostHeight * 0.6,
	);
	const tracking = page.getByRole("button", { name: "Follow execution" });
	await tracking.click();
	await expect(tracking).toHaveAttribute("aria-pressed", "true");
	const trackedZoom = await host.getAttribute("data-zoom");
	await page
		.getByRole("combobox", { name: "Playback speed" })
		.selectOption("32");
	await page.evaluate(() => window.__browserQuality.resetLongTasks());
	await page.getByRole("button", { name: "Run trace", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("playing");

	const pacing = await page.evaluate(
		() =>
			new Promise<{
				frames: number;
				invalidStepReadouts: string[];
				p95GapMs: number;
			}>((resolve) => {
				const gaps: number[] = [];
				const invalidStepReadouts = new Set<string>();
				const startedAt = performance.now();
				let previous = startedAt;
				const sample = (now: number) => {
					gaps.push(now - previous);
					previous = now;
					const detail = document.querySelector(
						'[data-testid="timeline-readout"] + small',
					)?.textContent;
					const match = detail?.match(/^step (\d+)\/(\d+)$/);
					if (match != null && Number(match[1]) > Number(match[2])) {
						invalidStepReadouts.add(detail ?? "");
					}
					if (now - startedAt >= 3_000) {
						const sorted = [...gaps].sort((left, right) => left - right);
						resolve({
							frames: gaps.length,
							invalidStepReadouts: [...invalidStepReadouts],
							p95GapMs: sorted[Math.floor(sorted.length * 0.95)] ?? Infinity,
						});
						return;
					}
					requestAnimationFrame(sample);
				};
				requestAnimationFrame(sample);
			}),
	);
	const readout = await page.getByTestId("timeline-readout").textContent();
	const cursor = Number(readout?.split("/")[0]?.trim());
	expect(pacing.frames).toBeGreaterThan(100);
	expect(pacing.invalidStepReadouts).toEqual([]);
	expect(pacing.p95GapMs).toBeLessThan(35);
	const performanceBreakdown = {
		frameDecode: Number(
			await page.locator("html").getAttribute("data-frame-decode-ms"),
		),
		layout: Number(await host.getAttribute("data-layout-ms")),
		packetDecode: Number(
			await page.locator("html").getAttribute("data-packet-decode-ms"),
		),
		payloadValidation: Number(
			await page.locator("html").getAttribute("data-payload-validation-ms"),
		),
		sceneUpdate: Number(await host.getAttribute("data-scene-update-ms")),
	};
	expect(
		await page.evaluate(() => window.__browserQuality.longTasks),
		`frame timing: ${JSON.stringify(performanceBreakdown)}`,
	).toEqual([]);
	expect(cursor).toBeGreaterThanOrEqual(10);
	expect(cursor).toBeLessThan(1_000);
	await expect(page.getByTestId("engine-status")).toHaveText("playing");
	await expect(host).toHaveAttribute("data-track-execution", "true");
	await expect(host).toHaveAttribute("data-tracked-key", /^node:/);
	await expect(host).toHaveAttribute("data-zoom", trackedZoom ?? "");
});

test("an 8,000-node degenerate Splay query stays inside the WASM stack", async ({
	browserName,
	page,
}) => {
	test.skip(
		browserName !== "chromium",
		"The generated WASM stack boundary is identical across browser engines",
	);
	test.setTimeout(120_000);
	await page.goto("/");
	await page.getByLabel("Structure", { exact: true }).selectOption("splay");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("initial");
	await page.getByLabel("Count", { exact: true }).fill("8000");
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.nth(1)
		.selectOption("ascending");
	await page.getByLabel("Key maximum", { exact: true }).fill("7999");
	await page.getByLabel("insert overwrite", { exact: true }).fill("0");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();

	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await page.getByRole("textbox", { name: "Operations DSL" }).fill("get 0");
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByText("8000 entries", { exact: true })).toBeVisible();
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 1");

	await page.getByRole("button", { name: "Next step" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("1 / 1", {
		timeout: 60_000,
	});
	await expect(page.getByText("Compare 0", { exact: true })).toBeVisible();
	await expect(page.getByTestId("engine-status")).not.toHaveText("invalid");
});

test("comparison, traversal, and rotation expose their complete visual emphasis", async ({
	page,
}) => {
	await page.goto("/");
	await page.getByRole("button", { name: "DSL", exact: true }).click();
	await page
		.getByRole("textbox", { name: "Initial DSL" })
		.fill('insert 1 "one"\ninsert 2 "two"');
	await page
		.getByRole("textbox", { name: "Operations DSL" })
		.fill('insert 3 "three"');
	await page.getByRole("button", { name: "Apply DSL", exact: true }).click();
	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("atomic");

	const host = page.getByTestId("pixi-host");
	await host.evaluate((element) => {
		element.dataset.sawStructuralTransition = "false";
		element.dataset.maxEnteringEdgeCount = "0";
		element.dataset.maxExitingEdgeCount = "0";
		new MutationObserver(() => {
			if (element.dataset.structuralTransition === "active") {
				element.dataset.sawStructuralTransition = "true";
			}
			element.dataset.maxEnteringEdgeCount = String(
				Math.max(
					Number(element.dataset.maxEnteringEdgeCount),
					Number(element.dataset.enteringEdgeCount),
				),
			);
			element.dataset.maxExitingEdgeCount = String(
				Math.max(
					Number(element.dataset.maxExitingEdgeCount),
					Number(element.dataset.exitingEdgeCount),
				),
			);
		}).observe(element, {
			attributeFilter: [
				"data-structural-transition",
				"data-entering-edge-count",
				"data-exiting-edge-count",
			],
			attributes: true,
		});
	});
	await page.getByRole("button", { name: "Next step" }).click();
	await expect(host).toHaveAttribute("data-compare-node-count", "1");
	await expect(page.getByTestId("query-operand")).toHaveText("QUERY3");
	await expect(host).toHaveAttribute("data-saw-structural-transition", "false");

	for (let step = 0; step < 20; step += 1) {
		if (await page.getByText("Rotate left", { exact: true }).isVisible()) {
			break;
		}
		await page.getByRole("button", { name: "Next step" }).click();
	}
	await expect(page.getByText("Rotate left", { exact: true })).toBeVisible();
	await expect(page.getByTestId("query-operand")).toBeHidden();
	await expect(host).toHaveAttribute("data-saw-structural-transition", "true");
	await expect
		.poll(async () =>
			Number(await host.getAttribute("data-max-entering-edge-count")),
		)
		.toBeGreaterThan(0);
	await expect
		.poll(async () =>
			Number(await host.getAttribute("data-max-exiting-edge-count")),
		)
		.toBeGreaterThan(0);
	await expect(host).toHaveAttribute("data-mutation-node-count", "2");
	await expect(host).toHaveAttribute("data-current-edge-count", "1");
	await expect
		.poll(async () =>
			Number(await host.getAttribute("data-visited-node-count")),
		)
		.toBeGreaterThan(1);
	await expect
		.poll(async () =>
			Number(await host.getAttribute("data-visited-edge-count")),
		)
		.toBeGreaterThan(0);
	await expect(host).toHaveAttribute("data-structural-transition", "settled");
});

test("5,000 entities retain detail selection without losing frame pacing", async ({
	browserName,
	page,
}) => {
	test.skip(
		browserName !== "chromium",
		"Frame pacing is measured in the designated Chromium performance browser",
	);
	test.setTimeout(90_000);
	await page.goto("/");
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-renderer",
		"webgl",
	);
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("initial");
	await page.getByLabel("Count", { exact: true }).fill("5000");
	await page.getByLabel("Key maximum", { exact: true }).fill("10000");
	await page.getByLabel("insert overwrite", { exact: true }).fill("0");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();

	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByText("5000 entries", { exact: true })).toBeVisible();
	const host = page.getByTestId("pixi-host");
	await expect(host).toHaveAttribute("data-mode", "detail");
	await expect(host).toHaveAttribute("data-rendered-count", "5000");
	await expect(page.getByTestId("lod-indicator")).toHaveText("Detail view");
	await expect(host).toHaveAttribute("data-root-screen-x", /\d/);
	await expect(host).toHaveAttribute("data-root-screen-y", /\d/);
	await host.click({
		position: {
			x: Number(await host.getAttribute("data-root-screen-x")),
			y: Number(await host.getAttribute("data-root-screen-y")),
		},
	});
	await expect(host).toHaveAttribute("data-selection-marker-visible", "true");
	await expect(page.getByText("SELECTION", { exact: true })).toBeVisible();
	const tracking = page.getByRole("button", { name: "Follow execution" });
	await tracking.click();
	await page.getByRole("button", { name: "Next step" }).click();
	await expect(host).toHaveAttribute("data-track-execution", "true");
	await expect(host).toHaveAttribute("data-tracked-key", /^node:/);
	await expect
		.poll(async () => host.getAttribute("data-tracked-key"))
		.toBe(await host.getAttribute("data-active-key"));

	const pacing = await page.evaluate(
		() =>
			new Promise<{ frames: number; p95GapMs: number }>((resolve) => {
				const gaps: number[] = [];
				let previous = performance.now();
				const startedAt = previous;
				const sample = (now: number) => {
					gaps.push(now - previous);
					previous = now;
					if (now - startedAt >= 1_500) {
						const sorted = [...gaps].sort((left, right) => left - right);
						resolve({
							frames: gaps.length,
							p95GapMs: sorted[Math.floor(sorted.length * 0.95)] ?? Infinity,
						});
						return;
					}
					requestAnimationFrame(sample);
				};
				requestAnimationFrame(sample);
			}),
	);
	expect(pacing.frames).toBeGreaterThan(50);
	expect(pacing.p95GapMs).toBeLessThan(35);
});

test("an automatic LOD transition restores an understandable camera", async ({
	page,
}) => {
	test.setTimeout(90_000);
	await page.goto("/");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("initial");
	await page.getByLabel("Count", { exact: true }).fill("8000");
	await page.getByLabel("Key maximum", { exact: true }).fill("16000");
	await page.getByLabel("insert overwrite", { exact: true }).fill("0");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();

	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("operations");
	await page.getByLabel("Count", { exact: true }).fill("1");
	await page.getByLabel("Key minimum", { exact: true }).fill("16001");
	await page.getByLabel("Key maximum", { exact: true }).fill("16001");
	await page.getByLabel("remove", { exact: true }).fill("0");
	await page.getByLabel("get", { exact: true }).fill("0");
	await page.getByLabel("lower_bound", { exact: true }).fill("0");
	await page.getByLabel("insert overwrite", { exact: true }).fill("0");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();

	await page.getByRole("button", { name: "Load", exact: true }).click();
	const host = page.getByTestId("pixi-host");
	await expect(host).toHaveAttribute("data-mode", "detail");
	await host.hover();
	await page.mouse.wheel(0, -300);
	await expect(host).toHaveAttribute("data-follow", "false");

	for (let step = 0; step < 80; step += 1) {
		if ((await host.getAttribute("data-mode")) === "summary") {
			break;
		}
		await page.getByRole("button", { name: "Next step" }).click();
	}
	await expect(host).toHaveAttribute("data-mode", "summary");
	await expect(host).toHaveAttribute("data-follow", "true");
	await expect(page.getByTestId("lod-indicator")).toHaveText("Summary view");
	const hostHeight = await host.evaluate(
		(element) => element.getBoundingClientRect().height,
	);
	expect(Number(await host.getAttribute("data-layout-span-y"))).toBeGreaterThan(
		hostHeight * 0.6,
	);
});

test("10,000 entries remain bounded for the structurally expansive X-fast trie", async ({
	page,
}) => {
	test.setTimeout(90_000);
	await page.goto("/");
	await page.getByLabel("Structure", { exact: true }).selectOption("x-fast");
	await expect(page.getByTestId("engine-status")).toHaveText("idle");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("initial");
	await page.getByLabel("Count", { exact: true }).fill("10000");
	await page.getByLabel("Key maximum", { exact: true }).fill("65535");
	await page.getByLabel("insert overwrite", { exact: true }).fill("0");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();

	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("operations");
	await page.getByLabel("Count", { exact: true }).fill("2");
	await page.getByLabel("insert", { exact: true }).fill("0");
	await page.getByLabel("remove", { exact: true }).fill("0");
	await page.getByLabel("get", { exact: true }).fill("1");
	await page.getByLabel("lower_bound", { exact: true }).fill("0");
	await page.getByLabel("get hit", { exact: true }).fill("10000");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();
	await page.evaluate(
		() =>
			new Promise<void>((resolve) =>
				requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
			),
	);
	await page.evaluate(() => window.__browserQuality.resetLongTasks());

	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("ready");
	await expect(page.getByText("10000 entries", { exact: true })).toBeVisible();
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-rendered-count",
		"2000",
	);
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-mode",
		"summary",
	);
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 2");
	const cancellationDownload = page.waitForEvent("download");
	await page.evaluate(() => {
		const buttons = [...document.querySelectorAll("button")];
		const next = buttons.find(
			(button) => button.getAttribute("aria-label") === "Next step",
		);
		const exportButton = buttons.find(
			(button) => button.textContent?.trim() === "Export",
		);
		if (next === undefined || exportButton === undefined) {
			throw new Error("race controls are missing");
		}
		next.click();
		exportButton.click();
	});
	await cancellationDownload;
	await expect(page.getByTestId("engine-status")).toHaveText("paused");
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 2");
	await page.getByRole("button", { name: "Next step" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("1 / 2");
	await page.getByLabel("Playback speed").selectOption("32");
	const operationStarted = Date.now();
	await page.getByRole("button", { name: "Run trace", exact: true }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("2 / 2");
	await expect(page.getByTestId("engine-status")).not.toHaveText("invalid");
	expect(Date.now() - operationStarted).toBeLessThan(5_000);
	const entityCount = Number(
		await page.getByTestId("pixi-host").getAttribute("data-entity-count"),
	);
	const performanceBreakdown = {
		decode: Number(
			await page.locator("html").getAttribute("data-frame-decode-ms"),
		),
		packetDecode: Number(
			await page.locator("html").getAttribute("data-packet-decode-ms"),
		),
		payloadValidation: Number(
			await page.locator("html").getAttribute("data-payload-validation-ms"),
		),
		layout: Number(
			await page.getByTestId("pixi-host").getAttribute("data-layout-ms"),
		),
	};
	expect(Number.isSafeInteger(entityCount)).toBe(true);
	expect(entityCount).toBeGreaterThan(10_000);
	expect(entityCount).toBeLessThanOrEqual(250_000);
	expect(
		await page.evaluate(() => window.__browserQuality.longTasks),
		`frame timing: ${JSON.stringify(performanceBreakdown)}`,
	).toEqual([]);
});

test("a bulk Scapegoat rebuild validates without blocking the main thread", async ({
	browserName,
	page,
}) => {
	test.skip(
		browserName !== "chromium",
		"Long tasks are measured in the designated Chromium performance browser",
	);
	test.setTimeout(120_000);
	await page.goto("/");
	await page.getByLabel("Structure", { exact: true }).selectOption("scapegoat");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("initial");
	await page.getByLabel("Count", { exact: true }).fill("10000");
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.nth(1)
		.selectOption("ascending");
	await page.getByLabel("Key maximum", { exact: true }).fill("20000");
	await page.getByLabel("insert overwrite", { exact: true }).fill("0");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();

	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page
		.getByTestId("generator-dialog")
		.getByRole("combobox")
		.first()
		.selectOption("operations");
	await page.getByLabel("Count", { exact: true }).fill("3334");
	await page.getByLabel("insert", { exact: true }).fill("0");
	await page.getByLabel("remove", { exact: true }).fill("1");
	await page.getByLabel("get", { exact: true }).fill("0");
	await page.getByLabel("lower_bound", { exact: true }).fill("0");
	await page.getByLabel("remove hit", { exact: true }).fill("10000");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();

	await page.getByRole("button", { name: "Load", exact: true }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("0 / 3334");
	await page.getByLabel("Timeline position").evaluate((element) => {
		const input = element as HTMLInputElement;
		const setNativeValue = Object.getOwnPropertyDescriptor(
			HTMLInputElement.prototype,
			"value",
		)?.set;
		if (setNativeValue === undefined) {
			throw new Error("range input has no native value setter");
		}
		setNativeValue.call(input, "3333");
		input.dispatchEvent(new Event("input", { bubbles: true }));
	});
	await expect(page.getByTestId("timeline-readout")).toHaveText("3333 / 3334");
	await page
		.getByRole("combobox", { name: "Playback granularity" })
		.selectOption("atomic");
	const rebuildMetric = page
		.getByText("rebuild items", { exact: true })
		.locator("..")
		.locator("dd");
	const rebuildItemsBefore = Number(await rebuildMetric.textContent());
	await page.evaluate(() => window.__browserQuality.resetLongTasks());

	await page.getByRole("button", { name: "Next step" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("3334 / 3334");
	await expect(page.getByTestId("engine-status")).not.toHaveText("invalid");
	expect(await page.evaluate(() => window.__browserQuality.longTasks)).toEqual(
		[],
	);

	await page.evaluate(() => window.__browserQuality.resetLongTasks());
	const rebuild = page.getByText("Rebuild subtree", { exact: true });
	for (let step = 0; step < 128 && !(await rebuild.isVisible()); step += 1) {
		const readout = page.getByText(/^step \d+\/\d+$/);
		const beforeStep = await readout.textContent();
		await page.getByRole("button", { name: "Next step" }).click();
		await expect.poll(async () => readout.textContent()).not.toBe(beforeStep);
	}
	await expect(rebuild).toBeVisible();
	await expect
		.poll(
			async () =>
				Number(await rebuildMetric.textContent()) - rebuildItemsBefore,
		)
		.toBe(6_666);
	expect(
		Number(
			await page
				.getByTestId("pixi-host")
				.getAttribute("data-mutation-node-count"),
		),
	).toBeGreaterThan(5_000);
	expect(await page.evaluate(() => window.__browserQuality.longTasks)).toEqual(
		[],
	);
});

test("Run trace completes the default Scenario within ten seconds without a long task", async ({
	page,
}) => {
	await page.goto("/");
	await expect(page.getByTestId("structure-canvas")).toBeVisible();
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-renderer",
		"webgl",
	);
	await expect(
		page.getByRole("textbox", { name: "Scenario JSON" }),
	).toBeVisible();
	await page
		.getByRole("combobox", { name: "Playback speed" })
		.selectOption("4");
	await page.evaluate(
		() =>
			new Promise<void>((resolve) =>
				requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
			),
	);
	await page.evaluate(() => window.__browserQuality.resetLongTasks());
	const startedAt = await page.evaluate(() => performance.now());
	await page.getByRole("button", { name: "Run trace" }).click();
	await expect(page.getByTestId("timeline-readout")).toHaveText("4 / 4");
	await expect(page.getByTestId("engine-status")).toHaveText("paused");
	await expect(page.getByTestId("seek-progress")).toHaveText("item 4");
	await expect(page.getByText("3 entries", { exact: true })).toBeVisible();
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-entity-count",
		"3",
	);

	expect(await page.evaluate(() => window.__browserQuality.longTasks)).toEqual(
		[],
	);
	expect(
		await page.evaluate((start) => performance.now() - start, startedAt),
	).toBeLessThan(10_000);
});

test("normal playback remains responsive for a continuous ten-second window", async ({
	page,
}) => {
	test.setTimeout(45_000);
	await page.goto("/");
	await expect(page.getByTestId("pixi-host")).toHaveAttribute(
		"data-renderer",
		"webgl",
	);
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await page.getByLabel("Count", { exact: true }).fill("100");
	await page.getByRole("button", { name: "Generate", exact: true }).click();
	await expect(page.getByTestId("generator-dialog")).toBeHidden();
	await expect(page.getByTestId("engine-status")).toHaveText("idle");
	await page
		.getByRole("combobox", { name: "Playback speed" })
		.selectOption("4");
	await page.evaluate(
		() =>
			new Promise<void>((resolve) =>
				requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
			),
	);
	await page.evaluate(() => window.__browserQuality.resetLongTasks());
	await page.getByRole("button", { name: "Run trace" }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("playing");
	await expect
		.poll(async () => {
			const text = await page.getByTestId("timeline-readout").textContent();
			return Number(text?.split("/")[0]?.trim() ?? "0");
		})
		.toBeGreaterThan(0);

	await page.waitForTimeout(10_000);
	const readout = await page.getByTestId("timeline-readout").textContent();
	const cursor = Number(readout?.split("/")[0]?.trim());
	expect(Number.isSafeInteger(cursor) && cursor > 0 && cursor < 100).toBe(true);
	await expect(page.getByTestId("engine-status")).toHaveText("playing");
	expect(await page.evaluate(() => window.__browserQuality.longTasks)).toEqual(
		[],
	);
	await page.getByRole("button", { name: "Pause", exact: true }).click();
	await expect(page.getByTestId("engine-status")).toHaveText("paused");
});
