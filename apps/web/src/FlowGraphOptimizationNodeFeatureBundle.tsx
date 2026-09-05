import type { ReactNode } from "react";
import { FlowGraphOverlayOwnedLeaves } from "./FlowGraphOverlayOwnedLeaves";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import {
	flowPolynomialPrimalScan,
	ordinaryFlowEventEntityRefs,
} from "./flow-event-highlight";
import {
	enhancedCapacityScalingGateStatus,
	orlinMcfBelowGateWitness,
	orlinMcfPhaseGateStatus,
	polynomialDualScaleGateStatus,
} from "./flow-graph-rational-scales";
import { formatFlowRational } from "./flow-parametric-view";
import type { FlowSceneV9OverlayField } from "./flow-scene-wire/generated/overlays";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;

function OwnedOptimizationNodeFeature({
	state,
	nodeId,
	overlay,
	sourceRole,
	children,
}: Readonly<{
	state: FlowEntityGraphState;
	nodeId: string;
	overlay: FlowSceneV9OverlayField;
	sourceRole: string;
	children: ReactNode;
}>) {
	return (
		<FlowGraphOverlayOwnedLeaves
			state={state}
			bundle="node-optimization"
			entity={{ kind: "node", id: nodeId }}
			owners={[{ overlay, role: sourceRole }]}
		>
			{children}
		</FlowGraphOverlayOwnedLeaves>
	);
}

/** Scaling, simplex, prediction, and discrete optimization node glyphs. */
export function FlowGraphOptimizationNodeFeatureBundle({
	state,
	nodeId,
	supernode,
	enabled,
}: Readonly<{
	state: FlowEntityGraphState;
	nodeId: string;
	supernode: boolean;
	enabled: boolean;
}>) {
	if (!enabled) return null;
	const data = state.renderData;
	const views = data.overlayViews;
	const parametricForestOrientation =
		views.parametric?.traversal?.kind === "initialize-forest"
			? views.parametric.traversal.orientation
			: undefined;
	const parametricChildPolicy =
		views.parametric?.traversal?.kind === "restart-smaller-child" ||
		views.parametric?.traversal?.kind === "continue-larger-child"
			? views.parametric.traversal.kind
			: undefined;
	const predictionNode = data.predictionNodeById.get(nodeId);
	const tardosNode = data.tardosNodeById.get(nodeId);
	const tardosPotentialAnchor =
		tardosNode !== undefined &&
		views.tardosFramework?.nodes[0]?.node_id === nodeId;
	const tardosAnchorStage = views.tardosFramework?.stage;
	const tardosClassifiedCount =
		views.tardosFramework?.fixed_variables.length ?? 0;
	const tardosAnchorGlyph =
		tardosAnchorStage === "complete"
			? "✓"
			: tardosAnchorStage === "classify-fixed-variables"
				? tardosClassifiedCount === 0
					? "∅"
					: "F"
				: "π";
	const binaryNode = data.binaryNodeById.get(nodeId);
	const binaryComponentsPublished =
		views.binaryBlocking?.stage === "contracted" ||
		views.binaryBlocking?.stage === "complete";
	const binaryComponentBand =
		binaryNode === undefined || !binaryComponentsPublished
			? undefined
			: Number(BigInt(binaryNode.component) % 4n);
	const relaxedMndcFamilyBand = data.relaxedMndcFamilyNodeBand.get(nodeId);
	const relaxedMndcInitializing =
		views.relaxedMndc?.stage === "initialize" &&
		state.context.traceEventSemantics?.changed_entity_refs.some(
			(reference) => reference.kind === "node" && reference.node_id === nodeId,
		) === true;
	const enhancedView = views.enhancedCapacityScaling;
	const enhancedNode = data.enhancedScalingNodeById.get(nodeId);
	const enhancedComponent =
		enhancedNode === undefined
			? undefined
			: data.enhancedScalingComponentById.get(enhancedNode.component_id);
	const enhancedPhaseGateStage =
		enhancedView?.stage === "begin-phase"
			? "active"
			: enhancedView?.stage === "halve-scale"
				? "next"
				: undefined;
	const enhancedPhaseGateStatus =
		enhancedView !== undefined &&
		enhancedPhaseGateStage !== undefined &&
		enhancedComponent?.component_id === nodeId
			? enhancedCapacityScalingGateStatus(
					enhancedComponent.excess,
					enhancedView.delta,
				)
			: undefined;
	const enhancedPhaseGateRole =
		enhancedPhaseGateStatus === "excess" ||
		enhancedPhaseGateStatus === "deficit"
			? enhancedPhaseGateStatus
			: undefined;
	const enhancedRole =
		enhancedNode === undefined
			? undefined
			: enhancedView?.source_component === enhancedNode.component_id
				? "source"
				: enhancedView?.sink_component === enhancedNode.component_id
					? "sink"
					: undefined;
	const orlinNode = data.orlinMcfOriginalNodeById.get(nodeId);
	const orlinComponent =
		orlinNode === undefined
			? undefined
			: data.orlinMcfComponentById.get(orlinNode.component_id);
	const orlinPhaseGate =
		views.orlinMcf?.stage === "begin-phase" &&
		orlinComponent?.members.find((member) =>
			data.orlinMcfOriginalNodeById.has(member),
		) === nodeId
			? orlinMcfPhaseGateStatus(orlinComponent.excess, views.orlinMcf.delta)
			: undefined;
	const orlinPhaseGateRole =
		orlinPhaseGate === "excess" || orlinPhaseGate === "deficit"
			? orlinPhaseGate
			: undefined;
	const orlinBelowGateWitness =
		views.orlinMcf?.stage === "begin-phase"
			? orlinMcfBelowGateWitness(
					views.orlinMcf.components,
					views.orlinMcf.delta,
				)
			: undefined;
	const orlinBelowGateWitnessNode =
		orlinBelowGateWitness?.members.find((member) =>
			data.orlinMcfOriginalNodeById.has(member),
		) === nodeId;
	const orlinDualExpansion =
		orlinNode !== undefined && views.orlinMcf?.stage === "expand-dual";
	const orlinRole =
		orlinNode === undefined
			? undefined
			: views.orlinMcf?.source_component === orlinNode.component_id
				? "source"
				: views.orlinMcf?.sink_component === orlinNode.component_id
					? "sink"
					: undefined;
	const dualNode = data.dualSimplexNodeById.get(nodeId);
	const polynomialDualNode = data.polynomialDualNodeById.get(nodeId);
	const polynomialDualScaleGate =
		polynomialDualNode !== undefined &&
		!polynomialDualNode.root &&
		views.polynomialDualSimplex?.stage === "begin-scale"
			? polynomialDualScaleGateStatus(
					polynomialDualNode.excess,
					views.polynomialDualSimplex.delta,
				)
			: undefined;
	const polynomialPrimalNode = data.polynomialPrimalNodeById.get(nodeId);
	const doubleScalingNode = data.doubleScalingOriginalNodeById.get(nodeId);
	const doubleScalingImbalance =
		doubleScalingNode === undefined ? 0n : BigInt(doubleScalingNode.imbalance);
	const doubleScalingImbalanceSign =
		doubleScalingImbalance > 0n
			? "excess"
			: doubleScalingImbalance < 0n
				? "deficit"
				: "balanced";
	const doubleScalingDelta = BigInt(views.doubleScaling?.delta ?? "0");
	const doubleScalingDeltaGate =
		doubleScalingNode !== undefined &&
		views.doubleScaling?.stage === "start-capacity-phase" &&
		doubleScalingDelta > 0n &&
		(doubleScalingImbalance < 0n
			? -doubleScalingImbalance
			: doubleScalingImbalance) >= doubleScalingDelta;
	const eligible = polynomialPrimalNode?.flags.includes("eligible") === true;
	const awake = polynomialPrimalNode?.flags.includes("awake") === true;
	const nStar = polynomialPrimalNode?.flags.includes("in-n-star") === true;
	const root = polynomialPrimalNode?.flags.includes("root") === true;
	const sourceEventFocus = ordinaryFlowEventEntityRefs(state.context).some(
		(reference) => reference.kind === "node" && reference.node_id === nodeId,
	);
	const polynomialPrimalScan = flowPolynomialPrimalScan(state.context);
	const nodeScan =
		polynomialPrimalScan?.target.kind === "node" &&
		polynomialPrimalScan.target.node_id === nodeId
			? polynomialPrimalScan
			: undefined;

	return (
		<>
			{supernode && <circle className="flow-supernode-ring" r="35" />}
			{tardosPotentialAnchor && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="tardos_framework_overlay"
					sourceRole="nodes.canonical-potential-anchor"
				>
					<g
						className={`flow-tardos-potential-anchor${tardosAnchorStage === "classify-fixed-variables" ? " flow-tardos-potential-anchor-classified" : ""}${tardosAnchorStage === "complete" ? " flow-tardos-potential-anchor-complete" : ""}`}
						data-tardos-potential-anchor={tardosNode.potential}
						data-tardos-classified-count={
							tardosAnchorStage === "classify-fixed-variables"
								? tardosClassifiedCount
								: undefined
						}
						data-tardos-certificate-complete={
							tardosAnchorStage === "complete" || undefined
						}
						transform="translate(-31 -31)"
					>
						<title>
							{tardosAnchorStage === "complete"
								? `${nodeId}: Tardos variable-fixing certificate complete`
								: tardosAnchorStage === "classify-fixed-variables"
									? `${nodeId}: Tardos classification found ${tardosClassifiedCount} fixed variables`
									: `${nodeId}: canonical Tardos potential π ${tardosNode.potential}`}
						</title>
						<circle r="8" />
						<text textAnchor="middle" dominantBaseline="central">
							{tardosAnchorGlyph}
						</text>
					</g>
				</OwnedOptimizationNodeFeature>
			)}
			{doubleScalingNode !== undefined && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="double_scaling_overlay"
					sourceRole="nodes.imbalance-state"
				>
					{doubleScalingImbalanceSign === "balanced" ? (
						<path
							className="flow-double-scaling-imbalance flow-double-scaling-imbalance-balanced"
							data-double-scaling-imbalance={doubleScalingNode.imbalance}
							d="M -35 26 L -31 22 L -27 26 L -31 30 Z"
						>
							<title>{`${nodeId}: transformed imbalance 0 (balanced)`}</title>
						</path>
					) : (
						<path
							className={`flow-double-scaling-imbalance flow-double-scaling-imbalance-${doubleScalingImbalanceSign}`}
							data-double-scaling-imbalance={doubleScalingNode.imbalance}
							d={
								doubleScalingImbalanceSign === "excess"
									? "M -38 31 L -31 19 L -24 31 Z"
									: "M -38 21 L -31 33 L -24 21 Z"
							}
						>
							<title>{`${nodeId}: transformed imbalance ${doubleScalingNode.imbalance} (${doubleScalingImbalanceSign})`}</title>
						</path>
					)}
				</OwnedOptimizationNodeFeature>
			)}
			{doubleScalingNode !== undefined && doubleScalingDeltaGate && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="double_scaling_overlay"
					sourceRole="nodes.delta-gate"
				>
					<circle
						className={`flow-double-scaling-delta-gate flow-double-scaling-delta-gate-${doubleScalingImbalanceSign}`}
						data-double-scaling-delta={views.doubleScaling?.delta}
						cx="-31"
						cy="26"
						r="11"
					>
						<title>{`${nodeId}: |imbalance| ${doubleScalingImbalance < 0n ? -doubleScalingImbalance : doubleScalingImbalance} ≥ capacity scale Δ ${doubleScalingDelta}`}</title>
					</circle>
				</OwnedOptimizationNodeFeature>
			)}
			{relaxedMndcInitializing && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="relaxed_mndc_overlay"
					sourceRole="trace_event.changed-entity.split-node-copies"
				>
					<g className="flow-mndc-split-node-glyph">
						<path d="M -34 -13 H -40 V 13 H -34 M 34 -13 H 40 V 13 H 34" />
						<text x="-46" textAnchor="middle" dominantBaseline="central">
							L
						</text>
						<text x="46" textAnchor="middle" dominantBaseline="central">
							R
						</text>
					</g>
				</OwnedOptimizationNodeFeature>
			)}
			{parametricForestOrientation !== undefined && sourceEventFocus && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="parametric_overlay"
					sourceRole="traversal.initialize-forest.singleton-roots"
				>
					<path
						className={`flow-parametric-singleton-root flow-parametric-singleton-root-${parametricForestOrientation}`}
						d="M -6 -34 L 0 -40 L 6 -34 M 0 -40 V -46"
					/>
					<circle
						className={`flow-parametric-singleton-root-dot flow-parametric-singleton-root-${parametricForestOrientation}`}
						cy="-48"
						r="2.75"
					/>
				</OwnedOptimizationNodeFeature>
			)}
			{parametricChildPolicy !== undefined && sourceEventFocus && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="parametric_overlay"
					sourceRole={`traversal.${parametricChildPolicy}.active-node`}
				>
					{parametricChildPolicy === "restart-smaller-child" ? (
						<path
							className="flow-parametric-child-policy flow-parametric-child-restart"
							d="M -7 -39 A 8 8 0 1 1 5 -34 M -8 -39 L -2 -40 L -5 -34"
						/>
					) : (
						<path
							className="flow-parametric-child-policy flow-parametric-child-continue"
							d="M -9 -39 H 6 M 1 -44 L 7 -39 L 1 -34"
						/>
					)}
				</OwnedOptimizationNodeFeature>
			)}
			{predictionNode !== undefined && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="prediction_assisted_epsilon_overlay"
					sourceRole="nodes.predicted-and-current-price"
				>
					<circle className="flow-prediction-ring" r="38" />
					{predictionNode.prediction_clipped && (
						<circle className="flow-prediction-clipped-ring" r="43" />
					)}
					{predictionNode.active && (
						<circle className="flow-prediction-active-ring" r="48" />
					)}
				</OwnedOptimizationNodeFeature>
			)}
			{binaryComponentBand !== undefined && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="binary_blocking_overlay"
					sourceRole="nodes.binary-distance-component"
				>
					<circle
						className={`flow-binary-component-ring flow-binary-component-ring-${binaryComponentBand}`}
						r="38"
					/>
				</OwnedOptimizationNodeFeature>
			)}
			{relaxedMndcFamilyBand !== undefined && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="relaxed_mndc_overlay"
					sourceRole="family.cycle-membership"
				>
					<circle
						className={`flow-mndc-family-ring flow-mndc-family-ring-${relaxedMndcFamilyBand % 4}`}
						r="39"
					/>
				</OwnedOptimizationNodeFeature>
			)}
			{enhancedRole !== undefined && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="enhanced_capacity_scaling_overlay"
					sourceRole={`components.${enhancedRole}`}
				>
					<circle
						className={`flow-enhanced-component-ring flow-enhanced-component-ring-${enhancedRole}`}
						r="40"
					/>
				</OwnedOptimizationNodeFeature>
			)}
			{enhancedPhaseGateRole !== undefined &&
				enhancedPhaseGateStage !== undefined && (
					<OwnedOptimizationNodeFeature
						state={state}
						nodeId={nodeId}
						overlay="enhanced_capacity_scaling_overlay"
						sourceRole={`components.${enhancedPhaseGateRole}-three-quarter-delta-${enhancedPhaseGateStage}`}
					>
						<g
							className={`flow-enhanced-phase-gate flow-enhanced-phase-gate-${enhancedPhaseGateRole} flow-enhanced-phase-gate-${enhancedPhaseGateStage}`}
							data-enhanced-phase-gate={enhancedPhaseGateRole}
							data-enhanced-phase-gate-stage={enhancedPhaseGateStage}
							data-enhanced-phase-gate-delta={`${enhancedView?.delta.numerator}/${enhancedView?.delta.denominator}`}
							transform="translate(0 -50)"
						>
							<title>{`${enhancedPhaseGateStage === "active" ? "Active" : "Next"} quotient gate: component excess ${enhancedPhaseGateRole === "excess" ? "≥ 3Δ/4" : "≤ −3Δ/4"}`}</title>
							<rect x="-28" y="-8" width="56" height="16" rx="8" />
							<text textAnchor="middle" dominantBaseline="central">
								{enhancedPhaseGateStage === "next" ? "NEXT " : ""}
								{enhancedPhaseGateRole === "excess" ? "+≥¾Δ" : "−≥¾Δ"}
							</text>
						</g>
					</OwnedOptimizationNodeFeature>
				)}
			{orlinRole !== undefined && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="orlin_mcf_overlay"
					sourceRole={`components.${orlinRole}`}
				>
					<circle
						className={`flow-orlin-component-ring flow-orlin-component-ring-${orlinRole}`}
						r="44"
					/>
				</OwnedOptimizationNodeFeature>
			)}
			{orlinPhaseGateRole !== undefined && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="orlin_mcf_overlay"
					sourceRole={`components.${orlinPhaseGateRole}-three-quarter-delta-gate`}
				>
					<g
						className={`flow-orlin-phase-gate flow-orlin-phase-gate-${orlinPhaseGateRole}`}
						data-orlin-phase-gate={orlinPhaseGateRole}
						data-orlin-phase-gate-delta={`${views.orlinMcf?.delta.numerator}/${views.orlinMcf?.delta.denominator}`}
						transform="translate(0 -50)"
					>
						<title>{`Active Orlin quotient component: excess ${orlinPhaseGateRole === "excess" ? "≥ 3Δ/4" : "≤ −3Δ/4"}`}</title>
						<rect x="-28" y="-8" width="56" height="16" rx="8" />
						<text textAnchor="middle" dominantBaseline="central">
							{orlinPhaseGateRole === "excess" ? "+≥¾Δ" : "−≥¾Δ"}
						</text>
					</g>
				</OwnedOptimizationNodeFeature>
			)}
			{orlinBelowGateWitnessNode && orlinBelowGateWitness !== undefined && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="orlin_mcf_overlay"
					sourceRole="components.maximum-below-three-quarter-delta-witness"
				>
					<g
						className="flow-orlin-phase-gate flow-orlin-phase-gate-below"
						data-orlin-phase-gate="below"
						data-orlin-phase-gate-excess={`${orlinBelowGateWitness.excess.numerator}/${orlinBelowGateWitness.excess.denominator}`}
						data-orlin-phase-gate-delta={`${views.orlinMcf?.delta.numerator}/${views.orlinMcf?.delta.denominator}`}
						transform="translate(0 -50)"
					>
						<title>{`No active Orlin quotient component: maximum |excess| is ${formatFlowRational(orlinBelowGateWitness.excess)}, below 3Δ/4`}</title>
						<rect x="-31" y="-8" width="62" height="16" rx="8" />
						<text textAnchor="middle" dominantBaseline="central">
							MAX &lt;¾Δ
						</text>
					</g>
				</OwnedOptimizationNodeFeature>
			)}
			{orlinDualExpansion && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="orlin_mcf_overlay"
					sourceRole="nodes.expand-component-dual"
				>
					<g
						className="flow-orlin-dual-expansion"
						data-orlin-dual-expansion={orlinNode.component_id}
						transform="translate(31 -31)"
					>
						<title>{`${nodeId}: expand quotient component ${orlinNode.component_id} dual π ${orlinNode.potential} to this transformed node`}</title>
						<circle r="9" />
						<text textAnchor="middle" dominantBaseline="central">
							π↓
						</text>
					</g>
				</OwnedOptimizationNodeFeature>
			)}
			{dualNode?.in_cut && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="dual_network_simplex_overlay"
					sourceRole="nodes.pivot-cut"
				>
					<circle className="flow-dual-cut-ring" r="40" />
				</OwnedOptimizationNodeFeature>
			)}
			{polynomialDualNode?.in_pivot_cut && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="polynomial_dual_simplex_overlay"
					sourceRole="nodes.pivot-cut"
				>
					<circle className="flow-polynomial-dual-cut-ring" r="47" />
				</OwnedOptimizationNodeFeature>
			)}
			{polynomialDualNode !== undefined &&
				polynomialDualScaleGate !== undefined && (
					<OwnedOptimizationNodeFeature
						state={state}
						nodeId={nodeId}
						overlay="polynomial_dual_simplex_overlay"
						sourceRole={`nodes.delta-gate.${polynomialDualScaleGate}`}
					>
						{polynomialDualScaleGate === "active" ? (
							<path
								className="flow-polynomial-dual-scale-gate flow-polynomial-dual-scale-gate-active"
								data-polynomial-dual-scale-gate="active"
								d="M 22 -34 L 29 -46 L 36 -34 Z"
							>
								<title>{`${nodeId}: excess ${formatFlowRational(polynomialDualNode.excess)} > Δ ${formatFlowRational(views.polynomialDualSimplex?.delta ?? { numerator: "0", denominator: "1" })}`}</title>
							</path>
						) : (
							<circle
								className="flow-polynomial-dual-scale-gate flow-polynomial-dual-scale-gate-below"
								data-polynomial-dual-scale-gate="below"
								cx="29"
								cy="-39"
								r="4"
							>
								<title>{`${nodeId}: excess ${formatFlowRational(polynomialDualNode.excess)} ≤ Δ ${formatFlowRational(views.polynomialDualSimplex?.delta ?? { numerator: "0", denominator: "1" })}`}</title>
							</circle>
						)}
					</OwnedOptimizationNodeFeature>
				)}
			{polynomialDualNode?.bad && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="polynomial_dual_simplex_overlay"
					sourceRole="nodes.bad-arc-endpoint"
				>
					<circle className="flow-polynomial-dual-bad-ring" r="42" />
				</OwnedOptimizationNodeFeature>
			)}
			{polynomialDualNode?.active && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="polynomial_dual_simplex_overlay"
					sourceRole="nodes.active"
				>
					<circle className="flow-polynomial-dual-active-ring" r="37" />
				</OwnedOptimizationNodeFeature>
			)}
			{polynomialDualNode?.root && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="polynomial_dual_simplex_overlay"
					sourceRole="nodes.root"
				>
					<rect
						className="flow-polynomial-dual-root-ring"
						x="-36"
						y="-36"
						width="72"
						height="72"
						rx="12"
					/>
				</OwnedOptimizationNodeFeature>
			)}
			{eligible && sourceEventFocus && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="polynomial_primal_simplex_overlay"
					sourceRole="nodes.eligible"
				>
					<circle className="flow-polynomial-eligible-ring" r="36" />
				</OwnedOptimizationNodeFeature>
			)}
			{awake && sourceEventFocus && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="polynomial_primal_simplex_overlay"
					sourceRole="nodes.awake"
				>
					<circle className="flow-polynomial-awake-ring" r="41" />
				</OwnedOptimizationNodeFeature>
			)}
			{nStar && sourceEventFocus && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="polynomial_primal_simplex_overlay"
					sourceRole="nodes.n-star"
				>
					<circle className="flow-polynomial-n-star-ring" r="46" />
				</OwnedOptimizationNodeFeature>
			)}
			{root && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="polynomial_primal_simplex_overlay"
					sourceRole="nodes.root"
				>
					<rect
						className="flow-polynomial-root-ring"
						x="-37"
						y="-37"
						width="74"
						height="74"
						rx="13"
					/>
				</OwnedOptimizationNodeFeature>
			)}
			{nodeScan !== undefined && (
				<OwnedOptimizationNodeFeature
					state={state}
					nodeId={nodeId}
					overlay="polynomial_primal_simplex_overlay"
					sourceRole="nodes.primitive-scan"
				>
					<g
						className="flow-polynomial-scan-badge flow-polynomial-node-scan-badge"
						data-polynomial-primal-scan={nodeScan.ordinal}
						transform="translate(0 -53)"
					>
						<rect x="-48" y="-7" width="96" height="14" rx="7" />
						<text textAnchor="middle" dominantBaseline="central">
							{nodeScan.caption}
						</text>
					</g>
				</OwnedOptimizationNodeFeature>
			)}
		</>
	);
}
