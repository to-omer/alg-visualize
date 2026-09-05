import { describe, expect, it } from "vitest";

import { FlowEventPublicationCoordinator } from "./flow-event-publication-coordinator";

describe("FlowEventPublicationCoordinator", () => {
	it("commits only an exact generation/publication pair", () => {
		const coordinator = new FlowEventPublicationCoordinator();
		coordinator.stage(4, "19", 7);

		expect(coordinator.acknowledge(4, "18", true)).toEqual({
			kind: "ignored",
		});
		expect(coordinator.acknowledge(4, "19", true)).toEqual({
			kind: "accepted",
			sessionSerial: 7,
		});
	});

	it("returns the owner serial when rejection or supersession needs rollback", () => {
		const coordinator = new FlowEventPublicationCoordinator();
		expect(coordinator.hasPending()).toBe(false);
		coordinator.stage(2, "3", 11);
		expect(coordinator.hasPending()).toBe(true);
		expect(coordinator.acknowledge(2, "3", false)).toEqual({
			kind: "rejected",
			sessionSerial: 11,
		});
		coordinator.stage(3, "4", 12);
		expect(coordinator.discard()).toBe(12);
		expect(coordinator.discard()).toBeUndefined();
		expect(coordinator.hasPending()).toBe(false);
	});
});
