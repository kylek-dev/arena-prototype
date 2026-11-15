//! Decorative map props: CC0 Quaternius Fantasy Props MegaKit models placed by
//! the map file. Presentation only; the rules never read props, so units walk
//! and see through them and the camera does not collide with them.
//!
//! `scripts/prepare_props.py` copies each prop's glTF to `assets/props/<id>.gltf`
//! with shared, shrunken trim-sheet textures.
use crate::venue::VenueEntity;
use arena_map::{MapFile, Prop};
use bevy::prelude::*;
use std::collections::HashMap;

/// Model handles kept across venue rebuilds, so editing a map never reloads
/// a prop that is already placed.
#[derive(Resource, Default)]
pub(crate) struct PropModels(HashMap<String, Handle<WorldAsset>>);

impl PropModels {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
    fn get(&mut self, server: &AssetServer, model: &str) -> Handle<WorldAsset> {
        self.0
            .entry(model.to_owned())
            .or_insert_with(|| {
                server.load(GltfAssetLabel::Scene(0).from_asset(format!("props/{model}.gltf")))
            })
            .clone()
    }
}

/// Where a prop stands: the map's degrees turn counter-clockwise seen from above.
pub(crate) fn transform(prop: &Prop) -> Transform {
    Transform::from_xyz(prop.x, prop.y, prop.z)
        .with_rotation(Quat::from_rotation_y(prop.rotation.to_radians()))
        .with_scale(Vec3::splat(prop.scale))
}

/// Spawns every prop of a validated map, tagged so a rebuild replaces them.
pub(crate) fn spawn(
    commands: &mut Commands,
    server: &AssetServer,
    models: &mut PropModels,
    map: &MapFile,
) {
    for prop in &map.props {
        commands.spawn((
            VenueEntity,
            Name::new(format!("Prop {}", prop.model)),
            WorldAssetRoot(models.get(server, &prop.model)),
            transform(prop),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_is_counter_clockwise_from_above_in_degrees() {
        let prop = Prop {
            model: "Bench".into(),
            x: 1.0,
            z: 2.0,
            y: 0.5,
            rotation: 90.0,
            scale: 2.0,
        };
        let t = transform(&prop);
        assert_eq!(t.translation, Vec3::new(1.0, 0.5, 2.0));
        // Seen from above with north (−Z) up, +X (east) turns to north.
        let east = t.rotation * Vec3::X;
        assert!((east - Vec3::NEG_Z).length() < 1e-5, "{east}");
        assert_eq!(t.scale, Vec3::splat(2.0));
    }
}
