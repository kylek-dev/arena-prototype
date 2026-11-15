//! Presentation-only picking proxies, independent of targeting and combat rules.
use crate::prism::Prism;
use arena_map::MapFile;
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

/// Conservative picking proxies for a map's hexagonal pillars, grown to
/// cover the base, bands and crown.
pub(super) fn pillar_blockers(map: &MapFile) -> Vec<Prism> {
    let top = super::venue::pillar_top(&map.venue);
    map.layout
        .pillars
        .iter()
        .map(|pillar| {
            Prism::hexagon(
                super::world(pillar.center),
                pillar.half_x as f32 / 1000.0 + 0.4,
                pillar.half_z as f32 / 1000.0 + 0.4,
                0.0,
                top,
            )
        })
        .collect()
}

fn blocker_distance(ray: Ray3d, blockers: &[Prism]) -> f32 {
    blockers
        .iter()
        .filter_map(|blocker| blocker.entry(ray.origin, *ray.direction, f32::INFINITY, 0.0))
        .map(|(distance, _)| distance)
        .fold(f32::INFINITY, f32::min)
}

pub(super) fn pick(
    ray: Ray3d,
    units: impl Iterator<Item = (usize, Vec3)>,
    blockers: &[Prism],
) -> Option<usize> {
    let blocker = blocker_distance(ray, blockers);
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
                [(1, Vec3::ZERO), (0, Vec3::new(0.0, 0.0, 4.0))].into_iter(),
                &[]
            ),
            Some(0)
        );
        assert_eq!(
            pick(ray, [(1, Vec3::new(4.0, 0.0, 0.0))].into_iter(), &[]),
            None
        );
        assert_eq!(
            pick(ray, [(1, Vec3::new(0.0, 0.0, 10.0))].into_iter(), &[]),
            None
        );
    }
    #[test]
    fn pillars_occlude_units() {
        let map = arena_map::colosseum();
        let blockers = pillar_blockers(&map);
        let pillar = map.layout.pillars[0];
        let p = super::super::world(pillar.center);
        let face = pillar.half_z as f32 / 1000.0 + 0.4;
        let ray = Ray3d::new(p + Vec3::new(0.0, 1.0, face + 3.0), Dir3::NEG_Z);
        assert_eq!(
            pick(
                ray,
                [(1, p - Vec3::Z * (face + 1.0))].into_iter(),
                &blockers
            ),
            None
        );
        assert_eq!(
            pick(
                ray,
                [(0, p + Vec3::Z * (face + 1.0))].into_iter(),
                &blockers
            ),
            Some(0)
        );
        // Looking up the shaft never sees past the pillar.
        let up = Ray3d::new(
            p + Vec3::new(0.0, 1.0, face + 3.0),
            Dir3::new(Vec3::new(0.0, 1.0, -0.4)).unwrap(),
        );
        assert!(blocker_distance(up, &blockers).is_finite());
    }
}
