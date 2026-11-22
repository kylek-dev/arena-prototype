//! Presentation-only glTF character. The rules still own position/collision.
use std::collections::HashMap;

use bevy::{prelude::*, world_serialization::WorldInstanceReady};

use crate::character_animation::{Clip, Controller, JumpEdges, Timing};

pub(crate) const HEIGHT: f32 = 1.9;

/// Per-model build output, measured rest bounds and authored clip timing.
/// IDs match the manifest written by scripts/prepare_character.py.
#[derive(Debug, PartialEq)]
pub struct Profile {
    pub id: &'static str,
    pub path: &'static str,
    /// Measured lowest sole and highest point of the glTF rest meshes.
    pub(crate) sole: f32,
    pub(crate) top: f32,
    pub timing: Timing,
}

pub const BASE_FEMALE: Profile = Profile {
    id: "base-female",
    path: "characters/base-female.glb",
    sole: 0.000_058,
    top: 1.747,
    // 30 fps keys from t = 0: leaves the floor on frame 8, lands on 25;
    // the Pulse cosmetic starts late in the gather (frame 14) and settles by 36.
    timing: Timing {
        jump: (8.0 / 30.0, 25.0 / 30.0),
        cast: (14.0 / 30.0, 36.0 / 30.0),
        jump_edges: None,
        locomotion_cycle: None,
    },
};

pub const BASE_MALE: Profile = Profile {
    id: "base-male",
    path: "characters/base-male.glb",
    sole: 0.009,
    top: 1.907,
    timing: Timing::BASE_MALE,
};

pub const QUATERNIUS_FEMALE: Profile = Profile {
    id: "quaternius-female",
    path: "characters/quaternius-female.glb",
    // Ranger boot sole to Hair_Buns top; source geometry is in metres.
    sole: -0.003_967_253_5,
    top: 1.779_151_6,
    timing: Timing {
        jump: (0.0, 2.5),
        cast: (0.0, 0.5),
        jump_edges: Some(JumpEdges {
            // The complete UAL1 takeoff (it ends on Jump_Loop's first pose) and
            // full landing/recovery. No gameplay windup; world takeoff still
            // happens immediately.
            start: (0.0, 40.0 / 30.0),
            land: (0.0, 38.0 / 30.0),
        }),
        // Jog_Fwd_Loop and the universal lateral run: 28 frames, left foot
        // planted from frame 1 and right from frame 15.
        locomotion_cycle: Some(28.0 / 30.0),
    },
};

impl Profile {
    /// Unknown or absent manifest models select the procedural fallback.
    pub fn from_id(id: &str) -> Option<&'static Self> {
        [&QUATERNIUS_FEMALE, &BASE_FEMALE, &BASE_MALE]
            .into_iter()
            .find(|profile| profile.id == id)
    }

    /// Blender -Y becomes glTF +Z. Turn it toward Bevy -Z, normalize the rest
    /// height to the shared character height and set the sole just above the floor.
    pub fn transform(&self) -> Transform {
        let scale = HEIGHT / (self.top - self.sole);
        Transform {
            translation: Vec3::Y * (0.01 - self.sole * scale),
            rotation: Quat::from_rotation_y(std::f32::consts::PI),
            scale: Vec3::splat(scale),
        }
    }
}

#[derive(Component)]
pub struct CharacterFallback;

/// Tags each fighter's procedural fallback humanoid so kits hide only their own actor's fallback.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FallbackActor(pub usize);

/// Cached glTF scene and animation graph shared across all fighters wearing the same kit.
pub struct CachedKit {
    pub profile: &'static Profile,
    pub scene: Handle<WorldAsset>,
    pub asset: Handle<Gltf>,
    pub graph: Handle<AnimationGraph>,
    pub nodes: Vec<AnimationNodeIndex>,
    pub failed_logged: bool,
}

/// Loads each customizable kit GLB and animation graph at most once, even in 3v3.
#[derive(Resource, Default)]
pub struct KitCache {
    entries: HashMap<String, CachedKit>,
}

impl KitCache {
    pub fn get(&self, path: &str) -> Option<&CachedKit> {
        self.entries.get(path)
    }

    pub fn insert_from_model(&mut self, model: &CharacterModel) {
        if let (Some(profile), Some(scene), Some(asset)) =
            (model.profile, model.scene.clone(), model.asset.clone())
        {
            self.entries
                .entry(profile.path.to_owned())
                .or_insert(CachedKit {
                    profile,
                    scene,
                    asset,
                    graph: model.graph.clone(),
                    nodes: model.nodes.clone(),
                    failed_logged: false,
                });
        }
    }

    pub fn get_or_load(
        &mut self,
        profile: &'static Profile,
        server: &AssetServer,
        graphs: &mut Assets<AnimationGraph>,
    ) -> &CachedKit {
        self.entries
            .entry(profile.path.to_owned())
            .or_insert_with(|| {
                let loaded = CharacterModel::load(Some(profile), server, graphs);
                CachedKit {
                    profile,
                    scene: loaded.scene.expect("profile always loads a scene"),
                    asset: loaded.asset.expect("profile always loads a gltf"),
                    graph: loaded.graph,
                    nodes: loaded.nodes,
                    failed_logged: false,
                }
            })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut CachedKit> {
        self.entries.values_mut()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Resource)]
pub struct CharacterModel {
    pub scene: Option<Handle<WorldAsset>>,
    asset: Option<Handle<Gltf>>,
    graph: Handle<AnimationGraph>,
    pub nodes: Vec<AnimationNodeIndex>,
    pub profile: Option<&'static Profile>,
    status: &'static str,
    pub meshes: usize,
    /// Assembled from a customizable kit, so appearance changes may swap it.
    pub kit: bool,
}
impl CharacterModel {
    pub fn set_kit_loading(&mut self, cached: &CachedKit) {
        self.scene = Some(cached.scene.clone());
        self.asset = Some(cached.asset.clone());
        self.graph = cached.graph.clone();
        self.nodes = cached.nodes.clone();
        self.profile = Some(cached.profile);
        self.status = "loading";
        self.meshes = 0;
        self.kit = true;
    }

    pub fn set_kit_ready(&mut self, cached: &CachedKit, meshes: usize) {
        self.scene = Some(cached.scene.clone());
        self.asset = Some(cached.asset.clone());
        self.graph = cached.graph.clone();
        self.nodes = cached.nodes.clone();
        self.profile = Some(cached.profile);
        self.status = "ready";
        self.meshes = meshes;
        self.kit = true;
    }
    pub fn load(
        profile: Option<&'static Profile>,
        server: &AssetServer,
        graphs: &mut Assets<AnimationGraph>,
    ) -> Self {
        let scene = profile.map(|p| server.load(GltfAssetLabel::Scene(0).from_asset(p.path)));
        let asset = profile.map(|p| server.load(p.path));
        let (graph, nodes) = if let Some(profile) = profile {
            let count = if profile.timing.jump_edges.is_some() {
                Clip::ALL.len()
            } else {
                Clip::BASE_COUNT
            };
            let (graph, nodes) =
                AnimationGraph::from_clips(Clip::ALL.into_iter().take(count).map(|clip| {
                    server.load(GltfAssetLabel::Animation(clip as usize).from_asset(profile.path))
                }));
            (graphs.add(graph), nodes)
        } else {
            (Handle::default(), Vec::new())
        };
        let status = if scene.is_some() {
            "loading"
        } else {
            "placeholder"
        };
        Self {
            scene,
            asset,
            graph,
            nodes,
            profile,
            status,
            meshes: 0,
            kit: false,
        }
    }
    pub fn status(&self) -> &'static str {
        self.status
    }
    #[cfg(test)]
    pub(crate) fn ready(profile: &'static Profile) -> Self {
        Self {
            scene: None,
            asset: None,
            graph: Handle::default(),
            nodes: Vec::new(),
            profile: Some(profile),
            status: "ready",
            meshes: 1,
            kit: true,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn on_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    mut players: Query<&mut AnimationPlayer>,
    mut roots: Query<&mut Visibility, Without<CharacterFallback>>,
    mut fallback: Query<&mut Visibility, With<CharacterFallback>>,
    mut carriers: Query<&mut crate::animation::Animator>,
    mut model: ResMut<CharacterModel>,
    kit_cache: Option<Res<KitCache>>,
    desired: Option<Res<crate::appearance::Desired>>,
    scenes: Query<&WorldAssetRoot>,
    mut instances: Query<&mut crate::appearance::KitInstance>,
) {
    if let Ok(mut instance) = instances.get_mut(ready.entity) {
        let actor = instance.actor;
        // A kit replaced before it finished loading is discarded, not shown.
        let still_wanted = desired
            .as_ref()
            .and_then(|d| d.spec_for(actor))
            .map(|spec| spec.kit.path == instance.path)
            .unwrap_or_else(|| {
                actor == 0
                    && scenes
                        .get(ready.entity)
                        .ok()
                        .is_some_and(|root| model.scene.as_ref() == Some(&root.0))
            });
        if !still_wanted {
            commands.entity(ready.entity).despawn();
            return;
        }
        let mesh_count = children
            .iter_descendants(ready.entity)
            .filter(|entity| meshes.contains(*entity))
            .count();
        let player_entities: Vec<_> = children
            .iter_descendants(ready.entity)
            .filter(|entity| players.contains(*entity))
            .collect();
        let (profile, graph, nodes) = if let Some(cached) = kit_cache
            .as_deref()
            .and_then(|cache| cache.get(&instance.path))
        {
            (cached.profile, cached.graph.clone(), cached.nodes.clone())
        } else if let Some(profile) = model.profile {
            (profile, model.graph.clone(), model.nodes.clone())
        } else {
            if actor == 0 {
                model.status = "failed";
            }
            warn!("Kit scene for actor {actor} became ready without a profile; retaining fallback");
            return;
        };
        if mesh_count == 0 || player_entities.len() != 1 {
            if actor == 0 {
                model.status = "failed";
            }
            warn!(
                "Kit for actor {actor} requires meshes and one animation player; retaining fallback"
            );
            return;
        }
        let entity = player_entities[0];
        let mut player = players.get_mut(entity).expect("queried animation player");
        let mut transitions = AnimationTransitions::new();
        if let Some(&idle) = nodes.get(Clip::Idle as usize) {
            transitions
                .play(&mut player, idle, std::time::Duration::ZERO)
                .repeat();
        }
        commands.entity(entity).insert((
            AnimationGraphHandle(graph),
            transitions,
            Controller::new_for(actor, profile.timing, nodes),
        ));
        instance.ready = true;
        instance.meshes = mesh_count;
        if actor == 0 {
            model.meshes = mesh_count;
            model.status = "ready";
        }
        for mut carrier in &mut carriers {
            if carrier.actor == actor {
                carrier.authored = true;
            }
        }
        // Kit visibility is revealed by `appearance::apply` once part visibility
        // and material overrides are applied.
        return;
    }

    // Legacy single-model path (non-kit BaseMale/BaseFemale/QuaterniusFemale on actor 0).
    if let Ok(root) = scenes.get(ready.entity)
        && model.scene.as_ref() != Some(&root.0)
    {
        commands.entity(ready.entity).despawn();
        return;
    }
    model.meshes = children
        .iter_descendants(ready.entity)
        .filter(|entity| meshes.contains(*entity))
        .count();
    let player_entities: Vec<_> = children
        .iter_descendants(ready.entity)
        .filter(|entity| players.contains(*entity))
        .collect();
    let Some(profile) = model.profile else {
        model.status = "failed";
        warn!("Character scene became ready without a model profile; retaining fallback");
        return;
    };
    if model.meshes == 0 || player_entities.len() != 1 {
        model.status = "failed";
        warn!("Character requires meshes and one animation player; retaining fallback");
        return;
    }
    let entity = player_entities[0];
    let mut player = players.get_mut(entity).expect("queried animation player");
    let mut transitions = AnimationTransitions::new();
    transitions
        .play(
            &mut player,
            model.nodes[Clip::Idle as usize],
            std::time::Duration::ZERO,
        )
        .repeat();
    commands.entity(entity).insert((
        AnimationGraphHandle(model.graph.clone()),
        transitions,
        Controller::new(profile.timing),
    ));
    model.status = "ready";
    for mut carrier in &mut carriers {
        if carrier.actor == 0 {
            carrier.authored = true;
        }
    }
    if let Ok(mut visibility) = roots.get_mut(ready.entity) {
        *visibility = Visibility::Inherited;
    }
    for mut visibility in &mut fallback {
        *visibility = Visibility::Hidden;
    }
}

pub fn check_loading(
    asset_server: Res<AssetServer>,
    mut model: ResMut<CharacterModel>,
    mut kit_cache: Option<ResMut<KitCache>>,
) {
    if let Some(cache) = kit_cache.as_deref_mut() {
        for cached in cache.iter_mut() {
            if cached.failed_logged {
                continue;
            }
            let failed = asset_server
                .get_load_state(cached.asset.id())
                .is_some_and(|state| state.is_failed())
                || asset_server
                    .get_recursive_dependency_load_state(cached.asset.id())
                    .is_some_and(|state| state.is_failed());
            if failed {
                cached.failed_logged = true;
                warn!(
                    "Character kit '{}' failed to load; retaining procedural fallback",
                    cached.profile.path
                );
            }
        }
    }
    if model.status != "loading" {
        return;
    }
    let Some(asset) = &model.asset else {
        return;
    };
    let failed = asset_server
        .get_load_state(asset.id())
        .is_some_and(|state| state.is_failed())
        || asset_server
            .get_recursive_dependency_load_state(asset.id())
            .is_some_and(|state| state.is_failed());
    if failed {
        model.status = "failed";
        warn!("Character asset failed to load; retaining placeholder");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::world_serialization::WorldInstanceSpawner;

    #[test]
    #[ignore = "requires python3 scripts/prepare_character.py and local CC0 sources"]
    fn prepared_quaternius_kit_loads_in_bevy_without_a_renderer() {
        const KIT: &str = "characters/kits/human-female.glb";
        use bevy::{asset::AssetPlugin, mesh::MeshPlugin};
        let mut app = App::new();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../web/public/assets")
            .canonicalize()
            .expect("prepare local character first");
        assert!(
            path.join(KIT).metadata().unwrap().len() < 14_000_000,
            "prepared character exceeded the local payload budget"
        );
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin {
                file_path: path.to_string_lossy().into_owned(),
                ..default()
            },
            bevy::world_serialization::WorldSerializationPlugin,
            MeshPlugin,
            AnimationPlugin,
            bevy::gltf::GltfPlugin::default(),
        ))
        .init_asset::<Image>();
        app.finish();
        app.cleanup();
        let server = app.world().resource::<AssetServer>().clone();
        let handle: Handle<Gltf> = server.load(KIT);
        // Drain real asset tasks; no sleeps, browser, window, renderer or GPU.
        let mut loaded = false;
        for _ in 0..100_000 {
            app.update();
            match server.get_load_state(handle.id()) {
                Some(bevy::asset::LoadState::Failed(error)) => panic!("{error}"),
                Some(bevy::asset::LoadState::Loaded) => {
                    loaded = true;
                    break;
                }
                _ => {}
            }
        }
        assert!(
            loaded,
            "asset tasks did not complete within the update budget"
        );
        let gltf = app.world().resource::<Assets<Gltf>>().get(&handle).unwrap();
        assert_eq!(gltf.scenes.len(), 1);
        // The body's skin plus parts whose bind pose differs slightly.
        assert!(!gltf.skins.is_empty());
        assert_eq!(gltf.animations.len(), Clip::ALL.len());
        assert!(!gltf.meshes.is_empty());
        for name in [
            "Body_Head",
            "Body_Torso",
            "Body_Feet",
            "Eyes",
            "Eyebrows_Female",
            "Hair_Buns",
            "Hair_Long",
            "Hair_Beard",
            "Ranger_Body",
            "Ranger_Bracers",
            "Ranger_Pauldrons",
            "Ranger_Hood",
            "Peasant_Legs",
        ] {
            assert!(gltf.named_nodes.contains_key(name), "missing {name}");
        }
        for name in [
            "Skin",
            "Skin_Hands",
            "Hair_1",
            "Hair_2",
            "Eyes",
            "Ranger",
            "Peasant",
        ] {
            assert!(
                gltf.named_materials.contains_key(name),
                "missing material {name}"
            );
        }
        assert!(!gltf.named_nodes.contains_key("Superhero_Female"));
        for name in ["Jump", "Jump_Start", "Jump_Land"] {
            assert!(gltf.named_animations.contains_key(name));
        }
        let clips = app.world().resource::<Assets<AnimationClip>>();
        for handle in &gltf.animations {
            let clip = clips.get(handle).expect("loaded clip");
            assert!(clip.duration() > 0.0);
            assert!(!clip.curves().is_empty());
        }
        for name in ["Strafe_Left", "Strafe_Right"] {
            let clip = clips.get(gltf.named_animations.get(name).unwrap()).unwrap();
            // Frame-aligned with Jog_Fwd_Loop (28 frames at 30 samples/s).
            assert!((clip.duration() - 28.0 / 30.0).abs() < 1e-5);
        }
        for (_, image) in app.world().resource::<Assets<Image>>().iter() {
            assert!(image.width() <= 1024 && image.height() <= 1024);
        }
        // Every other built kit (races carry sparse morph targets) loads too.
        let mut others = Vec::new();
        for entry in std::fs::read_dir(path.join("characters/kits")).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if name.ends_with(".glb") && format!("characters/kits/{name}") != KIT {
                others.push(server.load::<Gltf>(format!("characters/kits/{name}")));
            }
        }
        for _ in 0..100_000 {
            app.update();
            if others.iter().all(|h| {
                matches!(
                    server.get_load_state(h.id()),
                    Some(bevy::asset::LoadState::Loaded | bevy::asset::LoadState::Failed(_))
                )
            }) {
                break;
            }
        }
        for handle in &others {
            if let Some(bevy::asset::LoadState::Failed(error)) = server.get_load_state(handle.id())
            {
                panic!("{error}");
            }
            let gltf = app.world().resource::<Assets<Gltf>>().get(handle).unwrap();
            assert_eq!(gltf.animations.len(), Clip::ALL.len());
            assert!(gltf.named_nodes.contains_key("Eyes"));
            assert!(gltf.named_materials.contains_key("Skin"));
        }
    }

    fn model() -> CharacterModel {
        let (_, nodes) =
            AnimationGraph::from_clips(Clip::ALL.map(|_| Handle::<AnimationClip>::default()));
        CharacterModel {
            scene: Some(Handle::default()),
            asset: None,
            graph: Handle::default(),
            nodes,
            profile: Some(&BASE_FEMALE),
            status: "loading",
            meshes: 0,
            kit: false,
        }
    }

    #[test]
    fn readiness_requires_meshes_and_one_player_before_hiding_fallback() {
        for (mesh_count, player_count) in [(1, 1), (0, 1), (1, 0), (1, 2)] {
            let mut app = App::new();
            app.insert_resource(model());
            let fallback = app
                .world_mut()
                .spawn((CharacterFallback, Visibility::Inherited))
                .id();
            let carrier = app
                .world_mut()
                .spawn(crate::animation::Animator::new(0))
                .id();
            let root = app
                .world_mut()
                .spawn(Visibility::Hidden)
                .with_children(|parent| {
                    for _ in 0..mesh_count {
                        parent.spawn(Mesh3d(Handle::default()));
                    }
                    for _ in 0..player_count {
                        parent.spawn(AnimationPlayer::default());
                    }
                })
                .observe(on_ready)
                .id();
            let instance_id =
                WorldInstanceSpawner::default().spawn(Handle::<WorldAsset>::default());
            app.world_mut().trigger(WorldInstanceReady {
                entity: root,
                instance_id,
            });
            let world = app.world_mut();
            world.flush();
            let ready = mesh_count == 1 && player_count == 1;
            assert_eq!(
                world.resource::<CharacterModel>().status(),
                if ready { "ready" } else { "failed" }
            );
            assert_eq!(
                *world.get::<Visibility>(fallback).unwrap(),
                if ready {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                }
            );
            assert_eq!(
                *world.get::<Visibility>(root).unwrap(),
                if ready {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                }
            );
            assert_eq!(
                world
                    .get::<crate::animation::Animator>(carrier)
                    .unwrap()
                    .authored,
                ready
            );
            assert_eq!(
                world.query::<&Controller>().iter(world).count(),
                usize::from(ready)
            );
            assert_eq!(
                world.query::<&AnimationGraphHandle>().iter(world).count(),
                usize::from(ready)
            );
        }
    }

    #[test]
    fn measured_bounds_are_grounded_and_gltf_forward_faces_game_forward() {
        for profile in [&QUATERNIUS_FEMALE, &BASE_FEMALE, &BASE_MALE] {
            let transform = profile.transform();
            let sole = transform.transform_point(Vec3::Y * profile.sole);
            let top = transform.transform_point(Vec3::Y * profile.top);
            assert!((sole.y - 0.01).abs() < 0.0001);
            assert!((top.y - sole.y - HEIGHT).abs() < 0.0001);
            assert!((transform.rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_Z, 0.0001));
            // The authored character's left (+X) must become game left (-X).
            assert!((transform.rotation * Vec3::X).abs_diff_eq(Vec3::NEG_X, 0.0001));
        }
        // BaseMale keeps its previously reviewed placement.
        let male = BASE_MALE.transform();
        assert!((male.scale.x - 1.9 / 1.898).abs() < 0.0001);
    }

    #[test]
    fn manifest_ids_select_profiles_and_unknown_ids_use_the_fallback() {
        assert_eq!(Profile::from_id("base-female"), Some(&BASE_FEMALE));
        assert_eq!(Profile::from_id("base-male"), Some(&BASE_MALE));
        assert_eq!(
            Profile::from_id("quaternius-female"),
            Some(&QUATERNIUS_FEMALE)
        );
        for id in ["", "BaseFemale", "base-female.glb", "other"] {
            assert_eq!(Profile::from_id(id), None);
        }
        for profile in [&QUATERNIUS_FEMALE, &BASE_FEMALE, &BASE_MALE] {
            assert_eq!(profile.path, format!("characters/{}.glb", profile.id));
        }
    }

    #[test]
    fn animation_system_drives_graph_and_keeps_jump_root_rule_owned() {
        use bevy::animation::RepeatAnimation;
        let mut app = App::new();
        let model = model();
        let nodes = model.nodes.clone();
        app.insert_resource(model)
            .init_resource::<Time>()
            .init_resource::<Time<Fixed>>()
            .init_resource::<crate::Practice>()
            .add_systems(Update, crate::character_animation::animate);
        let fighter = app
            .world_mut()
            .spawn((crate::FighterVisual(0), Transform::default()))
            .id();
        let player = app
            .world_mut()
            .spawn((
                Controller::new(BASE_FEMALE.timing),
                AnimationPlayer::default(),
                AnimationTransitions::new(),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
        app.world_mut().run_schedule(Update);
        assert_eq!(
            app.world()
                .get::<AnimationPlayer>(player)
                .unwrap()
                .animation(nodes[Clip::Idle as usize])
                .unwrap()
                .repeat_mode(),
            RepeatAnimation::Forever
        );
        app.world_mut()
            .get_mut::<Transform>(fighter)
            .unwrap()
            .translation
            .z = -0.05;
        app.world_mut().run_schedule(Update);
        assert_eq!(
            app.world()
                .get::<AnimationTransitions>(player)
                .unwrap()
                .get_main_animation(),
            Some(nodes[Clip::Forward as usize])
        );
        app.world_mut()
            .resource_mut::<crate::Practice>()
            .arena
            .step(&[arena_sim::arena::ArenaCommand::Jump { actor: 0 }]);
        app.world_mut()
            .get_mut::<Transform>(fighter)
            .unwrap()
            .translation
            .y = 0.22;
        app.world_mut().run_schedule(Update);
        let active = app
            .world()
            .get::<AnimationPlayer>(player)
            .unwrap()
            .animation(nodes[Clip::Jump as usize])
            .unwrap();
        assert_eq!(active.speed(), 0.0);
        assert!(!active.is_paused());
        assert_eq!(
            app.world().get::<Transform>(fighter).unwrap().translation.y,
            0.22
        );
        *app.world_mut().resource_mut::<crate::Practice>() = crate::Practice::default();
        app.world_mut()
            .get_mut::<Transform>(fighter)
            .unwrap()
            .translation = Vec3::ZERO;
        app.world_mut().run_schedule(Update);
        let player = app.world().get::<AnimationPlayer>(player).unwrap();
        assert_eq!(player.playing_animations().count(), 1);
        assert!(player.animation(nodes[Clip::Idle as usize]).is_some());
    }
}
