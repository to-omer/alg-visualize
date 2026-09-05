import {
	createContext,
	type ReactNode,
	useContext,
	useId,
	useMemo,
} from "react";

/** Produces a document-unique, selector-safe prefix for one rendered instance. */
export function useFlowDomIdScope(namespace: string): string {
	const reactId = useId();
	return useMemo(
		() => `${namespace}-${reactId.replace(/[^A-Za-z0-9_-]/g, "")}`,
		[namespace, reactId],
	);
}

export function flowScopedDomId(scope: string, localId: string): string {
	return `${scope}-${localId}`;
}

export function flowScopedSvgUrl(scope: string, localId: string): string {
	return `url(#${flowScopedDomId(scope, localId)})`;
}

const FlowGraphIdScopeContext = createContext<string | undefined>(undefined);

export function FlowGraphIdScopeProvider({
	scope,
	children,
}: Readonly<{ scope: string; children: ReactNode }>) {
	return (
		<FlowGraphIdScopeContext.Provider value={scope}>
			{children}
		</FlowGraphIdScopeContext.Provider>
	);
}

export function useFlowGraphIdScope(): string {
	const scope = useContext(FlowGraphIdScopeContext);
	if (scope === undefined) {
		throw new Error("Flow graph SVG IDs require an instance scope");
	}
	return scope;
}
