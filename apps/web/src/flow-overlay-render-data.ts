import { buildFlowOverlayViews } from "./flow-overlay-contribution-registry";
import type { FlowCurrentSceneV9, FlowRationalV1 } from "./flow-scene";
import type { FlowSceneV9OverlayField } from "./flow-scene-wire/generated/overlays";
import { absoluteBigInt } from "./flow-visual-scales";

type FlowOverlaySnapshot = Readonly<
	Pick<FlowCurrentSceneV9, FlowSceneV9OverlayField>
>;

/**
 * Builds the algorithm-specific lookup tables and visual scales consumed by
 * the SVG renderer. Keeping this projection outside React makes the renderer
 * declarative and gives every overlay a single presentation boundary.
 */
export function buildFlowOverlayRenderData(
	scene: FlowCurrentSceneV9,
	overlays: FlowOverlaySnapshot,
) {
	const binaryArcKeys = (
		arcs: readonly { edge_id: string; direction: string }[],
	) => new Set(arcs.map((arc) => `${arc.edge_id}:${arc.direction}`));
	const binaryBaseZeroArcKeys = binaryArcKeys(
		overlays.binary_blocking_overlay?.base_zero_arcs ?? [],
	);
	const binarySpecialArcKeys = binaryArcKeys(
		overlays.binary_blocking_overlay?.special_arcs ?? [],
	);
	const binaryAdmissibleArcKeys = binaryArcKeys(
		overlays.binary_blocking_overlay?.admissible_arcs ?? [],
	);
	const binaryZeroAdmissibleArcKeys = binaryArcKeys(
		overlays.binary_blocking_overlay?.zero_admissible_arcs ?? [],
	);
	const binaryNodeById = new Map(
		(overlays.binary_blocking_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const cancelTightenAdmissibleArcKeys = binaryArcKeys(
		overlays.cancel_tighten_overlay?.admissible_arcs ?? [],
	);
	const cancelTightenCycleArcKeys = binaryArcKeys(
		overlays.cancel_tighten_overlay?.active_cycle ?? [],
	);
	const cancelTightenInspectedArcKeys = binaryArcKeys(
		overlays.cancel_tighten_overlay?.inspected_arcs ?? [],
	);
	const cancelTightenCycleEdges = new Set(
		(overlays.cancel_tighten_overlay?.active_cycle ?? []).map(
			(arc) => arc.edge_id,
		),
	);
	const cancelTightenNodeById = new Map(
		(overlays.cancel_tighten_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const relaxedMndcAssignmentArcKeys = binaryArcKeys(
		(overlays.relaxed_mndc_overlay?.nodes ?? []).flatMap((node) =>
			node.selected_arc === undefined ? [] : [node.selected_arc],
		),
	);
	const relaxedMndcInspectedArcKeys = binaryArcKeys(
		overlays.relaxed_mndc_overlay?.inspected_arcs ?? [],
	);
	const relaxedMndcCycleByArc = new Map<string, number>();
	const relaxedMndcCycleByEdge = new Map<string, number>();
	for (const [cycleIndex, cycle] of (
		overlays.relaxed_mndc_overlay?.family ?? []
	).entries()) {
		for (const arc of cycle.arcs) {
			relaxedMndcCycleByArc.set(`${arc.edge_id}:${arc.direction}`, cycleIndex);
			relaxedMndcCycleByEdge.set(arc.edge_id, cycleIndex);
		}
	}
	const relaxedMndcNodeById = new Map(
		(overlays.relaxed_mndc_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const relaxedMndcFamilyNodeBand = new Map<string, number>();
	for (const arc of scene.residual_arcs) {
		const band = relaxedMndcCycleByArc.get(`${arc.edge_id}:${arc.direction}`);
		if (band !== undefined) relaxedMndcFamilyNodeBand.set(arc.from, band);
	}
	const enhancedScalingNodeById = new Map(
		(overlays.enhanced_capacity_scaling_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const enhancedScalingComponentById = new Map(
		(overlays.enhanced_capacity_scaling_overlay?.components ?? []).map(
			(component) => [component.component_id, component],
		),
	);
	const enhancedScalingEdgeById = new Map(
		(overlays.enhanced_capacity_scaling_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const enhancedScalingPathArcKeys = binaryArcKeys(
		overlays.enhanced_capacity_scaling_overlay?.path ?? [],
	);
	const maximumEnhancedVirtualFlow = (
		overlays.enhanced_capacity_scaling_overlay?.edges ?? []
	).reduce((maximum, edge) => {
		const value = BigInt(edge.virtual_flow.numerator);
		return value > maximum ? value : maximum;
	}, 1n);
	const orlinMcfComponentById = new Map(
		(overlays.orlin_mcf_overlay?.components ?? []).map((component) => [
			component.component_id,
			component,
		]),
	);
	const orlinMcfOriginalNodeById = new Map(
		(overlays.orlin_mcf_overlay?.nodes ?? [])
			.filter((node) => node.kind === "original")
			.map((node) => [node.node_id, node]),
	);
	const orlinMcfCapacityNodeByEdge = new Map(
		(overlays.orlin_mcf_overlay?.nodes ?? [])
			.filter(
				(node) =>
					node.kind === "capacity" && node.capacity_edge_id !== undefined,
			)
			.map((node) => [node.capacity_edge_id as string, node]),
	);
	const orlinMcfBranchesByEdge = new Map<
		string,
		{
			flow?: NonNullable<typeof overlays.orlin_mcf_overlay>["arcs"][number];
			slack?: NonNullable<typeof overlays.orlin_mcf_overlay>["arcs"][number];
		}
	>();
	for (const arc of overlays.orlin_mcf_overlay?.arcs ?? []) {
		const branches = orlinMcfBranchesByEdge.get(arc.edge_id) ?? {};
		branches[arc.branch] = arc;
		orlinMcfBranchesByEdge.set(arc.edge_id, branches);
	}
	const orlinMcfPathArcKeys = new Set(
		(overlays.orlin_mcf_overlay?.path ?? []).map(
			(arc) => `${arc.edge_id}:${arc.branch}:${arc.direction}`,
		),
	);
	const orlinMcfActiveEdges = new Set(
		(overlays.orlin_mcf_overlay?.path ?? []).map((arc) => arc.edge_id),
	);
	const orlinMcfContractionKey =
		overlays.orlin_mcf_overlay?.contraction_arc === undefined
			? undefined
			: `${overlays.orlin_mcf_overlay.contraction_arc.edge_id}:${overlays.orlin_mcf_overlay.contraction_arc.branch}:${overlays.orlin_mcf_overlay.contraction_arc.direction}`;
	const maximumOrlinMcfBranchFlow = (
		overlays.orlin_mcf_overlay?.arcs ?? []
	).reduce<FlowRationalV1>(
		(maximum, arc) => {
			const left = BigInt(arc.flow.numerator) * BigInt(maximum.denominator);
			const right = BigInt(maximum.numerator) * BigInt(arc.flow.denominator);
			return left > right ? arc.flow : maximum;
		},
		{ numerator: "1", denominator: "1" },
	);
	const orlinMaxNodeById = new Map(
		(overlays.orlin_max_flow_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const orlinMaxResidualByKey = new Map(
		(overlays.orlin_max_flow_overlay?.residual_arcs ?? []).map((arc) => [
			`${arc.edge_id}:${arc.direction}`,
			arc,
		]),
	);
	const orlinMaxActiveCompactByOrdinal = new Map(
		(overlays.orlin_max_flow_overlay?.active_compact_path ?? []).map(
			(reference) => [reference.ordinal, reference.reverse],
		),
	);
	const orlinMaxActiveOriginalKeys = new Set(
		(overlays.orlin_max_flow_overlay?.active_original_path ?? []).map(
			(reference) => `${reference.edge_id}:${reference.direction}`,
		),
	);
	const electricalNodeById = new Map(
		(overlays.electrical_flow_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const electricalEdgeById = new Map(
		(overlays.electrical_flow_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const electricalPotentials = (
		overlays.electrical_flow_overlay?.nodes ?? []
	).map((node) => Number(node.potential));
	const minimumElectricalPotential = Math.min(...electricalPotentials, 0);
	const maximumElectricalPotential = Math.max(...electricalPotentials, 0);
	const maximumElectricalEnergy = Math.max(
		...(overlays.electrical_flow_overlay?.edges ?? []).map((edge) =>
			Number(edge.energy),
		),
		0,
	);
	const electricalPotentialBand = (potential: number): number => {
		const span = maximumElectricalPotential - minimumElectricalPotential;
		if (span <= Number.EPSILON) return 2;
		return Math.max(
			0,
			Math.min(
				4,
				Math.round(((potential - minimumElectricalPotential) / span) * 4),
			),
		);
	};
	const electricalEnergyBand = (energy: number): number =>
		maximumElectricalEnergy <= Number.EPSILON
			? 0
			: Math.max(
					1,
					Math.min(4, Math.ceil((energy / maximumElectricalEnergy) * 4)),
				);
	const augmentingElectricalNodeById = new Map(
		(overlays.augmenting_electrical_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const augmentingElectricalEdgeById = new Map(
		(overlays.augmenting_electrical_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const augmentingPotentials = (
		overlays.augmenting_electrical_overlay?.nodes ?? []
	).map((node) => Number(node.potential));
	const minimumAugmentingPotential = Math.min(...augmentingPotentials, 0);
	const maximumAugmentingPotential = Math.max(...augmentingPotentials, 0);
	const maximumAugmentingCoupling = Math.max(
		...(overlays.augmenting_electrical_overlay?.nodes ?? []).map((node) =>
			Math.abs(Number(node.coupling_violation)),
		),
		0,
	);
	const maximumAugmentingCongestion = Math.max(
		...(overlays.augmenting_electrical_overlay?.edges ?? []).map((edge) =>
			Number(edge.congestion),
		),
		0,
	);
	const augmentingPotentialBand = (potential: number): number => {
		const span = maximumAugmentingPotential - minimumAugmentingPotential;
		if (span <= Number.EPSILON) return 2;
		return Math.max(
			0,
			Math.min(
				4,
				Math.round(((potential - minimumAugmentingPotential) / span) * 4),
			),
		);
	};
	const augmentingCongestionBand = (congestion: number): number =>
		maximumAugmentingCongestion <= Number.EPSILON
			? 0
			: Math.max(
					1,
					Math.min(
						4,
						Math.ceil((congestion / maximumAugmentingCongestion) * 4),
					),
				);
	const interiorPointNodeById = new Map(
		(overlays.interior_point_max_flow_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const interiorPointEdgeById = new Map(
		(overlays.interior_point_max_flow_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const interiorPointPotentials = (
		overlays.interior_point_max_flow_overlay?.nodes ?? []
	).map((node) => Number(node.potential));
	const minimumInteriorPointPotential = Math.min(...interiorPointPotentials, 0);
	const maximumInteriorPointPotential = Math.max(...interiorPointPotentials, 0);
	const interiorPointPotentialBand = (potential: number): number => {
		const span = maximumInteriorPointPotential - minimumInteriorPointPotential;
		if (span <= Number.EPSILON) return 2;
		return Math.max(
			0,
			Math.min(
				4,
				Math.round(((potential - minimumInteriorPointPotential) / span) * 4),
			),
		);
	};
	const maximumInteriorPointCongestion = Math.max(
		...(overlays.interior_point_max_flow_overlay?.edges ?? []).map((edge) =>
			Number(edge.congestion),
		),
		0,
	);
	const maximumInteriorPointSlack = Math.max(
		...(overlays.interior_point_max_flow_overlay?.edges ?? []).map((edge) =>
			Number(edge.slack),
		),
		0,
	);
	const maximumInteriorPointResistance = Math.max(
		...(overlays.interior_point_max_flow_overlay?.edges ?? []).map((edge) =>
			Number(edge.resistance),
		),
		0,
	);
	const electricalIpmMcfNodeById = new Map(
		(overlays.electrical_ipm_mcf_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const electricalIpmMcfEdgeById = new Map(
		(overlays.electrical_ipm_mcf_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const electricalIpmMcfPotentials = (
		overlays.electrical_ipm_mcf_overlay?.nodes ?? []
	).map((node) => Number(node.potential));
	const minimumElectricalIpmMcfPotential = Math.min(
		...electricalIpmMcfPotentials,
		0,
	);
	const maximumElectricalIpmMcfPotential = Math.max(
		...electricalIpmMcfPotentials,
		0,
	);
	const electricalIpmMcfPotentialBand = (potential: number): number => {
		const span =
			maximumElectricalIpmMcfPotential - minimumElectricalIpmMcfPotential;
		if (span <= Number.EPSILON) return 2;
		return Math.max(
			0,
			Math.min(
				4,
				Math.round(((potential - minimumElectricalIpmMcfPotential) / span) * 4),
			),
		);
	};
	const maximumElectricalIpmMcfPotentialDirection = Math.max(
		...(overlays.electrical_ipm_mcf_overlay?.nodes ?? []).map((node) =>
			Math.abs(Number(node.potential_direction)),
		),
		0,
	);
	const maximumElectricalIpmMcfSlack = Math.max(
		...(overlays.electrical_ipm_mcf_overlay?.edges ?? []).map((edge) =>
			Math.abs(Number(edge.lower_slack)),
		),
		0,
	);
	const maximumElectricalIpmMcfResistance = Math.max(
		...(overlays.electrical_ipm_mcf_overlay?.edges ?? []).map((edge) =>
			Math.abs(Number(edge.resistance)),
		),
		0,
	);
	const maximumElectricalIpmMcfCurrent = Math.max(
		...(overlays.electrical_ipm_mcf_overlay?.edges ?? []).map((edge) =>
			Math.abs(Number(edge.electrical_current)),
		),
		0,
	);
	const interiorPointMagnitudeBand = (
		value: number,
		maximum: number,
	): number =>
		maximum <= Number.EPSILON
			? 0
			: Math.max(1, Math.min(4, Math.ceil((value / maximum) * 4)));
	const minimumRatioNodeById = new Map(
		(overlays.minimum_ratio_cycle_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const minimumRatioEdgeById = new Map(
		(overlays.minimum_ratio_cycle_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const maximumMinimumRatioLength = (
		overlays.minimum_ratio_cycle_overlay?.edges ?? []
	).reduce(
		(maximum, edge) =>
			BigInt(edge.length) > maximum ? BigInt(edge.length) : maximum,
		1n,
	);
	const maximumMinimumRatioGradient = (
		overlays.minimum_ratio_cycle_overlay?.edges ?? []
	).reduce((maximum, edge) => {
		const magnitude = absoluteBigInt(BigInt(edge.gradient));
		return magnitude > maximum ? magnitude : maximum;
	}, 0n);
	const randomizedInitialPointVisible = ![
		undefined,
		"ready",
		"build-return-edge-reduction",
	].includes(overlays.randomized_almost_linear_overlay?.stage);
	const randomizedVisibleNodes = randomizedInitialPointVisible
		? (overlays.randomized_almost_linear_overlay?.nodes ?? [])
		: [];
	const randomizedVisibleEdges = randomizedInitialPointVisible
		? (overlays.randomized_almost_linear_overlay?.edges ?? [])
		: [];
	const randomizedAlmostLinearNodeById = new Map(
		randomizedVisibleNodes.map((node) => [node.node_id, node]),
	);
	const randomizedAlmostLinearEdgeById = new Map(
		randomizedVisibleEdges.map((edge) => [edge.edge_id, edge]),
	);
	const deterministicInitialPointVisible = ![
		undefined,
		"ready",
		"build-return-edge-reduction",
	].includes(overlays.deterministic_almost_linear_overlay?.stage);
	const deterministicVisibleNodes = deterministicInitialPointVisible
		? (overlays.deterministic_almost_linear_overlay?.nodes ?? [])
		: [];
	const deterministicVisibleEdges = deterministicInitialPointVisible
		? (overlays.deterministic_almost_linear_overlay?.edges ?? [])
		: [];
	const deterministicAlmostLinearNodeById = new Map(
		deterministicVisibleNodes.map((node) => [node.node_id, node]),
	);
	const deterministicAlmostLinearEdgeById = new Map(
		deterministicVisibleEdges.map((edge) => [edge.edge_id, edge]),
	);
	const maximumRandomizedLength = Math.max(
		...(overlays.randomized_almost_linear_overlay?.edges ?? []).map((edge) =>
			Number(edge.length),
		),
		1,
	);
	const maximumRandomizedGradient = Math.max(
		...(overlays.randomized_almost_linear_overlay?.edges ?? []).map((edge) =>
			Math.abs(Number(edge.gradient)),
		),
		0,
	);
	const randomizedMagnitudeBand = (value: number, maximum: number): number =>
		maximum <= Number.EPSILON
			? 0
			: Math.max(1, Math.min(4, Math.ceil((Math.abs(value) / maximum) * 4)));
	const maximumDeterministicLength = Math.max(
		...(overlays.deterministic_almost_linear_overlay?.edges ?? []).map((edge) =>
			Math.abs(Number(edge.length)),
		),
		1,
	);
	const maximumDeterministicGradient = Math.max(
		...(overlays.deterministic_almost_linear_overlay?.edges ?? []).map((edge) =>
			Math.abs(Number(edge.gradient)),
		),
		0,
	);
	const dualSimplexNodeById = new Map(
		(overlays.dual_network_simplex_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const dualSimplexEdgeById = new Map(
		(overlays.dual_network_simplex_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const polynomialDualNodeById = new Map(
		(overlays.polynomial_dual_simplex_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const polynomialDualEdgeById = new Map(
		(overlays.polynomial_dual_simplex_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const maximumPolynomialDualPseudoflow = (
		overlays.polynomial_dual_simplex_overlay?.edges ?? []
	).reduce<FlowRationalV1>(
		(maximum, edge) => {
			const edgeMagnitude = absoluteBigInt(BigInt(edge.pseudoflow.numerator));
			const maximumMagnitude = absoluteBigInt(BigInt(maximum.numerator));
			return edgeMagnitude * BigInt(maximum.denominator) >
				maximumMagnitude * BigInt(edge.pseudoflow.denominator)
				? edge.pseudoflow
				: maximum;
		},
		{ numerator: "0", denominator: "1" },
	);
	const polynomialPrimalNodeById = new Map(
		(overlays.polynomial_primal_simplex_overlay?.nodes ?? [])
			.filter((node) => node.kind === "original")
			.map((node) => [node.entity_id, node]),
	);
	const polynomialPrimalEdgeById = new Map(
		(overlays.polynomial_primal_simplex_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const doubleScalingArcKeys = (
		arcs: readonly {
			edge_id: string;
			branch: string;
			direction: string;
		}[],
	) =>
		new Set(arcs.map((arc) => `${arc.edge_id}:${arc.branch}:${arc.direction}`));
	const doubleScalingAdmissibleArcKeys = doubleScalingArcKeys(
		overlays.double_scaling_overlay?.admissible_arcs ?? [],
	);
	const doubleScalingActiveArcKeys = doubleScalingArcKeys(
		overlays.double_scaling_overlay?.active_path ?? [],
	);
	const doubleScalingInspectedArc =
		overlays.double_scaling_overlay?.inspected_arc;
	const doubleScalingActiveEdges = new Set(
		(overlays.double_scaling_overlay?.active_path ?? []).map(
			(arc) => arc.edge_id,
		),
	);
	const doubleScalingActiveBranches = new Map<string, string[]>();
	for (const arc of overlays.double_scaling_overlay?.active_path ?? []) {
		const labels = doubleScalingActiveBranches.get(arc.edge_id) ?? [];
		labels.push(`${arc.branch}:${arc.direction}`);
		doubleScalingActiveBranches.set(arc.edge_id, labels);
	}
	const doubleScalingOriginalNodeById = new Map(
		(overlays.double_scaling_overlay?.nodes ?? [])
			.filter((node) => node.kind === "original")
			.map((node) => [node.entity_id, node]),
	);
	const doubleScalingEdgeById = new Map(
		(overlays.double_scaling_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const convexCostEdgeById = new Map(
		(overlays.convex_cost_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const convexSimplexEdgeById = new Map(
		(overlays.convex_network_simplex_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const convexSimplexNodeById = new Map(
		(overlays.convex_network_simplex_overlay?.nodes ?? []).map((node) => [
			node.entity_id,
			node,
		]),
	);
	const tardosNodeById = new Map(
		(overlays.tardos_framework_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const tardosResidualByArc = new Map(
		(overlays.tardos_framework_overlay?.residual_arcs ?? []).map((arc) => [
			`${arc.edge_id}:${arc.direction}`,
			arc,
		]),
	);
	const tardosFixedByEdge = new Map(
		(overlays.tardos_framework_overlay?.fixed_variables ?? []).map((fixed) => [
			fixed.edge_id,
			fixed,
		]),
	);
	const maximumTardosReducedCost = (
		overlays.tardos_framework_overlay?.residual_arcs ?? []
	).reduce((maximum, arc) => {
		const magnitude = absoluteBigInt(BigInt(arc.reduced_cost));
		return magnitude > maximum ? magnitude : maximum;
	}, 0n);
	const predictionNodeById = new Map(
		(overlays.prediction_assisted_epsilon_overlay?.nodes ?? []).map((node) => [
			node.node_id,
			node,
		]),
	);
	const predictionEdgeById = new Map(
		(overlays.prediction_assisted_epsilon_overlay?.edges ?? []).map((edge) => [
			edge.edge_id,
			edge,
		]),
	);
	const predictionActiveArcKey =
		overlays.prediction_assisted_epsilon_overlay?.active_arc === undefined
			? undefined
			: `${overlays.prediction_assisted_epsilon_overlay.active_arc.edge_id}:${overlays.prediction_assisted_epsilon_overlay.active_arc.direction}`;
	const maximumPredictionScaledCost = (
		overlays.prediction_assisted_epsilon_overlay?.edges ?? []
	).reduce((maximum, edge) => {
		const magnitude = absoluteBigInt(BigInt(edge.scaled_cost));
		return magnitude > maximum ? magnitude : maximum;
	}, 0n);
	const convexActiveDirectionsByEdge = new Map<
		string,
		Set<"forward" | "reverse">
	>();
	for (const arc of overlays.convex_cost_overlay?.active_cycle ?? []) {
		const directions =
			convexActiveDirectionsByEdge.get(arc.edge_id) ??
			new Set<"forward" | "reverse">();
		directions.add(arc.direction);
		convexActiveDirectionsByEdge.set(arc.edge_id, directions);
	}
	for (const arc of overlays.convex_network_simplex_overlay?.cycle ?? []) {
		if (!convexSimplexEdgeById.has(arc.entity_id)) continue;
		const directions =
			convexActiveDirectionsByEdge.get(arc.entity_id) ??
			new Set<"forward" | "reverse">();
		directions.add(arc.direction);
		convexActiveDirectionsByEdge.set(arc.entity_id, directions);
	}
	const convexEligibleDirectionsByEdge = new Map<
		string,
		Set<"forward" | "reverse">
	>();
	// Initialization installs segment occupancy but has not opened the first
	// delta-residual network yet. A completed scale closes that network after
	// proving that no source-to-deficit path remains. Exposing eligibility only
	// while a scale is live makes those exact algorithm boundaries visible
	// without inventing progress or highlighting every edge twice.
	const convexEligibilityIsLive = ![
		undefined,
		"initialize",
		"complete-scale",
		"optimal",
	].includes(overlays.convex_cost_overlay?.stage);
	for (const arc of convexEligibilityIsLive
		? (overlays.convex_cost_overlay?.eligible_arcs ?? [])
		: []) {
		const directions =
			convexEligibleDirectionsByEdge.get(arc.edge_id) ??
			new Set<"forward" | "reverse">();
		directions.add(arc.direction);
		convexEligibleDirectionsByEdge.set(arc.edge_id, directions);
	}
	const maximumConvexMarginalMagnitude = (
		overlays.convex_cost_overlay?.edges ?? []
	).reduce((maximum, edge) => {
		return edge.segments.reduce((segmentMaximum, segment) => {
			const magnitude = absoluteBigInt(BigInt(segment.marginal_cost));
			return magnitude > segmentMaximum ? magnitude : segmentMaximum;
		}, maximum);
	}, 0n);
	return {
		overlayViews: buildFlowOverlayViews(overlays),
		binaryArcKeys,
		binaryBaseZeroArcKeys,
		binarySpecialArcKeys,
		binaryAdmissibleArcKeys,
		binaryZeroAdmissibleArcKeys,
		binaryNodeById,
		cancelTightenAdmissibleArcKeys,
		cancelTightenCycleArcKeys,
		cancelTightenInspectedArcKeys,
		cancelTightenCycleEdges,
		cancelTightenNodeById,
		relaxedMndcAssignmentArcKeys,
		relaxedMndcInspectedArcKeys,
		relaxedMndcCycleByArc,
		relaxedMndcCycleByEdge,
		relaxedMndcNodeById,
		relaxedMndcFamilyNodeBand,
		enhancedScalingNodeById,
		enhancedScalingComponentById,
		enhancedScalingEdgeById,
		enhancedScalingPathArcKeys,
		maximumEnhancedVirtualFlow,
		orlinMcfComponentById,
		orlinMcfOriginalNodeById,
		orlinMcfCapacityNodeByEdge,
		orlinMcfBranchesByEdge,
		orlinMcfPathArcKeys,
		orlinMcfActiveEdges,
		orlinMcfContractionKey,
		maximumOrlinMcfBranchFlow,
		orlinMaxNodeById,
		orlinMaxResidualByKey,
		orlinMaxActiveCompactByOrdinal,
		orlinMaxActiveOriginalKeys,
		electricalNodeById,
		electricalEdgeById,
		electricalPotentials,
		minimumElectricalPotential,
		maximumElectricalPotential,
		maximumElectricalEnergy,
		electricalPotentialBand,
		electricalEnergyBand,
		augmentingElectricalNodeById,
		augmentingElectricalEdgeById,
		augmentingPotentials,
		minimumAugmentingPotential,
		maximumAugmentingPotential,
		maximumAugmentingCoupling,
		maximumAugmentingCongestion,
		augmentingPotentialBand,
		augmentingCongestionBand,
		interiorPointNodeById,
		interiorPointEdgeById,
		interiorPointPotentials,
		minimumInteriorPointPotential,
		maximumInteriorPointPotential,
		interiorPointPotentialBand,
		maximumInteriorPointCongestion,
		maximumInteriorPointSlack,
		maximumInteriorPointResistance,
		interiorPointMagnitudeBand,
		electricalIpmMcfNodeById,
		electricalIpmMcfEdgeById,
		electricalIpmMcfPotentialBand,
		maximumElectricalIpmMcfPotentialDirection,
		maximumElectricalIpmMcfSlack,
		maximumElectricalIpmMcfResistance,
		maximumElectricalIpmMcfCurrent,
		minimumRatioNodeById,
		minimumRatioEdgeById,
		maximumMinimumRatioLength,
		maximumMinimumRatioGradient,
		randomizedAlmostLinearNodeById,
		randomizedAlmostLinearEdgeById,
		deterministicAlmostLinearNodeById,
		deterministicAlmostLinearEdgeById,
		maximumRandomizedLength,
		maximumRandomizedGradient,
		randomizedMagnitudeBand,
		maximumDeterministicLength,
		maximumDeterministicGradient,
		dualSimplexNodeById,
		dualSimplexEdgeById,
		polynomialDualNodeById,
		polynomialDualEdgeById,
		maximumPolynomialDualPseudoflow,
		polynomialPrimalNodeById,
		polynomialPrimalEdgeById,
		doubleScalingArcKeys,
		doubleScalingAdmissibleArcKeys,
		doubleScalingActiveArcKeys,
		doubleScalingInspectedArc,
		doubleScalingActiveEdges,
		doubleScalingActiveBranches,
		doubleScalingOriginalNodeById,
		doubleScalingEdgeById,
		convexCostEdgeById,
		convexSimplexEdgeById,
		convexSimplexNodeById,
		tardosNodeById,
		tardosResidualByArc,
		tardosFixedByEdge,
		maximumTardosReducedCost,
		predictionNodeById,
		predictionEdgeById,
		predictionActiveArcKey,
		maximumPredictionScaledCost,
		convexActiveDirectionsByEdge,
		convexEligibleDirectionsByEdge,
		maximumConvexMarginalMagnitude,
	};
}

export type FlowOverlayRenderData = ReturnType<
	typeof buildFlowOverlayRenderData
>;
