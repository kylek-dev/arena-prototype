//! Bevy presentation adapter for the shared arena rules. DOM integration is
//! isolated in `platform`; rules remain usable in native headless tests.
mod animation;
mod appearance;
mod camera;
mod character;
mod character_animation;
mod combat_text;
mod graphics;
mod heraldry;
mod hover;
mod humanoid;
mod lighting;
mod lightning;
mod nameplates;
mod network;
mod palette;
mod prism;
mod props;
mod sky;
mod startup;
mod textures;
mod venue;
mod weapons;

use arena_map::MapFile;
use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::{
    ATTACK_COOLDOWN, ATTACK_DAMAGE, ATTACK_RANGE, ActorInput, Arena, ArenaEvent, Axes, Combatant,
    Position,
};
use bevy::{asset::AssetMetaCheck, core_pipeline::tonemapping::Tonemapping, prelude::*};

#[derive(Resource)]
struct Practice {
    arena: Arena,
    previous: Vec<Vec3>,
    movement: Vec2,
    facing: f32,
    /// Spawn headings: each unit starts facing the opposing team's spawns.
    facings: Vec<f32>,
    selected: Option<usize>,
    attack: bool,
    jump: bool,
    feedback: &'static str,
    /// The Pulse lightning currently on screen, if any.
    zap: Option<lightning::Zap>,
    frames: u32,
    combat_text: combat_text::CombatText,
    /// In a network match: interpolated authoritative positions to draw,
    /// replacing the local fixed-step interpolation.
    remote: Option<Vec<Vec3>>,
    /// In a network match: whether each local actor's player is attached.
    connected: Option<Vec<bool>>,
}
impl Default for Practice {
    fn default() -> Self {
        Self::new(&arena_map::colosseum(), 1)
    }
}
impl Practice {
    /// A fresh round on `map`, which must already be valid.
    fn new(map: &MapFile, size: usize) -> Self {
        let arena =
            Arena::practice(map.layout.clone(), size).expect("validated map and practice size");
        let facings = spawn_facings(arena.fighters());
        let previous = arena
            .fighters()
            .iter()
            .copied()
            .map(fighter_world)
            .collect();
        Self {
            arena,
            previous,
            movement: Vec2::ZERO,
            facing: facings[0],
            facings,
            selected: None,
            attack: false,
            jump: false,
            feedback: "Select the opponent to begin",
            zap: None,
            frames: 0,
            combat_text: default(),
            remote: None,
            connected: None,
        }
    }
}

/// The network match in progress, if the browser runtime has one.
#[derive(Resource, Default)]
struct Net {
    session: Option<network::Session>,
    /// Seconds since start, read once per frame for the fixed tick.
    now: f64,
    last_status: Option<(f64, String)>,
}
#[derive(Resource)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    distance: f32,
    eye: Option<Vec3>,
}
impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.65,
            distance: 13.0,
            eye: None,
        }
    }
}
/// The map in play. Replacing it rebuilds the venue and starts a fresh round.
#[derive(Resource)]
struct ActiveMap(MapFile);
impl Default for ActiveMap {
    fn default() -> Self {
        Self(arena_map::colosseum())
    }
}
/// The last map revision received from the editor, if any.
#[derive(Resource, Default)]
struct MapEdits {
    revision: Option<f64>,
}
#[derive(Component)]
struct FighterVisual(usize);

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
pub fn run() {
    App::new()
        .insert_resource(practice_window_settings(cfg!(target_arch = "wasm32")))
        .insert_resource(ClearColor(palette::color(palette::NIGHT_6)))
        .insert_resource(GlobalAmbientLight {
            brightness: 500.0,
            ..default()
        })
        .insert_resource(Time::<Fixed>::from_hz(TICKS_PER_SECOND as f64))
        .init_resource::<ActiveMap>()
        .init_resource::<MapEdits>()
        .init_resource::<props::PropModels>()
        .init_resource::<Practice>()
        .init_resource::<Net>()
        .init_resource::<Orbit>()
        .init_resource::<animation::Settings>()
        .init_resource::<appearance::Desired>()
        .init_resource::<appearance::Preview>()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    // These static GLBs use default loader settings, with no .meta sidecars.
                    meta_check: AssetMetaCheck::Never,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Arena Prototype".into(),
                        canvas: Some("#arena".into()),
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: false,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(startup::StartupPlugin)
        .add_systems(Startup, setup)
        .add_systems(
            PreUpdate,
            (
                (
                    graphics::apply,
                    lighting::read_settings.pipe(lighting::apply),
                )
                    .chain(),
                weapons::read_selection.pipe(weapons::apply),
                (
                    appearance::read,
                    network_receive,
                    apply_map_edits,
                    read_controls,
                )
                    .chain(),
            ),
        )
        .add_systems(FixedUpdate, simulate)
        .add_systems(
            Update,
            (
                rebuild_venue.run_if(resource_changed::<ActiveMap>),
                character::check_loading,
                appearance::switch_kit,
                appearance::apply,
                textures::attach_arena_textures,
                sky::attach,
                startup::update,
                present,
                animation::animate,
                character_animation::animate,
                weapons::fit_to_body,
                weapons::animate,
                weapons::animate_staff,
                lightning::tag_hands,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            (
                publish
                    .after(bevy::transform::TransformSystems::Propagate)
                    .after(bevy::camera::CameraUpdateSystems),
                // This frame's animated hand, before visibility is resolved.
                lightning::draw
                    .after(bevy::transform::TransformSystems::Propagate)
                    .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
            ),
        )
        .run();
}

fn practice_window_settings(browser: bool) -> bevy::winit::WinitSettings {
    // The browser HUD can own DOM focus while the arena remains visible.
    // Canvas focus must not select Winit's event-driven 60 Hz low-power mode.
    // Hidden tabs still follow browser rAF throttling; input cancellation is separate.
    if browser {
        bevy::winit::WinitSettings::continuous()
    } else {
        bevy::winit::WinitSettings::game()
    }
}
/// Every unit starts facing the centre of the opposing team. Models look down
/// their local −Z, so a heading `f` faces `(−sin f, −cos f)` in plan.
fn spawn_facings(fighters: &[Combatant]) -> Vec<f32> {
    (0..fighters.len())
        .map(|actor| {
            let opponents: Vec<Vec3> = fighters
                .iter()
                .enumerate()
                .filter(|(other, _)| other % 2 != actor % 2)
                .map(|(_, fighter)| world(fighter.position))
                .collect();
            let centre = opponents.iter().sum::<Vec3>() / opponents.len().max(1) as f32;
            let toward = centre - world(fighters[actor].position);
            (-toward.x).atan2(-toward.z)
        })
        .collect()
}
/// Presentation yaw (radians about +Y, 0 facing −Z) to the rules' binary
/// angle: 65536 units per turn, wrapping.
fn binary_angle(yaw: f32) -> u16 {
    ((yaw / std::f32::consts::TAU).rem_euclid(1.0) * 65536.0).round() as u32 as u16
}
fn world(position: Position) -> Vec3 {
    Vec3::new(position.x as f32 / 1000.0, 0.0, position.z as f32 / 1000.0)
}
fn fighter_world(fighter: Combatant) -> Vec3 {
    world(fighter.position) + Vec3::Y * (fighter.height as f32 / 1000.0)
}
fn material(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.9,
        ..default()
    })
}
#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut images: ResMut<Assets<Image>>,
    practice: Res<Practice>,
) {
    let model_id = platform::character_model();
    // A kit-built player starts from the browser's saved appearance, so the
    // first model loaded is already the right one.
    let kit = (model_id.as_deref() == Some(appearance::KITS_MODEL))
        .then(platform::read_appearance)
        .flatten()
        .and_then(|(_, json)| appearance::Spec::parse(&json));
    let mut character = character::CharacterModel::load(
        match &kit {
            Some(spec) => Some(appearance::kit_profile(&spec.kit)),
            None => model_id.and_then(|id| character::Profile::from_id(&id)),
        },
        &asset_server,
        &mut graphs,
    );
    character.kit = kit.is_some();
    let kit_instance = kit.as_ref().map(|spec| spec.kit.path.clone());
    let player_scene = character.scene.clone();
    let player_transform = character
        .profile
        .map(character::Profile::transform)
        .unwrap_or_default();
    commands.insert_resource(character);
    // The venue itself is spawned (and respawned after edits) from the
    // active map by `rebuild_venue`; its materials and textures load once.
    let venue_assets = venue::VenueAssets::new(&mut meshes, &mut materials, &mut images);
    commands.insert_resource(textures::ArenaTextures::load(
        &asset_server,
        venue_assets.texture_sets(),
    ));
    commands.insert_resource(venue_assets);
    commands.insert_resource(sky::Sky::load(&asset_server));
    let teal = material(&mut materials, palette::color(palette::TEAL_4));
    let orange = material(&mut materials, palette::color(palette::EMBER_4));
    let character_meshes = graphics::CharacterMeshes::new(&mut meshes);
    let painted = textures::PaintedTextures::new(&mut images);
    for index in 0..6 {
        let accent = if index % 2 == 0 {
            teal.clone()
        } else {
            orange.clone()
        };
        let humanoid = humanoid::Humanoid::new(
            humanoid::Appearance::practice(index, accent),
            &mut materials,
            &painted,
        );
        commands
            .spawn((
                FighterVisual(index),
                Transform::default(),
                Visibility::default(),
            ))
            .with_children(|parent| {
                let mut body = parent.spawn((
                    animation::Animator::new(index),
                    Transform::default(),
                    Visibility::default(),
                ));
                if index == 0 {
                    body.insert(appearance::PlayerBody);
                }
                body.with_children(|body| {
                    if index == 0 && weapons::EQUIPPED {
                        body.spawn((
                            weapons::EquipmentCarrier,
                            Transform::default(),
                            Visibility::default(),
                        ));
                    }
                    humanoid.spawn(body, &character_meshes, index);
                    if let Some(scene) = &player_scene
                        && index == 0
                    {
                        let mut root = body.spawn((
                            WorldAssetRoot(scene.clone()),
                            player_transform,
                            Visibility::Hidden,
                        ));
                        if let Some(path) = &kit_instance {
                            root.insert(appearance::KitInstance::new(path));
                        }
                        root.observe(character::on_ready);
                    }
                });
            });
    }
    commands.insert_resource(character_meshes);
    commands.insert_resource(painted);
    lightning::spawn(&mut commands, &mut meshes, &mut materials);
    commands.spawn((
        lighting::ArenaLight::Key,
        DirectionalLight {
            color: palette::color(palette::SAND_8),
            illuminance: 6500.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, -0.55, 0.0)),
    ));
    commands.spawn((
        Name::new("Cool arena fill"),
        lighting::ArenaLight::Fill,
        DirectionalLight {
            color: palette::color(palette::SKY_7),
            illuminance: 3200.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.45, 2.3, 0.0)),
    ));
    let focus = world(practice.arena.fighters()[0].position) + Vec3::Y;
    commands.spawn((
        Camera3d::default(),
        bevy::camera::Hdr,
        Tonemapping::Reinhard,
        bevy::render::view::ColorGrading::default(),
        graphics::arena_exposure(),
        platform::read_graphics().msaa,
        Transform::from_translation(focus + Vec3::new(0.0, 7.8, 10.3)).looking_at(focus, Vec3::Y),
    ));
}
/// Applies a map revision from the browser editor. A valid, different map
/// becomes active (rebuilding the venue); the round continues when every
/// fighter is still walkable and restarts otherwise. Problems are reported
/// back and the current map stays in play.
fn apply_map_edits(
    mut edits: ResMut<MapEdits>,
    mut map: ResMut<ActiveMap>,
    mut practice: ResMut<Practice>,
    mut orbit: ResMut<Orbit>,
    net: Res<Net>,
) {
    // A network match plays on the server's map; edits wait until it ends.
    if net.session.is_some() {
        return;
    }
    let Some((revision, json)) = platform::read_map() else {
        return;
    };
    if edits.revision == Some(revision) {
        return;
    }
    edits.revision = Some(revision);
    let errors = match arena_map::parse(&json) {
        Ok(next) => {
            if next != map.0 {
                // Keep the round (and the camera) while everyone still has
                // somewhere to stand, so editing previews in place.
                if !practice.arena.relayout(next.layout.clone()) {
                    let size = practice.arena.fighters().len() / 2;
                    restart(&mut practice, &mut orbit, &next, size);
                }
                map.0 = next;
            }
            Vec::new()
        }
        Err(errors) => errors,
    };
    platform::map_status(&map_status_json(revision, &errors));
}
fn map_status_json(revision: f64, errors: &[String]) -> String {
    format!(
        r#"{{"revision":{revision},"errors":{}}}"#,
        serde_json::to_string(errors).expect("strings serialize")
    )
}
/// Replaces the venue with one built from the active map.
#[allow(clippy::too_many_arguments)]
fn rebuild_venue(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    assets: Res<venue::VenueAssets>,
    server: Res<AssetServer>,
    mut prop_models: ResMut<props::PropModels>,
    map: Res<ActiveMap>,
    old: Query<Entity, With<venue::VenueEntity>>,
    mut orbit: ResMut<Orbit>,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    venue::spawn(&mut commands, &mut meshes, &assets, &map.0);
    props::spawn(&mut commands, &server, &mut prop_models, &map.0);
    // The previous eye may now be inside new geometry.
    orbit.eye = None;
}
/// Starts a fresh round on `map`, keeping the team size, with the camera
/// behind the player's spawn heading.
fn restart(practice: &mut Practice, orbit: &mut Orbit, map: &MapFile, size: usize) {
    *practice = Practice::new(map, size);
    *orbit = Orbit {
        yaw: practice.facing,
        ..default()
    };
}
fn read_controls(
    mut practice: ResMut<Practice>,
    mut orbit: ResMut<Orbit>,
    map: Res<ActiveMap>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    visuals: Query<(&FighterVisual, &GlobalTransform)>,
    preview: Res<appearance::Preview>,
    net: Res<Net>,
) {
    let mut input = platform::read_input();
    // The server owns the round in a network match: no local reset or
    // practice size change.
    if net.session.is_some() {
        input.reset = false;
        input.team_size = None;
    }
    if preview.open {
        // The character stands still for the close-up; only a reset or mode
        // change from the menu still applies.
        input = Input {
            team_size: input.team_size,
            reset: input.reset,
            ..default()
        };
    }
    if input.reset || input.team_size.is_some() {
        let size = input
            .team_size
            .unwrap_or(practice.arena.fighters().len() / 2);
        restart(&mut practice, &mut orbit, &map.0, size);
    }
    let blockers = hover::pillar_blockers(&map.0);
    orbit.yaw -= input.orbit_x * 0.005;
    orbit.pitch = camera::pitch(orbit.pitch, input.orbit_y * 0.004);
    orbit.distance = camera::zoom(orbit.distance, input.zoom * 0.01);
    update_movement(&mut practice, &orbit, &input);
    update_selection(&mut practice, &input);
    let actor_count = practice.arena.fighters().len();
    let pick = |cursor| {
        cameras.single().ok().and_then(|(camera, view)| {
            camera.viewport_to_world(view, cursor).ok().and_then(|ray| {
                hover::pick(
                    ray,
                    visuals
                        .iter()
                        .filter(|(unit, _)| unit.0 < actor_count)
                        .map(|(unit, transform)| (unit.0, transform.translation())),
                    &blockers,
                )
            })
        })
    };
    if let Some(unit) = update_world_clicks(&mut practice, &input, pick) {
        platform::focus_unit(unit);
    }
    if input.attack {
        practice.attack = true;
    }
    practice.jump |= input.jump;
}
// Both buttons use the same rendered-unit/pillar picker. Focus is browser-owned
// identity state, independent of selection; a right-click miss leaves it alone.
fn update_world_clicks(
    practice: &mut Practice,
    input: &Input,
    pick: impl Fn(Vec2) -> Option<usize>,
) -> Option<usize> {
    if let Some(cursor) = input.click {
        practice.selected = pick(cursor);
        practice.feedback = match practice.selected {
            Some(0) => "Player selected",
            Some(1) => "Opponent selected",
            _ => "Target cleared",
        };
    }
    if input.reset {
        None
    } else {
        input.focus_click.and_then(pick)
    }
}
// Explicit frame intents resolve opponent, player, then clear; canvas picking
// follows them. Selection is presentation intent, never permission to deal damage.
fn update_selection(practice: &mut Practice, input: &Input) {
    if input.select {
        practice.selected = Some(1);
        practice.feedback = "Opponent selected";
    }
    if input.select_player {
        practice.selected = Some(0);
        practice.feedback = "Player selected";
    }
    if let Some(actor) = input
        .select_actor
        .filter(|actor| *actor < practice.arena.fighters().len())
    {
        practice.selected = Some(actor);
    }
    if input.clear {
        practice.selected = None;
        practice.feedback = "Target cleared";
    }
}
fn update_movement(practice: &mut Practice, orbit: &Orbit, input: &Input) {
    if input.steer {
        practice.facing = orbit.yaw;
    }
    // WASD always uses character heading: A/D strafe and S backs up without
    // turning. Camera-only orbit never changes movement, even after stopping.
    let rotation = Quat::from_rotation_y(practice.facing);
    let movement = rotation * Vec3::new(input.x, 0.0, input.z).clamp_length_max(1.0);
    practice.movement = Vec2::new(movement.x, movement.z);
}

fn simulate(mut practice: ResMut<Practice>, mut net: ResMut<Net>) {
    let now = net.now;
    if let Some(session) = net.session.as_mut() {
        // Only intent leaves the client; the server's snapshots come back
        // through `network_receive`.
        let jump = std::mem::take(&mut practice.jump);
        let mut attack = None;
        if std::mem::take(&mut practice.attack) {
            attack = practice.selected;
            if attack.is_none() {
                practice.feedback = "Select a target first";
            }
        }
        let movement = [
            (practice.movement.x * 1000.0) as i16,
            (practice.movement.y * 1000.0) as i16,
        ];
        if let Some(frame) =
            session.input(movement, binary_angle(practice.facing), jump, attack, now)
        {
            platform::send_network(&frame);
        }
        return;
    }
    practice.previous = practice
        .arena
        .fighters()
        .iter()
        .copied()
        .map(fighter_world)
        .collect();
    // Slot 0 is the local player's whole intent for this tick; the other
    // practice actors stay idle.
    let mut input = ActorInput {
        movement: Some(Axes {
            x: (practice.movement.x * 1000.0) as i16,
            z: (practice.movement.y * 1000.0) as i16,
        }),
        facing: Some(binary_angle(practice.facing)),
        jump: std::mem::take(&mut practice.jump),
        attack: None,
    };
    if std::mem::take(&mut practice.attack) {
        if let Some(target) = practice.selected {
            input.attack = Some(target);
        } else {
            practice.feedback = "Select a target first";
        }
    }
    for event in practice.arena.advance(&[input]) {
        match event {
            ArenaEvent::Hit {
                actor,
                target,
                damage,
            } => {
                practice
                    .combat_text
                    .record(actor, target, -i32::from(damage));
                practice.feedback = "Pulse hit · 25 damage";
                practice.zap = Some(lightning::Zap::new(actor, target, practice.arena.tick()));
            }
            ArenaEvent::Finished(_) => {
                practice.feedback = "Round complete · reset to play again";
            }
            // A second press in flight is harmless, not a spell-cast error.
            ArenaEvent::Rejected {
                reason: arena_sim::arena::Rejection::Airborne,
                ..
            } => (),
            ArenaEvent::Rejected { actor: 0, reason } => {
                practice.feedback = rejection_feedback(reason);
            }
            _ => (),
        }
    }
}
/// The action bar's status for the local player (actor 0) attacking
/// `target`.
fn attack_status(arena: &Arena, target: usize) -> &'static str {
    match arena.can_attack(0, target) {
        Ok(()) => "Ready",
        Err(arena_sim::arena::Rejection::OutOfRange) => "Out of range",
        Err(arena_sim::arena::Rejection::Cooldown) => "Recharging",
        Err(arena_sim::arena::Rejection::LineOfSight) => "Line of sight blocked",
        Err(arena_sim::arena::Rejection::InvalidTarget) => "Invalid target",
        // The player is down while teammates may fight on: not the round's end.
        Err(arena_sim::arena::Rejection::InvalidActor) => "You are defeated",
        Err(_) => "Round complete",
    }
}

/// Feedback when a defeated player tries to act while the round goes on.
const DEFEATED_FEEDBACK: &str = "You are defeated · your team fights on";

fn rejection_feedback(reason: arena_sim::arena::Rejection) -> &'static str {
    match reason {
        arena_sim::arena::Rejection::OutOfRange => "Out of range · move closer",
        arena_sim::arena::Rejection::Cooldown => "Pulse is recharging",
        arena_sim::arena::Rejection::LineOfSight => "Line of sight blocked by pillar",
        arena_sim::arena::Rejection::MatchOver => "Round complete · reset to play again",
        // The player's own fighter is down; in a team match the round goes on.
        arena_sim::arena::Rejection::InvalidActor => DEFEATED_FEEDBACK,
        _ => "Command rejected",
    }
}

/// Exchanges frames with the browser's socket and shows the server's state.
/// Without a match from the runtime this does nothing, and leaving one
/// returns to a fresh local practice round.
fn network_receive(
    mut net: ResMut<Net>,
    mut practice: ResMut<Practice>,
    mut orbit: ResMut<Orbit>,
    mut map: ResMut<ActiveMap>,
    time: Res<Time<Real>>,
    mut fixed: ResMut<Time<Fixed>>,
) {
    let now = time.elapsed_secs_f64();
    net.now = now;
    // In a match inputs follow the server's ticks as measured on this
    // clock, paced to keep this player's queue on the server near one input;
    // otherwise it is the plain rules tick.
    let (pace, rate) = net
        .session
        .as_ref()
        .map_or((1.0, TICKS_PER_SECOND as f64), |session| {
            (session.pace(), session.tick_rate())
        });
    let timestep = std::time::Duration::from_secs_f64(pace / rate);
    if fixed.timestep() != timestep {
        fixed.set_timestep(timestep);
    }
    let Some(feed) = platform::read_network() else {
        if net.session.take().is_some() {
            net.last_status = None;
            restart(&mut practice, &mut orbit, &map.0, 1);
            practice.feedback = "Left the match · practice round";
        }
        return;
    };
    if net.session.is_none() {
        // Matches play on the server's map; the colosseum is the only one.
        let colosseum = arena_map::colosseum();
        if map.0 != colosseum {
            map.0 = colosseum;
        }
        practice.feedback = "Connecting to the match…";
    }
    let Net {
        session,
        last_status,
        ..
    } = &mut *net;
    let session = session.get_or_insert_default();
    let rejections = session.receive(&feed, now, &map.0.layout);
    // A new match starts facing where the server placed this player: out of
    // its starting room. The camera starts behind it.
    if let Some(heading) = session.take_initial_heading() {
        practice.facing = heading;
        orbit.yaw = heading;
    }
    if let Some(frame) = session.ping(now) {
        platform::send_network(&frame);
    }
    if session.map_differs() == Some(true) {
        practice.feedback = "This build's map differs from the server's";
    }
    for reason in rejections {
        // A second press in flight is harmless, not a spell-cast error.
        if reason != arena_sim::arena::Rejection::Airborne {
            practice.feedback = rejection_feedback(reason);
        }
    }
    if let Some(mut view) = session.view(time.delta_secs_f64(), now) {
        // The HUD's range and sight checks use where the player sees itself.
        if let Some(predicted) = session.predicted() {
            view.fighters[0] = predicted;
        }
        if practice.arena.tick() != view.tick
            || practice.arena.fighters() != view.fighters.as_slice()
            || practice.arena.result() != view.result
        {
            practice.arena =
                Arena::restore(map.0.layout.clone(), view.tick, view.fighters, view.result)
                    .expect("the session only keeps complete rosters");
        }
        let count = view.positions.len();
        if practice.selected.is_some_and(|actor| actor >= count) {
            practice.selected = None;
        }
        practice.facings.resize(count, 0.0);
        // The player's own heading stays local for responsive turning.
        practice.facings[1..].copy_from_slice(&view.facings[1..]);
        practice.previous = view.positions.clone();
        practice.remote = Some(view.positions);
        practice.connected = Some(view.connected);
        for shown in view.shown {
            match shown {
                network::Shown::Hit {
                    actor,
                    target,
                    damage,
                } => {
                    practice
                        .combat_text
                        .record(actor, target, -i32::from(damage));
                    if actor == 0 {
                        practice.feedback = "Pulse hit · 25 damage";
                    }
                    practice.zap = Some(lightning::Zap::new(actor, target, practice.arena.tick()));
                }
                network::Shown::Finished { won } => {
                    practice.feedback = match won {
                        Some(true) => "Victory · the match closes shortly",
                        Some(false) => "Defeat · the match closes shortly",
                        None => "Draw · the match closes shortly",
                    };
                }
                network::Shown::Jumped { .. } => {}
            }
        }
    }
    // Tell the runtime about phase changes at once, and stats 4 times a second.
    let status = session.status_json();
    let phase_changed = last_status
        .as_ref()
        .is_none_or(|(_, previous)| previous.split(',').next() != status.split(',').next());
    if phase_changed || last_status.as_ref().is_some_and(|(at, _)| now - at >= 0.25) {
        platform::network_status(&status);
        *last_status = Some((now, status));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn present(
    mut practice: ResMut<Practice>,
    mut orbit: ResMut<Orbit>,
    startup: Res<startup::State>,
    preview: Res<appearance::Preview>,
    obstacles: Query<&camera::Obstacle>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut fighters: Query<(&FighterVisual, &mut Transform, &mut Visibility), Without<Camera>>,
    mut cameras: Query<&mut Transform, (With<Camera>, Without<FighterVisual>)>,
    net: Option<Res<Net>>,
) {
    let positions: Vec<Vec3> = match &practice.remote {
        // In a network match the player's own fighter is predicted, drawn
        // part-way through the current fixed tick like local practice.
        Some(remote) => {
            let mut positions = remote.clone();
            let own = net
                .as_ref()
                .and_then(|net| net.session.as_ref())
                .and_then(network::Session::own);
            if let (Some(own), Some(first)) = (own, positions.first_mut()) {
                *first = own.at(fixed.overstep_fraction());
            }
            positions
        }
        None => (0..practice.arena.fighters().len())
            .map(|i| {
                practice.previous[i].lerp(
                    fighter_world(practice.arena.fighters()[i]),
                    fixed.overstep_fraction(),
                )
            })
            .collect(),
    };
    practice.combat_text.advance(time.delta_secs());
    let mut hide_player = false;
    if let Ok(mut camera) = cameras.single_mut()
        && preview.open
        && startup.sweep().is_none()
    {
        *camera = preview_camera(
            positions[0],
            practice.facing,
            &preview,
            &obstacles.iter().copied().collect::<Vec<_>>(),
        );
        // The ordinary boom snaps back from where it was when closed.
        orbit.eye = None;
    } else if let Ok(mut camera) = cameras.single_mut() {
        let focus = positions[0] + Vec3::Y * camera::focus_height(orbit.distance);
        // Behind the loading overlay the camera sweeps around the player so
        // every direction is drawn (and its GPU work done) before play. The
        // eye snaps there and back rather than easing into the first view.
        let sweep = startup.sweep();
        let direction = Quat::from_rotation_y(orbit.yaw + sweep.unwrap_or(0.0))
            * Vec3::new(0.0, orbit.pitch.sin(), orbit.pitch.cos());
        let obstacles: Vec<_> = obstacles.iter().copied().collect();
        // First person sits at the eyes and looks along the boom; there is no
        // boom to collide, and a near-zero offset would give a noisy view.
        let first_person = sweep.is_none() && camera::first_person(orbit.distance);
        let eye = if first_person {
            focus
        } else {
            camera::resolve_eye(
                focus,
                direction,
                orbit.distance,
                orbit.eye.filter(|_| sweep.is_none()),
                time.delta_secs(),
                &obstacles,
            )
        };
        orbit.eye = sweep.is_none().then_some(eye);
        // Never render the player's own model from inside it.
        hide_player = camera::hides_player(focus, eye);
        // Keep a valid view direction even if the focus is inside an obstacle.
        let view = (focus - eye)
            .try_normalize()
            .filter(|_| !first_person)
            .unwrap_or(-direction);
        *camera = Transform::from_translation(eye).looking_to(view, Vec3::Y);
    }
    for (fighter, mut transform, mut visibility) in &mut fighters {
        *visibility = if fighter.0 < positions.len() && !(fighter.0 == 0 && hide_player) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if fighter.0 >= positions.len() {
            continue;
        }
        transform.translation = positions[fighter.0];
        transform.rotation = Quat::from_rotation_y(if fighter.0 == 0 {
            practice.facing
        } else {
            practice.facings[fighter.0]
        });
    }
    practice.frames = practice.frames.saturating_add(1);
}
/// Close-up framing for the customization screen: in front of the player,
/// turned by the preview yaw, with the character left of centre so the
/// options panel on the right does not cover it.
fn preview_camera(
    feet: Vec3,
    facing: f32,
    preview: &appearance::Preview,
    obstacles: &[camera::Obstacle],
) -> Transform {
    let (height, distance) = preview.framing();
    let focus = feet + Vec3::Y * height;
    let forward = Quat::from_rotation_y(facing) * Vec3::NEG_Z;
    let direction = (Quat::from_rotation_y(preview.yaw) * forward + Vec3::Y * 0.08).normalize();
    // Walls still shorten the boom (a starting room is small), but never
    // below arm's length of the face.
    let eye = camera::resolve_eye(focus, direction, distance, None, 0.0, obstacles);
    let eye = if eye.distance(focus) < 0.6 {
        focus + direction * 0.6
    } else {
        eye
    };
    let view = (focus - eye).normalize();
    let right = view.cross(Vec3::Y).normalize_or_zero();
    let aim = focus + right * eye.distance(focus) * 0.22;
    Transform::from_translation(eye).looking_at(aim, Vec3::Y)
}
fn publish(
    practice: Res<Practice>,
    startup: Res<startup::State>,
    map: Res<ActiveMap>,
    orbit: Res<Orbit>,
    character: Res<character::CharacterModel>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    visuals: Query<(&FighterVisual, &GlobalTransform)>,
) {
    let fighters = practice.arena.fighters();
    let player = fighters[0];
    let target = fighters[1];
    let selected_target = fighters[practice.selected.unwrap_or(1)];
    let distance =
        (player.position.distance_squared(selected_target.position) as f64).sqrt() / 1000.0;
    let cooldown =
        player.ready_at.saturating_sub(practice.arena.tick()) as f64 / TICKS_PER_SECOND as f64;
    let attack_status = attack_status(&practice.arena, practice.selected.unwrap_or(1));
    let screen = cameras
        .single()
        .ok()
        .and_then(|(camera, transform)| {
            camera
                .world_to_viewport(transform, world(target.position) + Vec3::Y)
                .ok()
        })
        .unwrap_or(Vec2::splat(-1.0));
    let hovered = platform::read_hover()
        .and_then(|cursor| {
            cameras
                .single()
                .ok()
                .and_then(|(camera, transform)| camera.viewport_to_world(transform, cursor).ok())
        })
        .and_then(|ray| {
            hover::pick(
                ray,
                visuals
                    .iter()
                    .filter(|(unit, _)| unit.0 < fighters.len())
                    .map(|(unit, transform)| (unit.0, transform.translation())),
                &hover::pillar_blockers(&map.0),
            )
        });
    let hovered = hovered
        .map(|actor| format!("\"{}\"", unit_tag(actor)))
        .unwrap_or_else(|| "null".into());
    // The local player is deliberately excluded, independently of selection/focus.
    let plates = cameras
        .single()
        .ok()
        .map(|(camera, view)| {
            visuals
                .iter()
                .filter(|(unit, _)| unit.0 > 0 && unit.0 < fighters.len())
                .filter_map(|(unit, transform)| {
                    nameplates::anchor(camera, view, transform.translation()).map(|point| {
                        format!(
                            r#"{{"tag":"{}","x":{},"y":{}}}"#,
                            unit_tag(unit.0),
                            point.x,
                            point.y
                        )
                    })
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    let combat_text = cameras.single().ok().map(|(camera, view)| {
        practice.combat_text.entries.iter().filter_map(|entry| {
            let (_, transform) = visuals.iter().find(|(unit, _)| unit.0 == entry.target)?;
            let point = nameplates::anchor(camera, view, transform.translation() + Vec3::Y * (0.25 + entry.age * 0.7))?;
            Some(format!(r#"{{"id":{},"source":"Player","target":"{}","delta":{},"x":{},"y":{},"progress":{}}}"#,
                entry.id, unit_tag(entry.target), entry.delta, point.x, point.y, entry.age / 1.2))
        }).collect::<Vec<_>>().join(",")
    }).unwrap_or_default();
    let roster = fighters
        .iter()
        .enumerate()
        .map(|(actor, fighter)| {
            // Network matches say whether each player is attached.
            let connected = practice
                .connected
                .as_ref()
                .and_then(|connected| connected.get(actor))
                .map(|connected| format!(r#","connected":{connected}"#))
                .unwrap_or_default();
            format!(
                r#"{{"actor":{},"tag":"{}","health":{}{}}}"#,
                actor,
                unit_tag(actor),
                fighter.health,
                connected
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let extra = format!(
        r#""startup":{},"teamSize":{},"selectedActor":{},"roster":[{}],"#,
        startup.json(),
        fighters.len() / 2,
        practice
            .selected
            .map(|actor| actor.to_string())
            .unwrap_or_else(|| "null".into()),
        roster
    );
    let json = format!(
        r#"{{"tick":{},"frames":{},"player":{{"x":{},"z":{},"health":{},"yaw":{}}},"target":{{"x":{},"z":{},"health":{},"screenX":{},"screenY":{}}},"selected":{},"selectedPlayer":{},"hoveredUnit":{},"distance":{:.2},"range":{},"cooldown":{:.3},"attackStatus":"{}","feedback":"{}","won":{},"camera":{{"yaw":{},"pitch":{},"distance":{}}},"character":{{"status":"{}","meshes":{},"animated":{}}},"maxHealth":{},"pulse":{{"damage":{},"range":{},"cooldown":{}}},"nameplates":[{}],"combatText":[{}]}}"#,
        practice.arena.tick(),
        practice.frames,
        player.position.x,
        player.position.z,
        player.health,
        practice.facing,
        target.position.x,
        target.position.z,
        target.health,
        screen.x,
        screen.y,
        practice.selected.is_some(),
        practice.selected == Some(0),
        hovered,
        distance,
        ATTACK_RANGE / 1000,
        cooldown,
        attack_status,
        practice.feedback,
        practice.arena.result().is_some(),
        orbit.yaw,
        orbit.pitch,
        orbit.distance,
        character.status(),
        character.meshes,
        true,
        arena_sim::arena::MAX_HEALTH,
        ATTACK_DAMAGE,
        ATTACK_RANGE / 1000,
        ATTACK_COOLDOWN as f64 / TICKS_PER_SECOND as f64,
        plates,
        combat_text
    );
    platform::publish(&format!("{{{}{}", extra, &json[1..]));
}

fn unit_tag(actor: usize) -> String {
    if actor == 0 {
        "Player".into()
    } else if actor.is_multiple_of(2) {
        format!("Team{}", actor / 2)
    } else {
        format!("Opponent{}", actor / 2 + 1)
    }
}

#[derive(Default)]
struct Input {
    team_size: Option<usize>,
    select_actor: Option<usize>,
    x: f32,
    z: f32,
    attack: bool,
    jump: bool,
    select: bool,
    select_player: bool,
    clear: bool,
    reset: bool,
    orbit_x: f32,
    orbit_y: f32,
    zoom: f32,
    click: Option<Vec2>,
    focus_click: Option<Vec2>,
    steer: bool,
}
#[cfg(target_arch = "wasm32")]
mod platform {
    use super::Input;
    use bevy::prelude::Vec2;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen(
        inline_js = "export function arena_character() { const id = globalThis.arenaShell.characterModel; return typeof id === 'string' ? id : undefined; } export function arena_input() { return globalThis.arenaShell.readInput(); } export function arena_hover() { return globalThis.arenaShell.readHover?.() ?? []; } export function arena_graphics() { return globalThis.arenaShell.readGraphics?.() ?? []; } export function arena_weapons() { return globalThis.arenaShell.readWeapons?.() ?? []; } export function arena_lighting() { return globalThis.arenaShell.readLighting?.() ?? []; } export function arena_focus(tag) { globalThis.arenaShell.onFocusUnit?.(tag); } export function arena_state(json) { globalThis.arenaShell.onState(JSON.parse(json)); } export function arena_map() { return globalThis.arenaShell.readMap?.() ?? []; } export function arena_map_status(json) { globalThis.arenaShell.onMapStatus?.(JSON.parse(json)); } export function arena_appearance() { return globalThis.arenaShell.readAppearance?.() ?? []; } export function arena_preview() { return globalThis.arenaShell.readPreview?.() ?? []; } export function arena_network() { return globalThis.arenaShell.readNetwork?.() ?? []; } export function arena_network_send(frame) { globalThis.arenaShell.sendNetwork?.(frame.slice()); } export function arena_network_status(json) { globalThis.arenaShell.onNetwork?.(JSON.parse(json)); }"
    )]
    extern "C" {
        fn arena_character() -> Option<String>;
        fn arena_input() -> js_sys::Array;
        fn arena_hover() -> js_sys::Array;
        fn arena_graphics() -> js_sys::Array;
        fn arena_weapons() -> js_sys::Array;
        fn arena_lighting() -> js_sys::Array;
        fn arena_state(json: &str);
        fn arena_focus(tag: &str);
        fn arena_map() -> js_sys::Array;
        fn arena_map_status(json: &str);
        fn arena_appearance() -> js_sys::Array;
        fn arena_preview() -> js_sys::Array;
        fn arena_network() -> js_sys::Array;
        fn arena_network_send(frame: &[u8]);
        fn arena_network_status(json: &str);
    }
    /// The runtime's match feed as `[generation, ...frames]`; `None` when
    /// there is no match (older shells send nothing).
    pub fn read_network() -> Option<super::network::Feed> {
        let values = arena_network();
        let generation = values
            .get(0)
            .as_f64()
            .filter(|g| g.is_finite() && *g >= 0.0 && *g <= f64::from(u32::MAX))?
            as u32;
        let frames = (1..values.length())
            .filter_map(|i| values.get(i).dyn_into::<js_sys::Uint8Array>().ok())
            .map(|frame| frame.to_vec())
            .collect();
        Some(super::network::Feed { generation, frames })
    }
    pub fn send_network(frame: &[u8]) {
        arena_network_send(frame);
    }
    pub fn network_status(json: &str) {
        arena_network_status(json);
    }
    /// The player's appearance spec as `[revision, json]`; older shells send none.
    pub fn read_appearance() -> Option<(f64, String)> {
        let values = arena_appearance();
        let revision = values.get(0).as_f64().filter(|r| r.is_finite())?;
        Some((revision, values.get(1).as_string()?))
    }
    pub fn read_preview() -> super::appearance::Preview {
        let values = arena_preview();
        super::appearance::Preview::from_values(
            values.get(0).as_f64(),
            values.get(1).as_f64(),
            values.get(2).as_f64(),
        )
    }
    /// The editor's current map as `[revision, json]`; older shells send none.
    pub fn read_map() -> Option<(f64, String)> {
        let values = arena_map();
        let revision = values.get(0).as_f64().filter(|r| r.is_finite())?;
        Some((revision, values.get(1).as_string()?))
    }
    pub fn map_status(json: &str) {
        arena_map_status(json);
    }
    pub fn character_model() -> Option<String> {
        arena_character()
    }
    pub fn read_graphics() -> super::graphics::Settings {
        let values = arena_graphics();
        super::graphics::Settings::from_values(
            values.get(0).as_f64(),
            values.get(1).as_f64(),
            values.get(2).as_f64(),
        )
    }
    pub fn read_weapons() -> super::weapons::Selection {
        let values = arena_weapons();
        super::weapons::Selection::from_values(values.get(0).as_f64(), values.get(1).as_f64())
    }
    pub fn read_lighting() -> super::lighting::Settings {
        let values = arena_lighting();
        super::lighting::Settings::from_values::<{ super::lighting::VALUE_COUNT }>(
            std::array::from_fn(|i| values.get(i as u32).as_f64()),
        )
    }
    pub fn read_hover() -> Option<Vec2> {
        let a = arena_hover();
        let x = a.get(0).as_f64()? as f32;
        let y = a.get(1).as_f64()? as f32;
        (x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0).then_some(Vec2::new(x, y))
    }
    pub fn read_input() -> Input {
        let a = arena_input();
        let number = |i| a.get(i).as_f64().unwrap_or(0.0) as f32;
        Input {
            team_size: a
                .get(18)
                .as_f64()
                .filter(|n| [1.0, 2.0, 3.0].contains(n))
                .map(|n| n as usize),
            select_actor: a
                .get(17)
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0.0 && *n < 6.0 && n.fract() == 0.0)
                .map(|n| n as usize),
            x: number(0),
            z: number(1),
            attack: number(2) != 0.0,
            jump: number(14) == 1.0,
            select: number(3) != 0.0,
            select_player: number(13) == 1.0,
            clear: number(4) != 0.0,
            reset: number(5) != 0.0,
            orbit_x: number(6),
            orbit_y: number(7),
            zoom: number(8),
            steer: number(11) != 0.0,
            click: if number(9) >= 0.0 {
                Some(Vec2::new(number(9), number(10)))
            } else {
                None
            },
            // Older shells have no appended focus-click fields. Missing,
            // negative or nonfinite coordinates mean no click, never (0, 0).
            focus_click: a
                .get(15)
                .as_f64()
                .zip(a.get(16).as_f64())
                .and_then(|(x, y)| super::focus_cursor(x as f32, y as f32)),
        }
    }
    pub fn focus_unit(unit: usize) {
        arena_focus(&super::unit_tag(unit));
    }
    pub fn publish(json: &str) {
        arena_state(json);
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn focus_cursor(x: f32, y: f32) -> Option<Vec2> {
    (x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0).then_some(Vec2::new(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_sim::arena::ArenaCommand;

    #[test]
    fn a_defeated_player_is_told_so_while_the_round_goes_on() {
        // 2v2: the local player (actor 0) is down; everyone else stands.
        let layout = arena_map::colosseum().layout;
        let start = Arena::practice(layout.clone(), 2).unwrap();
        let mut fighters = start.fighters().to_vec();
        fighters[0].health = 0;
        let arena = Arena::restore(layout.clone(), 1, fighters.clone(), None).unwrap();
        assert_eq!(arena.result(), None);
        assert_eq!(attack_status(&arena, 1), "You are defeated");
        assert_eq!(
            rejection_feedback(arena_sim::arena::Rejection::InvalidActor),
            "You are defeated · your team fights on"
        );
        // Once the server decides the round, it is the round that is over.
        let over = Arena::restore(
            layout,
            1,
            fighters,
            Some(arena_sim::arena::RoundResult::Won { team: 1 }),
        )
        .unwrap();
        assert_eq!(attack_status(&over, 1), "Round complete");
    }

    #[test]
    fn browser_hud_focus_does_not_enable_event_driven_low_power_updates() {
        use bevy::winit::UpdateMode;
        let settings = practice_window_settings(true);
        for focused in [true, false] {
            assert!(matches!(
                settings.update_mode(focused),
                UpdateMode::Continuous
            ));
        }
        assert!(matches!(
            practice_window_settings(false).unfocused_mode,
            UpdateMode::Reactive { .. }
        ));
    }

    #[test]
    fn practice_rosters_present_all_active_actors_and_hide_unused_models() {
        let mut app = App::new();
        app.init_resource::<Practice>()
            .init_resource::<Orbit>()
            .init_resource::<appearance::Preview>()
            .init_resource::<Time>()
            .init_resource::<Time<Fixed>>()
            .init_resource::<startup::State>()
            .add_systems(Update, (present, animation::animate).chain())
            .init_resource::<animation::Settings>();
        let visuals: Vec<_> = (0..6)
            .map(|actor| {
                app.world_mut()
                    .spawn((
                        FighterVisual(actor),
                        Transform::default(),
                        Visibility::default(),
                    ))
                    .id()
            })
            .collect();
        for size in [1, 2, 3, 1] {
            *app.world_mut().resource_mut::<Practice>() =
                Practice::new(&arena_map::colosseum(), size);
            for actor in 0..size * 2 {
                update_selection(
                    &mut app.world_mut().resource_mut::<Practice>(),
                    &Input {
                        select_actor: Some(actor),
                        ..default()
                    },
                );
                assert_eq!(app.world().resource::<Practice>().selected, Some(actor));
            }
            update_selection(
                &mut app.world_mut().resource_mut::<Practice>(),
                &Input {
                    select_actor: Some(6),
                    ..default()
                },
            );
            assert_eq!(
                app.world().resource::<Practice>().selected,
                Some(size * 2 - 1)
            );
            app.world_mut().run_schedule(Update);
            for (actor, entity) in visuals.iter().enumerate() {
                assert_eq!(
                    *app.world().get::<Visibility>(*entity).unwrap(),
                    if actor < size * 2 {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    }
                );
                if actor < size * 2 {
                    assert_eq!(
                        app.world().get::<Transform>(*entity).unwrap().translation,
                        fighter_world(app.world().resource::<Practice>().arena.fighters()[actor])
                    );
                }
            }
            assert_eq!(unit_tag(0), "Player");
            for actor in 1..size * 2 {
                assert_eq!(
                    unit_tag(actor),
                    if actor.is_multiple_of(2) {
                        format!("Team{}", actor / 2)
                    } else {
                        format!("Opponent{}", actor / 2 + 1)
                    }
                );
            }
        }
    }

    #[test]
    fn network_matches_show_the_servers_state_from_this_seat_and_send_only_intent() {
        use arena_protocol::{ClientMessage, ServerMessage, Snapshot, Welcome};
        let mut app = App::new();
        app.init_resource::<Practice>()
            .init_resource::<Net>()
            .init_resource::<Orbit>()
            .init_resource::<ActiveMap>()
            .insert_resource(Time::<Real>::default())
            .insert_resource(Time::<Fixed>::from_hz(TICKS_PER_SECOND as f64))
            .add_systems(PreUpdate, network_receive)
            .add_systems(FixedUpdate, simulate);
        let sent = || std::mem::take(&mut *platform::SENT.lock().unwrap());
        sent();
        let feed = |frames| {
            *platform::NETWORK.lock().unwrap() = Some(network::Feed {
                generation: 1,
                frames,
            });
        };
        // The runtime greets the server itself; the engine sends nothing
        // until it is welcomed.
        feed(vec![]);
        app.world_mut().run_schedule(PreUpdate);
        assert!(sent().is_empty());
        // The server's 1v1, seen from seat 1, while seat 0 walks.
        let mut server = Arena::practice(arena_map::colosseum().layout, 1).unwrap();
        let mut frames = vec![
            ServerMessage::Welcome(Welcome {
                protocol: arena_protocol::PROTOCOL_VERSION,
                actor: 1,
                team_size: 1,
                tick: 0,
                tick_rate: 30,
                rules: arena_protocol::RULES_VERSION,
                map: network::fnv1a(arena_map::COLOSSEUM_JSON.as_bytes()),
            })
            .encode(),
        ];
        let mut history = Vec::new();
        for _ in 0..6 {
            server.step(&[ArenaCommand::Move {
                actor: 0,
                x: 0,
                z: -1000,
            }]);
            history.push(server.clone());
            frames.push(
                ServerMessage::Snapshot(Snapshot::capture(&server, 0, vec![], &[false, true]))
                    .encode(),
            );
        }
        feed(frames);
        app.world_mut().run_schedule(PreUpdate);
        {
            let practice = app.world().resource::<Practice>();
            // The view trails the newest tick (6) by three ticks.
            let shown = &history[2];
            assert_eq!(practice.arena.tick(), 3);
            assert_eq!(
                practice.arena.fighters()[0],
                shown.fighters()[1],
                "local 0 is seat 1"
            );
            assert_eq!(practice.arena.fighters()[1], shown.fighters()[0]);
            assert_eq!(practice.connected, Some(vec![true, false]));
            assert_eq!(
                practice.remote.as_ref().unwrap()[1],
                fighter_world(shown.fighters()[0])
            );
        }
        // Attacking the selected enemy sends intent naming server actor 0
        // and never advances the rules locally.
        {
            let mut practice = app.world_mut().resource_mut::<Practice>();
            practice.selected = Some(1);
            practice.attack = true;
        }
        app.world_mut().run_schedule(FixedUpdate);
        let inputs: Vec<_> = sent()
            .iter()
            .filter_map(|frame| match ClientMessage::decode(frame) {
                Ok(ClientMessage::Input(input)) => Some(input),
                _ => None,
            })
            .collect();
        let [input] = inputs[..] else {
            panic!("expected one input, got {inputs:?}");
        };
        assert_eq!((input.seq, input.attack), (1, Some(0)));
        assert_eq!(app.world().resource::<Practice>().arena.tick(), 3);
        assert!(platform::NETWORK_STATUS.lock().unwrap().is_some());
        // When the runtime ends the match, a fresh local practice round starts.
        *platform::NETWORK.lock().unwrap() = None;
        app.world_mut().run_schedule(PreUpdate);
        let practice = app.world().resource::<Practice>();
        assert!(app.world().resource::<Net>().session.is_none());
        assert_eq!(
            (practice.arena.tick(), practice.remote.is_none()),
            (0, true)
        );
    }

    #[test]
    fn simulate_sends_the_player_heading_as_authoritative_facing() {
        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        assert_eq!(binary_angle(0.0), 0);
        assert_eq!(binary_angle(FRAC_PI_2), 0x4000);
        assert_eq!(binary_angle(PI), 0x8000);
        assert_eq!(binary_angle(-FRAC_PI_2), 0xC000);
        assert_eq!(binary_angle(TAU), 0);
        assert_eq!(
            binary_angle(-0.000_001),
            0,
            "rounding wraps, never saturates"
        );
        let mut app = App::new();
        app.init_resource::<Practice>()
            .init_resource::<Net>()
            .add_systems(FixedUpdate, simulate);
        app.world_mut().resource_mut::<Practice>().facing = -FRAC_PI_2;
        app.world_mut().run_schedule(FixedUpdate);
        let practice = app.world().resource::<Practice>();
        assert_eq!(practice.arena.fighters()[0].facing, 0xC000);
        // Opponents keep their rule-owned spawn heading.
        assert_eq!(practice.arena.fighters()[1].facing, 0x8000);
    }

    #[test]
    fn combat_numbers_come_from_successful_hits_not_attempts_and_reset_clears_them() {
        let mut app = App::new();
        app.init_resource::<Practice>()
            .init_resource::<Net>()
            .add_systems(FixedUpdate, simulate);
        {
            let mut practice = app.world_mut().resource_mut::<Practice>();
            practice.selected = Some(1);
            practice.attack = true;
        }
        app.world_mut().run_schedule(FixedUpdate);
        assert!(
            app.world()
                .resource::<Practice>()
                .combat_text
                .entries
                .is_empty()
        );
        {
            let mut practice = app.world_mut().resource_mut::<Practice>();
            let fighters = practice.arena.fighters();
            let gap = fighters[0].position.z - fighters[1].position.z;
            for _ in 0..(gap - ATTACK_RANGE) / arena_sim::arena::MOVE_PER_TICK + 1 {
                practice.arena.step(&[ArenaCommand::Move {
                    actor: 0,
                    x: 0,
                    z: -1000,
                }]);
            }
            practice.attack = true;
        }
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world().resource::<Practice>().combat_text.entries[0].delta,
            -25
        );
        app.world_mut().resource_mut::<Practice>().attack = true;
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world().resource::<Practice>().combat_text.entries.len(),
            1
        );
        *app.world_mut().resource_mut::<Practice>() = Practice::default();
        assert!(
            app.world()
                .resource::<Practice>()
                .combat_text
                .entries
                .is_empty()
        );
    }

    #[test]
    fn right_model_click_focus_is_independent_of_target_and_misses_or_reset_do_nothing() {
        let mut practice = Practice {
            selected: Some(1),
            ..default()
        };
        let mut input = Input {
            focus_click: Some(Vec2::ZERO),
            ..default()
        };
        for unit in [0, 1] {
            assert_eq!(
                update_world_clicks(&mut practice, &input, |_| Some(unit)),
                Some(unit)
            );
            assert_eq!(practice.selected, Some(1));
        }
        assert_eq!(update_world_clicks(&mut practice, &input, |_| None), None);
        assert_eq!(practice.selected, Some(1));
        input.reset = true;
        assert_eq!(
            update_world_clicks(&mut practice, &input, |_| Some(0)),
            None
        );
        input.reset = false;
        input.focus_click = None;
        input.click = Some(Vec2::ZERO);
        assert_eq!(
            update_world_clicks(&mut practice, &input, |_| Some(0)),
            None
        );
        assert_eq!(practice.selected, Some(0));
        update_world_clicks(&mut practice, &input, |_| None);
        assert_eq!(practice.selected, None);
    }

    #[test]
    fn focus_click_coordinates_must_be_finite_and_nonnegative() {
        assert_eq!(focus_cursor(25.0, 40.0), Some(Vec2::new(25.0, 40.0)));
        for (x, y) in [
            (-1.0, 0.0),
            (0.0, -1.0),
            (f32::NAN, 1.0),
            (1.0, f32::INFINITY),
        ] {
            assert_eq!(focus_cursor(x, y), None);
        }
    }

    #[test]
    fn preview_camera_faces_the_character_and_zooms_to_the_face() {
        let feet = Vec3::new(2.0, 0.0, -3.0);
        let mut preview = appearance::Preview::from_values(Some(1.0), Some(0.0), Some(0.0));
        let body = preview_camera(feet, 0.0, &preview, &[]);
        // Facing 0 looks down -Z, so the front view sits on -Z of the player.
        assert!(body.translation.z < feet.z - 3.0);
        assert!(body.forward().z > 0.9);
        preview.shown_zoom = 1.0;
        let face = preview_camera(feet, 0.0, &preview, &[]);
        assert!(face.translation.distance(feet) < body.translation.distance(feet));
        assert!(face.translation.y > body.translation.y);
        // Turning the preview walks the eye around the player.
        preview.yaw = std::f32::consts::PI;
        let back = preview_camera(feet, 0.0, &preview, &[]);
        assert!(back.translation.z > feet.z);
        // A wall right in front still leaves an arm's length to the face.
        let (_, wall) = camera::solid_wall(
            Transform::from_translation(feet + Vec3::new(0.0, 1.0, -0.8))
                .with_scale(Vec3::new(4.0, 4.0, 0.2)),
        );
        preview.yaw = 0.0;
        let close = preview_camera(feet, 0.0, &preview, &[wall]);
        assert!(
            (close
                .translation
                .distance(feet + Vec3::Y * preview.framing().0)
                - 0.6)
                .abs()
                < 1e-3
        );
    }

    #[test]
    fn presentation_retracts_and_recovers_without_changing_requested_zoom() {
        let mut app = App::new();
        app.init_resource::<Practice>()
            .insert_resource(Orbit {
                pitch: 0.0,
                ..default()
            })
            .init_resource::<Time>()
            .init_resource::<Time<Fixed>>()
            .init_resource::<startup::State>()
            .init_resource::<appearance::Preview>()
            .add_systems(Update, present);
        let focus = app.world().resource::<Practice>().previous[0] + Vec3::Y;
        let camera = app
            .world_mut()
            .spawn((Camera::default(), Transform::default()))
            .id();
        let blocker = app
            .world_mut()
            .spawn(camera::solid_box(
                Transform::from_translation(focus + Vec3::Z * 13.0)
                    .with_scale(Vec3::new(3.0, 4.0, 3.0)),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
        app.world_mut().run_schedule(Update);
        let eye = app.world().get::<Transform>(camera).unwrap();
        assert!((eye.translation.distance(focus) - 11.19).abs() < 1e-5);
        assert_eq!(app.world().resource::<Orbit>().distance, 13.0);

        app.world_mut().despawn(blocker);
        app.world_mut().run_schedule(Update);
        let recovering = app
            .world()
            .get::<Transform>(camera)
            .unwrap()
            .translation
            .distance(focus);
        assert!(recovering > 11.19 && recovering < 13.0);
        for _ in 0..180 {
            app.world_mut().run_schedule(Update);
        }
        assert!(
            (app.world()
                .get::<Transform>(camera)
                .unwrap()
                .translation
                .distance(focus)
                - 13.0)
                .abs()
                < 0.001
        );
        assert_eq!(app.world().resource::<Orbit>().distance, 13.0);
    }

    #[test]
    fn first_person_sits_at_eye_level_hides_the_player_and_looks_along_the_boom() {
        let mut app = App::new();
        app.init_resource::<Practice>()
            .insert_resource(Orbit {
                pitch: -0.4,
                distance: camera::zoom(6.0, -10.0),
                ..default()
            })
            .init_resource::<Time>()
            .init_resource::<Time<Fixed>>()
            .init_resource::<startup::State>()
            .init_resource::<appearance::Preview>()
            .add_systems(Update, present);
        let feet = app.world().resource::<Practice>().previous[0];
        let camera = app
            .world_mut()
            .spawn((Camera::default(), Transform::default()))
            .id();
        let player = app
            .world_mut()
            .spawn((
                FighterVisual(0),
                Transform::default(),
                Visibility::Inherited,
            ))
            .id();
        app.world_mut().run_schedule(Update);
        let eye = *app.world().get::<Transform>(camera).unwrap();
        assert!((eye.translation - (feet + Vec3::Y * camera::focus_height(0.0))).length() < 1e-5);
        let direction = Vec3::new(0.0, (-0.4f32).sin(), (-0.4f32).cos());
        assert!(eye.forward().dot(-direction) > 0.9999);
        assert_eq!(
            app.world().get::<Visibility>(player),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn editor_maps_rebuild_the_venue_keep_or_restart_rounds_and_report_problems() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<Image>()
            .init_resource::<ActiveMap>()
            .init_resource::<MapEdits>()
            .init_resource::<props::PropModels>()
            .init_asset::<WorldAsset>()
            .init_resource::<Practice>()
            .init_resource::<Net>()
            .init_resource::<Orbit>()
            .init_resource::<appearance::Preview>()
            .add_systems(
                Update,
                (
                    apply_map_edits,
                    rebuild_venue.run_if(resource_changed::<ActiveMap>),
                )
                    .chain(),
            );
        let assets = {
            let world = app.world_mut();
            let (mut meshes, mut materials, mut images) = (
                world.remove_resource::<Assets<Mesh>>().unwrap(),
                world.remove_resource::<Assets<StandardMaterial>>().unwrap(),
                world.remove_resource::<Assets<Image>>().unwrap(),
            );
            let assets = venue::VenueAssets::new(&mut meshes, &mut materials, &mut images);
            world.insert_resource(meshes);
            world.insert_resource(materials);
            world.insert_resource(images);
            assets
        };
        app.insert_resource(assets);
        app.update();
        let count = |app: &mut App| {
            app.world_mut()
                .query_filtered::<(), With<venue::VenueEntity>>()
                .iter(app.world())
                .count()
        };
        let initial = count(&mut app);
        assert!(initial > 100);
        // A valid edit: one pillar fewer and a wider floor.
        let mut value: serde_json::Value = serde_json::from_str(arena_map::COLOSSEUM_JSON).unwrap();
        value["pillars"].as_array_mut().unwrap().pop();
        value["floor"]["width"] = 120.into();
        value["ground"] = "sand".into();
        value["props"] = serde_json::json!([
            { "model": "Barrel", "x": 3, "z": 4 },
            { "model": "Barrel", "x": -3, "z": 4, "rotation": 45 },
            { "model": "Bench", "x": 0, "z": -6 }
        ]);
        app.world_mut()
            .resource_mut::<Practice>()
            .arena
            .step(&[ArenaCommand::Move {
                actor: 0,
                x: 1000,
                z: 0,
            }]);
        *platform::MAP.lock().unwrap() = Some((1.0, value.to_string()));
        app.update();
        let status = platform::STATUS.lock().unwrap().clone().unwrap();
        assert_eq!(status, r#"{"revision":1,"errors":[]}"#);
        assert_eq!(app.world().resource::<ActiveMap>().0.layout.half_x, 60_000);
        assert_eq!(
            app.world().resource::<ActiveMap>().0.layout.pillars.len(),
            3
        );
        // Everyone can still stand where they are, so the round continues.
        let practice = app.world().resource::<Practice>();
        assert_eq!(practice.arena.tick(), 1);
        assert_eq!(practice.arena.layout().half_x, 60_000);
        let player = practice.arena.fighters()[0].position;
        assert_ne!(player, practice.arena.layout().spawns(0)[0]);
        assert_ne!(count(&mut app), initial);
        let mut props = app
            .world_mut()
            .query_filtered::<&Name, (With<venue::VenueEntity>, With<WorldAssetRoot>)>();
        assert_eq!(props.iter(app.world()).count(), 3);
        // Two barrels share one loaded model.
        assert_eq!(app.world().resource::<props::PropModels>().len(), 2);
        // Sliding the player's starting room away leaves them inside the
        // stands, so the round restarts at the map's spawns.
        assert!(
            player.z > 42_000,
            "the player is still in the starting room"
        );
        value["rooms"][0]["offset"] = 20.into();
        *platform::MAP.lock().unwrap() = Some((1.5, value.to_string()));
        app.update();
        let practice = app.world().resource::<Practice>();
        assert_eq!(practice.arena.tick(), 0);
        assert_eq!(
            practice.arena.fighters()[0].position,
            practice.arena.layout().spawns(0)[0]
        );
        // An invalid edit is reported and leaves the current map in play.
        value["rooms"][0]["ceiling"] = 4.into();
        *platform::MAP.lock().unwrap() = Some((2.0, value.to_string()));
        app.update();
        let status = platform::STATUS.lock().unwrap().clone().unwrap();
        assert!(status.contains(r#""revision":2"#) && status.contains("three character heights"));
        assert_eq!(
            app.world().resource::<ActiveMap>().0.layout.pillars.len(),
            3
        );
        // The same revision is not re-applied every frame.
        *platform::STATUS.lock().unwrap() = None;
        app.update();
        assert!(platform::STATUS.lock().unwrap().is_none());
        *platform::MAP.lock().unwrap() = None;
    }

    #[test]
    fn every_team_size_spawns_both_teams_facing_each_other() {
        for size in 1..=3 {
            let practice = Practice::new(&arena_map::colosseum(), size);
            let fighters = practice.arena.fighters();
            assert_eq!(practice.facing, practice.facings[0]);
            for (actor, fighter) in fighters.iter().enumerate() {
                // Models look down their local −Z.
                let forward = Quat::from_rotation_y(practice.facings[actor]) * Vec3::NEG_Z;
                for (other, opponent) in fighters.iter().enumerate() {
                    if other % 2 != actor % 2 {
                        let toward = world(opponent.position) - world(fighter.position);
                        assert!(forward.dot(toward.normalize()) > 0.8, "{actor}->{other}");
                    }
                }
            }
        }
    }

    #[test]
    fn queued_jump_drains_once_and_presentation_uses_rule_height() {
        let mut app = App::new();
        app.init_resource::<Practice>()
            .init_resource::<Net>()
            .add_systems(FixedUpdate, simulate);
        {
            let mut practice = app.world_mut().resource_mut::<Practice>();
            practice.movement = Vec2::new(0.0, -1.0);
            practice.jump = true;
        }
        app.world_mut().run_schedule(FixedUpdate);
        let practice = app.world().resource::<Practice>();
        assert!(!practice.jump);
        let fighter = practice.arena.fighters()[0];
        assert_eq!(fighter.height, arena_sim::arena::JUMP_SPEED);
        assert_eq!(fighter_world(fighter).y, fighter.height as f32 / 1000.0);
        assert_eq!(practice.previous[0].y, 0.0);
        app.world_mut().resource_mut::<Practice>().movement = Vec2::ZERO;
        app.world_mut().run_schedule(FixedUpdate);
        let practice = app.world().resource::<Practice>();
        assert_eq!(
            practice.arena.fighters()[0].position.z,
            fighter.position.z - arena_sim::arena::MOVE_PER_TICK
        );
        assert!(practice.previous[0].y > 0.0);
    }

    #[test]
    fn frame_selection_supports_self_without_authorizing_self_damage() {
        let mut practice = Practice::default();
        update_selection(
            &mut practice,
            &Input {
                select_player: true,
                ..default()
            },
        );
        assert_eq!(practice.selected, Some(0));
        assert_eq!(
            practice.arena.can_attack(0, 0),
            Err(arena_sim::arena::Rejection::InvalidTarget)
        );
        let before = practice.arena.fighters().to_vec();
        assert!(matches!(
            practice.arena.step(&[ArenaCommand::Attack {
                actor: 0,
                target: 0
            }])[0],
            ArenaEvent::Rejected {
                actor: 0,
                reason: arena_sim::arena::Rejection::InvalidTarget
            }
        ));
        assert_eq!(practice.arena.fighters(), before);
        update_selection(
            &mut practice,
            &Input {
                select: true,
                ..default()
            },
        );
        assert_eq!(practice.selected, Some(1));
        update_selection(
            &mut practice,
            &Input {
                select: true,
                select_player: true,
                clear: true,
                ..default()
            },
        );
        assert_eq!(practice.selected, None);
    }

    #[test]
    fn pulse_presentation_metadata_matches_the_practice_rules() {
        let health = arena_sim::arena::MAX_HEALTH;
        assert_eq!(health.div_ceil(ATTACK_DAMAGE), 4);
        assert_eq!(ATTACK_RANGE / 1000, 6);
        assert_eq!(ATTACK_COOLDOWN as f64 / TICKS_PER_SECOND as f64, 1.0);
    }

    #[test]
    fn releasing_free_look_preserves_walking_heading_and_view() {
        let mut practice = Practice {
            facing: 0.7,
            ..default()
        };
        let mut orbit = Orbit {
            yaw: -1.2,
            ..default()
        };
        let input = Input {
            z: -1.0,
            ..default()
        };
        update_movement(&mut practice, &orbit, &input);
        let movement = practice.movement;
        orbit.yaw = -2.0;
        update_movement(&mut practice, &orbit, &input);
        for _ in 0..3 {
            update_movement(&mut practice, &orbit, &input);
            assert_eq!(practice.facing, 0.7);
            assert_eq!(practice.movement, movement);
            assert_eq!(orbit.yaw, -2.0);
        }
    }

    #[test]
    fn movement_after_camera_orbit_and_stopping_uses_character_heading() {
        let mut practice = Practice::default();
        let orbit = Orbit {
            yaw: 1.0,
            ..default()
        };
        let mut input = Input {
            z: -1.0,
            ..default()
        };
        update_movement(&mut practice, &orbit, &input);
        input.z = 0.0;
        update_movement(&mut practice, &orbit, &input);
        assert_eq!(practice.movement, Vec2::ZERO);
        assert_eq!(practice.facing, 0.0);
        input.z = -1.0;
        update_movement(&mut practice, &orbit, &input);
        assert_eq!(practice.facing, 0.0);
        assert_eq!(practice.movement, Vec2::new(0.0, -1.0));
    }

    #[test]
    fn steering_turns_character_and_subsequent_movement() {
        let mut practice = Practice::default();
        let orbit = Orbit {
            yaw: 1.0,
            ..default()
        };
        let mut input = Input {
            z: -1.0,
            ..default()
        };
        update_movement(&mut practice, &orbit, &input);
        input.steer = true;
        update_movement(&mut practice, &orbit, &input);
        assert_eq!(practice.facing, orbit.yaw);
        input.steer = false;
        update_movement(&mut practice, &orbit, &input);
        assert!((practice.facing - orbit.yaw).abs() < 0.0001);
    }

    #[test]
    fn wasd_strafes_and_backs_up_without_turning_at_any_heading() {
        for facing in [0.0, 0.7, -2.0] {
            for (x, z) in [
                (-1.0, 0.0),
                (1.0, 0.0),
                (0.0, -1.0),
                (0.0, 1.0),
                (1.0, -1.0),
            ] {
                let mut practice = Practice {
                    facing,
                    ..default()
                };
                let orbit = Orbit {
                    yaw: 2.5,
                    ..default()
                };
                let input = Input { x, z, ..default() };
                let expected =
                    Quat::from_rotation_y(facing) * Vec3::new(x, 0.0, z).clamp_length_max(1.0);
                for _ in 0..3 {
                    update_movement(&mut practice, &orbit, &input);
                    assert_eq!(practice.facing, facing);
                    assert_eq!(practice.movement, Vec2::new(expected.x, expected.z));
                    assert!(practice.movement.length() <= 1.0001);
                }
            }
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
mod platform {
    pub fn read_lighting() -> super::lighting::Settings {
        super::lighting::Settings::from_values([None; super::lighting::VALUE_COUNT])
    }
    pub fn read_weapons() -> super::weapons::Selection {
        super::weapons::Selection::from_values(None, None)
    }
    pub fn read_graphics() -> super::graphics::Settings {
        // Exercise the same defaults as an older browser shell with no callback.
        super::graphics::Settings::from_values(None, None, None)
    }
    pub fn read_hover() -> Option<bevy::prelude::Vec2> {
        None
    }
    pub fn character_model() -> Option<String> {
        None
    }
    pub fn read_appearance() -> Option<(f64, String)> {
        None
    }
    pub fn read_preview() -> super::appearance::Preview {
        PREVIEW.lock().unwrap().unwrap_or_default()
    }
    pub static PREVIEW: std::sync::Mutex<Option<super::appearance::Preview>> =
        std::sync::Mutex::new(None);
    pub fn read_input() -> super::Input {
        super::Input::default()
    }
    pub fn publish(_: &str) {}
    pub fn focus_unit(_: usize) {}
    /// Tests stand in for the browser's socket through these: the feed's
    /// frames are consumed when read.
    pub static NETWORK: std::sync::Mutex<Option<super::network::Feed>> =
        std::sync::Mutex::new(None);
    pub static SENT: std::sync::Mutex<Vec<Vec<u8>>> = std::sync::Mutex::new(Vec::new());
    pub static NETWORK_STATUS: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    pub fn read_network() -> Option<super::network::Feed> {
        NETWORK
            .lock()
            .unwrap()
            .as_mut()
            .map(|feed| super::network::Feed {
                generation: feed.generation,
                frames: std::mem::take(&mut feed.frames),
            })
    }
    pub fn send_network(frame: &[u8]) {
        SENT.lock().unwrap().push(frame.to_vec());
    }
    pub fn network_status(json: &str) {
        *NETWORK_STATUS.lock().unwrap() = Some(json.to_owned());
    }
    pub fn read_map() -> Option<(f64, String)> {
        MAP.lock().unwrap().clone()
    }
    pub fn map_status(json: &str) {
        *STATUS.lock().unwrap() = Some(json.to_owned());
    }
    /// Tests stand in for the browser editor through these. Systems run on
    /// worker threads, so these are process-wide rather than thread-local.
    pub static MAP: std::sync::Mutex<Option<(f64, String)>> = std::sync::Mutex::new(None);
    pub static STATUS: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
}
