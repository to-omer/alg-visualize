import {
	CONVEX_COST_SCALING_ALGORITHM,
	CONVEX_NETWORK_SIMPLEX_ALGORITHM,
} from "./flow-algorithm-presentation";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import { formatFlowRational } from "./flow-parametric-view";
import type { FlowCurrentSceneV9 } from "./flow-scene";
import { flowWorkbenchPolicy } from "./flow-workbench-policy";
import { flowModelWorkbenchProblem } from "./flow-workbench-problem";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;
type SvgDataValue = string | number | boolean | undefined;

function flowModelHasMinimumCut(model: FlowCurrentSceneV9["model"]): boolean {
	switch (model.kind) {
		case "max-flow":
		case "parametric-max-flow":
		case "planar-max-flow":
		case "min-cost-max-flow":
			return true;
		case "bipartite-matching":
		case "fixed-flow-min-cost":
		case "circulation":
		case "transshipment":
		case "assignment":
		case "transportation":
		case "convex-cost-flow":
			return false;
	}
}

/** Base graph encoding copy derived from the canonical workbench model policy. */
export function flowGraphModelAccessibleDescription(
	model: FlowCurrentSceneV9["model"],
): string {
	const problem = flowModelWorkbenchProblem(model);
	const descriptions = [
		"Outer edge width shows capacity; inner width shows current flow; arrow markers show edge direction.",
		"Leader lines connect visible annotations to their exact edge; parallel edges use separated curved lanes with one-based arrow tokens, and selecting a token opens its matching lane badge.",
	];
	if (flowWorkbenchPolicy(problem).showsCost) {
		descriptions.push(
			"Color and dash pattern show signed unit cost, continuous intensity shows absolute cost magnitude, and labels show flow, capacity, and unit cost.",
		);
	}
	if (flowModelHasMinimumCut(model)) {
		descriptions.push("The minimum cut is highlighted after optimization.");
	}
	return descriptions.join(" ");
}

export function flowGraphSceneClassName(state: FlowEntityGraphState): string {
	return state.visualization.features.transportation
		? "flow-graph flow-graph-transportation"
		: "flow-graph";
}

/**
 * Stable diagnostic attributes for browser tests and developer inspection.
 * Raw rich-overlay view access is confined to this projection boundary.
 */
export function flowGraphSceneDataAttributes(
	state: FlowEntityGraphState,
): Readonly<Record<`data-${string}`, SvgDataValue>> {
	const { plan, context, visualization } = state;
	const views = state.renderData.overlayViews;
	const ibfs = visualization.ibfsView;
	const eibfs = visualization.eibfsView;
	const features = visualization.features;
	return {
		"data-flow-lod": plan.level,
		"data-rendered-nodes": plan.nodes.length,
		"data-rendered-original-edges": plan.edges.length,
		"data-rendered-residual-arcs": state.visibleResidualArcs.length,
		"data-active-overlays": plan.overlayPresentation.activeFields.join("|"),
		"data-planar-dual-faces": state.planarDual?.faces.length ?? 0,
		"data-planar-dual-edges": state.planarDual?.edges.length ?? 0,
		"data-ibfs-source-depth": ibfs?.sourceDepth,
		"data-ibfs-sink-depth": ibfs?.sinkDepth,
		"data-ibfs-shortest-path-length": ibfs?.shortestPathLength,
		"data-eibfs-phase": eibfs?.phaseDirection,
		"data-eibfs-stage": visualization.eibfsStage,
		"data-eibfs-source-depth": eibfs?.sourceDepth,
		"data-eibfs-sink-depth": eibfs?.sinkDepth,
		"data-binary-blocking-stage": views.binaryBlocking?.stage,
		"data-binary-blocking-delta": views.binaryBlocking?.delta,
		"data-cancel-tighten-stage": views.cancelTighten?.stage,
		"data-cancel-tighten-epsilon":
			views.cancelTighten === undefined
				? undefined
				: formatFlowRational(views.cancelTighten.epsilon),
		"data-relaxed-mndc-stage": views.relaxedMndc?.stage,
		"data-relaxed-mndc-epsilon":
			views.relaxedMndc === undefined
				? undefined
				: formatFlowRational(views.relaxedMndc.epsilon),
		"data-relaxed-mndc-family": views.relaxedMndc?.family.length,
		"data-enhanced-scaling-stage": views.enhancedCapacityScaling?.stage,
		"data-enhanced-scaling-delta":
			views.enhancedCapacityScaling === undefined
				? undefined
				: formatFlowRational(views.enhancedCapacityScaling.delta),
		"data-enhanced-scaling-components":
			views.enhancedCapacityScaling?.components.length,
		"data-orlin-mcf-stage": views.orlinMcf?.stage,
		"data-orlin-mcf-delta":
			views.orlinMcf === undefined
				? undefined
				: formatFlowRational(views.orlinMcf.delta),
		"data-orlin-mcf-capacity-nodes": views.orlinMcf?.nodes.filter(
			(node) => node.kind === "capacity",
		).length,
		"data-orlin-mcf-shortcuts": views.orlinMcf?.shortcut_arcs,
		"data-orlin-max-stage": views.orlinMaxFlow?.stage,
		"data-orlin-max-delta": views.orlinMaxFlow?.delta,
		"data-orlin-max-gamma":
			views.orlinMaxFlow === undefined
				? undefined
				: formatFlowRational(views.orlinMaxFlow.gamma),
		"data-orlin-max-case": views.orlinMaxFlow?.phase_case,
		"data-orlin-max-components": state.orlinMaxComponentBoxes.length,
		"data-orlin-max-compact-arcs": state.orlinMaxCompactVisuals.length,
		"data-electrical-stage": views.electricalFlow?.stage,
		"data-electrical-iteration": views.electricalFlow?.iteration,
		"data-electrical-residual": views.electricalFlow?.residual_l2,
		"data-electrical-energy": views.electricalFlow?.total_energy,
		"data-electrical-effective-resistance":
			views.electricalFlow?.effective_resistance,
		"data-electrical-converged": views.electricalFlow?.converged || undefined,
		"data-augmenting-electrical-stage": views.augmentingElectrical?.stage,
		"data-augmenting-electrical-alpha": views.augmentingElectrical?.alpha,
		"data-augmenting-electrical-remaining":
			views.augmentingElectrical?.remaining,
		"data-augmenting-electrical-l3": views.augmentingElectrical?.congestion_l3,
		"data-augmenting-electrical-l4": views.augmentingElectrical?.congestion_l4,
		"data-augmenting-electrical-coupling":
			views.augmentingElectrical?.coupling_l2,
		"data-interior-point-stage": views.interiorPointMaxFlow?.stage,
		"data-interior-point-mu": views.interiorPointMaxFlow?.mu,
		"data-interior-point-gap": views.interiorPointMaxFlow?.duality_gap,
		"data-interior-point-centrality": views.interiorPointMaxFlow?.centrality,
		"data-interior-point-target": views.interiorPointMaxFlow?.target_value,
		"data-minimum-ratio-stage": views.minimumRatioCycle?.stage,
		"data-minimum-ratio-candidate":
			views.minimumRatioCycle?.candidate_ratio === undefined
				? undefined
				: formatFlowRational(views.minimumRatioCycle.candidate_ratio),
		"data-minimum-ratio-best":
			views.minimumRatioCycle?.best_ratio === undefined
				? undefined
				: formatFlowRational(views.minimumRatioCycle.best_ratio),
		"data-minimum-ratio-simple-cycles": views.minimumRatioCycle?.simple_cycles,
		"data-minimum-ratio-fundamental-cycles":
			views.minimumRatioCycle?.fundamental_cycles,
		"data-randomized-almost-linear-stage": views.randomizedAlmostLinear?.stage,
		"data-randomized-almost-linear-seed": views.randomizedAlmostLinear?.seed,
		"data-randomized-almost-linear-forest-pool":
			views.randomizedAlmostLinear?.forest_pool_size,
		"data-randomized-almost-linear-samples":
			views.randomizedAlmostLinear?.sample_count,
		"data-randomized-almost-linear-draws":
			views.randomizedAlmostLinear?.random_draws,
		"data-randomized-almost-linear-miss-numerator":
			views.randomizedAlmostLinear?.miss_probability.numerator,
		"data-randomized-almost-linear-miss-denominator":
			views.randomizedAlmostLinear?.miss_probability.denominator,
		"data-randomized-almost-linear-iteration":
			views.randomizedAlmostLinear?.iteration,
		"data-randomized-almost-linear-rebuild":
			views.randomizedAlmostLinear?.rebuild_epoch,
		"data-randomized-almost-linear-isolation-attempt":
			views.randomizedAlmostLinear?.isolation_attempt,
		"data-randomized-almost-linear-final-point-gap":
			views.randomizedAlmostLinear?.final_point_gap,
		"data-randomized-almost-linear-final-point-threshold":
			views.randomizedAlmostLinear?.final_point_threshold,
		"data-deterministic-almost-linear-stage":
			views.deterministicAlmostLinear?.stage,
		"data-deterministic-active-level":
			views.deterministicAlmostLinear?.active_level,
		"data-deterministic-active-branches":
			views.deterministicAlmostLinear?.active_branches.join("/"),
		"data-deterministic-passes":
			views.deterministicAlmostLinear?.passes.join("/"),
		"data-deterministic-core-edges":
			views.deterministicAlmostLinear?.core_edges,
		"data-deterministic-spanner-edges":
			views.deterministicAlmostLinear?.spanner_edges,
		"data-deterministic-final-point-gap":
			views.deterministicAlmostLinear?.final_point_gap === undefined
				? undefined
				: formatFlowRational(views.deterministicAlmostLinear.final_point_gap),
		"data-deterministic-final-point-threshold":
			views.deterministicAlmostLinear === undefined
				? undefined
				: formatFlowRational(
						views.deterministicAlmostLinear.final_point_threshold,
					),
		"data-deterministic-rounding-processed-edge":
			views.deterministicAlmostLinear?.rounding_processed_edge,
		"data-dual-simplex-stage": views.dualNetworkSimplex?.stage,
		"data-dual-simplex-tree-edges": views.dualNetworkSimplex?.edges.filter(
			(edge) => edge.in_tree,
		).length,
		"data-dual-simplex-cut-size": views.dualNetworkSimplex?.cut_side.length,
		"data-dual-simplex-price-delta":
			views.dualNetworkSimplex?.pivot_price_delta,
		"data-polynomial-dual-stage": views.polynomialDualSimplex?.stage,
		"data-polynomial-dual-phase": views.polynomialDualSimplex?.phase,
		"data-polynomial-dual-delta":
			views.polynomialDualSimplex === undefined
				? undefined
				: formatFlowRational(views.polynomialDualSimplex.delta),
		"data-polynomial-dual-tree-edges":
			views.polynomialDualSimplex?.edges.filter((edge) => edge.in_tree).length,
		"data-polynomial-dual-bad-edges":
			views.polynomialDualSimplex?.bad_edges.length,
		"data-polynomial-dual-path-length":
			views.polynomialDualSimplex?.augment_path.length,
		"data-polynomial-primal-stage": views.polynomialPrimalSimplex?.stage,
		"data-polynomial-primal-phase": views.polynomialPrimalSimplex?.phase,
		"data-polynomial-primal-epsilon":
			views.polynomialPrimalSimplex?.epsilon === undefined
				? undefined
				: formatFlowRational(views.polynomialPrimalSimplex.epsilon),
		"data-polynomial-primal-n-star":
			views.polynomialPrimalSimplex?.nodes.filter((node) =>
				node.flags.includes("in-n-star"),
			).length,
		"data-polynomial-primal-artificial-tree":
			views.polynomialPrimalSimplex?.artificial_edges.filter(
				(edge) => edge.basis === "tree",
			).length,
		"data-double-scaling-stage": views.doubleScaling?.stage,
		"data-double-scaling-epsilon": views.doubleScaling?.epsilon,
		"data-double-scaling-delta": views.doubleScaling?.delta,
		"data-convex-cost-stage": views.convexCost?.stage,
		"data-convex-active-cycle": views.convexCost?.active_cycle.length,
		"data-convex-cost-scale": views.convexCost?.scale,
		"data-convex-eligible-arcs": (views.convexCost?.eligible_arcs ?? []).length,
		"data-convex-simplex-stage": views.convexNetworkSimplex?.stage,
		"data-convex-simplex-tree-edges": views.convexNetworkSimplex?.edges.filter(
			(edge) => edge.basis === "tree",
		).length,
		"data-convex-simplex-crossings":
			context.algorithmId === CONVEX_NETWORK_SIMPLEX_ALGORITHM
				? context.metrics[3]
				: undefined,
		"data-convex-simplex-exchanges":
			context.algorithmId === CONVEX_NETWORK_SIMPLEX_ALGORITHM
				? context.metrics[4]
				: undefined,
		"data-convex-simplex-multi-pivots":
			context.algorithmId === CONVEX_NETWORK_SIMPLEX_ALGORITHM
				? context.metrics[6]
				: undefined,
		"data-prediction-epsilon-stage": views.predictionAssistedEpsilon?.stage,
		"data-prediction-epsilon-attempt": views.predictionAssistedEpsilon?.attempt,
		"data-prediction-epsilon-maximum-attempt":
			views.predictionAssistedEpsilon?.maximum_attempt,
		"data-prediction-epsilon-exponent":
			views.predictionAssistedEpsilon?.exponent,
		"data-prediction-epsilon-scale":
			views.predictionAssistedEpsilon?.scale_exponent,
		"data-prediction-epsilon-clipped":
			views.predictionAssistedEpsilon?.nodes.filter(
				(node) => node.prediction_clipped,
			).length,
		"data-tardos-stage": views.tardosFramework?.stage,
		"data-tardos-epsilon": views.tardosFramework?.epsilon,
		"data-tardos-threshold": views.tardosFramework?.threshold,
		"data-tardos-determinant-bound": views.tardosFramework?.determinant_bound,
		"data-tardos-fixed-variables":
			views.tardosFramework?.fixed_variables.length,
		"data-network-simplex-mode": features.networkSimplex
			? features.dynamicTreeNetworkSimplex
				? "dynamic-tree"
				: "explicit"
			: undefined,
	};
}

/** Rich, additive accessible copy for the projected graph scene. */
export function flowGraphAccessibleDescription(
	state: FlowEntityGraphState,
): string {
	const { context, visualization } = state;
	const views = state.renderData.overlayViews;
	const features = visualization.features;
	const descriptions = [
		...state.plan.overlayPresentation.accessibleDescriptions,
		flowGraphModelAccessibleDescription(context.model),
	];
	const add = (condition: boolean, copy: string): void => {
		if (condition) descriptions.push(copy);
	};
	add(
		context.model.kind === "parametric-max-flow",
		"Edge width and four teal levels show exact current capacity u(λ), using one fixed scale over the full parameter range. Hatched nodes are ties between the minimum and maximum source-side cuts at the same λ.",
	);
	add(
		state.hasBalances,
		"Signed node values show balance. Solid rings mark supply; dashed rings mark demand.",
	);
	add(
		views.feasibility?.domain.kind === "standalone-transformation",
		"This boundary replaces the public graph with the algorithm-owned feasibility network because its internal nodes have no public-node identity. SS and ST are artificial routing terminals, not input vertices.",
	);
	add(
		views.feasibility?.domain.kind === "node-aligned-transformation",
		"This boundary draws the transformed feasibility network over the same public vertices. Dim original edges provide orientation only; the brighter feasibility arcs are the current internal computation.",
	);
	add(state.gridgen, "GRIDGEN supernodes use dotted rings and a super label.");
	add(
		views.interiorPointMaxFlow !== undefined,
		"For interior-point max flow, the outer rail is unit capacity and the pale-green inner stroke is fractional central-path flow. Solid cyan or dashed violet shows signed electrical direction, gold shows dual slack, violet dots show resistance, and amber-to-red glow shows congestion. Inner node rings show potential; outer dashed rings show the enumerated exact target-cut side. Edges leaving the sink or entering the source appear as gray dotted terminal-normalized edges.",
	);
	add(
		views.minimumRatioCycle !== undefined,
		"Minimum-ratio cycle is an internal Chen–Kyng–Liu–Peng primitive, not a max-flow solver. Outer width shows positive length ℓ; the cool-to-warm inner stroke shows signed gradient g. Dashed blue is the deterministic spanning forest, orange is the candidate under evaluation, and violet is the current best cycle. Arrows show sign relative to stored edge direction. Node rings and annotations show component, depth, and candidate conservation error.",
	);
	add(
		views.randomizedAlmostLinear !== undefined,
		"In the bounded tree-chain implementation of randomized almost-linear max flow, outer width shows source length ℓ and blue or orange inner strokes show signed gradient g. Thin cyan dashes encode sampled-tree membership count, long green dashes mark the queried tree, violet arrows mark a fundamental cycle, and yellow glow marks coordinates changed by Detect. The curved t→s edge is the return edge with capacity mU and cost −1; stars and dotted edges are artificial edges used only for strict-interior initialization. After the iteration cap, the view steps through the bounded feasible-set oracle, isolation, the source final-point error gate, and nearest-integer rounding. It does not claim the paper's almost-linear running time.",
	);
	add(
		views.deterministicAlmostLinear !== undefined,
		"In the bounded shifted tree-chain implementation of deterministic almost-linear max flow, teal dash-dots mark each level's tree chain, long heavy green dashes mark the current partial forest, orange dashes mark the contracted core, solid gold marks the deterministic spanner, violet dots summarize the spanner embedding, and magenta arrows mark the cycle chosen by Query. During rounding, double cyan dashes mark the Kang–Payor fractional forest and pink arrows mark cost-nonincreasing fractional cycles. The curved t→s edge and stars expose the max-flow reduction and strict-interior initialization. This bounded visualization covers n≤7, m≤8, and assignment≤100,000 with an additive-error-below-1/2 rational final point and deterministic rounding; it does not claim the paper's dynamic-data-structure running time.",
	);
	add(
		state.frameGroups.length > 0,
		"Dashed background boxes and frame numbers identify RMFGEN frames.",
	);
	add(
		features.networkSimplex,
		features.dynamicTreeNetworkSimplex
			? "Dashed blue marks the current basis tree shared by two directional link-cut forests. Dotted rings mark basis components connected to the artificial root. Orange marks the fundamental cycle used by the cycle-minimum query and lazy path update. Cut/link events reveal the exchanged basis."
			: "Dashed blue marks the current basis tree, dotted rings mark basis components connected to the artificial root, and orange marks the selected fundamental cycle.",
	);
	add(
		features.transportation,
		"Dashed blue marks the current transportation basis forest. Solid orange +θ routes increase flow; dashed orange −θ routes decrease it. Node annotations u/v are row/column potentials.",
	);
	add(
		features.dynamicTreeBlocking,
		"Dashed blue marks represented-tree arcs directed toward the sink; dotted rings mark current represented roots. Orange residual arcs receive the lazy path update. A tree arc whose capacity reaches zero is removed by the following cut event.",
	);
	add(
		features.dynamicTreePushRelabel,
		"Dashed blue marks represented-tree arcs whose height drops by one; dotted rings mark represented roots currently in the FIFO queue. Orange residual arcs receive the lazy root-path send. Saturated edges or child edges preceding a relabel are removed by cut events.",
	);
	add(
		context.algorithmId === "current-arc-heuristic",
		"For current-arc optimization, one orange residual arc is the cursor stored at the selected node. During discharge it skips ineligible arcs and advances, returning to the first arc only after relabeling. At a push boundary, the same orange stroke marks the arc that received flow.",
	);
	add(
		features.goldbergRao,
		"Goldberg–Rao node annotations show 0–1 distance d(v) to the sink. Orange residual arcs mark a blocking-or-Δ update on the contracted DAG or a lift within a component. Event details show current Δ and cut capacity; the inspector shows cumulative counts of base-zero arcs, special arcs, and SCCs.",
	);
	add(
		features.binaryBlocking,
		"Binary blocking flow is one Goldberg–Rao subproblem, not a max-flow solver. Node annotations show 0–1 distance d(v) and zero-length SCC number. Long teal dashes are base-zero arcs, violet dots are special arcs, heavy blue strokes are zero-length admissible arcs, and orange marks the current admissible set or atomic-lift target.",
	);
	add(
		features.distanceDirected,
		"Distance-Directed DD2 node annotations show exact distance d(v) to the sink in the current threshold residual graph. Dashed blue marks the shortest-path in-tree toward the sink. Orange marks the unique current tree path or an exchanged parent edge. Event details show the threshold, bottleneck, and post-relabel distance.",
	);
	if (visualization.ibfsView !== undefined) {
		descriptions.push(
			context.algorithmId === "boykov-kolmogorov"
				? "Boykov–Kolmogorov's reused S/T trees use solid teal and dashed indigo. Outer dotted rings mark the active frontier; dashed orange rings mark orphans. Orange residual arcs form the augmenting path where the trees meet. Grow keeps the trees, while Adopt repairs only parent edges broken by Augment."
				: "IBFS uses solid teal with S·dₛ for the S tree and dashed indigo with T·dₜ for the T tree. Dashed orange rings mark orphans; outer dotted rings mark the growth frontier. Orange residual arcs form the current shortest augmenting path.",
		);
	}
	if (visualization.eibfsView !== undefined) {
		descriptions.push(
			"Excesses IBFS uses solid teal for the S forest and dashed indigo for the T forest. Node annotations show membership, retained distance, and finite imbalance. Corner symbols identify terminal roots ±∞ or finite excess/deficit roots ±. Dashed orange rings mark orphans, outer dotted rings mark the current frontier, and rounded frames mark repair targets. T-forest residual arrows point from child to root.",
		);
	} else if (visualization.eibfsStage === "recovery") {
		descriptions.push(
			"Excesses IBFS is restoring conservation by canceling positive flow on the same side of the frozen cut. The search forest is intentionally hidden. Orange residual arcs form the current same-cut cancellation path.",
		);
	} else if (visualization.eibfsStage === "certified") {
		descriptions.push(
			"Excesses IBFS pseudoflow recovery is complete. The view now shows an independently verified feasible maximum flow; the search forest remains hidden.",
		);
	}
	add(
		features.warmStartPushRelabel,
		"For warm-start push–relabel, dotted capacity rails mark edges with positive predicted flow. Red nodes and the cut boundary show the maintained S side. Orange residual arcs trace paths actually changed during cut saturation, excess/deficit separation, and conservation recovery. Node annotations show Algorithm 4 height h and signed imbalance.",
	);
	add(
		context.outcome?.kind === "infeasible",
		"Dotted orange nodes are the cut witness reachable from supply nodes in the residual graph. Highlighted routes cross the witness partition.",
	);
	add(
		features.relaxation,
		"Relaxation node annotations show source price π, deficit d, and FIFO label order. Orange residual arcs point in the direction that changes flow.",
	);
	add(
		features.epsilonRelaxation,
		"Epsilon-relaxation node annotations show price p̂ on the (n+1)-scaled cost system, surplus g, and selection order. Orange marks an epsilon-balanced residual push.",
	);
	add(
		features.predictionAssisted,
		"For prediction-assisted ε-relaxation, dotted violet rings show input dual predictions, broken magenta rings show predictions clipped by Algorithm 1, and double orange rings mark positive-surplus work items. Teal, violet, and gray edge strokes encode positive, negative, and zero scaled cost at scale t through width and dash pattern. Orange arrows are ε-balanced pushes. The T ladder shows Remark 1's exponential candidates: hatched × marks aborted trials; solid marks the current or successful trial.",
	);
	add(
		features.tardosFramework,
		"For Tardos network-matrix variable fixing, node annotation π is the input potential. Solid teal, dashed violet, and dotted gray residual arcs show positive, negative, and zero reduced cost; width shows absolute value. Directions strictly above nε use a double orange stroke, and the original edge is highlighted as certified fixed at lower bound L or upper bound U. This is one variable-fixing primitive, not a complete optimization claim.",
	);
	add(
		features.blockingPrimalDual,
		"Blocking-flow primal-dual node annotations show dual price π, shortest slack d̄, equality level ℓ, or remaining balance b′ according to the event. Orange marks the current zero-reduced-cost augmenting path.",
	);
	add(
		features.arcFixing,
		"Edges temporarily excluded by bound-only speculative arc fixing use wide, long dashes with low saturation. An active orange operation takes visual priority over the fixed state.",
	);
	add(
		features.doubleScaling,
		"In double scaling, dashed violet marks negative-reduced-cost arcs in the transportation transform and solid orange marks the current admissible path. Edge tooltips distinguish costed flow branches from zero-cost slack branches. Node annotations show scaled price π̂, imbalance e, and current arc a.",
	);
	add(
		features.relaxedMndc,
		"In relaxed MNDC, dashed violet marks residual arcs chosen by the split-node assignment. Heavy color-coded strokes and matching rings identify the vertex-disjoint negative-cycle family canceled together. Node annotations σL/σR are exact assignment duals, ↦ points to the matching right copy, and ε is the outer dyadic relaxation phase.",
	);
	add(
		features.enhancedCapacityScaling,
		"For enhanced capacity scaling, translucent outlines show contracted components and orange residual arcs show the quotient shortest path. Inner width shows exact virtual flow; teal marks tight arcs, violet dots mark strongly feasible arcs, and faint strokes mark component-internal arcs. Node annotations show component, exact excess, dual price, and distance at selection.",
	);
	add(
		features.orlinMcf,
		"Orlin MCF converts each finite-capacity edge into a central diamond capacity node and two branches. Solid F is the costed flow branch; dashed S is the zero-cost slack branch. Width shows exact pseudoflow, teal marks tight arcs, violet marks 3nΔ contraction candidates, and orange marks the compressed shortest path after eliminating capacity nodes.",
	);
	add(
		features.orlinMaxFlow,
		"In Orlin max flow, solid teal A is abundant, long-dashed violet Ā is anti-abundant, dotted gray S is small, and double yellow M is medium. Rounded frames show contracted components and heavy solid rings mark critical components. Compact arcs encode capacity by outer width, flow by inner width, and kind by O/P/T. Orange marks the current transfer, lift, or repair.",
	);
	add(
		features.dualNetworkSimplex,
		"In dual network simplex, heavy teal strokes form the dual-feasible tree basis. Dashed red marks a leaving arc with negative signed basic flow; double orange marks an entering arc leaving the head-side cut. Blue node rings show cut H, annotation π is exact dual price, and edge labels xᴮ and c̄ are basic flow and reduced cost.",
	);
	add(
		features.polynomialDualSimplex,
		"In polynomial dual network simplex, the heavy teal outer stroke is the dual-feasible tree, teal dots with varying width show auxiliary pseudoflow, and long violet dashes show active-to-root augmentation. Short red dashes mark bad or leaving arcs; double orange marks entering arcs. Capacity, cost, and actual-flow inner strokes remain separate from the paper's auxiliary pseudoflow layer.",
	);
	add(
		features.polynomialPrimalSimplex,
		"In polynomial primal network simplex, heavy solid teal is the tree basis; thin gray dashes and dots are nonbasic edges at lower and upper bounds. Long violet dashes mark an admissible fundamental cycle, double orange marks entering, and short red dashes mark leaving. Inner width shows perturbed flow. Solid, dotted, and long-dashed node rings mark eligible, awake, and N*; annotation q is the exact premultiplier.",
	);
	if (features.convexCost) {
		descriptions.push(
			context.algorithmId === CONVEX_NETWORK_SIMPLEX_ALGORITHM
				? "In Pasche's piecewise-linear convex network simplex, heavy solid teal is the compact tree basis including the artificial root; gray dots are nonbasic edges at breakpoints. Long violet dashes and direction arrows mark the priced fundamental cycle, double orange marks entering, and short red dashes mark the leaving breakpoint chosen by Cunningham's rule. A strong frame on the segmented rail marks a tree edge's active segment. A cross-breakpoint step may pass several breakpoints but performs at most one final basis exchange."
				: context.algorithmId === CONVEX_COST_SCALING_ALGORITHM
					? "In Pinto–Shamir convex cost scaling, dashed violet marks forward or reverse marginal segments with at least current Δ residual capacity. Orange marks the chosen shortest path or saturated segment. Segmented rails show segment width, slope, and utilization. Node annotations show dual potential, residual imbalance, and Dijkstra order."
					: "For piecewise-linear convex costs, the color inside the capacity rail shows the marginal cost of the next forward unit. The segmented rail below the label shows each segment's width, slope, and used fraction. Solid orange is the forward direction of an improving cycle; dashed orange is reverse. Label φ is current cost; μ+ and μ− are forward and reverse residual marginal costs.",
		);
	}
	add(
		state.planarDual?.kind === "hassin-split",
		"Translucent circles are faces of the split dual. Blue bidirectional arrows are dual arcs crossing original edges; labels show forward capacity / reverse 0. Orange marks the face settled by Dijkstra and its predecessor arc.",
	);
	add(
		state.planarDual?.kind === "borradaile-klein-unsplit",
		"Translucent circles are faces of the unsplit dual; f∞ is the designated infinite face. Blue bidirectional arrows are capacity / 0 dual arcs used by clockwise-cycle preprocessing. Orange primal residual edges and node numbers show the right-first tree and leftmost path being reconstructed.",
	);
	return descriptions.join(" ");
}
