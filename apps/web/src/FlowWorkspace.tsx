import {
	type CSSProperties,
	lazy,
	Suspense,
	useCallback,
	useEffect,
	useMemo,
	useReducer,
	useRef,
	useState,
} from "react";
import type { EngineRequest, EngineResponse } from "./engine-types";
import { FlowEncodingKey } from "./FlowEncodingKey";
import { FlowEntityGraph, type RmfgenFrameGroup } from "./FlowEntityGraph";
import { FlowInspectorPanel } from "./FlowInspectorPanel";
import { FlowMobileEdgeSelection } from "./FlowMobileEdgeSelection";
import { FlowOverlayRichStatusRegion } from "./FlowOverlayRichStatusRegion";
import { FlowOverviewGraph } from "./FlowOverviewGraph";
import { FlowTimeline } from "./FlowTimeline";
import {
	type FlowAlgorithmCatalogEntry,
	flowScenarioSelection,
} from "./flow-algorithm-catalog";
import {
	defaultFlowAlgorithmConfig,
	flowScenarioNodeIds,
} from "./flow-algorithm-config";
import type { FlowAlgorithmConformanceContract } from "./flow-algorithm-conformance";
import {
	flowScopedDomId,
	flowScopedSvgUrl,
	useFlowDomIdScope,
} from "./flow-dom-id";
import { flowEngineErrorOwner } from "./flow-engine-error-owner";
import type { FlowEntitySelection } from "./flow-entity-navigator";
import { flowEventCaption } from "./flow-event-caption";
import {
	buildFlowGenerationResultSummary,
	type FlowGenerationResultSummary as FlowGenerationSummary,
} from "./flow-generation-result";
import type {
	FlowGeneratorFixture,
	FlowGeneratorFixturePreset,
} from "./flow-generator-fixture";
import type { FlowGeneratorForm } from "./flow-generator-form";
import type {
	FlowGeneratorProgress,
	FlowGeneratorRunProfile,
	FlowGeneratorWorkerRequest,
	FlowGeneratorWorkerResponse,
} from "./flow-generator-worker-protocol";
import {
	buildFlowNodePositions,
	FLOW_VIEWBOX_HEIGHT,
	FLOW_VIEWBOX_WIDTH,
} from "./flow-layout";
import {
	chooseFlowCanvasLod,
	constrainFlowCanvasLodToBaseline,
	type FlowCanvasLod,
} from "./flow-lod-policy";
import { flowOverlayCanvasClassName } from "./flow-overlay-canvas-chrome";
import { buildFlowOverlayPresentation } from "./flow-overlay-presentation";
import {
	formatFlowRational,
	projectFlowParametricChart,
} from "./flow-parametric-view";
import {
	createFlowBoundaryInventory,
	flowAdjacentVisibleBoundary,
	flowEffectivePlaybackGranularity,
	flowSceneVisibleAtGranularity,
	recordFlowBoundary,
	resetFlowBoundaryInventory,
} from "./flow-playback";
import {
	type FlowPlaybackGranularity,
	type FlowPreferenceSnapshot,
	type FlowViewMode,
	readFlowPreferences,
	writeFlowPreferences,
} from "./flow-preferences";
import {
	assertFlowPublicationAlgorithmIdentity,
	flowPublicationLifecycleReducer,
	INITIAL_FLOW_PUBLICATION_LIFECYCLE,
} from "./flow-publication-lifecycle";
import {
	buildFlowRenderPlan,
	type FlowRenderPlan,
	flowLodForScene,
} from "./flow-render-plan";
import { flowResourceLimitMessage } from "./flow-resource-limit";
import {
	decodeFlowCurrentSceneV9,
	type FlowCurrentSceneV9,
} from "./flow-scene";
import {
	flowWorkbenchModelRejectionMessage,
	flowWorkbenchPolicy,
} from "./flow-workbench-policy";
import {
	type FlowWorkbenchProblemKind,
	flowGeneratorTargetProblem,
	flowInputModelKind,
	flowProblemTitle,
} from "./flow-workbench-problem";
import { assemblePublicationV6, decodePacketPartV6 } from "./packet-v6";
import { useEngineWorker } from "./use-engine-worker";
import { useFlowCanvasViewport } from "./use-flow-canvas-viewport";
import { useFlowGeneratorWorker } from "./use-flow-generator-worker";
import { useFlowInspectorViewModel } from "./use-flow-inspector-view-model";
import { useFlowKeyboardControls } from "./use-flow-keyboard-controls";

type FlowStatus =
	| "loading"
	| "generating"
	| "configuring"
	| "solving"
	| "ready"
	| "edited"
	| "error"
	| "limited"
	| "fatal";
type MobilePanel = "input" | "inspector";
type FlowGeneratorReplayRequest = Omit<
	FlowGeneratorWorkerRequest,
	"jobId" | "kind"
>;
type PendingFlowGeneration = Readonly<{
	generation: number;
	summary: FlowGenerationSummary;
}>;

const FlowGeneratorDialog = lazy(async () => {
	const module = await import("./FlowGeneratorDialog");
	return { default: module.FlowGeneratorDialog };
});

const FlowAlgorithmCatalogDialog = lazy(async () => {
	const module = await import("./FlowAlgorithmCatalogDialog");
	return { default: module.FlowAlgorithmCatalogDialog };
});

function modelLabel(model: FlowCurrentSceneV9["model"]): string {
	switch (model.kind) {
		case "max-flow":
			return "Max Flow";
		case "parametric-max-flow":
			return "Parametric Max Flow";
		case "fixed-flow-min-cost":
			return `Min-Cost Flow · demand ${model.required_flow}`;
		case "min-cost-max-flow":
			return "Min-Cost Max-Flow";
		case "circulation":
			return "Min-Cost Circulation";
		case "transshipment":
			return "Min-Cost Transshipment";
		case "convex-cost-flow":
			return "Convex-Cost Flow";
		case "bipartite-matching":
			return `Bipartite Matching · L ${model.left.length} / R ${model.right.length}`;
		case "assignment":
			return `Assignment · ${model.objective === "minimize" ? "minimize" : "maximize"} · ${model.agents.length} × ${model.tasks.length}`;
		case "transportation":
			return `Transportation · ${model.origins.length} origins / ${model.destinations.length} destinations`;
		case "planar-max-flow":
			return "Planar Max Flow";
	}
}

function generatorFormForProblem(
	form: FlowGeneratorForm,
	problemKind: FlowWorkbenchProblemKind,
): FlowGeneratorForm {
	return flowWorkbenchPolicy(problemKind).normalizeGenerator(form);
}

function dynamicEibfsStageLabel(
	stage: NonNullable<FlowCurrentSceneV9["dynamic_eibfs_overlay"]>["stage"],
): string {
	switch (stage) {
		case "initial-solve":
			return "Solve initial network";
		case "apply-update":
			return "Apply capacity update";
		case "repair-capacity":
			return "Repair capacity overflow";
		case "repair-forest":
			return "Repair search forests";
		case "repair-violation":
			return "Repair dynamic violation";
		case "continue-solve":
			return "Resume from warm state";
		case "prefix-recovery":
			return "Recover certificate flow";
		case "prefix-certified":
			return "Prefix certified";
		case "resume-reusable-pseudoflow":
			return "Restore reusable pseudoflow";
	}
}

function DynamicEibfsUpdateStrip({
	overlay,
}: {
	overlay: FlowCurrentSceneV9["dynamic_eibfs_overlay"];
}) {
	if (overlay === undefined) return null;
	const updateIndex = Number(overlay.update_index);
	const updateTotal = Number(overlay.update_total);
	return (
		<section className="flow-dynamic-strip" aria-label="Dynamic EIBFS updates">
			<div className="flow-dynamic-progress">
				<span className="flow-dynamic-prefix">
					PREFIX {overlay.update_index}/{overlay.update_total}
				</span>
				<progress
					max={updateTotal}
					value={updateIndex}
					aria-label={`Capacity update ${overlay.update_index} of ${overlay.update_total}`}
				/>
				<strong>{dynamicEibfsStageLabel(overlay.stage)}</strong>
			</div>
			<div className="flow-dynamic-facts">
				{overlay.changed_edge !== undefined && (
					<span className="flow-dynamic-edge">
						{overlay.changed_edge} · {overlay.old_capacity} →{" "}
						{overlay.new_capacity}
					</span>
				)}
				{overlay.violation !== undefined && (
					<span className="flow-dynamic-violation">
						repair {overlay.violation}
					</span>
				)}
				<span>reused {overlay.reused_forest_nodes} nodes</span>
				<span>invalidated {overlay.invalidated_parent_arcs} parents</span>
				<span>promoted {overlay.promoted_roots} roots</span>
				{overlay.prefix_value !== undefined && (
					<span className="flow-dynamic-certified">
						max flow {overlay.prefix_value}
					</span>
				)}
			</div>
		</section>
	);
}

function buildRmfgenFrameGroups(
	nodes: FlowCurrentSceneV9["graph"]["nodes"],
	positions: ReadonlyMap<string, { x: number; y: number }>,
): RmfgenFrameGroup[] {
	const bounds = new Map<
		number,
		{ minimumX: number; maximumX: number; minimumY: number; maximumY: number }
	>();
	for (const node of nodes) {
		const match = /^f([0-9]{3})r[0-9]{3}c[0-9]{3}$/.exec(node.id);
		const position = positions.get(node.id);
		if (match === null || position === undefined) return [];
		const frame = Number(match[1]);
		const current = bounds.get(frame);
		if (current === undefined) {
			bounds.set(frame, {
				minimumX: position.x,
				maximumX: position.x,
				minimumY: position.y,
				maximumY: position.y,
			});
		} else {
			current.minimumX = Math.min(current.minimumX, position.x);
			current.maximumX = Math.max(current.maximumX, position.x);
			current.minimumY = Math.min(current.minimumY, position.y);
			current.maximumY = Math.max(current.maximumY, position.y);
		}
	}
	return [...bounds.entries()]
		.sort(([left], [right]) => left - right)
		.map(([frame, bound]) => {
			const x = Math.max(8, bound.minimumX - 34);
			const y = Math.max(8, bound.minimumY - 30);
			const maximumX = Math.min(FLOW_VIEWBOX_WIDTH - 8, bound.maximumX + 34);
			const maximumY = Math.min(FLOW_VIEWBOX_HEIGHT - 8, bound.maximumY + 30);
			return {
				frame,
				x,
				y,
				width: maximumX - x,
				height: maximumY - y,
			};
		});
}

function ParametricAnalysisPanel({
	scene,
	overlay,
}: {
	scene: FlowCurrentSceneV9;
	overlay: NonNullable<FlowCurrentSceneV9["parametric_overlay"]>;
}) {
	const idScope = useFlowDomIdScope("flow-parametric-analysis");
	const analysisTitleId = flowScopedDomId(idScope, "title");
	const chartTitleId = flowScopedDomId(idScope, "chart-title");
	const chartDescriptionId = flowScopedDomId(idScope, "chart-description");
	const chartHatchId = flowScopedDomId(idScope, "chart-hatch");
	const projection = useMemo(() => projectFlowParametricChart(scene), [scene]);
	if (projection === undefined) return null;
	const traversal = overlay.traversal;
	const left = 52;
	const right = 704;
	const top = 18;
	const bottom = 132;
	const x = (unit: number) => left + unit * (right - left);
	const y = (unit: number) => top + unit * (bottom - top);
	const traversalLabel =
		traversal === undefined
			? "initialize"
			: traversal.cold_static_rerun
				? `cold rerun #${traversal.static_run_ordinal ?? "—"}`
				: traversal.kind.replaceAll("-", " ");
	const retainedLabel =
		traversal === undefined
			? "—"
			: traversal.normalized_tree_reused && traversal.labels_retained
				? "tree + labels"
				: traversal.normalized_tree_reused
					? "tree"
					: traversal.labels_retained
						? "labels"
						: "restart";

	return (
		<section
			className="flow-parametric-analysis"
			aria-labelledby={analysisTitleId}
		>
			<div className="flow-parametric-analysis-heading">
				<div>
					<p className="eyebrow">PARAMETRIC CUT ENVELOPE</p>
					<h3 id={analysisTitleId}>Maximum flow F(λ)</h3>
				</div>
				<span className="flow-parametric-current-value">
					λ = {projection.currentParameterLabel}
				</span>
			</div>
			<div className="flow-parametric-analysis-grid">
				<figure className="flow-parametric-chart">
					<svg
						viewBox="0 0 720 158"
						role="img"
						aria-labelledby={`${chartTitleId} ${chartDescriptionId}`}
						style={
							{
								"--flow-parametric-chart-hatch": flowScopedSvgUrl(
									idScope,
									"chart-hatch",
								),
							} as CSSProperties
						}
					>
						<title id={chartTitleId}>
							Piecewise-linear maximum flow and breakpoints
						</title>
						<desc id={chartDescriptionId}>
							The horizontal axis is parameter λ and the vertical axis is the
							exact maximum flow. Hatched bands mark tied intervals where the
							minimal and maximal source-side cuts differ. The amber marker is
							the current event.
						</desc>
						<defs>
							<pattern
								id={chartHatchId}
								width="8"
								height="8"
								patternUnits="userSpaceOnUse"
								patternTransform="rotate(35)"
							>
								<line
									x1="0"
									y1="0"
									x2="0"
									y2="8"
									className="flow-parametric-chart-hatch-line"
								/>
							</pattern>
						</defs>
						<line
							className="flow-parametric-axis"
							x1={left}
							y1={bottom}
							x2={right}
							y2={bottom}
						/>
						<line
							className="flow-parametric-axis"
							x1={left}
							y1={top}
							x2={left}
							y2={bottom}
						/>
						{projection.segments
							.filter((segment) => segment.tied)
							.map((segment) => (
								<rect
									key={`tie:${segment.lowerLabel}:${segment.upperLabel}`}
									className="flow-parametric-tie-interval"
									x={x(segment.x1)}
									y={top}
									width={Math.max(1, x(segment.x2) - x(segment.x1))}
									height={bottom - top}
								/>
							))}
						{projection.segments.map((segment) => (
							<line
								key={`segment:${segment.lowerLabel}:${segment.upperLabel}`}
								className="flow-parametric-value-segment"
								x1={x(segment.x1)}
								y1={y(segment.y1)}
								x2={x(segment.x2)}
								y2={y(segment.y2)}
							/>
						))}
						{projection.breakpoints.map((breakpoint) => (
							<g
								key={`breakpoint:${breakpoint.parameterLabel}`}
								className="flow-parametric-breakpoint"
							>
								<line
									x1={x(breakpoint.x)}
									y1={top}
									x2={x(breakpoint.x)}
									y2={bottom}
								/>
								<circle
									cx={x(breakpoint.x)}
									cy={y(breakpoint.y)}
									r={breakpoint.tied ? 5 : 3.5}
								/>
							</g>
						))}
						<line
							className="flow-parametric-current-marker"
							x1={x(projection.currentX)}
							y1={top - 3}
							x2={x(projection.currentX)}
							y2={bottom + 3}
						/>
						<text className="flow-parametric-axis-label" x={left} y="151">
							{projection.domainMinimumLabel}
						</text>
						<text
							className="flow-parametric-axis-label"
							x={right}
							y="151"
							textAnchor="end"
						>
							{projection.domainMaximumLabel} · λ
						</text>
						<text
							className="flow-parametric-axis-label"
							x={left - 6}
							y={top + 4}
							textAnchor="end"
						>
							{projection.valueMaximumLabel}
						</text>
						<text
							className="flow-parametric-axis-label"
							x={left - 6}
							y={bottom}
							textAnchor="end"
						>
							{projection.valueMinimumLabel}
						</text>
					</svg>
					<figcaption>
						The solid line is F(λ); hatching marks intervals with multiple
						optimal minimum cuts.
					</figcaption>
				</figure>
				<div className="flow-parametric-facts">
					<dl>
						<div className="flow-parametric-fact">
							<dt>Traversal</dt>
							<dd>{traversalLabel}</dd>
						</div>
						<div className="flow-parametric-fact">
							<dt>Retained state</dt>
							<dd>{retainedLabel}</dd>
						</div>
						<div className="flow-parametric-fact">
							<dt>Range</dt>
							<dd>
								{traversal === undefined
									? "—"
									: `${formatFlowRational(traversal.lower)}…${formatFlowRational(traversal.upper)}`}
							</dd>
						</div>
						<div className="flow-parametric-fact">
							<dt>Renormalize</dt>
							<dd>
								{traversal === undefined
									? "—"
									: `${traversal.renormalization_pushes} push · ${traversal.renormalization_splits} split`}
							</dd>
						</div>
						{traversal?.race_winner !== undefined && (
							<div className="flow-parametric-fact">
								<dt>Free-run race</dt>
								<dd>{traversal.race_winner} wins</dd>
							</div>
						)}
					</dl>
					<ul
						className="flow-parametric-segment-list"
						aria-label="Certified intervals"
					>
						{projection.segments.length === 0 ? (
							<li>Searching for intervals</li>
						) : (
							projection.segments.map((segment) => (
								<li key={`formula:${segment.lowerLabel}:${segment.upperLabel}`}>
									[{segment.lowerLabel}, {segment.upperLabel}] · F ={" "}
									{segment.formula}
								</li>
							))
						)}
					</ul>
				</div>
			</div>
		</section>
	);
}

function FlowGraph({
	scene,
	graphRevision,
	problemKind,
	viewMode,
	fitRequest,
	selection,
	onSelectionChange,
	onRenderPlanChange,
}: {
	scene: FlowCurrentSceneV9;
	graphRevision: number;
	problemKind: FlowWorkbenchProblemKind;
	viewMode: FlowViewMode;
	fitRequest: number;
	selection: FlowEntitySelection | undefined;
	onSelectionChange: (selection: FlowEntitySelection) => void;
	onRenderPlanChange: (plan: FlowRenderPlan) => void;
}) {
	const graphResetKey = graphRevision.toString();
	const canvas = useFlowCanvasViewport(graphResetKey);
	useEffect(() => {
		if (fitRequest > 0) canvas.fit();
	}, [canvas.fit, fitRequest]);
	const baselineLod = flowLodForScene(scene);
	const [lod, setLod] = useState<FlowCanvasLod>(() => baselineLod);
	const visibleEdgeCount =
		viewMode === "original"
			? scene.graph.edges.length
			: viewMode === "residual"
				? scene.residual_arcs.length
				: scene.graph.edges.length + scene.residual_arcs.length;
	useEffect(() => setLod(baselineLod), [baselineLod]);
	useEffect(() => {
		setLod((current) =>
			constrainFlowCanvasLodToBaseline(
				chooseFlowCanvasLod({
					current,
					zoom: canvas.viewport.zoom,
					viewport: canvas.size,
					entityCounts: {
						nodes: scene.graph.nodes.length,
						edges: visibleEdgeCount,
					},
				}),
				baselineLod,
			),
		);
	}, [
		baselineLod,
		canvas.size,
		canvas.viewport.zoom,
		scene.graph.nodes.length,
		visibleEdgeCount,
	]);
	const plan = useMemo(() => buildFlowRenderPlan(scene, lod), [lod, scene]);
	useEffect(() => onRenderPlanChange(plan), [onRenderPlanChange, plan]);
	const frameGroups = useMemo(
		() =>
			buildRmfgenFrameGroups(
				scene.graph.nodes,
				buildFlowNodePositions(scene.graph.nodes, {
					edges: scene.graph.edges,
					model: scene.model,
				}),
			),
		[scene.graph.edges, scene.graph.nodes, scene.model],
	);
	return (
		<div className="flow-interactive-graph">
			<fieldset className="flow-canvas-controls">
				<legend className="visually-hidden">Canvas zoom controls</legend>
				<button
					type="button"
					className="quiet-button"
					aria-label="Zoom out"
					onClick={canvas.zoomOut}
					disabled={canvas.viewport.zoom <= 1}
				>
					−
				</button>
				<button
					type="button"
					className="quiet-button flow-canvas-fit"
					onClick={canvas.fit}
				>
					Fit
				</button>
				<output aria-label="Canvas zoom">
					{Math.round(canvas.viewport.zoom * 100)}%
				</output>
				<button
					type="button"
					className="quiet-button"
					aria-label="Zoom in"
					onClick={canvas.zoomIn}
					disabled={canvas.viewport.zoom >= 8}
				>
					+
				</button>
			</fieldset>
			{plan.kind === "overview" ? (
				<FlowOverviewGraph
					key={graphRevision}
					plan={plan}
					problemKind={problemKind}
					viewMode={viewMode}
					frameGroups={frameGroups}
					selection={selection}
					onSelectionChange={onSelectionChange}
					canvasBinding={canvas.svgBinding}
				/>
			) : (
				<FlowEntityGraph
					key={graphRevision}
					plan={{ render: plan, viewMode, frameGroups }}
					selection={selection}
					onSelectionChange={onSelectionChange}
					canvasBinding={canvas.svgBinding}
				/>
			)}
		</div>
	);
}

export function FlowWorkspace({
	active,
	problemKind,
}: {
	active: boolean;
	problemKind: FlowWorkbenchProblemKind;
}) {
	const inputPanelId = `flow-${problemKind}-input-panel`;
	const inspectorPanelId = `flow-${problemKind}-inspector-panel`;
	const scenarioEditorId = `flow-${problemKind}-scenario-editor`;
	const policy = useMemo(() => flowWorkbenchPolicy(problemKind), [problemKind]);
	const defaultScenario = useMemo(() => policy.defaultScenario(), [policy]);
	const baselineGenerator = useMemo(() => policy.defaultGenerator(), [policy]);
	const initialPreferences = useMemo(() => {
		const preferences = readFlowPreferences(
			typeof window === "undefined" ? undefined : window.localStorage,
			problemKind,
			baselineGenerator,
		);
		return {
			...preferences,
			generator: policy.restoreGenerator(preferences.generator),
		};
	}, [baselineGenerator, policy, problemKind]);
	const [scenario, setScenario] = useState(defaultScenario);
	const [flowDsl, setFlowDsl] = useState(() => policy.defaultDsl());
	const [inputFormat, setInputFormat] = useState<"json" | "dsl">("json");
	const [viewMode, setViewMode] = useState<FlowViewMode>(
		initialPreferences.viewMode,
	);
	const [preferredPlaybackGranularity, setPreferredPlaybackGranularity] =
		useState<FlowPlaybackGranularity>(initialPreferences.granularity);
	const [playbackModeNotice, setPlaybackModeNotice] = useState<string>();
	const [playbackSpeed, setPlaybackSpeed] = useState(1);
	const [canvasFitRequest, setCanvasFitRequest] = useState(0);
	const [mobilePanel, setMobilePanel] = useState<MobilePanel>();
	const [scene, setScene] = useState<FlowCurrentSceneV9>();
	const effectivePlaybackGranularity =
		scene === undefined
			? preferredPlaybackGranularity
			: flowEffectivePlaybackGranularity(
					preferredPlaybackGranularity,
					scene.trace_steps,
				);
	const [status, setStatus] = useState<FlowStatus>("loading");
	const [publicationLifecycle, dispatchPublicationLifecycle] = useReducer(
		flowPublicationLifecycleReducer,
		INITIAL_FLOW_PUBLICATION_LIFECYCLE,
	);
	const publicationLifecycleRef = useRef(publicationLifecycle);
	const [error, setError] = useState<string>();
	const [generatorOpen, setGeneratorOpen] = useState(false);
	const generatorTriggerRef = useRef<HTMLButtonElement>(null);
	const generatorWasOpen = useRef(false);
	const [generatorForm, setGeneratorForm] = useState<FlowGeneratorForm>(
		initialPreferences.generator,
	);
	const [generatorFamilyGroup, setGeneratorFamilyGroup] = useState(
		initialPreferences.familyGroup,
	);
	const preferenceSnapshot = useRef<FlowPreferenceSnapshot>({
		generator: initialPreferences.generator,
		viewMode: initialPreferences.viewMode,
		granularity: initialPreferences.granularity,
		familyGroup: initialPreferences.familyGroup,
	});
	const [generatorError, setGeneratorError] = useState<string>();
	const [generatorProgress, setGeneratorProgress] =
		useState<FlowGeneratorProgress>();
	const [generatorResult, setGeneratorResult] =
		useState<FlowGenerationSummary>();
	const [generatorFixtures, setGeneratorFixtures] =
		useState<FlowGeneratorFixture[]>();
	const [generatorFixtureError, setGeneratorFixtureError] = useState<string>();
	const [algorithmCatalogOpen, setAlgorithmCatalogOpen] = useState(false);
	const algorithmCatalogTriggerRef = useRef<HTMLButtonElement>(null);
	const algorithmCatalogShouldRestoreFocus = useRef(false);
	const [algorithmCatalog, setAlgorithmCatalog] =
		useState<FlowAlgorithmCatalogEntry[]>();
	useEffect(() => {
		if (generatorWasOpen.current && !generatorOpen) {
			generatorTriggerRef.current?.focus();
		}
		generatorWasOpen.current = generatorOpen;
	}, [generatorOpen]);
	const [algorithmConformance, setAlgorithmConformance] =
		useState<FlowAlgorithmConformanceContract[]>();
	const [algorithmCatalogError, setAlgorithmCatalogError] = useState<string>();
	const [selectedEntity, setSelectedEntity] = useState<FlowEntitySelection>();
	const [renderPlan, setRenderPlan] = useState<FlowRenderPlan>();
	const [graphRevision, setGraphRevision] = useState(0);
	const generation = useRef(0);
	const seekRequestSerial = useRef(0);
	const generatorJob = useRef(0);
	const pendingFlowGeneration = useRef<PendingFlowGeneration | undefined>(
		undefined,
	);
	const engineFailed = useRef(false);
	const sceneRef = useRef<FlowCurrentSceneV9 | undefined>(undefined);
	const acceptedGraphIdentity = useRef<string | undefined>(undefined);
	const preferredPlaybackGranularityRef = useRef(preferredPlaybackGranularity);
	const traceBoundaryInventory = useRef(createFlowBoundaryInventory());
	const navigationDirection = useRef<-1 | 0 | 1>(0);
	const discardProvenanceOnLoad = useRef(false);
	const autoplay = useRef(false);
	const autoplayTimer = useRef<number | undefined>(undefined);
	const postRef = useRef<(request: EngineRequest) => void>(() => undefined);
	const inputPanelTriggerRef = useRef<HTMLButtonElement>(null);
	const inspectorPanelTriggerRef = useRef<HTMLButtonElement>(null);
	const inputPanelCloseRef = useRef<HTMLButtonElement>(null);
	const inspectorPanelCloseRef = useRef<HTMLButtonElement>(null);
	const topbarRef = useRef<HTMLElement>(null);
	const inputPanelRef = useRef<HTMLElement>(null);
	const scenarioEditorRef = useRef<HTMLTextAreaElement>(null);
	const canvasPanelRef = useRef<HTMLElement>(null);
	const inspectorPanelRef = useRef<HTMLElement>(null);
	const resetTraceBoundaryIndex = useCallback(() => {
		resetFlowBoundaryInventory(traceBoundaryInventory.current);
	}, []);
	const postNextNavigation = useCallback(() => {
		seekRequestSerial.current += 1;
		postRef.current({
			kind: "next",
			generation: generation.current,
			requestSerial: seekRequestSerial.current,
		});
	}, []);
	const scheduleAutoplayStep = useCallback(
		(speed: number) => {
			if (autoplayTimer.current !== undefined) {
				window.clearTimeout(autoplayTimer.current);
			}
			autoplayTimer.current = window.setTimeout(
				() => {
					autoplayTimer.current = undefined;
					postNextNavigation();
				},
				Math.max(16, 140 / speed),
			);
		},
		[postNextNavigation],
	);
	const recordTraceBoundary = useCallback((decoded: FlowCurrentSceneV9) => {
		const cursor = Number(decoded.event_id);
		const extent = Number(decoded.event_count);
		recordFlowBoundary(
			traceBoundaryInventory.current,
			cursor,
			extent,
			decoded.trace_event?.minimum_granularity,
		);
	}, []);
	const handleResponse = useCallback(
		async (response: EngineResponse) => {
			if (response.generation !== generation.current) return false;
			if (engineFailed.current) return false;
			if (
				"seekRequestSerial" in response &&
				response.seekRequestSerial !== undefined &&
				response.seekRequestSerial !== seekRequestSerial.current
			) {
				return false;
			}
			if (response.kind === "error") {
				autoplay.current = false;
				if (response.source === "engine") {
					engineFailed.current = true;
					setError(response.message);
					setStatus("fatal");
					return true;
				}
				if (
					response.requestKind === "create" &&
					pendingFlowGeneration.current?.generation === response.generation
				) {
					pendingFlowGeneration.current = undefined;
					setGeneratorProgress(undefined);
					setGeneratorError(response.message);
					setStatus(sceneRef.current === undefined ? "error" : "ready");
					return true;
				}
				const owner = flowEngineErrorOwner(response.requestKind);
				if (owner === "generator-fixtures") {
					setGeneratorFixtureError(response.message);
					return true;
				}
				if (owner === "algorithm-catalog") {
					setAlgorithmCatalogError(response.message);
					setStatus(sceneRef.current === undefined ? "error" : "ready");
					return true;
				}
				setError(response.message);
				setStatus("error");
				return true;
			}
			if (response.kind === "flow-catalog") {
				setAlgorithmCatalog(response.entries);
				setAlgorithmConformance(response.conformance);
				setAlgorithmCatalogError(undefined);
				return true;
			}
			if (response.kind === "flow-generator-fixtures") {
				setGeneratorFixtures(response.fixtures);
				setGeneratorFixtureError(undefined);
				return true;
			}
			if (response.kind === "scenario-prepared") {
				setScenario(response.scenario);
				setInputFormat("json");
				discardProvenanceOnLoad.current = false;
				setAlgorithmCatalogError(undefined);
				setAlgorithmCatalogOpen(false);
				resetTraceBoundaryIndex();
				generation.current += 1;
				setStatus("loading");
				postRef.current({
					kind: "create",
					generation: generation.current,
					scenario: response.scenario,
					discardProvenance: false,
					flowInputFormat: "json",
				});
				return true;
			}
			if (response.kind !== "flow-ready" && response.kind !== "flow-update")
				return false;
			const stageAction = {
				kind: "stage" as const,
				generation: response.generation,
				publicationId: response.publicationId,
			};
			publicationLifecycleRef.current = flowPublicationLifecycleReducer(
				publicationLifecycleRef.current,
				stageAction,
			);
			dispatchPublicationLifecycle(stageAction);
			const decodedParts = response.parts.map(decodePacketPartV6);
			if (
				decodedParts.some(
					(part) =>
						part.header.pluginOrdinal !== 2 ||
						part.header.payloadSchemaVersion !== 3 ||
						part.header.publicationId !== response.publicationId ||
						part.header.generation !== response.generation.toString(),
				)
			) {
				throw new Error(
					"Flow publication identity does not match its Worker envelope",
				);
			}
			const payload = await assemblePublicationV6(decodedParts);
			if (response.generation !== generation.current) return false;
			if (
				response.kind === "flow-update" &&
				response.seekRequestSerial !== undefined &&
				response.seekRequestSerial !== seekRequestSerial.current
			) {
				return false;
			}
			const decoded = decodeFlowCurrentSceneV9(payload);
			assertFlowPublicationAlgorithmIdentity(
				response.algorithm,
				decoded.algorithm.id,
			);
			const effectiveGranularity = flowEffectivePlaybackGranularity(
				preferredPlaybackGranularityRef.current,
				decoded.trace_steps,
			);
			if (effectiveGranularity !== preferredPlaybackGranularityRef.current) {
				const preferredLabel =
					preferredPlaybackGranularityRef.current === "micro"
						? "Detail"
						: preferredPlaybackGranularityRef.current === "operation"
							? "Operation"
							: "Phase";
				const effectiveLabel =
					effectiveGranularity === "micro"
						? "Detail"
						: effectiveGranularity === "operation"
							? "Operation"
							: "Phase";
				const unavailableReason = (() => {
					if (
						preferredPlaybackGranularityRef.current === "micro" &&
						decoded.trace_steps.detail.availability === "unavailable"
					) {
						return decoded.trace_steps.detail.reason;
					}
					if (
						preferredPlaybackGranularityRef.current === "operation" &&
						decoded.trace_steps.operation_availability.availability ===
							"unavailable"
					) {
						return decoded.trace_steps.operation_availability.reason;
					}
					if (
						preferredPlaybackGranularityRef.current === "phase" &&
						decoded.trace_steps.phase_availability.availability ===
							"unavailable"
					) {
						return decoded.trace_steps.phase_availability.reason;
					}
					throw new Error(
						"Effective playback differs without an unavailable boundary",
					);
				})();
				setPlaybackModeNotice(
					`${preferredLabel} is unavailable. Using ${effectiveLabel} for this algorithm; your ${preferredLabel} preference is retained. ${unavailableReason}`,
				);
			} else {
				setPlaybackModeNotice(undefined);
			}
			const rejection = flowWorkbenchModelRejectionMessage(
				policy,
				decoded.model.kind,
			);
			if (rejection !== undefined) throw new Error(rejection);
			if (
				response.kind === "flow-ready" &&
				pendingFlowGeneration.current?.generation === response.generation
			) {
				resetTraceBoundaryIndex();
			}
			recordTraceBoundary(decoded);
			const visibleBoundary =
				navigationDirection.current === 0 ||
				flowSceneVisibleAtGranularity(decoded, effectiveGranularity);
			if (!visibleBoundary) {
				setError(undefined);
				setStatus("solving");
				if (navigationDirection.current === -1) {
					const hiddenCursor = Number(decoded.event_id);
					seekRequestSerial.current += 1;
					postRef.current({
						kind: "seek",
						generation: generation.current,
						target: Math.max(0, hiddenCursor - 1),
						requestSerial: seekRequestSerial.current,
					});
				} else {
					postNextNavigation();
				}
				return true;
			}
			const presented = decoded;
			if (response.kind === "flow-ready") {
				const nextGraphIdentity = JSON.stringify([
					presented.model,
					presented.graph.nodes,
					presented.graph.edges,
				]);
				if (acceptedGraphIdentity.current !== nextGraphIdentity) {
					acceptedGraphIdentity.current = nextGraphIdentity;
					setGraphRevision((current) => current + 1);
					setSelectedEntity(undefined);
					setRenderPlan(undefined);
				}
			}
			sceneRef.current = presented;
			setScene(presented);
			if (response.kind === "flow-ready") {
				setScenario(response.scenario);
				discardProvenanceOnLoad.current = false;
			}
			const resourceLimitMessage = flowResourceLimitMessage(decoded);
			setError(resourceLimitMessage);
			const continuePlaying =
				autoplay.current && decoded.solve_status === "running";
			setStatus(
				continuePlaying
					? "solving"
					: resourceLimitMessage === undefined
						? "ready"
						: "limited",
			);
			if (continuePlaying) {
				scheduleAutoplayStep(playbackSpeed);
			} else {
				autoplay.current = false;
			}
			return true;
		},
		[
			playbackSpeed,
			policy,
			postNextNavigation,
			recordTraceBoundary,
			resetTraceBoundaryIndex,
			scheduleAutoplayStep,
		],
	);
	const reportFatal = useCallback((message: string) => {
		engineFailed.current = true;
		setError(message);
		setStatus("fatal");
	}, []);
	const handleAcknowledged = useCallback(
		(response: EngineResponse, accepted: boolean) => {
			if (response.kind === "flow-ready" || response.kind === "flow-update") {
				const current = publicationLifecycleRef.current;
				const acknowledgement = {
					kind: "acknowledge" as const,
					generation: response.generation,
					publicationId: response.publicationId,
					accepted,
				};
				const next = flowPublicationLifecycleReducer(current, acknowledgement);
				if (next === current) return;
				publicationLifecycleRef.current = next;
				dispatchPublicationLifecycle(acknowledgement);
				if (response.kind === "flow-ready") {
					const pending = pendingFlowGeneration.current;
					if (pending?.generation === response.generation) {
						pendingFlowGeneration.current = undefined;
						setGeneratorProgress(undefined);
						if (accepted) {
							setGeneratorResult(pending.summary);
							setGeneratorError(undefined);
							setInputFormat("json");
							discardProvenanceOnLoad.current = false;
							setGeneratorOpen(false);
						}
					}
				}
			}
		},
		[],
	);
	const { post: postToEngine } = useEngineWorker(
		handleResponse,
		reportFatal,
		handleAcknowledged,
	);
	const post = useCallback(
		(request: EngineRequest) => {
			if (!engineFailed.current) postToEngine(request);
		},
		[postToEngine],
	);
	postRef.current = post;
	const handleGeneratorResponse = useCallback(
		(response: FlowGeneratorWorkerResponse) => {
			if (response.jobId !== generatorJob.current) return;
			switch (response.kind) {
				case "progress":
					setGeneratorProgress({
						stage: response.stage,
						completedPhases: response.completedPhases,
						totalPhases: response.totalPhases,
					});
					break;
				case "complete":
					try {
						const summary = buildFlowGenerationResultSummary(
							response.scenario,
							response.stats,
						);
						const rejection = flowWorkbenchModelRejectionMessage(
							policy,
							summary.modelKind,
						);
						if (rejection !== undefined) throw new Error(rejection);
						setGeneratorError(undefined);
						generation.current += 1;
						pendingFlowGeneration.current = {
							generation: generation.current,
							summary,
						};
						setStatus("loading");
						postRef.current({
							kind: "create",
							generation: generation.current,
							scenario: response.scenario,
							discardProvenance: false,
							flowInputFormat: "json",
						});
					} catch (summaryError) {
						setGeneratorProgress(undefined);
						setGeneratorResult(undefined);
						setGeneratorError(
							summaryError instanceof Error
								? summaryError.message
								: "Generated summary validation failed",
						);
						setStatus(sceneRef.current === undefined ? "error" : "ready");
					}
					break;
				case "error":
					setGeneratorProgress(undefined);
					setGeneratorError(response.message);
					setStatus(sceneRef.current === undefined ? "error" : "ready");
					break;
			}
		},
		[policy],
	);
	const { start: startGenerator, cancel: cancelGenerator } =
		useFlowGeneratorWorker(handleGeneratorResponse);
	useEffect(
		() => () => {
			if (autoplayTimer.current !== undefined) {
				window.clearTimeout(autoplayTimer.current);
			}
		},
		[],
	);
	const persistPreference = useCallback(
		(patch: Partial<FlowPreferenceSnapshot>) => {
			const next = { ...preferenceSnapshot.current, ...patch };
			preferenceSnapshot.current = next;
			writeFlowPreferences(window.localStorage, problemKind, next);
		},
		[problemKind],
	);
	const closeMobilePanel = useCallback(() => {
		const closing = mobilePanel;
		setMobilePanel(undefined);
		window.requestAnimationFrame(() => {
			if (closing === "input") inputPanelTriggerRef.current?.focus();
			if (closing === "inspector") inspectorPanelTriggerRef.current?.focus();
		});
	}, [mobilePanel]);
	useEffect(() => {
		if (mobilePanel === undefined) return undefined;
		const panel =
			mobilePanel === "input"
				? inputPanelRef.current
				: inspectorPanelRef.current;
		const closeButton =
			mobilePanel === "input"
				? inputPanelCloseRef.current
				: inspectorPanelCloseRef.current;
		if (panel === null) return undefined;
		const workspaceSwitcher = panel
			.closest(".workbench-shell")
			?.querySelector<HTMLElement>(".workspace-switcher");
		const background = [
			workspaceSwitcher,
			topbarRef.current,
			inputPanelRef.current,
			canvasPanelRef.current,
			inspectorPanelRef.current,
		].filter(
			(element): element is HTMLElement =>
				element !== null && element !== panel,
		);
		for (const element of background) element.inert = true;
		closeButton?.focus();
		const onKeyDown = (event: KeyboardEvent) => {
			if (event.key === "Escape") {
				event.preventDefault();
				closeMobilePanel();
				return;
			}
			if (event.key !== "Tab") return;
			const focusIsInsidePanel = panel.contains(document.activeElement);
			const focusable = [
				...panel.querySelectorAll<HTMLElement>(
					'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
				),
			].filter(
				(element) =>
					!element.hidden &&
					element.getAttribute("aria-hidden") !== "true" &&
					element.getClientRects().length > 0,
			);
			const first = focusable[0];
			const last = focusable.at(-1);
			if (first === undefined || last === undefined) {
				event.preventDefault();
				panel.focus();
				return;
			}
			if (!focusIsInsidePanel) {
				event.preventDefault();
				(event.shiftKey ? last : first).focus();
				return;
			}
			if (event.shiftKey && document.activeElement === first) {
				event.preventDefault();
				last.focus();
			} else if (!event.shiftKey && document.activeElement === last) {
				event.preventDefault();
				first.focus();
			}
		};
		window.addEventListener("keydown", onKeyDown);
		return () => {
			window.removeEventListener("keydown", onKeyDown);
			for (const element of background) element.inert = false;
		};
	}, [closeMobilePanel, mobilePanel]);
	useEffect(() => {
		const desktop = window.matchMedia("(min-width: 1101px)");
		const leaveDrawerLayout = () => {
			if (desktop.matches) setMobilePanel(undefined);
		};
		leaveDrawerLayout();
		desktop.addEventListener("change", leaveDrawerLayout);
		return () => desktop.removeEventListener("change", leaveDrawerLayout);
	}, []);
	const load = useCallback(() => {
		const input = inputFormat === "dsl" ? flowDsl : scenario;
		const inputModelKind = flowInputModelKind(input, inputFormat);
		const rejection =
			inputModelKind === undefined
				? undefined
				: flowWorkbenchModelRejectionMessage(policy, inputModelKind);
		if (rejection !== undefined) {
			autoplay.current = false;
			setError(rejection);
			setStatus("error");
			return;
		}
		autoplay.current = false;
		resetTraceBoundaryIndex();
		generation.current += 1;
		setError(undefined);
		setStatus("loading");
		post({
			kind: "create",
			generation: generation.current,
			scenario: input,
			discardProvenance:
				inputFormat === "json" && discardProvenanceOnLoad.current,
			flowInputFormat: inputFormat,
		});
	}, [flowDsl, inputFormat, policy, post, resetTraceBoundaryIndex, scenario]);
	const advance = useCallback(
		(continuous: boolean) => {
			navigationDirection.current = 1;
			autoplay.current = continuous;
			setError(undefined);
			setStatus("solving");
			postNextNavigation();
		},
		[postNextNavigation],
	);
	const step = useCallback(() => advance(false), [advance]);
	const solve = useCallback(() => {
		advance(true);
	}, [advance]);
	const pause = useCallback(() => {
		autoplay.current = false;
		navigationDirection.current = 0;
		seekRequestSerial.current += 1;
		if (autoplayTimer.current !== undefined) {
			window.clearTimeout(autoplayTimer.current);
			autoplayTimer.current = undefined;
		}
		setStatus(sceneRef.current === undefined ? "loading" : "ready");
	}, []);
	useEffect(() => {
		if (!active && autoplay.current) pause();
	}, [active, pause]);
	const seekInDirection = useCallback(
		(target: number, direction: -1 | 0 | 1) => {
			autoplay.current = false;
			navigationDirection.current = direction;
			seekRequestSerial.current += 1;
			setError(undefined);
			setStatus("solving");
			post({
				kind: "seek",
				generation: generation.current,
				target,
				requestSerial: seekRequestSerial.current,
			});
		},
		[post],
	);
	const seek = useCallback(
		(target: number) => {
			const cursor = Number(sceneRef.current?.event_id ?? 0);
			seekInDirection(target, target < cursor ? -1 : target > cursor ? 1 : 0);
		},
		[seekInDirection],
	);
	const seekRaw = useCallback(
		(target: number) => seekInDirection(target, 0),
		[seekInDirection],
	);
	const stepBackward = useCallback(
		() =>
			seekInDirection(
				Math.max(0, Number(sceneRef.current?.event_id ?? 1) - 1),
				-1,
			),
		[seekInDirection],
	);
	const dispatchFlowGeneration = useCallback(
		(request: FlowGeneratorReplayRequest) => {
			autoplay.current = false;
			generatorJob.current += 1;
			setGeneratorError(undefined);
			setGeneratorProgress({
				stage: "initializing",
				completedPhases: 0,
				totalPhases: 3,
			});
			setStatus("generating");
			startGenerator({
				kind: "generate",
				jobId: generatorJob.current,
				...request,
			});
		},
		[startGenerator],
	);
	const startFlowGeneration = useCallback(
		(
			spec: string,
			recommendedRunProfile: FlowGeneratorRunProfile,
			recommendedAlgorithmId?: string,
		) => {
			dispatchFlowGeneration({
				scenario,
				spec,
				recommendedRunProfile,
				...(recommendedAlgorithmId === undefined
					? {}
					: { recommendedAlgorithmId }),
			});
		},
		[dispatchFlowGeneration, scenario],
	);
	const generate = useCallback(async () => {
		const { encodeFlowGeneratorSpec } = await import(
			"./flow-generator-family-registry"
		);
		startFlowGeneration(
			encodeFlowGeneratorSpec(
				generatorFormForProblem(generatorForm, problemKind),
				flowGeneratorTargetProblem(
					generatorFormForProblem(generatorForm, problemKind),
					problemKind,
				),
			),
			"trace",
		);
	}, [generatorForm, problemKind, startFlowGeneration]);
	const generateFixturePreset = useCallback(
		(preset: FlowGeneratorFixturePreset, defaultAlgorithmId: string) => {
			startFlowGeneration(
				JSON.stringify(preset.spec),
				preset.recommended_run_profile,
				defaultAlgorithmId,
			);
		},
		[startFlowGeneration],
	);
	const cancelGeneration = useCallback(() => {
		const cancelledWorker = cancelGenerator();
		const cancelledPublication = pendingFlowGeneration.current !== undefined;
		if (!cancelledWorker && !cancelledPublication) return;
		generatorJob.current += 1;
		if (cancelledPublication) {
			pendingFlowGeneration.current = undefined;
			generation.current += 1;
		}
		setGeneratorProgress(undefined);
		setGeneratorError(undefined);
		setStatus(sceneRef.current === undefined ? "error" : "ready");
	}, [cancelGenerator]);
	const openAlgorithmCatalog = useCallback(() => {
		setAlgorithmCatalogError(undefined);
		setAlgorithmCatalogOpen(true);
		if (algorithmCatalog !== undefined) return;
		post({ kind: "get-flow-catalog", generation: generation.current });
	}, [algorithmCatalog, post]);
	const changeGeneratorOpen = useCallback(
		(open: boolean) => {
			if (
				status === "generating" ||
				pendingFlowGeneration.current !== undefined
			)
				return;
			setGeneratorOpen(open);
			if (!open || generatorFixtures !== undefined) return;
			setGeneratorFixtureError(undefined);
			post({
				kind: "get-flow-generator-fixtures",
				generation: generation.current,
			});
		},
		[generatorFixtures, post, status],
	);
	const changeGeneratorForm = useCallback(
		(form: FlowGeneratorForm) => {
			persistPreference({ generator: form });
			setGeneratorForm(form);
		},
		[persistPreference],
	);
	const changeGeneratorFamilyGroup = useCallback(
		(group: FlowPreferenceSnapshot["familyGroup"]) => {
			persistPreference({ familyGroup: group });
			setGeneratorFamilyGroup(group);
		},
		[persistPreference],
	);
	const changeViewMode = useCallback(
		(mode: FlowViewMode) => {
			persistPreference({ viewMode: mode });
			setViewMode(mode);
		},
		[persistPreference],
	);
	const selectAlgorithm = useCallback(
		(entry: FlowAlgorithmCatalogEntry) => {
			let config: Record<string, unknown>;
			try {
				config = defaultFlowAlgorithmConfig(
					entry.id,
					flowScenarioNodeIds(scenario),
				);
			} catch (configError) {
				setAlgorithmCatalogError(
					configError instanceof Error
						? configError.message
						: "Flow algorithm configuration failed",
				);
				return;
			}
			setAlgorithmCatalogError(undefined);
			setStatus("configuring");
			post({
				kind: "set-algorithm",
				generation: generation.current,
				scenario,
				algorithm: entry.id,
				config,
			});
		},
		[post, scenario],
	);

	useEffect(() => {
		resetTraceBoundaryIndex();
		generation.current += 1;
		setError(undefined);
		setStatus("loading");
		post({
			kind: "create",
			generation: generation.current,
			scenario: defaultScenario,
			discardProvenance: false,
			flowInputFormat: "json",
		});
	}, [defaultScenario, post, resetTraceBoundaryIndex]);

	const currentOverlayPresentation = useMemo(
		() =>
			scene === undefined ? undefined : buildFlowOverlayPresentation(scene),
		[scene],
	);
	const currentOverlayViews =
		currentOverlayPresentation?.renderData.overlayViews;
	const configuredSelection = useMemo(
		() => flowScenarioSelection(scenario),
		[scenario],
	);
	const displayedInput = inputFormat === "json" ? scenario : flowDsl;
	const displayedInputByteLength = useMemo(
		() => new TextEncoder().encode(displayedInput).byteLength,
		[displayedInput],
	);
	const flowLod = scene === undefined ? undefined : renderPlan?.level;
	const edgeDisclosure = useMemo(() => {
		if (scene === undefined || renderPlan === undefined) {
			return { label: "0 edges", title: "No graph is loaded." };
		}
		const originalTotal = scene.graph.edges.length;
		const residualTotal = scene.residual_arcs.length;
		if (renderPlan.kind === "entities") {
			if (viewMode === "original") {
				return {
					label: `${renderPlan.edges.length}/${originalTotal} edges shown`,
					title: "Every original edge is rendered individually.",
				};
			}
			if (viewMode === "residual") {
				return {
					label: `${renderPlan.context.residualArcs.length}/${residualTotal} arcs shown`,
					title: "Every residual arc is rendered individually.",
				};
			}
			return {
				label: `${renderPlan.edges.length + renderPlan.context.residualArcs.length}/${originalTotal + residualTotal} edges/arcs shown`,
				title: "Every original edge and residual arc is rendered individually.",
			};
		}
		if (viewMode === "original") {
			return {
				label: `${originalTotal} edges → ${renderPlan.originalEdges.length} bundles`,
				title:
					"Overview preserves every original edge inside deterministic route bundles. Select a bundle to inspect its members.",
			};
		}
		if (viewMode === "residual") {
			return {
				label: `${residualTotal} arcs → ${renderPlan.residualArcs.length} bundles`,
				title:
					"Overview preserves every residual arc inside deterministic route bundles. Select a bundle to inspect its members.",
			};
		}
		return {
			label: `${originalTotal + residualTotal} edges/arcs → ${renderPlan.originalEdges.length + renderPlan.residualArcs.length} bundles`,
			title:
				"Overview preserves every original edge and residual arc inside deterministic route bundles.",
		};
	}, [renderPlan, scene, viewMode]);
	const inspectorViewModel = useFlowInspectorViewModel(
		scene,
		renderPlan,
		currentOverlayPresentation,
	);
	const { flowLodLabel } = inspectorViewModel.overview;
	const { selectedMeanLabel } = inspectorViewModel.algorithmDetail;
	const statusLabel =
		status === "loading"
			? "Validating"
			: status === "generating"
				? "Generating"
				: status === "configuring"
					? "Validating algorithm"
					: status === "solving"
						? "Tracing"
						: status === "edited"
							? "Edited"
							: status === "error"
								? "Input error"
								: status === "limited"
									? "Resource limit"
									: status === "fatal"
										? "Engine error"
										: "Validated";
	const operationBusy =
		publicationLifecycle.kind === "decoding" ||
		status === "loading" ||
		status === "solving" ||
		status === "generating" ||
		status === "configuring" ||
		status === "fatal";
	const playbackOptionsDisabled =
		publicationLifecycle.kind === "decoding" ||
		status === "loading" ||
		status === "generating" ||
		status === "configuring" ||
		status === "fatal";
	useEffect(() => {
		if (algorithmCatalogOpen) {
			algorithmCatalogShouldRestoreFocus.current = true;
			return;
		}
		if (operationBusy || !algorithmCatalogShouldRestoreFocus.current) {
			return;
		}
		algorithmCatalogTriggerRef.current?.focus();
		algorithmCatalogShouldRestoreFocus.current = false;
	}, [algorithmCatalogOpen, operationBusy]);
	const canAdvance =
		status === "ready" &&
		publicationLifecycle.kind !== "decoding" &&
		(scene?.solve_status === "ready" || scene?.solve_status === "running");
	const eventCursor = (() => {
		if (scene === undefined) return undefined;
		const cursor = Number(scene.event_id);
		return Number.isSafeInteger(cursor) ? cursor : undefined;
	})();
	const eventExtent = (() => {
		if (scene === undefined) return undefined;
		const extent = Number(scene.event_count);
		return Number.isSafeInteger(extent) ? extent : undefined;
	})();
	const visibleBoundaryPositions =
		effectivePlaybackGranularity === "phase"
			? traceBoundaryInventory.current.phasePositions
			: traceBoundaryInventory.current.operationPositions;
	const boundaryInventoryComplete =
		eventExtent !== undefined &&
		traceBoundaryInventory.current.minimumByRawPosition.size === eventExtent;
	const boundaryInventoryPrefixEnd = traceBoundaryInventory.current.prefixEnd;
	const stepForwardAtVisibleGranularity = useCallback(() => {
		if (
			effectivePlaybackGranularity === "micro" ||
			!boundaryInventoryComplete ||
			eventCursor === undefined
		) {
			step();
			return;
		}
		const target = flowAdjacentVisibleBoundary(
			visibleBoundaryPositions,
			eventCursor,
			1,
		);
		if (target === undefined) {
			step();
			return;
		}
		seek(target);
	}, [
		boundaryInventoryComplete,
		effectivePlaybackGranularity,
		eventCursor,
		seek,
		step,
		visibleBoundaryPositions,
	]);
	const stepBackwardAtVisibleGranularity = useCallback(() => {
		if (
			effectivePlaybackGranularity === "micro" ||
			!boundaryInventoryComplete ||
			eventCursor === undefined
		) {
			stepBackward();
			return;
		}
		const target = flowAdjacentVisibleBoundary(
			visibleBoundaryPositions,
			eventCursor,
			-1,
		);
		if (target === undefined) {
			stepBackward();
			return;
		}
		seek(target);
	}, [
		boundaryInventoryComplete,
		effectivePlaybackGranularity,
		eventCursor,
		seek,
		stepBackward,
		visibleBoundaryPositions,
	]);
	const changePlaybackGranularity = useCallback(
		(next: FlowPlaybackGranularity) => {
			setPlaybackModeNotice(undefined);
			preferredPlaybackGranularityRef.current = next;
			persistPreference({ granularity: next });
			setPreferredPlaybackGranularity(next);
		},
		[persistPreference],
	);
	const changePlaybackSpeed = useCallback(
		(next: number) => {
			setPlaybackSpeed(next);
			if (autoplay.current && autoplayTimer.current !== undefined) {
				scheduleAutoplayStep(next);
			}
		},
		[scheduleAutoplayStep],
	);
	useFlowKeyboardControls({
		busy: operationBusy,
		canStepForward: canAdvance,
		cursor: eventCursor ?? 0,
		enabled: active,
		extent: eventExtent ?? 0,
		playing: autoplay.current,
		onFit: () => setCanvasFitRequest((current) => current + 1),
		onPause: pause,
		onPlay: solve,
		onSeek: seek,
		onSpeedChange: changePlaybackSpeed,
		onStepBackward: stepBackwardAtVisibleGranularity,
		onStepForward: stepForwardAtVisibleGranularity,
		speed: playbackSpeed,
	});
	return (
		<div className="flow-shell">
			<header ref={topbarRef} className="topbar flow-topbar">
				<div className="brand-block">
					<span className="brand-mark" aria-hidden="true" />
					<div>
						<p className="eyebrow">ALGORITHM WORKBENCH</p>
						<h1>{flowProblemTitle(problemKind)}</h1>
					</div>
				</div>
				<div className="operation-strip">
					<span className={`flow-status flow-status-${status}`}>
						{statusLabel}
					</span>
					<button
						ref={algorithmCatalogTriggerRef}
						type="button"
						className="quiet-button"
						onClick={openAlgorithmCatalog}
						disabled={operationBusy}
					>
						Algorithm
					</button>
					<button
						ref={generatorTriggerRef}
						type="button"
						className="quiet-button"
						onClick={() => {
							setGeneratorError(undefined);
							changeGeneratorOpen(true);
						}}
						disabled={operationBusy}
					>
						Generate
					</button>
					<button
						type="button"
						className="quiet-button"
						onClick={load}
						disabled={operationBusy}
					>
						Load
					</button>
					<button
						type="button"
						className="primary-button"
						onClick={autoplay.current ? pause : solve}
						disabled={!autoplay.current && !canAdvance}
					>
						{autoplay.current ? "Pause" : "Run trace"}
					</button>
					<div className="flow-mobile-panel-controls">
						<button
							ref={inputPanelTriggerRef}
							type="button"
							className="quiet-button"
							aria-controls={inputPanelId}
							aria-expanded={mobilePanel === "input"}
							onClick={() =>
								setMobilePanel((current) =>
									current === "input" ? undefined : "input",
								)
							}
						>
							Input
						</button>
						<button
							ref={inspectorPanelTriggerRef}
							type="button"
							className="quiet-button"
							aria-controls={inspectorPanelId}
							aria-expanded={mobilePanel === "inspector"}
							onClick={() =>
								setMobilePanel((current) =>
									current === "inspector" ? undefined : "inspector",
								)
							}
						>
							Inspector
						</button>
					</div>
				</div>
			</header>

			<main className="flow-workspace">
				{/* biome-ignore lint/a11y/useAriaPropsSupportedByRole: role and aria-modal switch atomically in the mobile-dialog state. */}
				<section
					ref={inputPanelRef}
					id={inputPanelId}
					className={`flow-input-panel${mobilePanel === "input" ? " is-mobile-open" : ""}`}
					aria-label="Flow Scenario input"
					role={mobilePanel === "input" ? "dialog" : undefined}
					aria-modal={mobilePanel === "input" ? true : undefined}
					tabIndex={mobilePanel === "input" ? -1 : undefined}
				>
					<div className="panel-heading">
						<div>
							<p className="eyebrow">INPUT</p>
							<h2>{inputFormat === "json" ? "Scenario JSON" : "Flow DSL"}</h2>
						</div>
						<div className="panel-heading-actions flow-input-heading-actions">
							<small className="flow-input-bytes">
								{displayedInputByteLength.toLocaleString()} bytes
							</small>
							<button
								ref={inputPanelCloseRef}
								type="button"
								className="quiet-button flow-mobile-close"
								onClick={closeMobilePanel}
								aria-label="Close input panel"
							>
								Close
							</button>
						</div>
					</div>
					<fieldset className="flow-input-format">
						<legend className="visually-hidden">Input format</legend>
						<button
							type="button"
							className="quiet-button flow-input-format-button"
							aria-pressed={inputFormat === "json"}
							onClick={() => setInputFormat("json")}
							disabled={operationBusy}
						>
							JSON
						</button>
						<button
							type="button"
							className="quiet-button flow-input-format-button"
							aria-pressed={inputFormat === "dsl"}
							onClick={() => setInputFormat("dsl")}
							disabled={operationBusy}
						>
							Flow DSL
						</button>
						<small>
							{inputFormat === "dsl"
								? "Strict line and column diagnostics"
								: "Preserves complete replay metadata"}
						</small>
					</fieldset>
					<label className="visually-hidden" htmlFor={scenarioEditorId}>
						{inputFormat === "json" ? "Flow Scenario JSON" : "Flow DSL"}
					</label>
					<div className="flow-input-body">
						<textarea
							ref={scenarioEditorRef}
							id={scenarioEditorId}
							className="flow-scenario-editor"
							spellCheck={false}
							value={inputFormat === "json" ? scenario : flowDsl}
							onChange={(event) => {
								setGeneratorResult(undefined);
								if (inputFormat === "json") {
									setScenario(event.target.value);
									discardProvenanceOnLoad.current = true;
								} else {
									setFlowDsl(event.target.value);
								}
								setStatus("edited");
							}}
						/>
					</div>
				</section>

				<section
					ref={canvasPanelRef}
					className={flowOverlayCanvasClassName(
						currentOverlayViews,
						scene?.model.kind === "parametric-max-flow",
					)}
					aria-label="Flow graph visualization"
				>
					<div className="canvas-heading">
						<div>
							<p className="eyebrow">VALIDATED SCENE</p>
							<h2>
								{scene === undefined
									? "Waiting for input"
									: modelLabel(scene.model)}
							</h2>
						</div>
						<div className="canvas-meta">
							<fieldset className="flow-view-switcher">
								<legend className="visually-hidden">Graph view</legend>
								{(["original", "residual", "both"] as const).map((mode) => (
									<button
										type="button"
										className="flow-view-button"
										key={mode}
										aria-pressed={
											(scene?.model.kind === "parametric-max-flow"
												? "original"
												: viewMode) === mode
										}
										onClick={() => changeViewMode(mode)}
										disabled={
											scene?.model.kind === "parametric-max-flow" &&
											mode !== "original"
										}
									>
										{mode === "original"
											? "Original"
											: mode === "residual"
												? "Residual"
												: "Both"}
									</button>
								))}
							</fieldset>
							<span className={`flow-lod-level flow-lod-${flowLod ?? "none"}`}>
								{flowLodLabel}
							</span>
							<span className="flow-node-count">
								{scene?.graph.nodes.length ?? 0} nodes
							</span>
							<span className="flow-edge-count" title={edgeDisclosure.title}>
								{edgeDisclosure.label}
							</span>
							<span className="flow-event-count">
								event {scene?.event_id ?? "—"}/{scene?.event_count ?? "—"}
							</span>
							<FlowEncodingKey problemKind={problemKind} />
							{generatorResult !== undefined && (
								<details className="flow-generation-details" role="status">
									<summary>
										Generated {generatorResult.stats.node_count} nodes ·{" "}
										{generatorResult.stats.edge_count} edges
									</summary>
									<span>
										{generatorResult.familyId} · seed {generatorResult.seed}
									</span>
								</details>
							)}
						</div>
					</div>
					<DynamicEibfsUpdateStrip
						overlay={currentOverlayViews?.dynamicEibfs}
					/>
					<FlowOverlayRichStatusRegion
						scene={scene}
						views={currentOverlayViews}
						presentation={currentOverlayPresentation}
					/>
					{scene?.model.kind === "parametric-max-flow" &&
						currentOverlayViews?.parametric !== undefined && (
							<ParametricAnalysisPanel
								scene={scene}
								overlay={currentOverlayViews.parametric}
							/>
						)}
					<div className="flow-canvas-viewport">
						{scene === undefined ? (
							<div className="canvas-loading" role="status">
								{error ?? "Validating scenario…"}
							</div>
						) : (
							<FlowGraph
								scene={scene}
								graphRevision={graphRevision}
								problemKind={problemKind}
								fitRequest={canvasFitRequest}
								viewMode={
									scene.model.kind === "parametric-max-flow"
										? "original"
										: viewMode
								}
								selection={selectedEntity}
								onSelectionChange={setSelectedEntity}
								onRenderPlanChange={setRenderPlan}
							/>
						)}
						{scene !== undefined && (
							<FlowMobileEdgeSelection
								scene={scene}
								selection={selectedEntity}
								showsCost={policy.showsCost}
								onSelectionChange={setSelectedEntity}
							/>
						)}
					</div>
					<div
						className={`flow-message flow-message-compact ${status === "limited" ? "flow-message-limit" : error === undefined ? "" : "flow-message-error"}`}
						aria-live="polite"
					>
						<div>
							<strong
								className={
									error === undefined && scene?.trace_event !== undefined
										? "flow-event-title"
										: undefined
								}
							>
								{error ??
									(scene?.model.kind === "parametric-max-flow" &&
									currentOverlayViews?.parametric !== undefined ? (
										<>
											<span className="flow-event-action">
												{flowEventCaption(scene)}
											</span>
											<span className="flow-event-exact-detail">
												· λ{" "}
												{formatFlowRational(
													currentOverlayViews.parametric.parameter,
												)}
											</span>
										</>
									) : scene?.trace_event === undefined ? (
										problemKind === "max-flow" ? (
											"Capacity, flow, and direction use independent channels"
										) : (
											"Capacity, flow, direction, and cost use independent channels"
										)
									) : (
										<>
											<span className="flow-event-action">
												{flowEventCaption(scene)}
											</span>
											{selectedMeanLabel !== undefined && (
												<span className="flow-event-exact-detail">
													· mean {selectedMeanLabel}
												</span>
											)}
										</>
									))}
							</strong>
						</div>
					</div>
				</section>

				<FlowInspectorPanel
					panelId={inspectorPanelId}
					mobilePanel={mobilePanel}
					scene={scene}
					renderPlan={renderPlan}
					selectedEntity={selectedEntity}
					setSelectedEntity={setSelectedEntity}
					inspectorPanelCloseRef={inspectorPanelCloseRef}
					inspectorPanelRef={inspectorPanelRef}
					closeMobilePanel={closeMobilePanel}
					presentation={currentOverlayPresentation}
					viewModel={inspectorViewModel}
				/>
			</main>
			<FlowTimeline
				cursor={eventCursor ?? 0}
				extent={eventExtent ?? 0}
				navigationDisabled={operationBusy}
				optionsDisabled={playbackOptionsDisabled}
				canStepForward={canAdvance}
				granularity={effectivePlaybackGranularity}
				currentBoundary={scene?.trace_event?.minimum_granularity}
				traceSteps={scene?.trace_steps}
				workProgress={scene?.trace_event_semantics?.work_progress}
				modeNotice={playbackModeNotice}
				visibleBoundaryPositions={visibleBoundaryPositions}
				boundaryInventoryComplete={boundaryInventoryComplete}
				boundaryInventoryPrefixEnd={boundaryInventoryPrefixEnd}
				speed={playbackSpeed}
				onGranularityChange={changePlaybackGranularity}
				onSeek={seek}
				onRawSeek={seekRaw}
				onSpeedChange={changePlaybackSpeed}
				onStepBackward={stepBackwardAtVisibleGranularity}
				onStepForward={stepForwardAtVisibleGranularity}
			/>
			{generatorOpen && (
				<Suspense fallback={null}>
					<FlowGeneratorDialog
						busy={
							status === "generating" ||
							pendingFlowGeneration.current !== undefined
						}
						error={generatorError}
						fixtureError={generatorFixtureError}
						fixtures={generatorFixtures}
						form={generatorForm}
						familyGroup={generatorFamilyGroup}
						onChange={changeGeneratorForm}
						onFamilyGroupChange={changeGeneratorFamilyGroup}
						onCancelGeneration={cancelGeneration}
						onGenerate={generate}
						onGenerateFixturePreset={generateFixturePreset}
						onOpenChange={changeGeneratorOpen}
						open={generatorOpen}
						progress={generatorProgress}
						problemKind={problemKind}
					/>
				</Suspense>
			)}
			{algorithmCatalogOpen && (
				<Suspense fallback={null}>
					<FlowAlgorithmCatalogDialog
						conformance={algorithmConformance}
						entries={algorithmCatalog}
						error={algorithmCatalogError}
						workspaceProblem={problemKind}
						modelKind={configuredSelection?.modelKind}
						nodeCount={configuredSelection?.nodeCount}
						edgeCount={configuredSelection?.edgeCount}
						graphShape={configuredSelection?.graphShape}
						admissionFacts={configuredSelection?.admissionFacts}
						dynamicUpdates={configuredSelection?.dynamicUpdates}
						currentAlgorithmId={configuredSelection?.algorithmId}
						editable={inputFormat === "json"}
						onOpenChange={(open) => {
							if (status !== "configuring") setAlgorithmCatalogOpen(open);
						}}
						onSelect={selectAlgorithm}
						open={algorithmCatalogOpen}
					/>
				</Suspense>
			)}
		</div>
	);
}
