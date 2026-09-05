import { describe, expect, it } from "vitest";
import { isTransportationOptimalityCertificateRoute } from "./FlowGraphOriginalEdgeFeatureBundle";

describe("transportation optimality route certificate", () => {
	it("marks exactly nonbasic routes at the matching optimal boundary", () => {
		expect(
			isTransportationOptimalityCertificateRoute(
				"transportation-simplex",
				"transportation-simplex.optimal",
				false,
			),
		).toBe(true);
		expect(
			isTransportationOptimalityCertificateRoute(
				"transportation-simplex",
				"transportation-simplex.optimal",
				true,
			),
		).toBe(false);
		expect(
			isTransportationOptimalityCertificateRoute("modi", "modi.optimal", false),
		).toBe(true);
	});

	it("does not leak the certificate into pricing or another algorithm", () => {
		expect(
			isTransportationOptimalityCertificateRoute(
				"transportation-simplex",
				"transportation-simplex.bland-price",
				false,
			),
		).toBe(false);
		expect(
			isTransportationOptimalityCertificateRoute(
				"modi",
				"transportation-simplex.optimal",
				false,
			),
		).toBe(false);
	});
});
