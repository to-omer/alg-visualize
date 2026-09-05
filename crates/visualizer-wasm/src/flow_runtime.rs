//! Closed algorithm-to-runtime routing for the WebAssembly flow adapter.
//!
//! The core catalog describes public compatibility. This module owns the
//! executable adapter choice. Keeping the conversion from [`AlgorithmId`] total
//! makes a newly added catalog ID a compile-time change at this boundary instead
//! of an unsupported-runtime fallback.

use flow::{AlgorithmId, RuntimeRouteKind};

/// Runtime-visible failure classes shared by fast and trace projections.
///
/// Solver-specific invariant and input errors stay fatal. Only a verified
/// infeasibility witness or an explicit bounded-resource exit may become a
/// normal terminal scene.
pub(super) enum RuntimeFailure {
    Infeasible(flow::InfeasibilityWitness),
    ResourceLimit,
    Fatal(String),
}

pub(super) trait ClassifyRuntimeFailure {
    fn classify(self) -> RuntimeFailure;
}

macro_rules! impl_failure_classifier {
    (
        $error:ty,
        infeasible = $infeasible:pat => $witness:ident,
        resource = $resource:pat $(,)?
    ) => {
        impl ClassifyRuntimeFailure for $error {
            fn classify(self) -> RuntimeFailure {
                match self {
                    $infeasible => RuntimeFailure::Infeasible($witness),
                    $resource => RuntimeFailure::ResourceLimit,
                    error => RuntimeFailure::Fatal(error.to_string()),
                }
            }
        }
    };
}

impl_failure_classifier!(
    flow::EdmondsKarpError,
    infeasible = flow::EdmondsKarpError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::EdmondsKarpError::AdmissionLimit | flow::EdmondsKarpError::WorkLimit,
);
impl_failure_classifier!(
    flow::FordFulkersonError,
    infeasible = flow::FordFulkersonError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::FordFulkersonError::AdmissionLimit | flow::FordFulkersonError::WorkLimit,
);
impl_failure_classifier!(
    flow::DinicError,
    infeasible = flow::DinicError::Feasibility(flow::FeasibilityError::Infeasible(witness)) => witness,
    resource = flow::DinicError::AdmissionLimit,
);
impl_failure_classifier!(
    flow::BlockingPreflowError,
    infeasible = flow::BlockingPreflowError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::BlockingPreflowError::AdmissionLimit | flow::BlockingPreflowError::WorkLimit,
);
impl_failure_classifier!(
    flow::PseudoflowError,
    infeasible = flow::PseudoflowError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::PseudoflowError::AdmissionLimit | flow::PseudoflowError::WorkLimit,
);
impl_failure_classifier!(
    flow::SapError,
    infeasible = flow::SapError::Feasibility(flow::FeasibilityError::Infeasible(witness)) => witness,
    resource = flow::SapError::AdmissionLimit | flow::SapError::WorkLimit,
);
impl_failure_classifier!(
    flow::PushRelabelError,
    infeasible = flow::PushRelabelError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::PushRelabelError::AdmissionLimit | flow::PushRelabelError::WorkLimit,
);
impl_failure_classifier!(
    flow::DynamicTreeBlockingError,
    infeasible = flow::DynamicTreeBlockingError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::DynamicTreeBlockingError::AdmissionLimit
        | flow::DynamicTreeBlockingError::WorkLimit
        | flow::DynamicTreeBlockingError::Trace(flow::FlowTraceError::EventLimit),
);
impl_failure_classifier!(
    flow::DynamicTreePushRelabelError,
    infeasible = flow::DynamicTreePushRelabelError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::DynamicTreePushRelabelError::AdmissionLimit
        | flow::DynamicTreePushRelabelError::WorkLimit
        | flow::DynamicTreePushRelabelError::Trace(flow::FlowTraceError::EventLimit),
);

impl_failure_classifier!(
    flow::BellmanFordSspError,
    infeasible = flow::BellmanFordSspError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::BellmanFordSspError::AdmissionLimit
        | flow::BellmanFordSspError::AugmentationLimit,
);
impl_failure_classifier!(
    flow::PotentialDijkstraSspError,
    infeasible = flow::PotentialDijkstraSspError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::PotentialDijkstraSspError::AdmissionLimit
        | flow::PotentialDijkstraSspError::AugmentationLimit,
);
impl_failure_classifier!(
    flow::BlockingPrimalDualError,
    infeasible = flow::BlockingPrimalDualError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::BlockingPrimalDualError::AdmissionLimit
        | flow::BlockingPrimalDualError::WorkLimit
        | flow::BlockingPrimalDualError::Trace(flow::FlowTraceError::EventLimit),
);
impl_failure_classifier!(
    flow::CapacityScalingError,
    infeasible = flow::CapacityScalingError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::CapacityScalingError::AdmissionLimit | flow::CapacityScalingError::WorkLimit,
);
impl_failure_classifier!(
    flow::CostScalingError,
    infeasible = flow::CostScalingError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::CostScalingError::AdmissionLimit | flow::CostScalingError::WorkLimit,
);
impl_failure_classifier!(
    flow::OutOfKilterError,
    infeasible = flow::OutOfKilterError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::OutOfKilterError::AdmissionLimit | flow::OutOfKilterError::WorkLimit,
);
impl_failure_classifier!(
    flow::RelaxationError,
    infeasible = flow::RelaxationError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::RelaxationError::AdmissionLimit
        | flow::RelaxationError::WorkLimit
        | flow::RelaxationError::Trace(flow::FlowTraceError::EventLimit),
);
impl_failure_classifier!(
    flow::EpsilonRelaxationError,
    infeasible = flow::EpsilonRelaxationError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::EpsilonRelaxationError::AdmissionLimit
        | flow::EpsilonRelaxationError::WorkLimit
        | flow::EpsilonRelaxationError::Trace(flow::FlowTraceError::EventLimit),
);
impl_failure_classifier!(
    flow::NetworkSimplexError,
    infeasible = flow::NetworkSimplexError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::NetworkSimplexError::AdmissionLimit | flow::NetworkSimplexError::WorkLimit,
);
impl_failure_classifier!(
    flow::SimpleCycleCancelingError,
    infeasible = flow::SimpleCycleCancelingError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::SimpleCycleCancelingError::AdmissionLimit
        | flow::SimpleCycleCancelingError::WorkLimit,
);
impl_failure_classifier!(
    flow::MinimumMeanCycleCancelingError,
    infeasible = flow::MinimumMeanCycleCancelingError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::MinimumMeanCycleCancelingError::AdmissionLimit
        | flow::MinimumMeanCycleCancelingError::WorkLimit,
);
impl_failure_classifier!(
    flow::CancelTightenError,
    infeasible = flow::CancelTightenError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::CancelTightenError::AdmissionLimit | flow::CancelTightenError::WorkLimit,
);
impl_failure_classifier!(
    flow::RelaxedMndcError,
    infeasible = flow::RelaxedMndcError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::RelaxedMndcError::AdmissionLimit | flow::RelaxedMndcError::WorkLimit,
);
impl_failure_classifier!(
    flow::EnhancedCapacityScalingError,
    infeasible = flow::EnhancedCapacityScalingError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::EnhancedCapacityScalingError::AdmissionLimit
        | flow::EnhancedCapacityScalingError::WorkLimit,
);
impl_failure_classifier!(
    flow::OrlinMcfError,
    infeasible = flow::OrlinMcfError::Feasibility(flow::FeasibilityError::Infeasible(witness)) => witness,
    resource = flow::OrlinMcfError::AdmissionLimit | flow::OrlinMcfError::WorkLimit,
);
impl_failure_classifier!(
    flow::FlowFrameworkMcfError,
    infeasible = flow::FlowFrameworkMcfError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::FlowFrameworkMcfError::AdmissionLimit
        | flow::FlowFrameworkMcfError::IterationLimit
        | flow::FlowFrameworkMcfError::Dynamic(
            flow::DynamicMinRatioCycleError::AdmissionLimit
        ),
);
impl_failure_classifier!(
    flow::PrimalDualIpmError,
    infeasible = flow::PrimalDualIpmError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::PrimalDualIpmError::AdmissionLimit | flow::PrimalDualIpmError::NonConvergence,
);
impl ClassifyRuntimeFailure for flow::ElectricalIpmMcfError {
    fn classify(self) -> RuntimeFailure {
        match self {
            Self::Feasibility(flow::FeasibilityError::Infeasible(witness)) => {
                RuntimeFailure::Infeasible(witness)
            }
            Self::AdmissionLimit
            | Self::IsolationGuardExhausted
            | Self::NonConvergence
            | Self::Feasibility(flow::FeasibilityError::TraceWorkLimit) => {
                RuntimeFailure::ResourceLimit
            }
            error @ (Self::InvalidDivergence
            | Self::Feasibility(
                flow::FeasibilityError::InvalidDivergence
                | flow::FeasibilityError::InvalidTerminals
                | flow::FeasibilityError::ArithmeticOverflow
                | flow::FeasibilityError::TraceInvariant,
            )
            | Self::IsolationInvariant
            | Self::NumericalFailure
            | Self::RecoveryFailure
            | Self::ArithmeticOverflow
            | Self::Certificate(_)
            | Self::TraceVerification) => RuntimeFailure::Fatal(error.to_string()),
        }
    }
}
impl_failure_classifier!(
    flow::DualNetworkSimplexError,
    infeasible = flow::DualNetworkSimplexError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::DualNetworkSimplexError::AdmissionLimit
        | flow::DualNetworkSimplexError::WorkLimit,
);
impl_failure_classifier!(
    flow::PolynomialPrimalSimplexError,
    infeasible = flow::PolynomialPrimalSimplexError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::PolynomialPrimalSimplexError::AdmissionLimit
        | flow::PolynomialPrimalSimplexError::WorkLimit,
);
impl_failure_classifier!(
    flow::PolynomialDualSimplexError,
    infeasible = flow::PolynomialDualSimplexError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::PolynomialDualSimplexError::AdmissionLimit
        | flow::PolynomialDualSimplexError::WorkLimit,
);
impl_failure_classifier!(
    flow::DoubleScalingError,
    infeasible = flow::DoubleScalingError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::DoubleScalingError::AdmissionLimit | flow::DoubleScalingError::WorkLimit,
);
impl_failure_classifier!(
    flow::PredictionAssistedEpsilonError,
    infeasible = flow::PredictionAssistedEpsilonError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::PredictionAssistedEpsilonError::AdmissionLimit
        | flow::PredictionAssistedEpsilonError::WorkLimit,
);
impl_failure_classifier!(
    flow::TardosFrameworkError,
    infeasible = flow::TardosFrameworkError::Feasibility(
        flow::FeasibilityError::Infeasible(witness)
    ) => witness,
    resource = flow::TardosFrameworkError::AdmissionLimit,
);

impl ClassifyRuntimeFailure for flow::PrimalDualError {
    fn classify(self) -> RuntimeFailure {
        match self {
            Self::Kernel(error) => error.classify(),
            error => RuntimeFailure::Fatal(error.to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RuntimeRunner {
    MaxFlow(MaxFlowRunner),
    MinCostFlow(MinCostFlowRunner),
    MinCostMaxFlow,
    ParametricMaxFlow(ParametricMaxFlowRunner),
    BipartiteMatching,
    Assignment(AssignmentRunner),
    Transportation(TransportationRunner),
    PlanarMaxFlow(PlanarMaxFlowRunner),
    ConvexCostFlow(ConvexCostFlowRunner),
}

impl RuntimeRunner {
    #[allow(
        clippy::too_many_lines,
        reason = "the exhaustive catalog boundary intentionally keeps all IDs in one compiler-checked match"
    )]
    pub(super) const fn for_algorithm(id: AlgorithmId) -> Self {
        match id {
            AlgorithmId::FordFulkerson => Self::MaxFlow(MaxFlowRunner::FordFulkerson(
                FordFulkersonRunner::FordFulkerson,
            )),
            AlgorithmId::DfsFordFulkerson => Self::MaxFlow(MaxFlowRunner::FordFulkerson(
                FordFulkersonRunner::DepthFirst,
            )),
            AlgorithmId::WidestAugmentingPath => Self::MaxFlow(MaxFlowRunner::FordFulkerson(
                FordFulkersonRunner::WidestPath,
            )),
            AlgorithmId::CapacityScalingAugmentingPath => Self::MaxFlow(
                MaxFlowRunner::FordFulkerson(FordFulkersonRunner::CapacityScaling),
            ),
            AlgorithmId::EdmondsKarp => Self::MaxFlow(MaxFlowRunner::EdmondsKarp),
            AlgorithmId::ShortestAugmentingPath => {
                Self::MaxFlow(MaxFlowRunner::Sap(SapRunner::ShortestAugmentingPath))
            }
            AlgorithmId::Isap => Self::MaxFlow(MaxFlowRunner::Sap(SapRunner::Improved)),
            AlgorithmId::DistanceDirectedAugmentingPath => Self::MaxFlow(
                MaxFlowRunner::DistanceDirected(DistanceDirectedRunner::ExactTree),
            ),
            AlgorithmId::DistanceDirectedScalingAugmentingPath => Self::MaxFlow(
                MaxFlowRunner::DistanceDirected(DistanceDirectedRunner::CapacityScaling),
            ),
            AlgorithmId::Dinic => Self::MaxFlow(MaxFlowRunner::Dinic(DinicRunner::General)),
            AlgorithmId::UnitCapacityDinic => {
                Self::MaxFlow(MaxFlowRunner::Dinic(DinicRunner::UnitCapacity))
            }
            AlgorithmId::UnitNetworkDinic => {
                Self::MaxFlow(MaxFlowRunner::Dinic(DinicRunner::UnitNetwork))
            }
            AlgorithmId::KarzanovPreflow => Self::MaxFlow(MaxFlowRunner::BlockingPreflow(
                BlockingPreflowRunner::Karzanov,
            )),
            AlgorithmId::Mpm => {
                Self::MaxFlow(MaxFlowRunner::BlockingPreflow(BlockingPreflowRunner::Mpm))
            }
            AlgorithmId::DynamicTreeBlockingFlow => {
                Self::MaxFlow(MaxFlowRunner::DynamicTreeBlocking)
            }
            AlgorithmId::BinaryBlockingFlow => Self::MaxFlow(MaxFlowRunner::BinaryBlocking),
            AlgorithmId::GoldbergRao => Self::MaxFlow(MaxFlowRunner::GoldbergRao),
            AlgorithmId::GenericPushRelabel => {
                Self::MaxFlow(MaxFlowRunner::PushRelabel(PushRelabelRunner::Generic))
            }
            AlgorithmId::FifoPushRelabel => {
                Self::MaxFlow(MaxFlowRunner::PushRelabel(PushRelabelRunner::Fifo))
            }
            AlgorithmId::RelabelToFront => Self::MaxFlow(MaxFlowRunner::PushRelabel(
                PushRelabelRunner::RelabelToFront,
            )),
            AlgorithmId::HighestLabelPushRelabel => {
                Self::MaxFlow(MaxFlowRunner::PushRelabel(PushRelabelRunner::HighestLabel))
            }
            AlgorithmId::ExcessScalingPushRelabel => {
                Self::MaxFlow(MaxFlowRunner::PushRelabel(PushRelabelRunner::ExcessScaling))
            }
            AlgorithmId::PartialAugmentRelabelMaxFlow => Self::MaxFlow(MaxFlowRunner::PushRelabel(
                PushRelabelRunner::PartialAugmentRelabel,
            )),
            AlgorithmId::CurrentArcHeuristic => {
                Self::MaxFlow(MaxFlowRunner::PushRelabel(PushRelabelRunner::CurrentArc))
            }
            AlgorithmId::GlobalRelabelHeuristic => {
                Self::MaxFlow(MaxFlowRunner::PushRelabel(PushRelabelRunner::GlobalRelabel))
            }
            AlgorithmId::GapRelabelHeuristic => {
                Self::MaxFlow(MaxFlowRunner::PushRelabel(PushRelabelRunner::GapRelabel))
            }
            AlgorithmId::DynamicTreePushRelabel => {
                Self::MaxFlow(MaxFlowRunner::DynamicTreePushRelabel)
            }
            AlgorithmId::SynchronousParallelPushRelabel => {
                Self::MaxFlow(MaxFlowRunner::SynchronousPushRelabel)
            }
            AlgorithmId::HochbaumPseudoflow => {
                Self::MaxFlow(MaxFlowRunner::Pseudoflow(PseudoflowRunner::Hochbaum))
            }
            AlgorithmId::PseudoflowSimplex => {
                Self::MaxFlow(MaxFlowRunner::Pseudoflow(PseudoflowRunner::Simplex))
            }
            AlgorithmId::BoykovKolmogorov => Self::MaxFlow(MaxFlowRunner::BoykovKolmogorov),
            AlgorithmId::Ibfs => Self::MaxFlow(MaxFlowRunner::Ibfs),
            AlgorithmId::Eibfs => Self::MaxFlow(MaxFlowRunner::Eibfs),
            AlgorithmId::ElectricalFlow => Self::MaxFlow(MaxFlowRunner::ElectricalFlow),
            AlgorithmId::AugmentingElectricalFlow => {
                Self::MaxFlow(MaxFlowRunner::AugmentingElectricalFlow)
            }
            AlgorithmId::InteriorPointMaxFlow => Self::MaxFlow(MaxFlowRunner::InteriorPoint),
            AlgorithmId::MinimumRatioCycleMaxFlow => {
                Self::MaxFlow(MaxFlowRunner::MinimumRatioCycle)
            }
            AlgorithmId::OrlinMaxFlow => Self::MaxFlow(MaxFlowRunner::Orlin),
            AlgorithmId::RandomizedAlmostLinearMaxFlow => {
                Self::MaxFlow(MaxFlowRunner::RandomizedAlmostLinear)
            }
            AlgorithmId::DeterministicAlmostLinearMaxFlow => {
                Self::MaxFlow(MaxFlowRunner::DeterministicAlmostLinear)
            }
            AlgorithmId::WeightedAugmentingPaths => {
                Self::MaxFlow(MaxFlowRunner::WeightedAugmentingPaths)
            }
            AlgorithmId::WeightedPushRelabel => Self::MaxFlow(MaxFlowRunner::WeightedPushRelabel),
            AlgorithmId::DynamicEibfs => Self::MaxFlow(MaxFlowRunner::DynamicEibfs),
            AlgorithmId::WarmStartPushRelabel => Self::MaxFlow(MaxFlowRunner::WarmStartPushRelabel),

            AlgorithmId::ParametricPseudoflow => {
                Self::ParametricMaxFlow(ParametricMaxFlowRunner::Pseudoflow)
            }
            AlgorithmId::ParametricBreakpointRerun => {
                Self::ParametricMaxFlow(ParametricMaxFlowRunner::BreakpointRerun)
            }
            AlgorithmId::HopcroftKarp => Self::BipartiteMatching,
            AlgorithmId::HassinStPlanar => Self::PlanarMaxFlow(PlanarMaxFlowRunner::Hassin),
            AlgorithmId::BorradaileKleinPlanar => {
                Self::PlanarMaxFlow(PlanarMaxFlowRunner::BorradaileKlein)
            }

            AlgorithmId::SimpleCycleCanceling => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::SimpleCycleCanceling,
            )),
            AlgorithmId::MinimumMeanCycleCanceling => Self::MinCostFlow(
                MinCostFlowRunner::Classical(ClassicalMinCostFlowRunner::MinimumMeanCycleCanceling),
            ),
            AlgorithmId::CancelAndTighten => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::CancelAndTighten,
            )),
            AlgorithmId::RelaxedMostNegativeCycle => Self::MinCostFlow(
                MinCostFlowRunner::Classical(ClassicalMinCostFlowRunner::RelaxedMostNegativeCycle),
            ),
            AlgorithmId::SuccessiveShortestPath => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::SuccessiveShortestPath,
            )),
            AlgorithmId::BellmanFordSsp => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::BellmanFordSsp,
            )),
            AlgorithmId::PotentialDijkstraSsp => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::PotentialDijkstraSsp,
            )),
            AlgorithmId::PrimalDualMcf => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::PrimalDual,
            )),
            AlgorithmId::BlockingFlowPrimalDual => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::BlockingFlowPrimalDual,
            )),
            AlgorithmId::CapacityScalingMcf => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::CapacityScaling(CapacityScalingRunner::Capacity),
            )),
            AlgorithmId::ExcessScalingMcf => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::CapacityScaling(CapacityScalingRunner::Excess),
            )),
            AlgorithmId::EnhancedCapacityScaling => Self::MinCostFlow(
                MinCostFlowRunner::Classical(ClassicalMinCostFlowRunner::EnhancedCapacityScaling),
            ),
            AlgorithmId::CostScaling => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::CostScaling(CostScalingRunner::CostScaling),
            )),
            AlgorithmId::CostScalingPushRelabel => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::CostScaling(CostScalingRunner::PushRelabel),
            )),
            AlgorithmId::AugmentRelabel => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::CostScaling(CostScalingRunner::AugmentRelabel),
            )),
            AlgorithmId::PartialAugmentRelabelMcf => Self::MinCostFlow(
                MinCostFlowRunner::Classical(ClassicalMinCostFlowRunner::CostScaling(
                    CostScalingRunner::PartialAugmentRelabel,
                )),
            ),
            AlgorithmId::PriceRefinement => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::CostScaling(CostScalingRunner::PriceRefinement),
            )),
            AlgorithmId::ArcFixing => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::CostScaling(CostScalingRunner::ArcFixing),
            )),
            AlgorithmId::GeneralizedCostScaling => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::CostScaling(CostScalingRunner::Generalized),
            )),
            AlgorithmId::DoubleScaling => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::DoubleScaling,
            )),
            AlgorithmId::PrimalNetworkSimplex => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::NetworkSimplex(NetworkSimplexRunner::Primal),
            )),
            AlgorithmId::DynamicTreeNetworkSimplex => {
                Self::MinCostFlow(MinCostFlowRunner::Classical(
                    ClassicalMinCostFlowRunner::NetworkSimplex(NetworkSimplexRunner::DynamicTree),
                ))
            }
            AlgorithmId::DualNetworkSimplex => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::DualNetworkSimplex,
            )),
            AlgorithmId::PolynomialPrimalNetworkSimplex => {
                Self::MinCostFlow(MinCostFlowRunner::Classical(
                    ClassicalMinCostFlowRunner::PolynomialPrimalNetworkSimplex,
                ))
            }
            AlgorithmId::PolynomialDualNetworkSimplex => {
                Self::MinCostFlow(MinCostFlowRunner::Classical(
                    ClassicalMinCostFlowRunner::PolynomialDualNetworkSimplex,
                ))
            }
            AlgorithmId::OutOfKilter => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::OutOfKilter,
            )),
            AlgorithmId::Relaxation => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::Relaxation,
            )),
            AlgorithmId::EpsilonRelaxation => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::EpsilonRelaxation,
            )),
            AlgorithmId::TardosFramework => Self::MinCostFlow(MinCostFlowRunner::TardosFramework),
            AlgorithmId::OrlinMcf => Self::MinCostFlow(MinCostFlowRunner::Classical(
                ClassicalMinCostFlowRunner::Orlin,
            )),
            AlgorithmId::PrimalDualInteriorPointMcf => {
                Self::MinCostFlow(MinCostFlowRunner::PrimalDualInteriorPoint)
            }
            AlgorithmId::ElectricalFlowInteriorPointMcf => {
                Self::MinCostFlow(MinCostFlowRunner::ElectricalFlowInteriorPoint)
            }
            AlgorithmId::MinimumRatioCycleMcf => {
                Self::MinCostFlow(MinCostFlowRunner::MinimumRatioCycle)
            }
            AlgorithmId::RandomizedAlmostLinearMcf => {
                Self::MinCostFlow(MinCostFlowRunner::RandomizedAlmostLinear)
            }
            AlgorithmId::DeterministicAlmostLinearMcf => {
                Self::MinCostFlow(MinCostFlowRunner::DeterministicAlmostLinear)
            }
            AlgorithmId::PredictionAssistedEpsilonRelaxation => {
                Self::MinCostFlow(MinCostFlowRunner::PredictionAssistedEpsilonRelaxation)
            }

            AlgorithmId::SuccessiveShortestAugmentingPath => Self::MinCostMaxFlow,
            AlgorithmId::Hungarian => Self::Assignment(AssignmentRunner::Hungarian),
            AlgorithmId::Auction => Self::Assignment(AssignmentRunner::Auction),
            AlgorithmId::TransportationSimplex => {
                Self::Transportation(TransportationRunner::Simplex)
            }
            AlgorithmId::Modi => Self::Transportation(TransportationRunner::Modi),
            AlgorithmId::SegmentExpandedConvexMcf => {
                Self::ConvexCostFlow(ConvexCostFlowRunner::SegmentExpanded)
            }
            AlgorithmId::ConvexCostScaling => {
                Self::ConvexCostFlow(ConvexCostFlowRunner::CostScaling)
            }
            AlgorithmId::ConvexNetworkSimplex => {
                Self::ConvexCostFlow(ConvexCostFlowRunner::NetworkSimplex)
            }
        }
    }

    pub(super) const fn route(self) -> RuntimeRouteKind {
        match self {
            Self::MaxFlow(_) => RuntimeRouteKind::MaxFlow,
            Self::MinCostFlow(_) => RuntimeRouteKind::MinCostFlow,
            Self::MinCostMaxFlow => RuntimeRouteKind::MinCostMaxFlow,
            Self::ParametricMaxFlow(_) => RuntimeRouteKind::ParametricMaxFlow,
            Self::BipartiteMatching => RuntimeRouteKind::BipartiteMatching,
            Self::Assignment(_) => RuntimeRouteKind::Assignment,
            Self::Transportation(_) => RuntimeRouteKind::Transportation,
            Self::PlanarMaxFlow(_) => RuntimeRouteKind::PlanarMaxFlow,
            Self::ConvexCostFlow(_) => RuntimeRouteKind::ConvexCostFlow,
        }
    }
}

macro_rules! runner_ids {
    ($name:ident { $($variant:ident => $algorithm:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(super) enum $name {
            $($variant),+
        }
    };
}

macro_rules! runner_variants {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(super) enum $name {
            $($variant),+
        }
    };
}

runner_ids!(FordFulkersonRunner {
    FordFulkerson => FordFulkerson,
    DepthFirst => DfsFordFulkerson,
    WidestPath => WidestAugmentingPath,
    CapacityScaling => CapacityScalingAugmentingPath,
});

runner_ids!(SapRunner {
    ShortestAugmentingPath => ShortestAugmentingPath,
    Improved => Isap,
});

runner_ids!(DistanceDirectedRunner {
    ExactTree => DistanceDirectedAugmentingPath,
    CapacityScaling => DistanceDirectedScalingAugmentingPath,
});

runner_ids!(DinicRunner {
    General => Dinic,
    UnitCapacity => UnitCapacityDinic,
    UnitNetwork => UnitNetworkDinic,
});

runner_ids!(BlockingPreflowRunner {
    Karzanov => KarzanovPreflow,
    Mpm => Mpm,
});

runner_ids!(PushRelabelRunner {
    Generic => GenericPushRelabel,
    Fifo => FifoPushRelabel,
    RelabelToFront => RelabelToFront,
    HighestLabel => HighestLabelPushRelabel,
    ExcessScaling => ExcessScalingPushRelabel,
    PartialAugmentRelabel => PartialAugmentRelabelMaxFlow,
    CurrentArc => CurrentArcHeuristic,
    GlobalRelabel => GlobalRelabelHeuristic,
    GapRelabel => GapRelabelHeuristic,
});

runner_ids!(PseudoflowRunner {
    Hochbaum => HochbaumPseudoflow,
    Simplex => PseudoflowSimplex,
});

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MaxFlowRunner {
    FordFulkerson(FordFulkersonRunner),
    EdmondsKarp,
    Sap(SapRunner),
    DistanceDirected(DistanceDirectedRunner),
    Dinic(DinicRunner),
    BlockingPreflow(BlockingPreflowRunner),
    DynamicTreeBlocking,
    BinaryBlocking,
    GoldbergRao,
    PushRelabel(PushRelabelRunner),
    DynamicTreePushRelabel,
    SynchronousPushRelabel,
    Pseudoflow(PseudoflowRunner),
    BoykovKolmogorov,
    Ibfs,
    Eibfs,
    ElectricalFlow,
    AugmentingElectricalFlow,
    InteriorPoint,
    MinimumRatioCycle,
    Orlin,
    RandomizedAlmostLinear,
    DeterministicAlmostLinear,
    WeightedAugmentingPaths,
    WeightedPushRelabel,
    DynamicEibfs,
    WarmStartPushRelabel,
}

runner_ids!(CostScalingRunner {
    CostScaling => CostScaling,
    PushRelabel => CostScalingPushRelabel,
    AugmentRelabel => AugmentRelabel,
    PartialAugmentRelabel => PartialAugmentRelabelMcf,
    PriceRefinement => PriceRefinement,
    ArcFixing => ArcFixing,
    Generalized => GeneralizedCostScaling,
});

runner_ids!(CapacityScalingRunner {
    Capacity => CapacityScalingMcf,
    Excess => ExcessScalingMcf,
});

runner_variants!(NetworkSimplexRunner {
    Primal,
    DynamicTree,
});

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ClassicalMinCostFlowRunner {
    SimpleCycleCanceling,
    MinimumMeanCycleCanceling,
    CancelAndTighten,
    RelaxedMostNegativeCycle,
    SuccessiveShortestPath,
    BellmanFordSsp,
    PotentialDijkstraSsp,
    PrimalDual,
    BlockingFlowPrimalDual,
    CapacityScaling(CapacityScalingRunner),
    EnhancedCapacityScaling,
    CostScaling(CostScalingRunner),
    DoubleScaling,
    NetworkSimplex(NetworkSimplexRunner),
    DualNetworkSimplex,
    PolynomialPrimalNetworkSimplex,
    PolynomialDualNetworkSimplex,
    OutOfKilter,
    Relaxation,
    EpsilonRelaxation,
    Orlin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MinCostFlowRunner {
    Classical(ClassicalMinCostFlowRunner),
    TardosFramework,
    PrimalDualInteriorPoint,
    ElectricalFlowInteriorPoint,
    MinimumRatioCycle,
    RandomizedAlmostLinear,
    DeterministicAlmostLinear,
    PredictionAssistedEpsilonRelaxation,
}

runner_variants!(ParametricMaxFlowRunner {
    Pseudoflow,
    BreakpointRerun,
});

runner_variants!(AssignmentRunner { Hungarian, Auction });

runner_variants!(TransportationRunner { Simplex, Modi });

runner_variants!(PlanarMaxFlowRunner {
    Hassin,
    BorradaileKlein,
});

runner_variants!(ConvexCostFlowRunner {
    SegmentExpanded,
    CostScaling,
    NetworkSimplex,
});

#[cfg(test)]
mod tests {
    use super::*;
    use flow::{AlgorithmId, algorithm_catalog};

    #[test]
    fn every_algorithm_has_one_runner_with_the_catalog_route() {
        assert_eq!(AlgorithmId::ALL.len(), algorithm_catalog().len());
        for (id, descriptor) in AlgorithmId::ALL.iter().zip(algorithm_catalog()) {
            let runner = RuntimeRunner::for_algorithm(*id);
            assert_eq!(runner.route(), descriptor.runtime_route, "{}", id.as_str());
        }
    }

    #[test]
    fn electrical_ipm_classifier_never_hides_invariant_failures_as_resource_limits() {
        for error in [
            flow::ElectricalIpmMcfError::AdmissionLimit,
            flow::ElectricalIpmMcfError::IsolationGuardExhausted,
            flow::ElectricalIpmMcfError::NonConvergence,
        ] {
            assert!(matches!(error.classify(), RuntimeFailure::ResourceLimit));
        }
        for error in [
            flow::ElectricalIpmMcfError::InvalidDivergence,
            flow::ElectricalIpmMcfError::Feasibility(flow::FeasibilityError::ArithmeticOverflow),
            flow::ElectricalIpmMcfError::IsolationInvariant,
            flow::ElectricalIpmMcfError::NumericalFailure,
            flow::ElectricalIpmMcfError::RecoveryFailure,
            flow::ElectricalIpmMcfError::ArithmeticOverflow,
            flow::ElectricalIpmMcfError::Certificate(flow::CertificateError::ArithmeticOverflow),
            flow::ElectricalIpmMcfError::TraceVerification,
        ] {
            assert!(matches!(error.classify(), RuntimeFailure::Fatal(_)));
        }
    }
}
