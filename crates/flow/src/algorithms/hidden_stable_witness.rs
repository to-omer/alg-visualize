//! Exact bounded verifier for Hidden Stable alpha-Flow Updates.
//!
//! This module verifies Definition 4.4 of van den Brand et al., "A
//! Deterministic Almost-Linear Time Algorithm for Minimum-Cost Flow"
//! (arXiv:2309.16629v1). It is an auditor for hidden witnesses, not the data
//! structure that consumes only observable graph updates. Stable edge IDs and
//! explicit insertion epochs make the factor-two width condition exact even
//! when endpoints change or an edge is deleted and later reinserted.
//!
//! The paper states quasipolynomial upper/lower scalar bounds asymptotically.
//! This bounded verifier instead receives an explicit positive exact scalar
//! band and certifies every length and width against it. It does not claim the
//! asymptotic data-structure theorem or reveal a witness to a decision rule.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use thiserror::Error;

/// Maximum vertices in one witness stage.
pub const HIDDEN_STABLE_WITNESS_MAX_NODES: usize = 8;
/// Stable edge-ID universe size.
pub const HIDDEN_STABLE_WITNESS_MAX_EDGES: usize = 12;
/// Maximum dynamic stages in one certificate.
pub const HIDDEN_STABLE_WITNESS_MAX_STAGES: usize = 256;
/// Maximum trace boundaries, including completion.
pub const HIDDEN_STABLE_WITNESS_MAX_TRACE_EVENTS: usize = 257;
/// Maximum numerator or denominator width of an exact scalar.
pub const HIDDEN_STABLE_WITNESS_MAX_RATIONAL_BITS: u64 = 512;

const CATALOG_ID: &str = "hidden-stable-witness";

/// Exact verifier parameters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HiddenStableWitnessConfig {
    /// Fixed vertex universe; endpoints may change within it.
    pub node_count: usize,
    /// Required objective magnitude from Definition 4.4.
    pub alpha: BigRational,
    /// Inclusive positive lower bound for every length and width.
    pub scalar_lower: BigRational,
    /// Inclusive upper bound for every length and width.
    pub scalar_upper: BigRational,
}

/// One current edge and its hidden witness coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HiddenStableEdgeWitness {
    /// Stable ID in `[0, HIDDEN_STABLE_WITNESS_MAX_EDGES)`.
    pub edge: usize,
    /// Current tail; may change because of a vertex split.
    pub from: usize,
    /// Current head; may change because of a vertex split.
    pub to: usize,
    /// Whether this stage starts a new explicit insertion epoch.
    pub explicitly_inserted: bool,
    /// Current exact positive length.
    pub length: BigRational,
    /// Current exact signed gradient.
    pub gradient: BigRational,
    /// Hidden circulation coordinate.
    pub circulation: BigRational,
    /// Hidden exact positive coordinate-wise upper bound.
    pub width: BigRational,
}

/// One complete current dynamic graph and hidden witness pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HiddenStableWitnessStage {
    /// Current edges in strictly increasing stable-ID order.
    pub edges: Vec<HiddenStableEdgeWitness>,
}

/// Exact work counters for witness verification.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HiddenStableWitnessMetrics {
    /// Verified stages.
    pub stages: u64,
    /// Current-edge witness rows inspected.
    pub edge_rows: u64,
    /// Cross-stage factor-two comparisons.
    pub stability_comparisons: u64,
    /// Explicit insertion epochs started, including stage zero.
    pub insertion_epochs: u64,
    /// Reversible public state transitions.
    pub state_transitions: u64,
}

/// Complete auditor state at a stage boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HiddenStableWitnessSnapshot {
    /// Last verified zero-based stage, if any.
    pub latest_stage: Option<usize>,
    /// Current stable edge IDs.
    pub active_edges: Vec<usize>,
    /// Insertion epoch start stage for each stable ID.
    pub insertion_epoch: Vec<Option<usize>>,
    /// Minimum width observed in each current insertion epoch.
    pub minimum_epoch_width: Vec<Option<BigRational>>,
    /// Latest exact gradient objective `<g,c>`.
    pub latest_objective: Option<BigRational>,
    /// Latest exact width norm `||w||_1`.
    pub latest_width_norm: Option<BigRational>,
    /// Latest exact objective ratio.
    pub latest_ratio: Option<BigRational>,
    /// Whether completion was emitted.
    pub complete: bool,
    /// Exact work counters.
    pub metrics: HiddenStableWitnessMetrics,
}

/// Exact public certificate for one accepted stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HiddenStableStageCertificate {
    /// Zero-based dynamic stage.
    pub stage: usize,
    /// Exact `<g,c>`.
    pub objective: BigRational,
    /// Exact `||w||_1`.
    pub width_norm: BigRational,
    /// Exact objective ratio.
    pub ratio: BigRational,
    /// Factor-two comparisons made at this stage.
    pub stability_comparisons: u64,
}

/// Source meaning of one verifier boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HiddenStableWitnessEventKind {
    /// One stage satisfied all five bounded verifier conditions.
    StageVerified {
        /// Exact bounded witness certificate.
        certificate: Box<HiddenStableStageCertificate>,
    },
    /// Every supplied stage was verified.
    Completed,
}

/// One fully reversible verification event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HiddenStableWitnessTraceEvent {
    /// Stable component identity.
    pub catalog_id: &'static str,
    /// Source-level event meaning.
    pub kind: HiddenStableWitnessEventKind,
    /// State before the event.
    pub before: HiddenStableWitnessSnapshot,
    /// State after the event.
    pub after: HiddenStableWitnessSnapshot,
}

/// Exact bounded witness certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HiddenStableWitnessResult {
    /// Terminal verifier state.
    pub final_snapshot: HiddenStableWitnessSnapshot,
}

/// Complete reversible verifier transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HiddenStableWitnessTraceResult {
    /// Empty epoch state.
    pub base_snapshot: HiddenStableWitnessSnapshot,
    /// One event per stage, then completion.
    pub events: Vec<HiddenStableWitnessTraceEvent>,
    /// Exact certificate result.
    pub result: HiddenStableWitnessResult,
}

/// Explicit bounded-verifier failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum HiddenStableWitnessError {
    /// Config or stage representation is malformed.
    #[error("hidden stable witness input is invalid")]
    InvalidInput,
    /// Exact data exceeds the published small-instance band.
    #[error("hidden stable witness exceeds its admission band")]
    AdmissionLimit,
    /// A Definition 4.4 witness condition does not hold.
    #[error("hidden stable witness condition failed")]
    WitnessViolation,
    /// Checked work accounting overflowed.
    #[error("hidden stable witness arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied transcript is not the exact stable replay.
    #[error("hidden stable witness trace verification failed")]
    TraceVerification,
}

struct InternalRun {
    base_snapshot: HiddenStableWitnessSnapshot,
    events: Vec<HiddenStableWitnessTraceEvent>,
    result: HiddenStableWitnessResult,
}

struct StageSummary {
    objective: BigRational,
    width_norm: BigRational,
    ratio: BigRational,
    stability_comparisons: u64,
}

/// Verifies every stage without recording events.
///
/// # Errors
///
/// Rejects malformed/admission-exceeding input or any failed witness clause.
pub fn verify_hidden_stable_witness(
    config: &HiddenStableWitnessConfig,
    stages: &[HiddenStableWitnessStage],
) -> Result<HiddenStableWitnessResult, HiddenStableWitnessError> {
    run_internal(config, stages, false).map(|run| run.result)
}

/// Records one exact boundary per verified stage and completion.
///
/// # Errors
///
/// Returns a verifier failure or independent replay-checker failure.
pub fn trace_hidden_stable_witness(
    config: &HiddenStableWitnessConfig,
    stages: &[HiddenStableWitnessStage],
) -> Result<HiddenStableWitnessTraceResult, HiddenStableWitnessError> {
    let run = run_internal(config, stages, true)?;
    let trace = HiddenStableWitnessTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_hidden_stable_witness_trace(config, stages, &trace)?;
    Ok(trace)
}

/// Independently reconstructs every bounded witness condition and event.
///
/// This checker does not invoke the production verifier or its stage helper.
///
/// # Errors
///
/// Rejects source-invalid input or any transcript drift.
pub fn check_hidden_stable_witness_trace(
    config: &HiddenStableWitnessConfig,
    stages: &[HiddenStableWitnessStage],
    trace: &HiddenStableWitnessTraceResult,
) -> Result<(), HiddenStableWitnessError> {
    audit_input(config, stages)?;
    let mut snapshot = audit_base_snapshot();
    if trace.base_snapshot != snapshot
        || trace.events.len()
            != stages
                .len()
                .checked_add(1)
                .ok_or(HiddenStableWitnessError::TraceVerification)?
    {
        return Err(HiddenStableWitnessError::TraceVerification);
    }
    for (stage_index, (stage, event)) in stages.iter().zip(&trace.events).enumerate() {
        if event.catalog_id != CATALOG_ID || event.before != snapshot {
            return Err(HiddenStableWitnessError::TraceVerification);
        }
        let mut after = snapshot.clone();
        let summary = audit_stage(config, stage_index, stage, &mut after)?;
        after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
        let kind = HiddenStableWitnessEventKind::StageVerified {
            certificate: Box::new(HiddenStableStageCertificate {
                stage: stage_index,
                objective: summary.objective,
                width_norm: summary.width_norm,
                ratio: summary.ratio,
                stability_comparisons: summary.stability_comparisons,
            }),
        };
        if event.kind != kind || event.after != after {
            return Err(HiddenStableWitnessError::TraceVerification);
        }
        snapshot = after;
    }
    let completion = trace
        .events
        .last()
        .ok_or(HiddenStableWitnessError::TraceVerification)?;
    let mut final_snapshot = snapshot.clone();
    final_snapshot.complete = true;
    final_snapshot.metrics.state_transitions =
        audit_increment(final_snapshot.metrics.state_transitions)?;
    if completion.catalog_id != CATALOG_ID
        || completion.before != snapshot
        || completion.kind != HiddenStableWitnessEventKind::Completed
        || completion.after != final_snapshot
        || trace.result.final_snapshot != final_snapshot
    {
        return Err(HiddenStableWitnessError::TraceVerification);
    }
    Ok(())
}

fn run_internal(
    config: &HiddenStableWitnessConfig,
    stages: &[HiddenStableWitnessStage],
    record: bool,
) -> Result<InternalRun, HiddenStableWitnessError> {
    validate_input(config, stages)?;
    let mut snapshot = base_snapshot();
    let base_snapshot = snapshot.clone();
    let capacity = stages
        .len()
        .checked_add(1)
        .ok_or(HiddenStableWitnessError::ArithmeticOverflow)?;
    let mut events = Vec::with_capacity(if record { capacity } else { 0 });
    for (stage_index, stage) in stages.iter().enumerate() {
        let before = snapshot.clone();
        let summary = verify_stage(config, stage_index, stage, &mut snapshot)?;
        snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
        if record {
            events.push(HiddenStableWitnessTraceEvent {
                catalog_id: CATALOG_ID,
                kind: HiddenStableWitnessEventKind::StageVerified {
                    certificate: Box::new(HiddenStableStageCertificate {
                        stage: stage_index,
                        objective: summary.objective,
                        width_norm: summary.width_norm,
                        ratio: summary.ratio,
                        stability_comparisons: summary.stability_comparisons,
                    }),
                },
                before,
                after: snapshot.clone(),
            });
        }
    }
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        events.push(HiddenStableWitnessTraceEvent {
            catalog_id: CATALOG_ID,
            kind: HiddenStableWitnessEventKind::Completed,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(InternalRun {
        base_snapshot,
        events,
        result: HiddenStableWitnessResult {
            final_snapshot: snapshot,
        },
    })
}

fn base_snapshot() -> HiddenStableWitnessSnapshot {
    HiddenStableWitnessSnapshot {
        latest_stage: None,
        active_edges: Vec::new(),
        insertion_epoch: vec![None; HIDDEN_STABLE_WITNESS_MAX_EDGES],
        minimum_epoch_width: vec![None; HIDDEN_STABLE_WITNESS_MAX_EDGES],
        latest_objective: None,
        latest_width_norm: None,
        latest_ratio: None,
        complete: false,
        metrics: HiddenStableWitnessMetrics::default(),
    }
}

fn verify_stage(
    config: &HiddenStableWitnessConfig,
    stage_index: usize,
    stage: &HiddenStableWitnessStage,
    snapshot: &mut HiddenStableWitnessSnapshot,
) -> Result<StageSummary, HiddenStableWitnessError> {
    let previous_active = snapshot.active_edges.clone();
    let current_active = stage.edges.iter().map(|edge| edge.edge).collect::<Vec<_>>();
    let mut divergence = vec![BigRational::zero(); config.node_count];
    let mut objective = BigRational::zero();
    let mut width_norm = BigRational::zero();
    let mut comparisons = 0_u64;
    for edge in &stage.edges {
        let was_active = previous_active.binary_search(&edge.edge).is_ok();
        if stage_index == 0 {
            if !edge.explicitly_inserted {
                return Err(HiddenStableWitnessError::WitnessViolation);
            }
        } else if !was_active && !edge.explicitly_inserted {
            return Err(HiddenStableWitnessError::WitnessViolation);
        }
        if edge.explicitly_inserted {
            snapshot.insertion_epoch[edge.edge] = Some(stage_index);
            snapshot.minimum_epoch_width[edge.edge] = Some(edge.width.clone());
            snapshot.metrics.insertion_epochs = increment(snapshot.metrics.insertion_epochs)?;
        } else {
            let minimum = snapshot.minimum_epoch_width[edge.edge]
                .as_ref()
                .ok_or(HiddenStableWitnessError::WitnessViolation)?;
            comparisons = increment(comparisons)?;
            if edge.width > minimum * BigInt::from(2_u8) {
                return Err(HiddenStableWitnessError::WitnessViolation);
            }
            if edge.width < *minimum {
                snapshot.minimum_epoch_width[edge.edge] = Some(edge.width.clone());
            }
        }
        if (&edge.length * &edge.circulation).abs() > edge.width {
            return Err(HiddenStableWitnessError::WitnessViolation);
        }
        divergence[edge.from] += &edge.circulation;
        divergence[edge.to] -= &edge.circulation;
        objective += &edge.gradient * &edge.circulation;
        width_norm += &edge.width;
        snapshot.metrics.edge_rows = increment(snapshot.metrics.edge_rows)?;
    }
    for &edge in &previous_active {
        if current_active.binary_search(&edge).is_err() {
            snapshot.insertion_epoch[edge] = None;
            snapshot.minimum_epoch_width[edge] = None;
        }
    }
    if divergence.iter().any(|value| !value.is_zero()) || width_norm.is_zero() {
        return Err(HiddenStableWitnessError::WitnessViolation);
    }
    let ratio = &objective / &width_norm;
    if ratio > -&config.alpha {
        return Err(HiddenStableWitnessError::WitnessViolation);
    }
    snapshot.latest_stage = Some(stage_index);
    snapshot.active_edges = current_active;
    snapshot.latest_objective = Some(objective.clone());
    snapshot.latest_width_norm = Some(width_norm.clone());
    snapshot.latest_ratio = Some(ratio.clone());
    snapshot.metrics.stages = increment(snapshot.metrics.stages)?;
    snapshot.metrics.stability_comparisons = snapshot
        .metrics
        .stability_comparisons
        .checked_add(comparisons)
        .ok_or(HiddenStableWitnessError::ArithmeticOverflow)?;
    Ok(StageSummary {
        objective,
        width_norm,
        ratio,
        stability_comparisons: comparisons,
    })
}

fn validate_input(
    config: &HiddenStableWitnessConfig,
    stages: &[HiddenStableWitnessStage],
) -> Result<(), HiddenStableWitnessError> {
    if config.node_count == 0
        || config.alpha <= BigRational::zero()
        || config.scalar_lower <= BigRational::zero()
        || config.scalar_upper < config.scalar_lower
        || stages.is_empty()
    {
        return Err(HiddenStableWitnessError::InvalidInput);
    }
    if config.node_count > HIDDEN_STABLE_WITNESS_MAX_NODES
        || stages.len() > HIDDEN_STABLE_WITNESS_MAX_STAGES
        || scalar_too_wide(&config.alpha)
        || scalar_too_wide(&config.scalar_lower)
        || scalar_too_wide(&config.scalar_upper)
    {
        return Err(HiddenStableWitnessError::AdmissionLimit);
    }
    for stage in stages {
        if stage.edges.is_empty() || stage.edges.len() > HIDDEN_STABLE_WITNESS_MAX_EDGES {
            return Err(HiddenStableWitnessError::InvalidInput);
        }
        let mut previous = None;
        for edge in &stage.edges {
            if edge.edge >= HIDDEN_STABLE_WITNESS_MAX_EDGES
                || previous.is_some_and(|value| value >= edge.edge)
                || edge.from >= config.node_count
                || edge.to >= config.node_count
                || edge.length < config.scalar_lower
                || edge.length > config.scalar_upper
                || edge.width < config.scalar_lower
                || edge.width > config.scalar_upper
            {
                return Err(HiddenStableWitnessError::InvalidInput);
            }
            if scalar_too_wide(&edge.length)
                || scalar_too_wide(&edge.gradient)
                || scalar_too_wide(&edge.circulation)
                || scalar_too_wide(&edge.width)
            {
                return Err(HiddenStableWitnessError::AdmissionLimit);
            }
            previous = Some(edge.edge);
        }
    }
    Ok(())
}

fn scalar_too_wide(value: &BigRational) -> bool {
    value.numer().bits() > HIDDEN_STABLE_WITNESS_MAX_RATIONAL_BITS
        || value.denom().bits() > HIDDEN_STABLE_WITNESS_MAX_RATIONAL_BITS
}

fn increment(value: u64) -> Result<u64, HiddenStableWitnessError> {
    value
        .checked_add(1)
        .ok_or(HiddenStableWitnessError::ArithmeticOverflow)
}

// The audit path duplicates validation, circulation, epoch, and score logic.
fn audit_input(
    config: &HiddenStableWitnessConfig,
    stages: &[HiddenStableWitnessStage],
) -> Result<(), HiddenStableWitnessError> {
    if config.node_count == 0
        || config.node_count > HIDDEN_STABLE_WITNESS_MAX_NODES
        || config.alpha <= BigRational::zero()
        || config.scalar_lower <= BigRational::zero()
        || config.scalar_upper < config.scalar_lower
        || stages.is_empty()
        || stages.len() > HIDDEN_STABLE_WITNESS_MAX_STAGES
        || audit_wide(&config.alpha)
        || audit_wide(&config.scalar_lower)
        || audit_wide(&config.scalar_upper)
    {
        return Err(HiddenStableWitnessError::TraceVerification);
    }
    for stage in stages {
        if stage.edges.is_empty() || stage.edges.len() > HIDDEN_STABLE_WITNESS_MAX_EDGES {
            return Err(HiddenStableWitnessError::TraceVerification);
        }
        for (position, edge) in stage.edges.iter().enumerate() {
            if edge.edge >= HIDDEN_STABLE_WITNESS_MAX_EDGES
                || position > 0 && stage.edges[position - 1].edge >= edge.edge
                || edge.from >= config.node_count
                || edge.to >= config.node_count
                || edge.length < config.scalar_lower
                || edge.length > config.scalar_upper
                || edge.width < config.scalar_lower
                || edge.width > config.scalar_upper
                || audit_wide(&edge.length)
                || audit_wide(&edge.gradient)
                || audit_wide(&edge.circulation)
                || audit_wide(&edge.width)
            {
                return Err(HiddenStableWitnessError::TraceVerification);
            }
        }
    }
    Ok(())
}

fn audit_base_snapshot() -> HiddenStableWitnessSnapshot {
    HiddenStableWitnessSnapshot {
        latest_stage: None,
        active_edges: Vec::new(),
        insertion_epoch: vec![None; HIDDEN_STABLE_WITNESS_MAX_EDGES],
        minimum_epoch_width: vec![None; HIDDEN_STABLE_WITNESS_MAX_EDGES],
        latest_objective: None,
        latest_width_norm: None,
        latest_ratio: None,
        complete: false,
        metrics: HiddenStableWitnessMetrics::default(),
    }
}

fn audit_stage(
    config: &HiddenStableWitnessConfig,
    stage_index: usize,
    stage: &HiddenStableWitnessStage,
    snapshot: &mut HiddenStableWitnessSnapshot,
) -> Result<StageSummary, HiddenStableWitnessError> {
    let old_active = snapshot.active_edges.clone();
    let new_active = stage.edges.iter().map(|row| row.edge).collect::<Vec<_>>();
    let mut balance = vec![BigRational::zero(); config.node_count];
    let mut dot = BigRational::zero();
    let mut norm = BigRational::zero();
    let mut comparisons = 0_u64;
    for row in &stage.edges {
        let continuing = old_active.binary_search(&row.edge).is_ok();
        if (stage_index == 0 || !continuing) && !row.explicitly_inserted {
            return Err(HiddenStableWitnessError::TraceVerification);
        }
        if row.explicitly_inserted {
            snapshot.insertion_epoch[row.edge] = Some(stage_index);
            snapshot.minimum_epoch_width[row.edge] = Some(row.width.clone());
            snapshot.metrics.insertion_epochs = audit_increment(snapshot.metrics.insertion_epochs)?;
        } else {
            let prior_minimum = snapshot.minimum_epoch_width[row.edge]
                .as_ref()
                .ok_or(HiddenStableWitnessError::TraceVerification)?;
            comparisons = audit_increment(comparisons)?;
            if row.width > prior_minimum * BigInt::from(2_u8) {
                return Err(HiddenStableWitnessError::TraceVerification);
            }
            if row.width < *prior_minimum {
                snapshot.minimum_epoch_width[row.edge] = Some(row.width.clone());
            }
        }
        if (&row.length * &row.circulation).abs() > row.width {
            return Err(HiddenStableWitnessError::TraceVerification);
        }
        balance[row.from] += &row.circulation;
        balance[row.to] -= &row.circulation;
        dot += &row.gradient * &row.circulation;
        norm += &row.width;
        snapshot.metrics.edge_rows = audit_increment(snapshot.metrics.edge_rows)?;
    }
    for &edge in &old_active {
        if new_active.binary_search(&edge).is_err() {
            snapshot.insertion_epoch[edge] = None;
            snapshot.minimum_epoch_width[edge] = None;
        }
    }
    if balance.iter().any(|value| !value.is_zero()) || norm.is_zero() {
        return Err(HiddenStableWitnessError::TraceVerification);
    }
    let ratio = &dot / &norm;
    if ratio > -&config.alpha {
        return Err(HiddenStableWitnessError::TraceVerification);
    }
    snapshot.latest_stage = Some(stage_index);
    snapshot.active_edges = new_active;
    snapshot.latest_objective = Some(dot.clone());
    snapshot.latest_width_norm = Some(norm.clone());
    snapshot.latest_ratio = Some(ratio.clone());
    snapshot.metrics.stages = audit_increment(snapshot.metrics.stages)?;
    snapshot.metrics.stability_comparisons = snapshot
        .metrics
        .stability_comparisons
        .checked_add(comparisons)
        .ok_or(HiddenStableWitnessError::TraceVerification)?;
    Ok(StageSummary {
        objective: dot,
        width_norm: norm,
        ratio,
        stability_comparisons: comparisons,
    })
}

fn audit_increment(value: u64) -> Result<u64, HiddenStableWitnessError> {
    value
        .checked_add(1)
        .ok_or(HiddenStableWitnessError::TraceVerification)
}

fn audit_wide(value: &BigRational) -> bool {
    value.numer().bits() > HIDDEN_STABLE_WITNESS_MAX_RATIONAL_BITS
        || value.denom().bits() > HIDDEN_STABLE_WITNESS_MAX_RATIONAL_BITS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn config() -> HiddenStableWitnessConfig {
        HiddenStableWitnessConfig {
            node_count: 3,
            alpha: BigRational::new(1.into(), 2.into()),
            scalar_lower: BigRational::new(1.into(), 8.into()),
            scalar_upper: rational(8),
        }
    }

    fn stage(widths: [i64; 3], inserted: bool) -> HiddenStableWitnessStage {
        let endpoints = [(0, 1), (1, 2), (2, 0)];
        HiddenStableWitnessStage {
            edges: endpoints
                .into_iter()
                .enumerate()
                .map(|(edge, (from, to))| HiddenStableEdgeWitness {
                    edge,
                    from,
                    to,
                    explicitly_inserted: inserted,
                    length: rational(1),
                    gradient: if edge == 0 { rational(-2) } else { rational(0) },
                    circulation: rational(1),
                    width: rational(widths[edge]),
                })
                .collect(),
        }
    }

    #[test]
    fn verifies_circulation_validity_ratio_band_and_factor_two_history() {
        let stages = vec![stage([1, 1, 1], true), stage([2, 1, 1], false)];
        let result = verify_hidden_stable_witness(&config(), &stages).expect("witness");
        assert_eq!(
            result.final_snapshot.latest_ratio,
            Some(BigRational::new((-1).into(), 2.into()))
        );
        assert_eq!(result.final_snapshot.metrics.stability_comparisons, 3);
        assert_eq!(result.final_snapshot.metrics.insertion_epochs, 3);
    }

    #[test]
    fn explicit_reinsertion_resets_width_history() {
        let mut reinserted = stage([4, 1, 1], true);
        reinserted.edges[0].gradient = rational(-4);
        let stages = vec![stage([1, 1, 1], true), reinserted];
        let result = verify_hidden_stable_witness(&config(), &stages).expect("reinsert");
        assert_eq!(result.final_snapshot.insertion_epoch[0], Some(1));
        assert_eq!(
            result.final_snapshot.minimum_epoch_width[0],
            Some(rational(4))
        );
    }

    #[test]
    fn rejects_width_growth_without_insertion_and_invalid_valid_pair() {
        let unstable = vec![stage([1, 1, 1], true), stage([3, 1, 1], false)];
        assert_eq!(
            verify_hidden_stable_witness(&config(), &unstable),
            Err(HiddenStableWitnessError::WitnessViolation)
        );
        let mut invalid = stage([1, 1, 1], true);
        invalid.edges[0].circulation = rational(2);
        assert_eq!(
            verify_hidden_stable_witness(&config(), &[invalid]),
            Err(HiddenStableWitnessError::WitnessViolation)
        );
    }

    #[test]
    fn deletion_requires_explicit_insertion_before_id_reappears() {
        let first = stage([1, 1, 1], true);
        let mut deleted = stage([1, 1, 1], false);
        deleted.edges.pop();
        deleted.edges[0].circulation = rational(0);
        deleted.edges[1].circulation = rational(0);
        deleted.edges[0].gradient = rational(-1);
        deleted.edges[0].from = 0;
        deleted.edges[0].to = 0;
        deleted.edges[0].circulation = rational(1);
        let mut reappeared = stage([1, 1, 1], false);
        reappeared.edges[0].from = 0;
        reappeared.edges[0].to = 0;
        reappeared.edges[0].circulation = rational(1);
        reappeared.edges[1].circulation = rational(0);
        reappeared.edges[2].circulation = rational(0);
        assert_eq!(
            verify_hidden_stable_witness(&config(), &[first, deleted, reappeared]),
            Err(HiddenStableWitnessError::WitnessViolation)
        );
    }

    #[test]
    fn fast_trace_and_independent_checker_match_and_reject_tampering() {
        let stages = vec![stage([1, 1, 1], true), stage([2, 1, 1], false)];
        let fast = verify_hidden_stable_witness(&config(), &stages).expect("fast");
        let mut trace = trace_hidden_stable_witness(&config(), &stages).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(trace.events.len(), 3);
        let HiddenStableWitnessEventKind::StageVerified { certificate } = &mut trace.events[1].kind
        else {
            panic!("stage");
        };
        certificate.ratio = rational(-1);
        assert_eq!(
            check_hidden_stable_witness_trace(&config(), &stages, &trace),
            Err(HiddenStableWitnessError::TraceVerification)
        );
    }

    #[test]
    fn rejects_out_of_band_and_unsorted_stage_rows() {
        let mut out_of_band = stage([1, 1, 1], true);
        out_of_band.edges[0].width = rational(9);
        assert_eq!(
            verify_hidden_stable_witness(&config(), &[out_of_band]),
            Err(HiddenStableWitnessError::InvalidInput)
        );
        let mut unsorted = stage([1, 1, 1], true);
        unsorted.edges.swap(0, 1);
        assert_eq!(
            verify_hidden_stable_witness(&config(), &[unsorted]),
            Err(HiddenStableWitnessError::InvalidInput)
        );
    }
}
