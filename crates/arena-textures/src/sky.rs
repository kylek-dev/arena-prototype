//! Original hand-painted-style sky, rendered offline into a vertically stacked
//! cubemap PNG (+X, −X, +Y, −Y, +Z, −Z faces, top to bottom). Painting at high
//! resolution here keeps startup free of procedural work in the browser.
//!
//! The look follows the art direction: a warm horizon under a saturated blue
//! zenith, a soft sun glow, and large banded clouds with painted light — flat
//! lit bodies, cool undersides and bright sun-side crowns — rather than
//! photographic detail.

use arena_palette::{self as palette, Swatch};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct V3(pub f32, pub f32, pub f32);

impl V3 {
    /// A palette swatch in sRGB; every painted anchor below is one.
    const fn of(swatch: Swatch) -> Self {
        let [r, g, b] = swatch.srgb();
        V3(r, g, b)
    }
    fn dot(self, o: Self) -> f32 {
        self.0 * o.0 + self.1 * o.1 + self.2 * o.2
    }
    fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn normalize(self) -> Self {
        let l = self.len();
        V3(self.0 / l, self.1 / l, self.2 / l)
    }
    fn lerp(self, o: Self, t: f32) -> Self {
        V3(
            self.0 + (o.0 - self.0) * t,
            self.1 + (o.1 - self.1) * t,
            self.2 + (o.2 - self.2) * t,
        )
    }
    fn scale(self, s: f32) -> Self {
        V3(self.0 * s, self.1 * s, self.2 * s)
    }
}

/// Direction through a texel of cube face `face` (+X, −X, +Y, −Y, +Z, −Z), for
/// `u`, `v` in −1..1 with `v` increasing down the image (wgpu convention).
pub fn direction(face: usize, u: f32, v: f32) -> V3 {
    match face {
        0 => V3(1.0, -v, -u),
        1 => V3(-1.0, -v, u),
        2 => V3(u, 1.0, v),
        3 => V3(u, -1.0, -v),
        4 => V3(u, -v, 1.0),
        _ => V3(-u, -v, -1.0),
    }
    .normalize()
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ seed.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 16;
    (h & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (xi, yi) = (ix as i32, iy as i32);
    let a = hash(xi, yi, seed);
    let b = hash(xi + 1, yi, seed);
    let c = hash(xi, yi + 1, seed);
    let d = hash(xi + 1, yi + 1, seed);
    a + (b - a) * sx + (c - a) * sy + (a - b - c + d) * sx * sy
}

/// Fractal noise in 0..1 with a rotation between octaves to hide the grid.
fn fbm(mut x: f32, mut y: f32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amplitude, mut total) = (0.0, 0.5, 0.0);
    for octave in 0..octaves {
        sum += value_noise(x, y, seed + octave) * amplitude;
        total += amplitude;
        (x, y) = (x * 1.6 - y * 1.2 + 17.0, x * 1.2 + y * 1.6 + 5.0);
        amplitude *= 0.5;
    }
    sum / total
}

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Direction toward the sun: the default key light (pitch 51.5°, yaw −31.5°)
/// reversed, lowered so the glow sits in the band visible over the venue.
pub fn sun() -> V3 {
    let (pitch, yaw) = (51.5_f32.to_radians(), (-31.5_f32).to_radians());
    // Bevy Quat::from_euler(XYZ, −pitch, yaw, 0) applied to +Z.
    let toward = V3(yaw.sin(), pitch.sin() * yaw.cos(), pitch.cos() * yaw.cos());
    V3(toward.0, toward.1.clamp(0.3, 0.55), toward.2).normalize()
}

/// Cloudless sky: gradient, brushed wash and sun glow.
fn clear(dir: V3, sun: V3) -> V3 {
    let zenith = V3::of(palette::SKY_3);
    let upper = V3::of(palette::SKY_5);
    let horizon = V3::of(palette::SAND_7);
    let below = V3::of(palette::STONE_5);
    let up = dir.1;
    let mut color = if up >= 0.0 {
        horizon
            .lerp(upper, smoothstep(0.0, 0.3, up))
            .lerp(zenith, smoothstep(0.3, 0.98, up))
    } else {
        horizon.lerp(below, smoothstep(0.0, 0.15, -up))
    };
    // Broad, brushy variation in the clear sky so it never reads as a flat
    // digital gradient.
    let wash = fbm(dir.0 * 2.0 + 3.0, dir.2 * 2.0 + dir.1, 3, 91) - 0.5;
    color = color.lerp(V3::of(palette::ARCANE_4), (wash * 0.18).max(0.0));
    color = color.lerp(V3::of(palette::SKY_5), (-wash * 0.15).max(0.0));
    let facing = dir.dot(sun).max(0.0);
    color = color.lerp(
        V3::of(palette::SAND_8),
        facing.powf(48.0) * 0.75 + facing.powf(6.0) * 0.16,
    );
    color
}

/// The painted sky colour (sRGB 0..1) along unit `dir`.
pub fn paint(dir: V3, sun: V3) -> V3 {
    let up = dir.1;
    let facing = dir.dot(sun).max(0.0);
    let mut color = clear(dir, sun);
    if up > 0.0 {
        // A flat cloud deck so clouds shrink and crowd toward the horizon.
        let k = 1.5 / (up + 0.1);
        let (x, y) = (dir.0 * k, dir.2 * k);
        // Domain warp gives the swirling, brushed outlines of painted clouds.
        let wx = fbm(x * 0.5 + 11.0, y * 0.5, 2, 7) - 0.5;
        let wy = fbm(x * 0.5, y * 0.5 + 23.0, 2, 13) - 0.5;
        let (x, y) = (x + wx * 0.55, y + wy * 0.55);
        let shape = fbm(x, y, 6, 1);
        let fade = smoothstep(0.03, 0.25, up);
        let cover = smoothstep(0.50, 0.555, shape) * fade;
        // Light comes from the sun side: compare with the shape displaced away
        // from the sun to find lit and shadowed rims, then band them.
        let (sx, sy) = (sun.0 * 0.22, sun.2 * 0.22);
        let lit = shape - fbm(x - sx, y - sy, 6, 1);
        let shadow_band = smoothstep(-0.005, -0.03, lit);
        let rim_band = smoothstep(0.012, 0.035, lit);
        let body = V3::of(palette::NIGHT_12);
        let shadow = V3::of(palette::ARCANE_6);
        let rim = V3::of(palette::SAND_8);
        let mut cloud = body.lerp(shadow, shadow_band * 0.85);
        cloud = cloud.lerp(rim, rim_band);
        // Interior depth: thicker cores are slightly cooler, like a painter's
        // second value under the lit crown.
        let core = smoothstep(0.6, 0.75, shape);
        cloud = cloud.lerp(V3::of(palette::SKY_7), core * (1.0 - rim_band) * 0.4);
        // Warm the clouds near the sun.
        cloud = cloud.lerp(V3::of(palette::SAND_8), facing.powf(8.0) * 0.4);
        color = color.lerp(cloud, cover * 0.95);
        // A few high, thin brushed streaks above the deck.
        let streak = fbm(x * 0.35 + 40.0, y * 2.2, 4, 31);
        let wisps = smoothstep(0.66, 0.76, streak) * fade * (1.0 - cover) * 0.22;
        color = color.lerp(V3::of(palette::SKY_8), wisps);
    }
    V3(
        color.0.clamp(0.0, 1.0),
        color.1.clamp(0.0, 1.0),
        color.2.clamp(0.0, 1.0),
    )
}

/// RGB8 pixels for six `size`² faces stacked vertically.
pub fn render(size: u32) -> Vec<u8> {
    let sun = sun();
    let mut data = Vec::with_capacity((size * size * 6 * 3) as usize);
    for face in 0..6 {
        for y in 0..size {
            for x in 0..size {
                let u = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
                let v = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
                let c = paint(direction(face, u, v), sun).scale(255.0);
                data.extend([c.0, c.1, c.2].map(|c| c.round() as u8));
            }
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_point_along_axes_and_upper_rows_look_up() {
        let axes = [
            V3(1.0, 0.0, 0.0),
            V3(-1.0, 0.0, 0.0),
            V3(0.0, 1.0, 0.0),
            V3(0.0, -1.0, 0.0),
            V3(0.0, 0.0, 1.0),
            V3(0.0, 0.0, -1.0),
        ];
        for (face, axis) in axes.into_iter().enumerate() {
            assert!(direction(face, 0.0, 0.0).dot(axis) > 0.9999);
            if face != 2 && face != 3 {
                assert!(direction(face, 0.0, -0.9).1 > 0.0);
            }
        }
    }

    #[test]
    fn sky_is_blue_overhead_warm_at_the_horizon_and_brightest_toward_the_sun() {
        let sun = sun();
        // Average many directions so individual clouds do not decide the test.
        let mean = |up: f32| {
            let mut sum = V3(0.0, 0.0, 0.0);
            for i in 0..64 {
                let a = i as f32 / 64.0 * std::f32::consts::TAU;
                let r = (1.0 - up * up).sqrt();
                let c = paint(V3(a.cos() * r, up, a.sin() * r), sun);
                sum = V3(sum.0 + c.0, sum.1 + c.1, sum.2 + c.2);
            }
            sum.scale(1.0 / 64.0)
        };
        let high = mean(0.95);
        let low = mean(0.02);
        assert!(high.2 > high.0 + 0.2, "{high:?}");
        assert!(low.0 > low.2, "{low:?}");
        let away = clear(V3(-sun.0, sun.1, -sun.2).normalize(), sun);
        let toward = clear(sun, sun);
        assert!(toward.0 + toward.1 > away.0 + away.1);
    }

    #[test]
    fn render_is_deterministic_and_sized() {
        let a = render(16);
        assert_eq!(a.len(), 16 * 16 * 6 * 3);
        assert_eq!(a, render(16));
    }
}
