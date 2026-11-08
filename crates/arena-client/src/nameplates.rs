//! Screen-space anchors derived from interpolated presentation, never rule positions.
use bevy::prelude::*;

pub(super) fn anchor(camera: &Camera, view: &GlobalTransform, position: Vec3) -> Option<Vec2> {
    let size = camera.logical_viewport_size()?;
    // Nameplates are an overlay: world geometry must not occlude them.
    if size.min_element() <= 0.0 {
        return None;
    }
    let point = camera
        .world_to_viewport(view, position + Vec3::Y * 2.25)
        .ok()?
        / size;
    (point.is_finite() && point.cmpge(Vec2::ZERO).all() && point.cmple(Vec2::ONE).all())
        .then_some(point)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::camera::CameraProjection;

    fn test_camera() -> Camera {
        let mut camera = Camera::default();
        camera.computed.clip_from_view = PerspectiveProjection {
            fov: 1.0,
            aspect_ratio: 4.0 / 3.0,
            near: 0.1,
            ..default()
        }
        .get_clip_from_view();
        camera.computed.target_info = Some(bevy::camera::RenderTargetInfo {
            physical_size: UVec2::new(1600, 1200),
            scale_factor: 2.0,
        });
        camera
    }

    #[test]
    fn anchors_follow_rendered_translation_and_reject_offscreen_and_behind_camera() {
        let camera = test_camera();
        let view = GlobalTransform::from(Transform::from_xyz(0.0, 2.25, 8.0));
        let first = anchor(&camera, &view, Vec3::ZERO).unwrap();
        assert!(first.abs_diff_eq(Vec2::splat(0.5), 0.001));
        let moved = anchor(&camera, &view, Vec3::new(1.0, 0.5, 0.0)).unwrap();
        assert!(moved.x > first.x && moved.y < first.y);
        assert_eq!(anchor(&camera, &view, Vec3::new(100.0, 0.0, 0.0)), None);
        assert_eq!(anchor(&camera, &view, Vec3::new(0.0, 0.0, 10.0)), None);
        let turned = GlobalTransform::from(
            Transform::from_xyz(0.0, 2.25, 8.0)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        );
        assert_eq!(anchor(&camera, &turned, Vec3::ZERO), None);
    }

    #[test]
    fn nameplates_remain_visible_and_projected_when_pillars_block_world_hover() {
        let camera = test_camera();
        let p = super::super::world(arena_sim::arena::PILLARS[0].center);
        let position = p - Vec3::Z * 3.0;
        let eye = p + Vec3::new(0.0, 2.25, 5.0);
        let view = GlobalTransform::from(Transform::from_translation(eye));
        let ray = Ray3d::new(eye, Dir3::new(position + Vec3::Y - eye).unwrap());
        assert_eq!(
            super::super::hover::pick(ray, [(1, position)].into_iter()),
            None
        );
        assert!(
            anchor(&camera, &view, position)
                .unwrap()
                .abs_diff_eq(Vec2::splat(0.5), 0.001)
        );
    }

    #[test]
    fn missing_viewport_cannot_publish_an_anchor() {
        assert_eq!(
            anchor(&Camera::default(), &GlobalTransform::default(), Vec3::ZERO),
            None
        );
    }
}
