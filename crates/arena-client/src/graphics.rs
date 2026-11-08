//! Device-local presentation preferences; never change simulation geometry.
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub msaa: Msaa,
    pub high_detail: bool,
    pub bloom: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            msaa: Msaa::Sample4,
            high_detail: true,
            bloom: true,
        }
    }
}

impl Settings {
    pub fn from_values(samples: Option<f64>, detail: Option<f64>, bloom: Option<f64>) -> Self {
        Self {
            msaa: if samples == Some(1.0) {
                Msaa::Off
            } else {
                Msaa::Sample4
            },
            high_detail: detail != Some(0.0),
            bloom: bloom != Some(0.0),
        }
    }
}

/// Thresholded glow keeps the arena crisp instead of blurring every surface.
pub(crate) fn weapon_bloom() -> Bloom {
    Bloom {
        intensity: 0.22,
        low_frequency_boost: 0.4,
        prefilter: BloomPrefilter {
            threshold: 0.8,
            threshold_softness: 0.35,
        },
        ..Bloom::OLD_SCHOOL
    }
}

/// Explicit exposure and a gentle shoulder: retain a readable arena rather
/// than relying on the camera default and ACES's darker toe.
pub(crate) fn arena_exposure() -> bevy::camera::Exposure {
    bevy::camera::Exposure { ev100: 10.0 }
}

#[derive(Component, Clone, Copy)]
pub(crate) enum CharacterPart {
    Body,
    Head,
}

#[derive(Resource)]
pub(crate) struct CharacterMeshes {
    low: [Handle<Mesh>; 2],
    high: [Handle<Mesh>; 2],
    applied: Option<Settings>,
}

impl CharacterMeshes {
    pub fn new(meshes: &mut Assets<Mesh>) -> Self {
        Self {
            low: [
                meshes.add(Capsule3d::new(0.5, 1.0)),
                meshes.add(Sphere::new(0.5).mesh().uv(16, 12)),
            ],
            high: [
                meshes.add(Capsule3d::new(0.5, 1.0).mesh().longitudes(64).latitudes(32)),
                meshes.add(Sphere::new(0.5).mesh().uv(48, 32)),
            ],
            applied: None,
        }
    }

    pub fn handles(&self, high_detail: bool) -> &[Handle<Mesh>; 2] {
        if high_detail { &self.high } else { &self.low }
    }
}

pub(crate) fn apply(
    mut commands: Commands,
    mut meshes: ResMut<CharacterMeshes>,
    mut cameras: Query<(Entity, &mut Msaa), With<Camera3d>>,
    mut parts: Query<(&CharacterPart, &mut Mesh3d)>,
) {
    let settings = super::platform::read_graphics();
    if meshes.applied == Some(settings) {
        return;
    }
    for (entity, mut msaa) in &mut cameras {
        *msaa = settings.msaa;
        if settings.bloom {
            commands.entity(entity).insert(weapon_bloom());
        } else {
            commands.entity(entity).remove::<Bloom>();
        }
    }
    let handles = meshes.handles(settings.high_detail);
    for (part, mut mesh) in &mut parts {
        mesh.0 = handles[match part {
            CharacterPart::Body => 0,
            CharacterPart::Head => 1,
        }]
        .clone();
    }
    meshes.applied = Some(settings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapon_glow_requires_hdr_and_uses_a_bounded_highlight_only_filter() {
        let mut world = World::new();
        let bloom = weapon_bloom();
        assert!(bloom.intensity > 0.0 && bloom.intensity < 0.3);
        assert_eq!(bloom.prefilter.threshold, 0.8);
        assert!(bloom.prefilter.threshold_softness > 0.0);
        assert_eq!(bloom.max_mip_dimension, 512);
        let camera = world.spawn(bloom).id();
        assert!(world.get::<bevy::camera::Hdr>(camera).is_some());
        world.entity_mut(camera).remove::<Bloom>();
        assert!(world.get::<bevy::camera::Hdr>(camera).is_some());
    }

    #[test]
    fn explicit_arena_exposure_preserves_the_users_tuned_starting_value() {
        assert_eq!(arena_exposure().ev100, 10.0);
        assert!(arena_exposure().exposure().is_finite());
    }

    #[test]
    fn missing_and_invalid_values_use_quality_defaults() {
        for value in [
            None,
            Some(f64::NAN),
            Some(f64::INFINITY),
            Some(-1.0),
            Some(8.0),
        ] {
            assert_eq!(
                Settings::from_values(value, value, value),
                Settings::default()
            );
        }
        assert_eq!(
            Settings::from_values(Some(1.0), Some(0.0), Some(0.0)).msaa,
            Msaa::Off
        );
        assert!(!Settings::from_values(Some(1.0), Some(0.0), Some(0.0)).high_detail);
        assert!(!Settings::from_values(None, None, Some(0.0)).bloom);
    }

    #[test]
    fn higher_detail_adds_geometry_without_changing_body_bounds() {
        let mut assets = Assets::<Mesh>::default();
        let meshes = CharacterMeshes::new(&mut assets);
        for index in 0..2 {
            let low = assets.get(&meshes.low[index]).unwrap();
            let high = assets.get(&meshes.high[index]).unwrap();
            assert!(high.count_vertices() > low.count_vertices());
            let bounds = |mesh: &Mesh| {
                let bevy::mesh::VertexAttributeValues::Float32x3(positions) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                else {
                    panic!("expected float positions");
                };
                positions.iter().fold(
                    (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                    |(min, max), p| (min.min(Vec3::from(*p)), max.max(Vec3::from(*p))),
                )
            };
            let (low_min, low_max) = bounds(low);
            let (high_min, high_max) = bounds(high);
            assert!(low_min.abs_diff_eq(high_min, 0.0001));
            assert!(low_max.abs_diff_eq(high_max, 0.0001));
        }
    }
}
