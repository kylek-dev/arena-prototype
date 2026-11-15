//! Offline, bounded character textures. No client/runtime dependency.
use image::{Rgba, RgbaImage, imageops::FilterType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Color,
    Normal,
    Data,
}

fn linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn resize(source: &RgbaImage, maximum: u32, kind: Kind) -> Result<RgbaImage, String> {
    let (w, h) = source.dimensions();
    if maximum == 0 || w == 0 || h == 0 {
        return Err("texture dimensions must be positive".into());
    }
    if w.max(h) <= maximum {
        return Ok(source.clone());
    }
    let scale = maximum as f64 / f64::from(w.max(h));
    let width = (f64::from(w) * scale).round().max(1.0) as u32;
    let height = (f64::from(h) * scale).round().max(1.0) as u32;
    if kind == Kind::Color {
        // Filter albedo in linear light, with premultiplied alpha to avoid
        // dark/color fringes around hair or other cutout geometry.
        let mut input = image::Rgba32FImage::new(w, h);
        for (dst, src) in input.pixels_mut().zip(source.pixels()) {
            let alpha = f32::from(src[3]) / 255.0;
            *dst = Rgba([
                linear(f32::from(src[0]) / 255.0) * alpha,
                linear(f32::from(src[1]) / 255.0) * alpha,
                linear(f32::from(src[2]) / 255.0) * alpha,
                alpha,
            ]);
        }
        let filtered = image::imageops::resize(&input, width, height, FilterType::Triangle);
        return Ok(RgbaImage::from_fn(width, height, |x, y| {
            let p = filtered.get_pixel(x, y);
            let alpha = p[3];
            let color = if alpha > 1e-6 {
                [
                    byte(srgb(p[0] / alpha)),
                    byte(srgb(p[1] / alpha)),
                    byte(srgb(p[2] / alpha)),
                ]
            } else {
                [0; 3]
            };
            Rgba([color[0], color[1], color[2], byte(alpha)])
        }));
    }
    let mut output = image::imageops::resize(source, width, height, FilterType::Triangle);
    if kind == Kind::Normal {
        for p in output.pixels_mut() {
            let mut n = [p[0], p[1], p[2]].map(|v| f32::from(v) / 127.5 - 1.0);
            let length = n.iter().map(|v| v * v).sum::<f32>().sqrt();
            if length > 1e-5 {
                n = n.map(|v| v / length);
            } else {
                n = [0.0, 0.0, 1.0];
            }
            for (channel, value) in p.0[..3].iter_mut().zip(n) {
                *channel = byte(value * 0.5 + 0.5);
            }
        }
    }
    Ok(output)
}

fn luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

fn saturation(p: &Rgba<u8>) -> f32 {
    let max = p.0[..3].iter().copied().max().unwrap_or(0);
    let min = p.0[..3].iter().copied().min().unwrap_or(0);
    if max == 0 {
        0.0
    } else {
        f32::from(max - min) / f32::from(max)
    }
}

/// Located iris in a centred eye texture: pupil centre and iris radius, pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Iris {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

/// Finds the pupil and dark limbal ring (the only near-black areas; their
/// bounding box is centred on the iris even when a specular highlight covers
/// part of the pupil), then walks outward past coloured iris to pale sclera.
/// Returns `None` for textures without that structure.
pub fn locate_iris(source: &RgbaImage) -> Option<Iris> {
    let (w, h) = source.dimensions();
    let (mut low, mut high) = ((u32::MAX, u32::MAX), (0, 0));
    for (x, y, p) in source.enumerate_pixels() {
        if p.0[..3].iter().all(|v| *v < 50) {
            low = (low.0.min(x), low.1.min(y));
            high = (high.0.max(x), high.1.max(y));
        }
    }
    if low.0 > high.0 {
        return None;
    }
    let cx = (low.0 + high.0) as f32 * 0.5;
    let cy = (low.1 + high.1) as f32 * 0.5;
    let limit = w.min(h) as f32 * 0.45;
    let mut distances: Vec<f32> = (0..32)
        .filter_map(|i| {
            let angle = i as f32 / 32.0 * std::f32::consts::TAU;
            let (dx, dy) = (angle.cos(), angle.sin());
            let mut r = 1.0;
            let mut coloured = 0;
            while r < limit {
                let (x, y) = ((cx + dx * r).round(), (cy + dy * r).round());
                if x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 {
                    return None;
                }
                let p = source.get_pixel(x as u32, y as u32);
                let value = f32::from(*p.0[..3].iter().max().unwrap_or(&0)) / 255.0;
                if saturation(p) > 0.4 && (0.1..0.7).contains(&value) {
                    coloured += 1;
                }
                if coloured >= 3 && value > 0.55 && saturation(p) < 0.3 {
                    return Some(r);
                }
                r += 0.5;
            }
            None
        })
        .collect();
    if distances.len() < 16 {
        return None;
    }
    distances.sort_by(f32::total_cmp);
    let radius = distances[distances.len() / 2];
    (radius > 2.0).then_some(Iris {
        x: cx,
        y: cy,
        radius,
    })
}

/// Recolours only the iris: luminance detail is kept (pupil stays dark, the
/// pale specular dot stays pale) and the average iris tone becomes `target`.
pub fn recolor_iris(source: &RgbaImage, target: [u8; 3]) -> Result<RgbaImage, String> {
    let iris = locate_iris(source).ok_or("no pupil/iris found in eye texture")?;
    let lin = |p: &Rgba<u8>| [0, 1, 2].map(|i| linear(f32::from(p[i]) / 255.0));
    let inside = |x: u32, y: u32| {
        let d = ((x as f32 - iris.x).powi(2) + (y as f32 - iris.y).powi(2)).sqrt();
        d / iris.radius
    };
    let (mut total, mut count) = (0.0f32, 0u32);
    for (x, y, p) in source.enumerate_pixels() {
        let d = inside(x, y);
        if (0.45..0.9).contains(&d) && saturation(p) > 0.2 {
            total += luminance(lin(p));
            count += 1;
        }
    }
    if count == 0 || total <= 0.0 {
        return Err("iris has no coloured pixels".into());
    }
    let mean = total / count as f32;
    let target = target.map(|v| linear(f32::from(v) / 255.0));
    Ok(RgbaImage::from_fn(
        source.width(),
        source.height(),
        |x, y| {
            let p = source.get_pixel(x, y);
            let d = inside(x, y);
            // Full effect inside the iris, fading out across its rim.
            let edge = ((1.08 - d) / 0.13).clamp(0.0, 1.0);
            let weight = edge * (saturation(p) / 0.25).clamp(0.0, 1.0);
            if weight <= 0.0 {
                return *p;
            }
            let rgb = lin(p);
            // Slightly under the mean keeps bright rim pixels from glowing.
            let scale = (luminance(rgb) / mean).min(1.6) * 0.8;
            let mut out = [0u8; 3];
            for i in 0..3 {
                let value = rgb[i] + (target[i] * scale - rgb[i]) * weight;
                out[i] = byte(srgb(value.clamp(0.0, 1.0)));
            }
            Rgba([out[0], out[1], out[2], p[3]])
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_preserve_aspect_and_never_upscale_or_accept_zero() {
        let source = RgbaImage::from_pixel(8, 4, Rgba([50, 100, 200, 255]));
        assert_eq!(resize(&source, 4, Kind::Data).unwrap().dimensions(), (4, 2));
        assert_eq!(resize(&source, 16, Kind::Data).unwrap(), source);
        assert!(resize(&source, 0, Kind::Data).is_err());
    }

    #[test]
    fn color_averages_in_linear_light_and_does_not_bleed_transparent_rgb() {
        let source = RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255; 4])
            }
        });
        let color = resize(&source, 1, Kind::Color).unwrap();
        assert!((i32::from(color.get_pixel(0, 0)[0]) - 188).abs() <= 1);
        let source = RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([255, 0, 0, 0])
            } else {
                Rgba([255; 4])
            }
        });
        let color = resize(&source, 1, Kind::Color).unwrap();
        assert_eq!(&color.get_pixel(0, 0).0[..3], &[255, 255, 255]);
        assert!((i32::from(color.get_pixel(0, 0)[3]) - 128).abs() <= 1);
    }

    #[test]
    fn normals_are_renormalized_but_data_channels_are_not_gamma_corrected() {
        let source = RgbaImage::from_pixel(4, 4, Rgba([180, 128, 180, 255]));
        let normal = resize(&source, 1, Kind::Normal).unwrap();
        let p = normal.get_pixel(0, 0);
        let length = p.0[..3]
            .iter()
            .map(|v| (f32::from(*v) / 127.5 - 1.0).powi(2))
            .sum::<f32>()
            .sqrt();
        assert!((length - 1.0).abs() < 0.01);
        assert_eq!(
            resize(&source, 1, Kind::Data).unwrap().get_pixel(0, 0),
            &Rgba([180, 128, 180, 255])
        );
    }

    fn synthetic_eye() -> RgbaImage {
        // Pink surround, pale sclera disc, brown iris, black pupil, white dot.
        RgbaImage::from_fn(64, 64, |x, y| {
            let d = ((x as f32 - 32.0).powi(2) + (y as f32 - 30.0).powi(2)).sqrt();
            if (x, y) == (34, 28) {
                Rgba([250, 250, 250, 255])
            } else if d < 3.0 {
                Rgba([10, 8, 6, 255])
            } else if d < 9.0 {
                Rgba([120, 70, 30, 255])
            } else if d < 20.0 {
                Rgba([235, 225, 220, 255])
            } else {
                Rgba([200, 120, 120, 255])
            }
        })
    }

    #[test]
    fn iris_is_found_around_the_pupil_and_only_it_is_recoloured() {
        let eye = synthetic_eye();
        let iris = locate_iris(&eye).unwrap();
        assert!((iris.x - 32.0).abs() < 0.6 && (iris.y - 30.0).abs() < 0.6);
        assert!((8.0..=10.0).contains(&iris.radius), "{iris:?}");
        let blue = recolor_iris(&eye, [60, 120, 200]).unwrap();
        let iris_pixel = blue.get_pixel(32 + 6, 30);
        assert!(
            iris_pixel[2] > iris_pixel[0],
            "iris turned blue: {iris_pixel:?}"
        );
        // The pupil stays dark; sclera, surround and specular dot are untouched.
        assert!(blue.get_pixel(32, 30).0[..3].iter().all(|v| *v < 40));
        for (x, y) in [(32 + 14, 30), (2, 2), (34, 28)] {
            assert_eq!(blue.get_pixel(x, y), eye.get_pixel(x, y), "({x}, {y})");
        }
        assert!(recolor_iris(&RgbaImage::from_pixel(8, 8, Rgba([200; 4])), [0; 3]).is_err());
    }
}
