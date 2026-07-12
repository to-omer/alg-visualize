import { useRef } from "react";

import { ALGORITHMS, type AlgorithmId } from "./scenario";

type TopBarProps = {
	algorithm: AlgorithmId;
	atEnd: boolean;
	disabled: boolean;
	onAlgorithmChange: (algorithm: AlgorithmId) => void;
	onExport: () => void;
	onGenerate: () => void;
	onImport: (file: File | undefined) => void;
	onLoad: () => void;
	onParameters: () => void;
	onRun: () => void;
	playing: boolean;
};

export function TopBar({
	algorithm,
	atEnd,
	disabled,
	onAlgorithmChange,
	onExport,
	onGenerate,
	onImport,
	onLoad,
	onParameters,
	onRun,
	playing,
}: TopBarProps) {
	const fileInput = useRef<HTMLInputElement>(null);
	return (
		<header className="topbar">
			<div className="brand-block">
				<span className="brand-mark" aria-hidden="true" />
				<div>
					<p className="eyebrow">ALGORITHM WORKBENCH</p>
					<h1>Ordered Map</h1>
				</div>
			</div>
			<fieldset
				className="operation-strip"
				aria-label="Operation controls"
				disabled={disabled}
			>
				<label>
					<span>Structure</span>
					<select
						aria-label="Structure"
						value={algorithm}
						onChange={(event) =>
							onAlgorithmChange(event.target.value as AlgorithmId)
						}
					>
						{ALGORITHMS.map(([id, label]) => (
							<option key={id} value={id}>
								{label}
							</option>
						))}
					</select>
				</label>
				<button
					type="button"
					className="quiet-button"
					onClick={() => fileInput.current?.click()}
				>
					Import
				</button>
				<input
					accept="application/json,.json"
					aria-hidden="true"
					className="file-input"
					ref={fileInput}
					tabIndex={-1}
					type="file"
					onChange={(event) => onImport(event.target.files?.[0])}
				/>
				<button type="button" className="quiet-button" onClick={onExport}>
					Export
				</button>
				<button type="button" className="quiet-button" onClick={onGenerate}>
					Generate
				</button>
				<button type="button" className="quiet-button" onClick={onParameters}>
					Parameters
				</button>
				<button type="button" className="quiet-button" onClick={onLoad}>
					Load
				</button>
				<button type="button" className="primary-button" onClick={onRun}>
					{playing ? "Pause" : atEnd ? "Replay trace" : "Run trace"}
				</button>
			</fieldset>
		</header>
	);
}
