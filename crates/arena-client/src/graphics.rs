//! Device-local presentation preferences; never change simulation geometry.
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub msaa: Msaa,
    pub high_detail: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            msaa: Msaa::Sample4,
            high_detail: true,
        }
    }
}

impl Settings {
    pub fn from_values(samples: Option<f64>, detail: Option<f64>) -> Self {
        Self {
            msaa: if samples == Some(1.0) {
                Msaa::Off
            } else {
                Msaa::Sample4
            },
            high_detail: detail != Some(0.0),
        }
    }
}

#[derive(Component)]
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
                meshes.add(Capsule3d::new(0.32, 0.7)),
                meshes.add(Sphere::new(0.23).mesh().uv(16, 12)),
            ],
            high: [
                meshes.add(
                    Capsule3d::new(0.32, 0.7)
                        .mesh()
                        .longitudes(64)
                        .latitudes(32),
                ),
                meshes.add(Sphere::new(0.23).mesh().uv(48, 32)),
            ],
            applied: None,
        }
    }

    pub fn handles(&self, high_detail: bool) -> &[Handle<Mesh>; 2] {
        if high_detail { &self.high } else { &self.low }
    }
}

pub(crate) fn apply(
    mut meshes: ResMut<CharacterMeshes>,
    mut cameras: Query<&mut Msaa, With<Camera3d>>,
    mut parts: Query<(&CharacterPart, &mut Mesh3d)>,
) {
    let settings = super::platform::read_graphics();
    if meshes.applied == Some(settings) {
        return;
    }
    for mut msaa in &mut cameras {
        *msaa = settings.msaa;
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
    fn missing_and_invalid_values_use_quality_defaults() {
        for value in [
            None,
            Some(f64::NAN),
            Some(f64::INFINITY),
            Some(-1.0),
            Some(8.0),
        ] {
            assert_eq!(Settings::from_values(value, value), Settings::default());
        }
        assert_eq!(Settings::from_values(Some(1.0), Some(0.0)).msaa, Msaa::Off);
        assert!(!Settings::from_values(Some(1.0), Some(0.0)).high_detail);
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
