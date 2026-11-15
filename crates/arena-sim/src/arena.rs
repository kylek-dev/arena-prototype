//! Planar arena rules for the browser spike; positions are integer millimetres.
//! Actor order is supplied by the caller; this is a practice scene, not a fair
//! competitive scheduler. No rendering, networking, floating point, or OS time.

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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaCommand {
    /// Axes are quantized to [-1000, 1000] and normalized by the rules.
    Move {
        actor: usize,
        x: i16,
        z: i16,
    },
    Attack {
        actor: usize,
        target: usize,
    },
    Jump {
        actor: usize,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    InvalidActor,
    InvalidTarget,
    InvalidMovement,
    AlreadyMoved,
    Airborne,
    MatchOver,
    Cooldown,
    OutOfRange,
    LineOfSight,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaEvent {
    Jumped {
        actor: usize,
    },
    Moved {
        actor: usize,
    },
    Hit {
        actor: usize,
        target: usize,
        damage: u16,
    },
    Won {
        actor: usize,
    },
    Rejected(Rejection),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arena {
    tick: u64,
    layout: Layout,
    fighters: Vec<Combatant>,
    winner: Option<usize>,
}
impl Arena {
    /// Local practice roster: even actors are friendly, odd actors are enemies.
    /// Actor zero is the player; command ordering remains caller supplied.
    /// Team `t`'s `k`th member starts at that team's `k`th room spawn. The
    /// layout must already be valid (see `Layout::validate`).
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
            })
            .collect();
        Some(Self {
            tick: 0,
            layout,
            fighters,
            winner: None,
        })
    }
}
impl Arena {
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
    pub fn winner(&self) -> Option<usize> {
        self.winner
    }
    pub fn can_attack(&self, actor: usize, target: usize) -> Result<(), Rejection> {
        if self.winner.is_some() {
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
    pub fn step(&mut self, commands: &[ArenaCommand]) -> Vec<ArenaEvent> {
        let mut events = Vec::new();
        let mut moved = vec![false; self.fighters.len()];
        // Existing airborne momentum resolves before this tick's ordered commands.
        // Ground momentum expires without a Move, preventing stale running jumps.
        if self.winner.is_none() {
            for fighter in &mut self.fighters {
                if fighter.height > 0 {
                    fighter.position = self.layout.slide(fighter.position, fighter.momentum);
                } else {
                    fighter.momentum = Position { x: 0, z: 0 };
                }
            }
        }
        for &command in commands {
            if self.winner.is_some() {
                events.push(ArenaEvent::Rejected(Rejection::MatchOver));
                continue;
            }
            match command {
                ArenaCommand::Move { actor, x, z } => {
                    if actor >= self.fighters.len() || self.fighters[actor].health == 0 {
                        events.push(ArenaEvent::Rejected(Rejection::InvalidActor));
                        continue;
                    }
                    if moved[actor] {
                        events.push(ArenaEvent::Rejected(Rejection::AlreadyMoved));
                        continue;
                    }
                    if !(-1000..=1000).contains(&x) || !(-1000..=1000).contains(&z) {
                        events.push(ArenaEvent::Rejected(Rejection::InvalidMovement));
                        continue;
                    }
                    moved[actor] = true;
                    if self.fighters[actor].height > 0 {
                        continue;
                    }
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
                ArenaCommand::Jump { actor } => {
                    let Some(fighter) = self.fighters.get_mut(actor) else {
                        events.push(ArenaEvent::Rejected(Rejection::InvalidActor));
                        continue;
                    };
                    if fighter.health == 0 {
                        events.push(ArenaEvent::Rejected(Rejection::InvalidActor));
                        continue;
                    }
                    if fighter.height > 0 || fighter.vertical_velocity > 0 {
                        events.push(ArenaEvent::Rejected(Rejection::Airborne));
                        continue;
                    }
                    fighter.vertical_velocity = JUMP_SPEED;
                    events.push(ArenaEvent::Jumped { actor });
                }
                ArenaCommand::Attack { actor, target } => {
                    if let Err(reason) = self.can_attack(actor, target) {
                        events.push(ArenaEvent::Rejected(reason));
                        continue;
                    }
                    let damage = ATTACK_DAMAGE.min(self.fighters[target].health);
                    self.fighters[target].health -= damage;
                    self.fighters[actor].ready_at = self.tick + ATTACK_COOLDOWN;
                    events.push(ArenaEvent::Hit {
                        actor,
                        target,
                        damage,
                    });
                    if self
                        .fighters
                        .iter()
                        .enumerate()
                        .all(|(index, fighter)| index % 2 == actor % 2 || fighter.health == 0)
                    {
                        self.winner = Some(actor);
                        events.push(ArenaEvent::Won { actor });
                    }
                }
            }
        }
        if self.winner.is_none() {
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
        }
        self.tick += 1;
        events
    }
}

#[cfg(test)]
mod tests {
    use crate::map::{Block, fixture};
    /// Floor half-extents of the synthetic fixture layout.
    const ARENA_X: i32 = 54_000;
    const ARENA_Z: i32 = 42_000;
    fn default_arena() -> Arena {
        Arena::practice(fixture(), 1).unwrap()
    }
    fn pillars() -> Vec<Block> {
        fixture().pillars
    }
    fn line_of_sight(a: Position, b: Position) -> bool {
        fixture().line_of_sight(a, b)
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
                    vec![ArenaEvent::Rejected(Rejection::InvalidTarget)]
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
                assert_eq!(arena.winner(), (target == size * 2 - 1).then_some(0));
                if arena.winner().is_none() {
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
                    .contains(&ArenaEvent::Rejected(Rejection::Airborne))
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
                ArenaEvent::Jumped { actor: 0 },
                ArenaEvent::Rejected(Rejection::Airborne),
            ]
        );
        for _ in 0..30 {
            arena.step(&[]);
        }
        assert_eq!(arena.fighters[0].position, before);
        assert_eq!(arena.fighters[0].height, 0);
        assert_eq!(
            arena.step(&[ArenaCommand::Jump { actor: usize::MAX }]),
            vec![ArenaEvent::Rejected(Rejection::InvalidActor)]
        );
        arena.winner = Some(1);
        assert_eq!(
            arena.step(&[JUMP]),
            vec![ArenaEvent::Rejected(Rejection::MatchOver)]
        );
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
        assert_eq!(events[0], ArenaEvent::Rejected(Rejection::InvalidMovement));
        assert_eq!(events[1], ArenaEvent::Rejected(Rejection::AlreadyMoved));
        assert_eq!(arena.fighters[0].health, 75);
        assert_eq!(arena.fighters[0].position.z, before.z - MOVE_PER_TICK);
        assert!(arena.fighters[0].height > 0);
        arena.winner = Some(1);
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
            vec![ArenaEvent::Rejected(Rejection::OutOfRange)]
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
                vec![ArenaEvent::Rejected(Rejection::Cooldown)]
            );
        }
        for _ in 0..3 {
            while arena.tick() < arena.fighters()[0].ready_at {
                arena.step(&[]);
            }
            arena.step(&[ATTACK]);
        }
        assert_eq!(arena.winner(), Some(0));
        assert_eq!(
            arena.step(&[forward(), ATTACK]),
            vec![ArenaEvent::Rejected(Rejection::MatchOver); 2]
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
                ArenaEvent::Rejected(Rejection::InvalidActor),
                ArenaEvent::Rejected(Rejection::InvalidMovement),
                ArenaEvent::Rejected(Rejection::InvalidTarget)
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
            // Within range, a sight line clipping a corner is still rejected.
            let mut arena = default_arena();
            let (cx, cz, h) = (pillar.center.x, pillar.center.z, pillar.half_x);
            arena.fighters[0].position = Position {
                x: cx + h + 500,
                z: cz + h - 2_000,
            };
            arena.fighters[1].position = Position {
                x: cx + h - 2_000,
                z: cz + h + 500,
            };
            assert_eq!(arena.can_attack(0, 1), Err(Rejection::LineOfSight));
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
}
