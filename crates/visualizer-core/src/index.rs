//! Rank/select indexes for present and absent generator keys.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use thiserror::Error;

const PRESENT_PRIORITY_DOMAIN: u64 = 0x5052_4553_454e_5401;
const GAP_PRIORITY_DOMAIN: u64 = 0x4741_505f_494e_4458;

/// Failure to mutate or query a bounded key universe.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum IndexError {
    /// The inclusive universe bounds are reversed.
    #[error("key universe minimum exceeds maximum")]
    ReversedUniverse,
    /// A key is outside the inclusive universe.
    #[error("key is outside the generator universe")]
    OutOfUniverse,
    /// A key is already present.
    #[error("key is already present")]
    AlreadyPresent,
    /// A key is absent.
    #[error("key is absent")]
    Absent,
}

/// Deterministic present-key and absent-gap rank/select index.
///
/// Both trees are treaps with priorities derived from the key rather than from
/// process allocation order. Subtree counts/cardinalities make rank selection
/// logarithmic in the expected tree height without materializing the universe.
#[derive(Debug)]
pub struct UniverseIndex {
    minimum: u64,
    maximum: u64,
    present: Option<Box<PresentNode>>,
    gaps: BTreeMap<u64, u64>,
    gap_tree: Option<Box<GapNode>>,
}

impl UniverseIndex {
    /// Creates an empty index over an inclusive key universe.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::ReversedUniverse`] for reversed bounds.
    pub fn new(minimum: u64, maximum: u64) -> Result<Self, IndexError> {
        if minimum > maximum {
            return Err(IndexError::ReversedUniverse);
        }
        let mut gaps = BTreeMap::new();
        gaps.insert(minimum, maximum);
        Ok(Self {
            minimum,
            maximum,
            present: None,
            gaps,
            gap_tree: Some(Box::new(GapNode::new(minimum, maximum))),
        })
    }

    /// Returns the number of present keys.
    pub fn present_len(&self) -> u64 {
        PresentNode::size(self.present.as_deref())
    }

    /// Returns the number of absent keys using `u128` for the full `u64` range.
    pub fn absent_len(&self) -> u128 {
        GapNode::cardinality(self.gap_tree.as_deref())
    }

    /// Inserts an absent key.
    ///
    /// # Errors
    ///
    /// Rejects keys outside the universe and duplicate inserts.
    pub fn insert(&mut self, key: u64) -> Result<(), IndexError> {
        self.require_in_universe(key)?;
        let (&start, &end) = self
            .gaps
            .range(..=key)
            .next_back()
            .filter(|(_, end)| **end >= key)
            .ok_or(IndexError::AlreadyPresent)?;

        self.remove_gap(start);
        if start < key {
            self.insert_gap(start, key - 1);
        }
        if key < end {
            self.insert_gap(key + 1, end);
        }
        self.present = PresentNode::insert(self.present.take(), key);
        Ok(())
    }

    /// Removes a present key.
    ///
    /// # Errors
    ///
    /// Rejects keys outside the universe and absent removes.
    pub fn remove(&mut self, key: u64) -> Result<(), IndexError> {
        self.require_in_universe(key)?;
        let (next_present, removed) = PresentNode::remove(self.present.take(), key);
        self.present = next_present;
        if !removed {
            return Err(IndexError::Absent);
        }

        let predecessor = self
            .gaps
            .range(..key)
            .next_back()
            .and_then(|(&start, &end)| (end.checked_add(1) == Some(key)).then_some((start, end)));
        let successor = key
            .checked_add(1)
            .and_then(|start| self.gaps.get_key_value(&start).map(|(&s, &end)| (s, end)));

        let start = predecessor.map_or(key, |(start, _)| start);
        let end = successor.map_or(key, |(_, end)| end);
        if let Some((start, _)) = predecessor {
            self.remove_gap(start);
        }
        if let Some((start, _)) = successor {
            self.remove_gap(start);
        }
        self.insert_gap(start, end);
        Ok(())
    }

    /// Selects the zero-based present-key rank.
    pub fn select_present(&self, rank: u64) -> Option<u64> {
        PresentNode::select(self.present.as_deref(), rank)
    }

    /// Selects the zero-based absent-key rank.
    pub fn select_absent(&self, rank: u128) -> Option<u64> {
        GapNode::select(self.gap_tree.as_deref(), rank)
    }

    /// Counts present keys strictly smaller than `key`.
    pub fn rank_present(&self, key: u64) -> u64 {
        PresentNode::rank(self.present.as_deref(), key)
    }

    /// Counts absent keys strictly smaller than `key` within the universe.
    pub fn rank_absent(&self, key: u64) -> u128 {
        let bounded = key.clamp(self.minimum, self.maximum);
        let universe_before = u128::from(bounded) - u128::from(self.minimum);
        universe_before - u128::from(self.rank_present(bounded))
    }

    /// Returns the inclusive universe bounds.
    pub const fn bounds(&self) -> (u64, u64) {
        (self.minimum, self.maximum)
    }

    fn require_in_universe(&self, key: u64) -> Result<(), IndexError> {
        if (self.minimum..=self.maximum).contains(&key) {
            Ok(())
        } else {
            Err(IndexError::OutOfUniverse)
        }
    }

    fn remove_gap(&mut self, start: u64) {
        self.gaps.remove(&start);
        self.gap_tree = GapNode::remove(self.gap_tree.take(), start).0;
    }

    fn insert_gap(&mut self, start: u64, end: u64) {
        let previous = self.gaps.insert(start, end);
        debug_assert!(previous.is_none());
        self.gap_tree = GapNode::insert(self.gap_tree.take(), Box::new(GapNode::new(start, end)));
    }
}

#[derive(Debug)]
struct PresentNode {
    key: u64,
    priority: u64,
    size: u64,
    left: Option<Box<Self>>,
    right: Option<Box<Self>>,
}

impl PresentNode {
    fn new(key: u64) -> Self {
        Self {
            key,
            priority: priority(key, PRESENT_PRIORITY_DOMAIN),
            size: 1,
            left: None,
            right: None,
        }
    }

    fn size(node: Option<&Self>) -> u64 {
        node.map_or(0, |node| node.size)
    }

    fn update(&mut self) {
        self.size = 1 + Self::size(self.left.as_deref()) + Self::size(self.right.as_deref());
    }

    fn comes_before(left: &Self, right: &Self) -> bool {
        (left.priority, left.key) < (right.priority, right.key)
    }

    fn merge(left: Option<Box<Self>>, right: Option<Box<Self>>) -> Option<Box<Self>> {
        match (left, right) {
            (None, node) | (node, None) => node,
            (Some(mut left), Some(mut right)) => {
                if Self::comes_before(&left, &right) {
                    left.right = Self::merge(left.right.take(), Some(right));
                    left.update();
                    Some(left)
                } else {
                    right.left = Self::merge(Some(left), right.left.take());
                    right.update();
                    Some(right)
                }
            }
        }
    }

    fn split(root: Option<Box<Self>>, key: u64) -> (Option<Box<Self>>, Option<Box<Self>>) {
        let Some(mut root) = root else {
            return (None, None);
        };
        if root.key < key {
            let (left, right) = Self::split(root.right.take(), key);
            root.right = left;
            root.update();
            (Some(root), right)
        } else {
            let (left, right) = Self::split(root.left.take(), key);
            root.left = right;
            root.update();
            (left, Some(root))
        }
    }

    fn insert(root: Option<Box<Self>>, key: u64) -> Option<Box<Self>> {
        let node = Box::new(Self::new(key));
        let (left, right) = Self::split(root, key);
        Self::merge(Self::merge(left, Some(node)), right)
    }

    fn remove(root: Option<Box<Self>>, key: u64) -> (Option<Box<Self>>, bool) {
        let Some(mut root) = root else {
            return (None, false);
        };
        match key.cmp(&root.key) {
            Ordering::Less => {
                let (left, removed) = Self::remove(root.left.take(), key);
                root.left = left;
                root.update();
                (Some(root), removed)
            }
            Ordering::Greater => {
                let (right, removed) = Self::remove(root.right.take(), key);
                root.right = right;
                root.update();
                (Some(root), removed)
            }
            Ordering::Equal => (Self::merge(root.left.take(), root.right.take()), true),
        }
    }

    fn select(root: Option<&Self>, rank: u64) -> Option<u64> {
        let node = root?;
        let left_size = Self::size(node.left.as_deref());
        match rank.cmp(&left_size) {
            Ordering::Less => Self::select(node.left.as_deref(), rank),
            Ordering::Equal => Some(node.key),
            Ordering::Greater => Self::select(node.right.as_deref(), rank - left_size - 1),
        }
    }

    fn rank(root: Option<&Self>, key: u64) -> u64 {
        let Some(node) = root else {
            return 0;
        };
        if key <= node.key {
            Self::rank(node.left.as_deref(), key)
        } else {
            Self::size(node.left.as_deref()) + 1 + Self::rank(node.right.as_deref(), key)
        }
    }
}

#[derive(Debug)]
struct GapNode {
    start: u64,
    end: u64,
    priority: u64,
    subtree_cardinality: u128,
    left: Option<Box<Self>>,
    right: Option<Box<Self>>,
}

impl GapNode {
    fn new(start: u64, end: u64) -> Self {
        debug_assert!(start <= end);
        Self {
            start,
            end,
            priority: priority(start, GAP_PRIORITY_DOMAIN),
            subtree_cardinality: interval_len(start, end),
            left: None,
            right: None,
        }
    }

    fn cardinality(node: Option<&Self>) -> u128 {
        node.map_or(0, |node| node.subtree_cardinality)
    }

    fn update(&mut self) {
        self.subtree_cardinality = interval_len(self.start, self.end)
            + Self::cardinality(self.left.as_deref())
            + Self::cardinality(self.right.as_deref());
    }

    fn comes_before(left: &Self, right: &Self) -> bool {
        (left.priority, left.start) < (right.priority, right.start)
    }

    fn merge(left: Option<Box<Self>>, right: Option<Box<Self>>) -> Option<Box<Self>> {
        match (left, right) {
            (None, node) | (node, None) => node,
            (Some(mut left), Some(mut right)) => {
                if Self::comes_before(&left, &right) {
                    left.right = Self::merge(left.right.take(), Some(right));
                    left.update();
                    Some(left)
                } else {
                    right.left = Self::merge(Some(left), right.left.take());
                    right.update();
                    Some(right)
                }
            }
        }
    }

    fn split(root: Option<Box<Self>>, start: u64) -> (Option<Box<Self>>, Option<Box<Self>>) {
        let Some(mut root) = root else {
            return (None, None);
        };
        if root.start < start {
            let (left, right) = Self::split(root.right.take(), start);
            root.right = left;
            root.update();
            (Some(root), right)
        } else {
            let (left, right) = Self::split(root.left.take(), start);
            root.left = right;
            root.update();
            (left, Some(root))
        }
    }

    fn insert(root: Option<Box<Self>>, node: Box<Self>) -> Option<Box<Self>> {
        let (left, right) = Self::split(root, node.start);
        Self::merge(Self::merge(left, Some(node)), right)
    }

    fn remove(root: Option<Box<Self>>, start: u64) -> (Option<Box<Self>>, bool) {
        let Some(mut root) = root else {
            return (None, false);
        };
        match start.cmp(&root.start) {
            Ordering::Less => {
                let (left, removed) = Self::remove(root.left.take(), start);
                root.left = left;
                root.update();
                (Some(root), removed)
            }
            Ordering::Greater => {
                let (right, removed) = Self::remove(root.right.take(), start);
                root.right = right;
                root.update();
                (Some(root), removed)
            }
            Ordering::Equal => (Self::merge(root.left.take(), root.right.take()), true),
        }
    }

    fn select(root: Option<&Self>, rank: u128) -> Option<u64> {
        let node = root?;
        let left = Self::cardinality(node.left.as_deref());
        let own = interval_len(node.start, node.end);
        if rank < left {
            Self::select(node.left.as_deref(), rank)
        } else if rank < left + own {
            let offset = u64::try_from(rank - left).ok()?;
            node.start.checked_add(offset)
        } else {
            Self::select(node.right.as_deref(), rank - left - own)
        }
    }
}

fn interval_len(start: u64, end: u64) -> u128 {
    u128::from(end) - u128::from(start) + 1
}

const fn priority(key: u64, domain: u64) -> u64 {
    let mut value = key ^ domain;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use proptest::prelude::*;

    use super::*;

    #[test]
    fn full_u64_universe_uses_u128_cardinality() {
        let index = UniverseIndex::new(0, u64::MAX).expect("valid bounds");

        assert_eq!(index.absent_len(), u128::from(u64::MAX) + 1);
        assert_eq!(index.select_absent(0), Some(0));
        assert_eq!(index.select_absent(u128::from(u64::MAX)), Some(u64::MAX));
    }

    proptest! {
        #[test]
        fn rank_select_matches_naive_bitset(operations in prop::collection::vec((any::<bool>(), 0_u8..16), 0..128)) {
            let mut index = UniverseIndex::new(0, 15).expect("valid bounds");
            let mut model = BTreeSet::new();

            for (insert, key) in operations {
                if insert {
                    let expected = model.insert(u64::from(key));
                    prop_assert_eq!(index.insert(u64::from(key)).is_ok(), expected);
                } else {
                    let expected = model.remove(&u64::from(key));
                    prop_assert_eq!(index.remove(u64::from(key)).is_ok(), expected);
                }

                let present: Vec<_> = model.iter().copied().collect();
                let absent: Vec<_> = (0..=15).filter(|key| !model.contains(key)).collect();
                prop_assert_eq!(index.present_len(), present.len() as u64);
                prop_assert_eq!(index.absent_len(), absent.len() as u128);
                for (rank, key) in present.iter().enumerate() {
                    prop_assert_eq!(index.select_present(rank as u64), Some(*key));
                    prop_assert_eq!(index.rank_present(*key), rank as u64);
                }
                for (rank, key) in absent.iter().enumerate() {
                    prop_assert_eq!(index.select_absent(rank as u128), Some(*key));
                }
                prop_assert_eq!(index.select_present(present.len() as u64), None);
                prop_assert_eq!(index.select_absent(absent.len() as u128), None);
            }
        }
    }
}
