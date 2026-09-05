export type DisposableCandidate = { free: () => void };

type PendingCandidate<T> = {
	generation: number;
	publicationId: string;
	candidate: T;
};

export type PublicationAcknowledgement<T> =
	| { kind: "ignored" }
	| { kind: "rejected" }
	| { kind: "accepted"; candidate: T };

/** Owns at most one unpublished V6 candidate and requires an exact ACK. */
export class PublicationCandidateCoordinator<T extends DisposableCandidate> {
	#pending: PendingCandidate<T> | undefined;

	stage(generation: number, publicationId: string, candidate: T): void {
		if (this.#pending !== undefined) {
			throw new Error("A publication candidate is already awaiting ACK");
		}
		this.#pending = { generation, publicationId, candidate };
	}

	acknowledge(
		generation: number,
		publicationId: string,
		accepted: boolean,
	): PublicationAcknowledgement<T> {
		const pending = this.#pending;
		if (
			pending === undefined ||
			pending.generation !== generation ||
			pending.publicationId !== publicationId
		) {
			return { kind: "ignored" };
		}
		this.#pending = undefined;
		if (!accepted) {
			pending.candidate.free();
			return { kind: "rejected" };
		}
		return { kind: "accepted", candidate: pending.candidate };
	}

	discard(): void {
		const pending = this.#pending;
		this.#pending = undefined;
		pending?.candidate.free();
	}

	hasPending(): boolean {
		return this.#pending !== undefined;
	}
}
