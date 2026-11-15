//! Original painted heraldry for the venue: hanging swallowtail banners and
//! rooftop pennants. Textures are painted in code at startup (no downloads):
//! a lit cloth field, a gold-embroidered border with stitching, a medallion
//! bearing one of three original sigils, and a chevron hem. Meshes carry real
//! folds and swallowtail/pennant outlines instead of flat boxes.
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::{PI, TAU};

/// Banner cloth size in metres.
pub(crate) const BANNER_WIDTH: f32 = 4.0;
pub(crate) const BANNER_HEIGHT: f32 = 10.0;
/// Swallowtail notch depth as a fraction of height.
const NOTCH: f32 = 0.14;
pub(crate) const PENNANT_LENGTH: f32 = 5.0;
pub(crate) const PENNANT_HEIGHT: f32 = 1.6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sigil {
    Sun,
    Moon,
    Crown,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Livery {
    field: Vec3,
    accent: Vec3,
    pub(crate) sigil: Sigil,
}

/// Deep, saturated fantasy liveries that stay clear of the teal/orange team
/// colours reserved for gameplay.
pub(crate) const LIVERIES: [Livery; 3] = [
    Livery {
        field: Vec3::new(0.40, 0.17, 0.62),
        accent: Vec3::new(0.97, 0.88, 0.62),
        sigil: Sigil::Sun,
    },
    Livery {
        field: Vec3::new(0.13, 0.27, 0.70),
        accent: Vec3::new(0.86, 0.92, 1.0),
        sigil: Sigil::Moon,
    },
    Livery {
        field: Vec3::new(0.10, 0.46, 0.30),
        accent: Vec3::new(0.98, 0.86, 0.52),
        sigil: Sigil::Crown,
    },
];

const GOLD_LIGHT: Vec3 = Vec3::new(1.0, 0.86, 0.46);
const GOLD: Vec3 = Vec3::new(0.90, 0.64, 0.24);
const GOLD_DARK: Vec3 = Vec3::new(0.52, 0.30, 0.12);
const INK: Vec3 = Vec3::new(0.16, 0.09, 0.14);

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Soft painterly brush variation in roughly −1..1.
fn brush(x: f32, y: f32) -> f32 {
    ((x * 3.1 + y * 0.7).sin() * 0.5
        + (x * 1.3 - y * 2.3 + 1.7).sin() * 0.3
        + (x * 7.0 + y * 5.0 + 0.4).sin() * 0.2)
        .clamp(-1.0, 1.0)
}

/// Swallowtail bottom edge (fraction of height) at horizontal fraction `u`.
fn cut(u: f32) -> f32 {
    1.0 - NOTCH * (1.0 - (2.0 * u - 1.0).abs())
}

/// Whether point `p` (medallion units, y down) lies inside the sigil.
fn sigil_contains(sigil: Sigil, p: Vec2) -> bool {
    match sigil {
        Sigil::Sun => {
            let r = p.length();
            let k = (p.y.atan2(p.x) / TAU + 0.5) * 16.0;
            let f = k - k.round();
            let long = (k.round() as i32).rem_euclid(2) == 0;
            let reach = if long { 0.55 } else { 0.32 };
            let disc = r < 0.42 && !(0.27..0.32).contains(&r);
            disc || (r >= 0.42 && r < 0.42 + reach * (1.0 - 2.0 * f.abs()))
        }
        Sigil::Moon => {
            let crescent = p.length() < 0.82 && (p - Vec2::new(0.34, -0.2)).length() > 0.66;
            let q = (p - Vec2::new(0.42, -0.42)).abs();
            let star = q.x.sqrt() + q.y.sqrt() < 0.52;
            crescent || star
        }
        Sigil::Crown => {
            let x = p.x.abs();
            if x > 0.78 {
                return false;
            }
            let band = (0.12..0.55).contains(&p.y);
            let peak = [0.0_f32, 0.66]
                .iter()
                .map(|c| (x - c).abs())
                .fold(f32::MAX, f32::min);
            let points = p.y < 0.12 && p.y > -0.58 + peak * 1.55;
            let jewels = [Vec2::new(0.0, -0.66), Vec2::new(0.66, -0.66)]
                .iter()
                .any(|j| (Vec2::new(x, p.y) - *j).length() < 0.13);
            let gem = (p - Vec2::new(0.0, 0.33)).length() < 0.11;
            (band && !gem) || points || jewels
        }
    }
}

/// Gold with painted light from the upper left.
fn gold(p: Vec2) -> Vec3 {
    let t = smoothstep(-0.9, 0.9, -0.6 * p.x - p.y);
    GOLD_DARK
        .lerp(GOLD, smoothstep(0.0, 0.5, t))
        .lerp(GOLD_LIGHT, smoothstep(0.5, 1.0, t))
}

/// Medallion with sigil at metres `m` from its centre, radius `radius`.
/// Returns `None` outside the medallion so the field shows through.
fn medallion(livery: &Livery, m: Vec2, radius: f32) -> Option<Vec3> {
    let p = m / radius;
    let r = p.length();
    if r > 1.12 {
        return None;
    }
    if r > 1.02 {
        return Some(gold(p).lerp(INK, smoothstep(1.08, 1.12, r)));
    }
    if r > 0.97 {
        return Some(INK);
    }
    // A darker roundel ground so the gold sigil reads from far away.
    let ground = livery.field * (0.55 + 0.15 * smoothstep(1.0, 0.0, r));
    if sigil_contains(livery.sigil, p) {
        return Some(gold(p));
    }
    // Shrinking coordinates grows the shape: the difference is an ink outline.
    if sigil_contains(livery.sigil, p * 0.9) {
        return Some(INK);
    }
    Some(ground)
}

/// Embroidered gold border: a raised band with stitched dashes and ink edges.
fn border(distance: f32, along: f32, width: f32) -> Option<Vec3> {
    if distance > width + 0.05 {
        return None;
    }
    if distance > width || distance < 0.035 {
        return Some(INK);
    }
    let across = distance / width;
    let ridge = 1.0 - (across * 2.0 - 1.0).abs();
    let mut color = GOLD_DARK
        .lerp(GOLD, ridge.sqrt())
        .lerp(GOLD_LIGHT, ridge.powi(4) * 0.6);
    let stitch = (along / 0.28).fract() < 0.55;
    if stitch && (across - 0.5).abs() < 0.09 {
        color = GOLD_DARK;
    }
    Some(color)
}

/// sRGB colour of the banner at texture coordinates (u right, v down).
pub(crate) fn banner_pixel(livery: &Livery, u: f32, v: f32) -> Vec3 {
    let (w, h) = (BANNER_WIDTH, BANNER_HEIGHT);
    let (x, y) = (u * w, v * h);
    let slope = NOTCH * h / (w * 0.5);
    let to_cut = (cut(u) - v) * h / (1.0 + slope * slope).sqrt();
    let side = x.min(w - x);
    let edge = side.min(y).min(to_cut.max(0.0));
    let along = if edge == side { y } else { x };
    // Painted light: brighter top, cool deeper hem, soft brushwork.
    let lit = 1.18 - 0.4 * smoothstep(0.0, 1.0, v) + 0.05 * brush(x, y);
    let fold = 0.06 * (u * TAU * 1.5).cos();
    let mut color = livery.field * (lit + fold);
    // Header sleeve where the rod passes through.
    if (0.0..1.3).contains(&y) {
        color = livery.field * 0.7;
    }
    if (1.3..1.42).contains(&y) || (1.6..1.66).contains(&y) {
        color = livery.accent;
    }
    // Chevron hem following the swallowtail.
    if (0.9..1.4).contains(&to_cut) {
        color = livery.accent * (0.9 + 0.1 * brush(x * 2.0, y));
    } else if (1.4..1.48).contains(&to_cut) || (0.82..0.9).contains(&to_cut) {
        color = INK;
    } else if (1.48..1.95).contains(&to_cut) {
        // A row of small gold diamonds.
        let cell = (x / 0.5).fract() - 0.5;
        let mid = (to_cut - 1.715) / 0.22;
        if cell.abs() + mid.abs() < 0.45 {
            color = gold(Vec2::new(cell, mid));
        }
    }
    // Inner accent line inside the border.
    if (0.5..0.56).contains(&edge) && y > 1.3 {
        color = livery.accent;
    }
    if let Some(sigil) = medallion(livery, Vec2::new(x - w * 0.5, y - 4.1), 1.45) {
        color = sigil;
    }
    if let Some(trim) = border(edge, along, 0.28) {
        color = trim;
    }
    color.clamp(Vec3::ZERO, Vec3::ONE)
}

/// sRGB colour of a pennant at (u along from the pole, v down).
pub(crate) fn pennant_pixel(livery: &Livery, u: f32, v: f32) -> Vec3 {
    let (l, h) = (PENNANT_LENGTH, PENNANT_HEIGHT);
    let (x, y) = (u * l, v * h);
    // Distance to the slanted edges of the triangle, in metres.
    let half = 0.5 * h * (1.0 - u);
    let slope = 0.5 * h / l;
    let slant = (half - (y - 0.5 * h).abs()) / (1.0 + slope * slope).sqrt();
    let edge = x.min(slant.max(0.0));
    let lit = 1.15 - 0.3 * v + 0.05 * brush(x * 2.0, y * 2.0);
    let mut color = livery.field * lit;
    if (0.35..0.45).contains(&edge) {
        color = livery.accent;
    }
    if let Some(sigil) = medallion(livery, Vec2::new(x - 0.85, y - 0.5 * h), 0.5) {
        color = sigil;
    }
    if let Some(trim) = border(edge, x + y, 0.16) {
        color = trim;
    }
    color.clamp(Vec3::ZERO, Vec3::ONE)
}

/// Paints an image with 2×2 supersampling so sigil edges stay smooth.
fn paint(width: u32, height: u32, pixel: impl Fn(f32, f32) -> Vec3) -> Image {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let mut sum = Vec3::ZERO;
            for (dx, dy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                sum += pixel(
                    (x as f32 + dx) / width as f32,
                    (y as f32 + dy) / height as f32,
                );
            }
            let c = sum * 0.25;
            data.extend(
                c.to_array()
                    .map(|c| (c * 255.0).round() as u8)
                    .into_iter()
                    .chain([255]),
            );
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
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    crate::textures::add_mips(&mut image);
    image
}

pub(crate) fn banner_image(livery: &Livery) -> Image {
    paint(256, 640, |u, v| banner_pixel(livery, u, v))
}

pub(crate) fn pennant_image(livery: &Livery) -> Image {
    paint(320, 104, |u, v| pennant_pixel(livery, u, v))
}

/// Builds a grid mesh from a parametric surface over (u, v) in 0..1. Normals
/// come from the surface derivatives and face the local +Z side.
fn surface(columns: u32, rows: u32, point: impl Fn(f32, f32) -> (Vec3, Vec2)) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let e = 1e-3;
    for j in 0..=rows {
        for i in 0..=columns {
            let (s, t) = (i as f32 / columns as f32, j as f32 / rows as f32);
            let (p, uv) = point(s, t);
            let du = point((s + e).min(1.0), t).0 - point((s - e).max(0.0), t).0;
            let dv = point(s, (t + e).min(1.0)).0 - point(s, (t - e).max(0.0)).0;
            let mut n = dv.cross(du).normalize_or(Vec3::Z);
            if n.z < 0.0 {
                n = -n;
            }
            positions.push(p.to_array());
            normals.push(n.to_array());
            uvs.push(uv.to_array());
        }
    }
    let mut indices = Vec::new();
    let stride = columns + 1;
    for j in 0..rows {
        for i in 0..columns {
            let a = j * stride + i;
            let (b, c, d) = (a + 1, a + stride + 1, a + stride);
            // Counter-clockwise from +Z: rows run downward, columns rightward.
            indices.extend([a, d, c, a, c, b]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// Hanging banner: top edge at the origin, hanging down −Y, facing +Z, with
/// three soft vertical folds that deepen toward the swallowtail hem.
pub(crate) fn banner_mesh() -> Mesh {
    surface(16, 24, |s, t| {
        let v = t * cut(s);
        let depth = 0.14 * (0.35 + 0.65 * v);
        let z = depth * (s * TAU * 1.5).sin();
        let position = Vec3::new((s - 0.5) * BANNER_WIDTH, -v * BANNER_HEIGHT, z + depth);
        (position, Vec2::new(s, v))
    })
}

/// Pennant flying along local +X from a pole at the origin, rippling in wind.
pub(crate) fn pennant_mesh(phase: f32) -> Mesh {
    surface(20, 6, |s, t| {
        let half = 0.5 * (1.0 - s) + 0.01;
        let v = 0.5 + (t - 0.5) * 2.0 * half;
        let z = 0.32 * s * (s * TAU * 1.1 + phase).sin();
        let droop = 0.25 * s * s;
        let position = Vec3::new(s * PENNANT_LENGTH, (0.5 - v) * PENNANT_HEIGHT - droop, z);
        (position, Vec2::new(s, v))
    })
}

/// Bottom tips of the swallowtail, in banner-local space.
pub(crate) fn tail_tips() -> [Vec3; 2] {
    [
        Vec3::new(-BANNER_WIDTH * 0.5, -BANNER_HEIGHT, 0.05),
        Vec3::new(BANNER_WIDTH * 0.5, -BANNER_HEIGHT, 0.05),
    ]
}

/// A gentle downward tassel orientation helper.
pub(crate) fn hanging() -> Quat {
    Quat::from_rotation_x(PI)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banners_have_gold_borders_sigils_and_lit_fields() {
        for livery in &LIVERIES {
            let edge = banner_pixel(livery, 0.01, 0.5);
            let field_top = banner_pixel(livery, 0.2, 0.25);
            let field_low = banner_pixel(livery, 0.2, 0.7);
            // A point inside each sigil, in medallion units.
            let inside = match livery.sigil {
                Sigil::Moon => Vec2::new(-0.6, 0.0),
                Sigil::Sun | Sigil::Crown => Vec2::ZERO,
            };
            let sigil = banner_pixel(
                livery,
                0.5 + inside.x * 1.45 / BANNER_WIDTH,
                (4.1 + inside.y * 1.45) / BANNER_HEIGHT,
            );
            // Gold reads warm (red over blue); the field keeps its hue.
            assert!(edge.x > edge.z, "{edge:?}");
            assert!(sigil.x > sigil.z, "{sigil:?}");
            assert!(field_top.length() > field_low.length());
            let hue = livery.field.normalize();
            assert!(field_top.normalize().dot(hue) > 0.95);
        }
    }

    #[test]
    fn sigils_are_distinct_and_centred() {
        let mut shapes = Vec::new();
        for sigil in [Sigil::Sun, Sigil::Moon, Sigil::Crown] {
            let mut mask = Vec::new();
            for y in 0..24 {
                for x in 0..24 {
                    let p = Vec2::new(x as f32, y as f32) / 12.0 - Vec2::ONE;
                    mask.push(sigil_contains(sigil, p));
                }
            }
            let filled = mask.iter().filter(|m| **m).count();
            assert!((60..400).contains(&filled), "{sigil:?} {filled}");
            shapes.push(mask);
        }
        assert_ne!(shapes[0], shapes[1]);
        assert_ne!(shapes[1], shapes[2]);
    }

    #[test]
    fn meshes_follow_the_cloth_outlines_and_face_outward() {
        let mesh = banner_mesh();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            unreachable!()
        };
        let lowest_middle = p
            .iter()
            .filter(|p| p[0].abs() < 0.1)
            .map(|p| p[1])
            .fold(f32::MAX, f32::min);
        // The notch rises above the tails.
        assert!(lowest_middle > -BANNER_HEIGHT * (1.0 - NOTCH) - 0.01);
        assert!(p.iter().any(|p| p[1] < -BANNER_HEIGHT + 0.01));
        assert!(p.iter().all(|p| p[2] >= -1e-4));
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(n)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            unreachable!()
        };
        assert!(n.iter().all(|n| n[2] > 0.5));
        let pennant = pennant_mesh(0.0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
            pennant.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            unreachable!()
        };
        assert!(
            p.iter()
                .all(|p| (0.0..=PENNANT_LENGTH + 1e-4).contains(&p[0]))
        );
    }
}
