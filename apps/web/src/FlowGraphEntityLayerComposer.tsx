import { FlowGraphAdvancedAlgorithmFeatureBundle } from "./FlowGraphAdvancedAlgorithmFeatureBundle";
import {
	FlowGraphAlgorithmMidLayers,
	FlowGraphAlgorithmUnderlays,
	FlowGraphBlockingFlowLevelLayer,
} from "./FlowGraphAlgorithmLayers";
import { FlowGraphEdgeAnnotationLayers } from "./FlowGraphEdgeAnnotationFeatureBundle";
import { FlowGraphFeasibilityLayer } from "./FlowGraphFeasibilityLayer";
import { FlowGraphOriginalLayer } from "./FlowGraphOriginalEdgeFeatureBundle";
import {
	FlowGraphAuxiliaryCellLayer,
	FlowGraphOverlayFeatureLayers,
	FlowGraphOverlayStatusLayer,
	FlowGraphSourceOperationLayer,
} from "./FlowGraphOverlayFeatureLayers";
import { FlowGraphResidualLayer } from "./FlowGraphResidualFeatureBundle";
import { FlowGraphNodeLayer } from "./FlowGraphRichNodeFeatureBundle";
import { FlowRmfgenFrameGroups } from "./FlowRmfgenFrameGroups";
import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import type { FlowEntitySelection } from "./flow-entity-navigator";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;

/**
 * Owns SVG feature ordering for the entity graph.
 *
 * Overlay-specific projection and presentation stay behind the contribution
 * registry. The root graph only needs this composer and never chooses an
 * algorithm-specific layer.
 */
export function FlowGraphEntityLayerComposer({
	state,
	selection,
	hoveredEdgeId,
}: Readonly<{
	state: FlowEntityGraphState;
	selection: FlowEntitySelection | undefined;
	hoveredEdgeId: string | undefined;
}>) {
	const feasibilityDomain =
		state.renderData.overlayViews.feasibility?.domain.kind;
	if (feasibilityDomain === "standalone-transformation") {
		return (
			<>
				<FlowGraphFeasibilityLayer state={state} />
				<FlowGraphSourceOperationLayer state={state} />
			</>
		);
	}
	if (feasibilityDomain === "node-aligned-transformation") {
		return (
			<>
				<g className="flow-feasibility-public-edge-context">
					<FlowGraphOriginalLayer
						state={state}
						selection={selection}
						hoveredEdgeId={hoveredEdgeId}
					/>
				</g>
				<FlowGraphFeasibilityLayer state={state} />
				<FlowGraphNodeLayer state={state} selection={selection} />
				<FlowGraphSourceOperationLayer state={state} />
			</>
		);
	}
	return (
		<>
			<FlowGraphOverlayFeatureLayers state={state} />
			<FlowRmfgenFrameGroups groups={state.frameGroups} />
			<FlowGraphAlgorithmUnderlays state={state} selection={selection} />
			<FlowGraphResidualLayer
				state={state}
				selection={selection}
				hoveredEdgeId={hoveredEdgeId}
			/>
			<FlowGraphAlgorithmMidLayers state={state} selection={selection} />
			<FlowGraphBlockingFlowLevelLayer state={state} />
			<FlowGraphOriginalLayer
				state={state}
				selection={selection}
				hoveredEdgeId={hoveredEdgeId}
			/>
			<FlowGraphFeasibilityLayer state={state} />
			<FlowGraphAdvancedAlgorithmFeatureBundle state={state} />
			<FlowGraphEdgeAnnotationLayers
				state={state}
				selection={selection}
				hoveredEdgeId={hoveredEdgeId}
			/>
			<FlowGraphNodeLayer state={state} selection={selection} />
			<FlowGraphAuxiliaryCellLayer state={state} />
			<FlowGraphOverlayStatusLayer state={state} />
			<FlowGraphSourceOperationLayer state={state} />
		</>
	);
}
