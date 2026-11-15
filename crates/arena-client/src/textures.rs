//! Stylized surface textures for procedural geometry. Presentation only.
//!
//! Arena stone comes from prepared CC0 tiling PNGs (see assets/textures/README.md)
//! and is attached to its materials only after loading, so a missing file leaves
//! the original flat colour rather than invisible geometry. Character textures are
//! original "painted light" gradients generated here at startup.
use bevy::asset::RenderAssetUsages;
use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::TAU;

pub const FLOOR_PATH: &str = "textures/stone-floor.png";
pub const BRICKS_PATH: &str = "textures/stone-bricks.png";
/// Original painted pillar granite (`arena-textures` granite painter).
pub const GRANITE_PATH: &str = "textures/stone-granite.png";
pub const STONE_WALL_PATH: &str = "textures/stone-wall.png";
pub const WOOD_PATH: &str = "textures/wood-planks.png";
pub const GRATE_PATH: &str = "textures/metal-grate.png";
pub const PANELS_PATH: &str = "textures/metal-panels.png";
pub const SAND_PATH: &str = "textures/ground-sand.png";
pub const SNOW_PATH: &str = "textures/ground-snow.png";
pub const GRASS_PATH: &str = "textures/ground-grass.png";
/// Metres covered by one repeat of each tiling texture.
pub const FLOOR_TILE: f32 = 4.0;
pub const BRICKS_TILE: f32 = 2.0;
/// Granite has no direction, so it tiles in world space like the rest.
pub const GRANITE_TILE: f32 = 2.0;

fn sampler(address_v: ImageAddressMode) -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: address_v,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        // wgpu clamps this to 1 where anisotropic filtering is unavailable.
        anisotropy_clamp: 8,
        ..default()
    })
}

/// A unit cube whose UVs follow world position, so scaled boxes tile at a
/// constant texel density and neighbouring boxes continue the same pattern.
/// The transform keeps its scale because camera collision reads it.
pub fn tiled_cube(transform: &Transform, tile: f32) -> Mesh {
    let mut mesh = Cuboid::from_size(Vec3::ONE).mesh().build();
    let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(values)) => values.clone(),
        _ => unreachable!("cuboid meshes have float positions"),
    };
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(values)) => values.clone(),
        _ => unreachable!("cuboid meshes have float normals"),
    };
    let uvs: Vec<[f32; 2]> = positions
        .iter()
        .zip(&normals)
        .map(|(p, n)| {
            let world = transform.translation + Vec3::from(*p) * transform.scale;
            let n = Vec3::from(*n);
            let uv = if n.y.abs() > 0.5 {
                Vec2::new(world.x, world.z)
            } else if n.x.abs() > 0.5 {
                // Walls keep courses horizontal: v runs down the face.
                Vec2::new(world.z * n.x.signum(), -world.y)
            } else {
                Vec2::new(-world.x * n.z.signum(), -world.y)
            };
            (uv / tile).to_array()
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
}

/// A unit hexagonal prism (corners on ±X, flat faces on ±Z, matching the
/// rules' pillar) inscribed in the unit cube that `transform` scales, with
/// world-aligned UVs like `tiled_cube`: each side runs along its own face,
/// rows follow world height, and the caps follow world position.
pub fn tiled_hexagon(transform: &Transform, tile: f32) -> Mesh {
    let world = |p: Vec3| transform.translation + p * transform.scale;
    let corners = crate::prism::hexagon_corners(Vec3::ZERO, 0.5, 0.5);
    let (mut positions, mut normals, mut uvs, mut indices) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut face = |points: &[Vec3], normal: Vec3, uv: &dyn Fn(Vec3) -> Vec2| {
        let first = positions.len() as u32;
        for &p in points {
            positions.push(p.to_array());
            normals.push(normal.to_array());
            uvs.push((uv(world(p)) / tile).to_array());
        }
        // A fan, wound so its front faces outward.
        let winding = (points[1] - points[0])
            .cross(points[2] - points[0])
            .dot(normal);
        for i in 1..points.len() as u32 - 1 {
            if winding > 0.0 {
                indices.extend([first, first + i, first + i + 1]);
            } else {
                indices.extend([first, first + i + 1, first + i]);
            }
        }
    };
    for i in 0..6 {
        let (a, b) = (corners[i], corners[(i + 1) % 6]);
        // Local normal (Bevy applies the inverse-transpose for scale); the UV
        // runs along the face as it stands in the world.
        let edge = b - a;
        let normal = Vec3::new(edge.z, 0.0, -edge.x).normalize();
        let along = (world(b) - world(a)).normalize();
        let quad = [
            a - Vec3::Y * 0.5,
            b - Vec3::Y * 0.5,
            b + Vec3::Y * 0.5,
            a + Vec3::Y * 0.5,
        ];
        face(&quad, normal, &|w| Vec2::new(w.dot(along), -w.y));
    }
    let cap = |y: f32| corners.map(|c| c + Vec3::Y * y);
    face(&cap(0.5), Vec3::Y, &|w| Vec2::new(w.x, w.z));
    face(&cap(-0.5), Vec3::NEG_Y, &|w| Vec2::new(w.x, w.z));
    Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(bevy::mesh::Indices::U32(indices))
}

fn to_linear(value: u8) -> f32 {
    let c = f32::from(value) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(linear: f32) -> u8 {
    let c = linear.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round() as u8
}

/// Append a gamma-correct box-filtered mip chain to an sRGB RGBA8 image.
/// Bevy 0.20 does not generate mips for PNGs; without them the painted
/// detail shimmers at camera distance. Returns false if left unchanged.
pub fn add_mips(image: &mut Image) -> bool {
    if image.texture_descriptor.format != TextureFormat::Rgba8UnormSrgb
        || image.texture_descriptor.mip_level_count != 1
        || image.texture_descriptor.size.depth_or_array_layers != 1
    {
        return false;
    }
    let Some(base) = image.data.as_ref() else {
        return false;
    };
    let lut: Vec<f32> = (0..=255).map(to_linear).collect();
    let (mut width, mut height) = (image.width(), image.height());
    let mut data = base.clone();
    let mut level: Vec<f32> = base
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            [
                lut[p[0] as usize],
                lut[p[1] as usize],
                lut[p[2] as usize],
                f32::from(p[3]) / 255.0,
            ]
        })
        .collect();
    let mut levels = 1;
    while width > 1 || height > 1 {
        let (next_w, next_h) = ((width / 2).max(1), (height / 2).max(1));
        let mut next = vec![0.0; (next_w * next_h * 4) as usize];
        for y in 0..next_h {
            for x in 0..next_w {
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let sx = (x * 2 + dx).min(width - 1);
                    let sy = (y * 2 + dy).min(height - 1);
                    let from = ((sy * width + sx) * 4) as usize;
                    let to = ((y * next_w + x) * 4) as usize;
                    for c in 0..4 {
                        next[to + c] += level[from + c] / 4.0;
                    }
                }
            }
        }
        data.extend(next.as_chunks::<4>().0.iter().flat_map(|p| {
            [
                to_srgb(p[0]),
                to_srgb(p[1]),
                to_srgb(p[2]),
                (p[3] * 255.0).round() as u8,
            ]
        }));
        (width, height, level) = (next_w, next_h, next);
        levels += 1;
    }
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    true
}

struct Pending {
    image: Handle<Image>,
    materials: Vec<Handle<StandardMaterial>>,
    done: bool,
}

/// Arena textures waiting to be attached once loaded (and mipmapped).
#[derive(Resource)]
pub struct ArenaTextures {
    pending: Vec<Pending>,
}

impl ArenaTextures {
    pub(crate) fn ready(&self) -> bool {
        self.pending.iter().all(|p| p.done)
    }
    /// Loads each tiling texture and remembers which materials it belongs to.
    pub fn load(
        server: &AssetServer,
        sets: impl IntoIterator<Item = (&'static str, Vec<Handle<StandardMaterial>>)>,
    ) -> Self {
        Self {
            pending: sets
                .into_iter()
                .map(|(path, materials)| Pending {
                    image: server
                        .load_builder()
                        .with_settings(|settings: &mut ImageLoaderSettings| {
                            settings.sampler = sampler(ImageAddressMode::Repeat);
                        })
                        .load(path),
                    materials,
                    done: false,
                })
                .collect(),
        }
    }
}

pub fn attach_arena_textures(
    server: Res<AssetServer>,
    mut textures: ResMut<ArenaTextures>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for pending in textures.pending.iter_mut().filter(|p| !p.done) {
        if server
            .get_load_state(pending.image.id())
            .is_some_and(|state| state.is_failed())
        {
            warn!("Arena texture failed to load; keeping flat colours");
            pending.done = true;
            continue;
        }
        let Some(mut image) = images.get_mut(pending.image.id()) else {
            continue;
        };
        add_mips(&mut image);
        for handle in &pending.materials {
            if let Some(mut material) = materials.get_mut(handle.id()) {
                material.base_color_texture = Some(pending.image.clone());
            }
        }
        pending.done = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    Skin,
    Cloth,
    Leather,
    Hair,
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Soft, wrap-around brush variation in roughly -1..1 (periodic in u).
fn mottle(u: f32, v: f32, seed: f32) -> f32 {
    (f32::sin(TAU * 3.0 * u + 5.0 * v + seed)
        + 0.6 * f32::sin(TAU * 5.0 * u - 9.0 * v + 1.7 + seed)
        + 0.4 * f32::sin(TAU * 8.0 * u + 13.0 * v + 2.0 * seed))
        / 2.0
}

/// Linear albedo multiplier for sphere/capsule UVs (u around, v top→bottom).
/// Light is painted in: warm tops fall to cool, hue-shifted undersides.
pub fn paint(kind: Paint, u: f32, v: f32) -> Vec3 {
    let shade = smoothstep(0.15, 1.0, v);
    let (light, shadow, variation) = match kind {
        Paint::Skin => (
            Vec3::new(1.0, 0.97, 0.93),
            Vec3::new(0.78, 0.68, 0.76),
            0.025,
        ),
        Paint::Cloth => (
            Vec3::new(0.97, 0.95, 0.90),
            Vec3::new(0.58, 0.60, 0.75),
            0.05,
        ),
        Paint::Leather => (
            Vec3::new(1.0, 0.95, 0.88),
            Vec3::new(0.52, 0.52, 0.64),
            0.07,
        ),
        Paint::Hair => (Vec3::new(1.0, 0.97, 0.92), Vec3::new(0.56, 0.52, 0.64), 0.0),
    };
    let mut color = light.lerp(shadow, shade) * (1.0 + variation * mottle(u, v, 0.4));
    match kind {
        Paint::Skin => {}
        Paint::Cloth => {
            // A darker hem with a dashed light stitch above it.
            let hem = smoothstep(0.79, 0.81, v) * (1.0 - smoothstep(0.89, 0.91, v));
            color *= 1.0 - 0.28 * hem;
            let stitch = (1.0 - smoothstep(0.004, 0.010, (v - 0.775).abs()))
                * f32::from(u8::from((u * 40.0).fract() < 0.55));
            color = color.lerp(Vec3::new(1.0, 0.97, 0.90), stitch * 0.85);
        }
        Paint::Leather => {
            let scuff = smoothstep(0.55, 0.9, mottle(u * 2.0 % 1.0, v * 1.7, 2.3));
            color = color.lerp(Vec3::splat(1.0), 0.10 * scuff);
        }
        Paint::Hair => {
            // Strands radiate from the crown pole; a sheen band catches the light.
            let s = (u * 36.0 + 0.08 * f32::sin(TAU * 4.0 * u)).fract();
            let strand = 0.78 + 0.22 * (1.0 - (2.0 * s - 1.0).powi(2));
            let sheen = (-((v - 0.32) / 0.07).powi(2)).exp() * (0.4 + 0.6 * strand);
            color = color * strand + Vec3::splat(0.22 * sheen);
        }
    }
    color.min(Vec3::ONE).max(Vec3::ZERO)
}

/// Shared character textures, created once; materials reference them.
#[derive(Resource, Clone, Default)]
pub struct PaintedTextures {
    pub skin: Handle<Image>,
    pub cloth: Handle<Image>,
    pub leather: Handle<Image>,
    pub hair: Handle<Image>,
}

pub fn painted_image(kind: Paint) -> Image {
    let (width, height) = if kind == Paint::Hair {
        (256, 64)
    } else {
        (128, 64)
    };
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let color = paint(
                kind,
                (x as f32 + 0.5) / width as f32,
                (y as f32 + 0.5) / height as f32,
            );
            data.extend([to_srgb(color.x), to_srgb(color.y), to_srgb(color.z), 255]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = sampler(ImageAddressMode::ClampToEdge);
    add_mips(&mut image);
    image
}

impl PaintedTextures {
    pub fn new(images: &mut Assets<Image>) -> Self {
        Self {
            skin: images.add(painted_image(Paint::Skin)),
            cloth: images.add(painted_image(Paint::Cloth)),
            leather: images.add(painted_image(Paint::Leather)),
            hair: images.add(painted_image(Paint::Hair)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hexagon_prism_faces_outward_and_fits_its_box() {
        let transform = Transform::from_xyz(2.0, 5.0, -3.0).with_scale(Vec3::new(9.0, 10.0, 9.0));
        let mesh = tiled_hexagon(&transform, 2.0);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            unreachable!()
        };
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            unreachable!()
        };
        let Some(bevy::mesh::Indices::U32(indices)) = mesh.indices() else {
            unreachable!()
        };
        // Six quads and two six-corner caps, in the unit cube the transform
        // scales (the venue keeps the scaled transform on the entity).
        assert_eq!(positions.len(), 6 * 4 + 2 * 6);
        assert_eq!(indices.len(), (6 * 2 + 2 * 4) * 3);
        for p in positions {
            assert!(Vec3::from(*p).abs().max_element() <= 0.5 + 1e-6);
        }
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            unreachable!()
        };
        // Side UVs span the real 10 m height at the 2 m tile.
        let v: Vec<f32> = uvs.iter().take(24).map(|uv| uv[1]).collect();
        let span =
            v.iter().copied().fold(f32::MIN, f32::max) - v.iter().copied().fold(f32::MAX, f32::min);
        assert!((span - 5.0).abs() < 1e-4);
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(positions[triangle[k] as usize]));
            let facing = (b - a).cross(c - a);
            let normal = Vec3::from(normals[triangle[0] as usize]);
            assert!(facing.dot(normal) > 0.0, "inward triangle {triangle:?}");
            // Every face points away from the prism's axis.
            assert!(((a + b + c) / 3.0).dot(normal) > 0.0);
        }
    }

    #[test]
    fn tiled_cube_uvs_follow_world_scale_and_continue_across_neighbours() {
        let uv_range = |transform: Transform, axis: Vec3| {
            let mesh = tiled_cube(&transform, 2.0);
            let Some(VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                unreachable!()
            };
            let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                unreachable!()
            };
            let face: Vec<Vec2> = normals
                .iter()
                .zip(uvs)
                .filter(|(n, _)| Vec3::from(**n).abs_diff_eq(axis, 0.01))
                .map(|(_, uv)| Vec2::from(*uv))
                .collect();
            let min = face.iter().copied().reduce(Vec2::min).unwrap();
            let max = face.iter().copied().reduce(Vec2::max).unwrap();
            (min, max)
        };
        // A 10 m × 4 m floor top spans 5 × 2 repeats of a 2 m tile.
        let floor = Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(10.0, 0.2, 4.0));
        let (min, max) = uv_range(floor, Vec3::Y);
        assert!((max - min).abs_diff_eq(Vec2::new(5.0, 2.0), 1e-4));
        // A 4 m-tall pillar face spans two repeats vertically, courses level.
        let pillar = Transform::from_xyz(3.0, 2.0, 0.0).with_scale(Vec3::new(1.0, 4.0, 1.0));
        let (min, max) = uv_range(pillar, Vec3::Z);
        assert!((max - min).abs_diff_eq(Vec2::new(0.5, 2.0), 1e-4));
        // Adjacent boxes share the seam's UV, so the pattern continues.
        let left = Transform::from_xyz(-1.0, 0.0, 0.0).with_scale(Vec3::new(2.0, 1.0, 1.0));
        let right = Transform::from_xyz(1.0, 0.0, 0.0).with_scale(Vec3::new(2.0, 1.0, 1.0));
        assert!((uv_range(left, Vec3::Y).1.x - uv_range(right, Vec3::Y).0.x).abs() < 1e-4);
    }

    #[test]
    fn mips_halve_to_one_pixel_and_preserve_average_brightness() {
        let mut image = Image::new(
            Extent3d {
                width: 8,
                height: 4,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            (0..32)
                .flat_map(|i| {
                    if i % 2 == 0 {
                        [255, 255, 255, 255]
                    } else {
                        [0, 0, 0, 255]
                    }
                })
                .collect(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        assert!(add_mips(&mut image));
        assert_eq!(image.texture_descriptor.mip_level_count, 4);
        let data = image.data.as_ref().unwrap();
        assert_eq!(data.len(), (32 + 8 + 2 + 1) * 4);
        // Black/white columns average to linear 0.5, not sRGB mid-grey 128.
        assert_eq!(&data[data.len() - 4..], &[188, 188, 188, 255]);
        // Already-mipped or non-sRGB images are left alone.
        assert!(!add_mips(&mut image));
    }

    #[test]
    fn painted_light_falls_from_warm_tops_to_cool_undersides_and_wraps() {
        for kind in [Paint::Skin, Paint::Cloth, Paint::Leather, Paint::Hair] {
            let top = paint(kind, 0.3, 0.05);
            let bottom = paint(kind, 0.3, 0.97);
            assert!(top.element_sum() > bottom.element_sum(), "{kind:?}");
            // Shadows shift cooler than their highlights (more blue per red).
            assert!(
                bottom.z / bottom.x > top.z / top.x + 0.03,
                "{kind:?} hue shift"
            );
            for v in [0.1, 0.5, 0.8] {
                assert!(
                    paint(kind, 0.0, v).abs_diff_eq(paint(kind, 1.0, v), 0.02),
                    "{kind:?} seam"
                );
            }
            let image = painted_image(kind);
            assert!(image.texture_descriptor.mip_level_count > 1);
        }
        // The cloth hem is darker than the cloth just above it.
        assert!(paint(Paint::Cloth, 0.3, 0.85).x < paint(Paint::Cloth, 0.3, 0.74).x);
    }
}
