//! Fighter appearance from customizable character kits (ADR 0040).
//!
//! The browser resolves each fighter's look (local player, practice opponents,
//! or networked peers) against the kit manifest into a render spec: which kit,
//! which named part nodes are visible, per-slot material tints/texture variants
//! and morph weights (a race build, face presets). This module only applies those specs per actor;
//! it knows nothing about hairstyles or outfits.
//! Presentation only: nothing here reaches the simulation.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use arena_protocol::MAX_FIGHTERS;
use bevy::{gltf::GltfMaterialName, mesh::morph::MorphWeights, prelude::*};

use crate::character::{
    CharacterFallback, CharacterModel, FallbackActor, KitCache, Profile, QUATERNIUS_FEMALE,
};

/// Manifest id meaning "assemble fighters from customizable kits".
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
    /// Morph target weights per named node, in the node's own target order
    /// (face presets, ADR 0053). Empty for kits without them.
    pub morphs: Vec<(String, Vec<f32>)>,
}

/// Most nodes and targets per node a spec may set morph weights on.
const MAX_MORPH_NODES: usize = 16;
const MAX_MORPH_TARGETS: usize = 16;

impl Spec {
    /// Parses one browser render spec from a JSON string. Anything malformed is
    /// rejected whole rather than half-applied.
    #[cfg(test)]
    pub fn parse(json: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        Self::from_value(&value)
    }

    pub fn from_value(value: &serde_json::Value) -> Option<Self> {
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
        let morphs = match value.get("morphs") {
            None | Some(serde_json::Value::Null) => Vec::new(),
            Some(list) => list
                .as_array()
                .filter(|list| list.len() <= MAX_MORPH_NODES)?
                .iter()
                .map(|m| {
                    let weights = m.get("weights")?.as_array()?;
                    if weights.len() > MAX_MORPH_TARGETS {
                        return None;
                    }
                    let weights = weights
                        .iter()
                        .map(|w| finite(w).map(|w| w.clamp(0.0, 1.0)))
                        .collect::<Option<Vec<_>>>()?;
                    Some((m.get("node")?.as_str()?.to_owned(), weights))
                })
                .collect::<Option<Vec<_>>>()?,
        };
        Some(Self {
            kit,
            parts,
            materials,
            morph,
            morphs,
        })
    }

    pub fn visible(&self, name: &str) -> Option<bool> {
        self.parts
            .iter()
            .find(|(part, _)| part == name)
            .map(|(_, shown)| *shown)
    }
}

/// Parses either a per-fighter roster payload (`{"specs":[spec0, ...]}`) or a
/// legacy single-player spec JSON (mapped to actor 0).
pub fn parse_roster(json: &str) -> Option<[Option<Spec>; MAX_FIGHTERS]> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let mut out: [Option<Spec>; MAX_FIGHTERS] = std::array::from_fn(|_| None);
    if let Some(specs) = value.get("specs").and_then(serde_json::Value::as_array) {
        if specs.is_empty() || specs.len() > MAX_FIGHTERS {
            return None;
        }
        for (slot, entry) in out.iter_mut().zip(specs) {
            if entry.is_null() {
                *slot = None;
            } else {
                *slot = Some(Spec::from_value(entry)?);
            }
        }
        return Some(out);
    }
    out[0] = Some(Spec::from_value(&value)?);
    Some(out)
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

/// The last per-fighter specs received from the browser.
#[derive(Resource, Default)]
pub struct Desired {
    revision: Option<f64>,
    specs: [Option<Spec>; MAX_FIGHTERS],
    /// Bumped per actor whenever that actor's spec changes, so only that fighter re-applies.
    generations: [u64; MAX_FIGHTERS],
    /// Hub players' specs by actor (`hub::HUB_ACTOR_BASE` and up), with
    /// their generation.
    hub: HashMap<usize, (Spec, u64)>,
    hub_generation: u64,
}

impl Desired {
    pub fn spec_for(&self, actor: usize) -> Option<&Spec> {
        if actor >= crate::hub::HUB_ACTOR_BASE {
            return self.hub.get(&actor).map(|(spec, _)| spec);
        }
        self.specs.get(actor).and_then(Option::as_ref)
    }

    pub fn generation_for(&self, actor: usize) -> u64 {
        if actor >= crate::hub::HUB_ACTOR_BASE {
            return self
                .hub
                .get(&actor)
                .map_or(0, |(_, generation)| *generation);
        }
        self.generations.get(actor).copied().unwrap_or(0)
    }

    /// A hub player's spec (`None` once they are gone or have none).
    pub fn set_hub(&mut self, actor: usize, spec: Option<Spec>) {
        match spec {
            Some(spec) => {
                if self.hub.get(&actor).map(|(known, _)| known) != Some(&spec) {
                    self.hub_generation += 1;
                    self.hub.insert(actor, (spec, self.hub_generation));
                }
            }
            None => {
                self.hub.remove(&actor);
            }
        }
    }

    pub fn set_roster(&mut self, next: [Option<Spec>; MAX_FIGHTERS]) {
        for (actor, spec) in next.into_iter().enumerate() {
            if self.specs[actor] != spec {
                self.specs[actor] = spec;
                self.generations[actor] += 1;
            }
        }
    }
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

/// Marks a spawned kit scene root: which fighter owns it, which kit it is,
/// whether its glTF scene has finished instantiating, mesh count, the spec
/// generation it shows, and its per-fighter deduplicated `(source, handle)`
/// material overrides.
#[derive(Component)]
pub struct KitInstance {
    pub actor: usize,
    pub path: String,
    pub ready: bool,
    pub meshes: usize,
    pub applied: Option<u64>,
    slot_materials: HashMap<String, (StandardMaterial, Handle<StandardMaterial>)>,
}
impl KitInstance {
    pub fn new_for(actor: usize, path: &str) -> Self {
        Self {
            actor,
            path: path.to_owned(),
            ready: false,
            meshes: 0,
            applied: None,
            slot_materials: HashMap::new(),
        }
    }
}

/// The local player's body carrier (used by lightning hand tagging).
#[derive(Component)]
pub struct PlayerBody;

/// Body carrier for any fighter (`0..MAX_FIGHTERS`), under which kit scenes spawn.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FighterBody(pub usize);

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
    match parse_roster(&json) {
        Some(roster) => desired.set_roster(roster),
        None => warn!("Ignoring malformed character appearance"),
    }
}

/// Ensures every active fighter has a kit instance matching its desired kit,
/// sharing cached GLBs and animation graphs across fighters via [`KitCache`].
/// Previous kits stay visible until the replacement is ready and styled.
#[allow(clippy::too_many_arguments)]
pub fn switch_kit(
    mut commands: Commands,
    desired: Res<Desired>,
    practice: Res<crate::Practice>,
    server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut model: ResMut<CharacterModel>,
    mut kit_cache: ResMut<KitCache>,
    bodies: Query<(Entity, &FighterBody)>,
    instances: Query<&KitInstance>,
) {
    // Only kit-built clients spawn kits; legacy single models stay as they are.
    if !model.kit {
        return;
    }
    let active_count = practice.arena.fighters().len().min(MAX_FIGHTERS);
    for (body_entity, body) in &bodies {
        let actor = body.0;
        if actor >= active_count {
            continue;
        }
        let Some(spec) = desired.spec_for(actor) else {
            continue;
        };
        let already_spawned = instances
            .iter()
            .any(|inst| inst.actor == actor && inst.path == spec.kit.path);
        if already_spawned {
            continue;
        }
        let profile = kit_profile(&spec.kit);
        let cached = kit_cache.get_or_load(profile, &server, &mut graphs);
        let scene = cached.scene.clone();
        if actor == 0 && model.profile.map(|p| p.path) != Some(profile.path) {
            model.set_kit_loading(cached);
        }
        commands.entity(body_entity).with_children(|parent| {
            parent
                .spawn((
                    WorldAssetRoot(scene),
                    profile.transform(),
                    Visibility::Hidden,
                    KitInstance::new_for(actor, profile.path),
                ))
                .observe(crate::character::on_ready);
        });
    }
}

/// Applies part visibility, deduplicated per-fighter material overrides, and
/// morphs to ready kit instances whose shown generation is stale.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn apply(
    mut commands: Commands,
    desired: Res<Desired>,
    server: Res<AssetServer>,
    mut model: Option<ResMut<CharacterModel>>,
    kit_cache: Option<Res<KitCache>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut instances: Query<(Entity, &mut KitInstance)>,
    children: Query<&Children>,
    mut named: Query<
        (&Name, &mut Visibility),
        (Without<CharacterFallback>, Without<FallbackActor>),
    >,
    mut surfaces: Query<(&GltfMaterialName, &mut MeshMaterial3d<StandardMaterial>)>,
    mut morphs: Query<(&Name, &mut MorphWeights)>,
    mut fallback_actors: Query<(&FallbackActor, &mut Visibility), Without<CharacterFallback>>,
    mut player_fallback: Query<&mut Visibility, With<CharacterFallback>>,
) {
    for (root, mut instance) in &mut instances {
        if !instance.ready {
            continue;
        }
        let actor = instance.actor;
        let Some(spec) = desired.spec_for(actor) else {
            continue;
        };
        let generation = desired.generation_for(actor);
        if instance.applied == Some(generation) {
            continue;
        }
        // Only style the instance matching the currently desired kit path; an
        // outgoing kit keeps its look until the replacement is ready.
        if instance.path != spec.kit.path {
            continue;
        }
        instance.applied = Some(generation);
        // Restore CharacterModel if actor 0 reverted back to an already-ready kit
        // while a replacement kit was still loading (or had failed).
        if actor == 0
            && let (Some(model), Some(cache)) = (model.as_deref_mut(), kit_cache.as_deref())
            && (model.profile.map(|p| p.path) != Some(instance.path.as_str())
                || model.status() != "ready")
            && let Some(cached) = cache.get(&instance.path)
        {
            model.set_kit_ready(cached, instance.meshes.max(1));
        }
        reveal(
            &mut commands,
            root,
            actor,
            &mut fallback_actors,
            &mut player_fallback,
        );
        let mut updated_slots: HashMap<String, Handle<StandardMaterial>> = HashMap::new();
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
            if let Ok((name, mut weights)) = morphs.get_mut(entity) {
                if let Some((node, weight)) = &spec.morph
                    && name.as_str() == node
                    && let Some(first) = weights.weights_mut().first_mut()
                {
                    *first = *weight;
                }
                if let Some((_, values)) =
                    spec.morphs.iter().find(|(node, _)| node == name.as_str())
                {
                    for (target, value) in weights.weights_mut().iter_mut().zip(values) {
                        *target = *value;
                    }
                }
            }
            let Ok((material_name, mut handle)) = surfaces.get_mut(entity) else {
                continue;
            };
            let Some(change) = spec.materials.iter().find(|m| m.name == material_name.0) else {
                continue;
            };
            if let Some(shared_slot_handle) = updated_slots.get(&material_name.0) {
                handle.0 = shared_slot_handle.clone();
                continue;
            }
            // Share one overridden StandardMaterial handle per material slot on
            // this fighter, keeping the original glTF StandardMaterial alongside
            // it so re-applying never depends on child iteration order or mutates
            // the shared glTF asset.
            let slot_handle = if let Some((base, existing_handle)) =
                instance.slot_materials.get(&material_name.0).cloned()
            {
                let next = overridden(base, change, &server);
                if let Some(mut existing) = materials.get_mut(&existing_handle) {
                    *existing = next;
                    existing_handle
                } else {
                    let fresh = materials.add(next);
                    instance
                        .slot_materials
                        .get_mut(&material_name.0)
                        .expect("slot exists")
                        .1 = fresh.clone();
                    fresh
                }
            } else {
                let Some(original) = materials.get(&handle.0).cloned() else {
                    continue;
                };
                let next = overridden(original.clone(), change, &server);
                let fresh = materials.add(next);
                instance
                    .slot_materials
                    .insert(material_name.0.clone(), (original, fresh.clone()));
                fresh
            };
            handle.0 = slot_handle.clone();
            updated_slots.insert(material_name.0.clone(), slot_handle);
        }
    }
}

/// Shows a freshly applied kit for `actor` and retires whatever stood in for
/// that same actor: its previous kit, or its procedural fallback at startup.
fn reveal(
    commands: &mut Commands,
    root: Entity,
    actor: usize,
    fallback_actors: &mut Query<(&FallbackActor, &mut Visibility), Without<CharacterFallback>>,
    player_fallback: &mut Query<&mut Visibility, With<CharacterFallback>>,
) {
    commands.entity(root).insert(Visibility::Inherited);
    commands.queue(move |world: &mut World| {
        let others: Vec<Entity> = world
            .query::<(Entity, &KitInstance)>()
            .iter(world)
            .filter(|(entity, inst)| *entity != root && inst.actor == actor)
            .map(|(entity, _)| entity)
            .collect();
        for entity in others {
            world.despawn(entity);
        }
    });
    for (fallback_actor, mut visibility) in fallback_actors.iter_mut() {
        if fallback_actor.0 == actor {
            *visibility = Visibility::Hidden;
        }
    }
    if actor == 0 {
        for mut visibility in player_fallback.iter_mut() {
            *visibility = Visibility::Hidden;
        }
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

    const SPEC_FEMALE: &str = r#"{"kit":{"id":"human-female","path":"characters/kits/human-female.glb","sole":-0.004,"scale":1.0656},
        "parts":[["Hair_Buns",true],["Body_Torso",false]],
        "materials":[{"name":"Skin","tint":[0.4,0.3,0.2],"texture":null}],
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
    fn roster_json_parses_multi_fighter_and_single_legacy_specs() {
        let roster_json = format!(r#"{{"specs":[{SPEC},{SPEC_FEMALE},null]}}"#);
        let roster = parse_roster(&roster_json).unwrap();
        assert_eq!(roster[0].as_ref().unwrap().kit.id, "human-male");
        assert_eq!(roster[1].as_ref().unwrap().kit.id, "human-female");
        assert!(roster[2].is_none());
        // Legacy single-spec JSON still populates actor 0.
        let single = parse_roster(SPEC).unwrap();
        assert_eq!(single[0].as_ref().unwrap().kit.id, "human-male");
        assert!(single[1].is_none());
        // Empty or over-max roster arrays are rejected.
        assert!(parse_roster(r#"{"specs":[]}"#).is_none());
        let seven = format!(r#"{{"specs":[{SPEC},{SPEC},{SPEC},{SPEC},{SPEC},{SPEC},{SPEC}]}}"#);
        assert!(parse_roster(&seven).is_none());
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
        // Face presets: weights per node, bounded; a malformed list rejects the spec.
        assert!(Spec::parse(SPEC).unwrap().morphs.is_empty());
        let faces = |morphs: &str| {
            Spec::parse(&SPEC.replace(
                "\"morph\":null",
                &format!("\"morph\":null,\"morphs\":{morphs}"),
            ))
        };
        assert_eq!(
            faces(r#"[{"node":"Body_Head","weights":[0,1,7]},{"node":"Brows_Bold","weights":[]}]"#)
                .unwrap()
                .morphs,
            vec![
                ("Body_Head".to_owned(), vec![0.0, 1.0, 1.0]),
                ("Brows_Bold".to_owned(), vec![])
            ]
        );
        for bad in [
            r#"[{"node":"Body_Head"}]"#,
            r#"[{"node":7,"weights":[0]}]"#,
            r#"[{"node":"Body_Head","weights":["1"]}]"#,
            r#"{"node":"Body_Head","weights":[0]}"#,
        ] {
            assert_eq!(faces(bad), None, "{bad}");
        }
        let many = format!(
            "[{}]",
            vec![r#"{"node":"N","weights":[0]}"#; MAX_MORPH_NODES + 1].join(",")
        );
        assert_eq!(faces(&many), None);
        let wide = format!(
            r#"[{{"node":"N","weights":[{}]}}]"#,
            vec!["0"; MAX_MORPH_TARGETS + 1].join(",")
        );
        assert_eq!(faces(&wide), None);
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
    fn apply_shows_chosen_parts_deduplicates_per_fighter_materials_and_isolates_actors() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<StandardMaterial>()
            .add_systems(Update, apply);
        let spec0 = Spec::parse(SPEC).unwrap();
        let spec1 = Spec::parse(SPEC_FEMALE).unwrap();
        let mut desired = Desired::default();
        desired.set_roster([Some(spec0), Some(spec1), None, None, None, None]);
        app.insert_resource(desired);
        let shared = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let fallback0 = app
            .world_mut()
            .spawn((
                CharacterFallback,
                FallbackActor(0),
                Name::new("Procedural humanoid"),
                Visibility::Inherited,
            ))
            .id();
        let fallback1 = app
            .world_mut()
            .spawn((
                FallbackActor(1),
                Name::new("Procedural humanoid"),
                Visibility::Inherited,
            ))
            .id();
        let previous0 = app
            .world_mut()
            .spawn(KitInstance {
                actor: 0,
                path: "characters/kits/human-female.glb".into(),
                ready: true,
                meshes: 1,
                applied: Some(0),
                slot_materials: HashMap::new(),
            })
            .id();
        let mut hair0 = Entity::PLACEHOLDER;
        let mut torso0 = Entity::PLACEHOLDER;
        let mut skin0_a = Entity::PLACEHOLDER;
        let mut skin0_b = Entity::PLACEHOLDER;
        let mut inst0 = KitInstance::new_for(0, "characters/kits/human-male.glb");
        inst0.ready = true;
        let root0 = app
            .world_mut()
            .spawn((inst0, Visibility::Hidden))
            .with_children(|root| {
                hair0 = root
                    .spawn((Name::new("Hair_Long"), Visibility::Hidden))
                    .id();
                torso0 = root
                    .spawn((Name::new("Body_Torso"), Visibility::Inherited))
                    .with_children(|node| {
                        skin0_a = node
                            .spawn((
                                GltfMaterialName("Skin".into()),
                                MeshMaterial3d(shared.clone()),
                            ))
                            .id();
                        skin0_b = node
                            .spawn((
                                GltfMaterialName("Skin".into()),
                                MeshMaterial3d(shared.clone()),
                            ))
                            .id();
                    })
                    .id();
            })
            .id();

        let mut skin1 = Entity::PLACEHOLDER;
        let mut inst1 = KitInstance::new_for(1, "characters/kits/human-female.glb");
        inst1.ready = true;
        let root1 = app
            .world_mut()
            .spawn((inst1, Visibility::Hidden))
            .with_children(|root| {
                skin1 = root
                    .spawn((
                        GltfMaterialName("Skin".into()),
                        MeshMaterial3d(shared.clone()),
                    ))
                    .id();
            })
            .id();

        app.update();
        let world = app.world();
        assert_eq!(world.get::<Visibility>(hair0), Some(&Visibility::Inherited));
        assert_eq!(world.get::<Visibility>(torso0), Some(&Visibility::Hidden));
        assert_eq!(world.get::<Visibility>(root0), Some(&Visibility::Inherited));
        assert_eq!(world.get::<Visibility>(root1), Some(&Visibility::Inherited));
        assert_eq!(
            world.get::<Visibility>(fallback0),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            world.get::<Visibility>(fallback1),
            Some(&Visibility::Hidden)
        );
        assert!(
            world.get_entity(previous0).is_err(),
            "actor 0 previous kit retired"
        );
        assert!(
            world.get_entity(root1).is_ok(),
            "actor 1 kit untouched by actor 0 reveal"
        );

        let own0_a = &world
            .get::<MeshMaterial3d<StandardMaterial>>(skin0_a)
            .unwrap()
            .0;
        let own0_b = &world
            .get::<MeshMaterial3d<StandardMaterial>>(skin0_b)
            .unwrap()
            .0;
        let own1 = &world
            .get::<MeshMaterial3d<StandardMaterial>>(skin1)
            .unwrap()
            .0;
        assert_ne!(
            own0_a, &shared,
            "overrides never mutate shared glTF material"
        );
        assert_eq!(
            own0_a, own0_b,
            "same fighter shares one handle per material slot"
        );
        assert_ne!(
            own0_a, own1,
            "distinct fighters keep isolated material handles"
        );
        let materials = world.resource::<Assets<StandardMaterial>>();
        assert_eq!(materials.get(&shared).unwrap().base_color, Color::WHITE);
        assert!((materials.get(own0_a).unwrap().base_color.to_srgba().red - 0.8).abs() < 1e-4);
        assert!((materials.get(own1).unwrap().base_color.to_srgba().red - 0.4).abs() < 1e-4);

        // Re-applying generation 2 edits actor 0's material handle in place without affecting actor 1.
        let own0_handle = own0_a.clone();
        let own1_handle = own1.clone();
        app.update();
        {
            let mut desired = app.world_mut().resource_mut::<Desired>();
            desired.specs[0].as_mut().unwrap().materials[0].tint = [0.5, 0.5, 0.5];
            desired.generations[0] += 1;
        }
        app.update();
        let world = app.world();
        assert_eq!(
            world
                .get::<MeshMaterial3d<StandardMaterial>>(skin0_a)
                .unwrap()
                .0,
            own0_handle
        );
        let materials = world.resource::<Assets<StandardMaterial>>();
        assert!(
            (materials
                .get(&own0_handle)
                .unwrap()
                .base_color
                .to_srgba()
                .red
                - 0.5)
                .abs()
                < 1e-4
        );
        assert!(
            (materials
                .get(&own1_handle)
                .unwrap()
                .base_color
                .to_srgba()
                .red
                - 0.4)
                .abs()
                < 1e-4
        );
    }

    #[test]
    fn switch_kit_shares_cached_glbs_across_all_six_fighters_in_3v3() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<WorldAsset>()
            .init_asset::<Gltf>()
            .init_asset::<AnimationClip>()
            .init_asset::<AnimationGraph>()
            .insert_resource(crate::Practice::new(&arena_map::colosseum(), 3))
            .insert_resource(KitCache::default())
            .insert_resource(CharacterModel::ready(kit_profile(
                &Spec::parse(SPEC).unwrap().kit,
            )))
            .add_systems(Update, switch_kit);
        let male = Spec::parse(SPEC).unwrap();
        let female = Spec::parse(SPEC_FEMALE).unwrap();
        let mut desired = Desired::default();
        desired.set_roster([
            Some(male.clone()),
            Some(female.clone()),
            Some(male.clone()),
            Some(female.clone()),
            Some(male),
            Some(female),
        ]);
        app.insert_resource(desired);
        for actor in 0..MAX_FIGHTERS {
            app.world_mut().spawn(FighterBody(actor));
        }
        app.update();
        let world = app.world_mut();
        assert_eq!(
            world.resource::<KitCache>().len(),
            2,
            "6 fighters across 2 kits reuse only 2 cached GLB/AnimationGraph entries"
        );
        let mut spawned_actors: Vec<usize> = world
            .query::<&KitInstance>()
            .iter(world)
            .map(|inst| inst.actor)
            .collect();
        spawned_actors.sort_unstable();
        assert_eq!(spawned_actors, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn reverting_actor_0_back_to_a_ready_kit_restores_character_model_status() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<WorldAsset>()
            .init_asset::<Gltf>()
            .init_asset::<AnimationClip>()
            .init_asset::<AnimationGraph>()
            .init_asset::<StandardMaterial>()
            .insert_resource(crate::Practice::new(&arena_map::colosseum(), 1))
            .insert_resource(KitCache::default())
            .add_systems(Update, (switch_kit, apply).chain());

        let kit_a = Spec::parse(SPEC).unwrap();
        let kit_b = Spec::parse(SPEC_FEMALE).unwrap();
        let prof_a = kit_profile(&kit_a.kit);
        {
            let server = app.world().resource::<AssetServer>().clone();
            app.world_mut()
                .resource_scope(|world, mut cache: Mut<KitCache>| {
                    world.resource_scope(|_, mut graphs: Mut<Assets<AnimationGraph>>| {
                        cache.get_or_load(prof_a, &server, &mut graphs);
                    });
                });
        }
        app.insert_resource(CharacterModel::ready(prof_a));
        let mut desired = Desired::default();
        desired.set_roster([Some(kit_a.clone()), None, None, None, None, None]);
        app.insert_resource(desired);
        let body0 = app.world_mut().spawn(FighterBody(0)).id();
        let mut ready_a = KitInstance::new_for(0, &kit_a.kit.path);
        ready_a.ready = true;
        ready_a.meshes = 12;
        app.world_mut()
            .spawn((ready_a, Visibility::Inherited, ChildOf(body0)));
        app.update();
        assert_eq!(app.world().resource::<CharacterModel>().status(), "ready");

        // Switch actor 0 to Kit B: CharacterModel transitions to loading for Kit B.
        app.world_mut().resource_mut::<Desired>().set_roster([
            Some(kit_b),
            None,
            None,
            None,
            None,
            None,
        ]);
        app.update();
        assert_eq!(app.world().resource::<CharacterModel>().status(), "loading");
        assert_eq!(
            app.world()
                .resource::<CharacterModel>()
                .profile
                .map(|p| p.path),
            Some("characters/kits/human-female.glb")
        );

        // Revert back to Kit A before Kit B finishes: CharacterModel must recover to ready Kit A.
        app.world_mut().resource_mut::<Desired>().set_roster([
            Some(kit_a),
            None,
            None,
            None,
            None,
            None,
        ]);
        app.update();
        assert_eq!(app.world().resource::<CharacterModel>().status(), "ready");
        assert_eq!(
            app.world()
                .resource::<CharacterModel>()
                .profile
                .map(|p| p.path),
            Some("characters/kits/human-male.glb")
        );
        assert_eq!(app.world().resource::<CharacterModel>().meshes, 12);
    }
}
