import init, {
	canonical_edited_scenario_json,
	canonical_flow_dsl_json,
	canonical_flow_scenario_json,
	canonical_scenario_json,
	engine_contract_json,
	flow_algorithm_catalog_json,
	flow_algorithm_conformance_contracts_json,
	flow_generator_fixture_manifest_json,
	generate_initial_json,
	generate_operations_json,
	parse_initial_dsl_json,
	parse_operations_dsl_json,
	scenario_has_legacy_derived_revisions,
	validate_dsl_document_size,
	WasmSession,
} from "../../../packages/wasm/visualizer_engine.js";
import { assertExpectedEngineContractV1 } from "./engine-contract";
import { engineRequestErrorSource } from "./engine-error-source";
import type {
	CurrentFrame,
	EngineRequest,
	EngineResponse,
} from "./engine-types";
import {
	decodeFlowAlgorithmCatalog,
	flowAlgorithmSelectionReason,
	flowAlgorithmSelectionReasonMessage,
	flowScenarioSelection,
} from "./flow-algorithm-catalog";
import {
	assertFlowAlgorithmConfig,
	flowScenarioNodeIds,
} from "./flow-algorithm-config";
import { decodeFlowAlgorithmConformanceContracts } from "./flow-algorithm-conformance";
import { FlowEventPublicationCoordinator } from "./flow-event-publication-coordinator";
import { decodeFlowGeneratorFixtureManifest } from "./flow-generator-fixture";
import { FlowSeekPublicationCoordinator } from "./flow-seek-publication-coordinator";
import {
	type FlowSessionLease,
	ownsActiveFlowSession,
	runWithActiveFlowSession,
} from "./flow-session-lease";
import { encodeFramePacket, type FramePacketKind } from "./packet";
import { encodePublicationV6 } from "./packet-v6";
import { PublicationCandidateCoordinator } from "./publication-candidate-coordinator";
import {
	StagedNextCoordinator,
	StagedNextRollbackError,
} from "./staged-next-coordinator";
import { fitsUtf8Budget } from "./utf8-budget";

let session: WasmSession | undefined;
let activeGeneration = 0;
let sessionSerial = 0;
let nextPublicationId = 1n;
const stagedNext = new StagedNextCoordinator();
const stagedFlowCurrent = new PublicationCandidateCoordinator<WasmSession>();
const stagedFlowEvent = new FlowEventPublicationCoordinator();
const stagedFlowSeek = new FlowSeekPublicationCoordinator();
type DeferredFlowNavigationRequest = Extract<
	EngineRequest,
	{ kind: "next" | "seek" }
>;
let deferredFlowNavigation: DeferredFlowNavigationRequest | undefined;
let flowNavigationOperation = 0;
let activeFlowNavigationOperation: number | undefined;

type StagedCurrentPublication =
	| {
			kind: "create";
			generation: number;
			candidate: WasmSession;
	  }
	| {
			kind: "seek";
			generation: number;
			sessionSerial: number;
	  };

let stagedCurrent: StagedCurrentPublication | undefined;

class EngineBootstrapError extends Error {
	constructor(cause: unknown) {
		super(
			cause instanceof Error
				? `WASM engine initialization failed: ${cause.message}`
				: `WASM engine initialization failed: ${String(cause)}`,
		);
		this.name = "EngineBootstrapError";
	}
}

let initializationFailure: EngineBootstrapError | undefined;
const initialized = init()
	.then(() => {
		assertExpectedEngineContractV1(JSON.parse(engine_contract_json()));
	})
	.catch((error: unknown) => {
		initializationFailure = new EngineBootstrapError(error);
	});
const MAX_DSL_BYTES = 64 * 1024 * 1024;

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function requireSession(): WasmSession {
	if (session === undefined) {
		throw new Error("No active visualization session");
	}
	return session;
}

function discardStagedCurrent() {
	const staged = stagedCurrent;
	stagedCurrent = undefined;
	stagedFlowCurrent.discard();
	if (staged?.kind === "create") {
		staged.candidate.free();
	} else if (
		staged?.kind === "seek" &&
		staged.sessionSerial === sessionSerial
	) {
		session?.discard_staged_seek();
	}
}

function discardStagedFlowEvent() {
	const owner = stagedFlowEvent.discard();
	if (owner === sessionSerial) session?.discard_staged_next();
}

function discardStagedFlowSeek() {
	const owner = stagedFlowSeek.cancel();
	if (
		session?.plugin_id() === "flow" &&
		(owner === undefined || owner === sessionSerial)
	) {
		session.discard_staged_seek();
	}
}

type FlowCurrentAcknowledgement =
	| Readonly<{ kind: "current" | "event" | "seek"; accepted: boolean }>
	| undefined;

function acknowledgeFlowCurrent(
	generation: number,
	publicationId: string,
	accepted: boolean,
): FlowCurrentAcknowledgement {
	const acknowledgement = stagedFlowCurrent.acknowledge(
		generation,
		publicationId,
		accepted,
	);
	if (acknowledgement.kind === "accepted") {
		const previous = session;
		session = acknowledgement.candidate;
		sessionSerial += 1;
		previous?.free();
		return { kind: "current", accepted: true };
	}
	if (acknowledgement.kind === "rejected") {
		return { kind: "current", accepted: false };
	}
	const eventAcknowledgement = stagedFlowEvent.acknowledge(
		generation,
		publicationId,
		accepted,
	);
	if (eventAcknowledgement.kind !== "ignored") {
		if (
			eventAcknowledgement.sessionSerial !== sessionSerial ||
			session === undefined
		) {
			return undefined;
		}
		if (eventAcknowledgement.kind === "accepted") {
			session.commit_staged_next();
		} else {
			session.discard_staged_next();
		}
		return {
			kind: "event",
			accepted: eventAcknowledgement.kind === "accepted",
		};
	}
	const seekAcknowledgement = stagedFlowSeek.acknowledge(
		generation,
		publicationId,
		accepted,
	);
	if (
		seekAcknowledgement.kind === "ignored" ||
		seekAcknowledgement.sessionSerial !== sessionSerial ||
		session === undefined
	) {
		return undefined;
	}
	if (seekAcknowledgement.kind === "accepted") {
		session.commit_staged_seek();
	} else {
		session.discard_staged_seek();
	}
	return {
		kind: "seek",
		accepted: seekAcknowledgement.kind === "accepted",
	};
}

function acknowledgeCurrent(generation: number, accepted: boolean) {
	const staged = stagedCurrent;
	if (staged === undefined || staged.generation !== generation) return;
	stagedCurrent = undefined;
	if (!accepted) {
		if (staged.kind === "create") staged.candidate.free();
		else if (staged.sessionSerial === sessionSerial)
			session?.discard_staged_seek();
		return;
	}
	if (staged.kind === "create") {
		const previous = session;
		session = staged.candidate;
		sessionSerial += 1;
		previous?.free();
		buildSeekIndex(sessionSerial);
		return;
	}
	if (staged.sessionSerial !== sessionSerial || session === undefined) {
		throw new Error("Seek publication no longer owns the active session");
	}
	session.commit_staged_seek();
}

function respond(response: EngineResponse, transfer: Transferable[] = []) {
	self.postMessage(response, { transfer });
}

function framePacket(kind: FramePacketKind, json: string): ArrayBuffer {
	return encodeFramePacket(kind, json);
}

function fail(
	generation: number,
	error: unknown,
	source: "engine" | "input",
	requestKind: EngineRequest["kind"],
	seekRequestSerial?: number,
) {
	respond({
		kind: "error",
		generation,
		requestKind,
		message: error instanceof Error ? error.message : String(error),
		source,
		...(seekRequestSerial === undefined ? {} : { seekRequestSerial }),
	});
}

function withoutProvenance(scenario: string): string {
	let parsed: {
		plugin?: unknown;
		payload: {
			initial?: { provenance?: unknown };
			operations?: { provenance?: unknown };
			generator_provenance?: unknown;
		};
	};
	try {
		parsed = JSON.parse(scenario) as typeof parsed;
	} catch (error: unknown) {
		throw new Error(
			`invalid Scenario JSON: ${error instanceof Error ? error.message : String(error)}`,
		);
	}
	if (parsed.plugin === "flow") {
		delete parsed.payload.generator_provenance;
	} else {
		if (parsed.payload.initial !== undefined)
			delete parsed.payload.initial.provenance;
		if (parsed.payload.operations !== undefined)
			delete parsed.payload.operations.provenance;
	}
	return JSON.stringify(parsed);
}

function scenarioPlugin(source: string): "ordered-map" | "flow" {
	const parsed: unknown = JSON.parse(source);
	if (!isRecord(parsed)) throw new Error("Scenario envelope must be an object");
	if (parsed.plugin === "ordered-map" || parsed.plugin === "flow") {
		return parsed.plugin;
	}
	throw new Error("Scenario plugin is unsupported");
}

function canonicalForPlugin(source: string): string {
	return scenarioPlugin(source) === "flow"
		? canonical_flow_scenario_json(source)
		: canonical_scenario_json(source);
}

function canonicalEditedForPlugin(source: string): string {
	return scenarioPlugin(source) === "flow"
		? canonical_flow_scenario_json(source)
		: canonical_edited_scenario_json(source);
}

function prettyScenario(canonical: string): string {
	return JSON.stringify(JSON.parse(canonical), null, 2);
}

function revisionStatus(source: string): "current" | "legacy-derived" {
	return scenario_has_legacy_derived_revisions(source)
		? "legacy-derived"
		: "current";
}

function parseDslDiagnostic(error: unknown):
	| {
			code: string;
			line: number;
			column: number;
			message: string;
	  }
	| undefined {
	if (!(error instanceof Error)) {
		return undefined;
	}
	try {
		const value: unknown = JSON.parse(error.message);
		if (
			isRecord(value) &&
			typeof value.code === "string" &&
			Number.isSafeInteger(value.line) &&
			(value.line as number) > 0 &&
			Number.isSafeInteger(value.column) &&
			(value.column as number) > 0 &&
			typeof value.message === "string"
		) {
			return value as {
				code: string;
				line: number;
				column: number;
				message: string;
			};
		}
	} catch {
		return undefined;
	}
	return undefined;
}

function selectedAlgorithm(source: string): {
	id: string;
	config: Record<string, unknown>;
} {
	return (
		JSON.parse(source) as {
			payload: { algorithm: { id: string; config: Record<string, unknown> } };
		}
	).payload.algorithm;
}

function assertFlowAlgorithmIsRunnable(
	source: string,
	selected: Readonly<{ id: string; config: Record<string, unknown> }>,
): void {
	const selectedScenario = flowScenarioSelection(source);
	if (selectedScenario === undefined) {
		throw new Error(
			"Flow execution requires a valid supported model and graph",
		);
	}
	const descriptor = decodeFlowAlgorithmCatalog(
		flow_algorithm_catalog_json(),
	).find((entry) => entry.id === selected.id);
	if (descriptor === undefined) {
		throw new Error("Selected flow algorithm is not present in the catalog");
	}
	const reason = flowAlgorithmSelectionReason(
		descriptor,
		selectedScenario.modelKind,
		selectedScenario.nodeCount,
		selectedScenario.edgeCount,
		selectedScenario.graphShape,
		selectedScenario.dynamicUpdates,
		selectedScenario.admissionFacts,
	);
	if (reason !== "ready") {
		throw new Error(
			`Selected flow algorithm is not runnable: ${flowAlgorithmSelectionReasonMessage(descriptor, reason)}`,
		);
	}
	assertFlowAlgorithmConfig(
		selected.id,
		selected.config,
		flowScenarioNodeIds(source),
	);
}

function decodeGenerated(
	json: string,
	stream: "initial" | "operations",
): {
	items: unknown[];
	provenance: Record<string, unknown>;
	stats: Record<string, number>;
} {
	const generated: unknown = JSON.parse(json);
	const itemField = stream === "initial" ? "entries" : "operations";
	if (
		!isRecord(generated) ||
		!Array.isArray(generated[itemField]) ||
		!isRecord(generated.provenance) ||
		!isRecord(generated.stats) ||
		!Object.values(generated.stats).every(
			(value) => Number.isSafeInteger(value) && (value as number) >= 0,
		)
	) {
		throw new Error("The generator returned an invalid response");
	}
	return {
		items: generated[itemField],
		provenance: generated.provenance,
		stats: generated.stats as Record<string, number>,
	};
}

type SeekChunk = {
	done: boolean;
	cursor: number;
	target: number;
	frame?: CurrentFrame;
};

function runSeek(
	generation: number,
	target: number,
	seekRequestSerial?: number,
	navigationOperation?: number,
) {
	const activeSession = requireSession();
	if (!Number.isSafeInteger(target) || target < 0) {
		if (activeSession.plugin_id() === "flow") {
			discardStagedFlowSeek();
		} else {
			activeSession.discard_staged_seek();
		}
		fail(
			generation,
			new Error("Seek target is invalid"),
			"input",
			"seek",
			seekRequestSerial,
		);
		if (navigationOperation !== undefined) {
			finishFlowNavigation(navigationOperation);
		}
		return;
	}
	if (activeSession.plugin_id() === "flow") {
		const operation = stagedFlowSeek.begin();
		const lease: FlowSessionLease<WasmSession> = {
			session: activeSession,
			serial: sessionSerial,
		};
		activeSession.discard_staged_seek();
		activeSession.begin_seek(target);
		runFlowSeek(
			generation,
			lease,
			operation.operation,
			seekRequestSerial,
			navigationOperation,
		);
		return;
	}
	activeSession.begin_seek(target);
	const resume = () => {
		if (generation !== activeGeneration || session === undefined) {
			return;
		}
		try {
			const chunk = JSON.parse(session.resume_seek_json(128)) as SeekChunk;
			if (chunk.done) {
				if (chunk.frame === undefined) {
					throw new Error("Completed seek omitted its full frame");
				}
				try {
					const packet = framePacket("current", JSON.stringify(chunk.frame));
					stagedCurrent = {
						kind: "seek",
						generation,
						sessionSerial,
					};
					respond(
						{
							kind: "seeked",
							generation,
							packet,
							...(seekRequestSerial === undefined ? {} : { seekRequestSerial }),
						},
						[packet],
					);
				} catch (error: unknown) {
					discardStagedCurrent();
					throw error;
				}
				return;
			}
			respond({
				kind: "seek-progress",
				generation,
				cursor: chunk.cursor,
				target: chunk.target,
				...(seekRequestSerial === undefined ? {} : { seekRequestSerial }),
			});
			self.setTimeout(resume, 0);
		} catch (error: unknown) {
			session?.discard_staged_seek();
			fail(
				generation,
				error,
				engineRequestErrorSource("seek", error),
				"seek",
				seekRequestSerial,
			);
		}
	};
	resume();
}

function runFlowSeek(
	generation: number,
	lease: FlowSessionLease<WasmSession>,
	operation: number,
	seekRequestSerial?: number,
	navigationOperation?: number,
) {
	const activeSession = lease.session;
	const finishNavigation = () => {
		if (navigationOperation !== undefined) {
			finishFlowNavigation(navigationOperation);
		}
	};
	const resume = async () => {
		if (
			!stagedFlowSeek.isCurrent(operation) ||
			generation !== activeGeneration ||
			!ownsActiveFlowSession(lease, session, sessionSerial)
		) {
			finishNavigation();
			return;
		}
		try {
			const chunk: unknown = JSON.parse(activeSession.resume_seek_json(128));
			if (
				!isRecord(chunk) ||
				typeof chunk.done !== "boolean" ||
				typeof chunk.cursor !== "string" ||
				typeof chunk.target !== "string"
			) {
				throw new Error("Flow seek returned an invalid progress contract");
			}
			if (!chunk.done) {
				respond({
					kind: "seek-progress",
					generation,
					cursor: Number(chunk.cursor),
					target: Number(chunk.target),
					...(seekRequestSerial === undefined ? {} : { seekRequestSerial }),
				});
				self.setTimeout(() => void resume(), 0);
				return;
			}
			if (chunk.frame === undefined) {
				throw new Error("Completed flow seek omitted its full scene");
			}
			const publicationId = nextPublicationId.toString();
			nextPublicationId += 1n;
			const parts = await encodePublicationV6(
				{
					pluginOrdinal: activeSession.plugin_ordinal(),
					payloadSchemaVersion: 3,
					publicationId,
					generation: generation.toString(),
				},
				new TextEncoder().encode(JSON.stringify(chunk.frame)),
			);
			if (
				!stagedFlowSeek.isCurrent(operation) ||
				generation !== activeGeneration ||
				!ownsActiveFlowSession(lease, session, sessionSerial)
			) {
				finishNavigation();
				return;
			}
			stagedFlowSeek.stage(operation, generation, publicationId, sessionSerial);
			respond(
				{
					kind: "flow-update",
					generation,
					publicationId,
					algorithm: activeSession.algorithm_id(),
					parts,
					...(seekRequestSerial === undefined ? {} : { seekRequestSerial }),
				},
				parts,
			);
			finishNavigation();
		} catch (error: unknown) {
			if (
				stagedFlowSeek.isCurrent(operation) &&
				ownsActiveFlowSession(lease, session, sessionSerial)
			) {
				discardStagedFlowSeek();
				fail(
					generation,
					error,
					engineRequestErrorSource("seek", error),
					"seek",
					seekRequestSerial,
				);
			}
			finishNavigation();
		}
	};
	void resume();
}

function buildSeekIndex(serial: number) {
	const resume = () => {
		if (serial !== sessionSerial || session === undefined) {
			return;
		}
		try {
			const done = session.resume_seek_index(128);
			respond({
				kind: done ? "index-ready" : "index-progress",
				generation: activeGeneration,
				coverage: session.seek_coverage(),
				itemCount: session.item_count(),
			});
			if (!done) {
				self.setTimeout(resume, 0);
			}
		} catch (error: unknown) {
			respond({
				kind: "index-error",
				generation: activeGeneration,
				message: error instanceof Error ? error.message : String(error),
			});
		}
	};
	self.setTimeout(resume, 0);
}

async function runNext(
	generation: number,
	navigationRequestSerial?: number,
): Promise<void> {
	const activeSession = requireSession();
	const lease: FlowSessionLease<WasmSession> = {
		session: activeSession,
		serial: sessionSerial,
	};
	const json = activeSession.stage_next_json();
	if (json === undefined) {
		respond({
			kind: "ended",
			generation,
			...(navigationRequestSerial === undefined
				? {}
				: { seekRequestSerial: navigationRequestSerial }),
		});
		return;
	}
	if (activeSession.plugin_id() === "flow") {
		const publicationId = nextPublicationId.toString();
		nextPublicationId += 1n;
		try {
			const parts = await encodePublicationV6(
				{
					pluginOrdinal: activeSession.plugin_ordinal(),
					payloadSchemaVersion: 3,
					publicationId,
					generation: generation.toString(),
				},
				new TextEncoder().encode(json),
			);
			if (
				generation !== activeGeneration ||
				!ownsActiveFlowSession(lease, session, sessionSerial)
			) {
				runWithActiveFlowSession(lease, session, sessionSerial, (owned) =>
					owned.discard_staged_next(),
				);
				return;
			}
			stagedFlowEvent.stage(generation, publicationId, sessionSerial);
			respond(
				{
					kind: "flow-update",
					generation,
					publicationId,
					algorithm: activeSession.algorithm_id(),
					parts,
					...(navigationRequestSerial === undefined
						? {}
						: { seekRequestSerial: navigationRequestSerial }),
				},
				parts,
			);
		} catch (error: unknown) {
			runWithActiveFlowSession(lease, session, sessionSerial, (owned) =>
				owned.discard_staged_next(),
			);
			throw error;
		}
		return;
	}
	stagedNext.stage(generation);
	try {
		const packet = framePacket("commit", json);
		respond({ kind: "commit", generation, packet }, [packet]);
	} catch (error: unknown) {
		stagedNext.discard(activeSession);
		throw error;
	}
}

function flowNavigationBlocked(): boolean {
	return (
		activeFlowNavigationOperation !== undefined ||
		stagedFlowCurrent.hasPending() ||
		stagedFlowEvent.hasPending() ||
		stagedFlowSeek.hasPending()
	);
}

function beginFlowNavigation(): number {
	flowNavigationOperation += 1;
	activeFlowNavigationOperation = flowNavigationOperation;
	return flowNavigationOperation;
}

function finishFlowNavigation(operation: number): void {
	if (activeFlowNavigationOperation !== operation) return;
	activeFlowNavigationOperation = undefined;
	void runDeferredFlowNavigation();
}

function invalidateFlowNavigation(): void {
	flowNavigationOperation += 1;
	activeFlowNavigationOperation = undefined;
}

async function startFlowNavigation(
	request: DeferredFlowNavigationRequest,
): Promise<void> {
	const operation = beginFlowNavigation();
	if (request.kind === "seek") {
		try {
			runSeek(
				request.generation,
				request.target,
				request.requestSerial,
				operation,
			);
		} catch (error: unknown) {
			finishFlowNavigation(operation);
			throw error;
		}
		return;
	}
	try {
		await runNext(request.generation, request.requestSerial);
	} finally {
		finishFlowNavigation(operation);
	}
}

async function runDeferredFlowNavigation(): Promise<void> {
	if (flowNavigationBlocked() || deferredFlowNavigation === undefined) return;
	const deferred = deferredFlowNavigation;
	deferredFlowNavigation = undefined;
	if (deferred.generation === activeGeneration) {
		await startFlowNavigation(deferred);
	}
}

self.addEventListener("message", (event: MessageEvent<EngineRequest>) => {
	const request = event.data;
	void initialized
		.then(async () => {
			if (initializationFailure !== undefined) {
				throw initializationFailure;
			}
			if (request.generation < activeGeneration) {
				return;
			}
			if (
				(request.kind === "next" || request.kind === "seek") &&
				request.generation === activeGeneration &&
				flowNavigationBlocked()
			) {
				deferredFlowNavigation = request;
				return;
			}
			if (request.generation > activeGeneration) {
				deferredFlowNavigation = undefined;
				invalidateFlowNavigation();
			}
			activeGeneration = request.generation;
			if (request.kind !== "commit-ack") {
				stagedNext.discard(session);
			}
			if (request.kind !== "flow-current-ack") {
				discardStagedFlowEvent();
			}
			if (request.kind !== "flow-current-ack" && request.kind !== "seek") {
				discardStagedFlowSeek();
			}
			if (
				request.kind !== "current-ack" &&
				request.kind !== "flow-current-ack"
			) {
				discardStagedCurrent();
			}
			if (
				request.kind !== "seek" &&
				request.kind !== "current-ack" &&
				request.kind !== "flow-current-ack"
			) {
				session?.discard_staged_seek();
			}
			switch (request.kind) {
				case "get-flow-catalog": {
					const entries = decodeFlowAlgorithmCatalog(
						flow_algorithm_catalog_json(),
					);
					respond({
						kind: "flow-catalog",
						generation: request.generation,
						entries,
						conformance: decodeFlowAlgorithmConformanceContracts(
							flow_algorithm_conformance_contracts_json(),
							entries,
						),
					});
					break;
				}
				case "get-flow-generator-fixtures": {
					respond({
						kind: "flow-generator-fixtures",
						generation: request.generation,
						fixtures: decodeFlowGeneratorFixtureManifest(
							flow_generator_fixture_manifest_json(),
						),
					});
					break;
				}
				case "flow-current-ack": {
					const acknowledgement = acknowledgeFlowCurrent(
						request.generation,
						request.publicationId,
						request.accepted,
					);
					if (
						acknowledgement !== undefined &&
						deferredFlowNavigation !== undefined
					) {
						const deferred = deferredFlowNavigation;
						deferredFlowNavigation = undefined;
						if (
							(acknowledgement.accepted ||
								acknowledgement.kind !== "current") &&
							deferred.generation === activeGeneration
						) {
							deferredFlowNavigation = deferred;
							await runDeferredFlowNavigation();
						}
					}
					break;
				}
				case "current-ack": {
					acknowledgeCurrent(request.generation, request.accepted);
					break;
				}
				case "commit-ack": {
					stagedNext.acknowledge(
						requireSession(),
						request.generation,
						request.accepted,
					);
					break;
				}
				case "create": {
					const decodedSource =
						request.flowInputFormat === "dsl"
							? canonical_flow_dsl_json(request.scenario)
							: request.scenario;
					const source = request.discardProvenance
						? canonicalEditedForPlugin(withoutProvenance(decodedSource))
						: decodedSource;
					if (scenarioPlugin(source) === "flow") {
						const canonical = canonical_flow_scenario_json(source);
						assertFlowAlgorithmIsRunnable(
							canonical,
							selectedAlgorithm(canonical),
						);
					}
					const createdSession = new WasmSession(source);
					let staged = false;
					let canonicalScenario: string;
					let selected: ReturnType<typeof selectedAlgorithm>;
					let packet: ArrayBuffer;
					try {
						canonicalScenario = createdSession.scenario_json();
						selected = selectedAlgorithm(canonicalScenario);
						if (createdSession.plugin_id() === "flow") {
							const publicationId = nextPublicationId.toString();
							nextPublicationId += 1n;
							const parts = await encodePublicationV6(
								{
									pluginOrdinal: createdSession.plugin_ordinal(),
									payloadSchemaVersion: 3,
									publicationId,
									generation: request.generation.toString(),
								},
								new TextEncoder().encode(createdSession.current_frame_json()),
							);
							if (request.generation !== activeGeneration) {
								createdSession.free();
								break;
							}
							stagedFlowCurrent.stage(
								request.generation,
								publicationId,
								createdSession,
							);
							staged = true;
							respond(
								{
									kind: "flow-ready",
									generation: request.generation,
									publicationId,
									algorithm: createdSession.algorithm_id(),
									parts,
									scenario: prettyScenario(canonicalScenario),
								},
								parts,
							);
							break;
						}
						packet = framePacket(
							"current",
							createdSession.current_frame_json(),
						);
						stagedCurrent = {
							kind: "create",
							generation: request.generation,
							candidate: createdSession,
						};
						staged = true;
						respond(
							{
								kind: "ready",
								generation: request.generation,
								algorithm: createdSession.algorithm_id(),
								algorithmConfig: selected.config,
								itemCount: createdSession.item_count(),
								packet,
								revisionStatus: revisionStatus(canonicalScenario),
								...(request.discardProvenance
									? { scenario: prettyScenario(canonicalScenario) }
									: {}),
							},
							[packet],
						);
					} catch (error: unknown) {
						if (staged) discardStagedCurrent();
						else createdSession.free();
						throw error;
					}
					break;
				}
				case "next": {
					await startFlowNavigation(request);
					break;
				}
				case "seek": {
					await startFlowNavigation(request);
					break;
				}
				case "prepare-dsl": {
					if (
						!fitsUtf8Budget(
							[request.initialDsl, request.operationsDsl],
							MAX_DSL_BYTES,
						)
					) {
						respond({
							kind: "input-diagnostic",
							generation: request.generation,
							stream: "operations",
							code: "DSL_BYTE_LIMIT",
							line: 1,
							column: 1,
							message: "manual input exceeds the combined 64 MiB limit",
						});
						break;
					}
					try {
						validate_dsl_document_size(
							request.initialDsl,
							request.operationsDsl,
						);
					} catch (error: unknown) {
						const diagnostic = parseDslDiagnostic(error);
						if (diagnostic === undefined) throw error;
						respond({
							kind: "input-diagnostic",
							generation: request.generation,
							stream: "operations",
							...diagnostic,
						});
						break;
					}
					const parsed = JSON.parse(request.scenario) as {
						payload: {
							initial: { entries: unknown[]; provenance?: unknown };
							operations: { items: unknown[]; provenance?: unknown };
						};
					};
					try {
						parsed.payload.initial.entries = JSON.parse(
							parse_initial_dsl_json(request.initialDsl),
						);
					} catch (error: unknown) {
						const diagnostic = parseDslDiagnostic(error);
						if (diagnostic === undefined) throw error;
						respond({
							kind: "input-diagnostic",
							generation: request.generation,
							stream: "initial",
							...diagnostic,
						});
						break;
					}
					try {
						parsed.payload.operations.items = JSON.parse(
							parse_operations_dsl_json(request.operationsDsl),
						);
					} catch (error: unknown) {
						const diagnostic = parseDslDiagnostic(error);
						if (diagnostic === undefined) throw error;
						respond({
							kind: "input-diagnostic",
							generation: request.generation,
							stream: "operations",
							...diagnostic,
						});
						break;
					}
					delete parsed.payload.initial.provenance;
					delete parsed.payload.operations.provenance;
					const canonical = canonical_edited_scenario_json(
						JSON.stringify(parsed),
					);
					respond({
						kind: "scenario-prepared",
						generation: request.generation,
						scenario: prettyScenario(canonical),
						revisionStatus: "current",
					});
					break;
				}
				case "generate": {
					const parsed = JSON.parse(request.scenario) as {
						payload: {
							initial: { entries: unknown[]; provenance?: unknown };
							operations: { items: unknown[]; provenance?: unknown };
						};
					};
					const generated = decodeGenerated(
						request.stream === "initial"
							? generate_initial_json(request.spec)
							: generate_operations_json(
									request.spec,
									JSON.stringify(parsed.payload.initial.entries),
								),
						request.stream,
					);
					if (request.stream === "initial") {
						parsed.payload.initial.entries = generated.items;
						parsed.payload.initial.provenance = generated.provenance;
						delete parsed.payload.operations.provenance;
					} else {
						parsed.payload.operations.items = generated.items;
						parsed.payload.operations.provenance = generated.provenance;
					}
					const canonical = canonical_edited_scenario_json(
						JSON.stringify(parsed),
					);
					respond({
						kind: "scenario-prepared",
						generation: request.generation,
						scenario: prettyScenario(canonical),
						stats: generated.stats,
						revisionStatus: "current",
					});
					break;
				}
				case "format-dsl": {
					const parsed = JSON.parse(request.scenario) as {
						payload: {
							initial: { entries: { key: string; value: string }[] };
							operations: {
								items: (
									| { op: "insert"; key: string; value: string }
									| { op: "remove" | "get" | "lower_bound"; key: string }
								)[];
							};
						};
					};
					const initialDsl = parsed.payload.initial.entries
						.map(
							(entry) => `insert ${entry.key} ${JSON.stringify(entry.value)}`,
						)
						.join("\n");
					const operationsDsl = parsed.payload.operations.items
						.map((operation) =>
							operation.op === "insert"
								? `insert ${operation.key} ${JSON.stringify(operation.value)}`
								: `${operation.op} ${operation.key}`,
						)
						.join("\n");
					respond({
						kind: "dsl-formatted",
						generation: request.generation,
						initialDsl,
						operationsDsl,
					});
					break;
				}
				case "import-scenario": {
					if (
						request.byteLength !== request.bytes.byteLength ||
						request.byteLength > 64 * 1024 * 1024
					) {
						throw new Error(
							"Scenario import buffer has an invalid byte length",
						);
					}
					const source = new TextDecoder("utf-8", { fatal: true }).decode(
						request.bytes,
					);
					const canonical = canonicalForPlugin(source);
					const selected = selectedAlgorithm(canonical);
					const plugin = scenarioPlugin(canonical);
					respond({
						kind: "scenario-prepared",
						generation: request.generation,
						scenario: prettyScenario(canonical),
						revisionStatus:
							plugin === "flow" ? "current" : revisionStatus(canonical),
						algorithm: selected.id,
						algorithmConfig: selected.config,
					});
					break;
				}
				case "set-algorithm": {
					if (scenarioPlugin(request.scenario) === "flow") {
						assertFlowAlgorithmIsRunnable(request.scenario, {
							id: request.algorithm,
							config: request.config,
						});
					}
					const parsed = JSON.parse(request.scenario) as {
						payload: {
							algorithm: { id: string; config: Record<string, unknown> };
						};
					};
					parsed.payload.algorithm = {
						id: request.algorithm,
						config: request.config,
					};
					const canonical = canonicalEditedForPlugin(JSON.stringify(parsed));
					respond({
						kind: "scenario-prepared",
						generation: request.generation,
						scenario: prettyScenario(canonical),
						algorithm: request.algorithm,
						algorithmConfig: request.config,
						revisionStatus: "current",
					});
					break;
				}
				case "export-scenario": {
					const canonical = request.discardProvenance
						? canonicalEditedForPlugin(withoutProvenance(request.scenario))
						: canonicalForPlugin(request.scenario);
					const plugin = scenarioPlugin(canonical);
					respond({
						kind: "scenario-exported",
						generation: request.generation,
						canonical,
						scenario: prettyScenario(canonical),
						revisionStatus:
							plugin === "flow" ? "current" : revisionStatus(canonical),
					});
					break;
				}
			}
		})
		.catch((error: unknown) =>
			fail(
				request.generation,
				error,
				error instanceof StagedNextRollbackError ||
					error instanceof EngineBootstrapError
					? "engine"
					: engineRequestErrorSource(request.kind, error),
				request.kind,
				request.kind === "next" || request.kind === "seek"
					? request.requestSerial
					: undefined,
			),
		);
});
