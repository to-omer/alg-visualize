import type { FlowCurrentSceneV9 as FlowCurrentSceneV9Wire } from "./flow-scene-wire/generated/FlowCurrentSceneV9";
import {
	FLOW_SCENE_V9_OVERLAY_DECODERS,
	type FlowSceneV9OverlayField,
} from "./flow-scene-wire/generated/overlays";

export type FlowOverlaySemanticPolicy =
	| Readonly<{ kind: "validator-required" }>
	| Readonly<{ kind: "structural-exemption"; reason: string }>;

export type FlowOverlayPresentationPolicy =
	| Readonly<{ kind: "rich" }>
	| Readonly<{
			kind: "generic";
			accent: "teal" | "violet" | "amber";
	  }>;

export const FLOW_OVERLAY_FEATURE_BUNDLE_KEYS = Object.freeze([
	"original-edge-discrete-underlay",
	"original-edge-tree-chain",
	"original-edge-electrical",
	"original-edge-discrete-overlay",
	"node-continuous",
	"node-optimization",
	"node-search",
	"feasibility",
	"advanced-algorithm",
	"rich-status",
] as const);

export type FlowOverlayFeatureBundleKey =
	(typeof FLOW_OVERLAY_FEATURE_BUNDLE_KEYS)[number];

export type FlowOverlayContributionDefinition = Readonly<{
	field: FlowSceneV9OverlayField;
	viewKey: string;
	title: string;
	description: string;
	semantic: FlowOverlaySemanticPolicy;
	presentation: FlowOverlayPresentationPolicy;
	featureBundles: readonly FlowOverlayFeatureBundleKey[];
	statusFields: readonly string[];
	sceneGroup: "exclusive-forest" | null;
}>;

export type FlowOverlayContributionRegistry = Readonly<
	Record<FlowSceneV9OverlayField, FlowOverlayContributionDefinition>
>;

export type FlowOverlaySemanticValidator = (value: unknown) => unknown;
export type FlowOverlaySemanticBindings = Readonly<
	Record<FlowSceneV9OverlayField, FlowOverlaySemanticValidator | null>
>;

const required = Object.freeze({ kind: "validator-required" } as const);
const rich = Object.freeze({ kind: "rich" } as const);
const continuousBundles = Object.freeze([
	"original-edge-electrical",
	"original-edge-tree-chain",
	"node-continuous",
	"rich-status",
] as const satisfies readonly FlowOverlayFeatureBundleKey[]);
const continuousAdvancedBundles = Object.freeze([
	...continuousBundles,
	"advanced-algorithm",
] as const satisfies readonly FlowOverlayFeatureBundleKey[]);
const optimizationBundles = Object.freeze([
	"original-edge-discrete-underlay",
	"original-edge-discrete-overlay",
	"node-optimization",
	"rich-status",
] as const satisfies readonly FlowOverlayFeatureBundleKey[]);
const searchBundles = Object.freeze([
	"original-edge-discrete-underlay",
	"node-search",
	"rich-status",
] as const satisfies readonly FlowOverlayFeatureBundleKey[]);
const treeChainBundles = Object.freeze([
	"original-edge-tree-chain",
	"node-continuous",
	"rich-status",
] as const satisfies readonly FlowOverlayFeatureBundleKey[]);

const advancedAlgorithmBundles = Object.freeze([
	"advanced-algorithm",
	"rich-status",
] as const satisfies readonly FlowOverlayFeatureBundleKey[]);

function richContribution<
	const Field extends FlowSceneV9OverlayField,
	const ViewKey extends string,
>(
	field: Field,
	viewKey: ViewKey,
	title: string,
	description: string,
	featureBundles: readonly FlowOverlayFeatureBundleKey[],
	statusFields: readonly string[] = ["stage", "phase", "iteration"],
	sceneGroup: "exclusive-forest" | null = "exclusive-forest",
): FlowOverlayContributionDefinition &
	Readonly<{ field: Field; viewKey: ViewKey }> {
	return {
		field,
		viewKey,
		title,
		description,
		semantic: required,
		presentation: rich,
		featureBundles,
		statusFields,
		sceneGroup,
	};
}

function continuousContribution<
	const Field extends FlowSceneV9OverlayField,
	const ViewKey extends string,
>(
	field: Field,
	viewKey: ViewKey,
	title: string,
	description: string,
	statusFields?: readonly string[],
	sceneGroup?: "exclusive-forest" | null,
) {
	return richContribution(
		field,
		viewKey,
		title,
		description,
		continuousBundles,
		statusFields,
		sceneGroup,
	);
}

function optimizationContribution<
	const Field extends FlowSceneV9OverlayField,
	const ViewKey extends string,
>(
	field: Field,
	viewKey: ViewKey,
	title: string,
	description: string,
	statusFields?: readonly string[],
	sceneGroup?: "exclusive-forest" | null,
) {
	return richContribution(
		field,
		viewKey,
		title,
		description,
		optimizationBundles,
		statusFields,
		sceneGroup,
	);
}

function searchContribution<
	const Field extends FlowSceneV9OverlayField,
	const ViewKey extends string,
>(
	field: Field,
	viewKey: ViewKey,
	title: string,
	description: string,
	statusFields?: readonly string[],
	sceneGroup?: "exclusive-forest" | null,
) {
	return richContribution(
		field,
		viewKey,
		title,
		description,
		searchBundles,
		statusFields,
		sceneGroup,
	);
}

function treeChainContribution<
	const Field extends FlowSceneV9OverlayField,
	const ViewKey extends string,
>(
	field: Field,
	viewKey: ViewKey,
	title: string,
	description: string,
	statusFields?: readonly string[],
	sceneGroup?: "exclusive-forest" | null,
) {
	return richContribution(
		field,
		viewKey,
		title,
		description,
		treeChainBundles,
		statusFields,
		sceneGroup,
	);
}

function advancedAlgorithmContribution<
	const Field extends FlowSceneV9OverlayField,
	const ViewKey extends string,
>(
	field: Field,
	viewKey: ViewKey,
	title: string,
	description: string,
	statusFields?: readonly string[],
	sceneGroup?: "exclusive-forest" | null,
) {
	return richContribution(
		field,
		viewKey,
		title,
		description,
		advancedAlgorithmBundles,
		statusFields,
		sceneGroup,
	);
}

/**
 * The single handwritten extension point for a generated scene overlay.
 *
 * Every Rust-generated `*_overlay` field must have exactly one contribution.
 * Rich entries keep their existing algorithm-specific rendering; a future
 * generic entry automatically receives entity marks, SVG fallback decoration,
 * status, legend, inspector rows, and an accessible description.
 */
export const FLOW_OVERLAY_CONTRIBUTIONS = {
	augmenting_electrical_overlay: richContribution(
		"augmenting_electrical_overlay",
		"augmentingElectrical",
		"Augmenting electrical flow",
		"Shows the central flow, electrical correction, congestion, and boost intervals.",
		continuousAdvancedBundles,
	),
	binary_blocking_overlay: optimizationContribution(
		"binary_blocking_overlay",
		"binaryBlocking",
		"Binary Blocking Flow",
		"Shows 0/1 lengths, components, admissible arcs, and blocking-flow stages.",
		["stage", "delta", "delivered", "upper_bound"],
	),
	cancel_tighten_overlay: optimizationContribution(
		"cancel_tighten_overlay",
		"cancelTighten",
		"Cancel and Tighten",
		"Shows negative cycles, potentials, and ε-optimality updates.",
		["stage", "epsilon", "iteration"],
	),
	convex_cost_overlay: optimizationContribution(
		"convex_cost_overlay",
		"convexCost",
		"Convex-cost flow",
		"Shows piecewise-linear costs, marginal costs, and improvement directions.",
		["stage", "delta", "iteration"],
	),
	convex_network_simplex_overlay: optimizationContribution(
		"convex_network_simplex_overlay",
		"convexNetworkSimplex",
		"Convex network simplex",
		"Shows the compact basis, priced cycles, and breakpoint pivots.",
	),
	deterministic_almost_linear_overlay: treeChainContribution(
		"deterministic_almost_linear_overlay",
		"deterministicAlmostLinear",
		"Deterministic tree-chain max flow",
		"Shows hierarchical forests, spanners, embeddings, and rounding.",
	),
	double_scaling_overlay: optimizationContribution(
		"double_scaling_overlay",
		"doubleScaling",
		"Double Scaling",
		"Shows concurrent scaling of capacity, excess, and cost.",
		["stage", "delta", "epsilon", "iteration"],
	),
	dual_network_simplex_overlay: optimizationContribution(
		"dual_network_simplex_overlay",
		"dualNetworkSimplex",
		"Dual Network Simplex",
		"Shows the dual-feasible basis, cut, and entering or leaving arcs.",
	),
	dynamic_eibfs_overlay: searchContribution(
		"dynamic_eibfs_overlay",
		"dynamicEibfs",
		"Dynamic EIBFS",
		"Shows the search forest reused after a capacity update and its repair state.",
		undefined,
		null,
	),
	eibfs_overlay: searchContribution(
		"eibfs_overlay",
		"eibfs",
		"EIBFS",
		"Shows bidirectional BFS forests, orphans, frontiers, and adoption.",
	),
	electrical_flow_overlay: continuousContribution(
		"electrical_flow_overlay",
		"electricalFlow",
		"Electrical flow",
		"Shows resistance, potential, current, congestion, and energy.",
	),
	electrical_ipm_mcf_overlay: continuousContribution(
		"electrical_ipm_mcf_overlay",
		"electricalIpmMcf",
		"Electrical-flow IPM MCF",
		"Shows centrality and correction directions for an electrical-flow interior-point method.",
	),
	feasibility_overlay: richContribution(
		"feasibility_overlay",
		"feasibility",
		"Feasible-flow construction",
		"Shows the lower-bound shift, artificial terminals, FIFO Push–Relabel routing, and exact flow extraction.",
		["feasibility", "rich-status"],
		["use_kind", "stage", "total_required", "routed"],
		null,
	),
	enhanced_capacity_scaling_overlay: optimizationContribution(
		"enhanced_capacity_scaling_overlay",
		"enhancedCapacityScaling",
		"Enhanced Capacity Scaling",
		"Shows contracted components, virtual flow, tight arcs, and shortest paths.",
		["stage", "delta", "iteration"],
	),
	flow_framework_mcf_overlay: advancedAlgorithmContribution(
		"flow_framework_mcf_overlay",
		"flowFrameworkMcf",
		"Deterministic flow-framework MCF",
		"Shows the deterministic sparse hierarchy and minimum-ratio-cycle search.",
		undefined,
		null,
	),
	interior_point_max_flow_overlay: richContribution(
		"interior_point_max_flow_overlay",
		"interiorPointMaxFlow",
		"Max-flow interior point",
		"Shows central flow, slack, resistance, congestion, and rounding.",
		continuousAdvancedBundles,
	),
	minimum_ratio_cycle_mcf_overlay: advancedAlgorithmContribution(
		"minimum_ratio_cycle_mcf_overlay",
		"minimumRatioCycleMcf",
		"Minimum-ratio Cycle MCF",
		"Shows the minimum-ratio cycle and improvement of the MCF interior-point method.",
		undefined,
		null,
	),
	minimum_ratio_cycle_overlay: treeChainContribution(
		"minimum_ratio_cycle_overlay",
		"minimumRatioCycle",
		"Minimum-ratio Cycle",
		"Shows candidate minimum-ratio cycles for the max-flow interior-point method.",
		undefined,
		null,
	),
	orlin_max_flow_overlay: optimizationContribution(
		"orlin_max_flow_overlay",
		"orlinMaxFlow",
		"Orlin max flow",
		"Shows residual classes, contracted components, compact arcs, and transfers.",
	),
	orlin_mcf_overlay: optimizationContribution(
		"orlin_mcf_overlay",
		"orlinMcf",
		"Orlin minimum-cost flow",
		"Shows capacity nodes, flow and slack branches, contractions, and shortest paths.",
		["stage", "delta", "iteration"],
	),
	parametric_overlay: optimizationContribution(
		"parametric_overlay",
		"parametric",
		"Parametric Flow",
		"Shows parameter intervals, breakpoints, and nested-cut transitions.",
		["stage"],
	),
	polynomial_dual_simplex_overlay: optimizationContribution(
		"polynomial_dual_simplex_overlay",
		"polynomialDualSimplex",
		"Polynomial Dual Simplex",
		"Shows auxiliary pseudoflow, bad arcs, and augmentation paths.",
	),
	polynomial_primal_simplex_overlay: optimizationContribution(
		"polynomial_primal_simplex_overlay",
		"polynomialPrimalSimplex",
		"Polynomial Primal Simplex",
		"Shows the perturbed basis, eligible cycles, and entering or leaving arcs.",
	),
	prediction_assisted_epsilon_overlay: optimizationContribution(
		"prediction_assisted_epsilon_overlay",
		"predictionAssistedEpsilon",
		"Prediction-assisted ε-relaxation",
		"Shows predicted prices, clipping, the scale ladder, and ε-balanced arcs.",
		["stage", "epsilon", "attempt", "maximum_attempt"],
	),
	primal_dual_ipm_mcf_overlay: advancedAlgorithmContribution(
		"primal_dual_ipm_mcf_overlay",
		"primalDualIpmMcf",
		"Primal-dual IPM MCF",
		"Shows primal-dual centrality, slack, and Newton corrections.",
	),
	randomized_almost_linear_mcf_overlay: advancedAlgorithmContribution(
		"randomized_almost_linear_mcf_overlay",
		"randomizedAlmostLinearMcf",
		"Randomized almost-linear MCF",
		"Shows randomized tree chains and minimum-ratio-cycle search.",
		undefined,
		null,
	),
	randomized_almost_linear_overlay: treeChainContribution(
		"randomized_almost_linear_overlay",
		"randomizedAlmostLinear",
		"Randomized tree-chain max flow",
		"Shows seeded forest sampling, the IPM, and final rounding.",
		undefined,
		null,
	),
	relaxed_mndc_overlay: optimizationContribution(
		"relaxed_mndc_overlay",
		"relaxedMndc",
		"Relaxed Most-negative Cycle",
		"Shows relaxed negative-cycle candidates and dual assignments.",
		["stage", "epsilon", "iteration"],
	),
	tardos_framework_overlay: optimizationContribution(
		"tardos_framework_overlay",
		"tardosFramework",
		"Tardos Framework",
		"Shows pricing, normalization, and the variable-fixing certificate.",
	),
	weighted_augmenting_paths_overlay: advancedAlgorithmContribution(
		"weighted_augmenting_paths_overlay",
		"weightedAugmentingPaths",
		"Weighted Augmenting Paths",
		"Shows weighted distances, shortcuts, and augmenting paths.",
		undefined,
		null,
	),
	weighted_push_relabel_shortcut_overlay: advancedAlgorithmContribution(
		"weighted_push_relabel_shortcut_overlay",
		"weightedPushRelabelShortcut",
		"Weighted Push–Relabel",
		"Shows weighted heights, the shortcut graph, and local pushes.",
		undefined,
		null,
	),
} satisfies FlowOverlayContributionRegistry;

const generatedFields = FLOW_SCENE_V9_OVERLAY_DECODERS.map(([field]) => field);

function assertExactFields(
	actual: readonly string[],
	expected: readonly string[],
	label: string,
): void {
	const expectedSet = new Set(expected);
	const actualSet = new Set(actual);
	const missing = expected.filter((field) => !actualSet.has(field));
	const unknown = actual.filter((field) => !expectedSet.has(field));
	if (
		missing.length > 0 ||
		unknown.length > 0 ||
		actualSet.size !== actual.length
	) {
		throw new Error(
			`${label} does not exactly cover generated overlays (missing: ${missing.join(", ") || "none"}; unknown/duplicate: ${unknown.join(", ") || (actualSet.size === actual.length ? "none" : "duplicate")})`,
		);
	}
}

function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function assertFlowOverlayContributionRegistry(
	registry: Readonly<Record<string, unknown>>,
	expectedFields: readonly string[] = generatedFields,
): asserts registry is FlowOverlayContributionRegistry {
	assertExactFields(Object.keys(registry), expectedFields, "Overlay registry");
	for (const field of expectedFields) {
		const contribution = registry[field];
		if (!isRecord(contribution) || contribution.field !== field) {
			throw new Error(`Overlay registry entry ${field} has a mismatched field`);
		}
		if (
			typeof contribution.viewKey !== "string" ||
			contribution.viewKey.trim() === ""
		) {
			throw new Error(`Overlay registry entry ${field} has no view key`);
		}
		if (
			typeof contribution.title !== "string" ||
			contribution.title.trim() === "" ||
			typeof contribution.description !== "string" ||
			contribution.description.trim() === ""
		) {
			throw new Error(`Overlay registry entry ${field} has incomplete copy`);
		}
		if (!isRecord(contribution.semantic)) {
			throw new Error(`Overlay registry entry ${field} has no semantic policy`);
		}
		if (contribution.semantic.kind === "structural-exemption") {
			if (
				typeof contribution.semantic.reason !== "string" ||
				contribution.semantic.reason.trim() === ""
			) {
				throw new Error(
					`Overlay registry entry ${field} has an unexplained semantic exemption`,
				);
			}
		} else if (contribution.semantic.kind !== "validator-required") {
			throw new Error(
				`Overlay registry entry ${field} has an invalid semantic policy`,
			);
		}
		if (
			!isRecord(contribution.presentation) ||
			(contribution.presentation.kind !== "rich" &&
				contribution.presentation.kind !== "generic")
		) {
			throw new Error(
				`Overlay registry entry ${field} has an invalid presentation policy`,
			);
		}
		if (
			contribution.presentation.kind === "generic" &&
			!(["teal", "violet", "amber"] as const).includes(
				contribution.presentation.accent as "teal" | "violet" | "amber",
			)
		) {
			throw new Error(`Overlay registry entry ${field} has an invalid accent`);
		}
		if (
			!Array.isArray(contribution.featureBundles) ||
			contribution.featureBundles.length === 0 ||
			!contribution.featureBundles.every(
				(bundle) =>
					typeof bundle === "string" &&
					(FLOW_OVERLAY_FEATURE_BUNDLE_KEYS as readonly string[]).includes(
						bundle,
					),
			) ||
			new Set(contribution.featureBundles).size !==
				contribution.featureBundles.length
		) {
			throw new Error(
				`Overlay registry entry ${field} has invalid feature bundles`,
			);
		}
		if (
			!Array.isArray(contribution.statusFields) ||
			!contribution.statusFields.every((item) => typeof item === "string")
		) {
			throw new Error(
				`Overlay registry entry ${field} has invalid status fields`,
			);
		}
		if (
			contribution.sceneGroup !== null &&
			contribution.sceneGroup !== "exclusive-forest"
		) {
			throw new Error(
				`Overlay registry entry ${field} has an invalid scene group`,
			);
		}
	}
	const viewKeys = expectedFields.map((field) => {
		const contribution = registry[field];
		return isRecord(contribution) && typeof contribution.viewKey === "string"
			? contribution.viewKey
			: "";
	});
	if (new Set(viewKeys).size !== viewKeys.length) {
		throw new Error("Overlay registry contains duplicate view keys");
	}
}

export function assertFlowOverlaySemanticBindings(
	bindings: Readonly<Record<string, unknown>>,
	registry: FlowOverlayContributionRegistry = FLOW_OVERLAY_CONTRIBUTIONS,
): asserts bindings is FlowOverlaySemanticBindings {
	const fields = Object.keys(registry);
	assertExactFields(Object.keys(bindings), fields, "Overlay semantic bindings");
	for (const field of fields as FlowSceneV9OverlayField[]) {
		const binding = bindings[field];
		const policy = registry[field].semantic;
		if (policy.kind === "validator-required" && typeof binding !== "function") {
			throw new Error(`Overlay semantic validator ${field} is required`);
		}
		if (policy.kind === "structural-exemption" && binding !== null) {
			throw new Error(
				`Overlay semantic exemption ${field} must use an explicit null binding`,
			);
		}
	}
}

assertFlowOverlayContributionRegistry(FLOW_OVERLAY_CONTRIBUTIONS);

export const FLOW_OVERLAY_CONTRIBUTION_FIELDS = Object.freeze(
	generatedFields.slice(),
);

export const FLOW_OVERLAY_CONTRIBUTION_ENTRIES = Object.freeze(
	FLOW_OVERLAY_CONTRIBUTION_FIELDS.map(
		(field) => FLOW_OVERLAY_CONTRIBUTIONS[field],
	),
);

export function buildActiveFlowOverlayFeatureBundles(
	activeFields: readonly FlowSceneV9OverlayField[],
): ReadonlySet<FlowOverlayFeatureBundleKey> {
	const bundles = new Set<FlowOverlayFeatureBundleKey>();
	for (const field of activeFields) {
		for (const bundle of FLOW_OVERLAY_CONTRIBUTIONS[field].featureBundles) {
			bundles.add(bundle);
		}
	}
	return bundles;
}

type RegisteredFlowOverlayContributions = typeof FLOW_OVERLAY_CONTRIBUTIONS;

export type FlowOverlayViews = Readonly<{
	[Field in keyof RegisteredFlowOverlayContributions as RegisteredFlowOverlayContributions[Field]["viewKey"]]: FlowCurrentSceneV9Wire[Field];
}>;

export function buildFlowOverlayViews(
	overlays: Readonly<Pick<FlowCurrentSceneV9Wire, FlowSceneV9OverlayField>>,
): FlowOverlayViews {
	return Object.fromEntries(
		FLOW_OVERLAY_CONTRIBUTION_ENTRIES.map(({ field, viewKey }) => [
			viewKey,
			overlays[field],
		]),
	) as FlowOverlayViews;
}
