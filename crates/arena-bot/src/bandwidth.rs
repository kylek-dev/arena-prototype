//! Online snapshot bandwidth evaluator for full 30 Hz snapshots vs lossless
//! field-mask + timer-decay deltas and lower snapshot rates (20 Hz and 15 Hz).
//!
//! Operates in O(1) memory per seat so it never perturbs large load runs while
//! producing exact `postcard` wire-byte comparisons on real combat traffic.

use arena_protocol::delta::{FieldChangeCounts, SnapshotDelta, full_snapshot_encoded_len};
use arena_protocol::{MAX_EVENTS, Snapshot, TickEvent};
use serde::Serialize;

/// Per-seat O(1)-memory accumulator of snapshot bandwidth metrics.
#[derive(Debug, Default)]
pub struct BandwidthStats {
    pub snapshots_30hz: u64,
    pub keyframes_30hz: u64,
    pub deltas_30hz: u64,
    pub snapshots_with_events: u64,
    pub total_events: u64,
    pub full_30hz_bytes: u64,
    pub full_30hz_min_bytes: u32,
    pub full_30hz_max_bytes: u32,
    pub delta_30hz_bytes: u64,
    pub delta_30hz_no_hash_bytes: u64,
    pub delta_30hz_max_bytes: u32,
    pub snapshots_20hz: u64,
    pub full_20hz_bytes: u64,
    pub delta_20hz_bytes: u64,
    pub snapshots_15hz: u64,
    pub full_15hz_bytes: u64,
    pub delta_15hz_bytes: u64,
    pub field_changes: FieldChangeCounts,
}

impl BandwidthStats {
    pub fn merge(&mut self, other: Self) {
        self.snapshots_30hz += other.snapshots_30hz;
        self.keyframes_30hz += other.keyframes_30hz;
        self.deltas_30hz += other.deltas_30hz;
        self.snapshots_with_events += other.snapshots_with_events;
        self.total_events += other.total_events;
        self.full_30hz_bytes += other.full_30hz_bytes;
        if other.snapshots_30hz > 0 {
            self.full_30hz_min_bytes = if self.full_30hz_min_bytes == 0 {
                other.full_30hz_min_bytes
            } else {
                self.full_30hz_min_bytes.min(other.full_30hz_min_bytes)
            };
        }
        self.full_30hz_max_bytes = self.full_30hz_max_bytes.max(other.full_30hz_max_bytes);
        self.delta_30hz_bytes += other.delta_30hz_bytes;
        self.delta_30hz_no_hash_bytes += other.delta_30hz_no_hash_bytes;
        self.delta_30hz_max_bytes = self.delta_30hz_max_bytes.max(other.delta_30hz_max_bytes);
        self.snapshots_20hz += other.snapshots_20hz;
        self.full_20hz_bytes += other.full_20hz_bytes;
        self.delta_20hz_bytes += other.delta_20hz_bytes;
        self.snapshots_15hz += other.snapshots_15hz;
        self.full_15hz_bytes += other.full_15hz_bytes;
        self.delta_15hz_bytes += other.delta_15hz_bytes;
        self.field_changes.merge(other.field_changes);
    }

    /// Summarizes steady-state active-combat bandwidth at `tick_rate` Hz.
    /// Dividing by active stream player-seconds (`snapshots_30hz / tick_rate`)
    /// keeps `full_30hz_kbit_per_s_per_player` exact (`mean_bytes * tick_rate * 8 / 1000`)
    /// regardless of waiting, post-round linger, or reconnect away gaps.
    pub fn summarize(self, tick_rate: u32) -> BandwidthReport {
        let rate = f64::from(tick_rate.max(1));
        let active_player_seconds = (self.snapshots_30hz as f64 / rate).max(1e-9);
        let kbit_s = |bytes: u64| {
            if self.snapshots_30hz == 0 {
                0.0
            } else {
                bytes as f64 * 8.0 / 1000.0 / active_player_seconds
            }
        };
        let mean = |bytes: u64, count: u64| {
            if count == 0 {
                0.0
            } else {
                bytes as f64 / count as f64
            }
        };
        let saving_pct = |candidate: u64, baseline: u64| {
            if baseline == 0 {
                0.0
            } else {
                (1.0 - candidate as f64 / baseline as f64) * 100.0
            }
        };
        let field_rate = |count: u64| {
            if self.field_changes.fighter_ticks == 0 {
                0.0
            } else {
                count as f64 / self.field_changes.fighter_ticks as f64
            }
        };

        BandwidthReport {
            snapshots_30hz: self.snapshots_30hz,
            keyframes_30hz: self.keyframes_30hz,
            deltas_30hz: self.deltas_30hz,
            event_snapshot_rate: if self.snapshots_30hz == 0 {
                0.0
            } else {
                self.snapshots_with_events as f64 / self.snapshots_30hz as f64
            },
            mean_events_per_snapshot: mean(self.total_events, self.snapshots_30hz),
            full_30hz_mean_bytes: mean(self.full_30hz_bytes, self.snapshots_30hz),
            full_30hz_min_bytes: self.full_30hz_min_bytes,
            full_30hz_max_bytes: self.full_30hz_max_bytes,
            full_30hz_kbit_per_s_per_player: kbit_s(self.full_30hz_bytes),
            delta_30hz_mean_bytes: mean(self.delta_30hz_bytes, self.snapshots_30hz),
            delta_30hz_max_bytes: self.delta_30hz_max_bytes,
            delta_30hz_kbit_per_s_per_player: kbit_s(self.delta_30hz_bytes),
            delta_30hz_savings_pct: saving_pct(self.delta_30hz_bytes, self.full_30hz_bytes),
            delta_30hz_no_hash_kbit_per_s_per_player: kbit_s(self.delta_30hz_no_hash_bytes),
            delta_30hz_no_hash_savings_pct: saving_pct(
                self.delta_30hz_no_hash_bytes,
                self.full_30hz_bytes,
            ),
            full_20hz_kbit_per_s_per_player: kbit_s(self.full_20hz_bytes),
            full_20hz_savings_pct: saving_pct(self.full_20hz_bytes, self.full_30hz_bytes),
            delta_20hz_kbit_per_s_per_player: kbit_s(self.delta_20hz_bytes),
            delta_20hz_savings_pct: saving_pct(self.delta_20hz_bytes, self.full_30hz_bytes),
            full_15hz_kbit_per_s_per_player: kbit_s(self.full_15hz_bytes),
            full_15hz_savings_pct: saving_pct(self.full_15hz_bytes, self.full_30hz_bytes),
            delta_15hz_kbit_per_s_per_player: kbit_s(self.delta_15hz_bytes),
            delta_15hz_savings_pct: saving_pct(self.delta_15hz_bytes, self.full_30hz_bytes),
            field_change_rates: FieldChangeRates {
                fighter_ticks: self.field_changes.fighter_ticks,
                position: field_rate(self.field_changes.position_changed),
                facing: field_rate(self.field_changes.facing_changed),
                vertical_or_momentum: field_rate(self.field_changes.vertical_or_momentum_changed),
                health: field_rate(self.field_changes.health_changed),
                resource: field_rate(self.field_changes.resource_changed),
                timers_raw: field_rate(self.field_changes.timers_raw_changed),
                timers_unexpected: field_rate(self.field_changes.timers_unexpected_changed),
                active_cast_raw: field_rate(self.field_changes.active_cast_raw_changed),
                active_cast_unexpected: field_rate(
                    self.field_changes.active_cast_unexpected_changed,
                ),
                connected: field_rate(self.field_changes.connected_changed),
            },
        }
    }
}

/// Fraction of fighter-ticks where each field changed between consecutive ticks.
#[derive(Clone, Debug, Default, Serialize)]
pub struct FieldChangeRates {
    pub fighter_ticks: u64,
    pub position: f64,
    pub facing: f64,
    pub vertical_or_momentum: f64,
    pub health: f64,
    pub resource: f64,
    pub timers_raw: f64,
    pub timers_unexpected: f64,
    pub active_cast_raw: f64,
    pub active_cast_unexpected: f64,
    pub connected: f64,
}

/// Serializable snapshot bandwidth comparison report included in `arena_bot::Report`.
#[derive(Clone, Debug, Default, Serialize)]
pub struct BandwidthReport {
    pub snapshots_30hz: u64,
    pub keyframes_30hz: u64,
    pub deltas_30hz: u64,
    pub event_snapshot_rate: f64,
    pub mean_events_per_snapshot: f64,
    pub full_30hz_mean_bytes: f64,
    pub full_30hz_min_bytes: u32,
    pub full_30hz_max_bytes: u32,
    pub full_30hz_kbit_per_s_per_player: f64,
    pub delta_30hz_mean_bytes: f64,
    pub delta_30hz_max_bytes: u32,
    pub delta_30hz_kbit_per_s_per_player: f64,
    pub delta_30hz_savings_pct: f64,
    pub delta_30hz_no_hash_kbit_per_s_per_player: f64,
    pub delta_30hz_no_hash_savings_pct: f64,
    pub full_20hz_kbit_per_s_per_player: f64,
    pub full_20hz_savings_pct: f64,
    pub delta_20hz_kbit_per_s_per_player: f64,
    pub delta_20hz_savings_pct: f64,
    pub full_15hz_kbit_per_s_per_player: f64,
    pub full_15hz_savings_pct: f64,
    pub delta_15hz_kbit_per_s_per_player: f64,
    pub delta_15hz_savings_pct: f64,
    pub field_change_rates: FieldChangeRates,
}

/// Per-seat stateful stream tracker computing exact full vs delta vs decimated
/// snapshot sizes. Resets cleanly on reconnects so post-reconnect snapshots
/// count honestly as full keyframes.
#[derive(Debug, Default)]
pub struct BandwidthTracker {
    prev_30hz: Option<Snapshot>,
    prev_20hz: Option<Snapshot>,
    pending_events_20hz: Vec<TickEvent>,
    step_20hz: u64,
    prev_15hz: Option<Snapshot>,
    pending_events_15hz: Vec<TickEvent>,
    step_15hz: u64,
}

impl BandwidthTracker {
    pub fn reset_connection(&mut self) {
        self.prev_30hz = None;
        self.prev_20hz = None;
        self.pending_events_20hz.clear();
        self.step_20hz = 0;
        self.prev_15hz = None;
        self.pending_events_15hz.clear();
        self.step_15hz = 0;
    }

    pub fn observe(&mut self, stats: &mut BandwidthStats, snapshot: &Snapshot, raw_len: usize) {
        // Measure running-match snapshots only (tick == 0 waiting frames are idle).
        if snapshot.tick == 0 {
            return;
        }
        let raw_u64 = raw_len as u64;
        let raw_u32 = u32::try_from(raw_len).unwrap_or(u32::MAX);
        stats.snapshots_30hz += 1;
        stats.full_30hz_bytes += raw_u64;
        stats.full_30hz_min_bytes = if stats.full_30hz_min_bytes == 0 {
            raw_u32
        } else {
            stats.full_30hz_min_bytes.min(raw_u32)
        };
        stats.full_30hz_max_bytes = stats.full_30hz_max_bytes.max(raw_u32);
        if !snapshot.events.is_empty() {
            stats.snapshots_with_events += 1;
            stats.total_events += snapshot.events.len() as u64;
        }

        // 30 Hz lossless delta (keyframe on first snapshot, reconnect, or tick rewind).
        if let Some(prev) = &self.prev_30hz
            && let Some((delta, counts)) = SnapshotDelta::from_pair(prev, snapshot)
        {
            let size = delta.encoded_size();
            stats.deltas_30hz += 1;
            stats.delta_30hz_bytes += size.total_bytes as u64;
            stats.delta_30hz_no_hash_bytes +=
                size.total_bytes.saturating_sub(size.hash_bytes) as u64;
            stats.delta_30hz_max_bytes = stats
                .delta_30hz_max_bytes
                .max(u32::try_from(size.total_bytes).unwrap_or(u32::MAX));
            stats.field_changes.merge(counts);
        } else {
            stats.keyframes_30hz += 1;
            stats.delta_30hz_bytes += raw_u64;
            stats.delta_30hz_no_hash_bytes += raw_u64;
            stats.delta_30hz_max_bytes = stats.delta_30hz_max_bytes.max(raw_u32);
        }
        self.prev_30hz = Some(snapshot.clone());

        // 20 Hz (transmit 2 of every 3 ticks; always transmit terminal result snapshots).
        self.step_20hz += 1;
        let emit_20hz =
            self.step_20hz == 1 || !self.step_20hz.is_multiple_of(3) || snapshot.result.is_some();
        if emit_20hz {
            let had_pending = !self.pending_events_20hz.is_empty();
            let carried = build_carried_snapshot(snapshot, &mut self.pending_events_20hz);
            let full_len = if !had_pending {
                raw_len
            } else {
                full_snapshot_encoded_len(&carried)
            };
            let delta_len = self
                .prev_20hz
                .as_ref()
                .and_then(|prev| SnapshotDelta::from_pair(prev, &carried))
                .map_or(full_len, |(delta, _)| delta.encoded_size().total_bytes);
            stats.snapshots_20hz += 1;
            stats.full_20hz_bytes += full_len as u64;
            stats.delta_20hz_bytes += delta_len as u64;
            self.prev_20hz = Some(carried);
        } else {
            extend_capped(&mut self.pending_events_20hz, &snapshot.events);
        }

        // 15 Hz (strict 2-tick stride starting from step 1; always transmit terminal result snapshots).
        self.step_15hz += 1;
        let emit_15hz = !self.step_15hz.is_multiple_of(2) || snapshot.result.is_some();
        if emit_15hz {
            let had_pending = !self.pending_events_15hz.is_empty();
            let carried = build_carried_snapshot(snapshot, &mut self.pending_events_15hz);
            let full_len = if !had_pending {
                raw_len
            } else {
                full_snapshot_encoded_len(&carried)
            };
            let delta_len = self
                .prev_15hz
                .as_ref()
                .and_then(|prev| SnapshotDelta::from_pair(prev, &carried))
                .map_or(full_len, |(delta, _)| delta.encoded_size().total_bytes);
            stats.snapshots_15hz += 1;
            stats.full_15hz_bytes += full_len as u64;
            stats.delta_15hz_bytes += delta_len as u64;
            self.prev_15hz = Some(carried);
        } else {
            extend_capped(&mut self.pending_events_15hz, &snapshot.events);
        }
    }
}

fn extend_capped(dst: &mut Vec<TickEvent>, src: &[TickEvent]) {
    let remaining = MAX_EVENTS.saturating_sub(dst.len());
    dst.extend(src.iter().copied().take(remaining));
}

fn build_carried_snapshot(snapshot: &Snapshot, pending: &mut Vec<TickEvent>) -> Snapshot {
    if pending.is_empty() {
        return snapshot.clone();
    }
    let mut events = std::mem::take(pending);
    extend_capped(&mut events, &snapshot.events);
    Snapshot {
        events,
        ..snapshot.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_protocol::{ServerMessage, WireEvent};
    use arena_sim::arena::{ActorInput, Arena, Axes};

    #[test]
    fn tracker_measures_full_delta_and_decimated_rates_across_reconnects() {
        let layout = arena_map::colosseum().layout;
        let mut arena = Arena::practice(layout, 3).unwrap();
        let mut tracker = BandwidthTracker::default();
        let mut stats = BandwidthStats::default();

        for step in 1u32..=30 {
            if step == 16 {
                tracker.reset_connection();
            }
            let mut slots = [ActorInput::default(); 6];
            slots[0].movement = Some(Axes { x: 1000, z: 0 });
            slots[1].movement = Some(Axes { x: -1000, z: 0 });
            if step % 10 == 0 {
                slots[0].jump = true;
            }
            let tick = arena.tick();
            let events: Vec<TickEvent> = arena
                .advance(&slots)
                .iter()
                .filter_map(|e| WireEvent::for_recipient(e, 0))
                .map(|event| TickEvent { tick, event })
                .collect();
            let snap = Snapshot::capture(&arena, step, events, &[true; 6]);
            let frame_len = ServerMessage::Snapshot(snap.clone()).encode().len();
            tracker.observe(&mut stats, &snap, frame_len);
        }

        // First snapshot + post-reconnect snapshot (step 16) are both keyframes.
        assert_eq!(stats.snapshots_30hz, 30);
        assert_eq!(stats.keyframes_30hz, 2);
        assert_eq!(stats.deltas_30hz, 28);
        assert!(stats.delta_30hz_bytes < stats.full_30hz_bytes);
        assert!(stats.delta_30hz_no_hash_bytes < stats.delta_30hz_bytes);
        assert!(stats.full_15hz_bytes < stats.full_20hz_bytes);
        assert!(stats.full_20hz_bytes < stats.full_30hz_bytes);
        assert!(stats.delta_15hz_bytes < stats.delta_30hz_bytes);

        let report = stats.summarize(30);
        // Active-combat kbit/s must always match exact `mean_bytes * tick_rate * 8 / 1000`.
        let expected_full_kbit = report.full_30hz_mean_bytes * 30.0 * 8.0 / 1000.0;
        assert!(
            (report.full_30hz_kbit_per_s_per_player - expected_full_kbit).abs() < 1e-9,
            "{} vs {expected_full_kbit}",
            report.full_30hz_kbit_per_s_per_player
        );
        let expected_delta_kbit = report.delta_30hz_mean_bytes * 30.0 * 8.0 / 1000.0;
        assert!(
            (report.delta_30hz_kbit_per_s_per_player - expected_delta_kbit).abs() < 1e-9,
            "{} vs {expected_delta_kbit}",
            report.delta_30hz_kbit_per_s_per_player
        );
        assert!(report.delta_30hz_savings_pct > 20.0);
        assert!(report.full_15hz_savings_pct > 40.0);
        assert!(report.field_change_rates.position > 0.0);
    }
}
