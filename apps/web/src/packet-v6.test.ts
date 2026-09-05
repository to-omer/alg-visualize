import { readFile } from "node:fs/promises";

import { describe, expect, it } from "vitest";

import {
	assemblePublicationV6,
	decodePacketPartV6,
	encodePublicationV6,
	PacketV6ValidationError,
} from "./packet-v6";

const fixtureUrl = new URL(
	"../../../fixtures/contracts/packet-v6.json",
	import.meta.url,
);

function decodeHex(value: string): ArrayBuffer {
	const bytes = new Uint8Array(value.length / 2);
	for (let index = 0; index < bytes.length; index += 1) {
		bytes[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16);
	}
	return bytes.buffer;
}

async function fixture() {
	return JSON.parse(await readFile(fixtureUrl, "utf8")) as {
		pluginOrdinal: number;
		payloadSchemaVersion: number;
		publicationId: string;
		generation: string;
		payloadUtf8: string;
		payloadSha256: string;
		partsHex: string[];
	};
}

function requiredPart<T>(parts: readonly T[], index: number): T {
	const part = parts[index];
	if (part === undefined) throw new Error("fixture part is missing");
	return part;
}

describe("PacketHeaderV6", () => {
	it("decodes and assembles the exact Rust-authority fixture", async () => {
		const golden = await fixture();
		const parts = golden.partsHex.map((part) =>
			decodePacketPartV6(decodeHex(part)),
		);

		expect(parts[0]?.header).toMatchObject({
			pluginOrdinal: golden.pluginOrdinal,
			payloadSchemaVersion: golden.payloadSchemaVersion,
			publicationId: golden.publicationId,
			generation: golden.generation,
			partCount: 3,
			totalBytes: 22,
			payloadSha256: golden.payloadSha256,
		});
		const payload = await assemblePublicationV6(parts.reverse());
		expect(new TextDecoder().decode(payload)).toBe(golden.payloadUtf8);
	});

	it("encodes the exact Rust-authority fixture", async () => {
		const golden = await fixture();
		const packets = await encodePublicationV6(
			{
				pluginOrdinal: golden.pluginOrdinal,
				payloadSchemaVersion: golden.payloadSchemaVersion,
				publicationId: golden.publicationId,
				generation: golden.generation,
				partBodyBytes: 9,
			},
			new TextEncoder().encode(golden.payloadUtf8),
		);

		expect(
			packets.map((packet) =>
				Array.from(new Uint8Array(packet), (byte) =>
					byte.toString(16).padStart(2, "0"),
				).join(""),
			),
		).toEqual(golden.partsHex);
	});

	it("rejects noncanonical identities and invalid chunk limits before encoding", async () => {
		await expect(
			encodePublicationV6(
				{
					pluginOrdinal: 2,
					payloadSchemaVersion: 1,
					publicationId: "01",
					generation: "1",
				},
				new Uint8Array(),
			),
		).rejects.toThrowError(/canonical u64/);
		await expect(
			encodePublicationV6(
				{
					pluginOrdinal: 2,
					payloadSchemaVersion: 1,
					publicationId: "1",
					generation: "1",
					partBodyBytes: 0,
				},
				new Uint8Array(),
			),
		).rejects.toThrowError(/part body limit/);
	});

	it("rejects reserved flags and mismatched body lengths", async () => {
		const golden = await fixture();
		const flagged = new Uint8Array(decodeHex(golden.partsHex[0] ?? ""));
		flagged[44] = 1;
		expect(() => decodePacketPartV6(flagged.buffer)).toThrowError(
			PacketV6ValidationError,
		);

		const truncated = decodeHex((golden.partsHex[0] ?? "").slice(0, -2));
		expect(() => decodePacketPartV6(truncated)).toThrowError(/body length/);
	});

	it("rejects duplicate parts and digest corruption atomically", async () => {
		const golden = await fixture();
		const parts = golden.partsHex.map((part) =>
			decodePacketPartV6(decodeHex(part)),
		);
		await expect(
			assemblePublicationV6([
				requiredPart(parts, 0),
				requiredPart(parts, 0),
				requiredPart(parts, 2),
			]),
		).rejects.toThrowError(/duplicate/);

		const corrupt = golden.partsHex.map((part) =>
			decodePacketPartV6(decodeHex(part)),
		);
		const corruptBody = requiredPart(corrupt, 1).body;
		corruptBody[0] = (corruptBody[0] ?? 0) ^ 0xff;
		await expect(assemblePublicationV6(corrupt)).rejects.toThrowError(/digest/);
	});
});
