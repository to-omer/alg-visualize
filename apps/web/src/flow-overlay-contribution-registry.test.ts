import { describe, expect, it } from "vitest";

import {
	assertFlowOverlayContributionRegistry,
	assertFlowOverlaySemanticBindings,
	buildActiveFlowOverlayFeatureBundles,
	buildFlowOverlayViews,
	FLOW_OVERLAY_CONTRIBUTION_ENTRIES,
	FLOW_OVERLAY_CONTRIBUTION_FIELDS,
	FLOW_OVERLAY_CONTRIBUTIONS,
	FLOW_OVERLAY_FEATURE_BUNDLE_KEYS,
	type FlowOverlayContributionRegistry,
} from "./flow-overlay-contribution-registry";
import { FLOW_SCENE_V9_OVERLAY_DECODERS } from "./flow-scene-wire/generated/overlays";

describe("flow overlay contribution registry", () => {
	it("totally and uniquely covers every generated overlay field", () => {
		const generated = FLOW_SCENE_V9_OVERLAY_DECODERS.map(([field]) => field);
		expect(FLOW_OVERLAY_CONTRIBUTION_FIELDS).toEqual(generated);
		expect(FLOW_OVERLAY_CONTRIBUTION_ENTRIES).toHaveLength(generated.length);
		expect(
			new Set(FLOW_OVERLAY_CONTRIBUTION_ENTRIES.map(({ viewKey }) => viewKey))
				.size,
		).toBe(generated.length);
		for (const contribution of FLOW_OVERLAY_CONTRIBUTION_ENTRIES) {
			expect(contribution.semantic.kind).toBe("validator-required");
			expect(contribution.presentation.kind).toBe("rich");
			expect(contribution.title).not.toBe("");
			expect(contribution.description).not.toBe("");
			expect(contribution.featureBundles.length).toBeGreaterThan(0);
			for (const bundle of contribution.featureBundles) {
				expect(FLOW_OVERLAY_FEATURE_BUNDLE_KEYS).toContain(bundle);
			}
		}
		expect(
			new Set(
				FLOW_OVERLAY_CONTRIBUTION_ENTRIES.flatMap(
					({ featureBundles }) => featureBundles,
				),
			),
		).toEqual(new Set(FLOW_OVERLAY_FEATURE_BUNDLE_KEYS));
	});

	it("derives the exact active physical feature-bundle set from active fields", () => {
		expect(
			buildActiveFlowOverlayFeatureBundles([
				"electrical_flow_overlay",
				"eibfs_overlay",
			]),
		).toEqual(
			new Set([
				"original-edge-electrical",
				"original-edge-tree-chain",
				"original-edge-discrete-underlay",
				"node-continuous",
				"node-search",
				"rich-status",
			]),
		);
	});

	it.each([
		"augmenting_electrical_overlay",
		"flow_framework_mcf_overlay",
		"interior_point_max_flow_overlay",
		"minimum_ratio_cycle_mcf_overlay",
		"primal_dual_ipm_mcf_overlay",
		"randomized_almost_linear_mcf_overlay",
		"weighted_augmenting_paths_overlay",
		"weighted_push_relabel_shortcut_overlay",
	] as const)("declares the advanced graph bundle used by %s", (field) => {
		expect(FLOW_OVERLAY_CONTRIBUTIONS[field].featureBundles).toContain(
			"advanced-algorithm",
		);
	});

	it("rejects missing and unknown contribution fields", () => {
		const missing = { ...FLOW_OVERLAY_CONTRIBUTIONS } as Record<
			string,
			unknown
		>;
		delete missing.augmenting_electrical_overlay;
		expect(() => assertFlowOverlayContributionRegistry(missing)).toThrow(
			/missing: augmenting_electrical_overlay/,
		);

		const unknown = {
			...FLOW_OVERLAY_CONTRIBUTIONS,
			future_overlay: {
				field: "future_overlay",
				viewKey: "future",
				title: "Future",
				description: "Future overlay",
				semantic: { kind: "validator-required" },
				presentation: { kind: "generic", accent: "teal" },
				statusFields: ["stage"],
			},
		};
		expect(() => assertFlowOverlayContributionRegistry(unknown)).toThrow(
			/unknown\/duplicate: future_overlay/,
		);
	});

	it("rejects unexplained exemptions and mismatched semantic bindings", () => {
		const unexplained = {
			...FLOW_OVERLAY_CONTRIBUTIONS,
			augmenting_electrical_overlay: {
				...FLOW_OVERLAY_CONTRIBUTIONS.augmenting_electrical_overlay,
				semantic: { kind: "structural-exemption", reason: "" },
			},
		};
		expect(() => assertFlowOverlayContributionRegistry(unexplained)).toThrow(
			/unexplained semantic exemption/,
		);

		const bindings = Object.fromEntries(
			FLOW_OVERLAY_CONTRIBUTION_FIELDS.map((field) => [field, () => undefined]),
		) as Record<string, unknown>;
		delete bindings.binary_blocking_overlay;
		expect(() => assertFlowOverlaySemanticBindings(bindings)).toThrow(
			/missing: binary_blocking_overlay/,
		);
		bindings.binary_blocking_overlay = () => undefined;
		bindings.future_overlay = () => undefined;
		expect(() => assertFlowOverlaySemanticBindings(bindings)).toThrow(
			/unknown\/duplicate: future_overlay/,
		);

		const explicitExemption = {
			...FLOW_OVERLAY_CONTRIBUTIONS,
			augmenting_electrical_overlay: {
				...FLOW_OVERLAY_CONTRIBUTIONS.augmenting_electrical_overlay,
				semantic: {
					kind: "structural-exemption" as const,
					reason: "The generated decoder is the complete domain contract.",
				},
			},
		} as FlowOverlayContributionRegistry;
		delete bindings.future_overlay;
		expect(() =>
			assertFlowOverlaySemanticBindings(bindings, explicitExemption),
		).toThrow(/must use an explicit null binding/);
		bindings.augmenting_electrical_overlay = null;
		expect(() =>
			assertFlowOverlaySemanticBindings(bindings, explicitExemption),
		).not.toThrow();
	});

	it("rejects missing, duplicate, and unknown feature bundles", () => {
		for (const featureBundles of [
			[],
			["node-search", "node-search"],
			["future-bundle"],
		]) {
			const invalid = {
				...FLOW_OVERLAY_CONTRIBUTIONS,
				eibfs_overlay: {
					...FLOW_OVERLAY_CONTRIBUTIONS.eibfs_overlay,
					featureBundles,
				},
			};
			expect(() => assertFlowOverlayContributionRegistry(invalid)).toThrow(
				/invalid feature bundles/,
			);
		}
	});

	it("projects every registered active overlay through its owned view key", () => {
		const sentinels = Object.fromEntries(
			FLOW_OVERLAY_CONTRIBUTION_FIELDS.map((field) => [field, { field }]),
		);
		const views = buildFlowOverlayViews(sentinels as never);
		for (const contribution of FLOW_OVERLAY_CONTRIBUTION_ENTRIES) {
			expect(views[contribution.viewKey as keyof typeof views]).toBe(
				sentinels[contribution.field],
			);
		}
	});
});
