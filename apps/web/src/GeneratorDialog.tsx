import * as Dialog from "@radix-ui/react-dialog";

export type GeneratorStream = "initial" | "operations";
export type KeyDistribution =
	| "uniform"
	| "ascending"
	| "descending"
	| "hotspot";

export type GeneratorForm = {
	stream: GeneratorStream;
	seed: string;
	count: number;
	keyMin: string;
	keyMax: string;
	distribution: KeyDistribution;
	valuePrefix: string;
	valueMaxScalarValues: number;
	insertWeight: number;
	removeWeight: number;
	getWeight: number;
	lowerBoundWeight: number;
	getHitRate: number;
	removeHitRate: number;
	overwriteRate: number;
};

export const DEFAULT_GENERATOR: GeneratorForm = {
	stream: "operations",
	seed: "42",
	count: 20,
	keyMin: "0",
	keyMax: "255",
	distribution: "uniform",
	valuePrefix: "value-",
	valueMaxScalarValues: 24,
	insertWeight: 1,
	removeWeight: 1,
	getWeight: 1,
	lowerBoundWeight: 1,
	getHitRate: 5_000,
	removeHitRate: 5_000,
	overwriteRate: 2_500,
};

type GeneratorDialogProps = {
	error: string | undefined;
	form: GeneratorForm;
	onChange: (form: GeneratorForm) => void;
	onGenerate: () => void;
	onOpenChange: (open: boolean) => void;
	open: boolean;
};

export function GeneratorDialog({
	error,
	form,
	onChange,
	onGenerate,
	onOpenChange,
	open,
}: GeneratorDialogProps) {
	const update = <Key extends keyof GeneratorForm>(
		key: Key,
		value: GeneratorForm[Key],
	) => {
		onChange({ ...form, [key]: value });
	};

	return (
		<Dialog.Root open={open} onOpenChange={onOpenChange}>
			<Dialog.Portal>
				<Dialog.Overlay className="dialog-overlay" />
				<Dialog.Content
					className="dialog-content"
					data-testid="generator-dialog"
				>
					<Dialog.Title>Generate deterministic input</Dialog.Title>
					<Dialog.Description>
						The Rust generator materializes every item and records its exact
						seed, targets, achieved statistics, and digest in the Scenario.
					</Dialog.Description>
					{error !== undefined && (
						<p className="dialog-error" role="alert">
							{error}
						</p>
					)}
					<div className="generator-grid">
						<label>
							Target
							<select
								value={form.stream}
								onChange={(event) =>
									update("stream", event.target.value as GeneratorStream)
								}
							>
								<option value="initial">Initial entries</option>
								<option value="operations">Operations</option>
							</select>
						</label>
						<label>
							Seed
							<input
								value={form.seed}
								inputMode="numeric"
								onChange={(event) => update("seed", event.target.value)}
							/>
						</label>
						<label>
							Count
							<input
								value={form.count}
								inputMode="numeric"
								max={form.stream === "initial" ? 10_000 : 100_000}
								min={0}
								onChange={(event) =>
									update("count", Number(event.target.value))
								}
							/>
						</label>
						<label>
							Distribution
							<select
								value={form.distribution}
								onChange={(event) =>
									update("distribution", event.target.value as KeyDistribution)
								}
							>
								<option value="uniform">Uniform</option>
								<option value="ascending">Ascending</option>
								<option value="descending">Descending</option>
								<option value="hotspot">Hotspot</option>
							</select>
						</label>
						<label>
							Key minimum
							<input
								value={form.keyMin}
								inputMode="numeric"
								onChange={(event) => update("keyMin", event.target.value)}
							/>
						</label>
						<label>
							Key maximum
							<input
								value={form.keyMax}
								inputMode="numeric"
								onChange={(event) => update("keyMax", event.target.value)}
							/>
						</label>
						<label>
							Value prefix
							<input
								value={form.valuePrefix}
								onChange={(event) => update("valuePrefix", event.target.value)}
							/>
						</label>
						<label>
							Maximum value length
							<input
								value={form.valueMaxScalarValues}
								inputMode="numeric"
								max={256}
								min={0}
								onChange={(event) =>
									update("valueMaxScalarValues", Number(event.target.value))
								}
							/>
						</label>
					</div>
					{form.stream === "operations" && (
						<fieldset className="generator-group">
							<legend>Operation weights</legend>
							{(
								[
									["insert", "insertWeight"],
									["remove", "removeWeight"],
									["get", "getWeight"],
									["lower_bound", "lowerBoundWeight"],
								] as const
							).map(([label, field]) => (
								<label key={field}>
									{label}
									<input
										value={form[field]}
										inputMode="numeric"
										min={0}
										onChange={(event) =>
											update(field, Number(event.target.value))
										}
									/>
								</label>
							))}
						</fieldset>
					)}
					<fieldset className="generator-group">
						<legend>Target rates · 100 bp = 1%</legend>
						{form.stream === "operations" && (
							<>
								<label>
									<span>get hit ({form.getHitRate / 100}%)</span>
									<input
										aria-label="get hit"
										value={form.getHitRate}
										inputMode="numeric"
										max={10_000}
										min={0}
										onChange={(event) =>
											update("getHitRate", Number(event.target.value))
										}
									/>
								</label>
								<label>
									<span>remove hit ({form.removeHitRate / 100}%)</span>
									<input
										aria-label="remove hit"
										value={form.removeHitRate}
										inputMode="numeric"
										max={10_000}
										min={0}
										onChange={(event) =>
											update("removeHitRate", Number(event.target.value))
										}
									/>
								</label>
							</>
						)}
						<label>
							<span>insert overwrite ({form.overwriteRate / 100}%)</span>
							<input
								aria-label="insert overwrite"
								value={form.overwriteRate}
								inputMode="numeric"
								max={10_000}
								min={0}
								onChange={(event) =>
									update("overwriteRate", Number(event.target.value))
								}
							/>
						</label>
					</fieldset>
					<div className="dialog-actions">
						<Dialog.Close asChild>
							<button type="button" className="quiet-button">
								Cancel
							</button>
						</Dialog.Close>
						<button
							type="button"
							className="primary-button"
							onClick={onGenerate}
						>
							Generate
						</button>
					</div>
				</Dialog.Content>
			</Dialog.Portal>
		</Dialog.Root>
	);
}
