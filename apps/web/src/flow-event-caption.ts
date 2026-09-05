import type { FlowCurrentSceneV9, FlowTraceEventV1 } from "./flow-scene";

function referencedResidualArc(
	scene: FlowCurrentSceneV9,
	event: FlowTraceEventV1,
) {
	const active = scene.residual_arcs.find((arc) => arc.active);
	if (active !== undefined) return active;
	const reference = event.entity_refs.find(
		(entity) => entity.kind === "residual-arc",
	);
	if (reference?.kind === "residual-arc") {
		return scene.residual_arcs.find(
			(arc) =>
				arc.edge_id === reference.edge_id &&
				arc.direction === reference.direction,
		);
	}
	return undefined;
}

function eventResidualArc(scene: FlowCurrentSceneV9, event: FlowTraceEventV1) {
	const reference = event.entity_refs.find(
		(entity) => entity.kind === "residual-arc" || entity.kind === "edge",
	);
	if (reference?.kind === "residual-arc") {
		return scene.residual_arcs.find(
			(arc) =>
				arc.edge_id === reference.edge_id &&
				arc.direction === reference.direction,
		);
	}
	if (reference?.kind === "edge") {
		return scene.residual_arcs.find((arc) => arc.edge_id === reference.edge_id);
	}
	return undefined;
}

function referencedNode(event: FlowTraceEventV1): string | undefined {
	const reference = event.entity_refs.find((entity) => entity.kind === "node");
	return reference?.kind === "node" ? reference.node_id : undefined;
}

function detailValue(event: FlowTraceEventV1): string | undefined {
	return event.detail?.value;
}

function humanize(value: string): string {
	const words = value.replaceAll("-", " ");
	return `${words.charAt(0).toUpperCase()}${words.slice(1)}`;
}

function fallbackCaption(
	scene: FlowCurrentSceneV9,
	event: FlowTraceEventV1,
): string {
	const action = event.catalog_id.split(".").at(-1) ?? event.catalog_id;
	const words = action.replaceAll("-", " ");
	const role = scene.trace_event_semantics?.role;
	const prefix =
		role === "observe"
			? "Observe"
			: role === "select"
				? "Select"
				: role === "mutate"
					? "Update"
					: role === "commit"
						? "Commit"
						: role === "certify"
							? "Certify"
							: undefined;
	const actionLabel = `${words.charAt(0).toUpperCase()}${words.slice(1)}`;
	const caption =
		prefix === undefined ? actionLabel : `${prefix} · ${actionLabel}`;
	if (event.detail === undefined) return caption;
	return `${caption} · ${humanize(event.detail.label)} = ${event.detail.value}`;
}

function feasibilityNodeLabel(
	node:
		| NonNullable<FlowCurrentSceneV9["feasibility_overlay"]>["focus_node"]
		| undefined,
): string | undefined {
	if (node === undefined) return undefined;
	if (node.kind === "super-source") return "SS";
	if (node.kind === "super-sink") return "ST";
	return node.original_node_id;
}

function feasibilityFocus(
	scene: FlowCurrentSceneV9,
): Readonly<{ from: string; to: string }> | undefined {
	const overlay = scene.feasibility_overlay;
	const focused = overlay?.arcs.find((arc) => arc.focused);
	if (focused === undefined) return undefined;
	const from = feasibilityNodeLabel(focused.from);
	const to = feasibilityNodeLabel(focused.to);
	if (from === undefined || to === undefined) return undefined;
	return focused.focused_direction === "reverse"
		? { from: to, to: from }
		: { from, to };
}

function feasibilityCaption(
	scene: FlowCurrentSceneV9,
	event: FlowTraceEventV1,
): string {
	const overlay = scene.feasibility_overlay;
	const stage = event.catalog_id.slice("feasibility.".length);
	const node = feasibilityNodeLabel(overlay?.focus_node);
	const focus = feasibilityFocus(scene);
	const arc = focus === undefined ? undefined : `${focus.from} → ${focus.to}`;
	const value = detailValue(event);
	const routed = overlay === undefined ? undefined : overlay.routed;
	const required = overlay === undefined ? undefined : overlay.total_required;
	switch (stage) {
		case "add-original-arc":
			return arc === undefined
				? `Shift one lower-bounded edge${value === undefined ? "" : ` · residual capacity ${value}`}`
				: `Shift lower bound on ${arc}${value === undefined ? "" : ` · residual capacity ${value}`}`;
		case "add-return-arc":
			return `Add temporary return arc${arc === undefined ? "" : ` ${arc}`}${value === undefined ? "" : ` · capacity ${value}`}`;
		case "inspect-node-imbalance":
			return `Inspect shifted balance${node === undefined ? "" : ` at ${node}`}`;
		case "add-imbalance-arc":
			return `Connect imbalance arc${arc === undefined ? "" : ` ${arc}`}${value === undefined ? "" : ` · capacity ${value}`}`;
		case "initialize-source-height":
			return `Initialize super source${value === undefined ? "" : ` · height ${value}`}`;
		case "inspect-source-arc":
			return `Inspect super-source residual arc${arc === undefined ? "" : ` ${arc}`}${value === undefined ? "" : ` · residual capacity ${value}`}`;
		case "activate-node":
			return `Enqueue active node${node === undefined ? "" : ` ${node}`}${value === undefined ? "" : ` · excess ${value}`}`;
		case "select-active-node":
			return `Dequeue active node${node === undefined ? "" : ` ${node}`}${value === undefined ? "" : ` · excess ${value}`}`;
		case "inspect-discharge-arc":
			return `Inspect current residual arc${arc === undefined ? "" : ` ${arc}`}${value === undefined ? "" : ` · residual capacity ${value}`}`;
		case "inspect-relabel-arc":
			return `Inspect relabel candidate${arc === undefined ? "" : ` ${arc}`}${value === undefined ? "" : ` · residual capacity ${value}`}`;
		case "push":
			return `Push${value === undefined ? "" : ` ${value}`} on auxiliary residual arc${arc === undefined ? "" : ` ${arc}`}`;
		case "advance-current-arc":
			return `Advance current arc${node === undefined ? "" : ` at ${node}`}${value === undefined ? "" : ` · index ${value}`}`;
		case "relabel":
			return `Relabel${node === undefined ? "" : ` ${node}`}${value === undefined ? "" : ` · height ${value}`}`;
		case "complete-discharge":
			return `Finish discharge${node === undefined ? "" : ` of ${node}`}${value === undefined ? "" : ` · excess ${value}`}`;
		case "complete-routing":
			return `Auxiliary routing complete${routed === undefined || required === undefined ? "" : ` · ${routed} / ${required}`}`;
		case "inspect-cut-arc":
			return `Inspect cut-search residual arc${arc === undefined ? "" : ` ${arc}`}${value === undefined ? "" : ` · residual capacity ${value}`}`;
		case "mark-reachable":
			return `Mark cut-reachable node${node === undefined ? "" : ` ${node}`}`;
		case "extract-original-flow":
			return `Extract original flow${arc === undefined ? "" : ` on ${arc}`}${value === undefined ? "" : ` · flow ${value}`}`;
		case "feasible":
			return `Feasibility certified${routed === undefined || required === undefined ? "" : ` · routed ${routed} / ${required}`}`;
		case "infeasible":
			return `Infeasible cut certified${value === undefined ? "" : ` · ${value} units unsatisfied`}`;
		default:
			return fallbackCaption(scene, event);
	}
}

/** Converts revision-owned trace IDs into one concise educational action. */
export function flowEventCaption(scene: FlowCurrentSceneV9): string {
	const event = scene.trace_event;
	if (event === undefined) return "Input boundary";
	if (event.catalog_id.startsWith("feasibility.")) {
		return feasibilityCaption(scene, event);
	}
	const action = event.catalog_id.split(".").at(-1);
	const arc = referencedResidualArc(scene, event);
	const arcLabel = arc === undefined ? undefined : `${arc.from} → ${arc.to}`;
	const value = detailValue(event);
	switch (action) {
		case "select-source":
			return `Select surplus node ${referencedNode(event) ?? ""}`.trimEnd();
		case "bfs":
			return "Start breadth-first search";
		case "discover":
		case "inspect-residual-arc":
			return arcLabel === undefined
				? "Inspect a residual edge"
				: `Inspect residual edge ${arcLabel}`;
		case "build-reverse-zero-one-adjacency":
			return arcLabel === undefined
				? "Index one reverse 0–1 residual arc"
				: `Index reverse 0–1 arc ${arcLabel}`;
		case "relax-binary-distance":
			return arcLabel === undefined
				? "Relax one binary-distance arc"
				: `Relax binary distance on ${arcLabel}`;
		case "inspect-binary-length":
			return arcLabel === undefined
				? "Classify one residual arc as length 0 or 1"
				: `Classify binary length on ${arcLabel}`;
		case "inspect-initial-cut-arc":
			return arcLabel === undefined
				? "Measure one residual arc in the initial source cut"
				: `Measure initial source-cut arc ${arcLabel}`;
		case "build-zero-scc-adjacency":
			return arcLabel === undefined
				? "Add one zero-length arc to the SCC graph"
				: `Add zero-length SCC arc ${arcLabel}`;
		case "inspect-zero-scc-reverse-arc":
			return arcLabel === undefined
				? "Traverse one reverse SCC arc"
				: `Traverse reverse SCC arc ${arcLabel}`;
		case "inspect-canonical-cut-arc":
			return arcLabel === undefined
				? "Measure one arc against the current distance cut"
				: `Measure cut crossing at ${arcLabel}`;
		case "inspect-contracted-arc":
			return arcLabel === undefined
				? "Inspect one contracted admissible arc"
				: `Inspect contracted arc ${arcLabel}`;
		case "build-lift-adjacency":
			return arcLabel === undefined
				? "Index one internal SCC lift arc"
				: `Index lift arc ${arcLabel}`;
		case "inspect-lift-arc":
			return arcLabel === undefined
				? "Search one internal SCC lift arc"
				: `Search lift arc ${arcLabel}`;
		case "apply-contracted-flow":
			return value === undefined
				? "Apply the contracted flow to original residual arcs"
				: `Apply contracted flow · ${value} units`;
		case "apply-lift-path":
			return value === undefined
				? "Route one component balance along its lift path"
				: `Lift component flow · ${value} units`;
		case "inspect-primitive-arc-checkpoint": {
			const inspected = eventResidualArc(scene, event);
			return inspected === undefined
				? "Inspect an augmented residual edge"
				: `Inspect residual edge ${inspected.from} → ${inspected.to}`;
		}
		case "completion-inspect-primitive-arc-checkpoint": {
			const inspected = eventResidualArc(scene, event);
			return inspected === undefined
				? "Inspect an exact residual edge"
				: `Inspect exact residual edge ${inspected.from} → ${inspected.to}`;
		}
		case "relabel-checkpoint":
			return referencedNode(event) === undefined
				? "Relabel one weighted vertex"
				: `Relabel vertex ${referencedNode(event)}`;
		case "completion-relabel-checkpoint":
			return referencedNode(event) === undefined
				? "Relabel one exact-flow vertex"
				: `Relabel exact-flow vertex ${referencedNode(event)}`;
		case "completion-augment-path":
			return value === undefined
				? "Augment one exact residual path"
				: `Augment exact residual path · bottleneck ${value}`;
		case "completion-residual-round":
			return value === undefined
				? "Finish one exact residual round"
				: `Finish exact residual round ${value}`;
		case "measure-short-flow":
			return value === undefined
				? "Measure the routed short flow"
				: `Measure short flow · ${value} units routed`;
		case "compute-distance-layers":
			return value === undefined
				? "Build residual distance layers"
				: `Build residual distance layers · ${value} arc scans`;
		case "select-sparse-cut":
			return value === undefined
				? "Select a sparse cut"
				: `Select sparse cut · capacity ${value}`;
		case "complete-residual-rounds":
			return value === undefined
				? "Complete the exact residual flow"
				: `Complete exact residual flow · ${value} rounds`;
		case "check-certificate":
			return "Verify the max-flow/min-cut certificate";
		case "relax":
			return arcLabel === undefined
				? "Relax a residual edge"
				: `Relax residual edge ${arcLabel}`;
		case "bfs-complete":
			return value === undefined
				? "Breadth-first search complete"
				: `Search complete · ${value} nodes reached`;
		case "shortest-path":
			return value === undefined
				? "Shortest-path search complete"
				: `Shortest-path search complete · ${value} nodes reached`;
		case "reconstruct-path":
			if (value === undefined) return "Build the augmenting path";
			return `Build path prefix · ${value} edge${value === "1" ? "" : "s"}`;
		case "bottleneck":
			return value === undefined
				? "Measure the bottleneck"
				: `Bottleneck = ${value}`;
		case "augment":
			return value === undefined
				? "Commit the flow update"
				: `Commit +${value} flow`;
		case "update-potentials":
			return "Update feasible node potentials";
		case "optimal":
			return "Optimality certificate verified";
		default:
			return fallbackCaption(scene, event);
	}
}
