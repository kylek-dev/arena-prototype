//! Presentation-only arena venue, generated entirely from the active map file:
//! the floor, a pit wall with a parapet, tiered stands with benches, a
//! concourse, the outer colosseum wall with doors, banners and rooftop
//! pennants, the starting rooms and the pillars. Rules never read any of
//! this; they use the map's `Layout`.
//!
//! All boxes come from one pure function (`boxes`) so spawning, rebuilding
//! after an edit and the camera tests all see exactly the same geometry.
//! Starting rooms are cut out of the walls and stands by box subtraction, so a
//! room can sit on any edge at any size the map allows.
use crate::{camera, heraldry, palette, textures};
use arena_map::{Ground, MapFile, Venue};
use arena_sim::map::Edge;
use bevy::prelude::*;

const PILLAR_CROWN: f32 = 1.6;
/// Pillar bands repeat at this spacing up the shaft.
const PILLAR_BAND: f32 = 12.0;
const PARAPET: f32 = 0.9;
const CORNICE_DEPTH: f32 = 9.0;
const CORNICE_RISE: f32 = 1.4;
const BAY: f32 = 12.0;

/// Marks every entity the venue spawns, so an edited map can replace it.
#[derive(Component)]
pub(crate) struct VenueEntity;

/// What a venue box is made of; each surface maps to one tinted material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Surface {
    Floor,
    /// Starting-room floors: always stone, whatever the map's ground.
    RoomFloor,
    Sand,
    Snow,
    Grass,
    Base,
    PitWall,
    Trim,
    Stands,
    Bench,
    OuterWall,
    Gold,
    Door,
    Gate,
    PillarStone,
    /// Bands and crowns: the pillar's light trim.
    PillarEdge,
    /// The plinth course between the floor and the shaft.
    PillarBase,
}

impl Surface {
    /// The arena floor's surface for a map's ground choice.
    pub(crate) fn ground(ground: Ground) -> Self {
        match ground {
            Ground::Stone => Surface::Floor,
            Ground::Sand => Surface::Sand,
            Ground::Snow => Surface::Snow,
            Ground::Grass => Surface::Grass,
        }
    }

    const ALL: [Surface; 17] = [
        Surface::Floor,
        Surface::RoomFloor,
        Surface::Sand,
        Surface::Snow,
        Surface::Grass,
        Surface::Base,
        Surface::PitWall,
        Surface::Trim,
        Surface::Stands,
        Surface::Bench,
        Surface::OuterWall,
        Surface::Gold,
        Surface::Door,
        Surface::Gate,
        Surface::PillarStone,
        Surface::PillarEdge,
        Surface::PillarBase,
    ];

    /// Texture and metres per repeat. Every venue surface is textured.
    fn texture(self) -> (&'static str, f32) {
        match self {
            Surface::Floor | Surface::RoomFloor | Surface::Base | Surface::Stands => {
                (textures::FLOOR_PATH, textures::FLOOR_TILE)
            }
            Surface::Sand => (textures::SAND_PATH, 6.0),
            Surface::Snow => (textures::SNOW_PATH, 7.0),
            Surface::Grass => (textures::GRASS_PATH, 3.0),
            Surface::PitWall | Surface::Trim => (textures::BRICKS_PATH, textures::BRICKS_TILE),
            // Pillars are speckled granite, not the walls' bricks, so cover
            // reads against the wall behind it by pattern, not only tint.
            Surface::PillarStone | Surface::PillarEdge | Surface::PillarBase => {
                (textures::GRANITE_PATH, textures::GRANITE_TILE)
            }
            Surface::OuterWall => (textures::STONE_WALL_PATH, 6.0),
            Surface::Bench | Surface::Door => (textures::WOOD_PATH, 1.6),
            Surface::Gate => (textures::GRATE_PATH, 2.0),
            Surface::Gold => (textures::PANELS_PATH, 1.0),
        }
    }

    /// Flat tint shown until the texture loads, and multiplied with it after.
    fn tint(self) -> Color {
        palette::color(match self {
            Surface::Floor | Surface::RoomFloor => palette::STONE_3,
            // Ground textures keep their painted colour.
            Surface::Sand => palette::SAND_8,
            Surface::Snow => palette::SKY_8,
            Surface::Grass => palette::LEAF_7,
            Surface::Base => palette::NIGHT_2,
            Surface::PitWall => palette::STONE_4,
            Surface::Trim => palette::STONE_4,
            Surface::Stands => palette::SAND_6,
            Surface::Bench => palette::WOOD_4,
            Surface::OuterWall => palette::SAND_7,
            Surface::Gold => palette::GOLD_4,
            Surface::Door => palette::WOOD_3,
            Surface::Gate => palette::STONE_2,
            // Stone surfaces share one neutral texture value, so these tints
            // are what shows. Floor and walls sit together without a seam;
            // pillars are light granite with lighter bands and crowns
            // so cover stands out against the walls behind it, and
            // their plinths step between floor and shaft.
            Surface::PillarStone => palette::STONE_6,
            Surface::PillarEdge => palette::STONE_7,
            Surface::PillarBase => palette::STONE_5,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Collision {
    /// Decoration; the camera passes through.
    None,
    /// Enclosing structure: the camera always pulls in before it.
    Wall,
    /// Floors: also enclose, and keep low views above the map.
    Floor,
    /// Free-standing pillars may hide the player from a distant camera.
    Pillar,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Piece {
    pub(crate) transform: Transform,
    pub(crate) surface: Surface,
    collision: Collision,
}
impl Piece {
    #[cfg(test)]
    pub(crate) fn solid(&self) -> bool {
        self.collision != Collision::None
    }
    /// Pillar pieces are hexagons inscribed in their box, like the rules'
    /// pillar footprint; everything else is the box itself.
    fn hexagonal(&self) -> bool {
        matches!(
            self.surface,
            Surface::PillarStone | Surface::PillarEdge | Surface::PillarBase
        )
    }
    fn obstacle(&self) -> Option<(Transform, camera::Obstacle)> {
        match self.collision {
            Collision::None => None,
            Collision::Wall => Some(camera::solid_wall(self.transform)),
            Collision::Floor => Some(camera::solid_floor(self.transform)),
            Collision::Pillar if self.hexagonal() => Some(camera::solid_pillar(self.transform)),
            Collision::Pillar => Some(camera::solid_box(self.transform)),
        }
    }
}

/// The map's floor and venue in metres, with helpers for edge-relative boxes.
struct Plan<'a> {
    hx: f32,
    hz: f32,
    v: &'a Venue,
}

/// One side of the rectangle at some offset beyond the floor edge.
struct Side {
    origin: Vec3,
    along: Vec3,
    inward: Vec3,
    half: f32,
}
impl Side {
    /// Box size given width along the side, depth toward the arena and height.
    fn size(&self, width: f32, depth: f32, height: f32) -> Vec3 {
        if self.along == Vec3::X {
            Vec3::new(width, height, depth)
        } else {
            Vec3::new(depth, height, width)
        }
    }
    fn at(&self, s: f32, out: f32, y: f32) -> Vec3 {
        self.origin + self.along * s + self.inward * out + Vec3::Y * y
    }
    /// Rotation turning local +Z toward the arena.
    fn facing(&self) -> Quat {
        Quat::from_rotation_arc(Vec3::Z, self.inward)
    }
    /// Evenly spaced bay boundaries along the side.
    fn bays(&self) -> Vec<f32> {
        let count = (2.0 * self.half / BAY).floor() as i32;
        let start = -(count as f32) * BAY * 0.5;
        (0..=count).map(|i| start + i as f32 * BAY).collect()
    }
}

fn mm(value: i32) -> f32 {
    value as f32 / 1000.0
}

impl<'a> Plan<'a> {
    fn new(map: &'a MapFile) -> Self {
        Self {
            hx: mm(map.layout.half_x),
            hz: mm(map.layout.half_z),
            v: &map.venue,
        }
    }

    fn edge_distance(&self, edge: Edge) -> f32 {
        match edge {
            Edge::North | Edge::South => self.hz,
            Edge::West | Edge::East => self.hx,
        }
    }

    /// World point `along` an edge's axis, `out` beyond it, at height `y`.
    fn edge_point(&self, edge: Edge, along: f32, out: f32, y: f32) -> Vec3 {
        let beyond = self.edge_distance(edge) + out;
        match edge {
            Edge::North => Vec3::new(along, y, -beyond),
            Edge::South => Vec3::new(along, y, beyond),
            Edge::West => Vec3::new(-beyond, y, along),
            Edge::East => Vec3::new(beyond, y, along),
        }
    }

    /// Box spanning `along`, `out` and `y` ranges relative to an edge.
    fn edge_box(&self, edge: Edge, along: (f32, f32), out: (f32, f32), y: (f32, f32)) -> Transform {
        let p = self.edge_point(edge, along.0, out.0, y.0);
        let q = self.edge_point(edge, along.1, out.1, y.1);
        Transform::from_translation((p + q) * 0.5).with_scale((q - p).abs())
    }

    /// Four boxes forming a closed band between `inner` and `outer` metres
    /// beyond the floor edge. North/south boxes own the corners.
    fn ring(&self, inner: f32, outer: f32, bottom: f32, top: f32) -> [Transform; 4] {
        let (hx, hz) = (self.hx, self.hz);
        let mid = (inner + outer) * 0.5;
        let depth = outer - inner;
        let y = (bottom + top) * 0.5;
        let height = top - bottom;
        let ns = Vec3::new(2.0 * (hx + outer), height, depth);
        let ew = Vec3::new(depth, height, 2.0 * (hz + inner));
        [
            Transform::from_xyz(0.0, y, -(hz + mid)).with_scale(ns),
            Transform::from_xyz(0.0, y, hz + mid).with_scale(ns),
            Transform::from_xyz(-(hx + mid), y, 0.0).with_scale(ew),
            Transform::from_xyz(hx + mid, y, 0.0).with_scale(ew),
        ]
    }

    fn sides(&self, offset: f32) -> [Side; 4] {
        let (hx, hz) = (self.hx, self.hz);
        [
            Side {
                origin: Vec3::new(0.0, 0.0, -(hz + offset)),
                along: Vec3::X,
                inward: Vec3::Z,
                half: hx + offset,
            },
            Side {
                origin: Vec3::new(0.0, 0.0, hz + offset),
                along: Vec3::X,
                inward: Vec3::NEG_Z,
                half: hx + offset,
            },
            Side {
                origin: Vec3::new(-(hx + offset), 0.0, 0.0),
                along: Vec3::Z,
                inward: Vec3::X,
                half: hz,
            },
            Side {
                origin: Vec3::new(hx + offset, 0.0, 0.0),
                along: Vec3::Z,
                inward: Vec3::NEG_X,
                half: hz,
            },
        ]
    }

    fn tier_top(&self, tier: u32) -> f32 {
        self.v.pit_wall_height + (tier + 1) as f32 * self.v.tier_rise
    }
}

fn bounds(t: &Transform) -> (Vec3, Vec3) {
    let half = t.scale.abs() * 0.5;
    (t.translation - half, t.translation + half)
}

fn from_bounds(min: Vec3, max: Vec3) -> Transform {
    Transform::from_translation((min + max) * 0.5).with_scale(max - min)
}

/// Axis-aligned box difference: what is left of `t` after removing `hole`,
/// as up to six boxes. World-anchored UVs keep the pieces' textures seamless.
fn subtract(t: Transform, hole: (Vec3, Vec3)) -> Vec<Transform> {
    let (mut min, mut max) = bounds(&t);
    let (hmin, hmax) = hole;
    if (0..3).any(|a| hmin[a] >= max[a] || hmax[a] <= min[a]) {
        return vec![t];
    }
    let mut pieces = Vec::new();
    for axis in 0..3 {
        if hmin[axis] > min[axis] {
            let mut top = max;
            top[axis] = hmin[axis];
            pieces.push(from_bounds(min, top));
            min[axis] = hmin[axis];
        }
        if hmax[axis] < max[axis] {
            let mut bottom = min;
            bottom[axis] = hmax[axis];
            pieces.push(from_bounds(bottom, max));
            max[axis] = hmax[axis];
        }
    }
    pieces
}

/// Every static venue box for a map, including floor, rooms and pillars.
pub(crate) fn boxes(map: &MapFile) -> Vec<Piece> {
    let plan = Plan::new(map);
    let v = plan.v;
    let mut pieces = Vec::new();
    let mut add = |transform: Transform, surface: Surface, collision: Collision| {
        pieces.push(Piece {
            transform,
            surface,
            collision,
        });
    };
    let at = |center: Vec3, size: Vec3| Transform::from_translation(center).with_scale(size);
    // The pit wall encloses the floor; its parapet frames the front tier.
    for t in plan.ring(0.0, v.pit_wall_thickness, 0.0, v.pit_wall_height) {
        add(t, Surface::PitWall, Collision::Wall);
    }
    let top = v.pit_wall_height;
    for t in plan.ring(-0.1, 0.4, top, top + PARAPET) {
        add(t, Surface::Trim, Collision::Wall);
    }
    for t in plan.ring(-0.15, 0.45, top + PARAPET, top + PARAPET + 0.15) {
        add(t, Surface::Gold, Collision::None);
    }
    // Stepped stands: each tier is one block from the ground, and its wooden
    // bench sits within the camera's clearance of the tread.
    for tier in 0..v.tiers {
        let inner = v.pit_wall_thickness + tier as f32 * v.tier_tread;
        let top = plan.tier_top(tier);
        for t in plan.ring(inner, inner + v.tier_tread, 0.0, top) {
            add(t, Surface::Stands, Collision::Wall);
        }
        for t in plan.ring(inner + 0.2, inner + 0.8, top, top + 0.28) {
            add(t, Surface::Bench, Collision::None);
        }
    }
    let concourse = v.concourse_height();
    for t in plan.ring(v.stands_depth(), v.wall_inner(), 0.0, concourse) {
        add(t, Surface::Stands, Collision::Wall);
    }
    for t in plan.ring(v.wall_inner(), v.footprint(), 0.0, v.outer_wall_height) {
        add(t, Surface::OuterWall, Collision::Wall);
    }
    // An overhanging cornice reads as the roof edge against the sky.
    let lip = v.footprint() - CORNICE_DEPTH;
    let cornice_top = v.outer_wall_height + CORNICE_RISE;
    for t in plan.ring(lip, v.footprint() + 0.6, v.outer_wall_height, cornice_top) {
        add(t, Surface::Trim, Collision::Wall);
    }
    for t in plan.ring(
        lip - 0.3,
        lip + 0.3,
        v.outer_wall_height - 0.5,
        cornice_top - 1.0,
    ) {
        add(t, Surface::Gold, Collision::None);
    }
    // Facade: pilasters framing bays, each bay with a framed wooden door at
    // concourse level. Banners hang above the doors.
    for side in plan.sides(v.wall_inner()) {
        let height = v.outer_wall_height - concourse;
        let bays = side.bays();
        for (i, s) in bays.iter().copied().enumerate() {
            add(
                at(
                    side.at(s, 0.5, concourse + height * 0.5),
                    side.size(1.6, 1.0, height),
                ),
                Surface::Trim,
                Collision::Wall,
            );
            add(
                at(side.at(s, 0.6, concourse + 0.5), side.size(2.2, 1.2, 1.0)),
                Surface::Trim,
                Collision::Wall,
            );
            if i + 1 == bays.len() {
                continue;
            }
            let mid = s + BAY * 0.5;
            add(
                at(
                    side.at(mid, 0.06, concourse + 1.8),
                    side.size(3.0, 0.12, 3.6),
                ),
                Surface::Door,
                Collision::None,
            );
            for jamb in [-1.8, 1.8] {
                add(
                    at(
                        side.at(mid + jamb, 0.2, concourse + 2.0),
                        side.size(0.6, 0.4, 4.0),
                    ),
                    Surface::Trim,
                    Collision::None,
                );
            }
            add(
                at(
                    side.at(mid, 0.22, concourse + 4.25),
                    side.size(4.4, 0.44, 0.6),
                ),
                Surface::Trim,
                Collision::None,
            );
            add(
                at(
                    side.at(mid, 0.3, concourse + 4.25),
                    side.size(0.7, 0.5, 0.8),
                ),
                Surface::Gold,
                Collision::None,
            );
        }
    }
    // Cut each starting room, a straight tunnel as wide as its opening, out of
    // everything built so far.
    let holes: Vec<_> = map
        .layout
        .rooms
        .iter()
        .zip(&map.rooms)
        .map(|(room, look)| {
            let half = mm(room.width) * 0.5;
            let along = (mm(room.offset) - half, mm(room.offset) + half);
            bounds(&plan.edge_box(
                room.edge,
                along,
                (-0.05, mm(room.depth)),
                (-0.05, look.ceiling),
            ))
        })
        .collect();
    let mut carved = Vec::new();
    for piece in pieces {
        let mut parts = vec![piece.transform];
        for hole in &holes {
            parts = parts.into_iter().flat_map(|t| subtract(t, *hole)).collect();
        }
        carved.extend(
            parts
                .into_iter()
                .map(|transform| Piece { transform, ..piece }),
        );
    }
    let mut pieces = carved;
    let mut add = |transform: Transform, surface: Surface, collision: Collision| {
        pieces.push(Piece {
            transform,
            surface,
            collision,
        });
    };
    // The floor and a base slab under the whole venue, so no void shows.
    let (hx, hz) = (plan.hx, plan.hz);
    add(
        Transform::from_xyz(0.0, -0.08, 0.0).with_scale(Vec3::new(hx * 2.0, 0.16, hz * 2.0)),
        Surface::ground(map.ground),
        Collision::Floor,
    );
    let base = v.footprint() * 2.0 + 2.0;
    add(
        Transform::from_xyz(0.0, -0.5, 0.0).with_scale(Vec3::new(
            hx * 2.0 + base,
            0.8,
            hz * 2.0 + base,
        )),
        Surface::Base,
        Collision::Floor,
    );
    for (room, look) in map.layout.rooms.iter().zip(&map.rooms) {
        let half = mm(room.width) * 0.5;
        let (a0, a1) = (mm(room.offset) - half, mm(room.offset) + half);
        let depth = mm(room.depth);
        let ceiling = look.ceiling;
        let b = |along, out, y| plan.edge_box(room.edge, along, out, y);
        // The arena's flagstone continues through the gate (same texture,
        // tint and world-aligned UVs), brick-faced side and back walls, a
        // plank ceiling on cross beams; facings on the solid structure, so
        // none collide.
        add(
            b((a0, a1), (0.0, depth), (-0.16, 0.0)),
            Surface::RoomFloor,
            Collision::None,
        );
        add(
            b((a0, a0 + 0.1), (0.0, depth), (0.0, ceiling)),
            Surface::PitWall,
            Collision::None,
        );
        add(
            b((a1 - 0.1, a1), (0.0, depth), (0.0, ceiling)),
            Surface::PitWall,
            Collision::None,
        );
        add(
            b((a0, a1), (depth - 0.1, depth), (0.0, ceiling)),
            Surface::PitWall,
            Collision::None,
        );
        add(
            b(
                (a0, a1),
                (v.pit_wall_thickness, depth),
                (ceiling - 0.08, ceiling),
            ),
            Surface::Door,
            Collision::None,
        );
        let inside = depth - v.pit_wall_thickness;
        let beams = (inside / 2.5).round().max(1.0) as i32;
        for i in 0..beams {
            let out = v.pit_wall_thickness + inside * (i as f32 + 0.5) / beams as f32;
            add(
                b((a0, a1), (out - 0.2, out + 0.2), (ceiling - 0.45, ceiling)),
                Surface::Bench,
                Collision::None,
            );
        }
        // The portcullis is raised: only its lower edge shows below the lintel.
        let mid = v.pit_wall_thickness * 0.5;
        add(
            b(
                (a0, a1),
                (mid - 0.08, mid + 0.08),
                (ceiling - 0.45, ceiling + 0.6),
            ),
            Surface::Gate,
            Collision::None,
        );
        // Gate front on the arena side: shallow stone jambs, gilded lintel.
        add(
            b((a0 - 0.8, a0), (-0.15, 0.0), (0.0, ceiling + 0.6)),
            Surface::Trim,
            Collision::None,
        );
        add(
            b((a1, a1 + 0.8), (-0.15, 0.0), (0.0, ceiling + 0.6)),
            Surface::Trim,
            Collision::None,
        );
        add(
            b((a0 - 0.8, a1 + 0.8), (-0.2, 0.0), (ceiling, ceiling + 0.7)),
            Surface::Gold,
            Collision::None,
        );
    }
    let crown = v.pillar_height + PILLAR_CROWN;
    for pillar in &map.layout.pillars {
        let c = Vec3::new(mm(pillar.center.x), 0.0, mm(pillar.center.z));
        let size = Vec3::new(mm(pillar.half_x) * 2.0, 0.0, mm(pillar.half_z) * 2.0);
        let grow = |by: f32| size + Vec3::new(by, 0.0, by);
        add(
            at(c + Vec3::Y * 0.12, grow(0.35) + Vec3::Y * 0.24),
            Surface::PillarBase,
            Collision::Pillar,
        );
        add(
            at(
                c + Vec3::Y * v.pillar_height * 0.5,
                size + Vec3::Y * v.pillar_height,
            ),
            Surface::PillarStone,
            Collision::Pillar,
        );
        // Banding gives the tall shafts a readable scale and rhythm.
        let mut y = PILLAR_BAND;
        while y < v.pillar_height - 4.0 {
            add(
                at(c + Vec3::Y * y, grow(0.3) + Vec3::Y * 0.6),
                Surface::PillarEdge,
                Collision::Pillar,
            );
            y += PILLAR_BAND;
        }
        add(
            at(
                c + Vec3::Y * (v.pillar_height + crown) * 0.5,
                grow(0.8) + Vec3::Y * PILLAR_CROWN,
            ),
            Surface::PillarEdge,
            Collision::Pillar,
        );
    }
    pieces
}

/// Height of a pillar including its crown, used by picking proxies.
pub(crate) fn pillar_top(venue: &Venue) -> f32 {
    venue.pillar_height + PILLAR_CROWN
}

/// Banner hanging points (top centre and facing) in each facade bay.
fn banner_slots(map: &MapFile) -> Vec<(Vec3, Quat)> {
    let plan = Plan::new(map);
    let mut slots = Vec::new();
    for side in plan.sides(plan.v.wall_inner()) {
        let bays = side.bays();
        for pair in bays.windows(2) {
            let mid = (pair[0] + pair[1]) * 0.5;
            slots.push((
                side.at(mid, 0.25, plan.v.outer_wall_height - 2.2),
                side.facing(),
            ));
        }
    }
    slots
}

/// Rooftop flagpole bases along the cornice lip, every other bay.
fn pole_slots(map: &MapFile) -> Vec<Vec3> {
    let plan = Plan::new(map);
    let lip = plan.v.footprint() - CORNICE_DEPTH + 1.2;
    let mut slots = Vec::new();
    for side in plan.sides(lip) {
        for (i, s) in side.bays().into_iter().enumerate() {
            if i % 2 == 0 {
                slots.push(side.at(s, 0.0, plan.v.outer_wall_height + CORNICE_RISE));
            }
        }
    }
    slots
}

/// Materials and heraldry shared by every rebuild, so textures load once.
#[derive(Resource)]
pub(crate) struct VenueAssets {
    surfaces: Vec<(Surface, Handle<StandardMaterial>)>,
    banners: Vec<Handle<StandardMaterial>>,
    pennants: Vec<Handle<StandardMaterial>>,
    banner: Handle<Mesh>,
    pennant_waves: Vec<Handle<Mesh>>,
    rod: Handle<Mesh>,
    finial: Handle<Mesh>,
    tassel: Handle<Mesh>,
    pole: Handle<Mesh>,
    tip: Handle<Mesh>,
}

impl VenueAssets {
    pub(crate) fn new(
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>,
    ) -> Self {
        let surfaces = Surface::ALL
            .iter()
            .map(|&surface| {
                let handle = materials.add(StandardMaterial {
                    base_color: surface.tint(),
                    perceptual_roughness: if surface == Surface::Gold { 0.6 } else { 0.92 },
                    ..default()
                });
                (surface, handle)
            })
            .collect();
        let mut cloth = |image: Image| {
            materials.add(StandardMaterial {
                base_color_texture: Some(images.add(image)),
                perceptual_roughness: 0.88,
                double_sided: true,
                cull_mode: None,
                ..default()
            })
        };
        let banners = heraldry::LIVERIES
            .iter()
            .map(|livery| cloth(heraldry::banner_image(livery)))
            .collect();
        let pennants = heraldry::LIVERIES
            .iter()
            .map(|livery| cloth(heraldry::pennant_image(livery)))
            .collect();
        Self {
            surfaces,
            banners,
            pennants,
            banner: meshes.add(heraldry::banner_mesh()),
            pennant_waves: (0..6)
                .map(|i| meshes.add(heraldry::pennant_mesh(i as f32 * 1.7)))
                .collect(),
            rod: meshes.add(Cylinder::new(0.09, heraldry::BANNER_WIDTH + 0.8)),
            finial: meshes.add(Sphere::new(0.2)),
            tassel: meshes.add(Cone {
                radius: 0.16,
                height: 0.6,
            }),
            pole: meshes.add(Cylinder::new(0.12, 7.0)),
            tip: meshes.add(Sphere::new(0.3)),
        }
    }

    fn material(&self, surface: Surface) -> Handle<StandardMaterial> {
        self.surfaces
            .iter()
            .find(|(s, _)| *s == surface)
            .map(|(_, handle)| handle.clone())
            .expect("every surface has a material")
    }

    /// Materials grouped by texture, for `textures::ArenaTextures`.
    pub(crate) fn texture_sets(&self) -> Vec<(&'static str, Vec<Handle<StandardMaterial>>)> {
        let mut sets: Vec<(&'static str, Vec<Handle<StandardMaterial>>)> = Vec::new();
        for (surface, handle) in &self.surfaces {
            let (path, _) = surface.texture();
            match sets.iter_mut().find(|(p, _)| *p == path) {
                Some((_, list)) => list.push(handle.clone()),
                None => sets.push((path, vec![handle.clone()])),
            }
        }
        sets
    }
}

/// Spawns the venue for a map. Every entity is tagged with `VenueEntity`.
pub(crate) fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    assets: &VenueAssets,
    map: &MapFile,
) {
    for piece in boxes(map) {
        let (_, tile) = piece.surface.texture();
        let mesh = if piece.hexagonal() {
            textures::tiled_hexagon(&piece.transform, tile)
        } else {
            textures::tiled_cube(&piece.transform, tile)
        };
        let mut entity = commands.spawn((
            VenueEntity,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(assets.material(piece.surface)),
        ));
        match piece.obstacle() {
            Some(collider) => entity.insert(collider),
            None => entity.insert(piece.transform),
        };
    }
    let gold = assets.material(Surface::Gold);
    for (i, (top, facing)) in banner_slots(map).into_iter().enumerate() {
        let banner = Transform::from_translation(top).with_rotation(facing);
        commands.spawn((
            VenueEntity,
            Mesh3d(assets.banner.clone()),
            MeshMaterial3d(assets.banners[i % assets.banners.len()].clone()),
            banner,
        ));
        let across = facing * Vec3::X;
        let hanger = top + facing * Vec3::Z * 0.2 - Vec3::Y * 0.35;
        commands.spawn((
            VenueEntity,
            Mesh3d(assets.rod.clone()),
            MeshMaterial3d(gold.clone()),
            Transform::from_translation(hanger)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, across)),
        ));
        for end in [-1.0, 1.0] {
            commands.spawn((
                VenueEntity,
                Mesh3d(assets.finial.clone()),
                MeshMaterial3d(gold.clone()),
                Transform::from_translation(
                    hanger + across * end * (heraldry::BANNER_WIDTH * 0.5 + 0.45),
                ),
            ));
        }
        for tip in heraldry::tail_tips() {
            commands.spawn((
                VenueEntity,
                Mesh3d(assets.tassel.clone()),
                MeshMaterial3d(gold.clone()),
                Transform::from_translation(banner.transform_point(tip) - Vec3::Y * 0.3)
                    .with_rotation(heraldry::hanging()),
            ));
        }
    }
    // One prevailing wind for every flag.
    let wind = Quat::from_rotation_y(0.35);
    for (i, base) in pole_slots(map).into_iter().enumerate() {
        commands.spawn((
            VenueEntity,
            Mesh3d(assets.pole.clone()),
            MeshMaterial3d(gold.clone()),
            Transform::from_translation(base + Vec3::Y * 3.5),
        ));
        commands.spawn((
            VenueEntity,
            Mesh3d(assets.tip.clone()),
            MeshMaterial3d(gold.clone()),
            Transform::from_translation(base + Vec3::Y * 7.2),
        ));
        commands.spawn((
            VenueEntity,
            Mesh3d(assets.pennant_waves[i % assets.pennant_waves.len()].clone()),
            MeshMaterial3d(assets.pennants[i % assets.pennants.len()].clone()),
            Transform::from_translation(base + Vec3::Y * 6.1).with_rotation(wind),
        ));
    }
}

/// Camera obstacles a map's venue contributes, for tests that drive the camera.
#[cfg(test)]
pub(crate) fn obstacles(map: &MapFile) -> Vec<camera::Obstacle> {
    boxes(map)
        .iter()
        .filter_map(|piece| piece.obstacle().map(|(_, obstacle)| obstacle))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colosseum() -> MapFile {
        arena_map::colosseum()
    }

    fn blocked_by(map: &MapFile) -> impl Fn(Vec3) -> bool {
        let solid: Vec<_> = boxes(map)
            .into_iter()
            .filter(|piece| piece.solid())
            .map(|piece| bounds(&piece.transform))
            .collect();
        move |p: Vec3| {
            solid
                .iter()
                .any(|(min, max)| p.cmpgt(*min).all() && p.cmplt(*max).all())
        }
    }

    #[test]
    fn subtraction_leaves_the_box_minus_the_hole_exactly() {
        let t = Transform::from_xyz(0.0, 1.0, 0.0).with_scale(Vec3::new(4.0, 2.0, 4.0));
        let hole = (Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.5, 3.0));
        let parts = subtract(t, hole);
        let volume: f32 = parts
            .iter()
            .map(|p| p.scale.x * p.scale.y * p.scale.z)
            .sum();
        // 32 m³ box minus the 2 × 1.5 × 3 m overlap.
        assert!((volume - (32.0 - 9.0)).abs() < 1e-4);
        for part in &parts {
            let (min, max) = bounds(part);
            let overlap = (0..3).all(|a| min[a] < hole.1[a] && max[a] > hole.0[a]);
            assert!(!overlap);
        }
        assert_eq!(subtract(t, (Vec3::splat(5.0), Vec3::splat(6.0))), vec![t]);
    }

    #[test]
    fn venue_stays_off_the_floor_and_pillars_tower_over_the_camera() {
        let map = colosseum();
        let (hx, hz) = (mm(map.layout.half_x), mm(map.layout.half_z));
        let near_pillar = |p: Vec3| {
            map.layout.pillars.iter().any(|pillar| {
                (p.x - mm(pillar.center.x)).abs() <= mm(pillar.half_x) + 0.5
                    && (p.z - mm(pillar.center.z)).abs() <= mm(pillar.half_z) + 0.5
            })
        };
        for piece in boxes(&map) {
            if piece.transform.translation.y < 0.0 {
                continue;
            }
            let (min, max) = bounds(&piece.transform);
            let over_floor = min.x < hx && max.x > -hx && min.z < hz && max.z > -hz;
            // Beyond pillars, only thin facings on the pit wall reach into play.
            if over_floor && min.y < map.venue.pit_wall_height {
                let facing = (min.z.abs().min(max.z.abs()) > hz - 0.25)
                    || (min.x.abs().min(max.x.abs()) > hx - 0.25);
                assert!(
                    near_pillar(piece.transform.translation) || facing,
                    "{:?} {min:?}..{max:?}",
                    piece.surface
                );
            }
        }
        assert!(map.venue.pillar_height > arena_map::HIGHEST_EYE + 10.0);
    }

    #[test]
    fn starting_rooms_are_straight_tall_enclosed_and_match_the_rules() {
        let map = colosseum();
        let blocked = blocked_by(&map);
        let plan = Plan::new(&map);
        for (room, look) in map.layout.rooms.iter().zip(&map.rooms) {
            assert!(look.ceiling >= 3.0 * arena_map::CHARACTER_HEIGHT);
            let half = mm(room.width) * 0.5;
            let (a0, a1) = (mm(room.offset) - half, mm(room.offset) + half);
            let depth = mm(room.depth);
            let p = |along, out, y| plan.edge_point(room.edge, along, out, y);
            // Open everywhere inside, from the opening to the back wall, up
            // to the ceiling: one straight tunnel with no wider pockets.
            for along in [a0 + 0.05, (a0 + a1) * 0.5, a1 - 0.05] {
                for out in [0.05, depth * 0.5, depth - 0.05] {
                    for y in [0.1, look.ceiling * 0.5, look.ceiling - 0.05] {
                        assert!(!blocked(p(along, out, y)), "{along} {out} {y}");
                    }
                }
            }
            // Solid just outside the sides, behind the back and above.
            for out in [0.3, depth * 0.5, depth - 0.3] {
                assert!(blocked(p(a0 - 0.1, out, 2.0)));
                assert!(blocked(p(a1 + 0.1, out, 2.0)));
            }
            assert!(blocked(p((a0 + a1) * 0.5, depth + 0.1, 2.0)));
            assert!(blocked(p((a0 + a1) * 0.5, depth * 0.5, look.ceiling + 0.1)));
            // The pit wall beside the opening is solid.
            assert!(blocked(p(
                a1 + 0.3,
                map.venue.pit_wall_thickness * 0.5,
                2.0
            )));
        }
        // Every spawn the rules use is inside an open room.
        for team in 0..2 {
            for spawn in map.layout.spawns(team) {
                assert!(map.layout.walkable(spawn));
                let world = Vec3::new(mm(spawn.x), 1.0, mm(spawn.z));
                assert!(!blocked(world), "{spawn:?}");
            }
        }
    }

    #[test]
    fn rooms_on_any_edge_are_carved_the_same_way() {
        let mut map = colosseum();
        map.layout.rooms[0].edge = Edge::East;
        map.layout.rooms[0].offset = 10_000;
        assert_eq!(map.layout.validate(), Ok(()));
        let blocked = blocked_by(&map);
        let spawn = map.layout.spawns(0)[0];
        assert!(!blocked(Vec3::new(mm(spawn.x), 1.0, mm(spawn.z))));
        // An east room runs along Z; 1 m past its side wall is solid.
        let room = &map.layout.rooms[0];
        let beyond = mm(room.offset) + mm(room.width) * 0.5 + 1.0;
        assert!(blocked(Vec3::new(mm(spawn.x), 1.0, beyond)));
    }

    #[test]
    fn the_map_ground_surfaces_only_the_arena_floor() {
        for ground in Ground::ALL {
            let mut map = colosseum();
            map.ground = ground;
            let floors: Vec<_> = boxes(&map)
                .into_iter()
                .filter(|p| p.surface == Surface::ground(ground))
                .collect();
            assert_eq!(floors.len(), 1, "{ground:?}");
            let floor = floors[0].transform;
            assert_eq!(floor.scale.x, mm(map.layout.half_x) * 2.0);
            assert!(floor.translation.y < 0.0);
        }
    }

    #[test]
    fn stone_that_meets_shares_texture_value_and_steps_gently_in_tint() {
        // Floors and room floors are one material look; walls sit a gentle
        // step from the floor; pillars use their own granite.
        let look = |s: Surface| (s.texture(), s.tint());
        assert_eq!(look(Surface::RoomFloor), look(Surface::Floor));
        for pillar in [
            Surface::PillarStone,
            Surface::PillarEdge,
            Surface::PillarBase,
        ] {
            assert_ne!(pillar.texture(), Surface::PitWall.texture());
        }
        assert_eq!(look(Surface::Trim), look(Surface::PitWall));
        let gentle = |a: Surface, b: Surface| {
            let (a, b) = (a.tint().to_srgba(), b.tint().to_srgba());
            for (x, y) in [(a.red, b.red), (a.green, b.green), (a.blue, b.blue)] {
                assert!((x - y).abs() < 0.06, "{a:?} vs {b:?}");
            }
        };
        gentle(Surface::Floor, Surface::PitWall);
        // Cover must read against the walls behind it: shafts are at least
        // twice as bright, and plinths step between the floor and the shaft.
        let luma = |s: Surface| s.tint().to_linear().luminance();
        assert!(luma(Surface::PillarStone) > luma(Surface::PitWall) * 2.0);
        let plinth = luma(Surface::PillarBase);
        assert!(luma(Surface::Floor) < plinth && plinth < luma(Surface::PillarStone));
        assert!(luma(Surface::PillarEdge) > luma(Surface::PillarStone));
        let map = colosseum();
        let rooms = boxes(&map)
            .into_iter()
            .filter(|p| p.surface == Surface::RoomFloor)
            .count();
        assert_eq!(rooms, map.layout.rooms.len());
    }

    #[test]
    fn spawned_pillars_are_hexagonal_prisms() {
        let mut world = World::new();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut images = Assets::<Image>::default();
        let assets = VenueAssets::new(&mut meshes, &mut materials, &mut images);
        let map = colosseum();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        spawn(
            &mut Commands::new(&mut queue, &world),
            &mut meshes,
            &assets,
            &map,
        );
        queue.apply(&mut world);
        let shaft = assets.material(Surface::PillarStone);
        let mut query = world.query::<(&Mesh3d, &MeshMaterial3d<StandardMaterial>)>();
        let shafts: Vec<usize> = query
            .iter(&world)
            .filter(|(_, material)| material.0 == shaft)
            .map(|(mesh, _)| meshes.get(&mesh.0).unwrap().count_vertices())
            .collect();
        // One per pillar, each six side quads and two six-corner caps rather
        // than a 24-vertex cube.
        assert_eq!(shafts.len(), map.layout.pillars.len());
        assert!(shafts.iter().all(|&vertices| vertices == 6 * 4 + 2 * 6));
    }

    #[test]
    fn every_surface_is_textured_and_nothing_is_paper_thin() {
        // The old flat teal strips on pillars and gates read as plastic.
        for piece in boxes(&colosseum()) {
            let _ = piece.surface.texture();
            assert!(piece.transform.scale.min_element() >= 0.075, "{:?}", piece);
        }
    }

    #[test]
    fn banners_and_pennants_hang_above_the_stands_facing_the_arena() {
        let map = colosseum();
        let slots = banner_slots(&map);
        assert!(slots.len() > 20);
        for (top, facing) in &slots {
            assert!(top.y - heraldry::BANNER_HEIGHT > map.venue.concourse_height() + 4.0);
            let inward = *facing * Vec3::Z;
            assert!(inward.dot(-top.with_y(0.0).normalize()) > 0.3);
        }
        let poles = pole_slots(&map);
        assert!(!poles.is_empty());
        assert!(
            poles
                .iter()
                .all(|base| base.y == map.venue.outer_wall_height + CORNICE_RISE)
        );
    }
}
