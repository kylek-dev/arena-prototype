//! Planar arena rules for the browser spike; positions are integer millimetres.
//! Actor order is supplied by the caller; this is a practice scene, not a fair
//! competitive scheduler. No rendering, networking, floating point, or OS time.

use crate::TICKS_PER_SECOND;

pub const MOVE_PER_TICK: i32 = 160;
pub const BODY_RADIUS: i32 = 450;
// Half-extents of the playable floor, in millimetres.
pub const ARENA_X: i32 = 18_000;
pub const ARENA_Z: i32 = 14_000;
pub const ATTACK_RANGE: i32 = 6_000;
pub const ATTACK_COOLDOWN: u64 = TICKS_PER_SECOND;
pub const ATTACK_DAMAGE: u16 = 25;

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
pub struct Pillar {
    pub center: Position,
    pub half_size: i32,
}
impl Pillar {
    fn contains(self, point: Position, padding: i32) -> bool {
        (point.x - self.center.x).abs() <= self.half_size + padding
            && (point.z - self.center.z).abs() <= self.half_size + padding
    }
}
pub const PILLARS: [Pillar; 4] = [
    Pillar {
        center: Position {
            x: -7_000,
            z: -5_000,
        },
        half_size: 1_500,
    },
    Pillar {
        center: Position {
            x: 7_000,
            z: -5_000,
        },
        half_size: 1_500,
    },
    Pillar {
        center: Position {
            x: -7_000,
            z: 5_000,
        },
        half_size: 1_500,
    },
    Pillar {
        center: Position { x: 7_000, z: 5_000 },
        half_size: 1_500,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combatant {
    pub position: Position,
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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    InvalidActor,
    InvalidTarget,
    InvalidMovement,
    AlreadyMoved,
    MatchOver,
    Cooldown,
    OutOfRange,
    LineOfSight,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaEvent {
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
    fighters: [Combatant; 2],
    winner: Option<usize>,
}
impl Default for Arena {
    fn default() -> Self {
        Self {
            tick: 0,
            fighters: [
                Combatant {
                    position: Position { x: 0, z: 5_500 },
                    health: 100,
                    ready_at: 0,
                },
                Combatant {
                    position: Position { x: 0, z: -3_500 },
                    health: 100,
                    ready_at: 0,
                },
            ],
            winner: None,
        }
    }
}
impl Arena {
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn fighters(&self) -> &[Combatant; 2] {
        &self.fighters
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
        if actor == target {
            return Err(Rejection::InvalidTarget);
        }
        if self.tick < attacker.ready_at {
            return Err(Rejection::Cooldown);
        }
        if attacker.position.distance_squared(defender.position) > i64::from(ATTACK_RANGE).pow(2) {
            return Err(Rejection::OutOfRange);
        }
        if !line_of_sight(attacker.position, defender.position) {
            return Err(Rejection::LineOfSight);
        }
        Ok(())
    }
    pub fn step(&mut self, commands: &[ArenaCommand]) -> Vec<ArenaEvent> {
        let mut events = Vec::new();
        let mut moved = [false; 2];
        for &command in commands {
            if self.winner.is_some() {
                events.push(ArenaEvent::Rejected(Rejection::MatchOver));
                continue;
            }
            match command {
                ArenaCommand::Move { actor, x, z } => {
                    if actor >= self.fighters.len() {
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
                    let length = ((i64::from(x).pow(2) + i64::from(z).pow(2)) as u64)
                        .isqrt()
                        .max(1000) as i32;
                    let dx = i32::from(x) * MOVE_PER_TICK / length;
                    let dz = i32::from(z) * MOVE_PER_TICK / length;
                    let p = &mut self.fighters[actor].position;
                    // Axis-separated collision permits sliding; maximum step is smaller
                    // than pillar thickness, so a valid command cannot tunnel through.
                    let next = Position {
                        x: (p.x + dx).clamp(-ARENA_X + BODY_RADIUS, ARENA_X - BODY_RADIUS),
                        z: p.z,
                    };
                    if !PILLARS
                        .iter()
                        .any(|pillar| pillar.contains(next, BODY_RADIUS))
                    {
                        *p = next;
                    }
                    let next = Position {
                        x: p.x,
                        z: (p.z + dz).clamp(-ARENA_Z + BODY_RADIUS, ARENA_Z - BODY_RADIUS),
                    };
                    if !PILLARS
                        .iter()
                        .any(|pillar| pillar.contains(next, BODY_RADIUS))
                    {
                        *p = next;
                    }
                    events.push(ArenaEvent::Moved { actor });
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
                    if self.fighters[target].health == 0 {
                        self.winner = Some(actor);
                        events.push(ArenaEvent::Won { actor });
                    }
                }
            }
        }
        self.tick += 1;
        events
    }
}

/// Segment-versus-rectangle tests use the same pillar footprints as collision.
pub fn line_of_sight(start: Position, end: Position) -> bool {
    !PILLARS.iter().any(|pillar| {
        if pillar.contains(start, 0) || pillar.contains(end, 0) {
            return true;
        }
        let x = pillar.center.x;
        let z = pillar.center.z;
        let r = pillar.half_size;
        let corners = [
            Position { x: x - r, z: z - r },
            Position { x: x + r, z: z - r },
            Position { x: x + r, z: z + r },
            Position { x: x - r, z: z + r },
        ];
        (0..4).any(|i| segments_intersect(start, end, corners[i], corners[(i + 1) % 4]))
    })
}
fn segments_intersect(a: Position, b: Position, c: Position, d: Position) -> bool {
    fn cross(a: Position, b: Position, c: Position) -> i64 {
        (i64::from(b.x) - i64::from(a.x)) * (i64::from(c.z) - i64::from(a.z))
            - (i64::from(b.z) - i64::from(a.z)) * (i64::from(c.x) - i64::from(a.x))
    }
    let overlaps = a.x.min(b.x) <= c.x.max(d.x)
        && c.x.min(d.x) <= a.x.max(b.x)
        && a.z.min(b.z) <= c.z.max(d.z)
        && c.z.min(d.z) <= a.z.max(b.z);
    overlaps
        && cross(a, b, c).signum() * cross(a, b, d).signum() <= 0
        && cross(c, d, a).signum() * cross(c, d, b).signum() <= 0
}

#[cfg(test)]
mod tests {
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
    #[test]
    fn attack_checks_range_cooldown_and_match_end() {
        let mut arena = Arena::default();
        assert_eq!(
            arena.step(&[ATTACK]),
            vec![ArenaEvent::Rejected(Rejection::OutOfRange)]
        );
        for _ in 0..20 {
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
        let mut arena = Arena::default();
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
        for pillar in PILLARS {
            for (x, z) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let mut arena = Arena::default();
                arena.fighters[0].position = Position {
                    x: pillar.center.x + x * 2_500,
                    z: pillar.center.z + z * 2_500,
                };
                arena.fighters[1].position = Position {
                    x: pillar.center.x - x * 2_500,
                    z: pillar.center.z - z * 2_500,
                };
                assert_eq!(arena.can_attack(0, 1), Err(Rejection::LineOfSight));
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
                assert!(clearance > pillar.half_size + BODY_RADIUS);
                assert!(clearance <= pillar.half_size + BODY_RADIUS + MOVE_PER_TICK);
            }
            // A segment touching the edge is blocked too.
            assert!(!line_of_sight(
                Position {
                    x: pillar.center.x + pillar.half_size,
                    z: pillar.center.z + 2500
                },
                Position {
                    x: pillar.center.x + pillar.half_size,
                    z: pillar.center.z - 2500
                }
            ));
        }
        assert!(line_of_sight(
            Position { x: 0, z: ARENA_Z },
            Position { x: 0, z: -ARENA_Z }
        ));
        assert!(line_of_sight(
            Position { x: -ARENA_X, z: 0 },
            Position { x: ARENA_X, z: 0 }
        ));
    }
    #[test]
    fn replay_and_world_bounds_are_repeatable() {
        // Use the open middle lane so this tests the wall, not a pillar.
        let mut a = Arena::default();
        a.fighters[0].position = Position { x: 0, z: 0 };
        let mut b = Arena::default();
        b.fighters[0].position = Position { x: 0, z: 0 };
        for _ in 0..200 {
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
        let initial = Arena::default();
        for fighter in initial.fighters() {
            assert!(fighter.position.x.abs() < ARENA_X - BODY_RADIUS);
            assert!(fighter.position.z.abs() < ARENA_Z - BODY_RADIUS);
            assert!(
                !PILLARS
                    .iter()
                    .any(|p| p.contains(fighter.position, BODY_RADIUS))
            );
        }
        for (x, z) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut arena = Arena::default();
            arena.fighters[0].position = Position { x: 0, z: 0 };
            for _ in 0..200 {
                arena.step(&[ArenaCommand::Move {
                    actor: 0,
                    x: x * 1000,
                    z: z * 1000,
                }]);
            }
            assert_eq!(
                arena.fighters()[0].position,
                Position {
                    x: i32::from(x) * (ARENA_X - BODY_RADIUS),
                    z: i32::from(z) * (ARENA_Z - BODY_RADIUS),
                }
            );
        }
    }
}
