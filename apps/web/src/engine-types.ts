import type { FlowAlgorithmCatalogEntry } from "./flow-algorithm-catalog";
import type { FlowAlgorithmConformanceContract } from "./flow-algorithm-conformance";

export type ArenaKey = {
	index: number;
	generation: number;
};

export type StructureEntityId =
	| { kind: "node"; id: ArenaKey }
	| { kind: "auxiliary"; id: ArenaKey };

export type StructureLink = {
	slot: number;
	role: string;
	target: StructureEntityId;
};

export type StructureNode = {
	id: StructureEntityId;
	role: string;
	entries: ArenaKey[];
	keys: string[];
	links: StructureLink[];
	metadata: [string, string][];
};

export type StructureSnapshot = {
	root: StructureEntityId | null;
	nodes: StructureNode[];
};

export type Metrics = {
	comparisons: string;
	node_visits: string;
	bit_tests: string;
	rotations: string;
	recolors: string;
	splits: string;
	merges: string;
	rebuild_items: string;
	allocations: string;
	frees: string;
};

export type CanonicalEntry = {
	id: ArenaKey;
	key: string;
	value: string;
};

export type CanonicalSnapshot = {
	entries: CanonicalEntry[];
	metrics: Metrics;
};

export type TraceKind =
	| "compare"
	| "descend"
	| "insert"
	| "overwrite"
	| "remove"
	| "rotate-left"
	| "rotate-right"
	| "update-metadata"
	| "rebuild"
	| "split"
	| "merge"
	| "move-entry"
	| "result";

export type TraceEvent = {
	catalog_id: number;
	kind: TraceKind;
	node: StructureEntityId | null;
	target: StructureEntityId | null;
	entry: ArenaKey | null;
	key: string | null;
	patch_count: number;
	patch_start: number;
};

export type MetricOrdinal =
	| "comparisons"
	| "node-visits"
	| "bit-tests"
	| "rotations"
	| "recolors"
	| "splits"
	| "merges"
	| "rebuild-items"
	| "allocations"
	| "frees";

export type StatePatchRecord =
	| {
			kind: "root";
			before: StructureEntityId | null;
			after: StructureEntityId | null;
	  }
	| {
			kind: "node";
			id: StructureEntityId;
			before: StructureNode | null;
			after: StructureNode | null;
	  }
	| {
			kind: "entry";
			id: ArenaKey;
			before: CanonicalEntry | null;
			after: CanonicalEntry | null;
	  }
	| {
			kind: "metric";
			ordinal: MetricOrdinal;
			before: string;
			after: string;
	  };

export type InputDiagnostic = {
	stream: "initial" | "operations";
	code: string;
	line: number;
	column: number;
	message: string;
};

export type OperationResult =
	| { kind: "inserted"; entry: ArenaKey }
	| { kind: "overwritten"; entry: ArenaKey; previous: string }
	| { kind: "removed"; entry: ArenaKey; value: string }
	| { kind: "miss" }
	| { kind: "found"; entry: ArenaKey; key: string; value: string };

export type CurrentFrame = {
	itemIndex: number;
	itemCount: number;
	structure: StructureSnapshot;
	canonical: CanonicalSnapshot;
};

export type CommitFrame = {
	baseItemIndex: number;
	itemIndex: number;
	itemCount: number;
	initialBuild: boolean;
	result: OperationResult;
	trace: TraceEvent[];
	patches: StatePatchRecord[];
};

export type EngineFrame = CurrentFrame &
	Partial<
		Pick<
			CommitFrame,
			"baseItemIndex" | "initialBuild" | "result" | "trace" | "patches"
		>
	>;

export type EngineRequest =
	| { kind: "get-flow-catalog"; generation: number }
	| { kind: "get-flow-generator-fixtures"; generation: number }
	| {
			kind: "create";
			generation: number;
			scenario: string;
			discardProvenance: boolean;
			flowInputFormat?: "json" | "dsl";
	  }
	| {
			kind: "next";
			generation: number;
			/** UI-owned identity used to reject a navigation replaced by pause or seek. */
			requestSerial?: number;
	  }
	| {
			kind: "commit-ack";
			generation: number;
			accepted: boolean;
	  }
	| {
			kind: "current-ack";
			generation: number;
			accepted: boolean;
	  }
	| {
			kind: "flow-current-ack";
			generation: number;
			publicationId: string;
			accepted: boolean;
	  }
	| {
			kind: "seek";
			generation: number;
			target: number;
			/** UI-owned identity used to reject a response from a replaced seek. */
			requestSerial?: number;
	  }
	| {
			kind: "prepare-dsl";
			generation: number;
			scenario: string;
			initialDsl: string;
			operationsDsl: string;
	  }
	| {
			kind: "generate";
			generation: number;
			scenario: string;
			spec: string;
			stream: "initial" | "operations";
	  }
	| { kind: "format-dsl"; generation: number; scenario: string }
	| {
			kind: "import-scenario";
			generation: number;
			byteLength: number;
			bytes: ArrayBuffer;
	  }
	| {
			kind: "export-scenario";
			generation: number;
			scenario: string;
			discardProvenance: boolean;
	  }
	| {
			kind: "set-algorithm";
			generation: number;
			scenario: string;
			algorithm: string;
			config: Record<string, unknown>;
	  };

export type EngineResponse =
	| {
			kind: "flow-catalog";
			generation: number;
			entries: FlowAlgorithmCatalogEntry[];
			conformance: FlowAlgorithmConformanceContract[];
	  }
	| {
			kind: "flow-generator-fixtures";
			generation: number;
			fixtures: import("./flow-generator-fixture").FlowGeneratorFixture[];
	  }
	| {
			kind: "ready";
			generation: number;
			algorithm: string;
			algorithmConfig: Record<string, unknown>;
			itemCount: number;
			packet: ArrayBuffer;
			revisionStatus: "current" | "legacy-derived";
			scenario?: string;
	  }
	| { kind: "commit"; generation: number; packet: ArrayBuffer }
	| {
			kind: "flow-ready";
			generation: number;
			publicationId: string;
			algorithm: string;
			parts: ArrayBuffer[];
			scenario: string;
	  }
	| {
			kind: "flow-update";
			generation: number;
			publicationId: string;
			algorithm: string;
			parts: ArrayBuffer[];
			/** Present when this publication completes a serial-owned navigation. */
			seekRequestSerial?: number;
	  }
	| {
			kind: "seeked";
			generation: number;
			packet: ArrayBuffer;
			seekRequestSerial?: number;
	  }
	| {
			kind: "seek-progress";
			generation: number;
			cursor: number;
			target: number;
			seekRequestSerial?: number;
	  }
	| {
			kind: "index-progress" | "index-ready";
			generation: number;
			coverage: number;
			itemCount: number;
	  }
	| { kind: "index-error"; generation: number; message: string }
	| {
			kind: "scenario-prepared";
			generation: number;
			scenario: string;
			stats?: Record<string, number>;
			algorithm?: string;
			algorithmConfig?: Record<string, unknown>;
			revisionStatus: "current" | "legacy-derived";
	  }
	| {
			kind: "dsl-formatted";
			generation: number;
			initialDsl: string;
			operationsDsl: string;
	  }
	| {
			kind: "input-diagnostic";
			generation: number;
			stream: "initial" | "operations";
			code: string;
			line: number;
			column: number;
			message: string;
	  }
	| {
			kind: "scenario-exported";
			generation: number;
			canonical: string;
			scenario: string;
			revisionStatus: "current" | "legacy-derived";
	  }
	| {
			kind: "ended";
			generation: number;
			seekRequestSerial?: number;
	  }
	| {
			kind: "error";
			generation: number;
			/** Identifies the failed request without consulting mutable UI state. */
			requestKind: EngineRequest["kind"];
			message: string;
			source: "engine" | "input";
			seekRequestSerial?: number;
	  };

export function entityIdKey(id: StructureEntityId): string {
	return `${id.kind}:${id.id.index}:${id.id.generation}`;
}
