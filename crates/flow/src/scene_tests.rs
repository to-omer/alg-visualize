use std::collections::BTreeMap;

use super::*;
use crate::feasibility::{
    CapturedFeasibilityRequest, FeasibilityTraceDirection, FeasibilityTraceOutcome,
    apply_feasibility_trace_event, trace_feasible_flow,
};
use crate::model::{FlowNode, NodeId, UnresolvedFlowEdge};
use crate::scenario::{FlowEdgeV1, FlowNodeV1, FlowPositionV1};

#[test]
fn ready_scene_uses_exact_decimal_cursor_and_metric_values() {
    let scene = FlowCurrentSceneV9::ready(
        FlowProblemModelV1::MaxFlow {
            source: "s".to_owned(),
            sink: "t".to_owned(),
        },
        FlowGraphV1 {
            nodes: vec![FlowNodeV1 {
                id: "s".to_owned(),
                supply: "0".to_owned(),
                position: Some(FlowPositionV1 {
                    x: "0".to_owned(),
                    y: "0".to_owned(),
                }),
            }],
            edges: vec![],
        },
        FlowAlgorithmSelectionV1 {
            id: "edmonds-karp".to_owned(),
            config: BTreeMap::new(),
        },
        RunProfileV1::Trace,
        TraceGranularityV1::Operation,
        crate::catalog::find_algorithm("edmonds-karp")
            .expect("test algorithm exists")
            .trace_steps,
    );
    let value = serde_json::to_value(scene).expect("scene serializes");

    assert_eq!(value["event_id"], "0");
    assert_eq!(value["event_count"], "0");
    assert_eq!(value["edge_states"].as_array().map(Vec::len), Some(0));
    assert!(value.get("outcome").is_none());
    assert_eq!(value["metrics"].as_array().map(Vec::len), Some(16));
    assert!(
        value["metrics"]
            .as_array()
            .expect("metrics is an array")
            .iter()
            .all(|metric| metric == "0")
    );
}

#[test]
fn trace_metric_projection_preserves_all_sixteen_slot_identities() {
    let projected = trace_metrics(FlowTraceMetrics {
        bfs_runs: 1,
        relaxation_passes: 2,
        residual_arc_scans: 3,
        augmentations: 4,
        path_searches: 5,
        scaling_phases: 6,
        blocking_flow_phases: 7,
        relabels: 8,
        retreats: 9,
        reverse_bfs_runs: 10,
        gap_terminations: 11,
        pushes: 12,
        saturating_pushes: 13,
        nonsaturating_pushes: 14,
        discharges: 15,
        active_vertex_selections: 16,
    });
    assert_eq!(
        projected,
        std::array::from_fn(|index| (index + 1).to_string())
    );
}

#[test]
fn trace_projection_uses_the_snapshot_current_capacity() {
    let source = NodeId::parse("s").expect("source");
    let sink = NodeId::parse("t").expect("sink");
    let edge_id = EdgeId::parse("e").expect("edge");
    let graph = FlowNetwork::new(
        vec![
            FlowNode::new(source.clone(), 0),
            FlowNode::new(sink.clone(), 0),
        ],
        vec![UnresolvedFlowEdge {
            id: edge_id.clone(),
            from: source,
            to: sink,
            lower: 0,
            capacity: 5,
            cost: 0,
        }],
    )
    .expect("graph");
    let mut state = ResidualState::at_lower_bounds(&graph);
    state
        .set_current_capacity(&edge_id, 3)
        .expect("capacity update");
    let snapshot = FlowTraceSnapshot::capture(
        &graph,
        &state,
        vec![None; 2],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        FlowTraceMetrics::default(),
    );
    let mut scene = FlowCurrentSceneV9::ready(
        FlowProblemModelV1::MaxFlow {
            source: "s".to_owned(),
            sink: "t".to_owned(),
        },
        FlowGraphV1 {
            nodes: ["s", "t"]
                .into_iter()
                .map(|id| FlowNodeV1 {
                    id: id.to_owned(),
                    supply: "0".to_owned(),
                    position: None,
                })
                .collect(),
            edges: vec![FlowEdgeV1 {
                id: "e".to_owned(),
                from: "s".to_owned(),
                to: "t".to_owned(),
                lower: "0".to_owned(),
                capacity: "5".to_owned(),
                cost: "0".to_owned(),
                convex_cost: None,
                initial_flow: None,
            }],
        },
        FlowAlgorithmSelectionV1 {
            id: "dynamic-eibfs".to_owned(),
            config: BTreeMap::new(),
        },
        RunProfileV1::Trace,
        TraceGranularityV1::Operation,
        crate::catalog::find_algorithm("dynamic-eibfs")
            .expect("test algorithm exists")
            .trace_steps,
    );

    scene
        .apply_trace_snapshot(&graph, &snapshot, None, 0)
        .expect("projection");
    assert_eq!(scene.graph.edges[0].capacity, "3");
    assert_eq!(scene.residual_arcs[0].capacity, "3");
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one projection transcript checks auxiliary topology, local focus, and terminal cleanup together"
)]
fn feasibility_projection_exposes_artificial_topology_and_local_push_focus() {
    let source = NodeId::parse("s").expect("source");
    let sink = NodeId::parse("t").expect("sink");
    let graph = FlowNetwork::new(
        vec![
            FlowNode::new(source.clone(), 0),
            FlowNode::new(sink.clone(), 0),
        ],
        vec![UnresolvedFlowEdge {
            id: EdgeId::parse("e").expect("edge"),
            from: source,
            to: sink,
            lower: 1,
            capacity: 3,
            cost: 2,
        }],
    )
    .expect("graph");
    let required = [2, -2];
    let request = CapturedFeasibilityRequest::Balance {
        required_divergence: required.to_vec(),
    };
    let traced = trace_feasible_flow(&graph, &required).expect("feasibility trace");
    assert!(matches!(
        traced.outcome,
        FeasibilityTraceOutcome::Feasible(_)
    ));
    let mut scene = FlowCurrentSceneV9::ready(
        FlowProblemModelV1::Transshipment {},
        FlowGraphV1 {
            nodes: ["s", "t"]
                .into_iter()
                .map(|id| FlowNodeV1 {
                    id: id.to_owned(),
                    supply: if id == "s" { "2" } else { "-2" }.to_owned(),
                    position: None,
                })
                .collect(),
            edges: vec![FlowEdgeV1 {
                id: "e".to_owned(),
                from: "s".to_owned(),
                to: "t".to_owned(),
                lower: "1".to_owned(),
                capacity: "3".to_owned(),
                cost: "2".to_owned(),
                convex_cost: None,
                initial_flow: None,
            }],
        },
        FlowAlgorithmSelectionV1 {
            id: "cost-scaling".to_owned(),
            config: BTreeMap::new(),
        },
        RunProfileV1::Trace,
        TraceGranularityV1::Micro,
        crate::catalog::find_algorithm("cost-scaling")
            .expect("test algorithm exists")
            .trace_steps,
    );
    scene
        .apply_feasibility_trace_snapshot(
            &graph,
            &request,
            &traced.trace.base_snapshot,
            None,
            0,
            u64::try_from(traced.trace.events.len()).expect("bounded event count"),
        )
        .expect("base projection");
    let base = scene
        .feasibility_overlay
        .as_ref()
        .expect("feasibility overlay");
    assert_eq!(base.stage, FlowFeasibilityStageV1::Ready);
    assert_eq!(base.nodes.len(), graph.nodes().len() + 2);
    assert!(base.arcs.is_empty());

    let mut replay = traced.trace.base_snapshot.clone();
    let mut saw_push = false;
    for (index, event) in traced.trace.events.iter().enumerate() {
        apply_feasibility_trace_event(&mut replay, event, FeasibilityTraceDirection::Forward)
            .expect("source replay");
        scene
            .apply_feasibility_trace_snapshot(
                &graph,
                &request,
                &replay,
                Some(event),
                u64::try_from(index + 1).expect("bounded event id"),
                u64::try_from(traced.trace.events.len()).expect("bounded event count"),
            )
            .expect("event projection");
        if event.kind == FeasibilityTraceEventKind::Push {
            let overlay = scene
                .feasibility_overlay
                .as_ref()
                .expect("feasibility overlay");
            assert_eq!(overlay.stage, FlowFeasibilityStageV1::Push);
            assert!(overlay.focus_arc.is_some());
            assert_eq!(overlay.arcs.iter().filter(|arc| arc.focused).count(), 1);
            assert_eq!(scene.metrics[11], overlay.metrics.pushes);
            saw_push = true;
        }
    }
    assert!(saw_push);
    assert_eq!(replay, traced.trace.final_snapshot);
    assert_eq!(scene.edge_states[0].flow, "2");
    let final_overlay = scene
        .feasibility_overlay
        .as_ref()
        .expect("final feasibility overlay");
    assert_eq!(final_overlay.stage, FlowFeasibilityStageV1::Feasible);
    assert!(
        final_overlay
            .arcs
            .iter()
            .any(|arc| { arc.arc.kind == FlowFeasibilityArcKindV1::FromSuperSource })
    );
    assert!(
        final_overlay
            .arcs
            .iter()
            .any(|arc| { arc.arc.kind == FlowFeasibilityArcKindV1::ToSuperSink })
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn polynomial_dual_overlay_rejects_bad_subtree_and_tree_support_drift() {
    let node_ids = ["a", "b", "c"].map(|id| NodeId::parse(id).expect("node id"));
    let graph = FlowNetwork::new(
        node_ids
            .iter()
            .cloned()
            .map(|id| FlowNode::new(id, 0))
            .collect(),
        [
            ("eab", 0_usize, 1_usize),
            ("ebc", 1_usize, 2_usize),
            ("eca", 2_usize, 0_usize),
        ]
        .into_iter()
        .map(|(id, from, to)| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge id"),
            from: node_ids[from].clone(),
            to: node_ids[to].clone(),
            lower: 0,
            capacity: 5,
            cost: 0,
        })
        .collect(),
    )
    .expect("graph");
    let zero = FlowRationalV1 {
        numerator: "0".to_owned(),
        denominator: "1".to_owned(),
    };
    let one = FlowRationalV1 {
        numerator: "1".to_owned(),
        denominator: "1".to_owned(),
    };
    let mut overlay = FlowPolynomialDualSimplexOverlayV1 {
        stage: FlowPolynomialDualSimplexStageV1::InitializePseudoflow,
        phase: "0".to_owned(),
        delta: one.clone(),
        nodes: ["a", "b", "c"]
            .into_iter()
            .enumerate()
            .map(|(index, id)| FlowPolynomialDualNodeStateV1 {
                node_id: id.to_owned(),
                potential: "0".to_owned(),
                excess: zero.clone(),
                root: index == 0,
                active: false,
                bad: false,
                in_pivot_cut: false,
            })
            .collect(),
        edges: ["eab", "ebc", "eca"]
            .into_iter()
            .enumerate()
            .map(|(index, id)| FlowPolynomialDualEdgeStateV1 {
                edge_id: id.to_owned(),
                pseudoflow: if index < 2 { one.clone() } else { zero.clone() },
                basic_flow: if index < 2 { "1" } else { "0" }.to_owned(),
                reduced_cost: "0".to_owned(),
                in_tree: index < 2,
                bad: false,
                in_augment_path: false,
                augment_direction: None,
            })
            .collect(),
        active_node: None,
        augment_path: Vec::new(),
        bad_edges: Vec::new(),
        bad_nodes: Vec::new(),
        leaving_edge: None,
        entering_edge: None,
        pivot_cut: Vec::new(),
        pivot_price_delta: None,
    };
    validate_polynomial_dual_simplex_overlay(&graph, &overlay)
        .expect("tree-supported positive pseudoflow is valid");

    let mut active = overlay.clone();
    active.stage = FlowPolynomialDualSimplexStageV1::SelectActive;
    active.active_node = Some("c".to_owned());
    active.nodes[2].active = true;
    active.augment_path = vec![
        FlowResidualArcRefV1 {
            edge_id: "ebc".to_owned(),
            direction: "reverse".to_owned(),
        },
        FlowResidualArcRefV1 {
            edge_id: "eab".to_owned(),
            direction: "reverse".to_owned(),
        },
    ];
    for (index, direction) in [(1_usize, "reverse"), (0_usize, "reverse")] {
        active.edges[index].in_augment_path = true;
        active.edges[index].augment_direction = Some(direction.to_owned());
    }
    validate_polynomial_dual_simplex_overlay(&graph, &active)
        .expect("ordered reverse tree path reaches the root");
    active.augment_path[1].direction = "forward".to_owned();
    active.edges[0].augment_direction = Some("forward".to_owned());
    assert_eq!(
        validate_polynomial_dual_simplex_overlay(&graph, &active),
        Err(FlowSceneError::SnapshotGraphMismatch)
    );

    overlay.edges[0].pseudoflow = zero.clone();
    assert_eq!(
        validate_polynomial_dual_simplex_overlay(&graph, &overlay),
        Err(FlowSceneError::SnapshotGraphMismatch)
    );
    overlay.edges[0].pseudoflow = one;
    overlay.edges[2].in_tree = true;
    assert_eq!(
        validate_polynomial_dual_simplex_overlay(&graph, &overlay),
        Err(FlowSceneError::SnapshotGraphMismatch)
    );
}

#[test]
fn polynomial_primal_overlay_rejects_disconnected_basis_and_artificial_flag_drift() {
    let node_ids = ["a", "b", "c"].map(|id| NodeId::parse(id).expect("node id"));
    let graph = FlowNetwork::new(
        node_ids
            .iter()
            .cloned()
            .map(|id| FlowNode::new(id, 0))
            .collect(),
        [
            ("eab", 0_usize, 1_usize),
            ("ebc", 1_usize, 2_usize),
            ("eca", 2_usize, 0_usize),
        ]
        .into_iter()
        .map(|(id, from, to)| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge id"),
            from: node_ids[from].clone(),
            to: node_ids[to].clone(),
            lower: 0,
            capacity: 5,
            cost: 0,
        })
        .collect(),
    )
    .expect("graph");
    let zero = FlowRationalV1 {
        numerator: "0".to_owned(),
        denominator: "1".to_owned(),
    };
    let mut overlay = FlowPolynomialPrimalSimplexOverlayV1 {
        stage: FlowPolynomialPrimalSimplexStageV1::InitializeBasis,
        phase: "0".to_owned(),
        epsilon: None,
        perturbation_scale: "4".to_owned(),
        nodes: node_ids
            .iter()
            .map(|id| FlowPolynomialPrimalNodeStateV1 {
                entity_id: id.as_str().to_owned(),
                kind: FlowPolynomialPrimalNodeKindV1::Original,
                premultiplier: zero.clone(),
                flags: Vec::new(),
            })
            .chain(std::iter::once(FlowPolynomialPrimalNodeStateV1 {
                entity_id: "artificial-root".to_owned(),
                kind: FlowPolynomialPrimalNodeKindV1::ArtificialRoot,
                premultiplier: zero.clone(),
                flags: vec![FlowPolynomialPrimalNodeFlagV1::Root],
            }))
            .collect(),
        edges: ["eab", "ebc", "eca"]
            .into_iter()
            .map(|id| FlowPolynomialPrimalEdgeStateV1 {
                edge_id: id.to_owned(),
                basis: FlowPolynomialPrimalBasisStateV1::Lower,
                perturbed_flow: "0".to_owned(),
                unperturbed_basic_flow: "0".to_owned(),
                reduced_cost: zero.clone(),
                in_cycle: false,
                entering: false,
                leaving: false,
            })
            .collect(),
        artificial_edges: ["a", "b", "c"]
            .into_iter()
            .map(|id| FlowPolynomialPrimalArtificialEdgeStateV1 {
                entity_id: format!("artificial:{id}"),
                node_id: id.to_owned(),
                basis: FlowPolynomialPrimalBasisStateV1::Tree,
                perturbed_flow: "1".to_owned(),
                unperturbed_basic_flow: "0".to_owned(),
                in_cycle: false,
                entering: false,
                leaving: false,
            })
            .collect(),
        entering: None,
        leaving_entity: None,
        cycle: Vec::new(),
        delta: None,
        potential_shift: None,
    };
    validate_polynomial_primal_simplex_overlay(&graph, &overlay)
        .expect("artificial star basis is valid");

    overlay.artificial_edges[0].in_cycle = true;
    assert_eq!(
        validate_polynomial_primal_simplex_overlay(&graph, &overlay),
        Err(FlowSceneError::SnapshotGraphMismatch)
    );
    overlay.artificial_edges[0].in_cycle = false;
    for edge in &mut overlay.edges {
        edge.basis = FlowPolynomialPrimalBasisStateV1::Tree;
        edge.perturbed_flow = "1".to_owned();
    }
    for edge in &mut overlay.artificial_edges {
        edge.basis = FlowPolynomialPrimalBasisStateV1::Lower;
        edge.perturbed_flow = "0".to_owned();
    }
    assert_eq!(
        validate_polynomial_primal_simplex_overlay(&graph, &overlay),
        Err(FlowSceneError::SnapshotGraphMismatch)
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn flow_framework_overlay_preserves_node_projection_and_rejects_corruption_atomically() {
    let node_ids = ["a", "b", "c"].map(|id| NodeId::parse(id).expect("node id"));
    let edge_specs = [
        ("eab", 0_usize, 1_usize),
        ("ebc", 1_usize, 2_usize),
        ("eca", 2_usize, 0_usize),
    ];
    let graph = FlowNetwork::new(
        node_ids
            .iter()
            .cloned()
            .map(|id| FlowNode::new(id, 0))
            .collect(),
        edge_specs
            .iter()
            .map(|&(id, from, to)| UnresolvedFlowEdge {
                id: EdgeId::parse(id).expect("edge id"),
                from: node_ids[from].clone(),
                to: node_ids[to].clone(),
                lower: 0,
                capacity: 5,
                cost: 1,
            })
            .collect(),
    )
    .expect("graph");
    let graph_scene = FlowGraphV1 {
        nodes: ["a", "b", "c"]
            .into_iter()
            .map(|id| FlowNodeV1 {
                id: id.to_owned(),
                supply: "0".to_owned(),
                position: None,
            })
            .collect(),
        edges: edge_specs
            .into_iter()
            .map(|(id, from, to)| FlowEdgeV1 {
                id: id.to_owned(),
                from: node_ids[from].as_str().to_owned(),
                to: node_ids[to].as_str().to_owned(),
                lower: "0".to_owned(),
                capacity: "5".to_owned(),
                cost: "1".to_owned(),
                convex_cost: None,
                initial_flow: None,
            })
            .collect(),
    };
    let zero = FlowRationalV1 {
        numerator: "0".to_owned(),
        denominator: "1".to_owned(),
    };
    let mut overlay = FlowFrameworkMcfOverlayV1 {
        stage: FlowFrameworkMcfStageV1::InitializeSourcePoint,
        dynamic_operation: None,
        dynamic_operation_serial: None,
        iteration: "0".to_owned(),
        reinitialized: false,
        potential_before: "0".to_owned(),
        potential_after: "0".to_owned(),
        gap_before: "0".to_owned(),
        gap_after: "0".to_owned(),
        exact_gap_before: zero.clone(),
        exact_gap_after: zero.clone(),
        stopping_gap: FlowRationalV1 {
            numerator: "1".to_owned(),
            denominator: "2".to_owned(),
        },
        accepted_ratio: zero.clone(),
        target_progress: zero.clone(),
        termination: None,
        optimum_cost: None,
        final_point_nodes: None,
        final_point_edges: None,
        levels: [0, 1]
            .into_iter()
            .map(|level| FlowFrameworkMcfLevelStateV1 {
                level: level.to_string(),
                active_branch: "0".to_owned(),
                passes: "0".to_owned(),
            })
            .collect(),
        edges: ["eab", "ebc", "eca"]
            .into_iter()
            .map(|edge_id| FlowFrameworkMcfEdgeStateV1 {
                edge_id: edge_id.to_owned(),
                flow: zero.clone(),
                cycle_coefficient: zero.clone(),
                selected: false,
            })
            .collect(),
    };
    let mut scene = FlowCurrentSceneV9::ready(
        FlowProblemModelV1::Transshipment {},
        graph_scene,
        FlowAlgorithmSelectionV1 {
            id: "deterministic-almost-linear-mcf".to_owned(),
            config: BTreeMap::new(),
        },
        RunProfileV1::Trace,
        TraceGranularityV1::Operation,
        crate::catalog::find_algorithm("deterministic-almost-linear-mcf")
            .expect("test algorithm exists")
            .trace_steps,
    );

    scene
        .apply_flow_framework_mcf_boundary(&graph, &[0, 0, 0], overlay.clone(), 1, 4)
        .expect("valid initial source projection");
    assert_eq!(scene.node_trace_states.len(), graph.nodes().len());
    let committed = serde_json::to_value(&scene).expect("committed scene serializes");

    let mut invalid_gate = overlay.clone();
    invalid_gate.stopping_gap = FlowRationalV1 {
        numerator: "2".to_owned(),
        denominator: "3".to_owned(),
    };
    assert_eq!(
        scene.apply_flow_framework_mcf_boundary(&graph, &[0, 0, 0], invalid_gate, 2, 4),
        Err(FlowSceneError::SnapshotGraphMismatch)
    );
    assert_eq!(
        serde_json::to_value(&scene).expect("rejected gate serializes"),
        committed
    );

    overlay.edges[0].selected = true;
    overlay.edges[0].cycle_coefficient = FlowRationalV1 {
        numerator: "1".to_owned(),
        denominator: "1".to_owned(),
    };
    assert_eq!(
        scene.apply_flow_framework_mcf_boundary(&graph, &[0, 0, 0], overlay, 2, 4),
        Err(FlowSceneError::SnapshotGraphMismatch)
    );
    assert_eq!(
        serde_json::to_value(&scene).expect("rejected scene serializes"),
        committed
    );
}

#[test]
fn convex_scaling_overlay_recomputes_exact_delta_eligible_boundaries() {
    let source = NodeId::parse("s").expect("source");
    let sink = NodeId::parse("t").expect("sink");
    let graph = FlowNetwork::new(
        vec![
            FlowNode::new(source.clone(), 0),
            FlowNode::new(sink.clone(), 0),
        ],
        vec![UnresolvedFlowEdge {
            id: EdgeId::parse("edge").expect("edge id"),
            from: source,
            to: sink,
            lower: 1,
            capacity: 4,
            cost: 0,
        }],
    )
    .expect("graph");
    let segments = vec![
        FlowConvexCostSegmentStateV1 {
            segment: "0".to_owned(),
            start_flow: "0".to_owned(),
            end_flow: "1".to_owned(),
            flow: "1".to_owned(),
            marginal_cost: "-2".to_owned(),
        },
        FlowConvexCostSegmentStateV1 {
            segment: "1".to_owned(),
            start_flow: "1".to_owned(),
            end_flow: "4".to_owned(),
            flow: "1".to_owned(),
            marginal_cost: "3".to_owned(),
        },
    ];
    let eligible = FlowConvexCostArcRefV1 {
        edge_id: "edge".to_owned(),
        segment: "1".to_owned(),
        direction: "forward".to_owned(),
    };
    let mut overlay = FlowConvexCostOverlayV1 {
        stage: FlowConvexCostStageV1::StartScale,
        scale: Some("2".to_owned()),
        edges: vec![FlowConvexCostEdgeStateV1 {
            edge_id: "edge".to_owned(),
            base_cost_at_zero: "0".to_owned(),
            flow: "2".to_owned(),
            total_cost: "1".to_owned(),
            forward_marginal_cost: Some("3".to_owned()),
            reverse_marginal_cost: Some("3".to_owned()),
            segments,
        }],
        active_cycle: Vec::new(),
        eligible_arcs: vec![eligible.clone()],
    };
    validate_convex_cost_overlay(&graph, &[2], &overlay)
        .expect("exact forward boundary is eligible at delta two");

    overlay.eligible_arcs.clear();
    assert_eq!(
        validate_convex_cost_overlay(&graph, &[2], &overlay),
        Err(FlowSceneError::SnapshotGraphMismatch)
    );
    overlay.eligible_arcs = vec![eligible.clone(), eligible];
    assert_eq!(
        validate_convex_cost_overlay(&graph, &[2], &overlay),
        Err(FlowSceneError::SnapshotGraphMismatch)
    );
}
