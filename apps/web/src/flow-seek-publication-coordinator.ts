import type { FlowEventAcknowledgement } from "./flow-event-publication-coordinator";

type PendingFlowSeek = {
	generation: number;
	publicationId: string;
	sessionSerial: number;
};

export type FlowSeekOperation = Readonly<{
	operation: number;
	discardedSessionSerial?: number;
}>;

/** Owns one async flow seek from begin through exact V6 ACK identity. */
export class FlowSeekPublicationCoordinator {
	#operation = 0;
	#pending: PendingFlowSeek | undefined;

	begin(): FlowSeekOperation {
		const discardedSessionSerial = this.#pending?.sessionSerial;
		this.#pending = undefined;
		this.#operation += 1;
		return discardedSessionSerial === undefined
			? { operation: this.#operation }
			: { operation: this.#operation, discardedSessionSerial };
	}

	isCurrent(operation: number): boolean {
		return operation === this.#operation;
	}

	stage(
		operation: number,
		generation: number,
		publicationId: string,
		sessionSerial: number,
	): void {
		if (!this.isCurrent(operation)) {
			throw new Error("A stale flow seek cannot stage a publication");
		}
		if (this.#pending !== undefined) {
			throw new Error("A flow seek publication is already awaiting ACK");
		}
		this.#pending = { generation, publicationId, sessionSerial };
	}

	acknowledge(
		generation: number,
		publicationId: string,
		accepted: boolean,
	): FlowEventAcknowledgement {
		const pending = this.#pending;
		if (
			pending === undefined ||
			pending.generation !== generation ||
			pending.publicationId !== publicationId
		) {
			return { kind: "ignored" };
		}
		this.#pending = undefined;
		return accepted
			? { kind: "accepted", sessionSerial: pending.sessionSerial }
			: { kind: "rejected", sessionSerial: pending.sessionSerial };
	}

	cancel(): number | undefined {
		const sessionSerial = this.#pending?.sessionSerial;
		this.#pending = undefined;
		this.#operation += 1;
		return sessionSerial;
	}

	hasPending(): boolean {
		return this.#pending !== undefined;
	}
}
