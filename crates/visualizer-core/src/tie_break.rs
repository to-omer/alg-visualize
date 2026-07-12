//! Shared, integer-only tie-break oracles from the algorithm definition ledger.

/// Whether the left candidate is above the right candidate in Treap, Zip, and
/// Y-fast bucket heaps. Higher random attribute wins; equal attributes put the
/// smaller key above.
pub const fn heap_left_above(
    left_attribute: u64,
    left_key: u64,
    right_attribute: u64,
    right_key: u64,
) -> bool {
    left_attribute > right_attribute || (left_attribute == right_attribute && left_key < right_key)
}

/// WBT repair choice for one heavy-child near/far pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WbtRotation {
    /// Strict `near_weight < 2 * far_weight`.
    Single,
    /// Equality and the greater-than case.
    Double,
}

/// Chooses the normative WBT rotation using `u128` to avoid overflow.
pub fn wbt_rotation(near_subtree_size: u64, far_subtree_size: u64) -> WbtRotation {
    let near_weight = u128::from(near_subtree_size) + 1;
    let far_weight = u128::from(far_subtree_size) + 1;
    if near_weight < 2 * far_weight {
        WbtRotation::Single
    } else {
        WbtRotation::Double
    }
}

/// B-tree pre-descent repair action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BTreeRepair {
    /// Borrow from the left sibling when possible.
    BorrowLeft,
    /// Otherwise borrow from the right sibling when possible.
    BorrowRight,
    /// Otherwise merge with a sibling.
    Merge,
}

/// Implements the left-borrow, right-borrow, merge priority.
pub const fn btree_repair(left_can_lend: bool, right_can_lend: bool) -> BTreeRepair {
    if left_can_lend {
        BTreeRepair::BorrowLeft
    } else if right_can_lend {
        BTreeRepair::BorrowRight
    } else {
        BTreeRepair::Merge
    }
}

/// Two-child BST deletion moves the successor entry, never its value alone.
pub const fn successor_key(candidates: &[u64]) -> Option<u64> {
    let mut index = 0;
    let mut minimum = None;
    while index < candidates.len() {
        let key = candidates[index];
        minimum = match minimum {
            Some(current) if current < key => Some(current),
            Some(_) | None => Some(key),
        };
        index += 1;
    }
    minimum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn randomized_heap_ties_put_smaller_key_above() {
        assert!(heap_left_above(9, 4, 9, 5));
        assert!(!heap_left_above(9, 5, 9, 4));
        assert!(heap_left_above(10, 99, 9, 0));
    }

    #[test]
    fn wbt_equality_chooses_double_rotation() {
        assert_eq!(wbt_rotation(2, 1), WbtRotation::Single);
        assert_eq!(wbt_rotation(3, 1), WbtRotation::Double);
    }

    #[test]
    fn btree_repair_prefers_left_then_right_then_merge() {
        assert_eq!(btree_repair(true, true), BTreeRepair::BorrowLeft);
        assert_eq!(btree_repair(false, true), BTreeRepair::BorrowRight);
        assert_eq!(btree_repair(false, false), BTreeRepair::Merge);
    }

    #[test]
    fn successor_oracle_is_input_order_independent() {
        assert_eq!(successor_key(&[8, 3, 5]), Some(3));
        assert_eq!(successor_key(&[]), None);
    }
}
