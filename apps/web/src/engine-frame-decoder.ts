import type { CommitFrame, CurrentFrame } from "./engine-types";
import { decodeEngineFramePacketAsync } from "./packet";

export function decodeFrame(
	packet: ArrayBuffer,
	kind: "commit",
): Promise<CommitFrame>;
export function decodeFrame(
	packet: ArrayBuffer,
	kind: "current",
): Promise<CurrentFrame>;
export async function decodeFrame(
	packet: ArrayBuffer,
	kind: "commit" | "current",
): Promise<CommitFrame | CurrentFrame> {
	const startedAt = performance.now();
	const frame = await decodeEngineFramePacketAsync(packet, kind, (timing) => {
		document.documentElement.dataset.packetDecodeMs =
			timing.packetDecodeMs.toFixed(3);
		document.documentElement.dataset.payloadValidationMs =
			timing.payloadValidationMs.toFixed(3);
	});
	document.documentElement.dataset.frameDecodeMs = (
		performance.now() - startedAt
	).toFixed(3);
	return frame;
}
