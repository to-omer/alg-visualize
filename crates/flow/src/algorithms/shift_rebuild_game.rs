//! Exact player strategy for the deterministic shift-and-rebuild game.
//!
//! This module implements Algorithm 3 of van den Brand et al., "A
//! Deterministic Almost-Linear Time Algorithm for Minimum-Cost Flow"
//! (arXiv:2309.16629v1). It is deliberately a strategy primitive: the caller
//! supplies the adversary's rebuild and round-continuation transcript. The
//! player never reads the hidden weights when choosing a shift. Exact weights
//! are retained only to replay the representative-round update from Definition
//! 7.2 when a branch index wraps.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::One;
use thiserror::Error;

/// Maximum source depth `d`; the game has `d + 1` levels.
pub const SHIFT_REBUILD_GAME_MAX_DEPTH: usize = 16;
/// Maximum branches maintained per level.
pub const SHIFT_REBUILD_GAME_MAX_BRANCHES: usize = 64;
/// Maximum hidden-weight exponent range.
pub const SHIFT_REBUILD_GAME_MAX_PSI: u64 = 64;
/// Maximum rounds in one deterministic transcript.
pub const SHIFT_REBUILD_GAME_MAX_ROUNDS: usize = 4_096;
/// Maximum player shifts in one transcript.
pub const SHIFT_REBUILD_GAME_MAX_SHIFTS: u64 = 100_000;
/// Maximum reversible public boundaries.
pub const SHIFT_REBUILD_GAME_MAX_TRACE_EVENTS: usize = 250_000;

const CATALOG_ID: &str = "deterministic-shift-rebuild-game";

/// Fixed game parameters from Definition 7.2 and Algorithm 3.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShiftRebuildGameConfig {
    /// Source depth `d`; levels are indexed `0..=d`.
    pub depth: usize,
    /// Branching factor `k`.
    pub branches: usize,
    /// Hidden weight range exponent `Ψ`; pass ceiling is `2Ψ`.
    pub psi: u64,
}

/// One adversary-controlled round transcript visible to the player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftRebuildRound {
    /// Hidden positive weight `W(t)`, retained for representative-round replay.
    pub weight: BigRational,
    /// Optional adversary `rebuild-step(i)` in Game Stage 2.
    pub rebuild_level: Option<usize>,
    /// Number of `round-continuing-step` choices before the completing choice.
    pub continuations: u64,
}

/// Lifecycle of the exact Algorithm 3 transcript.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShiftRebuildGameStage {
    /// Valid configuration before round 1.
    Ready,
    /// Game Stage 1 installed `W(t)`.
    RoundStarted,
    /// Game Stage 2 completed and Stage 3 may decide the round.
    AwaitingRoundDecision,
    /// The adversary requested another shift in Game Stage 3.
    ContinuationRequested,
    /// Game Stage 4 performed the source-defined shift.
    Shifted,
    /// Game Stage 5 completed the current round.
    RoundCompleted,
    /// Every supplied round completed.
    Complete,
}

/// Player-visible and replay-relevant state of one level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShiftRebuildLevelState {
    /// Zero-based source level.
    pub level: usize,
    /// Current branch `shift_i` in `0..k`.
    pub shift: usize,
    /// Completed full branch passes at this level.
    pub passes: u64,
    /// Hidden representative round `repT_i`, exposed only for replay.
    pub representative_round: usize,
    /// Player `shift-step(i)` count.
    pub shift_steps: u64,
    /// Adversary `rebuild-step(i)` count at exactly this level.
    pub rebuild_steps: u64,
}

/// Exact operation counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShiftRebuildGameMetrics {
    /// Completed rounds.
    pub completed_rounds: u64,
    /// Stage-2 adversary steps, including do-nothing steps.
    pub adversary_steps: u64,
    /// Adversary rebuild steps.
    pub rebuild_steps: u64,
    /// Round-continuing decisions.
    pub continuation_decisions: u64,
    /// Player shift steps.
    pub shift_steps: u64,
    /// Shift steps that wrapped a branch index to zero.
    pub branch_wraps: u64,
    /// Public state transitions.
    pub state_transitions: u64,
}

/// Complete state at one reversible Algorithm 3 boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftRebuildGameSnapshot {
    /// Current lifecycle stage.
    pub stage: ShiftRebuildGameStage,
    /// One-based current round, or zero before round 1.
    pub current_round: usize,
    /// Rounds already completed.
    pub completed_rounds: usize,
    /// Definition 7.2 step counter `s`.
    pub step_counter: u64,
    /// Exact hidden weights for rounds already begun.
    pub weight_history: Vec<BigRational>,
    /// Level states in ascending level order.
    pub levels: Vec<ShiftRebuildLevelState>,
    /// Exact counters.
    pub metrics: ShiftRebuildGameMetrics,
}

/// Source meaning of one reversible boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShiftRebuildGameEventKind {
    /// Game Stage 1 begins round `t`.
    BeginRound {
        /// One-based source round.
        round: usize,
        /// Hidden weight, shown by the visualizer but never read by the player.
        weight: BigRational,
    },
    /// Game Stage 2 selected `rebuild-step(i)`.
    AdversaryRebuild {
        /// Rebuilt level; this resets its suffix.
        level: usize,
    },
    /// Game Stage 2 selected `do-nothing-step`.
    AdversaryDoNothing,
    /// Game Stage 3 selected `round-continuing-step`.
    RoundContinues,
    /// Game Stage 4 selected the largest eligible level and shifted it.
    PlayerShift {
        /// Shifted level.
        level: usize,
        /// Whether `shift_i` wrapped to zero and incremented `passes_i`.
        wrapped: bool,
    },
    /// Game Stage 3/5 completed the current round.
    RoundCompletes,
    /// Every supplied round has completed.
    Completed,
}

/// One fully reversible source boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftRebuildGameTraceEvent {
    /// Stable component identity.
    pub catalog_id: &'static str,
    /// Source transition meaning.
    pub kind: ShiftRebuildGameEventKind,
    /// State before the transition.
    pub before: ShiftRebuildGameSnapshot,
    /// State after the transition.
    pub after: ShiftRebuildGameSnapshot,
}

/// Deterministic Algorithm 3 output counters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftRebuildGameResult {
    /// Player shift counts `(s_0, ..., s_d)`.
    pub shifts: Vec<u64>,
    /// Adversary rebuild counts `(r_0, ..., r_d)`.
    pub rebuilds: Vec<u64>,
    /// Final replay state.
    pub final_snapshot: ShiftRebuildGameSnapshot,
}

/// Complete exact transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftRebuildGameTraceResult {
    /// State before round 1.
    pub base_snapshot: ShiftRebuildGameSnapshot,
    /// Atomic game boundaries.
    pub events: Vec<ShiftRebuildGameTraceEvent>,
    /// Final source counters.
    pub result: ShiftRebuildGameResult,
}

/// Shift-and-rebuild strategy failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ShiftRebuildGameError {
    /// Configuration or transcript exceeds its published small-instance band.
    #[error("shift-and-rebuild game exceeds its admission band")]
    AdmissionLimit,
    /// Depth, branching, Ψ, a rebuild level, or a hidden weight is invalid.
    #[error("shift-and-rebuild game input is invalid")]
    InvalidInput,
    /// The adversary requested continuation after all levels reached `2Ψ` passes.
    #[error("shift-and-rebuild player strategy has no eligible level")]
    StrategyExhausted,
    /// Checked counters overflowed.
    #[error("shift-and-rebuild game arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied trace violates Algorithm 3 transition semantics.
    #[error("shift-and-rebuild game trace verification failed")]
    TraceVerification,
}

struct InternalRun {
    base_snapshot: ShiftRebuildGameSnapshot,
    events: Vec<ShiftRebuildGameTraceEvent>,
    result: ShiftRebuildGameResult,
}

/// Executes Algorithm 3 without retaining trace events.
///
/// # Errors
///
/// Rejects invalid/out-of-band transcripts, counter overflow, or an adversary
/// continuation after every level has exhausted its source pass ceiling.
pub fn play_shift_rebuild_game(
    config: ShiftRebuildGameConfig,
    rounds: &[ShiftRebuildRound],
) -> Result<ShiftRebuildGameResult, ShiftRebuildGameError> {
    run_internal(config, rounds, false).map(|run| run.result)
}

/// Executes and records every source boundary of Algorithm 3.
///
/// # Errors
///
/// Returns any execution or independent checker failure.
pub fn trace_shift_rebuild_game(
    config: ShiftRebuildGameConfig,
    rounds: &[ShiftRebuildRound],
) -> Result<ShiftRebuildGameTraceResult, ShiftRebuildGameError> {
    let run = run_internal(config, rounds, true)?;
    let trace = ShiftRebuildGameTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_shift_rebuild_game_trace(config, rounds, &trace)?;
    Ok(trace)
}

/// Checks an Algorithm 3 transcript without invoking the production runner.
///
/// # Errors
///
/// Rejects lifecycle, selection, wrap, reset, representative-round, metric,
/// replay, or final-counter drift.
pub fn check_shift_rebuild_game_trace(
    config: ShiftRebuildGameConfig,
    rounds: &[ShiftRebuildRound],
    trace: &ShiftRebuildGameTraceResult,
) -> Result<(), ShiftRebuildGameError> {
    validate_input(config, rounds)?;
    let expected_base = initial_snapshot(config);
    if trace.base_snapshot != expected_base {
        return Err(ShiftRebuildGameError::TraceVerification);
    }
    validate_snapshot(config, &trace.base_snapshot)?;
    let mut cursor = &trace.base_snapshot;
    let mut round_index = 0_usize;
    let mut continuations_seen = 0_u64;
    for event in &trace.events {
        if event.catalog_id != CATALOG_ID || &event.before != cursor {
            return Err(ShiftRebuildGameError::TraceVerification);
        }
        validate_transition(
            config,
            rounds,
            &mut round_index,
            &mut continuations_seen,
            event,
        )?;
        validate_snapshot(config, &event.after)?;
        cursor = &event.after;
    }
    if round_index != rounds.len()
        || continuations_seen != 0
        || cursor != &trace.result.final_snapshot
        || cursor.stage != ShiftRebuildGameStage::Complete
        || trace.result.shifts
            != cursor
                .levels
                .iter()
                .map(|level| level.shift_steps)
                .collect::<Vec<_>>()
        || trace.result.rebuilds
            != cursor
                .levels
                .iter()
                .map(|level| level.rebuild_steps)
                .collect::<Vec<_>>()
    {
        return Err(ShiftRebuildGameError::TraceVerification);
    }
    Ok(())
}

fn run_internal(
    config: ShiftRebuildGameConfig,
    rounds: &[ShiftRebuildRound],
    record: bool,
) -> Result<InternalRun, ShiftRebuildGameError> {
    validate_input(config, rounds)?;
    let mut snapshot = initial_snapshot(config);
    let base_snapshot = snapshot.clone();
    let mut events = Vec::new();
    for (index, round) in rounds.iter().enumerate() {
        let round_number = index
            .checked_add(1)
            .ok_or(ShiftRebuildGameError::ArithmeticOverflow)?;
        run_round(
            config,
            round_number,
            round,
            &mut snapshot,
            &mut events,
            record,
        )?;
    }
    transition(
        &mut snapshot,
        &mut events,
        record,
        ShiftRebuildGameEventKind::Completed,
        |state| {
            state.stage = ShiftRebuildGameStage::Complete;
            Ok(())
        },
    )?;
    let result = ShiftRebuildGameResult {
        shifts: snapshot
            .levels
            .iter()
            .map(|level| level.shift_steps)
            .collect(),
        rebuilds: snapshot
            .levels
            .iter()
            .map(|level| level.rebuild_steps)
            .collect(),
        final_snapshot: snapshot,
    };
    Ok(InternalRun {
        base_snapshot,
        events,
        result,
    })
}

fn run_round(
    config: ShiftRebuildGameConfig,
    round_number: usize,
    round: &ShiftRebuildRound,
    snapshot: &mut ShiftRebuildGameSnapshot,
    events: &mut Vec<ShiftRebuildGameTraceEvent>,
    record: bool,
) -> Result<(), ShiftRebuildGameError> {
    transition(
        snapshot,
        events,
        record,
        ShiftRebuildGameEventKind::BeginRound {
            round: round_number,
            weight: round.weight.clone(),
        },
        |state| {
            state.stage = ShiftRebuildGameStage::RoundStarted;
            state.current_round = round_number;
            state.weight_history.push(round.weight.clone());
            Ok(())
        },
    )?;
    run_adversary_step(round.rebuild_level, snapshot, events, record)?;
    for _ in 0..round.continuations {
        transition(
            snapshot,
            events,
            record,
            ShiftRebuildGameEventKind::RoundContinues,
            |state| {
                state.stage = ShiftRebuildGameStage::ContinuationRequested;
                state.metrics.continuation_decisions =
                    checked_increment(state.metrics.continuation_decisions)?;
                Ok(())
            },
        )?;
        let level = largest_eligible_level(snapshot, config)
            .ok_or(ShiftRebuildGameError::StrategyExhausted)?;
        let wrapped = snapshot.levels[level]
            .shift
            .checked_add(1)
            .is_some_and(|shift| shift == config.branches);
        transition(
            snapshot,
            events,
            record,
            ShiftRebuildGameEventKind::PlayerShift { level, wrapped },
            |state| apply_player_shift(state, config, level),
        )?;
    }
    transition(
        snapshot,
        events,
        record,
        ShiftRebuildGameEventKind::RoundCompletes,
        |state| {
            state.stage = ShiftRebuildGameStage::RoundCompleted;
            state.completed_rounds = state
                .completed_rounds
                .checked_add(1)
                .ok_or(ShiftRebuildGameError::ArithmeticOverflow)?;
            state.metrics.completed_rounds = checked_increment(state.metrics.completed_rounds)?;
            Ok(())
        },
    )
}

fn run_adversary_step(
    rebuild_level: Option<usize>,
    snapshot: &mut ShiftRebuildGameSnapshot,
    events: &mut Vec<ShiftRebuildGameTraceEvent>,
    record: bool,
) -> Result<(), ShiftRebuildGameError> {
    if let Some(level) = rebuild_level {
        transition(
            snapshot,
            events,
            record,
            ShiftRebuildGameEventKind::AdversaryRebuild { level },
            |state| apply_rebuild(state, level),
        )
    } else {
        transition(
            snapshot,
            events,
            record,
            ShiftRebuildGameEventKind::AdversaryDoNothing,
            apply_adversary_do_nothing,
        )
    }
}

fn validate_input(
    config: ShiftRebuildGameConfig,
    rounds: &[ShiftRebuildRound],
) -> Result<(), ShiftRebuildGameError> {
    if config.depth == 0 || config.branches == 0 || config.psi == 0 {
        return Err(ShiftRebuildGameError::InvalidInput);
    }
    if config.depth > SHIFT_REBUILD_GAME_MAX_DEPTH
        || config.branches > SHIFT_REBUILD_GAME_MAX_BRANCHES
        || config.psi > SHIFT_REBUILD_GAME_MAX_PSI
        || rounds.is_empty()
        || rounds.len() > SHIFT_REBUILD_GAME_MAX_ROUNDS
    {
        return Err(ShiftRebuildGameError::AdmissionLimit);
    }
    let exponent =
        usize::try_from(config.psi).map_err(|_| ShiftRebuildGameError::ArithmeticOverflow)?;
    let lower = BigRational::new(BigInt::one(), BigInt::one() << exponent);
    let upper = BigRational::from_integer(BigInt::one() << exponent);
    let total_continuations = rounds.iter().try_fold(0_u64, |sum, round| {
        if round
            .rebuild_level
            .is_some_and(|level| level > config.depth)
            || round.weight <= lower
            || round.weight >= upper
        {
            return Err(ShiftRebuildGameError::InvalidInput);
        }
        sum.checked_add(round.continuations)
            .ok_or(ShiftRebuildGameError::ArithmeticOverflow)
    })?;
    if total_continuations > SHIFT_REBUILD_GAME_MAX_SHIFTS {
        return Err(ShiftRebuildGameError::AdmissionLimit);
    }
    let estimated_events = rounds
        .len()
        .checked_mul(3)
        .and_then(|base| {
            usize::try_from(total_continuations)
                .ok()
                .and_then(|continuations| continuations.checked_mul(2))
                .and_then(|continuations| base.checked_add(continuations))
        })
        .and_then(|events| events.checked_add(1))
        .ok_or(ShiftRebuildGameError::AdmissionLimit)?;
    if estimated_events > SHIFT_REBUILD_GAME_MAX_TRACE_EVENTS {
        return Err(ShiftRebuildGameError::AdmissionLimit);
    }
    Ok(())
}

fn initial_snapshot(config: ShiftRebuildGameConfig) -> ShiftRebuildGameSnapshot {
    ShiftRebuildGameSnapshot {
        stage: ShiftRebuildGameStage::Ready,
        current_round: 0,
        completed_rounds: 0,
        step_counter: 0,
        weight_history: Vec::new(),
        levels: (0..=config.depth)
            .map(|level| ShiftRebuildLevelState {
                level,
                shift: 0,
                passes: 0,
                representative_round: 1,
                shift_steps: 0,
                rebuild_steps: 0,
            })
            .collect(),
        metrics: ShiftRebuildGameMetrics::default(),
    }
}

fn transition(
    snapshot: &mut ShiftRebuildGameSnapshot,
    events: &mut Vec<ShiftRebuildGameTraceEvent>,
    record: bool,
    kind: ShiftRebuildGameEventKind,
    update: impl FnOnce(&mut ShiftRebuildGameSnapshot) -> Result<(), ShiftRebuildGameError>,
) -> Result<(), ShiftRebuildGameError> {
    let before = snapshot.clone();
    update(snapshot)?;
    snapshot.metrics.state_transitions = checked_increment(snapshot.metrics.state_transitions)?;
    if record {
        if events.len() >= SHIFT_REBUILD_GAME_MAX_TRACE_EVENTS {
            return Err(ShiftRebuildGameError::AdmissionLimit);
        }
        events.push(ShiftRebuildGameTraceEvent {
            catalog_id: CATALOG_ID,
            kind,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(())
}

fn apply_rebuild(
    state: &mut ShiftRebuildGameSnapshot,
    level: usize,
) -> Result<(), ShiftRebuildGameError> {
    let current_round = state.current_round;
    for current in state.levels.iter_mut().skip(level) {
        current.shift = 0;
        current.passes = 0;
        current.representative_round = current_round;
    }
    let rebuilt = state
        .levels
        .get_mut(level)
        .ok_or(ShiftRebuildGameError::InvalidInput)?;
    rebuilt.rebuild_steps = checked_increment(rebuilt.rebuild_steps)?;
    state.stage = ShiftRebuildGameStage::AwaitingRoundDecision;
    state.step_counter = checked_increment(state.step_counter)?;
    state.metrics.adversary_steps = checked_increment(state.metrics.adversary_steps)?;
    state.metrics.rebuild_steps = checked_increment(state.metrics.rebuild_steps)?;
    Ok(())
}

fn apply_adversary_do_nothing(
    state: &mut ShiftRebuildGameSnapshot,
) -> Result<(), ShiftRebuildGameError> {
    state.stage = ShiftRebuildGameStage::AwaitingRoundDecision;
    state.step_counter = checked_increment(state.step_counter)?;
    state.metrics.adversary_steps = checked_increment(state.metrics.adversary_steps)?;
    Ok(())
}

fn largest_eligible_level(
    state: &ShiftRebuildGameSnapshot,
    config: ShiftRebuildGameConfig,
) -> Option<usize> {
    let pass_ceiling = config.psi.checked_mul(2)?;
    state
        .levels
        .iter()
        .rposition(|level| level.passes < pass_ceiling)
}

fn apply_player_shift(
    state: &mut ShiftRebuildGameSnapshot,
    config: ShiftRebuildGameConfig,
    level: usize,
) -> Result<(), ShiftRebuildGameError> {
    if largest_eligible_level(state, config) != Some(level) {
        return Err(ShiftRebuildGameError::StrategyExhausted);
    }
    let current_round = state.current_round;
    let deeper_start = level
        .checked_add(1)
        .ok_or(ShiftRebuildGameError::ArithmeticOverflow)?;
    for deeper in state.levels.iter_mut().skip(deeper_start) {
        deeper.shift = 0;
        deeper.passes = 0;
        deeper.representative_round = current_round;
    }
    let selected = state
        .levels
        .get_mut(level)
        .ok_or(ShiftRebuildGameError::InvalidInput)?;
    selected.shift = selected
        .shift
        .checked_add(1)
        .ok_or(ShiftRebuildGameError::ArithmeticOverflow)?
        % config.branches;
    let wrapped = selected.shift == 0;
    if wrapped {
        selected.passes = checked_increment(selected.passes)?;
        selected.representative_round = minimum_weight_round(
            &state.weight_history,
            selected.representative_round,
            current_round,
        )?;
    }
    selected.shift_steps = checked_increment(selected.shift_steps)?;
    state.stage = ShiftRebuildGameStage::Shifted;
    state.step_counter = checked_increment(state.step_counter)?;
    state.metrics.shift_steps = checked_increment(state.metrics.shift_steps)?;
    if wrapped {
        state.metrics.branch_wraps = checked_increment(state.metrics.branch_wraps)?;
    }
    Ok(())
}

fn minimum_weight_round(
    weights: &[BigRational],
    start_round: usize,
    end_round: usize,
) -> Result<usize, ShiftRebuildGameError> {
    if start_round == 0 || start_round > end_round || end_round > weights.len() {
        return Err(ShiftRebuildGameError::InvalidInput);
    }
    let mut best_round = start_round;
    let first_candidate = start_round
        .checked_add(1)
        .ok_or(ShiftRebuildGameError::ArithmeticOverflow)?;
    for round in first_candidate..=end_round {
        if weights[round - 1] < weights[best_round - 1] {
            best_round = round;
        }
    }
    Ok(best_round)
}

fn validate_snapshot(
    config: ShiftRebuildGameConfig,
    snapshot: &ShiftRebuildGameSnapshot,
) -> Result<(), ShiftRebuildGameError> {
    let pass_ceiling = config
        .psi
        .checked_mul(2)
        .ok_or(ShiftRebuildGameError::TraceVerification)?;
    let level_count = config
        .depth
        .checked_add(1)
        .ok_or(ShiftRebuildGameError::TraceVerification)?;
    let total_shift_steps = checked_level_metric_sum(snapshot, |level| level.shift_steps)?;
    let total_rebuild_steps = checked_level_metric_sum(snapshot, |level| level.rebuild_steps)?;
    let total_game_steps = snapshot
        .metrics
        .adversary_steps
        .checked_add(snapshot.metrics.shift_steps)
        .ok_or(ShiftRebuildGameError::TraceVerification)?;
    if snapshot.levels.len() != level_count
        || snapshot.current_round != snapshot.weight_history.len()
        || snapshot.completed_rounds > snapshot.current_round
        || snapshot.metrics.completed_rounds
            != u64::try_from(snapshot.completed_rounds)
                .map_err(|_| ShiftRebuildGameError::TraceVerification)?
        || snapshot.step_counter != total_game_steps
        || snapshot.metrics.shift_steps != total_shift_steps
        || snapshot.metrics.rebuild_steps != total_rebuild_steps
        || snapshot.levels.iter().enumerate().any(|(index, level)| {
            level.level != index
                || level.shift >= config.branches
                || level.passes > pass_ceiling
                || level.representative_round == 0
                || level.representative_round > snapshot.current_round.max(1)
        })
    {
        return Err(ShiftRebuildGameError::TraceVerification);
    }
    Ok(())
}

fn checked_level_metric_sum(
    snapshot: &ShiftRebuildGameSnapshot,
    metric: impl Fn(&ShiftRebuildLevelState) -> u64,
) -> Result<u64, ShiftRebuildGameError> {
    snapshot.levels.iter().try_fold(0_u64, |sum, level| {
        sum.checked_add(metric(level))
            .ok_or(ShiftRebuildGameError::TraceVerification)
    })
}

#[allow(clippy::too_many_lines)]
fn validate_transition(
    config: ShiftRebuildGameConfig,
    rounds: &[ShiftRebuildRound],
    round_index: &mut usize,
    continuations_seen: &mut u64,
    event: &ShiftRebuildGameTraceEvent,
) -> Result<(), ShiftRebuildGameError> {
    let before = &event.before;
    let after = &event.after;
    if after.metrics.state_transitions
        != before
            .metrics
            .state_transitions
            .checked_add(1)
            .ok_or(ShiftRebuildGameError::TraceVerification)?
    {
        return Err(ShiftRebuildGameError::TraceVerification);
    }
    let mut expected = before.clone();
    match &event.kind {
        ShiftRebuildGameEventKind::BeginRound { round, weight } => {
            let source = rounds
                .get(*round_index)
                .ok_or(ShiftRebuildGameError::TraceVerification)?;
            if !matches!(
                before.stage,
                ShiftRebuildGameStage::Ready | ShiftRebuildGameStage::RoundCompleted
            ) || *round
                != round_index
                    .checked_add(1)
                    .ok_or(ShiftRebuildGameError::TraceVerification)?
                || source.weight != *weight
                || before.completed_rounds != *round_index
            {
                return Err(ShiftRebuildGameError::TraceVerification);
            }
            expected.stage = ShiftRebuildGameStage::RoundStarted;
            expected.current_round = *round;
            expected.weight_history.push(weight.clone());
        }
        ShiftRebuildGameEventKind::AdversaryRebuild { level } => {
            let source = rounds
                .get(*round_index)
                .ok_or(ShiftRebuildGameError::TraceVerification)?;
            if before.stage != ShiftRebuildGameStage::RoundStarted
                || source.rebuild_level != Some(*level)
            {
                return Err(ShiftRebuildGameError::TraceVerification);
            }
            apply_rebuild(&mut expected, *level)?;
        }
        ShiftRebuildGameEventKind::AdversaryDoNothing => {
            let source = rounds
                .get(*round_index)
                .ok_or(ShiftRebuildGameError::TraceVerification)?;
            if before.stage != ShiftRebuildGameStage::RoundStarted || source.rebuild_level.is_some()
            {
                return Err(ShiftRebuildGameError::TraceVerification);
            }
            apply_adversary_do_nothing(&mut expected)?;
        }
        ShiftRebuildGameEventKind::RoundContinues => {
            let source = rounds
                .get(*round_index)
                .ok_or(ShiftRebuildGameError::TraceVerification)?;
            if !matches!(
                before.stage,
                ShiftRebuildGameStage::AwaitingRoundDecision | ShiftRebuildGameStage::Shifted
            ) || *continuations_seen >= source.continuations
            {
                return Err(ShiftRebuildGameError::TraceVerification);
            }
            *continuations_seen = checked_increment(*continuations_seen)?;
            expected.stage = ShiftRebuildGameStage::ContinuationRequested;
            expected.metrics.continuation_decisions =
                checked_increment(expected.metrics.continuation_decisions)?;
        }
        ShiftRebuildGameEventKind::PlayerShift { level, wrapped } => {
            let selected = before
                .levels
                .get(*level)
                .ok_or(ShiftRebuildGameError::TraceVerification)?;
            if before.stage != ShiftRebuildGameStage::ContinuationRequested
                || largest_eligible_level(before, config) != Some(*level)
                || *wrapped
                    != selected
                        .shift
                        .checked_add(1)
                        .is_some_and(|shift| shift == config.branches)
            {
                return Err(ShiftRebuildGameError::TraceVerification);
            }
            apply_player_shift(&mut expected, config, *level)?;
        }
        ShiftRebuildGameEventKind::RoundCompletes => {
            let source = rounds
                .get(*round_index)
                .ok_or(ShiftRebuildGameError::TraceVerification)?;
            if !matches!(
                before.stage,
                ShiftRebuildGameStage::AwaitingRoundDecision | ShiftRebuildGameStage::Shifted
            ) || *continuations_seen != source.continuations
            {
                return Err(ShiftRebuildGameError::TraceVerification);
            }
            expected.stage = ShiftRebuildGameStage::RoundCompleted;
            expected.completed_rounds = expected
                .completed_rounds
                .checked_add(1)
                .ok_or(ShiftRebuildGameError::TraceVerification)?;
            expected.metrics.completed_rounds =
                checked_increment(expected.metrics.completed_rounds)?;
            *round_index = round_index
                .checked_add(1)
                .ok_or(ShiftRebuildGameError::TraceVerification)?;
            *continuations_seen = 0;
        }
        ShiftRebuildGameEventKind::Completed => {
            if before.stage != ShiftRebuildGameStage::RoundCompleted || *round_index != rounds.len()
            {
                return Err(ShiftRebuildGameError::TraceVerification);
            }
            expected.stage = ShiftRebuildGameStage::Complete;
        }
    }
    expected.metrics.state_transitions = checked_increment(expected.metrics.state_transitions)?;
    if expected != *after {
        return Err(ShiftRebuildGameError::TraceVerification);
    }
    Ok(())
}

fn checked_increment(value: u64) -> Result<u64, ShiftRebuildGameError> {
    value
        .checked_add(1)
        .ok_or(ShiftRebuildGameError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use num_rational::BigRational;

    use super::*;

    fn weight(numerator: i64, denominator: i64) -> BigRational {
        BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
    }

    fn config() -> ShiftRebuildGameConfig {
        ShiftRebuildGameConfig {
            depth: 1,
            branches: 2,
            psi: 1,
        }
    }

    #[test]
    fn largest_eligible_strategy_cascades_from_deep_to_shallow_level() {
        let trace = trace_shift_rebuild_game(
            config(),
            &[ShiftRebuildRound {
                weight: weight(1, 1),
                rebuild_level: None,
                continuations: 5,
            }],
        )
        .expect("trace");
        assert_eq!(trace.result.shifts, vec![1, 4]);
        assert_eq!(trace.result.final_snapshot.levels[1].passes, 0);
        let selected = trace
            .events
            .iter()
            .filter_map(|event| match event.kind {
                ShiftRebuildGameEventKind::PlayerShift { level, .. } => Some(level),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(selected, vec![1, 1, 1, 1, 0]);
    }

    #[test]
    fn rebuild_resets_exact_suffix_and_counts_only_selected_level() {
        let rounds = [
            ShiftRebuildRound {
                weight: weight(3, 2),
                rebuild_level: None,
                continuations: 2,
            },
            ShiftRebuildRound {
                weight: weight(3, 4),
                rebuild_level: Some(1),
                continuations: 1,
            },
        ];
        let result = play_shift_rebuild_game(config(), &rounds).expect("play");
        assert_eq!(result.rebuilds, vec![0, 1]);
        assert_eq!(result.final_snapshot.levels[0].representative_round, 1);
        assert_eq!(result.final_snapshot.levels[1].representative_round, 2);
    }

    #[test]
    fn branch_wrap_uses_earliest_minimum_hidden_weight_round() {
        let config = ShiftRebuildGameConfig {
            depth: 1,
            branches: 1,
            psi: 2,
        };
        let rounds = [
            ShiftRebuildRound {
                weight: weight(3, 1),
                rebuild_level: None,
                continuations: 1,
            },
            ShiftRebuildRound {
                weight: weight(1, 1),
                rebuild_level: None,
                continuations: 1,
            },
        ];
        let result = play_shift_rebuild_game(config, &rounds).expect("play");
        assert_eq!(result.final_snapshot.levels[1].representative_round, 2);
        assert_eq!(result.final_snapshot.metrics.branch_wraps, 2);
    }

    #[test]
    fn independent_checker_rejects_nonlargest_shift() {
        let rounds = [ShiftRebuildRound {
            weight: weight(1, 1),
            rebuild_level: None,
            continuations: 1,
        }];
        let mut trace = trace_shift_rebuild_game(config(), &rounds).expect("trace");
        let event = trace
            .events
            .iter_mut()
            .find(|event| matches!(event.kind, ShiftRebuildGameEventKind::PlayerShift { .. }))
            .expect("shift");
        let ShiftRebuildGameEventKind::PlayerShift { level, .. } = &mut event.kind else {
            panic!("shift event");
        };
        *level = 0;
        assert_eq!(
            check_shift_rebuild_game_trace(config(), &rounds, &trace),
            Err(ShiftRebuildGameError::TraceVerification)
        );
    }

    #[test]
    fn rejects_hidden_weight_boundary_and_strategy_exhaustion() {
        assert_eq!(
            play_shift_rebuild_game(
                config(),
                &[ShiftRebuildRound {
                    weight: weight(2, 1),
                    rebuild_level: None,
                    continuations: 0,
                }]
            ),
            Err(ShiftRebuildGameError::InvalidInput)
        );
        assert_eq!(
            play_shift_rebuild_game(
                ShiftRebuildGameConfig {
                    depth: 1,
                    branches: 1,
                    psi: 1,
                },
                &[ShiftRebuildRound {
                    weight: weight(1, 1),
                    rebuild_level: None,
                    continuations: 9,
                }]
            ),
            Err(ShiftRebuildGameError::StrategyExhausted)
        );
    }
}
