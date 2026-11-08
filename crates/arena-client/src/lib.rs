//! Bevy presentation adapter for the shared arena rules. DOM integration is
//! isolated in `platform`; rules remain usable in native headless tests.
mod animation;
mod camera;
mod character;
mod character_animation;
mod combat_text;
mod graphics;
mod hover;
mod humanoid;
mod lighting;
mod nameplates;
mod weapons;

use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::{
    ARENA_X, ARENA_Z, ATTACK_COOLDOWN, ATTACK_DAMAGE, ATTACK_RANGE, Arena, ArenaCommand,
    ArenaEvent, Combatant, PILLARS, Position,
};
use bevy::{asset::AssetMetaCheck, core_pipeline::tonemapping::Tonemapping, prelude::*};

#[derive(Resource)]
struct Practice {
    arena: Arena,
    previous: [Vec3; 2],
    movement: Vec2,
    facing: f32,
    selected: Option<usize>,
    attack: bool,
    jump: bool,
    feedback: &'static str,
    flash: f32,
    frames: u32,
    combat_text: combat_text::CombatText,
}
impl Default for Practice {
    fn default() -> Self {
        let arena = Arena::default();
        let previous = arena.fighters().map(fighter_world);
        Self {
            arena,
            previous,
            movement: Vec2::ZERO,
            facing: 0.0,
            selected: None,
            attack: false,
            jump: false,
            feedback: "Select the opponent to begin",
            flash: 0.0,
            frames: 0,
            combat_text: default(),
        }
    }
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
#[derive(Component)]
struct FighterVisual(usize);
#[derive(Component)]
struct TargetRing;
#[derive(Component)]
struct AttackBeam;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
pub fn run() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.18, 0.21, 0.25)))
        .insert_resource(GlobalAmbientLight {
            brightness: 500.0,
            ..default()
        })
        .insert_resource(Time::<Fixed>::from_hz(TICKS_PER_SECOND as f64))
        .init_resource::<Practice>()
        .init_resource::<Orbit>()
        .init_resource::<animation::Settings>()
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
                read_controls,
            ),
        )
        .add_systems(FixedUpdate, simulate)
        .add_systems(
            Update,
            (
                character::check_loading,
                present,
                animation::animate,
                character_animation::animate,
                weapons::animate,
                weapons::animate_staff,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            publish
                .after(bevy::transform::TransformSystems::Propagate)
                .after(bevy::camera::CameraUpdateSystems),
        )
        .run();
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
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let character = character::CharacterModel::load(
        platform::has_character_model(),
        &asset_server,
        &mut graphs,
    );
    let player_scene = character.scene.clone();
    commands.insert_resource(character);
    let stone = material(&mut materials, Color::srgb(0.28, 0.34, 0.38));
    let edge = material(&mut materials, Color::srgb(0.13, 0.19, 0.23));
    let dark = material(&mut materials, Color::srgb(0.06, 0.10, 0.14));
    let teal = material(&mut materials, Color::srgb(0.15, 0.84, 0.75));
    let orange = material(&mut materials, Color::srgb(1.0, 0.39, 0.19));
    let gold = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.72, 0.32),
        unlit: true,
        ..default()
    });
    let line = material(&mut materials, Color::srgb(0.37, 0.44, 0.46));
    let cube = meshes.add(Cuboid::from_size(Vec3::ONE));
    let half_x = ARENA_X as f32 / 1000.0;
    let half_z = ARENA_Z as f32 / 1000.0;
    let width = half_x * 2.0;
    let depth = half_z * 2.0;
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(dark.clone()),
        camera::solid_floor(Transform::from_xyz(0.0, -0.5, 0.0).with_scale(Vec3::new(
            width + 2.0,
            0.8,
            depth + 2.0,
        ))),
    ));
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(stone.clone()),
        camera::solid_floor(
            Transform::from_xyz(0.0, -0.08, 0.0).with_scale(Vec3::new(width, 0.16, depth)),
        ),
    ));
    // Thin seams make camera movement and scale readable without external textures.
    for x in (-ARENA_X + 2000..ARENA_X).step_by(2000) {
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(edge.clone()),
            Transform::from_xyz(x as f32 / 1000.0, 0.006, 0.0)
                .with_scale(Vec3::new(0.025, 0.01, depth)),
        ));
    }
    for z in (-ARENA_Z + 1000..ARENA_Z).step_by(2000) {
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(edge.clone()),
            Transform::from_xyz(0.0, 0.007, z as f32 / 1000.0)
                .with_scale(Vec3::new(width, 0.01, 0.025)),
        ));
    }
    for (x, z, sx, sz) in [
        (0.0, -half_z - 0.2, width + 0.8, 0.4),
        (0.0, half_z + 0.2, width + 0.8, 0.4),
        (-half_x - 0.2, 0.0, 0.4, depth),
        (half_x + 0.2, 0.0, 0.4, depth),
    ] {
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(edge.clone()),
            camera::solid_box(Transform::from_xyz(x, 0.25, z).with_scale(Vec3::new(sx, 0.5, sz))),
        ));
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(line.clone()),
            camera::solid_box(Transform::from_xyz(x, 0.51, z).with_scale(Vec3::new(sx, 0.025, sz))),
        ));
    }
    for pillar in PILLARS {
        let p = world(pillar.center);
        let size = pillar.half_size as f32 / 1000.0 * 2.0;
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(edge.clone()),
            camera::solid_box(
                Transform::from_translation(p + Vec3::Y * 0.12).with_scale(Vec3::new(
                    size + 0.35,
                    0.24,
                    size + 0.35,
                )),
            ),
        ));
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(stone.clone()),
            camera::solid_box(
                Transform::from_translation(p + Vec3::Y * 2.0)
                    .with_scale(Vec3::new(size, 4.0, size)),
            ),
        ));
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(line.clone()),
            camera::solid_box(
                Transform::from_translation(p + Vec3::Y * 4.03).with_scale(Vec3::new(
                    size + 0.14,
                    0.14,
                    size + 0.14,
                )),
            ),
        ));
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(teal.clone()),
            camera::solid_box(
                Transform::from_translation(p + Vec3::new(0.0, 3.3, size / 2.0 + 0.006))
                    .with_scale(Vec3::new(0.55, 0.06, 0.02)),
            ),
        ));
    }
    let circle = meshes.add(Circle::new(0.65));
    let character_meshes = graphics::CharacterMeshes::new(&mut meshes);
    for (index, accent) in [(0, teal.clone()), (1, orange.clone())] {
        let humanoid = humanoid::Humanoid::new(
            humanoid::Appearance::practice(index, accent),
            &mut materials,
        );
        commands
            .spawn((
                FighterVisual(index),
                Transform::default(),
                Visibility::default(),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(circle.clone()),
                    MeshMaterial3d(dark.clone()),
                    Transform::from_xyz(0.0, 0.015, 0.0)
                        .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                ));
                parent
                    .spawn((
                        animation::Animator::new(index),
                        Transform::default(),
                        Visibility::default(),
                    ))
                    .with_children(|body| {
                        if index == 0 {
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
                            body.spawn((
                                WorldAssetRoot(scene.clone()),
                                character::model_transform(),
                                Visibility::Hidden,
                            ))
                            .observe(character::on_ready);
                        }
                    });
            });
    }
    commands.insert_resource(character_meshes);
    commands.spawn((
        TargetRing,
        Mesh3d(meshes.add(Annulus::new(0.72, 0.80))),
        MeshMaterial3d(gold.clone()),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        Visibility::Hidden,
    ));
    commands.spawn((
        AttackBeam,
        Mesh3d(cube),
        MeshMaterial3d(gold),
        Transform::default(),
        Visibility::Hidden,
    ));
    commands.spawn((
        lighting::ArenaLight::Key,
        DirectionalLight {
            color: Color::srgb(1.0, 0.95, 0.88),
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
            color: Color::srgb(0.88, 0.93, 1.0),
            illuminance: 3200.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.45, 2.3, 0.0)),
    ));
    let focus = world(Arena::default().fighters()[0].position) + Vec3::Y;
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
fn read_controls(
    mut practice: ResMut<Practice>,
    mut orbit: ResMut<Orbit>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    visuals: Query<(&FighterVisual, &GlobalTransform)>,
) {
    let input = platform::read_input();
    if input.reset {
        *practice = Practice::default();
        *orbit = Orbit::default();
    }
    orbit.yaw -= input.orbit_x * 0.005;
    orbit.pitch = (orbit.pitch + input.orbit_y * 0.004).clamp(-1.2, 1.2);
    orbit.distance = (orbit.distance + input.zoom * 0.01).clamp(6.0, 22.0);
    update_movement(&mut practice, &orbit, &input);
    update_selection(&mut practice, &input);
    let pick = |cursor| {
        cameras.single().ok().and_then(|(camera, view)| {
            camera.viewport_to_world(view, cursor).ok().and_then(|ray| {
                hover::pick(
                    ray,
                    visuals
                        .iter()
                        .map(|(unit, transform)| (unit.0, transform.translation())),
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

fn simulate(mut practice: ResMut<Practice>) {
    practice.previous = practice.arena.fighters().map(fighter_world);
    let mut commands = vec![ArenaCommand::Move {
        actor: 0,
        x: (practice.movement.x * 1000.0) as i16,
        z: (practice.movement.y * 1000.0) as i16,
    }];
    if practice.jump {
        practice.jump = false;
        commands.push(ArenaCommand::Jump { actor: 0 });
    }
    if practice.attack {
        practice.attack = false;
        if let Some(target) = practice.selected {
            commands.push(ArenaCommand::Attack { actor: 0, target });
        } else {
            practice.feedback = "Select a target first";
        }
    }
    for event in practice.arena.step(&commands) {
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
                practice.flash = 0.18;
            }
            ArenaEvent::Won { .. } => practice.feedback = "Round complete · reset to play again",
            // A second press in flight is harmless, not a spell-cast error.
            ArenaEvent::Rejected(arena_sim::arena::Rejection::Airborne) => (),
            ArenaEvent::Rejected(reason) => {
                practice.feedback = match reason {
                    arena_sim::arena::Rejection::OutOfRange => "Out of range · move closer",
                    arena_sim::arena::Rejection::Cooldown => "Pulse is recharging",
                    arena_sim::arena::Rejection::LineOfSight => "Line of sight blocked by pillar",
                    arena_sim::arena::Rejection::MatchOver => {
                        "Round complete · reset to play again"
                    }
                    _ => "Command rejected",
                }
            }
            _ => (),
        }
    }
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn present(
    mut practice: ResMut<Practice>,
    mut orbit: ResMut<Orbit>,
    obstacles: Query<&camera::Obstacle>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut fighters: Query<
        (&FighterVisual, &mut Transform),
        (Without<Camera>, Without<TargetRing>, Without<AttackBeam>),
    >,
    mut cameras: Query<
        &mut Transform,
        (
            With<Camera>,
            Without<FighterVisual>,
            Without<TargetRing>,
            Without<AttackBeam>,
        ),
    >,
    mut ring: Query<
        (&mut Transform, &mut Visibility),
        (
            With<TargetRing>,
            Without<FighterVisual>,
            Without<Camera>,
            Without<AttackBeam>,
        ),
    >,
    mut beam: Query<
        (&mut Transform, &mut Visibility),
        (
            With<AttackBeam>,
            Without<FighterVisual>,
            Without<Camera>,
            Without<TargetRing>,
        ),
    >,
) {
    let positions: [Vec3; 2] = std::array::from_fn(|i| {
        practice.previous[i].lerp(
            fighter_world(practice.arena.fighters()[i]),
            fixed.overstep_fraction(),
        )
    });
    practice.combat_text.advance(time.delta_secs());
    for (fighter, mut transform) in &mut fighters {
        transform.translation = positions[fighter.0];
        if fighter.0 == 0 {
            transform.rotation = Quat::from_rotation_y(practice.facing);
        }
    }
    if let Ok(mut camera) = cameras.single_mut() {
        let focus = positions[0] + Vec3::Y;
        let direction =
            Quat::from_rotation_y(orbit.yaw) * Vec3::new(0.0, orbit.pitch.sin(), orbit.pitch.cos());
        let obstacles: Vec<_> = obstacles.iter().copied().collect();
        let eye = camera::resolve_eye(
            focus,
            direction,
            orbit.distance,
            orbit.eye,
            time.delta_secs(),
            &obstacles,
        );
        orbit.eye = Some(eye);
        // Keep a valid view direction even if the focus is inside an obstacle.
        let view = (focus - eye).try_normalize().unwrap_or(-direction);
        *camera = Transform::from_translation(eye).looking_to(view, Vec3::Y);
    }
    if let Ok((mut transform, mut visibility)) = ring.single_mut() {
        transform.translation = positions[practice.selected.unwrap_or(1)] + Vec3::Y * 0.03;
        *visibility = if practice.selected.is_some() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    practice.flash = (practice.flash - time.delta_secs()).max(0.0);
    if let Ok((mut transform, mut visibility)) = beam.single_mut() {
        let from = positions[0] + Vec3::Y;
        let to = positions[1] + Vec3::Y;
        *transform = Transform::from_translation((from + to) * 0.5)
            .looking_at(to, Vec3::Y)
            .with_scale(Vec3::new(0.065, 0.065, from.distance(to)));
        *visibility = if practice.flash > 0.0 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    practice.frames = practice.frames.saturating_add(1);
}
fn publish(
    practice: Res<Practice>,
    orbit: Res<Orbit>,
    character: Res<character::CharacterModel>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    visuals: Query<(&FighterVisual, &GlobalTransform)>,
) {
    let fighters = practice.arena.fighters();
    let player = fighters[0];
    let target = fighters[1];
    let distance = (player.position.distance_squared(target.position) as f64).sqrt() / 1000.0;
    let cooldown =
        player.ready_at.saturating_sub(practice.arena.tick()) as f64 / TICKS_PER_SECOND as f64;
    let attack_status = match practice.arena.can_attack(0, practice.selected.unwrap_or(1)) {
        Ok(()) => "Ready",
        Err(arena_sim::arena::Rejection::OutOfRange) => "Out of range",
        Err(arena_sim::arena::Rejection::Cooldown) => "Recharging",
        Err(arena_sim::arena::Rejection::LineOfSight) => "Line of sight blocked",
        Err(arena_sim::arena::Rejection::InvalidTarget) => "Invalid target",
        _ => "Round complete",
    };
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
                    .map(|(unit, transform)| (unit.0, transform.translation())),
            )
        });
    let hovered = match hovered {
        Some(0) => "\"Player\"",
        Some(1) => "\"Opponent1\"",
        _ => "null",
    };
    // The local player is deliberately excluded. Team anchors can be appended
    // when real teammates exist; selection/focus do not control plate visibility.
    let plates = cameras
        .single()
        .ok()
        .and_then(|(camera, view)| {
            visuals
                .iter()
                .find(|(unit, _)| unit.0 == 1)
                .and_then(|(_, transform)| {
                    nameplates::anchor(camera, view, transform.translation())
                })
        })
        .map(|point| format!(r#"{{"tag":"Opponent1","x":{},"y":{}}}"#, point.x, point.y))
        .unwrap_or_default();
    let combat_text = cameras.single().ok().map(|(camera, view)| {
        practice.combat_text.entries.iter().filter_map(|entry| {
            let (_, transform) = visuals.iter().find(|(unit, _)| unit.0 == entry.target)?;
            let point = nameplates::anchor(camera, view, transform.translation() + Vec3::Y * (0.25 + entry.age * 0.7))?;
            Some(format!(r#"{{"id":{},"source":"Player","target":"Opponent1","delta":{},"x":{},"y":{},"progress":{}}}"#,
                entry.id, entry.delta, point.x, point.y, entry.age / 1.2))
        }).collect::<Vec<_>>().join(",")
    }).unwrap_or_default();
    platform::publish(&format!(
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
        practice.arena.winner().is_some(),
        orbit.yaw,
        orbit.pitch,
        orbit.distance,
        character.status(),
        character.meshes,
        true,
        Arena::default().fighters()[0].health,
        ATTACK_DAMAGE,
        ATTACK_RANGE / 1000,
        ATTACK_COOLDOWN as f64 / TICKS_PER_SECOND as f64,
        plates,
        combat_text
    ));
}

#[derive(Default)]
struct Input {
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
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen(
        inline_js = "export function arena_has_character() { return globalThis.arenaShell.hasCharacterModel === true; } export function arena_input() { return globalThis.arenaShell.readInput(); } export function arena_hover() { return globalThis.arenaShell.readHover?.() ?? []; } export function arena_graphics() { return globalThis.arenaShell.readGraphics?.() ?? []; } export function arena_weapons() { return globalThis.arenaShell.readWeapons?.() ?? []; } export function arena_lighting() { return globalThis.arenaShell.readLighting?.() ?? []; } export function arena_focus(unit) { const tag = unit === 0 ? 'Player' : unit === 1 ? 'Opponent1' : null; if (tag) globalThis.arenaShell.onFocusUnit?.(tag); } export function arena_state(json) { globalThis.arenaShell.onState(JSON.parse(json)); }"
    )]
    extern "C" {
        fn arena_has_character() -> bool;
        fn arena_input() -> js_sys::Array;
        fn arena_hover() -> js_sys::Array;
        fn arena_graphics() -> js_sys::Array;
        fn arena_weapons() -> js_sys::Array;
        fn arena_lighting() -> js_sys::Array;
        fn arena_state(json: &str);
        fn arena_focus(unit: u32);
    }
    pub fn has_character_model() -> bool {
        arena_has_character()
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
        arena_focus(unit as u32);
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

    #[test]
    fn combat_numbers_come_from_successful_hits_not_attempts_and_reset_clears_them() {
        let mut app = App::new();
        app.init_resource::<Practice>()
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
            for _ in 0..30 {
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
    fn presentation_retracts_and_recovers_without_changing_requested_zoom() {
        let mut app = App::new();
        app.init_resource::<Practice>()
            .insert_resource(Orbit {
                pitch: 0.0,
                ..default()
            })
            .init_resource::<Time>()
            .init_resource::<Time<Fixed>>()
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
    fn queued_jump_drains_once_and_presentation_uses_rule_height() {
        let mut app = App::new();
        app.init_resource::<Practice>()
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
        let before = *practice.arena.fighters();
        assert!(matches!(
            practice.arena.step(&[ArenaCommand::Attack {
                actor: 0,
                target: 0
            }])[0],
            ArenaEvent::Rejected(arena_sim::arena::Rejection::InvalidTarget)
        ));
        assert_eq!(*practice.arena.fighters(), before);
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
        let arena = Arena::default();
        let health = arena.fighters()[1].health;
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
    pub fn has_character_model() -> bool {
        false
    }
    pub fn read_input() -> super::Input {
        super::Input::default()
    }
    pub fn publish(_: &str) {}
    pub fn focus_unit(_: usize) {}
}
