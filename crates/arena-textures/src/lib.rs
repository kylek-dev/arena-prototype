//! Turn downloaded hand-painted tiling textures into small, near-neutral
//! value maps. The game tints them in code, so one source can match the arena
//! palette without editing images by hand. Offline only; never part of combat.

pub mod character;
pub mod sky;

/// How one source texture becomes a runtime texture.
#[derive(Clone, Copy, Debug)]
pub struct Recipe {
    /// Fraction of the original chroma kept around each pixel's luma (0 = grey).
    pub chroma: f32,
    /// Darken pixels by ambient occlusion: 0 ignores it, 1 multiplies fully.
    pub occlusion: f32,
    /// Lift saturated areas (bricks) above neutral ones (mortar) by this much,
    /// so desaturation keeps them apart.
    pub saturated_lift: f32,
    /// Scale each pixel's distance from the mean (below 1 calms busy ground).
    pub contrast: f32,
    /// Mean output luma in 0..1 sRGB space; leaves headroom for painted highlights.
    pub mean: f32,
}

/// RGB8 image data, row-major.
#[derive(Clone, Debug, PartialEq)]
pub struct Rgb {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 3]>,
}

fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// Neutralize, apply occlusion, normalize and halve the resolution with a 2×2
/// box filter. Halving keeps tiling seamless; general resampling would not.
pub fn prepare(color: &Rgb, occlusion: Option<&[u8]>, recipe: Recipe) -> Rgb {
    assert_eq!(color.pixels.len(), (color.width * color.height) as usize);
    assert!(color.width.is_multiple_of(2) && color.height.is_multiple_of(2));
    if let Some(ao) = occlusion {
        assert_eq!(ao.len(), color.pixels.len());
    }
    let mut values: Vec<[f32; 3]> = color
        .pixels
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let c = p.map(|v| f32::from(v) / 255.0);
            let l = luma(c);
            let saturation =
                c.iter().copied().fold(0.0, f32::max) - c.iter().copied().fold(1.0, f32::min);
            let lift = 1.0 - recipe.saturated_lift + recipe.saturated_lift * 2.0 * saturation;
            let ao = occlusion.map_or(1.0, |ao| {
                1.0 - recipe.occlusion + recipe.occlusion * f32::from(ao[i]) / 255.0
            });
            c.map(|v| (l + recipe.chroma * (v - l)) * lift * ao)
        })
        .collect();
    let mean = values.iter().map(|c| luma(*c)).sum::<f32>() / values.len() as f32;
    let scale = if mean > 0.0 { recipe.mean / mean } else { 1.0 };
    for c in &mut values {
        *c = c.map(|v| recipe.mean + recipe.contrast * (v * scale - recipe.mean));
    }
    let (width, height) = (color.width / 2, color.height / 2);
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let c = values[((y * 2 + dy) * color.width + x * 2 + dx) as usize];
                for (s, v) in sum.iter_mut().zip(c) {
                    *s += v / 4.0;
                }
            }
            pixels.push(sum.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    Rgb {
        width,
        height,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEUTRAL: Recipe = Recipe {
        chroma: 0.0,
        occlusion: 0.0,
        saturated_lift: 0.0,
        contrast: 1.0,
        mean: 0.5,
    };

    fn image(width: u32, height: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Rgb {
        Rgb {
            width,
            height,
            pixels: (0..height)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .map(|(x, y)| f(x, y))
                .collect(),
        }
    }

    fn mean_luma(image: &Rgb) -> f32 {
        image
            .pixels
            .iter()
            .map(|p| luma(p.map(|v| f32::from(v) / 255.0)))
            .sum::<f32>()
            / image.pixels.len() as f32
    }

    #[test]
    fn halves_resolution_neutralizes_and_normalizes_the_mean() {
        let source = image(8, 4, |x, _| {
            if x % 2 == 0 {
                [200, 80, 60]
            } else {
                [90, 140, 200]
            }
        });
        let out = prepare(&source, None, NEUTRAL);
        assert_eq!((out.width, out.height, out.pixels.len()), (4, 2, 8));
        for p in &out.pixels {
            assert!(p[0] == p[1] && p[1] == p[2], "{p:?} is not neutral");
        }
        assert!((mean_luma(&out) - 0.5).abs() < 0.01);
    }

    #[test]
    fn kept_chroma_occlusion_and_saturated_lift_change_the_expected_pixels() {
        let red = image(2, 2, |_, _| [200, 60, 60]);
        let tinted = prepare(
            &red,
            None,
            Recipe {
                chroma: 0.5,
                ..NEUTRAL
            },
        );
        assert!(tinted.pixels[0][0] > tinted.pixels[0][1]);
        // A dark occlusion quadrant darkens only its own output pixel.
        let flat = image(4, 2, |_, _| [128, 128, 128]);
        let ao: Vec<u8> = (0..8).map(|i| if i % 4 < 2 { 0 } else { 255 }).collect();
        let shaded = prepare(
            &flat,
            Some(&ao),
            Recipe {
                occlusion: 1.0,
                ..NEUTRAL
            },
        );
        assert!(shaded.pixels[0][0] < shaded.pixels[1][0]);
        // Saturated bricks stay lighter than equally bright grey mortar.
        let bricks = image(4, 2, |x, _| {
            if x < 2 {
                [190, 60, 70]
            } else {
                [104, 104, 104]
            }
        });
        let lifted = prepare(
            &bricks,
            None,
            Recipe {
                saturated_lift: 0.6,
                ..NEUTRAL
            },
        );
        assert!(lifted.pixels[0][0] > lifted.pixels[1][0] + 40);
        // Lower contrast pulls both sides toward the same mean.
        let calm = prepare(
            &bricks,
            None,
            Recipe {
                saturated_lift: 0.6,
                contrast: 0.5,
                ..NEUTRAL
            },
        );
        let spread = |i: &Rgb| i32::from(i.pixels[0][0]) - i32::from(i.pixels[1][0]);
        assert!(spread(&calm) < spread(&lifted) * 2 / 3);
        assert!((mean_luma(&calm) - 0.5).abs() < 0.01);
    }

    #[test]
    fn tiling_is_preserved_because_each_output_pixel_uses_its_own_block() {
        // A one-pixel-wide seam column must not bleed into neighbouring outputs.
        let source = image(
            8,
            2,
            |x, _| if x == 0 { [0, 0, 0] } else { [180, 180, 180] },
        );
        let out = prepare(
            &source,
            None,
            Recipe {
                mean: 0.6,
                ..NEUTRAL
            },
        );
        let last = out.pixels[3][0];
        assert!(out.pixels[0][0] < last);
        assert_eq!(out.pixels[1][0], last);
        assert_eq!(out.pixels[2][0], last);
    }
}
