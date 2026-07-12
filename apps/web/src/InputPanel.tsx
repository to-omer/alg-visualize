import { lazy, Suspense, useState } from "react";

import type { InputDiagnostic } from "./engine-types";

const EditorPanel = lazy(async () => {
	const module = await import("./EditorPanel");
	return { default: module.EditorPanel };
});

function EditorFallback() {
	return (
		<div className="editor-host editor-loading" data-testid="scenario-editor">
			Loading editor…
		</div>
	);
}

export type InputMode = "json" | "dsl";
type OperationKind = "insert" | "remove" | "get" | "lower_bound";

type InputPanelProps = {
	diagnostic: InputDiagnostic | undefined;
	engineDisabled: boolean;
	initialDsl: string;
	mode: InputMode;
	onApplyDsl: () => void;
	onInitialDslChange: (value: string) => void;
	onModeChange: (mode: InputMode) => void;
	onOperationsDslChange: (value: string) => void;
	onScenarioChange: (value: string) => void;
	operationsDsl: string;
	scenario: string;
	statusLabel: string;
};

export function InputPanel({
	diagnostic,
	engineDisabled,
	initialDsl,
	mode,
	onApplyDsl,
	onInitialDslChange,
	onModeChange,
	onOperationsDslChange,
	onScenarioChange,
	operationsDsl,
	scenario,
	statusLabel,
}: InputPanelProps) {
	const [operation, setOperation] = useState<OperationKind>("insert");
	const [key, setKey] = useState("0");
	const [value, setValue] = useState("");
	const appendOperation = () => {
		const line =
			operation === "insert"
				? `insert ${key} ${JSON.stringify(value)}`
				: `${operation} ${key}`;
		onOperationsDslChange(
			`${operationsDsl}${operationsDsl.length === 0 ? "" : "\n"}${line}`,
		);
	};

	return (
		<aside className="input-panel panel">
			<div className="panel-heading">
				<div>
					<p className="eyebrow">SCENARIO</p>
					<h2>Input</h2>
				</div>
				<div className="panel-heading-actions">
					<fieldset
						className="segmented-control"
						aria-label="Input format"
						disabled={engineDisabled}
					>
						<button
							type="button"
							aria-pressed={mode === "json"}
							onClick={() => onModeChange("json")}
						>
							JSON
						</button>
						<button
							type="button"
							aria-pressed={mode === "dsl"}
							onClick={() => onModeChange("dsl")}
						>
							DSL
						</button>
					</fieldset>
					<span
						className={`status-dot status-${statusLabel}`}
						data-testid="engine-status"
					>
						{statusLabel}
					</span>
				</div>
			</div>
			{mode === "json" ? (
				<Suspense fallback={<EditorFallback />}>
					<EditorPanel
						ariaLabel="Scenario JSON"
						value={scenario}
						onChange={onScenarioChange}
					/>
				</Suspense>
			) : (
				<div className="dsl-input">
					<section>
						<div className="dsl-heading">
							<strong>Initial</strong>
							<span>insert only</span>
						</div>
						<Suspense fallback={<EditorFallback />}>
							<EditorPanel
								ariaLabel="Initial DSL"
								diagnostic={
									diagnostic?.stream === "initial" ? diagnostic : undefined
								}
								language="dsl"
								value={initialDsl}
								onChange={onInitialDslChange}
							/>
						</Suspense>
					</section>
					<section>
						<div className="dsl-heading">
							<strong>Operations</strong>
							<span>up to 100,000</span>
						</div>
						<Suspense fallback={<EditorFallback />}>
							<EditorPanel
								ariaLabel="Operations DSL"
								diagnostic={
									diagnostic?.stream === "operations" ? diagnostic : undefined
								}
								language="dsl"
								value={operationsDsl}
								onChange={onOperationsDslChange}
							/>
						</Suspense>
					</section>
					<fieldset
						className="single-operation"
						aria-label="Append one operation"
					>
						<select
							aria-label="Operation"
							value={operation}
							onChange={(event) =>
								setOperation(event.target.value as OperationKind)
							}
						>
							<option value="insert">insert</option>
							<option value="remove">remove</option>
							<option value="get">get</option>
							<option value="lower_bound">lower_bound</option>
						</select>
						<input
							aria-label="Operation key"
							inputMode="numeric"
							placeholder="key"
							value={key}
							onChange={(event) => setKey(event.target.value)}
						/>
						{operation === "insert" && (
							<input
								aria-label="Operation value"
								placeholder="value"
								value={value}
								onChange={(event) => setValue(event.target.value)}
							/>
						)}
						<button
							type="button"
							className="quiet-button"
							onClick={appendOperation}
						>
							Append
						</button>
					</fieldset>
					<button
						type="button"
						className="primary-button dsl-apply"
						disabled={engineDisabled}
						onClick={onApplyDsl}
					>
						Apply DSL
					</button>
				</div>
			)}
		</aside>
	);
}
