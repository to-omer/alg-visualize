import * as Dialog from "@radix-ui/react-dialog";
import { useMemo, useState } from "react";
import {
	DEFAULT_FLOW_ALGORITHM_CATALOG_FILTERS,
	type FlowAlgorithmCatalogEntry,
	type FlowAlgorithmCatalogFilters,
	type FlowGraphAdmissionFacts,
	type FlowModelKind,
	filterFlowAlgorithmCatalogByFacets,
	flowAlgorithmSelectionReason,
	flowAlgorithmSelectionReasonMessage,
	flowAlgorithmShapeReport,
} from "./flow-algorithm-catalog";
import type {
	FlowAlgorithmConformanceContract,
	FlowCheckerContractKind,
	FlowNumericSafetyContractKind,
} from "./flow-algorithm-conformance";
import { flowScopedDomId, useFlowDomIdScope } from "./flow-dom-id";
import type { FlowGraphShape } from "./flow-graph-shape";
import "./flow-dialog-enhancements.css";

const STATUS_LABELS: Record<FlowAlgorithmCatalogEntry["status"], string> = {
	executable: "Available",
	planned: "Planned",
	"source-blocked": "Source pending",
};

const KIND_LABELS: Record<FlowAlgorithmCatalogEntry["kind"], string> = {
	solver: "solver",
	variant: "variant",
	heuristic: "heuristic",
	primitive: "primitive",
};

const SCOPE_LABELS: Record<
	FlowAlgorithmCatalogEntry["implementation_scope"],
	string
> = {
	"source-complete": "Complete source-defined transition system",
	"bounded-oracle-guided": "Source transitions with a bounded exact oracle",
	"source-component": "Standalone primitive or heuristic",
	"project-oracle-demonstrator":
		"Project demonstrator: source prefix plus optimum-vector oracle",
	"external-completion": "Transitional: completed by another solver",
	"precomputed-optimum-projection":
		"Transitional: projects a precomputed optimum",
};

const INITIAL_ORACLE_LABELS: Record<
	Exclude<FlowAlgorithmCatalogEntry["initial_oracle_dependency"], "none">,
	string
> = {
	"project-exact-max-flow-scalar-target":
		"Project exact max-flow scalar target (initial objective gap / potential)",
	"project-exact-min-cost-scalar-optimum":
		"Project exact minimum-cost scalar optimum (initial potential)",
	"project-optimum-vector-initial-state":
		"Project optimum-vector oracle (initial relative-interior state)",
	"project-isolation-face-optimum-facts":
		"Project isolation / fixed-face optimum facts (enumerated vector discarded)",
	"project-feasible-face-initial-state-and-scalar-optimum":
		"Project feasible-face barycenter plus scalar optimum (initial relative-interior state)",
};

const CHECKER_LABELS: Record<FlowCheckerContractKind, string> = {
	"independent-max-flow-certificate":
		"Independent max-flow / min-cut certificate",
	"independent-min-cost-flow-certificate":
		"Independent balance / cost / dual certificate",
	"independent-min-cost-max-flow-certificate":
		"Independent max-flow plus min-cost certificate",
	"independent-bipartite-matching-certificate":
		"Independent matching / Kőnig cover certificate",
	"independent-assignment-certificate":
		"Independent assignment primal / dual certificate",
	"independent-convex-cost-certificate":
		"Independent marginal-residual convex certificate",
	"source-defined-invariant": "Source-defined invariant / replay checker",
	"project-oracle-demonstrator-invariant":
		"Project oracle-composite invariant / replay checker",
};

const NUMERIC_SAFETY_LABELS: Record<FlowNumericSafetyContractKind, string> = {
	"aggregate-safe-wide-arithmetic":
		"Aggregate-safe wide integer bounds with preflight rejection",
	"bounded-kernel-checked-arithmetic": "Checked arithmetic in a bounded kernel",
	"structural-domain-proof": "Numeric range proved by structural constraints",
};

const MODEL_LABELS: Record<FlowModelKind, string> = {
	"max-flow": "Max Flow",
	"parametric-max-flow": "Parametric Max Flow",
	"fixed-flow-min-cost": "Fixed-Flow Min-Cost",
	"min-cost-max-flow": "Min-Cost Max-Flow",
	circulation: "Circulation",
	transshipment: "Min-Cost Transshipment",
	"convex-cost-flow": "Piecewise-Linear Convex-Cost Flow",
	"bipartite-matching": "Maximum Bipartite Matching",
	assignment: "Assignment",
	transportation: "Transportation",
	"planar-max-flow": "Embedded Planar Max Flow",
};

const REQUIREMENT_LABELS = {
	"no-self-loops": "No self-loops",
	"zero-flow-feasible": "Zero flow is feasible",
	"positive-capacity": "Every edge has positive capacity",
	"non-empty-edges": "At least one edge",
	"zero-cost": "Every edge has zero cost",
	"distinct-terminals": "Distinct source and sink",
	"underlying-connected": "Connected underlying graph",
	"unit-capacity": "Unit capacity",
	"unit-network": "unit network",
	bipartite: "Bipartite graph",
	"balanced-bipartite": "Balanced bipartite graph",
	"transportation-network": "Transportation network",
	"planar-embedding": "Validated planar embedding",
	"strongly-connected": "Strongly connected by positive-capacity edges",
	"nonbinding-transshipment-capacities":
		"Every residual capacity width covers the lower-adjusted required flow",
} as const;

function selectionLabel(
	entry: FlowAlgorithmCatalogEntry,
	modelKind: FlowModelKind | undefined,
	nodeCount: number | undefined,
	edgeCount: number | undefined,
	graphShape: FlowGraphShape | undefined,
	dynamicUpdates:
		| Readonly<{ count: number; capacityOnly: boolean }>
		| undefined,
	admissionFacts: FlowGraphAdmissionFacts | undefined,
	current: boolean,
	editable: boolean,
): string {
	const reason = flowAlgorithmSelectionReason(
		entry,
		modelKind,
		nodeCount,
		edgeCount,
		graphShape,
		dynamicUpdates,
		admissionFacts,
	);
	if (current && reason === "ready") return "Current";
	if (!editable && !current) return "Switch to JSON to select";
	return reason === "ready"
		? "Select"
		: flowAlgorithmSelectionReasonMessage(entry, reason);
}

function currentAvailabilityLabel(
	entry: FlowAlgorithmCatalogEntry,
	reason: ReturnType<typeof flowAlgorithmSelectionReason>,
): string {
	if (entry.status !== "executable") return STATUS_LABELS[entry.status];
	if (reason === "ready") return "Available";
	if (reason === "incompatible" || reason === "invalid-model") {
		return "Different model";
	}
	if (
		reason === "node-limit" ||
		reason === "edge-limit" ||
		reason.startsWith("kernel-")
	) {
		return "Graph outside limits";
	}
	return "Graph incompatible";
}

type Props = {
	conformance: FlowAlgorithmConformanceContract[] | undefined;
	entries: FlowAlgorithmCatalogEntry[] | undefined;
	error: string | undefined;
	workspaceProblem: "max-flow" | "min-cost-flow";
	modelKind: FlowModelKind | undefined;
	nodeCount: number | undefined;
	edgeCount: number | undefined;
	graphShape: FlowGraphShape | undefined;
	admissionFacts: FlowGraphAdmissionFacts | undefined;
	dynamicUpdates:
		| Readonly<{ count: number; capacityOnly: boolean }>
		| undefined;
	currentAlgorithmId: string | undefined;
	editable: boolean;
	onOpenChange: (open: boolean) => void;
	onSelect: (entry: FlowAlgorithmCatalogEntry) => void;
	open: boolean;
};

export function FlowAlgorithmCatalogDialog({
	conformance,
	entries,
	error,
	workspaceProblem,
	modelKind,
	nodeCount,
	edgeCount,
	graphShape,
	admissionFacts,
	dynamicUpdates,
	currentAlgorithmId,
	editable,
	onOpenChange,
	onSelect,
	open,
}: Props) {
	const idScope = useFlowDomIdScope("flow-algorithm-catalog");
	const [query, setQuery] = useState("");
	const [filters, setFilters] = useState<FlowAlgorithmCatalogFilters>(
		DEFAULT_FLOW_ALGORITHM_CATALOG_FILTERS,
	);
	const conformanceById = useMemo(
		() =>
			new Map(
				conformance?.map((contract) => [contract.algorithm_id, contract]),
			),
		[conformance],
	);
	const filtered = useMemo(
		() =>
			filterFlowAlgorithmCatalogByFacets(entries ?? [], query, filters, {
				workspaceProblem,
				modelKind,
				...(nodeCount === undefined ? {} : { nodeCount }),
				...(edgeCount === undefined ? {} : { edgeCount }),
				...(graphShape === undefined ? {} : { graphShape }),
				...(admissionFacts === undefined ? {} : { admissionFacts }),
				...(dynamicUpdates === undefined ? {} : { dynamicUpdates }),
			}),
		[
			dynamicUpdates,
			edgeCount,
			entries,
			filters,
			graphShape,
			admissionFacts,
			modelKind,
			nodeCount,
			query,
			workspaceProblem,
		],
	);
	const families = useMemo(
		() => [...new Set(entries?.map((entry) => entry.family) ?? [])].sort(),
		[entries],
	);
	const grouped = useMemo(() => {
		const groups = new Map<string, FlowAlgorithmCatalogEntry[]>();
		for (const entry of filtered) {
			const group = groups.get(entry.family);
			if (group === undefined) groups.set(entry.family, [entry]);
			else group.push(entry);
		}
		return [...groups];
	}, [filtered]);
	const executableCount =
		entries?.filter((entry) => entry.status === "executable").length ?? 0;

	return (
		<Dialog.Root
			open={open}
			onOpenChange={(nextOpen) => {
				if (!nextOpen) {
					setQuery("");
					setFilters(DEFAULT_FLOW_ALGORITHM_CATALOG_FILTERS);
				}
				onOpenChange(nextOpen);
			}}
		>
			<Dialog.Portal>
				<Dialog.Overlay className="dialog-overlay" />
				<Dialog.Content className="dialog-content flow-algorithm-dialog">
					<header className="flow-algorithm-header">
						<div>
							<Dialog.Title>Flow algorithms</Dialog.Title>
							<Dialog.Description>
								Browse source-pinned implementations. Selection is limited to
								executable endpoints compatible with the current model; each
								card states its implementation scope separately.
							</Dialog.Description>
						</div>
						<Dialog.Close asChild>
							<button type="button" className="quiet-button">
								Close
							</button>
						</Dialog.Close>
					</header>
					<div className="flow-algorithm-summary" aria-live="polite">
						<span>
							Current model
							<strong>
								{modelKind === undefined
									? "Unavailable"
									: MODEL_LABELS[modelKind]}
								{nodeCount === undefined || edgeCount === undefined
									? ""
									: ` · ${nodeCount.toLocaleString()} nodes / ${edgeCount.toLocaleString()} edges`}
							</strong>
						</span>
						<span>
							Catalog
							<strong>{entries?.length ?? "—"} methods</strong>
						</span>
						<span>
							Executable
							<strong>{entries === undefined ? "—" : executableCount}</strong>
						</span>
						<span>
							Results
							<strong>{entries === undefined ? "—" : filtered.length}</strong>
						</span>
					</div>
					<label className="flow-algorithm-search">
						Search algorithms, aliases, families, sources, or complexity
						<input
							type="search"
							value={query}
							onChange={(event) => setQuery(event.target.value)}
							placeholder="e.g. Dinic, SSP, push-relabel"
						/>
					</label>
					<fieldset className="flow-algorithm-filters">
						<legend className="visually-hidden">Algorithm filters</legend>
						<label>
							Compatibility
							<select
								value={filters.compatibility}
								onChange={(event) =>
									setFilters((current) => ({
										...current,
										compatibility: event.target
											.value as FlowAlgorithmCatalogFilters["compatibility"],
									}))
								}
							>
								<option value="workspace">
									This workspace · disabled retained
								</option>
								<option value="model-compatible">Current model</option>
								<option value="runnable-now">Runnable now</option>
								<option value="all">All Max + Min methods</option>
							</select>
						</label>
						<label>
							Family
							<select
								value={filters.family}
								onChange={(event) =>
									setFilters((current) => ({
										...current,
										family: event.target.value,
									}))
								}
							>
								<option value="all">All</option>
								{families.map((family) => (
									<option key={family} value={family}>
										{family}
									</option>
								))}
							</select>
						</label>
						<label>
							Kind
							<select
								value={filters.kind}
								onChange={(event) =>
									setFilters((current) => ({
										...current,
										kind: event.target
											.value as FlowAlgorithmCatalogFilters["kind"],
									}))
								}
							>
								<option value="all">All</option>
								<option value="solver">solver</option>
								<option value="variant">variant</option>
								<option value="heuristic">heuristic</option>
								<option value="primitive">primitive</option>
							</select>
						</label>
						<label>
							Randomness
							<select
								value={filters.randomness}
								onChange={(event) =>
									setFilters((current) => ({
										...current,
										randomness: event.target
											.value as FlowAlgorithmCatalogFilters["randomness"],
									}))
								}
							>
								<option value="all">All</option>
								<option value="deterministic">Deterministic</option>
								<option value="randomized">Randomized</option>
							</select>
						</label>
					</fieldset>
					<button
						type="button"
						className="quiet-button flow-algorithm-filter-reset"
						onClick={() => {
							setQuery("");
							setFilters(DEFAULT_FLOW_ALGORITHM_CATALOG_FILTERS);
						}}
					>
						Reset search and filters
					</button>
					{!editable && (
						<p className="flow-algorithm-notice">
							Switch to JSON input before selecting an algorithm; this preserves
							your Flow DSL edits.
						</p>
					)}
					{error !== undefined && (
						<p className="dialog-error" role="alert">
							{error}
						</p>
					)}
					{entries === undefined && error === undefined ? (
						<p className="flow-algorithm-empty" role="status">
							Validating the WASM catalog…
						</p>
					) : grouped.length === 0 ? (
						<p className="flow-algorithm-empty">No matching algorithms.</p>
					) : (
						<div className="flow-algorithm-groups">
							{grouped.map(([family, familyEntries]) => (
								<section key={family} className="flow-algorithm-group">
									<h3>
										{family} <span>{familyEntries.length}</span>
									</h3>
									<ul>
										{familyEntries.map((entry) => {
											const contract = conformanceById.get(entry.id);
											const titleId = flowScopedDomId(
												idScope,
												`title-${entry.id}`,
											);
											const current = entry.id === currentAlgorithmId;
											const reason = flowAlgorithmSelectionReason(
												entry,
												modelKind,
												nodeCount,
												edgeCount,
												graphShape,
												dynamicUpdates,
												admissionFacts,
											);
											const shapeReport = flowAlgorithmShapeReport(
												entry,
												graphShape,
											);
											const selectable =
												editable && reason === "ready" && !current;
											const reasonId = flowScopedDomId(
												idScope,
												`reason-${entry.id}`,
											);
											const reasonLabel = selectionLabel(
												entry,
												modelKind,
												nodeCount,
												edgeCount,
												graphShape,
												dynamicUpdates,
												admissionFacts,
												current,
												editable,
											);
											const showSelectionStatus = reason !== "ready";
											const selectionDescription =
												current && reason === "ready"
													? "Currently selected"
													: current
														? `Currently selected; ${reasonLabel}`
														: reason === "ready" && editable
															? "Available for the current graph"
															: reasonLabel;
											const availableStepBoundaries = [
												entry.trace_steps.detail.availability === "available"
													? "Detail"
													: undefined,
												entry.trace_steps.operation_availability
													.availability === "available"
													? "Operation"
													: undefined,
												entry.trace_steps.phase_availability.availability ===
												"available"
													? "Phase"
													: undefined,
											].filter((boundary) => boundary !== undefined);
											const unavailableStepBoundaries = [
												entry.trace_steps.detail.availability === "unavailable"
													? "Detail"
													: undefined,
												entry.trace_steps.operation_availability
													.availability === "unavailable"
													? "Operation"
													: undefined,
												entry.trace_steps.phase_availability.availability ===
												"unavailable"
													? "Phase"
													: undefined,
											].filter((boundary) => boundary !== undefined);
											const stepSupportLabel = `${availableStepBoundaries.join(
												" + ",
											)}${
												unavailableStepBoundaries.length > 0
													? ` · no ${unavailableStepBoundaries.join(" / ")}`
													: ""
											}`;
											return (
												<li
													key={entry.id}
													data-algorithm-id={entry.id}
													data-selection-reason={reason}
													aria-describedby={reasonId}
													tabIndex={selectable ? undefined : 0}
													className={
														selectable ? undefined : "flow-algorithm-disabled"
													}
													data-compatible-generator-fixtures={contract?.compatible_generator_fixture_ids.join(
														" ",
													)}
													aria-labelledby={titleId}
												>
													<div className="flow-algorithm-main">
														<div className="flow-algorithm-title-row">
															<strong id={titleId}>{entry.title}</strong>
															<span
																className={`flow-algorithm-status ${
																	reason === "ready"
																		? `flow-algorithm-status-${entry.status}`
																		: "flow-algorithm-status-disabled"
																}`}
															>
																{currentAvailabilityLabel(entry, reason)}
															</span>
															{current && (
																<span className="flow-algorithm-current">
																	Current
																</span>
															)}
															<details className="flow-algorithm-step-contract">
																<summary
																	className="flow-algorithm-step-status"
																	aria-label={`Step support: ${stepSupportLabel}. Expand for Phase, Operation, and Detail definitions.`}
																>
																	{stepSupportLabel}
																</summary>
																<div>
																	<small>
																		<strong>Phase</strong>{" "}
																		{entry.trace_steps.phase_availability
																			.availability === "available"
																			? entry.trace_steps.phase_unit
																			: `Unavailable — ${entry.trace_steps.phase_availability.reason}`}
																	</small>
																	<small>
																		<strong>Operation</strong>{" "}
																		{entry.trace_steps.operation_availability
																			.availability === "available"
																			? entry.trace_steps.operation_unit
																			: `Unavailable — ${entry.trace_steps.operation_availability.reason}`}
																	</small>
																	<small>
																		<strong>Detail</strong>{" "}
																		{entry.trace_steps.detail.availability ===
																		"available"
																			? entry.trace_steps.detail.unit
																			: entry.trace_steps.detail.reason}
																	</small>
																	<small>
																		<strong>Primary work counter</strong>{" "}
																		{entry.trace_steps.primary_work.unit} ·{" "}
																		{entry.trace_steps.primary_work.abstraction}
																	</small>
																</div>
															</details>
														</div>
														<code>{entry.id}</code>
														<small
															id={reasonId}
															className={
																showSelectionStatus
																	? "flow-algorithm-selection-status"
																	: "visually-hidden"
															}
														>
															{selectionDescription}
														</small>
														<details className="flow-algorithm-metadata">
															<summary>Implementation details</summary>
															<div>
																<p>
																	Complexity / implementation claim:{" "}
																	{entry.complexity}
																</p>
																<small>
																	{KIND_LABELS[entry.kind]} ·{" "}
																	{entry.models.join(", ")} ·
																	{entry.exact ? "exact" : "approximate"}
																	{entry.randomized ? " · randomized" : ""}
																</small>
																{entry.terminal_oracle_dependency !==
																	"none" && (
																	<small>
																		Terminal dependency: project optimum-vector
																		oracle
																	</small>
																)}
																{entry.initial_oracle_dependency !== "none" && (
																	<small>
																		Initial dependency:{" "}
																		{
																			INITIAL_ORACLE_LABELS[
																				entry.initial_oracle_dependency
																			]
																		}
																	</small>
																)}
																<small>
																	Implementation scope:{" "}
																	{SCOPE_LABELS[entry.implementation_scope]}
																</small>
																{entry.search_terms.length > 0 && (
																	<small>
																		Related parent method:{" "}
																		{entry.search_terms.join(", ")}
																		(not an alias for a complete solver)
																	</small>
																)}
																<small>
																	Structural requirements:{" "}
																	{shapeReport.length === 0
																		? "General directed graph"
																		: shapeReport
																				.map(
																					({ requirement, status }) =>
																						`${status === "satisfied" ? "✓" : status === "unsatisfied" ? "×" : "?"} ${REQUIREMENT_LABELS[requirement]}`,
																				)
																				.join(" / ")}
																</small>
																<small>
																	Initialization: {entry.initial_construction} /{" "}
																	{entry.initial_optimality} · Limit{" "}
																	{entry.initial_band.max_nodes.toLocaleString()}{" "}
																	nodes /{" "}
																	{entry.initial_band.max_edges.toLocaleString()}{" "}
																	edges
																</small>
																<small>
																	source: {entry.source_id}
																	{contract === undefined
																		? ""
																		: ` · ${contract.source.kind} · reviewed ${contract.source.reviewed}`}
																</small>
																{contract !== undefined && (
																	<details className="flow-algorithm-source-contract">
																		<summary>
																			Source and conformance contract
																		</summary>
																		<p>{contract.source.fixed_source}</p>
																		<p>
																			{contract.source.catalog_scope_and_claims}
																		</p>
																		<p>
																			checker:{" "}
																			{
																				CHECKER_LABELS[
																					contract.checker_contract_kind
																				]
																			}
																		</p>
																		<p>
																			Numeric bounds:{" "}
																			{
																				NUMERIC_SAFETY_LABELS[
																					contract.numeric_safety_contract_kind
																				]
																			}
																		</p>
																		<p>
																			work limit: catalog admission
																			{contract.work_limit_contract
																				.source_termination_argument
																				? " + source termination argument"
																				: ""}
																			{contract.work_limit_contract
																				.checked_runtime_work_ceiling
																				? " + checked implementation ceiling"
																				: ""}
																		</p>
																	</details>
																)}
															</div>
														</details>
													</div>
													<button
														type="button"
														className="quiet-button flow-algorithm-select"
														disabled={!selectable}
														aria-describedby={titleId}
														onClick={() => onSelect(entry)}
													>
														{reasonLabel}
													</button>
												</li>
											);
										})}
									</ul>
								</section>
							))}
						</div>
					)}
				</Dialog.Content>
			</Dialog.Portal>
		</Dialog.Root>
	);
}
