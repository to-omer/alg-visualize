export type StagedNextSession = {
	commit_staged_next: () => void;
	discard_staged_next: () => void;
};

export class StagedNextRollbackError extends Error {
	constructor(cause: unknown) {
		super(cause instanceof Error ? cause.message : String(cause), { cause });
		this.name = "StagedNextRollbackError";
	}
}

export class StagedNextCoordinator {
	private generation: number | undefined;

	stage(generation: number) {
		if (this.generation !== undefined) {
			throw new Error("a staged next operation is already pending");
		}
		this.generation = generation;
	}

	acknowledge(
		session: StagedNextSession,
		generation: number,
		accepted: boolean,
	) {
		if (this.generation !== generation) return;
		if (!accepted) {
			this.discard(session);
			return;
		}
		this.generation = undefined;
		session.commit_staged_next();
	}

	discard(session: StagedNextSession | undefined) {
		if (this.generation === undefined) return;
		this.generation = undefined;
		try {
			session?.discard_staged_next();
		} catch (error: unknown) {
			throw new StagedNextRollbackError(error);
		}
	}
}
