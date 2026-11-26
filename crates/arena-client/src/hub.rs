//! The social hub in the renderer ([ADR 0056](../../../docs/decisions/0056-social-hub.md)):
//! the graybox plaza, the local player walking it (predicted with the
//! door's own `arena_sim::hub` rules), and everyone else in view with their
//! chosen look, interpolated between the door's 10 Hz states. The page owns
//! the connection: it hands over the hub as JSON (`readHub`) and takes the
//! local intent (`onHubIntent`) and where each name goes on screen
//! (`onHubScreen`).

use std::collections::{HashMap, VecDeque};

use arena_sim::arena::{AXIS_LIMIT, Axes, Position};
use arena_sim::hub::{self as rules, Body, HUB_TICKS_PER_SECOND, Intent, MAX_HUB_PLAYERS};
use bevy::prelude::*;

/// Kit instances for hub players use actors from here up, one per slot, so
/// they never collide with the arena's fighters (`0..MAX_FIGHTERS`).
pub const HUB_ACTOR_BASE: usize = 1000;
/// Seconds per hub tick.
const STEP: f32 = 1.0 / HUB_TICKS_PER_SECOND as f32;
/// A correction larger than this is a jump, not drawn as a blend.
const SNAP_METRES: f32 = 3.0;
/// Corrections fade at this rate per second (about 150 ms to settle).
const BLEND_PER_SECOND: f32 = 20.0;
/// Moves kept for replay; the door holds at most 8 waiting.
const MAX_PENDING: usize = 32;

/// One player in view, as the page described them.
#[derive(Clone, Debug, PartialEq)]
pub struct Seen {
    pub id: u32,
    pub x: i32,
    pub z: i32,
    pub facing: u16,
    /// Millimetres above the plaza (jumping).
    pub height: i32,
    pub admin: bool,
    /// The render spec for their look (as the arena's roster specs), or
    /// `None` when the page could not build one (a kit not installed).
    pub spec: Option<serde_json::Value>,
}

/// The hub as the page last handed it over.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub x: i32,
    pub z: i32,
    /// This player's height, upward speed and jump momentum: with `x`, `z`
    /// the door's whole body for it.
    pub y: i32,
    pub vy: i32,
    pub mx: i32,
    pub mz: i32,
    pub tick: u64,
    /// The last of this player's moves the door applied.
    pub ack: u32,
    pub players: Vec<Seen>,
}

/// Parses `readHub`'s JSON: `null` (not in the hub) or
/// `{x, z, tick, players: [{id, x, z, facing, admin, spec}]}`. Anything
/// malformed reads as not in the hub.
pub fn parse_view(json: &str) -> Option<View> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let object = value.as_object()?;
    let int = |v: &serde_json::Value| v.as_i64().and_then(|n| i32::try_from(n).ok());
    let players = object.get("players")?.as_array()?;
    if players.len() >= MAX_HUB_PLAYERS {
        return None;
    }
    let mut seen = Vec::with_capacity(players.len());
    for player in players {
        let p = player.as_object()?;
        seen.push(Seen {
            id: p.get("id")?.as_u64().and_then(|n| u32::try_from(n).ok())?,
            x: int(p.get("x")?)?,
            z: int(p.get("z")?)?,
            facing: p
                .get("facing")?
                .as_u64()
                .and_then(|n| u16::try_from(n).ok())?,
            height: int(p.get("height")?)?.max(0),
            admin: p.get("admin")?.as_bool()?,
            spec: p.get("spec").filter(|spec| spec.is_object()).cloned(),
        });
    }
    Some(View {
        x: int(object.get("x")?)?,
        z: int(object.get("z")?)?,
        y: int(object.get("y")?)?.max(0),
        vy: int(object.get("vy")?)?,
        mx: int(object.get("mx")?)?,
        mz: int(object.get("mz")?)?,
        tick: object.get("tick")?.as_u64()?,
        ack: object
            .get("ack")?
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())?,
        players: seen,
    })
}

/// A binary angle (65536 per turn, 0 facing −Z) as a yaw in radians.
pub fn yaw(facing: u16) -> f32 {
    f32::from(facing) / 65536.0 * std::f32::consts::TAU
}

/// Millimetres on the plaza as world metres.
pub fn world(at: Position) -> Vec3 {
    Vec3::new(at.x as f32 / 1000.0, 0.0, at.z as f32 / 1000.0)
}

/// A body in world metres, its height included.
pub fn body_world(body: Body) -> Vec3 {
    world(body.at) + Vec3::Y * body.height as f32 / 1000.0
}

/// The local player's walk, predicted with the door's rules: one numbered
/// move per hub tick (sent to the door, which applies each once, in order),
/// drawn between the last two steps. Each door state is reconciled by
/// starting from the door's place and replaying the moves it has not
/// applied yet, so prediction and door agree exactly; what changes on
/// screen fades out instead of jumping.
#[derive(Clone, Debug, PartialEq)]
pub struct Walk {
    pub previous: Body,
    pub current: Body,
    /// The ground intent (walking and facing); a jump press waits in `jump`
    /// for the next step.
    pub intent: Intent,
    pub jump: bool,
    /// Seconds into the current hub tick.
    pub elapsed: f32,
    /// The last move's number, and the moves the door has not applied.
    seq: u32,
    pending: VecDeque<(u32, Intent)>,
    /// The intent last sent, so standing still sends nothing.
    sent: Intent,
    /// Drawn offset left by a correction, fading to zero.
    error: Vec3,
}

impl Walk {
    pub fn at(start: Position) -> Self {
        let body = Body::standing(start);
        Self {
            previous: body,
            current: body,
            intent: Intent::default(),
            jump: false,
            elapsed: 0.0,
            seq: 0,
            pending: VecDeque::new(),
            sent: Intent::default(),
            error: Vec3::ZERO,
        }
    }

    /// Advances by `dt` seconds of the current intent; returns the moves to
    /// send: one per step that walks, turns, jumps or is in the air (so the
    /// door's replay of a jump matches tick for tick).
    pub fn advance(&mut self, dt: f32) -> Vec<(u32, Intent)> {
        let dt = dt.clamp(0.0, 1.0);
        self.error *= (-BLEND_PER_SECOND * dt).exp();
        self.elapsed += dt;
        let mut moves = Vec::new();
        while self.elapsed >= STEP {
            self.elapsed -= STEP;
            let intent = Intent {
                jump: std::mem::take(&mut self.jump),
                ..self.intent
            };
            let flying = self.current.airborne();
            self.previous = self.current;
            self.current = rules::step_body(self.current, intent);
            let walking = intent.movement != Axes::default();
            if walking || flying || intent.jump || intent != self.sent {
                self.seq = self.seq.wrapping_add(1).max(1);
                self.sent = intent;
                self.pending.push_back((self.seq, intent));
                if self.pending.len() > MAX_PENDING {
                    self.pending.pop_front();
                }
                moves.push((self.seq, intent));
            }
        }
        moves
    }

    /// Where to draw the player now.
    pub fn shown(&self) -> Vec3 {
        body_world(self.previous).lerp(body_world(self.current), self.elapsed / STEP) + self.error
    }

    /// The door's body for this player after its move `ack`: replays the
    /// moves after it from there.
    pub fn reconcile(&mut self, door: Body, ack: u32) {
        let before = self.shown();
        while self.pending.front().is_some_and(|(seq, _)| *seq <= ack) {
            self.pending.pop_front();
        }
        let mut at = door;
        let mut last = door;
        for (_, intent) in &self.pending {
            last = at;
            at = rules::step_body(at, *intent);
        }
        // Mid-step, the drawn position blends from the step before.
        self.previous = if self.pending.is_empty() { at } else { last };
        self.current = at;
        self.error = Vec3::ZERO;
        let gap = before - self.shown();
        // A small difference fades out; a jump is a jump.
        if gap.length() <= SNAP_METRES {
            self.error = gap;
        }
    }
}

/// The intent for the arena's world movement (`Practice::movement`, a unit
/// vector at most) and heading.
pub fn intent(movement: Vec2, heading: u16) -> Intent {
    let axis = |v: f32| (v.clamp(-1.0, 1.0) * f32::from(AXIS_LIMIT)).round() as i16;
    Intent {
        movement: Axes {
            x: axis(movement.x),
            z: axis(movement.y),
        },
        facing: heading,
        jump: false,
    }
}

/// Marks a hub player's avatar root, and the plaza's entities.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HubAvatar {
    pub id: u32,
    /// Kit instances under it belong to actor `HUB_ACTOR_BASE + slot`.
    pub slot: usize,
}
#[derive(Component)]
pub struct Plaza;
/// The plaza's camera obstacles: the only ones the boom meets in the hub.
#[derive(Component)]
pub struct PlazaObstacle;

/// Someone else in view: drawn between the door's last two places.
struct Remote {
    slot: usize,
    entity: Entity,
    from: Vec3,
    to: Vec3,
    /// Seconds since the newest place arrived.
    since: f32,
    facing: f32,
    spec: Option<serde_json::Value>,
}

/// The hub as the renderer keeps it.
#[derive(Resource, Default)]
pub struct Hub {
    revision: Option<f64>,
    tick: u64,
    /// The local player's predicted walk; `None` outside the hub.
    pub walk: Option<Walk>,
    remotes: HashMap<u32, Remote>,
}

impl Hub {
    pub fn active(&self) -> bool {
        self.walk.is_some()
    }

    /// Height (mm) and upward speed (mm per arena tick) to animate a hub
    /// avatar's jump with: the local walk's own, or estimated for others
    /// from their last two places.
    pub fn airborne(&self, slot: Option<usize>) -> (i32, i32) {
        match slot {
            None => self
                .walk
                .as_ref()
                .map_or((0, 0), |walk| (walk.current.height, walk.current.vertical)),
            Some(slot) => self
                .remotes
                .values()
                .find(|remote| remote.slot == slot)
                .map_or((0, 0), |remote| {
                    let height = (remote.to.y * 1000.0) as i32;
                    let rising = ((remote.to.y - remote.from.y) * 1000.0) as i32;
                    (height, rising / 3)
                }),
        }
    }
}

/// Graybox colours for the plaza: the arena's stone, a darker trim.
const FLOOR: crate::palette::Swatch = crate::palette::STONE_3;
const STONE: crate::palette::Swatch = crate::palette::STONE_5;

/// The page's hub, when it changed: enters, leaves, moves the others and
/// corrects the local walk.
#[allow(clippy::too_many_arguments)]
pub(crate) fn read(
    mut commands: Commands,
    mut hub: ResMut<Hub>,
    mut desired: ResMut<crate::appearance::Desired>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut venue: Query<&mut Visibility, (With<crate::venue::VenueEntity>, Without<HubAvatar>)>,
    plaza: Query<Entity, With<Plaza>>,
    avatars: Query<(&HubAvatar, &Transform)>,
    mut orbit: Option<ResMut<crate::Orbit>>,
) {
    let Some((revision, json)) = crate::platform::read_hub() else {
        return;
    };
    if hub.revision == Some(revision) {
        return;
    }
    hub.revision = Some(revision);
    let Some(view) = parse_view(&json) else {
        if hub.active() {
            leave(&mut commands, &mut hub, &mut desired, &mut venue, &plaza);
            // The boom starts afresh where the player now stands.
            if let Some(orbit) = orbit.as_mut() {
                orbit.eye = None;
            }
        }
        return;
    };
    let door = Body {
        at: Position {
            x: view.x,
            z: view.z,
        },
        height: view.y,
        vertical: view.vy,
        momentum: Position {
            x: view.mx,
            z: view.mz,
        },
    };
    if !hub.active() {
        // Entering: the arena's venue gives way to the plaza.
        for mut visibility in &mut venue {
            *visibility = Visibility::Hidden;
        }
        spawn_plaza(&mut commands, &mut meshes, &mut materials);
        hub.walk = Some(Walk::at(door.at));
        if let Some(orbit) = orbit.as_mut() {
            orbit.eye = None;
        }
    } else if view.tick != hub.tick
        && let Some(walk) = hub.walk.as_mut()
    {
        walk.reconcile(door, view.ack);
    }
    hub.tick = view.tick;
    let shown: HashMap<u32, Vec3> = avatars
        .iter()
        .map(|(avatar, transform)| (avatar.id, transform.translation))
        .collect();
    // Those who left go first, so their slots are free for arrivals: two
    // players never share a kit actor.
    let gone: Vec<u32> = hub
        .remotes
        .keys()
        .copied()
        .filter(|id| !view.players.iter().any(|seen| seen.id == *id))
        .collect();
    for id in gone {
        if let Some(remote) = hub.remotes.remove(&id) {
            commands.entity(remote.entity).despawn();
            desired.set_hub(HUB_ACTOR_BASE + remote.slot, None);
        }
    }
    for seen in view.players {
        let to = world(Position {
            x: seen.x,
            z: seen.z,
        }) + Vec3::Y * seen.height as f32 / 1000.0;
        let facing = yaw(seen.facing);
        if let Some(remote) = hub.remotes.get_mut(&seen.id) {
            remote.from = shown.get(&seen.id).copied().unwrap_or(remote.to);
            remote.to = to;
            remote.since = 0.0;
            remote.facing = facing;
            if remote.spec != seen.spec {
                remote.spec = seen.spec.clone();
                desired.set_hub(HUB_ACTOR_BASE + remote.slot, spec_of(&seen.spec));
            }
            continue;
        }
        // The lowest free slot keeps kit actors few and stable.
        let Some(slot) = (0..MAX_HUB_PLAYERS)
            .find(|slot| !hub.remotes.values().any(|remote| remote.slot == *slot))
        else {
            continue;
        };
        let entity = commands
            .spawn((
                HubAvatar { id: seen.id, slot },
                Transform::from_translation(to).with_rotation(Quat::from_rotation_y(facing)),
                Visibility::default(),
            ))
            .id();
        desired.set_hub(HUB_ACTOR_BASE + slot, spec_of(&seen.spec));
        hub.remotes.insert(
            seen.id,
            Remote {
                slot,
                entity,
                from: to,
                to,
                since: 0.0,
                facing,
                spec: seen.spec,
            },
        );
    }
}

fn spec_of(value: &Option<serde_json::Value>) -> Option<crate::appearance::Spec> {
    value.as_ref().and_then(crate::appearance::Spec::from_value)
}

fn leave(
    commands: &mut Commands,
    hub: &mut Hub,
    desired: &mut crate::appearance::Desired,
    venue: &mut Query<&mut Visibility, (With<crate::venue::VenueEntity>, Without<HubAvatar>)>,
    plaza: &Query<Entity, With<Plaza>>,
) {
    for (_, remote) in hub.remotes.drain() {
        commands.entity(remote.entity).despawn();
        desired.set_hub(HUB_ACTOR_BASE + remote.slot, None);
    }
    for entity in plaza {
        commands.entity(entity).despawn();
    }
    for mut visibility in venue.iter_mut() {
        *visibility = Visibility::Inherited;
    }
    hub.walk = None;
    crate::platform::hub_screen("[]");
}

fn spawn_plaza(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let side = 2.0 * rules::PLAZA_HALF as f32 / 1000.0;
    let material = |materials: &mut Assets<StandardMaterial>, swatch| {
        materials.add(StandardMaterial {
            base_color: crate::palette::color(swatch),
            perceptual_roughness: 0.9,
            ..default()
        })
    };
    let floor = material(materials, FLOOR);
    let stone = material(materials, STONE);
    commands.spawn((
        Plaza,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(side, side))),
        MeshMaterial3d(floor),
        Transform::default(),
    ));
    // The camera stays above the floor and out of the columns.
    let (transform, obstacle) = crate::camera::solid_floor(
        Transform::from_xyz(0.0, -0.5, 0.0).with_scale(Vec3::new(side, 1.0, side)),
    );
    commands.spawn((Plaza, PlazaObstacle, transform, obstacle));
    for column in rules::COLUMNS {
        let radius = column.radius as f32 / 1000.0;
        // The fountain is low and wide; the columns stand tall.
        let height = if radius > 2.0 { 1.2 } else { 6.0 };
        let center = world(column.center) + Vec3::Y * height / 2.0;
        commands.spawn((
            Plaza,
            Mesh3d(meshes.add(Cylinder::new(radius, height))),
            MeshMaterial3d(stone.clone()),
            Transform::from_translation(center),
        ));
        let (transform, obstacle) =
            crate::camera::solid_pillar(Transform::from_translation(center).with_scale(Vec3::new(
                2.0 * radius,
                height,
                2.0 * radius,
            )));
        commands.spawn((Plaza, PlazaObstacle, transform, obstacle));
    }
}

/// The local intent from this frame's controls: the hub takes the movement
/// and heading, and the arena behind it gets none of it (nor casts or
/// jumps). The walk numbers and sends its moves as it steps (`advance`).
pub(crate) fn steer(mut hub: ResMut<Hub>, mut practice: ResMut<crate::Practice>) {
    let Some(walk) = hub.walk.as_mut() else {
        return;
    };
    walk.intent = intent(practice.movement, crate::binary_angle(practice.facing));
    // A jump press waits for the walk's next step.
    walk.jump |= practice.jump;
    practice.movement = Vec2::ZERO;
    practice.cast = None;
    practice.jump = false;
    practice.stop_cast = false;
    // The arena's fighters are not here: nothing to target.
    practice.selected = None;
}

/// Walks the local player and moves everyone else between the door's
/// places.
pub(crate) fn advance(
    mut hub: ResMut<Hub>,
    time: Res<Time>,
    mut avatars: Query<(&HubAvatar, &mut Transform)>,
    mut rebuilt: Query<&mut Visibility, Added<crate::venue::VenueEntity>>,
) {
    // A venue rebuilt meanwhile (an edited map) stays out of the hub.
    if hub.active() {
        for mut visibility in &mut rebuilt {
            *visibility = Visibility::Hidden;
        }
    }
    let dt = time.delta_secs();
    if let Some(walk) = hub.walk.as_mut() {
        // Each step that walks or turns goes to the door, numbered.
        for (seq, step) in walk.advance(dt) {
            crate::platform::hub_intent(
                seq,
                step.movement.x,
                step.movement.z,
                step.facing,
                step.jump,
            );
        }
    }
    for remote in hub.remotes.values_mut() {
        remote.since += dt;
    }
    for (avatar, mut transform) in &mut avatars {
        if let Some(remote) = hub.remotes.get(&avatar.id) {
            transform.translation = remote.from.lerp(remote.to, (remote.since / STEP).min(1.0));
            transform.rotation = Quat::from_rotation_y(remote.facing);
        }
    }
}

/// Gives each avatar a body: a plain stand-in at once (and for looks no
/// installed kit fits), and its kit once the spec is known, as `switch_kit`
/// does for the arena's fighters; the stand-in goes when the kit is shown.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_kits(
    mut commands: Commands,
    desired: Res<crate::appearance::Desired>,
    model: Option<Res<crate::character::CharacterModel>>,
    server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut kit_cache: ResMut<crate::character::KitCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    avatars: Query<(Entity, &HubAvatar, Option<&Children>)>,
    instances: Query<&crate::appearance::KitInstance>,
    stand_ins: Query<(), With<StandIn>>,
) {
    // Legacy single-model builds have no kits to dress others in.
    let kits = model.as_ref().is_none_or(|model| model.kit);
    for (entity, avatar, children) in &avatars {
        let actor = HUB_ACTOR_BASE + avatar.slot;
        let children: Vec<Entity> = children.map(|c| c.iter().collect()).unwrap_or_default();
        let spec = desired.spec_for(actor).filter(|_| kits);
        let wanted = |inst: &crate::appearance::KitInstance| {
            spec.is_some_and(|spec| inst.actor == actor && inst.path == spec.kit.path)
        };
        let mut shown = false;
        let mut loading = false;
        for child in &children {
            if let Ok(inst) = instances.get(*child) {
                if wanted(inst) {
                    shown |= inst.ready;
                    loading |= !inst.ready;
                } else {
                    // A look gone or changed kit: its old body goes.
                    commands.entity(*child).despawn();
                }
            }
        }
        let stand_in: Vec<Entity> = children
            .iter()
            .copied()
            .filter(|child| stand_ins.contains(*child))
            .collect();
        if shown {
            for child in stand_in {
                commands.entity(child).despawn();
            }
            continue;
        }
        if stand_in.is_empty() {
            let body = meshes.add(Capsule3d::new(0.35, 1.1));
            let material = materials.add(StandardMaterial {
                base_color: crate::palette::color(STONE),
                ..default()
            });
            commands.entity(entity).with_children(|parent| {
                parent.spawn((
                    StandIn,
                    Mesh3d(body),
                    MeshMaterial3d(material),
                    Transform::from_xyz(0.0, 0.9, 0.0),
                ));
            });
        }
        if let Some(spec) = spec
            && !loading
        {
            let profile = crate::appearance::kit_profile(&spec.kit);
            let cached = kit_cache.get_or_load(profile, &server, &mut graphs);
            let scene = cached.scene.clone();
            commands.entity(entity).with_children(|parent| {
                parent
                    .spawn((
                        WorldAssetRoot(scene),
                        profile.transform(),
                        Visibility::Hidden,
                        crate::appearance::KitInstance::new_for(actor, profile.path),
                    ))
                    .observe(crate::character::on_ready);
            });
        }
    }
}

/// A plain body for a hub player whose look cannot be shown.
#[derive(Component)]
pub struct StandIn;

/// Where each avatar's name goes on screen, for the page's nameplates:
/// `[[id, x, y], ...]` in viewport fractions, only those on screen.
pub(crate) fn screen(
    hub: Res<Hub>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    avatars: Query<(&HubAvatar, &GlobalTransform)>,
) {
    if !hub.active() {
        return;
    }
    let Ok((camera, view)) = cameras.single() else {
        return;
    };
    let mut json = String::from("[");
    for (avatar, transform) in &avatars {
        if let Some(point) = crate::nameplates::anchor(camera, view, transform.translation()) {
            if json.len() > 1 {
                json.push(',');
            }
            json.push_str(&format!("[{},{:.4},{:.4}]", avatar.id, point.x, point.y));
        }
    }
    json.push(']');
    crate::platform::hub_screen(&json);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_pages_hub_strictly() {
        assert_eq!(parse_view("null"), None);
        let view = parse_view(
            r#"{"x":-5250,"z":24000,"y":0,"vy":0,"mx":0,"mz":0,"tick":7,"ack":3,"players":[{"id":3,"x":10,"z":-20,"facing":16384,"height":400,"admin":true,"spec":{"kit":{}}},{"id":4,"x":0,"z":0,"facing":0,"height":0,"admin":false,"spec":null}]}"#,
        )
        .unwrap();
        assert_eq!((view.x, view.z, view.tick, view.ack), (-5250, 24000, 7, 3));
        assert_eq!(view.players[0].facing, 16384);
        assert!(view.players[0].spec.is_some() && view.players[1].spec.is_none());
        for bad in [
            r#"{"x":0,"z":0,"tick":1,"ack":0}"#,
            r#"{"x":0,"z":0,"tick":1,"players":[]}"#,
            r#"{"x":0.5,"z":0,"y":0,"vy":0,"mx":0,"mz":0,"tick":1,"ack":0,"players":[]}"#,
            r#"{"x":0,"z":0,"y":0,"vy":0,"mx":0,"mz":0,"tick":1,"ack":0,"players":[{"id":3,"x":0,"z":0,"facing":70000,"admin":false}]}"#,
            r#"{"x":0,"z":0,"y":0,"vy":0,"mx":0,"mz":0,"tick":1,"ack":0,"players":[{"id":-1,"x":0,"z":0,"facing":0,"admin":false}]}"#,
            "[1,2]",
        ] {
            assert_eq!(parse_view(bad), None, "{bad}");
        }
        let crowd: Vec<String> = (1..=MAX_HUB_PLAYERS)
            .map(|id| format!(r#"{{"id":{id},"x":0,"z":0,"facing":0,"height":0,"admin":false}}"#))
            .collect();
        let crowd = format!(
            r#"{{"x":0,"z":0,"y":0,"vy":0,"mx":0,"mz":0,"tick":1,"ack":0,"players":[{}]}}"#,
            crowd.join(",")
        );
        assert_eq!(parse_view(&crowd), None, "never more than 39 others");
    }

    #[test]
    fn walks_with_the_doors_rules_and_replays_what_the_door_has_not_applied() {
        let start = rules::spawn_point(0);
        let mut walk = Walk::at(start);
        walk.intent = intent(Vec2::new(1.0, 0.0), 0);
        assert!(walk.advance(STEP * 0.5).is_empty(), "half a tick: no step");
        assert!(walk.shown().abs_diff_eq(world(start), 1e-4));
        let mut sent = walk.advance(STEP * 0.5 + 1e-4);
        sent.extend(walk.advance(STEP));
        sent.extend(walk.advance(STEP));
        assert_eq!(
            sent.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        let mut door = Body::standing(start);
        for _ in 0..3 {
            door = rules::step_body(door, walk.intent);
        }
        assert_eq!(walk.current, door);
        // The door has applied the first move only: replaying the other two
        // lands exactly where the prediction stood, so nothing moves.
        let shown = walk.shown();
        walk.reconcile(rules::step_body(Body::standing(start), walk.intent), 1);
        assert_eq!(walk.current, door);
        assert!(walk.shown().abs_diff_eq(shown, 1e-4));
        // Stopping sends one standing move, then nothing.
        walk.intent = intent(Vec2::ZERO, 0);
        assert_eq!(walk.advance(STEP).len(), 1);
        assert!(walk.advance(STEP * 3.0).is_empty());
        // Turning in place is a move.
        walk.intent = intent(Vec2::ZERO, 9000);
        assert_eq!(walk.advance(STEP).len(), 1);
        // A small disagreement fades out instead of jumping.
        let before = walk.shown();
        walk.reconcile(
            Body::standing(Position {
                x: door.at.x - 500,
                z: door.at.z,
            }),
            u32::MAX,
        );
        assert!(walk.shown().abs_diff_eq(before, 1e-4), "drawn where it was");
        for _ in 0..30 {
            walk.advance(1.0 / 60.0);
        }
        assert!(
            walk.shown().abs_diff_eq(body_world(walk.current), 0.01),
            "faded out"
        );
        // A large one is a jump.
        let far = Position {
            x: start.x,
            z: start.z - 10_000,
        };
        walk.reconcile(Body::standing(far), u32::MAX);
        assert!(walk.shown().abs_diff_eq(world(far), 1e-4));
        // A jump rises, is sent every airborne tick, and lands.
        walk.intent = intent(Vec2::ZERO, 9000);
        walk.jump = true;
        let first = walk.advance(STEP);
        assert!(first[0].1.jump && walk.current.height > 0);
        let mut airborne_moves = 0;
        while walk.current.airborne() {
            airborne_moves += walk.advance(STEP).len();
        }
        assert_eq!(airborne_moves, 7, "one per airborne tick");
        assert!(walk.advance(STEP).is_empty(), "landed and standing");
        // A frame hitch never runs away: at most a second of steps.
        walk.intent = intent(Vec2::new(0.0, -1.0), 0);
        assert_eq!(walk.advance(60.0).len(), 10);
    }

    #[test]
    fn intents_and_angles_follow_the_arena() {
        assert_eq!(
            intent(Vec2::new(0.6, -2.0), 123),
            Intent {
                movement: Axes { x: 600, z: -1000 },
                facing: 123,
                jump: false,
            }
        );
        assert!(intent(Vec2::new(1.0, 1.0), 0).is_valid());
        assert!((yaw(16384) - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert_eq!(crate::binary_angle(yaw(40000)), 40000);
    }
}
