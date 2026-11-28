//! Authored character clips driven by presentation samples; never writes rules.
use std::time::Duration;

use arena_sim::arena::{Combatant, GRAVITY, JUMP_SPEED};
use arena_sim::spells::SpellId;
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
    /// Authored segments of the included Jump_Start / Jump_Land clips. The
    /// start segment ends on the airborne loop's first pose.
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
    /// Shared length of locomotion loops whose footfalls are frame-aligned
    /// (Backward is the reversed Forward). Switching between them keeps the
    /// step phase instead of restarting the new loop from its first frame.
    pub locomotion_cycle: Option<f32>,
}

impl Timing {
    /// BaseMale keys start at 1/30 s; its Jump is 1.6 s and Cast 2.0 s long.
    pub const BASE_MALE: Self = Self {
        jump: (1.0 / 30.0 + 1.6 * 0.19, 1.0 / 30.0 + 1.6 * 0.79),
        cast: (1.0 / 30.0 + 2.0 * 0.60, 1.0 / 30.0 + 2.0 * 0.60 + 0.8),
        jump_edges: None,
        locomotion_cycle: None,
    };
}

const CAST_SPEED: f32 = 1.5;
const CHANNEL_SPEED: f32 = 0.55;
/// Share of rule flight that plays the complete authored takeoff, so it hands
/// over to the airborne loop exactly on that loop's first pose.
const TAKEOFF_SHARE: f32 = 0.70;
const LANDING_SECONDS: f32 = 0.80;
/// Any clear sideways component plays the strafe: forward/back + strafe keys
/// (45 degrees) always strafe, with the feet stepping across. The two bounds
/// (about 24 and 19 degrees off straight) leave a band where the current clip
/// is kept, so jitter near the boundary cannot alternate clips every frame.
const STRAFE_ENTER: f32 = 0.45;
const STRAFE_EXIT: f32 = 0.35;

#[derive(Component)]
pub(crate) struct Controller {
    actor: usize,
    timing: Timing,
    /// Per-kit animation graph node indices; empty falls back to `CharacterModel.nodes`.
    nodes: Vec<AnimationNodeIndex>,
    previous: Option<Vec3>,
    tick: u64,
    pulse_cd: u32,
    cast_remaining: f32,
    was_channeling: bool,
    current: Option<Clip>,
    locomotion: Option<Clip>,
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

fn locomotion(velocity: Vec2, previous: Option<Clip>) -> Clip {
    let (along, across) = (velocity.y.abs(), velocity.x.abs());
    let longitudinal = match previous {
        Some(Clip::Left | Clip::Right) => across < along * STRAFE_EXIT,
        _ => across <= along * STRAFE_ENTER,
    };
    if velocity.length() < 0.08 {
        Clip::Idle
    } else if longitudinal {
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

/// Pose phase of an aligned locomotion loop; Backward is Forward reversed.
fn pose_phase(clip: Clip, time: f32, cycle: f32) -> Option<f32> {
    match clip {
        Clip::Forward | Clip::Left | Clip::Right => Some(time.rem_euclid(cycle)),
        Clip::Backward => Some((cycle - time).rem_euclid(cycle)),
        _ => None,
    }
}

/// Where `to` should start so its feet continue `from`'s step.
fn carried_seek(from: Clip, time: f32, to: Clip, cycle: f32) -> Option<f32> {
    pose_phase(to, pose_phase(from, time, cycle)?, cycle)
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
        Self::new_for(0, timing, Vec::new())
    }

    pub(crate) fn new_for(actor: usize, timing: Timing, nodes: Vec<AnimationNodeIndex>) -> Self {
        Self {
            actor,
            timing,
            nodes,
            previous: None,
            tick: 0,
            pulse_cd: 0,
            cast_remaining: 0.0,
            was_channeling: false,
            current: None,
            locomotion: None,
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
        let pulse_cd = sample.fighter.cooldown_remaining(SpellId::Pulse);
        let pulse_started = !reset && pulse_cd > self.pulse_cd;
        let channeling = !reset && sample.fighter.active_cast.is_some();
        let channel_started = channeling && !self.was_channeling;
        let cast_started = pulse_started || channel_started;
        self.previous = Some(sample.position);
        self.tick = sample.tick;
        self.pulse_cd = pulse_cd;
        self.cast_remaining = if reset {
            0.0
        } else if pulse_started {
            (self.timing.cast.1 - self.timing.cast.0) / CAST_SPEED
        } else if !channeling && self.was_channeling {
            0.0
        } else {
            (self.cast_remaining - sample.dt).max(0.0)
        };
        self.was_channeling = channeling;
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
        let moving = locomotion(velocity, self.locomotion);
        if moving != Clip::Idle {
            // The planted landing recovery cannot carry a moving body: running
            // out of a landing (or starting to move during recovery) goes
            // straight to the gait instead of sliding the crouched pose.
            self.land_remaining = 0.0;
        }
        // Start follows the first part of actual rule flight: never delay takeoff
        // or play anticipation while the rules say the player is still grounded.
        let phase = flight_phase(sample.fighter, sample.overstep);
        let flight_seconds =
            (2.0 * JUMP_SPEED as f32 / GRAVITY as f32 + 1.0) / arena_sim::TICKS_PER_SECOND as f32;
        let clip = if airborne {
            if self.timing.jump_edges.is_some() && self.takeoff && phase < TAKEOFF_SHARE {
                Clip::JumpStart
            } else {
                Clip::Jump
            }
        } else if self.cast_remaining > 0.0 || channeling {
            // Play the cast clip on a rule-owned Pulse cooldown edge or while
            // channeling an authoritative timed cast, never on rejected input.
            Clip::Cast
        } else if self.timing.jump_edges.is_some() && self.land_remaining > 0.0 {
            Clip::JumpLand
        } else {
            self.locomotion = Some(moving);
            moving
        };
        Playback {
            clip,
            speed: match clip {
                Clip::Jump | Clip::JumpStart | Clip::JumpLand => 0.0,
                Clip::Cast => {
                    if channeling && self.cast_remaining <= 0.0 {
                        CHANNEL_SPEED
                    } else {
                        CAST_SPEED
                    }
                }
                Clip::Idle => 1.0,
                _ => (velocity.length() / 2.4).clamp(0.65, 2.0),
            },
            seek: match clip {
                // After a complete takeoff the loop continues from its first
                // pose in real flight time; without edges it spans the flight.
                Clip::Jump if self.takeoff && self.timing.jump_edges.is_some() => Some(
                    (self.timing.jump.0 + (phase - TAKEOFF_SHARE).max(0.0) * flight_seconds)
                        .min(self.timing.jump.1),
                ),
                Clip::Jump => Some(jump_seek(self.timing, sample.fighter, sample.overstep)),
                Clip::JumpStart => self.timing.jump_edges.map(|edges| {
                    edges.start.0
                        + (edges.start.1 - edges.start.0) * (phase / TAKEOFF_SHARE).clamp(0.0, 1.0)
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

#[allow(clippy::too_many_arguments)]
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
    let mut transforms = [None::<Transform>; crate::MAX_ACTORS];
    for (visual, transform) in &fighters {
        if visual.0 < transforms.len() {
            transforms[visual.0] = Some(*transform);
        }
    }
    let roster = practice.arena.fighters();
    for (mut controller, mut player, mut transitions) in &mut players {
        let actor = controller.actor;
        let Some(&fighter) = roster.get(actor) else {
            continue;
        };
        let Some(transform) = transforms.get(actor).copied().flatten() else {
            continue;
        };
        let playback = controller.step(Sample {
            position: transform.translation,
            rotation: transform.rotation,
            tick: practice.arena.tick(),
            fighter,
            overstep: fixed.overstep_fraction(),
            dt: time.delta_secs(),
        });
        let nodes = if controller.nodes.is_empty() {
            &model.nodes
        } else {
            &controller.nodes
        };
        let Some(&index) = nodes.get(playback.clip as usize) else {
            continue;
        };
        if playback.reset {
            player.stop_all();
            *transitions = AnimationTransitions::new();
        }
        let change = playback.reset
            || controller.current != Some(playback.clip)
            || (playback.clip == Clip::Cast && playback.cast_started);
        if change {
            // Aligned loops continue the current step rather than restarting.
            let carried = controller
                .timing
                .locomotion_cycle
                .filter(|_| !playback.reset)
                .zip(controller.current)
                .and_then(|(cycle, from)| {
                    let &from_node = nodes.get(from as usize)?;
                    let time = player.animation(from_node)?.seek_time();
                    carried_seek(from, time, playback.clip, cycle)
                });
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
            if let Some(seek) = playback.seek.or(carried) {
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
            (Vec3::new(-1.0, 0.0, -1.0), Clip::Left),
            (Vec3::new(1.0, 0.0, 1.0), Clip::Right),
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
        accepted.fighter.cooldowns[SpellId::Pulse.index()] = 29;
        let playback = controller.step(accepted);
        assert_eq!(playback.clip, Clip::Cast);
        assert!(playback.cast_started);
        for tick in 3..80 {
            let mut unchanged = sample(Vec3::ZERO, tick);
            unchanged.fighter.cooldowns[SpellId::Pulse.index()] = 32u32.saturating_sub(tick as u32);
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
            accepted.fighter.cooldowns[SpellId::Pulse.index()] = 29;
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
        let mut last_start_seek = 0.0;
        let mut first_loop_seek = None;
        for _ in 0..80 {
            let playback = controller.step(rule_sample(&arena));
            if last != Some(playback.clip) {
                changes.push(playback.clip);
                last = Some(playback.clip);
            }
            match playback.clip {
                Clip::JumpStart => last_start_seek = playback.seek.unwrap(),
                Clip::Jump => {
                    first_loop_seek.get_or_insert(playback.seek.unwrap());
                }
                _ => {}
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
        assert!(start_frames >= 14);
        assert!(land_frames >= 23);
        // The takeoff finishes on the loop's first pose before handing over.
        let span = edges.start.1 - edges.start.0;
        assert!(last_start_seek > edges.start.1 - 0.1 * span);
        let first_loop_seek = first_loop_seek.unwrap();
        assert!(first_loop_seek - timing.jump.0 < 0.1);
    }

    #[test]
    fn landing_while_moving_runs_on_instead_of_sliding_the_recovery_pose() {
        use crate::character::QUATERNIUS_FEMALE;
        let timing = QUATERNIUS_FEMALE.timing;
        let run = |arena: &mut Arena, controller: &mut Controller, x: &mut f32, steps, moving| {
            let mut clips = Vec::new();
            for _ in 0..steps {
                let fighter = arena.fighters()[0];
                let mut value = sample(
                    Vec3::new(*x, fighter.height as f32 / 1000.0, 0.0),
                    arena.tick(),
                );
                value.fighter = fighter;
                value.dt = 1.0 / arena_sim::TICKS_PER_SECOND as f32;
                clips.push(controller.step(value).clip);
                if moving {
                    *x -= 0.24;
                }
                arena.step(&[]);
            }
            clips
        };
        // Moving through touchdown never enters the planted landing pose.
        let mut arena = Arena::practice(arena_map::colosseum().layout, 1).unwrap();
        let mut controller = Controller::new(timing);
        let mut x = 0.0;
        run(&mut arena, &mut controller, &mut x, 1, true);
        arena.step(&[ArenaCommand::Jump { actor: 0 }]);
        let clips = run(&mut arena, &mut controller, &mut x, 40, true);
        assert!(clips.contains(&Clip::Jump));
        assert!(!clips.contains(&Clip::JumpLand));
        assert_eq!(*clips.last().unwrap(), Clip::Left);
        // A standing landing recovers until movement starts, then never resumes.
        let mut arena = Arena::practice(arena_map::colosseum().layout, 1).unwrap();
        let mut controller = Controller::new(timing);
        let mut x = 0.0;
        run(&mut arena, &mut controller, &mut x, 1, false);
        arena.step(&[ArenaCommand::Jump { actor: 0 }]);
        let clips = run(&mut arena, &mut controller, &mut x, 26, false);
        assert_eq!(*clips.last().unwrap(), Clip::JumpLand);
        let clips = run(&mut arena, &mut controller, &mut x, 3, true);
        assert!(clips[1..].iter().all(|clip| *clip == Clip::Left));
        let clips = run(&mut arena, &mut controller, &mut x, 3, false);
        assert!(clips[1..].iter().all(|clip| *clip == Clip::Idle));
    }

    #[test]
    fn diagonals_always_strafe_whichever_key_was_pressed_first() {
        // W then A, A then W, S then D, D then S: every diagonal is a strafe.
        for (first, diagonal, clip) in [
            (Vec3::NEG_Z, Vec3::new(-1.0, 0.0, -1.0), Clip::Left),
            (Vec3::NEG_X, Vec3::new(-1.0, 0.0, -1.0), Clip::Left),
            (Vec3::Z, Vec3::new(1.0, 0.0, 1.0), Clip::Right),
            (Vec3::X, Vec3::new(1.0, 0.0, 1.0), Clip::Right),
        ] {
            let mut controller = controller();
            controller.step(sample(Vec3::ZERO, 0));
            let mut position = first * 0.24;
            controller.step(sample(position, 1));
            let diagonal = diagonal.normalize() * 0.24;
            for tick in 2..40 {
                // Alternating jitter of a few degrees never flips the clip.
                let wobble = if tick % 2 == 0 { 1.05 } else { 0.95 };
                position += Vec3::new(diagonal.x * wobble, 0.0, diagonal.z);
                assert_eq!(controller.step(sample(position, tick)).clip, clip);
            }
        }
        // Near-straight drift (e.g. sliding along a wall) stays forward, and
        // jitter around the strafe boundary keeps the current clip.
        let mut controller = controller();
        controller.step(sample(Vec3::ZERO, 0));
        let mut position = Vec3::ZERO;
        for (tick, across) in [0.02, 0.0, 0.02, 0.10, 0.096, 0.10].into_iter().enumerate() {
            position += Vec3::new(-across, 0.0, -0.24);
            let clip = controller.step(sample(position, tick as u64 + 1)).clip;
            assert_eq!(clip, Clip::Forward, "{tick}");
        }
        position += Vec3::new(-0.12, 0.0, -0.24);
        assert_eq!(controller.step(sample(position, 10)).clip, Clip::Left);
        for (tick, across) in [0.10, 0.096, 0.10].into_iter().enumerate() {
            position += Vec3::new(-across, 0.0, -0.24);
            assert_eq!(
                controller.step(sample(position, tick as u64 + 11)).clip,
                Clip::Left
            );
        }
        position += Vec3::new(-0.03, 0.0, -0.24);
        assert_eq!(controller.step(sample(position, 20)).clip, Clip::Forward);
    }

    #[test]
    fn aligned_loops_carry_the_step_phase_across_switches() {
        let cycle = 28.0 / 30.0;
        for (from, time, to, expected) in [
            (Clip::Forward, 0.2, Clip::Left, 0.2),
            (Clip::Left, 0.5, Clip::Right, 0.5),
            (Clip::Right, cycle + 0.1, Clip::Forward, 0.1),
            (Clip::Forward, 0.2, Clip::Backward, cycle - 0.2),
            (Clip::Backward, cycle - 0.2, Clip::Left, 0.2),
        ] {
            let seek = carried_seek(from, time, to, cycle).unwrap();
            assert!((seek - expected).abs() < 1e-5, "{from:?}->{to:?}: {seek}");
        }
        assert_eq!(carried_seek(Clip::Idle, 0.2, Clip::Left, cycle), None);
        assert_eq!(
            carried_seek(Clip::Forward, 0.2, Clip::JumpLand, cycle),
            None
        );
        use crate::character::{BASE_FEMALE, BASE_MALE, QUATERNIUS_FEMALE};
        // Only the profile whose loops are authored frame-aligned carries phase.
        assert_eq!(QUATERNIUS_FEMALE.timing.locomotion_cycle, Some(cycle));
        assert_eq!(BASE_FEMALE.timing.locomotion_cycle, None);
        assert_eq!(BASE_MALE.timing.locomotion_cycle, None);
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
        successful_cast.fighter.cooldowns[SpellId::Pulse.index()] = 29;
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
            accepted.fighter.cooldowns[SpellId::Pulse.index()] = 29;
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
        first.fighter.cooldowns[SpellId::Pulse.index()] = 10;
        let playback = controller().step(first);
        assert!(playback.reset);
        assert_eq!(playback.clip, Clip::Idle);
    }

    #[test]
    fn timed_cast_plays_cast_clip_and_clears_on_interrupt() {
        use arena_sim::arena::ActiveCastState;
        let mut controller = controller();
        controller.step(sample(Vec3::ZERO, 0));
        let mut channeling = sample(Vec3::ZERO, 1);
        channeling.fighter.active_cast = Some(ActiveCastState {
            spell: SpellId::RiftBolt,
            target: Some(1),
            remaining_ticks: 44,
            total_ticks: 45,
        });
        let started = controller.step(channeling);
        assert_eq!(started.clip, Clip::Cast);
        assert!(started.cast_started);
        assert_eq!(started.speed, CHANNEL_SPEED);
        let mut continuing = sample(Vec3::ZERO, 2);
        continuing.fighter.active_cast = Some(ActiveCastState {
            spell: SpellId::RiftBolt,
            target: Some(1),
            remaining_ticks: 43,
            total_ticks: 45,
        });
        let held = controller.step(continuing);
        assert_eq!(held.clip, Clip::Cast);
        assert!(!held.cast_started);
        let interrupted = controller.step(sample(Vec3::ZERO, 3));
        assert_eq!(interrupted.clip, Clip::Idle);
    }

    #[test]
    fn multiple_fighters_animate_independently_from_their_own_transforms() {
        let mut app = App::new();
        let (_, nodes_a) =
            AnimationGraph::from_clips(Clip::ALL.map(|_| Handle::<AnimationClip>::default()));
        let (_, nodes_b) =
            AnimationGraph::from_clips(Clip::ALL.map(|_| Handle::<AnimationClip>::default()));
        app.insert_resource(crate::character::CharacterModel::ready(
            &crate::character::QUATERNIUS_FEMALE,
        ))
        .init_resource::<Time>()
        .init_resource::<Time<Fixed>>()
        .insert_resource(crate::Practice::new(&arena_map::colosseum(), 1))
        .add_systems(Update, animate);

        let f0 = app
            .world_mut()
            .spawn((crate::FighterVisual(0), Transform::default()))
            .id();
        let f1 = app
            .world_mut()
            .spawn((crate::FighterVisual(1), Transform::default()))
            .id();
        let p0 = app
            .world_mut()
            .spawn((
                Controller::new_for(
                    0,
                    crate::character::QUATERNIUS_FEMALE.timing,
                    nodes_a.clone(),
                ),
                AnimationPlayer::default(),
                AnimationTransitions::new(),
            ))
            .id();
        let p1 = app
            .world_mut()
            .spawn((
                Controller::new_for(
                    1,
                    crate::character::QUATERNIUS_FEMALE.timing,
                    nodes_b.clone(),
                ),
                AnimationPlayer::default(),
                AnimationTransitions::new(),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(1.0 / 60.0));
        app.world_mut().run_schedule(Update);
        // Move ONLY actor 1 forward while actor 0 stays idle.
        app.world_mut()
            .get_mut::<Transform>(f1)
            .unwrap()
            .translation
            .z = -0.05;
        let _ = f0;
        app.world_mut().run_schedule(Update);
        assert_eq!(
            app.world()
                .get::<AnimationTransitions>(p0)
                .unwrap()
                .get_main_animation(),
            Some(nodes_a[Clip::Idle as usize])
        );
        assert_eq!(
            app.world()
                .get::<AnimationTransitions>(p1)
                .unwrap()
                .get_main_animation(),
            Some(nodes_b[Clip::Forward as usize])
        );
    }
}
