import type {
	StructureEntityId,
	StructureNode,
	StructureSnapshot,
} from "./engine-types";

type StructureIndex = {
	auxiliary: Map<number, StructureNode>;
	nodes: Map<number, StructureNode>;
};

const indexes = new WeakMap<StructureSnapshot, StructureIndex>();

function structureIndex(structure: StructureSnapshot): StructureIndex {
	const cached = indexes.get(structure);
	if (cached !== undefined) {
		return cached;
	}
	const index: StructureIndex = { auxiliary: new Map(), nodes: new Map() };
	for (const node of structure.nodes) {
		const target = node.id.kind === "node" ? index.nodes : index.auxiliary;
		target.set(node.id.id.index, node);
	}
	indexes.set(structure, index);
	return index;
}

export function getStructureNode(
	structure: StructureSnapshot,
	id: StructureEntityId,
): StructureNode | undefined {
	const index = structureIndex(structure);
	const node = (id.kind === "node" ? index.nodes : index.auxiliary).get(
		id.id.index,
	);
	return node?.id.id.generation === id.id.generation ? node : undefined;
}

export function getStructureNodeByKey(
	structure: StructureSnapshot,
	key: string,
): StructureNode | undefined {
	const [kind, index, generation, trailing] = key.split(":");
	if (
		trailing !== undefined ||
		(kind !== "node" && kind !== "auxiliary") ||
		index === undefined ||
		generation === undefined
	) {
		return undefined;
	}
	const parsedIndex = Number(index);
	const parsedGeneration = Number(generation);
	if (
		!Number.isSafeInteger(parsedIndex) ||
		!Number.isSafeInteger(parsedGeneration) ||
		parsedIndex < 0 ||
		parsedGeneration < 0
	) {
		return undefined;
	}
	return getStructureNode(structure, {
		kind,
		id: { index: parsedIndex, generation: parsedGeneration },
	});
}
