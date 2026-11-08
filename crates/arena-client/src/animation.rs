//! Lightweight presentation poses; never writes to the simulation.
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum JointKind {
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
            phase: self.phase,
            direction: self.velocity.normalize_or_zero(),
            amount: (speed / 3.0).min(1.0) * (1.0 - self.airborne),
            airborne: self.airborne,
        }
    }
}

fn joint_rotation(joint: &Joint, pose: Pose, settings: &Settings) -> Quat {
    let swing = pose.phase.sin() * joint.side * pose.amount;
    // -Z is forward. Reverse swing for backing up; lateral steps swing about Z.
    let forward = -pose.direction.y;
    let strafe = pose.direction.x;
    let (pitch, roll) = match joint.kind {
        JointKind::Hip => (
            swing * forward * settings.leg_swing + pose.airborne * 0.50,
            swing * strafe * settings.leg_swing,
        ),
        JointKind::Knee => (-swing.max(0.0) * 0.60 - pose.airborne * 0.95, 0.0),
        JointKind::Shoulder => (
            -swing * forward * settings.arm_swing - pose.airborne * 0.22,
            -swing * strafe * settings.arm_swing,
        ),
        JointKind::Elbow => (0.10 * pose.amount + pose.airborne * 0.35, 0.0),
    };
    Quat::from_euler(EulerRot::XYZ, pitch, 0.0, roll)
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
    let mut poses = [Pose::default(); 2];
    for (mut animator, mut transform) in &mut roots {
        let Some((_, fighter)) = fighters.iter().find(|(f, _)| f.0 == animator.actor) else {
            continue;
        };
        let actor = animator.actor;
        let pose = animator.step(
            fighter.translation,
            fighter.rotation,
            practice.arena.tick(),
            practice.arena.fighters()[actor].height > 0,
            time.delta_secs(),
            &settings,
        );
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
            -1.0,
            Pose {
                direction: Vec2::X,
                ..pose
            },
        ) * Vec3::NEG_Y;
        assert!(lateral.x.abs() > 0.1);
        assert!(lateral.z.abs() < 0.0001);
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
