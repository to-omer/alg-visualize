type PendingFlowEvent = {
	generation: number;
	publicationId: string;
	sessionSerial: number;
};

export type FlowEventAcknowledgement =
	| { kind: "ignored" }
	| { kind: "rejected"; sessionSerial: number }
	| { kind: "accepted"; sessionSerial: number };

/** Requires exact V6 identity before a staged event mutates its owning session. */
export class FlowEventPublicationCoordinator {
	#pending: PendingFlowEvent | undefined;

	stage(
		generation: number,
		publicationId: string,
		sessionSerial: number,
	): void {
		if (this.#pending !== undefined) {
			throw new Error("A flow event publication is already awaiting ACK");
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

	discard(): number | undefined {
		const serial = this.#pending?.sessionSerial;
		this.#pending = undefined;
		return serial;
	}

	hasPending(): boolean {
		return this.#pending !== undefined;
	}
}
