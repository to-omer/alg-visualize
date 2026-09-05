/** Identity of the exact WASM session instance owned by an async operation. */
export type FlowSessionLease<T extends object> = Readonly<{
	session: T;
	serial: number;
}>;

/**
 * Runs a rollback only while the async operation still owns the live wrapper.
 * A replaced WASM wrapper may already have been freed and must never be
 * touched, even for cleanup.
 */
export function runWithActiveFlowSession<T extends object>(
	lease: FlowSessionLease<T>,
	activeSession: T | undefined,
	activeSerial: number,
	action: (session: T) => void,
): boolean {
	if (lease.session !== activeSession || lease.serial !== activeSerial) {
		return false;
	}
	action(lease.session);
	return true;
}

export function ownsActiveFlowSession<T extends object>(
	lease: FlowSessionLease<T>,
	activeSession: T | undefined,
	activeSerial: number,
): boolean {
	return lease.session === activeSession && lease.serial === activeSerial;
}
