export type FlowPublicationLifecycle =
	| Readonly<{ kind: "idle" }>
	| Readonly<{
			kind: "decoding";
			generation: number;
			publicationId: string;
	  }>
	| Readonly<{
			kind: "ready";
			generation: number;
			publicationId: string;
	  }>
	| Readonly<{
			kind: "failed";
			generation: number;
			publicationId: string;
	  }>;

export type FlowPublicationLifecycleAction =
	| Readonly<{
			kind: "stage";
			generation: number;
			publicationId: string;
	  }>
	| Readonly<{
			kind: "acknowledge";
			generation: number;
			publicationId: string;
			accepted: boolean;
	  }>;

export const INITIAL_FLOW_PUBLICATION_LIFECYCLE: FlowPublicationLifecycle = {
	kind: "idle",
};

export function assertFlowPublicationAlgorithmIdentity(
	envelopeAlgorithm: string,
	payloadAlgorithm: string,
): void {
	if (envelopeAlgorithm !== payloadAlgorithm) {
		throw new Error(
			"Flow publication algorithm does not match its Worker envelope",
		);
	}
}

export function flowPublicationLifecycleReducer(
	state: FlowPublicationLifecycle,
	action: FlowPublicationLifecycleAction,
): FlowPublicationLifecycle {
	if (action.kind === "stage") {
		return {
			kind: "decoding",
			generation: action.generation,
			publicationId: action.publicationId,
		};
	}
	if (
		state.kind !== "decoding" ||
		state.generation !== action.generation ||
		state.publicationId !== action.publicationId
	) {
		return state;
	}
	return {
		kind: action.accepted ? "ready" : "failed",
		generation: action.generation,
		publicationId: action.publicationId,
	};
}
