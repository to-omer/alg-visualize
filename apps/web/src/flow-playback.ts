import type { FlowPlaybackGranularity } from "./flow-preferences";
import type { FlowCurrentSceneV9 } from "./flow-scene";

const GRANULARITY_RANK: Readonly<Record<FlowPlaybackGranularity, number>> = {
	phase: 0,
	operation: 1,
	micro: 2,
};

export type FlowBoundaryInventory = {
	readonly minimumByRawPosition: Map<number, FlowPlaybackGranularity>;
	readonly phasePositions: number[];
	readonly operationPositions: number[];
	extent: number;
	prefixEnd: number;
};

export function createFlowBoundaryInventory(): FlowBoundaryInventory {
	return {
		minimumByRawPosition: new Map(),
		phasePositions: [0],
		operationPositions: [0],
		extent: 0,
		prefixEnd: 0,
	};
}

export function resetFlowBoundaryInventory(
	inventory: FlowBoundaryInventory,
): void {
	inventory.minimumByRawPosition.clear();
	inventory.phasePositions.splice(0, inventory.phasePositions.length, 0);
	inventory.operationPositions.splice(
		0,
		inventory.operationPositions.length,
		0,
	);
	inventory.extent = 0;
	inventory.prefixEnd = 0;
}

function insertSortedUnique(positions: number[], position: number): void {
	let low = 0;
	let high = positions.length;
	while (low < high) {
		const middle = low + Math.floor((high - low) / 2);
		if ((positions[middle] ?? Number.POSITIVE_INFINITY) < position) {
			low = middle + 1;
		} else {
			high = middle;
		}
	}
	if (positions[low] !== position) positions.splice(low, 0, position);
}

function ensureFlowBoundaryExtent(
	inventory: FlowBoundaryInventory,
	extent: number,
): void {
	if (inventory.extent === extent) return;
	if (inventory.extent !== 0 || inventory.minimumByRawPosition.size > 0) {
		throw new Error(
			"Flow boundary extent changed without resetting its session",
		);
	}
	inventory.extent = extent;
	if (extent > 0) {
		insertSortedUnique(inventory.phasePositions, extent);
		insertSortedUnique(inventory.operationPositions, extent);
	}
}

/** Incrementally records one raw event without rescanning the observed prefix. */
export function recordFlowBoundary(
	inventory: FlowBoundaryInventory,
	position: number,
	extent: number,
	minimum: FlowPlaybackGranularity | undefined,
): void {
	if (
		!Number.isSafeInteger(position) ||
		!Number.isSafeInteger(extent) ||
		position < 0 ||
		position > extent
	) {
		throw new Error("Flow boundary position is outside its canonical extent");
	}
	ensureFlowBoundaryExtent(inventory, extent);
	if (minimum === undefined || position === 0) return;
	const previous = inventory.minimumByRawPosition.get(position);
	if (previous !== undefined) {
		if (previous !== minimum) {
			throw new Error("Flow boundary kind changed at a stable raw position");
		}
		return;
	}
	inventory.minimumByRawPosition.set(position, minimum);
	while (inventory.minimumByRawPosition.has(inventory.prefixEnd + 1)) {
		inventory.prefixEnd += 1;
	}
	if (position >= extent) return;
	if (minimum === "phase") {
		insertSortedUnique(inventory.phasePositions, position);
		insertSortedUnique(inventory.operationPositions, position);
	} else if (minimum === "operation") {
		insertSortedUnique(inventory.operationPositions, position);
	}
}

export function flowTraceBoundaryVisible(
	minimum: FlowPlaybackGranularity,
	selected: FlowPlaybackGranularity,
): boolean {
	return GRANULARITY_RANK[minimum] <= GRANULARITY_RANK[selected];
}

/**
 * Keeps a user's preferred boundary while selecting the nearest boundary kind
 * that the current endpoint actually records.
 */
export function flowEffectivePlaybackGranularity(
	preferred: FlowPlaybackGranularity,
	steps: FlowCurrentSceneV9["trace_steps"],
): FlowPlaybackGranularity {
	const phaseAvailable = steps.phase_availability.availability === "available";
	const operationAvailable =
		steps.operation_availability.availability === "available";
	const detailAvailable = steps.detail.availability === "available";
	if (preferred === "micro") {
		if (detailAvailable) return "micro";
		if (operationAvailable) return "operation";
		if (phaseAvailable) return "phase";
	} else if (preferred === "operation") {
		if (operationAvailable) return "operation";
		if (phaseAvailable) return "phase";
		if (detailAvailable) return "micro";
	} else {
		if (phaseAvailable) return "phase";
		if (operationAvailable) return "operation";
		if (detailAvailable) return "micro";
	}
	throw new Error("Flow endpoint records no playback boundary kind");
}

/**
 * Projects discovered raw trace metadata into the ordinal positions exposed by
 * a semantic or phase slider. Initial and terminal boundaries remain reachable
 * even before every interior event has been visited.
 */
export function flowVisibleBoundaryPositions(
	minimumByRawPosition: ReadonlyMap<number, FlowPlaybackGranularity>,
	selected: FlowPlaybackGranularity,
	rawExtent: number,
): number[] {
	const positions = new Set<number>([0]);
	for (const [position, minimum] of minimumByRawPosition) {
		if (
			Number.isSafeInteger(position) &&
			position > 0 &&
			position < rawExtent &&
			flowTraceBoundaryVisible(minimum, selected)
		) {
			positions.add(position);
		}
	}
	if (rawExtent > 0) positions.add(rawExtent);
	return [...positions].sort((left, right) => left - right);
}

/** Finds the next semantic boundary without replaying already indexed Detail events. */
export function flowAdjacentVisibleBoundary(
	positions: readonly number[],
	cursor: number,
	direction: -1 | 1,
): number | undefined {
	if (direction === 1) {
		return positions.find((position) => position > cursor);
	}
	for (let index = positions.length - 1; index >= 0; index -= 1) {
		const position = positions[index];
		if (position !== undefined && position < cursor) return position;
	}
	return undefined;
}

/** Returns the greatest raw position whose complete prefix has been observed. */
export function flowKnownRawPrefixEnd(
	minimumByRawPosition: ReadonlyMap<number, FlowPlaybackGranularity>,
	rawExtent: number,
): number {
	let prefixEnd = 0;
	while (prefixEnd < rawExtent && minimumByRawPosition.has(prefixEnd + 1)) {
		prefixEnd += 1;
	}
	return prefixEnd;
}

/**
 * Keeps the solver trace canonical while deciding which committed boundaries a
 * selected playback mode exposes. Terminal and initial boundaries are always
 * visible so filtered playback cannot run past a result.
 */
export function flowSceneVisibleAtGranularity(
	scene: FlowCurrentSceneV9,
	selected: FlowPlaybackGranularity,
): boolean {
	if (scene.trace_event === undefined || scene.solve_status !== "running") {
		return true;
	}
	return flowTraceBoundaryVisible(
		scene.trace_event.minimum_granularity,
		selected,
	);
}
