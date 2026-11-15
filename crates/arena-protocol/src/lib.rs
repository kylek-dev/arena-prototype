//! Match wire protocol (ADR 0044, `docs/PROTOCOL.md`).
//!
//! Every WebSocket binary frame holds exactly one message encoded with
//! `postcard`. Decoding is strict and bounded: oversized frames, trailing
//! bytes, unknown variants and over-long fields are errors, never panics.
//! Clients send intent only; nothing here lets a client name an actor or
//! claim a position, health or cooldown. The JSON rendering is for
//! diagnostics and is never accepted on the wire.

use arena_sim::arena::{ActorInput, Arena, ArenaEvent, Axes, Combatant, Rejection, RoundResult};
use serde::{Deserialize, Serialize};

/// Bumped on any incompatible change to a message layout. `Hello` stays
/// variant 0 with `protocol` as its first field forever, so a server can
/// always read the version of a client it no longer understands.
pub const PROTOCOL_VERSION: u16 = 1;
/// Bumped whenever a rules change could alter a replayed outcome.
pub const RULES_VERSION: u32 = 1;
/// Largest frame a client may send.
pub const MAX_CLIENT_FRAME: usize = 1024;
/// Largest frame the server sends; a full 3v3 snapshot is far smaller.
pub const MAX_SERVER_FRAME: usize = 16 * 1024;
/// Largest join ticket accepted in `Hello`.
pub const MAX_TICKET: usize = 512;
/// Most fighters a snapshot may describe (3v3).
pub const MAX_FIGHTERS: usize = 6;
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
    /// Attack this actor index.
    pub attack: Option<u8>,
}
impl InputFrame {
    /// Folds this frame into the slot for the next tick: the latest movement,
    /// facing and attack target win, and a jump from any frame is kept, so
    /// a one-shot press is never lost between ticks and the rules still see
    /// at most one command of each kind.
    pub fn merge_into(&self, slot: &mut ActorInput) {
        slot.movement = Some(Axes {
            x: self.movement[0],
            z: self.movement[1],
        });
        slot.facing = Some(self.facing);
        slot.jump |= self.jump;
        if let Some(target) = self.attack {
            slot.attack = Some(usize::from(target));
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FighterState {
    pub x: i32,
    pub z: i32,
    pub height: i32,
    pub vertical_velocity: i32,
    pub momentum: [i32; 2],
    pub health: u16,
    pub ready_at: u64,
    pub facing: u16,
    /// Whether this fighter's player is attached. A disconnected fighter
    /// stands idle (no forfeit) and its player may rejoin with the same
    /// ticket; clients show it as disconnected. Not part of the rules or
    /// `hash`.
    pub connected: bool,
}
impl FighterState {
    pub fn new(fighter: &Combatant, connected: bool) -> Self {
        Self {
            x: fighter.position.x,
            z: fighter.position.z,
            height: fighter.height,
            vertical_velocity: fighter.vertical_velocity,
            momentum: [fighter.momentum.x, fighter.momentum.z],
            health: fighter.health,
            ready_at: fighter.ready_at,
            facing: fighter.facing,
            connected,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireResult {
    Won { team: u8 },
    Draw,
}
impl From<RoundResult> for WireResult {
    fn from(result: RoundResult) -> Self {
        match result {
            RoundResult::Won { team } => Self::Won { team: team as u8 },
            RoundResult::Draw => Self::Draw,
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
    Hit {
        actor: u8,
        target: u8,
        damage: u16,
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
        match *event {
            ArenaEvent::Moved { .. } => None,
            ArenaEvent::Jumped { actor: index } => Some(Self::Jumped {
                actor: actor(index)?,
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
    OutOfRange,
    LineOfSight,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Empty,
    Oversized,
    /// Undecodable, truncated, an unknown variant, or trailing bytes.
    Malformed,
    /// Well formed but over a field limit (ticket, fighters or events).
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

impl ServerMessage {
    pub fn encode(&self) -> Vec<u8> {
        encode(self)
    }
    pub fn decode(frame: &[u8]) -> Result<Self, DecodeError> {
        let message: Self = decode(frame, MAX_SERVER_FRAME)?;
        match &message {
            Self::Snapshot(snapshot)
                if snapshot.fighters.len() > MAX_FIGHTERS || snapshot.events.len() > MAX_EVENTS =>
            {
                Err(DecodeError::OverLimit)
            }
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

fn encode<T: Serialize>(message: &T) -> Vec<u8> {
    // Only allocation failure can fail here: every type is plain data.
    postcard::to_allocvec(message).expect("protocol messages always encode")
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
