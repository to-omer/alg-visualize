//! Checked initializer from the source MWU forest collection to dynamic cores.
//!
//! Lemma 5.5 selects `k` rooted low-stretch forests, while the dynamic
//! tree-chain primitives consume one [`DynamicSparseCoreCollectionInput`] per
//! level. This module makes that previously implicit boundary executable. It
//! preserves stable source-edge slots (including holes), uses the selected
//! spanning tree as each branch's fixed reference tree, and initializes the
//! dynamic forest with the exact source-certified stretch vector.
//!
//! MWU selection first minimizes the source normalized-copy weighted tree
//! stretch, then materializes that tree's HLD-refined Dynamic LSF. The bridge
//! preserves its explicit reference root and seed set, and checks exact
//! equality of the selected forest, component roots, and stretch upper bounds
//! across the dynamic initializer. It also rechecks the bounded measured-LSST
//! threshold, large-stretch endpoint seeds, finite tree-decomposition volume,
//! and published component partition. The source hides decomposition constants,
//! so this finite stronger contract does not claim its asymptotic bound,
//! dynamic spanner recourse, or almost-linear runtime.

use std::collections::BTreeSet;

use num_bigint::BigInt;
use num_rational::BigRational;
use thiserror::Error;

use super::{
    DYNAMIC_SPARSE_CORE_COLLECTION_MAX_BRANCHES, DYNAMIC_SPARSE_CORE_MAX_EDGES,
    DYNAMIC_SPARSE_CORE_MAX_NODES, DynamicCoreGraphInput, DynamicLowStretchForestEdge,
    DynamicLowStretchForestInput, DynamicSparseCoreCollectionError,
    DynamicSparseCoreCollectionInput, DynamicSparseCoreCollectionResult,
    DynamicSparseCoreCollectionStageTraceResult, DynamicSparseCoreInput, LowStretchForestMwuBranch,
    LowStretchForestMwuConfig, LowStretchForestMwuError, LowStretchForestMwuResult,
    LowStretchForestMwuTraceResult, LowStretchForestTreePiece, ShiftedTreeChainGraph,
    build_low_stretch_forest_mwu_collection, check_dynamic_sparse_core_collection_stage_trace,
    check_low_stretch_forest_mwu_trace, execute_dynamic_sparse_core_collection_stages,
    trace_dynamic_sparse_core_collection_stages, trace_low_stretch_forest_mwu_collection,
};

/// Bounded stable universes used by one initialized dynamic level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicMwuCollectionBridgeConfig {
    /// Positive MWU round count and resulting branch count `k`.
    pub branches: usize,
    /// Stable vertex universe available to later split operations.
    pub maximum_node_count: usize,
    /// Stable edge-slot universe; source edge IDs index this universe.
    pub stable_edge_slots: usize,
}

/// Exact MWU output, converted collection input, and checked initialized state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicMwuCollectionBridgeResult {
    /// Source-certified MWU result in round order.
    pub mwu: LowStretchForestMwuResult,
    /// Exact dynamic collection initialized from `mwu`.
    pub collection: DynamicSparseCoreCollectionInput,
    /// Completed zero-stage initialization of every dynamic branch.
    pub initialized: DynamicSparseCoreCollectionResult,
}

/// Independently checkable transcripts on both sides of the initializer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicMwuCollectionBridgeTraceResult {
    /// Source MWU selections and weight updates.
    pub mwu_trace: LowStretchForestMwuTraceResult,
    /// Zero-stage dynamic collection initialization and component transcripts.
    pub collection_trace: DynamicSparseCoreCollectionStageTraceResult,
    /// Exact terminal bridge result.
    pub result: DynamicMwuCollectionBridgeResult,
}

/// Explicit bounded initializer failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicMwuCollectionBridgeError {
    /// Stable universes or source IDs are malformed.
    #[error("dynamic MWU collection bridge input is invalid")]
    InvalidInput,
    /// The request exceeds the shared explicit small-graph band.
    #[error("dynamic MWU collection bridge exceeds its admission band")]
    AdmissionLimit,
    /// The certified MWU construction failed.
    #[error("dynamic MWU collection bridge MWU failed: {0}")]
    Mwu(#[from] LowStretchForestMwuError),
    /// The initialized dynamic sparse-core collection failed.
    #[error("dynamic MWU collection bridge collection failed: {0}")]
    Collection(#[from] DynamicSparseCoreCollectionError),
    /// A selected mask, root, forest, or stretch does not cross the boundary.
    #[error("dynamic MWU collection bridge invariant failed")]
    InvariantViolation,
    /// A supplied pair of transcripts is not the exact checked conversion.
    #[error("dynamic MWU collection bridge trace verification failed")]
    TraceVerification,
}

/// Builds and initializes the dynamic branch collection without bridge traces.
///
/// # Errors
///
/// Rejects malformed/out-of-band stable universes, MWU failure, invalid branch
/// conversion, or dynamic sparse-core initialization failure.
pub fn build_dynamic_mwu_sparse_core_collection(
    graph: &ShiftedTreeChainGraph,
    config: DynamicMwuCollectionBridgeConfig,
) -> Result<DynamicMwuCollectionBridgeResult, DynamicMwuCollectionBridgeError> {
    validate_bridge_input(graph, config)?;
    let mwu = build_low_stretch_forest_mwu_collection(
        graph,
        LowStretchForestMwuConfig {
            rounds: config.branches,
        },
    )?;
    let collection = convert_collection(graph, config, &mwu.branches)?;
    let initialized = execute_dynamic_sparse_core_collection_stages(&collection, &[])?;
    Ok(DynamicMwuCollectionBridgeResult {
        mwu,
        collection,
        initialized,
    })
}

/// Records and checks the MWU selections and zero-stage dynamic initialization.
///
/// # Errors
///
/// Returns any input, MWU, collection, cross-contract, or replay-check failure.
pub fn trace_dynamic_mwu_sparse_core_collection(
    graph: &ShiftedTreeChainGraph,
    config: DynamicMwuCollectionBridgeConfig,
) -> Result<DynamicMwuCollectionBridgeTraceResult, DynamicMwuCollectionBridgeError> {
    validate_bridge_input(graph, config)?;
    let mwu_trace = trace_low_stretch_forest_mwu_collection(
        graph,
        LowStretchForestMwuConfig {
            rounds: config.branches,
        },
    )?;
    let collection = convert_collection(graph, config, &mwu_trace.result.branches)?;
    let collection_trace = trace_dynamic_sparse_core_collection_stages(&collection, &[])?;
    let trace = DynamicMwuCollectionBridgeTraceResult {
        result: DynamicMwuCollectionBridgeResult {
            mwu: mwu_trace.result.clone(),
            collection,
            initialized: collection_trace.result.clone(),
        },
        mwu_trace,
        collection_trace,
    };
    check_dynamic_mwu_sparse_core_collection_trace(graph, config, &trace)?;
    Ok(trace)
}

/// Independently checks both component transcripts and their exact conversion.
///
/// This checker never calls either bridge construction path. It asks the MWU
/// and dynamic collection checkers to replay their own mathematics, rebuilds
/// the collection input with an audit conversion, then compares the selected
/// masks, component roots, and stretches with every dynamic forest snapshot.
///
/// # Errors
///
/// Rejects component trace drift, stable-slot remapping, mask/root/stretch
/// refinement/stretch mismatch, or a forged terminal result.
pub fn check_dynamic_mwu_sparse_core_collection_trace(
    graph: &ShiftedTreeChainGraph,
    config: DynamicMwuCollectionBridgeConfig,
    trace: &DynamicMwuCollectionBridgeTraceResult,
) -> Result<(), DynamicMwuCollectionBridgeError> {
    validate_bridge_input(graph, config)?;
    let mwu_config = LowStretchForestMwuConfig {
        rounds: config.branches,
    };
    check_low_stretch_forest_mwu_trace(graph, mwu_config, &trace.mwu_trace)?;
    let expected_collection =
        audit_convert_collection(graph, config, &trace.mwu_trace.result.branches)?;
    if trace.result.mwu != trace.mwu_trace.result || trace.result.collection != expected_collection
    {
        return Err(DynamicMwuCollectionBridgeError::TraceVerification);
    }
    check_dynamic_sparse_core_collection_stage_trace(
        &expected_collection,
        &[],
        &trace.collection_trace,
    )?;
    if trace.result.initialized != trace.collection_trace.result {
        return Err(DynamicMwuCollectionBridgeError::TraceVerification);
    }
    audit_cross_contract(
        graph,
        &trace.mwu_trace.result.branches,
        &expected_collection,
        &trace.collection_trace,
    )
}

fn validate_bridge_input(
    graph: &ShiftedTreeChainGraph,
    config: DynamicMwuCollectionBridgeConfig,
) -> Result<(), DynamicMwuCollectionBridgeError> {
    if config.branches == 0
        || config.maximum_node_count < graph.node_count
        || config.stable_edge_slots == 0
    {
        return Err(DynamicMwuCollectionBridgeError::InvalidInput);
    }
    if config.branches > DYNAMIC_SPARSE_CORE_COLLECTION_MAX_BRANCHES
        || config.maximum_node_count > DYNAMIC_SPARSE_CORE_MAX_NODES
        || config.stable_edge_slots > DYNAMIC_SPARSE_CORE_MAX_EDGES
    {
        return Err(DynamicMwuCollectionBridgeError::AdmissionLimit);
    }
    let mut source_ids = BTreeSet::new();
    for edge in &graph.edges {
        if edge.source_edge >= config.stable_edge_slots || !source_ids.insert(edge.source_edge) {
            return Err(DynamicMwuCollectionBridgeError::InvalidInput);
        }
    }
    Ok(())
}

fn convert_collection(
    graph: &ShiftedTreeChainGraph,
    config: DynamicMwuCollectionBridgeConfig,
    branches: &[LowStretchForestMwuBranch],
) -> Result<DynamicSparseCoreCollectionInput, DynamicMwuCollectionBridgeError> {
    if branches.len() != config.branches {
        return Err(DynamicMwuCollectionBridgeError::InvariantViolation);
    }
    let mut edge_slots = vec![None; config.stable_edge_slots];
    let mut initial_gradients = vec![None; config.stable_edge_slots];
    for edge in &graph.edges {
        edge_slots[edge.source_edge] = Some(DynamicLowStretchForestEdge {
            edge: edge.source_edge,
            from: edge.from,
            to: edge.to,
            length: edge.length.clone(),
        });
        initial_gradients[edge.source_edge] = Some(edge.gradient.clone());
    }
    let mut converted = Vec::with_capacity(branches.len());
    for branch in branches {
        converted.push(convert_branch(
            graph,
            config,
            branch,
            edge_slots.clone(),
            initial_gradients.clone(),
        )?);
    }
    Ok(DynamicSparseCoreCollectionInput {
        branches: converted,
    })
}

fn convert_branch(
    graph: &ShiftedTreeChainGraph,
    config: DynamicMwuCollectionBridgeConfig,
    branch: &LowStretchForestMwuBranch,
    edge_slots: Vec<Option<DynamicLowStretchForestEdge>>,
    initial_gradients: Vec<Option<BigRational>>,
) -> Result<DynamicSparseCoreInput, DynamicMwuCollectionBridgeError> {
    validate_branch_shape(graph, config, branch)?;
    let reference_tree_edges = mask_source_edges(graph, branch.tree_mask)?;
    let mut overestimates = vec![None; config.stable_edge_slots];
    for (edge, stretch) in graph.edges.iter().zip(&branch.stretch_overestimates) {
        overestimates[edge.source_edge] = Some(stretch.clone());
    }
    Ok(DynamicSparseCoreInput {
        core: DynamicCoreGraphInput {
            forest: DynamicLowStretchForestInput {
                initial_node_count: graph.node_count,
                maximum_node_count: config.maximum_node_count,
                edge_slots,
                reference_tree_edges,
                reference_root: branch.reference_root,
                initial_root_seeds: branch.root_seeds.clone(),
                initial_stretch_overestimates: Some(overestimates),
            },
            initial_gradients,
        },
        branches: config.branches,
    })
}

// Deliberately separate from the production conversion: the checker fills
// stable arrays by slot and derives masks with direct index scans.
fn audit_convert_collection(
    graph: &ShiftedTreeChainGraph,
    config: DynamicMwuCollectionBridgeConfig,
    branches: &[LowStretchForestMwuBranch],
) -> Result<DynamicSparseCoreCollectionInput, DynamicMwuCollectionBridgeError> {
    if branches.len() != config.branches {
        return Err(DynamicMwuCollectionBridgeError::TraceVerification);
    }
    let mut edge_slots = vec![None; config.stable_edge_slots];
    let mut gradients = vec![None; config.stable_edge_slots];
    for edge in &graph.edges {
        if edge_slots[edge.source_edge].is_some() {
            return Err(DynamicMwuCollectionBridgeError::TraceVerification);
        }
        edge_slots[edge.source_edge] = Some(DynamicLowStretchForestEdge {
            edge: edge.source_edge,
            from: edge.from,
            to: edge.to,
            length: edge.length.clone(),
        });
        gradients[edge.source_edge] = Some(edge.gradient.clone());
    }
    let mut result = Vec::with_capacity(branches.len());
    for branch in branches {
        audit_validate_branch_shape(graph, config, branch)?;
        let reference_tree_edges = graph
            .edges
            .iter()
            .enumerate()
            .filter_map(|(index, edge)| {
                (branch.tree_mask & (1_u64 << index) != 0).then_some(edge.source_edge)
            })
            .collect::<Vec<_>>();
        let mut overestimates = vec![None; config.stable_edge_slots];
        for index in 0..graph.edges.len() {
            overestimates[graph.edges[index].source_edge] =
                Some(branch.stretch_overestimates[index].clone());
        }
        result.push(DynamicSparseCoreInput {
            core: DynamicCoreGraphInput {
                forest: DynamicLowStretchForestInput {
                    initial_node_count: graph.node_count,
                    maximum_node_count: config.maximum_node_count,
                    edge_slots: edge_slots.clone(),
                    reference_tree_edges,
                    reference_root: branch.reference_root,
                    initial_root_seeds: branch.root_seeds.clone(),
                    initial_stretch_overestimates: Some(overestimates),
                },
                initial_gradients: gradients.clone(),
            },
            branches: config.branches,
        });
    }
    Ok(DynamicSparseCoreCollectionInput { branches: result })
}

fn validate_branch_shape(
    graph: &ShiftedTreeChainGraph,
    config: DynamicMwuCollectionBridgeConfig,
    branch: &LowStretchForestMwuBranch,
) -> Result<(), DynamicMwuCollectionBridgeError> {
    let edge_bits = valid_edge_bits(graph.edges.len())?;
    if branch.tree_mask & !edge_bits != 0
        || branch.forest_mask & !branch.tree_mask != 0
        || branch.tree_mask.count_ones() as usize != graph.node_count - 1
        || branch.reference_root >= graph.node_count
        || branch.root_seeds.is_empty()
        || branch.root_seeds.windows(2).any(|pair| pair[0] >= pair[1])
        || branch
            .root_seeds
            .iter()
            .any(|&root| root >= graph.node_count)
        || !branch_lsst_contract(graph, config, branch)
        || branch.roots.len() != graph.node_count
        || branch.stretch_overestimates.len() != graph.edges.len()
    {
        return Err(DynamicMwuCollectionBridgeError::InvariantViolation);
    }
    Ok(())
}

fn audit_validate_branch_shape(
    graph: &ShiftedTreeChainGraph,
    config: DynamicMwuCollectionBridgeConfig,
    branch: &LowStretchForestMwuBranch,
) -> Result<(), DynamicMwuCollectionBridgeError> {
    let edge_bits = valid_edge_bits(graph.edges.len())?;
    if branch.forest_mask & !branch.tree_mask != 0
        || branch.tree_mask | edge_bits != edge_bits
        || branch.tree_mask.count_ones() as usize != graph.node_count - 1
        || branch.reference_root >= graph.node_count
        || branch.root_seeds.is_empty()
        || branch.root_seeds.windows(2).any(|pair| pair[0] >= pair[1])
        || branch
            .root_seeds
            .iter()
            .any(|&root| root >= graph.node_count)
        || !branch_lsst_contract(graph, config, branch)
        || branch.roots.len() != graph.node_count
        || branch.stretch_overestimates.len() != graph.edges.len()
    {
        return Err(DynamicMwuCollectionBridgeError::TraceVerification);
    }
    Ok(())
}

fn branch_lsst_contract(
    graph: &ShiftedTreeChainGraph,
    config: DynamicMwuCollectionBridgeConfig,
    branch: &LowStretchForestMwuBranch,
) -> bool {
    let Ok(edge_count) = u64::try_from(graph.edges.len()) else {
        return false;
    };
    if branch.weight_copy_counts.len() != graph.edges.len()
        || branch.tree_stretches.len() != graph.edges.len()
        || branch.stretch_overestimates.len() != graph.edges.len()
        || branch.roots.len() != graph.node_count
        || branch
            .weight_copy_counts
            .iter()
            .any(|&copies| copies == 0 || copies > edge_count)
        || branch
            .tree_stretches
            .iter()
            .any(|stretch| stretch <= &BigRational::from_integer(BigInt::from(0_u8)))
    {
        return false;
    }
    let Some(total_copies) = branch
        .weight_copy_counts
        .iter()
        .try_fold(0_u64, |sum, &copies| sum.checked_add(copies))
    else {
        return false;
    };
    if total_copies == 0 || total_copies > edge_count.saturating_mul(2) {
        return false;
    }
    let score = branch
        .weight_copy_counts
        .iter()
        .zip(&branch.tree_stretches)
        .fold(
            BigRational::from_integer(BigInt::from(0_u8)),
            |sum, (&copies, stretch)| sum + stretch * BigInt::from(copies),
        );
    score == branch.weighted_tree_stretch
        && branch.measured_lsst_gamma == score / BigInt::from(total_copies)
        && branch_root_refinement_contract(graph, config, branch)
        && branch_partition_contract(graph, branch)
}

fn branch_root_refinement_contract(
    graph: &ShiftedTreeChainGraph,
    config: DynamicMwuCollectionBridgeConfig,
    branch: &LowStretchForestMwuBranch,
) -> bool {
    if branch.decomposition_volume_limit != config.branches
        || branch
            .large_stretch_edges
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || branch
            .large_stretch_edges
            .iter()
            .any(|&edge| edge >= graph.edges.len())
        || branch
            .decomposition_seeds
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || branch
            .decomposition_seeds
            .iter()
            .any(|&seed| seed >= graph.node_count)
    {
        return false;
    }
    let log = usize::BITS - graph.node_count.saturating_sub(1).leading_zeros();
    let Ok(log) = usize::try_from(log).map(|value| value.max(1)) else {
        return false;
    };
    let Some(scale) = log
        .checked_mul(log)
        .and_then(|value| value.checked_mul(log))
        .and_then(|value| value.checked_mul(log))
        .and_then(|value| value.checked_mul(config.branches))
    else {
        return false;
    };
    let Ok(scale) = u64::try_from(scale) else {
        return false;
    };
    if branch.large_stretch_threshold != &branch.measured_lsst_gamma * BigInt::from(scale) {
        return false;
    }
    let expected_large_edges = branch
        .stretch_overestimates
        .iter()
        .enumerate()
        .filter_map(|(edge, stretch)| (stretch >= &branch.large_stretch_threshold).then_some(edge))
        .collect::<Vec<_>>();
    if branch.large_stretch_edges != expected_large_edges {
        return false;
    }
    let mut required_seeds = BTreeSet::from([0]);
    for &edge in &branch.large_stretch_edges {
        required_seeds.insert(graph.edges[edge].from);
        required_seeds.insert(graph.edges[edge].to);
    }
    if branch
        .decomposition_seeds
        .iter()
        .any(|seed| required_seeds.contains(seed))
    {
        return false;
    }
    required_seeds.extend(branch.decomposition_seeds.iter().copied());
    branch.root_seeds == required_seeds.into_iter().collect::<Vec<_>>()
}

fn branch_partition_contract(
    graph: &ShiftedTreeChainGraph,
    branch: &LowStretchForestMwuBranch,
) -> bool {
    if branch.roots.iter().any(|&root| root >= graph.node_count) {
        return false;
    }
    let mut roots = branch.roots.clone();
    roots.sort_unstable();
    roots.dedup();
    let mut expected = Vec::with_capacity(roots.len());
    for root in roots {
        let vertices = branch
            .roots
            .iter()
            .enumerate()
            .filter_map(|(vertex, &component_root)| (component_root == root).then_some(vertex))
            .collect::<Vec<_>>();
        if vertices.binary_search(&root).is_err() {
            return false;
        }
        let tree_edges = graph
            .edges
            .iter()
            .enumerate()
            .filter_map(|(edge, row)| {
                (branch.forest_mask & (1_u64 << edge) != 0
                    && branch.roots[row.from] == root
                    && branch.roots[row.to] == root)
                    .then_some(edge)
            })
            .collect::<Vec<_>>();
        let adjacent_non_root_edges = graph
            .edges
            .iter()
            .enumerate()
            .filter_map(|(edge, row)| {
                ((branch.roots[row.from] == root && row.from != root)
                    || (branch.roots[row.to] == root && row.to != root))
                    .then_some(edge)
            })
            .collect::<Vec<_>>();
        if adjacent_non_root_edges.len() > branch.decomposition_volume_limit {
            return false;
        }
        expected.push(LowStretchForestTreePiece {
            root,
            vertices,
            tree_edges,
            adjacent_non_root_edges,
        });
    }
    branch.tree_partition == expected
}

fn valid_edge_bits(edge_count: usize) -> Result<u64, DynamicMwuCollectionBridgeError> {
    let shift =
        u32::try_from(edge_count).map_err(|_| DynamicMwuCollectionBridgeError::AdmissionLimit)?;
    1_u64
        .checked_shl(shift)
        .and_then(|value| value.checked_sub(1))
        .ok_or(DynamicMwuCollectionBridgeError::AdmissionLimit)
}

fn mask_source_edges(
    graph: &ShiftedTreeChainGraph,
    mask: u64,
) -> Result<Vec<usize>, DynamicMwuCollectionBridgeError> {
    if mask & !valid_edge_bits(graph.edges.len())? != 0 {
        return Err(DynamicMwuCollectionBridgeError::InvariantViolation);
    }
    Ok(graph
        .edges
        .iter()
        .enumerate()
        .filter_map(|(index, edge)| (mask & (1_u64 << index) != 0).then_some(edge.source_edge))
        .collect())
}

fn audit_cross_contract(
    graph: &ShiftedTreeChainGraph,
    branches: &[LowStretchForestMwuBranch],
    collection: &DynamicSparseCoreCollectionInput,
    trace: &DynamicSparseCoreCollectionStageTraceResult,
) -> Result<(), DynamicMwuCollectionBridgeError> {
    if branches.len() != collection.branches.len() || branches.len() != trace.branch_traces.len() {
        return Err(DynamicMwuCollectionBridgeError::TraceVerification);
    }
    for ((branch, input), branch_trace) in branches
        .iter()
        .zip(&collection.branches)
        .zip(&trace.branch_traces)
    {
        let forest = &branch_trace.core_trace.forest_trace.base_snapshot;
        let expected_tree = graph
            .edges
            .iter()
            .enumerate()
            .filter_map(|(index, edge)| {
                (branch.tree_mask & (1_u64 << index) != 0).then_some(edge.source_edge)
            })
            .collect::<BTreeSet<_>>();
        let expected_roots = audit_hld_closure(
            &input.core.forest.initial_root_seeds,
            &forest.auxiliary_parent,
            input.core.forest.initial_node_count,
        )?;
        let mut expected_forest = graph
            .edges
            .iter()
            .enumerate()
            .filter_map(|(index, edge)| {
                (branch.forest_mask & (1_u64 << index) != 0).then_some(edge.source_edge)
            })
            .collect::<Vec<_>>();
        expected_forest.sort_unstable();
        if forest.roots != expected_roots
            || forest.component_roots != branch.roots
            || forest.forest_edges != expected_forest
            || forest
                .forest_edges
                .iter()
                .any(|edge| !expected_tree.contains(edge))
            || forest.edge_slots != input.core.forest.edge_slots
        {
            return Err(DynamicMwuCollectionBridgeError::TraceVerification);
        }
        for (index, edge) in graph.edges.iter().enumerate() {
            let expected = Some(branch.stretch_overestimates[index].clone());
            if forest.stretch_overestimates[edge.source_edge] != expected
                || forest.current_stretches[edge.source_edge]
                    .as_ref()
                    .is_none_or(|current| current > expected.as_ref().expect("present"))
            {
                return Err(DynamicMwuCollectionBridgeError::TraceVerification);
            }
        }
        for slot in 0..input.core.forest.edge_slots.len() {
            if input.core.forest.edge_slots[slot].is_none()
                && (forest.current_stretches[slot].is_some()
                    || forest.stretch_overestimates[slot].is_some())
            {
                return Err(DynamicMwuCollectionBridgeError::TraceVerification);
            }
        }
    }
    Ok(())
}

fn audit_hld_closure(
    seeds: &[usize],
    auxiliary_parent: &[Option<usize>],
    static_node_count: usize,
) -> Result<Vec<usize>, DynamicMwuCollectionBridgeError> {
    if auxiliary_parent.len() < static_node_count
        || seeds.iter().any(|&seed| seed >= static_node_count)
    {
        return Err(DynamicMwuCollectionBridgeError::TraceVerification);
    }
    let mut roots = BTreeSet::new();
    for &seed in seeds {
        let mut cursor = seed;
        loop {
            roots.insert(cursor);
            let Some(parent) = auxiliary_parent[cursor] else {
                break;
            };
            if parent >= static_node_count {
                return Err(DynamicMwuCollectionBridgeError::TraceVerification);
            }
            cursor = parent;
        }
    }
    Ok(roots.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use num_traits::One;

    use super::*;
    use crate::algorithms::ShiftedTreeChainEdge;

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn graph_with_holes() -> ShiftedTreeChainGraph {
        ShiftedTreeChainGraph {
            node_count: 4,
            edges: vec![
                ShiftedTreeChainEdge {
                    source_edge: 7,
                    from: 0,
                    to: 1,
                    length: rational(1),
                    gradient: rational(-2),
                },
                ShiftedTreeChainEdge {
                    source_edge: 1,
                    from: 1,
                    to: 3,
                    length: rational(1),
                    gradient: rational(3),
                },
                ShiftedTreeChainEdge {
                    source_edge: 9,
                    from: 0,
                    to: 2,
                    length: rational(2),
                    gradient: rational(5),
                },
                ShiftedTreeChainEdge {
                    source_edge: 3,
                    from: 2,
                    to: 3,
                    length: rational(3),
                    gradient: rational(-7),
                },
                ShiftedTreeChainEdge {
                    source_edge: 5,
                    from: 1,
                    to: 2,
                    length: rational(1),
                    gradient: rational(11),
                },
            ],
        }
    }

    fn config() -> DynamicMwuCollectionBridgeConfig {
        DynamicMwuCollectionBridgeConfig {
            branches: 4,
            maximum_node_count: 6,
            stable_edge_slots: 10,
        }
    }

    #[test]
    fn fast_trace_and_both_component_checkers_match_with_stable_holes() {
        let graph = graph_with_holes();
        let fast = build_dynamic_mwu_sparse_core_collection(&graph, config()).expect("fast");
        let trace = trace_dynamic_mwu_sparse_core_collection(&graph, config()).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(trace.result.collection.branches.len(), 4);
        assert_eq!(
            trace.result.collection.branches[0]
                .core
                .forest
                .edge_slots
                .len(),
            10
        );
        assert!(trace.result.collection.branches[0].core.forest.edge_slots[0].is_none());
        assert!(trace.result.collection.branches[0].core.forest.edge_slots[7].is_some());
        check_dynamic_mwu_sparse_core_collection_trace(&graph, config(), &trace).expect("check");
    }

    #[test]
    fn each_dynamic_base_exactly_matches_the_selected_hld_forest() {
        let graph = graph_with_holes();
        let trace = trace_dynamic_mwu_sparse_core_collection(&graph, config()).expect("trace");
        for ((selected, input), dynamic) in trace
            .mwu_trace
            .result
            .branches
            .iter()
            .zip(&trace.result.collection.branches)
            .zip(&trace.collection_trace.branch_traces)
        {
            let forest = &dynamic.core_trace.forest_trace.base_snapshot;
            let mut selected_edges = graph
                .edges
                .iter()
                .enumerate()
                .filter_map(|(index, edge)| {
                    (selected.forest_mask & (1_u64 << index) != 0).then_some(edge.source_edge)
                })
                .collect::<Vec<_>>();
            selected_edges.sort_unstable();
            assert_eq!(forest.forest_edges, selected_edges);
            assert_eq!(forest.component_roots, selected.roots);
            assert_eq!(
                forest.roots,
                audit_hld_closure(
                    &input.core.forest.initial_root_seeds,
                    &forest.auxiliary_parent,
                    graph.node_count,
                )
                .expect("closure")
            );
            for (index, edge) in graph.edges.iter().enumerate() {
                assert!(
                    forest.current_stretches[edge.source_edge]
                        .as_ref()
                        .expect("stretch")
                        <= &selected.stretch_overestimates[index]
                );
            }
        }
    }

    #[test]
    fn checker_rejects_collection_tree_root_seed_and_stretch_tampering() {
        let graph = graph_with_holes();
        let mut trace = trace_dynamic_mwu_sparse_core_collection(&graph, config()).expect("trace");
        trace.result.collection.branches[0]
            .core
            .forest
            .reference_tree_edges[0] = 0;
        assert_eq!(
            check_dynamic_mwu_sparse_core_collection_trace(&graph, config(), &trace),
            Err(DynamicMwuCollectionBridgeError::TraceVerification)
        );

        let mut trace = trace_dynamic_mwu_sparse_core_collection(&graph, config()).expect("trace");
        trace.result.collection.branches[0]
            .core
            .forest
            .reference_root = 1;
        assert_eq!(
            check_dynamic_mwu_sparse_core_collection_trace(&graph, config(), &trace),
            Err(DynamicMwuCollectionBridgeError::TraceVerification)
        );

        let mut trace = trace_dynamic_mwu_sparse_core_collection(&graph, config()).expect("trace");
        trace.result.collection.branches[0]
            .core
            .forest
            .initial_root_seeds = vec![0, 2];
        assert_eq!(
            check_dynamic_mwu_sparse_core_collection_trace(&graph, config(), &trace),
            Err(DynamicMwuCollectionBridgeError::TraceVerification)
        );

        let mut trace = trace_dynamic_mwu_sparse_core_collection(&graph, config()).expect("trace");
        trace.collection_trace.branch_traces[0]
            .core_trace
            .forest_trace
            .base_snapshot
            .stretch_overestimates[7] = Some(BigRational::one());
        assert!(check_dynamic_mwu_sparse_core_collection_trace(&graph, config(), &trace).is_err());
    }

    #[test]
    fn conversion_rejects_lsst_threshold_and_partition_tampering() {
        let graph = graph_with_holes();
        let result = build_dynamic_mwu_sparse_core_collection(&graph, config()).expect("bridge");

        let mut branches = result.mwu.branches.clone();
        branches[0].measured_lsst_gamma += BigRational::one();
        assert_eq!(
            convert_collection(&graph, config(), &branches),
            Err(DynamicMwuCollectionBridgeError::InvariantViolation)
        );

        let mut branches = result.mwu.branches.clone();
        branches[0].decomposition_volume_limit += 1;
        assert_eq!(
            convert_collection(&graph, config(), &branches),
            Err(DynamicMwuCollectionBridgeError::InvariantViolation)
        );

        let mut branches = result.mwu.branches.clone();
        branches[0].tree_partition[0]
            .adjacent_non_root_edges
            .push(0);
        assert_eq!(
            convert_collection(&graph, config(), &branches),
            Err(DynamicMwuCollectionBridgeError::InvariantViolation)
        );
    }

    #[test]
    fn rejects_duplicate_out_of_slot_and_small_vertex_universes() {
        let mut graph = graph_with_holes();
        graph.edges[1].source_edge = 7;
        assert_eq!(
            build_dynamic_mwu_sparse_core_collection(&graph, config()),
            Err(DynamicMwuCollectionBridgeError::InvalidInput)
        );
        let mut graph = graph_with_holes();
        graph.edges[0].source_edge = 10;
        assert_eq!(
            build_dynamic_mwu_sparse_core_collection(&graph, config()),
            Err(DynamicMwuCollectionBridgeError::InvalidInput)
        );
        let mut invalid = config();
        invalid.maximum_node_count = 3;
        assert_eq!(
            build_dynamic_mwu_sparse_core_collection(&graph_with_holes(), invalid),
            Err(DynamicMwuCollectionBridgeError::InvalidInput)
        );
    }

    #[test]
    fn propagates_disconnection_and_admission_failures() {
        let mut disconnected = graph_with_holes();
        disconnected.edges.truncate(1);
        assert_eq!(
            build_dynamic_mwu_sparse_core_collection(&disconnected, config()),
            Err(DynamicMwuCollectionBridgeError::Mwu(
                LowStretchForestMwuError::Disconnected
            ))
        );
        let mut excessive = config();
        excessive.stable_edge_slots = DYNAMIC_SPARSE_CORE_MAX_EDGES + 1;
        assert_eq!(
            build_dynamic_mwu_sparse_core_collection(&graph_with_holes(), excessive),
            Err(DynamicMwuCollectionBridgeError::AdmissionLimit)
        );
    }
}
