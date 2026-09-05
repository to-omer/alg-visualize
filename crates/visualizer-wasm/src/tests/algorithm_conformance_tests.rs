use super::*;
use flow::flow_algorithm_conformance_contracts;
use std::fmt::Write as _;

fn with_algorithm_and_profile(source: &str, algorithm: AlgorithmId, run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("conformance fixture is JSON");
    scenario["payload"]["algorithm"]["id"] = serde_json::json!(algorithm.as_str());
    scenario["payload"]["run_profile"] = serde_json::json!(run_profile);
    scenario.to_string()
}

fn unit_network_scenario(algorithm: AlgorithmId) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&flow_scenario_with_algorithm(algorithm.as_str()))
            .expect("unit-network fixture is JSON");
    scenario["payload"]["graph"]["edges"][0]["capacity"] = serde_json::json!("1");
    scenario.to_string()
}

fn requires_nonbinding_transshipment_capacities(scenario: &serde_json::Value) -> bool {
    scenario["payload"]["algorithm"]["id"]
        .as_str()
        .and_then(|id| id.parse::<AlgorithmId>().ok())
        .and_then(find_algorithm_by_id)
        .is_some_and(|descriptor| {
            descriptor
                .graph_requirements
                .contains(&flow::GraphRequirement::NonbindingTransshipmentCapacities)
        })
}

fn scenario_with_edge_count(source: &str, edge_count: usize) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("boundary fixture is JSON");
    let requires_nonbinding_capacity = requires_nonbinding_transshipment_capacities(&scenario);
    match scenario["payload"]["algorithm"]["id"].as_str() {
        Some("hopcroft-karp") => {
            return bipartite_boundary_scenario(scenario, edge_count);
        }
        Some("hungarian" | "auction") => {
            return assignment_boundary_scenario(scenario, edge_count);
        }
        Some("transportation-simplex" | "modi") => {
            return transportation_boundary_scenario(scenario, edge_count);
        }
        Some("hassin-st-planar" | "borradaile-klein-planar") => {
            return planar_boundary_scenario(scenario, edge_count);
        }
        Some("segment-expanded-convex-mcf" | "convex-cost-scaling" | "convex-network-simplex") => {
            return convex_boundary_scenario(scenario, edge_count);
        }
        _ => {}
    }
    let edges = scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("boundary fixture edges are an array");
    assert!(!edges.is_empty(), "boundary fixture needs one real edge");
    assert!(
        edges.len() <= edge_count,
        "boundary fixture already exceeds requested edge count"
    );
    let prototype = edges[0].clone();
    for ordinal in edges.len()..edge_count {
        let mut edge = prototype.clone();
        let object = edge
            .as_object_mut()
            .expect("boundary prototype edge is an object");
        object.insert(
            "id".to_owned(),
            serde_json::json!(format!("conformance-pad-{ordinal:05}")),
        );
        object.insert("lower".to_owned(), serde_json::json!("0"));
        object.insert(
            "capacity".to_owned(),
            if requires_nonbinding_capacity {
                prototype["capacity"].clone()
            } else {
                serde_json::json!("1")
            },
        );
        object.insert("cost".to_owned(), serde_json::json!("0"));
        object.remove("initial_flow");
        object.remove("convex_cost");
        edges.push(edge);
    }
    scenario.to_string()
}

fn bipartite_boundary_scenario(mut scenario: serde_json::Value, edge_count: usize) -> String {
    const LEFT_COUNT: usize = 58;
    const RIGHT_COUNT: usize = 339;
    assert!((20_000..=20_001).contains(&edge_count));
    let left = (0..LEFT_COUNT)
        .map(|index| format!("l{index:03}"))
        .collect::<Vec<_>>();
    let right = (0..RIGHT_COUNT)
        .map(|index| format!("r{index:03}"))
        .collect::<Vec<_>>();
    let nodes = std::iter::once(serde_json::json!({ "id": "s" }))
        .chain(left.iter().map(|id| serde_json::json!({ "id": id })))
        .chain(right.iter().map(|id| serde_json::json!({ "id": id })))
        .chain(std::iter::once(serde_json::json!({ "id": "t" })))
        .collect::<Vec<_>>();
    let mut edges = Vec::with_capacity(edge_count);
    for (index, node) in left.iter().enumerate() {
        edges.push(serde_json::json!({
            "id": format!("source-{index:03}"), "from": "s", "to": node,
            "capacity": "1", "cost": "0"
        }));
    }
    let compatibility_count = edge_count - LEFT_COUNT - RIGHT_COUNT;
    for (left_index, from) in left.iter().enumerate() {
        for (right_index, to) in right.iter().enumerate() {
            if edges.len() == LEFT_COUNT + compatibility_count {
                break;
            }
            edges.push(serde_json::json!({
                "id": format!("compat-{left_index:03}-{right_index:03}"),
                "from": from, "to": to, "capacity": "1", "cost": "0"
            }));
        }
        if edges.len() == LEFT_COUNT + compatibility_count {
            break;
        }
    }
    for (index, node) in right.iter().enumerate() {
        edges.push(serde_json::json!({
            "id": format!("sink-{index:03}"), "from": node, "to": "t",
            "capacity": "1", "cost": "0"
        }));
    }
    assert_eq!(edges.len(), edge_count);
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "bipartite-matching",
        "left": left,
        "right": right,
        "flow_adapter": { "source": "s", "sink": "t" }
    });
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn assignment_boundary_scenario(mut scenario: serde_json::Value, edge_count: usize) -> String {
    const AGENT_COUNT: usize = 100;
    const TASK_COUNT: usize = 201;
    assert!((20_000..=20_001).contains(&edge_count));
    let agents = (0..AGENT_COUNT)
        .map(|index| format!("a{index:03}"))
        .collect::<Vec<_>>();
    let tasks = (0..TASK_COUNT)
        .map(|index| format!("t{index:03}"))
        .collect::<Vec<_>>();
    let nodes = agents
        .iter()
        .chain(&tasks)
        .map(|id| serde_json::json!({ "id": id }))
        .collect::<Vec<_>>();
    let mut edges = Vec::with_capacity(edge_count);
    for (agent_index, from) in agents.iter().enumerate() {
        for (task_index, to) in tasks.iter().enumerate() {
            if edges.len() == edge_count {
                break;
            }
            edges.push(serde_json::json!({
                "id": format!("assignment-{agent_index:03}-{task_index:03}"),
                "from": from, "to": to, "capacity": "1", "cost": "0"
            }));
        }
        if edges.len() == edge_count {
            break;
        }
    }
    assert_eq!(edges.len(), edge_count);
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "assignment", "agents": agents, "tasks": tasks, "objective": "minimize"
    });
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn transportation_boundary_scenario(mut scenario: serde_json::Value, edge_count: usize) -> String {
    const ORIGIN_COUNT: usize = 32;
    const DESTINATION_COUNT: usize = 65;
    assert!((2_048..=2_049).contains(&edge_count));
    let origins = (0..ORIGIN_COUNT)
        .map(|index| format!("o{index:02}"))
        .collect::<Vec<_>>();
    let destinations = (0..DESTINATION_COUNT)
        .map(|index| format!("d{index:02}"))
        .collect::<Vec<_>>();
    let nodes = origins
        .iter()
        .map(|id| serde_json::json!({ "id": id, "supply": "65" }))
        .chain(
            destinations
                .iter()
                .map(|id| serde_json::json!({ "id": id, "supply": "-32" })),
        )
        .collect::<Vec<_>>();
    let mut edges = Vec::with_capacity(edge_count);
    for (origin_index, from) in origins.iter().enumerate() {
        for (destination_index, to) in destinations.iter().enumerate() {
            if edges.len() == edge_count {
                break;
            }
            edges.push(serde_json::json!({
                "id": format!("route-{origin_index:02}-{destination_index:02}"),
                "from": from, "to": to, "capacity": "64", "cost": "0"
            }));
        }
        if edges.len() == edge_count {
            break;
        }
    }
    assert_eq!(edges.len(), edge_count);
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "transportation", "origins": origins, "destinations": destinations
    });
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn planar_boundary_scenario(mut scenario: serde_json::Value, edge_count: usize) -> String {
    let edge_ids = (0..edge_count)
        .map(|index| format!("parallel-{index:04}"))
        .collect::<Vec<_>>();
    let edges = edge_ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "id": id, "from": "s", "to": "t", "capacity": "1", "cost": "0"
            })
        })
        .collect::<Vec<_>>();
    let source_rotation = edge_ids
        .iter()
        .map(|id| serde_json::json!({ "edge_id": id, "direction": "forward" }))
        .collect::<Vec<_>>();
    let sink_rotation = edge_ids
        .iter()
        .rev()
        .map(|id| serde_json::json!({ "edge_id": id, "direction": "reverse" }))
        .collect::<Vec<_>>();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "planar-max-flow",
        "source": "s",
        "sink": "t",
        "embedding": {
            "rotations": [
                { "node_id": "s", "darts": source_rotation },
                { "node_id": "t", "darts": sink_rotation }
            ],
            "outer_face": { "edge_id": &edge_ids[0], "direction": "forward" },
            "terminal_corners": {
                "source": { "edge_id": &edge_ids[0], "direction": "forward" },
                "sink": { "edge_id": edge_ids.last().expect("planar edge"), "direction": "reverse" }
            }
        }
    });
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [{ "id": "s" }, { "id": "t" }],
        "edges": edges
    });
    scenario.to_string()
}

fn convex_boundary_scenario(mut scenario: serde_json::Value, edge_count: usize) -> String {
    let prototypes = scenario["payload"]["graph"]["edges"]
        .as_array()
        .expect("convex fixture edges")
        .clone();
    assert!(!prototypes.is_empty());
    let edges = (0..edge_count)
        .map(|ordinal| {
            let prototype = &prototypes[ordinal % prototypes.len()];
            serde_json::json!({
                "id": format!("linear-{ordinal:04}"),
                "from": prototype["from"].clone(),
                "to": prototype["to"].clone(),
                "lower": "0",
                "capacity": "1",
                "cost": "0"
            })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["graph"]["edges"] = serde_json::json!(edges);
    scenario.to_string()
}

fn scenario_with_node_count(source: &str, node_count: usize) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("node-boundary fixture is JSON");
    let requires_nonbinding_capacity = requires_nonbinding_transshipment_capacities(&scenario);
    let padding_capacity = if requires_nonbinding_capacity {
        scenario["payload"]["graph"]["edges"][0]["capacity"]
            .as_str()
            .expect("nonbinding fixture capacity")
            .to_owned()
    } else {
        "1".to_owned()
    };
    let requires_strong_connectivity = scenario["payload"]["algorithm"]["id"]
        .as_str()
        .and_then(flow::find_algorithm)
        .is_some_and(|descriptor| {
            descriptor
                .graph_requirements
                .contains(&flow::GraphRequirement::StronglyConnected)
        });
    match scenario["payload"]["algorithm"]["id"].as_str() {
        Some("hopcroft-karp") => return bipartite_node_boundary_scenario(scenario, node_count),
        Some("hungarian" | "auction") => {
            return assignment_node_boundary_scenario(scenario, node_count);
        }
        Some("transportation-simplex" | "modi") => {
            return transportation_node_boundary_scenario(scenario, node_count);
        }
        Some("hassin-st-planar" | "borradaile-klein-planar") => {
            return planar_node_boundary_scenario(scenario, node_count);
        }
        Some("distance-directed-scaling-augmenting-path") => {
            return distance_directed_scaling_node_boundary_scenario(scenario, node_count);
        }
        Some("segment-expanded-convex-mcf" | "convex-cost-scaling" | "convex-network-simplex") => {
            return convex_node_boundary_scenario(scenario, node_count);
        }
        Some("deterministic-almost-linear-mcf") => {
            return strict_interior_node_boundary_scenario(scenario, node_count);
        }
        _ => {}
    }
    let nodes = scenario["payload"]["graph"]["nodes"]
        .as_array_mut()
        .expect("node-boundary nodes are an array");
    assert!(nodes.len() <= node_count);
    let first_node = nodes[0]["id"]
        .as_str()
        .expect("node-boundary anchor")
        .to_owned();
    let original_node_count = nodes.len();
    for ordinal in original_node_count..node_count {
        nodes.push(serde_json::json!({ "id": format!("zz-pad-node-{ordinal:05}") }));
    }
    let edges = scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("node-boundary edges are an array");
    for ordinal in original_node_count..node_count {
        edges.push(serde_json::json!({
            "id": format!("zz-pad-edge-{ordinal:05}"),
            "from": &first_node,
            "to": format!("zz-pad-node-{ordinal:05}"),
            "lower": "0",
            "capacity": &padding_capacity,
            "cost": "0"
        }));
        if requires_strong_connectivity {
            edges.push(serde_json::json!({
                "id": format!("zz-pad-return-{ordinal:05}"),
                "from": format!("zz-pad-node-{ordinal:05}"),
                "to": &first_node,
                "lower": "0",
                "capacity": &padding_capacity,
                "cost": "0"
            }));
        }
    }
    match scenario["payload"]["algorithm"]["id"].as_str() {
        Some("tardos-framework") => {
            let potentials = scenario["payload"]["algorithm"]["config"]["potentials"]
                .as_object_mut()
                .expect("Tardos node-boundary potentials are an object");
            for ordinal in original_node_count..node_count {
                potentials.insert(format!("zz-pad-node-{ordinal:05}"), serde_json::json!("0"));
            }
        }
        Some("prediction-assisted-epsilon-relaxation") => {
            let potentials = scenario["payload"]["algorithm"]["config"]["predicted_potentials"]
                .as_object_mut()
                .expect("prediction node-boundary potentials are an object");
            for ordinal in original_node_count..node_count {
                potentials.insert(format!("zz-pad-node-{ordinal:05}"), serde_json::json!("0"));
            }
        }
        _ => {}
    }
    scenario.to_string()
}

fn distance_directed_scaling_node_boundary_scenario(
    mut scenario: serde_json::Value,
    node_count: usize,
) -> String {
    assert!(
        (2..=flow::DISTANCE_DIRECTED_MAX_NODES + 1).contains(&node_count),
        "node-boundary fixture only constructs the admitted DD2 band and its one-past rejection"
    );
    let internal_nodes = (0..node_count.saturating_sub(2))
        .map(|index| format!("v{index:03}"))
        .collect::<Vec<_>>();
    let node_ids = std::iter::once("s".to_owned())
        .chain(internal_nodes)
        .chain(std::iter::once("t".to_owned()))
        .collect::<Vec<_>>();
    let capacity = node_count.next_power_of_two().to_string();
    let nodes = node_ids
        .iter()
        .map(|id| serde_json::json!({ "id": id }))
        .collect::<Vec<_>>();
    let edges = node_ids
        .windows(2)
        .enumerate()
        .map(|(index, endpoints)| {
            serde_json::json!({
                "id": format!("chain-{index:03}"),
                "from": &endpoints[0],
                "to": &endpoints[1],
                "lower": "0",
                "capacity": &capacity,
                "cost": "0"
            })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn strict_interior_node_boundary_scenario(
    mut scenario: serde_json::Value,
    node_count: usize,
) -> String {
    assert!(node_count >= 2);
    let node_ids = (0..node_count)
        .map(|ordinal| format!("strict-{ordinal:03}"))
        .collect::<Vec<_>>();
    let nodes = node_ids
        .iter()
        .enumerate()
        .map(|(ordinal, id)| {
            serde_json::json!({
                "id": id,
                "supply": if ordinal == 0 {
                    "1"
                } else if ordinal + 1 == node_count {
                    "-1"
                } else {
                    "0"
                }
            })
        })
        .collect::<Vec<_>>();
    let edges = node_ids
        .windows(2)
        .enumerate()
        .map(|(ordinal, endpoints)| {
            serde_json::json!({
                "id": format!("strict-edge-{ordinal:03}"),
                "from": &endpoints[0],
                "to": &endpoints[1],
                "lower": "0",
                "capacity": "2",
                "cost": (ordinal + 1).to_string()
            })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn bipartite_node_boundary_scenario(mut scenario: serde_json::Value, node_count: usize) -> String {
    assert!((2..=2_001).contains(&node_count));
    let internal_count = node_count.saturating_sub(2);
    let left_count = internal_count.div_ceil(2);
    let right_count = internal_count / 2;
    let left = (0..left_count)
        .map(|index| format!("l{index:04}"))
        .collect::<Vec<_>>();
    let right = (0..right_count)
        .map(|index| format!("r{index:04}"))
        .collect::<Vec<_>>();
    let nodes = std::iter::once(serde_json::json!({ "id": "s" }))
        .chain(left.iter().map(|id| serde_json::json!({ "id": id })))
        .chain(right.iter().map(|id| serde_json::json!({ "id": id })))
        .chain(std::iter::once(serde_json::json!({ "id": "t" })))
        .collect::<Vec<_>>();
    let mut edges = Vec::with_capacity(left_count + right_count + left_count.min(right_count));
    for (index, left_node) in left.iter().enumerate() {
        edges.push(serde_json::json!({
            "id": format!("source-{index:04}"), "from": "s", "to": left_node,
            "capacity": "1", "cost": "0"
        }));
    }
    for (index, left_node) in left.iter().enumerate().take(right_count) {
        let right_node = &right[index];
        edges.push(serde_json::json!({
            "id": format!("compat-{index:04}"), "from": left_node, "to": right_node,
            "capacity": "1", "cost": "0"
        }));
    }
    for (index, right_node) in right.iter().enumerate() {
        edges.push(serde_json::json!({
            "id": format!("sink-{index:04}"), "from": right_node, "to": "t",
            "capacity": "1", "cost": "0"
        }));
    }
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "bipartite-matching", "left": left, "right": right,
        "flow_adapter": { "source": "s", "sink": "t" }
    });
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn assignment_node_boundary_scenario(mut scenario: serde_json::Value, node_count: usize) -> String {
    let agent_count = node_count / 2;
    let task_count = node_count - agent_count;
    let agents = (0..agent_count)
        .map(|index| format!("a{index:04}"))
        .collect::<Vec<_>>();
    let tasks = (0..task_count)
        .map(|index| format!("t{index:04}"))
        .collect::<Vec<_>>();
    let nodes = agents
        .iter()
        .chain(&tasks)
        .map(|id| serde_json::json!({ "id": id }))
        .collect::<Vec<_>>();
    let edges = agents
        .iter()
        .enumerate()
        .map(|(agent, from)| {
            let task = agent;
            let to = &tasks[task];
            serde_json::json!({
                "id": format!("assignment-{agent:04}-{task:04}"),
                "from": from,
                "to": to,
                "capacity": "1",
                "cost": (1 + (agent * 7 + task * 3) % 17).to_string()
            })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "assignment", "agents": agents, "tasks": tasks, "objective": "minimize"
    });
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn transportation_node_boundary_scenario(
    mut scenario: serde_json::Value,
    node_count: usize,
) -> String {
    let origin_count = node_count / 2;
    let destination_count = node_count - origin_count;
    let origins = (0..origin_count)
        .map(|index| format!("o{index:03}"))
        .collect::<Vec<_>>();
    let destinations = (0..destination_count)
        .map(|index| format!("d{index:03}"))
        .collect::<Vec<_>>();
    let nodes = origins
        .iter()
        .map(|id| serde_json::json!({ "id": id, "supply": destination_count.to_string() }))
        .chain(
            destinations
                .iter()
                .map(|id| serde_json::json!({ "id": id, "supply": format!("-{origin_count}") })),
        )
        .collect::<Vec<_>>();
    // Missing origin/destination pairs are forbidden routes in the native
    // transportation declaration. An empty route set therefore remains a
    // semantically valid (though infeasible) model while isolating this test
    // from the independent edge-admission boundary.
    let edges = Vec::<serde_json::Value>::new();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "transportation", "origins": origins, "destinations": destinations
    });
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn planar_node_boundary_scenario(mut scenario: serde_json::Value, node_count: usize) -> String {
    let node_ids = (0..node_count)
        .map(|index| format!("n{index:03}"))
        .collect::<Vec<_>>();
    let edge_ids = (0..node_count.saturating_sub(1))
        .map(|index| format!("path-{index:03}"))
        .collect::<Vec<_>>();
    let nodes = node_ids
        .iter()
        .map(|id| serde_json::json!({ "id": id }))
        .collect::<Vec<_>>();
    let edges = edge_ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            serde_json::json!({
                "id": id, "from": &node_ids[index], "to": &node_ids[index + 1],
                "capacity": "1", "cost": "0"
            })
        })
        .collect::<Vec<_>>();
    let rotations = node_ids
        .iter()
        .enumerate()
        .map(|(index, node_id)| {
            let mut darts = Vec::with_capacity(2);
            if index > 0 {
                darts.push(serde_json::json!({
                    "edge_id": &edge_ids[index - 1], "direction": "reverse"
                }));
            }
            if index < edge_ids.len() {
                darts.push(serde_json::json!({
                    "edge_id": &edge_ids[index], "direction": "forward"
                }));
            }
            serde_json::json!({ "node_id": node_id, "darts": darts })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "planar-max-flow", "source": &node_ids[0],
        "sink": node_ids.last().expect("planar sink"),
        "embedding": {
            "rotations": rotations,
            "outer_face": { "edge_id": &edge_ids[0], "direction": "forward" },
            "terminal_corners": {
                "source": { "edge_id": &edge_ids[0], "direction": "forward" },
                "sink": { "edge_id": edge_ids.last().expect("planar edge"), "direction": "reverse" }
            }
        }
    });
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn convex_node_boundary_scenario(mut scenario: serde_json::Value, node_count: usize) -> String {
    let node_ids = (0..node_count)
        .map(|index| format!("n{index:03}"))
        .collect::<Vec<_>>();
    let nodes = node_ids
        .iter()
        .map(|id| serde_json::json!({ "id": id }))
        .collect::<Vec<_>>();
    let edges = (0..node_count)
        .map(|index| {
            serde_json::json!({
                "id": format!("cycle-{index:03}"),
                "from": &node_ids[index],
                "to": &node_ids[(index + 1) % node_count],
                "capacity": "1", "cost": "0"
            })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn hochbaum_pseudoflow_split_scenario(run_profile: &str) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&push_relabel_scenario("hochbaum-pseudoflow"))
            .expect("pseudoflow split representative is JSON");
    scenario["payload"]["run_profile"] = serde_json::json!(run_profile);
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [
            { "id": "s" }, { "id": "a" }, { "id": "b" },
            { "id": "c" }, { "id": "t" }
        ],
        "edges": [
            { "id": "sa", "from": "s", "to": "a", "capacity": "9" },
            { "id": "sb", "from": "s", "to": "b", "capacity": "4" },
            { "id": "ac", "from": "a", "to": "c", "capacity": "3" },
            { "id": "bc", "from": "b", "to": "c", "capacity": "7" },
            { "id": "ct", "from": "c", "to": "t", "capacity": "8" }
        ]
    });
    scenario.to_string()
}

/// One positive, source-meaningful runtime fixture per closed algorithm ID.
///
/// The exhaustive match is intentional: adding an ID cannot silently inherit
/// another algorithm's test case or escape the cross-layer conformance sweep.
#[allow(clippy::too_many_lines)]
fn conformance_scenario(algorithm: AlgorithmId, run_profile: &str) -> String {
    let source = match algorithm {
        AlgorithmId::FordFulkerson
        | AlgorithmId::DfsFordFulkerson
        | AlgorithmId::EdmondsKarp
        | AlgorithmId::ShortestAugmentingPath
        | AlgorithmId::Isap
        | AlgorithmId::WidestAugmentingPath
        | AlgorithmId::CapacityScalingAugmentingPath
        | AlgorithmId::DistanceDirectedAugmentingPath
        | AlgorithmId::DistanceDirectedScalingAugmentingPath
        | AlgorithmId::Dinic
        | AlgorithmId::KarzanovPreflow
        | AlgorithmId::Mpm
        | AlgorithmId::DynamicTreeBlockingFlow
        | AlgorithmId::BinaryBlockingFlow
        | AlgorithmId::GoldbergRao
        | AlgorithmId::GenericPushRelabel
        | AlgorithmId::FifoPushRelabel
        | AlgorithmId::RelabelToFront
        | AlgorithmId::HighestLabelPushRelabel
        | AlgorithmId::ExcessScalingPushRelabel
        | AlgorithmId::DynamicTreePushRelabel
        | AlgorithmId::PartialAugmentRelabelMaxFlow
        | AlgorithmId::SynchronousParallelPushRelabel
        | AlgorithmId::CurrentArcHeuristic
        | AlgorithmId::GlobalRelabelHeuristic
        | AlgorithmId::GapRelabelHeuristic
        | AlgorithmId::PseudoflowSimplex
        | AlgorithmId::BoykovKolmogorov => push_relabel_scenario(algorithm.as_str()),
        AlgorithmId::HochbaumPseudoflow => hochbaum_pseudoflow_split_scenario(run_profile),
        AlgorithmId::UnitCapacityDinic | AlgorithmId::UnitNetworkDinic => {
            unit_network_scenario(algorithm)
        }
        AlgorithmId::ParametricPseudoflow | AlgorithmId::ParametricBreakpointRerun => {
            parametric_scenario(algorithm.as_str(), run_profile)
        }
        AlgorithmId::Ibfs => ibfs_scenario(run_profile),
        AlgorithmId::Eibfs => eibfs_scenario(run_profile),
        AlgorithmId::HopcroftKarp => hopcroft_karp_scenario(run_profile),
        AlgorithmId::HassinStPlanar | AlgorithmId::BorradaileKleinPlanar => {
            planar_scenario(algorithm.as_str(), run_profile)
        }
        AlgorithmId::ElectricalFlow => electrical_flow_scenario(run_profile),
        AlgorithmId::AugmentingElectricalFlow => augmenting_electrical_scenario(run_profile),
        AlgorithmId::InteriorPointMaxFlow => interior_point_max_flow_scenario(run_profile),
        AlgorithmId::MinimumRatioCycleMaxFlow => minimum_ratio_cycle_scenario(run_profile),
        AlgorithmId::OrlinMaxFlow => orlin_max_flow_scenario(run_profile),
        AlgorithmId::RandomizedAlmostLinearMaxFlow => {
            randomized_almost_linear_scenario(run_profile)
        }
        AlgorithmId::DeterministicAlmostLinearMaxFlow => {
            deterministic_almost_linear_scenario(run_profile)
        }
        AlgorithmId::WeightedAugmentingPaths => weighted_augmenting_paths_scenario(run_profile),
        AlgorithmId::WeightedPushRelabel => weighted_push_relabel_shortcut_scenario(run_profile),
        AlgorithmId::DynamicEibfs => dynamic_eibfs_scenario(run_profile),
        AlgorithmId::WarmStartPushRelabel => warm_start_push_relabel_scenario(run_profile),
        AlgorithmId::SimpleCycleCanceling => simple_cycle_canceling_scenario(),
        AlgorithmId::MinimumMeanCycleCanceling => minimum_mean_cycle_canceling_scenario(),
        AlgorithmId::CancelAndTighten => cancel_tighten_scenario(),
        AlgorithmId::RelaxedMostNegativeCycle => relaxed_mndc_scenario(run_profile),
        AlgorithmId::SuccessiveShortestPath
        | AlgorithmId::BellmanFordSsp
        | AlgorithmId::PotentialDijkstraSsp => potential_dijkstra_scenario(),
        AlgorithmId::SuccessiveShortestAugmentingPath => {
            successive_shortest_augmenting_path_scenario()
        }
        AlgorithmId::PrimalDualMcf => primal_dual_scenario(),
        AlgorithmId::BlockingFlowPrimalDual => blocking_primal_dual_scenario(),
        AlgorithmId::CapacityScalingMcf => capacity_scaling_scenario(),
        AlgorithmId::EnhancedCapacityScaling => enhanced_capacity_scaling_scenario(run_profile),
        AlgorithmId::CostScaling
        | AlgorithmId::CostScalingPushRelabel
        | AlgorithmId::AugmentRelabel
        | AlgorithmId::PartialAugmentRelabelMcf
        | AlgorithmId::PriceRefinement
        | AlgorithmId::GeneralizedCostScaling
        | AlgorithmId::PrimalNetworkSimplex
        | AlgorithmId::DynamicTreeNetworkSimplex
        | AlgorithmId::OutOfKilter => cost_scaling_scenario(algorithm.as_str()),
        AlgorithmId::ArcFixing => arc_fixing_scenario(run_profile),
        AlgorithmId::ExcessScalingMcf => excess_scaling_scenario(run_profile),
        AlgorithmId::DoubleScaling => double_scaling_scenario(),
        AlgorithmId::DualNetworkSimplex => dual_network_simplex_scenario(run_profile),
        AlgorithmId::PolynomialPrimalNetworkSimplex => {
            polynomial_primal_simplex_scenario(run_profile)
        }
        AlgorithmId::PolynomialDualNetworkSimplex => polynomial_dual_simplex_scenario(run_profile),
        AlgorithmId::TransportationSimplex | AlgorithmId::Modi => {
            transportation_scenario(algorithm.as_str(), run_profile, false)
        }
        AlgorithmId::Relaxation => relaxation_scenario(),
        AlgorithmId::EpsilonRelaxation => epsilon_relaxation_scenario(run_profile),
        AlgorithmId::Hungarian => hungarian_scenario(run_profile, false),
        AlgorithmId::Auction => auction_scenario(run_profile, false),
        AlgorithmId::TardosFramework => tardos_framework_scenario(run_profile),
        AlgorithmId::OrlinMcf => orlin_mcf_scenario(run_profile),
        AlgorithmId::PrimalDualInteriorPointMcf => primal_dual_ipm_mcf_scenario(run_profile),
        AlgorithmId::ElectricalFlowInteriorPointMcf => electrical_ipm_mcf_scenario(run_profile),
        AlgorithmId::MinimumRatioCycleMcf => minimum_ratio_cycle_mcf_scenario(run_profile),
        AlgorithmId::RandomizedAlmostLinearMcf => {
            randomized_almost_linear_mcf_scenario(run_profile)
        }
        AlgorithmId::DeterministicAlmostLinearMcf => {
            deterministic_almost_linear_mcf_scenario(run_profile)
        }
        AlgorithmId::SegmentExpandedConvexMcf => convex_cost_scenario(),
        AlgorithmId::ConvexCostScaling => convex_cost_scaling_scenario(run_profile),
        AlgorithmId::ConvexNetworkSimplex => convex_network_simplex_scenario(run_profile),
        AlgorithmId::PredictionAssistedEpsilonRelaxation => {
            prediction_assisted_epsilon_scenario(run_profile)
        }
    };
    with_algorithm_and_profile(&source, algorithm, run_profile)
}

fn arithmetic_boundary_scenario(algorithm: AlgorithmId) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&conformance_scenario(algorithm, "fast"))
            .expect("arithmetic fixture is JSON");
    let descriptor = flow::find_algorithm_by_id(algorithm).expect("catalog descriptor");
    // Exhaustive/research kernels publish narrower source-specific numeric
    // limits in their focused tests. Keep their canonical witness intact here;
    // this cross-ID sweep stresses the aggregate-safe wide envelope of the
    // general kernels without manufacturing an invalid source instance.
    let preserve_bounded_fixture_numbers = flow::numeric_safety_contract_kind(descriptor)
        != flow::NumericSafetyContractKind::AggregateSafeWideArithmetic;
    let fixed_capacity = preserve_bounded_fixture_numbers
        || descriptor.graph_requirements.iter().any(|requirement| {
            matches!(
                requirement,
                flow::GraphRequirement::UnitCapacity | flow::GraphRequirement::UnitNetwork
            )
        })
        || matches!(
            descriptor.runtime_route,
            flow::RuntimeRouteKind::BipartiteMatching
                | flow::RuntimeRouteKind::Assignment
                | flow::RuntimeRouteKind::ConvexCostFlow
        );
    let fixed_cost = preserve_bounded_fixture_numbers
        || descriptor
            .graph_requirements
            .contains(&flow::GraphRequirement::ZeroCost)
        || matches!(
            descriptor.runtime_route,
            flow::RuntimeRouteKind::BipartiteMatching
                | flow::RuntimeRouteKind::ParametricMaxFlow
                | flow::RuntimeRouteKind::ConvexCostFlow
        );
    let edges = scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("arithmetic edges");
    let edge_count = u64::try_from(edges.len())
        .expect("fixture edge count fits u64")
        .max(1);
    let safe_capacity = u64::MAX / edge_count;
    let safe_cost = i64::MAX / i64::try_from(edge_count).expect("fixture edge count fits i64");
    for edge in edges {
        if !fixed_capacity {
            edge["capacity"] = serde_json::json!(safe_capacity.to_string());
        }
        if !fixed_cost {
            let negative = edge["cost"]
                .as_str()
                .is_some_and(|cost| cost.starts_with('-'));
            let cost = if negative { -safe_cost } else { safe_cost };
            edge["cost"] = serde_json::json!(cost.to_string());
            if let Some(convex) = edge.get_mut("convex_cost") {
                convex["base_cost_at_zero"] = serde_json::json!(safe_cost.to_string());
                for segment in convex["segments"].as_array_mut().expect("convex segments") {
                    segment["marginal_cost"] = serde_json::json!(safe_cost.to_string());
                }
            }
        }
    }
    scenario.to_string()
}

fn arithmetic_overflow_scenario(algorithm: AlgorithmId) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(&conformance_scenario(algorithm, "fast"))
            .expect("overflow fixture is JSON");
    let descriptor = flow::find_algorithm_by_id(algorithm).expect("catalog descriptor");
    let unit_domain = descriptor.graph_requirements.iter().any(|requirement| {
        matches!(
            requirement,
            flow::GraphRequirement::UnitCapacity | flow::GraphRequirement::UnitNetwork
        )
    });
    let cost_route = matches!(
        descriptor.runtime_route,
        flow::RuntimeRouteKind::MinCostFlow
            | flow::RuntimeRouteKind::MinCostMaxFlow
            | flow::RuntimeRouteKind::Transportation
            | flow::RuntimeRouteKind::ConvexCostFlow
    );
    let terminal_flow_route = matches!(
        descriptor.runtime_route,
        flow::RuntimeRouteKind::MaxFlow
            | flow::RuntimeRouteKind::MinCostMaxFlow
            | flow::RuntimeRouteKind::PlanarMaxFlow
    );
    for edge in scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("overflow edges")
    {
        if (cost_route || terminal_flow_route) && !unit_domain && edge.get("convex_cost").is_none()
        {
            edge["capacity"] = serde_json::json!(u64::MAX.to_string());
        }
        if cost_route {
            if let Some(convex) = edge.get_mut("convex_cost") {
                convex["base_cost_at_zero"] = serde_json::json!(i128::MAX.to_string());
                for segment in convex["segments"]
                    .as_array_mut()
                    .expect("overflow convex segments")
                {
                    segment["marginal_cost"] = serde_json::json!(i64::MAX.to_string());
                }
            } else {
                edge["cost"] = serde_json::json!(i64::MAX.to_string());
            }
        }
    }
    scenario.to_string()
}

#[expect(
    clippy::too_many_lines,
    reason = "the shared conformance runner retains one lifecycle from admission through atomic replay verification"
)]
fn run_to_end(
    algorithm: AlgorithmId,
    run_profile: &str,
    verify_staging_atomicity: bool,
) -> (String, String, usize, String, Vec<String>) {
    let source = conformance_scenario(algorithm, run_profile);
    let mut session = FlowSession::new(&source).unwrap_or_else(|error| {
        panic!(
            "{} {run_profile} conformance fixture failed admission: {error:?}",
            algorithm.as_str()
        )
    });
    assert!(
        matches!(
            (run_profile, session.scenario.payload.run_profile),
            ("trace", RunProfileV1::Trace) | ("fast", RunProfileV1::Fast)
        ),
        "{} conformance fixture changed its requested run profile",
        algorithm.as_str()
    );
    let first_staged_event = stage_first_conformance_event(
        &mut session,
        algorithm,
        run_profile,
        verify_staging_atomicity,
    );
    let replay_base = session
        .frame_json_at(0)
        .expect("prepared base conformance frame serializes");
    let mut public_timeline = vec![
        replay_base.clone(),
        session
            .current_frame_json()
            .expect("first committed conformance frame serializes"),
    ];
    let mut event_count = 1_usize;
    loop {
        let next = session.stage_next_json().unwrap_or_else(|error| {
            panic!(
                "{} {run_profile} failed at event {event_count}: {error:?}",
                algorithm.as_str()
            )
        });
        if next.is_none() {
            break;
        }
        session.commit_staged_next();
        event_count += 1;
        public_timeline.push(
            session
                .current_frame_json()
                .expect("committed conformance frame serializes"),
        );
        assert!(
            event_count <= 100_000,
            "{} exceeded the conformance event ceiling",
            algorithm.as_str()
        );
    }
    let final_frame = session
        .current_frame_json()
        .expect("final conformance frame serializes");
    let replay_base_value: serde_json::Value =
        serde_json::from_str(&replay_base).expect("prepared base conformance frame is JSON");
    assert_eq!(
        replay_base_value["event_id"],
        "0",
        "{} base frame does not identify cursor zero",
        algorithm.as_str()
    );
    assert_eq!(
        replay_base_value["event_count"],
        event_count.to_string(),
        "{} base frame does not retain the prepared event extent",
        algorithm.as_str()
    );
    if verify_staging_atomicity {
        assert_replay_publication_atomicity(
            &mut session,
            algorithm,
            event_count,
            &replay_base,
            &final_frame,
        );
    }
    let first_source_event = if run_profile == "trace" {
        assert!(
            first_staged_event.starts_with(algorithm.as_str())
                || first_staged_event.starts_with("feasibility."),
            "{} first staged trace event belongs to neither its source contract nor shared feasibility: {first_staged_event:?}",
            algorithm.as_str()
        );
        public_timeline
            .iter()
            .map(|frame| {
                serde_json::from_str::<serde_json::Value>(frame)
                    .expect("source-event discovery frame is JSON")
            })
            .filter_map(|frame| {
                frame["trace_event"]["catalog_id"]
                    .as_str()
                    .map(str::to_owned)
            })
            .find(|catalog_id| !catalog_id.starts_with("feasibility."))
            .unwrap_or_else(|| panic!("{} has no source trace event", algorithm.as_str()))
    } else {
        first_staged_event
    };
    (
        replay_base,
        final_frame,
        event_count,
        first_source_event,
        public_timeline,
    )
}

fn stage_first_conformance_event(
    session: &mut FlowSession,
    algorithm: AlgorithmId,
    run_profile: &str,
    verify_staging_atomicity: bool,
) -> String {
    let base = session
        .current_frame_json()
        .expect("base conformance frame serializes");
    let staged = session
        .stage_next_json()
        .unwrap_or_else(|error| {
            panic!(
                "{} {run_profile} failed initial execution: {error:?}",
                algorithm.as_str()
            )
        })
        .unwrap_or_else(|| panic!("{} produced no event", algorithm.as_str()));
    let first: serde_json::Value =
        serde_json::from_str(&staged).expect("first conformance frame is JSON");
    let first_event = first["trace_event"]["catalog_id"].as_str().map_or_else(
        || {
            format!(
                "<metadata-free:{}:{:?}>",
                first["solve_status"].as_str().unwrap_or("unknown"),
                first["event_count"]
            )
        },
        str::to_owned,
    );
    if verify_staging_atomicity {
        assert_eq!(
            session
                .current_frame_json()
                .expect("unstaged base frame serializes"),
            base,
            "{} changed committed state before ACK",
            algorithm.as_str()
        );
        session.discard_staged_next();
        assert_eq!(
            session
                .current_frame_json()
                .expect("discarded base frame serializes"),
            base,
            "{} changed committed state after discard",
            algorithm.as_str()
        );
        session
            .stage_next_json()
            .unwrap_or_else(|error| {
                panic!(
                    "{} failed deterministic restaging: {error:?}",
                    algorithm.as_str()
                )
            })
            .unwrap_or_else(|| panic!("{} lost its first event", algorithm.as_str()));
    }
    session.commit_staged_next();
    first_event
}

fn assert_replay_publication_atomicity(
    session: &mut FlowSession,
    algorithm: AlgorithmId,
    event_count: usize,
    replay_base: &str,
    final_frame: &str,
) {
    session
        .begin_seek(0)
        .expect("prepared trace accepts a base seek");
    assert!(
        publish_staged_flow_seek_json(session, 1).is_err(),
        "{} tiny publication budget must reject",
        algorithm.as_str()
    );
    assert_eq!(
        session
            .current_frame_json()
            .expect("failed publication retains the final frame"),
        final_frame,
        "{} changed committed state after publication failure",
        algorithm.as_str()
    );
    session
        .begin_seek(0)
        .expect("failed publication leaves seek staging reusable");
    session
        .resume_seek_json(1)
        .expect("base replay candidate serializes");
    session.commit_staged_seek();
    assert_eq!(
        session
            .current_frame_json()
            .expect("replayed base frame serializes"),
        replay_base,
        "{} base replay is not byte exact",
        algorithm.as_str()
    );
    session
        .begin_seek(event_count)
        .expect("prepared trace accepts a final seek");
    session
        .resume_seek_json(1)
        .expect("final replay candidate serializes");
    session.commit_staged_seek();
    assert_eq!(
        session
            .current_frame_json()
            .expect("replayed final frame serializes"),
        final_frame,
        "{} final replay is not byte exact",
        algorithm.as_str()
    );
}

fn normalized_final_frame(source: &str) -> serde_json::Value {
    let frame: serde_json::Value = serde_json::from_str(source).expect("conformance frame is JSON");
    let residual_arcs = frame["residual_arcs"]
        .as_array()
        .expect("residual arcs are an array")
        .iter()
        .cloned()
        .map(|mut arc| {
            arc.as_object_mut()
                .expect("residual arc is an object")
                .remove("active");
            arc
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "result_schema_version": frame["result_schema_version"].clone(),
        "frame_revision": frame["frame_revision"].clone(),
        "solve_status": frame["solve_status"].clone(),
        "model": frame["model"].clone(),
        "graph": frame["graph"].clone(),
        "algorithm": frame["algorithm"].clone(),
        "edge_states": frame["edge_states"].clone(),
        "residual_arcs": residual_arcs,
        "outcome": frame["outcome"].clone(),
        "metrics": frame["metrics"].clone()
    })
}

fn aggregate_trace_feasibility_work(
    algorithm: AlgorithmId,
    trace_timeline: &[String],
) -> Option<serde_json::Value> {
    const METRIC_NAMES: [&str; 9] = [
        "original_edge_inspections",
        "original_node_inspections",
        "auxiliary_adjacency_inspections",
        "pushes",
        "relabels",
        "active_node_selections",
        "discharges",
        "cut_adjacency_inspections",
        "extracted_original_edges",
    ];
    let mut invocations = 0_u128;
    let mut totals = [0_u128; METRIC_NAMES.len()];
    for source in trace_timeline {
        let frame: serde_json::Value =
            serde_json::from_str(source).expect("trace feasibility frame is JSON");
        assert!(
            frame.get("feasibility_work").is_none(),
            "{algorithm} Trace frame retained a Fast-only feasibility summary"
        );
        if !matches!(
            frame["trace_event"]["catalog_id"].as_str(),
            Some("feasibility.feasible" | "feasibility.infeasible")
        ) {
            continue;
        }
        invocations += 1;
        let metrics = frame
            .get("feasibility_overlay")
            .and_then(|overlay| overlay.get("metrics"))
            .unwrap_or_else(|| {
                panic!("{algorithm} terminal feasibility event omitted its source metrics")
            });
        for (index, name) in METRIC_NAMES.iter().enumerate() {
            totals[index] += metrics[*name]
                .as_str()
                .unwrap_or_else(|| panic!("{algorithm} feasibility metric {name} is absent"))
                .parse::<u128>()
                .unwrap_or_else(|_| {
                    panic!("{algorithm} feasibility metric {name} is noncanonical")
                });
        }
    }
    (invocations != 0).then(|| {
        serde_json::json!({
            "invocations": invocations.to_string(),
            "metrics": METRIC_NAMES
                .iter()
                .zip(totals)
                .map(|(name, total)| ((*name).to_owned(), serde_json::Value::String(total.to_string())))
                .collect::<serde_json::Map<_, _>>()
        })
    })
}

fn differing_top_level_fields(left: &serde_json::Value, right: &serde_json::Value) -> Vec<String> {
    left.as_object()
        .expect("normalized frame is an object")
        .keys()
        .filter(|key| left.get(*key) != right.get(*key))
        .cloned()
        .collect()
}

fn certified_flow_vector(graph: &flow::FlowNetwork, frame: &serde_json::Value) -> Vec<u64> {
    let by_id = frame["edge_states"]
        .as_array()
        .expect("certified edge states")
        .iter()
        .map(|state| {
            (
                state["edge_id"].as_str().expect("certified edge ID"),
                state["flow"]
                    .as_str()
                    .expect("certified flow")
                    .parse::<u64>()
                    .expect("canonical certified flow"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    graph
        .edges()
        .iter()
        .map(|edge| {
            *by_id
                .get(edge.id().as_str())
                .unwrap_or_else(|| panic!("missing certified edge {}", edge.id().as_str()))
        })
        .collect()
}

fn definitely_invalid_flow_vector(graph: &flow::FlowNetwork, flows: &[u64]) -> Vec<u64> {
    let mut corrupted = flows.to_vec();
    let (index, edge) = graph
        .edges()
        .iter()
        .enumerate()
        .next()
        .expect("positive conformance graph has an edge");
    corrupted[index] = if edge.capacity() < u64::MAX {
        edge.capacity() + 1
    } else if edge.lower() > 0 {
        edge.lower() - 1
    } else if flows[index] == 0 {
        u64::MAX
    } else {
        0
    };
    corrupted
}

fn parse_outcome_i128(frame: &serde_json::Value, field: &str) -> i128 {
    frame["outcome"][field]
        .as_str()
        .unwrap_or_else(|| panic!("missing outcome field {field}"))
        .parse()
        .unwrap_or_else(|_| panic!("noncanonical outcome field {field}"))
}

#[allow(clippy::too_many_lines)]
fn verify_final_with_independent_checker(
    algorithm: AlgorithmId,
    scenario_source: &str,
    frame_source: &str,
    public_trace_timeline: &[String],
) {
    let scenario = decode_flow_scenario(scenario_source.as_bytes()).expect("checker Scenario");
    let graph = scenario.canonical_network().expect("checker graph");
    let frame: serde_json::Value = serde_json::from_str(frame_source).expect("checker final frame");
    let checker = flow::checker_contract_kind(algorithm);
    if checker == flow::CheckerContractKind::SourceDefinedInvariant {
        assert!(frame["outcome"].is_object());
        assert!(matches!(
            frame["solve_status"].as_str(),
            Some("optimal" | "primitive-complete")
        ));
        verify_source_defined_checker(algorithm, &scenario, &graph, &frame, public_trace_timeline);
        return;
    }
    if checker == flow::CheckerContractKind::ProjectOracleDemonstratorInvariant {
        assert_eq!(frame["solve_status"], "optimal");
        verify_project_oracle_checker(algorithm, &scenario, &graph, &frame, public_trace_timeline);
        return;
    }
    let flows = certified_flow_vector(&graph, &frame);
    let corrupted = definitely_invalid_flow_vector(&graph, &flows);
    match checker {
        flow::CheckerContractKind::IndependentMaxFlowCertificate => {
            let (source, sink) = match &scenario.payload.model {
                FlowProblemModelV1::MaxFlow { source, sink }
                | FlowProblemModelV1::PlanarMaxFlow { source, sink, .. } => {
                    terminal_indices(&graph, source, sink).expect("checker terminals")
                }
                model => panic!("unexpected max-flow checker model {model:?}"),
            };
            let certificate =
                flow::check_max_flow(&graph, source, sink, &flows).expect("max-flow checker");
            assert_eq!(certificate.value, parse_outcome_i128(&frame, "value"));
            assert_eq!(
                certificate.cut_bound,
                parse_outcome_i128(&frame, "cut_bound")
            );
            assert!(flow::check_max_flow(&graph, source, sink, &corrupted).is_err());
        }
        flow::CheckerContractKind::IndependentMinCostFlowCertificate => {
            let target = match &scenario.payload.model {
                FlowProblemModelV1::FixedFlowMinCost {
                    source,
                    sink,
                    required_flow,
                } => {
                    let (source, sink) =
                        terminal_indices(&graph, source, sink).expect("checker terminals");
                    flow::fixed_flow_divergences(
                        &graph,
                        source,
                        sink,
                        required_flow.parse().expect("checker required flow"),
                    )
                    .expect("checker fixed divergence")
                }
                FlowProblemModelV1::Circulation {}
                | FlowProblemModelV1::Transshipment {}
                | FlowProblemModelV1::Transportation { .. } => {
                    flow::supply_divergences(&graph).expect("checker supply divergence")
                }
                model => panic!("unexpected min-cost checker model {model:?}"),
            };
            let certificate =
                flow::check_min_cost_flow(&graph, &target, &flows).expect("minimum-cost checker");
            assert_eq!(
                certificate.total_cost,
                parse_outcome_i128(&frame, "total_cost")
            );
            assert!(flow::check_min_cost_flow(&graph, &target, &corrupted).is_err());
        }
        flow::CheckerContractKind::IndependentMinCostMaxFlowCertificate => {
            let FlowProblemModelV1::MinCostMaxFlow { source, sink } = &scenario.payload.model
            else {
                panic!("unexpected min-cost max-flow checker model");
            };
            let (source, sink) = terminal_indices(&graph, source, sink).expect("checker terminals");
            let certificate = flow::check_min_cost_max_flow(&graph, source, sink, &flows)
                .expect("minimum-cost maximum-flow checker");
            assert_eq!(
                certificate.max_flow.value,
                parse_outcome_i128(&frame, "value")
            );
            assert_eq!(
                certificate.min_cost.total_cost,
                parse_outcome_i128(&frame, "total_cost")
            );
            assert!(flow::check_min_cost_max_flow(&graph, source, sink, &corrupted).is_err());
        }
        flow::CheckerContractKind::IndependentBipartiteMatchingCertificate => {
            let FlowProblemModelV1::BipartiteMatching {
                left,
                right,
                flow_adapter,
            } = &scenario.payload.model
            else {
                panic!("unexpected bipartite checker model");
            };
            let adapter = flow_adapter
                .as_ref()
                .map(|adapter| (adapter.source.as_str(), adapter.sink.as_str()));
            let model = flow::BipartiteMatchingGraph::new(&graph, left, right, adapter)
                .expect("bipartite checker model");
            let certificate =
                flow::check_bipartite_matching(&graph, &model, &flows).expect("bipartite checker");
            assert_eq!(
                i128::from(certificate.cardinality),
                parse_outcome_i128(&frame, "cardinality")
            );
            assert!(flow::check_bipartite_matching(&graph, &model, &corrupted).is_err());
        }
        flow::CheckerContractKind::IndependentAssignmentCertificate => {
            let FlowProblemModelV1::Assignment {
                agents,
                tasks,
                objective,
            } = &scenario.payload.model
            else {
                panic!("unexpected assignment checker model");
            };
            let model = flow::AssignmentGraph::new(&graph, agents, tasks, *objective)
                .expect("assignment checker model");
            let labels = |field: &str| {
                frame["outcome"][field]
                    .as_array()
                    .unwrap_or_else(|| panic!("missing assignment {field}"))
                    .iter()
                    .map(|label| {
                        label["label"]
                            .as_str()
                            .expect("assignment label")
                            .parse::<i128>()
                            .expect("canonical assignment label")
                    })
                    .collect::<Vec<_>>()
            };
            let agent_labels = labels("agent_labels");
            let task_labels = labels("task_labels");
            let certificate =
                flow::check_assignment(&graph, &model, &flows, &agent_labels, &task_labels)
                    .expect("assignment checker");
            assert_eq!(
                certificate.total_cost,
                parse_outcome_i128(&frame, "total_cost")
            );
            assert!(
                flow::check_assignment(&graph, &model, &corrupted, &agent_labels, &task_labels)
                    .is_err()
            );
        }
        flow::CheckerContractKind::IndependentConvexCostCertificate => {
            let problem = scenario
                .convex_cost_problem(&graph)
                .expect("convex checker model");
            let certificate =
                flow::check_convex_cost_flow(&problem, &flows).expect("convex checker");
            assert_eq!(
                certificate.total_cost,
                parse_outcome_i128(&frame, "total_cost")
            );
            assert!(flow::check_convex_cost_flow(&problem, &corrupted).is_err());
        }
        flow::CheckerContractKind::SourceDefinedInvariant
        | flow::CheckerContractKind::ProjectOracleDemonstratorInvariant => {
            unreachable!("handled before flow decoding")
        }
    }
}

#[allow(clippy::too_many_lines)]
fn verify_project_oracle_checker(
    algorithm: AlgorithmId,
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    fast_final: &serde_json::Value,
    public_timeline: &[String],
) {
    let (checked_projection, overlay_key) = match algorithm {
        AlgorithmId::RandomizedAlmostLinearMaxFlow => {
            let FlowProblemModelV1::MaxFlow { source, sink } = &scenario.payload.model else {
                panic!("randomized max demonstrator requires max-flow model");
            };
            let (source, sink) =
                terminal_indices(graph, source, sink).expect("randomized max terminals");
            let trace = flow::trace_randomized_almost_linear_max_flow(graph, source, sink)
                .expect("randomized max demonstrator trace");
            flow::check_randomized_almost_linear_max_flow_trace(graph, source, sink, &trace)
                .expect("randomized max demonstrator checker");
            verify_randomized_max_public_fields(graph, &trace, public_timeline);
            let projection =
                randomized_almost_linear_trace_frames(scenario, graph, source, sink, &trace)
                    .expect("randomized max demonstrator projection");
            let mut corrupted = trace;
            corrupted.events.pop().expect("randomized max trace event");
            assert!(
                flow::check_randomized_almost_linear_max_flow_trace(
                    graph, source, sink, &corrupted,
                )
                .is_err()
            );
            (projection, "randomized_almost_linear_overlay")
        }
        AlgorithmId::DeterministicAlmostLinearMaxFlow => {
            let FlowProblemModelV1::MaxFlow { source, sink } = &scenario.payload.model else {
                panic!("deterministic max demonstrator requires max-flow model");
            };
            let (source, sink) =
                terminal_indices(graph, source, sink).expect("deterministic max terminals");
            let trace = flow::trace_deterministic_almost_linear_max_flow(graph, source, sink)
                .expect("deterministic max demonstrator trace");
            flow::check_deterministic_almost_linear_max_flow_trace(graph, source, sink, &trace)
                .expect("deterministic max demonstrator checker");
            verify_deterministic_max_public_fields(graph, &trace, public_timeline);
            let projection =
                deterministic_almost_linear_trace_frames(scenario, graph, source, sink, &trace)
                    .expect("deterministic max demonstrator projection");
            let mut corrupted = trace;
            corrupted
                .events
                .pop()
                .expect("deterministic max trace event");
            assert!(
                flow::check_deterministic_almost_linear_max_flow_trace(
                    graph, source, sink, &corrupted,
                )
                .is_err()
            );
            (projection, "deterministic_almost_linear_overlay")
        }
        AlgorithmId::RandomizedAlmostLinearMcf => {
            let target = min_cost_target(scenario, graph);
            let trace = flow::trace_randomized_almost_linear_mcf(graph, &target)
                .expect("randomized MCF demonstrator trace");
            flow::check_randomized_almost_linear_mcf_trace(
                graph,
                &target,
                flow::RANDOMIZED_ALMOST_LINEAR_MCF_DEFAULT_SEED,
                &trace,
            )
            .expect("randomized MCF demonstrator checker");
            verify_randomized_mcf_public_fields(graph, &trace, public_timeline);
            let projection = randomized_almost_linear_mcf_trace_frames(scenario, graph, &trace)
                .expect("randomized MCF demonstrator projection");
            let mut corrupted = trace;
            corrupted.events.pop().expect("randomized MCF trace event");
            assert!(
                flow::check_randomized_almost_linear_mcf_trace(
                    graph,
                    &target,
                    flow::RANDOMIZED_ALMOST_LINEAR_MCF_DEFAULT_SEED,
                    &corrupted,
                )
                .is_err()
            );
            (projection, "randomized_almost_linear_mcf_overlay")
        }
        _ => panic!("unexpected project-oracle demonstrator {algorithm}"),
    };
    let checked_source_projection = normalize_prepared_flow_timeline(checked_projection)
        .expect("checked projection timeline normalizes");
    let checked_timeline = PreparedFlowTimeline::from_source_frames(checked_source_projection)
        .expect("checked source timeline serializes");
    let checked_projection = (0..checked_timeline.len())
        .map(|index| {
            checked_timeline
                .materialize(index)
                .and_then(|frame| {
                    serde_json::to_value(frame).map_err(|error| JsError::new(&error.to_string()))
                })
                .expect("project projection materializes")
        })
        .collect::<Vec<_>>();
    let public_projection =
        assert_public_source_projection(algorithm, public_timeline, &checked_projection);
    let public_final = public_projection
        .last()
        .expect("public project projection has a terminal frame");
    for field in [
        "solve_status",
        "edge_states",
        "outcome",
        "metrics",
        overlay_key,
    ] {
        assert_eq!(
            fast_final.get(field),
            public_final.get(field),
            "{algorithm} fast terminal field {field} is not the composed checked projection"
        );
    }
}

fn debug_variant_kebab(value: &impl std::fmt::Debug) -> String {
    let source = format!("{value:?}");
    let mut result = String::with_capacity(source.len());
    for (index, character) in source.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            result.push('-');
        }
        result.push(character.to_ascii_lowercase());
    }
    result
}

fn public_frame_values(public_timeline: &[String]) -> Vec<serde_json::Value> {
    public_timeline
        .iter()
        .map(|frame| serde_json::from_str(frame).expect("public typed frame is JSON"))
        .collect()
}

fn public_source_frame_values(public_timeline: &[String]) -> Vec<serde_json::Value> {
    let frames = public_frame_values(public_timeline)
        .into_iter()
        .filter(|frame| {
            frame["trace_event"]["catalog_id"]
                .as_str()
                .is_none_or(|catalog_id| !catalog_id.starts_with("feasibility."))
        })
        .collect::<Vec<_>>();
    assert!(frames.iter().all(|frame| {
        frame["trace_event"]["catalog_id"]
            .as_str()
            .is_none_or(|catalog_id| !catalog_id.ends_with(".work-observation"))
    }));
    frames
}

fn metric_values(frame: &serde_json::Value) -> Vec<u128> {
    frame["metrics"]
        .as_array()
        .expect("source projection metrics")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("source metric is a string")
                .parse::<u128>()
                .expect("source metric is a canonical integer")
        })
        .collect()
}

fn normalized_composed_source_event(
    frame: &serde_json::Value,
    source_index: usize,
    source_event_count: usize,
    metrics: &serde_json::Value,
) -> serde_json::Value {
    let mut normalized = frame.clone();
    normalized["event_id"] = serde_json::json!(source_index.to_string());
    normalized["event_count"] = serde_json::json!(source_event_count.to_string());
    normalized["metrics"] = metrics.clone();
    normalized["trace_event"]["event_id"] = serde_json::json!(source_index.to_string());
    normalized["trace_event"]["parent_phase_id"] = serde_json::Value::Null;
    normalized["trace_event_semantics"] = serde_json::Value::Null;
    normalized
}

fn assert_public_source_projection(
    algorithm: AlgorithmId,
    public_timeline: &[String],
    checked_projection: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    let public_projection = public_source_frame_values(public_timeline);
    assert_eq!(
        public_projection.len(),
        checked_projection.len(),
        "{algorithm} composed timeline changed the number of source boundaries"
    );
    let public_base = public_projection.first().expect("public source base");
    let checked_base = checked_projection.first().expect("checked source base");
    for field in [
        "algorithm",
        "frame_revision",
        "graph",
        "model",
        "result_schema_version",
        "run_profile",
        "trace_granularity",
        "trace_steps",
    ] {
        assert_eq!(
            public_base.get(field),
            checked_base.get(field),
            "{algorithm} public Ready field {field} disagrees with its checked source projection"
        );
    }
    assert_eq!(public_base["solve_status"], "ready");
    assert!(public_base["trace_event"].is_null());

    let public_first_metrics = metric_values(&public_projection[1]);
    let checked_first_metrics = metric_values(&checked_projection[1]);
    let offsets = public_first_metrics
        .iter()
        .zip(&checked_first_metrics)
        .map(|(public, checked)| {
            public
                .checked_sub(*checked)
                .expect("composed feasibility work cannot erase source work")
        })
        .collect::<Vec<_>>();
    let source_event_count = checked_projection.len() - 1;
    for index in 1..checked_projection.len() {
        let public_metrics = metric_values(&public_projection[index]);
        let checked_metrics = metric_values(&checked_projection[index]);
        assert_eq!(
            public_metrics
                .iter()
                .zip(&checked_metrics)
                .map(|(public, checked)| public.checked_sub(*checked))
                .collect::<Option<Vec<_>>>(),
            Some(offsets.clone()),
            "{algorithm} feasibility work offset changed inside the source trace at boundary {index}"
        );
        assert_eq!(
            normalized_composed_source_event(
                &public_projection[index],
                index,
                source_event_count,
                &checked_projection[index]["metrics"],
            ),
            normalized_composed_source_event(
                &checked_projection[index],
                index,
                source_event_count,
                &checked_projection[index]["metrics"],
            ),
            "{algorithm} public source boundary {index} is not its checked typed projection"
        );
    }
    public_projection
}

fn verify_public_boundary_metadata(
    frames: &[serde_json::Value],
    index: usize,
    expected_stage: &str,
    expected_catalog_id: Option<&str>,
) {
    assert_eq!(frames[index]["event_count"], frames[0]["event_count"]);
    if let Some(catalog_id) = expected_catalog_id {
        assert!(
            frames[index]["event_id"]
                .as_str()
                .and_then(|value| value.parse::<u64>().ok())
                .is_some_and(|event_id| event_id > 0),
            "a source event owns a nonzero absolute public cursor"
        );
        assert_eq!(frames[index]["trace_event"]["catalog_id"], catalog_id);
    } else {
        assert_eq!(frames[index]["event_id"], "0");
        assert!(frames[index]["trace_event"].is_null());
        return;
    }
    let overlay_stage = frames[index]
        .as_object()
        .expect("public frame object")
        .iter()
        .find_map(|(key, value)| {
            key.ends_with("_overlay")
                .then(|| value.get("stage"))
                .flatten()
        })
        .and_then(serde_json::Value::as_str)
        .expect("typed overlay stage");
    assert_eq!(overlay_stage, expected_stage);
}

fn verify_randomized_max_public_fields(
    graph: &flow::FlowNetwork,
    trace: &flow::RandomizedAlmostLinearMaxFlowTraceResult,
    public_timeline: &[String],
) {
    let frames = public_source_frame_values(public_timeline);
    assert_eq!(frames.len(), trace.events.len() + 1);
    for (event_index, event) in trace.events.iter().enumerate() {
        let index = event_index + 1;
        let snapshot = &event.after;
        let stage = debug_variant_kebab(&snapshot.stage);
        verify_public_boundary_metadata(&frames, index, &stage, Some(event.catalog_id));
        let overlay = &frames[index]["randomized_almost_linear_overlay"];
        assert_eq!(overlay["target_value"], snapshot.target_value.to_string());
        assert_eq!(
            overlay["return_capacity"],
            snapshot.return_capacity.to_string()
        );
        assert_eq!(overlay["iteration"], snapshot.iteration.to_string());
        assert_eq!(
            overlay["forest_pool_size"],
            snapshot.forest_pool_size.to_string()
        );
        assert_eq!(overlay["potential"], snapshot.potential.decimal());
        assert_eq!(overlay["cost_gap"], snapshot.cost_gap.decimal());
        let nodes = overlay["nodes"].as_array().expect("randomized max nodes");
        let edges = overlay["edges"].as_array().expect("randomized max edges");
        assert_eq!(nodes.len(), graph.nodes().len());
        assert_eq!(edges.len(), graph.edges().len());
        for (node, projected) in graph.nodes().iter().zip(nodes) {
            assert_eq!(projected["node_id"], node.id().as_str());
        }
        for ((edge, state), projected) in graph.edges().iter().zip(&snapshot.edges).zip(edges) {
            assert_eq!(projected["edge_id"], edge.id().as_str());
            assert_eq!(projected["interior_flow"], state.interior_flow.decimal());
            assert_eq!(projected["gradient"], state.gradient.decimal());
            assert_eq!(projected["length"], state.length.decimal());
            assert_eq!(
                projected["final_flow"],
                state
                    .final_flow
                    .map_or(serde_json::Value::Null, |value| serde_json::json!(
                        value.to_string()
                    ))
            );
        }
    }
}

fn verify_deterministic_max_public_fields(
    graph: &flow::FlowNetwork,
    trace: &flow::DeterministicAlmostLinearMaxFlowTraceResult,
    public_timeline: &[String],
) {
    let frames = public_source_frame_values(public_timeline);
    assert_eq!(frames.len(), trace.events.len() + 1);
    for (event_index, event) in trace.events.iter().enumerate() {
        let index = event_index + 1;
        let snapshot = &event.after;
        verify_public_boundary_metadata(
            &frames,
            index,
            &debug_variant_kebab(&snapshot.stage),
            Some(event.catalog_id),
        );
        let overlay = &frames[index]["deterministic_almost_linear_overlay"];
        assert_eq!(overlay["target_value"], snapshot.target_value.to_string());
        assert_eq!(
            overlay["return_capacity"],
            snapshot.return_capacity.to_string()
        );
        assert_eq!(overlay["iteration"], snapshot.iteration.to_string());
        assert_eq!(overlay["rebuild_epoch"], snapshot.rebuild_epoch.to_string());
        assert_eq!(overlay["potential"], snapshot.potential.decimal());
        assert_eq!(overlay["cost_gap"], snapshot.cost_gap.decimal());
        let nodes = overlay["nodes"]
            .as_array()
            .expect("deterministic max nodes");
        let edges = overlay["edges"]
            .as_array()
            .expect("deterministic max edges");
        assert_eq!(nodes.len(), graph.nodes().len());
        assert_eq!(edges.len(), graph.edges().len());
        for (node, projected) in graph.nodes().iter().zip(nodes) {
            assert_eq!(projected["node_id"], node.id().as_str());
        }
        for ((edge, state), projected) in graph.edges().iter().zip(&snapshot.edges).zip(edges) {
            assert_eq!(projected["edge_id"], edge.id().as_str());
            assert_eq!(projected["interior_flow"], state.interior_flow.decimal());
            assert_eq!(projected["gradient"], state.gradient.decimal());
            assert_eq!(projected["length"], state.length.decimal());
            assert_eq!(
                projected["final_flow"],
                state
                    .final_flow
                    .map_or(serde_json::Value::Null, |value| serde_json::json!(
                        value.to_string()
                    ))
            );
        }
    }
}

fn verify_randomized_mcf_public_fields(
    graph: &flow::FlowNetwork,
    trace: &flow::RandomizedAlmostLinearMcfTraceResult,
    public_timeline: &[String],
) {
    let frames = public_source_frame_values(public_timeline);
    assert_eq!(frames.len(), trace.events.len() + 1);
    for (event_index, event) in trace.events.iter().enumerate() {
        let index = event_index + 1;
        let snapshot = &event.after;
        let stage = debug_variant_kebab(&snapshot.stage);
        let catalog_id = format!(
            "{}.{stage}",
            AlgorithmId::RandomizedAlmostLinearMcf.as_str()
        );
        verify_public_boundary_metadata(&frames, index, &stage, Some(&catalog_id));
        let overlay = &frames[index]["randomized_almost_linear_mcf_overlay"];
        assert_eq!(overlay["seed"], snapshot.seed.to_string());
        assert_eq!(overlay["initial_cost"], snapshot.initial_cost.decimal());
        assert_eq!(overlay["current_cost"], snapshot.current_cost.decimal());
        assert_eq!(overlay["optimum_cost"], snapshot.optimum_cost.to_string());
        assert_eq!(overlay["potential"], snapshot.potential.decimal());
        assert_eq!(overlay["exact_recovery"], snapshot.exact_recovery);
        let nodes = overlay["nodes"].as_array().expect("randomized MCF nodes");
        let edges = overlay["edges"].as_array().expect("randomized MCF edges");
        assert_eq!(nodes.len(), graph.nodes().len());
        assert_eq!(edges.len(), graph.edges().len());
        for (node, projected) in graph.nodes().iter().zip(nodes) {
            assert_eq!(projected["node_id"], node.id().as_str());
        }
        for ((edge, state), projected) in graph.edges().iter().zip(&snapshot.edges).zip(edges) {
            assert_eq!(projected["edge_id"], edge.id().as_str());
            assert_eq!(projected["current_flow"], state.current_flow.decimal());
            assert_eq!(projected["gradient"], state.gradient.decimal());
            assert_eq!(projected["length"], state.length.decimal());
            assert_eq!(
                projected["final_flow"],
                state
                    .final_flow
                    .map_or(serde_json::Value::Null, |value| serde_json::json!(
                        value.to_string()
                    ))
            );
        }
    }
}

fn verify_binary_blocking_public_fields(
    graph: &flow::FlowNetwork,
    trace: &flow::BinaryBlockingStepTraceResult,
    public_timeline: &[String],
) {
    let frames = public_source_frame_values(public_timeline);
    let source_frames = frames.iter().enumerate().skip(1).collect::<Vec<_>>();
    let mut replay = trace.base_snapshot.clone();
    assert_eq!(source_frames.len(), trace.events.len());
    assert_eq!(frames[0]["event_id"], "0");
    assert_eq!(
        frames[0]["event_count"],
        frames.len().saturating_sub(1).to_string()
    );
    assert!(frames[0]["trace_event"].is_null());
    assert!(frames[0]["binary_blocking_overlay"].is_null());
    for ((public_index, frame), event) in source_frames.into_iter().zip(&trace.events) {
        flow::apply_trace_event(graph, &mut replay, event, flow::FlowTraceDirection::Forward)
            .expect("binary source boundary replays");
        let stage = match event.catalog_id.as_str() {
            "binary-blocking-flow.inspect-initial-cut-arc"
            | "binary-blocking-flow.inspect-binary-length"
            | "binary-blocking-flow.inspect-residual-arc"
            | "binary-blocking-flow.build-reverse-zero-one-adjacency"
            | "binary-blocking-flow.relax-binary-distance"
            | "binary-blocking-flow.build-zero-scc-adjacency"
            | "binary-blocking-flow.inspect-zero-scc-reverse-arc"
            | "binary-blocking-flow.inspect-canonical-cut-arc" => "analyzing",
            "binary-blocking-flow.analyze-binary-network" => "analyzed",
            "binary-blocking-flow.contract-zero-scc"
            | "binary-blocking-flow.inspect-contracted-arc"
            | "binary-blocking-flow.build-lift-adjacency"
            | "binary-blocking-flow.inspect-lift-arc"
            | "binary-blocking-flow.apply-contracted-flow"
            | "binary-blocking-flow.apply-lift-path" => "contracted",
            "binary-blocking-flow.complete-primitive" => "complete",
            catalog_id => panic!("unexpected binary blocking boundary {catalog_id}"),
        };
        verify_public_boundary_metadata(&frames, public_index, stage, Some(&event.catalog_id));
        let overlay = &frame["binary_blocking_overlay"];
        assert_eq!(overlay["upper_bound"], trace.result.upper_bound.to_string());
        assert_eq!(overlay["delta"], trace.result.delta.to_string());
        assert_eq!(
            overlay["delivered"],
            if usize::try_from(event.event_id).expect("binary event ID fits usize")
                == trace.events.len()
            {
                trace.result.value.to_string()
            } else {
                "0".to_owned()
            }
        );
        let nodes = overlay["nodes"].as_array().expect("binary nodes");
        assert_eq!(nodes.len(), graph.nodes().len());
        for (index, (node, projected)) in graph.nodes().iter().zip(nodes).enumerate() {
            assert_eq!(projected["node_id"], node.id().as_str());
            assert_eq!(
                projected["distance"],
                replay.node_labels[index].map_or(serde_json::Value::Null, |value| {
                    serde_json::json!(value.to_string())
                })
            );
            assert_eq!(
                projected["component"],
                if stage == "contracted" || stage == "complete" {
                    trace.result.component_of[index].to_string()
                } else {
                    index.to_string()
                }
            );
        }
        let classification_complete = matches!(
            event.catalog_id.as_str(),
            "binary-blocking-flow.build-zero-scc-adjacency"
                | "binary-blocking-flow.inspect-zero-scc-reverse-arc"
                | "binary-blocking-flow.inspect-canonical-cut-arc"
                | "binary-blocking-flow.analyze-binary-network"
                | "binary-blocking-flow.contract-zero-scc"
                | "binary-blocking-flow.inspect-contracted-arc"
                | "binary-blocking-flow.build-lift-adjacency"
                | "binary-blocking-flow.inspect-lift-arc"
                | "binary-blocking-flow.apply-contracted-flow"
                | "binary-blocking-flow.apply-lift-path"
                | "binary-blocking-flow.complete-primitive"
        );
        assert_binary_classification_overlay(overlay, &trace.result, classification_complete);
    }
    assert_eq!(replay, trace.final_snapshot);
}

fn assert_binary_classification_overlay(
    overlay: &serde_json::Value,
    result: &flow::BinaryBlockingStepResult,
    classification_complete: bool,
) {
    for (field, arcs) in [
        ("base_zero_arcs", result.base_zero_arcs.as_slice()),
        ("special_arcs", result.special_arcs.as_slice()),
        ("admissible_arcs", result.admissible_arcs.as_slice()),
        (
            "zero_admissible_arcs",
            result.zero_admissible_arcs.as_slice(),
        ),
    ] {
        assert_public_residual_arcs(
            &overlay[field],
            visible_binary_arcs(classification_complete, arcs),
        );
    }
}

fn visible_binary_arcs(
    classification_complete: bool,
    arcs: &[flow::ResidualArcId],
) -> &[flow::ResidualArcId] {
    if classification_complete { arcs } else { &[] }
}

fn assert_public_residual_arcs(value: &serde_json::Value, expected: &[flow::ResidualArcId]) {
    let arcs = value.as_array().expect("public residual arc array");
    assert_eq!(arcs.len(), expected.len());
    for (projected, arc) in arcs.iter().zip(expected) {
        assert_eq!(projected["edge_id"], arc.original_edge().as_str());
        assert_eq!(
            projected["direction"],
            match arc.direction() {
                flow::ResidualDirection::Forward => "forward",
                flow::ResidualDirection::Reverse => "reverse",
            }
        );
    }
}

fn assert_public_rational(value: &serde_json::Value, expected: &flow::ParametricRational) {
    assert_eq!(value["numerator"], expected.numerator().to_string());
    assert_eq!(value["denominator"], expected.denominator().to_string());
}

fn assert_optional_public_rational(
    value: &serde_json::Value,
    expected: Option<&flow::ParametricRational>,
) {
    if let Some(expected) = expected {
        assert_public_rational(value, expected);
    } else {
        assert!(value.is_null());
    }
}

fn optional_u64_string(value: Option<u64>) -> serde_json::Value {
    value.map_or(serde_json::Value::Null, |value| {
        serde_json::json!(value.to_string())
    })
}

fn assert_parametric_core_overlay(
    graph: &flow::FlowNetwork,
    problem: &flow::ParametricMaxFlowProblem,
    parameter: &flow::ParametricRational,
    overlay: &serde_json::Value,
) {
    assert_public_rational(&overlay["parameter"], parameter);
    let scale = problem
        .visual_scale_max_capacity(graph)
        .expect("parametric visual scale");
    assert_public_rational(&overlay["visual_scale_max_capacity"], &scale);
    let capacities = overlay["edge_capacities"]
        .as_array()
        .expect("parametric edge capacities");
    assert_eq!(capacities.len(), graph.edges().len());
    for (index, (edge, projected)) in graph.edges().iter().zip(capacities).enumerate() {
        assert_eq!(projected["edge_id"], edge.id().as_str());
        let capacity = problem
            .capacity_at(graph, index, parameter)
            .expect("parametric edge capacity");
        assert_public_rational(&projected["capacity"], &capacity);
    }
}

fn node_id_values(nodes: &[flow::NodeId]) -> serde_json::Value {
    serde_json::Value::Array(
        nodes
            .iter()
            .map(|node| serde_json::json!(node.as_str()))
            .collect(),
    )
}

fn assert_public_parametric_segment(
    projected: &serde_json::Value,
    segment: &flow::ParametricSegment,
) {
    assert_public_rational(&projected["lower"], &segment.lower);
    assert_public_rational(&projected["upper"], &segment.upper);
    assert_eq!(
        projected["intercept"],
        segment.minimal_cut.intercept.to_string()
    );
    assert_eq!(projected["slope"], segment.minimal_cut.slope.to_string());
    assert_eq!(
        projected["minimal_source_side"],
        node_id_values(&segment.minimal_cut.source_side)
    );
    assert_eq!(
        projected["maximal_source_side"],
        node_id_values(&segment.maximal_cut.source_side)
    );
}

fn assert_public_parametric_breakpoint(
    projected: &serde_json::Value,
    breakpoint: &flow::ParametricBreakpoint,
) {
    assert_public_rational(&projected["parameter"], &breakpoint.parameter);
    assert_eq!(
        projected["before_source_side"],
        node_id_values(&breakpoint.before_source_side)
    );
    assert_eq!(
        projected["after_source_side"],
        node_id_values(&breakpoint.after_source_side)
    );
    assert_eq!(
        projected["exact_minimal_source_side"],
        node_id_values(&breakpoint.exact_minimal_source_side)
    );
    assert_eq!(
        projected["exact_maximal_source_side"],
        node_id_values(&breakpoint.exact_maximal_source_side)
    );
    assert_eq!(
        projected["entering_nodes"],
        node_id_values(&breakpoint.entering_nodes)
    );
}

fn assert_parametric_recorded_outputs(
    overlay: &serde_json::Value,
    segments: &[&flow::ParametricSegment],
    breakpoints: &[&flow::ParametricBreakpoint],
) {
    let projected_segments = overlay["recorded_segments"]
        .as_array()
        .expect("recorded parametric segments");
    let projected_breakpoints = overlay["recorded_breakpoints"]
        .as_array()
        .expect("recorded parametric breakpoints");
    assert_eq!(projected_segments.len(), segments.len());
    assert_eq!(projected_breakpoints.len(), breakpoints.len());
    for (projected, segment) in projected_segments.iter().zip(segments) {
        assert_public_parametric_segment(projected, segment);
    }
    for (projected, breakpoint) in projected_breakpoints.iter().zip(breakpoints) {
        assert_public_parametric_breakpoint(projected, breakpoint);
    }
}

fn assert_parametric_event_outputs(
    overlay: &serde_json::Value,
    optimal: bool,
    result_segments: &[flow::ParametricSegment],
    result_breakpoints: &[flow::ParametricBreakpoint],
    recorded_segments: &[&flow::ParametricSegment],
    recorded_breakpoints: &[&flow::ParametricBreakpoint],
) {
    if optimal {
        assert_parametric_recorded_outputs(
            overlay,
            &result_segments.iter().collect::<Vec<_>>(),
            &result_breakpoints.iter().collect::<Vec<_>>(),
        );
    } else {
        assert_parametric_recorded_outputs(overlay, recorded_segments, recorded_breakpoints);
    }
}

fn assert_parametric_pseudoflow_traversal(
    traversal: &serde_json::Value,
    event: &flow::ParametricPseudoflowTraceEvent,
    kind: &str,
) {
    assert_eq!(traversal["kind"], kind);
    assert_public_rational(&traversal["lower"], &event.lower);
    assert_public_rational(&traversal["upper"], &event.upper);
    assert_optional_public_rational(&traversal["probe"], event.parameter.as_ref());
    assert_eq!(
        traversal["orientation"],
        event
            .orientation
            .map_or(serde_json::Value::Null, |orientation| {
                serde_json::json!(match orientation {
                    flow::ParametricTraversalOrientation::Forward => "forward",
                    flow::ParametricTraversalOrientation::Reverse => "reverse",
                })
            })
    );
    assert_eq!(
        traversal["race_winner"],
        event.race_winner.map_or(serde_json::Value::Null, |winner| {
            serde_json::json!(match winner {
                flow::ParametricRaceWinner::Forward => "forward",
                flow::ParametricRaceWinner::Reverse => "reverse",
            })
        })
    );
    assert_eq!(traversal["cold_static_rerun"], false);
    assert!(traversal["static_run_ordinal"].is_null());
    assert!(traversal["scale_denominator"].is_null());
    assert_eq!(traversal["lower_source_side"], serde_json::json!([]));
    assert_eq!(traversal["upper_source_side"], serde_json::json!([]));
    assert_eq!(
        traversal["normalized_tree_reused"],
        event.normalized_tree_reused
    );
    assert_eq!(traversal["labels_retained"], event.labels_retained);
    assert_eq!(
        traversal["active_nodes"],
        optional_u64_string(event.active_nodes)
    );
    assert_eq!(
        traversal["left_active_nodes"],
        optional_u64_string(event.left_active_nodes)
    );
    assert_eq!(
        traversal["right_active_nodes"],
        optional_u64_string(event.right_active_nodes)
    );
    assert_eq!(
        traversal["renormalization_pushes"],
        event.renormalization_pushes.to_string()
    );
    assert_eq!(
        traversal["renormalization_splits"],
        event.renormalization_splits.to_string()
    );
}

fn verify_parametric_pseudoflow_public_fields(
    algorithm: AlgorithmId,
    graph: &flow::FlowNetwork,
    problem: &flow::ParametricMaxFlowProblem,
    trace: &flow::ParametricPseudoflowTraceResult,
    public_timeline: &[String],
) {
    let frames = public_source_frame_values(public_timeline);
    assert_eq!(frames.len(), trace.events.len() + 1);
    assert_eq!(frames[0]["event_id"], "0");
    assert!(frames[0]["trace_event"].is_null());
    assert_parametric_core_overlay(
        graph,
        problem,
        problem.minimum(),
        &frames[0]["parametric_overlay"],
    );
    assert_parametric_recorded_outputs(&frames[0]["parametric_overlay"], &[], &[]);
    let mut recorded_segments = Vec::new();
    let mut recorded_breakpoints = Vec::new();
    for (index, event) in trace.events.iter().enumerate() {
        let frame = &frames[index + 1];
        let kind = debug_variant_kebab(&event.kind);
        assert_eq!(frame["event_count"], trace.events.len().to_string());
        assert_eq!(
            frame["trace_event"]["catalog_id"],
            format!("{}.{}", algorithm.as_str(), kind)
        );
        let parameter = if event.kind == flow::ParametricPseudoflowEventKind::Optimal {
            problem.maximum()
        } else {
            event.parameter.as_ref().unwrap_or(&event.lower)
        };
        let overlay = &frame["parametric_overlay"];
        if event.kind == flow::ParametricPseudoflowEventKind::RecordSegment
            && let Some(segment) = trace
                .result
                .segments
                .iter()
                .find(|segment| segment.lower == event.lower && segment.upper == event.upper)
            && !recorded_segments
                .iter()
                .any(|candidate: &&flow::ParametricSegment| {
                    candidate.lower == segment.lower && candidate.upper == segment.upper
                })
        {
            recorded_segments.push(segment);
        }
        if event.kind == flow::ParametricPseudoflowEventKind::RecordBreakpoint
            && let Some(parameter) = &event.parameter
            && let Some(breakpoint) = trace
                .result
                .breakpoints
                .iter()
                .find(|breakpoint| breakpoint.parameter == *parameter)
        {
            recorded_breakpoints.push(breakpoint);
        }
        assert_parametric_core_overlay(graph, problem, parameter, overlay);
        assert_parametric_event_outputs(
            overlay,
            event.kind == flow::ParametricPseudoflowEventKind::Optimal,
            &trace.result.segments,
            &trace.result.breakpoints,
            &recorded_segments,
            &recorded_breakpoints,
        );
        assert_parametric_pseudoflow_traversal(&overlay["traversal"], event, &kind);
    }
}

fn verify_parametric_rerun_public_fields(
    algorithm: AlgorithmId,
    graph: &flow::FlowNetwork,
    problem: &flow::ParametricMaxFlowProblem,
    trace: &flow::ParametricBreakpointRerunTraceResult,
    public_timeline: &[String],
) {
    let frames = public_source_frame_values(public_timeline);
    assert_eq!(frames.len(), trace.events.len() + 1);
    assert_eq!(frames[0]["event_id"], "0");
    assert!(frames[0]["trace_event"].is_null());
    assert_parametric_core_overlay(
        graph,
        problem,
        problem.minimum(),
        &frames[0]["parametric_overlay"],
    );
    assert_parametric_recorded_outputs(&frames[0]["parametric_overlay"], &[], &[]);
    let mut recorded_segments = Vec::new();
    let mut recorded_breakpoints = Vec::new();
    for (index, event) in trace.events.iter().enumerate() {
        let frame = &frames[index + 1];
        let kind = debug_variant_kebab(&event.kind);
        assert_eq!(frame["event_count"], trace.events.len().to_string());
        assert_eq!(
            frame["trace_event"]["catalog_id"],
            format!("{}.{}", algorithm.as_str(), kind)
        );
        let parameter = if event.kind == flow::ParametricTraceEventKind::Optimal {
            problem.maximum()
        } else {
            event.parameter.as_ref().unwrap_or(&event.lower)
        };
        let overlay = &frame["parametric_overlay"];
        if event.kind == flow::ParametricTraceEventKind::RecordSegment
            && let Some(segment) = trace
                .result
                .segments
                .iter()
                .find(|segment| segment.lower == event.lower && segment.upper == event.upper)
            && !recorded_segments
                .iter()
                .any(|candidate: &&flow::ParametricSegment| {
                    candidate.lower == segment.lower && candidate.upper == segment.upper
                })
        {
            recorded_segments.push(segment);
        }
        if event.kind == flow::ParametricTraceEventKind::RecordBreakpoint
            && let Some(parameter) = &event.parameter
            && let Some(breakpoint) = trace
                .result
                .breakpoints
                .iter()
                .find(|breakpoint| breakpoint.parameter == *parameter)
        {
            recorded_breakpoints.push(breakpoint);
        }
        assert_parametric_core_overlay(graph, problem, parameter, overlay);
        assert_parametric_event_outputs(
            overlay,
            event.kind == flow::ParametricTraceEventKind::Optimal,
            &trace.result.segments,
            &trace.result.breakpoints,
            &recorded_segments,
            &recorded_breakpoints,
        );
        assert_parametric_rerun_traversal(&overlay["traversal"], event, &kind);
    }
}

fn assert_parametric_rerun_traversal(
    traversal: &serde_json::Value,
    event: &flow::ParametricTraceEvent,
    kind: &str,
) {
    assert_eq!(traversal["kind"], kind);
    assert_public_rational(&traversal["lower"], &event.lower);
    assert_public_rational(&traversal["upper"], &event.upper);
    assert_optional_public_rational(&traversal["probe"], event.parameter.as_ref());
    assert!(traversal["orientation"].is_null());
    assert!(traversal["race_winner"].is_null());
    assert_eq!(traversal["cold_static_rerun"], event.cold_static_rerun);
    assert_eq!(
        traversal["static_run_ordinal"],
        optional_u64_string(event.static_run_ordinal)
    );
    assert_eq!(
        traversal["scale_denominator"],
        event
            .scale_denominator
            .as_ref()
            .map_or(serde_json::Value::Null, |value| serde_json::json!(
                value.to_string()
            ))
    );
    assert_eq!(
        traversal["lower_source_side"],
        node_id_values(&event.lower_source_side)
    );
    assert_eq!(
        traversal["upper_source_side"],
        node_id_values(&event.upper_source_side)
    );
    assert_eq!(
        traversal["normalized_tree_reused"],
        event.normalized_tree_reused
    );
    assert_eq!(traversal["labels_retained"], false);
    assert!(traversal["active_nodes"].is_null());
    assert!(traversal["left_active_nodes"].is_null());
    assert!(traversal["right_active_nodes"].is_null());
    assert_eq!(traversal["renormalization_pushes"], "0");
    assert_eq!(traversal["renormalization_splits"], "0");
}

fn verify_electrical_public_fields(
    graph: &flow::FlowNetwork,
    trace: &flow::ElectricalFlowTraceResult,
    public_timeline: &[String],
) {
    let frames = public_source_frame_values(public_timeline);
    assert_eq!(frames.len(), trace.events.len() + 1);
    for (event_index, event) in trace.events.iter().enumerate() {
        let index = event_index + 1;
        let snapshot = &event.after;
        verify_public_boundary_metadata(
            &frames,
            index,
            &debug_variant_kebab(&snapshot.stage),
            Some(event.catalog_id),
        );
        let overlay = &frames[index]["electrical_flow_overlay"];
        assert_eq!(overlay["iteration"], snapshot.iteration.to_string());
        assert_eq!(overlay["residual_l2"], snapshot.residual_l2.decimal());
        assert_eq!(
            overlay["effective_resistance"],
            snapshot.effective_resistance.decimal()
        );
        assert_eq!(overlay["total_energy"], snapshot.total_energy.decimal());
        assert_eq!(overlay["converged"], snapshot.converged);
        let nodes = overlay["nodes"].as_array().expect("electrical nodes");
        let edges = overlay["edges"].as_array().expect("electrical edges");
        assert_eq!(nodes.len(), graph.nodes().len());
        assert_eq!(edges.len(), graph.edges().len());
        for ((node, potential), projected) in
            graph.nodes().iter().zip(&snapshot.potentials).zip(nodes)
        {
            assert_eq!(projected["node_id"], node.id().as_str());
            assert_eq!(projected["potential"], potential.decimal());
        }
        for ((edge, state), projected) in graph.edges().iter().zip(&snapshot.edges).zip(edges) {
            assert_eq!(projected["edge_id"], edge.id().as_str());
            assert_eq!(projected["current"], state.current.decimal());
            assert_eq!(projected["energy"], state.energy.decimal());
        }
    }
}

fn verify_minimum_ratio_cycle_public_fields(
    graph: &flow::FlowNetwork,
    trace: &flow::MinimumRatioCycleTraceResult,
    public_timeline: &[String],
) {
    let frames = public_source_frame_values(public_timeline);
    assert_eq!(frames.len(), trace.events.len() + 1);
    for (event_index, event) in trace.events.iter().enumerate() {
        let index = event_index + 1;
        let snapshot = &event.after;
        let public_catalog_id = event
            .catalog_id
            .strip_prefix("minimum-ratio-cycle.")
            .map_or_else(
                || event.catalog_id.to_owned(),
                |suffix| format!("minimum-ratio-cycle-max-flow.{suffix}"),
            );
        verify_public_boundary_metadata(
            &frames,
            index,
            &debug_variant_kebab(&snapshot.stage),
            Some(&public_catalog_id),
        );
        let overlay = &frames[index]["minimum_ratio_cycle_overlay"];
        assert_eq!(
            overlay["selected_edge_count"],
            snapshot.selected_edge_count.to_string()
        );
        assert_eq!(
            overlay["maximum_absolute_balance"],
            snapshot.maximum_absolute_balance.to_string()
        );
        assert_eq!(
            overlay["enumerated_vectors"],
            snapshot.metrics.enumerated_vectors.to_string()
        );
        let edges = overlay["edges"].as_array().expect("ratio-cycle edges");
        assert_eq!(edges.len(), graph.edges().len());
        for ((edge, state), projected) in graph.edges().iter().zip(&snapshot.edges).zip(edges) {
            assert_eq!(projected["edge_id"], edge.id().as_str());
            assert_eq!(projected["gradient"], state.gradient.to_string());
            assert_eq!(projected["length"], state.length.to_string());
            assert_eq!(projected["selected_sign"], state.selected_sign.to_string());
        }
    }
}

fn verify_tardos_public_fields(
    graph: &flow::FlowNetwork,
    trace: &flow::TardosFrameworkTraceResult,
    public_timeline: &[String],
) {
    let frames = public_source_frame_values(public_timeline);
    assert_eq!(frames.len(), trace.events.len() + 1);
    for (event_index, event) in trace.events.iter().enumerate() {
        let index = event_index + 1;
        let snapshot = &event.after;
        verify_public_boundary_metadata(
            &frames,
            index,
            &debug_variant_kebab(&snapshot.stage),
            Some(event.catalog_id),
        );
        let overlay = &frames[index]["tardos_framework_overlay"];
        assert_eq!(overlay["epsilon"], snapshot.epsilon.to_string());
        assert_eq!(overlay["threshold"], snapshot.threshold.to_string());
        assert_eq!(overlay["determinant_bound"], "1");
        let nodes = overlay["nodes"].as_array().expect("Tardos nodes");
        assert_eq!(nodes.len(), graph.nodes().len());
        for ((node, potential), projected) in
            graph.nodes().iter().zip(&snapshot.potentials).zip(nodes)
        {
            assert_eq!(projected["node_id"], node.id().as_str());
            assert_eq!(projected["potential"], potential.to_string());
        }
        assert_eq!(
            overlay["fixed_variables"].as_array().map(Vec::len),
            Some(snapshot.fixed_variables.len())
        );
    }
}

fn verify_minimum_ratio_cycle_mcf_public_fields(
    graph: &flow::FlowNetwork,
    trace: &flow::MinimumRatioCycleMcfTraceResult,
    public_timeline: &[String],
) {
    let frames = public_source_frame_values(public_timeline);
    assert_eq!(frames.len(), trace.events.len() + 1);
    for (event_index, event) in trace.events.iter().enumerate() {
        let index = event_index + 1;
        let snapshot = &event.after;
        verify_public_boundary_metadata(
            &frames,
            index,
            &debug_variant_kebab(&snapshot.stage),
            Some(event.catalog_id),
        );
        let overlay = &frames[index]["minimum_ratio_cycle_mcf_overlay"];
        assert_eq!(overlay["alpha"], snapshot.alpha.decimal());
        assert_eq!(overlay["optimum_cost"], snapshot.optimum_cost.to_string());
        assert_eq!(overlay["initial_cost"], snapshot.initial_cost.decimal());
        assert_eq!(overlay["current_cost"], snapshot.current_cost.decimal());
        assert_eq!(overlay["cost_gap"], snapshot.cost_gap.decimal());
        assert_eq!(
            overlay["selected_edge_count"],
            snapshot.selected_edge_count.to_string()
        );
        let edges = overlay["edges"].as_array().expect("ratio-cycle MCF edges");
        assert_eq!(edges.len(), graph.edges().len());
        for ((edge, state), projected) in graph.edges().iter().zip(&snapshot.edges).zip(edges) {
            assert_eq!(projected["edge_id"], edge.id().as_str());
            assert_eq!(projected["initial_flow"], state.initial_flow.decimal());
            assert_eq!(projected["updated_flow"], state.updated_flow.decimal());
            assert_eq!(projected["gradient"], state.gradient.decimal());
            assert_eq!(projected["length"], state.length.decimal());
        }
    }
}

fn min_cost_target(scenario: &FlowScenarioV1, graph: &flow::FlowNetwork) -> Vec<i128> {
    match &scenario.payload.model {
        FlowProblemModelV1::FixedFlowMinCost {
            source,
            sink,
            required_flow,
        } => {
            let (source, sink) =
                terminal_indices(graph, source, sink).expect("source checker terminals");
            flow::fixed_flow_divergences(
                graph,
                source,
                sink,
                required_flow.parse().expect("source checker required flow"),
            )
            .expect("source checker fixed divergence")
        }
        FlowProblemModelV1::Circulation {}
        | FlowProblemModelV1::Transshipment {}
        | FlowProblemModelV1::Transportation { .. } => {
            flow::supply_divergences(graph).expect("source checker supply divergence")
        }
        model => panic!("unexpected source-defined MCF model {model:?}"),
    }
}

#[allow(clippy::too_many_lines)]
fn verify_source_defined_checker(
    algorithm: AlgorithmId,
    scenario: &FlowScenarioV1,
    graph: &flow::FlowNetwork,
    fast_final: &serde_json::Value,
    public_timeline: &[String],
) {
    let checked_projection = match algorithm {
        AlgorithmId::BinaryBlockingFlow => {
            let FlowProblemModelV1::MaxFlow { source, sink } = &scenario.payload.model else {
                panic!("binary blocking-flow requires max-flow model");
            };
            let (source, sink) =
                terminal_indices(graph, source, sink).expect("binary checker terminals");
            let initial = graph
                .edges()
                .iter()
                .map(flow::FlowEdge::lower)
                .collect::<Vec<_>>();
            let trace = flow::trace_binary_blocking_first_step(graph, source, sink, &initial)
                .expect("binary blocking trace");
            flow::check_binary_blocking_step_trace(graph, source, sink, &trace)
                .expect("binary blocking checker");
            verify_binary_blocking_public_fields(graph, &trace, public_timeline);
            let checked_projection =
                binary_blocking_trace_frames(scenario, graph, &trace).expect("binary projection");
            let mut corrupted = trace;
            corrupted.events.pop().expect("binary trace event");
            assert!(
                flow::check_binary_blocking_step_trace(graph, source, sink, &corrupted).is_err()
            );
            checked_projection
        }
        AlgorithmId::ParametricPseudoflow => {
            let problem = scenario
                .parametric_problem(graph)
                .expect("parametric source checker problem");
            let trace = flow::trace_parametric_pseudoflow(graph, &problem)
                .expect("parametric pseudoflow trace");
            flow::check_parametric_pseudoflow_trace(graph, &problem, &trace)
                .expect("parametric pseudoflow checker");
            verify_parametric_pseudoflow_public_fields(
                algorithm,
                graph,
                &problem,
                &trace,
                public_timeline,
            );
            let checked_projection =
                parametric_pseudoflow_trace_frames(scenario, graph, &problem, &trace)
                    .expect("parametric pseudoflow projection");
            let mut corrupted = trace;
            corrupted.events.pop().expect("parametric trace event");
            assert!(flow::check_parametric_pseudoflow_trace(graph, &problem, &corrupted).is_err());
            checked_projection
        }
        AlgorithmId::ParametricBreakpointRerun => {
            let problem = scenario
                .parametric_problem(graph)
                .expect("parametric rerun checker problem");
            let trace = flow::trace_parametric_breakpoint_rerun(graph, &problem)
                .expect("parametric rerun trace");
            flow::check_parametric_breakpoint_rerun_trace(graph, &problem, &trace)
                .expect("parametric rerun checker");
            verify_parametric_rerun_public_fields(
                algorithm,
                graph,
                &problem,
                &trace,
                public_timeline,
            );
            let checked_projection =
                parametric_rerun_trace_frames(scenario, graph, &problem, &trace)
                    .expect("parametric rerun projection");
            let mut corrupted = trace;
            corrupted.events.pop().expect("parametric rerun event");
            assert!(
                flow::check_parametric_breakpoint_rerun_trace(graph, &problem, &corrupted).is_err()
            );
            checked_projection
        }
        AlgorithmId::ElectricalFlow => {
            let FlowProblemModelV1::MaxFlow { source, sink } = &scenario.payload.model else {
                panic!("electrical flow requires max-flow model");
            };
            let (source, sink) =
                terminal_indices(graph, source, sink).expect("electrical checker terminals");
            let trace =
                flow::trace_electrical_flow(graph, source, sink).expect("electrical-flow trace");
            flow::check_electrical_flow_trace(graph, source, sink, &trace)
                .expect("electrical-flow checker");
            verify_electrical_public_fields(graph, &trace, public_timeline);
            let checked_projection =
                electrical_flow_trace_frames(scenario, graph, source, sink, &trace)
                    .expect("electrical projection");
            let mut corrupted = trace;
            corrupted.events.pop().expect("electrical trace event");
            assert!(flow::check_electrical_flow_trace(graph, source, sink, &corrupted).is_err());
            checked_projection
        }
        AlgorithmId::MinimumRatioCycleMaxFlow => {
            let trace = flow::trace_minimum_ratio_cycle(graph).expect("minimum-ratio-cycle trace");
            flow::check_minimum_ratio_cycle_trace(graph, &trace)
                .expect("minimum-ratio-cycle checker");
            verify_minimum_ratio_cycle_public_fields(graph, &trace, public_timeline);
            let checked_projection = minimum_ratio_cycle_trace_frames(scenario, graph, &trace)
                .expect("minimum-ratio-cycle projection");
            let mut corrupted = trace;
            corrupted.events.pop().expect("minimum-ratio-cycle event");
            assert!(flow::check_minimum_ratio_cycle_trace(graph, &corrupted).is_err());
            checked_projection
        }
        AlgorithmId::TardosFramework => {
            let target = min_cost_target(scenario, graph);
            let potentials =
                tardos_framework_config(scenario, graph).expect("Tardos source checker potentials");
            let trace = flow::trace_tardos_framework_primitive(graph, &target, &potentials)
                .expect("Tardos trace");
            flow::check_tardos_framework_trace(graph, &target, &potentials, &trace)
                .expect("Tardos checker");
            verify_tardos_public_fields(graph, &trace, public_timeline);
            let checked_projection =
                tardos_framework_trace_frames(scenario, graph, &trace).expect("Tardos projection");
            let mut corrupted = trace;
            corrupted.events.pop().expect("Tardos trace event");
            assert!(
                flow::check_tardos_framework_trace(graph, &target, &potentials, &corrupted)
                    .is_err()
            );
            checked_projection
        }
        AlgorithmId::MinimumRatioCycleMcf => {
            let target = min_cost_target(scenario, graph);
            let trace = flow::trace_minimum_ratio_cycle_mcf(graph, &target)
                .expect("minimum-ratio-cycle MCF trace");
            flow::check_minimum_ratio_cycle_mcf_trace(graph, &target, &trace)
                .expect("minimum-ratio-cycle MCF checker");
            verify_minimum_ratio_cycle_mcf_public_fields(graph, &trace, public_timeline);
            let checked_projection = minimum_ratio_cycle_mcf_trace_frames(scenario, graph, &trace)
                .expect("minimum-ratio-cycle MCF projection");
            let mut corrupted = trace;
            corrupted
                .events
                .pop()
                .expect("minimum-ratio-cycle MCF event");
            assert!(flow::check_minimum_ratio_cycle_mcf_trace(graph, &target, &corrupted).is_err());
            checked_projection
        }
        _ => panic!("unexpected source-defined checker {algorithm}"),
    };
    let checked_projection = normalize_prepared_flow_timeline(checked_projection)
        .expect("source-defined checked projection timeline normalizes")
        .iter()
        .map(|frame| serde_json::to_value(frame).expect("checked projection serializes"))
        .collect::<Vec<_>>();
    let public_projection =
        assert_public_source_projection(algorithm, public_timeline, &checked_projection);
    let public_final = public_projection
        .last()
        .expect("public projection has a terminal frame");
    let overlay_key = match algorithm {
        AlgorithmId::BinaryBlockingFlow => "binary_blocking_overlay",
        AlgorithmId::ParametricPseudoflow | AlgorithmId::ParametricBreakpointRerun => {
            "parametric_overlay"
        }
        AlgorithmId::ElectricalFlow => "electrical_flow_overlay",
        AlgorithmId::MinimumRatioCycleMaxFlow => "minimum_ratio_cycle_overlay",
        AlgorithmId::TardosFramework => "tardos_framework_overlay",
        AlgorithmId::MinimumRatioCycleMcf => "minimum_ratio_cycle_mcf_overlay",
        _ => unreachable!("source-defined checker filtered above"),
    };
    for field in [
        "solve_status",
        "edge_states",
        "outcome",
        "metrics",
        overlay_key,
    ] {
        let mut fast_field = fast_final.get(field).cloned();
        let mut public_field = public_final.get(field).cloned();
        if field == "parametric_overlay" {
            for value in [&mut fast_field, &mut public_field] {
                if let Some(object) = value.as_mut().and_then(serde_json::Value::as_object_mut) {
                    object.remove("traversal");
                }
            }
        }
        assert_eq!(
            fast_field, public_field,
            "{algorithm} fast terminal field {field} is not the composed checked projection"
        );
    }
}

#[test]
fn every_algorithm_constructs_the_closed_typed_dispatch_key() {
    for algorithm in AlgorithmId::ALL.iter().copied() {
        let scenario_source = conformance_scenario(algorithm, "trace");
        let scenario = decode_flow_scenario(scenario_source.as_bytes())
            .expect("typed dispatch conformance scenario");
        let runner = RuntimeRunner::for_algorithm(algorithm);
        let expected_route = runner.route();
        let dispatch =
            FlowDispatch::try_new(&scenario.payload.model, runner).unwrap_or_else(|error| {
                panic!("{} typed dispatch failed: {error:?}", algorithm.as_str())
            });
        let actual_route = match dispatch {
            FlowDispatch::ParametricMaxFlow(_) => RuntimeRouteKind::ParametricMaxFlow,
            FlowDispatch::MaxFlow { .. } => RuntimeRouteKind::MaxFlow,
            FlowDispatch::FixedFlowMinCost { .. } | FlowDispatch::BalanceMinCost(_) => {
                RuntimeRouteKind::MinCostFlow
            }
            FlowDispatch::MinCostMaxFlow { .. } => RuntimeRouteKind::MinCostMaxFlow,
            FlowDispatch::BipartiteMatching { .. } => RuntimeRouteKind::BipartiteMatching,
            FlowDispatch::Assignment { .. } => RuntimeRouteKind::Assignment,
            FlowDispatch::Transportation { .. } => RuntimeRouteKind::Transportation,
            FlowDispatch::PlanarMaxFlow { .. } => RuntimeRouteKind::PlanarMaxFlow,
            FlowDispatch::ConvexCostFlow(_) => RuntimeRouteKind::ConvexCostFlow,
        };
        assert_eq!(actual_route, expected_route, "{}", algorithm.as_str());
    }

    let max_flow_source = conformance_scenario(AlgorithmId::EdmondsKarp, "trace");
    let max_flow = decode_flow_scenario(max_flow_source.as_bytes())
        .expect("incompatible typed dispatch scenario");
    assert!(
        FlowDispatch::try_new(&max_flow.payload.model, RuntimeRunner::MinCostMaxFlow).is_err(),
        "an incompatible runner must fail closed instead of selecting a fallback"
    );
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one closed 93-endpoint audit keeps the catalog-wide invariants atomic"
)]
fn every_algorithm_has_fast_trace_identity_parity_replay_and_atomic_publication() {
    let contracts = flow_algorithm_conformance_contracts().expect("conformance contracts");
    assert_eq!(contracts.len(), AlgorithmId::ALL.len());
    let mut violations = Vec::new();
    for (algorithm, contract) in AlgorithmId::ALL.iter().copied().zip(&contracts) {
        assert_eq!(contract.algorithm_id, algorithm);
        let (base, trace_final, event_count, first_event, trace_timeline) =
            run_to_end(algorithm, "trace", true);
        assert!(event_count > 0, "{} has an empty trace", algorithm.as_str());
        if !first_event.starts_with(algorithm.as_str()) {
            violations.push(format!(
                "{} first event belongs to another contract: {first_event}",
                algorithm.as_str()
            ));
        }
        let trace_value: serde_json::Value =
            serde_json::from_str(&trace_final).expect("trace final is JSON");
        assert_eq!(trace_value["algorithm"]["id"], algorithm.as_str());

        let descriptor = find_algorithm_by_id(algorithm).expect("catalog descriptor");
        let expected_steps =
            serde_json::to_value(descriptor.trace_steps).expect("step contract serializes");
        let mut detailed_boundaries = 0_usize;
        let mut phase_boundaries = 0_usize;
        let mut operation_boundaries = 0_usize;
        let mut certify_boundaries = 0_usize;
        let mut boundary_by_event = std::collections::BTreeMap::new();
        let mut catalog_ids = std::collections::BTreeSet::new();
        let mut parent_links = Vec::new();
        for frame in &trace_timeline {
            let value: serde_json::Value =
                serde_json::from_str(frame).expect("trace timeline frame is JSON");
            assert_eq!(
                value["trace_steps"],
                expected_steps,
                "{} scene step contract drifted from its descriptor",
                algorithm.as_str()
            );
            let event = &value["trace_event"];
            let event_id = event["event_id"].as_str();
            let boundary = event["minimum_granularity"].as_str();
            if let (Some(event_id), Some(boundary)) = (event_id, boundary) {
                if let Some(catalog_id) = event["catalog_id"].as_str() {
                    assert_ne!(
                        catalog_id,
                        algorithm.as_str(),
                        "{} event {event_id} collapses its step identity into the algorithm identity",
                        algorithm.as_str()
                    );
                    assert!(
                        catalog_id.contains('.'),
                        "{} event {event_id} lacks a stage/action suffix: {catalog_id}",
                        algorithm.as_str()
                    );
                    catalog_ids.insert(catalog_id.to_owned());
                }
                let semantics = &value["trace_event_semantics"];
                let role = semantics["role"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{event_id} is missing its typed event role"));
                assert!(
                    ["observe", "select", "mutate", "commit", "certify"].contains(&role),
                    "{} event {event_id} has an unknown role {role}",
                    algorithm.as_str()
                );
                if role == "certify" {
                    certify_boundaries += 1;
                } else {
                    match boundary {
                        "phase" => phase_boundaries += 1,
                        "operation" => operation_boundaries += 1,
                        "micro" => detailed_boundaries += 1,
                        _ => {}
                    }
                }
                if boundary == "phase" {
                    assert!(
                        event["parent_phase_id"].as_str().is_none(),
                        "{} phase event {} must not have a parent phase",
                        algorithm.as_str(),
                        event_id
                    );
                }
                let work = semantics["work_deltas"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{event_id} is missing typed work deltas"));
                assert_eq!(
                    work.first().and_then(|delta| delta["unit"].as_str()),
                    Some("published-transition"),
                    "{} event {event_id} is missing the publication work unit",
                    algorithm.as_str()
                );
                if event["catalog_id"] == "edmonds-karp.bfs-complete" {
                    assert!(
                        work.iter().any(|delta| delta["unit"] == "bfs-run"),
                        "Edmonds-Karp BFS completion must own one BFS work unit"
                    );
                }
                if algorithm == AlgorithmId::ElectricalFlow {
                    assert!(
                        work.iter().all(|delta| delta["unit"] != "bfs-run"),
                        "electrical-flow metric slots must not be mislabeled as BFS work"
                    );
                }
                assert_eq!(
                    work.first().and_then(|delta| delta["count"].as_str()),
                    Some("1"),
                    "{} event {event_id} has an invalid publication delta",
                    algorithm.as_str()
                );
                let detail_work = work
                    .iter()
                    .filter(|delta| delta["unit"] == "detail-primitive")
                    .collect::<Vec<_>>();
                if boundary == "micro" {
                    assert_eq!(
                        detail_work.len(),
                        1,
                        "{} Detail event {event_id} must own one detail primitive",
                        algorithm.as_str()
                    );
                    assert_eq!(
                        detail_work[0]["count"].as_str(),
                        Some("1"),
                        "{} Detail event {event_id} aggregated its published primitive",
                        algorithm.as_str()
                    );
                } else {
                    assert!(
                        detail_work.is_empty(),
                        "{} non-Detail event {event_id} claims a detail primitive",
                        algorithm.as_str()
                    );
                }
                let largest_delta = work
                    .iter()
                    .map(|delta| {
                        delta["count"]
                            .as_str()
                            .expect("work delta count")
                            .parse::<u128>()
                            .expect("canonical work delta")
                    })
                    .max()
                    .expect("published transition delta");
                assert_eq!(
                    semantics["aggregation_count"]
                        .as_str()
                        .expect("aggregation count")
                        .parse::<u128>()
                        .expect("canonical aggregation count"),
                    largest_delta,
                    "{} event {event_id} aggregation disagrees with its work deltas",
                    algorithm.as_str()
                );
                let touched_refs = event["entity_refs"].as_array().expect("event entity refs");
                let touched = touched_refs
                    .iter()
                    .map(serde_json::Value::to_string)
                    .collect::<std::collections::BTreeSet<_>>();
                assert_eq!(
                    touched.len(),
                    touched_refs.len(),
                    "{} event {event_id} publishes duplicate focus identities",
                    algorithm.as_str()
                );
                let changed_refs = semantics["changed_entity_refs"]
                    .as_array()
                    .expect("changed entity refs");
                let changed = changed_refs
                    .iter()
                    .map(serde_json::Value::to_string)
                    .collect::<std::collections::BTreeSet<_>>();
                assert_eq!(
                    changed.len(),
                    changed_refs.len(),
                    "{} event {event_id} publishes duplicate changed identities",
                    algorithm.as_str()
                );
                assert!(
                    boundary_by_event
                        .insert(event_id.to_owned(), boundary.to_owned())
                        .is_none(),
                    "{} emitted duplicate event identity {event_id}",
                    algorithm.as_str()
                );
                if let Some(parent) = event["parent_phase_id"].as_str() {
                    parent_links.push((event_id.to_owned(), parent.to_owned()));
                }
            }
        }
        assert_eq!(
            certify_boundaries,
            1,
            "{} must expose exactly one terminal certification role",
            algorithm.as_str()
        );
        if matches!(
            algorithm,
            AlgorithmId::PrimalNetworkSimplex | AlgorithmId::DynamicTreeNetworkSimplex
        ) {
            let pivot_work = trace_timeline
                .iter()
                .map(|frame| {
                    serde_json::from_str::<serde_json::Value>(frame)
                        .expect("network simplex trace frame is JSON")
                })
                .flat_map(|frame| {
                    frame["trace_event_semantics"]["work_deltas"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                })
                .filter(|delta| delta["unit"] == "simplex-pivot")
                .map(|delta| {
                    delta["count"]
                        .as_str()
                        .expect("pivot work count")
                        .parse::<u128>()
                        .expect("canonical pivot work count")
                })
                .sum::<u128>();
            let final_pivots = trace_value["metrics"][3]
                .as_str()
                .expect("network simplex pivot metric")
                .parse::<u128>()
                .expect("canonical network simplex pivot metric");
            assert_eq!(
                pivot_work,
                final_pivots,
                "{} must publish exactly one simplex-pivot work unit per completed pivot",
                algorithm.as_str()
            );
        }
        for (event_id, parent_id) in parent_links {
            assert_eq!(
                boundary_by_event.get(&parent_id).map(String::as_str),
                Some("phase"),
                "{} event {event_id} refers to non-phase parent {parent_id}",
                algorithm.as_str()
            );
        }
        if std::env::var_os("FLOW_STEP_AUDIT").is_some() {
            println!(
                "FLOW_STEP_AUDIT\t{}\t{}\t{}\t{}\t{}\t{}",
                algorithm.as_str(),
                event_count,
                phase_boundaries,
                operation_boundaries,
                detailed_boundaries,
                catalog_ids.into_iter().collect::<Vec<_>>().join(",")
            );
        }
        match descriptor.trace_steps.phase_availability {
            flow::AlgorithmStepAvailabilityV1::Available => {
                if phase_boundaries == 0 {
                    violations.push(format!(
                        "{} declares Phase playback but its canonical fixture emits no phase boundary",
                        algorithm.as_str()
                    ));
                }
            }
            flow::AlgorithmStepAvailabilityV1::Unavailable { .. } => assert_eq!(
                phase_boundaries,
                0,
                "{} emits phase boundaries while its descriptor disables Phase playback",
                algorithm.as_str()
            ),
        }
        match descriptor.trace_steps.operation_availability {
            flow::AlgorithmStepAvailabilityV1::Available => {
                if operation_boundaries == 0 {
                    violations.push(format!(
                        "{} declares Operation playback but its canonical fixture emits no operation boundary",
                        algorithm.as_str()
                    ));
                }
            }
            flow::AlgorithmStepAvailabilityV1::Unavailable { .. } => assert_eq!(
                operation_boundaries,
                0,
                "{} emits operation boundaries while its descriptor disables Operation playback",
                algorithm.as_str()
            ),
        }
        match descriptor.trace_steps.detail {
            flow::AlgorithmDetailStepV1::Available { .. } => {
                if detailed_boundaries == 0 {
                    violations.push(format!(
                        "{} declares Detailed playback but its canonical fixture emits no detail boundary",
                        algorithm.as_str()
                    ));
                }
            }
            flow::AlgorithmDetailStepV1::Unavailable { .. } => assert_eq!(
                detailed_boundaries,
                0,
                "{} emits detail boundaries while its descriptor disables Detailed playback",
                algorithm.as_str()
            ),
        }
        assert_ne!(trace_value["solve_status"], "ready");
        assert_ne!(trace_value["solve_status"], "resource-limit");

        let (_fast_base, fast_final, fast_event_count, _, _) = run_to_end(algorithm, "fast", false);
        assert_eq!(
            fast_event_count,
            1,
            "{} fast execution must publish one atomic result boundary",
            algorithm.as_str()
        );
        let fast_value: serde_json::Value =
            serde_json::from_str(&fast_final).expect("fast final is JSON");
        let expected_feasibility_work =
            aggregate_trace_feasibility_work(algorithm, &trace_timeline);
        if fast_value.get("feasibility_work") != expected_feasibility_work.as_ref() {
            violations.push(format!(
                "{} Fast feasibility summary differs from its exact Trace source work: actual={:?}, expected={expected_feasibility_work:?}",
                algorithm.as_str(),
                fast_value.get("feasibility_work")
            ));
        }
        let normalized_fast = normalized_final_frame(&fast_final);
        let normalized_trace = normalized_final_frame(&trace_final);
        if normalized_fast != normalized_trace {
            violations.push(format!(
                "{} fast and trace contract-relevant final snapshots differ in {:?}",
                algorithm.as_str(),
                differing_top_level_fields(&normalized_fast, &normalized_trace)
            ));
        }
        verify_final_with_independent_checker(
            algorithm,
            &conformance_scenario(algorithm, "trace"),
            &fast_final,
            &trace_timeline,
        );
        let (fresh_base, fresh_final, fresh_event_count, fresh_first_event, fresh_timeline) =
            run_to_end(algorithm, "trace", true);
        assert_eq!(fresh_base, base, "{} base drifted", algorithm.as_str());
        assert_eq!(
            fresh_event_count,
            event_count,
            "{} event count drifted",
            algorithm.as_str()
        );
        assert_eq!(
            fresh_first_event,
            first_event,
            "{} first event drifted",
            algorithm.as_str()
        );
        assert_eq!(
            fresh_timeline,
            trace_timeline,
            "{} fresh-session trace drifted",
            algorithm.as_str()
        );
        assert_eq!(
            fresh_final,
            trace_final,
            "{} final trace frame drifted",
            algorithm.as_str()
        );
    }
    assert!(
        violations.is_empty(),
        "algorithm conformance violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn capacity_scaling_mcf_fast_and_trace_contract_projection_match() {
    let (_, trace_final, _, _, _) = run_to_end(AlgorithmId::CapacityScalingMcf, "trace", false);
    let (_, fast_final, _, _, _) = run_to_end(AlgorithmId::CapacityScalingMcf, "fast", false);
    assert_eq!(
        normalized_final_frame(&fast_final),
        normalized_final_frame(&trace_final)
    );
}

#[test]
fn parametric_pseudoflow_accepts_its_declared_edge_boundary() {
    let algorithm = AlgorithmId::ParametricPseudoflow;
    let descriptor = find_algorithm_by_id(algorithm).expect("catalog descriptor");
    let maximum = usize::try_from(descriptor.initial_band.max_edges).expect("edge band");
    let positive = conformance_scenario(algorithm, "fast");
    let accepted = scenario_with_edge_count(&positive, maximum);
    let scenario = decode_flow_scenario(accepted.as_bytes()).expect("boundary Scenario");
    let graph = scenario.canonical_network().expect("boundary graph");
    scenario
        .parametric_problem(&graph)
        .expect("boundary parametric problem");
    FlowSession::new(&accepted).expect("boundary session");
    let rejected = scenario_with_edge_count(&positive, maximum + 1);
    let mut rejected_session = FlowSession::new(&rejected).expect("one-past session");
    let limited = rejected_session
        .stage_next_json()
        .expect("one-past stages")
        .expect("one-past resource boundary");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&limited).expect("resource JSON")["solve_status"],
        "resource-limit"
    );
}

#[test]
fn structured_models_accept_their_declared_edge_boundaries() {
    for algorithm in [
        AlgorithmId::HopcroftKarp,
        AlgorithmId::Hungarian,
        AlgorithmId::Auction,
        AlgorithmId::TransportationSimplex,
        AlgorithmId::Modi,
        AlgorithmId::HassinStPlanar,
        AlgorithmId::BorradaileKleinPlanar,
        AlgorithmId::SegmentExpandedConvexMcf,
        AlgorithmId::ConvexCostScaling,
        AlgorithmId::ConvexNetworkSimplex,
    ] {
        let descriptor = find_algorithm_by_id(algorithm).expect("catalog descriptor");
        let maximum = usize::try_from(descriptor.initial_band.max_edges).expect("edge band");
        let positive = conformance_scenario(algorithm, "fast");
        let accepted = scenario_with_edge_count(&positive, maximum);
        decode_flow_scenario(accepted.as_bytes()).unwrap_or_else(|error| {
            panic!(
                "{} maximum boundary failed Scenario validation: {error}",
                algorithm.as_str()
            )
        });
        FlowSession::new(&accepted).unwrap_or_else(|_| {
            panic!(
                "{} rejected its structured maximum edge boundary",
                algorithm.as_str()
            )
        });
        let one_past = scenario_with_edge_count(&positive, maximum + 1);
        let mut limited = FlowSession::new(&one_past).unwrap_or_else(|_| {
            panic!(
                "{} rejected one-past before resource publication",
                algorithm.as_str()
            )
        });
        let frame = limited
            .stage_next_json()
            .expect("resource boundary stages")
            .expect("resource boundary exists");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&frame).expect("resource JSON")["solve_status"],
            "resource-limit"
        );
    }
}

#[test]
fn resource_admission_never_masks_model_config_or_profile_errors() {
    let oversized = |algorithm| {
        let source = conformance_scenario(algorithm, "fast");
        let descriptor = find_algorithm_by_id(algorithm).expect("catalog descriptor");
        scenario_with_edge_count(
            &source,
            usize::try_from(descriptor.initial_band.max_edges).expect("edge band") + 1,
        )
    };

    let mut invalid_model: serde_json::Value =
        serde_json::from_str(&oversized(AlgorithmId::SimpleCycleCanceling))
            .expect("model fixture JSON");
    invalid_model["payload"]["model"] = serde_json::json!({
        "kind": "max-flow",
        "source": "s",
        "sink": "t"
    });
    let invalid_model_source = invalid_model.to_string();
    assert_eq!(
        validate_flow_session_input(&invalid_model_source)
            .expect_err("unsupported model must fail before resource admission"),
        "selected flow algorithm does not support the requested problem model",
        "session input validation published resource-limit for an unsupported model"
    );
    let invalid_model =
        decode_flow_scenario(invalid_model_source.as_bytes()).expect("invalid model Scenario");
    let invalid_model_graph = invalid_model
        .canonical_network()
        .expect("invalid model graph");
    assert!(
        validate_model_contract(
            &invalid_model,
            find_algorithm_by_id(AlgorithmId::SimpleCycleCanceling).expect("model descriptor"),
        )
        .is_err(),
        "one-past graph masked an unsupported model"
    );
    assert!(
        invalid_model_graph.edges().len()
            > usize::try_from(
                find_algorithm_by_id(AlgorithmId::SimpleCycleCanceling)
                    .expect("model descriptor")
                    .initial_band
                    .max_edges,
            )
            .expect("model edge band"),
        "model fixture is not actually one-past"
    );

    let mut invalid_config: serde_json::Value =
        serde_json::from_str(&oversized(AlgorithmId::EdmondsKarp)).expect("config fixture JSON");
    invalid_config["payload"]["algorithm"]["config"] = serde_json::json!({ "unexpected": true });
    let invalid_config_source = invalid_config.to_string();
    assert_eq!(
        validate_flow_session_input(&invalid_config_source)
            .expect_err("unsupported config must fail before resource admission"),
        "Phase-2 flow solvers require an empty algorithm config",
        "session input validation published resource-limit for an unsupported config"
    );
    let invalid_config =
        decode_flow_scenario(invalid_config_source.as_bytes()).expect("invalid config Scenario");
    let invalid_config_graph = invalid_config
        .canonical_network()
        .expect("invalid config graph");
    assert!(
        validate_execution_contract(
            &invalid_config,
            &invalid_config_graph,
            AlgorithmId::EdmondsKarp,
        )
        .is_err(),
        "one-past graph masked a nonempty unsupported config"
    );

    let mut invalid_profile: serde_json::Value =
        serde_json::from_str(&oversized(AlgorithmId::EdmondsKarp)).expect("profile fixture JSON");
    invalid_profile["payload"]["run_profile"] = serde_json::json!("cpu-parallel");
    let invalid_profile_source = invalid_profile.to_string();
    assert_eq!(
        validate_flow_session_input(&invalid_profile_source)
            .expect_err("unsupported profile must fail before resource admission"),
        "CPU-parallel flow execution is not available in this phase",
        "session input validation published resource-limit for an unsupported profile"
    );
    let invalid_profile =
        decode_flow_scenario(invalid_profile_source.as_bytes()).expect("invalid profile Scenario");
    let invalid_profile_graph = invalid_profile
        .canonical_network()
        .expect("invalid profile graph");
    assert!(
        validate_execution_contract(
            &invalid_profile,
            &invalid_profile_graph,
            AlgorithmId::EdmondsKarp,
        )
        .is_err(),
        "one-past graph masked an unsupported CPU-parallel profile"
    );
}

fn assert_semantic_error_before_resource(scenario: &serde_json::Value, expected_message: &str) {
    let source = scenario.to_string();
    let error = validate_flow_session_input(&source)
        .expect_err("malformed oversized Scenario must fail before resource publication");
    assert!(
        error.contains(expected_message),
        "expected {expected_message:?} in {error:?}"
    );
}

fn one_past_edge_value(algorithm: AlgorithmId) -> serde_json::Value {
    let descriptor = find_algorithm_by_id(algorithm).expect("catalog descriptor");
    let maximum = usize::try_from(descriptor.initial_band.max_edges).expect("edge band");
    serde_json::from_str(&scenario_with_edge_count(
        &conformance_scenario(algorithm, "fast"),
        maximum + 1,
    ))
    .expect("one-past edge fixture is JSON")
}

#[test]
fn oversized_graphs_do_not_mask_exact_graph_requirements() {
    let mut unit_capacity = one_past_edge_value(AlgorithmId::UnitCapacityDinic);
    unit_capacity["payload"]["graph"]["edges"][0]["capacity"] = serde_json::json!("2");
    assert_semantic_error_before_resource(&unit_capacity, "unit capacities");

    let mut unit_network = one_past_edge_value(AlgorithmId::UnitNetworkDinic);
    unit_network["payload"]["graph"]["nodes"]
        .as_array_mut()
        .expect("unit-network nodes")
        .push(serde_json::json!({ "id": "zz-bad" }));
    for index in 0..2 {
        unit_network["payload"]["graph"]["edges"][index]["from"] = serde_json::json!("s");
        unit_network["payload"]["graph"]["edges"][index]["to"] = serde_json::json!("zz-bad");
    }
    assert_semantic_error_before_resource(&unit_network, "unit network");

    let mut strongly_connected = one_past_edge_value(AlgorithmId::EnhancedCapacityScaling);
    strongly_connected["payload"]["graph"]["nodes"]
        .as_array_mut()
        .expect("transshipment nodes")
        .push(serde_json::json!({ "id": "zz-isolated", "supply": "0" }));
    assert_semantic_error_before_resource(&strongly_connected, "strong connectivity");
}

#[test]
fn oversized_graphs_do_not_mask_structured_model_errors() {
    let mut matching = one_past_edge_value(AlgorithmId::HopcroftKarp);
    let duplicate_left = matching["payload"]["model"]["left"][0].clone();
    matching["payload"]["model"]["left"][1] = duplicate_left;
    assert_semantic_error_before_resource(&matching, "bipartite matching model");

    let mut assignment = one_past_edge_value(AlgorithmId::Hungarian);
    let duplicate_from = assignment["payload"]["graph"]["edges"][0]["from"].clone();
    let duplicate_to = assignment["payload"]["graph"]["edges"][0]["to"].clone();
    assignment["payload"]["graph"]["edges"][1]["from"] = duplicate_from;
    assignment["payload"]["graph"]["edges"][1]["to"] = duplicate_to;
    assert_semantic_error_before_resource(&assignment, "assignment model");

    let mut transportation = one_past_edge_value(AlgorithmId::TransportationSimplex);
    transportation["payload"]["graph"]["edges"][0]["capacity"] = serde_json::json!("1");
    assert_semantic_error_before_resource(&transportation, "transportation model");

    let mut planar = one_past_edge_value(AlgorithmId::HassinStPlanar);
    planar["payload"]["model"]["embedding"]["rotations"][0]["darts"]
        .as_array_mut()
        .expect("planar rotation")
        .pop();
    assert_semantic_error_before_resource(&planar, "planar embedding");

    let mut parametric = one_past_edge_value(AlgorithmId::ParametricPseudoflow);
    parametric["payload"]["model"]["capacity_slopes"]
        .as_array_mut()
        .expect("capacity slopes")
        .push(serde_json::json!({ "edge_id": "zz-missing-edge", "slope": "1" }));
    assert_semantic_error_before_resource(&parametric, "parametric max-flow model");

    let mut convex = one_past_edge_value(AlgorithmId::SegmentExpandedConvexMcf);
    convex["payload"]["graph"]["edges"][0]["convex_cost"] = serde_json::json!({
        "base_cost_at_zero": "0",
        "segments": [{ "end_flow": "2", "marginal_cost": "0" }]
    });
    assert_semantic_error_before_resource(&convex, "convex-cost flow model");
}

#[test]
fn oversized_graphs_fail_closed_for_global_preconditions() {
    let mut negative_cycle = one_past_edge_value(AlgorithmId::SuccessiveShortestPath);
    negative_cycle["payload"]["graph"]["edges"][0]["from"] = serde_json::json!("a");
    negative_cycle["payload"]["graph"]["edges"][0]["to"] = serde_json::json!("b");
    negative_cycle["payload"]["graph"]["edges"][0]["cost"] = serde_json::json!("-1");
    negative_cycle["payload"]["graph"]["edges"][1]["from"] = serde_json::json!("b");
    negative_cycle["payload"]["graph"]["edges"][1]["to"] = serde_json::json!("a");
    negative_cycle["payload"]["graph"]["edges"][1]["cost"] = serde_json::json!("-1");
    assert_semantic_error_before_resource(&negative_cycle, "negative-cost cycles");

    let descriptor = find_algorithm_by_id(AlgorithmId::DeterministicAlmostLinearMcf)
        .expect("strict-interior descriptor");
    let maximum = usize::try_from(descriptor.initial_band.max_nodes).expect("node band");
    let source = conformance_scenario(AlgorithmId::DeterministicAlmostLinearMcf, "fast");
    let mut no_interior: serde_json::Value =
        serde_json::from_str(&strict_interior_node_boundary_scenario(
            serde_json::from_str(&source).expect("strict source"),
            maximum + 1,
        ))
        .expect("strict one-past JSON");
    no_interior["payload"]["graph"]["edges"][0]["capacity"] = serde_json::json!("1");
    assert_semantic_error_before_resource(&no_interior, "strictly inside every edge bound");

    let beyond_margin: serde_json::Value =
        serde_json::from_str(&strict_interior_node_boundary_scenario(
            serde_json::from_str(&source).expect("strict source"),
            maximum + 2,
        ))
        .expect("strict beyond-margin JSON");
    assert_semantic_error_before_resource(&beyond_margin, "cannot be certified");
}

#[test]
fn transportation_requirement_respects_declared_isolated_destinations() {
    let mut value: serde_json::Value = serde_json::from_str(&conformance_scenario(
        AlgorithmId::TransportationSimplex,
        "fast",
    ))
    .expect("transportation conformance fixture is JSON");
    value["payload"]["generator_provenance"] = serde_json::Value::Null;
    value["payload"]["model"] = serde_json::json!({
        "kind": "transportation",
        "origins": ["o0"],
        "destinations": ["d0"]
    });
    value["payload"]["graph"] = serde_json::json!({
        "nodes": [
            { "id": "d0", "supply": "-1" },
            { "id": "o0", "supply": "1" }
        ],
        "edges": []
    });
    let source = value.to_string();
    let validated = validate_flow_session_input(&source)
        .expect("declared sparse transportation model is admitted before solving");
    assert!(!validated.resource_admission_limited);
    FlowSession::new(&source).expect("isolated destination reaches the native infeasibility path");
}

#[test]
fn max_flow_supplies_are_rejected_before_a_ready_session_exists() {
    let mut value: serde_json::Value =
        serde_json::from_str(&conformance_scenario(AlgorithmId::EdmondsKarp, "fast"))
            .expect("max-flow conformance fixture is JSON");
    value["payload"]["generator_provenance"] = serde_json::Value::Null;
    value["payload"]["graph"]["nodes"][0]["supply"] = serde_json::json!("1");
    value["payload"]["graph"]["nodes"][1]["supply"] = serde_json::json!("-1");
    let source = value.to_string();
    let error = validate_flow_session_input(&source)
        .expect_err("nonzero max-flow supplies must fail before ready publication");
    assert!(error.contains("terminal-flow models require zero node supplies"));
}

#[test]
fn unconsumed_initial_flow_is_rejected_before_a_ready_session_exists() {
    let mut value: serde_json::Value =
        serde_json::from_str(&conformance_scenario(AlgorithmId::EdmondsKarp, "fast"))
            .expect("max-flow conformance fixture is JSON");
    value["payload"]["generator_provenance"] = serde_json::Value::Null;
    value["payload"]["graph"]["edges"][0]["initial_flow"] = serde_json::json!("1");
    let source = value.to_string();
    let error = validate_flow_session_input(&source)
        .expect_err("an ignored non-lower initial flow must fail before ready publication");
    assert!(error.contains("does not consume a non-lower initial_flow declaration"));
}

#[test]
fn unbalanced_cost_flow_models_are_rejected_before_ready_publication() {
    for algorithm in [
        AlgorithmId::PotentialDijkstraSsp,
        AlgorithmId::ArcFixing,
        AlgorithmId::EnhancedCapacityScaling,
        AlgorithmId::SegmentExpandedConvexMcf,
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&conformance_scenario(algorithm, "fast"))
                .expect("cost-flow conformance fixture is JSON");
        value["payload"]["generator_provenance"] = serde_json::Value::Null;
        let supply = value["payload"]["graph"]["nodes"][0]["supply"]
            .as_str()
            .unwrap_or("0")
            .parse::<i64>()
            .expect("fixture supply is canonical");
        value["payload"]["graph"]["nodes"][0]["supply"] =
            serde_json::json!((supply + 1).to_string());
        let error = validate_flow_session_input(&value.to_string())
            .expect_err("an unbalanced cost-flow model must fail before ready publication");
        assert_eq!(
            error,
            "invalid flow Scenario value: balance-flow models require zero total node supply",
            "{}",
            algorithm.as_str()
        );
    }
}

#[test]
fn every_algorithm_enforces_its_exact_catalog_edge_admission_boundary_atomically() {
    for algorithm in AlgorithmId::ALL.iter().copied() {
        let descriptor = find_algorithm_by_id(algorithm).expect("catalog descriptor");
        let maximum = usize::try_from(descriptor.initial_band.max_edges)
            .expect("catalog edge band fits usize");
        let positive = conformance_scenario(algorithm, "fast");

        let accepted = scenario_with_edge_count(&positive, maximum);
        validate_flow_session_input(&accepted).unwrap_or_else(|error| {
            panic!(
                "{} boundary fixture violates its declared preconditions: {error}",
                algorithm.as_str()
            )
        });
        let accepted_session = FlowSession::new(&accepted).unwrap_or_else(|_| {
            panic!(
                "{} rejected its declared maximum edge admission boundary",
                algorithm.as_str()
            )
        });
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                &accepted_session
                    .current_frame_json()
                    .expect("accepted boundary serializes")
            )
            .expect("accepted boundary is JSON")["solve_status"],
            "ready",
            "{} did not retain a ready scene at the declared maximum",
            algorithm.as_str()
        );

        let rejected = scenario_with_edge_count(&positive, maximum + 1);
        validate_flow_session_input(&rejected).unwrap_or_else(|error| {
            panic!(
                "{} one-past fixture violates a precondition before resource admission: {error}",
                algorithm.as_str()
            )
        });
        let mut rejected_session = FlowSession::new(&rejected).unwrap_or_else(|_| {
            panic!(
                "{} could not stage its one-past boundary",
                algorithm.as_str()
            )
        });
        let ready = rejected_session
            .current_frame_json()
            .expect("one-past ready boundary serializes");
        let limited = rejected_session
            .stage_next_json()
            .unwrap_or_else(|_| {
                panic!(
                    "{} failed to publish its one-past resource result",
                    algorithm.as_str()
                )
            })
            .unwrap_or_else(|| panic!("{} lost its one-past result", algorithm.as_str()));
        let limited_value: serde_json::Value =
            serde_json::from_str(&limited).expect("resource boundary is JSON");
        assert_eq!(
            limited_value["solve_status"],
            "resource-limit",
            "{} did not reject one edge past its catalog band",
            algorithm.as_str()
        );
        assert_eq!(
            rejected_session
                .current_frame_json()
                .expect("unacknowledged resource boundary retains ready state"),
            ready,
            "{} changed committed state before resource-limit ACK",
            algorithm.as_str()
        );
        rejected_session.discard_staged_next();
        assert_eq!(
            rejected_session
                .stage_next_json()
                .expect("discarded resource boundary restages")
                .expect("discarded resource boundary is repeatable"),
            limited,
            "{} resource-limit result is not deterministic after discard",
            algorithm.as_str()
        );
    }
}

#[test]
fn every_algorithm_enforces_its_exact_catalog_node_admission_boundary_atomically() {
    for algorithm in AlgorithmId::ALL.iter().copied() {
        let descriptor = find_algorithm_by_id(algorithm).expect("catalog descriptor");
        let maximum = usize::try_from(descriptor.initial_band.max_nodes)
            .expect("catalog node band fits usize");
        let positive = conformance_scenario(algorithm, "fast");

        let accepted = scenario_with_node_count(&positive, maximum);
        validate_flow_session_input(&accepted).unwrap_or_else(|error| {
            panic!(
                "{} boundary fixture violates its declared preconditions: {error}",
                algorithm.as_str()
            )
        });
        FlowSession::new(&accepted).unwrap_or_else(|_| {
            panic!(
                "{} rejected its declared maximum node admission boundary",
                algorithm.as_str()
            )
        });

        let rejected = scenario_with_node_count(&positive, maximum + 1);
        validate_flow_session_input(&rejected).unwrap_or_else(|error| {
            panic!(
                "{} one-past fixture violates a precondition before resource admission: {error}",
                algorithm.as_str()
            )
        });
        let mut rejected_session = FlowSession::new(&rejected).unwrap_or_else(|_| {
            panic!(
                "{} could not stage its one-past node boundary",
                algorithm.as_str()
            )
        });
        let ready = rejected_session
            .current_frame_json()
            .expect("one-past node ready boundary serializes");
        let limited = rejected_session
            .stage_next_json()
            .unwrap_or_else(|_| {
                panic!(
                    "{} failed to publish its one-past node resource result",
                    algorithm.as_str()
                )
            })
            .unwrap_or_else(|| panic!("{} lost its node resource result", algorithm.as_str()));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&limited).expect("node resource JSON")["solve_status"],
            "resource-limit",
            "{} did not reject one node past its catalog band",
            algorithm.as_str()
        );
        assert_eq!(
            rejected_session
                .current_frame_json()
                .expect("unacknowledged node limit retains ready state"),
            ready,
            "{} changed committed state before node resource-limit ACK",
            algorithm.as_str()
        );
        rejected_session.discard_staged_next();
        assert_eq!(
            rejected_session
                .stage_next_json()
                .expect("discarded node limit restages")
                .expect("discarded node limit is repeatable"),
            limited,
            "{} node resource-limit result is not deterministic after discard",
            algorithm.as_str()
        );
    }
}

#[test]
fn every_algorithm_handles_its_safe_wide_or_source_bounded_numeric_envelope_atomically() {
    for algorithm in AlgorithmId::ALL.iter().copied() {
        let source = arithmetic_boundary_scenario(algorithm);
        let mut session = FlowSession::new(&source).unwrap_or_else(|_| {
            panic!(
                "{} rejected its numeric boundary before a ready scene",
                algorithm.as_str()
            )
        });
        let ready = session
            .current_frame_json()
            .expect("numeric boundary ready scene serializes");
        match session
            .stage_next_json()
            .unwrap_or_else(|_| panic!("{} failed safe wide arithmetic", algorithm.as_str()))
        {
            Some(candidate) => {
                serde_json::from_str::<serde_json::Value>(&candidate)
                    .expect("numeric boundary candidate is JSON");
                assert_eq!(
                    session
                        .current_frame_json()
                        .expect("unacknowledged numeric candidate retains ready scene"),
                    ready,
                    "{} changed committed state before numeric-boundary ACK",
                    algorithm.as_str()
                );
                session.discard_staged_next();
                assert_eq!(
                    session
                        .stage_next_json()
                        .expect("numeric boundary restages")
                        .expect("numeric boundary candidate is repeatable"),
                    candidate,
                    "{} numeric boundary result is not deterministic after discard",
                    algorithm.as_str()
                );
            }
            None => panic!(
                "{} produced no public numeric-boundary result",
                algorithm.as_str()
            ),
        }
    }
}

#[test]
fn every_applicable_algorithm_fails_closed_at_its_numeric_admission_boundary() {
    let mut resource_scenes = 0_usize;
    let mut scenario_rejections = 0_usize;
    for algorithm in AlgorithmId::ALL.iter().copied() {
        let source = arithmetic_overflow_scenario(algorithm);
        let scenario = match decode_flow_scenario(source.as_bytes()) {
            Ok(scenario) => scenario,
            Err(error) => {
                let message = error.to_string();
                assert!(
                    message == "flow aggregate numeric bound exceeded"
                        || message == "invalid flow Scenario value: convex-cost flow model",
                    "{} escaped through an unrelated Scenario error",
                    algorithm.as_str()
                );
                scenario_rejections += 1;
                continue;
            }
        };
        let graph = scenario
            .canonical_network()
            .expect("overflow fixture canonical graph");
        let descriptor = flow::find_algorithm_by_id(algorithm).expect("catalog descriptor");
        if !kernel_resource_admission_limited(&scenario, &graph, descriptor) {
            continue;
        }
        resource_scenes += 1;
        let mut session = FlowSession::new(&source).unwrap_or_else(|_| {
            panic!(
                "{} rejected overflow before resource publication",
                algorithm.as_str()
            )
        });
        let ready = session
            .current_frame_json()
            .expect("overflow ready scene serializes");
        let limited = session
            .stage_next_json()
            .unwrap_or_else(|_| panic!("{} overflow did not stage", algorithm.as_str()))
            .unwrap_or_else(|| panic!("{} overflow lost its resource result", algorithm.as_str()));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&limited).expect("overflow scene JSON")["solve_status"],
            "resource-limit",
            "{} did not publish checked numeric overflow as a resource limit",
            algorithm.as_str()
        );
        assert_eq!(
            session
                .current_frame_json()
                .expect("unacknowledged overflow retains ready scene"),
            ready,
            "{} changed committed state before overflow ACK",
            algorithm.as_str()
        );
        session.discard_staged_next();
        assert_eq!(
            session
                .stage_next_json()
                .expect("discarded overflow restages")
                .expect("discarded overflow is repeatable"),
            limited,
            "{} overflow resource result is not deterministic",
            algorithm.as_str()
        );
    }
    assert!(
        resource_scenes + scenario_rejections >= 50,
        "numeric failure evidence unexpectedly covered only {} descriptors",
        resource_scenes + scenario_rejections
    );
    assert!(
        resource_scenes > 0,
        "no numeric resource scene was exercised"
    );
    assert!(
        scenario_rejections > 0,
        "no aggregate numeric Scenario rejection was exercised"
    );
}

#[derive(Clone, Debug)]
struct RepresentativeTraceAudit {
    label: String,
    node_count: usize,
    edge_count: usize,
    event_count: usize,
    distinct_actions: usize,
    granularity_counts: [usize; 3],
    primary_work: u128,
    primary_work_boundaries: u128,
    primary_work_unit: String,
    primary_work_abstraction: String,
    maximum_primary_work_delta: u128,
    first_detail: RepresentativeBoundaryWitness,
    middle_detail: RepresentativeBoundaryWitness,
    last_detail: RepresentativeBoundaryWitness,
    first_primary_work: RepresentativeBoundaryWitness,
    maximum_aggregation: RepresentativeBoundaryWitness,
    maximum_primary_work: RepresentativeBoundaryWitness,
    overlay_witnesses: std::collections::BTreeMap<String, RepresentativeBoundaryWitness>,
    primary_work_actions: std::collections::BTreeSet<String>,
    action_boundaries: std::collections::BTreeMap<String, String>,
    scenario_digest: String,
    trace_digest: String,
    control_contract: Option<String>,
    control_digest: Option<String>,
    source: String,
}

#[derive(Debug)]
struct RepresentativeComplexityGrowthWitness {
    driver: String,
    controlled_family: String,
    controlled: bool,
    control_contract: String,
    control_digest: String,
    smaller_driver: u128,
    larger_driver: u128,
    smaller_label: String,
    larger_label: String,
    smaller_primary_work: u128,
    larger_primary_work: u128,
    smaller_primary_boundaries: u128,
    larger_primary_boundaries: u128,
    smaller_event_count: usize,
    larger_event_count: usize,
    smaller_detail: usize,
    larger_detail: usize,
    smaller_nodes: usize,
    larger_nodes: usize,
    smaller_edges: usize,
    larger_edges: usize,
}

#[derive(Clone, Debug)]
struct RepresentativeBoundaryWitness {
    event: usize,
    catalog_id: String,
    primary_delta: u128,
    primary_completed: u128,
    detail_completed: u128,
    aggregation: u128,
    work_deltas: Vec<RepresentativeWorkDelta>,
    work_first: Option<u128>,
    work_last: Option<u128>,
    work_total: Option<u128>,
    active_overlays: Vec<String>,
    overlay_scalar_values:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    touched_identities: Vec<String>,
    changed_identities: Vec<String>,
}

#[derive(Clone, Debug)]
struct RepresentativeWorkDelta {
    unit: String,
    count: u128,
}

#[derive(Default)]
struct RepresentativeBoundaryAuditState {
    actions: std::collections::BTreeSet<String>,
    granularity_counts: [usize; 3],
    published_transitions: u128,
    detail_primitives: u128,
    primary_work: u128,
    primary_work_boundaries: u128,
    declared_detail_total: Option<u128>,
    declared_primary_total: Option<u128>,
    first_detail: Option<RepresentativeBoundaryWitness>,
    first_primary_work: Option<RepresentativeBoundaryWitness>,
    detail_witnesses: Vec<RepresentativeBoundaryWitness>,
    previous_detail_signature: Option<String>,
    previous_detail_catalog_id: Option<String>,
    maximum_aggregation: Option<RepresentativeBoundaryWitness>,
    maximum_primary_work: Option<RepresentativeBoundaryWitness>,
    overlay_witnesses: std::collections::BTreeMap<String, RepresentativeBoundaryWitness>,
    primary_work_actions: std::collections::BTreeSet<String>,
    action_boundaries: std::collections::BTreeMap<String, String>,
}

fn digest_hex(bytes: &[u8]) -> String {
    encode_digest(&Sha256::digest(bytes))
}

fn representative_control_contract(label: &str) -> Option<&'static str> {
    [
        ("controlled-size-", "node-padding-v1"),
        ("transportation-matrix-", "transportation-matrix-size-v1"),
        ("ipm-path-", "ipm-path-length-v1"),
        ("electrical-ipm-capacity-", "electrical-ipm-capacity-v1"),
        ("bounded-face-extra-", "bounded-face-parallel-edge-v1"),
        ("electrical-structure-", "electrical-edge-count-v1"),
        ("binary-zero-scc-", "binary-zero-scc-size-v1"),
        ("unit-network-", "unit-network-path-count-v1"),
        ("parallel-cost-paths-", "parallel-cost-path-count-v1"),
        ("convex-parallel-", "convex-parallel-edge-count-v1"),
        ("convex-cost-", "convex-marginal-cost-scale-v1"),
        ("convex-scaling-path-", "convex-scaling-path-length-v1"),
        ("source-contract-", "source-contract-declared-driver-v1"),
    ]
    .into_iter()
    .find_map(|(prefix, contract)| label.starts_with(prefix).then_some(contract))
}

fn representative_control_provenance(
    algorithm: AlgorithmId,
    label: &str,
    source: &str,
    canonical: &str,
) -> Result<(Option<String>, Option<String>), String> {
    let Some(contract) = representative_control_contract(label) else {
        return Ok((None, None));
    };
    let parse_driver = |prefix: &str, suffix: &str| -> Result<usize, String> {
        label
            .strip_prefix(prefix)
            .and_then(|value| value.strip_suffix(suffix))
            .ok_or_else(|| format!("{label} violates its {contract} label grammar"))?
            .parse::<usize>()
            .map_err(|error| format!("{label} scale driver: {error}"))
    };
    let expected = match contract {
        "node-padding-v1" => {
            scenario_with_node_count(canonical, parse_driver("controlled-size-n", "")?)
        }
        "transportation-matrix-size-v1" => representative_transportation_matrix_variant(
            canonical,
            parse_driver("transportation-matrix-", "")?,
        ),
        "ipm-path-length-v1" => {
            representative_primal_dual_ipm_path_variant(canonical, parse_driver("ipm-path-", "")?)
        }
        "electrical-ipm-capacity-v1" => representative_electrical_ipm_capacity_variant(
            canonical,
            parse_driver("electrical-ipm-capacity-", "")?,
        ),
        "bounded-face-parallel-edge-v1" => representative_bounded_mcf_face_variant(
            canonical,
            algorithm,
            parse_driver("bounded-face-extra-", "")?,
        ),
        "electrical-edge-count-v1" => representative_augmenting_electrical_structure_variant(
            canonical,
            parse_driver("electrical-structure-", "")?,
        ),
        "binary-zero-scc-size-v1" => {
            representative_binary_zero_scc_variant(canonical, parse_driver("binary-zero-scc-", "")?)
        }
        "unit-network-path-count-v1" => representative_unit_network_variant(
            canonical,
            algorithm,
            parse_driver("unit-network-", "-paths")?,
        ),
        "parallel-cost-path-count-v1" => {
            representative_ssap_scale_variant(canonical, parse_driver("parallel-cost-paths-", "")?)
        }
        "convex-parallel-edge-count-v1" => representative_convex_network_simplex_parallel_variant(
            canonical,
            parse_driver("convex-parallel-", "")?,
        ),
        "convex-marginal-cost-scale-v1" => representative_convex_cost_variant(
            canonical,
            i128::try_from(parse_driver("convex-cost-x", "")?)
                .map_err(|_| format!("{label} scale driver exceeds i128"))?,
        ),
        "convex-scaling-path-length-v1" => representative_convex_cost_scaling_path_variant(
            canonical,
            parse_driver("convex-scaling-path-", "")?,
        ),
        "source-contract-declared-driver-v1" => representative_source_specific_variant(
            canonical,
            algorithm,
            parse_driver("source-contract-variant-", "")?,
        ),
        _ => return Err(format!("{label} uses unknown control contract {contract}")),
    };
    let expected: serde_json::Value = serde_json::from_str(&expected)
        .map_err(|error| format!("{label} expected controlled Scenario: {error}"))?;
    let actual: serde_json::Value = serde_json::from_str(source)
        .map_err(|error| format!("{label} controlled Scenario: {error}"))?;
    if actual != expected {
        return Err(format!(
            "{label} differs from the exact {contract} scale transform"
        ));
    }
    let canonical: serde_json::Value = serde_json::from_str(canonical)
        .map_err(|error| format!("{label} canonical control Scenario: {error}"))?;
    let digest = digest_hex(format!("{}:{contract}:{canonical}", algorithm.as_str()).as_bytes());
    Ok((Some(contract.to_owned()), Some(digest)))
}

fn encode_digest(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String is infallible");
    }
    encoded
}

fn representative_numeric_variant(source: &str, variant: usize) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("representative fixture is JSON");
    let edges = scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("representative fixture edges");
    let edge_index = variant.saturating_sub(1) % edges.len();
    let edge = edges[edge_index]
        .as_object_mut()
        .expect("representative edge object");
    let field = if variant.is_multiple_of(2) {
        "cost"
    } else {
        "capacity"
    };
    let current = edge
        .get(field)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("0")
        .parse::<i128>()
        .expect("representative numeric field");
    let adjustment = if field == "cost" && current < 0 {
        -1
    } else {
        1
    };
    edge.insert(
        field.to_owned(),
        serde_json::json!((current + adjustment).to_string()),
    );
    scenario.to_string()
}

fn representative_unit_network_variant(
    source: &str,
    algorithm: AlgorithmId,
    path_count: usize,
) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("unit-network representative is JSON");
    let internal_nodes = (0..path_count)
        .map(|index| format!("u{index}"))
        .collect::<Vec<_>>();
    let nodes = std::iter::once(serde_json::json!({ "id": "s" }))
        .chain(
            internal_nodes
                .iter()
                .map(|node| serde_json::json!({ "id": node })),
        )
        .chain(std::iter::once(serde_json::json!({ "id": "t" })))
        .collect::<Vec<_>>();
    let edges = internal_nodes
        .iter()
        .enumerate()
        .flat_map(|(index, node)| {
            [
                serde_json::json!({
                    "id": format!("in-{index}"),
                    "from": "s",
                    "to": node,
                    "capacity": "1",
                    "cost": "0"
                }),
                serde_json::json!({
                    "id": format!("out-{index}"),
                    "from": node,
                    "to": "t",
                    "capacity": "1",
                    "cost": "0"
                }),
            ]
        })
        .collect::<Vec<_>>();
    scenario["payload"]["model"] =
        serde_json::json!({ "kind": "max-flow", "source": "s", "sink": "t" });
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario["payload"]["algorithm"]["id"] = serde_json::json!(algorithm.as_str());
    scenario.to_string()
}

fn representative_parametric_variant(source: &str, variant: usize) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("parametric representative is JSON");
    if variant == 1 {
        scenario["payload"]["model"]["parameter"]["maximum"]["numerator"] = serde_json::json!("3");
    } else {
        scenario["payload"]["model"]["capacity_slopes"][0]["slope"] = serde_json::json!("3");
    }
    scenario.to_string()
}

fn representative_convex_cost_variant(source: &str, factor: i128) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("convex representative is JSON");
    for edge in scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("convex representative edges")
    {
        if let Some(cost) = edge.get_mut("cost") {
            let scaled = cost
                .as_str()
                .expect("convex edge cost")
                .parse::<i128>()
                .expect("convex edge cost integer")
                * factor;
            *cost = serde_json::json!(scaled.to_string());
        }
        if let Some(segments) = edge
            .get_mut("convex_cost")
            .and_then(|cost| cost.get_mut("segments"))
            .and_then(serde_json::Value::as_array_mut)
        {
            for segment in segments {
                let scaled = segment["marginal_cost"]
                    .as_str()
                    .expect("convex marginal cost")
                    .parse::<i128>()
                    .expect("convex marginal integer")
                    * factor;
                segment["marginal_cost"] = serde_json::json!(scaled.to_string());
            }
        }
    }
    scenario.to_string()
}

fn representative_convex_network_simplex_parallel_variant(
    source: &str,
    edge_count: usize,
) -> String {
    assert!(edge_count >= 3);
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("convex simplex representative is JSON");
    let required = edge_count * 2 / 3;
    let nodes = serde_json::json!([
        { "id": "s", "supply": required.to_string() },
        { "id": "t", "supply": format!("-{required}") }
    ]);
    let edges = (0..edge_count)
        .map(|index| {
            serde_json::json!({
                "id": format!("parallel-{index:02}"),
                "from": "s",
                "to": "t",
                "capacity": "1",
                "cost": "0",
                "convex_cost": {
                    "base_cost_at_zero": "0",
                    "segments": [{
                        "end_flow": "1",
                        "marginal_cost": (index + 1).to_string()
                    }]
                }
            })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

/// A readable, deliberately nontrivial input for kernels whose tiny canonical
/// example does not exercise enough of their declared primary work.  These
/// fixtures enlarge graph structure—not just numeric magnitudes—so Step count
/// remains an honest witness for the algorithm's asymptotic inner operation.
#[allow(
    clippy::too_many_lines,
    reason = "one closed test-fixture table keeps algorithm-specific graph shapes easy to audit"
)]
fn representative_work_rich_variant(source: &str, algorithm: AlgorithmId) -> Option<String> {
    let mut scenario: serde_json::Value = serde_json::from_str(source).ok()?;
    match algorithm {
        AlgorithmId::HassinStPlanar => {
            return Some(planar_boundary_scenario(scenario, 12));
        }
        AlgorithmId::Hungarian => {
            let agents = (0..4).map(|index| format!("a{index}")).collect::<Vec<_>>();
            let tasks = (0..5).map(|index| format!("t{index}")).collect::<Vec<_>>();
            let nodes = agents
                .iter()
                .chain(&tasks)
                .map(|id| serde_json::json!({ "id": id }))
                .collect::<Vec<_>>();
            let edges = agents
                .iter()
                .enumerate()
                .flat_map(|(agent_index, from)| {
                    tasks.iter().enumerate().map(move |(task_index, to)| {
                        serde_json::json!({
                            "id": format!("assignment-{agent_index}-{task_index}"),
                            "from": from,
                            "to": to,
                            "capacity": "1",
                            "cost": (1 + (agent_index * 7 + task_index * 3) % 17).to_string()
                        })
                    })
                })
                .collect::<Vec<_>>();
            scenario["payload"]["model"] = serde_json::json!({
                "kind": "assignment", "agents": agents, "tasks": tasks, "objective": "minimize"
            });
            scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
        }
        AlgorithmId::AugmentingElectricalFlow => {
            scenario["payload"]["graph"] = serde_json::json!({
                "nodes": [
                    { "id": "s" }, { "id": "a" }, { "id": "b" },
                    { "id": "c" }, { "id": "t" }
                ],
                "edges": [
                    { "id": "sa", "from": "s", "to": "a", "capacity": "8", "cost": "0" },
                    { "id": "at", "from": "a", "to": "t", "capacity": "8", "cost": "0" },
                    { "id": "sb", "from": "s", "to": "b", "capacity": "4", "cost": "0" },
                    { "id": "bt", "from": "b", "to": "t", "capacity": "4", "cost": "0" },
                    { "id": "sc", "from": "s", "to": "c", "capacity": "1", "cost": "0" },
                    { "id": "ct", "from": "c", "to": "t", "capacity": "1", "cost": "0" }
                ]
            });
        }
        AlgorithmId::WeightedAugmentingPaths => {
            scenario["payload"]["graph"] = serde_json::json!({
                "nodes": [
                    { "id": "s" }, { "id": "a" }, { "id": "b" },
                    { "id": "c" }, { "id": "t" }
                ],
                "edges": [
                    { "id": "sa", "from": "s", "to": "a", "capacity": "7", "cost": "0" },
                    { "id": "sb", "from": "s", "to": "b", "capacity": "4", "cost": "0" },
                    { "id": "ab", "from": "a", "to": "b", "capacity": "7", "cost": "0" },
                    { "id": "ba", "from": "b", "to": "a", "capacity": "7", "cost": "0" },
                    { "id": "ac", "from": "a", "to": "c", "capacity": "4", "cost": "0" },
                    { "id": "bc", "from": "b", "to": "c", "capacity": "6", "cost": "0" },
                    { "id": "ct", "from": "c", "to": "t", "capacity": "10", "cost": "0" }
                ]
            });
        }
        AlgorithmId::DynamicEibfs => {
            scenario["payload"]["graph"]["nodes"]
                .as_array_mut()?
                .push(serde_json::json!({ "id": "e" }));
            scenario["payload"]["graph"]["edges"]
                .as_array_mut()?
                .extend([
                    serde_json::json!({ "id": "ae", "from": "a", "to": "e", "capacity": "2" }),
                    serde_json::json!({ "id": "be", "from": "b", "to": "e", "capacity": "2" }),
                    serde_json::json!({ "id": "ec", "from": "e", "to": "c", "capacity": "2" }),
                ]);
        }
        AlgorithmId::WarmStartPushRelabel => {
            scenario["payload"]["graph"]["nodes"]
                .as_array_mut()?
                .push(serde_json::json!({ "id": "c" }));
            scenario["payload"]["graph"]["edges"]
                .as_array_mut()?
                .extend([
                    serde_json::json!({ "id": "sc", "from": "s", "to": "c", "capacity": "4", "initial_flow": "4" }),
                    serde_json::json!({ "id": "ct", "from": "c", "to": "t", "capacity": "2", "initial_flow": "0" }),
                    serde_json::json!({ "id": "ac", "from": "a", "to": "c", "capacity": "2", "initial_flow": "0" }),
                    serde_json::json!({ "id": "cb", "from": "c", "to": "b", "capacity": "2", "initial_flow": "0" }),
                ]);
        }
        AlgorithmId::EpsilonRelaxation => {
            let edge_count = 16;
            scenario["payload"]["model"]["required_flow"] =
                serde_json::json!(edge_count.to_string());
            scenario["payload"]["graph"]["edges"] = serde_json::Value::Array(
                (1..=edge_count)
                    .map(|cost| {
                        serde_json::json!({
                            "id": format!("parallel-{cost:02}"),
                            "from": "s",
                            "to": "t",
                            "capacity": "1",
                            "cost": cost.to_string()
                        })
                    })
                    .collect(),
            );
        }
        AlgorithmId::ParametricPseudoflow | AlgorithmId::ParametricBreakpointRerun => {
            let nodes = ["s", "a", "b", "c", "t"]
                .into_iter()
                .map(|id| serde_json::json!({ "id": id }))
                .collect::<Vec<_>>();
            let edges = serde_json::json!([
                { "id": "sa", "from": "s", "to": "a", "capacity": "1", "cost": "0" },
                { "id": "sb", "from": "s", "to": "b", "capacity": "3", "cost": "0" },
                { "id": "sc", "from": "s", "to": "c", "capacity": "5", "cost": "0" },
                { "id": "at", "from": "a", "to": "t", "capacity": "7", "cost": "0" },
                { "id": "bt", "from": "b", "to": "t", "capacity": "6", "cost": "0" },
                { "id": "ct", "from": "c", "to": "t", "capacity": "8", "cost": "0" },
                { "id": "ab", "from": "a", "to": "b", "capacity": "3", "cost": "0" },
                { "id": "bc", "from": "b", "to": "c", "capacity": "2", "cost": "0" },
                { "id": "b-loop", "from": "b", "to": "b", "capacity": "3", "cost": "0" }
            ]);
            scenario["payload"]["model"]["parameter"]["maximum"]["numerator"] =
                serde_json::json!("3");
            scenario["payload"]["model"]["capacity_slopes"] = serde_json::json!([
                { "edge_id": "bt", "slope": "-1" },
                { "edge_id": "ct", "slope": "-1" },
                { "edge_id": "sb", "slope": "1" },
                { "edge_id": "sc", "slope": "1" }
            ]);
            scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
        }
        AlgorithmId::ElectricalFlow => {
            let node_ids = std::iter::once("s".to_owned())
                .chain((0..16).map(|index| format!("v{index:02}")))
                .chain(std::iter::once("t".to_owned()))
                .collect::<Vec<_>>();
            let nodes = node_ids
                .iter()
                .map(|id| serde_json::json!({ "id": id }))
                .collect::<Vec<_>>();
            let mut edges = node_ids
                .windows(2)
                .enumerate()
                .map(|(index, pair)| {
                    serde_json::json!({
                        "id": format!("path-{index:02}"), "from": &pair[0], "to": &pair[1],
                        "capacity": (1 + index % 5).to_string(), "cost": "0"
                    })
                })
                .collect::<Vec<_>>();
            for index in 0..13 {
                edges.push(serde_json::json!({
                    "id": format!("chord-{index:02}"), "from": &node_ids[index],
                    "to": &node_ids[index + 3], "capacity": (2 + index % 7).to_string(),
                    "cost": "0"
                }));
            }
            scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
        }
        AlgorithmId::RandomizedAlmostLinearMaxFlow
        | AlgorithmId::DeterministicAlmostLinearMaxFlow => {
            let node_ids = ["s", "a", "b", "c", "d", "e", "t"];
            let nodes = node_ids
                .into_iter()
                .map(|id| serde_json::json!({ "id": id }))
                .collect::<Vec<_>>();
            let edges = serde_json::json!([
                { "id": "sa", "from": "s", "to": "a", "capacity": "1", "cost": "0" },
                { "id": "sb", "from": "s", "to": "b", "capacity": "1", "cost": "0" },
                { "id": "ac", "from": "a", "to": "c", "capacity": "1", "cost": "0" },
                { "id": "bc", "from": "b", "to": "c", "capacity": "1", "cost": "0" },
                { "id": "cd", "from": "c", "to": "d", "capacity": "1", "cost": "0" },
                { "id": "ce", "from": "c", "to": "e", "capacity": "1", "cost": "0" },
                { "id": "dt", "from": "d", "to": "t", "capacity": "1", "cost": "0" },
                { "id": "et", "from": "e", "to": "t", "capacity": "1", "cost": "0" }
            ]);
            scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
        }
        AlgorithmId::MinimumRatioCycleMcf | AlgorithmId::RandomizedAlmostLinearMcf => {
            let nodes = serde_json::json!([
                { "id": "s", "supply": "1" },
                { "id": "a", "supply": "0" },
                { "id": "b", "supply": "0" },
                { "id": "t", "supply": "-1" }
            ]);
            let edges = serde_json::json!([
                { "id": "sa-1", "from": "s", "to": "a", "capacity": "1", "cost": "1" },
                { "id": "sa-2", "from": "s", "to": "a", "capacity": "1", "cost": "2" },
                { "id": "at", "from": "a", "to": "t", "capacity": "1", "cost": "2" },
                { "id": "sb", "from": "s", "to": "b", "capacity": "1", "cost": "3" },
                { "id": "bt", "from": "b", "to": "t", "capacity": "1", "cost": "1" },
                { "id": "ab", "from": "a", "to": "b", "capacity": "1", "cost": "1" },
                { "id": "ba", "from": "b", "to": "a", "capacity": "1", "cost": "2" }
            ]);
            scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
        }
        AlgorithmId::PrimalDualInteriorPointMcf => {
            // The four-node dense fixture drives the exact forest-subset trace
            // beyond the public event budget.  A single original arc still
            // exercises every source phase (forest construction, fundamental
            // cycle sampling, crossover, and recovery) while remaining a
            // readable bounded-kernel witness.
            scenario["payload"]["graph"] = serde_json::json!({
                "nodes": [
                    { "id": "s", "supply": "2" },
                    { "id": "t", "supply": "-2" }
                ],
                "edges": [
                    { "id": "st", "from": "s", "to": "t", "capacity": "2", "cost": "1" }
                ]
            });
        }
        AlgorithmId::ElectricalFlowInteriorPointMcf => {
            scenario["payload"]["graph"] = serde_json::json!({
                "nodes": [
                    { "id": "s", "supply": "3" },
                    { "id": "a", "supply": "0" },
                    { "id": "b", "supply": "0" },
                    { "id": "t", "supply": "-3" }
                ],
                "edges": [
                    { "id": "sa", "from": "s", "to": "a", "capacity": "3", "cost": "1" },
                    { "id": "at", "from": "a", "to": "t", "capacity": "3", "cost": "2" },
                    { "id": "sb", "from": "s", "to": "b", "capacity": "3", "cost": "3" },
                    { "id": "bt", "from": "b", "to": "t", "capacity": "3", "cost": "1" },
                    { "id": "ab", "from": "a", "to": "b", "capacity": "2", "cost": "1" }
                ]
            });
        }
        AlgorithmId::EnhancedCapacityScaling => {
            let node_ids = (0..10).map(|index| format!("v{index}")).collect::<Vec<_>>();
            let nodes = node_ids
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    serde_json::json!({
                        "id": id,
                        "supply": if index == 0 { "20" } else if index == 9 { "-20" } else { "0" }
                    })
                })
                .collect::<Vec<_>>();
            let edges = node_ids
                .iter()
                .enumerate()
                .flat_map(|(from_index, from)| {
                    node_ids
                        .iter()
                        .enumerate()
                        .filter(move |(to_index, _)| *to_index != from_index)
                        .map(move |(to_index, to)| {
                            serde_json::json!({
                                "id": format!("e{from_index}-{to_index}"),
                                "from": from,
                                "to": to,
                                "capacity": "40",
                                "cost": (1 + (from_index * 7 + to_index * 3) % 11).to_string()
                            })
                        })
                })
                .collect::<Vec<_>>();
            scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
        }
        AlgorithmId::ExcessScalingMcf => {
            let branches = (0..5).map(|index| format!("p{index}")).collect::<Vec<_>>();
            let nodes = std::iter::once(serde_json::json!({ "id": "s" }))
                .chain(branches.iter().map(|id| serde_json::json!({ "id": id })))
                .chain(std::iter::once(serde_json::json!({ "id": "t" })))
                .collect::<Vec<_>>();
            let edges = branches
                .iter()
                .enumerate()
                .flat_map(|(index, node)| {
                    [
                        serde_json::json!({
                            "id": format!("in-{index}"), "from": "s", "to": node,
                            "capacity": "15", "cost": (index + 1).to_string()
                        }),
                        serde_json::json!({
                            "id": format!("out-{index}"), "from": node, "to": "t",
                            "capacity": "15", "cost": (5 - index).to_string()
                        }),
                    ]
                })
                .collect::<Vec<_>>();
            scenario["payload"]["model"]["required_flow"] = serde_json::json!("15");
            scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
        }
        AlgorithmId::DualNetworkSimplex | AlgorithmId::PolynomialDualNetworkSimplex => {
            let node_ids = (0..5).map(|index| format!("v{index}")).collect::<Vec<_>>();
            let nodes = node_ids
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    serde_json::json!({
                        "id": id,
                        "supply": if index == 0 { "5" } else if index == 4 { "-5" } else { "0" }
                    })
                })
                .collect::<Vec<_>>();
            let edges = node_ids
                .iter()
                .enumerate()
                .flat_map(|(from_index, from)| {
                    node_ids
                        .iter()
                        .enumerate()
                        .filter(move |(to_index, _)| from_index != *to_index)
                        .map(move |(to_index, to)| {
                            serde_json::json!({
                                "id": format!("e{from_index}-{to_index}"), "from": from, "to": to,
                                "capacity": "10", "cost": (1 + (from_index * 3 + to_index * 5) % 11).to_string()
                            })
                        })
                })
                .collect::<Vec<_>>();
            scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
        }
        AlgorithmId::TransportationSimplex | AlgorithmId::Modi => {
            let origins = (0..4).map(|index| format!("o{index}")).collect::<Vec<_>>();
            let destinations = (0..4).map(|index| format!("d{index}")).collect::<Vec<_>>();
            let origin_supply = [3, 2, 4, 3];
            let destination_demand = [2, 4, 3, 3];
            let nodes = origins
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    serde_json::json!({ "id": id, "supply": origin_supply[index].to_string() })
                })
                .chain(destinations.iter().enumerate().map(|(index, id)| {
                    serde_json::json!({ "id": id, "supply": format!("-{}", destination_demand[index]) })
                }))
                .collect::<Vec<_>>();
            let edges = origins
                .iter()
                .enumerate()
                .flat_map(|(origin_index, from)| {
                    destinations.iter().enumerate().map(move |(destination_index, to)| {
                        serde_json::json!({
                            "id": format!("route-{origin_index}-{destination_index}"),
                            "from": from, "to": to, "capacity": "12",
                            "cost": (1 + (origin_index * 7 + destination_index * 3) % 13).to_string()
                        })
                    })
                })
                .collect::<Vec<_>>();
            scenario["payload"]["model"] = serde_json::json!({
                "kind": "transportation", "origins": origins, "destinations": destinations
            });
            scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
        }
        AlgorithmId::SegmentExpandedConvexMcf | AlgorithmId::ConvexCostScaling => {
            let node_ids = std::iter::once("s".to_owned())
                .chain((0..5).map(|index| format!("v{index}")))
                .chain(std::iter::once("t".to_owned()))
                .collect::<Vec<_>>();
            let nodes = node_ids
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    serde_json::json!({
                        "id": id,
                        "supply": if index == 0 { "4" } else if index + 1 == node_ids.len() { "-4" } else { "0" }
                    })
                })
                .collect::<Vec<_>>();
            let mut edges = node_ids
                .windows(2)
                .enumerate()
                .map(|(index, pair)| {
                    serde_json::json!({
                        "id": format!("path-{index}"), "from": &pair[0], "to": &pair[1],
                        "lower": "0", "capacity": "4", "cost": (1 + index % 3).to_string()
                    })
                })
                .collect::<Vec<_>>();
            edges.push(serde_json::json!({
                "id": "direct", "from": "s", "to": "t", "capacity": "4", "cost": "0",
                "convex_cost": {
                    "base_cost_at_zero": "0",
                    "segments": [
                        { "end_flow": "1", "marginal_cost": "-2" },
                        { "end_flow": "2", "marginal_cost": "2" },
                        { "end_flow": "4", "marginal_cost": "9" }
                    ]
                }
            }));
            scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
            if algorithm == AlgorithmId::SegmentExpandedConvexMcf {
                let nodes = serde_json::json!([
                    { "id": "s", "supply": "16" },
                    { "id": "t", "supply": "-16" }
                ]);
                let edges = (0..24)
                    .map(|index| {
                        serde_json::json!({
                            "id": format!("segment-{index:02}"),
                            "from": "s",
                            "to": "t",
                            "capacity": "1",
                            "cost": "0",
                            "convex_cost": {
                                "base_cost_at_zero": "0",
                                "segments": [{
                                    "end_flow": "1",
                                    "marginal_cost": (index + 1).to_string()
                                }]
                            }
                        })
                    })
                    .collect::<Vec<_>>();
                scenario["payload"]["graph"] =
                    serde_json::json!({ "nodes": nodes, "edges": edges });
            }
        }
        AlgorithmId::ConvexNetworkSimplex => {
            let nodes = serde_json::json!([
                { "id": "s", "supply": "16" },
                { "id": "t", "supply": "-16" }
            ]);
            let edges = (0..24)
                .map(|index| {
                    serde_json::json!({
                        "id": format!("parallel-{index:02}"),
                        "from": "s",
                        "to": "t",
                        "capacity": "1",
                        "cost": "0",
                        "convex_cost": {
                            "base_cost_at_zero": "0",
                            "segments": [{
                                "end_flow": "1",
                                "marginal_cost": (index + 1).to_string()
                            }]
                        }
                    })
                })
                .collect::<Vec<_>>();
            scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
        }
        AlgorithmId::PredictionAssistedEpsilonRelaxation => {
            let branches = (0..5).map(|index| format!("p{index}")).collect::<Vec<_>>();
            let nodes = std::iter::once(serde_json::json!({ "id": "s" }))
                .chain(branches.iter().map(|id| serde_json::json!({ "id": id })))
                .chain(std::iter::once(serde_json::json!({ "id": "t" })))
                .collect::<Vec<_>>();
            let edges = branches
                .iter()
                .enumerate()
                .flat_map(|(index, node)| {
                    [
                        serde_json::json!({
                            "id": format!("in-{index}"), "from": "s", "to": node,
                            "capacity": "3", "cost": (index + 1).to_string()
                        }),
                        serde_json::json!({
                            "id": format!("out-{index}"), "from": node, "to": "t",
                            "capacity": "3", "cost": (5 - index).to_string()
                        }),
                    ]
                })
                .collect::<Vec<_>>();
            let mut potentials = serde_json::Map::new();
            potentials.insert("s".to_owned(), serde_json::json!("200"));
            potentials.insert("t".to_owned(), serde_json::json!("-200"));
            for (index, node) in branches.iter().enumerate() {
                potentials.insert(
                    node.clone(),
                    serde_json::json!(((index as i128 - 2) * 75).to_string()),
                );
            }
            scenario["payload"]["model"]["required_flow"] = serde_json::json!("15");
            scenario["payload"]["algorithm"]["config"]["predicted_potentials"] =
                serde_json::Value::Object(potentials);
            scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
        }
        _ => return None,
    }
    Some(scenario.to_string())
}

fn representative_source_specific_variant(
    source: &str,
    algorithm: AlgorithmId,
    variant: usize,
) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("source-specific representative is JSON");
    match algorithm {
        AlgorithmId::DynamicEibfs => vary_dynamic_eibfs(&mut scenario, variant),
        AlgorithmId::WarmStartPushRelabel => vary_warm_start(&mut scenario, variant),
        AlgorithmId::ExcessScalingMcf => vary_excess_scaling(&mut scenario, variant),
        AlgorithmId::PolynomialDualNetworkSimplex => {
            vary_polynomial_dual_simplex(&mut scenario, variant);
        }
        AlgorithmId::TardosFramework => vary_tardos_framework(&mut scenario, variant),
        AlgorithmId::DeterministicAlmostLinearMcf => {
            vary_deterministic_almost_linear_mcf(&mut scenario, variant);
        }
        AlgorithmId::PredictionAssistedEpsilonRelaxation => {
            vary_prediction_assisted_relaxation(&mut scenario, variant);
        }
        _ => panic!("algorithm does not require a source-specific representative"),
    }
    scenario.to_string()
}

fn vary_dynamic_eibfs(scenario: &mut serde_json::Value, variant: usize) {
    let capacities = if variant == 1 {
        ["3", "5", "1"]
    } else {
        ["2", "3", "2"]
    };
    for (update, capacity) in scenario["payload"]["updates"]
        .as_array_mut()
        .expect("dynamic update array")
        .iter_mut()
        .zip(capacities)
    {
        update["capacity"] = serde_json::json!(capacity);
    }
}

fn vary_warm_start(scenario: &mut serde_json::Value, variant: usize) {
    let predicted = if variant == 1 {
        ["4", "1", "1", "4"]
    } else {
        ["3", "0", "0", "2"]
    };
    for (edge, flow) in scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("warm-start edge array")
        .iter_mut()
        .zip(predicted)
    {
        edge["initial_flow"] = serde_json::json!(flow);
    }
}

fn vary_excess_scaling(scenario: &mut serde_json::Value, variant: usize) {
    let required_flow = if variant == 1 { "11" } else { "9" };
    scenario["payload"]["model"]["required_flow"] = serde_json::json!(required_flow);
    for edge in scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("excess-scaling edge array")
    {
        edge["capacity"] = serde_json::json!(required_flow);
    }
}

fn vary_polynomial_dual_simplex(scenario: &mut serde_json::Value, variant: usize) {
    let amount = if variant == 1 { "2" } else { "4" };
    let nodes = scenario["payload"]["graph"]["nodes"]
        .as_array_mut()
        .expect("dual-simplex nodes");
    let positive = nodes
        .iter()
        .position(|node| node["supply"].as_str().is_some_and(|value| value != "0"))
        .expect("positive dual-simplex supply");
    let negative = nodes
        .iter()
        .rposition(|node| node["supply"].as_str().is_some_and(|value| value != "0"))
        .expect("negative dual-simplex supply");
    nodes[positive]["supply"] = serde_json::json!(amount);
    nodes[negative]["supply"] = serde_json::json!(format!("-{amount}"));
    for edge in scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("dual-simplex edges")
    {
        edge["capacity"] = serde_json::json!("100");
    }
}

fn vary_tardos_framework(scenario: &mut serde_json::Value, variant: usize) {
    let potential = if variant == 1 { "1" } else { "-2" };
    scenario["payload"]["algorithm"]["config"]["potentials"]["a"] = serde_json::json!(potential);
    scenario["payload"]["graph"]["edges"][2]["cost"] =
        serde_json::json!(if variant == 1 { "21" } else { "23" });
    let edges = scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("Tardos representative edges");
    for index in 0..12 {
        edges.push(serde_json::json!({
            "id": format!("audit-direct-{variant}-{index}"),
            "from": "s",
            "to": "t",
            "capacity": "2",
            "cost": (30 + index).to_string(),
        }));
    }
}

fn vary_deterministic_almost_linear_mcf(scenario: &mut serde_json::Value, variant: usize) {
    let costs = if variant == 1 {
        ["1", "2", "6"]
    } else {
        ["1", "2", "7"]
    };
    for (edge, cost) in scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("flow-framework edges")
        .iter_mut()
        .zip(costs)
    {
        edge["cost"] = serde_json::json!(cost);
        let lower = edge
            .get("lower")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("0")
            .parse::<i128>()
            .expect("strict-interior lower");
        let capacity = edge["capacity"]
            .as_str()
            .expect("strict-interior capacity")
            .parse::<i128>()
            .expect("strict-interior capacity integer");
        assert!(lower < capacity, "representative preserves strict interior");
    }
}

fn vary_prediction_assisted_relaxation(scenario: &mut serde_json::Value, variant: usize) {
    let (source, middle, sink) = if variant == 1 {
        ("0", "0", "0")
    } else {
        ("40", "-30", "20")
    };
    let predictions = &mut scenario["payload"]["algorithm"]["config"]["predicted_potentials"];
    predictions["s"] = serde_json::json!(source);
    predictions["a"] = serde_json::json!(middle);
    predictions["t"] = serde_json::json!(sink);
}

fn audit_representative_frame(
    algorithm: AlgorithmId,
    label: &str,
    index: usize,
    frame: &serde_json::Value,
    audit: &mut RepresentativeBoundaryAuditState,
) -> Result<(), String> {
    let event = &frame["trace_event"];
    let catalog_id = event["catalog_id"]
        .as_str()
        .ok_or_else(|| format!("{label} frame {index} omitted catalog_id"))?;
    if catalog_id.ends_with(".primary-work-unit") {
        return Err(format!(
            "{label} frame {index} publishes forbidden counter-only Detail {catalog_id}"
        ));
    }
    if catalog_id.ends_with(".work-observation") {
        return Err(format!(
            "{label} frame {index} publishes a synthetic work observation"
        ));
    }
    if catalog_id == algorithm.as_str() || !catalog_id.contains('.') {
        return Err(format!(
            "{label} frame {index} collapses its action identity into {catalog_id}"
        ));
    }
    if event["pseudocode_line"].as_str().is_none_or(str::is_empty) {
        return Err(format!(
            "{label} frame {index} omitted its pseudocode boundary"
        ));
    }
    if event["patch_count"].as_u64().is_none() {
        return Err(format!("{label} frame {index} omitted its patch count"));
    }
    let boundary = event["minimum_granularity"]
        .as_str()
        .ok_or_else(|| format!("{label} frame {index} omitted granularity"))?;
    let role = frame["trace_event_semantics"]["role"]
        .as_str()
        .ok_or_else(|| format!("{label} frame {index} omitted its semantic role"))?;
    if !["observe", "select", "mutate", "commit", "certify"].contains(&role) {
        return Err(format!("{label} frame {index} has unknown role {role}"));
    }
    let granularity_index = match boundary {
        "phase" => 0,
        "operation" => 1,
        "micro" => 2,
        _ => {
            return Err(format!(
                "{label} frame {index} has unknown granularity {boundary:?}"
            ));
        }
    };
    if role != "certify" || boundary == "micro" {
        audit.granularity_counts[granularity_index] += 1;
    }
    let witness = audit_representative_work(label, index, catalog_id, frame, boundary, audit)?;
    record_representative_witness(label, index, frame, role, witness, audit)?;
    if let Some(previous) = audit
        .action_boundaries
        .insert(catalog_id.to_owned(), boundary.to_owned())
        && previous != boundary
    {
        return Err(format!(
            "{label} action {catalog_id} changed boundary from {previous} to {boundary}"
        ));
    }
    audit_representative_entities(label, index, frame, boundary)?;
    audit.actions.insert(catalog_id.to_owned());
    Ok(())
}

fn record_representative_witness(
    label: &str,
    index: usize,
    frame: &serde_json::Value,
    role: &str,
    witness: RepresentativeBoundaryWitness,
    audit: &mut RepresentativeBoundaryAuditState,
) -> Result<(), String> {
    if role != "certify" && audit.first_detail.is_none() {
        audit.first_detail = Some(witness.clone());
    }
    let signature = representative_detail_signature(frame);
    if audit.previous_detail_signature.as_deref() == Some(signature.as_str()) {
        let previous = audit
            .previous_detail_catalog_id
            .as_deref()
            .unwrap_or("<unknown>");
        return Err(format!(
            "{label} frame {index} ({}) repeats frame {} ({previous}) without a published visual change; role={} focus={} primary-block={} progress={}",
            witness.catalog_id,
            index.saturating_sub(1),
            frame["trace_event_semantics"]["role"],
            frame["trace_event"]["entity_refs"],
            frame["trace_event_semantics"]["primary_work_block"],
            frame["trace_event_semantics"]["work_progress"],
        ));
    }
    audit.previous_detail_signature = Some(signature);
    audit.previous_detail_catalog_id = Some(witness.catalog_id.clone());
    audit.detail_witnesses.push(witness.clone());
    for overlay in &witness.active_overlays {
        audit
            .overlay_witnesses
            .entry(overlay.clone())
            .or_insert_with(|| witness.clone());
    }
    if witness.primary_delta > 0 {
        let source_action =
            audit_representative_primary_work_range(label, index, frame, witness.primary_delta)?;
        audit.primary_work_boundaries = audit
            .primary_work_boundaries
            .checked_add(1)
            .ok_or_else(|| format!("{label} primary-work boundary overflow"))?;
        audit.primary_work_actions.insert(source_action);
        if audit.first_primary_work.is_none() {
            audit.first_primary_work = Some(witness.clone());
        }
    }
    if audit
        .maximum_aggregation
        .as_ref()
        .is_none_or(|maximum| witness.aggregation > maximum.aggregation)
    {
        audit.maximum_aggregation = Some(witness.clone());
    }
    if audit
        .maximum_primary_work
        .as_ref()
        .is_none_or(|maximum| witness.primary_delta > maximum.primary_delta)
    {
        audit.maximum_primary_work = Some(witness);
    }
    Ok(())
}

fn audit_representative_primary_work_range(
    label: &str,
    index: usize,
    frame: &serde_json::Value,
    primary_delta: u128,
) -> Result<String, String> {
    let (first, last, total) = representative_primary_work_range(label, index, frame)?;
    let block = last
        .checked_sub(first)
        .and_then(|distance| distance.checked_add(1))
        .ok_or_else(|| format!("{label} frame {index} has a descending work range"))?;
    if block != primary_delta || first == 0 || last > total {
        return Err(format!(
            "{label} frame {index} primary-work range does not match its {primary_delta} counted units"
        ));
    }
    let catalog_id = frame["trace_event"]["catalog_id"]
        .as_str()
        .ok_or_else(|| format!("{label} frame {index} omitted catalog id"))?;
    if catalog_id.ends_with(".work-observation") {
        return Err(format!(
            "{label} frame {index} uses a synthetic primary-work boundary"
        ));
    }
    Ok(catalog_id.to_owned())
}

fn representative_primary_work_range(
    label: &str,
    index: usize,
    frame: &serde_json::Value,
) -> Result<(u128, u128, u128), String> {
    let parse = |value: &str| {
        value
            .parse::<u128>()
            .map_err(|error| format!("{label} frame {index} work range: {error}"))
    };
    let typed_block = &frame["trace_event_semantics"]["primary_work_block"];
    let first = parse(
        typed_block["first"]
            .as_str()
            .ok_or_else(|| format!("{label} frame {index} omitted primary-work first"))?,
    )?;
    let last = parse(
        typed_block["last"]
            .as_str()
            .ok_or_else(|| format!("{label} frame {index} omitted primary-work last"))?,
    )?;
    let total = parse(
        typed_block["total"]
            .as_str()
            .ok_or_else(|| format!("{label} frame {index} omitted primary-work total"))?,
    )?;
    Ok((first, last, total))
}

// This intentionally closed structural projection enumerates every typed
// overlay; keeping the registry in one match makes an unreviewed overlay fail
// the representative audit instead of silently dropping from its signature.
#[allow(clippy::too_many_lines)]
fn representative_detail_signature(frame: &serde_json::Value) -> String {
    let mut overlays = serde_json::Map::new();
    for (field, value) in frame.as_object().into_iter().flatten() {
        if !field.ends_with("_overlay") {
            continue;
        }
        let structural = match value {
            serde_json::Value::Object(object) => serde_json::Value::Object(
                object
                    .iter()
                    .filter(|(_, child)| child.is_array() || child.is_object())
                    .map(|(key, child)| (key.clone(), child.clone()))
                    .collect(),
            ),
            _ => value.clone(),
        };
        overlays.insert(field.clone(), structural);
    }
    // These ordinals are rendered on the exact inspected edge/arc in original
    // and residual modes. Keep them in the source-scene signature so repeated
    // visits to one target must advance a visible, kernel-owned value.
    let rendered_source_detail = matches!(
        frame["trace_event"]["catalog_id"].as_str(),
        Some(
            "minimum-mean-cycle-canceling.inspect-residual-arc"
                | "polynomial-primal-network-simplex.inspect-extended-arc"
                | "relaxation.scan-balanced-arcs"
                | "relaxation.scan-boundary-flow-arc"
                | "relaxation.scan-price-cut-arc"
        )
    )
    .then(|| frame["trace_event"]["detail"].clone());
    let catalog_id = frame["trace_event"]["catalog_id"].as_str().unwrap_or("");
    let pseudocode_line = frame["trace_event"]["pseudocode_line"]
        .as_str()
        .unwrap_or("");
    let is_inspection_name = |value: &str| {
        value
            .split(['.', ':', '-'])
            .any(|segment| segment == "inspect" || segment == "scan")
    };
    let exact_arc_target = frame["trace_event"]["entity_refs"]
        .as_array()
        .filter(|refs| refs.len() == 1)
        .and_then(|refs| refs.first())
        .filter(|target| matches!(target["kind"].as_str(), Some("edge" | "residual-arc")));
    let specialized_source_detail = matches!(
        catalog_id,
        "minimum-mean-cycle-canceling.inspect-residual-arc"
            | "polynomial-primal-network-simplex.inspect-extended-arc"
            | "relaxation.scan-balanced-arcs"
            | "relaxation.scan-boundary-flow-arc"
            | "relaxation.scan-price-cut-arc"
    );
    // The web renderer anchors this measured position directly to the one
    // inspected edge/residual arc. It is source work, not a synthetic frame
    // discriminator, so repeated visits must advance a visible value.
    let rendered_arc_inspection_work = (!specialized_source_detail
        && matches!(
            frame["trace_event_semantics"]["role"].as_str(),
            Some("select" | "mutate")
        )
        && !frame["trace_event_semantics"]["primary_work_block"].is_null()
        && exact_arc_target.is_some()
        && (is_inspection_name(catalog_id) || is_inspection_name(pseudocode_line)))
    .then(|| {
        serde_json::json!({
            "target": exact_arc_target,
            "primary_work_block": frame["trace_event_semantics"]["primary_work_block"],
            "primary_completed": frame["trace_event_semantics"]["work_progress"]["primary_completed"],
            "primary_total": frame["trace_event_semantics"]["work_progress"]["primary_total"],
        })
    });
    let cancel_tighten_stage_badge = frame["cancel_tighten_overlay"].as_object().map(|overlay| {
        serde_json::json!({
            "stage": overlay.get("stage"),
            "phase": overlay.get("phase"),
            "epsilon": overlay.get("epsilon"),
            "delta": overlay.get("delta"),
        })
    });
    let scaling_stage_badge = matches!(
        frame["trace_event"]["catalog_id"].as_str(),
        Some(
            "capacity-scaling-mcf.start-scaling-phase"
                | "capacity-scaling-mcf.complete-scaling-phase"
                | "excess-scaling-mcf.start-excess-phase"
                | "excess-scaling-mcf.complete-excess-phase"
        )
    )
    .then(|| {
        serde_json::json!({
            "catalog_id": frame["trace_event"]["catalog_id"],
            "detail": frame["trace_event"]["detail"],
        })
    });
    let goldberg_rao_stage_badge = matches!(
        frame["trace_event"]["catalog_id"].as_str(),
        Some(
            "goldberg-rao.initialize-cut-gap"
                | "goldberg-rao.start-gap-phase"
                | "goldberg-rao.inspect-residual-arc"
                | "goldberg-rao.build-reverse-zero-one-adjacency"
                | "goldberg-rao.relax-binary-distance"
                | "goldberg-rao.inspect-binary-length"
                | "goldberg-rao.binary-length-distance"
                | "goldberg-rao.minimum-canonical-cut"
                | "goldberg-rao.mark-special-arcs"
                | "goldberg-rao.contract-zero-scc"
                | "goldberg-rao.blocking-or-delta-flow"
                | "goldberg-rao.lift-component-flow"
                | "goldberg-rao.halve-cut-gap"
                | "goldberg-rao.optimal"
        )
    )
    .then(|| {
        serde_json::json!({
            "catalog_id": frame["trace_event"]["catalog_id"],
            "detail": frame["trace_event"]["detail"],
        })
    });
    let hassin_stage_badge = matches!(
        frame["trace_event"]["catalog_id"].as_str(),
        Some(
            "hassin-st-planar.split-outer-face"
                | "hassin-st-planar.settle-dual-face"
                | "hassin-st-planar.reconstruct-primal-flow"
                | "hassin-st-planar.optimal-dual-cut"
        )
    )
    .then(|| {
        serde_json::json!({
            "catalog_id": frame["trace_event"]["catalog_id"],
            "detail": frame["trace_event"]["detail"],
            "dual_faces": frame["metrics"][5],
            "settled_faces": frame["metrics"][15],
            "positive_flow_edges": frame["metrics"][11],
        })
    });
    let enhanced_scaling_stage_badge =
        frame["enhanced_capacity_scaling_overlay"]
            .as_object()
            .map(|overlay| {
                serde_json::json!({
                    "stage": overlay.get("stage"),
                    "phase": overlay.get("phase"),
                    "delta": overlay.get("delta"),
                    "augmentation": overlay.get("augmentation"),
                    "contraction_arc": overlay.get("contraction_arc"),
                })
            });
    let orlin_mcf_stage_badge = frame["orlin_mcf_overlay"].as_object().map(|overlay| {
        serde_json::json!({
            "stage": overlay.get("stage"),
            "phase": overlay.get("phase"),
            "delta": overlay.get("delta"),
            "augmentation": overlay.get("augmentation"),
            "inspection_serial": overlay.get("inspection_serial"),
        })
    });
    let primal_dual_ipm_forest_badge =
        frame["primal_dual_ipm_mcf_overlay"]
            .as_object()
            .map(|overlay| {
                serde_json::json!({
                    "stage": overlay.get("stage"),
                    "forest_subset_serial": overlay.get("forest_subset_serial"),
                })
            });
    let electrical_ipm_stage_badge =
        frame["electrical_ipm_mcf_overlay"]
            .as_object()
            .map(|overlay| {
                serde_json::json!({
                    "stage": overlay.get("stage"),
                    "mu": overlay.get("mu"),
                    "isolation_attempt": overlay.get("isolation_attempt"),
                })
            });
    let minimum_ratio_cycle_stage_badge =
        frame["minimum_ratio_cycle_mcf_overlay"]
            .as_object()
            .map(|overlay| {
                serde_json::json!({
                    "stage": overlay.get("stage"),
                    "candidate_ratio": overlay.get("candidate_ratio"),
                    "best_ratio": overlay.get("best_ratio"),
                })
            });
    let randomized_almost_linear_mcf_stage_badge = frame["randomized_almost_linear_mcf_overlay"]
        .as_object()
        .map(|overlay| {
            serde_json::json!({
                "stage": overlay.get("stage"),
                "feasible_flows": overlay.get("feasible_flows"),
                "isolation_attempt": overlay.get("isolation_attempt"),
                "sampled_forest_index": overlay.get("sampled_forest_index"),
                "detected_coordinates": overlay.get("detected_coordinates"),
            })
        });
    let deterministic_almost_linear_mcf_stage_badge = frame["flow_framework_mcf_overlay"]
        .as_object()
        .map(|overlay| {
            serde_json::json!({
                "stage": overlay.get("stage"),
                "iteration": overlay.get("iteration"),
                "gap_before": overlay.get("gap_before"),
                "gap_after": overlay.get("gap_after"),
                "potential_before": overlay.get("potential_before"),
                "potential_after": overlay.get("potential_after"),
                "accepted_ratio": overlay.get("accepted_ratio"),
                "reinitialized": overlay.get("reinitialized"),
            })
        });
    let convex_cost_stage_badge = frame["convex_cost_overlay"].as_object().map(|overlay| {
        serde_json::json!({
            "stage": overlay.get("stage"),
            "scale": overlay.get("scale"),
        })
    });
    let cost_refine_stage_badge = matches!(
        frame["trace_event"]["catalog_id"].as_str(),
        Some(
            "cost-scaling.start-refine"
                | "cost-scaling.complete-refine"
                | "cost-scaling-push-relabel.start-refine"
                | "cost-scaling-push-relabel.complete-refine"
                | "augment-relabel.start-refine"
                | "augment-relabel.complete-refine"
                | "partial-augment-relabel-mcf.start-refine"
                | "partial-augment-relabel-mcf.complete-refine"
                | "price-refinement.start-refine"
                | "price-refinement.complete-refine"
                | "arc-fixing.start-refine"
                | "arc-fixing.complete-refine"
                | "generalized-cost-scaling.start-refine"
                | "generalized-cost-scaling.complete-refine"
        )
    )
    .then(|| {
        serde_json::json!({
            "catalog_id": frame["trace_event"]["catalog_id"],
            "detail": frame["trace_event"]["detail"],
        })
    });
    let price_refinement_stage_badge = matches!(
        frame["trace_event"]["catalog_id"].as_str(),
        Some(
            "price-refinement.start-potential-only-attempt"
                | "price-refinement.complete-relaxation-round"
                | "price-refinement.succeed-without-flow-change"
                | "price-refinement.fail-and-rollback-prices"
        )
    )
    .then(|| {
        serde_json::json!({
            "catalog_id": frame["trace_event"]["catalog_id"],
            "detail": frame["trace_event"]["detail"],
        })
    });
    let polynomial_primal_stage_badge =
        frame["polynomial_primal_simplex_overlay"]
            .as_object()
            .map(|overlay| {
                serde_json::json!({
                    "stage": overlay.get("stage"),
                    "phase": overlay.get("phase"),
                    "epsilon": overlay.get("epsilon"),
                    "delta": overlay.get("delta"),
                    "potential_shift": overlay.get("potential_shift"),
                })
            });
    let polynomial_dual_stage_badge =
        frame["polynomial_dual_simplex_overlay"]
            .as_object()
            .map(|overlay| {
                serde_json::json!({
                    "stage": overlay.get("stage"),
                    "phase": overlay.get("phase"),
                    "delta": overlay.get("delta"),
                    "pivot_price_delta": overlay.get("pivot_price_delta"),
                })
            });
    let electrical_flow_stage_badge = frame["electrical_flow_overlay"].as_object().map(|overlay| {
        serde_json::json!({
            "stage": overlay.get("stage"),
            "iteration": overlay.get("iteration"),
            "residual_l2": overlay.get("residual_l2"),
            "relative_tolerance": overlay.get("relative_tolerance"),
        })
    });
    let augmenting_electrical_stage_badge =
        frame["augmenting_electrical_overlay"]
            .as_object()
            .map(|overlay| {
                serde_json::json!({
                            "stage": overlay.get("stage"),
                            "working_nodes": overlay.get("working_nodes"),
                            "working_edges": overlay.get("working_edges"),
                            "current_value": overlay.get("current_value"),
                            "working_target": overlay.get("working_target"),
                        "remaining": overlay.get("remaining"),
                    "active_pivot_node": overlay.get("active_pivot_node"),
                "active_working_path": overlay.get("active_working_path"),
                "active_extraction_cycle": overlay.get("active_extraction_cycle"),
                "active_discrete_amount": overlay.get("active_discrete_amount"),
                })
            });
    let interior_point_max_flow_stage_badge = frame["interior_point_max_flow_overlay"]
        .as_object()
        .map(|overlay| {
            serde_json::json!({
                "stage": overlay.get("stage"),
                "mu": overlay.get("mu"),
                "duality_gap": overlay.get("duality_gap"),
                "electrical_energy": overlay.get("electrical_energy"),
            })
        });
    let minimum_ratio_cycle_max_flow_stage_badge = frame["minimum_ratio_cycle_overlay"]
        .as_object()
        .map(|overlay| {
            serde_json::json!({
                "stage": overlay.get("stage"),
                "enumerated_vectors": overlay.get("enumerated_vectors"),
                "candidate_ratio": overlay.get("candidate_ratio"),
                "best_ratio": overlay.get("best_ratio"),
            })
        });
    let randomized_almost_linear_max_flow_stage_badge = frame["randomized_almost_linear_overlay"]
        .as_object()
        .map(|overlay| {
            serde_json::json!({
                "stage": overlay.get("stage"),
                "return_flow": overlay.get("return_flow"),
                "return_capacity": overlay.get("return_capacity"),
                "artificial_flow": overlay.get("artificial_flow"),
                "iteration": overlay.get("iteration"),
            })
        });
    let deterministic_almost_linear_max_flow_stage_badge =
        frame["deterministic_almost_linear_overlay"]
            .as_object()
            .map(|overlay| {
                serde_json::json!({
                    "stage": overlay.get("stage"),
                    "return_flow": overlay.get("return_flow"),
                    "return_capacity": overlay.get("return_capacity"),
                    "artificial_flow": overlay.get("artificial_flow"),
                    "core_vertices": overlay.get("core_vertices"),
                    "core_edges": overlay.get("core_edges"),
                    "active_level": overlay.get("active_level"),
                    "active_branches": overlay.get("active_branches"),
                    "passes": overlay.get("passes"),
                    "selected_off_tree_edge": overlay.get("selected_off_tree_edge"),
                    "selected_cycle_kind": overlay.get("selected_cycle_kind"),
                    "fundamental_cycles": overlay.get("fundamental_cycles"),
                })
            });
    let weighted_augmenting_paths_stage_badge = frame["weighted_augmenting_paths_overlay"]
        .as_object()
        .map(|overlay| {
            serde_json::json!({
                "stage": overlay.get("stage"),
                "phase": overlay.get("phase"),
                "phase_count": overlay.get("phase_count"),
                "capacity_bit": overlay.get("capacity_bit"),
                "round": overlay.get("round"),
                "relabel_jumps": overlay.get("relabel_jumps"),
                "augmentations": overlay.get("augmentations"),
            })
        });
    let weighted_push_relabel_stage_badge = frame["weighted_push_relabel_shortcut_overlay"]
        .as_object()
        .map(|overlay| {
            serde_json::json!({
                "stage": overlay.get("stage"),
                "hierarchy_levels": overlay.get("hierarchy_levels"),
                "height": overlay.get("height"),
                "routed": overlay.get("routed"),
                "demand": overlay.get("demand"),
                "relabel_steps": overlay.get("relabel_steps"),
                "augmentations": overlay.get("augmentations"),
                "residual_rounds": overlay.get("residual_rounds"),
            })
        });
    let eibfs_stage_badge = frame["eibfs_overlay"].as_object().map(|overlay| {
        let dynamic = frame["dynamic_eibfs_overlay"].as_object();
        serde_json::json!({
            "catalog_id": frame["trace_event"]["catalog_id"],
            "detail": frame["trace_event"]["detail"],
            "phase_direction": overlay.get("phase_direction"),
            "source_depth": overlay.get("source_depth"),
            "sink_depth": overlay.get("sink_depth"),
            "dynamic_stage": dynamic.and_then(|value| value.get("stage")),
            "update_index": dynamic.and_then(|value| value.get("update_index")),
            "update_total": dynamic.and_then(|value| value.get("update_total")),
            "repair_arc_scans": dynamic.and_then(|value| value.get("repair_arc_scans")),
        })
    });
    serde_json::json!({
        "edge_states": frame["edge_states"],
        "residual_arcs": frame["residual_arcs"],
        "node_trace_states": frame["node_trace_states"],
        "pseudoflow_forest": frame["pseudoflow_forest"],
        "overlays": overlays,
        "focus": frame["trace_event"]["entity_refs"],
        "changed": frame["trace_event_semantics"]["changed_entity_refs"],
        "rendered_source_detail": rendered_source_detail,
        "rendered_arc_inspection_work": rendered_arc_inspection_work,
        "cancel_tighten_stage_badge": cancel_tighten_stage_badge,
        "scaling_stage_badge": scaling_stage_badge,
        "goldberg_rao_stage_badge": goldberg_rao_stage_badge,
        "hassin_stage_badge": hassin_stage_badge,
        "enhanced_scaling_stage_badge": enhanced_scaling_stage_badge,
        "orlin_mcf_stage_badge": orlin_mcf_stage_badge,
        "primal_dual_ipm_forest_badge": primal_dual_ipm_forest_badge,
        "electrical_ipm_stage_badge": electrical_ipm_stage_badge,
        "minimum_ratio_cycle_stage_badge": minimum_ratio_cycle_stage_badge,
        "randomized_almost_linear_mcf_stage_badge": randomized_almost_linear_mcf_stage_badge,
        "deterministic_almost_linear_mcf_stage_badge": deterministic_almost_linear_mcf_stage_badge,
        "convex_cost_stage_badge": convex_cost_stage_badge,
        "cost_refine_stage_badge": cost_refine_stage_badge,
        "price_refinement_stage_badge": price_refinement_stage_badge,
        "polynomial_primal_stage_badge": polynomial_primal_stage_badge,
        "polynomial_dual_stage_badge": polynomial_dual_stage_badge,
        "electrical_flow_stage_badge": electrical_flow_stage_badge,
        "augmenting_electrical_stage_badge": augmenting_electrical_stage_badge,
        "interior_point_max_flow_stage_badge": interior_point_max_flow_stage_badge,
        "minimum_ratio_cycle_max_flow_stage_badge": minimum_ratio_cycle_max_flow_stage_badge,
        "randomized_almost_linear_max_flow_stage_badge": randomized_almost_linear_max_flow_stage_badge,
        "deterministic_almost_linear_max_flow_stage_badge": deterministic_almost_linear_max_flow_stage_badge,
        "weighted_augmenting_paths_stage_badge": weighted_augmenting_paths_stage_badge,
        "weighted_push_relabel_stage_badge": weighted_push_relabel_stage_badge,
        "eibfs_stage_badge": eibfs_stage_badge,
        "outcome": frame["outcome"],
    })
    .to_string()
}

fn audit_representative_work(
    label: &str,
    index: usize,
    catalog_id: &str,
    frame: &serde_json::Value,
    boundary: &str,
    audit: &mut RepresentativeBoundaryAuditState,
) -> Result<RepresentativeBoundaryWitness, String> {
    let work = frame["trace_event_semantics"]["work_deltas"]
        .as_array()
        .ok_or_else(|| format!("{label} frame {index} omitted work deltas"))?;
    if work.first().and_then(|delta| delta["unit"].as_str()) != Some("published-transition")
        || work.first().and_then(|delta| delta["count"].as_str()) != Some("1")
    {
        return Err(format!(
            "{label} frame {index} omitted its unit publication work"
        ));
    }
    let mut largest_work = 1_u128;
    let mut detail_work = None;
    let mut primary_delta = 0_u128;
    let mut work_deltas = Vec::with_capacity(work.len());
    for delta in work {
        let unit = delta["unit"]
            .as_str()
            .ok_or_else(|| format!("{label} frame {index} has an invalid work unit"))?;
        let count = delta["count"]
            .as_str()
            .ok_or_else(|| format!("{label} frame {index} has an invalid work count"))?
            .parse::<u128>()
            .map_err(|error| format!("{label} frame {index} work count: {error}"))?;
        if count == 0 {
            return Err(format!("{label} frame {index} has zero work"));
        }
        work_deltas.push(RepresentativeWorkDelta {
            unit: unit.to_owned(),
            count,
        });
        largest_work = largest_work.max(count);
        match unit {
            "published-transition" => {
                audit.published_transitions = audit
                    .published_transitions
                    .checked_add(count)
                    .ok_or_else(|| format!("{label} publication work overflow"))?;
            }
            "detail-primitive" => {
                detail_work = Some(count);
                audit.detail_primitives = audit
                    .detail_primitives
                    .checked_add(count)
                    .ok_or_else(|| format!("{label} detail work overflow"))?;
            }
            "primary-work" => {
                primary_delta = count;
                audit.primary_work = audit
                    .primary_work
                    .checked_add(count)
                    .ok_or_else(|| format!("{label} primary work overflow"))?;
            }
            _ => {}
        }
    }
    if (boundary == "micro") != detail_work.is_some() || detail_work.is_some_and(|count| count != 1)
    {
        return Err(format!(
            "{label} frame {index} does not account one Detail primitive"
        ));
    }
    let aggregation = frame["trace_event_semantics"]["aggregation_count"]
        .as_str()
        .ok_or_else(|| format!("{label} frame {index} omitted aggregation count"))?
        .parse::<u128>()
        .map_err(|error| format!("{label} frame {index} aggregation count: {error}"))?;
    if aggregation != largest_work {
        return Err(format!(
            "{label} frame {index} aggregation {aggregation} != largest work delta {largest_work}"
        ));
    }
    let (detail_completed, primary_completed) =
        audit_representative_work_progress(label, index, frame, audit)?;
    let primary_block = &frame["trace_event_semantics"]["primary_work_block"];
    Ok(RepresentativeBoundaryWitness {
        event: index,
        catalog_id: catalog_id.to_owned(),
        primary_delta,
        primary_completed,
        detail_completed,
        aggregation,
        work_deltas,
        work_first: optional_u128(&primary_block["first"]),
        work_last: optional_u128(&primary_block["last"]),
        work_total: optional_u128(&primary_block["total"]),
        active_overlays: representative_active_overlays(frame),
        overlay_scalar_values: representative_overlay_scalar_values(frame),
        touched_identities: representative_entity_identities(&frame["trace_event"]["entity_refs"]),
        changed_identities: representative_entity_identities(
            &frame["trace_event_semantics"]["changed_entity_refs"],
        ),
    })
}

fn optional_u128(value: &serde_json::Value) -> Option<u128> {
    value
        .as_str()
        .and_then(|integer| integer.parse::<u128>().ok())
}

fn representative_active_overlays(frame: &serde_json::Value) -> Vec<String> {
    frame
        .as_object()
        .into_iter()
        .flat_map(serde_json::Map::keys)
        .filter(|field| {
            field.ends_with("_overlay") && frame.get(*field).is_some_and(|value| !value.is_null())
        })
        .cloned()
        .collect()
}

fn representative_overlay_scalar_values(
    frame: &serde_json::Value,
) -> std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> {
    frame
        .as_object()
        .into_iter()
        .flat_map(|scene| scene.iter())
        .filter(|(field, value)| field.ends_with("_overlay") && value.is_object())
        .map(|(field, value)| {
            let scalars = value
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(name, scalar)| match scalar {
                    serde_json::Value::String(value) => Some((name.clone(), value.clone())),
                    serde_json::Value::Bool(value) => Some((name.clone(), value.to_string())),
                    serde_json::Value::Number(value) => Some((name.clone(), value.to_string())),
                    _ => None,
                })
                .collect();
            (field.clone(), scalars)
        })
        .collect()
}

fn audit_representative_work_progress(
    label: &str,
    index: usize,
    frame: &serde_json::Value,
    audit: &mut RepresentativeBoundaryAuditState,
) -> Result<(u128, u128), String> {
    let progress = &frame["trace_event_semantics"]["work_progress"];
    let progress_value = |field: &str| -> Result<u128, String> {
        progress[field]
            .as_str()
            .ok_or_else(|| format!("{label} frame {index} omitted work progress {field}"))?
            .parse::<u128>()
            .map_err(|error| format!("{label} frame {index} progress {field}: {error}"))
    };
    let detail_completed = progress_value("detail_completed")?;
    let detail_total = progress_value("detail_total")?;
    let primary_completed = progress_value("primary_completed")?;
    let primary_total = progress_value("primary_total")?;
    if detail_completed != audit.published_transitions || primary_completed != audit.primary_work {
        return Err(format!(
            "{label} frame {index} work progress disagrees with accumulated deltas"
        ));
    }
    if detail_completed > detail_total || primary_completed > primary_total {
        return Err(format!(
            "{label} frame {index} work progress exceeded its declared total"
        ));
    }
    if audit
        .declared_detail_total
        .replace(detail_total)
        .is_some_and(|previous| previous != detail_total)
        || audit
            .declared_primary_total
            .replace(primary_total)
            .is_some_and(|previous| previous != primary_total)
    {
        return Err(format!(
            "{label} frame {index} changed its immutable work totals"
        ));
    }
    Ok((detail_completed, primary_completed))
}

fn representative_entity_identity(entity: &serde_json::Value) -> Option<String> {
    match entity["kind"].as_str()? {
        "node" => Some(format!("node:{}", entity["node_id"].as_str()?)),
        "edge" => Some(format!("edge:{}", entity["edge_id"].as_str()?)),
        "residual-arc" => Some(format!(
            "residual-arc:{}:{}",
            entity["edge_id"].as_str()?,
            entity["direction"].as_str()?
        )),
        _ => None,
    }
}

fn representative_entity_identities(value: &serde_json::Value) -> Vec<String> {
    let mut identities = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(representative_entity_identity)
        .collect::<Vec<_>>();
    identities.sort();
    identities.dedup();
    identities
}

fn audit_representative_entities(
    label: &str,
    index: usize,
    frame: &serde_json::Value,
    boundary: &str,
) -> Result<(), String> {
    let catalog_id = frame["trace_event"]["catalog_id"]
        .as_str()
        .unwrap_or("unknown-action");
    let event = &frame["trace_event"];
    let touched_values = event["entity_refs"]
        .as_array()
        .ok_or_else(|| format!("{label} frame {index} omitted focus entities"))?;
    let touched = touched_values
        .iter()
        .map(|entity| {
            representative_entity_identity(entity)
                .ok_or_else(|| format!("{label} frame {index} has an invalid focus entity"))
        })
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    let changed = frame["trace_event_semantics"]["changed_entity_refs"]
        .as_array()
        .ok_or_else(|| format!("{label} frame {index} omitted changed entities"))?
        .iter()
        .map(|entity| {
            representative_entity_identity(entity)
                .ok_or_else(|| format!("{label} frame {index} has an invalid changed entity"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if touched.len() != touched_values.len() {
        return Err(format!(
            "{label} frame {index} contains duplicate focus entities"
        ));
    }
    if changed
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != changed.len()
    {
        return Err(format!(
            "{label} frame {index} contains duplicate changed entities"
        ));
    }
    if boundary == "micro" {
        let focused_nodes = touched_values
            .iter()
            .filter_map(|entity| {
                (entity["kind"].as_str() == Some("node"))
                    .then(|| entity["node_id"].as_str())
                    .flatten()
            })
            .collect::<std::collections::BTreeSet<_>>();
        let focused_edges = touched_values
            .iter()
            .filter_map(|entity| entity["edge_id"].as_str())
            .collect::<std::collections::BTreeSet<_>>();
        // A Micro boundary is one ordinary-network primitive: one arc with at
        // most its two endpoints, one node, or no ordinary identity when the
        // typed auxiliary overlay owns the work. Paths, cuts, forests, and
        // candidate sets belong to Operation/Phase state and must not be
        // republished as a graph-wide local focus.
        if focused_edges.len() > 1 {
            return Err(format!(
                "{label} frame {index} action {catalog_id} marks {} edges at one Micro boundary; publish one inspected arc and keep aggregate state in its typed overlay",
                focused_edges.len(),
            ));
        }
        // A matrix or assignment cell is one typed auxiliary primitive whose
        // row and column are two ordinary graph nodes. Keeping both coordinates
        // visible is the smallest faithful focus even when no ordinary network
        // edge connects them. This closed list must name the concrete producer;
        // arbitrary two-node Micro events remain forbidden.
        let inspects_two_node_auxiliary_cell = matches!(
            catalog_id,
            "electrical-flow.matrix-scalar-product"
                | "hungarian.inspect-cell"
                | "relaxed-most-negative-cycle.inspect-assignment-cell"
        );
        let maximum_focused_nodes = if focused_edges.is_empty() && !inspects_two_node_auxiliary_cell
        {
            1
        } else {
            2
        };
        if focused_nodes.len() > maximum_focused_nodes {
            return Err(format!(
                "{label} frame {index} action {catalog_id} marks {} nodes at one Micro boundary; publish one inspected node, one arc's endpoints, or the two endpoints of one declared typed auxiliary primitive and keep aggregate state in its typed overlay",
                focused_nodes.len(),
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn audit_representative_trace(
    algorithm: AlgorithmId,
    label: &str,
    source: &str,
    canonical: &str,
) -> Result<RepresentativeTraceAudit, String> {
    let mut trace_hasher = Sha256::new();
    validate_flow_session_input(source)
        .map_err(|error| format!("{label} failed runtime admission: {error}"))?;
    let mut session =
        FlowSession::new(source).map_err(|_| format!("{label} failed runtime admission"))?;
    let base = representative_trace_base(label, source, &session)?;
    let primary_ordinal = base.primary_ordinal;
    let base_primary_work = base.primary_work;
    let primary_work_unit = base.primary_work_unit;
    let primary_work_abstraction = base.primary_work_abstraction;
    let node_count = base.node_count;
    let edge_count = base.edge_count;
    let scenario_digest = base.scenario_digest;
    let first = session
        .stage_next_json()
        .map_err(|_| format!("{label} failed trace preparation"))?
        .ok_or_else(|| format!("{label} produced no event"))?;
    let first: serde_json::Value =
        serde_json::from_str(&first).map_err(|error| format!("{label} first frame: {error}"))?;
    update_representative_trace_digest(&mut trace_hasher, &first);
    if first["solve_status"] == "resource-limit" {
        return Err(format!("{label} reached its resource limit"));
    }
    let event_count = first["event_count"]
        .as_str()
        .ok_or_else(|| format!("{label} omitted event_count"))?
        .parse::<usize>()
        .map_err(|error| format!("{label} event_count: {error}"))?;
    if event_count == 0 {
        return Err(format!("{label} produced an empty trace"));
    }
    let mut audit = RepresentativeBoundaryAuditState::default();
    audit_representative_frame(algorithm, label, 1, &first, &mut audit)?;
    session.commit_staged_next();
    for index in 2..=event_count {
        let frame = session
            .stage_next_json()
            .map_err(|_| format!("{label} failed at event {index}"))?
            .ok_or_else(|| format!("{label} ended before event {index}"))?;
        let frame_value: serde_json::Value = serde_json::from_str(&frame)
            .map_err(|error| format!("{label} frame {index} JSON: {error}"))?;
        update_representative_trace_digest(&mut trace_hasher, &frame_value);
        audit_representative_frame(algorithm, label, index, &frame_value, &mut audit)?;
        session.commit_staged_next();
    }
    let final_frame = representative_final_frame(label, &mut session)?;
    if std::env::var_os("FLOW_REPRESENTATIVE_DEBUG_METRICS").is_some() {
        eprintln!(
            "FLOW_REPRESENTATIVE_METRICS\t{}\t{label}\t{}",
            algorithm.as_str(),
            final_frame["metrics"]
        );
    }
    let (
        distinct_actions,
        first_detail,
        first_primary_work,
        maximum_aggregation,
        maximum_primary_work,
    ) = validate_representative_terminal(
        label,
        &final_frame,
        event_count,
        primary_ordinal,
        base_primary_work,
        &audit,
    )?;
    let maximum_primary_work_delta = validate_meaningful_primary_work_boundaries(
        label,
        &primary_work_abstraction,
        &audit,
        &maximum_primary_work,
    )?;
    let (middle_detail, last_detail) = representative_detail_witnesses(label, &audit)?;
    let (control_contract, control_digest) =
        representative_control_provenance(algorithm, label, source, canonical)?;
    Ok(RepresentativeTraceAudit {
        label: label.to_owned(),
        node_count,
        edge_count,
        event_count,
        distinct_actions,
        granularity_counts: audit.granularity_counts,
        primary_work: audit.primary_work,
        primary_work_boundaries: audit.primary_work_boundaries,
        primary_work_unit,
        primary_work_abstraction,
        maximum_primary_work_delta,
        first_detail,
        middle_detail,
        last_detail,
        first_primary_work,
        maximum_aggregation,
        maximum_primary_work,
        overlay_witnesses: audit.overlay_witnesses,
        primary_work_actions: audit.primary_work_actions,
        action_boundaries: audit.action_boundaries,
        scenario_digest,
        trace_digest: encode_digest(&trace_hasher.finalize()),
        control_contract,
        control_digest,
        source: source.to_owned(),
    })
}

fn representative_detail_witnesses(
    label: &str,
    audit: &RepresentativeBoundaryAuditState,
) -> Result<(RepresentativeBoundaryWitness, RepresentativeBoundaryWitness), String> {
    let middle = audit
        .detail_witnesses
        .get(audit.detail_witnesses.len() / 2)
        .cloned()
        .ok_or_else(|| format!("{label} exposes no middle Detail witness"))?;
    let last = audit
        .detail_witnesses
        .last()
        .cloned()
        .ok_or_else(|| format!("{label} exposes no final Detail witness"))?;
    Ok((middle, last))
}

struct RepresentativeTraceBase {
    primary_ordinal: usize,
    primary_work: u128,
    primary_work_unit: String,
    primary_work_abstraction: String,
    node_count: usize,
    edge_count: usize,
    scenario_digest: String,
}

fn representative_trace_base(
    label: &str,
    source: &str,
    session: &FlowSession,
) -> Result<RepresentativeTraceBase, String> {
    let base = session
        .current_frame_json()
        .map_err(|_| format!("{label} failed to project its input boundary"))?;
    let base: serde_json::Value =
        serde_json::from_str(&base).map_err(|error| format!("{label} base frame: {error}"))?;
    let primary_ordinal = usize::try_from(
        base["trace_steps"]["primary_work"]["metric_ordinal"]
            .as_u64()
            .ok_or_else(|| format!("{label} omitted its primary work counter"))?,
    )
    .map_err(|_| format!("{label} primary work counter ordinal overflow"))?;
    let primary_work = base["metrics"]
        .get(primary_ordinal)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("{label} omitted its base primary work value"))?
        .parse::<u128>()
        .map_err(|error| format!("{label} base primary work: {error}"))?;
    let primary_work_unit = base["trace_steps"]["primary_work"]["unit"]
        .as_str()
        .filter(|unit| !unit.is_empty())
        .ok_or_else(|| format!("{label} omitted its primary work unit"))?
        .to_owned();
    let primary_work_abstraction = base["trace_steps"]["primary_work"]["abstraction"]
        .as_str()
        .filter(|abstraction| ["primitive", "iteration", "oracle-call"].contains(abstraction))
        .ok_or_else(|| format!("{label} omitted its primary work abstraction"))?
        .to_owned();
    let node_count = base["graph"]["nodes"]
        .as_array()
        .ok_or_else(|| format!("{label} omitted graph nodes"))?
        .len();
    let edge_count = base["graph"]["edges"]
        .as_array()
        .ok_or_else(|| format!("{label} omitted graph edges"))?
        .len();
    Ok(RepresentativeTraceBase {
        primary_ordinal,
        primary_work,
        primary_work_unit,
        primary_work_abstraction,
        node_count,
        edge_count,
        scenario_digest: digest_hex(source.as_bytes()),
    })
}

fn representative_final_frame(
    label: &str,
    session: &mut FlowSession,
) -> Result<serde_json::Value, String> {
    if session
        .stage_next_json()
        .map_err(|_| format!("{label} failed after its declared final event"))?
        .is_some()
    {
        return Err(format!("{label} emitted beyond its declared event_count"));
    }

    let final_frame = session
        .current_frame_json()
        .map_err(|_| format!("{label} failed to project its final frame"))?;
    let final_frame: serde_json::Value = serde_json::from_str(&final_frame)
        .map_err(|error| format!("{label} final frame JSON: {error}"))?;
    Ok(final_frame)
}

fn validate_representative_terminal(
    label: &str,
    final_frame: &serde_json::Value,
    event_count: usize,
    primary_ordinal: usize,
    base_primary_work: u128,
    audit: &RepresentativeBoundaryAuditState,
) -> Result<
    (
        usize,
        RepresentativeBoundaryWitness,
        RepresentativeBoundaryWitness,
        RepresentativeBoundaryWitness,
        RepresentativeBoundaryWitness,
    ),
    String,
> {
    let terminal_status = final_frame["solve_status"].as_str();
    let valid_terminal =
        terminal_status == Some("optimal") || terminal_status == Some("primitive-complete");
    if !valid_terminal || final_frame["outcome"].is_null() {
        return Err(format!(
            "{label} did not publish its certified terminal result"
        ));
    }
    let final_primary_work = final_frame["metrics"]
        .get(primary_ordinal)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("{label} omitted its final primary work value"))?
        .parse::<u128>()
        .map_err(|error| format!("{label} final primary work: {error}"))?;
    let measured_primary_work = final_primary_work
        .checked_sub(base_primary_work)
        .ok_or_else(|| format!("{label} primary work counter decreased"))?;
    if audit.primary_work != measured_primary_work {
        return Err(format!(
            "{label} attributed {} primary work units but its selected metric advanced by {measured_primary_work}",
            audit.primary_work
        ));
    }
    if measured_primary_work == 0 {
        return Err(format!(
            "{label} primary work witness did not advance on a meaningful source boundary"
        ));
    }
    if audit.published_transitions
        != u128::try_from(event_count).map_err(|_| format!("{label} event count overflow"))?
    {
        return Err(format!(
            "{label} publication work {} != event count {event_count}",
            audit.published_transitions
        ));
    }
    if audit.detail_primitives
        != u128::try_from(audit.granularity_counts[2])
            .map_err(|_| format!("{label} Detail count overflow"))?
    {
        return Err(format!(
            "{label} Detail work {} != Detail boundaries {}",
            audit.detail_primitives, audit.granularity_counts[2]
        ));
    }
    if audit.declared_primary_total != Some(measured_primary_work)
        || audit.declared_detail_total
            != Some(
                u128::try_from(event_count)
                    .map_err(|_| format!("{label} Detail total overflow"))?,
            )
    {
        return Err(format!(
            "{label} final work progress totals do not match the complete trace"
        ));
    }
    let distinct_actions = audit.actions.len();
    if distinct_actions < event_count.min(3) {
        return Err(format!(
            "{label} exposes only {distinct_actions} distinct actions across {event_count} steps"
        ));
    }
    let first_detail = audit
        .first_detail
        .clone()
        .ok_or_else(|| format!("{label} exposes no nonterminal Detail boundary"))?;
    let first_primary_work = audit
        .first_primary_work
        .clone()
        .ok_or_else(|| format!("{label} exposes no primary-work boundary"))?;
    let maximum_aggregation = audit
        .maximum_aggregation
        .clone()
        .ok_or_else(|| format!("{label} exposes no aggregation witness"))?;
    let maximum_primary_work = audit
        .maximum_primary_work
        .clone()
        .ok_or_else(|| format!("{label} exposes no primary-work witness"))?;
    Ok((
        distinct_actions,
        first_detail,
        first_primary_work,
        maximum_aggregation,
        maximum_primary_work,
    ))
}

fn validate_meaningful_primary_work_boundaries(
    label: &str,
    primary_work_abstraction: &str,
    audit: &RepresentativeBoundaryAuditState,
    maximum_primary_work: &RepresentativeBoundaryWitness,
) -> Result<u128, String> {
    if primary_work_abstraction == "oracle-call" {
        return Err(format!(
            "{label} stops its Detail witness at an opaque oracle call"
        ));
    }
    if !matches!(primary_work_abstraction, "primitive" | "iteration") {
        return Err(format!("{label} has an unknown primary work abstraction"));
    }
    if audit.primary_work_boundaries == 0 {
        return Err(format!(
            "{label} exposes no source boundary for {} measured primary-work units",
            audit.primary_work,
        ));
    }
    let minimum_geometric_boundaries = u128::from(u128::BITS - audit.primary_work.leading_zeros());
    if audit.primary_work_boundaries < minimum_geometric_boundaries {
        return Err(format!(
            "{label} compresses {} measured primary-work units into only {} source boundaries; at least {minimum_geometric_boundaries} source-time boundaries are required to preserve geometric complexity growth",
            audit.primary_work, audit.primary_work_boundaries,
        ));
    }
    if maximum_primary_work.primary_delta == 0 {
        return Err(format!(
            "{label} event {} ({}) omitted its measured source-boundary work",
            maximum_primary_work.event, maximum_primary_work.catalog_id,
        ));
    }
    Ok(maximum_primary_work.primary_delta)
}

fn update_representative_trace_digest(hasher: &mut Sha256, frame: &serde_json::Value) {
    hasher.update(frame.to_string().as_bytes());
}

fn representative_witness_json(witness: &RepresentativeBoundaryWitness) -> serde_json::Value {
    serde_json::json!({
        "event": witness.event,
        "catalog_id": witness.catalog_id,
        "primary_delta": witness.primary_delta.to_string(),
        "primary_completed": witness.primary_completed.to_string(),
        "detail_completed": witness.detail_completed.to_string(),
        "aggregation": witness.aggregation.to_string(),
        "work_deltas": witness.work_deltas.iter().map(|delta| serde_json::json!({
            "unit": delta.unit,
            "count": delta.count.to_string(),
        })).collect::<Vec<_>>(),
        "work_first": witness.work_first.map(|value| value.to_string()),
        "work_last": witness.work_last.map(|value| value.to_string()),
        "work_total": witness.work_total.map(|value| value.to_string()),
        "active_overlays": witness.active_overlays,
        "overlay_scalar_values": witness.overlay_scalar_values,
        "touched_identities": witness.touched_identities,
        "changed_identities": witness.changed_identities,
    })
}

fn representative_generator_sources(
    algorithm: AlgorithmId,
    compatible_family_ids: &[String],
) -> Vec<(String, String)> {
    const PREFERRED_FAMILIES: &[&str] = &[
        "parallel-paths",
        "dinic-worst-case",
        "planted-bottleneck",
        "diamond-chain",
        "netgen-skeleton",
        "cycle",
        "goto-torus",
        "hall-tight-bipartite",
        "washington-matching",
        "planar-triangulated",
        "assignment-matrix",
        "transportation-table",
    ];
    let mut sources = Vec::new();
    for family_id in PREFERRED_FAMILIES.iter().filter(|family_id| {
        compatible_family_ids
            .iter()
            .any(|candidate| candidate == **family_id)
    }) {
        let Some(fixture) = flow::generator_algorithm_fixture(family_id) else {
            continue;
        };
        for preset in fixture.presets.iter().take(2) {
            let source = representative_generator_scenario(algorithm, &fixture, preset);
            sources.push((
                format!("generator:{}:{}", fixture.family_id, preset.label),
                with_algorithm_and_profile(&source, algorithm, "trace"),
            ));
        }
    }
    sources
}

fn representative_controlled_size_sources(
    algorithm: AlgorithmId,
    canonical: &str,
) -> Vec<(String, String)> {
    // This source algorithm's declared numeric driver is log C.  Enlarging n
    // crosses its intentionally tiny exact-kernel budget before producing a
    // readable trace, so its two source-contract cost variants below form the
    // controlled family instead.
    if algorithm == AlgorithmId::DeterministicAlmostLinearMcf {
        return Vec::new();
    }
    let scenario: serde_json::Value =
        serde_json::from_str(canonical).expect("controlled representative is JSON");
    let baseline = scenario["payload"]["graph"]["nodes"]
        .as_array()
        .expect("controlled representative nodes")
        .len();
    let descriptor =
        flow::find_algorithm_by_id(algorithm).expect("controlled representative descriptor exists");
    let maximum = descriptor
        .admission_contract
        .max_nodes
        .map_or(descriptor.initial_band.max_nodes, |maximum| {
            maximum.min(descriptor.initial_band.max_nodes)
        });
    let maximum = usize::try_from(maximum).expect("catalog node limit fits usize");
    let mut targets = vec![
        baseline.saturating_add(1),
        baseline.saturating_add(3),
        baseline.saturating_add(baseline.max(6)),
        baseline.saturating_add(12),
        baseline.saturating_add(24),
    ];
    if algorithm == AlgorithmId::WeightedPushRelabel {
        // n=7 already crosses the bounded 32k-frame trace budget after exact
        // 256-inspection blocks; n=6 is the largest readable middle witness.
        targets.push(baseline.saturating_add(2));
    }
    targets
        .into_iter()
        .map(|target| target.min(maximum))
        .filter(|target| *target > baseline)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|target| {
            (
                format!("controlled-size-n{target}"),
                scenario_with_node_count(canonical, target),
            )
        })
        .collect()
}

fn representative_ssap_scale_variant(canonical: &str, path_count: usize) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(canonical).expect("SSAP representative is JSON");
    let mut nodes = vec![
        serde_json::json!({ "id": "s" }),
        serde_json::json!({ "id": "t" }),
    ];
    let mut edges = Vec::with_capacity(path_count * 2);
    for index in 0..path_count {
        let middle = format!("p{index:02}");
        nodes.push(serde_json::json!({ "id": middle.clone() }));
        edges.push(serde_json::json!({
            "id": format!("sp{index:02}"),
            "from": "s",
            "to": middle,
            "capacity": "1",
            "cost": (index + 1).to_string(),
        }));
        edges.push(serde_json::json!({
            "id": format!("p{index:02}t"),
            "from": middle,
            "to": "t",
            "capacity": "1",
            "cost": "0",
        }));
    }
    scenario["payload"]["graph"]["nodes"] = serde_json::Value::Array(nodes);
    scenario["payload"]["graph"]["edges"] = serde_json::Value::Array(edges);
    scenario.to_string()
}

fn representative_augmenting_electrical_structure_variant(
    canonical: &str,
    edge_count: usize,
) -> String {
    let mut scenario: serde_json::Value =
        serde_json::from_str(canonical).expect("augmenting-electrical representative is JSON");
    let edges = match edge_count {
        5 => serde_json::json!([
            { "id": "sa", "from": "s", "to": "a", "capacity": "8", "cost": "0" },
            { "id": "at", "from": "a", "to": "t", "capacity": "8", "cost": "0" },
            { "id": "sb", "from": "s", "to": "b", "capacity": "4", "cost": "0" },
            { "id": "bc", "from": "b", "to": "c", "capacity": "4", "cost": "0" },
            { "id": "ct", "from": "c", "to": "t", "capacity": "4", "cost": "0" }
        ]),
        6 => serde_json::json!([
            { "id": "sa", "from": "s", "to": "a", "capacity": "8", "cost": "0" },
            { "id": "at", "from": "a", "to": "t", "capacity": "8", "cost": "0" },
            { "id": "sb", "from": "s", "to": "b", "capacity": "4", "cost": "0" },
            { "id": "bt", "from": "b", "to": "t", "capacity": "4", "cost": "0" },
            { "id": "sc", "from": "s", "to": "c", "capacity": "1", "cost": "0" },
            { "id": "ct", "from": "c", "to": "t", "capacity": "1", "cost": "0" }
        ]),
        _ => panic!("unsupported augmenting-electrical representative edge count"),
    };
    scenario["payload"]["graph"] = serde_json::json!({
        "nodes": [
            { "id": "s" }, { "id": "a" }, { "id": "b" },
            { "id": "c" }, { "id": "t" }
        ],
        "edges": edges
    });
    scenario.to_string()
}

fn representative_generator_scenario(
    algorithm: AlgorithmId,
    fixture: &flow::GeneratorAlgorithmFixtureV1,
    preset: &flow::GeneratorPresetV1,
) -> String {
    let source = generator_fixture_scenario(fixture, preset);
    if fixture.family_id != "cycle" {
        return source;
    }
    let mut scenario: serde_json::Value =
        serde_json::from_str(&source).expect("cycle representative is JSON");
    scenario["payload"]["model"] = serde_json::json!({ "kind": "transshipment" });
    let nodes = scenario["payload"]["graph"]["nodes"]
        .as_array_mut()
        .expect("cycle representative nodes");
    let demand_index = nodes.len() / 2;
    for node in nodes.iter_mut() {
        node["supply"] = serde_json::json!("0");
    }
    nodes[0]["supply"] = serde_json::json!("4");
    nodes[demand_index]["supply"] = serde_json::json!("-4");
    let expose_negative_cycle = matches!(
        algorithm,
        AlgorithmId::SimpleCycleCanceling
            | AlgorithmId::MinimumMeanCycleCanceling
            | AlgorithmId::CancelAndTighten
            | AlgorithmId::RelaxedMostNegativeCycle
    );
    for (index, edge) in scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("cycle representative edges")
        .iter_mut()
        .enumerate()
    {
        edge["lower"] = serde_json::json!("0");
        edge["capacity"] = serde_json::json!("8");
        edge["cost"] = if expose_negative_cycle {
            serde_json::json!("-1")
        } else {
            serde_json::json!((1 + index % 3).to_string())
        };
    }
    scenario.to_string()
}

fn representative_transportation_matrix_variant(source: &str, size: usize) -> String {
    assert!(size >= 2);
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("transportation representative is JSON");
    let origins = (0..size)
        .map(|index| format!("o{index}"))
        .collect::<Vec<_>>();
    let destinations = (0..size)
        .map(|index| format!("d{index}"))
        .collect::<Vec<_>>();
    let nodes = origins
        .iter()
        .map(|id| serde_json::json!({ "id": id, "supply": size.to_string() }))
        .chain(
            destinations
                .iter()
                .map(|id| serde_json::json!({ "id": id, "supply": format!("-{size}") })),
        )
        .collect::<Vec<_>>();
    let total_shipment = size
        .checked_mul(size)
        .expect("small matrix size")
        .to_string();
    let edges = origins
        .iter()
        .enumerate()
        .flat_map(|(origin_index, from)| {
            let total_shipment = total_shipment.clone();
            destinations
                .iter()
                .enumerate()
                .map(move |(destination_index, to)| {
                    serde_json::json!({
                        "id": format!("route-{origin_index}-{destination_index}"),
                        "from": from,
                        "to": to,
                        "capacity": total_shipment,
                        "cost": (1 + (origin_index * 7 + destination_index * 3) % 17).to_string()
                    })
                })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["model"] = serde_json::json!({
        "kind": "transportation",
        "origins": origins,
        "destinations": destinations
    });
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn representative_primal_dual_ipm_path_variant(source: &str, edge_count: usize) -> String {
    assert!(edge_count >= 1);
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("primal-dual IPM representative is JSON");
    let node_ids = std::iter::once("s".to_owned())
        .chain((1..edge_count).map(|index| format!("p{index}")))
        .chain(std::iter::once("t".to_owned()))
        .collect::<Vec<_>>();
    let nodes = node_ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "supply": if id == "s" { "2" } else if id == "t" { "-2" } else { "0" }
            })
        })
        .collect::<Vec<_>>();
    let edges = node_ids
        .windows(2)
        .enumerate()
        .map(|(index, endpoints)| {
            serde_json::json!({
                "id": format!("path-{index}"),
                "from": &endpoints[0],
                "to": &endpoints[1],
                "capacity": "2",
                "cost": (index + 1).to_string()
            })
        })
        .collect::<Vec<_>>();
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn representative_electrical_ipm_capacity_variant(source: &str, maximum_capacity: usize) -> String {
    assert!(maximum_capacity >= 3);
    let work_rich =
        representative_work_rich_variant(source, AlgorithmId::ElectricalFlowInteriorPointMcf)
            .expect("electrical IPM representative exists");
    let mut scenario: serde_json::Value =
        serde_json::from_str(&work_rich).expect("electrical IPM representative is JSON");
    scenario["payload"]["graph"]["edges"][0]["capacity"] =
        serde_json::json!(maximum_capacity.to_string());
    scenario.to_string()
}

fn representative_bounded_mcf_face_variant(
    source: &str,
    algorithm: AlgorithmId,
    extra_parallel_edges: usize,
) -> String {
    let work_rich = representative_work_rich_variant(source, algorithm)
        .expect("bounded MCF face representative exists");
    let mut scenario: serde_json::Value =
        serde_json::from_str(&work_rich).expect("bounded MCF face is JSON");
    let edges = scenario["payload"]["graph"]["edges"]
        .as_array_mut()
        .expect("bounded MCF face edges");
    for index in 0..extra_parallel_edges {
        edges.push(serde_json::json!({
            "id": format!("sa-extra-{index}"),
            "from": "s",
            "to": "a",
            "capacity": "1",
            "cost": (4 + index).to_string()
        }));
    }
    scenario.to_string()
}

fn representative_binary_zero_scc_variant(source: &str, internal_nodes: usize) -> String {
    assert!(internal_nodes >= 2);
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("binary blocking representative is JSON");
    let internal = (0..internal_nodes)
        .map(|index| format!("z{index:02}"))
        .collect::<Vec<_>>();
    let nodes = std::iter::once("s".to_owned())
        .chain(internal.iter().cloned())
        .chain(std::iter::once("t".to_owned()))
        .map(|id| serde_json::json!({ "id": id }))
        .collect::<Vec<_>>();
    let mut edges = vec![serde_json::json!({
        "id": "source-link",
        "from": "s",
        "to": &internal[0],
        "capacity": "12",
        "cost": "0"
    })];
    for (index, pair) in internal.windows(2).enumerate() {
        edges.push(serde_json::json!({
            "id": format!("forward-{index:02}"),
            "from": &pair[0],
            "to": &pair[1],
            "capacity": "12",
            "cost": "0"
        }));
        edges.push(serde_json::json!({
            "id": format!("reverse-{index:02}"),
            "from": &pair[1],
            "to": &pair[0],
            "capacity": "12",
            "cost": "0"
        }));
    }
    edges.push(serde_json::json!({
        "id": "sink-link",
        "from": internal.last().expect("nonempty internal path"),
        "to": "t",
        "capacity": "12",
        "cost": "0"
    }));
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn representative_convex_cost_scaling_path_variant(source: &str, internal_nodes: usize) -> String {
    assert!(internal_nodes >= 2);
    let mut scenario: serde_json::Value =
        serde_json::from_str(source).expect("convex scaling representative is JSON");
    let node_ids = std::iter::once("s".to_owned())
        .chain((0..internal_nodes).map(|index| format!("v{index}")))
        .chain(std::iter::once("t".to_owned()))
        .collect::<Vec<_>>();
    let nodes = node_ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            serde_json::json!({
                "id": id,
                "supply": if index == 0 { "4" } else if index + 1 == node_ids.len() { "-4" } else { "0" }
            })
        })
        .collect::<Vec<_>>();
    let mut edges = node_ids
        .windows(2)
        .enumerate()
        .map(|(index, pair)| {
            serde_json::json!({
                "id": format!("path-{index}"),
                "from": &pair[0],
                "to": &pair[1],
                "lower": "0",
                "capacity": "4",
                "cost": (1 + index % 3).to_string()
            })
        })
        .collect::<Vec<_>>();
    edges.push(serde_json::json!({
        "id": "direct",
        "from": "s",
        "to": "t",
        "capacity": "4",
        "cost": "0",
        "convex_cost": {
            "base_cost_at_zero": "0",
            "segments": [
                { "end_flow": "1", "marginal_cost": "-2" },
                { "end_flow": "2", "marginal_cost": "2" },
                { "end_flow": "4", "marginal_cost": "9" }
            ]
        }
    }));
    scenario["payload"]["graph"] = serde_json::json!({ "nodes": nodes, "edges": edges });
    scenario.to_string()
}

fn representative_targeted_complexity_sources(
    algorithm: AlgorithmId,
    canonical: &str,
) -> Vec<(String, String)> {
    match algorithm {
        AlgorithmId::TransportationSimplex | AlgorithmId::Modi => [5, 6]
            .into_iter()
            .map(|size| {
                (
                    format!("transportation-matrix-{size}"),
                    representative_transportation_matrix_variant(canonical, size),
                )
            })
            .collect(),
        AlgorithmId::PrimalDualInteriorPointMcf => [1, 2]
            .into_iter()
            .map(|edge_count| {
                (
                    format!("ipm-path-{edge_count}"),
                    representative_primal_dual_ipm_path_variant(canonical, edge_count),
                )
            })
            .collect(),
        AlgorithmId::ElectricalFlowInteriorPointMcf => [3, 4]
            .into_iter()
            .map(|maximum_capacity| {
                (
                    format!("electrical-ipm-capacity-{maximum_capacity}"),
                    representative_electrical_ipm_capacity_variant(canonical, maximum_capacity),
                )
            })
            .collect(),
        AlgorithmId::MinimumRatioCycleMcf | AlgorithmId::RandomizedAlmostLinearMcf => [0, 1]
            .into_iter()
            .map(|extra| {
                (
                    format!("bounded-face-extra-{extra}"),
                    representative_bounded_mcf_face_variant(canonical, algorithm, extra),
                )
            })
            .collect(),
        AlgorithmId::BinaryBlockingFlow => [2, 3]
            .into_iter()
            .map(|internal_nodes| {
                (
                    format!("binary-zero-scc-{internal_nodes}"),
                    representative_binary_zero_scc_variant(canonical, internal_nodes),
                )
            })
            .collect(),
        AlgorithmId::ConvexCostScaling => [4, 5]
            .into_iter()
            .map(|internal_nodes| {
                (
                    format!("convex-scaling-path-{internal_nodes}"),
                    representative_convex_cost_scaling_path_variant(canonical, internal_nodes),
                )
            })
            .collect(),
        AlgorithmId::AugmentingElectricalFlow => [5, 6]
            .into_iter()
            .map(|edge_count| {
                (
                    format!("electrical-structure-{edge_count}"),
                    representative_augmenting_electrical_structure_variant(canonical, edge_count),
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn representative_trace_candidates(
    algorithm: AlgorithmId,
    canonical: &str,
    compatible_family_ids: &[String],
) -> Vec<(String, String)> {
    let mut candidates = vec![("canonical".to_owned(), canonical.to_owned())];
    if let Some(source) = representative_work_rich_variant(canonical, algorithm) {
        candidates.push(("work-rich-structure".to_owned(), source.clone()));
        let (label, variant) = match algorithm {
            AlgorithmId::Hungarian | AlgorithmId::Auction => ("work-rich-cost-variant", 2),
            _ => ("work-rich-capacity-variant", 1),
        };
        candidates.push((
            label.to_owned(),
            representative_numeric_variant(&source, variant),
        ));
    }
    candidates.extend(representative_targeted_complexity_sources(
        algorithm, canonical,
    ));
    candidates.extend(representative_controlled_size_sources(algorithm, canonical));
    let requires_source_specific_representatives =
        requires_source_specific_representatives(algorithm);
    // Generator presets can drive the literal weighted relabel kernel to tens
    // of thousands of semantic source events.  Its canonical and numeric
    // variants already expose >19k measured arc inspections, so retain those
    // bounded, readable witnesses instead of turning the visual audit into a
    // resource-limit benchmark.
    let prefers_bounded_numeric_representatives = algorithm == AlgorithmId::WeightedAugmentingPaths;
    if !requires_source_specific_representatives && !prefers_bounded_numeric_representatives {
        candidates.extend(representative_generator_sources(
            algorithm,
            compatible_family_ids,
        ));
    }
    candidates.extend(representative_structural_variants(algorithm, canonical));
    candidates
}

fn representative_structural_variants(
    algorithm: AlgorithmId,
    canonical: &str,
) -> Vec<(String, String)> {
    let mut variants = Vec::new();
    let requires_source_specific_representatives =
        requires_source_specific_representatives(algorithm);
    if matches!(
        algorithm,
        AlgorithmId::UnitCapacityDinic
            | AlgorithmId::UnitNetworkDinic
            | AlgorithmId::InteriorPointMaxFlow
    ) {
        for path_count in [2, 3] {
            variants.push((
                format!("unit-network-{path_count}-paths"),
                representative_unit_network_variant(canonical, algorithm, path_count),
            ));
        }
    } else if matches!(
        algorithm,
        AlgorithmId::ParametricPseudoflow | AlgorithmId::ParametricBreakpointRerun
    ) {
        variants.push((
            "parameter-range-variant".to_owned(),
            representative_parametric_variant(canonical, 1),
        ));
        variants.push((
            "capacity-slope-variant".to_owned(),
            representative_parametric_variant(canonical, 2),
        ));
    } else if algorithm == AlgorithmId::ConvexNetworkSimplex {
        for edge_count in [12, 24] {
            variants.push((
                format!("convex-parallel-{edge_count}"),
                representative_convex_network_simplex_parallel_variant(canonical, edge_count),
            ));
        }
    } else if matches!(
        algorithm,
        AlgorithmId::SegmentExpandedConvexMcf | AlgorithmId::ConvexCostScaling
    ) {
        for multiplier in [2, 3] {
            variants.push((
                format!("convex-cost-x{multiplier}"),
                representative_convex_cost_variant(canonical, multiplier),
            ));
        }
    } else if algorithm == AlgorithmId::SuccessiveShortestAugmentingPath {
        for path_count in [3, 5] {
            variants.push((
                format!("parallel-cost-paths-{path_count}"),
                representative_ssap_scale_variant(canonical, path_count),
            ));
        }
    } else if requires_source_specific_representatives {
        for variant in [1, 2] {
            variants.push((
                format!("source-contract-variant-{variant}"),
                representative_source_specific_variant(canonical, algorithm, variant),
            ));
        }
    } else {
        let variant_ids = if matches!(
            algorithm,
            AlgorithmId::Hungarian
                | AlgorithmId::Auction
                | AlgorithmId::DeterministicAlmostLinearMcf
        ) {
            [2, 6]
        } else if matches!(
            algorithm,
            AlgorithmId::ElectricalFlow
                | AlgorithmId::AugmentingElectricalFlow
                | AlgorithmId::InteriorPointMaxFlow
                | AlgorithmId::DynamicEibfs
                | AlgorithmId::WarmStartPushRelabel
        ) {
            [1, 3]
        } else {
            [1, 2]
        };
        for variant in variant_ids {
            variants.push((
                format!("numeric-variant-{variant}"),
                representative_numeric_variant(canonical, variant),
            ));
        }
    }
    variants
}

const fn requires_source_specific_representatives(algorithm: AlgorithmId) -> bool {
    matches!(
        algorithm,
        AlgorithmId::DynamicEibfs
            | AlgorithmId::WarmStartPushRelabel
            | AlgorithmId::ExcessScalingMcf
            | AlgorithmId::PolynomialDualNetworkSimplex
            | AlgorithmId::TardosFramework
            | AlgorithmId::DeterministicAlmostLinearMcf
            | AlgorithmId::PredictionAssistedEpsilonRelaxation
    )
}

fn representative_trace_summary(audit: &RepresentativeTraceAudit) -> String {
    format!(
        "{}:n{}m{}:{}/{}[P{} O{} D{} W{} B{} A{}]",
        audit.label,
        audit.node_count,
        audit.edge_count,
        audit.event_count,
        audit.distinct_actions,
        audit.granularity_counts[0],
        audit.granularity_counts[1],
        audit.granularity_counts[2],
        audit.primary_work,
        audit.primary_work_boundaries,
        audit.maximum_aggregation.aggregation,
    )
}

fn declared_numeric_complexity_driver(
    algorithm: AlgorithmId,
    audit: &RepresentativeTraceAudit,
) -> Option<(&'static str, u128)> {
    let scenario: serde_json::Value = serde_json::from_str(&audit.source).ok()?;
    let edges = scenario["payload"]["graph"]["edges"].as_array()?;
    match algorithm {
        AlgorithmId::DeterministicAlmostLinearMcf => edges
            .iter()
            .filter_map(|edge| {
                edge["cost"]
                    .as_str()?
                    .parse::<i128>()
                    .ok()
                    .map(i128::unsigned_abs)
            })
            .max()
            .map(|value| ("maximum-absolute-cost", value)),
        AlgorithmId::ConvexCostScaling => edges
            .iter()
            .filter_map(|edge| edge["capacity"].as_str()?.parse::<u128>().ok())
            .max()
            .map(|value| ("maximum-capacity", value)),
        AlgorithmId::ElectricalFlowInteriorPointMcf => edges
            .iter()
            .filter_map(|edge| edge["capacity"].as_str()?.parse::<u128>().ok())
            .max()
            .map(|value| ("maximum-capacity", value)),
        _ => None,
    }
}

fn representative_complexity_family(label: &str) -> String {
    if let Some(rest) = label.strip_prefix("generator:") {
        return format!("generator:{}", rest.split(':').next().unwrap_or("unknown"));
    }
    for (prefix, family) in [
        ("unit-network-", "unit-network"),
        ("parallel-cost-paths-", "parallel-cost-paths"),
        ("work-rich-", "work-rich"),
        ("source-contract-", "source-contract"),
        ("controlled-size-", "controlled-size"),
        ("convex-cost-", "convex-cost"),
        ("convex-parallel-", "convex-parallel"),
        ("convex-scaling-path-", "convex-scaling-path"),
        ("parameter-range-", "parametric-contract"),
        ("capacity-slope-", "parametric-contract"),
        ("numeric-variant-", "canonical-numeric"),
        ("transportation-matrix-", "transportation-matrix"),
        ("ipm-path-", "ipm-path"),
        ("electrical-ipm-capacity-", "electrical-ipm-capacity"),
        ("bounded-face-extra-", "bounded-face"),
        ("electrical-structure-", "electrical-structure"),
        ("binary-zero-scc-", "binary-zero-scc"),
    ] {
        if label.starts_with(prefix) {
            return family.to_owned();
        }
    }
    label.to_owned()
}

#[allow(clippy::too_many_lines)]
fn verify_complexity_growth_witness(
    algorithm: AlgorithmId,
    admitted: &[RepresentativeTraceAudit],
) -> Result<RepresentativeComplexityGrowthWitness, String> {
    let witness = admitted.iter().flat_map(|smaller| {
        admitted.iter().filter_map(move |larger| {
            let smaller_graph_entities = u128::try_from(smaller.node_count)
                .ok()?
                .checked_add(u128::try_from(smaller.edge_count).ok()?)?;
            let larger_graph_entities = u128::try_from(larger.node_count)
                .ok()?
                .checked_add(u128::try_from(larger.edge_count).ok()?)?;
            let structural_growth = (smaller_graph_entities < larger_graph_entities)
                .then_some((smaller_graph_entities, larger_graph_entities));
            let numeric_growth = declared_numeric_complexity_driver(algorithm, smaller)
                .zip(declared_numeric_complexity_driver(algorithm, larger))
                .filter(
                    |((smaller_name, smaller_driver), (larger_name, larger_driver))| {
                        smaller_name == larger_name && smaller_driver < larger_driver
                    },
                );
            (smaller.scenario_digest != larger.scenario_digest
                && (structural_growth.is_some() || numeric_growth.is_some())
                && smaller.primary_work < larger.primary_work)
                .then_some((
                    smaller,
                    larger,
                    structural_growth,
                    numeric_growth,
                    smaller.control_digest.is_some()
                        && smaller.control_digest == larger.control_digest
                        && smaller.control_contract == larger.control_contract,
                ))
        })
    });
    let Some((smaller, larger, structural_growth, numeric_growth, controlled)) = witness
        .max_by_key(|(smaller, larger, _, _, controlled)| {
            (
                *controlled,
                larger.primary_work - smaller.primary_work,
                larger.event_count,
            )
        })
    else {
        return Err(format!(
            "{} has no graph-entity-count or declared-numeric complexity-growth witness whose driver and measured work both increase: {}",
            algorithm.as_str(),
            admitted
                .iter()
                .map(representative_trace_summary)
                .collect::<Vec<_>>()
                .join(",")
        ));
    };
    if !controlled {
        return Err(format!(
            "{} has no complexity-growth pair produced by one typed scale contract",
            algorithm.as_str(),
        ));
    }
    let smaller_work_scale = u128::BITS - smaller.primary_work.leading_zeros();
    let larger_work_scale = u128::BITS - larger.primary_work.leading_zeros();
    if larger_work_scale > smaller_work_scale
        && larger.primary_work_boundaries <= smaller.primary_work_boundaries
        && larger.event_count <= smaller.event_count
    {
        return Err(format!(
            "{} measured work grows from {} to {} across a complexity scale, but neither source primary-work boundaries ({} -> {}) nor visible source events ({} -> {}) grow",
            algorithm.as_str(),
            smaller.primary_work,
            larger.primary_work,
            smaller.primary_work_boundaries,
            larger.primary_work_boundaries,
            smaller.event_count,
            larger.event_count,
        ));
    }
    let (driver, smaller_driver, larger_driver) =
        if let Some((smaller_driver, larger_driver)) = structural_growth {
            ("graph-entity-count", smaller_driver, larger_driver)
        } else {
            let ((driver, smaller_driver), (_, larger_driver)) =
                numeric_growth.expect("non-structural witness must own a declared numeric driver");
            (driver, smaller_driver, larger_driver)
        };
    Ok(RepresentativeComplexityGrowthWitness {
        driver: driver.to_owned(),
        controlled_family: if controlled {
            representative_complexity_family(&smaller.label)
        } else {
            "cross-family".to_owned()
        },
        controlled,
        control_contract: smaller
            .control_contract
            .clone()
            .expect("controlled witness owns a typed scale contract"),
        control_digest: smaller
            .control_digest
            .clone()
            .expect("controlled witness owns a scale-pair provenance digest"),
        smaller_driver,
        larger_driver,
        smaller_label: smaller.label.clone(),
        larger_label: larger.label.clone(),
        smaller_primary_work: smaller.primary_work,
        larger_primary_work: larger.primary_work,
        smaller_primary_boundaries: smaller.primary_work_boundaries,
        larger_primary_boundaries: larger.primary_work_boundaries,
        smaller_event_count: smaller.event_count,
        larger_event_count: larger.event_count,
        smaller_detail: smaller.event_count,
        larger_detail: larger.event_count,
        smaller_nodes: smaller.node_count,
        larger_nodes: larger.node_count,
        smaller_edges: smaller.edge_count,
        larger_edges: larger.edge_count,
    })
}

fn select_representative_trio(
    algorithm: AlgorithmId,
    candidates: &[RepresentativeTraceAudit],
) -> Option<(
    Vec<RepresentativeTraceAudit>,
    RepresentativeComplexityGrowthWitness,
)> {
    let canonical = candidates
        .iter()
        .position(|candidate| candidate.label == "canonical")?;
    for left in 0..candidates.len() {
        for right in (left + 1)..candidates.len() {
            if left == canonical || right == canonical {
                continue;
            }
            let trio = vec![
                candidates[canonical].clone(),
                candidates[left].clone(),
                candidates[right].clone(),
            ];
            let rich = trio
                .iter()
                .filter(|audit| audit.primary_work >= MINIMUM_COMPLEXITY_WITNESS_WORK)
                .count();
            if rich < 2 {
                continue;
            }
            if let Ok(growth) = verify_complexity_growth_witness(algorithm, &trio)
                && growth.controlled
            {
                return Some((trio, growth));
            }
        }
    }
    None
}

fn verify_representative_boundary_stability(
    algorithm: AlgorithmId,
    admitted: &[RepresentativeTraceAudit],
) -> Result<(), String> {
    let mut stable_action_boundaries = std::collections::BTreeMap::new();
    for audit in admitted {
        for (catalog_id, boundary) in &audit.action_boundaries {
            if let Some(previous) =
                stable_action_boundaries.insert(catalog_id.as_str(), boundary.as_str())
                && previous != boundary
            {
                return Err(format!(
                    "{} action {catalog_id} changed boundary from {previous} to {boundary} across representative traces",
                    algorithm.as_str()
                ));
            }
        }
    }
    Ok(())
}

fn verify_declared_boundary_availability(
    algorithm: AlgorithmId,
    admitted: &[RepresentativeTraceAudit],
) -> Result<(), String> {
    let descriptor = flow::find_algorithm_by_id(algorithm)
        .ok_or_else(|| format!("{} is missing from the catalog", algorithm.as_str()))?;
    for (label, index, availability) in [
        ("Phase", 0, descriptor.trace_steps.phase_availability),
        (
            "Operation",
            1,
            descriptor.trace_steps.operation_availability,
        ),
    ] {
        let observed = admitted
            .iter()
            .filter(|audit| audit.granularity_counts[index] > 0)
            .count();
        match availability {
            flow::AlgorithmStepAvailabilityV1::Available if observed == 0 => {
                return Err(format!(
                    "{} declares {label} available but none of its representative traces publishes one",
                    algorithm.as_str()
                ));
            }
            flow::AlgorithmStepAvailabilityV1::Unavailable { .. } if observed != 0 => {
                return Err(format!(
                    "{} declares {label} unavailable but {observed} representative traces publish one",
                    algorithm.as_str()
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

const MINIMUM_COMPLEXITY_WITNESS_WORK: u128 = 12;

#[allow(
    clippy::too_many_lines,
    reason = "the representative audit keeps candidate selection and all cross-trace invariants together"
)]
fn audit_algorithm_representatives(
    algorithm: AlgorithmId,
    compatible_family_ids: &[String],
    verbose: bool,
) -> Result<
    (
        Vec<RepresentativeTraceAudit>,
        RepresentativeComplexityGrowthWitness,
    ),
    String,
> {
    let canonical = conformance_scenario(algorithm, "trace");
    let candidates = representative_trace_candidates(algorithm, &canonical, compatible_family_ids);
    let mut admitted = Vec::new();
    let mut rejected = Vec::new();
    for (label, source) in candidates {
        if verbose {
            eprintln!(
                "FLOW_REPRESENTATIVE_CANDIDATE\t{}\t{label}",
                algorithm.as_str()
            );
        }
        match audit_representative_trace(algorithm, &label, &source, &canonical) {
            Ok(result) => admitted.push(result),
            Err(error) => {
                if verbose {
                    eprintln!(
                        "FLOW_REPRESENTATIVE_REJECTED\t{}\t{label}\t{error}",
                        algorithm.as_str()
                    );
                }
                rejected.push(error);
            }
        }
        if select_representative_trio(algorithm, &admitted).is_some() {
            break;
        }
    }
    let Some((admitted, growth_witness)) = select_representative_trio(algorithm, &admitted) else {
        return Err(format!(
            "{} cannot select canonical plus two work-rich traces with a strict measured-work growth witness from {} admitted candidates: {}; rejected: {}",
            algorithm.as_str(),
            admitted.len(),
            admitted
                .iter()
                .map(representative_trace_summary)
                .collect::<Vec<_>>()
                .join(","),
            rejected.join(" | ")
        ));
    };
    if !admitted.iter().any(|audit| audit.label == "canonical") {
        return Err(format!(
            "{} canonical trace failed the representative gate: {}",
            algorithm.as_str(),
            rejected.join(" | ")
        ));
    }
    if admitted
        .iter()
        .map(|audit| audit.scenario_digest.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != admitted.len()
    {
        return Err(format!(
            "{} admitted duplicate representative graph/config digests",
            algorithm.as_str()
        ));
    }
    if admitted
        .iter()
        .map(|audit| audit.trace_digest.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != admitted.len()
    {
        return Err(format!(
            "{} admitted representatives with indistinguishable semantic trace signatures: {}",
            algorithm.as_str(),
            admitted
                .iter()
                .map(|audit| format!("{}={}", audit.label, audit.trace_digest))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    verify_representative_boundary_stability(algorithm, &admitted)?;
    verify_declared_boundary_availability(algorithm, &admitted)?;
    let richest = admitted
        .iter()
        .map(|audit| audit.event_count)
        .max()
        .expect("three admitted traces");
    if admitted
        .iter()
        .filter(|audit| audit.primary_work >= MINIMUM_COMPLEXITY_WITNESS_WORK)
        .count()
        < 2
    {
        let richest = admitted
            .iter()
            .max_by_key(|audit| (audit.primary_work, audit.primary_work_boundaries))
            .expect("three admitted traces");
        return Err(format!(
            "{} has fewer than two nontrivial complexity witnesses: its richest representative exposes {} measured {} across {} meaningful source events; the fixture-quality floor is {MINIMUM_COMPLEXITY_WITNESS_WORK} measured units on real source actions; representatives: {}",
            algorithm.as_str(),
            richest.primary_work,
            richest.primary_work_unit,
            richest.event_count,
            admitted
                .iter()
                .map(representative_trace_summary)
                .collect::<Vec<_>>()
                .join(","),
        ));
    }
    println!(
        "FLOW_REPRESENTATIVE_AUDIT\t{}\t{}\t{}\t{}",
        algorithm.as_str(),
        richest,
        admitted
            .iter()
            .map(|audit| audit.label.as_str())
            .collect::<Vec<_>>()
            .join(","),
        admitted
            .iter()
            .map(representative_trace_summary)
            .collect::<Vec<_>>()
            .join(",")
    );
    Ok((admitted, growth_witness))
}

/// Exercises the mathematical-infeasibility branch of the shared auxiliary
/// constructor through the same `FlowSession` composition path used by the UI.
/// Successful representative algorithms cannot cover the residual-cut scan,
/// so this is a separate contract witness rather than a fake fourth member of
/// any algorithm's complexity-growth trio.
fn audit_infeasible_feasibility_composition() -> Result<std::collections::BTreeSet<String>, String>
{
    const LABEL: &str = "shared-feasibility-infeasible-cut";
    let mut scenario: serde_json::Value = serde_json::from_str(&conformance_scenario(
        AlgorithmId::PotentialDijkstraSsp,
        "trace",
    ))
    .expect("potential-Dijkstra conformance fixture is JSON");
    // The canonical fixture has total source-to-sink capacity seven. Requiring
    // eight keeps the Scenario structurally valid while forcing the actual
    // lower-bound feasibility kernel to publish its residual-cut witness.
    scenario["payload"]["model"]["required_flow"] = serde_json::json!("8");
    let source = scenario.to_string();
    validate_flow_session_input(&source)
        .map_err(|error| format!("{LABEL} failed runtime admission: {error}"))?;
    let mut session =
        FlowSession::new(&source).map_err(|_| format!("{LABEL} failed runtime admission"))?;
    let base = representative_trace_base(LABEL, &source, &session)?;
    let first = session
        .stage_next_json()
        .map_err(|_| format!("{LABEL} failed trace preparation"))?
        .ok_or_else(|| format!("{LABEL} produced no event"))?;
    let first: serde_json::Value =
        serde_json::from_str(&first).map_err(|error| format!("{LABEL} first frame: {error}"))?;
    let event_count = first["event_count"]
        .as_str()
        .ok_or_else(|| format!("{LABEL} omitted event_count"))?
        .parse::<usize>()
        .map_err(|error| format!("{LABEL} event_count: {error}"))?;
    let mut audit = RepresentativeBoundaryAuditState::default();
    audit_representative_frame(
        AlgorithmId::PotentialDijkstraSsp,
        LABEL,
        1,
        &first,
        &mut audit,
    )?;
    session.commit_staged_next();
    for index in 2..=event_count {
        let frame = session
            .stage_next_json()
            .map_err(|_| format!("{LABEL} failed at event {index}"))?
            .ok_or_else(|| format!("{LABEL} ended before event {index}"))?;
        let frame: serde_json::Value = serde_json::from_str(&frame)
            .map_err(|error| format!("{LABEL} frame {index}: {error}"))?;
        audit_representative_frame(
            AlgorithmId::PotentialDijkstraSsp,
            LABEL,
            index,
            &frame,
            &mut audit,
        )?;
        session.commit_staged_next();
    }
    let final_frame = representative_final_frame(LABEL, &mut session)?;
    if final_frame["solve_status"] != "infeasible" || final_frame["outcome"].is_null() {
        return Err(format!(
            "{LABEL} did not publish its certified infeasible terminal result"
        ));
    }
    let final_primary_work = final_frame["metrics"]
        .get(base.primary_ordinal)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("{LABEL} omitted its final primary work value"))?
        .parse::<u128>()
        .map_err(|error| format!("{LABEL} final primary work: {error}"))?;
    let measured_primary_work = final_primary_work
        .checked_sub(base.primary_work)
        .ok_or_else(|| format!("{LABEL} primary work counter decreased"))?;
    if audit.primary_work != measured_primary_work
        || audit.published_transitions
            != u128::try_from(event_count).map_err(|_| format!("{LABEL} event count overflow"))?
        || audit.detail_primitives
            != u128::try_from(audit.granularity_counts[2])
                .map_err(|_| format!("{LABEL} Detail count overflow"))?
    {
        return Err(format!(
            "{LABEL} work attribution does not match its composed trace"
        ));
    }
    let observed = audit
        .action_boundaries
        .iter()
        .filter(|(_, boundary)| boundary.as_str() == "micro")
        .map(|(catalog_id, _)| catalog_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if !observed.contains("feasibility.inspect-cut-arc") {
        return Err(format!(
            "{LABEL} omitted the residual-cut arc inspection that certifies infeasibility"
        ));
    }
    Ok(observed)
}

#[test]
fn infeasible_feasibility_composition_publishes_its_cut_scan() {
    audit_infeasible_feasibility_composition().expect("infeasible cut trace contract");
}

#[test]
#[ignore = "intentional long-running 93-algorithm representative graph audit"]
#[allow(
    clippy::too_many_lines,
    reason = "the release audit owns one closed 93-endpoint sweep and its schema-16 manifest"
)]
fn every_algorithm_has_multiple_readable_representative_traces() {
    let contracts = flow_algorithm_conformance_contracts().expect("conformance contracts");
    let selected_algorithm = std::env::var("FLOW_REPRESENTATIVE_ALGORITHM").ok();
    let start_algorithm = std::env::var("FLOW_REPRESENTATIVE_START").ok();
    let mut reached_start = start_algorithm.is_none();
    let mut violations = Vec::new();
    let mut audited_algorithms = 0_usize;
    let mut browser_cases = Vec::new();
    let mut complexity_growth = Vec::new();
    let mut observed_detail_catalog_ids = std::collections::BTreeSet::new();
    let mut observed_primary_work_catalog_ids = std::collections::BTreeSet::new();
    for (algorithm, contract) in AlgorithmId::ALL.iter().copied().zip(&contracts) {
        if !reached_start {
            reached_start = start_algorithm.as_deref() == Some(algorithm.as_str());
            if !reached_start {
                continue;
            }
        }
        if selected_algorithm
            .as_deref()
            .is_some_and(|selected| selected != algorithm.as_str())
        {
            continue;
        }
        audited_algorithms += 1;
        match audit_algorithm_representatives(
            algorithm,
            &contract.compatible_generator_fixture_ids,
            selected_algorithm.is_some(),
        ) {
            Ok((admitted, growth)) => {
                complexity_growth.push(serde_json::json!({
                    "algorithm_id": algorithm.as_str(),
                    "driver": growth.driver,
                    "controlled_family": growth.controlled_family,
                    "controlled": growth.controlled,
                    "control_contract": growth.control_contract,
                    "control_digest": growth.control_digest,
                    "smaller_driver": growth.smaller_driver.to_string(),
                    "larger_driver": growth.larger_driver.to_string(),
                    "smaller_label": growth.smaller_label,
                    "larger_label": growth.larger_label,
                    "smaller_primary_work": growth.smaller_primary_work.to_string(),
                    "larger_primary_work": growth.larger_primary_work.to_string(),
                    "smaller_primary_boundary_count": growth.smaller_primary_boundaries.to_string(),
                    "larger_primary_boundary_count": growth.larger_primary_boundaries.to_string(),
                    "smaller_event_count": growth.smaller_event_count,
                    "larger_event_count": growth.larger_event_count,
                    "smaller_detail_count": growth.smaller_detail,
                    "larger_detail_count": growth.larger_detail,
                    "smaller_node_count": growth.smaller_nodes,
                    "larger_node_count": growth.larger_nodes,
                    "smaller_edge_count": growth.smaller_edges,
                    "larger_edge_count": growth.larger_edges,
                }));
                for audit in admitted {
                    observed_primary_work_catalog_ids
                        .extend(audit.primary_work_actions.iter().cloned());
                    observed_detail_catalog_ids.extend(
                        audit
                            .action_boundaries
                            .iter()
                            .filter(|(catalog_id, boundary)| {
                                boundary.as_str() == "micro"
                                    && !catalog_id.ends_with(".work-observation")
                            })
                            .map(|(catalog_id, _)| catalog_id.clone()),
                    );
                    browser_cases.push(serde_json::json!({
                        "algorithm_id": algorithm.as_str(),
                        "label": audit.label,
                        "node_count": audit.node_count,
                        "edge_count": audit.edge_count,
                        "event_count": audit.event_count,
                        "phase_count": audit.granularity_counts[0],
                        "operation_count": audit.granularity_counts[1],
                        "detail_count": audit.event_count,
                        "primary_work": audit.primary_work.to_string(),
                        "primary_work_boundary_count": audit.primary_work_boundaries.to_string(),
                        "primary_work_unit": audit.primary_work_unit,
                        "primary_work_abstraction": audit.primary_work_abstraction,
                        "maximum_primary_work_delta": audit.maximum_primary_work_delta.to_string(),
                        "first_detail": representative_witness_json(&audit.first_detail),
                        "middle_detail": representative_witness_json(&audit.middle_detail),
                        "last_detail": representative_witness_json(&audit.last_detail),
                        "first_primary_work": representative_witness_json(&audit.first_primary_work),
                        "maximum_aggregation": representative_witness_json(&audit.maximum_aggregation),
                        "maximum_primary_work": representative_witness_json(&audit.maximum_primary_work),
                        "overlay_witnesses": audit.overlay_witnesses.iter().map(|(field, witness)| {
                            (field.clone(), representative_witness_json(witness))
                        }).collect::<std::collections::BTreeMap<_, _>>(),
                        "scenario_digest": audit.scenario_digest,
                        "trace_digest": audit.trace_digest,
                        "scenario": serde_json::from_str::<serde_json::Value>(&audit.source)
                            .expect("audited representative remains JSON"),
                    }));
                }
            }
            Err(error) => violations.push(error),
        }
    }
    if selected_algorithm.is_none() && start_algorithm.is_none() {
        match audit_infeasible_feasibility_composition() {
            Ok(observed) => observed_detail_catalog_ids.extend(observed),
            Err(error) => violations.push(error),
        }
        let declared = crate::flow_trace_contract::SOURCE_DETAIL_PRIMITIVE_CATALOG_IDS
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let primary_boundary_actions =
            crate::flow_trace_contract::SOURCE_PRIMARY_WORK_BOUNDARY_CATALOG_IDS
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
        let observed = observed_detail_catalog_ids
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        if declared != observed {
            let undeclared = observed
                .difference(&declared)
                .copied()
                .filter(|catalog_id| {
                    !primary_boundary_actions.contains(*catalog_id)
                        && !observed_primary_work_catalog_ids.contains(*catalog_id)
                })
                .collect::<Vec<_>>();
            let unobserved = declared.difference(&observed).collect::<Vec<_>>();
            if !unobserved.is_empty() || !undeclared.is_empty() {
                violations.push(format!(
                    "Detail primitive registry mismatch; unobserved={unobserved:?}, undeclared={undeclared:?}",
                ));
            }
        }
    }
    assert!(
        reached_start,
        "FLOW_REPRESENTATIVE_START names no catalog algorithm"
    );
    assert!(
        audited_algorithms > 0,
        "representative audit selected no catalog algorithm"
    );
    if selected_algorithm.is_none() && start_algorithm.is_none() {
        assert_eq!(
            audited_algorithms,
            AlgorithmId::ALL.len(),
            "full representative audit did not cover the complete catalog"
        );
    }
    assert!(
        violations.is_empty(),
        "representative trace audit failed:\n{}",
        violations.join("\n")
    );
    write_representative_browser_manifest(
        selected_algorithm.as_deref(),
        start_algorithm.as_deref(),
        &browser_cases,
        &complexity_growth,
    );
}

fn write_representative_browser_manifest(
    selected_algorithm: Option<&str>,
    start_algorithm: Option<&str>,
    browser_cases: &[serde_json::Value],
    complexity_growth: &[serde_json::Value],
) {
    let Some(path) = std::env::var_os("FLOW_REPRESENTATIVE_MANIFEST") else {
        return;
    };
    assert!(
        selected_algorithm.is_none() && start_algorithm.is_none(),
        "a browser manifest can only be written by the complete audit"
    );
    assert_eq!(
        browser_cases.len(),
        AlgorithmId::ALL.len() * 3,
        "browser manifest must contain three cases per algorithm"
    );
    assert_eq!(
        complexity_growth.len(),
        AlgorithmId::ALL.len(),
        "browser manifest must contain one complexity-growth witness per algorithm"
    );
    let manifest = serde_json::json!({
        "schema_version": 17,
        "algorithm_count": AlgorithmId::ALL.len(),
        "cases_per_algorithm": 3,
        "complexity_growth": complexity_growth,
        "cases": browser_cases,
    });
    let path = std::path::PathBuf::from(path);
    let path = if path.is_absolute() {
        path
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("browser manifest parent directory creates");
    }
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&manifest).expect("browser manifest serializes"),
    )
    .expect("browser manifest writes");
}
