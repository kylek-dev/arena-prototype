//! Bevy presentation adapter for the shared arena rules. DOM integration is
//! isolated in `platform`; rules remain usable in native headless tests.
use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::{ATTACK_RANGE, Arena, ArenaCommand, ArenaEvent, PILLARS, Position};
use bevy::{core_pipeline::tonemapping::Tonemapping, prelude::*};

#[derive(Resource)]
struct Practice {
    arena: Arena,
    previous: [Vec3; 2],
    movement: Vec2,
    selected: bool,
    attack: bool,
    feedback: &'static str,
    flash: f32,
    frames: u32,
}
impl Default for Practice {
    fn default() -> Self {
        let arena = Arena::default();
        let previous = arena.fighters().map(|f| world(f.position));
        Self {
            arena,
            previous,
            movement: Vec2::ZERO,
            selected: false,
            attack: false,
            feedback: "Select the opponent to begin",
            flash: 0.0,
            frames: 0,
        }
    }
}
#[derive(Resource)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    distance: f32,
}
impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.65,
            distance: 13.0,
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
        .insert_resource(ClearColor(Color::srgb(0.035, 0.055, 0.08)))
        .insert_resource(GlobalAmbientLight {
            brightness: 260.0,
            ..default()
        })
        .insert_resource(Time::<Fixed>::from_hz(TICKS_PER_SECOND as f64))
        .init_resource::<Practice>()
        .init_resource::<Orbit>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Arena Prototype".into(),
                canvas: Some("#arena".into()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(PreUpdate, read_controls)
        .add_systems(FixedUpdate, simulate)
        .add_systems(Update, (present, publish).chain())
        .run();
}
fn world(position: Position) -> Vec3 {
    Vec3::new(position.x as f32 / 1000.0, 0.0, position.z as f32 / 1000.0)
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
) {
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
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(dark.clone()),
        Transform::from_xyz(0.0, -0.5, 0.0).with_scale(Vec3::new(26.0, 0.8, 20.0)),
    ));
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(stone.clone()),
        Transform::from_xyz(0.0, -0.08, 0.0).with_scale(Vec3::new(24.0, 0.16, 18.0)),
    ));
    // Thin seams make camera movement and scale readable without external textures.
    for x in -5..=5 {
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(edge.clone()),
            Transform::from_xyz(x as f32 * 2.0, 0.006, 0.0)
                .with_scale(Vec3::new(0.025, 0.01, 18.0)),
        ));
    }
    for z in -4..=4 {
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(edge.clone()),
            Transform::from_xyz(0.0, 0.007, z as f32 * 2.0)
                .with_scale(Vec3::new(24.0, 0.01, 0.025)),
        ));
    }
    for (x, z, sx, sz) in [
        (0.0, -9.2, 24.8, 0.4),
        (0.0, 9.2, 24.8, 0.4),
        (-12.2, 0.0, 0.4, 18.0),
        (12.2, 0.0, 0.4, 18.0),
    ] {
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(edge.clone()),
            Transform::from_xyz(x, 0.25, z).with_scale(Vec3::new(sx, 0.5, sz)),
        ));
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(line.clone()),
            Transform::from_xyz(x, 0.51, z).with_scale(Vec3::new(sx, 0.025, sz)),
        ));
    }
    for pillar in PILLARS {
        let p = world(pillar.center);
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(edge.clone()),
            Transform::from_translation(p + Vec3::Y * 0.12).with_scale(Vec3::new(2.15, 0.24, 2.15)),
        ));
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(stone.clone()),
            Transform::from_translation(p + Vec3::Y * 1.6).with_scale(Vec3::new(1.8, 3.2, 1.8)),
        ));
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(line.clone()),
            Transform::from_translation(p + Vec3::Y * 3.23).with_scale(Vec3::new(1.94, 0.14, 1.94)),
        ));
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(teal.clone()),
            Transform::from_translation(p + Vec3::new(0.0, 2.65, 0.906))
                .with_scale(Vec3::new(0.55, 0.06, 0.02)),
        ));
    }
    let circle = meshes.add(Circle::new(0.65));
    let body = meshes.add(Capsule3d::new(0.32, 0.7));
    let head = meshes.add(Sphere::new(0.23).mesh().uv(16, 12));
    for (index, accent) in [(0, teal.clone()), (1, orange.clone())] {
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
                parent.spawn((
                    Mesh3d(body.clone()),
                    MeshMaterial3d(accent.clone()),
                    Transform::from_xyz(0.0, 0.95, 0.0),
                ));
                parent.spawn((
                    Mesh3d(head.clone()),
                    MeshMaterial3d(line.clone()),
                    Transform::from_xyz(0.0, 1.7, 0.0),
                ));
                parent.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(dark.clone()),
                    Transform::from_xyz(0.0, 1.35, -0.32).with_scale(Vec3::new(0.5, 0.13, 0.2)),
                ));
                parent.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(accent.clone()),
                    Transform::from_xyz(0.0, 0.10, -0.55).with_scale(Vec3::new(0.16, 0.04, 0.4)),
                ));
            });
    }
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
        DirectionalLight {
            illuminance: 9000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, -0.55, 0.0)),
    ));
    let focus = world(Arena::default().fighters()[0].position) + Vec3::Y;
    commands.spawn((
        Camera3d::default(),
        Tonemapping::None,
        Msaa::Off,
        Transform::from_translation(focus + Vec3::new(0.0, 7.8, 10.3)).looking_at(focus, Vec3::Y),
    ));
}
fn read_controls(
    mut practice: ResMut<Practice>,
    mut orbit: ResMut<Orbit>,
    cameras: Query<(&Camera, &GlobalTransform)>,
) {
    let input = platform::read_input();
    if input.reset {
        *practice = Practice::default();
        *orbit = Orbit::default();
    }
    orbit.yaw -= input.orbit_x * 0.005;
    orbit.pitch = (orbit.pitch + input.orbit_y * 0.004).clamp(0.25, 1.2);
    orbit.distance = (orbit.distance + input.zoom * 0.01).clamp(6.0, 22.0);
    let rotation = Quat::from_rotation_y(orbit.yaw);
    let movement = rotation * Vec3::new(input.x, 0.0, input.z).clamp_length_max(1.0);
    practice.movement = Vec2::new(movement.x, movement.z);
    if input.select {
        practice.selected = true;
        practice.feedback = "Opponent selected";
    }
    if input.clear {
        practice.selected = false;
        practice.feedback = "Target cleared";
    }
    if let Some(cursor) = input.click
        && let Ok((camera, transform)) = cameras.single()
        && let Ok(ray) = camera.viewport_to_world(transform, cursor)
    {
        let to_target = world(practice.arena.fighters()[1].position) + Vec3::Y - ray.origin;
        let along = to_target.dot(*ray.direction);
        let hit = along > 0.0 && (to_target - *ray.direction * along).length_squared() < 0.9 * 0.9;
        practice.selected = hit;
        practice.feedback = if hit {
            "Opponent selected"
        } else {
            "Target cleared"
        };
    }
    if input.attack {
        practice.attack = true;
    }
}
fn simulate(mut practice: ResMut<Practice>) {
    practice.previous = practice.arena.fighters().map(|f| world(f.position));
    let mut commands = vec![ArenaCommand::Move {
        actor: 0,
        x: (practice.movement.x * 1000.0) as i16,
        z: (practice.movement.y * 1000.0) as i16,
    }];
    if practice.attack {
        practice.attack = false;
        if practice.selected {
            commands.push(ArenaCommand::Attack {
                actor: 0,
                target: 1,
            });
        } else {
            practice.feedback = "Select a target first";
        }
    }
    for event in practice.arena.step(&commands) {
        match event {
            ArenaEvent::Hit { .. } => {
                practice.feedback = "Pulse hit · 25 damage";
                practice.flash = 0.18;
            }
            ArenaEvent::Won { .. } => practice.feedback = "Round complete · reset to play again",
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
    orbit: Res<Orbit>,
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
            world(practice.arena.fighters()[i].position),
            fixed.overstep_fraction(),
        )
    });
    for (fighter, mut transform) in &mut fighters {
        transform.translation = positions[fighter.0];
        if fighter.0 == 0 && practice.movement.length_squared() > 0.001 {
            transform.rotation =
                Quat::from_rotation_y((-practice.movement.x).atan2(-practice.movement.y));
        }
    }
    if let Ok(mut camera) = cameras.single_mut() {
        let focus = positions[0] + Vec3::Y;
        let offset = Quat::from_rotation_y(orbit.yaw)
            * Vec3::new(
                0.0,
                orbit.pitch.sin() * orbit.distance,
                orbit.pitch.cos() * orbit.distance,
            );
        *camera = Transform::from_translation(focus + offset).looking_at(focus, Vec3::Y);
    }
    if let Ok((mut transform, mut visibility)) = ring.single_mut() {
        transform.translation = positions[1] + Vec3::Y * 0.03;
        *visibility = if practice.selected {
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
    cameras: Query<(&Camera, &GlobalTransform)>,
) {
    let fighters = practice.arena.fighters();
    let player = fighters[0];
    let target = fighters[1];
    let distance = (player.position.distance_squared(target.position) as f64).sqrt() / 1000.0;
    let cooldown =
        player.ready_at.saturating_sub(practice.arena.tick()) as f64 / TICKS_PER_SECOND as f64;
    let attack_status = match practice.arena.can_attack(0, 1) {
        Ok(()) => "Ready",
        Err(arena_sim::arena::Rejection::OutOfRange) => "Out of range",
        Err(arena_sim::arena::Rejection::Cooldown) => "Recharging",
        Err(arena_sim::arena::Rejection::LineOfSight) => "Line of sight blocked",
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
    platform::publish(&format!(
        r#"{{"tick":{},"frames":{},"player":{{"x":{},"z":{},"health":{}}},"target":{{"x":{},"z":{},"health":{},"screenX":{},"screenY":{}}},"selected":{},"distance":{:.2},"range":{},"cooldown":{:.3},"attackStatus":"{}","feedback":"{}","won":{},"camera":{{"yaw":{},"pitch":{},"distance":{}}}}}"#,
        practice.arena.tick(),
        practice.frames,
        player.position.x,
        player.position.z,
        player.health,
        target.position.x,
        target.position.z,
        target.health,
        screen.x,
        screen.y,
        practice.selected,
        distance,
        ATTACK_RANGE / 1000,
        cooldown,
        attack_status,
        practice.feedback,
        practice.arena.winner().is_some(),
        orbit.yaw,
        orbit.pitch,
        orbit.distance
    ));
}

#[derive(Default)]
struct Input {
    x: f32,
    z: f32,
    attack: bool,
    select: bool,
    clear: bool,
    reset: bool,
    orbit_x: f32,
    orbit_y: f32,
    zoom: f32,
    click: Option<Vec2>,
}
#[cfg(target_arch = "wasm32")]
mod platform {
    use super::Input;
    use bevy::prelude::Vec2;
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen(
        inline_js = "export function arena_input() { return globalThis.arenaShell.readInput(); } export function arena_state(json) { globalThis.arenaShell.onState(JSON.parse(json)); }"
    )]
    extern "C" {
        fn arena_input() -> js_sys::Array;
        fn arena_state(json: &str);
    }
    pub fn read_input() -> Input {
        let a = arena_input();
        let number = |i| a.get(i).as_f64().unwrap_or(0.0) as f32;
        Input {
            x: number(0),
            z: number(1),
            attack: number(2) != 0.0,
            select: number(3) != 0.0,
            clear: number(4) != 0.0,
            reset: number(5) != 0.0,
            orbit_x: number(6),
            orbit_y: number(7),
            zoom: number(8),
            click: if number(9) >= 0.0 {
                Some(Vec2::new(number(9), number(10)))
            } else {
                None
            },
        }
    }
    pub fn publish(json: &str) {
        arena_state(json);
    }
}
#[cfg(not(target_arch = "wasm32"))]
mod platform {
    pub fn read_input() -> super::Input {
        super::Input::default()
    }
    pub fn publish(_: &str) {}
}
