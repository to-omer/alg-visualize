import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";

const repositoryRoot = fileURLToPath(new URL("../../../", import.meta.url));
const script = join(repositoryRoot, "scripts/flow-visual-audit-artifacts.mjs");
const temporaryRoots: string[] = [];

function sha256(bytes: Buffer): string {
	return createHash("sha256").update(bytes).digest("hex");
}

function writeAudit(root: string, suffix: string): string {
	const directory = join(root, suffix);
	mkdirSync(directory);
	const records = ["early", "middle", "late"].map((witness, index) => {
		const bytes = Buffer.from(`${suffix}:${witness}`);
		const digest = sha256(bytes);
		writeFileSync(join(directory, `${digest}.png`), bytes);
		return {
			algorithm_id: "test-algorithm",
			case_label: "largest",
			witness,
			event: index + 1,
			file: `${digest}.png`,
			byte_size: bytes.byteLength,
			sha256: digest,
			graph_projection_sha256: String(index).padStart(64, "0"),
		};
	});
	writeFileSync(
		join(directory, "visual-audit-index.json"),
		`${JSON.stringify(
			{
				schema_version: 2,
				manifest_sha256: "f".repeat(64),
				algorithm_count: 1,
				records,
			},
			null,
			2,
		)}\n`,
	);
	return directory;
}

function run(
	mode: "compare" | "promote",
	fresh: string,
	retained: string,
	reviewed = false,
) {
	return spawnSync(process.execPath, [script, mode, fresh, retained], {
		cwd: repositoryRoot,
		encoding: "utf8",
		env: {
			...process.env,
			...(reviewed ? { FLOW_VISUAL_AUDIT_REVIEWED: "1" } : {}),
		},
	});
}

afterEach(() => {
	for (const root of temporaryRoots.splice(0)) {
		rmSync(root, { recursive: true, force: true });
	}
});

describe("flow visual audit artifact gate", () => {
	it("accepts only byte-identical fresh and retained schema-2 artifacts", () => {
		const root = mkdtempSync(join(tmpdir(), "flow-visual-audit-"));
		temporaryRoots.push(root);
		const fresh = writeAudit(root, "same");
		const retained = join(root, "retained");
		mkdirSync(retained);
		const index = JSON.parse(
			readFileSync(join(fresh, "visual-audit-index.json"), "utf8"),
		) as { records: Array<{ file: string }> };
		for (const file of [
			"visual-audit-index.json",
			...index.records.map((record) => record.file),
		]) {
			writeFileSync(join(retained, file), readFileSync(join(fresh, file)));
		}

		const result = run("compare", fresh, retained);
		expect(result.status, result.stderr).toBe(0);
		expect(result.stdout).toContain("exactly match retained artifacts");
	});

	it("rejects a self-consistent fresh rerender that differs from retained", () => {
		const root = mkdtempSync(join(tmpdir(), "flow-visual-audit-"));
		temporaryRoots.push(root);
		const fresh = writeAudit(root, "fresh");
		const retained = writeAudit(root, "retained");

		const result = run("compare", fresh, retained);
		expect(result.status).not.toBe(0);
		expect(result.stderr).toContain("indexes differ");
	});

	it("requires explicit reviewed promotion and never replaces retained evidence", () => {
		const root = mkdtempSync(join(tmpdir(), "flow-visual-audit-"));
		temporaryRoots.push(root);
		const fresh = writeAudit(root, "fresh");
		const retained = join(root, "retained");

		expect(run("promote", fresh, retained).status).not.toBe(0);
		const promoted = run("promote", fresh, retained, true);
		expect(promoted.status, promoted.stderr).toBe(0);
		expect(run("compare", fresh, retained).status).toBe(0);
		expect(run("promote", fresh, retained, true).status).not.toBe(0);
	});
});
