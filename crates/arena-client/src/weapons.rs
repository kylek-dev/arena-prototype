//! Selectable Verdant Eclipse glaives and Aetherthorn staff with procedural fallbacks.
//! All geometry, mounting and effects are presentation-only; no combat rules.
use bevy::{
    asset::RenderAssetUsages, gltf::GltfMaterialName, mesh::PrimitiveTopology, prelude::*,
    world_serialization::WorldInstanceReady,
};
use std::collections::HashMap;

pub const MODEL_PATH: &str = "weapons/verdant-eclipse.glb";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Purple,
    Yellow,
    Red,
    Blue,
    Green,
    Cyan,
}

impl Variant {
    fn color(self) -> Color {
        match self {
            Self::Purple => Color::srgb(0.72, 0.08, 1.0),
            Self::Yellow => Color::srgb(1.0, 0.85, 0.025),
            Self::Red => Color::srgb(1.0, 0.035, 0.025),
            Self::Blue => Color::srgb(0.035, 0.3, 1.0),
            Self::Green => Color::srgb(0.08, 1.0, 0.012),
            Self::Cyan => Color::srgb(0.035, 0.65, 0.9),
        }
    }

    fn emission(self, strength: f32) -> LinearRgba {
        let c = self.color().to_linear();
        LinearRgba::new(c.red * strength, c.green * strength, c.blue * strength, 1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Component)]
pub struct Selection {
    staff: bool,
    variant: Variant,
}

impl Selection {
    pub fn from_values(kind: Option<f64>, color: Option<f64>) -> Self {
        let variant = match color {
            Some(1.0) => Variant::Yellow,
            Some(2.0) => Variant::Red,
            Some(3.0) => Variant::Blue,
            Some(4.0) => Variant::Green,
            Some(5.0) => Variant::Cyan,
            _ => Variant::Purple,
        };
        Self {
            staff: kind == Some(1.0),
            variant,
        }
    }
}

/// Attached to the player's motion carrier, never to simulation entities.
#[derive(Component)]
pub struct EquipmentCarrier;

/// Instantiate each requested combination at most once. Switching cached groups
/// only changes parent visibility; hidden model-ready callbacks cannot reveal them.
pub fn read_selection() -> Selection {
    super::platform::read_weapons()
}

pub fn apply(
    In(selection): In<Selection>,
    mut commands: Commands,
    carriers: Query<Entity, With<EquipmentCarrier>>,
    mut groups: Query<(&Selection, &mut Visibility)>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut found = false;
    for (cached, mut visibility) in &mut groups {
        let selected = *cached == selection;
        found |= selected;
        let next = if selected {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != next {
            *visibility = next;
        }
    }
    if found {
        return;
    }
    let Ok(carrier) = carriers.single() else {
        return;
    };
    commands.entity(carrier).with_children(|parent| {
        parent
            .spawn((selection, Transform::default(), Visibility::Inherited))
            .with_children(|group| {
                if selection.staff {
                    spawn_staff(
                        group,
                        &asset_server,
                        &mut meshes,
                        &mut materials,
                        selection.variant,
                    );
                } else {
                    let scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset(MODEL_PATH));
                    BackBlades::new(&mut meshes, &mut materials, selection.variant)
                        .spawn(group, Some(&scene));
                }
            });
    });
}

fn staff_transform() -> Transform {
    // The exported staff is Y-up, 5.3 units tall. Keep its tip above the floor
    // and its crown above the shoulder. Offset the foot opposite the tilt so
    // the shaft crosses the center of the back at torso height, not beside it.
    Transform::from_xyz(-0.255, 0.12, 0.4)
        .with_rotation(Quat::from_rotation_z(-12.0_f32.to_radians()))
        .with_scale(Vec3::splat(0.46))
}

fn spawn_staff(
    parent: &mut ChildSpawnerCommands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    variant: Variant,
) {
    let shaft = meshes.add(Cylinder::new(0.075, 3.8));
    let crystal = meshes.add(Sphere::new(0.3).mesh().uv(6, 4));
    let dark = materials.add(StandardMaterial {
        base_color: Color::srgb(0.012, 0.026, 0.046),
        metallic: 0.65,
        ..default()
    });
    let energy = materials.add(StandardMaterial {
        base_color: variant.color(),
        emissive: variant.emission(3.0),
        ..default()
    });
    let halo = meshes.add(staff_halo_mesh());
    let halo_tint = variant.color().to_linear();
    let glow = materials.add(StandardMaterial {
        base_color: Color::linear_rgb(
            halo_tint.red * 2.5,
            halo_tint.green * 2.5,
            halo_tint.blue * 2.5,
        ),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        cull_mode: None,
        ..default()
    });
    parent
        .spawn((
            Name::new("Aetherthorn back mount"),
            staff_transform(),
            Visibility::Inherited,
        ))
        .with_children(|mount| {
            let fallback = mount
                .spawn((Transform::default(), Visibility::Inherited))
                .with_children(|fallback| {
                    fallback.spawn((
                        Mesh3d(shaft),
                        MeshMaterial3d(dark),
                        Transform::from_xyz(0.0, 2.0, 0.0),
                    ));
                    fallback.spawn((
                        Mesh3d(crystal.clone()),
                        MeshMaterial3d(energy.clone()),
                        Transform::from_xyz(0.0, 4.72, 0.0).with_scale(Vec3::new(1.0, 2.0, 1.0)),
                    ));
                })
                .id();
            // These effects are siblings of the imported model/fallback: model
            // readiness never removes them, and the selected group still hides them.
            for (y, scale) in [
                (4.72, Vec3::new(0.85, 1.25, 1.0)),
                (2.55, Vec3::new(0.20, 0.5, 1.0)),
            ] {
                mount.spawn((
                    Name::new("Aetherthorn soft energy halo"),
                    StaffHalo { scale },
                    Mesh3d(halo.clone()),
                    MeshMaterial3d(glow.clone()),
                    Transform::from_xyz(0.0, y, 0.0).with_scale(scale),
                ));
            }
            for index in 0..12 {
                mount.spawn((
                    Name::new("Aetherthorn orbiting spark"),
                    StaffMote { index },
                    Mesh3d(crystal.clone()),
                    MeshMaterial3d(energy.clone()),
                    Transform::from_translation(staff_mote_position(0.0, index))
                        .with_scale(Vec3::splat(0.055)),
                ));
            }
            mount
                .spawn((
                    Name::new("Aetherthorn"),
                    ImportedGlaive { fallback, variant },
                    WorldAssetRoot(
                        asset_server
                            .load(GltfAssetLabel::Scene(0).from_asset("weapons/aetherthorn.glb")),
                    ),
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .observe(on_ready);
        });
    parent.spawn((
        PointLight {
            color: variant.color(),
            intensity: 55.0,
            range: 2.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(staff_transform().transform_point(Vec3::Y * 4.72)),
    ));
}

/// Only recolor energy and strongly green jade; leave metal/leather/electrum alone.
fn tinted_material(source: &StandardMaterial, variant: Variant) -> Option<StandardMaterial> {
    let mut material = source.clone();
    let emission = source.emissive;
    let strength = emission.red.max(emission.green).max(emission.blue);
    if strength > 0.0 {
        material.base_color = variant.color().with_alpha(source.base_color.alpha());
        material.emissive = variant.emission(strength);
    } else {
        let base = source.base_color.to_linear();
        if base.green <= base.red * 2.0 || base.green <= base.blue * 2.0 {
            return None;
        }
        let tint = variant.color().to_linear();
        let intensity = base.green;
        material.base_color = Color::linear_rgba(
            tint.red * intensity,
            tint.green * intensity,
            tint.blue * intensity,
            base.alpha,
        );
    }
    Some(material)
}

fn crystal_material(source: &StandardMaterial, variant: Variant) -> StandardMaterial {
    let mut material = source.clone();
    let tint = variant.color().to_linear();
    // Keep a lit surface, but put the broad crystal body above the bloom knee
    // in every palette. Sub-threshold emission cannot produce a body halo.
    material.base_color = Color::linear_rgba(
        tint.red * 0.32 + 0.04,
        tint.green * 0.32 + 0.04,
        tint.blue * 0.32 + 0.04,
        source.base_color.alpha(),
    );
    material.emissive = variant.emission(3.2);
    material.emissive_exposure_weight = 0.0;
    material.metallic = 0.15;
    material.perceptual_roughness = 0.24;
    material.unlit = false;
    material
}

fn recolor_model(
    entity: Entity,
    variant: Variant,
    children: &Query<&Children>,
    handles: &mut Query<(
        &mut MeshMaterial3d<StandardMaterial>,
        Option<&GltfMaterialName>,
    )>,
    materials: &mut Assets<StandardMaterial>,
) {
    // One clone per source material per instance, not one per mesh or per frame.
    let mut cache = HashMap::new();
    for descendant in children.iter_descendants(entity) {
        let Ok((mut handle, name)) = handles.get_mut(descendant) else {
            continue;
        };
        let source_id = handle.0.id();
        if let Some(replacement) = cache.get(&source_id) {
            handle.0 = Handle::clone(replacement);
            continue;
        }
        let Some(tinted) = materials.get(source_id).and_then(|m| {
            if name.is_some_and(|name| name.0 == "Luminous cyan crystal") {
                Some(crystal_material(m, variant))
            } else {
                tinted_material(m, variant)
            }
        }) else {
            continue;
        };
        let replacement = materials.add(tinted);
        handle.0 = replacement.clone();
        cache.insert(source_id, replacement);
    }
}

#[derive(Component)]
struct ImportedGlaive {
    fallback: Entity,
    variant: Variant,
}

/// Blender exports its XY blade plane into glTF's XZ plane. Restore that plane,
/// align its center grip with the procedural weapon's Y axis, and fit 9.3 source
/// units into the existing 2.16 m weapon span (before the shared mount scale).
fn model_transform() -> Transform {
    Transform::from_rotation(
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
            * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
    )
    .with_scale(Vec3::splat(2.16 / 9.3))
}

fn on_ready(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    imported: Query<&ImportedGlaive>,
    mut visibility: Query<&mut Visibility>,
    mut handles: Query<(
        &mut MeshMaterial3d<StandardMaterial>,
        Option<&GltfMaterialName>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if let Ok(model) = imported.get(ready.entity) {
        recolor_model(
            ready.entity,
            model.variant,
            &children,
            &mut handles,
            &mut materials,
        );
    }
    reveal_model(ready.entity, &children, &meshes, &imported, &mut visibility);
}

fn reveal_model(
    entity: Entity,
    children: &Query<&Children>,
    meshes: &Query<(), With<Mesh3d>>,
    imported: &Query<&ImportedGlaive>,
    visibility: &mut Query<&mut Visibility>,
) {
    let Ok(model) = imported.get(entity) else {
        return;
    };
    if !children
        .iter_descendants(entity)
        .any(|entity| meshes.contains(entity))
    {
        warn!("Weapon scene has no meshes; retaining procedural fallback");
        return;
    }
    if let Ok(mut fallback) = visibility.get_mut(model.fallback) {
        *fallback = Visibility::Hidden;
    }
    if let Ok(mut model) = visibility.get_mut(entity) {
        *model = Visibility::Inherited;
    }
}

// Authored inner/outer rim corners form a real open crescent recess and three
// angular outer teeth. Cross-sections keep triangulation inside that outline.
// Dimensions stay in XY; blade thickness and cutting-edge band do not grow.
const SECTIONS: [(f32, f32, f32); 13] = [
    section(0.23, -0.060, 0.060),
    section(0.34, -0.095, 0.150),
    section(0.43, -0.080, 0.270),
    section(0.49, -0.035, 0.205),
    section(0.56, 0.090, 0.310),
    section(0.64, 0.230, 0.405),
    section(0.70, 0.275, 0.340),
    section(0.78, 0.300, 0.430),
    section(0.86, 0.275, 0.490),
    section(0.93, 0.300, 0.565),
    section(0.97, 0.390, 0.515),
    section(1.02, 0.480, 0.560),
    section(1.08, 0.600, 0.600),
];

// Renderer representation: height, center, half-width (metres).
const fn section(y: f32, inner: f32, outer: f32) -> (f32, f32, f32) {
    (y, (inner + outer) * 0.5, (outer - inner) * 0.5)
}

const EDGE_DEPTH: f32 = 0.004;
const RIDGE_DEPTH: f32 = 0.025;
const EDGE_BAND: f32 = 0.018;
const ARC_STEPS_PER_SECTION: usize = 3;

fn tip_taper(width: f32) -> f32 {
    (width / 0.035).min(1.0)
}

#[derive(Clone, Copy)]
enum BladeLayer {
    Metal,
    Edge,
    Halo,
}

#[derive(Component)]
pub struct ElectricalArc {
    variant: usize,
    offset: usize,
}

pub struct BackBlades {
    variant: Variant,
    blade: Handle<Mesh>,
    edge: Handle<Mesh>,
    halo: Handle<Mesh>,
    arcs: [Handle<Mesh>; 3],
    cube: Handle<Mesh>,
    gem: Handle<Mesh>,
    metal: Handle<StandardMaterial>,
    grip: Handle<StandardMaterial>,
    energy: Handle<StandardMaterial>,
    haze: Handle<StandardMaterial>,
    spark: Handle<StandardMaterial>,
}

impl BackBlades {
    pub fn new(
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        variant: Variant,
    ) -> Self {
        Self {
            variant,
            blade: meshes.add(blade_mesh(BladeLayer::Metal)),
            edge: meshes.add(blade_mesh(BladeLayer::Edge)),
            halo: meshes.add(blade_mesh(BladeLayer::Halo)),
            arcs: std::array::from_fn(|variant| meshes.add(lightning_mesh(variant))),
            cube: meshes.add(Cuboid::from_size(Vec3::ONE)),
            gem: meshes.add(Sphere::new(0.085).mesh().uv(12, 8)),
            metal: materials.add(StandardMaterial {
                base_color: Color::srgb(0.055, 0.13, 0.105),
                metallic: 0.8,
                perceptual_roughness: 0.3,
                ..default()
            }),
            grip: materials.add(StandardMaterial {
                base_color: Color::srgb(0.025, 0.045, 0.035),
                perceptual_roughness: 0.85,
                ..default()
            }),
            energy: materials.add(StandardMaterial {
                base_color: variant.color(),
                emissive: variant.emission(5.0),
                unlit: true,
                ..default()
            }),
            haze: materials.add(StandardMaterial {
                base_color: variant.color().with_alpha(0.13),
                alpha_mode: AlphaMode::Add,
                unlit: true,
                ..default()
            }),
            spark: materials.add(StandardMaterial {
                base_color: variant.color(),
                emissive: variant.emission(6.0),
                unlit: true,
                cull_mode: None,
                ..default()
            }),
        }
    }

    pub fn spawn(&self, parent: &mut ChildSpawnerCommands, scene: Option<&Handle<WorldAsset>>) {
        for (index, transform) in mounts().into_iter().enumerate() {
            parent
                .spawn((
                    Name::new(format!("Storm glaive {index}")),
                    transform,
                    Visibility::Inherited,
                ))
                .with_children(|sword| {
                    let fallback = sword
                        .spawn((
                            Name::new("Procedural glaive fallback"),
                            Transform::default(),
                            Visibility::Inherited,
                        ))
                        .with_children(|sword| {
                            // One curved blade at each end, leaving the middle free to grip.
                            for end in 0..2 {
                                let rotation =
                                    Quat::from_rotation_z(end as f32 * std::f32::consts::PI);
                                for (mesh, material) in [
                                    (&self.blade, &self.metal),
                                    (&self.edge, &self.energy),
                                    (&self.halo, &self.haze),
                                ] {
                                    sword.spawn((
                                        Mesh3d(mesh.clone()),
                                        MeshMaterial3d(material.clone()),
                                        Transform::from_rotation(rotation),
                                    ));
                                }
                                for (variant, mesh) in self.arcs.iter().enumerate() {
                                    sword.spawn((
                                        ElectricalArc {
                                            variant,
                                            offset: index + end,
                                        },
                                        Mesh3d(mesh.clone()),
                                        MeshMaterial3d(self.spark.clone()),
                                        Transform::from_rotation(rotation),
                                        Visibility::Hidden,
                                    ));
                                }
                            }
                            // Center grip and two split guards: no sword hilt at either end.
                            for (position, size, angle, material) in [
                                (Vec3::ZERO, Vec3::new(0.10, 0.44, 0.10), 0.0, &self.grip),
                                (
                                    Vec3::new(0.0, -0.23, 0.0),
                                    Vec3::new(0.32, 0.08, 0.09),
                                    0.0,
                                    &self.metal,
                                ),
                                (
                                    Vec3::new(0.0, 0.23, 0.0),
                                    Vec3::new(0.32, 0.08, 0.09),
                                    0.0,
                                    &self.metal,
                                ),
                                (
                                    Vec3::new(-0.16, -0.28, 0.0),
                                    Vec3::new(0.065, 0.18, 0.12),
                                    -0.5,
                                    &self.metal,
                                ),
                                (
                                    Vec3::new(0.16, 0.28, 0.0),
                                    Vec3::new(0.065, 0.18, 0.12),
                                    0.5,
                                    &self.metal,
                                ),
                                (
                                    Vec3::new(0.0, -0.16, 0.0),
                                    Vec3::new(0.12, 0.035, 0.12),
                                    0.0,
                                    &self.energy,
                                ),
                                (
                                    Vec3::new(0.0, 0.16, 0.0),
                                    Vec3::new(0.12, 0.035, 0.12),
                                    0.0,
                                    &self.energy,
                                ),
                            ] {
                                sword.spawn((
                                    Mesh3d(self.cube.clone()),
                                    MeshMaterial3d(material.clone()),
                                    Transform::from_translation(position)
                                        .with_rotation(Quat::from_rotation_z(angle))
                                        .with_scale(size),
                                ));
                            }
                            for y in [-0.23, 0.23] {
                                for z in [-0.09, 0.09] {
                                    sword.spawn((
                                        Mesh3d(self.gem.clone()),
                                        MeshMaterial3d(self.energy.clone()),
                                        Transform::from_xyz(0.0, y, z)
                                            .with_scale(Vec3::new(1.0, 0.8, 0.5)),
                                    ));
                                }
                            }
                        })
                        .id();
                    if let Some(scene) = scene {
                        sword
                            .spawn((
                                Name::new("Verdant Eclipse"),
                                ImportedGlaive {
                                    fallback,
                                    variant: self.variant,
                                },
                                WorldAssetRoot(scene.clone()),
                                model_transform(),
                                Visibility::Hidden,
                            ))
                            .observe(on_ready);
                    }
                });
        }
        // Variant-matched spill; no postprocessing required on WebGL.
        parent.spawn((
            PointLight {
                color: self.variant.color(),
                intensity: 55.0,
                range: 2.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(0.0, 1.25, 0.8),
        ));
    }
}

fn mounts() -> [Transform; 2] {
    // Fighters face -Z; +Z is their back. Both center grips share one close-fitting
    // plane and intentionally intersect at the X instead of stacking in depth.
    // Stay outside the model/fallback hierarchy so loading cannot hide them.
    [-1.0, 1.0].map(|side| {
        Transform::from_xyz(0.0, 1.25, 0.36)
            .with_rotation(
                Quat::from_rotation_z(side * 20.0_f32.to_radians())
                    * Quat::from_rotation_y(if side < 0.0 {
                        std::f32::consts::PI
                    } else {
                        0.0
                    }),
            )
            .with_scale(Vec3::splat(0.9))
    })
}

fn blade_mesh(layer: BladeLayer) -> Mesh {
    let mut positions = Vec::<[f32; 3]>::new();
    let mut triangle = |a: Vec3, b: Vec3, c: Vec3| {
        if (b - a).cross(c - a).length_squared() > 1e-12 {
            positions.extend([a.to_array(), b.to_array(), c.to_array()]);
        }
    };
    for pair in SECTIONS.windows(2) {
        for face in [-1.0, 1.0] {
            for side in [-1.0, 1.0] {
                let section = |(y, center, width): (f32, f32, f32)| {
                    let depth = face * tip_taper(width);
                    match layer {
                        BladeLayer::Edge => [
                            Vec3::new(center + side * width, y, depth * 0.005),
                            Vec3::new(
                                center + side * (width - EDGE_BAND * tip_taper(width)),
                                y,
                                depth * 0.010,
                            ),
                        ],
                        BladeLayer::Metal => [
                            Vec3::new(center + side * width, y, depth * EDGE_DEPTH),
                            Vec3::new(center, y, depth * RIDGE_DEPTH),
                        ],
                        BladeLayer::Halo => [
                            Vec3::new(
                                center + side * (width + 0.025 * tip_taper(width)),
                                y,
                                depth * 0.04,
                            ),
                            Vec3::new(
                                center + side * (width - 0.025 * tip_taper(width)),
                                y,
                                depth * 0.04,
                            ),
                        ],
                    }
                };
                let [a, b] = section(pair[0]);
                let [c, d] = section(pair[1]);
                if side * face > 0.0 {
                    triangle(a, c, b);
                    triangle(b, c, d);
                } else {
                    triangle(a, b, c);
                    triangle(b, d, c);
                }
            }
        }
        if matches!(layer, BladeLayer::Metal) {
            // Close the thin sides of the blade.
            for side in [-1.0, 1.0] {
                let (y0, x0, w0) = pair[0];
                let (y1, x1, w1) = pair[1];
                let depth0 = EDGE_DEPTH * tip_taper(w0);
                let depth1 = EDGE_DEPTH * tip_taper(w1);
                let a = Vec3::new(x0 + side * w0, y0, -depth0);
                let b = Vec3::new(x0 + side * w0, y0, depth0);
                let c = Vec3::new(x1 + side * w1, y1, -depth1);
                let d = Vec3::new(x1 + side * w1, y1, depth1);
                if side > 0.0 {
                    triangle(a, c, b);
                    triangle(b, c, d);
                } else {
                    triangle(a, b, c);
                    triangle(b, d, c);
                }
            }
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.compute_flat_normals();
    mesh
}

/// Cached jagged ribbons, alternating in visibility rather than allocating meshes
/// every frame. Local renderer time drives decoration only, never simulation.
fn lightning_mesh(variant: usize) -> Mesh {
    let mut positions = Vec::new();
    for face in [-1.0, 1.0] {
        for side in [-1.0, 1.0] {
            let steps = (SECTIONS.len() - 1) * ARC_STEPS_PER_SECTION;
            let points: Vec<Vec3> = (0..=steps)
                .map(|i| {
                    // Include every authored corner so effects do not bridge notches.
                    let t = i as f32 / ARC_STEPS_PER_SECTION as f32;
                    let section = (t.floor() as usize).min(SECTIONS.len() - 2);
                    let fraction = t - section as f32;
                    let (y0, x0, width0) = SECTIONS[section];
                    let (y1, x1, width1) = SECTIONS[section + 1];
                    let a = Vec3::new(y0, x0, width0);
                    let b = Vec3::new(y1, x1, width1);
                    let profile = a.lerp(b, fraction);
                    let jitter = ((i * 13 + variant * 7) % 11) as f32 / 10.0 - 0.5;
                    Vec3::new(
                        profile.y
                            + side * (profile.z - EDGE_BAND * 0.5 * tip_taper(profile.z))
                            + jitter * 0.035,
                        profile.x,
                        face * (0.05 + jitter.abs() * 0.03),
                    )
                })
                .collect();
            for pair in points.windows(2) {
                let width = Vec3::X * 0.005;
                let [a, b] = [pair[0] - width, pair[0] + width];
                let [c, d] = [pair[1] - width, pair[1] + width];
                positions.extend([a, b, c, b, d, c].map(|p| p.to_array()));
            }
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.compute_flat_normals();
    mesh
}

fn active_arc(seconds: f32, offset: usize) -> usize {
    ((seconds * 12.0) as usize + offset) % 3
}

#[derive(Component)]
pub struct StaffHalo {
    scale: Vec3,
}

#[derive(Component)]
pub struct StaffMote {
    index: usize,
}

/// Soft vertex-alpha disc, not a solid shell or an external texture. The edge
/// reaches zero opacity, so additive blending has no rectangular sprite border.
fn staff_halo_mesh() -> Mesh {
    const STEPS: usize = 48;
    let rings = [(0.0, 0.22), (0.25, 0.18), (0.55, 0.08), (1.0, 0.0)];
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    for pair in rings.windows(2) {
        for i in 0..STEPS {
            let a = i as f32 * std::f32::consts::TAU / STEPS as f32;
            let b = (i + 1) as f32 * std::f32::consts::TAU / STEPS as f32;
            for (angle, (radius, alpha)) in [
                (a, pair[0]),
                (a, pair[1]),
                (b, pair[1]),
                (a, pair[0]),
                (b, pair[1]),
                (b, pair[0]),
            ] {
                positions.push([angle.cos() * radius, angle.sin() * radius, 0.0]);
                colors.push([1.0, 1.0, 1.0, alpha]);
            }
        }
    }
    let count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
}

fn staff_mote_position(seconds: f32, index: usize) -> Vec3 {
    let phase = index as f32 * std::f32::consts::TAU / 12.0;
    let angle = seconds * 0.9 + phase;
    let radius = 0.40 + 0.08 * (phase * 2.0 + seconds).sin();
    Vec3::new(
        radius * angle.cos(),
        4.72 + 0.6 * (phase + seconds * 0.65).sin(),
        radius * angle.sin(),
    )
}

pub fn animate_staff(
    time: Res<Time>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    globals: Query<&GlobalTransform>,
    mut halos: Query<(&StaffHalo, &ChildOf, &mut Transform), Without<StaffMote>>,
    mut motes: Query<(&StaffMote, &mut Transform), Without<StaffHalo>>,
) {
    let seconds = time.elapsed_secs();
    if let Ok(camera) = cameras.single() {
        let camera_rotation = camera.rotation();
        for (halo, parent, mut transform) in &mut halos {
            if let Ok(parent) = globals.get(parent.parent()) {
                transform.rotation = parent.rotation().inverse() * camera_rotation;
                transform.scale = halo.scale * (1.0 + 0.06 * (seconds * 1.8).sin());
            }
        }
    }
    for (mote, mut transform) in &mut motes {
        transform.translation = staff_mote_position(seconds, mote.index);
        transform.scale = Vec3::splat(0.045 + 0.015 * (seconds * 2.0 + mote.index as f32).sin());
    }
}

pub fn animate(time: Res<Time>, mut arcs: Query<(&ElectricalArc, &mut Visibility)>) {
    for (arc, mut visibility) in &mut arcs {
        let next = if arc.variant == active_arc(time.elapsed_secs(), arc.offset) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != next {
            *visibility = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn switches_all_combinations_with_one_visible_group_and_bounded_cached_assets() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            TransformPlugin,
            bevy::camera::visibility::VisibilityPlugin,
        ))
        .init_asset::<WorldAsset>()
        .init_asset::<Mesh>()
        .init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .init_resource::<Assets<StandardMaterial>>();
        let carrier = app
            .world_mut()
            .spawn((
                EquipmentCarrier,
                Transform::default(),
                Visibility::Inherited,
            ))
            .id();
        for _ in 0..2 {
            for kind in [0.0, 1.0] {
                for color in 0..6 {
                    let selection = Selection::from_values(Some(kind), Some(f64::from(color)));
                    app.world_mut()
                        .run_system_once_with(apply, selection)
                        .unwrap();
                    // Simulate asynchronous imported meshes becoming ready, including
                    // instances whose groups have already been switched off.
                    let world = app.world_mut();
                    let roots: Vec<_> = world
                        .query_filtered::<Entity, With<ImportedGlaive>>()
                        .iter(world)
                        .collect();
                    for root in roots {
                        if world.get::<Children>(root).is_none() {
                            world.spawn((Mesh3d::default(), Transform::default(), ChildOf(root)));
                        }
                    }
                    world
                        .run_system_once(
                            |children: Query<&Children>,
                             meshes: Query<(), With<Mesh3d>>,
                             imported: Query<&ImportedGlaive>,
                             entities: Query<Entity, With<ImportedGlaive>>,
                             mut visibility: Query<&mut Visibility>| {
                                for entity in &entities {
                                    reveal_model(
                                        entity,
                                        &children,
                                        &meshes,
                                        &imported,
                                        &mut visibility,
                                    );
                                }
                            },
                        )
                        .unwrap();
                    app.update();
                    let world = app.world_mut();
                    let roots: Vec<_> = world
                        .query_filtered::<Entity, With<ImportedGlaive>>()
                        .iter(world)
                        .collect();
                    for root in roots {
                        let mut ancestor = root;
                        while world.get::<Selection>(ancestor).is_none() {
                            ancestor = world.get::<ChildOf>(ancestor).unwrap().parent();
                        }
                        let expected = *world.get::<Selection>(ancestor).unwrap() == selection;
                        assert_eq!(
                            world.get::<InheritedVisibility>(root).unwrap().get(),
                            expected
                        );
                        for child in world.get::<Children>(root).unwrap() {
                            assert_eq!(
                                world.get::<InheritedVisibility>(*child).unwrap().get(),
                                expected
                            );
                        }
                    }
                    let effects: Vec<_> = world.query_filtered::<Entity, Or<(With<StaffHalo>, With<StaffMote>, With<PointLight>)>>().iter(world).collect();
                    for effect in effects {
                        let mut ancestor = effect;
                        while world.get::<Selection>(ancestor).is_none() {
                            ancestor = world.get::<ChildOf>(ancestor).unwrap().parent();
                        }
                        assert_eq!(
                            world.get::<InheritedVisibility>(effect).unwrap().get(),
                            *world.get::<Selection>(ancestor).unwrap() == selection
                        );
                    }
                    let mut groups = world.query::<(&Selection, &Visibility, &ChildOf)>();
                    let visible: Vec<_> = groups
                        .iter(world)
                        .filter(|(_, visibility, _)| **visibility == Visibility::Inherited)
                        .collect();
                    assert_eq!(visible.len(), 1);
                    assert_eq!(*visible[0].0, selection);
                    assert_eq!(visible[0].2.parent(), carrier);
                }
            }
            let world = app.world_mut();
            assert_eq!(world.query::<&Selection>().iter(world).count(), 12);
            // Six groups, each with two glaives; six with one staff.
            assert_eq!(world.query::<&ImportedGlaive>().iter(world).count(), 18);
            assert_eq!(world.resource::<Assets<Mesh>>().len(), 66);
            assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 48);
            assert_eq!(world.query::<&StaffHalo>().iter(world).count(), 12);
            assert_eq!(world.query::<&StaffMote>().iter(world).count(), 72);
        }
    }

    #[test]
    fn staff_halo_is_soft_at_the_edge_and_motes_orbit_without_leaving_the_crown() {
        let mesh = staff_halo_mesh();
        let VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!("positions");
        };
        let VertexAttributeValues::Float32x4(colors) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap()
        else {
            panic!("colors");
        };
        assert_eq!(positions.len(), colors.len());
        for (position, color) in positions.iter().zip(colors) {
            let radius = Vec3::from(*position).length();
            assert!(radius <= 1.00001 && radius.is_finite());
            assert!((0.0..=0.22).contains(&color[3]));
            if radius > 0.999 {
                assert_eq!(color[3], 0.0);
            }
        }
        for index in 0..12 {
            assert_ne!(
                staff_mote_position(0.0, index),
                staff_mote_position(1.0, index)
            );
            for seconds in [0.0, 0.5, 10.0, 1000.0] {
                let p = staff_mote_position(seconds, index);
                assert!(p.is_finite() && p.x.abs() <= 0.48 && p.z.abs() <= 0.48);
                assert!((4.11..=5.33).contains(&p.y));
            }
        }
    }

    #[test]
    fn staff_halo_faces_camera_through_rotated_parent_and_updates_cached_transforms() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .add_systems(Update, animate_staff);
        let parent_rotation = Quat::from_rotation_y(0.8);
        let camera_rotation = Quat::from_rotation_y(-0.4) * Quat::from_rotation_x(0.2);
        let parent = app
            .world_mut()
            .spawn(GlobalTransform::from(Transform::from_rotation(
                parent_rotation,
            )))
            .id();
        app.world_mut().spawn((
            Camera3d::default(),
            GlobalTransform::from(Transform::from_rotation(camera_rotation)),
        ));
        let halo = app
            .world_mut()
            .spawn((
                StaffHalo { scale: Vec3::ONE },
                ChildOf(parent),
                Transform::default(),
            ))
            .id();
        let mote = app
            .world_mut()
            .spawn((StaffMote { index: 0 }, Transform::default()))
            .id();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(1));
        app.update();
        let transform = app.world().get::<Transform>(halo).unwrap();
        assert!((parent_rotation * transform.rotation).abs_diff_eq(camera_rotation, 1e-5));
        assert!(transform.scale.x > 1.0 && transform.scale.x <= 1.06);
        assert_eq!(
            app.world().get::<Transform>(mote).unwrap().translation,
            staff_mote_position(1.0, 0)
        );
    }

    #[test]
    fn crystal_body_is_lit_and_clears_bloom_threshold_in_every_palette() {
        let source = StandardMaterial {
            base_color: Color::srgb(0.035, 0.65, 0.9),
            emissive: LinearRgba::new(0.1, 1.8, 2.6, 1.0),
            metallic: 0.3,
            ..default()
        };
        for color in 0..6 {
            let variant = Selection::from_values(Some(1.0), Some(f64::from(color))).variant;
            let body = crystal_material(&source, variant);
            assert_eq!(body.emissive, variant.emission(3.2));
            let peak = body
                .emissive
                .red
                .max(body.emissive.green)
                .max(body.emissive.blue);
            assert!(peak > crate::graphics::weapon_bloom().prefilter.threshold * 1.5);
            assert_eq!(body.emissive_exposure_weight, 0.0);
            assert!(!body.unlit);
            assert_eq!(body.metallic, 0.15);
            assert!(body.perceptual_roughness >= 0.2);
            assert_ne!(body.base_color, variant.color());
            let inlay = tinted_material(&source, variant).unwrap();
            assert_eq!(inlay.emissive, variant.emission(2.6));
        }
        assert_eq!(source.emissive.blue, 2.6);
        assert_eq!(source.metallic, 0.3);
    }

    #[test]
    fn named_imported_crystal_gets_surface_tuning_without_changing_source_or_inlays() {
        let mut app = App::new();
        app.init_resource::<Assets<StandardMaterial>>();
        let source = StandardMaterial {
            emissive: LinearRgba::new(0.1, 1.8, 2.6, 1.0),
            ..default()
        };
        let handle = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(source.clone());
        let root = app.world_mut().spawn_empty().id();
        let body = app
            .world_mut()
            .spawn((
                MeshMaterial3d(handle.clone()),
                GltfMaterialName("Luminous cyan crystal".into()),
                ChildOf(root),
            ))
            .id();
        app.world_mut()
            .run_system_once_with(
                |In(root): In<Entity>,
                 children: Query<&Children>,
                 mut handles: Query<(
                    &mut MeshMaterial3d<StandardMaterial>,
                    Option<&GltfMaterialName>,
                )>,
                 mut materials: ResMut<Assets<StandardMaterial>>| {
                    recolor_model(root, Variant::Cyan, &children, &mut handles, &mut materials);
                },
                root,
            )
            .unwrap();
        let world = app.world();
        let materials = world.resource::<Assets<StandardMaterial>>();
        let tuned = &world
            .get::<MeshMaterial3d<StandardMaterial>>(body)
            .unwrap()
            .0;
        assert_ne!(tuned, &handle);
        assert_eq!(
            materials.get(tuned).unwrap().emissive,
            Variant::Cyan.emission(3.2)
        );
        assert_eq!(materials.get(&handle).unwrap().emissive, source.emissive);
    }

    #[test]
    fn cosmetic_selection_covers_all_six_palettes_and_rejects_invalid_values() {
        for (index, variant) in [
            Variant::Purple,
            Variant::Yellow,
            Variant::Red,
            Variant::Blue,
            Variant::Green,
            Variant::Cyan,
        ]
        .into_iter()
        .enumerate()
        {
            for kind in [0.0, 1.0] {
                assert_eq!(
                    Selection::from_values(Some(kind), Some(index as f64)),
                    Selection {
                        staff: kind == 1.0,
                        variant
                    }
                );
            }
        }
        for invalid in [
            None,
            Some(-1.0),
            Some(0.5),
            Some(6.0),
            Some(f64::NAN),
            Some(f64::INFINITY),
        ] {
            assert_eq!(
                Selection::from_values(invalid, invalid),
                Selection {
                    staff: false,
                    variant: Variant::Purple
                }
            );
        }
    }

    #[test]
    fn palettes_preserve_energy_strength_and_non_energy_finishes() {
        let source = StandardMaterial {
            base_color: Color::srgba(0.08, 1.0, 0.012, 0.7),
            emissive: LinearRgba::new(0.4, 5.0, 0.06, 1.0),
            metallic: 0.25,
            perceptual_roughness: 0.24,
            ..default()
        };
        for variant in [
            Variant::Purple,
            Variant::Yellow,
            Variant::Red,
            Variant::Blue,
            Variant::Green,
            Variant::Cyan,
        ] {
            let tinted = tinted_material(&source, variant).unwrap();
            assert_eq!(tinted.base_color, variant.color().with_alpha(0.7));
            assert_eq!(tinted.emissive, variant.emission(5.0));
            assert_eq!(tinted.metallic, source.metallic);
            assert_eq!(tinted.perceptual_roughness, source.perceptual_roughness);
        }
        for color in [
            Color::srgb(0.025, 0.043, 0.046),
            Color::srgb(0.32, 0.23, 0.08),
            Color::srgb(0.014, 0.024, 0.022),
        ] {
            assert!(
                tinted_material(
                    &StandardMaterial {
                        base_color: color,
                        ..default()
                    },
                    Variant::Red
                )
                .is_none()
            );
        }
        assert!(
            tinted_material(
                &StandardMaterial {
                    base_color: Color::srgb(0.009, 0.15, 0.047),
                    ..default()
                },
                Variant::Red
            )
            .is_some()
        );
    }

    #[test]
    fn instance_tints_reuse_clones_without_mutating_the_glb_or_each_other() {
        let mut app = App::new();
        app.init_resource::<Assets<StandardMaterial>>().add_systems(
            Update,
            |roots: Query<(Entity, &ImportedGlaive)>,
             children: Query<&Children>,
             mut handles: Query<(
                &mut MeshMaterial3d<StandardMaterial>,
                Option<&GltfMaterialName>,
            )>,
             mut materials: ResMut<Assets<StandardMaterial>>| {
                for (entity, model) in &roots {
                    recolor_model(
                        entity,
                        model.variant,
                        &children,
                        &mut handles,
                        &mut materials,
                    );
                }
            },
        );
        let source = StandardMaterial {
            base_color: Color::srgb(0.08, 1.0, 0.012),
            emissive: LinearRgba::new(0.4, 5.0, 0.06, 1.0),
            ..default()
        };
        let (energy, metal) = {
            let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
            (
                materials.add(source.clone()),
                materials.add(StandardMaterial {
                    base_color: Color::srgb(0.025, 0.043, 0.046),
                    ..default()
                }),
            )
        };
        let mut instances = Vec::new();
        for variant in [Variant::Purple, Variant::Blue] {
            let root = app
                .world_mut()
                .spawn(ImportedGlaive {
                    fallback: Entity::PLACEHOLDER,
                    variant,
                })
                .id();
            let a = app
                .world_mut()
                .spawn((MeshMaterial3d(energy.clone()), ChildOf(root)))
                .id();
            let b = app
                .world_mut()
                .spawn((MeshMaterial3d(energy.clone()), ChildOf(root)))
                .id();
            let untouched = app
                .world_mut()
                .spawn((MeshMaterial3d(metal.clone()), ChildOf(root)))
                .id();
            instances.push((variant, a, b, untouched));
        }
        app.update();
        let world = app.world();
        let materials = world.resource::<Assets<StandardMaterial>>();
        assert_eq!(materials.len(), 4); // Two originals, one energy clone per instance.
        assert_eq!(materials.get(&energy).unwrap().emissive, source.emissive);
        assert_eq!(
            materials.get(&energy).unwrap().base_color,
            source.base_color
        );
        let mut cloned = Vec::new();
        for (variant, a, b, untouched) in instances {
            let a = &world.get::<MeshMaterial3d<StandardMaterial>>(a).unwrap().0;
            let b = &world.get::<MeshMaterial3d<StandardMaterial>>(b).unwrap().0;
            assert_eq!(a, b);
            assert_ne!(a, &energy);
            assert_eq!(materials.get(a).unwrap().emissive, variant.emission(5.0));
            assert_eq!(
                world
                    .get::<MeshMaterial3d<StandardMaterial>>(untouched)
                    .unwrap()
                    .0,
                metal
            );
            cloned.push(a.id());
        }
        assert_ne!(cloned[0], cloned[1]);
    }

    #[test]
    fn imported_blade_plane_and_span_fit_the_existing_mounts() {
        let model = model_transform();
        assert_eq!(model.translation, Vec3::ZERO);
        let tip = model.transform_point(Vec3::X * 4.65);
        assert!(tip.abs_diff_eq(Vec3::Y * 1.08, 1e-6));
        let thickness = model.rotation * Vec3::Y;
        assert!(thickness.abs_diff_eq(Vec3::Z, 1e-6));
        for mount in mounts() {
            for tip in [Vec3::new(4.65, 0.0, -2.22), Vec3::new(-4.65, 0.0, 2.22)] {
                let point = mount.transform_point(model.transform_point(tip));
                assert!(point.y > 0.1 && point.z > 0.2);
            }
        }
    }

    #[test]
    fn staff_mount_is_normalized_y_up_and_keeps_tip_above_floor_behind_player() {
        let mount = staff_transform();
        let bottom = mount.transform_point(Vec3::new(0.0, 0.085, 0.0));
        let crown = mount.transform_point(Vec3::new(0.0, 5.33, 0.0));
        assert!(bottom.y > 0.1 && bottom.z >= 0.4);
        assert!(crown.y > 2.3 && crown.y < 2.6);
        assert!(crown.x > bottom.x);
        assert!(crown.z >= 0.4);
        let back_center = mount.transform_point(Vec3::Y * 2.665);
        assert!(back_center.x.abs() < 0.01);
        assert!((1.25..1.4).contains(&back_center.y));
        assert_eq!(back_center.z, 0.4);
        assert!((mount.scale.x * 5.33 - 2.4518).abs() < 1e-4);
    }

    #[test]
    fn imported_meshes_replace_only_their_own_fallback_when_ready() {
        let mut app = App::new();
        app.add_systems(
            Update,
            |children: Query<&Children>,
             meshes: Query<(), With<Mesh3d>>,
             imported: Query<&ImportedGlaive>,
             entities: Query<Entity, With<ImportedGlaive>>,
             mut visibility: Query<&mut Visibility>| {
                for entity in &entities {
                    reveal_model(entity, &children, &meshes, &imported, &mut visibility);
                }
            },
        );
        let fallback = app.world_mut().spawn(Visibility::Inherited).id();
        let other_fallback = app.world_mut().spawn(Visibility::Inherited).id();
        let model = app
            .world_mut()
            .spawn((
                ImportedGlaive {
                    fallback,
                    variant: Variant::Purple,
                },
                Visibility::Hidden,
            ))
            .id();
        app.world_mut().spawn((
            ImportedGlaive {
                fallback: other_fallback,
                variant: Variant::Blue,
            },
            Visibility::Hidden,
        ));
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(fallback).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            *app.world().get::<Visibility>(model).unwrap(),
            Visibility::Hidden
        );
        app.world_mut().spawn((Mesh3d::default(), ChildOf(model)));
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(fallback).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            *app.world().get::<Visibility>(model).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            *app.world().get::<Visibility>(other_fallback).unwrap(),
            Visibility::Inherited
        );
    }

    #[test]
    fn equipped_pair_shares_one_scene_and_keeps_two_visible_fallbacks() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(
                Startup,
                |mut commands: Commands,
                 mut meshes: ResMut<Assets<Mesh>>,
                 mut materials: ResMut<Assets<StandardMaterial>>| {
                    let weapons = BackBlades::new(&mut meshes, &mut materials, Variant::Purple);
                    commands
                        .spawn(Transform::default())
                        .with_children(|parent| {
                            weapons.spawn(parent, Some(&Handle::default()));
                        });
                },
            );
        app.update();
        let world = app.world_mut();
        let mut imported =
            world.query::<(&ImportedGlaive, &WorldAssetRoot, &Visibility, &ChildOf)>();
        let models: Vec<_> = imported.iter(world).collect();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].0.variant, Variant::Purple);
        assert_eq!(models[1].0.variant, Variant::Purple);
        assert_eq!(models[0].1.0, models[1].1.0);
        assert_ne!(models[0].0.fallback, models[1].0.fallback);
        for (model, _, visibility, parent) in models {
            assert_eq!(*visibility, Visibility::Hidden);
            assert_eq!(
                world.get::<Visibility>(model.fallback),
                Some(&Visibility::Inherited)
            );
            assert_eq!(world.get::<ChildOf>(model.fallback), Some(parent));
        }
        let mut lights = world.query::<&PointLight>();
        assert_eq!(lights.single(world).unwrap().color, Variant::Purple.color());
    }

    #[test]
    fn mounts_form_a_flush_intersecting_x_behind_the_fighter() {
        let [left, right] = mounts();
        let a = left.rotation * Vec3::Y;
        let b = right.rotation * Vec3::Y;
        assert!((a.dot(b) - 40.0_f32.to_radians().cos()).abs() < 1e-6);
        assert!(a.x * b.x < 0.0 && a.y > 0.0 && b.y > 0.0);
        for axis in [a, b] {
            assert!((axis.y - 20.0_f32.to_radians().cos()).abs() < 1e-6);
            assert!(axis.y > 0.9 && axis.x.abs() < 0.35);
        }
        assert_eq!(left.translation, right.translation);
        assert_eq!(left.translation.z, 0.36);
        for mount in mounts() {
            for end in [-1.0, 1.0] {
                assert!(mount.transform_point(Vec3::new(0.60, 1.08, 0.0) * end).y > 0.1);
            }
        }
    }

    #[test]
    fn blade_and_emissive_edges_have_finite_geometry_and_outward_normals() {
        for layer in [BladeLayer::Metal, BladeLayer::Edge, BladeLayer::Halo] {
            let mesh = blade_mesh(layer);
            let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("missing positions");
            };
            let Some(VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("missing normals");
            };
            assert_eq!(positions.len(), normals.len());
            assert_eq!(positions.len() % 3, 0);
            assert!(!positions.is_empty());
            for (position, normal) in positions.iter().zip(normals) {
                let p = Vec3::from_array(*position);
                let n = Vec3::from_array(*normal);
                assert!(p.is_finite() && n.is_finite());
                assert!((n.length() - 1.0).abs() < 1e-5);
                assert!(p.y >= 0.23 && p.y <= 1.08);
                if n.z.abs() > 0.1 && p.z.abs() > 1e-6 {
                    assert!(p.z * n.z > 0.0);
                }
            }
        }
    }

    #[test]
    fn broad_blade_faces_keep_the_thin_profile_and_single_sharp_point() {
        assert!(SECTIONS.iter().all(|(_, _, width)| *width <= 0.175 + 1e-6));
        let mesh = blade_mesh(BladeLayer::Metal);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("missing blade positions");
        };
        let tip = Vec3::new(0.60, 1.08, 0.0);
        let face_x: Vec<_> = positions
            .iter()
            .filter(|p| p[1] == 0.43)
            .map(|p| p[0])
            .collect();
        let min = face_x.iter().copied().fold(f32::INFINITY, f32::min);
        let max = face_x.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!((max - min - 0.35).abs() < 1e-6);
        assert!(positions.iter().any(|p| Vec3::from_array(*p) == tip));
        for p in positions {
            assert!(p[2].abs() <= RIDGE_DEPTH);
            if p[1] == tip.y {
                assert_eq!(Vec3::from_array(*p), tip);
            }
        }
    }

    #[test]
    fn crescent_is_empty_geometry_and_outer_rim_has_three_angular_teeth() {
        let mesh = blade_mesh(BladeLayer::Metal);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("missing blade positions");
        };
        let covers = |point: Vec2| {
            positions.as_chunks::<3>().0.iter().any(|triangle| {
                let [a, b, c] =
                    [triangle[0], triangle[1], triangle[2]].map(|p| Vec2::new(p[0], p[1]));
                if (b - a).perp_dot(c - a).abs() < 1e-8 {
                    return false;
                }
                let signs = [
                    (b - a).perp_dot(point - a),
                    (c - b).perp_dot(point - b),
                    (a - c).perp_dot(point - c),
                ];
                signs.iter().all(|s| *s >= -1e-6) || signs.iter().all(|s| *s <= 1e-6)
            })
        };
        // The middle of the old broad face is now genuinely absent, not painted dark.
        assert!(!covers(Vec2::new(0.12, 0.70)));
        assert!(covers(Vec2::new(0.30, 0.70)));
        let teeth = SECTIONS
            .windows(3)
            .filter(|triple| {
                let outer = |section: (f32, f32, f32)| section.1 + section.2;
                outer(triple[1]) > outer(triple[0]) && outer(triple[1]) > outer(triple[2])
            })
            .count();
        assert_eq!(teeth, 3);
        for &(y, center, width) in &SECTIONS {
            assert!(width >= 0.0);
            for side in [-1.0, 1.0] {
                assert!(
                    positions
                        .iter()
                        .any(|p| p[1] == y && (p[0] - (center + side * width)).abs() < 1e-6)
                );
            }
        }
    }

    #[test]
    fn wider_faces_do_not_widen_the_glowing_cutting_edge() {
        let mesh = blade_mesh(BladeLayer::Edge);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("missing edge positions");
        };
        for (height, center, width) in SECTIONS {
            for p in positions.iter().filter(|p| p[1] == height) {
                let distance_from_rim = width - (p[0] - center).abs();
                assert!(distance_from_rim >= -1e-6);
                assert!(distance_from_rim <= EDGE_BAND + 1e-6);
                assert!(p[2].abs() <= 0.010);
            }
        }
    }

    #[test]
    fn electricity_cycles_cached_variants_and_stays_finite() {
        assert_eq!(active_arc(0.0, 0), 0);
        assert_eq!(active_arc(0.09, 0), 1);
        assert_eq!(active_arc(0.18, 0), 2);
        assert_eq!(active_arc(0.26, 0), 0);
        assert_ne!(active_arc(0.0, 0), active_arc(0.0, 1));
        for variant in 0..3 {
            let mesh = lightning_mesh(variant);
            let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("missing arc positions");
            };
            assert_eq!(
                positions.len(),
                4 * (SECTIONS.len() - 1) * ARC_STEPS_PER_SECTION * 6
            );
            assert!(positions.iter().all(|p| Vec3::from_array(*p).is_finite()));
        }
    }

    #[test]
    fn crossed_pair_spawns_four_electrical_blade_ends_without_frame_allocations() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(
                Startup,
                |mut commands: Commands,
                 mut meshes: ResMut<Assets<Mesh>>,
                 mut materials: ResMut<Assets<StandardMaterial>>| {
                    let weapons = BackBlades::new(&mut meshes, &mut materials, Variant::Purple);
                    commands
                        .spawn((Name::new("Wearer"), Transform::default()))
                        .with_children(|parent| weapons.spawn(parent, None));
                },
            )
            .add_systems(Update, animate);
        app.update();
        let world = app.world_mut();
        let mut named = world.query::<(&Name, &Children)>();
        let wearer = named
            .iter(world)
            .find(|(name, _)| name.as_str() == "Wearer")
            .unwrap();
        assert_eq!(wearer.1.len(), 3); // Two glaives and one shared light.
        assert_eq!(
            named
                .iter(world)
                .filter(|(name, _)| name.as_str().starts_with("Storm glaive"))
                .count(),
            2
        );
        let mut arcs = world.query::<(&ElectricalArc, &Visibility)>();
        assert_eq!(arcs.iter(world).count(), 12); // Four ends, three cached variants.
        assert_eq!(
            arcs.iter(world)
                .filter(|(_, v)| **v == Visibility::Inherited)
                .count(),
            4
        );
        let meshes = world.resource::<Assets<Mesh>>().len();
        let entities = world.entities().len();
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(0.09));
        app.update();
        let world = app.world_mut();
        for (arc, visibility) in arcs.iter(world) {
            assert_eq!(
                *visibility == Visibility::Inherited,
                arc.variant == (1 + arc.offset) % 3
            );
        }
        assert_eq!(world.resource::<Assets<Mesh>>().len(), meshes);
        assert_eq!(world.entities().len(), entities);
    }
}
