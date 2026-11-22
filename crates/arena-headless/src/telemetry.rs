//! Structured combat telemetry and summary statistics shared by
//! `arena-headless` and `arena-bot`.
//!
//! All collections use deterministic ordering (`BTreeMap`) and every floating
//! point rate or percentile guards against zero denominators so JSON output is
//! always valid and free of `NaN`/`Infinity`.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use arena_sim::arena::{ArenaEvent, InterruptReason, Rejection, RoundResult};
use arena_sim::spells::SpellId;
use serde::Serialize;

/// Version of the `HeadlessReport` JSON schema.
pub const TELEMETRY_SCHEMA_VERSION: u32 = 1;

/// Outcome of a single completed arena round.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchOutcome {
    Team0Win,
    Team1Win,
    MutualDraw,
    TimeLimitDraw,
}

impl From<RoundResult> for MatchOutcome {
    fn from(result: RoundResult) -> Self {
        match result {
            RoundResult::Won { team: 0 } => Self::Team0Win,
            RoundResult::Won { .. } => Self::Team1Win,
            RoundResult::Draw => Self::MutualDraw,
            RoundResult::TimeLimitDraw => Self::TimeLimitDraw,
        }
    }
}

impl MatchOutcome {
    pub const fn tag(self) -> u8 {
        match self {
            Self::Team0Win => 1,
            Self::Team1Win => 2,
            Self::MutualDraw => 3,
            Self::TimeLimitDraw => 4,
        }
    }
}

/// Aggregated round outcome counts and win/draw rates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct OutcomeSummary {
    pub matches_finished: u64,
    pub team0_wins: u64,
    pub team1_wins: u64,
    pub mutual_draws: u64,
    pub time_limit_draws: u64,
    pub total_draws: u64,
    pub team0_win_rate: f64,
    pub team1_win_rate: f64,
    pub draw_rate: f64,
    pub time_limit_draw_rate: f64,
}

impl OutcomeSummary {
    pub fn from_results(results: impl IntoIterator<Item = RoundResult>) -> Self {
        let mut team0_wins = 0u64;
        let mut team1_wins = 0u64;
        let mut mutual_draws = 0u64;
        let mut time_limit_draws = 0u64;
        for result in results {
            match MatchOutcome::from(result) {
                MatchOutcome::Team0Win => team0_wins += 1,
                MatchOutcome::Team1Win => team1_wins += 1,
                MatchOutcome::MutualDraw => mutual_draws += 1,
                MatchOutcome::TimeLimitDraw => time_limit_draws += 1,
            }
        }
        let total_draws = mutual_draws + time_limit_draws;
        let matches_finished = team0_wins + team1_wins + total_draws;
        let rate = |n: u64| {
            if matches_finished == 0 {
                0.0
            } else {
                n as f64 / matches_finished as f64
            }
        };
        Self {
            matches_finished,
            team0_wins,
            team1_wins,
            mutual_draws,
            time_limit_draws,
            total_draws,
            team0_win_rate: rate(team0_wins),
            team1_win_rate: rate(team1_wins),
            draw_rate: rate(total_draws),
            time_limit_draw_rate: rate(time_limit_draws),
        }
    }
}

/// Distribution of completed match durations in ticks and seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct DurationSummary {
    pub count: usize,
    pub min_ticks: u64,
    pub mean_ticks: f64,
    pub p50_ticks: u64,
    pub p95_ticks: u64,
    pub max_ticks: u64,
    pub min_s: f64,
    pub mean_s: f64,
    pub p50_s: f64,
    pub p95_s: f64,
    pub max_s: f64,
}

impl DurationSummary {
    /// Summarizes round durations using nearest-rank percentiles.
    pub fn from_ticks(mut ticks: Vec<u64>, tick_rate: u64) -> Self {
        if ticks.is_empty() {
            return Self::default();
        }
        ticks.sort_unstable();
        let count = ticks.len();
        let sum: u128 = ticks.iter().map(|&t| u128::from(t)).sum();
        let mean_ticks = sum as f64 / count as f64;
        let rate = tick_rate.max(1) as f64;
        let at = |fraction: f64| -> u64 {
            let rank = ((fraction * count as f64).ceil() as usize).clamp(1, count);
            ticks[rank - 1]
        };
        let min_ticks = ticks[0];
        let p50_ticks = at(0.50);
        let p95_ticks = at(0.95);
        let max_ticks = ticks[count - 1];
        Self {
            count,
            min_ticks,
            mean_ticks,
            p50_ticks,
            p95_ticks,
            max_ticks,
            min_s: min_ticks as f64 / rate,
            mean_s: mean_ticks / rate,
            p50_s: p50_ticks as f64 / rate,
            p95_s: p95_ticks as f64 / rate,
            max_s: max_ticks as f64 / rate,
        }
    }
}

/// Collects per-tick wall-clock compute timings in nanoseconds (percentiles
/// stored as `u32` clamped to `u32::MAX` ns ≈ 4.29 s; `total_ns` and `max_ns`
/// stay unclamped `u128` so `max_us` never falls below `mean_us`) and
/// summarizes in microseconds.
#[derive(Clone, Debug, Default)]
pub struct TickTimings {
    samples_ns: Vec<u32>,
    total_ns: u128,
    max_ns: u128,
}

impl TickTimings {
    pub fn record(&mut self, elapsed: Duration) {
        let nanos = elapsed.as_nanos();
        self.total_ns = self.total_ns.saturating_add(nanos);
        self.max_ns = self.max_ns.max(nanos);
        self.samples_ns
            .push(u32::try_from(nanos).unwrap_or(u32::MAX));
    }

    pub fn summary(&mut self) -> TickComputeSummary {
        let total_ticks = self.samples_ns.len() as u64;
        if total_ticks == 0 {
            return TickComputeSummary::default();
        }
        self.samples_ns.sort_unstable();
        let count = self.samples_ns.len();
        let at_us = |fraction: f64| -> f64 {
            let rank = ((fraction * count as f64).ceil() as usize).clamp(1, count);
            f64::from(self.samples_ns[rank - 1]) / 1000.0
        };
        let total_us = self.total_ns as f64 / 1000.0;
        let mean_us = total_us / total_ticks as f64;
        let max_us = self.max_ns as f64 / 1000.0;
        let ticks_per_second = if self.total_ns == 0 {
            0.0
        } else {
            total_ticks as f64 / (self.total_ns as f64 / 1_000_000_000.0)
        };
        TickComputeSummary {
            total_ticks,
            total_us,
            mean_us,
            p50_us: at_us(0.50),
            p95_us: at_us(0.95),
            p99_us: at_us(0.99),
            max_us,
            ticks_per_second,
        }
    }
}

/// Summary of pure simulation `Arena::advance` compute time in microseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct TickComputeSummary {
    pub total_ticks: u64,
    pub total_us: f64,
    pub mean_us: f64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
    pub ticks_per_second: f64,
}

/// Per-spell event counts across the shared four-spell arena kit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SpellCounts {
    pub pulse: u64,
    pub rift_bolt: u64,
    pub quell: u64,
    pub mend: u64,
}

impl SpellCounts {
    pub fn record(&mut self, spell: SpellId) {
        match spell {
            SpellId::Pulse => self.pulse += 1,
            SpellId::RiftBolt => self.rift_bolt += 1,
            SpellId::Quell => self.quell += 1,
            SpellId::Mend => self.mend += 1,
        }
    }

    pub const fn total(&self) -> u64 {
        self.pulse + self.rift_bolt + self.quell + self.mend
    }

    pub fn merge(&mut self, other: &Self) {
        self.pulse += other.pulse;
        self.rift_bolt += other.rift_bolt;
        self.quell += other.quell;
        self.mend += other.mend;
    }
}

/// Deterministic combat telemetry counters for balance and LOS analysis.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CombatMetrics {
    /// Total line-of-sight blocks (`los_rejections + los_cast_interrupts`).
    pub los_blocks: u64,
    /// Cast attempts rejected immediately for blocked line of sight.
    pub los_rejections: u64,
    /// In-flight timed casts interrupted at completion for blocked line of sight.
    pub los_cast_interrupts: u64,
    /// Total out-of-range failures (cast rejections + completion interrupts).
    pub out_of_range_blocks: u64,
    pub hits: u64,
    pub total_damage: u64,
    pub damage_by_team: [u64; 2],
    pub heals: u64,
    pub total_healing: u64,
    pub healing_by_team: [u64; 2],
    pub jumps: u64,
    /// Timed casts started (`CastStarted` events).
    pub casts_started: SpellCounts,
    /// Completed timed casts and instant interrupt casts (`CastCompleted` events;
    /// instant `Pulse` attacks emit `Hit` directly rather than `CastCompleted`).
    pub casts_completed: SpellCounts,
    /// Interrupted timed casts (`CastInterrupted` events).
    pub casts_interrupted: SpellCounts,
    /// Enemy kick interrupts landed (`InterruptReason::Interrupted`).
    pub interrupts_landed: u64,
    pub interrupts_by_reason: BTreeMap<String, u64>,
    pub rejections_total: u64,
    pub rejections_by_reason: BTreeMap<String, u64>,
}

impl CombatMetrics {
    pub fn record_arena_event(&mut self, event: &ArenaEvent) {
        match *event {
            ArenaEvent::Moved { .. } | ArenaEvent::Finished(_) => {}
            ArenaEvent::Jumped { .. } => self.record_jump(),
            ArenaEvent::CastStarted { spell, .. } => self.record_cast_started(spell),
            ArenaEvent::CastCompleted { spell, .. } => self.record_cast_completed(spell),
            ArenaEvent::CastInterrupted { spell, reason, .. } => {
                self.record_cast_interrupted(spell, reason);
            }
            ArenaEvent::Hit { actor, damage, .. } => self.record_hit(actor, damage),
            ArenaEvent::Healed { actor, amount, .. } => self.record_heal(actor, amount),
            ArenaEvent::Rejected { reason, .. } => self.record_rejection(reason),
        }
    }

    pub fn record_jump(&mut self) {
        self.jumps += 1;
    }

    pub fn record_cast_started(&mut self, spell: SpellId) {
        self.casts_started.record(spell);
    }

    pub fn record_cast_completed(&mut self, spell: SpellId) {
        self.casts_completed.record(spell);
    }

    pub fn record_cast_interrupted(&mut self, spell: SpellId, reason: InterruptReason) {
        self.casts_interrupted.record(spell);
        *self
            .interrupts_by_reason
            .entry(interrupt_reason_key(reason).to_owned())
            .or_default() += 1;
        match reason {
            InterruptReason::LineOfSight => {
                self.los_cast_interrupts += 1;
                self.los_blocks += 1;
            }
            InterruptReason::OutOfRange => {
                self.out_of_range_blocks += 1;
            }
            InterruptReason::Interrupted => {
                self.interrupts_landed += 1;
            }
            InterruptReason::Cancelled
            | InterruptReason::Moved
            | InterruptReason::Silenced
            | InterruptReason::TargetDead
            | InterruptReason::CasterDead => {}
        }
    }

    pub fn record_hit(&mut self, actor: usize, damage: u16) {
        self.hits += 1;
        let amount = u64::from(damage);
        self.total_damage += amount;
        self.damage_by_team[actor % 2] += amount;
    }

    pub fn record_heal(&mut self, actor: usize, amount: u16) {
        self.heals += 1;
        let value = u64::from(amount);
        self.total_healing += value;
        self.healing_by_team[actor % 2] += value;
    }

    pub fn record_rejection(&mut self, reason: Rejection) {
        self.rejections_total += 1;
        *self
            .rejections_by_reason
            .entry(rejection_key(reason).to_owned())
            .or_default() += 1;
        match reason {
            Rejection::LineOfSight => {
                self.los_rejections += 1;
                self.los_blocks += 1;
            }
            Rejection::OutOfRange => {
                self.out_of_range_blocks += 1;
            }
            _ => {}
        }
    }

    pub fn merge(&mut self, other: &Self) {
        self.los_blocks += other.los_blocks;
        self.los_rejections += other.los_rejections;
        self.los_cast_interrupts += other.los_cast_interrupts;
        self.out_of_range_blocks += other.out_of_range_blocks;
        self.hits += other.hits;
        self.total_damage += other.total_damage;
        self.damage_by_team[0] += other.damage_by_team[0];
        self.damage_by_team[1] += other.damage_by_team[1];
        self.heals += other.heals;
        self.total_healing += other.total_healing;
        self.healing_by_team[0] += other.healing_by_team[0];
        self.healing_by_team[1] += other.healing_by_team[1];
        self.jumps += other.jumps;
        self.casts_started.merge(&other.casts_started);
        self.casts_completed.merge(&other.casts_completed);
        self.casts_interrupted.merge(&other.casts_interrupted);
        self.interrupts_landed += other.interrupts_landed;
        for (key, &count) in &other.interrupts_by_reason {
            *self.interrupts_by_reason.entry(key.clone()).or_default() += count;
        }
        self.rejections_total += other.rejections_total;
        for (key, &count) in &other.rejections_by_reason {
            *self.rejections_by_reason.entry(key.clone()).or_default() += count;
        }
    }
}

/// Telemetry record for a single simulated match.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MatchRecord {
    pub match_index: usize,
    pub seed: u64,
    pub result: MatchOutcome,
    pub duration_ticks: u64,
    pub duration_s: f64,
    pub los_blocks: u64,
    pub hits: u64,
    pub total_damage: u64,
    pub total_healing: u64,
    pub interrupts_landed: u64,
    pub final_state_hash: u64,
}

/// Complete structured report emitted by `arena-headless` combat simulations.
#[derive(Clone, Debug, Serialize)]
pub struct HeadlessReport {
    pub schema_version: u32,
    pub map: String,
    pub map_hash: u64,
    pub team_size: usize,
    pub tick_rate: u64,
    pub round_ticks: u64,
    pub seed: u64,
    pub matches_requested: usize,
    /// Deterministic FNV-1a digest over every match's seed, duration, outcome,
    /// and final `Arena::state_hash()`.
    pub combined_state_hash: u64,
    pub outcomes: OutcomeSummary,
    pub duration: DurationSummary,
    pub combat: CombatMetrics,
    pub tick_compute: TickComputeSummary,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<MatchRecord>,
}

impl fmt::Display for HeadlessReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "scenario       {} matches × {}v{} on {} (seed {}, limit {} ticks / {:.0} s)",
            self.matches_requested,
            self.team_size,
            self.team_size,
            self.map,
            self.seed,
            self.round_ticks,
            self.round_ticks as f64 / self.tick_rate.max(1) as f64
        )?;
        writeln!(
            f,
            "outcomes       {} finished: team0 {} ({:.1}%), team1 {} ({:.1}%), draws {} ({:.1}%; {} mutual, {} time-limit)",
            self.outcomes.matches_finished,
            self.outcomes.team0_wins,
            self.outcomes.team0_win_rate * 100.0,
            self.outcomes.team1_wins,
            self.outcomes.team1_win_rate * 100.0,
            self.outcomes.total_draws,
            self.outcomes.draw_rate * 100.0,
            self.outcomes.mutual_draws,
            self.outcomes.time_limit_draws,
        )?;
        writeln!(
            f,
            "duration       mean {:.1} ticks ({:.2} s)  p50 {} ({:.2} s)  p95 {} ({:.2} s)  max {} ({:.2} s)",
            self.duration.mean_ticks,
            self.duration.mean_s,
            self.duration.p50_ticks,
            self.duration.p50_s,
            self.duration.p95_ticks,
            self.duration.p95_s,
            self.duration.max_ticks,
            self.duration.max_s,
        )?;
        writeln!(
            f,
            "combat         {} hits ({} dmg, team0 {} / team1 {}), {} heals ({} hp), {} interrupts landed",
            self.combat.hits,
            self.combat.total_damage,
            self.combat.damage_by_team[0],
            self.combat.damage_by_team[1],
            self.combat.heals,
            self.combat.total_healing,
            self.combat.interrupts_landed,
        )?;
        writeln!(
            f,
            "blocks         {} LOS blocks ({} rejected, {} mid-cast), {} out-of-range, {} total rejections",
            self.combat.los_blocks,
            self.combat.los_rejections,
            self.combat.los_cast_interrupts,
            self.combat.out_of_range_blocks,
            self.combat.rejections_total,
        )?;
        writeln!(
            f,
            "casts          started {} (bolt {} mend {}), completed {} (bolt {} quell {} mend {}), interrupted {}",
            self.combat.casts_started.total(),
            self.combat.casts_started.rift_bolt,
            self.combat.casts_started.mend,
            self.combat.casts_completed.total(),
            self.combat.casts_completed.rift_bolt,
            self.combat.casts_completed.quell,
            self.combat.casts_completed.mend,
            self.combat.casts_interrupted.total(),
        )?;
        writeln!(
            f,
            "tick compute   {} ticks, mean {:.2} µs  p50 {:.2} µs  p95 {:.2} µs  p99 {:.2} µs  max {:.2} µs ({:.0} ticks/s)",
            self.tick_compute.total_ticks,
            self.tick_compute.mean_us,
            self.tick_compute.p50_us,
            self.tick_compute.p95_us,
            self.tick_compute.p99_us,
            self.tick_compute.max_us,
            self.tick_compute.ticks_per_second,
        )?;
        writeln!(f, "state hash     {:016x}", self.combined_state_hash)?;
        Ok(())
    }
}

pub const fn interrupt_reason_key(reason: InterruptReason) -> &'static str {
    match reason {
        InterruptReason::Cancelled => "cancelled",
        InterruptReason::Moved => "moved",
        InterruptReason::Interrupted => "interrupted",
        InterruptReason::Silenced => "silenced",
        InterruptReason::OutOfRange => "out_of_range",
        InterruptReason::LineOfSight => "line_of_sight",
        InterruptReason::TargetDead => "target_dead",
        InterruptReason::CasterDead => "caster_dead",
    }
}

pub const fn rejection_key(reason: Rejection) -> &'static str {
    match reason {
        Rejection::InvalidActor => "invalid_actor",
        Rejection::InvalidTarget => "invalid_target",
        Rejection::InvalidMovement => "invalid_movement",
        Rejection::Duplicate => "duplicate",
        Rejection::Airborne => "airborne",
        Rejection::MatchOver => "match_over",
        Rejection::Cooldown => "cooldown",
        Rejection::GlobalCooldown => "global_cooldown",
        Rejection::AlreadyCasting => "already_casting",
        Rejection::NotEnoughResource => "not_enough_resource",
        Rejection::Moving => "moving",
        Rejection::Silenced => "silenced",
        Rejection::NotCasting => "not_casting",
        Rejection::OutOfRange => "out_of_range",
        Rejection::LineOfSight => "line_of_sight",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_summaries_are_zero_and_serialize_to_finite_json() {
        let outcomes = OutcomeSummary::from_results([]);
        assert_eq!(outcomes.matches_finished, 0);
        assert_eq!(outcomes.draw_rate, 0.0);
        assert_eq!(outcomes.team0_win_rate, 0.0);

        let duration = DurationSummary::from_ticks(Vec::new(), 30);
        assert_eq!(duration.count, 0);
        assert_eq!(duration.mean_s, 0.0);

        let mut timings = TickTimings::default();
        let compute = timings.summary();
        assert_eq!(compute.total_ticks, 0);
        assert_eq!(compute.ticks_per_second, 0.0);

        // Zero-duration sample must never produce Infinity or NaN in JSON.
        timings.record(Duration::ZERO);
        let zero_compute = timings.summary();
        assert_eq!(zero_compute.total_ticks, 1);
        assert_eq!(zero_compute.ticks_per_second, 0.0);
        assert!(zero_compute.ticks_per_second.is_finite());

        let json = serde_json::to_string(&(outcomes, duration, zero_compute)).unwrap();
        assert!(!json.contains("null"), "unexpected null in {json}");
    }

    #[test]
    fn outcomes_and_duration_percentiles_are_exact() {
        let outcomes = OutcomeSummary::from_results([
            RoundResult::Won { team: 0 },
            RoundResult::Won { team: 1 },
            RoundResult::Draw,
            RoundResult::TimeLimitDraw,
        ]);
        assert_eq!(outcomes.matches_finished, 4);
        assert_eq!(outcomes.team0_wins, 1);
        assert_eq!(outcomes.team1_wins, 1);
        assert_eq!(outcomes.mutual_draws, 1);
        assert_eq!(outcomes.time_limit_draws, 1);
        assert_eq!(outcomes.total_draws, 2);
        assert!((outcomes.draw_rate - 0.5).abs() < 1e-12);
        assert!((outcomes.time_limit_draw_rate - 0.25).abs() < 1e-12);
        assert!((outcomes.team0_win_rate - 0.25).abs() < 1e-12);

        let dur = DurationSummary::from_ticks((1..=100).collect(), 30);
        assert_eq!(dur.count, 100);
        assert_eq!(dur.min_ticks, 1);
        assert_eq!(dur.p50_ticks, 50);
        assert_eq!(dur.p95_ticks, 95);
        assert_eq!(dur.max_ticks, 100);
        assert!((dur.min_s - 1.0 / 30.0).abs() < 1e-12);
        assert!((dur.mean_ticks - 50.5).abs() < 1e-12);
        assert!((dur.p50_s - 50.0 / 30.0).abs() < 1e-12);
    }

    #[test]
    fn combat_metrics_records_every_event_and_merges_cleanly() {
        let mut a = CombatMetrics::default();
        a.record_arena_event(&ArenaEvent::CastStarted {
            actor: 0,
            spell: SpellId::RiftBolt,
            target: Some(1),
            duration_ticks: 45,
        });
        a.record_arena_event(&ArenaEvent::CastInterrupted {
            actor: 0,
            spell: SpellId::RiftBolt,
            reason: InterruptReason::LineOfSight,
            by: None,
        });
        a.record_arena_event(&ArenaEvent::CastStarted {
            actor: 1,
            spell: SpellId::Mend,
            target: Some(1),
            duration_ticks: 45,
        });
        a.record_arena_event(&ArenaEvent::CastInterrupted {
            actor: 1,
            spell: SpellId::Mend,
            reason: InterruptReason::Interrupted,
            by: Some(0),
        });
        a.record_arena_event(&ArenaEvent::CastCompleted {
            actor: 0,
            spell: SpellId::Pulse,
            target: Some(1),
        });
        a.record_arena_event(&ArenaEvent::Hit {
            actor: 0,
            target: 1,
            damage: 25,
        });
        a.record_arena_event(&ArenaEvent::Healed {
            actor: 1,
            target: 1,
            amount: 20,
        });
        a.record_arena_event(&ArenaEvent::Rejected {
            actor: 0,
            reason: Rejection::LineOfSight,
        });
        a.record_arena_event(&ArenaEvent::Rejected {
            actor: 1,
            reason: Rejection::OutOfRange,
        });
        a.record_arena_event(&ArenaEvent::Jumped { actor: 0 });
        a.record_arena_event(&ArenaEvent::Moved { actor: 0 });
        a.record_arena_event(&ArenaEvent::Finished(RoundResult::Won { team: 0 }));

        assert_eq!(a.los_blocks, 2);
        assert_eq!(a.los_rejections, 1);
        assert_eq!(a.los_cast_interrupts, 1);
        assert_eq!(a.out_of_range_blocks, 1);
        assert_eq!(a.interrupts_landed, 1);
        assert_eq!(a.hits, 1);
        assert_eq!(a.total_damage, 25);
        assert_eq!(a.damage_by_team, [25, 0]);
        assert_eq!(a.heals, 1);
        assert_eq!(a.total_healing, 20);
        assert_eq!(a.healing_by_team, [0, 20]);
        assert_eq!(a.jumps, 1);
        assert_eq!(a.casts_started.rift_bolt, 1);
        assert_eq!(a.casts_started.mend, 1);
        assert_eq!(a.casts_completed.pulse, 1);
        assert_eq!(a.casts_interrupted.rift_bolt, 1);
        assert_eq!(a.casts_interrupted.mend, 1);
        assert_eq!(a.rejections_total, 2);
        assert_eq!(a.rejections_by_reason.get("line_of_sight"), Some(&1));
        assert_eq!(a.interrupts_by_reason.get("interrupted"), Some(&1));

        let mut merged = CombatMetrics::default();
        merged.merge(&a);
        merged.merge(&a);
        assert_eq!(merged.los_blocks, 4);
        assert_eq!(merged.total_damage, 50);
        assert_eq!(merged.total_healing, 40);
        assert_eq!(merged.interrupts_landed, 2);
        assert_eq!(merged.rejections_by_reason.get("line_of_sight"), Some(&2));
    }

    #[test]
    fn tick_timings_measures_sub_microsecond_and_percentiles() {
        let mut timings = TickTimings::default();
        for ns in 1..=100 {
            timings.record(Duration::from_nanos(ns * 100));
        }
        let s = timings.summary();
        assert_eq!(s.total_ticks, 100);
        assert!((s.p50_us - 5.0).abs() < 1e-9);
        assert!((s.p95_us - 9.5).abs() < 1e-9);
        assert!((s.p99_us - 9.9).abs() < 1e-9);
        assert!((s.max_us - 10.0).abs() < 1e-9);
        assert!(s.ticks_per_second > 100_000.0);

        // Even an extreme >4.29s pause preserves max_us >= mean_us.
        let mut paused = TickTimings::default();
        paused.record(Duration::from_secs(6));
        let ps = paused.summary();
        assert!((ps.max_us - 6_000_000.0).abs() < 1e-6);
        assert!(ps.max_us >= ps.mean_us);
    }
}
