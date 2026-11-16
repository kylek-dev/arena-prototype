//! Pulse presentation: several thin, forking blue lightning strands from the
//! caster's casting hand to the target. Presentation only. The rules have
//! already resolved the hit, and nothing here feeds back into the simulation.
//!
//! One additive, unlit mesh is rebuilt every frame while a bolt lives. Each
//! main strand bows away from the others and is roughened by midpoint
//! displacement, so the strands read as separate arcs rather than one beam.
//! Short branches fork off them, and all of it re-rolls many times a second
//! so the bolt crackles. Strands are camera-facing ribbons: a bright
//! hairline core inside a soft glow, with flares at the hand and the impact.
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    light::{NotShadowCaster, NotShadowReceiver},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::{FighterVisual, Practice, appearance::PlayerBody};

/// How long one Pulse's lightning lasts, in seconds.
pub(crate) const LIFETIME: f32 = 0.42;
/// The bolt holds full strength until this fraction of its life, then fades.
const FADE_FROM: f32 = 0.5;
/// Re-rolls per second. Each roll re-forks every strand, which makes the crackle.
const CRACKLE_HZ: f32 = 26.0;
/// Seconds for the leaders to grow from the hand to the target.
const REACH_SECONDS: f32 = 0.05;
/// Main strands per bolt.
pub(crate) const STRANDS: usize = 5;
/// Subdivision levels: a main strand has 2^DEPTH + 1 points.
const DEPTH: u32 = 5;
const BRANCH_DEPTH: u32 = 3;
/// Main strands land anywhere within this radius of the target point.
const LANDING_SPREAD: f32 = 0.17;
/// Height of the struck point (the chest) above a fighter's origin.
const CHEST: f32 = 1.2;
/// Strands leave from just in front of the hand bone, at the fingers.
const FINGER_REACH: f32 = 0.06;
/// Ribbon half widths in metres: a hairline core inside a soft glow.
const CORE_HALF_WIDTH: f32 = 0.007;
const GLOW_HALF_WIDTH: f32 = 0.05;
/// Linear HDR colours. Values above 1 feed the bloom; tonemapping keeps them blue.
const CORE: Vec3 = Vec3::new(1.5, 2.6, 6.0);
const GLOW: Vec3 = Vec3::new(0.08, 0.32, 1.5);

/// One Pulse's lightning, recorded when the rules report a hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Zap {
    pub actor: usize,
    pub target: usize,
    seed: u32,
    age: f32,
}

impl Zap {
    pub fn new(actor: usize, target: usize, tick: u64) -> Self {
        Self {
            actor,
            target,
            seed: hash(tick as u32, (actor as u32) ^ ((target as u32) << 8)),
            age: 0.0,
        }
    }
}

/// The single bolt mesh entity.
#[derive(Component)]
pub(crate) struct Bolt(Handle<Mesh>);

/// A bone or body part lightning can leave from, on the player's casting
/// (left) side. A lower rank is preferred.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CastingHand(u8);

impl CastingHand {
    /// Quaternius kits raise the left hand in their Cast clip: prefer its
    /// knuckles, then the wrist. The procedural fallback names both hands
    /// "Hand", so `pick_origin` keeps the one on the caster's left.
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "middle_01_l" => Some(Self(0)),
            "hand_l" => Some(Self(1)),
            "Hand" => Some(Self(2)),
            _ => None,
        }
    }
    fn either_side(self) -> bool {
        self.0 == 2
    }
}

pub(crate) fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    // Start from a real bolt so the mesh is never empty and its vertex layout
    // matches every later frame (warmup compiles exactly this pipeline).
    let mut sample = Ribbons::default();
    sample.bolt(
        Vec3::Y,
        Vec3::new(0.0, 1.0, -6.0),
        Vec3::new(0.0, 4.0, 8.0),
        0,
        0.1,
    );
    let mesh = meshes.add(sample.into_mesh());
    commands.spawn((
        Name::new("Pulse lightning"),
        Bolt(mesh.clone()),
        Mesh3d(mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            // Vertex colours carry the HDR colour; additive blending
            // premultiplies by their alpha.
            base_color: Color::WHITE,
            alpha_mode: AlphaMode::Add,
            unlit: true,
            double_sided: true,
            cull_mode: None,
            fog_enabled: false,
            ..default()
        })),
        // Vertices are written in world space every frame, so the spawn-time
        // bounds would be stale.
        Transform::default(),
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
        Visibility::Hidden,
    ));
}

/// Marks hand bones and parts under the player's body as they appear,
/// including those of kits swapped in later.
pub(crate) fn tag_hands(
    mut commands: Commands,
    named: Query<(Entity, &Name), Added<Name>>,
    parents: Query<&ChildOf>,
    bodies: Query<(), With<PlayerBody>>,
) {
    for (entity, name) in &named {
        let Some(hand) = CastingHand::from_name(name.as_str()) else {
            continue;
        };
        if parents
            .iter_ancestors(entity)
            .any(|ancestor| bodies.contains(ancestor))
        {
            commands.entity(entity).insert(hand);
        }
    }
}

/// Runs after transform propagation so the bolt leaves from this frame's
/// animated hand, not last frame's.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn draw(
    mut practice: ResMut<Practice>,
    startup: Res<crate::startup::State>,
    time: Res<Time>,
    mut bolts: Query<(&Bolt, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    fighters: Query<(&FighterVisual, &GlobalTransform)>,
    hands: Query<(Entity, &CastingHand, &GlobalTransform)>,
    parents: Query<&ChildOf>,
    shown: Query<&Visibility, Without<Bolt>>,
    bodies: Query<(), With<PlayerBody>>,
) {
    let Ok((bolt, mut visibility)) = bolts.single_mut() else {
        return;
    };
    // Behind the loading overlay a sample bolt is drawn so its pipeline is
    // compiled before the first real Pulse.
    let zap = practice.zap.or_else(|| {
        startup.sweep().map(|_| Zap {
            age: 0.12,
            ..Zap::new(0, 1, 0)
        })
    });
    let fighter = |index: usize| {
        fighters
            .iter()
            .find(|(visual, _)| visual.0 == index)
            .map(|(_, transform)| *transform)
    };
    let frame = zap.and_then(|zap| {
        let eye = cameras.single().ok()?.translation();
        let caster = fighter(zap.actor)?;
        let target = fighter(zap.target)?.translation() + Vec3::Y * CHEST;
        let candidates: Vec<_> = (zap.actor == 0)
            .then(|| {
                hands.iter().map(|(entity, rank, transform)| {
                    // Only the shown model counts: a kit still loading, or the
                    // fallback it replaced, sits under a hidden root. The
                    // fighter itself may be hidden when the camera is inside it.
                    let active = parents
                        .iter_ancestors(entity)
                        .take_while(|ancestor| !bodies.contains(*ancestor))
                        .all(|ancestor| !matches!(shown.get(ancestor), Ok(Visibility::Hidden)));
                    (*rank, transform.translation(), active)
                })
            })
            .into_iter()
            .flatten()
            .collect();
        let hand = pick_origin(&candidates, &caster).unwrap_or_else(|| {
            caster.translation() + caster.rotation() * Vec3::new(-0.18, 1.3, -0.4)
        });
        let origin = hand + (target - hand).normalize_or_zero() * FINGER_REACH;
        let mut ribbons = Ribbons::default();
        ribbons.bolt(origin, target, eye, zap.seed, zap.age);
        Some(ribbons)
    });
    if let Some(zap) = &mut practice.zap {
        zap.age += time.delta_secs();
        if zap.age >= LIFETIME {
            practice.zap = None;
        }
    }
    let next = match frame {
        Some(ribbons) if !ribbons.indices.is_empty() => {
            if let Some(mut mesh) = meshes.get_mut(&bolt.0) {
                ribbons.write(&mut mesh);
            }
            Visibility::Visible
        }
        _ => Visibility::Hidden,
    };
    if *visibility != next {
        *visibility = next;
    }
}

/// The best shown hand on the caster's casting side. `candidates` are
/// (rank, world position, shown).
fn pick_origin(candidates: &[(CastingHand, Vec3, bool)], caster: &GlobalTransform) -> Option<Vec3> {
    candidates
        .iter()
        .filter(|(_, _, active)| *active)
        .filter(|(rank, position, _)| {
            // Models face their local −Z, so the caster's left is local −X.
            !rank.either_side()
                || (*position - caster.translation()).dot(caster.right().into()) < 0.0
        })
        .min_by_key(|(rank, _, _)| *rank)
        .map(|(_, position, _)| *position)
}

/// Brightness over a bolt's life: full at once, then a linear fade to zero.
pub(crate) fn envelope(age: f32) -> f32 {
    if !(0.0..LIFETIME).contains(&age) {
        return 0.0;
    }
    let fade = LIFETIME * FADE_FROM;
    if age < fade {
        1.0
    } else {
        1.0 - (age - fade) / (LIFETIME - fade)
    }
}

/// One jagged polyline of a bolt.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Strand {
    pub points: Vec<Vec3>,
    /// Branches are thinner and dimmer and start part way along a main strand.
    pub branch: bool,
    /// Fraction of the main strand's length at which this strand starts.
    pub starts_at: f32,
    /// Per-roll flicker, 0 when the strand skips this roll.
    pub strength: f32,
}

/// The strands of one roll of a bolt from `from` to `to`. Deterministic in
/// `seed`; main strands come first.
pub(crate) fn fork(from: Vec3, to: Vec3, seed: u32) -> Vec<Strand> {
    let mut rng = Rng::new(seed);
    let span = to - from;
    let length = span.length().max(0.01);
    let direction = span.normalize_or(Vec3::NEG_Z);
    let (u, v) = direction.any_orthonormal_pair();
    let mut strands = Vec::with_capacity(STRANDS * 3);
    let mut branches = Vec::new();
    let turn = rng.unit() * std::f32::consts::TAU;
    for index in 0..STRANDS {
        // Even bow headings (with jitter) fan the strands apart between the
        // hand and the target so each reads on its own.
        let heading =
            turn + (index as f32 + 0.35 * rng.signed()) * std::f32::consts::TAU / STRANDS as f32;
        let bow = (u * heading.cos() + v * heading.sin())
            * (length * (0.045 + 0.045 * rng.unit())).min(0.45);
        let landing = {
            let angle = rng.unit() * std::f32::consts::TAU;
            (u * angle.cos() + v * angle.sin()) * LANDING_SPREAD * rng.unit().sqrt()
        };
        let start = from + (u * rng.signed() + v * rng.signed()) * 0.012;
        let end = to + landing;
        let roughness = (length * 0.06).clamp(0.03, 0.3);
        let points: Vec<Vec3> = jag(&mut rng, DEPTH, roughness, (u, v))
            .into_iter()
            .enumerate()
            .map(|(k, offset)| {
                let t = k as f32 / (1 << DEPTH) as f32;
                start.lerp(end, t) + bow * (t * std::f32::consts::PI).sin() + offset
            })
            .collect();
        // The first strand never skips a roll, so a bolt is never empty.
        let strength = if index > 0 && rng.unit() < 0.15 {
            0.0
        } else {
            0.6 + 0.4 * rng.unit()
        };
        let forks = 1 + usize::from(rng.unit() < 0.6);
        for _ in 0..forks {
            let count = points.len() - 1;
            let at = (count as f32 * (0.12 + 0.6 * rng.unit())) as usize;
            let tangent = (points[at + 1] - points[at]).normalize_or(direction);
            let spread = (u * rng.signed() + v * rng.signed()).normalize_or(u);
            let heading = (tangent + spread * (0.6 + 0.6 * rng.unit())).normalize();
            let reach = (length * (0.06 + 0.12 * rng.unit())).clamp(0.12, 0.9);
            let (bu, bv) = heading.any_orthonormal_pair();
            let base = points[at];
            let twig: Vec<Vec3> = jag(&mut rng, BRANCH_DEPTH, reach * 0.18, (bu, bv))
                .into_iter()
                .enumerate()
                .map(|(k, offset)| {
                    base + heading * reach * k as f32 / (1 << BRANCH_DEPTH) as f32 + offset
                })
                .collect();
            branches.push(Strand {
                points: twig,
                branch: true,
                starts_at: at as f32 / count as f32,
                strength: strength * (0.45 + 0.35 * rng.unit()),
            });
        }
        strands.push(Strand {
            points,
            branch: false,
            starts_at: 0.0,
            strength,
        });
    }
    strands.extend(branches);
    strands
}

/// Midpoint displacement in the plane `basis`: 2^depth + 1 offsets that are
/// zero at both ends, with each level half as rough as the one above.
fn jag(rng: &mut Rng, depth: u32, roughness: f32, basis: (Vec3, Vec3)) -> Vec<Vec3> {
    let count = 1usize << depth;
    let mut offsets = vec![Vec3::ZERO; count + 1];
    let mut amplitude = roughness;
    let mut step = count;
    while step > 1 {
        let half = step / 2;
        for start in (0..count).step_by(step) {
            let middle = (offsets[start] + offsets[start + step]) * 0.5;
            offsets[start + half] =
                middle + (basis.0 * rng.signed() + basis.1 * rng.signed()) * amplitude;
        }
        amplitude *= 0.55;
        step = half;
    }
    offsets
}

/// Camera-facing ribbon geometry in world space.
#[derive(Default)]
pub(crate) struct Ribbons {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl Ribbons {
    /// The whole bolt at `age`, seen from `eye`.
    pub fn bolt(&mut self, from: Vec3, to: Vec3, eye: Vec3, seed: u32, age: f32) {
        let fade = envelope(age);
        if fade <= 0.0 {
            return;
        }
        let roll = (age * CRACKLE_HZ) as u32;
        let reach = (age / REACH_SECONDS).clamp(0.0, 1.0).max(0.2);
        let strands = fork(from, to, hash(seed, roll));
        for strand in &strands {
            let grown = if strand.branch {
                (reach - strand.starts_at) / (1.0 - strand.starts_at).max(0.01)
            } else {
                reach
            };
            let shown = ((strand.points.len() - 1) as f32 * grown).ceil() as usize + 1;
            if strand.strength <= 0.0 || grown <= 0.0 || shown < 2 {
                continue;
            }
            let points = &strand.points[..shown.min(strand.points.len())];
            let (scale, strength) = if strand.branch {
                (0.65, strand.strength * fade)
            } else {
                (1.0, strand.strength * fade)
            };
            self.strip(
                points,
                eye,
                GLOW_HALF_WIDTH * scale,
                GLOW,
                0.55 * strength,
                0.0,
                strand.branch,
            );
            self.strip(
                points,
                eye,
                CORE_HALF_WIDTH * scale,
                CORE,
                strength,
                0.45 * strength,
                strand.branch,
            );
        }
        let mut rng = Rng::new(hash(seed ^ 0x5eed, roll));
        self.flare(from, eye, 0.08 + 0.04 * rng.unit(), fade);
        if reach >= 1.0 {
            self.flare(to, eye, 0.2 + 0.08 * rng.unit(), 0.8 * fade);
        }
    }

    /// A ribbon of three vertices across: edges at `edge` alpha, centre at
    /// `centre`. Branches taper to their tip.
    #[allow(clippy::too_many_arguments)]
    fn strip(
        &mut self,
        points: &[Vec3],
        eye: Vec3,
        half_width: f32,
        color: Vec3,
        centre: f32,
        edge: f32,
        taper: bool,
    ) {
        let base = self.positions.len() as u32;
        let last = points.len() - 1;
        for (k, point) in points.iter().enumerate() {
            let tangent = (points[k.min(last - 1) + 1] - points[k.saturating_sub(1).min(last - 1)])
                .normalize_or(Vec3::Z);
            let side = tangent
                .cross(eye - *point)
                .normalize_or(tangent.any_orthonormal_vector());
            let along = k as f32 / last as f32;
            let width = if taper { 1.0 - 0.75 * along } else { 1.0 } * half_width;
            let tip = if taper { 1.0 - along * along } else { 1.0 };
            for (offset, alpha) in [(-width, edge), (0.0, centre), (width, edge)] {
                self.positions.push((*point + side * offset).to_array());
                self.colors.push(color.extend(alpha * tip).to_array());
            }
        }
        for k in 0..last as u32 {
            let [a, b] = [base + 3 * k, base + 3 * (k + 1)];
            for lane in 0..2 {
                let [a, b] = [a + lane, b + lane];
                self.indices.extend([a, b, a + 1, a + 1, b, b + 1]);
            }
        }
    }

    /// A soft camera-facing glow disc.
    fn flare(&mut self, centre: Vec3, eye: Vec3, radius: f32, strength: f32) {
        const SIDES: u32 = 14;
        let (u, v) = (eye - centre).normalize_or(Vec3::Z).any_orthonormal_pair();
        let base = self.positions.len() as u32;
        self.positions.push(centre.to_array());
        self.colors.push((CORE * 0.8).extend(strength).to_array());
        for side in 0..SIDES {
            let angle = side as f32 * std::f32::consts::TAU / SIDES as f32;
            self.positions
                .push((centre + (u * angle.cos() + v * angle.sin()) * radius).to_array());
            self.colors.push(GLOW.extend(0.0).to_array());
        }
        for side in 0..SIDES {
            self.indices
                .extend([base, base + 1 + side, base + 1 + (side + 1) % SIDES]);
        }
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        self.write(&mut mesh);
        mesh
    }

    fn write(self, mesh: &mut Mesh) {
        let count = self.positions.len();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_indices(Indices::U32(self.indices));
    }
}

/// A small xorshift generator: decoration only, so quality needs are modest.
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(hash(seed, 0x9e37_79b9) | 1)
    }
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    /// In [0, 1).
    fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
    /// In [-1, 1).
    fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

fn hash(a: u32, b: u32) -> u32 {
    let mut x = a.wrapping_mul(0x85eb_ca6b) ^ b.wrapping_mul(0xc2b2_ae35).rotate_left(16);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    const FROM: Vec3 = Vec3::new(0.3, 1.4, 0.0);
    const TO: Vec3 = Vec3::new(-1.0, 1.2, -6.0);

    fn distance_to_segment(point: Vec3) -> f32 {
        let span = TO - FROM;
        let t = ((point - FROM).dot(span) / span.length_squared()).clamp(0.0, 1.0);
        point.distance(FROM + span * t)
    }

    #[test]
    fn several_thin_strands_leave_the_hand_and_land_on_the_target() {
        for seed in 0..200 {
            let strands = fork(FROM, TO, seed);
            let main: Vec<_> = strands.iter().filter(|s| !s.branch).collect();
            assert_eq!(main.len(), STRANDS);
            assert!(strands.iter().filter(|s| s.branch).count() >= STRANDS);
            assert!(main[0].strength > 0.0, "a roll is never empty");
            for strand in &main {
                assert_eq!(strand.points.len(), (1 << DEPTH) + 1);
                assert!(strand.points[0].distance(FROM) < 0.03);
                assert!(strand.points.last().unwrap().distance(TO) <= LANDING_SPREAD + 1e-4);
                // Jagged, but still a bolt between the two, not a scribble.
                for point in &strand.points {
                    assert!(distance_to_segment(*point) < 1.0, "seed {seed}: {point}");
                }
            }
            for twig in strands.iter().filter(|s| s.branch) {
                assert!((0.0..1.0).contains(&twig.starts_at));
                assert!(twig.strength <= main.iter().map(|s| s.strength).fold(0.0, f32::max));
                let length = twig.points[0].distance(*twig.points.last().unwrap());
                assert!((0.05..1.0).contains(&length), "{length}");
            }
        }
    }

    #[test]
    fn strands_fan_apart_instead_of_overlapping_into_one_beam() {
        for seed in 0..200 {
            let strands = fork(FROM, TO, seed);
            let middles: Vec<Vec3> = strands
                .iter()
                .filter(|s| !s.branch)
                .map(|s| s.points[s.points.len() / 2])
                .collect();
            let mut nearest = f32::MAX;
            let mut total = 0.0;
            let mut pairs = 0;
            for (i, a) in middles.iter().enumerate() {
                for b in &middles[i + 1..] {
                    nearest = nearest.min(a.distance(*b));
                    total += a.distance(*b);
                    pairs += 1;
                }
            }
            assert!(total / pairs as f32 > 0.25, "seed {seed}");
            assert!(nearest > 0.01, "seed {seed}");
        }
    }

    #[test]
    fn rolls_are_deterministic_and_re_fork_every_roll() {
        assert_eq!(fork(FROM, TO, 7), fork(FROM, TO, 7));
        assert_ne!(fork(FROM, TO, 7), fork(FROM, TO, 8));
        let mut a = Ribbons::default();
        let mut b = Ribbons::default();
        a.bolt(FROM, TO, Vec3::new(0.0, 5.0, 8.0), 3, 0.10);
        b.bolt(
            FROM,
            TO,
            Vec3::new(0.0, 5.0, 8.0),
            3,
            0.10 + 1.0 / CRACKLE_HZ,
        );
        assert_ne!(a.positions, b.positions);
    }

    #[test]
    fn envelope_holds_then_fades_out_by_the_end_of_life() {
        assert_eq!(envelope(0.0), 1.0);
        assert_eq!(envelope(LIFETIME * FADE_FROM * 0.9), 1.0);
        let mut previous = 1.0;
        for step in 0..=100 {
            let value = envelope(LIFETIME * step as f32 / 100.0);
            assert!(value <= previous && value >= 0.0);
            previous = value;
        }
        assert_eq!(envelope(LIFETIME), 0.0);
        assert_eq!(envelope(-0.1), 0.0);
        let mut ribbons = Ribbons::default();
        ribbons.bolt(FROM, TO, Vec3::Y * 9.0, 1, LIFETIME);
        assert!(ribbons.indices.is_empty());
    }

    #[test]
    fn ribbons_are_thin_valid_and_grow_from_the_hand() {
        let eye = Vec3::new(2.0, 6.0, 9.0);
        let mut early = Ribbons::default();
        early.bolt(FROM, TO, eye, 11, 0.0);
        let mut full = Ribbons::default();
        full.bolt(FROM, TO, eye, 11, REACH_SECONDS);
        for ribbons in [&early, &full] {
            assert_eq!(ribbons.positions.len(), ribbons.colors.len());
            assert_eq!(ribbons.indices.len() % 3, 0);
            assert!(
                ribbons
                    .indices
                    .iter()
                    .all(|&i| (i as usize) < ribbons.positions.len())
            );
            assert!(
                ribbons
                    .colors
                    .iter()
                    .flatten()
                    .all(|c| c.is_finite() && *c >= 0.0)
            );
        }
        // Leaders are still on their way: nothing near the target yet.
        let reached = |r: &Ribbons| {
            r.positions
                .iter()
                .map(|p| Vec3::from_array(*p).distance(TO))
                .fold(f32::MAX, f32::min)
        };
        assert!(reached(&early) > 1.0);
        assert!(reached(&full) < LANDING_SPREAD + GLOW_HALF_WIDTH + 0.3);
        // The core ribbon is a hairline, not a beam.
        const { assert!(CORE_HALF_WIDTH * 2.0 < 0.02) };
        // Blue, bright enough to bloom.
        const { assert!(CORE.z > CORE.x && CORE.z > 1.0 && GLOW.z > GLOW.y && GLOW.y > GLOW.x) };
    }

    #[test]
    fn the_shown_left_hand_is_the_origin() {
        let caster = GlobalTransform::from(
            Transform::from_xyz(4.0, 0.0, 2.0).with_rotation(Quat::from_rotation_y(1.1)),
        );
        let local = |x: f32| caster.transform_point(Vec3::new(x, 1.3, -0.3));
        let (left, right) = (local(-0.3), local(0.3));
        let fallback = [(CastingHand(2), right, true), (CastingHand(2), left, true)];
        assert_eq!(pick_origin(&fallback, &caster), Some(left));
        // A shown kit's bones beat the fallback; a hidden kit's never do.
        let knuckle = local(-0.35);
        let mut all = fallback.to_vec();
        all.push((CastingHand(1), local(-0.32), true));
        all.push((CastingHand(0), knuckle, true));
        assert_eq!(pick_origin(&all, &caster), Some(knuckle));
        for entry in all.iter_mut().filter(|e| e.0.0 < 2) {
            entry.2 = false;
        }
        assert_eq!(pick_origin(&all, &caster), Some(left));
        assert_eq!(pick_origin(&[], &caster), None);
    }

    #[test]
    fn only_the_players_hands_are_tagged() {
        let mut world = World::new();
        let body = world.spawn(PlayerBody).id();
        let kit = world.spawn(ChildOf(body)).id();
        let knuckle = world.spawn((Name::new("middle_01_l"), ChildOf(kit))).id();
        let right = world.spawn((Name::new("middle_01_r"), ChildOf(kit))).id();
        let fallback = world.spawn((Name::new("Hand"), ChildOf(body))).id();
        let opponent = world.spawn_empty().id();
        let theirs = world.spawn((Name::new("Hand"), ChildOf(opponent))).id();
        world.run_system_once(tag_hands).unwrap();
        assert_eq!(world.get::<CastingHand>(knuckle), Some(&CastingHand(0)));
        assert_eq!(world.get::<CastingHand>(fallback), Some(&CastingHand(2)));
        assert_eq!(world.get::<CastingHand>(right), None);
        assert_eq!(world.get::<CastingHand>(theirs), None);
    }

    #[test]
    fn a_hit_shows_lightning_from_the_hand_then_hides_it() {
        use bevy::time::TimeUpdateStrategy;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Practice>()
            .init_resource::<crate::startup::State>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(50),
            ))
            .add_systems(Update, draw);
        let world = app.world_mut();
        world
            .run_system_once(
                |mut commands: Commands,
                 mut meshes: ResMut<Assets<Mesh>>,
                 mut materials: ResMut<Assets<StandardMaterial>>| {
                    spawn(&mut commands, &mut meshes, &mut materials)
                },
            )
            .unwrap();
        world.spawn((
            Camera3d::default(),
            GlobalTransform::from_xyz(0.0, 5.0, 10.0),
        ));
        let caster = world
            .spawn((FighterVisual(0), GlobalTransform::default()))
            .id();
        world.spawn((FighterVisual(1), GlobalTransform::from_xyz(0.0, 0.0, -6.0)));
        let body = world.spawn((PlayerBody, ChildOf(caster))).id();
        let palm = Vec3::new(-0.2, 1.45, -0.45);
        world.spawn((
            CastingHand(0),
            GlobalTransform::from_translation(palm),
            ChildOf(body),
        ));
        let bolt = world.query::<&Bolt>().single(world).unwrap().0.clone();
        let shown = |app: &mut App| {
            let world = app.world_mut();
            *world
                .query_filtered::<&Visibility, With<Bolt>>()
                .single(world)
                .unwrap()
        };

        app.update();
        assert_eq!(shown(&mut app), Visibility::Hidden, "no hit, no bolt");

        app.world_mut().resource_mut::<Practice>().zap = Some(Zap::new(0, 1, 30));
        app.update();
        assert_eq!(shown(&mut app), Visibility::Visible);
        let mesh = app.world().resource::<Assets<Mesh>>().get(&bolt).unwrap();
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        // Strands leave from the fingers just past the palm, not the stomach.
        let nearest = positions
            .iter()
            .map(|p| Vec3::from_array(*p).distance(palm))
            .fold(f32::MAX, f32::min);
        assert!(nearest < FINGER_REACH + 0.05, "{nearest}");

        for _ in 0..(LIFETIME / 0.05).ceil() as usize + 1 {
            app.update();
        }
        assert_eq!(shown(&mut app), Visibility::Hidden);
        assert_eq!(app.world().resource::<Practice>().zap, None);
    }

    #[test]
    fn zaps_differ_per_cast() {
        assert_ne!(Zap::new(0, 1, 30).seed, Zap::new(0, 1, 60).seed);
        assert_eq!(Zap::new(0, 1, 30).age, 0.0);
    }
}
