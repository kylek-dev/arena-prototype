//! Map layout: the walkable plan the rules use, in integer millimetres.
//!
//! A layout is a rectangular floor, hexagonal pillars and starting rooms.
//! Each room is one straight rectangle running back from a floor edge, exactly
//! as wide as its opening, so it has no corners to hide in. Layouts are plain
//! data; parsing and presentation settings live in the `arena-map` crate.
//! No floating point, I/O or rendering.

use crate::arena::{BODY_RADIUS, Position};

/// Largest team the practice rules support; every team needs this many spawns.
pub const MAX_TEAM: usize = 3;
/// How far a room's walkable region reaches into the floor. More than one
/// tick of movement, so bodies cross between floor and room without ever
/// leaving both.
const ROOM_OVERLAP: i32 = 1_000;
/// Grid step for validation sampling (reachability and visibility).
const SAMPLE: i32 = 500;

/// Floor edges. North is −Z (the opposing side in practice), south is +Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Edge {
    North,
    South,
    West,
    East,
}
impl Edge {
    pub const ALL: [Edge; 4] = [Edge::North, Edge::South, Edge::West, Edge::East];
}

/// A hexagonal pillar: the hexagon inscribed in its `half_x` × `half_z` box,
/// with corners at ±`half_x` on its X axis and flat faces at ±`half_z`.
/// Bodies keep `BODY_RADIUS` from it, so its walkable outline is the hexagon
/// grown by a rounded margin: faces to slide along and corners to round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pillar {
    pub center: Position,
    pub half_x: i32,
    pub half_z: i32,
}

/// Closest point to `p` on segment `a`–`b`, rounded to the millimetre.
fn closest_on_segment(p: Position, a: Position, b: Position) -> Position {
    let (abx, abz) = (i64::from(b.x - a.x), i64::from(b.z - a.z));
    let along = i64::from(p.x - a.x) * abx + i64::from(p.z - a.z) * abz;
    let length = abx * abx + abz * abz;
    if along <= 0 || length == 0 {
        a
    } else if along >= length {
        b
    } else {
        Position {
            x: a.x + (abx * along / length) as i32,
            z: a.z + (abz * along / length) as i32,
        }
    }
}

impl Pillar {
    /// Corners in counter-clockwise order seen from above (+X toward +Z).
    pub fn corners(self) -> [Position; 6] {
        let Position { x, z } = self.center;
        let (hx, hz, mid) = (self.half_x, self.half_z, self.half_x / 2);
        [
            Position { x: x + hx, z },
            Position {
                x: x + mid,
                z: z + hz,
            },
            Position {
                x: x - mid,
                z: z + hz,
            },
            Position { x: x - hx, z },
            Position {
                x: x - mid,
                z: z - hz,
            },
            Position {
                x: x + mid,
                z: z - hz,
            },
        ]
    }

    fn edges(self) -> impl Iterator<Item = (Position, Position)> {
        let corners = self.corners();
        (0..6).map(move |i| (corners[i], corners[(i + 1) % 6]))
    }

    /// Inside the hexagon or on its outline.
    fn inside(self, p: Position) -> bool {
        self.edges().all(|(a, b)| {
            let cross = i64::from(b.x - a.x) * i64::from(p.z - a.z)
                - i64::from(b.z - a.z) * i64::from(p.x - a.x);
            cross >= 0
        })
    }

    /// Whether `point` is within `padding` of the hexagon (or inside it).
    pub fn contains(self, point: Position, padding: i32) -> bool {
        if self.inside(point) {
            return true;
        }
        let limit = i128::from(padding).pow(2);
        self.edges().any(|(a, b)| {
            // Exact squared distance to the segment, compared without division.
            let (abx, abz) = (i128::from(b.x - a.x), i128::from(b.z - a.z));
            let (apx, apz) = (i128::from(point.x - a.x), i128::from(point.z - a.z));
            let along = apx * abx + apz * abz;
            let length = abx * abx + abz * abz;
            if along <= 0 || length == 0 {
                apx * apx + apz * apz <= limit
            } else if along >= length {
                let (bpx, bpz) = (i128::from(point.x - b.x), i128::from(point.z - b.z));
                bpx * bpx + bpz * bpz <= limit
            } else {
                let cross = abx * apz - abz * apx;
                cross * cross <= limit * length
            }
        })
    }

    /// Moves `point` straight out to just beyond `padding` from the nearest
    /// part of the outline: off a face along its normal, around a corner
    /// radially. A body pressed into a face therefore keeps the part of its
    /// step that runs along the face, and rounds corners smoothly.
    pub(crate) fn push_out(self, point: Position, padding: i32) -> Position {
        let Some((nearest, (a, b))) = self
            .edges()
            .map(|(a, b)| (closest_on_segment(point, a, b), (a, b)))
            .min_by_key(|(c, _)| c.distance_squared(point))
        else {
            return point;
        };
        let away = if self.inside(point) || nearest == point {
            // From inside, leave through the nearest face.
            Position {
                x: b.z - a.z,
                z: a.x - b.x,
            }
        } else {
            Position {
                x: point.x - nearest.x,
                z: point.z - nearest.z,
            }
        };
        let length = (i64::from(away.x).pow(2) + i64::from(away.z).pow(2))
            .isqrt()
            .max(1);
        // A few extra millimetres absorb rounding here and when `slide`
        // trims the step back to its requested length, so the result is clear.
        let reach = i64::from(padding) + 4;
        Position {
            x: nearest.x + (i64::from(away.x) * reach / length) as i32,
            z: nearest.z + (i64::from(away.z) * reach / length) as i32,
        }
    }
}

/// A spawn point inside a room: `across` from the room's centre line (toward
/// +X for north/south rooms, +Z for west/east rooms) and `back` from the floor
/// edge into the room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spawn {
    pub across: i32,
    pub back: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Room {
    /// 0 = friendly (even actors), 1 = opposing (odd actors).
    pub team: u8,
    pub edge: Edge,
    /// Centre of the opening along its edge (X for north/south, Z for west/east).
    pub offset: i32,
    pub width: i32,
    /// From the floor edge to the back wall.
    pub depth: i32,
    pub spawns: Vec<Spawn>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub half_x: i32,
    pub half_z: i32,
    pub pillars: Vec<Pillar>,
    pub rooms: Vec<Room>,
}

/// Axis-aligned rectangle by corners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub min: Position,
    pub max: Position,
}
impl Rect {
    pub fn contains(self, p: Position) -> bool {
        (self.min.x..=self.max.x).contains(&p.x) && (self.min.z..=self.max.z).contains(&p.z)
    }
}

impl Layout {
    /// Half length of an edge.
    fn edge_half(&self, edge: Edge) -> i32 {
        match edge {
            Edge::North | Edge::South => self.half_x,
            Edge::West | Edge::East => self.half_z,
        }
    }

    /// Distance from the centre to an edge.
    fn edge_distance(&self, edge: Edge) -> i32 {
        match edge {
            Edge::North | Edge::South => self.half_z,
            Edge::West | Edge::East => self.half_x,
        }
    }

    /// World position of a point `along` an edge's axis and `out` beyond it
    /// (negative `out` is inside the floor).
    pub fn edge_point(&self, edge: Edge, along: i32, out: i32) -> Position {
        let beyond = self.edge_distance(edge) + out;
        match edge {
            Edge::North => Position {
                x: along,
                z: -beyond,
            },
            Edge::South => Position {
                x: along,
                z: beyond,
            },
            Edge::West => Position {
                x: -beyond,
                z: along,
            },
            Edge::East => Position {
                x: beyond,
                z: along,
            },
        }
    }

    /// Rectangle spanned by `along` in `[a0, a1]` and `out` in `[o0, o1]`.
    fn edge_rect(&self, edge: Edge, (a0, a1): (i32, i32), (o0, o1): (i32, i32)) -> Rect {
        let p = self.edge_point(edge, a0, o0);
        let q = self.edge_point(edge, a1, o1);
        Rect {
            min: Position {
                x: p.x.min(q.x),
                z: p.z.min(q.z),
            },
            max: Position {
                x: p.x.max(q.x),
                z: p.z.max(q.z),
            },
        }
    }

    /// The room's interior, from the floor edge to its back wall.
    pub fn room_rect(&self, room: &Room) -> Rect {
        let half = room.width / 2;
        self.edge_rect(
            room.edge,
            (room.offset - half, room.offset + half),
            (0, room.depth),
        )
    }

    pub fn spawn_position(&self, room: &Room, spawn: Spawn) -> Position {
        self.edge_point(room.edge, room.offset + spawn.across, spawn.back)
    }

    /// Spawn positions for a team, in room order.
    pub fn spawns(&self, team: u8) -> Vec<Position> {
        self.rooms
            .iter()
            .filter(|room| room.team == team)
            .flat_map(|room| room.spawns.iter().map(|s| self.spawn_position(room, *s)))
            .collect()
    }

    /// A walkable floor point just in front of a room's opening.
    pub fn lookout(&self, room: &Room) -> Position {
        self.edge_point(room.edge, room.offset, -ROOM_OVERLAP)
    }

    /// Walkable rectangles for a body centre, already shrunk by the body
    /// radius: the floor and each room, which reaches into the floor.
    pub fn regions(&self) -> Vec<Rect> {
        let r = BODY_RADIUS;
        let mut regions = vec![Rect {
            min: Position {
                x: -self.half_x + r,
                z: -self.half_z + r,
            },
            max: Position {
                x: self.half_x - r,
                z: self.half_z - r,
            },
        }];
        for room in &self.rooms {
            let half = room.width / 2;
            regions.push(self.edge_rect(
                room.edge,
                (room.offset - half + r, room.offset + half - r),
                (-ROOM_OVERLAP, room.depth - r),
            ));
        }
        regions
    }

    /// Whether a body centred at `p` fits in the walkable space, clear of pillars.
    pub fn walkable(&self, p: Position) -> bool {
        self.regions().iter().any(|region| region.contains(p)) && !self.blocked(p)
    }

    fn blocked(&self, p: Position) -> bool {
        self.pillars
            .iter()
            .any(|pillar| pillar.contains(p, BODY_RADIUS))
    }

    /// Moves one axis at a time; each axis goes as far toward its target as
    /// any region containing the body allows. A pillar the step runs into
    /// pushes the body back out to its surface, so it slides along faces and
    /// around corners. If that leaves the body wedged (against a wall or a
    /// second pillar), pillars veto each axis instead. The same footprint
    /// applies in the air: jumping cannot bypass cover.
    pub fn slide(&self, mut p: Position, delta: Position) -> Position {
        let regions = self.regions();
        let reach = |p: Position, target: i32, along_x: bool| {
            let coord = |q: Position| if along_x { q.x } else { q.z };
            let start = coord(p);
            regions
                .iter()
                .filter(|region| region.contains(p))
                .map(|region| target.clamp(coord(region.min), coord(region.max)))
                .fold(start, |best, limit| {
                    if (limit - start).abs() > (best - start).abs() {
                        limit
                    } else {
                        best
                    }
                })
        };
        let mut target = Position {
            x: reach(p, p.x + delta.x, true),
            z: p.z,
        };
        target.z = reach(target, target.z + delta.z, false);
        for pillar in &self.pillars {
            if pillar.contains(target, BODY_RADIUS) {
                target = pillar.push_out(target, BODY_RADIUS);
            }
        }
        // Sliding never outruns the requested step: trim millimetre rounding.
        let step = |q: Position| i64::from(q.x - p.x).pow(2) + i64::from(q.z - p.z).pow(2);
        let allowed = i64::from(delta.x).pow(2) + i64::from(delta.z).pow(2);
        if step(target) > allowed {
            let (have, want) = (step(target).isqrt() + 1, allowed.isqrt());
            target = Position {
                x: p.x + (i64::from(target.x - p.x) * want / have) as i32,
                z: p.z + (i64::from(target.z - p.z) * want / have) as i32,
            };
        }
        if self.walkable(target) {
            return target;
        }
        let next = Position {
            x: reach(p, p.x + delta.x, true),
            z: p.z,
        };
        if !self.blocked(next) {
            p = next;
        }
        let next = Position {
            x: p.x,
            z: reach(p, p.z + delta.z, false),
        };
        if !self.blocked(next) {
            p = next;
        }
        p
    }

    /// Wall faces in plan: the floor boundary except room openings, and each
    /// room's two sides and back.
    pub fn walls(&self) -> Vec<(Position, Position)> {
        let mut walls = Vec::new();
        for edge in Edge::ALL {
            let half = self.edge_half(edge);
            let mut openings: Vec<(i32, i32)> = self
                .rooms
                .iter()
                .filter(|room| room.edge == edge)
                .map(|room| (room.offset - room.width / 2, room.offset + room.width / 2))
                .collect();
            openings.sort();
            let mut start = -half;
            for (a, b) in openings.into_iter().chain([(half, half)]) {
                if a > start {
                    walls.push((self.edge_point(edge, start, 0), self.edge_point(edge, a, 0)));
                }
                start = start.max(b);
            }
        }
        for room in &self.rooms {
            let (a, b) = (room.offset - room.width / 2, room.offset + room.width / 2);
            let corner = |along, out| self.edge_point(room.edge, along, out);
            walls.push((corner(a, 0), corner(a, room.depth)));
            walls.push((corner(b, 0), corner(b, room.depth)));
            walls.push((corner(a, room.depth), corner(b, room.depth)));
        }
        walls
    }

    /// Pillars and walls block sight; segments touching an edge are blocked too.
    pub fn line_of_sight(&self, start: Position, end: Position) -> bool {
        if self
            .walls()
            .iter()
            .any(|&(a, b)| segments_intersect(start, end, a, b))
        {
            return false;
        }
        !self.pillars.iter().any(|pillar| {
            if pillar.contains(start, 0) || pillar.contains(end, 0) {
                return true;
            }
            pillar
                .edges()
                .any(|(a, b)| segments_intersect(start, end, a, b))
        })
    }

    /// Checks the layout is playable. Messages are meant for map authors.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let r = BODY_RADIUS;
        let m = |mm: i32| format!("{:.1} m", f64::from(mm) / 1000.0);
        for (name, half) in [("Floor width", self.half_x), ("Floor depth", self.half_z)] {
            if !(10_000..=200_000).contains(&half) {
                errors.push(format!("{name} must be 20–400 m (is {}).", m(half * 2)));
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        for (i, pillar) in self.pillars.iter().enumerate() {
            let n = i + 1;
            if pillar.half_x < 500 || pillar.half_z < 500 {
                errors.push(format!("Pillar {n} must be at least 1 m on each side."));
            }
            if pillar.center.x.abs() + pillar.half_x > self.half_x
                || pillar.center.z.abs() + pillar.half_z > self.half_z
            {
                errors.push(format!("Pillar {n} must lie entirely on the floor."));
            }
        }
        for (i, room) in self.rooms.iter().enumerate() {
            let n = i + 1;
            if room.team > 1 {
                errors.push(format!("Room {n} must belong to team 1 or 2."));
            }
            if room.width < 2 * r + 600 {
                errors.push(format!(
                    "Room {n} must be at least {} wide.",
                    m(2 * r + 600)
                ));
            }
            if !(3_000..=60_000).contains(&room.depth) {
                errors.push(format!(
                    "Room {n} must be 3–60 m deep (is {}).",
                    m(room.depth)
                ));
            }
            if room.offset.abs() + room.width / 2 > self.edge_half(room.edge) - 1_000 {
                errors.push(format!(
                    "Room {n}'s opening must stay at least 1 m from the floor's corners."
                ));
            }
            for (j, other) in self.rooms.iter().enumerate().skip(i + 1) {
                if other.edge == room.edge
                    && (room.offset - other.offset).abs() < (room.width + other.width) / 2 + 1_000
                {
                    errors.push(format!(
                        "Rooms {n} and {} overlap or are less than 1 m apart.",
                        j + 1
                    ));
                }
            }
            for (k, spawn) in room.spawns.iter().enumerate() {
                if spawn.across.abs() > room.width / 2 - r
                    || !(r..=room.depth - r).contains(&spawn.back)
                {
                    errors.push(format!("Room {n} spawn {} must be inside the room.", k + 1));
                }
            }
            let lookout = self.lookout(room);
            if self.blocked(lookout) {
                errors.push(format!("A pillar blocks room {n}'s opening."));
            }
        }
        for team in 0..2u8 {
            let count = self.spawns(team).len();
            if count < MAX_TEAM {
                errors.push(format!(
                    "Team {} needs at least {MAX_TEAM} spawns in its rooms (has {count}).",
                    team + 1
                ));
            }
        }
        let all: Vec<_> = (0..2u8).flat_map(|team| self.spawns(team)).collect();
        for (i, a) in all.iter().enumerate() {
            if all[i + 1..]
                .iter()
                .any(|b| a.distance_squared(*b) < i64::from(2 * r).pow(2))
            {
                errors.push("Spawns must be at least 0.9 m apart.".into());
                break;
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        // Every spawn must reach every other through walkable space.
        let reachable = self.reachable_from(all[0]);
        if all.iter().any(|p| !reachable(*p)) {
            errors
                .push("Every spawn must be able to reach the others; pillars seal one off.".into());
        }
        // No hiding corners: all of a room is visible from just outside it.
        for (i, room) in self.rooms.iter().enumerate() {
            let lookout = self.lookout(room);
            let hidden = self
                .samples(self.room_rect(room))
                .into_iter()
                .any(|p| self.walkable(p) && !self.line_of_sight(lookout, p));
            if hidden {
                errors.push(format!("Room {} has spots hidden from its opening.", i + 1));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn samples(&self, rect: Rect) -> Vec<Position> {
        let mut points = Vec::new();
        let mut x = rect.min.x;
        while x <= rect.max.x {
            let mut z = rect.min.z;
            while z <= rect.max.z {
                points.push(Position { x, z });
                z += SAMPLE;
            }
            x += SAMPLE;
        }
        points
    }

    /// Flood fill over a grid of walkable sample points; returns a membership
    /// test that snaps a position to its nearest grid point.
    fn reachable_from(&self, start: Position) -> impl Fn(Position) -> bool {
        let reach = self
            .edge_distance(Edge::North)
            .max(self.edge_distance(Edge::West))
            + self.rooms.iter().map(|room| room.depth).max().unwrap_or(0);
        let cells = reach / SAMPLE + 1;
        let size = (2 * cells + 1) as usize;
        let index = move |p: Position| {
            let i = (p.x + SAMPLE / 2).div_euclid(SAMPLE) + cells;
            let j = (p.z + SAMPLE / 2).div_euclid(SAMPLE) + cells;
            ((0..size as i32).contains(&i) && (0..size as i32).contains(&j))
                .then_some((i as usize, j as usize))
        };
        let point = move |i: usize, j: usize| Position {
            x: (i as i32 - cells) * SAMPLE,
            z: (j as i32 - cells) * SAMPLE,
        };
        let mut seen = vec![false; size * size];
        // Spawns sit off-grid; start from the walkable grid point nearest it.
        let mut stack: Vec<(usize, usize)> = index(start)
            .into_iter()
            .chain(self.near_cells(start, &index))
            .filter(|&(i, j)| self.walkable(point(i, j)))
            .take(1)
            .collect();
        while let Some((i, j)) = stack.pop() {
            if std::mem::replace(&mut seen[i * size + j], true) {
                continue;
            }
            for (di, dj) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (ni, nj) = (i as i32 + di, j as i32 + dj);
                if (0..size as i32).contains(&ni)
                    && (0..size as i32).contains(&nj)
                    && !seen[ni as usize * size + nj as usize]
                    && self.walkable(point(ni as usize, nj as usize))
                {
                    stack.push((ni as usize, nj as usize));
                }
            }
        }
        move |p: Position| {
            index(p).into_iter().any(|(i, j)| {
                [(0i32, 0i32), (1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(di, dj)| {
                        let (ni, nj) = (i as i32 + di, j as i32 + dj);
                        (0..size as i32).contains(&ni)
                            && (0..size as i32).contains(&nj)
                            && seen[ni as usize * size + nj as usize]
                    })
            })
        }
    }

    fn near_cells(
        &self,
        p: Position,
        index: &impl Fn(Position) -> Option<(usize, usize)>,
    ) -> Vec<(usize, usize)> {
        [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .filter_map(|(dx, dz)| {
                index(Position {
                    x: p.x + dx * SAMPLE,
                    z: p.z + dz * SAMPLE,
                })
            })
            .collect()
    }
}

pub(crate) fn segments_intersect(a: Position, b: Position, c: Position, d: Position) -> bool {
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

/// Synthetic layout for rules tests: the shape of the default colosseum.
#[cfg(test)]
pub(crate) fn fixture() -> Layout {
    let pillar = |x, z| Pillar {
        center: Position { x, z },
        half_x: 4_500,
        half_z: 4_500,
    };
    let room = |team, edge| Room {
        team,
        edge,
        offset: 0,
        width: 6_000,
        depth: 11_000,
        spawns: vec![
            Spawn {
                across: 0,
                back: 5_000,
            },
            Spawn {
                across: -2_000,
                back: 5_000,
            },
            Spawn {
                across: 2_000,
                back: 5_000,
            },
        ],
    };
    Layout {
        half_x: 54_000,
        half_z: 42_000,
        pillars: vec![
            pillar(-21_000, -15_000),
            pillar(21_000, -15_000),
            pillar(-21_000, 15_000),
            pillar(21_000, 15_000),
        ],
        rooms: vec![room(0, Edge::South), room(1, Edge::North)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_is_valid_and_rooms_have_no_corners() {
        let layout = fixture();
        assert_eq!(layout.validate(), Ok(()));
        for room in &layout.rooms {
            // Straight back: the interior is exactly the opening's width.
            let rect = layout.room_rect(room);
            let span = match room.edge {
                Edge::North | Edge::South => rect.max.x - rect.min.x,
                Edge::West | Edge::East => rect.max.z - rect.min.z,
            };
            assert_eq!(span, room.width);
        }
    }

    #[test]
    fn invalid_layouts_explain_what_to_fix() {
        let message = |edit: fn(&mut Layout)| {
            let mut layout = fixture();
            edit(&mut layout);
            layout.validate().unwrap_err().join(" ")
        };
        assert!(message(|l| l.half_x = 5_000).contains("Floor width"));
        assert!(message(|l| l.pillars[0].center.x = -52_000).contains("entirely on the floor"));
        assert!(message(|l| l.rooms[0].width = 1_000).contains("at least"));
        assert!(message(|l| l.rooms[0].offset = 50_500).contains("corners"));
        assert!(message(|l| l.rooms[1].spawns.truncate(2)).contains("Team 2 needs"));
        assert!(message(|l| l.rooms[0].spawns[1].across = 0).contains("0.9 m apart"));
        assert!(message(|l| l.rooms[0].spawns[0].back = 20_000).contains("inside the room"));
        assert!(
            message(|l| l.pillars.push(Pillar {
                center: Position { x: 0, z: 40_000 },
                half_x: 4_000,
                half_z: 1_500,
            }))
            .contains("blocks room 1")
        );
        // A wall-to-wall pillar seals the opposing room's half off.
        assert!(
            message(|l| l.pillars.push(Pillar {
                center: Position { x: 0, z: 0 },
                half_x: 54_000,
                half_z: 1_000,
            }))
            .contains("reach the others")
        );
        assert!(
            message(|l| {
                let mut room = l.rooms[0].clone();
                room.offset = 4_000;
                l.rooms.push(room);
            })
            .contains("overlap")
        );
    }

    #[test]
    fn rooms_open_only_through_their_own_width_and_hide_nothing() {
        let layout = fixture();
        let south = &layout.rooms[0];
        let lookout = layout.lookout(south);
        for p in layout.samples(layout.room_rect(south)) {
            if layout.walkable(p) {
                assert!(layout.line_of_sight(lookout, p), "{p:?}");
            }
        }
        // Beside the opening the pit wall blocks sight into the room.
        assert!(!layout.line_of_sight(
            Position {
                x: 2_000,
                z: 46_000
            },
            Position {
                x: 8_000,
                z: 39_000
            }
        ));
        // Rooms on any edge work the same way.
        let mut east = fixture();
        east.rooms[0].edge = Edge::East;
        assert_eq!(east.validate(), Ok(()));
        let spawn = east.spawns(0)[0];
        assert!(spawn.x > east.half_x && east.walkable(spawn));
    }
}
