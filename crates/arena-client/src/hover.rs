//! Presentation-only picking proxies, independent of targeting and combat rules.
use arena_sim::arena::PILLARS;
use bevy::prelude::*;

fn box_hit(ray: Ray3d, min: Vec3, max: Vec3) -> Option<f32> {
    let mut near = 0.0_f32;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        let direction = ray.direction[axis];
        if direction.abs() < 1e-6 {
            if ray.origin[axis] < min[axis] || ray.origin[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - ray.origin[axis]) / direction;
            let b = (max[axis] - ray.origin[axis]) / direction;
            near = near.max(a.min(b));
            far = far.min(a.max(b));
        }
    }
    (far >= near).then_some(near)
}

pub(super) fn pick(ray: Ray3d, units: impl Iterator<Item = (usize, Vec3)>) -> Option<usize> {
    let blocker = PILLARS
        .iter()
        .filter_map(|pillar| {
            let p = super::world(pillar.center);
            let half_size = pillar.half_size as f32 / 1000.0 + 0.175;
            // Conservative proxy includes the pillar's base and cap.
            box_hit(
                ray,
                p + Vec3::new(-half_size, 0.0, -half_size),
                p + Vec3::new(half_size, 4.1, half_size),
            )
        })
        .fold(f32::INFINITY, f32::min);
    units
        .filter_map(|(id, p)| {
            box_hit(
                ray,
                p + Vec3::new(-0.45, 0.05, -0.45),
                p + Vec3::new(0.45, 1.95, 0.45),
            )
            .filter(|distance| *distance < blocker)
            .map(|distance| (id, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nearest_unit_wins_and_misses_clear() {
        let ray = Ray3d::new(Vec3::new(0.0, 1.0, 8.0), Dir3::NEG_Z);
        assert_eq!(
            pick(
                ray,
                [(1, Vec3::ZERO), (0, Vec3::new(0.0, 0.0, 4.0))].into_iter()
            ),
            Some(0)
        );
        assert_eq!(pick(ray, [(1, Vec3::new(4.0, 0.0, 0.0))].into_iter()), None);
        assert_eq!(
            pick(ray, [(1, Vec3::new(0.0, 0.0, 10.0))].into_iter()),
            None
        );
    }
    #[test]
    fn pillars_occlude_units() {
        let p = super::super::world(PILLARS[0].center);
        let ray = Ray3d::new(p + Vec3::new(0.0, 1.0, 5.0), Dir3::NEG_Z);
        assert_eq!(pick(ray, [(1, p - Vec3::Z * 3.0)].into_iter()), None);
        assert_eq!(pick(ray, [(0, p + Vec3::Z * 3.0)].into_iter()), Some(0));
    }
}
