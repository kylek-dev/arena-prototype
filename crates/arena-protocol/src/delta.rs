//! Offline/diagnostic lossless delta codec for [`Snapshot`] streams.
//!
//! This module does **not** alter [`crate::PROTOCOL_VERSION`] or any live wire
//! message. It provides a lossless, round-trip-verified field-mask +
//! timer-decay-predicted delta representation so `arena-bot` can measure exact
//! `postcard` bandwidth savings of delta encoding and lower snapshot rates on
//! live combat traffic before deciding whether to change the wire protocol.

use arena_sim::spells::SpellId;
use serde::{Deserialize, Serialize};

use crate::{
    DecodeError, FighterScore, FighterState, MAX_EVENTS, MAX_FIGHTERS, MAX_SERVER_FRAME, Snapshot,
    TickEvent, WireCast, WireResult, valid_snapshot,
};

/// Bitmask flags for [`FighterDelta`]. Most frequently changing fields occupy
/// bits 0..=6 so ordinary ground-combat ticks encode the mask as 1 varint byte.
pub const MASK_DX: u16 = 1 << 0;
pub const MASK_DZ: u16 = 1 << 1;
pub const MASK_DFACING: u16 = 1 << 2;
pub const MASK_RESOURCE: u16 = 1 << 3;
pub const MASK_HEALTH: u16 = 1 << 4;
pub const MASK_GCD: u16 = 1 << 5;
pub const MASK_COOLDOWNS: u16 = 1 << 6;
pub const MASK_ACTIVE_CAST: u16 = 1 << 7;
pub const MASK_DHEIGHT: u16 = 1 << 8;
pub const MASK_DVEL: u16 = 1 << 9;
pub const MASK_DMOMENTUM: u16 = 1 << 10;
pub const MASK_LOCKOUT: u16 = 1 << 11;
pub const MASK_CONNECTED_FLIP: u16 = 1 << 12;

/// Per-field change counters accumulated across consecutive snapshots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldChangeCounts {
    /// Total fighter-ticks compared against a prior snapshot.
    pub fighter_ticks: u64,
    pub position_changed: u64,
    pub facing_changed: u64,
    pub vertical_or_momentum_changed: u64,
    pub health_changed: u64,
    pub resource_changed: u64,
    /// Fighter-ticks where any timer (GCD/lockout/cooldown) changed raw value.
    pub timers_raw_changed: u64,
    /// Fighter-ticks where any timer deviated from normal `-tick_delta` decay.
    pub timers_unexpected_changed: u64,
    /// Fighter-ticks where `active_cast` changed raw value (including tickdown).
    pub active_cast_raw_changed: u64,
    /// Fighter-ticks where `active_cast` deviated from normal `-tick_delta` decay.
    pub active_cast_unexpected_changed: u64,
    pub connected_changed: u64,
}

impl FieldChangeCounts {
    pub fn merge(&mut self, other: Self) {
        self.fighter_ticks += other.fighter_ticks;
        self.position_changed += other.position_changed;
        self.facing_changed += other.facing_changed;
        self.vertical_or_momentum_changed += other.vertical_or_momentum_changed;
        self.health_changed += other.health_changed;
        self.resource_changed += other.resource_changed;
        self.timers_raw_changed += other.timers_raw_changed;
        self.timers_unexpected_changed += other.timers_unexpected_changed;
        self.active_cast_raw_changed += other.active_cast_raw_changed;
        self.active_cast_unexpected_changed += other.active_cast_unexpected_changed;
        self.connected_changed += other.connected_changed;
    }
}

/// Exact `postcard` byte-size breakdown of one encoded [`SnapshotDelta`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeltaSize {
    /// Total wire bytes of the `SnapshotDelta` frame (including `hash` and `events`).
    pub total_bytes: usize,
    /// Portion of `total_bytes` spent encoding the 64-bit state `hash`.
    pub hash_bytes: usize,
    /// Portion of `total_bytes` spent encoding `events` (including length prefix).
    pub events_bytes: usize,
}

/// Lossless delta for one fighter relative to `prev` after `tick_delta` ticks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FighterDelta {
    pub mask: u16,
    pub dx: i32,
    pub dz: i32,
    pub dfacing: i32,
    pub resource: u16,
    pub health: u16,
    pub gcd_remaining: u32,
    pub cooldown_mask: u8,
    pub cooldown_values: [u32; SpellId::COUNT],
    pub active_cast: Option<WireCast>,
    pub dheight: i32,
    pub dvel: i32,
    pub dmomentum: [i32; 2],
    pub lockout_remaining: u32,
}

/// Lossless delta for one [`Snapshot`] relative to a prior snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotDelta {
    pub tick_delta: u64,
    pub ack_delta: u32,
    pub queued: u8,
    pub hash: u64,
    pub result_changed: Option<Option<WireResult>>,
    pub scoreboard_changed: Option<Option<Vec<FighterScore>>>,
    pub fighters: Vec<FighterDelta>,
    pub events: Vec<TickEvent>,
}

/// Zero-allocation `postcard` serializer flavor that counts exact output bytes.
#[derive(Default)]
struct CountingFlavor {
    count: usize,
}

impl postcard::ser_flavors::Flavor for CountingFlavor {
    type Output = usize;

    fn try_push(&mut self, _data: u8) -> postcard::Result<()> {
        self.count += 1;
        Ok(())
    }

    fn try_extend(&mut self, data: &[u8]) -> postcard::Result<()> {
        self.count += data.len();
        Ok(())
    }

    fn finalize(self) -> postcard::Result<Self::Output> {
        Ok(self.count)
    }
}

fn postcard_len<T: Serialize + ?Sized>(value: &T) -> usize {
    postcard::serialize_with_flavor(value, CountingFlavor::default())
        .expect("counting flavor never fails")
}

/// Expected timer value after `tick_delta` ticks of normal countdown decay.
fn expected_timer(prev: u32, tick_delta: u64) -> u32 {
    prev.saturating_sub(u32::try_from(tick_delta).unwrap_or(u32::MAX))
}

/// Expected `active_cast` after `tick_delta` ticks if uninterrupted.
fn expected_cast(prev: Option<WireCast>, tick_delta: u64) -> Option<WireCast> {
    let cast = prev?;
    let dt = u32::try_from(tick_delta).unwrap_or(u32::MAX);
    let remaining = cast.remaining_ticks.saturating_sub(dt);
    (remaining > 0).then_some(WireCast {
        remaining_ticks: remaining,
        ..cast
    })
}

impl SnapshotDelta {
    /// Builds a lossless delta from `prev` to `curr` and returns per-field
    /// change counts. Returns `None` if `curr.tick < prev.tick`, the roster
    /// size changed, or the fighter count is outside `2..=MAX_FIGHTERS` (or odd)
    /// — caller must send a full keyframe instead.
    pub fn from_pair(prev: &Snapshot, curr: &Snapshot) -> Option<(Self, FieldChangeCounts)> {
        let count = curr.fighters.len();
        if curr.tick < prev.tick
            || count != prev.fighters.len()
            || !(2..=MAX_FIGHTERS).contains(&count)
            || !count.is_multiple_of(2)
        {
            return None;
        }
        let tick_delta = curr.tick - prev.tick;
        let ack_delta = curr.ack.wrapping_sub(prev.ack);
        let result_changed = (curr.result != prev.result).then_some(curr.result);
        let scoreboard_changed =
            (curr.scoreboard != prev.scoreboard).then(|| curr.scoreboard.clone());

        let mut counts = FieldChangeCounts::default();
        let mut fighters = Vec::with_capacity(curr.fighters.len());
        for (p, c) in prev.fighters.iter().zip(&curr.fighters) {
            counts.fighter_ticks += 1;
            let mut d = FighterDelta::default();
            if c.x != p.x {
                d.mask |= MASK_DX;
                d.dx = c.x.wrapping_sub(p.x);
            }
            if c.z != p.z {
                d.mask |= MASK_DZ;
                d.dz = c.z.wrapping_sub(p.z);
            }
            if c.x != p.x || c.z != p.z {
                counts.position_changed += 1;
            }
            if c.facing != p.facing {
                d.mask |= MASK_DFACING;
                // Shortest signed wrap-around delta in `[-32768, 32767]`.
                d.dfacing = i32::from(c.facing.wrapping_sub(p.facing) as i16);
                counts.facing_changed += 1;
            }
            if c.resource != p.resource {
                d.mask |= MASK_RESOURCE;
                d.resource = c.resource;
                counts.resource_changed += 1;
            }
            if c.health != p.health {
                d.mask |= MASK_HEALTH;
                d.health = c.health;
                counts.health_changed += 1;
            }

            let raw_timer_changed = c.gcd_remaining != p.gcd_remaining
                || c.lockout_remaining != p.lockout_remaining
                || c.cooldowns != p.cooldowns;
            if raw_timer_changed {
                counts.timers_raw_changed += 1;
            }
            let mut unexpected_timer = false;
            if c.gcd_remaining != expected_timer(p.gcd_remaining, tick_delta) {
                d.mask |= MASK_GCD;
                d.gcd_remaining = c.gcd_remaining;
                unexpected_timer = true;
            }
            if c.lockout_remaining != expected_timer(p.lockout_remaining, tick_delta) {
                d.mask |= MASK_LOCKOUT;
                d.lockout_remaining = c.lockout_remaining;
                unexpected_timer = true;
            }
            let mut cd_mask = 0u8;
            for idx in 0..SpellId::COUNT {
                if c.cooldowns[idx] != expected_timer(p.cooldowns[idx], tick_delta) {
                    cd_mask |= 1u8 << idx;
                    d.cooldown_values[idx] = c.cooldowns[idx];
                }
            }
            if cd_mask != 0 {
                d.mask |= MASK_COOLDOWNS;
                d.cooldown_mask = cd_mask;
                unexpected_timer = true;
            }
            if unexpected_timer {
                counts.timers_unexpected_changed += 1;
            }

            if c.active_cast != p.active_cast {
                counts.active_cast_raw_changed += 1;
            }
            if c.active_cast != expected_cast(p.active_cast, tick_delta) {
                d.mask |= MASK_ACTIVE_CAST;
                d.active_cast = c.active_cast;
                counts.active_cast_unexpected_changed += 1;
            }

            let mut vert_or_mom = false;
            if c.height != p.height {
                d.mask |= MASK_DHEIGHT;
                d.dheight = c.height.wrapping_sub(p.height);
                vert_or_mom = true;
            }
            if c.vertical_velocity != p.vertical_velocity {
                d.mask |= MASK_DVEL;
                d.dvel = c.vertical_velocity.wrapping_sub(p.vertical_velocity);
                vert_or_mom = true;
            }
            if c.momentum != p.momentum {
                d.mask |= MASK_DMOMENTUM;
                d.dmomentum = [
                    c.momentum[0].wrapping_sub(p.momentum[0]),
                    c.momentum[1].wrapping_sub(p.momentum[1]),
                ];
                vert_or_mom = true;
            }
            if vert_or_mom {
                counts.vertical_or_momentum_changed += 1;
            }
            if c.connected != p.connected {
                d.mask |= MASK_CONNECTED_FLIP;
                counts.connected_changed += 1;
            }
            fighters.push(d);
        }

        Some((
            Self {
                tick_delta,
                ack_delta,
                queued: curr.queued,
                hash: curr.hash,
                result_changed,
                scoreboard_changed,
                fighters,
                events: curr.events.clone(),
            },
            counts,
        ))
    }

    /// Encodes this delta into compact `postcard` bytes with a leading message
    /// variant byte (`1u32`) matching `ServerMessage::Snapshot`'s framing cost.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + self.fighters.len() * 8 + self.events.len() * 10);
        out = postcard::to_extend(
            &(
                1u32,
                self.tick_delta,
                self.ack_delta,
                self.queued,
                self.hash,
                self.result_changed,
                &self.scoreboard_changed,
                self.fighters.len() as u8,
            ),
            out,
        )
        .expect("delta header always encodes");
        for f in &self.fighters {
            out = postcard::to_extend(&f.mask, out).expect("mask encodes");
            if f.mask & MASK_DX != 0 {
                out = postcard::to_extend(&f.dx, out).expect("dx encodes");
            }
            if f.mask & MASK_DZ != 0 {
                out = postcard::to_extend(&f.dz, out).expect("dz encodes");
            }
            if f.mask & MASK_DFACING != 0 {
                out = postcard::to_extend(&f.dfacing, out).expect("dfacing encodes");
            }
            if f.mask & MASK_RESOURCE != 0 {
                out = postcard::to_extend(&f.resource, out).expect("resource encodes");
            }
            if f.mask & MASK_HEALTH != 0 {
                out = postcard::to_extend(&f.health, out).expect("health encodes");
            }
            if f.mask & MASK_GCD != 0 {
                out = postcard::to_extend(&f.gcd_remaining, out).expect("gcd encodes");
            }
            if f.mask & MASK_COOLDOWNS != 0 {
                out = postcard::to_extend(&f.cooldown_mask, out).expect("cd_mask encodes");
                for idx in 0..SpellId::COUNT {
                    if f.cooldown_mask & (1u8 << idx) != 0 {
                        out = postcard::to_extend(&f.cooldown_values[idx], out)
                            .expect("cd value encodes");
                    }
                }
            }
            if f.mask & MASK_ACTIVE_CAST != 0 {
                out = postcard::to_extend(&f.active_cast, out).expect("cast encodes");
            }
            if f.mask & MASK_DHEIGHT != 0 {
                out = postcard::to_extend(&f.dheight, out).expect("dheight encodes");
            }
            if f.mask & MASK_DVEL != 0 {
                out = postcard::to_extend(&f.dvel, out).expect("dvel encodes");
            }
            if f.mask & MASK_DMOMENTUM != 0 {
                out = postcard::to_extend(&f.dmomentum, out).expect("dmomentum encodes");
            }
            if f.mask & MASK_LOCKOUT != 0 {
                out = postcard::to_extend(&f.lockout_remaining, out).expect("lockout encodes");
            }
        }
        postcard::to_extend(&self.events, out).expect("events encode")
    }

    /// Computes exact [`Self::encode`] wire size without allocating a frame buffer.
    pub fn encoded_size(&self) -> DeltaSize {
        let mut total = postcard_len(&(
            1u32,
            self.tick_delta,
            self.ack_delta,
            self.queued,
            self.hash,
            self.result_changed,
            &self.scoreboard_changed,
            self.fighters.len() as u8,
        ));
        let hash_bytes = postcard_len(&self.hash);
        for f in &self.fighters {
            total += postcard_len(&f.mask);
            if f.mask & MASK_DX != 0 {
                total += postcard_len(&f.dx);
            }
            if f.mask & MASK_DZ != 0 {
                total += postcard_len(&f.dz);
            }
            if f.mask & MASK_DFACING != 0 {
                total += postcard_len(&f.dfacing);
            }
            if f.mask & MASK_RESOURCE != 0 {
                total += postcard_len(&f.resource);
            }
            if f.mask & MASK_HEALTH != 0 {
                total += postcard_len(&f.health);
            }
            if f.mask & MASK_GCD != 0 {
                total += postcard_len(&f.gcd_remaining);
            }
            if f.mask & MASK_COOLDOWNS != 0 {
                total += postcard_len(&f.cooldown_mask);
                for idx in 0..SpellId::COUNT {
                    if f.cooldown_mask & (1u8 << idx) != 0 {
                        total += postcard_len(&f.cooldown_values[idx]);
                    }
                }
            }
            if f.mask & MASK_ACTIVE_CAST != 0 {
                total += postcard_len(&f.active_cast);
            }
            if f.mask & MASK_DHEIGHT != 0 {
                total += postcard_len(&f.dheight);
            }
            if f.mask & MASK_DVEL != 0 {
                total += postcard_len(&f.dvel);
            }
            if f.mask & MASK_DMOMENTUM != 0 {
                total += postcard_len(&f.dmomentum);
            }
            if f.mask & MASK_LOCKOUT != 0 {
                total += postcard_len(&f.lockout_remaining);
            }
        }
        let events_bytes = postcard_len(&self.events);
        total += events_bytes;
        DeltaSize {
            total_bytes: total,
            hash_bytes,
            events_bytes,
        }
    }

    /// Decodes a delta frame and reconstructs the exact [`Snapshot`] against
    /// `prev`, enforcing the same decode invariants as [`crate::ServerMessage::decode`].
    pub fn apply(prev: &Snapshot, frame: &[u8]) -> Result<Snapshot, DecodeError> {
        if frame.is_empty() {
            return Err(DecodeError::Empty);
        }
        if frame.len() > MAX_SERVER_FRAME {
            return Err(DecodeError::Oversized);
        }
        let (
            (
                variant,
                tick_delta,
                ack_delta,
                queued,
                hash,
                result_changed,
                scoreboard_changed,
                count,
            ),
            mut rest,
        ) = postcard::take_from_bytes::<(
            u32,
            u64,
            u32,
            u8,
            u64,
            Option<Option<WireResult>>,
            Option<Option<Vec<FighterScore>>>,
            u8,
        )>(frame)
        .map_err(|_| DecodeError::Malformed)?;
        if variant != 1 {
            return Err(DecodeError::Malformed);
        }
        let count = usize::from(count);
        if count != prev.fighters.len() || count > MAX_FIGHTERS {
            return Err(DecodeError::OverLimit);
        }
        let tick = prev
            .tick
            .checked_add(tick_delta)
            .ok_or(DecodeError::OverLimit)?;
        let ack = prev.ack.wrapping_add(ack_delta);
        let result = result_changed.unwrap_or(prev.result);
        let scoreboard = scoreboard_changed.unwrap_or_else(|| prev.scoreboard.clone());

        let mut fighters = Vec::with_capacity(count);
        for base in &prev.fighters {
            let (mask, next) =
                postcard::take_from_bytes::<u16>(rest).map_err(|_| DecodeError::Malformed)?;
            rest = next;
            // Reject unknown reserved mask bits.
            if mask & !((1u16 << 13) - 1) != 0 {
                return Err(DecodeError::Malformed);
            }
            let mut f = FighterState {
                gcd_remaining: expected_timer(base.gcd_remaining, tick_delta),
                lockout_remaining: expected_timer(base.lockout_remaining, tick_delta),
                cooldowns: std::array::from_fn(|i| expected_timer(base.cooldowns[i], tick_delta)),
                active_cast: expected_cast(base.active_cast, tick_delta),
                ..*base
            };
            if mask & MASK_DX != 0 {
                let (dx, next) =
                    postcard::take_from_bytes::<i32>(rest).map_err(|_| DecodeError::Malformed)?;
                rest = next;
                f.x = f.x.wrapping_add(dx);
            }
            if mask & MASK_DZ != 0 {
                let (dz, next) =
                    postcard::take_from_bytes::<i32>(rest).map_err(|_| DecodeError::Malformed)?;
                rest = next;
                f.z = f.z.wrapping_add(dz);
            }
            if mask & MASK_DFACING != 0 {
                let (dfacing, next) =
                    postcard::take_from_bytes::<i32>(rest).map_err(|_| DecodeError::Malformed)?;
                rest = next;
                if !(-32_768..=32_767).contains(&dfacing) {
                    return Err(DecodeError::Malformed);
                }
                f.facing = f.facing.wrapping_add(dfacing as i16 as u16);
            }
            if mask & MASK_RESOURCE != 0 {
                let (resource, next) =
                    postcard::take_from_bytes::<u16>(rest).map_err(|_| DecodeError::Malformed)?;
                rest = next;
                f.resource = resource;
            }
            if mask & MASK_HEALTH != 0 {
                let (health, next) =
                    postcard::take_from_bytes::<u16>(rest).map_err(|_| DecodeError::Malformed)?;
                rest = next;
                f.health = health;
            }
            if mask & MASK_GCD != 0 {
                let (gcd, next) =
                    postcard::take_from_bytes::<u32>(rest).map_err(|_| DecodeError::Malformed)?;
                rest = next;
                f.gcd_remaining = gcd;
            }
            if mask & MASK_COOLDOWNS != 0 {
                let (cd_mask, next) =
                    postcard::take_from_bytes::<u8>(rest).map_err(|_| DecodeError::Malformed)?;
                rest = next;
                if cd_mask & !((1u8 << SpellId::COUNT) - 1) != 0 {
                    return Err(DecodeError::Malformed);
                }
                for idx in 0..SpellId::COUNT {
                    if cd_mask & (1u8 << idx) != 0 {
                        let (cd, next) = postcard::take_from_bytes::<u32>(rest)
                            .map_err(|_| DecodeError::Malformed)?;
                        rest = next;
                        f.cooldowns[idx] = cd;
                    }
                }
            }
            if mask & MASK_ACTIVE_CAST != 0 {
                let (cast, next) = postcard::take_from_bytes::<Option<WireCast>>(rest)
                    .map_err(|_| DecodeError::Malformed)?;
                rest = next;
                f.active_cast = cast;
            }
            if mask & MASK_DHEIGHT != 0 {
                let (dheight, next) =
                    postcard::take_from_bytes::<i32>(rest).map_err(|_| DecodeError::Malformed)?;
                rest = next;
                f.height = f.height.wrapping_add(dheight);
            }
            if mask & MASK_DVEL != 0 {
                let (dvel, next) =
                    postcard::take_from_bytes::<i32>(rest).map_err(|_| DecodeError::Malformed)?;
                rest = next;
                f.vertical_velocity = f.vertical_velocity.wrapping_add(dvel);
            }
            if mask & MASK_DMOMENTUM != 0 {
                let (dmom, next) = postcard::take_from_bytes::<[i32; 2]>(rest)
                    .map_err(|_| DecodeError::Malformed)?;
                rest = next;
                f.momentum = [
                    f.momentum[0].wrapping_add(dmom[0]),
                    f.momentum[1].wrapping_add(dmom[1]),
                ];
            }
            if mask & MASK_LOCKOUT != 0 {
                let (lockout, next) =
                    postcard::take_from_bytes::<u32>(rest).map_err(|_| DecodeError::Malformed)?;
                rest = next;
                f.lockout_remaining = lockout;
            }
            if mask & MASK_CONNECTED_FLIP != 0 {
                f.connected = !f.connected;
            }
            fighters.push(f);
        }

        let (events, trailing) = postcard::take_from_bytes::<Vec<TickEvent>>(rest)
            .map_err(|_| DecodeError::Malformed)?;
        if !trailing.is_empty() {
            return Err(DecodeError::Malformed);
        }
        if events.len() > MAX_EVENTS {
            return Err(DecodeError::OverLimit);
        }
        let snapshot = Snapshot {
            tick,
            ack,
            queued,
            hash,
            result,
            fighters,
            scoreboard,
            events,
        };
        if !valid_snapshot(&snapshot) {
            return Err(DecodeError::OverLimit);
        }
        Ok(snapshot)
    }
}

/// Computes exact `postcard` wire byte length of `ServerMessage::Snapshot(snapshot)`
/// without allocating a frame buffer. Useful for rate-reduction comparisons.
pub fn full_snapshot_encoded_len(snapshot: &Snapshot) -> usize {
    // Variant 1 (`ServerMessage::Snapshot`) followed by `Snapshot` fields.
    postcard_len(&(1u32, snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ServerMessage, WireEvent};
    use arena_sim::arena::{ActorInput, Arena, Axes, CastAction};

    #[test]
    fn lossless_delta_round_trips_full_combat_and_matches_zero_alloc_size() {
        let layout = arena_map::colosseum().layout;
        let mut arena = Arena::practice(layout, 3).unwrap();
        let connected = [true, true, true, true, false, true];
        let mut prev = Snapshot::capture(&arena, 0, Vec::new(), &connected);
        let mut prev_every_2 = prev.clone();
        let mut total_full = 0usize;
        let mut total_delta = 0usize;
        let mut total_counts = FieldChangeCounts::default();

        for step in 1u32..=120 {
            let mut slots = [ActorInput::default(); MAX_FIGHTERS];
            for (actor, slot) in slots.iter_mut().enumerate() {
                slot.movement = Some(Axes {
                    x: if actor % 2 == 0 { 600 } else { -600 },
                    z: if actor % 2 == 0 { -800 } else { 800 },
                });
                slot.facing = Some((step as u16).wrapping_mul(300).wrapping_add(actor as u16));
                if step == 10 && actor == 0 {
                    slot.jump = true;
                }
                if step == 20 && actor == 0 {
                    slot.movement = Some(Axes { x: 0, z: 0 });
                    slot.action = Some(CastAction::Cast {
                        spell: SpellId::RiftBolt,
                        target: Some(1),
                    });
                }
                if (21..=66).contains(&step) && actor == 0 {
                    slot.movement = Some(Axes { x: 0, z: 0 });
                }
                if step == 35 && actor == 1 {
                    slot.action = Some(CastAction::Cast {
                        spell: SpellId::Quell,
                        target: Some(0),
                    });
                }
                if step == 80 && actor == 2 {
                    slot.action = ActorInput::pulse(3);
                }
            }
            let tick = arena.tick();
            let raw_events = arena.advance(&slots);
            let events: Vec<TickEvent> = raw_events
                .iter()
                .filter_map(|e| WireEvent::for_recipient(e, 0))
                .map(|event| TickEvent { tick, event })
                .collect();
            let mut conn = connected;
            if step >= 50 {
                conn[4] = true;
            }
            let curr = Snapshot::capture(&arena, step, events, &conn);

            // Verify zero-alloc full size helper equals `ServerMessage::encode`.
            let full_frame = ServerMessage::Snapshot(curr.clone()).encode();
            assert_eq!(full_snapshot_encoded_len(&curr), full_frame.len());
            total_full += full_frame.len();

            // Verify 30 Hz delta round-trip and exact size match.
            let (delta, counts) = SnapshotDelta::from_pair(&prev, &curr).expect("valid delta");
            let encoded = delta.encode();
            let size = delta.encoded_size();
            assert_eq!(size.total_bytes, encoded.len());
            assert!(size.hash_bytes >= 1 && size.hash_bytes <= 10);
            let rebuilt = SnapshotDelta::apply(&prev, &encoded).expect("lossless decode");
            assert_eq!(rebuilt, curr);
            total_delta += encoded.len();
            total_counts.merge(counts);

            // Verify 15 Hz (2-tick stride) delta also round-trips losslessly.
            if step % 2 == 0 {
                let (delta2, _) =
                    SnapshotDelta::from_pair(&prev_every_2, &curr).expect("valid 2-tick delta");
                let encoded2 = delta2.encode();
                assert_eq!(delta2.encoded_size().total_bytes, encoded2.len());
                let rebuilt2 =
                    SnapshotDelta::apply(&prev_every_2, &encoded2).expect("lossless 15 Hz decode");
                assert_eq!(rebuilt2, curr);
                prev_every_2 = curr.clone();
            }

            prev = curr;
        }
        assert!(total_delta < total_full, "{total_delta} vs {total_full}");
        assert!(total_counts.fighter_ticks == 120 * 6);
        assert!(total_counts.connected_changed == 1);
    }

    #[test]
    fn timer_and_cast_decay_prediction_omits_steady_countdown_fields() {
        let layout = arena_map::colosseum().layout;
        let arena = Arena::practice(layout, 1).unwrap();
        let mut prev = Snapshot::capture(&arena, 1, Vec::new(), &[true, true]);
        prev.tick = 10;
        prev.fighters[0].gcd_remaining = 20;
        prev.fighters[0].lockout_remaining = 50;
        prev.fighters[0].cooldowns = [30, 60, 180, 90];
        prev.fighters[0].active_cast = Some(WireCast {
            spell: SpellId::RiftBolt.into(),
            target: Some(1),
            remaining_ticks: 40,
            total_ticks: 45,
        });

        let mut curr = prev.clone();
        curr.tick = 11;
        curr.ack = 2;
        curr.fighters[0].gcd_remaining = 19;
        curr.fighters[0].lockout_remaining = 49;
        curr.fighters[0].cooldowns = [29, 59, 179, 89];
        curr.fighters[0].active_cast = Some(WireCast {
            spell: SpellId::RiftBolt.into(),
            target: Some(1),
            remaining_ticks: 39,
            total_ticks: 45,
        });

        let (delta, counts) = SnapshotDelta::from_pair(&prev, &curr).unwrap();
        assert_eq!(
            delta.fighters[0].mask, 0,
            "steady decay should emit zero mask bits"
        );
        assert_eq!(counts.timers_raw_changed, 1);
        assert_eq!(counts.timers_unexpected_changed, 0);
        assert_eq!(counts.active_cast_raw_changed, 1);
        assert_eq!(counts.active_cast_unexpected_changed, 0);
        let bytes = delta.encode();
        assert_eq!(SnapshotDelta::apply(&prev, &bytes), Ok(curr));

        // Natural cast completion (`remaining_ticks == 1` decaying to `None`) also needs zero mask bits.
        let mut completing_prev = prev.clone();
        completing_prev.fighters[0].active_cast = Some(WireCast {
            spell: SpellId::RiftBolt.into(),
            target: Some(1),
            remaining_ticks: 1,
            total_ticks: 45,
        });
        let mut completed_curr = completing_prev.clone();
        completed_curr.tick += 1;
        completed_curr.fighters[0].gcd_remaining = completing_prev.fighters[0].gcd_remaining - 1;
        completed_curr.fighters[0].lockout_remaining =
            completing_prev.fighters[0].lockout_remaining - 1;
        for cd in &mut completed_curr.fighters[0].cooldowns {
            *cd -= 1;
        }
        completed_curr.fighters[0].active_cast = None;
        let (done_delta, done_counts) =
            SnapshotDelta::from_pair(&completing_prev, &completed_curr).unwrap();
        assert_eq!(done_delta.fighters[0].mask, 0);
        assert_eq!(done_counts.active_cast_raw_changed, 1);
        assert_eq!(done_counts.active_cast_unexpected_changed, 0);
        assert_eq!(
            SnapshotDelta::apply(&completing_prev, &done_delta.encode()),
            Ok(completed_curr)
        );
    }

    #[test]
    fn delta_rejects_rewinds_mismatched_rosters_and_corrupted_frames() {
        let layout = arena_map::colosseum().layout;
        let a1 = Arena::practice(layout.clone(), 1).unwrap();
        let a2 = Arena::practice(layout, 2).unwrap();
        let mut s1 = Snapshot::capture(&a1, 1, Vec::new(), &[true, true]);
        s1.tick = 5;
        let mut rewind = s1.clone();
        rewind.tick = 4;
        assert!(SnapshotDelta::from_pair(&s1, &rewind).is_none());

        let s2 = Snapshot::capture(&a2, 1, Vec::new(), &[true; 4]);
        assert!(SnapshotDelta::from_pair(&s1, &s2).is_none());

        // Oversized or odd/empty rosters are refused by `from_pair`.
        let mut oversized = s1.clone();
        oversized.fighters = vec![FighterState::default(); MAX_FIGHTERS + 2];
        assert!(SnapshotDelta::from_pair(&oversized, &oversized).is_none());
        let mut odd = s1.clone();
        odd.fighters = vec![FighterState::default(); 3];
        assert!(SnapshotDelta::from_pair(&odd, &odd).is_none());

        assert_eq!(SnapshotDelta::apply(&s1, &[]), Err(DecodeError::Empty));
        assert_eq!(
            SnapshotDelta::apply(&s1, &vec![0u8; MAX_SERVER_FRAME + 1]),
            Err(DecodeError::Oversized)
        );
        // Wrong leading variant (!= 1).
        let (good, _) = SnapshotDelta::from_pair(&s1, &s1).unwrap();
        let mut bad_variant = good.encode();
        bad_variant[0] = 2;
        assert_eq!(
            SnapshotDelta::apply(&s1, &bad_variant),
            Err(DecodeError::Malformed)
        );
        // Mismatched fighter count against `prev`.
        let (good2, _) = SnapshotDelta::from_pair(&s2, &s2).unwrap();
        assert_eq!(
            SnapshotDelta::apply(&s1, &good2.encode()),
            Err(DecodeError::OverLimit)
        );
        // `prev.tick + tick_delta` u64 overflow.
        let mut overflow_delta = good.clone();
        overflow_delta.tick_delta = u64::MAX;
        assert_eq!(
            SnapshotDelta::apply(&s1, &overflow_delta.encode()),
            Err(DecodeError::OverLimit)
        );
        // Out-of-range `dfacing` (`32_768` and `-32_769`).
        for bad_dfacing in [32_768, -32_769] {
            let mut d = good.clone();
            d.fighters[0].mask = MASK_DFACING;
            d.fighters[0].dfacing = bad_dfacing;
            assert_eq!(
                SnapshotDelta::apply(&s1, &d.encode()),
                Err(DecodeError::Malformed)
            );
        }
        // Unknown reserved mask bit.
        let mut bad = good.clone();
        bad.fighters[0].mask = 1u16 << 15;
        assert_eq!(
            SnapshotDelta::apply(&s1, &bad.encode()),
            Err(DecodeError::Malformed)
        );
        // Reserved cooldown submask bit (`1 << SpellId::COUNT`).
        let mut bad_cd = good.clone();
        bad_cd.fighters[0].mask = MASK_COOLDOWNS;
        bad_cd.fighters[0].cooldown_mask = 1u8 << SpellId::COUNT;
        assert_eq!(
            SnapshotDelta::apply(&s1, &bad_cd.encode()),
            Err(DecodeError::Malformed)
        );
        // Invalid `result_changed` (`Won { team: 2 }`).
        let mut bad_result = good.clone();
        bad_result.result_changed = Some(Some(WireResult::Won { team: 2 }));
        assert_eq!(
            SnapshotDelta::apply(&s1, &bad_result.encode()),
            Err(DecodeError::OverLimit)
        );
        // Events over `MAX_EVENTS`.
        let mut too_many_events = good.clone();
        too_many_events.events = vec![
            TickEvent {
                tick: 5,
                event: WireEvent::Jumped { actor: 0 },
            };
            MAX_EVENTS + 1
        ];
        assert_eq!(
            SnapshotDelta::apply(&s1, &too_many_events.encode()),
            Err(DecodeError::OverLimit)
        );
        // Adversarial truncated varint events length prefix (promises huge Vec).
        let mut huge_events = good.encode();
        huge_events.pop(); // remove empty events length byte 0x00
        huge_events.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
        assert_eq!(
            SnapshotDelta::apply(&s1, &huge_events),
            Err(DecodeError::Malformed)
        );
        // Trailing bytes rejected.
        let mut trailing = good.encode();
        trailing.push(0);
        assert_eq!(
            SnapshotDelta::apply(&s1, &trailing),
            Err(DecodeError::Malformed)
        );
        // Invalid health (> MAX_HEALTH) rejected by `valid_snapshot`.
        let mut invalid_hp = good;
        invalid_hp.fighters[0].mask = MASK_HEALTH;
        invalid_hp.fighters[0].health = 9999;
        assert_eq!(
            SnapshotDelta::apply(&s1, &invalid_hp.encode()),
            Err(DecodeError::OverLimit)
        );
    }

    #[test]
    fn delta_round_trips_finished_snapshot_with_scoreboard() {
        let layout = arena_map::colosseum().layout;
        let arena = Arena::practice(layout, 1).unwrap();
        let prev = Snapshot::capture(&arena, 1, Vec::new(), &[true, true]);
        let mut finished = prev.clone();
        finished.tick = 42;
        finished.ack = 40;
        finished.result = Some(WireResult::Won { team: 0 });
        finished.fighters[1].health = 0;
        finished.scoreboard = Some(vec![
            FighterScore {
                damage_done: 100,
                damage_taken: 25,
                healing_done: 30,
                healing_received: 30,
                killing_blows: 1,
            },
            FighterScore {
                damage_done: 25,
                damage_taken: 100,
                healing_done: 0,
                healing_received: 0,
                killing_blows: 0,
            },
        ]);

        let (delta, _) = SnapshotDelta::from_pair(&prev, &finished).unwrap();
        let bytes = delta.encode();
        assert_eq!(delta.encoded_size().total_bytes, bytes.len());
        assert_eq!(SnapshotDelta::apply(&prev, &bytes), Ok(finished));
    }
}
