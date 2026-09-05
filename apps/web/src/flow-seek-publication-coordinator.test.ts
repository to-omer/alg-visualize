import { describe, expect, it } from "vitest";
import { FlowSeekPublicationCoordinator } from "./flow-seek-publication-coordinator";

describe("FlowSeekPublicationCoordinator", () => {
	it("invalidates an encoding operation before a replacement seek can publish", async () => {
		const coordinator = new FlowSeekPublicationCoordinator();
		const first = coordinator.begin();
		let releaseFirst: (() => void) | undefined;
		const delayedEncode = new Promise<void>((resolve) => {
			releaseFirst = resolve;
		});
		const firstPublication = (async () => {
			await delayedEncode;
			if (!coordinator.isCurrent(first.operation)) return "discarded";
			coordinator.stage(first.operation, 4, "11", 7);
			return "staged";
		})();

		const second = coordinator.begin();
		coordinator.stage(second.operation, 4, "12", 7);
		releaseFirst?.();

		await expect(firstPublication).resolves.toBe("discarded");
		expect(coordinator.acknowledge(4, "11", true)).toEqual({
			kind: "ignored",
		});
		expect(coordinator.acknowledge(4, "12", true)).toEqual({
			kind: "accepted",
			sessionSerial: 7,
		});
	});

	it("replaces an ACK-pending seek and rejects both its late ACK and operation", () => {
		const coordinator = new FlowSeekPublicationCoordinator();
		const first = coordinator.begin();
		coordinator.stage(first.operation, 2, "21", 5);

		const second = coordinator.begin();
		expect(second.discardedSessionSerial).toBe(5);
		expect(coordinator.isCurrent(first.operation)).toBe(false);
		expect(coordinator.acknowledge(2, "21", true)).toEqual({
			kind: "ignored",
		});
		coordinator.stage(second.operation, 2, "22", 5);
		expect(coordinator.acknowledge(2, "22", false)).toEqual({
			kind: "rejected",
			sessionSerial: 5,
		});
	});

	it("cancel invalidates work even before it has a publication identity", () => {
		const coordinator = new FlowSeekPublicationCoordinator();
		expect(coordinator.hasPending()).toBe(false);
		const active = coordinator.begin();
		expect(coordinator.cancel()).toBeUndefined();
		expect(coordinator.isCurrent(active.operation)).toBe(false);
		expect(() => coordinator.stage(active.operation, 1, "31", 3)).toThrow(
			"stale flow seek",
		);
		expect(coordinator.hasPending()).toBe(false);
	});
});
