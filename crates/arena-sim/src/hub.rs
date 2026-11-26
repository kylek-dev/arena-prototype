//! The social hub's rules ([ADR 0056](../../../docs/decisions/0056-social-hub.md)):
//! walking around a graybox plaza, nothing else. Integer millimetres and a
//! fixed hub tick, no clocks, I/O or rendering, so the front door integrates
//! intents with exactly the code the browser predicts with.
//!
//! There is no combat and bodies pass through each other (nobody can block
//! a doorway); the plaza's edge and its columns are solid.

use crate::TICKS_PER_SECOND;
use crate::arena::{AXIS_LIMIT, Axes, BODY_RADIUS, GRAVITY, JUMP_SPEED, MOVE_PER_TICK, Position};

/// Hub ticks per second: movement is integrated and sent this often.
pub const HUB_TICKS_PER_SECOND: u64 = 10;
const _: () = assert!(TICKS_PER_SECOND.is_multiple_of(HUB_TICKS_PER_SECOND));
/// The arena's walking speed (7.2 m/s), per hub tick. One hub step can
/// cover up to 2 mm more than three truncated arena steps; the arena also
/// slides along walls where the hub pushes out of columns.
pub const HUB_MOVE_PER_TICK: i32 = MOVE_PER_TICK * (TICKS_PER_SECOND / HUB_TICKS_PER_SECOND) as i32;
/// The plaza is a square floor `2 × PLAZA_HALF` on a side (80 m).
pub const PLAZA_HALF: i32 = 40_000;
/// Players see the others within this distance (40 m). On the 80 m
/// graybox plaza this filters little; it bounds the cost of larger hubs.
pub const INTEREST_RADIUS: i32 = 40_000;
/// Most players in one hub instance.
pub const MAX_HUB_PLAYERS: usize = 40;

/// A solid round column standing on the plaza.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Column {
    pub center: Position,
    pub radius: i32,
}

/// The graybox plaza: a fountain in the middle and four columns around it.
pub const COLUMNS: [Column; 5] = [
    Column {
        center: Position { x: 0, z: 0 },
        radius: 4_000,
    },
    Column {
        center: Position {
            x: -16_000,
            z: -16_000,
        },
        radius: 1_500,
    },
    Column {
        center: Position {
            x: 16_000,
            z: -16_000,
        },
        radius: 1_500,
    },
    Column {
        center: Position {
            x: -16_000,
            z: 16_000,
        },
        radius: 1_500,
    },
    Column {
        center: Position {
            x: 16_000,
            z: 16_000,
        },
        radius: 1_500,
    },
];

/// Where arrivals appear: rows south of the fountain, one place per player
/// of a full instance, so a crowd arriving together does not stand in one
/// spot.
pub const SPAWN: Position = Position { x: 0, z: 24_000 };
const SPAWN_SPACING: i32 = 1_500;
const SPAWN_PER_ROW: u32 = 8;
const SPAWN_ROWS: u32 = MAX_HUB_PLAYERS.div_ceil(SPAWN_PER_ROW as usize) as u32;

// Columns stand apart and well inside the edge, so pushing out of one never
// lands in another or outside the plaza.
const _: () = {
    let mut i = 0;
    while i < COLUMNS.len() {
        let a = COLUMNS[i];
        let reach = a.radius + BODY_RADIUS;
        assert!(a.center.x.abs() + reach < PLAZA_HALF - BODY_RADIUS);
        assert!(a.center.z.abs() + reach < PLAZA_HALF - BODY_RADIUS);
        let mut j = i + 1;
        while j < COLUMNS.len() {
            let b = COLUMNS[j];
            let (dx, dz) = (
                (a.center.x - b.center.x) as i64,
                (a.center.z - b.center.z) as i64,
            );
            let gap = (reach + b.radius + BODY_RADIUS) as i64;
            assert!(dx * dx + dz * dz > gap * gap);
            j += 1;
        }
        i += 1;
    }
    // Spawn rows stay clear of the fountain and inside the plaza.
    assert!(SPAWN.z - COLUMNS[0].radius - BODY_RADIUS > 0);
    assert!(SPAWN.z + SPAWN_ROWS as i32 * SPAWN_SPACING < PLAZA_HALF - BODY_RADIUS);
};

/// What a player asks for each hub tick: a ground direction (each axis in
/// `[-AXIS_LIMIT, AXIS_LIMIT]`, normalized as in the arena, so a full stick
/// walks at full speed and a small one slower) and a facing (a binary angle:
/// 65536 per turn, as `Combatant::facing`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Intent {
    pub movement: Axes,
    pub facing: u16,
    /// Jump this tick (only from the ground, as in the arena).
    pub jump: bool,
}

impl Intent {
    /// Whether every axis is in range; the door refuses anything else.
    pub fn is_valid(self) -> bool {
        (-AXIS_LIMIT..=AXIS_LIMIT).contains(&self.movement.x)
            && (-AXIS_LIMIT..=AXIS_LIMIT).contains(&self.movement.z)
    }
}

/// Where `position` is after one hub tick of `intent`: at most
/// [`HUB_MOVE_PER_TICK`] away, inside the plaza and outside every column.
/// An invalid intent stands still.
pub fn step(position: Position, intent: Intent) -> Position {
    let Axes { x, z } = intent.movement;
    if !intent.is_valid() || (x == 0 && z == 0) {
        return settle(position);
    }
    let length = ((i64::from(x).pow(2) + i64::from(z).pow(2)) as u64)
        .isqrt()
        .max(AXIS_LIMIT as u64) as i32;
    let start = settle(position);
    let moved = Position {
        x: start.x + i32::from(x) * HUB_MOVE_PER_TICK / length,
        z: start.z + i32::from(z) * HUB_MOVE_PER_TICK / length,
    };
    settle(moved)
}

/// Arena ticks per hub tick: the vertical motion runs the arena's own
/// per-tick jump arithmetic this many times, so a hub jump is an arena jump.
const ARENA_TICKS_PER_HUB_TICK: u32 = (TICKS_PER_SECOND / HUB_TICKS_PER_SECOND) as u32;

/// A hub player's body: where they stand, how high they are, and the
/// ground displacement they carry through a jump (no steering in the air,
/// as in the arena).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Body {
    pub at: Position,
    /// Millimetres above the plaza.
    pub height: i32,
    /// Millimetres per arena tick, upwards.
    pub vertical: i32,
    /// Displacement per hub tick kept from the take-off.
    pub momentum: Position,
}

/// No displacement.
const STILL: Position = Position { x: 0, z: 0 };

impl Body {
    pub fn standing(at: Position) -> Self {
        Self {
            at: settle(at),
            height: 0,
            vertical: 0,
            momentum: STILL,
        }
    }

    pub fn airborne(self) -> bool {
        self.height > 0 || self.vertical > 0
    }
}

/// One hub tick of a body, in the arena's order: momentum carries an
/// airborne body; on the ground the intent walks it (and the step becomes
/// its momentum); a jump leaves the ground; then the vertical motion runs
/// the arena's arithmetic for each arena tick in the hub tick. An invalid
/// intent neither walks nor jumps.
pub fn step_body(body: Body, intent: Intent) -> Body {
    let mut next = body;
    if body.airborne() {
        next.at = settle(Position {
            x: body.at.x.saturating_add(body.momentum.x),
            z: body.at.z.saturating_add(body.momentum.z),
        });
    } else {
        next.momentum = STILL;
        if intent.is_valid() {
            next.at = step(body.at, intent);
            next.momentum = Position {
                x: next.at.x - body.at.x,
                z: next.at.z - body.at.z,
            };
            if intent.jump {
                next.vertical = JUMP_SPEED;
            }
        }
    }
    for _ in 0..ARENA_TICKS_PER_HUB_TICK {
        if next.airborne() {
            next.height = (next.height + next.vertical).max(0);
            next.vertical -= GRAVITY;
            if next.height == 0 {
                next.vertical = 0;
                next.momentum = STILL;
            }
        }
    }
    next
}

/// The nearest walkable place to `position`: inside the plaza's edge and
/// pushed out of any column it overlaps, along the line from its centre.
pub fn settle(position: Position) -> Position {
    let limit = PLAZA_HALF - BODY_RADIUS;
    let mut at = Position {
        x: position.x.clamp(-limit, limit),
        z: position.z.clamp(-limit, limit),
    };
    for column in COLUMNS {
        let reach = column.radius + BODY_RADIUS;
        let (dx, dz) = (at.x - column.center.x, at.z - column.center.z);
        let distance_squared = i64::from(dx).pow(2) + i64::from(dz).pow(2);
        if distance_squared >= i64::from(reach).pow(2) {
            continue;
        }
        // Dead centre has no direction: step out to the south.
        let (dx, dz, distance) = if distance_squared == 0 {
            (0, 1, 1)
        } else {
            (dx, dz, (distance_squared as u64).isqrt().max(1) as i32)
        };
        // Round outward so the result is never still inside.
        let out = |d: i32| {
            let scaled = i64::from(d) * i64::from(reach);
            (scaled + scaled.signum() * (i64::from(distance) - 1)) / i64::from(distance)
        };
        at = Position {
            x: column.center.x + out(dx) as i32,
            z: column.center.z + out(dz) as i32,
        };
    }
    // Columns sit well inside the edge, so pushing out never leaves it.
    at
}

/// The `index`th arrival's place: rows of eight south of [`SPAWN`], one
/// place for each player of a full instance, then the same places again.
pub fn spawn_point(index: u32) -> Position {
    let row = (index / SPAWN_PER_ROW) % SPAWN_ROWS;
    let column = (index % SPAWN_PER_ROW) as i32 - (SPAWN_PER_ROW as i32 / 2);
    settle(Position {
        x: SPAWN.x + column * SPAWN_SPACING + SPAWN_SPACING / 2,
        z: SPAWN.z + row as i32 * SPAWN_SPACING,
    })
}

/// Whether a player at `from` sees one at `other`.
pub fn in_interest(from: Position, other: Position) -> bool {
    from.distance_squared(other) <= i64::from(INTEREST_RADIUS).pow(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn walk(x: i16, z: i16) -> Intent {
        Intent {
            movement: Axes { x, z },
            facing: 0,
            jump: false,
        }
    }

    fn inside_a_column(at: Position) -> bool {
        COLUMNS.iter().any(|column| {
            at.distance_squared(column.center) < i64::from(column.radius + BODY_RADIUS).pow(2)
        })
    }

    #[test]
    fn walks_at_arena_speed_and_never_faster() {
        let start = spawn_point(0);
        assert_eq!(HUB_MOVE_PER_TICK, 720);
        let east = step(start, walk(AXIS_LIMIT, 0));
        assert_eq!((east.x - start.x, east.z - start.z), (HUB_MOVE_PER_TICK, 0));
        // Diagonals and oversized sticks are normalized, as in the arena.
        for intent in [walk(AXIS_LIMIT, AXIS_LIMIT), walk(-700, 700), walk(1, -1)] {
            let next = step(start, intent);
            assert!(start.distance_squared(next) <= i64::from(HUB_MOVE_PER_TICK).pow(2));
        }
        // A small stick walks slower; no input stands still.
        let slow = step(start, walk(500, 0));
        assert_eq!(slow.x - start.x, HUB_MOVE_PER_TICK / 2);
        assert_eq!(step(start, walk(0, 0)), start);
    }

    #[test]
    fn refuses_out_of_range_axes() {
        let start = spawn_point(3);
        for intent in [
            walk(AXIS_LIMIT + 1, 0),
            walk(0, i16::MIN),
            walk(i16::MAX, i16::MAX),
        ] {
            assert!(!intent.is_valid());
            assert_eq!(step(start, intent), start);
        }
    }

    #[test]
    fn keeps_players_inside_the_plaza_and_out_of_columns() {
        // Walking into each wall for long enough stops at the edge.
        for intent in [
            walk(AXIS_LIMIT, 0),
            walk(-AXIS_LIMIT, 0),
            walk(0, AXIS_LIMIT),
            walk(0, -AXIS_LIMIT),
        ] {
            let mut at = spawn_point(0);
            for _ in 0..200 {
                at = step(at, intent);
                assert!(
                    at.x.abs() <= PLAZA_HALF - BODY_RADIUS
                        && at.z.abs() <= PLAZA_HALF - BODY_RADIUS
                );
                assert!(!inside_a_column(at), "{at:?}");
            }
        }
        // Walking straight at the fountain slides to its surface.
        let mut at = Position { x: 0, z: 10_000 };
        for _ in 0..30 {
            at = step(at, walk(0, -AXIS_LIMIT));
            assert!(!inside_a_column(at), "{at:?}");
        }
        assert!(at.z >= COLUMNS[0].radius + BODY_RADIUS);
        // Stepping from an unsettled extreme cannot overflow.
        for raw in [
            Position {
                x: i32::MAX,
                z: i32::MAX,
            },
            Position {
                x: i32::MIN,
                z: i32::MIN,
            },
        ] {
            let at = step(raw, walk(AXIS_LIMIT, -AXIS_LIMIT));
            assert!(
                at.x.abs() <= PLAZA_HALF - BODY_RADIUS && at.z.abs() <= PLAZA_HALF - BODY_RADIUS
            );
        }
        // Any place settles to a walkable one, including dead centres and far away.
        for raw in [
            Position { x: 0, z: 0 },
            Position {
                x: 16_000,
                z: 16_000,
            },
            Position {
                x: i32::MAX,
                z: i32::MIN,
            },
            Position {
                x: -16_100,
                z: -15_900,
            },
        ] {
            let at = settle(raw);
            assert!(!inside_a_column(at), "{raw:?} -> {at:?}");
            assert_eq!(settle(at), at, "settling is stable");
        }
    }

    #[test]
    fn spawns_apart_on_walkable_ground() {
        let points: Vec<_> = (0..MAX_HUB_PLAYERS as u32).map(spawn_point).collect();
        for (i, a) in points.iter().enumerate() {
            assert!(!inside_a_column(*a));
            for b in &points[i + 1..] {
                if a != b {
                    assert!(a.distance_squared(*b) >= i64::from(SPAWN_SPACING).pow(2));
                }
            }
        }
        // Every player of a full instance has a place of their own; then
        // the rows repeat rather than walking off the plaza.
        let distinct: std::collections::HashSet<_> = points.iter().map(|p| (p.x, p.z)).collect();
        assert_eq!(distinct.len(), MAX_HUB_PLAYERS);
        assert_eq!(spawn_point(0), spawn_point(MAX_HUB_PLAYERS as u32));
    }

    #[test]
    fn jumps_as_in_the_arena_and_keeps_its_momentum() {
        let start = Body::standing(spawn_point(0));
        // A standing jump rises, peaks and lands where it took off.
        let mut body = step_body(
            start,
            Intent {
                jump: true,
                ..Intent::default()
            },
        );
        assert!(body.airborne() && body.height > 0);
        let mut apex = 0;
        let mut ticks = 1;
        while body.airborne() {
            apex = apex.max(body.height);
            // Jumping again in the air does nothing.
            body = step_body(
                body,
                Intent {
                    jump: true,
                    ..Intent::default()
                },
            );
            ticks += 1;
            assert!(ticks < 20, "lands");
        }
        assert_eq!((body.at, body.height, body.vertical), (start.at, 0, 0));
        // The arena's arc: about 0.73 s and 1.32 m (220 + 200 + … + 20 mm).
        assert_eq!(ticks, 8);
        assert_eq!(apex, 1_320);
        // A running jump carries its take-off step and ignores steering.
        let east = Intent {
            movement: Axes {
                x: AXIS_LIMIT,
                z: 0,
            },
            facing: 0,
            jump: true,
        };
        let west = Intent {
            movement: Axes {
                x: -AXIS_LIMIT,
                z: 0,
            },
            ..Intent::default()
        };
        let mut body = step_body(start, east);
        assert_eq!(body.momentum.x, HUB_MOVE_PER_TICK);
        while body.airborne() {
            let before = body.at.x;
            body = step_body(body, west);
            assert!(body.at.x > before, "still flying east");
        }
        assert_eq!(body.momentum, STILL);
    }

    #[test]
    fn sees_players_within_forty_metres() {
        let at = Position { x: 0, z: 0 };
        assert!(in_interest(
            at,
            Position {
                x: INTEREST_RADIUS,
                z: 0
            }
        ));
        assert!(!in_interest(
            at,
            Position {
                x: INTEREST_RADIUS,
                z: 1
            }
        ));
        // Corner to corner is out of view.
        assert!(!in_interest(
            Position {
                x: -PLAZA_HALF,
                z: -PLAZA_HALF
            },
            Position {
                x: PLAZA_HALF,
                z: PLAZA_HALF
            }
        ));
    }
}
