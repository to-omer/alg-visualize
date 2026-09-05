//! Deterministic Heavy-Light auxiliary tree from CKLPPS Lemma B.9.
//!
//! The construction keeps the vertex set and root of a checked reference tree,
//! partitions it into deterministic heavy chains, replaces each chain by a
//! balanced binary-search tree ordered by reference depth, and keeps the
//! minimum-depth chain vertex as that BST's root. Light edges connect the chain
//! BSTs. Ancestor closure in the resulting tree is branch-free in the original
//! reference tree.

use thiserror::Error;

/// Exact, immutable Heavy-Light auxiliary tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HldBranchFreeTree {
    /// Shared root of the reference and auxiliary trees.
    pub root: usize,
    /// Reference-tree subtree size used by the deterministic heavy choice.
    pub subtree_size: Vec<usize>,
    /// Heavy child, with equal-size ties broken by stable vertex ID.
    pub heavy_child: Vec<Option<usize>>,
    /// Minimum-reference-depth vertex of each node's heavy chain.
    pub chain_head: Vec<usize>,
    /// Zero-based position on the heavy chain, ordered by reference depth.
    pub chain_index: Vec<usize>,
    /// Parent in the source auxiliary tree.
    pub auxiliary_parent: Vec<Option<usize>>,
    /// Depth in the source auxiliary tree.
    pub auxiliary_depth: Vec<usize>,
}

/// Malformed reference input or a failed source construction invariant.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum HldBranchFreeTreeError {
    /// The supplied parent relation is not one rooted tree.
    #[error("HLD reference parent relation is invalid")]
    InvalidReferenceTree,
    /// The deterministic auxiliary-tree construction failed an invariant.
    #[error("HLD auxiliary tree invariant failed")]
    InvariantViolation,
}

/// Builds the deterministic auxiliary tree used by the Dynamic LSF proof.
///
/// # Errors
///
/// Rejects an empty, cyclic, disconnected, or incorrectly rooted parent
/// relation, as well as any failed post-construction invariant.
pub fn build_hld_branch_free_tree(
    reference_parent: &[Option<usize>],
    root: usize,
) -> Result<HldBranchFreeTree, HldBranchFreeTreeError> {
    let children = checked_children(reference_parent, root)?;
    let reference_depth = checked_depths(&children, root)?;
    let mut postorder = (0..reference_parent.len()).collect::<Vec<_>>();
    postorder.sort_unstable_by_key(|&node| (usize::MAX - reference_depth[node], node));

    let mut subtree_size = vec![1_usize; reference_parent.len()];
    let mut heavy_child = vec![None; reference_parent.len()];
    for node in postorder {
        let mut selected = None;
        for &child in &children[node] {
            subtree_size[node] = subtree_size[node]
                .checked_add(subtree_size[child])
                .ok_or(HldBranchFreeTreeError::InvariantViolation)?;
            if selected.is_none_or(|current| {
                subtree_size[child] > subtree_size[current]
                    || (subtree_size[child] == subtree_size[current] && child < current)
            }) {
                selected = Some(child);
            }
        }
        heavy_child[node] = selected;
    }

    let mut heads = (0..reference_parent.len())
        .filter(|&node| {
            reference_parent[node].is_none_or(|parent| heavy_child[parent] != Some(node))
        })
        .collect::<Vec<_>>();
    heads.sort_unstable_by_key(|&node| (reference_depth[node], node));

    let mut chain_head = vec![usize::MAX; reference_parent.len()];
    let mut chain_index = vec![usize::MAX; reference_parent.len()];
    let mut auxiliary_parent = vec![None; reference_parent.len()];
    for head in heads {
        let mut chain = Vec::new();
        let mut cursor = Some(head);
        while let Some(node) = cursor {
            chain_index[node] = chain.len();
            chain_head[node] = head;
            chain.push(node);
            cursor = heavy_child[node];
        }
        auxiliary_parent[head] = reference_parent[head];
        if chain.len() > 1 {
            build_balanced_suffix(&chain[1..], head, &mut auxiliary_parent);
        }
    }

    let auxiliary_children = checked_children(&auxiliary_parent, root)
        .map_err(|_| HldBranchFreeTreeError::InvariantViolation)?;
    let auxiliary_depth = checked_depths(&auxiliary_children, root)
        .map_err(|_| HldBranchFreeTreeError::InvariantViolation)?;
    let tree = HldBranchFreeTree {
        root,
        subtree_size,
        heavy_child,
        chain_head,
        chain_index,
        auxiliary_parent,
        auxiliary_depth,
    };
    check_hld_branch_free_tree(reference_parent, &tree)?;
    Ok(tree)
}

/// Independently checks the deterministic construction and concrete bounds.
///
/// # Errors
///
/// Returns an invariant error if any published structural property represented
/// by this bounded construction is violated.
pub fn check_hld_branch_free_tree(
    reference_parent: &[Option<usize>],
    tree: &HldBranchFreeTree,
) -> Result<(), HldBranchFreeTreeError> {
    let node_count = reference_parent.len();
    let children = checked_children(reference_parent, tree.root)?;
    let reference_depth = checked_depths(&children, tree.root)?;
    if tree.subtree_size.len() != node_count
        || tree.heavy_child.len() != node_count
        || tree.chain_head.len() != node_count
        || tree.chain_index.len() != node_count
        || tree.auxiliary_parent.len() != node_count
        || tree.auxiliary_depth.len() != node_count
    {
        return Err(HldBranchFreeTreeError::InvariantViolation);
    }
    let auxiliary_children = checked_children(&tree.auxiliary_parent, tree.root)
        .map_err(|_| HldBranchFreeTreeError::InvariantViolation)?;
    let rebuilt_auxiliary_depth = checked_depths(&auxiliary_children, tree.root)
        .map_err(|_| HldBranchFreeTreeError::InvariantViolation)?;
    if rebuilt_auxiliary_depth != tree.auxiliary_depth {
        return Err(HldBranchFreeTreeError::InvariantViolation);
    }

    let expected_auxiliary_parent =
        rebuild_auxiliary_parent(reference_parent, &reference_depth, &tree.heavy_child);
    if expected_auxiliary_parent != tree.auxiliary_parent {
        return Err(HldBranchFreeTreeError::InvariantViolation);
    }

    for node in 0..node_count {
        let expected_size = 1 + children[node]
            .iter()
            .map(|&child| tree.subtree_size[child])
            .sum::<usize>();
        if tree.subtree_size[node] != expected_size {
            return Err(HldBranchFreeTreeError::InvariantViolation);
        }
        let expected_heavy = children[node].iter().copied().min_by(|&left, &right| {
            tree.subtree_size[right]
                .cmp(&tree.subtree_size[left])
                .then(left.cmp(&right))
        });
        if tree.heavy_child[node] != expected_heavy {
            return Err(HldBranchFreeTreeError::InvariantViolation);
        }
        let expected_head = reference_parent[node]
            .filter(|&parent| tree.heavy_child[parent] == Some(node))
            .map_or(node, |parent| tree.chain_head[parent]);
        let expected_index = reference_parent[node]
            .filter(|&parent| tree.heavy_child[parent] == Some(node))
            .map_or(0, |parent| tree.chain_index[parent] + 1);
        if tree.chain_head[node] != expected_head || tree.chain_index[node] != expected_index {
            return Err(HldBranchFreeTreeError::InvariantViolation);
        }
    }

    let chain_bound = ceil_log2(node_count).saturating_add(1);
    for node in 0..node_count {
        let mut chain_count = 0_usize;
        let mut cursor = Some(node);
        let mut previous_head = None;
        while let Some(current) = cursor {
            if previous_head != Some(tree.chain_head[current]) {
                chain_count += 1;
                previous_head = Some(tree.chain_head[current]);
            }
            cursor = reference_parent[current];
        }
        if chain_count > chain_bound {
            return Err(HldBranchFreeTreeError::InvariantViolation);
        }
    }
    let height_bound = chain_bound.saturating_mul(chain_bound);
    if tree.auxiliary_depth.iter().copied().max().unwrap_or(0) > height_bound {
        return Err(HldBranchFreeTreeError::InvariantViolation);
    }
    for node in 0..node_count {
        if tree.chain_head[node] == node {
            if tree.auxiliary_parent[node] != reference_parent[node] {
                return Err(HldBranchFreeTreeError::InvariantViolation);
            }
        } else {
            let parent =
                tree.auxiliary_parent[node].ok_or(HldBranchFreeTreeError::InvariantViolation)?;
            if tree.chain_head[parent] != tree.chain_head[node]
                || reference_depth[parent] == reference_depth[node]
            {
                return Err(HldBranchFreeTreeError::InvariantViolation);
            }
        }
    }
    Ok(())
}

/// Returns the auxiliary ancestor closure, sorted by stable vertex ID.
#[must_use]
pub fn hld_ancestor_closure(tree: &HldBranchFreeTree, seeds: &[usize]) -> Vec<usize> {
    let mut included = vec![false; tree.auxiliary_parent.len()];
    for &seed in seeds {
        let mut cursor = Some(seed);
        while let Some(node) = cursor {
            included[node] = true;
            cursor = tree.auxiliary_parent[node];
        }
    }
    included
        .into_iter()
        .enumerate()
        .filter_map(|(node, include)| include.then_some(node))
        .collect()
}

/// Checks the CKLPPS branch-free definition in the reference tree.
#[must_use]
pub fn is_branch_free(reference_parent: &[Option<usize>], roots: &[usize]) -> bool {
    let Ok(children) = checked_children(
        reference_parent,
        reference_parent
            .iter()
            .position(Option::is_none)
            .unwrap_or(reference_parent.len()),
    ) else {
        return false;
    };
    let Ok(depth) = checked_depths(
        &children,
        reference_parent
            .iter()
            .position(Option::is_none)
            .unwrap_or(reference_parent.len()),
    ) else {
        return false;
    };
    if roots.iter().any(|&root| root >= reference_parent.len()) {
        return false;
    }
    for (index, &left) in roots.iter().enumerate() {
        for &right in &roots[index..] {
            let Some(lca) = lowest_common_ancestor(reference_parent, &depth, left, right) else {
                return false;
            };
            if roots.binary_search(&lca).is_err() {
                return false;
            }
        }
    }
    true
}

fn checked_children(
    parent: &[Option<usize>],
    root: usize,
) -> Result<Vec<Vec<usize>>, HldBranchFreeTreeError> {
    if parent.is_empty() || root >= parent.len() || parent[root].is_some() {
        return Err(HldBranchFreeTreeError::InvalidReferenceTree);
    }
    let mut children = vec![Vec::new(); parent.len()];
    for (node, &parent_node) in parent.iter().enumerate() {
        if node == root {
            continue;
        }
        let parent_node = parent_node.ok_or(HldBranchFreeTreeError::InvalidReferenceTree)?;
        if parent_node >= parent.len() || parent_node == node {
            return Err(HldBranchFreeTreeError::InvalidReferenceTree);
        }
        children[parent_node].push(node);
    }
    for row in &mut children {
        row.sort_unstable();
    }
    Ok(children)
}

fn checked_depths(
    children: &[Vec<usize>],
    root: usize,
) -> Result<Vec<usize>, HldBranchFreeTreeError> {
    if root >= children.len() {
        return Err(HldBranchFreeTreeError::InvalidReferenceTree);
    }
    let mut depth = vec![usize::MAX; children.len()];
    depth[root] = 0;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        for &child in children[node].iter().rev() {
            if depth[child] != usize::MAX {
                return Err(HldBranchFreeTreeError::InvalidReferenceTree);
            }
            depth[child] = depth[node]
                .checked_add(1)
                .ok_or(HldBranchFreeTreeError::InvariantViolation)?;
            stack.push(child);
        }
    }
    if depth.contains(&usize::MAX) {
        return Err(HldBranchFreeTreeError::InvalidReferenceTree);
    }
    Ok(depth)
}

fn build_balanced_suffix(nodes: &[usize], parent: usize, auxiliary_parent: &mut [Option<usize>]) {
    if nodes.is_empty() {
        return;
    }
    let middle = nodes.len() / 2;
    let root = nodes[middle];
    auxiliary_parent[root] = Some(parent);
    build_balanced_suffix(&nodes[..middle], root, auxiliary_parent);
    build_balanced_suffix(&nodes[middle + 1..], root, auxiliary_parent);
}

fn rebuild_auxiliary_parent(
    reference_parent: &[Option<usize>],
    reference_depth: &[usize],
    heavy_child: &[Option<usize>],
) -> Vec<Option<usize>> {
    let mut auxiliary_parent = vec![None; reference_parent.len()];
    let mut heads = (0..reference_parent.len())
        .filter(|&node| {
            reference_parent[node].is_none_or(|parent| heavy_child[parent] != Some(node))
        })
        .collect::<Vec<_>>();
    heads.sort_unstable_by_key(|&node| (reference_depth[node], node));
    for head in heads {
        let mut chain = Vec::new();
        let mut cursor = Some(head);
        while let Some(node) = cursor {
            chain.push(node);
            cursor = heavy_child[node];
        }
        auxiliary_parent[head] = reference_parent[head];
        if chain.len() > 1 {
            build_balanced_suffix(&chain[1..], head, &mut auxiliary_parent);
        }
    }
    auxiliary_parent
}

fn lowest_common_ancestor(
    parent: &[Option<usize>],
    depth: &[usize],
    mut left: usize,
    mut right: usize,
) -> Option<usize> {
    while depth[left] > depth[right] {
        left = parent[left]?;
    }
    while depth[right] > depth[left] {
        right = parent[right]?;
    }
    while left != right {
        left = parent[left]?;
        right = parent[right]?;
    }
    Some(left)
}

fn ceil_log2(value: usize) -> usize {
    if value <= 1 {
        0
    } else {
        usize::BITS as usize - (value - 1).leading_zeros() as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_parent() -> Vec<Option<usize>> {
        // 0 - 1 - 2 - 4 - 6
        //     |   |   `- 7
        //     3   5
        vec![
            None,
            Some(0),
            Some(1),
            Some(1),
            Some(2),
            Some(2),
            Some(4),
            Some(4),
        ]
    }

    #[test]
    fn heavy_chains_are_deterministic_and_auxiliary_tree_differs_from_reference() {
        let tree = build_hld_branch_free_tree(&sample_parent(), 0).expect("tree");
        assert_eq!(tree.heavy_child[0], Some(1));
        assert_eq!(tree.heavy_child[1], Some(2));
        assert_eq!(tree.heavy_child[2], Some(4));
        assert_eq!(tree.heavy_child[4], Some(6));
        assert_ne!(tree.auxiliary_parent, sample_parent());
        check_hld_branch_free_tree(&sample_parent(), &tree).expect("check");
    }

    #[test]
    fn every_seed_subset_has_branch_free_auxiliary_ancestor_closure() {
        let parent = sample_parent();
        let tree = build_hld_branch_free_tree(&parent, 0).expect("tree");
        for mask in 0_usize..(1_usize << parent.len()) {
            let seeds = (0..parent.len())
                .filter(|&node| mask & (1_usize << node) != 0)
                .collect::<Vec<_>>();
            let closure = hld_ancestor_closure(&tree, &seeds);
            assert!(is_branch_free(&parent, &closure), "mask={mask:#x}");
        }
    }

    #[test]
    fn rejects_cycle_and_wrong_root() {
        assert_eq!(
            build_hld_branch_free_tree(&[Some(1), Some(0)], 0),
            Err(HldBranchFreeTreeError::InvalidReferenceTree)
        );
        assert_eq!(
            build_hld_branch_free_tree(&[None, Some(0)], 1),
            Err(HldBranchFreeTreeError::InvalidReferenceTree)
        );
    }
}
