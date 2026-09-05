import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { basename, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

type ScreenshotWitness = "early" | "middle" | "late";

type ReviewLedger = Readonly<{
	schema_version: 2;
	reviewer: string;
	manifest_sha256: string;
	screenshot_index_sha256: string;
	visual_surface_sha256: string;
	visual_surface_files: readonly string[];
	algorithm_count: number;
	reviews: readonly Readonly<{
		algorithm_id: string;
		verdict: "pass";
		reviewed_on: string;
		note: string;
		checks: Readonly<{
			step_change: "pass";
			focus_locality: "pass";
			annotation_ownership: "pass";
			arrowhead_contrast: "pass";
			parallel_edges: "pass" | "not-present";
			source_state: "pass";
		}>;
		screenshots: readonly Readonly<{
			case_label: string;
			witness: ScreenshotWitness;
			event: number;
			file: string;
			byte_size: number;
			sha256: string;
			graph_projection_sha256: string;
		}>[];
	}>[];
}>;

type ScreenshotAuditIndex = Readonly<{
	schema_version: 2;
	manifest_sha256: string;
	algorithm_count: number;
	records: readonly Readonly<{
		algorithm_id: string;
		case_label: string;
		witness: ScreenshotWitness;
		event: number;
		file: string;
		byte_size: number;
		sha256: string;
		graph_projection_sha256: string;
	}>[];
}>;

type AuditManifest = Readonly<{
	schema_version: 17;
	algorithm_count: number;
	cases: readonly Readonly<{
		algorithm_id: string;
		label: string;
		node_count: number;
		edge_count: number;
		first_detail: { event: number };
		middle_detail: { event: number };
		last_detail: { event: number };
		first_primary_work: { event: number };
		maximum_aggregation: { event: number };
		maximum_primary_work: { event: number };
	}>[];
}>;

const repositoryRoot = fileURLToPath(new URL("../../../", import.meta.url));
const manifestPath = join(
	repositoryRoot,
	"fixtures/flow-representative-audit.json",
);
const ledgerPath = join(
	repositoryRoot,
	"fixtures/flow-visual-review-ledger.json",
);
const screenshotIndexPath = join(
	repositoryRoot,
	"fixtures/flow-visual-audit/visual-audit-index.json",
);
const screenshotDirectory = join(repositoryRoot, "fixtures/flow-visual-audit");

function walkFiles(directory: string): string[] {
	return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
		const path = join(directory, entry.name);
		return entry.isDirectory() ? walkFiles(path) : [path];
	});
}

function visualSurfaceFiles(): string[] {
	const sourceRoot = join(repositoryRoot, "apps/web/src");
	const productionSurface = walkFiles(sourceRoot).filter((path) => {
		const name = basename(path);
		const sourcePath = relative(sourceRoot, path).replaceAll("\\", "/");
		if (/\.(?:test|spec)\.[cm]?[jt]sx?$/u.test(name)) return false;
		if (
			sourcePath
				.split("/")
				.some(
					(segment) =>
						segment.startsWith("Flow") || segment.startsWith("flow-"),
				)
		)
			return true;
		return new Set([
			"App.tsx",
			"engine-error-source.ts",
			"engine-session-response.ts",
			"engine-types.ts",
			"engine-worker.ts",
			"packet-v6.ts",
			"playback.ts",
			"publication-candidate-coordinator.ts",
			"styles-compact.css",
			"styles.css",
			"use-engine-worker.ts",
			"utf8-budget.ts",
		]).has(name);
	});
	return [
		...productionSurface,
		join(repositoryRoot, "tests/browser/flow-browser-coverage.ts"),
		join(repositoryRoot, "tests/browser/flow-representative-audit.spec.ts"),
	]
		.map((path) => relative(repositoryRoot, path).replaceAll("\\", "/"))
		.sort();
}

function sha256(path: string): string {
	return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function pngDimensions(
	path: string,
): Readonly<{ width: number; height: number }> {
	const bytes = readFileSync(path);
	if (
		bytes.byteLength < 24 ||
		bytes.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a" ||
		bytes.subarray(12, 16).toString("ascii") !== "IHDR"
	) {
		throw new Error(`${path} is not a canonical PNG`);
	}
	const width = bytes.readUInt32BE(16);
	const height = bytes.readUInt32BE(20);
	if (width === 0 || height === 0) throw new Error(`${path} has an empty IHDR`);
	return { width, height };
}

function visualSurfaceSha256(files: readonly string[]): string {
	const hash = createHash("sha256");
	for (const file of files) {
		const path = resolve(repositoryRoot, file);
		hash.update(file);
		hash.update("\0");
		hash.update(readFileSync(path));
		hash.update("\0");
	}
	return hash.digest("hex");
}

function candidateScreenshotEvents(
	auditCase: AuditManifest["cases"][number],
): ReadonlySet<number> {
	return new Set([
		auditCase.first_primary_work.event,
		auditCase.first_detail.event,
		auditCase.maximum_aggregation.event,
		auditCase.middle_detail.event,
		auditCase.maximum_primary_work.event,
		auditCase.last_detail.event,
	]);
}

function largestVisualAuditCases(
	manifest: AuditManifest,
): Map<string, AuditManifest["cases"][number]> {
	const result = new Map<string, AuditManifest["cases"][number]>();
	for (const auditCase of manifest.cases) {
		const current = result.get(auditCase.algorithm_id);
		if (
			current === undefined ||
			auditCase.edge_count > current.edge_count ||
			(auditCase.edge_count === current.edge_count &&
				auditCase.node_count > current.node_count)
		) {
			result.set(auditCase.algorithm_id, auditCase);
		}
	}
	return result;
}

describe("flow visual review ledger", () => {
	it("binds a direct-review verdict for every algorithm to the current trace and UI surface", () => {
		expect(
			existsSync(ledgerPath),
			"run the full screenshot audit and review every algorithm before release",
		).toBe(true);
		expect(
			existsSync(screenshotIndexPath),
			"persist the screenshot audit index before recording review verdicts",
		).toBe(true);
		const manifest = JSON.parse(
			readFileSync(manifestPath, "utf8"),
		) as AuditManifest;
		const ledger = JSON.parse(readFileSync(ledgerPath, "utf8")) as ReviewLedger;
		const screenshotIndex = JSON.parse(
			readFileSync(screenshotIndexPath, "utf8"),
		) as ScreenshotAuditIndex;
		const surfaceFiles = visualSurfaceFiles();
		const manifestAlgorithms = [
			...new Set(manifest.cases.map((auditCase) => auditCase.algorithm_id)),
		].sort();
		const largestCases = largestVisualAuditCases(manifest);

		expect(manifest.schema_version).toBe(17);
		expect(ledger.schema_version).toBe(2);
		expect(ledger.reviewer.trim().length).toBeGreaterThanOrEqual(3);
		expect(ledger.manifest_sha256).toBe(sha256(manifestPath));
		expect(screenshotIndex.schema_version).toBe(2);
		expect(screenshotIndex.manifest_sha256).toBe(sha256(manifestPath));
		expect(screenshotIndex.algorithm_count).toBe(manifest.algorithm_count);
		expect(ledger.screenshot_index_sha256).toBe(sha256(screenshotIndexPath));
		expect(ledger.visual_surface_files).toEqual(surfaceFiles);
		expect(ledger.visual_surface_sha256).toBe(
			visualSurfaceSha256(surfaceFiles),
		);
		expect(ledger.algorithm_count).toBe(manifest.algorithm_count);
		expect(ledger.reviews.map((review) => review.algorithm_id).sort()).toEqual(
			manifestAlgorithms,
		);
		expect(
			new Set(ledger.reviews.map((review) => review.algorithm_id)).size,
		).toBe(manifest.algorithm_count);
		expect(
			new Set(screenshotIndex.records.map((record) => record.algorithm_id)),
		).toEqual(new Set(manifestAlgorithms));
		expect(screenshotIndex.records).toHaveLength(manifest.algorithm_count * 3);
		for (const algorithmId of manifestAlgorithms) {
			const auditCase = largestCases.get(algorithmId);
			expect(auditCase, `${algorithmId} largest visual case`).toBeDefined();
			if (auditCase === undefined) continue;
			const records = screenshotIndex.records.filter(
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
			const candidateEvents = candidateScreenshotEvents(auditCase);
			expect(
				records.every((record) => candidateEvents.has(record.event)),
				`${algorithmId} screenshot events are audited witnesses`,
			).toBe(true);
			expect(new Set(records.map((record) => record.event)).size).toBe(3);
			expect(
				new Set(records.map((record) => record.graph_projection_sha256)).size,
				`${algorithmId} graph projections`,
			).toBe(3);
			expect(new Set(records.map((record) => record.sha256)).size).toBe(3);
		}
		expect(
			new Set(screenshotIndex.records.map((record) => record.sha256)).size,
		).toBe(screenshotIndex.records.length);
		for (const record of screenshotIndex.records) {
			expect(record.file).toBe(`${record.sha256}.png`);
			const path = join(screenshotDirectory, record.file);
			expect(existsSync(path), `${record.file} exists`).toBe(true);
			expect(statSync(path).size, `${record.file} byte size`).toBe(
				record.byte_size,
			);
			expect(sha256(path), `${record.file} content hash`).toBe(record.sha256);
			expect(record.graph_projection_sha256).toMatch(/^[0-9a-f]{64}$/u);
			expect(pngDimensions(path)).toEqual({ width: 1600, height: 1000 });
		}
		const retainedEntries = readdirSync(screenshotDirectory, {
			withFileTypes: true,
		});
		expect(
			retainedEntries.every((entry) => entry.isFile()),
			"retained visual audit contains only regular artifact files",
		).toBe(true);
		expect(
			retainedEntries.map((entry) => entry.name).sort(),
			"retained visual audit has no orphan or pending artifacts",
		).toEqual(
			[
				"visual-audit-index.json",
				...screenshotIndex.records.map((record) => record.file),
			].sort(),
		);
		expect(
			ledger.reviews
				.flatMap((review) =>
					review.screenshots.map((screenshot) => ({
						algorithm_id: review.algorithm_id,
						...screenshot,
					})),
				)
				.sort((left, right) =>
					`${left.algorithm_id}:${left.witness}`.localeCompare(
						`${right.algorithm_id}:${right.witness}`,
					),
				),
		).toEqual(
			[...screenshotIndex.records].sort((left, right) =>
				`${left.algorithm_id}:${left.witness}`.localeCompare(
					`${right.algorithm_id}:${right.witness}`,
				),
			),
		);

		for (const review of ledger.reviews) {
			expect(review.verdict, `${review.algorithm_id} review verdict`).toBe(
				"pass",
			);
			expect(review.reviewed_on).toMatch(/^20[0-9]{2}-[01][0-9]-[0-3][0-9]$/u);
			expect(review.note.trim().length).toBeGreaterThanOrEqual(12);
			expect(review.checks).toMatchObject({
				step_change: "pass",
				focus_locality: "pass",
				annotation_ownership: "pass",
				arrowhead_contrast: "pass",
				source_state: "pass",
			});
			expect(["pass", "not-present"]).toContain(review.checks.parallel_edges);
			expect(review.screenshots).toHaveLength(3);
			expect(
				new Set(
					review.screenshots.map(
						(record) => `${record.case_label}:${record.witness}`,
					),
				).size,
			).toBe(review.screenshots.length);

			for (const screenshot of review.screenshots) {
				expect(screenshot.sha256).toMatch(/^[0-9a-f]{64}$/u);
				expect(screenshot.graph_projection_sha256).toMatch(/^[0-9a-f]{64}$/u);
				expect(screenshot.byte_size).toBeGreaterThan(0);
				expect(screenshot.file).toBe(`${screenshot.sha256}.png`);
				expect(screenshot.event).toBeGreaterThan(0);
				const auditCase = manifest.cases.find(
					(candidate) =>
						candidate.algorithm_id === review.algorithm_id &&
						candidate.label === screenshot.case_label,
				);
				expect(
					auditCase,
					`${review.algorithm_id}/${screenshot.case_label} reviewed case`,
				).toBeDefined();
				if (auditCase === undefined) continue;
				expect(
					candidateScreenshotEvents(auditCase).has(screenshot.event),
					`${review.algorithm_id}/${screenshot.witness} audited witness event`,
				).toBe(true);
			}
		}
	});
});
