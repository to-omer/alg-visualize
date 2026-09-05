import { describe, expect, it, vi } from "vitest";

import { PublicationCandidateCoordinator } from "./publication-candidate-coordinator";

function candidate() {
	return { free: vi.fn() };
}

describe("PublicationCandidateCoordinator", () => {
	it("commits only the exact generation and publication ID", () => {
		const coordinator = new PublicationCandidateCoordinator();
		const value = candidate();
		coordinator.stage(7, "18446744073709551615", value);

		expect(coordinator.acknowledge(6, "18446744073709551615", true)).toEqual({
			kind: "ignored",
		});
		expect(coordinator.acknowledge(7, "9", true)).toEqual({
			kind: "ignored",
		});
		expect(value.free).not.toHaveBeenCalled();
		expect(coordinator.hasPending()).toBe(true);

		expect(coordinator.acknowledge(7, "18446744073709551615", true)).toEqual({
			kind: "accepted",
			candidate: value,
		});
		expect(coordinator.hasPending()).toBe(false);
		expect(value.free).not.toHaveBeenCalled();
	});

	it("frees rejected or superseded candidates exactly once", () => {
		const coordinator = new PublicationCandidateCoordinator();
		const rejected = candidate();
		coordinator.stage(1, "1", rejected);
		expect(coordinator.acknowledge(1, "1", false)).toEqual({
			kind: "rejected",
		});
		expect(rejected.free).toHaveBeenCalledTimes(1);

		const superseded = candidate();
		coordinator.stage(2, "2", superseded);
		coordinator.discard();
		coordinator.discard();
		expect(superseded.free).toHaveBeenCalledTimes(1);
	});

	it("rejects a second outstanding candidate", () => {
		const coordinator = new PublicationCandidateCoordinator();
		coordinator.stage(1, "1", candidate());

		expect(() => coordinator.stage(1, "2", candidate())).toThrowError(
			/awaiting ACK/,
		);
	});
});
