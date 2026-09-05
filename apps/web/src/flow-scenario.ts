import type { FlowWorkbenchProblemKind } from "./flow-workbench-problem";

export function defaultFlowScenario(
	problem: FlowWorkbenchProblemKind = "max-flow",
): string {
	const isMaxFlow = problem === "max-flow";
	return JSON.stringify(
		{
			schema_version: 1,
			scenario_encoding_revision: "rfc8785-jcs/1",
			plugin: "flow",
			reproducibility: {
				declared: {
					algorithm_revision: "flow-algorithms/8",
					rng_version: 1,
					plugin_result_revision: "flow-result/9",
					metrics_catalog_revision: "flow-metrics/6",
					trace_revision: "flow-trace/9",
					projection_revision: "flow-projection/6",
					layout_revision: "flow-layout/1",
					frame_encoding_revision: "flow-scene/9",
				},
			},
			payload: {
				model: isMaxFlow
					? { kind: "max-flow", source: "s", sink: "t" }
					: {
							kind: "fixed-flow-min-cost",
							source: "s",
							sink: "t",
							required_flow: "10",
						},
				graph: {
					nodes: [
						{ id: "s", supply: "0", position: { x: "90", y: "270" } },
						{ id: "a", supply: "0", position: { x: "290", y: "130" } },
						{ id: "b", supply: "0", position: { x: "290", y: "390" } },
						{ id: "c", supply: "0", position: { x: "560", y: "130" } },
						{ id: "d", supply: "0", position: { x: "560", y: "390" } },
						{ id: "t", supply: "0", position: { x: "810", y: "270" } },
					],
					edges: [
						{
							id: "sa",
							from: "s",
							to: "a",
							lower: "0",
							capacity: "12",
							cost: isMaxFlow ? "0" : "2",
						},
						{
							id: "sb",
							from: "s",
							to: "b",
							lower: "0",
							capacity: "8",
							cost: isMaxFlow ? "0" : "-1",
						},
						{
							id: "ac",
							from: "a",
							to: "c",
							lower: "0",
							capacity: "9",
							cost: isMaxFlow ? "0" : "1",
						},
						{
							id: "ad",
							from: "a",
							to: "d",
							lower: "0",
							capacity: "4",
							cost: isMaxFlow ? "0" : "-2",
						},
						{
							id: "bc",
							from: "b",
							to: "c",
							lower: "0",
							capacity: "3",
							cost: isMaxFlow ? "0" : "3",
						},
						{
							id: "bd",
							from: "b",
							to: "d",
							lower: "0",
							capacity: "7",
							cost: "0",
						},
						{
							id: "ct",
							from: "c",
							to: "t",
							lower: "0",
							capacity: "10",
							cost: isMaxFlow ? "0" : "2",
						},
						{
							id: "dt",
							from: "d",
							to: "t",
							lower: "0",
							capacity: "11",
							cost: isMaxFlow ? "0" : "-1",
						},
					],
				},
				algorithm: {
					id: isMaxFlow ? "edmonds-karp" : "successive-shortest-path",
					config: {},
				},
				run_profile: "trace",
				trace_granularity: "operation",
				algorithm_seed: "0",
			},
		},
		null,
		2,
	);
}
