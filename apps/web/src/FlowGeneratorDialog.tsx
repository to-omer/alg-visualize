import * as Dialog from "@radix-ui/react-dialog";
import { useMemo, useState } from "react";
import { FlowGeneratorShapePreview } from "./FlowGeneratorShapePreview";
import { flowScopedDomId, useFlowDomIdScope } from "./flow-dom-id";
import {
	ASSIGNMENT_SHAPE_OPTIONS,
	type AssignmentMatrixShapeId,
	applyAssignmentShape,
	applyGridgraphPreset,
	applyNetgenPreset,
	applyTransportationShape,
	applyWashingtonMatchingPreset,
	applyWashingtonSquareMeshPreset,
	assignmentCostLabels,
	assignmentParameterLabels,
	capacityFieldLabels,
	costFieldLabels,
	type FlowGeneratorFamilyDescriptor,
	type FlowGeneratorFamilyId,
	type FlowGeneratorFieldKey,
	type FlowGeneratorForm,
	flowGeneratorFamilyDescriptor,
	GRIDGRAPH_PRESET_OPTIONS,
	type GridgraphPresetId,
	NETGEN_PRESET_OPTIONS,
	TRANSPORTATION_SHAPE_OPTIONS,
	type TransportationTableShapeId,
	transportationCostLabels,
	transportationParameterLabel,
	WASHINGTON_MATCHING_PRESET_OPTIONS,
	WASHINGTON_PRESET_OPTIONS,
	WASHINGTON_SQUARE_MESH_PRESET_OPTIONS,
	type WashingtonMatchingPresetId,
	type WashingtonPresetId,
	type WashingtonSquareMeshPresetId,
} from "./flow-generator-family-registry";
import {
	FLOW_GENERATOR_PICKER_GROUP_LABELS,
	FLOW_GENERATOR_PICKER_GROUPS,
	type FlowGeneratorFixture,
	type FlowGeneratorPickerGroup,
	filterFlowGeneratorFixtures,
	flowGeneratorFamilyDisplayName,
	flowGeneratorFixtureKind,
} from "./flow-generator-fixture";
import type { FlowGeneratorProgress } from "./flow-generator-worker-protocol";
import {
	type FlowWorkbenchProblemKind,
	flowFixtureAdaptationLabel,
	flowFixtureDisabledReason,
	flowFixtureMatchesProblem,
	flowFixturePresetDisabledReason,
	flowGeneratorFormAdaptationLabel,
	flowGeneratorFormDisabledReason,
	flowNetgenPresetDisabledReason,
	flowProblemTitle,
} from "./flow-workbench-problem";
import "./flow-dialog-enhancements.css";

export * from "./flow-generator-family-registry";

type Props = {
	busy: boolean;
	error: string | undefined;
	fixtureError: string | undefined;
	fixtures: readonly FlowGeneratorFixture[] | undefined;
	form: FlowGeneratorForm;
	familyGroup: FlowGeneratorPickerGroup;
	onChange: (form: FlowGeneratorForm) => void;
	onFamilyGroupChange: (group: FlowGeneratorPickerGroup) => void;
	onCancelGeneration: () => void;
	onGenerate: () => void;
	onGenerateFixturePreset: (
		preset: FlowGeneratorFixture["presets"][number],
		defaultAlgorithmId: string,
	) => void;
	onOpenChange: (open: boolean) => void;
	open: boolean;
	progress: FlowGeneratorProgress | undefined;
	problemKind: FlowWorkbenchProblemKind;
};

const FIXTURE_KIND_ORDER = [
	"Structural",
	"Random",
	"Specialized",
	"Benchmark",
	"Stress",
	"Worst case",
] as const;

function generationStageLabel(progress: FlowGeneratorProgress): string {
	switch (progress.stage) {
		case "initializing":
			return "Preparing generator";
		case "materializing":
			return "Building graph";
		case "validating":
			return "Validating scenario and digest";
	}
}

function applyWashingtonLevelPreset(
	descriptor: FlowGeneratorFamilyDescriptor,
	form: FlowGeneratorForm,
	preset: Exclude<WashingtonPresetId, "custom">,
): FlowGeneratorForm {
	const applyPreset = descriptor.applyWashingtonLevelPreset;
	if (applyPreset === undefined) {
		throw new Error(
			`Flow generator family ${descriptor.id} declares washington-level without a preset callback`,
		);
	}
	return applyPreset(form, preset);
}

export function FlowGeneratorDialog({
	busy,
	error,
	fixtureError,
	fixtures,
	form,
	familyGroup,
	onChange,
	onFamilyGroupChange,
	onCancelGeneration,
	onGenerate,
	onGenerateFixturePreset,
	onOpenChange,
	open,
	progress,
	problemKind,
}: Props) {
	const idScope = useFlowDomIdScope("flow-generator-dialog");
	const validationStatusId = flowScopedDomId(idScope, "validation-status");
	const [familyQuery, setFamilyQuery] = useState("");
	const [familyLibraryOpen, setFamilyLibraryOpen] = useState(false);
	const filteredFixtures = useMemo(
		() => filterFlowGeneratorFixtures(fixtures ?? [], familyQuery, familyGroup),
		[familyGroup, familyQuery, fixtures],
	);
	const selectedFixture = fixtures?.find(
		(fixture) =>
			fixture.family_id === form.family &&
			flowFixtureMatchesProblem(fixture, problemKind),
	);
	const visibleFixtures = useMemo(() => {
		if (
			selectedFixture === undefined ||
			filteredFixtures.some(
				(fixture) => fixture.family_id === selectedFixture.family_id,
			)
		) {
			return filteredFixtures;
		}
		return [selectedFixture, ...filteredFixtures];
	}, [filteredFixtures, selectedFixture]);
	const availableFixtureCount = filteredFixtures.filter(
		(fixture) => flowFixtureDisabledReason(fixture, problemKind) === undefined,
	).length;
	const recommendedAlgorithms = selectedFixture?.algorithm_compatibility
		.filter(
			(entry) =>
				flowFixtureAdaptationLabel(selectedFixture, problemKind) ===
					undefined && entry.state === "recommended",
		)
		.map((entry) => entry.algorithm_id);
	const selectedAdaptation =
		selectedFixture === undefined
			? undefined
			: flowGeneratorFormAdaptationLabel(form, problemKind);
	const descriptor = flowGeneratorFamilyDescriptor(form.family);
	const fixturePresetDisabledReason =
		selectedFixture === undefined
			? undefined
			: flowFixturePresetDisabledReason(selectedFixture, problemKind);
	const fixturePresetStatusId = flowScopedDomId(
		idScope,
		"fixture-preset-status",
	);
	const descriptorStatusId =
		descriptor.statusId === undefined
			? undefined
			: flowScopedDomId(idScope, descriptor.statusId);
	const statusDescription = (localId: string) =>
		`${flowScopedDomId(idScope, localId)} ${validationStatusId}`;
	const update = <Key extends keyof FlowGeneratorForm>(
		key: Key,
		value: FlowGeneratorForm[Key],
	) => {
		onChange(descriptor.customize(form, key, value));
	};
	const parameterControls = descriptor.parameters(form);
	const estimate = descriptor.estimate(form);
	const validationError =
		descriptor.validation(form) ??
		flowGeneratorFormDisabledReason(form, problemKind);
	const fixedConstruction = descriptor.fixedConstruction(form);
	const generatorFieldA11y = (field: FlowGeneratorFieldKey) => ({
		"aria-describedby": validationStatusId,
		"aria-invalid": descriptor.fieldInvalid(form, field),
		"data-generator-field": field,
	});

	return (
		<Dialog.Root
			open={open}
			onOpenChange={(nextOpen) => {
				if (!nextOpen) {
					setFamilyQuery("");
					setFamilyLibraryOpen(false);
				}
				onOpenChange(nextOpen);
			}}
		>
			<Dialog.Portal>
				<Dialog.Overlay className="dialog-overlay" />
				<Dialog.Content
					className="dialog-content flow-generator-dialog"
					aria-busy={busy}
				>
					<Dialog.Title>
						Generate {flowProblemTitle(problemKind)} graph
					</Dialog.Title>
					<Dialog.Description className="visually-hidden">
						Choose a reproducible shape, seed, and edge distributions. A
						successful generation is loaded immediately.
					</Dialog.Description>
					<div className="flow-generator-dialog-scroll-region">
						{error !== undefined && (
							<p className="dialog-error" role="alert">
								{error}
							</p>
						)}
						{fixtureError !== undefined && (
							<p className="dialog-error" role="alert">
								{fixtureError}
							</p>
						)}
						{fixtures === undefined && fixtureError === undefined && (
							<p className="generator-fixture-loading" role="status">
								Loading generator families…
							</p>
						)}
						<fieldset
							className="generator-config"
							disabled={busy || fixtures === undefined}
						>
							<div className="generator-grid">
								<div className="flow-generator-shape-selector">
									<div>
										<span>Graph shape</span>
										<strong>
											{selectedFixture === undefined
												? "Choose a compatible shape"
												: flowGeneratorFamilyDisplayName(
														selectedFixture.family_id,
													)}
										</strong>
										<small>
											{selectedFixture === undefined
												? `${availableFixtureCount} compatible families`
												: `${selectedFixture.layout_class} · ${selectedAdaptation ?? selectedFixture.model}`}
										</small>
									</div>
									<button
										type="button"
										className="quiet-button"
										aria-expanded={familyLibraryOpen}
										onClick={() => setFamilyLibraryOpen((current) => !current)}
									>
										{familyLibraryOpen ? "Close shape browser" : "Change shape"}
									</button>
								</div>
								{familyLibraryOpen && (
									<>
										<div className="flow-generator-discovery">
											<label>
												Search families
												<input
													type="search"
													value={familyQuery}
													onChange={(event) =>
														setFamilyQuery(event.target.value)
													}
													placeholder="grid, bipartite, Dinic…"
												/>
											</label>
											<label>
												Category
												<select
													value={familyGroup}
													onChange={(event) =>
														onFamilyGroupChange(
															event.target.value as FlowGeneratorPickerGroup,
														)
													}
												>
													{FLOW_GENERATOR_PICKER_GROUPS.map((group) => (
														<option key={group} value={group}>
															{FLOW_GENERATOR_PICKER_GROUP_LABELS[group]}
														</option>
													))}
												</select>
											</label>
											<small
												className="flow-generator-search-status"
												role="status"
												aria-live="polite"
											>
												{filteredFixtures.length} / {fixtures?.length ?? 0}{" "}
												families · {availableFixtureCount} available
												{visibleFixtures.length !== filteredFixtures.length
													? " · selected family retained"
													: ""}
											</small>
										</div>
										<fieldset className="flow-generator-family-picker">
											<legend>Graph shape</legend>
											{FIXTURE_KIND_ORDER.map((kind) => {
												const options = visibleFixtures.filter(
													(fixture) =>
														flowGeneratorFixtureKind(fixture) === kind,
												);
												if (options.length === 0) return null;
												return (
													<section
														key={kind}
														className="flow-generator-family-group"
													>
														<h3>{kind}</h3>
														<div className="flow-generator-family-list">
															{options.map((fixture) => {
																const disabledReason =
																	flowFixtureDisabledReason(
																		fixture,
																		problemKind,
																	);
																const adaptation = flowFixtureAdaptationLabel(
																	fixture,
																	problemKind,
																);
																const selected =
																	fixture.family_id === form.family;
																const nameId = flowScopedDomId(
																	idScope,
																	`family-${fixture.family_id}`,
																);
																const reasonId = flowScopedDomId(
																	idScope,
																	`family-reason-${fixture.family_id}`,
																);
																return (
																	<button
																		type="button"
																		key={fixture.family_id}
																		className={`flow-generator-family-option${selected ? " is-selected" : ""}${disabledReason === undefined ? "" : " is-disabled"}`}
																		aria-pressed={selected}
																		aria-disabled={disabledReason !== undefined}
																		aria-labelledby={nameId}
																		aria-describedby={reasonId}
																		data-family-id={fixture.family_id}
																		data-selection-state={
																			disabledReason === undefined
																				? "available"
																				: "disabled"
																		}
																		onClick={() => {
																			if (disabledReason !== undefined) return;
																			const family =
																				fixture.family_id as FlowGeneratorFamilyId;
																			const next =
																				family === "netgen-skeleton"
																					? applyNetgenPreset(
																							form,
																							problemKind === "max-flow"
																								? "single-source-max-flow"
																								: "general-min-cost",
																						)
																					: flowGeneratorFamilyDescriptor(
																							family,
																						).defaults(form);
																			onChange(next);
																			setFamilyQuery("");
																			setFamilyLibraryOpen(false);
																		}}
																	>
																		<span id={nameId}>
																			<strong>
																				{flowGeneratorFamilyDisplayName(
																					fixture.family_id,
																				)}
																			</strong>
																			<small>{fixture.layout_class}</small>
																		</span>
																		<small id={reasonId}>
																			{disabledReason ??
																				adaptation ??
																				(selected ? "Selected" : "Available")}
																		</small>
																	</button>
																);
															})}
														</div>
													</section>
												);
											})}
										</fieldset>
									</>
								)}
								{selectedFixture !== undefined && (
									<div className="generator-fixture-summary" aria-live="polite">
										<FlowGeneratorShapePreview fixture={selectedFixture} />
										<details className="generator-fixture-tools">
											<summary>Presets &amp; generator notes</summary>
											<div className="generator-fixture-details">
												<strong>
													{flowGeneratorFamilyDisplayName(
														selectedFixture.family_id,
													)}
												</strong>
												<span>
													{selectedAdaptation ?? selectedFixture.model} ·{" "}
													{selectedFixture.difficulty}
												</span>
												<small>
													{selectedAdaptation === undefined
														? `Recommended: ${recommendedAlgorithms?.join(" / ") ?? "—"}`
														: `Use any compatible ${flowProblemTitle(problemKind)} algorithm`}
												</small>
												<p>{selectedFixture.purpose}</p>
												<dl
													className="generator-fixture-provenance"
													aria-label="Generator provenance and reproducibility"
												>
													<div>
														<dt>Origin</dt>
														<dd>{selectedFixture.origin}</dd>
													</div>
													<div>
														<dt>Sampling</dt>
														<dd>{selectedFixture.sampling}</dd>
													</div>
													<div>
														<dt>Source ID</dt>
														<dd>
															<code>{selectedFixture.source_id}</code>
														</dd>
													</div>
													<div>
														<dt>Tags</dt>
														<dd>{selectedFixture.tags.join(" · ")}</dd>
													</div>
												</dl>
												<small>{selectedFixture.admission_note}</small>
												<div className="generator-fixture-preset-actions">
													{fixturePresetDisabledReason !== undefined && (
														<small id={fixturePresetStatusId}>
															Canonical benchmark presets are unavailable:{" "}
															{fixturePresetDisabledReason}. Configure this
															topology above for the current workspace instead.
														</small>
													)}
													{selectedFixture.presets.map((preset, index) => (
														<button
															type="button"
															key={preset.purpose}
															aria-disabled={
																fixturePresetDisabledReason !== undefined
															}
															aria-describedby={
																fixturePresetDisabledReason === undefined
																	? undefined
																	: fixturePresetStatusId
															}
															className={
																fixturePresetDisabledReason === undefined
																	? undefined
																	: "is-disabled"
															}
															onClick={() => {
																if (fixturePresetDisabledReason !== undefined)
																	return;
																onGenerateFixturePreset(
																	preset,
																	selectedFixture.default_algorithm_id,
																);
															}}
														>
															{[
																"Readable trace",
																"Standard comparison",
																"Practical boundary",
															][index] ?? preset.label}
														</button>
													))}
												</div>
											</div>
										</details>
									</div>
								)}
								<label>
									Seed
									<input
										{...generatorFieldA11y("seed")}
										value={form.seed}
										inputMode="numeric"
										onChange={(event) => update("seed", event.target.value)}
									/>
								</label>
								{descriptor.features.has("assignment-shape") && (
									<>
										<label>
											Matrix shape
											<select
												value={form.assignmentShape}
												onChange={(event) =>
													onChange(
														applyAssignmentShape(
															form,
															event.target.value as AssignmentMatrixShapeId,
														),
													)
												}
											>
												{ASSIGNMENT_SHAPE_OPTIONS.map((option) => (
													<option key={option.id} value={option.id}>
														{option.label} · {option.detail}
													</option>
												))}
											</select>
										</label>
										<label>
											Objective
											<select
												value={form.assignmentObjective}
												onChange={(event) =>
													update(
														"assignmentObjective",
														event.target.value as "minimize" | "maximize",
													)
												}
											>
												<option value="minimize">Minimize cost</option>
												<option value="maximize">Maximize cost</option>
											</select>
										</label>
									</>
								)}
								{descriptor.features.has("transportation-shape") && (
									<label>
										Transportation table shape
										<select
											aria-label="Transportation table shape"
											value={form.transportationShape}
											onChange={(event) =>
												onChange(
													applyTransportationShape(
														form,
														event.target.value as TransportationTableShapeId,
													),
												)
											}
										>
											{TRANSPORTATION_SHAPE_OPTIONS.map((option) => (
												<option key={option.id} value={option.id}>
													{option.label} · {option.detail}
												</option>
											))}
										</select>
									</label>
								)}
								{descriptor.features.has("netgen") && (
									<fieldset
										className="flow-generator-netgen-presets"
										aria-describedby={statusDescription(
											"flow-generator-netgen-kind",
										)}
									>
										<legend>Use-case preset</legend>
										<div className="flow-generator-netgen-preset-list">
											{[
												...NETGEN_PRESET_OPTIONS,
												{ id: "custom" as const, label: "Custom" },
											].map((option) => {
												const disabledReason = flowNetgenPresetDisabledReason(
													option.id,
													problemKind,
												);
												const reasonId = flowScopedDomId(
													idScope,
													`netgen-preset-${option.id}`,
												);
												return (
													<button
														type="button"
														key={option.id}
														aria-pressed={form.netgenPreset === option.id}
														aria-disabled={disabledReason !== undefined}
														aria-describedby={reasonId}
														data-netgen-preset={option.id}
														className={
															disabledReason === undefined
																? undefined
																: "is-disabled"
														}
														onClick={() => {
															if (disabledReason !== undefined) return;
															onChange(
																option.id === "custom"
																	? { ...form, netgenPreset: "custom" }
																	: applyNetgenPreset(form, option.id),
															);
														}}
													>
														<span>{option.label}</span>
														<small id={reasonId}>
															{disabledReason ??
																(form.netgenPreset === option.id
																	? "Selected"
																	: "Available")}
														</small>
													</button>
												);
											})}
										</div>
									</fieldset>
								)}
								{descriptor.features.has("gridgraph") && (
									<label>
										Shape preset
										<select
											value={form.gridgraphPreset}
											aria-describedby={statusDescription(
												"flow-generator-gridgraph-shape",
											)}
											onChange={(event) => {
												const preset = event.target.value as GridgraphPresetId;
												onChange(
													preset === "custom"
														? { ...form, gridgraphPreset: "custom" }
														: applyGridgraphPreset(form, preset),
												);
											}}
										>
											{GRIDGRAPH_PRESET_OPTIONS.map((option) => (
												<option key={option.id} value={option.id}>
													{option.label}
												</option>
											))}
											<option value="custom">Custom</option>
										</select>
									</label>
								)}
								{descriptor.features.has("washington-level") && (
									<label>
										Shape preset
										<select
											value={form.washingtonPreset}
											aria-describedby={statusDescription(
												"flow-generator-washington-shape",
											)}
											onChange={(event) => {
												const preset = event.target.value as WashingtonPresetId;
												onChange(
													preset === "custom"
														? { ...form, washingtonPreset: "custom" }
														: applyWashingtonLevelPreset(
																descriptor,
																form,
																preset,
															),
												);
											}}
										>
											{WASHINGTON_PRESET_OPTIONS.map((option) => (
												<option key={option.id} value={option.id}>
													{option.label}
												</option>
											))}
											<option value="custom">Custom</option>
										</select>
									</label>
								)}
								{descriptor.features.has("washington-matching") && (
									<label>
										Shape preset
										<select
											value={form.washingtonMatchingPreset}
											aria-describedby={statusDescription(
												"flow-generator-washington-matching-shape",
											)}
											onChange={(event) => {
												const preset = event.target
													.value as WashingtonMatchingPresetId;
												onChange(
													preset === "custom"
														? { ...form, washingtonMatchingPreset: "custom" }
														: applyWashingtonMatchingPreset(form, preset),
												);
											}}
										>
											{WASHINGTON_MATCHING_PRESET_OPTIONS.map((option) => (
												<option key={option.id} value={option.id}>
													{option.label}
												</option>
											))}
											<option value="custom">Custom</option>
										</select>
									</label>
								)}
								{descriptor.features.has("washington-square-mesh") && (
									<label>
										Shape preset
										<select
											value={form.washingtonSquareMeshPreset}
											aria-describedby={statusDescription(
												"flow-generator-washington-square-mesh-shape",
											)}
											onChange={(event) => {
												const preset = event.target
													.value as WashingtonSquareMeshPresetId;
												onChange(
													preset === "custom"
														? { ...form, washingtonSquareMeshPreset: "custom" }
														: applyWashingtonSquareMeshPreset(form, preset),
												);
											}}
										>
											{WASHINGTON_SQUARE_MESH_PRESET_OPTIONS.map((option) => (
												<option key={option.id} value={option.id}>
													{option.label}
												</option>
											))}
											<option value="custom">Custom</option>
										</select>
									</label>
								)}
								{parameterControls.map((control) => (
									<label key={control.field}>
										{control.label}
										<input
											{...generatorFieldA11y(control.field)}
											type="number"
											min={control.minimum}
											max={control.maximum}
											step={control.step}
											value={form[control.field]}
											onChange={(event) =>
												update(control.field, Number(event.target.value))
											}
										/>
									</label>
								))}
								{descriptor.features.has("assignment-shape") &&
									assignmentParameterLabels(form.assignmentShape).map(
										(label, index) =>
											label === undefined ? null : (
												<label key={label}>
													{label}
													<input
														{...generatorFieldA11y(
															index === 0 ? "tertiary" : "quaternary",
														)}
														type="number"
														min={0}
														max={
															form.assignmentShape === "uniform" ||
															form.assignmentShape === "planted-optimum"
																? 1_000
																: undefined
														}
														value={
															index === 0 ? form.tertiary : form.quaternary
														}
														onChange={(event) =>
															update(
																index === 0 ? "tertiary" : "quaternary",
																Number(event.target.value),
															)
														}
													/>
												</label>
											),
									)}
								{descriptor.features.has("transportation-shape") &&
									transportationParameterLabel(form.transportationShape) !==
										undefined && (
										<label>
											{transportationParameterLabel(form.transportationShape)}
											<input
												{...generatorFieldA11y("quaternary")}
												type="number"
												min={1}
												max={
													form.transportationShape === "sparse-feasible"
														? 1_000
														: 1_000_000_000
												}
												value={form.quaternary}
												onChange={(event) =>
													update("quaternary", Number(event.target.value))
												}
											/>
										</label>
									)}
								{descriptor.features.has("transportation-shape") &&
									transportationCostLabels(form.transportationShape).map(
										(label, index) =>
											label === "" || label === undefined ? null : (
												<label key={label}>
													{label}
													<input
														{...generatorFieldA11y(
															index === 0 ? "costMinimum" : "costMaximum",
														)}
														type="number"
														min={-1_000_000_000}
														max={1_000_000_000}
														value={
															index === 0 ? form.costMinimum : form.costMaximum
														}
														onChange={(event) =>
															update(
																index === 0 ? "costMinimum" : "costMaximum",
																event.target.value,
															)
														}
													/>
												</label>
											),
									)}
								{descriptor.features.has("assignment-shape") &&
									assignmentCostLabels(form.assignmentShape).map(
										(label, index) =>
											label === undefined ? null : (
												<label key={label}>
													{label}
													<input
														{...generatorFieldA11y(
															index === 0 ? "costMinimum" : "costMaximum",
														)}
														type="number"
														min={-1_000_000_000}
														max={1_000_000_000}
														value={
															index === 0 ? form.costMinimum : form.costMaximum
														}
														onChange={(event) =>
															update(
																index === 0 ? "costMinimum" : "costMaximum",
																event.target.value,
															)
														}
													/>
												</label>
											),
									)}
								{descriptor.features.has("assignment-shape") &&
									form.assignmentShape === "planted-optimum" && (
										<label>
											Additional noise limit
											<input
												{...generatorFieldA11y("assignmentNoise")}
												type="number"
												min={0}
												max={1_000_000_000}
												value={form.assignmentNoise}
												onChange={(event) =>
													update("assignmentNoise", Number(event.target.value))
												}
											/>
										</label>
									)}
								{descriptor.features.has("grid-toggle") && (
									<label className="generator-checkbox">
										<input
											type="checkbox"
											checked={form.toggle}
											onChange={(event) =>
												update("toggle", event.target.checked)
											}
										/>
										{descriptor.toggleLabel}
									</label>
								)}
								{descriptor.features.has("rmfgen") && (
									<>
										<label>
											Minimum inter-frame capacity c1
											<input
												{...generatorFieldA11y("capacityMinimum")}
												type="number"
												min={0}
												max={1_000}
												value={form.capacityMinimum}
												inputMode="numeric"
												onChange={(event) =>
													update("capacityMinimum", event.target.value)
												}
											/>
										</label>
										<label>
											Maximum inter-frame capacity c2
											<input
												{...generatorFieldA11y("capacityMaximum")}
												type="number"
												min={0}
												max={1_000}
												value={form.capacityMaximum}
												inputMode="numeric"
												onChange={(event) =>
													update("capacityMaximum", event.target.value)
												}
											/>
										</label>
									</>
								)}
								{descriptor.features.has("gridgen") && (
									<>
										<label>
											Average degree
											<input
												{...generatorFieldA11y("quaternary")}
												type="number"
												min={1}
												value={form.quaternary}
												onChange={(event) =>
													update("quaternary", Number(event.target.value))
												}
											/>
										</label>
										<label>
											Total supply
											<input
												{...generatorFieldA11y("gridgenTotalSupply")}
												type="number"
												min={1}
												max={1_000_000_000}
												value={form.gridgenTotalSupply}
												onChange={(event) =>
													update("gridgenTotalSupply", event.target.value)
												}
											/>
										</label>
										<label>
											Regular-edge minimum capacity
											<input
												{...generatorFieldA11y("capacityMinimum")}
												type="number"
												min={0}
												max={1_000_000_000}
												value={form.capacityMinimum}
												onChange={(event) =>
													update("capacityMinimum", event.target.value)
												}
											/>
										</label>
										<label>
											Regular-edge maximum capacity
											<input
												{...generatorFieldA11y("capacityMaximum")}
												type="number"
												min={0}
												max={1_000_000_000}
												value={form.capacityMaximum}
												onChange={(event) =>
													update("capacityMaximum", event.target.value)
												}
											/>
										</label>
										<label>
											Regular-edge minimum cost
											<input
												{...generatorFieldA11y("costMinimum")}
												type="number"
												min={0}
												max={1_000_000_000}
												value={form.costMinimum}
												onChange={(event) =>
													update("costMinimum", event.target.value)
												}
											/>
										</label>
										<label>
											Regular-edge maximum cost
											<input
												{...generatorFieldA11y("costMaximum")}
												type="number"
												min={0}
												max={1_000_000_000}
												value={form.costMaximum}
												onChange={(event) =>
													update("costMaximum", event.target.value)
												}
											/>
										</label>
									</>
								)}
								{descriptor.features.has("gridgraph") && (
									<>
										<label>
											Maximum capacity U
											<input
												{...generatorFieldA11y("capacityMaximum")}
												type="number"
												min={1}
												max={1_000_000_000}
												value={form.capacityMaximum}
												onChange={(event) =>
													update("capacityMaximum", event.target.value)
												}
											/>
										</label>
										<label>
											Maximum cost C
											<input
												{...generatorFieldA11y("costMaximum")}
												type="number"
												min={1}
												max={1_000_000_000}
												value={form.costMaximum}
												onChange={(event) =>
													update("costMaximum", event.target.value)
												}
											/>
										</label>
									</>
								)}
								{descriptor.features.has("washington-level") && (
									<label>
										Maximum capacity C
										<input
											{...generatorFieldA11y("capacityMaximum")}
											type="number"
											min={1}
											max={100_000_000}
											value={form.capacityMaximum}
											onChange={(event) =>
												update("capacityMaximum", event.target.value)
											}
										/>
									</label>
								)}
								{descriptor.features.has("washington-square-mesh") && (
									<label>
										Maximum capacity C
										<input
											{...generatorFieldA11y("capacityMaximum")}
											type="number"
											min={1}
											max={100_000_000}
											value={form.capacityMaximum}
											onChange={(event) =>
												update("capacityMaximum", event.target.value)
											}
										/>
									</label>
								)}
								{descriptor.features.has("goto-torus") && (
									<>
										<label>
											Random-edge capacity limit U
											<input
												{...generatorFieldA11y("capacityMaximum")}
												type="number"
												min={8}
												max={1_000_000_000}
												value={form.capacityMaximum}
												onChange={(event) =>
													update("capacityMaximum", event.target.value)
												}
											/>
										</label>
										<label>
											Maximum cost
											<input
												{...generatorFieldA11y("costMaximum")}
												type="number"
												min={8}
												max={1_000_000_000}
												value={form.costMaximum}
												onChange={(event) =>
													update("costMaximum", event.target.value)
												}
											/>
										</label>
									</>
								)}
							</div>
							{descriptor.features.has("netgen") && (
								<>
									<fieldset
										className="generator-group"
										aria-describedby={validationStatusId}
									>
										<legend>Supply and terminal behavior</legend>
										<div className="generator-grid">
											<label>
												Total supply B
												<input
													{...generatorFieldA11y("netgenTotalSupply")}
													type="number"
													min={Math.max(form.secondary, form.tertiary)}
													max={1_000_000_000}
													value={form.netgenTotalSupply}
													onChange={(event) =>
														update("netgenTotalSupply", event.target.value)
													}
												/>
											</label>
											<label>
												Transshipment sources
												<input
													{...generatorFieldA11y("netgenTransshipmentSources")}
													type="number"
													min={0}
													max={form.secondary}
													value={form.netgenTransshipmentSources}
													onChange={(event) =>
														update(
															"netgenTransshipmentSources",
															Number(event.target.value),
														)
													}
												/>
											</label>
											<label>
												Transshipment sinks
												<input
													{...generatorFieldA11y("netgenTransshipmentSinks")}
													type="number"
													min={0}
													max={form.tertiary}
													value={form.netgenTransshipmentSinks}
													onChange={(event) =>
														update(
															"netgenTransshipmentSinks",
															Number(event.target.value),
														)
													}
												/>
											</label>
										</div>
									</fieldset>
									<fieldset
										className="generator-group"
										aria-describedby={validationStatusId}
									>
										<legend>Skeleton and added-edge attributes</legend>
										<div className="generator-grid">
											<label>
												High-cost skeleton edges
												<input
													{...generatorFieldA11y("netgenHighCostPercentage")}
													type="number"
													min={0}
													max={100}
													value={form.netgenHighCostPercentage}
													onChange={(event) =>
														update(
															"netgenHighCostPercentage",
															Number(event.target.value),
														)
													}
												/>
												<small>%</small>
											</label>
											<label>
												Capacitated skeleton / added edges
												<input
													{...generatorFieldA11y("netgenCapacitatedPercentage")}
													type="number"
													min={0}
													max={100}
													value={form.netgenCapacitatedPercentage}
													onChange={(event) =>
														update(
															"netgenCapacitatedPercentage",
															Number(event.target.value),
														)
													}
												/>
												<small>%</small>
											</label>
											<label>
												Minimum cost c1
												<input
													{...generatorFieldA11y("costMinimum")}
													type="number"
													min={-1_000_000_000}
													max={1_000_000_000}
													value={form.costMinimum}
													onChange={(event) =>
														update("costMinimum", event.target.value)
													}
												/>
											</label>
											<label>
												Maximum cost c2
												<input
													{...generatorFieldA11y("costMaximum")}
													type="number"
													min={-1_000_000_000}
													max={1_000_000_000}
													value={form.costMaximum}
													onChange={(event) =>
														update("costMaximum", event.target.value)
													}
												/>
											</label>
											<label>
												Minimum capacity u1
												<input
													{...generatorFieldA11y("capacityMinimum")}
													type="number"
													min={0}
													max={1_000_000_000}
													value={form.capacityMinimum}
													onChange={(event) =>
														update("capacityMinimum", event.target.value)
													}
												/>
											</label>
											<label>
												Maximum capacity u2
												<input
													{...generatorFieldA11y("capacityMaximum")}
													type="number"
													min={0}
													max={1_000_000_000}
													value={form.capacityMaximum}
													onChange={(event) =>
														update("capacityMaximum", event.target.value)
													}
												/>
											</label>
										</div>
									</fieldset>
								</>
							)}
							{descriptor.features.has("vision-grid") && (
								<div className="flow-fixed-attributes" role="note">
									<strong>Boykov–Kolmogorov image-segmentation model</strong>
									<span>
										Every pixel connects to both source and sink terminals;
										adjacent pixels receive bidirectional n-links.
									</span>
									<small>
										Choose 4- or 8-neighbor connectivity. Independent streams
										make capacities reproducible; max-flow edge costs stay at
										zero.
									</small>
								</div>
							)}
							{fixedConstruction !== undefined ? (
								<details
									className="flow-fixed-attributes"
									id={descriptorStatusId}
								>
									<summary>Generation contract</summary>
									<div role="status" aria-live="polite">
										<strong>{fixedConstruction.heading}</strong>
										<span>{fixedConstruction.detail}</span>
										<small>{fixedConstruction.note}</small>
									</div>
								</details>
							) : (
								<fieldset className="generator-group flow-attribute-grid">
									<legend>Edge attributes</legend>
									<label>
										Capacity distribution
										<select
											value={form.capacityKind}
											onChange={(event) =>
												update(
													"capacityKind",
													event.target
														.value as FlowGeneratorForm["capacityKind"],
												)
											}
										>
											<option value="unit">Unit · always 1</option>
											<option value="constant">constant</option>
											<option value="uniform">inclusive uniform</option>
											<option value="bimodal">bimodal · 50 / 50</option>
											<option value="power-of-two-buckets">
												power-of-two buckets
											</option>
										</select>
									</label>
									{form.capacityKind !== "unit" && (
										<label>
											{capacityFieldLabels(form.capacityKind)[0]}
											<input
												value={form.capacityMinimum}
												inputMode="numeric"
												onChange={(event) =>
													update("capacityMinimum", event.target.value)
												}
											/>
										</label>
									)}
									{capacityFieldLabels(form.capacityKind)[1] !== undefined && (
										<label>
											{capacityFieldLabels(form.capacityKind)[1]}
											<input
												value={form.capacityMaximum}
												inputMode="numeric"
												onChange={(event) =>
													update("capacityMaximum", event.target.value)
												}
											/>
										</label>
									)}
									{problemKind === "min-cost-flow" && (
										<label>
											Cost distribution
											<select
												value={form.costKind}
												onChange={(event) =>
													update(
														"costKind",
														event.target.value as FlowGeneratorForm["costKind"],
													)
												}
											>
												<option value="zero">Zero · always 0</option>
												<option value="constant">constant</option>
												<option value="uniform">inclusive uniform</option>
												<option value="bimodal">bimodal · 50 / 50</option>
												<option value="capacity-correlated">
													capacity-correlated
												</option>
											</select>
										</label>
									)}
									{problemKind === "min-cost-flow" &&
										form.costKind !== "zero" && (
											<label>
												{costFieldLabels(form.costKind)[0]}
												<input
													value={form.costMinimum}
													inputMode="numeric"
													onChange={(event) =>
														update("costMinimum", event.target.value)
													}
												/>
											</label>
										)}
									{problemKind === "min-cost-flow" &&
										costFieldLabels(form.costKind)[1] !== undefined && (
											<label>
												{costFieldLabels(form.costKind)[1]}
												<input
													value={form.costMaximum}
													inputMode="numeric"
													onChange={(event) =>
														update("costMaximum", event.target.value)
													}
												/>
											</label>
										)}
									{problemKind === "min-cost-flow" &&
										form.costKind === "capacity-correlated" && (
											<>
												<label>
													Correlation direction
													<select
														value={form.costCorrelationDirection}
														onChange={(event) =>
															update(
																"costCorrelationDirection",
																event.target
																	.value as FlowGeneratorForm["costCorrelationDirection"],
															)
														}
													>
														<option value="positive">
															Positive · larger capacity costs more
														</option>
														<option value="negative">
															Negative · larger capacity costs less
														</option>
													</select>
												</label>
												<label>
													Maximum jitter (±)
													<input
														value={form.costMaximumJitter}
														inputMode="numeric"
														onChange={(event) =>
															update("costMaximumJitter", event.target.value)
														}
													/>
												</label>
											</>
										)}
								</fieldset>
							)}
						</fieldset>
						<div
							id={validationStatusId}
							className={`flow-generation-estimate ${validationError !== undefined ? "is-invalid" : ""}`}
							role="status"
							aria-live="polite"
						>
							<span>Estimated size</span>
							<strong>
								{estimate.nodes.toString()} nodes ·{" "}
								{descriptor.estimateIsUpperBound(form) ? "≤ " : ""}
								{estimate.edges.toString()} edges
							</strong>
							<small>
								{validationError !== undefined
									? validationError
									: "The current graph stays intact until validation succeeds."}
							</small>
						</div>
						{busy && progress !== undefined && (
							<div
								className="flow-generation-progress"
								role="status"
								aria-live="polite"
							>
								<span>{generationStageLabel(progress)}</span>
								<strong>
									Stage{" "}
									{Math.min(progress.completedPhases + 1, progress.totalPhases)}
									/{progress.totalPhases}
								</strong>
								<progress
									max={progress.totalPhases}
									value={progress.completedPhases}
									aria-label={`${generationStageLabel(progress)}. ${progress.completedPhases} of ${progress.totalPhases} stages complete; now at stage ${Math.min(progress.completedPhases + 1, progress.totalPhases)}.`}
								/>
							</div>
						)}
					</div>
					<div className="dialog-actions">
						{busy ? (
							<button
								key="cancel-generation"
								type="button"
								className="quiet-button"
								onClick={onCancelGeneration}
							>
								Cancel generation
							</button>
						) : (
							<Dialog.Close asChild>
								<button
									key="close-generator-dialog"
									type="button"
									className="quiet-button"
								>
									Cancel
								</button>
							</Dialog.Close>
						)}
						<button
							type="button"
							className="primary-button"
							disabled={validationError !== undefined || busy}
							onClick={onGenerate}
						>
							{busy ? "Generating…" : "Generate & load"}
						</button>
					</div>
				</Dialog.Content>
			</Dialog.Portal>
		</Dialog.Root>
	);
}
