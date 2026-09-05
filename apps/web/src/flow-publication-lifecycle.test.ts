import { describe, expect, it } from "vitest";
import {
	assertFlowPublicationAlgorithmIdentity,
	flowPublicationLifecycleReducer,
	INITIAL_FLOW_PUBLICATION_LIFECYCLE,
} from "./flow-publication-lifecycle";

describe("flow publication lifecycle", () => {
	it("rejects an algorithm identity mismatch before publication commit", () => {
		expect(() =>
			assertFlowPublicationAlgorithmIdentity(
				"successive-shortest-path",
				"excess-scaling-mcf",
			),
		).toThrow("does not match its Worker envelope");
		expect(() =>
			assertFlowPublicationAlgorithmIdentity(
				"successive-shortest-path",
				"successive-shortest-path",
			),
		).not.toThrow();
	});

	it("consumes an exact successful acknowledgement", () => {
		const decoding = flowPublicationLifecycleReducer(
			INITIAL_FLOW_PUBLICATION_LIFECYCLE,
			{ kind: "stage", generation: 7, publicationId: "19" },
		);
		expect(
			flowPublicationLifecycleReducer(decoding, {
				kind: "acknowledge",
				generation: 7,
				publicationId: "19",
				accepted: true,
			}),
		).toEqual({ kind: "ready", generation: 7, publicationId: "19" });
	});

	it("consumes a rejected publication so a retry is not locked out", () => {
		const decoding = flowPublicationLifecycleReducer(
			INITIAL_FLOW_PUBLICATION_LIFECYCLE,
			{ kind: "stage", generation: 4, publicationId: "9" },
		);
		const mismatched = flowPublicationLifecycleReducer(decoding, {
			kind: "acknowledge",
			generation: 4,
			publicationId: "8",
			accepted: false,
		});
		expect(mismatched).toBe(decoding);
		const failed = flowPublicationLifecycleReducer(mismatched, {
			kind: "acknowledge",
			generation: 4,
			publicationId: "9",
			accepted: false,
		});
		expect(failed).toEqual({
			kind: "failed",
			generation: 4,
			publicationId: "9",
		});
		expect(
			flowPublicationLifecycleReducer(failed, {
				kind: "stage",
				generation: 5,
				publicationId: "10",
			}),
		).toEqual({ kind: "decoding", generation: 5, publicationId: "10" });
	});
});
