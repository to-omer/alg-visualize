//! Deterministic materialized graph generators for the flow workspace.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;
use visualizer_core::jcs::{canonicalize, sha256_hex};
use visualizer_core::rng::{RngError, RngV1};

use crate::algorithms::{
    BOYKOV_KOLMOGOROV_MAX_EDGES, BOYKOV_KOLMOGOROV_MAX_NODES, HUNGARIAN_MAX_CELL_SCANS,
    HUNGARIAN_MAX_EDGES, HUNGARIAN_MAX_NODES, solve_dinic,
};
use crate::assignment::AssignmentObjectiveV1;
use crate::model::{
    EdgeId, FlowNetwork, FlowNode, MAX_FLOW_EDGES, MAX_FLOW_NODES, NodeId, UnresolvedFlowEdge,
};
use crate::scenario::{
    FlowBipartiteAdapterV1, FlowEdgeV1, FlowGraphV1, FlowNodeV1, FlowPlanarDartDirectionV1,
    FlowPlanarDartV1, FlowPlanarEmbeddingV1, FlowPlanarRotationV1, FlowPlanarTerminalCornersV1,
    FlowPositionV1, FlowProblemModelV1, GeneratorDifficultyCertificateV1, GeneratorProvenanceV1,
};
use crate::transportation::{TRANSPORTATION_MAX_EDGES, TRANSPORTATION_MAX_NODES};

/// Revision of every generator DTO and deterministic ordering rule in this module.
pub const FLOW_GENERATOR_REVISION: &str = "flow-generator/27";
/// Canonical family IDs that must each have a source-policy record.
pub const FLOW_GENERATOR_FAMILY_IDS: &[&str] = &[
    "arborescence",
    "assignment-matrix",
    "bipartite-random",
    "cherkassky-goldberg-ak-stress",
    "clustered-directed",
    "complete-dag",
    "cycle",
    "diamond-chain",
    "dinic-worst-case",
    "erdos-renyi-directed",
    "glover-dense-acyclic-stress",
    "goldberg-mesh-circulation",
    "goto-torus",
    "grid-2d",
    "grid-3d",
    "gridgen-grid",
    "gridgraph-grid",
    "hall-tight-bipartite",
    "ladder",
    "layered-dag",
    "multi-source-sink",
    "netgen-skeleton",
    "parallel-paths",
    "path",
    "planar-triangulated",
    "planted-bottleneck",
    "preferential-attachment-directed",
    "random-dag",
    "random-geometric",
    "random-regular-directed",
    "rmfgen-frames",
    "strongly-connected",
    "torus",
    "transportation-table",
    "vision-segmentation-grid",
    "waissi-setubal-acyclic-dense",
    "waissi-transit-one-way-grid",
    "waissi-transit-two-way-grid",
    "washington-basic-line",
    "washington-cheriyan-stress",
    "washington-dinic-phase-stress",
    "washington-double-exponential-line",
    "washington-exponential-line",
    "washington-goldberg-fifo-stress",
    "washington-matching",
    "washington-mesh",
    "washington-random-level",
    "washington-square-mesh",
    "watts-strogatz-fixed",
    "zadeh-phase-chain-stress",
];
/// Maximum UTF-8 byte length accepted before generator JSON decoding.
pub const MAX_FLOW_GENERATOR_SPEC_BYTES: usize = 64 * 1024 * 1024;

const TOPOLOGY_RNG_DOMAIN: &str = "rng.flow-generator.topology";
const CAPACITY_RNG_DOMAIN: &str = "rng.flow-generator.capacity";
const COST_RNG_DOMAIN: &str = "rng.flow-generator.cost";
const SUPPLY_RNG_DOMAIN: &str = "rng.flow-generator.supply";

/// Closed graph-family selection for the initial generator foundation.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "family_id", rename_all = "kebab-case")]
pub enum FlowGeneratorFamilyV1 {
    /// One directed source-to-sink path.
    Path {
        /// Total node count, including terminals.
        nodes: u32,
    },
    /// One directed cycle with fixed circular layout.
    Cycle {
        /// Total cycle-node count.
        nodes: u32,
    },
    /// Several internally vertex-disjoint source-to-sink paths.
    ParallelPaths {
        /// Number of parallel paths.
        path_count: u32,
        /// Internal nodes on every path.
        internal_nodes: u32,
    },
    /// Repeated split/merge diamonds.
    DiamondChain {
        /// Number of two-branch stages.
        stages: u32,
    },
    /// Two-row directed ladder with optional crossing rungs.
    Ladder {
        /// Number of columns, including terminal columns.
        columns: u32,
        /// Add both diagonal directions between adjacent columns.
        cross_edges: bool,
    },
    /// Sparse deterministic layered DAG.
    LayeredDag {
        /// Number of internal layers.
        layers: u32,
        /// Nodes per internal layer.
        width: u32,
        /// Consecutive targets per node in the next layer.
        fanout: u32,
    },
    /// Every forward edge under canonical topological ordering.
    CompleteDag {
        /// Total node count.
        nodes: u32,
    },
    /// Directed right/down rectangular grid.
    #[serde(rename = "grid-2d")]
    Grid2d {
        /// Grid rows.
        rows: u32,
        /// Grid columns.
        columns: u32,
        /// Add down-right diagonal arcs.
        diagonals: bool,
    },
    /// Terminal-heavy bidirectional 4/8-neighbor grid used by vision graph cuts.
    VisionSegmentationGrid {
        /// Pixel rows.
        rows: u32,
        /// Pixel columns.
        columns: u32,
        /// Add both diagonal neighbor pairs in addition to the 4-neighbor grid.
        eight_neighbor: bool,
    },
    /// Directed periodic right/down grid.
    Torus {
        /// Periodic rows; at least three avoids duplicate neighbors.
        rows: u32,
        /// Periodic columns; at least three avoids duplicate neighbors.
        columns: u32,
    },
    /// Uniform simple directed `G(n,m)` without self-loops.
    ErdosRenyiDirected {
        /// Total node count.
        nodes: u32,
        /// Exact number of uniformly sampled directed edges.
        edge_count: u32,
    },
    /// Waissi's acyclic family requiring exactly n-1 Dinic phases.
    DinicWorstCase {
        /// Total node count, including source and sink.
        nodes: u32,
    },
    /// First DIMACS function 9, kept as source-claimed deterministic stress.
    WashingtonDinicPhaseStress {
        /// Total node count, including source and sink.
        nodes: u32,
    },
    /// First DIMACS function 10, measured against the deterministic FIFO policy.
    WashingtonGoldbergFifoStress {
        /// Number of parallel unit bottlenecks and tail-chain edges.
        block_size: u32,
    },
    /// First DIMACS function 11, preserving its four chained gadgets and bridge.
    WashingtonCheriyanStress {
        /// Official parameter `n`: bridge width and finite gateway capacity.
        bridge_width: u32,
        /// Official parameter `m`: entry arcs into each chained gadget.
        gadget_entries: u32,
        /// Official parameter `c`: new chain vertices per gadget entry.
        chain_length: u32,
    },
    /// Cherkassky–Goldberg's deterministic AK family for push–relabel evaluation.
    CherkasskyGoldbergAkStress {
        /// Family size `k`; produces `4k+6` nodes and `6k+7` edges.
        size: u32,
    },
    /// First DIMACS AC: every canonical forward pair with random capacity.
    WaissiSetubalAcyclicDense {
        /// Total node count; produces `n(n-1)/2` edges.
        nodes: u32,
    },
    /// Glover special-capacity dense DAG as materialized by Waissi's generator.
    GloverDenseAcyclicStress {
        /// Total node count; produces `n(n-1)/2` edges.
        nodes: u32,
    },
    /// Waissi's square one-way transit grid with randomized street directions.
    WaissiTransitOneWayGrid {
        /// Width and height of the square internal transit grid.
        dimension: u32,
        /// Inclusive maximum capacity and direction-sampling range.
        maximum_capacity: u32,
    },
    /// Waissi's square two-way transit grid with random positive capacities.
    WaissiTransitTwoWayGrid {
        /// Width and height of the square internal transit grid.
        dimension: u32,
        /// Inclusive maximum capacity for every directed arc.
        maximum_capacity: u32,
    },
    /// Goldberg's toroidal long-range mesh, transformed from signed bounds.
    GoldbergMeshCirculation {
        /// Horizontal period `X` of the toroidal grid.
        columns: u32,
        /// Vertical period `Y` of the toroidal grid.
        rows: u32,
        /// Positive horizontal distances materialized from every node.
        horizontal_degree: u32,
        /// Positive vertical distances materialized from every node.
        vertical_degree: u32,
    },
    /// First DIMACS function 4 random unit-capacity bipartite network.
    WashingtonMatching {
        /// Vertices in each side of the balanced bipartition.
        part_size: u32,
        /// Distinct right-side neighbors selected by every left vertex.
        degree: u32,
    },
    /// First DIMACS function 1 cylindrical three-neighbor layered mesh.
    WashingtonMesh {
        /// Vertices per level; at least three keep the three neighbors distinct.
        rows: u32,
        /// Number of consecutive levels.
        columns: u32,
        /// Inclusive maximum inter-level capacity; minimum is one.
        maximum_capacity: u32,
    },
    /// First DIMACS function 5 row-major forward-offset square mesh.
    WashingtonSquareMesh {
        /// Width and height of the square row-major grid.
        dimension: u32,
        /// Consecutive forward offsets attempted from every non-final column.
        degree: u32,
        /// Inclusive maximum internal capacity; minimum is one.
        maximum_capacity: u32,
    },
    /// First DIMACS function 6 forward line with uniform capacities.
    WashingtonBasicLine {
        /// Number of width-sized blocks in the internal line.
        levels: u32,
        /// Vertices per block and source/sink terminal degree.
        width: u32,
        /// Distinct forward offsets sampled per internal vertex.
        degree: u32,
    },
    /// First DIMACS function 7 forward line with distance-decaying capacities.
    WashingtonExponentialLine {
        /// Number of width-sized blocks in the internal line.
        levels: u32,
        /// Vertices per block and source/sink terminal degree.
        width: u32,
        /// Distinct forward offsets sampled per internal vertex.
        degree: u32,
    },
    /// First DIMACS function 8 signed-offset line with distance-decaying capacities.
    WashingtonDoubleExponentialLine {
        /// Number of width-sized blocks in the internal line.
        levels: u32,
        /// Vertices per block and source/sink terminal degree.
        width: u32,
        /// Distinct signed offsets sampled per internal vertex.
        degree: u32,
    },
    /// Paper-inspired phase-chain stress with verified finite-size cubic growth.
    #[serde(rename = "zadeh-phase-chain-stress")]
    ZadehPhaseChainStress {
        /// Size of each of the three node groups; must be a multiple of four.
        group_size: u32,
    },
    /// Complete rooted directed tree.
    Arborescence {
        /// Children of every non-leaf node.
        branching: u32,
        /// Number of edge levels below the root.
        depth: u32,
    },
    /// Directed base cycle plus uniformly sampled non-cycle arcs.
    StronglyConnected {
        /// Total node count.
        nodes: u32,
        /// Exact number of additional simple arcs.
        extra_edges: u32,
    },
    /// Positive-axis directed three-dimensional grid.
    #[serde(rename = "grid-3d")]
    Grid3d {
        /// Number of z layers.
        layers: u32,
        /// Rows per layer.
        rows: u32,
        /// Columns per layer.
        columns: u32,
    },
    /// Exact uniform bipartite allowed-edge sample with explicit terminals.
    BipartiteRandom {
        /// Left partition size.
        left: u32,
        /// Right partition size.
        right: u32,
        /// Exact sampled left-to-right edge count.
        edge_count: u32,
    },
    /// Native rectangular assignment graph with a declared cost-matrix shape.
    AssignmentMatrix {
        /// Number of agents which must each receive one distinct task.
        agents: u32,
        /// Number of available tasks.
        tasks: u32,
        /// Minimize or maximize the selected edge-cost sum.
        objective: AssignmentObjectiveV1,
        /// Allowed-edge topology and integral cost-matrix construction.
        shape: AssignmentMatrixShapeV1,
    },
    /// Native balanced transportation table with explicit allowed routes.
    TransportationTable {
        /// Number of positive-supply origins.
        origins: u32,
        /// Number of negative-supply destinations.
        destinations: u32,
        /// Positive total supply and absolute total demand.
        total_supply: u32,
        /// Allowed-route topology, supply construction, and cost matrix.
        shape: TransportationTableShapeV1,
    },
    /// Integer-coordinate Gilbert disk graph, oriented by node ordinal.
    RandomGeometric {
        /// Total point count. The current practical cap is 448 because the
        /// safe complete-graph edge bound must fit the global edge cap.
        nodes: u32,
        /// Inclusive Euclidean connection radius in layout-coordinate units.
        radius: u32,
    },
    /// Randomly relabeled directed circulant with exact in/out degree.
    RandomRegularDirected {
        /// Total node count.
        nodes: u32,
        /// Exact in-degree and out-degree of every node.
        degree: u32,
    },
    /// Finite Barabasi-Albert construction, oriented from older to newer nodes.
    PreferentialAttachmentDirected {
        /// Total node count.
        nodes: u32,
        /// Distinct older neighbors selected for every post-seed node.
        attachment_count: u32,
    },
    /// Fan triangulation of a convex polygon with acyclic edge orientation.
    PlanarTriangulated {
        /// Boundary vertex count.
        nodes: u32,
    },
    /// Explicit super-terminal transform around source/middle/sink partitions.
    MultiSourceSink {
        /// Number of vertices adjacent to the super source.
        sources: u32,
        /// Connector-layer width.
        intermediate: u32,
        /// Number of vertices adjacent to the super sink.
        sinks: u32,
    },
    /// Uniform exact-edge-count DAG over a fixed topological order.
    RandomDag {
        /// Total node count.
        nodes: u32,
        /// Number of sampled forward edges.
        edge_count: u32,
    },
    /// Directed fixed-rewire-count derivative of the Watts-Strogatz model.
    WattsStrogatzFixed {
        /// Total ring node count.
        nodes: u32,
        /// Even directed neighborhood size before rewiring.
        neighborhood: u32,
        /// Exact number of clockwise lattice arcs rewired.
        rewire_count: u32,
    },
    /// Directed cycle clusters with uniformly sampled cross-cluster arcs.
    ClusteredDirected {
        /// Number of clusters.
        clusters: u32,
        /// Nodes in each equal-size cluster.
        cluster_size: u32,
        /// Exact number of sampled cross-cluster arcs.
        bridge_edges: u32,
    },
    /// Random middle cut with unit bottleneck arcs and high-capacity outer arcs.
    PlantedBottleneck {
        /// Left partition size.
        left: u32,
        /// Right partition size.
        right: u32,
        /// Exact number of unit-capacity cut arcs.
        cut_edges: u32,
    },
    /// Unit-capacity bipartite graph with an explicit Hall-tight prefix.
    HallTightBipartite {
        /// Equal left/right partition size.
        part_size: u32,
        /// Prefix size whose neighborhood is exactly the same-size right prefix.
        tight_prefix: u32,
    },
    /// Goldfarb--Grigoriadis RMFGEN random-frame network with project RNGs.
    RmfgenFrames {
        /// Side length `a` of every square frame.
        frame_size: u32,
        /// Number `b` of consecutive frames.
        depth: u32,
        /// Minimum inter-frame capacity `c1` in the source range 0..=1000.
        minimum_capacity: u32,
        /// Maximum inter-frame capacity `c2` in the source range 0..=1000.
        maximum_capacity: u32,
    },
    /// Lee--Orlin GRIDGEN grid/supernode structure with project RNGs.
    GridgenGrid {
        /// Number of grid rows, excluding the supernode.
        rows: u32,
        /// Number of grid columns, excluding the supernode.
        columns: u32,
        /// Equal number of distinct positive-supply and negative-demand nodes.
        terminal_pairs: u32,
        /// Target average out-degree, counting the supernode.
        average_degree: u32,
        /// Positive total supply and absolute total demand.
        total_supply: u32,
        /// Whether every grid link is represented in both directions.
        two_way: bool,
        /// Inclusive minimum ordinary-arc capacity.
        minimum_capacity: u32,
        /// Inclusive maximum ordinary-arc capacity.
        maximum_capacity: u32,
        /// Inclusive minimum nonnegative ordinary-arc unit cost.
        minimum_cost: u32,
        /// Inclusive maximum nonnegative ordinary-arc unit cost.
        maximum_cost: u32,
    },
    /// Resende GRIDGRAPH right/down grid with project RNGs.
    GridgraphGrid {
        /// Number `W` of grid rows, excluding the two terminals.
        rows: u32,
        /// Number `L` of grid columns, excluding the two terminals.
        columns: u32,
        /// Inclusive maximum ordinary-grid capacity; minimum is one.
        maximum_capacity: u32,
        /// Inclusive maximum arc cost; minimum is one.
        maximum_cost: u32,
    },
    /// Anderson's Washington Random Level max-flow structure with project RNGs.
    WashingtonRandomLevel {
        /// Vertices per level; at least three are needed for three distinct targets.
        rows: u32,
        /// Number of consecutive levels.
        columns: u32,
        /// Inclusive maximum inter-level capacity; minimum is one.
        maximum_capacity: u32,
    },
    /// Goldberg GOTO grid-on-torus structure with project RNGs and integer decay.
    GotoTorus {
        /// Total node count, including any non-grid remainder nodes.
        nodes: u32,
        /// Exact materialized arc count.
        edge_count: u32,
        /// Inclusive maximum capacity used by ordinary arcs.
        maximum_capacity: u32,
        /// Inclusive maximum cost used by horizontal arcs.
        maximum_cost: u32,
    },
    /// Klingman--Napier--Stutz NETGEN skeleton with project RNGs.
    NetgenSkeleton {
        /// Total node count.
        nodes: u32,
        /// Source count, including transshipment-enabled sources.
        sources: u32,
        /// Sink count, including transshipment-enabled sinks.
        sinks: u32,
        /// Exact materialized arc count.
        edge_count: u32,
        /// Inclusive minimum signed unit cost.
        minimum_cost: i64,
        /// Inclusive maximum signed unit cost.
        maximum_cost: i64,
        /// Positive total supply and absolute total demand.
        total_supply: u32,
        /// Sources which may also have incoming arcs.
        transshipment_sources: u32,
        /// Sinks which may also have outgoing arcs.
        transshipment_sinks: u32,
        /// Percentage of skeleton arcs assigned the maximum cost.
        high_cost_percentage: u32,
        /// Percentage of arcs assigned a finite sampled capacity.
        capacitated_percentage: u32,
        /// Inclusive minimum sampled capacity.
        minimum_capacity: u32,
        /// Inclusive maximum sampled capacity.
        maximum_capacity: u32,
    },
}

/// Cost and allowed-edge constructions for native assignment scenarios.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "kebab-case")]
pub enum AssignmentMatrixShapeV1 {
    /// Exact-density allowed-edge sample with independent uniform costs.
    Uniform {
        /// Exact edge density in thousandths, in `0..=1000`.
        density_per_mille: u32,
        /// Inclusive minimum edge cost.
        minimum_cost: i64,
        /// Inclusive maximum edge cost.
        maximum_cost: i64,
    },
    /// Complete matrix in which every allowed assignment has the same cost.
    Equal {
        /// Shared cost of every pair.
        cost: i64,
    },
    /// Complete matrix with cheap within-block and expensive cross-block pairs.
    Block {
        /// Number of ordinal blocks used on both partitions.
        blocks: u32,
        /// Cost when agent and task belong to the same block.
        within_cost: i64,
        /// Cost when the blocks differ.
        between_cost: i64,
    },
    /// Complete matrix with one planted pair per agent and close alternatives.
    NearTie {
        /// Cost of every planted pair.
        base_cost: i64,
        /// Positive objective-oriented separation from every alternative.
        gap: u32,
    },
    /// Sparse matrix with a random planted unique optimum.
    PlantedOptimum {
        /// Exact edge density in thousandths, in `1..=1000`.
        density_per_mille: u32,
        /// Cost of every planted pair.
        base_cost: i64,
        /// Positive objective-oriented separation from every distractor.
        gap: u32,
        /// Inclusive nonnegative random cost spread beyond the gap.
        noise: u32,
    },
    /// Complete Monge matrix `scale * |agent-task|`.
    Monge {
        /// Positive integral scale.
        scale: u32,
    },
    /// Complete anti-Monge matrix `-scale * |agent-task|`.
    AntiMonge {
        /// Positive integral scale.
        scale: u32,
    },
    /// Independently sampled fixed out-degree per agent, without feasibility conditioning.
    SparseAllowed {
        /// Exact number of distinct tasks adjacent to every agent.
        degree: u32,
        /// Inclusive minimum edge cost.
        minimum_cost: i64,
        /// Inclusive maximum edge cost.
        maximum_cost: i64,
    },
    /// Explicit Hall-deficient prefix with an exact smaller neighborhood.
    HallDeficient {
        /// Size of the deficient agent prefix.
        witness_agents: u32,
        /// Exact neighborhood size of that prefix; strictly smaller.
        witness_tasks: u32,
        /// Inclusive minimum edge cost.
        minimum_cost: i64,
        /// Inclusive maximum edge cost.
        maximum_cost: i64,
    },
}

/// Cost, balance, and allowed-route constructions for transportation tables.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "kebab-case")]
pub enum TransportationTableShapeV1 {
    /// Complete table with independent inclusive-uniform integral costs.
    DenseUniform {
        /// Inclusive minimum route cost.
        minimum_cost: i64,
        /// Inclusive maximum route cost.
        maximum_cost: i64,
    },
    /// Random allowed routes containing a planted northwest-corner feasible support.
    SparseFeasible {
        /// Target allowed-route density in thousandths; the feasible support is
        /// retained even when it requires a larger realized density.
        density_per_mille: u32,
        /// Inclusive minimum route cost.
        minimum_cost: i64,
        /// Inclusive maximum route cost.
        maximum_cost: i64,
    },
    /// Complete equal-cost square table with unit supplies and demands.
    UnitDegenerate {
        /// Shared route cost.
        cost: i64,
    },
    /// Complete table with low within-block and high cross-block costs.
    Block {
        /// Positive ordinal block count shared by both partitions.
        blocks: u32,
        /// Cost when origin and destination ordinals share a block.
        within_cost: i64,
        /// Cost when their blocks differ.
        between_cost: i64,
    },
    /// Complete table with one cheap ordinal route and close alternatives.
    NearTie {
        /// Cost of each planted ordinal route.
        base_cost: i64,
        /// Positive separation from every alternative.
        gap: u32,
    },
    /// Complete Monge matrix `scale * |origin-destination|`.
    Monge {
        /// Positive integral scale.
        scale: u32,
    },
    /// Balanced table whose first origin can reach only a smaller-demand first destination.
    CutInfeasible {
        /// Inclusive minimum route cost.
        minimum_cost: i64,
        /// Inclusive maximum route cost.
        maximum_cost: i64,
    },
}

/// Orthogonal upper-capacity distribution.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "kebab-case")]
pub enum CapacityDistributionV1 {
    /// Every edge has capacity one.
    Unit {},
    /// Every edge has the same capacity.
    Constant {
        /// Canonical unsigned decimal.
        value: String,
    },
    /// Exact discrete uniform distribution over an inclusive interval.
    Uniform {
        /// Canonical unsigned inclusive minimum.
        minimum: String,
        /// Canonical unsigned inclusive maximum.
        maximum: String,
    },
    /// Equal-probability mixture of two distinct capacity atoms.
    Bimodal {
        /// First canonical unsigned atom.
        first: String,
        /// Second canonical unsigned atom.
        second: String,
    },
    /// Uniform selection among inclusive powers-of-two buckets.
    PowerOfTwoBuckets {
        /// Inclusive minimum exponent in `0..=63`.
        minimum_exponent: u32,
        /// Inclusive maximum exponent in `0..=63`.
        maximum_exponent: u32,
    },
}

/// Direction of the declared capacity-to-cost relationship.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CapacityCostCorrelationV1 {
    /// Larger capacities map to larger base costs.
    Positive,
    /// Larger capacities map to smaller base costs.
    Negative,
}

/// Orthogonal integral unit-cost distribution.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "kebab-case")]
pub enum CostDistributionV1 {
    /// Every edge has zero cost.
    Zero {},
    /// Every edge has the same signed cost.
    Constant {
        /// Canonical signed decimal.
        value: String,
    },
    /// Exact discrete uniform distribution over an inclusive interval.
    Uniform {
        /// Canonical signed inclusive minimum.
        minimum: String,
        /// Canonical signed inclusive maximum.
        maximum: String,
    },
    /// Equal-probability mixture of two distinct signed cost atoms.
    Bimodal {
        /// First canonical signed atom.
        first: String,
        /// Second canonical signed atom.
        second: String,
    },
    /// Linear capacity-correlated base cost plus bounded integral jitter.
    CapacityCorrelated {
        /// Inclusive minimum realized cost after clamping.
        minimum: String,
        /// Inclusive maximum realized cost after clamping.
        maximum: String,
        /// Positive or negative capacity/cost relationship.
        direction: CapacityCostCorrelationV1,
        /// Canonical nonnegative maximum absolute jitter.
        maximum_jitter: String,
    },
}

/// Strict materialized graph-generator request.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FlowGeneratorTargetProblemV1 {
    /// Preserve the problem model defined by the selected family.
    #[default]
    Native,
    /// Materialize the generated topology as an ordinary source/sink max-flow problem.
    MaxFlow,
    /// Legacy revision-26 target retained only to regenerate imported scenarios.
    MinCostMaxFlow,
    /// Materialize the generated topology as a fixed-flow minimum-cost problem.
    FixedFlowMinCost,
}

/// Strict materialized graph-generator request.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowGeneratorSpecV1 {
    /// Must equal `flow-generator/27`.
    pub generator_revision: String,
    /// Canonical unsigned 64-bit seed.
    pub seed: String,
    /// Topology and deterministic layout.
    pub family: FlowGeneratorFamilyV1,
    /// Capacity stream, independent from topology and cost streams.
    pub capacity: CapacityDistributionV1,
    /// Independent cost RNG stream; a declared correlated distribution may
    /// intentionally use the already-materialized capacity value.
    pub cost: CostDistributionV1,
    /// Explicit output problem. Native is omitted so revisions 1–25 remain
    /// decodable without reinterpreting their materialization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target_problem: Option<FlowGeneratorTargetProblemV1>,
}

/// Exact materialized summary shown before Scenario adoption.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowGeneratorStatsV1 {
    /// Generated node count.
    pub node_count: u32,
    /// Generated edge count.
    pub edge_count: u32,
    /// Minimum realized capacity, or zero for an edgeless graph.
    pub minimum_capacity: String,
    /// Maximum realized capacity, or zero for an edgeless graph.
    pub maximum_capacity: String,
    /// Minimum realized cost, or zero for an edgeless graph.
    pub minimum_cost: String,
    /// Maximum realized cost, or zero for an edgeless graph.
    pub maximum_cost: String,
}

/// Candidate graph plus reproducibility material ready for Scenario insertion.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedFlowGraphV1 {
    /// Materialized graph; generation parameters never remain implicit.
    pub graph: FlowGraphV1,
    /// Family-appropriate default model; the UI may explicitly replace it.
    pub suggested_model: FlowProblemModelV1,
    /// Digest and classification persisted until manual graph editing.
    pub provenance: GeneratorProvenanceV1,
    /// Exact realized statistics.
    pub stats: FlowGeneratorStatsV1,
}

/// Generator validation or bounded randomness failure.
#[derive(Debug, Error)]
pub enum FlowGenerationError {
    /// Generator JSON exceeds the transfer and decode budget.
    #[error("flow generator input exceeds the 64 MiB UTF-8 limit")]
    InputSize,
    /// Strict JSON decoding failed.
    #[error("invalid flow generator JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Revision or a bounded family/attribute parameter is invalid.
    #[error("invalid flow generator setting: {0}")]
    Invalid(&'static str),
    /// Requested topology exceeds a hard graph cap before allocation.
    #[error("flow generator size exceeds graph limits")]
    SizeLimit,
    /// Exact bounded random sampling failed.
    #[error(transparent)]
    Rng(#[from] RngError),
    /// Checked topology or attribute arithmetic overflowed.
    #[error("flow generator arithmetic overflow")]
    ArithmeticOverflow,
    /// Canonical digest construction failed.
    #[error("flow generator canonicalization failed")]
    Canonicalization,
}

/// Strictly decodes and generates one complete candidate graph.
///
/// # Errors
///
/// Rejects unknown fields, unsupported revision, noncanonical numbers,
/// preflight size violations, bounded RNG failure, or checked overflow.
pub fn generate_flow_graph_json(source: &str) -> Result<GeneratedFlowGraphV1, FlowGenerationError> {
    validate_generator_input_size(source.len())?;
    let spec: FlowGeneratorSpecV1 = serde_json::from_str(source)?;
    generate_flow_graph(&spec)
}

fn validate_generator_input_size(byte_len: usize) -> Result<(), FlowGenerationError> {
    if byte_len > MAX_FLOW_GENERATOR_SPEC_BYTES {
        return Err(FlowGenerationError::InputSize);
    }
    Ok(())
}

/// Deterministically materializes one candidate graph.
///
/// # Errors
///
/// Rejects invalid parameters before large allocation and returns no partial
/// graph if topology, attributes, validation, or digest construction fails.
pub fn generate_flow_graph(
    spec: &FlowGeneratorSpecV1,
) -> Result<GeneratedFlowGraphV1, FlowGenerationError> {
    let (seed, capacity_distribution, cost_distribution) = validate_generator_spec(spec)?;
    let mut topology_rng = RngV1::from_seed(seed, TOPOLOGY_RNG_DOMAIN);
    let mut capacity_rng = RngV1::from_seed(seed, CAPACITY_RNG_DOMAIN);
    let mut cost_rng = RngV1::from_seed(seed, COST_RNG_DOMAIN);
    let mut supply_rng = RngV1::from_seed(seed, SUPPLY_RNG_DOMAIN);
    let topology = build_topology(
        &spec.family,
        &mut topology_rng,
        &mut capacity_rng,
        &mut cost_rng,
        &mut supply_rng,
    )?;
    enforce_graph_limits(topology.nodes.len(), topology.edges.len())?;

    let mut capacities = Vec::with_capacity(topology.edges.len());
    let mut costs = Vec::with_capacity(topology.edges.len());
    let Topology {
        nodes,
        edges: topology_edges,
        suggested_model: native_model,
        fixed_capacities,
        fixed_costs,
    } = topology;
    let edges = topology_edges
        .into_iter()
        .enumerate()
        .map(|(index, (from, to))| {
            let capacity = match &fixed_capacities {
                Some(values) => *values
                    .get(index)
                    .ok_or(FlowGenerationError::Canonicalization)?,
                None => capacity_distribution.sample(&mut capacity_rng)?,
            };
            let cost = match &fixed_costs {
                Some(values) => *values
                    .get(index)
                    .ok_or(FlowGenerationError::Canonicalization)?,
                None => cost_distribution.sample(
                    &mut cost_rng,
                    capacity,
                    capacity_distribution.bounds(),
                )?,
            };
            capacities.push(capacity);
            costs.push(cost);
            Ok(FlowEdgeV1 {
                id: format!("e{index:06}"),
                from,
                to,
                lower: "0".to_owned(),
                capacity: capacity.to_string(),
                cost: cost.to_string(),
                convex_cost: None,
                initial_flow: None,
            })
        })
        .collect::<Result<Vec<_>, FlowGenerationError>>()?;
    let mut graph = FlowGraphV1 { nodes, edges };
    let suggested_model = adapt_generated_problem(
        &mut graph,
        native_model,
        spec.target_problem.unwrap_or_default(),
    )?;
    let stats = stats(&graph, &capacities, &costs)?;
    let classification = generator_classification(spec);
    let semantic = serde_json::json!({
        "graph": &graph,
        "suggested_model": &suggested_model,
    });
    let encoded = serde_json::to_vec(&semantic)?;
    let canonical = canonicalize(&encoded).map_err(|_| FlowGenerationError::Canonicalization)?;
    let parameters = spec_parameters(spec)?;
    let difficulty_certificate = difficulty_certificate(&spec.family)?;
    let provenance = GeneratorProvenanceV1 {
        generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
        family_id: family_id(&spec.family).to_owned(),
        seed: spec.seed.clone(),
        parameters,
        materialized_sha256: sha256_hex(&canonical),
        difficulty: classification.difficulty.to_owned(),
        origin: classification.origin.to_owned(),
        sampling: classification.sampling.to_owned(),
        tags: classification.tags,
        source_id: classification.source_id.to_owned(),
        difficulty_certificate,
    };
    Ok(GeneratedFlowGraphV1 {
        graph,
        suggested_model,
        provenance,
        stats,
    })
}

fn validate_generator_spec(
    spec: &FlowGeneratorSpecV1,
) -> Result<(u64, ValidatedCapacity, ValidatedCost), FlowGenerationError> {
    if spec.generator_revision != FLOW_GENERATOR_REVISION {
        return Err(FlowGenerationError::Invalid("generator revision"));
    }
    let seed = parse_u64(&spec.seed, "seed")?;
    let capacity = ValidatedCapacity::new(&spec.capacity)?;
    let cost = ValidatedCost::new(&spec.cost)?;
    if attributes_fixed_by_construction(&spec.family)
        && (!matches!(spec.capacity, CapacityDistributionV1::Unit {})
            || !matches!(spec.cost, CostDistributionV1::Zero {}))
    {
        return Err(FlowGenerationError::Invalid(
            "family attributes are fixed by the declared construction",
        ));
    }
    Ok((seed, capacity, cost))
}

fn generated_max_flow_terminals(
    graph: &FlowGraphV1,
) -> Result<(String, String), FlowGenerationError> {
    let first_edge = graph.edges.iter().find(|edge| edge.from != edge.to).ok_or(
        FlowGenerationError::Invalid("target max-flow topology has no directed connection"),
    )?;
    let source = first_edge.from.clone();
    let mut adjacency = BTreeMap::<&str, Vec<&str>>::new();
    for edge in &graph.edges {
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }
    let mut distance = BTreeMap::from([(source.as_str(), 0_u32)]);
    let mut queue = std::collections::VecDeque::from([source.as_str()]);
    while let Some(node) = queue.pop_front() {
        let next_distance = distance
            .get(node)
            .copied()
            .ok_or(FlowGenerationError::Canonicalization)?
            .checked_add(1)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        for target in adjacency.get(node).into_iter().flatten().copied() {
            if distance.contains_key(target) {
                continue;
            }
            distance.insert(target, next_distance);
            queue.push_back(target);
        }
    }
    let sink = distance
        .into_iter()
        .filter(|(node, _)| *node != source)
        .max_by(|(left_node, left_distance), (right_node, right_distance)| {
            left_distance
                .cmp(right_distance)
                .then_with(|| right_node.cmp(left_node))
        })
        .map(|(node, _)| node.to_owned())
        .ok_or(FlowGenerationError::Invalid(
            "target max-flow topology has no reachable sink",
        ))?;
    Ok((source, sink))
}

fn adapt_generated_problem(
    graph: &mut FlowGraphV1,
    native: FlowProblemModelV1,
    target: FlowGeneratorTargetProblemV1,
) -> Result<FlowProblemModelV1, FlowGenerationError> {
    match target {
        FlowGeneratorTargetProblemV1::Native => Ok(native),
        FlowGeneratorTargetProblemV1::MinCostMaxFlow => match native {
            FlowProblemModelV1::MaxFlow { source, sink }
            | FlowProblemModelV1::PlanarMaxFlow { source, sink, .. } => {
                Ok(FlowProblemModelV1::MinCostMaxFlow { source, sink })
            }
            FlowProblemModelV1::BipartiteMatching {
                flow_adapter: Some(adapter),
                ..
            } => Ok(FlowProblemModelV1::MinCostMaxFlow {
                source: adapter.source,
                sink: adapter.sink,
            }),
            _ => Err(FlowGenerationError::Invalid(
                "family cannot be materialized as min-cost max-flow",
            )),
        },
        FlowGeneratorTargetProblemV1::FixedFlowMinCost => match native {
            FlowProblemModelV1::MaxFlow { source, sink }
            | FlowProblemModelV1::PlanarMaxFlow { source, sink, .. } => {
                generated_fixed_flow_min_cost_model(graph, source, sink)
            }
            FlowProblemModelV1::BipartiteMatching {
                flow_adapter: Some(adapter),
                ..
            } => generated_fixed_flow_min_cost_model(graph, adapter.source, adapter.sink),
            _ => Err(FlowGenerationError::Invalid(
                "family cannot be materialized as fixed-flow min-cost",
            )),
        },
        FlowGeneratorTargetProblemV1::MaxFlow => match native {
            model @ FlowProblemModelV1::MaxFlow { .. } => Ok(model),
            FlowProblemModelV1::Circulation {} | FlowProblemModelV1::Transshipment {} => {
                let (source, sink) = generated_max_flow_terminals(graph)?;
                for node in &mut graph.nodes {
                    "0".clone_into(&mut node.supply);
                }
                Ok(FlowProblemModelV1::MaxFlow { source, sink })
            }
            _ => Err(FlowGenerationError::Invalid(
                "family cannot be materialized as max-flow",
            )),
        },
    }
}

fn generated_fixed_flow_min_cost_model(
    graph: &FlowGraphV1,
    source: String,
    sink: String,
) -> Result<FlowProblemModelV1, FlowGenerationError> {
    let network = generated_flow_network(graph)?;
    let source_id = NodeId::parse(&source).map_err(|_| FlowGenerationError::Canonicalization)?;
    let sink_id = NodeId::parse(&sink).map_err(|_| FlowGenerationError::Canonicalization)?;
    let source_index = network
        .node_index(&source_id)
        .ok_or(FlowGenerationError::Canonicalization)?;
    let sink_index = network
        .node_index(&sink_id)
        .ok_or(FlowGenerationError::Canonicalization)?;
    let maximum = solve_dinic(&network, source_index, sink_index)
        .map_err(|_| FlowGenerationError::Canonicalization)?;
    let required_flow = u64::try_from(maximum.certificate.value)
        .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
    Ok(FlowProblemModelV1::FixedFlowMinCost {
        source,
        sink,
        required_flow: required_flow.to_string(),
    })
}

fn generated_flow_network(graph: &FlowGraphV1) -> Result<FlowNetwork, FlowGenerationError> {
    let nodes = graph
        .nodes
        .iter()
        .map(|node| {
            Ok(FlowNode::new(
                NodeId::parse(&node.id).map_err(|_| FlowGenerationError::Canonicalization)?,
                parse_i64(&node.supply, "node supply")?,
            ))
        })
        .collect::<Result<Vec<_>, FlowGenerationError>>()?;
    let edges = graph
        .edges
        .iter()
        .map(|edge| {
            Ok(UnresolvedFlowEdge {
                id: EdgeId::parse(&edge.id).map_err(|_| FlowGenerationError::Canonicalization)?,
                from: NodeId::parse(&edge.from)
                    .map_err(|_| FlowGenerationError::Canonicalization)?,
                to: NodeId::parse(&edge.to).map_err(|_| FlowGenerationError::Canonicalization)?,
                lower: parse_u64(&edge.lower, "edge lower")?,
                capacity: parse_u64(&edge.capacity, "edge capacity")?,
                cost: parse_i64(&edge.cost, "edge cost")?,
            })
        })
        .collect::<Result<Vec<_>, FlowGenerationError>>()?;
    FlowNetwork::new(nodes, edges).map_err(|_| FlowGenerationError::Canonicalization)
}

pub(crate) fn difficulty_certificate(
    family: &FlowGeneratorFamilyV1,
) -> Result<Option<GeneratorDifficultyCertificateV1>, FlowGenerationError> {
    let certificate = match *family {
        FlowGeneratorFamilyV1::DinicWorstCase { nodes } => {
            let phases = u64::from(nodes)
                .checked_sub(1)
                .ok_or(FlowGenerationError::ArithmeticOverflow)?;
            GeneratorDifficultyCertificateV1 {
                target_algorithm_id: "dinic".to_owned(),
                tie_breaking: "stable-residual-id-level-bfs-current-arc-dfs".to_owned(),
                exact_metrics: BTreeMap::from([
                    ("bfs-runs".to_owned(), u64::from(nodes).to_string()),
                    ("blocking-flow-phases".to_owned(), phases.to_string()),
                    ("max-flow-value".to_owned(), phases.to_string()),
                ]),
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(certificate))
}

fn attributes_fixed_by_construction(family: &FlowGeneratorFamilyV1) -> bool {
    matches!(
        family,
        FlowGeneratorFamilyV1::DinicWorstCase { .. }
            | FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { .. }
            | FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { .. }
            | FlowGeneratorFamilyV1::WashingtonCheriyanStress { .. }
            | FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { .. }
            | FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { .. }
            | FlowGeneratorFamilyV1::GloverDenseAcyclicStress { .. }
            | FlowGeneratorFamilyV1::WaissiTransitOneWayGrid { .. }
            | FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid { .. }
            | FlowGeneratorFamilyV1::GoldbergMeshCirculation { .. }
            | FlowGeneratorFamilyV1::ZadehPhaseChainStress { .. }
            | FlowGeneratorFamilyV1::PlantedBottleneck { .. }
            | FlowGeneratorFamilyV1::HallTightBipartite { .. }
            | FlowGeneratorFamilyV1::AssignmentMatrix { .. }
            | FlowGeneratorFamilyV1::TransportationTable { .. }
            | FlowGeneratorFamilyV1::RmfgenFrames { .. }
            | FlowGeneratorFamilyV1::GridgenGrid { .. }
            | FlowGeneratorFamilyV1::GridgraphGrid { .. }
            | FlowGeneratorFamilyV1::WashingtonMatching { .. }
            | FlowGeneratorFamilyV1::WashingtonMesh { .. }
            | FlowGeneratorFamilyV1::WashingtonRandomLevel { .. }
            | FlowGeneratorFamilyV1::WashingtonSquareMesh { .. }
            | FlowGeneratorFamilyV1::WashingtonBasicLine { .. }
            | FlowGeneratorFamilyV1::WashingtonExponentialLine { .. }
            | FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine { .. }
            | FlowGeneratorFamilyV1::GotoTorus { .. }
            | FlowGeneratorFamilyV1::NetgenSkeleton { .. }
    )
}

pub(crate) struct GeneratorClassification {
    pub(crate) origin: &'static str,
    pub(crate) sampling: &'static str,
    pub(crate) difficulty: &'static str,
    pub(crate) tags: Vec<String>,
    pub(crate) source_id: &'static str,
}

pub(crate) fn generator_classification(spec: &FlowGeneratorSpecV1) -> GeneratorClassification {
    let family = &spec.family;
    let source_id = generator_source_id(family);
    let origin = generator_origin(family);
    let difficulty = generator_difficulty(family);
    let distribution_randomized = !attributes_fixed_by_construction(family)
        && (!matches!(
            &spec.capacity,
            CapacityDistributionV1::Unit {} | CapacityDistributionV1::Constant { .. }
        ) || !matches!(
            &spec.cost,
            CostDistributionV1::Zero {} | CostDistributionV1::Constant { .. }
        ));
    let sampling = if generator_family_is_randomized(family) || distribution_randomized {
        "randomized"
    } else {
        "deterministic"
    };
    let tags = generator_tags(family);
    GeneratorClassification {
        origin,
        sampling,
        difficulty,
        tags,
        source_id,
    }
}

fn generator_origin(family: &FlowGeneratorFamilyV1) -> &'static str {
    match family {
        FlowGeneratorFamilyV1::RmfgenFrames { .. }
        | FlowGeneratorFamilyV1::GridgenGrid { .. }
        | FlowGeneratorFamilyV1::GridgraphGrid { .. }
        | FlowGeneratorFamilyV1::WashingtonMatching { .. }
        | FlowGeneratorFamilyV1::WashingtonMesh { .. }
        | FlowGeneratorFamilyV1::WashingtonRandomLevel { .. }
        | FlowGeneratorFamilyV1::WashingtonSquareMesh { .. }
        | FlowGeneratorFamilyV1::WashingtonBasicLine { .. }
        | FlowGeneratorFamilyV1::WashingtonExponentialLine { .. }
        | FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine { .. }
        | FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { .. }
        | FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { .. }
        | FlowGeneratorFamilyV1::WashingtonCheriyanStress { .. }
        | FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { .. }
        | FlowGeneratorFamilyV1::GloverDenseAcyclicStress { .. }
        | FlowGeneratorFamilyV1::WaissiTransitOneWayGrid { .. }
        | FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid { .. }
        | FlowGeneratorFamilyV1::GoldbergMeshCirculation { .. }
        | FlowGeneratorFamilyV1::GotoTorus { .. }
        | FlowGeneratorFamilyV1::NetgenSkeleton { .. } => "official-benchmark-derived",
        FlowGeneratorFamilyV1::VisionSegmentationGrid { .. }
        | FlowGeneratorFamilyV1::RandomGeometric { .. }
        | FlowGeneratorFamilyV1::PreferentialAttachmentDirected { .. }
        | FlowGeneratorFamilyV1::WattsStrogatzFixed { .. }
        | FlowGeneratorFamilyV1::DinicWorstCase { .. }
        | FlowGeneratorFamilyV1::ZadehPhaseChainStress { .. }
        | FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { .. } => "paper-derived",
        _ => "project-synthetic",
    }
}

fn generator_difficulty(family: &FlowGeneratorFamilyV1) -> &'static str {
    match family {
        FlowGeneratorFamilyV1::DinicWorstCase { .. } => "verified-worst-case",
        FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { .. }
        | FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { .. }
        | FlowGeneratorFamilyV1::WashingtonCheriyanStress { .. }
        | FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { .. }
        | FlowGeneratorFamilyV1::GloverDenseAcyclicStress { .. }
        | FlowGeneratorFamilyV1::ZadehPhaseChainStress { .. }
        | FlowGeneratorFamilyV1::TransportationTable {
            shape:
                TransportationTableShapeV1::UnitDegenerate { .. }
                | TransportationTableShapeV1::NearTie { .. },
            ..
        }
        | FlowGeneratorFamilyV1::PlantedBottleneck { .. } => "stress",
        _ => "ordinary",
    }
}

pub(crate) fn generator_family_is_randomized(family: &FlowGeneratorFamilyV1) -> bool {
    matches!(
        family,
        FlowGeneratorFamilyV1::ErdosRenyiDirected { .. }
            | FlowGeneratorFamilyV1::StronglyConnected { .. }
            | FlowGeneratorFamilyV1::BipartiteRandom { .. }
            | FlowGeneratorFamilyV1::AssignmentMatrix {
                shape: AssignmentMatrixShapeV1::Uniform { .. }
                    | AssignmentMatrixShapeV1::PlantedOptimum { .. }
                    | AssignmentMatrixShapeV1::SparseAllowed { .. }
                    | AssignmentMatrixShapeV1::HallDeficient { .. },
                ..
            }
            | FlowGeneratorFamilyV1::TransportationTable {
                shape: TransportationTableShapeV1::DenseUniform { .. }
                    | TransportationTableShapeV1::SparseFeasible { .. }
                    | TransportationTableShapeV1::Block { .. }
                    | TransportationTableShapeV1::NearTie { .. }
                    | TransportationTableShapeV1::Monge { .. }
                    | TransportationTableShapeV1::CutInfeasible { .. },
                ..
            }
            | FlowGeneratorFamilyV1::RandomGeometric { .. }
            | FlowGeneratorFamilyV1::RandomRegularDirected { .. }
            | FlowGeneratorFamilyV1::PreferentialAttachmentDirected { .. }
            | FlowGeneratorFamilyV1::RandomDag { .. }
            | FlowGeneratorFamilyV1::WattsStrogatzFixed { .. }
            | FlowGeneratorFamilyV1::ClusteredDirected { .. }
            | FlowGeneratorFamilyV1::PlantedBottleneck { .. }
            | FlowGeneratorFamilyV1::RmfgenFrames { .. }
            | FlowGeneratorFamilyV1::GridgenGrid { .. }
            | FlowGeneratorFamilyV1::GridgraphGrid { .. }
            | FlowGeneratorFamilyV1::WashingtonMatching { .. }
            | FlowGeneratorFamilyV1::WashingtonMesh { .. }
            | FlowGeneratorFamilyV1::WashingtonRandomLevel { .. }
            | FlowGeneratorFamilyV1::WashingtonSquareMesh { .. }
            | FlowGeneratorFamilyV1::WashingtonBasicLine { .. }
            | FlowGeneratorFamilyV1::WashingtonExponentialLine { .. }
            | FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine { .. }
            | FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { .. }
            | FlowGeneratorFamilyV1::WaissiTransitOneWayGrid { .. }
            | FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid { .. }
            | FlowGeneratorFamilyV1::GoldbergMeshCirculation { .. }
            | FlowGeneratorFamilyV1::GotoTorus { .. }
            | FlowGeneratorFamilyV1::NetgenSkeleton { .. }
    )
}

fn generator_tags(family: &FlowGeneratorFamilyV1) -> Vec<String> {
    let mut tags = vec![family_id(family).to_owned()];
    extend_assignment_generator_tags(family, &mut tags);
    extend_transportation_generator_tags(family, &mut tags);
    extend_shape_generator_tags(family, &mut tags);
    extend_specialized_generator_tags(family, &mut tags);
    tags.sort();
    tags.dedup();
    tags
}

fn extend_transportation_generator_tags(family: &FlowGeneratorFamilyV1, tags: &mut Vec<String>) {
    let FlowGeneratorFamilyV1::TransportationTable { shape, .. } = family else {
        return;
    };
    tags.extend([
        "balanced".to_owned(),
        "bipartite".to_owned(),
        "transportation".to_owned(),
    ]);
    tags.push(transportation_shape_id(shape).to_owned());
    match shape {
        TransportationTableShapeV1::SparseFeasible { .. } => tags.push("sparse".to_owned()),
        TransportationTableShapeV1::UnitDegenerate { .. } => {
            tags.extend(["degenerate-basis".to_owned(), "tie-rich".to_owned()]);
        }
        TransportationTableShapeV1::NearTie { .. } => tags.push("tie-rich".to_owned()),
        TransportationTableShapeV1::CutInfeasible { .. } => {
            tags.extend(["cut-witness".to_owned(), "infeasible".to_owned()]);
        }
        TransportationTableShapeV1::DenseUniform { .. }
        | TransportationTableShapeV1::Block { .. }
        | TransportationTableShapeV1::Monge { .. } => {}
    }
}

fn extend_assignment_generator_tags(family: &FlowGeneratorFamilyV1, tags: &mut Vec<String>) {
    if let FlowGeneratorFamilyV1::AssignmentMatrix {
        objective, shape, ..
    } = family
    {
        tags.push("assignment".to_owned());
        tags.push(
            match objective {
                AssignmentObjectiveV1::Minimize => "minimize",
                AssignmentObjectiveV1::Maximize => "maximize",
            }
            .to_owned(),
        );
        tags.push(assignment_shape_id(shape).to_owned());
        if matches!(shape, AssignmentMatrixShapeV1::HallDeficient { .. }) {
            tags.push("hall-deficient".to_owned());
        }
    }
}

#[allow(clippy::too_many_lines)]
fn extend_shape_generator_tags(family: &FlowGeneratorFamilyV1, tags: &mut Vec<String>) {
    if matches!(
        family,
        FlowGeneratorFamilyV1::Grid2d { .. }
            | FlowGeneratorFamilyV1::VisionSegmentationGrid { .. }
            | FlowGeneratorFamilyV1::Grid3d { .. }
            | FlowGeneratorFamilyV1::Torus { .. }
            | FlowGeneratorFamilyV1::RmfgenFrames { .. }
            | FlowGeneratorFamilyV1::GridgenGrid { .. }
            | FlowGeneratorFamilyV1::GridgraphGrid { .. }
            | FlowGeneratorFamilyV1::WashingtonMesh { .. }
            | FlowGeneratorFamilyV1::WashingtonRandomLevel { .. }
            | FlowGeneratorFamilyV1::WashingtonSquareMesh { .. }
            | FlowGeneratorFamilyV1::GotoTorus { .. }
            | FlowGeneratorFamilyV1::WaissiTransitOneWayGrid { .. }
            | FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid { .. }
            | FlowGeneratorFamilyV1::GoldbergMeshCirculation { .. }
    ) {
        tags.push("grid".to_owned());
    }
    if matches!(family, FlowGeneratorFamilyV1::WashingtonMesh { .. }) {
        tags.push("cylindrical".to_owned());
    }
    if matches!(family, FlowGeneratorFamilyV1::VisionSegmentationGrid { .. }) {
        tags.extend([
            "bidirectional".to_owned(),
            "terminal-heavy".to_owned(),
            "vision-graph-cut".to_owned(),
        ]);
    }
    if matches!(
        family,
        FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid { .. }
    ) {
        tags.push("bidirectional".to_owned());
        tags.push("transit-grid".to_owned());
    }
    if matches!(
        family,
        FlowGeneratorFamilyV1::WaissiTransitOneWayGrid { .. }
    ) {
        tags.push("one-way".to_owned());
        tags.push("transit-grid".to_owned());
    }
    if matches!(
        family,
        FlowGeneratorFamilyV1::GoldbergMeshCirculation { .. }
    ) {
        tags.push("bidirectional".to_owned());
        tags.push("circulation".to_owned());
        tags.push("distance-decay".to_owned());
        tags.push("signed-cost".to_owned());
        tags.push("toroidal".to_owned());
    }
    if matches!(
        family,
        FlowGeneratorFamilyV1::WashingtonBasicLine { .. }
            | FlowGeneratorFamilyV1::WashingtonExponentialLine { .. }
            | FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine { .. }
    ) {
        tags.push("line".to_owned());
    }
    if matches!(
        family,
        FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine { .. }
    ) {
        tags.push("bidirectional-offset".to_owned());
    }
    if matches!(
        family,
        FlowGeneratorFamilyV1::LayeredDag { .. }
            | FlowGeneratorFamilyV1::CompleteDag { .. }
            | FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { .. }
            | FlowGeneratorFamilyV1::GloverDenseAcyclicStress { .. }
            | FlowGeneratorFamilyV1::RandomDag { .. }
            | FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { .. }
            | FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { .. }
            | FlowGeneratorFamilyV1::WashingtonCheriyanStress { .. }
            | FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { .. }
            | FlowGeneratorFamilyV1::WashingtonMatching { .. }
            | FlowGeneratorFamilyV1::WashingtonMesh { .. }
            | FlowGeneratorFamilyV1::WashingtonRandomLevel { .. }
            | FlowGeneratorFamilyV1::WashingtonSquareMesh { .. }
            | FlowGeneratorFamilyV1::WashingtonBasicLine { .. }
            | FlowGeneratorFamilyV1::WashingtonExponentialLine { .. }
            | FlowGeneratorFamilyV1::ZadehPhaseChainStress { .. }
    ) {
        tags.push("dag".to_owned());
    }
    if matches!(
        family,
        FlowGeneratorFamilyV1::BipartiteRandom { .. }
            | FlowGeneratorFamilyV1::HallTightBipartite { .. }
            | FlowGeneratorFamilyV1::AssignmentMatrix { .. }
            | FlowGeneratorFamilyV1::TransportationTable { .. }
            | FlowGeneratorFamilyV1::WashingtonMatching { .. }
    ) {
        tags.push("bipartite".to_owned());
    }
    if matches!(family, FlowGeneratorFamilyV1::WashingtonMatching { .. }) {
        tags.push("unit-capacity".to_owned());
        tags.push("unit-network".to_owned());
    }
}

fn extend_specialized_generator_tags(family: &FlowGeneratorFamilyV1, tags: &mut Vec<String>) {
    if matches!(
        family,
        FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { .. }
    ) {
        tags.push("dinic".to_owned());
        tags.push("phase-chain".to_owned());
    }
    if matches!(
        family,
        FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { .. }
    ) {
        tags.push("dag".to_owned());
        tags.push("fifo".to_owned());
        tags.push("push-relabel".to_owned());
    }
    if matches!(
        family,
        FlowGeneratorFamilyV1::WashingtonCheriyanStress { .. }
    ) {
        tags.push("dag".to_owned());
        tags.push("push-relabel".to_owned());
        tags.push("unit-bottleneck".to_owned());
    }
    if matches!(
        family,
        FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { .. }
    ) {
        tags.push("ak".to_owned());
        tags.push("dag".to_owned());
        tags.push("push-relabel".to_owned());
    }
    if matches!(
        family,
        FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { .. }
    ) {
        tags.push("acyclic-dense".to_owned());
        tags.push("dimacs".to_owned());
        tags.push("fully-dense".to_owned());
    }
    if matches!(
        family,
        FlowGeneratorFamilyV1::GloverDenseAcyclicStress { .. }
    ) {
        tags.push("acyclic-dense".to_owned());
        tags.push("fully-dense".to_owned());
        tags.push("glover".to_owned());
    }
}

fn generator_source_id(family: &FlowGeneratorFamilyV1) -> &'static str {
    match family {
        FlowGeneratorFamilyV1::VisionSegmentationGrid { .. } => {
            "boykov-kolmogorov-2004-vision-grid-derived"
        }
        FlowGeneratorFamilyV1::RandomGeometric { .. } => {
            "gilbert-random-plane-networks-1961-derived"
        }
        FlowGeneratorFamilyV1::PreferentialAttachmentDirected { .. } => {
            "barabasi-albert-preferential-attachment-1999-derived"
        }
        FlowGeneratorFamilyV1::WattsStrogatzFixed { .. } => {
            "watts-strogatz-small-world-1998-fixed-count-derived"
        }
        FlowGeneratorFamilyV1::RmfgenFrames { .. } => {
            "goldfarb-grigoriadis-rmfgen-1988-project-rng-derived"
        }
        FlowGeneratorFamilyV1::GridgenGrid { .. } => {
            "lee-orlin-gridgen-1991-project-rng-uniform-derived"
        }
        FlowGeneratorFamilyV1::GridgraphGrid { .. } => {
            "resende-gridgraph-1991-ggraph1-project-rng-derived"
        }
        FlowGeneratorFamilyV1::WashingtonMatching { .. } => {
            "anderson-washington-matching-1991-project-rng-derived"
        }
        FlowGeneratorFamilyV1::WashingtonMesh { .. } => {
            "anderson-washington-mesh-1991-project-rng-derived"
        }
        FlowGeneratorFamilyV1::WashingtonSquareMesh { .. } => {
            "anderson-washington-square-mesh-1991-project-rng-derived"
        }
        FlowGeneratorFamilyV1::WashingtonBasicLine { .. } => {
            "anderson-washington-basic-line-1991-project-rng-derived"
        }
        FlowGeneratorFamilyV1::WashingtonExponentialLine { .. } => {
            "anderson-washington-exponential-line-1991-project-rng-derived"
        }
        FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine { .. } => {
            "anderson-washington-double-exponential-line-1991-project-rng-derived"
        }
        FlowGeneratorFamilyV1::WashingtonRandomLevel { .. } => {
            "anderson-washington-random-level-1991-project-rng-derived"
        }
        FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { .. } => {
            "anderson-washington-dinic-bad-case-1991-derived"
        }
        FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { .. } => {
            "anderson-washington-gold-bad-case-1991-derived"
        }
        FlowGeneratorFamilyV1::WashingtonCheriyanStress { .. } => {
            "anderson-washington-cheriyan-1991-derived"
        }
        FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { .. } => {
            "cherkassky-goldberg-ak-1997-independent-derived"
        }
        FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { .. } => {
            "waissi-setubal-ac-1991-project-rng-derived"
        }
        FlowGeneratorFamilyV1::GloverDenseAcyclicStress { .. } => {
            "waissi-glover-dense-acyclic-1991-derived"
        }
        FlowGeneratorFamilyV1::WaissiTransitOneWayGrid { .. } => {
            "waissi-transit-one-way-grid-1991-project-rng-derived"
        }
        FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid { .. } => {
            "waissi-transit-two-way-grid-1991-project-rng-derived"
        }
        FlowGeneratorFamilyV1::GoldbergMeshCirculation { .. } => {
            "goldberg-mesh1-1991-project-rng-signed-bound-derived"
        }
        FlowGeneratorFamilyV1::GotoTorus { .. } => "goldberg-goto-1991-project-rng-power2-derived",
        FlowGeneratorFamilyV1::NetgenSkeleton { .. } => {
            "klingman-napier-stutz-netgen-1974-project-rng-independent-derived"
        }
        FlowGeneratorFamilyV1::DinicWorstCase { .. } => "waissi-dinic-worst-case-1991",
        FlowGeneratorFamilyV1::AssignmentMatrix { .. } => "flow-assignment-matrix-contract-v1",
        FlowGeneratorFamilyV1::TransportationTable { .. } => {
            "flow-transportation-table-contract-v1"
        }
        FlowGeneratorFamilyV1::ZadehPhaseChainStress { .. } => {
            "zadeh-pathological-max-flow-1973-derived-phase-chain"
        }
        _ => "flow-generator-contract-v1",
    }
}

#[derive(Clone, Debug)]
struct Topology {
    nodes: Vec<FlowNodeV1>,
    edges: Vec<(String, String)>,
    suggested_model: FlowProblemModelV1,
    fixed_capacities: Option<Vec<u64>>,
    fixed_costs: Option<Vec<i64>>,
}

fn build_topology(
    family: &FlowGeneratorFamilyV1,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
    supply_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let fixed =
        build_fixed_attribute_topology(family, topology_rng, capacity_rng, cost_rng, supply_rng)?;
    if let Some(topology) = fixed {
        return Ok(topology);
    }
    build_base_topology(family, topology_rng)
}

#[expect(
    clippy::too_many_lines,
    reason = "the closed family match keeps exhaustive dispatch visible"
)]
fn build_base_topology(
    family: &FlowGeneratorFamilyV1,
    topology_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    match *family {
        FlowGeneratorFamilyV1::Path { nodes } => path_topology(nodes),
        FlowGeneratorFamilyV1::Cycle { nodes } => cycle_topology(nodes),
        FlowGeneratorFamilyV1::ParallelPaths {
            path_count,
            internal_nodes,
        } => parallel_paths_topology(path_count, internal_nodes),
        FlowGeneratorFamilyV1::DiamondChain { stages } => diamond_topology(stages),
        FlowGeneratorFamilyV1::Ladder {
            columns,
            cross_edges,
        } => ladder_topology(columns, cross_edges),
        FlowGeneratorFamilyV1::LayeredDag {
            layers,
            width,
            fanout,
        } => layered_topology(layers, width, fanout),
        FlowGeneratorFamilyV1::CompleteDag { nodes } => complete_dag_topology(nodes),
        FlowGeneratorFamilyV1::Grid2d {
            rows,
            columns,
            diagonals,
        } => grid_topology(rows, columns, diagonals, false),
        FlowGeneratorFamilyV1::VisionSegmentationGrid {
            rows,
            columns,
            eight_neighbor,
        } => vision_segmentation_grid_topology(rows, columns, eight_neighbor),
        FlowGeneratorFamilyV1::Torus { rows, columns } => grid_topology(rows, columns, false, true),
        FlowGeneratorFamilyV1::ErdosRenyiDirected { nodes, edge_count } => {
            erdos_renyi_topology(nodes, edge_count, topology_rng)
        }
        FlowGeneratorFamilyV1::Arborescence { branching, depth } => {
            arborescence_topology(branching, depth)
        }
        FlowGeneratorFamilyV1::StronglyConnected { nodes, extra_edges } => {
            strongly_connected_topology(nodes, extra_edges, topology_rng)
        }
        FlowGeneratorFamilyV1::Grid3d {
            layers,
            rows,
            columns,
        } => grid_3d_topology(layers, rows, columns),
        FlowGeneratorFamilyV1::BipartiteRandom {
            left,
            right,
            edge_count,
        } => bipartite_random_topology(left, right, edge_count, topology_rng),
        FlowGeneratorFamilyV1::RandomGeometric { nodes, radius } => {
            random_geometric_topology(nodes, radius, topology_rng)
        }
        FlowGeneratorFamilyV1::RandomRegularDirected { nodes, degree } => {
            random_regular_directed_topology(nodes, degree, topology_rng)
        }
        FlowGeneratorFamilyV1::PreferentialAttachmentDirected {
            nodes,
            attachment_count,
        } => preferential_attachment_topology(nodes, attachment_count, topology_rng),
        FlowGeneratorFamilyV1::PlanarTriangulated { nodes } => planar_triangulated_topology(nodes),
        FlowGeneratorFamilyV1::MultiSourceSink {
            sources,
            intermediate,
            sinks,
        } => multi_source_sink_topology(sources, intermediate, sinks),
        FlowGeneratorFamilyV1::RandomDag { nodes, edge_count } => {
            random_dag_topology(nodes, edge_count, topology_rng)
        }
        FlowGeneratorFamilyV1::WattsStrogatzFixed {
            nodes,
            neighborhood,
            rewire_count,
        } => watts_strogatz_fixed_topology(nodes, neighborhood, rewire_count, topology_rng),
        FlowGeneratorFamilyV1::ClusteredDirected {
            clusters,
            cluster_size,
            bridge_edges,
        } => clustered_directed_topology(clusters, cluster_size, bridge_edges, topology_rng),
        FlowGeneratorFamilyV1::PlantedBottleneck {
            left,
            right,
            cut_edges,
        } => planted_bottleneck_topology(left, right, cut_edges, topology_rng),
        FlowGeneratorFamilyV1::HallTightBipartite {
            part_size,
            tight_prefix,
        } => hall_tight_bipartite_topology(part_size, tight_prefix),
        FlowGeneratorFamilyV1::AssignmentMatrix { .. }
        | FlowGeneratorFamilyV1::TransportationTable { .. }
        | FlowGeneratorFamilyV1::RmfgenFrames { .. }
        | FlowGeneratorFamilyV1::GridgenGrid { .. }
        | FlowGeneratorFamilyV1::GridgraphGrid { .. }
        | FlowGeneratorFamilyV1::DinicWorstCase { .. }
        | FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { .. }
        | FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { .. }
        | FlowGeneratorFamilyV1::WashingtonCheriyanStress { .. }
        | FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { .. }
        | FlowGeneratorFamilyV1::GloverDenseAcyclicStress { .. }
        | FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { .. }
        | FlowGeneratorFamilyV1::WaissiTransitOneWayGrid { .. }
        | FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid { .. }
        | FlowGeneratorFamilyV1::GoldbergMeshCirculation { .. }
        | FlowGeneratorFamilyV1::WashingtonMatching { .. }
        | FlowGeneratorFamilyV1::WashingtonMesh { .. }
        | FlowGeneratorFamilyV1::WashingtonRandomLevel { .. }
        | FlowGeneratorFamilyV1::WashingtonSquareMesh { .. }
        | FlowGeneratorFamilyV1::WashingtonBasicLine { .. }
        | FlowGeneratorFamilyV1::WashingtonExponentialLine { .. }
        | FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine { .. }
        | FlowGeneratorFamilyV1::ZadehPhaseChainStress { .. }
        | FlowGeneratorFamilyV1::GotoTorus { .. }
        | FlowGeneratorFamilyV1::NetgenSkeleton { .. } => {
            unreachable!("fixed-attribute family handled before base topology dispatch")
        }
    }
}

fn build_deterministic_stress_topology(
    family: &FlowGeneratorFamilyV1,
) -> Result<Option<Topology>, FlowGenerationError> {
    let topology = match *family {
        FlowGeneratorFamilyV1::DinicWorstCase { nodes } => dinic_worst_case_topology(nodes)?,
        FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { nodes } => {
            washington_dinic_stress_topology(nodes)?
        }
        FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { block_size } => {
            washington_goldberg_fifo_stress_topology(block_size)?
        }
        FlowGeneratorFamilyV1::WashingtonCheriyanStress {
            bridge_width,
            gadget_entries,
            chain_length,
        } => washington_cheriyan_stress_topology(bridge_width, gadget_entries, chain_length)?,
        FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { size } => {
            cherkassky_goldberg_ak_stress_topology(size)?
        }
        FlowGeneratorFamilyV1::GloverDenseAcyclicStress { nodes } => {
            glover_dense_acyclic_stress_topology(nodes)?
        }
        FlowGeneratorFamilyV1::ZadehPhaseChainStress { group_size } => {
            zadeh_phase_chain_topology(group_size)?
        }
        _ => return Ok(None),
    };
    Ok(Some(topology))
}

fn build_fixed_attribute_topology(
    family: &FlowGeneratorFamilyV1,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
    supply_rng: &mut RngV1,
) -> Result<Option<Topology>, FlowGenerationError> {
    if let Some(topology) = build_deterministic_stress_topology(family)? {
        return Ok(Some(topology));
    }
    if let Some(topology) = build_native_table_topology(family, topology_rng, cost_rng, supply_rng)?
    {
        return Ok(Some(topology));
    }
    if let Some(topology) =
        build_grid_benchmark_topology(family, topology_rng, capacity_rng, cost_rng, supply_rng)?
    {
        return Ok(Some(topology));
    }
    if let Some(topology) = build_signed_bound_benchmark_topology(family, capacity_rng, cost_rng)? {
        return Ok(Some(topology));
    }
    let topology = match *family {
        FlowGeneratorFamilyV1::RmfgenFrames {
            frame_size,
            depth,
            minimum_capacity,
            maximum_capacity,
        } => rmfgen_frames_topology(
            frame_size,
            depth,
            minimum_capacity,
            maximum_capacity,
            topology_rng,
            capacity_rng,
        )?,
        FlowGeneratorFamilyV1::GotoTorus {
            nodes,
            edge_count,
            maximum_capacity,
            maximum_cost,
        } => goto_torus_topology(
            nodes,
            edge_count,
            maximum_capacity,
            maximum_cost,
            topology_rng,
            capacity_rng,
            cost_rng,
        )?,
        FlowGeneratorFamilyV1::NetgenSkeleton {
            nodes,
            sources,
            sinks,
            edge_count,
            minimum_cost,
            maximum_cost,
            total_supply,
            transshipment_sources,
            transshipment_sinks,
            high_cost_percentage,
            capacitated_percentage,
            minimum_capacity,
            maximum_capacity,
        } => netgen_skeleton_topology(
            nodes,
            sources,
            sinks,
            edge_count,
            minimum_cost,
            maximum_cost,
            total_supply,
            transshipment_sources,
            transshipment_sinks,
            high_cost_percentage,
            capacitated_percentage,
            minimum_capacity,
            maximum_capacity,
            topology_rng,
            capacity_rng,
            cost_rng,
            supply_rng,
        )?,
        FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { nodes } => {
            waissi_setubal_acyclic_dense_topology(nodes, capacity_rng)?
        }
        FlowGeneratorFamilyV1::WaissiTransitOneWayGrid {
            dimension,
            maximum_capacity,
        } => waissi_transit_one_way_grid_topology(
            dimension,
            maximum_capacity,
            topology_rng,
            capacity_rng,
        )?,
        FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid {
            dimension,
            maximum_capacity,
        } => waissi_transit_two_way_grid_topology(dimension, maximum_capacity, capacity_rng)?,
        _ => return Ok(None),
    };
    Ok(Some(topology))
}

fn build_native_table_topology(
    family: &FlowGeneratorFamilyV1,
    topology_rng: &mut RngV1,
    cost_rng: &mut RngV1,
    supply_rng: &mut RngV1,
) -> Result<Option<Topology>, FlowGenerationError> {
    let topology = match family {
        FlowGeneratorFamilyV1::AssignmentMatrix {
            agents,
            tasks,
            objective,
            shape,
        } => {
            assignment_matrix_topology(*agents, *tasks, *objective, shape, topology_rng, cost_rng)?
        }
        FlowGeneratorFamilyV1::TransportationTable {
            origins,
            destinations,
            total_supply,
            shape,
        } => transportation_table_topology(
            *origins,
            *destinations,
            *total_supply,
            shape,
            topology_rng,
            cost_rng,
            supply_rng,
        )?,
        _ => return Ok(None),
    };
    Ok(Some(topology))
}

fn build_signed_bound_benchmark_topology(
    family: &FlowGeneratorFamilyV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<Option<Topology>, FlowGenerationError> {
    let FlowGeneratorFamilyV1::GoldbergMeshCirculation {
        columns,
        rows,
        horizontal_degree,
        vertical_degree,
    } = *family
    else {
        return Ok(None);
    };
    goldberg_mesh_circulation_topology(
        columns,
        rows,
        horizontal_degree,
        vertical_degree,
        capacity_rng,
        cost_rng,
    )
    .map(Some)
}

fn build_grid_benchmark_topology(
    family: &FlowGeneratorFamilyV1,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
    supply_rng: &mut RngV1,
) -> Result<Option<Topology>, FlowGenerationError> {
    if let Some(topology) =
        build_washington_line_benchmark_topology(family, topology_rng, capacity_rng)?
    {
        return Ok(Some(topology));
    }
    let topology = match *family {
        FlowGeneratorFamilyV1::GridgenGrid {
            rows,
            columns,
            terminal_pairs,
            average_degree,
            total_supply,
            two_way,
            minimum_capacity,
            maximum_capacity,
            minimum_cost,
            maximum_cost,
        } => gridgen_grid_topology(
            rows,
            columns,
            terminal_pairs,
            average_degree,
            total_supply,
            two_way,
            minimum_capacity,
            maximum_capacity,
            minimum_cost,
            maximum_cost,
            topology_rng,
            capacity_rng,
            cost_rng,
            supply_rng,
        )?,
        FlowGeneratorFamilyV1::GridgraphGrid {
            rows,
            columns,
            maximum_capacity,
            maximum_cost,
        } => gridgraph_grid_topology(
            rows,
            columns,
            maximum_capacity,
            maximum_cost,
            capacity_rng,
            cost_rng,
        )?,
        FlowGeneratorFamilyV1::WashingtonMatching { part_size, degree } => {
            washington_matching_topology(part_size, degree, topology_rng)?
        }
        FlowGeneratorFamilyV1::WashingtonMesh {
            rows,
            columns,
            maximum_capacity,
        } => washington_mesh_topology(rows, columns, maximum_capacity, capacity_rng)?,
        FlowGeneratorFamilyV1::WashingtonSquareMesh {
            dimension,
            degree,
            maximum_capacity,
        } => washington_square_mesh_topology(dimension, degree, maximum_capacity, capacity_rng)?,
        FlowGeneratorFamilyV1::WashingtonRandomLevel {
            rows,
            columns,
            maximum_capacity,
        } => washington_random_level_topology(
            rows,
            columns,
            maximum_capacity,
            topology_rng,
            capacity_rng,
        )?,
        _ => return Ok(None),
    };
    Ok(Some(topology))
}

fn build_washington_line_benchmark_topology(
    family: &FlowGeneratorFamilyV1,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
) -> Result<Option<Topology>, FlowGenerationError> {
    let (levels, width, degree, profile) = match *family {
        FlowGeneratorFamilyV1::WashingtonBasicLine {
            levels,
            width,
            degree,
        } => (levels, width, degree, WashingtonLineProfile::Basic),
        FlowGeneratorFamilyV1::WashingtonExponentialLine {
            levels,
            width,
            degree,
        } => (levels, width, degree, WashingtonLineProfile::Exponential),
        FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine {
            levels,
            width,
            degree,
        } => (
            levels,
            width,
            degree,
            WashingtonLineProfile::DoubleExponential,
        ),
        _ => return Ok(None),
    };
    washington_line_topology(levels, width, degree, profile, topology_rng, capacity_rng).map(Some)
}

fn path_topology(count: u32) -> Result<Topology, FlowGenerationError> {
    require_range(count, 2, MAX_FLOW_NODES, "path node count")?;
    let nodes = linear_nodes(count, "v")?;
    let edges = (0..count - 1)
        .map(|index| (node_id(index, count), node_id(index + 1, count)))
        .collect();
    Ok(st_topology(nodes, edges, count))
}

fn cycle_topology(count: u32) -> Result<Topology, FlowGenerationError> {
    require_range(count, 2, MAX_FLOW_NODES, "cycle node count")?;
    let nodes = circular_nodes(count)?;
    let edges = (0..count)
        .map(|index| {
            (
                format!("v{index:04}"),
                format!("v{:04}", (index + 1) % count),
            )
        })
        .collect();
    Ok(Topology {
        nodes,
        edges,
        suggested_model: FlowProblemModelV1::Circulation {},
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn parallel_paths_topology(
    path_count: u32,
    internal_nodes: u32,
) -> Result<Topology, FlowGenerationError> {
    require_range(path_count, 1, MAX_FLOW_NODES, "parallel path count")?;
    let node_count = 2_u64
        .checked_add(u64::from(path_count) * u64::from(internal_nodes))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(
        as_usize(node_count)?,
        as_usize(u64::from(path_count) * (u64::from(internal_nodes) + 1))?,
    )?;
    let mut nodes = vec![
        positioned_node("s", 40, 270),
        positioned_node("t", 860, 270),
    ];
    let mut edges = Vec::new();
    for path in 0..path_count {
        let mut previous = "s".to_owned();
        for offset in 0..internal_nodes {
            let id = format!("p{path:03}n{offset:03}");
            let x = interpolate(80, 820, offset + 1, internal_nodes + 1)?;
            let y = interpolate(70, 470, path + 1, path_count + 1)?;
            nodes.push(positioned_node(&id, x, y));
            edges.push((previous, id.clone()));
            previous = id;
        }
        edges.push((previous, "t".to_owned()));
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn diamond_topology(stages: u32) -> Result<Topology, FlowGenerationError> {
    require_range(stages, 1, 3_000, "diamond stages")?;
    let mut nodes = vec![positioned_node("s", 40, 270)];
    let mut edges = Vec::new();
    let mut previous = "s".to_owned();
    for stage in 0..stages {
        let upper = format!("d{stage:04}u");
        let lower = format!("d{stage:04}l");
        let merge = if stage + 1 == stages {
            "t".to_owned()
        } else {
            format!("d{stage:04}m")
        };
        let x = interpolate(70, 810, stage, stages)?;
        nodes.push(positioned_node(&upper, x, 170));
        nodes.push(positioned_node(&lower, x, 370));
        if merge != "t" {
            nodes.push(positioned_node(&merge, x + 60, 270));
        }
        edges.extend([
            (previous.clone(), upper.clone()),
            (previous.clone(), lower.clone()),
            (upper, merge.clone()),
            (lower, merge.clone()),
        ]);
        previous = merge;
    }
    nodes.push(positioned_node("t", 860, 270));
    enforce_graph_limits(nodes.len(), edges.len())?;
    Ok(Topology {
        nodes,
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn ladder_topology(columns: u32, cross_edges: bool) -> Result<Topology, FlowGenerationError> {
    require_range(columns, 2, 5_000, "ladder columns")?;
    let mut nodes = Vec::with_capacity(as_usize(u64::from(columns) * 2)?);
    let mut edges = Vec::new();
    for column in 0..columns {
        let x = interpolate(40, 860, column, columns - 1)?;
        for (row, y) in [(0_u32, 190_i64), (1, 350)] {
            nodes.push(positioned_node(&format!("r{row}c{column:04}"), x, y));
        }
        edges.push((format!("r0c{column:04}"), format!("r1c{column:04}")));
        if column + 1 < columns {
            for row in 0..2 {
                edges.push((
                    format!("r{row}c{column:04}"),
                    format!("r{row}c{:04}", column + 1),
                ));
            }
            if cross_edges {
                edges.push((format!("r0c{column:04}"), format!("r1c{:04}", column + 1)));
                edges.push((format!("r1c{column:04}"), format!("r0c{:04}", column + 1)));
            }
        }
    }
    enforce_graph_limits(nodes.len(), edges.len())?;
    Ok(Topology {
        nodes,
        edges,
        suggested_model: max_flow_model("r0c0000", &format!("r1c{:04}", columns - 1)),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn layered_topology(layers: u32, width: u32, fanout: u32) -> Result<Topology, FlowGenerationError> {
    require_range(layers, 1, 1_000, "layer count")?;
    require_range(width, 1, 1_000, "layer width")?;
    require_range(
        fanout,
        1,
        usize::try_from(width).map_err(|_| FlowGenerationError::SizeLimit)?,
        "fanout",
    )?;
    let node_count = 2_u64 + u64::from(layers) * u64::from(width);
    let edge_count = u64::from(width)
        .checked_mul(2)
        .and_then(|boundary| {
            u64::from(layers.saturating_sub(1))
                .checked_mul(u64::from(width))
                .and_then(|value| value.checked_mul(u64::from(fanout)))
                .and_then(|middle| boundary.checked_add(middle))
        })
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;
    let mut nodes = vec![
        positioned_node("s", 40, 270),
        positioned_node("t", 860, 270),
    ];
    for layer in 0..layers {
        let x = interpolate(90, 810, layer + 1, layers + 1)?;
        for offset in 0..width {
            let y = interpolate(50, 490, offset + 1, width + 1)?;
            nodes.push(positioned_node(&format!("l{layer:03}n{offset:04}"), x, y));
        }
    }
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    for offset in 0..width {
        edges.push(("s".to_owned(), format!("l000n{offset:04}")));
    }
    for layer in 0..layers.saturating_sub(1) {
        for from in 0..width {
            for delta in 0..fanout {
                edges.push((
                    format!("l{layer:03}n{from:04}"),
                    format!("l{:03}n{:04}", layer + 1, (from + delta) % width),
                ));
            }
        }
    }
    for offset in 0..width {
        edges.push((format!("l{:03}n{offset:04}", layers - 1), "t".to_owned()));
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn complete_dag_topology(count: u32) -> Result<Topology, FlowGenerationError> {
    require_range(count, 2, MAX_FLOW_NODES, "complete DAG node count")?;
    let edge_count = u64::from(count)
        .checked_mul(u64::from(count - 1))
        .and_then(|value| value.checked_div(2))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(u64::from(count))?, as_usize(edge_count)?)?;
    let nodes = linear_nodes(count, "v")?;
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    for from in 0..count {
        for to in from + 1..count {
            edges.push((node_id(from, count), node_id(to, count)));
        }
    }
    Ok(st_topology(nodes, edges, count))
}

const DENSE_ACYCLIC_NODE_LIMIT: usize = 200;
const WAISSI_SETUBAL_AC_MAXIMUM_CAPACITY: u64 = 1_000_000;

fn waissi_setubal_acyclic_dense_topology(
    count: u32,
    capacity_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(
        count,
        2,
        DENSE_ACYCLIC_NODE_LIMIT,
        "Waissi-Setubal acyclic-dense node count",
    )?;
    let edge_count = u64::from(count)
        .checked_mul(u64::from(count - 1))
        .and_then(|value| value.checked_div(2))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(u64::from(count))?, as_usize(edge_count)?)?;
    let nodes = linear_nodes(count, "v")?;
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(edge_count)?);
    for from in 0..count {
        for to in from + 1..count {
            edges.push((node_id(from, count), node_id(to, count)));
            capacities.push(sample_uniform_u64(
                capacity_rng,
                1,
                WAISSI_SETUBAL_AC_MAXIMUM_CAPACITY,
            )?);
        }
    }
    debug_assert_eq!(edges.len(), as_usize(edge_count)?);
    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; edges.len()]),
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(capacities),
    })
}

fn glover_dense_chain_capacity(from: u32, count: u32) -> Result<u64, FlowGenerationError> {
    let doubled_one_based_tail = i64::from(from + 1)
        .checked_mul(2)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let offset = doubled_one_based_tail
        .checked_sub(i64::from(count))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let squared = offset
        .checked_mul(offset)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    squared
        .checked_div(4)
        .and_then(|value| value.checked_add(1))
        .ok_or(FlowGenerationError::ArithmeticOverflow)
}

fn glover_dense_acyclic_stress_topology(count: u32) -> Result<Topology, FlowGenerationError> {
    require_range(
        count,
        2,
        DENSE_ACYCLIC_NODE_LIMIT,
        "Glover dense acyclic node count",
    )?;
    let edge_count = u64::from(count)
        .checked_mul(u64::from(count - 1))
        .and_then(|value| value.checked_div(2))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(u64::from(count))?, as_usize(edge_count)?)?;
    let nodes = linear_nodes(count, "v")?;
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(edge_count)?);
    for from in 0..count {
        for to in from + 1..count {
            edges.push((node_id(from, count), node_id(to, count)));
            capacities.push(if to == from + 1 {
                glover_dense_chain_capacity(from, count)?
            } else {
                1
            });
        }
    }
    debug_assert_eq!(edges.len(), as_usize(edge_count)?);
    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; edges.len()]),
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(capacities),
    })
}

const WAISSI_TRANSIT_DIMENSION_LIMIT: usize = 44;
const WAISSI_TRANSIT_MAXIMUM_CAPACITY: usize = 1_000_000_000;

#[derive(Clone, Copy)]
struct WaissiTransitCounts {
    node_count: u64,
    edge_count: u64,
}

fn validate_waissi_transit_config(
    dimension: u32,
    maximum_capacity: u32,
    arcs_per_grid_node: u64,
    family: &'static str,
) -> Result<WaissiTransitCounts, FlowGenerationError> {
    require_range(dimension, 2, WAISSI_TRANSIT_DIMENSION_LIMIT, family)?;
    require_range(
        maximum_capacity,
        1,
        WAISSI_TRANSIT_MAXIMUM_CAPACITY,
        "Waissi transit grid maximum capacity",
    )?;
    let grid_nodes = u64::from(dimension)
        .checked_mul(u64::from(dimension))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let node_count = grid_nodes
        .checked_add(2)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edge_count = grid_nodes
        .checked_mul(arcs_per_grid_node)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;
    Ok(WaissiTransitCounts {
        node_count,
        edge_count,
    })
}

fn waissi_transit_nodes(
    dimension: u32,
    node_count: u64,
) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    let mut nodes = Vec::with_capacity(as_usize(node_count)?);
    nodes.push(positioned_node("s", 32, 270));
    for column in 0..dimension {
        for row in 0..dimension {
            nodes.push(positioned_node(
                &waissi_transit_id(row, column),
                interpolate(188, 712, column, dimension - 1)?,
                interpolate(68, 472, row, dimension - 1)?,
            ));
        }
    }
    nodes.push(positioned_node("t", 868, 270));
    if nodes.len() != as_usize(node_count)? {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(nodes)
}

fn push_waissi_transit_pair(
    endpoints: &mut Vec<(String, String)>,
    capacities: &mut Vec<u64>,
    from: String,
    to: String,
    maximum_capacity: u64,
    capacity_rng: &mut RngV1,
) -> Result<(), FlowGenerationError> {
    endpoints.push((from.clone(), to.clone()));
    capacities.push(sample_uniform_u64(capacity_rng, 1, maximum_capacity)?);
    endpoints.push((to, from));
    capacities.push(sample_uniform_u64(capacity_rng, 1, maximum_capacity)?);
    Ok(())
}

fn push_waissi_transit_arc(
    endpoints: &mut Vec<(String, String)>,
    capacities: &mut Vec<u64>,
    from: String,
    to: String,
    maximum_capacity: u64,
    capacity_rng: &mut RngV1,
) -> Result<(), FlowGenerationError> {
    endpoints.push((from, to));
    capacities.push(sample_uniform_u64(capacity_rng, 1, maximum_capacity)?);
    Ok(())
}

fn waissi_street_is_reversed(
    draw: u64,
    maximum_capacity: u64,
) -> Result<bool, FlowGenerationError> {
    Ok(draw
        .checked_mul(2)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?
        < maximum_capacity)
}

fn waissi_transit_one_way_grid_topology(
    dimension: u32,
    maximum_capacity: u32,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let counts = validate_waissi_transit_config(
        dimension,
        maximum_capacity,
        2,
        "Waissi one-way transit grid dimension",
    )?;
    let nodes = waissi_transit_nodes(dimension, counts.node_count)?;
    let mut endpoints = Vec::with_capacity(as_usize(counts.edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(counts.edge_count)?);
    let maximum_capacity = u64::from(maximum_capacity);
    for row in 0..dimension {
        push_waissi_transit_arc(
            &mut endpoints,
            &mut capacities,
            "s".to_owned(),
            waissi_transit_id(row, 0),
            maximum_capacity,
            capacity_rng,
        )?;
    }
    for column in 0..dimension {
        for row in 0..dimension {
            let mut push_street = |first: String, second: String| {
                let draw = topology_rng.bounded_u64(maximum_capacity)?;
                let (from, to) = if waissi_street_is_reversed(draw, maximum_capacity)? {
                    (second, first)
                } else {
                    (first, second)
                };
                push_waissi_transit_arc(
                    &mut endpoints,
                    &mut capacities,
                    from,
                    to,
                    maximum_capacity,
                    capacity_rng,
                )
            };
            if row + 1 < dimension {
                push_street(
                    waissi_transit_id(row, column),
                    waissi_transit_id(row + 1, column),
                )?;
            }
            if column + 1 < dimension {
                push_street(
                    waissi_transit_id(row, column),
                    waissi_transit_id(row, column + 1),
                )?;
            }
        }
    }
    for row in 0..dimension {
        push_waissi_transit_arc(
            &mut endpoints,
            &mut capacities,
            waissi_transit_id(row, dimension - 1),
            "t".to_owned(),
            maximum_capacity,
            capacity_rng,
        )?;
    }
    if endpoints.len() != as_usize(counts.edge_count)? {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; endpoints.len()]),
        edges: endpoints,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(capacities),
    })
}

fn waissi_transit_two_way_grid_topology(
    dimension: u32,
    maximum_capacity: u32,
    capacity_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let counts = validate_waissi_transit_config(
        dimension,
        maximum_capacity,
        4,
        "Waissi two-way transit grid dimension",
    )?;
    let nodes = waissi_transit_nodes(dimension, counts.node_count)?;

    let mut endpoints = Vec::with_capacity(as_usize(counts.edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(counts.edge_count)?);
    let maximum_capacity = u64::from(maximum_capacity);
    for row in 0..dimension {
        push_waissi_transit_pair(
            &mut endpoints,
            &mut capacities,
            "s".to_owned(),
            waissi_transit_id(row, 0),
            maximum_capacity,
            capacity_rng,
        )?;
    }
    for column in 0..dimension {
        for row in 0..dimension {
            if row + 1 < dimension {
                push_waissi_transit_pair(
                    &mut endpoints,
                    &mut capacities,
                    waissi_transit_id(row, column),
                    waissi_transit_id(row + 1, column),
                    maximum_capacity,
                    capacity_rng,
                )?;
            }
            if column + 1 < dimension {
                push_waissi_transit_pair(
                    &mut endpoints,
                    &mut capacities,
                    waissi_transit_id(row, column),
                    waissi_transit_id(row, column + 1),
                    maximum_capacity,
                    capacity_rng,
                )?;
            }
        }
    }
    for row in 0..dimension {
        push_waissi_transit_pair(
            &mut endpoints,
            &mut capacities,
            waissi_transit_id(row, dimension - 1),
            "t".to_owned(),
            maximum_capacity,
            capacity_rng,
        )?;
    }
    if endpoints.len() != as_usize(counts.edge_count)? {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; endpoints.len()]),
        edges: endpoints,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(capacities),
    })
}

const GOLDBERG_MESH_DIMENSION_LIMIT: usize = 32;
const GOLDBERG_MESH_DEGREE_LIMIT: usize = 8;
const GOLDBERG_MESH_MAXIMUM_CAPACITY: u64 = 1_000;
const GOLDBERG_MESH_MAXIMUM_COST_MAGNITUDE: i64 = 999;

fn goldberg_mesh_circulation_topology(
    columns: u32,
    rows: u32,
    horizontal_degree: u32,
    vertical_degree: u32,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(
        columns,
        3,
        GOLDBERG_MESH_DIMENSION_LIMIT,
        "Goldberg mesh columns",
    )?;
    require_range(rows, 3, GOLDBERG_MESH_DIMENSION_LIMIT, "Goldberg mesh rows")?;
    validate_goldberg_mesh_degree(
        horizontal_degree,
        columns,
        "Goldberg mesh horizontal degree",
    )?;
    validate_goldberg_mesh_degree(vertical_degree, rows, "Goldberg mesh vertical degree")?;
    if horizontal_degree == 0 && vertical_degree == 0 {
        return Err(FlowGenerationError::Invalid("Goldberg mesh degree"));
    }

    let node_count = u64::from(rows)
        .checked_mul(u64::from(columns))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let logical_edge_count = node_count
        .checked_mul(u64::from(horizontal_degree) + u64::from(vertical_degree))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edge_count = logical_edge_count
        .checked_mul(2)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;

    let mut nodes = Vec::with_capacity(as_usize(node_count)?);
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(edge_count)?);
    let mut costs = Vec::with_capacity(as_usize(edge_count)?);
    for row in 0..rows {
        for column in 0..columns {
            let from = goldberg_mesh_id(row, column);
            nodes.push(positioned_node(
                &from,
                interpolate(72, 828, column, columns - 1)?,
                interpolate(58, 482, row, rows - 1)?,
            ));
            for distance in 1..=horizontal_degree {
                push_goldberg_mesh_signed_link(
                    &mut edges,
                    &mut capacities,
                    &mut costs,
                    from.clone(),
                    goldberg_mesh_id(row, (column + distance) % columns),
                    distance,
                    capacity_rng,
                    cost_rng,
                )?;
            }
            for distance in 1..=vertical_degree {
                push_goldberg_mesh_signed_link(
                    &mut edges,
                    &mut capacities,
                    &mut costs,
                    from.clone(),
                    goldberg_mesh_id((row + distance) % rows, column),
                    distance,
                    capacity_rng,
                    cost_rng,
                )?;
            }
        }
    }
    if nodes.len() != as_usize(node_count)?
        || edges.len() != as_usize(edge_count)?
        || capacities.len() != edges.len()
        || costs.len() != edges.len()
    {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: FlowProblemModelV1::Circulation {},
        fixed_capacities: Some(capacities),
        fixed_costs: Some(costs),
    })
}

fn validate_goldberg_mesh_degree(
    degree: u32,
    period: u32,
    field: &'static str,
) -> Result<(), FlowGenerationError> {
    let maximum = ((period - 1) / 2).min(
        u32::try_from(GOLDBERG_MESH_DEGREE_LIMIT)
            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
    );
    require_range(
        degree,
        0,
        usize::try_from(maximum).map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
        field,
    )
}

#[allow(clippy::too_many_arguments)]
fn push_goldberg_mesh_signed_link(
    edges: &mut Vec<(String, String)>,
    capacities: &mut Vec<u64>,
    costs: &mut Vec<i64>,
    from: String,
    to: String,
    distance: u32,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<(), FlowGenerationError> {
    let reverse_capacity = goldberg_mesh_distance_capacity(capacity_rng, distance)?;
    let forward_capacity = goldberg_mesh_distance_capacity(capacity_rng, distance)?;
    let cost = goldberg_mesh_signed_cost(cost_rng)?;
    edges.push((from.clone(), to.clone()));
    capacities.push(forward_capacity);
    costs.push(cost);
    edges.push((to, from));
    capacities.push(reverse_capacity);
    costs.push(-cost);
    Ok(())
}

fn goldberg_mesh_distance_capacity(
    rng: &mut RngV1,
    distance: u32,
) -> Result<u64, FlowGenerationError> {
    if distance == 0 {
        return Err(FlowGenerationError::Canonicalization);
    }
    let divisor = 1_u64
        .checked_shl(distance - 1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    Ok(sample_uniform_u64(rng, 1, GOLDBERG_MESH_MAXIMUM_CAPACITY)? / divisor)
}

fn goldberg_mesh_signed_cost(rng: &mut RngV1) -> Result<i64, FlowGenerationError> {
    let sign = if rng.bounded_u64(2)? == 0 { -1 } else { 1 };
    let magnitude = sample_uniform_i64(rng, 0, GOLDBERG_MESH_MAXIMUM_COST_MAGNITUDE)?;
    Ok(sign * magnitude)
}

fn goldberg_mesh_id(row: u32, column: u32) -> String {
    format!("m{row:04}c{column:04}")
}

fn grid_topology(
    rows: u32,
    columns: u32,
    diagonals: bool,
    torus: bool,
) -> Result<Topology, FlowGenerationError> {
    let minimum = if torus { 3 } else { 1 };
    require_range(rows, minimum, MAX_FLOW_NODES, "grid rows")?;
    require_range(columns, minimum, MAX_FLOW_NODES, "grid columns")?;
    let node_count = u64::from(rows)
        .checked_mul(u64::from(columns))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if node_count < 2 {
        return Err(FlowGenerationError::Invalid("grid needs two nodes"));
    }
    enforce_graph_limits(as_usize(node_count)?, 0)?;
    let mut nodes = Vec::with_capacity(as_usize(node_count)?);
    let mut edges = Vec::new();
    for row in 0..rows {
        for column in 0..columns {
            let id = grid_id(row, column);
            let x = interpolate(40, 860, column, columns.saturating_sub(1).max(1))?;
            let y = interpolate(40, 500, row, rows.saturating_sub(1).max(1))?;
            nodes.push(positioned_node(&id, x, y));
            if torus || column + 1 < columns {
                edges.push((id.clone(), grid_id(row, (column + 1) % columns)));
            }
            if torus || row + 1 < rows {
                edges.push((id.clone(), grid_id((row + 1) % rows, column)));
            }
            if diagonals && row + 1 < rows && column + 1 < columns {
                edges.push((id, grid_id(row + 1, column + 1)));
            }
        }
    }
    enforce_graph_limits(nodes.len(), edges.len())?;
    let sink = grid_id(rows - 1, columns - 1);
    Ok(Topology {
        nodes,
        edges,
        suggested_model: if torus {
            FlowProblemModelV1::Circulation {}
        } else {
            max_flow_model(&grid_id(0, 0), &sink)
        },
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn vision_segmentation_grid_topology(
    rows: u32,
    columns: u32,
    eight_neighbor: bool,
) -> Result<Topology, FlowGenerationError> {
    require_range(rows, 1, BOYKOV_KOLMOGOROV_MAX_NODES, "vision grid rows")?;
    require_range(
        columns,
        1,
        BOYKOV_KOLMOGOROV_MAX_NODES,
        "vision grid columns",
    )?;
    let rows_u64 = u64::from(rows);
    let columns_u64 = u64::from(columns);
    let pixels = rows_u64
        .checked_mul(columns_u64)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let nodes = pixels
        .checked_add(2)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let horizontal = rows_u64
        .checked_mul(columns_u64.saturating_sub(1))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let vertical = rows_u64
        .saturating_sub(1)
        .checked_mul(columns_u64)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let diagonal = if eight_neighbor {
        2_u64
            .checked_mul(rows_u64.saturating_sub(1))
            .and_then(|value| value.checked_mul(columns_u64.saturating_sub(1)))
            .ok_or(FlowGenerationError::ArithmeticOverflow)?
    } else {
        0
    };
    let neighbor_pairs = horizontal
        .checked_add(vertical)
        .and_then(|value| value.checked_add(diagonal))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edges = 2_u64
        .checked_mul(pixels)
        .and_then(|value| value.checked_add(2_u64.checked_mul(neighbor_pairs)?))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if nodes > BOYKOV_KOLMOGOROV_MAX_NODES as u64 || edges > BOYKOV_KOLMOGOROV_MAX_EDGES as u64 {
        return Err(FlowGenerationError::SizeLimit);
    }
    enforce_graph_limits(as_usize(nodes)?, as_usize(edges)?)?;

    let mut generated_nodes = Vec::with_capacity(as_usize(nodes)?);
    generated_nodes.push(positioned_node("s", 40, 270));
    let mut generated_edges = Vec::with_capacity(as_usize(edges)?);
    for row in 0..rows {
        for column in 0..columns {
            let id = vision_grid_id(row, column);
            let x = interpolate(150, 750, column, columns.saturating_sub(1).max(1))?;
            let y = interpolate(40, 500, row, rows.saturating_sub(1).max(1))?;
            generated_nodes.push(positioned_node(&id, x, y));
            generated_edges.push(("s".to_owned(), id.clone()));
            generated_edges.push((id.clone(), "t".to_owned()));
            for neighbor in [
                column
                    .checked_add(1)
                    .filter(|&next| next < columns)
                    .map(|next| vision_grid_id(row, next)),
                row.checked_add(1)
                    .filter(|&next| next < rows)
                    .map(|next| vision_grid_id(next, column)),
                (eight_neighbor && row + 1 < rows && column + 1 < columns)
                    .then(|| vision_grid_id(row + 1, column + 1)),
                (eight_neighbor && row + 1 < rows && column > 0)
                    .then(|| vision_grid_id(row + 1, column - 1)),
            ]
            .into_iter()
            .flatten()
            {
                generated_edges.push((id.clone(), neighbor.clone()));
                generated_edges.push((neighbor, id.clone()));
            }
        }
    }
    generated_nodes.push(positioned_node("t", 860, 270));
    if generated_nodes.len() != as_usize(nodes)? || generated_edges.len() != as_usize(edges)? {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(Topology {
        nodes: generated_nodes,
        edges: generated_edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn rmfgen_frames_topology(
    frame_size: u32,
    depth: u32,
    minimum_capacity: u32,
    maximum_capacity: u32,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(frame_size, 2, 1_000, "RMFGEN frame size")?;
    require_range(depth, 1, 1_000, "RMFGEN depth")?;
    if minimum_capacity > maximum_capacity || maximum_capacity > 1_000 {
        return Err(FlowGenerationError::Invalid("RMFGEN capacity interval"));
    }
    let minimum_capacity = u64::from(minimum_capacity);
    let maximum_capacity = u64::from(maximum_capacity);

    let a = u64::from(frame_size);
    let b = u64::from(depth);
    let frame_nodes = a
        .checked_mul(a)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let node_count = frame_nodes
        .checked_mul(b)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let in_frame_edges = 4_u64
        .checked_mul(a)
        .and_then(|value| value.checked_mul(a - 1))
        .and_then(|value| value.checked_mul(b))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let inter_frame_edges = frame_nodes
        .checked_mul(b - 1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edge_count = in_frame_edges
        .checked_add(inter_frame_edges)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;

    let in_frame_capacity = maximum_capacity
        .checked_mul(frame_nodes)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let nodes = rmfgen_nodes(frame_size, depth, as_usize(node_count)?)?;

    let frame_node_count =
        u32::try_from(frame_nodes).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
    let mut permutation = (0..frame_node_count).collect::<Vec<_>>();
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(edge_count)?);
    for frame in 0..depth {
        if frame + 1 < depth {
            shuffle_indices(&mut permutation, topology_rng)?;
        }
        for row in 0..frame_size {
            for column in 0..frame_size {
                let from = rmfgen_id(frame, row, column);
                if frame + 1 < depth {
                    let ordinal = row
                        .checked_mul(frame_size)
                        .and_then(|value| value.checked_add(column))
                        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
                    let target = permutation[as_usize(u64::from(ordinal))?];
                    edges.push((
                        from.clone(),
                        rmfgen_id(frame + 1, target / frame_size, target % frame_size),
                    ));
                    capacities.push(sample_uniform_u64(
                        capacity_rng,
                        minimum_capacity,
                        maximum_capacity,
                    )?);
                }
                for (neighbor_row, neighbor_column) in [
                    row.checked_add(1)
                        .filter(|&value| value < frame_size)
                        .map(|value| (value, column)),
                    row.checked_sub(1).map(|value| (value, column)),
                    column
                        .checked_add(1)
                        .filter(|&value| value < frame_size)
                        .map(|value| (row, value)),
                    column.checked_sub(1).map(|value| (row, value)),
                ]
                .into_iter()
                .flatten()
                {
                    edges.push((
                        from.clone(),
                        rmfgen_id(frame, neighbor_row, neighbor_column),
                    ));
                    capacities.push(in_frame_capacity);
                }
            }
        }
    }
    if edges.len() != as_usize(edge_count)? || capacities.len() != edges.len() {
        return Err(FlowGenerationError::Canonicalization);
    }
    let source = rmfgen_id(0, 0, 0);
    let sink = rmfgen_id(depth - 1, frame_size - 1, frame_size - 1);
    Ok(Topology {
        nodes,
        edges,
        suggested_model: max_flow_model(&source, &sink),
        fixed_capacities: Some(capacities),
        fixed_costs: Some(vec![0; as_usize(edge_count)?]),
    })
}

fn rmfgen_nodes(
    frame_size: u32,
    depth: u32,
    node_count: usize,
) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    let layout_columns = depth
        .checked_mul(frame_size)
        .and_then(|value| value.checked_add(depth - 1))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let mut nodes = Vec::with_capacity(node_count);
    let frame_stride = frame_size
        .checked_add(1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    for frame in 0..depth {
        for row in 0..frame_size {
            for column in 0..frame_size {
                let axis = frame
                    .checked_mul(frame_stride)
                    .and_then(|value| value.checked_add(column))
                    .ok_or(FlowGenerationError::ArithmeticOverflow)?;
                nodes.push(positioned_node(
                    &rmfgen_id(frame, row, column),
                    interpolate(40, 860, axis, layout_columns - 1)?,
                    interpolate(500, 40, row, frame_size - 1)?,
                ));
            }
        }
    }
    Ok(nodes)
}

#[allow(clippy::too_many_arguments)]
fn gridgen_grid_topology(
    rows: u32,
    columns: u32,
    terminal_pairs: u32,
    average_degree: u32,
    total_supply: u32,
    two_way: bool,
    minimum_capacity: u32,
    maximum_capacity: u32,
    minimum_cost: u32,
    maximum_cost: u32,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
    supply_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let config = GridgenConfig {
        rows,
        columns,
        terminal_pairs,
        average_degree,
        total_supply,
        two_way,
        minimum_capacity,
        maximum_capacity,
        minimum_cost,
        maximum_cost,
    };
    let counts = validate_gridgen_config(config)?;
    let materialized_nodes = gridgen_nodes_and_terminals(config, counts, topology_rng, supply_rng)?;
    if materialized_nodes.nodes.len() != as_usize(counts.node_count)? {
        return Err(FlowGenerationError::Canonicalization);
    }
    let materialized_edges = gridgen_edges(
        config,
        counts,
        &materialized_nodes.source_indices,
        &materialized_nodes.sink_indices,
        topology_rng,
        capacity_rng,
        cost_rng,
    )?;
    Ok(Topology {
        nodes: materialized_nodes.nodes,
        edges: materialized_edges.edges,
        suggested_model: FlowProblemModelV1::Transshipment {},
        fixed_capacities: Some(materialized_edges.capacities),
        fixed_costs: Some(materialized_edges.costs),
    })
}

#[derive(Clone, Copy, Debug)]
struct GridgenConfig {
    rows: u32,
    columns: u32,
    terminal_pairs: u32,
    average_degree: u32,
    total_supply: u32,
    two_way: bool,
    minimum_capacity: u32,
    maximum_capacity: u32,
    minimum_cost: u32,
    maximum_cost: u32,
}

#[derive(Clone, Copy, Debug)]
struct GridgenCounts {
    grid_nodes: u64,
    node_count: u64,
    edge_count: u64,
}

#[derive(Debug)]
struct GridgenNodes {
    nodes: Vec<FlowNodeV1>,
    source_indices: Vec<u32>,
    sink_indices: Vec<u32>,
}

#[derive(Debug)]
struct GridgenEdges {
    edges: Vec<(String, String)>,
    capacities: Vec<u64>,
    costs: Vec<i64>,
}

fn validate_gridgen_config(config: GridgenConfig) -> Result<GridgenCounts, FlowGenerationError> {
    require_range(config.rows, 2, 1_000, "GRIDGEN rows")?;
    require_range(config.columns, 2, 1_000, "GRIDGEN columns")?;
    if config.minimum_capacity > config.maximum_capacity || config.maximum_capacity > 1_000_000_000
    {
        return Err(FlowGenerationError::Invalid("GRIDGEN capacity interval"));
    }
    if config.minimum_cost > config.maximum_cost || config.maximum_cost > 1_000_000_000 {
        return Err(FlowGenerationError::Invalid("GRIDGEN cost interval"));
    }
    let grid_nodes = u64::from(config.rows)
        .checked_mul(u64::from(config.columns))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if config.terminal_pairs == 0 || u64::from(config.terminal_pairs) * 2 > grid_nodes {
        return Err(FlowGenerationError::Invalid("GRIDGEN terminal pairs"));
    }
    if config.average_degree == 0 || u64::from(config.average_degree) > grid_nodes {
        return Err(FlowGenerationError::Invalid("GRIDGEN average degree"));
    }
    if config.total_supply < config.terminal_pairs || config.total_supply > 1_000_000_000 {
        return Err(FlowGenerationError::Invalid("GRIDGEN total supply"));
    }

    let node_count = grid_nodes
        .checked_add(1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let lattice_links = u64::from(config.rows)
        .checked_mul(u64::from(config.columns - 1))
        .and_then(|horizontal| {
            u64::from(config.rows - 1)
                .checked_mul(u64::from(config.columns))
                .and_then(|vertical| horizontal.checked_add(vertical))
        })
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let direction_count = if config.two_way { 2_u64 } else { 1_u64 };
    let basic_edges = lattice_links
        .checked_mul(direction_count)
        .and_then(|value| value.checked_add(u64::from(config.terminal_pairs) * 2))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let requested_edges = node_count
        .checked_mul(u64::from(config.average_degree))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edge_count = basic_edges.max(requested_edges);
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;
    Ok(GridgenCounts {
        grid_nodes,
        node_count,
        edge_count,
    })
}

fn gridgen_nodes_and_terminals(
    config: GridgenConfig,
    counts: GridgenCounts,
    topology_rng: &mut RngV1,
    supply_rng: &mut RngV1,
) -> Result<GridgenNodes, FlowGenerationError> {
    let grid_node_count =
        u32::try_from(counts.grid_nodes).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
    let mut terminal_order = (0..grid_node_count).collect::<Vec<_>>();
    shuffle_indices(&mut terminal_order, topology_rng)?;
    let pair_count =
        usize::try_from(config.terminal_pairs).map_err(|_| FlowGenerationError::SizeLimit)?;
    let source_indices = terminal_order[..pair_count].to_vec();
    let sink_indices = terminal_order[pair_count..pair_count * 2].to_vec();
    let source_supplies =
        positive_composition(config.total_supply, config.terminal_pairs, supply_rng)?;
    let sink_demands =
        positive_composition(config.total_supply, config.terminal_pairs, supply_rng)?;
    let mut balances = BTreeMap::new();
    for (&index, &supply) in source_indices.iter().zip(&source_supplies) {
        balances.insert(index, i64::from(supply));
    }
    for (&index, &demand) in sink_indices.iter().zip(&sink_demands) {
        balances.insert(index, -i64::from(demand));
    }

    let mut nodes = Vec::with_capacity(as_usize(counts.node_count)?);
    for row in 0..config.rows {
        for column in 0..config.columns {
            let index = row * config.columns + column;
            nodes.push(FlowNodeV1 {
                id: gridgen_id(row, column),
                supply: balances.get(&index).copied().unwrap_or(0).to_string(),
                position: Some(FlowPositionV1 {
                    x: interpolate(40, 760, column, config.columns - 1)?.to_string(),
                    y: interpolate(40, 500, row, config.rows - 1)?.to_string(),
                }),
            });
        }
    }
    nodes.push(positioned_node("super", 880, 270));
    Ok(GridgenNodes {
        nodes,
        source_indices,
        sink_indices,
    })
}

#[allow(clippy::too_many_arguments)]
fn push_gridgen_ordinary_edge(
    edges: &mut Vec<(String, String)>,
    capacities: &mut Vec<u64>,
    costs: &mut Vec<i64>,
    from: String,
    to: String,
    config: GridgenConfig,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<(), FlowGenerationError> {
    edges.push((from, to));
    capacities.push(sample_uniform_u64(
        capacity_rng,
        u64::from(config.minimum_capacity),
        u64::from(config.maximum_capacity),
    )?);
    costs.push(sample_uniform_i64(
        cost_rng,
        i64::from(config.minimum_cost),
        i64::from(config.maximum_cost),
    )?);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn gridgen_edges(
    config: GridgenConfig,
    counts: GridgenCounts,
    source_indices: &[u32],
    sink_indices: &[u32],
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<GridgenEdges, FlowGenerationError> {
    let edge_capacity = as_usize(counts.edge_count)?;
    let mut edges = Vec::with_capacity(edge_capacity);
    let mut capacities = Vec::with_capacity(edge_capacity);
    let mut costs = Vec::with_capacity(edge_capacity);
    gridgen_lattice_edges(
        config,
        &mut edges,
        &mut capacities,
        &mut costs,
        capacity_rng,
        cost_rng,
    )?;
    gridgen_super_edges(
        config,
        source_indices,
        sink_indices,
        &mut edges,
        &mut capacities,
        &mut costs,
    )?;
    gridgen_extra_edges(
        config,
        counts,
        &mut edges,
        &mut capacities,
        &mut costs,
        topology_rng,
        capacity_rng,
        cost_rng,
    )?;
    if edges.len() != edge_capacity
        || capacities.len() != edge_capacity
        || costs.len() != edge_capacity
    {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(GridgenEdges {
        edges,
        capacities,
        costs,
    })
}

#[allow(clippy::too_many_arguments)]
fn gridgen_lattice_edges(
    config: GridgenConfig,
    edges: &mut Vec<(String, String)>,
    capacities: &mut Vec<u64>,
    costs: &mut Vec<i64>,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<(), FlowGenerationError> {
    for row in 0..config.rows {
        for column in 0..config.columns - 1 {
            let left = gridgen_id(row, column);
            let right = gridgen_id(row, column + 1);
            if config.two_way {
                push_gridgen_ordinary_edge(
                    edges,
                    capacities,
                    costs,
                    left.clone(),
                    right.clone(),
                    config,
                    capacity_rng,
                    cost_rng,
                )?;
                push_gridgen_ordinary_edge(
                    edges,
                    capacities,
                    costs,
                    right,
                    left,
                    config,
                    capacity_rng,
                    cost_rng,
                )?;
            } else {
                let (from, to) = if row % 2 == 0 {
                    (left, right)
                } else {
                    (right, left)
                };
                push_gridgen_ordinary_edge(
                    edges,
                    capacities,
                    costs,
                    from,
                    to,
                    config,
                    capacity_rng,
                    cost_rng,
                )?;
            }
        }
    }
    for column in 0..config.columns {
        for row in 0..config.rows - 1 {
            let upper = gridgen_id(row, column);
            let lower = gridgen_id(row + 1, column);
            if config.two_way {
                push_gridgen_ordinary_edge(
                    edges,
                    capacities,
                    costs,
                    upper.clone(),
                    lower.clone(),
                    config,
                    capacity_rng,
                    cost_rng,
                )?;
                push_gridgen_ordinary_edge(
                    edges,
                    capacities,
                    costs,
                    lower,
                    upper,
                    config,
                    capacity_rng,
                    cost_rng,
                )?;
            } else {
                let (from, to) = if column % 2 == 0 {
                    (upper, lower)
                } else {
                    (lower, upper)
                };
                push_gridgen_ordinary_edge(
                    edges,
                    capacities,
                    costs,
                    from,
                    to,
                    config,
                    capacity_rng,
                    cost_rng,
                )?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn gridgen_super_edges(
    config: GridgenConfig,
    source_indices: &[u32],
    sink_indices: &[u32],
    edges: &mut Vec<(String, String)>,
    capacities: &mut Vec<u64>,
    costs: &mut Vec<i64>,
) -> Result<(), FlowGenerationError> {
    let high_cost = costs
        .iter()
        .copied()
        .max()
        .unwrap_or(0)
        .checked_mul(2)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    for &index in source_indices {
        edges.push((
            gridgen_id(index / config.columns, index % config.columns),
            "super".to_owned(),
        ));
        capacities.push(u64::from(config.total_supply));
        costs.push(high_cost);
    }
    for &index in sink_indices {
        edges.push((
            "super".to_owned(),
            gridgen_id(index / config.columns, index % config.columns),
        ));
        capacities.push(u64::from(config.total_supply));
        costs.push(high_cost);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn gridgen_extra_edges(
    config: GridgenConfig,
    counts: GridgenCounts,
    edges: &mut Vec<(String, String)>,
    capacities: &mut Vec<u64>,
    costs: &mut Vec<i64>,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<(), FlowGenerationError> {
    let ordered_pair_count = counts
        .grid_nodes
        .checked_mul(counts.grid_nodes - 1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    while edges.len() < as_usize(counts.edge_count)? {
        let ordinal = topology_rng.bounded_u64(ordered_pair_count)?;
        let from = ordinal / (counts.grid_nodes - 1);
        let target_rank = ordinal % (counts.grid_nodes - 1);
        let to = if target_rank < from {
            target_rank
        } else {
            target_rank + 1
        };
        let from = u32::try_from(from).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        let to = u32::try_from(to).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        push_gridgen_ordinary_edge(
            edges,
            capacities,
            costs,
            gridgen_id(from / config.columns, from % config.columns),
            gridgen_id(to / config.columns, to % config.columns),
            config,
            capacity_rng,
            cost_rng,
        )?;
    }
    Ok(())
}

fn positive_composition(
    total: u32,
    parts: u32,
    rng: &mut RngV1,
) -> Result<Vec<u32>, FlowGenerationError> {
    if parts == 0 || total < parts {
        return Err(FlowGenerationError::Invalid("positive composition"));
    }
    let cuts = sample_ordinals(u64::from(total - 1), u64::from(parts - 1), rng)?;
    let mut result =
        Vec::with_capacity(usize::try_from(parts).map_err(|_| FlowGenerationError::SizeLimit)?);
    let mut previous = 0_u64;
    for cut in cuts.into_iter().map(|value| value + 1) {
        result.push(
            u32::try_from(cut - previous).map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
        );
        previous = cut;
    }
    result.push(
        u32::try_from(u64::from(total) - previous)
            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
    );
    Ok(result)
}

const GRIDGRAPH_INTERNAL_SOLVER_NODE_LIMIT: u64 = 2_000;

#[derive(Clone, Copy, Debug)]
struct GridgraphConfig {
    rows: u32,
    columns: u32,
    maximum_capacity: u32,
    maximum_cost: u32,
}

#[derive(Clone, Copy, Debug)]
struct GridgraphCounts {
    node_count: u64,
    edge_count: u64,
}

#[derive(Debug)]
struct GridgraphEdges {
    endpoints: Vec<(String, String)>,
    capacities: Vec<u64>,
    costs: Vec<i64>,
}

fn gridgraph_grid_topology(
    rows: u32,
    columns: u32,
    maximum_capacity: u32,
    maximum_cost: u32,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let config = GridgraphConfig {
        rows,
        columns,
        maximum_capacity,
        maximum_cost,
    };
    let counts = validate_gridgraph_config(config)?;
    let mut nodes = gridgraph_nodes(config, counts)?;
    let materialized = gridgraph_edges(config, counts, capacity_rng, cost_rng)?;
    let maximum_flow =
        gridgraph_maximum_flow(&nodes, &materialized.endpoints, &materialized.capacities)?;
    let maximum_flow =
        i64::try_from(maximum_flow).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
    nodes
        .first_mut()
        .ok_or(FlowGenerationError::Canonicalization)?
        .supply = maximum_flow.to_string();
    nodes
        .last_mut()
        .ok_or(FlowGenerationError::Canonicalization)?
        .supply = maximum_flow
        .checked_neg()
        .ok_or(FlowGenerationError::ArithmeticOverflow)?
        .to_string();

    Ok(Topology {
        nodes,
        edges: materialized.endpoints,
        suggested_model: FlowProblemModelV1::Transshipment {},
        fixed_capacities: Some(materialized.capacities),
        fixed_costs: Some(materialized.costs),
    })
}

fn validate_gridgraph_config(
    config: GridgraphConfig,
) -> Result<GridgraphCounts, FlowGenerationError> {
    require_range(config.rows, 2, 1_000, "GRIDGRAPH rows")?;
    // The archived ggraph1.f block structure duplicates arcs at L=2. Keep the
    // public source-derived contract inside the historical executable domain.
    require_range(config.columns, 3, 1_000, "GRIDGRAPH columns")?;
    require_range(
        config.maximum_capacity,
        1,
        1_000_000_000,
        "GRIDGRAPH maximum capacity",
    )?;
    require_range(
        config.maximum_cost,
        1,
        1_000_000_000,
        "GRIDGRAPH maximum cost",
    )?;
    let grid_nodes = u64::from(config.rows)
        .checked_mul(u64::from(config.columns))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let node_count = grid_nodes
        .checked_add(2)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if node_count > GRIDGRAPH_INTERNAL_SOLVER_NODE_LIMIT {
        return Err(FlowGenerationError::SizeLimit);
    }
    let edge_count = grid_nodes
        .checked_mul(2)
        .and_then(|value| value.checked_add(u64::from(config.rows)))
        .and_then(|value| value.checked_sub(u64::from(config.columns)))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;
    Ok(GridgraphCounts {
        node_count,
        edge_count,
    })
}

fn gridgraph_nodes(
    config: GridgraphConfig,
    counts: GridgraphCounts,
) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    let mut nodes = Vec::with_capacity(as_usize(counts.node_count)?);
    nodes.push(positioned_node("s", 68, 270));
    for row in 0..config.rows {
        for column in 0..config.columns {
            nodes.push(positioned_node(
                &gridgraph_id(row, column),
                interpolate(168, 732, column, config.columns - 1)?,
                interpolate(68, 472, row, config.rows - 1)?,
            ));
        }
    }
    nodes.push(positioned_node("t", 832, 270));
    if nodes.len() != as_usize(counts.node_count)? {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(nodes)
}

fn gridgraph_edges(
    config: GridgraphConfig,
    counts: GridgraphCounts,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<GridgraphEdges, FlowGenerationError> {
    let edge_count = as_usize(counts.edge_count)?;
    let row_count =
        usize::try_from(config.rows).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
    let mut endpoints = Vec::with_capacity(edge_count);
    let mut capacities = Vec::with_capacity(edge_count);
    let mut costs = Vec::with_capacity(edge_count);
    let mut source_capacities = vec![0_u64; row_count];
    let mut sink_capacities = vec![0_u64; row_count];

    for row in 0..config.rows {
        for column in 0..config.columns {
            if column + 1 < config.columns {
                let capacity = gridgraph_capacity(config, capacity_rng)?;
                gridgraph_push_edge(
                    &mut endpoints,
                    &mut capacities,
                    &mut costs,
                    gridgraph_id(row, column),
                    gridgraph_id(row, column + 1),
                    capacity,
                    config,
                    cost_rng,
                )?;
                if column == 0 {
                    gridgraph_add_terminal_capacity(&mut source_capacities, row, capacity)?;
                }
                if column + 1 == config.columns - 1 {
                    gridgraph_add_terminal_capacity(&mut sink_capacities, row, capacity)?;
                }
            }
            if row + 1 < config.rows {
                let capacity = gridgraph_capacity(config, capacity_rng)?;
                gridgraph_push_edge(
                    &mut endpoints,
                    &mut capacities,
                    &mut costs,
                    gridgraph_id(row, column),
                    gridgraph_id(row + 1, column),
                    capacity,
                    config,
                    cost_rng,
                )?;
                if column == 0 {
                    gridgraph_add_terminal_capacity(&mut source_capacities, row, capacity)?;
                }
                if column == config.columns - 1 {
                    gridgraph_add_terminal_capacity(&mut sink_capacities, row + 1, capacity)?;
                }
            }
        }
    }

    for row in 0..config.rows {
        let row_index =
            usize::try_from(row).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        gridgraph_push_edge(
            &mut endpoints,
            &mut capacities,
            &mut costs,
            "s".to_owned(),
            gridgraph_id(row, 0),
            source_capacities[row_index],
            config,
            cost_rng,
        )?;
    }
    for row in 0..config.rows {
        let row_index =
            usize::try_from(row).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        gridgraph_push_edge(
            &mut endpoints,
            &mut capacities,
            &mut costs,
            gridgraph_id(row, config.columns - 1),
            "t".to_owned(),
            sink_capacities[row_index],
            config,
            cost_rng,
        )?;
    }

    if endpoints.len() != edge_count || capacities.len() != edge_count || costs.len() != edge_count
    {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(GridgraphEdges {
        endpoints,
        capacities,
        costs,
    })
}

fn gridgraph_capacity(
    config: GridgraphConfig,
    capacity_rng: &mut RngV1,
) -> Result<u64, FlowGenerationError> {
    sample_uniform_u64(capacity_rng, 1, u64::from(config.maximum_capacity))
}

#[allow(clippy::too_many_arguments)]
fn gridgraph_push_edge(
    endpoints: &mut Vec<(String, String)>,
    capacities: &mut Vec<u64>,
    costs: &mut Vec<i64>,
    from: String,
    to: String,
    capacity: u64,
    config: GridgraphConfig,
    cost_rng: &mut RngV1,
) -> Result<(), FlowGenerationError> {
    endpoints.push((from, to));
    capacities.push(capacity);
    costs.push(sample_uniform_i64(
        cost_rng,
        1,
        i64::from(config.maximum_cost),
    )?);
    Ok(())
}

fn gridgraph_add_terminal_capacity(
    capacities: &mut [u64],
    row: u32,
    capacity: u64,
) -> Result<(), FlowGenerationError> {
    let row = usize::try_from(row).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
    capacities[row] = capacities[row]
        .checked_add(capacity)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    Ok(())
}

fn gridgraph_maximum_flow(
    nodes: &[FlowNodeV1],
    endpoints: &[(String, String)],
    capacities: &[u64],
) -> Result<u64, FlowGenerationError> {
    let model_nodes = nodes
        .iter()
        .map(|node| {
            NodeId::parse(&node.id)
                .map(|id| FlowNode::new(id, 0))
                .map_err(|_| FlowGenerationError::Canonicalization)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let model_edges = endpoints
        .iter()
        .zip(capacities)
        .enumerate()
        .map(|(index, ((from, to), &capacity))| {
            Ok(UnresolvedFlowEdge {
                id: EdgeId::parse(&format!("gridgraph-edge-{index:05}"))
                    .map_err(|_| FlowGenerationError::Canonicalization)?,
                from: NodeId::parse(from).map_err(|_| FlowGenerationError::Canonicalization)?,
                to: NodeId::parse(to).map_err(|_| FlowGenerationError::Canonicalization)?,
                lower: 0,
                capacity,
                cost: 0,
            })
        })
        .collect::<Result<Vec<_>, FlowGenerationError>>()?;
    let network = FlowNetwork::new(model_nodes, model_edges)
        .map_err(|_| FlowGenerationError::Canonicalization)?;
    let source_id = NodeId::parse("s").map_err(|_| FlowGenerationError::Canonicalization)?;
    let sink_id = NodeId::parse("t").map_err(|_| FlowGenerationError::Canonicalization)?;
    let source = network
        .node_index(&source_id)
        .ok_or(FlowGenerationError::Canonicalization)?;
    let sink = network
        .node_index(&sink_id)
        .ok_or(FlowGenerationError::Canonicalization)?;
    let result =
        solve_dinic(&network, source, sink).map_err(|_| FlowGenerationError::Canonicalization)?;
    u64::try_from(result.certificate.value).map_err(|_| FlowGenerationError::ArithmeticOverflow)
}

const WASHINGTON_RANDOM_LEVEL_NODE_LIMIT: u64 = 2_000;
const WASHINGTON_MAXIMUM_CAPACITY: usize = 100_000_000;
const WASHINGTON_VISUAL_EDGE_LIMIT: u64 = 20_000;

#[derive(Clone, Copy, Debug)]
struct WashingtonMatchingConfig {
    part_size: u32,
    degree: u32,
}

#[derive(Clone, Copy, Debug)]
struct WashingtonMatchingCounts {
    nodes: u64,
    edges: u64,
}

fn washington_matching_topology(
    part_size: u32,
    degree: u32,
    topology_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let config = WashingtonMatchingConfig { part_size, degree };
    let counts = validate_washington_matching_config(config)?;
    let nodes = washington_matching_nodes(config, counts)?;
    let edges = washington_matching_edges(config, counts, topology_rng)?;
    let edge_count = edges.len();
    Ok(Topology {
        nodes,
        edges,
        suggested_model: bipartite_matching_adapter_model(config.part_size),
        fixed_capacities: Some(vec![1; edge_count]),
        fixed_costs: Some(vec![0; edge_count]),
    })
}

fn validate_washington_matching_config(
    config: WashingtonMatchingConfig,
) -> Result<WashingtonMatchingCounts, FlowGenerationError> {
    require_range(config.part_size, 2, 999, "Washington Matching part size")?;
    require_range(
        config.degree,
        1,
        usize::try_from(config.part_size).map_err(|_| FlowGenerationError::SizeLimit)?,
        "Washington Matching degree",
    )?;
    let part_size = u64::from(config.part_size);
    let nodes = part_size
        .checked_mul(2)
        .and_then(|value| value.checked_add(2))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edges = part_size
        .checked_mul(
            u64::from(config.degree)
                .checked_add(2)
                .ok_or(FlowGenerationError::ArithmeticOverflow)?,
        )
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if nodes > WASHINGTON_RANDOM_LEVEL_NODE_LIMIT || edges > WASHINGTON_VISUAL_EDGE_LIMIT {
        return Err(FlowGenerationError::SizeLimit);
    }
    enforce_graph_limits(as_usize(nodes)?, as_usize(edges)?)?;
    Ok(WashingtonMatchingCounts { nodes, edges })
}

fn washington_matching_nodes(
    config: WashingtonMatchingConfig,
    counts: WashingtonMatchingCounts,
) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    let mut nodes = Vec::with_capacity(as_usize(counts.nodes)?);
    nodes.push(positioned_node("s", 68, 270));
    for side in ['l', 'r'] {
        let x = if side == 'l' { 310 } else { 590 };
        for index in 0..config.part_size {
            nodes.push(positioned_node(
                &washington_matching_id(side, index),
                x,
                interpolate(68, 472, index, config.part_size - 1)?,
            ));
        }
    }
    nodes.push(positioned_node("t", 832, 270));
    if nodes.len() != as_usize(counts.nodes)? {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(nodes)
}

fn washington_matching_edges(
    config: WashingtonMatchingConfig,
    counts: WashingtonMatchingCounts,
    topology_rng: &mut RngV1,
) -> Result<Vec<(String, String)>, FlowGenerationError> {
    let mut edges = Vec::with_capacity(as_usize(counts.edges)?);
    for left in 0..config.part_size {
        edges.push(("s".to_owned(), washington_matching_id('l', left)));
    }
    for left in 0..config.part_size {
        for right in sample_ordinals(
            u64::from(config.part_size),
            u64::from(config.degree),
            topology_rng,
        )? {
            let right =
                u32::try_from(right).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
            edges.push((
                washington_matching_id('l', left),
                washington_matching_id('r', right),
            ));
        }
    }
    for right in 0..config.part_size {
        edges.push((washington_matching_id('r', right), "t".to_owned()));
    }
    if edges.len() != as_usize(counts.edges)? {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(edges)
}

fn washington_matching_id(side: char, index: u32) -> String {
    format!("{side}{index:04}")
}

#[derive(Clone, Copy, Debug)]
struct WashingtonMeshConfig {
    rows: u32,
    columns: u32,
    maximum_capacity: u32,
}

#[derive(Clone, Copy, Debug)]
struct WashingtonMeshCounts {
    nodes: u64,
    edges: u64,
}

fn washington_mesh_topology(
    rows: u32,
    columns: u32,
    maximum_capacity: u32,
    capacity_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let config = WashingtonMeshConfig {
        rows,
        columns,
        maximum_capacity,
    };
    let counts = validate_washington_mesh_config(config)?;
    let nodes = washington_mesh_nodes(config, counts)?;
    let fixed_edges = washington_mesh_edges(config, counts, capacity_rng)?;
    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; fixed_edges.endpoints.len()]),
        edges: fixed_edges.endpoints,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(fixed_edges.capacities),
    })
}

fn validate_washington_mesh_config(
    config: WashingtonMeshConfig,
) -> Result<WashingtonMeshCounts, FlowGenerationError> {
    require_range(config.rows, 3, 1_000, "Washington Mesh rows")?;
    require_range(config.columns, 2, 1_000, "Washington Mesh columns")?;
    require_range(
        config.maximum_capacity,
        1,
        WASHINGTON_MAXIMUM_CAPACITY,
        "Washington Mesh maximum capacity",
    )?;
    let grid_nodes = u64::from(config.rows)
        .checked_mul(u64::from(config.columns))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let nodes = grid_nodes
        .checked_add(2)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if nodes > WASHINGTON_RANDOM_LEVEL_NODE_LIMIT {
        return Err(FlowGenerationError::SizeLimit);
    }
    let edges = grid_nodes
        .checked_mul(3)
        .and_then(|value| value.checked_sub(u64::from(config.rows)))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(nodes)?, as_usize(edges)?)?;
    Ok(WashingtonMeshCounts { nodes, edges })
}

fn washington_mesh_nodes(
    config: WashingtonMeshConfig,
    counts: WashingtonMeshCounts,
) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    let mut nodes = Vec::with_capacity(as_usize(counts.nodes)?);
    nodes.push(positioned_node("s", 68, 270));
    for column in 0..config.columns {
        for row in 0..config.rows {
            nodes.push(positioned_node(
                &washington_mesh_id(column, row),
                interpolate(168, 732, column, config.columns - 1)?,
                interpolate(68, 472, row, config.rows - 1)?,
            ));
        }
    }
    nodes.push(positioned_node("t", 832, 270));
    if nodes.len() != as_usize(counts.nodes)? {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(nodes)
}

fn washington_mesh_edges(
    config: WashingtonMeshConfig,
    counts: WashingtonMeshCounts,
    capacity_rng: &mut RngV1,
) -> Result<FixedEdgeSet, FlowGenerationError> {
    let edge_count = as_usize(counts.edges)?;
    let mut endpoints = Vec::with_capacity(edge_count);
    let mut capacities = Vec::with_capacity(edge_count);
    let terminal_capacity = u64::from(config.maximum_capacity)
        .checked_mul(3)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    for row in 0..config.rows {
        endpoints.push(("s".to_owned(), washington_mesh_id(0, row)));
        capacities.push(terminal_capacity);
    }
    for column in 0..config.columns - 1 {
        for row in 0..config.rows {
            let targets = [
                (row + config.rows - 1) % config.rows,
                row,
                (row + 1) % config.rows,
            ];
            for target in targets {
                endpoints.push((
                    washington_mesh_id(column, row),
                    washington_mesh_id(column + 1, target),
                ));
                capacities.push(sample_uniform_u64(
                    capacity_rng,
                    1,
                    u64::from(config.maximum_capacity),
                )?);
            }
        }
    }
    for row in 0..config.rows {
        endpoints.push((washington_mesh_id(config.columns - 1, row), "t".to_owned()));
        capacities.push(terminal_capacity);
    }
    if endpoints.len() != edge_count || capacities.len() != edge_count {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(FixedEdgeSet {
        endpoints,
        capacities,
    })
}

fn washington_mesh_id(column: u32, row: u32) -> String {
    format!("m{column:04}r{row:04}")
}

#[derive(Clone, Copy, Debug)]
struct WashingtonSquareMeshConfig {
    dimension: u32,
    degree: u32,
    maximum_capacity: u32,
}

#[derive(Clone, Copy, Debug)]
struct WashingtonSquareMeshCounts {
    nodes: u64,
    edges: u64,
}

fn washington_square_mesh_topology(
    dimension: u32,
    degree: u32,
    maximum_capacity: u32,
    capacity_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let config = WashingtonSquareMeshConfig {
        dimension,
        degree,
        maximum_capacity,
    };
    let counts = validate_washington_square_mesh_config(config)?;
    let nodes = washington_square_mesh_nodes(config, counts)?;
    let fixed_edges = washington_square_mesh_edges(config, counts, capacity_rng)?;
    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; fixed_edges.endpoints.len()]),
        edges: fixed_edges.endpoints,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(fixed_edges.capacities),
    })
}

fn validate_washington_square_mesh_config(
    config: WashingtonSquareMeshConfig,
) -> Result<WashingtonSquareMeshCounts, FlowGenerationError> {
    require_range(config.dimension, 2, 44, "Washington Square Mesh dimension")?;
    require_range(
        config.degree,
        1,
        usize::try_from(config.dimension).map_err(|_| FlowGenerationError::SizeLimit)?,
        "Washington Square Mesh degree",
    )?;
    require_range(
        config.maximum_capacity,
        1,
        WASHINGTON_MAXIMUM_CAPACITY,
        "Washington Square Mesh maximum capacity",
    )?;
    let dimension = u64::from(config.dimension);
    let degree = u64::from(config.degree);
    let grid_nodes = dimension
        .checked_mul(dimension)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let nodes = grid_nodes
        .checked_add(2)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let full_internal = degree
        .checked_mul(
            dimension
                .checked_mul(dimension - 1)
                .ok_or(FlowGenerationError::ArithmeticOverflow)?,
        )
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let clipped_tail = degree
        .checked_mul(degree - 1)
        .and_then(|value| value.checked_div(2))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edges = full_internal
        .checked_sub(clipped_tail)
        .and_then(|value| value.checked_add(2 * dimension))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if nodes > WASHINGTON_RANDOM_LEVEL_NODE_LIMIT || edges > WASHINGTON_VISUAL_EDGE_LIMIT {
        return Err(FlowGenerationError::SizeLimit);
    }
    enforce_graph_limits(as_usize(nodes)?, as_usize(edges)?)?;
    Ok(WashingtonSquareMeshCounts { nodes, edges })
}

fn washington_square_mesh_nodes(
    config: WashingtonSquareMeshConfig,
    counts: WashingtonSquareMeshCounts,
) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    let mut nodes = Vec::with_capacity(as_usize(counts.nodes)?);
    nodes.push(positioned_node("s", 68, 270));
    for column in 0..config.dimension {
        for row in 0..config.dimension {
            nodes.push(positioned_node(
                &washington_square_mesh_id(column, row),
                interpolate(168, 732, column, config.dimension - 1)?,
                interpolate(68, 472, row, config.dimension - 1)?,
            ));
        }
    }
    nodes.push(positioned_node("t", 832, 270));
    if nodes.len() != as_usize(counts.nodes)? {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(nodes)
}

fn washington_square_mesh_edges(
    config: WashingtonSquareMeshConfig,
    counts: WashingtonSquareMeshCounts,
    capacity_rng: &mut RngV1,
) -> Result<FixedEdgeSet, FlowGenerationError> {
    let edge_count = as_usize(counts.edges)?;
    let mut endpoints = Vec::with_capacity(edge_count);
    let mut capacities = Vec::with_capacity(edge_count);
    let terminal_capacity = u64::from(config.maximum_capacity)
        .checked_mul(3)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    for row in 0..config.dimension {
        endpoints.push(("s".to_owned(), washington_square_mesh_id(0, row)));
        capacities.push(terminal_capacity);
    }
    let grid_nodes = u64::from(config.dimension)
        .checked_mul(u64::from(config.dimension))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    for column in 0..config.dimension - 1 {
        for row in 0..config.dimension {
            let from_ordinal = u64::from(column)
                .checked_mul(u64::from(config.dimension))
                .and_then(|value| value.checked_add(u64::from(row)))
                .ok_or(FlowGenerationError::ArithmeticOverflow)?;
            for offset in 0..config.degree {
                let target_ordinal = from_ordinal
                    .checked_add(u64::from(config.dimension))
                    .and_then(|value| value.checked_add(u64::from(offset)))
                    .ok_or(FlowGenerationError::ArithmeticOverflow)?;
                if target_ordinal >= grid_nodes {
                    continue;
                }
                let target_column = target_ordinal / u64::from(config.dimension);
                let target_row = target_ordinal % u64::from(config.dimension);
                endpoints.push((
                    washington_square_mesh_id(column, row),
                    washington_square_mesh_id(
                        u32::try_from(target_column)
                            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
                        u32::try_from(target_row)
                            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
                    ),
                ));
                capacities.push(sample_uniform_u64(
                    capacity_rng,
                    1,
                    u64::from(config.maximum_capacity),
                )?);
            }
        }
    }
    for row in 0..config.dimension {
        endpoints.push((
            washington_square_mesh_id(config.dimension - 1, row),
            "t".to_owned(),
        ));
        capacities.push(terminal_capacity);
    }
    if endpoints.len() != edge_count || capacities.len() != edge_count {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(FixedEdgeSet {
        endpoints,
        capacities,
    })
}

fn washington_square_mesh_id(column: u32, row: u32) -> String {
    format!("q{column:04}r{row:04}")
}

const WASHINGTON_LINE_MAX_DEGREE: u32 = 20;
const WASHINGTON_LINE_MAX_CAPACITY: u64 = 1_000_000;
const WASHINGTON_LINE_VISUAL_NODE_LIMIT: u64 = 2_000;
const WASHINGTON_LINE_TERMINAL_CAPACITY: u64 = 20 * WASHINGTON_LINE_MAX_CAPACITY;
const WASHINGTON_LINE_CAPACITY_RANGES: [u64; 20] = [
    1_000_000, 500_000, 250_000, 125_000, 62_500, 31_250, 15_625, 7_812, 3_906, 1_953, 976, 488,
    244, 122, 61, 31, 15, 7, 4, 2,
];

#[derive(Clone, Copy)]
enum WashingtonLineProfile {
    Basic,
    Exponential,
    DoubleExponential,
}

fn washington_line_topology(
    levels: u32,
    width: u32,
    degree: u32,
    profile: WashingtonLineProfile,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let internal_nodes = validate_washington_line(levels, width, degree, profile)?;
    let mut nodes = Vec::with_capacity(as_usize(internal_nodes + 2)?);
    nodes.push(positioned_node("s", 68, 270));
    for ordinal in 0..internal_nodes {
        let level = ordinal / u64::from(width);
        let row = ordinal % u64::from(width);
        nodes.push(positioned_node(
            &washington_line_id(level, row),
            interpolate(
                168,
                732,
                u32::try_from(level).map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
                levels - 1,
            )?,
            interpolate(
                68,
                472,
                u32::try_from(row).map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
                width - 1,
            )?,
        ));
    }
    nodes.push(positioned_node("t", 832, 270));

    let upper_edges = u64::from(width)
        .checked_mul(2)
        .and_then(|value| value.checked_add(internal_nodes.checked_mul(u64::from(degree))?))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let mut endpoints = Vec::with_capacity(as_usize(upper_edges)?);
    let mut capacities = Vec::with_capacity(as_usize(upper_edges)?);
    for row in 0..u64::from(width) {
        endpoints.push(("s".to_owned(), washington_line_id(0, row)));
        capacities.push(WASHINGTON_LINE_TERMINAL_CAPACITY);
    }

    let span = u64::from(width)
        .checked_mul(u64::from(degree))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    for ordinal in 0..internal_nodes {
        let ranks = match profile {
            WashingtonLineProfile::Basic | WashingtonLineProfile::Exponential => {
                sample_ordinals(span, u64::from(degree), topology_rng)?
            }
            WashingtonLineProfile::DoubleExponential => sample_ordinals(
                span.checked_mul(2)
                    .and_then(|value| value.checked_add(1))
                    .ok_or(FlowGenerationError::ArithmeticOverflow)?,
                u64::from(degree),
                topology_rng,
            )?,
        };
        for rank in ranks {
            let offset = match profile {
                WashingtonLineProfile::Basic | WashingtonLineProfile::Exponential => {
                    i64::try_from(rank + 1).map_err(|_| FlowGenerationError::ArithmeticOverflow)?
                }
                WashingtonLineProfile::DoubleExponential => {
                    i64::try_from(rank).map_err(|_| FlowGenerationError::ArithmeticOverflow)?
                        - i64::try_from(span)
                            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?
                }
            };
            if offset == 0 {
                continue;
            }
            let target = i128::from(ordinal)
                .checked_add(i128::from(offset))
                .ok_or(FlowGenerationError::ArithmeticOverflow)?;
            if target < 0 || target >= i128::from(internal_nodes) {
                continue;
            }
            let target =
                u64::try_from(target).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
            let maximum_capacity = washington_line_capacity_limit(profile, offset, width)?;
            endpoints.push((
                washington_line_id(ordinal / u64::from(width), ordinal % u64::from(width)),
                washington_line_id(target / u64::from(width), target % u64::from(width)),
            ));
            capacities.push(sample_uniform_u64(capacity_rng, 1, maximum_capacity)?);
        }
    }
    for row in 0..u64::from(width) {
        endpoints.push((
            washington_line_id(u64::from(levels - 1), row),
            "t".to_owned(),
        ));
        capacities.push(WASHINGTON_LINE_TERMINAL_CAPACITY);
    }
    enforce_graph_limits(nodes.len(), endpoints.len())?;
    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; endpoints.len()]),
        edges: endpoints,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(capacities),
    })
}

fn validate_washington_line(
    levels: u32,
    width: u32,
    degree: u32,
    profile: WashingtonLineProfile,
) -> Result<u64, FlowGenerationError> {
    require_range(levels, 2, MAX_FLOW_NODES, "Washington Line levels")?;
    require_range(width, 1, MAX_FLOW_NODES, "Washington Line width")?;
    require_range(
        degree,
        1,
        usize::try_from(WASHINGTON_LINE_MAX_DEGREE)
            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
        "Washington Line degree",
    )?;
    if matches!(profile, WashingtonLineProfile::DoubleExponential)
        && (degree > 19 || (width == 1 && degree > 18))
    {
        return Err(FlowGenerationError::Invalid(
            "Washington Double Exponential Line safe degree",
        ));
    }
    let internal_nodes = u64::from(levels)
        .checked_mul(u64::from(width))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let nodes = internal_nodes
        .checked_add(2)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let upper_edges = internal_nodes
        .checked_mul(u64::from(degree))
        .and_then(|value| value.checked_add(u64::from(width) * 2))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if nodes > WASHINGTON_LINE_VISUAL_NODE_LIMIT || upper_edges > WASHINGTON_VISUAL_EDGE_LIMIT {
        return Err(FlowGenerationError::SizeLimit);
    }
    enforce_graph_limits(as_usize(nodes)?, as_usize(upper_edges)?)?;
    Ok(internal_nodes)
}

fn washington_line_capacity_limit(
    profile: WashingtonLineProfile,
    offset: i64,
    width: u32,
) -> Result<u64, FlowGenerationError> {
    let index = match profile {
        WashingtonLineProfile::Basic => return Ok(WASHINGTON_LINE_MAX_CAPACITY),
        WashingtonLineProfile::Exponential => u64::try_from((offset - 1) / i64::from(width))
            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
        WashingtonLineProfile::DoubleExponential => {
            ((offset - 1) / i64::from(width)).unsigned_abs()
        }
    };
    WASHINGTON_LINE_CAPACITY_RANGES
        .get(as_usize(index)?)
        .copied()
        .ok_or(FlowGenerationError::Canonicalization)
}

fn washington_line_id(level: u64, row: u64) -> String {
    format!("w{level:04}r{row:04}")
}

#[derive(Clone, Copy, Debug)]
struct WashingtonRandomLevelConfig {
    rows: u32,
    columns: u32,
    maximum_capacity: u32,
}

#[derive(Clone, Copy, Debug)]
struct WashingtonRandomLevelCounts {
    nodes: u64,
    edges: u64,
}

struct WashingtonRandomLevelEdges {
    edges: Vec<(String, String)>,
    capacities: Vec<u64>,
}

fn washington_random_level_topology(
    rows: u32,
    columns: u32,
    maximum_capacity: u32,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let config = WashingtonRandomLevelConfig {
        rows,
        columns,
        maximum_capacity,
    };
    let counts = validate_washington_random_level_config(config)?;
    let nodes = washington_random_level_nodes(config, counts)?;
    let WashingtonRandomLevelEdges { edges, capacities } =
        washington_random_level_edges(config, counts, topology_rng, capacity_rng)?;
    Ok(Topology {
        nodes,
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_costs: Some(vec![0; as_usize(counts.edges)?]),
        fixed_capacities: Some(capacities),
    })
}

fn validate_washington_random_level_config(
    config: WashingtonRandomLevelConfig,
) -> Result<WashingtonRandomLevelCounts, FlowGenerationError> {
    require_range(config.rows, 3, 1_000, "Washington Random Level rows")?;
    require_range(config.columns, 2, 1_000, "Washington Random Level columns")?;
    require_range(
        config.maximum_capacity,
        1,
        WASHINGTON_MAXIMUM_CAPACITY,
        "Washington Random Level maximum capacity",
    )?;
    let grid_nodes = u64::from(config.rows)
        .checked_mul(u64::from(config.columns))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let nodes = grid_nodes
        .checked_add(2)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if nodes > WASHINGTON_RANDOM_LEVEL_NODE_LIMIT {
        return Err(FlowGenerationError::SizeLimit);
    }
    let edges = grid_nodes
        .checked_mul(3)
        .and_then(|value| value.checked_sub(u64::from(config.rows)))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(nodes)?, as_usize(edges)?)?;
    Ok(WashingtonRandomLevelCounts { nodes, edges })
}

fn washington_random_level_nodes(
    config: WashingtonRandomLevelConfig,
    counts: WashingtonRandomLevelCounts,
) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    let mut nodes = Vec::with_capacity(as_usize(counts.nodes)?);
    nodes.push(positioned_node("s", 68, 270));
    for column in 0..config.columns {
        for row in 0..config.rows {
            nodes.push(positioned_node(
                &washington_random_level_id(column, row),
                interpolate(168, 732, column, config.columns - 1)?,
                interpolate(68, 472, row, config.rows - 1)?,
            ));
        }
    }
    nodes.push(positioned_node("t", 832, 270));
    if nodes.len() != as_usize(counts.nodes)? {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(nodes)
}

fn washington_random_level_edges(
    config: WashingtonRandomLevelConfig,
    counts: WashingtonRandomLevelCounts,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
) -> Result<WashingtonRandomLevelEdges, FlowGenerationError> {
    let edge_count = as_usize(counts.edges)?;
    let mut edges = Vec::with_capacity(edge_count);
    let mut capacities = Vec::with_capacity(edge_count);
    let terminal_capacity = u64::from(config.maximum_capacity)
        .checked_mul(3)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    for row in 0..config.rows {
        edges.push(("s".to_owned(), washington_random_level_id(0, row)));
        capacities.push(terminal_capacity);
    }
    for column in 0..config.columns - 1 {
        for row in 0..config.rows {
            for target in sample_ordinals(u64::from(config.rows), 3, topology_rng)? {
                let target =
                    u32::try_from(target).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
                edges.push((
                    washington_random_level_id(column, row),
                    washington_random_level_id(column + 1, target),
                ));
                capacities.push(sample_uniform_u64(
                    capacity_rng,
                    1,
                    u64::from(config.maximum_capacity),
                )?);
            }
        }
    }
    for row in 0..config.rows {
        edges.push((
            washington_random_level_id(config.columns - 1, row),
            "t".to_owned(),
        ));
        capacities.push(terminal_capacity);
    }
    if edges.len() != edge_count || capacities.len() != edge_count {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(WashingtonRandomLevelEdges { edges, capacities })
}

fn washington_random_level_id(column: u32, row: u32) -> String {
    format!("w{column:04}r{row:04}")
}

#[derive(Clone, Copy)]
struct GotoConfig {
    nodes: u32,
    edge_count: u32,
    maximum_capacity: u32,
    maximum_cost: u32,
}

#[derive(Clone, Copy, Debug)]
struct GotoShape {
    columns: u32,
    rows: u32,
    grid_nodes: u32,
    extra_nodes: u32,
    horizontal_degree: u32,
    vertical_degree: u32,
    extra_edges: u32,
}

#[derive(Debug)]
struct GotoEdges {
    edges: Vec<(String, String)>,
    capacities: Vec<u64>,
    costs: Vec<i64>,
    supply: u64,
}

#[allow(clippy::too_many_arguments)]
fn goto_torus_topology(
    nodes: u32,
    edge_count: u32,
    maximum_capacity: u32,
    maximum_cost: u32,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let config = GotoConfig {
        nodes,
        edge_count,
        maximum_capacity,
        maximum_cost,
    };
    let shape = validate_goto_config(config)?;
    let mut materialized_nodes = goto_nodes(shape)?;
    let materialized_edges = goto_edges(config, shape, topology_rng, capacity_rng, cost_rng)?;
    let supply = i64::try_from(materialized_edges.supply)
        .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
    let sink_index = usize::try_from(shape.grid_nodes - 1)
        .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
    materialized_nodes
        .first_mut()
        .ok_or(FlowGenerationError::Canonicalization)?
        .supply = supply.to_string();
    materialized_nodes
        .get_mut(sink_index)
        .ok_or(FlowGenerationError::Canonicalization)?
        .supply = (-supply).to_string();

    Ok(Topology {
        nodes: materialized_nodes,
        edges: materialized_edges.edges,
        suggested_model: FlowProblemModelV1::Transshipment {},
        fixed_capacities: Some(materialized_edges.capacities),
        fixed_costs: Some(materialized_edges.costs),
    })
}

fn validate_goto_config(config: GotoConfig) -> Result<GotoShape, FlowGenerationError> {
    require_range(config.nodes, 15, MAX_FLOW_NODES, "GOTO nodes")?;
    enforce_graph_limits(
        usize::try_from(config.nodes).map_err(|_| FlowGenerationError::SizeLimit)?,
        usize::try_from(config.edge_count).map_err(|_| FlowGenerationError::SizeLimit)?,
    )?;
    let nodes = u128::from(config.nodes);
    let edges = u128::from(config.edge_count);
    if edges < 6 * nodes || edges.pow(3) > nodes.pow(5) {
        return Err(FlowGenerationError::Invalid("GOTO edge count"));
    }
    require_range(
        config.maximum_capacity,
        8,
        1_000_000_000,
        "GOTO maximum capacity",
    )?;
    require_range(config.maximum_cost, 8, 1_000_000_000, "GOTO maximum cost")?;

    let (columns, rows) = goto_grid_dimensions(config.nodes)?;
    let grid_nodes = columns
        .checked_mul(rows)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let extra_nodes = config
        .nodes
        .checked_sub(grid_nodes)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let (horizontal_degree, vertical_degree, extra_edges) =
        goto_degrees_and_extra_edges(config.edge_count, columns, rows, grid_nodes, extra_nodes)?;
    Ok(GotoShape {
        columns,
        rows,
        grid_nodes,
        extra_nodes,
        horizontal_degree,
        vertical_degree,
        extra_edges,
    })
}

fn goto_grid_dimensions(nodes: u32) -> Result<(u32, u32), FlowGenerationError> {
    let mut cube_root = 1_u32;
    while u64::from(cube_root + 1).pow(3) <= u64::from(nodes) {
        cube_root += 1;
    }
    let initial_columns = cube_root
        .checked_mul(cube_root)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let rows = nodes
        .checked_div(initial_columns)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let columns = nodes
        .checked_div(rows)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if rows < 2 || columns < 2 {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok((columns, rows))
}

fn goto_fixed_edge_count(
    _columns: u32,
    rows: u32,
    grid_nodes: u32,
    extra_nodes: u32,
    horizontal_degree: u32,
    vertical_degree: u32,
) -> Result<u64, FlowGenerationError> {
    let torus_edges = u64::from(grid_nodes)
        .checked_mul(u64::from(horizontal_degree) + u64::from(vertical_degree))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let cut_replacements = u64::from(rows)
        .checked_mul(
            u64::from(horizontal_degree)
                .checked_mul(u64::from(horizontal_degree) + 1)
                .and_then(|value| value.checked_div(2))
                .ok_or(FlowGenerationError::ArithmeticOverflow)?,
        )
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let opened_torus = torus_edges
        .checked_add(cut_replacements)
        .and_then(|value| value.checked_sub(2 * u64::from(horizontal_degree)))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let extra_chain = if extra_nodes > 0 {
        u64::from(extra_nodes) + 1
    } else {
        0
    };
    opened_torus
        .checked_add(extra_chain)
        .and_then(|value| value.checked_add(u64::from(grid_nodes - 1)))
        .ok_or(FlowGenerationError::ArithmeticOverflow)
}

fn goto_remaining_edges(
    target_edges: u32,
    columns: u32,
    rows: u32,
    grid_nodes: u32,
    extra_nodes: u32,
    horizontal_degree: u32,
    vertical_degree: u32,
) -> Result<i128, FlowGenerationError> {
    Ok(i128::from(target_edges)
        - i128::from(goto_fixed_edge_count(
            columns,
            rows,
            grid_nodes,
            extra_nodes,
            horizontal_degree,
            vertical_degree,
        )?))
}

fn goto_degrees_and_extra_edges(
    target_edges: u32,
    columns: u32,
    rows: u32,
    grid_nodes: u32,
    extra_nodes: u32,
) -> Result<(u32, u32, u32), FlowGenerationError> {
    let mut vertical_degree = 1_u32;
    loop {
        vertical_degree = vertical_degree
            .checked_add(1)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        let horizontal_degree = vertical_degree
            .checked_mul(vertical_degree)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        if goto_remaining_edges(
            target_edges,
            columns,
            rows,
            grid_nodes,
            extra_nodes,
            horizontal_degree,
            vertical_degree,
        )? < 0
        {
            break;
        }
    }
    vertical_degree = vertical_degree
        .checked_sub(1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let mut horizontal_degree = vertical_degree
        .checked_mul(vertical_degree)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;

    loop {
        vertical_degree = vertical_degree
            .checked_add(1)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        if goto_remaining_edges(
            target_edges,
            columns,
            rows,
            grid_nodes,
            extra_nodes,
            horizontal_degree,
            vertical_degree,
        )? < 0
        {
            break;
        }
    }
    vertical_degree = vertical_degree
        .checked_sub(1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?
        .min(rows - 1);

    loop {
        horizontal_degree = horizontal_degree
            .checked_add(1)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        if goto_remaining_edges(
            target_edges,
            columns,
            rows,
            grid_nodes,
            extra_nodes,
            horizontal_degree,
            vertical_degree,
        )? < 0
        {
            break;
        }
    }
    horizontal_degree = horizontal_degree
        .checked_sub(1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?
        .min(columns - 1);
    if horizontal_degree == 0 || vertical_degree == 0 {
        return Err(FlowGenerationError::Canonicalization);
    }
    let remaining = goto_remaining_edges(
        target_edges,
        columns,
        rows,
        grid_nodes,
        extra_nodes,
        horizontal_degree,
        vertical_degree,
    )?;
    let extra_edges =
        u32::try_from(remaining).map_err(|_| FlowGenerationError::Canonicalization)?;
    Ok((horizontal_degree, vertical_degree, extra_edges))
}

fn goto_nodes(shape: GotoShape) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    let mut nodes = Vec::with_capacity(
        usize::try_from(shape.grid_nodes + shape.extra_nodes)
            .map_err(|_| FlowGenerationError::SizeLimit)?,
    );
    for row in 0..shape.rows {
        for column in 0..shape.columns {
            nodes.push(FlowNodeV1 {
                id: goto_grid_id(row, column),
                supply: "0".to_owned(),
                position: Some(FlowPositionV1 {
                    x: interpolate(52, 760, column, shape.columns - 1)?.to_string(),
                    y: interpolate(48, 492, row, shape.rows - 1)?.to_string(),
                }),
            });
        }
    }
    for index in 0..shape.extra_nodes {
        nodes.push(FlowNodeV1 {
            id: goto_extra_id(index),
            supply: "0".to_owned(),
            position: Some(FlowPositionV1 {
                x: "840".to_owned(),
                y: if shape.extra_nodes == 1 {
                    "270".to_owned()
                } else {
                    interpolate(48, 492, index, shape.extra_nodes - 1)?.to_string()
                },
            }),
        });
    }
    Ok(nodes)
}

fn goto_edges(
    config: GotoConfig,
    shape: GotoShape,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<GotoEdges, FlowGenerationError> {
    let edge_capacity =
        usize::try_from(config.edge_count).map_err(|_| FlowGenerationError::SizeLimit)?;
    let mut result = GotoEdges {
        edges: Vec::with_capacity(edge_capacity),
        capacities: Vec::with_capacity(edge_capacity),
        costs: Vec::with_capacity(edge_capacity),
        supply: goto_ceil_sqrt(u64::from(config.maximum_capacity)),
    };
    goto_opened_torus_edges(config, shape, &mut result, capacity_rng, cost_rng)?;
    goto_extra_node_chain(config, shape, &mut result);
    goto_scattered_edges(
        config,
        shape,
        &mut result,
        topology_rng,
        capacity_rng,
        cost_rng,
    )?;
    goto_return_path(config, shape, &mut result);
    if result.edges.len() != edge_capacity
        || result.capacities.len() != edge_capacity
        || result.costs.len() != edge_capacity
    {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(result)
}

fn push_goto_edge(result: &mut GotoEdges, from: String, to: String, capacity: u64, cost: i64) {
    result.edges.push((from, to));
    result.capacities.push(capacity);
    result.costs.push(cost);
}

fn goto_opened_torus_edges(
    config: GotoConfig,
    shape: GotoShape,
    result: &mut GotoEdges,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<(), FlowGenerationError> {
    let source = goto_grid_id(0, 0);
    let sink = goto_grid_id(shape.rows - 1, shape.columns - 1);
    let maximum_degree = shape.horizontal_degree.max(shape.vertical_degree);
    for row in 0..shape.rows {
        for column in 0..shape.columns {
            let from = goto_grid_id(row, column);
            for distance in 1..=shape.horizontal_degree {
                let target_column = (column + distance) % shape.columns;
                let to = goto_grid_id(row, target_column);
                let capacity = goto_distance_capacity(
                    capacity_rng,
                    distance,
                    maximum_degree,
                    config.maximum_capacity,
                )?;
                let cost = sample_uniform_i64(cost_rng, 0, i64::from(config.maximum_cost))?;
                if target_column > column {
                    push_goto_edge(result, from.clone(), to.clone(), capacity, cost);
                } else {
                    if from != sink {
                        push_goto_edge(result, from.clone(), sink.clone(), capacity, cost);
                        result.supply = result
                            .supply
                            .checked_add(capacity)
                            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
                    }
                    if to != source {
                        push_goto_edge(result, source.clone(), to.clone(), capacity, cost);
                    }
                }
                if to == sink {
                    result.supply = result
                        .supply
                        .checked_add(capacity)
                        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
                }
            }
            for distance in 1..=shape.vertical_degree {
                let target_row = (row + distance) % shape.rows;
                push_goto_edge(
                    result,
                    from.clone(),
                    goto_grid_id(target_row, column),
                    sample_uniform_u64(capacity_rng, 1, u64::from(config.maximum_capacity))?,
                    sample_uniform_i64(cost_rng, 0, 8)?,
                );
            }
        }
    }
    Ok(())
}

fn goto_extra_node_chain(config: GotoConfig, shape: GotoShape, result: &mut GotoEdges) {
    if shape.extra_nodes == 0 {
        return;
    }
    let capacity = goto_ceil_sqrt(u64::from(config.maximum_capacity));
    let cost = i64::from(config.maximum_cost / 2);
    let source = goto_grid_id(0, 0);
    let sink = goto_grid_id(shape.rows - 1, shape.columns - 1);
    push_goto_edge(result, source, goto_extra_id(0), capacity, cost);
    for index in 0..shape.extra_nodes - 1 {
        push_goto_edge(
            result,
            goto_extra_id(index),
            goto_extra_id(index + 1),
            capacity,
            cost,
        );
    }
    push_goto_edge(
        result,
        goto_extra_id(shape.extra_nodes - 1),
        sink,
        capacity,
        cost,
    );
}

fn goto_scattered_edges(
    config: GotoConfig,
    shape: GotoShape,
    result: &mut GotoEdges,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<(), FlowGenerationError> {
    let grid_candidates = u64::from(shape.grid_nodes - 2);
    let maximum_degree = shape.horizontal_degree.max(shape.vertical_degree);
    for _ in 0..shape.extra_edges {
        let grid_ordinal = 1 + topology_rng.bounded_u64(grid_candidates)?;
        let grid_id = goto_grid_id(
            u32::try_from(grid_ordinal / u64::from(shape.columns))
                .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
            u32::try_from(grid_ordinal % u64::from(shape.columns))
                .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
        );
        let (from, to) = if shape.extra_nodes > 0 {
            let extra_index =
                u32::try_from(topology_rng.bounded_u64(u64::from(shape.extra_nodes))?)
                    .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
            let extra_id = goto_extra_id(extra_index);
            if topology_rng.bounded_u64(2)? == 0 {
                (grid_id, extra_id)
            } else {
                (extra_id, grid_id)
            }
        } else {
            let source_index = grid_ordinal - 1;
            let target_rank = topology_rng.bounded_u64(grid_candidates - 1)?;
            let target_index = if target_rank < source_index {
                target_rank
            } else {
                target_rank + 1
            };
            let target_ordinal = target_index + 1;
            (
                grid_id,
                goto_grid_id(
                    u32::try_from(target_ordinal / u64::from(shape.columns))
                        .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
                    u32::try_from(target_ordinal % u64::from(shape.columns))
                        .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
                ),
            )
        };
        push_goto_edge(
            result,
            from,
            to,
            goto_distance_capacity(
                capacity_rng,
                shape.horizontal_degree,
                maximum_degree,
                config.maximum_capacity,
            )?,
            sample_uniform_i64(cost_rng, 0, 8)?,
        );
    }
    Ok(())
}

fn goto_return_path(config: GotoConfig, shape: GotoShape, result: &mut GotoEdges) {
    let return_cost = i64::from((config.maximum_cost / shape.rows).max(1));
    let supply = result.supply;
    for column in 0..shape.columns {
        for row in 0..shape.rows - 1 {
            push_goto_edge(
                result,
                goto_grid_id(row, column),
                goto_grid_id(row + 1, column),
                supply,
                return_cost,
            );
        }
        if column + 1 < shape.columns {
            push_goto_edge(
                result,
                goto_grid_id(shape.rows - 1, column),
                goto_grid_id(0, column + 1),
                supply,
                return_cost,
            );
        }
    }
}

fn goto_distance_capacity(
    rng: &mut RngV1,
    distance: u32,
    maximum_degree: u32,
    maximum_capacity: u32,
) -> Result<u64, FlowGenerationError> {
    if distance == 0 || distance > maximum_degree {
        return Err(FlowGenerationError::Canonicalization);
    }
    let raw = sample_uniform_u64(rng, 1, u64::from(maximum_capacity))?;
    let exponent = u64::from(distance - 1)
        .checked_mul(u64::from(maximum_capacity.ilog2()))
        .and_then(|value| value.checked_div(u64::from(maximum_degree) + 2))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let divisor = 1_u64
        .checked_shl(u32::try_from(exponent).map_err(|_| FlowGenerationError::ArithmeticOverflow)?)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    Ok(raw.div_ceil(divisor).max(1))
}

fn goto_ceil_sqrt(value: u64) -> u64 {
    let mut low = 0_u64;
    let mut high = 65_536_u64;
    while low < high {
        let middle = low.midpoint(high);
        if middle * middle >= value {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    low
}

fn goto_grid_id(row: u32, column: u32) -> String {
    format!("t{row:04}c{column:04}")
}

fn goto_extra_id(index: u32) -> String {
    format!("x{index:04}")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NetgenProblemKind {
    Assignment,
    Transportation,
    Transshipment,
    MaxFlow,
}

#[derive(Clone, Copy)]
struct NetgenConfig {
    nodes: u32,
    sources: u32,
    sinks: u32,
    edge_count: u32,
    minimum_cost: i64,
    maximum_cost: i64,
    total_supply: u32,
    transshipment_sources: u32,
    transshipment_sinks: u32,
    high_cost_percentage: u32,
    capacitated_percentage: u32,
    minimum_capacity: u32,
    maximum_capacity: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NetgenShape {
    problem_kind: NetgenProblemKind,
    middle_nodes: u32,
    sinks_per_source: u32,
    skeleton_edges: u32,
    allowed_edges: u64,
    pure_sources: u32,
    tail_count: u32,
    head_count: u32,
}

#[derive(Clone, Copy)]
struct NetgenSupportArc {
    sink: u32,
    flow: u32,
    chain_position: u32,
}

struct NetgenSkeletonMaterialization {
    edges: Vec<(String, String)>,
    capacities: Vec<u64>,
    costs: Vec<i64>,
    witness: Vec<(u32, u32, u32)>,
    ordinals: BTreeSet<u64>,
}

#[allow(clippy::too_many_arguments)]
fn netgen_skeleton_topology(
    nodes: u32,
    sources: u32,
    sinks: u32,
    edge_count: u32,
    minimum_cost: i64,
    maximum_cost: i64,
    total_supply: u32,
    transshipment_sources: u32,
    transshipment_sinks: u32,
    high_cost_percentage: u32,
    capacitated_percentage: u32,
    minimum_capacity: u32,
    maximum_capacity: u32,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
    supply_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let config = NetgenConfig {
        nodes,
        sources,
        sinks,
        edge_count,
        minimum_cost,
        maximum_cost,
        total_supply,
        transshipment_sources,
        transshipment_sinks,
        high_cost_percentage,
        capacitated_percentage,
        minimum_capacity,
        maximum_capacity,
    };
    let shape = validate_netgen_config(config)?;
    if shape.problem_kind == NetgenProblemKind::Assignment {
        netgen_assignment_topology(config, shape, topology_rng, cost_rng)
    } else {
        netgen_general_topology(
            config,
            shape,
            topology_rng,
            capacity_rng,
            cost_rng,
            supply_rng,
        )
    }
}

fn validate_netgen_config(config: NetgenConfig) -> Result<NetgenShape, FlowGenerationError> {
    let terminal_count = validate_netgen_scalar_ranges(config)?;
    let problem_kind = netgen_problem_kind(config, terminal_count)?;
    let middle_nodes = config.nodes - terminal_count;
    let sinks_per_source = netgen_sinks_per_source(config, problem_kind)?;
    let skeleton_edges = if problem_kind == NetgenProblemKind::Assignment {
        config.sources
    } else {
        middle_nodes
            .checked_add(
                config
                    .sources
                    .checked_mul(sinks_per_source)
                    .ok_or(FlowGenerationError::ArithmeticOverflow)?,
            )
            .ok_or(FlowGenerationError::ArithmeticOverflow)?
    };
    if config.edge_count < skeleton_edges {
        return Err(FlowGenerationError::Invalid("NETGEN skeleton edge count"));
    }
    let pure_sources = config.sources - config.transshipment_sources;
    let tail_count = config.nodes - config.sinks + config.transshipment_sinks;
    let head_count = config.nodes - pure_sources;
    let allowed_edges =
        netgen_allowed_edge_count(config, problem_kind, pure_sources, tail_count, head_count)?;
    if u64::from(config.edge_count) > allowed_edges {
        return Err(FlowGenerationError::Invalid("NETGEN allowed edge count"));
    }
    enforce_graph_limits(
        as_usize(u64::from(config.nodes))?,
        as_usize(u64::from(config.edge_count))?,
    )?;
    Ok(NetgenShape {
        problem_kind,
        middle_nodes,
        sinks_per_source,
        skeleton_edges,
        allowed_edges,
        pure_sources,
        tail_count,
        head_count,
    })
}

fn validate_netgen_scalar_ranges(config: NetgenConfig) -> Result<u32, FlowGenerationError> {
    require_range(config.nodes, 2, MAX_FLOW_NODES, "NETGEN node count")?;
    require_range(config.sources, 1, MAX_FLOW_NODES, "NETGEN source count")?;
    require_range(config.sinks, 1, MAX_FLOW_NODES, "NETGEN sink count")?;
    let terminal_count = config
        .sources
        .checked_add(config.sinks)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if terminal_count > config.nodes {
        return Err(FlowGenerationError::Invalid("NETGEN terminal count"));
    }
    if config.edge_count < config.nodes {
        return Err(FlowGenerationError::Invalid("NETGEN edge count"));
    }
    if config.minimum_cost < -1_000_000_000
        || config.maximum_cost > 1_000_000_000
        || config.minimum_cost > config.maximum_cost
    {
        return Err(FlowGenerationError::Invalid("NETGEN cost range"));
    }
    if config.total_supply < config.sources.max(config.sinks) || config.total_supply > 1_000_000_000
    {
        return Err(FlowGenerationError::Invalid("NETGEN total supply"));
    }
    if config.transshipment_sources > config.sources {
        return Err(FlowGenerationError::Invalid(
            "NETGEN transshipment source count",
        ));
    }
    if config.transshipment_sinks > config.sinks {
        return Err(FlowGenerationError::Invalid(
            "NETGEN transshipment sink count",
        ));
    }
    if config.high_cost_percentage > 100 || config.capacitated_percentage > 100 {
        return Err(FlowGenerationError::Invalid("NETGEN percentage"));
    }
    if config.minimum_capacity > config.maximum_capacity || config.maximum_capacity > 1_000_000_000
    {
        return Err(FlowGenerationError::Invalid("NETGEN capacity range"));
    }
    Ok(terminal_count)
}

fn netgen_problem_kind(
    config: NetgenConfig,
    terminal_count: u32,
) -> Result<NetgenProblemKind, FlowGenerationError> {
    let assignment = terminal_count == config.nodes
        && config.sources == config.sinks
        && config.transshipment_sources == 0
        && config.transshipment_sinks == 0
        && config.total_supply == config.sources;
    if assignment {
        Ok(NetgenProblemKind::Assignment)
    } else if config.minimum_cost == 1 && config.maximum_cost == 1 {
        if config.sources != 1 || config.sinks != 1 {
            return Err(FlowGenerationError::Invalid(
                "NETGEN max-flow terminal count",
            ));
        }
        Ok(NetgenProblemKind::MaxFlow)
    } else if terminal_count == config.nodes
        && config.transshipment_sources == 0
        && config.transshipment_sinks == 0
    {
        Ok(NetgenProblemKind::Transportation)
    } else {
        Ok(NetgenProblemKind::Transshipment)
    }
}

fn netgen_sinks_per_source(
    config: NetgenConfig,
    problem_kind: NetgenProblemKind,
) -> Result<u32, FlowGenerationError> {
    if problem_kind == NetgenProblemKind::Assignment {
        return Ok(1);
    }
    let doubled_sinks = u64::from(config.sinks)
        .checked_mul(2)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let rounded = doubled_sinks
        .checked_add(u64::from(config.sources - 1))
        .and_then(|value| value.checked_div(u64::from(config.sources)))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    Ok(u32::try_from(rounded)
        .map_err(|_| FlowGenerationError::ArithmeticOverflow)?
        .clamp(1, config.sinks))
}

fn netgen_allowed_edge_count(
    config: NetgenConfig,
    problem_kind: NetgenProblemKind,
    pure_sources: u32,
    tail_count: u32,
    head_count: u32,
) -> Result<u64, FlowGenerationError> {
    if problem_kind == NetgenProblemKind::Assignment {
        u64::from(config.sources)
            .checked_mul(u64::from(config.sinks))
            .ok_or(FlowGenerationError::ArithmeticOverflow)
    } else {
        let intersection = tail_count
            .checked_sub(pure_sources)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        u64::from(tail_count)
            .checked_mul(u64::from(head_count))
            .and_then(|value| value.checked_sub(u64::from(intersection)))
            .ok_or(FlowGenerationError::ArithmeticOverflow)
    }
}

fn netgen_general_topology(
    config: NetgenConfig,
    shape: NetgenShape,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
    supply_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let chains = netgen_source_chains(config, shape, topology_rng)?;
    let (support_arcs, source_supplies) =
        netgen_support_arcs(config, shape, &chains, topology_rng, supply_rng)?;
    let NetgenSkeletonMaterialization {
        mut edges,
        mut capacities,
        mut costs,
        witness,
        ordinals,
    } = netgen_materialize_general_skeleton(
        config,
        shape,
        &chains,
        &support_arcs,
        &source_supplies,
        capacity_rng,
        cost_rng,
    )?;
    if edges.len() != as_usize(u64::from(shape.skeleton_edges))? {
        return Err(FlowGenerationError::Canonicalization);
    }
    if witness
        .iter()
        .zip(&capacities)
        .any(|(&(_, _, flow), &capacity)| u64::from(flow) > capacity)
    {
        return Err(FlowGenerationError::Canonicalization);
    }
    netgen_append_general_extra_edges(
        config,
        shape,
        &ordinals,
        &mut edges,
        &mut capacities,
        &mut costs,
        topology_rng,
        capacity_rng,
        cost_rng,
    )?;
    let mut balances = netgen_witness_balances(config.nodes, &witness)?;
    if balances.iter().sum::<i64>() != 0
        || balances[..as_usize(u64::from(config.sources))?]
            .iter()
            .any(|&balance| balance <= 0)
        || balances[as_usize(u64::from(config.sources))?
            ..as_usize(u64::from(config.nodes - config.sinks))?]
            .iter()
            .any(|&balance| balance != 0)
        || balances[as_usize(u64::from(config.nodes - config.sinks))?..]
            .iter()
            .any(|&balance| balance >= 0)
    {
        return Err(FlowGenerationError::Canonicalization);
    }
    let suggested_model = if shape.problem_kind == NetgenProblemKind::MaxFlow {
        balances.fill(0);
        // NETGEN uses the unit-cost range as its historical problem-class
        // discriminator. Cost is not part of a max-flow objective, so the
        // materialized Max Flow workspace graph remains canonically cost-free.
        costs.fill(0);
        max_flow_model(
            &netgen_node_id(config, shape, 0),
            &netgen_node_id(config, shape, config.nodes - 1),
        )
    } else {
        FlowProblemModelV1::Transshipment {}
    };
    let nodes = netgen_nodes(config, shape, &chains, &balances)?;
    if edges.len() != as_usize(u64::from(config.edge_count))?
        || capacities.len() != edges.len()
        || costs.len() != edges.len()
    {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model,
        fixed_capacities: Some(capacities),
        fixed_costs: Some(costs),
    })
}

fn netgen_support_arcs(
    config: NetgenConfig,
    shape: NetgenShape,
    chains: &[Vec<u32>],
    topology_rng: &mut RngV1,
    supply_rng: &mut RngV1,
) -> Result<(Vec<NetgenSupportArc>, Vec<u32>), FlowGenerationError> {
    let (support_pairs, cover_pairs) = netgen_support_pairs(config, shape, topology_rng)?;
    let mut support_flows = weak_composition(
        config.total_supply - config.sources.max(config.sinks),
        u32::try_from(support_pairs.len()).map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
        supply_rng,
    )?;
    let support_index = support_pairs
        .iter()
        .copied()
        .enumerate()
        .map(|(index, pair)| (pair, index))
        .collect::<BTreeMap<_, _>>();
    for pair in cover_pairs {
        let index = *support_index
            .get(&pair)
            .ok_or(FlowGenerationError::Canonicalization)?;
        support_flows[index] = support_flows[index]
            .checked_add(1)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    }
    let mut source_supplies = vec![0_u32; as_usize(u64::from(config.sources))?];
    let mut arcs = Vec::with_capacity(support_pairs.len());
    for ((source, sink), flow) in support_pairs.into_iter().zip(support_flows) {
        let source_index = as_usize(u64::from(source))?;
        let chain_length = chains
            .get(source_index)
            .ok_or(FlowGenerationError::Canonicalization)?
            .len();
        let source_supply = source_supplies
            .get_mut(source_index)
            .ok_or(FlowGenerationError::Canonicalization)?;
        *source_supply = source_supply
            .checked_add(flow)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        arcs.push(NetgenSupportArc {
            sink,
            flow,
            chain_position: u32::try_from(topology_rng.bounded_u64(
                u64::try_from(chain_length).map_err(|_| FlowGenerationError::ArithmeticOverflow)?
                    + 1,
            )?)
            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
        });
    }
    if source_supplies.iter().copied().map(u64::from).sum::<u64>() != u64::from(config.total_supply)
        || source_supplies.contains(&0)
    {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok((arcs, source_supplies))
}

fn netgen_materialize_general_skeleton(
    config: NetgenConfig,
    shape: NetgenShape,
    chains: &[Vec<u32>],
    support_arcs: &[NetgenSupportArc],
    source_supplies: &[u32],
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<NetgenSkeletonMaterialization, FlowGenerationError> {
    let mut result = NetgenSkeletonMaterialization {
        edges: Vec::with_capacity(as_usize(u64::from(config.edge_count))?),
        capacities: Vec::with_capacity(as_usize(u64::from(config.edge_count))?),
        costs: Vec::with_capacity(as_usize(u64::from(config.edge_count))?),
        witness: Vec::with_capacity(as_usize(u64::from(shape.skeleton_edges))?),
        ordinals: BTreeSet::new(),
    };
    let support_width = as_usize(u64::from(shape.sinks_per_source))?;
    for source in 0..config.sources {
        let source_index = as_usize(u64::from(source))?;
        let chain = chains
            .get(source_index)
            .ok_or(FlowGenerationError::Canonicalization)?;
        let start = source_index
            .checked_mul(support_width)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        let source_arcs = support_arcs
            .get(start..start + support_width)
            .ok_or(FlowGenerationError::Canonicalization)?;
        netgen_materialize_source_chain(
            config,
            shape,
            source,
            chain,
            source_arcs,
            *source_supplies
                .get(source_index)
                .ok_or(FlowGenerationError::Canonicalization)?,
            &mut result,
            capacity_rng,
            cost_rng,
        )?;
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn netgen_materialize_source_chain(
    config: NetgenConfig,
    shape: NetgenShape,
    source: u32,
    chain: &[u32],
    support_arcs: &[NetgenSupportArc],
    source_supply: u32,
    result: &mut NetgenSkeletonMaterialization,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<(), FlowGenerationError> {
    for (position, &to) in chain.iter().enumerate() {
        let from = if position == 0 {
            source
        } else {
            chain[position - 1]
        };
        let position =
            u32::try_from(position).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        let flow = support_arcs
            .iter()
            .filter(|arc| arc.chain_position > position)
            .try_fold(0_u32, |sum, arc| {
                sum.checked_add(arc.flow)
                    .ok_or(FlowGenerationError::ArithmeticOverflow)
            })?;
        netgen_push_general_skeleton(
            config,
            shape,
            from,
            to,
            source_supply,
            flow,
            &mut result.edges,
            &mut result.capacities,
            &mut result.costs,
            &mut result.witness,
            &mut result.ordinals,
            capacity_rng,
            cost_rng,
        )?;
    }
    for &arc in support_arcs {
        let from = if arc.chain_position == 0 {
            source
        } else {
            *chain
                .get(as_usize(u64::from(arc.chain_position - 1))?)
                .ok_or(FlowGenerationError::Canonicalization)?
        };
        let to = config.nodes - config.sinks + arc.sink;
        netgen_push_general_skeleton(
            config,
            shape,
            from,
            to,
            source_supply,
            arc.flow,
            &mut result.edges,
            &mut result.capacities,
            &mut result.costs,
            &mut result.witness,
            &mut result.ordinals,
            capacity_rng,
            cost_rng,
        )?;
    }
    Ok(())
}

fn netgen_assignment_topology(
    config: NetgenConfig,
    shape: NetgenShape,
    topology_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    let mut sink_order = (0..config.sinks).collect::<Vec<_>>();
    shuffle_indices(&mut sink_order, topology_rng)?;
    let mut edges = Vec::with_capacity(as_usize(u64::from(config.edge_count))?);
    let mut capacities = Vec::with_capacity(edges.capacity());
    let mut costs = Vec::with_capacity(edges.capacity());
    let mut skeleton_ordinals = BTreeSet::new();
    for source in 0..config.sources {
        let sink = *sink_order
            .get(as_usize(u64::from(source))?)
            .ok_or(FlowGenerationError::Canonicalization)?;
        let to = config
            .sources
            .checked_add(sink)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        edges.push((
            netgen_node_id(config, shape, source),
            netgen_node_id(config, shape, to),
        ));
        capacities.push(1);
        let high_cost = netgen_percent_hit(cost_rng, config.high_cost_percentage)?;
        let sampled_cost = sample_uniform_i64(cost_rng, config.minimum_cost, config.maximum_cost)?;
        costs.push(if high_cost {
            config.maximum_cost
        } else {
            sampled_cost
        });
        skeleton_ordinals.insert(
            u64::from(source)
                .checked_mul(u64::from(config.sinks))
                .and_then(|value| value.checked_add(u64::from(sink)))
                .ok_or(FlowGenerationError::ArithmeticOverflow)?,
        );
    }
    let extra_count = u64::from(config.edge_count - shape.skeleton_edges);
    for ordinal in netgen_sample_complement_ordinals(
        shape.allowed_edges,
        &skeleton_ordinals,
        extra_count,
        topology_rng,
    )? {
        let source = u32::try_from(ordinal / u64::from(config.sinks))
            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        let sink = u32::try_from(ordinal % u64::from(config.sinks))
            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        let to = config
            .sources
            .checked_add(sink)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        edges.push((
            netgen_node_id(config, shape, source),
            netgen_node_id(config, shape, to),
        ));
        capacities.push(1);
        costs.push(sample_uniform_i64(
            cost_rng,
            config.minimum_cost,
            config.maximum_cost,
        )?);
    }
    let mut balances = vec![0_i64; as_usize(u64::from(config.nodes))?];
    for source in 0..config.sources {
        balances[as_usize(u64::from(source))?] = 1;
    }
    for sink in config.sources..config.nodes {
        balances[as_usize(u64::from(sink))?] = -1;
    }
    let chains = vec![Vec::new(); as_usize(u64::from(config.sources))?];
    let nodes = netgen_nodes(config, shape, &chains, &balances)?;
    if edges.len() != as_usize(u64::from(config.edge_count))? {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: FlowProblemModelV1::Transshipment {},
        fixed_capacities: Some(capacities),
        fixed_costs: Some(costs),
    })
}

fn netgen_source_chains(
    config: NetgenConfig,
    shape: NetgenShape,
    topology_rng: &mut RngV1,
) -> Result<Vec<Vec<u32>>, FlowGenerationError> {
    let mut chains = vec![Vec::new(); as_usize(u64::from(config.sources))?];
    let middle_start = config.sources;
    let middle_end = config.nodes - config.sinks;
    let mut middle = (middle_start..middle_end).collect::<Vec<_>>();
    shuffle_indices(&mut middle, topology_rng)?;
    let balanced_count = shape
        .middle_nodes
        .checked_mul(3)
        .and_then(|value| value.checked_div(5))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    for (rank, node) in middle.into_iter().enumerate() {
        let source = if u32::try_from(rank).map_err(|_| FlowGenerationError::ArithmeticOverflow)?
            < balanced_count
        {
            u32::try_from(rank).map_err(|_| FlowGenerationError::ArithmeticOverflow)?
                % config.sources
        } else {
            u32::try_from(topology_rng.bounded_u64(u64::from(config.sources))?)
                .map_err(|_| FlowGenerationError::ArithmeticOverflow)?
        };
        chains
            .get_mut(as_usize(u64::from(source))?)
            .ok_or(FlowGenerationError::Canonicalization)?
            .push(node);
    }
    Ok(chains)
}

type NetgenPairs = (Vec<(u32, u32)>, Vec<(u32, u32)>);

fn netgen_support_pairs(
    config: NetgenConfig,
    shape: NetgenShape,
    topology_rng: &mut RngV1,
) -> Result<NetgenPairs, FlowGenerationError> {
    let mut sink_order = (0..config.sinks).collect::<Vec<_>>();
    shuffle_indices(&mut sink_order, topology_rng)?;
    let mut by_source = vec![BTreeSet::new(); as_usize(u64::from(config.sources))?];
    let mut cover_pairs =
        Vec::with_capacity(as_usize(u64::from(config.sources.max(config.sinks)))?);
    if config.sources <= config.sinks {
        for ordinal in 0..config.sinks {
            let source = ordinal % config.sources;
            let sink = sink_order[as_usize(u64::from(ordinal))?];
            by_source[as_usize(u64::from(source))?].insert(sink);
            cover_pairs.push((source, sink));
        }
    } else {
        for source in 0..config.sources {
            let sink = sink_order[as_usize(u64::from(source % config.sinks))?];
            by_source[as_usize(u64::from(source))?].insert(sink);
            cover_pairs.push((source, sink));
        }
    }
    for sinks in &mut by_source {
        let required = u64::from(shape.sinks_per_source)
            .checked_sub(
                u64::try_from(sinks.len()).map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
            )
            .ok_or(FlowGenerationError::Canonicalization)?;
        let excluded = sinks
            .iter()
            .copied()
            .map(u64::from)
            .collect::<BTreeSet<_>>();
        for sink in netgen_sample_complement_ordinals(
            u64::from(config.sinks),
            &excluded,
            required,
            topology_rng,
        )? {
            sinks.insert(u32::try_from(sink).map_err(|_| FlowGenerationError::ArithmeticOverflow)?);
        }
    }
    let mut support_pairs = Vec::with_capacity(as_usize(
        u64::from(config.sources) * u64::from(shape.sinks_per_source),
    )?);
    for (source, sinks) in by_source.into_iter().enumerate() {
        let source = u32::try_from(source).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        support_pairs.extend(sinks.into_iter().map(|sink| (source, sink)));
    }
    if support_pairs.len()
        != as_usize(u64::from(config.sources) * u64::from(shape.sinks_per_source))?
    {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok((support_pairs, cover_pairs))
}

fn weak_composition(
    total: u32,
    parts: u32,
    rng: &mut RngV1,
) -> Result<Vec<u32>, FlowGenerationError> {
    if parts == 0 {
        return Err(FlowGenerationError::Invalid("weak composition"));
    }
    if parts == 1 {
        return Ok(vec![total]);
    }
    let slots = u64::from(total)
        .checked_add(u64::from(parts - 1))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let separators = sample_ordinals(slots, u64::from(parts - 1), rng)?;
    let mut result = Vec::with_capacity(as_usize(u64::from(parts))?);
    let mut previous = None;
    for separator in separators {
        let value = match previous {
            Some(previous) => separator - previous - 1,
            None => separator,
        };
        result.push(u32::try_from(value).map_err(|_| FlowGenerationError::ArithmeticOverflow)?);
        previous = Some(separator);
    }
    let last = slots
        .checked_sub(previous.ok_or(FlowGenerationError::Canonicalization)? + 1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    result.push(u32::try_from(last).map_err(|_| FlowGenerationError::ArithmeticOverflow)?);
    if result.iter().copied().map(u64::from).sum::<u64>() != u64::from(total) {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn netgen_push_general_skeleton(
    config: NetgenConfig,
    shape: NetgenShape,
    from: u32,
    to: u32,
    source_supply: u32,
    flow: u32,
    edges: &mut Vec<(String, String)>,
    capacities: &mut Vec<u64>,
    costs: &mut Vec<i64>,
    witness: &mut Vec<(u32, u32, u32)>,
    ordinals: &mut BTreeSet<u64>,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<(), FlowGenerationError> {
    let ordinal = netgen_general_pair_ordinal(config, shape, from, to)?;
    if !ordinals.insert(ordinal) {
        return Err(FlowGenerationError::Canonicalization);
    }
    edges.push((
        netgen_node_id(config, shape, from),
        netgen_node_id(config, shape, to),
    ));
    let capacitated = netgen_percent_hit(capacity_rng, config.capacitated_percentage)?;
    capacities.push(if capacitated {
        u64::from(source_supply.max(config.minimum_capacity))
    } else {
        u64::from(config.total_supply)
    });
    let high_cost = netgen_percent_hit(cost_rng, config.high_cost_percentage)?;
    let sampled = sample_uniform_i64(cost_rng, config.minimum_cost, config.maximum_cost)?;
    costs.push(if high_cost {
        config.maximum_cost
    } else {
        sampled
    });
    witness.push((from, to, flow));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn netgen_append_general_extra_edges(
    config: NetgenConfig,
    shape: NetgenShape,
    skeleton_ordinals: &BTreeSet<u64>,
    edges: &mut Vec<(String, String)>,
    capacities: &mut Vec<u64>,
    costs: &mut Vec<i64>,
    topology_rng: &mut RngV1,
    capacity_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<(), FlowGenerationError> {
    let extra_count = u64::from(config.edge_count - shape.skeleton_edges);
    for ordinal in netgen_sample_complement_ordinals(
        shape.allowed_edges,
        skeleton_ordinals,
        extra_count,
        topology_rng,
    )? {
        let (from, to) = netgen_general_pair_from_ordinal(config, shape, ordinal)?;
        edges.push((
            netgen_node_id(config, shape, from),
            netgen_node_id(config, shape, to),
        ));
        let capacitated = netgen_percent_hit(capacity_rng, config.capacitated_percentage)?;
        let sampled_capacity = sample_uniform_u64(
            capacity_rng,
            u64::from(config.minimum_capacity),
            u64::from(config.maximum_capacity),
        )?;
        capacities.push(if capacitated {
            sampled_capacity
        } else {
            u64::from(config.total_supply)
        });
        costs.push(sample_uniform_i64(
            cost_rng,
            config.minimum_cost,
            config.maximum_cost,
        )?);
    }
    Ok(())
}

fn netgen_percent_hit(rng: &mut RngV1, percentage: u32) -> Result<bool, FlowGenerationError> {
    Ok(rng.bounded_u64(100)? < u64::from(percentage))
}

fn netgen_sample_complement_ordinals(
    candidate_count: u64,
    excluded: &BTreeSet<u64>,
    selected_count: u64,
    rng: &mut RngV1,
) -> Result<Vec<u64>, FlowGenerationError> {
    let excluded_count =
        u64::try_from(excluded.len()).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
    let available = candidate_count
        .checked_sub(excluded_count)
        .ok_or(FlowGenerationError::Canonicalization)?;
    if selected_count > available || excluded.iter().any(|&value| value >= candidate_count) {
        return Err(FlowGenerationError::Canonicalization);
    }
    let excluded = excluded.iter().copied().collect::<Vec<_>>();
    sample_ordinals(available, selected_count, rng)?
        .into_iter()
        .map(|rank| netgen_expand_complement_rank(rank, &excluded))
        .collect()
}

fn netgen_expand_complement_rank(rank: u64, excluded: &[u64]) -> Result<u64, FlowGenerationError> {
    let excluded_count =
        u64::try_from(excluded.len()).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
    let mut low = rank;
    let mut high = rank
        .checked_add(excluded_count)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    while low < high {
        let middle = low.midpoint(high);
        let skipped = u64::try_from(excluded.partition_point(|&value| value <= middle))
            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        let present = middle
            .checked_add(1)
            .and_then(|value| value.checked_sub(skipped))
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        if present > rank {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    if excluded.binary_search(&low).is_ok() {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok(low)
}

fn netgen_general_pair_from_ordinal(
    config: NetgenConfig,
    shape: NetgenShape,
    ordinal: u64,
) -> Result<(u32, u32), FlowGenerationError> {
    if ordinal >= shape.allowed_edges {
        return Err(FlowGenerationError::Canonicalization);
    }
    let pure_block = u64::from(shape.pure_sources)
        .checked_mul(u64::from(shape.head_count))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if ordinal < pure_block {
        let from = u32::try_from(ordinal / u64::from(shape.head_count))
            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        let head_rank = u32::try_from(ordinal % u64::from(shape.head_count))
            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        let to = shape
            .pure_sources
            .checked_add(head_rank)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        return Ok((from, to));
    }
    let block_size = shape
        .head_count
        .checked_sub(1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if block_size == 0 {
        return Err(FlowGenerationError::Canonicalization);
    }
    let remainder = ordinal - pure_block;
    let from = shape
        .pure_sources
        .checked_add(
            u32::try_from(remainder / u64::from(block_size))
                .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
        )
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if from >= shape.tail_count {
        return Err(FlowGenerationError::Canonicalization);
    }
    let head_rank = u32::try_from(remainder % u64::from(block_size))
        .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
    let mut to = shape
        .pure_sources
        .checked_add(head_rank)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if to >= from {
        to = to
            .checked_add(1)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    }
    if to >= config.nodes || to == from {
        return Err(FlowGenerationError::Canonicalization);
    }
    Ok((from, to))
}

fn netgen_general_pair_ordinal(
    config: NetgenConfig,
    shape: NetgenShape,
    from: u32,
    to: u32,
) -> Result<u64, FlowGenerationError> {
    if from >= shape.tail_count || to < shape.pure_sources || to >= config.nodes || from == to {
        return Err(FlowGenerationError::Canonicalization);
    }
    if from < shape.pure_sources {
        return u64::from(from)
            .checked_mul(u64::from(shape.head_count))
            .and_then(|value| value.checked_add(u64::from(to - shape.pure_sources)))
            .ok_or(FlowGenerationError::ArithmeticOverflow);
    }
    let pure_block = u64::from(shape.pure_sources)
        .checked_mul(u64::from(shape.head_count))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let block_size = shape
        .head_count
        .checked_sub(1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let mut head_rank = to - shape.pure_sources;
    if to > from {
        head_rank -= 1;
    }
    pure_block
        .checked_add(
            u64::from(from - shape.pure_sources)
                .checked_mul(u64::from(block_size))
                .ok_or(FlowGenerationError::ArithmeticOverflow)?,
        )
        .and_then(|value| value.checked_add(u64::from(head_rank)))
        .ok_or(FlowGenerationError::ArithmeticOverflow)
}

fn netgen_witness_balances(
    node_count: u32,
    witness: &[(u32, u32, u32)],
) -> Result<Vec<i64>, FlowGenerationError> {
    let mut balances = vec![0_i64; as_usize(u64::from(node_count))?];
    for &(from, to, flow) in witness {
        let from = balances
            .get_mut(as_usize(u64::from(from))?)
            .ok_or(FlowGenerationError::Canonicalization)?;
        *from = from
            .checked_add(i64::from(flow))
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        let to = balances
            .get_mut(as_usize(u64::from(to))?)
            .ok_or(FlowGenerationError::Canonicalization)?;
        *to = to
            .checked_sub(i64::from(flow))
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    }
    Ok(balances)
}

fn netgen_nodes(
    config: NetgenConfig,
    shape: NetgenShape,
    chains: &[Vec<u32>],
    balances: &[i64],
) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    if balances.len() != as_usize(u64::from(config.nodes))? {
        return Err(FlowGenerationError::Canonicalization);
    }
    let mut chain_position = vec![None; as_usize(u64::from(config.nodes))?];
    let maximum_chain = chains.iter().map(Vec::len).max().unwrap_or(0);
    for (source, chain) in chains.iter().enumerate() {
        for (position, &node) in chain.iter().enumerate() {
            chain_position[as_usize(u64::from(node))?] = Some((source, position));
        }
    }
    let mut nodes = Vec::with_capacity(as_usize(u64::from(config.nodes))?);
    for index in 0..config.nodes {
        let (x, y) = if index < config.sources {
            (60, interpolate(50, 490, index + 1, config.sources + 1)?)
        } else if index >= config.nodes - config.sinks {
            let sink = index - (config.nodes - config.sinks);
            (940, interpolate(50, 490, sink + 1, config.sinks + 1)?)
        } else {
            let (source, position) = chain_position[as_usize(u64::from(index))?]
                .ok_or(FlowGenerationError::Canonicalization)?;
            let source =
                u32::try_from(source).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
            let position =
                u32::try_from(position).map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
            let denominator = u32::try_from(maximum_chain + 1)
                .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
            (
                interpolate(160, 840, position + 1, denominator)?,
                interpolate(50, 490, source + 1, config.sources + 1)?,
            )
        };
        let mut node = positioned_node(&netgen_node_id(config, shape, index), x, y);
        node.supply = balances[as_usize(u64::from(index))?].to_string();
        nodes.push(node);
    }
    Ok(nodes)
}

fn netgen_node_id(config: NetgenConfig, shape: NetgenShape, index: u32) -> String {
    if index < config.sources {
        if index < shape.pure_sources {
            format!("s{index:04}")
        } else {
            format!("sx{index:04}")
        }
    } else if index >= config.nodes - config.sinks {
        let sink = index - (config.nodes - config.sinks);
        if sink < config.transshipment_sinks {
            format!("tx{sink:04}")
        } else {
            format!("t{sink:04}")
        }
    } else {
        format!("x{:04}", index - config.sources)
    }
}

fn erdos_renyi_topology(
    count: u32,
    edge_count: u32,
    rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(count, 2, MAX_FLOW_NODES, "random node count")?;
    let candidate_count = u64::from(count)
        .checked_mul(u64::from(count - 1))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if u64::from(edge_count) > candidate_count {
        return Err(FlowGenerationError::Invalid("random edge count"));
    }
    enforce_graph_limits(
        as_usize(u64::from(count))?,
        usize::try_from(edge_count).map_err(|_| FlowGenerationError::SizeLimit)?,
    )?;
    let edges = sample_ordinals(candidate_count, u64::from(edge_count), rng)?
        .into_iter()
        .map(|ordinal| {
            let width = u64::from(count - 1);
            let from = ordinal / width;
            let remainder = ordinal % width;
            let to = if remainder >= from {
                remainder + 1
            } else {
                remainder
            };
            Ok((format!("v{from:04}"), format!("v{to:04}")))
        })
        .collect::<Result<Vec<_>, FlowGenerationError>>()?;
    Ok(Topology {
        nodes: circular_nodes(count)?,
        edges,
        suggested_model: max_flow_model("v0000", &format!("v{:04}", count - 1)),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn arborescence_topology(branching: u32, depth: u32) -> Result<Topology, FlowGenerationError> {
    require_range(branching, 1, MAX_FLOW_NODES, "arborescence branching")?;
    require_range(depth, 1, MAX_FLOW_NODES, "arborescence depth")?;
    let mut level_size = 1_u64;
    let mut node_count = 1_u64;
    for _ in 0..depth {
        level_size = level_size
            .checked_mul(u64::from(branching))
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        node_count = node_count
            .checked_add(level_size)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        enforce_graph_limits(as_usize(node_count)?, as_usize(node_count - 1)?)?;
    }
    let count = u32::try_from(node_count).map_err(|_| FlowGenerationError::SizeLimit)?;
    let mut nodes = vec![positioned_node("s", 40, 270)];
    let mut edges = Vec::with_capacity(as_usize(node_count - 1)?);
    let mut parents = vec!["s".to_owned()];
    let mut ordinal = 1_u32;
    for level in 1..=depth {
        let child_count = u32::try_from(parents.len())
            .ok()
            .and_then(|value| value.checked_mul(branching))
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        let mut children = Vec::with_capacity(child_count as usize);
        for parent in &parents {
            for _ in 0..branching {
                let id = node_id(ordinal, count);
                let level_offset = u32::try_from(children.len())
                    .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
                nodes.push(positioned_node(
                    &id,
                    interpolate(40, 860, level, depth)?,
                    interpolate(40, 500, level_offset + 1, child_count + 1)?,
                ));
                edges.push((parent.clone(), id.clone()));
                children.push(id);
                ordinal += 1;
            }
        }
        parents = children;
    }
    Ok(st_topology(nodes, edges, count))
}

fn strongly_connected_topology(
    count: u32,
    extra_edges: u32,
    rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(count, 3, MAX_FLOW_NODES, "strongly connected node count")?;
    let allowed_per_source = u64::from(count - 2);
    let candidate_count = u64::from(count)
        .checked_mul(allowed_per_source)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if u64::from(extra_edges) > candidate_count {
        return Err(FlowGenerationError::Invalid(
            "strongly connected extra edge count",
        ));
    }
    let edge_count = u64::from(count) + u64::from(extra_edges);
    enforce_graph_limits(as_usize(u64::from(count))?, as_usize(edge_count)?)?;
    let mut edges = (0..count)
        .map(|from| (format!("v{from:04}"), format!("v{:04}", (from + 1) % count)))
        .collect::<Vec<_>>();
    for ordinal in sample_ordinals(candidate_count, u64::from(extra_edges), rng)? {
        let from = ordinal / allowed_per_source;
        let mut to = ordinal % allowed_per_source;
        let successor = (from + 1) % u64::from(count);
        let first_excluded = from.min(successor);
        let second_excluded = from.max(successor);
        if to >= first_excluded {
            to += 1;
        }
        if to >= second_excluded {
            to += 1;
        }
        edges.push((format!("v{from:04}"), format!("v{to:04}")));
    }
    Ok(Topology {
        nodes: circular_nodes(count)?,
        edges,
        suggested_model: max_flow_model("v0000", &format!("v{:04}", count - 1)),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn grid_3d_topology(layers: u32, rows: u32, columns: u32) -> Result<Topology, FlowGenerationError> {
    require_range(layers, 1, MAX_FLOW_NODES, "3D grid layers")?;
    require_range(rows, 1, MAX_FLOW_NODES, "3D grid rows")?;
    require_range(columns, 1, MAX_FLOW_NODES, "3D grid columns")?;
    let layer_size = u64::from(rows)
        .checked_mul(u64::from(columns))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let node_count = u64::from(layers)
        .checked_mul(layer_size)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if node_count < 2 {
        return Err(FlowGenerationError::Invalid("3D grid needs two nodes"));
    }
    let x_edges = u64::from(layers) * u64::from(rows) * u64::from(columns - 1);
    let y_edges = u64::from(layers) * u64::from(rows - 1) * u64::from(columns);
    let z_edges = u64::from(layers - 1) * layer_size;
    let edge_count = x_edges
        .checked_add(y_edges)
        .and_then(|value| value.checked_add(z_edges))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;
    let mut nodes = Vec::with_capacity(as_usize(node_count)?);
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    for layer in 0..layers {
        for row in 0..rows {
            for column in 0..columns {
                let id = grid_3d_id(layer, row, column);
                let planar_x = interpolate(50, 760, column, columns.saturating_sub(1).max(1))?;
                let layer_offset = interpolate(0, 100, layer, layers.saturating_sub(1).max(1))?;
                nodes.push(positioned_node(
                    &id,
                    planar_x + layer_offset,
                    interpolate(50, 490, row, rows.saturating_sub(1).max(1))? - layer_offset / 3,
                ));
                if column + 1 < columns {
                    edges.push((id.clone(), grid_3d_id(layer, row, column + 1)));
                }
                if row + 1 < rows {
                    edges.push((id.clone(), grid_3d_id(layer, row + 1, column)));
                }
                if layer + 1 < layers {
                    edges.push((id, grid_3d_id(layer + 1, row, column)));
                }
            }
        }
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: max_flow_model(
            &grid_3d_id(0, 0, 0),
            &grid_3d_id(layers - 1, rows - 1, columns - 1),
        ),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn bipartite_random_topology(
    left: u32,
    right: u32,
    edge_count: u32,
    rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(left, 1, MAX_FLOW_NODES, "bipartite left size")?;
    require_range(right, 1, MAX_FLOW_NODES, "bipartite right size")?;
    let candidate_count = u64::from(left)
        .checked_mul(u64::from(right))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if u64::from(edge_count) > candidate_count {
        return Err(FlowGenerationError::Invalid("bipartite edge count"));
    }
    let node_count = 2_u64 + u64::from(left) + u64::from(right);
    let total_edges = u64::from(left) + u64::from(edge_count) + u64::from(right);
    enforce_graph_limits(as_usize(node_count)?, as_usize(total_edges)?)?;
    let mut nodes = vec![
        positioned_node("s", 40, 270),
        positioned_node("t", 860, 270),
    ];
    let mut edges = Vec::with_capacity(as_usize(total_edges)?);
    for index in 0..left {
        let id = format!("l{index:04}");
        nodes.push(positioned_node(
            &id,
            280,
            interpolate(40, 500, index + 1, left + 1)?,
        ));
        edges.push(("s".to_owned(), id));
    }
    for index in 0..right {
        let id = format!("r{index:04}");
        nodes.push(positioned_node(
            &id,
            620,
            interpolate(40, 500, index + 1, right + 1)?,
        ));
    }
    for ordinal in sample_ordinals(candidate_count, u64::from(edge_count), rng)? {
        let from = ordinal / u64::from(right);
        let to = ordinal % u64::from(right);
        edges.push((format!("l{from:04}"), format!("r{to:04}")));
    }
    for index in 0..right {
        edges.push((format!("r{index:04}"), "t".to_owned()));
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn assignment_matrix_topology(
    agents: u32,
    tasks: u32,
    objective: AssignmentObjectiveV1,
    shape: &AssignmentMatrixShapeV1,
    topology_rng: &mut RngV1,
    cost_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(agents, 1, HUNGARIAN_MAX_NODES, "assignment agent count")?;
    require_range(tasks, 1, HUNGARIAN_MAX_NODES, "assignment task count")?;
    let node_count = u64::from(agents)
        .checked_add(u64::from(tasks))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let cell_work = u128::from(agents)
        .checked_mul(u128::from(agents))
        .and_then(|value| value.checked_mul(u128::from(tasks)))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if node_count
        > u64::try_from(HUNGARIAN_MAX_NODES).map_err(|_| FlowGenerationError::SizeLimit)?
        || cell_work > HUNGARIAN_MAX_CELL_SCANS
    {
        return Err(FlowGenerationError::Invalid(
            "assignment Hungarian admission band",
        ));
    }
    let candidate_count = u64::from(agents)
        .checked_mul(u64::from(tasks))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let planted_tasks = assignment_planted_tasks(agents, tasks, shape, topology_rng)?;
    let edge_ordinals = assignment_edge_ordinals(
        agents,
        tasks,
        shape,
        candidate_count,
        planted_tasks.as_deref(),
        topology_rng,
    )?;
    if edge_ordinals.len() > HUNGARIAN_MAX_EDGES {
        return Err(FlowGenerationError::Invalid(
            "assignment Hungarian edge admission band",
        ));
    }
    enforce_graph_limits(as_usize(node_count)?, edge_ordinals.len())?;

    let mut nodes = Vec::with_capacity(as_usize(node_count)?);
    for agent in 0..agents {
        nodes.push(positioned_node(
            &assignment_agent_id(agent),
            220,
            interpolate(40, 500, agent + 1, agents + 1)?,
        ));
    }
    for task in 0..tasks {
        nodes.push(positioned_node(
            &assignment_task_id(task),
            680,
            interpolate(40, 500, task + 1, tasks + 1)?,
        ));
    }

    let mut edges = Vec::with_capacity(edge_ordinals.len());
    let mut costs = Vec::with_capacity(edge_ordinals.len());
    for ordinal in edge_ordinals {
        let agent = u32::try_from(ordinal / u64::from(tasks))
            .map_err(|_| FlowGenerationError::SizeLimit)?;
        let task = u32::try_from(ordinal % u64::from(tasks))
            .map_err(|_| FlowGenerationError::SizeLimit)?;
        edges.push((assignment_agent_id(agent), assignment_task_id(task)));
        costs.push(assignment_pair_cost(
            agent,
            task,
            objective,
            shape,
            planted_tasks.as_deref(),
            cost_rng,
        )?);
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: FlowProblemModelV1::Assignment {
            agents: (0..agents).map(assignment_agent_id).collect(),
            tasks: (0..tasks).map(assignment_task_id).collect(),
            objective,
        },
        fixed_capacities: Some(vec![1; costs.len()]),
        fixed_costs: Some(costs),
    })
}

fn transportation_table_topology(
    origins: u32,
    destinations: u32,
    total_supply: u32,
    shape: &TransportationTableShapeV1,
    topology_rng: &mut RngV1,
    cost_rng: &mut RngV1,
    supply_rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(
        origins,
        1,
        TRANSPORTATION_MAX_NODES,
        "transportation origin count",
    )?;
    require_range(
        destinations,
        1,
        TRANSPORTATION_MAX_NODES,
        "transportation destination count",
    )?;
    let node_count = u64::from(origins)
        .checked_add(u64::from(destinations))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let candidate_count = u64::from(origins)
        .checked_mul(u64::from(destinations))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if node_count
        > u64::try_from(TRANSPORTATION_MAX_NODES).map_err(|_| FlowGenerationError::SizeLimit)?
        || candidate_count
            > u64::try_from(TRANSPORTATION_MAX_EDGES).map_err(|_| FlowGenerationError::SizeLimit)?
    {
        return Err(FlowGenerationError::Invalid(
            "transportation algorithm admission band",
        ));
    }
    if total_supply < origins.max(destinations) {
        return Err(FlowGenerationError::Invalid(
            "transportation positive balance total",
        ));
    }
    let (supplies, demands) =
        transportation_balances(origins, destinations, total_supply, shape, supply_rng)?;
    let edge_ordinals = transportation_edge_ordinals(
        destinations,
        candidate_count,
        shape,
        &supplies,
        &demands,
        topology_rng,
    )?;
    enforce_graph_limits(as_usize(node_count)?, edge_ordinals.len())?;

    let mut nodes = Vec::with_capacity(as_usize(node_count)?);
    for (origin, supply) in supplies.iter().copied().enumerate() {
        let origin = u32::try_from(origin).map_err(|_| FlowGenerationError::SizeLimit)?;
        let mut node = positioned_node(
            &transportation_origin_id(origin),
            220,
            interpolate(40, 500, origin + 1, origins + 1)?,
        );
        node.supply = supply.to_string();
        nodes.push(node);
    }
    for (destination, demand) in demands.iter().copied().enumerate() {
        let destination = u32::try_from(destination).map_err(|_| FlowGenerationError::SizeLimit)?;
        let mut node = positioned_node(
            &transportation_destination_id(destination),
            680,
            interpolate(40, 500, destination + 1, destinations + 1)?,
        );
        node.supply = i64::from(demand)
            .checked_neg()
            .ok_or(FlowGenerationError::ArithmeticOverflow)?
            .to_string();
        nodes.push(node);
    }

    let mut edges = Vec::with_capacity(edge_ordinals.len());
    let mut costs = Vec::with_capacity(edge_ordinals.len());
    for ordinal in edge_ordinals {
        let origin = u32::try_from(ordinal / u64::from(destinations))
            .map_err(|_| FlowGenerationError::SizeLimit)?;
        let destination = u32::try_from(ordinal % u64::from(destinations))
            .map_err(|_| FlowGenerationError::SizeLimit)?;
        edges.push((
            transportation_origin_id(origin),
            transportation_destination_id(destination),
        ));
        costs.push(transportation_pair_cost(
            origin,
            destination,
            destinations,
            shape,
            cost_rng,
        )?);
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: FlowProblemModelV1::Transportation {
            origins: (0..origins).map(transportation_origin_id).collect(),
            destinations: (0..destinations)
                .map(transportation_destination_id)
                .collect(),
        },
        fixed_capacities: Some(vec![u64::from(total_supply); costs.len()]),
        fixed_costs: Some(costs),
    })
}

fn transportation_balances(
    origins: u32,
    destinations: u32,
    total_supply: u32,
    shape: &TransportationTableShapeV1,
    rng: &mut RngV1,
) -> Result<(Vec<u32>, Vec<u32>), FlowGenerationError> {
    match shape {
        TransportationTableShapeV1::UnitDegenerate { .. } => {
            if origins != destinations || total_supply != origins {
                return Err(FlowGenerationError::Invalid(
                    "unit-degenerate transportation dimensions",
                ));
            }
            Ok((
                vec![1; as_usize(u64::from(origins))?],
                vec![1; as_usize(u64::from(destinations))?],
            ))
        }
        TransportationTableShapeV1::CutInfeasible { .. } => {
            if origins < 2
                || destinations < 2
                || total_supply < origins.saturating_add(1).max(destinations)
            {
                return Err(FlowGenerationError::Invalid(
                    "transportation cut-witness dimensions",
                ));
            }
            let mut supplies = vec![1; as_usize(u64::from(origins))?];
            supplies[0] = total_supply
                .checked_sub(origins - 1)
                .ok_or(FlowGenerationError::ArithmeticOverflow)?;
            let mut demands = vec![1];
            demands.extend(positive_composition(
                total_supply - 1,
                destinations - 1,
                rng,
            )?);
            Ok((supplies, demands))
        }
        TransportationTableShapeV1::DenseUniform { .. }
        | TransportationTableShapeV1::SparseFeasible { .. }
        | TransportationTableShapeV1::Block { .. }
        | TransportationTableShapeV1::NearTie { .. }
        | TransportationTableShapeV1::Monge { .. } => Ok((
            positive_composition(total_supply, origins, rng)?,
            positive_composition(total_supply, destinations, rng)?,
        )),
    }
}

fn transportation_edge_ordinals(
    destinations: u32,
    candidate_count: u64,
    shape: &TransportationTableShapeV1,
    supplies: &[u32],
    demands: &[u32],
    rng: &mut RngV1,
) -> Result<BTreeSet<u64>, FlowGenerationError> {
    match *shape {
        TransportationTableShapeV1::SparseFeasible {
            density_per_mille,
            minimum_cost,
            maximum_cost,
        } => {
            validate_transportation_cost_interval(minimum_cost, maximum_cost)?;
            if density_per_mille == 0 || density_per_mille > 1_000 {
                return Err(FlowGenerationError::Invalid("transportation route density"));
            }
            let required = transportation_northwest_support(destinations, supplies, demands)?;
            let target = candidate_count
                .checked_mul(u64::from(density_per_mille))
                .and_then(|value| value.checked_add(999))
                .and_then(|value| value.checked_div(1_000))
                .ok_or(FlowGenerationError::ArithmeticOverflow)?
                .max(u64::try_from(required.len()).map_err(|_| FlowGenerationError::SizeLimit)?);
            sample_ordinals_excluding(candidate_count, target, &required, rng)
        }
        TransportationTableShapeV1::CutInfeasible {
            minimum_cost,
            maximum_cost,
        } => {
            validate_transportation_cost_interval(minimum_cost, maximum_cost)?;
            Ok((0..candidate_count)
                .filter(|ordinal| *ordinal == 0 || *ordinal / u64::from(destinations) != 0)
                .collect())
        }
        TransportationTableShapeV1::DenseUniform {
            minimum_cost,
            maximum_cost,
        } => {
            validate_transportation_cost_interval(minimum_cost, maximum_cost)?;
            Ok((0..candidate_count).collect())
        }
        TransportationTableShapeV1::UnitDegenerate { .. } => Ok((0..candidate_count).collect()),
        TransportationTableShapeV1::Block { blocks, .. } => {
            if blocks == 0 {
                return Err(FlowGenerationError::Invalid("transportation block count"));
            }
            Ok((0..candidate_count).collect())
        }
        TransportationTableShapeV1::NearTie { gap, .. } => {
            if gap == 0 {
                return Err(FlowGenerationError::Invalid("transportation near-tie gap"));
            }
            Ok((0..candidate_count).collect())
        }
        TransportationTableShapeV1::Monge { scale } => {
            if scale == 0 {
                return Err(FlowGenerationError::Invalid("transportation Monge scale"));
            }
            Ok((0..candidate_count).collect())
        }
    }
}

fn transportation_northwest_support(
    destinations: u32,
    supplies: &[u32],
    demands: &[u32],
) -> Result<BTreeSet<u64>, FlowGenerationError> {
    let mut remaining_supply = supplies.to_vec();
    let mut remaining_demand = demands.to_vec();
    let (mut origin, mut destination) = (0_usize, 0_usize);
    let mut support = BTreeSet::new();
    while origin < remaining_supply.len() && destination < remaining_demand.len() {
        support.insert(
            u64::try_from(origin)
                .ok()
                .and_then(|value| value.checked_mul(u64::from(destinations)))
                .and_then(|value| value.checked_add(u64::try_from(destination).ok()?))
                .ok_or(FlowGenerationError::ArithmeticOverflow)?,
        );
        let shipment = remaining_supply[origin].min(remaining_demand[destination]);
        remaining_supply[origin] -= shipment;
        remaining_demand[destination] -= shipment;
        if remaining_supply[origin] == 0 {
            origin += 1;
        }
        if remaining_demand[destination] == 0 {
            destination += 1;
        }
    }
    Ok(support)
}

fn transportation_pair_cost(
    origin: u32,
    destination: u32,
    destination_count: u32,
    shape: &TransportationTableShapeV1,
    rng: &mut RngV1,
) -> Result<i64, FlowGenerationError> {
    match *shape {
        TransportationTableShapeV1::DenseUniform {
            minimum_cost,
            maximum_cost,
        }
        | TransportationTableShapeV1::SparseFeasible {
            minimum_cost,
            maximum_cost,
            ..
        }
        | TransportationTableShapeV1::CutInfeasible {
            minimum_cost,
            maximum_cost,
        } => sample_uniform_i64(rng, minimum_cost, maximum_cost),
        TransportationTableShapeV1::UnitDegenerate { cost } => Ok(cost),
        TransportationTableShapeV1::Block {
            blocks,
            within_cost,
            between_cost,
        } => Ok(if origin % blocks == destination % blocks {
            within_cost
        } else {
            between_cost
        }),
        TransportationTableShapeV1::NearTie { base_cost, gap } => {
            if origin % destination_count == destination {
                Ok(base_cost)
            } else {
                base_cost
                    .checked_add(i64::from(gap))
                    .ok_or(FlowGenerationError::ArithmeticOverflow)
            }
        }
        TransportationTableShapeV1::Monge { scale } => i64::from(origin.abs_diff(destination))
            .checked_mul(i64::from(scale))
            .ok_or(FlowGenerationError::ArithmeticOverflow),
    }
}

fn validate_transportation_cost_interval(
    minimum: i64,
    maximum: i64,
) -> Result<(), FlowGenerationError> {
    if minimum > maximum {
        return Err(FlowGenerationError::Invalid("transportation cost interval"));
    }
    Ok(())
}

fn transportation_origin_id(index: u32) -> String {
    format!("o{index:04}")
}

fn transportation_destination_id(index: u32) -> String {
    format!("d{index:04}")
}

fn assignment_edge_ordinals(
    agents: u32,
    tasks: u32,
    shape: &AssignmentMatrixShapeV1,
    candidate_count: u64,
    planted_tasks: Option<&[u32]>,
    rng: &mut RngV1,
) -> Result<BTreeSet<u64>, FlowGenerationError> {
    match *shape {
        AssignmentMatrixShapeV1::Uniform {
            density_per_mille,
            minimum_cost,
            maximum_cost,
        } => {
            validate_assignment_cost_interval(minimum_cost, maximum_cost)?;
            let selected = assignment_density_count(candidate_count, density_per_mille, false)?;
            sample_ordinals(candidate_count, selected, rng)
        }
        AssignmentMatrixShapeV1::Equal { .. }
        | AssignmentMatrixShapeV1::Block { .. }
        | AssignmentMatrixShapeV1::NearTie { .. }
        | AssignmentMatrixShapeV1::Monge { .. }
        | AssignmentMatrixShapeV1::AntiMonge { .. } => Ok((0..candidate_count).collect()),
        AssignmentMatrixShapeV1::PlantedOptimum {
            density_per_mille,
            gap,
            ..
        } => {
            if tasks < agents {
                return Err(FlowGenerationError::Invalid(
                    "planted assignment requires at least as many tasks as agents",
                ));
            }
            if gap == 0 {
                return Err(FlowGenerationError::Invalid("planted assignment gap"));
            }
            let selected = assignment_density_count(candidate_count, density_per_mille, true)?
                .max(u64::from(agents));
            let planted = planted_tasks
                .ok_or(FlowGenerationError::Canonicalization)?
                .iter()
                .enumerate()
                .map(|(agent, task)| {
                    Ok(
                        u64::try_from(agent).map_err(|_| FlowGenerationError::SizeLimit)?
                            * u64::from(tasks)
                            + u64::from(*task),
                    )
                })
                .collect::<Result<BTreeSet<_>, FlowGenerationError>>()?;
            sample_ordinals_excluding(candidate_count, selected, &planted, rng)
        }
        AssignmentMatrixShapeV1::SparseAllowed {
            degree,
            minimum_cost,
            maximum_cost,
        } => {
            validate_assignment_cost_interval(minimum_cost, maximum_cost)?;
            if degree > tasks {
                return Err(FlowGenerationError::Invalid("assignment sparse degree"));
            }
            let mut selected = BTreeSet::new();
            for agent in 0..agents {
                for task in sample_ordinals(u64::from(tasks), u64::from(degree), rng)? {
                    selected.insert(u64::from(agent) * u64::from(tasks) + task);
                }
            }
            Ok(selected)
        }
        AssignmentMatrixShapeV1::HallDeficient {
            witness_agents,
            witness_tasks,
            minimum_cost,
            maximum_cost,
        } => {
            validate_assignment_cost_interval(minimum_cost, maximum_cost)?;
            if witness_agents == 0
                || witness_agents > agents
                || witness_tasks >= witness_agents
                || witness_tasks > tasks
            {
                return Err(FlowGenerationError::Invalid(
                    "assignment Hall witness dimensions",
                ));
            }
            let mut selected = BTreeSet::new();
            for agent in 0..agents {
                let adjacent_tasks = if agent < witness_agents {
                    witness_tasks
                } else {
                    tasks
                };
                for task in 0..adjacent_tasks {
                    selected.insert(u64::from(agent) * u64::from(tasks) + u64::from(task));
                }
            }
            Ok(selected)
        }
    }
}

fn assignment_planted_tasks(
    agents: u32,
    tasks: u32,
    shape: &AssignmentMatrixShapeV1,
    rng: &mut RngV1,
) -> Result<Option<Vec<u32>>, FlowGenerationError> {
    if !matches!(shape, AssignmentMatrixShapeV1::PlantedOptimum { .. }) {
        return Ok(None);
    }
    let mut task_order = (0..tasks).collect::<Vec<_>>();
    shuffle_indices(&mut task_order, rng)?;
    task_order.truncate(as_usize(u64::from(agents))?);
    Ok(Some(task_order))
}

fn assignment_pair_cost(
    agent: u32,
    task: u32,
    objective: AssignmentObjectiveV1,
    shape: &AssignmentMatrixShapeV1,
    planted_tasks: Option<&[u32]>,
    rng: &mut RngV1,
) -> Result<i64, FlowGenerationError> {
    match *shape {
        AssignmentMatrixShapeV1::Uniform {
            minimum_cost,
            maximum_cost,
            ..
        }
        | AssignmentMatrixShapeV1::SparseAllowed {
            minimum_cost,
            maximum_cost,
            ..
        }
        | AssignmentMatrixShapeV1::HallDeficient {
            minimum_cost,
            maximum_cost,
            ..
        } => sample_uniform_i64(rng, minimum_cost, maximum_cost),
        AssignmentMatrixShapeV1::Equal { cost } => Ok(cost),
        AssignmentMatrixShapeV1::Block {
            blocks,
            within_cost,
            between_cost,
        } => {
            if blocks == 0 {
                return Err(FlowGenerationError::Invalid("assignment block count"));
            }
            Ok(if agent % blocks == task % blocks {
                within_cost
            } else {
                between_cost
            })
        }
        AssignmentMatrixShapeV1::NearTie { base_cost, gap } => {
            if gap == 0 {
                return Err(FlowGenerationError::Invalid("assignment near-tie gap"));
            }
            if agent == task {
                Ok(base_cost)
            } else {
                objective_oriented_offset(base_cost, gap, objective)
            }
        }
        AssignmentMatrixShapeV1::PlantedOptimum {
            base_cost,
            gap,
            noise,
            ..
        } => {
            let planted = planted_tasks
                .and_then(|tasks| tasks.get(as_usize(u64::from(agent)).ok()?))
                .is_some_and(|planted| *planted == task);
            if planted {
                Ok(base_cost)
            } else {
                let extra = if noise == 0 {
                    0
                } else {
                    u32::try_from(rng.bounded_u64(u64::from(noise) + 1)?)
                        .map_err(|_| FlowGenerationError::ArithmeticOverflow)?
                };
                objective_oriented_offset(
                    base_cost,
                    gap.checked_add(extra)
                        .ok_or(FlowGenerationError::ArithmeticOverflow)?,
                    objective,
                )
            }
        }
        AssignmentMatrixShapeV1::Monge { scale } => {
            assignment_distance_cost(agent, task, scale, false)
        }
        AssignmentMatrixShapeV1::AntiMonge { scale } => {
            assignment_distance_cost(agent, task, scale, true)
        }
    }
}

fn assignment_density_count(
    candidate_count: u64,
    density_per_mille: u32,
    require_positive: bool,
) -> Result<u64, FlowGenerationError> {
    if density_per_mille > 1_000 || (require_positive && density_per_mille == 0) {
        return Err(FlowGenerationError::Invalid("assignment density"));
    }
    candidate_count
        .checked_mul(u64::from(density_per_mille))
        .and_then(|value| value.checked_add(if require_positive { 999 } else { 0 }))
        .and_then(|value| value.checked_div(1_000))
        .ok_or(FlowGenerationError::ArithmeticOverflow)
}

fn sample_ordinals_excluding(
    candidate_count: u64,
    selected_count: u64,
    required: &BTreeSet<u64>,
    rng: &mut RngV1,
) -> Result<BTreeSet<u64>, FlowGenerationError> {
    let required_count =
        u64::try_from(required.len()).map_err(|_| FlowGenerationError::SizeLimit)?;
    if selected_count < required_count || selected_count > candidate_count {
        return Err(FlowGenerationError::Invalid("assignment planted density"));
    }
    let optional_count = candidate_count
        .checked_sub(required_count)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let optional_selected = selected_count
        .checked_sub(required_count)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let sampled_ranks = sample_ordinals(optional_count, optional_selected, rng)?;
    let mut selected = required.clone();
    let mut rank = 0_u64;
    for ordinal in 0..candidate_count {
        if required.contains(&ordinal) {
            continue;
        }
        if sampled_ranks.contains(&rank) {
            selected.insert(ordinal);
        }
        rank = rank
            .checked_add(1)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    }
    Ok(selected)
}

fn objective_oriented_offset(
    base: i64,
    offset: u32,
    objective: AssignmentObjectiveV1,
) -> Result<i64, FlowGenerationError> {
    match objective {
        AssignmentObjectiveV1::Minimize => base.checked_add(i64::from(offset)),
        AssignmentObjectiveV1::Maximize => base.checked_sub(i64::from(offset)),
    }
    .ok_or(FlowGenerationError::ArithmeticOverflow)
}

fn assignment_distance_cost(
    agent: u32,
    task: u32,
    scale: u32,
    negate: bool,
) -> Result<i64, FlowGenerationError> {
    if scale == 0 {
        return Err(FlowGenerationError::Invalid("assignment matrix scale"));
    }
    let distance = agent.abs_diff(task);
    let value = i64::from(distance)
        .checked_mul(i64::from(scale))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if negate {
        value
            .checked_neg()
            .ok_or(FlowGenerationError::ArithmeticOverflow)
    } else {
        Ok(value)
    }
}

fn validate_assignment_cost_interval(
    minimum: i64,
    maximum: i64,
) -> Result<(), FlowGenerationError> {
    if minimum > maximum {
        return Err(FlowGenerationError::Invalid("assignment cost interval"));
    }
    Ok(())
}

fn assignment_agent_id(index: u32) -> String {
    format!("a{index:04}")
}

fn assignment_task_id(index: u32) -> String {
    format!("t{index:04}")
}

fn random_geometric_topology(
    count: u32,
    radius: u32,
    rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(count, 2, MAX_FLOW_NODES, "random geometric node count")?;
    require_range(radius, 1, 1_000, "random geometric radius")?;
    let candidate_count = u64::from(count)
        .checked_mul(u64::from(count - 1))
        .and_then(|value| value.checked_div(2))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    // This family declares a safe edge upper bound before allocation. A
    // spatial acceleration structure can raise the node cap in a later
    // generator revision without changing this sample space.
    enforce_graph_limits(as_usize(u64::from(count))?, as_usize(candidate_count)?)?;

    let count_usize = as_usize(u64::from(count))?;
    let mut coordinates = Vec::with_capacity(count_usize);
    let mut nodes = Vec::with_capacity(count_usize);
    for index in 0..count {
        let x = i64::try_from(40_u64 + rng.bounded_u64(821)?)
            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        let y = i64::try_from(40_u64 + rng.bounded_u64(461)?)
            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
        coordinates.push((x, y));
        nodes.push(positioned_node(&format!("v{index:04}"), x, y));
    }
    let squared_radius = i128::from(radius) * i128::from(radius);
    let mut edges = Vec::new();
    for from in 0..count {
        for to in from + 1..count {
            let from_index = as_usize(u64::from(from))?;
            let to_index = as_usize(u64::from(to))?;
            let (from_x, from_y) = coordinates[from_index];
            let (to_x, to_y) = coordinates[to_index];
            let delta_x = i128::from(from_x - to_x);
            let delta_y = i128::from(from_y - to_y);
            if delta_x * delta_x + delta_y * delta_y <= squared_radius {
                edges.push((format!("v{from:04}"), format!("v{to:04}")));
            }
        }
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: max_flow_model("v0000", &format!("v{:04}", count - 1)),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn shuffle_indices(values: &mut [u32], rng: &mut RngV1) -> Result<(), FlowGenerationError> {
    for index in (1..values.len()).rev() {
        let bound = u64::try_from(index + 1).map_err(|_| FlowGenerationError::SizeLimit)?;
        let other =
            usize::try_from(rng.bounded_u64(bound)?).map_err(|_| FlowGenerationError::SizeLimit)?;
        values.swap(index, other);
    }
    Ok(())
}

fn random_regular_directed_topology(
    count: u32,
    degree: u32,
    rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(count, 2, MAX_FLOW_NODES, "random regular node count")?;
    require_range(
        degree,
        1,
        usize::try_from(count - 1).map_err(|_| FlowGenerationError::SizeLimit)?,
        "random regular degree",
    )?;
    let edge_count = u64::from(count)
        .checked_mul(u64::from(degree))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(u64::from(count))?, as_usize(edge_count)?)?;

    let offsets = sample_ordinals(u64::from(count - 1), u64::from(degree), rng)?
        .into_iter()
        .map(|offset| u32::try_from(offset + 1).map_err(|_| FlowGenerationError::SizeLimit))
        .collect::<Result<Vec<_>, _>>()?;
    let mut permutation = (0..count).collect::<Vec<_>>();
    shuffle_indices(&mut permutation, rng)?;
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    for rank in 0..count {
        for &offset in &offsets {
            let from = permutation[as_usize(u64::from(rank))?];
            let target_rank = (rank + offset) % count;
            let to = permutation[as_usize(u64::from(target_rank))?];
            edges.push((format!("v{from:04}"), format!("v{to:04}")));
        }
    }
    edges.sort_unstable();
    Ok(Topology {
        nodes: circular_nodes(count)?,
        edges,
        suggested_model: max_flow_model("v0000", &format!("v{:04}", count - 1)),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn preferential_attachment_topology(
    count: u32,
    attachment_count: u32,
    rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(
        count,
        2,
        MAX_FLOW_NODES,
        "preferential attachment node count",
    )?;
    require_range(
        attachment_count,
        1,
        usize::try_from(count - 1).map_err(|_| FlowGenerationError::SizeLimit)?,
        "preferential attachment count",
    )?;
    let seed_count = u64::from(attachment_count) + 1;
    let seed_edges = seed_count
        .checked_mul(seed_count - 1)
        .and_then(|value| value.checked_div(2))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let growth_edges = u64::from(count)
        .checked_sub(seed_count)
        .and_then(|remaining| remaining.checked_mul(u64::from(attachment_count)))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edge_count = seed_edges
        .checked_add(growth_edges)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(u64::from(count))?, as_usize(edge_count)?)?;

    let mut degrees = vec![0_u64; as_usize(u64::from(count))?];
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    let seed_count_u32 = u32::try_from(seed_count).map_err(|_| FlowGenerationError::SizeLimit)?;
    for from in 0..seed_count_u32 {
        for to in from + 1..seed_count_u32 {
            edges.push((format!("v{from:04}"), format!("v{to:04}")));
            degrees[as_usize(u64::from(from))?] += 1;
            degrees[as_usize(u64::from(to))?] += 1;
        }
    }

    for new_node in seed_count_u32..count {
        let mut selected = BTreeSet::new();
        for _ in 0..attachment_count {
            let total_weight = (0..new_node)
                .filter(|candidate| !selected.contains(candidate))
                .try_fold(0_u64, |total, candidate| {
                    total
                        .checked_add(degrees[as_usize(u64::from(candidate))?])
                        .ok_or(FlowGenerationError::ArithmeticOverflow)
                })?;
            if total_weight == 0 {
                return Err(FlowGenerationError::Canonicalization);
            }
            let mut draw = rng.bounded_u64(total_weight)?;
            let mut chosen = None;
            for candidate in 0..new_node {
                if selected.contains(&candidate) {
                    continue;
                }
                let weight = degrees[as_usize(u64::from(candidate))?];
                if draw < weight {
                    chosen = Some(candidate);
                    break;
                }
                draw -= weight;
            }
            selected.insert(chosen.ok_or(FlowGenerationError::Canonicalization)?);
        }
        for target in selected {
            edges.push((format!("v{target:04}"), format!("v{new_node:04}")));
            degrees[as_usize(u64::from(target))?] += 1;
            degrees[as_usize(u64::from(new_node))?] += 1;
        }
    }
    Ok(Topology {
        nodes: circular_nodes(count)?,
        edges,
        suggested_model: max_flow_model("v0000", &format!("v{:04}", count - 1)),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn planar_triangulated_topology(count: u32) -> Result<Topology, FlowGenerationError> {
    require_range(count, 3, MAX_FLOW_NODES, "planar triangulated node count")?;
    let edge_count = u64::from(count)
        .checked_mul(2)
        .and_then(|value| value.checked_sub(3))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(u64::from(count))?, as_usize(edge_count)?)?;
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    for index in 0..count - 1 {
        edges.push((format!("v{index:04}"), format!("v{:04}", index + 1)));
    }
    edges.push(("v0000".to_owned(), format!("v{:04}", count - 1)));
    for target in 2..count - 1 {
        edges.push(("v0000".to_owned(), format!("v{target:04}")));
    }
    Ok(Topology {
        nodes: circular_nodes(count)?,
        edges,
        suggested_model: planar_fan_model(count),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn planar_fan_dart(edge: u32, direction: FlowPlanarDartDirectionV1) -> FlowPlanarDartV1 {
    FlowPlanarDartV1 {
        edge_id: format!("e{edge:06}"),
        direction,
    }
}

fn planar_fan_model(count: u32) -> FlowProblemModelV1 {
    let mut rotations = Vec::with_capacity(count as usize);
    let mut source_darts = Vec::with_capacity(count as usize);
    source_darts.push(planar_fan_dart(0, FlowPlanarDartDirectionV1::Forward));
    for target in 2..count - 1 {
        source_darts.push(planar_fan_dart(
            count + target - 2,
            FlowPlanarDartDirectionV1::Forward,
        ));
    }
    source_darts.push(planar_fan_dart(
        count - 1,
        FlowPlanarDartDirectionV1::Forward,
    ));
    rotations.push(FlowPlanarRotationV1 {
        node_id: "v0000".to_owned(),
        darts: source_darts,
    });
    rotations.push(FlowPlanarRotationV1 {
        node_id: "v0001".to_owned(),
        darts: vec![
            planar_fan_dart(0, FlowPlanarDartDirectionV1::Reverse),
            planar_fan_dart(1, FlowPlanarDartDirectionV1::Forward),
        ],
    });
    for index in 2..count - 1 {
        rotations.push(FlowPlanarRotationV1 {
            node_id: format!("v{index:04}"),
            darts: vec![
                planar_fan_dart(index - 1, FlowPlanarDartDirectionV1::Reverse),
                planar_fan_dart(index, FlowPlanarDartDirectionV1::Forward),
                planar_fan_dart(count + index - 2, FlowPlanarDartDirectionV1::Reverse),
            ],
        });
    }
    rotations.push(FlowPlanarRotationV1 {
        node_id: format!("v{:04}", count - 1),
        darts: vec![
            planar_fan_dart(count - 2, FlowPlanarDartDirectionV1::Reverse),
            planar_fan_dart(count - 1, FlowPlanarDartDirectionV1::Reverse),
        ],
    });

    FlowProblemModelV1::PlanarMaxFlow {
        source: "v0000".to_owned(),
        sink: format!("v{:04}", count - 1),
        embedding: FlowPlanarEmbeddingV1 {
            rotations,
            outer_face: planar_fan_dart(count - 1, FlowPlanarDartDirectionV1::Forward),
            terminal_corners: Some(FlowPlanarTerminalCornersV1 {
                source: planar_fan_dart(count - 1, FlowPlanarDartDirectionV1::Forward),
                sink: planar_fan_dart(count - 2, FlowPlanarDartDirectionV1::Reverse),
            }),
        },
    }
}

fn multi_source_sink_topology(
    sources: u32,
    intermediate: u32,
    sinks: u32,
) -> Result<Topology, FlowGenerationError> {
    require_range(sources, 1, MAX_FLOW_NODES, "multi-source count")?;
    require_range(
        intermediate,
        1,
        MAX_FLOW_NODES,
        "multi-source intermediate count",
    )?;
    require_range(sinks, 1, MAX_FLOW_NODES, "multi-sink count")?;
    let node_count = 2_u64
        .checked_add(u64::from(sources))
        .and_then(|value| value.checked_add(u64::from(intermediate)))
        .and_then(|value| value.checked_add(u64::from(sinks)))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edge_count = u64::from(sources)
        .checked_add(
            u64::from(sources)
                .checked_mul(u64::from(intermediate))
                .ok_or(FlowGenerationError::ArithmeticOverflow)?,
        )
        .and_then(|value| value.checked_add(u64::from(intermediate).checked_mul(u64::from(sinks))?))
        .and_then(|value| value.checked_add(u64::from(sinks)))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;

    let mut nodes = vec![
        positioned_node("s", 40, 270),
        positioned_node("t", 860, 270),
    ];
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    for source in 0..sources {
        let id = format!("u{source:04}");
        nodes.push(positioned_node(
            &id,
            210,
            interpolate(40, 500, source + 1, sources + 1)?,
        ));
        edges.push(("s".to_owned(), id));
    }
    for connector in 0..intermediate {
        nodes.push(positioned_node(
            &format!("v{connector:04}"),
            450,
            interpolate(40, 500, connector + 1, intermediate + 1)?,
        ));
    }
    for sink in 0..sinks {
        nodes.push(positioned_node(
            &format!("w{sink:04}"),
            690,
            interpolate(40, 500, sink + 1, sinks + 1)?,
        ));
    }
    for source in 0..sources {
        for connector in 0..intermediate {
            edges.push((format!("u{source:04}"), format!("v{connector:04}")));
        }
    }
    for connector in 0..intermediate {
        for sink in 0..sinks {
            edges.push((format!("v{connector:04}"), format!("w{sink:04}")));
        }
    }
    for sink in 0..sinks {
        edges.push((format!("w{sink:04}"), "t".to_owned()));
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn unordered_pair_from_ordinal(
    count: u32,
    ordinal: u64,
) -> Result<(u32, u32), FlowGenerationError> {
    let prefix = |from: u32| -> u128 {
        let from = u128::from(from);
        from * (2 * u128::from(count) - from - 1) / 2
    };
    let mut low = 0_u32;
    let mut high = count - 1;
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if prefix(middle) <= u128::from(ordinal) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let from = low;
    let offset = u128::from(ordinal)
        .checked_sub(prefix(from))
        .ok_or(FlowGenerationError::Canonicalization)?;
    let to = u128::from(from) + 1 + offset;
    Ok((
        from,
        u32::try_from(to).map_err(|_| FlowGenerationError::Canonicalization)?,
    ))
}

fn random_dag_topology(
    count: u32,
    edge_count: u32,
    rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(count, 2, MAX_FLOW_NODES, "random DAG node count")?;
    let candidate_count = u64::from(count)
        .checked_mul(u64::from(count - 1))
        .and_then(|value| value.checked_div(2))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if u64::from(edge_count) > candidate_count {
        return Err(FlowGenerationError::Invalid("random DAG edge count"));
    }
    enforce_graph_limits(
        as_usize(u64::from(count))?,
        as_usize(u64::from(edge_count))?,
    )?;
    let edges = sample_ordinals(candidate_count, u64::from(edge_count), rng)?
        .into_iter()
        .map(|ordinal| {
            let (from, to) = unordered_pair_from_ordinal(count, ordinal)?;
            Ok((format!("v{from:04}"), format!("v{to:04}")))
        })
        .collect::<Result<Vec<_>, FlowGenerationError>>()?;
    Ok(Topology {
        nodes: circular_nodes(count)?,
        edges,
        suggested_model: max_flow_model("v0000", &format!("v{:04}", count - 1)),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn watts_strogatz_fixed_topology(
    count: u32,
    neighborhood: u32,
    rewire_count: u32,
    rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    const MAX_REWIRE_SCAN_WORK: u64 = 5_000_000;
    require_range(count, 4, MAX_FLOW_NODES, "small-world node count")?;
    if neighborhood < 2 || neighborhood >= count || !neighborhood.is_multiple_of(2) {
        return Err(FlowGenerationError::Invalid(
            "small-world even neighborhood",
        ));
    }
    let half = neighborhood / 2;
    let edge_count = u64::from(count)
        .checked_mul(u64::from(half))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if u64::from(rewire_count) > edge_count {
        return Err(FlowGenerationError::Invalid("small-world rewire count"));
    }
    let scan_work = u64::from(count)
        .checked_mul(u64::from(rewire_count))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if scan_work > MAX_REWIRE_SCAN_WORK {
        return Err(FlowGenerationError::Invalid(
            "small-world deterministic rewiring work",
        ));
    }
    enforce_graph_limits(as_usize(u64::from(count))?, as_usize(edge_count)?)?;

    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    let mut outgoing = vec![BTreeSet::new(); as_usize(u64::from(count))?];
    for from in 0..count {
        for offset in 1..=half {
            let to = (from + offset) % count;
            edges.push((from, to));
            outgoing[as_usize(u64::from(from))?].insert(to);
        }
    }
    for ordinal in sample_ordinals(edge_count, u64::from(rewire_count), rng)? {
        let edge_index = as_usize(ordinal)?;
        let (from, old_to) = edges[edge_index];
        let used = &mut outgoing[as_usize(u64::from(from))?];
        if !used.remove(&old_to) {
            return Err(FlowGenerationError::Canonicalization);
        }
        let available_count = u64::from(count - 1)
            .checked_sub(u64::try_from(used.len()).map_err(|_| FlowGenerationError::SizeLimit)?)
            .ok_or(FlowGenerationError::Canonicalization)?;
        let mut rank = rng.bounded_u64(available_count)?;
        let mut selected = None;
        for candidate in 0..count {
            if candidate == from || used.contains(&candidate) {
                continue;
            }
            if rank == 0 {
                selected = Some(candidate);
                break;
            }
            rank -= 1;
        }
        let new_to = selected.ok_or(FlowGenerationError::Canonicalization)?;
        used.insert(new_to);
        edges[edge_index].1 = new_to;
    }
    Ok(Topology {
        nodes: circular_nodes(count)?,
        edges: edges
            .into_iter()
            .map(|(from, to)| (format!("v{from:04}"), format!("v{to:04}")))
            .collect(),
        suggested_model: max_flow_model("v0000", &format!("v{:04}", count - 1)),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn integer_ceil_sqrt(value: u32) -> u32 {
    let mut root = 1_u32;
    while root.saturating_mul(root) < value {
        root += 1;
    }
    root
}

fn clustered_directed_topology(
    clusters: u32,
    cluster_size: u32,
    bridge_edges: u32,
    rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(clusters, 2, MAX_FLOW_NODES, "cluster count")?;
    require_range(cluster_size, 2, MAX_FLOW_NODES, "cluster size")?;
    let node_count = u64::from(clusters)
        .checked_mul(u64::from(cluster_size))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let candidates_per_node = u64::from(cluster_size)
        .checked_mul(u64::from(clusters - 1))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let candidate_count = node_count
        .checked_mul(candidates_per_node)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if u64::from(bridge_edges) > candidate_count {
        return Err(FlowGenerationError::Invalid("cluster bridge edge count"));
    }
    let edge_count = node_count
        .checked_add(u64::from(bridge_edges))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;

    let columns = integer_ceil_sqrt(clusters);
    let rows = clusters.div_ceil(columns);
    let mut nodes = Vec::with_capacity(as_usize(node_count)?);
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    for cluster in 0..clusters {
        let column = cluster % columns;
        let row = cluster / columns;
        let cell_left = interpolate(80, 820, column, columns)?;
        let cell_right = interpolate(80, 820, column + 1, columns)?;
        let cell_top = interpolate(80, 460, row, rows)?;
        let cell_bottom = interpolate(80, 460, row + 1, rows)?;
        let local_columns = integer_ceil_sqrt(cluster_size);
        let local_rows = cluster_size.div_ceil(local_columns);
        for local in 0..cluster_size {
            let id = format!("c{cluster:03}n{local:04}");
            nodes.push(positioned_node(
                &id,
                interpolate(
                    cell_left,
                    cell_right,
                    local % local_columns + 1,
                    local_columns + 1,
                )?,
                interpolate(
                    cell_top,
                    cell_bottom,
                    local / local_columns + 1,
                    local_rows + 1,
                )?,
            ));
            edges.push((
                id,
                format!("c{cluster:03}n{:04}", (local + 1) % cluster_size),
            ));
        }
    }
    for ordinal in sample_ordinals(candidate_count, u64::from(bridge_edges), rng)? {
        let from_ordinal = ordinal / candidates_per_node;
        let target_compact = ordinal % candidates_per_node;
        let from_cluster = from_ordinal / u64::from(cluster_size);
        let from_local = from_ordinal % u64::from(cluster_size);
        let mut target_cluster = target_compact / u64::from(cluster_size);
        let target_local = target_compact % u64::from(cluster_size);
        if target_cluster >= from_cluster {
            target_cluster += 1;
        }
        edges.push((
            format!("c{from_cluster:03}n{from_local:04}"),
            format!("c{target_cluster:03}n{target_local:04}"),
        ));
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: max_flow_model(
            "c000n0000",
            &format!("c{:03}n{:04}", clusters - 1, cluster_size - 1),
        ),
        fixed_capacities: None,
        fixed_costs: None,
    })
}

fn planted_bottleneck_topology(
    left: u32,
    right: u32,
    cut_edges: u32,
    rng: &mut RngV1,
) -> Result<Topology, FlowGenerationError> {
    require_range(left, 1, MAX_FLOW_NODES, "bottleneck left size")?;
    require_range(right, 1, MAX_FLOW_NODES, "bottleneck right size")?;
    require_range(cut_edges, 1, MAX_FLOW_EDGES, "bottleneck cut edge count")?;
    let candidate_count = u64::from(left)
        .checked_mul(u64::from(right))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    if u64::from(cut_edges) > candidate_count {
        return Err(FlowGenerationError::Invalid("bottleneck cut edge count"));
    }
    let node_count = 2_u64 + u64::from(left) + u64::from(right);
    let edge_count = u64::from(left) + u64::from(cut_edges) + u64::from(right);
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;
    let mut nodes = vec![
        positioned_node("s", 40, 270),
        positioned_node("t", 860, 270),
    ];
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(edge_count)?);
    let outer_capacity = u64::from(cut_edges);
    for index in 0..left {
        let id = format!("l{index:04}");
        nodes.push(positioned_node(
            &id,
            280,
            interpolate(40, 500, index + 1, left + 1)?,
        ));
        edges.push(("s".to_owned(), id));
        capacities.push(outer_capacity);
    }
    for index in 0..right {
        nodes.push(positioned_node(
            &format!("r{index:04}"),
            620,
            interpolate(40, 500, index + 1, right + 1)?,
        ));
    }
    for ordinal in sample_ordinals(candidate_count, u64::from(cut_edges), rng)? {
        let from = ordinal / u64::from(right);
        let to = ordinal % u64::from(right);
        edges.push((format!("l{from:04}"), format!("r{to:04}")));
        capacities.push(1);
    }
    for index in 0..right {
        edges.push((format!("r{index:04}"), "t".to_owned()));
        capacities.push(outer_capacity);
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(capacities),
        fixed_costs: Some(vec![0; as_usize(edge_count)?]),
    })
}

fn hall_tight_bipartite_topology(
    part_size: u32,
    tight_prefix: u32,
) -> Result<Topology, FlowGenerationError> {
    require_range(part_size, 2, MAX_FLOW_NODES, "Hall-tight partition size")?;
    if tight_prefix == 0 || tight_prefix >= part_size {
        return Err(FlowGenerationError::Invalid("Hall-tight prefix size"));
    }
    let cross_edges = u64::from(tight_prefix)
        .checked_mul(u64::from(tight_prefix))
        .and_then(|value| {
            value
                .checked_add(u64::from(part_size - tight_prefix).checked_mul(u64::from(part_size))?)
        })
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let node_count = 2_u64 + 2 * u64::from(part_size);
    let edge_count = 2 * u64::from(part_size) + cross_edges;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;
    let mut nodes = vec![
        positioned_node("s", 40, 270),
        positioned_node("t", 860, 270),
    ];
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    for left in 0..part_size {
        let id = format!("l{left:04}");
        nodes.push(positioned_node(
            &id,
            280,
            interpolate(40, 500, left + 1, part_size + 1)?,
        ));
        edges.push(("s".to_owned(), id));
    }
    for right in 0..part_size {
        nodes.push(positioned_node(
            &format!("r{right:04}"),
            620,
            interpolate(40, 500, right + 1, part_size + 1)?,
        ));
    }
    for left in 0..part_size {
        let target_count = if left < tight_prefix {
            tight_prefix
        } else {
            part_size
        };
        for right in 0..target_count {
            edges.push((format!("l{left:04}"), format!("r{right:04}")));
        }
    }
    for right in 0..part_size {
        edges.push((format!("r{right:04}"), "t".to_owned()));
    }
    Ok(Topology {
        nodes,
        edges,
        suggested_model: bipartite_matching_adapter_model(part_size),
        fixed_capacities: Some(vec![1; as_usize(edge_count)?]),
        fixed_costs: Some(vec![0; as_usize(edge_count)?]),
    })
}

fn sample_ordinals(
    candidate_count: u64,
    selected_count: u64,
    rng: &mut RngV1,
) -> Result<BTreeSet<u64>, FlowGenerationError> {
    let mut ordinals = BTreeSet::new();
    for candidate in candidate_count - selected_count..candidate_count {
        let sampled = rng.bounded_u64(candidate + 1)?;
        if !ordinals.insert(sampled) {
            ordinals.insert(candidate);
        }
    }
    Ok(ordinals)
}

fn dinic_worst_case_topology(count: u32) -> Result<Topology, FlowGenerationError> {
    require_range(count, 2, MAX_FLOW_NODES, "Dinic worst-case node count")?;
    let edge_count = u64::from(count)
        .checked_mul(2)
        .and_then(|value| value.checked_sub(3))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(u64::from(count))?, as_usize(edge_count)?)?;
    let nodes = linear_nodes(count, "v")?;
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(edge_count)?);
    for index in 0..count - 1 {
        edges.push((node_id(index, count), node_id(index + 1, count)));
        capacities.push(if index + 2 == count {
            1
        } else {
            u64::from(count)
        });
    }
    for index in 0..count.saturating_sub(2) {
        edges.push((node_id(index, count), "t".to_owned()));
        capacities.push(1);
    }
    debug_assert_eq!(edges.len(), as_usize(edge_count)?);
    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; edges.len()]),
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(capacities),
    })
}

const WASHINGTON_DINIC_STRESS_NODE_LIMIT: usize = 2_000;

fn washington_dinic_stress_topology(count: u32) -> Result<Topology, FlowGenerationError> {
    require_range(
        count,
        2,
        WASHINGTON_DINIC_STRESS_NODE_LIMIT,
        "Washington Dinic phase stress node count",
    )?;
    let edge_count = u64::from(count)
        .checked_mul(2)
        .and_then(|value| value.checked_sub(3))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(u64::from(count))?, as_usize(edge_count)?)?;
    let nodes = linear_nodes(count, "v")?;
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(edge_count)?);
    for index in 0..count - 1 {
        edges.push((node_id(index, count), node_id(index + 1, count)));
        capacities.push(u64::from(count));
    }
    for index in 0..count.saturating_sub(2) {
        edges.push((node_id(index, count), "t".to_owned()));
        capacities.push(1);
    }
    debug_assert_eq!(edges.len(), as_usize(edge_count)?);
    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; edges.len()]),
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(capacities),
    })
}

const WASHINGTON_GOLDBERG_FIFO_STRESS_BLOCK_LIMIT: usize = 64;

fn washington_goldberg_fifo_stress_topology(
    block_size: u32,
) -> Result<Topology, FlowGenerationError> {
    require_range(
        block_size,
        2,
        WASHINGTON_GOLDBERG_FIFO_STRESS_BLOCK_LIMIT,
        "Washington Goldberg FIFO stress block size",
    )?;
    let node_count = u64::from(block_size)
        .checked_mul(3)
        .and_then(|value| value.checked_add(3))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edge_count = u64::from(block_size)
        .checked_mul(4)
        .and_then(|value| value.checked_add(1))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;

    let count = u32::try_from(node_count).map_err(|_| FlowGenerationError::SizeLimit)?;
    let merge = 2 * block_size + 2;
    let mut nodes = Vec::with_capacity(as_usize(node_count)?);
    nodes.push(positioned_node("s", 40, 270));
    nodes.push(positioned_node(&node_id(1, count), 140, 270));
    for branch in 0..block_size {
        let y = interpolate(70, 470, branch, block_size - 1)?;
        nodes.push(positioned_node(&node_id(branch + 2, count), 300, y));
    }
    for branch in 0..block_size {
        let y = interpolate(70, 470, branch, block_size - 1)?;
        nodes.push(positioned_node(
            &node_id(block_size + branch + 2, count),
            460,
            y,
        ));
    }
    nodes.push(positioned_node(&node_id(merge, count), 580, 270));
    for tail in 0..block_size {
        nodes.push(positioned_node(
            &node_id(merge + tail + 1, count),
            interpolate(650, 860, tail, block_size - 1)?,
            270,
        ));
    }

    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(edge_count)?);
    let wide_capacity = u64::from(block_size);
    edges.push(("s".to_owned(), node_id(1, count)));
    capacities.push(wide_capacity);
    for branch in 0..block_size {
        let upper = branch + 2;
        let lower = block_size + branch + 2;
        edges.push((node_id(1, count), node_id(upper, count)));
        capacities.push(wide_capacity);
        edges.push((node_id(upper, count), node_id(lower, count)));
        capacities.push(1);
        edges.push((node_id(lower, count), node_id(merge, count)));
        capacities.push(wide_capacity);
    }
    for tail in merge..merge + block_size {
        edges.push((node_id(tail, count), node_id(tail + 1, count)));
        capacities.push(wide_capacity);
    }

    debug_assert_eq!(nodes.len(), as_usize(node_count)?);
    debug_assert_eq!(edges.len(), as_usize(edge_count)?);
    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; edges.len()]),
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(capacities),
    })
}

const CHERKASSKY_GOLDBERG_AK_SIZE_LIMIT: usize = 128;
const CHERKASSKY_GOLDBERG_AK_TERMINAL_CAPACITY: u64 = 1_000_000;

fn cherkassky_goldberg_ak_stress_topology(size: u32) -> Result<Topology, FlowGenerationError> {
    require_range(
        size,
        2,
        CHERKASSKY_GOLDBERG_AK_SIZE_LIMIT,
        "Cherkassky-Goldberg AK size",
    )?;
    let node_count = u64::from(size)
        .checked_mul(4)
        .and_then(|value| value.checked_add(6))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edge_count = u64::from(size)
        .checked_mul(6)
        .and_then(|value| value.checked_add(7))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;

    let mut nodes = Vec::with_capacity(as_usize(node_count)?);
    nodes.push(positioned_node("s", 40, 270));
    for index in 0..=size {
        nodes.push(positioned_node(
            &format!("a{index:04}"),
            interpolate(145, 510, index, size)?,
            80,
        ));
    }
    for index in 0..=size {
        nodes.push(positioned_node(
            &format!("b{index:04}"),
            interpolate(510, 760, index, size)?,
            190,
        ));
    }
    let second_last = size
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    for index in 0..=second_last {
        let y = if index <= size { 345 } else { 450 };
        nodes.push(positioned_node(
            &format!("c{index:04}"),
            interpolate(145, 760, index, second_last)?,
            y,
        ));
    }
    nodes.push(positioned_node("t", 860, 270));

    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(edge_count)?);
    for index in 0..size {
        edges.push((format!("a{index:04}"), format!("a{:04}", index + 1)));
        capacities.push(u64::from(size - index + 1));
        edges.push((format!("a{index:04}"), "b0000".to_owned()));
        capacities.push(1);
    }
    edges.push((format!("a{size:04}"), format!("b{size:04}")));
    capacities.push(1);
    edges.push((format!("a{size:04}"), "b0000".to_owned()));
    capacities.push(1);
    for index in 0..size {
        edges.push((format!("b{index:04}"), format!("b{:04}", index + 1)));
        capacities.push(u64::from(size + 1));
    }

    for index in 0..second_last {
        edges.push((format!("c{index:04}"), format!("c{:04}", index + 1)));
        capacities.push(u64::from(size));
    }
    for index in 0..size {
        edges.push((
            format!("c{index:04}"),
            format!("c{:04}", second_last - index),
        ));
        capacities.push(1);
    }

    edges.push(("s".to_owned(), "a0000".to_owned()));
    capacities.push(CHERKASSKY_GOLDBERG_AK_TERMINAL_CAPACITY);
    edges.push(("s".to_owned(), "c0000".to_owned()));
    capacities.push(CHERKASSKY_GOLDBERG_AK_TERMINAL_CAPACITY);
    edges.push((format!("b{size:04}"), "t".to_owned()));
    capacities.push(CHERKASSKY_GOLDBERG_AK_TERMINAL_CAPACITY);
    edges.push((format!("c{second_last:04}"), "t".to_owned()));
    capacities.push(CHERKASSKY_GOLDBERG_AK_TERMINAL_CAPACITY);

    debug_assert_eq!(nodes.len(), as_usize(node_count)?);
    debug_assert_eq!(edges.len(), as_usize(edge_count)?);
    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; edges.len()]),
        edges,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(capacities),
    })
}

const WASHINGTON_CHERIYAN_BRIDGE_LIMIT: usize = 64;
const WASHINGTON_CHERIYAN_ENTRY_LIMIT: usize = 12;
const WASHINGTON_CHERIYAN_CHAIN_LIMIT: usize = 10;
const WASHINGTON_CHERIYAN_VERY_BIG: u64 = 1_000_000;

#[derive(Clone, Copy, Debug)]
struct WashingtonCheriyanCounts {
    node_count: u64,
    edge_count: u64,
    chain_nodes: u32,
}

fn washington_cheriyan_stress_topology(
    bridge_width: u32,
    gadget_entries: u32,
    chain_length: u32,
) -> Result<Topology, FlowGenerationError> {
    let counts = validate_washington_cheriyan_shape(bridge_width, gadget_entries, chain_length)?;
    let count = u32::try_from(counts.node_count).map_err(|_| FlowGenerationError::SizeLimit)?;
    let nodes = washington_cheriyan_nodes(bridge_width, counts.chain_nodes, count)?;
    let fixed_edges =
        washington_cheriyan_edges(bridge_width, gadget_entries, chain_length, counts, count)?;

    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; fixed_edges.endpoints.len()]),
        edges: fixed_edges.endpoints,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(fixed_edges.capacities),
    })
}

fn validate_washington_cheriyan_shape(
    bridge_width: u32,
    gadget_entries: u32,
    chain_length: u32,
) -> Result<WashingtonCheriyanCounts, FlowGenerationError> {
    require_range(
        bridge_width,
        1,
        WASHINGTON_CHERIYAN_BRIDGE_LIMIT,
        "Washington Cheriyan bridge width",
    )?;
    require_range(
        gadget_entries,
        1,
        WASHINGTON_CHERIYAN_ENTRY_LIMIT,
        "Washington Cheriyan gadget entry count",
    )?;
    require_range(
        chain_length,
        1,
        WASHINGTON_CHERIYAN_CHAIN_LIMIT,
        "Washington Cheriyan chain length",
    )?;

    // The archived C source prints `4*m*c + n + 6`, but its four Gadget
    // calls, Bridge, and Sink actually allocate `4*m*c + 2*n + 7` nodes.
    // Preserve the construction rather than the stale diagnostic formula.
    let gadget_node_count = u64::from(gadget_entries)
        .checked_mul(u64::from(chain_length))
        .and_then(|value| value.checked_mul(4))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let node_count = gadget_node_count
        .checked_add(u64::from(bridge_width) * 2)
        .and_then(|value| value.checked_add(7))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let gadget_edge_count = u64::from(gadget_entries)
        .checked_mul(u64::from(chain_length) + 1)
        .and_then(|value| value.checked_mul(4))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edge_count = gadget_edge_count
        .checked_add(u64::from(bridge_width) * 3)
        .and_then(|value| value.checked_add(3))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;
    let chain_nodes = gadget_entries
        .checked_mul(chain_length)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    Ok(WashingtonCheriyanCounts {
        node_count,
        edge_count,
        chain_nodes,
    })
}

fn washington_cheriyan_nodes(
    bridge_width: u32,
    chain_nodes: u32,
    count: u32,
) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    let node_count = u64::from(count);
    let mut nodes = Vec::with_capacity(as_usize(node_count)?);
    nodes.push(positioned_node("s", 40, 270));
    nodes.push(positioned_node(&node_id(1, count), 280, 80));
    nodes.push(positioned_node(&node_id(2, count), 280, 460));
    nodes.push(positioned_node(&node_id(3, count), 670, 270));

    let gadget_layouts = [
        (280, 80, 100, 150),
        (280, 460, 100, 390),
        (670, 270, 360, 70),
        (670, 270, 360, 470),
    ];
    let mut next_node = 4_u32;
    for (start_x, start_y, end_x, end_y) in gadget_layouts {
        for ordinal in 0..chain_nodes {
            let progress = ordinal + 1;
            nodes.push(positioned_node(
                &node_id(next_node, count),
                interpolate(start_x, end_x, progress, chain_nodes)?,
                interpolate(start_y, end_y, progress, chain_nodes)?,
            ));
            next_node = next_node
                .checked_add(1)
                .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        }
    }

    let bridge_in = next_node;
    next_node = next_node
        .checked_add(1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let bridge_out = next_node;
    next_node = next_node
        .checked_add(1)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    nodes.push(positioned_node(&node_id(bridge_in, count), 350, 165));
    nodes.push(positioned_node(&node_id(bridge_out, count), 580, 375));
    for branch in 0..bridge_width {
        let left = next_node;
        next_node = next_node
            .checked_add(1)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        let right = next_node;
        next_node = next_node
            .checked_add(1)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        let y = interpolate(185, 355, branch, bridge_width - 1)?;
        nodes.push(positioned_node(&node_id(left, count), 425, y));
        nodes.push(positioned_node(&node_id(right, count), 505, y));
    }
    let sink = next_node;
    nodes.push(positioned_node("t", 860, 270));
    debug_assert_eq!(sink + 1, count);
    debug_assert_eq!(nodes.len(), as_usize(node_count)?);
    Ok(nodes)
}

fn washington_cheriyan_edges(
    bridge_width: u32,
    gadget_entries: u32,
    chain_length: u32,
    counts: WashingtonCheriyanCounts,
    count: u32,
) -> Result<FixedEdgeSet, FlowGenerationError> {
    let mut edges = Vec::with_capacity(as_usize(counts.edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(counts.edge_count)?);
    let gateway_capacity = u64::from(bridge_width);
    let mut next_gadget_node = 4_u32;
    for (from, to) in [(0_u32, 1_u32), (0, 2), (1, 3), (2, 3)] {
        let mut current = to;
        for _ in 0..gadget_entries {
            for _ in 0..chain_length {
                let previous = current;
                current = next_gadget_node;
                next_gadget_node = next_gadget_node
                    .checked_add(1)
                    .ok_or(FlowGenerationError::ArithmeticOverflow)?;
                edges.push((node_id(current, count), node_id(previous, count)));
                capacities.push(WASHINGTON_CHERIYAN_VERY_BIG);
            }
            edges.push((node_id(from, count), node_id(current, count)));
            capacities.push(gateway_capacity);
        }
    }

    let bridge_in = 4 + 4 * counts.chain_nodes;
    let bridge_out = bridge_in + 1;
    edges.push((node_id(1, count), node_id(bridge_in, count)));
    capacities.push(gateway_capacity);
    edges.push((node_id(bridge_out, count), node_id(2, count)));
    capacities.push(gateway_capacity);
    let mut bridge_node = bridge_out + 1;
    for _ in 0..bridge_width {
        let left = bridge_node;
        let right = bridge_node + 1;
        bridge_node = bridge_node
            .checked_add(2)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?;
        edges.push((node_id(bridge_in, count), node_id(left, count)));
        capacities.push(gateway_capacity);
        edges.push((node_id(right, count), node_id(bridge_out, count)));
        capacities.push(gateway_capacity);
        edges.push((node_id(left, count), node_id(right, count)));
        capacities.push(1);
    }
    let sink = bridge_node;
    edges.push((node_id(3, count), node_id(sink, count)));
    capacities.push(WASHINGTON_CHERIYAN_VERY_BIG);

    debug_assert_eq!(next_gadget_node, bridge_in);
    debug_assert_eq!(sink + 1, count);
    debug_assert_eq!(edges.len(), as_usize(counts.edge_count)?);
    Ok(FixedEdgeSet {
        endpoints: edges,
        capacities,
    })
}

fn zadeh_phase_chain_topology(group_size: u32) -> Result<Topology, FlowGenerationError> {
    require_range(group_size, 4, 20, "Zadeh shortest-path group size")?;
    if !group_size.is_multiple_of(4) {
        return Err(FlowGenerationError::Invalid(
            "Zadeh shortest-path group size must be a multiple of four",
        ));
    }
    let rounds = group_size / 4;
    let node_count = u64::from(group_size)
        .checked_mul(3)
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let edge_count = u64::from(group_size)
        .checked_mul(u64::from(group_size))
        .and_then(|value| value.checked_mul(3))
        .map(|value| value / 2)
        .and_then(|value| value.checked_add(u64::from(group_size)))
        .and_then(|value| value.checked_sub(2))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    enforce_graph_limits(as_usize(node_count)?, as_usize(edge_count)?)?;

    let phase_flow = u64::from(group_size)
        .checked_mul(u64::from(group_size))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let trunk_capacity = phase_flow
        .checked_mul(u64::from(rounds))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let connector_capacity = u64::from(group_size);
    let nodes = zadeh_phase_chain_nodes(group_size, rounds, node_count)?;
    let fixed_edges = zadeh_phase_chain_edges(
        group_size,
        rounds,
        edge_count,
        trunk_capacity,
        connector_capacity,
    )?;

    Ok(Topology {
        nodes,
        fixed_costs: Some(vec![0; fixed_edges.endpoints.len()]),
        edges: fixed_edges.endpoints,
        suggested_model: max_flow_model("s", "t"),
        fixed_capacities: Some(fixed_edges.capacities),
    })
}

fn zadeh_phase_chain_nodes(
    group_size: u32,
    rounds: u32,
    node_count: u64,
) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    let mut nodes = Vec::with_capacity(as_usize(node_count)?);
    nodes.push(positioned_node("s", 40, 270));
    nodes.push(positioned_node("lh", 110, 270));
    for stage in 1..rounds {
        let anchor_x = interpolate(140, 330, stage, rounds)?;
        nodes.push(positioned_node(
            &format!("lp{stage:02}"),
            anchor_x.saturating_sub(24),
            350,
        ));
        nodes.push(positioned_node(&format!("la{stage:02}"), anchor_x, 270));
    }
    for index in 0..group_size {
        nodes.push(positioned_node(
            &format!("ns{index:02}"),
            400,
            interpolate(40, 360, index, group_size - 1)?,
        ));
    }
    for index in 0..group_size {
        nodes.push(positioned_node(
            &format!("nt{index:02}"),
            500,
            interpolate(40, 360, index, group_size - 1)?,
        ));
    }
    for stage in (1..rounds).rev() {
        let anchor_x = interpolate(570, 760, rounds - stage, rounds)?;
        nodes.push(positioned_node(&format!("ra{stage:02}"), anchor_x, 270));
        nodes.push(positioned_node(
            &format!("rp{stage:02}"),
            anchor_x.saturating_add(24),
            350,
        ));
    }
    nodes.push(positioned_node("rh", 790, 270));
    nodes.push(positioned_node("t", 860, 270));
    Ok(nodes)
}

struct FixedEdgeSet {
    endpoints: Vec<(String, String)>,
    capacities: Vec<u64>,
}

fn zadeh_phase_chain_edges(
    group_size: u32,
    rounds: u32,
    edge_count: u64,
    trunk_capacity: u64,
    connector_capacity: u64,
) -> Result<FixedEdgeSet, FlowGenerationError> {
    let mut edges = Vec::with_capacity(as_usize(edge_count)?);
    let mut capacities = Vec::with_capacity(as_usize(edge_count)?);
    let mut add = |from: String, to: String, capacity: u64| {
        edges.push((from, to));
        capacities.push(capacity);
    };

    // This phase-chain form preserves Zadeh's n^3 / 108 augmentation
    // mechanism while making the stable shortest-path order explicit.
    add("s".to_owned(), "lh".to_owned(), trunk_capacity);
    for stage in 1..rounds {
        let previous_anchor = if stage == 1 {
            "lh".to_owned()
        } else {
            format!("la{:02}", stage - 1)
        };
        add(previous_anchor, format!("lp{stage:02}"), trunk_capacity);
        add(
            format!("lp{stage:02}"),
            format!("la{stage:02}"),
            trunk_capacity,
        );
    }
    for stage in 1..rounds {
        let previous_anchor = if stage == 1 {
            "rh".to_owned()
        } else {
            format!("ra{:02}", stage - 1)
        };
        add(
            format!("ra{stage:02}"),
            format!("rp{stage:02}"),
            trunk_capacity,
        );
        add(format!("rp{stage:02}"), previous_anchor, trunk_capacity);
    }
    add("rh".to_owned(), "t".to_owned(), trunk_capacity);

    for stage in 0..rounds {
        let left_anchor = if stage == 0 {
            "lh".to_owned()
        } else {
            format!("la{stage:02}")
        };
        let right_anchor = if stage == 0 {
            "rh".to_owned()
        } else {
            format!("ra{stage:02}")
        };
        for index in 0..group_size {
            if stage % 2 == 0 {
                add(
                    left_anchor.clone(),
                    format!("ns{index:02}"),
                    connector_capacity,
                );
                add(
                    format!("nt{index:02}"),
                    right_anchor.clone(),
                    connector_capacity,
                );
            } else {
                add(
                    left_anchor.clone(),
                    format!("nt{index:02}"),
                    connector_capacity,
                );
                add(
                    format!("ns{index:02}"),
                    right_anchor.clone(),
                    connector_capacity,
                );
            }
        }
    }
    for source_index in 0..group_size {
        for sink_index in 0..group_size {
            add(
                format!("ns{source_index:02}"),
                format!("nt{sink_index:02}"),
                1,
            );
        }
    }

    debug_assert_eq!(edges.len(), as_usize(edge_count)?);
    Ok(FixedEdgeSet {
        endpoints: edges,
        capacities,
    })
}

fn linear_nodes(count: u32, _prefix: &str) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    (0..count)
        .map(|index| {
            Ok(positioned_node(
                &node_id(index, count),
                interpolate(40, 860, index, count - 1)?,
                270,
            ))
        })
        .collect()
}

fn circular_nodes(count: u32) -> Result<Vec<FlowNodeV1>, FlowGenerationError> {
    // Integer-only perimeter placement avoids platform-dependent trigonometry.
    let side = count.div_ceil(4).max(1);
    (0..count)
        .map(|index| {
            let segment = index / side;
            let offset = index % side;
            let coordinate = interpolate(90, 810, offset, side)?;
            let (x, y) = match segment {
                0 => (coordinate, 70),
                1 => (810, interpolate(70, 470, offset, side)?),
                2 => (810 - (coordinate - 90), 470),
                _ => (90, 470 - (interpolate(70, 470, offset, side)? - 70)),
            };
            Ok(positioned_node(&format!("v{index:04}"), x, y))
        })
        .collect()
}

fn st_topology(nodes: Vec<FlowNodeV1>, edges: Vec<(String, String)>, count: u32) -> Topology {
    Topology {
        nodes,
        edges,
        suggested_model: max_flow_model(&node_id(0, count), &node_id(count - 1, count)),
        fixed_capacities: None,
        fixed_costs: None,
    }
}

fn positioned_node(id: &str, x: i64, y: i64) -> FlowNodeV1 {
    FlowNodeV1 {
        id: id.to_owned(),
        supply: "0".to_owned(),
        position: Some(FlowPositionV1 {
            x: x.to_string(),
            y: y.to_string(),
        }),
    }
}

fn node_id(index: u32, count: u32) -> String {
    if index == 0 {
        "s".to_owned()
    } else if index + 1 == count {
        "t".to_owned()
    } else {
        format!("v{index:04}")
    }
}

fn grid_id(row: u32, column: u32) -> String {
    format!("r{row:04}c{column:04}")
}

fn vision_grid_id(row: u32, column: u32) -> String {
    format!("p{row:04}c{column:04}")
}

fn waissi_transit_id(row: u32, column: u32) -> String {
    format!("tr{row:03}c{column:03}")
}

fn grid_3d_id(layer: u32, row: u32, column: u32) -> String {
    format!("z{layer:03}r{row:03}c{column:03}")
}

fn rmfgen_id(frame: u32, row: u32, column: u32) -> String {
    format!("f{frame:03}r{row:03}c{column:03}")
}

fn gridgen_id(row: u32, column: u32) -> String {
    format!("g{row:04}c{column:04}")
}

fn gridgraph_id(row: u32, column: u32) -> String {
    format!("q{row:04}c{column:04}")
}

fn max_flow_model(source: &str, sink: &str) -> FlowProblemModelV1 {
    FlowProblemModelV1::MaxFlow {
        source: source.to_owned(),
        sink: sink.to_owned(),
    }
}

fn bipartite_matching_adapter_model(part_size: u32) -> FlowProblemModelV1 {
    FlowProblemModelV1::BipartiteMatching {
        left: (0..part_size).map(|index| format!("l{index:04}")).collect(),
        right: (0..part_size).map(|index| format!("r{index:04}")).collect(),
        flow_adapter: Some(FlowBipartiteAdapterV1 {
            source: "s".to_owned(),
            sink: "t".to_owned(),
        }),
    }
}

fn interpolate(
    start: i64,
    end: i64,
    numerator: u32,
    denominator: u32,
) -> Result<i64, FlowGenerationError> {
    if denominator == 0 {
        return Ok(start.midpoint(end));
    }
    let delta = i128::from(end - start)
        .checked_mul(i128::from(numerator))
        .and_then(|value| value.checked_div(i128::from(denominator)))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    i64::try_from(i128::from(start) + delta).map_err(|_| FlowGenerationError::ArithmeticOverflow)
}

fn require_range(
    value: u32,
    minimum: u32,
    maximum: usize,
    field: &'static str,
) -> Result<(), FlowGenerationError> {
    if value < minimum || usize::try_from(value).map_or(true, |value| value > maximum) {
        return Err(FlowGenerationError::Invalid(field));
    }
    Ok(())
}

fn enforce_graph_limits(nodes: usize, edges: usize) -> Result<(), FlowGenerationError> {
    if nodes > MAX_FLOW_NODES || edges > MAX_FLOW_EDGES {
        return Err(FlowGenerationError::SizeLimit);
    }
    Ok(())
}

fn as_usize(value: u64) -> Result<usize, FlowGenerationError> {
    usize::try_from(value).map_err(|_| FlowGenerationError::SizeLimit)
}

fn family_id(family: &FlowGeneratorFamilyV1) -> &'static str {
    match family {
        FlowGeneratorFamilyV1::Path { .. } => "path",
        FlowGeneratorFamilyV1::Cycle { .. } => "cycle",
        FlowGeneratorFamilyV1::ParallelPaths { .. } => "parallel-paths",
        FlowGeneratorFamilyV1::DiamondChain { .. } => "diamond-chain",
        FlowGeneratorFamilyV1::Ladder { .. } => "ladder",
        FlowGeneratorFamilyV1::LayeredDag { .. } => "layered-dag",
        FlowGeneratorFamilyV1::CompleteDag { .. } => "complete-dag",
        FlowGeneratorFamilyV1::Grid2d { .. } => "grid-2d",
        FlowGeneratorFamilyV1::VisionSegmentationGrid { .. } => "vision-segmentation-grid",
        FlowGeneratorFamilyV1::Torus { .. } => "torus",
        FlowGeneratorFamilyV1::ErdosRenyiDirected { .. } => "erdos-renyi-directed",
        FlowGeneratorFamilyV1::DinicWorstCase { .. } => "dinic-worst-case",
        FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { .. } => "washington-dinic-phase-stress",
        FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { .. } => {
            "washington-goldberg-fifo-stress"
        }
        FlowGeneratorFamilyV1::WashingtonCheriyanStress { .. } => "washington-cheriyan-stress",
        FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { .. } => "cherkassky-goldberg-ak-stress",
        FlowGeneratorFamilyV1::GloverDenseAcyclicStress { .. } => "glover-dense-acyclic-stress",
        FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { .. } => "waissi-setubal-acyclic-dense",
        FlowGeneratorFamilyV1::WaissiTransitOneWayGrid { .. } => "waissi-transit-one-way-grid",
        FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid { .. } => "waissi-transit-two-way-grid",
        FlowGeneratorFamilyV1::GoldbergMeshCirculation { .. } => "goldberg-mesh-circulation",
        FlowGeneratorFamilyV1::WashingtonMatching { .. } => "washington-matching",
        FlowGeneratorFamilyV1::WashingtonSquareMesh { .. } => "washington-square-mesh",
        FlowGeneratorFamilyV1::WashingtonBasicLine { .. } => "washington-basic-line",
        FlowGeneratorFamilyV1::WashingtonExponentialLine { .. } => "washington-exponential-line",
        FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine { .. } => {
            "washington-double-exponential-line"
        }
        FlowGeneratorFamilyV1::ZadehPhaseChainStress { .. } => "zadeh-phase-chain-stress",
        FlowGeneratorFamilyV1::Arborescence { .. } => "arborescence",
        FlowGeneratorFamilyV1::StronglyConnected { .. } => "strongly-connected",
        FlowGeneratorFamilyV1::Grid3d { .. } => "grid-3d",
        FlowGeneratorFamilyV1::BipartiteRandom { .. } => "bipartite-random",
        FlowGeneratorFamilyV1::AssignmentMatrix { .. } => "assignment-matrix",
        FlowGeneratorFamilyV1::TransportationTable { .. } => "transportation-table",
        FlowGeneratorFamilyV1::RandomGeometric { .. } => "random-geometric",
        FlowGeneratorFamilyV1::RandomRegularDirected { .. } => "random-regular-directed",
        FlowGeneratorFamilyV1::PreferentialAttachmentDirected { .. } => {
            "preferential-attachment-directed"
        }
        FlowGeneratorFamilyV1::PlanarTriangulated { .. } => "planar-triangulated",
        FlowGeneratorFamilyV1::MultiSourceSink { .. } => "multi-source-sink",
        FlowGeneratorFamilyV1::RandomDag { .. } => "random-dag",
        FlowGeneratorFamilyV1::WattsStrogatzFixed { .. } => "watts-strogatz-fixed",
        FlowGeneratorFamilyV1::ClusteredDirected { .. } => "clustered-directed",
        FlowGeneratorFamilyV1::PlantedBottleneck { .. } => "planted-bottleneck",
        FlowGeneratorFamilyV1::HallTightBipartite { .. } => "hall-tight-bipartite",
        FlowGeneratorFamilyV1::RmfgenFrames { .. } => "rmfgen-frames",
        FlowGeneratorFamilyV1::GridgenGrid { .. } => "gridgen-grid",
        FlowGeneratorFamilyV1::GridgraphGrid { .. } => "gridgraph-grid",
        FlowGeneratorFamilyV1::WashingtonMesh { .. } => "washington-mesh",
        FlowGeneratorFamilyV1::WashingtonRandomLevel { .. } => "washington-random-level",
        FlowGeneratorFamilyV1::GotoTorus { .. } => "goto-torus",
        FlowGeneratorFamilyV1::NetgenSkeleton { .. } => "netgen-skeleton",
    }
}

fn assignment_shape_id(shape: &AssignmentMatrixShapeV1) -> &'static str {
    match shape {
        AssignmentMatrixShapeV1::Uniform { .. } => "uniform",
        AssignmentMatrixShapeV1::Equal { .. } => "equal",
        AssignmentMatrixShapeV1::Block { .. } => "block",
        AssignmentMatrixShapeV1::NearTie { .. } => "near-tie",
        AssignmentMatrixShapeV1::PlantedOptimum { .. } => "planted-optimum",
        AssignmentMatrixShapeV1::Monge { .. } => "monge",
        AssignmentMatrixShapeV1::AntiMonge { .. } => "anti-monge",
        AssignmentMatrixShapeV1::SparseAllowed { .. } => "sparse-allowed",
        AssignmentMatrixShapeV1::HallDeficient { .. } => "hall-deficient",
    }
}

fn transportation_shape_id(shape: &TransportationTableShapeV1) -> &'static str {
    match shape {
        TransportationTableShapeV1::DenseUniform { .. } => "dense-uniform",
        TransportationTableShapeV1::SparseFeasible { .. } => "sparse-feasible",
        TransportationTableShapeV1::UnitDegenerate { .. } => "unit-degenerate",
        TransportationTableShapeV1::Block { .. } => "block",
        TransportationTableShapeV1::NearTie { .. } => "near-tie",
        TransportationTableShapeV1::Monge { .. } => "monge",
        TransportationTableShapeV1::CutInfeasible { .. } => "cut-infeasible",
    }
}

fn spec_parameters(
    spec: &FlowGeneratorSpecV1,
) -> Result<BTreeMap<String, serde_json::Value>, FlowGenerationError> {
    let value = serde_json::to_value(spec)?;
    let serde_json::Value::Object(parameters) = value else {
        return Err(FlowGenerationError::Canonicalization);
    };
    Ok(parameters.into_iter().collect())
}

fn stats(
    graph: &FlowGraphV1,
    capacities: &[u64],
    costs: &[i64],
) -> Result<FlowGeneratorStatsV1, FlowGenerationError> {
    Ok(FlowGeneratorStatsV1 {
        node_count: u32::try_from(graph.nodes.len()).map_err(|_| FlowGenerationError::SizeLimit)?,
        edge_count: u32::try_from(graph.edges.len()).map_err(|_| FlowGenerationError::SizeLimit)?,
        minimum_capacity: capacities.iter().copied().min().unwrap_or(0).to_string(),
        maximum_capacity: capacities.iter().copied().max().unwrap_or(0).to_string(),
        minimum_cost: costs.iter().copied().min().unwrap_or(0).to_string(),
        maximum_cost: costs.iter().copied().max().unwrap_or(0).to_string(),
    })
}

#[derive(Clone, Copy)]
enum ValidatedCapacity {
    Constant(u64),
    Uniform(u64, u64),
    Bimodal(u64, u64),
    PowerOfTwoBuckets(u32, u32),
}

impl ValidatedCapacity {
    fn new(distribution: &CapacityDistributionV1) -> Result<Self, FlowGenerationError> {
        match distribution {
            CapacityDistributionV1::Unit {} => Ok(Self::Constant(1)),
            CapacityDistributionV1::Constant { value } => {
                Ok(Self::Constant(parse_u64(value, "capacity")?))
            }
            CapacityDistributionV1::Uniform { minimum, maximum } => {
                let minimum = parse_u64(minimum, "minimum capacity")?;
                let maximum = parse_u64(maximum, "maximum capacity")?;
                if minimum > maximum {
                    return Err(FlowGenerationError::Invalid("capacity interval"));
                }
                Ok(Self::Uniform(minimum, maximum))
            }
            CapacityDistributionV1::Bimodal { first, second } => {
                let first = parse_u64(first, "first capacity atom")?;
                let second = parse_u64(second, "second capacity atom")?;
                if first == second {
                    return Err(FlowGenerationError::Invalid(
                        "bimodal capacity atoms must differ",
                    ));
                }
                Ok(Self::Bimodal(first, second))
            }
            CapacityDistributionV1::PowerOfTwoBuckets {
                minimum_exponent,
                maximum_exponent,
            } => {
                if minimum_exponent > maximum_exponent || *maximum_exponent > 63 {
                    return Err(FlowGenerationError::Invalid(
                        "capacity power-of-two exponent interval",
                    ));
                }
                Ok(Self::PowerOfTwoBuckets(
                    *minimum_exponent,
                    *maximum_exponent,
                ))
            }
        }
    }

    fn bounds(self) -> (u64, u64) {
        match self {
            Self::Constant(value) => (value, value),
            Self::Uniform(minimum, maximum) => (minimum, maximum),
            Self::Bimodal(first, second) => (first.min(second), first.max(second)),
            Self::PowerOfTwoBuckets(minimum, maximum) => (1_u64 << minimum, 1_u64 << maximum),
        }
    }

    fn sample(self, rng: &mut RngV1) -> Result<u64, FlowGenerationError> {
        match self {
            Self::Constant(value) => Ok(value),
            Self::Uniform(minimum, maximum) => sample_uniform_u64(rng, minimum, maximum),
            Self::Bimodal(first, second) => Ok(if rng.bounded_u64(2)? == 0 {
                first
            } else {
                second
            }),
            Self::PowerOfTwoBuckets(minimum, maximum) => {
                let exponent = sample_uniform_u64(rng, u64::from(minimum), u64::from(maximum))?;
                1_u64
                    .checked_shl(
                        u32::try_from(exponent)
                            .map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
                    )
                    .ok_or(FlowGenerationError::ArithmeticOverflow)
            }
        }
    }
}

#[derive(Clone, Copy)]
struct CorrelatedCost {
    minimum: i64,
    maximum: i64,
    direction: CapacityCostCorrelationV1,
    maximum_jitter: i64,
}

#[derive(Clone, Copy)]
enum ValidatedCost {
    Constant(i64),
    Uniform(i64, i64),
    Bimodal(i64, i64),
    CapacityCorrelated(CorrelatedCost),
}

impl ValidatedCost {
    fn new(distribution: &CostDistributionV1) -> Result<Self, FlowGenerationError> {
        match distribution {
            CostDistributionV1::Zero {} => Ok(Self::Constant(0)),
            CostDistributionV1::Constant { value } => Ok(Self::Constant(parse_i64(value, "cost")?)),
            CostDistributionV1::Uniform { minimum, maximum } => {
                let minimum = parse_i64(minimum, "minimum cost")?;
                let maximum = parse_i64(maximum, "maximum cost")?;
                if minimum > maximum {
                    return Err(FlowGenerationError::Invalid("cost interval"));
                }
                Ok(Self::Uniform(minimum, maximum))
            }
            CostDistributionV1::Bimodal { first, second } => {
                let first = parse_i64(first, "first cost atom")?;
                let second = parse_i64(second, "second cost atom")?;
                if first == second {
                    return Err(FlowGenerationError::Invalid(
                        "bimodal cost atoms must differ",
                    ));
                }
                Ok(Self::Bimodal(first, second))
            }
            CostDistributionV1::CapacityCorrelated {
                minimum,
                maximum,
                direction,
                maximum_jitter,
            } => {
                let minimum = parse_i64(minimum, "minimum correlated cost")?;
                let maximum = parse_i64(maximum, "maximum correlated cost")?;
                let maximum_jitter = parse_i64(maximum_jitter, "maximum cost jitter")?;
                if minimum > maximum {
                    return Err(FlowGenerationError::Invalid("correlated cost interval"));
                }
                if maximum_jitter < 0 {
                    return Err(FlowGenerationError::Invalid(
                        "maximum cost jitter is negative",
                    ));
                }
                Ok(Self::CapacityCorrelated(CorrelatedCost {
                    minimum,
                    maximum,
                    direction: *direction,
                    maximum_jitter,
                }))
            }
        }
    }

    fn sample(
        self,
        rng: &mut RngV1,
        capacity: u64,
        capacity_bounds: (u64, u64),
    ) -> Result<i64, FlowGenerationError> {
        match self {
            Self::Constant(value) => Ok(value),
            Self::Uniform(minimum, maximum) => sample_uniform_i64(rng, minimum, maximum),
            Self::Bimodal(first, second) => Ok(if rng.bounded_u64(2)? == 0 {
                first
            } else {
                second
            }),
            Self::CapacityCorrelated(config) => {
                correlated_cost(rng, capacity, capacity_bounds, config)
            }
        }
    }
}

fn sample_uniform_u64(
    rng: &mut RngV1,
    minimum: u64,
    maximum: u64,
) -> Result<u64, FlowGenerationError> {
    if minimum == 0 && maximum == u64::MAX {
        return Ok(rng.next_u64());
    }
    let span = maximum
        .checked_sub(minimum)
        .and_then(|value| value.checked_add(1))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    minimum
        .checked_add(rng.bounded_u64(span)?)
        .ok_or(FlowGenerationError::ArithmeticOverflow)
}

fn sample_uniform_i64(
    rng: &mut RngV1,
    minimum: i64,
    maximum: i64,
) -> Result<i64, FlowGenerationError> {
    let span = i128::from(maximum)
        .checked_sub(i128::from(minimum))
        .and_then(|value| value.checked_add(1))
        .and_then(|value| u128::try_from(value).ok())
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let offset = if span == u128::from(u64::MAX) + 1 {
        u128::from(rng.next_u64())
    } else {
        u128::from(rng.bounded_u64(
            u64::try_from(span).map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
        )?)
    };
    i64::try_from(
        i128::from(minimum)
            .checked_add(
                i128::try_from(offset).map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
            )
            .ok_or(FlowGenerationError::ArithmeticOverflow)?,
    )
    .map_err(|_| FlowGenerationError::ArithmeticOverflow)
}

fn correlated_cost(
    rng: &mut RngV1,
    capacity: u64,
    (capacity_minimum, capacity_maximum): (u64, u64),
    config: CorrelatedCost,
) -> Result<i64, FlowGenerationError> {
    let CorrelatedCost {
        minimum: cost_minimum,
        maximum: cost_maximum,
        direction,
        maximum_jitter,
    } = config;
    let capacity_span = u128::from(capacity_maximum - capacity_minimum);
    let capacity_offset = match direction {
        CapacityCostCorrelationV1::Positive => capacity - capacity_minimum,
        CapacityCostCorrelationV1::Negative => capacity_maximum - capacity,
    };
    let cost_span = u128::try_from(i128::from(cost_maximum) - i128::from(cost_minimum))
        .map_err(|_| FlowGenerationError::ArithmeticOverflow)?;
    let scaled_offset = if capacity_span == 0 {
        cost_span / 2
    } else {
        u128::from(capacity_offset)
            .checked_mul(cost_span)
            .ok_or(FlowGenerationError::ArithmeticOverflow)?
            / capacity_span
    };
    let base = i128::from(cost_minimum)
        .checked_add(
            i128::try_from(scaled_offset).map_err(|_| FlowGenerationError::ArithmeticOverflow)?,
        )
        .ok_or(FlowGenerationError::ArithmeticOverflow)?;
    let jitter = sample_uniform_i64(rng, -maximum_jitter, maximum_jitter)?;
    let realized = base
        .checked_add(i128::from(jitter))
        .ok_or(FlowGenerationError::ArithmeticOverflow)?
        .clamp(i128::from(cost_minimum), i128::from(cost_maximum));
    i64::try_from(realized).map_err(|_| FlowGenerationError::ArithmeticOverflow)
}

fn parse_u64(value: &str, field: &'static str) -> Result<u64, FlowGenerationError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(FlowGenerationError::Invalid(field));
    }
    value
        .parse()
        .map_err(|_| FlowGenerationError::Invalid(field))
}

fn parse_i64(value: &str, field: &'static str) -> Result<i64, FlowGenerationError> {
    if value == "-0"
        || value.starts_with('+')
        || value.is_empty()
        || (value.starts_with('0') && value.len() > 1)
        || (value.starts_with('-') && (value.len() == 1 || value.as_bytes().get(1) == Some(&b'0')))
        || !value
            .bytes()
            .enumerate()
            .all(|(index, byte)| byte.is_ascii_digit() || (index == 0 && byte == b'-'))
    {
        return Err(FlowGenerationError::Invalid(field));
    }
    value
        .parse()
        .map_err(|_| FlowGenerationError::Invalid(field))
}

#[cfg(test)]
#[path = "generator_tests.rs"]
mod tests;
