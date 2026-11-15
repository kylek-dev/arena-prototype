//! Original painted granite for the arena pillars: a seamless greyscale tile
//! of interlocking crystal grains (mid feldspar, light quartz, dark mica)
//! over broad, soft tonal drifts. Grains are a centimetre or two across, so
//! the surface reads as detailed stone up close and averages to a calm value
//! at camera distance instead of the walls' brick pattern. The game tints it
//! with a palette swatch.
//!
//! Every layer is built on a grid that wraps at the tile edge, so the tile
//! repeats without seams.

use std::f32::consts::TAU;

/// Linear average shared with the prepared floor and brick textures, so one
/// palette tint looks the same on every stone surface.
pub const LINEAR_MEAN: f32 = 0.492;
/// Grain cells per tile edge (≈1.7 cm grains on the game's 2 m repeat).
const GRAINS: u32 = 120;
/// Fine speckle cells per tile edge (≈0.6 cm).
const SPECKS: u32 = 320;
/// Mineral clustering cells per tile edge (≈10 cm drifts).
const CLUSTERS: u32 = 20;
/// Broad tonal drift cells per tile edge.
const DRIFT: u32 = 5;
/// Supersamples per pixel edge, so grain edges stay smooth when magnified.
const SAMPLES: u32 = 3;

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Stable hash of a wrapped grid cell to 0..1.
fn hash(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x
        .wrapping_mul(374_761_393)
        .wrapping_add(y.wrapping_mul(668_265_263))
        .wrapping_add(seed.wrapping_mul(2_246_822_519));
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0xffff) as f32 / 65_535.0
}

/// Smooth value noise on an `n`-cell grid that wraps, in 0..1.
fn value_noise(u: f32, v: f32, n: u32, seed: u32) -> f32 {
    let (x, y) = (u * n as f32, v * n as f32);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let cell = |dx: u32, dy: u32| {
        let cx = (x0 as i64 + i64::from(dx)).rem_euclid(i64::from(n)) as u32;
        let cy = (y0 as i64 + i64::from(dy)).rem_euclid(i64::from(n)) as u32;
        hash(cx, cy, seed)
    };
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let top = cell(0, 0) + (cell(1, 0) - cell(0, 0)) * sx;
    let bottom = cell(0, 1) + (cell(1, 1) - cell(0, 1)) * sx;
    top + (bottom - top) * sy
}

/// Nearest and second-nearest jittered feature points on an `n`-cell grid
/// that wraps: (distance to nearest, distance to second, nearest cell).
fn cells(u: f32, v: f32, n: u32, seed: u32) -> (f32, f32, (u32, u32)) {
    let (x, y) = (u * n as f32, v * n as f32);
    let (x0, y0) = (x.floor() as i64, y.floor() as i64);
    let (mut d1, mut d2, mut nearest) = (f32::MAX, f32::MAX, (0, 0));
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (gx, gy) = (x0 + dx, y0 + dy);
            let cx = gx.rem_euclid(i64::from(n)) as u32;
            let cy = gy.rem_euclid(i64::from(n)) as u32;
            let px = gx as f32 + 0.15 + 0.7 * hash(cx, cy, seed);
            let py = gy as f32 + 0.15 + 0.7 * hash(cx, cy, seed + 1);
            let d = (px - x).hypot(py - y);
            if d < d1 {
                (d2, d1, nearest) = (d1, d, (cx, cy));
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    (d1, d2, nearest)
}

/// Mineral value for a grain: mostly mid feldspar, some light quartz, a
/// scattering of dark mica.
/// Contrast is kept soft so the surface reads as detail, not noise.
fn mineral(pick: f32, shade: f32) -> f32 {
    if pick < 0.15 {
        0.47 + 0.06 * shade
    } else if pick < 0.40 {
        0.82 + 0.05 * shade
    } else {
        0.67 + 0.07 * shade
    }
}

/// Painted value (sRGB 0..1, before normalizing) at tile coordinates `u`, `v`
/// in 0..1.
pub fn paint(u: f32, v: f32) -> f32 {
    // Interlocking grains with soft, slightly darker boundaries.
    let (d1, d2, (cx, cy)) = cells(u, v, GRAINS, 11);
    // Minerals cluster in drifts rather than scattering evenly.
    let cluster = 0.35 * (value_noise(u, v, CLUSTERS, 41) - 0.5);
    let mut value = mineral(hash(cx, cy, 13) + cluster, hash(cx, cy, 17));
    value *= 0.94 + 0.06 * smoothstep(0.0, 0.12, d2 - d1);
    // Fine dark and light specks inside the grains.
    let (s1, _, (sx, sy)) = cells(u, v, SPECKS, 23);
    let speck = 1.0 - smoothstep(0.18, 0.32, s1);
    let kind = hash(sx, sy, 29);
    if kind < 0.10 {
        value = value * (1.0 - speck) + 0.40 * speck;
    } else if kind > 0.95 {
        value = value * (1.0 - speck) + 0.92 * speck;
    }
    // Broad, painterly tonal drift so large faces never look flat or tiled.
    let drift = value_noise(u, v, DRIFT, 31) - 0.5 + 0.5 * (value_noise(u, v, DRIFT * 3, 37) - 0.5);
    value += 0.12 * drift;
    // A faint brushed sheen across the stone.
    value += 0.015 * f32::sin(TAU * (3.0 * u + 2.0 * v) + 6.0 * drift);
    value.clamp(0.0, 1.0)
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Greyscale RGB8 pixels for a `size`² tile whose linear average is
/// `LINEAR_MEAN`. Each pixel averages a grid of samples.
pub fn render(size: u32) -> Vec<u8> {
    let sample = |x: u32, y: u32| {
        let mut sum = 0.0;
        for sy in 0..SAMPLES {
            for sx in 0..SAMPLES {
                let dx = (sx as f32 + 0.5) / SAMPLES as f32;
                let dy = (sy as f32 + 0.5) / SAMPLES as f32;
                sum += to_linear(paint(
                    (x as f32 + dx) / size as f32,
                    (y as f32 + dy) / size as f32,
                ));
            }
        }
        sum / (SAMPLES * SAMPLES) as f32
    };
    let linear: Vec<f32> = (0..size)
        .flat_map(|y| (0..size).map(move |x| (x, y)))
        .map(|(x, y)| sample(x, y))
        .collect();
    let mean = linear.iter().sum::<f32>() / linear.len() as f32;
    let scale = LINEAR_MEAN / mean;
    linear
        .iter()
        .flat_map(|&l| {
            let c = (to_srgb((l * scale).clamp(0.0, 1.0)) * 255.0).round() as u8;
            [c, c, c]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_mean(data: &[u8]) -> f32 {
        let n = data.len() / 3;
        data.chunks(3)
            .map(|p| to_linear(f32::from(p[0]) / 255.0))
            .sum::<f32>()
            / n as f32
    }

    #[test]
    fn tile_is_grey_deterministic_and_matches_the_stone_average() {
        let a = render(64);
        assert_eq!(a, render(64));
        assert_eq!(a.len(), 64 * 64 * 3);
        assert!(a.chunks(3).all(|p| p[0] == p[1] && p[1] == p[2]));
        assert!((linear_mean(&a) - LINEAR_MEAN).abs() < 0.01);
    }

    #[test]
    fn tile_wraps_without_a_seam() {
        // Every layer's grid wraps, so just inside opposite edges matches
        // just past them.
        for i in 0..64 {
            let t = (i as f32 + 0.5) / 64.0;
            let e = 1e-4;
            assert!(
                (paint(e, t) - paint(1.0 + e, t)).abs() < 1e-3,
                "u seam at {t}"
            );
            assert!(
                (paint(t, e) - paint(t, 1.0 + e)).abs() < 1e-3,
                "v seam at {t}"
            );
        }
    }

    #[test]
    fn grains_mix_light_mid_and_dark_minerals() {
        let values: Vec<f32> = (0..4096)
            .map(|i| {
                paint(
                    (i % 64) as f32 / 64.0 + 0.003,
                    (i / 64) as f32 / 64.0 + 0.003,
                )
            })
            .collect();
        let share =
            |f: &dyn Fn(f32) -> bool| values.iter().filter(|v| f(**v)).count() as f32 / 4096.0;
        assert!(share(&|v| v < 0.55) > 0.08, "dark mica");
        assert!(share(&|v| v > 0.78) > 0.12, "light quartz");
        assert!(share(&|v| (0.6..0.76).contains(&v)) > 0.3, "mid feldspar");
        // Soft contrast: nothing near black or white.
        assert!(values.iter().all(|v| (0.25..=1.0).contains(v)));
    }
}
