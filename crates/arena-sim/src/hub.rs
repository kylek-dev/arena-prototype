//! The hub's place ([ADR 0057](../../../docs/decisions/0057-hub-on-the-world-server.md)):
//! the graybox plaza a hub world (`Arena::hub`) runs on, where its training
//! enemies stand, where arrivals appear, and whom a player sees. The rules
//! are the arena's own; this is only the map. Integer millimetres, no
//! clocks, I/O or rendering.

use crate::arena::{BODY_RADIUS, HALF_TURN, Position};
use crate::map::{Layout, Pillar};

/// The plaza is a square floor `2 × PLAZA_HALF` on a side (80 m).
pub const PLAZA_HALF: i32 = 40_000;
/// Players see the others within this distance (40 m). On the 80 m
/// graybox plaza this filters little; it bounds the cost of larger worlds.
pub const INTEREST_RADIUS: i32 = 40_000;
/// Most players one hub world is meant to hold once interest filtering and
/// deltas carry it (ADR 0057's last step); arrival places are laid out for
/// this many. Today's cap is `arena::HUB_PLAYER_CAP`.
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

/// The plaza as an arena map, for hub worlds on the world server (ADR 0057):
/// the same square floor, each column a regular hexagonal pillar inscribed
/// in the round column (corners on its circle, so it stands inside the
/// column and nobody is pushed out of a place the door's hub allowed), and
/// no rooms.
pub fn plaza() -> Layout {
    Layout {
        half_x: PLAZA_HALF,
        half_z: PLAZA_HALF,
        pillars: COLUMNS
            .iter()
            .map(|column| Pillar {
                center: column.center,
                half_x: column.radius,
                // A regular hexagon's flat faces: radius × √3 / 2.
                half_z: column.radius * 866 / 1000,
            })
            .collect(),
        rooms: Vec::new(),
    }
}

/// A digest of the plaza's layout (its floor and pillars), FNV-1a over
/// their little-endian numbers: `Welcome.map` for hub worlds, so a client
/// with another plaza notices.
pub fn plaza_digest() -> u64 {
    let layout = plaza();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut write = |value: i32| {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    write(layout.half_x);
    write(layout.half_z);
    for pillar in &layout.pillars {
        for value in [
            pillar.center.x,
            pillar.center.z,
            pillar.half_x,
            pillar.half_z,
        ] {
            write(value);
        }
    }
    hash
}

/// Where a hub world's training enemies stand, and their facing: a row
/// north of the fountain, facing the arrivals in the south.
pub const TRAINING_POSTS: [(Position, u16); 4] = [
    (
        Position {
            x: -9_000,
            z: -24_000,
        },
        HALF_TURN,
    ),
    (
        Position {
            x: -3_000,
            z: -24_000,
        },
        HALF_TURN,
    ),
    (
        Position {
            x: 3_000,
            z: -24_000,
        },
        HALF_TURN,
    ),
    (
        Position {
            x: 9_000,
            z: -24_000,
        },
        HALF_TURN,
    ),
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

    fn inside_a_column(at: Position) -> bool {
        COLUMNS.iter().any(|column| {
            at.distance_squared(column.center) < i64::from(column.radius + BODY_RADIUS).pow(2)
        })
    }

    #[test]
    fn keeps_players_inside_the_plaza_and_out_of_columns() {
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
