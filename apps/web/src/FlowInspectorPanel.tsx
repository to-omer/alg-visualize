import type { Dispatch, RefObject, SetStateAction } from "react";
import { FlowEntityNavigator } from "./FlowEntityNavigator";
import {
	FlowInspectorAlgorithmDetailSection,
	FlowInspectorScenarioSection,
} from "./FlowInspectorAlgorithmSections";
import { FlowInspectorLegendSection } from "./FlowInspectorLegendSection";
import {
	FlowInspectorContinuousSection,
	FlowInspectorOverviewSection,
} from "./FlowInspectorSummarySections";
import { FlowOverlayRegistryInspector } from "./FlowOverlayRegistryInspector";
import { FlowStepEvidenceSection } from "./FlowStepEvidenceSection";
import type { FlowEntitySelection } from "./flow-entity-navigator";
import type { FlowOverlayPresentation } from "./flow-overlay-presentation";
import type { FlowRenderPlan } from "./flow-render-plan";
import type { FlowCurrentSceneV9 } from "./flow-scene";
import type { FlowInspectorViewModel } from "./use-flow-inspector-view-model";

export type FlowInspectorPanelShellProps = Readonly<{
	panelId: string;
	mobilePanel: "input" | "inspector" | undefined;
	scene: FlowCurrentSceneV9 | undefined;
	renderPlan: FlowRenderPlan | undefined;
	selectedEntity: FlowEntitySelection | undefined;
	setSelectedEntity: Dispatch<SetStateAction<FlowEntitySelection | undefined>>;
	inspectorPanelCloseRef: RefObject<HTMLButtonElement | null>;
	inspectorPanelRef: RefObject<HTMLElement | null>;
	closeMobilePanel: () => void;
	presentation: FlowOverlayPresentation | undefined;
}>;

export type FlowInspectorPanelProps = FlowInspectorPanelShellProps &
	Readonly<{ viewModel: FlowInspectorViewModel }>;

export function FlowInspectorPanel(props: FlowInspectorPanelProps) {
	const {
		panelId,
		mobilePanel,
		scene,
		renderPlan,
		selectedEntity,
		setSelectedEntity,
		inspectorPanelCloseRef,
		inspectorPanelRef,
		closeMobilePanel,
		presentation,
		viewModel,
	} = props;
	return (
		<>
			{/* biome-ignore lint/a11y/useAriaPropsSupportedByRole: role and aria-modal switch atomically in the mobile-dialog state. */}
			<aside
				ref={inspectorPanelRef}
				id={panelId}
				className={`flow-inspector-panel${mobilePanel === "inspector" ? " is-mobile-open" : ""}`}
				aria-label="Flow scene inspector"
				role={mobilePanel === "inspector" ? "dialog" : undefined}
				aria-modal={mobilePanel === "inspector" ? true : undefined}
				tabIndex={mobilePanel === "inspector" ? -1 : undefined}
			>
				<div className="panel-heading">
					<div>
						<p className="eyebrow">INSPECTOR</p>
						<h2>Step inspector</h2>
					</div>
					<button
						ref={inspectorPanelCloseRef}
						type="button"
						className="quiet-button flow-mobile-close"
						onClick={closeMobilePanel}
						aria-label="Close inspector panel"
					>
						Close
					</button>
				</div>
				<div className="flow-inspector-content">
					<FlowStepEvidenceSection scene={scene} />
					<FlowInspectorOverviewSection
						scene={scene}
						presentation={presentation}
						{...viewModel.overview}
					/>
					<FlowEntityNavigator
						scene={scene}
						plan={renderPlan}
						selection={selectedEntity}
						onSelectionChange={setSelectedEntity}
					/>
					<FlowOverlayRegistryInspector presentation={presentation} />
					<FlowInspectorContinuousSection
						scene={scene}
						presentation={presentation}
						{...viewModel.continuous}
					/>
					<FlowInspectorScenarioSection
						scene={scene}
						presentation={presentation}
						{...viewModel.scenario}
					/>
					<FlowInspectorAlgorithmDetailSection
						scene={scene}
						presentation={presentation}
						{...viewModel.algorithmDetail}
					/>
					<FlowInspectorLegendSection
						scene={scene}
						presentation={presentation}
					/>
				</div>
			</aside>
		</>
	);
}
