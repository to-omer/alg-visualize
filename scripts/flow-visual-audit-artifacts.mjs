import { createHash, timingSafeEqual } from "node:crypto";
import {
	cpSync,
	existsSync,
	lstatSync,
	mkdirSync,
	readdirSync,
	readFileSync,
	renameSync,
	rmSync,
	statSync,
} from "node:fs";
import { basename, dirname, join, resolve } from "node:path";

const INDEX_FILE = "visual-audit-index.json";
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const SCREENSHOT_WITNESSES = ["early", "middle", "late"];

function fail(message) {
	throw new Error(`flow visual audit: ${message}`);
}

function sha256(bytes) {
	return createHash("sha256").update(bytes).digest("hex");
}

function readAudit(directory) {
	const root = resolve(directory);
	if (!existsSync(root) || !statSync(root).isDirectory()) {
		fail(`${root} is not an audit directory`);
	}
	const entries = readdirSync(root, { withFileTypes: true });
	if (entries.some((entry) => !entry.isFile())) {
		fail(`${root} contains a non-file artifact`);
	}
	const indexPath = join(root, INDEX_FILE);
	if (!existsSync(indexPath)) fail(`${indexPath} is missing`);
	const indexBytes = readFileSync(indexPath);
	const index = JSON.parse(indexBytes.toString("utf8"));
	if (
		index?.schema_version !== 2 ||
		!SHA256_PATTERN.test(index.manifest_sha256) ||
		!Number.isSafeInteger(index.algorithm_count) ||
		index.algorithm_count <= 0 ||
		!Array.isArray(index.records) ||
		index.records.length !== index.algorithm_count * 3
	) {
		fail(`${indexPath} is not a schema-2 complete audit index`);
	}
	const expectedFiles = new Set([INDEX_FILE]);
	const witnessKeys = new Set();
	const algorithmWitnesses = new Map();
	const records = index.records.map((record, ordinal) => {
		if (
			typeof record?.algorithm_id !== "string" ||
			record.algorithm_id.length === 0 ||
			typeof record.case_label !== "string" ||
			record.case_label.length === 0 ||
			!SCREENSHOT_WITNESSES.includes(record.witness) ||
			!Number.isSafeInteger(record.event) ||
			record.event <= 0 ||
			typeof record.file !== "string" ||
			basename(record.file) !== record.file ||
			!SHA256_PATTERN.test(record.sha256) ||
			record.file !== `${record.sha256}.png` ||
			!Number.isSafeInteger(record.byte_size) ||
			record.byte_size <= 0 ||
			!SHA256_PATTERN.test(record.graph_projection_sha256)
		) {
			fail(`${indexPath} record ${ordinal} is invalid`);
		}
		const witnessKey = `${record.algorithm_id}:${record.witness}`;
		if (witnessKeys.has(witnessKey)) {
			fail(`${indexPath} contains duplicate witness ${witnessKey}`);
		}
		witnessKeys.add(witnessKey);
		const witnesses = algorithmWitnesses.get(record.algorithm_id) ?? new Set();
		witnesses.add(record.witness);
		algorithmWitnesses.set(record.algorithm_id, witnesses);
		const path = join(root, record.file);
		if (!existsSync(path) || !lstatSync(path).isFile()) {
			fail(`${path} is missing or is not a regular file`);
		}
		const bytes = readFileSync(path);
		if (
			bytes.byteLength !== record.byte_size ||
			sha256(bytes) !== record.sha256
		) {
			fail(`${path} does not match its index record`);
		}
		expectedFiles.add(record.file);
		return { ...record, bytes };
	});
	if (algorithmWitnesses.size !== index.algorithm_count) {
		fail(`${indexPath} algorithm count does not match its records`);
	}
	for (const [algorithmId, witnesses] of algorithmWitnesses) {
		if (
			witnesses.size !== SCREENSHOT_WITNESSES.length ||
			SCREENSHOT_WITNESSES.some((witness) => !witnesses.has(witness))
		) {
			fail(`${indexPath} has an incomplete witness set for ${algorithmId}`);
		}
	}
	const actualFiles = entries.map((entry) => entry.name).sort();
	const indexedFiles = [...expectedFiles].sort();
	if (JSON.stringify(actualFiles) !== JSON.stringify(indexedFiles)) {
		fail(`${root} contains missing, orphaned, or pending artifacts`);
	}
	return { root, index, indexBytes, records };
}

function logicalIndex(index) {
	return JSON.stringify(index);
}

function compare(freshDirectory, retainedDirectory) {
	const fresh = readAudit(freshDirectory);
	const retained = readAudit(retainedDirectory);
	if (logicalIndex(fresh.index) !== logicalIndex(retained.index)) {
		fail("fresh and retained schema-2 indexes differ");
	}
	for (let index = 0; index < fresh.records.length; index += 1) {
		const freshRecord = fresh.records[index];
		const retainedRecord = retained.records[index];
		if (
			freshRecord.bytes.byteLength !== retainedRecord.bytes.byteLength ||
			!timingSafeEqual(freshRecord.bytes, retainedRecord.bytes)
		) {
			fail(`fresh and retained PNG bytes differ for ${freshRecord.file}`);
		}
	}
	process.stdout.write(
		`flow visual audit: ${fresh.index.algorithm_count} algorithms and ${fresh.records.length} PNGs exactly match retained artifacts\n`,
	);
}

function promote(freshDirectory, retainedDirectory) {
	if (process.env.FLOW_VISUAL_AUDIT_REVIEWED !== "1") {
		fail(
			"promotion requires FLOW_VISUAL_AUDIT_REVIEWED=1 after direct human review",
		);
	}
	const fresh = readAudit(freshDirectory);
	const retained = resolve(retainedDirectory);
	if (existsSync(retained)) {
		fail(
			`${retained} already exists; retained evidence is never replaced implicitly`,
		);
	}
	const parent = dirname(retained);
	mkdirSync(parent, { recursive: true });
	const staging = join(
		parent,
		`.${basename(retained)}.promoting-${process.pid}-${Date.now()}`,
	);
	if (existsSync(staging)) fail(`${staging} already exists`);
	try {
		mkdirSync(staging);
		for (const entry of readdirSync(fresh.root)) {
			cpSync(join(fresh.root, entry), join(staging, entry), {
				errorOnExist: true,
				force: false,
			});
		}
		const promotion = readAudit(staging);
		if (logicalIndex(fresh.index) !== logicalIndex(promotion.index)) {
			fail("staged promotion index differs from the reviewed fresh index");
		}
		renameSync(staging, retained);
	} catch (error) {
		if (existsSync(staging)) rmSync(staging, { recursive: true, force: true });
		throw error;
	}
	process.stdout.write(
		`flow visual audit: promoted ${fresh.records.length} reviewed PNGs to ${retained}\n`,
	);
}

const [mode, freshDirectory, retainedDirectory] = process.argv.slice(2);
if (
	!(mode === "compare" || mode === "promote") ||
	!freshDirectory ||
	!retainedDirectory
) {
	fail(
		"usage: node scripts/flow-visual-audit-artifacts.mjs <compare|promote> <fresh-directory> <retained-directory>",
	);
}

if (mode === "compare") compare(freshDirectory, retainedDirectory);
else promote(freshDirectory, retainedDirectory);
