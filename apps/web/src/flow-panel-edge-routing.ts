export type FlowPanelPoint = Readonly<{ x: number; y: number }>;

type FlowPanelEdge = Readonly<{ id: string; from: string; to: string }>;

export type FlowPanelEdgeRoute = Readonly<{
	d: string;
	label: FlowPanelPoint;
	labelAnchor: FlowPanelPoint;
	labelLeader?: Readonly<{ from: FlowPanelPoint; to: FlowPanelPoint }>;
	parallelIndex: number;
	parallelCount: number;
}>;

type RouteOptions = Readonly<{
	width: number;
	height: number;
	paddingX: number;
	paddingY: number;
	laneSpacing?: number;
	nodeRadius?: number;
	markerClearance?: number;
	labelWidth?: number;
	labelHeight?: number;
	labelEdgeIds?: readonly string[];
}>;

function unorderedPairKey(edge: FlowPanelEdge): string {
	return edge.from < edge.to
		? `${edge.from}\u0000${edge.to}`
		: `${edge.to}\u0000${edge.from}`;
}

function compareText(left: string, right: string): number {
	return left < right ? -1 : left > right ? 1 : 0;
}

function clampPoint(
	point: FlowPanelPoint,
	options: RouteOptions,
): FlowPanelPoint {
	return {
		x: Math.min(
			options.width - options.paddingX,
			Math.max(options.paddingX, point.x),
		),
		y: Math.min(
			options.height - options.paddingY,
			Math.max(options.paddingY, point.y),
		),
	};
}

function quadraticRoute(
	from: FlowPanelPoint,
	to: FlowPanelPoint,
	lane: number,
	options: RouteOptions,
): Pick<FlowPanelEdgeRoute, "d" | "label" | "labelAnchor"> {
	const dx = to.x - from.x;
	const dy = to.y - from.y;
	const length = Math.max(1, Math.hypot(dx, dy));
	const tangent = { x: dx / length, y: dy / length };
	const nodeRadius = options.nodeRadius ?? 30;
	const markerClearance = options.markerClearance ?? 10;
	const start = {
		x: from.x + tangent.x * (nodeRadius + markerClearance),
		y: from.y + tangent.y * (nodeRadius + markerClearance),
	};
	const end = {
		x: to.x - tangent.x * (nodeRadius + markerClearance),
		y: to.y - tangent.y * (nodeRadius + markerClearance),
	};
	const control = clampPoint(
		{
			x: (start.x + end.x) / 2 - tangent.y * lane,
			y: (start.y + end.y) / 2 + tangent.x * lane,
		},
		options,
	);
	const labelAnchor = clampPoint(
		{
			x: 0.25 * start.x + 0.5 * control.x + 0.25 * end.x,
			y: 0.25 * start.y + 0.5 * control.y + 0.25 * end.y,
		},
		options,
	);
	return {
		d: `M ${start.x} ${start.y} Q ${control.x} ${control.y} ${end.x} ${end.y}`,
		label: labelAnchor,
		labelAnchor,
	};
}

function laneInterval(
	from: FlowPanelPoint,
	to: FlowPanelPoint,
	options: RouteOptions,
): readonly [number, number] {
	const dx = to.x - from.x;
	const dy = to.y - from.y;
	const length = Math.max(1, Math.hypot(dx, dy));
	const midpoint = { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 };
	const normal = { x: -dy / length, y: dx / length };
	let minimum = Number.NEGATIVE_INFINITY;
	let maximum = Number.POSITIVE_INFINITY;
	for (const [base, coefficient, lower, upper] of [
		[midpoint.x, normal.x, options.paddingX, options.width - options.paddingX],
		[midpoint.y, normal.y, options.paddingY, options.height - options.paddingY],
	] as const) {
		if (Math.abs(coefficient) < 1e-9) continue;
		const first = (lower - base) / coefficient;
		const second = (upper - base) / coefficient;
		minimum = Math.max(minimum, Math.min(first, second));
		maximum = Math.min(maximum, Math.max(first, second));
	}
	return [minimum, maximum];
}

function fitLaneOffsets(
	desired: readonly number[],
	minimum: number,
	maximum: number,
): number[] {
	if (desired.length === 0) return [];
	const desiredMinimum = Math.min(...desired);
	const desiredMaximum = Math.max(...desired);
	if (desiredMinimum >= minimum && desiredMaximum <= maximum)
		return [...desired];
	if (desired.length === 1 || desiredMaximum === desiredMinimum) {
		return [Math.min(maximum, Math.max(minimum, desired[0] ?? 0))];
	}
	const desiredSpan = desiredMaximum - desiredMinimum;
	const availableSpan = Math.max(0, maximum - minimum);
	if (desiredSpan > availableSpan) {
		return desired.map(
			(value) =>
				minimum + ((value - desiredMinimum) / desiredSpan) * availableSpan,
		);
	}
	const shift =
		desiredMinimum < minimum
			? minimum - desiredMinimum
			: desiredMaximum > maximum
				? maximum - desiredMaximum
				: 0;
	return desired.map((value) => value + shift);
}

function loopRoute(
	center: FlowPanelPoint,
	index: number,
	count: number,
	options: RouteOptions,
): Pick<FlowPanelEdgeRoute, "d" | "label" | "labelAnchor"> {
	const viewCenter = { x: options.width / 2, y: options.height / 2 };
	const dx = viewCenter.x - center.x;
	const dy = viewCenter.y - center.y;
	const length = Math.max(1, Math.hypot(dx, dy));
	const baseAngle = Math.atan2(dy / length, dx / length);
	const ring = Math.floor(index / 3);
	const ringCount = Math.max(1, Math.ceil(count / 3));
	const progress = ringCount === 1 ? 0 : ring / (ringCount - 1);
	const angle = baseAngle + ((index % 3) - 1) * 0.34;
	const direction = { x: Math.cos(angle), y: Math.sin(angle) };
	const side = { x: -direction.y, y: direction.x };
	const radial = 54 + progress * 150;
	const lateral = 34 + progress * 76;
	const nodeRadius = options.nodeRadius ?? 30;
	const markerClearance = options.markerClearance ?? 10;
	const startAngle = angle - 0.5;
	const endAngle = angle + 0.5;
	const start = {
		x: center.x + Math.cos(startAngle) * nodeRadius,
		y: center.y + Math.sin(startAngle) * nodeRadius,
	};
	const end = {
		x: center.x + Math.cos(endAngle) * (nodeRadius + markerClearance),
		y: center.y + Math.sin(endAngle) * (nodeRadius + markerClearance),
	};
	const first = clampPoint(
		{
			x: center.x + direction.x * radial + side.x * lateral,
			y: center.y + direction.y * radial + side.y * lateral,
		},
		options,
	);
	const second = clampPoint(
		{
			x: center.x + direction.x * radial - side.x * lateral,
			y: center.y + direction.y * radial - side.y * lateral,
		},
		options,
	);
	const labelAnchor = clampPoint(
		{
			x: (first.x + second.x) / 2,
			y: (first.y + second.y) / 2,
		},
		options,
	);
	return {
		d: `M ${start.x} ${start.y} C ${first.x} ${first.y} ${second.x} ${second.y} ${end.x} ${end.y}`,
		label: labelAnchor,
		labelAnchor,
	};
}

type LabelBox = Readonly<{
	left: number;
	right: number;
	top: number;
	bottom: number;
}>;

function overlaps(left: LabelBox, right: LabelBox): boolean {
	return !(
		left.right + 4 <= right.left ||
		right.right + 4 <= left.left ||
		left.bottom + 4 <= right.top ||
		right.bottom + 4 <= left.top
	);
}

function labelBox(
	center: FlowPanelPoint,
	width: number,
	height: number,
): LabelBox {
	return {
		left: center.x - width / 2,
		right: center.x + width / 2,
		top: center.y - height / 2,
		bottom: center.y + height / 2,
	};
}

function labelCandidates(
	anchor: FlowPanelPoint,
	width: number,
	height: number,
	options: RouteOptions,
): FlowPanelPoint[] {
	const candidates: FlowPanelPoint[] = [];
	for (let ring = 0; ring <= 7; ring += 1) {
		for (let row = -ring; row <= ring; row += 1) {
			for (let column = -ring; column <= ring; column += 1) {
				if (Math.max(Math.abs(row), Math.abs(column)) !== ring) continue;
				const point = {
					x: anchor.x + column * (width + 10),
					y: anchor.y + row * (height + 8),
				};
				candidates.push({
					x: Math.min(
						options.width - options.paddingX - width / 2,
						Math.max(options.paddingX + width / 2, point.x),
					),
					y: Math.min(
						options.height - options.paddingY - height / 2,
						Math.max(options.paddingY + height / 2, point.y),
					),
				});
			}
		}
	}
	const globalCandidates: FlowPanelPoint[] = [];
	const minimumX = options.paddingX + width / 2;
	const maximumX = options.width - options.paddingX - width / 2;
	const minimumY = options.paddingY + height / 2;
	const maximumY = options.height - options.paddingY - height / 2;
	for (let y = minimumY; y <= maximumY; y += height + 8) {
		for (let x = minimumX; x <= maximumX; x += width + 10) {
			globalCandidates.push({ x, y });
		}
	}
	globalCandidates.sort((left, right) => {
		const leftDistance = (left.x - anchor.x) ** 2 + (left.y - anchor.y) ** 2;
		const rightDistance = (right.x - anchor.x) ** 2 + (right.y - anchor.y) ** 2;
		return leftDistance - rightDistance || left.y - right.y || left.x - right.x;
	});
	const denseCandidates: FlowPanelPoint[] = [];
	const denseStep = Math.max(8, Math.floor(Math.min(width, height) / 3));
	for (let y = minimumY; y <= maximumY; y += denseStep) {
		for (let x = minimumX; x <= maximumX; x += denseStep) {
			denseCandidates.push({ x, y });
		}
	}
	denseCandidates.sort((left, right) => {
		const leftDistance = (left.x - anchor.x) ** 2 + (left.y - anchor.y) ** 2;
		const rightDistance = (right.x - anchor.x) ** 2 + (right.y - anchor.y) ** 2;
		return leftDistance - rightDistance || left.y - right.y || left.x - right.x;
	});
	const seen = new Set<string>();
	return [...candidates, ...globalCandidates, ...denseCandidates].filter(
		(candidate) => {
			const key = `${candidate.x}\u0000${candidate.y}`;
			if (seen.has(key)) return false;
			seen.add(key);
			return true;
		},
	);
}

function leaderToLabel(
	anchor: FlowPanelPoint,
	label: FlowPanelPoint,
	width: number,
	height: number,
): FlowPanelEdgeRoute["labelLeader"] {
	const dx = anchor.x - label.x;
	const dy = anchor.y - label.y;
	const normalizedDistance = Math.max(
		Math.abs(dx) / (width / 2),
		Math.abs(dy) / (height / 2),
	);
	// An anchor inside the label has no visible ownership gap to bridge. Drawing a
	// leader there only puts a short, ambiguous stroke underneath the text.
	if (normalizedDistance <= 1) return undefined;
	const scale = 1 / normalizedDistance;
	const to = { x: label.x + dx * scale, y: label.y + dy * scale };
	if (Math.hypot(anchor.x - to.x, anchor.y - to.y) < 4) return undefined;
	return {
		from: anchor,
		to,
	};
}

function placeRouteLabels(
	routes: ReadonlyMap<string, FlowPanelEdgeRoute>,
	positions: ReadonlyMap<string, FlowPanelPoint>,
	options: RouteOptions,
): ReadonlyMap<string, FlowPanelEdgeRoute> {
	const width = options.labelWidth ?? 140;
	const height = options.labelHeight ?? 40;
	const nodeRadius = (options.nodeRadius ?? 30) + 8;
	const nodeBoxes = [...positions.values()].map((point) =>
		labelBox(point, nodeRadius * 2, nodeRadius * 2),
	);
	const visibleLabelIds =
		options.labelEdgeIds === undefined
			? undefined
			: new Set(options.labelEdgeIds);
	const byId = [...routes]
		.filter(([id]) => visibleLabelIds === undefined || visibleLabelIds.has(id))
		.sort(([left], [right]) => compareText(left, right));
	const complete = (
		placed: ReadonlyMap<string, FlowPanelEdgeRoute>,
	): ReadonlyMap<string, FlowPanelEdgeRoute> => {
		if (visibleLabelIds === undefined) {
			return new Map(
				[...routes].map(([id, route]) => [id, placed.get(id) ?? route]),
			);
		}
		const occupied = [
			...nodeBoxes,
			...[...placed.values()].map((route) =>
				labelBox(route.label, width, height),
			),
		];
		return new Map(
			[...routes].map(([id, route]) => {
				const visible = placed.get(id);
				if (visible !== undefined) return [id, visible];
				const label = labelCandidates(
					route.labelAnchor,
					width,
					height,
					options,
				).find((candidate) => {
					const candidateBox = labelBox(candidate, width, height);
					return occupied.every((box) => !overlaps(candidateBox, box));
				});
				if (label === undefined) return [id, route];
				const labelLeader = leaderToLabel(
					route.labelAnchor,
					label,
					width,
					height,
				);
				return [
					id,
					{
						...route,
						label,
						...(labelLeader === undefined ? {} : { labelLeader }),
					},
				];
			}),
		);
	};
	const tryOrder = (
		ordered: readonly (readonly [string, FlowPanelEdgeRoute])[],
	): ReadonlyMap<string, FlowPanelEdgeRoute> | undefined => {
		const occupied = [...nodeBoxes];
		const placed = new Map<string, FlowPanelEdgeRoute>();
		for (const [id, route] of ordered) {
			const label = labelCandidates(
				route.labelAnchor,
				width,
				height,
				options,
			).find((candidate) => {
				const candidateBox = labelBox(candidate, width, height);
				return occupied.every((box) => !overlaps(candidateBox, box));
			});
			if (label === undefined) return undefined;
			occupied.push(labelBox(label, width, height));
			const labelLeader = leaderToLabel(
				route.labelAnchor,
				label,
				width,
				height,
			);
			placed.set(id, {
				...route,
				label,
				...(labelLeader === undefined ? {} : { labelLeader }),
			});
		}
		return placed;
	};
	const coordinateOrder = (
		axis: "x" | "y",
		direction: 1 | -1,
	): (readonly [string, FlowPanelEdgeRoute])[] =>
		[...byId].sort(
			([leftId, left], [rightId, right]) =>
				direction * (left.labelAnchor[axis] - right.labelAnchor[axis]) ||
				compareText(leftId, rightId),
		);
	for (const order of [
		byId,
		[...byId].reverse(),
		coordinateOrder("x", 1),
		coordinateOrder("x", -1),
		coordinateOrder("y", 1),
		coordinateOrder("y", -1),
	]) {
		const placed = tryOrder(order);
		if (placed !== undefined) return complete(placed);
	}
	return complete(
		new Map(
			byId.map(([id, route]) => [
				id,
				{
					...route,
					label: route.labelAnchor,
				},
			]),
		),
	);
}

/** Builds stable, direction-aware routes shared by algorithm-state panels. */
export function buildFlowPanelEdgeRoutes(
	edges: readonly FlowPanelEdge[],
	positions: ReadonlyMap<string, FlowPanelPoint>,
	options: RouteOptions,
): ReadonlyMap<string, FlowPanelEdgeRoute> {
	const groups = new Map<string, FlowPanelEdge[]>();
	for (const edge of edges) {
		const key = unorderedPairKey(edge);
		const group = groups.get(key);
		if (group === undefined) groups.set(key, [edge]);
		else group.push(edge);
	}
	const routes = new Map<string, FlowPanelEdgeRoute>();
	const spacing = options.laneSpacing ?? 34;
	for (const [key, unordered] of groups) {
		const group = [...unordered].sort((left, right) =>
			compareText(left.id, right.id),
		);
		if (group[0]?.from === group[0]?.to) {
			for (const [index, edge] of group.entries()) {
				const center = positions.get(edge.from);
				if (center === undefined) continue;
				routes.set(edge.id, {
					...loopRoute(center, index, group.length, options),
					parallelIndex: index + 1,
					parallelCount: group.length,
				});
			}
			continue;
		}
		const [low = "", high = ""] = key.split("\u0000");
		const forward = group.filter(
			(edge) => edge.from === low && edge.to === high,
		);
		const reverse = group.filter(
			(edge) => edge.from === high && edge.to === low,
		);
		const hasOpposite = forward.length > 0 && reverse.length > 0;
		const lowPosition = positions.get(low);
		const highPosition = positions.get(high);
		if (lowPosition === undefined || highPosition === undefined) continue;
		const desiredPhysicalOffsets = [
			...forward.map((_, index) =>
				hasOpposite
					? (index + 0.7) * spacing
					: (index - (forward.length - 1) / 2) * spacing,
			),
			...reverse.map((_, index) =>
				hasOpposite
					? -(index + 0.7) * spacing
					: (index - (reverse.length - 1) / 2) * spacing,
			),
		];
		const [minimumLane, maximumLane] = laneInterval(
			lowPosition,
			highPosition,
			options,
		);
		const physicalOffsets = fitLaneOffsets(
			desiredPhysicalOffsets,
			minimumLane,
			maximumLane,
		);
		let offsetIndex = 0;
		for (const [directionIndex, directionGroup] of [
			forward,
			reverse,
		].entries()) {
			for (const [index, edge] of directionGroup.entries()) {
				const from = positions.get(edge.from);
				const to = positions.get(edge.to);
				if (from === undefined || to === undefined) continue;
				const physicalOffset = physicalOffsets[offsetIndex] ?? 0;
				offsetIndex += 1;
				const lane = directionIndex === 0 ? physicalOffset : -physicalOffset;
				routes.set(edge.id, {
					...quadraticRoute(from, to, lane, options),
					parallelIndex: index + 1,
					parallelCount: directionGroup.length,
				});
			}
		}
	}
	return placeRouteLabels(routes, positions, options);
}
