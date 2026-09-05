import { FlowDeterministicAlmostLinearMcfPanel } from "./FlowDeterministicAlmostLinearMcfPanel";
import { FlowElectricalIpmMcfPanel } from "./FlowElectricalIpmMcfPanel";
import { FlowMinimumRatioCycleMcfPanel } from "./FlowMinimumRatioCycleMcfPanel";
import { FlowOverlayContributionStatus } from "./FlowOverlayContributionStatus";
import { FlowPrimalDualIpmMcfPanel } from "./FlowPrimalDualIpmMcfPanel";
import { FlowRandomizedAlmostLinearMcfPanel } from "./FlowRandomizedAlmostLinearMcfPanel";
import { FlowWeightedAugmentingPathsPanel } from "./FlowWeightedAugmentingPathsPanel";
import { FlowWeightedPushRelabelShortcutPanel } from "./FlowWeightedPushRelabelShortcutPanel";
import {
	buildActiveFlowOverlayFeatureBundles,
	type FlowOverlayViews,
} from "./flow-overlay-contribution-registry";
import type { FlowOverlayPresentation } from "./flow-overlay-presentation";
import { formatFlowRational } from "./flow-parametric-view";
import type { FlowCurrentSceneV9 } from "./flow-scene";

type Props = Readonly<{
	scene: FlowCurrentSceneV9 | undefined;
	views: FlowOverlayViews | undefined;
	presentation: FlowOverlayPresentation | undefined;
}>;

/** Algorithm-specific status and panel bundle kept out of the workspace shell. */
export function FlowOverlayRichStatusRegion({
	scene,
	views,
	presentation,
}: Props) {
	if (
		presentation === undefined ||
		!buildActiveFlowOverlayFeatureBundles(presentation.activeFields).has(
			"rich-status",
		)
	) {
		return null;
	}
	return (
		<details className="flow-rich-status-disclosure">
			<summary aria-label="Show algorithm state details">
				<span aria-hidden="true">i</span>
				Algorithm state
			</summary>
			<div className="flow-rich-status-body">
				{views?.orlinMaxFlow !== undefined && (
					<div
						className="flow-orlin-max-phase-strip"
						role="status"
						aria-label="Orlin maximum-flow improvement phase"
					>
						<strong>{`Orlin phase ${scene?.metrics[0] ?? "—"} · ${views.orlinMaxFlow.stage}`}</strong>
						<span>{`Δ ${views.orlinMaxFlow.delta}`}</span>
						<span>{`Γ ${formatFlowRational(views.orlinMaxFlow.gamma)}`}</span>
						<span>{`critical ${new Set(views.orlinMaxFlow.nodes.filter((node) => node.critical).map((node) => node.component_id)).size} / components ${new Set(views.orlinMaxFlow.nodes.map((node) => node.component_id)).size}`}</span>
						<span className="flow-orlin-max-case">
							{views.orlinMaxFlow.phase_case ?? "case pending"}
						</span>
						<small>c¹⁶ &gt; m⁹ / c³ ≥ m / c³ &lt; m</small>
					</div>
				)}
				{views?.electricalFlow !== undefined && (
					<div
						className="flow-electrical-phase-strip"
						role="status"
						aria-label="Electrical-flow numerical phase"
						data-electrical-stage={views.electricalFlow.stage}
					>
						<strong>{`Electrical · ${views.electricalFlow.stage}`}</strong>
						<span>{`CG ${views.electricalFlow.iteration}`}</span>
						<span>{`‖r‖₂ ${views.electricalFlow.residual_l2}`}</span>
						<span>{`E ${views.electricalFlow.total_energy}`}</span>
						<span>{`Rₑff ${views.electricalFlow.effective_resistance}`}</span>
						<span className="flow-electrical-convergence">
							{views.electricalFlow.converged
								? `✓ tolerance ${views.electricalFlow.relative_tolerance}`
								: `solving · tolerance ${views.electricalFlow.relative_tolerance}`}
						</span>
						<small>
							{views.electricalFlow.exact_effective_resistance === undefined
								? "exact rational oracle pending"
								: `exact ${formatFlowRational(views.electricalFlow.exact_effective_resistance)} · error ${views.electricalFlow.maximum_absolute_error}`}
						</small>
					</div>
				)}
				{views?.augmentingElectrical !== undefined && (
					<div
						className="flow-augmenting-electrical-phase-strip"
						role="status"
						aria-label="Augmenting electrical maximum-flow phase"
						data-augmenting-electrical-stage={views.augmentingElectrical.stage}
					>
						<strong>{`Augmenting electrical · ${views.augmentingElectrical.stage}`}</strong>
						<span>{`α ${views.augmentingElectrical.alpha}`}</span>
						<span>{`remaining ${views.augmentingElectrical.remaining}`}</span>
						<span>{`‖ρ‖₃ ${views.augmentingElectrical.congestion_l3}`}</span>
						<span>{`‖ρ‖₄ ${views.augmentingElectrical.congestion_l4}`}</span>
						<span className="flow-augmenting-coupling">{`‖γ‖₂ ${views.augmentingElectrical.coupling_l2}`}</span>
						<span>{`work ${views.augmentingElectrical.working_nodes}v/${views.augmentingElectrical.working_edges}e`}</span>
						{views.augmentingElectrical.active_pivot_node !== undefined && (
							<span>{`pivot w${views.augmentingElectrical.active_pivot_node}/${views.augmentingElectrical.working_nodes}`}</span>
						)}
						{views.augmentingElectrical.active_working_path.length > 0 && (
							<span>{`push ${views.augmentingElectrical.active_discrete_amount} on ${views.augmentingElectrical.active_working_path.map((arc) => `${arc.from_node}→${arc.to_node} (w${arc.edge}, x=${arc.flow_after})`).join(" · ")}`}</span>
						)}
						{views.augmentingElectrical.active_extraction_cycle.length > 0 && (
							<span>{`cancel ${views.augmentingElectrical.active_discrete_amount} on ${views.augmentingElectrical.active_extraction_cycle.map((arc) => `${arc.kind}(e${arc.edge})`).join("→")}`}</span>
						)}
						<small>{`target original/reduced/preconditioned ${views.augmentingElectrical.original_target}/${views.augmentingElectrical.transformed_target}/${views.augmentingElectrical.working_target}`}</small>
					</div>
				)}
				{views?.interiorPointMaxFlow !== undefined && (
					<div
						className="flow-interior-point-phase-strip"
						role="status"
						aria-label="Interior-point maximum-flow phase"
						data-testid="flow-interior-point-status"
						data-interior-point-stage={views.interiorPointMaxFlow.stage}
					>
						<strong>{`IPM · ${views.interiorPointMaxFlow.stage}`}</strong>
						<span>{`μ ${views.interiorPointMaxFlow.mu}`}</span>
						<span>{`gap ${views.interiorPointMaxFlow.duality_gap}`}</span>
						<span>{`‖γ‖₂ ${views.interiorPointMaxFlow.centrality}`}</span>
						<span>{`δ ${views.interiorPointMaxFlow.step_size}`}</span>
						<span>{`‖ρ‖₄ ${views.interiorPointMaxFlow.congestion_l4}`}</span>
						<span>{`E ${views.interiorPointMaxFlow.electrical_energy}`}</span>
						<span>{`Ḡ ${views.interiorPointMaxFlow.b_matching_nodes}v/${views.interiorPointMaxFlow.b_matching_edges}e`}</span>
						<span>{`Gᵦ ${views.interiorPointMaxFlow.working_nodes}v/${views.interiorPointMaxFlow.working_edges}a`}</span>
						<small>{`target ${views.interiorPointMaxFlow.target_value} · unit-capacity §2–§5 kernel + matching recovery`}</small>
					</div>
				)}
				{views?.minimumRatioCycle !== undefined && (
					<div
						className="flow-minimum-ratio-phase-strip"
						role="status"
						aria-label="Minimum-ratio cycle primitive phase"
						data-testid="flow-minimum-ratio-status"
						data-minimum-ratio-stage={views.minimumRatioCycle.stage}
					>
						<strong>{`Min-ratio primitive · ${views.minimumRatioCycle.stage}`}</strong>
						<span>{`candidate ${views.minimumRatioCycle.candidate_ratio === undefined ? "—" : formatFlowRational(views.minimumRatioCycle.candidate_ratio)}`}</span>
						<span>{`best ${views.minimumRatioCycle.best_ratio === undefined ? "—" : formatFlowRational(views.minimumRatioCycle.best_ratio)}`}</span>
						<span>{`cycle ${views.minimumRatioCycle.selected_edge_count}e`}</span>
						<span>{`balance∞ ${views.minimumRatioCycle.maximum_absolute_balance}`}</span>
						<span>{`simple ${views.minimumRatioCycle.simple_cycles}`}</span>
						<span>{`basis ${views.minimumRatioCycle.fundamental_cycles}`}</span>
						<span>{`vectors ${views.minimumRatioCycle.enumerated_vectors}`}</span>
						<small>
							exact bounded oracle · n≤8 / m≤11 · maximum-flow subroutine
						</small>
					</div>
				)}
				{scene !== undefined && views?.minimumRatioCycleMcf !== undefined && (
					<FlowMinimumRatioCycleMcfPanel
						graph={scene.graph}
						overlay={views.minimumRatioCycleMcf}
					/>
				)}
				{scene !== undefined &&
					views?.randomizedAlmostLinearMcf !== undefined && (
						<FlowRandomizedAlmostLinearMcfPanel
							graph={scene.graph}
							overlay={views.randomizedAlmostLinearMcf}
						/>
					)}
				{scene !== undefined && views?.flowFrameworkMcf !== undefined && (
					<FlowDeterministicAlmostLinearMcfPanel
						graph={scene.graph}
						overlay={views.flowFrameworkMcf}
					/>
				)}
				{views?.weightedAugmentingPaths !== undefined && (
					<FlowWeightedAugmentingPathsPanel
						overlay={views.weightedAugmentingPaths}
					/>
				)}
				{views?.weightedPushRelabelShortcut !== undefined && (
					<FlowWeightedPushRelabelShortcutPanel
						overlay={views.weightedPushRelabelShortcut}
					/>
				)}
				{views?.randomizedAlmostLinear !== undefined && (
					<div
						className="flow-randomized-almost-linear-phase-strip"
						role="status"
						aria-label="Bounded randomized tree-chain maximum-flow phase"
						data-testid="flow-randomized-almost-linear-status"
						data-randomized-almost-linear-stage={
							views.randomizedAlmostLinear.stage
						}
					>
						<strong>{`Randomized tree-chain · ${views.randomizedAlmostLinear.stage}`}</strong>
						<span>{`seed ${views.randomizedAlmostLinear.seed}`}</span>
						<span>{`forest ${views.randomizedAlmostLinear.forest_pool_size} · sample ${views.randomizedAlmostLinear.sample_count} · draws ${views.randomizedAlmostLinear.random_draws}`}</span>
						<span>{`Pr[miss] ${views.randomizedAlmostLinear.miss_probability.numerator}/${views.randomizedAlmostLinear.miss_probability.denominator}`}</span>
						<span>{`IPM ${views.randomizedAlmostLinear.iteration}/8 · rebuild ${views.randomizedAlmostLinear.rebuild_epoch}`}</span>
						<span>{`Φ ${views.randomizedAlmostLinear.potential} · gap ${views.randomizedAlmostLinear.cost_gap} · α ${views.randomizedAlmostLinear.alpha}`}</span>
						<span>{`ratio ${views.randomizedAlmostLinear.selected_ratio ?? "—"} / pool ${views.randomizedAlmostLinear.exact_pool_ratio ?? "—"}`}</span>
						<span>{`isolation ${views.randomizedAlmostLinear.isolation_attempt} · Pr[failure] ${views.randomizedAlmostLinear.isolation_failure_probability.numerator}/${views.randomizedAlmostLinear.isolation_failure_probability.denominator} · D ${views.randomizedAlmostLinear.isolation_scale}`}</span>
						<span>{`final point gap ${views.randomizedAlmostLinear.final_point_gap ?? "—"} ≤ ${views.randomizedAlmostLinear.final_point_threshold} · mix ${views.randomizedAlmostLinear.final_point_mix ?? "—"}`}</span>
						<span>{`return ${views.randomizedAlmostLinear.final_return_flow ?? views.randomizedAlmostLinear.return_flow}/${views.randomizedAlmostLinear.return_capacity} · artificial ${views.randomizedAlmostLinear.final_artificial_flow ?? views.randomizedAlmostLinear.artificial_flow}`}</span>
						<small>
							bounded n≤8 / m≤10 / assignments≤100,000 · finite forest
							population · isolation + source final-point rounding · seeded
							replay · no m^(1+o(1)) runtime claim
						</small>
					</div>
				)}
				{views?.deterministicAlmostLinear !== undefined && (
					<div
						className="flow-deterministic-almost-linear-phase-strip"
						role="status"
						aria-label="Bounded deterministic shifted tree-chain maximum-flow phase"
						data-testid="flow-deterministic-almost-linear-status"
						data-deterministic-almost-linear-stage={
							views.deterministicAlmostLinear.stage
						}
						data-deterministic-active-level={
							views.deterministicAlmostLinear.active_level
						}
						data-deterministic-cycle-kind={
							views.deterministicAlmostLinear.selected_cycle_kind
						}
					>
						<strong>{`Deterministic shifted tree-chain · ${views.deterministicAlmostLinear.stage}`}</strong>
						<span>{`levels ${views.deterministicAlmostLinear.level_count} · branches [${views.deterministicAlmostLinear.active_branches.join("/")}] · passes [${views.deterministicAlmostLinear.passes.join("/")}]${views.deterministicAlmostLinear.active_level === undefined ? "" : ` · active L${views.deterministicAlmostLinear.active_level}`}`}</span>
						<span>{`forest pool ${views.deterministicAlmostLinear.forest_pool_size} · ${views.deterministicAlmostLinear.branch_count} branches/level · ${views.deterministicAlmostLinear.built_branch_records} records built`}</span>
						<span>{`core ${views.deterministicAlmostLinear.core_vertices}v/${views.deterministicAlmostLinear.core_edges}e · spanner ${views.deterministicAlmostLinear.spanner_edges}e · embedding ${views.deterministicAlmostLinear.embedding_hops} hops`}</span>
						<span>{`IPM ${views.deterministicAlmostLinear.iteration}/6 · rebuild ${views.deterministicAlmostLinear.rebuild_epoch}`}</span>
						<span>{`Φ ${views.deterministicAlmostLinear.potential} · gap ${views.deterministicAlmostLinear.cost_gap} · α ${views.deterministicAlmostLinear.alpha}`}</span>
						<span>{`ratio ${views.deterministicAlmostLinear.selected_ratio ?? "—"} / pool ${views.deterministicAlmostLinear.exact_pool_ratio ?? "—"} · ${views.deterministicAlmostLinear.selected_cycle_kind ?? "—"}`}</span>
						<span>{`final point gap ${views.deterministicAlmostLinear.final_point_gap === undefined ? "—" : formatFlowRational(views.deterministicAlmostLinear.final_point_gap)} < ${formatFlowRational(views.deterministicAlmostLinear.final_point_threshold)} · mix ${views.deterministicAlmostLinear.final_point_mix === undefined ? "—" : formatFlowRational(views.deterministicAlmostLinear.final_point_mix)}`}</span>
						<span>{`rounding edge ${views.deterministicAlmostLinear.rounding_processed_edge ?? "—"} · forest ${views.deterministicAlmostLinear.edges.filter((edge) => edge.rounding_forest_edge).length + Number(views.deterministicAlmostLinear.rounding_return_forest_edge)} · cycle ${views.deterministicAlmostLinear.edges.filter((edge) => edge.rounding_cycle_sign !== "0").length + Number(views.deterministicAlmostLinear.rounding_return_sign !== "0")}`}</span>
						<span>{`return ${views.deterministicAlmostLinear.final_return_flow ?? (views.deterministicAlmostLinear.rounding_return_flow === undefined ? views.deterministicAlmostLinear.return_flow : formatFlowRational(views.deterministicAlmostLinear.rounding_return_flow))}/${views.deterministicAlmostLinear.return_capacity} · artificial ${views.deterministicAlmostLinear.final_artificial_flow ?? views.deterministicAlmostLinear.artificial_flow}`}</span>
						<small>
							bounded n≤7 / m≤8 / assignments≤100,000 · exhaustive forest
							population · stable Shift/Rebuild · additive-half final point ·
							Kang–Payor deterministic rounding · no m^(1+o(1)) runtime claim
						</small>
					</div>
				)}
				{scene !== undefined && views?.primalDualIpmMcf !== undefined && (
					<FlowPrimalDualIpmMcfPanel
						graph={scene.graph}
						overlay={views.primalDualIpmMcf}
					/>
				)}
				{scene !== undefined && views?.electricalIpmMcf !== undefined && (
					<FlowElectricalIpmMcfPanel
						graph={scene.graph}
						overlay={views.electricalIpmMcf}
					/>
				)}
				<FlowOverlayContributionStatus presentation={presentation} />
			</div>
		</details>
	);
}
