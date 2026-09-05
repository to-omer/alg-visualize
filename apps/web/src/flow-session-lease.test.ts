import { describe, expect, it, vi } from "vitest";

import {
	ownsActiveFlowSession,
	runWithActiveFlowSession,
} from "./flow-session-lease";

describe("flow session lease", () => {
	it("does not touch a freed wrapper when deferred encoding resumes after replacement", async () => {
		const oldSession = {
			discard: vi.fn(() => {
				throw new Error("freed WASM wrapper was touched");
			}),
		};
		const replacement = { discard: vi.fn() };
		const lease = { session: oldSession, serial: 4 };
		let resumeEncoding: (() => void) | undefined;
		const encoded = new Promise<void>((resolve) => {
			resumeEncoding = resolve;
		});
		let activeSession: typeof oldSession | typeof replacement | undefined =
			oldSession;
		let activeSerial = 4;
		const deferredRollback = encoded.then(() =>
			runWithActiveFlowSession(lease, activeSession, activeSerial, (session) =>
				session.discard(),
			),
		);

		activeSession = replacement;
		activeSerial = 5;
		resumeEncoding?.();

		await expect(deferredRollback).resolves.toBe(false);
		expect(oldSession.discard).not.toHaveBeenCalled();
		expect(replacement.discard).not.toHaveBeenCalled();
		expect(ownsActiveFlowSession(lease, activeSession, activeSerial)).toBe(
			false,
		);
	});

	it("rolls back exactly once while the captured session still owns the operation", () => {
		const session = { discard: vi.fn() };
		const lease = { session, serial: 9 };

		expect(
			runWithActiveFlowSession(lease, session, 9, (owned) => owned.discard()),
		).toBe(true);
		expect(session.discard).toHaveBeenCalledTimes(1);
	});
});
