//! Match wire protocol (ADR 0044, ADR 0048, `docs/PROTOCOL.md`).
//!
//! Every WebSocket binary frame holds exactly one message encoded with
//! `postcard`. Decoding is strict and bounded: oversized frames, trailing
//! bytes, unknown variants and over-long or out-of-bounds fields are errors,
//! never panics. Clients send intent only; nothing here lets a client name an
//! actor or claim a position, health, resource, cast progress or cooldown.
//! The JSON rendering is for diagnostics and is never accepted on the wire.

pub mod delta;
pub mod prediction;

use arena_sim::arena::{
    ActiveCastState, ActorInput, Arena, ArenaEvent, Axes, CastAction, Combatant, InterruptReason,
    MAX_ROUND_TICKS, Position, Rejection, RoundResult,
};
use arena_sim::spells::SpellId;
use serde::{Deserialize, Serialize};

/// Bumped on any incompatible change to a message layout. `Hello` stays
/// variant 0 with `protocol` as its first field forever, so a server can
/// always read the version of a client it no longer understands.
/// 2: `Welcome.round_ticks` and `WireResult::TimeLimitDraw`.
/// 3: `InputFrame.action` (`WireAction`), casting/resource fields on `FighterState`,
///    and cast/interrupt/heal events & rejections.
/// 4: `InputFrame.view_tick` for bounded server-side targeting lag compensation (ADR 0049).
pub const PROTOCOL_VERSION: u16 = 4;
/// Bumped whenever a rules change could alter a replayed outcome.
/// 2: rounds end in a draw after twenty minutes (`MAX_ROUND_TICKS`).
/// 3: time-limit draws are distinct from mutual-elimination draws and round limits are explicit.
/// 4: server-authoritative casting, GCD, per-spell cooldowns, resources, and interrupts.
/// 5: bounded server-side target-position history rewind for cast starts (ADR 0049).
pub const RULES_VERSION: u32 = 5;
/// Largest frame a client may send.
pub const MAX_CLIENT_FRAME: usize = 1024;
/// Largest frame the server sends; a full 3v3 snapshot is far smaller.
pub const MAX_SERVER_FRAME: usize = 16 * 1024;
/// Largest join ticket accepted in `Hello`.
pub const MAX_TICKET: usize = 512;
/// Most fighters a snapshot may describe (3v3).
pub const MAX_FIGHTERS: usize = 6;
const _: () = assert!(MAX_FIGHTERS == arena_sim::map::MAX_TEAM * 2);
/// Most events one snapshot may carry; the server sends more snapshots
/// rather than exceed it.
pub const MAX_EVENTS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Must stay variant 0; see [`PROTOCOL_VERSION`].
    Hello {
        protocol: u16,
        ticket: Vec<u8>,
    },
    Input(InputFrame),
    Ping {
        nonce: u32,
    },
}

/// Wire identifier for a spell in the shared arena kit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireSpell {
    Pulse,
    RiftBolt,
    Quell,
    Mend,
}
impl From<WireSpell> for SpellId {
    fn from(spell: WireSpell) -> Self {
        match spell {
            WireSpell::Pulse => Self::Pulse,
            WireSpell::RiftBolt => Self::RiftBolt,
            WireSpell::Quell => Self::Quell,
            WireSpell::Mend => Self::Mend,
        }
    }
}
impl From<SpellId> for WireSpell {
    fn from(spell: SpellId) -> Self {
        match spell {
            SpellId::Pulse => Self::Pulse,
            SpellId::RiftBolt => Self::RiftBolt,
            SpellId::Quell => Self::Quell,
            SpellId::Mend => Self::Mend,
        }
    }
}

/// One combat intent carried in an [`InputFrame`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireAction {
    #[default]
    None,
    Cast {
        spell: WireSpell,
        target: Option<u8>,
    },
    CancelCast,
}

/// One input message. Several may arrive between ticks; the server merges
/// them into the sender's slot with [`InputFrame::merge_into`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputFrame {
    /// Increases by one per message; repeats and regressions are dropped.
    pub seq: u32,
    /// Planar axes, each in ±1000; the rules reject anything else.
    pub movement: [i16; 2],
    /// Binary angle, as `Combatant::facing`.
    pub facing: u16,
    pub jump: bool,
    /// Spell cast or explicit cast cancellation for this tick.
    pub action: WireAction,
    /// Authoritative server tick of the remote fighters rendered on the client
    /// when this intent was generated (`0` = unspecified / present tick only).
    /// Never trusted directly: the server clamps it monotonically to at most
    /// `MAX_LAG_COMP_TICKS` behind the current tick and validates only against
    /// server-recorded target positions.
    pub view_tick: u64,
}
impl InputFrame {
    /// Convenience action for the default Pulse attack against `target`.
    pub const fn pulse(target: u8) -> WireAction {
        WireAction::Cast {
            spell: WireSpell::Pulse,
            target: Some(target),
        }
    }

    /// Folds this frame into the slot for the next tick: the latest movement,
    /// facing and non-empty combat action win, and a jump from any frame is
    /// kept, so a one-shot press is never lost between ticks and the rules
    /// still see at most one command of each kind.
    pub fn merge_into(&self, slot: &mut ActorInput) {
        slot.movement = Some(Axes {
            x: self.movement[0],
            z: self.movement[1],
        });
        slot.facing = Some(self.facing);
        slot.jump |= self.jump;
        match self.action {
            WireAction::None => {}
            WireAction::Cast { spell, target } => {
                slot.action = Some(CastAction::Cast {
                    spell: spell.into(),
                    target: target.map(usize::from),
                });
            }
            WireAction::CancelCast => {
                slot.action = Some(CastAction::Cancel);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerMessage {
    Welcome(Welcome),
    Snapshot(Snapshot),
    Pong {
        nonce: u32,
    },
    /// Sent once before the server closes the connection.
    Error(ErrorCode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Welcome {
    pub protocol: u16,
    /// The seat this connection controls, derived from its ticket.
    pub actor: u8,
    pub team_size: u8,
    pub tick: u64,
    pub tick_rate: u8,
    /// Total ticks in the round before a time-limit draw (`1..=MAX_ROUND_TICKS`).
    pub round_ticks: u64,
    pub rules: u32,
    /// Digest of the match layout.
    pub map: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The tick this state follows (`Arena::tick()` after advancing).
    pub tick: u64,
    /// The last input `seq` applied for the recipient.
    pub ack: u32,
    /// The recipient's inputs still queued after this tick. Clients pace
    /// their sending to keep it near one (`prediction::Pacer`).
    pub queued: u8,
    /// `Arena::state_hash()` of this state.
    pub hash: u64,
    pub result: Option<WireResult>,
    /// Every fighter in actor order, with enough state to rebuild the
    /// arena for prediction and to check `hash`.
    pub fighters: Vec<FighterState>,
    pub events: Vec<TickEvent>,
}

impl Snapshot {
    /// The arena's current state for one recipient, carrying `events`
    /// already filtered with [`WireEvent::for_recipient`]. `connected[i]`
    /// says whether actor `i`'s player is attached; missing entries are
    /// disconnected.
    pub fn capture(arena: &Arena, ack: u32, events: Vec<TickEvent>, connected: &[bool]) -> Self {
        Self {
            tick: arena.tick(),
            ack,
            queued: 0,
            hash: arena.state_hash(),
            result: arena.result().map(WireResult::from),
            fighters: arena
                .fighters()
                .iter()
                .enumerate()
                .map(|(index, fighter)| {
                    FighterState::new(fighter, connected.get(index).copied().unwrap_or(false))
                })
                .collect(),
            events,
        }
    }
}

/// Wire representation of an in-progress timed cast.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireCast {
    pub spell: WireSpell,
    pub target: Option<u8>,
    pub remaining_ticks: u32,
    pub total_ticks: u32,
}
impl From<ActiveCastState> for WireCast {
    fn from(cast: ActiveCastState) -> Self {
        Self {
            spell: cast.spell.into(),
            target: cast.target.map(|t| t as u8),
            remaining_ticks: cast.remaining_ticks,
            total_ticks: cast.total_ticks,
        }
    }
}
impl From<WireCast> for ActiveCastState {
    fn from(cast: WireCast) -> Self {
        Self {
            spell: cast.spell.into(),
            target: cast.target.map(usize::from),
            remaining_ticks: cast.remaining_ticks,
            total_ticks: cast.total_ticks,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FighterState {
    pub x: i32,
    pub z: i32,
    pub height: i32,
    pub vertical_velocity: i32,
    pub momentum: [i32; 2],
    pub health: u16,
    pub resource: u16,
    pub gcd_remaining: u32,
    pub lockout_remaining: u32,
    pub cooldowns: [u32; SpellId::COUNT],
    pub active_cast: Option<WireCast>,
    pub facing: u16,
    /// Whether this fighter's player is attached. A disconnected fighter
    /// stands idle (no forfeit) and its player may rejoin with the same
    /// ticket; clients show it as disconnected. Not part of the rules or
    /// `hash`.
    pub connected: bool,
}
impl FighterState {
    /// The rule state this describes (without `connected`).
    pub fn combatant(&self) -> Combatant {
        Combatant {
            position: Position {
                x: self.x,
                z: self.z,
            },
            height: self.height,
            vertical_velocity: self.vertical_velocity,
            momentum: Position {
                x: self.momentum[0],
                z: self.momentum[1],
            },
            health: self.health,
            resource: self.resource,
            gcd_remaining: self.gcd_remaining,
            lockout_remaining: self.lockout_remaining,
            cooldowns: self.cooldowns,
            active_cast: self.active_cast.map(ActiveCastState::from),
            facing: self.facing,
        }
    }
    pub fn new(fighter: &Combatant, connected: bool) -> Self {
        Self {
            x: fighter.position.x,
            z: fighter.position.z,
            height: fighter.height,
            vertical_velocity: fighter.vertical_velocity,
            momentum: [fighter.momentum.x, fighter.momentum.z],
            health: fighter.health,
            resource: fighter.resource,
            gcd_remaining: fighter.gcd_remaining,
            lockout_remaining: fighter.lockout_remaining,
            cooldowns: fighter.cooldowns,
            active_cast: fighter.active_cast.map(WireCast::from),
            facing: fighter.facing,
            connected,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireResult {
    Won { team: u8 },
    Draw,
    TimeLimitDraw,
}
impl From<WireResult> for RoundResult {
    fn from(result: WireResult) -> Self {
        match result {
            WireResult::Won { team } => Self::Won {
                team: usize::from(team),
            },
            WireResult::Draw => Self::Draw,
            WireResult::TimeLimitDraw => Self::TimeLimitDraw,
        }
    }
}
impl From<RoundResult> for WireResult {
    fn from(result: RoundResult) -> Self {
        match result {
            RoundResult::Won { team } => Self::Won { team: team as u8 },
            RoundResult::Draw => Self::Draw,
            RoundResult::TimeLimitDraw => Self::TimeLimitDraw,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireInterruptReason {
    Cancelled,
    Moved,
    Interrupted,
    Silenced,
    OutOfRange,
    LineOfSight,
    TargetDead,
    CasterDead,
}
impl From<WireInterruptReason> for InterruptReason {
    fn from(reason: WireInterruptReason) -> Self {
        match reason {
            WireInterruptReason::Cancelled => Self::Cancelled,
            WireInterruptReason::Moved => Self::Moved,
            WireInterruptReason::Interrupted => Self::Interrupted,
            WireInterruptReason::Silenced => Self::Silenced,
            WireInterruptReason::OutOfRange => Self::OutOfRange,
            WireInterruptReason::LineOfSight => Self::LineOfSight,
            WireInterruptReason::TargetDead => Self::TargetDead,
            WireInterruptReason::CasterDead => Self::CasterDead,
        }
    }
}
impl From<InterruptReason> for WireInterruptReason {
    fn from(reason: InterruptReason) -> Self {
        match reason {
            InterruptReason::Cancelled => Self::Cancelled,
            InterruptReason::Moved => Self::Moved,
            InterruptReason::Interrupted => Self::Interrupted,
            InterruptReason::Silenced => Self::Silenced,
            InterruptReason::OutOfRange => Self::OutOfRange,
            InterruptReason::LineOfSight => Self::LineOfSight,
            InterruptReason::TargetDead => Self::TargetDead,
            InterruptReason::CasterDead => Self::CasterDead,
        }
    }
}

/// An event and the tick it happened on (the tick before advancing).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickEvent {
    pub tick: u64,
    pub event: WireEvent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireEvent {
    Jumped {
        actor: u8,
    },
    CastStarted {
        actor: u8,
        spell: WireSpell,
        target: Option<u8>,
        duration_ticks: u32,
    },
    CastCompleted {
        actor: u8,
        spell: WireSpell,
        target: Option<u8>,
    },
    CastInterrupted {
        actor: u8,
        spell: WireSpell,
        reason: WireInterruptReason,
        by: Option<u8>,
    },
    Hit {
        actor: u8,
        target: u8,
        damage: u16,
    },
    Healed {
        actor: u8,
        target: u8,
        amount: u16,
    },
    Finished(WireResult),
    /// Only ever sent to the actor whose intent was refused.
    Rejected(WireRejection),
}
impl WireEvent {
    /// What `recipient` may see of a rules event. Movement is visible in the
    /// fighter state, so `Moved` is not sent; other players' rejections and
    /// events naming a nonexistent actor are withheld.
    pub fn for_recipient(event: &ArenaEvent, recipient: usize) -> Option<Self> {
        let actor = |index: usize| u8::try_from(index).ok().filter(|_| index < MAX_FIGHTERS);
        let opt_actor = |index: Option<usize>| match index {
            None => Some(None),
            Some(i) => Some(Some(actor(i)?)),
        };
        match *event {
            ArenaEvent::Moved { .. } => None,
            ArenaEvent::Jumped { actor: index } => Some(Self::Jumped {
                actor: actor(index)?,
            }),
            ArenaEvent::CastStarted {
                actor: index,
                spell,
                target,
                duration_ticks,
            } => Some(Self::CastStarted {
                actor: actor(index)?,
                spell: spell.into(),
                target: opt_actor(target)?,
                duration_ticks,
            }),
            ArenaEvent::CastCompleted {
                actor: index,
                spell,
                target,
            } => Some(Self::CastCompleted {
                actor: actor(index)?,
                spell: spell.into(),
                target: opt_actor(target)?,
            }),
            ArenaEvent::CastInterrupted {
                actor: index,
                spell,
                reason,
                by,
            } => Some(Self::CastInterrupted {
                actor: actor(index)?,
                spell: spell.into(),
                reason: reason.into(),
                by: opt_actor(by)?,
            }),
            ArenaEvent::Hit {
                actor: index,
                target,
                damage,
            } => Some(Self::Hit {
                actor: actor(index)?,
                target: actor(target)?,
                damage,
            }),
            ArenaEvent::Healed {
                actor: index,
                target,
                amount,
            } => Some(Self::Healed {
                actor: actor(index)?,
                target: actor(target)?,
                amount,
            }),
            ArenaEvent::Finished(result) => Some(Self::Finished(result.into())),
            ArenaEvent::Rejected { actor, reason } => {
                (actor == recipient).then(|| Self::Rejected(reason.into()))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireRejection {
    InvalidActor,
    InvalidTarget,
    InvalidMovement,
    Duplicate,
    Airborne,
    MatchOver,
    Cooldown,
    GlobalCooldown,
    AlreadyCasting,
    NotEnoughResource,
    Moving,
    Silenced,
    NotCasting,
    OutOfRange,
    LineOfSight,
}
impl From<WireRejection> for Rejection {
    fn from(reason: WireRejection) -> Self {
        match reason {
            WireRejection::InvalidActor => Self::InvalidActor,
            WireRejection::InvalidTarget => Self::InvalidTarget,
            WireRejection::InvalidMovement => Self::InvalidMovement,
            WireRejection::Duplicate => Self::Duplicate,
            WireRejection::Airborne => Self::Airborne,
            WireRejection::MatchOver => Self::MatchOver,
            WireRejection::Cooldown => Self::Cooldown,
            WireRejection::GlobalCooldown => Self::GlobalCooldown,
            WireRejection::AlreadyCasting => Self::AlreadyCasting,
            WireRejection::NotEnoughResource => Self::NotEnoughResource,
            WireRejection::Moving => Self::Moving,
            WireRejection::Silenced => Self::Silenced,
            WireRejection::NotCasting => Self::NotCasting,
            WireRejection::OutOfRange => Self::OutOfRange,
            WireRejection::LineOfSight => Self::LineOfSight,
        }
    }
}
impl From<Rejection> for WireRejection {
    fn from(reason: Rejection) -> Self {
        match reason {
            Rejection::InvalidActor => Self::InvalidActor,
            Rejection::InvalidTarget => Self::InvalidTarget,
            Rejection::InvalidMovement => Self::InvalidMovement,
            Rejection::Duplicate => Self::Duplicate,
            Rejection::Airborne => Self::Airborne,
            Rejection::MatchOver => Self::MatchOver,
            Rejection::Cooldown => Self::Cooldown,
            Rejection::GlobalCooldown => Self::GlobalCooldown,
            Rejection::AlreadyCasting => Self::AlreadyCasting,
            Rejection::NotEnoughResource => Self::NotEnoughResource,
            Rejection::Moving => Self::Moving,
            Rejection::Silenced => Self::Silenced,
            Rejection::NotCasting => Self::NotCasting,
            Rejection::OutOfRange => Self::OutOfRange,
            Rejection::LineOfSight => Self::LineOfSight,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    VersionMismatch,
    BadTicket,
    MatchFull,
    MatchOver,
    RateLimited,
    Malformed,
    HelloTimeout,
    ServerShutdown,
    /// A newer connection with the same ticket took this seat.
    Replaced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Empty,
    Oversized,
    /// Undecodable, truncated, an unknown variant, or trailing bytes.
    Malformed,
    /// Well formed but over a field limit (ticket, welcome fields, fighters, events, result team, or combat invariants).
    OverLimit,
}

impl ClientMessage {
    pub fn encode(&self) -> Vec<u8> {
        encode(self)
    }
    pub fn decode(frame: &[u8]) -> Result<Self, DecodeError> {
        let message: Self = decode(frame, MAX_CLIENT_FRAME)?;
        match &message {
            Self::Hello { ticket, .. } if ticket.len() > MAX_TICKET => Err(DecodeError::OverLimit),
            _ => Ok(message),
        }
    }
    pub fn to_debug_json(&self) -> String {
        debug_json(self)
    }
}

fn valid_welcome(welcome: &Welcome) -> bool {
    let team_size = usize::from(welcome.team_size);
    (1..=MAX_FIGHTERS / 2).contains(&team_size)
        && usize::from(welcome.actor) < team_size * 2
        && welcome.tick_rate > 0
        && welcome.round_ticks > 0
        && welcome.round_ticks <= MAX_ROUND_TICKS
}

fn valid_result(result: WireResult) -> bool {
    match result {
        WireResult::Won { team } => team <= 1,
        WireResult::Draw | WireResult::TimeLimitDraw => true,
    }
}

fn valid_snapshot(snapshot: &Snapshot) -> bool {
    let count = snapshot.fighters.len();
    if !(2..=MAX_FIGHTERS).contains(&count)
        || !count.is_multiple_of(2)
        || snapshot.events.len() > MAX_EVENTS
        || !snapshot.result.is_none_or(valid_result)
    {
        return false;
    }
    for fighter in &snapshot.fighters {
        if !fighter.combatant().is_valid(count) {
            return false;
        }
    }
    let valid_actor = |a: u8| usize::from(a) < count;
    for item in &snapshot.events {
        let ok = match item.event {
            WireEvent::Jumped { actor } => valid_actor(actor),
            WireEvent::CastStarted {
                actor,
                spell,
                target,
                duration_ticks,
            } => {
                valid_actor(actor)
                    && target.is_none_or(valid_actor)
                    && SpellId::from(spell).definition().cast_ticks > 0
                    && duration_ticks > 0
            }
            WireEvent::CastCompleted { actor, target, .. } => {
                valid_actor(actor) && target.is_none_or(valid_actor)
            }
            WireEvent::CastInterrupted {
                actor, spell, by, ..
            } => {
                valid_actor(actor)
                    && by.is_none_or(valid_actor)
                    && SpellId::from(spell).definition().cast_ticks > 0
            }
            WireEvent::Hit { actor, target, .. } | WireEvent::Healed { actor, target, .. } => {
                valid_actor(actor) && valid_actor(target)
            }
            WireEvent::Finished(result) => valid_result(result),
            WireEvent::Rejected(_) => true,
        };
        if !ok {
            return false;
        }
    }
    true
}

impl ServerMessage {
    pub fn encode(&self) -> Vec<u8> {
        encode(self)
    }
    pub fn decode(frame: &[u8]) -> Result<Self, DecodeError> {
        let message: Self = decode(frame, MAX_SERVER_FRAME)?;
        match &message {
            Self::Welcome(welcome) if !valid_welcome(welcome) => Err(DecodeError::OverLimit),
            Self::Snapshot(snapshot) if !valid_snapshot(snapshot) => Err(DecodeError::OverLimit),
            _ => Ok(message),
        }
    }
    pub fn to_debug_json(&self) -> String {
        debug_json(self)
    }
}

/// Reads the protocol version from a `Hello` frame of any version, so a
/// server can answer `VersionMismatch` instead of `Malformed`. `None` when
/// the frame is not a `Hello`.
pub fn peek_hello_protocol(frame: &[u8]) -> Option<u16> {
    if frame.len() > MAX_CLIENT_FRAME {
        return None;
    }
    let (variant, rest) = postcard::take_from_bytes::<u32>(frame).ok()?;
    if variant != 0 {
        return None;
    }
    postcard::take_from_bytes::<u16>(rest)
        .ok()
        .map(|(protocol, _)| protocol)
}

/// Pre-encoded common fields of one tick's [`Snapshot`] (`hash`, `result`,
/// `fighters`, and optionally shared `events`) so a server encodes them once
/// per tick instead of once per connected seat.
#[derive(Clone, Debug)]
pub struct EncodedSnapshotBody {
    tick: u64,
    /// Postcard bytes for `(hash, result, fighters)` or
    /// `(hash, result, fighters, events)` when `includes_events` is true.
    tail: Vec<u8>,
    includes_events: bool,
}

impl EncodedSnapshotBody {
    /// Encodes the seat-independent fields of a tick's snapshot once.
    /// Pass `shared_events = Some(events)` when every seat receives the exact
    /// same event slice (e.g. when no per-seat rejection occurred).
    pub fn new(
        tick: u64,
        hash: u64,
        result: Option<WireResult>,
        fighters: &[FighterState],
        shared_events: Option<&[TickEvent]>,
    ) -> Self {
        let mut tail = Vec::with_capacity(
            16 + fighters.len() * 32 + shared_events.map_or(0, |e| e.len() * 10 + 1),
        );
        tail = postcard::to_extend(&(hash, result, fighters), tail)
            .expect("snapshot body always encodes");
        let includes_events = if let Some(events) = shared_events {
            tail = postcard::to_extend(events, tail).expect("snapshot events always encode");
            true
        } else {
            false
        };
        Self {
            tick,
            tail,
            includes_events,
        }
    }

    /// Produces the exact wire frame of `ServerMessage::Snapshot` for one seat.
    /// When [`Self::new`] already encoded shared events, `events` is ignored.
    pub fn encode_for_seat(&self, ack: u32, queued: u8, events: &[TickEvent]) -> Vec<u8> {
        let extra = if self.includes_events {
            0
        } else {
            events.len() * 10 + 2
        };
        let mut frame = Vec::with_capacity(18 + self.tail.len() + extra);
        // ServerMessage::Snapshot is variant 1, followed by tick, ack, queued.
        frame = postcard::to_extend(&(1u32, self.tick, ack, queued), frame)
            .expect("snapshot header always encodes");
        frame.extend_from_slice(&self.tail);
        if !self.includes_events {
            frame = postcard::to_extend(events, frame).expect("snapshot events always encode");
        }
        frame
    }
}

fn encode<T: Serialize>(message: &T) -> Vec<u8> {
    // Only allocation failure can fail here: every type is plain data.
    // Pre-sizing avoids repeated small reallocations on common frames.
    postcard::to_extend(message, Vec::with_capacity(32)).expect("protocol messages always encode")
}

fn decode<'a, T: Deserialize<'a>>(frame: &'a [u8], limit: usize) -> Result<T, DecodeError> {
    if frame.is_empty() {
        return Err(DecodeError::Empty);
    }
    if frame.len() > limit {
        return Err(DecodeError::Oversized);
    }
    match postcard::take_from_bytes(frame) {
        Ok((message, [])) => Ok(message),
        _ => Err(DecodeError::Malformed),
    }
}

fn debug_json<T: Serialize>(message: &T) -> String {
    serde_json::to_string(message).expect("protocol messages always render as JSON")
}

#[cfg(test)]
mod tests;
