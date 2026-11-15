//! Original procedural humanoids. Appearance is presentation data, not fighter rules.
use bevy::prelude::*;

use crate::animation::{Joint, JointKind};
use crate::graphics::{CharacterMeshes, CharacterPart};
use crate::textures::PaintedTextures;

#[derive(Clone, Copy)]
pub(crate) enum HairStyle {
    Cropped,
    Crest,
}

/// A future character editor can supply this without changing the simulation or
/// replacing the model builder. No persistence/network appearance contract yet.
pub(crate) struct Appearance {
    pub skin: Color,
    pub hair: Color,
    pub eyes: Color,
    pub clothing: Handle<StandardMaterial>,
    /// Multipliers relative to the roughly two-metre prototype silhouette.
    pub height: f32,
    pub build: f32,
    pub hairstyle: HairStyle,
}

impl Appearance {
    pub fn practice(index: usize, clothing: Handle<StandardMaterial>) -> Self {
        if index == 0 {
            Self {
                skin: Color::srgb(0.64, 0.40, 0.27),
                hair: Color::srgb(0.065, 0.045, 0.035),
                eyes: Color::srgb(0.12, 0.52, 0.39),
                clothing,
                height: 1.0,
                build: 1.0,
                hairstyle: HairStyle::Cropped,
            }
        } else {
            Self {
                skin: Color::srgb(0.84, 0.62, 0.45),
                hair: Color::srgb(0.23, 0.075, 0.035),
                eyes: Color::srgb(0.19, 0.34, 0.60),
                clothing,
                height: 1.0,
                build: 1.06,
                hairstyle: HairStyle::Crest,
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Surface {
    Skin,
    Hair,
    Eyes,
    White,
    Dark,
    Clothing,
    Trousers,
    Leather,
}

struct Part {
    name: &'static str,
    shape: CharacterPart,
    surface: Surface,
    position: Vec3,
    scale: Vec3,
}

fn parts(style: HairStyle) -> Vec<Part> {
    use CharacterPart::{Body, Head};
    use Surface::*;
    let mut parts = Vec::new();
    let mut add = |name, shape, surface, position: [f32; 3], scale: [f32; 3]| {
        parts.push(Part {
            name,
            shape,
            surface,
            position: position.into(),
            scale: scale.into(),
        });
    };
    // Feet on y=0; forward is -Z, matching movement, picking and back mounts.
    add(
        "Tunic",
        Head,
        Clothing,
        [0.0, 1.14, 0.0],
        [0.56, 0.66, 0.36],
    );
    add("Hips", Head, Trousers, [0.0, 0.85, 0.0], [0.44, 0.30, 0.29]);
    add("Belt", Head, Leather, [0.0, 0.94, 0.0], [0.44, 0.10, 0.30]);
    add(
        "Buckle",
        Head,
        Clothing,
        [0.0, 0.94, -0.155],
        [0.10, 0.09, 0.04],
    );
    add("Neck", Body, Skin, [0.0, 1.48, 0.0], [0.15, 0.08, 0.15]);
    add("Head", Head, Skin, [0.0, 1.69, -0.015], [0.39, 0.46, 0.36]);
    add(
        "Nose",
        Head,
        Skin,
        [0.0, 1.68, -0.196],
        [0.075, 0.105, 0.10],
    );
    add(
        "Mouth",
        Head,
        Dark,
        [0.0, 1.585, -0.176],
        [0.115, 0.018, 0.018],
    );
    add(
        "Hair cap",
        Head,
        Hair,
        [0.0, 1.825, 0.005],
        [0.41, 0.22, 0.38],
    );
    add(
        "Hair back",
        Head,
        Hair,
        [0.0, 1.73, 0.115],
        [0.37, 0.27, 0.15],
    );
    if matches!(style, HairStyle::Crest) {
        add(
            "Hair crest",
            Head,
            Hair,
            [0.0, 1.91, -0.005],
            [0.15, 0.18, 0.31],
        );
    } else {
        add(
            "Swept fringe",
            Head,
            Hair,
            [-0.055, 1.825, -0.13],
            [0.29, 0.12, 0.16],
        );
    }
    for side in [-1.0, 1.0] {
        let x = |value: f32| side * value;
        add("Ear", Head, Skin, [x(0.20), 1.69, 0.0], [0.085, 0.13, 0.09]);
        add(
            "Eye white",
            Head,
            White,
            [x(0.083), 1.725, -0.177],
            [0.105, 0.067, 0.037],
        );
        add(
            "Iris",
            Head,
            Eyes,
            [x(0.083), 1.725, -0.196],
            [0.045, 0.050, 0.014],
        );
        add(
            "Pupil",
            Head,
            Dark,
            [x(0.083), 1.725, -0.204],
            [0.021, 0.033, 0.009],
        );
        add(
            "Brow",
            Head,
            Hair,
            [x(0.083), 1.775, -0.176],
            [0.115, 0.025, 0.035],
        );
        add(
            "Shoulder",
            Head,
            Clothing,
            [x(0.26), 1.34, 0.0],
            [0.21, 0.24, 0.24],
        );
        add(
            "Upper arm",
            Head,
            Clothing,
            [x(0.315), 1.19, 0.0],
            [0.17, 0.36, 0.19],
        );
        add(
            "Forearm",
            Head,
            Skin,
            [x(0.35), 0.94, -0.025],
            [0.13, 0.31, 0.15],
        );
        add(
            "Hand",
            Head,
            Skin,
            [x(0.35), 0.76, -0.03],
            [0.13, 0.19, 0.12],
        );
        add(
            "Elbow",
            Head,
            Skin,
            [x(0.35), 1.04, -0.015],
            [0.14, 0.16, 0.16],
        );
        add(
            "Wrist",
            Head,
            Skin,
            [x(0.35), 0.82, -0.03],
            [0.10, 0.13, 0.11],
        );
        add(
            "Thumb",
            Head,
            Skin,
            [x(0.295), 0.77, -0.07],
            [0.065, 0.11, 0.08],
        );
        add(
            "Thigh",
            Head,
            Trousers,
            [x(0.155), 0.66, 0.0],
            [0.19, 0.45, 0.22],
        );
        add(
            "Shin",
            Head,
            Trousers,
            [x(0.155), 0.32, 0.0],
            [0.15, 0.39, 0.18],
        );
        add(
            "Knee",
            Head,
            Trousers,
            [x(0.155), 0.47, 0.0],
            [0.165, 0.19, 0.19],
        );
        add(
            "Ankle",
            Head,
            Leather,
            [x(0.155), 0.15, 0.0],
            [0.13, 0.17, 0.15],
        );
        add(
            "Boot",
            Head,
            Leather,
            [x(0.155), 0.11, -0.075],
            [0.20, 0.21, 0.36],
        );
    }
    parts
}

pub(crate) struct Humanoid {
    appearance: Appearance,
    skin: Handle<StandardMaterial>,
    hair: Handle<StandardMaterial>,
    eyes: Handle<StandardMaterial>,
    white: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    clothing: Handle<StandardMaterial>,
    trousers: Handle<StandardMaterial>,
    leather: Handle<StandardMaterial>,
}

/// Painted textures darken undersides; lift the base colour (in linear space)
/// so the lit top keeps roughly the previous flat colour.
fn lift(color: Color, factor: f32) -> Color {
    let c = color.to_linear();
    Color::linear_rgba(
        (c.red * factor).min(1.0),
        (c.green * factor).min(1.0),
        (c.blue * factor).min(1.0),
        c.alpha,
    )
}

impl Humanoid {
    pub fn new(
        appearance: Appearance,
        materials: &mut Assets<StandardMaterial>,
        painted: &PaintedTextures,
    ) -> Self {
        let painted_matte =
            |base_color, texture: &Handle<Image>, perceptual_roughness| StandardMaterial {
                base_color,
                base_color_texture: Some(texture.clone()),
                perceptual_roughness,
                reflectance: 0.3,
                ..default()
            };
        // The accent material is shared with arena markers; clone its colour
        // into a textured cloth material instead of texturing the markers.
        let accent = materials
            .get(&appearance.clothing)
            .map_or(Color::WHITE, |material| material.base_color);
        Self {
            skin: materials.add(painted_matte(
                lift(appearance.skin, 1.06),
                &painted.skin,
                0.72,
            )),
            hair: materials.add(painted_matte(
                lift(appearance.hair, 1.15),
                &painted.hair,
                0.8,
            )),
            eyes: materials.add(appearance.eyes),
            white: materials.add(Color::srgb(0.92, 0.90, 0.84)),
            dark: materials.add(StandardMaterial {
                base_color: Color::srgb(0.045, 0.055, 0.065),
                perceptual_roughness: 0.85,
                ..default()
            }),
            clothing: materials.add(painted_matte(lift(accent, 1.12), &painted.cloth, 0.85)),
            trousers: materials.add(painted_matte(
                Color::srgb(0.19, 0.21, 0.28),
                &painted.cloth,
                0.9,
            )),
            leather: materials.add(painted_matte(
                Color::srgb(0.40, 0.27, 0.18),
                &painted.leather,
                0.75,
            )),
            appearance,
        }
    }

    pub fn spawn(&self, parent: &mut ChildSpawnerCommands, meshes: &CharacterMeshes, actor: usize) {
        let mut root = parent.spawn((
            Name::new("Procedural humanoid"),
            Transform::from_scale(Vec3::new(
                self.appearance.build,
                self.appearance.height,
                1.0,
            )),
            Visibility::default(),
        ));
        if actor == 0 {
            root.insert(crate::character::CharacterFallback);
        }
        root.with_children(|body| {
            let parts = parts(self.appearance.hairstyle);
            for part in parts.iter().filter(|part| limb(part.name).is_none()) {
                self.spawn_part(body, meshes, part, Vec3::ZERO);
            }
            let chest_origin = Vec3::new(0.0, 1.02, 0.0);
            body.spawn((
                Joint {
                    actor,
                    kind: JointKind::Chest,
                    side: 0.0,
                },
                Transform::from_translation(chest_origin),
                Visibility::default(),
            ))
            .with_children(|chest| {
                for part in parts
                    .iter()
                    .filter(|p| limb(p.name) == Some(JointKind::Chest))
                {
                    self.spawn_part(chest, meshes, part, chest_origin);
                }
                let head_origin = Vec3::new(0.0, 1.49, 0.0);
                chest
                    .spawn((
                        Joint {
                            actor,
                            kind: JointKind::Head,
                            side: 0.0,
                        },
                        Transform::from_translation(head_origin - chest_origin),
                        Visibility::default(),
                    ))
                    .with_children(|head| {
                        for part in parts
                            .iter()
                            .filter(|p| limb(p.name) == Some(JointKind::Head))
                        {
                            self.spawn_part(head, meshes, part, head_origin);
                        }
                    });
                self.spawn_limbs(chest, meshes, &parts, actor, true, chest_origin);
            });
            self.spawn_limbs(body, meshes, &parts, actor, false, Vec3::ZERO);
        });
    }

    fn spawn_limbs(
        &self,
        parent: &mut ChildSpawnerCommands,
        meshes: &CharacterMeshes,
        parts: &[Part],
        actor: usize,
        arms: bool,
        parent_origin: Vec3,
    ) {
        for side in [-1.0, 1.0] {
            let (upper, lower, origin, hinge) = if arms {
                (
                    JointKind::Shoulder,
                    JointKind::Elbow,
                    Vec3::new(side * 0.27, 1.34, 0.0),
                    Vec3::new(side * 0.35, 1.04, -0.015),
                )
            } else {
                (
                    JointKind::Hip,
                    JointKind::Knee,
                    Vec3::new(side * 0.155, 0.85, 0.0),
                    Vec3::new(side * 0.155, 0.47, 0.0),
                )
            };
            parent
                .spawn((
                    Joint {
                        actor,
                        kind: upper,
                        side,
                    },
                    Transform::from_translation(origin - parent_origin),
                    Visibility::default(),
                ))
                .with_children(|joint| {
                    for part in parts
                        .iter()
                        .filter(|p| limb(p.name) == Some(upper) && p.position.x.signum() == side)
                    {
                        self.spawn_part(joint, meshes, part, origin);
                    }
                    joint
                        .spawn((
                            Joint {
                                actor,
                                kind: lower,
                                side,
                            },
                            Transform::from_translation(hinge - origin),
                            Visibility::default(),
                        ))
                        .with_children(|joint| {
                            for part in parts.iter().filter(|p| {
                                limb(p.name) == Some(lower) && p.position.x.signum() == side
                            }) {
                                self.spawn_part(joint, meshes, part, hinge);
                            }
                        });
                });
        }
    }

    fn spawn_part(
        &self,
        parent: &mut ChildSpawnerCommands,
        meshes: &CharacterMeshes,
        part: &Part,
        origin: Vec3,
    ) {
        let index = match part.shape {
            CharacterPart::Body => 0,
            CharacterPart::Head => 1,
        };
        let material = match part.surface {
            Surface::Skin => &self.skin,
            Surface::Hair => &self.hair,
            Surface::Eyes => &self.eyes,
            Surface::White => &self.white,
            Surface::Dark => &self.dark,
            Surface::Clothing => &self.clothing,
            Surface::Trousers => &self.trousers,
            Surface::Leather => &self.leather,
        };
        parent.spawn((
            Name::new(part.name),
            Mesh3d(meshes.handles(true)[index].clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(part.position - origin).with_scale(part.scale),
            part.shape,
        ));
    }
}

fn limb(name: &str) -> Option<JointKind> {
    match name {
        "Tunic" | "Neck" => Some(JointKind::Chest),
        "Head" | "Nose" | "Mouth" | "Hair cap" | "Hair back" | "Hair crest" | "Swept fringe"
        | "Ear" | "Eye white" | "Iris" | "Pupil" | "Brow" => Some(JointKind::Head),
        "Shoulder" | "Upper arm" => Some(JointKind::Shoulder),
        "Elbow" | "Forearm" | "Wrist" | "Hand" | "Thumb" => Some(JointKind::Elbow),
        "Thigh" => Some(JointKind::Hip),
        "Knee" | "Shin" | "Ankle" | "Boot" => Some(JointKind::Knee),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_limb_segments_overlap_at_the_joints_instead_of_leaving_hard_cuffs() {
        let parts = parts(HairStyle::Cropped);
        for side in [-1.0, 1.0] {
            let part = |name| {
                parts
                    .iter()
                    .find(|p| p.name == name && p.position.x.signum() == side)
                    .unwrap()
            };
            for (a, b) in [
                ("Shoulder", "Upper arm"),
                ("Upper arm", "Elbow"),
                ("Elbow", "Forearm"),
                ("Forearm", "Wrist"),
                ("Wrist", "Hand"),
                ("Hand", "Thumb"),
                ("Thigh", "Knee"),
                ("Knee", "Shin"),
                ("Shin", "Ankle"),
                ("Ankle", "Boot"),
            ] {
                let (a, b) = (part(a), part(b));
                assert!(matches!(a.shape, CharacterPart::Head));
                assert!(matches!(b.shape, CharacterPart::Head));
                // Centers lie inside the Minkowski sum of the two ellipsoids.
                let separation = (a.position - b.position) / ((a.scale + b.scale) * 0.5);
                assert!(
                    separation.length() < 1.0,
                    "{} / {} have a gap",
                    a.name,
                    b.name
                );
            }
        }
        let torso = parts.iter().find(|p| p.name == "Tunic").unwrap();
        assert!(matches!(torso.shape, CharacterPart::Head));
        assert!(torso.scale.y > torso.scale.x);
    }

    #[test]
    fn fallback_is_one_subtree_and_all_parts_use_the_shared_quality_cache() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Time>()
            .init_resource::<crate::Practice>()
            .init_resource::<crate::animation::Settings>()
            .add_systems(
                Startup,
                |mut commands: Commands,
                 mut meshes: ResMut<Assets<Mesh>>,
                 mut materials: ResMut<Assets<StandardMaterial>>| {
                    let cache = CharacterMeshes::new(&mut meshes);
                    let clothing = materials.add(Color::srgb(0.1, 0.5, 0.5));
                    for index in 0..2 {
                        let model = Humanoid::new(
                            Appearance::practice(index, clothing.clone()),
                            &mut materials,
                            &PaintedTextures::default(),
                        );
                        commands
                            .spawn((crate::FighterVisual(index), Transform::default()))
                            .with_children(|parent| {
                                parent
                                    .spawn((
                                        crate::animation::Animator::new(index),
                                        Transform::default(),
                                    ))
                                    .with_children(|parent| model.spawn(parent, &cache, index));
                            });
                    }
                    commands.insert_resource(cache);
                },
            )
            .add_systems(
                Update,
                (crate::graphics::apply, crate::animation::animate).chain(),
            );
        app.update();
        let world = app.world_mut();
        let mut fallbacks = world.query_filtered::<
            (&Children, &mut Visibility),
            With<crate::character::CharacterFallback>,
        >();
        assert_eq!(fallbacks.iter(world).count(), 1);
        let (children, mut visibility) = fallbacks.single_mut(world).unwrap();
        assert_eq!(
            children.len(),
            parts(HairStyle::Cropped)
                .iter()
                .filter(|p| limb(p.name).is_none())
                .count()
                + 3
        );
        *visibility = Visibility::Hidden;
        let mut parts_query = world.query::<(&CharacterPart, &Mesh3d)>();
        let cache = world.resource::<CharacterMeshes>();
        assert_eq!(
            parts_query.iter(world).count(),
            parts(HairStyle::Cropped).len() + parts(HairStyle::Crest).len()
        );
        for (part, mesh) in parts_query.iter(world) {
            let index = match part {
                CharacterPart::Body => 0,
                CharacterPart::Head => 1,
            };
            assert_eq!(mesh.0, cache.handles(true)[index]);
        }
        let mut joints = world.query::<(&Joint, &Transform, &Children)>();
        assert_eq!(joints.iter(world).count(), 20);
        for (_, transform, children) in joints.iter(world) {
            assert!(transform.rotation.is_finite());
            assert!(!children.is_empty());
        }
        let entities = world.entities().len();
        let mesh_count = world.resource::<Assets<Mesh>>().len();
        for _ in 0..60 {
            let world = app.world_mut();
            world
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
            let mut fighters = world.query_filtered::<&mut Transform, With<crate::FighterVisual>>();
            for mut transform in fighters.iter_mut(world) {
                transform.translation.z -= 0.05;
            }
            app.update();
        }
        let world = app.world_mut();
        assert!(
            joints
                .iter(world)
                .any(|(_, transform, _)| !transform.rotation.abs_diff_eq(Quat::IDENTITY, 0.01))
        );
        assert_eq!(world.entities().len(), entities);
        assert_eq!(world.resource::<Assets<Mesh>>().len(), mesh_count);
    }

    #[test]
    fn slim_build_keeps_torso_and_limbs_narrow_without_gaps_to_the_hands() {
        let parts = parts(HairStyle::Cropped);
        let part = |name| parts.iter().find(|p| p.name == name).unwrap();
        let outer = |p: &Part| p.position.x.abs() + p.scale.x / 2.0;
        let inner = |p: &Part| p.position.x.abs() - p.scale.x / 2.0;
        let torso = part("Tunic");
        assert!(torso.scale.x <= 0.56 && torso.scale.x / torso.scale.y < 0.9);
        assert!(part("Hips").scale.x < torso.scale.x);
        // Arm span stays well under the previous 0.93 m silhouette.
        assert!(parts.iter().map(outer).fold(0.0, f32::max) <= 0.42);
        for name in ["Upper arm", "Forearm", "Thigh", "Shin"] {
            let limb = part(name);
            assert!(limb.scale.x <= 0.19 && limb.scale.z <= 0.22, "{name}");
        }
        // Hanging hands still clear the thighs in the bind pose.
        assert!(inner(part("Hand")) - outer(part("Thigh")) >= 0.03);
    }

    #[test]
    fn both_styles_have_paired_limbs_and_forward_facial_features() {
        for style in [HairStyle::Cropped, HairStyle::Crest] {
            let parts = parts(style);
            for name in [
                "Upper arm",
                "Forearm",
                "Hand",
                "Thigh",
                "Shin",
                "Boot",
                "Iris",
                "Pupil",
            ] {
                let pair: Vec<_> = parts.iter().filter(|p| p.name == name).collect();
                assert_eq!(pair.len(), 2, "{name}");
                assert_eq!(pair[0].position.x, -pair[1].position.x);
            }
            for part in &parts {
                assert!(part.scale.min_element() > 0.0);
                if matches!(part.name, "Nose" | "Mouth" | "Iris" | "Pupil") {
                    assert!(part.position.z < 0.0);
                }
                // Sphere diameter=1, capsule full height=2 before scaling.
                let half_height = match part.shape {
                    CharacterPart::Body => part.scale.y,
                    CharacterPart::Head => part.scale.y / 2.0,
                };
                assert!(part.position.y - half_height >= -0.0001);
                assert!(part.position.y + half_height <= 2.01);
            }
        }
    }
}
