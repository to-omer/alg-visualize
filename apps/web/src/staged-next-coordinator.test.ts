import { describe, expect, it, vi } from "vitest";

import {
	StagedNextCoordinator,
	StagedNextRollbackError,
} from "./staged-next-coordinator";

describe("staged next coordinator", () => {
	it("clears failed rollback ownership and never retries it", () => {
		const coordinator = new StagedNextCoordinator();
		const discard = vi.fn(() => {
			throw new Error("rollback failed");
		});
		const session = {
			commit_staged_next: vi.fn(),
			discard_staged_next: discard,
		};
		coordinator.stage(7);

		expect(() => coordinator.discard(session)).toThrowError(
			new StagedNextRollbackError(new Error("rollback failed")),
		);
		expect(() => coordinator.discard(session)).not.toThrow();
		expect(discard).toHaveBeenCalledTimes(1);
	});

	it("commits only the matching accepted generation", () => {
		const coordinator = new StagedNextCoordinator();
		const session = {
			commit_staged_next: vi.fn(),
			discard_staged_next: vi.fn(),
		};
		coordinator.stage(4);
		coordinator.acknowledge(session, 3, true);
		expect(session.commit_staged_next).not.toHaveBeenCalled();
		coordinator.acknowledge(session, 4, true);
		expect(session.commit_staged_next).toHaveBeenCalledOnce();
	});
});
