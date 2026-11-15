//! Convex prisms for presentation-only collision and picking: boxes and the
//! hexagonal pillars (the same hexagon the rules use, `arena_sim::map::Pillar`).
//! Each prism is the intersection of up to eight outward-facing planes.
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Prism {
    /// Outward unit normal in `xyz`, plane offset in `w`: inside is `n·p ≤ w`.
    planes: [Vec4; 8],
    count: usize,
}

impl Prism {
    fn from_planes(planes: &[Vec4]) -> Self {
        let mut all = [Vec4::ZERO; 8];
        all[..planes.len()].copy_from_slice(planes);
        Self {
            planes: all,
            count: planes.len(),
        }
    }

    pub(crate) fn cuboid(min: Vec3, max: Vec3) -> Self {
        Self::from_planes(&[
            Vec4::new(1.0, 0.0, 0.0, max.x),
            Vec4::new(-1.0, 0.0, 0.0, -min.x),
            Vec4::new(0.0, 1.0, 0.0, max.y),
            Vec4::new(0.0, -1.0, 0.0, -min.y),
            Vec4::new(0.0, 0.0, 1.0, max.z),
            Vec4::new(0.0, 0.0, -1.0, -min.z),
        ])
    }

    /// The hexagon inscribed in a `half_x` × `half_z` footprint around
    /// `center` (corners on ±X, flat faces on ±Z), from `bottom` to `top`.
    pub(crate) fn hexagon(center: Vec3, half_x: f32, half_z: f32, bottom: f32, top: f32) -> Self {
        let corners = hexagon_corners(center, half_x, half_z);
        let mut planes = [Vec4::ZERO; 8];
        for i in 0..6 {
            let (a, b) = (corners[i], corners[(i + 1) % 6]);
            let edge = b - a;
            // Corners run counter-clockwise from above, so the outward
            // normal is the edge turned clockwise.
            let normal = Vec3::new(edge.z, 0.0, -edge.x).normalize();
            planes[i] = normal.extend(normal.dot(a));
        }
        planes[6] = Vec4::new(0.0, 1.0, 0.0, top);
        planes[7] = Vec4::new(0.0, -1.0, 0.0, -bottom);
        Self::from_planes(&planes)
    }

    fn planes(&self) -> &[Vec4] {
        &self.planes[..self.count]
    }

    /// Inside the prism grown by `padding` on every face.
    pub(crate) fn contains(&self, point: Vec3, padding: f32) -> bool {
        self.planes()
            .iter()
            .all(|plane| plane.truncate().dot(point) <= plane.w + padding)
    }

    /// Where a ray from `origin` along unit `direction` first enters the
    /// prism grown by `padding`, within `limit`, and that face's normal (zero
    /// when the origin is already inside).
    pub(crate) fn entry(
        &self,
        origin: Vec3,
        direction: Vec3,
        limit: f32,
        padding: f32,
    ) -> Option<(f32, Vec3)> {
        let (mut enter, mut exit, mut normal) = (0.0_f32, limit, Vec3::ZERO);
        for plane in self.planes() {
            let n = plane.truncate();
            let distance = n.dot(origin) - plane.w - padding;
            let toward = n.dot(direction);
            if toward.abs() < 1e-6 {
                if distance > 0.0 {
                    return None;
                }
                continue;
            }
            let t = -distance / toward;
            if toward < 0.0 {
                if t >= enter {
                    enter = t;
                    normal = n;
                }
            } else {
                exit = exit.min(t);
            }
            if enter > exit {
                return None;
            }
        }
        Some((enter, normal))
    }
}

/// Corners of the pillar hexagon, counter-clockwise seen from above, at
/// `center`'s height. Matches `arena_sim::map::Pillar::corners`.
pub(crate) fn hexagon_corners(center: Vec3, half_x: f32, half_z: f32) -> [Vec3; 6] {
    let mid = half_x * 0.5;
    [
        Vec3::new(half_x, 0.0, 0.0),
        Vec3::new(mid, 0.0, half_z),
        Vec3::new(-mid, 0.0, half_z),
        Vec3::new(-half_x, 0.0, 0.0),
        Vec3::new(-mid, 0.0, -half_z),
        Vec3::new(mid, 0.0, -half_z),
    ]
    .map(|corner| center + corner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hexagon_excludes_the_square_corners_and_matches_the_rules() {
        let hex = Prism::hexagon(Vec3::ZERO, 4.5, 4.5, 0.0, 10.0);
        assert!(hex.contains(Vec3::new(0.0, 5.0, 0.0), 0.0));
        assert!(hex.contains(Vec3::new(4.4, 5.0, 0.0), 0.0));
        assert!(hex.contains(Vec3::new(2.0, 5.0, 4.4), 0.0));
        // Where a square pillar's corner was is now open.
        assert!(!hex.contains(Vec3::new(4.0, 5.0, 4.0), 0.0));
        assert!(!hex.contains(Vec3::new(0.0, 10.5, 0.0), 0.0));
        let rules = arena_sim::map::Pillar {
            center: arena_sim::arena::Position { x: 0, z: 0 },
            half_x: 4_500,
            half_z: 4_500,
        };
        for (corner, rule) in hexagon_corners(Vec3::ZERO, 4.5, 4.5)
            .iter()
            .zip(rules.corners())
        {
            assert!((corner.x - rule.x as f32 / 1000.0).abs() < 1e-3);
            assert!((corner.z - rule.z as f32 / 1000.0).abs() < 1e-3);
        }
    }

    #[test]
    fn rays_enter_through_the_facing_side() {
        let hex = Prism::hexagon(Vec3::ZERO, 4.5, 4.5, 0.0, 10.0);
        let (t, normal) = hex
            .entry(Vec3::new(0.0, 5.0, 10.0), Vec3::NEG_Z, 20.0, 0.0)
            .unwrap();
        assert!((t - 5.5).abs() < 1e-4);
        assert!(normal.abs_diff_eq(Vec3::Z, 1e-5));
        // Passing through the old square corner misses.
        let diagonal = Vec3::new(-1.0, 0.0, -1.0).normalize();
        assert!(
            hex.entry(Vec3::new(6.0, 5.0, 6.0), diagonal, 2.5, 0.0)
                .is_none()
        );
        let inside = hex
            .entry(Vec3::new(0.0, 5.0, 0.0), Vec3::X, 20.0, 0.0)
            .unwrap();
        assert_eq!(inside, (0.0, Vec3::ZERO));
        let cube = Prism::cuboid(Vec3::splat(-1.0), Vec3::splat(1.0));
        let (t, _) = cube
            .entry(Vec3::new(-5.0, 0.0, 0.0), Vec3::X, 10.0, 0.5)
            .unwrap();
        assert!((t - 3.5).abs() < 1e-4);
    }
}
