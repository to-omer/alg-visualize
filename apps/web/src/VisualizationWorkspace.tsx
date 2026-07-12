import { type ComponentProps, lazy, Suspense } from "react";

import { CanvasHeader } from "./CanvasHeader";
import { InputPanel } from "./InputPanel";
import { InspectorPanel } from "./InspectorPanel";

const PixiCanvas = lazy(async () => {
	const module = await import("./PixiCanvas");
	return { default: module.PixiCanvas };
});

type VisualizationWorkspaceProps = {
	canvas: ComponentProps<typeof PixiCanvas>;
	caption: {
		detail: string;
		error: string | undefined;
		guidance: string;
		title: string;
	};
	empty: boolean;
	header: ComponentProps<typeof CanvasHeader>;
	input: ComponentProps<typeof InputPanel>;
	inspector: ComponentProps<typeof InspectorPanel>;
	query: { key: string | undefined; ownedByNode: boolean };
};

export function VisualizationWorkspace({
	canvas,
	caption,
	empty,
	header,
	input,
	inspector,
	query,
}: VisualizationWorkspaceProps) {
	return (
		<main className="workspace">
			<InputPanel {...input} />

			<section className="canvas-panel" aria-label="Visualization canvas">
				<CanvasHeader {...header} />
				<div className="canvas-viewport">
					<Suspense
						fallback={
							<div className="canvas-loading" role="status">
								Preparing renderer…
							</div>
						}
					>
						<PixiCanvas {...canvas} />
					</Suspense>
					{query.key !== undefined && !query.ownedByNode && (
						<div className="query-operand" data-testid="query-operand">
							<span>QUERY</span>
							<strong>{query.key}</strong>
						</div>
					)}
					{empty && (
						<div className="canvas-empty" aria-hidden="true">
							<span className="canvas-empty-mark">∴</span>
							<strong>No structure loaded</strong>
							<p>
								Load the editable Scenario to inspect its stable identities and
								trace.
							</p>
						</div>
					)}
				</div>
				<div className="trace-caption" aria-live="polite">
					<span className="trace-pulse" aria-hidden="true" />
					<div>
						<strong>{caption.error ?? caption.title}</strong>
						<p>
							{caption.error === undefined ? caption.detail : caption.guidance}
						</p>
					</div>
				</div>
			</section>

			<InspectorPanel {...inspector} />
		</main>
	);
}
