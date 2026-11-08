//! Temporary live art controls, isolated from simulation and combat input.
use bevy::{camera::Exposure, post_process::bloom::Bloom, prelude::*};
use bevy::{core_pipeline::tonemapping::Tonemapping, render::view::ColorGrading};

pub const VALUE_COUNT: usize = 29;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    background: [f32; 3],
    ambient: f32,
    key: f32,
    fill: f32,
    exposure: f32,
    bloom_intensity: f32,
    bloom_threshold: f32,
    bloom_spread: f32,
    saturation: f32,
    contrast: f32,
    gamma: f32,
    shadows: f32,
    midtones: f32,
    highlights: f32,
    temperature: f32,
    tint: f32,
    key_color: [f32; 3],
    fill_color: [f32; 3],
    key_angles: [f32; 2],
    fill_angles: [f32; 2],
    tonemapping: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            background: [0.18, 0.21, 0.25],
            ambient: 500.0,
            key: 6500.0,
            fill: 3200.0,
            exposure: super::graphics::arena_exposure().ev100,
            bloom_intensity: 0.22,
            bloom_threshold: 0.8,
            bloom_spread: 0.4,
            saturation: 1.0,
            contrast: 1.0,
            gamma: 1.0,
            shadows: 0.0,
            midtones: 1.0,
            highlights: 1.0,
            temperature: 0.0,
            tint: 0.0,
            key_color: [1.0, 0.95, 0.88],
            fill_color: [0.88, 0.93, 1.0],
            key_angles: [0.9_f32.to_degrees(), -0.55_f32.to_degrees()],
            fill_angles: [0.45_f32.to_degrees(), 2.3_f32.to_degrees()],
            tonemapping: 0,
        }
    }
}

impl Settings {
    pub fn from_values<const N: usize>(values: [Option<f64>; N]) -> Self {
        let defaults = Self::default();
        let read = |index: usize, default: f32, min: f64, max: f64| {
            values
                .get(index)
                .copied()
                .flatten()
                .filter(|v| v.is_finite())
                .map_or(default, |v| v.clamp(min, max) as f32)
        };
        Self {
            background: std::array::from_fn(|i| read(i, defaults.background[i], 0.0, 1.0)),
            ambient: read(3, defaults.ambient, 0.0, 3000.0),
            key: read(4, defaults.key, 0.0, 20000.0),
            fill: read(5, defaults.fill, 0.0, 20000.0),
            exposure: read(6, defaults.exposure, 4.0, 14.0),
            bloom_intensity: read(7, defaults.bloom_intensity, 0.0, 1.0),
            bloom_threshold: read(8, defaults.bloom_threshold, 0.0, 5.0),
            bloom_spread: read(9, defaults.bloom_spread, 0.0, 1.0),
            saturation: read(10, defaults.saturation, 0.0, 2.0),
            contrast: read(11, defaults.contrast, 0.5, 1.5),
            gamma: read(12, defaults.gamma, 0.5, 1.5),
            shadows: read(13, defaults.shadows, -0.1, 0.2),
            midtones: read(14, defaults.midtones, 0.25, 2.0),
            highlights: read(15, defaults.highlights, 0.25, 2.0),
            temperature: read(16, defaults.temperature, -0.05, 0.05),
            tint: read(17, defaults.tint, -0.05, 0.05),
            key_color: std::array::from_fn(|i| read(18 + i, defaults.key_color[i], 0.0, 1.0)),
            fill_color: std::array::from_fn(|i| read(21 + i, defaults.fill_color[i], 0.0, 1.0)),
            key_angles: [
                read(24, defaults.key_angles[0], 5.0, 90.0),
                read(25, defaults.key_angles[1], -180.0, 180.0),
            ],
            fill_angles: [
                read(26, defaults.fill_angles[0], 5.0, 90.0),
                read(27, defaults.fill_angles[1], -180.0, 180.0),
            ],
            tonemapping: read(28, 0.0, 0.0, 2.0).round() as u8,
        }
    }
}

#[derive(Component)]
pub enum ArenaLight {
    Key,
    Fill,
}

pub fn read_settings() -> Settings {
    super::platform::read_lighting()
}

pub fn apply(
    In(settings): In<Settings>,
    mut background: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut lights: Query<(&ArenaLight, &mut DirectionalLight, &mut Transform)>,
    mut cameras: Query<
        (
            &mut Exposure,
            &mut ColorGrading,
            &mut Tonemapping,
            Option<&mut Bloom>,
        ),
        With<Camera3d>,
    >,
) {
    let color = Color::srgb(
        settings.background[0],
        settings.background[1],
        settings.background[2],
    );
    if background.0 != color {
        background.0 = color;
    }
    if ambient.brightness != settings.ambient {
        ambient.brightness = settings.ambient;
    }
    for (role, mut light, mut transform) in &mut lights {
        let (value, color, angles) = match role {
            ArenaLight::Key => (settings.key, settings.key_color, settings.key_angles),
            ArenaLight::Fill => (settings.fill, settings.fill_color, settings.fill_angles),
        };
        if light.illuminance != value {
            light.illuminance = value;
        }
        let color = Color::srgb(color[0], color[1], color[2]);
        if light.color != color {
            light.color = color;
        }
        let rotation = Quat::from_euler(
            EulerRot::XYZ,
            -angles[0].to_radians(),
            angles[1].to_radians(),
            0.0,
        );
        if transform.rotation != rotation {
            transform.rotation = rotation;
        }
    }
    for (mut exposure, mut grading, mut tonemapping, bloom) in &mut cameras {
        if exposure.ev100 != settings.exposure {
            exposure.ev100 = settings.exposure;
        }
        let mut desired = ColorGrading::default();
        desired.global.post_saturation = settings.saturation;
        desired.global.temperature = settings.temperature;
        desired.global.tint = settings.tint;
        for section in [
            &mut desired.shadows,
            &mut desired.midtones,
            &mut desired.highlights,
        ] {
            section.contrast = settings.contrast;
            section.gamma = settings.gamma;
        }
        desired.shadows.lift = settings.shadows;
        desired.midtones.gain = settings.midtones;
        desired.highlights.gain = settings.highlights;
        if *grading != desired {
            *grading = desired;
        }
        let desired = match settings.tonemapping {
            1 => Tonemapping::ReinhardLuminance,
            2 => Tonemapping::AcesFitted,
            _ => Tonemapping::Reinhard,
        };
        if *tonemapping != desired {
            *tonemapping = desired;
        }
        // Run after graphics preferences. Reapply when bloom is re-enabled,
        // even when the lighting sliders themselves haven't changed.
        if let Some(mut bloom) = bloom {
            if bloom.intensity != settings.bloom_intensity {
                bloom.intensity = settings.bloom_intensity;
            }
            if bloom.prefilter.threshold != settings.bloom_threshold {
                bloom.prefilter.threshold = settings.bloom_threshold;
            }
            if bloom.low_frequency_boost != settings.bloom_spread {
                bloom.low_frequency_boost = settings.bloom_spread;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn boundary_defaults_and_clamps_each_presentation_field() {
        for value in [None, Some(f64::NAN), Some(f64::INFINITY)] {
            assert_eq!(
                Settings::from_values([value; VALUE_COUNT]),
                Settings::default()
            );
        }
        let low = Settings::from_values([Some(-1000.0); VALUE_COUNT]);
        assert_eq!(low.background, [0.0; 3]);
        assert_eq!(low.exposure, 4.0);
        assert_eq!(low.ambient, 0.0);
        let high = Settings::from_values([Some(1e30); VALUE_COUNT]);
        assert_eq!(high.background, [1.0; 3]);
        assert_eq!(high.ambient, 3000.0);
        assert_eq!(high.key, 20000.0);
        assert_eq!(high.fill, 20000.0);
        assert_eq!(high.exposure, 14.0);
        assert_eq!(high.bloom_intensity, 1.0);
        assert_eq!(high.bloom_threshold, 5.0);
        assert_eq!(high.bloom_spread, 1.0);
        assert_eq!((low.saturation, high.saturation), (0.0, 2.0));
        assert_eq!((low.contrast, high.contrast), (0.5, 1.5));
        assert_eq!((low.gamma, high.gamma), (0.5, 1.5));
        assert_eq!((low.shadows, high.shadows), (-0.1, 0.2));
        assert_eq!((low.midtones, high.midtones), (0.25, 2.0));
        assert_eq!((low.highlights, high.highlights), (0.25, 2.0));
        assert_eq!((low.temperature, high.temperature), (-0.05, 0.05));
        assert_eq!((low.tint, high.tint), (-0.05, 0.05));
        assert_eq!(low.key_color, [0.0; 3]);
        assert_eq!(high.fill_color, [1.0; 3]);
        assert_eq!(low.key_angles, [5.0, -180.0]);
        assert_eq!(high.fill_angles, [90.0, 180.0]);
        assert_eq!((low.tonemapping, high.tonemapping), (0, 2));
        // Callback ordering matches the JavaScript control catalog.
        let explicit = Settings::from_values([
            Some(0.1),
            Some(0.2),
            Some(0.3),
            Some(700.0),
            Some(8000.0),
            Some(4000.0),
            Some(9.0),
            Some(0.5),
            Some(1.5),
            Some(0.6),
        ]);
        assert_eq!(explicit.background, [0.1, 0.2, 0.3]);
        assert_eq!(
            (
                explicit.ambient,
                explicit.key,
                explicit.fill,
                explicit.exposure
            ),
            (700.0, 8000.0, 4000.0, 9.0)
        );
        assert_eq!(
            (
                explicit.bloom_intensity,
                explicit.bloom_threshold,
                explicit.bloom_spread
            ),
            (0.5, 1.5, 0.6)
        );
    }

    #[test]
    fn updates_live_entities_and_preserves_unrelated_lights_and_bloom_toggle() {
        let mut world = World::new();
        world.insert_resource(ClearColor::default());
        world.insert_resource(GlobalAmbientLight::default());
        let key = world
            .spawn((
                ArenaLight::Key,
                DirectionalLight::default(),
                Transform::default(),
            ))
            .id();
        let fill = world
            .spawn((
                ArenaLight::Fill,
                DirectionalLight::default(),
                Transform::default(),
            ))
            .id();
        let other = world
            .spawn(DirectionalLight {
                illuminance: 123.0,
                ..default()
            })
            .id();
        let camera = world
            .spawn((
                Camera3d::default(),
                Exposure::default(),
                ColorGrading::default(),
                Tonemapping::Reinhard,
                super::super::graphics::weapon_bloom(),
            ))
            .id();
        let settings = Settings {
            ambient: 900.0,
            key: 10000.0,
            fill: 4500.0,
            exposure: 11.0,
            bloom_intensity: 0.6,
            saturation: 1.2,
            contrast: 1.1,
            gamma: 0.9,
            shadows: 0.05,
            midtones: 1.3,
            highlights: 0.7,
            temperature: 0.01,
            tint: -0.02,
            key_color: [0.7, 0.8, 0.9],
            fill_color: [0.9, 0.8, 0.7],
            key_angles: [45.0, 90.0],
            tonemapping: 2,
            ..default()
        };
        world.run_system_once_with(apply, settings).unwrap();
        assert_eq!(world.resource::<GlobalAmbientLight>().brightness, 900.0);
        assert_eq!(
            world.get::<DirectionalLight>(key).unwrap().illuminance,
            10000.0
        );
        assert_eq!(
            world.get::<DirectionalLight>(fill).unwrap().illuminance,
            4500.0
        );
        assert_eq!(
            world.get::<DirectionalLight>(other).unwrap().illuminance,
            123.0
        );
        assert_eq!(world.get::<Exposure>(camera).unwrap().ev100, 11.0);
        assert_eq!(world.get::<Bloom>(camera).unwrap().intensity, 0.6);
        let grading = world.get::<ColorGrading>(camera).unwrap();
        assert_eq!(grading.global.post_saturation, 1.2);
        assert_eq!(grading.global.temperature, 0.01);
        assert_eq!(grading.global.tint, -0.02);
        assert_eq!(grading.shadows.lift, 0.05);
        assert_eq!(grading.midtones.gain, 1.3);
        assert_eq!(grading.highlights.gain, 0.7);
        assert_eq!(grading.highlights.contrast, 1.1);
        assert_eq!(grading.midtones.gamma, 0.9);
        assert_eq!(
            *world.get::<Tonemapping>(camera).unwrap(),
            Tonemapping::AcesFitted
        );
        assert_eq!(
            world.get::<DirectionalLight>(key).unwrap().color,
            Color::srgb(0.7, 0.8, 0.9)
        );
        assert_eq!(
            world.get::<DirectionalLight>(fill).unwrap().color,
            Color::srgb(0.9, 0.8, 0.7)
        );
        assert_eq!(
            world.get::<Transform>(key).unwrap().rotation,
            Quat::from_euler(
                EulerRot::XYZ,
                -45_f32.to_radians(),
                90_f32.to_radians(),
                0.0
            )
        );
        world.entity_mut(camera).remove::<Bloom>();
        world.run_system_once_with(apply, settings).unwrap();
        assert!(world.get::<Bloom>(camera).is_none());
        world
            .entity_mut(camera)
            .insert(super::super::graphics::weapon_bloom());
        world.run_system_once_with(apply, settings).unwrap();
        assert_eq!(world.get::<Bloom>(camera).unwrap().intensity, 0.6);
        world
            .run_system_once_with(apply, Settings::default())
            .unwrap();
        assert_eq!(world.get::<Exposure>(camera).unwrap().ev100, 10.0);
        assert_eq!(
            *world.get::<ColorGrading>(camera).unwrap(),
            ColorGrading::default()
        );
        assert_eq!(
            *world.get::<Tonemapping>(camera).unwrap(),
            Tonemapping::Reinhard
        );
    }

    #[test]
    fn appended_controls_keep_older_callbacks_compatible_and_follow_catalog_order() {
        assert_eq!(Settings::from_values([None; 10]), Settings::default());
        let mut values = [None; VALUE_COUNT];
        for (i, value) in [
            1.2, 1.1, 0.9, 0.05, 1.3, 0.7, 0.01, -0.02, 0.7, 0.8, 0.9, 0.9, 0.8, 0.7, 45.0, 90.0,
            30.0, -60.0, 1.0,
        ]
        .into_iter()
        .enumerate()
        {
            values[10 + i] = Some(value);
        }
        let settings = Settings::from_values(values);
        assert_eq!(
            (settings.saturation, settings.contrast, settings.gamma),
            (1.2, 1.1, 0.9)
        );
        assert_eq!(
            (settings.shadows, settings.midtones, settings.highlights),
            (0.05, 1.3, 0.7)
        );
        assert_eq!((settings.temperature, settings.tint), (0.01, -0.02));
        assert_eq!(settings.key_color, [0.7, 0.8, 0.9]);
        assert_eq!(settings.fill_color, [0.9, 0.8, 0.7]);
        assert_eq!(settings.key_angles, [45.0, 90.0]);
        assert_eq!(settings.fill_angles, [30.0, -60.0]);
        assert_eq!(settings.tonemapping, 1);
    }
}
