import { useId, useState } from "react";
import { flowWorkbenchPolicy } from "./flow-workbench-policy";
import type { FlowWorkbenchProblemKind } from "./flow-workbench-problem";

export function FlowEncodingKey({
	problemKind,
}: Readonly<{
	problemKind: FlowWorkbenchProblemKind;
}>) {
	const panelId = useId();
	const [pinned, setPinned] = useState(false);
	const [hovered, setHovered] = useState(false);
	const [focused, setFocused] = useState(false);
	const open = pinned || hovered || focused;
	const showsCost = flowWorkbenchPolicy(problemKind).showsCost;

	return (
		<fieldset className="flow-encoding-key">
			<legend className="visually-hidden">Visual encoding key</legend>
			<span>
				<i className="flow-key-capacity" />
				Capacity
			</span>
			<span>
				<i className="flow-key-fill" />
				Flow
			</span>
			{showsCost && (
				<span>
					<i className="flow-key-cost" />
					Cost
				</span>
			)}
			<span>
				<i className="flow-key-focus" />
				Current
			</span>
			{/* biome-ignore lint/a11y/noStaticElementInteractions: hover/focus are delegated from the semantic help button so the tooltip remains open while crossing into it. */}
			<div
				className="flow-visual-help"
				onPointerEnter={() => setHovered(true)}
				onPointerLeave={() => setHovered(false)}
				onFocus={() => setFocused(true)}
				onBlur={(event) => {
					if (!event.currentTarget.contains(event.relatedTarget))
						setFocused(false);
				}}
				onKeyDown={(event) => {
					if (event.key !== "Escape") return;
					setPinned(false);
					setHovered(false);
					setFocused(false);
				}}
			>
				<button
					type="button"
					aria-label="Visual encoding help"
					aria-controls={panelId}
					aria-expanded={open}
					onClick={() => setPinned((current) => !current)}
				>
					?
				</button>
				<div
					id={panelId}
					className="flow-visual-help-panel"
					role="tooltip"
					hidden={!open}
				>
					<strong>Edge channels</strong>
					<p>Outer width is capacity. The light inner fill is current flow.</p>
					{showsCost && (
						<p>
							Amber solid is positive cost; cyan dashed is negative; dotted gray
							is zero. Intensity shows magnitude.
						</p>
					)}
					<p>
						Violet outlines mark the current trace focus and never replace data
						colors.
					</p>
				</div>
			</div>
		</fieldset>
	);
}
