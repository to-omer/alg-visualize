import type { FlowEntitySelection } from "./flow-entity-navigator";

type ResidualArcDirection = Extract<
	FlowEntitySelection,
	{ kind: "residual-arc" }
>["direction"];

export function isOriginalEdgeSelected(
	selection: FlowEntitySelection | undefined,
	edgeId: string,
): boolean {
	return selection?.kind === "edge" && selection.id === edgeId;
}

export function isResidualArcSelected(
	selection: FlowEntitySelection | undefined,
	edgeId: string,
	direction: ResidualArcDirection,
): boolean {
	return (
		selection?.kind === "residual-arc" &&
		selection.edgeId === edgeId &&
		selection.direction === direction
	);
}
