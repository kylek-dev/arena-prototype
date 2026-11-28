//! Hub worlds in the renderer ([ADR 0057](../../../docs/decisions/0057-hub-on-the-world-server.md)):
//! a hub is a network session like a match (the fighters, HUD, targeting
//! and casting all come from `network` and the match's own systems); this
//! module only swaps the venue. While the session is in a hub world, the
//! arena's venue gives way to the graybox plaza, whose floor and columns
//! are the camera's only obstacles.

use bevy::prelude::*;

use arena_sim::hub as rules;

/// Whether the plaza stands in for the venue.
#[derive(Resource, Default)]
pub struct Hub {
    active: bool,
}

impl Hub {
    pub fn active(&self) -> bool {
        self.active
    }
}

#[derive(Component)]
pub struct Plaza;
/// The plaza's camera obstacles: the only ones the boom meets in the hub.
#[derive(Component)]
pub struct PlazaObstacle;

/// Graybox colours for the plaza: the arena's stone, a darker trim.
const FLOOR: crate::palette::Swatch = crate::palette::STONE_3;
const STONE: crate::palette::Swatch = crate::palette::STONE_5;

/// Shows the plaza while the network session is in a hub world, and the
/// arena's venue otherwise; a venue rebuilt meanwhile (an edited map) stays
/// hidden in the hub.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn venue(
    mut commands: Commands,
    mut hub: ResMut<Hub>,
    net: Option<Res<crate::Net>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut venue: Query<&mut Visibility, With<crate::venue::VenueEntity>>,
    rebuilt: Query<(), Added<crate::venue::VenueEntity>>,
    plaza: Query<Entity, With<Plaza>>,
    mut orbit: Option<ResMut<crate::Orbit>>,
) {
    let in_world = net
        .as_ref()
        .and_then(|net| net.session.as_ref())
        .is_some_and(crate::network::Session::is_hub);
    if in_world == hub.active {
        if hub.active && !rebuilt.is_empty() {
            for mut visibility in &mut venue {
                *visibility = Visibility::Hidden;
            }
        }
        return;
    }
    hub.active = in_world;
    for mut visibility in &mut venue {
        *visibility = if in_world {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    if in_world {
        spawn_plaza(&mut commands, &mut meshes, &mut materials);
    } else {
        for entity in &plaza {
            commands.entity(entity).despawn();
        }
    }
    // The boom starts afresh where the player now stands.
    if let Some(orbit) = orbit.as_mut() {
        orbit.eye = None;
    }
}

/// A rules position (millimetres) as a world point on the floor.
pub fn world(at: arena_sim::arena::Position) -> Vec3 {
    Vec3::new(at.x as f32 / 1000.0, 0.0, at.z as f32 / 1000.0)
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
    // Each column is drawn round; the rules stop bodies at the hexagonal
    // pillar inscribed in it (`rules::plaza`), so at a flat face a body can
    // stand a few centimetres into the drawn stone (graybox).
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
