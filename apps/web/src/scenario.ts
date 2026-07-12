export const ALGORITHMS = [
	["avl", "AVL"],
	["wbt", "WBT"],
	["aa", "AA"],
	["llrb", "LLRB"],
	["treap", "Treap"],
	["zip", "Zip tree"],
	["splay", "Splay"],
	["scapegoat", "Scapegoat"],
	["skip-list", "Skip list"],
	["b-tree", "B-tree"],
	["veb", "vEB"],
	["x-fast", "X-fast trie"],
	["y-fast", "Y-fast trie"],
] as const;

export type AlgorithmId = (typeof ALGORITHMS)[number][0];

export function defaultAlgorithmConfig(
	id: AlgorithmId,
): Record<string, unknown> {
	switch (id) {
		case "scapegoat":
			return { alpha_numerator: 2, alpha_denominator: 3 };
		case "skip-list":
			return { promotion: "1/2", max_level: 16 };
		case "b-tree":
			return { min_degree: 3 };
		case "veb":
		case "x-fast":
		case "y-fast":
			return { word_bits: 16 };
		default:
			return {};
	}
}

export function defaultScenario(id: AlgorithmId = "avl"): string {
	return JSON.stringify(
		{
			schema_version: 1,
			scenario_encoding_revision: "rfc8785-jcs/1",
			plugin: "ordered-map",
			reproducibility: {
				declared: {
					algorithm_revision: "ordered-map/1",
					rng_version: 1,
					plugin_result_revision: "ordered-map-result/1",
					metrics_catalog_revision: "ordered-map-metrics/1",
					trace_revision: "ordered-map-trace/3",
					projection_revision: "ordered-map-projection/2",
					layout_revision: "ordered-map-layout/1",
					frame_encoding_revision: "scene-frame/5",
				},
			},
			payload: {
				algorithm: { id, config: defaultAlgorithmConfig(id) },
				algorithm_seed: "42",
				initial: {
					entries: [
						{ key: "8", value: "root" },
						{ key: "3", value: "left" },
						{ key: "12", value: "right" },
					],
					show_build: false,
				},
				operations: {
					items: [
						{ op: "insert", key: "6", value: "new" },
						{ op: "get", key: "12" },
						{ op: "lower_bound", key: "7" },
						{ op: "remove", key: "3" },
					],
				},
			},
		},
		null,
		2,
	);
}
