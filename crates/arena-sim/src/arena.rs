//! Planar arena rules; positions are integer millimetres. No rendering,
//! networking, floating point, or OS time.
//!
//! Each tick takes one [`ActorInput`] slot per actor. The slot index is the
//! actor, so an input cannot act for someone else, and one slot carries at
//! most one movement, one facing, one jump and one combat action (the per-tick
//! budget). Slots resolve in phases, never in caller order (ADR 0044, ADR 0048):
//!
//! 1. airborne momentum carries every jumping body;
//! 2. ground movement, then facing, for every actor;
//! 3. jumps for every actor;
//! 4. combat: explicit cancellations and active timed casts resolve against one
//!    shared post-movement state (re-checking range, line of sight and target
//!    life at completion, then charging the cast's cost), new spell intents
//!    validate through `casting` and `combat_state` against that pre-damage
//!    state (instants pay at once, timed casts pay only on completion), and
//!    interrupts, damage, healing, death-interrupts and round completion land
//!    together;
//! 5. vertical motion;
//! 6. end-of-tick GCD, cooldown and lockout decay plus passive resource regen.
//!
//! Movement and jumps never depend on another actor (bodies do not collide
//! with each other), so phases 2 and 3 are order independent too.

use std::collections::BTreeMap;

use crate::TICKS_PER_SECOND;
use crate::casting::{CastError, CasterContext, CastingState, PreparedCast, TargetContext};
use crate::combat_state::{ActiveCast, CastStateError, CombatState};
use crate::effects;
use crate::map::{Layout, MAX_TEAM};
use crate::specs::Specialisation;
use crate::spells::{
    ARENA_CLASS, ARENA_RESOURCE, MAX_RESOURCE, PULSE_COOLDOWN_TICKS, PULSE_DAMAGE, PULSE_RANGE,
    RESOURCE_REGEN_AMOUNT, RESOURCE_REGEN_INTERVAL, SpellEffect, SpellId,
};

/// 7.2 m/s at 30 Hz.
pub const MOVE_PER_TICK: i32 = 240;
/// Millimetres/tick and millimetres/tick² (30 Hz): ~0.77 s airtime, 1.32 m apex.
pub const JUMP_SPEED: i32 = 220;
pub const GRAVITY: i32 = 20;
pub const BODY_RADIUS: i32 = 450;
pub const ATTACK_RANGE: i32 = PULSE_RANGE as i32;
pub const ATTACK_COOLDOWN: u64 = PULSE_COOLDOWN_TICKS as u64;
pub const ATTACK_DAMAGE: u16 = PULSE_DAMAGE;
pub const MAX_HEALTH: u16 = 100;
/// Longest a round may last: twenty minutes of ticks. A round still running
/// at its last tick ends in a draw, so passive play cannot hold a match (and
/// the server hosting it) open indefinitely.
pub const MAX_ROUND_TICKS: u64 = 20 * 60 * TICKS_PER_SECOND;
/// Movement axes are quantized to this magnitude on each axis.
pub const AXIS_LIMIT: i16 = 1000;
/// Binary angle of team 1's spawn heading: half a turn from team 0.
pub const HALF_TURN: u16 = 0x8000;
/// Equalized starter haste percentage used for normal GCD calculation (29 ticks at 23%).
pub const ARENA_HASTE_PERCENTAGE: u32 = Specialisation::Toxicologist.stat_template().haste() as u32;
/// Maximum ticks of authoritative target-position history retained for bounded
/// server-side targeting lag compensation (6 ticks = 200 ms at 30 Hz; ADR 0049).
pub const MAX_LAG_COMP_TICKS: u8 = 6;
const POSITION_HISTORY_LEN: usize = MAX_LAG_COMP_TICKS as usize;
pub use crate::spells::{
    INTERRUPT_LOCKOUT_TICKS as LOCKOUT_TICKS, MAX_RESOURCE as ARENA_MAX_RESOURCE,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub z: i32,
}
impl Position {
    pub fn distance_squared(self, other: Self) -> i64 {
        let x = i64::from(self.x) - i64::from(other.x);
        let z = i64::from(self.z) - i64::from(other.z);
        x * x + z * z
    }
}

/// Authoritative in-progress timed cast on a [`Combatant`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveCastState {
    pub spell: SpellId,
    pub target: Option<usize>,
    pub remaining_ticks: u32,
    pub total_ticks: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combatant {
    pub position: Position,
    pub height: i32,
    pub vertical_velocity: i32,
    /// Actual ground displacement at takeoff; no air steering or speed stacking.
    pub momentum: Position,
    pub health: u16,
    /// Current primary resource (`0..=MAX_RESOURCE`).
    pub resource: u16,
    /// Remaining ticks on the global cooldown.
    pub gcd_remaining: u32,
    /// Remaining ticks of interrupt lockout (blocks all spell casts).
    pub lockout_remaining: u32,
    /// Remaining personal cooldown ticks indexed by [`SpellId::index`].
    pub cooldowns: [u32; SpellId::COUNT],
    /// In-progress timed cast, if any.
    pub active_cast: Option<ActiveCastState>,
    /// Heading as a binary angle: 65536 units per turn, 0 faces −Z and
    /// angles grow counter-clockwise seen from above (a yaw about +Y). Every
    /// value is valid; no rule reads it yet, but it is authoritative state
    /// that snapshots relay to other players.
    pub facing: u16,
}
impl Combatant {
    /// A fresh standing fighter at `position` with full health and resources.
    pub const fn new(position: Position, facing: u16) -> Self {
        Self {
            position,
            height: 0,
            vertical_velocity: 0,
            momentum: Position { x: 0, z: 0 },
            health: MAX_HEALTH,
            resource: MAX_RESOURCE,
            gcd_remaining: 0,
            lockout_remaining: 0,
            cooldowns: [0; SpellId::COUNT],
            active_cast: None,
            facing,
        }
    }

    pub const fn cooldown_remaining(&self, spell: SpellId) -> u32 {
        self.cooldowns[spell.index()]
    }

    pub const fn is_casting(&self) -> bool {
        self.active_cast.is_some()
    }

    pub const fn is_locked_out(&self) -> bool {
        self.lockout_remaining > 0
    }

    /// Canonical combatant state invariant check shared by `Arena::restore`
    /// and wire-protocol snapshot decoding.
    pub fn is_valid(&self, roster_len: usize) -> bool {
        if self.health > MAX_HEALTH || self.resource > MAX_RESOURCE {
            return false;
        }
        if let Some(cast) = self.active_cast {
            let def = cast.spell.definition();
            if self.health == 0
                || def.cast_ticks == 0
                || cast.total_ticks == 0
                || cast.remaining_ticks == 0
                || cast.remaining_ticks > cast.total_ticks
                || cast.target.is_some_and(|t| t >= roster_len)
            {
                return false;
            }
        }
        true
    }
}

/// Planar movement intent. Each axis must lie in
/// `[-AXIS_LIMIT, AXIS_LIMIT]`; the rules normalize the length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Axes {
    pub x: i16,
    pub z: i16,
}

/// One actor's combat intent for one tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastAction {
    Cast {
        spell: SpellId,
        target: Option<usize>,
    },
    Cancel,
}

/// One actor's intent for one tick. `None`/`false` fields are idle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActorInput {
    pub movement: Option<Axes>,
    pub facing: Option<u16>,
    pub jump: bool,
    /// Spell cast or cast cancellation for this tick.
    pub action: Option<CastAction>,
    /// Server-derived bounded target-position rewind in ticks for cast-start
    /// geometry checks (`0` = present tick only; capped at [`MAX_LAG_COMP_TICKS`]
    /// inside [`Arena::advance`]). Never carries client coordinates.
    pub rewind_ticks: u8,
}
impl ActorInput {
    pub fn is_idle(&self) -> bool {
        self.movement.is_none() && self.facing.is_none() && !self.jump && self.action.is_none()
    }

    /// Convenience helper for the default Pulse attack against `target`.
    pub const fn pulse(target: usize) -> Option<CastAction> {
        Some(CastAction::Cast {
            spell: SpellId::Pulse,
            target: Some(target),
        })
    }
}

/// Convenience command list for scripts and tests; [`Arena::step`] bins it
/// into per-actor [`ActorInput`] slots. A network server builds the slots
/// itself from authenticated connections and calls [`Arena::advance`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaCommand {
    /// Axes are quantized to [-1000, 1000] and normalized by the rules.
    Move {
        actor: usize,
        x: i16,
        z: i16,
    },
    Face {
        actor: usize,
        facing: u16,
    },
    /// Shorthand for `Cast { actor, spell: SpellId::Pulse, target: Some(target) }`.
    Attack {
        actor: usize,
        target: usize,
    },
    Cast {
        actor: usize,
        spell: SpellId,
        target: Option<usize>,
    },
    CancelCast {
        actor: usize,
    },
    Jump {
        actor: usize,
    },
}
impl ArenaCommand {
    pub fn actor(self) -> usize {
        match self {
            Self::Move { actor, .. }
            | Self::Face { actor, .. }
            | Self::Attack { actor, .. }
            | Self::Cast { actor, .. }
            | Self::CancelCast { actor }
            | Self::Jump { actor } => actor,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    InvalidActor,
    InvalidTarget,
    InvalidMovement,
    /// A second command of the same kind for one actor in one tick.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptReason {
    Cancelled,
    Moved,
    Interrupted,
    Silenced,
    OutOfRange,
    LineOfSight,
    TargetDead,
    CasterDead,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoundResult {
    /// Every member of the other team is defeated. Teams are actor parity.
    Won { team: usize },
    /// Both teams were eliminated on the same tick.
    Draw,
    /// The round clock ran out while both teams still had standing fighters.
    TimeLimitDraw,
}
impl RoundResult {
    /// Whether the round ended without a winner (mutual elimination or time limit).
    pub fn is_draw(self) -> bool {
        matches!(self, Self::Draw | Self::TimeLimitDraw)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaEvent {
    Jumped {
        actor: usize,
    },
    Moved {
        actor: usize,
    },
    CastStarted {
        actor: usize,
        spell: SpellId,
        target: Option<usize>,
        duration_ticks: u32,
    },
    CastCompleted {
        actor: usize,
        spell: SpellId,
        target: Option<usize>,
    },
    CastInterrupted {
        actor: usize,
        spell: SpellId,
        reason: InterruptReason,
        by: Option<usize>,
    },
    /// `damage` is this hit's own amount against the target's health before
    /// the combat phase; simultaneous hits never see each other.
    Hit {
        actor: usize,
        target: usize,
        damage: u16,
    },
    Healed {
        actor: usize,
        target: usize,
        amount: u16,
    },
    Finished(RoundResult),
    /// `actor` is the slot (or the actor a scripted command named) whose
    /// intent was refused, so a server can route it to that player only.
    Rejected {
        actor: usize,
        reason: Rejection,
    },
}
/// Fixed-size ring buffer of past authoritative planar fighter positions for
/// bounded server-side targeting lag compensation (ADR 0049). Zero heap
/// allocations and strictly deterministic across replays.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PositionHistory {
    entries: [[Position; MAX_TEAM * 2]; POSITION_HISTORY_LEN],
    next: usize,
    len: usize,
}
impl Default for PositionHistory {
    fn default() -> Self {
        Self {
            entries: [[Position { x: 0, z: 0 }; MAX_TEAM * 2]; POSITION_HISTORY_LEN],
            next: 0,
            len: 0,
        }
    }
}
impl PositionHistory {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn push(&mut self, fighters: &[Combatant]) {
        let slot = &mut self.entries[self.next];
        *slot = [Position { x: 0, z: 0 }; MAX_TEAM * 2];
        for (dst, fighter) in slot.iter_mut().zip(fighters) {
            *dst = fighter.position;
        }
        self.next = (self.next + 1) % POSITION_HISTORY_LEN;
        if self.len < POSITION_HISTORY_LEN {
            self.len += 1;
        }
    }

    /// Returns `actor`'s authoritative position from `ticks_ago` completed ticks
    /// prior (`1..=MAX_LAG_COMP_TICKS`), or `None` if unavailable.
    fn position(&self, ticks_ago: u8, actor: usize) -> Option<Position> {
        let back = usize::from(ticks_ago);
        if back == 0 || back > self.len || actor >= MAX_TEAM * 2 {
            return None;
        }
        let index = (self.next + POSITION_HISTORY_LEN - back) % POSITION_HISTORY_LEN;
        Some(self.entries[index][actor])
    }
}

#[derive(Clone, Debug)]
pub struct Arena {
    tick: u64,
    max_round_ticks: u64,
    layout: Layout,
    fighters: Vec<Combatant>,
    result: Option<RoundResult>,
    position_history: PositionHistory,
}
impl PartialEq for Arena {
    fn eq(&self, other: &Self) -> bool {
        self.tick == other.tick
            && self.max_round_ticks == other.max_round_ticks
            && self.layout == other.layout
            && self.fighters == other.fighters
            && self.result == other.result
    }
}
impl Eq for Arena {}
impl Arena {
    /// Practice and match roster: even actors are team 0, odd actors team 1.
    /// Team `t`'s `k`th member starts at that team's `k`th room spawn, facing
    /// the other team's rooms. The layout must already be valid (see
    /// `Layout::validate`).
    pub fn practice(layout: Layout, team_size: usize) -> Option<Self> {
        if !(1..=MAX_TEAM).contains(&team_size) {
            return None;
        }
        let spawns = [layout.spawns(0), layout.spawns(1)];
        if spawns.iter().any(|team| team.len() < team_size) {
            return None;
        }
        let fighters: Vec<Combatant> = (0..team_size * 2)
            .map(|index| {
                // Team 0's rooms lie toward +Z, so it faces −Z (angle 0).
                let facing = if index % 2 == 0 { 0 } else { HALF_TURN };
                Combatant::new(spawns[index % 2][index / 2], facing)
            })
            .collect();
        let mut position_history = PositionHistory::default();
        position_history.push(&fighters);
        Some(Self {
            tick: 0,
            max_round_ticks: MAX_ROUND_TICKS,
            layout,
            fighters,
            result: None,
            position_history,
        })
    }
}
impl Arena {
    /// Rebuilds an arena from authoritative state, as a client presenting a
    /// server's snapshot does. `None` unless there are 2–6 fighters in
    /// teams of equal size with valid health, resource, and cast invariants.
    pub fn restore(
        layout: Layout,
        tick: u64,
        fighters: Vec<Combatant>,
        result: Option<RoundResult>,
    ) -> Option<Self> {
        if !valid_fighters(&fighters) {
            return None;
        }
        Some(Self {
            tick,
            max_round_ticks: MAX_ROUND_TICKS,
            layout,
            fighters,
            result,
            position_history: PositionHistory::default(),
        })
    }
    /// Sets this round's tick limit (`1..=MAX_ROUND_TICKS`), returning `None`
    /// if the limit is out of bounds.
    pub fn with_round_limit(mut self, max_round_ticks: u64) -> Option<Self> {
        self.set_round_limit(max_round_ticks).then_some(self)
    }
    /// Updates this round's tick limit (`1..=MAX_ROUND_TICKS`) in place.
    /// Returns `false` and leaves the arena unchanged if out of bounds.
    pub fn set_round_limit(&mut self, max_round_ticks: u64) -> bool {
        if max_round_ticks == 0 || max_round_ticks > MAX_ROUND_TICKS {
            return false;
        }
        self.max_round_ticks = max_round_ticks;
        true
    }
    /// Updates an arena's state without reallocating its layout or fighter
    /// buffer. Returns `false` and leaves the arena unchanged unless there
    /// are 2–6 fighters in teams of equal size with valid invariants.
    pub fn restore_in_place(
        &mut self,
        tick: u64,
        fighters: &[Combatant],
        result: Option<RoundResult>,
    ) -> bool {
        if !valid_fighters(fighters) {
            return false;
        }
        self.tick = tick;
        self.fighters.clear();
        self.fighters.extend_from_slice(fighters);
        self.result = result;
        self.position_history.clear();
        true
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    /// Total ticks allowed in this round before a [`RoundResult::TimeLimitDraw`].
    pub fn max_round_ticks(&self) -> u64 {
        self.max_round_ticks
    }
    /// Authoritative ticks remaining before the round clock expires.
    pub fn remaining_ticks(&self) -> u64 {
        self.max_round_ticks.saturating_sub(self.tick)
    }
    pub fn fighters(&self) -> &[Combatant] {
        &self.fighters
    }
    pub fn layout(&self) -> &Layout {
        &self.layout
    }
    /// Swaps in an edited, already validated layout mid-round, as the map
    /// editor's live preview does. Succeeds only when every fighter still
    /// stands in walkable space; otherwise nothing changes and the caller
    /// should start a fresh round. Health, cooldowns and the tick carry over.
    pub fn relayout(&mut self, layout: Layout) -> bool {
        if !self
            .fighters
            .iter()
            .all(|fighter| layout.walkable(fighter.position))
        {
            return false;
        }
        self.layout = layout;
        self.position_history.clear();
        true
    }
    pub fn result(&self) -> Option<RoundResult> {
        self.result
    }
    /// Read-only validation of `actor` casting `spell` at `target` (standing still).
    pub fn can_cast(
        &self,
        actor: usize,
        spell: SpellId,
        target: Option<usize>,
    ) -> Result<(), Rejection> {
        if self.result.is_some() {
            return Err(Rejection::MatchOver);
        }
        let Some(&fighter) = self.fighters.get(actor) else {
            return Err(Rejection::InvalidActor);
        };
        let def = spell.definition();
        let (combat, casting) = hydrate_casting(&fighter, actor);
        let mut health = fighter.health;
        let mut resources = BTreeMap::from([(ARENA_RESOURCE, fighter.resource)]);
        let caster = CasterContext {
            id: actor,
            class: ARENA_CLASS,
            health: &mut health,
            max_health: MAX_HEALTH,
            resources: &mut resources,
            known_spells: &SpellId::ALL,
            active_effects: &[],
            is_stunned: false,
            silence_blocks_spell: fighter.lockout_remaining > 0,
            is_moving: fighter.height > 0,
        };
        let target_ctx =
            target.and_then(|t| target_context(&self.fighters, &self.layout, actor, t));
        casting
            .can_cast(&combat, &caster, &def, target_ctx.as_ref())
            .map(|_| ())
            .map_err(rejection_from)
    }
    pub fn can_attack(&self, actor: usize, target: usize) -> Result<(), Rejection> {
        self.can_cast(actor, SpellId::Pulse, Some(target))
    }

    /// Bins a scripted command list into per-actor slots, then advances one
    /// tick. Commands naming a missing actor and second commands of a kind
    /// for the same actor are rejected first, in list order; everything else
    /// resolves exactly as [`Arena::advance`].
    pub fn step(&mut self, commands: &[ArenaCommand]) -> Vec<ArenaEvent> {
        let mut events = Vec::new();
        let count = self.fighters.len();
        let mut inputs = [ActorInput::default(); MAX_TEAM * 2];
        let inputs = &mut inputs[..count];
        for &command in commands {
            let actor = command.actor();
            let Some(slot) = inputs.get_mut(actor) else {
                events.push(ArenaEvent::Rejected {
                    actor,
                    reason: Rejection::InvalidActor,
                });
                continue;
            };
            let fresh = match command {
                ArenaCommand::Move { x, z, .. } => {
                    slot.movement.is_none() && {
                        slot.movement = Some(Axes { x, z });
                        true
                    }
                }
                ArenaCommand::Face { facing, .. } => {
                    slot.facing.is_none() && {
                        slot.facing = Some(facing);
                        true
                    }
                }
                ArenaCommand::Jump { .. } => {
                    !slot.jump && {
                        slot.jump = true;
                        true
                    }
                }
                ArenaCommand::Attack { target, .. } => {
                    slot.action.is_none() && {
                        slot.action = ActorInput::pulse(target);
                        true
                    }
                }
                ArenaCommand::Cast { spell, target, .. } => {
                    slot.action.is_none() && {
                        slot.action = Some(CastAction::Cast { spell, target });
                        true
                    }
                }
                ArenaCommand::CancelCast { .. } => {
                    slot.action.is_none() && {
                        slot.action = Some(CastAction::Cancel);
                        true
                    }
                }
            };
            if !fresh {
                events.push(ArenaEvent::Rejected {
                    actor,
                    reason: Rejection::Duplicate,
                });
            }
        }
        events.extend(self.advance(inputs));
        events
    }

    /// Advances one tick. `inputs[i]` is actor `i`'s intent; missing slots
    /// are idle and extra non-idle slots are rejected as invalid actors.
    /// Events are ordered by phase, then actor index.
    pub fn advance(&mut self, inputs: &[ActorInput]) -> Vec<ArenaEvent> {
        let mut events = Vec::new();
        let count = self.fighters.len();
        for (actor, input) in inputs.iter().enumerate().skip(count) {
            if !input.is_idle() {
                events.push(ArenaEvent::Rejected {
                    actor,
                    reason: Rejection::InvalidActor,
                });
            }
        }
        let inputs = &inputs[..inputs.len().min(count)];
        let reject = |events: &mut Vec<ArenaEvent>, actor, reason| {
            events.push(ArenaEvent::Rejected { actor, reason });
        };
        if self.result.is_some() {
            for (actor, input) in inputs.iter().enumerate() {
                let commands = usize::from(input.movement.is_some())
                    + usize::from(input.facing.is_some())
                    + usize::from(input.jump)
                    + usize::from(input.action.is_some());
                for _ in 0..commands {
                    reject(&mut events, actor, Rejection::MatchOver);
                }
            }
            self.tick = self.tick.saturating_add(1);
            return events;
        }

        let mut moving = [false; MAX_TEAM * 2];

        // 1. Existing airborne momentum resolves first. Ground momentum
        // expires without a Move, preventing stale running jumps.
        for (actor, fighter) in self.fighters.iter_mut().enumerate() {
            if fighter.height > 0 {
                fighter.position = self.layout.slide(fighter.position, fighter.momentum);
                moving[actor] = true;
            } else {
                fighter.momentum = Position { x: 0, z: 0 };
            }
        }

        // 2. Movement and facing.
        for (actor, input) in inputs.iter().enumerate() {
            let alive = self.fighters[actor].health > 0;
            if let Some(Axes { x, z }) = input.movement {
                if !alive {
                    reject(&mut events, actor, Rejection::InvalidActor);
                } else if !(-AXIS_LIMIT..=AXIS_LIMIT).contains(&x)
                    || !(-AXIS_LIMIT..=AXIS_LIMIT).contains(&z)
                {
                    reject(&mut events, actor, Rejection::InvalidMovement);
                } else if self.fighters[actor].height == 0 {
                    let length = ((i64::from(x).pow(2) + i64::from(z).pow(2)) as u64)
                        .isqrt()
                        .max(1000) as i32;
                    let dx = i32::from(x) * MOVE_PER_TICK / length;
                    let dz = i32::from(z) * MOVE_PER_TICK / length;
                    let fighter = &mut self.fighters[actor];
                    let before = fighter.position;
                    fighter.position = self.layout.slide(before, Position { x: dx, z: dz });
                    fighter.momentum = Position {
                        x: fighter.position.x - before.x,
                        z: fighter.position.z - before.z,
                    };
                    if x != 0 || z != 0 || fighter.position != before {
                        moving[actor] = true;
                    }
                    events.push(ArenaEvent::Moved { actor });
                }
            }
            // Turning is free while alive, including in the air; the dead
            // keep the heading they fell with.
            if let Some(facing) = input.facing
                && alive
            {
                self.fighters[actor].facing = facing;
            }
        }

        // 3. Jumps.
        for (actor, input) in inputs.iter().enumerate() {
            if !input.jump {
                continue;
            }
            let fighter = &mut self.fighters[actor];
            if fighter.health == 0 {
                reject(&mut events, actor, Rejection::InvalidActor);
            } else if fighter.height > 0 || fighter.vertical_velocity > 0 {
                reject(&mut events, actor, Rejection::Airborne);
            } else {
                fighter.vertical_velocity = JUMP_SPEED;
                moving[actor] = true;
                events.push(ArenaEvent::Jumped { actor });
            }
        }

        // 4. Combat, validated against one shared post-movement state and
        // applied together so actor order never breaks ties.
        let mut pre_combat = [Combatant::new(Position { x: 0, z: 0 }, 0); MAX_TEAM * 2];
        pre_combat[..count].copy_from_slice(&self.fighters);
        let pre_combat = &pre_combat[..count];

        let mut incoming_damage = [0u16; MAX_TEAM * 2];
        let mut heals = [(0usize, 0usize, 0u16); MAX_TEAM * 4];
        let mut heal_count = 0usize;
        let mut still_casting_after_4a = [false; MAX_TEAM * 2];
        let mut interrupts = [None::<(usize, u32)>; MAX_TEAM * 2];

        // 4a. Explicit cancellations and in-progress timed casts.
        // Collect completed casts first so `still_casting_after_4a` is finalized
        // across every actor before any completion effects are queued.
        let mut completed_casts: Vec<(usize, PreparedCast)> = Vec::new();
        for actor in 0..count {
            if inputs
                .get(actor)
                .is_some_and(|input| matches!(input.action, Some(CastAction::Cancel)))
            {
                if self.fighters[actor].health == 0 {
                    reject(&mut events, actor, Rejection::InvalidActor);
                } else if let Some(active) = self.fighters[actor].active_cast.take() {
                    events.push(ArenaEvent::CastInterrupted {
                        actor,
                        spell: active.spell,
                        reason: InterruptReason::Cancelled,
                        by: None,
                    });
                } else {
                    reject(&mut events, actor, Rejection::NotCasting);
                }
                continue;
            }

            let Some(active) = self.fighters[actor].active_cast else {
                continue;
            };
            let (mut combat, mut casting) = hydrate_casting(&self.fighters[actor], actor);
            let mut health = self.fighters[actor].health;
            let mut resources = BTreeMap::from([(ARENA_RESOURCE, self.fighters[actor].resource)]);
            let mut caster = CasterContext {
                id: actor,
                class: ARENA_CLASS,
                health: &mut health,
                max_health: MAX_HEALTH,
                resources: &mut resources,
                known_spells: &SpellId::ALL,
                active_effects: &[],
                is_stunned: false,
                silence_blocks_spell: self.fighters[actor].lockout_remaining > 0,
                is_moving: moving[actor],
            };
            let target_ctx = active
                .target
                .and_then(|t| target_context(pre_combat, &self.layout, actor, t));
            match casting.tick(&mut combat, &mut caster, target_ctx.as_ref()) {
                Ok(Some(prepared)) => {
                    // Timed casts pay their costs only when they complete.
                    self.fighters[actor].health = health;
                    self.fighters[actor].resource =
                        resources.get(&ARENA_RESOURCE).copied().unwrap_or(0);
                    self.fighters[actor].active_cast = None;
                    self.fighters[actor].cooldowns[active.spell.index()] =
                        combat.cooldown_remaining(active.spell);
                    events.push(ArenaEvent::CastCompleted {
                        actor,
                        spell: active.spell,
                        target: prepared.target_id,
                    });
                    completed_casts.push((actor, prepared));
                }
                Ok(None) => {
                    if let Some(updated) = combat.active_cast() {
                        self.fighters[actor].active_cast = Some(ActiveCastState {
                            spell: active.spell,
                            target: active.target,
                            remaining_ticks: updated.remaining_ticks,
                            total_ticks: updated.total_ticks,
                        });
                        still_casting_after_4a[actor] = true;
                    } else {
                        self.fighters[actor].active_cast = None;
                    }
                }
                Err(error) => {
                    self.fighters[actor].active_cast = None;
                    events.push(ArenaEvent::CastInterrupted {
                        actor,
                        spell: active.spell,
                        reason: interrupt_reason_from(error),
                        by: None,
                    });
                }
            }
        }
        for (actor, prepared) in &completed_casts {
            queue_spell_effects(
                *actor,
                prepared,
                pre_combat,
                &mut incoming_damage,
                &mut heals,
                &mut heal_count,
                &mut interrupts,
                &still_casting_after_4a,
                &mut events,
            );
        }

        // 4b. New spell intents.
        for (actor, input) in inputs.iter().enumerate() {
            let Some(CastAction::Cast { spell, target }) = input.action else {
                continue;
            };
            let def = spell.definition();
            let (mut combat, mut casting) = hydrate_casting(&self.fighters[actor], actor);
            let mut health = self.fighters[actor].health;
            let mut resources = BTreeMap::from([(ARENA_RESOURCE, self.fighters[actor].resource)]);
            let mut caster = CasterContext {
                id: actor,
                class: ARENA_CLASS,
                health: &mut health,
                max_health: MAX_HEALTH,
                resources: &mut resources,
                known_spells: &SpellId::ALL,
                active_effects: &[],
                is_stunned: false,
                silence_blocks_spell: self.fighters[actor].lockout_remaining > 0,
                is_moving: moving[actor],
            };
            let target_ctx = target.and_then(|t| {
                target_context_with_lag_comp(
                    pre_combat,
                    &self.position_history,
                    &self.layout,
                    actor,
                    t,
                    def.range,
                    input.rewind_ticks,
                )
            });
            match casting.try_cast(&mut combat, &mut caster, &def, target_ctx.as_ref()) {
                Err(error) => {
                    reject(&mut events, actor, rejection_from(error));
                }
                Ok(None) => {
                    let pending_target = casting.pending_target_id();
                    let active = combat.active_cast().copied().expect("timed cast started");
                    let fighter = &mut self.fighters[actor];
                    fighter.health = health;
                    fighter.resource = resources.get(&ARENA_RESOURCE).copied().unwrap_or(0);
                    fighter.gcd_remaining = combat.gcd_remaining();
                    fighter.active_cast = Some(ActiveCastState {
                        spell,
                        target: pending_target,
                        remaining_ticks: active.remaining_ticks,
                        total_ticks: active.total_ticks,
                    });
                    events.push(ArenaEvent::CastStarted {
                        actor,
                        spell,
                        target: pending_target,
                        duration_ticks: active.total_ticks,
                    });
                }
                Ok(Some(prepared)) => {
                    let fighter = &mut self.fighters[actor];
                    fighter.health = health;
                    fighter.resource = resources.get(&ARENA_RESOURCE).copied().unwrap_or(0);
                    fighter.gcd_remaining = combat.gcd_remaining();
                    fighter.cooldowns[spell.index()] = combat.cooldown_remaining(spell);
                    if def
                        .effects
                        .iter()
                        .any(|effect| matches!(effect, SpellEffect::Interrupt { .. }))
                    {
                        events.push(ArenaEvent::CastCompleted {
                            actor,
                            spell,
                            target: prepared.target_id,
                        });
                    }
                    queue_spell_effects(
                        actor,
                        &prepared,
                        pre_combat,
                        &mut incoming_damage,
                        &mut heals,
                        &mut heal_count,
                        &mut interrupts,
                        &still_casting_after_4a,
                        &mut events,
                    );
                }
            }
        }

        // Record this tick's authoritative post-movement positions so subsequent
        // ticks can perform bounded server-side targeting lag compensation.
        self.position_history.push(pre_combat);

        // 4c. Apply interrupts, damage, healing, death interrupts, and check round end.
        for (target, pending) in interrupts[..count].iter().enumerate() {
            if let Some((by, lockout_ticks)) = *pending
                && let Some(active) = self.fighters[target].active_cast.take()
            {
                let fighter = &mut self.fighters[target];
                fighter.lockout_remaining = fighter.lockout_remaining.max(lockout_ticks);
                events.push(ArenaEvent::CastInterrupted {
                    actor: target,
                    spell: active.spell,
                    reason: InterruptReason::Interrupted,
                    by: Some(by),
                });
            }
        }
        for (fighter, &damage) in self.fighters.iter_mut().zip(&incoming_damage[..count]) {
            fighter.health = fighter.health.saturating_sub(damage);
        }
        for &(actor, target, amount) in &heals[..heal_count] {
            let restored =
                effects::apply_heal(&mut self.fighters[target].health, MAX_HEALTH, amount);
            if restored > 0 {
                events.push(ArenaEvent::Healed {
                    actor,
                    target,
                    amount: restored,
                });
            }
        }
        for (actor, fighter) in self.fighters.iter_mut().enumerate() {
            if fighter.health == 0
                && let Some(active) = fighter.active_cast.take()
            {
                events.push(ArenaEvent::CastInterrupted {
                    actor,
                    spell: active.spell,
                    reason: InterruptReason::CasterDead,
                    by: None,
                });
            }
        }

        let standing = |team: usize| {
            self.fighters
                .iter()
                .enumerate()
                .any(|(index, fighter)| index % 2 == team && fighter.health > 0)
        };
        self.result = match (standing(0), standing(1)) {
            // Time is up after this tick: nobody won.
            (true, true) if self.tick.saturating_add(1) >= self.max_round_ticks => {
                Some(RoundResult::TimeLimitDraw)
            }
            (true, true) => None,
            (true, false) => Some(RoundResult::Won { team: 0 }),
            (false, true) => Some(RoundResult::Won { team: 1 }),
            (false, false) => Some(RoundResult::Draw),
        };
        if let Some(result) = self.result {
            events.push(ArenaEvent::Finished(result));
            self.tick = self.tick.saturating_add(1);
            return events;
        }

        // 5. Vertical motion.
        for fighter in &mut self.fighters {
            if fighter.height > 0 || fighter.vertical_velocity > 0 {
                fighter.height = (fighter.height + fighter.vertical_velocity).max(0);
                fighter.vertical_velocity -= GRAVITY;
                if fighter.height == 0 {
                    fighter.vertical_velocity = 0;
                    fighter.momentum = Position { x: 0, z: 0 };
                }
            }
        }

        // 6. End-of-tick timer decay and passive resource regeneration.
        let next_tick = self.tick.saturating_add(1);
        let regen = next_tick.is_multiple_of(RESOURCE_REGEN_INTERVAL);
        for fighter in &mut self.fighters {
            fighter.gcd_remaining = fighter.gcd_remaining.saturating_sub(1);
            fighter.lockout_remaining = fighter.lockout_remaining.saturating_sub(1);
            for cd in &mut fighter.cooldowns {
                *cd = cd.saturating_sub(1);
            }
            if regen && fighter.health > 0 {
                effects::gain_resource(&mut fighter.resource, MAX_RESOURCE, RESOURCE_REGEN_AMOUNT);
            }
        }
        self.tick = next_tick;
        events
    }

    /// Deterministic 64-bit FNV-1a digest of the tick, result and every
    /// fighter, for comparing a server's state with a replay or a client's
    /// prediction. The layout is fixed per match and versioned separately.
    pub fn state_hash(&self) -> u64 {
        Self::hash_state(self.tick, &self.fighters, self.result)
    }

    /// Computes [`Self::state_hash`] directly from components without needing
    /// an [`Arena`] or cloning its layout.
    pub fn hash_state(tick: u64, fighters: &[Combatant], result: Option<RoundResult>) -> u64 {
        let mut hash = Fnv::default();
        hash.write(&tick.to_le_bytes());
        hash.write(&match result {
            None => [0, 0],
            Some(RoundResult::Won { team }) => [1, team as u8],
            Some(RoundResult::Draw) => [2, 0],
            Some(RoundResult::TimeLimitDraw) => [2, 1],
        });
        for fighter in fighters {
            for value in [
                fighter.position.x,
                fighter.position.z,
                fighter.height,
                fighter.vertical_velocity,
                fighter.momentum.x,
                fighter.momentum.z,
            ] {
                hash.write(&value.to_le_bytes());
            }
            hash.write(&fighter.health.to_le_bytes());
            hash.write(&fighter.resource.to_le_bytes());
            hash.write(&fighter.gcd_remaining.to_le_bytes());
            hash.write(&fighter.lockout_remaining.to_le_bytes());
            for cd in fighter.cooldowns {
                hash.write(&cd.to_le_bytes());
            }
            match fighter.active_cast {
                None => hash.write(&[0]),
                Some(cast) => {
                    hash.write(&[1, cast.spell as u8]);
                    match cast.target {
                        None => hash.write(&[0]),
                        Some(target) => hash.write(&[1, target as u8]),
                    }
                    hash.write(&cast.remaining_ticks.to_le_bytes());
                    hash.write(&cast.total_ticks.to_le_bytes());
                }
            }
            hash.write(&fighter.facing.to_le_bytes());
        }
        hash.0
    }
}

fn valid_fighters(fighters: &[Combatant]) -> bool {
    if !(2..=MAX_TEAM * 2).contains(&fighters.len()) || !fighters.len().is_multiple_of(2) {
        return false;
    }
    fighters
        .iter()
        .all(|fighter| fighter.is_valid(fighters.len()))
}

fn hydrate_casting(fighter: &Combatant, actor: usize) -> (CombatState, CastingState) {
    let mut cooldowns = BTreeMap::new();
    for spell in SpellId::ALL {
        let remaining = fighter.cooldown_remaining(spell);
        if remaining > 0 {
            cooldowns.insert(spell, remaining);
        }
    }
    let (active_cast, pending) = match fighter.active_cast {
        Some(cast) => {
            let def = cast.spell.definition();
            (
                Some(ActiveCast::new(
                    cast.spell,
                    cast.remaining_ticks,
                    cast.total_ticks,
                    def.cooldown_ticks,
                )),
                Some(PreparedCast {
                    spell: def,
                    caster_id: actor,
                    target_id: cast.target,
                }),
            )
        }
        None => (None, None),
    };
    let combat = CombatState::from_parts(
        fighter.gcd_remaining,
        cooldowns,
        active_cast,
        ARENA_HASTE_PERCENTAGE,
        100,
    );
    (combat, CastingState::from_pending(pending))
}

fn target_context(
    fighters: &[Combatant],
    layout: &Layout,
    actor: usize,
    target: usize,
) -> Option<TargetContext<'static>> {
    let attacker = fighters.get(actor)?;
    let defender = fighters.get(target)?;
    Some(TargetContext {
        id: target,
        is_friendly: actor % 2 == target % 2,
        health: defender.health,
        max_health: MAX_HEALTH,
        distance_squared: attacker.position.distance_squared(defender.position) as u64,
        has_line_of_sight: actor == target
            || layout.line_of_sight(attacker.position, defender.position),
        is_stunned: false,
        active_effects: &[],
    })
}

/// Validates target geometry for cast starts, checking present-tick `pre_combat`
/// first and falling back to at most two bounded server-recorded target
/// positions (`clamped` and `clamped - 1` ticks ago, matching a client's
/// `[floor(view_tick), ceil(view_tick)]` interpolation bracket).
///
/// Both range and line of sight must hold simultaneously at the same historical
/// tick from the caster's present authoritative position; target health/liveness
/// and team identity always stay strictly present-tick.
fn target_context_with_lag_comp(
    fighters: &[Combatant],
    history: &PositionHistory,
    layout: &Layout,
    actor: usize,
    target: usize,
    spell_range: u16,
    rewind_ticks: u8,
) -> Option<TargetContext<'static>> {
    let present = target_context(fighters, layout, actor, target)?;
    if rewind_ticks == 0 || actor == target || present.health == 0 {
        return Some(present);
    }
    let range_sq = u64::from(spell_range).pow(2);
    if present.distance_squared <= range_sq && present.has_line_of_sight {
        return Some(present);
    }
    let attacker = fighters[actor].position;
    let clamped = rewind_ticks.min(MAX_LAG_COMP_TICKS);
    let candidates = if clamped > 1 {
        [Some(clamped), Some(clamped - 1)]
    } else {
        [Some(clamped), None]
    };
    for ticks_ago in candidates.into_iter().flatten() {
        let Some(hist_pos) = history.position(ticks_ago, target) else {
            continue;
        };
        let dist_sq = attacker.distance_squared(hist_pos) as u64;
        if dist_sq <= range_sq && layout.line_of_sight(attacker, hist_pos) {
            return Some(TargetContext {
                distance_squared: dist_sq,
                has_line_of_sight: true,
                ..present
            });
        }
    }
    Some(present)
}

#[allow(clippy::too_many_arguments)]
fn queue_spell_effects(
    actor: usize,
    prepared: &PreparedCast,
    pre_combat: &[Combatant],
    incoming_damage: &mut [u16; MAX_TEAM * 2],
    heals: &mut [(usize, usize, u16); MAX_TEAM * 4],
    heal_count: &mut usize,
    interrupts: &mut [Option<(usize, u32)>; MAX_TEAM * 2],
    still_casting_after_4a: &[bool; MAX_TEAM * 2],
    events: &mut Vec<ArenaEvent>,
) {
    let Some(target) = prepared.target_id else {
        return;
    };
    for effect in &prepared.spell.effects {
        match *effect {
            SpellEffect::Damage { amount, .. } => {
                let damage = amount.min(pre_combat[target].health);
                incoming_damage[target] = incoming_damage[target].saturating_add(damage);
                events.push(ArenaEvent::Hit {
                    actor,
                    target,
                    damage,
                });
            }
            SpellEffect::Heal { amount } => {
                if *heal_count < heals.len() {
                    heals[*heal_count] = (actor, target, amount);
                    *heal_count += 1;
                }
            }
            SpellEffect::Interrupt { lockout_ticks } if still_casting_after_4a[target] => {
                match &mut interrupts[target] {
                    Some((_, existing)) => {
                        *existing = (*existing).max(lockout_ticks);
                    }
                    slot @ None => {
                        *slot = Some((actor, lockout_ticks));
                    }
                }
            }
            _ => {}
        }
    }
}

fn rejection_from(error: CastError) -> Rejection {
    match error {
        CastError::CasterDead | CastError::InvalidHealth | CastError::WrongClass => {
            Rejection::InvalidActor
        }
        CastError::InvalidTarget | CastError::TargetDead | CastError::UnknownSpell => {
            Rejection::InvalidTarget
        }
        CastError::Timing(CastStateError::AlreadyCasting) => Rejection::AlreadyCasting,
        CastError::Timing(CastStateError::SpellCooldown { .. }) => Rejection::Cooldown,
        CastError::Timing(CastStateError::GlobalCooldown { .. }) => Rejection::GlobalCooldown,
        CastError::Timing(CastStateError::PassiveSpell) => Rejection::InvalidTarget,
        CastError::Silenced | CastError::Stunned => Rejection::Silenced,
        CastError::Moving => Rejection::Moving,
        CastError::OutOfRange => Rejection::OutOfRange,
        CastError::NoLineOfSight => Rejection::LineOfSight,
        CastError::NotEnoughResource(_)
        | CastError::InvalidResourceCost
        | CastError::NotEnoughHealth => Rejection::NotEnoughResource,
        CastError::ConditionNotMet | CastError::StateMismatch => Rejection::InvalidTarget,
    }
}

fn interrupt_reason_from(error: CastError) -> InterruptReason {
    match error {
        CastError::Moving => InterruptReason::Moved,
        CastError::Silenced | CastError::Stunned => InterruptReason::Silenced,
        CastError::OutOfRange => InterruptReason::OutOfRange,
        CastError::NoLineOfSight => InterruptReason::LineOfSight,
        CastError::TargetDead | CastError::InvalidTarget => InterruptReason::TargetDead,
        CastError::CasterDead | CastError::InvalidHealth => InterruptReason::CasterDead,
        _ => InterruptReason::Cancelled,
    }
}

struct Fnv(u64);
impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}
impl Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::map::{Pillar, fixture};
    /// Floor half-extents of the synthetic fixture layout.
    const ARENA_X: i32 = 54_000;
    const ARENA_Z: i32 = 42_000;
    fn default_arena() -> Arena {
        Arena::practice(fixture(), 1).unwrap()
    }
    fn pillars() -> Vec<Pillar> {
        fixture().pillars
    }
    fn line_of_sight(a: Position, b: Position) -> bool {
        fixture().line_of_sight(a, b)
    }
    fn rejected(actor: usize, reason: Rejection) -> ArenaEvent {
        ArenaEvent::Rejected { actor, reason }
    }
    #[test]
    fn relayout_keeps_the_round_only_while_every_fighter_stays_walkable() {
        let mut arena = default_arena();
        arena.fighters[0].position = Position { x: 0, z: 0 };
        arena.fighters[0].health = 40;
        let mut moved = fixture();
        moved.pillars[0].center.x += 2_000;
        assert!(arena.relayout(moved.clone()));
        assert_eq!(arena.layout(), &moved);
        assert_eq!(arena.fighters()[0].health, 40);
        let mut covering = moved.clone();
        covering.pillars[0].center = Position { x: 0, z: 0 };
        assert!(!arena.relayout(covering));
        assert_eq!(arena.layout(), &moved, "a rejected layout changes nothing");
    }
    #[test]
    fn practice_sizes_reject_friendly_fire_and_finish_only_after_team_elimination() {
        assert!(Arena::practice(fixture(), 0).is_none());
        assert!(Arena::practice(fixture(), 4).is_none());
        for size in 1..=3 {
            let mut arena = Arena::practice(fixture(), size).unwrap();
            assert_eq!(arena.fighters().len(), size * 2);
            for fighter in &mut arena.fighters {
                fighter.position = Position { x: 0, z: 0 };
            }
            let mut replay = arena.clone();
            let mut commands = Vec::new();
            if size > 1 {
                let before = arena.clone();
                assert_eq!(arena.can_attack(0, 2), Err(Rejection::InvalidTarget));
                assert_eq!(
                    arena.step(&[ArenaCommand::Attack {
                        actor: 0,
                        target: 2
                    }]),
                    vec![rejected(0, Rejection::InvalidTarget)]
                );
                assert_eq!(arena.fighters, before.fighters);
                replay = arena.clone();
            }
            for target in (1..size * 2).step_by(2) {
                for _ in 0..4 {
                    while arena.fighters[0].cooldown_remaining(SpellId::Pulse) > 0 {
                        commands.push(Vec::new());
                        arena.step(&[]);
                    }
                    let command = vec![ArenaCommand::Attack { actor: 0, target }];
                    arena.step(&command);
                    commands.push(command);
                }
                assert_eq!(
                    arena.result(),
                    (target == size * 2 - 1).then_some(RoundResult::Won { team: 0 })
                );
                if arena.result().is_none() {
                    assert_eq!(arena.can_attack(0, target), Err(Rejection::InvalidTarget));
                    assert_eq!(arena.can_attack(target, 0), Err(Rejection::InvalidActor));
                    let frozen = arena.fighters[target];
                    arena.step(&[
                        ArenaCommand::Move {
                            actor: target,
                            x: 1000,
                            z: 0,
                        },
                        ArenaCommand::Jump { actor: target },
                    ]);
                    commands.push(vec![
                        ArenaCommand::Move {
                            actor: target,
                            x: 1000,
                            z: 0,
                        },
                        ArenaCommand::Jump { actor: target },
                    ]);
                    assert_eq!(arena.fighters[target], frozen);
                }
            }
            for command in commands {
                replay.step(&command);
            }
            assert_eq!(arena, replay);
        }
    }
    use super::*;
    fn forward() -> ArenaCommand {
        ArenaCommand::Move {
            actor: 0,
            x: 0,
            z: -1000,
        }
    }
    const ATTACK: ArenaCommand = ArenaCommand::Attack {
        actor: 0,
        target: 1,
    };
    const JUMP: ArenaCommand = ArenaCommand::Jump { actor: 0 };

    #[test]
    fn jump_preserves_takeoff_momentum_until_exact_landing() {
        let mut arena = default_arena();
        arena.step(&[forward(), JUMP]);
        let takeoff = arena.fighters[0].position;
        assert_eq!(arena.fighters[0].height, JUMP_SPEED);
        let mut apex = JUMP_SPEED;
        let mut ticks = 1;
        while arena.fighters[0].height > 0 {
            // Opposite movement and repeated jump cannot steer, brake or boost.
            assert!(
                arena
                    .step(&[
                        ArenaCommand::Move {
                            actor: 0,
                            x: 0,
                            z: 1000
                        },
                        JUMP
                    ])
                    .contains(&rejected(0, Rejection::Airborne))
            );
            ticks += 1;
            apex = apex.max(arena.fighters[0].height);
            assert!(ticks < 30);
        }
        assert_eq!(ticks, 23);
        assert_eq!(apex, 1320);
        assert_eq!(arena.fighters[0].position.z, takeoff.z - 22 * MOVE_PER_TICK);
        assert_eq!(arena.fighters[0].vertical_velocity, 0);
        let landed = arena.fighters[0].position;
        arena.step(&[]);
        assert_eq!(arena.fighters[0].position, landed);
        assert_eq!(arena.step(&[JUMP]), vec![ArenaEvent::Jumped { actor: 0 }]);
    }

    #[test]
    fn stationary_jump_has_no_stale_momentum_and_cannot_double_jump() {
        let mut arena = default_arena();
        arena.step(&[forward()]);
        let before = arena.fighters[0].position;
        assert_eq!(
            arena.step(&[JUMP, JUMP]),
            vec![
                rejected(0, Rejection::Duplicate),
                ArenaEvent::Jumped { actor: 0 }
            ]
        );
        for _ in 0..30 {
            arena.step(&[]);
        }
        assert_eq!(arena.fighters[0].position, before);
        assert_eq!(arena.fighters[0].height, 0);
        assert_eq!(
            arena.step(&[ArenaCommand::Jump { actor: usize::MAX }]),
            vec![rejected(usize::MAX, Rejection::InvalidActor)]
        );
        arena.result = Some(RoundResult::Won { team: 1 });
        assert_eq!(arena.step(&[JUMP]), vec![rejected(0, Rejection::MatchOver)]);
    }

    #[test]
    fn jumping_keeps_planar_combat_and_airborne_command_budgets() {
        let mut arena = default_arena();
        arena.fighters[0].position = Position { x: 0, z: 0 };
        arena.fighters[1].position = Position { x: 0, z: -3_000 };
        arena.step(&[forward(), JUMP]);
        assert_eq!(arena.can_attack(0, 1), Ok(()));
        let before = arena.fighters[0].position;
        let events = arena.step(&[
            ArenaCommand::Move {
                actor: 0,
                x: i16::MAX,
                z: 0,
            },
            forward(),
            forward(),
            ArenaCommand::Attack {
                actor: 1,
                target: 0,
            },
        ]);
        // One movement per slot: the first (malformed) one is kept and
        // rejected, so the extra ones cannot replace it.
        assert_eq!(
            events,
            vec![
                rejected(0, Rejection::Duplicate),
                rejected(0, Rejection::Duplicate),
                rejected(0, Rejection::InvalidMovement),
                ArenaEvent::Hit {
                    actor: 1,
                    target: 0,
                    damage: ATTACK_DAMAGE
                },
            ]
        );
        assert_eq!(arena.fighters[0].health, 75);
        assert_eq!(arena.fighters[0].position.z, before.z - MOVE_PER_TICK);
        assert!(arena.fighters[0].height > 0);
        arena.result = Some(RoundResult::Won { team: 1 });
        let frozen = arena.fighters.clone();
        arena.step(&[forward(), JUMP]);
        assert_eq!(arena.fighters, frozen);
        arena = default_arena();
        assert!(arena.fighters.iter().all(|f| f.height == 0
            && f.vertical_velocity == 0
            && f.momentum == Position { x: 0, z: 0 }));
    }

    #[test]
    fn airborne_collision_speed_and_replay_are_bounded() {
        for start in [
            Position {
                x: ARENA_X - BODY_RADIUS - 200,
                z: 0,
            },
            // Running diagonally into the far corner pillar.
            Position {
                x: pillars()[3].center.x - pillars()[3].half_x - BODY_RADIUS - 500,
                z: pillars()[3].center.z,
            },
        ] {
            let mut a = default_arena();
            a.fighters[0].position = start;
            let mut b = a.clone();
            let mut previous = start;
            for tick in 0..35 {
                let mut commands = vec![ArenaCommand::Move {
                    actor: 0,
                    x: 1000,
                    z: 1000,
                }];
                if tick == 0 {
                    commands.push(JUMP);
                }
                assert_eq!(a.step(&commands), b.step(&commands));
                assert_eq!(a, b);
                let fighter = a.fighters[0];
                assert!(
                    fighter.position.distance_squared(previous) <= i64::from(MOVE_PER_TICK).pow(2)
                );
                assert!(fighter.position.x <= ARENA_X - BODY_RADIUS);
                assert!(fighter.position.z <= ARENA_Z - BODY_RADIUS);
                assert!(
                    !pillars()
                        .iter()
                        .any(|p| p.contains(fighter.position, BODY_RADIUS))
                );
                previous = fighter.position;
            }
        }
    }
    #[test]
    fn attack_checks_range_cooldown_and_match_end() {
        let mut arena = default_arena();
        assert_eq!(
            arena.step(&[ATTACK]),
            vec![rejected(0, Rejection::OutOfRange)]
        );
        let gap = arena.fighters()[0].position.z - arena.fighters()[1].position.z;
        for _ in 0..(gap - ATTACK_RANGE) / MOVE_PER_TICK + 1 {
            arena.step(&[forward()]);
        }
        arena.step(&[ATTACK]);
        assert_eq!(arena.fighters()[1].health, 75);
        assert_eq!(
            arena.fighters()[0].cooldown_remaining(SpellId::Pulse),
            PULSE_COOLDOWN_TICKS - 1
        );
        while arena.fighters()[0].cooldown_remaining(SpellId::Pulse) > 0 {
            assert_eq!(
                arena.step(&[ATTACK]),
                vec![rejected(0, Rejection::Cooldown)]
            );
        }
        for _ in 0..3 {
            while arena.fighters()[0].cooldown_remaining(SpellId::Pulse) > 0 {
                arena.step(&[]);
            }
            arena.step(&[ATTACK]);
        }
        assert_eq!(arena.result(), Some(RoundResult::Won { team: 0 }));
        assert_eq!(
            arena.step(&[forward(), ATTACK]),
            vec![rejected(0, Rejection::MatchOver); 2]
        );
        assert_eq!(arena.fighters()[1].health, 0);
    }
    #[test]
    fn invalid_inputs_and_diagonal_speed_are_bounded() {
        let mut arena = default_arena();
        assert_eq!(
            arena.step(&[
                ArenaCommand::Move {
                    actor: usize::MAX,
                    x: 0,
                    z: 0
                },
                ArenaCommand::Move {
                    actor: 0,
                    x: i16::MAX,
                    z: 0
                },
                ArenaCommand::Attack {
                    actor: 0,
                    target: 0
                }
            ]),
            vec![
                rejected(usize::MAX, Rejection::InvalidActor),
                rejected(0, Rejection::InvalidMovement),
                rejected(0, Rejection::InvalidTarget)
            ]
        );
        let before = arena.fighters()[0].position;
        arena.step(
            &[ArenaCommand::Move {
                actor: 0,
                x: 1000,
                z: 1000,
            }; 5],
        );
        assert!(
            arena.fighters()[0].position.distance_squared(before)
                <= i64::from(MOVE_PER_TICK).pow(2)
        );
    }
    #[test]
    fn pillars_block_movement_and_attacks() {
        for pillar in pillars() {
            for (x, z) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let mut arena = default_arena();
                let offset = pillar.half_x + 1_000;
                arena.fighters[0].position = Position {
                    x: pillar.center.x + x * offset,
                    z: pillar.center.z + z * offset,
                };
                arena.fighters[1].position = Position {
                    x: pillar.center.x - x * offset,
                    z: pillar.center.z - z * offset,
                };
                assert!(!line_of_sight(
                    arena.fighters()[0].position,
                    arena.fighters()[1].position
                ));
                for _ in 0..100 {
                    arena.step(&[ArenaCommand::Move {
                        actor: 0,
                        x: -1000 * x as i16,
                        z: -1000 * z as i16,
                    }]);
                    assert!(!pillar.contains(arena.fighters()[0].position, BODY_RADIUS));
                }
                let stopped = arena.fighters()[0].position;
                let clearance =
                    (stopped.x - pillar.center.x).abs() + (stopped.z - pillar.center.z).abs();
                assert!(clearance > pillar.half_x + BODY_RADIUS);
                assert!(clearance <= pillar.half_x + BODY_RADIUS + MOVE_PER_TICK);
            }
            // Within range, a sight line clipping a hexagon corner is still
            // rejected, while one passing beside it, where a square pillar's
            // corner used to be, is clear.
            let mut arena = default_arena();
            let (cx, cz, h) = (pillar.center.x, pillar.center.z, pillar.half_x);
            arena.fighters[0].position = Position {
                x: cx + h - 300,
                z: cz - 2_000,
            };
            arena.fighters[1].position = Position {
                x: cx + h - 300,
                z: cz + 2_000,
            };
            assert_eq!(arena.can_attack(0, 1), Err(Rejection::LineOfSight));
            arena.fighters[0].position = Position {
                x: cx + h - 300,
                z: cz + pillar.half_z - 500,
            };
            arena.fighters[1].position = Position {
                x: cx + h - 2_300,
                z: cz + pillar.half_z + 1_500,
            };
            assert_eq!(arena.can_attack(0, 1), Ok(()));
            // A segment touching the edge is blocked too.
            assert!(!line_of_sight(
                Position {
                    x: pillar.center.x + pillar.half_x,
                    z: pillar.center.z + pillar.half_x + 1_000
                },
                Position {
                    x: pillar.center.x + pillar.half_x,
                    z: pillar.center.z - pillar.half_x - 1_000
                }
            ));
        }
        assert!(line_of_sight(
            Position { x: 0, z: ARENA_Z },
            Position { x: 0, z: -ARENA_Z }
        ));
        // Sight lines that touch the pit wall are blocked, like pillar edges.
        assert!(line_of_sight(
            Position {
                x: -ARENA_X + 1,
                z: 0
            },
            Position {
                x: ARENA_X - 1,
                z: 0
            }
        ));
        assert!(!line_of_sight(
            Position { x: -ARENA_X, z: 0 },
            Position { x: ARENA_X, z: 0 }
        ));
    }
    #[test]
    fn hexagonal_pillars_let_bodies_slide_along_faces_and_round_corners() {
        let pillar = pillars()[0];
        let Position { x: cx, z: cz } = pillar.center;
        let run = |start: Position, x: i16, z: i16, ticks: usize| {
            let mut arena = default_arena();
            arena.fighters[0].position = start;
            let mut previous = start;
            for _ in 0..ticks {
                arena.step(&[ArenaCommand::Move { actor: 0, x, z }]);
                let now = arena.fighters()[0].position;
                assert!(!pillar.contains(now, BODY_RADIUS), "{now:?}");
                assert!(now.distance_squared(previous) <= i64::from(MOVE_PER_TICK).pow(2));
                previous = now;
            }
            previous
        };
        // Clipping the corner on the pillar's +X side while running north
        // to south: the body rounds it and carries on past the pillar.
        let past = run(
            Position {
                x: cx + pillar.half_x + BODY_RADIUS - 300,
                z: cz - 4_000,
            },
            0,
            1000,
            40,
        );
        assert!(past.z > cz + 4_000, "{past:?}");
        // Pressing diagonally into a slanted face keeps most of the speed
        // along the face instead of stopping dead.
        let start = Position {
            x: cx + 3_400 + BODY_RADIUS + 400,
            z: cz + 2_200 + BODY_RADIUS + 400,
        };
        let end = run(start, -1000, 300, 20);
        let travelled = end.distance_squared(start).isqrt();
        assert!(travelled > 20 * i64::from(MOVE_PER_TICK) / 3, "{travelled}");
    }
    #[test]
    fn replay_and_world_bounds_are_repeatable() {
        // Use the open middle lane so this tests the wall, not a pillar.
        let mut a = default_arena();
        a.fighters[0].position = Position { x: 0, z: 0 };
        let mut b = default_arena();
        b.fighters[0].position = Position { x: 0, z: 0 };
        for _ in 0..ARENA_X / MOVE_PER_TICK + 10 {
            let commands = [
                ArenaCommand::Move {
                    actor: 0,
                    x: 1000,
                    z: 0,
                },
                ATTACK,
            ];
            assert_eq!(a.step(&commands), b.step(&commands));
            assert_eq!(a, b);
        }
        assert_eq!(a.fighters()[0].position.x, ARENA_X - BODY_RADIUS);
    }

    #[test]
    fn all_walls_bound_movement_and_spawns_are_clear() {
        for size in 1..=3 {
            for (index, fighter) in Arena::practice(fixture(), size)
                .unwrap()
                .fighters()
                .iter()
                .enumerate()
            {
                assert!(fixture().walkable(fighter.position));
                // Every team starts in its own room, behind the pit wall.
                let sign = if index % 2 == 0 { 1 } else { -1 };
                assert!(fighter.position.z * sign > ARENA_Z);
            }
        }
        // Beside the gateways the pit wall is solid.
        let beside_gate = 10_000;
        for (x, z) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut arena = default_arena();
            arena.fighters[0].position = Position {
                x: if x == 0 { beside_gate } else { 0 },
                z: 0,
            };
            for _ in 0..ARENA_X / MOVE_PER_TICK + 10 {
                arena.step(&[ArenaCommand::Move {
                    actor: 0,
                    x: x * 1000,
                    z: z * 1000,
                }]);
            }
            assert_eq!(
                arena.fighters()[0].position,
                Position {
                    x: if x == 0 {
                        beside_gate
                    } else {
                        i32::from(x) * (ARENA_X - BODY_RADIUS)
                    },
                    z: i32::from(z) * (ARENA_Z - BODY_RADIUS),
                }
            );
        }
    }

    #[test]
    fn starting_rooms_open_onto_the_floor_only_through_their_width() {
        let run = |start: Position, x: i16, z: i16, ticks: i32| {
            let mut arena = default_arena();
            arena.fighters[0].position = start;
            for _ in 0..ticks {
                arena.step(&[ArenaCommand::Move { actor: 0, x, z }]);
            }
            arena.fighters()[0].position
        };
        let room = &fixture().rooms[0];
        let (half, depth) = (room.width / 2, room.depth);
        let room_back = ARENA_Z + depth - BODY_RADIUS;
        // Out of the friendly room and across the floor into the other room.
        let spawn = default_arena().fighters()[0].position;
        let crossed = run(spawn, 0, -1000, 2 * (ARENA_Z + depth) / MOVE_PER_TICK);
        assert_eq!(
            crossed,
            Position {
                x: 0,
                z: -room_back
            }
        );
        // From the floor straight into the friendly room's back wall.
        assert_eq!(
            run(Position { x: 0, z: 0 }, 0, 1000, 400),
            Position { x: 0, z: room_back }
        );
        // The room is a straight corridor: its sides continue the opening's
        // edges all the way back, with no wider pockets.
        for z in [ARENA_Z + 700, room_back] {
            assert_eq!(
                run(Position { x: 0, z }, 1000, 0, 100),
                Position {
                    x: half - BODY_RADIUS,
                    z
                }
            );
        }
        // Sight passes straight through the opening but not the pit wall.
        let inside = Position {
            x: 0,
            z: ARENA_Z + 5_000,
        };
        assert!(line_of_sight(
            inside,
            Position {
                x: 0,
                z: ARENA_Z - 3_000
            }
        ));
        assert!(!line_of_sight(
            Position {
                x: 2_000,
                z: ARENA_Z + 5_000
            },
            Position {
                x: 8_000,
                z: ARENA_Z - 3_000
            }
        ));
        // Both rooms see each other straight down their openings.
        assert!(line_of_sight(inside, Position { x: 0, z: -inside.z }));
    }

    /// Places every fighter on the open floor, `spacing` mm apart along X,
    /// all within range and sight of each other.
    fn close_quarters(team_size: usize, spacing: i32) -> Arena {
        let mut arena = Arena::practice(fixture(), team_size).unwrap();
        for (index, fighter) in arena.fighters.iter_mut().enumerate() {
            fighter.position = Position {
                x: index as i32 * spacing,
                z: 0,
            };
        }
        arena
    }

    #[test]
    fn restored_mid_cast_fighter_pays_exactly_once_on_completion() {
        use crate::spells::{RIFT_BOLT_CAST_TICKS, RIFT_BOLT_COST};

        // A reconnect or replay restores a fighter mid-cast with a part-spent
        // pool on a tick that never regenerates before completion.
        let mut original = close_quarters(1, 10_000);
        original.fighters[0].resource = 50;
        original.fighters[0].active_cast = Some(ActiveCastState {
            spell: SpellId::RiftBolt,
            target: Some(1),
            remaining_ticks: 2,
            total_ticks: RIFT_BOLT_CAST_TICKS,
        });
        original.fighters[0].gcd_remaining = 0;
        let mut restored = Arena::restore(
            original.layout().clone(),
            1,
            original.fighters().to_vec(),
            None,
        )
        .unwrap();
        assert_eq!(restored.step(&[]), vec![]);
        assert_eq!(restored.fighters()[0].resource, 50);
        let events = restored.step(&[]);
        assert!(events.contains(&ArenaEvent::CastCompleted {
            actor: 0,
            spell: SpellId::RiftBolt,
            target: Some(1),
        }));
        // Tick 2 completes (tick 3 regenerates +1 at its end).
        assert_eq!(restored.fighters()[0].resource, 50 - RIFT_BOLT_COST + 1);
        for _ in 0..10 {
            restored.step(&[]);
        }
        assert!(
            restored.fighters()[0].resource > 50 - RIFT_BOLT_COST,
            "never charged twice"
        );
    }

    #[test]
    fn simultaneous_lethal_hits_both_land_and_draw() {
        let mut arena = close_quarters(1, 3_000);
        arena.fighters[0].health = ATTACK_DAMAGE;
        arena.fighters[1].health = ATTACK_DAMAGE;
        let mut swapped = arena.clone();
        let commands = [
            ArenaCommand::Attack {
                actor: 0,
                target: 1,
            },
            ArenaCommand::Attack {
                actor: 1,
                target: 0,
            },
        ];
        let events = arena.step(&commands);
        let mut reversed = commands;
        reversed.reverse();
        assert_eq!(swapped.step(&reversed), events, "list order is irrelevant");
        assert_eq!(arena, swapped);
        assert_eq!(
            events,
            vec![
                ArenaEvent::Hit {
                    actor: 0,
                    target: 1,
                    damage: ATTACK_DAMAGE
                },
                ArenaEvent::Hit {
                    actor: 1,
                    target: 0,
                    damage: ATTACK_DAMAGE
                },
                ArenaEvent::Finished(RoundResult::Draw),
            ]
        );
        assert_eq!(arena.result(), Some(RoundResult::Draw));
        assert!(arena.fighters.iter().all(|fighter| fighter.health == 0));
        // A finished round refuses everything and only advances the clock.
        let tick = arena.tick();
        assert_eq!(
            arena.step(&commands),
            vec![
                rejected(0, Rejection::MatchOver),
                rejected(1, Rejection::MatchOver)
            ]
        );
        assert_eq!(arena.tick(), tick + 1);
    }

    #[test]
    fn focus_fire_hits_are_independent_and_the_last_standing_team_wins() {
        // 2v2: both enemies strike actor 0 (30 health) and actor 2 strikes
        // back at actor 1 on the same tick. Each hit is judged on the shared
        // pre-attack state, so both land, actor 0 falls, and actor 0's own
        // attack still resolves because it acted in the same tick.
        let mut arena = close_quarters(2, 1_000);
        arena.fighters[0].health = 30;
        let commands = [
            ArenaCommand::Attack {
                actor: 3,
                target: 0,
            },
            ArenaCommand::Attack {
                actor: 1,
                target: 0,
            },
            ArenaCommand::Attack {
                actor: 0,
                target: 1,
            },
        ];
        let events = arena.step(&commands);
        assert_eq!(
            events,
            vec![
                ArenaEvent::Hit {
                    actor: 0,
                    target: 1,
                    damage: ATTACK_DAMAGE
                },
                ArenaEvent::Hit {
                    actor: 1,
                    target: 0,
                    damage: ATTACK_DAMAGE
                },
                ArenaEvent::Hit {
                    actor: 3,
                    target: 0,
                    damage: ATTACK_DAMAGE
                },
            ]
        );
        assert_eq!(arena.fighters[0].health, 0);
        assert_eq!(arena.fighters[1].health, MAX_HEALTH - ATTACK_DAMAGE);
        assert_eq!(arena.result(), None, "actor 2 still stands for team 0");
        assert!([0, 1, 3].iter().all(|&actor| {
            arena.fighters[actor].cooldown_remaining(SpellId::Pulse) == PULSE_COOLDOWN_TICKS - 1
        }));
        // Last enemy of team 0 falls: team 1 wins.
        arena.fighters[2].health = 10;
        while arena.can_attack(1, 2).is_err() {
            arena.step(&[]);
        }
        assert_eq!(
            arena
                .step(&[ArenaCommand::Attack {
                    actor: 1,
                    target: 2
                }])
                .last(),
            Some(&ArenaEvent::Finished(RoundResult::Won { team: 1 }))
        );
    }

    #[test]
    fn command_list_order_never_changes_the_outcome() {
        // Every actor in a 3v3 moves, turns, jumps and attacks on one tick;
        // any permutation of the list gives the same events and state.
        let base = close_quarters(3, 1_000);
        let mut commands = Vec::new();
        for actor in 0..6 {
            commands.push(ArenaCommand::Move {
                actor,
                x: 1000 - actor as i16 * 300,
                z: 500,
            });
            commands.push(ArenaCommand::Face {
                actor,
                facing: actor as u16 * 9_000,
            });
            if actor % 3 == 0 {
                commands.push(ArenaCommand::Jump { actor });
            }
            commands.push(ArenaCommand::Attack {
                actor,
                target: (actor + 1) % 6,
            });
        }
        let mut expected = base.clone();
        let events = expected.step(&commands);
        for rotation in 1..commands.len() {
            for reverse in [false, true] {
                let mut order = commands.clone();
                order.rotate_left(rotation);
                if reverse {
                    order.reverse();
                }
                let mut arena = base.clone();
                assert_eq!(arena.step(&order), events);
                assert_eq!(arena, expected);
            }
        }
    }

    #[test]
    fn facing_is_authoritative_free_while_alive_and_frozen_when_defeated() {
        for size in 1..=3 {
            let arena = Arena::practice(fixture(), size).unwrap();
            for (index, fighter) in arena.fighters().iter().enumerate() {
                assert_eq!(fighter.facing, if index % 2 == 0 { 0 } else { HALF_TURN });
            }
        }
        let mut arena = default_arena();
        arena.step(&[ArenaCommand::Face {
            actor: 0,
            facing: u16::MAX,
        }]);
        assert_eq!(arena.fighters[0].facing, u16::MAX);
        // Turning in the air is allowed; it does not steer.
        arena.step(&[forward(), JUMP]);
        let momentum = arena.fighters[0].momentum;
        arena.step(&[ArenaCommand::Face {
            actor: 0,
            facing: 12_345,
        }]);
        assert_eq!(arena.fighters[0].facing, 12_345);
        assert_eq!(arena.fighters[0].momentum, momentum);
        assert_eq!(
            arena.step(&[
                ArenaCommand::Face {
                    actor: 0,
                    facing: 1
                },
                ArenaCommand::Face {
                    actor: 0,
                    facing: 2
                }
            ]),
            vec![rejected(0, Rejection::Duplicate)]
        );
        assert_eq!(arena.fighters[0].facing, 1, "the first facing wins");
        arena.fighters[0].health = 0;
        arena.fighters[1].health = 1;
        arena.step(&[ArenaCommand::Face {
            actor: 0,
            facing: 7,
        }]);
        assert_eq!(arena.fighters[0].facing, 1);
    }

    #[test]
    fn input_slots_bound_the_roster_and_budget() {
        let mut arena = default_arena();
        let before = arena.clone();
        let jump = ActorInput {
            jump: true,
            ..ActorInput::default()
        };
        // Missing slots are idle; extra idle slots are ignored; extra
        // non-idle slots are refused without touching anyone.
        assert_eq!(arena.advance(&[]), vec![]);
        assert_eq!(
            arena.advance(&[
                ActorInput::default(),
                ActorInput::default(),
                ActorInput::default(),
                jump
            ]),
            vec![rejected(3, Rejection::InvalidActor)]
        );
        assert_eq!(arena.fighters, before.fighters);
        assert_eq!(arena.tick(), 2);
        // A slot cannot name its actor, so an opponent's slot only moves the
        // opponent: actor identity is the slot index.
        assert_eq!(
            arena.advance(&[ActorInput::default(), jump]),
            vec![ArenaEvent::Jumped { actor: 1 }]
        );
        assert_eq!(arena.fighters[0], before.fighters[0]);
        assert!(arena.fighters[1].height > 0);
        // `step` and `advance` agree on the same intent.
        let mut a = default_arena();
        let mut b = default_arena();
        let input = ActorInput {
            movement: Some(Axes { x: 300, z: -1000 }),
            facing: Some(42),
            jump: true,
            action: ActorInput::pulse(1),
            rewind_ticks: 0,
        };
        assert_eq!(
            a.advance(&[input]),
            b.step(&[
                ArenaCommand::Attack {
                    actor: 0,
                    target: 1
                },
                JUMP,
                ArenaCommand::Face {
                    actor: 0,
                    facing: 42
                },
                ArenaCommand::Move {
                    actor: 0,
                    x: 300,
                    z: -1000
                },
            ])
        );
        assert_eq!(a, b);
    }

    #[test]
    fn state_hash_is_stable_and_covers_authoritative_state() {
        let mut arena = Arena {
            tick: 7,
            max_round_ticks: MAX_ROUND_TICKS,
            layout: fixture(),
            position_history: PositionHistory::default(),
            fighters: vec![
                Combatant {
                    position: Position {
                        x: -1_000,
                        z: 2_000,
                    },
                    height: 0,
                    vertical_velocity: 0,
                    momentum: Position { x: 0, z: 0 },
                    health: MAX_HEALTH,
                    resource: MAX_RESOURCE,
                    gcd_remaining: 0,
                    lockout_remaining: 0,
                    cooldowns: [0; SpellId::COUNT],
                    active_cast: None,
                    facing: 0,
                },
                Combatant {
                    position: Position {
                        x: 1_000,
                        z: -2_000,
                    },
                    height: 220,
                    vertical_velocity: 200,
                    momentum: Position { x: 240, z: 0 },
                    health: 75,
                    resource: 65,
                    gcd_remaining: 12,
                    lockout_remaining: 0,
                    cooldowns: [30, 0, 120, 0],
                    active_cast: Some(ActiveCastState {
                        spell: SpellId::RiftBolt,
                        target: Some(0),
                        remaining_ticks: 20,
                        total_ticks: 45,
                    }),
                    facing: HALF_TURN,
                },
            ],
            result: None,
        };
        // Golden value: the digest is a cross-build contract (server, replay,
        // browser), so its byte layout must only change deliberately.
        assert_eq!(arena.state_hash(), GOLDEN_STATE_HASH);
        assert_eq!(arena.clone().state_hash(), arena.state_hash());
        let mut seen = vec![arena.state_hash()];
        let mutations: [fn(&mut Arena); 12] = [
            |a| a.fighters[0].facing = 1,
            |a| a.fighters[1].health -= 1,
            |a| a.fighters[0].position.x += 1,
            |a| a.fighters[1].resource -= 1,
            |a| a.fighters[1].gcd_remaining += 1,
            |a| a.fighters[1].lockout_remaining += 1,
            |a| a.fighters[1].cooldowns[SpellId::Pulse.index()] += 1,
            |a| a.fighters[1].active_cast.as_mut().unwrap().remaining_ticks -= 1,
            |a| a.fighters[1].active_cast = None,
            |a| a.tick += 1,
            |a| a.result = Some(RoundResult::Draw),
            |a| a.result = Some(RoundResult::TimeLimitDraw),
        ];
        for mutate in mutations {
            mutate(&mut arena);
            let hash = arena.state_hash();
            assert!(!seen.contains(&hash));
            seen.push(hash);
        }
    }
    #[test]
    fn a_round_still_running_after_twenty_minutes_is_a_time_limit_draw() {
        assert_eq!(MAX_ROUND_TICKS, 36_000);
        let mut arena = close_quarters(1, 3_000);
        assert_eq!(arena.max_round_ticks(), MAX_ROUND_TICKS);
        assert_eq!(arena.remaining_ticks(), MAX_ROUND_TICKS);
        arena.tick = MAX_ROUND_TICKS - 2;
        assert_eq!(arena.remaining_ticks(), 2);
        assert_eq!(arena.step(&[]), vec![]);
        assert_eq!(arena.result(), None);
        assert_eq!(arena.remaining_ticks(), 1);
        // The last tick still resolves, then the round ends in a time-limit draw.
        assert_eq!(
            arena.step(&[forward()]),
            vec![
                ArenaEvent::Moved { actor: 0 },
                ArenaEvent::Finished(RoundResult::TimeLimitDraw)
            ]
        );
        assert_eq!(arena.result(), Some(RoundResult::TimeLimitDraw));
        assert!(arena.result().unwrap().is_draw());
        assert_eq!(arena.tick(), MAX_ROUND_TICKS);
        assert_eq!(arena.remaining_ticks(), 0);
        // Subsequent ticks reject all commands and remaining_ticks stays at 0.
        assert_eq!(
            arena.step(&[forward()]),
            vec![rejected(0, Rejection::MatchOver)]
        );
        assert_eq!(arena.remaining_ticks(), 0);
        // An elimination on that last tick still wins.
        let mut arena = close_quarters(1, 3_000);
        arena.tick = MAX_ROUND_TICKS - 1;
        arena.fighters[1].health = ATTACK_DAMAGE;
        assert_eq!(
            arena.step(&[ATTACK]).last(),
            Some(&ArenaEvent::Finished(RoundResult::Won { team: 0 }))
        );
        // Simultaneous mutual elimination on that last tick is a Draw, not a TimeLimitDraw.
        let mut mutual = close_quarters(1, 3_000);
        mutual.tick = MAX_ROUND_TICKS - 1;
        mutual.fighters[0].health = ATTACK_DAMAGE;
        mutual.fighters[1].health = ATTACK_DAMAGE;
        let events = mutual.step(&[
            ATTACK,
            ArenaCommand::Attack {
                actor: 1,
                target: 0,
            },
        ]);
        assert_eq!(
            events.last(),
            Some(&ArenaEvent::Finished(RoundResult::Draw))
        );
        assert_eq!(mutual.result(), Some(RoundResult::Draw));
        assert!(mutual.result().unwrap().is_draw());
        // Shorter round limits are bounded to 1..=MAX_ROUND_TICKS and replay deterministically.
        assert!(close_quarters(1, 3_000).with_round_limit(0).is_none());
        assert!(
            close_quarters(1, 3_000)
                .with_round_limit(MAX_ROUND_TICKS + 1)
                .is_none()
        );
        let mut short_a = close_quarters(1, 3_000).with_round_limit(3).unwrap();
        let mut short_b = short_a.clone();
        for expected_remaining in [2, 1, 0] {
            assert_eq!(short_a.step(&[]), short_b.step(&[]));
            assert_eq!(short_a.remaining_ticks(), expected_remaining);
        }
        assert_eq!(short_a.result(), Some(RoundResult::TimeLimitDraw));
        assert_eq!(short_a, short_b);
        // Tick and cooldown arithmetic near u64::MAX cannot overflow or panic.
        let mut saturated =
            Arena::restore(fixture(), u64::MAX, short_a.fighters().to_vec(), None).unwrap();
        assert_eq!(saturated.remaining_ticks(), 0);
        assert_eq!(
            saturated.step(&[ATTACK]),
            vec![
                ArenaEvent::Hit {
                    actor: 0,
                    target: 1,
                    damage: ATTACK_DAMAGE
                },
                ArenaEvent::Finished(RoundResult::TimeLimitDraw)
            ]
        );
        assert_eq!(
            saturated.fighters()[0].cooldown_remaining(SpellId::Pulse),
            PULSE_COOLDOWN_TICKS
        );
        assert_eq!(saturated.tick(), u64::MAX);
    }

    #[test]
    fn restored_arenas_continue_exactly_like_the_original() {
        let mut original = close_quarters(2, 1_000);
        original.step(&[forward(), JUMP, ATTACK]);
        let mut restored = Arena::restore(
            original.layout().clone(),
            original.tick(),
            original.fighters().to_vec(),
            original.result(),
        )
        .unwrap();
        assert_eq!(restored, original);
        assert_eq!(restored.state_hash(), original.state_hash());
        let commands = [forward(), ArenaCommand::Jump { actor: 3 }];
        assert_eq!(restored.step(&commands), original.step(&commands));
        assert_eq!(restored, original);
        let one = original.fighters()[0];
        for count in [0, 1, 3, 8] {
            assert!(Arena::restore(fixture(), 0, vec![one; count], None).is_none());
        }
        // In-place restore updates state without cloning the layout and keeps
        // the previous state intact when given an invalid fighter count.
        let mut reusable = default_arena();
        assert!(
            reusable.restore_in_place(original.tick(), original.fighters(), original.result(),)
        );
        assert_eq!(reusable, original);
        assert_eq!(
            Arena::hash_state(original.tick(), original.fighters(), original.result()),
            original.state_hash()
        );
        for count in [0, 1, 3, 8] {
            assert!(!reusable.restore_in_place(99, &vec![one; count], None));
            assert_eq!(reusable, original);
        }
    }

    #[test]
    fn timed_casts_enforce_gcd_per_spell_cooldowns_resources_and_completion_rechecks() {
        use crate::spells::{
            MEND_CAST_TICKS, MEND_COOLDOWN_TICKS, MEND_COST, MEND_HEAL, PULSE_COST,
            RIFT_BOLT_CAST_TICKS, RIFT_BOLT_COOLDOWN_TICKS, RIFT_BOLT_COST, RIFT_BOLT_DAMAGE,
        };

        // Start Rift Bolt at 10 m (within 12 m Rift Bolt range, outside 6 m Pulse range).
        let mut arena = close_quarters(1, 10_000);
        let cast_bolt = ArenaCommand::Cast {
            actor: 0,
            spell: SpellId::RiftBolt,
            target: Some(1),
        };
        assert_eq!(
            arena.step(&[cast_bolt]),
            vec![ArenaEvent::CastStarted {
                actor: 0,
                spell: SpellId::RiftBolt,
                target: Some(1),
                duration_ticks: RIFT_BOLT_CAST_TICKS,
            }]
        );
        // Resource is NOT spent at cast start; GCD started (29 - 1 = 28 after tick 0);
        // personal CD not started yet.
        assert_eq!(arena.fighters()[0].resource, MAX_RESOURCE);
        assert_eq!(arena.fighters()[0].gcd_remaining, 28);
        assert_eq!(arena.fighters()[0].cooldown_remaining(SpellId::RiftBolt), 0);
        assert_eq!(
            arena.fighters()[0].active_cast,
            Some(ActiveCastState {
                spell: SpellId::RiftBolt,
                target: Some(1),
                remaining_ticks: RIFT_BOLT_CAST_TICKS,
                total_ticks: RIFT_BOLT_CAST_TICKS,
            })
        );

        // Cannot start another spell (even off-GCD Quell) while already casting.
        assert_eq!(
            arena.step(&[ArenaCommand::Cast {
                actor: 0,
                spell: SpellId::Quell,
                target: Some(1),
            }]),
            vec![rejected(0, Rejection::AlreadyCasting)]
        );
        assert_eq!(
            arena.fighters()[0].active_cast.unwrap().remaining_ticks,
            RIFT_BOLT_CAST_TICKS - 1
        );

        // Stand still until the completion tick (tick 45): Rift Bolt completes and hits for 40.
        for _ in 2..RIFT_BOLT_CAST_TICKS {
            assert_eq!(arena.step(&[]), vec![]);
        }
        assert_eq!(arena.fighters()[1].health, MAX_HEALTH);
        assert_eq!(
            arena.step(&[]),
            vec![
                ArenaEvent::CastCompleted {
                    actor: 0,
                    spell: SpellId::RiftBolt,
                    target: Some(1),
                },
                ArenaEvent::Hit {
                    actor: 0,
                    target: 1,
                    damage: RIFT_BOLT_DAMAGE,
                },
            ]
        );
        assert_eq!(arena.fighters()[1].health, MAX_HEALTH - RIFT_BOLT_DAMAGE);
        assert_eq!(arena.fighters()[0].active_cast, None);
        assert_eq!(
            arena.fighters()[0].cooldown_remaining(SpellId::RiftBolt),
            RIFT_BOLT_COOLDOWN_TICKS - 1
        );
        // The pool stayed full (regen capped) for the whole cast; the cost is
        // charged only now, on the completion tick.
        assert_eq!(arena.fighters()[0].resource, MAX_RESOURCE - RIFT_BOLT_COST);

        // Re-check range on completion tick: enemy walks past 12 m just before completion.
        let mut range_loss = close_quarters(1, 11_900);
        range_loss.step(&[cast_bolt]);
        for _ in 1..RIFT_BOLT_CAST_TICKS {
            assert_eq!(range_loss.step(&[]), vec![]);
        }
        let events = range_loss.step(&[ArenaCommand::Move {
            actor: 1,
            x: 1000,
            z: 0,
        }]);
        assert!(events.contains(&ArenaEvent::CastInterrupted {
            actor: 0,
            spell: SpellId::RiftBolt,
            reason: InterruptReason::OutOfRange,
            by: None,
        }));
        assert_eq!(range_loss.fighters()[1].health, MAX_HEALTH);
        assert_eq!(
            range_loss.fighters()[0].resource,
            MAX_RESOURCE,
            "a cast that fails its completion re-check spends nothing"
        );
        assert_eq!(
            range_loss.fighters()[0].cooldown_remaining(SpellId::RiftBolt),
            0,
            "interrupted cast does not start personal cooldown"
        );

        // Re-check line of sight on completion tick: enemy ducks behind a pillar while staying in 12 m range.
        let pillar = pillars()[0];
        let mut los_loss = default_arena();
        los_loss.fighters[0].position = Position {
            x: pillar.center.x - pillar.half_x - 500,
            z: pillar.center.z + pillar.half_z + 200,
        };
        los_loss.fighters[1].position = Position {
            x: pillar.center.x + pillar.half_x + 500,
            z: pillar.center.z + pillar.half_z + 200,
        };
        assert_eq!(
            los_loss.step(&[cast_bolt]),
            vec![ArenaEvent::CastStarted {
                actor: 0,
                spell: SpellId::RiftBolt,
                target: Some(1),
                duration_ticks: RIFT_BOLT_CAST_TICKS,
            }]
        );
        for _ in 1..RIFT_BOLT_CAST_TICKS {
            // Enemy steps behind the pillar mid-cast; cast continues until completion re-check.
            if los_loss.tick() == 10 {
                los_loss.fighters[1].position.z = pillar.center.z;
            }
            assert_eq!(los_loss.step(&[]), vec![]);
        }
        assert_eq!(
            los_loss.step(&[]),
            vec![ArenaEvent::CastInterrupted {
                actor: 0,
                spell: SpellId::RiftBolt,
                reason: InterruptReason::LineOfSight,
                by: None,
            }]
        );
        assert_eq!(los_loss.fighters()[1].health, MAX_HEALTH);
        assert_eq!(los_loss.fighters()[0].resource, MAX_RESOURCE);

        // Friendly heal (Mend) heals self or teammate up to MAX_HEALTH, never enemy.
        let mut heal_arena = close_quarters(2, 2_000);
        heal_arena.fighters[2].health = 50;
        assert_eq!(
            heal_arena.step(&[ArenaCommand::Cast {
                actor: 0,
                spell: SpellId::Mend,
                target: Some(1),
            }]),
            vec![rejected(0, Rejection::InvalidTarget)]
        );
        assert_eq!(
            heal_arena.step(&[ArenaCommand::Cast {
                actor: 0,
                spell: SpellId::Mend,
                target: Some(2),
            }]),
            vec![ArenaEvent::CastStarted {
                actor: 0,
                spell: SpellId::Mend,
                target: Some(2),
                duration_ticks: MEND_CAST_TICKS,
            }]
        );
        assert_eq!(heal_arena.fighters()[0].resource, MAX_RESOURCE);
        for _ in 1..MEND_CAST_TICKS {
            heal_arena.step(&[]);
        }
        assert_eq!(
            heal_arena.step(&[]),
            vec![
                ArenaEvent::CastCompleted {
                    actor: 0,
                    spell: SpellId::Mend,
                    target: Some(2),
                },
                ArenaEvent::Healed {
                    actor: 0,
                    target: 2,
                    amount: MEND_HEAL,
                },
            ]
        );
        assert_eq!(heal_arena.fighters()[2].health, 50 + MEND_HEAL);
        assert_eq!(heal_arena.fighters()[0].resource, MAX_RESOURCE - MEND_COST);
        assert_eq!(
            heal_arena.fighters()[0].cooldown_remaining(SpellId::Mend),
            MEND_COOLDOWN_TICKS - 1
        );

        // Insufficient resource rejects cleanly without starting cooldown or GCD.
        let mut oom = close_quarters(1, 3_000);
        oom.fighters[0].resource = PULSE_COST - 1;
        assert_eq!(
            oom.step(&[ATTACK]),
            vec![rejected(0, Rejection::NotEnoughResource)]
        );
        assert_eq!(oom.fighters()[0].gcd_remaining, 0);
        assert_eq!(oom.fighters()[0].cooldown_remaining(SpellId::Pulse), 0);
    }

    #[test]
    fn interrupts_off_gcd_lockout_movement_cancel_and_fake_casting_work_authoritatively() {
        use crate::spells::{INTERRUPT_LOCKOUT_TICKS, QUELL_COOLDOWN_TICKS, QUELL_COST};

        let mut arena = close_quarters(1, 5_000);
        // Actor 1 fires Pulse (starts GCD = 29), Actor 0 starts Rift Bolt on tick 0.
        let events = arena.step(&[
            ArenaCommand::Cast {
                actor: 0,
                spell: SpellId::RiftBolt,
                target: Some(1),
            },
            ArenaCommand::Attack {
                actor: 1,
                target: 0,
            },
        ]);
        assert!(events.contains(&ArenaEvent::CastStarted {
            actor: 0,
            spell: SpellId::RiftBolt,
            target: Some(1),
            duration_ticks: crate::spells::RIFT_BOLT_CAST_TICKS,
        }));
        assert!(events.contains(&ArenaEvent::Hit {
            actor: 1,
            target: 0,
            damage: ATTACK_DAMAGE,
        }));
        // Actor 1 is on GCD (28 ticks left) and cannot cast Rift Bolt,
        // but CAN cast off-GCD Quell to interrupt Actor 0!
        assert_eq!(
            arena.can_cast(1, SpellId::RiftBolt, Some(0)),
            Err(Rejection::GlobalCooldown)
        );
        assert_eq!(arena.can_cast(1, SpellId::Quell, Some(0)), Ok(()));
        let events = arena.step(&[ArenaCommand::Cast {
            actor: 1,
            spell: SpellId::Quell,
            target: Some(0),
        }]);
        assert_eq!(
            events,
            vec![
                ArenaEvent::CastCompleted {
                    actor: 1,
                    spell: SpellId::Quell,
                    target: Some(0),
                },
                ArenaEvent::CastInterrupted {
                    actor: 0,
                    spell: SpellId::RiftBolt,
                    reason: InterruptReason::Interrupted,
                    by: Some(1),
                },
            ]
        );
        assert_eq!(arena.fighters()[0].active_cast, None);
        assert_eq!(
            arena.fighters()[0].lockout_remaining,
            INTERRUPT_LOCKOUT_TICKS - 1
        );
        // The interrupted cast spent no resource and its personal cooldown did NOT start.
        assert_eq!(arena.fighters()[0].resource, MAX_RESOURCE);
        assert_eq!(arena.fighters()[0].cooldown_remaining(SpellId::RiftBolt), 0);
        // Quell spent its cost and started its 6 s cooldown without resetting Actor 1's GCD.
        assert_eq!(
            arena.fighters()[1].cooldown_remaining(SpellId::Quell),
            QUELL_COOLDOWN_TICKS - 1
        );
        assert_eq!(
            arena.fighters()[1].resource,
            MAX_RESOURCE - crate::spells::PULSE_COST - QUELL_COST
        );
        // During lockout, Actor 0 cannot cast any spell.
        assert_eq!(
            arena.step(&[ATTACK]),
            vec![rejected(0, Rejection::Silenced)]
        );
        for _ in 0..INTERRUPT_LOCKOUT_TICKS {
            arena.step(&[]);
        }
        assert_eq!(arena.fighters()[0].lockout_remaining, 0);
        assert_eq!(arena.can_attack(0, 1), Ok(()));

        // Fake-casting (juke): cancelling via movement or CancelCast on the same tick
        // as an enemy Quell makes Quell whiff with no lockout!
        for juke_by_move in [false, true] {
            let mut juke = close_quarters(1, 5_000);
            juke.step(&[ArenaCommand::Cast {
                actor: 0,
                spell: SpellId::RiftBolt,
                target: Some(1),
            }]);
            let mut commands = vec![ArenaCommand::Cast {
                actor: 1,
                spell: SpellId::Quell,
                target: Some(0),
            }];
            if juke_by_move {
                commands.push(forward());
            } else {
                commands.push(ArenaCommand::CancelCast { actor: 0 });
            }
            let mut swapped = juke.clone();
            let events = juke.step(&commands);
            commands.reverse();
            assert_eq!(swapped.step(&commands), events, "order independent");
            assert_eq!(juke, swapped);
            assert_eq!(juke.fighters()[0].lockout_remaining, 0);
            assert_eq!(
                juke.fighters()[0].resource,
                MAX_RESOURCE,
                "a cancelled or moved-out-of cast spends nothing"
            );
            assert_eq!(
                juke.fighters()[1].cooldown_remaining(SpellId::Quell),
                QUELL_COOLDOWN_TICKS - 1,
                "baited interrupt still enters cooldown"
            );
            assert!(events.contains(&ArenaEvent::CastInterrupted {
                actor: 0,
                spell: SpellId::RiftBolt,
                reason: if juke_by_move {
                    InterruptReason::Moved
                } else {
                    InterruptReason::Cancelled
                },
                by: None,
            }));
        }

        // Cannot start a stationary cast while moving; CancelCast while idle rejects.
        let mut move_start = close_quarters(1, 5_000);
        assert_eq!(
            move_start.step(&[
                forward(),
                ArenaCommand::Cast {
                    actor: 0,
                    spell: SpellId::RiftBolt,
                    target: Some(1),
                },
            ]),
            vec![
                ArenaEvent::Moved { actor: 0 },
                rejected(0, Rejection::Moving),
            ]
        );
        assert_eq!(move_start.fighters()[0].resource, MAX_RESOURCE);
        assert_eq!(
            move_start.step(&[ArenaCommand::CancelCast { actor: 0 }]),
            vec![rejected(0, Rejection::NotCasting)]
        );

        // A lethal hit interrupts an in-progress cast as CasterDead and never leaves a dead caster casting.
        let mut lethal = close_quarters(1, 5_000);
        lethal.fighters[0].health = ATTACK_DAMAGE;
        lethal.step(&[ArenaCommand::Cast {
            actor: 0,
            spell: SpellId::RiftBolt,
            target: Some(1),
        }]);
        let events = lethal.step(&[ArenaCommand::Attack {
            actor: 1,
            target: 0,
        }]);
        assert!(events.contains(&ArenaEvent::CastInterrupted {
            actor: 0,
            spell: SpellId::RiftBolt,
            reason: InterruptReason::CasterDead,
            by: None,
        }));
        assert_eq!(lethal.fighters()[0].active_cast, None);
    }

    #[test]
    fn restore_rejects_corrupted_combat_and_cast_invariants() {
        let base = close_quarters(1, 3_000);
        let valid = base.fighters().to_vec();
        let bad_cases: Vec<Vec<Combatant>> = vec![
            {
                let mut f = valid.clone();
                f[0].health = MAX_HEALTH + 1;
                f
            },
            {
                let mut f = valid.clone();
                f[0].resource = MAX_RESOURCE + 1;
                f
            },
            {
                let mut f = valid.clone();
                f[0].active_cast = Some(ActiveCastState {
                    spell: SpellId::Pulse, // instant spell cannot be active cast
                    target: Some(1),
                    remaining_ticks: 1,
                    total_ticks: 1,
                });
                f
            },
            {
                let mut f = valid.clone();
                f[0].active_cast = Some(ActiveCastState {
                    spell: SpellId::RiftBolt,
                    target: Some(1),
                    remaining_ticks: 0,
                    total_ticks: 45,
                });
                f
            },
            {
                let mut f = valid.clone();
                f[0].active_cast = Some(ActiveCastState {
                    spell: SpellId::RiftBolt,
                    target: Some(1),
                    remaining_ticks: 46,
                    total_ticks: 45,
                });
                f
            },
            {
                let mut f = valid.clone();
                f[0].active_cast = Some(ActiveCastState {
                    spell: SpellId::RiftBolt,
                    target: Some(2), // out of bounds target
                    remaining_ticks: 10,
                    total_ticks: 45,
                });
                f
            },
            {
                let mut f = valid.clone();
                f[0].health = 0;
                f[0].active_cast = Some(ActiveCastState {
                    spell: SpellId::RiftBolt,
                    target: Some(1),
                    remaining_ticks: 10,
                    total_ticks: 45,
                });
                f
            },
        ];
        for fighters in bad_cases {
            assert!(Arena::restore(fixture(), 1, fighters, None).is_none());
        }
    }

    #[test]
    fn simultaneous_combat_boundaries_stay_order_independent_and_respect_movement() {
        use crate::spells::{MEND_CAST_TICKS, QUELL_COOLDOWN_TICKS, RIFT_BOLT_CAST_TICKS};

        // 1. Simultaneous lethal damage + Mend completion on the same tick never revives a dead target.
        let mut lethal_heal = close_quarters(2, 2_000);
        lethal_heal.fighters[0].health = ATTACK_DAMAGE;
        lethal_heal.step(&[ArenaCommand::Cast {
            actor: 2,
            spell: SpellId::Mend,
            target: Some(0),
        }]);
        for _ in 1..MEND_CAST_TICKS {
            lethal_heal.step(&[]);
        }
        let mut swapped = lethal_heal.clone();
        let cmds = [
            ArenaCommand::Attack {
                actor: 1,
                target: 0,
            },
            ArenaCommand::Face {
                actor: 2,
                facing: 123,
            },
        ];
        let events = lethal_heal.step(&cmds);
        let mut rev = cmds;
        rev.reverse();
        assert_eq!(swapped.step(&rev), events);
        assert_eq!(lethal_heal, swapped);
        assert_eq!(lethal_heal.fighters()[0].health, 0);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ArenaEvent::Healed { .. }))
        );

        // 2. Target dying before completion tick interrupts the cast as TargetDead without personal CD.
        let mut target_dead = close_quarters(2, 2_000);
        target_dead.fighters[1].health = ATTACK_DAMAGE;
        target_dead.step(&[ArenaCommand::Cast {
            actor: 0,
            spell: SpellId::RiftBolt,
            target: Some(1),
        }]);
        // Teammate kills actor 1 mid-cast; round continues because actor 3 still stands.
        target_dead.step(&[ArenaCommand::Attack {
            actor: 2,
            target: 1,
        }]);
        assert_eq!(target_dead.fighters()[1].health, 0);
        for _ in 2..RIFT_BOLT_CAST_TICKS {
            assert_eq!(target_dead.step(&[]), vec![]);
        }
        assert_eq!(
            target_dead.step(&[]),
            vec![ArenaEvent::CastInterrupted {
                actor: 0,
                spell: SpellId::RiftBolt,
                reason: InterruptReason::TargetDead,
                by: None,
            }]
        );
        assert_eq!(
            target_dead.fighters()[0].cooldown_remaining(SpellId::RiftBolt),
            0
        );

        // 3. Same-tick CastStarted (RiftBolt) + enemy Quell whiffs regardless of command order.
        let mut same_tick = close_quarters(1, 5_000);
        let mut same_tick_rev = same_tick.clone();
        let pair = [
            ArenaCommand::Cast {
                actor: 0,
                spell: SpellId::RiftBolt,
                target: Some(1),
            },
            ArenaCommand::Cast {
                actor: 1,
                spell: SpellId::Quell,
                target: Some(0),
            },
        ];
        let events = same_tick.step(&pair);
        let mut pair_rev = pair;
        pair_rev.reverse();
        assert_eq!(same_tick_rev.step(&pair_rev), events);
        assert_eq!(same_tick, same_tick_rev);
        assert_eq!(same_tick.fighters()[0].lockout_remaining, 0);
        assert!(same_tick.fighters()[0].active_cast.is_some());
        assert_eq!(
            same_tick.fighters()[1].cooldown_remaining(SpellId::Quell),
            QUELL_COOLDOWN_TICKS - 1
        );

        // 4. Zero-vector Move { x: 0, z: 0 } keeps stationary casts running; jumping interrupts/blocks them.
        let mut idle_move = close_quarters(1, 5_000);
        idle_move.step(&[ArenaCommand::Cast {
            actor: 0,
            spell: SpellId::RiftBolt,
            target: Some(1),
        }]);
        let events = idle_move.step(&[ArenaCommand::Move {
            actor: 0,
            x: 0,
            z: 0,
        }]);
        assert_eq!(events, vec![ArenaEvent::Moved { actor: 0 }]);
        assert!(idle_move.fighters()[0].active_cast.is_some());

        // Jumping mid-cast interrupts the stationary cast as Moved.
        let events = idle_move.step(&[JUMP]);
        assert_eq!(
            events,
            vec![
                ArenaEvent::Jumped { actor: 0 },
                ArenaEvent::CastInterrupted {
                    actor: 0,
                    spell: SpellId::RiftBolt,
                    reason: InterruptReason::Moved,
                    by: None,
                },
            ]
        );
        // While airborne (`height > 0`), starting a stationary cast is rejected as Moving.
        assert!(idle_move.fighters()[0].height > 0);
        idle_move.fighters[0].gcd_remaining = 0;
        assert_eq!(
            idle_move.step(&[ArenaCommand::Cast {
                actor: 0,
                spell: SpellId::RiftBolt,
                target: Some(1),
            }]),
            vec![rejected(0, Rejection::Moving)]
        );
    }
    #[test]
    fn bounded_lag_comp_rewinds_fleeing_target_without_range_creep_or_overflow() {
        // Place enemy at 5,800 mm (in Pulse 6,000 mm range) and record that tick.
        let mut base = close_quarters(1, 5_800);
        base.position_history.clear();
        base.position_history.push(&base.fighters);

        // Enemy flees +X at 240 mm/tick for 3 ticks: 5,800 -> 6,040 -> 6,280 -> 6,520 mm.
        let flee = ActorInput {
            movement: Some(Axes { x: 1000, z: 0 }),
            ..ActorInput::default()
        };
        for _ in 0..3 {
            base.advance(&[ActorInput::default(), flee]);
        }
        assert_eq!(base.fighters[1].position.x, 6_520);

        // Present-tick cast (rewind_ticks = 0) rejects OutOfRange.
        let mut strict = base.clone();
        assert_eq!(
            strict.advance(&[
                ActorInput {
                    action: ActorInput::pulse(1),
                    rewind_ticks: 0,
                    ..ActorInput::default()
                },
                ActorInput::default(),
            ]),
            vec![rejected(0, Rejection::OutOfRange)]
        );

        // Rewinding 4 ticks reaches the tick before fleeing (5,800 mm, in range): succeeds!
        // Note: on the cast tick itself `pre_combat` is at 6,520 (post_tick = 4), so
        // 4 ticks ago is tick 0 (5,800 mm) and 3 ticks ago is tick 1 (6,040 mm).
        let mut rewound = base.clone();
        let events = rewound.advance(&[
            ActorInput {
                action: ActorInput::pulse(1),
                rewind_ticks: 4,
                ..ActorInput::default()
            },
            ActorInput::default(),
        ]);
        assert_eq!(
            events,
            vec![ArenaEvent::Hit {
                actor: 0,
                target: 1,
                damage: ATTACK_DAMAGE,
            }]
        );

        // Interpolation bracket `[clamped, clamped - 1]`: if target was in range at
        // `clamped - 1` (e.g., walking into range at 5,800 mm 4 ticks ago while at
        // 6,040 mm 5 ticks ago), `rewind_ticks = 5` still hits cleanly.
        let mut bracket = close_quarters(1, 6_040);
        bracket.position_history.clear();
        bracket.position_history.push(&bracket.fighters); // 5 ticks ago: 6,040 (out)
        let approach = ActorInput {
            movement: Some(Axes { x: -1000, z: 0 }),
            ..ActorInput::default()
        };
        bracket.advance(&[ActorInput::default(), approach]); // 4 ticks ago: 5,800 (in)
        for _ in 0..3 {
            bracket.advance(&[ActorInput::default(), flee]); // 6,040 -> 6,280 -> 6,520
        }
        let events = bracket.advance(&[
            ActorInput {
                action: ActorInput::pulse(1),
                rewind_ticks: 5,
                ..ActorInput::default()
            },
            ActorInput::default(),
        ]);
        assert!(events.iter().any(|e| matches!(e, ArenaEvent::Hit { .. })));

        // Hard cap: fleeing for MAX_LAG_COMP_TICKS + 1 ticks leaves EVERY slot in
        // history out of range; even `rewind_ticks = u8::MAX` must reject OutOfRange.
        let mut capped = close_quarters(1, 5_800);
        capped.position_history.clear();
        capped.position_history.push(&capped.fighters);
        for _ in 0..=MAX_LAG_COMP_TICKS {
            capped.advance(&[ActorInput::default(), flee]);
        }
        assert_eq!(
            capped.advance(&[
                ActorInput {
                    action: ActorInput::pulse(1),
                    rewind_ticks: u8::MAX,
                    ..ActorInput::default()
                },
                ActorInput::default(),
            ]),
            vec![rejected(0, Rejection::OutOfRange)]
        );

        // Zero range creep against stationary targets: standing 1 mm out of range
        // (6,001 mm) is ALWAYS rejected even with full `MAX_LAG_COMP_TICKS` rewind.
        let mut stationary = close_quarters(1, ATTACK_RANGE + 1);
        stationary.position_history.clear();
        for _ in 0..MAX_LAG_COMP_TICKS {
            stationary.advance(&[]);
        }
        assert_eq!(
            stationary.advance(&[ActorInput {
                action: ActorInput::pulse(1),
                rewind_ticks: MAX_LAG_COMP_TICKS,
                ..ActorInput::default()
            }]),
            vec![rejected(0, Rejection::OutOfRange)]
        );
    }

    #[test]
    fn bounded_lag_comp_respects_pillar_los_no_mix_and_match_and_strict_completion() {
        use crate::spells::{RIFT_BOLT_CAST_TICKS, RIFT_BOLT_RANGE};
        let layout = fixture();
        let pillar = layout.pillars[0];
        // Caster sits directly south of pillar[0]; target starts east of the
        // pillar in clear view, then steps directly north behind the pillar.
        let caster_pos = Position {
            x: pillar.center.x,
            z: pillar.center.z + pillar.half_z + 1_000,
        };
        let hidden_pos = Position {
            x: pillar.center.x,
            z: pillar.center.z - pillar.half_z - 1_000,
        };
        let visible_pos = Position {
            x: pillar.center.x + pillar.half_x + 1_500,
            z: caster_pos.z,
        };
        assert!(layout.walkable(caster_pos));
        assert!(layout.walkable(hidden_pos));
        assert!(layout.walkable(visible_pos));
        assert!(layout.line_of_sight(caster_pos, visible_pos));
        assert!(!layout.line_of_sight(caster_pos, hidden_pos));
        assert!(caster_pos.distance_squared(hidden_pos) <= i64::from(RIFT_BOLT_RANGE).pow(2));
        assert!(caster_pos.distance_squared(visible_pos) <= i64::from(RIFT_BOLT_RANGE).pow(2));

        let mut arena = Arena::practice(layout.clone(), 1).unwrap();
        arena.fighters[0].position = caster_pos;
        arena.fighters[1].position = visible_pos;
        arena.position_history.clear();
        arena.position_history.push(&arena.fighters); // 3 ticks ago: visible & in range
        arena.fighters[1].position = hidden_pos;
        arena.advance(&[]); // 2 ticks ago: hidden
        arena.advance(&[]); // 1 tick ago: hidden

        // Present tick rejects LineOfSight; rewinding 3 ticks starts RiftBolt cleanly.
        let mut strict = arena.clone();
        assert_eq!(
            strict.advance(&[ActorInput {
                action: Some(CastAction::Cast {
                    spell: SpellId::RiftBolt,
                    target: Some(1),
                }),
                rewind_ticks: 0,
                ..ActorInput::default()
            }]),
            vec![rejected(0, Rejection::LineOfSight)]
        );

        let mut rewound = arena.clone();
        let events = rewound.advance(&[ActorInput {
            action: Some(CastAction::Cast {
                spell: SpellId::RiftBolt,
                target: Some(1),
            }),
            rewind_ticks: 3,
            ..ActorInput::default()
        }]);
        assert!(events.iter().any(|e| matches!(
            e,
            ArenaEvent::CastStarted {
                spell: SpellId::RiftBolt,
                ..
            }
        )));

        // Timed cast completions (Phase 4a) do NOT rewind: target staying hidden
        // interrupts RiftBolt on its completion tick as LineOfSight.
        for _ in 1..RIFT_BOLT_CAST_TICKS {
            assert_eq!(rewound.advance(&[]), vec![]);
        }
        assert_eq!(
            rewound.advance(&[]),
            vec![ArenaEvent::CastInterrupted {
                actor: 0,
                spell: SpellId::RiftBolt,
                reason: InterruptReason::LineOfSight,
                by: None,
            }]
        );

        // No mix-and-match across ticks: target was visible but OUT OF RANGE 3 ticks ago,
        // and is IN RANGE but BEHIND PILLAR on the present tick -> must still reject!
        let far_visible = Position {
            x: caster_pos.x + 15_000,
            z: caster_pos.z,
        };
        assert!(layout.line_of_sight(caster_pos, far_visible));
        assert!(caster_pos.distance_squared(far_visible) > i64::from(RIFT_BOLT_RANGE).pow(2));
        let mut mix = Arena::practice(layout.clone(), 1).unwrap();
        mix.fighters[0].position = caster_pos;
        mix.fighters[1].position = far_visible;
        mix.position_history.clear();
        mix.position_history.push(&mix.fighters);
        mix.fighters[1].position = hidden_pos;
        mix.advance(&[]);
        mix.advance(&[]);
        assert_eq!(
            mix.advance(&[ActorInput {
                action: Some(CastAction::Cast {
                    spell: SpellId::RiftBolt,
                    target: Some(1),
                }),
                rewind_ticks: 3,
                ..ActorInput::default()
            }]),
            vec![rejected(0, Rejection::LineOfSight)]
        );

        // Target liveness is strictly present-tick: a dead target cannot be hit via rewind
        // (2v2 keeps the match running via the living teammate).
        let mut dead = close_quarters(2, 3_000);
        dead.position_history.clear();
        dead.position_history.push(&dead.fighters);
        dead.fighters[1].health = 0;
        assert_eq!(
            dead.advance(&[ActorInput {
                action: ActorInput::pulse(1),
                rewind_ticks: 1,
                ..ActorInput::default()
            }]),
            vec![rejected(0, Rejection::InvalidTarget)]
        );

        // Relayout clears position history so pre-relayout positions cannot be reused.
        let mut relayout_arena = close_quarters(1, 5_800);
        relayout_arena.position_history.clear();
        relayout_arena
            .position_history
            .push(&relayout_arena.fighters);
        relayout_arena.fighters[1].position.x = 6_500;
        assert!(relayout_arena.relayout(layout));
        assert_eq!(
            relayout_arena.advance(&[ActorInput {
                action: ActorInput::pulse(1),
                rewind_ticks: 1,
                ..ActorInput::default()
            }]),
            vec![rejected(0, Rejection::OutOfRange)]
        );
    }

    #[test]
    fn lag_comp_replay_is_deterministic() {
        let mut a = close_quarters(1, 5_800);
        let mut b = a.clone();
        let flee = ActorInput {
            movement: Some(Axes { x: 1000, z: 0 }),
            ..ActorInput::default()
        };
        let cast = ActorInput {
            action: ActorInput::pulse(1),
            rewind_ticks: 3,
            ..ActorInput::default()
        };
        for tick in 0..8 {
            let slots = if tick == 3 {
                [cast, flee]
            } else {
                [ActorInput::default(), flee]
            };
            assert_eq!(a.advance(&slots), b.advance(&slots));
            assert_eq!(a.state_hash(), b.state_hash());
            assert_eq!(a, b);
            assert_eq!(a.position_history, b.position_history);
        }
    }

    const GOLDEN_STATE_HASH: u64 = 2_147_620_522_866_943_910;
}
