import init, {
	canonical_edited_scenario_json,
	canonical_scenario_json,
	generate_initial_json,
	generate_operations_json,
	parse_initial_dsl_json,
	parse_operations_dsl_json,
	scenario_has_legacy_derived_revisions,
	validate_dsl_document_size,
	WasmSession,
} from "../../../packages/wasm/visualizer_engine.js";
import { engineRequestErrorSource } from "./engine-error-source";
import type {
	CurrentFrame,
	EngineRequest,
	EngineResponse,
} from "./engine-types";
import { encodeFramePacket, type FramePacketKind } from "./packet";
import {
	StagedNextCoordinator,
	StagedNextRollbackError,
} from "./staged-next-coordinator";
import { fitsUtf8Budget } from "./utf8-budget";

let session: WasmSession | undefined;
let activeGeneration = 0;
let sessionSerial = 0;
const stagedNext = new StagedNextCoordinator();

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
const initialized = init().catch((error: unknown) => {
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
	if (staged?.kind === "create") {
		staged.candidate.free();
	} else if (
		staged?.kind === "seek" &&
		staged.sessionSerial === sessionSerial
	) {
		session?.discard_staged_seek();
	}
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

function fail(generation: number, error: unknown, source: "engine" | "input") {
	respond({
		kind: "error",
		generation,
		message: error instanceof Error ? error.message : String(error),
		source,
	});
}

function withoutProvenance(scenario: string): string {
	const parsed = JSON.parse(scenario) as {
		payload: {
			initial: { provenance?: unknown };
			operations: { provenance?: unknown };
		};
	};
	delete parsed.payload.initial.provenance;
	delete parsed.payload.operations.provenance;
	return JSON.stringify(parsed);
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

function runSeek(generation: number, target: number) {
	requireSession().begin_seek(target);
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
					respond({ kind: "seeked", generation, packet }, [packet]);
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
			});
			self.setTimeout(resume, 0);
		} catch (error: unknown) {
			session?.discard_staged_seek();
			fail(generation, error, engineRequestErrorSource("seek", error));
		}
	};
	resume();
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

self.addEventListener("message", (event: MessageEvent<EngineRequest>) => {
	const request = event.data;
	void initialized
		.then(() => {
			if (initializationFailure !== undefined) {
				throw initializationFailure;
			}
			if (request.generation < activeGeneration) {
				return;
			}
			activeGeneration = request.generation;
			if (request.kind !== "commit-ack") {
				stagedNext.discard(session);
			}
			if (request.kind !== "current-ack") {
				discardStagedCurrent();
			}
			if (request.kind !== "seek" && request.kind !== "current-ack") {
				session?.discard_staged_seek();
			}
			switch (request.kind) {
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
					const source = request.discardProvenance
						? canonical_edited_scenario_json(
								withoutProvenance(request.scenario),
							)
						: request.scenario;
					const createdSession = new WasmSession(source);
					let staged = false;
					let canonicalScenario: string;
					let selected: ReturnType<typeof selectedAlgorithm>;
					let packet: ArrayBuffer;
					try {
						canonicalScenario = createdSession.scenario_json();
						selected = selectedAlgorithm(canonicalScenario);
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
					const activeSession = requireSession();
					const json = activeSession.stage_next_json();
					if (json === undefined) {
						respond({ kind: "ended", generation: request.generation });
					} else {
						stagedNext.stage(request.generation);
						try {
							const packet = framePacket("commit", json);
							respond(
								{
									kind: "commit",
									generation: request.generation,
									packet,
								},
								[packet],
							);
						} catch (error: unknown) {
							stagedNext.discard(activeSession);
							throw error;
						}
					}
					break;
				}
				case "seek": {
					runSeek(request.generation, request.target);
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
					const canonical = canonical_scenario_json(source);
					const selected = selectedAlgorithm(canonical);
					respond({
						kind: "scenario-prepared",
						generation: request.generation,
						scenario: prettyScenario(canonical),
						revisionStatus: revisionStatus(canonical),
						algorithm: selected.id,
						algorithmConfig: selected.config,
					});
					break;
				}
				case "set-algorithm": {
					const parsed = JSON.parse(request.scenario) as {
						payload: {
							algorithm: { id: string; config: Record<string, unknown> };
						};
					};
					parsed.payload.algorithm = {
						id: request.algorithm,
						config: request.config,
					};
					const canonical = canonical_edited_scenario_json(
						JSON.stringify(parsed),
					);
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
						? canonical_edited_scenario_json(
								withoutProvenance(request.scenario),
							)
						: canonical_scenario_json(request.scenario);
					respond({
						kind: "scenario-exported",
						generation: request.generation,
						canonical,
						scenario: prettyScenario(canonical),
						revisionStatus: revisionStatus(canonical),
					});
					break;
				}
				case "dispose":
					sessionSerial += 1;
					session?.free();
					session = undefined;
					break;
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
			),
		);
});
