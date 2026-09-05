//! Shared alpha-power potential-reduction kernel for bounded MCF realizations.
//!
//! Chen et al. define the potential, gradient, and length coordinates in
//! Equation (4), Definition 3.2, and Lemma 3.3 of arXiv:2203.00671v2. This
//! module keeps those formulas in one checked implementation. The surrounding
//! algorithms remain responsible for initialization, approximate-coordinate
//! maintenance, minimum-ratio-cycle search, and integral recovery.

#![allow(clippy::cast_precision_loss)]

use thiserror::Error;

use crate::model::FlowNetwork;

pub(crate) const ALPHA_POWER_SOURCE_STEP_DENOMINATOR: f64 = 50.0;
pub(crate) const ALPHA_POWER_DECREASE_DENOMINATOR: f64 = 500.0;
const ALPHA_DENOMINATOR: f64 = 1_000.0;
const COST_TERM_MULTIPLIER: f64 = 20.0;
const MAX_KAPPA: f64 = 0.99;
const TOLERANCE: f64 = 1.0e-9;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AlphaPowerIpmEvaluation {
    pub(crate) alpha: f64,
    pub(crate) objective: f64,
    pub(crate) gap: f64,
    pub(crate) potential: f64,
    pub(crate) gradients: Vec<f64>,
    pub(crate) lengths: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AlphaPowerIpmStep {
    pub(crate) direction: Vec<f64>,
    pub(crate) updated_flow: Vec<f64>,
    pub(crate) gradient_dot: f64,
    pub(crate) weighted_length: f64,
    pub(crate) ratio: f64,
    pub(crate) kappa: f64,
    pub(crate) multiplier: f64,
    pub(crate) weighted_step_norm: f64,
    pub(crate) guaranteed_decrease: f64,
    pub(crate) actual_decrease: f64,
    pub(crate) before: AlphaPowerIpmEvaluation,
    pub(crate) after: AlphaPowerIpmEvaluation,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(crate) enum AlphaPowerIpmError {
    #[error("alpha-power IPM input shape is invalid")]
    InvalidShape,
    #[error("alpha-power IPM requires finite values")]
    NonFinite,
    #[error("alpha-power IPM requires a strict relative-interior point")]
    NotStrictInterior,
    #[error("alpha-power IPM requires a positive objective gap")]
    NonPositiveGap,
    #[error("alpha-power IPM direction is not an active-edge circulation")]
    NotCirculation,
    #[error("alpha-power IPM direction is not a strict descent direction")]
    NotDescent,
    #[error("alpha-power IPM source progress guarantee failed")]
    ProgressInvariant,
}

pub(crate) fn evaluate_alpha_power_ipm(
    graph: &FlowNetwork,
    active: &[bool],
    flow: &[f64],
    optimum_cost: f64,
) -> Result<AlphaPowerIpmEvaluation, AlphaPowerIpmError> {
    if active.len() != graph.edges().len()
        || flow.len() != graph.edges().len()
        || !optimum_cost.is_finite()
    {
        return Err(AlphaPowerIpmError::InvalidShape);
    }
    let active_count = active.iter().filter(|&&value| value).count().max(1);
    let maximum_capacity = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::capacity)
        .max()
        .unwrap_or(1)
        .max(2);
    let scale = (active_count as f64 * maximum_capacity as f64).max(2.0);
    let alpha = 1.0 / (ALPHA_DENOMINATOR * scale.ln());
    let objective = graph
        .edges()
        .iter()
        .zip(flow)
        .map(|(edge, &amount)| edge.cost() as f64 * amount)
        .sum::<f64>();
    let gap = objective - optimum_cost;
    if !objective.is_finite() || !gap.is_finite() {
        return Err(AlphaPowerIpmError::NonFinite);
    }
    if gap <= 0.0 {
        return Err(AlphaPowerIpmError::NonPositiveGap);
    }

    let mut potential = COST_TERM_MULTIPLIER * active_count as f64 * gap.ln();
    let mut gradients = vec![0.0; graph.edges().len()];
    let mut lengths = vec![0.0; graph.edges().len()];
    for (index, ((edge, &amount), &is_active)) in
        graph.edges().iter().zip(flow).zip(active).enumerate()
    {
        if !amount.is_finite() {
            return Err(AlphaPowerIpmError::NonFinite);
        }
        if !is_active {
            continue;
        }
        let lower_slack = amount - edge.lower() as f64;
        let upper_slack = edge.capacity() as f64 - amount;
        if !(lower_slack > 0.0 && upper_slack > 0.0) {
            return Err(AlphaPowerIpmError::NotStrictInterior);
        }
        let upper_term = upper_slack.powf(-1.0 - alpha);
        let lower_term = lower_slack.powf(-1.0 - alpha);
        potential += upper_slack.powf(-alpha) + lower_slack.powf(-alpha);
        lengths[index] = upper_term + lower_term;
        gradients[index] = COST_TERM_MULTIPLIER * active_count as f64 * edge.cost() as f64 / gap
            + alpha * upper_term
            - alpha * lower_term;
        if !potential.is_finite()
            || !lengths[index].is_finite()
            || lengths[index] <= 0.0
            || !gradients[index].is_finite()
        {
            return Err(AlphaPowerIpmError::NonFinite);
        }
    }
    Ok(AlphaPowerIpmEvaluation {
        alpha,
        objective,
        gap,
        potential,
        gradients,
        lengths,
    })
}

pub(crate) fn apply_alpha_power_source_step(
    graph: &FlowNetwork,
    active: &[bool],
    flow: &[f64],
    optimum_cost: f64,
    direction: &[f64],
) -> Result<AlphaPowerIpmStep, AlphaPowerIpmError> {
    if direction.len() != graph.edges().len() || direction.iter().any(|value| !value.is_finite()) {
        return Err(AlphaPowerIpmError::InvalidShape);
    }
    let before = evaluate_alpha_power_ipm(graph, active, flow, optimum_cost)?;
    let mut divergence = vec![0.0; graph.nodes().len()];
    for (index, (edge, &amount)) in graph.edges().iter().zip(direction).enumerate() {
        if !active[index] && amount.abs() > TOLERANCE {
            return Err(AlphaPowerIpmError::NotCirculation);
        }
        divergence[edge.from().as_usize()] += amount;
        divergence[edge.to().as_usize()] -= amount;
    }
    if divergence.iter().any(|value| value.abs() > TOLERANCE) {
        return Err(AlphaPowerIpmError::NotCirculation);
    }
    let gradient_dot = before
        .gradients
        .iter()
        .zip(direction)
        .map(|(&gradient, &amount)| gradient * amount)
        .sum::<f64>();
    let weighted_length = before
        .lengths
        .iter()
        .zip(direction)
        .map(|(&length, &amount)| length * amount.abs())
        .sum::<f64>();
    if !(gradient_dot < 0.0 && weighted_length > 0.0) {
        return Err(AlphaPowerIpmError::NotDescent);
    }
    let ratio = -gradient_dot / weighted_length;
    let kappa = ratio.min(MAX_KAPPA);
    let multiplier = kappa * kappa / (ALPHA_POWER_SOURCE_STEP_DENOMINATOR * -gradient_dot);
    if !(ratio.is_finite() && kappa > 0.0 && multiplier > 0.0 && multiplier.is_finite()) {
        return Err(AlphaPowerIpmError::NonFinite);
    }
    let updated_flow = flow
        .iter()
        .zip(direction)
        .map(|(&amount, &delta)| amount + multiplier * delta)
        .collect::<Vec<_>>();
    let after = evaluate_alpha_power_ipm(graph, active, &updated_flow, optimum_cost)?;
    let weighted_step_norm = multiplier * weighted_length;
    let guaranteed_decrease = kappa * kappa / ALPHA_POWER_DECREASE_DENOMINATOR;
    let actual_decrease = before.potential - after.potential;
    let tolerance = 1.0e-8 * before.potential.abs().max(1.0);
    if actual_decrease + tolerance < guaranteed_decrease
        || weighted_step_norm > kappa / 25.0 + tolerance
        || after.gap <= 0.0
    {
        return Err(AlphaPowerIpmError::ProgressInvariant);
    }
    Ok(AlphaPowerIpmStep {
        direction: direction.to_vec(),
        updated_flow,
        gradient_dot,
        weighted_length,
        ratio,
        kappa,
        multiplier,
        weighted_step_norm,
        guaranteed_decrease,
        actual_decrease,
        before,
        after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    fn triangle() -> FlowNetwork {
        let nodes = ["a", "b", "c"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
            .collect();
        let edges = [
            ("ab", "a", "b", 0),
            ("bc", "b", "c", 0),
            ("ca", "c", "a", -1),
        ]
        .into_iter()
        .map(|(id, from, to, cost)| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge"),
            from: NodeId::parse(from).expect("tail"),
            to: NodeId::parse(to).expect("head"),
            lower: 0,
            capacity: 2,
            cost,
        })
        .collect();
        FlowNetwork::new(nodes, edges).expect("network")
    }

    #[test]
    fn source_step_uses_published_formulas_and_decreases_the_potential() {
        let graph = triangle();
        let flow = vec![1.0; 3];
        let active = vec![true; 3];
        let evaluation =
            evaluate_alpha_power_ipm(&graph, &active, &flow, -2.0).expect("evaluation");
        assert!((evaluation.gap - 1.0).abs() <= f64::EPSILON);
        assert!(evaluation.lengths.iter().all(|&length| length > 0.0));
        let step = apply_alpha_power_source_step(&graph, &active, &flow, -2.0, &[1.0, 1.0, 1.0])
            .expect("source step");
        assert!(step.gradient_dot < 0.0);
        assert!(step.actual_decrease >= step.guaranteed_decrease);
        assert!(step.weighted_step_norm <= step.kappa / 25.0);
        assert!(step.after.gap < step.before.gap);
    }

    #[test]
    fn rejects_noncirculations_and_inactive_coordinate_movement() {
        let graph = triangle();
        let flow = vec![1.0; 3];
        assert_eq!(
            apply_alpha_power_source_step(&graph, &[true; 3], &flow, -2.0, &[1.0, 0.0, 0.0],),
            Err(AlphaPowerIpmError::NotCirculation)
        );
        assert_eq!(
            apply_alpha_power_source_step(
                &graph,
                &[true, true, false],
                &flow,
                -2.0,
                &[1.0, 1.0, 1.0],
            ),
            Err(AlphaPowerIpmError::NotCirculation)
        );
    }
}
