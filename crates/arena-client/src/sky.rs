//! The painted sky: an original 6×1024² cubemap rendered offline by
//! `arena-textures` (see `crates/arena-textures/src/sky.rs`) and shipped as a
//! vertically stacked PNG. Loading it is presentation only; if it fails, the
//! lighting panel's background colour shows instead.
use bevy::image::{
    ImageAddressMode, ImageArrayLayout, ImageFilterMode, ImageLoaderSettings, ImageSampler,
    ImageSamplerDescriptor,
};
use bevy::light::Skybox;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureViewDescriptor, TextureViewDimension};

pub const PATH: &str = "textures/sky.png";
/// Skybox luminance in cd/m²; with the arena's ~EV 9–10 exposure this keeps
/// the painted colours near their authored values after tonemapping.
pub(crate) const BRIGHTNESS: f32 = 1000.0;

#[derive(Resource)]
pub(crate) struct Sky {
    image: Handle<Image>,
    done: bool,
}

impl Sky {
    pub(crate) fn ready(&self) -> bool {
        self.done
    }
    pub(crate) fn load(server: &AssetServer) -> Self {
        let image = server
            .load_builder()
            .with_settings(|settings: &mut ImageLoaderSettings| {
                settings.array_layout = Some(ImageArrayLayout::RowCount { rows: 6 });
                settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::ClampToEdge,
                    address_mode_v: ImageAddressMode::ClampToEdge,
                    address_mode_w: ImageAddressMode::ClampToEdge,
                    mag_filter: ImageFilterMode::Linear,
                    min_filter: ImageFilterMode::Linear,
                    ..default()
                });
            })
            .load(PATH);
        Self { image, done: false }
    }
}

/// Views the loaded six-layer array as a cube, then attaches the skybox. The
/// camera only gains `Skybox` once the view is a cube, so the renderer never
/// binds the image with the wrong dimension.
pub(crate) fn attach(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut sky: ResMut<Sky>,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<Entity, With<Camera3d>>,
) {
    if sky.done {
        return;
    }
    if server
        .get_load_state(sky.image.id())
        .is_some_and(|state| state.is_failed())
    {
        warn!("Sky texture failed to load; keeping the background colour");
        sky.done = true;
        return;
    }
    let Some(mut image) = images.get_mut(sky.image.id()) else {
        return;
    };
    make_cube(&mut image);
    for camera in &cameras {
        commands.entity(camera).insert(Skybox {
            image: Some(sky.image.clone()),
            brightness: BRIGHTNESS,
            ..default()
        });
    }
    sky.done = true;
}

fn make_cube(image: &mut Image) {
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    #[test]
    fn stacked_faces_become_a_cube_view() {
        let mut image = Image::new_fill(
            Extent3d {
                width: 4,
                height: 24,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[255, 255, 255, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.reinterpret_stacked_2d_as_array(6).unwrap();
        make_cube(&mut image);
        assert_eq!(image.texture_descriptor.size.depth_or_array_layers, 6);
        assert_eq!(image.width(), image.height());
        assert_eq!(
            image.texture_view_descriptor.unwrap().dimension,
            Some(TextureViewDimension::Cube)
        );
    }
}
