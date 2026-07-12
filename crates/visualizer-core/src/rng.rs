//! Versioned integer-only random-number contract.

use serde::{Deserialize, Serialize};
use thiserror::Error;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const SPLITMIX_INCREMENT: u64 = 0x9e37_79b9_7f4a_7c15;

/// Maximum random words consumed by one Zip-tree insertion.
pub const MAX_RNG_DRAWS_PER_INSERT: u32 = 1_048_576;
/// Maximum raw words consumed by one exact bounded sample.
pub const MAX_BOUNDED_RNG_DRAWS: u32 = 1_024;

/// Every V1 RNG domain label. The append-only list guards against stream
/// aliasing between generator stages and algorithm-specific attributes.
pub const RNG_DOMAIN_LABELS: [&str; 11] = [
    "rng.generator.initial.descriptor-order",
    "rng.generator.initial.key-selection",
    "rng.generator.initial.value",
    "rng.generator.operations.descriptor-order",
    "rng.generator.operations.key-selection",
    "rng.generator.operations.value",
    "rng.algorithm.treap.priority",
    "rng.algorithm.zip.rank",
    "rng.algorithm.skip-list.height",
    "rng.algorithm.y-fast.representative",
    "rng.algorithm.y-fast.bucket-priority",
];

/// Hash KDF domains are deliberately disjoint from RNG domains.
pub const HASH_KDF_LABELS: [&str; 6] = [
    "hash.algorithm.veb.k0",
    "hash.algorithm.veb.k1",
    "hash.algorithm.x-fast.k0",
    "hash.algorithm.x-fast.k1",
    "hash.algorithm.y-fast.k0",
    "hash.algorithm.y-fast.k1",
];

/// Normative RNG effect of a successful ordered-map operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationDrawRule {
    /// No algorithm RNG call or raw word is consumed.
    None,
    /// Exactly one raw `next_u64` word is consumed.
    OneRawWord,
    /// Zip rank consumes one or more raw words up to the resource limit.
    ZipRank,
    /// One bounded call per promotion trial, with no call after reaching top.
    SkipHeight,
    /// One priority word then one exact bounded representative sample.
    YFastInsert,
}

/// Per-algorithm operation ledger row.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlgorithmDrawLedger {
    /// Stable algorithm ID.
    pub algorithm: &'static str,
    /// Draw rule for a successful new-key insert.
    pub new_insert: OperationDrawRule,
    /// Draw rule for insert overwrite.
    pub overwrite: OperationDrawRule,
    /// Draw rule for remove, get, and lower-bound, whether hit or miss.
    pub other_operations: OperationDrawRule,
    /// Whether the seed is also used by a separate hash KDF without consuming
    /// an RNG stream.
    pub uses_hash_kdf: bool,
}

/// Complete ordered-map V1 algorithm RNG ledger.
pub const ALGORITHM_DRAW_LEDGER: [AlgorithmDrawLedger; 13] = [
    deterministic_ledger("avl"),
    deterministic_ledger("wbt"),
    deterministic_ledger("aa"),
    deterministic_ledger("llrb"),
    AlgorithmDrawLedger {
        algorithm: "treap",
        new_insert: OperationDrawRule::OneRawWord,
        overwrite: OperationDrawRule::None,
        other_operations: OperationDrawRule::None,
        uses_hash_kdf: false,
    },
    AlgorithmDrawLedger {
        algorithm: "zip",
        new_insert: OperationDrawRule::ZipRank,
        overwrite: OperationDrawRule::None,
        other_operations: OperationDrawRule::None,
        uses_hash_kdf: false,
    },
    deterministic_ledger("splay"),
    deterministic_ledger("scapegoat"),
    AlgorithmDrawLedger {
        algorithm: "skip-list",
        new_insert: OperationDrawRule::SkipHeight,
        overwrite: OperationDrawRule::None,
        other_operations: OperationDrawRule::None,
        uses_hash_kdf: false,
    },
    deterministic_ledger("b-tree"),
    AlgorithmDrawLedger {
        algorithm: "veb",
        uses_hash_kdf: true,
        ..deterministic_ledger("veb")
    },
    AlgorithmDrawLedger {
        algorithm: "x-fast",
        uses_hash_kdf: true,
        ..deterministic_ledger("x-fast")
    },
    AlgorithmDrawLedger {
        algorithm: "y-fast",
        new_insert: OperationDrawRule::YFastInsert,
        overwrite: OperationDrawRule::None,
        other_operations: OperationDrawRule::None,
        uses_hash_kdf: true,
    },
];

const fn deterministic_ledger(algorithm: &'static str) -> AlgorithmDrawLedger {
    AlgorithmDrawLedger {
        algorithm,
        new_insert: OperationDrawRule::None,
        overwrite: OperationDrawRule::None,
        other_operations: OperationDrawRule::None,
        uses_hash_kdf: false,
    }
}

/// Failure to satisfy a bounded random contract.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum RngError {
    /// `bounded_u64` requires a positive exclusive bound.
    #[error("random bound must be positive")]
    ZeroBound,
    /// Bernoulli parameters must satisfy `numerator <= denominator`.
    #[error("invalid Bernoulli ratio")]
    InvalidBernoulliRatio,
    /// A Zip-tree rank consumed the per-insertion resource budget.
    #[error("random draw limit exceeded")]
    DrawLimitExceeded,
    /// Rank arithmetic exceeded the versioned representation.
    #[error("Zip-tree rank overflowed")]
    RankOverflow,
}

/// Checkpointable xoshiro256** state with domain-separated `SplitMix64` seeding.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RngV1 {
    state: [u64; 4],
    draws: u64,
}

impl RngV1 {
    /// Creates an RNG from a user seed and canonical UTF-8 domain label.
    pub fn from_seed(seed: u64, domain_label: &str) -> Self {
        let mut splitmix_state = seed ^ fnv1a64(domain_label.as_bytes());
        let mut state = [0; 4];
        for word in &mut state {
            *word = splitmix64_next(&mut splitmix_state);
        }
        Self { state, draws: 0 }
    }

    /// Returns the exact checkpoint state.
    pub const fn state(&self) -> [u64; 4] {
        self.state
    }

    /// Returns the number of xoshiro words consumed by this stream.
    pub const fn draws(&self) -> u64 {
        self.draws
    }

    /// Advances xoshiro256** once.
    pub fn next_u64(&mut self) -> u64 {
        let result = self.state[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let temporary = self.state[1] << 17;

        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];
        self.state[2] ^= temporary;
        self.state[3] = self.state[3].rotate_left(45);
        self.draws += 1;
        result
    }

    /// Samples `[0, bound)` without modulo bias.
    ///
    /// # Errors
    ///
    /// Returns [`RngError::ZeroBound`] when `bound` is zero.
    pub fn bounded_u64(&mut self, bound: u64) -> Result<u64, RngError> {
        bounded_u64_with(bound, MAX_BOUNDED_RNG_DRAWS, || self.next_u64())
    }

    /// Samples an exact rational Bernoulli distribution.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero denominator or a numerator greater than the
    /// denominator.
    pub fn bernoulli(&mut self, numerator: u64, denominator: u64) -> Result<bool, RngError> {
        if denominator == 0 {
            return Err(RngError::ZeroBound);
        }
        if numerator > denominator {
            return Err(RngError::InvalidBernoulliRatio);
        }
        Ok(self.bounded_u64(denominator)? < numerator)
    }

    /// Samples a Skip-list tower height in `1..=max_level`.
    ///
    /// A promotion trial consumes exactly one bounded draw. Reaching the top
    /// performs no further draw.
    ///
    /// # Errors
    ///
    /// Returns an error if `max_level` or `promotion_denominator` is zero.
    pub fn skip_list_height(
        &mut self,
        max_level: u8,
        promotion_denominator: u64,
    ) -> Result<u8, RngError> {
        if max_level == 0 {
            return Err(RngError::ZeroBound);
        }
        if promotion_denominator == 0 {
            return Err(RngError::ZeroBound);
        }

        let mut height = 1;
        while height < max_level {
            if self.bounded_u64(promotion_denominator)? != 0 {
                break;
            }
            height += 1;
        }
        Ok(height)
    }

    /// Samples the geometric Zip-tree rank from least-significant zero bits.
    ///
    /// # Errors
    ///
    /// Returns an error if the draw or integer representation limit is
    /// exceeded.
    pub fn zip_rank(&mut self) -> Result<u64, RngError> {
        let mut rank = 0_u64;
        for _ in 0..MAX_RNG_DRAWS_PER_INSERT {
            let word = self.next_u64();
            if word == 0 {
                rank = rank.checked_add(64).ok_or(RngError::RankOverflow)?;
                continue;
            }
            return rank
                .checked_add(u64::from(word.trailing_zeros()))
                .ok_or(RngError::RankOverflow);
        }
        Err(RngError::DrawLimitExceeded)
    }

    #[cfg(test)]
    const fn from_state(state: [u64; 4]) -> Self {
        Self { state, draws: 0 }
    }
}

/// Derives one deterministic hash key without creating or consuming an RNG
/// stream. Hash KDF labels are versioned separately from RNG domains.
pub fn derive_hash_key(seed: u64, domain_label: &str) -> u64 {
    let mut state = seed ^ fnv1a64(domain_label.as_bytes());
    splitmix64_next(&mut state)
}

/// Computes 64-bit FNV-1a over canonical bytes.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

fn splitmix64_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(SPLITMIX_INCREMENT);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn bounded_u64_with(
    bound: u64,
    maximum_draws: u32,
    mut next: impl FnMut() -> u64,
) -> Result<u64, RngError> {
    if bound == 0 {
        return Err(RngError::ZeroBound);
    }
    let threshold = bound.wrapping_neg() % bound;
    for _ in 0..maximum_draws {
        let value = next();
        if value >= threshold {
            return Ok(value % bound);
        }
    }
    Err(RngError::DrawLimitExceeded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_matches_published_vector() {
        assert_eq!(fnv1a64(b"hello"), 0xa430_d846_80aa_bd0b);
    }

    #[test]
    fn xoshiro_reference_state_matches_first_output() {
        let mut rng = RngV1::from_state([1, 2, 3, 4]);

        assert_eq!(rng.next_u64(), 11_520);
        assert_eq!(rng.state(), [7, 0, 262_146, 211_106_232_532_992]);
        assert_eq!(rng.draws(), 1);
    }

    #[test]
    fn domain_labels_create_independent_streams() {
        let a = RngV1::from_seed(42, "rng.algorithm.treap.priority");
        let b = RngV1::from_seed(42, "rng.algorithm.zip.rank");
        let a_again = RngV1::from_seed(42, "rng.algorithm.treap.priority");

        assert_ne!(a, b);
        assert_eq!(a, a_again);
    }

    #[test]
    fn skip_list_max_level_one_consumes_no_draw() {
        let mut rng = RngV1::from_seed(0, "rng.algorithm.skip-list.height");

        assert_eq!(rng.skip_list_height(1, 2), Ok(1));
        assert_eq!(rng.draws(), 0);
    }

    #[test]
    fn invalid_ratios_are_rejected_without_consuming_rng() {
        let mut rng = RngV1::from_seed(7, "rng.generator.operations.key-selection");

        assert_eq!(rng.bernoulli(1, 0), Err(RngError::ZeroBound));
        assert_eq!(rng.bernoulli(3, 2), Err(RngError::InvalidBernoulliRatio));
        assert_eq!(rng.draws(), 0);
    }

    #[test]
    fn domain_labels_have_unique_seeded_states() {
        let states: std::collections::HashSet<_> = RNG_DOMAIN_LABELS
            .iter()
            .map(|label| RngV1::from_seed(0, label).state())
            .collect();

        assert_eq!(states.len(), RNG_DOMAIN_LABELS.len());
        assert!(
            RNG_DOMAIN_LABELS
                .iter()
                .all(|label| !HASH_KDF_LABELS.contains(label))
        );
    }

    #[test]
    fn operation_ledger_is_complete_and_only_new_insert_draws() {
        let algorithms: Vec<_> = ALGORITHM_DRAW_LEDGER
            .iter()
            .map(|row| row.algorithm)
            .collect();
        assert_eq!(
            algorithms,
            [
                "avl",
                "wbt",
                "aa",
                "llrb",
                "treap",
                "zip",
                "splay",
                "scapegoat",
                "skip-list",
                "b-tree",
                "veb",
                "x-fast",
                "y-fast",
            ]
        );
        assert!(ALGORITHM_DRAW_LEDGER.iter().all(|row| {
            row.overwrite == OperationDrawRule::None
                && row.other_operations == OperationDrawRule::None
        }));
        assert_eq!(
            ALGORITHM_DRAW_LEDGER
                .iter()
                .filter(|row| row.uses_hash_kdf)
                .map(|row| row.algorithm)
                .collect::<Vec<_>>(),
            ["veb", "x-fast", "y-fast"]
        );
    }

    #[test]
    fn bounded_sampling_rejects_at_draw_cap_without_distribution_fallback() {
        let mut draws = 0;
        let result = bounded_u64_with(3, 7, || {
            draws += 1;
            0
        });

        assert_eq!(result, Err(RngError::DrawLimitExceeded));
        assert_eq!(draws, 7);
    }
}
