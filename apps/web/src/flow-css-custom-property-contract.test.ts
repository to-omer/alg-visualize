import { readdirSync, readFileSync } from "node:fs";
import { extname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const sourceRoot = fileURLToPath(new URL("./", import.meta.url));
const styles = ["styles.css", "styles-compact.css"]
	.map((file) => readFileSync(join(sourceRoot, file), "utf8"))
	.join("\n");

function productionSource(): string {
	return readdirSync(sourceRoot, { withFileTypes: true })
		.filter(
			(entry) =>
				entry.isFile() &&
				[".ts", ".tsx"].includes(extname(entry.name)) &&
				!entry.name.includes(".test."),
		)
		.map((entry) => readFileSync(join(sourceRoot, entry.name), "utf8"))
		.join("\n");
}

function matches(source: string, pattern: RegExp): string[] {
	return [...source.matchAll(pattern)].map((match) => match[1] ?? "");
}

describe("flow CSS custom-property contract", () => {
	it("defines every theme token in the root scope", () => {
		const root = styles.match(/^:root\s*\{([\s\S]*?)\n\}/u)?.[1];
		expect(root).toBeDefined();
		for (const token of [
			"--panel",
			"--surface-strong",
			"--ink",
			"--muted-strong",
			"--text-subtle",
			"--success",
			"--warning",
			"--danger",
			"--viz-series-1",
			"--viz-series-2",
			"--viz-series-3",
			"--viz-series-4",
		]) {
			expect(root, `${token} must resolve before SVG paint`).toMatch(
				new RegExp(`(?:^|\\n)\\s*${token}\\s*:`, "u"),
			);
		}
	});

	it("does not leave an unconditional CSS variable reference unresolved", () => {
		const required = new Set(matches(styles, /var\((--[a-z0-9-]+)\)/gu));
		const cssDeclarations = new Set(
			matches(styles, /(?:^|[;{])\s*(--[a-z0-9-]+)\s*:/gmu),
		);
		const runtimeDeclarations = new Set(
			matches(productionSource(), /["'](--[a-z0-9-]+)["']\s*:/gu),
		);
		const unresolved = [...required]
			.filter(
				(token) =>
					!cssDeclarations.has(token) && !runtimeDeclarations.has(token),
			)
			.sort();
		expect(unresolved).toEqual([]);
	});
});
