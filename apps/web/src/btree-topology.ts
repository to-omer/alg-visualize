import { entityIdKey, type StructureSnapshot } from "./engine-types";

/** Small-scene semantic topology used by cross-boundary conformance tests. */
export function encodeSmallBtreeTopology(
	structure: StructureSnapshot,
): string | undefined {
	if (
		structure.nodes.length > 256 ||
		!structure.nodes.every((node) => node.role.startsWith("btree-"))
	) {
		return undefined;
	}
	const nodes = new Map(
		structure.nodes.map((node) => [entityIdKey(node.id), node] as const),
	);
	const root =
		structure.root === null
			? []
			: (nodes.get(entityIdKey(structure.root))?.keys ?? []);
	const edges = structure.nodes
		.flatMap((node) =>
			node.links.map((link) => [
				node.keys,
				link.role,
				nodes.get(entityIdKey(link.target))?.keys ?? [],
			]),
		)
		.sort((left, right) =>
			JSON.stringify(left).localeCompare(JSON.stringify(right)),
		);
	return JSON.stringify({ edges, root });
}
