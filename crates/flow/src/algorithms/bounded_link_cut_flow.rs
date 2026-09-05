//! Exact bounded link-cut realization of the source tree-flow interface.
//!
//! The asymptotic algorithm keeps a changing family of spanning trees and uses
//! link-cut trees for path inner products, signed flow updates, absolute
//! movement, and `Detect`. On the eight-vertex stable-slot band we choose the
//! canonical stable-edge spanning forest of the current root graph and execute
//! those same operations exactly. Rational values share one certified integer
//! scale. Tree coordinates are updated as root-path differences; absolute
//! movement uses an undirected one-edge path update. The latter explicit
//! decomposition is intentionally bounded and carries no asymptotic claim.

use std::collections::{BTreeSet, VecDeque};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};
use thiserror::Error;

use super::DynamicLevelGraphSnapshot;
use super::data_structures::link_cut::{
    DynamicTreeEdge, DynamicTreeVertex, LinkCutError, LinkCutForest,
};

const MAX_SCALE_BITS: u64 = 4_096;

/// Exact source-interface operation counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BoundedLinkCutFlowMetrics {
    /// Tree-edge links across the four scalar forests.
    pub links: u64,
    /// Root-prefix sums used for signed gradient coordinates.
    pub gradient_root_path_sums: u64,
    /// Undirected path sums used for lengths.
    pub length_path_sums: u64,
    /// Root-path additions used for signed flow.
    pub flow_root_path_adds: u64,
    /// Positive undirected path additions used for absolute movement.
    pub movement_path_adds: u64,
    /// Final tree-edge flow/movement queries.
    pub edge_value_queries: u64,
    /// Active coordinates inspected by bounded `Detect`.
    pub detect_scans: u64,
}

/// Complete exact certificate for one normalized circulation application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedLinkCutFlowCertificate {
    /// Canonical stable-edge spanning forest maintained by the bounded runtime.
    pub tree_edges: Vec<usize>,
    /// Shared positive denominator used by every link-cut scalar.
    pub scale: BigInt,
    /// Exact inner product of current gradient and normalized circulation.
    pub normalized_gradient_dot: BigRational,
    /// Exact weighted one-norm of the normalized circulation.
    pub normalized_weighted_length: BigRational,
    /// Flow after subtracting the normalized circulation.
    pub final_flow: Vec<BigRational>,
    /// Absolute movement after adding the coordinate-wise absolute circulation.
    pub final_movement: Vec<BigRational>,
    /// Active stable slots satisfying the source detection inequality.
    pub detectable_edges: Vec<usize>,
    /// Exact bounded data-structure work.
    pub metrics: BoundedLinkCutFlowMetrics,
}

/// Bounded link-cut source-interface failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BoundedLinkCutFlowError {
    /// Graph, stable vectors, circulation, or threshold is malformed.
    #[error("bounded link-cut flow input is invalid")]
    InvalidInput,
    /// The common exact integer scale exceeds the published bounded band.
    #[error("bounded link-cut flow scale exceeds its admission band")]
    AdmissionLimit,
    /// One link-cut operation failed.
    #[error("bounded link-cut flow data structure failed")]
    LinkCut,
    /// A flow, movement, gradient, length, or detection invariant failed.
    #[error("bounded link-cut flow invariant failed")]
    InvariantViolation,
    /// Checked work arithmetic overflowed.
    #[error("bounded link-cut flow arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied certificate differs from independent explicit reconstruction.
    #[error("bounded link-cut flow certificate verification failed")]
    TraceVerification,
}

struct RootedForest {
    tree_edges: Vec<usize>,
    parent: Vec<Option<usize>>,
    parent_edge: Vec<Option<usize>>,
    parent_direction: Vec<i8>,
}

struct ScalarForests {
    gradient: LinkCutForest,
    length: LinkCutForest,
    flow: LinkCutForest,
    movement: LinkCutForest,
    vertices: Vec<DynamicTreeVertex>,
    edges: Vec<DynamicTreeEdge>,
}

struct FinalVectorInput<'a> {
    before_flow: &'a [BigRational],
    before_movement: &'a [BigRational],
    delta: &'a [BigRational],
    scale: &'a BigInt,
}

/// Applies one normalized root circulation through the bounded link-cut
/// interface and returns an independently checkable certificate.
///
/// # Errors
///
/// Rejects malformed graph/vector shapes, a non-circulation, nonpositive
/// lengths or threshold, excessive rational scale, link-cut failure, or any
/// mismatch against the exact stable-coordinate result.
pub fn apply_bounded_link_cut_flow(
    graph: &DynamicLevelGraphSnapshot,
    before_flow: &[BigRational],
    before_movement: &[BigRational],
    normalized_delta: &[BigRational],
    epsilon: &BigRational,
) -> Result<BoundedLinkCutFlowCertificate, BoundedLinkCutFlowError> {
    validate_input(
        graph,
        before_flow,
        before_movement,
        normalized_delta,
        epsilon,
    )?;
    let rooted = canonical_rooted_forest(graph);
    let scale = common_scale(
        graph,
        before_flow,
        before_movement,
        normalized_delta,
        epsilon,
    )?;
    let (mut forests, mut metrics) =
        build_scalar_forests(graph, &rooted, before_flow, before_movement, &scale)?;
    let (gradient_dot, weighted_length) = query_objective(
        graph,
        &rooted,
        &mut forests,
        normalized_delta,
        &scale,
        &mut metrics,
    )?;
    apply_coordinates(
        graph,
        &rooted,
        &mut forests,
        normalized_delta,
        &scale,
        &mut metrics,
    )?;
    let (final_flow, final_movement) = read_final_vectors(
        graph,
        &rooted,
        &mut forests,
        &FinalVectorInput {
            before_flow,
            before_movement,
            delta: normalized_delta,
            scale: &scale,
        },
        &mut metrics,
    )?;
    let detectable_edges = detect_edges(graph, &final_movement, epsilon);
    metrics.detect_scans = u64::try_from(graph.edge_slots.iter().flatten().count())
        .map_err(|_| BoundedLinkCutFlowError::ArithmeticOverflow)?;
    let certificate = BoundedLinkCutFlowCertificate {
        tree_edges: rooted.tree_edges,
        scale,
        normalized_gradient_dot: gradient_dot,
        normalized_weighted_length: weighted_length,
        final_flow,
        final_movement,
        detectable_edges,
        metrics,
    };
    check_bounded_link_cut_flow_certificate(
        graph,
        before_flow,
        before_movement,
        normalized_delta,
        epsilon,
        &certificate,
    )?;
    Ok(certificate)
}

/// Independently checks stable coordinates, objective values, tree identity,
/// detection, scale divisibility, and deterministic operation counts.
///
/// # Errors
///
/// Rejects any supplied certificate field that differs from an explicit graph
/// reconstruction. This checker does not invoke the link-cut runtime.
pub fn check_bounded_link_cut_flow_certificate(
    graph: &DynamicLevelGraphSnapshot,
    before_flow: &[BigRational],
    before_movement: &[BigRational],
    normalized_delta: &[BigRational],
    epsilon: &BigRational,
    certificate: &BoundedLinkCutFlowCertificate,
) -> Result<(), BoundedLinkCutFlowError> {
    validate_input(
        graph,
        before_flow,
        before_movement,
        normalized_delta,
        epsilon,
    )
    .map_err(audit_error)?;
    let rooted = canonical_rooted_forest(graph);
    let scale = common_scale(
        graph,
        before_flow,
        before_movement,
        normalized_delta,
        epsilon,
    )
    .map_err(audit_error)?;
    let gradient_dot = graph
        .edge_slots
        .iter()
        .zip(normalized_delta)
        .filter_map(|(row, delta)| row.as_ref().map(|row| &row.gradient * delta))
        .fold(BigRational::zero(), |sum, value| sum + value);
    let weighted_length = graph
        .edge_slots
        .iter()
        .zip(normalized_delta)
        .filter_map(|(row, delta)| row.as_ref().map(|row| &row.length * delta.abs()))
        .fold(BigRational::zero(), |sum, value| sum + value);
    let final_flow = before_flow
        .iter()
        .zip(normalized_delta)
        .map(|(flow, delta)| flow - delta)
        .collect::<Vec<_>>();
    let final_movement = before_movement
        .iter()
        .zip(normalized_delta)
        .map(|(movement, delta)| movement + delta.abs())
        .collect::<Vec<_>>();
    let detectable_edges = detect_edges(graph, &final_movement, epsilon);
    let active = graph.edge_slots.iter().flatten().count();
    let tree = rooted.tree_edges.len();
    let expected_metrics = BoundedLinkCutFlowMetrics {
        links: checked_u64(tree.checked_mul(4).ok_or_else(audit_failure)?)?,
        gradient_root_path_sums: checked_u64(tree.checked_mul(2).ok_or_else(audit_failure)?)?,
        length_path_sums: checked_u64(tree)?,
        flow_root_path_adds: checked_u64(tree.checked_mul(2).ok_or_else(audit_failure)?)?,
        movement_path_adds: checked_u64(tree)?,
        edge_value_queries: checked_u64(tree.checked_mul(2).ok_or_else(audit_failure)?)?,
        detect_scans: checked_u64(active)?,
    };
    if certificate.tree_edges != rooted.tree_edges
        || certificate.scale != scale
        || certificate.normalized_gradient_dot != gradient_dot
        || certificate.normalized_weighted_length != weighted_length
        || certificate.final_flow != final_flow
        || certificate.final_movement != final_movement
        || certificate.detectable_edges != detectable_edges
        || certificate.metrics != expected_metrics
    {
        return Err(BoundedLinkCutFlowError::TraceVerification);
    }
    Ok(())
}

fn validate_input(
    graph: &DynamicLevelGraphSnapshot,
    before_flow: &[BigRational],
    before_movement: &[BigRational],
    normalized_delta: &[BigRational],
    epsilon: &BigRational,
) -> Result<(), BoundedLinkCutFlowError> {
    let slots = graph.edge_slots.len();
    if graph.active_node_count == 0
        || before_flow.len() != slots
        || before_movement.len() != slots
        || normalized_delta.len() != slots
        || epsilon <= &BigRational::zero()
        || before_movement.iter().any(Signed::is_negative)
    {
        return Err(BoundedLinkCutFlowError::InvalidInput);
    }
    let mut divergence = vec![BigRational::zero(); graph.active_node_count];
    for (edge, (row, delta)) in graph.edge_slots.iter().zip(normalized_delta).enumerate() {
        match row {
            Some(row) => {
                if row.edge != edge
                    || row.from >= graph.active_node_count
                    || row.to >= graph.active_node_count
                    || row.length <= BigRational::zero()
                {
                    return Err(BoundedLinkCutFlowError::InvalidInput);
                }
                divergence[row.from] -= delta;
                divergence[row.to] += delta;
            }
            None if !delta.is_zero() => {
                return Err(BoundedLinkCutFlowError::InvalidInput);
            }
            None => {}
        }
    }
    if divergence.iter().any(|value| !value.is_zero()) {
        return Err(BoundedLinkCutFlowError::InvalidInput);
    }
    Ok(())
}

fn canonical_rooted_forest(graph: &DynamicLevelGraphSnapshot) -> RootedForest {
    let mut parent_set = (0..graph.active_node_count).collect::<Vec<_>>();
    let mut tree_edges = Vec::new();
    for row in graph.edge_slots.iter().flatten() {
        if row.from == row.to {
            continue;
        }
        let left = find_set(&mut parent_set, row.from);
        let right = find_set(&mut parent_set, row.to);
        if left != right {
            parent_set[right] = left;
            tree_edges.push(row.edge);
        }
    }
    let support = tree_edges.iter().copied().collect::<BTreeSet<_>>();
    let mut parent = vec![None; graph.active_node_count];
    let mut parent_edge = vec![None; graph.active_node_count];
    let mut parent_direction = vec![1_i8; graph.active_node_count];
    let mut visited = vec![false; graph.active_node_count];
    for root in 0..graph.active_node_count {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut queue = VecDeque::from([root]);
        while let Some(vertex) = queue.pop_front() {
            for row in graph.edge_slots.iter().flatten() {
                if !support.contains(&row.edge) {
                    continue;
                }
                let (next, direction) = if row.from == vertex {
                    (row.to, 1)
                } else if row.to == vertex {
                    (row.from, -1)
                } else {
                    continue;
                };
                if !visited[next] {
                    visited[next] = true;
                    parent[next] = Some(vertex);
                    parent_edge[next] = Some(row.edge);
                    parent_direction[next] = direction;
                    queue.push_back(next);
                }
            }
        }
    }
    RootedForest {
        tree_edges,
        parent,
        parent_edge,
        parent_direction,
    }
}

fn find_set(parent: &mut [usize], vertex: usize) -> usize {
    if parent[vertex] != vertex {
        parent[vertex] = find_set(parent, parent[vertex]);
    }
    parent[vertex]
}

fn common_scale(
    graph: &DynamicLevelGraphSnapshot,
    before_flow: &[BigRational],
    before_movement: &[BigRational],
    normalized_delta: &[BigRational],
    epsilon: &BigRational,
) -> Result<BigInt, BoundedLinkCutFlowError> {
    let mut scale = BigInt::one();
    for value in graph
        .edge_slots
        .iter()
        .flatten()
        .flat_map(|row| [&row.gradient, &row.length])
        .chain(before_flow)
        .chain(before_movement)
        .chain(normalized_delta)
        .chain([epsilon])
    {
        scale = lcm_positive(&scale, value.denom())?;
        if scale.bits() > MAX_SCALE_BITS {
            return Err(BoundedLinkCutFlowError::AdmissionLimit);
        }
    }
    Ok(scale)
}

fn lcm_positive(left: &BigInt, right: &BigInt) -> Result<BigInt, BoundedLinkCutFlowError> {
    if left <= &BigInt::zero() || right <= &BigInt::zero() {
        return Err(BoundedLinkCutFlowError::InvalidInput);
    }
    Ok((left / gcd(left.clone(), right.clone())) * right)
}

fn gcd(mut left: BigInt, mut right: BigInt) -> BigInt {
    while !right.is_zero() {
        let remainder = left % &right;
        left = right;
        right = remainder;
    }
    left.abs()
}

fn encode(value: &BigRational, scale: &BigInt) -> Result<BigInt, BoundedLinkCutFlowError> {
    if scale % value.denom() != BigInt::zero() {
        return Err(BoundedLinkCutFlowError::InvariantViolation);
    }
    Ok(value.numer() * (scale / value.denom()))
}

fn decode(value: BigInt, scale: &BigInt) -> BigRational {
    BigRational::new(value, scale.clone())
}

fn build_scalar_forests(
    graph: &DynamicLevelGraphSnapshot,
    rooted: &RootedForest,
    before_flow: &[BigRational],
    before_movement: &[BigRational],
    scale: &BigInt,
) -> Result<(ScalarForests, BoundedLinkCutFlowMetrics), BoundedLinkCutFlowError> {
    let nodes = graph.active_node_count;
    let slots = graph.edge_slots.len();
    let gradient = LinkCutForest::new(nodes, slots);
    let vertices = (0..nodes)
        .map(|index| gradient.vertex(index))
        .collect::<Result<Vec<_>, _>>()?;
    let edges = (0..slots)
        .map(|index| gradient.edge(index))
        .collect::<Result<Vec<_>, _>>()?;
    let mut forests = ScalarForests {
        gradient,
        length: LinkCutForest::new(nodes, slots),
        flow: LinkCutForest::new(nodes, slots),
        movement: LinkCutForest::new(nodes, slots),
        vertices,
        edges,
    };
    let mut children = (0..nodes)
        .filter(|&vertex| rooted.parent[vertex].is_some())
        .collect::<Vec<_>>();
    children.sort_by_key(|&child| depth(rooted, child));
    for child in children {
        let parent = rooted.parent[child].ok_or(BoundedLinkCutFlowError::InvariantViolation)?;
        let edge = rooted.parent_edge[child].ok_or(BoundedLinkCutFlowError::InvariantViolation)?;
        let direction = BigInt::from(rooted.parent_direction[child]);
        let row = graph.edge_slots[edge]
            .as_ref()
            .ok_or(BoundedLinkCutFlowError::InvariantViolation)?;
        let values = [
            encode(&row.gradient, scale)? * &direction,
            encode(&row.length, scale)?,
            encode(&before_flow[edge], scale)? * &direction,
            encode(&before_movement[edge], scale)?,
        ];
        link_all(&mut forests, edge, child, parent, values)?;
    }
    let links = rooted
        .tree_edges
        .len()
        .checked_mul(4)
        .ok_or(BoundedLinkCutFlowError::ArithmeticOverflow)?;
    Ok((
        forests,
        BoundedLinkCutFlowMetrics {
            links: checked_u64(links)?,
            ..BoundedLinkCutFlowMetrics::default()
        },
    ))
}

fn depth(rooted: &RootedForest, mut vertex: usize) -> usize {
    let mut depth = 0;
    while let Some(parent) = rooted.parent[vertex] {
        depth += 1;
        vertex = parent;
    }
    depth
}

fn link_all(
    forests: &mut ScalarForests,
    edge: usize,
    child: usize,
    parent: usize,
    values: [BigInt; 4],
) -> Result<(), BoundedLinkCutFlowError> {
    let slot = forests.edges[edge];
    let child = forests.vertices[child];
    let parent = forests.vertices[parent];
    let [gradient, length, flow, movement] = values;
    forests
        .gradient
        .link_rooted(slot, child, parent, gradient)?;
    forests.length.link_rooted(slot, child, parent, length)?;
    forests.flow.link_rooted(slot, child, parent, flow)?;
    forests
        .movement
        .link_rooted(slot, child, parent, movement)?;
    Ok(())
}

fn query_objective(
    graph: &DynamicLevelGraphSnapshot,
    rooted: &RootedForest,
    forests: &mut ScalarForests,
    delta: &[BigRational],
    scale: &BigInt,
    metrics: &mut BoundedLinkCutFlowMetrics,
) -> Result<(BigRational, BigRational), BoundedLinkCutFlowError> {
    let tree = rooted.tree_edges.iter().copied().collect::<BTreeSet<_>>();
    let mut gradient_dot = BigRational::zero();
    let mut weighted_length = BigRational::zero();
    for (edge, row) in graph.edge_slots.iter().enumerate() {
        let Some(row) = row else { continue };
        if tree.contains(&edge) {
            let child = rooted
                .parent_edge
                .iter()
                .position(|candidate| *candidate == Some(edge))
                .ok_or(BoundedLinkCutFlowError::InvariantViolation)?;
            let parent = rooted.parent[child].ok_or(BoundedLinkCutFlowError::InvariantViolation)?;
            let child_gradient = forests.gradient.root_path_sum(forests.vertices[child])?;
            let parent_gradient = forests.gradient.root_path_sum(forests.vertices[parent])?;
            let directed_gradient = decode(child_gradient - parent_gradient, scale);
            let stable_gradient = directed_gradient * BigInt::from(rooted.parent_direction[child]);
            let path_length = decode(
                forests
                    .length
                    .path_sum(forests.vertices[parent], forests.vertices[child])?,
                scale,
            );
            gradient_dot += stable_gradient * &delta[edge];
            weighted_length += path_length * delta[edge].abs();
        } else {
            gradient_dot += &row.gradient * &delta[edge];
            weighted_length += &row.length * delta[edge].abs();
        }
    }
    let tree_count = rooted.tree_edges.len();
    metrics.gradient_root_path_sums = checked_u64(
        tree_count
            .checked_mul(2)
            .ok_or(BoundedLinkCutFlowError::ArithmeticOverflow)?,
    )?;
    metrics.length_path_sums = checked_u64(tree_count)?;
    Ok((gradient_dot, weighted_length))
}

fn apply_coordinates(
    graph: &DynamicLevelGraphSnapshot,
    rooted: &RootedForest,
    forests: &mut ScalarForests,
    delta: &[BigRational],
    scale: &BigInt,
    metrics: &mut BoundedLinkCutFlowMetrics,
) -> Result<(), BoundedLinkCutFlowError> {
    for child in 0..graph.active_node_count {
        let (Some(parent), Some(edge)) = (rooted.parent[child], rooted.parent_edge[child]) else {
            continue;
        };
        let stable_change = -&delta[edge];
        let parent_direction = BigInt::from(rooted.parent_direction[child]);
        let rooted_change = encode(&stable_change, scale)? * parent_direction;
        forests
            .flow
            .root_path_add(forests.vertices[child], &rooted_change)?;
        forests
            .flow
            .root_path_add(forests.vertices[parent], &(-&rooted_change))?;
        forests.movement.path_add(
            forests.vertices[parent],
            forests.vertices[child],
            &encode(&delta[edge].abs(), scale)?,
        )?;
    }
    let tree_count = rooted.tree_edges.len();
    metrics.flow_root_path_adds = checked_u64(
        tree_count
            .checked_mul(2)
            .ok_or(BoundedLinkCutFlowError::ArithmeticOverflow)?,
    )?;
    metrics.movement_path_adds = checked_u64(tree_count)?;
    Ok(())
}

fn read_final_vectors(
    graph: &DynamicLevelGraphSnapshot,
    rooted: &RootedForest,
    forests: &mut ScalarForests,
    input: &FinalVectorInput<'_>,
    metrics: &mut BoundedLinkCutFlowMetrics,
) -> Result<(Vec<BigRational>, Vec<BigRational>), BoundedLinkCutFlowError> {
    let mut final_flow = input
        .before_flow
        .iter()
        .zip(input.delta)
        .map(|(flow, delta)| flow - delta)
        .collect::<Vec<_>>();
    let mut final_movement = input
        .before_movement
        .iter()
        .zip(input.delta)
        .map(|(movement, delta)| movement + delta.abs())
        .collect::<Vec<_>>();
    for child in 0..graph.active_node_count {
        let Some(edge) = rooted.parent_edge[child] else {
            continue;
        };
        let direction = BigInt::from(rooted.parent_direction[child]);
        final_flow[edge] =
            decode(forests.flow.edge_value(forests.edges[edge])?, input.scale) * direction;
        final_movement[edge] = decode(
            forests.movement.edge_value(forests.edges[edge])?,
            input.scale,
        );
    }
    let queries = rooted
        .tree_edges
        .len()
        .checked_mul(2)
        .ok_or(BoundedLinkCutFlowError::ArithmeticOverflow)?;
    metrics.edge_value_queries = checked_u64(queries)?;
    for edge in 0..graph.edge_slots.len() {
        let expected_flow = &input.before_flow[edge] - &input.delta[edge];
        let expected_movement = &input.before_movement[edge] + input.delta[edge].abs();
        if final_flow[edge] != expected_flow || final_movement[edge] != expected_movement {
            return Err(BoundedLinkCutFlowError::InvariantViolation);
        }
    }
    Ok((final_flow, final_movement))
}

fn detect_edges(
    graph: &DynamicLevelGraphSnapshot,
    movement: &[BigRational],
    epsilon: &BigRational,
) -> Vec<usize> {
    graph
        .edge_slots
        .iter()
        .enumerate()
        .filter_map(|(edge, row)| {
            row.as_ref()
                .filter(|row| &row.length * &movement[edge] >= *epsilon)
                .map(|_| edge)
        })
        .collect()
}

fn checked_u64(value: usize) -> Result<u64, BoundedLinkCutFlowError> {
    u64::try_from(value).map_err(|_| BoundedLinkCutFlowError::ArithmeticOverflow)
}

fn audit_failure() -> BoundedLinkCutFlowError {
    BoundedLinkCutFlowError::TraceVerification
}

fn audit_error(error: BoundedLinkCutFlowError) -> BoundedLinkCutFlowError {
    match error {
        BoundedLinkCutFlowError::InvalidInput | BoundedLinkCutFlowError::AdmissionLimit => error,
        _ => BoundedLinkCutFlowError::TraceVerification,
    }
}

impl From<LinkCutError> for BoundedLinkCutFlowError {
    fn from(_: LinkCutError) -> Self {
        Self::LinkCut
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::DynamicLevelEdge;

    fn rational(numerator: i64, denominator: i64) -> BigRational {
        BigRational::new(numerator.into(), denominator.into())
    }

    fn graph() -> DynamicLevelGraphSnapshot {
        DynamicLevelGraphSnapshot {
            active_node_count: 4,
            edge_slots: vec![
                Some(DynamicLevelEdge {
                    edge: 0,
                    from: 0,
                    to: 1,
                    length: rational(1, 2),
                    gradient: rational(2, 3),
                }),
                Some(DynamicLevelEdge {
                    edge: 1,
                    from: 1,
                    to: 2,
                    length: rational(3, 2),
                    gradient: rational(-1, 3),
                }),
                Some(DynamicLevelEdge {
                    edge: 2,
                    from: 2,
                    to: 0,
                    length: rational(2, 1),
                    gradient: rational(-2, 1),
                }),
                None,
                Some(DynamicLevelEdge {
                    edge: 4,
                    from: 2,
                    to: 3,
                    length: rational(1, 1),
                    gradient: rational(5, 1),
                }),
            ],
            stage: 0,
        }
    }

    #[test]
    fn exact_rational_tree_interface_matches_explicit_cycle_and_detect() {
        let graph = graph();
        let before_flow = vec![
            rational(1, 4),
            rational(1, 2),
            rational(3, 4),
            rational(0, 1),
            rational(2, 1),
        ];
        let before_movement = vec![BigRational::zero(); 5];
        let delta = vec![
            rational(1, 3),
            rational(1, 3),
            rational(1, 3),
            rational(0, 1),
            rational(0, 1),
        ];
        let certificate = apply_bounded_link_cut_flow(
            &graph,
            &before_flow,
            &before_movement,
            &delta,
            &rational(1, 4),
        )
        .expect("link-cut application");
        assert_eq!(certificate.scale, BigInt::from(12));
        assert_eq!(certificate.normalized_gradient_dot, rational(-5, 9));
        assert_eq!(certificate.normalized_weighted_length, rational(4, 3));
        assert_eq!(certificate.detectable_edges, vec![1, 2]);
        check_bounded_link_cut_flow_certificate(
            &graph,
            &before_flow,
            &before_movement,
            &delta,
            &rational(1, 4),
            &certificate,
        )
        .expect("certificate");

        let mut forged = certificate;
        forged.metrics.flow_root_path_adds += 1;
        assert_eq!(
            check_bounded_link_cut_flow_certificate(
                &graph,
                &before_flow,
                &before_movement,
                &delta,
                &rational(1, 4),
                &forged,
            ),
            Err(BoundedLinkCutFlowError::TraceVerification)
        );
    }
}
