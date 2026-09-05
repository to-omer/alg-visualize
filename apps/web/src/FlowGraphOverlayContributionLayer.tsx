import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;

/**
 * Registry-composed SVG fallback for an overlay without a bespoke rich layer.
 * Entity references discovered by the contribution projector become visible
 * without adding an overlay-specific branch to the root graph renderer.
 */
export function FlowGraphOverlayContributionLayer({
	state,
}: Readonly<{ state: FlowEntityGraphState }>) {
	const presentation = state.plan.overlayPresentation;
	if (
		presentation.genericNodeDecorations.length === 0 &&
		presentation.genericEdgeDecorations.length === 0 &&
		presentation.genericResidualArcDecorations.length === 0
	) {
		return null;
	}

	return (
		<g className="flow-overlay-contribution-layer">
			{presentation.genericResidualArcDecorations.map((decoration) => {
				if (decoration.kind !== "residual-arc") return null;
				const route = state.layout.routes.get(decoration.entityId);
				if (route === undefined) return null;
				return (
					<path
						key={`${decoration.overlay}:residual:${decoration.entityId}:${decoration.direction}`}
						className={`flow-overlay-contribution-edge flow-overlay-contribution-${decoration.accent}`}
						data-overlay-contribution={decoration.overlay}
						data-overlay-entity-kind="residual-arc"
						data-overlay-entity-id={decoration.entityId}
						data-overlay-residual-direction={decoration.direction}
						data-overlay-roles={decoration.roles.join("|")}
						d={
							decoration.direction === "forward"
								? route.path
								: route.reversePath
						}
					/>
				);
			})}
			{presentation.genericEdgeDecorations.map((decoration) => {
				if (decoration.kind !== "edge") return null;
				const route = state.layout.routes.get(decoration.entityId);
				if (route === undefined) return null;
				return (
					<path
						key={`${decoration.overlay}:edge:${decoration.entityId}`}
						className={`flow-overlay-contribution-edge flow-overlay-contribution-${decoration.accent}`}
						data-overlay-contribution={decoration.overlay}
						data-overlay-entity-kind="edge"
						data-overlay-entity-id={decoration.entityId}
						data-overlay-roles={decoration.roles.join("|")}
						d={route.path}
					/>
				);
			})}
			{presentation.genericNodeDecorations.map((decoration, index) => {
				if (decoration.kind !== "node") return null;
				const position = state.positions.get(decoration.entityId);
				if (position === undefined) return null;
				return (
					<circle
						key={`${decoration.overlay}:node:${decoration.entityId}`}
						className={`flow-overlay-contribution-node flow-overlay-contribution-${decoration.accent}`}
						data-overlay-contribution={decoration.overlay}
						data-overlay-entity-kind="node"
						data-overlay-entity-id={decoration.entityId}
						data-overlay-roles={decoration.roles.join("|")}
						cx={position.x}
						cy={position.y}
						r={35 + (index % 3) * 4}
					/>
				);
			})}
		</g>
	);
}
