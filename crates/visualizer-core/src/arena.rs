//! Safe generational arena with checkpoint-stable free-list order.

use std::collections::HashSet;
use std::mem::size_of;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable arena identity. Both fields are part of the checkpoint contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ArenaKey {
    index: u32,
    generation: u32,
}

impl ArenaKey {
    /// Returns the zero-based slot index.
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Returns the slot generation.
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ArenaSlot<T> {
    Occupied {
        generation: u32,
        value: T,
    },
    Vacant {
        generation: u32,
        next_free: Option<u32>,
    },
    Retired {
        generation: u32,
    },
}

/// Logical checkpoint representation. Spare vector capacity is intentionally
/// excluded, while every identity-affecting field is retained.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaSnapshot<T> {
    slots: Vec<ArenaSlot<T>>,
    free_head: Option<u32>,
    len: u32,
}

/// Arena operation or checkpoint validation failure.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ArenaError {
    /// No additional `u32` slot index can be represented.
    #[error("arena slot index space is exhausted")]
    SlotIndexExhausted,
    /// Allocation could not be reserved without exceeding available memory.
    #[error("arena allocation failed")]
    AllocationFailed,
    /// A checkpoint violates the arena's free-list or length invariants.
    #[error("arena checkpoint is corrupt: {0}")]
    CorruptCheckpoint(&'static str),
    /// Slice APIs require a positive work budget.
    #[error("arena codec slice budget must be positive")]
    ZeroSliceBudget,
    /// Binary input ended before a complete record was available.
    #[error("arena checkpoint binary is truncated")]
    TruncatedCodec,
    /// Binary input contains an unknown tag or trailing data.
    #[error("arena checkpoint binary format is invalid")]
    InvalidCodec,
}

const ARENA_CODEC_MAGIC: [u8; 4] = *b"ARV1";
const ARENA_CODEC_HEADER_BYTES: usize = 16;
const NONE_INDEX: u32 = u32::MAX;

/// Incremental encode result.
#[derive(Debug)]
pub enum EncodeProgress {
    /// More slot records remain.
    Pending,
    /// Complete compact checkpoint bytes.
    Complete(Vec<u8>),
}

/// Incremental restore result.
#[derive(Debug)]
pub enum RestoreProgress {
    /// More slot records remain.
    Pending,
    /// Fully validated restored arena.
    Complete(GenerationalArena<u64>),
}

/// Borrowing compact-codec continuation. The header is emitted at begin time;
/// each resume processes at most the caller's record budget.
#[derive(Debug)]
pub struct ArenaEncodeContinuation<'a> {
    snapshot: &'a ArenaSnapshot<u64>,
    next_slot: usize,
    output: Vec<u8>,
    complete: bool,
}

impl ArenaEncodeContinuation<'_> {
    /// Encodes at most `max_records` slot records.
    ///
    /// # Errors
    ///
    /// Rejects a zero budget, a second resume after completion, and allocation
    /// failure before growing the output buffer.
    pub fn resume(&mut self, max_records: usize) -> Result<EncodeProgress, ArenaError> {
        if max_records == 0 {
            return Err(ArenaError::ZeroSliceBudget);
        }
        if self.complete {
            return Err(ArenaError::InvalidCodec);
        }
        let end = self
            .next_slot
            .saturating_add(max_records)
            .min(self.snapshot.slots.len());
        let records = end - self.next_slot;
        self.output
            .try_reserve(records.saturating_mul(13))
            .map_err(|_| ArenaError::AllocationFailed)?;
        for slot in &self.snapshot.slots[self.next_slot..end] {
            encode_slot(slot, &mut self.output);
        }
        self.next_slot = end;
        if self.next_slot == self.snapshot.slots.len() {
            self.complete = true;
            Ok(EncodeProgress::Complete(std::mem::take(&mut self.output)))
        } else {
            Ok(EncodeProgress::Pending)
        }
    }
}

/// Owning compact restore continuation. `Rc<[u8]>` keeps the source alive
/// without a self-referential borrow while records are decoded across slices.
#[derive(Debug)]
pub struct ArenaRestoreContinuation {
    bytes: Rc<[u8]>,
    cursor: usize,
    expected_slots: usize,
    free_head: Option<u32>,
    len: u32,
    slots: Vec<ArenaSlot<u64>>,
    complete: bool,
}

impl ArenaRestoreContinuation {
    /// Decodes at most `max_records` slot records and validates the logical
    /// free-list when complete.
    ///
    /// # Errors
    ///
    /// Rejects malformed/truncated input, a zero budget, allocation failure,
    /// or a logical arena invariant violation.
    pub fn resume(&mut self, max_records: usize) -> Result<RestoreProgress, ArenaError> {
        if max_records == 0 {
            return Err(ArenaError::ZeroSliceBudget);
        }
        if self.complete {
            return Err(ArenaError::InvalidCodec);
        }
        let remaining = self.expected_slots - self.slots.len();
        let records = remaining.min(max_records);
        self.slots
            .try_reserve(records)
            .map_err(|_| ArenaError::AllocationFailed)?;
        for _ in 0..records {
            self.slots.push(decode_slot(&self.bytes, &mut self.cursor)?);
        }
        if self.slots.len() != self.expected_slots {
            return Ok(RestoreProgress::Pending);
        }
        if self.cursor != self.bytes.len() {
            return Err(ArenaError::InvalidCodec);
        }

        self.complete = true;
        let snapshot = ArenaSnapshot {
            slots: std::mem::take(&mut self.slots),
            free_head: self.free_head,
            len: self.len,
        };
        GenerationalArena::from_snapshot(snapshot).map(RestoreProgress::Complete)
    }
}

/// A safe arena whose future IDs are stable across snapshot restoration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GenerationalArena<T> {
    slots: Vec<ArenaSlot<T>>,
    free_head: Option<u32>,
    len: u32,
}

impl<T> GenerationalArena<T> {
    /// Creates an empty arena without allocating.
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_head: None,
            len: 0,
        }
    }

    /// Returns the number of occupied slots.
    pub const fn len(&self) -> u32 {
        self.len
    }

    /// Returns whether the arena contains no values.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the backing slot capacity used by memory admission.
    pub fn capacity(&self) -> usize {
        self.slots.capacity()
    }

    /// Estimates bytes owned directly by the arena, excluding allocations
    /// reachable through `T`.
    pub fn estimated_bytes(&self) -> usize {
        self.slots
            .capacity()
            .saturating_mul(size_of::<ArenaSlot<T>>())
    }

    /// Iterates occupied values in stable slot order.
    pub fn iter(&self) -> impl Iterator<Item = (ArenaKey, &T)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            if let ArenaSlot::Occupied { generation, value } = slot {
                let index = u32::try_from(index).ok()?;
                Some((
                    ArenaKey {
                        index,
                        generation: *generation,
                    },
                    value,
                ))
            } else {
                None
            }
        })
    }

    /// Reserves room for `additional` slots before a growth phase.
    ///
    /// # Errors
    ///
    /// Returns [`ArenaError::AllocationFailed`] if the allocator rejects the
    /// request.
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), ArenaError> {
        self.slots
            .try_reserve(additional)
            .map_err(|_| ArenaError::AllocationFailed)
    }

    /// Inserts a value and returns its stable identity.
    ///
    /// # Errors
    ///
    /// Returns an error if the slot index space is exhausted, memory cannot be
    /// reserved, or internal free-list state is invalid.
    pub fn try_insert(&mut self, value: T) -> Result<ArenaKey, ArenaError> {
        if let Some(index) = self.free_head {
            return self.insert_into_vacant(index, value);
        }

        let index = u32::try_from(self.slots.len()).map_err(|_| ArenaError::SlotIndexExhausted)?;
        if index == u32::MAX {
            return Err(ArenaError::SlotIndexExhausted);
        }

        self.try_reserve(1)?;
        let generation = 1;
        self.slots.push(ArenaSlot::Occupied { generation, value });
        self.len = self
            .len
            .checked_add(1)
            .ok_or(ArenaError::CorruptCheckpoint("occupied length overflow"))?;
        Ok(ArenaKey { index, generation })
    }

    fn insert_into_vacant(&mut self, index: u32, value: T) -> Result<ArenaKey, ArenaError> {
        let slot = self
            .slots
            .get_mut(index as usize)
            .ok_or(ArenaError::CorruptCheckpoint("free head is out of bounds"))?;
        let ArenaSlot::Vacant {
            generation,
            next_free,
        } = slot
        else {
            return Err(ArenaError::CorruptCheckpoint(
                "free head does not reference a vacant slot",
            ));
        };

        let generation = *generation;
        self.free_head = *next_free;
        *slot = ArenaSlot::Occupied { generation, value };
        self.len = self
            .len
            .checked_add(1)
            .ok_or(ArenaError::CorruptCheckpoint("occupied length overflow"))?;
        Ok(ArenaKey { index, generation })
    }

    /// Returns a shared reference when the key still identifies an occupied
    /// slot.
    pub fn get(&self, key: ArenaKey) -> Option<&T> {
        match self.slots.get(key.index as usize)? {
            ArenaSlot::Occupied { generation, value } if *generation == key.generation => {
                Some(value)
            }
            ArenaSlot::Occupied { .. } | ArenaSlot::Vacant { .. } | ArenaSlot::Retired { .. } => {
                None
            }
        }
    }

    /// Returns a mutable reference when the key still identifies an occupied
    /// slot.
    pub fn get_mut(&mut self, key: ArenaKey) -> Option<&mut T> {
        match self.slots.get_mut(key.index as usize)? {
            ArenaSlot::Occupied { generation, value } if *generation == key.generation => {
                Some(value)
            }
            ArenaSlot::Occupied { .. } | ArenaSlot::Vacant { .. } | ArenaSlot::Retired { .. } => {
                None
            }
        }
    }

    /// Removes a value. Stale and out-of-range keys have no effect.
    pub fn remove(&mut self, key: ArenaKey) -> Option<T> {
        let slot = self.slots.get_mut(key.index as usize)?;
        if !matches!(slot, ArenaSlot::Occupied { generation, .. } if *generation == key.generation)
        {
            return None;
        }

        let replacement = match key.generation.checked_add(1) {
            Some(generation) => ArenaSlot::Vacant {
                generation,
                next_free: self.free_head,
            },
            None => ArenaSlot::Retired {
                generation: key.generation,
            },
        };
        let old = std::mem::replace(slot, replacement);
        if key.generation != u32::MAX {
            self.free_head = Some(key.index);
        }
        self.len -= 1;

        match old {
            ArenaSlot::Occupied { value, .. } => Some(value),
            ArenaSlot::Vacant { .. } | ArenaSlot::Retired { .. } => None,
        }
    }

    /// Produces the logical checkpoint state without spare capacity.
    pub fn snapshot(&self) -> ArenaSnapshot<T>
    where
        T: Clone,
    {
        ArenaSnapshot {
            slots: self.slots.clone(),
            free_head: self.free_head,
            len: self.len,
        }
    }

    /// Restores an arena only after validating all identity invariants.
    ///
    /// # Errors
    ///
    /// Returns [`ArenaError::CorruptCheckpoint`] for a bad length, invalid
    /// index, cycle, duplicate free slot, or an unlinked reusable slot.
    pub fn from_snapshot(snapshot: ArenaSnapshot<T>) -> Result<Self, ArenaError> {
        Self::validate_snapshot(&snapshot)?;
        Ok(Self {
            slots: snapshot.slots,
            free_head: snapshot.free_head,
            len: snapshot.len,
        })
    }

    fn validate_snapshot(snapshot: &ArenaSnapshot<T>) -> Result<(), ArenaError> {
        if snapshot.slots.len() >= u32::MAX as usize {
            return Err(ArenaError::CorruptCheckpoint("too many slots"));
        }

        let occupied = snapshot
            .slots
            .iter()
            .filter(|slot| matches!(slot, ArenaSlot::Occupied { .. }))
            .count();
        if occupied != snapshot.len as usize {
            return Err(ArenaError::CorruptCheckpoint("occupied length mismatch"));
        }

        let reusable = snapshot
            .slots
            .iter()
            .filter(|slot| matches!(slot, ArenaSlot::Vacant { .. }))
            .count();
        let mut visited = HashSet::with_capacity(reusable);
        let mut cursor = snapshot.free_head;
        while let Some(index) = cursor {
            if !visited.insert(index) {
                return Err(ArenaError::CorruptCheckpoint("free-list cycle"));
            }
            cursor = match snapshot.slots.get(index as usize) {
                Some(ArenaSlot::Vacant { next_free, .. }) => *next_free,
                Some(ArenaSlot::Occupied { .. } | ArenaSlot::Retired { .. }) => {
                    return Err(ArenaError::CorruptCheckpoint(
                        "free list references a non-vacant slot",
                    ));
                }
                None => {
                    return Err(ArenaError::CorruptCheckpoint(
                        "free-list index is out of bounds",
                    ));
                }
            };
        }
        if visited.len() != reusable {
            return Err(ArenaError::CorruptCheckpoint(
                "reusable slot is missing from free list",
            ));
        }
        Ok(())
    }
}

impl GenerationalArena<u64> {
    /// Begins compact incremental checkpoint encoding without traversing slots.
    ///
    /// # Errors
    ///
    /// Rejects snapshots whose slot count cannot be represented by V1.
    pub fn begin_encode_snapshot(
        snapshot: &ArenaSnapshot<u64>,
    ) -> Result<ArenaEncodeContinuation<'_>, ArenaError> {
        let slot_count =
            u32::try_from(snapshot.slots.len()).map_err(|_| ArenaError::SlotIndexExhausted)?;
        let mut output = Vec::new();
        output
            .try_reserve(ARENA_CODEC_HEADER_BYTES)
            .map_err(|_| ArenaError::AllocationFailed)?;
        output.extend_from_slice(&ARENA_CODEC_MAGIC);
        output.extend_from_slice(&slot_count.to_le_bytes());
        output.extend_from_slice(&snapshot.free_head.unwrap_or(NONE_INDEX).to_le_bytes());
        output.extend_from_slice(&snapshot.len.to_le_bytes());
        Ok(ArenaEncodeContinuation {
            snapshot,
            next_slot: 0,
            output,
            complete: false,
        })
    }

    /// Begins compact incremental restore by parsing only the fixed header.
    ///
    /// # Errors
    ///
    /// Rejects a truncated or wrong-version header. Slot allocation and record
    /// parsing are deferred to [`ArenaRestoreContinuation::resume`].
    pub fn begin_restore_snapshot(bytes: Rc<[u8]>) -> Result<ArenaRestoreContinuation, ArenaError> {
        if bytes.len() < ARENA_CODEC_HEADER_BYTES {
            return Err(ArenaError::TruncatedCodec);
        }
        if bytes[..4] != ARENA_CODEC_MAGIC {
            return Err(ArenaError::InvalidCodec);
        }
        let expected_slots =
            usize::try_from(read_u32(&bytes, 4)?).map_err(|_| ArenaError::InvalidCodec)?;
        let encoded_free_head = read_u32(&bytes, 8)?;
        let free_head = (encoded_free_head != NONE_INDEX).then_some(encoded_free_head);
        let len = read_u32(&bytes, 12)?;
        Ok(ArenaRestoreContinuation {
            bytes,
            cursor: ARENA_CODEC_HEADER_BYTES,
            expected_slots,
            free_head,
            len,
            slots: Vec::new(),
            complete: false,
        })
    }
}

fn encode_slot(slot: &ArenaSlot<u64>, output: &mut Vec<u8>) {
    match slot {
        ArenaSlot::Occupied { generation, value } => {
            output.push(0);
            output.extend_from_slice(&generation.to_le_bytes());
            output.extend_from_slice(&value.to_le_bytes());
        }
        ArenaSlot::Vacant {
            generation,
            next_free,
        } => {
            output.push(1);
            output.extend_from_slice(&generation.to_le_bytes());
            output.extend_from_slice(&next_free.unwrap_or(NONE_INDEX).to_le_bytes());
        }
        ArenaSlot::Retired { generation } => {
            output.push(2);
            output.extend_from_slice(&generation.to_le_bytes());
        }
    }
}

fn decode_slot(bytes: &[u8], cursor: &mut usize) -> Result<ArenaSlot<u64>, ArenaError> {
    let tag = *bytes.get(*cursor).ok_or(ArenaError::TruncatedCodec)?;
    *cursor += 1;
    let generation = read_u32(bytes, *cursor)?;
    *cursor += 4;
    match tag {
        0 => {
            let value = read_u64(bytes, *cursor)?;
            *cursor += 8;
            Ok(ArenaSlot::Occupied { generation, value })
        }
        1 => {
            let encoded_next = read_u32(bytes, *cursor)?;
            *cursor += 4;
            Ok(ArenaSlot::Vacant {
                generation,
                next_free: (encoded_next != NONE_INDEX).then_some(encoded_next),
            })
        }
        2 => Ok(ArenaSlot::Retired { generation }),
        _ => Err(ArenaError::InvalidCodec),
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ArenaError> {
    let encoded: [u8; 4] = bytes
        .get(offset..offset.saturating_add(4))
        .ok_or(ArenaError::TruncatedCodec)?
        .try_into()
        .map_err(|_| ArenaError::TruncatedCodec)?;
    Ok(u32::from_le_bytes(encoded))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, ArenaError> {
    let encoded: [u8; 8] = bytes
        .get(offset..offset.saturating_add(8))
        .ok_or(ArenaError::TruncatedCodec)?
        .try_into()
        .map_err(|_| ArenaError::TruncatedCodec)?;
    Ok(u64::from_le_bytes(encoded))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn stale_key_never_reads_reused_slot() {
        let mut arena = GenerationalArena::new();
        let old = arena.try_insert("old").expect("test insert must succeed");

        assert_eq!(arena.remove(old), Some("old"));
        let new = arena.try_insert("new").expect("test insert must succeed");

        assert_eq!(old.index(), new.index());
        assert_ne!(old.generation(), new.generation());
        assert_eq!(arena.get(old), None);
        assert_eq!(arena.get(new), Some(&"new"));
    }

    #[test]
    fn snapshot_preserves_future_id_order() {
        let mut uninterrupted = GenerationalArena::new();
        let keys: Vec<_> = (0..5)
            .map(|value| {
                uninterrupted
                    .try_insert(value)
                    .expect("test insert must succeed")
            })
            .collect();
        assert_eq!(uninterrupted.remove(keys[1]), Some(1));
        assert_eq!(uninterrupted.remove(keys[3]), Some(3));

        let mut restored = GenerationalArena::from_snapshot(uninterrupted.snapshot())
            .expect("valid snapshot must restore");
        let uninterrupted_ids = [10, 11].map(|value| {
            uninterrupted
                .try_insert(value)
                .expect("insert must succeed")
        });
        let restored_ids =
            [10, 11].map(|value| restored.try_insert(value).expect("insert must succeed"));

        assert_eq!(restored_ids, uninterrupted_ids);
        assert_eq!(restored.snapshot(), uninterrupted.snapshot());
    }

    #[test]
    fn corrupt_free_list_cycle_is_rejected() {
        let snapshot = ArenaSnapshot {
            slots: vec![ArenaSlot::Vacant {
                generation: 2,
                next_free: Some(0),
            }],
            free_head: Some(0),
            len: 0,
        };

        assert_eq!(
            GenerationalArena::<u8>::from_snapshot(snapshot),
            Err(ArenaError::CorruptCheckpoint("free-list cycle"))
        );
    }

    fn encode(snapshot: &ArenaSnapshot<u64>, budget: usize) -> Vec<u8> {
        let mut continuation = GenerationalArena::begin_encode_snapshot(snapshot)
            .expect("bounded snapshot begins encoding");
        loop {
            match continuation
                .resume(budget)
                .expect("encoding slice succeeds")
            {
                EncodeProgress::Pending => {}
                EncodeProgress::Complete(bytes) => return bytes,
            }
        }
    }

    fn restore(bytes: Rc<[u8]>, budget: usize) -> GenerationalArena<u64> {
        let mut continuation =
            GenerationalArena::begin_restore_snapshot(bytes).expect("valid header begins restore");
        loop {
            match continuation.resume(budget).expect("restore slice succeeds") {
                RestoreProgress::Pending => {}
                RestoreProgress::Complete(arena) => return arena,
            }
        }
    }

    #[test]
    fn compact_codec_is_slice_budget_independent() {
        let mut arena = GenerationalArena::new();
        let keys: Vec<_> = (0..1_000)
            .map(|value| arena.try_insert(value).expect("bounded insert"))
            .collect();
        for key in keys.iter().step_by(3) {
            assert!(arena.remove(*key).is_some());
        }
        let snapshot = arena.snapshot();

        let one = encode(&snapshot, 1);
        let seventeen = encode(&snapshot, 17);
        let all = encode(&snapshot, usize::MAX);

        assert_eq!(one, seventeen);
        assert_eq!(one, all);
        assert!(one.len() <= ARENA_CODEC_HEADER_BYTES + snapshot.slots.len() * 13);
        for budget in [1, 19, usize::MAX] {
            let restored = restore(Rc::from(one.clone()), budget);
            assert_eq!(restored.snapshot(), snapshot);
        }
    }

    #[test]
    fn high_churn_codec_preserves_future_ids() {
        let mut uninterrupted = GenerationalArena::new();
        let keys: Vec<_> = (0..50_000)
            .map(|value| uninterrupted.try_insert(value).expect("bounded insert"))
            .collect();
        for key in keys.iter().step_by(2) {
            assert!(uninterrupted.remove(*key).is_some());
        }
        for value in 50_000..75_000 {
            let _ = uninterrupted.try_insert(value).expect("bounded insert");
        }

        let bytes = encode(&uninterrupted.snapshot(), 257);
        let source: Rc<[u8]> = Rc::from(bytes);
        let mut continuation =
            GenerationalArena::begin_restore_snapshot(Rc::clone(&source)).expect("valid header");
        drop(source);
        let mut restored = loop {
            match continuation.resume(131).expect("restore slice") {
                RestoreProgress::Pending => {}
                RestoreProgress::Complete(arena) => break arena,
            }
        };

        for value in 75_000..76_000 {
            assert_eq!(
                restored.try_insert(value).expect("restored insert"),
                uninterrupted
                    .try_insert(value)
                    .expect("uninterrupted insert")
            );
        }
        assert_eq!(restored.snapshot(), uninterrupted.snapshot());
    }

    #[test]
    fn truncated_codec_is_rejected_without_partial_arena() {
        let mut arena = GenerationalArena::new();
        let _ = arena.try_insert(7).expect("bounded insert");
        let bytes = encode(&arena.snapshot(), 1);

        for length in 0..bytes.len() {
            let source: Rc<[u8]> = Rc::from(bytes[..length].to_vec());
            match GenerationalArena::begin_restore_snapshot(source) {
                Err(ArenaError::TruncatedCodec) => {}
                Ok(mut continuation) => assert!(matches!(
                    continuation.resume(1),
                    Err(ArenaError::TruncatedCodec)
                )),
                other => panic!("unexpected truncated result: {other:?}"),
            }
        }
    }

    proptest! {
        #[test]
        fn snapshot_round_trip_preserves_state(actions in prop::collection::vec(any::<bool>(), 0..256)) {
            let mut arena = GenerationalArena::new();
            let mut live = Vec::new();
            for action in actions {
                if action || live.is_empty() {
                    let key = arena.try_insert(live.len() as u64).expect("bounded test insert must succeed");
                    live.push(key);
                } else if let Some(key) = live.pop() {
                    let _ = arena.remove(key);
                }
            }

            let snapshot = arena.snapshot();
            let restored = GenerationalArena::from_snapshot(snapshot.clone()).expect("generated snapshot must be valid");
            prop_assert_eq!(restored.snapshot(), snapshot);
        }
    }
}
