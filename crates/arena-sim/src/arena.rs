//! Planar arena rules; positions are integer millimetres. No rendering,
//! networking, floating point, or OS time.
//!
//! Each tick takes one [`ActorInput`] slot per actor. The slot index is the
//! actor, so an input cannot act for someone else, and one slot carries at
//! most one movement, one facing, one jump and one attack (the per-tick
//! budget). Slots resolve in phases, never in caller order (ADR 0044):
//!
//! 1. airborne momentum carries every jumping body;
//! 2. ground movement, then facing, for every actor;
//! 3. jumps for every actor;
//! 4. attacks: every attack is validated against the same post-movement
//!    state and their damage lands together, so swapping actor order never
//!    changes the outcome and two lethal hits on the same tick both land
//!    (when both teams fall, the round is a draw);
//! 5. vertical motion.
//!
//! Movement and jumps never depend on another actor (bodies do not collide
//! with each other), so phases 2 and 3 are order independent too.

use crate::TICKS_PER_SECOND;
use crate::map::{Layout, MAX_TEAM};

/// 7.2 m/s at 30 Hz.
pub const MOVE_PER_TICK: i32 = 240;
/// Millimetres/tick and millimetres/tick² (30 Hz): ~0.77 s airtime, 1.32 m apex.
pub const JUMP_SPEED: i32 = 220;
pub const GRAVITY: i32 = 20;
pub const BODY_RADIUS: i32 = 450;
pub const ATTACK_RANGE: i32 = 6_000;
pub const ATTACK_COOLDOWN: u64 = TICKS_PER_SECOND;
pub const ATTACK_DAMAGE: u16 = 25;
pub const MAX_HEALTH: u16 = 100;
/// Movement axes are quantized to this magnitude on each axis.
pub const AXIS_LIMIT: i16 = 1000;
/// Binary angle of team 1's spawn heading: half a turn from team 0.
pub const HALF_TURN: u16 = 0x8000;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combatant {
    pub position: Position,
    pub height: i32,
    pub vertical_velocity: i32,
    /// Actual ground displacement at takeoff; no air steering or speed stacking.
    pub momentum: Position,
    pub health: u16,
    pub ready_at: u64,
    /// Heading as a binary angle: 65536 units per turn, 0 faces −Z and
    /// angles grow counter-clockwise seen from above (a yaw about +Y). Every
    /// value is valid; no rule reads it yet, but it is authoritative state
    /// that snapshots relay to other players.
    pub facing: u16,
}

/// Planar movement intent. Each axis must lie in
/// `[-AXIS_LIMIT, AXIS_LIMIT]`; the rules normalize the length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Axes {
    pub x: i16,
    pub z: i16,
}

/// One actor's intent for one tick. `None`/`false` fields are idle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActorInput {
    pub movement: Option<Axes>,
    pub facing: Option<u16>,
    pub jump: bool,
    /// Attack this actor index.
    pub attack: Option<usize>,
}
impl ActorInput {
    pub fn is_idle(&self) -> bool {
        *self == Self::default()
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
    Attack {
        actor: usize,
        target: usize,
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
    OutOfRange,
    LineOfSight,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoundResult {
    /// Every member of the other team is defeated. Teams are actor parity.
    Won { team: usize },
    /// Both teams were eliminated on the same tick.
    Draw,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaEvent {
    Jumped {
        actor: usize,
    },
    Moved {
        actor: usize,
    },
    /// `damage` is this hit's own amount against the target's health before
    /// the attack phase; simultaneous hits never see each other.
    Hit {
        actor: usize,
        target: usize,
        damage: u16,
    },
    Finished(RoundResult),
    /// `actor` is the slot (or the actor a scripted command named) whose
    /// intent was refused, so a server can route it to that player only.
    Rejected {
        actor: usize,
        reason: Rejection,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arena {
    tick: u64,
    layout: Layout,
    fighters: Vec<Combatant>,
    result: Option<RoundResult>,
}
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
        let fighters = (0..team_size * 2)
            .map(|index| Combatant {
                position: spawns[index % 2][index / 2],
                health: MAX_HEALTH,
                ready_at: 0,
                height: 0,
                vertical_velocity: 0,
                momentum: Position { x: 0, z: 0 },
                // Team 0's rooms lie toward +Z, so it faces −Z (angle 0).
                facing: if index % 2 == 0 { 0 } else { HALF_TURN },
            })
            .collect();
        Some(Self {
            tick: 0,
            layout,
            fighters,
            result: None,
        })
    }
}
impl Arena {
    /// Rebuilds an arena from authoritative state, as a client presenting a
    /// server's snapshot does. `None` unless there are 2–6 fighters in
    /// teams of equal size.
    pub fn restore(
        layout: Layout,
        tick: u64,
        fighters: Vec<Combatant>,
        result: Option<RoundResult>,
    ) -> Option<Self> {
        if !(2..=MAX_TEAM * 2).contains(&fighters.len()) || !fighters.len().is_multiple_of(2) {
            return None;
        }
        Some(Self {
            tick,
            layout,
            fighters,
            result,
        })
    }
    pub fn tick(&self) -> u64 {
        self.tick
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
        true
    }
    pub fn result(&self) -> Option<RoundResult> {
        self.result
    }
    pub fn can_attack(&self, actor: usize, target: usize) -> Result<(), Rejection> {
        if self.result.is_some() {
            return Err(Rejection::MatchOver);
        }
        let Some(attacker) = self.fighters.get(actor) else {
            return Err(Rejection::InvalidActor);
        };
        let Some(defender) = self.fighters.get(target) else {
            return Err(Rejection::InvalidTarget);
        };
        if attacker.health == 0 {
            return Err(Rejection::InvalidActor);
        }
        if actor % 2 == target % 2 || defender.health == 0 {
            return Err(Rejection::InvalidTarget);
        }
        if self.tick < attacker.ready_at {
            return Err(Rejection::Cooldown);
        }
        if attacker.position.distance_squared(defender.position) > i64::from(ATTACK_RANGE).pow(2) {
            return Err(Rejection::OutOfRange);
        }
        if !self
            .layout
            .line_of_sight(attacker.position, defender.position)
        {
            return Err(Rejection::LineOfSight);
        }
        Ok(())
    }

    /// Bins a scripted command list into per-actor slots, then advances one
    /// tick. Commands naming a missing actor and second commands of a kind
    /// for the same actor are rejected first, in list order; everything else
    /// resolves exactly as [`Arena::advance`].
    pub fn step(&mut self, commands: &[ArenaCommand]) -> Vec<ArenaEvent> {
        let mut events = Vec::new();
        let mut inputs = vec![ActorInput::default(); self.fighters.len()];
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
                    slot.attack.is_none() && {
                        slot.attack = Some(target);
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
        events.extend(self.advance(&inputs));
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
                    + usize::from(input.attack.is_some());
                for _ in 0..commands {
                    reject(&mut events, actor, Rejection::MatchOver);
                }
            }
            self.tick += 1;
            return events;
        }

        // 1. Existing airborne momentum resolves first. Ground momentum
        // expires without a Move, preventing stale running jumps.
        for fighter in &mut self.fighters {
            if fighter.height > 0 {
                fighter.position = self.layout.slide(fighter.position, fighter.momentum);
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
                events.push(ArenaEvent::Jumped { actor });
            }
        }

        // 4. Attacks, validated against one shared state and applied together.
        let mut incoming = vec![0u16; count];
        for (actor, input) in inputs.iter().enumerate() {
            let Some(target) = input.attack else {
                continue;
            };
            if let Err(reason) = self.can_attack(actor, target) {
                reject(&mut events, actor, reason);
                continue;
            }
            let damage = ATTACK_DAMAGE.min(self.fighters[target].health);
            incoming[target] = incoming[target].saturating_add(damage);
            self.fighters[actor].ready_at = self.tick + ATTACK_COOLDOWN;
            events.push(ArenaEvent::Hit {
                actor,
                target,
                damage,
            });
        }
        for (fighter, damage) in self.fighters.iter_mut().zip(incoming) {
            fighter.health = fighter.health.saturating_sub(damage);
        }
        let standing = |team: usize| {
            self.fighters
                .iter()
                .enumerate()
                .any(|(index, fighter)| index % 2 == team && fighter.health > 0)
        };
        self.result = match (standing(0), standing(1)) {
            (true, true) => None,
            (true, false) => Some(RoundResult::Won { team: 0 }),
            (false, true) => Some(RoundResult::Won { team: 1 }),
            (false, false) => Some(RoundResult::Draw),
        };
        if let Some(result) = self.result {
            events.push(ArenaEvent::Finished(result));
            self.tick += 1;
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
        self.tick += 1;
        events
    }

    /// Deterministic 64-bit FNV-1a digest of the tick, result and every
    /// fighter, for comparing a server's state with a replay or a client's
    /// prediction. The layout is fixed per match and versioned separately.
    pub fn state_hash(&self) -> u64 {
        let mut hash = Fnv::default();
        hash.write(&self.tick.to_le_bytes());
        hash.write(&match self.result {
            None => [0, 0],
            Some(RoundResult::Won { team }) => [1, team as u8],
            Some(RoundResult::Draw) => [2, 0],
        });
        for fighter in &self.fighters {
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
            hash.write(&fighter.ready_at.to_le_bytes());
            hash.write(&fighter.facing.to_le_bytes());
        }
        hash.0
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
                    while arena.tick() < arena.fighters[0].ready_at {
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
        let ready = arena.fighters()[0].ready_at;
        while arena.tick() < ready {
            assert_eq!(
                arena.step(&[ATTACK]),
                vec![rejected(0, Rejection::Cooldown)]
            );
        }
        for _ in 0..3 {
            while arena.tick() < arena.fighters()[0].ready_at {
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
        assert!(
            [0, 1, 3]
                .iter()
                .all(|&actor| arena.fighters[actor].ready_at == arena.tick() - 1 + ATTACK_COOLDOWN)
        );
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
            attack: Some(1),
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
            layout: fixture(),
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
                    ready_at: 0,
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
                    ready_at: 30,
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
        let mutations: [fn(&mut Arena); 6] = [
            |a| a.fighters[0].facing = 1,
            |a| a.fighters[1].health -= 1,
            |a| a.fighters[0].position.x += 1,
            |a| a.fighters[1].ready_at += 1,
            |a| a.tick += 1,
            |a| a.result = Some(RoundResult::Draw),
        ];
        for mutate in mutations {
            mutate(&mut arena);
            let hash = arena.state_hash();
            assert!(!seen.contains(&hash));
            seen.push(hash);
        }
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
    }
    const GOLDEN_STATE_HASH: u64 = 395_746_746_345_043_275;
}
