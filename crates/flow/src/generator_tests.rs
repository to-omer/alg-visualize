use super::*;
use crate::algorithms::{
    HungarianOutcome, TransportationError, check_transportation_infeasibility,
    solve_borradaile_klein_planar, solve_cost_scaling, solve_edmonds_karp, solve_fifo_push_relabel,
    solve_generic_push_relabel, solve_global_relabel_push_relabel, solve_hassin_st_planar,
    solve_highest_label_push_relabel, solve_hungarian, solve_transportation_simplex,
    trace_edmonds_karp, trace_successive_shortest_path,
};
use crate::certificate::{check_min_cost_flow, fixed_flow_divergences};
use crate::feasibility::FeasibilityError;
use crate::generator_fixture::{
    GeneratorAlgorithmFixtureV1, GeneratorModelKindV1, generator_algorithm_fixtures,
};
use crate::model::{EdgeId, FlowNetwork, FlowNode, NodeId, UnresolvedFlowEdge};
use crate::scenario::TraceGranularityV1;
use visualizer_core::jcs::{canonicalize, sha256_hex};

#[derive(Clone, Copy)]
struct ReferenceArc {
    to: usize,
    reverse: usize,
    capacity: u64,
}

#[test]
fn every_generator_family_has_one_canonical_source_policy_record() {
    assert_eq!(FLOW_GENERATOR_FAMILY_IDS.len(), 50);
    assert!(
        FLOW_GENERATOR_FAMILY_IDS
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    );

    let registry = include_str!("../../../docs/flow-sources.md");
    let family_section = registry
        .split_once("## Generator family records")
        .expect("generator family section")
        .1;
    let mut documented = family_section
        .lines()
        .filter_map(|line| {
            line.strip_prefix("| `")
                .and_then(|rest| rest.split_once("` |").map(|(id, _)| id))
        })
        .collect::<Vec<_>>();
    documented.sort_unstable();
    assert_eq!(documented, FLOW_GENERATOR_FAMILY_IDS);
}

fn spec(family: FlowGeneratorFamilyV1) -> FlowGeneratorSpecV1 {
    FlowGeneratorSpecV1 {
        generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
        seed: "42".to_owned(),
        family,
        capacity: CapacityDistributionV1::Uniform {
            minimum: "1".to_owned(),
            maximum: "9".to_owned(),
        },
        cost: CostDistributionV1::Uniform {
            minimum: "-3".to_owned(),
            maximum: "5".to_owned(),
        },
        target_problem: None,
    }
}

fn fixed_spec(family: FlowGeneratorFamilyV1) -> FlowGeneratorSpecV1 {
    FlowGeneratorSpecV1 {
        generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
        seed: "42".to_owned(),
        family,
        capacity: CapacityDistributionV1::Unit {},
        cost: CostDistributionV1::Zero {},
        target_problem: None,
    }
}

fn generated_semantic_digest(generated: &GeneratedFlowGraphV1) -> String {
    let semantic = serde_json::json!({
        "graph": &generated.graph,
        "suggested_model": &generated.suggested_model,
    });
    let encoded = serde_json::to_vec(&semantic).expect("generated semantic payload serializes");
    let canonical = canonicalize(&encoded).expect("generated semantic payload canonicalizes");
    sha256_hex(&canonical)
}

#[test]
fn readable_random_defaults_have_nontrivial_answers_and_traces() {
    assert_readable_max_flow_default();
    assert_readable_min_cost_default();
}

#[test]
fn generic_topologies_materialize_explicit_cross_problem_models() {
    assert_layered_topology_materializes_fixed_flow_min_cost();
    assert_cycle_topology_materializes_max_flow();
    assert_terminal_topologies_materialize_fixed_flow_min_cost();
    assert_transshipment_topology_materializes_max_flow();
}

fn assert_layered_topology_materializes_fixed_flow_min_cost() {
    let mut layered = spec(FlowGeneratorFamilyV1::LayeredDag {
        layers: 4,
        width: 3,
        fanout: 2,
    });
    let native_layered =
        generate_flow_graph(&layered).expect("layered topology preserves its native model");
    layered.target_problem = Some(FlowGeneratorTargetProblemV1::FixedFlowMinCost);
    let generated =
        generate_flow_graph(&layered).expect("layered topology adapts to fixed-flow MCF");
    let FlowProblemModelV1::FixedFlowMinCost {
        source,
        sink,
        required_flow,
    } = &generated.suggested_model
    else {
        panic!("adapted layered graph must use fixed-flow min-cost model");
    };
    assert_eq!((source.as_str(), sink.as_str()), ("s", "t"));
    let adapted_network = canonical_network(&generated.graph);
    let source_index = adapted_network
        .node_index(&NodeId::parse(source).expect("source id"))
        .expect("source node");
    let sink_index = adapted_network
        .node_index(&NodeId::parse(sink).expect("sink id"))
        .expect("sink node");
    let independent_maximum = solve_edmonds_karp(&adapted_network, source_index, sink_index)
        .expect("independent maximum-flow value");
    assert_eq!(
        required_flow.parse::<i128>().expect("required flow"),
        independent_maximum.certificate.value
    );
    assert_eq!(
        generated.provenance.parameters.get("target_problem"),
        Some(&serde_json::json!("fixed-flow-min-cost"))
    );
    assert_eq!(
        serde_json::to_value(&native_layered.graph).expect("native graph serializes"),
        serde_json::to_value(&generated.graph).expect("adapted graph serializes")
    );
    assert_ne!(
        serde_json::to_value(&native_layered.suggested_model).expect("native model serializes"),
        serde_json::to_value(&generated.suggested_model).expect("adapted model serializes")
    );
    assert_ne!(
        native_layered.provenance.materialized_sha256,
        generated.provenance.materialized_sha256
    );
    for candidate in [&native_layered, &generated] {
        assert_eq!(
            candidate.provenance.materialized_sha256,
            generated_semantic_digest(candidate),
            "materialized digest must cover the final graph and adapted model"
        );
    }
}

fn assert_cycle_topology_materializes_max_flow() {
    let mut cycle = spec(FlowGeneratorFamilyV1::Cycle { nodes: 8 });
    cycle.target_problem = Some(FlowGeneratorTargetProblemV1::MaxFlow);
    let generated = generate_flow_graph(&cycle).expect("cycle topology adapts to max flow");
    let FlowProblemModelV1::MaxFlow { source, sink } = &generated.suggested_model else {
        panic!("adapted cycle must use max-flow model");
    };
    assert_ne!(source, sink);
    assert!(generated.graph.nodes.iter().all(|node| node.supply == "0"));
    let network = canonical_network(&generated.graph);
    let source_index = network
        .node_index(&NodeId::parse(source).expect("source id"))
        .expect("source node");
    let sink_index = network
        .node_index(&NodeId::parse(sink).expect("sink id"))
        .expect("sink node");
    let solved =
        solve_edmonds_karp(&network, source_index, sink_index).expect("adapted cycle solves");
    assert!(solved.certificate.value > 0);
}

fn assert_terminal_topologies_materialize_fixed_flow_min_cost() {
    for family in [
        FlowGeneratorFamilyV1::PlanarTriangulated { nodes: 9 },
        FlowGeneratorFamilyV1::HallTightBipartite {
            part_size: 8,
            tight_prefix: 3,
        },
    ] {
        let mut input = fixed_spec(family);
        input.target_problem = Some(FlowGeneratorTargetProblemV1::FixedFlowMinCost);
        let generated = generate_flow_graph(&input)
            .expect("terminal max-flow topology adapts to fixed-flow min-cost");
        assert!(matches!(
            generated.suggested_model,
            FlowProblemModelV1::FixedFlowMinCost { .. }
        ));
    }
}

fn assert_transshipment_topology_materializes_max_flow() {
    let native_cycle = generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::Cycle { nodes: 8 }))
        .expect("native circulation");
    let mut transshipment_graph = native_cycle.graph;
    transshipment_graph.nodes[0].supply = "3".to_owned();
    transshipment_graph.nodes[1].supply = "-3".to_owned();
    let adapted = adapt_generated_problem(
        &mut transshipment_graph,
        FlowProblemModelV1::Transshipment {},
        FlowGeneratorTargetProblemV1::MaxFlow,
    )
    .expect("transshipment topology adapts to max flow");
    assert!(matches!(adapted, FlowProblemModelV1::MaxFlow { .. }));
    assert!(
        transshipment_graph
            .nodes
            .iter()
            .all(|node| node.supply == "0")
    );
}

fn generated_model_matches_fixture_kind(
    expected: GeneratorModelKindV1,
    actual: &FlowProblemModelV1,
) -> bool {
    matches!(
        (expected, actual),
        (
            GeneratorModelKindV1::MaxFlow,
            FlowProblemModelV1::MaxFlow { .. }
        ) | (
            GeneratorModelKindV1::Circulation,
            FlowProblemModelV1::Circulation {}
        ) | (
            GeneratorModelKindV1::Transshipment,
            FlowProblemModelV1::Transshipment {}
        ) | (
            GeneratorModelKindV1::BipartiteMatching,
            FlowProblemModelV1::BipartiteMatching { .. }
        ) | (
            GeneratorModelKindV1::Assignment,
            FlowProblemModelV1::Assignment { .. }
        ) | (
            GeneratorModelKindV1::Transportation,
            FlowProblemModelV1::Transportation { .. }
        ) | (
            GeneratorModelKindV1::PlanarMaxFlow,
            FlowProblemModelV1::PlanarMaxFlow { .. }
        )
    )
}

fn assert_min_cost_workspace_materialization(fixture: &GeneratorAlgorithmFixtureV1) {
    let mut spec = fixture.presets[0].spec.clone();
    spec.target_problem = match fixture.model {
        GeneratorModelKindV1::MaxFlow
        | GeneratorModelKindV1::PlanarMaxFlow
        | GeneratorModelKindV1::BipartiteMatching => {
            Some(FlowGeneratorTargetProblemV1::FixedFlowMinCost)
        }
        GeneratorModelKindV1::Circulation
        | GeneratorModelKindV1::Transshipment
        | GeneratorModelKindV1::Assignment
        | GeneratorModelKindV1::Transportation => None,
    };
    let generated = generate_flow_graph(&spec).unwrap_or_else(|error| {
        panic!(
            "{} must materialize in the min-cost workspace: {error}",
            fixture.family_id
        )
    });
    let model_matches = if spec.target_problem.is_some() {
        matches!(
            &generated.suggested_model,
            FlowProblemModelV1::FixedFlowMinCost { .. }
        )
    } else {
        generated_model_matches_fixture_kind(fixture.model, &generated.suggested_model)
    };
    assert!(
        model_matches,
        "{} produced a non-min-cost model: {:?}",
        fixture.family_id, generated.suggested_model
    );
    let expected_target = spec
        .target_problem
        .map(|_| serde_json::json!("fixed-flow-min-cost"));
    assert_eq!(
        generated.provenance.parameters.get("target_problem"),
        expected_target.as_ref(),
        "{} min-cost target provenance drifted",
        fixture.family_id
    );
}

fn assert_max_flow_workspace_materialization(fixture: &GeneratorAlgorithmFixtureV1) -> bool {
    let mut spec = fixture.presets[0].spec.clone();
    spec.target_problem = match fixture.model {
        GeneratorModelKindV1::MaxFlow
        | GeneratorModelKindV1::PlanarMaxFlow
        | GeneratorModelKindV1::BipartiteMatching => None,
        GeneratorModelKindV1::Circulation
        | GeneratorModelKindV1::Transshipment
        | GeneratorModelKindV1::Assignment
        | GeneratorModelKindV1::Transportation => Some(FlowGeneratorTargetProblemV1::MaxFlow),
    };
    if matches!(
        fixture.model,
        GeneratorModelKindV1::Assignment | GeneratorModelKindV1::Transportation
    ) {
        assert!(matches!(
            generate_flow_graph(&spec),
            Err(FlowGenerationError::Invalid(
                "family cannot be materialized as max-flow"
            ))
        ));
        return false;
    }
    let generated = generate_flow_graph(&spec).unwrap_or_else(|error| {
        panic!(
            "{} must materialize in the max-flow workspace: {error}",
            fixture.family_id
        )
    });
    let model_matches = if spec.target_problem.is_some() {
        matches!(
            &generated.suggested_model,
            FlowProblemModelV1::MaxFlow { .. }
        )
    } else {
        generated_model_matches_fixture_kind(fixture.model, &generated.suggested_model)
    };
    assert!(
        model_matches,
        "{} produced a non-max-flow model: {:?}",
        fixture.family_id, generated.suggested_model
    );
    let expected_target = spec.target_problem.map(|_| serde_json::json!("max-flow"));
    assert_eq!(
        generated.provenance.parameters.get("target_problem"),
        expected_target.as_ref(),
        "{} max-flow target provenance drifted",
        fixture.family_id
    );
    true
}

#[test]
fn every_canonical_family_obeys_both_workspace_materialization_contracts() {
    let mut min_cost_family_ids = BTreeSet::new();
    let mut max_flow_family_ids = BTreeSet::new();
    let mut rejected_max_flow_family_ids = BTreeSet::new();

    for fixture in generator_algorithm_fixtures() {
        assert_min_cost_workspace_materialization(&fixture);
        min_cost_family_ids.insert(fixture.family_id.clone());

        if assert_max_flow_workspace_materialization(&fixture) {
            max_flow_family_ids.insert(fixture.family_id);
        } else {
            rejected_max_flow_family_ids.insert(fixture.family_id);
        }
    }

    assert_eq!(min_cost_family_ids.len(), 50);
    assert_eq!(max_flow_family_ids.len(), 48);
    assert_eq!(
        rejected_max_flow_family_ids,
        BTreeSet::from([
            "assignment-matrix".to_owned(),
            "transportation-table".to_owned(),
        ])
    );
}

fn assert_readable_max_flow_default() {
    let max_flow_spec = FlowGeneratorSpecV1 {
        generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
        seed: "42".to_owned(),
        family: FlowGeneratorFamilyV1::LayeredDag {
            layers: 5,
            width: 4,
            fanout: 2,
        },
        capacity: CapacityDistributionV1::Uniform {
            minimum: "3".to_owned(),
            maximum: "12".to_owned(),
        },
        cost: CostDistributionV1::Zero {},
        target_problem: None,
    };
    let generated = generate_flow_graph(&max_flow_spec).expect("default max-flow graph");
    assert_eq!(
        (generated.graph.nodes.len(), generated.graph.edges.len()),
        (22, 40)
    );
    let endpoints = generated
        .graph
        .edges
        .iter()
        .map(|edge| {
            (
                (edge.from.as_str(), edge.to.as_str()),
                edge.capacity.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for lane in 0..4 {
        let mut route = vec![("s".to_owned(), format!("l000n{lane:04}"))];
        for layer in 0..4 {
            route.push((
                format!("l{layer:03}n{lane:04}"),
                format!("l{:03}n{lane:04}", layer + 1),
            ));
        }
        route.push((format!("l004n{lane:04}"), "t".to_owned()));
        assert!(route.iter().all(|(from, to)| {
            endpoints
                .get(&(from.as_str(), to.as_str()))
                .is_some_and(|capacity| capacity.parse::<u64>().is_ok_and(|value| value >= 3))
        }));
    }
    let graph = canonical_network(&generated.graph);
    let source = graph
        .node_index(&NodeId::parse("s").expect("source id"))
        .expect("source node");
    let sink = graph
        .node_index(&NodeId::parse("t").expect("sink id"))
        .expect("sink node");
    let traced = trace_edmonds_karp(&graph, source, sink).expect("default max-flow trace");
    assert!(traced.result.certificate.value >= 12);
    assert!(traced.result.metrics.augmentations >= 5);
    assert!(traced.events.len() >= 50);
    assert!(
        traced
            .events
            .iter()
            .filter(|event| event.minimum_granularity == TraceGranularityV1::Micro)
            .count()
            >= 20
    );
}

fn assert_readable_min_cost_default() {
    let mut min_cost_spec = spec(FlowGeneratorFamilyV1::LayeredDag {
        layers: 5,
        width: 4,
        fanout: 2,
    });
    min_cost_spec.capacity = CapacityDistributionV1::Uniform {
        minimum: "3".to_owned(),
        maximum: "12".to_owned(),
    };
    min_cost_spec.target_problem = Some(FlowGeneratorTargetProblemV1::FixedFlowMinCost);
    let min_cost = generate_flow_graph(&min_cost_spec).expect("default min-cost graph");
    assert_eq!(
        (min_cost.graph.nodes.len(), min_cost.graph.edges.len()),
        (22, 40)
    );
    let graph = canonical_network(&min_cost.graph);
    let FlowProblemModelV1::FixedFlowMinCost {
        source,
        sink,
        required_flow,
    } = &min_cost.suggested_model
    else {
        panic!("default MCF generator must publish fixed-flow model");
    };
    let source = graph
        .node_index(&NodeId::parse(source).expect("source id"))
        .expect("source node");
    let sink = graph
        .node_index(&NodeId::parse(sink).expect("sink id"))
        .expect("sink node");
    let required_flow = required_flow.parse::<u64>().expect("required flow");
    let target =
        fixed_flow_divergences(&graph, source, sink, required_flow).expect("fixed-flow target");
    assert!(required_flow >= 15);
    let traced = trace_successive_shortest_path(&graph, &target)
        .expect("default min-cost trace and certificate");
    assert!(traced.result.metrics.augmentations >= 5);
    assert!(traced.events.len() >= 30);
    assert!(
        traced.events.len() <= 4_500,
        "the default graph must retain its complete complexity-faithful trace within the interactive publication budget"
    );
    assert!(
        traced
            .events
            .iter()
            .filter(|event| event.minimum_granularity == TraceGranularityV1::Micro)
            .count()
            >= 10
    );
    check_min_cost_flow(&graph, &target, &traced.result.flows)
        .expect("independent default min-cost certificate");
}

fn assignment_family(
    shape: AssignmentMatrixShapeV1,
    objective: AssignmentObjectiveV1,
) -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::AssignmentMatrix {
        agents: 4,
        tasks: 5,
        objective,
        shape,
    }
}

fn assignment_shape_cases() -> [(AssignmentMatrixShapeV1, usize, bool); 9] {
    [
        (
            AssignmentMatrixShapeV1::Uniform {
                density_per_mille: 500,
                minimum_cost: -4,
                maximum_cost: 7,
            },
            10,
            false,
        ),
        (AssignmentMatrixShapeV1::Equal { cost: 3 }, 20, true),
        (
            AssignmentMatrixShapeV1::Block {
                blocks: 2,
                within_cost: -2,
                between_cost: 9,
            },
            20,
            true,
        ),
        (
            AssignmentMatrixShapeV1::NearTie {
                base_cost: 10,
                gap: 1,
            },
            20,
            true,
        ),
        (
            AssignmentMatrixShapeV1::PlantedOptimum {
                density_per_mille: 400,
                base_cost: -5,
                gap: 3,
                noise: 2,
            },
            8,
            true,
        ),
        (AssignmentMatrixShapeV1::Monge { scale: 2 }, 20, true),
        (AssignmentMatrixShapeV1::AntiMonge { scale: 2 }, 20, true),
        (
            AssignmentMatrixShapeV1::SparseAllowed {
                degree: 2,
                minimum_cost: -3,
                maximum_cost: 5,
            },
            8,
            false,
        ),
        (
            AssignmentMatrixShapeV1::HallDeficient {
                witness_agents: 3,
                witness_tasks: 2,
                minimum_cost: 0,
                maximum_cost: 4,
            },
            11,
            false,
        ),
    ]
}

fn transportation_family(
    origins: u32,
    destinations: u32,
    total_supply: u32,
    shape: TransportationTableShapeV1,
) -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::TransportationTable {
        origins,
        destinations,
        total_supply,
        shape,
    }
}

fn transportation_feasible_shapes() -> [TransportationTableShapeV1; 6] {
    [
        TransportationTableShapeV1::DenseUniform {
            minimum_cost: -4,
            maximum_cost: 7,
        },
        TransportationTableShapeV1::SparseFeasible {
            density_per_mille: 350,
            minimum_cost: -2,
            maximum_cost: 8,
        },
        TransportationTableShapeV1::UnitDegenerate { cost: 3 },
        TransportationTableShapeV1::Block {
            blocks: 2,
            within_cost: 1,
            between_cost: 9,
        },
        TransportationTableShapeV1::NearTie {
            base_cost: 5,
            gap: 1,
        },
        TransportationTableShapeV1::Monge { scale: 2 },
    ]
}

#[test]
fn transportation_table_shapes_are_native_balanced_and_certified() {
    for shape in transportation_feasible_shapes() {
        let shape_id = transportation_shape_id(&shape);
        let (origins, destinations, total_supply) = if shape_id == "unit-degenerate" {
            (4, 4, 4)
        } else {
            (3, 4, 12)
        };
        let expected_edges = if shape_id == "sparse-feasible" {
            6
        } else {
            usize::try_from(origins * destinations).expect("small dimensions")
        };
        let generated = generate_flow_graph(&fixed_spec(transportation_family(
            origins,
            destinations,
            total_supply,
            shape,
        )))
        .expect("transportation table materializes");
        assert_transportation_graph_contract(&generated, expected_edges, total_supply, shape_id);
        let result = solve_generated_transportation(&generated)
            .expect("feasible generated table is certified");
        if shape_id == "unit-degenerate" {
            assert!(result.metrics.basis_extensions > 0);
        }
    }

    let cut = generate_flow_graph(&fixed_spec(transportation_family(
        3,
        4,
        12,
        TransportationTableShapeV1::CutInfeasible {
            minimum_cost: 0,
            maximum_cost: 4,
        },
    )))
    .expect("cut-infeasible transportation table materializes");
    assert_transportation_graph_contract(&cut, 9, 12, "cut-infeasible");
    let Err(TransportationError::Feasibility(FeasibilityError::Infeasible(witness))) =
        solve_generated_transportation(&cut)
    else {
        panic!("cut shape must return an exact infeasibility witness");
    };
    let FlowProblemModelV1::Transportation {
        origins,
        destinations,
    } = &cut.suggested_model
    else {
        panic!("native transportation model");
    };
    check_transportation_infeasibility(
        &canonical_network(&cut.graph),
        origins,
        destinations,
        &witness,
    )
    .expect("generated cut witness independently verifies");
    assert_eq!(witness.unsatisfied, 9);
    assert!(
        witness
            .reachable_original_nodes
            .contains(&NodeId::parse("o0000").expect("origin node"))
    );
}

fn assert_transportation_graph_contract(
    generated: &GeneratedFlowGraphV1,
    expected_edges: usize,
    total_supply: u32,
    shape_id: &str,
) {
    assert_eq!(generated.graph.edges.len(), expected_edges, "{shape_id}");
    assert_eq!(generated.provenance.generator_revision, "flow-generator/27");
    assert_eq!(generated.provenance.family_id, "transportation-table");
    assert_eq!(
        generated.provenance.source_id,
        "flow-transportation-table-contract-v1"
    );
    assert_eq!(generated.provenance.origin, "project-synthetic");
    assert_eq!(
        generated.provenance.sampling,
        if shape_id == "unit-degenerate" {
            "deterministic"
        } else {
            "randomized"
        }
    );
    assert_eq!(
        generated.provenance.difficulty,
        if shape_id == "unit-degenerate" || shape_id == "near-tie" {
            "stress"
        } else {
            "ordinary"
        }
    );
    assert!(generated.provenance.difficulty_certificate.is_none());
    assert!(generated.provenance.tags.contains(&shape_id.to_owned()));
    assert!(
        generated
            .graph
            .edges
            .iter()
            .all(|edge| { edge.lower == "0" && edge.capacity == total_supply.to_string() })
    );
}

fn solve_generated_transportation(
    generated: &GeneratedFlowGraphV1,
) -> Result<crate::TransportationResult, TransportationError> {
    let FlowProblemModelV1::Transportation {
        origins,
        destinations,
    } = &generated.suggested_model
    else {
        panic!("native transportation model");
    };
    solve_transportation_simplex(&canonical_network(&generated.graph), origins, destinations)
}

#[test]
fn transportation_formula_shapes_have_exact_row_major_cost_oracles() {
    let cases = [
        (
            TransportationTableShapeV1::Block {
                blocks: 2,
                within_cost: -3,
                between_cost: 11,
            },
            "block",
        ),
        (
            TransportationTableShapeV1::NearTie {
                base_cost: 5,
                gap: 2,
            },
            "near-tie",
        ),
        (TransportationTableShapeV1::Monge { scale: 3 }, "monge"),
    ];
    for (shape, shape_id) in cases {
        let generated = generate_flow_graph(&fixed_spec(transportation_family(3, 4, 12, shape)))
            .expect("formula table");
        assert_eq!(generated.graph.edges.first().expect("first").id, "e000000");
        assert_eq!(generated.graph.edges.last().expect("last").id, "e000011");
        for edge in &generated.graph.edges {
            let origin = edge
                .from
                .strip_prefix('o')
                .expect("origin prefix")
                .parse::<u32>()
                .expect("origin ordinal");
            let destination = edge
                .to
                .strip_prefix('d')
                .expect("destination prefix")
                .parse::<u32>()
                .expect("destination ordinal");
            let expected = match shape_id {
                "block" if origin % 2 == destination % 2 => -3,
                "block" => 11,
                "near-tie" if origin % 4 == destination => 5,
                "near-tie" => 7,
                "monge" => i64::from(origin.abs_diff(destination)) * 3,
                _ => unreachable!("closed formula case"),
            };
            assert_eq!(edge.cost.parse::<i64>().expect("cost"), expected);
        }
    }
}

#[test]
fn transportation_random_streams_are_reproducible_and_cost_independent() {
    let family = transportation_family(
        5,
        6,
        30,
        TransportationTableShapeV1::SparseFeasible {
            density_per_mille: 400,
            minimum_cost: -50,
            maximum_cost: 50,
        },
    );
    let first = generate_flow_graph(&fixed_spec(family.clone())).expect("first");
    let second = generate_flow_graph(&fixed_spec(family.clone())).expect("second");
    assert_eq!(
        first.provenance.materialized_sha256,
        second.provenance.materialized_sha256
    );
    assert_eq!(
        serde_json::to_value(&first.graph).expect("first graph"),
        serde_json::to_value(&second.graph).expect("second graph")
    );

    let mut different_seed = fixed_spec(family.clone());
    different_seed.seed = "43".to_owned();
    let different_seed = generate_flow_graph(&different_seed).expect("different seed");
    assert_ne!(
        first.provenance.materialized_sha256,
        different_seed.provenance.materialized_sha256
    );

    let FlowGeneratorFamilyV1::TransportationTable {
        origins,
        destinations,
        total_supply,
        ..
    } = family
    else {
        unreachable!("transportation family")
    };
    let changed_cost = generate_flow_graph(&fixed_spec(transportation_family(
        origins,
        destinations,
        total_supply,
        TransportationTableShapeV1::SparseFeasible {
            density_per_mille: 400,
            minimum_cost: 1_000,
            maximum_cost: 2_000,
        },
    )))
    .expect("changed cost interval");
    assert_eq!(
        first
            .graph
            .nodes
            .iter()
            .map(|node| (&node.id, &node.supply))
            .collect::<Vec<_>>(),
        changed_cost
            .graph
            .nodes
            .iter()
            .map(|node| (&node.id, &node.supply))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        first
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>(),
        changed_cost
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>()
    );
    assert_ne!(
        first
            .graph
            .edges
            .iter()
            .map(|edge| &edge.cost)
            .collect::<Vec<_>>(),
        changed_cost
            .graph
            .edges
            .iter()
            .map(|edge| &edge.cost)
            .collect::<Vec<_>>()
    );
}

#[test]
fn transportation_generator_rejects_unsafe_or_ambiguous_shapes_before_allocation() {
    for (origins, destinations, total_supply) in [(32, 64, 64), (8, 248, 248)] {
        let generated = generate_flow_graph(&fixed_spec(transportation_family(
            origins,
            destinations,
            total_supply,
            TransportationTableShapeV1::DenseUniform {
                minimum_cost: 0,
                maximum_cost: 0,
            },
        )))
        .expect("exact transportation admission boundary");
        assert_eq!(
            generated.graph.nodes.len(),
            usize::try_from(origins + destinations).expect("node count")
        );
        assert_eq!(
            generated.graph.edges.len(),
            usize::try_from(origins * destinations).expect("edge count")
        );
    }
    for family in [
        transportation_family(
            64,
            64,
            128,
            TransportationTableShapeV1::DenseUniform {
                minimum_cost: 0,
                maximum_cost: 1,
            },
        ),
        transportation_family(3, 4, 3, TransportationTableShapeV1::Monge { scale: 1 }),
        transportation_family(
            3,
            4,
            12,
            TransportationTableShapeV1::SparseFeasible {
                density_per_mille: 0,
                minimum_cost: 0,
                maximum_cost: 1,
            },
        ),
        transportation_family(
            3,
            4,
            12,
            TransportationTableShapeV1::UnitDegenerate { cost: 0 },
        ),
        transportation_family(
            1,
            4,
            12,
            TransportationTableShapeV1::CutInfeasible {
                minimum_cost: 0,
                maximum_cost: 1,
            },
        ),
        transportation_family(
            8,
            249,
            249,
            TransportationTableShapeV1::DenseUniform {
                minimum_cost: 0,
                maximum_cost: 1,
            },
        ),
        transportation_family(
            33,
            63,
            63,
            TransportationTableShapeV1::DenseUniform {
                minimum_cost: 0,
                maximum_cost: 1,
            },
        ),
    ] {
        assert!(generate_flow_graph(&fixed_spec(family)).is_err());
    }
}

#[test]
fn assignment_matrix_shapes_materialize_exact_native_contracts() {
    for (shape, expected_edges, must_be_feasible) in assignment_shape_cases() {
        let shape_id = assignment_shape_id(&shape).to_owned();
        let generated = generate_flow_graph(&fixed_spec(assignment_family(
            shape,
            AssignmentObjectiveV1::Minimize,
        )))
        .expect("assignment matrix generates");
        assert_eq!(generated.graph.nodes.len(), 9, "{shape_id}");
        assert_eq!(generated.graph.edges.len(), expected_edges, "{shape_id}");
        assert!(generated.graph.nodes.iter().all(|node| node.supply == "0"));
        assert!(
            generated
                .graph
                .edges
                .iter()
                .all(|edge| edge.lower == "0" && edge.capacity == "1")
        );
        assert!(generated.provenance.tags.contains(&shape_id));
        let FlowProblemModelV1::Assignment {
            agents,
            tasks,
            objective,
        } = &generated.suggested_model
        else {
            panic!("native assignment model");
        };
        let result = solve_hungarian(
            &canonical_network(&generated.graph),
            agents,
            tasks,
            *objective,
        )
        .expect("generated assignment is admitted");
        if must_be_feasible {
            assert!(
                matches!(result.outcome, HungarianOutcome::Optimal { .. }),
                "{shape_id}"
            );
        }
        if shape_id == "hall-deficient" {
            let HungarianOutcome::Infeasible { witness, .. } = result.outcome else {
                panic!("Hall-deficient generator must be infeasible");
            };
            assert_eq!(witness.agents.len(), 3);
            assert_eq!(witness.neighbor_tasks.len(), 2);
            assert_eq!(witness.deficiency, 1);
        }
    }
}

#[test]
fn assignment_planted_optimum_is_seeded_unique_and_objective_oriented() {
    for objective in [
        AssignmentObjectiveV1::Minimize,
        AssignmentObjectiveV1::Maximize,
    ] {
        let family = assignment_family(
            AssignmentMatrixShapeV1::PlantedOptimum {
                density_per_mille: 600,
                base_cost: 17,
                gap: 5,
                noise: 3,
            },
            objective,
        );
        let first = generate_flow_graph(&fixed_spec(family.clone())).expect("first");
        let second = generate_flow_graph(&fixed_spec(family.clone())).expect("second");
        assert_eq!(
            first.provenance.materialized_sha256,
            second.provenance.materialized_sha256
        );
        let mut changed = fixed_spec(family);
        changed.seed = "43".to_owned();
        let changed = generate_flow_graph(&changed).expect("changed seed");
        assert_ne!(
            first.provenance.materialized_sha256,
            changed.provenance.materialized_sha256
        );
        let FlowProblemModelV1::Assignment {
            agents,
            tasks,
            objective,
        } = &first.suggested_model
        else {
            panic!("assignment model");
        };
        let result = solve_hungarian(&canonical_network(&first.graph), agents, tasks, *objective)
            .expect("solve");
        let HungarianOutcome::Optimal { certificate, .. } = result.outcome else {
            panic!("planted shape is feasible");
        };
        assert_eq!(certificate.total_cost, 68);
    }
}

#[test]
fn assignment_generator_rejects_invalid_shapes_and_solver_scale() {
    for family in [
        assignment_family(
            AssignmentMatrixShapeV1::Uniform {
                density_per_mille: 1_001,
                minimum_cost: 0,
                maximum_cost: 1,
            },
            AssignmentObjectiveV1::Minimize,
        ),
        assignment_family(
            AssignmentMatrixShapeV1::SparseAllowed {
                degree: 6,
                minimum_cost: 0,
                maximum_cost: 1,
            },
            AssignmentObjectiveV1::Minimize,
        ),
        assignment_family(
            AssignmentMatrixShapeV1::HallDeficient {
                witness_agents: 3,
                witness_tasks: 3,
                minimum_cost: 0,
                maximum_cost: 1,
            },
            AssignmentObjectiveV1::Minimize,
        ),
        FlowGeneratorFamilyV1::AssignmentMatrix {
            agents: 300,
            tasks: 300,
            objective: AssignmentObjectiveV1::Minimize,
            shape: AssignmentMatrixShapeV1::SparseAllowed {
                degree: 1,
                minimum_cost: 0,
                maximum_cost: 1,
            },
        },
    ] {
        assert!(generate_flow_graph(&fixed_spec(family)).is_err());
    }
}

fn canonical_network(graph: &FlowGraphV1) -> FlowNetwork {
    FlowNetwork::new(
        graph
            .nodes
            .iter()
            .map(|node| {
                FlowNode::new(
                    NodeId::parse(&node.id).expect("generated node id"),
                    node.supply.parse().expect("generated supply"),
                )
            })
            .collect(),
        graph
            .edges
            .iter()
            .map(|edge| UnresolvedFlowEdge {
                id: EdgeId::parse(&edge.id).expect("generated edge id"),
                from: NodeId::parse(&edge.from).expect("generated tail"),
                to: NodeId::parse(&edge.to).expect("generated head"),
                lower: edge.lower.parse().expect("generated lower"),
                capacity: edge.capacity.parse().expect("generated capacity"),
                cost: edge.cost.parse().expect("generated cost"),
            })
            .collect(),
    )
    .expect("generated network validates")
}

fn reference_dinic(graph: &FlowGraphV1) -> (u64, usize) {
    let indices = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let mut residual = vec![Vec::<ReferenceArc>::new(); graph.nodes.len()];
    for edge in &graph.edges {
        let from = indices[edge.from.as_str()];
        let to = indices[edge.to.as_str()];
        let capacity = edge.capacity.parse::<u64>().expect("canonical capacity");
        let forward_reverse = residual[to].len();
        let reverse_reverse = residual[from].len();
        residual[from].push(ReferenceArc {
            to,
            reverse: forward_reverse,
            capacity,
        });
        residual[to].push(ReferenceArc {
            to: from,
            reverse: reverse_reverse,
            capacity: 0,
        });
    }
    let source = indices["s"];
    let sink = indices["t"];
    let mut phases = 0;
    let mut total = 0;
    loop {
        let mut level = vec![usize::MAX; residual.len()];
        let mut queue = std::collections::VecDeque::from([source]);
        level[source] = 0;
        while let Some(node) = queue.pop_front() {
            for arc in &residual[node] {
                if arc.capacity > 0 && level[arc.to] == usize::MAX {
                    level[arc.to] = level[node] + 1;
                    queue.push_back(arc.to);
                }
            }
        }
        if level[sink] == usize::MAX {
            break;
        }
        phases += 1;
        let mut cursor = vec![0; residual.len()];
        loop {
            let pushed =
                reference_blocking_dfs(source, sink, u64::MAX, &level, &mut cursor, &mut residual);
            if pushed == 0 {
                break;
            }
            total += pushed;
        }
    }
    (total, phases)
}

fn reference_blocking_dfs(
    node: usize,
    sink: usize,
    limit: u64,
    level: &[usize],
    cursor: &mut [usize],
    residual: &mut [Vec<ReferenceArc>],
) -> u64 {
    if node == sink {
        return limit;
    }
    while cursor[node] < residual[node].len() {
        let index = cursor[node];
        let arc = residual[node][index];
        if arc.capacity > 0 && level[arc.to] == level[node] + 1 {
            let pushed = reference_blocking_dfs(
                arc.to,
                sink,
                limit.min(arc.capacity),
                level,
                cursor,
                residual,
            );
            if pushed > 0 {
                residual[node][index].capacity -= pushed;
                residual[arc.to][arc.reverse].capacity += pushed;
                return pushed;
            }
        }
        cursor[node] += 1;
    }
    0
}

#[test]
#[allow(clippy::too_many_lines)]
fn basic_families_have_expected_exact_sizes() {
    let cases = [
        (FlowGeneratorFamilyV1::Path { nodes: 5 }, (5, 4)),
        (FlowGeneratorFamilyV1::Cycle { nodes: 5 }, (5, 5)),
        (
            FlowGeneratorFamilyV1::ParallelPaths {
                path_count: 3,
                internal_nodes: 2,
            },
            (8, 9),
        ),
        (FlowGeneratorFamilyV1::DiamondChain { stages: 3 }, (10, 12)),
        (
            FlowGeneratorFamilyV1::LayeredDag {
                layers: 3,
                width: 4,
                fanout: 2,
            },
            (14, 24),
        ),
        (
            FlowGeneratorFamilyV1::Grid2d {
                rows: 3,
                columns: 4,
                diagonals: false,
            },
            (12, 17),
        ),
        (
            FlowGeneratorFamilyV1::VisionSegmentationGrid {
                rows: 3,
                columns: 4,
                eight_neighbor: false,
            },
            (14, 58),
        ),
        (
            FlowGeneratorFamilyV1::Arborescence {
                branching: 2,
                depth: 3,
            },
            (15, 14),
        ),
        (
            FlowGeneratorFamilyV1::StronglyConnected {
                nodes: 5,
                extra_edges: 4,
            },
            (5, 9),
        ),
        (
            FlowGeneratorFamilyV1::Grid3d {
                layers: 2,
                rows: 3,
                columns: 4,
            },
            (24, 46),
        ),
        (
            FlowGeneratorFamilyV1::BipartiteRandom {
                left: 3,
                right: 4,
                edge_count: 5,
            },
            (9, 12),
        ),
        (
            FlowGeneratorFamilyV1::RandomGeometric {
                nodes: 10,
                radius: 1_000,
            },
            (10, 45),
        ),
        (
            FlowGeneratorFamilyV1::RandomRegularDirected {
                nodes: 12,
                degree: 3,
            },
            (12, 36),
        ),
        (
            FlowGeneratorFamilyV1::PreferentialAttachmentDirected {
                nodes: 10,
                attachment_count: 2,
            },
            (10, 17),
        ),
        (
            FlowGeneratorFamilyV1::PlanarTriangulated { nodes: 9 },
            (9, 15),
        ),
        (
            FlowGeneratorFamilyV1::MultiSourceSink {
                sources: 3,
                intermediate: 4,
                sinks: 2,
            },
            (11, 25),
        ),
    ];
    for (family, (nodes, edges)) in cases {
        let generated = generate_flow_graph(&spec(family)).expect("generation succeeds");
        assert_eq!(generated.graph.nodes.len(), nodes);
        assert_eq!(generated.graph.edges.len(), edges);
    }
}

#[test]
fn vision_grid_has_terminal_arcs_bidirectional_neighbors_and_sourced_provenance() {
    let baseline_spec = spec(FlowGeneratorFamilyV1::VisionSegmentationGrid {
        rows: 3,
        columns: 4,
        eight_neighbor: true,
    });
    let generated = generate_flow_graph(&baseline_spec).expect("bounded vision graph");
    let repeated = generate_flow_graph(&baseline_spec).expect("same seed repeats");
    assert_eq!(
        serde_json::to_vec(&generated).expect("generated output serializes"),
        serde_json::to_vec(&repeated).expect("repeated output serializes")
    );
    let mut changed_seed = baseline_spec;
    changed_seed.seed = "43".to_owned();
    let changed = generate_flow_graph(&changed_seed).expect("changed seed materializes");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>(),
        changed
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>()
    );
    assert_ne!(
        generated.provenance.materialized_sha256,
        changed.provenance.materialized_sha256
    );
    assert_eq!(generated.graph.nodes.len(), 14);
    assert_eq!(generated.graph.edges.len(), 82);
    assert_eq!(
        generated.provenance.source_id,
        "boykov-kolmogorov-2004-vision-grid-derived"
    );
    assert_eq!(generated.provenance.origin, "paper-derived");
    assert_eq!(generated.provenance.difficulty, "ordinary");
    for tag in ["bidirectional", "terminal-heavy", "vision-graph-cut"] {
        assert!(generated.provenance.tags.iter().any(|actual| actual == tag));
    }
    for row in 0..3 {
        for column in 0..4 {
            let pixel = vision_grid_id(row, column);
            assert!(
                generated
                    .graph
                    .edges
                    .iter()
                    .any(|edge| edge.from == "s" && edge.to == pixel)
            );
            assert!(
                generated
                    .graph
                    .edges
                    .iter()
                    .any(|edge| edge.from == pixel && edge.to == "t")
            );
        }
    }
    for (left, right) in [
        (vision_grid_id(0, 0), vision_grid_id(0, 1)),
        (vision_grid_id(0, 0), vision_grid_id(1, 0)),
        (vision_grid_id(0, 0), vision_grid_id(1, 1)),
    ] {
        assert!(
            generated
                .graph
                .edges
                .iter()
                .any(|edge| { edge.from == left && edge.to == right })
        );
        assert!(
            generated
                .graph
                .edges
                .iter()
                .any(|edge| { edge.from == right && edge.to == left })
        );
    }
    assert!(
        generate_flow_graph(&spec(FlowGeneratorFamilyV1::VisionSegmentationGrid {
            rows: 15,
            columns: 16,
            eight_neighbor: true,
        },))
        .is_err()
    );
}

#[test]
fn random_shape_families_have_expected_exact_sizes() {
    let cases = [
        (
            FlowGeneratorFamilyV1::RandomDag {
                nodes: 10,
                edge_count: 12,
            },
            (10, 12),
        ),
        (
            FlowGeneratorFamilyV1::WattsStrogatzFixed {
                nodes: 20,
                neighborhood: 4,
                rewire_count: 8,
            },
            (20, 40),
        ),
        (
            FlowGeneratorFamilyV1::ClusteredDirected {
                clusters: 3,
                cluster_size: 4,
                bridge_edges: 5,
            },
            (12, 17),
        ),
    ];
    for (family, (nodes, edges)) in cases {
        let generated = generate_flow_graph(&spec(family)).expect("generation succeeds");
        assert_eq!(generated.graph.nodes.len(), nodes);
        assert_eq!(generated.graph.edges.len(), edges);
    }
}

#[test]
fn rmfgen_frames_preserve_grid_permutations_and_source_capacity_rules() {
    let family = FlowGeneratorFamilyV1::RmfgenFrames {
        frame_size: 2,
        depth: 3,
        minimum_capacity: 3,
        maximum_capacity: 9,
    };
    let generated = generate_flow_graph(&fixed_spec(family.clone())).expect("RMFGEN-derived graph");
    assert_eq!(generated.graph.nodes.len(), 12);
    assert_eq!(generated.graph.edges.len(), 32);
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(
        generated.provenance.materialized_sha256,
        "3e17e003d396b027cb3a84ed864b191399047c7c0e6dcb614e87964d4994fff0"
    );
    assert_eq!(
        generated.provenance.source_id,
        "goldfarb-grigoriadis-rmfgen-1988-project-rng-derived"
    );
    assert!(matches!(
        generated.suggested_model,
        FlowProblemModelV1::MaxFlow { ref source, ref sink }
            if source == "f000r000c000" && sink == "f002r001c001"
    ));
    assert!(generated.graph.edges.iter().all(|edge| edge.cost == "0"));

    let mut endpoints = BTreeSet::new();
    let mut inter_targets = BTreeMap::<u32, BTreeSet<&str>>::new();
    let mut inter_sources = BTreeMap::<u32, BTreeSet<&str>>::new();
    let mut in_frame_count = 0;
    let mut inter_frame_count = 0;
    for edge in &generated.graph.edges {
        assert!(endpoints.insert((edge.from.as_str(), edge.to.as_str())));
        let from_frame = edge.from[1..4].parse::<u32>().expect("frame ordinal");
        let to_frame = edge.to[1..4].parse::<u32>().expect("frame ordinal");
        let capacity = edge.capacity.parse::<u64>().expect("capacity");
        if from_frame == to_frame {
            in_frame_count += 1;
            assert_eq!(capacity, 36);
            let from_row = edge.from[5..8].parse::<i32>().expect("row");
            let from_column = edge.from[9..12].parse::<i32>().expect("column");
            let to_row = edge.to[5..8].parse::<i32>().expect("row");
            let to_column = edge.to[9..12].parse::<i32>().expect("column");
            assert_eq!(
                (from_row - to_row).abs() + (from_column - to_column).abs(),
                1
            );
        } else {
            inter_frame_count += 1;
            assert_eq!(to_frame, from_frame + 1);
            assert!((3..=9).contains(&capacity));
            assert!(
                inter_sources
                    .entry(from_frame)
                    .or_default()
                    .insert(edge.from.as_str())
            );
            assert!(
                inter_targets
                    .entry(from_frame)
                    .or_default()
                    .insert(edge.to.as_str())
            );
        }
    }
    assert_eq!(in_frame_count, 24);
    assert_eq!(inter_frame_count, 8);
    for frame in 0..2 {
        assert_eq!(inter_sources[&frame].len(), 4);
        assert_eq!(inter_targets[&frame].len(), 4);
    }
}

#[test]
fn rmfgen_frames_are_reproducible_and_keep_rng_streams_independent() {
    let family = FlowGeneratorFamilyV1::RmfgenFrames {
        frame_size: 2,
        depth: 3,
        minimum_capacity: 3,
        maximum_capacity: 9,
    };
    let generated = generate_flow_graph(&fixed_spec(family.clone())).expect("generated");
    let repeated = generate_flow_graph(&fixed_spec(family.clone())).expect("same seed");
    assert_eq!(
        serde_json::to_vec(&generated).expect("serialize"),
        serde_json::to_vec(&repeated).expect("serialize")
    );
    let mut different_seed = fixed_spec(family);
    different_seed.seed = "43".to_owned();
    let changed = generate_flow_graph(&different_seed).expect("different seed");
    assert_ne!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.capacity))
            .collect::<Vec<_>>(),
        changed
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.capacity))
            .collect::<Vec<_>>()
    );

    let changed_interval = fixed_spec(FlowGeneratorFamilyV1::RmfgenFrames {
        frame_size: 2,
        depth: 3,
        minimum_capacity: 7,
        maximum_capacity: 11,
    });
    let changed_interval =
        generate_flow_graph(&changed_interval).expect("changed capacity stream parameters");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>(),
        changed_interval
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>()
    );
    assert_ne!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| &edge.capacity)
            .collect::<Vec<_>>(),
        changed_interval
            .graph
            .edges
            .iter()
            .map(|edge| &edge.capacity)
            .collect::<Vec<_>>()
    );
}

#[test]
fn rmfgen_frames_reject_attribute_drift_and_unsafe_source_parameters() {
    assert!(matches!(
        generate_flow_graph(&spec(FlowGeneratorFamilyV1::RmfgenFrames {
            frame_size: 2,
            depth: 2,
            minimum_capacity: 1,
            maximum_capacity: 9,
        })),
        Err(FlowGenerationError::Invalid(
            "family attributes are fixed by the declared construction"
        ))
    ));
    for family in [
        FlowGeneratorFamilyV1::RmfgenFrames {
            frame_size: 1,
            depth: 2,
            minimum_capacity: 1,
            maximum_capacity: 9,
        },
        FlowGeneratorFamilyV1::RmfgenFrames {
            frame_size: 2,
            depth: 0,
            minimum_capacity: 1,
            maximum_capacity: 9,
        },
        FlowGeneratorFamilyV1::RmfgenFrames {
            frame_size: 1_001,
            depth: 1,
            minimum_capacity: 1,
            maximum_capacity: 9,
        },
        FlowGeneratorFamilyV1::RmfgenFrames {
            frame_size: 2,
            depth: 1_001,
            minimum_capacity: 1,
            maximum_capacity: 9,
        },
        FlowGeneratorFamilyV1::RmfgenFrames {
            frame_size: 2,
            depth: 2,
            minimum_capacity: 10,
            maximum_capacity: 9,
        },
        FlowGeneratorFamilyV1::RmfgenFrames {
            frame_size: 2,
            depth: 2,
            minimum_capacity: 1,
            maximum_capacity: 1_001,
        },
    ] {
        assert!(generate_flow_graph(&fixed_spec(family)).is_err());
    }
    assert!(matches!(
        generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::RmfgenFrames {
            frame_size: 100,
            depth: 100,
            minimum_capacity: 1,
            maximum_capacity: 9,
        })),
        Err(FlowGenerationError::SizeLimit)
    ));

    let minimum = generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::RmfgenFrames {
        frame_size: 2,
        depth: 1,
        minimum_capacity: 0,
        maximum_capacity: 0,
    }))
    .expect("minimum non-degenerate project instance");
    assert_eq!(
        (minimum.graph.nodes.len(), minimum.graph.edges.len()),
        (4, 8)
    );
    assert!(minimum.graph.edges.iter().all(|edge| edge.capacity == "0"));

    let exact_node_limit = generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::RmfgenFrames {
        frame_size: 100,
        depth: 1,
        minimum_capacity: 1,
        maximum_capacity: 9,
    }))
    .expect("10,000-node boundary is admitted; edge cap is not tighter here");
    assert_eq!(exact_node_limit.graph.nodes.len(), MAX_FLOW_NODES);
    assert_eq!(exact_node_limit.graph.edges.len(), 39_600);
}

#[test]
fn gridgen_grid_materializes_grid_supernode_balances_and_uniform_attributes() {
    let family = FlowGeneratorFamilyV1::GridgenGrid {
        rows: 3,
        columns: 4,
        terminal_pairs: 2,
        average_degree: 3,
        total_supply: 20,
        two_way: true,
        minimum_capacity: 3,
        maximum_capacity: 9,
        minimum_cost: 2,
        maximum_cost: 7,
    };
    let generated =
        generate_flow_graph(&fixed_spec(family.clone())).expect("GRIDGEN-derived graph");
    assert_eq!(
        (generated.graph.nodes.len(), generated.graph.edges.len()),
        (13, 39)
    );
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(
        generated.provenance.source_id,
        "lee-orlin-gridgen-1991-project-rng-uniform-derived"
    );
    assert!(matches!(
        generated.suggested_model,
        FlowProblemModelV1::Transshipment {}
    ));

    let supplies = generated
        .graph
        .nodes
        .iter()
        .map(|node| node.supply.parse::<i64>().expect("canonical supply"))
        .collect::<Vec<_>>();
    assert_eq!(supplies.iter().filter(|&&value| value > 0).count(), 2);
    assert_eq!(supplies.iter().filter(|&&value| value < 0).count(), 2);
    assert_eq!(supplies.iter().filter(|&&value| value > 0).sum::<i64>(), 20);
    assert_eq!(
        supplies.iter().filter(|&&value| value < 0).sum::<i64>(),
        -20
    );
    assert_eq!(supplies.iter().sum::<i64>(), 0);

    let lattice_edge_count = 34;
    let lattice = &generated.graph.edges[..lattice_edge_count];
    let super_edges = &generated.graph.edges[lattice_edge_count..lattice_edge_count + 4];
    let additional = &generated.graph.edges[lattice_edge_count + 4..];
    assert_eq!(additional.len(), 1);
    assert!(lattice.iter().all(|edge| {
        (3..=9).contains(&edge.capacity.parse::<u32>().expect("capacity"))
            && (2..=7).contains(&edge.cost.parse::<u32>().expect("cost"))
    }));
    let high_cost = lattice
        .iter()
        .map(|edge| edge.cost.parse::<i64>().expect("cost"))
        .max()
        .expect("lattice edges")
        * 2;
    assert!(super_edges.iter().all(|edge| {
        edge.capacity == "20"
            && edge.cost == high_cost.to_string()
            && (edge.to == "super" || edge.from == "super")
    }));
    assert!(additional.iter().all(|edge| {
        edge.from != edge.to
            && edge.from != "super"
            && edge.to != "super"
            && (3..=9).contains(&edge.capacity.parse::<u32>().expect("capacity"))
            && (2..=7).contains(&edge.cost.parse::<u32>().expect("cost"))
    }));

    let endpoints = lattice
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect::<BTreeSet<_>>();
    for row in 0..3 {
        for column in 0..3 {
            let left = gridgen_id(row, column);
            let right = gridgen_id(row, column + 1);
            assert!(endpoints.contains(&(left.as_str(), right.as_str())));
            assert!(endpoints.contains(&(right.as_str(), left.as_str())));
        }
    }
    for column in 0..4 {
        for row in 0..2 {
            let upper = gridgen_id(row, column);
            let lower = gridgen_id(row + 1, column);
            assert!(endpoints.contains(&(upper.as_str(), lower.as_str())));
            assert!(endpoints.contains(&(lower.as_str(), upper.as_str())));
        }
    }

    let network = canonical_network(&generated.graph);
    let required = supplies.into_iter().map(i128::from).collect::<Vec<_>>();
    solve_cost_scaling(&network, &required).expect("supernode arcs guarantee feasibility");

    let repeated = generate_flow_graph(&fixed_spec(family)).expect("same seed");
    assert_eq!(
        serde_json::to_vec(&generated).expect("serialize"),
        serde_json::to_vec(&repeated).expect("serialize")
    );
}

#[test]
fn gridgen_grid_has_a_golden_digest_and_independent_attribute_streams() {
    let family = |minimum_capacity, maximum_capacity, minimum_cost, maximum_cost| {
        FlowGeneratorFamilyV1::GridgenGrid {
            rows: 3,
            columns: 4,
            terminal_pairs: 2,
            average_degree: 3,
            total_supply: 20,
            two_way: true,
            minimum_capacity,
            maximum_capacity,
            minimum_cost,
            maximum_cost,
        }
    };
    let generated =
        generate_flow_graph(&fixed_spec(family(3, 9, 2, 7))).expect("base GRIDGEN graph");
    assert_eq!(
        generated.provenance.materialized_sha256,
        "69310a771743efa26012061b9e9d0b0830ec4e9be6948fbe160e5ddf1eb5a318"
    );

    let changed_capacity = generate_flow_graph(&fixed_spec(family(101, 101, 2, 7)))
        .expect("changed capacity interval");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.cost))
            .collect::<Vec<_>>(),
        changed_capacity
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.cost))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        serde_json::to_vec(&generated.graph.nodes).expect("serialize nodes"),
        serde_json::to_vec(&changed_capacity.graph.nodes).expect("serialize nodes")
    );

    let changed_cost =
        generate_flow_graph(&fixed_spec(family(3, 9, 99, 99))).expect("changed cost interval");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.capacity))
            .collect::<Vec<_>>(),
        changed_cost
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.capacity))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        serde_json::to_vec(&generated.graph.nodes).expect("serialize nodes"),
        serde_json::to_vec(&changed_cost.graph.nodes).expect("serialize nodes")
    );
}

#[test]
fn gridgen_supply_stream_is_independent_and_seed_changes_the_instance() {
    let family = |total_supply| FlowGeneratorFamilyV1::GridgenGrid {
        rows: 3,
        columns: 4,
        terminal_pairs: 2,
        average_degree: 3,
        total_supply,
        two_way: true,
        minimum_capacity: 3,
        maximum_capacity: 9,
        minimum_cost: 2,
        maximum_cost: 7,
    };
    let generated = generate_flow_graph(&fixed_spec(family(20))).expect("base graph");
    let changed_supply =
        generate_flow_graph(&fixed_spec(family(21))).expect("changed total supply");
    let ordinary = |graph: &FlowGraphV1| {
        graph
            .edges
            .iter()
            .filter(|edge| edge.from != "super" && edge.to != "super")
            .map(|edge| {
                (
                    edge.from.clone(),
                    edge.to.clone(),
                    edge.capacity.clone(),
                    edge.cost.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(ordinary(&generated.graph), ordinary(&changed_supply.graph));
    assert_eq!(
        generated
            .graph
            .nodes
            .iter()
            .filter(|node| node.supply != "0")
            .map(|node| &node.id)
            .collect::<Vec<_>>(),
        changed_supply
            .graph
            .nodes
            .iter()
            .filter(|node| node.supply != "0")
            .map(|node| &node.id)
            .collect::<Vec<_>>()
    );
    assert_ne!(
        generated
            .graph
            .nodes
            .iter()
            .map(|node| &node.supply)
            .collect::<Vec<_>>(),
        changed_supply
            .graph
            .nodes
            .iter()
            .map(|node| &node.supply)
            .collect::<Vec<_>>()
    );

    let mut changed_seed = fixed_spec(family(20));
    changed_seed.seed = "43".to_owned();
    let changed_seed = generate_flow_graph(&changed_seed).expect("different seed");
    assert_ne!(
        serde_json::to_vec(&generated.graph).expect("serialize graph"),
        serde_json::to_vec(&changed_seed.graph).expect("serialize graph")
    );
}

#[test]
fn gridgen_extra_arcs_sample_ordered_pairs_with_replacement() {
    let generated = generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::GridgenGrid {
        rows: 3,
        columns: 4,
        terminal_pairs: 2,
        average_degree: 8,
        total_supply: 20,
        two_way: true,
        minimum_capacity: 3,
        maximum_capacity: 9,
        minimum_cost: 2,
        maximum_cost: 7,
    }))
    .expect("dense GRIDGEN-derived graph");
    assert_eq!(generated.graph.edges.len(), 13 * 8);

    let ordinary = generated
        .graph
        .edges
        .iter()
        .filter(|edge| edge.from != "super" && edge.to != "super")
        .collect::<Vec<_>>();
    assert!(ordinary.iter().all(|edge| edge.from != edge.to));
    let unique_pairs = ordinary
        .iter()
        .map(|edge| (&edge.from, &edge.to))
        .collect::<BTreeSet<_>>();
    assert!(
        unique_pairs.len() < ordinary.len(),
        "replacement sampling must preserve deterministic parallel arcs"
    );
}

#[test]
fn gridgen_grid_one_way_orientation_and_limits_are_explicit() {
    let one_way = generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::GridgenGrid {
        rows: 2,
        columns: 3,
        terminal_pairs: 1,
        average_degree: 1,
        total_supply: 5,
        two_way: false,
        minimum_capacity: 1,
        maximum_capacity: 1,
        minimum_cost: 0,
        maximum_cost: 0,
    }))
    .expect("one-way GRIDGEN-derived graph");
    assert_eq!(
        (one_way.graph.nodes.len(), one_way.graph.edges.len()),
        (7, 9)
    );
    let endpoints = one_way
        .graph
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect::<BTreeSet<_>>();
    assert!(endpoints.contains(&("g0000c0000", "g0000c0001")));
    assert!(endpoints.contains(&("g0000c0001", "g0000c0002")));
    assert!(endpoints.contains(&("g0001c0001", "g0001c0000")));
    assert!(endpoints.contains(&("g0001c0002", "g0001c0001")));
    assert!(endpoints.contains(&("g0000c0000", "g0001c0000")));
    assert!(endpoints.contains(&("g0001c0001", "g0000c0001")));
    assert!(endpoints.contains(&("g0000c0002", "g0001c0002")));
}

#[test]
fn gridgen_grid_rejects_generic_attribute_drift() {
    assert!(matches!(
        generate_flow_graph(&spec(FlowGeneratorFamilyV1::GridgenGrid {
            rows: 3,
            columns: 4,
            terminal_pairs: 1,
            average_degree: 2,
            total_supply: 5,
            two_way: true,
            minimum_capacity: 1,
            maximum_capacity: 9,
            minimum_cost: 0,
            maximum_cost: 7,
        })),
        Err(FlowGenerationError::Invalid(
            "family attributes are fixed by the declared construction"
        ))
    ));
}

#[test]
fn gridgen_grid_rejects_invalid_source_parameters() {
    for family in [
        FlowGeneratorFamilyV1::GridgenGrid {
            rows: 1,
            columns: 4,
            terminal_pairs: 1,
            average_degree: 2,
            total_supply: 5,
            two_way: true,
            minimum_capacity: 1,
            maximum_capacity: 9,
            minimum_cost: 0,
            maximum_cost: 7,
        },
        FlowGeneratorFamilyV1::GridgenGrid {
            rows: 3,
            columns: 4,
            terminal_pairs: 7,
            average_degree: 2,
            total_supply: 7,
            two_way: true,
            minimum_capacity: 1,
            maximum_capacity: 9,
            minimum_cost: 0,
            maximum_cost: 7,
        },
        FlowGeneratorFamilyV1::GridgenGrid {
            rows: 3,
            columns: 4,
            terminal_pairs: 2,
            average_degree: 13,
            total_supply: 20,
            two_way: true,
            minimum_capacity: 1,
            maximum_capacity: 9,
            minimum_cost: 0,
            maximum_cost: 7,
        },
        FlowGeneratorFamilyV1::GridgenGrid {
            rows: 3,
            columns: 4,
            terminal_pairs: 2,
            average_degree: 2,
            total_supply: 1,
            two_way: true,
            minimum_capacity: 1,
            maximum_capacity: 9,
            minimum_cost: 0,
            maximum_cost: 7,
        },
        FlowGeneratorFamilyV1::GridgenGrid {
            rows: 3,
            columns: 4,
            terminal_pairs: 2,
            average_degree: 2,
            total_supply: 20,
            two_way: true,
            minimum_capacity: 10,
            maximum_capacity: 9,
            minimum_cost: 0,
            maximum_cost: 7,
        },
        FlowGeneratorFamilyV1::GridgenGrid {
            rows: 3,
            columns: 4,
            terminal_pairs: 2,
            average_degree: 2,
            total_supply: 20,
            two_way: true,
            minimum_capacity: 1,
            maximum_capacity: 9,
            minimum_cost: 8,
            maximum_cost: 7,
        },
    ] {
        assert!(generate_flow_graph(&fixed_spec(family)).is_err());
    }
}

#[test]
fn gridgen_grid_enforces_materialized_graph_limits() {
    let family = |rows, columns, average_degree| FlowGeneratorFamilyV1::GridgenGrid {
        rows,
        columns,
        terminal_pairs: 1,
        average_degree,
        total_supply: 1,
        two_way: false,
        minimum_capacity: 0,
        maximum_capacity: 0,
        minimum_cost: 0,
        maximum_cost: 0,
    };
    let exact_limits = generate_flow_graph(&fixed_spec(family(11, 909, 10)))
        .expect("exact node and edge limits are admitted");
    assert_eq!(exact_limits.graph.nodes.len(), MAX_FLOW_NODES);
    assert_eq!(exact_limits.graph.edges.len(), MAX_FLOW_EDGES);

    for oversized in [family(10, 1_000, 1), family(5, 1_000, 20)] {
        assert!(matches!(
            generate_flow_graph(&fixed_spec(oversized)),
            Err(FlowGenerationError::SizeLimit)
        ));
    }
    assert!(matches!(
        generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::GridgenGrid {
            rows: 100,
            columns: 100,
            terminal_pairs: 1,
            average_degree: 10,
            total_supply: 1,
            two_way: true,
            minimum_capacity: 0,
            maximum_capacity: 0,
            minimum_cost: 0,
            maximum_cost: 0,
        })),
        Err(FlowGenerationError::SizeLimit)
    ));
}

fn gridgraph_family(
    rows: u32,
    columns: u32,
    maximum_capacity: u32,
    maximum_cost: u32,
) -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::GridgraphGrid {
        rows,
        columns,
        maximum_capacity,
        maximum_cost,
    }
}

#[test]
fn gridgraph_grid_materializes_source_semantics_and_exact_maximum_flow() {
    let generated = generate_flow_graph(&fixed_spec(gridgraph_family(4, 5, 9, 17)))
        .expect("GRIDGRAPH-derived graph");
    assert_eq!(
        (generated.graph.nodes.len(), generated.graph.edges.len()),
        (22, 39)
    );
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(
        generated.provenance.source_id,
        "resende-gridgraph-1991-ggraph1-project-rng-derived"
    );
    assert!(matches!(
        generated.suggested_model,
        FlowProblemModelV1::Transshipment {}
    ));
    assert_eq!(
        generated.provenance.materialized_sha256,
        "343955f9802b2d9dcab42e59a49d566a1acab2094dedb64225f3a76dccc7fcb5"
    );

    let source_supply = generated.graph.nodes[0]
        .supply
        .parse::<u64>()
        .expect("source supply");
    assert!(source_supply > 0);
    assert_eq!(generated.graph.nodes.last().expect("sink").id, "t");
    assert_eq!(
        generated.graph.nodes.last().expect("sink").supply,
        format!("-{source_supply}")
    );
    assert!(
        generated.graph.nodes[1..generated.graph.nodes.len() - 1]
            .iter()
            .all(|node| node.supply == "0")
    );
    let (reference_value, _) = reference_dinic(&generated.graph);
    assert_eq!(source_supply, reference_value);

    let grid_edges = &generated.graph.edges[..31];
    assert!(grid_edges.iter().all(|edge| {
        (1..=9).contains(&edge.capacity.parse::<u64>().expect("capacity"))
            && (1..=17).contains(&edge.cost.parse::<i64>().expect("cost"))
    }));
    assert!(
        generated
            .graph
            .edges
            .iter()
            .all(|edge| { (1..=17).contains(&edge.cost.parse::<i64>().expect("cost")) })
    );
    for row in 0..4 {
        let first = gridgraph_id(row, 0);
        let last = gridgraph_id(row, 4);
        let expected_source_capacity = grid_edges
            .iter()
            .filter(|edge| edge.from == first)
            .map(|edge| edge.capacity.parse::<u64>().expect("capacity"))
            .sum::<u64>();
        let expected_sink_capacity = grid_edges
            .iter()
            .filter(|edge| edge.to == last)
            .map(|edge| edge.capacity.parse::<u64>().expect("capacity"))
            .sum::<u64>();
        let source_edge = generated
            .graph
            .edges
            .iter()
            .find(|edge| edge.from == "s" && edge.to == first)
            .expect("source terminal arc");
        let sink_edge = generated
            .graph
            .edges
            .iter()
            .find(|edge| edge.from == last && edge.to == "t")
            .expect("sink terminal arc");
        assert_eq!(source_edge.capacity, expected_source_capacity.to_string());
        assert_eq!(sink_edge.capacity, expected_sink_capacity.to_string());
    }

    let network = canonical_network(&generated.graph);
    let supplies = generated
        .graph
        .nodes
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                node.supply.parse::<i128>().expect("canonical supply"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let required = network
        .nodes()
        .iter()
        .map(|node| supplies[node.id().as_str()])
        .collect::<Vec<_>>();
    solve_cost_scaling(&network, &required).expect("maximum-flow balances are feasible");
}

#[test]
fn gridgraph_square_wide_and_long_shapes_have_exact_directed_grid_counts() {
    for (rows, columns) in [(6, 6), (8, 4), (4, 8)] {
        let generated = generate_flow_graph(&fixed_spec(gridgraph_family(rows, columns, 100, 200)))
            .expect("GRIDGRAPH shape");
        let expected_nodes = usize::try_from(rows * columns + 2).expect("small fixture");
        let expected_edges =
            usize::try_from(2 * rows * columns + rows - columns).expect("small fixture");
        assert_eq!(generated.graph.nodes.len(), expected_nodes);
        assert_eq!(generated.graph.edges.len(), expected_edges);
        let supply = generated.graph.nodes[0]
            .supply
            .parse::<u64>()
            .expect("source supply");
        assert_eq!(supply, reference_dinic(&generated.graph).0);
        assert!(generated.graph.edges.iter().all(|edge| {
            let from_grid = edge.from.starts_with('q');
            let to_grid = edge.to.starts_with('q');
            if from_grid && to_grid {
                let from_row = edge.from[1..5].parse::<i32>().expect("row");
                let from_column = edge.from[6..10].parse::<i32>().expect("column");
                let to_row = edge.to[1..5].parse::<i32>().expect("row");
                let to_column = edge.to[6..10].parse::<i32>().expect("column");
                (to_row - from_row, to_column - from_column) == (0, 1)
                    || (to_row - from_row, to_column - from_column) == (1, 0)
            } else {
                edge.from == "s" || edge.to == "t"
            }
        }));
    }
}

#[test]
fn gridgraph_rng_streams_are_reproducible_and_parameter_independent() {
    let generated = generate_flow_graph(&fixed_spec(gridgraph_family(4, 5, 9, 17)))
        .expect("base GRIDGRAPH graph");
    let repeated =
        generate_flow_graph(&fixed_spec(gridgraph_family(4, 5, 9, 17))).expect("same seed");
    assert_eq!(
        serde_json::to_vec(&generated).expect("serialize"),
        serde_json::to_vec(&repeated).expect("serialize")
    );

    let changed_capacity = generate_flow_graph(&fixed_spec(gridgraph_family(4, 5, 101, 17)))
        .expect("changed capacity maximum");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.cost))
            .collect::<Vec<_>>(),
        changed_capacity
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.cost))
            .collect::<Vec<_>>()
    );

    let mut different_seed_spec = fixed_spec(gridgraph_family(4, 5, 9, 17));
    different_seed_spec.seed = "43".to_owned();
    let different_seed =
        generate_flow_graph(&different_seed_spec).expect("different GRIDGRAPH seed");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>(),
        different_seed
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>()
    );
    assert!(
        generated
            .graph
            .edges
            .iter()
            .zip(&different_seed.graph.edges)
            .any(|(left, right)| left.capacity != right.capacity || left.cost != right.cost)
    );

    let endpoint_prefix = generated
        .graph
        .edges
        .iter()
        .take(4)
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        endpoint_prefix,
        vec![
            ("q0000c0000", "q0000c0001"),
            ("q0000c0000", "q0001c0000"),
            ("q0000c0001", "q0000c0002"),
            ("q0000c0001", "q0001c0001"),
        ]
    );

    let changed_cost = generate_flow_graph(&fixed_spec(gridgraph_family(4, 5, 9, 101)))
        .expect("changed cost maximum");
    assert_eq!(
        serde_json::to_vec(&generated.graph.nodes).expect("serialize nodes"),
        serde_json::to_vec(&changed_cost.graph.nodes).expect("serialize nodes")
    );
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.capacity))
            .collect::<Vec<_>>(),
        changed_cost
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.capacity))
            .collect::<Vec<_>>()
    );
}

#[test]
fn gridgraph_rejects_attribute_drift_invalid_parameters_and_impractical_sizes() {
    assert!(matches!(
        generate_flow_graph(&spec(gridgraph_family(4, 5, 9, 17))),
        Err(FlowGenerationError::Invalid(
            "family attributes are fixed by the declared construction"
        ))
    ));
    for family in [
        gridgraph_family(1, 5, 9, 17),
        gridgraph_family(4, 1, 9, 17),
        gridgraph_family(4, 2, 9, 17),
        gridgraph_family(4, 5, 0, 17),
        gridgraph_family(4, 5, 9, 0),
        gridgraph_family(4, 5, 1_000_000_001, 17),
        gridgraph_family(4, 5, 9, 1_000_000_001),
    ] {
        assert!(generate_flow_graph(&fixed_spec(family)).is_err());
    }
    let exact_limit = generate_flow_graph(&fixed_spec(gridgraph_family(2, 999, 1, 1)))
        .expect("2,000-node internal solver boundary");
    assert_eq!(exact_limit.graph.nodes.len(), 2_000);
    assert!(matches!(
        generate_flow_graph(&fixed_spec(gridgraph_family(2, 1_000, 1, 1))),
        Err(FlowGenerationError::SizeLimit)
    ));
}

fn washington_random_level_family(
    rows: u32,
    columns: u32,
    maximum_capacity: u32,
) -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::WashingtonRandomLevel {
        rows,
        columns,
        maximum_capacity,
    }
}

fn washington_mesh_family(rows: u32, columns: u32, maximum_capacity: u32) -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::WashingtonMesh {
        rows,
        columns,
        maximum_capacity,
    }
}

fn washington_square_mesh_family(
    dimension: u32,
    degree: u32,
    maximum_capacity: u32,
) -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::WashingtonSquareMesh {
        dimension,
        degree,
        maximum_capacity,
    }
}

fn washington_matching_family(part_size: u32, degree: u32) -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::WashingtonMatching { part_size, degree }
}

#[test]
fn washington_matching_matches_function_four_unit_bipartite_structure() {
    let generated = generate_flow_graph(&fixed_spec(washington_matching_family(12, 3)))
        .expect("Washington Matching graph");
    assert_eq!(
        (generated.graph.nodes.len(), generated.graph.edges.len()),
        (26, 60)
    );
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(
        generated.provenance.tags,
        [
            "bipartite",
            "dag",
            "unit-capacity",
            "unit-network",
            "washington-matching",
        ]
    );
    assert_eq!(
        generated.provenance.source_id,
        "anderson-washington-matching-1991-project-rng-derived"
    );
    assert!(generated.provenance.difficulty_certificate.is_none());
    assert!(matches!(
        generated.suggested_model,
        FlowProblemModelV1::BipartiteMatching {
            ref left,
            ref right,
            flow_adapter: Some(ref adapter),
        } if left.len() == 12
            && right.len() == 12
            && adapter.source == "s"
            && adapter.sink == "t"
    ));
    assert!(
        generated
            .graph
            .edges
            .iter()
            .all(|edge| edge.capacity == "1" && edge.cost == "0")
    );

    let source_edges = &generated.graph.edges[..12];
    let matching_edges = &generated.graph.edges[12..48];
    let sink_edges = &generated.graph.edges[48..];
    assert!(
        source_edges
            .iter()
            .enumerate()
            .all(|(left, edge)| { edge.from == "s" && edge.to == format!("l{left:04}") })
    );
    assert!(
        sink_edges
            .iter()
            .enumerate()
            .all(|(right, edge)| { edge.from == format!("r{right:04}") && edge.to == "t" })
    );
    for left in 0..12 {
        let from = washington_matching_id('l', left);
        let outgoing = matching_edges
            .iter()
            .filter(|edge| edge.from == from)
            .collect::<Vec<_>>();
        assert_eq!(outgoing.len(), 3);
        let targets = outgoing
            .iter()
            .map(|edge| edge.to.as_str())
            .collect::<Vec<_>>();
        assert_eq!(targets.iter().copied().collect::<BTreeSet<_>>().len(), 3);
        assert!(targets.windows(2).all(|pair| pair[0] < pair[1]));
    }
    assert_eq!(
        generated.provenance.materialized_sha256,
        "8b4a1ae4baf6222749c5af1bdf555fcc90901b981ac8361a871f8ba7ee747ec5"
    );
}

#[test]
fn washington_matching_is_reproducible_seed_sensitive_and_practically_bounded() {
    let family = washington_matching_family(12, 3);
    let generated = generate_flow_graph(&fixed_spec(family.clone())).expect("base graph");
    let repeated = generate_flow_graph(&fixed_spec(family)).expect("same seed graph");
    assert_eq!(
        serde_json::to_vec(&generated).expect("serialize"),
        serde_json::to_vec(&repeated).expect("serialize")
    );

    let mut changed_seed_spec = fixed_spec(washington_matching_family(12, 3));
    changed_seed_spec.seed = "43".to_owned();
    let changed_seed = generate_flow_graph(&changed_seed_spec).expect("changed seed graph");
    assert!(
        generated
            .graph
            .edges
            .iter()
            .zip(&changed_seed.graph.edges)
            .any(|(left, right)| (left.from.as_str(), left.to.as_str())
                != (right.from.as_str(), right.to.as_str()))
    );

    assert!(generate_flow_graph(&spec(washington_matching_family(12, 3))).is_err());
    for family in [
        washington_matching_family(1, 1),
        washington_matching_family(12, 0),
        washington_matching_family(12, 13),
        washington_matching_family(1_000, 1),
    ] {
        assert!(generate_flow_graph(&fixed_spec(family)).is_err());
    }
    let exact_limit = generate_flow_graph(&fixed_spec(washington_matching_family(999, 18)))
        .expect("2,000-node and 20,000-edge admission boundary");
    assert_eq!(
        (exact_limit.graph.nodes.len(), exact_limit.graph.edges.len()),
        (2_000, 19_980)
    );
    assert!(matches!(
        generate_flow_graph(&fixed_spec(washington_matching_family(999, 19))),
        Err(FlowGenerationError::SizeLimit)
    ));
}

#[test]
fn washington_mesh_matches_function_one_cylindrical_neighbors() {
    let generated = generate_flow_graph(&fixed_spec(washington_mesh_family(6, 8, 9)))
        .expect("Washington Mesh graph");
    assert_eq!(
        (generated.graph.nodes.len(), generated.graph.edges.len()),
        (50, 138)
    );
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(
        generated.provenance.tags,
        ["cylindrical", "dag", "grid", "washington-mesh"]
    );
    assert_eq!(
        generated.provenance.source_id,
        "anderson-washington-mesh-1991-project-rng-derived"
    );
    assert!(generated.provenance.difficulty_certificate.is_none());

    let source_edges = &generated.graph.edges[..6];
    let level_edges = &generated.graph.edges[6..132];
    let sink_edges = &generated.graph.edges[132..];
    assert!(
        source_edges
            .iter()
            .all(|edge| { edge.from == "s" && edge.capacity == "27" && edge.cost == "0" })
    );
    assert!(
        sink_edges
            .iter()
            .all(|edge| { edge.to == "t" && edge.capacity == "27" && edge.cost == "0" })
    );
    for column in 0..7 {
        for row in 0..6 {
            let offset = usize::try_from((column * 6 + row) * 3).expect("bounded offset");
            let outgoing = &level_edges[offset..offset + 3];
            assert!(
                outgoing
                    .iter()
                    .all(|edge| edge.from == washington_mesh_id(column, row))
            );
            assert_eq!(
                outgoing
                    .iter()
                    .map(|edge| edge.to.clone())
                    .collect::<Vec<_>>(),
                [(row + 5) % 6, row, (row + 1) % 6]
                    .map(|target| washington_mesh_id(column + 1, target))
            );
            assert!(outgoing.iter().all(|edge| {
                (1..=9).contains(&edge.capacity.parse::<u64>().expect("capacity"))
                    && edge.cost == "0"
            }));
        }
    }
    assert_eq!(
        generated.provenance.materialized_sha256,
        "bf54bdb21a5acfc3b1dba0cedec7fbb58e5ee30aaa1aa48ff028aac2e5bd7612"
    );
}

#[test]
fn washington_mesh_is_reproducible_and_practically_bounded() {
    let family = washington_mesh_family(6, 8, 9);
    let generated = generate_flow_graph(&fixed_spec(family.clone())).expect("base graph");
    let repeated = generate_flow_graph(&fixed_spec(family)).expect("same seed graph");
    assert_eq!(
        serde_json::to_vec(&generated).expect("serialize"),
        serde_json::to_vec(&repeated).expect("serialize")
    );

    let mut changed_seed_spec = fixed_spec(washington_mesh_family(6, 8, 9));
    changed_seed_spec.seed = "43".to_owned();
    let changed_seed = generate_flow_graph(&changed_seed_spec).expect("changed seed graph");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>(),
        changed_seed
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>()
    );
    assert!(
        generated
            .graph
            .edges
            .iter()
            .zip(&changed_seed.graph.edges)
            .any(|(left, right)| left.capacity != right.capacity)
    );

    assert!(generate_flow_graph(&spec(washington_mesh_family(6, 8, 9))).is_err());
    for family in [
        washington_mesh_family(2, 8, 9),
        washington_mesh_family(6, 1, 9),
        washington_mesh_family(6, 8, 0),
        washington_mesh_family(6, 8, 100_000_001),
    ] {
        assert!(generate_flow_graph(&fixed_spec(family)).is_err());
    }
    let exact_limit = generate_flow_graph(&fixed_spec(washington_mesh_family(3, 666, 1)))
        .expect("2,000-node internal solver boundary");
    assert_eq!(exact_limit.graph.nodes.len(), 2_000);
    assert!(matches!(
        generate_flow_graph(&fixed_spec(washington_mesh_family(3, 667, 1))),
        Err(FlowGenerationError::SizeLimit)
    ));
}

#[test]
fn washington_square_mesh_matches_function_five_forward_offsets() {
    let generated = generate_flow_graph(&fixed_spec(washington_square_mesh_family(6, 3, 9)))
        .expect("Washington Square Mesh graph");
    assert_eq!(
        (generated.graph.nodes.len(), generated.graph.edges.len()),
        (38, 99)
    );
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(
        generated.provenance.tags,
        ["dag", "grid", "washington-square-mesh"]
    );
    assert_eq!(
        generated.provenance.source_id,
        "anderson-washington-square-mesh-1991-project-rng-derived"
    );
    assert!(generated.provenance.difficulty_certificate.is_none());

    let source_edges = &generated.graph.edges[..6];
    let internal_edges = &generated.graph.edges[6..93];
    let sink_edges = &generated.graph.edges[93..];
    assert!(
        source_edges
            .iter()
            .all(|edge| { edge.from == "s" && edge.capacity == "27" && edge.cost == "0" })
    );
    assert!(
        sink_edges
            .iter()
            .all(|edge| edge.to == "t" && edge.capacity == "27" && edge.cost == "0")
    );
    assert!(internal_edges.iter().all(|edge| {
        (1..=9).contains(&edge.capacity.parse::<u64>().expect("capacity")) && edge.cost == "0"
    }));

    let expected_internal = (0_u32..5)
        .flat_map(|column| {
            (0_u32..6).flat_map(move |row| {
                (0_u32..3).filter_map(move |offset| {
                    let target = column * 6 + row + 6 + offset;
                    (target < 36).then(|| {
                        (
                            washington_square_mesh_id(column, row),
                            washington_square_mesh_id(target / 6, target % 6),
                        )
                    })
                })
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        internal_edges
            .iter()
            .map(|edge| (edge.from.clone(), edge.to.clone()))
            .collect::<Vec<_>>(),
        expected_internal
    );
    assert_eq!(
        internal_edges[15..18]
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
            .collect::<Vec<_>>(),
        [
            ("q0000r0005", "q0001r0005"),
            ("q0000r0005", "q0002r0000"),
            ("q0000r0005", "q0002r0001"),
        ]
    );
    assert_eq!(
        generated.provenance.materialized_sha256,
        "225c466bcb9858d6788d7c68431835fdeb5be7413f428b05cff8abdc43eb747e"
    );
}

#[test]
fn washington_square_mesh_is_reproducible_and_practically_bounded() {
    let family = washington_square_mesh_family(6, 3, 9);
    let generated = generate_flow_graph(&fixed_spec(family.clone())).expect("base graph");
    let repeated = generate_flow_graph(&fixed_spec(family)).expect("same seed graph");
    assert_eq!(
        serde_json::to_vec(&generated).expect("serialize"),
        serde_json::to_vec(&repeated).expect("serialize")
    );

    let mut changed_seed_spec = fixed_spec(washington_square_mesh_family(6, 3, 9));
    changed_seed_spec.seed = "43".to_owned();
    let changed_seed = generate_flow_graph(&changed_seed_spec).expect("changed seed graph");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>(),
        changed_seed
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>()
    );
    assert!(
        generated
            .graph
            .edges
            .iter()
            .zip(&changed_seed.graph.edges)
            .any(|(left, right)| left.capacity != right.capacity)
    );

    assert!(generate_flow_graph(&spec(washington_square_mesh_family(6, 3, 9))).is_err());
    for family in [
        washington_square_mesh_family(1, 1, 9),
        washington_square_mesh_family(6, 0, 9),
        washington_square_mesh_family(6, 7, 9),
        washington_square_mesh_family(6, 3, 0),
        washington_square_mesh_family(6, 3, 100_000_001),
    ] {
        assert!(generate_flow_graph(&fixed_spec(family)).is_err());
    }
    let practical_limit =
        generate_flow_graph(&fixed_spec(washington_square_mesh_family(44, 10, 1)))
            .expect("largest admitted dimension with bounded degree");
    assert_eq!(
        (
            practical_limit.graph.nodes.len(),
            practical_limit.graph.edges.len()
        ),
        (1_938, 18_963)
    );
    assert!(matches!(
        generate_flow_graph(&fixed_spec(washington_square_mesh_family(44, 11, 1))),
        Err(FlowGenerationError::SizeLimit)
    ));
    assert!(generate_flow_graph(&fixed_spec(washington_square_mesh_family(45, 1, 1))).is_err());
}

fn washington_line_ordinal(id: &str, width: u32) -> i64 {
    let level = id[1..5].parse::<i64>().expect("line level");
    let row = id[6..10].parse::<i64>().expect("line row");
    level * i64::from(width) + row
}

#[test]
fn washington_line_functions_preserve_signed_offsets_and_capacity_profiles() {
    let families = [
        FlowGeneratorFamilyV1::WashingtonBasicLine {
            levels: 6,
            width: 4,
            degree: 3,
        },
        FlowGeneratorFamilyV1::WashingtonExponentialLine {
            levels: 6,
            width: 4,
            degree: 3,
        },
        FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine {
            levels: 6,
            width: 4,
            degree: 3,
        },
    ];
    let mut generated = Vec::new();
    for family in families {
        let graph = generate_flow_graph(&fixed_spec(family)).expect("Washington Line graph");
        assert_eq!(graph.graph.nodes.len(), 26);
        assert_eq!(graph.provenance.origin, "official-benchmark-derived");
        assert_eq!(graph.provenance.sampling, "randomized");
        assert!(graph.provenance.difficulty_certificate.is_none());
        assert!(
            graph
                .graph
                .edges
                .iter()
                .all(|edge| edge.cost == "0" && edge.lower == "0")
        );
        assert!(
            graph.graph.edges[..4]
                .iter()
                .all(|edge| { edge.from == "s" && edge.capacity == "20000000" })
        );
        assert!(
            graph.graph.edges[graph.graph.edges.len() - 4..]
                .iter()
                .all(|edge| edge.to == "t" && edge.capacity == "20000000")
        );
        generated.push(graph);
    }

    for graph in &generated[..2] {
        assert!(
            graph.graph.edges[4..graph.graph.edges.len() - 4]
                .iter()
                .all(|edge| washington_line_ordinal(&edge.from, 4)
                    < washington_line_ordinal(&edge.to, 4))
        );
        assert!(
            graph.graph.edges[4..graph.graph.edges.len() - 4]
                .iter()
                .all(|edge| edge.capacity.parse::<u64>().expect("capacity") <= 1_000_000)
        );
    }
    let exponential = &generated[1];
    for edge in &exponential.graph.edges[4..exponential.graph.edges.len() - 4] {
        let offset = washington_line_ordinal(&edge.to, 4) - washington_line_ordinal(&edge.from, 4);
        let range =
            WASHINGTON_LINE_CAPACITY_RANGES[usize::try_from((offset - 1) / 4).expect("profile")];
        assert!(edge.capacity.parse::<u64>().expect("capacity") <= range);
    }
    let double = &generated[2];
    assert!(
        double.graph.edges[4..double.graph.edges.len() - 4]
            .iter()
            .any(|edge| washington_line_ordinal(&edge.from, 4)
                > washington_line_ordinal(&edge.to, 4))
    );
    for edge in &double.graph.edges[4..double.graph.edges.len() - 4] {
        let offset = washington_line_ordinal(&edge.to, 4) - washington_line_ordinal(&edge.from, 4);
        let range =
            washington_line_capacity_limit(WashingtonLineProfile::DoubleExponential, offset, 4)
                .expect("profile");
        assert!(edge.capacity.parse::<u64>().expect("capacity") <= range);
    }
    assert_eq!(generated[0].graph.edges.len(), 61);
    assert_eq!(generated[1].graph.edges.len(), 61);
    assert_eq!(generated[2].graph.edges.len(), 63);
    assert_eq!(
        generated[0].provenance.materialized_sha256,
        "bbed79f53467de4d32d53bc5c8bebc7227db392fe306531bbabb79ff8718cdcb"
    );
    assert_eq!(
        generated[1].provenance.materialized_sha256,
        "2668341066f45c105a303a4f56c481a98f945ded6c846ef67cc0eb2a212b8b42"
    );
    assert_eq!(
        generated[2].provenance.materialized_sha256,
        "b9acefcdfa72d636fa20cc487c535e1b23a764dbaed0d5177d209614a5a59aff"
    );
}

#[test]
fn washington_line_functions_are_reproducible_and_practically_bounded() {
    let family = FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine {
        levels: 8,
        width: 5,
        degree: 6,
    };
    let generated = generate_flow_graph(&fixed_spec(family.clone())).expect("base graph");
    let repeated = generate_flow_graph(&fixed_spec(family)).expect("same seed graph");
    assert_eq!(
        serde_json::to_vec(&generated).expect("serialize"),
        serde_json::to_vec(&repeated).expect("serialize")
    );
    let mut changed = fixed_spec(FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine {
        levels: 8,
        width: 5,
        degree: 6,
    });
    changed.seed = "43".to_owned();
    let changed = generate_flow_graph(&changed).expect("changed seed graph");
    assert_ne!(
        generated.provenance.materialized_sha256,
        changed.provenance.materialized_sha256
    );

    for family in [
        FlowGeneratorFamilyV1::WashingtonBasicLine {
            levels: 1,
            width: 4,
            degree: 3,
        },
        FlowGeneratorFamilyV1::WashingtonExponentialLine {
            levels: 6,
            width: 0,
            degree: 3,
        },
        FlowGeneratorFamilyV1::WashingtonBasicLine {
            levels: 6,
            width: 4,
            degree: 0,
        },
        FlowGeneratorFamilyV1::WashingtonBasicLine {
            levels: 6,
            width: 4,
            degree: 21,
        },
        FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine {
            levels: 6,
            width: 4,
            degree: 20,
        },
        FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine {
            levels: 6,
            width: 1,
            degree: 19,
        },
    ] {
        assert!(generate_flow_graph(&fixed_spec(family)).is_err());
    }
    let admitted = generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::WashingtonBasicLine {
        levels: 50,
        width: 20,
        degree: 10,
    }))
    .expect("bounded practical line");
    assert_eq!(admitted.graph.nodes.len(), 1_002);
    assert!(admitted.graph.edges.len() <= 10_040);
    assert!(matches!(
        generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::WashingtonBasicLine {
            levels: 100,
            width: 20,
            degree: 10,
        })),
        Err(FlowGenerationError::SizeLimit)
    ));
}

#[test]
fn washington_random_level_has_exact_layered_structure_and_source_capacities() {
    let generated = generate_flow_graph(&fixed_spec(washington_random_level_family(6, 8, 9)))
        .expect("Washington Random Level graph");
    assert_eq!(
        (generated.graph.nodes.len(), generated.graph.edges.len()),
        (50, 138)
    );
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(
        generated.provenance.tags,
        ["dag", "grid", "washington-random-level"]
    );
    assert_eq!(
        generated.provenance.source_id,
        "anderson-washington-random-level-1991-project-rng-derived"
    );
    assert_eq!(
        generated.provenance.materialized_sha256,
        "1ce40d5419450340a76e04a3a32b316f8f8afc85028dbc4ed70ae47e032f6601"
    );
    assert!(matches!(
        generated.suggested_model,
        FlowProblemModelV1::MaxFlow { ref source, ref sink }
            if source == "s" && sink == "t"
    ));
    assert_eq!(generated.graph.nodes[0].id, "s");
    assert_eq!(generated.graph.nodes.last().expect("sink").id, "t");

    let source_edges = &generated.graph.edges[..6];
    let level_edges = &generated.graph.edges[6..132];
    let sink_edges = &generated.graph.edges[132..];
    assert!(
        source_edges
            .iter()
            .all(|edge| { edge.from == "s" && edge.capacity == "27" && edge.cost == "0" })
    );
    assert!(
        sink_edges
            .iter()
            .all(|edge| { edge.to == "t" && edge.capacity == "27" && edge.cost == "0" })
    );
    assert!(level_edges.iter().all(|edge| {
        (1..=9).contains(&edge.capacity.parse::<u64>().expect("capacity")) && edge.cost == "0"
    }));
    for column in 0..7 {
        for row in 0..6 {
            let from = washington_random_level_id(column, row);
            let outgoing = level_edges
                .iter()
                .filter(|edge| edge.from == from)
                .collect::<Vec<_>>();
            assert_eq!(outgoing.len(), 3);
            assert_eq!(
                outgoing
                    .iter()
                    .map(|edge| edge.to.as_str())
                    .collect::<BTreeSet<_>>()
                    .len(),
                3
            );
            assert!(
                outgoing
                    .iter()
                    .all(|edge| edge.to.starts_with(&format!("w{:04}r", column + 1)))
            );
        }
    }
    assert!(reference_dinic(&generated.graph).0 > 0);
}

#[test]
fn washington_random_level_is_reproducible_domain_separated_and_practically_bounded() {
    let family = washington_random_level_family(6, 8, 9);
    let generated = generate_flow_graph(&fixed_spec(family.clone())).expect("base graph");
    let repeated = generate_flow_graph(&fixed_spec(family)).expect("same seed graph");
    assert_eq!(
        serde_json::to_vec(&generated).expect("serialize"),
        serde_json::to_vec(&repeated).expect("serialize")
    );

    let changed_capacity =
        generate_flow_graph(&fixed_spec(washington_random_level_family(6, 8, 101)))
            .expect("changed capacity range");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>(),
        changed_capacity
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>()
    );

    let mut changed_seed_spec = fixed_spec(washington_random_level_family(6, 8, 9));
    changed_seed_spec.seed = "43".to_owned();
    let changed_seed = generate_flow_graph(&changed_seed_spec).expect("changed seed graph");
    assert!(
        generated
            .graph
            .edges
            .iter()
            .zip(&changed_seed.graph.edges)
            .any(|(left, right)| left.from != right.from
                || left.to != right.to
                || left.capacity != right.capacity)
    );

    assert!(matches!(
        generate_flow_graph(&spec(washington_random_level_family(6, 8, 9))),
        Err(FlowGenerationError::Invalid(
            "family attributes are fixed by the declared construction"
        ))
    ));
    for invalid in [
        washington_random_level_family(2, 8, 9),
        washington_random_level_family(6, 1, 9),
        washington_random_level_family(6, 8, 0),
        washington_random_level_family(6, 8, 100_000_001),
    ] {
        assert!(generate_flow_graph(&fixed_spec(invalid)).is_err());
    }
    let exact_limit = generate_flow_graph(&fixed_spec(washington_random_level_family(3, 666, 1)))
        .expect("2,000-node practical boundary");
    assert_eq!(exact_limit.graph.nodes.len(), 2_000);
    assert_eq!(exact_limit.graph.edges.len(), 5_991);
    assert!(matches!(
        generate_flow_graph(&fixed_spec(washington_random_level_family(3, 667, 1))),
        Err(FlowGenerationError::SizeLimit)
    ));
}

fn goto_default_family() -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::GotoTorus {
        nodes: 256,
        edge_count: 2_048,
        maximum_capacity: 1_000,
        maximum_cost: 10_000,
    }
}

fn goto_default_graph() -> GeneratedFlowGraphV1 {
    generate_flow_graph(&fixed_spec(goto_default_family())).expect("GOTO-derived graph")
}

#[test]
fn goto_torus_materializes_source_shape_and_golden_digest() {
    let generated = goto_default_graph();
    assert_eq!(
        (generated.graph.nodes.len(), generated.graph.edges.len()),
        (256, 2_048)
    );
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(
        generated.provenance.source_id,
        "goldberg-goto-1991-project-rng-power2-derived"
    );
    assert!(matches!(
        generated.suggested_model,
        FlowProblemModelV1::Transshipment {}
    ));
    assert_eq!(
        generated.provenance.materialized_sha256,
        "2e805d1e8d70057d6c72591811af5c4441904696a75352061bdee06033dd6308"
    );

    let shape = validate_goto_config(GotoConfig {
        nodes: 256,
        edge_count: 2_048,
        maximum_capacity: 1_000,
        maximum_cost: 10_000,
    })
    .expect("source parameter band");
    assert_eq!(
        (
            shape.columns,
            shape.rows,
            shape.grid_nodes,
            shape.extra_nodes,
            shape.horizontal_degree,
            shape.vertical_degree,
            shape.extra_edges,
        ),
        (36, 7, 252, 4, 4, 2, 218)
    );

    let nonzero = generated
        .graph
        .nodes
        .iter()
        .filter(|node| node.supply != "0")
        .collect::<Vec<_>>();
    assert_eq!(nonzero.len(), 2);
    assert_eq!(nonzero[0].id, "t0000c0000");
    assert_eq!(nonzero[1].id, "t0006c0035");
    let supply = nonzero[0].supply.parse::<u64>().expect("positive supply");
    assert_eq!(nonzero[1].supply, format!("-{supply}"));
    assert!(generated.graph.nodes.iter().any(|node| node.id == "x0003"));

    let repeated = goto_default_graph();
    assert_eq!(
        serde_json::to_vec(&generated).expect("serialize"),
        serde_json::to_vec(&repeated).expect("serialize")
    );
}

#[test]
fn goto_torus_small_fixture_matches_the_opened_cut_and_supply_oracle() {
    let generated = generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::GotoTorus {
        nodes: 15,
        edge_count: 90,
        maximum_capacity: 64,
        maximum_cost: 80,
    }))
    .expect("small GOTO-derived graph");
    let opened = &generated.graph.edges[..65];
    let source = "t0000c0000";
    let sink = "t0002c0004";
    let mut expected = Vec::<(String, String, bool)>::new();
    for row in 0..3 {
        for column in 0..5 {
            let from = format!("t{row:04}c{column:04}");
            for distance in 1..=2 {
                let target_column = (column + distance) % 5;
                let to = format!("t{row:04}c{target_column:04}");
                if target_column > column {
                    expected.push((from.clone(), to.clone(), to == sink));
                } else {
                    if from != sink {
                        expected.push((from.clone(), sink.to_owned(), true));
                    }
                    if to != source {
                        expected.push((source.to_owned(), to, false));
                    }
                }
            }
            for distance in 1..=2 {
                expected.push((
                    from.clone(),
                    format!("t{:04}c{column:04}", (row + distance) % 3),
                    false,
                ));
            }
        }
    }

    assert_eq!(expected.len(), 65);
    assert_eq!(
        opened
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
            .collect::<Vec<_>>(),
        expected
            .iter()
            .map(|(from, to, _)| (from.as_str(), to.as_str()))
            .collect::<Vec<_>>()
    );
    let expected_supply = opened
        .iter()
        .zip(&expected)
        .filter(|(_, (_, _, contributes))| *contributes)
        .try_fold(8_u64, |sum, (edge, _)| {
            sum.checked_add(edge.capacity.parse::<u64>().expect("capacity"))
        })
        .expect("supply fits");
    assert_eq!(generated.graph.nodes[0].supply, expected_supply.to_string());
    assert_eq!(
        generated.graph.nodes[14].supply,
        format!("-{expected_supply}")
    );
    assert!(opened.iter().all(|edge| {
        let parse = |id: &str| {
            (
                id[1..5].parse::<u32>().expect("row"),
                id[6..10].parse::<u32>().expect("column"),
            )
        };
        let (from_row, from_column) = parse(&edge.from);
        let (to_row, to_column) = parse(&edge.to);
        from_row != to_row || from_column <= to_column
    }));
}

#[test]
fn goto_torus_preserves_chain_scatter_and_parallel_arcs() {
    let generated = goto_default_graph();
    let supply = generated.graph.nodes[0]
        .supply
        .parse::<u64>()
        .expect("positive source supply");
    let opened_torus_count = 1_574;
    let chain = &generated.graph.edges[opened_torus_count..opened_torus_count + 5];
    assert!(
        chain
            .iter()
            .all(|edge| { edge.capacity == "32" && edge.cost == "5000" && edge.from != edge.to })
    );
    let scattered = &generated.graph.edges[opened_torus_count + 5..1_797];
    assert_eq!(scattered.len(), 218);
    assert!(scattered.iter().all(|edge| {
        let grid_to_extra = edge.from.starts_with('t') && edge.to.starts_with('x');
        let extra_to_grid = edge.from.starts_with('x') && edge.to.starts_with('t');
        (grid_to_extra || extra_to_grid)
            && edge.from != "t0000c0000"
            && edge.to != "t0000c0000"
            && edge.from != "t0006c0035"
            && edge.to != "t0006c0035"
    }));
    assert!(
        scattered.iter().any(|edge| edge.from.starts_with('t'))
            && scattered.iter().any(|edge| edge.from.starts_with('x'))
    );
    let return_path = &generated.graph.edges[1_797..];
    assert_eq!(return_path.len(), 251);
    assert!(return_path.iter().all(|edge| {
        edge.capacity == supply.to_string() && edge.cost == "1428" && edge.from != edge.to
    }));
    assert!(
        generated
            .graph
            .edges
            .iter()
            .all(|edge| edge.from != edge.to)
    );
    let endpoint_pairs = generated
        .graph
        .edges
        .iter()
        .map(|edge| (&edge.from, &edge.to))
        .collect::<BTreeSet<_>>();
    assert!(endpoint_pairs.len() < generated.graph.edges.len());
}

#[test]
fn goto_torus_return_path_is_an_independent_feasibility_witness() {
    let generated = goto_default_graph();
    let supply = generated.graph.nodes[0]
        .supply
        .parse::<i128>()
        .expect("positive source supply");
    let return_path = &generated.graph.edges[1_797..];
    assert_eq!(return_path.first().expect("path starts").from, "t0000c0000");
    assert_eq!(return_path.last().expect("path ends").to, "t0006c0035");
    assert!(
        return_path
            .windows(2)
            .all(|pair| pair[0].to == pair[1].from)
    );

    let visited = std::iter::once(return_path[0].from.as_str())
        .chain(return_path.iter().map(|edge| edge.to.as_str()))
        .collect::<BTreeSet<_>>();
    assert_eq!(visited.len(), 252);
    assert!(visited.iter().all(|node_id| node_id.starts_with('t')));

    let mut divergence = generated
        .graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), 0_i128))
        .collect::<BTreeMap<_, _>>();
    for edge in return_path {
        *divergence.get_mut(edge.from.as_str()).expect("known tail") += supply;
        *divergence.get_mut(edge.to.as_str()).expect("known head") -= supply;
    }
    for node in &generated.graph.nodes {
        assert_eq!(
            divergence[node.id.as_str()],
            node.supply.parse::<i128>().expect("canonical supply")
        );
    }
}

#[test]
fn goto_torus_rng_streams_and_integer_decay_are_explicit() {
    let family = |maximum_capacity, maximum_cost| FlowGeneratorFamilyV1::GotoTorus {
        nodes: 256,
        edge_count: 2_048,
        maximum_capacity,
        maximum_cost,
    };
    let generated = generate_flow_graph(&fixed_spec(family(1_000, 10_000))).expect("base");
    let changed_cost =
        generate_flow_graph(&fixed_spec(family(1_000, 20_000))).expect("changed cost");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.capacity))
            .collect::<Vec<_>>(),
        changed_cost
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.capacity))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        generated
            .graph
            .nodes
            .iter()
            .map(|node| (&node.id, &node.supply))
            .collect::<Vec<_>>(),
        changed_cost
            .graph
            .nodes
            .iter()
            .map(|node| (&node.id, &node.supply))
            .collect::<Vec<_>>()
    );

    let changed_capacity =
        generate_flow_graph(&fixed_spec(family(2_000, 10_000))).expect("changed capacity");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.cost))
            .collect::<Vec<_>>(),
        changed_capacity
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.cost))
            .collect::<Vec<_>>()
    );

    let base_rng = RngV1::from_seed(42, CAPACITY_RNG_DOMAIN);
    let capacities = (1..=8)
        .map(|distance| {
            let mut rng = base_rng;
            goto_distance_capacity(&mut rng, distance, 8, 1_000).expect("integer decay")
        })
        .collect::<Vec<_>>();
    let mut raw_rng = RngV1::from_seed(42, CAPACITY_RNG_DOMAIN);
    let raw = raw_rng.bounded_u64(1_000).expect("bounded raw") + 1;
    assert_eq!(raw, 678);
    assert_eq!(capacities, [678, 678, 339, 170, 85, 43, 22, 11]);
    for (index, &capacity) in capacities.iter().enumerate() {
        let distance = u64::try_from(index + 1).expect("small index");
        let exponent = ((distance - 1) * 9) / 10;
        let divisor = 1_u64 << exponent;
        #[allow(clippy::manual_div_ceil)]
        let independently_rounded = (raw + divisor - 1) / divisor;
        assert_eq!(capacity, independently_rounded);
    }
}

#[test]
fn goto_torus_source_band_and_project_caps_are_checked_exactly() {
    assert!(
        validate_goto_config(GotoConfig {
            nodes: 15,
            edge_count: 90,
            maximum_capacity: 8,
            maximum_cost: 8,
        })
        .is_ok()
    );
    assert!(
        validate_goto_config(GotoConfig {
            nodes: 10_000,
            edge_count: 100_000,
            maximum_capacity: 1_000_000_000,
            maximum_cost: 1_000_000_000,
        })
        .is_ok()
    );
    for config in [
        GotoConfig {
            nodes: 14,
            edge_count: 90,
            maximum_capacity: 8,
            maximum_cost: 8,
        },
        GotoConfig {
            nodes: 15,
            edge_count: 89,
            maximum_capacity: 8,
            maximum_cost: 8,
        },
        GotoConfig {
            nodes: 15,
            edge_count: 92,
            maximum_capacity: 8,
            maximum_cost: 8,
        },
        GotoConfig {
            nodes: 15,
            edge_count: 90,
            maximum_capacity: 7,
            maximum_cost: 8,
        },
        GotoConfig {
            nodes: 15,
            edge_count: 90,
            maximum_capacity: 8,
            maximum_cost: 7,
        },
    ] {
        assert!(validate_goto_config(config).is_err());
    }
    assert!(matches!(
        validate_goto_config(GotoConfig {
            nodes: 10_000,
            edge_count: 100_001,
            maximum_capacity: 8,
            maximum_cost: 8,
        }),
        Err(FlowGenerationError::SizeLimit)
    ));
}

fn netgen_family(config: NetgenConfig) -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::NetgenSkeleton {
        nodes: config.nodes,
        sources: config.sources,
        sinks: config.sinks,
        edge_count: config.edge_count,
        minimum_cost: config.minimum_cost,
        maximum_cost: config.maximum_cost,
        total_supply: config.total_supply,
        transshipment_sources: config.transshipment_sources,
        transshipment_sinks: config.transshipment_sinks,
        high_cost_percentage: config.high_cost_percentage,
        capacitated_percentage: config.capacitated_percentage,
        minimum_capacity: config.minimum_capacity,
        maximum_capacity: config.maximum_capacity,
    }
}

fn netgen_general_config() -> NetgenConfig {
    NetgenConfig {
        nodes: 20,
        sources: 3,
        sinks: 4,
        edge_count: 70,
        minimum_cost: -7,
        maximum_cost: 20,
        total_supply: 40,
        transshipment_sources: 1,
        transshipment_sinks: 1,
        high_cost_percentage: 100,
        capacitated_percentage: 100,
        minimum_capacity: 2,
        maximum_capacity: 9,
    }
}

#[test]
fn netgen_general_graph_has_exact_simple_structure_and_a_feasible_skeleton() {
    let config = netgen_general_config();
    let shape = validate_netgen_config(config).expect("valid NETGEN parameters");
    assert_eq!(shape.problem_kind, NetgenProblemKind::Transshipment);
    assert_eq!(shape.middle_nodes, 13);
    assert_eq!(shape.sinks_per_source, 3);
    assert_eq!(shape.skeleton_edges, 22);

    let generated = generate_flow_graph(&fixed_spec(netgen_family(config))).expect("NETGEN graph");
    assert_eq!(generated.graph.nodes.len(), 20);
    assert_eq!(generated.graph.edges.len(), 70);
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(
        generated.provenance.source_id,
        "klingman-napier-stutz-netgen-1974-project-rng-independent-derived"
    );
    assert!(matches!(
        generated.suggested_model,
        FlowProblemModelV1::Transshipment {}
    ));

    let supplies = generated
        .graph
        .nodes
        .iter()
        .map(|node| node.supply.parse::<i64>().expect("canonical balance"))
        .collect::<Vec<_>>();
    assert_eq!(supplies.iter().sum::<i64>(), 0);
    assert_eq!(supplies.iter().filter(|&&value| value > 0).count(), 3);
    assert_eq!(supplies.iter().filter(|&&value| value < 0).count(), 4);
    assert_eq!(supplies.iter().filter(|&&value| value == 0).count(), 13);
    assert_eq!(supplies.iter().filter(|&&value| value > 0).sum::<i64>(), 40);

    let endpoints = generated
        .graph
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect::<BTreeSet<_>>();
    assert_eq!(endpoints.len(), generated.graph.edges.len());
    assert!(endpoints.iter().all(|(from, to)| from != to));
    assert!(
        generated
            .graph
            .edges
            .iter()
            .all(|edge| !edge.to.starts_with('s') || edge.to.starts_with("sx"))
    );
    assert!(
        generated
            .graph
            .edges
            .iter()
            .all(|edge| !edge.from.starts_with('t') || edge.from.starts_with("tx"))
    );
    assert!(
        generated.graph.edges[..as_usize(u64::from(shape.skeleton_edges)).expect("small")]
            .iter()
            .all(|edge| edge.cost == "20")
    );
    assert!(
        generated.graph.edges[as_usize(u64::from(shape.skeleton_edges)).expect("small")..]
            .iter()
            .all(|edge| (2..=9).contains(&edge.capacity.parse::<u64>().expect("capacity")))
    );

    let network = canonical_network(&generated.graph);
    let required = network
        .nodes()
        .iter()
        .map(|node| i128::from(node.supply()))
        .collect::<Vec<_>>();
    solve_cost_scaling(&network, &required).expect("skeleton guarantees feasibility");
}

#[test]
fn netgen_assignment_transportation_and_single_terminal_max_flow_are_distinct() {
    let assignment = NetgenConfig {
        nodes: 12,
        sources: 6,
        sinks: 6,
        edge_count: 24,
        minimum_cost: 0,
        maximum_cost: 9,
        total_supply: 6,
        transshipment_sources: 0,
        transshipment_sinks: 0,
        high_cost_percentage: 0,
        capacitated_percentage: 0,
        minimum_capacity: 0,
        maximum_capacity: 0,
    };
    assert_eq!(
        validate_netgen_config(assignment)
            .expect("assignment")
            .problem_kind,
        NetgenProblemKind::Assignment
    );
    let assignment =
        generate_flow_graph(&fixed_spec(netgen_family(assignment))).expect("assignment graph");
    assert!(
        assignment
            .graph
            .edges
            .iter()
            .all(|edge| edge.capacity == "1")
    );
    assert_eq!(
        assignment
            .graph
            .nodes
            .iter()
            .filter(|node| node.supply == "1")
            .count(),
        6
    );
    assert_eq!(
        assignment
            .graph
            .nodes
            .iter()
            .filter(|node| node.supply == "-1")
            .count(),
        6
    );

    let transportation = NetgenConfig {
        nodes: 12,
        sources: 5,
        sinks: 7,
        edge_count: 35,
        minimum_cost: 0,
        maximum_cost: 9,
        total_supply: 20,
        transshipment_sources: 0,
        transshipment_sinks: 0,
        ..netgen_general_config()
    };
    assert_eq!(
        validate_netgen_config(transportation)
            .expect("transportation")
            .problem_kind,
        NetgenProblemKind::Transportation
    );

    let max_flow = NetgenConfig {
        nodes: 12,
        sources: 1,
        sinks: 1,
        edge_count: 24,
        minimum_cost: 1,
        maximum_cost: 1,
        total_supply: 20,
        transshipment_sources: 0,
        transshipment_sinks: 0,
        high_cost_percentage: 0,
        capacitated_percentage: 100,
        minimum_capacity: 1,
        maximum_capacity: 20,
    };
    assert_eq!(
        validate_netgen_config(max_flow)
            .expect("max flow")
            .problem_kind,
        NetgenProblemKind::MaxFlow
    );
    let max_flow =
        generate_flow_graph(&fixed_spec(netgen_family(max_flow))).expect("max-flow graph");
    assert!(max_flow.graph.nodes.iter().all(|node| node.supply == "0"));
    assert!(matches!(
        max_flow.suggested_model,
        FlowProblemModelV1::MaxFlow { ref source, ref sink }
            if source == "s0000" && sink == "t0000"
    ));
    assert!(max_flow.graph.edges.iter().all(|edge| edge.cost == "0"));
}

#[test]
fn netgen_assignment_applies_high_cost_percentage_to_its_matching_skeleton() {
    let config = NetgenConfig {
        nodes: 12,
        sources: 6,
        sinks: 6,
        edge_count: 24,
        minimum_cost: 0,
        maximum_cost: 9,
        total_supply: 6,
        transshipment_sources: 0,
        transshipment_sinks: 0,
        high_cost_percentage: 100,
        capacitated_percentage: 0,
        minimum_capacity: 0,
        maximum_capacity: 0,
    };
    let shape = validate_netgen_config(config).expect("assignment");
    let generated =
        generate_flow_graph(&fixed_spec(netgen_family(config))).expect("assignment graph");
    let skeleton_edges = as_usize(u64::from(shape.skeleton_edges)).expect("small skeleton");
    assert!(
        generated.graph.edges[..skeleton_edges]
            .iter()
            .all(|edge| edge.capacity == "1" && edge.cost == "9")
    );
    assert!(
        generated.graph.edges[skeleton_edges..]
            .iter()
            .all(|edge| edge.capacity == "1"
                && (0..=9).contains(&edge.cost.parse::<i64>().expect("cost")))
    );
}

#[test]
fn netgen_rng_domains_are_independent_and_output_has_a_golden_digest() {
    let config = netgen_general_config();
    let generated = generate_flow_graph(&fixed_spec(netgen_family(config))).expect("base");
    let repeated = generate_flow_graph(&fixed_spec(netgen_family(config))).expect("repeat");
    assert_eq!(
        serde_json::to_vec(&generated).expect("serialize"),
        serde_json::to_vec(&repeated).expect("serialize")
    );
    assert_eq!(
        generated.provenance.materialized_sha256,
        "2ac1d5844928df365c10871aa8a21bfad514f866df90d32266d4508d74827ce3"
    );

    let changed_cost = NetgenConfig {
        minimum_cost: -30,
        maximum_cost: 50,
        ..config
    };
    let changed_cost =
        generate_flow_graph(&fixed_spec(netgen_family(changed_cost))).expect("changed cost");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.capacity))
            .collect::<Vec<_>>(),
        changed_cost
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.capacity))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        generated
            .graph
            .nodes
            .iter()
            .map(|node| (&node.id, &node.supply))
            .collect::<Vec<_>>(),
        changed_cost
            .graph
            .nodes
            .iter()
            .map(|node| (&node.id, &node.supply))
            .collect::<Vec<_>>()
    );

    let changed_capacity = NetgenConfig {
        minimum_capacity: 10,
        maximum_capacity: 30,
        ..config
    };
    let changed_capacity = generate_flow_graph(&fixed_spec(netgen_family(changed_capacity)))
        .expect("changed capacity");
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.cost))
            .collect::<Vec<_>>(),
        changed_capacity
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.cost))
            .collect::<Vec<_>>()
    );
}

type NetgenClassCase = (u32, u32, u32, u32, u32, u32, u32, NetgenProblemKind);

#[allow(clippy::too_many_arguments)]
fn extend_netgen_cases(
    cases: &mut Vec<NetgenClassCase>,
    edges: &[u32],
    nodes: u32,
    sources: u32,
    sinks: u32,
    total_supply: u32,
    tsources: u32,
    tsinks: u32,
    kind: NetgenProblemKind,
) {
    cases.extend(edges.iter().map(|&edge_count| {
        (
            nodes,
            sources,
            sinks,
            edge_count,
            total_supply,
            tsources,
            tsinks,
            kind,
        )
    }));
}

fn standard_netgen_transport_assignment_cases() -> Vec<NetgenClassCase> {
    let mut cases = Vec::new();
    extend_netgen_cases(
        &mut cases,
        &[1_300, 1_500, 2_000, 2_200, 2_900],
        200,
        100,
        100,
        100_000,
        0,
        0,
        NetgenProblemKind::Transportation,
    );
    extend_netgen_cases(
        &mut cases,
        &[3_150, 4_500, 5_155, 6_075, 6_300],
        300,
        150,
        150,
        150_000,
        0,
        0,
        NetgenProblemKind::Transportation,
    );
    extend_netgen_cases(
        &mut cases,
        &[1_500, 2_250, 3_000, 3_750, 4_500],
        400,
        200,
        200,
        200,
        0,
        0,
        NetgenProblemKind::Assignment,
    );
    cases
}

fn standard_netgen_transshipment_cases() -> Vec<NetgenClassCase> {
    let mut cases = Vec::new();
    extend_netgen_cases(
        &mut cases,
        &[1_306, 2_443, 1_306, 2_443],
        400,
        8,
        60,
        400_000,
        0,
        0,
        NetgenProblemKind::Transshipment,
    );
    extend_netgen_cases(
        &mut cases,
        &[1_416, 2_836, 1_416, 2_836],
        400,
        8,
        60,
        400_000,
        5,
        50,
        NetgenProblemKind::Transshipment,
    );
    extend_netgen_cases(
        &mut cases,
        &[1_382, 2_676, 1_382, 2_676],
        400,
        4,
        12,
        400_000,
        0,
        0,
        NetgenProblemKind::Transshipment,
    );
    extend_netgen_cases(
        &mut cases,
        &[2_900, 3_400, 4_400, 4_800],
        1_000,
        50,
        50,
        1_000_000,
        0,
        0,
        NetgenProblemKind::Transshipment,
    );
    extend_netgen_cases(
        &mut cases,
        &[4_342, 4_385, 5_107, 5_730],
        1_500,
        75,
        75,
        1_500_000,
        0,
        0,
        NetgenProblemKind::Transshipment,
    );
    cases
}

fn standard_netgen_large_cases() -> Vec<NetgenClassCase> {
    vec![
        (
            8_000,
            200,
            1_000,
            15_000,
            4_000_000,
            100,
            300,
            NetgenProblemKind::Transshipment,
        ),
        (
            5_000,
            150,
            800,
            23_000,
            4_000_000,
            50,
            100,
            NetgenProblemKind::Transshipment,
        ),
        (
            3_000,
            125,
            500,
            35_000,
            2_000_000,
            25,
            50,
            NetgenProblemKind::Transshipment,
        ),
        (
            5_000,
            180,
            700,
            15_000,
            4_000_000,
            100,
            300,
            NetgenProblemKind::Transshipment,
        ),
        (
            3_000,
            100,
            300,
            23_000,
            2_000_000,
            50,
            100,
            NetgenProblemKind::Transshipment,
        ),
    ]
}

fn standard_netgen_cases() -> Vec<NetgenClassCase> {
    let mut cases = standard_netgen_transport_assignment_cases();
    cases.extend(standard_netgen_transshipment_cases());
    cases.extend(standard_netgen_large_cases());
    cases
}

#[test]
fn netgen_standard_forty_parameter_sets_keep_their_problem_classes() {
    let cases = standard_netgen_cases();
    assert_eq!(cases.len(), 40);
    for (nodes, sources, sinks, edge_count, total_supply, tsources, tsinks, expected) in cases {
        let config = NetgenConfig {
            nodes,
            sources,
            sinks,
            edge_count,
            minimum_cost: 1,
            maximum_cost: 10_000,
            total_supply,
            transshipment_sources: tsources,
            transshipment_sinks: tsinks,
            high_cost_percentage: 30,
            capacitated_percentage: 40,
            minimum_capacity: 0,
            maximum_capacity: 120_000,
        };
        assert_eq!(
            validate_netgen_config(config)
                .expect("historical parameter class remains admissible")
                .problem_kind,
            expected
        );
    }
}

#[test]
fn netgen_allowed_pair_ranking_and_dense_complement_sampling_are_exact() {
    let config = NetgenConfig {
        nodes: 5,
        sources: 1,
        sinks: 1,
        edge_count: 5,
        minimum_cost: 0,
        maximum_cost: 2,
        total_supply: 5,
        transshipment_sources: 0,
        transshipment_sinks: 0,
        high_cost_percentage: 0,
        capacitated_percentage: 0,
        minimum_capacity: 0,
        maximum_capacity: 0,
    };
    let shape = validate_netgen_config(config).expect("small NETGEN config");
    assert_eq!(shape.allowed_edges, 13);
    let mut pairs = BTreeSet::new();
    for ordinal in 0..shape.allowed_edges {
        let pair = netgen_general_pair_from_ordinal(config, shape, ordinal).expect("pair");
        assert!(pairs.insert(pair));
        assert_eq!(
            netgen_general_pair_ordinal(config, shape, pair.0, pair.1).expect("rank"),
            ordinal
        );
    }
    let excluded = BTreeSet::from([0, 2, 5, 8]);
    let mut rng = RngV1::from_seed(42, TOPOLOGY_RNG_DOMAIN);
    let selected = netgen_sample_complement_ordinals(13, &excluded, 9, &mut rng)
        .expect("select every nonexcluded ordinal");
    assert_eq!(selected.len(), 9);
    assert!(selected.iter().all(|ordinal| !excluded.contains(ordinal)));
    assert_eq!(selected.into_iter().collect::<BTreeSet<_>>().len(), 9);

    for seed in 0..64 {
        let mut rng = RngV1::from_seed(seed, TOPOLOGY_RNG_DOMAIN);
        let selected = netgen_sample_complement_ordinals(13, &excluded, 4, &mut rng)
            .expect("partial complement sample");
        assert_eq!(selected.len(), 4);
        assert!(selected.iter().all(|ordinal| !excluded.contains(ordinal)));
        assert_eq!(selected.into_iter().collect::<BTreeSet<_>>().len(), 4);
    }
}

#[test]
fn netgen_rejects_parameter_drift_before_allocation() {
    let base = netgen_general_config();
    for config in [
        NetgenConfig {
            total_supply: 3,
            ..base
        },
        NetgenConfig {
            edge_count: 19,
            ..base
        },
        NetgenConfig {
            transshipment_sources: 4,
            ..base
        },
        NetgenConfig {
            high_cost_percentage: 101,
            ..base
        },
        NetgenConfig {
            minimum_capacity: 10,
            maximum_capacity: 9,
            ..base
        },
        NetgenConfig {
            minimum_cost: 2,
            maximum_cost: 1,
            ..base
        },
        NetgenConfig {
            minimum_cost: 1,
            maximum_cost: 1,
            sources: 3,
            sinks: 4,
            ..base
        },
    ] {
        assert!(validate_netgen_config(config).is_err());
    }
    assert!(matches!(
        generate_flow_graph(&spec(netgen_family(base))),
        Err(FlowGenerationError::Invalid(
            "family attributes are fixed by the declared construction"
        ))
    ));
    let boundary = NetgenConfig {
        nodes: 10_000,
        sources: 100,
        sinks: 100,
        edge_count: 100_000,
        total_supply: 1_000_000_000,
        ..base
    };
    assert!(validate_netgen_config(boundary).is_ok());
}

#[test]
fn same_seed_is_byte_identical_and_attribute_streams_are_independent() {
    let request = spec(FlowGeneratorFamilyV1::ErdosRenyiDirected {
        nodes: 25,
        edge_count: 60,
    });
    let first =
        serde_json::to_vec(&generate_flow_graph(&request).expect("generated")).expect("serializes");
    let second =
        serde_json::to_vec(&generate_flow_graph(&request).expect("generated")).expect("serializes");
    assert_eq!(first, second);

    let mut changed_cost = request;
    changed_cost.cost = CostDistributionV1::Constant {
        value: "99".to_owned(),
    };
    let changed = generate_flow_graph(&changed_cost).expect("generated");
    let original: GeneratedFlowGraphV1 =
        serde_json::from_slice(&first).expect("generated JSON round trip");
    assert_eq!(
        original.provenance.materialized_sha256,
        "aa81ebc4df12e63bacf306d60a313ae06fbf922ec73ccba2d90f3c4eb211704d"
    );
    assert_eq!(
        original
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.capacity))
            .collect::<Vec<_>>(),
        changed
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to, &edge.capacity))
            .collect::<Vec<_>>()
    );
}

#[test]
fn advanced_attribute_distributions_are_exact_bounded_and_reproducible() {
    let mut request = spec(FlowGeneratorFamilyV1::Path { nodes: 96 });
    request.capacity = CapacityDistributionV1::Bimodal {
        first: "2".to_owned(),
        second: "101".to_owned(),
    };
    request.cost = CostDistributionV1::Bimodal {
        first: "-7".to_owned(),
        second: "13".to_owned(),
    };
    let bimodal = generate_flow_graph(&request).expect("bimodal generation succeeds");
    let capacities = bimodal
        .graph
        .edges
        .iter()
        .map(|edge| edge.capacity.as_str())
        .collect::<BTreeSet<_>>();
    let costs = bimodal
        .graph
        .edges
        .iter()
        .map(|edge| edge.cost.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(capacities, BTreeSet::from(["101", "2"]));
    assert_eq!(costs, BTreeSet::from(["-7", "13"]));

    request.capacity = CapacityDistributionV1::PowerOfTwoBuckets {
        minimum_exponent: 0,
        maximum_exponent: 6,
    };
    request.cost = CostDistributionV1::CapacityCorrelated {
        minimum: "-30".to_owned(),
        maximum: "30".to_owned(),
        direction: CapacityCostCorrelationV1::Positive,
        maximum_jitter: "0".to_owned(),
    };
    let correlated = generate_flow_graph(&request).expect("correlated generation succeeds");
    for edge in &correlated.graph.edges {
        let capacity = edge.capacity.parse::<u64>().expect("canonical capacity");
        let cost = edge.cost.parse::<i64>().expect("canonical cost");
        assert!(capacity.is_power_of_two() && capacity <= 64);
        let expected = -30 + i64::try_from((capacity - 1) * 60 / 63).expect("small value");
        assert_eq!(cost, expected);
    }
    let repeated = generate_flow_graph(&request).expect("repeated generation succeeds");
    assert_eq!(
        serde_json::to_vec(&correlated).expect("serializes"),
        serde_json::to_vec(&repeated).expect("serializes")
    );
}

#[test]
fn advanced_attribute_distributions_reject_degenerate_domains() {
    let mut request = spec(FlowGeneratorFamilyV1::Path { nodes: 4 });
    request.capacity = CapacityDistributionV1::Bimodal {
        first: "5".to_owned(),
        second: "5".to_owned(),
    };
    assert!(matches!(
        generate_flow_graph(&request),
        Err(FlowGenerationError::Invalid(
            "bimodal capacity atoms must differ"
        ))
    ));
    request.capacity = CapacityDistributionV1::PowerOfTwoBuckets {
        minimum_exponent: 0,
        maximum_exponent: 64,
    };
    assert!(matches!(
        generate_flow_graph(&request),
        Err(FlowGenerationError::Invalid(
            "capacity power-of-two exponent interval"
        ))
    ));
    request.capacity = CapacityDistributionV1::Unit {};
    request.cost = CostDistributionV1::CapacityCorrelated {
        minimum: "-3".to_owned(),
        maximum: "3".to_owned(),
        direction: CapacityCostCorrelationV1::Negative,
        maximum_jitter: "-1".to_owned(),
    };
    assert!(matches!(
        generate_flow_graph(&request),
        Err(FlowGenerationError::Invalid(
            "maximum cost jitter is negative"
        ))
    ));
}

#[test]
fn erdos_renyi_is_simple_bounded_and_does_not_scan_quadratic_candidates() {
    let generated = generate_flow_graph(&spec(FlowGeneratorFamilyV1::ErdosRenyiDirected {
        nodes: 10_000,
        edge_count: 10,
    }))
    .expect("sparse graph generation succeeds");
    let endpoints = generated
        .graph
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect::<BTreeSet<_>>();
    assert_eq!(endpoints.len(), 10);
    assert!(endpoints.iter().all(|(from, to)| from != to));
}

#[test]
fn preflight_rejects_dense_request_before_edge_allocation() {
    assert!(matches!(
        generate_flow_graph(&spec(FlowGeneratorFamilyV1::CompleteDag { nodes: 10_000 })),
        Err(FlowGenerationError::SizeLimit)
    ));
}

#[test]
fn strict_json_rejects_unknown_fields_and_noncanonical_seed() {
    let source = serde_json::json!({
        "generator_revision": FLOW_GENERATOR_REVISION,
        "seed": "01",
        "family": { "family_id": "path", "nodes": 4 },
        "capacity": { "kind": "unit" },
        "cost": { "kind": "zero" },
        "future": true
    })
    .to_string();
    assert!(matches!(
        generate_flow_graph_json(&source),
        Err(FlowGenerationError::Json(_))
    ));
}

#[test]
fn generator_json_size_limit_is_exact_without_decoding() {
    assert!(matches!(
        validate_generator_input_size(MAX_FLOW_GENERATOR_SPEC_BYTES),
        Ok(())
    ));
    assert!(matches!(
        validate_generator_input_size(MAX_FLOW_GENERATOR_SPEC_BYTES + 1),
        Err(FlowGenerationError::InputSize)
    ));
}

#[test]
fn grid_family_uses_the_contractual_kebab_case_identifier() {
    let source = serde_json::json!({
        "generator_revision": FLOW_GENERATOR_REVISION,
        "seed": "42",
        "family": {
            "family_id": "grid-2d",
            "rows": 3,
            "columns": 4,
            "diagonals": false
        },
        "capacity": { "kind": "unit" },
        "cost": { "kind": "zero" }
    })
    .to_string();
    let generated = generate_flow_graph_json(&source).expect("grid-2d is canonical");
    assert_eq!(generated.graph.nodes.len(), 12);
    assert_eq!(generated.graph.edges.len(), 17);

    let legacy_spelling = source.replace("grid-2d", "grid2d");
    assert!(matches!(
        generate_flow_graph_json(&legacy_spelling),
        Err(FlowGenerationError::Json(_))
    ));
}

#[test]
fn sourced_dinic_family_reaches_exact_worst_case_phase_count() {
    for nodes in 2..=16 {
        let mut request = spec(FlowGeneratorFamilyV1::DinicWorstCase { nodes });
        request.capacity = CapacityDistributionV1::Unit {};
        request.cost = CostDistributionV1::Zero {};
        let generated = generate_flow_graph(&request).expect("source construction generates");
        assert_eq!(generated.graph.edges.len(), (2 * nodes - 3) as usize);
        assert_eq!(
            reference_dinic(&generated.graph),
            (u64::from(nodes - 1), (nodes - 1) as usize)
        );
        assert_eq!(generated.provenance.difficulty, "verified-worst-case");
        assert_eq!(
            generated.provenance.source_id,
            "waissi-dinic-worst-case-1991"
        );
        let certificate = generated
            .provenance
            .difficulty_certificate
            .as_ref()
            .expect("verified worst case has a difficulty certificate");
        assert_eq!(certificate.target_algorithm_id, "dinic");
        assert_eq!(
            certificate.tie_breaking,
            "stable-residual-id-level-bfs-current-arc-dfs"
        );
        assert_eq!(
            certificate.exact_metrics,
            BTreeMap::from([
                ("bfs-runs".to_owned(), nodes.to_string()),
                ("blocking-flow-phases".to_owned(), (nodes - 1).to_string()),
                ("max-flow-value".to_owned(), (nodes - 1).to_string()),
            ])
        );
    }
}

#[test]
fn sourced_dinic_family_rejects_attribute_drift() {
    assert!(matches!(
        generate_flow_graph(&spec(FlowGeneratorFamilyV1::DinicWorstCase { nodes: 8 })),
        Err(FlowGenerationError::Invalid(
            "family attributes are fixed by the declared construction"
        ))
    ));
}

#[test]
fn washington_dinic_phase_stress_matches_function_nine_and_measured_phases() {
    for nodes in 2..=16 {
        let generated = generate_flow_graph(&fixed_spec(
            FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { nodes },
        ))
        .expect("Washington function 9 construction generates");
        assert_eq!(generated.graph.edges.len(), (2 * nodes - 3) as usize);
        assert_eq!(generated.provenance.origin, "official-benchmark-derived");
        assert_eq!(generated.provenance.sampling, "deterministic");
        assert_eq!(generated.provenance.difficulty, "stress");
        assert_eq!(
            generated.provenance.tags,
            [
                "dag",
                "dinic",
                "phase-chain",
                "washington-dinic-phase-stress"
            ]
        );
        assert_eq!(
            generated.provenance.source_id,
            "anderson-washington-dinic-bad-case-1991-derived"
        );
        assert!(generated.provenance.difficulty_certificate.is_none());
        let expected_value = if nodes == 2 { 2 } else { u64::from(nodes + 1) };
        let (value, blocking_phases) = reference_dinic(&generated.graph);
        assert_eq!(value, expected_value);
        assert_eq!(blocking_phases, (nodes - 1) as usize);
        assert_eq!(blocking_phases + 1, nodes as usize);

        let chain = &generated.graph.edges[..(nodes - 1) as usize];
        let shortcuts = &generated.graph.edges[(nodes - 1) as usize..];
        assert!(
            chain
                .iter()
                .all(|edge| edge.capacity == nodes.to_string() && edge.cost == "0")
        );
        assert!(
            shortcuts
                .iter()
                .all(|edge| edge.to == "t" && edge.capacity == "1" && edge.cost == "0")
        );
    }

    let generated = generate_flow_graph(&fixed_spec(
        FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { nodes: 8 },
    ))
    .expect("golden Washington function 9 construction");
    assert_eq!(
        generated.provenance.materialized_sha256,
        "bcbddfa6bd3aeed87a3b3303041bc2e900f58e6719400b5a4b5a036b4146a0d1"
    );
    for nodes in [1, 2_001] {
        assert!(
            generate_flow_graph(&fixed_spec(
                FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { nodes }
            ))
            .is_err()
        );
    }
    let mut drift = fixed_spec(FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { nodes: 8 });
    drift.capacity = CapacityDistributionV1::Constant {
        value: "2".to_owned(),
    };
    assert!(generate_flow_graph(&drift).is_err());
}

#[test]
fn washington_goldberg_fifo_stress_matches_function_ten_and_measured_work() {
    for (block_size, fifo_pushes, fifo_relabels) in
        [(8, 193, 110), (16, 573, 326), (32, 1_895, 1_076)]
    {
        let generated = generate_flow_graph(&fixed_spec(
            FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { block_size },
        ))
        .expect("Washington function 10 construction generates");
        assert_eq!(generated.graph.nodes.len(), (3 * block_size + 3) as usize);
        assert_eq!(generated.graph.edges.len(), (4 * block_size + 1) as usize);
        assert_eq!(generated.provenance.origin, "official-benchmark-derived");
        assert_eq!(generated.provenance.sampling, "deterministic");
        assert_eq!(generated.provenance.difficulty, "stress");
        assert_eq!(
            generated.provenance.tags,
            [
                "dag",
                "fifo",
                "push-relabel",
                "washington-goldberg-fifo-stress"
            ]
        );
        assert_eq!(
            generated.provenance.source_id,
            "anderson-washington-gold-bad-case-1991-derived"
        );
        assert!(generated.provenance.difficulty_certificate.is_none());

        let graph = canonical_network(&generated.graph);
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        let fifo = solve_fifo_push_relabel(&graph, source, sink).expect("bounded FIFO run");
        let highest = solve_highest_label_push_relabel(&graph, source, sink)
            .expect("bounded highest-label run");
        assert_eq!(fifo.certificate.value, i128::from(block_size));
        assert_eq!(fifo.metrics.pushes, fifo_pushes);
        assert_eq!(fifo.metrics.relabels, fifo_relabels);
        assert_eq!(highest.metrics.pushes, u64::from(5 * block_size));
        assert_eq!(highest.metrics.relabels, u64::from(4 * block_size));

        for (index, edge) in generated.graph.edges.iter().enumerate() {
            let bottleneck = index >= 2
                && index < (3 * block_size + 1) as usize
                && (index - 2).is_multiple_of(3);
            assert_eq!(
                edge.capacity,
                if bottleneck {
                    "1".to_owned()
                } else {
                    block_size.to_string()
                }
            );
            assert_eq!(edge.cost, "0");
        }
    }

    let generated = generate_flow_graph(&fixed_spec(
        FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { block_size: 8 },
    ))
    .expect("golden Washington function 10 construction");
    assert_eq!(
        generated.provenance.materialized_sha256,
        "efbf76dfb8ba408ded5a8b332ebd1a3020842c66b29df2494b775217caca11d9"
    );
    for block_size in [1, 65] {
        assert!(
            generate_flow_graph(&fixed_spec(
                FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { block_size }
            ))
            .is_err()
        );
    }
    let mut drift =
        fixed_spec(FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { block_size: 8 });
    drift.cost = CostDistributionV1::Constant {
        value: "1".to_owned(),
    };
    assert!(generate_flow_graph(&drift).is_err());
}

fn assert_washington_cheriyan_attributes(generated: &GeneratedFlowGraphV1) {
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "deterministic");
    assert_eq!(generated.provenance.difficulty, "stress");
    assert_eq!(
        generated.provenance.tags,
        [
            "dag",
            "push-relabel",
            "unit-bottleneck",
            "washington-cheriyan-stress"
        ]
    );
    assert_eq!(
        generated.provenance.source_id,
        "anderson-washington-cheriyan-1991-derived"
    );
    assert!(generated.provenance.difficulty_certificate.is_none());

    let capacity_counts =
        generated
            .graph
            .edges
            .iter()
            .fold(BTreeMap::<&str, usize>::new(), |mut counts, edge| {
                *counts.entry(edge.capacity.as_str()).or_default() += 1;
                assert_eq!(edge.cost, "0");
                counts
            });
    assert_eq!(
        capacity_counts,
        BTreeMap::from([("1", 8), ("8", 34), ("1000000", 33)])
    );
    let source_capacity = generated
        .graph
        .edges
        .iter()
        .filter(|edge| edge.from == "s")
        .map(|edge| edge.capacity.parse::<u64>().expect("capacity"))
        .sum::<u64>();
    assert_eq!(source_capacity, 64);
}

fn assert_washington_cheriyan_solver_metrics(generated: &GeneratedFlowGraphV1) {
    let graph = canonical_network(&generated.graph);
    let source = graph
        .node_index(&NodeId::parse("s").expect("source id"))
        .expect("source");
    let sink = graph
        .node_index(&NodeId::parse("t").expect("sink id"))
        .expect("sink");
    let generic =
        solve_generic_push_relabel(&graph, source, sink).expect("bounded generic push-relabel run");
    let fifo =
        solve_fifo_push_relabel(&graph, source, sink).expect("bounded FIFO push-relabel run");
    let global = solve_global_relabel_push_relabel(&graph, source, sink)
        .expect("bounded global-relabel run");
    assert_eq!(generic.certificate.value, 64);
    assert_eq!(fifo.certificate.value, 64);
    assert_eq!(global.certificate.value, 64);
    assert_eq!(
        (generic.metrics.pushes, generic.metrics.relabels),
        (222, 136)
    );
    assert_eq!((fifo.metrics.pushes, fifo.metrics.relabels), (175, 117));
    assert_eq!((global.metrics.pushes, global.metrics.relabels), (141, 26));
}

fn assert_washington_cheriyan_bounds() {
    for family in [
        FlowGeneratorFamilyV1::WashingtonCheriyanStress {
            bridge_width: 0,
            gadget_entries: 4,
            chain_length: 2,
        },
        FlowGeneratorFamilyV1::WashingtonCheriyanStress {
            bridge_width: 65,
            gadget_entries: 4,
            chain_length: 2,
        },
        FlowGeneratorFamilyV1::WashingtonCheriyanStress {
            bridge_width: 8,
            gadget_entries: 0,
            chain_length: 2,
        },
        FlowGeneratorFamilyV1::WashingtonCheriyanStress {
            bridge_width: 8,
            gadget_entries: 13,
            chain_length: 2,
        },
        FlowGeneratorFamilyV1::WashingtonCheriyanStress {
            bridge_width: 8,
            gadget_entries: 4,
            chain_length: 0,
        },
        FlowGeneratorFamilyV1::WashingtonCheriyanStress {
            bridge_width: 8,
            gadget_entries: 4,
            chain_length: 11,
        },
    ] {
        assert!(generate_flow_graph(&fixed_spec(family)).is_err());
    }

    let boundary = generate_flow_graph(&fixed_spec(
        FlowGeneratorFamilyV1::WashingtonCheriyanStress {
            bridge_width: 64,
            gadget_entries: 12,
            chain_length: 10,
        },
    ))
    .expect("declared practical boundary generates");
    assert_eq!(boundary.graph.nodes.len(), 615);
    assert_eq!(boundary.graph.edges.len(), 723);
    let boundary_graph = canonical_network(&boundary.graph);
    let boundary_source = boundary_graph
        .node_index(&NodeId::parse("s").expect("source id"))
        .expect("source");
    let boundary_sink = boundary_graph
        .node_index(&NodeId::parse("t").expect("sink id"))
        .expect("sink");
    assert_eq!(
        solve_fifo_push_relabel(&boundary_graph, boundary_source, boundary_sink)
            .expect("FIFO remains within the interactive work ceiling")
            .certificate
            .value,
        1_536
    );
}

#[test]
fn washington_cheriyan_stress_matches_function_eleven_actual_construction() {
    let family = FlowGeneratorFamilyV1::WashingtonCheriyanStress {
        bridge_width: 8,
        gadget_entries: 4,
        chain_length: 2,
    };
    let generated = generate_flow_graph(&fixed_spec(family.clone()))
        .expect("Washington function 11 construction generates");
    assert_eq!(generated.graph.nodes.len(), 55);
    assert_eq!(generated.graph.edges.len(), 75);
    assert_ne!(
        generated.graph.nodes.len(),
        4 * 4 * 2 + 8 + 6,
        "the archived source's printed node formula is not its allocation"
    );
    assert_washington_cheriyan_attributes(&generated);
    assert_washington_cheriyan_solver_metrics(&generated);
    assert_eq!(
        generated.provenance.materialized_sha256,
        "7fbdd77e43cff42ffc73c6417cba2054b79f6f85bd8ec4d5b04a5af1f8fd37cc"
    );
    assert_washington_cheriyan_bounds();

    let mut drift = fixed_spec(family);
    drift.capacity = CapacityDistributionV1::Constant {
        value: "8".to_owned(),
    };
    assert!(generate_flow_graph(&drift).is_err());
}

fn assert_ak_size_metrics(size: u32, expected_metrics: [(u64, u64); 4]) {
    let generated = generate_flow_graph(&fixed_spec(
        FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { size },
    ))
    .expect("AK construction generates");
    assert_eq!(generated.graph.nodes.len(), (4 * size + 6) as usize);
    assert_eq!(generated.graph.edges.len(), (6 * size + 7) as usize);
    assert_eq!(generated.provenance.origin, "paper-derived");
    assert_eq!(generated.provenance.sampling, "deterministic");
    assert_eq!(generated.provenance.difficulty, "stress");
    assert_eq!(
        generated.provenance.tags,
        ["ak", "cherkassky-goldberg-ak-stress", "dag", "push-relabel"]
    );
    assert_eq!(
        generated.provenance.source_id,
        "cherkassky-goldberg-ak-1997-independent-derived"
    );
    assert!(generated.provenance.difficulty_certificate.is_none());
    assert!(generated.graph.edges.iter().all(|edge| edge.cost == "0"));
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .filter(|edge| edge.capacity == "1000000")
            .count(),
        4
    );

    let graph = canonical_network(&generated.graph);
    let source = graph
        .node_index(&NodeId::parse("s").expect("source id"))
        .expect("source");
    let sink = graph
        .node_index(&NodeId::parse("t").expect("sink id"))
        .expect("sink");
    let results = [
        solve_generic_push_relabel(&graph, source, sink).expect("bounded generic run"),
        solve_fifo_push_relabel(&graph, source, sink).expect("bounded FIFO run"),
        solve_highest_label_push_relabel(&graph, source, sink).expect("bounded highest run"),
        solve_global_relabel_push_relabel(&graph, source, sink).expect("bounded global run"),
    ];
    let expected_value = i128::from(2 * size + 3);
    for (result, metrics) in results.iter().zip(expected_metrics) {
        assert_eq!(result.certificate.value, expected_value);
        assert_eq!((result.metrics.pushes, result.metrics.relabels), metrics);
    }
    if size == 4 {
        assert_eq!(
            generated.provenance.materialized_sha256,
            "651fed4b907c898c8397be15ede35adba01d8cdc88ba7cb4eb29d5b7bd8d54a4"
        );
    }
}

fn assert_ak_practical_boundary() {
    let boundary = generate_flow_graph(&fixed_spec(
        FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { size: 128 },
    ))
    .expect("practical AK boundary generates");
    assert_eq!(boundary.graph.nodes.len(), 518);
    assert_eq!(boundary.graph.edges.len(), 775);
    let boundary_graph = canonical_network(&boundary.graph);
    let boundary_source = boundary_graph
        .node_index(&NodeId::parse("s").expect("source id"))
        .expect("source");
    let boundary_sink = boundary_graph
        .node_index(&NodeId::parse("t").expect("sink id"))
        .expect("sink");
    assert_eq!(
        solve_fifo_push_relabel(&boundary_graph, boundary_source, boundary_sink)
            .expect("FIFO remains within the interactive work ceiling")
            .certificate
            .value,
        259
    );
}

#[test]
fn cherkassky_goldberg_ak_preserves_the_two_deterministic_gadgets() {
    for (size, expected_metrics) in [
        (2_u32, [(20, 14), (32, 23), (20, 14), (22, 6)]),
        (4, [(30, 22), (65, 52), (30, 22), (45, 13)]),
        (8, [(50, 38), (162, 130), (50, 38), (121, 35)]),
        (16, [(90, 70), (432, 350), (90, 70), (393, 114)]),
        (32, [(170, 134), (1_328, 1_082), (170, 134), (1_457, 438)]),
    ] {
        assert_ak_size_metrics(size, expected_metrics);
    }
    for size in [1, 129] {
        assert!(
            generate_flow_graph(&fixed_spec(
                FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { size }
            ))
            .is_err()
        );
    }
    assert_ak_practical_boundary();

    let mut drift = fixed_spec(FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { size: 4 });
    drift.cost = CostDistributionV1::Constant {
        value: "1".to_owned(),
    };
    assert!(generate_flow_graph(&drift).is_err());
}

#[test]
fn waissi_setubal_ac_preserves_the_official_dense_forward_sample_space() {
    let family = FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { nodes: 12 };
    let generated = generate_flow_graph(&fixed_spec(family.clone()))
        .expect("First DIMACS AC construction generates");
    assert_eq!(generated.graph.nodes.len(), 12);
    assert_eq!(generated.graph.edges.len(), 66);
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(
        generated.provenance.tags,
        [
            "acyclic-dense",
            "dag",
            "dimacs",
            "fully-dense",
            "waissi-setubal-acyclic-dense"
        ]
    );
    assert_eq!(
        generated.provenance.source_id,
        "waissi-setubal-ac-1991-project-rng-derived"
    );
    assert!(generated.provenance.difficulty_certificate.is_none());
    assert!(generated.graph.edges.iter().all(|edge| {
        edge.cost == "0"
            && edge
                .capacity
                .parse::<u64>()
                .is_ok_and(|value| (1..=1_000_000).contains(&value))
    }));
    let mut edge_index = 0;
    for from in 0..12_u32 {
        for to in from + 1..12 {
            let edge = &generated.graph.edges[edge_index];
            assert_eq!(
                (edge.from.clone(), edge.to.clone()),
                (node_id(from, 12), node_id(to, 12))
            );
            edge_index += 1;
        }
    }
    let graph = canonical_network(&generated.graph);
    let source = graph
        .node_index(&NodeId::parse("s").expect("source id"))
        .expect("source");
    let sink = graph
        .node_index(&NodeId::parse("t").expect("sink id"))
        .expect("sink");
    solve_dinic(&graph, source, sink).expect("dense benchmark remains solvable");

    let mut changed_seed = fixed_spec(family.clone());
    changed_seed.seed = "43".to_owned();
    let changed = generate_flow_graph(&changed_seed).expect("second seed generates");
    assert!(
        generated
            .graph
            .edges
            .iter()
            .zip(&changed.graph.edges)
            .any(|(left, right)| left.capacity != right.capacity)
    );
    assert!(
        generated
            .graph
            .edges
            .iter()
            .zip(&changed.graph.edges)
            .all(|(left, right)| (left.from.as_str(), left.to.as_str())
                == (right.from.as_str(), right.to.as_str()))
    );
    assert_eq!(
        generated.provenance.materialized_sha256,
        "1b0c63b6a6be19947e1a6918588ab4200a2845eeba5387fadc56f3b86c79dbdd"
    );

    assert!(
        generate_flow_graph(&fixed_spec(
            FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { nodes: 200 }
        ))
        .is_ok()
    );
    assert!(
        generate_flow_graph(&fixed_spec(
            FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { nodes: 201 }
        ))
        .is_err()
    );
    let mut drift = fixed_spec(family);
    drift.cost = CostDistributionV1::Constant {
        value: "1".to_owned(),
    };
    assert!(generate_flow_graph(&drift).is_err());
}

fn assert_glover_dense_attributes(generated: &GeneratedFlowGraphV1) {
    assert_eq!(generated.graph.nodes.len(), 12);
    assert_eq!(generated.graph.edges.len(), 66);
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "deterministic");
    assert_eq!(generated.provenance.difficulty, "stress");
    assert_eq!(
        generated.provenance.tags,
        [
            "acyclic-dense",
            "dag",
            "fully-dense",
            "glover",
            "glover-dense-acyclic-stress"
        ]
    );
    assert_eq!(
        generated.provenance.source_id,
        "waissi-glover-dense-acyclic-1991-derived"
    );
    assert!(generated.provenance.difficulty_certificate.is_none());
    let mut edge_index = 0;
    for from in 0..12_u32 {
        for to in from + 1..12 {
            let edge = &generated.graph.edges[edge_index];
            assert_eq!(
                (edge.from.clone(), edge.to.clone()),
                (node_id(from, 12), node_id(to, 12))
            );
            assert_eq!(
                edge.capacity,
                if to == from + 1 {
                    glover_dense_chain_capacity(from, 12)
                        .expect("chain capacity")
                        .to_string()
                } else {
                    "1".to_owned()
                }
            );
            assert_eq!(edge.cost, "0");
            edge_index += 1;
        }
    }
    assert_eq!(
        generated
            .graph
            .edges
            .iter()
            .map(|edge| edge.capacity.parse::<u64>().expect("capacity"))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([1, 2, 5, 10, 17, 26])
    );
    assert_eq!(
        generated.provenance.materialized_sha256,
        "d60100312ad2fa1770012512ba91f6cdce4da656ecd0bfb5b4d00de53ab05320"
    );
}

fn assert_glover_dense_solver_metrics(generated: &GeneratedFlowGraphV1) {
    let graph = canonical_network(&generated.graph);
    let source = graph
        .node_index(&NodeId::parse("s").expect("source id"))
        .expect("source");
    let sink = graph
        .node_index(&NodeId::parse("t").expect("sink id"))
        .expect("sink");
    let dinic = solve_dinic(&graph, source, sink).expect("bounded Dinic run");
    let fifo = solve_fifo_push_relabel(&graph, source, sink).expect("bounded FIFO run");
    let highest =
        solve_highest_label_push_relabel(&graph, source, sink).expect("bounded highest-label run");
    let expected_flow = i128::from(glover_dense_chain_capacity(0, 12).expect("source chain")) + 10;
    assert_eq!(dinic.certificate.value, expected_flow);
    assert_eq!(fifo.certificate.value, expected_flow);
    assert_eq!(highest.certificate.value, expected_flow);
    assert_eq!(
        (
            dinic.metrics.bfs_runs,
            dinic.metrics.blocking_flow_phases,
            dinic.metrics.augmentations
        ),
        (12, 11, 36)
    );
    assert_eq!((fifo.metrics.pushes, fifo.metrics.relabels), (66, 10));
    assert_eq!((highest.metrics.pushes, highest.metrics.relabels), (66, 10));
}

fn assert_glover_dense_dinic_growth() {
    for nodes in [2_u32, 3, 4, 5, 8, 12, 16] {
        let sample = generate_flow_graph(&fixed_spec(
            FlowGeneratorFamilyV1::GloverDenseAcyclicStress { nodes },
        ))
        .expect("bounded Glover-Waissi sample generates");
        let sample_graph = canonical_network(&sample.graph);
        let sample_source = sample_graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sample_sink = sample_graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        let sample_result = solve_dinic(&sample_graph, sample_source, sample_sink)
            .expect("bounded Glover-Waissi Dinic run");
        let sample_value = glover_dense_chain_capacity(0, nodes).expect("source chain capacity")
            + u64::from(nodes - 2);
        assert_eq!(sample_result.certificate.value, i128::from(sample_value));
        assert_eq!(sample_result.metrics.bfs_runs, u64::from(nodes));
        assert_eq!(
            sample_result.metrics.blocking_flow_phases,
            u64::from(nodes - 1)
        );
        assert_eq!(sample_result.metrics.augmentations, sample_value);
    }
}

fn assert_glover_dense_seed_and_bounds(family: FlowGeneratorFamilyV1) {
    let generated = generate_flow_graph(&fixed_spec(family.clone()))
        .expect("first deterministic graph generates");
    let mut changed_seed = fixed_spec(family.clone());
    changed_seed.seed = "43".to_owned();
    let changed = generate_flow_graph(&changed_seed).expect("second seed generates");
    assert_eq!(
        serde_json::to_value(&generated.graph).expect("first graph serializes"),
        serde_json::to_value(&changed.graph).expect("second graph serializes")
    );
    assert!(
        generate_flow_graph(&fixed_spec(
            FlowGeneratorFamilyV1::GloverDenseAcyclicStress { nodes: 200 }
        ))
        .is_ok()
    );
    assert!(
        generate_flow_graph(&fixed_spec(
            FlowGeneratorFamilyV1::GloverDenseAcyclicStress { nodes: 201 }
        ))
        .is_err()
    );
    let mut drift = fixed_spec(family);
    drift.capacity = CapacityDistributionV1::Constant {
        value: "2".to_owned(),
    };
    assert!(generate_flow_graph(&drift).is_err());
}

#[test]
fn glover_dense_acyclic_stress_preserves_waissis_special_capacities() {
    let family = FlowGeneratorFamilyV1::GloverDenseAcyclicStress { nodes: 12 };
    let generated = generate_flow_graph(&fixed_spec(family.clone()))
        .expect("Glover-Waissi dense stress generates");
    assert_glover_dense_attributes(&generated);
    assert_glover_dense_solver_metrics(&generated);
    assert_glover_dense_dinic_growth();
    assert_glover_dense_seed_and_bounds(family);
}

fn assert_waissi_transit_attributes(generated: &GeneratedFlowGraphV1) {
    assert_eq!(generated.graph.nodes.len(), 18);
    assert_eq!(generated.graph.edges.len(), 64);
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(
        generated.provenance.tags,
        [
            "bidirectional",
            "grid",
            "transit-grid",
            "waissi-transit-two-way-grid"
        ]
    );
    assert_eq!(
        generated.provenance.source_id,
        "waissi-transit-two-way-grid-1991-project-rng-derived"
    );
    assert!(generated.provenance.difficulty_certificate.is_none());
    let endpoints = generated
        .graph
        .edges
        .iter()
        .map(|edge| {
            assert_eq!(edge.lower, "0");
            assert_eq!(edge.cost, "0");
            assert!(
                edge.capacity
                    .parse::<u64>()
                    .is_ok_and(|capacity| (1..=100).contains(&capacity))
            );
            (edge.from.clone(), edge.to.clone())
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(endpoints.len(), 64);
    assert!(
        endpoints
            .iter()
            .all(|(from, to)| endpoints.contains(&(to.clone(), from.clone())))
    );
    for row in 0..4 {
        assert!(endpoints.contains(&("s".to_owned(), waissi_transit_id(row, 0))));
        assert!(endpoints.contains(&(waissi_transit_id(row, 0), "s".to_owned())));
        assert!(endpoints.contains(&(waissi_transit_id(row, 3), "t".to_owned())));
        assert!(endpoints.contains(&("t".to_owned(), waissi_transit_id(row, 3))));
    }
    let graph = canonical_network(&generated.graph);
    let source = graph
        .node_index(&NodeId::parse("s").expect("source id"))
        .expect("source");
    let sink = graph
        .node_index(&NodeId::parse("t").expect("sink id"))
        .expect("sink");
    solve_dinic(&graph, source, sink).expect("transit grid remains solvable");
    assert_eq!(
        generated.provenance.materialized_sha256,
        "06cbfcce4a3eea0e71755c711e2a9a6fcec9c7deb26f52c691136e7efca86435"
    );
}

fn assert_waissi_transit_seed_and_bounds(family: FlowGeneratorFamilyV1) {
    let generated =
        generate_flow_graph(&fixed_spec(family.clone())).expect("first transit grid generates");
    let mut changed_seed = fixed_spec(family.clone());
    changed_seed.seed = "43".to_owned();
    let changed = generate_flow_graph(&changed_seed).expect("second seed generates");
    assert!(
        generated
            .graph
            .edges
            .iter()
            .zip(&changed.graph.edges)
            .any(|(left, right)| left.capacity != right.capacity)
    );
    assert!(
        generated
            .graph
            .edges
            .iter()
            .zip(&changed.graph.edges)
            .all(|(left, right)| (left.from.as_str(), left.to.as_str())
                == (right.from.as_str(), right.to.as_str()))
    );
    assert!(
        generate_flow_graph(&fixed_spec(
            FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid {
                dimension: 44,
                maximum_capacity: 1_000_000_000,
            }
        ))
        .is_ok()
    );
    for invalid in [
        FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid {
            dimension: 1,
            maximum_capacity: 100,
        },
        FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid {
            dimension: 45,
            maximum_capacity: 100,
        },
        FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid {
            dimension: 4,
            maximum_capacity: 0,
        },
    ] {
        assert!(generate_flow_graph(&fixed_spec(invalid)).is_err());
    }
    let mut drift = fixed_spec(family);
    drift.cost = CostDistributionV1::Constant {
        value: "1".to_owned(),
    };
    assert!(generate_flow_graph(&drift).is_err());
}

fn waissi_one_way_default_graph() -> GeneratedFlowGraphV1 {
    generate_flow_graph(&fixed_spec(
        FlowGeneratorFamilyV1::WaissiTransitOneWayGrid {
            dimension: 4,
            maximum_capacity: 100,
        },
    ))
    .expect("Waissi one-way transit grid generates")
}

fn assert_waissi_one_way_streets(generated: &GeneratedFlowGraphV1) {
    let endpoints = generated
        .graph
        .edges
        .iter()
        .map(|edge| (edge.from.clone(), edge.to.clone()))
        .collect::<BTreeSet<_>>();
    assert_eq!(endpoints.len(), 32);
    for row in 0..4 {
        let left = waissi_transit_id(row, 0);
        let right = waissi_transit_id(row, 3);
        assert!(endpoints.contains(&("s".to_owned(), left.clone())));
        assert!(!endpoints.contains(&(left, "s".to_owned())));
        assert!(endpoints.contains(&(right.clone(), "t".to_owned())));
        assert!(!endpoints.contains(&("t".to_owned(), right)));
    }
    let mut street_count = 0;
    for column in 0..4 {
        for row in 0..4 {
            for (first, second) in [
                (row + 1 < 4).then(|| {
                    (
                        waissi_transit_id(row, column),
                        waissi_transit_id(row + 1, column),
                    )
                }),
                (column + 1 < 4).then(|| {
                    (
                        waissi_transit_id(row, column),
                        waissi_transit_id(row, column + 1),
                    )
                }),
            ]
            .into_iter()
            .flatten()
            {
                assert_ne!(
                    endpoints.contains(&(first.clone(), second.clone())),
                    endpoints.contains(&(second, first))
                );
                street_count += 1;
            }
        }
    }
    assert_eq!(street_count, 24);
}

fn assert_waissi_one_way_rng_streams_are_independent() {
    assert!(waissi_street_is_reversed(0, 3).expect("bounded draw"));
    assert!(waissi_street_is_reversed(1, 3).expect("bounded draw"));
    assert!(!waissi_street_is_reversed(2, 3).expect("bounded draw"));
    assert!(waissi_street_is_reversed(0, 4).expect("bounded draw"));
    assert!(waissi_street_is_reversed(1, 4).expect("bounded draw"));
    assert!(!waissi_street_is_reversed(2, 4).expect("bounded draw"));
    assert!(!waissi_street_is_reversed(3, 4).expect("bounded draw"));

    let mut topology_rng = RngV1::from_seed(42, TOPOLOGY_RNG_DOMAIN);
    let mut capacity_rng = RngV1::from_seed(42, CAPACITY_RNG_DOMAIN);
    let base = waissi_transit_one_way_grid_topology(4, 100, &mut topology_rng, &mut capacity_rng)
        .expect("base topology");

    let mut topology_rng = RngV1::from_seed(42, TOPOLOGY_RNG_DOMAIN);
    let mut shifted_capacity_rng = RngV1::from_seed(42, CAPACITY_RNG_DOMAIN);
    shifted_capacity_rng.next_u64();
    shifted_capacity_rng.next_u64();
    let shifted =
        waissi_transit_one_way_grid_topology(4, 100, &mut topology_rng, &mut shifted_capacity_rng)
            .expect("shifted-capacity topology");

    assert_eq!(base.edges, shifted.edges);
    assert_ne!(base.fixed_capacities, shifted.fixed_capacities);
}

fn assert_waissi_one_way_seed_and_bounds() {
    let mut changed_seed = fixed_spec(FlowGeneratorFamilyV1::WaissiTransitOneWayGrid {
        dimension: 4,
        maximum_capacity: 100,
    });
    changed_seed.seed = "43".to_owned();
    let changed = generate_flow_graph(&changed_seed).expect("second seed generates");
    assert_ne!(
        waissi_one_way_default_graph()
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>(),
        changed
            .graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<Vec<_>>()
    );
    assert!(
        generate_flow_graph(&fixed_spec(
            FlowGeneratorFamilyV1::WaissiTransitOneWayGrid {
                dimension: 44,
                maximum_capacity: 1_000_000_000,
            }
        ))
        .is_ok()
    );
    for invalid in [
        FlowGeneratorFamilyV1::WaissiTransitOneWayGrid {
            dimension: 1,
            maximum_capacity: 100,
        },
        FlowGeneratorFamilyV1::WaissiTransitOneWayGrid {
            dimension: 45,
            maximum_capacity: 100,
        },
        FlowGeneratorFamilyV1::WaissiTransitOneWayGrid {
            dimension: 4,
            maximum_capacity: 0,
        },
    ] {
        assert!(generate_flow_graph(&fixed_spec(invalid)).is_err());
    }
}

#[test]
fn waissi_transit_one_way_grid_preserves_one_random_direction_per_street() {
    let generated = waissi_one_way_default_graph();
    assert_eq!(generated.graph.nodes.len(), 18);
    assert_eq!(generated.graph.edges.len(), 32);
    assert!(
        generated
            .graph
            .nodes
            .iter()
            .all(|node| node.position.is_some())
    );
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(
        generated.provenance.tags,
        [
            "grid",
            "one-way",
            "transit-grid",
            "waissi-transit-one-way-grid"
        ]
    );
    assert_eq!(
        generated.provenance.source_id,
        "waissi-transit-one-way-grid-1991-project-rng-derived"
    );
    assert!(generated.provenance.difficulty_certificate.is_none());
    assert!(generated.graph.edges.iter().all(|edge| {
        edge.lower == "0"
            && edge.cost == "0"
            && edge
                .capacity
                .parse::<u64>()
                .is_ok_and(|capacity| (1..=100).contains(&capacity))
    }));
    assert_waissi_one_way_streets(&generated);
    let graph = canonical_network(&generated.graph);
    let source = graph
        .node_index(&NodeId::parse("s").expect("source id"))
        .expect("source");
    let sink = graph
        .node_index(&NodeId::parse("t").expect("sink id"))
        .expect("sink");
    solve_dinic(&graph, source, sink).expect("zero or positive maximum flow remains valid");
    assert_eq!(
        generated.provenance.materialized_sha256,
        "f2bd6008f50c2b3252a572afe868c0b03dd4825938e3dcb58ade6eef7091980f"
    );
    assert_waissi_one_way_rng_streams_are_independent();
    assert_waissi_one_way_seed_and_bounds();

    let mut drift = fixed_spec(FlowGeneratorFamilyV1::WaissiTransitOneWayGrid {
        dimension: 4,
        maximum_capacity: 100,
    });
    drift.capacity = CapacityDistributionV1::Constant {
        value: "2".to_owned(),
    };
    assert!(generate_flow_graph(&drift).is_err());
}

#[test]
fn waissi_transit_two_way_grid_preserves_bidirectional_street_pairs() {
    let family = FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid {
        dimension: 4,
        maximum_capacity: 100,
    };
    let generated = generate_flow_graph(&fixed_spec(family.clone()))
        .expect("Waissi two-way transit grid generates");
    assert_waissi_transit_attributes(&generated);
    assert_waissi_transit_seed_and_bounds(family);
}

#[test]
fn zadeh_inspired_stress_reaches_expected_finite_size_cubic_growth() {
    for group_size in [4_u32, 8, 12, 16, 20] {
        let generated =
            generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::ZadehPhaseChainStress {
                group_size,
            }))
            .expect("source construction generates");
        let graph = canonical_network(&generated.graph);
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        let result = solve_edmonds_karp(&graph, source, sink).expect("bounded execution");
        let expected_augmentations = u64::from(group_size).pow(3) / 4;
        let expected_edges = (3 * u64::from(group_size).pow(2)) / 2 + u64::from(group_size) - 2;
        assert_eq!(
            generated.graph.nodes.len(),
            usize::try_from(3 * group_size).expect("bounded node count")
        );
        assert_eq!(
            generated.graph.edges.len(),
            usize::try_from(expected_edges).expect("bounded edge count")
        );
        assert_eq!(result.metrics.augmentations, expected_augmentations);
        assert_eq!(result.metrics.bfs_runs, expected_augmentations + 1);
        assert_eq!(result.certificate.value, i128::from(expected_augmentations));
        assert_eq!(generated.provenance.difficulty, "stress");
        assert_eq!(generated.provenance.origin, "paper-derived");
        assert_eq!(generated.provenance.sampling, "deterministic");
        assert_eq!(
            generated.provenance.source_id,
            "zadeh-pathological-max-flow-1973-derived-phase-chain"
        );
        assert!(generated.provenance.difficulty_certificate.is_none());
    }
}

#[test]
fn sourced_zadeh_family_rejects_invalid_or_impractical_parameters() {
    for group_size in [0, 5, 8 + 1, 24] {
        assert!(
            generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::ZadehPhaseChainStress {
                group_size
            },))
            .is_err()
        );
    }
    assert!(matches!(
        generate_flow_graph(&spec(FlowGeneratorFamilyV1::ZadehPhaseChainStress {
            group_size: 8
        })),
        Err(FlowGenerationError::Invalid(
            "family attributes are fixed by the declared construction"
        ))
    ));
}

#[test]
fn structured_and_partitioned_families_preserve_their_declared_properties() {
    let strong = generate_flow_graph(&spec(FlowGeneratorFamilyV1::StronglyConnected {
        nodes: 8,
        extra_edges: 20,
    }))
    .expect("strongly connected graph");
    let strong_edges = strong
        .graph
        .edges
        .iter()
        .map(|edge| (&edge.from, &edge.to))
        .collect::<BTreeSet<_>>();
    assert_eq!(strong_edges.len(), strong.graph.edges.len());
    for index in 0..8 {
        assert!(
            strong_edges.contains(&(&format!("v{index:04}"), &format!("v{:04}", (index + 1) % 8)))
        );
    }

    let bipartite = generate_flow_graph(&spec(FlowGeneratorFamilyV1::BipartiteRandom {
        left: 7,
        right: 5,
        edge_count: 17,
    }))
    .expect("bipartite graph");
    let middle = bipartite
        .graph
        .edges
        .iter()
        .filter(|edge| edge.from.starts_with('l'))
        .collect::<Vec<_>>();
    assert_eq!(middle.len(), 17);
    assert!(middle.iter().all(|edge| edge.to.starts_with('r')));
    assert_eq!(
        middle
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<BTreeSet<_>>()
            .len(),
        17
    );

    assert!(matches!(
        generate_flow_graph(&spec(FlowGeneratorFamilyV1::Arborescence {
            branching: 100,
            depth: 3,
        })),
        Err(FlowGenerationError::SizeLimit)
    ));
}

#[test]
fn geometric_regular_and_preferential_families_preserve_their_contracts() {
    let radius = 120_i64;
    let geometric = generate_flow_graph(&spec(FlowGeneratorFamilyV1::RandomGeometric {
        nodes: 80,
        radius: u32::try_from(radius).expect("positive radius"),
    }))
    .expect("geometric graph");
    let positions = geometric
        .graph
        .nodes
        .iter()
        .map(|node| {
            let position = node.position.as_ref().expect("materialized position");
            (
                node.id.as_str(),
                (
                    position.x.parse::<i64>().expect("x"),
                    position.y.parse::<i64>().expect("y"),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for edge in &geometric.graph.edges {
        let from = positions[edge.from.as_str()];
        let to = positions[edge.to.as_str()];
        let delta_x = from.0 - to.0;
        let delta_y = from.1 - to.1;
        assert!(delta_x * delta_x + delta_y * delta_y <= radius * radius);
        assert!(edge.from < edge.to);
    }
    assert_eq!(
        geometric.provenance.source_id,
        "gilbert-random-plane-networks-1961-derived"
    );

    let regular = generate_flow_graph(&spec(FlowGeneratorFamilyV1::RandomRegularDirected {
        nodes: 40,
        degree: 5,
    }))
    .expect("regular graph");
    let mut incoming = BTreeMap::<&str, usize>::new();
    let mut outgoing = BTreeMap::<&str, usize>::new();
    let mut endpoints = BTreeSet::new();
    for edge in &regular.graph.edges {
        *outgoing.entry(edge.from.as_str()).or_default() += 1;
        *incoming.entry(edge.to.as_str()).or_default() += 1;
        assert_ne!(edge.from, edge.to);
        assert!(endpoints.insert((edge.from.as_str(), edge.to.as_str())));
    }
    assert!(regular.graph.nodes.iter().all(|node| {
        incoming.get(node.id.as_str()) == Some(&5) && outgoing.get(node.id.as_str()) == Some(&5)
    }));

    let preferential = generate_flow_graph(&spec(
        FlowGeneratorFamilyV1::PreferentialAttachmentDirected {
            nodes: 30,
            attachment_count: 3,
        },
    ))
    .expect("preferential attachment graph");
    assert_eq!(preferential.graph.edges.len(), 84);
    assert!(
        preferential
            .graph
            .edges
            .iter()
            .all(|edge| edge.from < edge.to)
    );
    assert_eq!(
        preferential.provenance.source_id,
        "barabasi-albert-preferential-attachment-1999-derived"
    );

    assert!(matches!(
        generate_flow_graph(&spec(FlowGeneratorFamilyV1::RandomGeometric {
            nodes: 449,
            radius: 1,
        })),
        Err(FlowGenerationError::SizeLimit)
    ));
}

#[test]
fn dag_small_world_and_clustered_families_preserve_their_sample_spaces() {
    let dag = generate_flow_graph(&spec(FlowGeneratorFamilyV1::RandomDag {
        nodes: 100,
        edge_count: 400,
    }))
    .expect("random DAG");
    assert_eq!(dag.graph.edges.len(), 400);
    assert!(dag.graph.edges.iter().all(|edge| edge.from < edge.to));
    assert_eq!(
        dag.graph
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<BTreeSet<_>>()
            .len(),
        400
    );
    assert_eq!(dag.provenance.difficulty, "ordinary");
    assert_eq!(dag.provenance.origin, "project-synthetic");
    assert_eq!(dag.provenance.sampling, "randomized");

    let small_world = generate_flow_graph(&spec(FlowGeneratorFamilyV1::WattsStrogatzFixed {
        nodes: 40,
        neighborhood: 6,
        rewire_count: 30,
    }))
    .expect("fixed-count small world");
    assert_eq!(small_world.graph.edges.len(), 120);
    let small_world_endpoints = small_world
        .graph
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect::<BTreeSet<_>>();
    assert_eq!(small_world_endpoints.len(), 120);
    assert!(small_world_endpoints.iter().all(|(from, to)| from != to));
    for index in 0..40 {
        assert_eq!(
            small_world
                .graph
                .edges
                .iter()
                .filter(|edge| edge.from == format!("v{index:04}"))
                .count(),
            3
        );
    }
    assert_eq!(
        small_world.provenance.source_id,
        "watts-strogatz-small-world-1998-fixed-count-derived"
    );

    let clustered = generate_flow_graph(&spec(FlowGeneratorFamilyV1::ClusteredDirected {
        clusters: 5,
        cluster_size: 6,
        bridge_edges: 25,
    }))
    .expect("clustered graph");
    assert_eq!(clustered.graph.nodes.len(), 30);
    assert_eq!(clustered.graph.edges.len(), 55);
    assert_eq!(
        clustered
            .graph
            .edges
            .iter()
            .filter(|edge| edge.from[0..4] != edge.to[0..4])
            .count(),
        25
    );
    assert_eq!(clustered.provenance.difficulty, "ordinary");
    assert_eq!(clustered.provenance.origin, "project-synthetic");
    assert_eq!(clustered.provenance.sampling, "randomized");
}

#[test]
fn planted_bottleneck_and_hall_tight_families_have_exact_certified_flow() {
    let bottleneck = generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::PlantedBottleneck {
        left: 7,
        right: 8,
        cut_edges: 13,
    }))
    .expect("planted bottleneck");
    assert_eq!(bottleneck.graph.nodes.len(), 17);
    assert_eq!(bottleneck.graph.edges.len(), 28);
    assert_eq!(reference_dinic(&bottleneck.graph).0, 13);
    assert_eq!(bottleneck.provenance.difficulty, "stress");
    assert_eq!(
        bottleneck
            .graph
            .edges
            .iter()
            .filter(|edge| edge.from.starts_with('l') && edge.to.starts_with('r'))
            .count(),
        13
    );

    let hall = generate_flow_graph(&fixed_spec(FlowGeneratorFamilyV1::HallTightBipartite {
        part_size: 8,
        tight_prefix: 3,
    }))
    .expect("Hall-tight graph");
    assert_eq!(reference_dinic(&hall.graph).0, 8);
    let tight_neighbors = hall
        .graph
        .edges
        .iter()
        .filter(|edge| {
            edge.from.starts_with('l') && edge.from[1..].parse::<u32>().expect("left ordinal") < 3
        })
        .map(|edge| edge.to.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(tight_neighbors, BTreeSet::from(["r0000", "r0001", "r0002"]));
    assert!(hall.graph.edges.iter().all(|edge| edge.capacity == "1"));
    assert!(matches!(
        hall.suggested_model,
        FlowProblemModelV1::BipartiteMatching {
            ref left,
            ref right,
            flow_adapter: Some(ref adapter),
        } if left.len() == 8
            && right.len() == 8
            && adapter.source == "s"
            && adapter.sink == "t"
    ));

    assert!(matches!(
        generate_flow_graph(&spec(FlowGeneratorFamilyV1::PlantedBottleneck {
            left: 2,
            right: 2,
            cut_edges: 2,
        })),
        Err(FlowGenerationError::Invalid(
            "family attributes are fixed by the declared construction"
        ))
    ));
}

#[test]
fn planar_and_multi_terminal_shapes_preserve_exact_constructions() {
    for count in 3..=20 {
        for seed in 0..8 {
            let mut input = spec(FlowGeneratorFamilyV1::PlanarTriangulated { nodes: count });
            input.seed = seed.to_string();
            let planar = generate_flow_graph(&input).expect("planar fan triangulation");
            assert_eq!(planar.graph.edges.len(), (2 * count - 3) as usize);
            let endpoints = planar
                .graph
                .edges
                .iter()
                .map(|edge| (edge.from.as_str(), edge.to.as_str()))
                .collect::<BTreeSet<_>>();
            assert_eq!(endpoints.len(), planar.graph.edges.len());
            assert!(endpoints.iter().all(|(from, to)| from < to));
            let FlowProblemModelV1::PlanarMaxFlow {
                source,
                sink,
                embedding,
            } = &planar.suggested_model
            else {
                panic!("planar family must materialize its native embedding model");
            };
            let network = canonical_network(&planar.graph);
            let source_index = network
                .node_index(&NodeId::parse(source).expect("generated source id"))
                .expect("generated source");
            let sink_index = network
                .node_index(&NodeId::parse(sink).expect("generated sink id"))
                .expect("generated sink");
            let hassin = solve_hassin_st_planar(&network, source_index, sink_index, embedding)
                .unwrap_or_else(|error| panic!("count {count}: {error:?}"));
            let reference =
                solve_edmonds_karp(&network, source_index, sink_index).expect("reference max flow");
            assert_eq!(hassin.certificate.value, reference.certificate.value);
            let leftmost =
                solve_borradaile_klein_planar(&network, source_index, sink_index, embedding)
                    .unwrap_or_else(|error| panic!("count {count}: {error:?}"));
            assert_eq!(leftmost.certificate.value, reference.certificate.value);
            assert_eq!(leftmost.metrics.preprocessing_runs, 1);
            assert_eq!(leftmost.metrics.dual_faces, u64::from(count - 1));
            assert_eq!(hassin.metrics.dual_shortest_path_runs, 1);
            assert_eq!(
                hassin.metrics.dual_faces,
                u64::from(count),
                "a fan with n vertices has n-1 faces before the split",
            );
        }
    }

    let transformed = generate_flow_graph(&spec(FlowGeneratorFamilyV1::MultiSourceSink {
        sources: 5,
        intermediate: 7,
        sinks: 3,
    }))
    .expect("multi terminal transform");
    assert_eq!(transformed.graph.nodes.len(), 17);
    assert_eq!(transformed.graph.edges.len(), 64);
    assert_eq!(
        transformed
            .graph
            .edges
            .iter()
            .filter(|edge| edge.from == "s")
            .count(),
        5
    );
    assert_eq!(
        transformed
            .graph
            .edges
            .iter()
            .filter(|edge| edge.to == "t")
            .count(),
        3
    );
    assert_eq!(
        transformed
            .graph
            .edges
            .iter()
            .filter(|edge| edge.from.starts_with('u') && edge.to.starts_with('v'))
            .count(),
        35
    );
    assert_eq!(
        transformed
            .graph
            .edges
            .iter()
            .filter(|edge| edge.from.starts_with('v') && edge.to.starts_with('w'))
            .count(),
        21
    );
}

fn goldberg_mesh_default_family() -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::GoldbergMeshCirculation {
        columns: 4,
        rows: 3,
        horizontal_degree: 1,
        vertical_degree: 1,
    }
}

#[test]
fn goldberg_mesh_circulation_materializes_signed_links_and_golden_digest() {
    let generated = generate_flow_graph(&fixed_spec(goldberg_mesh_default_family()))
        .expect("Goldberg mesh circulation");
    assert_eq!(
        (generated.graph.nodes.len(), generated.graph.edges.len()),
        (12, 48)
    );
    assert!(matches!(
        generated.suggested_model,
        FlowProblemModelV1::Circulation {}
    ));
    assert!(generated.graph.nodes.iter().all(|node| node.supply == "0"));
    assert_eq!(generated.provenance.difficulty, "ordinary");
    assert_eq!(generated.provenance.origin, "official-benchmark-derived");
    assert_eq!(generated.provenance.sampling, "randomized");
    assert_eq!(
        generated.provenance.source_id,
        "goldberg-mesh1-1991-project-rng-signed-bound-derived"
    );
    assert_eq!(
        generated.provenance.tags,
        [
            "bidirectional",
            "circulation",
            "distance-decay",
            "goldberg-mesh-circulation",
            "grid",
            "signed-cost",
            "toroidal",
        ]
    );
    assert_eq!(
        generated.provenance.materialized_sha256,
        "f4357b6b01219ead84861ecb193e455167a4dbafbf3c448b74a82a064cf8e02c"
    );

    let mut endpoint_set = BTreeSet::new();
    for pair in generated.graph.edges.chunks_exact(2) {
        let forward = &pair[0];
        let reverse = &pair[1];
        assert_eq!(
            (forward.from.as_str(), forward.to.as_str()),
            (reverse.to.as_str(), reverse.from.as_str())
        );
        assert_eq!(
            forward.cost.parse::<i64>().expect("forward cost"),
            -reverse.cost.parse::<i64>().expect("reverse cost")
        );
        assert!((1..=1_000).contains(&forward.capacity.parse::<u64>().expect("capacity")));
        assert!((1..=1_000).contains(&reverse.capacity.parse::<u64>().expect("capacity")));
        assert!(endpoint_set.insert((forward.from.as_str(), forward.to.as_str())));
        assert!(endpoint_set.insert((reverse.from.as_str(), reverse.to.as_str())));
    }
    assert_eq!(endpoint_set.len(), generated.graph.edges.len());

    let first = generated.graph.nodes.first().expect("first grid node");
    let last = generated.graph.nodes.last().expect("last grid node");
    assert_eq!(first.id, "m0000c0000");
    assert_eq!(last.id, "m0002c0003");
    assert_eq!(
        (
            first.position.as_ref().expect("position").x.as_str(),
            first.position.as_ref().expect("position").y.as_str(),
            last.position.as_ref().expect("position").x.as_str(),
            last.position.as_ref().expect("position").y.as_str(),
        ),
        ("72", "58", "828", "482")
    );

    let network = canonical_network(&generated.graph);
    let required = vec![0; network.nodes().len()];
    let optimized = solve_cost_scaling(&network, &required).expect("finite min-cost circulation");
    assert!(optimized.certificate.total_cost < 0);

    let repeated =
        generate_flow_graph(&fixed_spec(goldberg_mesh_default_family())).expect("same seed");
    assert_eq!(
        serde_json::to_vec(&generated).expect("serialize"),
        serde_json::to_vec(&repeated).expect("serialize")
    );
}

#[test]
fn goldberg_mesh_distance_decay_and_rng_streams_are_explicit() {
    let mut sampled_rng = RngV1::from_seed(7, CAPACITY_RNG_DOMAIN);
    let mut expected_rng = sampled_rng;
    for distance in 1..=8 {
        let expected_raw = sample_uniform_u64(&mut expected_rng, 1, GOLDBERG_MESH_MAXIMUM_CAPACITY)
            .expect("raw capacity draw");
        assert_eq!(
            goldberg_mesh_distance_capacity(&mut sampled_rng, distance)
                .expect("bounded distance capacity"),
            expected_raw / (1_u64 << (distance - 1)),
            "every supported distance uses exact floor division",
        );
    }
    let mut far_rng = RngV1::from_seed(11, CAPACITY_RNG_DOMAIN);
    let far_capacities = (0..128)
        .map(|_| goldberg_mesh_distance_capacity(&mut far_rng, 8).expect("distance-eight capacity"))
        .collect::<Vec<_>>();
    assert!(
        far_capacities.contains(&0),
        "source construction deliberately permits a zero-capacity distant arc",
    );

    let mut endpoint_capacity_rng = RngV1::from_seed(19, CAPACITY_RNG_DOMAIN);
    let mut endpoint_cost_rng = RngV1::from_seed(19, COST_RNG_DOMAIN);
    let endpoint_fixture = goldberg_mesh_circulation_topology(
        7,
        9,
        3,
        4,
        &mut endpoint_capacity_rng,
        &mut endpoint_cost_rng,
    )
    .expect("nondefault horizontal and vertical degrees");
    let node_id = |row: u32, column: u32| format!("m{row:04}c{column:04}");
    let mut expected_endpoints = Vec::new();
    for row in 0..9 {
        for column in 0..7 {
            let from = node_id(row, column);
            for distance in 1..=3 {
                let to = node_id(row, (column + distance) % 7);
                expected_endpoints.push((from.clone(), to.clone()));
                expected_endpoints.push((to, from.clone()));
            }
            for distance in 1..=4 {
                let to = node_id((row + distance) % 9, column);
                expected_endpoints.push((from.clone(), to.clone()));
                expected_endpoints.push((to, from.clone()));
            }
        }
    }
    assert_eq!(endpoint_fixture.edges, expected_endpoints);

    let mut capacity_rng = RngV1::from_seed(42, CAPACITY_RNG_DOMAIN);
    let mut cost_rng = RngV1::from_seed(42, COST_RNG_DOMAIN);
    let base = goldberg_mesh_circulation_topology(7, 7, 3, 0, &mut capacity_rng, &mut cost_rng)
        .expect("three-distance ring rows");
    let capacities = base.fixed_capacities.as_ref().expect("fixed capacities");
    let costs = base.fixed_costs.as_ref().expect("fixed costs");
    assert_eq!((base.nodes.len(), base.edges.len()), (49, 294));
    for (logical_index, pair) in capacities.chunks_exact(2).enumerate() {
        let distance = u32::try_from(logical_index % 3 + 1).expect("distance");
        let maximum = 1_000_u64 / (1_u64 << (distance - 1));
        assert!(pair.iter().all(|capacity| *capacity <= maximum));
        let cost_pair = &costs[logical_index * 2..logical_index * 2 + 2];
        assert_eq!(cost_pair[0], -cost_pair[1]);
    }

    let mut shifted_capacity_rng = RngV1::from_seed(42, CAPACITY_RNG_DOMAIN);
    shifted_capacity_rng.next_u64();
    let mut same_cost_rng = RngV1::from_seed(42, COST_RNG_DOMAIN);
    let shifted = goldberg_mesh_circulation_topology(
        7,
        7,
        3,
        0,
        &mut shifted_capacity_rng,
        &mut same_cost_rng,
    )
    .expect("shifted capacity stream");
    assert_eq!(base.edges, shifted.edges);
    assert_eq!(base.fixed_costs, shifted.fixed_costs);
    assert_ne!(base.fixed_capacities, shifted.fixed_capacities);

    let mut same_capacity_rng = RngV1::from_seed(42, CAPACITY_RNG_DOMAIN);
    let mut shifted_cost_rng = RngV1::from_seed(42, COST_RNG_DOMAIN);
    shifted_cost_rng.next_u64();
    let cost_shifted = goldberg_mesh_circulation_topology(
        7,
        7,
        3,
        0,
        &mut same_capacity_rng,
        &mut shifted_cost_rng,
    )
    .expect("shifted cost stream");
    assert_eq!(base.edges, cost_shifted.edges);
    assert_eq!(base.fixed_capacities, cost_shifted.fixed_capacities);
    assert_ne!(base.fixed_costs, cost_shifted.fixed_costs);
}

#[test]
fn goldberg_mesh_circulation_enforces_practical_and_unique_link_bounds() {
    let exact = generate_flow_graph(&fixed_spec(
        FlowGeneratorFamilyV1::GoldbergMeshCirculation {
            columns: 32,
            rows: 32,
            horizontal_degree: 8,
            vertical_degree: 8,
        },
    ))
    .expect("practical maximum");
    assert_eq!(
        (exact.graph.nodes.len(), exact.graph.edges.len()),
        (1_024, 32_768)
    );

    for invalid in [
        FlowGeneratorFamilyV1::GoldbergMeshCirculation {
            columns: 2,
            rows: 4,
            horizontal_degree: 0,
            vertical_degree: 1,
        },
        FlowGeneratorFamilyV1::GoldbergMeshCirculation {
            columns: 33,
            rows: 4,
            horizontal_degree: 1,
            vertical_degree: 1,
        },
        FlowGeneratorFamilyV1::GoldbergMeshCirculation {
            columns: 16,
            rows: 4,
            horizontal_degree: 8,
            vertical_degree: 1,
        },
        FlowGeneratorFamilyV1::GoldbergMeshCirculation {
            columns: 4,
            rows: 4,
            horizontal_degree: 0,
            vertical_degree: 0,
        },
    ] {
        assert!(generate_flow_graph(&fixed_spec(invalid)).is_err());
    }
    assert!(matches!(
        generate_flow_graph(&spec(goldberg_mesh_default_family())),
        Err(FlowGenerationError::Invalid(
            "family attributes are fixed by the declared construction"
        ))
    ));
}
