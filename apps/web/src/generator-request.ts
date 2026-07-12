import type { GeneratorForm } from "./GeneratorDialog";

export function generatorRequestSpec(generator: GeneratorForm): string {
	const common = {
		seed: generator.seed,
		count: generator.count,
		key_min: generator.keyMin,
		key_max: generator.keyMax,
		distribution: { kind: generator.distribution },
		value_prefix: generator.valuePrefix,
		value_max_scalar_values: generator.valueMaxScalarValues,
	};
	const spec =
		generator.stream === "initial"
			? { ...common, overwrite_rate_bps: generator.overwriteRate }
			: {
					...common,
					weights: {
						insert: generator.insertWeight,
						remove: generator.removeWeight,
						get: generator.getWeight,
						lower_bound: generator.lowerBoundWeight,
					},
					get_hit_rate_bps: generator.getHitRate,
					remove_hit_rate_bps: generator.removeHitRate,
					insert_overwrite_rate_bps: generator.overwriteRate,
				};
	return JSON.stringify(spec);
}
