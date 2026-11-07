//! Presentation-only glTF character. The rules still own position/collision.
use bevy::{prelude::*, world_serialization::WorldInstanceReady};

pub const MODEL_PATH: &str = "characters/pixellabs-undead-3417.glb";
const SOURCE_HEIGHT: f32 = 0.975_076;
const HEIGHT: f32 = 1.9;

#[derive(Component)]
pub struct CharacterFallback;

#[derive(Resource)]
pub struct CharacterModel {
    pub scene: Option<Handle<WorldAsset>>,
    status: &'static str,
    pub meshes: usize,
}
impl CharacterModel {
    pub fn new(scene: Option<Handle<WorldAsset>>) -> Self {
        let status = if scene.is_some() {
            "loading"
        } else {
            "placeholder"
        };
        Self {
            scene,
            status,
            meshes: 0,
        }
    }
    pub fn status(&self) -> &'static str {
        self.status
    }
}

/// The supplied scene is Y-up after its embedded node transform. Normalize its
/// measured bounds to 1.9 m, put its feet on the floor, and face Bevy's -Z.
pub fn model_transform() -> Transform {
    let scale = HEIGHT / SOURCE_HEIGHT;
    Transform {
        translation: Vec3::new(0.030_199 * scale, 0.02, 0.090_897_5 * scale),
        rotation: Quat::from_rotation_y(std::f32::consts::PI),
        scale: Vec3::splat(scale),
    }
}

pub fn on_ready(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    mut fallback: Query<&mut Visibility, With<CharacterFallback>>,
    mut model: ResMut<CharacterModel>,
) {
    model.meshes = children
        .iter_descendants(ready.entity)
        .filter(|entity| meshes.contains(*entity))
        .count();
    if model.meshes == 0 {
        model.status = "failed";
        warn!("Character scene spawned without any meshes; retaining placeholder");
        return;
    }
    model.status = "ready";
    for mut visibility in &mut fallback {
        *visibility = Visibility::Hidden;
    }
}

pub fn check_loading(asset_server: Res<AssetServer>, mut model: ResMut<CharacterModel>) {
    if model.status != "loading" {
        return;
    }
    let Some(scene) = &model.scene else {
        return;
    };
    let failed = asset_server
        .get_load_state(scene.id())
        .is_some_and(|state| state.is_failed())
        || asset_server
            .get_recursive_dependency_load_state(scene.id())
            .is_some_and(|state| state.is_failed());
    if failed {
        model.status = "failed";
        warn!("Character asset failed to load; retaining placeholder");
    }
}
