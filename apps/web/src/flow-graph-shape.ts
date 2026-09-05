import type { FlowEdgeV1, FlowNodeV1 } from "./flow-scene";

export type FlowTerminals = Readonly<{
	source: string;
	sink: string;
	requiredFlow?: bigint;
}>;

export type FlowTransportationPartitions = Readonly<{
	origins: readonly string[];
	destinations: readonly string[];
}>;

export type DirectedFlowBipartition = Readonly<{
	left: ReadonlySet<string>;
	right: ReadonlySet<string>;
	directionCoherence: number;
}>;

export type FlowGraphShape = Readonly<{
	noSelfLoops: boolean;
	zeroFlowFeasible: boolean;
	positiveCapacity: boolean;
	nonEmptyEdges: boolean;
	zeroCost: boolean;
	distinctTerminals: boolean;
	underlyingConnected: boolean;
	unitCapacity: boolean;
	unitNetwork: boolean;
	bipartite: boolean;
	balancedBipartite: boolean;
	transportationNetwork: boolean;
	stronglyConnected: boolean;
	nonbindingTransshipmentCapacities: boolean;
	lowerBoundResidualNegativeCycle: "absent" | "present" | "unavailable";
	planarEmbedding: "verified" | "unavailable";
}>;

const MAX_BROWSER_RESIDUAL_RELAXATIONS = 2_000_000;

function compareText(left: string, right: string): number {
	return left < right ? -1 : left > right ? 1 : 0;
}

function parseInteger(value: string): bigint | undefined {
	try {
		return BigInt(value);
	} catch {
		return undefined;
	}
}

function lowerBoundResidualNegativeCycle(
	nodes: readonly FlowNodeV1[],
	edges: readonly FlowEdgeV1[],
	validEndpoints: boolean,
): FlowGraphShape["lowerBoundResidualNegativeCycle"] {
	if (!validEndpoints) return "unavailable";
	const nodeIndex = new Map(nodes.map((node, index) => [node.id, index]));
	const residual: Array<Readonly<{ from: number; to: number; cost: bigint }>> =
		[];
	for (const edge of edges) {
		const from = nodeIndex.get(edge.from);
		const to = nodeIndex.get(edge.to);
		const lower = parseInteger(edge.lower);
		const capacity = parseInteger(edge.capacity);
		const cost = parseInteger(edge.cost);
		if (
			from === undefined ||
			to === undefined ||
			lower === undefined ||
			capacity === undefined ||
			cost === undefined ||
			lower < 0n ||
			capacity < lower
		) {
			return "unavailable";
		}
		if (capacity > lower) residual.push({ from, to, cost });
	}
	if (residual.every((arc) => arc.cost >= 0n)) return "absent";
	if (nodes.length * residual.length > MAX_BROWSER_RESIDUAL_RELAXATIONS) {
		return "unavailable";
	}

	// Zero distance to every node is the implicit super-source construction.
	// A relaxation in round |V| proves a negative cycle in any component.
	const distance = Array.from({ length: nodes.length }, () => 0n);
	for (let round = 0; round < nodes.length; round += 1) {
		let changed = false;
		for (const arc of residual) {
			const candidate = (distance[arc.from] ?? 0n) + arc.cost;
			if (candidate >= (distance[arc.to] ?? 0n)) continue;
			distance[arc.to] = candidate;
			changed = true;
			if (round + 1 === nodes.length) return "present";
		}
		if (!changed) return "absent";
	}
	return "absent";
}

export function directedFlowBipartition(
	nodes: readonly FlowNodeV1[],
	edges: readonly FlowEdgeV1[],
): DirectedFlowBipartition | undefined {
	if (nodes.length < 2) return undefined;
	const adjacency = new Map(nodes.map((node) => [node.id, [] as string[]]));
	const validEdges = edges.filter(
		(edge) => adjacency.has(edge.from) && adjacency.has(edge.to),
	);
	if (validEdges.length !== edges.length) return undefined;
	for (const edge of validEdges) {
		if (edge.from === edge.to) return undefined;
		adjacency.get(edge.from)?.push(edge.to);
		adjacency.get(edge.to)?.push(edge.from);
	}
	for (const neighbors of adjacency.values()) neighbors.sort(compareText);
	const color = new Map<string, 0 | 1>();
	const componentByNode = new Map<string, number>();
	let componentCount = 0;

	for (const start of [...adjacency.keys()].sort(compareText)) {
		if (color.has(start)) continue;
		const component = componentCount;
		componentCount += 1;
		color.set(start, 0);
		componentByNode.set(start, component);
		const queue = [start];
		for (let cursor = 0; cursor < queue.length; cursor += 1) {
			const current = queue[cursor];
			if (current === undefined) continue;
			const currentColor = color.get(current);
			if (currentColor === undefined) continue;
			for (const neighbor of adjacency.get(current) ?? []) {
				const neighborColor = color.get(neighbor);
				if (neighborColor === currentColor) return undefined;
				if (neighborColor !== undefined) continue;
				color.set(neighbor, currentColor === 0 ? 1 : 0);
				componentByNode.set(neighbor, component);
				queue.push(neighbor);
			}
		}
	}

	const statistics = Array.from({ length: componentCount }, () => ({
		edges: 0,
		zeroToOne: 0,
	}));
	for (const edge of validEdges) {
		const component = componentByNode.get(edge.from);
		if (component === undefined) continue;
		const statistic = statistics[component];
		if (statistic === undefined) continue;
		statistic.edges += 1;
		if (color.get(edge.from) === 0 && color.get(edge.to) === 1) {
			statistic.zeroToOne += 1;
		}
	}
	const flips = statistics.map(
		(statistic) => statistic.zeroToOne * 2 < statistic.edges,
	);
	const left = new Set<string>();
	const right = new Set<string>();
	for (const node of nodes) {
		const component = componentByNode.get(node.id);
		const flip = component === undefined ? false : (flips[component] ?? false);
		const belongsLeft = (color.get(node.id) === 0) !== flip;
		(belongsLeft ? left : right).add(node.id);
	}
	const preferredDirections = statistics.reduce(
		(total, statistic) =>
			total +
			Math.max(statistic.zeroToOne, statistic.edges - statistic.zeroToOne),
		0,
	);
	return {
		left,
		right,
		directionCoherence:
			validEdges.length === 0 ? 1 : preferredDirections / validEdges.length,
	};
}

export function analyzeFlowGraphShape(
	nodes: readonly FlowNodeV1[],
	edges: readonly FlowEdgeV1[],
	terminals?: FlowTerminals,
	transportationPartitions?: FlowTransportationPartitions,
): FlowGraphShape {
	const nodeIds = new Set(nodes.map((node) => node.id));
	const validEndpoints =
		nodeIds.size === nodes.length &&
		edges.every((edge) => nodeIds.has(edge.from) && nodeIds.has(edge.to));
	const noSelfLoops =
		validEndpoints && edges.every((edge) => edge.from !== edge.to);
	const unitCapacity =
		validEndpoints &&
		edges.every((edge) => edge.lower === "0" && edge.capacity === "1");
	const zeroFlowFeasible =
		validEndpoints &&
		nodes.every((node) => node.supply === "0") &&
		edges.every((edge) => edge.lower === "0");
	const positiveCapacity =
		validEndpoints &&
		edges.every((edge) => {
			const capacity = parseInteger(edge.capacity);
			return capacity !== undefined && capacity > 0n;
		});
	const nonEmptyEdges = validEndpoints && edges.length > 0;
	const zeroCost = validEndpoints && edges.every((edge) => edge.cost === "0");
	const distinctTerminals =
		terminals !== undefined &&
		terminals.source !== terminals.sink &&
		nodeIds.has(terminals.source) &&
		nodeIds.has(terminals.sink);
	const undirected = new Map(nodes.map((node) => [node.id, [] as string[]]));
	if (validEndpoints) {
		for (const edge of edges) {
			undirected.get(edge.from)?.push(edge.to);
			undirected.get(edge.to)?.push(edge.from);
		}
	}
	const underlyingSeen = new Set<string>();
	const underlyingStart = nodes[0]?.id;
	if (validEndpoints && underlyingStart !== undefined) {
		underlyingSeen.add(underlyingStart);
		const queue = [underlyingStart];
		for (let cursor = 0; cursor < queue.length; cursor += 1) {
			const current = queue[cursor];
			if (current === undefined) continue;
			for (const neighbor of undirected.get(current) ?? []) {
				if (underlyingSeen.has(neighbor)) continue;
				underlyingSeen.add(neighbor);
				queue.push(neighbor);
			}
		}
	}
	const underlyingConnected =
		validEndpoints && nodes.length > 0 && underlyingSeen.size === nodes.length;
	const indegree = new Map(nodes.map((node) => [node.id, 0]));
	const outdegree = new Map(nodes.map((node) => [node.id, 0]));
	for (const edge of edges) {
		if (!validEndpoints) break;
		indegree.set(edge.to, (indegree.get(edge.to) ?? 0) + 1);
		outdegree.set(edge.from, (outdegree.get(edge.from) ?? 0) + 1);
	}
	const unitNetwork =
		unitCapacity &&
		terminals !== undefined &&
		nodeIds.has(terminals.source) &&
		nodeIds.has(terminals.sink) &&
		nodes.every(
			(node) =>
				node.id === terminals.source ||
				node.id === terminals.sink ||
				indegree.get(node.id) === 1 ||
				outdegree.get(node.id) === 1,
		);
	const partition = validEndpoints
		? directedFlowBipartition(nodes, edges)
		: undefined;
	const bipartite = partition !== undefined;
	const balancedBipartite =
		partition !== undefined && partition.left.size === partition.right.size;
	const totalSupply = nodes.reduce<bigint | undefined>((total, node) => {
		const supply = parseInteger(node.supply);
		return total === undefined || supply === undefined
			? undefined
			: total + supply;
	}, 0n);
	const transportationNetwork =
		transportationPartitions === undefined
			? partition !== undefined &&
				partition.directionCoherence === 1 &&
				totalSupply === 0n &&
				nodes.every((node) => {
					const supply = parseInteger(node.supply);
					return (
						supply !== undefined &&
						(partition.left.has(node.id) ? supply >= 0n : supply <= 0n)
					);
				})
			: isDeclaredTransportationNetwork(nodes, edges, transportationPartitions);
	const positiveForward = new Map(
		nodes.map((node) => [node.id, [] as string[]]),
	);
	const positiveReverse = new Map(
		nodes.map((node) => [node.id, [] as string[]]),
	);
	let validWidths = validEndpoints;
	for (const edge of edges) {
		const lower = parseInteger(edge.lower);
		const capacity = parseInteger(edge.capacity);
		if (lower === undefined || capacity === undefined || capacity < lower) {
			validWidths = false;
			break;
		}
		if (capacity === lower) continue;
		positiveForward.get(edge.from)?.push(edge.to);
		positiveReverse.get(edge.to)?.push(edge.from);
	}
	const reachesEveryNode = (
		adjacency: ReadonlyMap<string, readonly string[]>,
	) => {
		const start = nodes[0]?.id;
		if (start === undefined) return false;
		const seen = new Set([start]);
		const queue = [start];
		for (let cursor = 0; cursor < queue.length; cursor += 1) {
			const current = queue[cursor];
			if (current === undefined) continue;
			for (const neighbor of adjacency.get(current) ?? []) {
				if (seen.has(neighbor)) continue;
				seen.add(neighbor);
				queue.push(neighbor);
			}
		}
		return seen.size === nodes.length;
	};
	const stronglyConnected =
		validWidths &&
		reachesEveryNode(positiveForward) &&
		reachesEveryNode(positiveReverse);
	const lowerDivergence = new Map(nodes.map((node) => [node.id, 0n]));
	let validTransshipmentNumbers =
		validEndpoints &&
		(terminals?.requiredFlow === undefined ||
			(terminals.requiredFlow >= 0n && distinctTerminals));
	for (const edge of edges) {
		const lower = parseInteger(edge.lower);
		if (lower === undefined) {
			validTransshipmentNumbers = false;
			break;
		}
		lowerDivergence.set(
			edge.from,
			(lowerDivergence.get(edge.from) ?? 0n) + lower,
		);
		lowerDivergence.set(edge.to, (lowerDivergence.get(edge.to) ?? 0n) - lower);
	}
	let shiftedPositiveSupply = 0n;
	for (const node of nodes) {
		const supply = parseInteger(node.supply);
		if (supply === undefined) {
			validTransshipmentNumbers = false;
			break;
		}
		const terminalFlow =
			terminals?.requiredFlow === undefined
				? 0n
				: node.id === terminals.source
					? terminals.requiredFlow
					: node.id === terminals.sink
						? -terminals.requiredFlow
						: 0n;
		const shifted =
			supply + terminalFlow - (lowerDivergence.get(node.id) ?? 0n);
		if (shifted > 0n) shiftedPositiveSupply += shifted;
	}
	const requiredWidth = shiftedPositiveSupply;
	const nonbindingTransshipmentCapacities =
		validTransshipmentNumbers &&
		edges.every((edge) => {
			const lower = parseInteger(edge.lower);
			const capacity = parseInteger(edge.capacity);
			return (
				lower !== undefined &&
				capacity !== undefined &&
				capacity - lower >= requiredWidth
			);
		});
	const residualNegativeCycle = lowerBoundResidualNegativeCycle(
		nodes,
		edges,
		validEndpoints,
	);
	return {
		noSelfLoops,
		zeroFlowFeasible,
		positiveCapacity,
		nonEmptyEdges,
		zeroCost,
		distinctTerminals,
		underlyingConnected,
		unitCapacity,
		unitNetwork,
		bipartite,
		balancedBipartite,
		transportationNetwork,
		stronglyConnected,
		nonbindingTransshipmentCapacities,
		lowerBoundResidualNegativeCycle: residualNegativeCycle,
		planarEmbedding: "unavailable",
	};
}

function isDeclaredTransportationNetwork(
	nodes: readonly FlowNodeV1[],
	edges: readonly FlowEdgeV1[],
	partitions: FlowTransportationPartitions,
): boolean {
	const { origins, destinations } = partitions;
	if (origins.length === 0 || destinations.length === 0) return false;
	const canonical = (ids: readonly string[]) =>
		ids.every((id, index) => index === 0 || (ids[index - 1] ?? id) < id);
	if (!canonical(origins) || !canonical(destinations)) return false;
	const originSet = new Set(origins);
	const destinationSet = new Set(destinations);
	if (
		originSet.size !== origins.length ||
		destinationSet.size !== destinations.length ||
		origins.some((id) => destinationSet.has(id))
	)
		return false;
	const nodeById = new Map(nodes.map((node) => [node.id, node]));
	if (
		nodeById.size !== nodes.length ||
		originSet.size + destinationSet.size !== nodes.length ||
		origins.some((id) => !nodeById.has(id)) ||
		destinations.some((id) => !nodeById.has(id))
	)
		return false;
	const originSupply = new Map<string, bigint>();
	const destinationDemand = new Map<string, bigint>();
	let totalSupply = 0n;
	let totalDemand = 0n;
	for (const id of origins) {
		const supply = parseInteger(nodeById.get(id)?.supply ?? "");
		if (supply === undefined || supply <= 0n) return false;
		originSupply.set(id, supply);
		totalSupply += supply;
	}
	for (const id of destinations) {
		const supply = parseInteger(nodeById.get(id)?.supply ?? "");
		if (supply === undefined || supply >= 0n) return false;
		const demand = -supply;
		destinationDemand.set(id, demand);
		totalDemand += demand;
	}
	if (totalSupply !== totalDemand) return false;
	const pairs = new Set<string>();
	for (const edge of edges) {
		const supply = originSupply.get(edge.from);
		const demand = destinationDemand.get(edge.to);
		const lower = parseInteger(edge.lower);
		const capacity = parseInteger(edge.capacity);
		if (
			supply === undefined ||
			demand === undefined ||
			lower !== 0n ||
			capacity === undefined ||
			capacity < (supply < demand ? supply : demand)
		)
			return false;
		const pair = `${edge.from}\u0000${edge.to}`;
		if (pairs.has(pair)) return false;
		pairs.add(pair);
	}
	return true;
}
