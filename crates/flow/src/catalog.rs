//! Machine-readable catalog for every normalized flow-algorithm name.

use std::{fmt, str::FromStr};

use serde::{Serialize, Serializer};

/// Whether a catalog entry solves the public model or exposes a named component.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogKind {
    /// Complete solver for one or more public problem models.
    Solver,
    /// Source-defined selection rule, data structure, or phase variant.
    Variant,
    /// Optional behavior of a solver kernel.
    Heuristic,
    /// Named subproblem with its own result and invariant checker.
    Primitive,
}

/// High-level problem compatibility advertised by a descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProblemKind {
    /// Single-source, single-sink maximum flow.
    MaxFlow,
    /// Linear minimum-cost flow, circulation, or min-cost max-flow.
    MinCostFlow,
    /// Bipartite matching.
    BipartiteMatching,
    /// Minimum-cost perfect assignment.
    Assignment,
    /// Transportation problem.
    Transportation,
    /// Embedded planar maximum flow.
    PlanarMaxFlow,
    /// Monotone parametric maximum flow.
    ParametricMaxFlow,
    /// Capacity/edge/terminal-update dynamic maximum flow.
    DynamicMaxFlow,
    /// Maximum flow seeded by a predicted or previously computed pseudoflow.
    WarmStartMaxFlow,
    /// Integral piecewise-linear separable convex-cost flow.
    ConvexCostFlow,
}

/// Exact public model admitted by a descriptor or specialized runtime variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogModelKind {
    /// Single-source, single-sink maximum flow.
    MaxFlow,
    /// Exact terminal-flow amount with minimum linear cost.
    FixedFlowMinCost,
    /// Lexicographic maximum flow followed by minimum cost.
    MinCostMaxFlow,
    /// Minimum-cost circulation.
    Circulation,
    /// Minimum-cost transshipment under node balances.
    Transshipment,
    /// Bipartite matching specialization.
    BipartiteMatching,
    /// Minimum-cost perfect assignment specialization.
    Assignment,
    /// Transportation specialization.
    Transportation,
    /// Embedded planar maximum flow.
    PlanarMaxFlow,
    /// Monotone parametric maximum flow.
    ParametricMaxFlow,
    /// Dynamic maximum flow under updates.
    DynamicMaxFlow,
    /// Warm-start maximum flow.
    WarmStartMaxFlow,
    /// Piecewise-linear convex-cost flow.
    ConvexCostFlow,
}

/// Structural graph property required by a source-specific complexity claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphRequirement {
    /// No original edge starts and ends at the same node.
    NoSelfLoops,
    /// Every node has zero supply and every original edge has lower bound zero.
    ZeroFlowFeasible,
    /// Every original edge has strictly positive capacity.
    PositiveCapacity,
    /// At least one original edge is present.
    NonEmptyEdges,
    /// Every original edge has zero cost.
    ZeroCost,
    /// Source and sink are present and distinct in the selected model.
    DistinctTerminals,
    /// Ignoring arc direction, all original nodes belong to one component.
    UnderlyingConnected,
    /// Every original edge has lower bound zero and capacity one.
    UnitCapacity,
    /// Unit capacities plus the classical unit-network degree condition.
    UnitNetwork,
    /// The underlying undirected graph is bipartite.
    Bipartite,
    /// The two bipartition sides have equal cardinality.
    BalancedBipartite,
    /// Arcs, supplies, and demands form a directed transportation network.
    TransportationNetwork,
    /// A combinatorial planar embedding is supplied and verified.
    PlanarEmbedding,
    /// Every original node reaches every other node through positive-width arcs.
    StronglyConnected,
    /// Each shifted arc width can carry the full transshipment supply, so the
    /// finite input is an exact nonbinding encoding of an uncapacitated arc.
    NonbindingTransshipmentCapacities,
}

/// Algorithm family used for catalog grouping and overlays.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlgorithmFamily {
    /// Augmenting-path methods.
    AugmentingPath,
    /// Blocking-flow methods.
    BlockingFlow,
    /// Preflow push/relabel methods.
    PushRelabel,
    /// Pseudoflow methods.
    Pseudoflow,
    /// Vision-oriented graph-cut methods.
    VisionGraph,
    /// Specialized combinatorial methods.
    Special,
    /// Electrical, interior-point, and related continuous methods.
    Continuous,
    /// Advanced discrete methods.
    AdvancedDiscrete,
    /// Dynamic methods.
    Dynamic,
    /// Negative-cycle cancellation methods.
    CycleCanceling,
    /// Shortest-path minimum-cost-flow methods.
    ShortestPath,
    /// Primal-dual methods.
    PrimalDual,
    /// Capacity, cost, or excess scaling methods.
    Scaling,
    /// Network-simplex methods.
    Simplex,
    /// Transportation-specific methods.
    Transportation,
    /// Price-relaxation methods.
    Relaxation,
    /// Assignment-specific methods.
    Assignment,
    /// Strongly polynomial frameworks.
    StronglyPolynomial,
    /// Convex-cost methods.
    Convex,
    /// Prediction-assisted methods.
    Prediction,
}

/// Catalog-owned explanation of the three nested trace boundaries.
///
/// This describes what one user-visible step means for an endpoint. It is
/// independent of the selected playback policy: `Semantic` exposes phase and
/// operation boundaries, while `Detailed` additionally exposes detail
/// boundaries when the endpoint records them.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AlgorithmStepContractV1 {
    /// Meaning of one phase boundary for this endpoint family.
    pub phase_unit: &'static str,
    /// Whether this endpoint records intermediate phase boundaries.
    pub phase_availability: AlgorithmStepAvailabilityV1,
    /// Meaning of one complete invariant-preserving operation.
    pub operation_unit: &'static str,
    /// Whether this endpoint records intermediate operation boundaries.
    pub operation_availability: AlgorithmStepAvailabilityV1,
    /// Whether the endpoint records source-level detail boundaries.
    pub detail: AlgorithmDetailStepV1,
    /// Monotone implementation counter used to audit trace length against work.
    pub primary_work: AlgorithmPrimaryWorkV1,
}

/// Abstraction level of the monotone counter used for trace-work accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlgorithmWorkAbstractionV1 {
    /// A directly enumerated combinatorial primitive such as an arc scan.
    Primitive,
    /// One source-level numerical or combinatorial iteration.
    Iteration,
    /// One bounded source-defined oracle or data-structure query.
    OracleCall,
}

/// Primary monotone work counter for one executable endpoint.
///
/// The counter is an implementation-work witness, not an assertion that its
/// value is the exact running time or that a few finite samples prove a Big-O
/// bound. Detail audits use it to reject arbitrary event-count thresholds and
/// to expose explicitly aggregated work.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AlgorithmPrimaryWorkV1 {
    /// Ordinal in the fixed scene metric vector.
    pub metric_ordinal: u8,
    /// Human-readable plural unit owned by the endpoint contract.
    pub unit: &'static str,
    /// Level at which one counter increment is meaningful.
    pub abstraction: AlgorithmWorkAbstractionV1,
    /// Work-domain classification used by catalog audits. It never creates
    /// graph focus or source events from a counter delta.
    pub visualization: AlgorithmWorkVisualizationV1,
}

/// Domain in which the primary implementation work occurs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlgorithmWorkVisualizationV1 {
    /// Residual, pricing, matching, or assignment work over graph edges.
    EdgeField,
    /// Cycle, forest, vector, or other combinatorial candidate enumeration.
    CandidateField,
    /// Matrix products and elimination work represented as a node potential field.
    NumericField,
}

/// Availability of an intermediate playback boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "availability", rename_all = "kebab-case")]
pub enum AlgorithmStepAvailabilityV1 {
    /// The endpoint records at least one boundary of this kind when applicable.
    Available,
    /// The current trace intentionally aggregates or omits this boundary kind.
    Unavailable {
        /// User-facing explanation; the UI must retain and disable the option.
        reason: &'static str,
    },
}

/// Availability and meaning of source-level detailed playback.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "availability", rename_all = "kebab-case")]
pub enum AlgorithmDetailStepV1 {
    /// Detailed playback exposes the stated primitive unit.
    Available {
        /// Meaning of one detail boundary.
        unit: &'static str,
    },
    /// The current trace aggregates its internal primitive work.
    Unavailable {
        /// User-facing explanation; the UI must not silently fall back.
        reason: &'static str,
    },
}

/// Required construction of an algorithm's initial state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InitialConstruction {
    /// A feasible zero flow after model transformation.
    ZeroFeasible,
    /// A zero pseudoflow with explicit excess and deficit.
    ZeroPseudoflowWithImbalance,
    /// Any feasible primal flow.
    AnyFeasible,
    /// A dual-feasible state.
    DualFeasible,
    /// An epsilon-optimal state.
    EpsilonOptimal,
    /// A source-specific initialization defined by the primary record.
    SourceDefined,
    /// A project-owned bounded oracle constructs an input used to demonstrate
    /// a cited source prefix or recovery lemma. This is not source initialization.
    ProjectOracleConstructed,
}

/// Optimality property required before or during initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InitialOptimalityRequirement {
    /// No initial optimality is required.
    None,
    /// Every partial flow must be optimal for its current value.
    OptimalForEveryPartialValue,
    /// Reduced costs must satisfy dual feasibility.
    DualFeasible,
    /// The state must satisfy the descriptor's epsilon-optimality rule.
    EpsilonOptimal,
    /// The source record defines an additional optimality requirement.
    SourceDefined,
    /// A project oracle supplies the optimum information used by a demonstrator.
    ProjectOracleConstructed,
}

/// Project-owned optimum information consumed while constructing or pricing
/// the first source-prefix state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InitialOracleDependency {
    /// The endpoint does not consume a project optimum oracle before source progress.
    None,
    /// A bounded cut oracle supplies the exact scalar maximum-flow target used
    /// to initialize the objective gap or source potential.
    ProjectExactMaxFlowScalarTarget,
    /// A bounded feasible-flow oracle supplies the exact scalar minimum cost
    /// used to initialize the source potential without retaining an optimum vector.
    ProjectExactMinCostScalarOptimum,
    /// A bounded optimum-vector oracle constructs the relative-interior state
    /// from which the disclosed source prefix starts.
    ProjectOptimumVectorInitialState,
    /// A bounded isolation/face oracle supplies optimum objective and fixed-face
    /// facts before source progress, then discards every enumerated flow vector.
    ProjectIsolationFaceOptimumFacts,
    /// A bounded feasible-face oracle constructs the strict relative-interior
    /// barycenter and exact scalar optimum before one source progress step.
    ProjectFeasibleFaceInitialStateAndScalarOptimum,
}

/// Project-owned terminal information used after the advertised source prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalOracleDependency {
    /// The endpoint constructs its terminal result without a project optimum-vector oracle.
    None,
    /// A project optimum-vector oracle constructs the disclosed rational final
    /// point consumed by the endpoint's terminal rounding step.
    ProjectOptimumVectorFinalPoint,
}

/// How an algorithm treats negative cycles in its initial residual graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NegativeCyclePolicy {
    /// Costs do not participate in this algorithm's model.
    NotApplicable,
    /// A negative cycle in any residual component rejects the initial state.
    RequireAbsentAnywhere,
    /// The algorithm is responsible for resolving negative cycles.
    ResolveInternally,
    /// The exact policy is defined by the primary source record.
    SourceDefined,
}

/// Publication readiness of an implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImplementationStatus {
    /// Source is fixed, implementation is not complete yet.
    Planned,
    /// The exact source variant must be fixed before implementation.
    SourceBlocked,
    /// The disclosed endpoint contract, trace/fast path, and required checker
    /// are production-ready. This status does not by itself claim that a
    /// bounded lab instantiates every data structure or runtime from its source.
    Executable,
}

/// How the published endpoint reaches its result relative to the cited source.
///
/// This is deliberately independent of [`ImplementationStatus`]: an endpoint
/// can be executable and replay-safe while still being a bounded demonstrator
/// rather than a complete realization of the named source algorithm.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImplementationScope {
    /// The advertised algorithm produces its own result inside the public band.
    SourceComplete,
    /// Exact bounded oracles replace asymptotic data structures while retaining
    /// the source transition sequence and termination rule.
    BoundedOracleGuided,
    /// A source-defined primitive or heuristic, not a complete solver claim.
    SourceComponent,
    /// A project-owned composite demonstrates source steps around an explicitly
    /// disclosed optimum-vector oracle. It is neither a source component nor a
    /// realization of the named parent solver.
    ProjectOracleDemonstrator,
    /// The source kernel is shown, then a different complete solver publishes
    /// the final result. This is an explicit migration state and release blocker.
    ExternalCompletion,
    /// An exact optimum is computed before source progress and later projected
    /// into the terminal state. This is an explicit migration state and blocker.
    PrecomputedOptimumProjection,
}

/// Top-level WASM execution route owned by a descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeRouteKind {
    /// Single-source/sink maximum-flow runtime.
    MaxFlow,
    /// Fixed-flow, circulation, or transshipment minimum-cost runtime.
    MinCostFlow,
    /// Lexicographic minimum-cost maximum-flow runtime.
    MinCostMaxFlow,
    /// Parametric maximum-flow runtime.
    ParametricMaxFlow,
    /// Bipartite matching runtime.
    BipartiteMatching,
    /// Assignment runtime.
    Assignment,
    /// Transportation runtime.
    Transportation,
    /// Embedded planar maximum-flow runtime.
    PlanarMaxFlow,
    /// Piecewise-linear convex-cost runtime.
    ConvexCostFlow,
}

/// Conservative public node/edge band; algorithm-specific work ceilings live
/// with the executable kernel and are checked at its boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct InitialAdmissionBand {
    /// Maximum original nodes considered by the initial implementation.
    pub max_nodes: u32,
    /// Maximum original edges considered by the initial implementation.
    pub max_edges: u32,
}

/// Exact integer encoded as a canonical decimal string at the JSON boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdmissionLimitU64(pub u64);

impl Serialize for AdmissionLimitU64 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

/// Additional pre-dispatch limits owned by the concrete algorithm kernel.
///
/// The generic [`InitialAdmissionBand`] remains the first coarse guard. These
/// fields publish any stricter executable-kernel boundary together with
/// source-specific minima, exact-enumeration envelopes, and dynamic-input
/// requirements.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AlgorithmAdmissionContractV1 {
    /// Minimum number of original nodes, when the kernel requires one.
    pub min_nodes: Option<u32>,
    /// Minimum number of original edges, when the kernel requires one.
    pub min_edges: Option<u32>,
    /// Maximum number of original nodes when stricter than the generic band.
    pub max_nodes: Option<u32>,
    /// Maximum number of original edges when stricter than the generic band.
    pub max_edges: Option<u32>,
    /// Maximum capacity on one original edge.
    pub max_capacity: Option<AdmissionLimitU64>,
    /// Maximum absolute cost on one original edge.
    pub max_absolute_cost: Option<AdmissionLimitU64>,
    /// Maximum product of inclusive lower-to-upper flow choices.
    pub max_assignment_space: Option<AdmissionLimitU64>,
    /// Maximum product of `capacity + 1` used by bounded max-flow oracles.
    pub max_capacity_state_space: Option<AdmissionLimitU64>,
    /// Whether the requested balances admit a flow strictly between every
    /// original edge's lower and upper bounds.
    pub strict_interior_required: bool,
    /// Minimum number of capacity updates required by a dynamic endpoint.
    pub min_dynamic_capacity_updates: Option<u32>,
    /// Maximum number of capacity updates accepted by a dynamic endpoint.
    pub max_dynamic_capacity_updates: Option<u32>,
    /// Whether the dynamic endpoint rejects every non-capacity update.
    pub capacity_updates_only: bool,
}

/// A normalized catalog entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AlgorithmDescriptor {
    /// Closed typed identity used by runtime and conformance dispatch.
    #[serde(skip)]
    pub algorithm_id: AlgorithmId,
    /// Stable kebab-case identifier.
    pub id: &'static str,
    /// Standard English display name.
    pub title: &'static str,
    /// Search aliases retained from the referenced conversation and literature.
    pub aliases: &'static [&'static str],
    /// Discovery-only terms that lead to this entry without claiming that the
    /// entry itself is a complete implementation of the named parent method.
    pub search_terms: &'static [&'static str],
    /// Solver, variant, heuristic, or primitive classification.
    pub kind: CatalogKind,
    /// Catalog grouping family.
    pub family: AlgorithmFamily,
    /// Endpoint-specific playback boundary contract.
    pub trace_steps: AlgorithmStepContractV1,
    /// Compatible public problem kinds.
    pub problems: &'static [ProblemKind],
    /// Exact public models supported by this source-defined variant.
    pub models: &'static [CatalogModelKind],
    /// Closed top-level runtime route; every entry declares this explicitly.
    pub runtime_route: RuntimeRouteKind,
    /// Additional graph-shape properties required by this descriptor.
    pub graph_requirements: &'static [GraphRequirement],
    /// Required initial-state contract.
    pub initial_construction: InitialConstruction,
    /// Required initial or partial-flow optimality.
    pub initial_optimality: InitialOptimalityRequirement,
    /// Project-owned optimum information consumed before source progress.
    pub initial_oracle_dependency: InitialOracleDependency,
    /// Initial residual negative-cycle handling.
    pub negative_cycle_policy: NegativeCyclePolicy,
    /// Whether the public outcome is exact rather than approximate.
    pub exact: bool,
    /// Whether the source algorithm uses randomization.
    pub randomized: bool,
    /// Source complexity or disclosed bounded-endpoint implementation claim;
    /// details and non-claims live in the source registry.
    pub complexity: &'static str,
    /// Primary record key in `docs/flow-sources.md`.
    pub source_id: &'static str,
    /// Initial conservative admission band.
    pub initial_band: InitialAdmissionBand,
    /// Source/kernel-specific admission facts exported to every client.
    pub admission_contract: AlgorithmAdmissionContractV1,
    /// Current publication status.
    pub status: ImplementationStatus,
    /// Source-fidelity/completion classification for this executable endpoint.
    pub implementation_scope: ImplementationScope,
    /// Project-owned optimum information used only at terminal recovery.
    pub terminal_oracle_dependency: TerminalOracleDependency,
}

const BAND_CLASSICAL: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 2_000,
    max_edges: 20_000,
};
const BAND_SMALL: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 256,
    max_edges: 2_048,
};
const BAND_CONVEX: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 128,
    max_edges: 1_024,
};
const BAND_COST_SCALING: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 512,
    max_edges: 4_096,
};
const BAND_RESEARCH: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 64,
    max_edges: 512,
};
const BAND_RANDOMIZED_ALMOST_LINEAR_MAX_FLOW: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 8,
    max_edges: 10,
};
const BAND_DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 7,
    max_edges: 8,
};
const BAND_ORLIN_MCF: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 32,
    max_edges: 96,
};
const BAND_ORLIN_MAX_FLOW: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 48,
    max_edges: 192,
};
const BAND_ELECTRICAL_FLOW: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 24,
    max_edges: 96,
};
const BAND_AUGMENTING_ELECTRICAL: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 5,
    max_edges: 6,
};
const BAND_INTERIOR_POINT_MAX_FLOW: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 8,
    max_edges: 10,
};
const BAND_MINIMUM_RATIO_CYCLE: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 8,
    max_edges: 11,
};
const BAND_WEIGHTED_AUGMENTING_PATHS: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 8,
    max_edges: 12,
};
const BAND_WEIGHTED_PUSH_RELABEL_SHORTCUT: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 8,
    max_edges: 12,
};
const BAND_DETERMINISTIC_ALMOST_LINEAR_MCF: InitialAdmissionBand = InitialAdmissionBand {
    max_nodes: 6,
    max_edges: 8,
};

const MAX: &[ProblemKind] = &[ProblemKind::MaxFlow];
const MCF: &[ProblemKind] = &[ProblemKind::MinCostFlow];
const TRANSSHIPMENT_MCF: &[ProblemKind] = &[ProblemKind::MinCostFlow];
const BF_MCF: &[ProblemKind] = &[ProblemKind::MinCostFlow];
const MCMF: &[ProblemKind] = &[ProblemKind::MinCostFlow];
const MATCHING: &[ProblemKind] = &[ProblemKind::BipartiteMatching];
const ASSIGNMENT: &[ProblemKind] = &[ProblemKind::Assignment];
const TRANSPORT: &[ProblemKind] = &[ProblemKind::Transportation];
const PLANAR: &[ProblemKind] = &[ProblemKind::PlanarMaxFlow];
const PARAMETRIC: &[ProblemKind] = &[ProblemKind::ParametricMaxFlow];
const DYNAMIC_MAX: &[ProblemKind] = &[ProblemKind::DynamicMaxFlow];
const WARM_START: &[ProblemKind] = &[ProblemKind::WarmStartMaxFlow];
const CONVEX: &[ProblemKind] = &[ProblemKind::ConvexCostFlow];

const MAX_MODELS: &[CatalogModelKind] = &[CatalogModelKind::MaxFlow];
const MCF_MODELS: &[CatalogModelKind] = &[
    CatalogModelKind::FixedFlowMinCost,
    CatalogModelKind::Circulation,
    CatalogModelKind::Transshipment,
];
const BF_MCF_MODELS: &[CatalogModelKind] = &[
    CatalogModelKind::FixedFlowMinCost,
    CatalogModelKind::Circulation,
    CatalogModelKind::Transshipment,
];
const MCMF_MODELS: &[CatalogModelKind] = &[CatalogModelKind::MinCostMaxFlow];
const TRANSSHIPMENT_MCF_MODELS: &[CatalogModelKind] = &[CatalogModelKind::Transshipment];
const MATCHING_MODELS: &[CatalogModelKind] = &[CatalogModelKind::BipartiteMatching];
const ASSIGNMENT_MODELS: &[CatalogModelKind] = &[CatalogModelKind::Assignment];
const TRANSPORT_MODELS: &[CatalogModelKind] = &[CatalogModelKind::Transportation];
const PLANAR_MODELS: &[CatalogModelKind] = &[CatalogModelKind::PlanarMaxFlow];
const PARAMETRIC_MODELS: &[CatalogModelKind] = &[CatalogModelKind::ParametricMaxFlow];
// Warm-start push-relabel is selected through the public max-flow scenario;
// `WarmStartMaxFlow` is a catalog problem grouping, not a wire model.
const WARM_START_MODELS: &[CatalogModelKind] = &[CatalogModelKind::MaxFlow];
const CONVEX_MODELS: &[CatalogModelKind] = &[CatalogModelKind::ConvexCostFlow];

const NO_GRAPH_REQUIREMENTS: &[GraphRequirement] = &[];
const NO_SELF_LOOP_REQUIREMENTS: &[GraphRequirement] = &[GraphRequirement::NoSelfLoops];
const POSITIVE_SIMPLE_MAX_FLOW_REQUIREMENTS: &[GraphRequirement] = &[
    GraphRequirement::NoSelfLoops,
    GraphRequirement::ZeroFlowFeasible,
    GraphRequirement::PositiveCapacity,
    GraphRequirement::NonEmptyEdges,
    GraphRequirement::DistinctTerminals,
];
const ELECTRICAL_FLOW_REQUIREMENTS: &[GraphRequirement] = &[
    GraphRequirement::NoSelfLoops,
    GraphRequirement::ZeroFlowFeasible,
    GraphRequirement::PositiveCapacity,
    GraphRequirement::NonEmptyEdges,
    GraphRequirement::ZeroCost,
    GraphRequirement::DistinctTerminals,
    GraphRequirement::UnderlyingConnected,
];
const AUGMENTING_ELECTRICAL_REQUIREMENTS: &[GraphRequirement] = &[
    GraphRequirement::NoSelfLoops,
    GraphRequirement::ZeroFlowFeasible,
    GraphRequirement::PositiveCapacity,
    GraphRequirement::NonEmptyEdges,
    GraphRequirement::ZeroCost,
    GraphRequirement::DistinctTerminals,
];
const UNIT_IPM_MAX_FLOW_REQUIREMENTS: &[GraphRequirement] = &[
    GraphRequirement::NoSelfLoops,
    GraphRequirement::ZeroFlowFeasible,
    GraphRequirement::UnitCapacity,
    GraphRequirement::NonEmptyEdges,
    GraphRequirement::ZeroCost,
    GraphRequirement::DistinctTerminals,
];
const ZERO_FLOW_FEASIBLE_REQUIREMENTS: &[GraphRequirement] = &[GraphRequirement::ZeroFlowFeasible];
const UNIT_CAPACITY_REQUIREMENTS: &[GraphRequirement] = &[GraphRequirement::UnitCapacity];
const UNIT_NETWORK_REQUIREMENTS: &[GraphRequirement] = &[
    GraphRequirement::UnitCapacity,
    GraphRequirement::UnitNetwork,
];
const BIPARTITE_REQUIREMENTS: &[GraphRequirement] = &[GraphRequirement::Bipartite];
const TRANSPORTATION_REQUIREMENTS: &[GraphRequirement] = &[
    GraphRequirement::Bipartite,
    GraphRequirement::TransportationNetwork,
];
const PLANAR_REQUIREMENTS: &[GraphRequirement] = &[GraphRequirement::PlanarEmbedding];
const UNCAPACITATED_TRANSSHIPMENT_REQUIREMENTS: &[GraphRequirement] = &[
    GraphRequirement::StronglyConnected,
    GraphRequirement::NonbindingTransshipmentCapacities,
];
const NONBINDING_TRANSSHIPMENT_REQUIREMENTS: &[GraphRequirement] =
    &[GraphRequirement::NonbindingTransshipmentCapacities];

macro_rules! catalog_models {
    (MAX) => {
        MAX_MODELS
    };
    (MCF) => {
        MCF_MODELS
    };
    (BF_MCF) => {
        BF_MCF_MODELS
    };
    (MCMF) => {
        MCMF_MODELS
    };
    (TRANSSHIPMENT_MCF) => {
        TRANSSHIPMENT_MCF_MODELS
    };
    (MATCHING) => {
        MATCHING_MODELS
    };
    (ASSIGNMENT) => {
        ASSIGNMENT_MODELS
    };
    (TRANSPORT) => {
        TRANSPORT_MODELS
    };
    (PLANAR) => {
        PLANAR_MODELS
    };
    (PARAMETRIC) => {
        PARAMETRIC_MODELS
    };
    (DYNAMIC_MAX) => {
        MAX_MODELS
    };
    (WARM_START) => {
        WARM_START_MODELS
    };
    (CONVEX) => {
        CONVEX_MODELS
    };
}

macro_rules! catalog_graph_requirements {
    (MATCHING) => {
        BIPARTITE_REQUIREMENTS
    };
    (ASSIGNMENT) => {
        BIPARTITE_REQUIREMENTS
    };
    (TRANSPORT) => {
        TRANSPORTATION_REQUIREMENTS
    };
    (PLANAR) => {
        PLANAR_REQUIREMENTS
    };
    ($problem:ident) => {
        NO_GRAPH_REQUIREMENTS
    };
}

#[derive(Clone, Copy)]
struct InitialContract {
    construction: InitialConstruction,
    optimality: InitialOptimalityRequirement,
    negative_cycles: NegativeCyclePolicy,
}

const MAX_FLOW_ZERO: InitialContract = InitialContract {
    construction: InitialConstruction::ZeroFeasible,
    optimality: InitialOptimalityRequirement::None,
    negative_cycles: NegativeCyclePolicy::NotApplicable,
};
const SSP_PSEUDOFLOW: InitialContract = InitialContract {
    construction: InitialConstruction::ZeroPseudoflowWithImbalance,
    optimality: InitialOptimalityRequirement::OptimalForEveryPartialValue,
    negative_cycles: NegativeCyclePolicy::RequireAbsentAnywhere,
};
const CAPACITY_SCALING_PSEUDOFLOW: InitialContract = InitialContract {
    construction: InitialConstruction::ZeroPseudoflowWithImbalance,
    optimality: InitialOptimalityRequirement::DualFeasible,
    negative_cycles: NegativeCyclePolicy::RequireAbsentAnywhere,
};
const ENHANCED_CAPACITY_SCALING: InitialContract = InitialContract {
    construction: InitialConstruction::ZeroPseudoflowWithImbalance,
    optimality: InitialOptimalityRequirement::DualFeasible,
    negative_cycles: NegativeCyclePolicy::RequireAbsentAnywhere,
};
const ANY_FEASIBLE_RESOLVE: InitialContract = InitialContract {
    construction: InitialConstruction::AnyFeasible,
    optimality: InitialOptimalityRequirement::None,
    negative_cycles: NegativeCyclePolicy::ResolveInternally,
};
const DUAL_FEASIBLE: InitialContract = InitialContract {
    construction: InitialConstruction::DualFeasible,
    optimality: InitialOptimalityRequirement::DualFeasible,
    negative_cycles: NegativeCyclePolicy::RequireAbsentAnywhere,
};
const EPSILON_OPTIMAL: InitialContract = InitialContract {
    construction: InitialConstruction::EpsilonOptimal,
    optimality: InitialOptimalityRequirement::EpsilonOptimal,
    negative_cycles: NegativeCyclePolicy::ResolveInternally,
};
const SOURCE_DEFINED: InitialContract = InitialContract {
    construction: InitialConstruction::SourceDefined,
    optimality: InitialOptimalityRequirement::SourceDefined,
    negative_cycles: NegativeCyclePolicy::SourceDefined,
};
const PROJECT_ORACLE: InitialContract = InitialContract {
    construction: InitialConstruction::ProjectOracleConstructed,
    optimality: InitialOptimalityRequirement::ProjectOracleConstructed,
    negative_cycles: NegativeCyclePolicy::SourceDefined,
};

macro_rules! initial_contract {
    (ZeroFeasible) => {
        MAX_FLOW_ZERO
    };
    (ZeroPseudoflowWithImbalance) => {
        SSP_PSEUDOFLOW
    };
    (CapacityScalingPseudoflow) => {
        CAPACITY_SCALING_PSEUDOFLOW
    };
    (EnhancedCapacityScaling) => {
        ENHANCED_CAPACITY_SCALING
    };
    (AnyFeasible) => {
        ANY_FEASIBLE_RESOLVE
    };
    (DualFeasible) => {
        DUAL_FEASIBLE
    };
    (EpsilonOptimal) => {
        EPSILON_OPTIMAL
    };
    (SourceDefined) => {
        SOURCE_DEFINED
    };
    (ProjectOracle) => {
        PROJECT_ORACLE
    };
}

macro_rules! descriptor {
    ($algorithm_id:expr, $id:literal, $title:literal, [$($alias:literal),* $(,)?],
     $kind:ident, $family:ident, $problems:ident, $runtime_route:ident,
     $initial_contract:ident, $exact:literal, $randomized:literal,
     $complexity:literal, $source:literal, $band:ident, $scope:ident,
     $status:ident) => {
        descriptor!(
            $algorithm_id, $id, $title, [$($alias),*], $kind, $family, $problems,
            $runtime_route, $initial_contract,
            $exact, $randomized, $complexity, $source, $band, $scope, $status,
            catalog_graph_requirements!($problems)
        )
    };
    ($algorithm_id:expr, $id:literal, $title:literal, [$($alias:literal),* $(,)?],
     $kind:ident, $family:ident, $problems:ident, $runtime_route:ident,
     $initial_contract:ident, $exact:literal, $randomized:literal,
     $complexity:literal, $source:literal, $band:ident, $scope:ident,
     $status:ident, $graph_requirements:expr) => {
        AlgorithmDescriptor {
            algorithm_id: $algorithm_id,
            id: $id,
            title: $title,
            aliases: &[$($alias),*],
            search_terms: algorithm_search_terms($algorithm_id),
            kind: CatalogKind::$kind,
            family: AlgorithmFamily::$family,
            trace_steps: algorithm_step_contract($algorithm_id, AlgorithmFamily::$family),
            problems: $problems,
            models: catalog_models!($problems),
            runtime_route: RuntimeRouteKind::$runtime_route,
            graph_requirements: $graph_requirements,
            initial_construction: initial_contract!($initial_contract).construction,
            initial_optimality: initial_contract!($initial_contract).optimality,
            initial_oracle_dependency: initial_oracle_dependency($algorithm_id),
            negative_cycle_policy: initial_contract!($initial_contract).negative_cycles,
            exact: $exact,
            randomized: $randomized,
            complexity: $complexity,
            source_id: $source,
            initial_band: $band,
            admission_contract: algorithm_admission_contract($algorithm_id),
            status: ImplementationStatus::$status,
            implementation_scope: ImplementationScope::$scope,
            terminal_oracle_dependency: terminal_oracle_dependency($algorithm_id),
        }
    };
}

/// Returns discovery-only parent names for related executable endpoints.
///
/// These terms are intentionally separate from aliases: resolving one helps a
/// reader discover the related component or demonstrator while its `kind` and
/// `implementation_scope` continue to state that it is not the complete parent
/// solver.
#[must_use]
pub const fn algorithm_search_terms(id: AlgorithmId) -> &'static [&'static str] {
    match id {
        AlgorithmId::TardosFramework => &["Tardos Strongly Polynomial Algorithm"],
        AlgorithmId::RandomizedAlmostLinearMaxFlow => &["Randomized almost-linear max flow"],
        AlgorithmId::DeterministicAlmostLinearMaxFlow => &["Deterministic almost-linear max flow"],
        AlgorithmId::RandomizedAlmostLinearMcf => &["Randomized almost-linear minimum-cost flow"],
        _ => &[],
    }
}

/// Returns project-owned optimum information consumed before source progress.
#[must_use]
pub const fn initial_oracle_dependency(id: AlgorithmId) -> InitialOracleDependency {
    match id {
        AlgorithmId::AugmentingElectricalFlow
        | AlgorithmId::InteriorPointMaxFlow
        | AlgorithmId::RandomizedAlmostLinearMaxFlow
        | AlgorithmId::DeterministicAlmostLinearMaxFlow => {
            InitialOracleDependency::ProjectExactMaxFlowScalarTarget
        }
        AlgorithmId::DeterministicAlmostLinearMcf => {
            InitialOracleDependency::ProjectExactMinCostScalarOptimum
        }
        AlgorithmId::RandomizedAlmostLinearMcf => {
            InitialOracleDependency::ProjectOptimumVectorInitialState
        }
        AlgorithmId::ElectricalFlowInteriorPointMcf => {
            InitialOracleDependency::ProjectIsolationFaceOptimumFacts
        }
        AlgorithmId::MinimumRatioCycleMcf => {
            InitialOracleDependency::ProjectFeasibleFaceInitialStateAndScalarOptimum
        }
        _ => InitialOracleDependency::None,
    }
}

/// Returns project-owned optimum information used after a source prefix.
#[must_use]
pub const fn terminal_oracle_dependency(id: AlgorithmId) -> TerminalOracleDependency {
    match id {
        AlgorithmId::RandomizedAlmostLinearMaxFlow
        | AlgorithmId::DeterministicAlmostLinearMaxFlow
        | AlgorithmId::RandomizedAlmostLinearMcf => {
            TerminalOracleDependency::ProjectOptimumVectorFinalPoint
        }
        _ => TerminalOracleDependency::None,
    }
}

const GENERAL_ADMISSION_CONTRACT: AlgorithmAdmissionContractV1 = AlgorithmAdmissionContractV1 {
    min_nodes: None,
    min_edges: None,
    max_nodes: None,
    max_edges: None,
    max_capacity: None,
    max_absolute_cost: None,
    max_assignment_space: None,
    max_capacity_state_space: None,
    strict_interior_required: false,
    min_dynamic_capacity_updates: None,
    max_dynamic_capacity_updates: None,
    capacity_updates_only: false,
};

#[allow(
    clippy::cast_possible_truncation,
    reason = "solver-owned practical limits are compile-time small integers and are cross-checked below"
)]
const fn admission_u32(value: usize) -> u32 {
    value as u32
}

const fn max_flow_admission_contract(id: AlgorithmId) -> Option<AlgorithmAdmissionContractV1> {
    match id {
        AlgorithmId::DynamicEibfs => Some(AlgorithmAdmissionContractV1 {
            min_dynamic_capacity_updates: Some(1),
            max_dynamic_capacity_updates: Some(256),
            capacity_updates_only: true,
            ..GENERAL_ADMISSION_CONTRACT
        }),
        AlgorithmId::ElectricalFlow => Some(AlgorithmAdmissionContractV1 {
            min_nodes: Some(2),
            min_edges: Some(1),
            max_capacity: Some(AdmissionLimitU64(crate::ELECTRICAL_FLOW_MAX_CAPACITY)),
            ..GENERAL_ADMISSION_CONTRACT
        }),
        AlgorithmId::AugmentingElectricalFlow => Some(AlgorithmAdmissionContractV1 {
            min_nodes: Some(2),
            min_edges: Some(1),
            max_capacity: Some(AdmissionLimitU64(crate::AUGMENTING_ELECTRICAL_MAX_CAPACITY)),
            ..GENERAL_ADMISSION_CONTRACT
        }),
        AlgorithmId::MinimumRatioCycleMaxFlow => Some(AlgorithmAdmissionContractV1 {
            min_nodes: Some(2),
            min_edges: Some(1),
            max_capacity: Some(AdmissionLimitU64(crate::MINIMUM_RATIO_CYCLE_MAX_LENGTH)),
            max_absolute_cost: Some(AdmissionLimitU64(
                crate::MINIMUM_RATIO_CYCLE_MAX_ABS_GRADIENT.unsigned_abs(),
            )),
            ..GENERAL_ADMISSION_CONTRACT
        }),
        AlgorithmId::WeightedAugmentingPaths => Some(AlgorithmAdmissionContractV1 {
            min_nodes: Some(2),
            min_edges: Some(1),
            max_capacity: Some(AdmissionLimitU64(
                crate::WEIGHTED_AUGMENTING_PATHS_MAX_CAPACITY,
            )),
            ..GENERAL_ADMISSION_CONTRACT
        }),
        AlgorithmId::WeightedPushRelabel => Some(AlgorithmAdmissionContractV1 {
            min_nodes: Some(2),
            min_edges: Some(1),
            max_capacity: Some(AdmissionLimitU64(
                crate::WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_CAPACITY,
            )),
            ..GENERAL_ADMISSION_CONTRACT
        }),
        AlgorithmId::RandomizedAlmostLinearMaxFlow => Some(AlgorithmAdmissionContractV1 {
            min_nodes: Some(2),
            min_edges: Some(1),
            max_capacity_state_space: Some(AdmissionLimitU64(
                crate::RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_ASSIGNMENTS,
            )),
            ..GENERAL_ADMISSION_CONTRACT
        }),
        AlgorithmId::DeterministicAlmostLinearMaxFlow => Some(AlgorithmAdmissionContractV1 {
            min_nodes: Some(2),
            min_edges: Some(1),
            max_capacity_state_space: Some(AdmissionLimitU64(
                crate::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_ASSIGNMENTS,
            )),
            ..GENERAL_ADMISSION_CONTRACT
        }),
        _ => None,
    }
}

const fn min_cost_admission_contract(id: AlgorithmId) -> AlgorithmAdmissionContractV1 {
    match id {
        AlgorithmId::MinimumRatioCycleMcf => AlgorithmAdmissionContractV1 {
            min_nodes: Some(2),
            min_edges: Some(1),
            max_nodes: Some(admission_u32(crate::MINIMUM_RATIO_CYCLE_MCF_MAX_NODES)),
            max_edges: Some(admission_u32(crate::MINIMUM_RATIO_CYCLE_MCF_MAX_EDGES)),
            max_capacity: Some(AdmissionLimitU64(
                crate::MINIMUM_RATIO_CYCLE_MCF_MAX_CAPACITY,
            )),
            max_absolute_cost: Some(AdmissionLimitU64(crate::MINIMUM_RATIO_CYCLE_MCF_MAX_COST)),
            max_assignment_space: Some(AdmissionLimitU64(
                crate::MINIMUM_RATIO_CYCLE_MCF_MAX_ENUMERATED_ASSIGNMENTS,
            )),
            ..GENERAL_ADMISSION_CONTRACT
        },
        AlgorithmId::RandomizedAlmostLinearMcf => AlgorithmAdmissionContractV1 {
            min_nodes: Some(1),
            min_edges: Some(1),
            max_nodes: Some(admission_u32(crate::RANDOMIZED_ALMOST_LINEAR_MCF_MAX_NODES)),
            max_edges: Some(admission_u32(crate::RANDOMIZED_ALMOST_LINEAR_MCF_MAX_EDGES)),
            max_capacity: Some(AdmissionLimitU64(
                crate::RANDOMIZED_ALMOST_LINEAR_MCF_MAX_CAPACITY,
            )),
            max_absolute_cost: Some(AdmissionLimitU64(
                crate::RANDOMIZED_ALMOST_LINEAR_MCF_MAX_COST,
            )),
            max_assignment_space: Some(AdmissionLimitU64(
                crate::RANDOMIZED_ALMOST_LINEAR_MCF_MAX_ASSIGNMENTS,
            )),
            ..GENERAL_ADMISSION_CONTRACT
        },
        AlgorithmId::DeterministicAlmostLinearMcf => AlgorithmAdmissionContractV1 {
            min_nodes: Some(2),
            min_edges: Some(1),
            max_nodes: Some(admission_u32(crate::FLOW_FRAMEWORK_MCF_MAX_NODES)),
            max_edges: Some(admission_u32(crate::FLOW_FRAMEWORK_MCF_MAX_EDGES)),
            max_capacity: Some(AdmissionLimitU64(crate::FLOW_FRAMEWORK_MCF_MAX_CAPACITY)),
            max_absolute_cost: Some(AdmissionLimitU64(crate::FLOW_FRAMEWORK_MCF_MAX_COST)),
            max_assignment_space: Some(AdmissionLimitU64(
                crate::FLOW_FRAMEWORK_MCF_MAX_ASSIGNMENTS,
            )),
            strict_interior_required: true,
            ..GENERAL_ADMISSION_CONTRACT
        },
        AlgorithmId::ElectricalFlowInteriorPointMcf => AlgorithmAdmissionContractV1 {
            max_nodes: Some(admission_u32(crate::ELECTRICAL_IPM_MCF_MAX_NODES)),
            max_edges: Some(admission_u32(crate::ELECTRICAL_IPM_MCF_MAX_EDGES)),
            max_capacity: Some(AdmissionLimitU64(crate::ELECTRICAL_IPM_MCF_MAX_CAPACITY)),
            max_absolute_cost: Some(AdmissionLimitU64(crate::ELECTRICAL_IPM_MCF_MAX_COST)),
            max_assignment_space: Some(AdmissionLimitU64(
                crate::ELECTRICAL_IPM_MCF_MAX_ENUMERATED_ASSIGNMENTS,
            )),
            ..GENERAL_ADMISSION_CONTRACT
        },
        AlgorithmId::PrimalDualInteriorPointMcf => AlgorithmAdmissionContractV1 {
            max_nodes: Some(admission_u32(crate::PRIMAL_DUAL_IPM_MCF_MAX_NODES)),
            max_edges: Some(admission_u32(crate::PRIMAL_DUAL_IPM_MCF_MAX_EDGES)),
            max_capacity: Some(AdmissionLimitU64(crate::PRIMAL_DUAL_IPM_MCF_MAX_CAPACITY)),
            max_absolute_cost: Some(AdmissionLimitU64(crate::PRIMAL_DUAL_IPM_MCF_MAX_COST)),
            ..GENERAL_ADMISSION_CONTRACT
        },
        _ => GENERAL_ADMISSION_CONTRACT,
    }
}

/// Returns the concrete kernel admission envelope from solver-owned constants.
#[must_use]
pub const fn algorithm_admission_contract(id: AlgorithmId) -> AlgorithmAdmissionContractV1 {
    match max_flow_admission_contract(id) {
        Some(contract) => contract,
        None => min_cost_admission_contract(id),
    }
}

macro_rules! flow_algorithms {
    ($($variant:ident => descriptor!($id:literal, $($descriptor:tt)*)),* $(,)?) => {
        /// Closed identity of every algorithm, variant, heuristic, and primitive.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum AlgorithmId {
            $(
                #[doc = concat!("Canonical ID `", $id, "`.")]
                $variant
            ),*
        }

        impl AlgorithmId {
            /// Catalog order, shared by serialization, conformance, and runtime tests.
            pub const ALL: &'static [Self] = &[$(Self::$variant),*];

            /// Returns the canonical stable kebab-case wire identifier.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $id),*
                }
            }
        }

        impl fmt::Display for AlgorithmId {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl Serialize for AlgorithmId {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl FromStr for AlgorithmId {
            type Err = UnknownAlgorithmId;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($id => Ok(Self::$variant),)*
                    _ => Err(UnknownAlgorithmId {
                        value: value.to_owned(),
                    }),
                }
            }
        }

        static ALGORITHM_CATALOG: &[AlgorithmDescriptor] = &[
            $(descriptor!(AlgorithmId::$variant, $id, $($descriptor)*)),*
        ];
    };
}

/// Error returned when a value is not one of the 93 canonical algorithm IDs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownAlgorithmId {
    value: String,
}

impl fmt::Display for UnknownAlgorithmId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown flow algorithm ID {:?}", self.value)
    }
}

impl std::error::Error for UnknownAlgorithmId {}

#[rustfmt::skip]
flow_algorithms! {
    FordFulkerson => descriptor!("ford-fulkerson", "Ford–Fulkerson", ["Ford-Fulkerson"], Solver, AugmentingPath, MAX, MaxFlow, ZeroFeasible, true, false, "O(mF) for integral capacities", "ford-fulkerson-1956", BAND_CLASSICAL, SourceComplete, Executable),
    DfsFordFulkerson => descriptor!("dfs-ford-fulkerson", "DFS Ford–Fulkerson", ["DFS Ford-Fulkerson"], Variant, AugmentingPath, MAX, MaxFlow, ZeroFeasible, true, false, "O(mF) for integral capacities", "ford-fulkerson-1956", BAND_CLASSICAL, SourceComplete, Executable),
    EdmondsKarp => descriptor!("edmonds-karp", "Edmonds–Karp", ["Edmonds-Karp"], Variant, AugmentingPath, MAX, MaxFlow, ZeroFeasible, true, false, "O(n m^2)", "edmonds-karp-1972", BAND_CLASSICAL, SourceComplete, Executable),
    ShortestAugmentingPath => descriptor!("shortest-augmenting-path", "Shortest Augmenting Path", ["SAP"], Solver, AugmentingPath, MAX, MaxFlow, ZeroFeasible, true, false, "O(n^2 m)", "ahuja-orlin-distance-directed-1991", BAND_CLASSICAL, SourceComplete, Executable),
    Isap => descriptor!("isap", "Improved Shortest Augmenting Path", ["ISAP"], Variant, AugmentingPath, MAX, MaxFlow, ZeroFeasible, true, false, "O(n^2 m)", "ahuja-orlin-distance-directed-1991", BAND_CLASSICAL, SourceComplete, Executable),
    WidestAugmentingPath => descriptor!("widest-augmenting-path", "Widest/Fattest Augmenting Path", ["Widest Path", "Fattest Path", "Widest/Fattest Path"], Variant, AugmentingPath, MAX, MaxFlow, ZeroFeasible, true, false, "weakly polynomial for integral capacities; see source record", "edmonds-karp-1972", BAND_CLASSICAL, SourceComplete, Executable),
    CapacityScalingAugmentingPath => descriptor!("capacity-scaling-augmenting-path", "Capacity-Scaling Augmenting Path", ["Capacity Scaling Ford–Fulkerson"], Variant, AugmentingPath, MAX, MaxFlow, ZeroFeasible, true, false, "O(m^2 log U) in the catalog implementation", "gabow-1985-scaling", BAND_CLASSICAL, SourceComplete, Executable),
    DistanceDirectedAugmentingPath => descriptor!("distance-directed-augmenting-path", "Distance-Directed Exact Tree (DD2)", ["Distance-Directed Augmenting Path", "Ahuja-Orlin DD2"], Solver, AugmentingPath, MAX, MaxFlow, ZeroFeasible, true, false, "DD2 kernel O(n^2 m); bounded build audits every repaired tree", "ahuja-orlin-distance-directed-1991", BAND_SMALL, SourceComplete, Executable),
    DistanceDirectedScalingAugmentingPath => descriptor!("distance-directed-scaling-augmenting-path", "Capacity-Scaled Distance-Directed DD2", ["Scaling DD2", "Distance-Directed Capacity Scaling"], Variant, AugmentingPath, MAX, MaxFlow, ZeroFeasible, true, false, "integral DD2 kernel O(n m log U); bounded build audits every repaired tree", "ahuja-orlin-distance-directed-1991", BAND_SMALL, SourceComplete, Executable),
    Dinic => descriptor!("dinic", "Dinic/Dinitz", ["Dinic", "Dinitz"], Solver, BlockingFlow, MAX, MaxFlow, ZeroFeasible, true, false, "O(n^2 m)", "dinitz-1970", BAND_CLASSICAL, SourceComplete, Executable),
    UnitCapacityDinic => descriptor!("unit-capacity-dinic", "Unit-Capacity Dinic", ["Unit-Capacity Dinic"], Variant, BlockingFlow, MAX, MaxFlow, ZeroFeasible, true, false, "O(m min(n^(2/3), sqrt(m)))", "even-tarjan-1975", BAND_CLASSICAL, SourceComplete, Executable, UNIT_CAPACITY_REQUIREMENTS),
    UnitNetworkDinic => descriptor!("unit-network-dinic", "Unit-Network Dinic", ["Dinic for unit networks"], Variant, BlockingFlow, MAX, MaxFlow, ZeroFeasible, true, false, "O(m sqrt(n))", "even-tarjan-1975", BAND_CLASSICAL, SourceComplete, Executable, UNIT_NETWORK_REQUIREMENTS),
    KarzanovPreflow => descriptor!("karzanov-preflow", "Karzanov Preflow", ["Karzanov"], Solver, BlockingFlow, MAX, MaxFlow, ZeroFeasible, true, false, "O(n^3)", "karzanov-1974", BAND_SMALL, SourceComplete, Executable),
    Mpm => descriptor!("mpm", "MPM", ["Malhotra–Pramodh Kumar–Maheshwari", "Malhotra-Pramodh Kumar-Maheshwari"], Solver, BlockingFlow, MAX, MaxFlow, ZeroFeasible, true, false, "O(n^3)", "malhotra-kumar-maheshwari-1978", BAND_SMALL, SourceComplete, Executable),
    DynamicTreeBlockingFlow => descriptor!("dynamic-tree-blocking-flow", "Dynamic-Tree Blocking Flow", ["Dynamic Tree Blocking Flow"], Variant, BlockingFlow, MAX, MaxFlow, ZeroFeasible, true, false, "O(n m log n)", "sleator-tarjan-1983", BAND_SMALL, SourceComplete, Executable),
    BinaryBlockingFlow => descriptor!("binary-blocking-flow", "Binary Blocking Flow Primitive", ["Binary Blocking Flow"], Primitive, BlockingFlow, MAX, MaxFlow, SourceDefined, true, false, "one bounded exact Goldberg-Rao binary-length subproblem; not a max-flow solver", "goldberg-rao-1998", BAND_SMALL, SourceComponent, Executable),
    GoldbergRao => descriptor!("goldberg-rao", "Goldberg–Rao", ["Goldberg-Rao"], Solver, BlockingFlow, MAX, MaxFlow, ZeroFeasible, true, false, "bounded exact binary-length phases; explicit SCC/DAG blocking work", "goldberg-rao-1998", BAND_SMALL, SourceComplete, Executable),
    GenericPushRelabel => descriptor!("generic-push-relabel", "Generic Push–Relabel", ["Push–Relabel", "Push-Relabel"], Solver, PushRelabel, MAX, MaxFlow, ZeroFeasible, true, false, "O(n^2 m)", "goldberg-tarjan-1988", BAND_CLASSICAL, SourceComplete, Executable),
    FifoPushRelabel => descriptor!("fifo-push-relabel", "FIFO Push–Relabel", ["FIFO Push-Relabel"], Variant, PushRelabel, MAX, MaxFlow, ZeroFeasible, true, false, "O(n^3)", "goldberg-tarjan-1988", BAND_CLASSICAL, SourceComplete, Executable),
    RelabelToFront => descriptor!("relabel-to-front", "Relabel-to-Front", ["Relabel to Front"], Variant, PushRelabel, MAX, MaxFlow, ZeroFeasible, true, false, "O(n^3)", "goldberg-tarjan-1988", BAND_CLASSICAL, SourceComplete, Executable),
    HighestLabelPushRelabel => descriptor!("highest-label-push-relabel", "Highest-Label Push–Relabel", ["Highest-Label", "HLPP"], Variant, PushRelabel, MAX, MaxFlow, ZeroFeasible, true, false, "O(n^2 sqrt(m))", "cheriyan-maheshwari-1989", BAND_CLASSICAL, SourceComplete, Executable),
    ExcessScalingPushRelabel => descriptor!("excess-scaling-push-relabel", "Excess-Scaling Push–Relabel", ["Excess Scaling Push-Relabel"], Variant, PushRelabel, MAX, MaxFlow, ZeroFeasible, true, false, "O(n m + n^2 log U)", "ahuja-orlin-excess-scaling-1989", BAND_SMALL, SourceComplete, Executable),
    DynamicTreePushRelabel => descriptor!("dynamic-tree-push-relabel", "Dynamic-Tree Push–Relabel", ["Dynamic Tree Push-Relabel"], Variant, PushRelabel, MAX, MaxFlow, ZeroFeasible, true, false, "O(n m log(n^2/m))", "goldberg-tarjan-1988", BAND_SMALL, SourceComplete, Executable),
    PartialAugmentRelabelMaxFlow => descriptor!("partial-augment-relabel-max-flow", "Partial Augment–Relabel", ["Partial Augment-Relabel Max Flow", "Partial Augment–Relabel Max Flow"], Variant, PushRelabel, MAX, MaxFlow, ZeroFeasible, true, false, "polynomial; source-specific bound", "goldberg-2008-partial-augment-relabel", BAND_CLASSICAL, SourceComplete, Executable),
    SynchronousParallelPushRelabel => descriptor!("synchronous-parallel-push-relabel", "Synchronous Parallel Push–Relabel (CPU)", ["Parallel Push–Relabel", "Parallel/GPU Push–Relabel", "GPU Push–Relabel (unsupported alias)"], Variant, PushRelabel, MAX, MaxFlow, ZeroFeasible, true, false, "source-defined synchronous rounds; bounded deterministic CPU simulation", "baumstark-blelloch-shun-2015", BAND_SMALL, SourceComplete, Executable),
    CurrentArcHeuristic => descriptor!("current-arc-heuristic", "Current Arc", ["current arc optimization"], Heuristic, PushRelabel, MAX, MaxFlow, ZeroFeasible, true, false, "does not change the generic push-relabel bound; avoids rescanning arcs at an unchanged height", "goldberg-tarjan-1988", BAND_CLASSICAL, SourceComponent, Executable),
    GlobalRelabelHeuristic => descriptor!("global-relabel-heuristic", "Global Relabeling", ["global relabel"], Heuristic, PushRelabel, MAX, MaxFlow, ZeroFeasible, true, false, "does not change the generic worst-case bound", "cherkassky-goldberg-1997", BAND_CLASSICAL, SourceComponent, Executable),
    GapRelabelHeuristic => descriptor!("gap-relabel-heuristic", "Gap Relabeling", ["gap relabel", "gap heuristic"], Heuristic, PushRelabel, MAX, MaxFlow, ZeroFeasible, true, false, "does not change the generic worst-case bound", "cherkassky-goldberg-1997", BAND_CLASSICAL, SourceComponent, Executable),
    HochbaumPseudoflow => descriptor!("hochbaum-pseudoflow", "Hochbaum Pseudoflow", ["Pseudoflow"], Solver, Pseudoflow, MAX, MaxFlow, SourceDefined, true, false, "O(n m log n) with dynamic trees; bounded explicit-tree trace kernel", "hochbaum-2008", BAND_SMALL, SourceComplete, Executable),
    PseudoflowSimplex => descriptor!("pseudoflow-simplex", "Pseudoflow Simplex", ["Pseudoflow Simplex"], Solver, Pseudoflow, MAX, MaxFlow, SourceDefined, true, false, "O(m n log n) with the source dynamic-tree structure; bounded explicit-basis implementation does not claim that end-to-end bound", "hochbaum-2008-pseudoflow-simplex", BAND_SMALL, SourceComplete, Executable),
    ParametricPseudoflow => descriptor!("parametric-pseudoflow", "Parametric Pseudoflow", ["Parametric Pseudoflow"], Solver, Pseudoflow, PARAMETRIC, ParametricMaxFlow, SourceDefined, true, false, "bounded explicit-tree retained-forest traversal; the source dynamic-tree bound is not claimed", "hochbaum-2008-parametric-section-9", BAND_RESEARCH, SourceComplete, Executable),
    ParametricBreakpointRerun => descriptor!("parametric-breakpoint-rerun", "Parametric Breakpoint Cold-Rerun Oracle", ["Parametric Breakpoint Rerun"], Variant, Pseudoflow, PARAMETRIC, ParametricMaxFlow, SourceDefined, true, false, "bounded exact recursive analysis with cold static pseudoflow and independent Edmonds-Karp runs", "gallo-grigoriadis-tarjan-1989-parametric", BAND_RESEARCH, SourceComplete, Executable),
    BoykovKolmogorov => descriptor!("boykov-kolmogorov", "Boykov–Kolmogorov", ["Boykov-Kolmogorov", "BK"], Solver, VisionGraph, MAX, MaxFlow, ZeroFeasible, true, false, "O(m n^2 |C|) trivial source bound; retained two-tree FIFO implementation", "boykov-kolmogorov-2004", BAND_SMALL, SourceComplete, Executable),
    Ibfs => descriptor!("ibfs", "Incremental Breadth-First Search", ["IBFS"], Solver, VisionGraph, MAX, MaxFlow, ZeroFeasible, true, false, "O(n^2 m)", "goldberg-hed-2011", BAND_SMALL, SourceComplete, Executable),
    Eibfs => descriptor!("eibfs", "Excesses IBFS", ["EIBFS", "Enhanced IBFS", "Enhanced IBFS (legacy misnomer)"], Solver, VisionGraph, MAX, MaxFlow, ZeroFeasible, true, false, "O(n^2 m) without dynamic trees", "goldberg-hed-2015", BAND_SMALL, SourceComplete, Executable),
    HopcroftKarp => descriptor!("hopcroft-karp", "Hopcroft–Karp", ["Hopcroft-Karp"], Solver, Special, MATCHING, BipartiteMatching, ZeroFeasible, true, false, "O((m+n) sqrt(n))", "hopcroft-karp-1973", BAND_CLASSICAL, SourceComplete, Executable),
    HassinStPlanar => descriptor!("hassin-st-planar", "Hassin st-Planar Dual Shortest-Path", ["平面グラフ双対最短路法", "st-planar dual shortest path"], Solver, Special, PLANAR, PlanarMaxFlow, ZeroFeasible, true, false, "O((n+m) log(n+m)) explicit split-dual Dijkstra", "hassin-1981", BAND_SMALL, SourceComplete, Executable),
    BorradaileKleinPlanar => descriptor!("borradaile-klein-planar", "Borradaile–Klein Leftmost-Path Planar Max Flow", ["Borradaile-Klein Planar Max Flow"], Solver, Special, PLANAR, PlanarMaxFlow, ZeroFeasible, true, false, "O(m(n+m)) bounded explicit-tree variant after O(m log n) dual preprocessing; not the source dynamic-tree bound", "borradaile-klein-2009", BAND_SMALL, SourceComplete, Executable),
    ElectricalFlow => descriptor!("electrical-flow", "Electrical Flow · Unit-Current Laplacian Primitive", ["Electrical Flow"], Primitive, Continuous, MAX, MaxFlow, SourceDefined, false, false, "source §2.3 minimum-energy primitive; bounded dense Jacobi-PCG plus exact rational oracle does not claim the nearly-linear solver bound", "christiano-kelner-madry-spielman-teng-2011", BAND_ELECTRICAL_FLOW, SourceComponent, Executable, ELECTRICAL_FLOW_REQUIREMENTS),
    AugmentingElectricalFlow => descriptor!("augmenting-electrical-flow", "Augmenting Electrical Flow · Bounded §3/§4", ["Augmenting Electrical Flows"], Solver, Continuous, MAX, MaxFlow, SourceDefined, true, false, "source §3 l4-safe progress with bounded explicit §4 boosts and exact cleanup; does not claim the source O~(m^(10/7) U^(1/7)) end-to-end bound", "madry-2016", BAND_AUGMENTING_ELECTRICAL, BoundedOracleGuided, Executable, AUGMENTING_ELECTRICAL_REQUIREMENTS),
    InteriorPointMaxFlow => descriptor!("interior-point-max-flow", "Interior-Point Max Flow · Bounded §2–§5", ["Interior-Point Max Flow"], Solver, Continuous, MAX, MaxFlow, SourceDefined, true, false, "source §5 O~(m^(3/2)) path-following kernel with dense exact electrical solves, path decomposition, b-matching rounding, and augmenting completion; bounded exact cut oracle installs F* and the implementation does not claim the source O~(m^(10/7)) end-to-end bound", "madry-2013", BAND_INTERIOR_POINT_MAX_FLOW, BoundedOracleGuided, Executable, UNIT_IPM_MAX_FLOW_REQUIREMENTS),
    MinimumRatioCycleMaxFlow => descriptor!("minimum-ratio-cycle-max-flow", "Minimum-Ratio Cycle · Exact Bounded Primitive", ["Minimum-Ratio Cycle Framework", "Minimum-Ratio-Cycle Framework"], Primitive, Continuous, MAX, MaxFlow, SourceDefined, true, false, "source objective min g^T delta / ||diag(l) delta||_1; bounded exact ternary enumeration plus independent DFS oracle does not claim the randomized dynamic-data-structure bound", "chen-kyng-liu-peng-2025", BAND_MINIMUM_RATIO_CYCLE, SourceComponent, Executable),
    OrlinMaxFlow => descriptor!("orlin-max-flow", "Orlin Max Flow · Explicit Compact Network", ["Orlin Max Flow", "Orlin's Max Flow"], Solver, AdvancedDiscrete, MAX, MaxFlow, ZeroFeasible, true, false, "O(nm) source construction; bounded explicit transitive-closure and logical-flow realization does not claim the end-to-end bound", "orlin-2013", BAND_ORLIN_MAX_FLOW, SourceComplete, Executable),
    RandomizedAlmostLinearMaxFlow => descriptor!("randomized-almost-linear-max-flow-oracle-demonstrator", "Randomized Tree-Chain Prefix + Oracle Final-Point Demonstrator", ["Randomized tree-chain oracle demonstrator"], Primitive, Continuous, MAX, MaxFlow, SourceDefined, true, true, "project-owned bounded composite: return-edge reduction, up to 8 seeded forest-chain Query/Detect/Rebuild progress steps, then nearest-integer recovery from an optimum-vector-oracle-constructed final point; it is not a source-defined component, does not implement the source outer stopping loop, and does not claim the randomized almost-linear max-flow solver/runtime", "chen-kyng-liu-peng-2025", BAND_RANDOMIZED_ALMOST_LINEAR_MAX_FLOW, ProjectOracleDemonstrator, Executable, POSITIVE_SIMPLE_MAX_FLOW_REQUIREMENTS),
    DeterministicAlmostLinearMaxFlow => descriptor!("deterministic-almost-linear-max-flow-oracle-demonstrator", "Deterministic Shifted Tree-Chain Prefix + Oracle Final-Point Demonstrator", ["Deterministic shifted tree-chain oracle demonstrator"], Primitive, Continuous, MAX, MaxFlow, SourceDefined, true, false, "project-owned bounded composite: a fixed shifted-tree-chain source prefix followed by Kang--Payor recovery from an optimum-vector-oracle-constructed additive-gap-below-1/2 point; it is not a source-defined component, does not implement the source outer stopping loop, and does not claim the deterministic almost-linear max-flow solver/runtime", "van-den-brand-et-al-2023", BAND_DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW, ProjectOracleDemonstrator, Executable, POSITIVE_SIMPLE_MAX_FLOW_REQUIREMENTS),
    WeightedAugmentingPaths => descriptor!("weighted-augmenting-paths", "Weighted Augmenting Paths · Exact Bounded Hierarchy", ["Weighted Augmenting Path", "Weighted Augmenting Paths"], Solver, AdvancedDiscrete, MAX, MaxFlow, SourceDefined, true, false, "source capacity scaling, directed expander hierarchy, respecting-order weights, and relabel-prioritized path listing; exhaustive phi certification does not claim n^(2+o(1)) log U", "bernstein-et-al-2024", BAND_WEIGHTED_AUGMENTING_PATHS, BoundedOracleGuided, Executable, POSITIVE_SIMPLE_MAX_FLOW_REQUIREMENTS),
    WeightedPushRelabel => descriptor!("weighted-push-relabel", "Weighted Push–Relabel · Exact Bounded Shortcut Kernel", ["Weighted Push-Relabel", "Weighted Push–Relabel"], Solver, AdvancedDiscrete, MAX, MaxFlow, SourceDefined, true, false, "source weak hierarchy, Steiner-star shortcuts, weighted relabel closure, residual distance-layer cut, and weighted original-residual completion; one-level SCC construction does not claim O~(n^2 log U)", "bernstein-et-al-2025", BAND_WEIGHTED_PUSH_RELABEL_SHORTCUT, BoundedOracleGuided, Executable, POSITIVE_SIMPLE_MAX_FLOW_REQUIREMENTS),
    DynamicEibfs => descriptor!("dynamic-eibfs", "Dynamic EIBFS", ["Dynamic Max Flow (EIBFS)"], Solver, Dynamic, DYNAMIC_MAX, MaxFlow, SourceDefined, true, false, "O(n^2 m) per source-defined solve without dynamic trees; update sequence is reused", "goldberg-hed-2015", BAND_SMALL, SourceComplete, Executable, ZERO_FLOW_FEASIBLE_REQUIREMENTS),
    WarmStartPushRelabel => descriptor!("warm-start-push-relabel", "Warm-Start Push–Relabel", ["Dynamic Max Flow", "Dynamic Push–Relabel"], Solver, Dynamic, WARM_START, MaxFlow, SourceDefined, true, false, "O(eta n^2) for the source-defined integral predicted-pseudoflow pipeline; bounded explicit auxiliary networks", "davies-vassilvitskii-wang-2024", BAND_SMALL, SourceComplete, Executable),

    SimpleCycleCanceling => descriptor!("simple-cycle-canceling", "Simple Cycle Canceling", ["Cycle Canceling"], Solver, CycleCanceling, BF_MCF, MinCostFlow, AnyFeasible, true, false, "pseudo-polynomial; O(K n m) for K cycle searches in this implementation", "klein-1967", BAND_SMALL, SourceComplete, Executable),
    MinimumMeanCycleCanceling => descriptor!("minimum-mean-cycle-canceling", "Minimum-Mean Cycle Canceling", ["Minimum Mean Cycle Canceling"], Solver, CycleCanceling, BF_MCF, MinCostFlow, AnyFeasible, true, false, "O(n^2 m^3 log n) with Karp O(nm) cycle selection", "goldberg-tarjan-1989-cycle", BAND_SMALL, SourceComplete, Executable),
    CancelAndTighten => descriptor!("cancel-and-tighten", "Cancel-and-Tighten · Exact Rational", ["Cancel and Tighten"], Solver, CycleCanceling, BF_MCF, MinCostFlow, AnyFeasible, true, false, "explicit DFS cycle cancellation and topological tightening; O(min(m n^2 log(nC), m^2 n^2 log(2n))) source bound", "goldberg-tarjan-cancel-tighten-1989", BAND_SMALL, SourceComplete, Executable),
    RelaxedMostNegativeCycle => descriptor!("relaxed-most-negative-cycle", "Relaxed Most-Negative-Cycle", ["Relaxed Most Negative Cycle", "Relaxed MNDC", "Node-Disjoint Assignment Cycle Canceling"], Solver, CycleCanceling, BF_MCF, MinCostFlow, AnyFeasible, true, false, "generic primal algorithm: O(mn) canceled objects per epsilon phase plus one dense exact assignment per node-disjoint family in this bounded implementation", "shigeno-iwata-mccormick-2000", BAND_RESEARCH, SourceComplete, Executable),
    SuccessiveShortestPath => descriptor!("successive-shortest-path", "Successive Shortest Path", ["SSP"], Solver, ShortestPath, BF_MCF, MinCostFlow, ZeroPseudoflowWithImbalance, true, false, "O(F n m) with the deterministic Bellman-Ford shortest-path subroutine", "jewell-1958-ssp", BAND_CLASSICAL, SourceComplete, Executable),
    BellmanFordSsp => descriptor!("bellman-ford-ssp", "Bellman–Ford SSP", ["Bellman-Ford SSP"], Variant, ShortestPath, BF_MCF, MinCostFlow, ZeroPseudoflowWithImbalance, true, false, "O(F n m)", "jewell-1958-ssp", BAND_CLASSICAL, SourceComplete, Executable),
    PotentialDijkstraSsp => descriptor!("potential-dijkstra-ssp", "Potential + Dijkstra SSP", ["Reduced-Cost Dijkstra SSP", "Potential Dijkstra SSP"], Variant, ShortestPath, BF_MCF, MinCostFlow, ZeroPseudoflowWithImbalance, true, false, "O(nm + F (m+n log n)) with Bellman-Ford potential initialization", "johnson-1977-reweighting", BAND_CLASSICAL, SourceComplete, Executable),
    SuccessiveShortestAugmentingPath => descriptor!("successive-shortest-augmenting-path", "Successive Shortest Augmenting Path", ["SSAP"], Variant, ShortestPath, MCMF, MinCostMaxFlow, ZeroPseudoflowWithImbalance, true, false, "O(nm + F (m+n log n)) with Bellman-Ford potential initialization", "edmonds-karp-1972", BAND_CLASSICAL, SourceComplete, Executable, ZERO_FLOW_FEASIBLE_REQUIREMENTS),
    PrimalDualMcf => descriptor!("primal-dual-mcf", "Primal–Dual", ["Primal-Dual MCF"], Variant, PrimalDual, BF_MCF, MinCostFlow, DualFeasible, true, false, "O(nm + F (m+n log n)) with Bellman-Ford dual initialization", "edmonds-karp-1972", BAND_CLASSICAL, SourceComplete, Executable),
    BlockingFlowPrimalDual => descriptor!("blocking-flow-primal-dual", "Blocking-Flow Primal–Dual", ["Blocking Flow Primal-Dual"], Variant, PrimalDual, BF_MCF, MinCostFlow, DualFeasible, true, false, "pseudo-polynomial restricted-primal method; deterministic Dinitz phases with explicit work ceilings", "ford-fulkerson-1957-dinitz-blocking", BAND_SMALL, SourceComplete, Executable),
    CapacityScalingMcf => descriptor!("capacity-scaling-mcf", "Capacity Scaling", ["Capacity Scaling MCF"], Solver, Scaling, BF_MCF, MinCostFlow, CapacityScalingPseudoflow, true, false, "O(nm + (n+m)(m+n log n) log B) in the finite-capacity implementation", "abdulaziz-ammer-2024-capacity-scaling", BAND_CLASSICAL, SourceComplete, Executable),
    EnhancedCapacityScaling => descriptor!("enhanced-capacity-scaling", "Enhanced Capacity Scaling · Uncapacitated Transshipment", ["Enhanced Capacity Scaling MCF", "Orlin Enhanced Capacity Scaling"], Variant, Scaling, TRANSSHIPMENT_MCF, MinCostFlow, EnhancedCapacityScaling, true, false, "O(n log n (m+n log n)) on the source uncapacitated transshipment model; bounded explicit quotient scans", "orlin-1993", BAND_RESEARCH, SourceComplete, Executable, UNCAPACITATED_TRANSSHIPMENT_REQUIREMENTS),
    CostScaling => descriptor!("cost-scaling", "Cost Scaling", ["Cost Scaling MCF"], Solver, Scaling, BF_MCF, MinCostFlow, EpsilonOptimal, true, false, "O(n^2 m log(nC)) with generic current-arc refine", "goldberg-tarjan-1990-cost-scaling", BAND_COST_SCALING, SourceComplete, Executable),
    CostScalingPushRelabel => descriptor!("cost-scaling-push-relabel", "Cost-Scaling Push–Relabel", ["Cost Scaling Push-Relabel"], Variant, Scaling, BF_MCF, MinCostFlow, EpsilonOptimal, true, false, "O(n^2 m log(nC)) with generic current-arc refine", "goldberg-tarjan-1990-cost-scaling", BAND_COST_SCALING, SourceComplete, Executable),
    AugmentRelabel => descriptor!("augment-relabel", "Augment–Relabel", ["Augment-Relabel"], Variant, Scaling, BF_MCF, MinCostFlow, EpsilonOptimal, true, false, "O(n^2 m log(nC)); generic COS bound", "kiraly-kovacs-2012-augment-relabel", BAND_COST_SCALING, SourceComplete, Executable),
    PartialAugmentRelabelMcf => descriptor!("partial-augment-relabel-mcf", "Partial Augment–Relabel (MCF)", ["Partial Augment-Relabel MCF", "Partial Augment–Relabel MCF"], Variant, Scaling, BF_MCF, MinCostFlow, EpsilonOptimal, true, false, "O(n^2 m log(nC)); generic COS bound", "lemon-partial-augment-relabel", BAND_COST_SCALING, SourceComplete, Executable),
    PriceRefinement => descriptor!("price-refinement", "Price Refinement", ["Price Refinement"], Heuristic, Scaling, BF_MCF, MinCostFlow, EpsilonOptimal, true, false, "O(nm) per price attempt; O(n^2 m log(nC)) including exact fallback", "goldberg-1997-price-refinement", BAND_COST_SCALING, SourceComponent, Executable),
    ArcFixing => descriptor!("arc-fixing", "Bound-only Speculative Arc Fixing", ["Arc Fixing", "Speculative Arc Fixing"], Heuristic, Scaling, BF_MCF, MinCostFlow, EpsilonOptimal, true, false, "bound-only project variant; speculative repair work is limited by explicit transition/scan ceilings; no tighter asymptotic bound claimed", "goldberg-1997-price-refinement", BAND_COST_SCALING, SourceComponent, Executable),
    ExcessScalingMcf => descriptor!("excess-scaling-mcf", "Excess Scaling · Transshipment", ["Excess Scaling MCF", "Orlin Excess Scaling"], Variant, Scaling, BF_MCF, MinCostFlow, CapacityScalingPseudoflow, true, false, "O(n (m+n log n) log B) for the bounded exact implementation on nonbinding-capacity transshipment inputs", "orlin-excess-scaling-transshipment", BAND_SMALL, SourceComplete, Executable, NONBINDING_TRANSSHIPMENT_REQUIREMENTS),
    DoubleScaling => descriptor!("double-scaling", "Double Scaling · Transportation", ["Double Scaling", "Double-Scaling MCF"], Solver, Scaling, BF_MCF, MinCostFlow, AnyFeasible, true, false, "O(nm log U log(nC)) for the explicit admissible-path implementation; no dynamic-tree speedup claimed", "ahuja-goldberg-orlin-tarjan-1992", BAND_SMALL, SourceComplete, Executable),
    GeneralizedCostScaling => descriptor!("generalized-cost-scaling", "Generalized Cost Scaling · Push Refine", ["Generalized Cost Scaling", "Generalized Cost-Scaling", "Generalized Cost Scaling Push Refine"], Variant, Scaling, BF_MCF, MinCostFlow, EpsilonOptimal, true, false, "configured framework variant; O(n^2 m log(nC)) with explicit generic current-arc push–relabel refine", "goldberg-network-flow-algorithms-1989", BAND_COST_SCALING, SourceComplete, Executable),
    PrimalNetworkSimplex => descriptor!("primal-network-simplex", "Primal Network Simplex", ["Network Simplex"], Solver, Simplex, BF_MCF, MinCostFlow, AnyFeasible, true, false, "pseudo-polynomial; O(nmCU) pivots in the strong-feasible analysis and O(n+m) explicit tree rebuild per basis exchange", "kiraly-kovacs-2012-network-simplex", BAND_SMALL, SourceComplete, Executable),
    DualNetworkSimplex => descriptor!("dual-network-simplex", "Dual Network Simplex · Uncapacitated Transshipment", ["Dual Network Simplex"], Solver, Simplex, TRANSSHIPMENT_MCF, MinCostFlow, DualFeasible, true, false, "natural dual-Bland pivot; input-dependent pivot count with explicit O(n+m) tree rebuild and O(m) cut pricing per pivot", "orlin-plotkin-tardos-1993-dual-simplex", BAND_RESEARCH, SourceComplete, Executable, UNCAPACITATED_TRANSSHIPMENT_REQUIREMENTS),
    PolynomialPrimalNetworkSimplex => descriptor!("polynomial-primal-network-simplex", "Polynomial Primal Network Simplex · Scaling Premultiplier", ["Polynomial Network Simplex"], Variant, Simplex, MCF, MinCostFlow, SourceDefined, true, false, "polynomial scaling-premultiplier pivot rule; bounded explicit tree and residual scans", "orlin-polynomial-network-simplex", BAND_RESEARCH, SourceComplete, Executable),
    PolynomialDualNetworkSimplex => descriptor!("polynomial-dual-network-simplex", "Polynomial Dual Network Simplex · Scaling-Simplex", ["Polynomial Dual Network Simplex"], Variant, Simplex, TRANSSHIPMENT_MCF, MinCostFlow, SourceDefined, true, false, "O(n² log(nB)) source Figure 3 pivots; bounded explicit tree and cut scans", "orlin-plotkin-tardos-1993-polynomial-dual-simplex", BAND_RESEARCH, SourceComplete, Executable, UNCAPACITATED_TRANSSHIPMENT_REQUIREMENTS),
    DynamicTreeNetworkSimplex => descriptor!("dynamic-tree-network-simplex", "Dynamic-Tree Network Simplex", ["Dynamic Tree Network Simplex"], Variant, Simplex, MCF, MinCostFlow, AnyFeasible, true, false, "bounded exact link-cut cycle pivots; explicit O(n+m) potential rebuild per exchange", "sleator-tarjan-1983", BAND_SMALL, SourceComplete, Executable),
    TransportationSimplex => descriptor!("transportation-simplex", "Transportation Simplex", ["Transportation Simplex"], Solver, Transportation, TRANSPORT, Transportation, AnyFeasible, true, false, "finite Bland Rule I; pivot count is input dependent and bounded by an explicit work ceiling", "ye-bland-transportation-simplex", BAND_SMALL, SourceComplete, Executable),
    Modi => descriptor!("modi", "MODI", ["Modified Distribution Method"], Variant, Transportation, TRANSPORT, Transportation, AnyFeasible, true, false, "same finite Bland transportation-simplex kernel; MODI changes terminology and trace presentation only", "uv-modi-transportation-preset", BAND_SMALL, SourceComplete, Executable),
    OutOfKilter => descriptor!("out-of-kilter", "Out-of-Kilter", ["Out of Kilter"], Solver, PrimalDual, BF_MCF, MinCostFlow, AnyFeasible, true, false, "pseudo-polynomial; T_feasible(n,m) + O((K+1)(n + m log m)) + T_certificate(n,m) for K corrections", "fulkerson-out-of-kilter-1961", BAND_SMALL, SourceComplete, Executable),
    Relaxation => descriptor!("relaxation", "Relaxation", ["Relaxation MCF"], Solver, Relaxation, BF_MCF, MinCostFlow, SourceDefined, true, false, "pseudo-polynomial; T_feasible(n,m) + O(K n m) + T_certificate(n,m) for K root iterations in the explicit-scan kernel", "bertsekas-tseng-1988", BAND_SMALL, SourceComplete, Executable),
    EpsilonRelaxation => descriptor!("epsilon-relaxation", "Epsilon-Relaxation", ["ε-Relaxation", "Epsilon Relaxation"], Variant, Relaxation, BF_MCF, MinCostFlow, SourceDefined, true, false, "pseudo-polynomial; T_feasible(n,m) + O(K(n+m)) + T_certificate(n,m) for K pushes and price rises in the explicit-scan kernel", "bertsekas-eckstein-1988-epsilon-relaxation", BAND_SMALL, SourceComplete, Executable),
    Hungarian => descriptor!("hungarian", "Hungarian", ["Hungarian Method"], Solver, Assignment, ASSIGNMENT, Assignment, SourceDefined, true, false, "O(a^2 t), hence O(n^3), for a agents and t tasks with a <= t", "kuhn-tomizawa-edmonds-karp-hungarian", BAND_CLASSICAL, SourceComplete, Executable),
    Auction => descriptor!("auction", "Auction", ["Auction Algorithm"], Solver, Assignment, ASSIGNMENT, Assignment, EpsilonOptimal, true, false, "source O(n A log(nC)) for its particular scaled symmetric implementation; this bounded rectangular implementation resets to equal prices per scale", "bertsekas-auction-1988", BAND_CLASSICAL, SourceComplete, Executable),
    TardosFramework => descriptor!("tardos-framework", "Tardos Network-Matrix Variable Fixing", ["Tardos Variable-Fixing Primitive"], Primitive, StronglyPolynomial, MCF, MinCostFlow, SourceDefined, true, false, "T_feasible(n,m) plus one exact O(n+m) network-matrix proximity/fixing scan; the source meta-algorithm is strongly polynomial", "tardos-1986", BAND_RESEARCH, SourceComponent, Executable),
    OrlinMcf => descriptor!("orlin-mcf", "Orlin MCF · Finite-Capacity Transformation", ["Orlin Minimum-Cost Flow"], Solver, StronglyPolynomial, MCF, MinCostFlow, SourceDefined, true, false, "O(m log n (m+n log n)) in Orlin's Section 5 implementation; bounded explicit compressed scans", "orlin-1993", BAND_ORLIN_MCF, SourceComplete, Executable),
    PrimalDualInteriorPointMcf => descriptor!("primal-dual-interior-point-mcf", "Integer Primal–Dual Interior-Point MCF · Bounded Exact Forest", ["Primal-Dual Interior-Point"], Solver, Continuous, BF_MCF, MinCostFlow, SourceDefined, true, true, "source expected O~(m^(3/2)) integer path following; bounded exact forest enumeration does not claim the source runtime", "becker-karrenbauer-mehlhorn-2016", BAND_RESEARCH, BoundedOracleGuided, Executable),
    ElectricalFlowInteriorPointMcf => descriptor!("electrical-flow-interior-point-mcf", "Electrical-Flow Interior-Point MCF · Bounded Isolation", ["Electrical Flow Interior-Point"], Solver, Continuous, MCF, MinCostFlow, SourceDefined, true, true, "source expected O~(m^(3/2) log^2 U); bounded isolation/face oracle, dense electrical solves, and direct central-estimate rounding do not claim the source runtime", "daitch-spielman-2008", BAND_RESEARCH, BoundedOracleGuided, Executable),
    MinimumRatioCycleMcf => descriptor!("minimum-ratio-cycle-mcf", "Minimum-Ratio-Cycle MCF · Source Progress Step", ["Minimum Ratio Cycle MCF"], Primitive, Continuous, MCF, MinCostFlow, SourceDefined, true, false, "source alpha-power potential and exact bounded min-ratio-cycle progress step; exhaustive feasible-face and cycle oracles do not claim the source dynamic-data-structure runtime", "chen-kyng-liu-peng-2025", BAND_RESEARCH, SourceComponent, Executable),
    RandomizedAlmostLinearMcf => descriptor!("randomized-almost-linear-mcf-oracle-demonstrator", "Randomized MCF Tree-Chain Prefix + Oracle Final-Point Demonstrator", ["Randomized MCF tree-chain oracle demonstrator"], Primitive, Continuous, MCF, MinCostFlow, ProjectOracle, true, true, "project-owned bounded composite: one seeded tree-chain Query/Detect/Rebuild progress step followed by nearest-integer recovery from an optimum-vector-oracle-constructed 1/(12m^3U^3) final point; it is not a source-defined component, does not implement the source outer stopping loop, and does not claim the randomized almost-linear minimum-cost-flow solver/runtime", "chen-kyng-liu-peng-2025", BAND_RESEARCH, ProjectOracleDemonstrator, Executable),
    DeterministicAlmostLinearMcf => descriptor!("deterministic-almost-linear-mcf", "Flow Framework MCF · Bounded Source Coordinator", ["Deterministic almost-linear minimum-cost flow"], Solver, Continuous, BF_MCF, MinCostFlow, SourceDefined, true, false, "exact for strict-interior loop-free inputs inside the 6-node/8-edge bounded-oracle band; a streaming oracle retains only scalar F*, while source initial point, periodic reinitialization, HLD-refined F_T(R,pi), Detect, topology-aware Algorithm 2 Query, source-scaled progress, the deterministic additive-1/2 final-point gate, Kang--Payor rounding, and independent certification construct the published flow; the stronger CKLPPS (mU)^-10 loop threshold and m^(1+o(1)) runtime are not claimed", "van-den-brand-et-al-2023", BAND_DETERMINISTIC_ALMOST_LINEAR_MCF, BoundedOracleGuided, Executable, NO_SELF_LOOP_REQUIREMENTS),
    SegmentExpandedConvexMcf => descriptor!("segment-expanded-convex-mcf", "Segment-Expanded Convex MCF Oracle", ["Parallel-Segment Convex MCF", "Expanded Convex-Cost Flow"], Solver, Convex, CONVEX, ConvexCostFlow, AnyFeasible, true, false, "bounded expansion to M linear arcs plus minimum-mean cycle canceling", "convex-segment-expansion-textbook", BAND_CONVEX, SourceComplete, Executable),
    ConvexCostScaling => descriptor!("convex-cost-scaling", "Piecewise-Linear Convex Cost Scaling · Marginal Δ-Scaling", ["Convex Cost Scaling"], Solver, Convex, CONVEX, ConvexCostFlow, SourceDefined, true, false, "source O(M log U (m+n log M)); bounded native marginal scans plus independent expanded oracle", "pinto-shamir-1994", BAND_CONVEX, SourceComplete, Executable),
    ConvexNetworkSimplex => descriptor!("convex-network-simplex", "Convex Network Simplex · Pasche Combined Pivot", ["Convex-Cost Network Simplex"], Solver, Convex, CONVEX, ConvexCostFlow, SourceDefined, true, false, "finite compact combined pivots; bounded explicit pricing/cycle scans plus independent expanded oracle", "pasche-1987-combined-pivot-convex-simplex", BAND_CONVEX, SourceComplete, Executable),
    PredictionAssistedEpsilonRelaxation => descriptor!("prediction-assisted-epsilon-relaxation", "Prediction-Assisted ε-Relaxation · Robust Cost Scaling", ["Prediction-Assisted ε-Relaxation", "Prediction-Assisted Epsilon-Relaxation", "Minimum-Cost Flow with Dual Predictions"], Solver, Prediction, MCF, MinCostFlow, SourceDefined, true, false, "O(min(n^3 log(error+1), n^3 log(nC))) source schedule; deterministic explicit-scan implementation with Remark 1 attempt ceilings", "chen-yao-yin-2026", BAND_RESEARCH, SourceComplete, Executable),
}

#[allow(
    clippy::too_many_lines,
    reason = "the closed family table is clearer as one exhaustive match"
)]
const fn algorithm_family_step_units(
    family: AlgorithmFamily,
) -> (&'static str, &'static str, &'static str) {
    match family {
        AlgorithmFamily::AugmentingPath => (
            "one boundary that starts or completes a residual-path search",
            "one completed flow augmentation",
            "one residual-arc inspection or path-prefix extension inside the search",
        ),
        AlgorithmFamily::BlockingFlow => (
            "one level or binary-length blocking-flow phase",
            "one admissible propagation or blocking-path augmentation",
            "one admissible-edge inspection, path extension, or local propagation",
        ),
        AlgorithmFamily::PushRelabel => (
            "one active-label scheduling or global relabel phase",
            "one push, relabel, discharge, or state-changing heuristic application",
            "one active-vertex choice, admissible-arc inspection, or local state mutation",
        ),
        AlgorithmFamily::Pseudoflow => (
            "one pseudoflow forest growth or recovery phase",
            "one merger, split, label change, or flow-recovery mutation",
            "one rooted-forest inspection, merger candidate, split, or normalization push",
        ),
        AlgorithmFamily::VisionGraph => (
            "one search-tree growth, augmentation, or adoption phase",
            "one tree growth mutation, path augmentation, orphan adoption, or distance repair",
            "one active-tree edge inspection, bridge selection, or orphan repair step",
        ),
        AlgorithmFamily::Special => (
            "one structure-specific search phase",
            "one matching, planar-dual, or specialized flow update",
            "one structure-specific edge inspection, label update, or path-prefix decision",
        ),
        AlgorithmFamily::Continuous => (
            "one barrier, electrical, centering, or recovery phase",
            "one committed linear-system, cycle, progress, or recovery update",
            "one numerical residual observation, iterative-solver update, or progress candidate",
        ),
        AlgorithmFamily::AdvancedDiscrete => (
            "one hierarchy, shortcut, contraction, or recovery phase",
            "one weighted-flow, relabel, contraction, expansion, or repair update",
            "one hierarchy, shortcut, tree, or graph-oracle inspection",
        ),
        AlgorithmFamily::Dynamic => (
            "one update, repair, or recertification phase",
            "one graph update or local repair mutation",
            "one changed-edge inspection, repair candidate, or recertification check",
        ),
        AlgorithmFamily::CycleCanceling => (
            "one negative-cycle search phase",
            "one negative-cycle cancellation",
            "one residual-edge relaxation, cycle-edge inspection, or bottleneck calculation",
        ),
        AlgorithmFamily::ShortestPath => (
            "one boundary that starts or completes shortest-path or potential maintenance",
            "one potential update or flow augmentation",
            "one residual-edge relaxation, predecessor choice, or path-prefix extension",
        ),
        AlgorithmFamily::PrimalDual => (
            "one admissible-network or dual-price phase",
            "one admissible augmentation or dual-price update",
            "one reduced-cost inspection, admissible-edge choice, or dual-slack calculation",
        ),
        AlgorithmFamily::Scaling => (
            "one fixed capacity, excess, cost, or epsilon scale",
            "one admissible push, augment, relabel, price refinement, or scale transition",
            "one eligible-arc inspection, admissible advance, push, or price-relaxation primitive",
        ),
        AlgorithmFamily::Simplex => (
            "one pricing or basis-maintenance phase",
            "one completed fundamental-cycle pivot or bound flip",
            "one entering-arc price comparison, fundamental-cycle edge, or ratio-test candidate",
        ),
        AlgorithmFamily::Transportation => (
            "one transportation pricing or feasibility phase",
            "one route augmentation, MODI price update, or basis pivot",
            "one transportation-cell price comparison, cycle edge, or ratio-test candidate",
        ),
        AlgorithmFamily::Relaxation => (
            "one price-relaxation sweep or epsilon phase",
            "one local price change, flow adjustment, or feasibility restoration",
            "one incident-arc scan, admissibility decision, local push, or price-rise primitive",
        ),
        AlgorithmFamily::Assignment => (
            "one equality-graph, bidding, or price-update phase",
            "one alternating augmentation, award, or dual-label update",
            "one equality-edge inspection, slack comparison, bid candidate, or path extension",
        ),
        AlgorithmFamily::StronglyPolynomial => (
            "one contraction, scaling, or variable-fixing phase",
            "one contraction, variable-fixing, or certified progress update",
            "one matrix-column, variable, cut, or contraction-candidate inspection",
        ),
        AlgorithmFamily::Convex => (
            "one marginal-cost scale, expansion, or convex-basis phase",
            "one segment augmentation, marginal push, or convex simplex pivot",
            "one marginal-segment inspection, price comparison, cycle edge, or ratio-test candidate",
        ),
        AlgorithmFamily::Prediction => (
            "one prediction preprocessing, robust attempt, or epsilon phase",
            "one price preprocessing, relaxation, robust restart, or certification operation",
            "one predicted-price inspection, admissible-arc choice, local relaxation, or robustness check",
        ),
    }
}

const fn algorithm_detail_step(
    id: AlgorithmId,
    family_detail_unit: &'static str,
) -> AlgorithmDetailStepV1 {
    AlgorithmDetailStepV1::Available {
        unit: algorithm_detail_step_unit(id, family_detail_unit),
    }
}

const fn algorithm_detail_step_unit(
    id: AlgorithmId,
    family_detail_unit: &'static str,
) -> &'static str {
    match id {
        AlgorithmId::FordFulkerson
        | AlgorithmId::DfsFordFulkerson
        | AlgorithmId::WidestAugmentingPath
        | AlgorithmId::CapacityScalingAugmentingPath => {
            "one selected residual-path prefix extension before the completed search"
        }
        AlgorithmId::EdmondsKarp => {
            "one residual-arc inspection, augmenting-path prefix extension, or bottleneck computation"
        }
        AlgorithmId::ShortestAugmentingPath | AlgorithmId::Isap => {
            "one admissible current-arc advance that extends the active path"
        }
        AlgorithmId::Dinic | AlgorithmId::UnitCapacityDinic | AlgorithmId::UnitNetworkDinic => {
            "one admissible level-path prefix extension before augmentation"
        }
        AlgorithmId::GenericPushRelabel
        | AlgorithmId::FifoPushRelabel
        | AlgorithmId::RelabelToFront
        | AlgorithmId::HighestLabelPushRelabel
        | AlgorithmId::ExcessScalingPushRelabel
        | AlgorithmId::PartialAugmentRelabelMaxFlow
        | AlgorithmId::CurrentArcHeuristic
        | AlgorithmId::GlobalRelabelHeuristic
        | AlgorithmId::GapRelabelHeuristic => {
            "one local push, relabel, admissible-path advance, or retreat inside the scheduled operation"
        }
        AlgorithmId::KarzanovPreflow | AlgorithmId::Mpm => {
            "one admissible preflow propagation at the current level"
        }
        AlgorithmId::DynamicTreePushRelabel => {
            "one published admissible-edge inspection before the dynamic-tree operation"
        }
        AlgorithmId::GoldbergRao => {
            "one positive residual arc inspected while building binary lengths and reverse 0–1 distances"
        }
        AlgorithmId::BinaryBlockingFlow => {
            "one positive residual arc inspected for its binary length before zero-SCC contraction"
        }
        AlgorithmId::ParametricPseudoflow => {
            "one cooperative forward/reverse retained-forest race at an exact parameter"
        }
        AlgorithmId::ParametricBreakpointRerun => {
            "one exact affine cut-function intersection before a cold static solve"
        }
        AlgorithmId::HopcroftKarp => {
            "one residual-arc extension of a layered alternating path before matching augmentation"
        }
        AlgorithmId::HassinStPlanar => "one settled dual face in the dual shortest-path search",
        AlgorithmId::HochbaumPseudoflow => {
            "one pseudoflow normalization push or one rooted-forest split"
        }
        _ => minimum_cost_detail_step_unit(id, family_detail_unit),
    }
}

const fn minimum_cost_detail_step_unit(
    id: AlgorithmId,
    family_detail_unit: &'static str,
) -> &'static str {
    match id {
        AlgorithmId::SimpleCycleCanceling => {
            "one complete Bellman–Ford relaxation pass with updated labels and frontier"
        }
        AlgorithmId::MinimumMeanCycleCanceling => {
            "one positive residual arc inspected before SCC and Karp minimum-mean selection"
        }
        AlgorithmId::Hungarian => {
            "one assignment-cell inspection or minimum-slack task selection before the dual update"
        }
        AlgorithmId::Auction => {
            "one allowed assignment-edge inspection or bid calculation before the award"
        }
        AlgorithmId::SuccessiveShortestPath | AlgorithmId::BellmanFordSsp => {
            "one successful residual-arc relaxation, shortest-path prefix extension, or bottleneck computation"
        }
        AlgorithmId::SuccessiveShortestAugmentingPath => {
            "one heap settlement, residual-arc relaxation, shortest-path prefix extension, or bottleneck inspection"
        }
        AlgorithmId::Relaxation => "one balanced-frontier scan and ascent-slope observation",
        AlgorithmId::EpsilonRelaxation => {
            "one incident-arc scan that selects the next price breakpoint"
        }
        AlgorithmId::CostScaling
        | AlgorithmId::CostScalingPushRelabel
        | AlgorithmId::AugmentRelabel
        | AlgorithmId::PartialAugmentRelabelMcf
        | AlgorithmId::PriceRefinement
        | AlgorithmId::ArcFixing
        | AlgorithmId::GeneralizedCostScaling => {
            "one admissible advance, push, or price-relaxation primitive"
        }
        AlgorithmId::PrimalNetworkSimplex => {
            "one fundamental-cycle construction before a completed pivot"
        }
        AlgorithmId::DynamicTreeNetworkSimplex => {
            "one directional path-minimum query before a completed pivot"
        }
        AlgorithmId::ConvexCostScaling => {
            "one shortest marginal-residual path selection before potential and flow updates"
        }
        AlgorithmId::ConvexNetworkSimplex => {
            "one bidirectional reduced-cost pricing result before the convex pivot"
        }
        AlgorithmId::SegmentExpandedConvexMcf => {
            "one expanded marginal residual arc inspected before minimum-mean-cycle selection"
        }
        AlgorithmId::TardosFramework => {
            "one positive residual direction priced or one strict proximity witness inspected"
        }
        _ => family_detail_unit,
    }
}

const fn algorithm_phase_step_availability(id: AlgorithmId) -> AlgorithmStepAvailabilityV1 {
    match id {
        AlgorithmId::FordFulkerson
        | AlgorithmId::DfsFordFulkerson
        | AlgorithmId::Isap
        | AlgorithmId::WidestAugmentingPath
        | AlgorithmId::CapacityScalingAugmentingPath
        | AlgorithmId::Dinic
        | AlgorithmId::BinaryBlockingFlow
        | AlgorithmId::HopcroftKarp
        | AlgorithmId::BorradaileKleinPlanar
        | AlgorithmId::Auction => AlgorithmStepAvailabilityV1::Unavailable {
            reason: "Primary work owns the former phase event as a Detail boundary, so this trace has no independent Phase boundary.",
        },
        _ => AlgorithmStepAvailabilityV1::Available,
    }
}

const fn algorithm_operation_step_availability(id: AlgorithmId) -> AlgorithmStepAvailabilityV1 {
    match id {
        AlgorithmId::WarmStartPushRelabel => AlgorithmStepAvailabilityV1::Unavailable {
            reason: "This trace publishes Phase boundaries and source-work Details without a separate Operation boundary.",
        },
        _ => AlgorithmStepAvailabilityV1::Available,
    }
}

const fn algorithm_phase_step_unit(
    id: AlgorithmId,
    family_phase_unit: &'static str,
) -> &'static str {
    match id {
        AlgorithmId::BinaryBlockingFlow => {
            "one binary-length analysis and zero-SCC contraction phase"
        }
        AlgorithmId::SimpleCycleCanceling => {
            "one complete Bellman–Ford residual negative-cycle search"
        }
        AlgorithmId::MinimumMeanCycleCanceling => {
            "one SCC decomposition and Karp minimum-mean-cycle search"
        }
        AlgorithmId::ParametricPseudoflow => {
            "one retained-forest initialization or recursive contraction split"
        }
        AlgorithmId::ParametricBreakpointRerun => {
            "one endpoint initialization or exact breakpoint subproblem"
        }
        AlgorithmId::SegmentExpandedConvexMcf => {
            "one expanded marginal-residual minimum-mean-cycle search"
        }
        _ => family_phase_unit,
    }
}

const fn primary_work(
    metric_ordinal: u8,
    unit: &'static str,
    visualization: AlgorithmWorkVisualizationV1,
) -> AlgorithmPrimaryWorkV1 {
    AlgorithmPrimaryWorkV1 {
        metric_ordinal,
        unit,
        abstraction: AlgorithmWorkAbstractionV1::Primitive,
        visualization,
    }
}

/// Returns the endpoint-owned counter used to compare trace density with work.
///
/// Combinatorial kernels publish the deepest exact counter already measured by
/// their implementation: an arc inspection, candidate enumeration, matrix
/// product, pivot, or oracle state transition. A phase, augmentation, or oracle
/// call is not sufficient when the implementation records work inside it.
/// Numerical methods may use one solver iteration only when that is the
/// deepest reversible work unit they expose. This match is intentionally
/// exhaustive: adding an endpoint without choosing its counter must fail
/// compilation.
#[allow(
    clippy::too_many_lines,
    reason = "one exhaustive table keeps all 93 endpoint counters compile-time closed"
)]
const fn algorithm_primary_work(id: AlgorithmId) -> AlgorithmPrimaryWorkV1 {
    use AlgorithmWorkVisualizationV1::{CandidateField, EdgeField, NumericField};

    match id {
        AlgorithmId::ElectricalFlow => {
            primary_work(3, "grounded matrix scalar products", NumericField)
        }
        AlgorithmId::AugmentingElectricalFlow => {
            primary_work(2, "electrical elimination pivots", NumericField)
        }
        AlgorithmId::InteriorPointMaxFlow => {
            primary_work(6, "electrical elimination pivots", NumericField)
        }
        AlgorithmId::MinimumRatioCycleMaxFlow => {
            primary_work(2, "ternary candidate-vector evaluations", CandidateField)
        }
        AlgorithmId::OrlinMaxFlow => {
            primary_work(14, "residual and logical arc inspections", EdgeField)
        }
        AlgorithmId::WarmStartPushRelabel => primary_work(
            2,
            "auxiliary and recovery residual-arc inspections",
            EdgeField,
        ),
        AlgorithmId::RandomizedAlmostLinearMaxFlow => {
            primary_work(4, "fundamental-cycle evaluations", CandidateField)
        }
        AlgorithmId::DeterministicAlmostLinearMaxFlow => {
            primary_work(6, "fundamental-cycle evaluations", CandidateField)
        }
        AlgorithmId::WeightedAugmentingPaths => {
            primary_work(12, "weighted relabel arc inspections", EdgeField)
        }
        AlgorithmId::WeightedPushRelabel => {
            primary_work(3, "weighted-kernel arc inspections", EdgeField)
        }
        AlgorithmId::SimpleCycleCanceling | AlgorithmId::MinimumMeanCycleCanceling => {
            primary_work(2, "residual-arc inspections", EdgeField)
        }
        AlgorithmId::SegmentExpandedConvexMcf => {
            primary_work(2, "expanded marginal residual-arc inspections", EdgeField)
        }
        AlgorithmId::RelaxedMostNegativeCycle => {
            primary_work(2, "assignment-cell inspections", EdgeField)
        }
        AlgorithmId::EnhancedCapacityScaling => {
            primary_work(2, "original residual-arc inspections", EdgeField)
        }
        AlgorithmId::DoubleScaling => {
            primary_work(2, "transformed residual-arc inspections", EdgeField)
        }
        AlgorithmId::PrimalNetworkSimplex
        | AlgorithmId::DynamicTreeNetworkSimplex
        | AlgorithmId::TransportationSimplex
        | AlgorithmId::Modi => primary_work(2, "pricing-arc inspections", EdgeField),
        AlgorithmId::DualNetworkSimplex => {
            primary_work(3, "initial-tree and pricing arc inspections", EdgeField)
        }
        AlgorithmId::PolynomialPrimalNetworkSimplex => primary_work(
            3,
            "residual and fundamental-cycle arc inspections",
            EdgeField,
        ),
        AlgorithmId::PolynomialDualNetworkSimplex => primary_work(
            5,
            "initial-tree, augmentation-path, and pricing arc inspections",
            EdgeField,
        ),
        AlgorithmId::OrlinMcf => primary_work(10, "compressed residual-arc inspections", EdgeField),
        AlgorithmId::PrimalDualInteriorPointMcf => {
            primary_work(2, "candidate forest-subset evaluations", CandidateField)
        }
        AlgorithmId::ElectricalFlowInteriorPointMcf => {
            primary_work(7, "Newton elimination pivots", NumericField)
        }
        AlgorithmId::MinimumRatioCycleMcf => {
            primary_work(4, "ternary cycle-vector evaluations", CandidateField)
        }
        AlgorithmId::RandomizedAlmostLinearMcf => {
            primary_work(12, "minimum-ratio cycle-vector evaluations", CandidateField)
        }
        AlgorithmId::DeterministicAlmostLinearMcf => {
            primary_work(6, "dynamic oracle edge inspections", EdgeField)
        }
        AlgorithmId::ConvexCostScaling => {
            primary_work(2, "marginal residual-arc inspections", EdgeField)
        }
        AlgorithmId::ConvexNetworkSimplex => {
            primary_work(2, "bidirectional pricing scans", EdgeField)
        }
        AlgorithmId::PredictionAssistedEpsilonRelaxation => {
            primary_work(2, "positive residual-arc inspections", EdgeField)
        }
        AlgorithmId::ParametricPseudoflow => {
            primary_work(15, "normalized-forest residual-arc inspections", EdgeField)
        }
        AlgorithmId::ParametricBreakpointRerun => {
            primary_work(2, "cold-solver residual-arc inspections", EdgeField)
        }
        AlgorithmId::HopcroftKarp => primary_work(2, "matching-edge inspections", EdgeField),
        AlgorithmId::Hungarian => primary_work(2, "assignment-cell inspections", EdgeField),
        AlgorithmId::Auction => primary_work(2, "assignment-edge inspections", EdgeField),
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
        | AlgorithmId::HochbaumPseudoflow
        | AlgorithmId::PseudoflowSimplex
        | AlgorithmId::BoykovKolmogorov
        | AlgorithmId::Ibfs
        | AlgorithmId::Eibfs
        | AlgorithmId::HassinStPlanar
        | AlgorithmId::BorradaileKleinPlanar
        | AlgorithmId::DynamicEibfs
        | AlgorithmId::GoldbergRao
        | AlgorithmId::UnitCapacityDinic
        | AlgorithmId::UnitNetworkDinic
        | AlgorithmId::BinaryBlockingFlow
        | AlgorithmId::CancelAndTighten
        | AlgorithmId::SuccessiveShortestPath
        | AlgorithmId::BellmanFordSsp
        | AlgorithmId::PotentialDijkstraSsp
        | AlgorithmId::SuccessiveShortestAugmentingPath
        | AlgorithmId::PrimalDualMcf
        | AlgorithmId::BlockingFlowPrimalDual
        | AlgorithmId::CapacityScalingMcf
        | AlgorithmId::CostScaling
        | AlgorithmId::CostScalingPushRelabel
        | AlgorithmId::AugmentRelabel
        | AlgorithmId::PartialAugmentRelabelMcf
        | AlgorithmId::PriceRefinement
        | AlgorithmId::ArcFixing
        | AlgorithmId::ExcessScalingMcf
        | AlgorithmId::GeneralizedCostScaling
        | AlgorithmId::OutOfKilter
        | AlgorithmId::Relaxation
        | AlgorithmId::EpsilonRelaxation
        | AlgorithmId::TardosFramework => primary_work(2, "residual-arc inspections", EdgeField),
    }
}

/// Returns the playback boundary contract shared by the catalog and scene.
///
/// Phase and operation units are family semantics. Detailed availability is a
/// closed endpoint capability: every catalog endpoint must emit the declared
/// source-defined primitive boundary before it can be added to this table.
#[must_use]
pub const fn algorithm_step_contract(
    id: AlgorithmId,
    family: AlgorithmFamily,
) -> AlgorithmStepContractV1 {
    let (family_phase_unit, operation_unit, family_detail_unit) =
        algorithm_family_step_units(family);

    AlgorithmStepContractV1 {
        phase_unit: algorithm_phase_step_unit(id, family_phase_unit),
        phase_availability: algorithm_phase_step_availability(id),
        operation_unit,
        operation_availability: algorithm_operation_step_availability(id),
        detail: algorithm_detail_step(id, family_detail_unit),
        primary_work: algorithm_primary_work(id),
    }
}

/// Returns the complete finite catalog, including planned and source-blocked entries.
#[must_use]
pub const fn algorithm_catalog() -> &'static [AlgorithmDescriptor] {
    ALGORITHM_CATALOG
}

/// Iterates only over production-ready entries.
pub fn executable_algorithms() -> impl Iterator<Item = &'static AlgorithmDescriptor> {
    ALGORITHM_CATALOG
        .iter()
        .filter(|descriptor| descriptor.status == ImplementationStatus::Executable)
}

/// Resolves a typed canonical ID without passing through aliases or display names.
#[must_use]
pub fn find_algorithm_by_id(id: AlgorithmId) -> Option<&'static AlgorithmDescriptor> {
    ALGORITHM_CATALOG
        .iter()
        .find(|descriptor| descriptor.algorithm_id == id)
}

/// Resolves a canonical ID, display title, or retained identity alias.
///
/// Discovery-only [`AlgorithmDescriptor::search_terms`] are deliberately not
/// accepted here. Machine Scenario IDs are parsed directly as [`AlgorithmId`].
#[must_use]
pub fn find_algorithm(query: &str) -> Option<&'static AlgorithmDescriptor> {
    let query = query.trim();
    if let Ok(id) = AlgorithmId::from_str(query) {
        return find_algorithm_by_id(id);
    }
    ALGORITHM_CATALOG.iter().find(|descriptor| {
        descriptor.id.eq_ignore_ascii_case(query)
            || descriptor.title.eq_ignore_ascii_case(query)
            || descriptor
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(query))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn normalized(value: &str) -> String {
        value.trim().to_lowercase()
    }

    #[test]
    fn canonical_ids_aliases_and_search_terms_are_unique() {
        let mut owners = BTreeMap::<String, &str>::new();
        for descriptor in algorithm_catalog() {
            for name in std::iter::once(descriptor.id)
                .chain(std::iter::once(descriptor.title))
                .chain(descriptor.aliases.iter().copied())
                .chain(descriptor.search_terms.iter().copied())
            {
                let key = normalized(name);
                if let Some(previous) = owners.insert(key, descriptor.id) {
                    assert_eq!(
                        previous, descriptor.id,
                        "catalog name {name:?} belongs to multiple descriptors"
                    );
                }
            }
        }
    }

    #[test]
    fn admission_contracts_are_derived_from_solver_owned_limits() {
        assert_eq!(
            find_algorithm("edmonds-karp")
                .expect("general descriptor exists")
                .admission_contract,
            GENERAL_ADMISSION_CONTRACT
        );
        assert_eq!(
            find_algorithm("electrical-flow")
                .expect("bounded descriptor exists")
                .admission_contract
                .max_capacity,
            Some(AdmissionLimitU64(crate::ELECTRICAL_FLOW_MAX_CAPACITY))
        );
        assert_eq!(
            find_algorithm("deterministic-almost-linear-mcf")
                .expect("bounded descriptor exists")
                .admission_contract
                .max_assignment_space,
            Some(AdmissionLimitU64(crate::FLOW_FRAMEWORK_MCF_MAX_ASSIGNMENTS))
        );
        for (id, maximum_nodes, maximum_edges) in [
            (
                "minimum-ratio-cycle-mcf",
                crate::MINIMUM_RATIO_CYCLE_MCF_MAX_NODES,
                crate::MINIMUM_RATIO_CYCLE_MCF_MAX_EDGES,
            ),
            (
                "randomized-almost-linear-mcf-oracle-demonstrator",
                crate::RANDOMIZED_ALMOST_LINEAR_MCF_MAX_NODES,
                crate::RANDOMIZED_ALMOST_LINEAR_MCF_MAX_EDGES,
            ),
            (
                "electrical-flow-interior-point-mcf",
                crate::ELECTRICAL_IPM_MCF_MAX_NODES,
                crate::ELECTRICAL_IPM_MCF_MAX_EDGES,
            ),
            (
                "primal-dual-interior-point-mcf",
                crate::PRIMAL_DUAL_IPM_MCF_MAX_NODES,
                crate::PRIMAL_DUAL_IPM_MCF_MAX_EDGES,
            ),
        ] {
            let contract = find_algorithm(id)
                .expect("bounded descriptor exists")
                .admission_contract;
            assert_eq!(
                contract
                    .max_nodes
                    .map(|value| usize::try_from(value).expect("u32 fits usize")),
                Some(maximum_nodes)
            );
            assert_eq!(
                contract
                    .max_edges
                    .map(|value| usize::try_from(value).expect("u32 fits usize")),
                Some(maximum_edges)
            );
        }
        let dynamic = find_algorithm("dynamic-eibfs")
            .expect("dynamic descriptor exists")
            .admission_contract;
        assert_eq!(dynamic.min_dynamic_capacity_updates, Some(1));
        assert_eq!(dynamic.max_dynamic_capacity_updates, Some(256));
        assert!(dynamic.capacity_updates_only);

        let serialized = serde_json::to_value(
            find_algorithm("electrical-flow").expect("bounded descriptor exists"),
        )
        .expect("descriptor serializes");
        assert_eq!(
            serialized["admission_contract"]["max_capacity"],
            crate::ELECTRICAL_FLOW_MAX_CAPACITY.to_string()
        );
    }

    #[test]
    fn typed_ids_round_trip_and_match_catalog_order() {
        assert_eq!(AlgorithmId::ALL.len(), ALGORITHM_CATALOG.len());
        for (id, descriptor) in AlgorithmId::ALL.iter().zip(ALGORITHM_CATALOG) {
            assert_eq!(*id, descriptor.algorithm_id);
            assert_eq!(id.as_str(), descriptor.id);
            assert_eq!(descriptor.id.parse::<AlgorithmId>(), Ok(*id));
            assert_eq!(find_algorithm_by_id(*id), Some(descriptor));
        }
        assert!("DINIC".parse::<AlgorithmId>().is_err());
        assert_eq!(find_algorithm("DINIC").map(|entry| entry.id), Some("dinic"));
    }

    #[test]
    fn executable_detail_witnesses_do_not_stop_at_opaque_oracle_calls() {
        for descriptor in executable_algorithms() {
            assert_ne!(
                descriptor.trace_steps.primary_work.abstraction,
                AlgorithmWorkAbstractionV1::OracleCall,
                "{} must count work inside its implemented oracle",
                descriptor.id,
            );
        }
    }

    #[test]
    fn every_endpoint_declares_a_detailed_step_unit() {
        let mut available = algorithm_catalog()
            .iter()
            .filter_map(|descriptor| {
                assert!(!descriptor.trace_steps.phase_unit.trim().is_empty());
                assert!(!descriptor.trace_steps.operation_unit.trim().is_empty());
                assert!(descriptor.trace_steps.primary_work.metric_ordinal < 16);
                assert!(!descriptor.trace_steps.primary_work.unit.trim().is_empty());
                match descriptor.trace_steps.detail {
                    AlgorithmDetailStepV1::Available { unit } => {
                        assert!(!unit.trim().is_empty());
                        Some(descriptor.id)
                    }
                    AlgorithmDetailStepV1::Unavailable { reason } => {
                        assert!(!reason.trim().is_empty());
                        None
                    }
                }
            })
            .collect::<Vec<_>>();
        available.sort_unstable();

        let mut expected = AlgorithmId::ALL
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>();
        expected.sort_unstable();
        assert_eq!(available, expected);
        assert_eq!(algorithm_catalog().len(), 93);
    }

    #[test]
    fn phase_and_operation_capabilities_are_closed_over_all_endpoints() {
        let mut phase_unavailable = Vec::new();
        let mut operation_unavailable = Vec::new();
        for descriptor in algorithm_catalog() {
            match descriptor.trace_steps.phase_availability {
                AlgorithmStepAvailabilityV1::Available => {}
                AlgorithmStepAvailabilityV1::Unavailable { reason } => {
                    assert!(!reason.trim().is_empty());
                    phase_unavailable.push(descriptor.id);
                }
            }
            match descriptor.trace_steps.operation_availability {
                AlgorithmStepAvailabilityV1::Available => {}
                AlgorithmStepAvailabilityV1::Unavailable { reason } => {
                    assert!(!reason.trim().is_empty());
                    operation_unavailable.push(descriptor.id);
                }
            }
        }
        assert_eq!(
            phase_unavailable
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "auction",
                "binary-blocking-flow",
                "borradaile-klein-planar",
                "capacity-scaling-augmenting-path",
                "dfs-ford-fulkerson",
                "dinic",
                "ford-fulkerson",
                "hopcroft-karp",
                "isap",
                "widest-augmenting-path",
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(
            operation_unavailable
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>(),
            ["warm-start-push-relabel"].into_iter().collect()
        );
    }

    #[test]
    fn runtime_routes_match_the_declared_public_models() {
        for descriptor in ALGORITHM_CATALOG {
            let expected = match descriptor.runtime_route {
                RuntimeRouteKind::MaxFlow => CatalogModelKind::MaxFlow,
                RuntimeRouteKind::MinCostFlow => descriptor.models[0],
                RuntimeRouteKind::MinCostMaxFlow => CatalogModelKind::MinCostMaxFlow,
                RuntimeRouteKind::ParametricMaxFlow => CatalogModelKind::ParametricMaxFlow,
                RuntimeRouteKind::BipartiteMatching => CatalogModelKind::BipartiteMatching,
                RuntimeRouteKind::Assignment => CatalogModelKind::Assignment,
                RuntimeRouteKind::Transportation => CatalogModelKind::Transportation,
                RuntimeRouteKind::PlanarMaxFlow => CatalogModelKind::PlanarMaxFlow,
                RuntimeRouteKind::ConvexCostFlow => CatalogModelKind::ConvexCostFlow,
            };
            assert!(
                descriptor.models.contains(&expected),
                "{} routes to {:?} without the matching public model",
                descriptor.id,
                descriptor.runtime_route
            );
            if descriptor.runtime_route == RuntimeRouteKind::MinCostFlow {
                assert!(descriptor.models.iter().all(|model| matches!(
                    model,
                    CatalogModelKind::FixedFlowMinCost
                        | CatalogModelKind::Circulation
                        | CatalogModelKind::Transshipment
                )));
            }
        }
    }

    #[test]
    fn every_entry_has_a_complexity_and_source_claim() {
        for descriptor in algorithm_catalog() {
            assert!(!descriptor.source_id.is_empty(), "{}", descriptor.id);
            assert!(!descriptor.complexity.is_empty(), "{}", descriptor.id);
            assert!(!descriptor.problems.is_empty(), "{}", descriptor.id);
        }
    }

    #[test]
    fn catalog_inventory_changes_are_explicit() {
        let mut executable = 0;
        let mut planned = 0;
        let mut source_blocked = 0;
        for descriptor in algorithm_catalog() {
            match descriptor.status {
                ImplementationStatus::Executable => executable += 1,
                ImplementationStatus::Planned => planned += 1,
                ImplementationStatus::SourceBlocked => source_blocked += 1,
            }
        }
        assert_eq!(algorithm_catalog().len(), 93);
        assert_eq!((executable, planned, source_blocked), (93, 0, 0));
    }

    #[test]
    fn noncanonical_completion_modes_and_source_components_are_explicit_and_closed() {
        let actual = algorithm_catalog()
            .iter()
            .filter(|descriptor| {
                matches!(
                    descriptor.implementation_scope,
                    ImplementationScope::ExternalCompletion
                        | ImplementationScope::PrecomputedOptimumProjection
                )
            })
            .map(|descriptor| descriptor.id)
            .collect::<Vec<_>>();
        assert!(actual.is_empty());
        let demonstrator = algorithm_catalog()
            .iter()
            .find(|descriptor| descriptor.id == "randomized-almost-linear-mcf-oracle-demonstrator")
            .expect("randomized MCF demonstrator");
        assert_eq!(demonstrator.kind, CatalogKind::Primitive);
        assert_eq!(
            demonstrator.initial_construction,
            InitialConstruction::ProjectOracleConstructed
        );
        assert_eq!(
            demonstrator.initial_oracle_dependency,
            InitialOracleDependency::ProjectOptimumVectorInitialState
        );
        assert_eq!(
            demonstrator.terminal_oracle_dependency,
            TerminalOracleDependency::ProjectOptimumVectorFinalPoint
        );
        assert_eq!(
            demonstrator.implementation_scope,
            ImplementationScope::ProjectOracleDemonstrator
        );
    }

    #[test]
    fn initial_project_oracle_roles_are_closed_and_exhaustive() {
        let classified = algorithm_catalog()
            .iter()
            .filter_map(|descriptor| {
                (descriptor.initial_oracle_dependency != InitialOracleDependency::None)
                    .then_some((descriptor.id, descriptor.initial_oracle_dependency))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            classified,
            vec![
                (
                    "augmenting-electrical-flow",
                    InitialOracleDependency::ProjectExactMaxFlowScalarTarget,
                ),
                (
                    "interior-point-max-flow",
                    InitialOracleDependency::ProjectExactMaxFlowScalarTarget,
                ),
                (
                    "randomized-almost-linear-max-flow-oracle-demonstrator",
                    InitialOracleDependency::ProjectExactMaxFlowScalarTarget,
                ),
                (
                    "deterministic-almost-linear-max-flow-oracle-demonstrator",
                    InitialOracleDependency::ProjectExactMaxFlowScalarTarget,
                ),
                (
                    "electrical-flow-interior-point-mcf",
                    InitialOracleDependency::ProjectIsolationFaceOptimumFacts,
                ),
                (
                    "minimum-ratio-cycle-mcf",
                    InitialOracleDependency::ProjectFeasibleFaceInitialStateAndScalarOptimum,
                ),
                (
                    "randomized-almost-linear-mcf-oracle-demonstrator",
                    InitialOracleDependency::ProjectOptimumVectorInitialState,
                ),
                (
                    "deterministic-almost-linear-mcf",
                    InitialOracleDependency::ProjectExactMinCostScalarOptimum,
                ),
            ]
        );
    }

    #[test]
    fn source_status_agrees_with_the_checked_in_registry() {
        let registry = include_str!("../../../docs/flow-sources.md");
        let (before_blocked, blocked) = registry
            .split_once("## Source-blocked records")
            .expect("source registry has an explicit blocked section");
        let (confirmed_prefix, _) = before_blocked
            .split_once("## Authoritative cross-checks")
            .expect("source registry separates confirmed records from cross-checks");
        let (_, confirmed) = confirmed_prefix
            .split_once("## Confirmed records")
            .expect("source registry has an explicit confirmed section");

        for descriptor in algorithm_catalog() {
            let source_marker = format!("`{}`", descriptor.source_id);
            match descriptor.status {
                ImplementationStatus::Planned | ImplementationStatus::Executable => assert!(
                    confirmed.contains(&source_marker),
                    "{} points to a source that is not a confirmed primary/project record: {}",
                    descriptor.id,
                    descriptor.source_id
                ),
                ImplementationStatus::SourceBlocked => {
                    let descriptor_marker = format!("`{}`", descriptor.id);
                    assert!(
                        blocked.contains(&descriptor_marker) && blocked.contains(&source_marker),
                        "{} must name its provisional source in the blocked registry",
                        descriptor.id
                    );
                }
            }
        }
    }

    #[test]
    fn public_models_match_the_runtime_selection_boundary() {
        assert!(!MCF_MODELS.contains(&CatalogModelKind::MinCostMaxFlow));
        assert_eq!(MCMF_MODELS, &[CatalogModelKind::MinCostMaxFlow]);
        assert_eq!(WARM_START_MODELS, &[CatalogModelKind::MaxFlow]);

        let warm_start = find_algorithm("warm-start-push-relabel").expect("warm-start entry");
        assert_eq!(warm_start.models, &[CatalogModelKind::MaxFlow]);
        let ssap = find_algorithm("successive-shortest-augmenting-path").expect("SSAP entry");
        assert_eq!(ssap.models, &[CatalogModelKind::MinCostMaxFlow]);
        for descriptor in algorithm_catalog() {
            if descriptor.id != "successive-shortest-augmenting-path" {
                assert!(
                    !descriptor
                        .models
                        .contains(&CatalogModelKind::MinCostMaxFlow),
                    "{} advertises a min-cost-max-flow runtime path it does not own",
                    descriptor.id
                );
            }
        }
    }

    #[test]
    fn corrected_and_legacy_names_resolve_without_mislabeling() {
        assert_eq!(
            find_algorithm("Enhanced IBFS").map(|descriptor| descriptor.title),
            Some("Excesses IBFS")
        );
        assert_eq!(
            find_algorithm("平面グラフ双対最短路法").map(|descriptor| descriptor.id),
            Some("hassin-st-planar")
        );
        assert_eq!(
            find_algorithm("Parallel/GPU Push–Relabel").map(|descriptor| descriptor.title),
            Some("Synchronous Parallel Push–Relabel (CPU)")
        );
    }

    fn assert_referenced_names_resolve(names: &[&str]) {
        for name in names {
            assert!(
                find_algorithm(name).is_some(),
                "referenced conversation name {name:?} is not normalized"
            );
        }
    }

    #[test]
    fn normalized_maximum_flow_conversation_names_resolve() {
        assert_referenced_names_resolve(&[
            "Ford–Fulkerson",
            "DFS Ford–Fulkerson",
            "Edmonds–Karp",
            "Shortest Augmenting Path",
            "ISAP",
            "Widest/Fattest Augmenting Path",
            "Capacity-Scaling Augmenting Path",
            "Distance-Directed Augmenting Path",
            "Dinic/Dinitz",
            "Unit-Capacity Dinic",
            "Karzanov Preflow",
            "MPM",
            "Dynamic-Tree Blocking Flow",
            "Binary Blocking Flow",
            "Goldberg–Rao",
            "Generic Push–Relabel",
            "FIFO Push–Relabel",
            "Relabel-to-Front",
            "Highest-Label Push–Relabel",
            "Excess-Scaling Push–Relabel",
            "Dynamic-Tree Push–Relabel",
            "Partial Augment–Relabel Max Flow",
            "Pseudoflow",
            "Pseudoflow Simplex",
            "Parametric Pseudoflow",
            "Boykov–Kolmogorov",
            "IBFS",
            "EIBFS",
            "Hopcroft–Karp",
            "平面グラフ双対最短路法",
            "Electrical Flow",
            "Augmenting Electrical Flows",
            "Interior-Point Max Flow",
            "Minimum-Ratio-Cycle Framework",
            "Orlin Max Flow",
            "Weighted Augmenting Paths",
            "Weighted Push–Relabel",
            "Parallel/GPU Push–Relabel",
            "Dynamic Max Flow",
        ]);
    }

    #[test]
    fn normalized_minimum_cost_flow_conversation_names_resolve() {
        assert_referenced_names_resolve(&[
            "Simple Cycle Canceling",
            "Minimum-Mean Cycle Canceling",
            "Cancel-and-Tighten",
            "Relaxed Most-Negative-Cycle",
            "Successive Shortest Path",
            "Bellman–Ford SSP",
            "Potential + Dijkstra SSP",
            "Successive Shortest Augmenting Path",
            "Primal–Dual",
            "Blocking-Flow Primal–Dual",
            "Capacity Scaling MCF",
            "Enhanced Capacity Scaling MCF",
            "Cost Scaling",
            "Cost-Scaling Push–Relabel",
            "Augment–Relabel",
            "Partial Augment–Relabel MCF",
            "Price Refinement",
            "Arc Fixing",
            "Excess Scaling MCF",
            "Double Scaling",
            "Generalized Cost Scaling",
            "Primal Network Simplex",
            "Dual Network Simplex",
            "Polynomial Network Simplex",
            "Polynomial Dual Network Simplex",
            "Dynamic-Tree Network Simplex",
            "Transportation Simplex",
            "MODI",
            "Out-of-Kilter",
            "Relaxation",
            "ε-Relaxation",
            "Hungarian",
            "Auction",
            "Orlin Minimum-Cost Flow",
            "Primal-Dual Interior-Point",
            "Electrical Flow Interior-Point",
            "Minimum Ratio Cycle MCF",
            "Deterministic almost-linear minimum-cost flow",
            "Convex Cost Scaling",
            "Prediction-Assisted ε-Relaxation",
        ]);
    }

    #[test]
    fn parent_solver_discovery_terms_are_not_machine_aliases() {
        for (parent, related_endpoint) in [
            (
                "Randomized almost-linear max flow",
                AlgorithmId::RandomizedAlmostLinearMaxFlow,
            ),
            (
                "Deterministic almost-linear max flow",
                AlgorithmId::DeterministicAlmostLinearMaxFlow,
            ),
            (
                "Randomized almost-linear minimum-cost flow",
                AlgorithmId::RandomizedAlmostLinearMcf,
            ),
        ] {
            assert!(find_algorithm(parent).is_none());
            assert!(
                find_algorithm_by_id(related_endpoint)
                    .expect("related endpoint descriptor")
                    .search_terms
                    .contains(&parent)
            );
        }
    }

    #[test]
    fn replaced_parent_slugs_are_not_machine_ids_or_aliases() {
        for old_slug in [
            "randomized-almost-linear-max-flow",
            "deterministic-almost-linear-max-flow",
            "randomized-almost-linear-mcf",
        ] {
            assert!(old_slug.parse::<AlgorithmId>().is_err());
            assert!(find_algorithm(old_slug).is_none());
        }
    }

    #[test]
    fn prediction_assisted_descriptor_exposes_the_source_defined_bounded_kernel() {
        let descriptor = find_algorithm("prediction-assisted-epsilon-relaxation")
            .expect("prediction-assisted descriptor exists");
        assert_eq!(descriptor.kind, CatalogKind::Solver);
        assert_eq!(descriptor.family, AlgorithmFamily::Prediction);
        assert_eq!(descriptor.problems, MCF);
        assert_eq!(descriptor.models, MCF_MODELS);
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::SourceDefined
        );
        assert_eq!(descriptor.source_id, "chen-yao-yin-2026");
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert!(descriptor.search_terms.is_empty());
        assert_eq!(descriptor.initial_band, BAND_RESEARCH);
        assert!(descriptor.complexity.contains("Remark 1"));
    }

    #[test]
    fn tardos_descriptor_is_a_checked_primitive_not_an_orlin_alias() {
        let descriptor = find_algorithm("tardos-framework").expect("Tardos descriptor exists");
        assert_eq!(descriptor.kind, CatalogKind::Primitive);
        assert_eq!(descriptor.family, AlgorithmFamily::StronglyPolynomial);
        assert_eq!(descriptor.problems, MCF);
        assert_eq!(descriptor.models, MCF_MODELS);
        assert_eq!(descriptor.source_id, "tardos-1986");
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(
            descriptor.search_terms,
            &["Tardos Strongly Polynomial Algorithm"]
        );
        assert_eq!(descriptor.initial_band, BAND_RESEARCH);
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit"),
            crate::TARDOS_FRAMEWORK_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit"),
            crate::TARDOS_FRAMEWORK_MAX_EDGES
        );
        assert!(descriptor.complexity.contains("proximity/fixing"));
        assert!(find_algorithm("Tardos Strongly Polynomial Algorithm").is_none());
    }

    #[test]
    fn orlin_mcf_descriptor_matches_the_finite_capacity_kernel() {
        let descriptor = find_algorithm("orlin-mcf").expect("Orlin MCF descriptor exists");
        assert_eq!(descriptor.kind, CatalogKind::Solver);
        assert_eq!(descriptor.family, AlgorithmFamily::StronglyPolynomial);
        assert_eq!(descriptor.problems, MCF);
        assert_eq!(descriptor.models, MCF_MODELS);
        assert!(descriptor.graph_requirements.is_empty());
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::SourceDefined
        );
        assert_eq!(descriptor.source_id, "orlin-1993");
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(descriptor.initial_band, BAND_ORLIN_MCF);
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit"),
            crate::ORLIN_MCF_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit"),
            crate::ORLIN_MCF_MAX_EDGES
        );
        assert!(descriptor.complexity.contains("Section 5"));
    }

    #[test]
    fn orlin_max_flow_descriptor_matches_the_explicit_compact_kernel() {
        let descriptor = find_algorithm("orlin-max-flow").expect("Orlin max-flow descriptor");
        assert_eq!(descriptor.kind, CatalogKind::Solver);
        assert_eq!(descriptor.family, AlgorithmFamily::AdvancedDiscrete);
        assert_eq!(descriptor.problems, MAX);
        assert_eq!(descriptor.models, MAX_MODELS);
        assert_eq!(descriptor.source_id, "orlin-2013");
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(descriptor.initial_band, BAND_ORLIN_MAX_FLOW);
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit"),
            crate::ORLIN_MAX_FLOW_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit"),
            crate::ORLIN_MAX_FLOW_MAX_EDGES
        );
        assert!(descriptor.complexity.contains("does not claim"));
    }

    #[test]
    fn electrical_flow_descriptor_is_a_bounded_primitive_not_a_max_flow_solver() {
        let descriptor = find_algorithm("electrical-flow").expect("electrical descriptor");
        assert_eq!(descriptor.kind, CatalogKind::Primitive);
        assert_eq!(descriptor.family, AlgorithmFamily::Continuous);
        assert_eq!(descriptor.problems, MAX);
        assert_eq!(descriptor.models, MAX_MODELS);
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::SourceDefined
        );
        assert!(!descriptor.exact);
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(descriptor.initial_band, BAND_ELECTRICAL_FLOW);
        assert_eq!(descriptor.graph_requirements, ELECTRICAL_FLOW_REQUIREMENTS);
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit"),
            crate::ELECTRICAL_FLOW_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit"),
            crate::ELECTRICAL_FLOW_MAX_EDGES
        );
        assert!(descriptor.complexity.contains("does not claim"));
    }

    #[test]
    fn electrical_ipm_mcf_descriptor_discloses_bounded_source_recovery() {
        let descriptor = find_algorithm("electrical-flow-interior-point-mcf")
            .expect("electrical IPM MCF descriptor");
        assert_eq!(descriptor.kind, CatalogKind::Solver);
        assert_eq!(descriptor.family, AlgorithmFamily::Continuous);
        assert_eq!(descriptor.problems, MCF);
        assert_eq!(descriptor.models, MCF_MODELS);
        assert!(descriptor.exact);
        assert!(descriptor.randomized);
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(
            descriptor.implementation_scope,
            ImplementationScope::BoundedOracleGuided
        );
        assert_eq!(descriptor.source_id, "daitch-spielman-2008");
        assert!(descriptor.complexity.contains("central-estimate rounding"));
        assert!(descriptor.complexity.contains("do not claim"));
    }

    #[test]
    fn augmenting_electrical_descriptor_matches_the_bounded_exact_kernel() {
        let descriptor =
            find_algorithm("augmenting-electrical-flow").expect("augmenting descriptor");
        assert_eq!(descriptor.kind, CatalogKind::Solver);
        assert_eq!(descriptor.family, AlgorithmFamily::Continuous);
        assert_eq!(descriptor.problems, MAX);
        assert_eq!(descriptor.models, MAX_MODELS);
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::SourceDefined
        );
        assert!(descriptor.exact);
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(
            descriptor.implementation_scope,
            ImplementationScope::BoundedOracleGuided
        );
        assert_eq!(descriptor.initial_band, BAND_AUGMENTING_ELECTRICAL);
        assert_eq!(descriptor.initial_band.max_nodes, 5);
        assert_eq!(descriptor.initial_band.max_edges, 6);
        assert_eq!(
            descriptor.admission_contract.max_capacity,
            Some(AdmissionLimitU64(8))
        );
        assert_eq!(
            descriptor.graph_requirements,
            AUGMENTING_ELECTRICAL_REQUIREMENTS
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit"),
            crate::AUGMENTING_ELECTRICAL_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit"),
            crate::AUGMENTING_ELECTRICAL_MAX_EDGES
        );
        assert!(descriptor.complexity.contains("does not claim"));
    }

    #[test]
    fn interior_point_descriptor_matches_the_bounded_section_five_kernel() {
        let descriptor =
            find_algorithm("interior-point-max-flow").expect("interior-point descriptor");
        assert_eq!(descriptor.kind, CatalogKind::Solver);
        assert_eq!(descriptor.family, AlgorithmFamily::Continuous);
        assert_eq!(descriptor.problems, MAX);
        assert_eq!(descriptor.models, MAX_MODELS);
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::SourceDefined
        );
        assert!(descriptor.exact);
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(
            descriptor.implementation_scope,
            ImplementationScope::BoundedOracleGuided
        );
        assert_eq!(descriptor.initial_band, BAND_INTERIOR_POINT_MAX_FLOW);
        assert_eq!(
            descriptor.graph_requirements,
            UNIT_IPM_MAX_FLOW_REQUIREMENTS
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit"),
            crate::INTERIOR_POINT_MAX_FLOW_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit"),
            crate::INTERIOR_POINT_MAX_FLOW_MAX_EDGES
        );
        assert!(descriptor.complexity.contains("does not claim"));
    }

    #[test]
    fn minimum_ratio_cycle_descriptor_matches_the_exact_bounded_primitive() {
        let descriptor =
            find_algorithm("minimum-ratio-cycle-max-flow").expect("ratio-cycle descriptor");
        assert_eq!(descriptor.kind, CatalogKind::Primitive);
        assert_eq!(descriptor.family, AlgorithmFamily::Continuous);
        assert_eq!(descriptor.problems, MAX);
        assert_eq!(descriptor.models, MAX_MODELS);
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::SourceDefined
        );
        assert!(descriptor.exact);
        assert!(!descriptor.randomized);
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(
            descriptor.implementation_scope,
            ImplementationScope::SourceComponent
        );
        assert_eq!(descriptor.initial_band, BAND_MINIMUM_RATIO_CYCLE);
        assert_eq!(descriptor.graph_requirements, NO_GRAPH_REQUIREMENTS);
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit"),
            crate::MINIMUM_RATIO_CYCLE_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit"),
            crate::MINIMUM_RATIO_CYCLE_MAX_EDGES
        );
        assert!(descriptor.complexity.contains("does not claim"));
    }

    #[test]
    fn randomized_tree_chain_endpoint_is_an_oracle_final_point_primitive() {
        let descriptor = find_algorithm("randomized-almost-linear-max-flow-oracle-demonstrator")
            .expect("randomized almost-linear max-flow descriptor");
        assert_eq!(descriptor.kind, CatalogKind::Primitive);
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::SourceDefined
        );
        assert_eq!(
            descriptor.initial_oracle_dependency,
            InitialOracleDependency::ProjectExactMaxFlowScalarTarget
        );
        assert_eq!(
            descriptor.terminal_oracle_dependency,
            TerminalOracleDependency::ProjectOptimumVectorFinalPoint
        );
        assert_eq!(descriptor.family, AlgorithmFamily::Continuous);
        assert!(descriptor.exact);
        assert!(descriptor.randomized);
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(
            descriptor.implementation_scope,
            ImplementationScope::ProjectOracleDemonstrator
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit"),
            crate::RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit"),
            crate::RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_EDGES
        );
        assert!(descriptor.complexity.contains("oracle-constructed"));
        assert!(descriptor.complexity.contains("does not implement"));
        assert!(descriptor.complexity.contains("outer stopping loop"));
        assert_eq!(
            descriptor.search_terms,
            &["Randomized almost-linear max flow"]
        );
    }

    #[test]
    fn deterministic_tree_chain_endpoint_is_an_oracle_final_point_primitive() {
        let descriptor = find_algorithm("deterministic-almost-linear-max-flow-oracle-demonstrator")
            .expect("deterministic almost-linear max-flow descriptor");
        assert_eq!(descriptor.kind, CatalogKind::Primitive);
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::SourceDefined
        );
        assert_eq!(
            descriptor.initial_oracle_dependency,
            InitialOracleDependency::ProjectExactMaxFlowScalarTarget
        );
        assert_eq!(
            descriptor.terminal_oracle_dependency,
            TerminalOracleDependency::ProjectOptimumVectorFinalPoint
        );
        assert_eq!(descriptor.family, AlgorithmFamily::Continuous);
        assert!(descriptor.exact);
        assert!(!descriptor.randomized);
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(
            descriptor.implementation_scope,
            ImplementationScope::ProjectOracleDemonstrator
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit"),
            crate::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit"),
            crate::DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_EDGES
        );
        assert!(descriptor.complexity.contains("gap-below-1/2"));
        assert!(descriptor.complexity.contains("Kang--Payor"));
        assert!(descriptor.complexity.contains("oracle-constructed"));
        assert!(descriptor.complexity.contains("does not implement"));
        assert!(descriptor.complexity.contains("outer stopping loop"));
        assert_eq!(
            descriptor.search_terms,
            &["Deterministic almost-linear max flow"]
        );
    }

    #[test]
    fn deterministic_almost_linear_mcf_descriptor_matches_its_bounded_contract() {
        let descriptor = find_algorithm("deterministic-almost-linear-mcf")
            .expect("deterministic almost-linear MCF descriptor");
        assert_eq!(descriptor.kind, CatalogKind::Solver);
        assert_eq!(descriptor.family, AlgorithmFamily::Continuous);
        assert_eq!(descriptor.problems, BF_MCF);
        assert_eq!(descriptor.models, BF_MCF_MODELS);
        assert!(descriptor.exact);
        assert!(!descriptor.randomized);
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(
            descriptor.initial_band,
            BAND_DETERMINISTIC_ALMOST_LINEAR_MCF
        );
        assert_eq!(descriptor.graph_requirements, NO_SELF_LOOP_REQUIREMENTS);
        assert_eq!(
            descriptor.implementation_scope,
            ImplementationScope::BoundedOracleGuided
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit"),
            crate::FLOW_FRAMEWORK_MCF_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit"),
            crate::FLOW_FRAMEWORK_MCF_MAX_EDGES
        );
        assert!(descriptor.complexity.contains("bounded-oracle band"));
        assert!(descriptor.complexity.contains("retains only scalar F*"));
        assert!(descriptor.complexity.contains("HLD-refined F_T(R,pi)"));
        assert!(
            descriptor
                .complexity
                .contains("additive-1/2 final-point gate")
        );
        assert!(descriptor.complexity.contains("(mU)^-10"));
        assert!(descriptor.complexity.contains("not claimed"));
        assert_eq!(descriptor.source_id, "van-den-brand-et-al-2023");
    }

    #[test]
    fn weighted_augmenting_paths_descriptor_matches_the_exact_bounded_hierarchy() {
        let descriptor =
            find_algorithm("weighted-augmenting-paths").expect("weighted paths descriptor");
        assert_eq!(descriptor.kind, CatalogKind::Solver);
        assert_eq!(descriptor.family, AlgorithmFamily::AdvancedDiscrete);
        assert_eq!(descriptor.problems, MAX);
        assert_eq!(descriptor.models, MAX_MODELS);
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::SourceDefined
        );
        assert!(descriptor.exact);
        assert!(!descriptor.randomized);
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(
            descriptor.implementation_scope,
            ImplementationScope::BoundedOracleGuided
        );
        assert_eq!(descriptor.source_id, "bernstein-et-al-2024");
        assert_eq!(descriptor.initial_band, BAND_WEIGHTED_AUGMENTING_PATHS);
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit"),
            crate::WEIGHTED_AUGMENTING_PATHS_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit"),
            crate::WEIGHTED_AUGMENTING_PATHS_MAX_EDGES
        );
        assert!(descriptor.complexity.contains("exhaustive phi"));
        assert!(descriptor.complexity.contains("does not claim"));
    }

    #[test]
    fn weighted_push_relabel_descriptor_matches_the_bounded_shortcut_kernel() {
        let descriptor =
            find_algorithm("weighted-push-relabel").expect("weighted push-relabel descriptor");
        assert_eq!(descriptor.kind, CatalogKind::Solver);
        assert_eq!(descriptor.family, AlgorithmFamily::AdvancedDiscrete);
        assert_eq!(descriptor.problems, MAX);
        assert_eq!(descriptor.models, MAX_MODELS);
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::SourceDefined
        );
        assert!(descriptor.exact);
        assert!(!descriptor.randomized);
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(
            descriptor.implementation_scope,
            ImplementationScope::BoundedOracleGuided
        );
        assert_eq!(descriptor.source_id, "bernstein-et-al-2025");
        assert_eq!(descriptor.initial_band, BAND_WEIGHTED_PUSH_RELABEL_SHORTCUT);
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit"),
            crate::WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit"),
            crate::WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_EDGES
        );
        assert!(descriptor.complexity.contains("Steiner-star"));
        assert!(descriptor.complexity.contains("does not claim"));
    }

    #[test]
    fn excesses_ibfs_descriptor_matches_the_bounded_static_kernel() {
        let descriptor = find_algorithm("eibfs").expect("EIBFS descriptor exists");
        assert_eq!(descriptor.title, "Excesses IBFS");
        assert_eq!(descriptor.kind, CatalogKind::Solver);
        assert_eq!(descriptor.family, AlgorithmFamily::VisionGraph);
        assert_eq!(descriptor.models, [CatalogModelKind::MaxFlow]);
        assert_eq!(descriptor.problems, [ProblemKind::MaxFlow]);
        assert!(descriptor.graph_requirements.is_empty());
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::ZeroFeasible
        );
        assert_eq!(descriptor.source_id, "goldberg-hed-2015");
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit fits usize"),
            crate::EIBFS_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit fits usize"),
            crate::EIBFS_MAX_EDGES
        );
    }

    #[test]
    fn dynamic_eibfs_descriptor_exposes_the_capacity_update_contract() {
        let descriptor = find_algorithm("dynamic-eibfs").expect("Dynamic EIBFS descriptor exists");
        assert_eq!(descriptor.title, "Dynamic EIBFS");
        assert_eq!(descriptor.family, AlgorithmFamily::Dynamic);
        assert_eq!(descriptor.models, [CatalogModelKind::MaxFlow]);
        assert_eq!(descriptor.problems, [ProblemKind::DynamicMaxFlow]);
        assert_eq!(descriptor.source_id, "goldberg-hed-2015");
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert_eq!(
            descriptor.graph_requirements,
            [GraphRequirement::ZeroFlowFeasible]
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn production_filter_never_leaks_planned_entries() {
        let executable = executable_algorithms()
            .map(|descriptor| descriptor.id)
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
    // This is intentionally one closed identity table: splitting it would make
    // duplicate or missing executable dispatch rows harder to review.
    #[allow(clippy::too_many_lines)]
    fn executable_variants_declare_exact_runtime_models() {
        let edmonds_karp = find_algorithm("edmonds-karp").expect("descriptor exists");
        assert_eq!(edmonds_karp.models, [CatalogModelKind::MaxFlow]);

        for id in [
            "ford-fulkerson",
            "dfs-ford-fulkerson",
            "shortest-augmenting-path",
            "isap",
            "widest-augmenting-path",
            "capacity-scaling-augmenting-path",
            "distance-directed-augmenting-path",
            "distance-directed-scaling-augmenting-path",
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
            "global-relabel-heuristic",
            "gap-relabel-heuristic",
            "hochbaum-pseudoflow",
            "boykov-kolmogorov",
            "weighted-augmenting-paths",
            "weighted-push-relabel",
        ] {
            assert_eq!(
                find_algorithm(id).expect("descriptor exists").models,
                [CatalogModelKind::MaxFlow]
            );
        }

        for id in ["parametric-pseudoflow", "parametric-breakpoint-rerun"] {
            let descriptor = find_algorithm(id).expect("parametric descriptor exists");
            assert_eq!(descriptor.models, [CatalogModelKind::ParametricMaxFlow]);
            assert_eq!(descriptor.problems, [ProblemKind::ParametricMaxFlow]);
            assert_eq!(descriptor.status, ImplementationStatus::Executable);
        }

        let dinic = find_algorithm("dinic").expect("descriptor exists");
        assert_eq!(dinic.models, [CatalogModelKind::MaxFlow]);

        let bellman_ford = find_algorithm("bellman-ford-ssp").expect("descriptor exists");
        assert_eq!(
            bellman_ford.models,
            [
                CatalogModelKind::FixedFlowMinCost,
                CatalogModelKind::Circulation,
                CatalogModelKind::Transshipment,
            ]
        );
        assert!(
            !bellman_ford
                .models
                .contains(&CatalogModelKind::MinCostMaxFlow)
        );
        let potential_dijkstra =
            find_algorithm("potential-dijkstra-ssp").expect("descriptor exists");
        assert_eq!(potential_dijkstra.models, bellman_ford.models);
        let cycle_canceling = find_algorithm("simple-cycle-canceling").expect("descriptor exists");
        assert_eq!(cycle_canceling.models, bellman_ford.models);
        let minimum_mean =
            find_algorithm("minimum-mean-cycle-canceling").expect("descriptor exists");
        assert_eq!(minimum_mean.models, bellman_ford.models);
        let cancel_and_tighten = find_algorithm("cancel-and-tighten").expect("descriptor exists");
        assert_eq!(cancel_and_tighten.models, bellman_ford.models);
        assert_eq!(cancel_and_tighten.initial_band, BAND_SMALL);
        assert_eq!(
            cancel_and_tighten.initial_construction,
            InitialConstruction::AnyFeasible
        );
        assert_eq!(
            cancel_and_tighten.negative_cycle_policy,
            NegativeCyclePolicy::ResolveInternally
        );
        let ssap =
            find_algorithm("successive-shortest-augmenting-path").expect("descriptor exists");
        assert_eq!(ssap.models, [CatalogModelKind::MinCostMaxFlow]);
        assert_eq!(
            ssap.graph_requirements,
            [GraphRequirement::ZeroFlowFeasible]
        );
        let primal_dual = find_algorithm("primal-dual-mcf").expect("descriptor exists");
        assert_eq!(primal_dual.models, bellman_ford.models);
        assert_eq!(
            primal_dual.initial_optimality,
            InitialOptimalityRequirement::DualFeasible
        );
        let blocking_primal_dual =
            find_algorithm("blocking-flow-primal-dual").expect("descriptor exists");
        assert_eq!(blocking_primal_dual.models, bellman_ford.models);
        let capacity_scaling = find_algorithm("capacity-scaling-mcf").expect("descriptor exists");
        assert_eq!(capacity_scaling.models, bellman_ford.models);
        let excess_scaling = find_algorithm("excess-scaling-mcf").expect("descriptor exists");
        assert_eq!(excess_scaling.models, bellman_ford.models);
        assert_eq!(excess_scaling.initial_band, BAND_SMALL);
        assert_eq!(
            excess_scaling.graph_requirements,
            [GraphRequirement::NonbindingTransshipmentCapacities]
        );
        assert_eq!(
            excess_scaling.initial_optimality,
            InitialOptimalityRequirement::DualFeasible
        );
        for id in [
            "cost-scaling",
            "cost-scaling-push-relabel",
            "augment-relabel",
            "partial-augment-relabel-mcf",
            "price-refinement",
            "arc-fixing",
            "generalized-cost-scaling",
        ] {
            let descriptor = find_algorithm(id).expect("descriptor exists");
            assert_eq!(descriptor.models, bellman_ford.models);
            assert_eq!(descriptor.initial_band, BAND_COST_SCALING);
            assert_eq!(
                descriptor.initial_optimality,
                InitialOptimalityRequirement::EpsilonOptimal
            );
            assert_eq!(
                descriptor.negative_cycle_policy,
                NegativeCyclePolicy::ResolveInternally
            );
        }
    }

    #[test]
    fn blocking_primal_dual_descriptor_declares_its_exact_contract() {
        let descriptor = find_algorithm("blocking-flow-primal-dual").expect("descriptor exists");
        assert_eq!(descriptor.kind, CatalogKind::Variant);
        assert_eq!(descriptor.family, AlgorithmFamily::PrimalDual);
        assert_eq!(descriptor.models, BF_MCF_MODELS);
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::DualFeasible
        );
        assert_eq!(
            descriptor.initial_optimality,
            InitialOptimalityRequirement::DualFeasible
        );
        assert_eq!(
            descriptor.negative_cycle_policy,
            NegativeCyclePolicy::RequireAbsentAnywhere
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit fits usize"),
            crate::algorithms::BLOCKING_PRIMAL_DUAL_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit fits usize"),
            crate::algorithms::BLOCKING_PRIMAL_DUAL_MAX_EDGES
        );
        assert_eq!(descriptor.source_id, "ford-fulkerson-1957-dinitz-blocking");
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
    }

    #[test]
    fn relaxed_mndc_descriptor_matches_the_dense_assignment_kernel() {
        let descriptor = find_algorithm("relaxed-most-negative-cycle").expect("descriptor exists");
        assert_eq!(descriptor.kind, CatalogKind::Solver);
        assert_eq!(descriptor.family, AlgorithmFamily::CycleCanceling);
        assert_eq!(descriptor.models, BF_MCF_MODELS);
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::AnyFeasible
        );
        assert_eq!(
            descriptor.initial_optimality,
            InitialOptimalityRequirement::None
        );
        assert_eq!(
            descriptor.negative_cycle_policy,
            NegativeCyclePolicy::ResolveInternally
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit fits usize"),
            crate::RELAXED_MNDC_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit fits usize"),
            crate::RELAXED_MNDC_MAX_EDGES
        );
        assert_eq!(descriptor.source_id, "shigeno-iwata-mccormick-2000");
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
    }

    #[test]
    fn enhanced_capacity_scaling_descriptor_matches_orlin_section_four() {
        let descriptor = find_algorithm("enhanced-capacity-scaling").expect("descriptor exists");
        assert_eq!(descriptor.kind, CatalogKind::Variant);
        assert_eq!(descriptor.family, AlgorithmFamily::Scaling);
        assert_eq!(descriptor.models, TRANSSHIPMENT_MCF_MODELS);
        assert_eq!(
            descriptor.graph_requirements,
            [
                GraphRequirement::StronglyConnected,
                GraphRequirement::NonbindingTransshipmentCapacities
            ]
        );
        assert_eq!(
            descriptor.initial_construction,
            InitialConstruction::ZeroPseudoflowWithImbalance
        );
        assert_eq!(
            descriptor.initial_optimality,
            InitialOptimalityRequirement::DualFeasible
        );
        assert_eq!(
            descriptor.negative_cycle_policy,
            NegativeCyclePolicy::RequireAbsentAnywhere
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit fits usize"),
            crate::ENHANCED_CAPACITY_SCALING_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit fits usize"),
            crate::ENHANCED_CAPACITY_SCALING_MAX_EDGES
        );
        assert_eq!(descriptor.source_id, "orlin-1993");
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
    }

    #[test]
    fn dual_network_simplex_descriptor_is_the_natural_transshipment_pivot() {
        let descriptor = find_algorithm("dual-network-simplex").expect("descriptor exists");
        assert_eq!(descriptor.kind, CatalogKind::Solver);
        assert_eq!(descriptor.family, AlgorithmFamily::Simplex);
        assert_eq!(descriptor.models, TRANSSHIPMENT_MCF_MODELS);
        assert_eq!(
            descriptor.graph_requirements,
            [
                GraphRequirement::StronglyConnected,
                GraphRequirement::NonbindingTransshipmentCapacities
            ]
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_nodes).expect("node limit fits"),
            crate::DUAL_NETWORK_SIMPLEX_MAX_NODES
        );
        assert_eq!(
            usize::try_from(descriptor.initial_band.max_edges).expect("edge limit fits"),
            crate::DUAL_NETWORK_SIMPLEX_MAX_EDGES
        );
        assert_eq!(
            descriptor.source_id,
            "orlin-plotkin-tardos-1993-dual-simplex"
        );
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
    }

    #[test]
    fn polynomial_dual_simplex_exposes_its_source_domain_before_execution() {
        let descriptor =
            find_algorithm("polynomial-dual-network-simplex").expect("descriptor exists");
        assert_eq!(descriptor.models, TRANSSHIPMENT_MCF_MODELS);
        assert_eq!(
            descriptor.graph_requirements,
            [
                GraphRequirement::StronglyConnected,
                GraphRequirement::NonbindingTransshipmentCapacities
            ]
        );
    }

    #[test]
    fn feasible_start_pivot_solvers_declare_exact_runtime_models() {
        let bellman_ford = find_algorithm("bellman-ford-ssp").expect("descriptor exists");
        let network_simplex = find_algorithm("primal-network-simplex").expect("descriptor exists");
        assert_eq!(network_simplex.models, bellman_ford.models);
        assert_eq!(network_simplex.initial_band, BAND_SMALL);
        assert_eq!(
            network_simplex.initial_construction,
            InitialConstruction::AnyFeasible
        );
        assert_eq!(
            network_simplex.negative_cycle_policy,
            NegativeCyclePolicy::ResolveInternally
        );

        let out_of_kilter = find_algorithm("out-of-kilter").expect("descriptor exists");
        assert_eq!(out_of_kilter.models, bellman_ford.models);
        assert_eq!(out_of_kilter.initial_band, BAND_SMALL);
        assert_eq!(
            out_of_kilter.initial_band.max_nodes,
            u32::try_from(crate::OUT_OF_KILTER_MAX_NODES).expect("node limit fits catalog")
        );
        assert_eq!(
            out_of_kilter.initial_band.max_edges,
            u32::try_from(crate::OUT_OF_KILTER_MAX_EDGES).expect("edge limit fits catalog")
        );
        assert_eq!(out_of_kilter.status, ImplementationStatus::Executable);
        assert_eq!(
            out_of_kilter.initial_construction,
            InitialConstruction::AnyFeasible
        );
        assert_eq!(
            out_of_kilter.negative_cycle_policy,
            NegativeCyclePolicy::ResolveInternally
        );

        let relaxation = find_algorithm("relaxation").expect("descriptor exists");
        assert_eq!(relaxation.models, BF_MCF_MODELS);
        assert_eq!(relaxation.initial_band, BAND_SMALL);
        assert_eq!(
            relaxation.initial_band.max_nodes,
            u32::try_from(crate::RELAXATION_MAX_NODES).expect("node limit fits catalog")
        );
        assert_eq!(
            relaxation.initial_band.max_edges,
            u32::try_from(crate::RELAXATION_MAX_EDGES).expect("edge limit fits catalog")
        );
        assert_eq!(relaxation.status, ImplementationStatus::Executable);
        assert_eq!(
            relaxation.initial_construction,
            InitialConstruction::SourceDefined
        );
        assert_eq!(
            relaxation.initial_optimality,
            InitialOptimalityRequirement::SourceDefined
        );
        assert_eq!(
            relaxation.negative_cycle_policy,
            NegativeCyclePolicy::SourceDefined
        );

        let epsilon_relaxation = find_algorithm("epsilon-relaxation").expect("descriptor exists");
        assert_eq!(epsilon_relaxation.models, BF_MCF_MODELS);
        assert_eq!(epsilon_relaxation.initial_band, BAND_SMALL);
        assert_eq!(
            epsilon_relaxation.initial_band.max_nodes,
            u32::try_from(crate::EPSILON_RELAXATION_MAX_NODES).expect("node limit fits catalog")
        );
        assert_eq!(
            epsilon_relaxation.initial_band.max_edges,
            u32::try_from(crate::EPSILON_RELAXATION_MAX_EDGES).expect("edge limit fits catalog")
        );
        assert_eq!(epsilon_relaxation.status, ImplementationStatus::Executable);
        assert_eq!(
            epsilon_relaxation.initial_construction,
            InitialConstruction::SourceDefined
        );
        assert_eq!(
            epsilon_relaxation.initial_optimality,
            InitialOptimalityRequirement::SourceDefined
        );
        assert_eq!(
            epsilon_relaxation.negative_cycle_policy,
            NegativeCyclePolicy::SourceDefined
        );
    }

    #[test]
    fn specialized_complexity_claims_publish_graph_requirements() {
        assert!(
            find_algorithm("edmonds-karp")
                .expect("descriptor exists")
                .graph_requirements
                .is_empty()
        );
        assert_eq!(
            find_algorithm("successive-shortest-augmenting-path")
                .expect("descriptor exists")
                .graph_requirements,
            [GraphRequirement::ZeroFlowFeasible]
        );
        assert_eq!(
            find_algorithm("unit-capacity-dinic")
                .expect("descriptor exists")
                .graph_requirements,
            [GraphRequirement::UnitCapacity]
        );
        assert_eq!(
            find_algorithm("unit-network-dinic")
                .expect("descriptor exists")
                .graph_requirements,
            [
                GraphRequirement::UnitCapacity,
                GraphRequirement::UnitNetwork
            ]
        );
        let hopcroft_karp = find_algorithm("hopcroft-karp").expect("descriptor exists");
        assert_eq!(
            hopcroft_karp.graph_requirements,
            [GraphRequirement::Bipartite]
        );
        assert_eq!(hopcroft_karp.status, ImplementationStatus::Executable);
        assert_eq!(
            hopcroft_karp.initial_band.max_nodes,
            u32::try_from(crate::HOPCROFT_KARP_MAX_NODES).expect("node limit fits catalog")
        );
        assert_eq!(
            hopcroft_karp.initial_band.max_edges,
            u32::try_from(crate::HOPCROFT_KARP_MAX_EDGES).expect("edge limit fits catalog")
        );
        let hungarian = find_algorithm("hungarian").expect("descriptor exists");
        assert_eq!(hungarian.kind, CatalogKind::Solver);
        assert_eq!(hungarian.family, AlgorithmFamily::Assignment);
        assert_eq!(hungarian.models, [CatalogModelKind::Assignment]);
        assert_eq!(hungarian.problems, [ProblemKind::Assignment]);
        assert_eq!(hungarian.graph_requirements, [GraphRequirement::Bipartite]);
        assert_eq!(hungarian.status, ImplementationStatus::Executable);
        assert_eq!(hungarian.source_id, "kuhn-tomizawa-edmonds-karp-hungarian");
        assert_eq!(
            hungarian.initial_band.max_nodes,
            u32::try_from(crate::HUNGARIAN_MAX_NODES).expect("node limit fits catalog")
        );
        assert_eq!(
            hungarian.initial_band.max_edges,
            u32::try_from(crate::HUNGARIAN_MAX_EDGES).expect("edge limit fits catalog")
        );
        let auction = find_algorithm("auction").expect("descriptor exists");
        assert_eq!(auction.kind, CatalogKind::Solver);
        assert_eq!(auction.family, AlgorithmFamily::Assignment);
        assert_eq!(auction.models, [CatalogModelKind::Assignment]);
        assert_eq!(auction.problems, [ProblemKind::Assignment]);
        assert_eq!(auction.graph_requirements, [GraphRequirement::Bipartite]);
        assert_eq!(auction.status, ImplementationStatus::Executable);
        assert_eq!(auction.source_id, "bertsekas-auction-1988");
        assert_eq!(
            auction.initial_band.max_nodes,
            u32::try_from(crate::AUCTION_MAX_NODES).expect("node limit fits catalog")
        );
        assert_eq!(
            auction.initial_band.max_edges,
            u32::try_from(crate::AUCTION_MAX_EDGES).expect("edge limit fits catalog")
        );
        for id in ["transportation-simplex", "modi"] {
            let descriptor = find_algorithm(id).expect("transportation descriptor exists");
            assert_eq!(descriptor.family, AlgorithmFamily::Transportation);
            assert_eq!(descriptor.models, [CatalogModelKind::Transportation]);
            assert_eq!(descriptor.problems, [ProblemKind::Transportation]);
            assert_eq!(
                descriptor.graph_requirements,
                [
                    GraphRequirement::Bipartite,
                    GraphRequirement::TransportationNetwork,
                ]
            );
            assert_eq!(descriptor.status, ImplementationStatus::Executable);
            assert_eq!(descriptor.initial_band, BAND_SMALL);
        }
        assert_eq!(
            find_algorithm("hassin-st-planar")
                .expect("descriptor exists")
                .graph_requirements,
            [GraphRequirement::PlanarEmbedding]
        );
    }

    #[test]
    fn bounded_borradaile_klein_variant_is_executable_only_for_embedded_planar_models() {
        let descriptor = find_algorithm("borradaile-klein-planar").expect("descriptor exists");
        assert_eq!(
            descriptor.graph_requirements,
            [GraphRequirement::PlanarEmbedding]
        );
        assert_eq!(descriptor.models, [CatalogModelKind::PlanarMaxFlow]);
        assert_eq!(descriptor.status, ImplementationStatus::Executable);
        assert!(descriptor.complexity.contains("explicit-tree variant"));
        assert!(
            descriptor
                .complexity
                .contains("not the source dynamic-tree bound")
        );
    }

    #[test]
    fn ssp_contract_composes_construction_optimality_and_cycle_policy() {
        for id in [
            "successive-shortest-path",
            "bellman-ford-ssp",
            "potential-dijkstra-ssp",
            "successive-shortest-augmenting-path",
        ] {
            let descriptor = find_algorithm(id).expect("SSP descriptor exists");
            assert_eq!(
                descriptor.initial_construction,
                InitialConstruction::ZeroPseudoflowWithImbalance
            );
            assert_eq!(
                descriptor.initial_optimality,
                InitialOptimalityRequirement::OptimalForEveryPartialValue
            );
            assert_eq!(
                descriptor.negative_cycle_policy,
                NegativeCyclePolicy::RequireAbsentAnywhere
            );
        }
    }

    #[test]
    fn equal_constructions_can_have_independent_optimality_contracts() {
        let ssp = find_algorithm("successive-shortest-path").expect("SSP descriptor exists");
        let scaling =
            find_algorithm("capacity-scaling-mcf").expect("capacity-scaling descriptor exists");

        assert_eq!(ssp.initial_construction, scaling.initial_construction);
        assert_eq!(
            ssp.initial_optimality,
            InitialOptimalityRequirement::OptimalForEveryPartialValue
        );
        assert_eq!(
            scaling.initial_optimality,
            InitialOptimalityRequirement::DualFeasible
        );
    }
}
