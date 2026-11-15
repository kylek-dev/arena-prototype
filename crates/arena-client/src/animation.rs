//! Lightweight presentation poses; never writes to the simulation.
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum JointKind {
    Chest,
    Head,
    Shoulder,
    Elbow,
    Hip,
    Knee,
}

#[derive(Component)]
pub(crate) struct Joint {
    pub actor: usize,
    pub kind: JointKind,
    pub side: f32,
}

/// Internal tuning, intentionally independent of appearance persistence/input.
#[derive(Resource)]
pub(crate) struct Settings {
    pub stride_length: f32,
    pub leg_swing: f32,
    pub arm_swing: f32,
    pub blend_rate: f32,
    pub walk_bob: f32,
    pub idle_breath: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            stride_length: 2.4,
            leg_swing: 0.48,
            arm_swing: 0.36,
            blend_rate: 14.0,
            walk_bob: 0.018,
            idle_breath: 0.004,
        }
    }
}

#[derive(Component, Default)]
pub(crate) struct Animator {
    pub actor: usize,
    /// Authored clips already contain body motion; do not bob them a second time.
    pub authored: bool,
    previous: Option<Vec3>,
    tick: u64,
    phase: f32,
    velocity: Vec2,
    airborne: f32,
}

#[derive(Clone, Copy, Default)]
struct Pose {
    seconds: f32,
    phase: f32,
    direction: Vec2,
    amount: f32,
    airborne: f32,
}

impl Animator {
    pub fn new(actor: usize) -> Self {
        Self { actor, ..default() }
    }

    fn step(
        &mut self,
        position: Vec3,
        rotation: Quat,
        tick: u64,
        airborne: bool,
        dt: f32,
        settings: &Settings,
    ) -> Pose {
        // Reset/teleport, first frame and background-tab gaps cannot become a stride.
        let delta = self.previous.map(|p| position - p).unwrap_or_default();
        let discontinuity = self.previous.is_none()
            || tick < self.tick
            || !(0.0..=0.1).contains(&dt)
            || delta.xz().length() > 1.0;
        self.previous = Some(position);
        self.tick = tick;
        if discontinuity {
            self.phase = 0.0;
            self.velocity = Vec2::ZERO;
            self.airborne = f32::from(airborne);
        }
        let blend = 1.0 - (-settings.blend_rate * dt.clamp(0.0, 0.1)).exp();
        let local = rotation.inverse() * delta;
        let velocity = if discontinuity || dt <= 0.0 {
            Vec2::ZERO
        } else {
            local.xz() / dt
        };
        self.velocity = self.velocity.lerp(velocity, blend);
        self.airborne += (f32::from(airborne) - self.airborne) * blend;
        let speed = self.velocity.length();
        if !airborne {
            self.phase = (self.phase
                + speed * dt.clamp(0.0, 0.1) * std::f32::consts::TAU
                    / settings.stride_length.max(0.1))
            .rem_euclid(std::f32::consts::TAU);
        }
        Pose {
            seconds: 0.0,
            phase: self.phase,
            direction: self.velocity.normalize_or_zero(),
            amount: (speed / 3.0).min(1.0) * (1.0 - self.airborne),
            airborne: self.airborne,
        }
    }
}

fn joint_rotation(joint: &Joint, pose: Pose, settings: &Settings) -> Quat {
    let swing = pose.phase.sin() * joint.side * pose.amount;
    // -Z is forward. Lateral hips only abduct (open), never adduct across
    // the stance centerline. Alternating open/close steps replace scissoring.
    let forward = -pose.direction.y;
    let strafe = pose.direction.x;
    let breath = (pose.seconds * 1.8).sin() * (1.0 - pose.amount);
    let (pitch, yaw, roll) = match joint.kind {
        JointKind::Chest => (
            -0.08 * forward * pose.amount - 0.10 * pose.airborne + 0.012 * breath,
            pose.phase.sin() * 0.055 * forward * pose.amount,
            -0.06 * strafe * pose.amount,
        ),
        JointKind::Head => (
            0.035 * forward * pose.amount - 0.012 * breath,
            -pose.phase.sin() * 0.035 * forward * pose.amount,
            0.025 * strafe * pose.amount,
        ),
        JointKind::Hip => (
            swing * forward * settings.leg_swing + pose.airborne * 0.50,
            0.0,
            joint.side * (swing * strafe).max(0.0) * settings.leg_swing * 0.5,
        ),
        JointKind::Knee => (
            -swing.max(0.0) * (0.65 * forward.abs() + 0.35 * strafe.abs()) - pose.airborne * 0.95,
            0.0,
            0.0,
        ),
        JointKind::Shoulder => (
            -swing * forward * settings.arm_swing - pose.airborne * 0.22,
            0.0,
            joint.side * (0.06 + 0.08 * pose.airborne) - swing * strafe * settings.arm_swing * 0.25,
        ),
        JointKind::Elbow => (
            0.12 + 0.24 * pose.amount
                + (-swing * forward).max(0.0) * 0.30
                + pose.airborne * 0.35
                + 0.015 * breath,
            0.0,
            0.0,
        ),
    };
    Quat::from_euler(EulerRot::XYZ, pitch, yaw, roll)
}

#[allow(clippy::type_complexity)]
pub(crate) fn animate(
    time: Res<Time>,
    practice: Res<crate::Practice>,
    settings: Res<Settings>,
    fighters: Query<(&crate::FighterVisual, &Transform), Without<Animator>>,
    mut roots: Query<(&mut Animator, &mut Transform), Without<crate::FighterVisual>>,
    mut joints: Query<(&Joint, &mut Transform), (Without<Animator>, Without<crate::FighterVisual>)>,
) {
    let mut poses = [Pose::default(); 6];
    for (mut animator, mut transform) in &mut roots {
        let Some((_, fighter)) = fighters.iter().find(|(f, _)| f.0 == animator.actor) else {
            continue;
        };
        let actor = animator.actor;
        if actor >= practice.arena.fighters().len() {
            continue;
        }
        let mut pose = animator.step(
            fighter.translation,
            fighter.rotation,
            practice.arena.tick(),
            practice.arena.fighters()[actor].height > 0,
            time.delta_secs(),
            &settings,
        );
        pose.seconds = time.elapsed_secs() + actor as f32 * 0.73;
        let breath = (time.elapsed_secs() * 1.8).sin() * 0.5 + 0.5;
        transform.translation.y = if animator.authored {
            0.0
        } else {
            settings.walk_bob * (1.0 - (pose.phase * 2.0).cos()) * pose.amount
                + settings.idle_breath * breath * (1.0 - pose.amount) * (1.0 - pose.airborne)
        };
        poses[actor] = pose;
    }
    for (joint, mut transform) in &mut joints {
        transform.rotation = joint_rotation(joint, poses[joint.actor], &settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_limbs_counter_swing_and_backward_and_strafe_change_axes() {
        let settings = Settings::default();
        let pose = Pose {
            seconds: 0.0,
            phase: std::f32::consts::FRAC_PI_2,
            direction: Vec2::NEG_Y,
            amount: 1.0,
            airborne: 0.0,
        };
        let rotation = |kind, side, pose| {
            joint_rotation(
                &Joint {
                    actor: 0,
                    kind,
                    side,
                },
                pose,
                &settings,
            )
        };
        let left = rotation(JointKind::Hip, -1.0, pose);
        assert!(left.abs_diff_eq(rotation(JointKind::Hip, 1.0, pose).inverse(), 0.0001));
        assert!(
            (left * Vec3::NEG_Y).z * (rotation(JointKind::Shoulder, -1.0, pose) * Vec3::NEG_Y).z
                < 0.0
        );
        let backwards = Pose {
            direction: Vec2::Y,
            ..pose
        };
        assert!(left.abs_diff_eq(rotation(JointKind::Hip, -1.0, backwards).inverse(), 0.0001));
        let lateral = rotation(
            JointKind::Hip,
            1.0,
            Pose {
                direction: Vec2::X,
                ..pose
            },
        ) * Vec3::NEG_Y;
        assert!(lateral.x.abs() > 0.1);
        assert!(lateral.z.abs() < 0.0001);
    }

    #[test]
    fn full_strafe_and_diagonal_cycles_keep_lower_legs_and_boots_in_their_own_lane() {
        let settings = Settings::default();
        for direction in [
            Vec2::X,
            Vec2::NEG_X,
            Vec2::new(1.0, 1.0),
            Vec2::new(-1.0, 1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(-1.0, -1.0),
        ] {
            for step in 0..=360 {
                for side in [-1.0, 1.0] {
                    let pose = Pose {
                        phase: step as f32 * std::f32::consts::TAU / 360.0,
                        direction: direction.normalize(),
                        amount: 1.0,
                        ..default()
                    };
                    let hip = joint_rotation(
                        &Joint {
                            actor: 0,
                            kind: JointKind::Hip,
                            side,
                        },
                        pose,
                        &settings,
                    );
                    let knee = joint_rotation(
                        &Joint {
                            actor: 0,
                            kind: JointKind::Knee,
                            side,
                        },
                        pose,
                        &settings,
                    );
                    // Conservative boxes around the ellipsoids in the real bind pose:
                    // hip y=.85, knee y=.47; both x=side*.155.
                    for (center, half) in [
                        (Vec3::new(0.0, -0.15, 0.0), Vec3::new(0.075, 0.195, 0.09)),
                        (Vec3::new(0.0, -0.36, -0.075), Vec3::new(0.10, 0.105, 0.18)),
                    ] {
                        for x in [-1.0, 1.0] {
                            for y in [-1.0, 1.0] {
                                for z in [-1.0, 1.0] {
                                    let point = Vec3::new(side * 0.155, 0.85, 0.0)
                                        + hip
                                            * (Vec3::new(0.0, -0.38, 0.0)
                                                + knee * (center + half * Vec3::new(x, y, z)));
                                    assert!(point.x * side >= 0.035, "crossed lane: {point:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn upper_body_breathes_and_counter_rotates_and_elbows_flex_during_a_stride() {
        let settings = Settings::default();
        let rotation = |kind, pose| {
            joint_rotation(
                &Joint {
                    actor: 0,
                    kind,
                    side: 1.0,
                },
                pose,
                &settings,
            )
        };
        let idle = Pose {
            seconds: 0.8,
            ..default()
        };
        assert!(!rotation(JointKind::Chest, idle).abs_diff_eq(Quat::IDENTITY, 0.001));
        let walk = Pose {
            phase: std::f32::consts::FRAC_PI_2,
            direction: Vec2::NEG_Y,
            amount: 1.0,
            ..default()
        };
        let chest = rotation(JointKind::Chest, walk).to_euler(EulerRot::XYZ);
        let head = rotation(JointKind::Head, walk).to_euler(EulerRot::XYZ);
        assert!((rotation(JointKind::Chest, walk) * Vec3::Y).z < 0.0);
        assert!(chest.1 * head.1 < 0.0);
        assert!(
            !rotation(JointKind::Elbow, walk).abs_diff_eq(rotation(JointKind::Elbow, idle), 0.1)
        );
    }

    #[test]
    fn movement_blends_to_idle_and_resets_do_not_animate_teleports() {
        let settings = Settings::default();
        let mut animator = Animator::default();
        let mut position = Vec3::ZERO;
        for tick in 0..60 {
            position.z -= 0.05;
            animator.step(position, Quat::IDENTITY, tick, false, 1.0 / 60.0, &settings);
        }
        assert!(animator.velocity.length() > 2.9);
        let phase = animator.phase;
        for tick in 60..180 {
            animator.step(position, Quat::IDENTITY, tick, false, 1.0 / 60.0, &settings);
        }
        assert!(animator.velocity.length() < 0.0001);
        assert_ne!(phase, animator.phase);
        let reset = animator.step(Vec3::ZERO, Quat::IDENTITY, 0, false, 1.0 / 60.0, &settings);
        assert_eq!(reset.amount, 0.0);
        assert_eq!(reset.phase, 0.0);
    }

    #[test]
    fn jump_tucks_knees_and_landing_blends_back_to_rest() {
        let settings = Settings::default();
        let mut animator = Animator::default();
        let pose = animator.step(Vec3::Y, Quat::IDENTITY, 1, true, 0.016, &settings);
        assert_eq!(pose.amount, 0.0);
        let knee = Joint {
            actor: 0,
            kind: JointKind::Knee,
            side: 1.0,
        };
        assert!(!joint_rotation(&knee, pose, &settings).abs_diff_eq(Quat::IDENTITY, 0.1));
        for tick in 2..122 {
            animator.step(
                Vec3::ZERO,
                Quat::IDENTITY,
                tick,
                false,
                1.0 / 60.0,
                &settings,
            );
        }
        assert!(animator.airborne < 0.0001);
        let paused = animator.step(Vec3::ZERO, Quat::IDENTITY, 122, false, 2.0, &settings);
        assert!(paused.phase.is_finite());
        assert_eq!(paused.amount, 0.0);
        assert_eq!(paused.phase, 0.0);
    }
}
