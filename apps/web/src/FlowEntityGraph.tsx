import {
	type CSSProperties,
	type MouseEvent as ReactMouseEvent,
	type PointerEvent as ReactPointerEvent,
	useEffect,
	useMemo,
	useState,
} from "react";
import { FlowGraphEntityLayerComposer } from "./FlowGraphEntityLayerComposer";
import {
	FlowGraphIdScopeProvider,
	flowScopedDomId,
	flowScopedSvgUrl,
	useFlowDomIdScope,
	useFlowGraphIdScope,
} from "./flow-dom-id";
import {
	type FlowEntityGraphPlan,
	projectFlowEntityGraphState,
} from "./flow-entity-graph-state";
import type { FlowEntitySelection } from "./flow-entity-navigator";
import {
	flowGraphAccessibleDescription,
	flowGraphSceneClassName,
	flowGraphSceneDataAttributes,
} from "./flow-graph-scene-metadata";
import type { FlowCanvasSvgBinding } from "./use-flow-canvas-viewport";

export { FlowRmfgenFrameGroups as RmfgenFrameGroups } from "./FlowRmfgenFrameGroups";
export type {
	FlowEntityGraphPlan,
	RmfgenFrameGroup,
} from "./flow-entity-graph-state";

export function FlowGraphMarkers() {
	const idScope = useFlowGraphIdScope();
	const scopedId = (localId: string) => flowScopedDomId(idScope, localId);
	return (
		<defs>
			{(["capacity", "flow", "focus"] as const).map((role) => (
				<marker
					key={`feasibility-${role}`}
					id={scopedId(`flow-arrow-feasibility-${role}`)}
					markerWidth="10"
					markerHeight="10"
					markerUnits="userSpaceOnUse"
					refX="8"
					refY="5"
					orient="auto"
					viewBox="0 0 10 10"
					overflow="visible"
				>
					<path
						d="M0,0 L10,5 L0,10 Z"
						className={`flow-arrow-feasibility-${role}`}
					/>
				</marker>
			))}
			<marker
				id={scopedId("flow-arrow-capacity")}
				markerWidth="12"
				markerHeight="12"
				refX="18"
				refY="6"
				orient="auto"
				markerUnits="userSpaceOnUse"
				viewBox="0 0 12 12"
				overflow="visible"
			>
				<path d="M0,0 L12,6 L0,12 Z" className="flow-arrow-capacity" />
			</marker>
			<marker
				id={scopedId("flow-arrow-context")}
				markerWidth="6"
				markerHeight="6"
				refX="8"
				refY="3"
				orient="auto"
				markerUnits="userSpaceOnUse"
				viewBox="0 0 6 6"
				overflow="visible"
			>
				<path d="M0,0 L6,3 L0,6 Z" className="flow-arrow-context" />
			</marker>
			<marker
				id={scopedId("flow-arrow-fill")}
				markerWidth="12"
				markerHeight="12"
				refX="18"
				refY="6"
				orient="auto"
				markerUnits="userSpaceOnUse"
				viewBox="0 0 12 12"
				overflow="visible"
			>
				<path d="M0,0 L12,6 L0,12 Z" className="flow-arrow-fill" />
			</marker>
			{(["positive", "negative", "zero", "mixed"] as const).map((kind) => (
				<marker
					key={kind}
					id={scopedId(`flow-arrow-${kind}`)}
					markerWidth="8"
					markerHeight="8"
					markerUnits="userSpaceOnUse"
					refX="6"
					refY="4"
					orient="auto"
				>
					<path d="M0,0 L8,4 L0,8 Z" className={`flow-arrow-${kind}`} />
				</marker>
			))}
			<marker
				id={scopedId("flow-arrow-residual")}
				markerWidth="8"
				markerHeight="8"
				markerUnits="userSpaceOnUse"
				refX="6"
				refY="4"
				orient="auto"
			>
				<path d="M0,0 L8,4 L0,8 Z" className="flow-arrow-residual" />
			</marker>
			<marker
				id={scopedId("flow-arrow-orlin-improvement")}
				markerWidth="10"
				markerHeight="10"
				markerUnits="userSpaceOnUse"
				refX="8"
				refY="5"
				orient="auto"
				viewBox="0 0 10 10"
				overflow="visible"
			>
				<path d="M0,0 L10,5 L0,10 Z" className="flow-arrow-orlin-improvement" />
			</marker>
			{(["positive", "negative"] as const).map((direction) => (
				<marker
					key={`electrical-${direction}`}
					id={scopedId(`flow-arrow-electrical-${direction}`)}
					markerWidth="9"
					markerHeight="9"
					markerUnits="userSpaceOnUse"
					refX="7"
					refY="4.5"
					orient="auto"
				>
					<path
						d="M0,0 L9,4.5 L0,9 Z"
						className={`flow-arrow-electrical-${direction}`}
					/>
				</marker>
			))}
			{(["forward", "reverse"] as const).map((direction) => (
				<marker
					key={`augmenting-electrical-${direction}`}
					id={scopedId(`flow-arrow-augmenting-electrical-${direction}`)}
					markerWidth="9"
					markerHeight="9"
					markerUnits="userSpaceOnUse"
					refX="7"
					refY="4.5"
					orient="auto"
				>
					<path
						d="M0,0 L9,4.5 L0,9 Z"
						className={`flow-arrow-augmenting-electrical-${direction}`}
					/>
				</marker>
			))}
			{(["central", "rounded"] as const).map((role) => (
				<marker
					key={`augmenting-electrical-${role}`}
					id={scopedId(`flow-arrow-augmenting-electrical-${role}`)}
					markerWidth="10"
					markerHeight="10"
					markerUnits="userSpaceOnUse"
					refX="8"
					refY="5"
					orient="auto"
					viewBox="0 0 10 10"
					overflow="visible"
				>
					<path
						d="M0,0 L10,5 L0,10 Z"
						className={`flow-arrow-augmenting-electrical-${role}`}
					/>
				</marker>
			))}
			{(["forward", "reverse"] as const).map((direction) => (
				<marker
					key={`interior-point-${direction}`}
					id={scopedId(`flow-arrow-interior-point-${direction}`)}
					markerWidth="9"
					markerHeight="9"
					markerUnits="userSpaceOnUse"
					refX="7"
					refY="4.5"
					orient="auto"
				>
					<path
						d="M0,0 L9,4.5 L0,9 Z"
						className={`flow-arrow-interior-point-${direction}`}
					/>
				</marker>
			))}
			{(["candidate", "selected"] as const).map((role) => (
				<marker
					key={`minimum-ratio-${role}`}
					id={scopedId(`flow-arrow-minimum-ratio-${role}`)}
					markerWidth="10"
					markerHeight="10"
					markerUnits="userSpaceOnUse"
					refX="8"
					refY="5"
					orient="auto"
				>
					<path
						d="M0,0 L10,5 L0,10 Z"
						className={`flow-arrow-minimum-ratio-${role}`}
					/>
				</marker>
			))}
			{(["cycle", "candidate-cycle", "return", "artificial"] as const).map(
				(role) => (
					<marker
						key={`randomized-almost-linear-${role}`}
						id={scopedId(`flow-arrow-randomized-almost-linear-${role}`)}
						markerWidth="10"
						markerHeight="10"
						markerUnits="userSpaceOnUse"
						refX="8"
						refY="5"
						orient="auto"
					>
						<path
							d="M0,0 L10,5 L0,10 Z"
							className={`flow-arrow-randomized-almost-linear-${role}`}
						/>
					</marker>
				),
			)}
			{(["cycle", "candidate-cycle", "return", "artificial"] as const).map(
				(role) => (
					<marker
						key={`deterministic-almost-linear-${role}`}
						id={scopedId(`flow-arrow-deterministic-almost-linear-${role}`)}
						markerWidth="10"
						markerHeight="10"
						markerUnits="userSpaceOnUse"
						refX="8"
						refY="5"
						orient="auto"
					>
						<path
							d="M0,0 L10,5 L0,10 Z"
							className={`flow-arrow-deterministic-almost-linear-${role}`}
						/>
					</marker>
				),
			)}
			<marker
				id={scopedId("flow-arrow-level-network")}
				markerWidth="11"
				markerHeight="11"
				markerUnits="userSpaceOnUse"
				refX="8.5"
				refY="5.5"
				orient="auto"
			>
				<path d="M0,0 L11,5.5 L0,11 Z" className="flow-arrow-level-network" />
			</marker>
			<marker
				id={scopedId("flow-arrow-residual-active")}
				markerWidth="8"
				markerHeight="8"
				markerUnits="userSpaceOnUse"
				refX="6"
				refY="4"
				orient="auto"
			>
				<path d="M0,0 L8,4 L0,8 Z" className="flow-arrow-residual-active" />
			</marker>
			<marker
				id={scopedId("flow-arrow-capacity-scaling")}
				markerWidth="10"
				markerHeight="10"
				markerUnits="userSpaceOnUse"
				refX="8"
				refY="5"
				orient="auto"
				viewBox="0 0 10 10"
				overflow="visible"
			>
				<path d="M0,0 L10,5 L0,10 Z" className="flow-arrow-capacity-scaling" />
			</marker>
			{(["active", "candidate", "tree"] as const).map((role) => (
				<marker
					key={`advanced-${role}`}
					id={scopedId(`flow-arrow-advanced-${role}`)}
					markerWidth="9"
					markerHeight="9"
					markerUnits="userSpaceOnUse"
					refX="7"
					refY="4.5"
					orient="auto"
				>
					<path
						d="M0,0 L9,4.5 L0,9 Z"
						className={`flow-arrow-advanced-${role}`}
					/>
				</marker>
			))}
			<marker
				id={scopedId("flow-arrow-forest")}
				markerWidth="8"
				markerHeight="8"
				markerUnits="userSpaceOnUse"
				refX="6"
				refY="4"
				orient="auto"
			>
				<path d="M0,0 L8,4 L0,8 Z" className="flow-arrow-forest" />
			</marker>
			<marker
				id={scopedId("flow-arrow-ibfs-source")}
				markerWidth="8"
				markerHeight="8"
				markerUnits="userSpaceOnUse"
				refX="6"
				refY="4"
				orient="auto"
			>
				<path d="M0,0 L8,4 L0,8 Z" className="flow-arrow-ibfs-source" />
			</marker>
			<marker
				id={scopedId("flow-arrow-ibfs-sink")}
				markerWidth="8"
				markerHeight="8"
				markerUnits="userSpaceOnUse"
				refX="6"
				refY="4"
				orient="auto"
			>
				<path d="M0,0 L8,4 L0,8 Z" className="flow-arrow-ibfs-sink" />
			</marker>
			<marker
				id={scopedId("flow-arrow-eibfs-source")}
				markerWidth="8"
				markerHeight="8"
				markerUnits="userSpaceOnUse"
				refX="6"
				refY="4"
				orient="auto"
			>
				<path d="M0,0 L8,4 L0,8 Z" className="flow-arrow-eibfs-source" />
			</marker>
			<marker
				id={scopedId("flow-arrow-eibfs-sink")}
				markerWidth="8"
				markerHeight="8"
				markerUnits="userSpaceOnUse"
				refX="6"
				refY="4"
				orient="auto"
			>
				<path d="M0,0 L8,4 L0,8 Z" className="flow-arrow-eibfs-sink" />
			</marker>
			<marker
				id={scopedId("flow-arrow-dual")}
				markerWidth="7"
				markerHeight="7"
				markerUnits="userSpaceOnUse"
				refX="6"
				refY="3.5"
				orient="auto-start-reverse"
			>
				<path d="M0,0 L7,3.5 L0,7 Z" className="flow-arrow-dual" />
			</marker>
			<marker
				id={scopedId("flow-arrow-dual-active")}
				markerWidth="7"
				markerHeight="7"
				markerUnits="userSpaceOnUse"
				refX="6"
				refY="3.5"
				orient="auto-start-reverse"
			>
				<path d="M0,0 L7,3.5 L0,7 Z" className="flow-arrow-dual-active" />
			</marker>
			<pattern
				id={scopedId("flow-parametric-tie-hatch")}
				width="7"
				height="7"
				patternUnits="userSpaceOnUse"
				patternTransform="rotate(35)"
			>
				<rect width="7" height="7" className="flow-parametric-tie-base" />
				<line
					x1="0"
					y1="0"
					x2="0"
					y2="7"
					className="flow-parametric-tie-stripe"
				/>
			</pattern>
		</defs>
	);
}

export function FlowEntityGraph({
	plan: graphPlan,
	selection,
	onSelectionChange,
	canvasBinding,
}: {
	plan: FlowEntityGraphPlan;
	selection: FlowEntitySelection | undefined;
	onSelectionChange: (selection: FlowEntitySelection) => void;
	canvasBinding: FlowCanvasSvgBinding;
}) {
	const idScope = useFlowDomIdScope("flow-graph");
	const titleId = flowScopedDomId(idScope, "title");
	const descriptionId = flowScopedDomId(idScope, "description");
	const [hoveredEdgeId, setHoveredEdgeId] = useState<string>();
	// biome-ignore lint/correctness/useExhaustiveDependencies: An external selection revision intentionally invalidates transient pointer hover, even though the selected value is not read by the effect.
	useEffect(() => {
		setHoveredEdgeId(undefined);
	}, [selection?.id, selection?.kind]);
	const state = useMemo(
		() => projectFlowEntityGraphState(graphPlan),
		[graphPlan],
	);
	const selectSpatialEntity = (event: ReactMouseEvent<SVGSVGElement>) => {
		if (!(event.target instanceof Element)) return;
		const entity = event.target.closest(
			"[data-node-id], [data-edge-id], [data-edge-label-for]",
		);
		if (entity === null || !event.currentTarget.contains(entity)) return;
		const nodeId = entity.getAttribute("data-node-id");
		if (nodeId !== null) {
			onSelectionChange({ kind: "node", id: nodeId });
			return;
		}
		const edgeId =
			entity.getAttribute("data-edge-id") ??
			entity.getAttribute("data-edge-label-for");
		if (edgeId === null) return;
		const residualDirection = entity.getAttribute("data-residual-direction");
		if (residualDirection === "forward" || residualDirection === "reverse") {
			onSelectionChange({
				kind: "residual-arc",
				id: JSON.stringify([edgeId, residualDirection]),
				edgeId,
				direction: residualDirection,
			});
			return;
		}
		onSelectionChange({ kind: "edge", id: edgeId });
	};
	const updateHoveredEdge = (event: ReactPointerEvent<SVGSVGElement>) => {
		if (!(event.target instanceof Element)) return;
		const entity = event.target.closest(
			"[data-edge-id], [data-edge-label-for]",
		);
		if (entity === null || !event.currentTarget.contains(entity)) {
			setHoveredEdgeId(undefined);
			return;
		}
		setHoveredEdgeId(
			entity.getAttribute("data-edge-id") ??
				entity.getAttribute("data-edge-label-for") ??
				undefined,
		);
	};

	return (
		<FlowGraphIdScopeProvider scope={idScope}>
			{/* biome-ignore lint/a11y/useKeyWithClickEvents: The graph is a pointer enhancement; the adjacent entity navigator provides complete keyboard controls. */}
			<svg
				{...canvasBinding}
				className={flowGraphSceneClassName(state)}
				role="img"
				aria-labelledby={titleId}
				aria-describedby={descriptionId}
				style={
					{
						"--flow-parametric-tie-hatch": flowScopedSvgUrl(
							idScope,
							"flow-parametric-tie-hatch",
						),
					} as CSSProperties
				}
				{...flowGraphSceneDataAttributes(state)}
				onClick={selectSpatialEntity}
				onPointerMove={updateHoveredEdge}
				onPointerLeave={() => setHoveredEdgeId(undefined)}
			>
				<title id={titleId}>Validated flow network</title>
				<desc id={descriptionId}>{flowGraphAccessibleDescription(state)}</desc>
				<FlowGraphMarkers />
				<FlowGraphEntityLayerComposer
					state={state}
					selection={selection}
					hoveredEdgeId={hoveredEdgeId}
				/>
			</svg>
		</FlowGraphIdScopeProvider>
	);
}
