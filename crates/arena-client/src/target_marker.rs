//! The ground marker under the current target: four corner brackets framing
//! its feet, turning slowly and breathing, ember for an enemy and teal for a
//! friend (the palette's team roles). It snaps in from wider when the target
//! changes. Unlit and drawn just above the floor, so it reads in any light;
//! hidden with no target. Presentation only.
use bevy::{
    asset::RenderAssetUsages,
    light::{NotShadowCaster, NotShadowReceiver},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::{FighterVisual, Practice, palette};

/// Half the bracketed square's side, the arm length and the arm width (m).
const HALF: f32 = 0.62;
const ARM: f32 = 0.3;
const WIDTH: f32 = 0.07;
/// Just above the floor, clear of z-fighting.
const LIFT: f32 = 0.03;
/// Turning speed (rad/s), breathing rate (Hz) and depth.
const SPIN: f32 = 0.45;
const BREATH_HZ: f32 = 0.7;
const BREATH: f32 = 0.05;
/// A new target's marker closes in from this scale over `SNAP_SECONDS`.
const SNAP_FROM: f32 = 1.7;
const SNAP_SECONDS: f32 = 0.2;
/// Over-bright, so the thresholded bloom gives it a soft glow.
const GLOW: f32 = 1.8;

#[derive(Component)]
pub(crate) struct TargetMarker;

/// The marker's one material, recoloured when the target's side changes.
#[derive(Resource)]
pub(crate) struct MarkerMaterial(Handle<StandardMaterial>);

pub(crate) fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let material = materials.add(StandardMaterial {
        base_color: tint(true),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        fog_enabled: false,
        ..default()
    });
    commands.insert_resource(MarkerMaterial(material.clone()));
    commands.spawn((
        Name::new("Target marker"),
        TargetMarker,
        Mesh3d(meshes.add(brackets_mesh())),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, LIFT, 0.0),
        NotShadowCaster,
        NotShadowReceiver,
        Visibility::Hidden,
    ));
}

/// The team colour, over-bright for the glow, slightly translucent.
fn tint(hostile: bool) -> Color {
    let swatch = if hostile {
        palette::EMBER_4
    } else {
        palette::TEAL_4
    };
    let c = palette::color(swatch).to_linear();
    Color::LinearRgba(LinearRgba {
        red: c.red * GLOW,
        green: c.green * GLOW,
        blue: c.blue * GLOW,
        alpha: 0.92,
    })
}

/// Whether `actor` is on the other side from the player (actor 0).
fn hostile(practice: &Practice, actor: usize) -> bool {
    practice.arena.team(actor) != practice.arena.team(0)
}

/// Four L-shaped corner brackets around the origin, flat on the XZ plane,
/// facing up: two quads per corner.
pub(crate) fn brackets_mesh() -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut quad = |x0: f32, z0: f32, x1: f32, z1: f32| {
        let base = positions.len() as u32;
        positions.extend([[x0, 0.0, z0], [x1, 0.0, z0], [x1, 0.0, z1], [x0, 0.0, z1]]);
        indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
    };
    for (sx, sz) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
        let (cx, cz) = (sx * HALF, sz * HALF);
        // The arm along X, then the arm along Z, meeting at the corner.
        let (ax0, ax1) = sorted(cx, cx - sx * ARM);
        let (az0, az1) = sorted(cz, cz - sz * WIDTH);
        quad(ax0, az0, ax1, az1);
        let (bx0, bx1) = sorted(cx, cx - sx * WIDTH);
        let (bz0, bz1) = sorted(cz - sz * WIDTH, cz - sz * ARM);
        quad(bx0, bz0, bx1, bz1);
    }
    let count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; count])
    .with_inserted_indices(Indices::U32(indices))
}

fn sorted(a: f32, b: f32) -> (f32, f32) {
    (a.min(b), a.max(b))
}

/// Puts the marker under the selected fighter's drawn position (on the
/// floor, whatever its jump height), turning, breathing and snapping in
/// after a change of target.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn place(
    practice: Res<Practice>,
    time: Res<Time>,
    material: Option<Res<MarkerMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    visuals: Query<(&FighterVisual, &Transform), Without<TargetMarker>>,
    mut marker: Query<(&mut Transform, &mut Visibility), With<TargetMarker>>,
    mut last: Local<Option<(usize, bool)>>,
    mut since: Local<f32>,
) {
    let Ok((mut transform, mut visibility)) = marker.single_mut() else {
        return;
    };
    let target = practice.selected.and_then(|actor| {
        visuals
            .iter()
            .find(|(visual, _)| visual.0 == actor)
            .map(|(_, at)| (actor, at.translation))
    });
    let Some((actor, at)) = target else {
        *visibility = Visibility::Hidden;
        *last = None;
        return;
    };
    let side = hostile(&practice, actor);
    if *last != Some((actor, side)) {
        if last.is_none_or(|(previous, _)| previous != actor) {
            *since = 0.0;
        }
        if last.is_none_or(|(_, previous)| previous != side)
            && let Some(material) = material
            && let Some(mut m) = materials.get_mut(&material.0)
        {
            m.base_color = tint(side);
        }
        *last = Some((actor, side));
    }
    *since += time.delta_secs();
    let elapsed = time.elapsed_secs();
    let snap = (*since / SNAP_SECONDS).min(1.0);
    let snap = 1.0 - (1.0 - snap) * (1.0 - snap);
    let scale = (SNAP_FROM + (1.0 - SNAP_FROM) * snap)
        * (1.0 + BREATH * (elapsed * BREATH_HZ * std::f32::consts::TAU).sin());
    *transform = Transform::from_xyz(at.x, LIFT, at.z)
        .with_rotation(Quat::from_rotation_y(elapsed * SPIN))
        .with_scale(Vec3::new(scale, 1.0, scale));
    *visibility = Visibility::Inherited;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brackets_are_four_flat_corners_around_the_feet() {
        let mesh = brackets_mesh();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        assert_eq!(positions.len(), 32);
        assert!(positions.iter().all(|p| p[1] == 0.0));
        // Every point sits in the band between the arms' inner ends and the
        // corner, so the feet in the middle stay clear.
        for p in positions {
            let reach = p[0].abs().max(p[2].abs());
            assert!((HALF - ARM - 1e-6..=HALF + 1e-6).contains(&reach), "{p:?}");
        }
        // Mirror-symmetric in both axes.
        for p in positions {
            assert!(
                positions
                    .iter()
                    .any(|q| (q[0] + p[0]).abs() < 1e-6 && q[2] == p[2])
            );
            assert!(
                positions
                    .iter()
                    .any(|q| (q[2] + p[2]).abs() < 1e-6 && q[0] == p[0])
            );
        }
    }

    #[test]
    fn it_follows_the_target_on_the_floor_in_its_sides_colour_and_hides_without_one() {
        let mut app = App::new();
        app.init_resource::<Practice>()
            .init_resource::<Time>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(Update, place);
        let world = app.world_mut();
        let (mut meshes, mut materials) = (Assets::<Mesh>::default(), Assets::default());
        let mut commands_queue = bevy::ecs::world::CommandQueue::default();
        let mut commands = Commands::new(&mut commands_queue, world);
        spawn(&mut commands, &mut meshes, &mut materials);
        commands_queue.apply(world);
        world.insert_resource(materials);
        world.spawn((FighterVisual(0), Transform::from_xyz(1.0, 0.0, 2.0)));
        // Mid-jump: the marker stays on the floor.
        world.spawn((FighterVisual(1), Transform::from_xyz(-3.0, 0.8, 4.0)));
        let marker = |app: &mut App| {
            app.world_mut()
                .query_filtered::<(&Transform, &Visibility), With<TargetMarker>>()
                .single(app.world())
                .map(|(t, v)| (*t, *v))
                .unwrap()
        };
        let colour = |app: &App| {
            let handle = &app.world().resource::<MarkerMaterial>().0;
            app.world()
                .resource::<Assets<StandardMaterial>>()
                .get(handle)
                .unwrap()
                .base_color
        };
        app.update();
        assert_eq!(marker(&mut app).1, Visibility::Hidden);

        app.world_mut().resource_mut::<Practice>().selected = Some(1);
        app.update();
        let (at, shown) = marker(&mut app);
        assert_eq!(shown, Visibility::Inherited);
        assert_eq!(
            (at.translation.x, at.translation.y, at.translation.z),
            (-3.0, LIFT, 4.0)
        );
        assert!(at.scale.x > 1.5, "a new target's marker starts wide");
        assert_eq!(colour(&app), tint(true));

        app.world_mut().resource_mut::<Practice>().selected = Some(0);
        app.update();
        assert_eq!(colour(&app), tint(false));
        assert_eq!(marker(&mut app).0.translation.x, 1.0);

        app.world_mut().resource_mut::<Practice>().selected = None;
        app.update();
        assert_eq!(marker(&mut app).1, Visibility::Hidden);
    }
}
