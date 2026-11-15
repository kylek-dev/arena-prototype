//! Authored character clips driven by presentation samples; never writes rules.
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
    JumpStart,
    JumpLand,
}

impl Clip {
    pub const BASE_COUNT: usize = 8;
    pub const ALL: [Self; 10] = [
        Self::Idle,
        Self::Forward,
        Self::Backward,
        Self::Left,
        Self::Right,
        Self::Jump,
        Self::Attack,
        Self::Cast,
        Self::JumpStart,
        Self::JumpLand,
    ];

    fn loops(self) -> bool {
        matches!(
            self,
            Self::Idle | Self::Forward | Self::Backward | Self::Left | Self::Right
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JumpEdges {
    /// Authored segments of the included Jump_Start / Jump_Land clips.
    pub start: (f32, f32),
    pub land: (f32, f32),
}

/// Authored clip segments, in seconds of each model's own clip time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    /// Jump: leaving the floor to touching down; seeked against rule flight.
    pub jump: (f32, f32),
    /// Cast: the cosmetic gather/release portion played on a successful Pulse.
    pub cast: (f32, f32),
    /// Older local characters have one combined jump clip, not separate edges.
    pub jump_edges: Option<JumpEdges>,
}

impl Timing {
    /// BaseMale keys start at 1/30 s; its Jump is 1.6 s and Cast 2.0 s long.
    pub const BASE_MALE: Self = Self {
        jump: (1.0 / 30.0 + 1.6 * 0.19, 1.0 / 30.0 + 1.6 * 0.79),
        cast: (1.0 / 30.0 + 2.0 * 0.60, 1.0 / 30.0 + 2.0 * 0.60 + 0.8),
        jump_edges: None,
    };
}

const CAST_SPEED: f32 = 1.5;
const TAKEOFF_SECONDS: f32 = 0.30;
const LANDING_SECONDS: f32 = 0.80;

#[derive(Component)]
pub(crate) struct Controller {
    timing: Timing,
    previous: Option<Vec3>,
    tick: u64,
    ready_at: u64,
    cast_remaining: f32,
    current: Option<Clip>,
    was_airborne: bool,
    takeoff: bool,
    land_remaining: f32,
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
fn jump_seek(timing: Timing, fighter: Combatant, overstep: f32) -> f32 {
    let phase = flight_phase(fighter, overstep);
    let (takeoff, landing) = timing.jump;
    takeoff + (landing - takeoff) * phase
}

fn flight_phase(fighter: Combatant, overstep: f32) -> f32 {
    let flight_ticks = 2.0 * JUMP_SPEED as f32 / GRAVITY as f32 + 1.0;
    let elapsed = if fighter.height > 0 {
        (JUMP_SPEED - fighter.vertical_velocity) as f32 / GRAVITY as f32 - 1.0
            + overstep.clamp(0.0, 1.0)
    } else {
        flight_ticks
    };
    (elapsed / flight_ticks).clamp(0.0, 1.0)
}

impl Controller {
    pub(crate) fn new(timing: Timing) -> Self {
        Self {
            timing,
            previous: None,
            tick: 0,
            ready_at: 0,
            cast_remaining: 0.0,
            current: None,
            was_airborne: false,
            takeoff: false,
            land_remaining: 0.0,
        }
    }

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
            (self.timing.cast.1 - self.timing.cast.0) / CAST_SPEED
        } else {
            (self.cast_remaining - sample.dt).max(0.0)
        };
        let velocity = if reset || sample.dt <= 0.0 {
            Vec2::ZERO
        } else {
            (sample.rotation.inverse() * delta).xz() / sample.dt
        };
        let airborne = sample.fighter.height > 0 || sample.position.y > 0.01;
        let landed = !reset && self.was_airborne && !airborne;
        self.takeoff = !reset && airborne && (self.takeoff || !self.was_airborne);
        self.land_remaining = if reset || airborne {
            0.0
        } else if landed {
            LANDING_SECONDS
        } else {
            (self.land_remaining - sample.dt).max(0.0)
        };
        self.was_airborne = airborne;
        // Start follows the first part of actual rule flight: never delay takeoff
        // or play anticipation while the rules say the player is still grounded.
        let flight_seconds =
            (2.0 * JUMP_SPEED as f32 / GRAVITY as f32 + 1.0) / arena_sim::TICKS_PER_SECOND as f32;
        let elapsed = flight_phase(sample.fighter, sample.overstep) * flight_seconds;
        let clip = if airborne {
            if self.timing.jump_edges.is_some() && self.takeoff && elapsed < TAKEOFF_SECONDS {
                Clip::JumpStart
            } else {
                Clip::Jump
            }
        } else if self.cast_remaining > 0.0 {
            // Pulse is the only current ability. Play its cosmetic release only
            // after a successful rule-owned cooldown edge, never on rejected input.
            Clip::Cast
        } else if self.timing.jump_edges.is_some() && self.land_remaining > 0.0 {
            Clip::JumpLand
        } else {
            locomotion(velocity)
        };
        Playback {
            clip,
            speed: match clip {
                Clip::Jump | Clip::JumpStart | Clip::JumpLand => 0.0,
                Clip::Cast => CAST_SPEED,
                Clip::Idle => 1.0,
                _ => (velocity.length() / 2.4).clamp(0.65, 2.0),
            },
            seek: match clip {
                Clip::Jump => Some(jump_seek(self.timing, sample.fighter, sample.overstep)),
                Clip::JumpStart => self.timing.jump_edges.map(|edges| {
                    edges.start.0
                        + (edges.start.1 - edges.start.0)
                            * (elapsed / TAKEOFF_SECONDS).clamp(0.0, 1.0)
                }),
                Clip::JumpLand => self.timing.jump_edges.map(|edges| {
                    edges.land.0
                        + (edges.land.1 - edges.land.0)
                            * (1.0 - self.land_remaining / LANDING_SECONDS).clamp(0.0, 1.0)
                }),
                Clip::Cast => Some(self.timing.cast.0),
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
                } else if matches!(playback.clip, Clip::JumpStart | Clip::Jump | Clip::JumpLand) {
                    Duration::from_millis(45)
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
            if matches!(playback.clip, Clip::Jump | Clip::JumpStart | Clip::JumpLand) {
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

    fn controller() -> Controller {
        Controller::new(Timing::BASE_MALE)
    }

    fn sample(position: Vec3, tick: u64) -> Sample {
        Sample {
            position,
            rotation: Quat::IDENTITY,
            tick,
            fighter: Arena::practice(arena_map::colosseum().layout, 1)
                .unwrap()
                .fighters()[0],
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
            let mut controller = controller();
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
        let mut controller = controller();
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
            let mut controller = controller();
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
        use crate::character::{BASE_FEMALE, BASE_MALE, QUATERNIUS_FEMALE};
        for timing in [
            BASE_MALE.timing,
            BASE_FEMALE.timing,
            QUATERNIUS_FEMALE.timing,
        ] {
            // The loop's seek contract is independent of optional edge clips.
            let timing = Timing {
                jump_edges: None,
                ..timing
            };
            let mut arena = Arena::practice(arena_map::colosseum().layout, 1).unwrap();
            let mut controller = Controller::new(timing);
            controller.step(sample(Vec3::ZERO, 0));
            arena.step(&[ArenaCommand::Jump { actor: 0 }]);
            let mut previous_seek = 0.0;
            let mut seeks = Vec::new();
            for _ in 0..23 {
                let fighter = arena.fighters()[0];
                if fighter.height > 0 {
                    let mut airborne =
                        sample(Vec3::Y * fighter.height as f32 / 1000.0, arena.tick());
                    airborne.fighter = fighter;
                    let playback = controller.step(airborne);
                    assert_eq!(playback.clip, Clip::Jump);
                    assert_eq!(playback.speed, 0.0);
                    let seek = playback.seek.unwrap();
                    assert!(seek >= previous_seek);
                    assert!((timing.jump.0..=timing.jump.1).contains(&seek));
                    previous_seek = seek;
                    seeks.push(seek);
                }
                arena.step(&[]);
            }
            // Flight covers most of the authored airborne segment, in order.
            let span = timing.jump.1 - timing.jump.0;
            assert!(seeks[0] < timing.jump.0 + 0.15 * span);
            assert!(*seeks.last().unwrap() > timing.jump.1 - 0.15 * span);
            assert_eq!(arena.fighters()[0].height, 0);
            assert_eq!(
                controller.step(sample(Vec3::ZERO, arena.tick())).clip,
                Clip::Idle
            );
        }
    }

    #[test]
    fn included_jump_edges_follow_flight_and_landing_once_without_delaying_rules() {
        use crate::character::QUATERNIUS_FEMALE;
        let timing = QUATERNIUS_FEMALE.timing;
        let edges = timing.jump_edges.unwrap();
        let mut arena = Arena::practice(arena_map::colosseum().layout, 1).unwrap();
        let mut controller = Controller::new(timing);
        let rule_sample = |arena: &Arena| {
            let fighter = arena.fighters()[0];
            let mut value = sample(Vec3::Y * fighter.height as f32 / 1000.0, arena.tick());
            value.fighter = fighter;
            value.dt = 1.0 / arena_sim::TICKS_PER_SECOND as f32;
            value
        };
        controller.step(rule_sample(&arena));
        arena.step(&[ArenaCommand::Jump { actor: 0 }]);
        assert!(
            arena.fighters()[0].height > 0,
            "animation must not delay takeoff"
        );
        let mut changes = Vec::new();
        let mut last = None;
        let mut start_frames = 0;
        let mut land_frames = 0;
        for _ in 0..80 {
            let playback = controller.step(rule_sample(&arena));
            if last != Some(playback.clip) {
                changes.push(playback.clip);
                last = Some(playback.clip);
            }
            match playback.clip {
                Clip::JumpStart | Clip::Jump => {
                    start_frames += usize::from(playback.clip == Clip::JumpStart);
                    assert!(arena.fighters()[0].height > 0);
                    let range = if playback.clip == Clip::JumpStart {
                        edges.start
                    } else {
                        timing.jump
                    };
                    assert!((range.0..=range.1).contains(&playback.seek.unwrap()));
                    assert_eq!(playback.speed, 0.0);
                }
                Clip::JumpLand => {
                    land_frames += 1;
                    assert_eq!(arena.fighters()[0].height, 0);
                    assert!((edges.land.0..=edges.land.1).contains(&playback.seek.unwrap()));
                    assert_eq!(playback.speed, 0.0);
                }
                _ => {}
            }
            arena.step(&[]);
        }
        assert_eq!(
            changes,
            vec![Clip::JumpStart, Clip::Jump, Clip::JumpLand, Clip::Idle]
        );
        // Visible at the 30 Hz rule-sample cadence, not the former 0.12/0.28 s.
        assert!(start_frames >= 8);
        assert!(land_frames >= 23);
    }

    #[test]
    fn jump_edges_do_not_replay_after_loading_and_reset_or_block_a_new_jump_or_cast() {
        use crate::character::QUATERNIUS_FEMALE;
        let mut controller = Controller::new(QUATERNIUS_FEMALE.timing);
        let mut airborne = sample(Vec3::Y * 0.1, 10);
        airborne.fighter.height = 100;
        airborne.fighter.vertical_velocity = JUMP_SPEED - GRAVITY;
        assert_eq!(controller.step(airborne).clip, Clip::Jump);
        let mut airborne = sample(Vec3::Y * 0.2, 11);
        airborne.fighter.height = 200;
        airborne.fighter.vertical_velocity = JUMP_SPEED - 2 * GRAVITY;
        assert_eq!(controller.step(airborne).clip, Clip::Jump);
        assert_eq!(controller.step(sample(Vec3::ZERO, 12)).clip, Clip::JumpLand);
        let mut jump_again = sample(Vec3::Y * 0.1, 13);
        jump_again.fighter.height = 100;
        jump_again.fighter.vertical_velocity = JUMP_SPEED - GRAVITY;
        assert_eq!(controller.step(jump_again).clip, Clip::JumpStart);
        let mut successful_cast = sample(Vec3::ZERO, 14);
        successful_cast.fighter.ready_at = 44;
        assert_eq!(controller.step(successful_cast).clip, Clip::Cast);
        // A reset discards both the cast and any pending landing recovery.
        assert_eq!(controller.step(sample(Vec3::ZERO, 0)).clip, Clip::Idle);
        assert_eq!(controller.step(sample(Vec3::ZERO, 1)).clip, Clip::Idle);
    }

    #[test]
    fn each_profile_seeks_inside_its_authored_clips() {
        use crate::character::{BASE_FEMALE, BASE_MALE, QUATERNIUS_FEMALE};
        // (timing, Jump length, Cast length) measured from each authored GLB.
        for (timing, jump, cast) in [
            (BASE_MALE.timing, 1.0 / 30.0 + 1.6, 1.0 / 30.0 + 2.0),
            (BASE_FEMALE.timing, 1.1, 1.2),
            (QUATERNIUS_FEMALE.timing, 2.5, 0.5),
        ] {
            assert!(0.0 <= timing.jump.0 && timing.jump.0 < timing.jump.1);
            assert!(timing.jump.1 <= jump);
            assert!(0.0 <= timing.cast.0 && timing.cast.0 < timing.cast.1);
            assert!(timing.cast.1 <= cast + 1e-4);
            let mut controller = Controller::new(timing);
            controller.step(sample(Vec3::ZERO, 0));
            let mut accepted = sample(Vec3::ZERO, 1);
            accepted.fighter.ready_at = 31;
            let playback = controller.step(accepted);
            assert_eq!(playback.clip, Clip::Cast);
            assert_eq!(playback.seek, Some(timing.cast.0));
            // The cosmetic release ends when its authored segment does.
            let wall = controller.cast_remaining;
            assert!((timing.cast.0 + wall * playback.speed - timing.cast.1).abs() < 1e-4);
            assert!(wall < 0.6);
        }
    }

    #[test]
    fn late_asset_loading_does_not_replay_an_old_cast() {
        let mut first = sample(Vec3::ZERO, 100);
        first.fighter.ready_at = 110;
        let playback = controller().step(first);
        assert!(playback.reset);
        assert_eq!(playback.clip, Clip::Idle);
    }
}
