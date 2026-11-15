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
}
