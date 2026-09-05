use std::cell::Cell;

use super::*;

#[path = "algorithm_conformance_tests.rs"]
mod algorithm_conformance;

fn scenario(show_build: bool) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "ordered-map",
        "reproducibility": { "declared": {
            "algorithm_revision": "ordered-map/1",
            "rng_version": 1,
            "plugin_result_revision": "ordered-map-result/1",
            "metrics_catalog_revision": "ordered-map-metrics/1",
            "trace_revision": "ordered-map-trace/3",
            "projection_revision": "ordered-map-projection/2",
            "layout_revision": "ordered-map-layout/1",
            "frame_encoding_revision": "scene-frame/5"
        }},
        "payload": {
            "algorithm": { "id": "avl", "config": {} },
            "algorithm_seed": "0",
            "initial": {
                "entries": [
                    { "key": "8", "value": "root" },
                    { "key": "3", "value": "left" }
                ],
                "show_build": show_build
            },
            "operations": { "items": [
                { "op": "insert", "key": "6", "value": "new" },
                { "op": "lower_bound", "key": "4" }
            ] }
        }
    })
    .to_string()
}

fn flow_scenario() -> String {
    flow_scenario_with_algorithm("edmonds-karp")
}

fn flow_scenario_with_algorithm(algorithm: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": { "kind": "max-flow", "source": "s", "sink": "t" },
            "graph": {
                "nodes": [{ "id": "s" }, { "id": "t" }],
                "edges": [{
                    "id": "st", "from": "s", "to": "t", "capacity": "9"
                }]
            },
            "algorithm": { "id": algorithm, "config": {} },
            "run_profile": "trace",
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn generator_fixture_scenario(
    fixture: &flow::GeneratorAlgorithmFixtureV1,
    preset: &flow::GeneratorPresetV1,
) -> String {
    let candidate = serde_json::to_value(
        generate_flow_graph_candidate(
            &serde_json::to_string(&preset.spec).expect("preset spec serializes"),
        )
        .unwrap_or_else(|error| {
            panic!(
                "{} {:?} preset must generate: {error}",
                fixture.family_id, preset.purpose
            )
        }),
    )
    .expect("generated candidate serializes");
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": candidate["suggested_model"].clone(),
            "graph": candidate["graph"].clone(),
            "algorithm": {
                "id": fixture.default_algorithm_id,
                "config": {}
            },
            "run_profile": match preset.recommended_run_profile {
                flow::GeneratorPresetRunProfileV1::Trace => "trace",
                flow::GeneratorPresetRunProfileV1::Fast => "fast",
            },
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn parametric_scenario(algorithm: &str, run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": {
                "kind": "parametric-max-flow",
                "source": "s",
                "sink": "t",
                "parameter": {
                    "minimum": { "numerator": "0", "denominator": "1" },
                    "maximum": { "numerator": "4", "denominator": "1" }
                },
                "capacity_slopes": [{ "edge_id": "sa", "slope": "2" }]
            },
            "graph": {
                "nodes": [{ "id": "a" }, { "id": "s" }, { "id": "t" }],
                "edges": [
                    { "id": "at", "from": "a", "to": "t", "capacity": "5" },
                    { "id": "sa", "from": "s", "to": "a", "capacity": "1" }
                ]
            },
            "algorithm": { "id": algorithm, "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn ibfs_scenario(run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": { "kind": "max-flow", "source": "s", "sink": "t" },
            "graph": {
                "nodes": [
                    { "id": "s" }, { "id": "a" }, { "id": "b" },
                    { "id": "c" }, { "id": "d" }, { "id": "t" }
                ],
                "edges": [
                    { "id": "sa", "from": "s", "to": "a", "capacity": "2" },
                    { "id": "sb", "from": "s", "to": "b", "capacity": "2" },
                    { "id": "ac", "from": "a", "to": "c", "capacity": "1" },
                    { "id": "bc", "from": "b", "to": "c", "capacity": "2" },
                    { "id": "cd", "from": "c", "to": "d", "capacity": "2" },
                    { "id": "dt", "from": "d", "to": "t", "capacity": "2" }
                ]
            },
            "algorithm": { "id": "ibfs", "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn eibfs_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&ibfs_scenario(run_profile)).expect("IBFS scenario JSON");
    scenario["payload"]["algorithm"] = serde_json::json!({ "id": "eibfs", "config": {} });
    scenario.to_string()
}

fn dynamic_eibfs_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&eibfs_scenario(run_profile)).expect("EIBFS scenario JSON");
    scenario["payload"]["algorithm"] = serde_json::json!({ "id": "dynamic-eibfs", "config": {} });
    scenario["payload"]["updates"] = serde_json::json!([
        { "kind": "set-capacity", "edge": "dt", "capacity": "4" },
        { "kind": "set-capacity", "edge": "cd", "capacity": "4" },
        { "kind": "set-capacity", "edge": "sa", "capacity": "0" }
    ]);
    scenario.to_string()
}

fn ibfs_sink_orphan_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&ibfs_scenario("trace")).expect("IBFS scenario JSON");
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [
            { "id": "s" }, { "id": "a" }, { "id": "b" }, { "id": "c" },
            { "id": "d" }, { "id": "e" }, { "id": "t" }
        ],
        "edges": [
            { "id": "at", "from": "a", "to": "t", "capacity": "2" },
            { "id": "bt", "from": "b", "to": "t", "capacity": "2" },
            { "id": "ca", "from": "c", "to": "a", "capacity": "1" },
            { "id": "cb", "from": "c", "to": "b", "capacity": "2" },
            { "id": "dc", "from": "d", "to": "c", "capacity": "2" },
            { "id": "ed", "from": "e", "to": "d", "capacity": "2" },
            { "id": "se", "from": "s", "to": "e", "capacity": "2" }
        ]
    });
    scenario.to_string()
}

fn ibfs_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&ibfs_scenario("trace")).expect("IBFS scenario JSON");
    let nodes = std::iter::once(serde_json::json!({ "id": "s" }))
        .chain((0..255).map(|index| serde_json::json!({ "id": format!("n{index:03}") })))
        .chain(std::iter::once(serde_json::json!({ "id": "t" })))
        .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": []
    });
    scenario.to_string()
}

fn eibfs_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&eibfs_scenario("trace")).expect("EIBFS scenario JSON");
    let nodes = std::iter::once(serde_json::json!({ "id": "s" }))
        .chain((0..255).map(|index| serde_json::json!({ "id": format!("n{index:03}") })))
        .chain(std::iter::once(serde_json::json!({ "id": "t" })))
        .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": []
    });
    scenario.to_string()
}

fn planar_scenario(algorithm: &str, run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": {
                "kind": "planar-max-flow",
                "source": "a",
                "sink": "c",
                "embedding": {
                    "rotations": [
                        { "node_id": "a", "darts": [
                            { "edge_id": "ab", "direction": "forward" },
                            { "edge_id": "ac", "direction": "forward" }
                        ]},
                        { "node_id": "b", "darts": [
                            { "edge_id": "ab", "direction": "reverse" },
                            { "edge_id": "bc", "direction": "forward" }
                        ]},
                        { "node_id": "c", "darts": [
                            { "edge_id": "bc", "direction": "reverse" },
                            { "edge_id": "ac", "direction": "reverse" }
                        ]}
                    ],
                    "outer_face": { "edge_id": "ab", "direction": "reverse" },
                    "terminal_corners": {
                        "source": { "edge_id": "ac", "direction": "forward" },
                        "sink": { "edge_id": "bc", "direction": "reverse" }
                    }
                }
            },
            "graph": {
                "nodes": [{ "id": "a" }, { "id": "b" }, { "id": "c" }],
                "edges": [
                    { "id": "ab", "from": "a", "to": "b", "capacity": "5" },
                    { "id": "ac", "from": "a", "to": "c", "capacity": "2" },
                    { "id": "bc", "from": "b", "to": "c", "capacity": "3" }
                ]
            },
            "algorithm": { "id": algorithm, "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn hopcroft_karp_scenario(run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": {
                "kind": "bipartite-matching",
                "left": ["l0", "l1"],
                "right": ["r0", "r1"],
                "flow_adapter": { "source": "s", "sink": "t" }
            },
            "graph": {
                "nodes": [
                    { "id": "l0" }, { "id": "l1" }, { "id": "r0" },
                    { "id": "r1" }, { "id": "s" }, { "id": "t" }
                ],
                "edges": [
                    { "id": "a0", "from": "s", "to": "l0", "capacity": "1" },
                    { "id": "a1", "from": "s", "to": "l1", "capacity": "1" },
                    { "id": "b00", "from": "l0", "to": "r0", "capacity": "1" },
                    { "id": "b01", "from": "l0", "to": "r1", "capacity": "1" },
                    { "id": "b10", "from": "l1", "to": "r0", "capacity": "1" },
                    { "id": "c0", "from": "r0", "to": "t", "capacity": "1" },
                    { "id": "c1", "from": "r1", "to": "t", "capacity": "1" }
                ]
            },
            "algorithm": { "id": "hopcroft-karp", "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn hungarian_scenario(run_profile: &str, infeasible: bool) -> String {
    let (agents, tasks, nodes, edges) = if infeasible {
        (
            serde_json::json!(["a0", "a1", "a2"]),
            serde_json::json!(["t0", "t1", "t2"]),
            serde_json::json!([
                { "id": "a0" }, { "id": "a1" }, { "id": "a2" },
                { "id": "t0" }, { "id": "t1" }, { "id": "t2" }
            ]),
            serde_json::json!([
                { "id": "e00", "from": "a0", "to": "t0", "capacity": "1", "cost": "1" },
                { "id": "e10", "from": "a1", "to": "t0", "capacity": "1", "cost": "2" },
                { "id": "e21", "from": "a2", "to": "t1", "capacity": "1", "cost": "0" },
                { "id": "e22", "from": "a2", "to": "t2", "capacity": "1", "cost": "0" }
            ]),
        )
    } else {
        (
            serde_json::json!(["a0", "a1"]),
            serde_json::json!(["t0", "t1", "t2"]),
            serde_json::json!([
                { "id": "a0" }, { "id": "a1" },
                { "id": "t0" }, { "id": "t1" }, { "id": "t2" }
            ]),
            serde_json::json!([
                { "id": "e00", "from": "a0", "to": "t0", "capacity": "1", "cost": "4" },
                { "id": "e01", "from": "a0", "to": "t1", "capacity": "1", "cost": "1" },
                { "id": "e10", "from": "a1", "to": "t0", "capacity": "1", "cost": "2" },
                { "id": "e11", "from": "a1", "to": "t1", "capacity": "1", "cost": "3" },
                { "id": "e12", "from": "a1", "to": "t2", "capacity": "1", "cost": "0" }
            ]),
        )
    };
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": {
                "kind": "assignment",
                "agents": agents,
                "tasks": tasks,
                "objective": "minimize"
            },
            "graph": { "nodes": nodes, "edges": edges },
            "algorithm": { "id": "hungarian", "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn auction_scenario(run_profile: &str, infeasible: bool) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&hungarian_scenario(run_profile, infeasible))
            .expect("Hungarian scenario helper is JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("auction");
    if infeasible {
        scenario["payload"]["model"]["agents"] = serde_json::json!(["a0", "a1", "a2"]);
        scenario["payload"]["model"]["tasks"] = serde_json::json!(["t0", "t1", "t2", "t3"]);
        scenario["payload"]["graph"] = serde_json::json!({
            "nodes": [
                { "id": "a0" }, { "id": "a1" }, { "id": "a2" },
                { "id": "t0" }, { "id": "t1" }, { "id": "t2" }, { "id": "t3" }
            ],
            "edges": [
                { "id": "e00", "from": "a0", "to": "t0", "capacity": "1", "cost": "1" },
                { "id": "e10", "from": "a1", "to": "t0", "capacity": "1", "cost": "2" },
                { "id": "e21", "from": "a2", "to": "t1", "capacity": "1", "cost": "0" },
                { "id": "e22", "from": "a2", "to": "t2", "capacity": "1", "cost": "0" },
                { "id": "e23", "from": "a2", "to": "t3", "capacity": "1", "cost": "0" }
            ]
        });
    }
    scenario.to_string()
}

fn auction_eviction_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&auction_scenario(run_profile, false)).expect("Auction JSON");
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "assignment",
        "agents": ["a0", "a1", "a2"],
        "tasks": ["t0", "t1", "t2"],
        "objective": "maximize"
    });
    let nodes = ["a0", "a1", "a2", "t0", "t1", "t2"]
        .into_iter()
        .map(|id| serde_json::json!({ "id": id }))
        .collect::<Vec<_>>();
    let edges = (0..3)
        .flat_map(|agent| {
            (0..3).map(move |task| {
                serde_json::json!({
                    "id": format!("e{agent}{task}"),
                    "from": format!("a{agent}"),
                    "to": format!("t{task}"),
                    "capacity": "1",
                    "cost": if task < 2 { "20" } else { "0" }
                })
            })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn transportation_scenario(algorithm: &str, run_profile: &str, infeasible: bool) -> String {
    let (nodes, edges) = if infeasible {
        (
            serde_json::json!([
                { "id": "d00", "supply": "-2" },
                { "id": "d01", "supply": "-4" },
                { "id": "o00", "supply": "3" },
                { "id": "o01", "supply": "3" }
            ]),
            serde_json::json!([
                { "id": "e000", "from": "o00", "to": "d00", "capacity": "2", "cost": "1" },
                { "id": "e001", "from": "o01", "to": "d00", "capacity": "2", "cost": "2" },
                { "id": "e002", "from": "o01", "to": "d01", "capacity": "3", "cost": "3" }
            ]),
        )
    } else {
        (
            serde_json::json!([
                { "id": "d00", "supply": "-2" },
                { "id": "d01", "supply": "-3" },
                { "id": "d02", "supply": "-2" },
                { "id": "o00", "supply": "4" },
                { "id": "o01", "supply": "3" }
            ]),
            serde_json::json!([
                { "id": "e000", "from": "o00", "to": "d00", "capacity": "2", "cost": "8" },
                { "id": "e001", "from": "o00", "to": "d01", "capacity": "3", "cost": "6" },
                { "id": "e002", "from": "o00", "to": "d02", "capacity": "2", "cost": "10" },
                { "id": "e003", "from": "o01", "to": "d00", "capacity": "2", "cost": "9" },
                { "id": "e004", "from": "o01", "to": "d01", "capacity": "3", "cost": "7" },
                { "id": "e005", "from": "o01", "to": "d02", "capacity": "2", "cost": "4" }
            ]),
        )
    };
    serde_json::json!({
            "schema_version": 1,
            "scenario_encoding_revision": "rfc8785-jcs/1",
            "plugin": "flow",
            "reproducibility": { "declared": {
                "algorithm_revision": "flow-algorithms/8",
                "rng_version": 1,
                "plugin_result_revision": "flow-result/9",
                "metrics_catalog_revision": "flow-metrics/6",
                "trace_revision": "flow-trace/9",
                "projection_revision": "flow-projection/6",
                "layout_revision": "flow-layout/1",
                "frame_encoding_revision": "flow-scene/9"
            }},
            "payload": {
                "model": {
                    "kind": "transportation",
                    "origins": ["o00", "o01"],
                    "destinations": if infeasible { serde_json::json!(["d00", "d01"]) } else { serde_json::json!(["d00", "d01", "d02"]) }
                },
                "graph": { "nodes": nodes, "edges": edges },
                "algorithm": { "id": algorithm, "config": {} },
                "run_profile": run_profile,
                "trace_granularity": "operation",
                "algorithm_seed": "0"
            }
        })
        .to_string()
}

fn push_relabel_scenario(algorithm: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": { "kind": "max-flow", "source": "s", "sink": "t" },
            "graph": {
                "nodes": [{ "id": "s" }, { "id": "a" }, { "id": "b" }, { "id": "t" }],
                "edges": [
                    { "id": "sa", "from": "s", "to": "a", "capacity": "5" },
                    { "id": "sb", "from": "s", "to": "b", "capacity": "4" },
                    { "id": "ab", "from": "a", "to": "b", "capacity": "2" },
                    { "id": "at", "from": "a", "to": "t", "capacity": "3" },
                    { "id": "bt", "from": "b", "to": "t", "capacity": "6" }
                ]
            },
            "algorithm": { "id": algorithm, "config": {} },
            "run_profile": "trace",
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn warm_start_push_relabel_scenario(run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": { "kind": "max-flow", "source": "s", "sink": "t" },
            "graph": {
                "nodes": [{ "id": "s" }, { "id": "a" }, { "id": "b" }, { "id": "t" }],
                "edges": [
                    { "id": "sa", "from": "s", "to": "a", "capacity": "5", "initial_flow": "5" },
                    { "id": "at", "from": "a", "to": "t", "capacity": "1", "initial_flow": "0" },
                    { "id": "sb", "from": "s", "to": "b", "capacity": "1", "initial_flow": "0" },
                    { "id": "bt", "from": "b", "to": "t", "capacity": "5", "initial_flow": "5" }
                ]
            },
            "algorithm": { "id": "warm-start-push-relabel", "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn potential_dijkstra_scenario() -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": {
                "kind": "fixed-flow-min-cost",
                "source": "s",
                "sink": "t",
                "required_flow": "4"
            },
            "graph": {
                "nodes": [
                    { "id": "a" }, { "id": "b" }, { "id": "s" }, { "id": "t" }
                ],
                "edges": [
                    { "id": "at", "from": "a", "to": "t", "capacity": "2", "cost": "3" },
                    { "id": "bt", "from": "b", "to": "t", "capacity": "2", "cost": "2" },
                    { "id": "sa", "from": "s", "to": "a", "capacity": "2", "cost": "-2" },
                    { "id": "sb", "from": "s", "to": "b", "capacity": "2", "cost": "1" },
                    { "id": "st", "from": "s", "to": "t", "capacity": "3", "cost": "7" }
                ]
            },
            "algorithm": { "id": "potential-dijkstra-ssp", "config": {} },
            "run_profile": "trace",
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn successive_shortest_augmenting_path_scenario() -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": { "kind": "min-cost-max-flow", "source": "s", "sink": "t" },
            "graph": {
                "nodes": [
                    { "id": "a" }, { "id": "b" }, { "id": "s" }, { "id": "t" }
                ],
                "edges": [
                    { "id": "at", "from": "a", "to": "t", "capacity": "2", "cost": "3" },
                    { "id": "bt", "from": "b", "to": "t", "capacity": "1", "cost": "2" },
                    { "id": "sa", "from": "s", "to": "a", "capacity": "2", "cost": "2" },
                    { "id": "sb", "from": "s", "to": "b", "capacity": "1", "cost": "0" }
                ]
            },
            "algorithm": { "id": "successive-shortest-augmenting-path", "config": {} },
            "run_profile": "trace",
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn primal_dual_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&potential_dijkstra_scenario()).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("primal-dual-mcf");
    scenario.to_string()
}

fn blocking_primal_dual_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&potential_dijkstra_scenario()).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("blocking-flow-primal-dual");
    scenario["payload"]["graph"]["edges"][3]["cost"] = serde_json::json!("-1");
    scenario.to_string()
}

fn capacity_scaling_scenario() -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": {
                "kind": "fixed-flow-min-cost",
                "source": "s",
                "sink": "t",
                "required_flow": "7"
            },
            "graph": {
                "nodes": [{ "id": "a" }, { "id": "s" }, { "id": "t" }],
                "edges": [
                    { "id": "at", "from": "a", "to": "t", "capacity": "4", "cost": "5" },
                    { "id": "direct", "from": "s", "to": "t", "capacity": "3", "cost": "0" },
                    { "id": "sa", "from": "s", "to": "a", "capacity": "4", "cost": "5" }
                ]
            },
            "algorithm": { "id": "capacity-scaling-mcf", "config": {} },
            "run_profile": "trace",
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn excess_scaling_scenario(run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": {
                "kind": "fixed-flow-min-cost",
                "source": "s",
                "sink": "t",
                "required_flow": "13"
            },
            "graph": {
                "nodes": [{ "id": "s" }, { "id": "t" }],
                "edges": [
                    { "id": "cheap", "from": "s", "to": "t", "capacity": "13", "cost": "1" },
                    { "id": "expensive", "from": "s", "to": "t", "capacity": "13", "cost": "3" }
                ]
            },
            "algorithm": { "id": "excess-scaling-mcf", "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn cost_scaling_scenario(algorithm: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&simple_cycle_canceling_scenario()).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!(algorithm);
    scenario.to_string()
}

fn arc_fixing_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&simple_cycle_canceling_scenario()).expect("scenario JSON");
    scenario["payload"]["model"] = serde_json::json!({ "kind": "circulation" });
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [{ "id": "v0" }, { "id": "v1" }, { "id": "v2" }],
        "edges": [
            { "id": "e0_1", "from": "v0", "to": "v1", "capacity": "4", "cost": "5" },
            { "id": "e0_2", "from": "v0", "to": "v2", "capacity": "1", "cost": "-3" },
            { "id": "e1_2", "from": "v1", "to": "v2", "capacity": "1", "cost": "-5" },
            { "id": "e2_0", "from": "v2", "to": "v0", "capacity": "2", "cost": "1" }
        ]
    });
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("arc-fixing");
    scenario["payload"]["run_profile"] = serde_json::json!(run_profile);
    scenario.to_string()
}

fn relaxation_scenario() -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": {
                "kind": "fixed-flow-min-cost",
                "source": "s",
                "sink": "t",
                "required_flow": "2"
            },
            "graph": {
                "nodes": [{ "id": "s" }, { "id": "t" }],
                "edges": [
                    { "id": "st", "from": "s", "to": "t", "capacity": "3", "cost": "5" }
                ]
            },
            "algorithm": { "id": "relaxation", "config": {} },
            "run_profile": "trace",
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn relaxation_trace_limit_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&relaxation_scenario()).expect("scenario JSON");
    scenario["payload"]["model"]["required_flow"] = serde_json::json!("160");
    scenario["payload"]["graph"]["edges"] = serde_json::Value::Array(
        (1_i64..=160)
            .map(|cost| {
                serde_json::json!({
                    "id": format!("edge-{cost:03}"),
                    "from": "s",
                    "to": "t",
                    "capacity": "1",
                    "cost": cost.to_string()
                })
            })
            .collect(),
    );
    scenario.to_string()
}

fn epsilon_relaxation_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&relaxation_scenario()).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("epsilon-relaxation");
    scenario["payload"]["run_profile"] = serde_json::json!(run_profile);
    scenario.to_string()
}

fn epsilon_relaxation_trace_limit_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&relaxation_trace_limit_scenario()).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("epsilon-relaxation");
    scenario.to_string()
}

fn epsilon_relaxation_work_limit_scenario() -> String {
    let edge_count = 1_200_i64;
    let mut scenario: serde_json::Value =
        serde_json::from_str(&epsilon_relaxation_scenario("fast")).expect("scenario JSON");
    scenario["payload"]["model"]["required_flow"] = serde_json::json!(edge_count.to_string());
    scenario["payload"]["graph"]["edges"] = serde_json::Value::Array(
        (1_i64..=edge_count)
            .map(|cost| {
                serde_json::json!({
                    "id": format!("edge-{cost:04}"),
                    "from": "s",
                    "to": "t",
                    "capacity": "1",
                    "cost": cost.to_string()
                })
            })
            .collect(),
    );
    scenario.to_string()
}

fn prediction_assisted_epsilon_scenario(run_profile: &str) -> String {
    serde_json::json!({
            "schema_version": 1,
            "scenario_encoding_revision": "rfc8785-jcs/1",
            "plugin": "flow",
            "reproducibility": { "declared": {
                "algorithm_revision": "flow-algorithms/8",
                "rng_version": 1,
                "plugin_result_revision": "flow-result/9",
                "metrics_catalog_revision": "flow-metrics/6",
                "trace_revision": "flow-trace/9",
                "projection_revision": "flow-projection/6",
                "layout_revision": "flow-layout/1",
                "frame_encoding_revision": "flow-scene/9"
            }},
            "payload": {
                "model": {
                    "kind": "fixed-flow-min-cost",
                    "source": "s",
                    "sink": "t",
                    "required_flow": "3"
                },
                "graph": {
                    "nodes": [{ "id": "s" }, { "id": "a" }, { "id": "t" }],
                    "edges": [
                        { "id": "at", "from": "a", "to": "t", "capacity": "3", "cost": "-1" },
                        { "id": "direct", "from": "s", "to": "t", "capacity": "3", "cost": "8" },
                        { "id": "sa", "from": "s", "to": "a", "capacity": "3", "cost": "2" }
                    ]
                },
                "algorithm": {
                    "id": "prediction-assisted-epsilon-relaxation",
                    "config": {
                        "predicted_potentials": { "a": "-100", "s": "200", "t": "170141183460469231731687303715884105727" },
                        "scaling_parameter": 2
                    }
                },
                "run_profile": run_profile,
                "trace_granularity": "operation",
                "algorithm_seed": "0"
            }
        })
        .to_string()
}

fn tardos_framework_scenario(run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": { "kind": "transshipment" },
            "graph": {
                "nodes": [
                    { "id": "s", "supply": "2" },
                    { "id": "a" },
                    { "id": "t", "supply": "-2" }
                ],
                "edges": [
                    { "id": "cheap-1", "from": "s", "to": "a", "capacity": "2", "cost": "1" },
                    { "id": "cheap-2", "from": "a", "to": "t", "capacity": "2", "cost": "1" },
                    { "id": "expensive", "from": "s", "to": "t", "capacity": "2", "cost": "20" }
                ]
            },
            "algorithm": {
                "id": "tardos-framework",
                "config": { "potentials": { "a": "0", "s": "0", "t": "0" } }
            },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn simple_cycle_canceling_scenario() -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": {
                "kind": "fixed-flow-min-cost",
                "source": "s",
                "sink": "t",
                "required_flow": "1"
            },
            "graph": {
                "nodes": [
                    { "id": "s" }, { "id": "t" }, { "id": "x" }, { "id": "y" }
                ],
                "edges": [
                    { "id": "path", "from": "s", "to": "t", "capacity": "1", "cost": "2" },
                    { "id": "xy", "from": "x", "to": "y", "capacity": "3", "cost": "-4" },
                    { "id": "yx", "from": "y", "to": "x", "capacity": "3", "cost": "1" }
                ]
            },
            "algorithm": { "id": "simple-cycle-canceling", "config": {} },
            "run_profile": "trace",
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn minimum_mean_cycle_canceling_scenario() -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": {
                "kind": "fixed-flow-min-cost",
                "source": "s",
                "sink": "t",
                "required_flow": "1"
            },
            "graph": {
                "nodes": [
                    { "id": "a" }, { "id": "b" }, { "id": "s" },
                    { "id": "t" }, { "id": "x" }
                ],
                "edges": [
                    { "id": "ab", "from": "a", "to": "b", "capacity": "1", "cost": "-3" },
                    { "id": "ba", "from": "b", "to": "a", "capacity": "1", "cost": "-2" },
                    { "id": "path", "from": "s", "to": "t", "capacity": "1", "cost": "2" },
                    { "id": "loop", "from": "x", "to": "x", "capacity": "1", "cost": "-3" }
                ]
            },
            "algorithm": { "id": "minimum-mean-cycle-canceling", "config": {} },
            "run_profile": "trace",
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn cancel_tighten_scenario() -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": {
                "kind": "fixed-flow-min-cost",
                "source": "s",
                "sink": "t",
                "required_flow": "1"
            },
            "graph": {
                "nodes": [
                    { "id": "a" }, { "id": "b" }, { "id": "c" },
                    { "id": "s" }, { "id": "t" }
                ],
                "edges": [
                    { "id": "ab", "from": "a", "to": "b", "capacity": "3", "cost": "-4" },
                    { "id": "bc", "from": "b", "to": "c", "capacity": "3", "cost": "1" },
                    { "id": "ca", "from": "c", "to": "a", "capacity": "3", "cost": "1" },
                    { "id": "path", "from": "s", "to": "t", "capacity": "1", "cost": "2" }
                ]
            },
            "algorithm": { "id": "cancel-and-tighten", "config": {} },
            "run_profile": "trace",
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn relaxed_mndc_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&cancel_tighten_scenario()).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("relaxed-most-negative-cycle");
    scenario["payload"]["run_profile"] = serde_json::json!(run_profile);
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [
            { "id": "a" }, { "id": "b" }, { "id": "c" },
            { "id": "d" }, { "id": "s" }, { "id": "t" }
        ],
        "edges": [
            { "id": "ab", "from": "a", "to": "b", "capacity": "2", "cost": "-4" },
            { "id": "ba", "from": "b", "to": "a", "capacity": "2", "cost": "1" },
            { "id": "cd", "from": "c", "to": "d", "capacity": "3", "cost": "-3" },
            { "id": "dc", "from": "d", "to": "c", "capacity": "3", "cost": "0" },
            { "id": "path", "from": "s", "to": "t", "capacity": "1", "cost": "0" }
        ]
    });
    scenario.to_string()
}

fn relaxed_mndc_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&relaxed_mndc_scenario("trace")).expect("scenario JSON");
    let nodes = std::iter::once(serde_json::json!({ "id": "s" }))
        .chain((0..63).map(|index| serde_json::json!({ "id": format!("n{index:02}") })))
        .chain(std::iter::once(serde_json::json!({ "id": "t" })))
        .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": [
            { "id": "path", "from": "s", "to": "t", "capacity": "1", "cost": "0" }
        ]
    });
    scenario.to_string()
}

fn enhanced_capacity_scaling_scenario(run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": { "kind": "transshipment" },
            "graph": {
                "nodes": [
                    { "id": "a", "supply": "5" },
                    { "id": "b", "supply": "-4" },
                    { "id": "c", "supply": "-1" }
                ],
                "edges": [
                    { "id": "ab", "from": "a", "to": "b", "capacity": "20", "cost": "0" },
                    { "id": "ac", "from": "a", "to": "c", "capacity": "20", "cost": "4" },
                    { "id": "ba", "from": "b", "to": "a", "capacity": "20", "cost": "4" },
                    { "id": "bc", "from": "b", "to": "c", "capacity": "20", "cost": "0" },
                    { "id": "ca", "from": "c", "to": "a", "capacity": "20", "cost": "4" },
                    { "id": "cb", "from": "c", "to": "b", "capacity": "20", "cost": "4" }
                ]
            },
            "algorithm": { "id": "enhanced-capacity-scaling", "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn orlin_mcf_scenario(run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": { "kind": "transshipment" },
            "graph": {
                "nodes": [
                    { "id": "s", "supply": "3" },
                    { "id": "m", "supply": "0" },
                    { "id": "t", "supply": "-3" }
                ],
                "edges": [
                    { "id": "a", "from": "s", "to": "m", "capacity": "3", "cost": "1" },
                    { "id": "b", "from": "m", "to": "t", "capacity": "3", "cost": "2" },
                    { "id": "expensive", "from": "s", "to": "t", "capacity": "3", "cost": "9" }
                ]
            },
            "algorithm": { "id": "orlin-mcf", "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn orlin_mcf_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&orlin_mcf_scenario("trace")).expect("scenario JSON");
    let nodes = (0..=flow::ORLIN_MCF_MAX_NODES)
            .map(|index| {
                serde_json::json!({
                    "id": format!("n{index:02}"),
                    "supply": if index == 0 { "1" } else if index == flow::ORLIN_MCF_MAX_NODES { "-1" } else { "0" }
                })
            })
            .collect::<Vec<_>>();
    let edges = (0..flow::ORLIN_MCF_MAX_NODES)
        .map(|index| {
            serde_json::json!({
                "id": format!("e{index:02}"),
                "from": format!("n{index:02}"),
                "to": format!("n{:02}", index + 1),
                "capacity": "1",
                "cost": "0"
            })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": edges
    });
    scenario.to_string()
}

fn primal_dual_ipm_mcf_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&orlin_mcf_scenario(run_profile)).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("primal-dual-interior-point-mcf");
    scenario.to_string()
}

fn primal_dual_ipm_mcf_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&primal_dual_ipm_mcf_scenario("trace")).expect("scenario JSON");
    let nodes = (0..=flow::PRIMAL_DUAL_IPM_MCF_MAX_NODES)
            .map(|index| {
                serde_json::json!({
                    "id": format!("n{index:02}"),
                    "supply": if index == 0 { "1" } else if index == flow::PRIMAL_DUAL_IPM_MCF_MAX_NODES { "-1" } else { "0" }
                })
            })
            .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": [{
            "id": "direct",
            "from": "n00",
            "to": format!("n{:02}", flow::PRIMAL_DUAL_IPM_MCF_MAX_NODES),
            "capacity": "1",
            "cost": "0"
        }]
    });
    scenario.to_string()
}

fn electrical_ipm_mcf_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&orlin_mcf_scenario(run_profile)).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] =
        serde_json::json!("electrical-flow-interior-point-mcf");
    scenario.to_string()
}

fn electrical_ipm_mcf_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&electrical_ipm_mcf_scenario("trace")).expect("scenario JSON");
    let nodes = (0..=flow::ELECTRICAL_IPM_MCF_MAX_NODES)
            .map(|index| {
                serde_json::json!({
                    "id": format!("n{index:02}"),
                    "supply": if index == 0 { "1" } else if index == flow::ELECTRICAL_IPM_MCF_MAX_NODES { "-1" } else { "0" }
                })
            })
            .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": [{
            "id": "direct",
            "from": "n00",
            "to": format!("n{:02}", flow::ELECTRICAL_IPM_MCF_MAX_NODES),
            "capacity": "1",
            "cost": "0"
        }]
    });
    scenario.to_string()
}

fn minimum_ratio_cycle_mcf_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&orlin_mcf_scenario(run_profile)).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("minimum-ratio-cycle-mcf");
    scenario.to_string()
}

fn randomized_almost_linear_mcf_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&orlin_mcf_scenario(run_profile)).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] =
        serde_json::json!("randomized-almost-linear-mcf-oracle-demonstrator");
    scenario.to_string()
}

fn deterministic_almost_linear_mcf_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&orlin_mcf_scenario(run_profile)).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("deterministic-almost-linear-mcf");
    scenario["payload"]["graph"]["edges"][1]["cost"] = serde_json::json!("1");
    scenario["payload"]["graph"]["edges"][2]["cost"] = serde_json::json!("5");
    scenario.to_string()
}

fn deterministic_almost_linear_mcf_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&deterministic_almost_linear_mcf_scenario("trace"))
            .expect("scenario JSON");
    let nodes = (0..=flow::FLOW_FRAMEWORK_MCF_MAX_NODES)
            .map(|index| {
                serde_json::json!({
                    "id": format!("n{index:02}"),
                    "supply": if index == 0 { "1" } else if index == flow::FLOW_FRAMEWORK_MCF_MAX_NODES { "-1" } else { "0" }
                })
            })
            .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": [{
            "id": "direct",
            "from": "n00",
            "to": format!("n{:02}", flow::FLOW_FRAMEWORK_MCF_MAX_NODES),
            "capacity": "2",
            "cost": "0"
        }]
    });
    scenario.to_string()
}

fn deterministic_almost_linear_mcf_self_loop_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&deterministic_almost_linear_mcf_scenario("trace"))
            .expect("scenario JSON");
    scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("edge array")
        .push(serde_json::json!({
            "id": "loop",
            "from": "m",
            "to": "m",
            "capacity": "1",
            "cost": "0"
        }));
    scenario.to_string()
}

fn deterministic_almost_linear_mcf_infeasible_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&deterministic_almost_linear_mcf_scenario("trace"))
            .expect("scenario JSON");
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [
            { "id": "s", "supply": "2" },
            { "id": "t", "supply": "-2" }
        ],
        "edges": [
            { "id": "direct", "from": "s", "to": "t", "capacity": "1", "cost": "0" }
        ]
    });
    scenario.to_string()
}

fn deterministic_almost_linear_mcf_saturated_cut_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&deterministic_almost_linear_mcf_scenario("trace"))
            .expect("scenario JSON");
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [
            { "id": "s", "supply": "3" },
            { "id": "a", "supply": "0" },
            { "id": "t", "supply": "-3" }
        ],
        "edges": [
            { "id": "sa", "from": "s", "to": "a", "lower": "0", "capacity": "3", "cost": "1" },
            { "id": "at", "from": "a", "to": "t", "lower": "0", "capacity": "3", "cost": "1" }
        ]
    });
    scenario.to_string()
}

fn minimum_ratio_cycle_mcf_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&minimum_ratio_cycle_mcf_scenario("trace")).expect("scenario JSON");
    let nodes = (0..=flow::MINIMUM_RATIO_CYCLE_MCF_MAX_NODES)
            .map(|index| {
                serde_json::json!({
                    "id": format!("n{index:02}"),
                    "supply": if index == 0 { "1" } else if index == flow::MINIMUM_RATIO_CYCLE_MCF_MAX_NODES { "-1" } else { "0" }
                })
            })
            .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": [{
            "id": "direct",
            "from": "n00",
            "to": format!("n{:02}", flow::MINIMUM_RATIO_CYCLE_MCF_MAX_NODES),
            "capacity": "1",
            "cost": "0"
        }]
    });
    scenario.to_string()
}

fn orlin_max_flow_scenario(run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": { "kind": "max-flow", "source": "s", "sink": "t" },
            "graph": {
                "nodes": [
                    { "id": "s" }, { "id": "a" }, { "id": "b" },
                    { "id": "c" }, { "id": "t" }
                ],
                "edges": [
                    { "id": "sa", "from": "s", "to": "a", "capacity": "10000", "cost": "0" },
                    { "id": "ab", "from": "a", "to": "b", "capacity": "1", "cost": "0" },
                    { "id": "ba", "from": "b", "to": "a", "capacity": "40000", "cost": "0" },
                    { "id": "bc", "from": "b", "to": "c", "capacity": "1", "cost": "0" },
                    { "id": "cb", "from": "c", "to": "b", "capacity": "40000", "cost": "0" },
                    { "id": "ct", "from": "c", "to": "t", "capacity": "10000", "cost": "0" },
                    { "id": "z0", "from": "s", "to": "s", "capacity": "0", "cost": "0" },
                    { "id": "z1", "from": "a", "to": "a", "capacity": "0", "cost": "0" },
                    { "id": "z2", "from": "b", "to": "b", "capacity": "0", "cost": "0" },
                    { "id": "z3", "from": "c", "to": "c", "capacity": "0", "cost": "0" },
                    { "id": "z4", "from": "t", "to": "t", "capacity": "0", "cost": "0" },
                    { "id": "z5", "from": "s", "to": "t", "capacity": "0", "cost": "0" }
                ]
            },
            "algorithm": { "id": "orlin-max-flow", "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn orlin_max_flow_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&orlin_max_flow_scenario("trace")).expect("scenario JSON");
    let nodes = (0..=flow::ORLIN_MAX_FLOW_MAX_NODES)
        .map(|index| serde_json::json!({ "id": format!("n{index:02}") }))
        .collect::<Vec<_>>();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "max-flow",
        "source": "n00",
        "sink": format!("n{:02}", flow::ORLIN_MAX_FLOW_MAX_NODES)
    });
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": [{
            "id": "direct",
            "from": "n00",
            "to": format!("n{:02}", flow::ORLIN_MAX_FLOW_MAX_NODES),
            "capacity": "1",
            "cost": "0"
        }]
    });
    scenario.to_string()
}

fn electrical_flow_scenario(run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": { "kind": "max-flow", "source": "s", "sink": "t" },
            "graph": {
                "nodes": [
                    { "id": "s" }, { "id": "a" },
                    { "id": "b" }, { "id": "t" }
                ],
                "edges": [
                    { "id": "sa", "from": "s", "to": "a", "capacity": "1", "cost": "0" },
                    { "id": "at", "from": "a", "to": "t", "capacity": "1", "cost": "0" },
                    { "id": "bs", "from": "b", "to": "s", "capacity": "2", "cost": "0" },
                    { "id": "bt", "from": "b", "to": "t", "capacity": "2", "cost": "0" }
                ]
            },
            "algorithm": { "id": "electrical-flow", "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn electrical_flow_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&electrical_flow_scenario("trace")).expect("scenario JSON");
    let nodes = (0..=flow::ELECTRICAL_FLOW_MAX_NODES)
        .map(|index| serde_json::json!({ "id": format!("n{index:02}") }))
        .collect::<Vec<_>>();
    let edges = (0..flow::ELECTRICAL_FLOW_MAX_NODES)
        .map(|index| {
            serde_json::json!({
                "id": format!("e{index:02}"),
                "from": format!("n{index:02}"),
                "to": format!("n{:02}", index + 1),
                "capacity": "1",
                "cost": "0"
            })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "max-flow",
        "source": "n00",
        "sink": format!("n{:02}", flow::ELECTRICAL_FLOW_MAX_NODES)
    });
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": edges
    });
    scenario.to_string()
}

fn augmenting_electrical_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&electrical_flow_scenario(run_profile)).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("augmenting-electrical-flow");
    scenario["payload"]["graph"]["edges"] = serde_json::json!([
        { "id": "sa", "from": "s", "to": "a", "capacity": "8", "cost": "0" },
        { "id": "at", "from": "a", "to": "t", "capacity": "8", "cost": "0" },
        { "id": "sb", "from": "s", "to": "b", "capacity": "1", "cost": "0" },
        { "id": "bt", "from": "b", "to": "t", "capacity": "1", "cost": "0" }
    ]);
    scenario.to_string()
}

fn augmenting_electrical_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&augmenting_electrical_scenario("trace")).expect("scenario JSON");
    let nodes = (0..=flow::AUGMENTING_ELECTRICAL_MAX_NODES)
        .map(|index| serde_json::json!({ "id": format!("n{index:02}") }))
        .collect::<Vec<_>>();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "max-flow",
        "source": "n00",
        "sink": format!("n{:02}", flow::AUGMENTING_ELECTRICAL_MAX_NODES)
    });
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": [{
            "id": "direct",
            "from": "n00",
            "to": format!("n{:02}", flow::AUGMENTING_ELECTRICAL_MAX_NODES),
            "capacity": "1",
            "cost": "0"
        }]
    });
    scenario.to_string()
}

fn interior_point_max_flow_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&electrical_flow_scenario(run_profile)).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("interior-point-max-flow");
    scenario["payload"]["graph"]["edges"] = serde_json::json!([
        { "id": "sa", "from": "s", "to": "a", "capacity": "1", "cost": "0" },
        { "id": "at", "from": "a", "to": "t", "capacity": "1", "cost": "0" },
        { "id": "sb", "from": "s", "to": "b", "capacity": "1", "cost": "0" },
        { "id": "bt", "from": "b", "to": "t", "capacity": "1", "cost": "0" },
        { "id": "ab", "from": "a", "to": "b", "capacity": "1", "cost": "0" }
    ]);
    scenario.to_string()
}

fn interior_point_max_flow_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&interior_point_max_flow_scenario("trace")).expect("scenario JSON");
    let nodes = (0..=flow::INTERIOR_POINT_MAX_FLOW_MAX_NODES)
        .map(|index| serde_json::json!({ "id": format!("n{index:02}") }))
        .collect::<Vec<_>>();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "max-flow",
        "source": "n00",
        "sink": format!("n{:02}", flow::INTERIOR_POINT_MAX_FLOW_MAX_NODES)
    });
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": [{
            "id": "direct",
            "from": "n00",
            "to": format!("n{:02}", flow::INTERIOR_POINT_MAX_FLOW_MAX_NODES),
            "capacity": "1",
            "cost": "0"
        }]
    });
    scenario.to_string()
}

fn minimum_ratio_cycle_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&electrical_flow_scenario(run_profile)).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("minimum-ratio-cycle-max-flow");
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [
            { "id": "s" }, { "id": "a" }, { "id": "b" },
            { "id": "c" }, { "id": "t" }
        ],
        "edges": [
            { "id": "sa", "from": "s", "to": "a", "capacity": "2", "cost": "4" },
            { "id": "ab", "from": "a", "to": "b", "capacity": "1", "cost": "-2" },
            { "id": "bs", "from": "b", "to": "s", "capacity": "1", "cost": "-1" },
            { "id": "ac", "from": "a", "to": "c", "capacity": "1", "cost": "8" },
            { "id": "cb", "from": "c", "to": "b", "capacity": "1", "cost": "0" },
            { "id": "st", "from": "s", "to": "t", "capacity": "1", "cost": "0" }
        ]
    });
    scenario.to_string()
}

fn minimum_ratio_cycle_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&minimum_ratio_cycle_scenario("trace")).expect("scenario JSON");
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [{ "id": "s" }, { "id": "t" }],
        "edges": (0..=flow::MINIMUM_RATIO_CYCLE_MAX_EDGES)
            .map(|index| serde_json::json!({
                "id": format!("e{index:02}"),
                "from": "s",
                "to": "t",
                "capacity": "1",
                "cost": "0"
            }))
            .collect::<Vec<_>>()
    });
    scenario.to_string()
}

fn randomized_almost_linear_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&electrical_flow_scenario(run_profile)).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] =
        serde_json::json!("randomized-almost-linear-max-flow-oracle-demonstrator");
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [{ "id": "s" }, { "id": "a" }, { "id": "b" }, { "id": "t" }],
        "edges": [
            { "id": "sa", "from": "s", "to": "a", "capacity": "3", "cost": "0" },
            { "id": "sb", "from": "s", "to": "b", "capacity": "2", "cost": "0" },
            { "id": "ab", "from": "a", "to": "b", "capacity": "1", "cost": "0" },
            { "id": "at", "from": "a", "to": "t", "capacity": "2", "cost": "0" },
            { "id": "bt", "from": "b", "to": "t", "capacity": "3", "cost": "0" }
        ]
    });
    scenario.to_string()
}

fn weighted_augmenting_paths_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&randomized_almost_linear_scenario(run_profile))
            .expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("weighted-augmenting-paths");
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [{ "id": "s" }, { "id": "a" }, { "id": "b" }, { "id": "t" }],
        "edges": [
            { "id": "sa", "from": "s", "to": "a", "capacity": "7", "cost": "0" },
            { "id": "sb", "from": "s", "to": "b", "capacity": "4", "cost": "0" },
            { "id": "ab", "from": "a", "to": "b", "capacity": "7", "cost": "0" },
            { "id": "ba", "from": "b", "to": "a", "capacity": "7", "cost": "0" },
            { "id": "at", "from": "a", "to": "t", "capacity": "4", "cost": "0" },
            { "id": "bt", "from": "b", "to": "t", "capacity": "6", "cost": "0" }
        ]
    });
    scenario.to_string()
}

fn weighted_augmenting_paths_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&weighted_augmenting_paths_scenario("trace")).expect("scenario JSON");
    let nodes = (0..=flow::WEIGHTED_AUGMENTING_PATHS_MAX_NODES)
        .map(|index| serde_json::json!({ "id": format!("n{index:02}") }))
        .collect::<Vec<_>>();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "max-flow",
        "source": "n00",
        "sink": format!("n{:02}", flow::WEIGHTED_AUGMENTING_PATHS_MAX_NODES)
    });
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": [{
            "id": "direct",
            "from": "n00",
            "to": format!("n{:02}", flow::WEIGHTED_AUGMENTING_PATHS_MAX_NODES),
            "capacity": "1",
            "cost": "0"
        }]
    });
    scenario.to_string()
}

fn weighted_push_relabel_shortcut_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&weighted_augmenting_paths_scenario(run_profile))
            .expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("weighted-push-relabel");
    scenario.to_string()
}

fn weighted_push_relabel_shortcut_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&weighted_push_relabel_shortcut_scenario("trace"))
            .expect("scenario JSON");
    let nodes = (0..=flow::WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_NODES)
        .map(|index| serde_json::json!({ "id": format!("n{index:02}") }))
        .collect::<Vec<_>>();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "max-flow",
        "source": "n00",
        "sink": format!("n{:02}", flow::WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_NODES)
    });
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": [{
            "id": "direct",
            "from": "n00",
            "to": format!("n{:02}", flow::WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_NODES),
            "capacity": "1",
            "cost": "0"
        }]
    });
    scenario.to_string()
}

fn randomized_almost_linear_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&randomized_almost_linear_scenario("trace")).expect("scenario JSON");
    let nodes = (0..=flow::RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_NODES)
        .map(|index| serde_json::json!({ "id": format!("n{index:02}") }))
        .collect::<Vec<_>>();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "max-flow",
        "source": "n00",
        "sink": format!("n{:02}", flow::RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_NODES)
    });
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": [{
            "id": "direct",
            "from": "n00",
            "to": format!("n{:02}", flow::RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_NODES),
            "capacity": "1",
            "cost": "0"
        }]
    });
    scenario.to_string()
}

fn deterministic_almost_linear_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&randomized_almost_linear_scenario(run_profile))
            .expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] =
        serde_json::json!("deterministic-almost-linear-max-flow-oracle-demonstrator");
    scenario.to_string()
}

fn deterministic_almost_linear_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&deterministic_almost_linear_scenario("trace"))
            .expect("scenario JSON");
    let nodes = (0..=flow::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_NODES)
        .map(|index| serde_json::json!({ "id": format!("n{index:02}") }))
        .collect::<Vec<_>>();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "max-flow",
        "source": "n00",
        "sink": format!("n{:02}", flow::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_NODES)
    });
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": [{
            "id": "direct",
            "from": "n00",
            "to": format!("n{:02}", flow::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_NODES),
            "capacity": "1",
            "cost": "0"
        }]
    });
    scenario.to_string()
}

fn enhanced_capacity_scaling_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&enhanced_capacity_scaling_scenario("trace")).expect("scenario JSON");
    let nodes = (0..=flow::ENHANCED_CAPACITY_SCALING_MAX_NODES)
            .map(|index| {
                serde_json::json!({
                    "id": format!("n{index:02}"),
                    "supply": if index == 0 { "1" } else if index == flow::ENHANCED_CAPACITY_SCALING_MAX_NODES { "-1" } else { "0" }
                })
            })
            .collect::<Vec<_>>();
    let edges = (0..nodes.len())
        .map(|index| {
            serde_json::json!({
                "id": format!("e{index:02}"),
                "from": format!("n{index:02}"),
                "to": format!("n{:02}", (index + 1) % nodes.len()),
                "capacity": "1",
                "cost": "0"
            })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": edges
    });
    scenario.to_string()
}

fn dual_network_simplex_scenario(run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": { "kind": "transshipment" },
            "graph": {
                "nodes": [
                    { "id": "a", "supply": "-5" },
                    { "id": "b", "supply": "5" },
                    { "id": "c", "supply": "0" }
                ],
                "edges": [
                    { "id": "ab", "from": "a", "to": "b", "capacity": "10", "cost": "1" },
                    { "id": "ac", "from": "a", "to": "c", "capacity": "10", "cost": "5" },
                    { "id": "ba", "from": "b", "to": "a", "capacity": "10", "cost": "4" },
                    { "id": "bc", "from": "b", "to": "c", "capacity": "10", "cost": "1" },
                    { "id": "ca", "from": "c", "to": "a", "capacity": "10", "cost": "4" },
                    { "id": "cb", "from": "c", "to": "b", "capacity": "10", "cost": "4" }
                ]
            },
            "algorithm": { "id": "dual-network-simplex", "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn dual_network_simplex_oversized_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&dual_network_simplex_scenario("trace")).expect("scenario JSON");
    let nodes = (0..=flow::DUAL_NETWORK_SIMPLEX_MAX_NODES)
        .map(|index| {
            serde_json::json!({
                "id": format!("n{index:02}"),
                "supply": if index == 0 { "1" } else if index == 1 { "-1" } else { "0" }
            })
        })
        .collect::<Vec<_>>();
    let edges = (0..nodes.len())
        .map(|index| {
            serde_json::json!({
                "id": format!("e{index:02}"),
                "from": format!("n{index:02}"),
                "to": format!("n{:02}", (index + 1) % nodes.len()),
                "capacity": "65",
                "cost": "0"
            })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": nodes,
        "edges": edges
    });
    scenario.to_string()
}

fn polynomial_dual_simplex_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&dual_network_simplex_scenario(run_profile)).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("polynomial-dual-network-simplex");
    scenario.to_string()
}

fn polynomial_primal_simplex_scenario(run_profile: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": { "kind": "transshipment" },
            "graph": {
                "nodes": [
                    { "id": "s", "supply": "3" },
                    { "id": "a", "supply": "0" },
                    { "id": "t", "supply": "-3" }
                ],
                "edges": [
                    { "id": "sa", "from": "s", "to": "a", "capacity": "4", "cost": "3" },
                    { "id": "at", "from": "a", "to": "t", "capacity": "4", "cost": "2" },
                    { "id": "st", "from": "s", "to": "t", "capacity": "4", "cost": "9" },
                    { "id": "as", "from": "a", "to": "s", "capacity": "2", "cost": "-1" }
                ]
            },
            "algorithm": { "id": "polynomial-primal-network-simplex", "config": {} },
            "run_profile": run_profile,
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string()
}

fn double_scaling_scenario() -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&cancel_tighten_scenario()).expect("scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("double-scaling");
    scenario.to_string()
}

fn convex_cost_scenario() -> String {
    serde_json::json!({
            "schema_version": 1,
            "scenario_encoding_revision": "rfc8785-jcs/1",
            "plugin": "flow",
            "reproducibility": { "declared": {
                "algorithm_revision": "flow-algorithms/8",
                "rng_version": 1,
                "plugin_result_revision": "flow-result/9",
                "metrics_catalog_revision": "flow-metrics/6",
                "trace_revision": "flow-trace/9",
                "projection_revision": "flow-projection/6",
                "layout_revision": "flow-layout/1",
                "frame_encoding_revision": "flow-scene/9"
            }},
            "payload": {
                "model": { "kind": "convex-cost-flow" },
                "graph": {
                    "nodes": [
                        { "id": "s", "supply": "3" },
                        { "id": "m", "supply": "0" },
                        { "id": "t", "supply": "-3" }
                    ],
                    "edges": [
                        {
                            "id": "direct", "from": "s", "to": "t",
                            "capacity": "3", "cost": "0",
                            "convex_cost": {
                                "base_cost_at_zero": "7",
                                "segments": [
                                    { "end_flow": "1", "marginal_cost": "0" },
                                    { "end_flow": "3", "marginal_cost": "5" }
                                ]
                            }
                        },
                        { "id": "sm", "from": "s", "to": "m", "lower": "1", "capacity": "3", "cost": "1" },
                        { "id": "mt", "from": "m", "to": "t", "lower": "1", "capacity": "3", "cost": "1" }
                    ]
                },
                "algorithm": { "id": "segment-expanded-convex-mcf", "config": {} },
                "run_profile": "trace",
                "trace_granularity": "operation",
                "algorithm_seed": "0"
            }
        })
        .to_string()
}

fn convex_cost_scaling_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&convex_cost_scenario()).expect("convex scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("convex-cost-scaling");
    scenario["payload"]["run_profile"] = serde_json::json!(run_profile);
    scenario.to_string()
}

fn convex_network_simplex_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&convex_cost_scenario()).expect("convex scenario JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!("convex-network-simplex");
    scenario["payload"]["run_profile"] = serde_json::json!(run_profile);
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [
            { "id": "s", "supply": "4" },
            { "id": "m", "supply": "0" },
            { "id": "t", "supply": "-4" }
        ],
        "edges": [
            {
                "id": "direct", "from": "s", "to": "t", "capacity": "4", "cost": "0",
                "convex_cost": {
                    "base_cost_at_zero": "0",
                    "segments": [
                        { "end_flow": "1", "marginal_cost": "-2" },
                        { "end_flow": "2", "marginal_cost": "1" },
                        { "end_flow": "4", "marginal_cost": "8" }
                    ]
                }
            },
            {
                "id": "sm", "from": "s", "to": "m", "capacity": "4", "cost": "0",
                "convex_cost": {
                    "base_cost_at_zero": "0",
                    "segments": [{ "end_flow": "4", "marginal_cost": "2" }]
                }
            },
            {
                "id": "mt", "from": "m", "to": "t", "capacity": "4", "cost": "0",
                "convex_cost": {
                    "base_cost_at_zero": "0",
                    "segments": [{ "end_flow": "4", "marginal_cost": "2" }]
                }
            }
        ]
    });
    scenario.to_string()
}

fn commit_next(session: &mut WasmSession) -> String {
    let frame = session
        .stage_next_json()
        .expect("next item stages")
        .expect("timeline has a next item");
    session.commit_staged_next();
    frame
}

fn commit_until_catalog(session: &mut WasmSession, catalog_id: &str) -> serde_json::Value {
    loop {
        let frame: serde_json::Value =
            serde_json::from_str(&commit_next(session)).expect("trace frame JSON");
        if frame["trace_event"]["catalog_id"] == catalog_id {
            return frame;
        }
        assert!(
            session.cursor() < session.item_count(),
            "trace does not contain {catalog_id}"
        );
    }
}

fn convex_network_simplex_fast_scene() -> serde_json::Value {
    let mut fast = WasmSession::new(&convex_network_simplex_scenario("fast"))
        .expect("convex network-simplex fast initializes");
    while fast
        .stage_next_json()
        .expect("convex simplex fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    serde_json::from_str(
        &fast
            .current_frame_json()
            .expect("convex simplex fast serializes"),
    )
    .expect("convex simplex fast JSON")
}

fn assert_shared_trace_detail_is_integer(scene: &serde_json::Value) {
    if let Some(detail) = scene["trace_event"]["detail"].as_object() {
        detail["value"]
            .as_str()
            .expect("trace detail value")
            .parse::<i128>()
            .expect("shared trace detail remains an integer");
    }
}

fn assert_shared_trace_detail_is_finite_decimal(scene: &serde_json::Value) {
    if let Some(detail) = scene["trace_event"]["detail"].as_object() {
        let value = detail["value"].as_str().expect("trace detail value");
        let parsed = value
            .parse::<f64>()
            .expect("shared trace detail remains a decimal");
        assert!(parsed.is_finite(), "shared trace detail remains finite");
        assert_ne!(value, "-0", "shared trace detail uses canonical zero");
    }
}

#[test]
fn zadeh_phase_chain_stress_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "zadeh-phase-chain-stress",
                    "group_size": 8
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("Zadeh-derived graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 24);
    assert_eq!(generated["stats"]["edge_count"], 102);
    assert_eq!(generated["provenance"]["difficulty"], "stress");
    assert_eq!(generated["provenance"]["origin"], "paper-derived");
    assert_eq!(generated["provenance"]["sampling"], "deterministic");
    assert_eq!(
        generated["provenance"]["source_id"],
        "zadeh-pathological-max-flow-1973-derived-phase-chain"
    );
    assert!(
        generated["provenance"]
            .get("difficulty_certificate")
            .is_none()
    );
}

#[test]
fn vision_segmentation_grid_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "vision-segmentation-grid",
                    "rows": 3,
                    "columns": 4,
                    "eight_neighbor": true
                },
                "capacity": { "kind": "uniform", "minimum": "1", "maximum": "12" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("vision grid crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 14);
    assert_eq!(generated["stats"]["edge_count"], 82);
    assert_eq!(generated["suggested_model"]["kind"], "max-flow");
    assert_eq!(generated["provenance"]["origin"], "paper-derived");
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "boykov-kolmogorov-2004-vision-grid-derived"
    );
    assert!(
        generated["provenance"]["tags"]
            .as_array()
            .expect("tags")
            .contains(&serde_json::json!("terminal-heavy"))
    );
}

#[test]
fn washington_dinic_phase_stress_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "washington-dinic-phase-stress",
                    "nodes": 8
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("Washington function 9 graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 8);
    assert_eq!(generated["stats"]["edge_count"], 13);
    assert_eq!(generated["stats"]["maximum_capacity"], "8");
    assert_eq!(generated["provenance"]["difficulty"], "stress");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "deterministic");
    assert_eq!(
        generated["provenance"]["source_id"],
        "anderson-washington-dinic-bad-case-1991-derived"
    );
    assert!(
        generated["provenance"]
            .get("difficulty_certificate")
            .is_none()
    );
}

#[test]
fn washington_goldberg_fifo_stress_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "washington-goldberg-fifo-stress",
                    "block_size": 8
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("Washington function 10 graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 27);
    assert_eq!(generated["stats"]["edge_count"], 33);
    assert_eq!(generated["stats"]["minimum_capacity"], "1");
    assert_eq!(generated["stats"]["maximum_capacity"], "8");
    assert_eq!(generated["provenance"]["difficulty"], "stress");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "deterministic");
    assert_eq!(
        generated["provenance"]["source_id"],
        "anderson-washington-gold-bad-case-1991-derived"
    );
    assert!(
        generated["provenance"]
            .get("difficulty_certificate")
            .is_none()
    );
}

#[test]
fn washington_cheriyan_stress_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "washington-cheriyan-stress",
                    "bridge_width": 8,
                    "gadget_entries": 4,
                    "chain_length": 2
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("Washington function 11 graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 55);
    assert_eq!(generated["stats"]["edge_count"], 75);
    assert_eq!(generated["stats"]["minimum_capacity"], "1");
    assert_eq!(generated["stats"]["maximum_capacity"], "1000000");
    assert_eq!(generated["provenance"]["difficulty"], "stress");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "deterministic");
    assert_eq!(
        generated["provenance"]["source_id"],
        "anderson-washington-cheriyan-1991-derived"
    );
    assert!(
        generated["provenance"]
            .get("difficulty_certificate")
            .is_none()
    );
}

#[test]
fn cherkassky_goldberg_ak_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "cherkassky-goldberg-ak-stress",
                    "size": 4
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("AK graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 22);
    assert_eq!(generated["stats"]["edge_count"], 31);
    assert_eq!(generated["stats"]["minimum_capacity"], "1");
    assert_eq!(generated["stats"]["maximum_capacity"], "1000000");
    assert_eq!(generated["provenance"]["difficulty"], "stress");
    assert_eq!(generated["provenance"]["origin"], "paper-derived");
    assert_eq!(generated["provenance"]["sampling"], "deterministic");
    assert_eq!(
        generated["provenance"]["source_id"],
        "cherkassky-goldberg-ak-1997-independent-derived"
    );
    assert!(
        generated["provenance"]
            .get("difficulty_certificate")
            .is_none()
    );
}

#[test]
fn waissi_setubal_acyclic_dense_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "waissi-setubal-acyclic-dense",
                    "nodes": 12
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("First DIMACS AC graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 12);
    assert_eq!(generated["stats"]["edge_count"], 66);
    let edges = generated["graph"]["edges"]
        .as_array()
        .expect("edges are an array");
    assert!(edges.iter().all(|edge| {
        edge["capacity"]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|capacity| (1..=1_000_000).contains(&capacity))
    }));
    assert!(edges.iter().all(|edge| edge["cost"] == "0"));
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "waissi-setubal-ac-1991-project-rng-derived"
    );
    assert!(
        generated["provenance"]
            .get("difficulty_certificate")
            .is_none()
    );
}

#[test]
fn glover_dense_acyclic_stress_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "glover-dense-acyclic-stress",
                    "nodes": 12
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("Glover-Waissi dense stress crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 12);
    assert_eq!(generated["stats"]["edge_count"], 66);
    assert_eq!(generated["stats"]["minimum_capacity"], "1");
    assert_eq!(generated["stats"]["maximum_capacity"], "26");
    assert_eq!(generated["provenance"]["difficulty"], "stress");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "deterministic");
    assert_eq!(
        generated["provenance"]["source_id"],
        "waissi-glover-dense-acyclic-1991-derived"
    );
    assert!(
        generated["provenance"]
            .get("difficulty_certificate")
            .is_none()
    );
}

#[test]
fn waissi_transit_one_way_grid_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "waissi-transit-one-way-grid",
                    "dimension": 4,
                    "maximum_capacity": 100
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("Waissi one-way transit grid crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 18);
    assert_eq!(generated["stats"]["edge_count"], 32);
    let edges = generated["graph"]["edges"]
        .as_array()
        .expect("edges are an array");
    assert!(edges.iter().all(|edge| {
        edge["cost"] == "0"
            && edge["capacity"]
                .as_str()
                .and_then(|value| value.parse::<u64>().ok())
                .is_some_and(|capacity| (1..=100).contains(&capacity))
    }));
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["tags"],
        serde_json::json!([
            "grid",
            "one-way",
            "transit-grid",
            "waissi-transit-one-way-grid"
        ])
    );
    assert_eq!(
        generated["provenance"]["source_id"],
        "waissi-transit-one-way-grid-1991-project-rng-derived"
    );
    assert_eq!(
        generated["provenance"]["materialized_sha256"],
        "f2bd6008f50c2b3252a572afe868c0b03dd4825938e3dcb58ade6eef7091980f"
    );
}

#[test]
fn waissi_transit_two_way_grid_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "waissi-transit-two-way-grid",
                    "dimension": 4,
                    "maximum_capacity": 100
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("Waissi transit grid crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 18);
    assert_eq!(generated["stats"]["edge_count"], 64);
    let edges = generated["graph"]["edges"]
        .as_array()
        .expect("edges are an array");
    assert!(edges.iter().all(|edge| {
        edge["cost"] == "0"
            && edge["capacity"]
                .as_str()
                .and_then(|value| value.parse::<u64>().ok())
                .is_some_and(|capacity| (1..=100).contains(&capacity))
    }));
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "waissi-transit-two-way-grid-1991-project-rng-derived"
    );
    assert!(
        generated["provenance"]
            .get("difficulty_certificate")
            .is_none()
    );
}

#[test]
fn assignment_matrix_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "assignment-matrix",
                    "agents": 4,
                    "tasks": 5,
                    "objective": "maximize",
                    "shape": {
                        "kind": "planted-optimum",
                        "density_per_mille": 600,
                        "base_cost": 17,
                        "gap": 5,
                        "noise": 3
                    }
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("assignment graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 9);
    assert_eq!(generated["stats"]["edge_count"], 12);
    assert_eq!(generated["suggested_model"]["kind"], "assignment");
    assert_eq!(generated["suggested_model"]["objective"], "maximize");
    assert_eq!(generated["provenance"]["origin"], "project-synthetic");
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "flow-assignment-matrix-contract-v1"
    );
    assert!(
        generated["graph"]["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .all(|edge| edge["capacity"] == "1")
    );
}

#[test]
fn transportation_table_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "transportation-table",
                    "origins": 3,
                    "destinations": 4,
                    "total_supply": 12,
                    "shape": {
                        "kind": "sparse-feasible",
                        "density_per_mille": 350,
                        "minimum_cost": -2,
                        "maximum_cost": 8
                    }
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("transportation table crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 7);
    assert_eq!(generated["stats"]["edge_count"], 6);
    assert_eq!(generated["stats"]["maximum_capacity"], "12");
    assert_eq!(generated["suggested_model"]["kind"], "transportation");
    assert_eq!(
        generated["suggested_model"]["origins"],
        serde_json::json!(["o0000", "o0001", "o0002"])
    );
    assert_eq!(generated["provenance"]["origin"], "project-synthetic");
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "flow-transportation-table-contract-v1"
    );
    assert!(
        generated["provenance"]["tags"]
            .as_array()
            .expect("tags")
            .contains(&serde_json::json!("sparse-feasible"))
    );
}

#[test]
fn rmfgen_frames_cross_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "rmfgen-frames",
                    "frame_size": 3,
                    "depth": 3,
                    "minimum_capacity": 2,
                    "maximum_capacity": 7
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("RMFGEN-derived graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 27);
    assert_eq!(generated["stats"]["edge_count"], 90);
    assert_eq!(generated["stats"]["maximum_capacity"], "63");
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["generator_revision"],
        "flow-generator/27"
    );
}

#[test]
fn gridgen_grid_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "gridgen-grid",
                    "rows": 3,
                    "columns": 4,
                    "terminal_pairs": 2,
                    "average_degree": 3,
                    "total_supply": 20,
                    "two_way": true,
                    "minimum_capacity": 3,
                    "maximum_capacity": 9,
                    "minimum_cost": 2,
                    "maximum_cost": 7
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("GRIDGEN-derived graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 13);
    assert_eq!(generated["stats"]["edge_count"], 39);
    assert_eq!(generated["suggested_model"]["kind"], "transshipment");
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "lee-orlin-gridgen-1991-project-rng-uniform-derived"
    );
    assert_eq!(
        generated["provenance"]["generator_revision"],
        "flow-generator/27"
    );
}

#[test]
fn gridgraph_grid_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "gridgraph-grid",
                    "rows": 4,
                    "columns": 5,
                    "maximum_capacity": 9,
                    "maximum_cost": 17
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("GRIDGRAPH-derived graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 22);
    assert_eq!(generated["stats"]["edge_count"], 39);
    assert_eq!(generated["suggested_model"]["kind"], "transshipment");
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "resende-gridgraph-1991-ggraph1-project-rng-derived"
    );
    assert_eq!(
        generated["provenance"]["generator_revision"],
        "flow-generator/27"
    );
    let nodes = generated["graph"]["nodes"]
        .as_array()
        .expect("nodes are an array");
    let source_supply = nodes[0]["supply"]
        .as_str()
        .expect("source supply")
        .parse::<i64>()
        .expect("source supply integer");
    let sink_supply = nodes.last().expect("sink")["supply"]
        .as_str()
        .expect("sink supply")
        .parse::<i64>()
        .expect("sink supply integer");
    assert!(source_supply > 0);
    assert_eq!(sink_supply, -source_supply);
}

#[test]
fn washington_matching_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "washington-matching",
                    "part_size": 12,
                    "degree": 3
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("Washington Matching graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 26);
    assert_eq!(generated["stats"]["edge_count"], 60);
    assert_eq!(generated["stats"]["minimum_capacity"], "1");
    assert_eq!(generated["stats"]["maximum_capacity"], "1");
    assert_eq!(
        generated["suggested_model"],
        serde_json::json!({
            "kind": "bipartite-matching",
            "left": (0..12).map(|index| format!("l{index:04}")).collect::<Vec<_>>(),
            "right": (0..12).map(|index| format!("r{index:04}")).collect::<Vec<_>>(),
            "flow_adapter": { "source": "s", "sink": "t" }
        })
    );
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "anderson-washington-matching-1991-project-rng-derived"
    );
    assert_eq!(
        generated["provenance"]["tags"],
        serde_json::json!([
            "bipartite",
            "dag",
            "unit-capacity",
            "unit-network",
            "washington-matching"
        ])
    );
    assert!(generated["provenance"]["difficulty_certificate"].is_null());
}

#[test]
fn washington_mesh_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "washington-mesh",
                    "rows": 6,
                    "columns": 8,
                    "maximum_capacity": 9
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("Washington Mesh graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 50);
    assert_eq!(generated["stats"]["edge_count"], 138);
    assert_eq!(generated["stats"]["minimum_capacity"], "1");
    assert_eq!(generated["stats"]["maximum_capacity"], "27");
    assert_eq!(generated["suggested_model"]["kind"], "max-flow");
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "anderson-washington-mesh-1991-project-rng-derived"
    );
    assert!(generated["provenance"]["difficulty_certificate"].is_null());
}

#[test]
fn washington_square_mesh_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "washington-square-mesh",
                    "dimension": 6,
                    "degree": 3,
                    "maximum_capacity": 9
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("Washington Square Mesh graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 38);
    assert_eq!(generated["stats"]["edge_count"], 99);
    assert_eq!(generated["stats"]["minimum_capacity"], "1");
    assert_eq!(generated["stats"]["maximum_capacity"], "27");
    assert_eq!(generated["suggested_model"]["kind"], "max-flow");
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "anderson-washington-square-mesh-1991-project-rng-derived"
    );
    assert_eq!(
        generated["provenance"]["tags"],
        serde_json::json!(["dag", "grid", "washington-square-mesh"])
    );
    assert!(generated["provenance"]["difficulty_certificate"].is_null());
}

#[test]
fn washington_line_profiles_cross_the_wasm_generator_boundary() {
    for (family_id, edge_count) in [
        ("washington-basic-line", 61),
        ("washington-exponential-line", 61),
        ("washington-double-exponential-line", 63),
    ] {
        let generated: serde_json::Value = serde_json::from_str(
            &generate_flow_graph_json(
                &serde_json::json!({
                    "generator_revision": "flow-generator/27",
                    "seed": "42",
                    "family": {
                        "family_id": family_id,
                        "levels": 6,
                        "width": 4,
                        "degree": 3
                    },
                    "capacity": { "kind": "unit" },
                    "cost": { "kind": "zero" }
                })
                .to_string(),
            )
            .expect("Washington Line graph crosses the WASM boundary"),
        )
        .expect("generated flow graph is JSON");
        assert_eq!(generated["stats"]["node_count"], 26);
        assert_eq!(generated["stats"]["edge_count"], edge_count);
        assert_eq!(generated["stats"]["maximum_capacity"], "20000000");
        assert_eq!(generated["stats"]["minimum_cost"], "0");
        assert_eq!(generated["suggested_model"]["kind"], "max-flow");
        assert_eq!(generated["provenance"]["difficulty"], "ordinary");
        assert_eq!(
            generated["provenance"]["origin"],
            "official-benchmark-derived"
        );
        assert_eq!(generated["provenance"]["sampling"], "randomized");
        assert!(generated["provenance"]["difficulty_certificate"].is_null());
    }
}

#[test]
fn washington_random_level_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "washington-random-level",
                    "rows": 6,
                    "columns": 8,
                    "maximum_capacity": 9
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("Washington Random Level graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 50);
    assert_eq!(generated["stats"]["edge_count"], 138);
    assert_eq!(generated["suggested_model"]["kind"], "max-flow");
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "anderson-washington-random-level-1991-project-rng-derived"
    );
}

#[test]
fn goto_torus_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "goto-torus",
                    "nodes": 32,
                    "edge_count": 256,
                    "maximum_capacity": 1_000,
                    "maximum_cost": 10_000
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("GOTO-derived graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 32);
    assert_eq!(generated["stats"]["edge_count"], 256);
    assert_eq!(generated["suggested_model"]["kind"], "transshipment");
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "goldberg-goto-1991-project-rng-power2-derived"
    );
    assert_eq!(
        generated["provenance"]["generator_revision"],
        "flow-generator/27"
    );
}

#[test]
fn goldberg_mesh_circulation_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "goldberg-mesh-circulation",
                    "columns": 4,
                    "rows": 3,
                    "horizontal_degree": 1,
                    "vertical_degree": 1
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("Goldberg mesh circulation crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 12);
    assert_eq!(generated["stats"]["edge_count"], 48);
    assert_eq!(generated["suggested_model"]["kind"], "circulation");
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "goldberg-mesh1-1991-project-rng-signed-bound-derived"
    );
    assert_eq!(
        generated["provenance"]["materialized_sha256"],
        "f4357b6b01219ead84861ecb193e455167a4dbafbf3c448b74a82a064cf8e02c"
    );
    assert_eq!(
        generated["provenance"]["tags"],
        serde_json::json!([
            "bidirectional",
            "circulation",
            "distance-decay",
            "goldberg-mesh-circulation",
            "grid",
            "signed-cost",
            "toroidal"
        ])
    );
    assert!(generated["provenance"]["difficulty_certificate"].is_null());

    let edges = generated["graph"]["edges"]
        .as_array()
        .expect("edges are an array");
    for pair in edges.chunks_exact(2) {
        assert_eq!(pair[0]["from"], pair[1]["to"]);
        assert_eq!(pair[0]["to"], pair[1]["from"]);
        let forward_cost = pair[0]["cost"]
            .as_str()
            .expect("cost")
            .parse::<i64>()
            .expect("integer cost");
        let reverse_cost = pair[1]["cost"]
            .as_str()
            .expect("cost")
            .parse::<i64>()
            .expect("integer cost");
        assert_eq!(forward_cost, -reverse_cost);
    }
}

#[test]
fn netgen_skeleton_crosses_the_wasm_generator_boundary() {
    let generated: serde_json::Value = serde_json::from_str(
        &generate_flow_graph_json(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "netgen-skeleton",
                    "nodes": 20,
                    "sources": 3,
                    "sinks": 4,
                    "edge_count": 70,
                    "minimum_cost": -7,
                    "maximum_cost": 20,
                    "total_supply": 40,
                    "transshipment_sources": 1,
                    "transshipment_sinks": 1,
                    "high_cost_percentage": 100,
                    "capacitated_percentage": 100,
                    "minimum_capacity": 2,
                    "maximum_capacity": 9
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("NETGEN-derived graph crosses the WASM boundary"),
    )
    .expect("generated flow graph is JSON");

    assert_eq!(generated["stats"]["node_count"], 20);
    assert_eq!(generated["stats"]["edge_count"], 70);
    assert_eq!(generated["suggested_model"]["kind"], "transshipment");
    assert_eq!(generated["provenance"]["difficulty"], "ordinary");
    assert_eq!(
        generated["provenance"]["origin"],
        "official-benchmark-derived"
    );
    assert_eq!(generated["provenance"]["sampling"], "randomized");
    assert_eq!(
        generated["provenance"]["source_id"],
        "klingman-napier-stutz-netgen-1974-project-rng-independent-derived"
    );
    assert_eq!(
        generated["provenance"]["generator_revision"],
        "flow-generator/27"
    );

    let supplies = generated["graph"]["nodes"]
        .as_array()
        .expect("nodes are an array")
        .iter()
        .map(|node| {
            node["supply"]
                .as_str()
                .expect("supply is canonical text")
                .parse::<i64>()
                .expect("supply is an integer")
        })
        .collect::<Vec<_>>();
    assert_eq!(supplies.iter().sum::<i64>(), 0);
    assert_eq!(supplies.iter().filter(|&&supply| supply > 0).count(), 3);
    assert_eq!(supplies.iter().filter(|&&supply| supply < 0).count(), 4);
    assert_eq!(
        supplies.iter().filter(|&&supply| supply > 0).sum::<i64>(),
        40
    );
}

#[test]
fn potential_dijkstra_dispatches_prices_paths_and_certified_metrics() {
    let mut session =
        WasmSession::new(&potential_dijkstra_scenario()).expect("potential Dijkstra Scenario");
    let mut catalog_ids = Vec::new();
    let mut feasibility_residual_arc_scans = 0_usize;
    let mut feasibility_relabels = 0_usize;
    while session
        .stage_next_json()
        .expect("potential Dijkstra trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("potential Dijkstra frame serializes"),
        )
        .expect("potential Dijkstra frame JSON");
        if let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() {
            catalog_ids.push(catalog_id.to_owned());
            if catalog_id == "feasibility.feasible" {
                feasibility_residual_arc_scans = frame["metrics"][2]
                    .as_str()
                    .and_then(|value| value.parse::<usize>().ok())
                    .expect("feasibility scan metric is a canonical integer");
                feasibility_relabels = frame["metrics"][7]
                    .as_str()
                    .and_then(|value| value.parse::<usize>().ok())
                    .expect("feasibility relabel metric is a canonical integer");
            }
        }
    }
    for catalog_id in [
        "potential-dijkstra-ssp.initial-potentials",
        "potential-dijkstra-ssp.shortest-path",
        "potential-dijkstra-ssp.update-potentials",
        "potential-dijkstra-ssp.augment",
        "potential-dijkstra-ssp.optimal",
    ] {
        assert!(catalog_ids.iter().any(|value| value == catalog_id));
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final potential Dijkstra scene serializes"),
    )
    .expect("final potential Dijkstra scene JSON");
    let residual_arc_scans = solved["metrics"][2]
        .as_str()
        .and_then(|value| value.parse::<usize>().ok())
        .expect("residual scan metric is a canonical integer");
    assert_eq!(
        catalog_ids
            .iter()
            .filter(|id| id.as_str() == "potential-dijkstra-ssp.inspect-residual-arc")
            .count(),
        residual_arc_scans
            .checked_sub(feasibility_residual_arc_scans)
            .expect("source scan metric includes the feasibility prefix"),
        "every Dijkstra residual-arc inspection owns one local Detail boundary"
    );
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "8");
    assert_eq!(solved["metrics"][3], "2");
    assert_eq!(solved["metrics"][4], "2");
    assert_eq!(
        solved["metrics"][7]
            .as_str()
            .and_then(|value| value.parse::<usize>().ok())
            .and_then(|value| value.checked_sub(feasibility_relabels)),
        Some(2),
        "the source solver performs two price updates after feasibility"
    );
    assert!(
        solved["metrics"][15]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|value| value > 0)
    );
    assert_eq!(
        solved["outcome"]["potentials"]
            .as_array()
            .expect("dual potentials")
            .len(),
        4
    );
}

#[test]
fn successive_shortest_path_dispatches_dedicated_events_with_fast_trace_parity() {
    let mut trace_scenario: serde_json::Value =
        serde_json::from_str(&potential_dijkstra_scenario()).expect("scenario JSON");
    trace_scenario["payload"]["algorithm"]["id"] = serde_json::json!("successive-shortest-path");
    let mut fast_scenario = trace_scenario.clone();
    fast_scenario["payload"]["run_profile"] = serde_json::json!("fast");

    let mut traced = WasmSession::new(&trace_scenario.to_string()).expect("trace SSP Scenario");
    let mut catalog_ids = Vec::new();
    let mut feasibility_residual_arc_scans = 0_usize;
    while traced
        .stage_next_json()
        .expect("trace event stages")
        .is_some()
    {
        traced.commit_staged_next();
        let frame: serde_json::Value =
            serde_json::from_str(&traced.current_frame_json().expect("trace frame serializes"))
                .expect("trace frame JSON");
        if let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() {
            catalog_ids.push(catalog_id.to_owned());
            if catalog_id == "feasibility.feasible" {
                feasibility_residual_arc_scans = frame["metrics"][2]
                    .as_str()
                    .and_then(|value| value.parse::<usize>().ok())
                    .expect("feasibility scan metric is a canonical integer");
            }
        }
    }
    assert!(
        catalog_ids
            .iter()
            .all(|id| id.starts_with("feasibility.") || id.starts_with("successive-shortest-path."))
    );
    for expected in [
        "successive-shortest-path.shortest-path",
        "successive-shortest-path.augment",
        "successive-shortest-path.optimal",
    ] {
        assert!(catalog_ids.iter().any(|id| id == expected));
    }
    let final_cursor = traced.cursor();
    let traced_final: serde_json::Value =
        serde_json::from_str(&traced.current_frame_json().expect("trace final serializes"))
            .expect("trace final JSON");
    let residual_arc_scans = traced_final["metrics"][2]
        .as_str()
        .and_then(|value| value.parse::<usize>().ok())
        .expect("residual scan metric is a canonical integer");
    assert_eq!(
        catalog_ids
            .iter()
            .filter(|id| id.as_str() == "successive-shortest-path.inspect-residual-arc")
            .count(),
        residual_arc_scans
            .checked_sub(feasibility_residual_arc_scans)
            .expect("source scan metric includes the feasibility prefix"),
        "every Bellman--Ford residual-arc inspection owns one local Detail boundary"
    );
    let base: serde_json::Value =
        serde_json::from_str(&traced.seek_json(0).expect("reverse seek")).expect("base JSON");
    assert_eq!(base["solve_status"], "ready");
    let replayed: serde_json::Value =
        serde_json::from_str(&traced.seek_json(final_cursor).expect("forward replay"))
            .expect("replayed JSON");
    assert_eq!(replayed, traced_final);

    let mut fast = WasmSession::new(&fast_scenario.to_string()).expect("fast SSP Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_final: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast final serializes"))
            .expect("fast final JSON");
    assert_eq!(traced_final["solve_status"], "optimal");
    assert_eq!(traced_final["edge_states"], fast_final["edge_states"]);
    assert_eq!(traced_final["outcome"], fast_final["outcome"]);
    assert_eq!(traced_final["metrics"], fast_final["metrics"]);
}

#[test]
fn successive_shortest_augmenting_path_dispatches_both_certificates() {
    let mut session =
        WasmSession::new(&successive_shortest_augmenting_path_scenario()).expect("SSAP Scenario");
    let mut catalog_ids = Vec::new();
    let mut path_costs = Vec::new();
    while session
        .stage_next_json()
        .expect("SSAP trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value =
            serde_json::from_str(&session.current_frame_json().expect("SSAP frame serializes"))
                .expect("SSAP frame JSON");
        if let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() {
            catalog_ids.push(catalog_id.to_owned());
        }
        if frame["trace_event"]["detail"]["label"] == "path-cost" {
            path_costs.push(
                frame["trace_event"]["detail"]["value"]
                    .as_str()
                    .expect("path cost string")
                    .to_owned(),
            );
        }
    }
    assert_eq!(path_costs, ["2", "5"]);
    assert!(
        catalog_ids
            .iter()
            .any(|id| id.ends_with(".no-augmenting-path"))
    );
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final SSAP scene serializes"),
    )
    .expect("final SSAP scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "min-cost-max-flow");
    assert_eq!(solved["outcome"]["value"], "3");
    assert_eq!(solved["outcome"]["cut_bound"], "3");
    assert_eq!(solved["outcome"]["total_cost"], "12");
    assert_eq!(solved["metrics"][3], "2");
    assert_eq!(solved["metrics"][4], "3");
    assert_eq!(solved["metrics"][7], "2");
    assert_eq!(
        solved["outcome"]["potentials"]
            .as_array()
            .expect("dual potentials")
            .len(),
        4
    );
}

#[test]
fn primal_dual_dispatches_tightening_and_admissible_augmentation_events() {
    let mut session = WasmSession::new(&primal_dual_scenario()).expect("primal-dual Scenario");
    let mut catalog_ids = Vec::new();
    while session
        .stage_next_json()
        .expect("primal-dual trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("primal-dual frame serializes"),
        )
        .expect("primal-dual frame JSON");
        if let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() {
            catalog_ids.push(catalog_id.to_owned());
        }
    }
    for catalog_id in [
        "primal-dual-mcf.initialize-dual",
        "primal-dual-mcf.shortest-slack-labels",
        "primal-dual-mcf.tighten-dual",
        "primal-dual-mcf.augment-admissible-path",
        "primal-dual-mcf.optimal",
    ] {
        assert!(catalog_ids.iter().any(|value| value == catalog_id));
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final primal-dual scene serializes"),
    )
    .expect("final primal-dual scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "min-cost-flow");
    assert_eq!(solved["outcome"]["total_cost"], "8");
    assert_eq!(solved["metrics"][3], "2");
    assert_eq!(solved["metrics"][4], "2");
    assert_eq!(solved["metrics"][7], "2");
}

fn blocking_event_frame<'a>(
    event_frames: &'a [(String, serde_json::Value)],
    catalog_id: &str,
    occurrence: usize,
) -> &'a serde_json::Value {
    event_frames
        .iter()
        .filter(|(id, _)| id == catalog_id)
        .nth(occurrence)
        .map(|(_, frame)| frame)
        .expect("catalog event frame")
}

fn assert_blocking_node_labels(frame: &serde_json::Value, expected: &[&str]) {
    let actual = frame["node_trace_states"]
        .as_array()
        .expect("node trace states")
        .iter()
        .map(|node| node["label"].as_str().expect("node label"))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

fn assert_blocking_augmentation(
    frame: &serde_json::Value,
    expected_arcs: &[&str],
    expected_remaining: (&str, &str),
    expected_augmentations: &str,
) {
    let focus = frame["trace_event"]["entity_refs"]
        .as_array()
        .expect("local blocking-flow focus");
    assert_eq!(
        focus
            .iter()
            .filter(|entity| entity["kind"] == "edge")
            .count(),
        1,
        "the complete path remains in active_path; Detail focus owns one bottleneck edge"
    );
    assert_eq!(
        focus
            .iter()
            .filter(|entity| entity["kind"] == "residual-arc")
            .count(),
        1
    );
    assert!(
        focus
            .iter()
            .filter(|entity| entity["kind"] == "node")
            .count()
            <= 2
    );
    let mut active_arcs = frame["residual_arcs"]
        .as_array()
        .expect("residual arcs")
        .iter()
        .filter(|arc| arc["active"] == serde_json::Value::Bool(true))
        .map(|arc| {
            format!(
                "{}:{}",
                arc["edge_id"].as_str().expect("edge id"),
                arc["direction"].as_str().expect("direction")
            )
        })
        .collect::<Vec<_>>();
    active_arcs.sort();
    assert_eq!(active_arcs, expected_arcs);
    if expected_remaining == ("0", "0") {
        assert!(frame["node_trace_states"][2]["remaining_divergence"].is_null());
        assert!(frame["node_trace_states"][3]["remaining_divergence"].is_null());
    } else {
        assert_eq!(
            frame["node_trace_states"][2]["remaining_divergence"],
            expected_remaining.0
        );
        assert_eq!(
            frame["node_trace_states"][3]["remaining_divergence"],
            expected_remaining.1
        );
    }
    assert_eq!(frame["metrics"][3], expected_augmentations);
}

#[test]
fn blocking_primal_dual_dispatches_multiple_paths_in_one_price_phase() {
    let mut session =
        WasmSession::new(&blocking_primal_dual_scenario()).expect("blocking Scenario");
    let mut catalog_ids = Vec::new();
    let mut event_frames = Vec::new();
    while session
        .stage_next_json()
        .expect("blocking primal-dual trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("blocking primal-dual frame serializes"),
        )
        .expect("blocking primal-dual frame JSON");
        if let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() {
            let catalog_id = catalog_id.to_owned();
            catalog_ids.push(catalog_id.clone());
            event_frames.push((catalog_id, frame));
        }
    }
    for catalog_id in [
        "blocking-flow-primal-dual.initialize-dual",
        "blocking-flow-primal-dual.shortest-slack-labels",
        "blocking-flow-primal-dual.tighten-dual",
        "blocking-flow-primal-dual.build-admissible-levels",
        "blocking-flow-primal-dual.augment-admissible-path",
        "blocking-flow-primal-dual.complete-blocking-flow",
        "blocking-flow-primal-dual.optimal",
    ] {
        assert!(catalog_ids.iter().any(|value| value == catalog_id));
    }
    assert_eq!(
        catalog_ids
            .iter()
            .filter(|value| value.ends_with("augment-admissible-path"))
            .count(),
        2
    );
    assert_eq!(
        catalog_ids
            .iter()
            .filter(|value| value.ends_with("tighten-dual"))
            .count(),
        1
    );

    let slack = blocking_event_frame(
        &event_frames,
        "blocking-flow-primal-dual.shortest-slack-labels",
        0,
    );
    assert_blocking_node_labels(slack, &["0", "0", "0", "1"]);
    let prices = blocking_event_frame(&event_frames, "blocking-flow-primal-dual.tighten-dual", 0);
    assert_blocking_node_labels(prices, &["-2", "-1", "0", "1"]);
    let levels = blocking_event_frame(
        &event_frames,
        "blocking-flow-primal-dual.build-admissible-levels",
        1,
    );
    assert_blocking_node_labels(levels, &["1", "1", "0", "2"]);

    let first = blocking_event_frame(
        &event_frames,
        "blocking-flow-primal-dual.augment-admissible-path",
        0,
    );
    assert_blocking_augmentation(first, &["at:forward", "sa:forward"], ("2", "-2"), "1");
    let second = blocking_event_frame(
        &event_frames,
        "blocking-flow-primal-dual.augment-admissible-path",
        1,
    );
    assert_blocking_augmentation(second, &["bt:forward", "sb:forward"], ("0", "0"), "2");

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final blocking primal-dual scene serializes"),
    )
    .expect("final blocking primal-dual scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "4");
    assert_eq!(solved["metrics"][0], "2");
    assert_eq!(solved["metrics"][3], "2");
    assert_eq!(solved["metrics"][4], "1");
    assert_eq!(solved["metrics"][6], "1");
    assert_eq!(solved["metrics"][7], "1");
}

#[test]
fn blocking_primal_dual_fast_infeasible_and_resource_scenes_preserve_contracts() {
    let mut fast_scenario: serde_json::Value =
        serde_json::from_str(&blocking_primal_dual_scenario()).expect("scenario JSON");
    fast_scenario["payload"]["run_profile"] = serde_json::json!("fast");
    let mut fast = WasmSession::new(&fast_scenario.to_string()).expect("fast Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value = serde_json::from_str(
        &fast
            .current_frame_json()
            .expect("fast blocking scene serializes"),
    )
    .expect("fast blocking scene JSON");
    assert_eq!(fast_scene["solve_status"], "optimal");
    assert_eq!(fast_scene["outcome"]["total_cost"], "4");
    assert_eq!(fast_scene["metrics"][0], "2");
    assert_eq!(fast_scene["metrics"][3], "2");
    assert_eq!(fast_scene["metrics"][4], "1");
    assert_eq!(fast_scene["metrics"][6], "1");
    assert_eq!(fast_scene["metrics"][7], "1");
    assert_eq!(fast_scene["metrics"][15], "4");

    let mut infeasible_scenario: serde_json::Value =
        serde_json::from_str(&blocking_primal_dual_scenario()).expect("scenario JSON");
    infeasible_scenario["payload"]["model"]["required_flow"] = serde_json::json!("8");
    let mut infeasible =
        WasmSession::new(&infeasible_scenario.to_string()).expect("infeasible Scenario");
    while infeasible
        .stage_next_json()
        .expect("infeasible result stages")
        .is_some()
    {
        infeasible.commit_staged_next();
    }
    let infeasible_scene: serde_json::Value = serde_json::from_str(
        &infeasible
            .current_frame_json()
            .expect("infeasible blocking scene serializes"),
    )
    .expect("infeasible blocking scene JSON");
    assert_eq!(infeasible_scene["solve_status"], "infeasible");
    assert_eq!(infeasible_scene["outcome"]["kind"], "infeasible");
    assert_eq!(infeasible_scene["outcome"]["unsatisfied"], "1");

    let resource_session =
        WasmSession::new(&blocking_primal_dual_scenario()).expect("resource Scenario");
    let SessionKind::Flow(resource_session) = &resource_session.kind else {
        panic!("expected flow session");
    };
    let graph = resource_session
        .scenario
        .canonical_network()
        .expect("canonical network");
    let resource_frames = resource_session
        .blocking_primal_dual_error_frames(
            &graph,
            &[0, 0, 4, -4],
            BlockingPrimalDualError::WorkLimit,
        )
        .expect("resource scene");
    let resource_scene = serde_json::to_value(resource_frames.last().expect("resource frame"))
        .expect("resource frame JSON");
    assert_eq!(resource_scene["solve_status"], "resource-limit");
    assert!(resource_scene["outcome"].is_null());
}

#[test]
fn capacity_scaling_dispatches_scales_and_phase_boundary_saturation() {
    let mut session =
        WasmSession::new(&capacity_scaling_scenario()).expect("capacity-scaling Scenario");
    let mut catalog_ids = Vec::new();
    let mut scale_details = Vec::new();
    while session
        .stage_next_json()
        .expect("capacity-scaling trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("capacity-scaling frame serializes"),
        )
        .expect("capacity-scaling frame JSON");
        if let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() {
            catalog_ids.push(catalog_id.to_owned());
        }
        if frame["trace_event"]["detail"]["label"] == "scale" {
            scale_details.push(
                frame["trace_event"]["detail"]["value"]
                    .as_str()
                    .expect("scale is exact decimal")
                    .to_owned(),
            );
        }
    }
    for catalog_id in [
        "capacity-scaling-mcf.initialize-potentials",
        "capacity-scaling-mcf.start-scaling-phase",
        "capacity-scaling-mcf.shortest-eligible-path",
        "capacity-scaling-mcf.update-potentials",
        "capacity-scaling-mcf.augment",
        "capacity-scaling-mcf.saturate-negative-arc",
        "capacity-scaling-mcf.complete-scaling-phase",
        "capacity-scaling-mcf.optimal",
    ] {
        assert!(catalog_ids.iter().any(|value| value == catalog_id));
    }
    assert_eq!(scale_details, ["4", "4", "4", "2", "2", "1", "1"]);

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final capacity-scaling scene serializes"),
    )
    .expect("final capacity-scaling scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "40");
    assert_eq!(solved["metrics"][3], "1");
    assert_eq!(solved["metrics"][4], "1");
    assert_eq!(solved["metrics"][5], "3");
    assert_eq!(solved["metrics"][7], "1");
    assert_eq!(solved["metrics"][12], "1");
}

#[test]
fn excess_scaling_dispatches_exact_delta_transshipment_phases_and_fast_trace_parity() {
    let mut traced =
        WasmSession::new(&excess_scaling_scenario("trace")).expect("excess-scaling Scenario");
    let mut catalog_ids = Vec::new();
    let mut deltas = Vec::new();
    while traced
        .stage_next_json()
        .expect("excess-scaling event stages")
        .is_some()
    {
        traced.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &traced
                .current_frame_json()
                .expect("excess-scaling frame serializes"),
        )
        .expect("excess-scaling frame JSON");
        if let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() {
            catalog_ids.push(catalog_id.to_owned());
            if catalog_id == "excess-scaling-mcf.augment-exact-delta" {
                deltas.push(
                    frame["trace_event"]["detail"]["value"]
                        .as_str()
                        .expect("delta is exact decimal")
                        .to_owned(),
                );
            }
        }
    }
    for catalog_id in [
        "excess-scaling-mcf.initialize-potentials",
        "excess-scaling-mcf.start-excess-phase",
        "excess-scaling-mcf.shortest-large-excess-path",
        "excess-scaling-mcf.update-potentials",
        "excess-scaling-mcf.augment-exact-delta",
        "excess-scaling-mcf.complete-excess-phase",
        "excess-scaling-mcf.optimal",
    ] {
        assert!(catalog_ids.iter().any(|value| value == catalog_id));
    }
    assert_eq!(deltas, ["8", "4", "1"]);

    let trace_final: serde_json::Value = serde_json::from_str(
        &traced
            .current_frame_json()
            .expect("final trace scene serializes"),
    )
    .expect("final trace scene JSON");
    assert_eq!(trace_final["solve_status"], "optimal");
    assert_eq!(trace_final["outcome"]["total_cost"], "13");
    assert_eq!(trace_final["metrics"][3], "3");
    assert_eq!(trace_final["metrics"][5], "4");
    assert_eq!(trace_final["metrics"][12], "0");

    traced
        .seek_json(0)
        .expect("backward seek to ready boundary");
    let ready: serde_json::Value =
        serde_json::from_str(&traced.current_frame_json().expect("ready scene serializes"))
            .expect("ready scene JSON");
    assert_eq!(ready["solve_status"], "ready");

    let mut fast =
        WasmSession::new(&excess_scaling_scenario("fast")).expect("fast excess-scaling Scenario");
    while fast
        .stage_next_json()
        .expect("fast excess-scaling result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_final: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast scene serializes"))
            .expect("fast scene JSON");
    assert_eq!(fast_final["outcome"], trace_final["outcome"]);
    assert_eq!(fast_final["edges"], trace_final["edges"]);
    assert_eq!(fast_final["metrics"], trace_final["metrics"]);
}

#[test]
fn excess_scaling_rejects_binding_fixed_flow_capacities_before_ready_publication() {
    let mut binding: serde_json::Value =
        serde_json::from_str(&excess_scaling_scenario("trace")).expect("scenario JSON");
    binding["payload"]["graph"]["edges"][0]["capacity"] = serde_json::json!("12");
    assert_eq!(
        validate_flow_session_input(&binding.to_string())
            .expect_err("binding capacity must fail before a ready scene exists"),
        "selected flow algorithm requires every residual capacity width to cover the lower-adjusted required flow"
    );
    assert!(
        validate_flow_session_input(&excess_scaling_scenario("trace")).is_ok(),
        "the exact nonbinding boundary remains admissible"
    );
}

#[test]
fn excess_scaling_accepts_and_solves_a_nonbinding_lower_bound_self_loop() {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&excess_scaling_scenario("trace")).expect("scenario JSON");
    scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("edge declarations")
        .push(serde_json::json!({
            "id": "loop",
            "from": "s",
            "to": "s",
            "lower": "2",
            "capacity": "15",
            "cost": "0"
        }));
    let encoded = scenario.to_string();
    assert!(
        validate_flow_session_input(&encoded).is_ok(),
        "a self-loop lower bound contributes zero node divergence"
    );

    let mut session = WasmSession::new(&encoded).expect("self-loop Scenario reaches ready");
    while session
        .stage_next_json()
        .expect("self-loop trace stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("self-loop terminal scene serializes"),
    )
    .expect("self-loop terminal scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "13");
}

#[test]
fn excess_scaling_does_not_invent_reverse_capacity_at_a_lower_bound() {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&excess_scaling_scenario("trace")).expect("scenario JSON");
    scenario["payload"]["model"] = serde_json::json!({ "kind": "transshipment" });
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [
            { "id": "a", "supply": "1" },
            { "id": "b", "supply": "-1" }
        ],
        "edges": [
            {
                "id": "fixed", "from": "a", "to": "b",
                "lower": "1", "capacity": "1", "cost": "2"
            },
            {
                "id": "optional", "from": "a", "to": "b",
                "lower": "0", "capacity": "1", "cost": "1"
            }
        ]
    });
    let encoded = scenario.to_string();
    assert!(
        validate_flow_session_input(&encoded).is_ok(),
        "a lower-bound flow has no reverse residual capacity"
    );

    let mut session = WasmSession::new(&encoded).expect("lower-bound Scenario reaches ready");
    while session
        .stage_next_json()
        .expect("lower-bound trace stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("lower-bound terminal scene serializes"),
    )
    .expect("lower-bound terminal scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "2");
}

#[test]
fn excess_scaling_rejects_a_negative_residual_cycle_before_ready_publication() {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&excess_scaling_scenario("trace")).expect("scenario JSON");
    scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("edge declarations")
        .push(serde_json::json!({
            "id": "negative-return",
            "from": "t",
            "to": "s",
            "lower": "0",
            "capacity": "13",
            "cost": "-2"
        }));
    let encoded = scenario.to_string();
    let expected = "selected flow algorithm requires a lower-bound residual graph without negative-cost cycles";
    assert_eq!(
        validate_flow_session_input(&encoded)
            .expect_err("negative residual cycle must fail before ready publication"),
        expected
    );
}

#[test]
fn cost_scaling_dispatches_all_refinement_controls_and_catalog_identities() {
    for (algorithm, prefix, control_events) in [
        (
            "cost-scaling",
            "cost-scaling.",
            ["select-active-vertex", "relabel", "push"],
        ),
        (
            "cost-scaling-push-relabel",
            "cost-scaling-push-relabel.",
            ["select-active-vertex", "relabel", "push"],
        ),
        (
            "generalized-cost-scaling",
            "generalized-cost-scaling.",
            ["select-active-vertex", "relabel", "push"],
        ),
        (
            "augment-relabel",
            "augment-relabel.",
            ["select-active-root", "relabel-tip", "augment-to-deficit"],
        ),
        (
            "partial-augment-relabel-mcf",
            "partial-augment-relabel-mcf.",
            ["select-active-root", "relabel-tip", "augment-to-deficit"],
        ),
        (
            "price-refinement",
            "price-refinement.",
            [
                "start-potential-only-attempt",
                "relax-price",
                "succeed-without-flow-change",
            ],
        ),
    ] {
        let mut session =
            WasmSession::new(&cost_scaling_scenario(algorithm)).expect("cost-scaling Scenario");
        let mut catalog_ids = Vec::new();
        let mut epsilons = Vec::new();
        while session
            .stage_next_json()
            .expect("cost-scaling trace event stages")
            .is_some()
        {
            session.commit_staged_next();
            let frame: serde_json::Value = serde_json::from_str(
                &session
                    .current_frame_json()
                    .expect("cost-scaling frame serializes"),
            )
            .expect("cost-scaling frame JSON");
            if let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() {
                catalog_ids.push(catalog_id.to_owned());
            }
            if frame["trace_event"]["catalog_id"] == format!("{prefix}start-refine") {
                epsilons.push(
                    frame["trace_event"]["detail"]["value"]
                        .as_str()
                        .expect("epsilon is exact decimal")
                        .to_owned(),
                );
            }
        }
        for suffix in [
            "initialize-feasible-circulation",
            "start-refine",
            "saturate-negative-arc",
            "complete-refine",
            "optimal",
        ]
        .into_iter()
        .chain(control_events)
        {
            assert!(
                catalog_ids
                    .iter()
                    .any(|value| value == &format!("{prefix}{suffix}")),
                "missing {prefix}{suffix}"
            );
        }
        assert_eq!(epsilons, ["16", "8", "4", "2", "1"]);

        let solved: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("final cost-scaling scene serializes"),
        )
        .expect("final cost-scaling scene JSON");
        assert_eq!(solved["solve_status"], "optimal");
        assert_eq!(solved["outcome"]["total_cost"], "-7");
        assert_eq!(solved["metrics"][5], "5");
        assert_ne!(solved["metrics"][7], "0");
        assert_ne!(solved["metrics"][11], "0");
    }
}

#[test]
fn cost_scaling_variants_publish_the_real_feasibility_prefix_before_refinement() {
    for algorithm in [
        "cost-scaling",
        "cost-scaling-push-relabel",
        "generalized-cost-scaling",
        "augment-relabel",
        "partial-augment-relabel-mcf",
        "price-refinement",
        "arc-fixing",
    ] {
        let scenario = if algorithm == "arc-fixing" {
            arc_fixing_scenario("trace")
        } else {
            cost_scaling_scenario(algorithm)
        };
        let mut session = WasmSession::new(&scenario).expect("cost-scaling Scenario");
        let ready: serde_json::Value =
            serde_json::from_str(&session.current_frame_json().expect("Ready serializes"))
                .expect("Ready JSON");
        assert!(ready.get("feasibility_overlay").is_none());

        let mut ids = Vec::new();
        let mut previous_primary = 0_u128;
        let mut saw_artificial_topology = false;
        while session
            .stage_next_json()
            .expect("composed trace event stages")
            .is_some()
        {
            session.commit_staged_next();
            let frame: serde_json::Value = serde_json::from_str(
                &session
                    .current_frame_json()
                    .expect("composed frame serializes"),
            )
            .expect("composed frame JSON");
            let id = frame["trace_event"]["catalog_id"]
                .as_str()
                .expect("every nonzero frame has a catalog id")
                .to_owned();
            let primary = frame["metrics"][2]
                .as_str()
                .expect("primary metric decimal")
                .parse::<u128>()
                .expect("primary metric integer");
            assert!(
                primary >= previous_primary,
                "{algorithm} primary work decreased at {id}"
            );
            previous_primary = primary;
            if id.starts_with("feasibility.") {
                let overlay = frame
                    .get("feasibility_overlay")
                    .expect("feasibility event owns its auxiliary overlay");
                assert_eq!(overlay["use_kind"], "initial-flow");
                assert_eq!(
                    overlay["nodes"].as_array().map(Vec::len),
                    frame["graph"]["nodes"]
                        .as_array()
                        .map(|nodes| nodes.len() + 2)
                );
                let focused_arcs = overlay["arcs"]
                    .as_array()
                    .expect("auxiliary arcs")
                    .iter()
                    .filter(|arc| arc["focused"] == true)
                    .count();
                assert!(focused_arcs <= 1);
                saw_artificial_topology |= overlay["arcs"]
                    .as_array()
                    .expect("auxiliary arcs")
                    .iter()
                    .any(|arc| {
                        matches!(
                            arc["arc"]["kind"].as_str(),
                            Some("from-super-source" | "to-super-sink")
                        )
                    });
            } else {
                assert!(frame.get("feasibility_overlay").is_none());
            }
            ids.push(id);
        }
        assert_eq!(
            ids.first().map(String::as_str),
            Some("feasibility.add-original-arc")
        );
        let feasible = ids
            .iter()
            .position(|id| id == "feasibility.feasible")
            .expect("feasibility terminal boundary");
        let refine = ids
            .iter()
            .position(|id| id.ends_with(".initialize-feasible-circulation"))
            .expect("cost-scaling initialization boundary");
        assert!(feasible < refine);
        if algorithm != "arc-fixing" {
            assert!(saw_artificial_topology);
        }
    }
}

#[test]
fn arc_fixing_serializes_fixed_overlays_fix_in_metrics_and_reverse_seek() {
    let mut session = WasmSession::new(&arc_fixing_scenario("trace")).expect("Arc Fixing Scenario");
    let mut frame_index = 0_usize;
    let mut fixed_frame = None;
    let mut saw_fix_in = false;
    while session
        .stage_next_json()
        .expect("Arc Fixing trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        frame_index += 1;
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("Arc Fixing frame serializes"),
        )
        .expect("Arc Fixing frame JSON");
        let catalog_id = frame["trace_event"]["catalog_id"]
            .as_str()
            .unwrap_or_default();
        if catalog_id == "arc-fixing.update-fixed-set" && fixed_frame.is_none() {
            let fixed = frame["residual_arcs"]
                .as_array()
                .expect("residual array")
                .iter()
                .filter(|arc| arc["fixed"] == true)
                .count();
            if fixed >= 2 {
                fixed_frame = Some((frame_index, frame.clone()));
            }
        }
        if catalog_id == "arc-fixing.fix-in" {
            saw_fix_in = true;
            assert!(
                frame["residual_arcs"]
                    .as_array()
                    .expect("residual array")
                    .iter()
                    .any(|arc| arc["active"] == true && arc["fixed"] != true),
                "fix-in exposes the restored active residual direction"
            );
        }
    }

    assert!(saw_fix_in);
    let (fixed_index, fixed_scene) = fixed_frame.expect("fixed-set frame exists");
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final Arc Fixing scene serializes"),
    )
    .expect("final Arc Fixing scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "-2");
    for metric in [0_usize, 1, 3, 4, 8] {
        assert_ne!(solved["metrics"][metric], "0", "metric {metric}");
    }

    let base: serde_json::Value =
        serde_json::from_str(&session.seek_json(0).expect("seek to base")).expect("base JSON");
    assert!(
        base["residual_arcs"]
            .as_array()
            .expect("base residual array")
            .iter()
            .all(|arc| arc["fixed"] != true)
    );
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(fixed_index)
            .expect("seek back to fixed frame"),
    )
    .expect("replayed fixed JSON");
    assert_eq!(replayed, fixed_scene);

    let mut fast =
        WasmSession::new(&arc_fixing_scenario("fast")).expect("fast Arc Fixing Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value = serde_json::from_str(
        &fast
            .current_frame_json()
            .expect("fast Arc Fixing scene serializes"),
    )
    .expect("fast Arc Fixing scene JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert!(
        fast_scene["residual_arcs"]
            .as_array()
            .expect("fast residual array")
            .iter()
            .any(|arc| arc["fixed"] == true)
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn out_of_kilter_dispatches_selection_label_price_and_breakthrough_events() {
    let mut session =
        WasmSession::new(&cost_scaling_scenario("out-of-kilter")).expect("Out-of-Kilter Scenario");
    let mut catalog_ids = Vec::new();
    let mut scan_ordinals = Vec::new();
    let mut saw_delta_two_prices = false;
    let mut saw_breakthrough_cycle = false;
    while session
        .stage_next_json()
        .expect("Out-of-Kilter trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("Out-of-Kilter frame serializes"),
        )
        .expect("Out-of-Kilter frame JSON");
        if let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() {
            catalog_ids.push(catalog_id.to_owned());
            if matches!(
                catalog_id,
                "out-of-kilter.select-out-of-kilter-arc"
                    | "out-of-kilter.inspect-cut-arc"
                    | "out-of-kilter.modified-label-search"
                    | "out-of-kilter.raise-unlabeled-prices"
                    | "out-of-kilter.breakthrough"
            ) {
                let focus = frame["trace_event"]["entity_refs"]
                    .as_array()
                    .expect("Out-of-Kilter local focus");
                assert_eq!(
                    focus.len(),
                    1,
                    "the search/cycle overlay owns the complete set; local focus owns one residual arc"
                );
                assert_eq!(focus[0]["kind"], "residual-arc");
            }
            if matches!(
                catalog_id,
                "out-of-kilter.modified-label-search" | "out-of-kilter.inspect-cut-arc"
            ) {
                assert_eq!(frame["trace_event"]["detail"]["label"], "scan-ordinal");
                scan_ordinals.push(
                    frame["trace_event"]["detail"]["value"]
                        .as_str()
                        .expect("Out-of-Kilter scan ordinal")
                        .parse::<u128>()
                        .expect("canonical Out-of-Kilter scan ordinal"),
                );
            }
            if catalog_id == "out-of-kilter.raise-unlabeled-prices"
                && frame["trace_event"]["detail"]["value"] == "2"
            {
                let mut prices = frame["node_trace_states"]
                    .as_array()
                    .expect("price state array")
                    .iter()
                    .map(|node| {
                        (
                            node["node_id"].as_str().expect("node id").to_owned(),
                            node["label"].as_str().expect("price label").to_owned(),
                        )
                    })
                    .collect::<Vec<_>>();
                prices.sort();
                assert_eq!(
                    prices,
                    [("s", "0"), ("t", "2"), ("x", "2"), ("y", "2")]
                        .map(|(node, price)| (node.to_owned(), price.to_owned()))
                );
                saw_delta_two_prices = true;
            }
            if catalog_id == "out-of-kilter.breakthrough" {
                let mut active = frame["residual_arcs"]
                    .as_array()
                    .expect("residual array")
                    .iter()
                    .filter(|arc| arc["active"] == true)
                    .map(|arc| {
                        format!(
                            "{}:{}",
                            arc["edge_id"].as_str().expect("edge id"),
                            arc["direction"].as_str().expect("direction")
                        )
                    })
                    .collect::<Vec<_>>();
                active.sort();
                assert_eq!(active, ["xy:forward", "yx:forward"]);
                assert_eq!(frame["trace_event"]["detail"]["value"], "3");
                saw_breakthrough_cycle = true;
            }
        }
    }
    for catalog_id in [
        "out-of-kilter.initialize-feasible-circulation",
        "out-of-kilter.select-out-of-kilter-arc",
        "out-of-kilter.inspect-cut-arc",
        "out-of-kilter.modified-label-search",
        "out-of-kilter.raise-unlabeled-prices",
        "out-of-kilter.breakthrough",
        "out-of-kilter.optimal",
    ] {
        assert!(
            catalog_ids.iter().any(|value| value == catalog_id),
            "missing {catalog_id}"
        );
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final Out-of-Kilter scene serializes"),
    )
    .expect("final Out-of-Kilter scene JSON");
    assert!(saw_delta_two_prices);
    assert!(saw_breakthrough_cycle);
    assert_eq!(scan_ordinals, [1, 2, 3]);
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "-7");
    assert_eq!(
        solved["metrics"],
        serde_json::json!([
            "3", "0", "3", "1", "0", "0", "0", "2", "0", "0", "0", "0", "0", "0", "0", "2"
        ])
    );
}

#[test]
fn out_of_kilter_fast_profile_projects_its_dedicated_metrics() {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&cost_scaling_scenario("out-of-kilter")).expect("scenario JSON");
    scenario["payload"]["run_profile"] = serde_json::json!("fast");
    let mut session = WasmSession::new(&scenario.to_string()).expect("fast Scenario");
    while session
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("fast Out-of-Kilter scene serializes"),
    )
    .expect("fast Out-of-Kilter scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "-7");
    assert_eq!(
        solved["metrics"],
        serde_json::json!([
            "3", "0", "3", "1", "0", "0", "0", "2", "0", "0", "0", "0", "0", "0", "0", "2"
        ])
    );
}

fn assert_relaxation_price_frame(frame: &serde_json::Value) {
    assert_eq!(frame["trace_event"]["detail"]["label"], "delta");
    assert_eq!(frame["trace_event"]["detail"]["value"], "5");
    assert_eq!(
        frame["node_trace_states"],
        serde_json::json!([
            { "node_id": "s", "label": "0", "remaining_divergence": "-2" },
            {
                "node_id": "t",
                "label": "-5",
                "remaining_divergence": "2",
                "search_ordinal": 0
            }
        ])
    );
}

fn assert_relaxation_augmentation_frame(frame: &serde_json::Value) {
    assert_eq!(frame["trace_event"]["detail"]["label"], "delta");
    assert_eq!(frame["trace_event"]["detail"]["value"], "2");
    let active = frame["residual_arcs"]
        .as_array()
        .expect("residual arc array")
        .iter()
        .filter(|arc| arc["active"] == true)
        .map(|arc| {
            format!(
                "{}:{} {}->{}",
                arc["edge_id"].as_str().expect("edge id"),
                arc["direction"].as_str().expect("direction"),
                arc["from"].as_str().expect("from node"),
                arc["to"].as_str().expect("to node")
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(active, ["st:forward s->t"]);
    let order = frame["node_trace_states"]
        .as_array()
        .expect("node trace state array")
        .iter()
        .filter_map(|node| {
            node["search_ordinal"]
                .as_u64()
                .map(|ordinal| format!("{}#{ordinal}", node["node_id"].as_str().expect("node id")))
        })
        .collect::<Vec<_>>();
    assert_eq!(order, ["s#1", "t#0"]);
}

fn collect_relaxation_trace(session: &mut WasmSession) -> (Vec<String>, Vec<String>, Vec<u128>) {
    let mut catalog_ids = Vec::new();
    let mut ascent_slopes = Vec::new();
    let mut scan_ordinals = Vec::new();
    while session
        .stage_next_json()
        .expect("Relaxation trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("Relaxation frame serializes"),
        )
        .expect("Relaxation frame JSON");
        let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() else {
            continue;
        };
        if catalog_id.ends_with(".work-observation") {
            continue;
        }
        if !catalog_id.starts_with("relaxation.") {
            continue;
        }
        catalog_ids.push(catalog_id.to_owned());
        match catalog_id {
            "relaxation.scan-balanced-arcs"
            | "relaxation.scan-price-cut-arc"
            | "relaxation.scan-boundary-flow-arc" => {
                assert_eq!(frame["trace_event"]["detail"]["label"], "scan-ordinal");
                let focus = frame["trace_event"]["entity_refs"]
                    .as_array()
                    .expect("Relaxation scan focus");
                assert_eq!(focus.len(), 1, "one scan owns one graph primitive");
                assert!(matches!(
                    focus[0]["kind"].as_str(),
                    Some("edge" | "residual-arc")
                ));
                scan_ordinals.push(
                    frame["trace_event"]["detail"]["value"]
                        .as_str()
                        .expect("scan ordinal is exact decimal")
                        .parse::<u128>()
                        .expect("canonical scan ordinal"),
                );
            }
            "relaxation.evaluate-ascent-slope" => {
                ascent_slopes.push(
                    frame["trace_event"]["detail"]["value"]
                        .as_str()
                        .expect("ascent slope is exact decimal")
                        .to_owned(),
                );
            }
            "relaxation.adjust-prices" => assert_relaxation_price_frame(&frame),
            "relaxation.augment-balanced-path" => {
                assert_relaxation_augmentation_frame(&frame);
            }
            _ => {}
        }
    }
    (catalog_ids, ascent_slopes, scan_ordinals)
}

#[test]
fn relaxation_dispatches_price_adjustment_and_balanced_path_events() {
    let mut session =
        WasmSession::new(&relaxation_scenario()).expect("Relaxation Scenario is valid");
    let (catalog_ids, ascent_slopes, scan_ordinals) = collect_relaxation_trace(&mut session);
    assert_eq!(
        catalog_ids,
        [
            "relaxation.initialize-complementary-slack-state",
            "relaxation.select-positive-deficit",
            "relaxation.scan-balanced-arcs",
            "relaxation.scan-balanced-arcs",
            "relaxation.evaluate-ascent-slope",
            "relaxation.scan-price-cut-arc",
            "relaxation.scan-boundary-flow-arc",
            "relaxation.adjust-prices",
            "relaxation.select-positive-deficit",
            "relaxation.scan-balanced-arcs",
            "relaxation.evaluate-ascent-slope",
            "relaxation.augment-balanced-path",
            "relaxation.optimal",
        ]
    );
    assert_eq!(ascent_slopes, ["2", "-1"]);
    assert_eq!(scan_ordinals, [1, 2, 3, 4, 6]);
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final Relaxation scene serializes"),
    )
    .expect("final Relaxation scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "10");
    assert_eq!(
        solved["node_trace_states"],
        serde_json::json!([
            {
                "node_id": "s",
                "label": "0"
            },
            {
                "node_id": "t",
                "label": "-5"
            }
        ])
    );
    assert_eq!(
        solved["metrics"],
        serde_json::json!([
            "2", "0", "6", "1", "2", "0", "0", "1", "0", "2", "0", "0", "0", "0", "3", "2"
        ])
    );
}

#[test]
fn relaxation_fast_profile_projects_its_dedicated_metrics() {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&relaxation_scenario()).expect("scenario JSON");
    scenario["payload"]["run_profile"] = serde_json::json!("fast");
    let mut session = WasmSession::new(&scenario.to_string()).expect("fast Scenario");
    while session
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("fast Relaxation scene serializes"),
    )
    .expect("fast Relaxation scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "10");
    assert_eq!(
        solved["node_trace_states"],
        serde_json::json!([
            {
                "node_id": "s",
                "label": "0"
            },
            {
                "node_id": "t",
                "label": "-5"
            }
        ])
    );
    assert_eq!(
        solved["metrics"],
        serde_json::json!([
            "2", "0", "6", "1", "2", "0", "0", "1", "0", "2", "0", "0", "0", "0", "3", "2"
        ])
    );
}

#[test]
fn relaxation_trace_projection_limit_becomes_a_resource_limit_scene() {
    let mut session = WasmSession::new(&relaxation_trace_limit_scenario())
        .expect("long Relaxation Scenario is valid");
    while session
        .stage_next_json()
        .expect("resource result stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let limited: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("resource-limit scene serializes"),
    )
    .expect("resource-limit scene JSON");
    assert_eq!(limited["solve_status"], "resource-limit");
    assert!(limited["outcome"].is_null());
}

#[test]
fn epsilon_relaxation_dispatches_complete_up_iteration_and_scaled_prices() {
    let mut session = WasmSession::new(&epsilon_relaxation_scenario("trace"))
        .expect("Epsilon-Relaxation Scenario is valid");
    let mut catalog_ids = Vec::new();
    while session
        .stage_next_json()
        .expect("Epsilon-Relaxation event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("Epsilon-Relaxation frame serializes"),
        )
        .expect("Epsilon-Relaxation frame JSON");
        let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() else {
            continue;
        };
        if catalog_id.ends_with(".work-observation") {
            continue;
        }
        if !catalog_id.starts_with("epsilon-relaxation.") {
            continue;
        }
        catalog_ids.push(catalog_id.to_owned());
        match catalog_id {
            "epsilon-relaxation.scan-price-breakpoint" => {
                assert!(
                    frame["trace_event"]["detail"]["label"]
                        .as_str()
                        .is_some_and(|label| label.contains("candidate-price"))
                );
                assert_eq!(frame["trace_event"]["detail"]["value"], "16");
            }
            "epsilon-relaxation.raise-price" => {
                assert_eq!(frame["trace_event"]["detail"]["label"], "delta");
                assert_eq!(frame["trace_event"]["detail"]["value"], "16");
                assert_eq!(frame["node_trace_states"][0]["label"], "16");
                assert_eq!(frame["node_trace_states"][0]["remaining_divergence"], "2");
            }
            "epsilon-relaxation.push-admissible-arc" => {
                assert_eq!(frame["trace_event"]["detail"]["value"], "2");
                assert_eq!(
                    frame["residual_arcs"]
                        .as_array()
                        .expect("residual arcs")
                        .iter()
                        .filter(|arc| arc["active"] == true)
                        .map(|arc| arc["edge_id"].as_str().expect("edge id"))
                        .collect::<Vec<_>>(),
                    ["st"]
                );
            }
            _ => {}
        }
    }
    assert_eq!(
        catalog_ids,
        [
            "epsilon-relaxation.initialize-epsilon-cs-state",
            "epsilon-relaxation.select-positive-surplus",
            "epsilon-relaxation.scan-price-breakpoint",
            "epsilon-relaxation.raise-price",
            "epsilon-relaxation.push-admissible-arc",
            "epsilon-relaxation.complete-up-iteration",
            "epsilon-relaxation.optimal",
        ]
    );
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final Epsilon-Relaxation scene serializes"),
    )
    .expect("final Epsilon-Relaxation scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "10");
    assert_eq!(solved["node_trace_states"][0]["label"], "16");
    assert_eq!(
        solved["metrics"],
        serde_json::json!([
            "0", "1", "3", "1", "1", "0", "0", "1", "0", "2", "0", "1", "0", "1", "1", "1"
        ])
    );
}

#[test]
fn epsilon_relaxation_fast_profile_preserves_scaled_source_prices() {
    let mut session = WasmSession::new(&epsilon_relaxation_scenario("fast"))
        .expect("fast Epsilon-Relaxation Scenario is valid");
    while session
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("fast Epsilon-Relaxation scene serializes"),
    )
    .expect("fast Epsilon-Relaxation scene JSON");
    assert_eq!(solved["node_trace_states"][0]["label"], "16");
    assert_eq!(solved["node_trace_states"][1]["label"], "0");
    assert_eq!(solved["metrics"][2], "3");
    assert_eq!(solved["metrics"][11], "1");
}

#[test]
fn epsilon_relaxation_trace_projection_limit_is_a_resource_scene() {
    let mut session = WasmSession::new(&epsilon_relaxation_trace_limit_scenario())
        .expect("long Epsilon-Relaxation Scenario is valid");
    while session
        .stage_next_json()
        .expect("resource result stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let limited: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("resource-limit scene serializes"),
    )
    .expect("resource-limit scene JSON");
    assert_eq!(limited["solve_status"], "resource-limit");
    assert!(limited["outcome"].is_null());
}

#[test]
fn epsilon_relaxation_fast_work_limit_is_a_resource_scene() {
    let mut session = WasmSession::new(&epsilon_relaxation_work_limit_scenario())
        .expect("work-limited Epsilon-Relaxation Scenario is valid");
    while session
        .stage_next_json()
        .expect("resource result stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let limited: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("resource-limit scene serializes"),
    )
    .expect("resource-limit scene JSON");
    assert_eq!(limited["solve_status"], "resource-limit");
    assert!(limited["outcome"].is_null());
}

#[test]
fn prediction_assisted_epsilon_trace_exposes_clip_attempt_scale_and_abort_state() {
    let mut session = WasmSession::new(&prediction_assisted_epsilon_scenario("trace"))
        .expect("prediction-assisted Scenario is valid");
    let mut catalog_ids = Vec::new();
    let mut saw_clipped_prediction = false;
    let mut saw_active_push = false;
    while session
        .stage_next_json()
        .expect("prediction-assisted trace stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("prediction-assisted frame serializes"),
        )
        .expect("prediction-assisted frame JSON");
        let overlay = &frame["prediction_assisted_epsilon_overlay"];
        saw_clipped_prediction |= overlay["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node["prediction_clipped"] == true));
        if let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() {
            catalog_ids.push(catalog_id.to_owned());
            if catalog_id == "prediction-assisted-epsilon-relaxation.push-epsilon-balanced-arc" {
                assert!(overlay["active_node"].is_string());
                assert!(overlay["active_arc"]["edge_id"].is_string());
                saw_active_push = true;
            }
        }
    }
    assert!(saw_clipped_prediction);
    assert!(saw_active_push);
    assert!(
        catalog_ids
            .iter()
            .any(|id| { id == "prediction-assisted-epsilon-relaxation.begin-exponent-attempt" })
    );
    assert!(
        catalog_ids
            .iter()
            .any(|id| { id == "prediction-assisted-epsilon-relaxation.abort-exponent-attempt" })
    );
    assert!(
        catalog_ids.iter().any(|id| {
            id == "prediction-assisted-epsilon-relaxation.initialize-scaled-epsilon-cs"
        })
    );
    assert_eq!(
        catalog_ids.last().map(String::as_str),
        Some("prediction-assisted-epsilon-relaxation.certify-optimum")
    );
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("prediction-assisted optimum serializes"),
    )
    .expect("prediction-assisted optimum JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "3");
    assert_eq!(
        solved["prediction_assisted_epsilon_overlay"]["stage"],
        "optimal"
    );
    assert!(
        solved["prediction_assisted_epsilon_overlay"]["certificate_aligned_prediction_error"]
            .is_string()
    );
    assert!(
        solved["metrics"][0]
            .as_str()
            .is_some_and(|value| value != "0")
    );
    assert!(
        solved["metrics"][1]
            .as_str()
            .is_some_and(|value| value != "0")
    );
}

#[test]
fn prediction_assisted_epsilon_fast_uses_the_same_dedicated_overlay_and_certificate() {
    let mut session = WasmSession::new(&prediction_assisted_epsilon_scenario("fast"))
        .expect("prediction-assisted fast Scenario is valid");
    while session
        .stage_next_json()
        .expect("prediction-assisted fast result stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("prediction-assisted fast scene serializes"),
    )
    .expect("prediction-assisted fast JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "3");
    assert_eq!(
        solved["prediction_assisted_epsilon_overlay"]["scaling_parameter"],
        "2"
    );
    assert_eq!(
        solved["prediction_assisted_epsilon_overlay"]["nodes"][2]["raw_predicted_price"],
        "170141183460469231731687303715884105727"
    );
}

#[test]
fn prediction_assisted_epsilon_config_is_closed_and_node_complete() {
    let mut missing: serde_json::Value =
        serde_json::from_str(&prediction_assisted_epsilon_scenario("fast")).expect("scenario JSON");
    missing["payload"]["algorithm"]["config"]["predicted_potentials"]
        .as_object_mut()
        .expect("prediction object")
        .remove("a");
    let decoded = decode_flow_scenario(missing.to_string().as_bytes()).expect("valid envelope");
    let graph = decoded.canonical_network().expect("canonical graph");
    assert!(prediction_assisted_epsilon_config(&decoded, &graph).is_err());

    let mut extra: serde_json::Value =
        serde_json::from_str(&prediction_assisted_epsilon_scenario("fast")).expect("scenario JSON");
    extra["payload"]["algorithm"]["config"]["unexpected"] = serde_json::json!(true);
    let decoded = decode_flow_scenario(extra.to_string().as_bytes()).expect("valid envelope");
    let graph = decoded.canonical_network().expect("canonical graph");
    assert!(prediction_assisted_epsilon_config(&decoded, &graph).is_err());

    let mut noncanonical: serde_json::Value =
        serde_json::from_str(&prediction_assisted_epsilon_scenario("fast")).expect("scenario JSON");
    noncanonical["payload"]["algorithm"]["config"]["predicted_potentials"]["a"] =
        serde_json::json!("+2");
    let decoded =
        decode_flow_scenario(noncanonical.to_string().as_bytes()).expect("valid envelope");
    let graph = decoded.canonical_network().expect("canonical graph");
    assert!(prediction_assisted_epsilon_config(&decoded, &graph).is_err());
}

#[test]
fn tardos_framework_trace_exposes_exact_variable_fixing_boundaries() {
    let mut session = WasmSession::new(&tardos_framework_scenario("trace"))
        .expect("Tardos framework Scenario is valid");
    let mut frames = Vec::new();
    while session
        .stage_next_json()
        .expect("Tardos framework event stages")
        .is_some()
    {
        session.commit_staged_next();
        frames.push(
            serde_json::from_str::<serde_json::Value>(
                &session
                    .current_frame_json()
                    .expect("Tardos framework frame serializes"),
            )
            .expect("Tardos framework frame JSON"),
        );
    }
    let frames = frames
        .into_iter()
        .filter(|frame| {
            frame["trace_event"]["catalog_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("tardos-framework."))
        })
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 8);
    assert_eq!(
        frames
            .iter()
            .map(|frame| frame["trace_event"]["catalog_id"]
                .as_str()
                .expect("catalog id"))
            .collect::<Vec<_>>(),
        [
            "tardos-framework.construct-feasible-flow",
            "tardos-framework.scan-residual-arc",
            "tardos-framework.scan-residual-arc",
            "tardos-framework.scan-residual-arc",
            "tardos-framework.measure-epsilon",
            "tardos-framework.inspect-fixed-variable",
            "tardos-framework.classify-fixed-variables",
            "tardos-framework.complete-primitive",
        ]
    );
    assert_eq!(
        frames[4]["tardos_framework_overlay"]["stage"],
        "measure-epsilon"
    );
    assert_eq!(frames[4]["tardos_framework_overlay"]["epsilon"], "1");
    assert_eq!(frames[4]["tardos_framework_overlay"]["threshold"], "3");
    assert!(
        frames[4]["tardos_framework_overlay"]["fixed_variables"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert_eq!(
        frames[6]["tardos_framework_overlay"]["fixed_variables"],
        serde_json::json!([{
            "edge_id": "expensive",
            "bound": "lower",
            "value": "0",
            "direction": "forward",
            "reduced_cost": "20"
        }])
    );
    let solved = frames.last().expect("complete primitive frame");
    assert_eq!(solved["solve_status"], "primitive-complete");
    assert_eq!(solved["outcome"]["kind"], "tardos-framework");
    assert_eq!(solved["outcome"]["epsilon"], "1");
    assert_eq!(solved["outcome"]["threshold"], "3");
    assert_eq!(solved["outcome"]["determinant_bound"], "1");
    assert_eq!(solved["metrics"][0], "1");
    assert_eq!(solved["metrics"][2], "3");
    assert_eq!(solved["metrics"][3], "1");
    assert_eq!(solved["metrics"][15], "8");

    let measurement_cursor = frames[4]["event_id"]
        .as_str()
        .and_then(|value| value.parse::<usize>().ok())
        .expect("measurement boundary has a canonical absolute cursor");
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(measurement_cursor)
            .expect("measurement boundary seeks"),
    )
    .expect("measurement replay JSON");
    assert_eq!(replayed, frames[4]);
}

#[test]
fn tardos_framework_fast_matches_trace_certificate_flow_and_metrics() {
    let mut traced =
        WasmSession::new(&tardos_framework_scenario("trace")).expect("Tardos trace Scenario");
    while traced
        .stage_next_json()
        .expect("Tardos trace result stages")
        .is_some()
    {
        traced.commit_staged_next();
    }
    let trace_scene: serde_json::Value = serde_json::from_str(
        &traced
            .current_frame_json()
            .expect("Tardos trace result serializes"),
    )
    .expect("Tardos trace result JSON");

    let mut fast =
        WasmSession::new(&tardos_framework_scenario("fast")).expect("Tardos fast Scenario");
    while fast
        .stage_next_json()
        .expect("Tardos fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value = serde_json::from_str(
        &fast
            .current_frame_json()
            .expect("Tardos fast result serializes"),
    )
    .expect("Tardos fast result JSON");
    assert_eq!(fast_scene["solve_status"], "primitive-complete");
    assert_eq!(fast_scene["edge_states"], trace_scene["edge_states"]);
    assert_eq!(fast_scene["outcome"], trace_scene["outcome"]);
    assert_eq!(fast_scene["metrics"], trace_scene["metrics"]);
    assert_eq!(
        fast_scene["tardos_framework_overlay"],
        trace_scene["tardos_framework_overlay"]
    );
}

#[test]
fn tardos_framework_config_is_closed_canonical_and_node_complete() {
    for mutation in ["missing", "extra", "noncanonical"] {
        let mut value: serde_json::Value =
            serde_json::from_str(&tardos_framework_scenario("fast")).expect("scenario JSON");
        match mutation {
            "missing" => {
                value["payload"]["algorithm"]["config"]["potentials"]
                    .as_object_mut()
                    .expect("potentials")
                    .remove("a");
            }
            "extra" => {
                value["payload"]["algorithm"]["config"]["unexpected"] = serde_json::json!(true);
            }
            "noncanonical" => {
                value["payload"]["algorithm"]["config"]["potentials"]["a"] =
                    serde_json::json!("+0");
            }
            _ => unreachable!(),
        }
        let decoded = decode_flow_scenario(value.to_string().as_bytes()).expect("valid envelope");
        let graph = decoded.canonical_network().expect("canonical graph");
        assert!(tardos_framework_config(&decoded, &graph).is_err());
    }
}

#[test]
fn primal_network_simplex_dispatches_pricing_cycles_and_degenerate_basis_pivots() {
    let mut session = WasmSession::new(&cost_scaling_scenario("primal-network-simplex"))
        .expect("network-simplex Scenario");
    let mut catalog_ids = Vec::new();
    let mut saw_tree_overlay = false;
    while session
        .stage_next_json()
        .expect("network-simplex trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("network-simplex frame serializes"),
        )
        .expect("network-simplex frame JSON");
        if let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() {
            catalog_ids.push(catalog_id.to_owned());
        }
        saw_tree_overlay |= frame["pseudoflow_forest"]["arcs"]
            .as_array()
            .is_some_and(|arcs| !arcs.is_empty());
    }
    for catalog_id in [
        "primal-network-simplex.initialize-artificial-basis",
        "primal-network-simplex.price-block",
        "primal-network-simplex.form-basic-cycle",
        "primal-network-simplex.exchange-basis",
        "primal-network-simplex.optimal",
    ] {
        assert!(
            catalog_ids.iter().any(|value| value == catalog_id),
            "missing {catalog_id}"
        );
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final network-simplex scene serializes"),
    )
    .expect("final network-simplex scene JSON");
    assert!(saw_tree_overlay);
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "-7");
    assert_ne!(solved["metrics"][3], "0");
    assert_ne!(solved["metrics"][4], "0");
    assert_ne!(solved["metrics"][11], "0");
    assert_ne!(solved["metrics"][13], "0");
}

#[test]
fn dynamic_tree_network_simplex_dispatches_reversible_directional_tree_pivots() {
    let source = cost_scaling_scenario("dynamic-tree-network-simplex");
    let mut session = WasmSession::new(&source).expect("dynamic-tree network-simplex Scenario");
    let mut query = None;
    let mut saw_exchange = false;
    let mut saw_original_tree_arc = false;
    while session
        .stage_next_json()
        .expect("dynamic-tree network-simplex trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("dynamic-tree network-simplex frame serializes"),
        )
        .expect("dynamic-tree network-simplex frame JSON");
        saw_original_tree_arc |= frame["pseudoflow_forest"]["arcs"]
            .as_array()
            .is_some_and(|arcs| !arcs.is_empty());
        match frame["trace_event"]["catalog_id"].as_str() {
            Some("dynamic-tree-network-simplex.query-cycle-minimum") => {
                assert!(
                    frame["residual_arcs"]
                        .as_array()
                        .is_some_and(|arcs| arcs.iter().any(|arc| arc["active"] == true))
                );
                query.get_or_insert((
                    session.event_cursor().parse::<usize>().expect("cursor"),
                    frame,
                ));
            }
            Some("dynamic-tree-network-simplex.cut-link-basis") => saw_exchange = true,
            _ => {}
        }
    }
    assert!(saw_exchange && saw_original_tree_arc);
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("dynamic-tree network-simplex result serializes"),
    )
    .expect("dynamic-tree network-simplex result JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "-7");
    for slot in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 13, 14, 15] {
        assert_ne!(solved["metrics"][slot], "0", "metric slot {slot}");
    }

    let (query_cursor, query_scene) = query.expect("cycle-minimum event exists");
    let base: serde_json::Value =
        serde_json::from_str(&session.seek_json(0).expect("base seek")).expect("base JSON");
    assert!(base["pseudoflow_forest"].is_null());
    let replayed: serde_json::Value =
        serde_json::from_str(&session.seek_json(query_cursor).expect("cycle-minimum seek"))
            .expect("replayed query JSON");
    assert_eq!(replayed, query_scene);

    let mut fast_source: serde_json::Value = serde_json::from_str(&source).expect("scenario");
    fast_source["payload"]["run_profile"] = serde_json::json!("fast");
    let mut fast = WasmSession::new(&fast_source.to_string()).expect("fast Scenario");
    while fast
        .stage_next_json()
        .expect("fast dynamic-tree network-simplex result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast result JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
}

#[test]
fn simple_cycle_canceling_dispatches_a_disconnected_negative_cycle() {
    let mut session = WasmSession::new(&simple_cycle_canceling_scenario())
        .expect("simple cycle canceling Scenario");
    let found = commit_until_catalog(&mut session, "simple-cycle-canceling.find-negative-cycle");
    assert_eq!(
        found["trace_event"]["catalog_id"],
        "simple-cycle-canceling.find-negative-cycle"
    );
    assert_eq!(found["trace_event"]["detail"]["label"], "cycle-cost");
    assert_eq!(found["trace_event"]["detail"]["value"], "-3");
    assert_eq!(
        found["residual_arcs"]
            .as_array()
            .expect("residual arcs")
            .iter()
            .filter(|arc| arc["active"] == true)
            .count(),
        2
    );

    let canceled: serde_json::Value =
        serde_json::from_str(&commit_next(&mut session)).expect("cycle cancellation frame");
    assert_eq!(
        canceled["trace_event"]["catalog_id"],
        "simple-cycle-canceling.cancel-negative-cycle"
    );
    assert_eq!(canceled["trace_event"]["detail"]["label"], "delta");
    assert_eq!(canceled["trace_event"]["detail"]["value"], "3");

    let solved = commit_until_catalog(&mut session, "simple-cycle-canceling.optimal");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "-7");
    assert_eq!(solved["metrics"][3], "1");
    assert_eq!(solved["metrics"][4], "2");
    assert_eq!(session.cursor(), session.item_count());
}

#[test]
fn minimum_mean_cycle_canceling_dispatches_the_smallest_average_cost_first() {
    let mut session = WasmSession::new(&minimum_mean_cycle_canceling_scenario())
        .expect("minimum-mean cycle canceling Scenario");
    let selected = commit_until_catalog(
        &mut session,
        "minimum-mean-cycle-canceling.select-minimum-mean-cycle",
    );
    assert_eq!(
        selected["trace_event"]["catalog_id"],
        "minimum-mean-cycle-canceling.select-minimum-mean-cycle"
    );
    assert_eq!(selected["trace_event"]["detail"]["label"], "cycle-cost");
    assert_eq!(selected["trace_event"]["detail"]["value"], "-3");
    assert_eq!(
        selected["residual_arcs"]
            .as_array()
            .expect("residual arcs")
            .iter()
            .filter(|arc| arc["active"] == true)
            .count(),
        1
    );

    let canceled: serde_json::Value =
        serde_json::from_str(&commit_next(&mut session)).expect("first cancellation");
    assert_eq!(
        canceled["trace_event"]["catalog_id"],
        "minimum-mean-cycle-canceling.cancel-minimum-mean-cycle"
    );
    assert_eq!(canceled["trace_event"]["detail"]["value"], "1");
    let second = commit_until_catalog(
        &mut session,
        "minimum-mean-cycle-canceling.select-minimum-mean-cycle",
    );
    assert_eq!(second["trace_event"]["detail"]["value"], "-5");
    assert_eq!(
        second["residual_arcs"]
            .as_array()
            .expect("residual arcs")
            .iter()
            .filter(|arc| arc["active"] == true)
            .count(),
        2
    );

    while session.cursor() < session.item_count() {
        let _ = commit_next(&mut session);
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("minimum-mean result serializes"),
    )
    .expect("minimum-mean result is JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "-6");
    assert_eq!(solved["metrics"][3], "2");
    assert_eq!(solved["metrics"][4], "3");
}

#[test]
#[allow(clippy::too_many_lines)]
fn cancel_and_tighten_dispatches_exact_prices_dag_ranks_and_reversible_cycles() {
    let source = cancel_tighten_scenario();
    let mut session = WasmSession::new(&source).expect("Cancel-and-Tighten Scenario");
    let initialized = commit_until_catalog(&mut session, "cancel-and-tighten.initialize");
    assert_eq!(initialized["cancel_tighten_overlay"]["stage"], "initialize");
    assert_eq!(
        initialized["cancel_tighten_overlay"]["epsilon"]["numerator"],
        "4"
    );
    assert_eq!(
        initialized["cancel_tighten_overlay"]["epsilon"]["denominator"],
        "1"
    );

    let mut selected = None;
    let mut tightened = None;
    let mut inspected = None;
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("trace frame JSON");
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("cancel-and-tighten.select-admissible-cycle") => {
                selected = Some((session.cursor(), scene));
            }
            Some("cancel-and-tighten.tighten") => {
                tightened.get_or_insert(scene);
            }
            Some("cancel-and-tighten.inspect-cycle-residual-arc") => {
                inspected.get_or_insert(scene);
            }
            _ => {}
        }
    }
    let inspected = inspected.expect("cycle-search inspection exists");
    assert_eq!(
        inspected["cancel_tighten_overlay"]["stage"],
        "inspect-cycle-arc"
    );
    assert_eq!(
        inspected["cancel_tighten_overlay"]["inspected_arcs"]
            .as_array()
            .expect("inspected residual operand")
            .len(),
        1
    );
    assert_eq!(
        inspected["trace_event"]["entity_refs"]
            .as_array()
            .expect("inspection focus")
            .len(),
        1
    );
    assert_eq!(inspected["trace_event"]["minimum_granularity"], "micro");
    let (selected_cursor, selected_scene) = selected.expect("cycle selection exists");
    assert_eq!(
        selected_scene["cancel_tighten_overlay"]["active_cycle"]
            .as_array()
            .expect("active cycle")
            .len(),
        3
    );
    assert_eq!(
        selected_scene["trace_event"]["entity_refs"]
            .as_array()
            .expect("cycle selection focus")
            .len(),
        1,
        "the cycle overlay owns the whole cycle; local focus owns one anchor arc"
    );
    assert!(
        selected_scene["cancel_tighten_overlay"]["admissible_arcs"]
            .as_array()
            .expect("admissible arcs")
            .len()
            >= 3
    );
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(selected_cursor)
            .expect("seek selected cycle"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, selected_scene);

    let tightened = tightened.expect("tighten boundary exists");
    assert_eq!(
        tightened["trace_event"]["entity_refs"]
            .as_array()
            .expect("tighten focus")
            .len(),
        1,
        "the node overlay owns all ranks; local focus owns one extremal rank"
    );
    assert_eq!(
        tightened["cancel_tighten_overlay"]["epsilon"]["numerator"],
        "16"
    );
    assert_eq!(
        tightened["cancel_tighten_overlay"]["epsilon"]["denominator"],
        "5"
    );
    let ranks = tightened["cancel_tighten_overlay"]["nodes"]
        .as_array()
        .expect("node states")
        .iter()
        .map(|node| node["rank"].as_str().expect("rank"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ranks.len(), 5);

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final Cancel-and-Tighten frame"),
    )
    .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "-4");
    assert_eq!(solved["cancel_tighten_overlay"]["stage"], "optimal");
    assert_ne!(solved["metrics"][2], "0");
    assert_eq!(solved["metrics"][3], "1");
    assert_ne!(solved["metrics"][4], "0");
    assert_ne!(solved["metrics"][5], "0");
    assert_eq!(solved["metrics"][5], "6");
    assert_eq!(solved["metrics"][7], "6");

    let mut fast_source: serde_json::Value = serde_json::from_str(&source).expect("scenario");
    fast_source["payload"]["run_profile"] = serde_json::json!("fast");
    let mut fast = WasmSession::new(&fast_source.to_string()).expect("fast Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value = serde_json::from_str(
        &fast
            .current_frame_json()
            .expect("fast Cancel-and-Tighten result serializes"),
    )
    .expect("fast result JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["cancel_tighten_overlay"],
        solved["cancel_tighten_overlay"]
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn relaxed_mndc_dispatches_exact_assignment_duals_and_two_disjoint_cycles() {
    let source = relaxed_mndc_scenario("trace");
    let mut session = WasmSession::new(&source).expect("relaxed-MNDC Scenario");
    let mut phase = None;
    let mut selected = None;
    let mut canceled = None;
    let mut residual_inspection = None;
    let mut cell_inspection = None;
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("trace frame JSON");
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("relaxed-most-negative-cycle.begin-phase") => {
                phase.get_or_insert(scene);
            }
            Some("relaxed-most-negative-cycle.select-family") => {
                if scene["relaxed_mndc_overlay"]["family"]
                    .as_array()
                    .is_some_and(|family| family.len() == 2)
                {
                    selected = Some((session.cursor(), scene));
                }
            }
            Some("relaxed-most-negative-cycle.cancel-family") => {
                if scene["relaxed_mndc_overlay"]["family"]
                    .as_array()
                    .is_some_and(|family| family.len() == 2)
                {
                    canceled = Some(scene);
                }
            }
            Some("relaxed-most-negative-cycle.inspect-residual-arc") => {
                residual_inspection.get_or_insert(scene);
            }
            Some("relaxed-most-negative-cycle.inspect-assignment-cell") => {
                cell_inspection.get_or_insert(scene);
            }
            _ => {}
        }
    }
    let phase = phase.expect("epsilon phase exists");
    assert_eq!(phase["relaxed_mndc_overlay"]["epsilon"]["numerator"], "2");
    assert_eq!(phase["relaxed_mndc_overlay"]["epsilon"]["denominator"], "1");

    let residual_inspection = residual_inspection.expect("residual inspection exists");
    assert_eq!(
        residual_inspection["relaxed_mndc_overlay"]["stage"],
        "inspect-residual-arc"
    );
    assert_eq!(
        residual_inspection["relaxed_mndc_overlay"]["inspected_arcs"]
            .as_array()
            .expect("concrete residual operand")
            .len(),
        1
    );
    assert!(
        residual_inspection["relaxed_mndc_overlay"]["nodes"]
            .as_array()
            .expect("identity assignment rows")
            .iter()
            .all(|node| node.get("selected_arc").is_none()),
        "a scan boundary must not reveal the completed assignment early"
    );
    let cell_inspection = cell_inspection.expect("assignment-cell inspection exists");
    assert_eq!(
        cell_inspection["relaxed_mndc_overlay"]["stage"],
        "inspect-assignment-cell"
    );
    assert!(cell_inspection["relaxed_mndc_overlay"]["active_assignment_cell"].is_object());
    assert!(
        cell_inspection["relaxed_mndc_overlay"]["nodes"]
            .as_array()
            .expect("identity assignment rows")
            .iter()
            .all(|node| node.get("selected_arc").is_none()),
        "a cell scan must focus only its source-kernel row and column"
    );

    let (selected_cursor, selected_scene) = selected.expect("two-cycle family exists");
    assert_eq!(
        selected_scene["trace_event"]["entity_refs"]
            .as_array()
            .expect("selected family focus"),
        &[serde_json::json!({
            "kind": "residual-arc",
            "edge_id": "ab",
            "direction": "forward",
        })]
    );
    let nodes = selected_scene["relaxed_mndc_overlay"]["nodes"]
        .as_array()
        .expect("assignment nodes");
    assert_eq!(nodes.len(), 6);
    assert_eq!(
        nodes
            .iter()
            .filter(|node| node.get("selected_arc").is_some())
            .count(),
        4
    );
    assert!(nodes.iter().all(|node| {
        node["left_dual"].as_str().is_some() && node["right_dual"].as_str().is_some()
    }));
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(selected_cursor)
            .expect("seek selected assignment"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, selected_scene);

    let canceled = canceled.expect("family cancellation exists");
    assert_eq!(
        canceled["trace_event"]["entity_refs"]
            .as_array()
            .expect("canceled family focus")
            .len(),
        1,
        "the family remains in the typed overlay; Detail focus owns one bottleneck arc"
    );
    let deltas = canceled["relaxed_mndc_overlay"]["family"]
        .as_array()
        .expect("cycles")
        .iter()
        .map(|cycle| cycle["delta"].as_str().expect("delta"))
        .collect::<BTreeSet<_>>();
    assert_eq!(deltas, BTreeSet::from(["2", "3"]));

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final relaxed-MNDC frame"),
    )
    .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "-15");
    assert_eq!(solved["relaxed_mndc_overlay"]["stage"], "optimal");
    assert_ne!(solved["metrics"][0], "0");
    assert_eq!(solved["metrics"][3], "2");
    assert_eq!(solved["metrics"][4], "1");

    let mut fast =
        WasmSession::new(&relaxed_mndc_scenario("fast")).expect("fast relaxed-MNDC Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["relaxed_mndc_overlay"],
        solved["relaxed_mndc_overlay"]
    );
}

#[test]
fn relaxed_mndc_admission_failure_is_public_and_staging_is_reversible() {
    let mut session = WasmSession::new(&relaxed_mndc_oversized_scenario())
        .expect("oversized relaxed-MNDC Scenario");
    let ready = session
        .current_frame_json()
        .expect("ready boundary serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let limited_value: serde_json::Value =
        serde_json::from_str(&limited).expect("resource boundary JSON");
    assert_eq!(limited_value["solve_status"], "resource-limit");
    assert_eq!(
        session
            .current_frame_json()
            .expect("ready still serializes"),
        ready
    );

    session
        .discard_staged_next()
        .expect("resource candidate can be discarded");
    assert_eq!(
        session.current_frame_json().expect("ready after discard"),
        ready
    );
    assert_eq!(
        session
            .stage_next_json()
            .expect("resource boundary restages")
            .expect("resource boundary exists again"),
        limited
    );
    session.commit_staged_next();
    assert_eq!(
        session
            .current_frame_json()
            .expect("committed limit serializes"),
        limited
    );
}

fn assert_enhanced_capacity_scaling_path_scene(selected: &serde_json::Value) {
    let overlay = &selected["enhanced_capacity_scaling_overlay"];
    assert_eq!(overlay["stage"], "select-path");
    assert_eq!(overlay["delta"]["numerator"], "5");
    assert_eq!(overlay["delta"]["denominator"], "1");
    assert_eq!(overlay["source_component"], "a");
    assert!(overlay["sink_component"].as_str().is_some());
    assert!(
        !overlay["path"]
            .as_array()
            .expect("selected path")
            .is_empty()
    );
    assert!(overlay["nodes"].as_array().is_some_and(|nodes| {
        nodes.iter().all(|node| {
            node["component_id"].as_str().is_some() && node["potential"].as_str().is_some()
        })
    }));
    assert!(overlay["edges"].as_array().is_some_and(|edges| {
        edges.iter().all(|edge| {
            edge["virtual_flow"]["denominator"] == "1"
                && edge["reduced_cost"].as_str().is_some()
                && edge["tight"].as_bool().is_some()
        })
    }));
}

fn assert_enhanced_capacity_scaling_contraction_scene(contracted: &serde_json::Value) {
    let overlay = &contracted["enhanced_capacity_scaling_overlay"];
    assert_eq!(overlay["stage"], "contract");
    assert!(overlay["contraction_arc"].as_str().is_some());
    assert!(
        overlay["components"]
            .as_array()
            .is_some_and(|components| components.len() < 3)
    );
}

fn assert_dual_simplex_initialized_scene(scene: &serde_json::Value) {
    let overlay = &scene["dual_network_simplex_overlay"];
    assert_eq!(overlay["stage"], "initialize-dual-tree");
    let edges = overlay["edges"].as_array().expect("edge overlay");
    assert_eq!(
        edges.iter().filter(|edge| edge["in_tree"] == true).count(),
        2
    );
    assert!(
        edges
            .iter()
            .all(|edge| edge["reduced_cost"].as_str().is_some())
    );
}

fn assert_dual_simplex_leaving_scene(scene: &serde_json::Value) {
    let overlay = &scene["dual_network_simplex_overlay"];
    let leaving_id = overlay["leaving_edge"]
        .as_str()
        .expect("leaving edge identity");
    assert!(
        overlay["edges"]
            .as_array()
            .expect("edge overlay")
            .iter()
            .any(|edge| edge["edge_id"] == leaving_id
                && edge["basic_flow"]
                    .as_str()
                    .is_some_and(|flow| flow.starts_with('-')))
    );
    assert!(
        !overlay["cut_side"]
            .as_array()
            .expect("head-side cut")
            .is_empty()
    );
}

fn assert_dual_simplex_pivot_scene(scene: &serde_json::Value) {
    let overlay = &scene["dual_network_simplex_overlay"];
    assert_eq!(overlay["stage"], "pivot");
    assert_eq!(
        overlay["edges"]
            .as_array()
            .expect("pivot edge overlay")
            .iter()
            .filter(|edge| edge["in_tree"] == true)
            .count(),
        2
    );
}

#[test]
fn enhanced_capacity_scaling_rational_projection_is_canonical_and_extreme_safe() {
    let zero = normalized_unsigned_rational(0, 8).expect("zero rational");
    assert_eq!(
        (zero.numerator.as_str(), zero.denominator.as_str()),
        ("0", "1")
    );

    let reduced = normalized_unsigned_rational(6, 8).expect("reducible rational");
    assert_eq!(
        (reduced.numerator.as_str(), reduced.denominator.as_str()),
        ("3", "4")
    );

    let minimum = normalized_signed_rational(i128::MIN, 1).expect("minimum i128 rational");
    assert_eq!(minimum.numerator, i128::MIN.to_string());
    assert_eq!(minimum.denominator, "1");
}

#[test]
fn enhanced_capacity_scaling_dispatches_exact_quotient_contraction_trace() {
    let source = enhanced_capacity_scaling_scenario("trace");
    let mut session = WasmSession::new(&source).expect("enhanced capacity scaling Scenario");
    let mut selected = None;
    let mut augmented = None;
    let mut contracted = None;
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("trace frame JSON");
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("enhanced-capacity-scaling.select-path") => {
                selected.get_or_insert((session.cursor(), scene));
            }
            Some("enhanced-capacity-scaling.augment") => {
                augmented.get_or_insert(scene);
            }
            Some("enhanced-capacity-scaling.contract") => {
                contracted.get_or_insert(scene);
            }
            _ => {}
        }
    }

    let (selected_cursor, selected) = selected.expect("quotient path selection exists");
    assert_enhanced_capacity_scaling_path_scene(&selected);
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(selected_cursor)
            .expect("seek selected quotient path"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, selected);

    let augmented = augmented.expect("exact delta augmentation exists");
    assert_eq!(
        augmented["enhanced_capacity_scaling_overlay"]["augmentation"]["numerator"],
        augmented["enhanced_capacity_scaling_overlay"]["delta"]["numerator"]
    );
    assert_eq!(
        augmented["enhanced_capacity_scaling_overlay"]["augmentation"]["denominator"],
        augmented["enhanced_capacity_scaling_overlay"]["delta"]["denominator"]
    );

    let contracted = contracted.expect("strongly feasible contraction exists");
    assert_enhanced_capacity_scaling_contraction_scene(&contracted);

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final enhanced capacity scaling frame"),
    )
    .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "0");
    assert_eq!(
        solved["enhanced_capacity_scaling_overlay"]["stage"],
        "optimal"
    );
    assert_ne!(solved["metrics"][0], "0");
    assert_ne!(solved["metrics"][3], "0");
    assert_ne!(solved["metrics"][5], "0");

    let mut fast = WasmSession::new(&enhanced_capacity_scaling_scenario("fast"))
        .expect("fast enhanced capacity scaling Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["enhanced_capacity_scaling_overlay"],
        solved["enhanced_capacity_scaling_overlay"]
    );
}

#[test]
fn transformed_feasibility_recovery_publishes_its_exact_internal_domain() {
    for (scenario, expected_kind) in [
        (
            enhanced_capacity_scaling_scenario("trace"),
            "node-aligned-transformation",
        ),
        (orlin_mcf_scenario("trace"), "standalone-transformation"),
        (convex_cost_scenario(), "node-aligned-transformation"),
    ] {
        let mut session = WasmSession::new(&scenario).expect("transformed feasibility scenario");
        let mut recovery = None;
        while session.cursor() < session.item_count() {
            let scene: serde_json::Value =
                serde_json::from_str(&commit_next(&mut session)).expect("trace frame JSON");
            if scene["feasibility_overlay"]["use_kind"] == "anchored-recovery" {
                recovery.get_or_insert(scene);
            }
        }
        let recovery = recovery.expect("anchored feasibility recovery is published");
        let overlay = &recovery["feasibility_overlay"];
        let overlay_fields = recovery
            .as_object()
            .expect("recovery scene object")
            .keys()
            .filter(|field| field.ends_with("_overlay"))
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(
            overlay_fields,
            vec!["feasibility_overlay"],
            "an auxiliary boundary must not retain a stale parent overlay"
        );
        assert!(recovery.get("pseudoflow_forest").is_none());
        assert_eq!(overlay["revision"], "flow-feasibility-overlay/2");
        assert_eq!(overlay["domain"]["kind"], expected_kind);
        assert_eq!(overlay["domain"]["request"]["kind"], "balance");
        let domain_edges = overlay["domain"]["edges"]
            .as_array()
            .expect("exact feasibility input edges");
        let published_arcs = overlay["arcs"]
            .as_array()
            .expect("feasibility construction prefix");
        for arc in published_arcs
            .iter()
            .filter(|arc| arc["arc"]["kind"] == "original")
        {
            assert!(domain_edges.iter().any(|edge| {
                edge["edge_id"] == arc["arc"]["original_edge_id"]
                    && edge["from_node_id"] == arc["from"]["original_node_id"]
                    && edge["to_node_id"] == arc["to"]["original_node_id"]
            }));
        }
        if expected_kind == "standalone-transformation" {
            assert!(overlay["domain"]["nodes"].as_array().is_some_and(|nodes| {
                nodes
                    .iter()
                    .any(|node| node.get("public_node_id").is_none())
            }));
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn orlin_mcf_dispatches_capacity_nodes_compressed_paths_and_fast_equivalence() {
    let mut session = WasmSession::new(&orlin_mcf_scenario("trace")).expect("Orlin MCF Scenario");
    let ready: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("Orlin MCF ready frame serializes"),
    )
    .expect("Orlin MCF ready frame JSON");
    assert!(ready.get("orlin_mcf_overlay").is_none());
    assert_eq!(ready["metrics"][0], "0");
    let mut transformed = None;
    let mut selected = None;
    let mut augmented = None;
    let mut inspection_stages = std::collections::BTreeSet::new();
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("Orlin MCF frame JSON");
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("orlin-mcf.transform-capacities") => transformed = Some(scene),
            Some("orlin-mcf.select-compressed-path") => {
                selected.get_or_insert((session.cursor(), scene));
            }
            Some("orlin-mcf.augment") => {
                augmented.get_or_insert(scene);
            }
            Some(catalog_id) if catalog_id.starts_with("orlin-mcf.inspect-") => {
                let stage = scene["orlin_mcf_overlay"]["stage"]
                    .as_str()
                    .expect("inspection stage");
                assert_eq!(catalog_id, format!("orlin-mcf.{stage}"));
                let inspected = scene["orlin_mcf_overlay"]["inspected_segment"]
                    .as_array()
                    .expect("transformed inspection segment");
                assert!((1..=2).contains(&inspected.len()));
                assert_eq!(
                    scene["orlin_mcf_overlay"]["inspection_serial"],
                    scene["trace_event"]["detail"]["value"]
                );
                assert!(
                    scene["trace_event"]["entity_refs"]
                        .as_array()
                        .is_some_and(|entities| !entities.is_empty()),
                    "Orlin MCF inspection must identify its residual segment"
                );
                inspection_stages.insert(stage.to_owned());
            }
            _ => {}
        }
    }
    assert_eq!(
        inspection_stages,
        [
            "inspect-compressed-arc".to_owned(),
            "inspect-compressed-residual-arc".to_owned(),
            "inspect-contractible-arc".to_owned(),
            "inspect-reachability-arc".to_owned(),
        ]
        .into_iter()
        .collect()
    );
    let transformed = transformed.expect("capacity transform boundary");
    let overlay = &transformed["orlin_mcf_overlay"];
    assert_eq!(overlay["stage"], "transform-capacities");
    assert_eq!(transformed["metrics"][0], "3");
    assert_eq!(overlay["nodes"].as_array().map(Vec::len), Some(6));
    assert_eq!(overlay["arcs"].as_array().map(Vec::len), Some(6));
    assert_eq!(
        overlay["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .filter(|node| node["kind"] == "capacity")
            .count(),
        3
    );
    let (selected_cursor, selected) = selected.expect("compressed path boundary");
    assert_eq!(
        selected["orlin_mcf_overlay"]["stage"],
        "select-compressed-path"
    );
    assert!(
        selected["orlin_mcf_overlay"]["path"]
            .as_array()
            .is_some_and(|path| !path.is_empty())
    );
    assert!(
        selected["orlin_mcf_overlay"]["path"]
            .as_array()
            .expect("path")
            .iter()
            .all(|arc| arc["branch"] == "flow" || arc["branch"] == "slack")
    );
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(selected_cursor)
            .expect("seek compressed path"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, selected);
    let augmented = augmented.expect("exact delta augmentation");
    assert_eq!(
        augmented["orlin_mcf_overlay"]["augmentation"],
        augmented["orlin_mcf_overlay"]["delta"]
    );
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final Orlin MCF frame"),
    )
    .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "9");
    assert_eq!(solved["orlin_mcf_overlay"]["stage"], "optimal");
    assert_eq!(solved["metrics"][0], "3");
    assert_ne!(solved["metrics"][4], "0");
    assert_ne!(solved["metrics"][7], "0");

    let mut fast = WasmSession::new(&orlin_mcf_scenario("fast")).expect("fast Orlin MCF Scenario");
    while fast
        .stage_next_json()
        .expect("fast Orlin MCF result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(fast_scene["orlin_mcf_overlay"], solved["orlin_mcf_overlay"]);
}

#[test]
#[allow(clippy::too_many_lines)]
fn primal_dual_ipm_mcf_dispatches_integer_minor_cycles_crossover_and_fast_parity() {
    let mut session =
        WasmSession::new(&primal_dual_ipm_mcf_scenario("trace")).expect("primal-dual IPM Scenario");
    let ready: serde_json::Value =
        serde_json::from_str(&session.current_frame_json().expect("ready serializes"))
            .expect("ready JSON");
    assert!(ready["primal_dual_ipm_mcf_overlay"].is_null());

    let mut reduction = None;
    let mut forest_inspection = None;
    let mut forest_inspections_avoid_generic_focus = true;
    let mut forest_subset_serials = Vec::new();
    let mut forest = None;
    let mut sampled = None;
    let mut cycle = None;
    let mut crossover = None;
    let mut recovery = None;
    while session.cursor() < session.item_count() {
        let cursor = session.cursor() + 1;
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("integer IPM frame JSON");
        let stage = scene["primal_dual_ipm_mcf_overlay"]["stage"].as_str();
        let catalog_id = scene["trace_event"]["catalog_id"].as_str();
        match (stage, catalog_id) {
            (Some("build-capacity-reduction"), _) => {
                reduction.get_or_insert(scene);
            }
            (
                Some("inspect-forest-subset"),
                Some("primal-dual-interior-point-mcf.inspect-forest-subset"),
            ) => {
                let overlay = &scene["primal_dual_ipm_mcf_overlay"];
                let serial = overlay["forest_subset_serial"]
                    .as_str()
                    .and_then(|value| value.parse::<u64>().ok())
                    .expect("forest subset serial");
                assert_eq!(
                    serial,
                    u64::try_from(forest_subset_serials.len()).expect("subset count") + 1
                );
                let candidate_arcs = overlay["arcs"]
                    .as_array()
                    .expect("auxiliary arcs")
                    .iter()
                    .filter(|arc| arc["forest_candidate"] == true)
                    .collect::<Vec<_>>();
                assert!(candidate_arcs.iter().all(|arc| arc["in_minor"] == true));
                assert_eq!(
                    scene["trace_event"]["detail"]["value"]
                        .as_str()
                        .and_then(|value| value.parse::<usize>().ok()),
                    Some(candidate_arcs.len())
                );
                forest_subset_serials.push(serial);
                forest_inspections_avoid_generic_focus &= scene["trace_event"]["entity_refs"]
                    .as_array()
                    .is_some_and(Vec::is_empty);
                forest_inspection.get_or_insert(scene);
            }
            (
                Some("build-low-stretch-forest"),
                Some("primal-dual-interior-point-mcf.build-low-stretch-forest"),
            ) => {
                forest.get_or_insert(scene);
            }
            (Some("sample-fundamental-cycle"), _) => {
                sampled.get_or_insert((cursor, scene));
            }
            (Some("centering-cycle-update"), _) => {
                cycle.get_or_insert(scene);
            }
            (Some("crossover-grow-cut"), _) => {
                crossover.get_or_insert(scene);
            }
            (Some("recover-admissible-flow"), _) => recovery = Some(scene),
            _ => {}
        }
    }

    let forest_inspection = forest_inspection.expect("forest-subset inspection boundary");
    assert!(!forest_subset_serials.is_empty());
    assert_eq!(
        forest_inspection["trace_event"]["detail"]["label"],
        "candidate subset arcs"
    );
    assert!(
        forest_inspection["trace_event"]["detail"]["value"]
            .as_str()
            .and_then(|value| value.parse::<usize>().ok())
            .is_some()
    );
    assert!(forest_inspections_avoid_generic_focus);

    let reduction = reduction.expect("capacity reduction boundary");
    let reduction_overlay = &reduction["primal_dual_ipm_mcf_overlay"];
    assert_eq!(reduction_overlay["nodes"].as_array().map(Vec::len), Some(6));
    assert_eq!(reduction_overlay["arcs"].as_array().map(Vec::len), Some(9));
    assert_eq!(
        reduction_overlay["nodes"]
            .as_array()
            .expect("auxiliary nodes")
            .iter()
            .filter(|node| node["kind"] == "capacity")
            .count(),
        3
    );
    let reduction_arcs = reduction_overlay["arcs"]
        .as_array()
        .expect("auxiliary arcs");
    for kind in ["upper", "lower", "artificial"] {
        assert_eq!(
            reduction_arcs
                .iter()
                .filter(|arc| arc["kind"] == kind)
                .count(),
            3
        );
    }

    let forest = forest.expect("minimum-condition forest boundary");
    assert!(
        forest["primal_dual_ipm_mcf_overlay"]["tree_condition_number"]["denominator"]
            .as_str()
            .and_then(|value| value.parse::<u128>().ok())
            .is_some_and(|value| value > 0)
    );
    assert!(
        forest["primal_dual_ipm_mcf_overlay"]["arcs"]
            .as_array()
            .is_some_and(|arcs| arcs.iter().any(|arc| arc["in_tree"] == true))
    );

    let (sampled_cursor, sampled) = sampled.expect("weighted cycle sample boundary");
    assert!(sampled["primal_dual_ipm_mcf_overlay"]["sampled_arc"].is_string());
    assert_eq!(sampled["trace_event"]["entity_refs"], serde_json::json!([]));
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(sampled_cursor)
            .expect("seek sampled cycle boundary"),
    )
    .expect("replayed integer IPM JSON");
    assert_eq!(replayed, sampled);

    let cycle = cycle.expect("rounded cycle boundary");
    assert_eq!(cycle["trace_event"]["entity_refs"], serde_json::json!([]));
    assert!(
        cycle["primal_dual_ipm_mcf_overlay"]["arcs"]
            .as_array()
            .is_some_and(|arcs| arcs.iter().any(|arc| arc["active_cycle_sign"] != "0"))
    );
    let crossover = crossover.expect("nested crossover boundary");
    assert!(
        crossover["primal_dual_ipm_mcf_overlay"]["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node["in_crossover_set"] == true))
    );
    let recovery = recovery.expect("admissible recovery boundary");
    assert!(
        recovery["edge_states"]
            .as_array()
            .is_some_and(|edges| edges.iter().any(|edge| edge["flow"] != "0"))
    );

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final integer IPM frame"),
    )
    .expect("final integer IPM JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "9");
    assert_eq!(solved["primal_dual_ipm_mcf_overlay"]["stage"], "optimal");
    assert_ne!(solved["metrics"][1], "0");
    assert_ne!(solved["metrics"][8], "0");
    assert_ne!(solved["metrics"][10], "0");

    let mut fast = WasmSession::new(&primal_dual_ipm_mcf_scenario("fast"))
        .expect("fast primal-dual IPM Scenario");
    while fast
        .stage_next_json()
        .expect("fast integer IPM result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast integer IPM JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["primal_dual_ipm_mcf_overlay"],
        solved["primal_dual_ipm_mcf_overlay"]
    );
}

#[test]
fn primal_dual_ipm_mcf_admission_failure_is_public_reversible_and_repeatable() {
    let mut session = WasmSession::new(&primal_dual_ipm_mcf_oversized_scenario())
        .expect("oversized primal-dual IPM Scenario");
    let ready = session.current_frame_json().expect("ready serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let value: serde_json::Value = serde_json::from_str(&limited).expect("resource JSON");
    assert_eq!(value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("discard resource candidate");
    assert_eq!(
        session
            .stage_next_json()
            .expect("restage")
            .expect("boundary"),
        limited
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn electrical_ipm_mcf_dispatches_isolation_newton_iteration_rounding_and_fast_parity() {
    let mut session = WasmSession::new(&electrical_ipm_mcf_scenario("trace"))
        .expect("electrical IPM MCF Scenario");
    let ready: serde_json::Value =
        serde_json::from_str(&session.current_frame_json().expect("ready serializes"))
            .expect("ready JSON");
    assert!(ready["electrical_ipm_mcf_overlay"].is_null());

    let mut isolated = None;
    let mut centering_iteration = None;
    let mut rounded = None;
    while session.cursor() < session.item_count() {
        let cursor = session.cursor() + 1;
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("electrical IPM frame JSON");
        assert_shared_trace_detail_is_finite_decimal(&scene);
        match scene["electrical_ipm_mcf_overlay"]["stage"].as_str() {
            Some("select-isolated-costs") => {
                isolated.get_or_insert(scene);
            }
            Some("damped-centering-step") => {
                centering_iteration.get_or_insert((cursor, scene));
            }
            Some("round-nearest-integer") => {
                rounded.get_or_insert(scene);
            }
            _ => {}
        }
    }

    let isolated = isolated.expect("isolated optimum boundary");
    let isolated_overlay = &isolated["electrical_ipm_mcf_overlay"];
    assert!(
        isolated_overlay["isolated_gap"]
            .as_str()
            .and_then(|value| value.parse::<i128>().ok())
            .is_some_and(|value| value > 0)
    );
    assert!(
        isolated_overlay["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().all(|edge| {
                edge["perturbation"]
                    .as_str()
                    .and_then(|value| value.parse::<u64>().ok())
                    .is_some_and(|value| value > 0)
                    && edge["isolated_cost"].as_str().is_some()
            }))
    );

    let (centering_cursor, centering_iteration) =
        centering_iteration.expect("Newton centering iteration boundary");
    assert!(
        centering_iteration["electrical_ipm_mcf_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().all(|edge| {
                edge["fixed_on_face"] == true
                    || (edge["resistance"]
                        .as_str()
                        .and_then(|value| value.parse::<f64>().ok())
                        .is_some_and(|value| value > 0.0)
                        && edge["conductance"]
                            .as_str()
                            .and_then(|value| value.parse::<f64>().ok())
                            .is_some_and(|value| value > 0.0))
            }))
    );
    assert!(
        centering_iteration["electrical_ipm_mcf_overlay"]["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node["anchored"] == true))
    );
    assert!(
        centering_iteration["electrical_ipm_mcf_overlay"]["electrical_energy"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value >= 0.0)
    );
    assert!(
        centering_iteration["electrical_ipm_mcf_overlay"]["step_size"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value > 0.0 && value <= 1.0)
    );
    assert_eq!(
        centering_iteration["trace_event"]["catalog_id"],
        "electrical-flow-interior-point-mcf.newton-centering-iteration"
    );
    assert_eq!(
        centering_iteration["trace_event"]["detail"]["label"],
        "centrality residual"
    );
    assert_eq!(
        centering_iteration["trace_event"]["detail"]["value"],
        centering_iteration["electrical_ipm_mcf_overlay"]["centrality_residual"]
    );
    assert_eq!(
        centering_iteration["metrics"][5],
        centering_iteration["metrics"][6]
    );
    assert_eq!(
        centering_iteration["metrics"][6],
        centering_iteration["metrics"][8]
    );
    assert_ne!(centering_iteration["metrics"][6], "0");
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(centering_cursor)
            .expect("seek Newton centering iteration boundary"),
    )
    .expect("replayed electrical IPM JSON");
    assert_eq!(replayed, centering_iteration);
    let rounded = rounded.expect("nearest-integer boundary");
    assert!(
        rounded["electrical_ipm_mcf_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().all(|edge| edge["final_flow"].is_string()))
    );

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final electrical IPM frame"),
    )
    .expect("final electrical IPM JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "9");
    assert_eq!(solved["electrical_ipm_mcf_overlay"]["stage"], "optimal");
    assert_ne!(solved["metrics"][2], "0");
    assert_ne!(solved["metrics"][6], "0");
    assert_ne!(solved["metrics"][9], "0");
    assert_ne!(solved["metrics"][12], "0");

    let mut fast = WasmSession::new(&electrical_ipm_mcf_scenario("fast"))
        .expect("fast electrical IPM MCF Scenario");
    while fast
        .stage_next_json()
        .expect("fast electrical IPM result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast electrical IPM JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["electrical_ipm_mcf_overlay"],
        solved["electrical_ipm_mcf_overlay"]
    );
}

#[test]
fn electrical_ipm_mcf_admission_failure_is_public_reversible_and_repeatable() {
    let mut session = WasmSession::new(&electrical_ipm_mcf_oversized_scenario())
        .expect("oversized electrical IPM MCF Scenario");
    let ready = session.current_frame_json().expect("ready serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let value: serde_json::Value = serde_json::from_str(&limited).expect("resource JSON");
    assert_eq!(value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("discard resource candidate");
    assert_eq!(
        session
            .stage_next_json()
            .expect("restage")
            .expect("boundary"),
        limited
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn minimum_ratio_cycle_mcf_dispatches_source_step_and_fast_parity() {
    let mut session = WasmSession::new(&minimum_ratio_cycle_mcf_scenario("trace"))
        .expect("minimum-ratio-cycle MCF Scenario");
    let ready: serde_json::Value =
        serde_json::from_str(&session.current_frame_json().expect("ready serializes"))
            .expect("ready JSON");
    assert!(ready["minimum_ratio_cycle_mcf_overlay"].is_null());

    let mut mapped = None;
    let mut inspected = None;
    let mut candidate = None;
    let mut applied = None;
    let mut measured = None;
    while session.cursor() < session.item_count() {
        let cursor = session.cursor() + 1;
        let scene: serde_json::Value = serde_json::from_str(&commit_next(&mut session))
            .expect("minimum-ratio-cycle MCF frame JSON");
        match scene["minimum_ratio_cycle_mcf_overlay"]["stage"].as_str() {
            Some("map-gradient-length") => {
                mapped.get_or_insert(scene);
            }
            Some("inspect-vector") => {
                inspected.get_or_insert(scene);
            }
            Some("evaluate-cycle") => {
                candidate.get_or_insert(scene);
            }
            Some("apply-source-step") => {
                applied.get_or_insert(scene);
            }
            Some("measure-potential-decrease") => {
                measured.get_or_insert((cursor, scene));
            }
            _ => {}
        }
    }
    let mapped = mapped.expect("source gradient/length boundary");
    assert!(
        mapped["minimum_ratio_cycle_mcf_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().all(|edge| {
                edge["fixed_on_face"] == true
                    || edge["length"]
                        .as_str()
                        .and_then(|value| value.parse::<f64>().ok())
                        .is_some_and(|value| value > 0.0)
            }))
    );
    let inspected = inspected.expect("signed-vector checkpoint boundary");
    assert_eq!(
        inspected["trace_event"]["entity_refs"],
        serde_json::json!([])
    );
    assert!(
        inspected["minimum_ratio_cycle_mcf_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().any(|edge| edge["candidate_sign"] != "0"))
    );
    let candidate = candidate.expect("candidate boundary");
    assert_eq!(
        candidate["trace_event"]["entity_refs"],
        serde_json::json!([])
    );
    assert!(
        candidate["minimum_ratio_cycle_mcf_overlay"]["candidate_ratio"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value <= 0.0)
    );
    assert!(
        candidate["minimum_ratio_cycle_mcf_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().any(|edge| edge["candidate_sign"] != "0"))
    );
    let applied = applied.expect("source step boundary");
    assert!(
        applied["minimum_ratio_cycle_mcf_overlay"]["eta"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value > 0.0)
    );
    assert!(
        applied["minimum_ratio_cycle_mcf_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges
                .iter()
                .any(|edge| { edge["initial_flow"] != edge["updated_flow"] }))
    );
    let (measured_cursor, measured) = measured.expect("potential decrease boundary");
    let measured_overlay = &measured["minimum_ratio_cycle_mcf_overlay"];
    let decrease = measured_overlay["potential_decrease"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .expect("measured decrease");
    let guaranteed = measured_overlay["guaranteed_decrease"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .expect("guaranteed decrease");
    assert!(decrease > 0.0 && decrease >= guaranteed);
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(measured_cursor)
            .expect("seek measured boundary"),
    )
    .expect("replayed measured JSON");
    assert_eq!(replayed, measured);

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final progress frame"),
    )
    .expect("final progress JSON");
    assert_eq!(solved["solve_status"], "primitive-complete");
    assert_eq!(solved["outcome"]["kind"], "minimum-ratio-cycle-mcf");
    assert_eq!(
        solved["minimum_ratio_cycle_mcf_overlay"]["stage"],
        "complete"
    );
    assert_ne!(solved["metrics"][0], "0");
    assert_ne!(solved["metrics"][5], "0");
    assert_eq!(solved["metrics"][9], "1");

    let mut fast = WasmSession::new(&minimum_ratio_cycle_mcf_scenario("fast"))
        .expect("fast minimum-ratio-cycle MCF Scenario");
    while fast
        .stage_next_json()
        .expect("fast progress result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast progress JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["minimum_ratio_cycle_mcf_overlay"],
        solved["minimum_ratio_cycle_mcf_overlay"]
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn randomized_almost_linear_mcf_dispatches_isolation_detect_recovery_and_fast_parity() {
    let mut session = WasmSession::new(&randomized_almost_linear_mcf_scenario("trace"))
        .expect("randomized almost-linear MCF scenario");
    let mut scenes = Vec::new();
    while let Some(frame) = session
        .stage_next_json()
        .expect("randomized MCF trace stages")
    {
        scenes.push(serde_json::from_str::<serde_json::Value>(&frame).expect("scene JSON"));
        session.commit_staged_next();
    }
    let stage = |name: &str| {
        scenes
            .iter()
            .find(|scene| scene["randomized_almost_linear_mcf_overlay"]["stage"] == name)
    };
    let isolated = stage("sample-isolation-costs").expect("isolation boundary");
    assert_eq!(
        isolated["randomized_almost_linear_mcf_overlay"]["isolation_attempt"],
        "1"
    );
    assert!(
        isolated["randomized_almost_linear_mcf_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().all(|edge| edge["isolation_draw"] != "0"))
    );
    let isolated_optimum = stage("select-isolated-optimum").expect("isolated optimum boundary");
    assert!(
        isolated_optimum["randomized_almost_linear_mcf_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges
                .iter()
                .all(|edge| edge["isolated_optimum_flow"].is_string()))
    );
    let assignment = stage("inspect-feasible-assignment").expect("assignment boundary");
    assert!(assignment["randomized_almost_linear_mcf_overlay"]["assignment_cursor"].is_string());
    assert_eq!(
        assignment["randomized_almost_linear_mcf_overlay"]["assignment_serial"],
        assignment["trace_event"]["detail"]["value"]
    );
    assert_eq!(
        assignment["trace_event"]["entity_refs"],
        serde_json::json!([])
    );
    let oracle_vector = stage("inspect-oracle-vector").expect("oracle vector boundary");
    assert_eq!(
        oracle_vector["randomized_almost_linear_mcf_overlay"]["oracle_vector_serial"],
        oracle_vector["trace_event"]["detail"]["value"]
    );
    assert_eq!(
        oracle_vector["trace_event"]["entity_refs"],
        serde_json::json!([])
    );
    assert!(
        oracle_vector["randomized_almost_linear_mcf_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().any(|edge| edge["candidate_sign"] != "0"))
    );
    let tree = stage("sample-tree-chain").expect("sampled tree-chain boundary");
    assert!(tree["randomized_almost_linear_mcf_overlay"]["sampled_forest_index"].is_string());
    let detect = stage("detect-changed-coordinates").expect("Detect boundary");
    assert_ne!(
        detect["randomized_almost_linear_mcf_overlay"]["detected_coordinates"],
        serde_json::Value::Null
    );
    let final_point = stage("construct-final-point").expect("final-point boundary");
    let final_point_overlay = &final_point["randomized_almost_linear_mcf_overlay"];
    let gap_numerator = final_point_overlay["final_point_gap"]["numerator"]
        .as_str()
        .and_then(|value| value.parse::<i128>().ok())
        .expect("gap numerator");
    let gap_denominator = final_point_overlay["final_point_gap"]["denominator"]
        .as_str()
        .and_then(|value| value.parse::<i128>().ok())
        .expect("gap denominator");
    let threshold_numerator = final_point_overlay["final_point_threshold"]["numerator"]
        .as_str()
        .and_then(|value| value.parse::<i128>().ok())
        .expect("threshold numerator");
    let threshold_denominator = final_point_overlay["final_point_threshold"]["denominator"]
        .as_str()
        .and_then(|value| value.parse::<i128>().ok())
        .expect("threshold denominator");
    assert!(gap_numerator * threshold_denominator <= threshold_numerator * gap_denominator);
    assert!(
        final_point_overlay["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().all(|edge| {
                edge["final_point_flow"].is_object() && edge["final_flow"].is_null()
            }))
    );
    let rounded = stage("round-nearest-integer").expect("rounding boundary");
    assert!(
        rounded["randomized_almost_linear_mcf_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().all(|edge| edge["final_flow"].is_string()))
    );
    let solved = scenes.last().expect("terminal scene");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "min-cost-flow");
    assert_eq!(solved["outcome"]["total_cost"], "9");
    assert_eq!(
        solved["randomized_almost_linear_mcf_overlay"]["stage"],
        "optimal"
    );
    assert_eq!(
        solved["randomized_almost_linear_mcf_overlay"]["exact_recovery"],
        true
    );

    let mut fast = WasmSession::new(&randomized_almost_linear_mcf_scenario("fast"))
        .expect("fast randomized MCF scenario");
    while fast
        .stage_next_json()
        .expect("fast randomized MCF stage")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast scene serializes"))
            .expect("fast scene JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(
        fast_scene["randomized_almost_linear_mcf_overlay"],
        solved["randomized_almost_linear_mcf_overlay"]
    );
}

fn assert_flow_framework_rational_digit_band(scene: &serde_json::Value) {
    for field in [
        "accepted_ratio",
        "target_progress",
        "exact_gap_before",
        "exact_gap_after",
        "stopping_gap",
    ] {
        let rational = &scene["flow_framework_mcf_overlay"][field];
        assert!(
            rational["numerator"]
                .as_str()
                .is_some_and(|value| value.trim_start_matches('-').len() <= 1_234)
        );
        assert!(
            rational["denominator"]
                .as_str()
                .is_some_and(|value| value.len() <= 1_234)
        );
    }
}

fn assert_deterministic_mcf_progress(scenes: &[serde_json::Value]) {
    let stage = |name: &str| {
        scenes
            .iter()
            .find(|scene| scene["flow_framework_mcf_overlay"]["stage"] == name)
    };
    let initial = stage("initialize-source-point").expect("source initial-point boundary");
    assert_eq!(
        initial["flow_framework_mcf_overlay"]["levels"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert!(stage("periodic-reinitialize").is_some());
    assert!(stage("detect").is_some());
    let query = stage("query-minimum-ratio-cycle").expect("ratio-cycle boundary");
    assert_eq!(query["trace_event"]["entity_refs"], serde_json::json!([]));
    let query_overlay = &query["flow_framework_mcf_overlay"];
    assert_ne!(query_overlay["accepted_ratio"]["numerator"], "0");
    assert_ne!(query_overlay["target_progress"]["numerator"], "0");
    let selected = query_overlay["edges"]
        .as_array()
        .expect("query edge projection");
    assert_eq!(
        selected
            .iter()
            .filter(|edge| edge["selected"] == true)
            .count(),
        3
    );
    assert_eq!(selected[0]["cycle_coefficient"]["numerator"], "1");
    assert_eq!(selected[1]["cycle_coefficient"]["numerator"], "1");
    assert_eq!(selected[2]["cycle_coefficient"]["numerator"], "-1");
    let progress = stage("source-progress").expect("source progress boundary");
    assert_eq!(
        progress["trace_event"]["entity_refs"],
        serde_json::json!([])
    );
    let progress_overlay = &progress["flow_framework_mcf_overlay"];
    let gap_before = progress_overlay["gap_before"]
        .as_str()
        .expect("gap before")
        .parse::<f64>()
        .expect("finite gap before");
    let gap_after = progress_overlay["gap_after"]
        .as_str()
        .expect("gap after")
        .parse::<f64>()
        .expect("finite gap after");
    assert!(gap_after < gap_before);

    let dynamic_boundaries = scenes
        .iter()
        .filter(|scene| scene["flow_framework_mcf_overlay"]["dynamic_operation"].is_string())
        .collect::<Vec<_>>();
    assert!(!dynamic_boundaries.is_empty());
    let mut previous_iteration = None::<String>;
    let mut previous_serial = 0_u64;
    for scene in dynamic_boundaries {
        let overlay = &scene["flow_framework_mcf_overlay"];
        let operation = overlay["dynamic_operation"]
            .as_str()
            .expect("dynamic operation");
        let serial = overlay["dynamic_operation_serial"]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .expect("dynamic operation serial");
        let iteration = overlay["iteration"]
            .as_str()
            .expect("source iteration")
            .to_owned();
        if previous_iteration.as_ref() != Some(&iteration) {
            previous_iteration = Some(iteration);
            previous_serial = 0;
        }
        assert!(serial > previous_serial);
        previous_serial = serial;
        let stage = overlay["stage"].as_str().expect("framework stage");
        assert!(matches!(
            (stage, operation),
            (
                "periodic-reinitialize",
                "topology-stage-applied" | "periodic-rebuilt"
            ) | ("detect", "detect-returned")
                | (
                    "query-minimum-ratio-cycle",
                    "cycle-queried-accepted"
                        | "cycle-queried-rejected"
                        | "level-shifted"
                        | "query-returned"
                )
                | ("source-progress", "flow-applied" | "completed")
        ));
    }
    for operation in ["flow-applied", "completed"] {
        assert!(scenes.iter().any(|scene| {
            scene["flow_framework_mcf_overlay"]["dynamic_operation"] == operation
                && scene["flow_framework_mcf_overlay"]["dynamic_operation_serial"].is_string()
                && scene["trace_event"]["entity_refs"] == serde_json::json!([])
        }));
    }
}

fn assert_deterministic_mcf_final(scenes: &[serde_json::Value]) -> &serde_json::Value {
    let stage = |name: &str| {
        scenes
            .iter()
            .find(|scene| scene["flow_framework_mcf_overlay"]["stage"] == name)
    };
    let rounding = stage("round-fractional-flow").expect("source final point boundary");
    let rounding_edges = rounding["flow_framework_mcf_overlay"]["final_point_edges"]
        .as_array()
        .expect("augmented final point");
    assert!(
        rounding_edges
            .iter()
            .all(|edge| edge["rounded_flow"].is_null())
    );
    assert!(
        rounding_edges
            .iter()
            .filter(|edge| edge["auxiliary"] == false)
            .any(|edge| edge["flow"]["denominator"] != "1")
    );
    let certificate = stage("check-certificate").expect("rounded certificate boundary");
    let certified_edges = certificate["flow_framework_mcf_overlay"]["final_point_edges"]
        .as_array()
        .expect("augmented rounded point");
    assert!(certified_edges.iter().all(|edge| {
        edge["rounded_flow"].is_string()
            && (edge["auxiliary"] != true || edge["rounded_flow"] == "0")
    }));
    let solved = scenes.last().expect("terminal scene");
    assert_flow_framework_rational_digit_band(solved);
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "min-cost-flow");
    assert_eq!(solved["outcome"]["total_cost"], "6");
    assert_eq!(solved["flow_framework_mcf_overlay"]["stage"], "optimal");
    assert_eq!(
        solved["flow_framework_mcf_overlay"]["termination"],
        "source-additive-half-gap"
    );
    assert_eq!(solved["flow_framework_mcf_overlay"]["optimum_cost"], "6");
    let solved_edges = solved["flow_framework_mcf_overlay"]["edges"]
        .as_array()
        .expect("solved edges");
    for (edge_id, expected) in [("a", "3"), ("b", "3"), ("expensive", "0")] {
        let edge = solved_edges
            .iter()
            .find(|edge| edge["edge_id"] == edge_id)
            .expect("canonical solved edge");
        assert_eq!(edge["flow"]["numerator"], expected);
        assert_eq!(edge["flow"]["denominator"], "1");
    }
    solved
}

#[test]
fn deterministic_almost_linear_mcf_dispatches_flow_framework_trace_and_fast_parity() {
    let mut session = WasmSession::new(&deterministic_almost_linear_mcf_scenario("trace"))
        .expect("deterministic almost-linear MCF scenario");
    let mut scenes = Vec::new();
    while let Some(frame) = session
        .stage_next_json()
        .expect("deterministic MCF trace stages")
    {
        scenes.push(serde_json::from_str::<serde_json::Value>(&frame).expect("scene JSON"));
        session.commit_staged_next();
    }
    assert_deterministic_mcf_progress(&scenes);
    let solved = assert_deterministic_mcf_final(&scenes);

    let mut fast = WasmSession::new(&deterministic_almost_linear_mcf_scenario("fast"))
        .expect("fast deterministic MCF scenario");
    while fast
        .stage_next_json()
        .expect("fast deterministic MCF stage")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast scene serializes"))
            .expect("fast scene JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(
        fast_scene["flow_framework_mcf_overlay"],
        solved["flow_framework_mcf_overlay"]
    );
}

#[test]
fn deterministic_almost_linear_mcf_admission_failure_is_public_reversible_and_repeatable() {
    let mut session = WasmSession::new(&deterministic_almost_linear_mcf_oversized_scenario())
        .expect("oversized deterministic MCF scenario");
    let ready = session.current_frame_json().expect("ready serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let value: serde_json::Value = serde_json::from_str(&limited).expect("resource JSON");
    assert_eq!(value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("discard resource candidate");
    assert_eq!(
        session
            .stage_next_json()
            .expect("restage")
            .expect("boundary"),
        limited
    );
}

#[test]
fn deterministic_almost_linear_mcf_self_loop_is_rejected_before_ready_publication() {
    let scenario =
        decode_flow_scenario(deterministic_almost_linear_mcf_self_loop_scenario().as_bytes())
            .expect("self-loop scenario is structurally valid");
    let graph = scenario.canonical_network().expect("canonical graph");
    let descriptor = find_algorithm_by_id(AlgorithmId::DeterministicAlmostLinearMcf)
        .expect("catalog descriptor");
    assert_eq!(
        validate_catalog_graph_contract(&scenario, descriptor, &graph),
        Err("selected flow algorithm requires a graph without self-loops")
    );
}

#[test]
fn deterministic_almost_linear_mcf_rejects_non_strict_inputs_before_ready_publication() {
    assert_eq!(
        validate_flow_session_input(&deterministic_almost_linear_mcf_infeasible_scenario())
            .expect_err("infeasible input has no strict interior"),
        "selected flow algorithm requires a feasible flow strictly inside every edge bound"
    );
    assert_eq!(
        validate_flow_session_input(&deterministic_almost_linear_mcf_saturated_cut_scenario())
            .expect_err("saturated cut has no strict interior"),
        "selected flow algorithm requires a feasible flow strictly inside every edge bound"
    );
}

#[test]
fn minimum_ratio_cycle_mcf_admission_failure_is_public_reversible_and_repeatable() {
    let mut session = WasmSession::new(&minimum_ratio_cycle_mcf_oversized_scenario())
        .expect("oversized minimum-ratio-cycle MCF Scenario");
    let ready = session.current_frame_json().expect("ready serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let value: serde_json::Value = serde_json::from_str(&limited).expect("resource JSON");
    assert_eq!(value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("discard resource candidate");
    assert_eq!(
        session
            .stage_next_json()
            .expect("restage")
            .expect("boundary"),
        limited
    );
}

#[test]
fn orlin_mcf_admission_failure_is_public_reversible_and_repeatable() {
    let mut session =
        WasmSession::new(&orlin_mcf_oversized_scenario()).expect("oversized Orlin Scenario");
    let ready = session
        .current_frame_json()
        .expect("ready boundary serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let limited_value: serde_json::Value =
        serde_json::from_str(&limited).expect("resource boundary JSON");
    assert_eq!(limited_value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready is unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("resource candidate can be discarded");
    assert_eq!(
        session.current_frame_json().expect("ready after discard"),
        ready
    );
    assert_eq!(
        session
            .stage_next_json()
            .expect("resource boundary restages")
            .expect("resource boundary exists again"),
        limited
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn orlin_max_flow_dispatches_compaction_transfer_lifting_and_fast_equivalence() {
    let mut session =
        WasmSession::new(&orlin_max_flow_scenario("trace")).expect("Orlin max-flow Scenario");
    let ready: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("ready frame serializes"),
    )
    .expect("ready JSON");
    assert_eq!(ready["solve_status"], "ready");
    assert!(ready["orlin_max_flow_overlay"].is_null());

    let mut selected = None;
    let mut transferred = None;
    let mut built = None;
    let mut lifted = None;
    let mut inspected_subproblem = None;
    let mut inspected_decomposition = None;
    let mut inspected_lift = None;
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("Orlin max-flow frame JSON");
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("orlin-max-flow.select-case") => {
                selected.get_or_insert((session.cursor(), scene));
            }
            Some("orlin-max-flow.transfer-capacity") => {
                transferred.get_or_insert(scene);
            }
            Some("orlin-max-flow.build-subproblem") => {
                built.get_or_insert(scene);
            }
            Some("orlin-max-flow.lift-path") => {
                lifted.get_or_insert(scene);
            }
            Some("orlin-max-flow.inspect-subproblem-arc") => {
                inspected_subproblem.get_or_insert(scene);
            }
            Some("orlin-max-flow.inspect-decomposition-arc") => {
                inspected_decomposition.get_or_insert(scene);
            }
            Some("orlin-max-flow.inspect-lift-residual-arc") => {
                inspected_lift.get_or_insert(scene);
            }
            _ => {}
        }
    }
    let (selected_cursor, selected) = selected.expect("three-way case selection");
    assert_eq!(
        selected["orlin_max_flow_overlay"]["phase_case"],
        "compact-approximation"
    );
    assert_eq!(
        selected["trace_event"]["entity_refs"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "the aggregate branch decision must focus one critical-node witness instead of flashing the whole graph"
    );
    assert_eq!(selected["trace_event"]["entity_refs"][0]["kind"], "node");
    assert_eq!(
        selected["orlin_max_flow_overlay"]["residual_arcs"]
            .as_array()
            .map(Vec::len),
        Some(24)
    );
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(selected_cursor)
            .expect("seek selected compact case"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, selected);

    let transferred = transferred.expect("anti-abundant capacity transfer");
    assert_eq!(
        transferred["orlin_max_flow_overlay"]["stage"],
        "transfer-capacity"
    );
    assert!(
        transferred["orlin_max_flow_overlay"]["active_original_path"]
            .as_array()
            .is_some_and(|path| path.len() >= 2)
    );
    let built = built.expect("compact subproblem boundary");
    assert!(
        built["orlin_max_flow_overlay"]["compact_arcs"]
            .as_array()
            .is_some_and(|arcs| arcs.iter().any(|arc| {
                arc["kind"] == "transferred-pseudo"
                    && arc["witness"]
                        .as_array()
                        .is_some_and(|path| path.len() >= 2)
            }))
    );
    let lifted = lifted.expect("compact path lift");
    let lifted_path = lifted["orlin_max_flow_overlay"]["active_original_path"]
        .as_array()
        .expect("lifted original residual path");
    assert!(lifted_path.len() >= 2, "lifted path: {lifted_path:?}");
    for inspected in [
        inspected_subproblem.expect("subproblem source scan"),
        inspected_decomposition.expect("decomposition source scan"),
    ] {
        let serials = inspected["orlin_max_flow_overlay"]["compact_arcs"]
            .as_array()
            .expect("compact arcs")
            .iter()
            .filter_map(|arc| arc["inspection_serial"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(serials.len(), 1);
        assert_eq!(
            serials[0],
            inspected["trace_event"]["detail"]["value"]
                .as_str()
                .expect("scan detail")
        );
    }
    let inspected_lift = inspected_lift.expect("original residual source scan");
    let lift_serials = inspected_lift["orlin_max_flow_overlay"]["residual_arcs"]
        .as_array()
        .expect("residual arcs")
        .iter()
        .filter_map(|arc| arc["inspection_serial"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(lift_serials.len(), 1);
    assert_eq!(
        lift_serials[0],
        inspected_lift["trace_event"]["detail"]["value"]
            .as_str()
            .expect("scan detail")
    );

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final Orlin max-flow frame"),
    )
    .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "1");
    assert_eq!(solved["outcome"]["cut_bound"], "1");
    assert_eq!(solved["orlin_max_flow_overlay"]["stage"], "optimal");
    assert_ne!(solved["metrics"][4], "0");
    assert_ne!(solved["metrics"][5], "0");
    assert_ne!(solved["metrics"][7], "0");
    assert_ne!(solved["metrics"][11], "0");

    let mut fast =
        WasmSession::new(&orlin_max_flow_scenario("fast")).expect("fast Orlin max-flow Scenario");
    while fast
        .stage_next_json()
        .expect("fast Orlin max-flow result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["orlin_max_flow_overlay"],
        solved["orlin_max_flow_overlay"]
    );
}

#[test]
fn orlin_max_flow_admission_failure_is_public_reversible_and_repeatable() {
    let mut session = WasmSession::new(&orlin_max_flow_oversized_scenario())
        .expect("oversized Orlin max-flow Scenario");
    let ready = session
        .current_frame_json()
        .expect("ready boundary serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let limited_value: serde_json::Value =
        serde_json::from_str(&limited).expect("resource boundary JSON");
    assert_eq!(limited_value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready is unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("resource candidate can be discarded");
    assert_eq!(
        session.current_frame_json().expect("ready after discard"),
        ready
    );
    assert_eq!(
        session
            .stage_next_json()
            .expect("resource boundary restages")
            .expect("resource boundary exists again"),
        limited
    );
}

#[test]
fn electrical_flow_catalog_contract_rejects_every_kernel_precondition_before_ready() {
    fn validate(value: &serde_json::Value) -> Result<(), &'static str> {
        let encoded = value.to_string();
        let scenario = decode_flow_scenario(encoded.as_bytes()).expect("valid scenario wire");
        let algorithm = scenario
            .payload
            .algorithm
            .id
            .parse::<AlgorithmId>()
            .expect("canonical algorithm ID");
        let graph = scenario.canonical_network().expect("canonical graph");
        let descriptor = find_algorithm_by_id(algorithm).expect("catalog descriptor");
        validate_catalog_graph_contract(&scenario, descriptor, &graph)
    }

    let base: serde_json::Value =
        serde_json::from_str(&electrical_flow_scenario("trace")).expect("electrical scenario JSON");
    assert_eq!(validate(&base), Ok(()));

    let mut self_loop = base.clone();
    self_loop["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("edge array")
        .push(serde_json::json!({
            "id": "loop", "from": "s", "to": "s", "capacity": "1", "cost": "0"
        }));
    assert_eq!(
        validate(&self_loop),
        Err("selected flow algorithm requires a graph without self-loops")
    );

    let mut nonzero_lower = base.clone();
    nonzero_lower["payload"]["graph"]["edges"][0]["lower"] = serde_json::json!("1");
    assert_eq!(
        validate(&nonzero_lower),
        Err("selected flow algorithm requires zero supplies and zero lower bounds")
    );

    let mut zero_capacity = base.clone();
    zero_capacity["payload"]["graph"]["edges"][0]["capacity"] = serde_json::json!("0");
    assert_eq!(
        validate(&zero_capacity),
        Err("selected flow algorithm requires every edge capacity to be positive")
    );

    let mut empty = base.clone();
    empty["payload"]["graph"]["edges"] = serde_json::json!([]);
    assert_eq!(
        validate(&empty),
        Err("selected flow algorithm requires at least one edge")
    );

    let mut priced = base.clone();
    priced["payload"]["graph"]["edges"][0]["cost"] = serde_json::json!("1");
    assert_eq!(
        validate(&priced),
        Err("selected flow algorithm requires zero-cost edges")
    );

    let mut disconnected = base;
    disconnected["payload"]["graph"]["nodes"]
        .as_array_mut()
        .expect("node array")
        .push(serde_json::json!({ "id": "isolated" }));
    assert_eq!(
        validate(&disconnected),
        Err("selected flow algorithm requires a connected underlying graph")
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn electrical_flow_dispatches_pcg_energy_exact_reference_and_fast_equivalence() {
    let mut session =
        WasmSession::new(&electrical_flow_scenario("trace")).expect("electrical-flow Scenario");
    let ready: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("ready frame serializes"),
    )
    .expect("ready JSON");
    assert_eq!(ready["solve_status"], "ready");
    assert!(ready["electrical_flow_overlay"].is_null());

    let mut iteration = None;
    let mut recovered = None;
    let mut exact = None;
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("electrical trace frame JSON");
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("electrical-flow.cg-iteration") => {
                iteration.get_or_insert((session.cursor(), scene));
            }
            Some("electrical-flow.recover-currents") => {
                recovered.get_or_insert(scene);
            }
            Some("electrical-flow.check-exact-reference") => {
                exact.get_or_insert(scene);
            }
            _ => {}
        }
    }
    let (iteration_cursor, iteration) = iteration.expect("PCG iteration");
    assert_eq!(
        iteration["electrical_flow_overlay"]["stage"],
        "conjugate-gradient-iteration"
    );
    assert_ne!(
        iteration["electrical_flow_overlay"]["residual_l2"],
        serde_json::Value::Null
    );
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(iteration_cursor)
            .expect("seek PCG iteration"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, iteration);

    let recovered = recovered.expect("current recovery");
    let edges = recovered["electrical_flow_overlay"]["edges"]
        .as_array()
        .expect("electrical edges");
    let backward = edges
        .iter()
        .find(|edge| edge["edge_id"] == "bs")
        .expect("backward-oriented resistor");
    assert!(
        backward["current"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value < 0.0)
    );
    assert!(edges.iter().all(|edge| {
        edge["energy"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value > 0.0)
    }));
    let exact = exact.expect("exact reference boundary");
    assert_eq!(
        exact["electrical_flow_overlay"]["exact_effective_resistance"],
        serde_json::json!({ "numerator": "2", "denominator": "5" })
    );

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek electrical final frame"),
    )
    .expect("final JSON");
    assert_eq!(solved["solve_status"], "primitive-complete");
    assert_eq!(solved["outcome"]["kind"], "electrical-flow");
    assert_eq!(
        solved["outcome"]["exact_effective_resistance"],
        serde_json::json!({ "numerator": "2", "denominator": "5" })
    );
    assert!(
        solved["residual_arcs"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert_ne!(solved["metrics"][2], "0");
    assert_eq!(solved["metrics"][5], "3");
    assert_eq!(solved["metrics"][6], "1");

    let mut fast =
        WasmSession::new(&electrical_flow_scenario("fast")).expect("fast electrical-flow Scenario");
    while fast
        .stage_next_json()
        .expect("fast electrical result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["solve_status"], "primitive-complete");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["electrical_flow_overlay"],
        solved["electrical_flow_overlay"]
    );
}

#[test]
fn electrical_flow_admission_failure_is_public_reversible_and_repeatable() {
    let mut session = WasmSession::new(&electrical_flow_oversized_scenario())
        .expect("oversized electrical-flow Scenario");
    let ready = session
        .current_frame_json()
        .expect("ready boundary serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let limited_value: serde_json::Value =
        serde_json::from_str(&limited).expect("resource boundary JSON");
    assert_eq!(limited_value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready is unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("resource candidate can be discarded");
    assert_eq!(
        session
            .stage_next_json()
            .expect("resource boundary restages")
            .expect("resource boundary exists again"),
        limited
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn augmenting_electrical_dispatches_boost_cleanup_certificate_and_fast_equivalence() {
    let mut session = WasmSession::new(&augmenting_electrical_scenario("trace"))
        .expect("augmenting-electrical Scenario");
    let ready: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("ready frame serializes"),
    )
    .expect("ready JSON");
    assert_eq!(ready["solve_status"], "ready");
    assert!(ready["augmenting_electrical_overlay"].is_null());

    let mut boost = None;
    let mut electrical = None;
    let mut rounded = None;
    let mut cleanup = None;
    let mut extracted = None;
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value = serde_json::from_str(&commit_next(&mut session))
            .expect("augmenting-electrical trace JSON");
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("augmenting-electrical-flow.boost-high-energy") => {
                boost.get_or_insert((session.cursor(), scene));
            }
            Some("augmenting-electrical-flow.solve-direction") => {
                electrical.get_or_insert(scene);
            }
            Some("augmenting-electrical-flow.round-central-flow") => {
                rounded.get_or_insert(scene);
            }
            Some("augmenting-electrical-flow.cleanup-augmenting-path") => {
                cleanup.get_or_insert(scene);
            }
            Some("augmenting-electrical-flow.extract-directed-reduction") => {
                extracted.get_or_insert(scene);
            }
            _ => {}
        }
    }
    let (boost_cursor, boost) = boost.expect("source boost boundary");
    assert_eq!(
        boost["augmenting_electrical_overlay"]["stage"],
        "boost-high-energy-arc"
    );
    assert!(
        boost["augmenting_electrical_overlay"]["working_edges"]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|working_edges| working_edges > 24)
    );
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(boost_cursor)
            .expect("seek source boost boundary"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, boost);
    let electrical = electrical.expect("electrical direction boundary");
    assert!(
        electrical["augmenting_electrical_overlay"]["electrical_energy"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value > 0.0)
    );
    let rounded = rounded.expect("rounded central-flow boundary");
    assert!(
        rounded["augmenting_electrical_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges
                .iter()
                .all(|edge| edge["rounded_central_flow"].is_string()))
    );
    let cleanup = cleanup.expect("working-graph cleanup boundary");
    let cleanup_path = cleanup["augmenting_electrical_overlay"]["active_working_path"]
        .as_array()
        .expect("cleanup path array");
    assert!(!cleanup_path.is_empty());
    assert_eq!(cleanup_path[0]["from_node"], "s");
    assert_eq!(
        cleanup_path.last().expect("last cleanup arc")["to_node"],
        "t"
    );
    assert!(cleanup_path.iter().all(|arc| arc["flow_after"].is_string()));
    let extracted = extracted.expect("directed-reduction extraction boundary");
    assert_eq!(
        extracted["augmenting_electrical_overlay"]["stage"],
        "extract-directed-flow"
    );
    assert!(
        extracted["augmenting_electrical_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().all(|edge| {
                edge["extraction_central_scaled"].is_string()
                    && edge["extraction_toward_source"].is_string()
                    && edge["extraction_out_of_sink"].is_string()
            }))
    );

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final frame"),
    )
    .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "9");
    assert_eq!(solved["augmenting_electrical_overlay"]["stage"], "optimal");
    assert!(
        solved["augmenting_electrical_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().all(|edge| edge["final_flow"].is_string()))
    );
    assert_ne!(solved["metrics"][1], "0");
    assert_ne!(solved["metrics"][5], "0");
    assert_eq!(solved["metrics"][10], "1");

    let mut fast = WasmSession::new(&augmenting_electrical_scenario("fast"))
        .expect("fast augmenting-electrical Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["augmenting_electrical_overlay"],
        solved["augmenting_electrical_overlay"]
    );
}

#[test]
fn augmenting_electrical_admission_failure_is_public_reversible_and_repeatable() {
    let mut session = WasmSession::new(&augmenting_electrical_oversized_scenario())
        .expect("oversized augmenting-electrical Scenario");
    let ready = session
        .current_frame_json()
        .expect("ready boundary serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let limited_value: serde_json::Value =
        serde_json::from_str(&limited).expect("resource boundary JSON");
    assert_eq!(limited_value["solve_status"], "resource-limit");
    assert_eq!(limited_value["resource_limit_reason"], "input-admission");
    assert_eq!(
        session.current_frame_json().expect("ready is unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("resource candidate can be discarded");
    assert_eq!(
        session
            .stage_next_json()
            .expect("resource boundary restages")
            .expect("resource boundary exists again"),
        limited
    );
}

#[test]
fn augmenting_electrical_capacity_limit_is_enforced_before_kernel_dispatch() {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&augmenting_electrical_scenario("trace"))
            .expect("augmenting-electrical Scenario JSON");
    scenario["payload"]["graph"]["edges"][0]["capacity"] = serde_json::json!("9");
    let source = scenario.to_string();
    let validated = validate_flow_session_input(&source).expect("valid bounded Scenario envelope");
    assert!(validated.resource_admission_limited);

    let mut session = FlowSession::new(&source).expect("resource-limited session");
    let candidate = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let candidate: serde_json::Value =
        serde_json::from_str(&candidate).expect("resource boundary JSON");
    assert_eq!(candidate["solve_status"], "resource-limit");
    assert_eq!(candidate["resource_limit_reason"], "input-admission");
}

#[test]
#[allow(clippy::too_many_lines)]
fn interior_point_dispatches_both_electrical_steps_rounding_and_fast_equivalence() {
    let mut session = WasmSession::new(&interior_point_max_flow_scenario("trace"))
        .expect("interior-point Scenario");
    let ready: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("ready frame serializes"),
    )
    .expect("ready JSON");
    assert!(ready["interior_point_max_flow_overlay"].is_null());
    let mut reduction = None;
    let mut associated = None;
    let mut centering = None;
    let mut extracted = None;
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("interior-point trace JSON");
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("interior-point-max-flow.build-b-matching-reduction") => {
                reduction = Some(scene);
            }
            Some("interior-point-max-flow.solve-associated-electrical") => {
                associated.get_or_insert((session.cursor(), scene));
            }
            Some("interior-point-max-flow.solve-centering-electrical") => {
                centering.get_or_insert(scene);
            }
            Some("interior-point-max-flow.extract-fractional-flow") => {
                extracted = Some(scene);
            }
            _ => {}
        }
    }
    let reduction = reduction.expect("b-matching reduction boundary");
    assert_eq!(
        reduction["interior_point_max_flow_overlay"]["stage"],
        "build-b-matching-reduction"
    );
    assert_eq!(
        reduction["interior_point_max_flow_overlay"]["b_matching_nodes"],
        "16"
    );
    assert_eq!(
        reduction["interior_point_max_flow_overlay"]["b_matching_edges"],
        "17"
    );
    let (associated_cursor, associated) = associated.expect("associated electrical boundary");
    assert!(
        associated["interior_point_max_flow_overlay"]["electrical_energy"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value > 0.0)
    );
    assert_eq!(
        associated["interior_point_max_flow_overlay"]["stage"],
        "solve-electrical-direction"
    );
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(associated_cursor)
            .expect("seek associated step"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, associated);
    let centering = centering.expect("centering electrical boundary");
    assert_eq!(
        centering["interior_point_max_flow_overlay"]["stage"],
        "solve-centering-direction"
    );
    let extracted = extracted.expect("fractional extraction boundary");
    assert!(
        extracted["interior_point_max_flow_overlay"]["duality_gap"]
            .as_str()
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value <= 0.5 + 1.0e-8)
    );
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final interior-point frame"),
    )
    .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "2");
    assert_eq!(
        solved["interior_point_max_flow_overlay"]["stage"],
        "optimal"
    );
    assert!(
        solved["interior_point_max_flow_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().all(|edge| edge["final_flow"].is_string()))
    );
    assert_ne!(solved["metrics"][5], "0");
    assert_eq!(solved["metrics"][7], solved["metrics"][8]);
    assert_eq!(solved["metrics"][10], "1");

    let mut fast = WasmSession::new(&interior_point_max_flow_scenario("fast"))
        .expect("fast interior-point Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["interior_point_max_flow_overlay"],
        solved["interior_point_max_flow_overlay"]
    );
}

#[test]
fn interior_point_admission_failure_is_public_reversible_and_repeatable() {
    let mut session = WasmSession::new(&interior_point_max_flow_oversized_scenario())
        .expect("oversized interior-point Scenario");
    let ready = session
        .current_frame_json()
        .expect("ready boundary serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let limited_value: serde_json::Value =
        serde_json::from_str(&limited).expect("resource boundary JSON");
    assert_eq!(limited_value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("resource candidate can be discarded");
    assert_eq!(
        session
            .stage_next_json()
            .expect("resource boundary restages")
            .expect("resource boundary exists again"),
        limited
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn minimum_ratio_cycle_dispatches_exact_objective_cycle_space_and_fast_equivalence() {
    let mut session = WasmSession::new(&minimum_ratio_cycle_scenario("trace"))
        .expect("minimum-ratio-cycle Scenario");
    let ready: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("ready frame serializes"),
    )
    .expect("ready JSON");
    assert!(ready["minimum_ratio_cycle_overlay"].is_null());
    let mut forest = None;
    let mut candidate = None;
    let mut best = None;
    let mut verified = None;
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value = serde_json::from_str(&commit_next(&mut session))
            .expect("minimum-ratio-cycle trace JSON");
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("minimum-ratio-cycle-max-flow.build-spanning-forest") => {
                forest = Some(scene);
            }
            Some("minimum-ratio-cycle-max-flow.evaluate-cycle") => {
                candidate.get_or_insert((session.cursor(), scene));
            }
            Some("minimum-ratio-cycle-max-flow.update-best") => best = Some(scene),
            Some("minimum-ratio-cycle-max-flow.verify-cycle-space") => {
                verified = Some(scene);
            }
            _ => {}
        }
    }
    let forest = forest.expect("forest boundary");
    assert_eq!(
        forest["minimum_ratio_cycle_overlay"]["fundamental_cycles"],
        "2"
    );
    assert!(
        forest["minimum_ratio_cycle_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges
                .iter()
                .filter(|edge| edge["tree_edge"] == true)
                .count()
                == 4)
    );
    let (candidate_cursor, candidate) = candidate.expect("candidate boundary");
    assert_eq!(
        candidate["minimum_ratio_cycle_overlay"]["stage"],
        "evaluate-cycle"
    );
    assert!(
        candidate["minimum_ratio_cycle_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().any(|edge| edge["candidate_sign"] == "-1"))
    );
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(candidate_cursor)
            .expect("seek candidate boundary"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, candidate);
    assert!(best.is_some(), "incumbent update boundary");
    let verified = verified.expect("cycle-space verification");
    assert_eq!(
        verified["minimum_ratio_cycle_overlay"]["maximum_absolute_balance"],
        "0"
    );
    assert!(
        verified["minimum_ratio_cycle_overlay"]["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().all(|node| node["candidate_balance"] == "0"))
    );

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final ratio frame"),
    )
    .expect("final JSON");
    assert_eq!(solved["solve_status"], "primitive-complete");
    assert_eq!(solved["outcome"]["kind"], "minimum-ratio-cycle");
    assert_eq!(
        solved["outcome"]["ratio"],
        serde_json::json!({ "numerator": "-10", "denominator": "3" })
    );
    assert_eq!(
        solved["minimum_ratio_cycle_overlay"]["selected_edge_count"],
        "3"
    );
    assert_eq!(solved["metrics"][7], "2");

    let mut fast = WasmSession::new(&minimum_ratio_cycle_scenario("fast"))
        .expect("fast minimum-ratio-cycle Scenario");
    while fast
        .stage_next_json()
        .expect("fast ratio result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["minimum_ratio_cycle_overlay"],
        solved["minimum_ratio_cycle_overlay"]
    );
}

#[test]
fn minimum_ratio_cycle_admission_failure_is_public_reversible_and_repeatable() {
    let mut session = WasmSession::new(&minimum_ratio_cycle_oversized_scenario())
        .expect("oversized minimum-ratio-cycle Scenario");
    let ready = session
        .current_frame_json()
        .expect("ready boundary serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let limited_value: serde_json::Value =
        serde_json::from_str(&limited).expect("resource boundary JSON");
    assert_eq!(limited_value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("resource candidate can be discarded");
    assert_eq!(
        session
            .stage_next_json()
            .expect("resource boundary restages")
            .expect("resource boundary exists again"),
        limited
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn weighted_augmenting_paths_dispatches_hierarchy_labels_paths_scaling_and_fast_parity() {
    let mut session = WasmSession::new(&weighted_augmenting_paths_scenario("trace"))
        .expect("weighted augmenting-path Scenario");
    let ready: serde_json::Value =
        serde_json::from_str(&session.current_frame_json().expect("ready serializes"))
            .expect("ready JSON");
    assert!(ready["weighted_augmenting_paths_overlay"].is_null());
    let mut phases = BTreeSet::new();
    let mut hierarchy = None;
    let mut certified = None;
    let mut assigned = None;
    let mut relabel = None;
    let mut augmentation = None;
    while session.cursor() < session.item_count() {
        let cursor = session.cursor() + 1;
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("weighted trace JSON");
        let overlay = &scene["weighted_augmenting_paths_overlay"];
        match overlay["stage"].as_str() {
            Some("begin-capacity-phase") => {
                phases.insert(overlay["phase"].as_str().expect("phase").to_owned());
            }
            Some("build-hierarchy") => {
                let includes_both_kinds = overlay["residual_arcs"].as_array().is_some_and(|arcs| {
                    arcs.iter().any(|arc| arc["hierarchy_kind"] == "dag")
                        && arcs.iter().any(|arc| arc["hierarchy_kind"] == "expanding")
                });
                if includes_both_kinds {
                    hierarchy.get_or_insert(scene);
                }
            }
            Some("certify-expansion") => {
                certified.get_or_insert(scene);
            }
            Some("assign-weights") => {
                assigned.get_or_insert(scene);
            }
            Some("relabel-sweep") => {
                relabel.get_or_insert(scene);
            }
            Some("augment-path") => {
                augmentation.get_or_insert((cursor, scene));
            }
            _ => {}
        }
    }
    assert_eq!(
        phases,
        BTreeSet::from(["0".to_owned(), "1".to_owned(), "2".to_owned()])
    );
    let hierarchy = hierarchy.expect("hierarchy boundary");
    assert!(
        hierarchy["weighted_augmenting_paths_overlay"]["residual_arcs"]
            .as_array()
            .is_some_and(|arcs| arcs.iter().any(|arc| arc["hierarchy_kind"] == "dag")
                && arcs.iter().any(|arc| arc["hierarchy_kind"] == "expanding"))
    );
    let certified = certified.expect("phi certificate");
    assert_ne!(
        certified["weighted_augmenting_paths_overlay"]["phi_numerator"],
        "0"
    );
    assert_ne!(
        certified["weighted_augmenting_paths_overlay"]["phi_denominator"],
        "0"
    );
    let assigned = assigned.expect("weight assignment");
    assert_ne!(assigned["weighted_augmenting_paths_overlay"]["height"], "0");
    assert!(
        assigned["weighted_augmenting_paths_overlay"]["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().all(|node| node["order"] != "0"))
    );
    let relabel = relabel.expect("relabel boundary");
    assert!(
        relabel["weighted_augmenting_paths_overlay"]["relabel_jumps"]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|value| value > 0)
    );
    let (augment_cursor, augmentation) = augmentation.expect("augment path");
    assert_ne!(
        augmentation["weighted_augmenting_paths_overlay"]["active_bottleneck"],
        "0"
    );
    assert!(
        augmentation["weighted_augmenting_paths_overlay"]["active_path"]
            .as_array()
            .is_some_and(|path| !path.is_empty())
    );
    assert_eq!(
        augmentation["trace_event"]["catalog_id"],
        "weighted-augmenting-paths.augment-path"
    );
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(augment_cursor)
            .expect("seek augmentation"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, augmentation);

    let solved: serde_json::Value =
        serde_json::from_str(&session.seek_json(session.item_count()).expect("seek final"))
            .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "max-flow");
    assert_eq!(solved["outcome"]["value"], "10");
    assert_eq!(
        solved["weighted_augmenting_paths_overlay"]["stage"],
        "optimal"
    );
    assert_eq!(solved["metrics"][0], "3");

    let mut fast = WasmSession::new(&weighted_augmenting_paths_scenario("fast"))
        .expect("fast weighted augmenting-path Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["weighted_augmenting_paths_overlay"],
        solved["weighted_augmenting_paths_overlay"]
    );
}

#[test]
fn weighted_augmenting_paths_admission_failure_is_reversible_and_repeatable() {
    let mut session = WasmSession::new(&weighted_augmenting_paths_oversized_scenario())
        .expect("oversized weighted augmenting-path Scenario");
    let ready = session.current_frame_json().expect("ready serializes");
    let limited = session
        .stage_next_json()
        .expect("resource stages")
        .expect("resource boundary");
    let value: serde_json::Value = serde_json::from_str(&limited).expect("resource JSON");
    assert_eq!(value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("discard resource candidate");
    assert_eq!(
        session
            .stage_next_json()
            .expect("restage")
            .expect("boundary"),
        limited
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn weighted_push_relabel_dispatches_shortcuts_cut_completion_and_fast_parity() {
    let mut session = WasmSession::new(&weighted_push_relabel_shortcut_scenario("trace"))
        .expect("weighted push-relabel Scenario");
    let ready: serde_json::Value =
        serde_json::from_str(&session.current_frame_json().expect("ready serializes"))
            .expect("ready JSON");
    assert!(ready["weighted_push_relabel_shortcut_overlay"].is_null());
    let mut hierarchy = None;
    let mut shortcuts = None;
    let mut relabel = None;
    let mut augmentation = None;
    let mut sparse_cut = None;
    let mut completion = None;
    while session.cursor() < session.item_count() {
        let cursor = session.cursor() + 1;
        let scene: serde_json::Value = serde_json::from_str(&commit_next(&mut session))
            .expect("weighted push-relabel trace JSON");
        let overlay = &scene["weighted_push_relabel_shortcut_overlay"];
        match overlay["stage"].as_str() {
            Some("build-weak-hierarchy") => hierarchy = Some(scene),
            Some("build-shortcut-graph") => shortcuts = Some(scene),
            Some("relabel-sweep") => {
                relabel.get_or_insert(scene);
            }
            Some("augment-path") => {
                augmentation.get_or_insert((cursor, scene));
            }
            Some("select-sparse-cut") => sparse_cut = Some(scene),
            Some("complete-residual-rounds") => completion = Some(scene),
            _ => {}
        }
    }
    let hierarchy = hierarchy.expect("weak hierarchy boundary");
    assert_eq!(
        hierarchy["weighted_push_relabel_shortcut_overlay"]["hierarchy_levels"],
        "1"
    );
    assert!(
        hierarchy["weighted_push_relabel_shortcut_overlay"]["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes
                .iter()
                .filter(|node| node["original"] == true)
                .all(|node| node["order"] != "0"))
    );
    let shortcuts = shortcuts.expect("shortcut boundary");
    assert!(
        shortcuts["weighted_push_relabel_shortcut_overlay"]["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node["original"] == false))
    );
    assert!(
        shortcuts["weighted_push_relabel_shortcut_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges
                .iter()
                .any(|edge| { edge["kind"] == "shortcut" && edge["weight"] == "2" }))
    );
    assert!(
            relabel.expect("relabel boundary")["weighted_push_relabel_shortcut_overlay"]
                ["relabel_steps"]
                .as_str()
                .and_then(|value| value.parse::<u64>().ok())
                .is_some_and(|value| value > 0)
        );
    let (augmentation_cursor, augmentation) = augmentation.expect("augmentation boundary");
    assert_ne!(
        augmentation["weighted_push_relabel_shortcut_overlay"]["active_bottleneck"],
        "0"
    );
    assert_eq!(
        augmentation["trace_event"]["catalog_id"],
        "weighted-push-relabel.augment-path"
    );
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(augmentation_cursor)
            .expect("seek augmentation"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, augmentation);
    assert!(
            sparse_cut.expect("sparse cut")["weighted_push_relabel_shortcut_overlay"]
                ["sparse_cut_capacity"]
                .is_string()
        );
    assert!(
            completion.expect("completion")["weighted_push_relabel_shortcut_overlay"]
                ["completion_augmentations"]
                .as_str()
                .and_then(|value| value.parse::<u64>().ok())
                .is_some_and(|value| value > 0)
        );
    let solved: serde_json::Value =
        serde_json::from_str(&session.seek_json(session.item_count()).expect("seek final"))
            .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "10");
    assert_eq!(
        solved["weighted_push_relabel_shortcut_overlay"]["stage"],
        "optimal"
    );

    let mut fast = WasmSession::new(&weighted_push_relabel_shortcut_scenario("fast"))
        .expect("fast weighted push-relabel Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["weighted_push_relabel_shortcut_overlay"],
        solved["weighted_push_relabel_shortcut_overlay"]
    );
}

#[test]
fn weighted_push_relabel_admission_failure_is_reversible_and_repeatable() {
    let mut session = WasmSession::new(&weighted_push_relabel_shortcut_oversized_scenario())
        .expect("oversized weighted push-relabel Scenario");
    let ready = session.current_frame_json().expect("ready serializes");
    let limited = session
        .stage_next_json()
        .expect("resource stages")
        .expect("resource boundary");
    let value: serde_json::Value = serde_json::from_str(&limited).expect("resource JSON");
    assert_eq!(value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("discard resource candidate");
    assert_eq!(
        session
            .stage_next_json()
            .expect("restage")
            .expect("boundary"),
        limited
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn randomized_almost_linear_dispatches_isolation_final_point_rounding_and_fast_parity() {
    let mut session = WasmSession::new(&randomized_almost_linear_scenario("trace"))
        .expect("randomized almost-linear Scenario");
    let ready: serde_json::Value =
        serde_json::from_str(&session.current_frame_json().expect("ready serializes"))
            .expect("ready JSON");
    assert!(ready["randomized_almost_linear_overlay"].is_null());
    let mut return_edge = None;
    let mut initial = None;
    let mut pool = None;
    let mut sample = None;
    let mut query = None;
    let mut potential = None;
    let mut detect = None;
    let mut isolation = None;
    let mut final_point = None;
    let mut rounded = None;
    while session.cursor() < session.item_count() {
        let cursor = session.cursor() + 1;
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("randomized trace JSON");
        match scene["randomized_almost_linear_overlay"]["stage"].as_str() {
            Some("build-return-edge-reduction") => return_edge = Some(scene),
            Some("build-initial-point") => initial = Some(scene),
            Some("enumerate-forest-pool") => pool = Some(scene),
            Some("sample-tree-chain") => sample = Some(scene),
            Some("query-minimum-ratio-cycle") => {
                query.get_or_insert((cursor, scene));
            }
            Some("potential-reduction-step") => {
                potential.get_or_insert(scene);
            }
            Some("detect-changed-coordinates") => {
                detect.get_or_insert(scene);
            }
            Some("sample-isolation-costs") => isolation = Some(scene),
            Some("construct-final-point") => final_point = Some(scene),
            Some("round-nearest-integer") => rounded = Some(scene),
            _ => {}
        }
    }
    let return_edge = return_edge.expect("return reduction");
    assert_eq!(
        return_edge["randomized_almost_linear_overlay"]["return_capacity"],
        "15"
    );
    assert_eq!(
        return_edge["trace_event"]["catalog_id"],
        "randomized-almost-linear-max-flow-oracle-demonstrator.return-edge"
    );
    let initial = initial.expect("initial point");
    assert_eq!(
        initial["randomized_almost_linear_overlay"]["artificial_edges"],
        "2"
    );
    assert_eq!(
        initial["randomized_almost_linear_overlay"]["artificial_flow"],
        "10"
    );
    let pool = pool.expect("forest pool");
    assert!(
        pool["randomized_almost_linear_overlay"]["forest_pool_size"]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|value| value > 1)
    );
    let sample = sample.expect("sample chain");
    assert_eq!(
        sample["randomized_almost_linear_overlay"]["sample_count"],
        "5"
    );
    assert_eq!(
        sample["randomized_almost_linear_overlay"]["random_draws"],
        "5"
    );
    assert!(
        sample["randomized_almost_linear_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges
                .iter()
                .any(|edge| edge["sampled_tree_memberships"] != "0"))
    );
    let (query_cursor, query) = query.expect("ratio query");
    assert!(query["randomized_almost_linear_overlay"]["selected_ratio"].is_string());
    assert!(query["randomized_almost_linear_overlay"]["exact_pool_ratio"].is_string());
    assert!(
        query["randomized_almost_linear_overlay"]["miss_probability"]["denominator"]
            .as_str()
            .is_some_and(|value| value != "0")
    );
    let replayed: serde_json::Value =
        serde_json::from_str(&session.seek_json(query_cursor).expect("seek ratio query"))
            .expect("replayed JSON");
    assert_eq!(replayed, query);
    let potential = potential.expect("potential step");
    assert_eq!(
        potential["randomized_almost_linear_overlay"]["iteration"],
        "1"
    );
    assert!(
        potential["node_trace_states"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().all(|node| node["remaining_divergence"] == "0"))
    );
    assert!(detect.is_some(), "Detect boundary");
    let isolation = isolation.expect("isolation sampling");
    assert!(
        isolation["randomized_almost_linear_overlay"]["isolation_attempt"]
            .as_str()
            .is_some_and(|value| value != "0")
    );
    assert!(
        isolation["randomized_almost_linear_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().all(|edge| edge["isolation_draw"] != "0"))
    );
    let final_point = final_point.expect("source final point");
    let gap = final_point["randomized_almost_linear_overlay"]["final_point_gap"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .expect("final-point gap");
    let threshold = final_point["randomized_almost_linear_overlay"]["final_point_threshold"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .expect("final-point threshold");
    assert!(gap <= threshold);
    let rounded = rounded.expect("nearest-integer rounding");
    assert_eq!(
        rounded["randomized_almost_linear_overlay"]["final_return_flow"],
        "5"
    );
    assert_eq!(
        rounded["randomized_almost_linear_overlay"]["final_artificial_flow"],
        "0"
    );

    let solved: serde_json::Value =
        serde_json::from_str(&session.seek_json(session.item_count()).expect("seek final"))
            .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "max-flow");
    assert_eq!(solved["outcome"]["value"], "5");
    assert_eq!(
        solved["randomized_almost_linear_overlay"]["stage"],
        "optimal"
    );
    assert_eq!(solved["metrics"][0], "4");

    let mut fast = WasmSession::new(&randomized_almost_linear_scenario("fast"))
        .expect("fast randomized Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["randomized_almost_linear_overlay"],
        solved["randomized_almost_linear_overlay"]
    );
}

#[test]
fn randomized_almost_linear_admission_failure_is_reversible_and_repeatable() {
    let mut session = WasmSession::new(&randomized_almost_linear_oversized_scenario())
        .expect("oversized randomized Scenario");
    let ready = session.current_frame_json().expect("ready serializes");
    let limited = session
        .stage_next_json()
        .expect("resource stages")
        .expect("resource boundary");
    let value: serde_json::Value = serde_json::from_str(&limited).expect("resource JSON");
    assert_eq!(value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("discard resource candidate");
    assert_eq!(
        session
            .stage_next_json()
            .expect("restage")
            .expect("boundary"),
        limited
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn deterministic_almost_linear_dispatches_core_spanner_final_point_rounding_and_fast_parity() {
    let mut session = WasmSession::new(&deterministic_almost_linear_scenario("trace"))
        .expect("deterministic almost-linear Scenario");
    let ready: serde_json::Value =
        serde_json::from_str(&session.current_frame_json().expect("ready serializes"))
            .expect("ready JSON");
    assert!(ready["deterministic_almost_linear_overlay"].is_null());
    let mut branches = None;
    let mut core = None;
    let mut spanner = None;
    let mut query = None;
    let mut potential = None;
    let mut feasible_set = None;
    let mut final_point = None;
    let mut rounding_operation = None;
    let mut rounded = None;
    while session.cursor() < session.item_count() {
        let cursor = session.cursor() + 1;
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("deterministic trace JSON");
        match scene["deterministic_almost_linear_overlay"]["stage"].as_str() {
            Some("build-branch-collection") => {
                branches.get_or_insert(scene);
            }
            Some("build-core-graph") => {
                core.get_or_insert(scene);
            }
            Some("build-spanner-embedding") => {
                spanner.get_or_insert(scene);
            }
            Some("query-minimum-ratio-cycle") => {
                query.get_or_insert((cursor, scene));
            }
            Some("potential-reduction-step") => {
                potential.get_or_insert(scene);
            }
            Some("enumerate-feasible-set") => feasible_set = Some(scene),
            Some("construct-final-point") => final_point = Some(scene),
            Some(
                "rounding-integral-edge"
                | "rounding-link-fractional-edge"
                | "rounding-cancel-fractional-cycle",
            ) => {
                rounding_operation.get_or_insert(scene);
            }
            Some("finish-flow-rounding") => rounded = Some(scene),
            _ => {}
        }
    }
    let branches = branches.expect("branch collection");
    assert_eq!(
        branches["deterministic_almost_linear_overlay"]["level_count"],
        "2"
    );
    assert_eq!(
        branches["deterministic_almost_linear_overlay"]["branch_count"],
        "3"
    );
    assert_eq!(
        branches["deterministic_almost_linear_overlay"]["active_branches"],
        serde_json::json!(["0", "0"])
    );
    let core = core.expect("contracted core");
    assert!(
        core["deterministic_almost_linear_overlay"]["core_edges"]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|value| value > 0)
    );
    let spanner = spanner.expect("spanner embedding");
    assert!(
        spanner["deterministic_almost_linear_overlay"]["spanner_edges"]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|value| value > 0)
    );
    assert!(
        spanner["deterministic_almost_linear_overlay"]["edges"]
            .as_array()
            .is_some_and(|edges| edges.iter().any(|edge| edge["active_core_edge"] == true))
    );
    let (query_cursor, query) = query.expect("ratio query");
    assert!(query["deterministic_almost_linear_overlay"]["selected_ratio"].is_string());
    assert!(query["deterministic_almost_linear_overlay"]["selected_cycle_kind"].is_string());
    let replayed: serde_json::Value =
        serde_json::from_str(&session.seek_json(query_cursor).expect("seek ratio query"))
            .expect("replayed JSON");
    assert_eq!(replayed, query);
    let potential = potential.expect("potential step");
    assert_eq!(
        potential["deterministic_almost_linear_overlay"]["iteration"],
        "1"
    );
    assert!(
        potential["node_trace_states"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().all(|node| node["remaining_divergence"] == "0"))
    );
    let feasible_set = feasible_set.expect("bounded feasible set");
    assert!(
        feasible_set["trace_event"]["detail"]["value"]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|value| value > 0)
    );
    let final_point = final_point.expect("deterministic additive-half final point");
    let gap = &final_point["deterministic_almost_linear_overlay"]["final_point_gap"];
    let threshold = &final_point["deterministic_almost_linear_overlay"]["final_point_threshold"];
    let gap_numerator = gap["numerator"]
        .as_str()
        .and_then(|value| value.parse::<i128>().ok())
        .expect("gap numerator");
    let gap_denominator = gap["denominator"]
        .as_str()
        .and_then(|value| value.parse::<i128>().ok())
        .expect("gap denominator");
    let threshold_numerator = threshold["numerator"]
        .as_str()
        .and_then(|value| value.parse::<i128>().ok())
        .expect("threshold numerator");
    let threshold_denominator = threshold["denominator"]
        .as_str()
        .and_then(|value| value.parse::<i128>().ok())
        .expect("threshold denominator");
    assert!(gap_numerator * threshold_denominator < threshold_numerator * gap_denominator);
    let rounding_operation = rounding_operation.expect("Kang--Payor rounding operation");
    assert!(
        rounding_operation["deterministic_almost_linear_overlay"]["rounding_processed_edge"]
            .is_string()
    );
    let repair = rounded.expect("completed deterministic flow rounding");
    assert_eq!(
        repair["deterministic_almost_linear_overlay"]["final_return_flow"],
        "5"
    );
    assert_eq!(
        repair["deterministic_almost_linear_overlay"]["final_artificial_flow"],
        "0"
    );

    let solved: serde_json::Value =
        serde_json::from_str(&session.seek_json(session.item_count()).expect("seek final"))
            .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "5");
    assert_eq!(
        solved["deterministic_almost_linear_overlay"]["stage"],
        "optimal"
    );
    assert_ne!(solved["metrics"][3], "0");
    assert_ne!(solved["metrics"][4], "0");
    assert_ne!(solved["metrics"][5], "0");

    let mut fast = WasmSession::new(&deterministic_almost_linear_scenario("fast"))
        .expect("fast deterministic Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["deterministic_almost_linear_overlay"],
        solved["deterministic_almost_linear_overlay"]
    );
}

#[test]
fn deterministic_almost_linear_admission_failure_is_reversible_and_repeatable() {
    let mut session = WasmSession::new(&deterministic_almost_linear_oversized_scenario())
        .expect("oversized deterministic Scenario");
    let ready = session.current_frame_json().expect("ready serializes");
    let limited = session
        .stage_next_json()
        .expect("resource stages")
        .expect("resource boundary");
    let value: serde_json::Value = serde_json::from_str(&limited).expect("resource JSON");
    assert_eq!(value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("discard resource candidate");
    assert_eq!(
        session
            .stage_next_json()
            .expect("restage")
            .expect("boundary"),
        limited
    );
}

#[test]
fn enhanced_capacity_scaling_admission_failure_is_public_and_reversible() {
    let mut session = WasmSession::new(&enhanced_capacity_scaling_oversized_scenario())
        .expect("oversized enhanced capacity scaling Scenario");
    let ready = session
        .current_frame_json()
        .expect("ready boundary serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let limited_value: serde_json::Value =
        serde_json::from_str(&limited).expect("resource boundary JSON");
    assert_eq!(limited_value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready is unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("resource candidate can be discarded");
    assert_eq!(
        session.current_frame_json().expect("ready after discard"),
        ready
    );
    assert_eq!(
        session
            .stage_next_json()
            .expect("resource boundary restages")
            .expect("resource boundary exists again"),
        limited
    );
}

#[test]
fn dual_network_simplex_dispatches_tree_cut_price_and_signed_basic_flow() {
    let mut session = WasmSession::new(&dual_network_simplex_scenario("trace"))
        .expect("dual network simplex Scenario");
    let mut initialized = None;
    let mut leaving = None;
    let mut entering = None;
    let mut pivot = None;
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("trace frame JSON");
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("dual-network-simplex.initialize-dual-tree") => {
                initialized.get_or_insert(scene);
            }
            Some("dual-network-simplex.select-leaving") => {
                leaving.get_or_insert((session.cursor(), scene));
            }
            Some("dual-network-simplex.select-entering") => {
                entering.get_or_insert(scene);
            }
            Some("dual-network-simplex.pivot") => {
                pivot.get_or_insert(scene);
            }
            _ => {}
        }
    }

    let initialized = initialized.expect("dual-feasible tree initialization exists");
    assert_dual_simplex_initialized_scene(&initialized);

    let (leaving_cursor, leaving) = leaving.expect("negative basic tree arc exists");
    assert_dual_simplex_leaving_scene(&leaving);
    let leaving_edge = &leaving["dual_network_simplex_overlay"]["leaving_edge"];
    assert!(
        leaving["trace_event_semantics"]["changed_entity_refs"]
            .as_array()
            .expect("leaving changed entities")
            .iter()
            .any(|entity| entity["kind"] == "edge" && entity["edge_id"] == *leaving_edge)
    );
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(leaving_cursor)
            .expect("seek leaving boundary"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, leaving);

    let entering = entering.expect("minimum reduced-cost cut arc exists");
    assert!(entering["dual_network_simplex_overlay"]["entering_edge"].is_string());
    let entering_edge = &entering["dual_network_simplex_overlay"]["entering_edge"];
    assert!(
        entering["trace_event"]["entity_refs"]
            .as_array()
            .expect("entering focus entities")
            .iter()
            .any(|entity| entity["kind"] == "edge" && entity["edge_id"] == *entering_edge)
    );
    assert!(entering["dual_network_simplex_overlay"]["pivot_price_delta"].is_string());
    let pivot = pivot.expect("basis exchange exists");
    assert_dual_simplex_pivot_scene(&pivot);

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final dual network simplex frame"),
    )
    .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "20");
    assert_eq!(solved["dual_network_simplex_overlay"]["stage"], "optimal");
    assert_ne!(solved["metrics"][0], "0");
    assert_ne!(solved["metrics"][3], "0");

    let mut fast = WasmSession::new(&dual_network_simplex_scenario("fast"))
        .expect("fast dual network simplex Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast result JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["dual_network_simplex_overlay"],
        solved["dual_network_simplex_overlay"]
    );
}

#[test]
fn dual_network_simplex_admission_failure_is_public_and_reversible() {
    let mut session = WasmSession::new(&dual_network_simplex_oversized_scenario())
        .expect("oversized dual network simplex Scenario");
    let ready = session
        .current_frame_json()
        .expect("ready boundary serializes");
    let limited = session
        .stage_next_json()
        .expect("resource boundary stages")
        .expect("resource boundary exists");
    let limited_value: serde_json::Value =
        serde_json::from_str(&limited).expect("resource boundary JSON");
    assert_eq!(limited_value["solve_status"], "resource-limit");
    assert_eq!(
        session.current_frame_json().expect("ready is unchanged"),
        ready
    );
    session
        .discard_staged_next()
        .expect("resource candidate can be discarded");
    assert_eq!(
        session.current_frame_json().expect("ready after discard"),
        ready
    );
    assert_eq!(
        session
            .stage_next_json()
            .expect("resource boundary restages")
            .expect("resource boundary exists again"),
        limited
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn polynomial_dual_simplex_dispatches_exact_scaling_and_make_good_state() {
    let source = polynomial_dual_simplex_scenario("trace");
    let mut session = WasmSession::new(&source).expect("polynomial dual simplex Scenario");
    let mut initialized = None;
    let mut pseudoflow = None;
    let mut begin = None;
    let mut active = None;
    let mut augmented = None;
    let mut bad = None;
    let mut entering = None;
    let mut pivot = None;
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("trace frame JSON");
        assert_shared_trace_detail_is_integer(&scene);
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("polynomial-dual-network-simplex.initialize-dual-tree") => {
                initialized.get_or_insert(scene);
            }
            Some("polynomial-dual-network-simplex.initialize-pseudoflow") => {
                pseudoflow.get_or_insert(scene);
            }
            Some("polynomial-dual-network-simplex.begin-delta-scale") => {
                begin.get_or_insert((session.cursor(), scene));
            }
            Some("polynomial-dual-network-simplex.select-active-node") => {
                active.get_or_insert(scene);
            }
            Some("polynomial-dual-network-simplex.augment-to-root") => {
                augmented.get_or_insert(scene);
            }
            Some("polynomial-dual-network-simplex.select-bad-subtree") => {
                bad.get_or_insert(scene);
            }
            Some("polynomial-dual-network-simplex.select-entering-arc") => {
                entering.get_or_insert(scene);
            }
            Some("polynomial-dual-network-simplex.pivot-make-good") => {
                pivot.get_or_insert(scene);
            }
            _ => {}
        }
    }

    let initialized = initialized.expect("dual-feasible initial tree exists");
    assert_eq!(
        initialized["polynomial_dual_simplex_overlay"]["edges"]
            .as_array()
            .expect("edge states")
            .iter()
            .filter(|edge| edge["in_tree"] == true)
            .count(),
        2
    );
    let pseudoflow = pseudoflow.expect("initial tree pseudoflow exists");
    assert!(
        pseudoflow["polynomial_dual_simplex_overlay"]["edges"]
            .as_array()
            .expect("edge states")
            .iter()
            .any(|edge| edge["pseudoflow"]["numerator"] != "0")
    );
    let (begin_cursor, begin) = begin.expect("delta phase exists");
    assert!(begin["polynomial_dual_simplex_overlay"]["delta"]["denominator"].is_string());
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(begin_cursor)
            .expect("seek exact delta phase"),
    )
    .expect("replayed JSON");
    assert_eq!(replayed, begin);

    let active = active.expect("active node selection exists");
    assert!(active["polynomial_dual_simplex_overlay"]["active_node"].is_string());
    assert!(
        active["polynomial_dual_simplex_overlay"]["augment_path"]
            .as_array()
            .is_some_and(|path| !path.is_empty())
    );
    let augmented = augmented.expect("exact-delta root augmentation exists");
    assert_eq!(
        augmented["polynomial_dual_simplex_overlay"]["augment_path"],
        active["polynomial_dual_simplex_overlay"]["augment_path"]
    );
    let bad = bad.expect("Make-Good bad subtree exists");
    assert!(bad["polynomial_dual_simplex_overlay"]["leaving_edge"].is_string());
    assert!(
        bad["polynomial_dual_simplex_overlay"]["bad_nodes"]
            .as_array()
            .is_some_and(|nodes| !nodes.is_empty())
    );
    let entering = entering.expect("Make-Good entering arc exists");
    assert!(entering["polynomial_dual_simplex_overlay"]["entering_edge"].is_string());
    assert!(entering["polynomial_dual_simplex_overlay"]["pivot_price_delta"].is_string());
    let pivot = pivot.expect("Make-Good basis exchange exists");
    assert_ne!(
        pivot["polynomial_dual_simplex_overlay"]["leaving_edge"],
        pivot["polynomial_dual_simplex_overlay"]["entering_edge"]
    );

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek polynomial dual optimum"),
    )
    .expect("optimal JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(
        solved["polynomial_dual_simplex_overlay"]["stage"],
        "optimal"
    );
    assert_ne!(solved["metrics"][0], "0");
    assert_ne!(solved["metrics"][1], "0");
    assert_ne!(solved["metrics"][2], "0");

    let mut fast = WasmSession::new(&polynomial_dual_simplex_scenario("fast"))
        .expect("fast polynomial dual simplex Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["polynomial_dual_simplex_overlay"],
        solved["polynomial_dual_simplex_overlay"]
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn polynomial_primal_simplex_dispatches_scaling_premultipliers_and_artificial_basis() {
    let mut session = WasmSession::new(&polynomial_primal_simplex_scenario("trace"))
        .expect("polynomial primal simplex Scenario");
    let mut initialized = None;
    let mut begin = None;
    let mut admissible = None;
    let mut pivot = None;
    let mut modify = None;
    let mut scan_ordinals = Vec::new();
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("trace frame JSON");
        assert_shared_trace_detail_is_integer(&scene);
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("polynomial-primal-network-simplex.initialize-perturbed-basis") => {
                initialized.get_or_insert(scene);
            }
            Some("polynomial-primal-network-simplex.begin-epsilon-scale") => {
                begin.get_or_insert((session.cursor(), scene));
            }
            Some("polynomial-primal-network-simplex.select-admissible-arc") => {
                admissible.get_or_insert(scene);
            }
            Some("polynomial-primal-network-simplex.pivot-fundamental-cycle") => {
                pivot.get_or_insert(scene);
            }
            Some("polynomial-primal-network-simplex.modify-epsilon-premultipliers") => {
                modify.get_or_insert(scene);
            }
            Some("polynomial-primal-network-simplex.inspect-extended-arc") => {
                assert!(
                    scene["trace_event"]["detail"]["label"]
                        .as_str()
                        .is_some_and(|label| label.ends_with("scan ordinal"))
                );
                scan_ordinals.push(
                    scene["trace_event"]["detail"]["value"]
                        .as_str()
                        .expect("source scan ordinal")
                        .parse::<u128>()
                        .expect("canonical source scan ordinal"),
                );
            }
            _ => {}
        }
    }
    assert_eq!(
        scan_ordinals,
        (1..=u128::try_from(scan_ordinals.len()).expect("scan count")).collect::<Vec<_>>()
    );
    let initialized = initialized.expect("perturbed star initialization exists");
    assert_eq!(
        initialized["polynomial_primal_simplex_overlay"]["nodes"]
            .as_array()
            .map(Vec::len),
        Some(4)
    );
    assert_eq!(
        initialized["polynomial_primal_simplex_overlay"]["artificial_edges"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );
    let (begin_cursor, begin) = begin.expect("epsilon phase exists");
    assert!(begin["polynomial_primal_simplex_overlay"]["epsilon"]["denominator"].is_string());
    assert!(
        begin["polynomial_primal_simplex_overlay"]["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node["flags"]
                .as_array()
                .is_some_and(|flags| flags.iter().any(|flag| flag == "in-n-star"))))
    );
    let replayed: serde_json::Value =
        serde_json::from_str(&session.seek_json(begin_cursor).expect("seek begin scale"))
            .expect("replayed begin JSON");
    assert_eq!(replayed, begin);
    let admissible = admissible.expect("admissible selection exists");
    assert_eq!(
        admissible["trace_event"]["entity_refs"]
            .as_array()
            .expect("local entering focus")
            .len(),
        1
    );
    assert!(admissible["polynomial_primal_simplex_overlay"]["entering"].is_object());
    assert!(
        admissible["polynomial_primal_simplex_overlay"]["cycle"]
            .as_array()
            .is_some_and(|cycle| !cycle.is_empty())
    );
    let pivot = pivot.expect("fundamental-cycle pivot exists");
    assert_eq!(
        pivot["trace_event"]["entity_refs"]
            .as_array()
            .expect("local pivot focus")
            .len(),
        1
    );
    assert!(pivot["polynomial_primal_simplex_overlay"]["delta"].is_object());
    let modify = modify.expect("premultiplier modification exists");
    assert_eq!(
        modify["trace_event"]["entity_refs"]
            .as_array()
            .expect("local premultiplier focus")
            .len(),
        1
    );
    assert!(modify["polynomial_primal_simplex_overlay"]["potential_shift"].is_object());

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek polynomial simplex optimum"),
    )
    .expect("optimal JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "15");
    assert_eq!(
        solved["polynomial_primal_simplex_overlay"]["stage"],
        "optimal"
    );
    assert_ne!(solved["metrics"][0], "0");
    assert_ne!(solved["metrics"][1], "0");

    let mut fast = WasmSession::new(&polynomial_primal_simplex_scenario("fast"))
        .expect("fast polynomial primal simplex Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast result JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["polynomial_primal_simplex_overlay"],
        solved["polynomial_primal_simplex_overlay"]
    );
}

#[test]
fn double_scaling_dispatches_nested_exact_transportation_phases() {
    let source = double_scaling_scenario();
    let mut session = WasmSession::new(&source).expect("Double Scaling Scenario");
    let mut cost_phase = None;
    let mut capacity_phase = None;
    let mut augmentation = None;
    let mut scans = Vec::new();
    while session.cursor() < session.item_count() {
        let scene: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).expect("trace frame JSON");
        match scene["trace_event"]["catalog_id"].as_str() {
            Some("double-scaling.start-cost-phase") => {
                cost_phase.get_or_insert(scene);
            }
            Some("double-scaling.start-capacity-phase") => {
                capacity_phase.get_or_insert(scene);
            }
            Some("double-scaling.augment-exact-delta") => {
                augmentation.get_or_insert(scene);
            }
            Some("double-scaling.inspect-transformed-residual-arc") => scans.push(scene),
            _ => {}
        }
    }
    let cost_phase = cost_phase.expect("cost phase exists");
    assert_eq!(
        cost_phase["double_scaling_overlay"]["stage"],
        "start-cost-phase"
    );
    assert_eq!(cost_phase["double_scaling_overlay"]["cost_phase"], "1");
    assert!(
        cost_phase["double_scaling_overlay"]["nodes"]
            .as_array()
            .expect("transformed nodes")
            .len()
            > 5
    );
    let capacity_phase = capacity_phase.expect("capacity phase exists");
    assert_ne!(capacity_phase["double_scaling_overlay"]["delta"], "0");
    let augmentation = augmentation.expect("augmentation exists");
    assert!(
        augmentation["double_scaling_overlay"]["active_path"]
            .as_array()
            .expect("active path")
            .iter()
            .all(|arc| arc["branch"] == "flow" || arc["branch"] == "slack")
    );
    assert!(augmentation["double_scaling_overlay"]["selected_root"].is_string());
    assert!(augmentation["double_scaling_overlay"]["selected_deficit"].is_string());
    assert!(!scans.is_empty());
    for scene in scans {
        assert_eq!(scene["double_scaling_overlay"]["stage"], "inspect-arc");
        assert!(scene["double_scaling_overlay"]["inspected_arc"].is_object());
        assert_eq!(scene["trace_event"]["minimum_granularity"], "micro");
        let refs = scene["trace_event"]["entity_refs"]
            .as_array()
            .expect("scan entity refs");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0]["kind"], "residual-arc");
    }

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(session.item_count())
            .expect("seek final double-scaling frame"),
    )
    .expect("final JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["total_cost"], "-4");
    assert_eq!(solved["double_scaling_overlay"]["stage"], "optimal");
    assert_eq!(solved["double_scaling_overlay"]["epsilon"], "1");
    assert_ne!(solved["metrics"][0], "0");
    assert_ne!(solved["metrics"][1], "0");
    assert_ne!(solved["metrics"][2], "0");
    assert_ne!(solved["metrics"][3], "0");

    let mut fast_source: serde_json::Value = serde_json::from_str(&source).expect("scenario");
    fast_source["payload"]["run_profile"] = serde_json::json!("fast");
    let mut fast = WasmSession::new(&fast_source.to_string()).expect("fast Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value = serde_json::from_str(
        &fast
            .current_frame_json()
            .expect("fast double-scaling result serializes"),
    )
    .expect("fast result JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
    assert_eq!(
        fast_scene["double_scaling_overlay"],
        solved["double_scaling_overlay"]
    );
}

#[test]
fn hidden_initial_build_is_present_in_first_scene_but_not_timeline() {
    let session = WasmSession::new(&scenario(false)).unwrap();
    assert_eq!(session.item_count(), 2);
    let frame: serde_json::Value =
        serde_json::from_str(&session.current_frame_json().unwrap()).unwrap();
    assert_eq!(frame["canonical"]["entries"].as_array().unwrap().len(), 2);
}

#[test]
fn static_session_dispatches_flow_without_changing_ordered_map_contracts() {
    let mut session = WasmSession::new(&flow_scenario()).expect("flow Scenario is valid");

    assert_eq!(session.plugin_id(), "flow");
    assert_eq!(session.plugin_ordinal(), 2);
    assert_eq!(session.transport_version(), 6);
    assert_eq!(session.algorithm_id(), "edmonds-karp");
    assert_eq!(session.event_cursor(), "0");
    let current_json = session.current_frame_json().expect("scene serializes");
    let current: serde_json::Value = serde_json::from_str(&current_json).expect("scene is JSON");
    assert_eq!(current["frame_revision"], "flow-scene/9");
    assert_eq!(current["result_schema_version"], 9);
    assert_eq!(current["solve_status"], "ready");
    assert_eq!(current["graph"]["edges"][0]["capacity"], "9");
    assert_eq!(current["residual_arcs"].as_array().map(Vec::len), Some(2));

    let staged_event = session
        .stage_next_json()
        .expect("stage is valid")
        .expect("dummy flow event exists");
    assert_eq!(session.event_cursor(), "0");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&staged_event).expect("staged scene is JSON")["event_id"],
        "1"
    );
    let solved: serde_json::Value =
        serde_json::from_str(&staged_event).expect("staged scene is JSON");
    assert_eq!(solved["solve_status"], "running");
    assert_eq!(solved["trace_event"]["catalog_id"], "edmonds-karp.bfs");
    session
        .discard_staged_next()
        .expect("candidate rejects cleanly");
    assert_eq!(
        session.current_frame_json().expect("scene serializes"),
        current_json
    );

    session
        .stage_next_json()
        .expect("stage is valid")
        .expect("dummy flow event exists");
    session.commit_staged_next();
    assert_eq!(session.event_cursor(), "1");

    while session
        .stage_next_json()
        .expect("trace event stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final scene serializes"),
    )
    .expect("final scene is JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "max-flow");
    assert_eq!(solved["outcome"]["value"], "9");
    assert_eq!(solved["edge_states"][0]["flow"], "9");
    assert_eq!(solved["metrics"][0], "2");
    let event_count = solved["event_count"].clone();
    let committed = session.current_frame_json().expect("scene serializes");

    session
        .begin_seek(0)
        .expect("current flow cursor is seekable");
    let staged: serde_json::Value =
        serde_json::from_str(&session.resume_seek_json(1).expect("seek resumes"))
            .expect("seek progress is JSON");
    assert_eq!(staged["done"], true);
    session.discard_staged_seek();
    assert_eq!(
        session.current_frame_json().expect("scene serializes"),
        committed
    );

    session.begin_seek(0).expect("start cursor is seekable");
    session
        .resume_seek_json(1)
        .expect("seek candidate completes");
    session.commit_staged_seek();
    assert_eq!(session.event_cursor(), "0");
    let base: serde_json::Value =
        serde_json::from_str(&session.current_frame_json().expect("base scene serializes"))
            .expect("base scene is JSON");
    assert_eq!(base["event_id"], "0");
    assert_eq!(base["event_count"], event_count);
    assert_eq!(base["edge_states"][0]["flow"], "0");
}

fn assert_parametric_terminal_overlay_complete(frame: &serde_json::Value) {
    assert_eq!(
        frame["parametric_overlay"]["recorded_segments"],
        frame["outcome"]["segments"]
    );
    assert_eq!(
        frame["parametric_overlay"]["recorded_breakpoints"],
        frame["outcome"]["breakpoints"]
    );
}

#[test]
fn parametric_trace_fast_and_cold_oracle_cross_the_v9_scene_boundary() {
    let canonical = FlowSession::new(&parametric_scenario("parametric-pseudoflow", "trace"))
        .expect("canonical parametric session");
    let canonical_frames = canonical
        .prepare_frames()
        .expect("canonical trace projects");
    let ready = serde_json::to_value(&canonical_frames[0]).expect("ready scene serializes");
    assert_eq!(ready["result_schema_version"], 9);
    assert_eq!(ready["frame_revision"], "flow-scene/9");
    assert_eq!(ready["edge_states"].as_array().map(Vec::len), Some(0));
    assert_eq!(ready["residual_arcs"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        ready["parametric_overlay"]["edge_capacities"][1]["capacity"]["numerator"],
        "1"
    );
    assert_eq!(
        ready["parametric_overlay"]["visual_scale_max_capacity"]["numerator"],
        "9"
    );
    let canonical_first_event =
        serde_json::to_value(&canonical_frames[1]).expect("first event serializes");
    let first_traversal = &canonical_first_event["parametric_overlay"]["traversal"];
    assert_eq!(first_traversal["cold_static_rerun"], false);
    assert_eq!(
        first_traversal["lower_source_side"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    assert_eq!(
        first_traversal["upper_source_side"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    assert!(canonical_frames.iter().skip(1).any(|scene| {
        scene
            .parametric_overlay
            .as_ref()
            .and_then(|overlay| overlay.traversal.as_ref())
            .is_some_and(|traversal| traversal.normalized_tree_reused)
    }));
    let canonical_final = serde_json::to_value(
        canonical_frames
            .last()
            .expect("canonical trace has an optimal frame"),
    )
    .expect("canonical optimum serializes");
    assert_eq!(canonical_final["solve_status"], "optimal");
    assert_eq!(
        canonical_final["outcome"]["segments"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        canonical_final["outcome"]["breakpoints"][0]["parameter"]["numerator"],
        "2"
    );
    assert_eq!(
        canonical_final["outcome"]["metrics"]["implementation"],
        "parametric-pseudoflow"
    );
    assert_parametric_terminal_overlay_complete(&canonical_final);

    let fast = FlowSession::new(&parametric_scenario("parametric-pseudoflow", "fast"))
        .expect("fast parametric session");
    let fast_frames = fast.prepare_frames().expect("fast analysis projects");
    assert_eq!(fast_frames.len(), 2);
    let fast_final = serde_json::to_value(fast_frames.last().expect("fast optimum frame"))
        .expect("fast optimum serializes");
    assert_eq!(fast_final["outcome"], canonical_final["outcome"]);

    let cold = FlowSession::new(&parametric_scenario("parametric-breakpoint-rerun", "trace"))
        .expect("cold oracle session");
    let cold_frames = cold.prepare_frames().expect("cold trace projects");
    assert!(cold_frames.iter().skip(1).any(|scene| {
        scene
            .parametric_overlay
            .as_ref()
            .and_then(|overlay| overlay.traversal.as_ref())
            .is_some_and(|traversal| {
                traversal.cold_static_rerun && !traversal.normalized_tree_reused
            })
    }));
    let cold_final = serde_json::to_value(cold_frames.last().expect("cold optimum frame"))
        .expect("cold optimum serializes");
    assert_eq!(
        cold_final["outcome"]["metrics"]["implementation"],
        "breakpoint-rerun"
    );
    assert_eq!(
        cold_final["outcome"]["segments"],
        canonical_final["outcome"]["segments"]
    );
    assert_parametric_terminal_overlay_complete(&cold_final);
}

#[test]
fn ibfs_fast_trace_forest_metrics_and_seek_are_exact() {
    let mut trace = WasmSession::new(&ibfs_scenario("trace")).expect("IBFS trace Scenario");
    let base: serde_json::Value =
        serde_json::from_str(&trace.current_frame_json().expect("IBFS base serializes"))
            .expect("IBFS base JSON");
    assert_eq!(base["solve_status"], "ready");

    let mut frames = vec![base];
    let mut event_ids = Vec::new();
    while trace
        .stage_next_json()
        .expect("IBFS event stages")
        .is_some()
    {
        trace.commit_staged_next();
        let frame: serde_json::Value =
            serde_json::from_str(&trace.current_frame_json().expect("IBFS frame serializes"))
                .expect("IBFS frame JSON");
        event_ids.push(
            frame["trace_event"]["catalog_id"]
                .as_str()
                .expect("IBFS event id")
                .to_owned(),
        );
        frames.push(frame);
    }
    assert!(event_ids.iter().any(|id| id == "ibfs.start-forward-pass"));
    assert!(event_ids.iter().any(|id| id == "ibfs.start-reverse-pass"));
    assert!(
        event_ids
            .iter()
            .any(|id| id == "ibfs.augment-shortest-path")
    );
    assert!(frames.iter().all(|frame| {
        frame
            .get("pseudoflow_forest")
            .is_none_or(|forest| forest["strong_nodes"].as_array().is_some_and(Vec::is_empty))
    }));
    let adoption_index = event_ids
        .iter()
        .position(|id| id == "ibfs.adopt-source-orphan")
        .map(|index| index + 1)
        .expect("source orphan adoption event");
    let adoption = &frames[adoption_index];
    assert!(
        adoption["pseudoflow_forest"]["arcs"]
            .as_array()
            .expect("IBFS forest arcs")
            .iter()
            .any(|arc| arc["edge_id"] == "bc" && arc["direction"] == "forward")
    );
    let c_state = adoption["node_trace_states"]
        .as_array()
        .expect("IBFS node states")
        .iter()
        .find(|node| node["node_id"] == "c")
        .expect("c state");
    assert_eq!(c_state["label"], "2");
    assert_eq!(c_state["search_ordinal"], 0);

    let solved = frames.last().expect("IBFS solved frame");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "max-flow");
    assert_eq!(solved["outcome"]["value"], "2");
    assert!(
        solved["metrics"][0]
            .as_str()
            .is_some_and(|value| value != "0")
    );
    assert!(
        solved["metrics"][10]
            .as_str()
            .is_some_and(|value| value != "0")
    );
    assert!(
        solved["metrics"][13]
            .as_str()
            .is_some_and(|value| value != "0")
    );

    trace
        .begin_seek(adoption_index)
        .expect("IBFS adoption cursor is seekable");
    trace
        .resume_seek_json(1)
        .expect("IBFS adoption seek completes");
    trace.commit_staged_seek();
    let sought: serde_json::Value = serde_json::from_str(
        &trace
            .current_frame_json()
            .expect("sought IBFS frame serializes"),
    )
    .expect("sought IBFS frame JSON");
    assert_eq!(sought, *adoption);

    let mut fast = WasmSession::new(&ibfs_scenario("fast")).expect("IBFS fast Scenario");
    let fast_result: serde_json::Value =
        serde_json::from_str(&commit_next(&mut fast)).expect("IBFS fast result JSON");
    assert_eq!(fast_result["solve_status"], "optimal");
    assert_eq!(fast_result["outcome"], solved["outcome"]);
    assert_eq!(fast_result["metrics"], solved["metrics"]);
    assert_eq!(fast_result["edge_states"], solved["edge_states"]);
    assert_eq!(fast_result["residual_arcs"], solved["residual_arcs"]);
}

#[test]
fn ibfs_sink_adoption_projects_parent_to_child_reverse_identity() {
    let mut session = WasmSession::new(&ibfs_sink_orphan_scenario()).expect("sink IBFS Scenario");
    let mut adoption = None;
    while let Some(frame) = session.stage_next_json().expect("IBFS event stages") {
        let value: serde_json::Value = serde_json::from_str(&frame).expect("IBFS frame is JSON");
        if value["trace_event"]["catalog_id"] == "ibfs.adopt-sink-orphan" {
            adoption = Some(value.clone());
        }
        assert!(
            value.get("pseudoflow_forest").is_none_or(|forest| {
                forest["strong_nodes"].as_array().is_some_and(Vec::is_empty)
            })
        );
        session.commit_staged_next();
    }
    let adoption = adoption.expect("sink adoption frame");
    assert!(
        adoption["pseudoflow_forest"]["arcs"]
            .as_array()
            .expect("forest arcs")
            .iter()
            .any(|arc| arc["edge_id"] == "cb" && arc["direction"] == "reverse")
    );
}

#[test]
fn ibfs_node_admission_limit_maps_to_public_resource_limit() {
    let mut session =
        WasmSession::new(&ibfs_oversized_scenario()).expect("oversized IBFS Scenario");
    let limited: serde_json::Value = serde_json::from_str(
        &session
            .stage_next_json()
            .expect("resource limit stages")
            .expect("resource limit frame"),
    )
    .expect("resource limit JSON");
    assert_eq!(limited["solve_status"], "resource-limit");
}

fn collect_eibfs_trace() -> (WasmSession, Vec<serde_json::Value>, Vec<String>) {
    let mut trace = WasmSession::new(&eibfs_scenario("trace")).expect("EIBFS trace Scenario");
    let base: serde_json::Value =
        serde_json::from_str(&trace.current_frame_json().expect("EIBFS base serializes"))
            .expect("EIBFS base JSON");
    assert_eq!(base["solve_status"], "ready");
    assert!(base.get("eibfs_overlay").is_none());
    let mut frames = vec![base];
    let mut event_ids = Vec::new();
    while trace
        .stage_next_json()
        .expect("EIBFS event stages")
        .is_some()
    {
        trace.commit_staged_next();
        let frame: serde_json::Value =
            serde_json::from_str(&trace.current_frame_json().expect("EIBFS frame serializes"))
                .expect("EIBFS frame JSON");
        event_ids.push(
            frame["trace_event"]["catalog_id"]
                .as_str()
                .expect("EIBFS event id")
                .to_owned(),
        );
        assert!(frame.get("pseudoflow_forest").is_none());
        frames.push(frame);
    }
    (trace, frames, event_ids)
}

fn assert_eibfs_event_catalog(event_ids: &[String]) {
    assert_eq!(
        event_ids.first().map(String::as_str),
        Some("eibfs.initialize-pseudoflow-forests")
    );
    assert!(event_ids.iter().any(|id| id == "eibfs.start-forward-phase"));
    assert!(event_ids.iter().any(|id| id == "eibfs.start-reverse-phase"));
    assert!(
        event_ids
            .iter()
            .any(|id| id.starts_with("eibfs.push-bridge-"))
    );
    assert!(
        event_ids
            .iter()
            .any(|id| id == "eibfs.begin-feasible-flow-recovery")
    );
    assert_eq!(
        event_ids.last().map(String::as_str),
        Some("eibfs.optimal-feasible-flow")
    );
}

fn assert_eibfs_overlay_and_recovery(frames: &[serde_json::Value], event_ids: &[String]) -> usize {
    let overlay_index = frames
        .iter()
        .position(|frame| frame.get("eibfs_overlay").is_some())
        .expect("EIBFS pseudoflow overlay frame");
    let overlay = &frames[overlay_index]["eibfs_overlay"];
    let nodes = overlay["nodes"].as_array().expect("EIBFS nodes");
    assert_eq!(nodes.len(), 6, "every graph node has an EIBFS membership");
    assert!(
        nodes
            .iter()
            .any(|node| node["node_id"] == "s" && node["root_kind"] == "source")
    );
    assert!(
        nodes
            .iter()
            .any(|node| node["node_id"] == "t" && node["root_kind"] == "sink")
    );
    let recovery_index = event_ids
        .iter()
        .position(|id| id == "eibfs.begin-feasible-flow-recovery")
        .map(|index| index + 1)
        .expect("EIBFS recovery event");
    let removed_overlay = frames[recovery_index - 1]["eibfs_overlay"]["nodes"]
        .as_array()
        .expect("pre-recovery EIBFS node projection");
    let recovery_changes = frames[recovery_index]["trace_event_semantics"]["changed_entity_refs"]
        .as_array()
        .expect("recovery changed entities");
    for node in removed_overlay {
        let node_id = &node["node_id"];
        assert!(
            recovery_changes
                .iter()
                .any(|entity| { entity["kind"] == "node" && entity["node_id"] == *node_id })
        );
    }
    assert!(
        frames[recovery_index..]
            .iter()
            .all(|frame| frame.get("eibfs_overlay").is_none())
    );
    overlay_index
}

fn assert_eibfs_solved_frame(solved: &serde_json::Value) {
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "max-flow");
    assert_eq!(solved["outcome"]["value"], "2");
    for metric in [0, 2, 3, 15] {
        assert!(
            solved["metrics"][metric]
                .as_str()
                .is_some_and(|value| value != "0")
        );
    }
}

#[test]
fn eibfs_fast_trace_overlay_recovery_metrics_and_seek_are_exact() {
    let (mut trace, frames, event_ids) = collect_eibfs_trace();
    assert_eibfs_event_catalog(&event_ids);
    let overlay_index = assert_eibfs_overlay_and_recovery(&frames, &event_ids);
    let overlay_frame = &frames[overlay_index];
    let solved = frames.last().expect("EIBFS solved frame");
    assert_eibfs_solved_frame(solved);

    trace
        .begin_seek(overlay_index)
        .expect("EIBFS overlay cursor is seekable");
    trace
        .resume_seek_json(1)
        .expect("EIBFS overlay seek completes");
    trace.commit_staged_seek();
    let sought: serde_json::Value = serde_json::from_str(
        &trace
            .current_frame_json()
            .expect("sought EIBFS frame serializes"),
    )
    .expect("sought EIBFS frame JSON");
    assert_eq!(sought, *overlay_frame);

    let mut fast = WasmSession::new(&eibfs_scenario("fast")).expect("EIBFS fast Scenario");
    let fast_result: serde_json::Value =
        serde_json::from_str(&commit_next(&mut fast)).expect("EIBFS fast result JSON");
    assert_eq!(fast_result["solve_status"], "optimal");
    assert_eq!(fast_result["outcome"], solved["outcome"]);
    assert_eq!(fast_result["metrics"], solved["metrics"]);
    assert_eq!(fast_result["edge_states"], solved["edge_states"]);
    assert_eq!(fast_result["residual_arcs"], solved["residual_arcs"]);
    assert!(fast_result.get("eibfs_overlay").is_none());
}

#[test]
fn eibfs_node_admission_limit_maps_to_public_resource_limit() {
    let mut session =
        WasmSession::new(&eibfs_oversized_scenario()).expect("oversized EIBFS Scenario");
    let limited: serde_json::Value = serde_json::from_str(
        &session
            .stage_next_json()
            .expect("resource limit stages")
            .expect("resource limit frame"),
    )
    .expect("resource limit JSON");
    assert_eq!(limited["solve_status"], "resource-limit");
}

#[test]
fn eibfs_eager_trace_budget_maps_to_public_resource_limit() {
    let session = FlowSession::new(&eibfs_scenario("trace")).expect("EIBFS session");
    let frames = session
        .eibfs_error_frames(EibfsError::Trace(FlowTraceError::EventLimit))
        .expect("resource-limit frames");
    assert_eq!(frames.len(), 2);
    assert!(matches!(
        frames[1].solve_status,
        FlowSolveStatusV1::ResourceLimit
    ));
    assert!(frames[1].eibfs_overlay.is_none());
    assert!(frames[1].outcome.is_none());
}

#[test]
fn dynamic_eibfs_projects_updates_repairs_prefixes_and_fast_result() {
    let mut trace =
        WasmSession::new(&dynamic_eibfs_scenario("trace")).expect("Dynamic EIBFS trace Scenario");
    let mut frames = Vec::new();
    let mut catalog_ids = Vec::new();
    while let Some(frame) = trace.stage_next_json().expect("Dynamic EIBFS event stages") {
        let value: serde_json::Value =
            serde_json::from_str(&frame).expect("Dynamic EIBFS frame JSON");
        catalog_ids.push(
            value["trace_event"]["catalog_id"]
                .as_str()
                .expect("catalog id")
                .to_owned(),
        );
        frames.push(value);
        trace.commit_staged_next();
    }
    assert!(
        catalog_ids
            .iter()
            .any(|id| id == "dynamic-eibfs.apply-capacity-update")
    );
    assert!(
        catalog_ids
            .iter()
            .any(|id| id == "dynamic-eibfs.repair-over-capacity")
    );
    assert!(
        catalog_ids
            .iter()
            .any(|id| id == "dynamic-eibfs.prefix-certified")
    );
    assert_eq!(
        catalog_ids
            .iter()
            .filter(|id| id.as_str() == "dynamic-eibfs.resume-reusable-pseudoflow")
            .count(),
        3
    );
    let overflow = frames
        .iter()
        .find(|frame| {
            frame["dynamic_eibfs_overlay"]["stage"] == "apply-update"
                && frame["dynamic_eibfs_overlay"]["changed_edge"] == "sa"
        })
        .expect("temporary over-capacity frame");
    let sa_flow = overflow["edge_states"]
        .as_array()
        .expect("edge states")
        .iter()
        .find(|edge| edge["edge_id"] == "sa")
        .expect("sa flow")["flow"]
        .as_str()
        .expect("flow")
        .parse::<u64>()
        .expect("u64 flow");
    assert!(sa_flow > 0);
    assert_eq!(
        overflow["graph"]["edges"]
            .as_array()
            .expect("graph edges")
            .iter()
            .find(|edge| edge["id"] == "sa")
            .expect("sa declaration")["capacity"],
        "0"
    );

    let solved = frames.last().expect("final certified prefix");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["dynamic_eibfs_overlay"]["stage"], "prefix-certified");
    assert_eq!(solved["dynamic_eibfs_overlay"]["update_index"], "3");
    assert!(solved.get("eibfs_overlay").is_none());

    let mut fast =
        WasmSession::new(&dynamic_eibfs_scenario("fast")).expect("Dynamic EIBFS fast Scenario");
    let fast_result: serde_json::Value =
        serde_json::from_str(&commit_next(&mut fast)).expect("Dynamic EIBFS fast result JSON");
    assert_eq!(fast_result["outcome"], solved["outcome"]);
    assert_eq!(fast_result["metrics"], solved["metrics"]);
    assert_eq!(fast_result["edge_states"], solved["edge_states"]);
    assert_eq!(
        fast_result["dynamic_eibfs_overlay"],
        solved["dynamic_eibfs_overlay"]
    );
}

#[test]
fn hassin_trace_projects_split_dual_events_metrics_and_certified_flow() {
    let mut session =
        WasmSession::new(&planar_scenario("hassin-st-planar", "trace")).expect("planar Scenario");
    let mut catalog_ids = Vec::new();
    while let Some(frame) = session.stage_next_json().expect("Hassin event stages") {
        let value: serde_json::Value = serde_json::from_str(&frame).expect("Hassin frame is JSON");
        let catalog_id = value["trace_event"]["catalog_id"]
            .as_str()
            .expect("semantic trace event");
        if !catalog_id.ends_with(".work-observation") {
            catalog_ids.push(catalog_id.to_owned());
        }
        session.commit_staged_next();
    }
    assert_eq!(
        catalog_ids,
        [
            "hassin-st-planar.split-outer-face",
            "hassin-st-planar.settle-dual-face",
            "hassin-st-planar.inspect-dual-arc",
            "hassin-st-planar.inspect-dual-arc",
            "hassin-st-planar.settle-dual-face",
            "hassin-st-planar.inspect-dual-arc",
            "hassin-st-planar.inspect-dual-arc",
            "hassin-st-planar.inspect-dual-arc",
            "hassin-st-planar.settle-dual-face",
            "hassin-st-planar.inspect-dual-arc",
            "hassin-st-planar.inspect-dual-arc",
            "hassin-st-planar.inspect-dual-arc",
            "hassin-st-planar.reconstruct-primal-flow",
            "hassin-st-planar.optimal-dual-cut",
        ]
    );
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("Hassin result serializes"),
    )
    .expect("Hassin result is JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "max-flow");
    assert_eq!(solved["outcome"]["value"], "5");
    assert_eq!(solved["metrics"][2], "8");
    assert_eq!(solved["metrics"][4], "1");
    assert_eq!(solved["metrics"][5], "3");
    assert_eq!(solved["metrics"][11], "3");
    assert_eq!(solved["metrics"][15], "3");
    assert_eq!(
        solved["edge_states"]
            .as_array()
            .expect("edge states")
            .iter()
            .map(|edge| edge["flow"].as_str().expect("flow").to_owned())
            .collect::<Vec<_>>(),
        ["3", "2", "3"]
    );

    session.begin_seek(0).expect("Hassin timeline is seekable");
    session
        .resume_seek_json(1)
        .expect("Hassin reverse seek completes");
    session.commit_staged_seek();
    let base: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("Hassin base serializes"),
    )
    .expect("Hassin base is JSON");
    assert_eq!(base["edge_states"][0]["flow"], "0");
}

#[test]
fn hassin_fast_profile_projects_the_same_certificate_and_dual_metrics() {
    let mut session = WasmSession::new(&planar_scenario("hassin-st-planar", "fast"))
        .expect("fast planar Scenario");
    while session
        .stage_next_json()
        .expect("fast Hassin result stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("fast Hassin scene serializes"),
    )
    .expect("fast Hassin scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "5");
    assert_eq!(
        solved["metrics"],
        serde_json::json!([
            "0", "0", "8", "0", "1", "3", "0", "0", "0", "0", "0", "3", "0", "0", "0", "3"
        ])
    );
}

#[test]
fn borradaile_klein_trace_projects_leftmost_paths_and_certified_flow() {
    let mut session = WasmSession::new(&planar_scenario("borradaile-klein-planar", "trace"))
        .expect("leftmost planar Scenario");
    let mut catalog_ids = Vec::new();
    while let Some(frame) = session
        .stage_next_json()
        .expect("Borradaile-Klein event stages")
    {
        let value: serde_json::Value =
            serde_json::from_str(&frame).expect("Borradaile-Klein frame is JSON");
        let catalog_id = value["trace_event"]["catalog_id"]
            .as_str()
            .expect("semantic trace event");
        if !catalog_id.ends_with(".work-observation") {
            catalog_ids.push(catalog_id.to_owned());
        }
        session.commit_staged_next();
    }
    assert_eq!(
        catalog_ids,
        [
            "borradaile-klein-planar.inspect-dual-arc",
            "borradaile-klein-planar.inspect-dual-arc",
            "borradaile-klein-planar.inspect-dual-arc",
            "borradaile-klein-planar.inspect-dual-arc",
            "borradaile-klein-planar.inspect-dual-arc",
            "borradaile-klein-planar.inspect-dual-arc",
            "borradaile-klein-planar.preprocess-clockwise-cycles",
            "borradaile-klein-planar.inspect-right-first-dart",
            "borradaile-klein-planar.right-first-leftmost-path",
            "borradaile-klein-planar.saturate-leftmost-path",
            "borradaile-klein-planar.inspect-right-first-dart",
            "borradaile-klein-planar.inspect-right-first-dart",
            "borradaile-klein-planar.inspect-right-first-dart",
            "borradaile-klein-planar.right-first-leftmost-path",
            "borradaile-klein-planar.saturate-leftmost-path",
            "borradaile-klein-planar.inspect-right-first-dart",
            "borradaile-klein-planar.inspect-right-first-dart",
            "borradaile-klein-planar.no-residual-path",
            "borradaile-klein-planar.optimal-cut",
        ]
    );
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("Borradaile-Klein result serializes"),
    )
    .expect("Borradaile-Klein result is JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "5");
    assert_eq!(solved["metrics"][0], "3");
    assert_eq!(solved["metrics"][1], "1");
    assert_eq!(solved["metrics"][2], "12");
    assert_eq!(solved["metrics"][3], "2");
    assert_eq!(solved["metrics"][5], "2");
    assert_eq!(solved["metrics"][12], "2");
    assert_eq!(solved["metrics"][15], "6");

    session
        .begin_seek(0)
        .expect("Borradaile-Klein timeline is seekable");
    session
        .resume_seek_json(1)
        .expect("Borradaile-Klein reverse seek completes");
    session.commit_staged_seek();
    let base: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("Borradaile-Klein base serializes"),
    )
    .expect("Borradaile-Klein base is JSON");
    assert_eq!(base["edge_states"][0]["flow"], "0");
}

#[test]
fn borradaile_klein_fast_profile_projects_exact_bounded_kernel_metrics() {
    let mut session = WasmSession::new(&planar_scenario("borradaile-klein-planar", "fast"))
        .expect("fast leftmost planar Scenario");
    while session
        .stage_next_json()
        .expect("fast Borradaile-Klein result stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("fast Borradaile-Klein scene serializes"),
    )
    .expect("fast Borradaile-Klein scene JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "5");
    assert_eq!(
        solved["metrics"],
        serde_json::json!([
            "3", "1", "12", "2", "3", "2", "1", "0", "0", "0", "0", "0", "2", "0", "0", "6"
        ])
    );
}

#[test]
fn hopcroft_karp_trace_fast_certificate_and_bidirectional_seek_cross_wasm() {
    let mut traced = WasmSession::new(&hopcroft_karp_scenario("trace")).expect("matching Scenario");
    let mut shortest_lengths = Vec::new();
    while let Some(frame) = traced.stage_next_json().expect("matching event stages") {
        let value: serde_json::Value =
            serde_json::from_str(&frame).expect("matching frame is JSON");
        if value["trace_event"]["catalog_id"] == "hopcroft-karp.level-bfs"
            && let Some(length) = value["trace_event"]["detail"]["value"].as_str()
        {
            shortest_lengths.push(length.to_owned());
        }
        traced.commit_staged_next();
    }
    assert_eq!(shortest_lengths, ["1", "3"]);
    let final_cursor = traced.item_count();
    let solved: serde_json::Value = serde_json::from_str(
        &traced
            .current_frame_json()
            .expect("matching result serializes"),
    )
    .expect("matching result is JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "bipartite-matching");
    assert_eq!(solved["outcome"]["cardinality"], "2");
    assert_eq!(
        solved["outcome"]["pairs"],
        serde_json::json!([
            { "edge_id": "b01", "left": "l0", "right": "r1" },
            { "edge_id": "b10", "left": "l1", "right": "r0" }
        ])
    );
    assert_eq!(solved["metrics"][0], "3");
    assert_eq!(solved["metrics"][3], "2");
    assert_eq!(solved["metrics"][6], "2");
    let solved_edge_states = solved["edge_states"].clone();

    traced.begin_seek(0).expect("matching base is seekable");
    let back: serde_json::Value = serde_json::from_str(
        &traced
            .resume_seek_json(final_cursor)
            .expect("backward seek serializes"),
    )
    .expect("backward seek is JSON");
    assert_eq!(back["done"], true);
    traced.commit_staged_seek();
    assert_eq!(traced.event_cursor(), "0");
    traced
        .begin_seek(final_cursor)
        .expect("prepared matching end is seekable");
    traced
        .resume_seek_json(final_cursor)
        .expect("forward seek serializes");
    traced.commit_staged_seek();
    let replayed: serde_json::Value =
        serde_json::from_str(&traced.current_frame_json().expect("replayed frame"))
            .expect("replayed frame is JSON");
    assert_eq!(replayed["edge_states"], solved_edge_states);
    assert_eq!(replayed["outcome"], solved["outcome"]);

    let mut fast =
        WasmSession::new(&hopcroft_karp_scenario("fast")).expect("fast matching Scenario");
    while fast
        .stage_next_json()
        .expect("fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_solved: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast result is JSON");
    assert_eq!(fast_solved["edge_states"], solved["edge_states"]);
    assert_eq!(fast_solved["outcome"], solved["outcome"]);
}

#[test]
#[allow(clippy::too_many_lines)]
fn hungarian_optimum_and_hall_witness_cross_wasm() {
    let mut traced =
        WasmSession::new(&hungarian_scenario("trace", false)).expect("assignment Scenario");
    let mut phases = Vec::new();
    let mut inspected_cells = 0;
    let mut selected_slacks = 0;
    while let Some(frame) = traced.stage_next_json().expect("Hungarian event stages") {
        let value: serde_json::Value =
            serde_json::from_str(&frame).expect("Hungarian frame is JSON");
        let catalog_id = value["trace_event"]["catalog_id"]
            .as_str()
            .expect("catalog id");
        phases.push(catalog_id.to_owned());
        if catalog_id == "hungarian.inspect-cell" {
            let refs = value["trace_event"]["entity_refs"]
                .as_array()
                .expect("cell focus");
            assert!((1..=2).contains(&refs.len()));
            assert!(refs.iter().all(|entity| {
                matches!(entity["kind"].as_str(), Some("node" | "residual-arc"))
            }));
            inspected_cells += 1;
        }
        if catalog_id == "hungarian.select-minimum-slack" {
            let refs = value["trace_event"]["entity_refs"]
                .as_array()
                .expect("minimum-slack focus");
            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0]["kind"], "residual-arc");
            selected_slacks += 1;
        }
        traced.commit_staged_next();
    }
    assert!(inspected_cells > 0);
    assert!(selected_slacks > 0);
    assert!(phases.iter().any(|phase| phase == "hungarian.dual-update"));
    assert!(phases.iter().any(|phase| phase == "hungarian.augment"));
    assert_eq!(phases.last().map(String::as_str), Some("hungarian.optimal"));
    let final_cursor = traced.item_count();
    let solved: serde_json::Value = serde_json::from_str(
        &traced
            .current_frame_json()
            .expect("Hungarian result serializes"),
    )
    .expect("Hungarian result JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "assignment");
    assert_eq!(solved["outcome"]["objective"], "minimize");
    assert_eq!(solved["outcome"]["total_cost"], "1");
    assert_eq!(
        solved["outcome"]["pairs"],
        serde_json::json!([
            { "agent": "a0", "edge_id": "e01", "task": "t1", "cost": "1" },
            { "agent": "a1", "edge_id": "e12", "task": "t2", "cost": "0" }
        ])
    );
    assert_eq!(solved["metrics"][3], "2");
    let solved_edges = solved["edge_states"].clone();

    traced.begin_seek(0).expect("Hungarian base seek");
    traced
        .resume_seek_json(final_cursor)
        .expect("Hungarian backward replay");
    traced.commit_staged_seek();
    traced.begin_seek(final_cursor).expect("Hungarian end seek");
    traced
        .resume_seek_json(final_cursor)
        .expect("Hungarian forward replay");
    traced.commit_staged_seek();
    let replayed: serde_json::Value =
        serde_json::from_str(&traced.current_frame_json().expect("replayed result"))
            .expect("replayed JSON");
    assert_eq!(replayed["edge_states"], solved_edges);
    assert_eq!(replayed["outcome"], solved["outcome"]);

    let mut fast = WasmSession::new(&hungarian_scenario("fast", false)).expect("fast assignment");
    while fast
        .stage_next_json()
        .expect("fast Hungarian result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_solved: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result")).expect("fast JSON");
    assert_eq!(fast_solved["outcome"], solved["outcome"]);
    assert_eq!(fast_solved["edge_states"], solved["edge_states"]);

    let mut infeasible =
        WasmSession::new(&hungarian_scenario("trace", true)).expect("Hall-deficient assignment");
    while infeasible
        .stage_next_json()
        .expect("Hall trace stages")
        .is_some()
    {
        infeasible.commit_staged_next();
    }
    let rejected: serde_json::Value = serde_json::from_str(
        &infeasible
            .current_frame_json()
            .expect("Hall witness serializes"),
    )
    .expect("Hall witness JSON");
    assert_eq!(rejected["solve_status"], "infeasible");
    assert_eq!(rejected["outcome"]["kind"], "assignment-infeasible");
    assert_eq!(rejected["outcome"]["deficiency"], "1");
    assert_eq!(
        rejected["outcome"]["hall_agents"],
        serde_json::json!(["a0", "a1"])
    );
    assert_eq!(
        rejected["outcome"]["neighbor_tasks"],
        serde_json::json!(["t0"])
    );
    assert_eq!(
        rejected["trace_event"]["catalog_id"],
        "hungarian.hall-witness"
    );
}

#[test]
fn auction_scales_bids_awards_and_certificates_cross_wasm() {
    let mut traced = WasmSession::new(&auction_scenario("trace", false)).expect("Auction Scenario");
    let mut phases = BTreeMap::<String, usize>::new();
    while let Some(frame) = traced.stage_next_json().expect("Auction event stages") {
        let value: serde_json::Value = serde_json::from_str(&frame).expect("Auction frame is JSON");
        let id = value["trace_event"]["catalog_id"]
            .as_str()
            .expect("catalog id")
            .to_owned();
        *phases.entry(id).or_default() += 1;
        traced.commit_staged_next();
    }
    for required in [
        "auction.scale-start",
        "auction.bid",
        "auction.award",
        "auction.scale-complete",
        "auction.optimal",
    ] {
        assert!(phases.contains_key(required));
    }
    let final_cursor = traced.item_count();
    let solved: serde_json::Value = serde_json::from_str(
        &traced
            .current_frame_json()
            .expect("Auction result serializes"),
    )
    .expect("Auction result JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "assignment");
    assert_eq!(solved["outcome"]["total_cost"], "1");
    assert_eq!(solved["metrics"][3], solved["metrics"][1]);
    assert_eq!(solved["metrics"][4], "2");
    assert_eq!(solved["metrics"][6], "0");
    assert!(
        solved["metrics"][5]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|phases| phases > 1)
    );
    let solved_edges = solved["edge_states"].clone();

    traced.begin_seek(0).expect("Auction base seek");
    traced
        .resume_seek_json(final_cursor)
        .expect("Auction backward replay");
    traced.commit_staged_seek();
    traced.begin_seek(final_cursor).expect("Auction end seek");
    traced
        .resume_seek_json(final_cursor)
        .expect("Auction forward replay");
    traced.commit_staged_seek();
    let replayed: serde_json::Value =
        serde_json::from_str(&traced.current_frame_json().expect("replayed Auction"))
            .expect("replayed Auction JSON");
    assert_eq!(replayed["edge_states"], solved_edges);
    assert_eq!(replayed["outcome"], solved["outcome"]);

    let mut infeasible =
        WasmSession::new(&auction_scenario("trace", true)).expect("Hall-deficient Auction");
    let mut infeasible_ids = Vec::new();
    while let Some(frame) = infeasible
        .stage_next_json()
        .expect("Hall-deficient Auction event stages")
    {
        let value: serde_json::Value = serde_json::from_str(&frame).expect("Hall frame is JSON");
        let catalog_id = value["trace_event"]["catalog_id"]
            .as_str()
            .expect("Hall catalog id");
        if !catalog_id.ends_with(".work-observation") {
            infeasible_ids.push(catalog_id.to_owned());
        }
        infeasible.commit_staged_next();
    }
    assert_eq!(
        infeasible_ids.last().map(String::as_str),
        Some("auction.hall-witness")
    );
    assert!(
        infeasible_ids[..infeasible_ids.len() - 1]
            .iter()
            .all(|id| id == "auction.inspect-assignment-edge"),
        "every Hall-search edge scan must precede the witness as a local Detail boundary"
    );
    let rejected: serde_json::Value =
        serde_json::from_str(&infeasible.current_frame_json().expect("Auction Hall scene"))
            .expect("Auction Hall JSON");
    assert_eq!(rejected["solve_status"], "infeasible");
    assert_eq!(rejected["outcome"]["deficiency"], "1");
    assert_eq!(
        rejected["outcome"]["hall_agents"],
        serde_json::json!(["a0", "a1"])
    );
    assert_eq!(
        rejected["outcome"]["neighbor_tasks"],
        serde_json::json!(["t0"])
    );
    assert_eq!(rejected["metrics"][15], "0");
    assert_eq!(
        rejected["trace_event"]["catalog_id"],
        "auction.hall-witness"
    );
}

#[test]
fn auction_fast_profile_projects_every_dedicated_metric_exactly() {
    let mut fast =
        WasmSession::new(&auction_scenario("fast", false)).expect("fast Auction Scenario");
    fast.stage_next_json()
        .expect("fast Auction result stages")
        .expect("fast Auction has one result");
    fast.commit_staged_next();
    let solved: serde_json::Value = serde_json::from_str(
        &fast
            .current_frame_json()
            .expect("fast Auction result serializes"),
    )
    .expect("fast Auction result is JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(
        solved["metrics"],
        serde_json::json!([
            "2", "6", "33", "6", "2", "3", "0", "0", "0", "0", "0", "0", "0", "0", "0", "6"
        ])
    );

    let mut traced =
        WasmSession::new(&auction_scenario("trace", false)).expect("trace Auction Scenario");
    while traced
        .stage_next_json()
        .expect("trace Auction event stages")
        .is_some()
    {
        traced.commit_staged_next();
    }
    let traced_result: serde_json::Value = serde_json::from_str(
        &traced
            .current_frame_json()
            .expect("trace Auction result serializes"),
    )
    .expect("trace Auction result is JSON");
    assert_eq!(solved["metrics"], traced_result["metrics"]);
}

#[test]
fn auction_fast_profile_preserves_nonzero_eviction_metrics() {
    let solve = |profile: &str| {
        let mut session = WasmSession::new(&auction_eviction_scenario(profile))
            .expect("Auction eviction Scenario");
        while session
            .stage_next_json()
            .expect("Auction eviction event stages")
            .is_some()
        {
            session.commit_staged_next();
        }
        serde_json::from_str::<serde_json::Value>(
            &session
                .current_frame_json()
                .expect("Auction eviction result serializes"),
        )
        .expect("Auction eviction result JSON")
    };
    let fast = solve("fast");
    let traced = solve("trace");
    assert_eq!(
        fast["metrics"],
        serde_json::json!([
            "3", "26", "129", "26", "3", "5", "11", "0", "0", "0", "0", "0", "0", "0", "0", "26"
        ])
    );
    assert_eq!(fast["metrics"], traced["metrics"]);
    assert_ne!(fast["metrics"][6], "0");
}

#[test]
#[allow(clippy::too_many_lines)]
fn transportation_simplex_and_modi_publish_distinct_traces_and_same_certificate() {
    let mut terminal = Vec::new();
    for algorithm in ["transportation-simplex", "modi"] {
        let mut traced = WasmSession::new(&transportation_scenario(algorithm, "trace", false))
            .expect("transportation trace Scenario");
        let mut catalog_ids = Vec::new();
        while let Some(frame) = traced
            .stage_next_json()
            .expect("transportation event stages")
        {
            let value: serde_json::Value =
                serde_json::from_str(&frame).expect("transportation frame JSON");
            let catalog_id = value["trace_event"]["catalog_id"]
                .as_str()
                .expect("catalog id");
            if catalog_id.starts_with(&format!("{algorithm}.")) {
                catalog_ids.push(catalog_id.to_owned());
            }
            if value["trace_event"]["catalog_id"]
                .as_str()
                .is_some_and(|id| {
                    id.ends_with("form-fundamental-cycle") || id.ends_with("form-closed-loop")
                })
            {
                assert_eq!(
                    value["trace_event"]["entity_refs"]
                        .as_array()
                        .expect("local transportation cycle focus")
                        .len(),
                    1,
                    "the complete loop remains in active_path; focus owns its entering route"
                );
            }
            traced.commit_staged_next();
        }
        assert!(
            catalog_ids
                .iter()
                .all(|id| id.starts_with(&format!("{algorithm}.")))
        );
        assert!(catalog_ids.iter().any(|id| {
            id.ends_with("form-fundamental-cycle") || id.ends_with("form-closed-loop")
        }));
        let final_cursor = traced.item_count();
        let solved: serde_json::Value = serde_json::from_str(
            &traced
                .current_frame_json()
                .expect("transportation result serializes"),
        )
        .expect("transportation result JSON");
        assert_eq!(solved["solve_status"], "optimal");
        assert_eq!(solved["outcome"]["kind"], "min-cost-flow");
        assert_eq!(solved["outcome"]["total_cost"], "43");
        assert!(
            solved["metrics"][3]
                .as_str()
                .and_then(|value| value.parse::<u64>().ok())
                .is_some_and(|pivots| pivots > 0)
        );
        assert!(
            solved["pseudoflow_forest"]["arcs"]
                .as_array()
                .is_some_and(|arcs| !arcs.is_empty())
        );
        traced.begin_seek(0).expect("transportation base seek");
        traced
            .resume_seek_json(final_cursor)
            .expect("transportation reverse replay");
        traced.commit_staged_seek();
        traced
            .begin_seek(final_cursor)
            .expect("transportation end seek");
        traced
            .resume_seek_json(final_cursor)
            .expect("transportation forward replay");
        traced.commit_staged_seek();

        let mut fast = WasmSession::new(&transportation_scenario(algorithm, "fast", false))
            .expect("transportation fast Scenario");
        fast.stage_next_json()
            .expect("transportation fast result stages")
            .expect("transportation fast result exists");
        fast.commit_staged_next();
        let fast_result: serde_json::Value = serde_json::from_str(
            &fast
                .current_frame_json()
                .expect("transportation fast result serializes"),
        )
        .expect("transportation fast JSON");
        assert_eq!(fast_result["metrics"], solved["metrics"]);
        assert_eq!(fast_result["outcome"], solved["outcome"]);
        terminal.push(solved);
    }
    assert_eq!(terminal[0]["outcome"], terminal[1]["outcome"]);

    let mut infeasible = WasmSession::new(&transportation_scenario(
        "transportation-simplex",
        "trace",
        true,
    ))
    .expect("sparse infeasible transportation Scenario");
    let mut cut_catalog_ids = Vec::new();
    while infeasible
        .stage_next_json()
        .expect("transportation cut stages")
        .is_some()
    {
        infeasible.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &infeasible
                .current_frame_json()
                .expect("transportation cut frame serializes"),
        )
        .expect("transportation cut frame JSON");
        if let Some(catalog_id) = frame["trace_event"]["catalog_id"].as_str() {
            cut_catalog_ids.push(catalog_id.to_owned());
        }
    }
    let rejected: serde_json::Value = serde_json::from_str(
        &infeasible
            .current_frame_json()
            .expect("transportation cut serializes"),
    )
    .expect("transportation cut JSON");
    assert_eq!(rejected["solve_status"], "infeasible");
    assert_eq!(rejected["outcome"]["kind"], "infeasible");
    assert_eq!(
        cut_catalog_ids.last().map(String::as_str),
        Some("feasibility.infeasible")
    );
    assert!(
        cut_catalog_ids
            .iter()
            .any(|id| id == "feasibility.inspect-cut-arc"),
        "the infeasibility certificate exposes its cut search before the terminal boundary"
    );
}

#[test]
fn feasibility_domain_publication_is_not_reported_as_a_whole_graph_change() {
    let mut session = WasmSession::new(&transportation_scenario(
        "transportation-simplex",
        "trace",
        false,
    ))
    .expect("transportation trace Scenario");
    let first: serde_json::Value = serde_json::from_str(
        &session
            .stage_next_json()
            .expect("first feasibility event stages")
            .expect("transportation feasibility trace is nonempty"),
    )
    .expect("first feasibility frame is JSON");
    assert_eq!(
        first["trace_event"]["catalog_id"],
        "feasibility.add-original-arc"
    );
    assert_eq!(
        first["trace_event_semantics"]["changed_entity_refs"],
        serde_json::json!([{
            "kind": "residual-arc",
            "edge_id": "e000",
            "direction": "forward"
        }]),
        "publishing the immutable feasibility domain must not fabricate changes to every declared node and edge"
    );
}

#[test]
fn prepared_flow_timeline_seeks_to_an_unplayed_end_atomically() {
    let mut session = WasmSession::new(&flow_scenario()).expect("flow Scenario is valid");

    let first: serde_json::Value = serde_json::from_str(
        &session
            .stage_next_json()
            .expect("first event prepares the timeline")
            .expect("first event exists"),
    )
    .expect("first event is JSON");
    let final_event = first["event_count"]
        .as_str()
        .and_then(|value| value.parse::<usize>().ok())
        .expect("event count fits the session index domain");
    assert!(final_event > 4);
    session.commit_staged_next();
    assert_eq!(session.event_cursor(), "1");

    session
        .begin_seek(final_event)
        .expect("a prepared but unplayed final event is seekable");
    let staged: serde_json::Value = serde_json::from_str(
        &session
            .resume_seek_json(1)
            .expect("final event seek serializes"),
    )
    .expect("seek result is JSON");
    assert_eq!(staged["cursor"], final_event.to_string());
    assert_eq!(staged["frame"]["solve_status"], "optimal");
    assert_eq!(session.event_cursor(), "1");

    session.discard_staged_seek();
    assert_eq!(session.event_cursor(), "1");
    session
        .begin_seek(final_event)
        .expect("discarding the candidate leaves the prepared end seekable");
    session
        .resume_seek_json(1)
        .expect("restaged final event seek serializes");

    session.commit_staged_seek();
    assert_eq!(session.event_cursor(), final_event.to_string());
    session
        .begin_seek(2)
        .expect("the atomic end seek commits the intermediate history");
    session.discard_staged_seek();
}

#[test]
fn synchronous_flow_seek_commits_a_prepared_unplayed_end_only_after_serialization() {
    let scenario = flow_scenario();
    let mut flow = FlowSession::new(&scenario).expect("flow Scenario is valid");
    let first: serde_json::Value = serde_json::from_str(
        &flow
            .stage_next_json()
            .expect("first event prepares the timeline")
            .expect("first event exists"),
    )
    .expect("first event is JSON");
    let final_event = first["event_count"]
        .as_str()
        .and_then(|value| value.parse::<usize>().ok())
        .expect("event count fits the session index domain");
    flow.commit_staged_next();
    assert_eq!(flow.cursor, 1);

    let frame: serde_json::Value = serde_json::from_str(
        &seek_flow_json(&mut flow, final_event, MAX_FRAME_JSON_BYTES)
            .expect("prepared end serializes and commits"),
    )
    .expect("final scene is JSON");
    assert_eq!(frame["event_id"], final_event.to_string());
    assert_eq!(frame["solve_status"], "optimal");
    assert_eq!(flow.cursor, final_event);
    assert_eq!(flow.committed_end, final_event);
    assert!(flow.staged_seek.is_none());
}

#[test]
fn eager_flow_timeline_budget_fails_closed_before_scene_cloning() {
    let scenario_json = flow_scenario();
    let flow = FlowSession::new(&scenario_json).expect("flow Scenario is valid");
    let mut base = flow.frames[0].clone();
    base.edge_states[0].flow = "1".to_owned();
    base.node_trace_states[0].label = Some("retained base state".to_owned());
    let frame_bytes = serde_json::to_vec(&base)
        .expect("ready scene serializes")
        .len();
    let over_budget_events = u64::try_from(MAX_EAGER_FLOW_TIMELINE_BYTES / frame_bytes + 1)
        .expect("test event count fits u64");
    let frames = trace_timeline_resource_limit_frames(&flow.scenario, &base, over_budget_events)
        .expect("budget calculation succeeds")
        .expect("oversized timeline becomes a resource result");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].event_id, "0");
    assert_eq!(frames[0].event_count, "1");
    assert_eq!(frames[1].event_id, "1");
    assert_eq!(frames[1].event_count, "1");
    assert!(matches!(
        frames[1].solve_status,
        FlowSolveStatusV1::ResourceLimit
    ));
    assert_eq!(
        serde_json::to_value(&frames[1].edge_states).expect("limited edge state serializes"),
        serde_json::to_value(&base.edge_states).expect("base edge state serializes")
    );
    assert_eq!(
        serde_json::to_value(&frames[1].node_trace_states).expect("limited node state serializes"),
        serde_json::to_value(&base.node_trace_states).expect("base node state serializes")
    );
}

#[test]
fn eager_flow_timeline_checks_actual_cumulative_frame_bytes() {
    let scenario_json = flow_scenario();
    let flow = FlowSession::new(&scenario_json).expect("flow Scenario is valid");
    let base = flow.frames[0].clone();
    let base_bytes = serialized_flow_scene_bytes(&base).expect("base serializes");
    let mut larger_event = base.clone();
    larger_event.event_count = "9".repeat(base_bytes + 1);
    let limit = base_bytes.checked_mul(2).expect("small fixture limit");
    assert!(serialized_flow_scene_bytes(&larger_event).expect("event serializes") > base_bytes);

    let mut timeline = EagerFlowTimeline::new(base).expect("timeline starts");
    assert!(
        !timeline
            .try_push_with_limit(larger_event, limit)
            .expect("actual byte check succeeds")
    );
    assert_eq!(timeline.finish().len(), 1);
}

#[test]
fn normalized_timeline_budget_rechecks_materialized_scene_bytes() {
    let scenario_json = flow_scenario();
    let flow = FlowSession::new(&scenario_json).expect("flow Scenario is valid");
    let base = flow.frames[0].clone();
    let base_bytes = serialized_flow_scene_bytes(&base).expect("base serializes");
    let mut larger_event = base.clone();
    larger_event.metrics[0] = "9".repeat(base_bytes + 1);
    let frames = normalize_prepared_flow_timeline_with_limit(
        &flow.scenario,
        vec![base, larger_event],
        base_bytes.checked_mul(2).expect("small fixture limit"),
    )
    .expect("post-normalization budget calculation succeeds");
    assert_eq!(frames.len(), 2);
    assert!(matches!(
        frames[1].solve_status,
        FlowSolveStatusV1::ResourceLimit
    ));
}

#[test]
fn detail_timeline_preserves_only_solver_published_boundaries() {
    let scenario_json = flow_scenario();
    let flow = FlowSession::new(&scenario_json).expect("flow Scenario is valid");
    let prepared = flow.prepare_frames().expect("reference timeline prepares");
    let source_base = prepared.first().expect("ready frame exists");
    let source_event = prepared
        .iter()
        .skip(1)
        .find(|frame| {
            frame.trace_event.as_ref().is_some_and(|event| {
                event.catalog_id.starts_with("edmonds-karp.")
                    && validate_primary_work_boundary(&event.catalog_id).is_ok()
            })
        })
        .expect("declared Edmonds-Karp work boundary exists");
    let source_granularity = source_event
        .trace_event
        .as_ref()
        .expect("source event has trace metadata")
        .minimum_granularity;

    for abstraction in [
        flow::FlowWorkAbstractionV1::Primitive,
        flow::FlowWorkAbstractionV1::Iteration,
        flow::FlowWorkAbstractionV1::OracleCall,
    ] {
        let mut base = source_base.clone();
        let mut event = source_event.clone();
        base.trace_steps.primary_work.abstraction = abstraction;
        event.trace_steps.primary_work.abstraction = abstraction;
        let ordinal = usize::from(base.trace_steps.primary_work.metric_ordinal);
        let start = primary_work_value(&base, ordinal).expect("base primary work is valid");
        event.metrics[ordinal] = start.checked_add(3).expect("small test delta").to_string();

        let normalized = normalize_prepared_flow_timeline(vec![base, event])
            .expect("source work contract normalizes");
        let prepared = PreparedFlowTimeline::from_source_frames(normalized)
            .expect("normalized source timeline is serializable");
        assert_eq!(prepared.len(), 2);
        assert!(prepared.full_frame(0).is_some());
        assert!(prepared.full_frame(1).is_some());
        let source_boundary = prepared.materialize(1).expect("source event materializes");
        let source_event = source_boundary
            .trace_event
            .as_ref()
            .expect("source event has trace metadata");
        assert_eq!(source_event.minimum_granularity, source_granularity);
        assert!(!source_event.catalog_id.ends_with(".work-observation"));
        let semantics = source_boundary
            .trace_event_semantics
            .as_ref()
            .expect("source boundary owns semantics");
        assert!(
            semantics.work_deltas.iter().any(|delta| {
                delta.unit == FlowTraceWorkUnitV1::PrimaryWork && delta.count == "3"
            })
        );
        assert_eq!(
            semantics.primary_work_block.as_ref().map(|block| (
                block.first.as_str(),
                block.last.as_str(),
                block.total.as_str(),
            )),
            Some(("1", "3", "3"))
        );
    }
}

#[test]
fn primary_work_rejects_an_undeclared_source_boundary() {
    assert_eq!(
        validate_primary_work_boundary("edmonds-karp.undeclared-primary-work"),
        Err("primary work advanced on an undeclared source boundary")
    );
    assert!(validate_primary_work_boundary("edmonds-karp.inspect-residual-arc").is_ok());
    assert!(validate_primary_work_boundary("dinic.level-bfs").is_ok());
}

#[test]
#[allow(clippy::too_many_lines)]
fn production_normalizer_rejects_nonlocal_micro_focus_mutations() {
    let flow = FlowSession::new(&ibfs_scenario("trace")).expect("IBFS Scenario is valid");
    let frames = flow.prepare_frames().expect("IBFS timeline prepares");

    let mut graph_wide = frames.clone();
    let node_refs = graph_wide[0]
        .graph
        .nodes
        .iter()
        .take(3)
        .map(|node| FlowTraceEntityRefSceneV1::Node {
            node_id: node.id.clone(),
        })
        .collect::<Vec<_>>();
    let event_index = graph_wide
        .iter_mut()
        .position(|scene| {
            scene
                .trace_event
                .as_ref()
                .is_some_and(|event| event.minimum_granularity == TraceGranularityV1::Micro)
        })
        .expect("IBFS exposes a Detail primitive");
    graph_wide[event_index]
        .trace_event
        .as_mut()
        .expect("selected scene owns a Detail event")
        .entity_refs = node_refs;
    let scene = &graph_wide[event_index];
    let event = scene
        .trace_event
        .as_ref()
        .expect("mutated Detail scene remains present");
    assert_eq!(
        validate_source_micro_locality(scene, event),
        Err("flow Detail primitive focuses too many ordinary nodes"),
        "one Micro event must not turn into graph-wide node focus",
    );

    let mut unrelated_endpoint = frames.clone();
    let (edge_id, unrelated_node) = unrelated_endpoint
        .iter()
        .skip(1)
        .filter_map(|scene| scene.trace_event.as_ref())
        .find_map(|event| {
            if event.minimum_granularity != TraceGranularityV1::Micro {
                return None;
            }
            let edge_id = event.entity_refs.iter().find_map(|entity| match entity {
                FlowTraceEntityRefSceneV1::Edge { edge_id }
                | FlowTraceEntityRefSceneV1::ResidualArc { edge_id, .. } => Some(edge_id),
                FlowTraceEntityRefSceneV1::Node { .. } => None,
            })?;
            let edge = unrelated_endpoint[0]
                .graph
                .edges
                .iter()
                .find(|edge| edge.id == *edge_id)?;
            let node = unrelated_endpoint[0]
                .graph
                .nodes
                .iter()
                .find(|node| node.id != edge.from && node.id != edge.to)?;
            Some((edge_id.clone(), node.id.clone()))
        })
        .expect("IBFS exposes an edge Detail with an unrelated graph node");
    let event_index = unrelated_endpoint
        .iter_mut()
        .position(|scene| {
            scene.trace_event.as_ref().is_some_and(|event| {
                event.minimum_granularity == TraceGranularityV1::Micro
                    && event.entity_refs.iter().any(|entity| match entity {
                        FlowTraceEntityRefSceneV1::Edge { edge_id: candidate }
                        | FlowTraceEntityRefSceneV1::ResidualArc {
                            edge_id: candidate, ..
                        } => candidate == &edge_id,
                        FlowTraceEntityRefSceneV1::Node { .. } => false,
                    })
            })
        })
        .expect("owning edge Detail remains present");
    unrelated_endpoint[event_index]
        .trace_event
        .as_mut()
        .expect("selected scene owns an edge Detail")
        .entity_refs = vec![
        FlowTraceEntityRefSceneV1::Edge { edge_id },
        FlowTraceEntityRefSceneV1::Node {
            node_id: unrelated_node,
        },
    ];
    let scene = &unrelated_endpoint[event_index];
    let event = scene
        .trace_event
        .as_ref()
        .expect("mutated edge Detail scene remains present");
    assert_eq!(
        validate_source_micro_locality(scene, event),
        Err("flow Detail primitive node focus is not an endpoint of its edge"),
        "a Detail node must be an endpoint of its focused edge",
    );

    let mut duplicate = frames;
    let event_index = duplicate
        .iter_mut()
        .position(|scene| {
            scene.trace_event.as_ref().is_some_and(|event| {
                event.minimum_granularity == TraceGranularityV1::Micro
                    && !event.entity_refs.is_empty()
            })
        })
        .expect("IBFS exposes a focused Detail primitive");
    let event = duplicate[event_index]
        .trace_event
        .as_mut()
        .expect("selected scene owns a focused Detail");
    let repeated = event.entity_refs[0].clone();
    event.entity_refs = vec![repeated.clone(), repeated];
    let scene = &duplicate[event_index];
    let event = scene
        .trace_event
        .as_ref()
        .expect("mutated duplicate Detail scene remains present");
    assert_eq!(
        validate_source_micro_locality(scene, event),
        Err("flow Detail primitive contains duplicate focus identities"),
        "duplicate focus identities must not be silently collapsed",
    );
}

#[test]
fn changed_entity_diff_records_removed_overlay_identity() {
    let before = serde_json::json!({ "active": { "node_id": "a" } });
    let after = serde_json::json!({ "active": null });
    let node_ids = std::collections::BTreeSet::from(["a"]);
    let edge_ids = std::collections::BTreeSet::new();
    let mut changed = std::collections::BTreeSet::new();
    collect_changed_entity_refs(
        Some(&before),
        Some(&after),
        None,
        &node_ids,
        &edge_ids,
        &mut changed,
    );
    assert!(changed.contains(&FlowTraceEntityRefSceneV1::Node {
        node_id: "a".to_owned(),
    }));
}

#[test]
fn changed_entity_diff_walks_removed_overlay_subtrees_and_scalar_ids() {
    let before = serde_json::json!({
        "overlay": {
            "leaving_edge": "e0",
            "forest": [{ "edge_id": "e1" }],
            "strong_nodes": ["a", "b"]
        }
    });
    let after = serde_json::json!({ "overlay": null });
    let node_ids = std::collections::BTreeSet::from(["a", "b"]);
    let edge_ids = std::collections::BTreeSet::from(["e0", "e1"]);
    let mut changed = std::collections::BTreeSet::new();
    collect_changed_entity_refs(
        Some(&before),
        Some(&after),
        None,
        &node_ids,
        &edge_ids,
        &mut changed,
    );
    assert_eq!(
        changed,
        std::collections::BTreeSet::from([
            FlowTraceEntityRefSceneV1::Node {
                node_id: "a".to_owned(),
            },
            FlowTraceEntityRefSceneV1::Node {
                node_id: "b".to_owned(),
            },
            FlowTraceEntityRefSceneV1::Edge {
                edge_id: "e0".to_owned(),
            },
            FlowTraceEntityRefSceneV1::Edge {
                edge_id: "e1".to_owned(),
            },
        ])
    );
}

#[test]
fn changed_entity_diff_compares_identity_arrays_without_reorder_noise() {
    let before = serde_json::json!({ "source_side": ["a", "b"] });
    let reordered = serde_json::json!({ "source_side": ["b", "a"] });
    let after = serde_json::json!({ "source_side": ["b", "c"] });
    let node_ids = std::collections::BTreeSet::from(["a", "b", "c"]);
    let edge_ids = std::collections::BTreeSet::new();
    let mut changed = std::collections::BTreeSet::new();
    collect_changed_entity_refs(
        Some(&before),
        Some(&reordered),
        None,
        &node_ids,
        &edge_ids,
        &mut changed,
    );
    assert!(changed.is_empty());

    collect_changed_entity_refs(
        Some(&before),
        Some(&after),
        None,
        &node_ids,
        &edge_ids,
        &mut changed,
    );
    assert_eq!(
        changed,
        std::collections::BTreeSet::from([
            FlowTraceEntityRefSceneV1::Node {
                node_id: "a".to_owned(),
            },
            FlowTraceEntityRefSceneV1::Node {
                node_id: "c".to_owned(),
            },
        ])
    );
}

#[test]
fn changed_entity_diff_records_both_sides_of_single_identity_replacement() {
    let before = serde_json::json!({
        "active_node": { "node_id": "a" },
        "active_arc": { "edge_id": "e", "direction": "forward" }
    });
    let after = serde_json::json!({
        "active_node": { "node_id": "b" },
        "active_arc": { "edge_id": "e", "direction": "reverse" }
    });
    let node_ids = std::collections::BTreeSet::from(["a", "b"]);
    let edge_ids = std::collections::BTreeSet::from(["e"]);
    let mut changed = std::collections::BTreeSet::new();
    collect_changed_entity_refs(
        Some(&before),
        Some(&after),
        None,
        &node_ids,
        &edge_ids,
        &mut changed,
    );
    assert_eq!(
        changed,
        std::collections::BTreeSet::from([
            FlowTraceEntityRefSceneV1::Node {
                node_id: "a".to_owned(),
            },
            FlowTraceEntityRefSceneV1::Node {
                node_id: "b".to_owned(),
            },
            FlowTraceEntityRefSceneV1::ResidualArc {
                edge_id: "e".to_owned(),
                direction: "forward".to_owned(),
            },
            FlowTraceEntityRefSceneV1::ResidualArc {
                edge_id: "e".to_owned(),
                direction: "reverse".to_owned(),
            },
        ])
    );
}

#[test]
fn changed_entity_diff_ignores_non_entity_scalar_id_collisions() {
    let before = serde_json::json!({
        "iteration": "0",
        "selected_edge_count": "0",
        "shortcut_arcs": "0",
        "working_nodes": "0",
        "working_edges": "0",
        "repair_arc_scans": "0",
        "selected_off_tree_edge": "0",
        "sampled_arc": "0"
    });
    let after = serde_json::json!({
        "iteration": "1",
        "selected_edge_count": "1",
        "shortcut_arcs": "1",
        "working_nodes": "1",
        "working_edges": "1",
        "repair_arc_scans": "1",
        "selected_off_tree_edge": "1",
        "sampled_arc": "1"
    });
    let node_ids = std::collections::BTreeSet::from(["1"]);
    let edge_ids = std::collections::BTreeSet::from(["1"]);
    let mut changed = std::collections::BTreeSet::new();
    collect_changed_entity_refs(
        Some(&before),
        Some(&after),
        None,
        &node_ids,
        &edge_ids,
        &mut changed,
    );
    assert!(changed.is_empty());
}

#[test]
fn flow_timeline_preserves_source_frames_and_remains_seekable() {
    let scenario_json = epsilon_relaxation_scenario("trace");
    let flow = FlowSession::new(&scenario_json).expect("flow Scenario is valid");
    let expected = flow
        .prepare_frames()
        .expect("full reference timeline prepares");
    let prepared = flow.prepare_timeline().expect("source timeline prepares");
    assert_eq!(prepared.len(), expected.len());
    assert!(
        (0..prepared.len()).all(|index| prepared.full_frame(index).is_some()),
        "every visible boundary must be an actual solver-published scene"
    );
    for (index, expected) in expected.iter().enumerate() {
        assert_eq!(
            serialize_frame(&prepared.materialize(index).expect("frame materializes"))
                .expect("prepared frame serializes"),
            serialize_frame(expected).expect("reference frame serializes")
        );
    }
    let materialized_bytes = expected
        .iter()
        .try_fold(0_usize, |sum, scene| {
            sum.checked_add(serialized_flow_scene_bytes(scene).expect("scene size"))
        })
        .expect("materialized byte total");
    assert_eq!(prepared.stored_bytes(), materialized_bytes);
}

#[test]
fn flow_session_seeks_an_unplayed_source_frame_without_changing_committed_state() {
    let scenario = epsilon_relaxation_scenario("trace");
    let mut flow = FlowSession::new(&scenario).expect("flow Scenario is valid");
    let expected_frames = flow.prepare_frames().expect("reference timeline prepares");
    flow.stage_next_json()
        .expect("first event prepares the timeline")
        .expect("first event exists");
    flow.commit_staged_next();
    let committed = flow.current_frame_json().expect("current frame serializes");
    let target = expected_frames
        .len()
        .checked_sub(2)
        .expect("trace has an interior source frame");
    let expected = serialize_frame(&expected_frames[target]).expect("reference target serializes");

    flow.begin_seek(target)
        .expect("source target stages without changing the committed frame");
    assert_eq!(flow.cursor, 1);
    assert_eq!(flow.committed_end, 1);
    assert_eq!(
        flow.current_frame_json()
            .expect("current frame is retained"),
        committed
    );
    assert_eq!(
        serialize_frame(flow.cached_frame(target).expect("target is available"))
            .expect("target serializes"),
        expected
    );
    flow.discard_staged_seek();
    assert_eq!(flow.cursor, 1);
    assert_eq!(flow.committed_end, 1);
}

#[test]
fn synchronous_flow_seek_discards_a_candidate_when_serialization_fails() {
    let scenario = flow_scenario();
    let mut flow = FlowSession::new(&scenario).expect("flow Scenario is valid");
    flow.stage_next_json()
        .expect("first event prepares the timeline")
        .expect("first event exists");
    flow.commit_staged_next();
    let committed = flow
        .current_frame_json()
        .expect("committed frame serializes");

    flow.begin_seek(4).expect("target can be prepared");
    assert!(publish_staged_flow_seek_json(&mut flow, 1).is_err());
    assert_eq!(flow.cursor, 1);
    assert_eq!(flow.committed_end, 1);
    assert!(flow.staged_seek.is_none());
    assert_eq!(
        flow.current_frame_json()
            .expect("old frame remains current"),
        committed
    );
    flow.begin_seek(4)
        .expect("failed synchronous publication leaves no stale candidate");
    flow.discard_staged_seek();
}

#[test]
fn dinic_dispatches_through_reversible_flow_frames_and_certifies_the_result() {
    let mut session = WasmSession::new(&flow_scenario_with_algorithm("dinic"))
        .expect("Dinic flow Scenario is valid");
    let first: serde_json::Value =
        serde_json::from_str(&commit_next(&mut session)).expect("first Dinic frame is JSON");
    assert_eq!(first["trace_event"]["catalog_id"], "dinic.level-bfs");
    let mut saw_level_bfs = false;

    while let Some(frame) = session.stage_next_json().expect("Dinic trace event stages") {
        let frame: serde_json::Value =
            serde_json::from_str(&frame).expect("Dinic trace frame is JSON");
        saw_level_bfs |= frame["trace_event"]["catalog_id"] == "dinic.level-bfs";
        session.commit_staged_next();
    }
    assert!(saw_level_bfs, "the source BFS operation remains observable");
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final Dinic scene serializes"),
    )
    .expect("final Dinic scene is JSON");
    assert_eq!(solved["algorithm"]["id"], "dinic");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["kind"], "max-flow");
    assert_eq!(solved["outcome"]["value"], "9");
    assert_eq!(solved["edge_states"][0]["flow"], "9");
    assert_eq!(solved["metrics"][0], "2");
    assert_eq!(solved["metrics"][6], "1");
}

#[test]
fn dynamic_tree_blocking_dispatches_reversible_forest_and_exact_metrics() {
    let source = flow_scenario_with_algorithm("dynamic-tree-blocking-flow");
    let mut session = WasmSession::new(&source).expect("dynamic-tree Scenario is valid");
    let mut linked = None;
    let mut saw_path_update = false;
    while session
        .stage_next_json()
        .expect("dynamic-tree trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("dynamic-tree frame serializes"),
        )
        .expect("dynamic-tree frame is JSON");
        match frame["trace_event"]["catalog_id"].as_str() {
            Some("dynamic-tree-blocking-flow.link-candidate") => {
                assert_eq!(
                    frame["pseudoflow_forest"]["arcs"].as_array().map(Vec::len),
                    Some(1)
                );
                linked = Some((
                    session.event_cursor().parse::<usize>().expect("cursor"),
                    frame,
                ));
            }
            Some("dynamic-tree-blocking-flow.augment-root-path") => {
                saw_path_update = frame["residual_arcs"]
                    .as_array()
                    .expect("residual arcs")
                    .iter()
                    .any(|arc| arc["active"] == true);
            }
            _ => {}
        }
    }
    assert!(
        saw_path_update,
        "root-path update publishes its active path"
    );
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("dynamic-tree result serializes"),
    )
    .expect("dynamic-tree result is JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "9");
    assert_eq!(solved["metrics"][0], "2");
    assert_eq!(solved["metrics"][3], "1");
    assert_eq!(solved["metrics"][6], "1");
    assert_eq!(solved["metrics"][11], "1");
    assert_eq!(solved["metrics"][12], "1");
    assert_eq!(solved["metrics"][13], "1");
    assert_eq!(solved["metrics"][15], "3");

    let (linked_cursor, linked_scene) = linked.expect("link event exists");
    let base: serde_json::Value =
        serde_json::from_str(&session.seek_json(0).expect("dynamic-tree base seek"))
            .expect("base scene is JSON");
    assert!(base["pseudoflow_forest"].is_null());
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(linked_cursor)
            .expect("dynamic-tree link seek"),
    )
    .expect("replayed link scene is JSON");
    assert_eq!(replayed, linked_scene);

    let mut fast_source: serde_json::Value = serde_json::from_str(&source).expect("scenario");
    fast_source["payload"]["run_profile"] = serde_json::json!("fast");
    let mut fast = WasmSession::new(&fast_source.to_string()).expect("fast Scenario");
    while fast
        .stage_next_json()
        .expect("fast dynamic-tree result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast result is JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
}

#[test]
fn goldberg_rao_dispatches_binary_phases_with_fast_trace_metric_parity() {
    let source = flow_scenario_with_algorithm("goldberg-rao");
    let mut session = WasmSession::new(&source).expect("Goldberg-Rao Scenario is valid");
    let mut seen = BTreeMap::<String, serde_json::Value>::new();
    while session
        .stage_next_json()
        .expect("Goldberg-Rao trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("Goldberg-Rao frame serializes"),
        )
        .expect("Goldberg-Rao frame is JSON");
        if let Some(id) = frame["trace_event"]["catalog_id"].as_str() {
            seen.insert(id.to_owned(), frame);
        }
    }
    for id in [
        "goldberg-rao.start-gap-phase",
        "goldberg-rao.inspect-residual-arc",
        "goldberg-rao.build-reverse-zero-one-adjacency",
        "goldberg-rao.relax-binary-distance",
        "goldberg-rao.inspect-binary-length",
        "goldberg-rao.binary-length-distance",
        "goldberg-rao.minimum-canonical-cut",
        "goldberg-rao.contract-zero-scc",
        "goldberg-rao.blocking-or-delta-flow",
        "goldberg-rao.lift-component-flow",
        "goldberg-rao.halve-cut-gap",
        "goldberg-rao.optimal",
    ] {
        assert!(seen.contains_key(id), "missing trace event {id}");
    }
    let distance = &seen["goldberg-rao.binary-length-distance"];
    assert_eq!(distance["node_trace_states"][0]["label"], "1");
    let update = &seen["goldberg-rao.blocking-or-delta-flow"];
    assert!(
        update["residual_arcs"]
            .as_array()
            .expect("residual arcs")
            .iter()
            .any(|arc| arc["active"] == true)
    );

    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("Goldberg-Rao result serializes"),
    )
    .expect("Goldberg-Rao result is JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "9");
    assert_ne!(solved["metrics"][0], "0");
    assert_ne!(solved["metrics"][3], "0");

    let mut fast_source: serde_json::Value = serde_json::from_str(&source).expect("scenario");
    fast_source["payload"]["run_profile"] = serde_json::json!("fast");
    let mut fast = WasmSession::new(&fast_source.to_string()).expect("fast Scenario");
    while fast
        .stage_next_json()
        .expect("fast Goldberg-Rao result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast result is JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the end-to-end primitive test keeps projection and tamper checks in one transcript"
)]
fn binary_blocking_dispatches_a_checked_primitive_without_a_max_flow_claim() {
    let mut value: serde_json::Value =
        serde_json::from_str(&flow_scenario_with_algorithm("binary-blocking-flow"))
            .expect("scenario");
    value["payload"]["graph"] = serde_json::json!({
        "nodes": [{ "id": "s" }, { "id": "a" }, { "id": "b" }, { "id": "t" }],
        "edges": [
            { "id": "sa", "from": "s", "to": "a", "capacity": "12" },
            { "id": "ab", "from": "a", "to": "b", "capacity": "12" },
            { "id": "ba", "from": "b", "to": "a", "capacity": "12" },
            { "id": "bt", "from": "b", "to": "t", "capacity": "12" }
        ]
    });
    let source = value.to_string();
    let mut session = WasmSession::new(&source).expect("binary primitive scenario");
    let frames = collect_committed_json_frames(&mut session, "binary primitive");
    let primary_work = frames.last().expect("binary terminal frame")["metrics"][2]
        .as_str()
        .expect("binary primary work")
        .parse::<usize>()
        .expect("binary primary work integer");
    let synthetic_ticks = frames
        .iter()
        .filter(|frame| {
            frame["trace_event"]["catalog_id"]
                .as_str()
                .is_some_and(|catalog_id| catalog_id.ends_with(".primary-work-unit"))
        })
        .count();
    assert_eq!(
        synthetic_ticks, 0,
        "legacy counter-only frames remain forbidden"
    );
    assert!(frames.iter().all(|frame| {
        frame["trace_event"]["catalog_id"]
            .as_str()
            .is_none_or(|catalog_id| !catalog_id.ends_with(".work-observation"))
    }));
    assert!(
        primary_work > 0,
        "source events retain the measured work total"
    );
    let analyzed = frames
        .iter()
        .find(|frame| {
            frame["trace_event"]["catalog_id"] == "binary-blocking-flow.analyze-binary-network"
        })
        .expect("binary analysis source boundary");
    assert_eq!(analyzed["binary_blocking_overlay"]["stage"], "analyzed");
    let inspections = frames
        .iter()
        .filter(|frame| {
            frame["trace_event"]["catalog_id"] == "binary-blocking-flow.inspect-binary-length"
        })
        .collect::<Vec<_>>();
    assert_eq!(inspections.len(), 4);
    assert!(inspections.iter().all(|frame| {
        frame["trace_event"]["minimum_granularity"] == "micro"
            && frame["binary_blocking_overlay"]["stage"] == "analyzing"
            && frame["binary_blocking_overlay"]["base_zero_arcs"] == serde_json::json!([])
            && frame["binary_blocking_overlay"]["special_arcs"] == serde_json::json!([])
            && frame["binary_blocking_overlay"]["admissible_arcs"] == serde_json::json!([])
            && frame["binary_blocking_overlay"]["zero_admissible_arcs"] == serde_json::json!([])
    }));
    let contracted = frames
        .iter()
        .find(|frame| {
            frame["trace_event"]["catalog_id"] == "binary-blocking-flow.contract-zero-scc"
        })
        .expect("contraction boundary");
    assert_eq!(contracted["binary_blocking_overlay"]["stage"], "contracted");
    assert_eq!(
        frames.last().expect("complete frame")["binary_blocking_overlay"]["stage"],
        "complete"
    );
    assert_eq!(
        analyzed["trace_event"]["catalog_id"],
        "binary-blocking-flow.analyze-binary-network"
    );
    assert_eq!(
        frames.last().expect("complete frame")["trace_event"]["catalog_id"],
        "binary-blocking-flow.complete-primitive"
    );
    assert!(
        analyzed["binary_blocking_overlay"]["admissible_arcs"]
            .as_array()
            .is_some_and(|arcs| !arcs.is_empty())
    );
    assert!(
        contracted["binary_blocking_overlay"]["zero_admissible_arcs"]
            .as_array()
            .is_some_and(|arcs| !arcs.is_empty())
    );
    let solved = frames.last().expect("complete frame");
    assert_eq!(solved["solve_status"], "primitive-complete");
    assert_eq!(solved["outcome"]["kind"], "binary-blocking-flow");
    assert!(solved["outcome"].get("cut_bound").is_none());
    assert!(solved["outcome"].get("source_side").is_none());
    assert_eq!(solved["outcome"]["termination"], "delta-reached");
    assert_eq!(solved["outcome"]["delivered"], solved["outcome"]["delta"]);

    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(
                analyzed["event_id"]
                    .as_str()
                    .expect("binary analysis event id")
                    .parse::<usize>()
                    .expect("binary analysis event id integer"),
            )
            .expect("analyzed boundary seeks backward"),
    )
    .expect("replayed frame is JSON");
    assert_eq!(replayed, *analyzed);

    value["payload"]["run_profile"] = serde_json::json!("fast");
    let mut fast = WasmSession::new(&value.to_string()).expect("fast primitive scenario");
    while fast
        .stage_next_json()
        .expect("fast primitive result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value = serde_json::from_str(
        &fast
            .current_frame_json()
            .expect("fast primitive serializes"),
    )
    .expect("fast primitive is JSON");
    assert_eq!(fast_scene["solve_status"], "primitive-complete");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(
        fast_scene["binary_blocking_overlay"],
        solved["binary_blocking_overlay"]
    );
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
}

fn collect_committed_json_frames(session: &mut WasmSession, label: &str) -> Vec<serde_json::Value> {
    let mut frames = Vec::new();
    while session
        .stage_next_json()
        .unwrap_or_else(|_| panic!("{label} event stages"))
        .is_some()
    {
        session.commit_staged_next();
        let frame = session
            .current_frame_json()
            .unwrap_or_else(|_| panic!("{label} frame serializes"));
        frames
            .push(serde_json::from_str(&frame).unwrap_or_else(|_| panic!("{label} frame is JSON")));
    }
    frames
}

#[test]
fn distance_directed_presets_dispatch_exact_trees_and_fast_trace_metric_parity() {
    for (algorithm, initialization, expected_phases) in [
        (
            "distance-directed-augmenting-path",
            "distance-directed-augmenting-path.reverse-bfs",
            "0",
        ),
        (
            "distance-directed-scaling-augmenting-path",
            "distance-directed-scaling-augmenting-path.start-scaling-phase",
            "4",
        ),
    ] {
        let source = flow_scenario_with_algorithm(algorithm);
        let mut session = WasmSession::new(&source).expect("distance-directed Scenario");
        let mut seen = BTreeMap::<String, serde_json::Value>::new();
        while session
            .stage_next_json()
            .expect("distance-directed trace event stages")
            .is_some()
        {
            session.commit_staged_next();
            let frame: serde_json::Value = serde_json::from_str(
                &session
                    .current_frame_json()
                    .expect("distance-directed frame serializes"),
            )
            .expect("distance-directed frame is JSON");
            if let Some(id) = frame["trace_event"]["catalog_id"].as_str() {
                seen.entry(id.to_owned()).or_insert(frame);
            }
        }
        let initialized = seen.get(initialization).expect("exact tree initialization");
        assert_eq!(
            initialized["pseudoflow_forest"]["arcs"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        for suffix in ["augment", "delete-node", "tree-repaired", "optimal"] {
            assert!(
                seen.contains_key(&format!("{algorithm}.{suffix}")),
                "missing {algorithm}.{suffix}"
            );
        }

        let solved: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("distance-directed result serializes"),
        )
        .expect("distance-directed result is JSON");
        assert_eq!(solved["solve_status"], "optimal");
        assert_eq!(solved["outcome"]["value"], "9");
        assert_eq!(solved["metrics"][5], expected_phases);
        assert_ne!(solved["metrics"][3], "0");
        assert_ne!(solved["metrics"][4], "0");
        assert_ne!(solved["metrics"][8], "0");
        assert_ne!(solved["metrics"][9], "0");
        assert_ne!(solved["metrics"][10], "0");
        assert_ne!(solved["metrics"][12], "0");

        let mut fast_source: serde_json::Value = serde_json::from_str(&source).expect("scenario");
        fast_source["payload"]["run_profile"] = serde_json::json!("fast");
        let mut fast = WasmSession::new(&fast_source.to_string()).expect("fast Scenario");
        while fast
            .stage_next_json()
            .expect("fast distance-directed result stages")
            .is_some()
        {
            fast.commit_staged_next();
        }
        let fast_scene: serde_json::Value = serde_json::from_str(
            &fast
                .current_frame_json()
                .expect("fast distance-directed result serializes"),
        )
        .expect("fast distance-directed result is JSON");
        assert_eq!(fast_scene["outcome"], solved["outcome"]);
        assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
        assert_eq!(fast_scene["metrics"], solved["metrics"]);
    }
}

#[test]
fn dynamic_tree_push_relabel_dispatches_fifo_tree_operations_and_exact_metrics() {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&flow_scenario_with_algorithm("dynamic-tree-push-relabel"))
            .expect("scenario");
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [{ "id": "s" }, { "id": "a" }, { "id": "b" }, { "id": "t" }],
        "edges": [
            { "id": "sa", "from": "s", "to": "a", "capacity": "9" },
            { "id": "ab", "from": "a", "to": "b", "capacity": "9" },
            { "id": "bt", "from": "b", "to": "t", "capacity": "9" }
        ]
    });
    let source = scenario.to_string();
    let mut session = WasmSession::new(&source).expect("dynamic-tree push-relabel Scenario");
    let mut linked = None;
    let mut saw_send = false;
    let mut saw_cut = false;
    let mut saw_relabel = false;
    while session
        .stage_next_json()
        .expect("dynamic-tree push-relabel event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("dynamic-tree push-relabel frame serializes"),
        )
        .expect("dynamic-tree push-relabel frame is JSON");
        match frame["trace_event"]["catalog_id"].as_str() {
            Some("dynamic-tree-push-relabel.link-small-trees") => {
                assert!(
                    frame["pseudoflow_forest"]["arcs"]
                        .as_array()
                        .is_some_and(|arcs| !arcs.is_empty())
                );
                linked.get_or_insert((
                    session.event_cursor().parse::<usize>().expect("cursor"),
                    frame,
                ));
            }
            Some("dynamic-tree-push-relabel.send-root-path") => saw_send = true,
            Some("dynamic-tree-push-relabel.cut-saturated-edge") => saw_cut = true,
            Some("dynamic-tree-push-relabel.relabel-root") => saw_relabel = true,
            _ => {}
        }
    }
    assert!(saw_send && saw_cut && saw_relabel);
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("dynamic-tree push-relabel result serializes"),
    )
    .expect("dynamic-tree push-relabel result is JSON");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "9");
    assert_eq!(solved["metrics"][0], "1");
    assert_eq!(solved["metrics"][1], "4");
    for slot in [2, 3, 4, 5, 6, 7, 9, 11, 12, 14, 15] {
        assert_ne!(solved["metrics"][slot], "0", "metric slot {slot}");
    }

    let (linked_cursor, linked_scene) = linked.expect("link event exists");
    let base: serde_json::Value =
        serde_json::from_str(&session.seek_json(0).expect("base seek")).expect("base JSON");
    assert!(base["pseudoflow_forest"].is_null());
    let replayed: serde_json::Value = serde_json::from_str(
        &session
            .seek_json(linked_cursor)
            .expect("dynamic-tree push-relabel link seek"),
    )
    .expect("replayed link scene is JSON");
    assert_eq!(replayed, linked_scene);

    scenario["payload"]["run_profile"] = serde_json::json!("fast");
    let mut fast = WasmSession::new(&scenario.to_string()).expect("fast Scenario");
    while fast
        .stage_next_json()
        .expect("fast dynamic-tree push-relabel result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast result serializes"))
            .expect("fast result JSON");
    assert_eq!(fast_scene["outcome"], solved["outcome"]);
    assert_eq!(fast_scene["edge_states"], solved["edge_states"]);
    assert_eq!(fast_scene["metrics"], solved["metrics"]);
}

#[test]
fn cubic_blocking_preflow_presets_dispatch_exact_push_and_phase_metrics() {
    for algorithm in ["karzanov-preflow", "mpm"] {
        let mut session = WasmSession::new(&flow_scenario_with_algorithm(algorithm))
            .expect("blocking-preflow Scenario is valid");
        let first = commit_until_catalog(&mut session, &format!("{algorithm}.level-bfs"));
        assert_eq!(
            first["trace_event"]["catalog_id"],
            format!("{algorithm}.level-bfs")
        );

        while session
            .stage_next_json()
            .expect("blocking-preflow trace event stages")
            .is_some()
        {
            session.commit_staged_next();
        }
        let solved: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("final blocking-preflow scene serializes"),
        )
        .expect("final blocking-preflow scene is JSON");
        assert_eq!(solved["solve_status"], "optimal");
        assert_eq!(solved["outcome"]["value"], "9");
        let metric = |index: usize| {
            solved["metrics"][index]
                .as_str()
                .expect("metric string")
                .parse::<u64>()
                .expect("canonical metric")
        };
        assert!(metric(0) > 0);
        assert!(metric(6) > 0);
        assert!(metric(11) > 0);
        assert_eq!(metric(11), metric(12) + metric(13));
        if algorithm == "karzanov-preflow" {
            assert_eq!(metric(15), 0);
        } else {
            assert_eq!(metric(14), 0);
            assert!(metric(15) > 0);
        }
    }
}

#[test]
fn isap_dispatches_reverse_bfs_relabel_and_gap_metrics() {
    let mut session =
        WasmSession::new(&flow_scenario_with_algorithm("isap")).expect("ISAP Scenario");
    let first: serde_json::Value =
        serde_json::from_str(&commit_next(&mut session)).expect("first ISAP frame is JSON");
    assert_eq!(first["trace_event"]["catalog_id"], "isap.reverse-bfs");
    let mut saw_reverse_bfs = first["trace_event"]["catalog_id"] == "isap.reverse-bfs";

    while let Some(frame) = session.stage_next_json().expect("ISAP trace event stages") {
        let frame: serde_json::Value =
            serde_json::from_str(&frame).expect("ISAP trace frame is JSON");
        saw_reverse_bfs |= frame["trace_event"]["catalog_id"] == "isap.reverse-bfs";
        session.commit_staged_next();
    }
    assert!(
        saw_reverse_bfs,
        "the source reverse-BFS operation remains observable"
    );
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final ISAP scene serializes"),
    )
    .expect("final ISAP scene is JSON");
    assert_eq!(solved["algorithm"]["id"], "isap");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "9");
    assert_eq!(solved["metrics"][7], "1");
    assert_eq!(solved["metrics"][9], "1");
    assert_eq!(solved["metrics"][10], "1");
}

#[test]
fn boykov_kolmogorov_dispatches_grow_augment_adopt_with_fast_trace_parity() {
    let trace_source = push_relabel_scenario("boykov-kolmogorov");
    let mut traced = WasmSession::new(&trace_source).expect("BK trace Scenario");
    let mut catalog_ids = Vec::new();
    let mut adoption_cursor = None;
    let mut adoption_scene = None;
    while traced
        .stage_next_json()
        .expect("BK trace event stages")
        .is_some()
    {
        traced.commit_staged_next();
        let frame: serde_json::Value =
            serde_json::from_str(&traced.current_frame_json().expect("BK frame serializes"))
                .expect("BK frame JSON");
        if let Some(id) = frame["trace_event"]["catalog_id"].as_str() {
            catalog_ids.push(id.to_owned());
            if id.starts_with("boykov-kolmogorov.adopt-")
                || id.starts_with("boykov-kolmogorov.free-")
            {
                adoption_cursor = Some(traced.cursor());
                adoption_scene = Some(frame);
            }
        }
    }
    for expected in [
        "boykov-kolmogorov.initialize",
        "boykov-kolmogorov.grow-source-tree",
        "boykov-kolmogorov.connect-trees",
        "boykov-kolmogorov.augment",
        "boykov-kolmogorov.complete-adoption",
        "boykov-kolmogorov.optimal",
    ] {
        assert!(
            catalog_ids.iter().any(|id| id == expected),
            "missing {expected}; observed {catalog_ids:?}"
        );
    }
    assert!(
        catalog_ids
            .iter()
            .all(|id| id.starts_with("boykov-kolmogorov."))
    );

    let final_cursor = traced.cursor();
    let traced_final: serde_json::Value =
        serde_json::from_str(&traced.current_frame_json().expect("BK final serializes"))
            .expect("BK final JSON");
    assert_eq!(traced_final["solve_status"], "optimal");
    assert_eq!(traced_final["outcome"]["value"], "9");
    assert_ne!(traced_final["metrics"][0], "0");
    assert_ne!(traced_final["metrics"][3], "0");
    assert_ne!(traced_final["metrics"][10], "0");

    let adoption_cursor = adoption_cursor.expect("orphan adoption/free event");
    let replayed: serde_json::Value =
        serde_json::from_str(&traced.seek_json(adoption_cursor).expect("BK adoption seek"))
            .expect("replayed BK adoption JSON");
    assert_eq!(replayed, adoption_scene.expect("adoption scene"));
    let base: serde_json::Value =
        serde_json::from_str(&traced.seek_json(0).expect("BK base seek")).expect("BK base JSON");
    assert!(base["pseudoflow_forest"].is_null());
    let forward: serde_json::Value =
        serde_json::from_str(&traced.seek_json(final_cursor).expect("BK final seek"))
            .expect("BK forward JSON");
    assert_eq!(forward, traced_final);

    let mut fast_value: serde_json::Value =
        serde_json::from_str(&trace_source).expect("BK Scenario JSON");
    fast_value["payload"]["run_profile"] = serde_json::json!("fast");
    let mut fast = WasmSession::new(&fast_value.to_string()).expect("BK fast Scenario");
    while fast.stage_next_json().expect("BK fast result").is_some() {
        fast.commit_staged_next();
    }
    let fast_final: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("BK fast final serializes"))
            .expect("BK fast final JSON");
    assert_eq!(fast_final["outcome"], traced_final["outcome"]);
    assert_eq!(fast_final["edge_states"], traced_final["edge_states"]);
    assert_eq!(fast_final["metrics"], traced_final["metrics"]);
}

#[test]
fn warm_start_push_relabel_repairs_prediction_with_replay_and_fast_trace_parity() {
    let trace_source = warm_start_push_relabel_scenario("trace");
    let mut traced = WasmSession::new(&trace_source).expect("warm-start trace Scenario");
    let ready: serde_json::Value = serde_json::from_str(
        &traced
            .current_frame_json()
            .expect("warm-start ready frame serializes"),
    )
    .expect("warm-start ready frame JSON");
    assert_eq!(ready["edge_states"][1]["flow"], "5");
    assert_eq!(ready["edge_states"][2]["flow"], "5");

    let mut catalog_ids = Vec::new();
    let mut repair_cursor = None;
    let mut repair_scene = None;
    while traced
        .stage_next_json()
        .expect("warm-start trace event stages")
        .is_some()
    {
        traced.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &traced
                .current_frame_json()
                .expect("warm-start frame serializes"),
        )
        .expect("warm-start frame JSON");
        if let Some(id) = frame["trace_event"]["catalog_id"].as_str() {
            catalog_ids.push(id.to_owned());
            if id == "warm-start-push-relabel.move-t-excess" {
                repair_cursor = Some(traced.cursor());
                repair_scene = Some(frame);
            }
        }
    }
    for expected in [
        "warm-start-push-relabel.initialize-prediction",
        "warm-start-push-relabel.saturate-cut",
        "warm-start-push-relabel.move-t-excess",
        "warm-start-push-relabel.move-s-deficit",
        "warm-start-push-relabel.recover-excess",
        "warm-start-push-relabel.recover-deficit",
        "warm-start-push-relabel.optimal",
    ] {
        assert!(
            catalog_ids.iter().any(|id| id == expected),
            "missing {expected}; observed {catalog_ids:?}"
        );
    }

    let final_cursor = traced.cursor();
    let traced_final: serde_json::Value = serde_json::from_str(
        &traced
            .current_frame_json()
            .expect("warm-start final frame serializes"),
    )
    .expect("warm-start final frame JSON");
    assert_eq!(traced_final["solve_status"], "optimal");
    assert_eq!(traced_final["outcome"]["value"], "2");
    assert_eq!(traced_final["metrics"][0], "3");
    assert_ne!(traced_final["metrics"][1], "0");
    assert_ne!(traced_final["metrics"][4], "0");
    assert_ne!(traced_final["metrics"][6], "0");

    let repair_cursor = repair_cursor.expect("sink-side repair event");
    let replayed: serde_json::Value = serde_json::from_str(
        &traced
            .seek_json(repair_cursor)
            .expect("warm-start repair seek"),
    )
    .expect("warm-start repair replay JSON");
    assert_eq!(replayed, repair_scene.expect("repair scene"));
    let base: serde_json::Value =
        serde_json::from_str(&traced.seek_json(0).expect("warm-start base seek"))
            .expect("warm-start base JSON");
    assert!(base["pseudoflow_forest"].is_null());
    let forward: serde_json::Value = serde_json::from_str(
        &traced
            .seek_json(final_cursor)
            .expect("warm-start final seek"),
    )
    .expect("warm-start forward JSON");
    assert_eq!(forward, traced_final);

    let fast_source = warm_start_push_relabel_scenario("fast");
    let mut fast = WasmSession::new(&fast_source).expect("warm-start fast Scenario");
    while fast
        .stage_next_json()
        .expect("warm-start fast result")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_final: serde_json::Value = serde_json::from_str(
        &fast
            .current_frame_json()
            .expect("warm-start fast frame serializes"),
    )
    .expect("warm-start fast frame JSON");
    assert_eq!(fast_final["outcome"], traced_final["outcome"]);
    assert_eq!(fast_final["edge_states"], traced_final["edge_states"]);
    assert_eq!(fast_final["metrics"], traced_final["metrics"]);
}

#[test]
fn synchronous_push_relabel_dispatches_round_barriers_with_fast_trace_parity() {
    let trace_source = push_relabel_scenario("synchronous-parallel-push-relabel");
    let mut traced = WasmSession::new(&trace_source).expect("synchronous trace Scenario");
    let mut catalog_ids = Vec::new();
    let mut proposal_cursor = None;
    let mut proposal_scene = None;
    while traced
        .stage_next_json()
        .expect("synchronous trace event stages")
        .is_some()
    {
        traced.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &traced
                .current_frame_json()
                .expect("synchronous trace frame serializes"),
        )
        .expect("synchronous trace frame JSON");
        if let Some(id) = frame["trace_event"]["catalog_id"].as_str() {
            catalog_ids.push(id.to_owned());
            if id == "synchronous-parallel-push-relabel.propose-round" {
                proposal_cursor = Some(traced.cursor());
                proposal_scene = Some(frame);
            }
        }
    }
    for expected in [
        "synchronous-parallel-push-relabel.initialize",
        "synchronous-parallel-push-relabel.global-relabel",
        "synchronous-parallel-push-relabel.propose-round",
        "synchronous-parallel-push-relabel.commit-round",
        "synchronous-parallel-push-relabel.optimal",
    ] {
        assert!(
            catalog_ids.iter().any(|id| id == expected),
            "missing {expected}"
        );
    }
    assert!(
        catalog_ids
            .iter()
            .all(|id| id.starts_with("synchronous-parallel-push-relabel."))
    );

    let final_cursor = traced.cursor();
    let traced_final: serde_json::Value = serde_json::from_str(
        &traced
            .current_frame_json()
            .expect("synchronous final serializes"),
    )
    .expect("synchronous final JSON");
    assert_eq!(traced_final["solve_status"], "optimal");
    assert_eq!(traced_final["outcome"]["value"], "9");
    assert_ne!(traced_final["metrics"][6], "0");
    assert_ne!(traced_final["metrics"][9], "0");

    let proposal_cursor = proposal_cursor.expect("proposal cursor");
    let proposal_scene = proposal_scene.expect("proposal scene");
    let base: serde_json::Value =
        serde_json::from_str(&traced.seek_json(0).expect("reverse seek")).expect("base JSON");
    assert_eq!(base["solve_status"], "ready");
    let replayed_proposal: serde_json::Value = serde_json::from_str(
        &traced
            .seek_json(proposal_cursor)
            .expect("proposal forward replay"),
    )
    .expect("proposal replay JSON");
    assert_eq!(replayed_proposal, proposal_scene);
    let replayed_final: serde_json::Value = serde_json::from_str(
        &traced
            .seek_json(final_cursor)
            .expect("final forward replay"),
    )
    .expect("final replay JSON");
    assert_eq!(replayed_final, traced_final);

    let mut fast_source: serde_json::Value =
        serde_json::from_str(&trace_source).expect("fast source JSON");
    fast_source["payload"]["run_profile"] = serde_json::json!("fast");
    let mut fast = WasmSession::new(&fast_source.to_string()).expect("synchronous fast Scenario");
    while fast
        .stage_next_json()
        .expect("synchronous fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_final: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast final serializes"))
            .expect("fast final JSON");
    assert_eq!(fast_final["edge_states"], traced_final["edge_states"]);
    assert_eq!(fast_final["outcome"], traced_final["outcome"]);
    assert_eq!(fast_final["metrics"], traced_final["metrics"]);
}

#[test]
fn push_relabel_presets_dispatch_height_excess_and_partitioned_push_metrics() {
    for algorithm in [
        "generic-push-relabel",
        "fifo-push-relabel",
        "relabel-to-front",
        "highest-label-push-relabel",
        "excess-scaling-push-relabel",
        "partial-augment-relabel-max-flow",
        "current-arc-heuristic",
        "global-relabel-heuristic",
        "gap-relabel-heuristic",
    ] {
        let mut session =
            WasmSession::new(&push_relabel_scenario(algorithm)).expect("push-relabel Scenario");
        let first: serde_json::Value = serde_json::from_str(&commit_next(&mut session))
            .expect("initial preflow frame is JSON");
        assert_eq!(
            first["trace_event"]["catalog_id"],
            format!("{algorithm}.initialize")
        );
        assert_eq!(first["metrics"][11], "2");
        let source = first["node_trace_states"]
            .as_array()
            .expect("trace states")
            .iter()
            .find(|node| node["node_id"] == "s")
            .expect("source trace state");
        assert_eq!(source["label"], "4");
        assert_eq!(source["remaining_divergence"], "-9");

        while session
            .stage_next_json()
            .expect("push-relabel trace event stages")
            .is_some()
        {
            session.commit_staged_next();
        }
        let solved: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("final push-relabel scene serializes"),
        )
        .expect("final push-relabel scene is JSON");
        assert_eq!(solved["solve_status"], "optimal");
        assert_eq!(solved["outcome"]["value"], "9");
        let metric = |index: usize| {
            solved["metrics"][index]
                .as_str()
                .expect("metric string")
                .parse::<u64>()
                .expect("canonical metric")
        };
        assert_eq!(metric(11), metric(12) + metric(13));
        if algorithm == "partial-augment-relabel-max-flow" {
            assert!(metric(3) > 0);
            assert!(metric(4) > 0);
            assert_eq!(metric(14), 0);
            assert!(metric(15) > 0);
        } else if algorithm == "excess-scaling-push-relabel" {
            assert!(metric(5) > 0);
            assert_eq!(metric(14), 0);
            assert!(metric(15) > 0);
        } else {
            assert_eq!(metric(14), metric(15));
        }
        if algorithm == "global-relabel-heuristic" {
            assert!(metric(9) > 0);
            assert_eq!(metric(10), 0);
        }
        if algorithm == "gap-relabel-heuristic" {
            assert_eq!(metric(9), 0);
        }
        assert!(metric(7) > 0);
    }
}

#[test]
fn pseudoflow_dispatches_normalized_forest_partition_and_recovery_metrics() {
    let mut session = WasmSession::new(&push_relabel_scenario("hochbaum-pseudoflow"))
        .expect("pseudoflow Scenario");
    let first: serde_json::Value =
        serde_json::from_str(&commit_next(&mut session)).expect("initial pseudoflow frame");
    assert_eq!(
        first["trace_event"]["catalog_id"],
        "hochbaum-pseudoflow.initialize"
    );
    assert_eq!(
        first["pseudoflow_forest"]["strong_nodes"],
        serde_json::json!(["a"])
    );
    assert_eq!(first["node_trace_states"][0]["label"], "1");

    let mut saw_merge = false;
    let mut saw_forest_arc = false;
    while session
        .stage_next_json()
        .expect("pseudoflow trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("pseudoflow frame serializes"),
        )
        .expect("pseudoflow frame JSON");
        saw_merge |= frame["trace_event"]["catalog_id"] == "hochbaum-pseudoflow.merge";
        saw_forest_arc |= frame["pseudoflow_forest"]["arcs"]
            .as_array()
            .is_some_and(|arcs| !arcs.is_empty());
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final pseudoflow scene serializes"),
    )
    .expect("final pseudoflow scene JSON");
    assert!(saw_merge);
    assert!(saw_forest_arc);
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "9");
    assert!(
        solved["metrics"][15]
            .as_str()
            .is_some_and(|value| value != "0")
    );
}

#[test]
fn pseudoflow_simplex_dispatches_enter_leave_basis_pivots_and_fast_parity() {
    let scenario = push_relabel_scenario("pseudoflow-simplex");
    let mut session = WasmSession::new(&scenario).expect("simplex Scenario");
    let first: serde_json::Value =
        serde_json::from_str(&commit_next(&mut session)).expect("simplex initial frame");
    assert_eq!(
        first["trace_event"]["catalog_id"],
        "pseudoflow-simplex.initialize"
    );

    let mut selections = 0_u64;
    let mut pivots = 0_u64;
    let mut saw_basis = false;
    while session
        .stage_next_json()
        .expect("simplex trace event stages")
        .is_some()
    {
        session.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &session
                .current_frame_json()
                .expect("simplex frame serializes"),
        )
        .expect("simplex frame JSON");
        let event = frame["trace_event"]["catalog_id"]
            .as_str()
            .unwrap_or_default();
        selections += u64::from(event == "pseudoflow-simplex.select-entering");
        pivots += u64::from(event.starts_with("pseudoflow-simplex.pivot-"));
        saw_basis |= frame["pseudoflow_forest"]["arcs"]
            .as_array()
            .is_some_and(|arcs| !arcs.is_empty());
    }
    let traced: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("simplex final scene serializes"),
    )
    .expect("simplex final scene JSON");
    assert_eq!(traced["solve_status"], "optimal");
    assert_eq!(traced["outcome"]["value"], "9");
    assert_eq!(selections, pivots);
    assert!(pivots > 0);
    assert!(saw_basis);
    assert_eq!(traced["metrics"][15], pivots.to_string());
    assert!(
        traced["metrics"][4]
            .as_str()
            .is_some_and(|value| value != "0")
    );

    let mut fast_value =
        serde_json::from_str::<serde_json::Value>(&scenario).expect("Scenario JSON");
    fast_value["payload"]["run_profile"] = serde_json::json!("fast");
    let fast_scenario = serde_json::to_string(&fast_value).expect("fast Scenario");
    let mut fast = WasmSession::new(&fast_scenario).expect("fast simplex Scenario");
    while fast.stage_next_json().expect("fast frame stages").is_some() {
        fast.commit_staged_next();
    }
    let fast_frame: serde_json::Value =
        serde_json::from_str(&fast.current_frame_json().expect("fast frame serializes"))
            .expect("fast frame JSON");
    assert_eq!(fast_frame["outcome"], traced["outcome"]);
    assert_eq!(fast_frame["edge_states"], traced["edge_states"]);
    assert_eq!(fast_frame["metrics"], traced["metrics"]);
}

#[test]
fn dfs_ford_fulkerson_dispatches_with_an_exact_path_search_metric() {
    let mut session = WasmSession::new(&flow_scenario_with_algorithm("dfs-ford-fulkerson"))
        .expect("DFS Ford-Fulkerson Scenario is valid");
    let first: serde_json::Value = serde_json::from_str(&commit_next(&mut session))
        .expect("first DFS Ford-Fulkerson frame is JSON");
    assert_eq!(
        first["trace_event"]["catalog_id"],
        "dfs-ford-fulkerson.search"
    );
    assert_eq!(first["metrics"][0], "0");
    assert_eq!(first["metrics"][4], "1");
    let mut path_build = first;
    while ![
        "dfs-ford-fulkerson.extend-path-prefix",
        "dfs-ford-fulkerson.complete-search",
    ]
    .contains(
        &path_build["trace_event"]["catalog_id"]
            .as_str()
            .expect("DFS Ford-Fulkerson trace catalog is text"),
    ) {
        path_build = serde_json::from_str(&commit_next(&mut session))
            .expect("DFS Ford-Fulkerson path-build frame is JSON");
    }
    assert_eq!(path_build["metrics"][0], "0");
    assert_eq!(path_build["metrics"][4], "1");

    while session
        .stage_next_json()
        .expect("DFS Ford-Fulkerson trace event stages")
        .is_some()
    {
        session.commit_staged_next();
    }
    let solved: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("final DFS Ford-Fulkerson scene serializes"),
    )
    .expect("final DFS Ford-Fulkerson scene is JSON");
    assert_eq!(solved["algorithm"]["id"], "dfs-ford-fulkerson");
    assert_eq!(solved["solve_status"], "optimal");
    assert_eq!(solved["outcome"]["value"], "9");
    assert_eq!(solved["metrics"][0], "0");
    let augmentations = solved["metrics"][3]
        .as_str()
        .expect("augmentation count is a string")
        .parse::<u64>()
        .expect("augmentation count is canonical");
    let path_searches = solved["metrics"][4]
        .as_str()
        .expect("path search count is a string")
        .parse::<u64>()
        .expect("path search count is canonical");
    assert_eq!(path_searches, augmentations + 1);
}

#[test]
fn runtime_handshake_export_matches_the_frozen_contract() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../fixtures/contracts/engine-contract-v1.json"
    ))
    .expect("engine fixture is JSON");

    assert_eq!(
        engine_contract_json().expect("contract canonicalizes"),
        fixture["canonical"]
            .as_str()
            .expect("fixture canonical is a string")
    );
}

#[test]
fn generator_fixture_manifest_export_is_total_and_strict_json() {
    let manifest: serde_json::Value = serde_json::from_str(
        &flow_generator_fixture_manifest_json().expect("fixture manifest canonicalizes"),
    )
    .expect("fixture manifest is JSON");
    let fixtures = manifest.as_array().expect("fixture manifest is an array");
    assert_eq!(fixtures.len(), 50);
    let catalog_size = algorithm_catalog().len();
    for fixture in fixtures {
        assert_eq!(fixture["presets"].as_array().expect("presets").len(), 3);
        assert_eq!(
            fixture["algorithm_compatibility"]
                .as_array()
                .expect("compatibility matrix")
                .len(),
            catalog_size
        );
    }
    assert_eq!(fixtures[0]["family_id"], "arborescence");
    assert_eq!(fixtures[49]["family_id"], "zadeh-phase-chain-stress");
}

#[test]
fn canonical_trace_and_fast_generator_presets_admit_their_default_algorithm() {
    for fixture in generator_algorithm_fixtures() {
        for preset in fixture.presets.iter().take(2) {
            let scenario = generator_fixture_scenario(&fixture, preset);
            let session = FlowSession::new(&scenario).unwrap_or_else(|error| {
                panic!(
                    "{} {:?} default {} failed runtime admission: {error:?}",
                    fixture.family_id, preset.purpose, fixture.default_algorithm_id
                )
            });
            assert_eq!(
                session.scenario.payload.algorithm.id,
                fixture.default_algorithm_id
            );
        }
    }
}

#[test]
fn canonical_boundary_generator_presets_fit_default_display_admission() {
    for fixture in generator_algorithm_fixtures() {
        let preset = &fixture.presets[2];
        let scenario = generator_fixture_scenario(&fixture, preset);
        let session = FlowSession::new(&scenario).unwrap_or_else(|error| {
            panic!(
                "{} boundary default {} failed runtime admission: {error:?}",
                fixture.family_id, fixture.default_algorithm_id
            )
        });
        assert!(
            !session.resource_admission_limited,
            "{} boundary default {} cannot be loaded into the graph workspace",
            fixture.family_id, fixture.default_algorithm_id
        );
    }
}

#[test]
fn canonical_trace_generator_presets_prepare_their_default_solver() {
    for fixture in generator_algorithm_fixtures() {
        let preset = &fixture.presets[0];
        eprintln!(
            "preparing {} with {}",
            fixture.family_id, fixture.default_algorithm_id
        );
        let scenario = generator_fixture_scenario(&fixture, preset);
        let mut session = FlowSession::new(&scenario).unwrap_or_else(|error| {
            panic!(
                "{} trace default {} failed runtime admission: {error:?}",
                fixture.family_id, fixture.default_algorithm_id
            )
        });
        let staged = session
            .stage_next_json()
            .unwrap_or_else(|error| {
                panic!(
                    "{} trace default {} failed solve preparation: {error:?}",
                    fixture.family_id, fixture.default_algorithm_id
                )
            })
            .unwrap_or_else(|| {
                panic!(
                    "{} trace default {} produced no event",
                    fixture.family_id, fixture.default_algorithm_id
                )
            });
        let scene: serde_json::Value =
            serde_json::from_str(&staged).expect("staged trace frame is JSON");
        assert_eq!(
            scene["solve_status"], "running",
            "{} trace default {} must begin a real source trace",
            fixture.family_id, fixture.default_algorithm_id
        );
        assert!(
            scene["trace_event"]["catalog_id"].is_string(),
            "{} trace default {} lacks source event identity",
            fixture.family_id,
            fixture.default_algorithm_id
        );
        assert!(
            scene["event_count"]
                .as_str()
                .and_then(|count| count.parse::<u64>().ok())
                .is_some_and(|count| count > 1),
            "{} trace default {} is not a multi-step teaching trace",
            fixture.family_id,
            fixture.default_algorithm_id
        );
    }
}

#[test]
fn visual_default_netgen_prepares_a_real_network_simplex_source_trace() {
    let candidate = serde_json::to_value(
        generate_flow_graph_candidate(
            &serde_json::json!({
                "generator_revision": "flow-generator/27",
                "seed": "42",
                "family": {
                    "family_id": "netgen-skeleton",
                    "nodes": 24,
                    "sources": 3,
                    "sinks": 4,
                    "edge_count": 80,
                    "minimum_cost": -5,
                    "maximum_cost": 20,
                    "total_supply": 60,
                    "transshipment_sources": 1,
                    "transshipment_sinks": 1,
                    "high_cost_percentage": 75,
                    "capacitated_percentage": 65,
                    "minimum_capacity": 2,
                    "maximum_capacity": 30
                },
                "capacity": { "kind": "unit" },
                "cost": { "kind": "zero" }
            })
            .to_string(),
        )
        .expect("visual NETGEN default generates"),
    )
    .expect("generated NETGEN candidate serializes");
    let scenario = serde_json::json!({
        "schema_version": 1,
        "scenario_encoding_revision": "rfc8785-jcs/1",
        "plugin": "flow",
        "reproducibility": { "declared": {
            "algorithm_revision": "flow-algorithms/8",
            "rng_version": 1,
            "plugin_result_revision": "flow-result/9",
            "metrics_catalog_revision": "flow-metrics/6",
            "trace_revision": "flow-trace/9",
            "projection_revision": "flow-projection/6",
            "layout_revision": "flow-layout/1",
            "frame_encoding_revision": "flow-scene/9"
        }},
        "payload": {
            "model": candidate["suggested_model"].clone(),
            "graph": candidate["graph"].clone(),
            "algorithm": { "id": "primal-network-simplex", "config": {} },
            "run_profile": "trace",
            "trace_granularity": "operation",
            "algorithm_seed": "0"
        }
    })
    .to_string();
    let mut session = FlowSession::new(&scenario).expect("visual NETGEN Scenario is valid");
    let ready: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("NETGEN ready frame serializes"),
    )
    .expect("NETGEN ready frame is JSON");
    let mut saw_internal_original_flow = false;
    while let Some(staged) = session
        .stage_next_json()
        .expect("NETGEN trace event stages")
    {
        let frame: serde_json::Value =
            serde_json::from_str(&staged).expect("NETGEN trace event is JSON");
        assert_eq!(frame["solve_status"], "running");
        assert!(frame["trace_event"]["catalog_id"].is_string());
        if frame["trace_event"]["catalog_id"]
            .as_str()
            .is_some_and(|catalog_id| catalog_id.starts_with("feasibility."))
        {
            let overlay = frame
                .get("feasibility_overlay")
                .expect("NETGEN feasibility precheck owns an overlay");
            assert_eq!(overlay["use_kind"], "precheck-only");
            assert_eq!(frame["edge_states"], ready["edge_states"]);
            assert_eq!(frame["residual_arcs"], ready["residual_arcs"]);
            saw_internal_original_flow |= overlay["arcs"]
                .as_array()
                .expect("NETGEN feasibility arcs")
                .iter()
                .any(|arc| arc["arc"]["kind"] == "original" && arc["flow"] != "0");
        }
        session.commit_staged_next();
        if saw_internal_original_flow {
            break;
        }
    }
    assert!(
        saw_internal_original_flow,
        "NETGEN precheck never exposed its internal original-edge flow"
    );
}

#[test]
fn canonical_fast_generator_presets_compute_a_terminal_result() {
    for fixture in generator_algorithm_fixtures() {
        let preset = &fixture.presets[1];
        let scenario = generator_fixture_scenario(&fixture, preset);
        let mut session = FlowSession::new(&scenario).unwrap_or_else(|error| {
            panic!(
                "{} fast default {} failed runtime admission: {error:?}",
                fixture.family_id, fixture.default_algorithm_id
            )
        });
        let staged = session
            .stage_next_json()
            .unwrap_or_else(|error| {
                panic!(
                    "{} fast default {} failed solve preparation: {error:?}",
                    fixture.family_id, fixture.default_algorithm_id
                )
            })
            .unwrap_or_else(|| {
                panic!(
                    "{} fast default {} produced no result",
                    fixture.family_id, fixture.default_algorithm_id
                )
            });
        let scene: serde_json::Value =
            serde_json::from_str(&staged).expect("staged fast frame is JSON");
        assert!(
            matches!(
                scene["solve_status"].as_str(),
                Some("optimal" | "infeasible" | "primitive-complete")
            ),
            "{} fast default {} did not compute a terminal result: {}",
            fixture.family_id,
            fixture.default_algorithm_id,
            scene["solve_status"]
        );
    }
}

#[test]
// The explicit ordered inventory is intentionally kept in one assertion so
// catalog drift cannot hide behind generated or split expectations.
#[allow(clippy::too_many_lines)]
fn flow_catalog_export_marks_only_production_ready_descriptors_executable() {
    let catalog: serde_json::Value =
        serde_json::from_str(&flow_algorithm_catalog_json().expect("flow catalog canonicalizes"))
            .expect("flow catalog is JSON");
    let executable = catalog
        .as_array()
        .expect("catalog is an array")
        .iter()
        .filter(|entry| entry["status"] == "executable")
        .map(|entry| entry["id"].as_str().expect("catalog id"))
        .collect::<Vec<_>>();
    assert_eq!(
        executable,
        [
            "ford-fulkerson",
            "dfs-ford-fulkerson",
            "edmonds-karp",
            "shortest-augmenting-path",
            "isap",
            "widest-augmenting-path",
            "capacity-scaling-augmenting-path",
            "distance-directed-augmenting-path",
            "distance-directed-scaling-augmenting-path",
            "dinic",
            "unit-capacity-dinic",
            "unit-network-dinic",
            "karzanov-preflow",
            "mpm",
            "dynamic-tree-blocking-flow",
            "binary-blocking-flow",
            "goldberg-rao",
            "generic-push-relabel",
            "fifo-push-relabel",
            "relabel-to-front",
            "highest-label-push-relabel",
            "excess-scaling-push-relabel",
            "dynamic-tree-push-relabel",
            "partial-augment-relabel-max-flow",
            "synchronous-parallel-push-relabel",
            "current-arc-heuristic",
            "global-relabel-heuristic",
            "gap-relabel-heuristic",
            "hochbaum-pseudoflow",
            "pseudoflow-simplex",
            "parametric-pseudoflow",
            "parametric-breakpoint-rerun",
            "boykov-kolmogorov",
            "ibfs",
            "eibfs",
            "hopcroft-karp",
            "hassin-st-planar",
            "borradaile-klein-planar",
            "electrical-flow",
            "augmenting-electrical-flow",
            "interior-point-max-flow",
            "minimum-ratio-cycle-max-flow",
            "orlin-max-flow",
            "randomized-almost-linear-max-flow-oracle-demonstrator",
            "deterministic-almost-linear-max-flow-oracle-demonstrator",
            "weighted-augmenting-paths",
            "weighted-push-relabel",
            "dynamic-eibfs",
            "warm-start-push-relabel",
            "simple-cycle-canceling",
            "minimum-mean-cycle-canceling",
            "cancel-and-tighten",
            "relaxed-most-negative-cycle",
            "successive-shortest-path",
            "bellman-ford-ssp",
            "potential-dijkstra-ssp",
            "successive-shortest-augmenting-path",
            "primal-dual-mcf",
            "blocking-flow-primal-dual",
            "capacity-scaling-mcf",
            "enhanced-capacity-scaling",
            "cost-scaling",
            "cost-scaling-push-relabel",
            "augment-relabel",
            "partial-augment-relabel-mcf",
            "price-refinement",
            "arc-fixing",
            "excess-scaling-mcf",
            "double-scaling",
            "generalized-cost-scaling",
            "primal-network-simplex",
            "dual-network-simplex",
            "polynomial-primal-network-simplex",
            "polynomial-dual-network-simplex",
            "dynamic-tree-network-simplex",
            "transportation-simplex",
            "modi",
            "out-of-kilter",
            "relaxation",
            "epsilon-relaxation",
            "hungarian",
            "auction",
            "tardos-framework",
            "orlin-mcf",
            "primal-dual-interior-point-mcf",
            "electrical-flow-interior-point-mcf",
            "minimum-ratio-cycle-mcf",
            "randomized-almost-linear-mcf-oracle-demonstrator",
            "deterministic-almost-linear-mcf",
            "segment-expanded-convex-mcf",
            "convex-cost-scaling",
            "convex-network-simplex",
            "prediction-assisted-epsilon-relaxation",
        ]
    );
}

#[test]
fn flow_conformance_contract_export_is_total_and_catalog_ordered() {
    let contracts: serde_json::Value = serde_json::from_str(
        &flow_algorithm_conformance_contracts_json().expect("conformance contracts canonicalize"),
    )
    .expect("source contracts are JSON");
    let contracts = contracts.as_array().expect("source contracts are an array");
    assert_eq!(contracts.len(), algorithm_catalog().len());
    for (descriptor, contract) in algorithm_catalog().iter().zip(contracts) {
        assert_eq!(contract["schema_revision"], "flow-algorithm-conformance/2");
        assert_eq!(contract["algorithm_id"], descriptor.id);
        assert_eq!(contract["algorithm_anchor"], descriptor.title);
        assert_eq!(
            contract["runtime_route"],
            serde_json::to_value(descriptor.runtime_route).expect("runtime route serializes")
        );
        assert_eq!(
            contract["checker_contract_kind"],
            serde_json::to_value(flow::checker_contract_kind(descriptor.algorithm_id))
                .expect("checker contract kind serializes")
        );
        assert!(contract["compatible_generator_fixture_ids"].is_array());
        assert_eq!(contract["source"]["source_id"], descriptor.source_id);
        assert!(
            contract["source"]["fixed_source"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
    }
}

#[test]
fn runtime_algorithm_ids_are_parsed_through_the_closed_typed_catalog() {
    assert_eq!(
        validate_runtime_algorithm("edmonds-karp"),
        Ok(AlgorithmId::EdmondsKarp)
    );
    assert_eq!(
        validate_runtime_algorithm("augmenting-electrical-flow"),
        Ok(AlgorithmId::AugmentingElectricalFlow)
    );
    assert_eq!(
        validate_runtime_algorithm("interior-point-max-flow"),
        Ok(AlgorithmId::InteriorPointMaxFlow)
    );
    assert_eq!(
        validate_runtime_algorithm("minimum-ratio-cycle-max-flow"),
        Ok(AlgorithmId::MinimumRatioCycleMaxFlow)
    );
    assert_eq!(
        validate_runtime_algorithm("minimum-ratio-cycle-mcf"),
        Ok(AlgorithmId::MinimumRatioCycleMcf)
    );
    assert_eq!(
        validate_runtime_algorithm("not-a-flow-algorithm"),
        Err("flow algorithm is not present in the catalog")
    );
    assert_eq!(
        validate_runtime_algorithm("EDMONDS-KARP"),
        Err("flow algorithm is not present in the catalog")
    );
}

#[test]
fn convex_cost_dispatches_segment_occupancy_marginals_and_exact_objective() {
    let mut session = WasmSession::new(&convex_cost_scenario()).expect("session initializes");
    let ready: serde_json::Value = serde_json::from_str(
        &session
            .current_frame_json()
            .expect("ready scene serializes"),
    )
    .expect("ready scene is JSON");
    assert_eq!(ready["model"]["kind"], "convex-cost-flow");
    assert!(ready.get("convex_cost_overlay").is_none());

    let mut frames = Vec::new();
    while session.stage_next_json().expect("next stages").is_some() {
        session.commit_staged_next();
        frames.push(
            serde_json::from_str::<serde_json::Value>(
                &session.current_frame_json().expect("scene serializes"),
            )
            .expect("scene is JSON"),
        );
        if frames
            .last()
            .and_then(|frame| frame["trace_event"]["catalog_id"].as_str())
            == Some("segment-expanded-convex-mcf.inspect-residual-arc")
        {
            let inspected = frames.last().expect("frame exists");
            assert_eq!(
                inspected["trace_event"]["entity_refs"]
                    .as_array()
                    .map(Vec::len),
                Some(1),
                "one inspected marginal residual arc is identified"
            );
            assert_eq!(
                inspected["trace_event"]["entity_refs"][0]["kind"],
                "residual-arc"
            );
            assert!(
                inspected["trace_event"]["detail"]["label"]
                    .as_str()
                    .is_some_and(|label| label.contains("arc-cost")),
                "scale-aware detail retains the source measurement"
            );
        }
    }
    assert!(!frames.is_empty());
    assert!(frames.iter().any(|frame| {
        frame["convex_cost_overlay"]["stage"] == "select-minimum-mean-cycle"
            && frame["convex_cost_overlay"]["active_cycle"]
                .as_array()
                .is_some_and(|cycle| !cycle.is_empty())
    }));
    let final_frame = frames.last().expect("final frame exists");
    assert_eq!(final_frame["solve_status"], "optimal");
    assert_eq!(final_frame["outcome"]["total_cost"], "11");
    assert_eq!(final_frame["convex_cost_overlay"]["stage"], "optimal");
    assert_eq!(
        final_frame["convex_cost_overlay"]["edges"][0]["segments"][0]["flow"],
        "1"
    );
    assert_eq!(
        final_frame["convex_cost_overlay"]["edges"][0]["forward_marginal_cost"],
        "5"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn convex_cost_scaling_dispatches_native_delta_paths_and_matches_fast_profile() {
    let mut traced = WasmSession::new(&convex_cost_scaling_scenario("trace"))
        .expect("native convex scaling trace initializes");
    let mut ids = Vec::new();
    let mut selected_cursor = None;
    let mut selected_scene = None;
    let mut potential_update_scene = None;
    while traced
        .stage_next_json()
        .expect("native convex event stages")
        .is_some()
    {
        traced.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &traced
                .current_frame_json()
                .expect("native convex scene serializes"),
        )
        .expect("native convex scene JSON");
        if let Some(id) = frame["trace_event"]["catalog_id"].as_str() {
            ids.push(id.to_owned());
            if id == "convex-cost-scaling.shortest-marginal-residual-path"
                && frame["convex_cost_overlay"]["active_cycle"]
                    .as_array()
                    .is_some_and(|path| !path.is_empty())
            {
                selected_cursor = Some(traced.cursor());
                selected_scene = Some(frame.clone());
            }
            if id == "convex-cost-scaling.update-reduced-cost-potentials"
                && potential_update_scene.is_none()
            {
                potential_update_scene = Some(frame.clone());
            }
        }
        let eligible = frame["convex_cost_overlay"]["eligible_arcs"]
            .as_array()
            .map_or(0, Vec::len);
        assert!(eligible <= 6, "at most two marginal pieces per edge");
    }
    for expected in [
        "convex-cost-scaling.initialize-marginal-residual",
        "convex-cost-scaling.start-delta-scale",
        "convex-cost-scaling.shortest-marginal-residual-path",
        "convex-cost-scaling.update-reduced-cost-potentials",
        "convex-cost-scaling.augment-to-breakpoint",
        "convex-cost-scaling.complete-delta-scale",
        "convex-cost-scaling.certify-expanded-oracle",
    ] {
        assert!(ids.iter().any(|id| id == expected), "missing {expected}");
    }
    let final_scene: serde_json::Value = serde_json::from_str(
        &traced
            .current_frame_json()
            .expect("native convex final serializes"),
    )
    .expect("native convex final JSON");
    assert_eq!(final_scene["solve_status"], "optimal");
    assert_eq!(final_scene["outcome"]["total_cost"], "11");
    assert_eq!(final_scene["convex_cost_overlay"]["scale"], "1");
    assert_ne!(final_scene["metrics"][0], "0");
    assert_ne!(final_scene["metrics"][6], "0");

    let cursor = selected_cursor.expect("selected native marginal path");
    let selected = selected_scene.expect("selected native marginal scene");
    assert_eq!(
        selected["trace_event"]["entity_refs"],
        serde_json::json!([])
    );
    assert!(
        selected["convex_cost_overlay"]["active_cycle"]
            .as_array()
            .is_some_and(|path| !path.is_empty())
    );
    let potential_update = potential_update_scene.expect("native convex dual update");
    assert_eq!(
        potential_update["trace_event"]["entity_refs"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "the shortest-path cutoff has one exact deficit-node owner"
    );
    assert_eq!(
        potential_update["trace_event"]["entity_refs"][0]["kind"],
        "node"
    );
    let replayed: serde_json::Value = serde_json::from_str(
        &traced
            .seek_json(cursor)
            .expect("native convex reverse seek"),
    )
    .expect("native convex replay JSON");
    assert_eq!(replayed, selected);

    let mut fast = WasmSession::new(&convex_cost_scaling_scenario("fast"))
        .expect("native convex fast initializes");
    while fast
        .stage_next_json()
        .expect("native convex fast result stages")
        .is_some()
    {
        fast.commit_staged_next();
    }
    let fast_scene: serde_json::Value = serde_json::from_str(
        &fast
            .current_frame_json()
            .expect("native convex fast serializes"),
    )
    .expect("native convex fast JSON");
    assert_eq!(fast_scene["outcome"], final_scene["outcome"]);
    assert_eq!(fast_scene["edge_states"], final_scene["edge_states"]);
    assert_eq!(fast_scene["metrics"], final_scene["metrics"]);
}

#[test]
fn convex_network_simplex_dispatches_combined_pivots_and_matches_fast_profile() {
    let mut traced = WasmSession::new(&convex_network_simplex_scenario("trace"))
        .expect("convex network-simplex trace initializes");
    let mut ids = Vec::new();
    let mut crossings_since_cycle = 0_usize;
    let mut saw_multi_crossing_close = false;
    let mut crossing_cursor = None;
    let mut crossing_scene = None;
    while traced
        .stage_next_json()
        .expect("convex simplex event stages")
        .is_some()
    {
        traced.commit_staged_next();
        let frame: serde_json::Value = serde_json::from_str(
            &traced
                .current_frame_json()
                .expect("convex simplex scene serializes"),
        )
        .expect("convex simplex scene JSON");
        let Some(id) = frame["trace_event"]["catalog_id"].as_str() else {
            continue;
        };
        ids.push(id.to_owned());
        if id == "convex-network-simplex.form-fundamental-cycle" {
            crossings_since_cycle = 0;
            assert!(
                frame["convex_network_simplex_overlay"]["cycle"]
                    .as_array()
                    .is_some_and(|cycle| !cycle.is_empty())
            );
        } else if id == "convex-network-simplex.cross-segment-breakpoint" {
            crossings_since_cycle += 1;
            if crossing_cursor.is_none() {
                crossing_cursor = Some(traced.cursor());
                crossing_scene = Some(frame.clone());
            }
        } else if matches!(
            id,
            "convex-network-simplex.exchange-basis" | "convex-network-simplex.flip-entering-bound"
        ) {
            saw_multi_crossing_close |= crossings_since_cycle > 1;
        }
    }
    for expected in [
        "convex-network-simplex.initialize-compact-basis",
        "convex-network-simplex.price-forward-backward",
        "convex-network-simplex.form-fundamental-cycle",
        "convex-network-simplex.cross-segment-breakpoint",
        "convex-network-simplex.exchange-basis",
        "convex-network-simplex.certify-expanded-oracle",
    ] {
        assert!(ids.iter().any(|id| id == expected), "missing {expected}");
    }
    assert!(saw_multi_crossing_close);
    let final_cursor = traced.cursor();
    let final_scene: serde_json::Value = serde_json::from_str(
        &traced
            .current_frame_json()
            .expect("convex simplex final serializes"),
    )
    .expect("convex simplex final JSON");
    assert_eq!(final_scene["solve_status"], "optimal");
    assert_eq!(final_scene["outcome"]["total_cost"], "7");
    assert_eq!(
        final_scene["convex_network_simplex_overlay"]["stage"],
        "optimal"
    );
    assert_ne!(final_scene["metrics"][1], "0");
    assert_ne!(final_scene["metrics"][3], "0");
    assert_ne!(final_scene["metrics"][6], "0");

    let cursor = crossing_cursor.expect("breakpoint cursor");
    let selected = crossing_scene.expect("breakpoint scene");
    let replayed: serde_json::Value = serde_json::from_str(
        &traced
            .seek_json(cursor)
            .expect("convex simplex reverse seek"),
    )
    .expect("convex simplex replay JSON");
    assert_eq!(replayed, selected);
    let replayed_final: serde_json::Value = serde_json::from_str(
        &traced
            .seek_json(final_cursor)
            .expect("convex simplex forward seek"),
    )
    .expect("convex simplex final replay JSON");
    assert_eq!(replayed_final, final_scene);

    let fast_scene = convex_network_simplex_fast_scene();
    assert_eq!(fast_scene["outcome"], final_scene["outcome"]);
    assert_eq!(fast_scene["edge_states"], final_scene["edge_states"]);
    assert_eq!(fast_scene["metrics"], final_scene["metrics"]);
    assert_eq!(
        fast_scene["convex_network_simplex_overlay"],
        final_scene["convex_network_simplex_overlay"]
    );
}

#[test]
fn staged_step_advances_cursor_only_after_commit_and_can_be_discarded() {
    let mut session = WasmSession::new(&scenario(false)).unwrap();
    let before = session.current_frame_json().unwrap();
    let staged = session
        .stage_next_json()
        .unwrap()
        .expect("timeline has a next item");

    assert_eq!(session.cursor(), 0);
    session.discard_staged_next().unwrap();
    assert_eq!(session.cursor(), 0);
    assert_eq!(session.current_frame_json().unwrap(), before);

    assert_eq!(session.stage_next_json().unwrap(), Some(staged));
    session.commit_staged_next();
    assert_eq!(session.cursor(), 1);
    assert_ne!(session.current_frame_json().unwrap(), before);
}

#[test]
fn serialization_limit_failure_does_not_stage_or_advance() {
    let mut session = WasmSession::new(&scenario(false)).unwrap();
    let before = session.current_frame_json().unwrap();

    assert!(session.stage_next_json_with_limit(1).is_err());
    assert_eq!(session.cursor(), 0);
    assert!(session.ordered_map_mut().staged_next.is_none());
    assert_eq!(session.current_frame_json().unwrap(), before);
}

#[test]
fn serialization_failure_restores_a_nonzero_committed_boundary() {
    let mut session = WasmSession::new(&scenario(false)).unwrap();
    commit_next(&mut session);
    let committed = session.current_frame_json().unwrap();

    assert!(session.stage_next_json_with_limit(1).is_err());
    assert_eq!(session.cursor(), 1);
    assert!(session.ordered_map_mut().staged_next.is_none());
    assert_eq!(session.current_frame_json().unwrap(), committed);

    commit_next(&mut session);
    assert_eq!(session.cursor(), 2);
}

#[test]
fn visible_initial_build_and_seek_are_exact() {
    let mut session = WasmSession::new(&scenario(true)).unwrap();
    assert_eq!(session.item_count(), 4);
    let first: serde_json::Value = serde_json::from_str(&commit_next(&mut session)).unwrap();
    assert_eq!(first["initialBuild"], true);
    let at_three = session.seek_json(3).unwrap();
    assert_eq!(session.cursor(), 3);
    let replayed = session.seek_json(3).unwrap();
    assert_eq!(at_three, replayed);
}

#[test]
fn trace_serialization_keeps_absent_option_fields_as_null() {
    let mut session = WasmSession::new(&scenario(false)).unwrap();
    commit_next(&mut session);
    let query_frame: serde_json::Value = serde_json::from_str(&commit_next(&mut session)).unwrap();
    let result_event = query_frame["trace"]
        .as_array()
        .and_then(|trace| trace.last())
        .unwrap();

    assert_eq!(
        result_event,
        &serde_json::json!({
            "catalog_id": 9,
            "kind": "result",
            "node": null,
            "target": null,
            "entry": null,
            "key": "4",
            "patch_start": 4,
            "patch_count": 0
        })
    );
}

#[test]
fn double_rotation_crosses_the_wasm_boundary_as_two_patch_spans() {
    let mut value: serde_json::Value = serde_json::from_str(&scenario(false)).unwrap();
    value["payload"]["initial"]["entries"] = serde_json::json!([
        { "key": "3", "value": "three" },
        { "key": "1", "value": "one" }
    ]);
    value["payload"]["operations"]["items"] = serde_json::json!([
        { "op": "insert", "key": "2", "value": "two" }
    ]);
    let mut session = WasmSession::new(&value.to_string()).unwrap();
    let frame: serde_json::Value = serde_json::from_str(&commit_next(&mut session)).unwrap();
    let trace = frame["trace"].as_array().unwrap();
    let rotations: Vec<_> = trace
        .iter()
        .filter(|event| matches!(event["kind"].as_str(), Some("rotate-left" | "rotate-right")))
        .collect();

    assert_eq!(rotations.len(), 2);
    assert_eq!(rotations[0]["kind"], "rotate-left");
    assert_eq!(rotations[1]["kind"], "rotate-right");
    assert!(rotations[0]["patch_count"].as_u64().unwrap() >= 3);
    assert!(rotations[1]["patch_count"].as_u64().unwrap() >= 3);
    assert_eq!(
        rotations[0]["patch_start"].as_u64().unwrap()
            + rotations[0]["patch_count"].as_u64().unwrap(),
        rotations[1]["patch_start"].as_u64().unwrap()
    );

    let current: serde_json::Value =
        serde_json::from_str(&session.current_frame_json().unwrap()).unwrap();
    let root = &current["structure"]["root"];
    let root_node = current["structure"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == *root)
        .unwrap();
    assert_eq!(root_node["keys"], serde_json::json!(["2"]));
}

#[test]
fn seek_resumes_in_bounded_chunks_and_restores_backward_state() {
    let mut session = WasmSession::new(&scenario(true)).unwrap();
    session.begin_seek(3).unwrap();
    let first: serde_json::Value =
        serde_json::from_str(&session.resume_seek_json(1).unwrap()).unwrap();
    assert_eq!(first["done"], false);
    assert_eq!(first["cursor"], 1);
    assert!(first.get("frame").is_none());

    let final_progress: serde_json::Value =
        serde_json::from_str(&session.resume_seek_json(2).unwrap()).unwrap();
    assert_eq!(final_progress["done"], true);
    assert_eq!(final_progress["frame"]["itemIndex"], 3);
    assert_eq!(session.cursor(), 0);
    session.commit_staged_seek();
    assert_eq!(session.cursor(), 3);

    session.begin_seek(1).unwrap();
    let backward: serde_json::Value =
        serde_json::from_str(&session.resume_seek_json(1).unwrap()).unwrap();
    assert_eq!(backward["done"], true);
    assert_eq!(backward["frame"]["itemIndex"], 1);
    session.commit_staged_seek();
    assert_eq!(session.cursor(), 1);
}

#[test]
fn seek_serialization_failure_preserves_current_boundary() {
    let mut session = WasmSession::new(&scenario(true)).unwrap();
    let before = session.current_frame_json().unwrap();
    session.begin_seek(3).unwrap();

    assert!(session.resume_seek_json_with_limit(3, 1).is_err());
    assert_eq!(session.cursor(), 0);
    assert_eq!(session.current_frame_json().unwrap(), before);
    session.discard_staged_seek();
    assert_eq!(session.cursor(), 0);
    assert_eq!(session.current_frame_json().unwrap(), before);
}

#[test]
fn full_u64_keys_and_metrics_cross_json_as_decimal_strings() {
    let mut value: serde_json::Value = serde_json::from_str(&scenario(false)).unwrap();
    value["payload"]["algorithm"] = serde_json::json!({
        "id": "veb",
        "config": { "word_bits": 64 }
    });
    value["payload"]["initial"]["entries"] = serde_json::json!([{
        "key": u64::MAX.to_string(),
        "value": "maximum"
    }]);
    let session = WasmSession::new(&value.to_string()).unwrap();
    let frame: serde_json::Value =
        serde_json::from_str(&session.current_frame_json().unwrap()).unwrap();
    assert_eq!(
        frame["canonical"]["entries"][0]["key"],
        u64::MAX.to_string()
    );
    assert!(frame["canonical"]["metrics"]["comparisons"].is_string());
}

#[test]
fn persisted_scenario_is_strict_rfc8785_json() {
    let canonical = canonical_scenario_json(&scenario(false)).unwrap();
    assert!(!canonical.contains('\n'));
    assert!(canonical.starts_with("{\"payload\":"));
    assert_eq!(
        canonical,
        WasmSession::new(&scenario(false))
            .unwrap()
            .scenario_json()
            .unwrap()
    );
}

#[test]
fn checkpoint_admission_evicts_before_clone_and_never_exceeds_budget() {
    let mut session = WasmSession::new(&scenario(false)).unwrap();
    session.ordered_map_mut().checkpoints.clear();
    session.ordered_map_mut().checkpoint_bytes = 0;
    let factory_calls = Cell::new(0);

    assert!(
        session
            .ordered_map_mut()
            .store_checkpoint_with_limits(1, 60, 100, 2, |view| {
                factory_calls.set(factory_calls.get() + 1);
                assert!(view.checkpoints.is_empty());
                view.algorithm.clone()
            })
    );
    assert_eq!(session.ordered_map_mut().checkpoint_bytes, 60);
    assert!(
        session
            .ordered_map_mut()
            .store_checkpoint_with_limits(2, 50, 100, 2, |view| {
                factory_calls.set(factory_calls.get() + 1);
                assert!(view.checkpoints.is_empty(), "eviction happens before clone");
                view.algorithm.clone()
            })
    );
    assert_eq!(factory_calls.get(), 2);
    assert_eq!(session.ordered_map_mut().checkpoint_bytes, 50);
    assert!(session.ordered_map_mut().checkpoint_bytes <= 100);

    let before = session.ordered_map_mut().checkpoints.len();
    assert!(
        !session
            .ordered_map_mut()
            .store_checkpoint_with_limits(3, 101, 100, 2, |view| {
                factory_calls.set(factory_calls.get() + 1);
                view.algorithm.clone()
            })
    );
    assert_eq!(factory_calls.get(), 2, "rejection does not clone");
    assert_eq!(session.ordered_map_mut().checkpoints.len(), before);
    assert_eq!(session.ordered_map_mut().checkpoint_bytes, 50);
}

#[test]
fn background_index_is_bounded_and_checkpoint_seek_is_exact() {
    let mut value: serde_json::Value = serde_json::from_str(&scenario(false)).unwrap();
    value["payload"]["operations"]["items"] = serde_json::Value::Array(
        (0..5_000_u64)
            .map(|key| {
                serde_json::json!({
                    "op": "insert",
                    "key": key.to_string(),
                    "value": format!("value-{key}")
                })
            })
            .collect(),
    );
    let source = value.to_string();
    let mut indexed = WasmSession::new(&source).unwrap();
    let mut previous = 0;
    while !indexed.resume_seek_index(127).unwrap() {
        assert!(indexed.seek_coverage() - previous <= 127);
        previous = indexed.seek_coverage();
    }
    assert_eq!(indexed.seek_coverage(), 5_000);
    assert!(
        indexed
            .ordered_map_mut()
            .checkpoints
            .iter()
            .any(|checkpoint| checkpoint.cursor == 4_096)
    );

    indexed.begin_seek(4_096).unwrap();
    assert_eq!(indexed.cursor(), 0);
    let indexed_frame: serde_json::Value =
        serde_json::from_str(&indexed.resume_seek_json(1).unwrap()).unwrap();
    let mut replayed = WasmSession::new(&source).unwrap();
    let replayed_frame: serde_json::Value =
        serde_json::from_str(&replayed.seek_json(4_096).unwrap()).unwrap();
    assert_eq!(indexed_frame["frame"], replayed_frame);
    indexed.commit_staged_seek();
    assert_eq!(indexed.cursor(), 4_096);
}
