import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useState } from "react";

import type { AlgorithmId } from "./scenario";

type AlgorithmSettingsDialogProps = {
	algorithm: AlgorithmId;
	config: Record<string, unknown>;
	error: string | undefined;
	onApply: (config: Record<string, unknown>) => void;
	onOpenChange: (open: boolean) => void;
	open: boolean;
};

export function AlgorithmSettingsDialog({
	algorithm,
	config,
	error,
	onApply,
	onOpenChange,
	open,
}: AlgorithmSettingsDialogProps) {
	const [draft, setDraft] = useState(config);
	useEffect(() => {
		if (open) {
			setDraft(config);
		}
	}, [config, open]);
	const numberField = (
		name: string,
		fallback: number,
		minimum: number,
		maximum: number,
	) => (
		<label>
			<span>
				{name.replaceAll("_", " ")}{" "}
				<small>
					({minimum}–{maximum})
				</small>
			</span>
			<input
				aria-label={name.replaceAll("_", " ")}
				inputMode="numeric"
				min={minimum}
				max={maximum}
				value={Number(draft[name] ?? fallback)}
				onChange={(event) =>
					setDraft((current) => ({
						...current,
						[name]: Number(event.target.value),
					}))
				}
			/>
		</label>
	);

	return (
		<Dialog.Root open={open} onOpenChange={onOpenChange}>
			<Dialog.Portal>
				<Dialog.Overlay className="dialog-overlay" />
				<Dialog.Content className="dialog-content algorithm-settings">
					<Dialog.Title>{algorithm} parameters</Dialog.Title>
					<Dialog.Description>
						Parameters that affect correctness are validated by the Rust engine
						before replacing the current Scenario.
					</Dialog.Description>
					{error !== undefined && (
						<p className="dialog-error" role="alert">
							{error}
						</p>
					)}
					<div className="generator-grid">
						{algorithm === "scapegoat" && (
							<>
								{numberField("alpha_numerator", 2, 1, 63)}
								{numberField("alpha_denominator", 3, 2, 64)}
							</>
						)}
						{algorithm === "skip-list" && (
							<>
								<label>
									promotion
									<select
										value={String(draft.promotion ?? "1/2")}
										onChange={(event) =>
											setDraft((current) => ({
												...current,
												promotion: event.target.value,
											}))
										}
									>
										<option value="1/2">1/2</option>
										<option value="1/4">1/4</option>
									</select>
								</label>
								{numberField("max_level", 16, 1, 64)}
							</>
						)}
						{algorithm === "b-tree" && numberField("min_degree", 3, 2, 16)}
						{(algorithm === "veb" ||
							algorithm === "x-fast" ||
							algorithm === "y-fast") &&
							numberField("word_bits", 16, 1, 64)}
					</div>
					{Object.keys(draft).length === 0 && (
						<p>
							This implementation has no user-adjustable correctness parameters.
						</p>
					)}
					<div className="dialog-actions">
						<Dialog.Close asChild>
							<button type="button" className="quiet-button">
								Cancel
							</button>
						</Dialog.Close>
						<button
							type="button"
							className="primary-button"
							onClick={() => onApply(draft)}
						>
							Apply
						</button>
					</div>
				</Dialog.Content>
			</Dialog.Portal>
		</Dialog.Root>
	);
}
