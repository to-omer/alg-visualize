//! Bounded deterministic realization of the paper's static `Sparsify` routine.
//!
//! The production dynamic spanner is asymptotic and delegates expander
//! decomposition, deterministic expander construction, and decremental
//! expander paths to sophisticated data structures.  On the repository's
//! eight-vertex admission band we retain the same three source tasks while
//! replacing only those delegated subroutines by exhaustive, exact ones:
//!
//! 1. certify an expander decomposition and construct a degree-bounded witness;
//! 2. embed every witness edge into the input graph by a canonical shortest path;
//! 3. embed every input edge into the witness and compose both embeddings.
//!
//! No almost-linear runtime or asymptotic expander constant is claimed here.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Zero};
use thiserror::Error;

/// Maximum synthetic witness edges in one exact small-graph bucket.
pub const DETERMINISTIC_SPANNER_MAX_WITNESS_EDGES: usize = 2_048;
/// Maximum total arcs stored across the two source embeddings.
pub const DETERMINISTIC_SPANNER_MAX_EMBEDDING_ARCS: usize = 32_768;

/// One signed stable edge in an exact path.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DeterministicSpannerArc {
    /// Stable input-graph or local witness edge ID.
    pub edge: usize,
    /// `1` follows the stored orientation and `-1` opposes it.
    pub direction: i8,
}

/// One synthetic unweighted witness edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeterministicSpannerWitnessEdge {
    /// Dense local witness edge ID.
    pub edge: usize,
    /// Canonical stored tail.
    pub from: usize,
    /// Canonical stored head.
    pub to: usize,
}

/// Exact target interval used by bounded `ConstructExpander`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeterministicSpannerDegreeTarget {
    /// Stable graph vertex.
    pub vertex: usize,
    /// Required inclusive witness degree.
    pub lower: usize,
    /// Permitted inclusive witness degree.
    pub upper: usize,
}

/// Source-task certificate for one factor-two length bucket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeterministicSpannerSparsifyCertificate {
    /// Stable input edge slots in this bucket.
    pub graph_edges: Vec<usize>,
    /// Source `Decompose` level containing all nonempty components.
    pub decomposition_level: usize,
    /// Nontrivial connected components in stable vertex order.
    pub decomposition_components: Vec<Vec<usize>>,
    /// Exact minimum conductance of the certified loopy components.
    pub phi: BigRational,
    /// Exact minimum conductance of the constructed witness components.
    pub witness_phi: BigRational,
    /// Per-vertex degree intervals passed to bounded `ConstructExpander`.
    pub degree_targets: Vec<DeterministicSpannerDegreeTarget>,
    /// Deterministically constructed witness multigraph.
    pub witness_edges: Vec<DeterministicSpannerWitnessEdge>,
    /// Task 2 embedding `Pi_(W -> J)`, indexed by witness edge ID.
    pub witness_to_graph: Vec<Vec<DeterministicSpannerArc>>,
    /// Task 3 embedding `Pi_(J -> W)`, indexed by stable input edge slot.
    pub graph_to_witness: Vec<Vec<DeterministicSpannerArc>>,
    /// Image of `Pi_(W -> J)`, hence the returned subgraph.
    pub selected_edges: Vec<usize>,
    /// Composed and loop-erased `Pi_(J -> tilde J)` by stable input slot.
    pub graph_to_sparsifier: Vec<Vec<DeterministicSpannerArc>>,
    /// Exact Task 2 congestion threshold used by the bounded realization.
    pub tau_vertex: BigRational,
    /// Exact Task 3 congestion threshold used by the bounded realization.
    pub tau_edge: BigRational,
    /// Exact maximum weighted vertex congestion after Task 2.
    pub task_two_vertex_congestion: BigRational,
    /// Exact maximum edge congestion after Task 3.
    pub task_three_edge_congestion: usize,
    /// Completed Task 2 outer iterations.
    pub task_two_rounds: usize,
    /// Completed Task 3 outer iterations.
    pub task_three_rounds: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DeterministicSpannerInputEdge {
    pub(crate) edge: usize,
    pub(crate) from: usize,
    pub(crate) to: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DeterministicSpannerSparsifyResult {
    pub(crate) selected_edges: Vec<usize>,
    pub(crate) embedding: Vec<Vec<DeterministicSpannerArc>>,
    pub(crate) maximum_congestion: usize,
    pub(crate) maximum_path_length: usize,
    pub(crate) certificate: DeterministicSpannerSparsifyCertificate,
}

/// Explicit bounded failure. The caller maps it to its public primitive error.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(crate) enum DeterministicSpannerSparsifyError {
    #[error("deterministic spanner input is invalid")]
    InvalidInput,
    #[error("deterministic spanner exceeds its bounded exact admission")]
    AdmissionLimit,
    #[error("deterministic spanner source invariant failed")]
    InvariantViolation,
    #[error("deterministic spanner checked arithmetic overflow")]
    ArithmeticOverflow,
}

struct DecompositionData {
    maximum_degree: usize,
    level: usize,
    loop_count: usize,
    components: Vec<Vec<usize>>,
    phi: BigRational,
}

struct SourceTaskData {
    witness_edges: Vec<DeterministicSpannerWitnessEdge>,
    degree_targets: Vec<DeterministicSpannerDegreeTarget>,
    witness_phi: BigRational,
    witness_to_graph: Vec<Vec<DeterministicSpannerArc>>,
    graph_to_witness: Vec<Vec<DeterministicSpannerArc>>,
    selected_edges: Vec<usize>,
    embedding: Vec<Vec<DeterministicSpannerArc>>,
    maximum_congestion: usize,
    maximum_path_length: usize,
}

struct TaskMetrics {
    tau_vertex: BigRational,
    tau_edge: BigRational,
    task_two_vertex_congestion: BigRational,
    task_three_edge_congestion: usize,
}

/// Runs all three source tasks on one unweighted factor-two length bucket.
pub(crate) fn bounded_deterministic_sparsify(
    vertex_count: usize,
    edge_slot_count: usize,
    input_edges: &[DeterministicSpannerInputEdge],
) -> Result<DeterministicSpannerSparsifyResult, DeterministicSpannerSparsifyError> {
    validate_input(vertex_count, edge_slot_count, input_edges)?;
    if input_edges.is_empty() {
        return Ok(empty_result(edge_slot_count));
    }

    let graph = GraphView::new(vertex_count, input_edges);
    let decomposition = decompose_graph(&graph)?;
    let tasks = construct_source_tasks(
        vertex_count,
        &graph,
        edge_slot_count,
        input_edges,
        &decomposition,
    )?;
    let metrics = compute_task_metrics(vertex_count, &graph, &decomposition, &tasks)?;
    check_task_metric_bounds(&metrics)?;
    let arc_count = tasks
        .witness_to_graph
        .iter()
        .chain(tasks.graph_to_witness.iter())
        .map(Vec::len)
        .try_fold(0_usize, usize::checked_add)
        .ok_or(DeterministicSpannerSparsifyError::ArithmeticOverflow)?;
    if arc_count > DETERMINISTIC_SPANNER_MAX_EMBEDDING_ARCS {
        return Err(DeterministicSpannerSparsifyError::AdmissionLimit);
    }
    let certificate = DeterministicSpannerSparsifyCertificate {
        graph_edges: input_edges.iter().map(|edge| edge.edge).collect(),
        decomposition_level: decomposition.level,
        decomposition_components: decomposition.components,
        phi: decomposition.phi,
        witness_phi: tasks.witness_phi,
        degree_targets: tasks.degree_targets,
        witness_edges: tasks.witness_edges,
        witness_to_graph: tasks.witness_to_graph,
        graph_to_witness: tasks.graph_to_witness,
        selected_edges: tasks.selected_edges.clone(),
        graph_to_sparsifier: tasks.embedding.clone(),
        tau_vertex: metrics.tau_vertex,
        tau_edge: metrics.tau_edge,
        task_two_vertex_congestion: metrics.task_two_vertex_congestion,
        task_three_edge_congestion: metrics.task_three_edge_congestion,
        task_two_rounds: 1,
        task_three_rounds: 1,
    };
    Ok(DeterministicSpannerSparsifyResult {
        selected_edges: tasks.selected_edges,
        embedding: tasks.embedding,
        maximum_congestion: tasks.maximum_congestion,
        maximum_path_length: tasks.maximum_path_length,
        certificate,
    })
}

/// Independently checks every public invariant in the bounded source certificate.
pub(crate) fn check_bounded_deterministic_sparsify_certificate(
    vertex_count: usize,
    edge_slot_count: usize,
    input_edges: &[DeterministicSpannerInputEdge],
    certificate: &DeterministicSpannerSparsifyCertificate,
) -> Result<DeterministicSpannerSparsifyResult, DeterministicSpannerSparsifyError> {
    validate_input(vertex_count, edge_slot_count, input_edges)?;
    if input_edges.is_empty() {
        let expected = empty_result(edge_slot_count);
        if &expected.certificate != certificate {
            return Err(DeterministicSpannerSparsifyError::InvariantViolation);
        }
        return Ok(expected);
    }

    let graph = GraphView::new(vertex_count, input_edges);
    let decomposition = decompose_graph(&graph)?;
    if certificate.graph_edges != input_edges.iter().map(|edge| edge.edge).collect::<Vec<_>>()
        || certificate.decomposition_level != decomposition.level
        || certificate.decomposition_components != decomposition.components
        || certificate.phi != decomposition.phi
        || certificate.task_two_rounds != 1
        || certificate.task_three_rounds != 1
    {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }

    check_degree_targets(
        &graph,
        &decomposition.components,
        decomposition.level,
        &decomposition.phi,
        &certificate.degree_targets,
        &certificate.witness_edges,
    )?;
    let witness_phi = minimum_witness_conductance(
        vertex_count,
        &certificate.witness_edges,
        &decomposition.components,
    )?;
    if certificate.witness_phi != witness_phi {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }
    let witness_graph = WitnessGraphView::new(vertex_count, &certificate.witness_edges);
    check_canonical_paths(
        &certificate.witness_edges,
        &certificate.witness_to_graph,
        |from, to| graph.shortest_path(from, to),
    )?;
    check_input_paths(
        input_edges,
        edge_slot_count,
        &certificate.graph_to_witness,
        |from, to| witness_graph.shortest_path(from, to),
    )?;

    let selected_edges = certificate
        .witness_to_graph
        .iter()
        .flatten()
        .map(|arc| arc.edge)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if certificate.selected_edges != selected_edges
        || certificate.graph_to_sparsifier.len() != edge_slot_count
    {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }
    let embedding = compose_all_embeddings(
        edge_slot_count,
        input_edges,
        &certificate.graph_to_witness,
        &certificate.witness_edges,
        &certificate.witness_to_graph,
        &graph,
    )?;
    if certificate.graph_to_sparsifier != embedding {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }
    let maximum_path_length = embedding.iter().map(Vec::len).max().unwrap_or(0);
    let final_maximum_congestion = maximum_congestion(edge_slot_count, &embedding)?;
    let task_data = SourceTaskData {
        witness_edges: certificate.witness_edges.clone(),
        degree_targets: Vec::new(),
        witness_phi,
        witness_to_graph: certificate.witness_to_graph.clone(),
        graph_to_witness: certificate.graph_to_witness.clone(),
        selected_edges: selected_edges.clone(),
        embedding: embedding.clone(),
        maximum_congestion: final_maximum_congestion,
        maximum_path_length,
    };
    let metrics = compute_task_metrics(vertex_count, &graph, &decomposition, &task_data)?;
    if certificate.tau_vertex != metrics.tau_vertex
        || certificate.tau_edge != metrics.tau_edge
        || certificate.task_two_vertex_congestion != metrics.task_two_vertex_congestion
        || certificate.task_three_edge_congestion != metrics.task_three_edge_congestion
    {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }
    check_task_metric_bounds(&metrics)?;
    Ok(DeterministicSpannerSparsifyResult {
        selected_edges,
        embedding,
        maximum_congestion: final_maximum_congestion,
        maximum_path_length,
        certificate: certificate.clone(),
    })
}

fn decompose_graph(
    graph: &GraphView,
) -> Result<DecompositionData, DeterministicSpannerSparsifyError> {
    let maximum_degree = graph.maximum_degree();
    let level = ceil_log2(maximum_degree.max(1))
        .checked_add(1)
        .ok_or(DeterministicSpannerSparsifyError::ArithmeticOverflow)?;
    let loop_count = 1_usize
        .checked_shl(
            u32::try_from(level).map_err(|_| DeterministicSpannerSparsifyError::AdmissionLimit)?,
        )
        .ok_or(DeterministicSpannerSparsifyError::AdmissionLimit)?;
    let components = graph.nontrivial_components();
    let phi = minimum_loopy_conductance(graph, &components, loop_count)?;
    if phi <= BigRational::zero() {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }
    Ok(DecompositionData {
        maximum_degree,
        level,
        loop_count,
        components,
        phi,
    })
}

fn construct_source_tasks(
    vertex_count: usize,
    graph: &GraphView,
    edge_slot_count: usize,
    input_edges: &[DeterministicSpannerInputEdge],
    decomposition: &DecompositionData,
) -> Result<SourceTaskData, DeterministicSpannerSparsifyError> {
    let (witness_edges, degree_targets) = construct_witness(
        graph,
        &decomposition.components,
        decomposition.level,
        &decomposition.phi,
    )?;
    let witness_graph = WitnessGraphView::new(vertex_count, &witness_edges);
    let witness_phi =
        minimum_witness_conductance(vertex_count, &witness_edges, &decomposition.components)?;
    let witness_to_graph = witness_edges
        .iter()
        .map(|edge| graph.shortest_path(edge.from, edge.to))
        .collect::<Result<Vec<_>, _>>()?;
    let mut graph_to_witness = vec![Vec::new(); edge_slot_count];
    for edge in input_edges {
        graph_to_witness[edge.edge] = witness_graph.shortest_path(edge.from, edge.to)?;
    }
    let selected_edges = image_edges(&witness_to_graph);
    let embedding = compose_all_embeddings(
        edge_slot_count,
        input_edges,
        &graph_to_witness,
        &witness_edges,
        &witness_to_graph,
        graph,
    )?;
    let maximum_path_length = embedding.iter().map(Vec::len).max().unwrap_or(0);
    let maximum_congestion = maximum_congestion(edge_slot_count, &embedding)?;
    Ok(SourceTaskData {
        witness_edges,
        degree_targets,
        witness_phi,
        witness_to_graph,
        graph_to_witness,
        selected_edges,
        embedding,
        maximum_congestion,
        maximum_path_length,
    })
}

fn image_edges(paths: &[Vec<DeterministicSpannerArc>]) -> Vec<usize> {
    paths
        .iter()
        .flatten()
        .map(|arc| arc.edge)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn compose_all_embeddings(
    edge_slot_count: usize,
    input_edges: &[DeterministicSpannerInputEdge],
    graph_to_witness: &[Vec<DeterministicSpannerArc>],
    witness_edges: &[DeterministicSpannerWitnessEdge],
    witness_to_graph: &[Vec<DeterministicSpannerArc>],
    graph: &GraphView,
) -> Result<Vec<Vec<DeterministicSpannerArc>>, DeterministicSpannerSparsifyError> {
    let selected = image_edges(witness_to_graph)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut embedding = vec![Vec::new(); edge_slot_count];
    for edge in input_edges {
        let witness_path = graph_to_witness
            .get(edge.edge)
            .ok_or(DeterministicSpannerSparsifyError::InvariantViolation)?;
        let composed = compose_embedding(
            edge.from,
            edge.to,
            witness_path,
            witness_edges,
            witness_to_graph,
            graph,
        )?;
        if composed.iter().any(|arc| !selected.contains(&arc.edge)) {
            return Err(DeterministicSpannerSparsifyError::InvariantViolation);
        }
        embedding[edge.edge] = composed;
    }
    Ok(embedding)
}

fn compute_task_metrics(
    vertex_count: usize,
    graph: &GraphView,
    decomposition: &DecompositionData,
    tasks: &SourceTaskData,
) -> Result<TaskMetrics, DeterministicSpannerSparsifyError> {
    let gamma_path = tasks.maximum_path_length.max(1);
    let phi_inverse = BigRational::one() / &decomposition.phi;
    let tau_vertex = BigRational::from_integer(BigInt::from(
        4_usize
            .checked_mul(gamma_path)
            .and_then(|value| value.checked_mul(decomposition.maximum_degree.max(1)))
            .ok_or(DeterministicSpannerSparsifyError::ArithmeticOverflow)?,
    )) * &phi_inverse;
    let tau_edge = BigRational::from_integer(BigInt::from(gamma_path)) * phi_inverse;
    let witness_weight =
        &decomposition.phi * BigRational::from_integer(BigInt::from(decomposition.loop_count));
    let task_two_vertex_congestion = maximum_weighted_vertex_congestion(
        vertex_count,
        &tasks.witness_edges,
        &tasks.witness_to_graph,
        graph,
        &witness_weight,
    )?;
    let task_three_edge_congestion =
        maximum_congestion(tasks.witness_edges.len(), &tasks.graph_to_witness)?;
    Ok(TaskMetrics {
        tau_vertex,
        tau_edge,
        task_two_vertex_congestion,
        task_three_edge_congestion,
    })
}

fn check_task_metric_bounds(
    metrics: &TaskMetrics,
) -> Result<(), DeterministicSpannerSparsifyError> {
    let three_halves = BigRational::new(BigInt::from(3_u8), BigInt::from(2_u8));
    if metrics.task_two_vertex_congestion > metrics.tau_vertex.clone() * &three_halves
        || BigRational::from_integer(BigInt::from(metrics.task_three_edge_congestion))
            > metrics.tau_edge.clone() * three_halves
    {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }
    Ok(())
}

fn empty_result(edge_slot_count: usize) -> DeterministicSpannerSparsifyResult {
    let certificate = DeterministicSpannerSparsifyCertificate {
        graph_edges: Vec::new(),
        decomposition_level: 0,
        decomposition_components: Vec::new(),
        phi: BigRational::one(),
        witness_phi: BigRational::one(),
        degree_targets: Vec::new(),
        witness_edges: Vec::new(),
        witness_to_graph: Vec::new(),
        graph_to_witness: vec![Vec::new(); edge_slot_count],
        selected_edges: Vec::new(),
        graph_to_sparsifier: vec![Vec::new(); edge_slot_count],
        tau_vertex: BigRational::one(),
        tau_edge: BigRational::one(),
        task_two_vertex_congestion: BigRational::zero(),
        task_three_edge_congestion: 0,
        task_two_rounds: 0,
        task_three_rounds: 0,
    };
    DeterministicSpannerSparsifyResult {
        selected_edges: Vec::new(),
        embedding: vec![Vec::new(); edge_slot_count],
        maximum_congestion: 0,
        maximum_path_length: 0,
        certificate,
    }
}

fn validate_input(
    vertex_count: usize,
    edge_slot_count: usize,
    input_edges: &[DeterministicSpannerInputEdge],
) -> Result<(), DeterministicSpannerSparsifyError> {
    let mut previous = None;
    for edge in input_edges {
        if edge.edge >= edge_slot_count
            || edge.from >= vertex_count
            || edge.to >= vertex_count
            || edge.from == edge.to
            || previous.is_some_and(|value| value >= edge.edge)
        {
            return Err(DeterministicSpannerSparsifyError::InvalidInput);
        }
        previous = Some(edge.edge);
    }
    Ok(())
}

fn check_degree_targets(
    graph: &GraphView,
    components: &[Vec<usize>],
    decomposition_level: usize,
    phi: &BigRational,
    targets: &[DeterministicSpannerDegreeTarget],
    witness_edges: &[DeterministicSpannerWitnessEdge],
) -> Result<(), DeterministicSpannerSparsifyError> {
    if witness_edges.len() > DETERMINISTIC_SPANNER_MAX_WITNESS_EDGES {
        return Err(DeterministicSpannerSparsifyError::AdmissionLimit);
    }
    let scale = BigRational::from_integer(BigInt::from(
        1_usize
            .checked_shl(
                u32::try_from(decomposition_level)
                    .map_err(|_| DeterministicSpannerSparsifyError::AdmissionLimit)?,
            )
            .ok_or(DeterministicSpannerSparsifyError::AdmissionLimit)?,
    ));
    let mut expected_targets = Vec::new();
    for component in components {
        for &vertex in component {
            let target =
                BigRational::from_integer(BigInt::from(graph.degrees[vertex])) / (phi * &scale);
            let lower = ceil_rational(&target)?.max(1);
            let upper = floor_rational(&(target * BigInt::from(18_u8)))?.max(lower);
            expected_targets.push(DeterministicSpannerDegreeTarget {
                vertex,
                lower,
                upper,
            });
        }
    }
    expected_targets.sort_by_key(|target| target.vertex);
    if targets != expected_targets.as_slice() {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }

    let mut component_of = vec![None; graph.vertex_count];
    for (component, vertices) in components.iter().enumerate() {
        for &vertex in vertices {
            component_of[vertex] = Some(component);
        }
    }
    let mut degrees = vec![0_usize; graph.vertex_count];
    for (expected_id, edge) in witness_edges.iter().enumerate() {
        if edge.edge != expected_id
            || edge.from >= graph.vertex_count
            || edge.to >= graph.vertex_count
            || edge.from >= edge.to
            || component_of[edge.from].is_none()
            || component_of[edge.from] != component_of[edge.to]
        {
            return Err(DeterministicSpannerSparsifyError::InvariantViolation);
        }
        degrees[edge.from] = degrees[edge.from]
            .checked_add(1)
            .ok_or(DeterministicSpannerSparsifyError::ArithmeticOverflow)?;
        degrees[edge.to] = degrees[edge.to]
            .checked_add(1)
            .ok_or(DeterministicSpannerSparsifyError::ArithmeticOverflow)?;
    }
    for target in targets {
        if degrees[target.vertex] < target.lower || degrees[target.vertex] > target.upper {
            return Err(DeterministicSpannerSparsifyError::InvariantViolation);
        }
    }
    Ok(())
}

fn check_canonical_paths<F>(
    witness_edges: &[DeterministicSpannerWitnessEdge],
    paths: &[Vec<DeterministicSpannerArc>],
    mut expected_path: F,
) -> Result<(), DeterministicSpannerSparsifyError>
where
    F: FnMut(
        usize,
        usize,
    ) -> Result<Vec<DeterministicSpannerArc>, DeterministicSpannerSparsifyError>,
{
    if paths.len() != witness_edges.len() {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }
    for edge in witness_edges {
        if paths[edge.edge] != expected_path(edge.from, edge.to)? {
            return Err(DeterministicSpannerSparsifyError::InvariantViolation);
        }
    }
    Ok(())
}

fn check_input_paths<F>(
    input_edges: &[DeterministicSpannerInputEdge],
    edge_slot_count: usize,
    paths: &[Vec<DeterministicSpannerArc>],
    mut expected_path: F,
) -> Result<(), DeterministicSpannerSparsifyError>
where
    F: FnMut(
        usize,
        usize,
    ) -> Result<Vec<DeterministicSpannerArc>, DeterministicSpannerSparsifyError>,
{
    if paths.len() != edge_slot_count {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }
    let active = input_edges
        .iter()
        .map(|edge| edge.edge)
        .collect::<BTreeSet<_>>();
    if paths
        .iter()
        .enumerate()
        .any(|(edge, path)| !active.contains(&edge) && !path.is_empty())
    {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }
    for edge in input_edges {
        if paths[edge.edge] != expected_path(edge.from, edge.to)? {
            return Err(DeterministicSpannerSparsifyError::InvariantViolation);
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct StoredEdge {
    edge: usize,
    from: usize,
    to: usize,
}

struct GraphView {
    vertex_count: usize,
    edges: Vec<StoredEdge>,
    degrees: Vec<usize>,
}

impl GraphView {
    fn new(vertex_count: usize, input: &[DeterministicSpannerInputEdge]) -> Self {
        let edges = input
            .iter()
            .map(|edge| StoredEdge {
                edge: edge.edge,
                from: edge.from,
                to: edge.to,
            })
            .collect::<Vec<_>>();
        let mut degrees = vec![0_usize; vertex_count];
        for edge in &edges {
            degrees[edge.from] += 1;
            degrees[edge.to] += 1;
        }
        Self {
            vertex_count,
            edges,
            degrees,
        }
    }

    fn maximum_degree(&self) -> usize {
        self.degrees.iter().copied().max().unwrap_or(0)
    }

    fn nontrivial_components(&self) -> Vec<Vec<usize>> {
        let mut seen = vec![false; self.vertex_count];
        let mut result = Vec::new();
        for start in 0..self.vertex_count {
            if seen[start] || self.degrees[start] == 0 {
                continue;
            }
            let mut queue = VecDeque::from([start]);
            let mut component = Vec::new();
            seen[start] = true;
            while let Some(vertex) = queue.pop_front() {
                component.push(vertex);
                for edge in &self.edges {
                    let next = if edge.from == vertex {
                        Some(edge.to)
                    } else if edge.to == vertex {
                        Some(edge.from)
                    } else {
                        None
                    };
                    if let Some(next) = next
                        && !seen[next]
                    {
                        seen[next] = true;
                        queue.push_back(next);
                    }
                }
            }
            component.sort_unstable();
            if component.len() > 1 {
                result.push(component);
            }
        }
        result
    }

    fn shortest_path(
        &self,
        source: usize,
        sink: usize,
    ) -> Result<Vec<DeterministicSpannerArc>, DeterministicSpannerSparsifyError> {
        shortest_path(
            self.vertex_count,
            source,
            sink,
            self.edges.iter().map(|edge| StoredEdge {
                edge: edge.edge,
                from: edge.from,
                to: edge.to,
            }),
        )
    }

    fn row(&self, stable_edge: usize) -> Option<StoredEdge> {
        self.edges
            .iter()
            .find(|edge| edge.edge == stable_edge)
            .copied()
    }
}

struct WitnessGraphView<'a> {
    vertex_count: usize,
    edges: &'a [DeterministicSpannerWitnessEdge],
}

impl<'a> WitnessGraphView<'a> {
    fn new(vertex_count: usize, edges: &'a [DeterministicSpannerWitnessEdge]) -> Self {
        Self {
            vertex_count,
            edges,
        }
    }

    fn shortest_path(
        &self,
        source: usize,
        sink: usize,
    ) -> Result<Vec<DeterministicSpannerArc>, DeterministicSpannerSparsifyError> {
        shortest_path(
            self.vertex_count,
            source,
            sink,
            self.edges.iter().map(|edge| StoredEdge {
                edge: edge.edge,
                from: edge.from,
                to: edge.to,
            }),
        )
    }
}

fn shortest_path(
    vertex_count: usize,
    source: usize,
    sink: usize,
    edges: impl IntoIterator<Item = StoredEdge>,
) -> Result<Vec<DeterministicSpannerArc>, DeterministicSpannerSparsifyError> {
    if source == sink {
        return Ok(Vec::new());
    }
    let edges = edges.into_iter().collect::<Vec<_>>();
    let mut adjacency = vec![Vec::<(usize, usize, i8)>::new(); vertex_count];
    for edge in &edges {
        adjacency[edge.from].push((edge.edge, edge.to, 1));
        adjacency[edge.to].push((edge.edge, edge.from, -1));
    }
    for list in &mut adjacency {
        list.sort_unstable();
    }
    let mut previous = vec![None::<(usize, DeterministicSpannerArc)>; vertex_count];
    let mut queue = VecDeque::from([source]);
    previous[source] = Some((
        source,
        DeterministicSpannerArc {
            edge: usize::MAX,
            direction: 1,
        },
    ));
    while let Some(vertex) = queue.pop_front() {
        if vertex == sink {
            break;
        }
        for &(edge, next, direction) in &adjacency[vertex] {
            if previous[next].is_none() {
                previous[next] = Some((vertex, DeterministicSpannerArc { edge, direction }));
                queue.push_back(next);
            }
        }
    }
    if previous[sink].is_none() {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }
    let mut path = Vec::new();
    let mut cursor = sink;
    while cursor != source {
        let (parent, arc) =
            previous[cursor].ok_or(DeterministicSpannerSparsifyError::InvariantViolation)?;
        path.push(arc);
        cursor = parent;
    }
    path.reverse();
    Ok(path)
}

fn minimum_loopy_conductance(
    graph: &GraphView,
    components: &[Vec<usize>],
    loop_count: usize,
) -> Result<BigRational, DeterministicSpannerSparsifyError> {
    let mut minimum: Option<BigRational> = None;
    for component in components {
        let component_set = component.iter().copied().collect::<BTreeSet<_>>();
        let total_volume = component.iter().try_fold(0_usize, |sum, &vertex| {
            sum.checked_add(graph.degrees[vertex])
                .and_then(|value| value.checked_add(loop_count))
        });
        let total_volume =
            total_volume.ok_or(DeterministicSpannerSparsifyError::ArithmeticOverflow)?;
        let subset_count = 1_usize
            .checked_shl(
                u32::try_from(component.len())
                    .map_err(|_| DeterministicSpannerSparsifyError::AdmissionLimit)?,
            )
            .ok_or(DeterministicSpannerSparsifyError::AdmissionLimit)?;
        for mask in 1..subset_count - 1 {
            if mask & 1 == 0 {
                continue;
            }
            let subset = component
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1_usize << index) != 0)
                .map(|(_, &vertex)| vertex)
                .collect::<BTreeSet<_>>();
            let volume = subset.iter().try_fold(0_usize, |sum, &vertex| {
                sum.checked_add(graph.degrees[vertex])
                    .and_then(|value| value.checked_add(loop_count))
            });
            let volume = volume.ok_or(DeterministicSpannerSparsifyError::ArithmeticOverflow)?;
            let cut = graph
                .edges
                .iter()
                .filter(|edge| {
                    component_set.contains(&edge.from)
                        && component_set.contains(&edge.to)
                        && (subset.contains(&edge.from) != subset.contains(&edge.to))
                })
                .count();
            let denominator = volume.min(total_volume - volume);
            if cut == 0 || denominator == 0 {
                return Err(DeterministicSpannerSparsifyError::InvariantViolation);
            }
            let ratio = BigRational::new(BigInt::from(cut), BigInt::from(denominator));
            if minimum.as_ref().is_none_or(|current| &ratio < current) {
                minimum = Some(ratio);
            }
        }
    }
    minimum.ok_or(DeterministicSpannerSparsifyError::InvariantViolation)
}

fn minimum_witness_conductance(
    vertex_count: usize,
    witness_edges: &[DeterministicSpannerWitnessEdge],
    components: &[Vec<usize>],
) -> Result<BigRational, DeterministicSpannerSparsifyError> {
    let mut degrees = vec![0_usize; vertex_count];
    for edge in witness_edges {
        degrees[edge.from] = degrees[edge.from]
            .checked_add(1)
            .ok_or(DeterministicSpannerSparsifyError::ArithmeticOverflow)?;
        degrees[edge.to] = degrees[edge.to]
            .checked_add(1)
            .ok_or(DeterministicSpannerSparsifyError::ArithmeticOverflow)?;
    }
    let mut minimum: Option<BigRational> = None;
    for component in components {
        let total_volume = component
            .iter()
            .try_fold(0_usize, |sum, &vertex| sum.checked_add(degrees[vertex]));
        let total_volume =
            total_volume.ok_or(DeterministicSpannerSparsifyError::ArithmeticOverflow)?;
        let subset_count = 1_usize
            .checked_shl(
                u32::try_from(component.len())
                    .map_err(|_| DeterministicSpannerSparsifyError::AdmissionLimit)?,
            )
            .ok_or(DeterministicSpannerSparsifyError::AdmissionLimit)?;
        for mask in 1..subset_count - 1 {
            if mask & 1 == 0 {
                continue;
            }
            let subset = component
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1_usize << index) != 0)
                .map(|(_, &vertex)| vertex)
                .collect::<BTreeSet<_>>();
            let volume = subset
                .iter()
                .try_fold(0_usize, |sum, &vertex| sum.checked_add(degrees[vertex]));
            let volume = volume.ok_or(DeterministicSpannerSparsifyError::ArithmeticOverflow)?;
            let cut = witness_edges
                .iter()
                .filter(|edge| subset.contains(&edge.from) != subset.contains(&edge.to))
                .count();
            let denominator = volume.min(total_volume.saturating_sub(volume));
            if cut == 0 || denominator == 0 {
                return Err(DeterministicSpannerSparsifyError::InvariantViolation);
            }
            let ratio = BigRational::new(BigInt::from(cut), BigInt::from(denominator));
            if minimum.as_ref().is_none_or(|current| &ratio < current) {
                minimum = Some(ratio);
            }
        }
    }
    minimum.ok_or(DeterministicSpannerSparsifyError::InvariantViolation)
}

fn construct_witness(
    graph: &GraphView,
    components: &[Vec<usize>],
    decomposition_level: usize,
    phi: &BigRational,
) -> Result<
    (
        Vec<DeterministicSpannerWitnessEdge>,
        Vec<DeterministicSpannerDegreeTarget>,
    ),
    DeterministicSpannerSparsifyError,
> {
    let scale = BigRational::from_integer(BigInt::from(
        1_usize
            .checked_shl(
                u32::try_from(decomposition_level)
                    .map_err(|_| DeterministicSpannerSparsifyError::AdmissionLimit)?,
            )
            .ok_or(DeterministicSpannerSparsifyError::AdmissionLimit)?,
    ));
    let mut witness = Vec::new();
    let mut targets = Vec::new();
    for component in components {
        let mut lower = BTreeMap::<usize, usize>::new();
        let mut upper = BTreeMap::<usize, usize>::new();
        for &vertex in component {
            let target =
                BigRational::from_integer(BigInt::from(graph.degrees[vertex])) / (phi * &scale);
            let low = ceil_rational(&target)?;
            let high = floor_rational(&(target * BigInt::from(18_u8)))?.max(low);
            lower.insert(vertex, low.max(1));
            upper.insert(vertex, high.max(low.max(1)));
            targets.push(DeterministicSpannerDegreeTarget {
                vertex,
                lower: low.max(1),
                upper: high.max(low.max(1)),
            });
        }
        let mut degrees = component
            .iter()
            .map(|&vertex| (vertex, 0_usize))
            .collect::<BTreeMap<_, _>>();
        if component.len() == 2 {
            add_witness_edge(&mut witness, &mut degrees, component[0], component[1])?;
        } else {
            for index in 0..component.len() {
                add_witness_edge(
                    &mut witness,
                    &mut degrees,
                    component[index],
                    component[(index + 1) % component.len()],
                )?;
            }
        }
        loop {
            let deficient = component
                .iter()
                .copied()
                .filter(|vertex| degrees[vertex] < lower[vertex])
                .max_by_key(|vertex| (lower[vertex] - degrees[vertex], usize::MAX - *vertex));
            let Some(vertex) = deficient else {
                break;
            };
            let mate = component
                .iter()
                .copied()
                .filter(|candidate| *candidate != vertex && degrees[candidate] < upper[candidate])
                .max_by_key(|candidate| {
                    (
                        lower[candidate].saturating_sub(degrees[candidate]),
                        upper[candidate] - degrees[candidate],
                        usize::MAX - *candidate,
                    )
                })
                .ok_or(DeterministicSpannerSparsifyError::AdmissionLimit)?;
            if degrees[&vertex] >= upper[&vertex] {
                return Err(DeterministicSpannerSparsifyError::AdmissionLimit);
            }
            add_witness_edge(&mut witness, &mut degrees, vertex, mate)?;
        }
        if component
            .iter()
            .any(|vertex| degrees[vertex] < lower[vertex] || degrees[vertex] > upper[vertex])
        {
            return Err(DeterministicSpannerSparsifyError::InvariantViolation);
        }
    }
    targets.sort_by_key(|target| target.vertex);
    Ok((witness, targets))
}

fn add_witness_edge(
    witness: &mut Vec<DeterministicSpannerWitnessEdge>,
    degrees: &mut BTreeMap<usize, usize>,
    left: usize,
    right: usize,
) -> Result<(), DeterministicSpannerSparsifyError> {
    if witness.len() >= DETERMINISTIC_SPANNER_MAX_WITNESS_EDGES || left == right {
        return Err(DeterministicSpannerSparsifyError::AdmissionLimit);
    }
    let edge = witness.len();
    witness.push(DeterministicSpannerWitnessEdge {
        edge,
        from: left.min(right),
        to: left.max(right),
    });
    *degrees
        .get_mut(&left)
        .ok_or(DeterministicSpannerSparsifyError::InvariantViolation)? += 1;
    *degrees
        .get_mut(&right)
        .ok_or(DeterministicSpannerSparsifyError::InvariantViolation)? += 1;
    Ok(())
}

fn compose_embedding(
    source: usize,
    sink: usize,
    witness_path: &[DeterministicSpannerArc],
    witness_edges: &[DeterministicSpannerWitnessEdge],
    witness_to_graph: &[Vec<DeterministicSpannerArc>],
    graph: &GraphView,
) -> Result<Vec<DeterministicSpannerArc>, DeterministicSpannerSparsifyError> {
    let mut walk = Vec::new();
    for arc in witness_path {
        let witness = witness_edges
            .get(arc.edge)
            .ok_or(DeterministicSpannerSparsifyError::InvariantViolation)?;
        let path = witness_to_graph
            .get(witness.edge)
            .ok_or(DeterministicSpannerSparsifyError::InvariantViolation)?;
        if arc.direction == 1 {
            walk.extend_from_slice(path);
        } else if arc.direction == -1 {
            walk.extend(path.iter().rev().map(|arc| DeterministicSpannerArc {
                edge: arc.edge,
                direction: -arc.direction,
            }));
        } else {
            return Err(DeterministicSpannerSparsifyError::InvariantViolation);
        }
    }
    loop_erase(source, sink, &walk, graph)
}

fn loop_erase(
    source: usize,
    sink: usize,
    walk: &[DeterministicSpannerArc],
    graph: &GraphView,
) -> Result<Vec<DeterministicSpannerArc>, DeterministicSpannerSparsifyError> {
    let mut vertices = vec![source];
    let mut path = Vec::new();
    for &arc in walk {
        let row = graph
            .row(arc.edge)
            .ok_or(DeterministicSpannerSparsifyError::InvariantViolation)?;
        let current = *vertices
            .last()
            .ok_or(DeterministicSpannerSparsifyError::InvariantViolation)?;
        let next = match arc.direction {
            1 if row.from == current => row.to,
            -1 if row.to == current => row.from,
            _ => return Err(DeterministicSpannerSparsifyError::InvariantViolation),
        };
        if let Some(position) = vertices.iter().position(|&vertex| vertex == next) {
            vertices.truncate(position + 1);
            path.truncate(position);
        } else {
            vertices.push(next);
            path.push(arc);
        }
    }
    if vertices.last().copied() != Some(sink) {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }
    Ok(path)
}

fn maximum_congestion(
    edge_slot_count: usize,
    embedding: &[Vec<DeterministicSpannerArc>],
) -> Result<usize, DeterministicSpannerSparsifyError> {
    let mut congestion = vec![0_usize; edge_slot_count];
    for path in embedding {
        for arc in path {
            congestion[arc.edge] = congestion[arc.edge]
                .checked_add(1)
                .ok_or(DeterministicSpannerSparsifyError::ArithmeticOverflow)?;
        }
    }
    Ok(congestion.into_iter().max().unwrap_or(0))
}

fn maximum_weighted_vertex_congestion(
    vertex_count: usize,
    witness_edges: &[DeterministicSpannerWitnessEdge],
    paths: &[Vec<DeterministicSpannerArc>],
    graph: &GraphView,
    witness_weight: &BigRational,
) -> Result<BigRational, DeterministicSpannerSparsifyError> {
    if paths.len() != witness_edges.len() {
        return Err(DeterministicSpannerSparsifyError::InvariantViolation);
    }
    let mut congestion = vec![BigRational::zero(); vertex_count];
    for edge in witness_edges {
        let mut cursor = edge.from;
        let mut seen = BTreeSet::from([cursor]);
        congestion[cursor] += witness_weight;
        for arc in &paths[edge.edge] {
            let row = graph
                .row(arc.edge)
                .ok_or(DeterministicSpannerSparsifyError::InvariantViolation)?;
            cursor = match arc.direction {
                1 if row.from == cursor => row.to,
                -1 if row.to == cursor => row.from,
                _ => return Err(DeterministicSpannerSparsifyError::InvariantViolation),
            };
            if !seen.insert(cursor) {
                return Err(DeterministicSpannerSparsifyError::InvariantViolation);
            }
            congestion[cursor] += witness_weight;
        }
        if cursor != edge.to {
            return Err(DeterministicSpannerSparsifyError::InvariantViolation);
        }
    }
    Ok(congestion.into_iter().max().unwrap_or_default())
}

fn ceil_log2(value: usize) -> usize {
    if value <= 1 {
        0
    } else {
        usize::try_from(usize::BITS - (value - 1).leading_zeros()).expect("bit width fits usize")
    }
}

fn ceil_rational(value: &BigRational) -> Result<usize, DeterministicSpannerSparsifyError> {
    let quotient = value.numer() / value.denom();
    let rounded = if value.numer() % value.denom() == BigInt::zero() {
        quotient
    } else {
        quotient + BigInt::one()
    };
    usize::try_from(rounded).map_err(|_| DeterministicSpannerSparsifyError::AdmissionLimit)
}

fn floor_rational(value: &BigRational) -> Result<usize, DeterministicSpannerSparsifyError> {
    usize::try_from(value.numer() / value.denom())
        .map_err(|_| DeterministicSpannerSparsifyError::AdmissionLimit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(edge: usize, from: usize, to: usize) -> DeterministicSpannerInputEdge {
        DeterministicSpannerInputEdge { edge, from, to }
    }

    #[test]
    fn three_source_tasks_return_a_certified_subgraph_with_exact_paths() {
        let graph = vec![
            edge(0, 0, 1),
            edge(1, 1, 2),
            edge(2, 2, 3),
            edge(3, 3, 0),
            edge(4, 0, 2),
            edge(5, 1, 3),
        ];
        let result = bounded_deterministic_sparsify(4, 6, &graph).expect("sparsify");
        assert!(!result.certificate.witness_edges.is_empty());
        assert_eq!(result.certificate.task_two_rounds, 1);
        assert_eq!(result.certificate.task_three_rounds, 1);
        assert!(result.selected_edges.len() <= graph.len());
        assert!(result.embedding.iter().all(|path| path.len() <= 3));
        assert!(result.maximum_congestion > 0);
        check_bounded_deterministic_sparsify_certificate(4, 6, &graph, &result.certificate)
            .expect("certificate");

        let mut forged = result.certificate.clone();
        forged.phi += BigRational::one();
        assert_eq!(
            check_bounded_deterministic_sparsify_certificate(4, 6, &graph, &forged),
            Err(DeterministicSpannerSparsifyError::InvariantViolation)
        );

        let mut forged = result.certificate.clone();
        forged.witness_phi += BigRational::one();
        assert_eq!(
            check_bounded_deterministic_sparsify_certificate(4, 6, &graph, &forged),
            Err(DeterministicSpannerSparsifyError::InvariantViolation)
        );

        let mut forged = result.certificate.clone();
        forged.task_two_vertex_congestion += BigRational::one();
        assert_eq!(
            check_bounded_deterministic_sparsify_certificate(4, 6, &graph, &forged),
            Err(DeterministicSpannerSparsifyError::InvariantViolation)
        );

        let mut forged = result.certificate;
        forged.witness_to_graph[0][0].direction *= -1;
        assert_eq!(
            check_bounded_deterministic_sparsify_certificate(4, 6, &graph, &forged),
            Err(DeterministicSpannerSparsifyError::InvariantViolation)
        );
    }

    #[test]
    fn parallel_edges_keep_stable_slot_paths_and_degree_bounds() {
        let graph = vec![edge(0, 0, 1), edge(2, 0, 1), edge(4, 0, 1), edge(5, 0, 1)];
        let result = bounded_deterministic_sparsify(2, 6, &graph).expect("sparsify");
        assert!(result.embedding[1].is_empty());
        assert!(result.embedding[3].is_empty());
        assert!(
            result.embedding[0]
                .iter()
                .all(|arc| arc.edge % 2 == 0 || arc.edge == 5)
        );
        let degrees =
            result
                .certificate
                .witness_edges
                .iter()
                .fold(vec![0_usize; 2], |mut degrees, edge| {
                    degrees[edge.from] += 1;
                    degrees[edge.to] += 1;
                    degrees
                });
        for target in &result.certificate.degree_targets {
            assert!(degrees[target.vertex] >= target.lower);
            assert!(degrees[target.vertex] <= target.upper);
        }
    }
}
