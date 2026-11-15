//! Player appearance from a customizable character kit (ADR 0040).
//!
//! The browser resolves the player's choices against the kit manifest into a
//! small render spec: which kit, which named part nodes are visible, per-slot
//! material tints/texture variants and an optional morph weight. This module
//! only applies that spec; it knows nothing about hairstyles or outfits.
//! Presentation only: nothing here reaches the simulation.
use std::sync::{Mutex, OnceLock};

use bevy::{gltf::GltfMaterialName, mesh::morph::MorphWeights, prelude::*};

use crate::character::{Profile, QUATERNIUS_FEMALE};

/// Manifest id meaning "assemble the player from a kit".
pub const KITS_MODEL: &str = "quaternius-kits";

#[derive(Clone, Debug, PartialEq)]
pub struct KitRef {
    pub id: String,
    pub path: String,
    pub sole: f32,
    pub scale: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialOverride {
    pub name: String,
    /// sRGB multiplier; components may slightly exceed 1 to lighten.
    pub tint: [f32; 3],
    /// Replacement base-colour texture, or `None` for the kit's own.
    pub texture: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub kit: KitRef,
    pub parts: Vec<(String, bool)>,
    pub materials: Vec<MaterialOverride>,
    pub morph: Option<(String, f32)>,
}

impl Spec {
    /// Parses the browser's render spec. Anything malformed is rejected whole
    /// rather than half-applied.
    pub fn parse(json: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let kit = value.get("kit")?;
        let finite = |v: &serde_json::Value| v.as_f64().filter(|n| n.is_finite()).map(|n| n as f32);
        let path = kit.get("path")?.as_str()?;
        // Asset paths stay inside the served character folder.
        if !path.starts_with("characters/") || path.contains("..") || !path.ends_with(".glb") {
            return None;
        }
        let kit = KitRef {
            id: kit.get("id")?.as_str()?.to_owned(),
            path: path.to_owned(),
            sole: finite(kit.get("sole")?)?,
            scale: finite(kit.get("scale")?).filter(|s| *s > 0.1 && *s < 10.0)?,
        };
        let parts = value
            .get("parts")?
            .as_array()?
            .iter()
            .map(|part| Some((part.get(0)?.as_str()?.to_owned(), part.get(1)?.as_bool()?)))
            .collect::<Option<Vec<_>>>()?;
        let materials = value
            .get("materials")?
            .as_array()?
            .iter()
            .map(|m| {
                let tint = m.get("tint")?.as_array()?;
                let tint = [0, 1, 2].map(|i| tint.get(i).and_then(finite));
                let texture = match m.get("texture") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(t) => {
                        let t = t.as_str()?;
                        if !t.starts_with("characters/") || t.contains("..") || !t.ends_with(".png")
                        {
                            return None;
                        }
                        Some(t.to_owned())
                    }
                };
                Some(MaterialOverride {
                    name: m.get("name")?.as_str()?.to_owned(),
                    tint: [tint[0]?, tint[1]?, tint[2]?].map(|c| c.clamp(0.0, 2.0)),
                    texture,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let morph = match value.get("morph") {
            None | Some(serde_json::Value::Null) => None,
            Some(m) => Some((
                m.get("node")?.as_str()?.to_owned(),
                finite(m.get("weight")?)?.clamp(0.0, 1.0),
            )),
        };
        Some(Self {
            kit,
            parts,
            materials,
            morph,
        })
    }

    pub fn visible(&self, name: &str) -> Option<bool> {
        self.parts
            .iter()
            .find(|(part, _)| part == name)
            .map(|(_, shown)| *shown)
    }
}

/// A stable `&'static Profile` per kit: the rest of the client keeps treating
/// profiles as static data. At most one entry per kit id is ever created.
pub fn kit_profile(kit: &KitRef) -> &'static Profile {
    static PROFILES: OnceLock<Mutex<Vec<&'static Profile>>> = OnceLock::new();
    let mut profiles = PROFILES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let top = kit.sole + crate::character::HEIGHT / kit.scale;
    if let Some(profile) = profiles
        .iter()
        .find(|p| p.id == kit.id && p.path == kit.path && p.sole == kit.sole && p.top == top)
    {
        return profile;
    }
    let profile: &'static Profile = Box::leak(Box::new(Profile {
        id: Box::leak(kit.id.clone().into_boxed_str()),
        path: Box::leak(kit.path.clone().into_boxed_str()),
        sole: kit.sole,
        top,
        // Every kit carries the same retargeted UAL1 clips.
        timing: QUATERNIUS_FEMALE.timing,
    }));
    profiles.push(profile);
    profile
}

/// The last spec and preview state received from the browser.
#[derive(Resource, Default)]
pub struct Desired {
    revision: Option<f64>,
    pub spec: Option<Spec>,
    /// Bumped whenever `spec` changes, so instances re-apply.
    pub generation: u64,
}

/// The close-up character view used by the customization screen.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct Preview {
    pub open: bool,
    /// Extra turn around the character, radians.
    pub yaw: f32,
    /// 0 = full body, 1 = face.
    pub zoom: f32,
    /// Eased zoom actually framed.
    pub shown_zoom: f32,
}

impl Preview {
    // Called by the browser bridge and tests; native builds read a stub.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn from_values(open: Option<f64>, yaw: Option<f64>, zoom: Option<f64>) -> Self {
        let finite = |v: Option<f64>| v.filter(|n| n.is_finite()).map(|n| n as f32);
        Self {
            open: open == Some(1.0),
            yaw: finite(yaw).unwrap_or(0.0),
            zoom: finite(zoom).unwrap_or(0.0).clamp(0.0, 1.0),
            shown_zoom: 0.0,
        }
    }

    /// Focus height above the feet and boom length for the eased zoom.
    pub fn framing(&self) -> (f32, f32) {
        let t = self.shown_zoom;
        (1.0 + 0.68 * t, 3.6 + (0.95 - 3.6) * t)
    }
}

/// Marks a spawned kit scene root: which kit it is and the spec generation
/// it shows.
#[derive(Component)]
pub struct KitInstance {
    pub path: String,
    pub applied: Option<u64>,
}
impl KitInstance {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_owned(),
            applied: None,
        }
    }
}

/// The player's body carrier, which kit scenes are spawned under.
#[derive(Component)]
pub struct PlayerBody;

/// The glTF material a mesh entity started with, before overrides.
#[derive(Component)]
pub struct SourceMaterial(StandardMaterial);

pub fn read(mut desired: ResMut<Desired>, mut preview: ResMut<Preview>, time: Res<Time>) {
    let next = crate::platform::read_preview();
    let shown = preview.shown_zoom;
    *preview = Preview {
        shown_zoom: shown + (next.zoom - shown) * (1.0 - (-8.0 * time.delta_secs()).exp()),
        ..next
    };
    let Some((revision, json)) = crate::platform::read_appearance() else {
        return;
    };
    if desired.revision == Some(revision) {
        return;
    }
    desired.revision = Some(revision);
    match Spec::parse(&json) {
        Some(spec) => {
            if desired.spec.as_ref() != Some(&spec) {
                desired.spec = Some(spec);
                desired.generation += 1;
            }
        }
        None => warn!("Ignoring malformed character appearance"),
    }
}

/// Loads a different kit when the spec names one. The current model stays
/// visible until the new one is ready (see `character::on_ready`).
pub fn switch_kit(
    mut commands: Commands,
    desired: Res<Desired>,
    server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut model: ResMut<crate::character::CharacterModel>,
    bodies: Query<Entity, With<PlayerBody>>,
) {
    let Some(spec) = &desired.spec else {
        return;
    };
    let Some(current) = model.profile else {
        return;
    };
    // Only kit-built players switch; legacy single models stay as they are.
    if !model.kit || current.path == spec.kit.path {
        return;
    }
    let profile = kit_profile(&spec.kit);
    *model = crate::character::CharacterModel::load(Some(profile), &server, &mut graphs);
    model.kit = true;
    let Some(scene) = model.scene.clone() else {
        return;
    };
    for body in &bodies {
        commands.entity(body).with_children(|body| {
            body.spawn((
                WorldAssetRoot(scene.clone()),
                profile.transform(),
                Visibility::Hidden,
                KitInstance::new(profile.path),
            ))
            .observe(crate::character::on_ready);
        });
    }
}

/// Applies part visibility, materials and morphs to ready kit instances whose
/// shown generation is stale.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn apply(
    mut commands: Commands,
    desired: Res<Desired>,
    server: Res<AssetServer>,
    model: Res<crate::character::CharacterModel>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut instances: Query<(Entity, &mut KitInstance)>,
    children: Query<&Children>,
    mut named: Query<(&Name, &mut Visibility), Without<crate::character::CharacterFallback>>,
    mut surfaces: Query<(
        &GltfMaterialName,
        &mut MeshMaterial3d<StandardMaterial>,
        Option<&SourceMaterial>,
    )>,
    mut morphs: Query<(&Name, &mut MorphWeights)>,
    mut fallback: Query<&mut Visibility, With<crate::character::CharacterFallback>>,
) {
    let Some(spec) = &desired.spec else {
        return;
    };
    if model.status() != "ready" {
        return;
    }
    for (root, mut instance) in &mut instances {
        if instance.applied == Some(desired.generation) {
            continue;
        }
        // Only the instance of the kit the spec names, once that kit is the
        // loaded model; an outgoing one keeps its look until it is replaced.
        if instance.path != spec.kit.path
            || model.profile.map(|p| p.path) != Some(spec.kit.path.as_str())
        {
            continue;
        }
        instance.applied = Some(desired.generation);
        reveal(&mut commands, root, &mut fallback);
        for entity in children.iter_descendants(root) {
            if let Ok((name, mut visibility)) = named.get_mut(entity)
                && let Some(shown) = spec.visible(name.as_str())
            {
                *visibility = if shown {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
            if let Ok((name, mut weights)) = morphs.get_mut(entity)
                && let Some((node, weight)) = &spec.morph
                && name.as_str() == node
                && let Some(first) = weights.weights_mut().first_mut()
            {
                *first = *weight;
            }
            let Ok((material_name, mut handle, source)) = surfaces.get_mut(entity) else {
                continue;
            };
            let Some(change) = spec.materials.iter().find(|m| m.name == material_name.0) else {
                continue;
            };
            let base = match source {
                Some(source) => source.0.clone(),
                None => {
                    let Some(original) = materials.get(&handle.0) else {
                        continue;
                    };
                    let original = original.clone();
                    commands
                        .entity(entity)
                        .insert(SourceMaterial(original.clone()));
                    original
                }
            };
            let next = overridden(base, change, &server);
            // Each mesh owns its copy, so overrides never leak into the shared
            // glTF asset (or into the next kit using it).
            if source.is_some()
                && let Some(mut existing) = materials.get_mut(&handle.0)
            {
                *existing = next;
            } else {
                handle.0 = materials.add(next);
            }
        }
    }
}

/// Shows a freshly applied kit and retires whatever stood in for it: the
/// previous kit, or the procedural fallback at startup.
fn reveal(
    commands: &mut Commands,
    root: Entity,
    fallback: &mut Query<&mut Visibility, With<crate::character::CharacterFallback>>,
) {
    commands.entity(root).insert(Visibility::Inherited);
    commands.queue(move |world: &mut World| {
        let others: Vec<Entity> = world
            .query_filtered::<Entity, With<KitInstance>>()
            .iter(world)
            .filter(|entity| *entity != root)
            .collect();
        for entity in others {
            world.despawn(entity);
        }
    });
    for mut visibility in fallback.iter_mut() {
        *visibility = Visibility::Hidden;
    }
}

fn overridden(
    mut material: StandardMaterial,
    change: &MaterialOverride,
    server: &AssetServer,
) -> StandardMaterial {
    let [r, g, b] = change.tint;
    let base = material.base_color.to_srgba();
    material.base_color = Color::srgba(base.red * r, base.green * g, base.blue * b, base.alpha);
    if let Some(path) = &change.texture {
        material.base_color_texture = Some(server.load(path.clone()));
    }
    material
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r#"{"kit":{"id":"human-male","path":"characters/kits/human-male.glb","sole":-0.0095,"scale":1.0656},
        "parts":[["Hair_Long",true],["Body_Torso",false]],
        "materials":[{"name":"Skin","tint":[0.8,0.6,0.5],"texture":null},
                     {"name":"Eyes","tint":[1,1,1],"texture":"characters/kits/textures/eye-blue.png"}],
        "morph":null}"#;

    #[test]
    fn spec_parses_parts_materials_and_kit() {
        let spec = Spec::parse(SPEC).unwrap();
        assert_eq!(spec.kit.path, "characters/kits/human-male.glb");
        assert_eq!(spec.visible("Hair_Long"), Some(true));
        assert_eq!(spec.visible("Body_Torso"), Some(false));
        assert_eq!(spec.visible("Hair_Buns"), None);
        assert_eq!(
            spec.materials[1].texture.as_deref(),
            Some("characters/kits/textures/eye-blue.png")
        );
        assert_eq!(spec.morph, None);
    }

    #[test]
    fn malformed_or_escaping_specs_are_rejected_whole() {
        for bad in [
            SPEC.replace("characters/kits/human-male.glb", "../secret.glb"),
            SPEC.replace("characters/kits/human-male.glb", "https://x/y.glb"),
            SPEC.replace("eye-blue.png", "../../eye.png"),
            SPEC.replace("[0.8,0.6,0.5]", "[0.8,0.6]"),
            SPEC.replace("1.0656", "0"),
            SPEC.replace("true]", "\"yes\"]"),
            "not json".into(),
        ] {
            assert_eq!(Spec::parse(&bad), None, "{bad}");
        }
        let morph = SPEC.replace(
            "\"morph\":null",
            "\"morph\":{\"node\":\"Body_Full\",\"weight\":4}",
        );
        assert_eq!(
            Spec::parse(&morph).unwrap().morph,
            Some(("Body_Full".into(), 1.0))
        );
    }

    #[test]
    fn kit_profiles_are_shared_and_use_one_scale() {
        let spec = Spec::parse(SPEC).unwrap();
        let a = kit_profile(&spec.kit);
        assert!(std::ptr::eq(a, kit_profile(&spec.kit)));
        let transform = a.transform();
        assert!((transform.scale.x - spec.kit.scale).abs() < 1e-4);
        // Bare soles sit on the floor, like every other profile.
        assert!((transform.transform_point(Vec3::Y * spec.kit.sole).y - 0.01).abs() < 1e-4);
        assert_eq!(a.timing, QUATERNIUS_FEMALE.timing);
    }

    #[test]
    fn preview_values_are_bounded_and_framing_zooms_toward_the_face() {
        let closed = Preview::from_values(None, Some(f64::NAN), None);
        assert!(!closed.open && closed.yaw == 0.0);
        let mut open = Preview::from_values(Some(1.0), Some(0.5), Some(7.0));
        assert!(open.open && open.zoom == 1.0);
        let body = open.framing();
        open.shown_zoom = 1.0;
        let face = open.framing();
        assert!(face.0 > body.0 && face.1 < body.1);
    }

    #[test]
    fn overrides_tint_the_source_and_swap_only_named_textures() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>();
        let server = app.world().resource::<AssetServer>().clone();
        let source = StandardMaterial {
            base_color: Color::WHITE,
            ..default()
        };
        let spec = Spec::parse(SPEC).unwrap();
        let skin = overridden(source.clone(), &spec.materials[0], &server);
        let srgb = skin.base_color.to_srgba();
        assert!((srgb.red - 0.8).abs() < 1e-4 && (srgb.blue - 0.5).abs() < 1e-4);
        assert!(skin.base_color_texture.is_none());
        let eyes = overridden(source, &spec.materials[1], &server);
        assert!(eyes.base_color_texture.is_some());
    }

    #[test]
    fn apply_shows_chosen_parts_copies_materials_and_retires_the_previous_kit() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<StandardMaterial>()
            .add_systems(Update, apply);
        let spec = Spec::parse(SPEC).unwrap();
        app.insert_resource(crate::character::CharacterModel::ready(kit_profile(
            &spec.kit,
        )));
        app.insert_resource(Desired {
            revision: Some(1.0),
            spec: Some(spec),
            generation: 1,
        });
        let shared = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let fallback = app
            .world_mut()
            .spawn((
                crate::character::CharacterFallback,
                Name::new("Procedural humanoid"),
                Visibility::Inherited,
            ))
            .id();
        let previous = app
            .world_mut()
            .spawn(KitInstance {
                path: "characters/kits/human-female.glb".into(),
                applied: Some(0),
            })
            .id();
        let mut hair = Entity::PLACEHOLDER;
        let mut torso = Entity::PLACEHOLDER;
        let mut skin = Entity::PLACEHOLDER;
        let root = app
            .world_mut()
            .spawn((
                KitInstance::new("characters/kits/human-male.glb"),
                Visibility::Hidden,
            ))
            .with_children(|root| {
                hair = root
                    .spawn((Name::new("Hair_Long"), Visibility::Hidden))
                    .id();
                torso = root
                    .spawn((Name::new("Body_Torso"), Visibility::Inherited))
                    .with_children(|node| {
                        skin = node
                            .spawn((
                                GltfMaterialName("Skin".into()),
                                MeshMaterial3d(shared.clone()),
                            ))
                            .id();
                    })
                    .id();
            })
            .id();
        app.update();
        let world = app.world();
        assert_eq!(world.get::<Visibility>(hair), Some(&Visibility::Inherited));
        assert_eq!(world.get::<Visibility>(torso), Some(&Visibility::Hidden));
        assert_eq!(world.get::<Visibility>(root), Some(&Visibility::Inherited));
        assert_eq!(world.get::<Visibility>(fallback), Some(&Visibility::Hidden));
        assert!(world.get_entity(previous).is_err(), "previous kit retired");
        let own = &world
            .get::<MeshMaterial3d<StandardMaterial>>(skin)
            .unwrap()
            .0;
        assert_ne!(own, &shared, "overrides use a per-mesh copy");
        let materials = world.resource::<Assets<StandardMaterial>>();
        assert_eq!(materials.get(&shared).unwrap().base_color, Color::WHITE);
        let tinted = materials.get(own).unwrap().base_color.to_srgba();
        assert!((tinted.red - 0.8).abs() < 1e-4);
        // Re-applying the same generation is a no-op; a new one edits the copy.
        let own = own.clone();
        app.update();
        let mut desired = app.world_mut().resource_mut::<Desired>();
        desired.spec.as_mut().unwrap().materials[0].tint = [0.5, 0.5, 0.5];
        desired.generation = 2;
        app.update();
        let world = app.world();
        assert_eq!(
            world
                .get::<MeshMaterial3d<StandardMaterial>>(skin)
                .unwrap()
                .0,
            own
        );
        let red = world
            .resource::<Assets<StandardMaterial>>()
            .get(&own)
            .unwrap()
            .base_color
            .to_srgba()
            .red;
        assert!((red - 0.5).abs() < 1e-4);
    }
}
