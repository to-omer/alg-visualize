import { defineConfig } from "@playwright/test";

const repositoryRoot = new URL("../..", import.meta.url).pathname;
const chromiumChannel =
	process.env.PLAYWRIGHT_CHROMIUM_CHANNEL ??
	(process.platform === "darwin" ? "chrome" : undefined);
const crossBrowser = process.env.PLAYWRIGHT_CROSS_BROWSER === "1";

export default defineConfig({
	testDir: ".",
	testMatch: "*.spec.ts",
	fullyParallel: false,
	maxFailures: process.env.CI === "true" ? 1 : 0,
	retries: 0,
	workers: 1,
	reporter: [["list"]],
	timeout: 30_000,
	expect: { timeout: 10_000 },
	use: {
		baseURL: "http://127.0.0.1:4173",
		headless: true,
		viewport: { width: 1440, height: 900 },
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
		command:
			"pnpm run build && pnpm exec vite preview --config apps/web/vite.config.ts --host 127.0.0.1 --port 4173",
		port: 4173,
		cwd: repositoryRoot,
		reuseExistingServer: false,
		timeout: 120_000,
	},
});
