import { describe, expect, it } from "vitest";

import {
	decodeEngineFramePacket,
	decodeEngineFramePacketAsync,
	encodeFramePacket,
	PacketValidationError,
} from "./packet";

const metrics = {
	comparisons: "0",
	node_visits: "0",
	bit_tests: "0",
	rotations: "0",
	recolors: "0",
	splits: "0",
	merges: "0",
	rebuild_items: "0",
	allocations: "0",
	frees: "0",
};

const currentFrame = {
	itemIndex: 0,
	itemCount: 1,
	structure: { root: null, nodes: [] },
	canonical: {
		entries: [],
		metrics,
	},
};

const commitFrame = {
	baseItemIndex: 0,
	itemIndex: 1,
	itemCount: 1,
	initialBuild: false,
	result: { kind: "miss" },
	trace: [
		{
			catalog_id: 9,
			kind: "result",
			node: null,
			target: null,
			entry: null,
			key: "7",
			patch_start: 0,
			patch_count: 0,
		},
	],
	patches: [],
};

const insertedId = { index: 0, generation: 1 };
const insertedEntity = { kind: "node", id: insertedId };
const insertedNode = {
	id: insertedEntity,
	role: "binary-node",
	entries: [insertedId],
	keys: ["7"],
	links: [],
	metadata: [["height", "1"]],
};
const patchedCommitFrame = {
	...commitFrame,
	result: { kind: "inserted", entry: insertedId },
	trace: [
		{
			catalog_id: 3,
			kind: "insert",
			node: insertedEntity,
			target: null,
			entry: insertedId,
			key: "7",
			patch_start: 0,
			patch_count: 4,
		},
	],
	patches: [
		{ kind: "root", before: null, after: insertedEntity },
		{ kind: "node", id: insertedEntity, before: null, after: insertedNode },
		{
			kind: "entry",
			id: insertedId,
			before: null,
			after: { id: insertedId, key: "7", value: "seven" },
		},
		{
			kind: "metric",
			ordinal: "allocations",
			before: "0",
			after: "2",
		},
	],
};

const populatedCurrentFrame = {
	...currentFrame,
	structure: { root: insertedEntity, nodes: [insertedNode] },
	canonical: {
		entries: [{ id: insertedId, key: "7", value: "seven" }],
		metrics: { ...metrics, allocations: "2" },
	},
};

function rawCurrentPacket(payload: string): ArrayBuffer {
	const encoded = new TextEncoder().encode(payload);
	const packet = new ArrayBuffer(16 + encoded.byteLength);
	const view = new DataView(packet);
	view.setUint32(0, 0x4656_4956, true);
	view.setUint16(4, 5, true);
	view.setUint8(6, 1);
	view.setUint8(7, 0);
	view.setUint32(8, packet.byteLength, true);
	view.setUint32(12, encoded.byteLength, true);
	new Uint8Array(packet, 16).set(encoded);
	return packet;
}

describe("frame packet validation", () => {
	it("round-trips a bounded current frame", () => {
		const packet = encodeFramePacket("current", JSON.stringify(currentFrame));

		expect(decodeEngineFramePacket(packet, "current")).toEqual(currentFrame);
	});

	it("accepts the nullable Rust Option fields in a commit trace", () => {
		const packet = encodeFramePacket("commit", JSON.stringify(commitFrame));

		expect(decodeEngineFramePacket(packet, "commit")).toEqual(commitFrame);
	});

	it("round-trips a non-empty reversible patch section", () => {
		const packet = encodeFramePacket(
			"commit",
			JSON.stringify(patchedCommitFrame),
		);

		expect(decodeEngineFramePacket(packet, "commit")).toEqual(
			patchedCommitFrame,
		);
	});

	it("rejects a trace span that skips its patch section", () => {
		const packet = encodeFramePacket(
			"commit",
			JSON.stringify({
				...patchedCommitFrame,
				trace: [{ ...patchedCommitFrame.trace[0], patch_start: 1 }],
			}),
		);

		expect(() => decodeEngineFramePacket(packet, "commit")).toThrowError(
			/frame payload does not match the scene-frame contract/,
		);
	});

	it("rejects a truncated patch record section", () => {
		const packet = encodeFramePacket(
			"commit",
			JSON.stringify(patchedCommitFrame),
		);
		const bytes = new Uint8Array(packet);
		const payload = new TextDecoder().decode(bytes.subarray(16));
		const corrupted = payload.replace(
			'"patch_record_count":4',
			'"patch_record_count":5',
		);
		expect(corrupted).not.toBe(payload);
		bytes.set(new TextEncoder().encode(corrupted), 16);

		expect(() => decodeEngineFramePacket(packet, "commit")).toThrowError(
			/frame packet record counts are invalid/,
		);
	});

	it("accepts auxiliary summary keys that are not logical entries", () => {
		const auxiliary = { kind: "auxiliary", id: { index: 0, generation: 0 } };
		const frame = {
			...currentFrame,
			structure: {
				root: auxiliary,
				nodes: [
					{
						id: auxiliary,
						role: "range-summary",
						entries: [],
						keys: ["3", "12"],
						links: [],
						metadata: [],
					},
				],
			},
		};
		const packet = encodeFramePacket("current", JSON.stringify(frame));

		expect(decodeEngineFramePacket(packet, "current")).toEqual(frame);
	});

	it.each([
		[
			"duplicate structural identity",
			{
				...populatedCurrentFrame,
				structure: {
					...populatedCurrentFrame.structure,
					nodes: [insertedNode, insertedNode],
				},
			},
		],
		[
			"dangling structural link",
			{
				...populatedCurrentFrame,
				structure: {
					...populatedCurrentFrame.structure,
					nodes: [
						{
							...insertedNode,
							links: [
								{
									role: "left",
									slot: 0,
									target: { kind: "node", id: { index: 99, generation: 1 } },
								},
							],
						},
					],
				},
			},
		],
		[
			"duplicate canonical entry identity",
			{
				...populatedCurrentFrame,
				canonical: {
					...populatedCurrentFrame.canonical,
					entries: [
						...populatedCurrentFrame.canonical.entries,
						{ id: insertedId, key: "8", value: "eight" },
					],
				},
			},
		],
	] as const)("rejects a current frame with %s", async (_, frame) => {
		const packet = encodeFramePacket("current", JSON.stringify(frame));

		expect(() => decodeEngineFramePacket(packet, "current")).toThrowError(
			/frame payload does not match the scene-frame contract/,
		);
		await expect(
			decodeEngineFramePacketAsync(packet, "current"),
		).rejects.toThrowError(
			/frame payload does not match the scene-frame contract/,
		);
	});

	it("rejects a patch whose embedded identity differs from its record key", async () => {
		const packet = encodeFramePacket(
			"commit",
			JSON.stringify({
				...patchedCommitFrame,
				patches: patchedCommitFrame.patches.map((patch, index) =>
					index === 1
						? {
								...patch,
								after: {
									...insertedNode,
									id: { kind: "node", id: { index: 9, generation: 1 } },
								},
							}
						: patch,
				),
			}),
		);

		expect(() => decodeEngineFramePacket(packet, "commit")).toThrowError(
			/frame payload does not match the scene-frame contract/,
		);
		await expect(
			decodeEngineFramePacketAsync(packet, "commit"),
		).rejects.toThrowError(
			/frame payload does not match the scene-frame contract/,
		);
	});

	it("rejects a kind mismatch before parsing the payload", () => {
		const packet = encodeFramePacket("current", JSON.stringify(currentFrame));

		expect(() => decodeEngineFramePacket(packet, "commit")).toThrowError(
			PacketValidationError,
		);
	});

	it("rejects an invalid timeline envelope", () => {
		const packet = encodeFramePacket(
			"current",
			JSON.stringify({ ...currentFrame, itemIndex: 2 }),
		);

		expect(() => decodeEngineFramePacket(packet, "current")).toThrowError(
			/frame envelope/,
		);
	});

	it("rejects inconsistent capacity before decoding JSON", () => {
		const packet = encodeFramePacket("current", JSON.stringify(currentFrame));
		new DataView(packet).setUint32(8, packet.byteLength + 1, true);

		expect(() => decodeEngineFramePacket(packet, "current")).toThrowError(
			/frame packet length/,
		);
	});

	it("rejects a segmented record count that does not match the payload", () => {
		const packet = encodeFramePacket("current", JSON.stringify(currentFrame));
		const bytes = new Uint8Array(packet);
		const payload = new TextDecoder().decode(bytes.subarray(16));
		const corrupted = payload.replace(
			'"node_record_count":0',
			'"node_record_count":1',
		);
		expect(corrupted).not.toBe(payload);
		bytes.set(new TextEncoder().encode(corrupted), 16);

		expect(() => decodeEngineFramePacket(packet, "current")).toThrowError(
			/frame packet record counts are invalid/,
		);
	});

	it("rejects excess record lines before collecting the declared payload", async () => {
		const valid = new TextDecoder().decode(
			new Uint8Array(
				encodeFramePacket("current", JSON.stringify(currentFrame)),
				16,
			),
		);
		const packet = rawCurrentPacket(`${valid}\n${"\n".repeat(100_000)}`);

		await expect(
			decodeEngineFramePacketAsync(packet, "current"),
		).rejects.toThrowError(/more records than declared/);
	});

	it("rejects malformed nested trace identities before publishing a frame", () => {
		const packet = encodeFramePacket(
			"commit",
			JSON.stringify({
				...commitFrame,
				trace: [
					{
						...commitFrame.trace[0],
						node: { index: null, generation: 0 },
					},
				],
			}),
		);

		expect(() => decodeEngineFramePacket(packet, "commit")).toThrowError(
			/frame payload does not match the scene-frame contract/,
		);
	});

	it.each([
		"01",
		"18446744073709551616",
		"-1",
		"1e3",
	])("rejects non-canonical or out-of-range u64 key %s", (key) => {
		const packet = encodeFramePacket(
			"commit",
			JSON.stringify({
				...commitFrame,
				trace: [{ ...commitFrame.trace[0], key }],
			}),
		);

		expect(() => decodeEngineFramePacket(packet, "commit")).toThrowError(
			/frame payload does not match the scene-frame contract/,
		);
	});

	it("asynchronously validates a frame without settling in the current turn", async () => {
		const packet = encodeFramePacket("commit", JSON.stringify(commitFrame));
		let settled = false;
		const decoded = decodeEngineFramePacketAsync(packet, "commit").then(
			(frame) => {
				settled = true;
				return frame;
			},
		);

		await Promise.resolve();
		expect(settled).toBe(false);
		await expect(decoded).resolves.toEqual(commitFrame);
	});

	it("asynchronously rejects malformed nested values before publishing", async () => {
		const packet = encodeFramePacket(
			"current",
			JSON.stringify({
				...currentFrame,
				canonical: {
					...currentFrame.canonical,
					entries: [
						{ id: { index: -1, generation: 0 }, key: "7", value: "bad" },
					],
				},
			}),
		);

		await expect(
			decodeEngineFramePacketAsync(packet, "current"),
		).rejects.toThrowError(
			/frame payload does not match the scene-frame contract/,
		);
	});
});
