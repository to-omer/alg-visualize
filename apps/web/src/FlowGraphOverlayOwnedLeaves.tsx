import {
	Children,
	cloneElement,
	Fragment,
	isValidElement,
	type ReactElement,
	type ReactNode,
} from "react";

import type { projectFlowEntityGraphState } from "./flow-entity-graph-state";
import {
	FLOW_OVERLAY_CONTRIBUTIONS,
	type FlowOverlayFeatureBundleKey,
} from "./flow-overlay-contribution-registry";
import type { FlowSceneV9OverlayField } from "./flow-scene-wire/generated/overlays";

type FlowEntityGraphState = ReturnType<typeof projectFlowEntityGraphState>;

const PAINTED_SVG_LEAF_NAMES = new Set([
	"circle",
	"ellipse",
	"line",
	"path",
	"polygon",
	"polyline",
	"rect",
	"text",
]);

export type FlowOverlayLeafOwner = Readonly<{
	overlay: FlowSceneV9OverlayField;
	/** Source-owned field or semantic role represented by the painted leaf. */
	role: string;
}>;

export type FlowOverlayLeafEntity = Readonly<{
	kind:
		| "node"
		| "edge"
		| "residual-arc"
		| "auxiliary-node"
		| "auxiliary-edge"
		| "auxiliary-residual-arc";
	id: string;
	direction?: "forward" | "reverse" | undefined;
}>;

type IntrinsicElement = ReactElement<
	Readonly<Record<string, unknown>> & Readonly<{ children?: ReactNode }>,
	string | typeof Fragment
>;

function annotatePaintedSvgLeaves(
	children: ReactNode,
	attributes: Readonly<Record<string, string | undefined>>,
): ReactNode {
	return Children.map(children, (child) => {
		if (!isValidElement(child)) return child;
		if (child.type !== Fragment && typeof child.type !== "string") {
			throw new Error(
				"FlowGraphOverlayOwnedLeaves must wrap intrinsic SVG content, not a component",
			);
		}
		const element = child as IntrinsicElement;
		const nested = annotatePaintedSvgLeaves(element.props.children, attributes);
		const isPaintedLeaf =
			typeof element.type === "string" &&
			PAINTED_SVG_LEAF_NAMES.has(element.type);
		return cloneElement(
			element,
			isPaintedLeaf ? attributes : undefined,
			nested,
		);
	});
}

/**
 * Attaches exact source ownership to the feature-specific SVG leaves that are
 * already being painted. It deliberately draws no fallback mark: if the
 * algorithm-specific renderer emits no leaf, the visual audit must fail.
 */
export function FlowGraphOverlayOwnedLeaves({
	state,
	bundle,
	entity,
	owners,
	children,
}: Readonly<{
	state: FlowEntityGraphState;
	bundle: FlowOverlayFeatureBundleKey;
	entity: FlowOverlayLeafEntity;
	owners: readonly FlowOverlayLeafOwner[];
	children: ReactNode;
}>) {
	const uniqueOwners = owners.filter(
		(owner, index) =>
			owners.findIndex(
				(candidate) =>
					candidate.overlay === owner.overlay && candidate.role === owner.role,
			) === index,
	);
	if (uniqueOwners.length === 0) {
		throw new Error("An overlay-owned SVG feature has no source owner");
	}
	for (const owner of uniqueOwners) {
		if (!state.plan.overlayPresentation.activeFields.includes(owner.overlay)) {
			throw new Error(
				`Inactive overlay ${owner.overlay} attempted to publish a graph leaf`,
			);
		}
		if (
			!FLOW_OVERLAY_CONTRIBUTIONS[owner.overlay].featureBundles.includes(bundle)
		) {
			throw new Error(
				`Overlay ${owner.overlay} does not declare feature bundle ${bundle}`,
			);
		}
	}
	if (
		(entity.kind === "residual-arc" ||
			entity.kind === "auxiliary-residual-arc") &&
		entity.direction !== "forward" &&
		entity.direction !== "reverse"
	) {
		throw new Error("A residual-arc leaf must publish its direction");
	}
	const overlays = [...new Set(uniqueOwners.map(({ overlay }) => overlay))];
	const roleBindings = uniqueOwners.map(
		({ overlay, role }) => `${overlay}:${role}`,
	);
	return annotatePaintedSvgLeaves(children, {
		"data-overlay-contribution":
			overlays.length === 1 ? overlays[0] : undefined,
		"data-overlay-contributions":
			overlays.length > 1 ? overlays.join(" ") : undefined,
		"data-overlay-feature-bundle": bundle,
		"data-overlay-entity-kind": entity.kind,
		"data-overlay-entity-id": entity.id,
		"data-overlay-residual-direction": entity.direction,
		"data-overlay-role": roleBindings.join("|"),
	});
}
