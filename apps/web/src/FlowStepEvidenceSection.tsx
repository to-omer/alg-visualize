import type { FlowCurrentSceneV9 } from "./flow-scene";
import { projectFlowStepEvidence } from "./flow-step-evidence";

type Props = Readonly<{
	scene: FlowCurrentSceneV9 | undefined;
}>;

/** Keeps the current algorithm action ahead of revision and wire diagnostics. */
export function FlowStepEvidenceSection({ scene }: Props) {
	const evidence = projectFlowStepEvidence(scene);
	if (evidence === undefined) return null;
	return (
		<section
			className="flow-step-evidence"
			data-testid="flow-step-evidence"
			data-evidence-kind="source-event"
		>
			<p className="eyebrow">CURRENT STEP</p>
			<h3>{evidence.action}</h3>
			<dl>
				<div>
					<dt>Work</dt>
					<dd data-testid="flow-step-work">{evidence.work}</dd>
				</div>
				<div>
					<dt>Focus</dt>
					<dd data-testid="flow-step-focus">{evidence.focus}</dd>
				</div>
				<div>
					<dt>Observation</dt>
					<dd data-testid="flow-step-observation">{evidence.observation}</dd>
				</div>
				<div>
					<dt>Effect</dt>
					<dd data-testid="flow-step-effect">{evidence.effect}</dd>
				</div>
			</dl>
			<p className="flow-step-pseudocode">
				<span>Pseudocode</span>
				<code>{evidence.pseudocode}</code>
			</p>
		</section>
	);
}
