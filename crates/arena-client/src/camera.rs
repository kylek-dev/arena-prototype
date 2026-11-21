//! Presentation-only camera contact collision. Map boxes and hexagonal pillar
//! pieces supply their own bounds, so decorative pillar caps, bases and arena
//! rims cannot be missed by the camera.
use crate::prism::Prism;
use bevy::prelude::*;

// Covers the near plane as well as the eye; a point ray would still clip corners.
const CLEARANCE: f32 = 0.3;
const CONTACT_GAP: f32 = 0.01;
const RECOVERY_RATE: f32 = 5.0;
/// An eye this far from its target that gained less than `STUCK_PROGRESS` in
/// a frame is wedged rather than travelling.
const STUCK_GAP: f32 = 0.05;
const STUCK_PROGRESS: f32 = 0.01;
/// Obstacles entered this close to the focus retract the boom even when the
/// requested eye lies beyond them; farther pillars may still occlude.
const NEAR_RETRACT: f32 = 2.0;

#[derive(Component, Clone, Copy)]
pub(super) struct Obstacle {
    shape: Prism,
    /// Floors, walls and other enclosing structure always shorten the boom.
    /// Only free-standing boxes such as pillars may sit between the player
    /// and a distant camera (ADR 0014).
    encloses: bool,
}

/// The static map uses unrotated unit cubes scaled to their visible dimensions.
pub(super) fn solid_box(transform: Transform) -> (Transform, Obstacle) {
    let half = transform.scale.abs() * 0.5;
    let obstacle = Obstacle {
        shape: Prism::cuboid(transform.translation - half, transform.translation + half),
        encloses: false,
    };
    (transform, obstacle)
}

/// A hexagonal pillar piece: the hexagon inscribed in the transform's
/// unrotated, scaled unit cube, like the rules' pillar footprint.
pub(super) fn solid_pillar(transform: Transform) -> (Transform, Obstacle) {
    let half = transform.scale.abs() * 0.5;
    let center = transform.translation;
    let obstacle = Obstacle {
        shape: Prism::hexagon(center, half.x, half.z, center.y - half.y, center.y + half.y),
        encloses: false,
    };
    (transform, obstacle)
}

pub(super) fn solid_floor(transform: Transform) -> (Transform, Obstacle) {
    solid_wall(transform)
}

/// Enclosing structure (walls, stands, ceilings): the camera never sits
/// behind it, so it cannot wedge into corners of a room or the stands.
pub(super) fn solid_wall(transform: Transform) -> (Transform, Obstacle) {
    let (transform, mut obstacle) = solid_box(transform);
    obstacle.encloses = true;
    (transform, obstacle)
}

struct Hit {
    distance: f32,
    normal: Vec3,
}

impl Obstacle {
    fn contains(self, point: Vec3) -> bool {
        self.shape.contains(point, CLEARANCE)
    }

    fn entry(self, origin: Vec3, direction: Vec3, limit: f32) -> Option<Hit> {
        // Sweeping a padded shape is conservative at corners, keeping the
        // camera's near plane outside geometry, not merely its center.
        let (enter, normal) = self.shape.entry(origin, direction, limit, CLEARANCE)?;
        Some(Hit {
            distance: (enter - CONTACT_GAP).max(0.0),
            normal,
        })
    }
}

#[cfg(test)]
pub(super) fn resolve_distance(
    focus: Vec3,
    direction: Vec3,
    requested: f32,
    previous: f32,
    delta_secs: f32,
    obstacles: impl IntoIterator<Item = Obstacle>,
) -> f32 {
    let obstacles: Vec<_> = obstacles.into_iter().collect();
    resolve_distance_slice(
        focus, direction, requested, previous, delta_secs, &obstacles,
    )
}

fn resolve_distance_slice(
    focus: Vec3,
    direction: Vec3,
    requested: f32,
    previous: f32,
    delta_secs: f32,
    obstacles: &[Obstacle],
) -> f32 {
    let mut safe = requested;
    // Contact at the requested eye, or an obstacle right beside the player,
    // retracts. A pillar farther along the boom may obscure the player.
    // Repeat because retracting out of one overlapping box can enter another.
    for _ in 0..=obstacles.len() {
        let before = safe;
        for obstacle in obstacles {
            let Some(hit) = obstacle.entry(focus, direction, safe) else {
                continue;
            };
            // A wall or pillar right beside the player always shortens the
            // boom: the eye stays on the boom instead of being dragged along
            // the face next to the player's head, which swung the view.
            if obstacle.encloses
                || hit.distance < NEAR_RETRACT
                || obstacle.contains(focus + direction * safe)
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
    let distance = resolve_distance_slice(
        focus,
        direction,
        requested,
        previous.map_or(requested, |eye| eye.distance(focus)),
        delta_secs,
        obstacles,
    );
    let target = focus + direction * distance;
    previous.map_or(target, |eye| {
        let slid = slide(eye, target, obstacles);
        // Sliding can wedge into concave corners (stand steps, a parapet over
        // the pit wall) whose only way out is up and over. Only when the eye
        // is truly stuck, and the player can see the boom directly, does it
        // rejoin the boom, at no more than its current distance so zoom stays
        // continuous. Moving eyes, and blocked booms such as orbiting behind
        // a pillar, keep sliding smoothly.
        let remaining = slid.distance(target);
        let stuck = remaining > STUCK_GAP && eye.distance(target) - remaining < STUCK_PROGRESS;
        if stuck && boom_clear(focus, target, obstacles) {
            focus + direction * distance.min(eye.distance(focus))
        } else {
            slid
        }
    })
}

/// Pitch stops just short of straight up/down so the view never degenerates
/// against the world up axis; this allows looking down from directly overhead.
pub(super) const PITCH_LIMIT: f32 = 1.55;
/// Shortest third-person boom. Zooming in past it snaps to first person
/// (distance 0); zooming out of first person starts here.
pub(super) const NEAREST_BOOM: f32 = 1.5;
pub(super) const FARTHEST_BOOM: f32 = 22.0;
/// Pivot height above the feet: chest height for ordinary views, rising to
/// eye level as the boom shortens below `PIVOT_RISE_WITHIN`.
const CHEST: f32 = 1.0;
const EYES: f32 = crate::character::HEIGHT * 0.92;
const PIVOT_RISE_WITHIN: f32 = 6.0;

pub(super) fn pitch(current: f32, delta: f32) -> f32 {
    (current + delta).clamp(-PITCH_LIMIT, PITCH_LIMIT)
}

pub(super) fn zoom(distance: f32, delta: f32) -> f32 {
    let next = (distance + delta).clamp(0.0, FARTHEST_BOOM);
    if next >= NEAREST_BOOM || delta == 0.0 {
        next
    } else if delta < 0.0 {
        0.0
    } else {
        NEAREST_BOOM
    }
}

pub(super) fn first_person(distance: f32) -> bool {
    distance < NEAREST_BOOM
}

pub(super) fn focus_height(distance: f32) -> f32 {
    let t = (distance / PIVOT_RISE_WITHIN).clamp(0.0, 1.0);
    EYES + (CHEST - EYES) * t
}

/// Below this distance from the focus the eye is inside the player's own
/// body, so the player model is hidden rather than rendered from within.
pub(super) const HIDE_PLAYER_WITHIN: f32 = 1.1;

pub(super) fn hides_player(focus: Vec3, eye: Vec3) -> bool {
    focus.distance(eye) < HIDE_PLAYER_WITHIN
}

fn boom_clear(focus: Vec3, target: Vec3, obstacles: &[Obstacle]) -> bool {
    let offset = target - focus;
    let length = offset.length();
    if length < 1e-6 {
        return true;
    }
    let direction = offset / length;
    !obstacles
        .iter()
        .any(|obstacle| obstacle.entry(focus, direction, length).is_some())
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
    /// Regression: the orbit camera must never end up inside, or stay snagged
    /// on, the stepped stands while orbiting from spots along the pit wall.
    #[test]
    fn orbiting_against_the_stands_never_traps_the_camera() {
        let map = arena_map::colosseum();
        let (hx, hz) = (
            map.layout.half_x as f32 / 1000.0,
            map.layout.half_z as f32 / 1000.0,
        );
        // The venue includes the floor and the starting rooms.
        let obstacles = crate::venue::obstacles(&map);
        let edge = 0.45 + 0.05;
        let spots = [
            Vec3::new(0.0, 1.0, hz - edge),
            Vec3::new(3.0, 1.0, -hz + edge),
            Vec3::new(hx - edge, 1.0, 5.0),
            Vec3::new(-hx + edge, 1.0, -9.0),
            Vec3::new(hx - edge, 1.0, hz - edge),
            Vec3::new(-hx + edge, 2.3, -hz + edge),
            // Inside each starting room, and standing in a gateway.
            Vec3::new(0.0, 1.0, hz + 5.0),
            Vec3::new(-2.0, 1.0, -hz - 9.0),
            Vec3::new(2.0, 1.0, hz + 0.7),
        ];
        let dt = 1.0 / 60.0;
        let direction = |yaw: f32, pitch: f32| {
            Quat::from_rotation_y(yaw) * Vec3::new(0.0, pitch.sin(), pitch.cos())
        };
        for focus in spots {
            for distance in [13.0, 22.0] {
                let mut eye = None;
                let mut frame = 0;
                // Sweep yaw twice around while pitch rolls between low and high.
                while frame < 900 {
                    let yaw = frame as f32 * 0.014;
                    let pitch = 0.45 + 0.75 * (frame as f32 * 0.021).sin();
                    let next =
                        resolve_eye(focus, direction(yaw, pitch), distance, eye, dt, &obstacles);
                    assert!(
                        !obstacles.iter().any(|o| o.contains(next)),
                        "eye inside geometry at {next:?} (focus {focus:?}, frame {frame})"
                    );
                    eye = Some(next);
                    frame += 1;
                }
                // Then look back toward the arena: the eye must settle on the
                // boom (no longer wedged off it) and hold still there.
                let toward_center = (Vec3::new(0.0, focus.y, 0.0) - focus).normalize();
                let yaw = toward_center.x.atan2(toward_center.z);
                let wanted = direction(yaw, 0.5);
                for _ in 0..240 {
                    eye = Some(resolve_eye(focus, wanted, distance, eye, dt, &obstacles));
                }
                let settled = eye.unwrap();
                let boom = (settled - focus).normalize_or_zero();
                assert!(
                    boom.dot(wanted) > 0.999,
                    "eye off the boom at {settled:?}, wanted direction {wanted:?} (focus {focus:?})"
                );
                let again = resolve_eye(focus, wanted, distance, eye, dt, &obstacles);
                assert!(
                    settled.distance(again) < 1e-3,
                    "eye still moving at {focus:?}"
                );
            }
        }
    }
    /// Walking past a pillar corner with the camera tucked behind the pillar
    /// must not teleport the eye, and holding still against a wall or pillar
    /// with the camera pressed in must leave the eye perfectly still.
    #[test]
    fn camera_stays_smooth_against_pillars_and_walls() {
        let map = arena_map::colosseum();
        let pillar = map.layout.pillars[3];
        let p = crate::world(pillar.center);
        let h = pillar.half_z as f32 / 1000.0;
        let obstacles = crate::venue::obstacles(&map);
        let dt = 1.0 / 60.0;
        for yaw in (0..24).map(|i| i as f32 / 24.0 * std::f32::consts::TAU) {
            for pitch in [-0.3_f32, 0.0, 0.3, 0.65, 1.0] {
                let direction =
                    Quat::from_rotation_y(yaw) * Vec3::new(0.0, pitch.sin(), pitch.cos());
                let start = Vec3::new(p.x, 1.0, p.z - h - 0.45);
                let mut eye = None;
                for frame in 0..240 {
                    let focus = start + Vec3::X * 7.2 * dt * frame as f32;
                    let next = resolve_eye(focus, direction, 13.0, eye, dt, &obstacles);
                    if let Some(old) = eye {
                        // Never more than a full immediate retraction (13 m)
                        // would allow, and never a sideways teleport.
                        let jump: Vec3 = next - old - Vec3::X * 7.2 * dt;
                        let along_boom = jump.dot(direction).abs();
                        let sideways = (jump - direction * jump.dot(direction)).length();
                        assert!(
                            sideways < 1.0 || along_boom > sideways,
                            "sideways pop {sideways} at yaw {yaw} pitch {pitch} frame {frame}"
                        );
                    }
                    eye = Some(next);
                }
                for start in [
                    Vec3::new(p.x + 1.0, 1.0, p.z - h - 0.45),
                    Vec3::new(10.0, 1.0, -42.0 + 0.45),
                ] {
                    let mut eye = None;
                    for _ in 0..120 {
                        eye = Some(resolve_eye(start, direction, 13.0, eye, dt, &obstacles));
                    }
                    let settled = eye.unwrap();
                    let again = resolve_eye(start, direction, 13.0, eye, dt, &obstacles);
                    assert!(
                        settled.distance(again) < 1e-4,
                        "still eye moved at yaw {yaw}"
                    );
                }
            }
        }
        assert!(hides_player(Vec3::Y, Vec3::Y + Vec3::Z * 0.4));
        assert!(!hides_player(Vec3::Y, Vec3::Y + Vec3::Z * 3.0));
    }

    #[test]
    fn zoom_snaps_between_first_person_and_the_nearest_boom() {
        assert_eq!(zoom(13.0, 1.0), 14.0);
        assert_eq!(zoom(21.5, 3.0), FARTHEST_BOOM);
        assert_eq!(zoom(2.0, -0.4), 1.6);
        assert_eq!(zoom(2.0, -1.0), 0.0);
        assert_eq!(zoom(6.0, -100.0), 0.0);
        assert_eq!(zoom(0.0, -1.0), 0.0);
        assert_eq!(zoom(0.0, 0.2), NEAREST_BOOM);
        assert_eq!(zoom(0.0, 4.0), 4.0);
        assert_eq!(zoom(0.0, 0.0), 0.0);
        assert!(first_person(zoom(3.0, -2.0)));
        assert!(!first_person(NEAREST_BOOM));
    }

    #[test]
    fn pitch_reaches_overhead_without_flipping() {
        assert_eq!(pitch(1.2, 1.0), PITCH_LIMIT);
        assert_eq!(pitch(-1.2, -1.0), -PITCH_LIMIT);
        let direction = Vec3::new(0.0, PITCH_LIMIT.sin(), PITCH_LIMIT.cos());
        assert!(direction.y > 0.999 && direction.z > 0.0);
        assert!(
            Transform::default()
                .looking_to(-direction, Vec3::Y)
                .is_finite()
        );
    }

    #[test]
    fn pivot_rises_to_eye_level_only_for_close_booms() {
        assert_eq!(focus_height(13.0), CHEST);
        assert_eq!(focus_height(6.0), CHEST);
        assert_eq!(focus_height(0.0), EYES);
        assert!(focus_height(3.0) > CHEST && focus_height(3.0) < EYES);
        const { assert!(EYES < crate::character::HEIGHT) };
    }
}
