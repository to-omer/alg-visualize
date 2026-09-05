import type { ReactNode } from "react";
import {
	FlowGraphOverlayOwnedLeaves,
	type FlowOverlayLeafOwner,
} from "./FlowGraphOverlayOwnedLeaves";
import { flowScopedSvgUrl, useFlowGraphIdScope } from "./flow-dom-id";
import {
	type projectFlowEntityGraphState,
	rationalMagnitudeStrokeWidth,
} from "./flow-entity-graph-state";
import { costMagnitudeBand } from "./flow-visual-scales";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
type OriginalVisual = FlowEntityGraphState["originalVisuals"][number];

function OwnedDiscreteEdgeFeature({
	state,
	visual,
	owners,
	entity = { kind: "edge" },
	children,
}: Readonly<{
	state: FlowEntityGraphState;
	visual: OriginalVisual;
	owners: readonly FlowOverlayLeafOwner[];
	entity?: Readonly<{
		kind: "edge" | "residual-arc" | "auxiliary-residual-arc";
		direction?: "forward" | "reverse" | undefined;
	}>;
	children: ReactNode;
}>) {
	if (owners.length === 0) return children;
	return (
		<FlowGraphOverlayOwnedLeaves
			state={state}
			bundle="original-edge-discrete-overlay"
			entity={{
				kind: entity.kind,
				id: visual.edge.id,
				direction: entity.direction,
			}}
			owners={owners}
		>
			{children}
		</FlowGraphOverlayOwnedLeaves>
	);
}

function algorithmOverlayRole(
	state: FlowEntityGraphState,
	visual: OriginalVisual,
): string | undefined {
	const views = state.renderData.overlayViews;
	if (views.dualNetworkSimplex?.inspected_edge === visual.edge.id)
		return "current";
	if (
		visual.dualSimplexState !== undefined &&
		(BigInt(visual.dualSimplexState.basic_flow) < 0n ||
			views.dualNetworkSimplex?.leaving_edge === visual.edge.id)
	)
		return "danger";
	if (
		visual.polynomialPrimalState?.leaving ||
		visual.convexSimplexState?.leaving ||
		views.polynomialDualSimplex?.leaving_edge === visual.edge.id
	)
		return "danger";
	if (
		visual.cancelTightenCycle ||
		visual.relaxedMndcCycle !== undefined ||
		visual.polynomialPrimalState?.in_cycle ||
		visual.convexSimplexState?.in_cycle
	)
		return "cycle";
	if (visual.cycleAdjustment === "add") return "add";
	if (visual.cycleAdjustment === "subtract") return "subtract";
	if (visual.convexEligibleDirections !== undefined) return "eligible";
	if (
		visual.matched ||
		visual.fixed ||
		visual.crossesCut ||
		visual.enhancedScalingState?.strongly_feasible
	)
		return visual.matched ? "matched" : visual.fixed ? "fixed" : "strong";
	if (
		visual.dualSimplexState?.in_tree ||
		visual.polynomialDualState?.in_tree ||
		visual.polynomialPrimalState?.basis === "tree" ||
		visual.convexSimplexState?.basis === "tree"
	)
		return "tree";
	if (
		visual.enhancedScalingState?.internal ||
		visual.dualSimplexState !== undefined ||
		visual.polynomialPrimalState?.basis === "lower" ||
		visual.polynomialPrimalState?.basis === "upper" ||
		visual.convexSimplexState?.basis === "breakpoint"
	)
		return "secondary";
	if (visual.enhancedScalingState?.tight) return "tight";
	return undefined;
}

export function FlowGraphDiscreteEdgeOverlayBundle({
	state,
	visual,
	enabled,
}: Readonly<{
	state: FlowEntityGraphState;
	visual: OriginalVisual;
	enabled: boolean;
}>) {
	const idScope = useFlowGraphIdScope();
	if (!enabled) return null;
	const maximumPolynomialDualPseudoflow =
		state.renderData.maximumPolynomialDualPseudoflow;
	const maximumPredictionScaledCost =
		state.renderData.maximumPredictionScaledCost;
	const tardosFixedByEdge = state.renderData.tardosFixedByEdge;
	const overlayRole = algorithmOverlayRole(state, visual);
	const overlayRoleOwners: FlowOverlayLeafOwner[] = [
		...(visual.cancelTightenCycle
			? [
					{
						overlay: "cancel_tighten_overlay" as const,
						role: "active_cycle",
					},
				]
			: []),
		...(visual.relaxedMndcCycle === undefined
			? []
			: [
					{
						overlay: "relaxed_mndc_overlay" as const,
						role: "family.cycle",
					},
				]),
		...(visual.enhancedScalingState === undefined
			? []
			: [
					{
						overlay: "enhanced_capacity_scaling_overlay" as const,
						role: "edges.quotient-state",
					},
				]),
		...(visual.dualSimplexState === undefined
			? []
			: [
					{
						overlay: "dual_network_simplex_overlay" as const,
						role: "edges.basis-and-pivot-state",
					},
				]),
		...(visual.polynomialDualState === undefined
			? []
			: [
					{
						overlay: "polynomial_dual_simplex_overlay" as const,
						role: "edges.auxiliary-basis-state",
					},
				]),
		...(visual.polynomialPrimalState === undefined
			? []
			: [
					{
						overlay: "polynomial_primal_simplex_overlay" as const,
						role: "edges.perturbed-basis-state",
					},
				]),
		...(visual.convexSimplexState === undefined
			? []
			: [
					{
						overlay: "convex_network_simplex_overlay" as const,
						role: "edges.compact-basis-state",
					},
				]),
		...(visual.convexState === undefined
			? []
			: [
					{
						overlay: "convex_cost_overlay" as const,
						role: "edges.marginal-cost-state",
					},
				]),
	];
	return (
		<>
			{visual.doubleScalingBranches?.map((activeBranch) => {
				const [branch, direction] = activeBranch.split(":") as [
					"flow" | "slack",
					"forward" | "reverse",
				];
				return (
					<OwnedDiscreteEdgeFeature
						key={`double-active:${activeBranch}`}
						state={state}
						visual={visual}
						owners={[
							{
								overlay: "double_scaling_overlay",
								role: `active_path.${branch}.${direction}`,
							},
						]}
						entity={{ kind: "auxiliary-residual-arc", direction }}
					>
						<path
							d={
								direction === "reverse"
									? visual.geometry.reversePath
									: visual.geometry.path
							}
							className={`flow-double-scaling-active-branch flow-double-scaling-active-${branch} flow-double-scaling-active-${direction}`}
							data-double-scaling-active-branch={branch}
							data-double-scaling-active-direction={direction}
							strokeWidth={visual.railWidth + (branch === "flow" ? 3 : 5)}
							markerEnd={flowScopedSvgUrl(
								idScope,
								"flow-arrow-residual-active",
							)}
						>
							<title>{`${visual.edge.id}: active transformed ${branch} branch, ${direction}`}</title>
						</path>
					</OwnedDiscreteEdgeFeature>
				);
			})}
			{visual.doubleScalingInspectedArc !== undefined && (
				<OwnedDiscreteEdgeFeature
					state={state}
					visual={visual}
					owners={[
						{
							overlay: "double_scaling_overlay",
							role: `inspected_arc.${visual.doubleScalingInspectedArc.branch}`,
						},
					]}
					entity={{
						kind: "residual-arc",
						direction: visual.doubleScalingInspectedArc.direction,
					}}
				>
					<path
						d={
							visual.doubleScalingInspectedArc.direction === "reverse"
								? visual.geometry.reversePath
								: visual.geometry.path
						}
						className={`flow-double-scaling-scan-overlay flow-double-scaling-scan-${visual.doubleScalingInspectedArc.branch} flow-double-scaling-scan-${visual.doubleScalingInspectedArc.direction}`}
						data-double-scaling-scan-branch={
							visual.doubleScalingInspectedArc.branch
						}
						data-double-scaling-scan-direction={
							visual.doubleScalingInspectedArc.direction
						}
						strokeWidth={visual.railWidth + 7}
						markerEnd={flowScopedSvgUrl(idScope, "flow-arrow-residual-active")}
					>
						<title>{`${visual.edge.id} · inspect ${visual.doubleScalingInspectedArc.branch} branch ${visual.doubleScalingInspectedArc.direction}`}</title>
					</path>
				</OwnedDiscreteEdgeFeature>
			)}
			{overlayRole !== undefined && (
				<OwnedDiscreteEdgeFeature
					state={state}
					visual={visual}
					owners={overlayRoleOwners}
				>
					<path
						d={
							visual.convexActiveDirections?.has("reverse") === true &&
							visual.convexActiveDirections?.has("forward") !== true
								? visual.geometry.reversePath
								: visual.geometry.path
						}
						className={`flow-algorithm-edge-overlay flow-algorithm-edge-overlay-${overlayRole}`}
						data-algorithm-edge-role={overlayRole}
						strokeWidth={
							overlayRole === "eligible"
								? Math.max(2, visual.railWidth + 1)
								: visual.railWidth + 6
						}
						markerEnd={flowScopedSvgUrl(idScope, "flow-arrow-residual-active")}
					/>
				</OwnedDiscreteEdgeFeature>
			)}
			{tardosFixedByEdge.has(visual.edge.id) && (
				<OwnedDiscreteEdgeFeature
					state={state}
					visual={visual}
					owners={[
						{
							overlay: "tardos_framework_overlay",
							role: "fixed_variables",
						},
					]}
				>
					<path
						d={
							tardosFixedByEdge.get(visual.edge.id)?.direction === "reverse"
								? visual.geometry.reversePath
								: visual.geometry.path
						}
						className={`flow-tardos-fixed-overlay flow-tardos-fixed-${tardosFixedByEdge.get(visual.edge.id)?.bound}`}
						strokeWidth={visual.railWidth + 5}
						markerEnd={flowScopedSvgUrl(idScope, "flow-arrow-residual-active")}
					/>
				</OwnedDiscreteEdgeFeature>
			)}
			{visual.predictionState !== undefined && (
				<OwnedDiscreteEdgeFeature
					state={state}
					visual={visual}
					owners={[
						{
							overlay: "prediction_assisted_epsilon_overlay",
							role: "edges.scaled-cost-and-balanced-arc",
						},
					]}
				>
					<path
						d={
							visual.predictionActiveDirection === "reverse"
								? visual.geometry.reversePath
								: visual.geometry.path
						}
						className={`flow-prediction-scaled-cost-overlay flow-prediction-scaled-${BigInt(visual.predictionState.scaled_cost) < 0n ? "negative" : BigInt(visual.predictionState.scaled_cost) > 0n ? "positive" : "zero"} flow-prediction-band-${costMagnitudeBand(BigInt(visual.predictionState.scaled_cost), maximumPredictionScaledCost)}${visual.predictionActiveDirection === undefined ? "" : " flow-prediction-scaled-active"}`}
						data-prediction-scaled-cost={visual.predictionState.scaled_cost}
						data-prediction-active-direction={visual.predictionActiveDirection}
						strokeWidth={
							2 +
							costMagnitudeBand(
								BigInt(visual.predictionState.scaled_cost),
								maximumPredictionScaledCost,
							)
						}
						markerEnd={
							visual.predictionActiveDirection === undefined
								? undefined
								: flowScopedSvgUrl(idScope, "flow-arrow-residual-active")
						}
					>
						<title>{`${visual.edge.id} · scaled cost ${visual.predictionState.scaled_cost}${visual.predictionActiveDirection === undefined ? "" : ` · active ${visual.predictionActiveDirection} ε-balanced arc`}`}</title>
					</path>
				</OwnedDiscreteEdgeFeature>
			)}
			{visual.polynomialDualState !== undefined &&
				BigInt(visual.polynomialDualState.pseudoflow.numerator) !== 0n && (
					<path
						d={visual.geometry.path}
						className="flow-polynomial-dual-pseudoflow-overlay"
						strokeWidth={rationalMagnitudeStrokeWidth(
							visual.polynomialDualState.pseudoflow,
							maximumPolynomialDualPseudoflow,
						)}
					/>
				)}
			{visual.polynomialDualState?.in_augment_path && (
				<path
					d={
						visual.polynomialDualState.augment_direction === "reverse"
							? visual.geometry.reversePath
							: visual.geometry.path
					}
					className="flow-polynomial-dual-path-overlay"
					strokeWidth={visual.railWidth + 4}
					markerEnd={flowScopedSvgUrl(idScope, "flow-arrow-residual")}
				/>
			)}
			{visual.polynomialPrimalState?.in_cycle && (
				<path
					d={visual.geometry.path}
					className="flow-polynomial-cycle-overlay"
					strokeWidth={visual.railWidth + 3}
				/>
			)}
			{visual.convexSimplexState?.in_cycle && (
				<path
					d={
						visual.convexActiveDirections?.has("reverse") === true
							? visual.geometry.reversePath
							: visual.geometry.path
					}
					className="flow-convex-simplex-cycle-overlay"
					strokeWidth={visual.railWidth + 4}
					markerEnd={flowScopedSvgUrl(idScope, "flow-arrow-residual-active")}
				/>
			)}
		</>
	);
}
