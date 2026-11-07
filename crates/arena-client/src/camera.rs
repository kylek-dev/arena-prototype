//! Presentation-only camera contact collision. Map boxes supply their own bounds,
//! so decorative pillar caps, bases and arena rims cannot be missed by the camera.
use bevy::prelude::*;

// Covers the near plane as well as the eye; a point ray would still clip corners.
const CLEARANCE: f32 = 0.3;
const CONTACT_GAP: f32 = 0.01;
const RECOVERY_RATE: f32 = 5.0;

#[derive(Component, Clone, Copy)]
pub(super) struct Obstacle {
    min: Vec3,
    max: Vec3,
    floor: bool,
}

/// The static map uses unrotated unit cubes scaled to their visible dimensions.
pub(super) fn solid_box(transform: Transform) -> (Transform, Obstacle) {
    let half = transform.scale.abs() * 0.5;
    let obstacle = Obstacle {
        min: transform.translation - half,
        max: transform.translation + half,
        floor: false,
    };
    (transform, obstacle)
}

pub(super) fn solid_floor(transform: Transform) -> (Transform, Obstacle) {
    let (transform, mut obstacle) = solid_box(transform);
    obstacle.floor = true;
    (transform, obstacle)
}

struct Hit {
    distance: f32,
    normal: Vec3,
}

impl Obstacle {
    fn contains(self, point: Vec3) -> bool {
        point.cmpge(self.min - Vec3::splat(CLEARANCE)).all()
            && point.cmple(self.max + Vec3::splat(CLEARANCE)).all()
    }

    fn entry(self, origin: Vec3, direction: Vec3, limit: f32) -> Option<Hit> {
        // Sweeping a padded box is conservative at corners, keeping the camera's
        // near plane outside geometry, not merely its center.
        let min = self.min - Vec3::splat(CLEARANCE);
        let max = self.max + Vec3::splat(CLEARANCE);
        let mut enter: f32 = 0.0;
        let mut exit = limit;
        let mut normal = Vec3::ZERO;
        for axis in 0..3 {
            if direction[axis].abs() < 1e-6 {
                if origin[axis] < min[axis] || origin[axis] > max[axis] {
                    return None;
                }
            } else {
                let a = (min[axis] - origin[axis]) / direction[axis];
                let b = (max[axis] - origin[axis]) / direction[axis];
                let entry = a.min(b);
                if entry >= enter {
                    enter = entry;
                    normal = Vec3::ZERO;
                    normal[axis] = -direction[axis].signum();
                }
                exit = exit.min(a.max(b));
                if enter > exit {
                    return None;
                }
            }
        }
        Some(Hit {
            distance: (enter - CONTACT_GAP).max(0.0),
            normal,
        })
    }
}

pub(super) fn resolve_distance(
    focus: Vec3,
    direction: Vec3,
    requested: f32,
    previous: f32,
    delta_secs: f32,
    obstacles: impl IntoIterator<Item = Obstacle>,
) -> f32 {
    let obstacles: Vec<_> = obstacles.into_iter().collect();
    let mut safe = requested;
    // Only contact at the requested eye causes pillar retraction. A pillar
    // elsewhere on the boom may obscure the player; that is not camera contact.
    // Repeat because retracting out of one overlapping box can enter another.
    for _ in 0..=obstacles.len() {
        let before = safe;
        for obstacle in &obstacles {
            if (obstacle.floor || obstacle.contains(focus + direction * safe))
                && let Some(hit) = obstacle.entry(focus, direction, safe)
            {
                safe = safe.min(hit.distance);
            }
        }
        if safe == before {
            break;
        }
    }
    // Retraction must be immediate; smoothing inward can leave the eye inside
    // geometry. Only recovery is eased, and is capped by this frame's sweep.
    if safe <= previous {
        safe
    } else {
        previous + (safe - previous) * (1.0 - (-RECOVERY_RATE * delta_secs).exp())
    }
}

/// Sweep the actual eye's motion, not the player-to-camera sight line. Clip the
/// remaining displacement against each contact normal to slide along faces.
fn slide(mut eye: Vec3, target: Vec3, obstacles: &[Obstacle]) -> Vec3 {
    let mut remaining = target - eye;
    for _ in 0..3 {
        let distance = remaining.length();
        if distance < 1e-6 {
            break;
        }
        let direction = remaining / distance;
        let hit = obstacles
            .iter()
            .filter_map(|obstacle| obstacle.entry(eye, direction, distance))
            .min_by(|a, b| a.distance.total_cmp(&b.distance));
        let Some(hit) = hit else {
            eye += remaining;
            break;
        };
        eye += direction * hit.distance;
        remaining -= direction * hit.distance;
        if hit.normal == Vec3::ZERO {
            break;
        }
        remaining -= hit.normal * remaining.dot(hit.normal).min(0.0);
    }
    eye
}

pub(super) fn resolve_eye(
    focus: Vec3,
    direction: Vec3,
    requested: f32,
    previous: Option<Vec3>,
    delta_secs: f32,
    obstacles: &[Obstacle],
) -> Vec3 {
    let distance = resolve_distance(
        focus,
        direction,
        requested,
        previous.map_or(requested, |eye| eye.distance(focus)),
        delta_secs,
        obstacles.iter().copied(),
    );
    let target = focus + direction * distance;
    previous.map_or(target, |eye| slide(eye, target, obstacles))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_at(center: Vec3, size: Vec3) -> Obstacle {
        solid_box(Transform::from_translation(center).with_scale(size)).1
    }

    fn resolve(focus: Vec3, direction: Vec3, obstacles: &[Obstacle]) -> f32 {
        resolve_distance(
            focus,
            direction,
            13.0,
            13.0,
            1.0 / 60.0,
            obstacles.iter().copied(),
        )
    }

    #[test]
    fn distant_pillars_may_occlude_without_retracting_camera() {
        let near = box_at(Vec3::new(0.0, 2.0, 5.0), Vec3::new(3.0, 4.0, 3.0));
        let far = box_at(Vec3::new(0.0, 2.0, 10.0), Vec3::new(3.0, 4.0, 3.0));
        for obstacles in [[near, far], [far, near]] {
            assert_eq!(resolve(Vec3::Y, Vec3::Z, &obstacles), 13.0);
            let behind = Vec3::Y + Vec3::Z * 13.0;
            assert_eq!(
                resolve_eye(Vec3::Y, Vec3::Z, 13.0, Some(behind), 0.016, &obstacles),
                behind
            );
        }
    }

    #[test]
    fn floor_retracts_below_normal_zoom_minimum() {
        let floor = solid_floor(
            Transform::from_xyz(0.0, -0.08, 0.0).with_scale(Vec3::new(36.0, 0.16, 28.0)),
        )
        .1;
        let direction = Vec3::new(0.0, -0.7, 0.7).normalize();
        let distance = resolve(Vec3::Y, direction, &[floor]);
        let eye = Vec3::Y + direction * distance;
        assert!(distance < 6.0);
        assert!(eye.y > CLEARANCE && eye.y < CLEARANCE + 0.02);
    }

    #[test]
    fn grazing_pillar_keeps_clearance_and_orbit_slides_around_corner() {
        let pillar = box_at(Vec3::new(0.0, 2.0, 13.0), Vec3::new(3.0, 4.0, 3.0));
        assert!(resolve(Vec3::new(1.7, 1.0, 0.0), Vec3::Z, &[pillar]) < 12.0);
        assert_eq!(resolve(Vec3::new(1.81, 1.0, 0.0), Vec3::Z, &[pillar]), 13.0);
        let along_face = resolve(Vec3::new(0.8, 1.0, 0.0), Vec3::Z, &[pillar]);
        assert!((along_face - 11.19).abs() < 1e-5);
        let above = resolve(Vec3::Y * 5.0, Vec3::Z, &[pillar]);
        assert_eq!(above, 13.0);
    }

    #[test]
    fn recovery_is_smooth_frame_rate_independent_and_respects_new_blockers() {
        let step = |previous, dt| resolve_distance(Vec3::Y, Vec3::Z, 13.0, previous, dt, []);
        let one = step(2.0, 1.0 / 30.0);
        let two = step(step(2.0, 1.0 / 60.0), 1.0 / 60.0);
        assert!(one > 2.0 && one < 13.0);
        assert!((one - two).abs() < 1e-5);
        let mut recovered = 2.0;
        for _ in 0..180 {
            recovered = step(recovered, 1.0 / 60.0);
        }
        assert!((recovered - 13.0).abs() < 0.001);
        let blocker = box_at(Vec3::new(0.0, 1.0, 13.0), Vec3::splat(1.0));
        assert!(resolve_distance(Vec3::Y, Vec3::Z, 13.0, recovered, 0.016, [blocker]) < 12.2);
        assert_eq!(
            resolve_distance(Vec3::Y, Vec3::Z, 6.0, 13.0, 0.016, []),
            6.0
        );
    }

    #[test]
    fn parallel_rays_and_focus_inside_bounds_are_safe() {
        let obstacle = box_at(Vec3::ZERO, Vec3::splat(2.0));
        assert_eq!(
            obstacle.entry(Vec3::ZERO, Vec3::Z, 13.0).unwrap().distance,
            0.0
        );
        assert_eq!(
            resolve(Vec3::new(3.0, 0.0, 0.0), Vec3::Z, &[obstacle]),
            13.0
        );
        assert_eq!(
            resolve(Vec3::new(0.0, 0.0, 3.0), Vec3::Z, &[obstacle]),
            13.0
        );
    }

    #[test]
    fn eye_motion_cannot_tunnel_through_pillar_from_either_side() {
        let pillar = box_at(Vec3::new(0.0, 2.0, 5.0), Vec3::new(3.0, 4.0, 3.0));
        let front = Vec3::new(0.0, 1.0, 0.0);
        let back = Vec3::new(0.0, 1.0, 13.0);
        let stopped_front = slide(front, back, &[pillar]);
        let stopped_back = slide(back, front, &[pillar]);
        assert!((stopped_front.z - 3.19).abs() < 1e-5);
        assert!((stopped_back.z - 6.81).abs() < 1e-5);
        assert!(!pillar.contains(stopped_front));
        assert!(!pillar.contains(stopped_back));
    }

    #[test]
    fn actual_eye_slides_along_face_and_can_move_around_to_back() {
        let pillar = box_at(Vec3::new(0.0, 2.0, 5.0), Vec3::new(3.0, 4.0, 3.0));
        let start = Vec3::new(0.0, 1.0, 2.0);
        let contact = slide(start, Vec3::new(1.0, 1.0, 5.0), &[pillar]);
        assert!((contact.x - 1.0).abs() < 1e-5);
        assert!(contact.z < 3.2 && contact.z > 3.18);
        assert!(!pillar.contains(contact));
        let side = slide(contact, Vec3::new(3.0, 1.0, 3.0), &[pillar]);
        let back = slide(side, Vec3::new(3.0, 1.0, 10.0), &[pillar]);
        let behind = slide(back, Vec3::new(0.0, 1.0, 13.0), &[pillar]);
        assert_eq!(behind, Vec3::new(0.0, 1.0, 13.0));
    }

    #[test]
    fn overlapping_contact_boxes_retract_independent_of_iteration_order() {
        let near = box_at(Vec3::new(0.0, 1.0, 10.0), Vec3::new(2.0, 2.0, 4.0));
        let far = box_at(Vec3::new(0.0, 1.0, 13.0), Vec3::new(2.0, 2.0, 4.0));
        for obstacles in [[near, far], [far, near]] {
            assert!((resolve(Vec3::Y, Vec3::Z, &obstacles) - 7.69).abs() < 1e-5);
        }
    }
}
