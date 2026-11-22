//! Targeting-fairness and lag-compensation disagreement measurement.
//!
//! A browser client predicts its own fighter's movement immediately while
//! rendering other fighters behind the newest received snapshot (default 100 ms =
//! `DEFAULT_INTERP_DELAY_TICKS = 3` ticks, adapting down to ~42 ms on low-jitter
//! links via `InterpDelay`). Meanwhile, `arena-server` validates spell range,
//! line of sight, and target liveness authoritatively with bounded target-position
//! history rewind (ADR 0049).
//!
//! This module reproduces that client view in `arena-bot` and correlates each
//! targeted spell cast's `InputFrame.seq` against the exact tick snapshot that
//! acknowledges it (`Snapshot.ack == seq`) to measure how often the server's
//! `OutOfRange`, `LineOfSight`, and `InvalidTarget` verdicts disagree with what
//! the player saw on screen.

use std::collections::VecDeque;

use arena_protocol::{FighterState, Snapshot, WireEvent, WireRejection};
use arena_sim::arena::{Combatant, Position};
use arena_sim::map::Layout;
use arena_sim::spells::SpellId;
use serde::Serialize;

/// Default remote-fighter view delay (100 ms = 3 ticks at 30 Hz), matching
/// `arena_protocol::prediction::DEFAULT_INTERP_DELAY_TICKS`.
pub const DEFAULT_INTERP_DELAY_MS: u64 = 100;
/// Maximum buffered snapshots per seat (~2 s at 30 Hz).
const MAX_BUFFERED_SNAPSHOTS: usize = 64;
/// Maximum unacknowledged casts tracked per seat.
const MAX_PENDING_CASTS: usize = 256;

/// Converts an interpolation delay in milliseconds into whole server ticks
/// (rounded to nearest tick).
pub fn interp_delay_ticks(delay_ms: u64, tick_rate: u32) -> u64 {
    let rate = u64::from(tick_rate.max(1));
    (delay_ms.saturating_mul(rate).saturating_add(500)) / 1000
}

/// Recent authoritative snapshots kept by one seat so it can render remote
/// fighters at `latest.tick - interp_delay_ticks`, matching the browser client.
#[derive(Debug, Default)]
pub struct ViewBuffer {
    snapshots: VecDeque<Snapshot>,
}

/// Browser-parity decision view paired with the exact server tick of its
/// delayed remote-fighter state.
#[derive(Clone, Debug)]
pub struct DecisionView {
    pub snapshot: Snapshot,
    pub remote_tick: u64,
}

impl ViewBuffer {
    pub fn clear(&mut self) {
        self.snapshots.clear();
    }

    pub fn push(&mut self, snapshot: Snapshot) {
        if self
            .snapshots
            .back()
            .is_some_and(|last| snapshot.tick < last.tick)
        {
            self.snapshots.clear();
        }
        self.snapshots.push_back(snapshot);
        while self.snapshots.len() > MAX_BUFFERED_SNAPSHOTS {
            self.snapshots.pop_front();
        }
    }

    pub fn latest(&self) -> Option<&Snapshot> {
        self.snapshots.back()
    }

    /// Builds the browser-parity decision view for `actor`:
    /// - Remote fighters (`index != actor`) come from the newest buffered
    ///   snapshot at or before `latest.tick.saturating_sub(delay_ticks)`
    ///   (falling back to the oldest buffered snapshot while warming up).
    /// - Own fighter (`index == actor`) uses `latest`'s authoritative combat
    ///   state (health, resource, cooldowns, lockout, active_cast) with
    ///   `predicted_self`'s locally predicted movement/height/facing overlaid.
    pub fn decision_view(
        &self,
        actor: usize,
        predicted_self: Option<Combatant>,
        delay_ticks: u64,
    ) -> Option<DecisionView> {
        let latest = self.snapshots.back()?;
        let target_tick = latest.tick.saturating_sub(delay_ticks);
        let remote = self
            .snapshots
            .iter()
            .rev()
            .find(|snap| snap.tick <= target_tick)
            .or_else(|| self.snapshots.front())?;

        let mut view = latest.clone();
        if remote.fighters.len() == view.fighters.len() {
            for (index, dst) in view.fighters.iter_mut().enumerate() {
                if index != actor {
                    *dst = remote.fighters[index];
                }
            }
        }
        if let (Some(me), Some(pred)) = (view.fighters.get_mut(actor), predicted_self) {
            me.x = pred.position.x;
            me.z = pred.position.z;
            me.height = pred.height;
            me.vertical_velocity = pred.vertical_velocity;
            me.momentum = [pred.momentum.x, pred.momentum.z];
            me.facing = pred.facing;
        }
        Some(DecisionView {
            snapshot: view,
            remote_tick: remote.tick,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct PendingCast {
    seq: u32,
    target: usize,
    spell_range_mm: u16,
    client_dist_mm: u32,
    client_in_range: bool,
    client_has_los: bool,
    client_target_alive: bool,
}

/// Raw per-seat targeting-fairness accumulator merged across all seats.
#[derive(Debug, Default)]
pub struct FairnessStats {
    pub casts_sent: u64,
    pub casts_resolved: u64,
    pub casts_folded_or_skipped: u64,
    pub casts_accepted: u64,
    /// Casts accepted by the server via bounded target-position lag compensation
    /// even though present-tick geometry was out of range or lacked LoS.
    pub casts_saved_by_lag_comp: u64,
    /// Matched casts that passed pre-geometry checks (`Cooldown`, `GlobalCooldown`,
    /// `AlreadyCasting`, `Silenced`, `Moving`, `InvalidTarget`, `InvalidActor`,
    /// `MatchOver`) and actually reached server range/LoS validation.
    pub casts_reached_geometry: u64,
    pub casts_reached_geometry_in_range: u64,
    pub casts_reached_geometry_has_los: u64,
    pub casts_client_in_range: u64,
    pub casts_client_has_los: u64,
    pub casts_client_valid: u64,
    pub server_rejected_out_of_range: u64,
    pub disagreed_out_of_range: u64,
    pub server_rejected_los: u64,
    pub disagreed_los: u64,
    pub server_rejected_invalid_target: u64,
    pub disagreed_target_dead: u64,
    pub geom_out_of_range_disagreements: u64,
    pub geom_los_disagreements: u64,
    pub geom_target_dead_disagreements: u64,
    pub distance_error_mm: Vec<u32>,
    pub receding_error_mm: Vec<u32>,
}

impl FairnessStats {
    pub fn merge(&mut self, other: Self) {
        self.casts_sent += other.casts_sent;
        self.casts_resolved += other.casts_resolved;
        self.casts_folded_or_skipped += other.casts_folded_or_skipped;
        self.casts_accepted += other.casts_accepted;
        self.casts_saved_by_lag_comp += other.casts_saved_by_lag_comp;
        self.casts_reached_geometry += other.casts_reached_geometry;
        self.casts_reached_geometry_in_range += other.casts_reached_geometry_in_range;
        self.casts_reached_geometry_has_los += other.casts_reached_geometry_has_los;
        self.casts_client_in_range += other.casts_client_in_range;
        self.casts_client_has_los += other.casts_client_has_los;
        self.casts_client_valid += other.casts_client_valid;
        self.server_rejected_out_of_range += other.server_rejected_out_of_range;
        self.disagreed_out_of_range += other.disagreed_out_of_range;
        self.server_rejected_los += other.server_rejected_los;
        self.disagreed_los += other.disagreed_los;
        self.server_rejected_invalid_target += other.server_rejected_invalid_target;
        self.disagreed_target_dead += other.disagreed_target_dead;
        self.geom_out_of_range_disagreements += other.geom_out_of_range_disagreements;
        self.geom_los_disagreements += other.geom_los_disagreements;
        self.geom_target_dead_disagreements += other.geom_target_dead_disagreements;
        self.distance_error_mm.extend(other.distance_error_mm);
        self.receding_error_mm.extend(other.receding_error_mm);
    }

    pub fn summarize(self) -> TargetingFairnessReport {
        let ratio = |num: u64, den: u64| {
            if den == 0 {
                0.0
            } else {
                num as f64 / den as f64
            }
        };
        TargetingFairnessReport {
            casts_sent: self.casts_sent,
            casts_resolved: self.casts_resolved,
            casts_folded_or_skipped: self.casts_folded_or_skipped,
            casts_accepted: self.casts_accepted,
            casts_saved_by_lag_comp: self.casts_saved_by_lag_comp,
            casts_reached_geometry: self.casts_reached_geometry,
            casts_reached_geometry_in_range: self.casts_reached_geometry_in_range,
            casts_reached_geometry_has_los: self.casts_reached_geometry_has_los,
            casts_client_in_range: self.casts_client_in_range,
            casts_client_has_los: self.casts_client_has_los,
            casts_client_valid: self.casts_client_valid,
            server_rejected_out_of_range: self.server_rejected_out_of_range,
            disagreed_out_of_range: self.disagreed_out_of_range,
            out_of_range_disagreement_rate: ratio(
                self.disagreed_out_of_range,
                self.casts_reached_geometry_in_range,
            ),
            server_rejected_los: self.server_rejected_los,
            disagreed_los: self.disagreed_los,
            los_disagreement_rate: ratio(self.disagreed_los, self.casts_reached_geometry_has_los),
            server_rejected_invalid_target: self.server_rejected_invalid_target,
            disagreed_target_dead: self.disagreed_target_dead,
            geom_out_of_range_disagreements: self.geom_out_of_range_disagreements,
            geom_out_of_range_rate: ratio(
                self.geom_out_of_range_disagreements,
                self.casts_client_in_range,
            ),
            geom_los_disagreements: self.geom_los_disagreements,
            geom_los_rate: ratio(self.geom_los_disagreements, self.casts_client_has_los),
            geom_target_dead_disagreements: self.geom_target_dead_disagreements,
            distance_error_mm: crate::percentiles(self.distance_error_mm),
            receding_error_mm: crate::percentiles(self.receding_error_mm),
        }
    }
}

/// Serializable targeting-fairness report included in `arena_bot::Report`.
#[derive(Clone, Debug, Default, Serialize)]
pub struct TargetingFairnessReport {
    pub casts_sent: u64,
    pub casts_resolved: u64,
    pub casts_folded_or_skipped: u64,
    pub casts_accepted: u64,
    pub casts_saved_by_lag_comp: u64,
    pub casts_reached_geometry: u64,
    pub casts_reached_geometry_in_range: u64,
    pub casts_reached_geometry_has_los: u64,
    pub casts_client_in_range: u64,
    pub casts_client_has_los: u64,
    pub casts_client_valid: u64,
    pub server_rejected_out_of_range: u64,
    pub disagreed_out_of_range: u64,
    pub out_of_range_disagreement_rate: f64,
    pub server_rejected_los: u64,
    pub disagreed_los: u64,
    pub los_disagreement_rate: f64,
    pub server_rejected_invalid_target: u64,
    pub disagreed_target_dead: u64,
    pub geom_out_of_range_disagreements: u64,
    pub geom_out_of_range_rate: f64,
    pub geom_los_disagreements: u64,
    pub geom_los_rate: f64,
    pub geom_target_dead_disagreements: u64,
    /// `|server_distance - client_distance|` at cast tick in mm: `[p50, p99, max]`.
    pub distance_error_mm: [u32; 3],
    /// `max(0, server_distance - client_distance)` at cast tick in mm: `[p50, p99, max]`.
    pub receding_error_mm: [u32; 3],
}

/// Parameters describing one targeted cast submitted by a bot seat.
#[derive(Clone, Copy, Debug)]
pub struct CastSubmission {
    pub seq: u32,
    pub actor: usize,
    pub spell: SpellId,
    pub target: usize,
}

/// Per-seat pending-cast tracker correlating submitted casts to server verdicts.
#[derive(Debug, Default)]
pub struct FairnessTracker {
    pending: VecDeque<PendingCast>,
    last_ack: u32,
}

impl FairnessTracker {
    /// Clears connection-scoped sequence state on `Welcome` / reconnect while
    /// preserving accumulated `FairnessStats`.
    pub fn reset_connection(&mut self) {
        self.pending.clear();
        self.last_ack = 0;
    }

    /// Records a targeted cast submitted with sequence `cast.seq` as seen in `view`.
    pub fn record_cast(
        &mut self,
        stats: &mut FairnessStats,
        cast: CastSubmission,
        view: &Snapshot,
        layout: &Layout,
    ) {
        let (Some(me), Some(other)) = (
            view.fighters.get(cast.actor),
            view.fighters.get(cast.target),
        ) else {
            return;
        };
        let me_pos = pos(me);
        let other_pos = pos(other);
        let spell_range_mm = cast.spell.definition().range;
        let dist_sq = me_pos.distance_squared(other_pos);
        let client_dist_mm = u32::try_from((dist_sq as u64).isqrt()).unwrap_or(u32::MAX);
        let client_in_range = dist_sq <= i64::from(spell_range_mm).pow(2);
        let client_has_los = cast.actor == cast.target || layout.line_of_sight(me_pos, other_pos);
        let client_target_alive = other.health > 0;

        stats.casts_sent += 1;
        self.pending.push_back(PendingCast {
            seq: cast.seq,
            target: cast.target,
            spell_range_mm,
            client_dist_mm,
            client_in_range,
            client_has_los,
            client_target_alive,
        });
        while self.pending.len() > MAX_PENDING_CASTS {
            self.pending.pop_front();
        }
    }

    /// Reconciles pending casts against an authoritative `snapshot`.
    pub fn on_snapshot(
        &mut self,
        stats: &mut FairnessStats,
        actor: usize,
        snapshot: &Snapshot,
        layout: &Layout,
    ) {
        if snapshot.tick == 0 {
            // Waiting phase discards inputs without applying them.
            while self
                .pending
                .front()
                .is_some_and(|cast| cast.seq <= snapshot.ack)
            {
                self.pending.pop_front();
            }
            self.last_ack = snapshot.ack;
            return;
        }

        let prev_ack = self.last_ack;
        self.last_ack = snapshot.ack;

        while self
            .pending
            .front()
            .is_some_and(|cast| cast.seq <= snapshot.ack)
        {
            let cast = self.pending.pop_front().expect("checked front");
            if cast.seq <= prev_ack {
                continue;
            }
            if cast.seq < snapshot.ack {
                // Folded into a later input or an intermediate snapshot was skipped;
                // exclude from single-tick rejection attribution.
                stats.casts_folded_or_skipped += 1;
                continue;
            }
            // Exact 1-to-1 match: `cast.seq == snapshot.ack` was executed on `snapshot.tick - 1`.
            let (Some(me), Some(other)) = (
                snapshot.fighters.get(actor),
                snapshot.fighters.get(cast.target),
            ) else {
                continue;
            };
            stats.casts_resolved += 1;
            if cast.client_in_range {
                stats.casts_client_in_range += 1;
            }
            if cast.client_has_los {
                stats.casts_client_has_los += 1;
            }
            if cast.client_in_range && cast.client_has_los && cast.client_target_alive {
                stats.casts_client_valid += 1;
            }

            let me_pos = pos(me);
            let other_pos = pos(other);
            let server_dist_sq = me_pos.distance_squared(other_pos);
            let server_dist_mm = u32::try_from((server_dist_sq as u64).isqrt()).unwrap_or(u32::MAX);
            let server_in_range = server_dist_sq <= i64::from(cast.spell_range_mm).pow(2);
            let server_has_los = actor == cast.target || layout.line_of_sight(me_pos, other_pos);

            stats
                .distance_error_mm
                .push(server_dist_mm.abs_diff(cast.client_dist_mm));
            stats
                .receding_error_mm
                .push(server_dist_mm.saturating_sub(cast.client_dist_mm));

            let expected_event_tick = snapshot.tick.saturating_sub(1);
            let mut cast_rejection: Option<WireRejection> = None;
            let mut saw_cast_effect = false;
            let mut target_took_damage = false;
            for tick_event in &snapshot.events {
                if tick_event.tick != expected_event_tick {
                    continue;
                }
                match tick_event.event {
                    WireEvent::Rejected(reason)
                        if !matches!(
                            reason,
                            WireRejection::Airborne
                                | WireRejection::InvalidMovement
                                | WireRejection::Duplicate
                                | WireRejection::NotCasting
                        ) =>
                    {
                        cast_rejection = Some(reason);
                    }
                    WireEvent::Hit {
                        actor: a,
                        target: t,
                        damage,
                    } => {
                        if usize::from(a) == actor {
                            saw_cast_effect = true;
                        }
                        if usize::from(t) == cast.target && damage > 0 {
                            target_took_damage = true;
                        }
                    }
                    WireEvent::CastStarted { actor: a, .. }
                    | WireEvent::CastCompleted { actor: a, .. }
                    | WireEvent::Healed { actor: a, .. }
                        if usize::from(a) == actor =>
                    {
                        saw_cast_effect = true;
                    }
                    _ => {}
                }
            }
            // `snapshot.fighters` is post-Phase-4c: a target killed on this tick had
            // `pre_combat.health > 0` and emitted a `Hit` with `damage > 0`.
            let server_target_was_alive = other.health > 0 || target_took_damage;
            if cast.client_in_range && !server_in_range {
                stats.geom_out_of_range_disagreements += 1;
            }
            if cast.client_has_los && !server_has_los {
                stats.geom_los_disagreements += 1;
            }
            if cast.client_target_alive && !server_target_was_alive {
                stats.geom_target_dead_disagreements += 1;
            }
            let (accepted, reached_geometry) = match cast_rejection {
                Some(WireRejection::OutOfRange) => {
                    stats.server_rejected_out_of_range += 1;
                    if cast.client_in_range {
                        stats.disagreed_out_of_range += 1;
                    }
                    (false, true)
                }
                Some(WireRejection::LineOfSight) => {
                    stats.server_rejected_los += 1;
                    if cast.client_has_los {
                        stats.disagreed_los += 1;
                    }
                    (false, true)
                }
                Some(WireRejection::NotEnoughResource) => {
                    // Checked after `check_target_geometry` in `arena-sim::casting`.
                    (false, true)
                }
                Some(WireRejection::InvalidTarget) => {
                    stats.server_rejected_invalid_target += 1;
                    if cast.client_target_alive {
                        stats.disagreed_target_dead += 1;
                    }
                    (false, false)
                }
                Some(_) => (false, false),
                None => (saw_cast_effect, saw_cast_effect),
            };
            if reached_geometry {
                stats.casts_reached_geometry += 1;
                if cast.client_in_range {
                    stats.casts_reached_geometry_in_range += 1;
                }
                if cast.client_has_los {
                    stats.casts_reached_geometry_has_los += 1;
                }
            }
            if accepted {
                stats.casts_accepted += 1;
                if !server_in_range || !server_has_los {
                    stats.casts_saved_by_lag_comp += 1;
                }
            }
        }

        if snapshot.result.is_some() {
            self.pending.clear();
        }
    }
}

fn pos(fighter: &FighterState) -> Position {
    Position {
        x: fighter.x,
        z: fighter.z,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_protocol::{TickEvent, WireEvent, WireRejection};

    fn fighter(x: i32, z: i32, health: u16) -> FighterState {
        FighterState {
            x,
            z,
            health,
            connected: true,
            ..FighterState::default()
        }
    }

    fn snap(tick: u64, ack: u32, fighters: Vec<FighterState>, events: Vec<TickEvent>) -> Snapshot {
        Snapshot {
            tick,
            ack,
            queued: 0,
            hash: 0,
            result: None,
            fighters,
            scoreboard: None,
            events,
        }
    }

    #[test]
    fn interp_delay_ticks_rounds_to_nearest_tick() {
        assert_eq!(interp_delay_ticks(0, 30), 0);
        assert_eq!(interp_delay_ticks(100, 30), 3);
        assert_eq!(interp_delay_ticks(150, 30), 5);
        assert_eq!(interp_delay_ticks(33, 30), 1);
    }

    #[test]
    fn view_buffer_delays_remote_fighters_while_overlaying_predicted_self() {
        let mut buf = ViewBuffer::default();
        for tick in 1..=6u64 {
            buf.push(snap(
                tick,
                tick as u32,
                vec![
                    fighter(0, -(tick as i32) * 240, 100),
                    fighter(3_000, -(tick as i32) * 240, 100),
                ],
                Vec::new(),
            ));
        }
        let mut predicted = Combatant::new(Position { x: 100, z: -2_000 }, 1234);
        predicted.health = 50; // Ignored: combat state stays authoritative from `latest`.

        let decision = buf.decision_view(0, Some(predicted), 3).unwrap();
        let view = decision.snapshot;
        assert_eq!(decision.remote_tick, 3);
        assert_eq!(view.tick, 6);
        // Self uses predicted movement + authoritative latest health.
        assert_eq!((view.fighters[0].x, view.fighters[0].z), (100, -2_000));
        assert_eq!(view.fighters[0].facing, 1234);
        assert_eq!(view.fighters[0].health, 100);
        // Remote fighter comes from tick `6 - 3 = 3`.
        assert_eq!(view.fighters[1].z, -3 * 240);
    }

    #[test]
    fn fairness_tracker_distinguishes_disagreements_from_agreed_rejections_and_folds() {
        let layout = arena_map::colosseum().layout;
        let mut tracker = FairnessTracker::default();
        let mut stats = FairnessStats::default();

        let cast = |seq, spell| CastSubmission {
            seq,
            actor: 0,
            spell,
            target: 1,
        };

        // Cast 1: client sees enemy at 5.8 m (in Pulse 6.0 m range, clear LoS).
        // Server executes at 6.3 m and rejects `OutOfRange` -> OutOfRange disagreement!
        let client_view = snap(
            10,
            0,
            vec![fighter(0, 0, 100), fighter(5_800, 0, 100)],
            Vec::new(),
        );
        tracker.record_cast(&mut stats, cast(1, SpellId::Pulse), &client_view, &layout);
        let server_snap_1 = snap(
            14,
            1,
            vec![fighter(0, 0, 100), fighter(6_300, 0, 100)],
            vec![TickEvent {
                tick: 13,
                event: WireEvent::Rejected(WireRejection::OutOfRange),
            }],
        );
        tracker.on_snapshot(&mut stats, 0, &server_snap_1, &layout);

        // Cast 2: client sees enemy behind a colosseum pillar corner (in RiftBolt 12 m
        // range, no LoS on client too). Server rejects `LineOfSight` -> agreed, not disagreed.
        let pillar = layout.pillars[0];
        let left = Position {
            x: pillar.center.x + 2_000,
            z: pillar.center.z + pillar.half_z + 1_000,
        };
        let right = Position {
            x: pillar.center.x + pillar.half_x + 1_000,
            z: pillar.center.z - 500,
        };
        assert!(!layout.line_of_sight(left, right));
        assert!(left.distance_squared(right) <= 12_000i64.pow(2));
        let behind_pillar = snap(
            15,
            1,
            vec![fighter(left.x, left.z, 100), fighter(right.x, right.z, 100)],
            Vec::new(),
        );
        tracker.record_cast(
            &mut stats,
            cast(2, SpellId::RiftBolt),
            &behind_pillar,
            &layout,
        );
        let server_snap_2 = snap(
            18,
            2,
            vec![fighter(left.x, left.z, 100), fighter(right.x, right.z, 100)],
            vec![TickEvent {
                tick: 17,
                event: WireEvent::Rejected(WireRejection::LineOfSight),
            }],
        );
        tracker.on_snapshot(&mut stats, 0, &server_snap_2, &layout);

        // Cast 3: client sees clear LoS and in range, server sees enemy ducked behind
        // pillar corner (still in 12 m range) and rejects `LineOfSight` -> LoS disagreement!
        let clear = Position {
            x: right.x,
            z: left.z,
        };
        assert!(layout.line_of_sight(left, clear));
        assert!(left.distance_squared(clear) <= 12_000i64.pow(2));
        let clear_view = snap(
            19,
            2,
            vec![fighter(left.x, left.z, 100), fighter(clear.x, clear.z, 100)],
            Vec::new(),
        );
        tracker.record_cast(&mut stats, cast(3, SpellId::RiftBolt), &clear_view, &layout);
        let server_snap_3 = snap(
            22,
            3,
            vec![fighter(left.x, left.z, 100), fighter(right.x, right.z, 100)],
            vec![TickEvent {
                tick: 21,
                event: WireEvent::Rejected(WireRejection::LineOfSight),
            }],
        );
        tracker.on_snapshot(&mut stats, 0, &server_snap_3, &layout);

        // Cast 4: folded (`ack` jumps from 3 to 5) must NOT misattribute seq 5's rejection.
        tracker.record_cast(&mut stats, cast(4, SpellId::Pulse), &client_view, &layout);
        let folded_snap = snap(
            24,
            5,
            vec![fighter(0, 0, 100), fighter(9_000, 0, 100)],
            vec![TickEvent {
                tick: 23,
                event: WireEvent::Rejected(WireRejection::OutOfRange),
            }],
        );
        tracker.on_snapshot(&mut stats, 0, &folded_snap, &layout);

        // Cast 5 after reconnect (seq restarts at 1) resolves as accepted Hit.
        tracker.reset_connection();
        let hit_view = snap(
            25,
            0,
            vec![fighter(0, 0, 100), fighter(4_000, 0, 100)],
            Vec::new(),
        );
        tracker.record_cast(&mut stats, cast(1, SpellId::Pulse), &hit_view, &layout);
        let hit_snap = snap(
            28,
            1,
            vec![fighter(0, 0, 100), fighter(4_000, 0, 75)],
            vec![TickEvent {
                tick: 27,
                event: WireEvent::Hit {
                    actor: 0,
                    target: 1,
                    damage: 25,
                },
            }],
        );
        tracker.on_snapshot(&mut stats, 0, &hit_snap, &layout);

        // Cast 6: rejected by GlobalCooldown before geometry check must NOT dilute
        // `casts_reached_geometry_in_range` / `out_of_range_disagreement_rate`.
        tracker.record_cast(&mut stats, cast(2, SpellId::Pulse), &hit_view, &layout);
        let gcd_snap = snap(
            29,
            2,
            vec![fighter(0, 0, 100), fighter(4_000, 0, 75)],
            vec![TickEvent {
                tick: 28,
                event: WireEvent::Rejected(WireRejection::GlobalCooldown),
            }],
        );
        tracker.on_snapshot(&mut stats, 0, &gcd_snap, &layout);

        let report = stats.summarize();
        assert_eq!(report.casts_sent, 6);
        assert_eq!(report.casts_resolved, 5);
        assert_eq!(report.casts_folded_or_skipped, 1);
        assert_eq!(report.casts_accepted, 1);
        assert_eq!(report.casts_reached_geometry, 4);
        assert_eq!(report.casts_reached_geometry_in_range, 4);
        assert_eq!(report.casts_reached_geometry_has_los, 3);
        assert_eq!(report.server_rejected_out_of_range, 1);
        assert_eq!(report.disagreed_out_of_range, 1);
        assert!((report.out_of_range_disagreement_rate - 0.25).abs() < 1e-12);
        assert_eq!(report.geom_out_of_range_disagreements, 1);
        assert_eq!(report.server_rejected_los, 2);
        assert_eq!(report.disagreed_los, 1);
        assert!((report.los_disagreement_rate - (1.0 / 3.0)).abs() < 1e-12);
        assert_eq!(report.geom_los_disagreements, 1);
        assert_eq!(report.distance_error_mm[2], 4013);
        assert_eq!(report.receding_error_mm[2], 4013);
    }

    #[test]
    fn killing_blow_is_not_counted_as_target_dead_disagreement() {
        let layout = arena_map::colosseum().layout;
        let mut tracker = FairnessTracker::default();
        let mut stats = FairnessStats::default();
        let view = snap(
            10,
            0,
            vec![fighter(0, 0, 100), fighter(3_000, 0, 25)],
            Vec::new(),
        );
        tracker.record_cast(
            &mut stats,
            CastSubmission {
                seq: 1,
                actor: 0,
                spell: SpellId::Pulse,
                target: 1,
            },
            &view,
            &layout,
        );
        // Post-tick target health is 0 because this Pulse killed them.
        let kill_snap = snap(
            12,
            1,
            vec![fighter(0, 0, 100), fighter(3_000, 0, 0)],
            vec![TickEvent {
                tick: 11,
                event: WireEvent::Hit {
                    actor: 0,
                    target: 1,
                    damage: 25,
                },
            }],
        );
        tracker.on_snapshot(&mut stats, 0, &kill_snap, &layout);

        // A follow-up cast sent while client still saw alive against an already-dead target
        // (no Hit on this tick) IS counted as a target-dead disagreement.
        tracker.record_cast(
            &mut stats,
            CastSubmission {
                seq: 2,
                actor: 0,
                spell: SpellId::Pulse,
                target: 1,
            },
            &view,
            &layout,
        );
        let dead_snap = snap(
            13,
            2,
            vec![fighter(0, 0, 100), fighter(3_000, 0, 0)],
            vec![TickEvent {
                tick: 12,
                event: WireEvent::Rejected(WireRejection::InvalidTarget),
            }],
        );
        tracker.on_snapshot(&mut stats, 0, &dead_snap, &layout);

        let report = stats.summarize();
        assert_eq!(report.casts_accepted, 1);
        assert_eq!(report.geom_target_dead_disagreements, 1);
        assert_eq!(report.disagreed_target_dead, 1);
    }
}
