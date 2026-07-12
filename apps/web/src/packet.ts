import type {
	CanonicalSnapshot,
	CommitFrame,
	CurrentFrame,
	StatePatchRecord,
	StructureEntityId,
	StructureSnapshot,
} from "./engine-types";

const MAGIC = 0x4656_4956;
const VERSION = 5;
const HEADER_BYTES = 16;
const MAX_PACKET_BYTES = 32 * 1024 * 1024;
const MAX_ENVELOPE_BYTES = 64 * 1024;
const MAX_RECORD_BYTES = 1024 * 1024;
const MAX_ENTRY_RECORDS = 110_000;
const MAX_NODE_RECORDS = 250_000;
const MAX_TRACE_RECORDS = 250_000;
const MAX_PATCH_RECORDS = 1_000_000;

export type FramePacketKind = "current" | "commit";

export type FrameDecodeTiming = {
	packetDecodeMs: number;
	payloadValidationMs: number;
};

const KIND: Record<FramePacketKind, number> = {
	current: 1,
	commit: 2,
};

function yieldToMainThread() {
	return new Promise<void>((resolve) => setTimeout(resolve, 0));
}

export class PacketValidationError extends Error {
	readonly code: "header" | "capacity" | "payload" | "limits";

	constructor(code: PacketValidationError["code"], message: string) {
		super(message);
		this.name = "PacketValidationError";
		this.code = code;
	}
}

function segmentedFrameJson(kind: FramePacketKind, json: string): string {
	try {
		const frame: unknown = JSON.parse(json);
		if (!isRecord(frame)) {
			throw new Error("frame envelope is not an object");
		}
		let nodes: unknown[] = [];
		let entries: unknown[] = [];
		let topLevel: Record<string, unknown>;
		if (kind === "current") {
			if (!isRecord(frame.structure) || !isRecord(frame.canonical)) {
				throw new Error("current frame state is missing");
			}
			const { nodes: nodeRecords, ...structure } = frame.structure;
			const { entries: entryRecords, ...canonical } = frame.canonical;
			if (!Array.isArray(nodeRecords) || !Array.isArray(entryRecords)) {
				throw new Error("current frame arrays are missing");
			}
			nodes = nodeRecords;
			entries = entryRecords;
			const { trace: _trace, patches: _patches, ...envelope } = frame;
			topLevel = { ...envelope, structure, canonical };
		} else {
			if ("structure" in frame || "canonical" in frame) {
				throw new Error("commit frame must contain deltas only");
			}
			const { trace: _trace, patches: _patches, ...envelope } = frame;
			topLevel = envelope;
		}
		const { trace, patches } = frame;
		if (
			kind === "commit" &&
			(!Array.isArray(trace) || !Array.isArray(patches))
		) {
			throw new Error("frame arrays are missing");
		}
		const traceRecords = kind === "commit" ? (trace as unknown[]) : [];
		const patchRecords = kind === "commit" ? (patches as unknown[]) : [];
		if (
			nodes.length > MAX_NODE_RECORDS ||
			entries.length > MAX_ENTRY_RECORDS ||
			traceRecords.length > MAX_TRACE_RECORDS ||
			patchRecords.length > MAX_PATCH_RECORDS
		) {
			throw new PacketValidationError(
				"limits",
				"frame record count exceeds its producer limit",
			);
		}
		const envelope = {
			...topLevel,
			entry_record_count: entries.length,
			node_record_count: nodes.length,
			trace_record_count: traceRecords.length,
			patch_record_count: patchRecords.length,
		};
		return [
			JSON.stringify(envelope),
			...nodes.map((node) => JSON.stringify(node)),
			...entries.map((entry) => JSON.stringify(entry)),
			...traceRecords.map((event) => JSON.stringify(event)),
			...patchRecords.map((record) => JSON.stringify(record)),
		].join("\n");
	} catch (error: unknown) {
		if (error instanceof PacketValidationError) {
			throw error;
		}
		throw new PacketValidationError(
			"payload",
			error instanceof Error ? error.message : "frame payload is invalid",
		);
	}
}

export function encodeFramePacket(
	kind: FramePacketKind,
	json: string,
): ArrayBuffer {
	if (json.length > MAX_PACKET_BYTES - HEADER_BYTES) {
		throw new PacketValidationError("limits", "frame payload exceeds 32 MiB");
	}
	const payload = new TextEncoder().encode(segmentedFrameJson(kind, json));
	if (payload.byteLength > MAX_PACKET_BYTES - HEADER_BYTES) {
		throw new PacketValidationError("limits", "frame payload exceeds 32 MiB");
	}
	const packet = new ArrayBuffer(HEADER_BYTES + payload.byteLength);
	const view = new DataView(packet);
	view.setUint32(0, MAGIC, true);
	view.setUint16(4, VERSION, true);
	view.setUint8(6, KIND[kind]);
	view.setUint8(7, 0);
	view.setUint32(8, packet.byteLength, true);
	view.setUint32(12, payload.byteLength, true);
	new Uint8Array(packet, HEADER_BYTES).set(payload);
	return packet;
}

function framePacketPayload(
	packet: ArrayBuffer,
	expectedKind: FramePacketKind,
): Uint8Array {
	if (
		packet.byteLength < HEADER_BYTES ||
		packet.byteLength > MAX_PACKET_BYTES
	) {
		throw new PacketValidationError(
			"capacity",
			"frame packet capacity is outside its limits",
		);
	}
	const view = new DataView(packet);
	const usedByteLength = view.getUint32(8, true);
	const payloadByteLength = view.getUint32(12, true);
	if (
		view.getUint32(0, true) !== MAGIC ||
		view.getUint16(4, true) !== VERSION ||
		view.getUint8(6) !== KIND[expectedKind] ||
		view.getUint8(7) !== 0
	) {
		throw new PacketValidationError("header", "frame packet header is invalid");
	}
	if (
		usedByteLength !== packet.byteLength ||
		payloadByteLength !== usedByteLength - HEADER_BYTES
	) {
		throw new PacketValidationError(
			"capacity",
			"frame packet length is invalid",
		);
	}
	return new Uint8Array(packet, HEADER_BYTES, payloadByteLength);
}

function decodeFramePacketText(
	packet: ArrayBuffer,
	expectedKind: FramePacketKind,
): string {
	try {
		return new TextDecoder("utf-8", { fatal: true }).decode(
			framePacketPayload(packet, expectedKind),
		);
	} catch (error: unknown) {
		if (error instanceof PacketValidationError) throw error;
		throw new PacketValidationError(
			"payload",
			error instanceof Error ? error.message : "frame payload is invalid",
		);
	}
}

async function decodeFramePacketLinesAsync(
	packet: ArrayBuffer,
	expectedKind: FramePacketKind,
): Promise<string[]> {
	const payload = framePacketPayload(packet, expectedKind);
	const lines: string[] = [];
	let lineStart = 0;
	const scanChunkBytes = 256 * 1024;
	const decoder = new TextDecoder("utf-8", { fatal: true });
	let declaredLineCount: number | undefined;
	try {
		for (let chunkStart = 0; chunkStart < payload.byteLength; ) {
			const chunkEnd = Math.min(
				payload.byteLength,
				chunkStart + scanChunkBytes,
			);
			for (let index = chunkStart; index < chunkEnd; index += 1) {
				const lineLimit =
					lines.length === 0 ? MAX_ENVELOPE_BYTES : MAX_RECORD_BYTES;
				if (index - lineStart > lineLimit) {
					throw new Error(
						lines.length === 0
							? "frame packet envelope exceeds 64 KiB"
							: "frame packet record exceeds 1 MiB",
					);
				}
				if (payload[index] === 0x0a) {
					const line = decoder.decode(payload.subarray(lineStart, index));
					if (lines.length === 0) {
						const segmented = segmentedEnvelopeFromLine(line, expectedKind);
						declaredLineCount = segmentedRecordCount(segmented) + 1;
					} else if (
						declaredLineCount !== undefined &&
						lines.length >= declaredLineCount
					) {
						throw new Error("frame packet has more records than declared");
					}
					lines.push(line);
					lineStart = index + 1;
				}
			}
			chunkStart = chunkEnd;
			if (chunkStart < payload.byteLength) await yieldToMainThread();
		}
		const finalLineLimit =
			lines.length === 0 ? MAX_ENVELOPE_BYTES : MAX_RECORD_BYTES;
		if (payload.byteLength - lineStart > finalLineLimit) {
			throw new Error(
				lines.length === 0
					? "frame packet envelope exceeds 64 KiB"
					: "frame packet record exceeds 1 MiB",
			);
		}
		if (declaredLineCount !== undefined && lines.length >= declaredLineCount) {
			throw new Error("frame packet has more records than declared");
		}
		lines.push(decoder.decode(payload.subarray(lineStart)));
		return lines;
	} catch (error: unknown) {
		throw new PacketValidationError(
			"payload",
			error instanceof Error ? error.message : "frame payload is invalid",
		);
	}
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

type SegmentedFrame = {
	entryCount: number;
	envelope: Record<string, unknown>;
	lines: string[];
	nodeCount: number;
	patchCount: number;
	traceCount: number;
};

function segmentedEnvelopeFromLine(
	line: string,
	expectedKind: FramePacketKind,
): SegmentedFrame {
	try {
		const envelope: unknown = JSON.parse(line);
		if (!isRecord(envelope)) {
			throw new Error("frame packet envelope is invalid");
		}
		const nodeCount = envelope.node_record_count;
		const entryCount = envelope.entry_record_count;
		const traceCount = envelope.trace_record_count;
		const patchCount = envelope.patch_record_count;
		if (
			!Number.isSafeInteger(nodeCount) ||
			(nodeCount as number) < 0 ||
			(nodeCount as number) > MAX_NODE_RECORDS ||
			!Number.isSafeInteger(entryCount) ||
			(entryCount as number) < 0 ||
			(entryCount as number) > MAX_ENTRY_RECORDS ||
			!Number.isSafeInteger(traceCount) ||
			(traceCount as number) < 0 ||
			(traceCount as number) > MAX_TRACE_RECORDS ||
			(expectedKind === "current" && traceCount !== 0) ||
			!Number.isSafeInteger(patchCount) ||
			(patchCount as number) < 0 ||
			(patchCount as number) > MAX_PATCH_RECORDS ||
			(expectedKind === "current" && patchCount !== 0) ||
			(expectedKind === "commit" &&
				((nodeCount as number) !== 0 || (entryCount as number) !== 0))
		) {
			throw new Error("frame packet record counts are invalid");
		}
		delete envelope.node_record_count;
		delete envelope.entry_record_count;
		delete envelope.trace_record_count;
		delete envelope.patch_record_count;
		return {
			entryCount: entryCount as number,
			envelope,
			lines: [],
			nodeCount: nodeCount as number,
			patchCount: patchCount as number,
			traceCount: traceCount as number,
		};
	} catch (error: unknown) {
		throw new PacketValidationError(
			"payload",
			error instanceof Error ? error.message : "frame payload is invalid",
		);
	}
}

function segmentedRecordCount(segmented: SegmentedFrame) {
	return (
		segmented.nodeCount +
		segmented.entryCount +
		segmented.traceCount +
		segmented.patchCount
	);
}

function segmentedEnvelopeFromLines(
	lines: string[],
	expectedKind: FramePacketKind,
): SegmentedFrame {
	const segmented = segmentedEnvelopeFromLine(lines[0] ?? "", expectedKind);
	if (lines.length !== segmentedRecordCount(segmented) + 1) {
		throw new PacketValidationError(
			"payload",
			"frame packet record counts are invalid",
		);
	}
	segmented.lines = lines;
	return segmented;
}

function segmentedEnvelope(
	payload: string,
	expectedKind: FramePacketKind,
): SegmentedFrame {
	return segmentedEnvelopeFromLines(payload.split("\n"), expectedKind);
}

function parseRecordLines(
	lines: string[],
	start: number,
	count: number,
): unknown[] {
	try {
		return lines.slice(start, start + count).map((line) => JSON.parse(line));
	} catch (error: unknown) {
		throw new PacketValidationError(
			"payload",
			error instanceof Error ? error.message : "frame record is invalid",
		);
	}
}

async function parseRecordLinesAsync(
	lines: string[],
	start: number,
	count: number,
): Promise<unknown[]> {
	const records = new Array<unknown>(count);
	const chunkSize = 512;
	try {
		for (let offset = 0; offset < count; offset += chunkSize) {
			const end = Math.min(count, offset + chunkSize);
			for (let index = offset; index < end; index += 1) {
				records[index] = JSON.parse(lines[start + index] ?? "");
			}
			if (end < count) {
				await new Promise<void>((resolve) => setTimeout(resolve, 0));
			}
		}
		return records;
	} catch (error: unknown) {
		throw new PacketValidationError(
			"payload",
			error instanceof Error ? error.message : "frame record is invalid",
		);
	}
}

function assembleSegmentedFrame(
	segmented: SegmentedFrame,
	nodes: unknown[],
	entries: unknown[],
	trace: unknown[],
	patches: unknown[],
	expectedKind: FramePacketKind,
): Record<string, unknown> {
	const frame: Record<string, unknown> = { ...segmented.envelope };
	if (expectedKind === "current") {
		const structure = segmented.envelope.structure;
		const canonical = segmented.envelope.canonical;
		if (!isRecord(structure) || !isRecord(canonical)) {
			throw new PacketValidationError("payload", "frame envelope is invalid");
		}
		frame.structure = { ...structure, nodes };
		frame.canonical = { ...canonical, entries };
	}
	if (expectedKind === "commit") {
		frame.trace = trace;
		frame.patches = patches;
	}
	return frame;
}

function parseSegmentedFrame(
	payload: string,
	expectedKind: FramePacketKind,
): Record<string, unknown> {
	const segmented = segmentedEnvelope(payload, expectedKind);
	const nodeStart = 1;
	const entryStart = nodeStart + segmented.nodeCount;
	const traceStart = entryStart + segmented.entryCount;
	const patchStart = traceStart + segmented.traceCount;
	return assembleSegmentedFrame(
		segmented,
		parseRecordLines(segmented.lines, nodeStart, segmented.nodeCount),
		parseRecordLines(segmented.lines, entryStart, segmented.entryCount),
		parseRecordLines(segmented.lines, traceStart, segmented.traceCount),
		parseRecordLines(segmented.lines, patchStart, segmented.patchCount),
		expectedKind,
	);
}

async function parseSegmentedFrameAsync(
	lines: string[],
	expectedKind: FramePacketKind,
): Promise<Record<string, unknown>> {
	const segmented = segmentedEnvelopeFromLines(lines, expectedKind);
	const nodeStart = 1;
	const entryStart = nodeStart + segmented.nodeCount;
	const traceStart = entryStart + segmented.entryCount;
	const patchStart = traceStart + segmented.traceCount;
	await new Promise<void>((resolve) => setTimeout(resolve, 0));
	const nodes = await parseRecordLinesAsync(
		segmented.lines,
		nodeStart,
		segmented.nodeCount,
	);
	const entries = await parseRecordLinesAsync(
		segmented.lines,
		entryStart,
		segmented.entryCount,
	);
	const trace = await parseRecordLinesAsync(
		segmented.lines,
		traceStart,
		segmented.traceCount,
	);
	const patches = await parseRecordLinesAsync(
		segmented.lines,
		patchStart,
		segmented.patchCount,
	);
	return assembleSegmentedFrame(
		segmented,
		nodes,
		entries,
		trace,
		patches,
		expectedKind,
	);
}

const MAX_U32 = 0xffff_ffff;
const MAX_U64_DECIMAL = "18446744073709551615";
const METRIC_FIELDS = [
	"comparisons",
	"node_visits",
	"bit_tests",
	"rotations",
	"recolors",
	"splits",
	"merges",
	"rebuild_items",
	"allocations",
	"frees",
] as const;
const TRACE_KINDS = new Set([
	"compare",
	"descend",
	"insert",
	"overwrite",
	"remove",
	"rotate-left",
	"rotate-right",
	"update-metadata",
	"rebuild",
	"split",
	"merge",
	"move-entry",
	"result",
]);
const METRIC_ORDINALS = new Set([
	"comparisons",
	"node-visits",
	"bit-tests",
	"rotations",
	"recolors",
	"splits",
	"merges",
	"rebuild-items",
	"allocations",
	"frees",
]);

function isU32(value: unknown): value is number {
	return (
		typeof value === "number" &&
		Number.isSafeInteger(value) &&
		value >= 0 &&
		value <= MAX_U32
	);
}

function isU64Decimal(value: unknown): value is string {
	if (
		typeof value !== "string" ||
		value.length === 0 ||
		value.length > MAX_U64_DECIMAL.length ||
		!/^(?:0|[1-9][0-9]*)$/.test(value)
	) {
		return false;
	}
	return value.length < MAX_U64_DECIMAL.length || value <= MAX_U64_DECIMAL;
}

function isArenaKey(value: unknown): boolean {
	return isRecord(value) && isU32(value.index) && isU32(value.generation);
}

function isStructureEntityId(value: unknown): boolean {
	return (
		isRecord(value) &&
		(value.kind === "node" || value.kind === "auxiliary") &&
		isArenaKey(value.id)
	);
}

function isStructureNode(node: unknown): boolean {
	return (
		isRecord(node) &&
		isStructureEntityId(node.id) &&
		typeof node.role === "string" &&
		Array.isArray(node.entries) &&
		node.entries.every(isArenaKey) &&
		Array.isArray(node.keys) &&
		node.keys.every(isU64Decimal) &&
		Array.isArray(node.links) &&
		node.links.every(
			(link) =>
				isRecord(link) &&
				isU32(link.slot) &&
				typeof link.role === "string" &&
				isStructureEntityId(link.target),
		) &&
		Array.isArray(node.metadata) &&
		node.metadata.every(
			(metadata) =>
				Array.isArray(metadata) &&
				metadata.length === 2 &&
				typeof metadata[0] === "string" &&
				isU64Decimal(metadata[1]),
		)
	);
}

function isStructureSnapshot(value: unknown): boolean {
	if (
		!isRecord(value) ||
		!(value.root === null || isStructureEntityId(value.root)) ||
		!Array.isArray(value.nodes)
	) {
		return false;
	}
	return value.nodes.every(isStructureNode);
}

function isCanonicalEntry(value: unknown): boolean {
	return (
		isRecord(value) &&
		isArenaKey(value.id) &&
		isU64Decimal(value.key) &&
		typeof value.value === "string"
	);
}

function isStatePatchRecord(value: unknown): boolean {
	if (!isRecord(value) || typeof value.kind !== "string") {
		return false;
	}
	switch (value.kind) {
		case "root":
			return (
				(value.before === null || isStructureEntityId(value.before)) &&
				(value.after === null || isStructureEntityId(value.after))
			);
		case "node":
			return (
				isStructureEntityId(value.id) &&
				(value.before === null || isStructureNode(value.before)) &&
				(value.after === null || isStructureNode(value.after))
			);
		case "entry":
			return (
				isArenaKey(value.id) &&
				(value.before === null || isCanonicalEntry(value.before)) &&
				(value.after === null || isCanonicalEntry(value.after))
			);
		case "metric":
			return (
				typeof value.ordinal === "string" &&
				METRIC_ORDINALS.has(value.ordinal) &&
				isU64Decimal(value.before) &&
				isU64Decimal(value.after)
			);
		default:
			return false;
	}
}

function isCanonicalSnapshot(value: unknown): boolean {
	if (
		!isRecord(value) ||
		!Array.isArray(value.entries) ||
		!isMetrics(value.metrics)
	) {
		return false;
	}
	return value.entries.every((entry) => isCanonicalEntry(entry));
}

function arenaIdentity(id: { generation: number; index: number }): string {
	return `${id.index}:${id.generation}`;
}

function entityIdentity(id: StructureEntityId): string {
	return `${id.kind}:${arenaIdentity(id.id)}`;
}

function sameArenaIdentity(
	left: { generation: number; index: number },
	right: { generation: number; index: number },
): boolean {
	return left.index === right.index && left.generation === right.generation;
}

function sameStructureIdentity(
	left: StructureEntityId,
	right: StructureEntityId,
): boolean {
	return left.kind === right.kind && sameArenaIdentity(left.id, right.id);
}

function* validateFrameReferences(
	structure: StructureSnapshot,
	canonical: CanonicalSnapshot,
): Generator<void, boolean> {
	let processed = 0;
	const shouldYield = () => {
		processed += 1;
		return processed % 1_024 === 0;
	};
	const entries = new Set<string>();
	const logicalKeys = new Set<string>();
	for (const entry of canonical.entries) {
		const identity = arenaIdentity(entry.id);
		if (entries.has(identity) || logicalKeys.has(entry.key)) {
			return false;
		}
		entries.add(identity);
		logicalKeys.add(entry.key);
		if (shouldYield()) {
			yield;
		}
	}

	const nodes = new Map<string, (typeof structure.nodes)[number]>();
	for (const node of structure.nodes) {
		const identity = entityIdentity(node.id);
		if (nodes.has(identity)) {
			return false;
		}
		nodes.set(identity, node);
		if (shouldYield()) {
			yield;
		}
	}
	if (structure.root !== null && !nodes.has(entityIdentity(structure.root))) {
		return false;
	}
	for (const node of structure.nodes) {
		const slots = new Set<number>();
		for (const entry of node.entries) {
			if (!entries.has(arenaIdentity(entry))) {
				return false;
			}
		}
		for (const link of node.links) {
			if (slots.has(link.slot) || !nodes.has(entityIdentity(link.target))) {
				return false;
			}
			slots.add(link.slot);
		}
		if (shouldYield()) {
			yield;
		}
	}

	const reachable = new Set<string>();
	const pending =
		structure.root === null ? [] : [entityIdentity(structure.root)];
	while (pending.length > 0) {
		const identity = pending.pop();
		if (identity === undefined || reachable.has(identity)) {
			continue;
		}
		reachable.add(identity);
		const node = nodes.get(identity);
		if (node === undefined) {
			return false;
		}
		for (const link of node.links) {
			pending.push(entityIdentity(link.target));
		}
		if (shouldYield()) {
			yield;
		}
	}
	for (const node of structure.nodes) {
		if (
			node.id.kind !== "auxiliary" &&
			!reachable.has(entityIdentity(node.id))
		) {
			return false;
		}
		if (shouldYield()) {
			yield;
		}
	}
	return true;
}

function frameReferencesAreValid(
	structure: StructureSnapshot,
	canonical: CanonicalSnapshot,
): boolean {
	const validation = validateFrameReferences(structure, canonical);
	let result = validation.next();
	while (!result.done) {
		result = validation.next();
	}
	return result.value;
}

async function frameReferencesAreValidAsync(
	structure: StructureSnapshot,
	canonical: CanonicalSnapshot,
): Promise<boolean> {
	const validation = validateFrameReferences(structure, canonical);
	let result = validation.next();
	while (!result.done) {
		await new Promise<void>((resolve) => setTimeout(resolve, 0));
		result = validation.next();
	}
	return result.value;
}

function patchIdentitiesAreValid(
	patches: readonly StatePatchRecord[],
): boolean {
	return patches.every((record) => {
		switch (record.kind) {
			case "node":
				return [record.before, record.after].every(
					(node) => node === null || sameStructureIdentity(record.id, node.id),
				);
			case "entry":
				return [record.before, record.after].every(
					(entry) => entry === null || sameArenaIdentity(record.id, entry.id),
				);
			case "root":
			case "metric":
				return true;
		}
		return false;
	});
}

function isStatePatchRecordWithIdentity(
	value: unknown,
): value is StatePatchRecord {
	return (
		isStatePatchRecord(value) &&
		patchIdentitiesAreValid([value as StatePatchRecord])
	);
}

function isMetrics(value: unknown): value is Record<string, unknown> {
	return (
		isRecord(value) &&
		METRIC_FIELDS.every((field) => isU64Decimal(value[field]))
	);
}

function isTraceEvent(value: unknown): boolean {
	return (
		isRecord(value) &&
		isU32(value.catalog_id) &&
		typeof value.kind === "string" &&
		TRACE_KINDS.has(value.kind) &&
		(value.node === null || isStructureEntityId(value.node)) &&
		(value.target === null || isStructureEntityId(value.target)) &&
		(value.entry === null || isArenaKey(value.entry)) &&
		(value.key === null || isU64Decimal(value.key)) &&
		isU32(value.patch_start) &&
		isU32(value.patch_count)
	);
}

function traceSpansCoverPatches(
	trace: readonly unknown[],
	patches: readonly unknown[],
): boolean {
	let offset = 0;
	for (const value of trace) {
		if (
			!isRecord(value) ||
			value.patch_start !== offset ||
			!isU32(value.patch_count)
		) {
			return false;
		}
		offset += value.patch_count;
		if (!Number.isSafeInteger(offset) || offset > patches.length) {
			return false;
		}
	}
	return offset === patches.length;
}

function isOperationResult(value: unknown): boolean {
	if (!isRecord(value) || typeof value.kind !== "string") {
		return false;
	}
	switch (value.kind) {
		case "inserted":
			return isArenaKey(value.entry);
		case "overwritten":
			return isArenaKey(value.entry) && typeof value.previous === "string";
		case "removed":
			return isArenaKey(value.entry) && typeof value.value === "string";
		case "miss":
			return true;
		case "found":
			return (
				isArenaKey(value.entry) &&
				isU64Decimal(value.key) &&
				typeof value.value === "string"
			);
		default:
			return false;
	}
}

function validateFrameEnvelope(
	frame: unknown,
	expectedKind: FramePacketKind,
): Record<string, unknown> {
	if (
		!isRecord(frame) ||
		!Number.isSafeInteger(frame.itemIndex) ||
		!Number.isSafeInteger(frame.itemCount) ||
		(frame.itemIndex as number) < 0 ||
		(frame.itemCount as number) < (frame.itemIndex as number) ||
		(frame.itemCount as number) > MAX_ENTRY_RECORDS
	) {
		throw new PacketValidationError("limits", "frame envelope is invalid");
	}
	if (
		expectedKind === "current" &&
		(!isRecord(frame.structure) ||
			!Array.isArray(frame.structure.nodes) ||
			frame.structure.nodes.length > MAX_NODE_RECORDS ||
			!isRecord(frame.canonical) ||
			!Array.isArray(frame.canonical.entries) ||
			frame.canonical.entries.length > MAX_ENTRY_RECORDS ||
			!isRecord(frame.canonical.metrics))
	) {
		throw new PacketValidationError("limits", "current frame is invalid");
	}
	if (
		expectedKind === "commit" &&
		(!Number.isSafeInteger(frame.baseItemIndex) ||
			(frame.baseItemIndex as number) < 0 ||
			(frame.baseItemIndex as number) + 1 !== (frame.itemIndex as number) ||
			typeof frame.initialBuild !== "boolean" ||
			!isRecord(frame.result) ||
			!Array.isArray(frame.trace) ||
			frame.trace.length > MAX_TRACE_RECORDS ||
			!Array.isArray(frame.patches) ||
			frame.patches.length > MAX_PATCH_RECORDS ||
			"structure" in frame ||
			"canonical" in frame)
	) {
		throw new PacketValidationError("limits", "commit envelope is invalid");
	}
	return frame;
}

function invalidFramePayload(): never {
	throw new PacketValidationError(
		"payload",
		"frame payload does not match the scene-frame contract",
	);
}

async function validateChunks(
	values: unknown[],
	validate: (value: unknown) => boolean,
): Promise<boolean> {
	const chunkSize = 1_024;
	for (let start = 0; start < values.length; start += chunkSize) {
		const end = Math.min(values.length, start + chunkSize);
		for (let index = start; index < end; index += 1) {
			if (!validate(values[index])) {
				return false;
			}
		}
		if (end < values.length) {
			await new Promise<void>((resolve) => setTimeout(resolve, 0));
		}
	}
	return true;
}

async function validateFramePayloadAsync(
	frame: Record<string, unknown>,
	expectedKind: FramePacketKind,
): Promise<void> {
	if (expectedKind === "commit") {
		if (
			!isOperationResult(frame.result) ||
			!Array.isArray(frame.trace) ||
			!(await validateChunks(frame.trace, isTraceEvent)) ||
			!Array.isArray(frame.patches) ||
			!(await validateChunks(frame.patches, isStatePatchRecordWithIdentity)) ||
			!traceSpansCoverPatches(frame.trace, frame.patches)
		) {
			invalidFramePayload();
		}
		return;
	}
	const structure = frame.structure;
	const canonical = frame.canonical;
	if (
		!isRecord(structure) ||
		!(structure.root === null || isStructureEntityId(structure.root)) ||
		!Array.isArray(structure.nodes) ||
		!isRecord(canonical) ||
		!Array.isArray(canonical.entries) ||
		!isMetrics(canonical.metrics)
	) {
		invalidFramePayload();
	}
	await new Promise<void>((resolve) => setTimeout(resolve, 0));
	if (!(await validateChunks(structure.nodes, isStructureNode))) {
		invalidFramePayload();
	}
	if (
		!(await validateChunks(
			canonical.entries,
			(entry) =>
				isRecord(entry) &&
				isArenaKey(entry.id) &&
				isU64Decimal(entry.key) &&
				typeof entry.value === "string",
		)) ||
		!(await frameReferencesAreValidAsync(
			structure as unknown as StructureSnapshot,
			canonical as unknown as CanonicalSnapshot,
		))
	) {
		invalidFramePayload();
	}
}

export function decodeEngineFramePacket(
	packet: ArrayBuffer,
	expectedKind: FramePacketKind,
): CurrentFrame | CommitFrame {
	const frame = validateFrameEnvelope(
		parseSegmentedFrame(
			decodeFramePacketText(packet, expectedKind),
			expectedKind,
		),
		expectedKind,
	);
	if (expectedKind === "commit") {
		if (
			!isOperationResult(frame.result) ||
			!(frame.trace as unknown[]).every(isTraceEvent) ||
			!(frame.patches as unknown[]).every(isStatePatchRecordWithIdentity) ||
			!traceSpansCoverPatches(
				frame.trace as unknown[],
				frame.patches as unknown[],
			)
		) {
			invalidFramePayload();
		}
	} else if (
		!isStructureSnapshot(frame.structure) ||
		!isCanonicalSnapshot(frame.canonical) ||
		!frameReferencesAreValid(
			frame.structure as unknown as StructureSnapshot,
			frame.canonical as unknown as CanonicalSnapshot,
		)
	) {
		invalidFramePayload();
	}
	return frame as unknown as CurrentFrame | CommitFrame;
}

export async function decodeEngineFramePacketAsync(
	packet: ArrayBuffer,
	expectedKind: FramePacketKind,
	onTiming?: (timing: FrameDecodeTiming) => void,
): Promise<CurrentFrame | CommitFrame> {
	const packetStartedAt = performance.now();
	const lines = await decodeFramePacketLinesAsync(packet, expectedKind);
	const parsed = await parseSegmentedFrameAsync(lines, expectedKind);
	const frame = validateFrameEnvelope(parsed, expectedKind);
	const packetDecodedAt = performance.now();
	await validateFramePayloadAsync(frame, expectedKind);
	onTiming?.({
		packetDecodeMs: packetDecodedAt - packetStartedAt,
		payloadValidationMs: performance.now() - packetDecodedAt,
	});
	return frame as unknown as CurrentFrame | CommitFrame;
}
