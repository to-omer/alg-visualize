export const PACKET_V6_HEADER_BYTES = 80;
export const MAX_PACKET_V6_BYTES = 32 * 1024 * 1024;
export const MAX_PUBLICATION_V6_BYTES = 64 * 1024 * 1024;
export const MAX_PACKET_V6_PARTS = 16;
export const MAX_PACKET_V6_BODY_BYTES =
	MAX_PACKET_V6_BYTES - PACKET_V6_HEADER_BYTES;

const MAX_U64 = (1n << 64n) - 1n;

export type PacketHeaderV6 = {
	pluginOrdinal: number;
	payloadSchemaVersion: number;
	publicationId: string;
	generation: string;
	partIndex: number;
	partCount: number;
	partBytes: number;
	totalBytes: number;
	payloadSha256: string;
};

export type DecodedPacketPartV6 = {
	header: PacketHeaderV6;
	body: Uint8Array;
};

export class PacketV6ValidationError extends Error {
	readonly code: "header" | "limits" | "part" | "publication" | "digest";

	constructor(code: PacketV6ValidationError["code"], message: string) {
		super(message);
		this.name = "PacketV6ValidationError";
		this.code = code;
	}
}

function digestHex(bytes: Uint8Array): string {
	let result = "";
	for (const byte of bytes) result += byte.toString(16).padStart(2, "0");
	return result;
}

function canonicalU64(value: string, field: string): bigint {
	if (!/^(0|[1-9][0-9]*)$/.test(value)) {
		throw new PacketV6ValidationError(
			"header",
			`${field} is not canonical u64`,
		);
	}
	const parsed = BigInt(value);
	if (parsed > MAX_U64) {
		throw new PacketV6ValidationError("header", `${field} exceeds u64`);
	}
	return parsed;
}

function uint32(value: number, field: string): number {
	if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
		throw new PacketV6ValidationError("header", `${field} is not u32`);
	}
	return value;
}

export type EncodePublicationV6Options = {
	pluginOrdinal: number;
	payloadSchemaVersion: number;
	publicationId: string;
	generation: string;
	/** Test-only-sized chunks remain protocol-valid; production omits this. */
	partBodyBytes?: number;
};

export async function encodePublicationV6(
	options: EncodePublicationV6Options,
	payload: Uint8Array,
): Promise<ArrayBuffer[]> {
	const pluginOrdinal = uint32(options.pluginOrdinal, "plugin ordinal");
	const payloadSchemaVersion = uint32(
		options.payloadSchemaVersion,
		"payload schema version",
	);
	if (pluginOrdinal === 0 || payloadSchemaVersion === 0) {
		throw new PacketV6ValidationError(
			"header",
			"plugin ordinal and payload schema must be nonzero",
		);
	}
	const publicationId = canonicalU64(options.publicationId, "publication id");
	const generation = canonicalU64(options.generation, "generation");
	if (payload.byteLength > MAX_PUBLICATION_V6_BYTES) {
		throw new PacketV6ValidationError(
			"limits",
			"V6 publication exceeds its byte limit",
		);
	}
	const partBodyBytes = options.partBodyBytes ?? MAX_PACKET_V6_BODY_BYTES;
	if (
		!Number.isSafeInteger(partBodyBytes) ||
		partBodyBytes <= 0 ||
		partBodyBytes > MAX_PACKET_V6_BODY_BYTES
	) {
		throw new PacketV6ValidationError(
			"limits",
			"V6 part body limit is invalid",
		);
	}
	const partCount = Math.max(1, Math.ceil(payload.byteLength / partBodyBytes));
	if (partCount > MAX_PACKET_V6_PARTS) {
		throw new PacketV6ValidationError(
			"limits",
			"V6 publication requires too many parts",
		);
	}
	const digestInput = new Uint8Array(payload.byteLength);
	digestInput.set(payload);
	const digest = new Uint8Array(
		await crypto.subtle.digest("SHA-256", digestInput.buffer),
	);
	const packets: ArrayBuffer[] = [];
	for (let partIndex = 0; partIndex < partCount; partIndex += 1) {
		const start = partIndex * partBodyBytes;
		const end = Math.min(payload.byteLength, start + partBodyBytes);
		const body = payload.subarray(start, end);
		const packet = new ArrayBuffer(PACKET_V6_HEADER_BYTES + body.byteLength);
		const bytes = new Uint8Array(packet);
		const view = new DataView(packet);
		bytes.set([0x41, 0x56, 0x50, 0x36], 0);
		view.setUint16(4, 6, true);
		view.setUint16(6, PACKET_V6_HEADER_BYTES, true);
		view.setUint32(8, pluginOrdinal, true);
		view.setUint32(12, payloadSchemaVersion, true);
		view.setBigUint64(16, publicationId, true);
		view.setBigUint64(24, generation, true);
		view.setUint16(32, partIndex, true);
		view.setUint16(34, partCount, true);
		view.setUint32(36, body.byteLength, true);
		view.setUint32(40, payload.byteLength, true);
		view.setUint32(44, 0, true);
		bytes.set(digest, 48);
		bytes.set(body, PACKET_V6_HEADER_BYTES);
		packets.push(packet);
	}
	return packets;
}

function samePublication(left: PacketHeaderV6, right: PacketHeaderV6): boolean {
	return (
		left.pluginOrdinal === right.pluginOrdinal &&
		left.payloadSchemaVersion === right.payloadSchemaVersion &&
		left.publicationId === right.publicationId &&
		left.generation === right.generation &&
		left.partCount === right.partCount &&
		left.totalBytes === right.totalBytes &&
		left.payloadSha256 === right.payloadSha256
	);
}

export function decodePacketPartV6(packet: ArrayBuffer): DecodedPacketPartV6 {
	if (
		packet.byteLength < PACKET_V6_HEADER_BYTES ||
		packet.byteLength > MAX_PACKET_V6_BYTES
	) {
		throw new PacketV6ValidationError(
			"limits",
			"V6 packet length is outside its limits",
		);
	}
	const bytes = new Uint8Array(packet);
	const view = new DataView(packet);
	if (
		bytes[0] !== 0x41 ||
		bytes[1] !== 0x56 ||
		bytes[2] !== 0x50 ||
		bytes[3] !== 0x36 ||
		view.getUint16(4, true) !== 6 ||
		view.getUint16(6, true) !== PACKET_V6_HEADER_BYTES ||
		view.getUint32(44, true) !== 0
	) {
		throw new PacketV6ValidationError("header", "V6 header is invalid");
	}
	const pluginOrdinal = view.getUint32(8, true);
	const partIndex = view.getUint16(32, true);
	const partCount = view.getUint16(34, true);
	const partBytes = view.getUint32(36, true);
	const totalBytes = view.getUint32(40, true);
	if (
		pluginOrdinal === 0 ||
		partCount === 0 ||
		partCount > MAX_PACKET_V6_PARTS ||
		partIndex >= partCount
	) {
		throw new PacketV6ValidationError("part", "V6 part metadata is invalid");
	}
	if (
		partBytes !== packet.byteLength - PACKET_V6_HEADER_BYTES ||
		partBytes > totalBytes ||
		totalBytes > MAX_PUBLICATION_V6_BYTES
	) {
		throw new PacketV6ValidationError("limits", "V6 body length is invalid");
	}
	return {
		header: {
			pluginOrdinal,
			payloadSchemaVersion: view.getUint32(12, true),
			publicationId: view.getBigUint64(16, true).toString(),
			generation: view.getBigUint64(24, true).toString(),
			partIndex,
			partCount,
			partBytes,
			totalBytes,
			payloadSha256: digestHex(bytes.subarray(48, 80)),
		},
		body: bytes.subarray(PACKET_V6_HEADER_BYTES),
	};
}

export async function assemblePublicationV6(
	parts: readonly DecodedPacketPartV6[],
): Promise<Uint8Array> {
	const first = parts[0];
	if (first === undefined || parts.length !== first.header.partCount) {
		throw new PacketV6ValidationError(
			"publication",
			"V6 publication is incomplete",
		);
	}
	const ordered: Array<Uint8Array | undefined> = new Array(
		first.header.partCount,
	);
	let observedBytes = 0;
	for (const part of parts) {
		if (!samePublication(first.header, part.header)) {
			throw new PacketV6ValidationError(
				"publication",
				"V6 publication fields disagree",
			);
		}
		if (ordered[part.header.partIndex] !== undefined) {
			throw new PacketV6ValidationError(
				"part",
				"V6 publication contains a duplicate part",
			);
		}
		ordered[part.header.partIndex] = part.body;
		observedBytes += part.body.byteLength;
	}
	if (observedBytes !== first.header.totalBytes) {
		throw new PacketV6ValidationError(
			"publication",
			"V6 publication total length is invalid",
		);
	}
	const payload = new Uint8Array(observedBytes);
	let offset = 0;
	for (const body of ordered) {
		if (body === undefined) {
			throw new PacketV6ValidationError(
				"publication",
				"V6 publication is missing a part",
			);
		}
		payload.set(body, offset);
		offset += body.byteLength;
	}
	const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", payload));
	if (digestHex(digest) !== first.header.payloadSha256) {
		throw new PacketV6ValidationError(
			"digest",
			"V6 publication digest is invalid",
		);
	}
	return payload;
}
