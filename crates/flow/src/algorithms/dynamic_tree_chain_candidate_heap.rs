//! Bounded maintained heap for dynamic tree-chain cycle candidates.
//!
//! The source keeps every fundamental-spanner candidate in a heap and refreshes
//! its quality when the corresponding embedding changes. The repository keeps
//! an indexed binary heap across Algorithm 2 snapshots and applies only source
//! insertion, quality replacement, and removal differences from each checked
//! query. The standalone rebuild-and-drain certificate remains available for
//! primitive tests. This preserves the source state boundary without claiming
//! its asymptotic or amortized update bound.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use num_rational::BigRational;
use thiserror::Error;

use super::{
    DynamicTreeChainCycleCandidate, DynamicTreeChainCycleQueryEventKind,
    DynamicTreeChainCycleQueryTraceResult, DynamicTreeChainCycleSource,
};

/// One heap row tied to the stable query work order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainCandidateHeapEntry {
    /// Zero-based work-item ordinal from the checked `FindCycle` traversal.
    pub work_ordinal: usize,
    /// Exact lifted and scored root circulation.
    pub candidate: DynamicTreeChainCycleCandidate,
}

/// Exact bounded heap work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicTreeChainCandidateHeapMetrics {
    /// Candidate qualities refreshed from current embeddings and attributes.
    pub candidate_refreshes: u64,
    /// Entries inserted into the binary heap.
    pub heap_pushes: u64,
    /// Entries removed while certifying the complete heap order.
    pub heap_pops: u64,
    /// Existing entries whose exact circulation or quality changed.
    pub heap_updates: u64,
}

/// Complete rebuild-and-query certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainCandidateHeapTrace {
    /// Candidate rows in stable source work order.
    pub entries: Vec<DynamicTreeChainCandidateHeapEntry>,
    /// Entry indices in exact binary-heap pop order.
    pub pop_order: Vec<usize>,
    /// First heap result, or `None` when no nonzero candidate exists.
    pub selected: Option<DynamicTreeChainCycleCandidate>,
    /// Exact finite work counters.
    pub metrics: DynamicTreeChainCandidateHeapMetrics,
}

/// Maintained indexed-heap state in exact binary-heap order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DynamicTreeChainCandidateHeapState {
    /// Current candidates. Index zero is the exact `FindCycle` result.
    pub heap: Vec<DynamicTreeChainCandidateHeapEntry>,
}

/// One deterministic difference applied to the maintained source heap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicTreeChainCandidateHeapTransition {
    /// A source candidate disappeared from the current tree chain.
    Removed {
        /// Stable source identity.
        source: DynamicTreeChainCycleSource,
        /// Exact previous row.
        before: DynamicTreeChainCandidateHeapEntry,
    },
    /// A newly materialized source candidate was inserted.
    Inserted {
        /// Exact current row.
        after: DynamicTreeChainCandidateHeapEntry,
    },
    /// A retained source candidate changed circulation, work order, or score.
    Updated {
        /// Exact previous row.
        before: DynamicTreeChainCandidateHeapEntry,
        /// Exact current row with the same source identity.
        after: Box<DynamicTreeChainCandidateHeapEntry>,
    },
}

/// Exact incremental refresh certificate for one checked query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainCandidateHeapRefreshTrace {
    /// Heap before applying current query differences.
    pub before: DynamicTreeChainCandidateHeapState,
    /// Stable deterministic differences in application order.
    pub transitions: Vec<DynamicTreeChainCandidateHeapTransition>,
    /// Heap after all differences.
    pub after: DynamicTreeChainCandidateHeapState,
    /// Current maximum, or `None` for an empty candidate set.
    pub selected: Option<DynamicTreeChainCycleCandidate>,
    /// Exact inspected/inserted/removed/updated counts.
    pub metrics: DynamicTreeChainCandidateHeapMetrics,
}

/// Bounded candidate-heap failure.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum DynamicTreeChainCandidateHeapError {
    /// Query events do not form the required stable completed traversal.
    #[error("dynamic tree-chain candidate heap input is invalid")]
    InvalidInput,
    /// Checked work arithmetic overflowed.
    #[error("dynamic tree-chain candidate heap arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied heap certificate differs from independent reconstruction.
    #[error("dynamic tree-chain candidate heap trace verification failed")]
    TraceVerification,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HeapItem {
    ratio: BigRational,
    work_ordinal: usize,
    entry: usize,
}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.ratio
            .cmp(&other.ratio)
            .then_with(|| other.work_ordinal.cmp(&self.work_ordinal))
            .then_with(|| other.entry.cmp(&self.entry))
    }
}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Rebuilds and drains the bounded source candidate heap.
///
/// # Errors
///
/// Rejects incomplete query events and checked counter overflow.
pub fn trace_dynamic_tree_chain_candidate_heap(
    query: &DynamicTreeChainCycleQueryTraceResult,
) -> Result<DynamicTreeChainCandidateHeapTrace, DynamicTreeChainCandidateHeapError> {
    let entries = candidate_entries(query)?;
    build_trace(entries, false)
}

/// Refreshes a maintained indexed binary heap from one checked `FindCycle` query.
///
/// # Errors
///
/// Rejects malformed prior heap state, incomplete query events, duplicate
/// source identities, or checked counter overflow.
pub fn trace_dynamic_tree_chain_candidate_heap_refresh(
    previous: &DynamicTreeChainCandidateHeapState,
    query: &DynamicTreeChainCycleQueryTraceResult,
) -> Result<DynamicTreeChainCandidateHeapRefreshTrace, DynamicTreeChainCandidateHeapError> {
    validate_heap_state(previous, false)?;
    let current = candidate_entries(query)?;
    let transitions = candidate_transitions(previous, &current, false)?;
    let mut heap = IndexedCandidateHeap::from_state(previous)?;
    for transition in &transitions {
        heap.apply(transition)?;
    }
    let after = DynamicTreeChainCandidateHeapState { heap: heap.rows };
    validate_current_heap(&after, &current, false)?;
    let selected = after.heap.first().map(|row| row.candidate.clone());
    let metrics = refresh_metrics(current.len(), &transitions, false)?;
    let trace = DynamicTreeChainCandidateHeapRefreshTrace {
        before: previous.clone(),
        transitions,
        after,
        selected,
        metrics,
    };
    check_dynamic_tree_chain_candidate_heap_refresh(query, previous, &trace)?;
    Ok(trace)
}

/// Independently reconstructs the heap rows, comparator, and complete pop order.
///
/// # Errors
///
/// Rejects any entry, stable ordinal, pop order, selected candidate, or metric
/// drift from the supplied checked query transcript.
pub fn check_dynamic_tree_chain_candidate_heap_trace(
    query: &DynamicTreeChainCycleQueryTraceResult,
    trace: &DynamicTreeChainCandidateHeapTrace,
) -> Result<(), DynamicTreeChainCandidateHeapError> {
    let entries = candidate_entries(query)
        .map_err(|_| DynamicTreeChainCandidateHeapError::TraceVerification)?;
    let expected = build_trace(entries, true)?;
    if &expected != trace {
        return Err(DynamicTreeChainCandidateHeapError::TraceVerification);
    }
    Ok(())
}

/// Independently checks a maintained heap refresh without invoking its indexed
/// update implementation.
///
/// The checker derives the exact set differences from stable source identity,
/// validates both heap orders explicitly, compares membership against every
/// current candidate row, and reconstructs all published counters.
///
/// # Errors
///
/// Rejects malformed prior/final heaps or any transition, candidate, selected
/// value, or metric drift.
pub fn check_dynamic_tree_chain_candidate_heap_refresh(
    query: &DynamicTreeChainCycleQueryTraceResult,
    previous: &DynamicTreeChainCandidateHeapState,
    trace: &DynamicTreeChainCandidateHeapRefreshTrace,
) -> Result<(), DynamicTreeChainCandidateHeapError> {
    validate_heap_state(previous, true)?;
    let current = candidate_entries(query)
        .map_err(|_| DynamicTreeChainCandidateHeapError::TraceVerification)?;
    let transitions = candidate_transitions(previous, &current, true)?;
    validate_current_heap(&trace.after, &current, true)?;
    let selected = trace.after.heap.first().map(|row| row.candidate.clone());
    let metrics = refresh_metrics(current.len(), &transitions, true)?;
    if trace.before != *previous
        || trace.transitions != transitions
        || trace.selected != selected
        || trace.metrics != metrics
    {
        return Err(DynamicTreeChainCandidateHeapError::TraceVerification);
    }
    Ok(())
}

struct IndexedCandidateHeap {
    rows: Vec<DynamicTreeChainCandidateHeapEntry>,
    positions: BTreeMap<DynamicTreeChainCycleSource, usize>,
}

impl IndexedCandidateHeap {
    fn from_state(
        state: &DynamicTreeChainCandidateHeapState,
    ) -> Result<Self, DynamicTreeChainCandidateHeapError> {
        let positions = state
            .heap
            .iter()
            .enumerate()
            .map(|(position, row)| (row.candidate.source, position))
            .collect::<BTreeMap<_, _>>();
        if positions.len() != state.heap.len() {
            return Err(DynamicTreeChainCandidateHeapError::InvalidInput);
        }
        Ok(Self {
            rows: state.heap.clone(),
            positions,
        })
    }

    fn apply(
        &mut self,
        transition: &DynamicTreeChainCandidateHeapTransition,
    ) -> Result<(), DynamicTreeChainCandidateHeapError> {
        match transition {
            DynamicTreeChainCandidateHeapTransition::Removed { source, before } => {
                let position = self
                    .positions
                    .get(source)
                    .copied()
                    .ok_or(DynamicTreeChainCandidateHeapError::InvalidInput)?;
                if self.rows[position] != *before {
                    return Err(DynamicTreeChainCandidateHeapError::InvalidInput);
                }
                self.remove(position);
            }
            DynamicTreeChainCandidateHeapTransition::Inserted { after } => {
                if self.positions.contains_key(&after.candidate.source) {
                    return Err(DynamicTreeChainCandidateHeapError::InvalidInput);
                }
                self.insert(after.clone());
            }
            DynamicTreeChainCandidateHeapTransition::Updated { before, after } => {
                if before.candidate.source != after.candidate.source {
                    return Err(DynamicTreeChainCandidateHeapError::InvalidInput);
                }
                let position = self
                    .positions
                    .get(&before.candidate.source)
                    .copied()
                    .ok_or(DynamicTreeChainCandidateHeapError::InvalidInput)?;
                if self.rows[position] != *before {
                    return Err(DynamicTreeChainCandidateHeapError::InvalidInput);
                }
                self.rows[position] = after.as_ref().clone();
                self.repair(position);
            }
        }
        Ok(())
    }

    fn insert(&mut self, row: DynamicTreeChainCandidateHeapEntry) {
        let source = row.candidate.source;
        let position = self.rows.len();
        self.rows.push(row);
        self.positions.insert(source, position);
        self.sift_up(position);
    }

    fn remove(&mut self, position: usize) {
        let removed = self.rows[position].candidate.source;
        let last = self.rows.len() - 1;
        if position != last {
            self.rows.swap(position, last);
            self.positions
                .insert(self.rows[position].candidate.source, position);
        }
        self.rows.pop();
        self.positions.remove(&removed);
        if position < self.rows.len() {
            self.repair(position);
        }
    }

    fn repair(&mut self, position: usize) {
        if position > 0 && entry_order(&self.rows[position], &self.rows[(position - 1) / 2]).is_gt()
        {
            self.sift_up(position);
        } else {
            self.sift_down(position);
        }
    }

    fn sift_up(&mut self, mut position: usize) {
        while position > 0 {
            let parent = (position - 1) / 2;
            if !entry_order(&self.rows[position], &self.rows[parent]).is_gt() {
                break;
            }
            self.swap_rows(position, parent);
            position = parent;
        }
    }

    fn sift_down(&mut self, mut position: usize) {
        loop {
            let left = position * 2 + 1;
            if left >= self.rows.len() {
                return;
            }
            let right = left + 1;
            let best = if right < self.rows.len()
                && entry_order(&self.rows[right], &self.rows[left]).is_gt()
            {
                right
            } else {
                left
            };
            if !entry_order(&self.rows[best], &self.rows[position]).is_gt() {
                return;
            }
            self.swap_rows(position, best);
            position = best;
        }
    }

    fn swap_rows(&mut self, left: usize, right: usize) {
        self.rows.swap(left, right);
        self.positions
            .insert(self.rows[left].candidate.source, left);
        self.positions
            .insert(self.rows[right].candidate.source, right);
    }
}

fn candidate_transitions(
    previous: &DynamicTreeChainCandidateHeapState,
    current: &[DynamicTreeChainCandidateHeapEntry],
    audit: bool,
) -> Result<Vec<DynamicTreeChainCandidateHeapTransition>, DynamicTreeChainCandidateHeapError> {
    let previous_rows = source_map(&previous.heap, audit)?;
    let current_rows = source_map(current, audit)?;
    let current_sources = current_rows.keys().copied().collect::<BTreeSet<_>>();
    let mut transitions = previous_rows
        .iter()
        .filter(|(source, _)| !current_sources.contains(source))
        .map(
            |(&source, before)| DynamicTreeChainCandidateHeapTransition::Removed {
                source,
                before: (*before).clone(),
            },
        )
        .collect::<Vec<_>>();
    for after in current {
        match previous_rows.get(&after.candidate.source) {
            None => transitions.push(DynamicTreeChainCandidateHeapTransition::Inserted {
                after: after.clone(),
            }),
            Some(before) if **before != *after => {
                transitions.push(DynamicTreeChainCandidateHeapTransition::Updated {
                    before: (*before).clone(),
                    after: Box::new(after.clone()),
                });
            }
            Some(_) => {}
        }
    }
    Ok(transitions)
}

fn source_map(
    entries: &[DynamicTreeChainCandidateHeapEntry],
    audit: bool,
) -> Result<
    BTreeMap<DynamicTreeChainCycleSource, &DynamicTreeChainCandidateHeapEntry>,
    DynamicTreeChainCandidateHeapError,
> {
    let map = entries
        .iter()
        .map(|row| (row.candidate.source, row))
        .collect::<BTreeMap<_, _>>();
    if map.len() != entries.len() {
        return Err(if audit {
            DynamicTreeChainCandidateHeapError::TraceVerification
        } else {
            DynamicTreeChainCandidateHeapError::InvalidInput
        });
    }
    Ok(map)
}

fn validate_heap_state(
    state: &DynamicTreeChainCandidateHeapState,
    audit: bool,
) -> Result<(), DynamicTreeChainCandidateHeapError> {
    source_map(&state.heap, audit)?;
    for child in 1..state.heap.len() {
        let parent = (child - 1) / 2;
        if entry_order(&state.heap[child], &state.heap[parent]).is_gt() {
            return Err(if audit {
                DynamicTreeChainCandidateHeapError::TraceVerification
            } else {
                DynamicTreeChainCandidateHeapError::InvalidInput
            });
        }
    }
    Ok(())
}

fn validate_current_heap(
    state: &DynamicTreeChainCandidateHeapState,
    current: &[DynamicTreeChainCandidateHeapEntry],
    audit: bool,
) -> Result<(), DynamicTreeChainCandidateHeapError> {
    validate_heap_state(state, audit)?;
    if source_map(&state.heap, audit)? != source_map(current, audit)? {
        return Err(if audit {
            DynamicTreeChainCandidateHeapError::TraceVerification
        } else {
            DynamicTreeChainCandidateHeapError::InvalidInput
        });
    }
    Ok(())
}

fn refresh_metrics(
    current_count: usize,
    transitions: &[DynamicTreeChainCandidateHeapTransition],
    audit: bool,
) -> Result<DynamicTreeChainCandidateHeapMetrics, DynamicTreeChainCandidateHeapError> {
    let count = |value| u64::try_from(value).map_err(|_| failure(audit));
    Ok(DynamicTreeChainCandidateHeapMetrics {
        candidate_refreshes: count(current_count)?,
        heap_pushes: count(
            transitions
                .iter()
                .filter(|row| {
                    matches!(
                        row,
                        DynamicTreeChainCandidateHeapTransition::Inserted { .. }
                    )
                })
                .count(),
        )?,
        heap_pops: count(
            transitions
                .iter()
                .filter(|row| {
                    matches!(row, DynamicTreeChainCandidateHeapTransition::Removed { .. })
                })
                .count(),
        )?,
        heap_updates: count(
            transitions
                .iter()
                .filter(|row| {
                    matches!(row, DynamicTreeChainCandidateHeapTransition::Updated { .. })
                })
                .count(),
        )?,
    })
}

fn entry_order(
    left: &DynamicTreeChainCandidateHeapEntry,
    right: &DynamicTreeChainCandidateHeapEntry,
) -> Ordering {
    left.candidate
        .ratio
        .cmp(&right.candidate.ratio)
        .then_with(|| right.work_ordinal.cmp(&left.work_ordinal))
        .then_with(|| right.candidate.source.cmp(&left.candidate.source))
}

fn candidate_entries(
    query: &DynamicTreeChainCycleQueryTraceResult,
) -> Result<Vec<DynamicTreeChainCandidateHeapEntry>, DynamicTreeChainCandidateHeapError> {
    if query
        .events
        .last()
        .is_none_or(|event| !matches!(event.kind, DynamicTreeChainCycleQueryEventKind::Completed))
    {
        return Err(DynamicTreeChainCandidateHeapError::InvalidInput);
    }
    Ok(query
        .events
        .iter()
        .enumerate()
        .filter_map(|(work_ordinal, event)| match &event.kind {
            DynamicTreeChainCycleQueryEventKind::CandidateEvaluated { candidate, .. } => {
                Some(DynamicTreeChainCandidateHeapEntry {
                    work_ordinal,
                    candidate: candidate.as_ref().clone(),
                })
            }
            _ => None,
        })
        .collect())
}

fn build_trace(
    entries: Vec<DynamicTreeChainCandidateHeapEntry>,
    audit: bool,
) -> Result<DynamicTreeChainCandidateHeapTrace, DynamicTreeChainCandidateHeapError> {
    let mut heap = BinaryHeap::with_capacity(entries.len());
    for (entry, row) in entries.iter().enumerate() {
        heap.push(HeapItem {
            ratio: row.candidate.ratio.clone(),
            work_ordinal: row.work_ordinal,
            entry,
        });
    }
    let mut pop_order = Vec::with_capacity(entries.len());
    while let Some(item) = heap.pop() {
        pop_order.push(item.entry);
    }
    let selected = pop_order
        .first()
        .map(|&entry| entries[entry].candidate.clone());
    let count = u64::try_from(entries.len()).map_err(|_| failure(audit))?;
    Ok(DynamicTreeChainCandidateHeapTrace {
        entries,
        pop_order,
        selected,
        metrics: DynamicTreeChainCandidateHeapMetrics {
            candidate_refreshes: count,
            heap_pushes: count,
            heap_pops: count,
            heap_updates: 0,
        },
    })
}

fn failure(audit: bool) -> DynamicTreeChainCandidateHeapError {
    if audit {
        DynamicTreeChainCandidateHeapError::TraceVerification
    } else {
        DynamicTreeChainCandidateHeapError::ArithmeticOverflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::{
        DynamicTreeChainCycleQueryEventKind, trace_dynamic_tree_chain_cycle_query,
    };

    use crate::algorithms::dynamic_tree_chain_query::tests::runtime_fixture;

    #[test]
    fn heap_selects_the_same_stable_best_candidate_and_rejects_pop_tampering() {
        let state = runtime_fixture();
        let query = trace_dynamic_tree_chain_cycle_query(&state, 2).expect("query");
        let trace = trace_dynamic_tree_chain_candidate_heap(&query).expect("heap");
        assert_eq!(trace.selected, query.result.best_candidate);
        assert_eq!(
            trace.entries.len(),
            query
                .events
                .iter()
                .filter(|event| matches!(
                    event.kind,
                    DynamicTreeChainCycleQueryEventKind::CandidateEvaluated { .. }
                ))
                .count()
        );
        check_dynamic_tree_chain_candidate_heap_trace(&query, &trace).expect("check");

        let mut forged = trace;
        forged.pop_order.reverse();
        assert_eq!(
            check_dynamic_tree_chain_candidate_heap_trace(&query, &forged),
            Err(DynamicTreeChainCandidateHeapError::TraceVerification)
        );
    }

    #[test]
    fn maintained_heap_reuses_unchanged_rows_and_applies_stable_differences() {
        let state = runtime_fixture();
        let query = trace_dynamic_tree_chain_cycle_query(&state, 2).expect("query");
        let first = trace_dynamic_tree_chain_candidate_heap_refresh(
            &DynamicTreeChainCandidateHeapState::default(),
            &query,
        )
        .expect("initial refresh");
        assert_eq!(first.selected, query.result.best_candidate);
        assert_eq!(
            usize::try_from(first.metrics.heap_pushes).expect("bounded count"),
            first.after.heap.len()
        );
        assert_eq!(first.metrics.heap_pops, 0);
        assert_eq!(first.metrics.heap_updates, 0);

        let unchanged = trace_dynamic_tree_chain_candidate_heap_refresh(&first.after, &query)
            .expect("unchanged refresh");
        assert!(unchanged.transitions.is_empty());
        assert_eq!(unchanged.after, first.after);
        assert_eq!(unchanged.metrics.heap_pushes, 0);
        assert_eq!(unchanged.metrics.heap_pops, 0);
        assert_eq!(unchanged.metrics.heap_updates, 0);

        let mut reduced_query = query.clone();
        let removed = reduced_query
            .events
            .iter()
            .position(|event| {
                matches!(
                    event.kind,
                    DynamicTreeChainCycleQueryEventKind::CandidateEvaluated { .. }
                )
            })
            .expect("candidate event");
        reduced_query.events.remove(removed);
        let reduced =
            trace_dynamic_tree_chain_candidate_heap_refresh(&unchanged.after, &reduced_query)
                .expect("difference refresh");
        assert_eq!(reduced.metrics.heap_pops, 1);
        assert_eq!(reduced.after.heap.len() + 1, unchanged.after.heap.len());
        check_dynamic_tree_chain_candidate_heap_refresh(&reduced_query, &unchanged.after, &reduced)
            .expect("refresh check");

        let mut forged = reduced;
        forged.metrics.heap_updates += 1;
        assert_eq!(
            check_dynamic_tree_chain_candidate_heap_refresh(
                &reduced_query,
                &unchanged.after,
                &forged,
            ),
            Err(DynamicTreeChainCandidateHeapError::TraceVerification)
        );
    }
}
