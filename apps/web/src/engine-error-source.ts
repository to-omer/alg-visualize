import type { EngineRequest } from "./engine-types";
import { PacketValidationError } from "./packet";

export type EngineErrorSource = "engine" | "input";

export function engineRequestErrorSource(
	kind: EngineRequest["kind"],
	error?: unknown,
): EngineErrorSource {
	const message = error instanceof Error ? error.message : String(error ?? "");
	if (
		(kind === "next" || kind === "seek") &&
		((error instanceof PacketValidationError && error.code === "limits") ||
			message.startsWith("ordered-map resource limit exceeded:") ||
			message === "frame JSON byte limit exceeded")
	) {
		return "input";
	}
	return kind === "next" ||
		kind === "seek" ||
		kind === "commit-ack" ||
		kind === "current-ack"
		? "engine"
		: "input";
}
