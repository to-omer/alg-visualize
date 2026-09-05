import type { FlowTraceEntityRefV1 } from "./flow-scene";

export function flowTraceEntityIdentity(entity: FlowTraceEntityRefV1): string {
	if (entity.kind === "node") return `node:${entity.node_id}`;
	if (entity.kind === "edge") return `edge:${entity.edge_id}`;
	return `residual-arc:${entity.edge_id}:${entity.direction}`;
}
