//! Primal-dual presentation of the reduced-cost SSP kernel.

use thiserror::Error;

use crate::algorithms::{
    PotentialDijkstraSspError, PotentialDijkstraSspMetrics, solve_potential_dijkstra_ssp,
    trace_potential_dijkstra_ssp_with_feasibility,
};
use crate::certificate::MinCostFlowCertificate;
use crate::feasibility::FeasibilityExecution;
use crate::model::FlowNetwork;
use crate::residual::{ResidualError, ResidualState};
use crate::trace::{
    FlowTraceDirection, FlowTraceError, FlowTraceEvent, FlowTraceSnapshot, apply_trace_event,
};

/// Certified primal-dual result using the same exact kernel as reduced-cost SSP.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimalDualResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed objective and feasible dual prices.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic shortest-path and price-update counters.
    pub metrics: PotentialDijkstraSspMetrics,
}

/// Certified primal-dual result with invariant-specific reversible events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimalDualTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: PrimalDualResult,
    /// Replay boundary at the lower-bound pseudoflow.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical events named for primal-dual operations.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after verified optimality.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Primal-dual construction or invariant failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PrimalDualError {
    /// The shared exact reduced-cost kernel rejected the instance or result.
    #[error(transparent)]
    Kernel(#[from] PotentialDijkstraSspError),
    /// Replaying the source kernel trace failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
    /// A replay boundary could not reconstruct a residual state.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// A selected admissible path was not tight under the published prices.
    #[error("primal-dual admissible path contains a nonzero reduced-cost arc")]
    NonTightAdmissiblePath,
    /// A dual event did not publish one exact price per node.
    #[error("primal-dual dual-price snapshot is incomplete")]
    IncompleteDualPrices,
    /// Checked reduced-cost arithmetic exceeded its declared domain.
    #[error("primal-dual reduced-cost arithmetic overflow")]
    ArithmeticOverflow,
}

/// Solves balanced linear minimum-cost flow through the primal-dual SSP kernel.
///
/// This is the explicit primal-dual presentation of the same Algorithm-B
/// shortest-path method used by potential + Dijkstra SSP: the flow is the
/// primal state, node prices are the dual state, and only zero reduced-cost
/// arcs of the selected admissible path carry augmentation.
///
/// # Errors
///
/// Returns every failure of the shared exact kernel.
pub fn solve_primal_dual(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<PrimalDualResult, PrimalDualError> {
    solve_potential_dijkstra_ssp(graph, required_divergence)
        .map(PrimalDualResult::from)
        .map_err(PrimalDualError::from)
}

/// Solves the primal--dual presentation while reporting the shared SSP
/// feasibility precheck to the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_primal_dual_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<PrimalDualResult, PrimalDualError> {
    crate::algorithms::solve_potential_dijkstra_ssp_with_feasibility(
        graph,
        required_divergence,
        feasibility,
    )
    .map(PrimalDualResult::from)
    .map_err(PrimalDualError::from)
}

/// Traces dual initialization, slack labels, tightening, and admissible flow.
///
/// Every dual-tightening and admissible-augmentation boundary is replayed and
/// independently checked to ensure every selected residual arc has reduced
/// cost exactly zero under the visible node prices.
///
/// # Errors
///
/// Returns shared-kernel, replay, residual, arithmetic, or tightness failures.
pub fn trace_primal_dual(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<PrimalDualTraceResult, PrimalDualError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_primal_dual_with_feasibility(graph, required_divergence, &mut feasibility)
}

/// Traces the primal--dual presentation while explicitly publishing the
/// shared SSP feasibility precheck from this same source execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_primal_dual_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<PrimalDualTraceResult, PrimalDualError> {
    let traced =
        trace_potential_dijkstra_ssp_with_feasibility(graph, required_divergence, feasibility)?;
    let mut replay = traced.base_snapshot.clone();
    let mut events = Vec::with_capacity(traced.events.len());
    for event in &traced.events {
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)?;
        if matches!(
            event.catalog_id.as_str(),
            "potential-dijkstra-ssp.update-potentials" | "potential-dijkstra-ssp.augment"
        ) {
            check_admissible_path_is_tight(graph, &replay)?;
        }
        events.push(remap_event(event));
    }
    if replay != traced.final_snapshot {
        return Err(PrimalDualError::Trace(FlowTraceError::Precondition));
    }
    Ok(PrimalDualTraceResult {
        result: PrimalDualResult::from(traced.result),
        base_snapshot: traced.base_snapshot,
        events,
        final_snapshot: traced.final_snapshot,
    })
}

impl From<crate::algorithms::PotentialDijkstraSspResult> for PrimalDualResult {
    fn from(result: crate::algorithms::PotentialDijkstraSspResult) -> Self {
        Self {
            flows: result.flows,
            certificate: result.certificate,
            metrics: result.metrics,
        }
    }
}

fn check_admissible_path_is_tight(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), PrimalDualError> {
    let potentials = snapshot
        .node_labels
        .iter()
        .copied()
        .collect::<Option<Vec<_>>>()
        .ok_or(PrimalDualError::IncompleteDualPrices)?;
    let state = ResidualState::from_flows(graph, &snapshot.flows)?;
    for id in &snapshot.active_path {
        let arc = state
            .arc(id)
            .ok_or(PrimalDualError::NonTightAdmissiblePath)?;
        let reduced = arc
            .cost
            .checked_add(potentials[arc.from.as_usize()])
            .and_then(|value| value.checked_sub(potentials[arc.to.as_usize()]))
            .ok_or(PrimalDualError::ArithmeticOverflow)?;
        if reduced != 0 {
            return Err(PrimalDualError::NonTightAdmissiblePath);
        }
    }
    Ok(())
}

fn remap_event(event: &FlowTraceEvent) -> FlowTraceEvent {
    let mut mapped = event.clone();
    let (catalog_id, pseudocode_line) = match event.catalog_id.as_str() {
        "potential-dijkstra-ssp.initial-potentials" => (
            "primal-dual-mcf.initialize-dual",
            "primal-dual:initialize-feasible-prices",
        ),
        "potential-dijkstra-ssp.shortest-path" => (
            "primal-dual-mcf.shortest-slack-labels",
            "primal-dual:compute-shortest-slacks",
        ),
        "potential-dijkstra-ssp.inspect-residual-arc" => (
            "primal-dual-mcf.inspect-residual-arc",
            "primal-dual:inspect-one-reduced-cost-arc",
        ),
        "potential-dijkstra-ssp.update-potentials" => (
            "primal-dual-mcf.tighten-dual",
            "primal-dual:tighten-prices-until-path-admissible",
        ),
        "potential-dijkstra-ssp.augment" => (
            "primal-dual-mcf.augment-admissible-path",
            "primal-dual:augment-zero-reduced-cost-path",
        ),
        "potential-dijkstra-ssp.optimal" => (
            "primal-dual-mcf.optimal",
            "primal-dual:return-complementary-optimum",
        ),
        _ => return mapped,
    };
    catalog_id.clone_into(&mut mapped.catalog_id);
    pseudocode_line.clone_into(&mut mapped.pseudocode_line);
    mapped
}

#[cfg(test)]
mod tests {
    use crate::certificate::fixed_flow_divergences;
    use crate::model::{EdgeId, FlowNode, NodeId, NodeIndex, UnresolvedFlowEdge};

    use super::*;

    fn fixture() -> (FlowNetwork, NodeIndex, NodeIndex, Vec<i128>) {
        let graph = FlowNetwork::new(
            ["s", "a", "b", "t"]
                .into_iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
                .collect(),
            [
                ("sa", "s", "a", 2, -2),
                ("at", "a", "t", 2, 3),
                ("sb", "s", "b", 2, 1),
                ("bt", "b", "t", 2, 2),
                ("st", "s", "t", 3, 7),
            ]
            .into_iter()
            .map(|(id, from, to, capacity, cost)| UnresolvedFlowEdge {
                id: EdgeId::parse(id).expect("edge"),
                from: NodeId::parse(from).expect("tail"),
                to: NodeId::parse(to).expect("head"),
                lower: 0,
                capacity,
                cost,
            })
            .collect(),
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        let target = fixed_flow_divergences(&graph, source, sink, 4).expect("target");
        (graph, source, sink, target)
    }

    #[test]
    fn matches_the_shared_exact_kernel_and_renames_every_semantic_phase() {
        let (graph, _source, _sink, target) = fixture();
        let expected = solve_potential_dijkstra_ssp(&graph, &target).expect("SSP");
        let traced = trace_primal_dual(&graph, &target).expect("primal-dual");
        assert_eq!(traced.result.flows, expected.flows);
        assert_eq!(traced.result.certificate, expected.certificate);
        for suffix in [
            "initialize-dual",
            "shortest-slack-labels",
            "tighten-dual",
            "augment-admissible-path",
            "optimal",
        ] {
            assert!(
                traced
                    .events
                    .iter()
                    .any(|event| event.catalog_id == format!("primal-dual-mcf.{suffix}"))
            );
        }
    }

    #[test]
    fn mapped_trace_replays_forward_and_reverse_without_changing_patches() {
        let (graph, _source, _sink, target) = fixture();
        let traced = trace_primal_dual(&graph, &target).expect("primal-dual");
        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward");
        }
        assert_eq!(replay, traced.final_snapshot);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse");
        }
        assert_eq!(replay, traced.base_snapshot);
    }
}
