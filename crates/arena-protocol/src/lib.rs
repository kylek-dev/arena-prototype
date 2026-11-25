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
/// 5: `FighterScore` and `Snapshot.scoreboard` for server-authoritative end-of-match totals.
/// 6: `WireAppearance` on `ClientMessage::Hello` and `ServerMessage::Appearances` per-seat relay.
/// 7: `Welcome.spectator` for watch-only delayed spectator connections.
/// 8: `WireAppearance.facial_hair_color`, and up to 24 part groups for the armour slots (ADR 0053).
/// 9: `ServerMessage::Handles`: each seat's server-set `Name#1234` handle.
pub const PROTOCOL_VERSION: u16 = 9;
/// Bumped whenever a rules change could alter a replayed outcome.
/// 2: rounds end in a draw after twenty minutes (`MAX_ROUND_TICKS`).
/// 3: time-limit draws are distinct from mutual-elimination draws and round limits are explicit.
/// 4: server-authoritative casting, GCD, per-spell cooldowns, resources, and interrupts.
/// 5: bounded server-side target-position history rewind for cast starts (ADR 0049).
/// 6: timed casts charge resource and health costs on completion, not at start (ADR 0048).
pub const RULES_VERSION: u32 = 6;
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
/// Maximum byte length of any identifier inside [`WireAppearance`].
pub const MAX_APPEARANCE_ID: usize = 32;
/// Maximum part-group choices inside [`WireAppearance`].
pub const MAX_APPEARANCE_GROUPS: usize = 24;
/// Maximum material-variant choices inside [`WireAppearance`].
pub const MAX_APPEARANCE_VARIANTS: usize = 8;

/// Bounded, identifier-only character appearance choice sent with `Hello` and
/// relayed per seat in [`ServerMessage::Appearances`]. Never carries asset
/// paths, floats, or simulation state; clients validate identifiers against
/// their local kit catalog and fall back to defaults for unknown values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireAppearance {
    pub ancestry: String,
    pub body: String,
    pub skin_tone: String,
    pub hair_color: String,
    /// Beard and stubble colour; clients send the hair colour until one is chosen.
    pub facial_hair_color: String,
    pub eyes: String,
    /// Race build morph weight in percent (`0..=100`, mapped to `0.0..=1.0`).
    pub build: u8,
    /// `(group_id, option_id)` pairs (unique keys, at most [`MAX_APPEARANCE_GROUPS`]).
    pub groups: Vec<(String, String)>,
    /// `(material_slot, variant_id)` pairs (unique keys, at most [`MAX_APPEARANCE_VARIANTS`]).
    pub variants: Vec<(String, String)>,
}

impl WireAppearance {
    /// Strict ASCII identifier check: never permits path separators, dots, whitespace, or empty strings.
    pub fn is_valid_id(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= MAX_APPEARANCE_ID
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    }

    /// Validates all identifier lengths, character sets, counts, uniqueness, and morph bounds.
    pub fn is_valid(&self) -> bool {
        if !Self::is_valid_id(&self.ancestry)
            || !Self::is_valid_id(&self.body)
            || !Self::is_valid_id(&self.skin_tone)
            || !Self::is_valid_id(&self.hair_color)
            || !Self::is_valid_id(&self.facial_hair_color)
            || !Self::is_valid_id(&self.eyes)
            || self.build > 100
            || self.groups.len() > MAX_APPEARANCE_GROUPS
            || self.variants.len() > MAX_APPEARANCE_VARIANTS
        {
            return false;
        }
        let valid_pairs = |pairs: &[(String, String)]| {
            for (i, (k, v)) in pairs.iter().enumerate() {
                if !Self::is_valid_id(k) || !Self::is_valid_id(v) {
                    return false;
                }
                if pairs[..i].iter().any(|(prev, _)| prev == k) {
                    return false;
                }
            }
            true
        };
        valid_pairs(&self.groups) && valid_pairs(&self.variants)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Must stay variant 0; see [`PROTOCOL_VERSION`].
    Hello {
        protocol: u16,
        ticket: Vec<u8>,
        /// Optional locked character look choice for this seat. Boxed so the
        /// per-tick `Input` messages stay small; the wire bytes are the same.
        appearance: Option<Box<WireAppearance>>,
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

/// Shortest handle name.
pub const MIN_HANDLE_NAME: usize = 3;
/// Longest handle name, in ASCII characters.
pub const MAX_HANDLE_NAME: usize = 16;
/// Handle numbers run `1..=MAX_HANDLE_NUMBER`, shown with four digits.
pub const MAX_HANDLE_NUMBER: u16 = 9999;

/// A player's `Name#1234` handle (social-identity step 3): a name the player
/// chose (ASCII letters and digits, starting with a letter) and a number the
/// front door assigned so the pair is unique. Fixed size so tickets stay
/// `Copy`; always set by the server, never by a client.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Handle {
    /// The name's bytes, then zeros.
    name: [u8; MAX_HANDLE_NAME],
    pub number: u16,
}

impl Handle {
    /// `None` unless `name` is 3–16 ASCII letters and digits starting with a
    /// letter, and `number` is `1..=9999`.
    pub fn new(name: &str, number: u16) -> Option<Self> {
        let bytes = name.as_bytes();
        if !(MIN_HANDLE_NAME..=MAX_HANDLE_NAME).contains(&bytes.len()) {
            return None;
        }
        let mut stored = [0; MAX_HANDLE_NAME];
        stored[..bytes.len()].copy_from_slice(bytes);
        let handle = Self {
            name: stored,
            number,
        };
        handle.is_valid().then_some(handle)
    }

    pub fn name(&self) -> &str {
        let len = self
            .name
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(MAX_HANDLE_NAME);
        std::str::from_utf8(&self.name[..len]).unwrap_or("")
    }

    /// Whether a decoded handle keeps the rules: a valid name, zero padding
    /// only after it, and a number in range.
    pub fn is_valid(&self) -> bool {
        let len = self
            .name
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(MAX_HANDLE_NAME);
        (MIN_HANDLE_NAME..=MAX_HANDLE_NAME).contains(&len)
            && self.name[0].is_ascii_alphabetic()
            && self.name[..len].iter().all(u8::is_ascii_alphanumeric)
            && self.name[len..].iter().all(|b| *b == 0)
            && (1..=MAX_HANDLE_NUMBER).contains(&self.number)
    }
}

impl std::fmt::Display for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{:04}", self.name(), self.number)
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
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
    /// Must stay variant 3; browser runtime recognizes `[3, code]`.
    Error(ErrorCode),
    /// Per-seat locked character looks in server actor order (`2..=MAX_FIGHTERS`, even).
    Appearances(Vec<Option<WireAppearance>>),
    /// Per-seat `Name#1234` handles from the seats' tickets, in server actor
    /// order (`2..=MAX_FIGHTERS`, even); `None` for a seat not joined yet or
    /// joined by a link that names nobody.
    Handles(Vec<Option<Handle>>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Welcome {
    pub protocol: u16,
    /// The seat this connection controls, derived from its ticket (`0` for spectators).
    pub actor: u8,
    pub team_size: u8,
    pub tick: u64,
    pub tick_rate: u8,
    /// Total ticks in the round before a time-limit draw (`1..=MAX_ROUND_TICKS`).
    pub round_ticks: u64,
    pub rules: u32,
    /// Digest of the match layout.
    pub map: u64,
    /// `true` for a watch-only spectator connection with no seat.
    pub spectator: bool,
}
impl Welcome {
    /// The seat this connection controls, or `None` when spectating.
    pub fn seat(&self) -> Option<usize> {
        (!self.spectator).then_some(usize::from(self.actor))
    }
}

/// Authoritative per-seat combat totals delivered in a finished match's final
/// [`Snapshot`] (in server actor order). Not part of `Arena::state_hash()`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FighterScore {
    pub damage_done: u32,
    pub damage_taken: u32,
    pub healing_done: u32,
    pub healing_received: u32,
    pub killing_blows: u16,
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
    /// Authoritative end-of-match per-seat totals in server actor order.
    /// Always `None` while the match is running; populated on the final
    /// snapshot (`result.is_some()`). Not part of `hash`.
    pub scoreboard: Option<Vec<FighterScore>>,
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
            scoreboard: None,
            events,
        }
    }

    /// Attaches authoritative per-seat end-of-match totals to this snapshot.
    pub fn with_scoreboard(mut self, scoreboard: Vec<FighterScore>) -> Self {
        self.scoreboard = Some(scoreboard);
        self
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

    /// Translates public arena events for a spectator connection.
    /// Private per-seat rejections (`ArenaEvent::Rejected`) are never sent.
    pub fn for_spectator(event: &ArenaEvent) -> Option<Self> {
        match event {
            ArenaEvent::Rejected { .. } => None,
            _ => Self::for_recipient(event, 0),
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
            Self::Hello {
                ticket, appearance, ..
            } if ticket.len() > MAX_TICKET
                || !appearance.as_deref().is_none_or(WireAppearance::is_valid) =>
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

fn valid_welcome(welcome: &Welcome) -> bool {
    let team_size = usize::from(welcome.team_size);
    let actor_valid = if welcome.spectator {
        welcome.actor == 0
    } else {
        usize::from(welcome.actor) < team_size * 2
    };
    (1..=MAX_FIGHTERS / 2).contains(&team_size)
        && actor_valid
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
    if let Some(scores) = &snapshot.scoreboard
        && (snapshot.result.is_none() || scores.len() != count)
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

fn valid_appearances(appearances: &[Option<WireAppearance>]) -> bool {
    let count = appearances.len();
    (2..=MAX_FIGHTERS).contains(&count)
        && count.is_multiple_of(2)
        && appearances
            .iter()
            .all(|look| look.as_ref().is_none_or(WireAppearance::is_valid))
}

fn valid_handles(handles: &[Option<Handle>]) -> bool {
    let count = handles.len();
    (2..=MAX_FIGHTERS).contains(&count)
        && count.is_multiple_of(2)
        && handles
            .iter()
            .all(|handle| handle.is_none_or(|h| h.is_valid()))
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
            Self::Appearances(appearances) if !valid_appearances(appearances) => {
                Err(DecodeError::OverLimit)
            }
            Self::Handles(handles) if !valid_handles(handles) => Err(DecodeError::OverLimit),
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
/// `fighters`, `scoreboard`, and optionally shared `events`) so a server
/// encodes them once per tick instead of once per connected seat.
#[derive(Clone, Debug)]
pub struct EncodedSnapshotBody {
    tick: u64,
    /// Postcard bytes for `(hash, result, fighters, scoreboard)` or
    /// `(hash, result, fighters, scoreboard, events)` when `includes_events` is true.
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
        scoreboard: Option<&[FighterScore]>,
        shared_events: Option<&[TickEvent]>,
    ) -> Self {
        let mut tail = Vec::with_capacity(
            18 + fighters.len() * 32
                + scoreboard.map_or(0, |s| s.len() * 12 + 2)
                + shared_events.map_or(0, |e| e.len() * 10 + 1),
        );
        tail = postcard::to_extend(&(hash, result, fighters, scoreboard), tail)
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
