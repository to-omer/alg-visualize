import { defineConfig } from "@playwright/test";

const repositoryRoot = new URL("../..", import.meta.url).pathname;
const chromiumChannel =
	process.env.PLAYWRIGHT_CHROMIUM_CHANNEL ??
	(process.platform === "darwin" ? "chrome" : undefined);
const crossBrowser = process.env.PLAYWRIGHT_CROSS_BROWSER === "1";
const usePrebuiltApplication = process.env.PLAYWRIGHT_PREBUILT === "1";
const reuseExistingServer =
	process.env.PLAYWRIGHT_REUSE_EXISTING_SERVER === "1";
const previewPort = Number(process.env.PLAYWRIGHT_PORT ?? "4173");
const previewCommand = `node node_modules/vite/bin/vite.js preview --config apps/web/vite.config.ts --host 127.0.0.1 --port ${previewPort}`;

export default defineConfig({
	testDir: ".",
	testMatch: "*.spec.ts",
	fullyParallel: false,
	maxFailures:
		process.env.FLOW_BROWSER_REPRESENTATIVE_AUDIT === "1" ||
		process.env.CI === "true"
			? 1
			: 0,
	retries: 0,
	workers: 1,
	reporter: [["list"]],
	timeout: 30_000,
	expect: { timeout: 10_000 },
	use: {
		baseURL: `http://127.0.0.1:${previewPort}`,
		headless: true,
		viewport: { width: 1440, height: 900 },
		screenshot: "only-on-failure",
		trace: "retain-on-failure",
		video: "retain-on-failure",
	},
	projects: crossBrowser
		? [
				{
					name: "chromium",
					use: {
						browserName: "chromium",
						...(chromiumChannel === undefined
							? {}
							: { channel: chromiumChannel }),
					},
				},
				{ name: "firefox", use: { browserName: "firefox" } },
				{ name: "webkit", use: { browserName: "webkit" } },
			]
		: [
				{
					name: "chromium",
					use: {
						browserName: "chromium",
						...(chromiumChannel === undefined
							? {}
							: { channel: chromiumChannel }),
					},
				},
			],
	webServer: {
		command: usePrebuiltApplication
			? previewCommand
			: `pnpm run build && ${previewCommand}`,
		port: previewPort,
		cwd: repositoryRoot,
		reuseExistingServer,
		timeout: 120_000,
	},
});
