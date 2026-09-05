//! Machine-readable source contracts joined to the canonical flow catalog.
//!
//! `docs/flow-sources.md` remains the single source of truth for citations and
//! reviewed source scope. This module parses only its closed Confirmed table;
//! prose, cross-checks, and blocked records cannot silently satisfy a catalog
//! descriptor.

use std::collections::BTreeMap;

use serde::Serialize;
use thiserror::Error;

use crate::catalog::{
    AlgorithmDescriptor, AlgorithmId, CatalogKind, CatalogModelKind, GraphRequirement,
    ImplementationScope, ImplementationStatus, InitialAdmissionBand, InitialConstruction,
    InitialOptimalityRequirement, InitialOracleDependency, NegativeCyclePolicy, RuntimeRouteKind,
    TerminalOracleDependency, algorithm_catalog,
};
use crate::generator_fixture::{
    GeneratorAlgorithmCompatibilityStateV1, generator_algorithm_fixtures,
};

/// Revision of the descriptor-level conformance manifest.
pub const FLOW_ALGORITHM_CONFORMANCE_REVISION: &str = "flow-algorithm-conformance/2";

const SOURCE_REGISTRY: &str = include_str!("../../../docs/flow-sources.md");
const CONFIRMED_HEADING: &str = "## Confirmed records";
const CROSS_CHECK_HEADING: &str = "## Authoritative cross-checks";

/// One strict row from the Confirmed source table.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConfirmedFlowSourceRecord {
    /// Stable source-record identity.
    pub source_id: String,
    /// Evidence class declared by the registry.
    pub kind: String,
    /// Citation, immutable URL, and fixed revision text.
    pub fixed_source: String,
    /// Algorithm/section, input, invariant, and claim scope used by the project.
    pub catalog_scope_and_claims: String,
    /// License and independent-implementation boundary.
    pub implementation_note: String,
    /// Last reviewed ISO date.
    pub reviewed: String,
}

/// Independent result-checking contract exercised by runtime conformance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckerContractKind {
    /// Reconstruct flow conservation and a minimum cut from original edges.
    IndependentMaxFlowCertificate,
    /// Reconstruct balances, objective, and residual dual feasibility.
    IndependentMinCostFlowCertificate,
    /// Check both maximum flow and minimum cost at that exact value.
    IndependentMinCostMaxFlowCertificate,
    /// Reconstruct matching incidence, augmenting-path absence, and Kőnig cover.
    IndependentBipartiteMatchingCertificate,
    /// Reconstruct the complete assignment and oriented primal/dual equality.
    IndependentAssignmentCertificate,
    /// Expand native convex segments and check marginal-residual optimality.
    IndependentConvexCostCertificate,
    /// The endpoint is a primitive or parametric analysis checked by its
    /// source-defined invariant/replay checker rather than a generic flow certificate.
    SourceDefinedInvariant,
    /// A project-owned oracle demonstrator is checked for its disclosed
    /// composite contract, not attributed to the cited source algorithm.
    ProjectOracleDemonstratorInvariant,
}

/// How one descriptor demonstrates arithmetic safety at its public boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NumericSafetyContractKind {
    /// General kernels execute an aggregate-safe envelope near the `u64`/`i64`
    /// limits and reject a larger aggregate before solver dispatch.
    AggregateSafeWideArithmetic,
    /// Bounded source kernels use checked arithmetic under their smaller
    /// source-specific numeric envelope and reject the first invalid value.
    BoundedKernelCheckedArithmetic,
    /// Unit/specialized models prove the relevant cardinality/objective bound
    /// directly from their structural contract.
    StructuralDomainProof,
}

/// Independent work-bounding capabilities exposed by one descriptor.
///
/// These are deliberately not an enum: source-complete kernels such as
/// network simplex and epsilon-relaxation have both a source termination
/// argument and explicit implementation work ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct WorkLimitContract {
    /// The cited source supplies a termination argument for the advertised
    /// kernel, under the descriptor's declared input requirements.
    pub source_termination_argument: bool,
    /// The concrete project kernel checks an ID-specific iteration, scan,
    /// pivot, round, or trace-event ceiling while it is running.
    pub checked_runtime_work_ceiling: bool,
    /// Node and edge counts are rejected before dispatch above the descriptor's
    /// conservative public admission band.
    pub catalog_admission_ceiling: bool,
}

/// Descriptor-level source contract produced without duplicating citations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FlowAlgorithmConformanceContract {
    /// Schema revision.
    pub schema_revision: &'static str,
    /// Canonical catalog identity.
    pub algorithm_id: AlgorithmId,
    /// Catalog title identifies the exact project variant within a shared source.
    pub algorithm_anchor: &'static str,
    /// Solver/variant/heuristic/primitive identity.
    pub kind: CatalogKind,
    /// Publication readiness, distinct from source-fidelity scope.
    pub status: ImplementationStatus,
    /// Whether bounded or external machinery participates in the result.
    pub implementation_scope: ImplementationScope,
    /// Closed top-level runtime route.
    pub runtime_route: RuntimeRouteKind,
    /// Exact public model set accepted by the descriptor.
    pub models: &'static [CatalogModelKind],
    /// Structural graph requirements enforced before execution.
    pub graph_requirements: &'static [GraphRequirement],
    /// Required initial-state construction.
    pub initial_construction: InitialConstruction,
    /// Required initial or partial-flow optimality.
    pub initial_optimality: InitialOptimalityRequirement,
    /// Project-owned optimum information consumed before source progress.
    pub initial_oracle_dependency: InitialOracleDependency,
    /// Initial residual negative-cycle policy.
    pub negative_cycle_policy: NegativeCyclePolicy,
    /// Project-owned optimum information used only after the source prefix.
    pub terminal_oracle_dependency: TerminalOracleDependency,
    /// Exact versus approximate public result.
    pub exact: bool,
    /// Whether source execution requires randomized choices.
    pub randomized: bool,
    /// Source complexity or disclosed bounded-endpoint implementation claim.
    pub complexity: &'static str,
    /// Conservative public node/edge admission band.
    pub initial_band: InitialAdmissionBand,
    /// Exact independent certificate family or source-defined invariant family.
    pub checker_contract_kind: CheckerContractKind,
    /// Arithmetic boundary evidence used by the cross-ID conformance sweep.
    pub numeric_safety_contract_kind: NumericSafetyContractKind,
    /// Independent execution-work capabilities exercised by the cross-ID sweep.
    pub work_limit_contract: WorkLimitContract,
    /// Canonical generator families structurally compatible with this descriptor.
    /// Specialized scenario fixtures remain a separate contract until they are
    /// moved out of the browser suite into the shared manifest.
    pub compatible_generator_fixture_ids: Vec<String>,
    /// Joined confirmed source record.
    pub source: ConfirmedFlowSourceRecord,
}

/// Strict source-registry projection failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum FlowSourceContractError {
    /// Required section boundaries are absent or reversed.
    #[error("flow source registry section layout is invalid")]
    SectionLayout,
    /// A confirmed Markdown table row does not contain exactly six cells.
    #[error("flow source registry row {line} has an invalid shape")]
    RowShape {
        /// One-based Markdown line number.
        line: usize,
    },
    /// A source ID is malformed or repeated.
    #[error("flow source registry row {line} has an invalid or duplicate source ID")]
    SourceId {
        /// One-based Markdown line number.
        line: usize,
    },
    /// The evidence kind is not one of the closed registry classes.
    #[error("flow source registry row {line} has an unknown evidence kind")]
    EvidenceKind {
        /// One-based Markdown line number.
        line: usize,
    },
    /// Required source content is empty.
    #[error("flow source registry row {line} has an empty required field")]
    EmptyField {
        /// One-based Markdown line number.
        line: usize,
    },
    /// The reviewed date is not canonical YYYY-MM-DD.
    #[error("flow source registry row {line} has an invalid reviewed date")]
    ReviewedDate {
        /// One-based Markdown line number.
        line: usize,
    },
    /// A catalog descriptor points outside the confirmed source table.
    #[error("flow descriptor {algorithm_id} has no confirmed source record {source_id}")]
    MissingCatalogSource {
        /// Canonical descriptor identity.
        algorithm_id: &'static str,
        /// Missing source-record identity.
        source_id: &'static str,
    },
}

/// Returns the closed checker family for one catalog algorithm.
#[must_use]
#[allow(clippy::too_many_lines)]
pub const fn checker_contract_kind(algorithm: AlgorithmId) -> CheckerContractKind {
    match algorithm {
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
        | AlgorithmId::UnitCapacityDinic
        | AlgorithmId::UnitNetworkDinic
        | AlgorithmId::KarzanovPreflow
        | AlgorithmId::Mpm
        | AlgorithmId::DynamicTreeBlockingFlow
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
        | AlgorithmId::HochbaumPseudoflow
        | AlgorithmId::PseudoflowSimplex
        | AlgorithmId::BoykovKolmogorov
        | AlgorithmId::Ibfs
        | AlgorithmId::Eibfs
        | AlgorithmId::HassinStPlanar
        | AlgorithmId::BorradaileKleinPlanar
        | AlgorithmId::AugmentingElectricalFlow
        | AlgorithmId::InteriorPointMaxFlow
        | AlgorithmId::OrlinMaxFlow
        | AlgorithmId::WeightedAugmentingPaths
        | AlgorithmId::WeightedPushRelabel
        | AlgorithmId::DynamicEibfs
        | AlgorithmId::WarmStartPushRelabel => CheckerContractKind::IndependentMaxFlowCertificate,
        AlgorithmId::SuccessiveShortestAugmentingPath => {
            CheckerContractKind::IndependentMinCostMaxFlowCertificate
        }
        AlgorithmId::HopcroftKarp => CheckerContractKind::IndependentBipartiteMatchingCertificate,
        AlgorithmId::Hungarian | AlgorithmId::Auction => {
            CheckerContractKind::IndependentAssignmentCertificate
        }
        AlgorithmId::SegmentExpandedConvexMcf
        | AlgorithmId::ConvexCostScaling
        | AlgorithmId::ConvexNetworkSimplex => {
            CheckerContractKind::IndependentConvexCostCertificate
        }
        AlgorithmId::SimpleCycleCanceling
        | AlgorithmId::MinimumMeanCycleCanceling
        | AlgorithmId::CancelAndTighten
        | AlgorithmId::RelaxedMostNegativeCycle
        | AlgorithmId::SuccessiveShortestPath
        | AlgorithmId::BellmanFordSsp
        | AlgorithmId::PotentialDijkstraSsp
        | AlgorithmId::PrimalDualMcf
        | AlgorithmId::BlockingFlowPrimalDual
        | AlgorithmId::CapacityScalingMcf
        | AlgorithmId::EnhancedCapacityScaling
        | AlgorithmId::CostScaling
        | AlgorithmId::CostScalingPushRelabel
        | AlgorithmId::AugmentRelabel
        | AlgorithmId::PartialAugmentRelabelMcf
        | AlgorithmId::PriceRefinement
        | AlgorithmId::ArcFixing
        | AlgorithmId::ExcessScalingMcf
        | AlgorithmId::DoubleScaling
        | AlgorithmId::GeneralizedCostScaling
        | AlgorithmId::PrimalNetworkSimplex
        | AlgorithmId::DualNetworkSimplex
        | AlgorithmId::PolynomialPrimalNetworkSimplex
        | AlgorithmId::PolynomialDualNetworkSimplex
        | AlgorithmId::DynamicTreeNetworkSimplex
        | AlgorithmId::TransportationSimplex
        | AlgorithmId::Modi
        | AlgorithmId::OutOfKilter
        | AlgorithmId::Relaxation
        | AlgorithmId::EpsilonRelaxation
        | AlgorithmId::OrlinMcf
        | AlgorithmId::PrimalDualInteriorPointMcf
        | AlgorithmId::ElectricalFlowInteriorPointMcf
        | AlgorithmId::DeterministicAlmostLinearMcf
        | AlgorithmId::PredictionAssistedEpsilonRelaxation => {
            CheckerContractKind::IndependentMinCostFlowCertificate
        }
        AlgorithmId::BinaryBlockingFlow
        | AlgorithmId::ParametricPseudoflow
        | AlgorithmId::ParametricBreakpointRerun
        | AlgorithmId::ElectricalFlow
        | AlgorithmId::MinimumRatioCycleMaxFlow
        | AlgorithmId::TardosFramework
        | AlgorithmId::MinimumRatioCycleMcf => CheckerContractKind::SourceDefinedInvariant,
        AlgorithmId::RandomizedAlmostLinearMaxFlow
        | AlgorithmId::DeterministicAlmostLinearMaxFlow
        | AlgorithmId::RandomizedAlmostLinearMcf => {
            CheckerContractKind::ProjectOracleDemonstratorInvariant
        }
    }
}

/// Returns the numeric-boundary evidence family derived from the descriptor's
/// public model and bounded-kernel contract.
#[must_use]
pub fn numeric_safety_contract_kind(descriptor: &AlgorithmDescriptor) -> NumericSafetyContractKind {
    if descriptor.initial_band.max_edges <= 512 {
        NumericSafetyContractKind::BoundedKernelCheckedArithmetic
    } else if descriptor.graph_requirements.iter().any(|requirement| {
        matches!(
            requirement,
            GraphRequirement::UnitCapacity | GraphRequirement::UnitNetwork
        )
    }) || matches!(
        descriptor.runtime_route,
        RuntimeRouteKind::ParametricMaxFlow
            | RuntimeRouteKind::BipartiteMatching
            | RuntimeRouteKind::Assignment
            | RuntimeRouteKind::ConvexCostFlow
    ) {
        NumericSafetyContractKind::StructuralDomainProof
    } else {
        NumericSafetyContractKind::AggregateSafeWideArithmetic
    }
}

/// Returns independent execution-work capabilities for one descriptor.
#[must_use]
pub const fn work_limit_contract(descriptor: &AlgorithmDescriptor) -> WorkLimitContract {
    WorkLimitContract {
        source_termination_argument: matches!(
            descriptor.implementation_scope,
            ImplementationScope::SourceComplete | ImplementationScope::BoundedOracleGuided
        ),
        checked_runtime_work_ceiling: has_checked_runtime_work_ceiling(descriptor.algorithm_id),
        catalog_admission_ceiling: true,
    }
}

/// Reports only explicit checked runtime ceilings in the concrete kernel.
///
/// This is intentionally conservative. A `false` value does not mean that the
/// algorithm may fail to terminate; it means that termination is supplied by
/// the algorithm/integer-state argument rather than by a separate project cap.
#[must_use]
pub const fn has_checked_runtime_work_ceiling(algorithm: AlgorithmId) -> bool {
    matches!(
        algorithm,
        AlgorithmId::FordFulkerson
            | AlgorithmId::DfsFordFulkerson
            | AlgorithmId::WidestAugmentingPath
            | AlgorithmId::CapacityScalingAugmentingPath
            | AlgorithmId::EdmondsKarp
            | AlgorithmId::ShortestAugmentingPath
            | AlgorithmId::Isap
            | AlgorithmId::DistanceDirectedAugmentingPath
            | AlgorithmId::DistanceDirectedScalingAugmentingPath
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
            | AlgorithmId::HochbaumPseudoflow
            | AlgorithmId::PseudoflowSimplex
            | AlgorithmId::ParametricPseudoflow
            | AlgorithmId::ParametricBreakpointRerun
            | AlgorithmId::BoykovKolmogorov
            | AlgorithmId::Ibfs
            | AlgorithmId::Eibfs
            | AlgorithmId::HopcroftKarp
            | AlgorithmId::HassinStPlanar
            | AlgorithmId::BorradaileKleinPlanar
            | AlgorithmId::ElectricalFlow
            | AlgorithmId::AugmentingElectricalFlow
            | AlgorithmId::InteriorPointMaxFlow
            | AlgorithmId::MinimumRatioCycleMaxFlow
            | AlgorithmId::OrlinMaxFlow
            | AlgorithmId::RandomizedAlmostLinearMaxFlow
            | AlgorithmId::DeterministicAlmostLinearMaxFlow
            | AlgorithmId::WeightedAugmentingPaths
            | AlgorithmId::WeightedPushRelabel
            | AlgorithmId::DynamicEibfs
            | AlgorithmId::WarmStartPushRelabel
            | AlgorithmId::SimpleCycleCanceling
            | AlgorithmId::MinimumMeanCycleCanceling
            | AlgorithmId::CancelAndTighten
            | AlgorithmId::RelaxedMostNegativeCycle
            | AlgorithmId::SuccessiveShortestPath
            | AlgorithmId::BellmanFordSsp
            | AlgorithmId::PotentialDijkstraSsp
            | AlgorithmId::SuccessiveShortestAugmentingPath
            | AlgorithmId::PrimalDualMcf
            | AlgorithmId::BlockingFlowPrimalDual
            | AlgorithmId::CapacityScalingMcf
            | AlgorithmId::EnhancedCapacityScaling
            | AlgorithmId::CostScaling
            | AlgorithmId::CostScalingPushRelabel
            | AlgorithmId::AugmentRelabel
            | AlgorithmId::PartialAugmentRelabelMcf
            | AlgorithmId::PriceRefinement
            | AlgorithmId::ArcFixing
            | AlgorithmId::ExcessScalingMcf
            | AlgorithmId::DoubleScaling
            | AlgorithmId::GeneralizedCostScaling
            | AlgorithmId::PrimalNetworkSimplex
            | AlgorithmId::DualNetworkSimplex
            | AlgorithmId::PolynomialPrimalNetworkSimplex
            | AlgorithmId::PolynomialDualNetworkSimplex
            | AlgorithmId::DynamicTreeNetworkSimplex
            | AlgorithmId::TransportationSimplex
            | AlgorithmId::Modi
            | AlgorithmId::OutOfKilter
            | AlgorithmId::Relaxation
            | AlgorithmId::EpsilonRelaxation
            | AlgorithmId::Hungarian
            | AlgorithmId::Auction
            | AlgorithmId::OrlinMcf
            | AlgorithmId::PrimalDualInteriorPointMcf
            | AlgorithmId::ElectricalFlowInteriorPointMcf
            | AlgorithmId::MinimumRatioCycleMcf
            | AlgorithmId::RandomizedAlmostLinearMcf
            | AlgorithmId::DeterministicAlmostLinearMcf
            | AlgorithmId::SegmentExpandedConvexMcf
            | AlgorithmId::ConvexCostScaling
            | AlgorithmId::ConvexNetworkSimplex
            | AlgorithmId::PredictionAssistedEpsilonRelaxation
    )
}

fn canonical_source_id(value: &str) -> Option<&str> {
    let id = value.strip_prefix('`')?.strip_suffix('`')?;
    (!id.is_empty()
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        }))
    .then_some(id)
}

fn canonical_reviewed_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u16>().ok();
    let month = value[5..7].parse::<u8>().ok();
    let day = value[8..10].parse::<u8>().ok();
    year.is_some_and(|year| year >= 2000)
        && month.is_some_and(|month| (1..=12).contains(&month))
        && day.is_some_and(|day| (1..=31).contains(&day))
}

/// Parses the closed Confirmed table into strict machine-readable records.
///
/// # Errors
///
/// Rejects section drift, malformed rows, unknown evidence classes, duplicate
/// IDs, empty cells, and noncanonical reviewed dates.
pub fn confirmed_flow_source_records()
-> Result<Vec<ConfirmedFlowSourceRecord>, FlowSourceContractError> {
    let (before_cross_checks, _) = SOURCE_REGISTRY
        .split_once(CROSS_CHECK_HEADING)
        .ok_or(FlowSourceContractError::SectionLayout)?;
    let (_, confirmed) = before_cross_checks
        .split_once(CONFIRMED_HEADING)
        .ok_or(FlowSourceContractError::SectionLayout)?;
    let allowed_evidence_components = [
        "project-specification",
        "primary-paper",
        "primary-report",
        "primary-source",
        "primary-paper chain",
        "primary-paper cross-check",
        "official-specification",
        "official-implementation-doc",
        "official-implementation-code",
        "official implementation",
        "official-code-report",
        "archival-implementation",
        "archived-software-notice",
        "institutional-record",
        "archival-full-text",
        "authoritative-secondary",
        "author-course-notes",
        "author lecture notes",
        "author-report",
        "author-manuscript",
        "author exposition",
        "author thesis",
        "author working paper",
        "author-monograph-chapter",
        "author-retrospective",
        "author-talk",
        "anti-cycling cross-check",
        "modern exact-framework paper",
        "university lecture notes",
        "official-university-teaching-note",
        "canonical textbook",
        "primary-monograph",
        "fixed preprint revision",
        "textbook-oracle",
    ];
    let heading_line = SOURCE_REGISTRY[..SOURCE_REGISTRY
        .find(CONFIRMED_HEADING)
        .ok_or(FlowSourceContractError::SectionLayout)?]
        .lines()
        .count();
    let mut records = Vec::new();
    let mut ids = BTreeMap::<String, usize>::new();
    for (offset, line) in confirmed.lines().enumerate() {
        let line_number = heading_line + offset + 2;
        if !line.starts_with("| `") {
            continue;
        }
        let row = line
            .strip_prefix("| ")
            .and_then(|value| value.strip_suffix(" |"))
            .ok_or(FlowSourceContractError::RowShape { line: line_number })?;
        let cells = row.split(" | ").map(str::trim).collect::<Vec<_>>();
        if cells.len() != 6 {
            return Err(FlowSourceContractError::RowShape { line: line_number });
        }
        let source_id = canonical_source_id(cells[0])
            .ok_or(FlowSourceContractError::SourceId { line: line_number })?;
        if ids.insert(source_id.to_owned(), line_number).is_some() {
            return Err(FlowSourceContractError::SourceId { line: line_number });
        }
        if cells[1]
            .split(" + ")
            .any(|kind| !allowed_evidence_components.contains(&kind))
        {
            return Err(FlowSourceContractError::EvidenceKind { line: line_number });
        }
        if cells[2..5].iter().any(|value| value.is_empty()) {
            return Err(FlowSourceContractError::EmptyField { line: line_number });
        }
        if !canonical_reviewed_date(cells[5]) {
            return Err(FlowSourceContractError::ReviewedDate { line: line_number });
        }
        records.push(ConfirmedFlowSourceRecord {
            source_id: source_id.to_owned(),
            kind: cells[1].to_owned(),
            fixed_source: cells[2].to_owned(),
            catalog_scope_and_claims: cells[3].to_owned(),
            implementation_note: cells[4].to_owned(),
            reviewed: cells[5].to_owned(),
        });
    }
    if records.is_empty() {
        return Err(FlowSourceContractError::SectionLayout);
    }
    Ok(records)
}

/// Joins every catalog descriptor to exactly one confirmed source record.
///
/// # Errors
///
/// Rejects any descriptor whose source key is absent from the Confirmed table.
pub fn flow_algorithm_conformance_contracts()
-> Result<Vec<FlowAlgorithmConformanceContract>, FlowSourceContractError> {
    let records = confirmed_flow_source_records()?
        .into_iter()
        .map(|record| (record.source_id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut fixture_ids = BTreeMap::<String, Vec<String>>::new();
    for fixture in generator_algorithm_fixtures() {
        for compatibility in fixture.algorithm_compatibility {
            if compatibility.state != GeneratorAlgorithmCompatibilityStateV1::Incompatible {
                fixture_ids
                    .entry(compatibility.algorithm_id)
                    .or_default()
                    .push(fixture.family_id.clone());
            }
        }
    }
    algorithm_catalog()
        .iter()
        .map(|descriptor: &'static AlgorithmDescriptor| {
            let source = records.get(descriptor.source_id).cloned().ok_or(
                FlowSourceContractError::MissingCatalogSource {
                    algorithm_id: descriptor.id,
                    source_id: descriptor.source_id,
                },
            )?;
            let compatible_generator_fixture_ids =
                fixture_ids.get(descriptor.id).cloned().unwrap_or_default();
            Ok(FlowAlgorithmConformanceContract {
                schema_revision: FLOW_ALGORITHM_CONFORMANCE_REVISION,
                algorithm_id: descriptor.algorithm_id,
                algorithm_anchor: descriptor.title,
                kind: descriptor.kind,
                status: descriptor.status,
                implementation_scope: descriptor.implementation_scope,
                runtime_route: descriptor.runtime_route,
                models: descriptor.models,
                graph_requirements: descriptor.graph_requirements,
                initial_construction: descriptor.initial_construction,
                initial_optimality: descriptor.initial_optimality,
                initial_oracle_dependency: descriptor.initial_oracle_dependency,
                negative_cycle_policy: descriptor.negative_cycle_policy,
                terminal_oracle_dependency: descriptor.terminal_oracle_dependency,
                exact: descriptor.exact,
                randomized: descriptor.randomized,
                complexity: descriptor.complexity,
                initial_band: descriptor.initial_band,
                checker_contract_kind: checker_contract_kind(descriptor.algorithm_id),
                numeric_safety_contract_kind: numeric_safety_contract_kind(descriptor),
                work_limit_contract: work_limit_contract(descriptor),
                compatible_generator_fixture_ids,
                source,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn confirmed_table_is_strict_unique_and_machine_readable() {
        let records = confirmed_flow_source_records().expect("confirmed source table");
        assert_eq!(records.len(), 101);
        assert_eq!(
            records
                .iter()
                .map(|record| record.source_id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            records.len()
        );
    }

    #[test]
    fn every_catalog_descriptor_joins_to_one_confirmed_record() {
        let contracts = flow_algorithm_conformance_contracts().expect("conformance contracts");
        assert_eq!(contracts.len(), algorithm_catalog().len());
        for (descriptor, contract) in algorithm_catalog().iter().zip(&contracts) {
            assert_eq!(contract.algorithm_id, descriptor.algorithm_id);
            assert_eq!(contract.algorithm_anchor, descriptor.title);
            assert_eq!(contract.runtime_route, descriptor.runtime_route);
            assert_eq!(
                contract.implementation_scope,
                descriptor.implementation_scope
            );
            assert_eq!(contract.models, descriptor.models);
            assert_eq!(contract.graph_requirements, descriptor.graph_requirements);
            assert_eq!(
                contract.initial_oracle_dependency,
                descriptor.initial_oracle_dependency
            );
            assert_eq!(
                contract.terminal_oracle_dependency,
                descriptor.terminal_oracle_dependency
            );
            assert_eq!(
                contract.checker_contract_kind,
                checker_contract_kind(descriptor.algorithm_id)
            );
            assert_eq!(
                contract.numeric_safety_contract_kind,
                numeric_safety_contract_kind(descriptor)
            );
            assert_eq!(
                contract.work_limit_contract,
                work_limit_contract(descriptor)
            );
            assert!(
                contract
                    .compatible_generator_fixture_ids
                    .windows(2)
                    .all(|window| window[0] < window[1])
            );
            assert_eq!(contract.source.source_id, descriptor.source_id);
            assert!(!contract.source.fixed_source.is_empty());
            assert!(!contract.source.catalog_scope_and_claims.is_empty());
        }
    }

    #[test]
    fn polynomial_dual_simplex_does_not_advertise_mixed_domain_fixtures() {
        let contracts = flow_algorithm_conformance_contracts().expect("conformance contracts");
        let contract = contracts
            .iter()
            .find(|contract| contract.algorithm_id == AlgorithmId::PolynomialDualNetworkSimplex)
            .expect("polynomial dual simplex contract");
        assert!(
            !contract
                .compatible_generator_fixture_ids
                .iter()
                .any(|fixture| fixture == "cycle")
        );
        assert!(
            !contract
                .compatible_generator_fixture_ids
                .iter()
                .any(|fixture| fixture == "gridgen-grid")
        );
    }

    #[test]
    fn work_capabilities_are_independent_and_do_not_hide_runtime_ceilings() {
        let descriptor = |id| {
            algorithm_catalog()
                .iter()
                .find(|descriptor| descriptor.algorithm_id == id)
                .expect("catalog descriptor")
        };
        for algorithm in [
            AlgorithmId::Dinic,
            AlgorithmId::DualNetworkSimplex,
            AlgorithmId::EpsilonRelaxation,
        ] {
            assert_eq!(
                work_limit_contract(descriptor(algorithm)),
                WorkLimitContract {
                    source_termination_argument: true,
                    checked_runtime_work_ceiling: algorithm != AlgorithmId::Dinic,
                    catalog_admission_ceiling: true,
                }
            );
        }
        for algorithm in [
            AlgorithmId::RandomizedAlmostLinearMaxFlow,
            AlgorithmId::TardosFramework,
        ] {
            assert_eq!(
                work_limit_contract(descriptor(algorithm)),
                WorkLimitContract {
                    source_termination_argument: false,
                    checked_runtime_work_ceiling: algorithm
                        == AlgorithmId::RandomizedAlmostLinearMaxFlow,
                    catalog_admission_ceiling: true,
                }
            );
        }

        for algorithm in [
            AlgorithmId::SuccessiveShortestPath,
            AlgorithmId::PrimalDualMcf,
            AlgorithmId::ExcessScalingMcf,
        ] {
            assert!(
                work_limit_contract(descriptor(algorithm)).checked_runtime_work_ceiling,
                "shared capped kernel was not disclosed for {algorithm}"
            );
        }
    }
}
