//! Strict persisted Scenario contract owned by the flow plugin.

use std::collections::BTreeMap;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Zero;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;
use visualizer_core::jcs::{canonicalize, sha256_hex};
use visualizer_core::scenario::{
    ReproducibilityMetadata, ScenarioError as EnvelopeError, decode_scenario_envelope,
};

use crate::algorithms::{ConvexCostProblem, ConvexCostSegment, ConvexEdgeCost};
use crate::assignment::{AssignmentGraph, AssignmentObjectiveV1};
use crate::bipartite::BipartiteMatchingGraph;
use crate::catalog::AlgorithmId;
use crate::generator::{
    CapacityDistributionV1, CostDistributionV1, FLOW_GENERATOR_REVISION, FlowGeneratorFamilyV1,
    FlowGeneratorSpecV1, FlowGeneratorTargetProblemV1, difficulty_certificate, generate_flow_graph,
    generator_classification,
};
use crate::model::{EdgeId, FlowModelError, FlowNetwork, FlowNode, NodeId, UnresolvedFlowEdge};
use crate::planar::PlanarEmbedding;
use crate::transportation::TransportationGraph;
use crate::{ParametricCapacitySlope, ParametricMaxFlowProblem, ParametricRational};

/// Algorithm semantics revision implemented by the first flow contract.
pub const ALGORITHM_REVISION: &str = "flow-algorithms/8";
/// Plugin result contract revision.
pub const PLUGIN_RESULT_REVISION: &str = "flow-result/9";
/// Metrics catalog revision.
pub const METRICS_CATALOG_REVISION: &str = "flow-metrics/6";
/// Trace catalog revision.
pub const TRACE_REVISION: &str = "flow-trace/9";
/// Projection contract revision.
pub const PROJECTION_REVISION: &str = "flow-projection/6";
/// Layout contract revision.
pub const LAYOUT_REVISION: &str = "flow-layout/1";
/// Plugin-local scene revision carried in packet V6.
pub const FRAME_ENCODING_REVISION: &str = "flow-scene/9";
/// Deterministic RNG contract revision.
pub const RNG_VERSION: u32 = 1;
/// Maximum number of dynamic updates in one Scenario.
pub const MAX_FLOW_UPDATES: usize = 100_000;
/// Source-defined Dynamic EIBFS capacity-update ceiling.
pub const DYNAMIC_EIBFS_MAX_UPDATES: usize = 256;
/// Maximum canonical JSON bytes of algorithm-owned configuration.
pub const MAX_ALGORITHM_CONFIG_BYTES: usize = 1024 * 1024;

/// Fully typed flow Scenario V1.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowScenarioV1 {
    /// Must equal `1`.
    pub schema_version: u32,
    /// Must equal `rfc8785-jcs/1`.
    pub scenario_encoding_revision: String,
    /// Must equal `flow`.
    pub plugin: String,
    /// Declared reproducibility revisions.
    pub reproducibility: ReproducibilityMetadata,
    /// Flow-plugin-owned payload.
    pub payload: FlowScenarioPayloadV1,
}

/// Primary flow payload shared by static and dynamic models.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowScenarioPayloadV1 {
    /// Problem semantics and terminals.
    pub model: FlowProblemModelV1,
    /// Materialized directed graph.
    pub graph: FlowGraphV1,
    /// Selected catalog algorithm and revision-owned configuration.
    pub algorithm: FlowAlgorithmSelectionV1,
    /// Trace, fast, or deterministic CPU-parallel execution.
    pub run_profile: RunProfileV1,
    /// Requested pedagogical event granularity.
    pub trace_granularity: TraceGranularityV1,
    /// Canonical unsigned 64-bit decimal seed.
    pub algorithm_seed: String,
    /// Optional bounded update sequence for dynamic models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub updates: Option<Vec<FlowUpdateV1>>,
    /// Optional generator provenance retained until materialized input is edited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub generator_provenance: Option<GeneratorProvenanceV1>,
}

/// Base problem models admitted by the first graph contract.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "kebab-case")]
pub enum FlowProblemModelV1 {
    /// Maximize flow between two terminals.
    MaxFlow {
        /// Source node identity.
        source: String,
        /// Sink node identity.
        sink: String,
    },
    /// Compute all exact minimum-cut regions for monotone affine terminal capacities.
    ParametricMaxFlow {
        /// Source node identity.
        source: String,
        /// Sink node identity.
        sink: String,
        /// Closed exact parameter domain.
        parameter: FlowParametricRangeV1,
        /// Nonzero affine coefficients in stable edge-ID order.
        capacity_slopes: Vec<FlowParametricCapacitySlopeV1>,
    },
    /// Send an exact amount between two terminals at minimum cost.
    FixedFlowMinCost {
        /// Source node identity.
        source: String,
        /// Sink node identity.
        sink: String,
        /// Canonical unsigned 64-bit decimal required flow.
        required_flow: String,
    },
    /// Maximize flow, then minimize cost at that value.
    MinCostMaxFlow {
        /// Source node identity.
        source: String,
        /// Sink node identity.
        sink: String,
    },
    /// Find or optimize a circulation under node balances.
    Circulation {},
    /// Satisfy node supplies and demands at minimum cost.
    Transshipment {},
    /// Find a maximum-cardinality matching between explicit vertex partitions.
    BipartiteMatching {
        /// Left partition in strictly increasing canonical node-ID order.
        left: Vec<String>,
        /// Right partition in strictly increasing canonical node-ID order.
        right: Vec<String>,
        /// Optional exact `s-L-R-t` unit-flow representation retained by a generator.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        flow_adapter: Option<FlowBipartiteAdapterV1>,
    },
    /// Assign every declared agent to one distinct allowed task.
    Assignment {
        /// Agents in strictly increasing canonical node-ID order.
        agents: Vec<String>,
        /// Tasks in strictly increasing canonical node-ID order.
        tasks: Vec<String>,
        /// Whether selected edge costs are minimized or maximized.
        objective: AssignmentObjectiveV1,
    },
    /// Ship an explicitly balanced amount from origins to destinations.
    Transportation {
        /// Positive-supply origins in canonical node-ID order.
        origins: Vec<String>,
        /// Negative-supply destinations in canonical node-ID order.
        destinations: Vec<String>,
    },
    /// Maximize flow in a connected graph with an explicit planar rotation system.
    PlanarMaxFlow {
        /// Source node identity.
        source: String,
        /// Sink node identity.
        sink: String,
        /// Combinatorial embedding; coordinates are never used as a substitute.
        embedding: FlowPlanarEmbeddingV1,
    },
    /// Satisfy node supplies and demands under separable integral convex costs.
    ConvexCostFlow {},
}

/// Canonical arbitrary-precision rational wire value.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowRationalV1 {
    /// Canonical signed decimal numerator.
    pub numerator: String,
    /// Canonical positive decimal denominator.
    pub denominator: String,
}

/// Closed exact parameter interval.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowParametricRangeV1 {
    /// Inclusive lower endpoint.
    pub minimum: FlowRationalV1,
    /// Inclusive upper endpoint.
    pub maximum: FlowRationalV1,
}

/// One nonzero affine capacity coefficient keyed by stable edge identity.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowParametricCapacitySlopeV1 {
    /// Stable original edge identity.
    pub edge_id: String,
    /// Canonical arbitrary-precision signed decimal coefficient.
    pub slope: String,
}

/// One oriented incidence of an original edge in a planar rotation system.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPlanarDartV1 {
    /// Stable original-edge identity.
    pub edge_id: String,
    /// Whether the dart follows or reverses the original edge direction.
    pub direction: FlowPlanarDartDirectionV1,
}

/// Orientation of one planar dart relative to its original directed edge.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, Ord, PartialEq, PartialOrd, Serialize, TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum FlowPlanarDartDirectionV1 {
    /// Tail-to-head orientation of the original edge.
    Forward,
    /// Head-to-tail orientation of the original edge.
    Reverse,
}

/// Clockwise cyclic order of all darts leaving one node.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPlanarRotationV1 {
    /// Node identity; rotations are listed in canonical node-ID order.
    pub node_id: String,
    /// Clockwise cyclic order. A self-loop contributes both dart directions.
    pub darts: Vec<FlowPlanarDartV1>,
}

/// Explicit source/sink corners used to split one common face for Hassin's construction.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPlanarTerminalCornersV1 {
    /// Dart leaving the source whose left face is the designated outer face.
    pub source: FlowPlanarDartV1,
    /// Dart leaving the sink whose left face is the designated outer face.
    pub sink: FlowPlanarDartV1,
}

/// Connected orientable rotation system plus explicit face anchors.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPlanarEmbeddingV1 {
    /// One clockwise rotation for every graph node.
    pub rotations: Vec<FlowPlanarRotationV1>,
    /// A dart with the designated outer face on its left.
    pub outer_face: FlowPlanarDartV1,
    /// Optional unambiguous corners for an st-planar common-face construction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub terminal_corners: Option<FlowPlanarTerminalCornersV1>,
}

/// Explicit auxiliary terminals for a matching-equivalent unit-flow network.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowBipartiteAdapterV1 {
    /// Source with exactly one unit arc to every left vertex.
    pub source: String,
    /// Sink with exactly one unit arc from every right vertex.
    pub sink: String,
}

/// Materialized directed graph declaration.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowGraphV1 {
    /// Stable node declarations.
    pub nodes: Vec<FlowNodeV1>,
    /// Stable original-edge declarations.
    pub edges: Vec<FlowEdgeV1>,
}

/// Persisted node declaration.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowNodeV1 {
    /// Stable 1–64-scalar node identity.
    pub id: String,
    /// Canonical signed 64-bit decimal supply; demand is negative.
    #[serde(default = "zero_decimal")]
    pub supply: String,
    /// Optional revision-fixed layout hint in signed fixed-point coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub position: Option<FlowPositionV1>,
}

/// Deterministic layout hint; values use signed fixed-point integer units.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowPositionV1 {
    /// Canonical signed 64-bit decimal x coordinate.
    pub x: String,
    /// Canonical signed 64-bit decimal y coordinate.
    pub y: String,
}

/// Persisted original-edge declaration.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowEdgeV1 {
    /// Stable 1–64-scalar edge identity.
    pub id: String,
    /// Tail node identity.
    pub from: String,
    /// Head node identity.
    pub to: String,
    /// Canonical unsigned 64-bit decimal lower bound.
    #[serde(default = "zero_decimal")]
    pub lower: String,
    /// Canonical unsigned 64-bit decimal capacity.
    pub capacity: String,
    /// Canonical signed 64-bit decimal unit cost.
    #[serde(default = "zero_decimal")]
    pub cost: String,
    /// Optional complete piecewise-linear convex objective replacing `cost`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub convex_cost: Option<FlowConvexCostV1>,
    /// Optional canonical unsigned 64-bit decimal initial flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub initial_flow: Option<String>,
}

/// Exact piecewise-linear separable convex objective on one original edge.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowConvexCostV1 {
    /// Constant objective contribution at zero flow as canonical `i128`.
    pub base_cost_at_zero: String,
    /// Strictly increasing integral breakpoints and nondecreasing slopes.
    pub segments: Vec<FlowConvexCostSegmentV1>,
}

/// One half-open integral-flow segment of a convex edge objective.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowConvexCostSegmentV1 {
    /// Exclusive upper flow boundary as canonical `u64`.
    pub end_flow: String,
    /// Marginal unit cost as canonical `i64`.
    pub marginal_cost: String,
}

/// Catalog algorithm selection.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlowAlgorithmSelectionV1 {
    /// Stable catalog ID.
    pub id: String,
    /// Algorithm-revision-owned closed configuration, decoded after dispatch.
    pub config: BTreeMap<String, serde_json::Value>,
}

/// Execution profile.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum RunProfileV1 {
    /// Record reversible pedagogical events.
    Trace,
    /// Compute only result, certificate, metrics, and bounded progress.
    Fast,
    /// Deterministic synchronous CPU-parallel execution.
    CpuParallel,
}

/// Trace detail requested by the user.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum TraceGranularityV1 {
    /// Major algorithm phases only.
    Phase,
    /// One event per meaningful algorithm operation.
    Operation,
    /// Source-defined inner steps where admission permits them.
    Micro,
}

/// A dynamic graph update retained in canonical order.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "kebab-case")]
pub enum FlowUpdateV1 {
    /// Change one original edge's capacity.
    SetCapacity {
        /// Original edge identity.
        edge: String,
        /// Canonical unsigned 64-bit decimal capacity.
        capacity: String,
    },
    /// Add a complete original edge declaration.
    AddEdge {
        /// New original edge.
        edge: FlowEdgeV1,
    },
    /// Remove one original edge.
    RemoveEdge {
        /// Original edge identity.
        edge: String,
    },
    /// Change a max-flow source or sink.
    SetTerminals {
        /// Source node identity.
        source: String,
        /// Sink node identity.
        sink: String,
    },
}

/// Generator metadata that does not participate in graph semantics.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GeneratorProvenanceV1 {
    /// Generator contract revision.
    pub generator_revision: String,
    /// Stable generator family ID.
    pub family_id: String,
    /// Canonical unsigned 64-bit decimal seed.
    pub seed: String,
    /// Family-revision-owned parameter object.
    pub parameters: BTreeMap<String, serde_json::Value>,
    /// SHA-256 of the generated semantic payload excluding provenance.
    pub materialized_sha256: String,
    /// Closed difficulty axis: `ordinary`, `stress`, or `verified-worst-case`.
    /// Revisions before 8 omit the other classification axes and certificate.
    pub difficulty: String,
    /// Construction provenance axis, required in generator revision 8+.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
    /// Whether topology or fixed attributes consume a seeded random stream.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sampling: String,
    /// Sorted, unique structural and model tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Primary-source registry ID when the family is source-derived.
    pub source_id: String,
    /// Machine-readable exact operation-growth claim for verified worst cases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty_certificate: Option<GeneratorDifficultyCertificateV1>,
}

/// Exact, source-backed operation-growth claim attached to a generated graph.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GeneratorDifficultyCertificateV1 {
    /// Catalog algorithm whose deterministic implementation is targeted.
    pub target_algorithm_id: String,
    /// Stable identifier for the ordering choices required by the claim.
    pub tie_breaking: String,
    /// Exact unsigned metric values, keyed by the closed metric IDs validated below.
    pub exact_metrics: BTreeMap<String, String>,
}

fn zero_decimal() -> String {
    "0".to_owned()
}

impl FlowScenarioV1 {
    /// Resolves the persisted declarations into the canonical graph model.
    ///
    /// # Errors
    ///
    /// Rejects invalid decimal strings, graph identities, bounds, endpoints,
    /// duplicates, or checked aggregate overflow.
    pub fn canonical_network(&self) -> Result<FlowNetwork, FlowScenarioError> {
        let nodes = self
            .payload
            .graph
            .nodes
            .iter()
            .map(|node| {
                Ok(FlowNode::new(
                    NodeId::parse(&node.id)?,
                    parse_i64(&node.supply, "node supply")?,
                ))
            })
            .collect::<Result<Vec<_>, FlowScenarioError>>()?;
        let edges = self
            .payload
            .graph
            .edges
            .iter()
            .map(|edge| {
                let lower = parse_u64(&edge.lower, "edge lower")?;
                let capacity = parse_u64(&edge.capacity, "edge capacity")?;
                if let Some(initial_flow) = &edge.initial_flow {
                    let initial_flow = parse_u64(initial_flow, "edge initial flow")?;
                    if initial_flow < lower || initial_flow > capacity {
                        return Err(FlowScenarioError::Invalid("edge initial flow bounds"));
                    }
                }
                Ok(UnresolvedFlowEdge {
                    id: EdgeId::parse(&edge.id)?,
                    from: NodeId::parse(&edge.from)?,
                    to: NodeId::parse(&edge.to)?,
                    lower,
                    capacity,
                    cost: parse_i64(&edge.cost, "edge cost")?,
                })
            })
            .collect::<Result<Vec<_>, FlowScenarioError>>()?;
        FlowNetwork::new(nodes, edges).map_err(Into::into)
    }

    /// Builds the exact validated parametric problem over an already
    /// canonicalized network.
    ///
    /// # Errors
    ///
    /// Rejects a nonparametric model, mismatched graph, invalid exact number,
    /// coefficient, terminal, monotonicity, or bounded capacity domain.
    pub fn parametric_problem(
        &self,
        network: &FlowNetwork,
    ) -> Result<ParametricMaxFlowProblem, FlowScenarioError> {
        let declaration = self.resolve_parametric_declaration(network)?;
        ParametricMaxFlowProblem::new(
            network,
            declaration.source,
            declaration.sink,
            declaration.minimum,
            declaration.maximum,
            declaration.coefficients,
        )
        .map_err(|_| FlowScenarioError::Invalid("parametric max-flow model"))
    }

    fn validate_parametric_declaration(
        &self,
        network: &FlowNetwork,
    ) -> Result<(), FlowScenarioError> {
        let declaration = self.resolve_parametric_declaration(network)?;
        ParametricMaxFlowProblem::validate_declaration(
            network,
            declaration.source,
            declaration.sink,
            declaration.minimum,
            declaration.maximum,
            declaration.coefficients,
        )
        .map_err(|_| FlowScenarioError::Invalid("parametric max-flow model"))
    }

    fn resolve_parametric_declaration(
        &self,
        network: &FlowNetwork,
    ) -> Result<ResolvedParametricDeclaration, FlowScenarioError> {
        let FlowProblemModelV1::ParametricMaxFlow {
            source,
            sink,
            parameter,
            capacity_slopes,
        } = &self.payload.model
        else {
            return Err(FlowScenarioError::Invalid("nonparametric flow model"));
        };
        let source = network
            .node_index(&NodeId::parse(source)?)
            .ok_or(FlowScenarioError::Invalid("missing parametric source"))?;
        let sink = network
            .node_index(&NodeId::parse(sink)?)
            .ok_or(FlowScenarioError::Invalid("missing parametric sink"))?;
        let coefficients = capacity_slopes
            .iter()
            .map(|coefficient| {
                Ok(ParametricCapacitySlope {
                    edge: EdgeId::parse(&coefficient.edge_id)?,
                    slope: parse_bigint(&coefficient.slope, true)?,
                })
            })
            .collect::<Result<Vec<_>, FlowScenarioError>>()?;
        Ok(ResolvedParametricDeclaration {
            source,
            sink,
            minimum: parse_rational(&parameter.minimum)?,
            maximum: parse_rational(&parameter.maximum)?,
            coefficients,
        })
    }

    /// Builds the exact native convex-cost problem over a canonical network.
    ///
    /// Edges without `convex_cost` retain their ordinary linear `cost` as one
    /// segment. A present convex objective replaces `cost`, which must be zero.
    ///
    /// # Errors
    ///
    /// Rejects a nonconvex model, graph mismatch, noncanonical numbers,
    /// ambiguous linear-plus-convex costs, or invalid segment data.
    pub fn convex_cost_problem<'graph>(
        &self,
        network: &'graph FlowNetwork,
    ) -> Result<ConvexCostProblem<'graph>, FlowScenarioError> {
        let edge_costs = self.resolve_convex_edge_costs(network)?;
        ConvexCostProblem::new(network, edge_costs)
            .map_err(|_| FlowScenarioError::Invalid("convex-cost flow model"))
    }

    fn validate_convex_cost_declaration(
        &self,
        network: &FlowNetwork,
    ) -> Result<(), FlowScenarioError> {
        let edge_costs = self.resolve_convex_edge_costs(network)?;
        ConvexCostProblem::validate_declaration(network, &edge_costs)
            .map_err(|_| FlowScenarioError::Invalid("convex-cost flow model"))
    }

    fn resolve_convex_edge_costs(
        &self,
        network: &FlowNetwork,
    ) -> Result<Vec<ConvexEdgeCost>, FlowScenarioError> {
        if !matches!(self.payload.model, FlowProblemModelV1::ConvexCostFlow {}) {
            return Err(FlowScenarioError::Invalid("nonconvex flow model"));
        }
        let by_id = self
            .payload
            .graph
            .edges
            .iter()
            .map(|edge| (edge.id.as_str(), edge))
            .collect::<BTreeMap<_, _>>();
        network
            .edges()
            .iter()
            .map(|edge| {
                let declaration = by_id
                    .get(edge.id().as_str())
                    .ok_or(FlowScenarioError::Invalid("convex edge identity mismatch"))?;
                match &declaration.convex_cost {
                    Some(cost) => {
                        if parse_i64(&declaration.cost, "convex edge linear cost")? != 0 {
                            return Err(FlowScenarioError::Invalid(
                                "convex edge cannot also declare linear cost",
                            ));
                        }
                        Ok(ConvexEdgeCost {
                            base_cost_at_zero: parse_i128(
                                &cost.base_cost_at_zero,
                                "convex base cost",
                            )?,
                            segments: cost
                                .segments
                                .iter()
                                .map(|segment| {
                                    Ok(ConvexCostSegment {
                                        end_flow: parse_u64(
                                            &segment.end_flow,
                                            "convex segment end",
                                        )?,
                                        marginal_cost: parse_i64(
                                            &segment.marginal_cost,
                                            "convex marginal cost",
                                        )?,
                                    })
                                })
                                .collect::<Result<Vec<_>, FlowScenarioError>>()?,
                        })
                    }
                    None => Ok(ConvexEdgeCost {
                        base_cost_at_zero: 0,
                        segments: (edge.capacity() > 0)
                            .then_some(ConvexCostSegment {
                                end_flow: edge.capacity(),
                                marginal_cost: edge.cost(),
                            })
                            .into_iter()
                            .collect(),
                    }),
                }
            })
            .collect::<Result<Vec<_>, FlowScenarioError>>()
    }
}

struct ResolvedParametricDeclaration {
    source: crate::model::NodeIndex,
    sink: crate::model::NodeIndex,
    minimum: ParametricRational,
    maximum: ParametricRational,
    coefficients: Vec<ParametricCapacitySlope>,
}

/// Flow Scenario decode or validation failure.
#[derive(Debug, Error)]
pub enum FlowScenarioError {
    /// Shared envelope validation failed.
    #[error(transparent)]
    Envelope(#[from] EnvelopeError),
    /// Strict plugin payload decoding failed.
    #[error("invalid flow Scenario JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Canonical graph construction failed.
    #[error(transparent)]
    Model(#[from] FlowModelError),
    /// A required flow revision is not supported by this build.
    #[error("unsupported flow Scenario contract: {0}")]
    Unsupported(&'static str),
    /// A bounded semantic condition failed.
    #[error("invalid flow Scenario value: {0}")]
    Invalid(&'static str),
}

/// Strictly decodes and validates one flow Scenario.
///
/// # Errors
///
/// Rejects invalid shared or plugin revisions, duplicate/unknown fields,
/// noncanonical numeric strings, invalid graph declarations, and bounded
/// resource-limit violations.
pub fn decode_flow_scenario(bytes: &[u8]) -> Result<FlowScenarioV1, FlowScenarioError> {
    let raw = decode_scenario_envelope(bytes)?;
    if raw.plugin != "flow" {
        return Err(FlowScenarioError::Unsupported("plugin"));
    }
    let payload: FlowScenarioPayloadV1 = serde_json::from_str(raw.payload.get())?;
    let scenario = FlowScenarioV1 {
        schema_version: raw.schema_version,
        scenario_encoding_revision: raw.scenario_encoding_revision,
        plugin: raw.plugin,
        reproducibility: raw.reproducibility,
        payload,
    };
    validate_flow_scenario(&scenario)?;
    Ok(scenario)
}

pub(crate) fn validate_flow_scenario(scenario: &FlowScenarioV1) -> Result<(), FlowScenarioError> {
    let declared = &scenario.reproducibility.declared;
    if declared.algorithm_revision != ALGORITHM_REVISION {
        return Err(FlowScenarioError::Unsupported("algorithm_revision"));
    }
    if declared.rng_version != RNG_VERSION {
        return Err(FlowScenarioError::Unsupported("rng_version"));
    }
    if declared.plugin_result_revision != PLUGIN_RESULT_REVISION {
        return Err(FlowScenarioError::Unsupported("plugin_result_revision"));
    }
    if declared.metrics_catalog_revision != METRICS_CATALOG_REVISION {
        return Err(FlowScenarioError::Unsupported("metrics_catalog_revision"));
    }
    if declared.trace_revision != TRACE_REVISION
        || declared.projection_revision != PROJECTION_REVISION
        || declared.layout_revision != LAYOUT_REVISION
        || declared.frame_encoding_revision != FRAME_ENCODING_REVISION
    {
        return Err(FlowScenarioError::Unsupported("derived revision"));
    }
    parse_u64(&scenario.payload.algorithm_seed, "algorithm seed")?;
    scenario
        .payload
        .algorithm
        .id
        .parse::<AlgorithmId>()
        .map_err(|_| FlowScenarioError::Unsupported("algorithm id"))?;
    let config_bytes = serde_json::to_vec(&scenario.payload.algorithm.config)?;
    if config_bytes.len() > MAX_ALGORITHM_CONFIG_BYTES {
        return Err(FlowScenarioError::Invalid("algorithm config byte limit"));
    }
    if scenario
        .payload
        .updates
        .as_ref()
        .is_some_and(|updates| updates.len() > MAX_FLOW_UPDATES)
    {
        return Err(FlowScenarioError::Invalid("dynamic update limit"));
    }
    validate_model(scenario)?;
    validate_updates(scenario)?;
    validate_dynamic_eibfs_selection(scenario)?;
    validate_warm_start_selection(scenario)?;
    validate_provenance(scenario)?;
    let network = scenario.canonical_network()?;
    if matches!(
        scenario.payload.model,
        FlowProblemModelV1::FixedFlowMinCost { .. }
            | FlowProblemModelV1::Circulation {}
            | FlowProblemModelV1::Transshipment {}
            | FlowProblemModelV1::ConvexCostFlow {}
    ) {
        network.validate_balanced_supplies().map_err(|_| {
            FlowScenarioError::Invalid("balance-flow models require zero total node supply")
        })?;
    }
    if matches!(
        scenario.payload.model,
        FlowProblemModelV1::ConvexCostFlow {}
    ) {
        scenario.validate_convex_cost_declaration(&network)?;
    }
    Ok(())
}

fn validate_model(scenario: &FlowScenarioV1) -> Result<(), FlowScenarioError> {
    let graph = &scenario.payload.graph;
    if matches!(
        scenario.payload.algorithm.id.as_str(),
        "parametric-pseudoflow" | "parametric-breakpoint-rerun"
    ) && !matches!(
        scenario.payload.model,
        FlowProblemModelV1::ParametricMaxFlow { .. }
    ) {
        return Err(FlowScenarioError::Invalid(
            "parametric algorithm requires parametric max-flow model",
        ));
    }
    match &scenario.payload.model {
        FlowProblemModelV1::MaxFlow { source, sink }
        | FlowProblemModelV1::MinCostMaxFlow { source, sink } => {
            validate_model_terminals(graph, source, sink)?;
            if graph.nodes.iter().any(|node| node.supply != "0") {
                return Err(FlowScenarioError::Invalid(
                    "terminal-flow models require zero node supplies",
                ));
            }
        }
        FlowProblemModelV1::ParametricMaxFlow {
            source,
            sink,
            parameter: _,
            capacity_slopes,
        } => {
            validate_model_terminals(graph, source, sink)?;
            validate_parametric_model(scenario, capacity_slopes)?;
        }
        FlowProblemModelV1::FixedFlowMinCost {
            source,
            sink,
            required_flow,
        } => {
            validate_model_terminals(graph, source, sink)?;
            parse_u64(required_flow, "required flow")?;
        }
        FlowProblemModelV1::Circulation {} | FlowProblemModelV1::Transshipment {} => {}
        FlowProblemModelV1::BipartiteMatching {
            left,
            right,
            flow_adapter,
        } => {
            validate_bipartite_matching_model(scenario, left, right, flow_adapter.as_ref())?;
        }
        FlowProblemModelV1::Assignment {
            agents,
            tasks,
            objective,
        } => {
            validate_assignment_model(scenario, agents, tasks, *objective)?;
        }
        FlowProblemModelV1::Transportation {
            origins,
            destinations,
        } => {
            validate_transportation_model(scenario, origins, destinations)?;
        }
        FlowProblemModelV1::PlanarMaxFlow {
            source,
            sink,
            embedding,
        } => {
            validate_planar_model(scenario, source, sink, embedding)?;
        }
        FlowProblemModelV1::ConvexCostFlow {} => {
            if scenario.payload.updates.is_some() {
                return Err(FlowScenarioError::Invalid(
                    "convex-cost flow does not accept dynamic updates",
                ));
            }
        }
    }
    Ok(())
}

fn validate_model_terminals(
    graph: &FlowGraphV1,
    source: &str,
    sink: &str,
) -> Result<(), FlowScenarioError> {
    NodeId::parse(source)?;
    NodeId::parse(sink)?;
    if source == sink {
        return Err(FlowScenarioError::Invalid("source equals sink"));
    }
    if !graph.nodes.iter().any(|node| node.id == source)
        || !graph.nodes.iter().any(|node| node.id == sink)
    {
        return Err(FlowScenarioError::Invalid("missing terminal node"));
    }
    Ok(())
}

fn validate_planar_model(
    scenario: &FlowScenarioV1,
    source: &str,
    sink: &str,
    embedding: &FlowPlanarEmbeddingV1,
) -> Result<(), FlowScenarioError> {
    validate_model_terminals(&scenario.payload.graph, source, sink)?;
    let network = scenario.canonical_network()?;
    let source_index = network
        .node_index(&NodeId::parse(source)?)
        .ok_or(FlowScenarioError::Invalid("missing planar source"))?;
    let sink_index = network
        .node_index(&NodeId::parse(sink)?)
        .ok_or(FlowScenarioError::Invalid("missing planar sink"))?;
    PlanarEmbedding::new(&network, source_index, sink_index, embedding)
        .map_err(|_| FlowScenarioError::Invalid("planar embedding"))?;
    if network.nodes().iter().any(|node| node.supply() != 0)
        || network.edges().iter().any(|edge| edge.lower() != 0)
    {
        return Err(FlowScenarioError::Invalid(
            "planar max-flow requires zero supplies and lower bounds",
        ));
    }
    validate_zero_initial_flow(
        &scenario.payload.graph,
        "planar max-flow initial flow must be zero",
    )
}

fn validate_parametric_model(
    scenario: &FlowScenarioV1,
    capacity_slopes: &[FlowParametricCapacitySlopeV1],
) -> Result<(), FlowScenarioError> {
    if scenario.payload.updates.is_some() {
        return Err(FlowScenarioError::Invalid(
            "parametric max-flow does not accept dynamic updates",
        ));
    }
    if !matches!(
        scenario.payload.algorithm.id.as_str(),
        "parametric-pseudoflow" | "parametric-breakpoint-rerun"
    ) {
        return Err(FlowScenarioError::Invalid(
            "parametric max-flow requires a parametric algorithm",
        ));
    }
    if capacity_slopes
        .windows(2)
        .any(|pair| pair[0].edge_id >= pair[1].edge_id)
    {
        return Err(FlowScenarioError::Invalid(
            "parametric coefficients must be in strict edge-ID order",
        ));
    }
    let network = scenario.canonical_network()?;
    scenario.validate_parametric_declaration(&network)?;
    validate_zero_initial_flow(
        &scenario.payload.graph,
        "parametric max-flow initial flow must be zero",
    )
}

fn parse_rational(value: &FlowRationalV1) -> Result<ParametricRational, FlowScenarioError> {
    let numerator = parse_bigint(&value.numerator, true)?;
    let denominator = parse_bigint(&value.denominator, false)?;
    if denominator <= BigInt::zero() {
        return Err(FlowScenarioError::Invalid(
            "parametric denominator must be positive",
        ));
    }
    let normalized = BigRational::new(numerator.clone(), denominator.clone());
    if normalized.numer() != &numerator || normalized.denom() != &denominator {
        return Err(FlowScenarioError::Invalid(
            "parametric rational must be normalized",
        ));
    }
    ParametricRational::new(numerator, denominator)
        .map_err(|_| FlowScenarioError::Invalid("parametric rational"))
}

fn parse_bigint(value: &str, signed: bool) -> Result<BigInt, FlowScenarioError> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    if value.is_empty()
        || value.starts_with('+')
        || (!signed && value.starts_with('-'))
        || digits.is_empty()
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
        || value == "-0"
        || digits.len() > 128
    {
        return Err(FlowScenarioError::Invalid(
            "noncanonical parametric integer",
        ));
    }
    value
        .parse::<BigInt>()
        .map_err(|_| FlowScenarioError::Invalid("parametric integer"))
}

fn validate_bipartite_matching_model(
    scenario: &FlowScenarioV1,
    left: &[String],
    right: &[String],
    flow_adapter: Option<&FlowBipartiteAdapterV1>,
) -> Result<(), FlowScenarioError> {
    let network = scenario.canonical_network()?;
    let adapter = flow_adapter.map(|adapter| (adapter.source.as_str(), adapter.sink.as_str()));
    BipartiteMatchingGraph::new(&network, left, right, adapter)
        .map_err(|_| FlowScenarioError::Invalid("bipartite matching model"))?;
    validate_zero_initial_flow(
        &scenario.payload.graph,
        "bipartite matching initial flow must be zero",
    )
}

fn validate_assignment_model(
    scenario: &FlowScenarioV1,
    agents: &[String],
    tasks: &[String],
    objective: AssignmentObjectiveV1,
) -> Result<(), FlowScenarioError> {
    let graph = &scenario.payload.graph;
    let network = scenario.canonical_network()?;
    let _ = objective;
    AssignmentGraph::validate_declaration(&network, agents, tasks)
        .map_err(|_| FlowScenarioError::Invalid("assignment model"))?;
    validate_zero_initial_flow(graph, "assignment initial flow must be zero")
}

fn validate_transportation_model(
    scenario: &FlowScenarioV1,
    origins: &[String],
    destinations: &[String],
) -> Result<(), FlowScenarioError> {
    let graph = &scenario.payload.graph;
    let network = scenario.canonical_network()?;
    TransportationGraph::validate_declaration(&network, origins, destinations)
        .map_err(|_| FlowScenarioError::Invalid("transportation model"))?;
    validate_zero_initial_flow(graph, "transportation initial flow must be zero")
}

fn validate_zero_initial_flow(
    graph: &FlowGraphV1,
    error: &'static str,
) -> Result<(), FlowScenarioError> {
    if graph
        .edges
        .iter()
        .any(|edge| edge.initial_flow.as_deref().is_some_and(|flow| flow != "0"))
    {
        return Err(FlowScenarioError::Invalid(error));
    }
    Ok(())
}

fn validate_updates(scenario: &FlowScenarioV1) -> Result<(), FlowScenarioError> {
    let Some(updates) = &scenario.payload.updates else {
        return Ok(());
    };
    for update in updates {
        match update {
            FlowUpdateV1::SetCapacity { edge, capacity } => {
                EdgeId::parse(edge)?;
                parse_u64(capacity, "updated capacity")?;
            }
            FlowUpdateV1::AddEdge { edge } => {
                EdgeId::parse(&edge.id)?;
                NodeId::parse(&edge.from)?;
                NodeId::parse(&edge.to)?;
                let lower = parse_u64(&edge.lower, "updated edge lower")?;
                let capacity = parse_u64(&edge.capacity, "updated edge capacity")?;
                if lower > capacity {
                    return Err(FlowScenarioError::Invalid(
                        "updated edge lower exceeds capacity",
                    ));
                }
                parse_i64(&edge.cost, "updated edge cost")?;
                if let Some(initial_flow) = &edge.initial_flow {
                    parse_u64(initial_flow, "updated edge initial flow")?;
                }
            }
            FlowUpdateV1::RemoveEdge { edge } => {
                EdgeId::parse(edge)?;
            }
            FlowUpdateV1::SetTerminals { source, sink } => {
                NodeId::parse(source)?;
                NodeId::parse(sink)?;
                if source == sink {
                    return Err(FlowScenarioError::Invalid("updated source equals sink"));
                }
            }
        }
    }
    Ok(())
}

fn validate_dynamic_eibfs_selection(scenario: &FlowScenarioV1) -> Result<(), FlowScenarioError> {
    if scenario.payload.algorithm.id != "dynamic-eibfs" {
        return Ok(());
    }
    if !matches!(scenario.payload.model, FlowProblemModelV1::MaxFlow { .. }) {
        return Err(FlowScenarioError::Invalid(
            "dynamic EIBFS requires max-flow model",
        ));
    }
    let updates = scenario
        .payload
        .updates
        .as_ref()
        .filter(|updates| !updates.is_empty())
        .ok_or(FlowScenarioError::Invalid(
            "dynamic EIBFS requires capacity updates",
        ))?;
    if updates.len() > DYNAMIC_EIBFS_MAX_UPDATES {
        return Err(FlowScenarioError::Invalid("dynamic EIBFS update limit"));
    }
    if scenario
        .payload
        .graph
        .nodes
        .iter()
        .any(|node| node.supply != "0")
        || scenario.payload.graph.edges.iter().any(|edge| {
            edge.lower != "0" || edge.initial_flow.as_deref().is_some_and(|flow| flow != "0")
        })
    {
        return Err(FlowScenarioError::Invalid(
            "dynamic EIBFS requires zero initial flow",
        ));
    }
    for update in updates {
        let FlowUpdateV1::SetCapacity { edge, capacity } = update else {
            return Err(FlowScenarioError::Invalid(
                "dynamic EIBFS accepts only capacity updates",
            ));
        };
        let declared = scenario
            .payload
            .graph
            .edges
            .iter()
            .find(|candidate| candidate.id == *edge)
            .ok_or(FlowScenarioError::Invalid(
                "dynamic EIBFS update edge is missing",
            ))?;
        if parse_u64(capacity, "updated capacity")? < parse_u64(&declared.lower, "edge lower")? {
            return Err(FlowScenarioError::Invalid(
                "dynamic EIBFS capacity below lower bound",
            ));
        }
    }
    Ok(())
}

fn validate_warm_start_selection(scenario: &FlowScenarioV1) -> Result<(), FlowScenarioError> {
    if scenario.payload.algorithm.id != "warm-start-push-relabel" {
        return Ok(());
    }
    if !matches!(scenario.payload.model, FlowProblemModelV1::MaxFlow { .. }) {
        return Err(FlowScenarioError::Invalid(
            "warm-start push-relabel requires max-flow model",
        ));
    }
    if scenario
        .payload
        .updates
        .as_ref()
        .is_some_and(|items| !items.is_empty())
    {
        return Err(FlowScenarioError::Invalid(
            "warm-start push-relabel does not accept dynamic updates",
        ));
    }
    if scenario
        .payload
        .graph
        .nodes
        .iter()
        .any(|node| node.supply != "0")
        || scenario
            .payload
            .graph
            .edges
            .iter()
            .any(|edge| edge.lower != "0")
    {
        return Err(FlowScenarioError::Invalid(
            "warm-start push-relabel requires zero supplies and lower bounds",
        ));
    }
    Ok(())
}

fn validate_provenance(scenario: &FlowScenarioV1) -> Result<(), FlowScenarioError> {
    let Some(provenance) = &scenario.payload.generator_provenance else {
        return Ok(());
    };
    if !matches!(
        provenance.generator_revision.as_str(),
        "flow-generator/1"
            | "flow-generator/2"
            | "flow-generator/3"
            | "flow-generator/4"
            | "flow-generator/5"
            | "flow-generator/6"
            | "flow-generator/7"
            | "flow-generator/8"
            | "flow-generator/9"
            | "flow-generator/10"
            | "flow-generator/11"
            | "flow-generator/12"
            | "flow-generator/13"
            | "flow-generator/14"
            | "flow-generator/15"
            | "flow-generator/16"
            | "flow-generator/17"
            | "flow-generator/18"
            | "flow-generator/19"
            | "flow-generator/20"
            | "flow-generator/21"
            | "flow-generator/22"
            | "flow-generator/23"
            | "flow-generator/24"
            | "flow-generator/25"
            | "flow-generator/26"
            | "flow-generator/27"
    ) {
        return Err(FlowScenarioError::Unsupported("generator revision"));
    }
    parse_u64(&provenance.seed, "generator seed")?;
    validate_provenance_parameter_identity(provenance)?;
    validate_difficulty_certificate(provenance)?;
    let spec = validate_generator_revision_contract(provenance)?;
    if provenance.materialized_sha256.len() != 64
        || !provenance
            .materialized_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(FlowScenarioError::Invalid("generator digest"));
    }
    let semantic = serde_json::json!({
        "graph": &scenario.payload.graph,
        "suggested_model": &scenario.payload.model,
    });
    let encoded = serde_json::to_vec(&semantic)?;
    let canonical = canonicalize(&encoded)
        .map_err(|_| FlowScenarioError::Invalid("generator digest canonicalization"))?;
    if provenance.materialized_sha256 != sha256_hex(&canonical) {
        return Err(FlowScenarioError::Invalid(
            "generator materialization digest",
        ));
    }
    validate_provenance_regeneration(scenario, provenance, &spec)?;
    Ok(())
}

fn validate_provenance_parameter_identity(
    provenance: &GeneratorProvenanceV1,
) -> Result<(), FlowScenarioError> {
    let revision = provenance
        .parameters
        .get("generator_revision")
        .and_then(serde_json::Value::as_str);
    let seed = provenance
        .parameters
        .get("seed")
        .and_then(serde_json::Value::as_str);
    let family = provenance
        .parameters
        .get("family")
        .and_then(serde_json::Value::as_object);
    let family_id = family
        .and_then(|value| value.get("family_id"))
        .and_then(serde_json::Value::as_str);
    if revision != Some(provenance.generator_revision.as_str())
        || seed != Some(provenance.seed.as_str())
        || family_id != Some(provenance.family_id.as_str())
    {
        return Err(FlowScenarioError::Invalid(
            "generator provenance parameter identity",
        ));
    }
    Ok(())
}

fn validate_provenance_regeneration(
    scenario: &FlowScenarioV1,
    provenance: &GeneratorProvenanceV1,
    spec: &FlowGeneratorSpecV1,
) -> Result<(), FlowScenarioError> {
    // Revisions 1..=23 share one immutable materialization kernel; those
    // revisions only introduced new parameter vocabulary and provenance
    // metadata. Regenerate legacy payloads through that kernel as well so a
    // valid digest cannot be relabeled as a different family/certificate.
    // A future semantic kernel change must branch here instead of silently
    // reinterpreting prior revisions.
    let mut compatibility_spec = spec.clone();
    FLOW_GENERATOR_REVISION.clone_into(&mut compatibility_spec.generator_revision);
    let regenerated = generate_flow_graph(&compatibility_spec)
        .map_err(|_| FlowScenarioError::Invalid("generator provenance regeneration"))?;
    let regenerated_model = if provenance.generator_revision != "flow-generator/24"
        && provenance.generator_revision != "flow-generator/25"
        && provenance.generator_revision != FLOW_GENERATOR_REVISION
        && matches!(
            spec.family,
            FlowGeneratorFamilyV1::PlanarTriangulated { .. }
        ) {
        let FlowGeneratorFamilyV1::PlanarTriangulated { nodes: node_count } = &spec.family else {
            unreachable!("guarded planar compatibility branch");
        };
        FlowProblemModelV1::MaxFlow {
            source: "v0000".to_owned(),
            sink: format!("v{:04}", *node_count - 1),
        }
    } else {
        regenerated.suggested_model.clone()
    };
    let regenerated_semantic = serde_json::to_value((&regenerated.graph, &regenerated_model))?;
    let imported_semantic =
        serde_json::to_value((&scenario.payload.graph, &scenario.payload.model))?;
    if regenerated_semantic != imported_semantic {
        return Err(FlowScenarioError::Invalid(
            "generator provenance regeneration mismatch",
        ));
    }
    if provenance.generator_revision == FLOW_GENERATOR_REVISION {
        let regenerated_provenance = serde_json::to_value(&regenerated.provenance)?;
        let imported_provenance = serde_json::to_value(provenance)?;
        if regenerated_provenance != imported_provenance {
            return Err(FlowScenarioError::Invalid(
                "generator provenance regeneration mismatch",
            ));
        }
    }
    Ok(())
}

fn validate_generator_revision_contract(
    provenance: &GeneratorProvenanceV1,
) -> Result<FlowGeneratorSpecV1, FlowScenarioError> {
    let revision = provenance
        .generator_revision
        .strip_prefix("flow-generator/")
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(FlowScenarioError::Invalid("generator revision"))?;
    let parameters = serde_json::Value::Object(
        provenance
            .parameters
            .clone()
            .into_iter()
            .collect::<serde_json::Map<_, _>>(),
    );
    let spec: FlowGeneratorSpecV1 = serde_json::from_value(parameters)
        .map_err(|_| FlowScenarioError::Invalid("generator provenance parameters"))?;
    if revision < generator_family_introduced_revision(&spec.family) {
        return Err(FlowScenarioError::Invalid(
            "generator family revision contract",
        ));
    }
    if revision < 26 && spec.target_problem.is_some() {
        return Err(FlowScenarioError::Invalid(
            "generator target problem revision contract",
        ));
    }
    if revision < 27
        && matches!(
            spec.target_problem,
            Some(FlowGeneratorTargetProblemV1::FixedFlowMinCost)
        )
    {
        return Err(FlowScenarioError::Invalid(
            "fixed-flow target problem revision contract",
        ));
    }
    if revision == 1
        && (matches!(
            spec.capacity,
            CapacityDistributionV1::Bimodal { .. }
                | CapacityDistributionV1::PowerOfTwoBuckets { .. }
        ) || matches!(
            spec.cost,
            CostDistributionV1::Bimodal { .. } | CostDistributionV1::CapacityCorrelated { .. }
        ))
    {
        return Err(FlowScenarioError::Invalid(
            "generator distribution revision contract",
        ));
    }

    let expected = generator_classification(&spec);
    if provenance.difficulty != expected.difficulty || provenance.source_id != expected.source_id {
        return Err(FlowScenarioError::Invalid(
            "generator classification revision contract",
        ));
    }
    if revision < 8 {
        if !provenance.origin.is_empty()
            || !provenance.sampling.is_empty()
            || !provenance.tags.is_empty()
            || provenance.difficulty_certificate.is_some()
        {
            return Err(FlowScenarioError::Invalid(
                "generator classification revision",
            ));
        }
    } else {
        if provenance.origin != expected.origin
            || provenance.sampling != expected.sampling
            || provenance.tags != expected.tags
        {
            return Err(FlowScenarioError::Invalid(
                "generator classification revision contract",
            ));
        }
        let expected_certificate = difficulty_certificate(&spec.family)
            .map_err(|_| FlowScenarioError::Invalid("difficulty certificate contract"))?;
        if provenance.difficulty_certificate != expected_certificate {
            return Err(FlowScenarioError::Invalid(
                "difficulty certificate revision contract",
            ));
        }
    }
    Ok(spec)
}

fn generator_family_introduced_revision(family: &FlowGeneratorFamilyV1) -> u32 {
    match family {
        FlowGeneratorFamilyV1::RmfgenFrames { .. } => 3,
        FlowGeneratorFamilyV1::GridgenGrid { .. } => 4,
        FlowGeneratorFamilyV1::GotoTorus { .. } => 5,
        FlowGeneratorFamilyV1::NetgenSkeleton { .. } => 6,
        FlowGeneratorFamilyV1::GridgraphGrid { .. } => 7,
        FlowGeneratorFamilyV1::WashingtonRandomLevel { .. } => 8,
        FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { .. } => 9,
        FlowGeneratorFamilyV1::WashingtonCheriyanStress { .. } => 10,
        FlowGeneratorFamilyV1::WashingtonMesh { .. } => 11,
        FlowGeneratorFamilyV1::WashingtonMatching { .. } => 12,
        FlowGeneratorFamilyV1::WashingtonSquareMesh { .. } => 13,
        FlowGeneratorFamilyV1::WashingtonBasicLine { .. }
        | FlowGeneratorFamilyV1::WashingtonExponentialLine { .. }
        | FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine { .. } => 14,
        FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { .. } => 15,
        FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { .. } => 16,
        FlowGeneratorFamilyV1::GloverDenseAcyclicStress { .. } => 17,
        FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid { .. } => 18,
        FlowGeneratorFamilyV1::WaissiTransitOneWayGrid { .. } => 19,
        FlowGeneratorFamilyV1::GoldbergMeshCirculation { .. } => 20,
        FlowGeneratorFamilyV1::AssignmentMatrix { .. } => 22,
        FlowGeneratorFamilyV1::TransportationTable { .. } => 23,
        FlowGeneratorFamilyV1::VisionSegmentationGrid { .. } => 25,
        _ => 1,
    }
}

fn validate_difficulty_certificate(
    provenance: &GeneratorProvenanceV1,
) -> Result<(), FlowScenarioError> {
    let Some(certificate) = &provenance.difficulty_certificate else {
        return Ok(());
    };
    if provenance.difficulty != "verified-worst-case" {
        return Err(FlowScenarioError::Invalid(
            "difficulty certificate classification",
        ));
    }
    if !is_canonical_slug(&certificate.target_algorithm_id)
        || !is_canonical_slug(&certificate.tie_breaking)
        || certificate.exact_metrics.is_empty()
    {
        return Err(FlowScenarioError::Invalid("difficulty certificate"));
    }
    for (metric, value) in &certificate.exact_metrics {
        if !matches!(
            metric.as_str(),
            "augmentations" | "bfs-runs" | "blocking-flow-phases" | "max-flow-value"
        ) {
            return Err(FlowScenarioError::Invalid("difficulty certificate metric"));
        }
        parse_u64(value, "difficulty certificate metric value")?;
    }
    Ok(())
}

fn is_canonical_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn parse_u64(value: &str, field: &'static str) -> Result<u64, FlowScenarioError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(FlowScenarioError::Invalid(field));
    }
    value.parse().map_err(|_| FlowScenarioError::Invalid(field))
}

fn parse_i64(value: &str, field: &'static str) -> Result<i64, FlowScenarioError> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty()
        || (digits.len() > 1 && digits.starts_with('0'))
        || value == "-0"
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(FlowScenarioError::Invalid(field));
    }
    value.parse().map_err(|_| FlowScenarioError::Invalid(field))
}

fn parse_i128(value: &str, field: &'static str) -> Result<i128, FlowScenarioError> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty()
        || (digits.len() > 1 && digits.starts_with('0'))
        || value == "-0"
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(FlowScenarioError::Invalid(field));
    }
    value.parse().map_err(|_| FlowScenarioError::Invalid(field))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::{
        CapacityDistributionV1, CostDistributionV1, FLOW_GENERATOR_REVISION, FlowGeneratorFamilyV1,
        FlowGeneratorSpecV1, generate_flow_graph,
    };

    fn valid_json() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "scenario_encoding_revision": "rfc8785-jcs/1",
            "plugin": "flow",
            "reproducibility": { "declared": {
                "algorithm_revision": ALGORITHM_REVISION,
                "rng_version": RNG_VERSION,
                "plugin_result_revision": PLUGIN_RESULT_REVISION,
                "metrics_catalog_revision": METRICS_CATALOG_REVISION,
                "trace_revision": TRACE_REVISION,
                "projection_revision": PROJECTION_REVISION,
                "layout_revision": LAYOUT_REVISION,
                "frame_encoding_revision": FRAME_ENCODING_REVISION
            }},
            "payload": {
                "model": { "kind": "max-flow", "source": "s", "sink": "t" },
                "graph": {
                    "nodes": [
                        { "id": "t", "supply": "0" },
                        { "id": "s", "supply": "0", "position": { "x": "-10", "y": "0" } }
                    ],
                    "edges": [
                        { "id": "e", "from": "s", "to": "t", "lower": "0", "capacity": "7", "cost": "-2" }
                    ]
                },
                "algorithm": { "id": "edmonds-karp", "config": {} },
                "run_profile": "trace",
                "trace_granularity": "operation",
                "algorithm_seed": "0"
            }
        })
    }

    #[test]
    fn discovery_only_algorithm_terms_are_not_machine_scenario_ids() {
        let mut value = valid_json();
        value["payload"]["algorithm"]["id"] =
            serde_json::json!("Tardos Strongly Polynomial Algorithm");
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Unsupported("algorithm id"))
        ));
    }

    #[test]
    fn display_titles_are_not_machine_scenario_ids() {
        let mut value = valid_json();
        value["payload"]["algorithm"]["id"] = serde_json::json!("Edmonds–Karp");
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Unsupported("algorithm id"))
        ));
    }

    fn generated_grid_scenario_json() -> serde_json::Value {
        let generated = generate_flow_graph(&FlowGeneratorSpecV1 {
            generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
            seed: "7".to_owned(),
            family: FlowGeneratorFamilyV1::Grid2d {
                rows: 3,
                columns: 4,
                diagonals: true,
            },
            capacity: CapacityDistributionV1::Uniform {
                minimum: "1".to_owned(),
                maximum: "9".to_owned(),
            },
            cost: CostDistributionV1::Uniform {
                minimum: "-2".to_owned(),
                maximum: "4".to_owned(),
            },
            target_problem: None,
        })
        .expect("generator materializes");
        let mut value = valid_json();
        value["payload"]["graph"] =
            serde_json::to_value(&generated.graph).expect("graph serializes");
        value["payload"]["model"] =
            serde_json::to_value(&generated.suggested_model).expect("model serializes");
        value["payload"]["generator_provenance"] =
            serde_json::to_value(&generated.provenance).expect("provenance serializes");
        value
    }

    fn generated_transportation_scenario_json() -> serde_json::Value {
        let generated = generate_flow_graph(&FlowGeneratorSpecV1 {
            generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
            seed: "7".to_owned(),
            family: FlowGeneratorFamilyV1::TransportationTable {
                origins: 3,
                destinations: 4,
                total_supply: 12,
                shape: crate::generator::TransportationTableShapeV1::SparseFeasible {
                    density_per_mille: 400,
                    minimum_cost: -2,
                    maximum_cost: 5,
                },
            },
            capacity: CapacityDistributionV1::Unit {},
            cost: CostDistributionV1::Zero {},
            target_problem: None,
        })
        .expect("transportation generator materializes");
        let mut value = valid_json();
        value["payload"]["graph"] =
            serde_json::to_value(&generated.graph).expect("graph serializes");
        value["payload"]["model"] =
            serde_json::to_value(&generated.suggested_model).expect("model serializes");
        value["payload"]["generator_provenance"] =
            serde_json::to_value(&generated.provenance).expect("provenance serializes");
        value["payload"]["algorithm"] =
            serde_json::json!({ "id": "transportation-simplex", "config": {} });
        value
    }

    fn bipartite_matching_json() -> serde_json::Value {
        let mut value = valid_json();
        value["payload"]["model"] = serde_json::json!({
            "kind": "bipartite-matching",
            "left": ["l0", "l1"],
            "right": ["r0", "r1"],
            "flow_adapter": { "source": "s", "sink": "t" }
        });
        value["payload"]["graph"] = serde_json::json!({
            "nodes": [
                { "id": "s", "supply": "0" },
                { "id": "l0", "supply": "0" },
                { "id": "l1", "supply": "0" },
                { "id": "r0", "supply": "0" },
                { "id": "r1", "supply": "0" },
                { "id": "t", "supply": "0" }
            ],
            "edges": [
                { "id": "a0", "from": "s", "to": "l0", "lower": "0", "capacity": "1", "cost": "0" },
                { "id": "a1", "from": "s", "to": "l1", "lower": "0", "capacity": "1", "cost": "0" },
                { "id": "b0", "from": "l0", "to": "r0", "lower": "0", "capacity": "1", "cost": "0" },
                { "id": "b1", "from": "l0", "to": "r1", "lower": "0", "capacity": "1", "cost": "0" },
                { "id": "b2", "from": "l1", "to": "r0", "lower": "0", "capacity": "1", "cost": "0" },
                { "id": "c0", "from": "r0", "to": "t", "lower": "0", "capacity": "1", "cost": "0" },
                { "id": "c1", "from": "r1", "to": "t", "lower": "0", "capacity": "1", "cost": "0" }
            ]
        });
        value["payload"]["algorithm"] = serde_json::json!({
            "id": "hopcroft-karp",
            "config": {}
        });
        value
    }

    fn planar_max_flow_json() -> serde_json::Value {
        let mut value = valid_json();
        value["payload"]["model"] = serde_json::json!({
            "kind": "planar-max-flow",
            "source": "a",
            "sink": "c",
            "embedding": {
                "rotations": [
                    {
                        "node_id": "a",
                        "darts": [
                            { "edge_id": "ab", "direction": "forward" },
                            { "edge_id": "ac", "direction": "forward" }
                        ]
                    },
                    {
                        "node_id": "b",
                        "darts": [
                            { "edge_id": "ab", "direction": "reverse" },
                            { "edge_id": "bc", "direction": "forward" }
                        ]
                    },
                    {
                        "node_id": "c",
                        "darts": [
                            { "edge_id": "bc", "direction": "reverse" },
                            { "edge_id": "ac", "direction": "reverse" }
                        ]
                    }
                ],
                "outer_face": { "edge_id": "ab", "direction": "reverse" },
                "terminal_corners": {
                    "source": { "edge_id": "ac", "direction": "forward" },
                    "sink": { "edge_id": "bc", "direction": "reverse" }
                }
            }
        });
        value["payload"]["graph"] = serde_json::json!({
            "nodes": [{ "id": "a" }, { "id": "b" }, { "id": "c" }],
            "edges": [
                { "id": "ab", "from": "a", "to": "b", "capacity": "4" },
                { "id": "ac", "from": "a", "to": "c", "capacity": "2" },
                { "id": "bc", "from": "b", "to": "c", "capacity": "3" }
            ]
        });
        value["payload"]["algorithm"] =
            serde_json::json!({ "id": "hassin-st-planar", "config": {} });
        value
    }

    #[test]
    fn strict_flow_decode_builds_a_canonical_graph() {
        let scenario = decode_flow_scenario(valid_json().to_string().as_bytes())
            .expect("fixture Scenario is valid");
        let graph = scenario
            .canonical_network()
            .expect("fixture graph resolves");

        assert_eq!(graph.nodes()[0].id().as_str(), "s");
        assert_eq!(graph.edges()[0].cost(), -2);
    }

    #[test]
    fn dynamic_eibfs_admits_only_bounded_capacity_update_sequences() {
        let mut valid = valid_json();
        valid["payload"]["algorithm"] = serde_json::json!({ "id": "dynamic-eibfs", "config": {} });
        valid["payload"]["updates"] = serde_json::json!([
            { "kind": "set-capacity", "edge": "e", "capacity": "4" }
        ]);
        decode_flow_scenario(valid.to_string().as_bytes())
            .expect("a bounded capacity-only Dynamic EIBFS Scenario is valid");

        let mut missing_updates = valid.clone();
        missing_updates["payload"]
            .as_object_mut()
            .expect("payload object")
            .remove("updates");
        assert!(matches!(
            decode_flow_scenario(missing_updates.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "dynamic EIBFS requires capacity updates"
            ))
        ));

        let mut structural = valid.clone();
        structural["payload"]["updates"] = serde_json::json!([
            { "kind": "remove-edge", "edge": "e" }
        ]);
        assert!(matches!(
            decode_flow_scenario(structural.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "dynamic EIBFS accepts only capacity updates"
            ))
        ));

        let mut missing_edge = valid.clone();
        missing_edge["payload"]["updates"][0]["edge"] = serde_json::json!("missing");
        assert!(matches!(
            decode_flow_scenario(missing_edge.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "dynamic EIBFS update edge is missing"
            ))
        ));

        let mut too_many = valid.clone();
        too_many["payload"]["updates"] = serde_json::Value::Array(
            (0..=DYNAMIC_EIBFS_MAX_UPDATES)
                .map(|_| {
                    serde_json::json!({
                        "kind": "set-capacity",
                        "edge": "e",
                        "capacity": "4"
                    })
                })
                .collect(),
        );
        assert!(matches!(
            decode_flow_scenario(too_many.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid("dynamic EIBFS update limit"))
        ));

        let mut wrong_model = valid.clone();
        wrong_model["payload"]["model"] = serde_json::json!({
            "kind": "min-cost-max-flow",
            "source": "s",
            "sink": "t"
        });
        assert!(matches!(
            decode_flow_scenario(wrong_model.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "dynamic EIBFS requires max-flow model"
            ))
        ));
    }

    #[test]
    fn bipartite_matching_model_round_trips_with_an_exact_unit_adapter() {
        let scenario = decode_flow_scenario(bipartite_matching_json().to_string().as_bytes())
            .expect("matching Scenario is valid");
        assert!(matches!(
            scenario.payload.model,
            FlowProblemModelV1::BipartiteMatching {
                ref left,
                ref right,
                flow_adapter: Some(_)
            } if left == &["l0", "l1"] && right == &["r0", "r1"]
        ));
    }

    #[test]
    fn planar_model_requires_a_complete_genus_zero_rotation_system() {
        let value = planar_max_flow_json();
        let scenario =
            decode_flow_scenario(value.to_string().as_bytes()).expect("embedded triangle is valid");
        assert!(matches!(
            scenario.payload.model,
            FlowProblemModelV1::PlanarMaxFlow {
                ref source,
                ref sink,
                ref embedding,
            } if source == "a" && sink == "c" && embedding.rotations.len() == 3
        ));

        let mut wrong_rotation = value.clone();
        wrong_rotation["payload"]["model"]["embedding"]["rotations"][0]["node_id"] =
            serde_json::json!("b");
        assert!(matches!(
            decode_flow_scenario(wrong_rotation.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid("planar embedding"))
        ));

        let mut lower_bound = value.clone();
        lower_bound["payload"]["graph"]["edges"][0]["lower"] = serde_json::json!("1");
        assert!(matches!(
            decode_flow_scenario(lower_bound.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "planar max-flow requires zero supplies and lower bounds"
            ))
        ));

        let mut future_field = value;
        future_field["payload"]["model"]["embedding"]["future"] = serde_json::json!(true);
        assert!(decode_flow_scenario(future_field.to_string().as_bytes()).is_err());
    }

    #[test]
    fn warm_start_push_relabel_accepts_bounded_predictions_but_rejects_other_state() {
        let mut valid = valid_json();
        valid["payload"]["algorithm"] =
            serde_json::json!({ "id": "warm-start-push-relabel", "config": {} });
        valid["payload"]["graph"]["edges"][0]["initial_flow"] = serde_json::json!("6");
        decode_flow_scenario(valid.to_string().as_bytes())
            .expect("capacity-bounded predicted pseudoflow is valid");

        let mut wrong_model = valid.clone();
        wrong_model["payload"]["model"] = serde_json::json!({
            "kind": "min-cost-max-flow",
            "source": "s",
            "sink": "t"
        });
        assert!(matches!(
            decode_flow_scenario(wrong_model.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "warm-start push-relabel requires max-flow model"
            ))
        ));

        let mut lower_bound = valid.clone();
        lower_bound["payload"]["graph"]["edges"][0]["lower"] = serde_json::json!("1");
        assert!(matches!(
            decode_flow_scenario(lower_bound.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "warm-start push-relabel requires zero supplies and lower bounds"
            ))
        ));

        let mut update = valid;
        update["payload"]["updates"] = serde_json::json!([{
            "kind": "set-capacity",
            "edge": "e",
            "capacity": "6"
        }]);
        assert!(matches!(
            decode_flow_scenario(update.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "warm-start push-relabel does not accept dynamic updates"
            ))
        ));
    }

    #[test]
    fn bipartite_matching_model_rejects_bad_partitions_and_initial_flow() {
        let mut noncanonical = bipartite_matching_json();
        noncanonical["payload"]["model"]["left"] = serde_json::json!(["l1", "l0"]);
        assert!(matches!(
            decode_flow_scenario(noncanonical.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid("bipartite matching model"))
        ));

        let mut initial = bipartite_matching_json();
        initial["payload"]["graph"]["edges"][2]["initial_flow"] = serde_json::json!("1");
        assert!(matches!(
            decode_flow_scenario(initial.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "bipartite matching initial flow must be zero"
            ))
        ));

        let mut malformed_adapter = bipartite_matching_json();
        malformed_adapter["payload"]["model"]["flow_adapter"]["future"] = serde_json::json!(true);
        assert!(decode_flow_scenario(malformed_adapter.to_string().as_bytes()).is_err());
    }

    #[test]
    fn assignment_model_accepts_rectangular_sparse_graph_and_rejects_drift() {
        let mut value = valid_json();
        value["payload"]["model"] = serde_json::json!({
            "kind": "assignment",
            "agents": ["a0", "a1"],
            "tasks": ["t0", "t1", "t2"],
            "objective": "maximize"
        });
        value["payload"]["graph"] = serde_json::json!({
            "nodes": [
                { "id": "a0" }, { "id": "a1" },
                { "id": "t0" }, { "id": "t1" }, { "id": "t2" }
            ],
            "edges": [
                { "id": "e00", "from": "a0", "to": "t0", "capacity": "1", "cost": "-4" },
                { "id": "e01", "from": "a0", "to": "t1", "capacity": "1", "cost": "8" },
                { "id": "e10", "from": "a1", "to": "t0", "capacity": "1", "cost": "3" },
                { "id": "e12", "from": "a1", "to": "t2", "capacity": "1", "cost": "7" }
            ]
        });
        value["payload"]["algorithm"] = serde_json::json!({ "id": "hungarian", "config": {} });
        let scenario = decode_flow_scenario(value.to_string().as_bytes())
            .expect("rectangular assignment is valid");
        assert!(matches!(
            scenario.payload.model,
            FlowProblemModelV1::Assignment {
                ref agents,
                ref tasks,
                objective: AssignmentObjectiveV1::Maximize,
            } if agents == &["a0", "a1"] && tasks == &["t0", "t1", "t2"]
        ));

        let mut noncanonical = value.clone();
        noncanonical["payload"]["model"]["agents"] = serde_json::json!(["a1", "a0"]);
        assert!(matches!(
            decode_flow_scenario(noncanonical.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid("assignment model"))
        ));
        value["payload"]["graph"]["edges"][0]["initial_flow"] = serde_json::json!("1");
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "assignment initial flow must be zero"
            ))
        ));
    }

    #[test]
    fn oversized_assignment_is_semantically_validated_without_dense_pair_allocation() {
        let mut value = valid_json();
        let agents = (0..1_000)
            .map(|index| format!("a{index:04}"))
            .collect::<Vec<_>>();
        let tasks = (0..1_001)
            .map(|index| format!("t{index:04}"))
            .collect::<Vec<_>>();
        value["payload"]["model"] = serde_json::json!({
            "kind": "assignment",
            "agents": agents,
            "tasks": tasks,
            "objective": "minimize"
        });
        value["payload"]["graph"] = serde_json::json!({
            "nodes": agents
                .iter()
                .chain(&tasks)
                .map(|id| serde_json::json!({ "id": id }))
                .collect::<Vec<_>>(),
            "edges": []
        });
        value["payload"]["algorithm"] = serde_json::json!({ "id": "auction", "config": {} });
        let scenario = decode_flow_scenario(value.to_string().as_bytes())
            .expect("oversized semantic declaration avoids the dense executable index");
        assert_eq!(scenario.payload.graph.nodes.len(), 2_001);
        assert!(scenario.payload.graph.edges.is_empty());
    }

    #[test]
    fn transportation_model_accepts_balanced_sparse_routes_and_rejects_contract_drift() {
        let mut value = valid_json();
        value["payload"]["model"] = serde_json::json!({
            "kind": "transportation",
            "origins": ["o0", "o1"],
            "destinations": ["d0", "d1"]
        });
        value["payload"]["graph"] = serde_json::json!({
            "nodes": [
                { "id": "d0", "supply": "-2" },
                { "id": "d1", "supply": "-3" },
                { "id": "o0", "supply": "3" },
                { "id": "o1", "supply": "2" }
            ],
            "edges": [
                { "id": "e00", "from": "o0", "to": "d0", "lower": "0", "capacity": "2", "cost": "4" },
                { "id": "e01", "from": "o0", "to": "d1", "lower": "0", "capacity": "3", "cost": "1" },
                { "id": "e11", "from": "o1", "to": "d1", "lower": "0", "capacity": "2", "cost": "3" }
            ]
        });
        value["payload"]["algorithm"] =
            serde_json::json!({ "id": "transportation-simplex", "config": {} });

        let scenario = decode_flow_scenario(value.to_string().as_bytes())
            .expect("balanced sparse transportation table is valid");
        assert!(matches!(
            scenario.payload.model,
            FlowProblemModelV1::Transportation {
                ref origins,
                ref destinations,
            } if origins == &["o0", "o1"] && destinations == &["d0", "d1"]
        ));

        let mut unbalanced = value.clone();
        unbalanced["payload"]["graph"]["nodes"][1]["supply"] = serde_json::json!("-4");
        assert!(matches!(
            decode_flow_scenario(unbalanced.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid("transportation model"))
        ));

        let mut binding_capacity = value.clone();
        binding_capacity["payload"]["graph"]["edges"][0]["capacity"] = serde_json::json!("1");
        assert!(matches!(
            decode_flow_scenario(binding_capacity.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid("transportation model"))
        ));

        let mut initial = value;
        initial["payload"]["graph"]["edges"][0]["initial_flow"] = serde_json::json!("1");
        assert!(matches!(
            decode_flow_scenario(initial.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "transportation initial flow must be zero"
            ))
        ));
    }

    #[test]
    fn unknown_payload_fields_and_noncanonical_numbers_are_rejected() {
        let mut unknown = valid_json();
        unknown["payload"]["future"] = serde_json::json!(true);
        assert!(decode_flow_scenario(unknown.to_string().as_bytes()).is_err());

        let mut noncanonical = valid_json();
        noncanonical["payload"]["graph"]["edges"][0]["capacity"] = serde_json::json!("07");
        assert!(matches!(
            decode_flow_scenario(noncanonical.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid("edge capacity"))
        ));
    }

    #[test]
    fn invalid_candidate_never_returns_a_partial_network() {
        let mut value = valid_json();
        value["payload"]["graph"]["edges"][0]["to"] = serde_json::json!("missing");

        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Model(FlowModelError::DanglingEndpoint))
        ));
    }

    #[test]
    fn initial_flow_and_terminal_bounds_are_checked_before_execution() {
        let mut value = valid_json();
        value["payload"]["graph"]["edges"][0]["initial_flow"] = serde_json::json!("8");
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid("edge initial flow bounds"))
        ));

        let mut same_terminal = valid_json();
        same_terminal["payload"]["model"]["sink"] = serde_json::json!("s");
        assert!(matches!(
            decode_flow_scenario(same_terminal.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid("source equals sink"))
        ));
    }

    #[test]
    fn terminal_flow_models_reject_node_supplies_before_execution() {
        for kind in ["max-flow", "min-cost-max-flow"] {
            let mut value = valid_json();
            value["payload"]["model"]["kind"] = serde_json::json!(kind);
            value["payload"]["graph"]["nodes"][0]["supply"] = serde_json::json!("1");
            value["payload"]["graph"]["nodes"][1]["supply"] = serde_json::json!("-1");
            assert!(matches!(
                decode_flow_scenario(value.to_string().as_bytes()),
                Err(FlowScenarioError::Invalid(
                    "terminal-flow models require zero node supplies"
                ))
            ));
        }
    }

    #[test]
    fn materialized_generator_digest_survives_round_trip_and_rejects_graph_edits() {
        let mut value = generated_grid_scenario_json();
        decode_flow_scenario(value.to_string().as_bytes())
            .expect("untouched generated Scenario validates");

        let current_provenance = value["payload"]["generator_provenance"].clone();
        for revision in [1, 7] {
            value["payload"]["generator_provenance"] = current_provenance.clone();
            let legacy = value["payload"]["generator_provenance"]
                .as_object_mut()
                .expect("generator provenance is an object");
            legacy.remove("origin");
            legacy.remove("sampling");
            legacy.remove("tags");
            let revision = format!("flow-generator/{revision}");
            value["payload"]["generator_provenance"]["generator_revision"] =
                serde_json::json!(&revision);
            value["payload"]["generator_provenance"]["parameters"]["generator_revision"] =
                serde_json::json!(&revision);
            decode_flow_scenario(value.to_string().as_bytes())
                .expect("bound legacy Grid2d provenance remains importable");
        }
        for revision in [8, 19] {
            value["payload"]["generator_provenance"] = current_provenance.clone();
            let revision = format!("flow-generator/{revision}");
            value["payload"]["generator_provenance"]["generator_revision"] =
                serde_json::json!(&revision);
            value["payload"]["generator_provenance"]["parameters"]["generator_revision"] =
                serde_json::json!(&revision);
            decode_flow_scenario(value.to_string().as_bytes())
                .expect("bound four-axis Grid2d provenance remains importable");
        }
        value["payload"]["generator_provenance"] = current_provenance.clone();
        value["payload"]["generator_provenance"]["generator_revision"] =
            serde_json::json!("flow-generator/19");
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "generator provenance parameter identity"
            ))
        ));
        value["payload"]["generator_provenance"] = current_provenance.clone();
        value["payload"]["generator_provenance"]["generator_revision"] =
            serde_json::json!("flow-generator/28");
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Unsupported("generator revision"))
        ));
        value["payload"]["generator_provenance"] = current_provenance.clone();
        value["payload"]["graph"]["edges"][0]["capacity"] = serde_json::json!("10");
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "generator materialization digest"
            ))
        ));
    }

    #[test]
    fn planar_generator_revision_preserves_legacy_model_semantics() {
        let generated = generate_flow_graph(&FlowGeneratorSpecV1 {
            generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
            seed: "17".to_owned(),
            family: FlowGeneratorFamilyV1::PlanarTriangulated { nodes: 7 },
            capacity: CapacityDistributionV1::Uniform {
                minimum: "1".to_owned(),
                maximum: "9".to_owned(),
            },
            cost: CostDistributionV1::Zero {},
            target_problem: None,
        })
        .expect("planar generator materializes");
        let legacy_model = FlowProblemModelV1::MaxFlow {
            source: "v0000".to_owned(),
            sink: "v0006".to_owned(),
        };
        let legacy_semantic = serde_json::json!({
            "graph": &generated.graph,
            "suggested_model": &legacy_model,
        });
        let legacy_canonical = canonicalize(
            &serde_json::to_vec(&legacy_semantic).expect("legacy semantic serializes"),
        )
        .expect("legacy semantic canonicalizes");

        let mut revision_24 = valid_json();
        revision_24["payload"]["graph"] =
            serde_json::to_value(&generated.graph).expect("graph serializes");
        revision_24["payload"]["model"] =
            serde_json::to_value(&generated.suggested_model).expect("model serializes");
        revision_24["payload"]["algorithm"] =
            serde_json::json!({ "id": "hassin-st-planar", "config": {} });
        revision_24["payload"]["generator_provenance"] =
            serde_json::to_value(&generated.provenance).expect("provenance serializes");
        revision_24["payload"]["generator_provenance"]["generator_revision"] =
            serde_json::json!("flow-generator/24");
        revision_24["payload"]["generator_provenance"]["parameters"]["generator_revision"] =
            serde_json::json!("flow-generator/24");
        decode_flow_scenario(revision_24.to_string().as_bytes())
            .expect("revision 24 retains the native planar model after revision 25");

        let mut legacy = valid_json();
        legacy["payload"]["graph"] =
            serde_json::to_value(&generated.graph).expect("graph serializes");
        legacy["payload"]["model"] = serde_json::to_value(&legacy_model).expect("model serializes");
        legacy["payload"]["generator_provenance"] =
            serde_json::to_value(&generated.provenance).expect("provenance serializes");
        legacy["payload"]["generator_provenance"]["generator_revision"] =
            serde_json::json!("flow-generator/23");
        legacy["payload"]["generator_provenance"]["parameters"]["generator_revision"] =
            serde_json::json!("flow-generator/23");
        legacy["payload"]["generator_provenance"]["materialized_sha256"] =
            serde_json::json!(sha256_hex(&legacy_canonical));
        decode_flow_scenario(legacy.to_string().as_bytes())
            .expect("revision 23 planar max-flow model remains importable");

        legacy["payload"]["generator_provenance"]["generator_revision"] =
            serde_json::json!(FLOW_GENERATOR_REVISION);
        legacy["payload"]["generator_provenance"]["parameters"]["generator_revision"] =
            serde_json::json!(FLOW_GENERATOR_REVISION);
        assert!(matches!(
            decode_flow_scenario(legacy.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "generator provenance regeneration mismatch"
            ))
        ));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn generator_revision_contract_rejects_downgrades_and_classification_forgery() {
        let mut transportation = generated_transportation_scenario_json();
        transportation["payload"]["generator_provenance"]["generator_revision"] =
            serde_json::json!("flow-generator/22");
        transportation["payload"]["generator_provenance"]["parameters"]["generator_revision"] =
            serde_json::json!("flow-generator/22");
        assert!(matches!(
            decode_flow_scenario(transportation.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "generator family revision contract"
            ))
        ));

        let generated_vision = generate_flow_graph(&FlowGeneratorSpecV1 {
            generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
            seed: "7".to_owned(),
            family: FlowGeneratorFamilyV1::VisionSegmentationGrid {
                rows: 3,
                columns: 4,
                eight_neighbor: true,
            },
            capacity: CapacityDistributionV1::Uniform {
                minimum: "1".to_owned(),
                maximum: "9".to_owned(),
            },
            cost: CostDistributionV1::Zero {},
            target_problem: None,
        })
        .expect("vision family materializes");
        let mut vision = valid_json();
        vision["payload"]["graph"] =
            serde_json::to_value(&generated_vision.graph).expect("graph serializes");
        vision["payload"]["model"] =
            serde_json::to_value(&generated_vision.suggested_model).expect("model serializes");
        vision["payload"]["algorithm"] =
            serde_json::json!({ "id": "boykov-kolmogorov", "config": {} });
        vision["payload"]["generator_provenance"] =
            serde_json::to_value(&generated_vision.provenance).expect("provenance serializes");
        vision["payload"]["generator_provenance"]["generator_revision"] =
            serde_json::json!("flow-generator/24");
        vision["payload"]["generator_provenance"]["parameters"]["generator_revision"] =
            serde_json::json!("flow-generator/24");
        assert!(matches!(
            decode_flow_scenario(vision.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "generator family revision contract"
            ))
        ));

        let generated_fixed_flow = generate_flow_graph(&FlowGeneratorSpecV1 {
            generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
            seed: "7".to_owned(),
            family: FlowGeneratorFamilyV1::Grid2d {
                rows: 2,
                columns: 3,
                diagonals: false,
            },
            capacity: CapacityDistributionV1::Uniform {
                minimum: "1".to_owned(),
                maximum: "9".to_owned(),
            },
            cost: CostDistributionV1::Uniform {
                minimum: "-2".to_owned(),
                maximum: "3".to_owned(),
            },
            target_problem: Some(FlowGeneratorTargetProblemV1::FixedFlowMinCost),
        })
        .expect("fixed-flow generator materializes");
        let mut downgraded_fixed_flow = valid_json();
        downgraded_fixed_flow["payload"]["graph"] =
            serde_json::to_value(&generated_fixed_flow.graph).expect("graph serializes");
        downgraded_fixed_flow["payload"]["model"] =
            serde_json::to_value(&generated_fixed_flow.suggested_model).expect("model serializes");
        downgraded_fixed_flow["payload"]["algorithm"] =
            serde_json::json!({ "id": "successive-shortest-path", "config": {} });
        downgraded_fixed_flow["payload"]["generator_provenance"] =
            serde_json::to_value(&generated_fixed_flow.provenance).expect("provenance serializes");
        downgraded_fixed_flow["payload"]["generator_provenance"]["generator_revision"] =
            serde_json::json!("flow-generator/26");
        downgraded_fixed_flow["payload"]["generator_provenance"]["parameters"]["generator_revision"] =
            serde_json::json!("flow-generator/26");
        assert!(matches!(
            decode_flow_scenario(downgraded_fixed_flow.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "fixed-flow target problem revision contract"
            ))
        ));

        let mut value = generated_grid_scenario_json();
        let current_provenance = value["payload"]["generator_provenance"].clone();
        value["payload"]["generator_provenance"]["generator_revision"] =
            serde_json::json!("flow-generator/19");
        value["payload"]["generator_provenance"]["parameters"]["generator_revision"] =
            serde_json::json!("flow-generator/19");
        value["payload"]["generator_provenance"]["parameters"]["family"] = serde_json::json!({
            "family_id": "goldberg-mesh-circulation",
            "columns": 4,
            "rows": 3,
            "horizontal_degree": 1,
            "vertical_degree": 1
        });
        value["payload"]["generator_provenance"]["family_id"] =
            serde_json::json!("goldberg-mesh-circulation");
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "generator family revision contract"
            ))
        ));

        value["payload"]["generator_provenance"] = current_provenance.clone();
        value["payload"]["generator_provenance"]["source_id"] = serde_json::json!("forged-source");
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "generator classification revision contract"
            ))
        ));
        value["payload"]["generator_provenance"] = current_provenance.clone();
        value["payload"]["generator_provenance"]["family_id"] = serde_json::json!("forged-family");
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "generator provenance parameter identity"
            ))
        ));
        value["payload"]["generator_provenance"] = current_provenance.clone();
        value["payload"]["generator_provenance"]["difficulty"] =
            serde_json::json!("verified-worst-case");
        value["payload"]["generator_provenance"]["difficulty_certificate"] = serde_json::json!({
            "target_algorithm_id": "dinic",
            "tie_breaking": "edge-order",
            "exact_metrics": { "blocking-flow-phases": "1" }
        });
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "generator classification revision contract"
            ))
        ));

        let certified = generate_flow_graph(&FlowGeneratorSpecV1 {
            generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
            seed: "7".to_owned(),
            family: FlowGeneratorFamilyV1::DinicWorstCase { nodes: 8 },
            capacity: CapacityDistributionV1::Unit {},
            cost: CostDistributionV1::Zero {},
            target_problem: None,
        })
        .expect("certified family materializes");
        let grid_digest = current_provenance["materialized_sha256"]
            .as_str()
            .expect("grid digest")
            .to_owned();
        value["payload"]["generator_provenance"] =
            serde_json::to_value(certified.provenance).expect("provenance serializes");
        value["payload"]["generator_provenance"]["generator_revision"] =
            serde_json::json!("flow-generator/19");
        value["payload"]["generator_provenance"]["parameters"]["generator_revision"] =
            serde_json::json!("flow-generator/19");
        value["payload"]["generator_provenance"]["materialized_sha256"] =
            serde_json::json!(grid_digest);
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "generator provenance regeneration mismatch"
            ))
        ));
    }

    #[test]
    fn difficulty_certificate_is_strict_and_bound_to_worst_case_provenance() {
        let generated = generate_flow_graph(&FlowGeneratorSpecV1 {
            generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
            seed: "42".to_owned(),
            family: FlowGeneratorFamilyV1::DinicWorstCase { nodes: 8 },
            capacity: CapacityDistributionV1::Unit {},
            cost: CostDistributionV1::Zero {},
            target_problem: None,
        })
        .expect("worst-case generator materializes");
        let mut value = valid_json();
        value["payload"]["graph"] =
            serde_json::to_value(&generated.graph).expect("graph serializes");
        value["payload"]["model"] =
            serde_json::to_value(&generated.suggested_model).expect("model serializes");
        value["payload"]["generator_provenance"] =
            serde_json::to_value(&generated.provenance).expect("provenance serializes");
        decode_flow_scenario(value.to_string().as_bytes())
            .expect("exact difficulty certificate validates");

        let mut noncanonical = value.clone();
        noncanonical["payload"]["generator_provenance"]["difficulty_certificate"]["exact_metrics"]
            ["augmentations"] = serde_json::json!("0128");
        assert!(matches!(
            decode_flow_scenario(noncanonical.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "difficulty certificate metric value"
            ))
        ));

        let mut unknown_metric = value.clone();
        unknown_metric["payload"]["generator_provenance"]["difficulty_certificate"]["exact_metrics"]
            ["future-work"] = serde_json::json!("1");
        assert!(matches!(
            decode_flow_scenario(unknown_metric.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid("difficulty certificate metric"))
        ));

        value["payload"]["generator_provenance"]["difficulty"] = serde_json::json!("stress");
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "difficulty certificate classification"
            ))
        ));

        let mut missing = value;
        missing["payload"]["generator_provenance"]["difficulty"] =
            serde_json::json!("verified-worst-case");
        missing["payload"]["generator_provenance"]
            .as_object_mut()
            .expect("provenance object")
            .remove("difficulty_certificate");
        assert!(matches!(
            decode_flow_scenario(missing.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid(
                "difficulty certificate revision contract"
            ))
        ));
    }

    #[test]
    fn convex_cost_model_validates_exact_segments_and_builds_native_problem() {
        let mut value = valid_json();
        value["payload"]["model"] = serde_json::json!({ "kind": "convex-cost-flow" });
        value["payload"]["algorithm"]["id"] = serde_json::json!("segment-expanded-convex-mcf");
        value["payload"]["graph"]["nodes"][0]["supply"] = serde_json::json!("-2");
        value["payload"]["graph"]["nodes"][1]["supply"] = serde_json::json!("2");
        value["payload"]["graph"]["edges"][0]["cost"] = serde_json::json!("0");
        value["payload"]["graph"]["edges"][0]["capacity"] = serde_json::json!("3");
        value["payload"]["graph"]["edges"][0]["convex_cost"] = serde_json::json!({
            "base_cost_at_zero": "-4",
            "segments": [
                { "end_flow": "1", "marginal_cost": "2" },
                { "end_flow": "3", "marginal_cost": "5" }
            ]
        });
        let scenario = decode_flow_scenario(value.to_string().as_bytes())
            .expect("exact convex-cost scenario validates");
        let network = scenario.canonical_network().expect("network is canonical");
        let problem = scenario
            .convex_cost_problem(&network)
            .expect("native convex problem builds");
        let result = crate::solve_segment_expanded_convex_cost(&problem)
            .expect("expanded oracle solves the native problem");
        assert_eq!(result.flows, vec![2]);
        assert_eq!(result.certificate.total_cost, 3);

        let mut nonconvex = value.clone();
        nonconvex["payload"]["graph"]["edges"][0]["convex_cost"]["segments"][1]["marginal_cost"] =
            serde_json::json!("1");
        assert!(matches!(
            decode_flow_scenario(nonconvex.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid("convex-cost flow model"))
        ));

        value["payload"]["graph"]["edges"][0]["convex_cost"]["base_cost_at_zero"] =
            serde_json::json!("-0");
        assert!(matches!(
            decode_flow_scenario(value.to_string().as_bytes()),
            Err(FlowScenarioError::Invalid("convex base cost"))
        ));
    }
}
