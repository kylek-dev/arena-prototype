//! The social hub in the renderer ([ADR 0056](../../../docs/decisions/0056-social-hub.md)):
//! the graybox plaza, the local player walking it (predicted with the
//! door's own `arena_sim::hub` rules), and everyone else in view with their
//! chosen look, interpolated between the door's 10 Hz states. The page owns
//! the connection: it hands over the hub as JSON (`readHub`) and takes the
//! local intent (`onHubIntent`) and where each name goes on screen
//! (`onHubScreen`).

use std::collections::HashMap;

use arena_sim::arena::{AXIS_LIMIT, Axes, Position};
use arena_sim::hub::{self as rules, HUB_TICKS_PER_SECOND, Intent, MAX_HUB_PLAYERS};
use bevy::prelude::*;

/// Kit instances for hub players use actors from here up, one per slot, so
/// they never collide with the arena's fighters (`0..MAX_FIGHTERS`).
pub const HUB_ACTOR_BASE: usize = 1000;
/// Seconds per hub tick.
const STEP: f32 = 1.0 / HUB_TICKS_PER_SECOND as f32;
/// Further than this from the door's place, the local player snaps to it.
const SNAP_METRES: f32 = 3.0;

/// One player in view, as the page described them.
#[derive(Clone, Debug, PartialEq)]
pub struct Seen {
    pub id: u32,
    pub x: i32,
    pub z: i32,
    pub facing: u16,
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
    pub tick: u64,
    /// Whether the door has applied every intent the page sent, so its
    /// place for this player is not behind them.
    pub settled: bool,
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
            admin: p.get("admin")?.as_bool()?,
            spec: p.get("spec").filter(|spec| spec.is_object()).cloned(),
        });
    }
    Some(View {
        x: int(object.get("x")?)?,
        z: int(object.get("z")?)?,
        tick: object.get("tick")?.as_u64()?,
        // Older pages send no flag: treat their places as settled.
        settled: object
            .get("settled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
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

/// The local player's walk: predicted a hub tick at a time with the door's
/// rules, drawn between the last two steps, and pulled to the door's place
/// when it drifts too far (or settles there once standing).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walk {
    pub previous: Position,
    pub current: Position,
    pub intent: Intent,
    /// Seconds into the current hub tick.
    pub elapsed: f32,
}

impl Walk {
    pub fn at(start: Position) -> Self {
        Self {
            previous: start,
            current: start,
            intent: Intent::default(),
            elapsed: 0.0,
        }
    }

    /// Advances by `dt` seconds of the current intent.
    pub fn advance(&mut self, dt: f32) {
        self.elapsed += dt.clamp(0.0, 1.0);
        while self.elapsed >= STEP {
            self.elapsed -= STEP;
            self.previous = self.current;
            self.current = rules::step(self.current, self.intent);
        }
    }

    /// Where to draw the player now.
    pub fn shown(&self) -> Vec3 {
        world(self.previous).lerp(world(self.current), self.elapsed / STEP)
    }

    /// The door's place for this player arrived. Moving players lead the
    /// door by their round trip, so only a large gap snaps; a standing one
    /// settles where the door put them once the door has applied every
    /// intent sent (`settled`), so stopping never pulls them back.
    pub fn correct(&mut self, door: Position, settled: bool) {
        let standing = self.intent.movement == Axes::default();
        let gap = world(self.current).distance(world(door));
        if gap > SNAP_METRES || (standing && settled && gap > 0.0) {
            self.previous = door;
            self.current = door;
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
    /// The intent last handed to the page.
    sent: Option<Intent>,
    remotes: HashMap<u32, Remote>,
}

impl Hub {
    pub fn active(&self) -> bool {
        self.walk.is_some()
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
    let door = Position {
        x: view.x,
        z: view.z,
    };
    if !hub.active() {
        // Entering: the arena's venue gives way to the plaza.
        for mut visibility in &mut venue {
            *visibility = Visibility::Hidden;
        }
        spawn_plaza(&mut commands, &mut meshes, &mut materials);
        hub.walk = Some(Walk::at(rules::settle(door)));
        hub.sent = None;
        if let Some(orbit) = orbit.as_mut() {
            orbit.eye = None;
        }
    } else if view.tick != hub.tick
        && let Some(walk) = hub.walk.as_mut()
    {
        walk.correct(door, view.settled);
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
        });
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
    hub.sent = None;
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
/// jumps). Hands the intent to the page when it changes.
pub(crate) fn steer(mut hub: ResMut<Hub>, mut practice: ResMut<crate::Practice>) {
    let Some(walk) = hub.walk.as_mut() else {
        return;
    };
    let next = intent(practice.movement, crate::binary_angle(practice.facing));
    walk.intent = next;
    practice.movement = Vec2::ZERO;
    practice.cast = None;
    practice.jump = false;
    practice.stop_cast = false;
    // The arena's fighters are not here: nothing to target.
    practice.selected = None;
    if hub.sent != Some(next) {
        hub.sent = Some(next);
        crate::platform::hub_intent(next.movement.x, next.movement.z, next.facing);
    }
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
        walk.advance(dt);
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
            r#"{"x":-5250,"z":24000,"tick":7,"settled":false,"players":[{"id":3,"x":10,"z":-20,"facing":16384,"admin":true,"spec":{"kit":{}}},{"id":4,"x":0,"z":0,"facing":0,"admin":false,"spec":null}]}"#,
        )
        .unwrap();
        assert_eq!(
            (view.x, view.z, view.tick, view.settled),
            (-5250, 24000, 7, false)
        );
        assert_eq!(view.players[0].facing, 16384);
        assert!(view.players[0].spec.is_some() && view.players[1].spec.is_none());
        for bad in [
            r#"{"x":0,"z":0,"tick":1}"#,
            r#"{"x":0.5,"z":0,"tick":1,"players":[]}"#,
            r#"{"x":0,"z":0,"tick":1,"players":[{"id":3,"x":0,"z":0,"facing":70000,"admin":false}]}"#,
            r#"{"x":0,"z":0,"tick":1,"players":[{"id":-1,"x":0,"z":0,"facing":0,"admin":false}]}"#,
            "[1,2]",
        ] {
            assert_eq!(parse_view(bad), None, "{bad}");
        }
        let crowd: Vec<String> = (1..=MAX_HUB_PLAYERS)
            .map(|id| format!(r#"{{"id":{id},"x":0,"z":0,"facing":0,"admin":false}}"#))
            .collect();
        let crowd = format!(
            r#"{{"x":0,"z":0,"tick":1,"players":[{}]}}"#,
            crowd.join(",")
        );
        assert_eq!(parse_view(&crowd), None, "never more than 39 others");
    }

    #[test]
    fn walks_with_the_doors_rules_and_settles_where_the_door_says() {
        let start = rules::spawn_point(0);
        let mut walk = Walk::at(start);
        walk.intent = intent(Vec2::new(1.0, 0.0), 0);
        walk.advance(STEP * 0.5);
        assert_eq!(walk.current, start, "half a tick: not stepped yet");
        assert!(walk.shown().abs_diff_eq(world(start), 1e-4));
        walk.advance(STEP * 0.5 + 1e-4);
        assert_eq!(walk.current, rules::step(start, walk.intent));
        // A small lead while moving is kept; a large gap snaps.
        let lead = walk.current;
        walk.correct(start, true);
        assert_eq!(walk.current, lead);
        walk.correct(
            Position {
                x: start.x + 10_000,
                z: start.z,
            },
            false,
        );
        assert_eq!(walk.current.x, start.x + 10_000);
        // Just stopped: the door's place still lags behind; no pull back
        // until it has applied every intent.
        walk.intent = Intent::default();
        let stopped = walk.current;
        let behind = Position {
            x: stopped.x - 1_000,
            z: stopped.z,
        };
        walk.correct(behind, false);
        assert_eq!(walk.current, stopped);
        walk.correct(behind, true);
        assert_eq!((walk.previous, walk.current), (behind, behind));
        // A frame hitch never runs away.
        walk.intent = intent(Vec2::new(0.0, -1.0), 0);
        walk.advance(60.0);
        assert!(
            behind.distance_squared(walk.current)
                <= i64::from(rules::HUB_MOVE_PER_TICK * 10).pow(2)
        );
    }

    #[test]
    fn intents_and_angles_follow_the_arena() {
        assert_eq!(
            intent(Vec2::new(0.6, -2.0), 123),
            Intent {
                movement: Axes { x: 600, z: -1000 },
                facing: 123
            }
        );
        assert!(intent(Vec2::new(1.0, 1.0), 0).is_valid());
        assert!((yaw(16384) - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert_eq!(crate::binary_angle(yaw(40000)), 40000);
    }
}
