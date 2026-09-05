//! Canonical generator presets and algorithm-compatibility metadata.
//!
//! This module is the single machine-readable contract shared by the generator
//! picker, quality matrix, and algorithm-oriented examples.  A fixture is not
//! an asymptotic-performance claim: only an explicit strict certificate may be
//! presented as a verified worst case.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::assignment::AssignmentObjectiveV1;
use crate::catalog::{CatalogModelKind, GraphRequirement, ImplementationStatus, algorithm_catalog};
use crate::generator::{
    AssignmentMatrixShapeV1, CapacityDistributionV1, CostDistributionV1, FLOW_GENERATOR_FAMILY_IDS,
    FLOW_GENERATOR_REVISION, FlowGeneratorFamilyV1, FlowGeneratorSpecV1,
    TransportationTableShapeV1, generator_classification, generator_family_is_randomized,
};

/// Stable layout class used by the visual-regression matrix.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratorLayoutClassV1 {
    /// Long paths, layered DAGs, and left-to-right gadgets.
    LinearLayered,
    /// Circular and radial structures.
    RadialCyclic,
    /// Local rectangular or volumetric grids.
    GridLocal,
    /// Periodic grids and toroidal meshes.
    GridPeriodic,
    /// Two-part tables, matching graphs, and terminal partitions.
    Partitioned,
    /// Trees and multi-level branching structures.
    Hierarchical,
    /// Community structure and hub-heavy random graphs.
    Clustered,
    /// Dense or geometric graphs whose edges dominate the view.
    DenseSpatial,
    /// Source-specific benchmark gadgets needing a dedicated layout audit.
    BenchmarkGadget,
}

/// Stable high-level group shown by the generator picker.
///
/// This is intentionally independent from provenance `sampling`: a fixed
/// topology with randomized capacities remains a structural family.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratorPickerGroupV1 {
    /// Recognizable path, tree, grid, layered, or cyclic topology.
    Structural,
    /// Topology itself is sampled from a random-graph model.
    Random,
    /// Assignment, transportation, planar, matching, or vision-specific model.
    Special,
    /// Construction derived from a published benchmark generator.
    Benchmark,
    /// Finite stress construction without a strict asymptotic certificate.
    Stress,
    /// Source-backed worst case with an exact difficulty certificate.
    WorstCase,
}

/// Exact suggested-model class of all canonical presets for one fixture.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratorModelKindV1 {
    /// Single-source, single-sink maximum flow.
    MaxFlow,
    /// Linear-cost circulation with no required terminal flow.
    Circulation,
    /// Balanced node-supply transshipment.
    Transshipment,
    /// Maximum-cardinality bipartite matching.
    BipartiteMatching,
    /// Rectangular minimum- or maximum-cost assignment.
    Assignment,
    /// Balanced transportation table.
    Transportation,
    /// Maximum flow with an explicit combinatorial planar embedding.
    PlanarMaxFlow,
}

/// Intended preset use.  Boundary means the practical UI/test boundary, not a
/// global graph-size maximum and not universal admission by every algorithm.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratorPresetPurposeV1 {
    /// Small graph intended for reversible operation-level traces.
    Trace,
    /// Medium graph intended for result-only algorithm comparisons.
    Fast,
    /// Largest canonical graph kept in routine browser and regression QA.
    Boundary,
}

/// Recommended execution mode for one generator preset.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratorPresetRunProfileV1 {
    /// Record reversible pedagogical events.
    Trace,
    /// Compute the result, certificate, and metrics without an event history.
    Fast,
}

/// Evidence level attached to an expected finite-size counter.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratorCounterEvidenceV1 {
    /// Repeated by the strict generator difficulty certificate.
    StrictCertificate,
    /// Exact finite-size regression, with no asymptotic worst-case claim.
    FiniteRegression,
    /// Exact consequence of the generated graph construction.
    StructuralIdentity,
}

/// One exact metric expected from a named algorithm on a preset.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GeneratorExpectedCounterV1 {
    /// Catalog descriptor whose metric is constrained.
    pub algorithm_id: String,
    /// Stable metric identifier in the algorithm result.
    pub metric_id: String,
    /// Exact canonical integer expected from the finite preset.
    pub exact_value: String,
    /// Strength and interpretation of the expectation.
    pub evidence: GeneratorCounterEvidenceV1,
}

/// One materialized generator request used by teaching, smoke, or boundary QA.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GeneratorPresetV1 {
    /// Teaching, routine comparison, or practical-boundary role.
    pub purpose: GeneratorPresetPurposeV1,
    /// Localized short display label.
    pub label: String,
    /// Default execution profile for algorithms selected with this preset.
    pub recommended_run_profile: GeneratorPresetRunProfileV1,
    /// Complete deterministic generator request.
    pub spec: FlowGeneratorSpecV1,
    /// True only when generation must emit a strict difficulty certificate.
    pub expects_strict_difficulty_certificate: bool,
    /// Exact finite metrics that quality gates may verify.
    pub expected_counters: Vec<GeneratorExpectedCounterV1>,
}

/// Relationship between one generator fixture and one catalog descriptor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratorAlgorithmCompatibilityStateV1 {
    /// Structurally compatible and particularly useful for this shape.
    Recommended,
    /// Structurally compatible; preset-specific admission still applies.
    Compatible,
    /// Rejected by publication status, model, or a required graph property.
    Incompatible,
}

/// Machine-readable compatibility record.  Admission bands and work ceilings
/// remain preset- and runtime-specific and are intentionally not hidden here.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GeneratorAlgorithmCompatibilityV1 {
    /// Canonical catalog descriptor ID.
    pub algorithm_id: String,
    /// Structural relationship to the canonical family presets.
    pub state: GeneratorAlgorithmCompatibilityStateV1,
    /// Explicit user-facing explanation of the classification.
    pub reason: String,
}

/// Canonical contract for one of the 50 generator families.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GeneratorAlgorithmFixtureV1 {
    /// Canonical generator family ID.
    pub family_id: String,
    /// Human-readable family name.
    pub title: String,
    /// Educational or diagnostic reason to select the family.
    pub purpose: String,
    /// Suggested public problem model shared by all canonical presets.
    pub model: GeneratorModelKindV1,
    /// Visual-regression layout class.
    pub layout_class: GeneratorLayoutClassV1,
    /// Stable generator-picker group; never inferred from randomized attributes.
    pub picker_group: GeneratorPickerGroupV1,
    /// Project-synthetic, paper-derived, or official-benchmark-derived origin.
    pub origin: String,
    /// Deterministic or randomized materialization classification.
    pub sampling: String,
    /// Ordinary, stress, or verified-worst-case classification.
    pub difficulty: String,
    /// Primary generator source-policy record.
    pub source_id: String,
    /// Stable search and structure tags from generator provenance.
    pub tags: Vec<String>,
    /// Trace, fast, and practical-boundary requests in that order.
    pub presets: Vec<GeneratorPresetV1>,
    /// One total compatibility record for every catalog descriptor.
    pub algorithm_compatibility: Vec<GeneratorAlgorithmCompatibilityV1>,
    /// Preferred executable descriptor for launching any canonical preset.
    pub default_algorithm_id: String,
    /// Scope limitation separating structural compatibility from runtime admission.
    pub admission_note: String,
}

#[derive(Clone, Copy)]
enum AttributeProfile {
    Variable,
    Fixed,
}

/// Returns the complete family manifest in canonical family-ID order.
#[must_use]
pub fn generator_algorithm_fixtures() -> Vec<GeneratorAlgorithmFixtureV1> {
    FLOW_GENERATOR_FAMILY_IDS
        .iter()
        .map(|family_id| {
            generator_algorithm_fixture(family_id)
                .unwrap_or_else(|| unreachable!("canonical family ID lacks a fixture"))
        })
        .collect()
}

/// Resolves one canonical family fixture.
#[must_use]
pub fn generator_algorithm_fixture(family_id: &str) -> Option<GeneratorAlgorithmFixtureV1> {
    let presets = fixture_presets(family_id)?;
    let (title, purpose) = fixture_copy(family_id)?;
    let model = fixture_model(family_id)?;
    let layout_class = fixture_layout(family_id)?;
    let classification = generator_classification(&presets[0].spec);
    let picker_group = fixture_picker_group(
        &presets[0].spec.family,
        model,
        classification.origin,
        classification.difficulty,
    );
    let recommended = recommended_algorithms(family_id, model);
    let default_algorithm_id = recommended
        .first()
        .unwrap_or_else(|| unreachable!("fixture lacks a preferred descriptor"));
    let algorithm_compatibility = algorithm_catalog()
        .iter()
        .map(|descriptor| {
            algorithm_compatibility(family_id, model, &presets, descriptor, recommended)
        })
        .collect();
    Some(GeneratorAlgorithmFixtureV1 {
        family_id: family_id.to_owned(),
        title: title.to_owned(),
        purpose: purpose.to_owned(),
        model,
        layout_class,
        picker_group,
        origin: classification.origin.to_owned(),
        sampling: classification.sampling.to_owned(),
        difficulty: classification.difficulty.to_owned(),
        source_id: classification.source_id.to_owned(),
        tags: classification.tags,
        presets,
        algorithm_compatibility,
        default_algorithm_id: (*default_algorithm_id).to_owned(),
        admission_note: "互換性は model と構造要件に対する判定です。各 preset は algorithm 固有の node/edge band と work ceiling を実行前に別途検査します。boundary はブラウザ生成・表示の実用境界であり、全 algorithm の実行保証ではありません。".to_owned(),
    })
}

fn fixture_picker_group(
    family: &FlowGeneratorFamilyV1,
    model: GeneratorModelKindV1,
    origin: &str,
    difficulty: &str,
) -> GeneratorPickerGroupV1 {
    if difficulty == "verified-worst-case" {
        return GeneratorPickerGroupV1::WorstCase;
    }
    if difficulty == "stress" {
        return GeneratorPickerGroupV1::Stress;
    }
    if origin == "official-benchmark-derived" {
        return GeneratorPickerGroupV1::Benchmark;
    }
    if !matches!(
        model,
        GeneratorModelKindV1::MaxFlow | GeneratorModelKindV1::Circulation
    ) || matches!(family, FlowGeneratorFamilyV1::VisionSegmentationGrid { .. })
    {
        return GeneratorPickerGroupV1::Special;
    }
    if generator_family_is_randomized(family) {
        return GeneratorPickerGroupV1::Random;
    }
    GeneratorPickerGroupV1::Structural
}

fn algorithm_compatibility(
    family_id: &str,
    model: GeneratorModelKindV1,
    presets: &[GeneratorPresetV1],
    descriptor: &crate::catalog::AlgorithmDescriptor,
    recommended: &[&str],
) -> GeneratorAlgorithmCompatibilityV1 {
    let (state, reason) = if descriptor.status != ImplementationStatus::Executable {
        (
            GeneratorAlgorithmCompatibilityStateV1::Incompatible,
            "catalog descriptor は現在 executable ではありません".to_owned(),
        )
    } else if !descriptor
        .models
        .iter()
        .any(|candidate| model_matches(model, *candidate))
    {
        (
            GeneratorAlgorithmCompatibilityStateV1::Incompatible,
            format!(
                "canonical preset の suggested model `{}` を受理しません",
                model_id(model)
            ),
        )
    } else if let Some(requirement) = descriptor
        .graph_requirements
        .iter()
        .find(|requirement| !fixture_guarantees(family_id, model, presets, **requirement))
    {
        (
            GeneratorAlgorithmCompatibilityStateV1::Incompatible,
            format!(
                "canonical preset は `{}` を保証しません",
                requirement_id(*requirement)
            ),
        )
    } else if recommended.contains(&descriptor.id) {
        (
            GeneratorAlgorithmCompatibilityStateV1::Recommended,
            "この形状で主要な挙動を観察できる推奨 descriptor です".to_owned(),
        )
    } else {
        (
            GeneratorAlgorithmCompatibilityStateV1::Compatible,
            "model と宣言済み構造要件は一致します。preset ごとの admission band は実行前に検査します".to_owned(),
        )
    };
    GeneratorAlgorithmCompatibilityV1 {
        algorithm_id: descriptor.id.to_owned(),
        state,
        reason,
    }
}

fn model_matches(model: GeneratorModelKindV1, candidate: CatalogModelKind) -> bool {
    matches!(
        (model, candidate),
        (GeneratorModelKindV1::MaxFlow, CatalogModelKind::MaxFlow)
            | (
                GeneratorModelKindV1::Circulation,
                CatalogModelKind::Circulation
            )
            | (
                GeneratorModelKindV1::Transshipment,
                CatalogModelKind::Transshipment
            )
            | (
                GeneratorModelKindV1::BipartiteMatching,
                CatalogModelKind::BipartiteMatching
            )
            | (
                GeneratorModelKindV1::Assignment,
                CatalogModelKind::Assignment
            )
            | (
                GeneratorModelKindV1::Transportation,
                CatalogModelKind::Transportation
            )
            | (
                GeneratorModelKindV1::PlanarMaxFlow,
                CatalogModelKind::PlanarMaxFlow
            )
    )
}

fn fixture_guarantees(
    family_id: &str,
    model: GeneratorModelKindV1,
    presets: &[GeneratorPresetV1],
    requirement: GraphRequirement,
) -> bool {
    match requirement {
        GraphRequirement::NoSelfLoops
        | GraphRequirement::PositiveCapacity
        | GraphRequirement::NonEmptyEdges => true,
        GraphRequirement::ZeroFlowFeasible => matches!(
            model,
            GeneratorModelKindV1::MaxFlow
                | GeneratorModelKindV1::Circulation
                | GeneratorModelKindV1::PlanarMaxFlow
        ),
        GraphRequirement::ZeroCost => {
            matches!(
                model,
                GeneratorModelKindV1::MaxFlow | GeneratorModelKindV1::PlanarMaxFlow
            ) && presets
                .iter()
                .all(|preset| matches!(preset.spec.cost, CostDistributionV1::Zero {}))
        }
        GraphRequirement::DistinctTerminals => matches!(
            model,
            GeneratorModelKindV1::MaxFlow | GeneratorModelKindV1::PlanarMaxFlow
        ),
        GraphRequirement::UnderlyingConnected => {
            model == GeneratorModelKindV1::PlanarMaxFlow
                || (model == GeneratorModelKindV1::MaxFlow
                    && !matches!(
                        family_id,
                        "erdos-renyi-directed" | "random-geometric" | "random-regular-directed"
                    ))
        }
        GraphRequirement::UnitCapacity => matches!(
            family_id,
            "assignment-matrix" | "hall-tight-bipartite" | "washington-matching"
        ),
        GraphRequirement::UnitNetwork => {
            matches!(family_id, "hall-tight-bipartite" | "washington-matching")
        }
        GraphRequirement::Bipartite => matches!(
            model,
            GeneratorModelKindV1::BipartiteMatching
                | GeneratorModelKindV1::Assignment
                | GeneratorModelKindV1::Transportation
        ),
        GraphRequirement::BalancedBipartite => {
            matches!(family_id, "hall-tight-bipartite" | "washington-matching")
        }
        GraphRequirement::TransportationNetwork => model == GeneratorModelKindV1::Transportation,
        GraphRequirement::PlanarEmbedding => model == GeneratorModelKindV1::PlanarMaxFlow,
        GraphRequirement::StronglyConnected => matches!(
            family_id,
            "cycle"
                | "goldberg-mesh-circulation"
                | "random-regular-directed"
                | "strongly-connected"
                | "torus"
        ),
        GraphRequirement::NonbindingTransshipmentCapacities => family_id == "transportation-table",
    }
}

fn model_id(model: GeneratorModelKindV1) -> &'static str {
    match model {
        GeneratorModelKindV1::MaxFlow => "max-flow",
        GeneratorModelKindV1::Circulation => "circulation",
        GeneratorModelKindV1::Transshipment => "transshipment",
        GeneratorModelKindV1::BipartiteMatching => "bipartite-matching",
        GeneratorModelKindV1::Assignment => "assignment",
        GeneratorModelKindV1::Transportation => "transportation",
        GeneratorModelKindV1::PlanarMaxFlow => "planar-max-flow",
    }
}

fn requirement_id(requirement: GraphRequirement) -> &'static str {
    match requirement {
        GraphRequirement::NoSelfLoops => "no-self-loops",
        GraphRequirement::ZeroFlowFeasible => "zero-flow-feasible",
        GraphRequirement::PositiveCapacity => "positive-capacity",
        GraphRequirement::NonEmptyEdges => "non-empty-edges",
        GraphRequirement::ZeroCost => "zero-cost",
        GraphRequirement::DistinctTerminals => "distinct-terminals",
        GraphRequirement::UnderlyingConnected => "underlying-connected",
        GraphRequirement::UnitCapacity => "unit-capacity",
        GraphRequirement::UnitNetwork => "unit-network",
        GraphRequirement::Bipartite => "bipartite",
        GraphRequirement::BalancedBipartite => "balanced-bipartite",
        GraphRequirement::TransportationNetwork => "transportation-network",
        GraphRequirement::PlanarEmbedding => "planar-embedding",
        GraphRequirement::StronglyConnected => "strongly-connected",
        GraphRequirement::NonbindingTransshipmentCapacities => {
            "nonbinding-transshipment-capacities"
        }
    }
}

fn recommended_algorithms(family_id: &str, model: GeneratorModelKindV1) -> &'static [&'static str] {
    match family_id {
        "dinic-worst-case" | "washington-dinic-phase-stress" => &["dinic"],
        "washington-goldberg-fifo-stress" => &["fifo-push-relabel", "highest-label-push-relabel"],
        "washington-cheriyan-stress" | "cherkassky-goldberg-ak-stress" => &[
            "generic-push-relabel",
            "fifo-push-relabel",
            "highest-label-push-relabel",
        ],
        "zadeh-phase-chain-stress" => &["edmonds-karp"],
        "glover-dense-acyclic-stress" | "waissi-setubal-acyclic-dense" => {
            &["dinic", "highest-label-push-relabel"]
        }
        "vision-segmentation-grid" => &["boykov-kolmogorov", "ibfs", "eibfs"],
        "planar-triangulated" => &["hassin-st-planar", "borradaile-klein-planar"],
        "hall-tight-bipartite" | "washington-matching" => &["hopcroft-karp"],
        "assignment-matrix" => &["hungarian", "auction"],
        "transportation-table" => &["transportation-simplex", "modi"],
        // These benchmark families make cost scaling perform enough source
        // work to exceed the visualizer's bounded reversible-timeline budget
        // even at their smallest canonical preset. Network simplex remains a
        // generic exact solver for the same model and produces a readable
        // operation-level trace for the canonical teaching preset.
        "goldberg-mesh-circulation" | "gridgraph-grid" | "goto-torus" => {
            &["primal-network-simplex", "cost-scaling"]
        }
        // The 400-node practical-boundary NETGEN preset exceeds the
        // deliberately small network-simplex display band, while cost
        // scaling admits it. The interactive 24-node generator default is
        // selected independently by the web client and still uses network
        // simplex for a compact readable trace.
        "netgen-skeleton" => &["cost-scaling", "primal-network-simplex"],
        _ => match model {
            GeneratorModelKindV1::MaxFlow => {
                &["dinic", "highest-label-push-relabel", "edmonds-karp"]
            }
            GeneratorModelKindV1::Circulation | GeneratorModelKindV1::Transshipment => {
                &["cost-scaling", "primal-network-simplex"]
            }
            GeneratorModelKindV1::BipartiteMatching => &["hopcroft-karp"],
            GeneratorModelKindV1::Assignment => &["hungarian", "auction"],
            GeneratorModelKindV1::Transportation => &["transportation-simplex", "modi"],
            GeneratorModelKindV1::PlanarMaxFlow => &["hassin-st-planar", "borradaile-klein-planar"],
        },
    }
}

fn fixture_model(family_id: &str) -> Option<GeneratorModelKindV1> {
    Some(match family_id {
        "cycle" | "goldberg-mesh-circulation" | "torus" => GeneratorModelKindV1::Circulation,
        "gridgen-grid" | "gridgraph-grid" | "goto-torus" | "netgen-skeleton" => {
            GeneratorModelKindV1::Transshipment
        }
        "hall-tight-bipartite" | "washington-matching" => GeneratorModelKindV1::BipartiteMatching,
        "assignment-matrix" => GeneratorModelKindV1::Assignment,
        "transportation-table" => GeneratorModelKindV1::Transportation,
        "planar-triangulated" => GeneratorModelKindV1::PlanarMaxFlow,
        id if FLOW_GENERATOR_FAMILY_IDS.contains(&id) => GeneratorModelKindV1::MaxFlow,
        _ => return None,
    })
}

fn fixture_layout(family_id: &str) -> Option<GeneratorLayoutClassV1> {
    Some(match family_id {
        "path" | "parallel-paths" | "diamond-chain" | "ladder" | "layered-dag" | "random-dag"
        | "multi-source-sink" => GeneratorLayoutClassV1::LinearLayered,
        "cycle" | "watts-strogatz-fixed" => GeneratorLayoutClassV1::RadialCyclic,
        "grid-2d" | "grid-3d" | "gridgen-grid" | "gridgraph-grid" | "vision-segmentation-grid" => {
            GeneratorLayoutClassV1::GridLocal
        }
        "torus"
        | "goto-torus"
        | "goldberg-mesh-circulation"
        | "waissi-transit-one-way-grid"
        | "waissi-transit-two-way-grid" => GeneratorLayoutClassV1::GridPeriodic,
        "assignment-matrix"
        | "bipartite-random"
        | "hall-tight-bipartite"
        | "transportation-table"
        | "washington-matching" => GeneratorLayoutClassV1::Partitioned,
        "arborescence" | "preferential-attachment-directed" => GeneratorLayoutClassV1::Hierarchical,
        "clustered-directed" | "planted-bottleneck" | "strongly-connected" => {
            GeneratorLayoutClassV1::Clustered
        }
        "complete-dag"
        | "erdos-renyi-directed"
        | "random-geometric"
        | "random-regular-directed"
        | "planar-triangulated" => GeneratorLayoutClassV1::DenseSpatial,
        id if FLOW_GENERATOR_FAMILY_IDS.contains(&id) => GeneratorLayoutClassV1::BenchmarkGadget,
        _ => return None,
    })
}

#[allow(clippy::too_many_lines)]
fn fixture_copy(family_id: &str) -> Option<(&'static str, &'static str)> {
    Some(match family_id {
        "arborescence" => ("有向木", "分岐数と深さが作る階層・細長さを比較する"),
        "assignment-matrix" => ("割当コスト行列", "疎な植込み最適割当と価格更新を観察する"),
        "bipartite-random" => (
            "ランダム二部グラフ",
            "exact-edge-count の matching 入力を比較する",
        ),
        "cherkassky-goldberg-ak-stress" => (
            "Cherkassky–Goldberg AK",
            "push–relabel selection policy の有限サイズ差を測る",
        ),
        "clustered-directed" => ("有向クラスタ", "局所密度と少数 bridge の分離を観察する"),
        "complete-dag" => ("完全 DAG", "稠密な前向き辺と残余辺の重なりを確認する"),
        "cycle" => ("有向サイクル", "循環・負閉路・残余逆辺の最小例を作る"),
        "diamond-chain" => ("ダイヤモンド列", "分岐と合流を繰り返す増加路を追う"),
        "dinic-worst-case" => (
            "Dinic n−1 phase",
            "stable-ID Dinic の厳密 certificate 付き最悪例を再現する",
        ),
        "erdos-renyi-directed" => ("Erdős–Rényi G(n,m)", "一様な単純有向辺標本を比較する"),
        "glover-dense-acyclic-stress" => (
            "Glover dense DAG",
            "特殊 chain 容量を持つ source-claimed stress を測る",
        ),
        "goldberg-mesh-circulation" => (
            "Goldberg Mesh",
            "signed cost と距離減衰容量を持つ torus circulation を調べる",
        ),
        "goto-torus" => (
            "GOTO opened torus",
            "格子・長距離辺・供給需要を組み合わせた benchmark を試す",
        ),
        "grid-2d" => ("2D グリッド", "局所的な右・下・対角経路を観察する"),
        "grid-3d" => ("3D グリッド", "層をまたぐ局所辺と投影の可読性を調べる"),
        "gridgen-grid" => (
            "GRIDGEN grid",
            "supernode と balanced terminal を持つ transshipment を試す",
        ),
        "gridgraph-grid" => (
            "GRIDGRAPH",
            "一次資料由来の right/down grid transshipment を再現する",
        ),
        "hall-tight-bipartite" => (
            "Hall-tight 二部グラフ",
            "tight prefix と完全 matching の境界を可視化する",
        ),
        "ladder" => ("ラダー", "平行 rail と cross edge の増加路選択を比べる"),
        "layered-dag" => (
            "レイヤー DAG",
            "BFS level と blocking flow を最も読みやすく示す",
        ),
        "multi-source-sink" => (
            "複数 source/sink 変換",
            "super terminal 変換と terminal-heavy 辺を示す",
        ),
        "netgen-skeleton" => (
            "NETGEN skeleton",
            "実行可能 skeleton 付き一般 transshipment を試す",
        ),
        "parallel-paths" => ("並列パス", "bottleneck と同時に使える独立経路を比較する"),
        "path" => ("パス", "残余更新と bottleneck の最小トレースを作る"),
        "planar-triangulated" => ("平面三角形分割", "rotation system と双対最短路を観察する"),
        "planted-bottleneck" => ("Planted bottleneck", "既知の unit cut と最大流値を検証する"),
        "preferential-attachment-directed" => {
            ("優先的選択", "hub を持つ成長グラフで局所集中を観察する")
        }
        "random-dag" => (
            "ランダム DAG",
            "固定 topological order の exact-edge 標本を試す",
        ),
        "random-geometric" => ("ランダム幾何グラフ", "距離閾値による空間局所性を観察する"),
        "random-regular-directed" => ("有向正則グラフ", "均一次数と再ラベル後の経路を比較する"),
        "rmfgen-frames" => (
            "RMFGEN frames",
            "frame 内格子と frame 間 permutation を再現する",
        ),
        "strongly-connected" => (
            "強連結ランダム",
            "base cycle が保証する到達性と追加辺を比較する",
        ),
        "torus" => ("有向トーラス", "周期境界を持つ circulation を観察する"),
        "transportation-table" => ("輸送表", "疎な feasible support と基底 pivot を観察する"),
        "vision-segmentation-grid" => (
            "画像分割格子",
            "terminal-heavy な局所 graph-cut を BK/IBFS で比較する",
        ),
        "waissi-setubal-acyclic-dense" => {
            ("Waissi–Setubal AC", "完全 DAG の乱数容量 benchmark を試す")
        }
        "waissi-transit-one-way-grid" => (
            "Waissi one-way transit",
            "street ごとに一方向を選ぶ transit grid を試す",
        ),
        "waissi-transit-two-way-grid" => (
            "Waissi two-way transit",
            "双方向 street の独立容量を比較する",
        ),
        "washington-basic-line" => (
            "Washington Basic Line",
            "固定 offset 数の forward line benchmark を試す",
        ),
        "washington-cheriyan-stress" => (
            "Washington Cheriyan",
            "四つの chain gadget と unit bridge を測る",
        ),
        "washington-dinic-phase-stress" => (
            "Washington Dinic phase",
            "source-claimed phase stress の有限値を測る",
        ),
        "washington-double-exponential-line" => (
            "Washington Double Exponential Line",
            "signed offset と距離減衰容量を試す",
        ),
        "washington-exponential-line" => (
            "Washington Exponential Line",
            "forward 距離 band の容量減衰を試す",
        ),
        "washington-goldberg-fifo-stress" => (
            "Washington Goldberg FIFO",
            "FIFO と highest-label の有限サイズ差を測る",
        ),
        "washington-matching" => (
            "Washington Matching",
            "固定左次数の unit-network benchmark を試す",
        ),
        "washington-mesh" => ("Washington Mesh", "円筒三近傍の level graph を試す"),
        "washington-random-level" => (
            "Washington Random Level",
            "次 level の三つの乱数 target を試す",
        ),
        "washington-square-mesh" => (
            "Washington Square Mesh",
            "row-major forward offset mesh を試す",
        ),
        "watts-strogatz-fixed" => (
            "Watts–Strogatz fixed",
            "固定本数 rewiring の小世界構造を比較する",
        ),
        "zadeh-phase-chain-stress" => (
            "Zadeh-inspired phase chain",
            "stable-BFS 増加回数 k³/4 の有限回帰を測る",
        ),
        _ => return None,
    })
}

fn variable_spec(family: FlowGeneratorFamilyV1, seed: &str) -> FlowGeneratorSpecV1 {
    FlowGeneratorSpecV1 {
        generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
        seed: seed.to_owned(),
        family,
        capacity: CapacityDistributionV1::Uniform {
            minimum: "1".to_owned(),
            maximum: "12".to_owned(),
        },
        cost: CostDistributionV1::Uniform {
            minimum: "-3".to_owned(),
            maximum: "5".to_owned(),
        },
        target_problem: None,
    }
}

fn fixed_spec(family: FlowGeneratorFamilyV1, seed: &str) -> FlowGeneratorSpecV1 {
    FlowGeneratorSpecV1 {
        generator_revision: FLOW_GENERATOR_REVISION.to_owned(),
        seed: seed.to_owned(),
        family,
        capacity: CapacityDistributionV1::Unit {},
        cost: CostDistributionV1::Zero {},
        target_problem: None,
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "fixture literals are small integer or string scalars and the helper normalizes both"
)]
fn counter(
    algorithm_id: &str,
    metric_id: &str,
    exact_value: impl ToString,
    evidence: GeneratorCounterEvidenceV1,
) -> GeneratorExpectedCounterV1 {
    GeneratorExpectedCounterV1 {
        algorithm_id: algorithm_id.to_owned(),
        metric_id: metric_id.to_owned(),
        exact_value: exact_value.to_string(),
        evidence,
    }
}

fn make_presets(
    profile: AttributeProfile,
    families: [FlowGeneratorFamilyV1; 3],
    counters: [Vec<GeneratorExpectedCounterV1>; 3],
    strict_certificate: bool,
) -> Vec<GeneratorPresetV1> {
    let purposes = [
        GeneratorPresetPurposeV1::Trace,
        GeneratorPresetPurposeV1::Fast,
        GeneratorPresetPurposeV1::Boundary,
    ];
    let labels = ["可読トレース", "通常比較", "実用境界"];
    let seeds = ["42", "2025", "9001"];
    families
        .into_iter()
        .zip(counters)
        .enumerate()
        .map(|(index, (family, expected_counters))| GeneratorPresetV1 {
            purpose: purposes[index],
            label: labels[index].to_owned(),
            recommended_run_profile: if index == 0 {
                GeneratorPresetRunProfileV1::Trace
            } else {
                GeneratorPresetRunProfileV1::Fast
            },
            spec: match profile {
                AttributeProfile::Variable => variable_spec(family, seeds[index]),
                AttributeProfile::Fixed => fixed_spec(family, seeds[index]),
            },
            expects_strict_difficulty_certificate: strict_certificate,
            expected_counters,
        })
        .collect()
}

fn no_counters() -> [Vec<GeneratorExpectedCounterV1>; 3] {
    [Vec::new(), Vec::new(), Vec::new()]
}

fn dinic_counters(nodes: u32) -> Vec<GeneratorExpectedCounterV1> {
    let phases = nodes - 1;
    [
        ("bfs-runs", nodes),
        ("blocking-flow-phases", phases),
        ("max-flow-value", phases),
    ]
    .into_iter()
    .map(|(metric, value)| {
        counter(
            "dinic",
            metric,
            value,
            GeneratorCounterEvidenceV1::StrictCertificate,
        )
    })
    .collect()
}

fn washington_dinic_counters(nodes: u32) -> Vec<GeneratorExpectedCounterV1> {
    [
        ("bfs-runs", nodes),
        ("blocking-flow-phases", nodes - 1),
        ("max-flow-value", if nodes == 2 { 2 } else { nodes + 1 }),
    ]
    .into_iter()
    .map(|(metric, value)| {
        counter(
            "dinic",
            metric,
            value,
            GeneratorCounterEvidenceV1::FiniteRegression,
        )
    })
    .collect()
}

fn zadeh_counters(group_size: u32) -> Vec<GeneratorExpectedCounterV1> {
    let augmentations = u64::from(group_size).pow(3) / 4;
    vec![
        counter(
            "edmonds-karp",
            "augmentations",
            augmentations,
            GeneratorCounterEvidenceV1::FiniteRegression,
        ),
        counter(
            "edmonds-karp",
            "bfs-runs",
            augmentations + 1,
            GeneratorCounterEvidenceV1::FiniteRegression,
        ),
        counter(
            "edmonds-karp",
            "max-flow-value",
            augmentations,
            GeneratorCounterEvidenceV1::FiniteRegression,
        ),
    ]
}

fn exact_flow_counters(value: u32) -> Vec<GeneratorExpectedCounterV1> {
    vec![counter(
        "dinic",
        "max-flow-value",
        value,
        GeneratorCounterEvidenceV1::StructuralIdentity,
    )]
}

fn matching_counters(value: u32) -> Vec<GeneratorExpectedCounterV1> {
    vec![counter(
        "hopcroft-karp",
        "matching-cardinality",
        value,
        GeneratorCounterEvidenceV1::StructuralIdentity,
    )]
}

fn push_relabel_counters(
    algorithm_id: &str,
    pushes: u64,
    relabels: u64,
) -> Vec<GeneratorExpectedCounterV1> {
    vec![
        counter(
            algorithm_id,
            "pushes",
            pushes,
            GeneratorCounterEvidenceV1::FiniteRegression,
        ),
        counter(
            algorithm_id,
            "relabels",
            relabels,
            GeneratorCounterEvidenceV1::FiniteRegression,
        ),
    ]
}

fn glover_trace_counters() -> Vec<GeneratorExpectedCounterV1> {
    [
        ("max-flow-value", 36),
        ("bfs-runs", 12),
        ("blocking-flow-phases", 11),
        ("augmentations", 36),
    ]
    .into_iter()
    .map(|(metric, value)| {
        counter(
            "dinic",
            metric,
            value,
            GeneratorCounterEvidenceV1::FiniteRegression,
        )
    })
    .collect()
}

fn cheriyan_trace_counters() -> Vec<GeneratorExpectedCounterV1> {
    let mut counters = push_relabel_counters("generic-push-relabel", 46, 34);
    counters.extend(push_relabel_counters("fifo-push-relabel", 34, 27));
    counters.extend(push_relabel_counters("global-relabel-heuristic", 29, 4));
    counters
}

fn cheriyan_fast_counters() -> Vec<GeneratorExpectedCounterV1> {
    let mut counters = push_relabel_counters("generic-push-relabel", 222, 136);
    counters.extend(push_relabel_counters("fifo-push-relabel", 175, 117));
    counters.extend(push_relabel_counters("global-relabel-heuristic", 141, 26));
    counters
}

#[allow(clippy::too_many_lines)]
fn fixture_presets(family_id: &str) -> Option<Vec<GeneratorPresetV1>> {
    let presets = match family_id {
        "path" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::Path { nodes: 8 },
                FlowGeneratorFamilyV1::Path { nodes: 64 },
                FlowGeneratorFamilyV1::Path { nodes: 512 },
            ],
            no_counters(),
            false,
        ),
        "cycle" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::Cycle { nodes: 8 },
                FlowGeneratorFamilyV1::Cycle { nodes: 64 },
                FlowGeneratorFamilyV1::Cycle { nodes: 512 },
            ],
            no_counters(),
            false,
        ),
        "parallel-paths" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::ParallelPaths {
                    path_count: 3,
                    internal_nodes: 2,
                },
                FlowGeneratorFamilyV1::ParallelPaths {
                    path_count: 12,
                    internal_nodes: 6,
                },
                FlowGeneratorFamilyV1::ParallelPaths {
                    path_count: 40,
                    internal_nodes: 12,
                },
            ],
            no_counters(),
            false,
        ),
        "diamond-chain" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::DiamondChain { stages: 4 },
                FlowGeneratorFamilyV1::DiamondChain { stages: 20 },
                FlowGeneratorFamilyV1::DiamondChain { stages: 100 },
            ],
            no_counters(),
            false,
        ),
        "ladder" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::Ladder {
                    columns: 6,
                    cross_edges: true,
                },
                FlowGeneratorFamilyV1::Ladder {
                    columns: 40,
                    cross_edges: true,
                },
                FlowGeneratorFamilyV1::Ladder {
                    columns: 300,
                    cross_edges: true,
                },
            ],
            no_counters(),
            false,
        ),
        "layered-dag" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::LayeredDag {
                    layers: 4,
                    width: 4,
                    fanout: 2,
                },
                FlowGeneratorFamilyV1::LayeredDag {
                    layers: 12,
                    width: 12,
                    fanout: 3,
                },
                FlowGeneratorFamilyV1::LayeredDag {
                    layers: 40,
                    width: 20,
                    fanout: 5,
                },
            ],
            no_counters(),
            false,
        ),
        "complete-dag" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::CompleteDag { nodes: 8 },
                FlowGeneratorFamilyV1::CompleteDag { nodes: 40 },
                FlowGeneratorFamilyV1::CompleteDag { nodes: 120 },
            ],
            no_counters(),
            false,
        ),
        "grid-2d" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::Grid2d {
                    rows: 4,
                    columns: 6,
                    diagonals: true,
                },
                FlowGeneratorFamilyV1::Grid2d {
                    rows: 20,
                    columns: 24,
                    diagonals: false,
                },
                FlowGeneratorFamilyV1::Grid2d {
                    rows: 40,
                    columns: 50,
                    diagonals: false,
                },
            ],
            no_counters(),
            false,
        ),
        "vision-segmentation-grid" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::VisionSegmentationGrid {
                    rows: 4,
                    columns: 5,
                    eight_neighbor: true,
                },
                FlowGeneratorFamilyV1::VisionSegmentationGrid {
                    rows: 10,
                    columns: 12,
                    eight_neighbor: false,
                },
                FlowGeneratorFamilyV1::VisionSegmentationGrid {
                    rows: 14,
                    columns: 16,
                    eight_neighbor: false,
                },
            ],
            no_counters(),
            false,
        ),
        "torus" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::Torus {
                    rows: 4,
                    columns: 5,
                },
                FlowGeneratorFamilyV1::Torus {
                    rows: 15,
                    columns: 18,
                },
                FlowGeneratorFamilyV1::Torus {
                    rows: 22,
                    columns: 22,
                },
            ],
            no_counters(),
            false,
        ),
        "erdos-renyi-directed" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::ErdosRenyiDirected {
                    nodes: 10,
                    edge_count: 20,
                },
                FlowGeneratorFamilyV1::ErdosRenyiDirected {
                    nodes: 100,
                    edge_count: 500,
                },
                FlowGeneratorFamilyV1::ErdosRenyiDirected {
                    nodes: 1_000,
                    edge_count: 5_000,
                },
            ],
            no_counters(),
            false,
        ),
        "dinic-worst-case" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::DinicWorstCase { nodes: 8 },
                FlowGeneratorFamilyV1::DinicWorstCase { nodes: 40 },
                FlowGeneratorFamilyV1::DinicWorstCase { nodes: 160 },
            ],
            [dinic_counters(8), dinic_counters(40), dinic_counters(160)],
            true,
        ),
        "washington-dinic-phase-stress" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { nodes: 12 },
                FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { nodes: 100 },
                FlowGeneratorFamilyV1::WashingtonDinicPhaseStress { nodes: 500 },
            ],
            [
                washington_dinic_counters(12),
                washington_dinic_counters(100),
                washington_dinic_counters(500),
            ],
            false,
        ),
        "washington-goldberg-fifo-stress" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { block_size: 4 },
                FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { block_size: 16 },
                FlowGeneratorFamilyV1::WashingtonGoldbergFifoStress { block_size: 32 },
            ],
            [
                push_relabel_counters("fifo-push-relabel", 63, 39),
                push_relabel_counters("fifo-push-relabel", 573, 326),
                push_relabel_counters("fifo-push-relabel", 1_895, 1_076),
            ],
            false,
        ),
        "washington-cheriyan-stress" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::WashingtonCheriyanStress {
                    bridge_width: 4,
                    gadget_entries: 2,
                    chain_length: 2,
                },
                FlowGeneratorFamilyV1::WashingtonCheriyanStress {
                    bridge_width: 8,
                    gadget_entries: 4,
                    chain_length: 2,
                },
                FlowGeneratorFamilyV1::WashingtonCheriyanStress {
                    bridge_width: 16,
                    gadget_entries: 6,
                    chain_length: 3,
                },
            ],
            [
                cheriyan_trace_counters(),
                cheriyan_fast_counters(),
                Vec::new(),
            ],
            false,
        ),
        "cherkassky-goldberg-ak-stress" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { size: 4 },
                FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { size: 16 },
                FlowGeneratorFamilyV1::CherkasskyGoldbergAkStress { size: 32 },
            ],
            [
                push_relabel_counters("fifo-push-relabel", 65, 52),
                push_relabel_counters("fifo-push-relabel", 432, 350),
                push_relabel_counters("fifo-push-relabel", 1_328, 1_082),
            ],
            false,
        ),
        "waissi-setubal-acyclic-dense" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { nodes: 12 },
                FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { nodes: 50 },
                FlowGeneratorFamilyV1::WaissiSetubalAcyclicDense { nodes: 150 },
            ],
            no_counters(),
            false,
        ),
        "glover-dense-acyclic-stress" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::GloverDenseAcyclicStress { nodes: 12 },
                FlowGeneratorFamilyV1::GloverDenseAcyclicStress { nodes: 50 },
                FlowGeneratorFamilyV1::GloverDenseAcyclicStress { nodes: 120 },
            ],
            [glover_trace_counters(), Vec::new(), Vec::new()],
            false,
        ),
        "waissi-transit-one-way-grid" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::WaissiTransitOneWayGrid {
                    dimension: 4,
                    maximum_capacity: 100,
                },
                FlowGeneratorFamilyV1::WaissiTransitOneWayGrid {
                    dimension: 12,
                    maximum_capacity: 1_000,
                },
                FlowGeneratorFamilyV1::WaissiTransitOneWayGrid {
                    dimension: 32,
                    maximum_capacity: 1_000_000,
                },
            ],
            no_counters(),
            false,
        ),
        "waissi-transit-two-way-grid" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid {
                    dimension: 4,
                    maximum_capacity: 100,
                },
                FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid {
                    dimension: 12,
                    maximum_capacity: 1_000,
                },
                FlowGeneratorFamilyV1::WaissiTransitTwoWayGrid {
                    dimension: 32,
                    maximum_capacity: 1_000_000,
                },
            ],
            no_counters(),
            false,
        ),
        "goldberg-mesh-circulation" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::GoldbergMeshCirculation {
                    columns: 4,
                    rows: 3,
                    horizontal_degree: 1,
                    vertical_degree: 1,
                },
                FlowGeneratorFamilyV1::GoldbergMeshCirculation {
                    columns: 12,
                    rows: 9,
                    horizontal_degree: 2,
                    vertical_degree: 2,
                },
                FlowGeneratorFamilyV1::GoldbergMeshCirculation {
                    columns: 14,
                    rows: 14,
                    horizontal_degree: 2,
                    vertical_degree: 2,
                },
            ],
            no_counters(),
            false,
        ),
        "washington-matching" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::WashingtonMatching {
                    part_size: 12,
                    degree: 3,
                },
                FlowGeneratorFamilyV1::WashingtonMatching {
                    part_size: 64,
                    degree: 8,
                },
                FlowGeneratorFamilyV1::WashingtonMatching {
                    part_size: 128,
                    degree: 16,
                },
            ],
            no_counters(),
            false,
        ),
        "washington-mesh" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::WashingtonMesh {
                    rows: 6,
                    columns: 8,
                    maximum_capacity: 100,
                },
                FlowGeneratorFamilyV1::WashingtonMesh {
                    rows: 16,
                    columns: 24,
                    maximum_capacity: 1_000,
                },
                FlowGeneratorFamilyV1::WashingtonMesh {
                    rows: 32,
                    columns: 40,
                    maximum_capacity: 1_000_000,
                },
            ],
            no_counters(),
            false,
        ),
        "washington-square-mesh" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::WashingtonSquareMesh {
                    dimension: 6,
                    degree: 3,
                    maximum_capacity: 100,
                },
                FlowGeneratorFamilyV1::WashingtonSquareMesh {
                    dimension: 18,
                    degree: 6,
                    maximum_capacity: 1_000,
                },
                FlowGeneratorFamilyV1::WashingtonSquareMesh {
                    dimension: 32,
                    degree: 16,
                    maximum_capacity: 1_000_000,
                },
            ],
            no_counters(),
            false,
        ),
        "washington-basic-line" => washington_line_presets("basic"),
        "washington-exponential-line" => washington_line_presets("exponential"),
        "washington-double-exponential-line" => washington_line_presets("double"),
        "zadeh-phase-chain-stress" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::ZadehPhaseChainStress { group_size: 4 },
                FlowGeneratorFamilyV1::ZadehPhaseChainStress { group_size: 8 },
                FlowGeneratorFamilyV1::ZadehPhaseChainStress { group_size: 20 },
            ],
            [zadeh_counters(4), zadeh_counters(8), zadeh_counters(20)],
            false,
        ),
        "arborescence" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::Arborescence {
                    branching: 2,
                    depth: 3,
                },
                FlowGeneratorFamilyV1::Arborescence {
                    branching: 3,
                    depth: 5,
                },
                FlowGeneratorFamilyV1::Arborescence {
                    branching: 3,
                    depth: 6,
                },
            ],
            no_counters(),
            false,
        ),
        "strongly-connected" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::StronglyConnected {
                    nodes: 10,
                    extra_edges: 15,
                },
                FlowGeneratorFamilyV1::StronglyConnected {
                    nodes: 100,
                    extra_edges: 300,
                },
                FlowGeneratorFamilyV1::StronglyConnected {
                    nodes: 800,
                    extra_edges: 3_000,
                },
            ],
            no_counters(),
            false,
        ),
        "grid-3d" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::Grid3d {
                    layers: 2,
                    rows: 3,
                    columns: 4,
                },
                FlowGeneratorFamilyV1::Grid3d {
                    layers: 5,
                    rows: 8,
                    columns: 10,
                },
                FlowGeneratorFamilyV1::Grid3d {
                    layers: 10,
                    rows: 12,
                    columns: 12,
                },
            ],
            no_counters(),
            false,
        ),
        "bipartite-random" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::BipartiteRandom {
                    left: 4,
                    right: 5,
                    edge_count: 10,
                },
                FlowGeneratorFamilyV1::BipartiteRandom {
                    left: 30,
                    right: 30,
                    edge_count: 200,
                },
                FlowGeneratorFamilyV1::BipartiteRandom {
                    left: 100,
                    right: 120,
                    edge_count: 3_000,
                },
            ],
            no_counters(),
            false,
        ),
        "assignment-matrix" => make_presets(
            AttributeProfile::Fixed,
            [
                assignment_family(4, 5, 700),
                assignment_family(12, 16, 600),
                assignment_family(40, 50, 500),
            ],
            no_counters(),
            false,
        ),
        "transportation-table" => make_presets(
            AttributeProfile::Fixed,
            [
                transportation_family(4, 5, 20, 500),
                transportation_family(12, 12, 120, 400),
                transportation_family(32, 40, 640, 300),
            ],
            no_counters(),
            false,
        ),
        "random-geometric" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::RandomGeometric {
                    nodes: 15,
                    radius: 250,
                },
                FlowGeneratorFamilyV1::RandomGeometric {
                    nodes: 80,
                    radius: 120,
                },
                FlowGeneratorFamilyV1::RandomGeometric {
                    nodes: 250,
                    radius: 100,
                },
            ],
            no_counters(),
            false,
        ),
        "random-regular-directed" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::RandomRegularDirected {
                    nodes: 12,
                    degree: 3,
                },
                FlowGeneratorFamilyV1::RandomRegularDirected {
                    nodes: 80,
                    degree: 6,
                },
                FlowGeneratorFamilyV1::RandomRegularDirected {
                    nodes: 500,
                    degree: 12,
                },
            ],
            no_counters(),
            false,
        ),
        "preferential-attachment-directed" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::PreferentialAttachmentDirected {
                    nodes: 12,
                    attachment_count: 2,
                },
                FlowGeneratorFamilyV1::PreferentialAttachmentDirected {
                    nodes: 100,
                    attachment_count: 4,
                },
                FlowGeneratorFamilyV1::PreferentialAttachmentDirected {
                    nodes: 1_000,
                    attachment_count: 6,
                },
            ],
            no_counters(),
            false,
        ),
        "planar-triangulated" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::PlanarTriangulated { nodes: 8 },
                FlowGeneratorFamilyV1::PlanarTriangulated { nodes: 48 },
                FlowGeneratorFamilyV1::PlanarTriangulated { nodes: 128 },
            ],
            no_counters(),
            false,
        ),
        "multi-source-sink" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::MultiSourceSink {
                    sources: 3,
                    intermediate: 4,
                    sinks: 2,
                },
                FlowGeneratorFamilyV1::MultiSourceSink {
                    sources: 10,
                    intermediate: 20,
                    sinks: 10,
                },
                FlowGeneratorFamilyV1::MultiSourceSink {
                    sources: 30,
                    intermediate: 80,
                    sinks: 30,
                },
            ],
            no_counters(),
            false,
        ),
        "random-dag" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::RandomDag {
                    nodes: 12,
                    edge_count: 20,
                },
                FlowGeneratorFamilyV1::RandomDag {
                    nodes: 100,
                    edge_count: 400,
                },
                FlowGeneratorFamilyV1::RandomDag {
                    nodes: 500,
                    edge_count: 5_000,
                },
            ],
            no_counters(),
            false,
        ),
        "watts-strogatz-fixed" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::WattsStrogatzFixed {
                    nodes: 20,
                    neighborhood: 4,
                    rewire_count: 8,
                },
                FlowGeneratorFamilyV1::WattsStrogatzFixed {
                    nodes: 100,
                    neighborhood: 8,
                    rewire_count: 200,
                },
                FlowGeneratorFamilyV1::WattsStrogatzFixed {
                    nodes: 500,
                    neighborhood: 12,
                    rewire_count: 1_500,
                },
            ],
            no_counters(),
            false,
        ),
        "clustered-directed" => make_presets(
            AttributeProfile::Variable,
            [
                FlowGeneratorFamilyV1::ClusteredDirected {
                    clusters: 3,
                    cluster_size: 5,
                    bridge_edges: 6,
                },
                FlowGeneratorFamilyV1::ClusteredDirected {
                    clusters: 10,
                    cluster_size: 10,
                    bridge_edges: 100,
                },
                FlowGeneratorFamilyV1::ClusteredDirected {
                    clusters: 30,
                    cluster_size: 20,
                    bridge_edges: 1_000,
                },
            ],
            no_counters(),
            false,
        ),
        "planted-bottleneck" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::PlantedBottleneck {
                    left: 5,
                    right: 6,
                    cut_edges: 8,
                },
                FlowGeneratorFamilyV1::PlantedBottleneck {
                    left: 30,
                    right: 30,
                    cut_edges: 120,
                },
                FlowGeneratorFamilyV1::PlantedBottleneck {
                    left: 100,
                    right: 120,
                    cut_edges: 2_000,
                },
            ],
            [
                exact_flow_counters(8),
                exact_flow_counters(120),
                exact_flow_counters(2_000),
            ],
            false,
        ),
        "hall-tight-bipartite" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::HallTightBipartite {
                    part_size: 8,
                    tight_prefix: 3,
                },
                FlowGeneratorFamilyV1::HallTightBipartite {
                    part_size: 32,
                    tight_prefix: 12,
                },
                FlowGeneratorFamilyV1::HallTightBipartite {
                    part_size: 100,
                    tight_prefix: 40,
                },
            ],
            [
                matching_counters(8),
                matching_counters(32),
                matching_counters(100),
            ],
            false,
        ),
        "rmfgen-frames" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::RmfgenFrames {
                    frame_size: 2,
                    depth: 3,
                    minimum_capacity: 3,
                    maximum_capacity: 9,
                },
                FlowGeneratorFamilyV1::RmfgenFrames {
                    frame_size: 6,
                    depth: 8,
                    minimum_capacity: 10,
                    maximum_capacity: 100,
                },
                FlowGeneratorFamilyV1::RmfgenFrames {
                    frame_size: 12,
                    depth: 12,
                    minimum_capacity: 10,
                    maximum_capacity: 1_000,
                },
            ],
            no_counters(),
            false,
        ),
        "gridgen-grid" => make_presets(
            AttributeProfile::Fixed,
            [
                gridgen_family(2, 2, 1, 1, 1),
                gridgen_family(15, 20, 8, 5, 160),
                gridgen_family(20, 20, 16, 6, 600),
            ],
            no_counters(),
            false,
        ),
        "gridgraph-grid" => make_presets(
            AttributeProfile::Fixed,
            [
                gridgraph_family(4, 5, 100, 1_000),
                gridgraph_family(12, 14, 10_000, 100_000),
                gridgraph_family(15, 16, 1_000_000, 1_000_000),
            ],
            no_counters(),
            false,
        ),
        "washington-random-level" => make_presets(
            AttributeProfile::Fixed,
            [
                FlowGeneratorFamilyV1::WashingtonRandomLevel {
                    rows: 6,
                    columns: 8,
                    maximum_capacity: 100,
                },
                FlowGeneratorFamilyV1::WashingtonRandomLevel {
                    rows: 16,
                    columns: 24,
                    maximum_capacity: 1_000,
                },
                FlowGeneratorFamilyV1::WashingtonRandomLevel {
                    rows: 32,
                    columns: 40,
                    maximum_capacity: 1_000_000,
                },
            ],
            no_counters(),
            false,
        ),
        "goto-torus" => make_presets(
            AttributeProfile::Fixed,
            [
                goto_family(15, 90, 8, 8),
                goto_family(100, 1_000, 10_000, 100_000),
                goto_family(200, 2_000, 1_000_000, 1_000_000),
            ],
            no_counters(),
            false,
        ),
        "netgen-skeleton" => make_presets(
            AttributeProfile::Fixed,
            [
                netgen_family(12, 2, 2, 30, 20, 0, 0, 1, 20, -3, 8),
                netgen_family(100, 10, 15, 800, 300, 2, 3, 2, 50, -10, 30),
                netgen_family(400, 24, 32, 4_000, 800, 4, 4, 2, 100, -20, 100),
            ],
            no_counters(),
            false,
        ),
        _ => return None,
    };
    Some(presets)
}

fn washington_line_presets(kind: &str) -> Vec<GeneratorPresetV1> {
    let make = |levels, width, degree| match kind {
        "basic" => FlowGeneratorFamilyV1::WashingtonBasicLine {
            levels,
            width,
            degree,
        },
        "exponential" => FlowGeneratorFamilyV1::WashingtonExponentialLine {
            levels,
            width,
            degree,
        },
        "double" => FlowGeneratorFamilyV1::WashingtonDoubleExponentialLine {
            levels,
            width,
            degree,
        },
        _ => unreachable!("closed Washington line kind"),
    };
    make_presets(
        AttributeProfile::Fixed,
        [make(8, 4, 3), make(30, 10, 8), make(80, 20, 10)],
        no_counters(),
        false,
    )
}

fn assignment_family(agents: u32, tasks: u32, density_per_mille: u32) -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::AssignmentMatrix {
        agents,
        tasks,
        objective: AssignmentObjectiveV1::Minimize,
        shape: AssignmentMatrixShapeV1::PlantedOptimum {
            density_per_mille,
            base_cost: 10,
            gap: 5,
            noise: 3,
        },
    }
}

fn transportation_family(
    origins: u32,
    destinations: u32,
    total_supply: u32,
    density_per_mille: u32,
) -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::TransportationTable {
        origins,
        destinations,
        total_supply,
        shape: TransportationTableShapeV1::SparseFeasible {
            density_per_mille,
            minimum_cost: -5,
            maximum_cost: 12,
        },
    }
}

fn gridgen_family(
    rows: u32,
    columns: u32,
    terminal_pairs: u32,
    average_degree: u32,
    total_supply: u32,
) -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::GridgenGrid {
        rows,
        columns,
        terminal_pairs,
        average_degree,
        total_supply,
        two_way: true,
        minimum_capacity: 1,
        maximum_capacity: 12,
        minimum_cost: 0,
        maximum_cost: 9,
    }
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

fn goto_family(
    nodes: u32,
    edge_count: u32,
    maximum_capacity: u32,
    maximum_cost: u32,
) -> FlowGeneratorFamilyV1 {
    FlowGeneratorFamilyV1::GotoTorus {
        nodes,
        edge_count,
        maximum_capacity,
        maximum_cost,
    }
}

#[allow(clippy::too_many_arguments)]
fn netgen_family(
    nodes: u32,
    sources: u32,
    sinks: u32,
    edge_count: u32,
    total_supply: u32,
    transshipment_sources: u32,
    transshipment_sinks: u32,
    minimum_capacity: u32,
    maximum_capacity: u32,
    minimum_cost: i64,
    maximum_cost: i64,
) -> FlowGeneratorFamilyV1 {
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
        high_cost_percentage: 75,
        capacitated_percentage: 65,
        minimum_capacity,
        maximum_capacity,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::algorithms::{
        PolynomialDualSimplexError, solve_dinic, solve_edmonds_karp, solve_fifo_push_relabel,
        solve_generic_push_relabel, solve_global_relabel_push_relabel, solve_hopcroft_karp,
        solve_polynomial_dual_network_simplex,
    };
    use crate::find_algorithm;
    use crate::generator::generate_flow_graph;
    use crate::model::{EdgeId, FlowNetwork, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::scenario::{FlowGraphV1, FlowProblemModelV1};

    #[test]
    fn manifest_covers_every_family_once_in_canonical_order() {
        let fixtures = generator_algorithm_fixtures();
        assert_eq!(fixtures.len(), FLOW_GENERATOR_FAMILY_IDS.len());
        assert_eq!(
            fixtures
                .iter()
                .map(|fixture| fixture.family_id.as_str())
                .collect::<Vec<_>>(),
            FLOW_GENERATOR_FAMILY_IDS
        );
        assert_eq!(
            fixtures
                .iter()
                .map(|fixture| fixture.family_id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            FLOW_GENERATOR_FAMILY_IDS.len()
        );
    }

    #[test]
    fn picker_groups_are_explicit_complete_and_nonempty() {
        let fixtures = generator_algorithm_fixtures();
        let counts = [
            GeneratorPickerGroupV1::Structural,
            GeneratorPickerGroupV1::Random,
            GeneratorPickerGroupV1::Special,
            GeneratorPickerGroupV1::Benchmark,
            GeneratorPickerGroupV1::Stress,
            GeneratorPickerGroupV1::WorstCase,
        ]
        .map(|group| {
            fixtures
                .iter()
                .filter(|fixture| fixture.picker_group == group)
                .count()
        });
        assert_eq!(counts, [12, 9, 5, 16, 7, 1]);

        let path = fixtures
            .iter()
            .find(|fixture| fixture.family_id == "path")
            .expect("path fixture");
        assert_eq!(path.sampling, "randomized");
        assert_eq!(path.picker_group, GeneratorPickerGroupV1::Structural);
    }

    #[test]
    fn every_preset_materializes_and_matches_declared_model() {
        for fixture in generator_algorithm_fixtures() {
            assert_eq!(fixture.presets.len(), 3, "{}", fixture.family_id);
            for (index, preset) in fixture.presets.iter().enumerate() {
                assert_eq!(
                    preset.purpose,
                    [
                        GeneratorPresetPurposeV1::Trace,
                        GeneratorPresetPurposeV1::Fast,
                        GeneratorPresetPurposeV1::Boundary,
                    ][index],
                    "{}",
                    fixture.family_id
                );
                let serialized_family = serde_json::to_value(&preset.spec.family)
                    .expect("canonical family fixture must serialize");
                assert_eq!(
                    serialized_family
                        .get("family_id")
                        .and_then(serde_json::Value::as_str),
                    Some(fixture.family_id.as_str()),
                    "{} {:?} serialized family ID drifted",
                    fixture.family_id,
                    preset.purpose
                );
                let generated = generate_flow_graph(&preset.spec).unwrap_or_else(|error| {
                    panic!(
                        "{} {:?} preset failed: {error}",
                        fixture.family_id, preset.purpose
                    )
                });
                assert!(
                    model_matches_generated(fixture.model, &generated.suggested_model),
                    "{} {:?} declared {:?}, generated {:?}",
                    fixture.family_id,
                    preset.purpose,
                    fixture.model,
                    generated.suggested_model
                );
                if preset.purpose == GeneratorPresetPurposeV1::Trace {
                    let positions = generated
                        .graph
                        .nodes
                        .iter()
                        .map(|node| {
                            let position = node.position.as_ref().unwrap_or_else(|| {
                                panic!(
                                    "{} {:?} node {} lacks a materialized position",
                                    fixture.family_id, preset.purpose, node.id
                                )
                            });
                            (position.x.as_str(), position.y.as_str())
                        })
                        .collect::<BTreeSet<_>>();
                    assert_eq!(
                        positions.len(),
                        generated.graph.nodes.len(),
                        "{} {:?} contains exact node-center overlap",
                        fixture.family_id,
                        preset.purpose
                    );
                }
                assert_eq!(
                    generated.provenance.difficulty_certificate.is_some(),
                    preset.expects_strict_difficulty_certificate,
                    "{} {:?}",
                    fixture.family_id,
                    preset.purpose
                );
            }
        }
    }

    #[test]
    fn algorithm_matrix_is_total_and_references_catalog_ids() {
        let catalog_ids = algorithm_catalog()
            .iter()
            .map(|descriptor| descriptor.id)
            .collect::<BTreeSet<_>>();
        for fixture in generator_algorithm_fixtures() {
            assert_eq!(fixture.algorithm_compatibility.len(), catalog_ids.len());
            let actual = fixture
                .algorithm_compatibility
                .iter()
                .map(|entry| entry.algorithm_id.as_str())
                .collect::<BTreeSet<_>>();
            assert_eq!(actual, catalog_ids, "{}", fixture.family_id);
            assert!(fixture.algorithm_compatibility.iter().any(|entry| {
                entry.state == GeneratorAlgorithmCompatibilityStateV1::Recommended
            }));
            assert!(fixture.algorithm_compatibility.iter().any(|entry| {
                entry.algorithm_id == fixture.default_algorithm_id
                    && entry.state == GeneratorAlgorithmCompatibilityStateV1::Recommended
            }));
        }
    }

    #[test]
    fn compatible_presets_really_satisfy_advanced_kernel_requirements() {
        let advanced = [
            GraphRequirement::PositiveCapacity,
            GraphRequirement::NonEmptyEdges,
            GraphRequirement::ZeroCost,
            GraphRequirement::DistinctTerminals,
            GraphRequirement::UnderlyingConnected,
        ];
        for fixture in generator_algorithm_fixtures() {
            let requirements = fixture
                .algorithm_compatibility
                .iter()
                .filter(|compatibility| {
                    compatibility.state != GeneratorAlgorithmCompatibilityStateV1::Incompatible
                })
                .filter_map(|compatibility| find_algorithm(&compatibility.algorithm_id))
                .flat_map(|descriptor| descriptor.graph_requirements.iter().copied())
                .filter(|requirement| advanced.contains(requirement))
                .fold(Vec::new(), |mut unique, requirement| {
                    if !unique.contains(&requirement) {
                        unique.push(requirement);
                    }
                    unique
                });
            if requirements.is_empty() {
                continue;
            }
            for preset in &fixture.presets {
                let generated = generate_flow_graph(&preset.spec).expect("fixture generates");
                let graph = canonical_network(&generated.graph);
                for requirement in &requirements {
                    let satisfied = match requirement {
                        GraphRequirement::PositiveCapacity => {
                            graph.edges().iter().all(|edge| edge.capacity() > 0)
                        }
                        GraphRequirement::NonEmptyEdges => !graph.edges().is_empty(),
                        GraphRequirement::ZeroCost => {
                            graph.edges().iter().all(|edge| edge.cost() == 0)
                        }
                        GraphRequirement::DistinctTerminals => match &generated.suggested_model {
                            FlowProblemModelV1::MaxFlow { source, sink }
                            | FlowProblemModelV1::PlanarMaxFlow { source, sink, .. } => {
                                source != sink
                                    && generated.graph.nodes.iter().any(|node| &node.id == source)
                                    && generated.graph.nodes.iter().any(|node| &node.id == sink)
                            }
                            _ => false,
                        },
                        GraphRequirement::UnderlyingConnected => underlying_connected(&graph),
                        _ => unreachable!("advanced requirement filter is closed"),
                    };
                    assert!(
                        satisfied,
                        "{} {:?} does not satisfy {:?}",
                        fixture.family_id, preset.purpose, requirement
                    );
                }
            }
        }
    }

    #[test]
    fn gridgen_polynomial_dual_compatibility_tracks_the_runtime_capacity_domain() {
        let fixture = generator_algorithm_fixture("gridgen-grid").expect("GRIDGEN fixture");
        let compatibility = fixture
            .algorithm_compatibility
            .iter()
            .find(|entry| entry.algorithm_id == "polynomial-dual-network-simplex")
            .expect("polynomial dual compatibility");
        assert_eq!(
            compatibility.state,
            GeneratorAlgorithmCompatibilityStateV1::Incompatible
        );

        let trace_graph = generate_flow_graph(&fixture.presets[0].spec).expect("trace GRIDGEN");
        let trace_network = canonical_network(&trace_graph.graph);
        let trace_required = trace_network
            .nodes()
            .iter()
            .map(|node| i128::from(node.supply()))
            .collect::<Vec<_>>();
        solve_polynomial_dual_network_simplex(&trace_network, &trace_required)
            .expect("nonbinding strongly connected GRIDGEN trace preset");

        let mut binding_spec = fixture.presets[0].spec.clone();
        binding_spec.family = FlowGeneratorFamilyV1::GridgenGrid {
            rows: 2,
            columns: 2,
            terminal_pairs: 1,
            average_degree: 1,
            total_supply: 2,
            two_way: true,
            minimum_capacity: 1,
            maximum_capacity: 1,
            minimum_cost: 0,
            maximum_cost: 9,
        };
        let binding_graph = generate_flow_graph(&binding_spec).expect("binding GRIDGEN");
        let binding_network = canonical_network(&binding_graph.graph);
        let binding_required = binding_network
            .nodes()
            .iter()
            .map(|node| i128::from(node.supply()))
            .collect::<Vec<_>>();
        assert_eq!(
            solve_polynomial_dual_network_simplex(&binding_network, &binding_required),
            Err(PolynomialDualSimplexError::CapacityBound)
        );
    }

    #[test]
    fn only_dinic_family_uses_strict_worst_case_certificate() {
        for fixture in generator_algorithm_fixtures() {
            let strict = fixture
                .presets
                .iter()
                .any(|preset| preset.expects_strict_difficulty_certificate);
            assert_eq!(strict, fixture.family_id == "dinic-worst-case");
            if fixture.difficulty == "verified-worst-case" {
                assert!(strict);
            }
            for preset in &fixture.presets {
                for counter in &preset.expected_counters {
                    assert!(find_algorithm(&counter.algorithm_id).is_some());
                    if counter.evidence == GeneratorCounterEvidenceV1::StrictCertificate {
                        assert!(preset.expects_strict_difficulty_certificate);
                    }
                }
            }
        }
    }

    #[test]
    fn declared_finite_counters_match_executable_kernels() {
        for fixture in generator_algorithm_fixtures() {
            for preset in &fixture.presets {
                if preset.expected_counters.is_empty() {
                    continue;
                }
                let generated = generate_flow_graph(&preset.spec).expect("fixture generates");
                let graph = canonical_network(&generated.graph);
                let algorithms = preset
                    .expected_counters
                    .iter()
                    .map(|counter| counter.algorithm_id.as_str())
                    .collect::<BTreeSet<_>>();
                for algorithm_id in algorithms {
                    let actual = actual_metrics(algorithm_id, &graph, &generated.suggested_model);
                    for expected in preset
                        .expected_counters
                        .iter()
                        .filter(|counter| counter.algorithm_id == algorithm_id)
                    {
                        assert_eq!(
                            actual.get(expected.metric_id.as_str()),
                            Some(&expected.exact_value),
                            "{} {:?} {} {}",
                            fixture.family_id,
                            preset.purpose,
                            algorithm_id,
                            expected.metric_id
                        );
                    }
                }
            }
        }
    }

    fn actual_metrics(
        algorithm_id: &str,
        graph: &FlowNetwork,
        model: &FlowProblemModelV1,
    ) -> BTreeMap<&'static str, String> {
        match algorithm_id {
            "dinic" => {
                let (source, sink) = max_flow_terminals(graph, model);
                let result = solve_dinic(graph, source, sink).expect("fixture Dinic run");
                BTreeMap::from([
                    ("bfs-runs", result.metrics.bfs_runs.to_string()),
                    (
                        "blocking-flow-phases",
                        result.metrics.blocking_flow_phases.to_string(),
                    ),
                    ("augmentations", result.metrics.augmentations.to_string()),
                    ("max-flow-value", result.certificate.value.to_string()),
                ])
            }
            "edmonds-karp" => {
                let (source, sink) = max_flow_terminals(graph, model);
                let result =
                    solve_edmonds_karp(graph, source, sink).expect("fixture Edmonds-Karp run");
                BTreeMap::from([
                    ("bfs-runs", result.metrics.bfs_runs.to_string()),
                    ("augmentations", result.metrics.augmentations.to_string()),
                    ("max-flow-value", result.certificate.value.to_string()),
                ])
            }
            "generic-push-relabel" => {
                let (source, sink) = max_flow_terminals(graph, model);
                let result = solve_generic_push_relabel(graph, source, sink)
                    .expect("fixture generic push-relabel run");
                push_relabel_metrics(result.metrics.pushes, result.metrics.relabels)
            }
            "fifo-push-relabel" => {
                let (source, sink) = max_flow_terminals(graph, model);
                let result = solve_fifo_push_relabel(graph, source, sink)
                    .expect("fixture FIFO push-relabel run");
                push_relabel_metrics(result.metrics.pushes, result.metrics.relabels)
            }
            "global-relabel-heuristic" => {
                let (source, sink) = max_flow_terminals(graph, model);
                let result = solve_global_relabel_push_relabel(graph, source, sink)
                    .expect("fixture global-relabel run");
                push_relabel_metrics(result.metrics.pushes, result.metrics.relabels)
            }
            "hopcroft-karp" => {
                let FlowProblemModelV1::BipartiteMatching {
                    left,
                    right,
                    flow_adapter,
                } = model
                else {
                    panic!("matching counter requires matching model")
                };
                let adapter = flow_adapter
                    .as_ref()
                    .map(|adapter| (adapter.source.as_str(), adapter.sink.as_str()));
                let result = solve_hopcroft_karp(graph, left, right, adapter)
                    .expect("fixture Hopcroft-Karp run");
                BTreeMap::from([(
                    "matching-cardinality",
                    result.certificate.cardinality.to_string(),
                )])
            }
            _ => panic!("counter uses unsupported fixture algorithm {algorithm_id}"),
        }
    }

    fn push_relabel_metrics(pushes: u64, relabels: u64) -> BTreeMap<&'static str, String> {
        BTreeMap::from([
            ("pushes", pushes.to_string()),
            ("relabels", relabels.to_string()),
        ])
    }

    fn max_flow_terminals(
        graph: &FlowNetwork,
        model: &FlowProblemModelV1,
    ) -> (crate::model::NodeIndex, crate::model::NodeIndex) {
        let FlowProblemModelV1::MaxFlow { source, sink } = model else {
            panic!("maximum-flow counter requires max-flow model")
        };
        (
            graph
                .node_index(&NodeId::parse(source).expect("source ID"))
                .expect("source node"),
            graph
                .node_index(&NodeId::parse(sink).expect("sink ID"))
                .expect("sink node"),
        )
    }

    fn canonical_network(graph: &FlowGraphV1) -> FlowNetwork {
        FlowNetwork::new(
            graph
                .nodes
                .iter()
                .map(|node| {
                    FlowNode::new(
                        NodeId::parse(&node.id).expect("generated node ID"),
                        node.supply.parse().expect("generated supply"),
                    )
                })
                .collect(),
            graph
                .edges
                .iter()
                .map(|edge| UnresolvedFlowEdge {
                    id: EdgeId::parse(&edge.id).expect("generated edge ID"),
                    from: NodeId::parse(&edge.from).expect("generated tail"),
                    to: NodeId::parse(&edge.to).expect("generated head"),
                    lower: edge.lower.parse().expect("generated lower bound"),
                    capacity: edge.capacity.parse().expect("generated capacity"),
                    cost: edge.cost.parse().expect("generated cost"),
                })
                .collect(),
        )
        .expect("generated graph validates")
    }

    fn underlying_connected(graph: &FlowNetwork) -> bool {
        if graph.nodes().is_empty() {
            return false;
        }
        let mut adjacency = vec![Vec::new(); graph.nodes().len()];
        for edge in graph.edges() {
            adjacency[edge.from().as_usize()].push(edge.to().as_usize());
            adjacency[edge.to().as_usize()].push(edge.from().as_usize());
        }
        let mut seen = vec![false; graph.nodes().len()];
        seen[0] = true;
        let mut stack = vec![0_usize];
        while let Some(node) = stack.pop() {
            for &neighbor in &adjacency[node] {
                if seen[neighbor] {
                    continue;
                }
                seen[neighbor] = true;
                stack.push(neighbor);
            }
        }
        seen.into_iter().all(|visited| visited)
    }

    fn model_matches_generated(
        model: GeneratorModelKindV1,
        generated: &FlowProblemModelV1,
    ) -> bool {
        matches!(
            (model, generated),
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
}
