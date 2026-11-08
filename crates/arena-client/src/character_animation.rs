//! Authored BaseMale clips driven by presentation samples; never writes rules.
use std::time::Duration;

use arena_sim::arena::{Combatant, GRAVITY, JUMP_SPEED};
use bevy::prelude::*;

/// Indices are fixed by scripts/prepare_character.py, not source export order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum Clip {
    Idle,
    Forward,
    Backward,
    Left,
    Right,
    Jump,
    Attack,
    Cast,
}

impl Clip {
    pub const ALL: [Self; 8] = [
        Self::Idle,
        Self::Forward,
        Self::Backward,
        Self::Left,
        Self::Right,
        Self::Jump,
        Self::Attack,
        Self::Cast,
    ];

    fn loops(self) -> bool {
        matches!(
            self,
            Self::Idle | Self::Forward | Self::Backward | Self::Left | Self::Right
        )
    }
}

#[derive(Component, Default)]
pub(crate) struct Controller {
    previous: Option<Vec3>,
    tick: u64,
    ready_at: u64,
    cast_remaining: f32,
    current: Option<Clip>,
}

struct Sample {
    position: Vec3,
    rotation: Quat,
    tick: u64,
    fighter: Combatant,
    overstep: f32,
    dt: f32,
}

#[derive(Debug)]
struct Playback {
    clip: Clip,
    speed: f32,
    seek: Option<f32>,
    reset: bool,
    cast_started: bool,
}

fn locomotion(velocity: Vec2) -> Clip {
    if velocity.length() < 0.08 {
        Clip::Idle
    } else if velocity.y.abs() >= velocity.x.abs() {
        if velocity.y < 0.0 {
            Clip::Forward
        } else {
            Clip::Backward
        }
    } else if velocity.x < 0.0 {
        Clip::Left
    } else {
        Clip::Right
    }
}

/// Map the rule-owned flight onto the authored takeoff-to-landing segment.
/// Jump root translation is removed at build time; this only poses the limbs.
fn jump_seek(fighter: Combatant, overstep: f32) -> f32 {
    let flight_ticks = 2.0 * JUMP_SPEED as f32 / GRAVITY as f32 + 1.0;
    let elapsed = if fighter.height > 0 {
        (JUMP_SPEED - fighter.vertical_velocity) as f32 / GRAVITY as f32 - 1.0
            + overstep.clamp(0.0, 1.0)
    } else {
        flight_ticks
    };
    let phase = (elapsed / flight_ticks).clamp(0.0, 1.0);
    1.0 / 30.0 + 1.6 * (0.19 + 0.60 * phase)
}

impl Controller {
    fn step(&mut self, sample: Sample) -> Playback {
        let delta = self
            .previous
            .map(|p| sample.position - p)
            .unwrap_or_default();
        let reset = self.previous.is_none()
            || sample.tick < self.tick
            || !(0.0..=0.1).contains(&sample.dt)
            || delta.xz().length() > 1.0;
        let cast_started = !reset && sample.fighter.ready_at > self.ready_at;
        self.previous = Some(sample.position);
        self.tick = sample.tick;
        self.ready_at = sample.fighter.ready_at;
        self.cast_remaining = if reset {
            0.0
        } else if cast_started {
            0.8 / 1.5
        } else {
            (self.cast_remaining - sample.dt).max(0.0)
        };
        let velocity = if reset || sample.dt <= 0.0 {
            Vec2::ZERO
        } else {
            (sample.rotation.inverse() * delta).xz() / sample.dt
        };
        let airborne = sample.fighter.height > 0 || sample.position.y > 0.01;
        let clip = if airborne {
            Clip::Jump
        } else if self.cast_remaining > 0.0 {
            // Pulse is the only current ability. Play its cosmetic release only
            // after a successful rule-owned cooldown edge, never on rejected input.
            Clip::Cast
        } else {
            locomotion(velocity)
        };
        Playback {
            clip,
            speed: match clip {
                Clip::Jump => 0.0,
                Clip::Cast => 1.5,
                Clip::Idle => 1.0,
                _ => (velocity.length() / 2.4).clamp(0.65, 2.0),
            },
            seek: match clip {
                Clip::Jump => Some(jump_seek(sample.fighter, sample.overstep)),
                Clip::Cast => Some(1.0 / 30.0 + 2.0 * 0.60),
                _ => None,
            },
            reset,
            cast_started,
        }
    }
}

pub(crate) fn animate(
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    practice: Res<crate::Practice>,
    model: Res<crate::character::CharacterModel>,
    fighters: Query<(&crate::FighterVisual, &Transform)>,
    mut players: Query<(
        &mut Controller,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
    )>,
) {
    let Some((_, transform)) = fighters.iter().find(|(f, _)| f.0 == 0) else {
        return;
    };
    for (mut controller, mut player, mut transitions) in &mut players {
        let playback = controller.step(Sample {
            position: transform.translation,
            rotation: transform.rotation,
            tick: practice.arena.tick(),
            fighter: practice.arena.fighters()[0],
            overstep: fixed.overstep_fraction(),
            dt: time.delta_secs(),
        });
        if playback.reset {
            player.stop_all();
            *transitions = AnimationTransitions::new();
        }
        let index = model.nodes[playback.clip as usize];
        let change = playback.reset
            || controller.current != Some(playback.clip)
            || (playback.clip == Clip::Cast && playback.cast_started);
        if change {
            let active = transitions.play(
                &mut player,
                index,
                if playback.reset {
                    Duration::ZERO
                } else {
                    Duration::from_millis(120)
                },
            );
            if playback.clip.loops() {
                active.repeat();
            }
            if let Some(seek) = playback.seek {
                active.seek_to(seek);
            }
            controller.current = Some(playback.clip);
        }
        if let Some(active) = player.animation_mut(index) {
            active.set_speed(playback.speed);
            if playback.clip == Clip::Jump {
                // Speed zero, not pause(): paused clips bypass transition cleanup.
                active.seek_to(playback.seek.unwrap_or_default());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_sim::arena::{Arena, ArenaCommand};

    fn sample(position: Vec3, tick: u64) -> Sample {
        Sample {
            position,
            rotation: Quat::IDENTITY,
            tick,
            fighter: Arena::default().fighters()[0],
            overstep: 0.5,
            dt: 1.0 / 60.0,
        }
    }

    #[test]
    fn movement_uses_actual_local_displacement_and_preserves_facing() {
        for (delta, expected) in [
            (Vec3::NEG_Z, Clip::Forward),
            (Vec3::Z, Clip::Backward),
            (Vec3::NEG_X, Clip::Left),
            (Vec3::X, Clip::Right),
            (Vec3::new(-1.0, 0.0, -1.0), Clip::Forward),
        ] {
            let mut controller = Controller::default();
            controller.step(sample(Vec3::ZERO, 0));
            let rotation = Quat::from_rotation_y(1.2);
            let mut moving = sample(rotation * delta * 0.05, 1);
            moving.rotation = rotation;
            assert_eq!(controller.step(moving).clip, expected);
            // Blocked intent and orbit/turn without displacement are not walking.
            let mut stopped = sample(rotation * delta * 0.05, 2);
            stopped.rotation = Quat::IDENTITY;
            assert_eq!(controller.step(stopped).clip, Clip::Idle);
        }
    }

    #[test]
    fn accepted_cooldown_edge_casts_once_and_rejected_input_cannot_cast() {
        let mut controller = Controller::default();
        controller.step(sample(Vec3::ZERO, 0));
        assert_eq!(controller.step(sample(Vec3::ZERO, 1)).clip, Clip::Idle);
        let mut accepted = sample(Vec3::ZERO, 2);
        accepted.fighter.ready_at = 32;
        let playback = controller.step(accepted);
        assert_eq!(playback.clip, Clip::Cast);
        assert!(playback.cast_started);
        for tick in 3..80 {
            let mut unchanged = sample(Vec3::ZERO, tick);
            unchanged.fighter.ready_at = 32;
            assert!(!controller.step(unchanged).cast_started);
        }
        assert_eq!(controller.cast_remaining, 0.0);
        assert_eq!(controller.step(sample(Vec3::ZERO, 80)).clip, Clip::Idle);
    }

    #[test]
    fn reset_teleport_and_tab_resume_clear_stale_casts_and_velocity() {
        for mode in 0..3 {
            let mut controller = Controller::default();
            controller.step(sample(Vec3::ZERO, 20));
            let mut accepted = sample(Vec3::ZERO, 21);
            accepted.fighter.ready_at = 51;
            assert_eq!(controller.step(accepted).clip, Clip::Cast);
            let mut reset = sample(Vec3::ZERO, 22);
            if mode == 0 {
                reset.tick = 0;
            } else if mode == 1 {
                reset.position = Vec3::X * 10.0;
            } else {
                reset.dt = 2.0;
            }
            let playback = controller.step(reset);
            assert!(playback.reset);
            assert_eq!(playback.clip, Clip::Idle);
            assert!(!playback.cast_started);
        }
    }

    #[test]
    fn jump_pose_tracks_rule_flight_without_supplying_height() {
        let mut arena = Arena::default();
        let mut controller = Controller::default();
        controller.step(sample(Vec3::ZERO, 0));
        arena.step(&[ArenaCommand::Jump { actor: 0 }]);
        let mut previous_seek = 0.0;
        for _ in 0..23 {
            let fighter = arena.fighters()[0];
            if fighter.height > 0 {
                let mut airborne = sample(Vec3::Y * fighter.height as f32 / 1000.0, arena.tick());
                airborne.fighter = fighter;
                let playback = controller.step(airborne);
                assert_eq!(playback.clip, Clip::Jump);
                assert_eq!(playback.speed, 0.0);
                let seek = playback.seek.unwrap();
                assert!(seek >= previous_seek);
                assert!((0.33..1.31).contains(&seek));
                previous_seek = seek;
            }
            arena.step(&[]);
        }
        assert_eq!(arena.fighters()[0].height, 0);
        assert_eq!(
            controller.step(sample(Vec3::ZERO, arena.tick())).clip,
            Clip::Idle
        );
    }

    #[test]
    fn late_asset_loading_does_not_replay_an_old_cast() {
        let mut first = sample(Vec3::ZERO, 100);
        first.fighter.ready_at = 110;
        let playback = Controller::default().step(first);
        assert!(playback.reset);
        assert_eq!(playback.clip, Clip::Idle);
    }
}
