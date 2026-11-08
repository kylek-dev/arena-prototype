//! Original procedural humanoids. Appearance is presentation data, not fighter rules.
use bevy::prelude::*;

use crate::animation::{Joint, JointKind};
use crate::graphics::{CharacterMeshes, CharacterPart};

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
        Body,
        Clothing,
        [0.0, 1.14, 0.0],
        [0.61, 0.36, 0.36],
    );
    add("Hips", Head, Dark, [0.0, 0.85, 0.0], [0.48, 0.30, 0.32]);
    add("Belt", Head, Dark, [0.0, 0.97, 0.0], [0.60, 0.09, 0.37]);
    add(
        "Buckle",
        Head,
        Clothing,
        [0.0, 0.97, -0.19],
        [0.10, 0.09, 0.04],
    );
    add("Neck", Body, Skin, [0.0, 1.48, 0.0], [0.16, 0.08, 0.16]);
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
            [x(0.32), 1.34, 0.0],
            [0.25, 0.26, 0.29],
        );
        add(
            "Upper arm",
            Body,
            Clothing,
            [x(0.38), 1.16, 0.0],
            [0.19, 0.15, 0.21],
        );
        add(
            "Forearm",
            Body,
            Skin,
            [x(0.40), 0.94, -0.025],
            [0.15, 0.14, 0.17],
        );
        add(
            "Hand",
            Head,
            Skin,
            [x(0.40), 0.76, -0.03],
            [0.15, 0.19, 0.13],
        );
        add(
            "Thigh",
            Body,
            Dark,
            [x(0.145), 0.65, 0.0],
            [0.22, 0.20, 0.25],
        );
        add(
            "Shin",
            Body,
            Dark,
            [x(0.145), 0.32, 0.0],
            [0.17, 0.18, 0.20],
        );
        add(
            "Boot",
            Head,
            Dark,
            [x(0.145), 0.11, -0.075],
            [0.23, 0.22, 0.39],
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
}

impl Humanoid {
    pub fn new(appearance: Appearance, materials: &mut Assets<StandardMaterial>) -> Self {
        Self {
            skin: materials.add(appearance.skin),
            hair: materials.add(appearance.hair),
            eyes: materials.add(appearance.eyes),
            white: materials.add(Color::srgb(0.92, 0.90, 0.84)),
            dark: materials.add(Color::srgb(0.045, 0.055, 0.065)),
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
            for side in [-1.0, 1.0] {
                for (upper, lower, origin, hinge) in [
                    (
                        JointKind::Shoulder,
                        JointKind::Elbow,
                        Vec3::new(side * 0.34, 1.34, 0.0),
                        Vec3::new(side * 0.40, 1.03, -0.015),
                    ),
                    (
                        JointKind::Hip,
                        JointKind::Knee,
                        Vec3::new(side * 0.145, 0.85, 0.0),
                        Vec3::new(side * 0.145, 0.47, 0.0),
                    ),
                ] {
                    body.spawn((
                        Joint {
                            actor,
                            kind: upper,
                            side,
                        },
                        Transform::from_translation(origin),
                        Visibility::default(),
                    ))
                    .with_children(|joint| {
                        for part in parts.iter().filter(|p| {
                            limb(p.name) == Some(upper) && p.position.x.signum() == side
                        }) {
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
        });
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
            Surface::Clothing => &self.appearance.clothing,
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
        "Shoulder" | "Upper arm" => Some(JointKind::Shoulder),
        "Forearm" | "Hand" => Some(JointKind::Elbow),
        "Thigh" => Some(JointKind::Hip),
        "Shin" | "Boot" => Some(JointKind::Knee),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                + 4
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
        assert_eq!(joints.iter(world).count(), 16);
        for (_, transform, children) in joints.iter(world) {
            assert_eq!(transform.rotation, Quat::IDENTITY);
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
