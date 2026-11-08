//! Presentation-only glTF character. The rules still own position/collision.
use bevy::{prelude::*, world_serialization::WorldInstanceReady};

use crate::character_animation::{Clip, Controller};

pub const MODEL_PATH: &str = "characters/base-male.glb";
const SOURCE_HEIGHT: f32 = 1.898;
const HEIGHT: f32 = 1.9;

#[derive(Component)]
pub struct CharacterFallback;

#[derive(Resource)]
pub struct CharacterModel {
    pub scene: Option<Handle<WorldAsset>>,
    asset: Option<Handle<Gltf>>,
    graph: Handle<AnimationGraph>,
    pub nodes: Vec<AnimationNodeIndex>,
    status: &'static str,
    pub meshes: usize,
}
impl CharacterModel {
    pub fn load(
        available: bool,
        server: &AssetServer,
        graphs: &mut Assets<AnimationGraph>,
    ) -> Self {
        let scene = available.then(|| server.load(GltfAssetLabel::Scene(0).from_asset(MODEL_PATH)));
        let asset = available.then(|| server.load(MODEL_PATH));
        let (graph, nodes) = if available {
            let (graph, nodes) = AnimationGraph::from_clips(Clip::ALL.map(|clip| {
                server.load(GltfAssetLabel::Animation(clip as usize).from_asset(MODEL_PATH))
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
            status,
            meshes: 0,
        }
    }
    pub fn status(&self) -> &'static str {
        self.status
    }
}

/// Blender -Y becomes glTF +Z. Turn it toward Bevy -Z, normalize the 1.898 m
/// rest height, and lower the measured sole offset to just above the floor.
pub fn model_transform() -> Transform {
    let scale = HEIGHT / SOURCE_HEIGHT;
    Transform {
        translation: Vec3::Y * (0.01 - 0.009 * scale),
        rotation: Quat::from_rotation_y(std::f32::consts::PI),
        scale: Vec3::splat(scale),
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
) {
    model.meshes = children
        .iter_descendants(ready.entity)
        .filter(|entity| meshes.contains(*entity))
        .count();
    let player_entities: Vec<_> = children
        .iter_descendants(ready.entity)
        .filter(|entity| players.contains(*entity))
        .collect();
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
        Controller::default(),
    ));
    if let Ok(mut visibility) = roots.get_mut(ready.entity) {
        *visibility = Visibility::Inherited;
    }
    model.status = "ready";
    for mut visibility in &mut fallback {
        *visibility = Visibility::Hidden;
    }
    for mut carrier in &mut carriers {
        if carrier.actor == 0 {
            carrier.authored = true;
        }
    }
}

pub fn check_loading(asset_server: Res<AssetServer>, mut model: ResMut<CharacterModel>) {
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

    fn model() -> CharacterModel {
        let (_, nodes) =
            AnimationGraph::from_clips(Clip::ALL.map(|_| Handle::<AnimationClip>::default()));
        CharacterModel {
            scene: Some(Handle::default()),
            asset: None,
            graph: Handle::default(),
            nodes,
            status: "loading",
            meshes: 0,
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
        let transform = model_transform();
        let sole = transform.transform_point(Vec3::Y * 0.009);
        let top = transform.transform_point(Vec3::Y * 1.907);
        assert!((sole.y - 0.01).abs() < 0.0001);
        assert!((top.y - sole.y - HEIGHT).abs() < 0.0001);
        assert!((transform.rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_Z, 0.0001));
        // The authored character's left (+X) must become game left (-X).
        assert!((transform.rotation * Vec3::X).abs_diff_eq(Vec3::NEG_X, 0.0001));
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
                Controller::default(),
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
