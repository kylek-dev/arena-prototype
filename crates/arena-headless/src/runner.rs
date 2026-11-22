//! Zero-network, deterministic headless arena match runner for objective
//! balance and tick-compute telemetry.

use std::time::Instant;

use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::{ActorInput, Arena, MAX_ROUND_TICKS, RoundResult};
use arena_sim::map::{Edge, Layout, MAX_TEAM};

use crate::policy::{Rng, decide};
use crate::telemetry::{
    CombatMetrics, DurationSummary, HeadlessReport, MatchOutcome, MatchRecord, OutcomeSummary,
    TELEMETRY_SCHEMA_VERSION, TickTimings,
};

/// Upper bound on batch size to keep CLI invocations bounded.
pub const MAX_HEADLESS_MATCHES: usize = 10_000;

/// Configuration for a headless combat telemetry batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadlessConfig {
    pub matches: usize,
    pub team_size: usize,
    pub round_ticks: u64,
    pub seed: u64,
    pub map_name: String,
    pub include_per_match: bool,
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            matches: 10,
            team_size: 2,
            round_ticks: MAX_ROUND_TICKS,
            seed: 1,
            map_name: "Colosseum".into(),
            include_per_match: false,
        }
    }
}

impl HeadlessConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=MAX_HEADLESS_MATCHES).contains(&self.matches) {
            return Err(format!(
                "matches must be between 1 and {MAX_HEADLESS_MATCHES}, got {}",
                self.matches
            ));
        }
        if !(1..=MAX_TEAM).contains(&self.team_size) {
            return Err(format!(
                "team_size must be between 1 and {MAX_TEAM}, got {}",
                self.team_size
            ));
        }
        if self.round_ticks == 0 || self.round_ticks > MAX_ROUND_TICKS {
            return Err(format!(
                "round_ticks must be between 1 and {MAX_ROUND_TICKS}, got {}",
                self.round_ticks
            ));
        }
        Ok(())
    }
}

/// Runs `config.matches` deterministic headless matches on `layout` and
/// returns structured combat and tick-compute telemetry.
///
/// The simulation itself stays pure and clock-free; only wall-clock time
/// around `Arena::advance` is measured for `tick_compute`.
pub fn run_matches(layout: &Layout, config: &HeadlessConfig) -> Result<HeadlessReport, String> {
    config.validate()?;
    let mut results: Vec<RoundResult> = Vec::with_capacity(config.matches);
    let mut durations: Vec<u64> = Vec::with_capacity(config.matches);
    let mut aggregate_combat = CombatMetrics::default();
    let mut tick_timings = TickTimings::default();
    let mut match_records = if config.include_per_match {
        Vec::with_capacity(config.matches)
    } else {
        Vec::new()
    };

    let mut combined_hash: u64 = 0xcbf2_9ce4_8422_2325;
    let map_hash = layout_digest(layout);
    let actor_count = config.team_size * 2;

    for match_index in 0..config.matches {
        let match_seed = config
            .seed
            .wrapping_add((match_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut match_rng = Rng::new(match_seed);
        let mut actor_rngs: [Rng; MAX_TEAM * 2] =
            std::array::from_fn(|_| Rng::new(match_rng.next_u64()));

        let mut arena = Arena::practice(layout.clone(), config.team_size)
            .ok_or_else(|| {
                format!(
                    "layout cannot host {}v{} matches (check starting room spawns)",
                    config.team_size, config.team_size
                )
            })?
            .with_round_limit(config.round_ticks)
            .ok_or_else(|| format!("invalid round_ticks {}", config.round_ticks))?;

        let mut match_combat = CombatMetrics::default();
        let mut inputs = [ActorInput::default(); MAX_TEAM * 2];

        while arena.result().is_none() {
            let tick = arena.tick();
            let fighters = arena.fighters();
            for actor in 0..actor_count {
                inputs[actor] = decide(actor, tick, fighters, &mut actor_rngs[actor]);
            }
            let started = Instant::now();
            let events = arena.advance(&inputs[..actor_count]);
            tick_timings.record(started.elapsed());
            for event in &events {
                match_combat.record_arena_event(event);
            }
        }

        let round_result = arena.result().expect("loop exited with result");
        let outcome = MatchOutcome::from(round_result);
        let duration_ticks = arena.tick();
        let final_state_hash = arena.state_hash();

        results.push(round_result);
        durations.push(duration_ticks);
        aggregate_combat.merge(&match_combat);

        combined_hash = fnv_u64(combined_hash, match_seed);
        combined_hash = fnv_u64(combined_hash, duration_ticks);
        combined_hash = fnv_u64(combined_hash, u64::from(outcome.tag()));
        combined_hash = fnv_u64(combined_hash, final_state_hash);

        if config.include_per_match {
            match_records.push(MatchRecord {
                match_index,
                seed: match_seed,
                result: outcome,
                duration_ticks,
                duration_s: duration_ticks as f64 / TICKS_PER_SECOND as f64,
                los_blocks: match_combat.los_blocks,
                hits: match_combat.hits,
                total_damage: match_combat.total_damage,
                total_healing: match_combat.total_healing,
                interrupts_landed: match_combat.interrupts_landed,
                final_state_hash,
            });
        }
    }

    Ok(HeadlessReport {
        schema_version: TELEMETRY_SCHEMA_VERSION,
        map: config.map_name.clone(),
        map_hash,
        team_size: config.team_size,
        tick_rate: TICKS_PER_SECOND,
        round_ticks: config.round_ticks,
        seed: config.seed,
        matches_requested: config.matches,
        combined_state_hash: combined_hash,
        outcomes: OutcomeSummary::from_results(results),
        duration: DurationSummary::from_ticks(durations, TICKS_PER_SECOND),
        combat: aggregate_combat,
        tick_compute: tick_timings.summary(),
        matches: match_records,
    })
}

fn layout_digest(layout: &Layout) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    hash = fnv_i32(hash, layout.half_x);
    hash = fnv_i32(hash, layout.half_z);
    hash = fnv_u64(hash, layout.pillars.len() as u64);
    for pillar in &layout.pillars {
        hash = fnv_i32(hash, pillar.center.x);
        hash = fnv_i32(hash, pillar.center.z);
        hash = fnv_i32(hash, pillar.half_x);
        hash = fnv_i32(hash, pillar.half_z);
    }
    hash = fnv_u64(hash, layout.rooms.len() as u64);
    for room in &layout.rooms {
        hash = fnv_u64(hash, room.team as u64);
        let edge_tag = match room.edge {
            Edge::North => 0u64,
            Edge::South => 1,
            Edge::West => 2,
            Edge::East => 3,
        };
        hash = fnv_u64(hash, edge_tag);
        hash = fnv_i32(hash, room.offset);
        hash = fnv_i32(hash, room.width);
        hash = fnv_i32(hash, room.depth);
        hash = fnv_u64(hash, room.spawns.len() as u64);
        for spawn in &room.spawns {
            hash = fnv_i32(hash, spawn.across);
            hash = fnv_i32(hash, spawn.back);
        }
    }
    hash
}

fn fnv_i32(mut hash: u64, value: i32) -> u64 {
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn fnv_u64(mut hash: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_batch_is_deterministic_and_emits_valid_json() {
        let map = arena_map::colosseum();
        let config = HeadlessConfig {
            matches: 4,
            team_size: 2,
            round_ticks: 60 * TICKS_PER_SECOND,
            seed: 42,
            map_name: map.name.clone(),
            include_per_match: true,
        };
        let first = run_matches(&map.layout, &config).unwrap();
        let second = run_matches(&map.layout, &config).unwrap();

        assert_eq!(first.combined_state_hash, second.combined_state_hash);
        assert_eq!(first.outcomes, second.outcomes);
        assert_eq!(first.duration, second.duration);
        assert_eq!(first.combat, second.combat);
        assert_eq!(first.matches, second.matches);

        assert_eq!(first.outcomes.matches_finished, 4);
        assert_eq!(first.duration.count, 4);
        assert_eq!(first.matches.len(), 4);
        assert!(first.combat.hits > 0, "expected bots to deal damage");
        assert!(first.combat.total_damage > 0);
        assert!(first.tick_compute.total_ticks > 0);
        assert!(first.tick_compute.mean_us.is_finite());

        let json = serde_json::to_string_pretty(&first).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["schema_version"], TELEMETRY_SCHEMA_VERSION);
        assert_eq!(parsed["outcomes"]["matches_finished"], 4);
        assert!(parsed["combat"]["los_blocks"].is_u64());
        assert!(parsed["tick_compute"]["mean_us"].is_f64());
    }

    #[test]
    fn short_round_limit_produces_100_percent_time_limit_draw_rate() {
        let map = arena_map::colosseum();
        let config = HeadlessConfig {
            matches: 3,
            team_size: 1,
            round_ticks: 30, // 1 second: fighters cannot even cross the arena
            seed: 9,
            map_name: map.name.clone(),
            include_per_match: true,
        };
        let report = run_matches(&map.layout, &config).unwrap();
        assert_eq!(report.outcomes.matches_finished, 3);
        assert_eq!(report.outcomes.time_limit_draws, 3);
        assert_eq!(report.outcomes.total_draws, 3);
        assert!((report.outcomes.draw_rate - 1.0).abs() < 1e-12);
        assert!((report.outcomes.time_limit_draw_rate - 1.0).abs() < 1e-12);
        assert_eq!(report.duration.min_ticks, 30);
        assert_eq!(report.duration.max_ticks, 30);
        assert!(
            report
                .matches
                .iter()
                .all(|m| m.result == MatchOutcome::TimeLimitDraw)
        );
    }

    #[test]
    fn config_validation_rejects_out_of_bounds_inputs() {
        let map = arena_map::colosseum();
        for bad in [
            HeadlessConfig {
                matches: 0,
                ..HeadlessConfig::default()
            },
            HeadlessConfig {
                matches: MAX_HEADLESS_MATCHES + 1,
                ..HeadlessConfig::default()
            },
            HeadlessConfig {
                team_size: 0,
                ..HeadlessConfig::default()
            },
            HeadlessConfig {
                team_size: MAX_TEAM + 1,
                ..HeadlessConfig::default()
            },
            HeadlessConfig {
                round_ticks: 0,
                ..HeadlessConfig::default()
            },
            HeadlessConfig {
                round_ticks: MAX_ROUND_TICKS + 1,
                ..HeadlessConfig::default()
            },
        ] {
            assert!(run_matches(&map.layout, &bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn different_seeds_diverge_and_colosseum_exercises_combat() {
        let map = arena_map::colosseum();
        let a = run_matches(
            &map.layout,
            &HeadlessConfig {
                matches: 6,
                team_size: 2,
                round_ticks: 120 * TICKS_PER_SECOND,
                seed: 1,
                map_name: map.name.clone(),
                include_per_match: false,
            },
        )
        .unwrap();
        let b = run_matches(
            &map.layout,
            &HeadlessConfig {
                matches: 6,
                team_size: 2,
                round_ticks: 120 * TICKS_PER_SECOND,
                seed: 2,
                map_name: map.name.clone(),
                include_per_match: false,
            },
        )
        .unwrap();
        assert_ne!(a.combined_state_hash, b.combined_state_hash);
        assert!(a.matches.is_empty(), "per-match omitted unless requested");
        assert!(a.combat.hits > 0 && b.combat.hits > 0);
        assert!(a.combat.casts_started.rift_bolt + b.combat.casts_started.rift_bolt > 0);
        assert!(a.combat.interrupts_landed + b.combat.interrupts_landed > 0);
    }

    #[test]
    fn center_pillar_layout_records_los_blocks_in_headless_telemetry() {
        use arena_sim::arena::Position;
        use arena_sim::map::{Pillar, Room, Spawn};

        // Compact arena with a central hexagonal pillar directly between teams
        // so combat naturally triggers line-of-sight blocks.
        let layout = Layout {
            half_x: 16_000,
            half_z: 14_000,
            pillars: vec![Pillar {
                center: Position { x: 0, z: 0 },
                half_x: 3_000,
                half_z: 3_000,
            }],
            rooms: vec![
                Room {
                    team: 0,
                    edge: Edge::South,
                    offset: 0,
                    width: 8_000,
                    depth: 6_000,
                    spawns: vec![
                        Spawn {
                            across: 0,
                            back: 2_000,
                        },
                        Spawn {
                            across: -2_000,
                            back: 2_000,
                        },
                        Spawn {
                            across: 2_000,
                            back: 2_000,
                        },
                    ],
                },
                Room {
                    team: 1,
                    edge: Edge::North,
                    offset: 0,
                    width: 8_000,
                    depth: 6_000,
                    spawns: vec![
                        Spawn {
                            across: 0,
                            back: 2_000,
                        },
                        Spawn {
                            across: -2_000,
                            back: 2_000,
                        },
                        Spawn {
                            across: 2_000,
                            back: 2_000,
                        },
                    ],
                },
            ],
        };
        assert_eq!(layout.validate(), Ok(()));

        let report = run_matches(
            &layout,
            &HeadlessConfig {
                matches: 6,
                team_size: 2,
                round_ticks: 60 * TICKS_PER_SECOND,
                seed: 5,
                map_name: "Center Pillar".into(),
                include_per_match: false,
            },
        )
        .unwrap();
        assert_eq!(report.outcomes.matches_finished, 6);
        assert!(
            report.combat.los_blocks > 0,
            "expected LOS blocks around central pillar: {:?}",
            report.combat
        );
        assert_eq!(
            report.combat.los_blocks,
            report.combat.los_rejections + report.combat.los_cast_interrupts
        );
        assert_eq!(
            report.combat.los_rejections,
            report
                .combat
                .rejections_by_reason
                .get("line_of_sight")
                .copied()
                .unwrap_or(0)
        );
    }
}
