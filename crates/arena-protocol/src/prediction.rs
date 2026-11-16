//! Client-side prediction of a player's own fighter (ADR 0044).
//!
//! The server applies one input per tick, in order, so a client can show its
//! own movement at once: it runs the shared rules on each input as it sends
//! it, and when a snapshot arrives it rebuilds from the server's state and
//! replays every input the server has not acknowledged yet. The server stays
//! the authority; the client only guesses its own fighter's next state.
//!
//! Attacks are never predicted (their outcome is the server's), and other
//! fighters are not predicted (bodies do not collide, so they cannot affect
//! this fighter's movement). A *correction* is any change to the predicted
//! state that reconciling causes: zero when the guess was right.

use std::collections::VecDeque;

use arena_sim::arena::{ActorInput, Arena, Combatant, RoundResult};
use arena_sim::map::Layout;

/// Unacknowledged inputs kept for replay (over 8 s at 30 Hz).
const MAX_PENDING: usize = 256;

/// Inputs a client aims to keep queued on the server at the *lowest* point
/// of the last second: one input of cushion left even just before a late
/// batch arrives.
pub const QUEUE_TARGET: u8 = 1;
/// Most a client stretches or shrinks its input interval to steer the queue.
pub const MAX_PACE: f64 = 0.04;
/// Snapshots the pacer looks back over: one second at 30 Hz.
pub const PACE_WINDOW: usize = 30;

/// Steers how fast a client sends inputs so the server always has one of
/// its inputs in hand. Real connections (and some local relays) deliver
/// inputs in clumps, so the queue swings; what matters is its lowest point.
/// If that reaches zero the server idles a tick waiting, so the client
/// sends a little faster and the queue grows until it covers the clumping;
/// if even the lowest point holds more than [`QUEUE_TARGET`], the extra is
/// only delay, so it sends a little slower. This makes the server's queue
/// an adaptive jitter buffer. The server folds inputs together only when
/// its queue stays long for a whole second, which a paced client avoids.
#[derive(Clone, Debug, Default)]
pub struct Pacer {
    recent: VecDeque<u8>,
    average: Option<f64>,
}
impl Pacer {
    /// Records the queue length a snapshot reported.
    pub fn observe(&mut self, queued: u8) {
        self.recent.push_back(queued);
        if self.recent.len() > PACE_WINDOW {
            self.recent.pop_front();
        }
        let sample = f64::from(queued);
        self.average = Some(
            self.average
                .map_or(sample, |avg| avg + (sample - avg) / PACE_WINDOW as f64),
        );
    }
    /// The average queue, for diagnostics.
    pub fn average(&self) -> Option<f64> {
        self.average
    }
    /// The lowest queue over the last second.
    pub fn minimum(&self) -> Option<u8> {
        self.recent.iter().copied().min()
    }
    /// Multiplier for the input interval: above 1 sends slower (a cushion
    /// to spare), below 1 faster (the queue ran dry).
    pub fn factor(&self) -> f64 {
        self.minimum().map_or(1.0, |min| {
            1.0 + MAX_PACE * (f64::from(min) - f64::from(QUEUE_TARGET)).clamp(-1.0, 1.0)
        })
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// The server's nominal tick rate, used until a rate has been measured.
pub const NOMINAL_TICK_RATE: f64 = arena_sim::TICKS_PER_SECOND as f64;
/// Seconds of snapshots a tick-rate estimate looks back over.
pub const RATE_WINDOW: f64 = 5.0;

/// Measures how fast the server's ticks advance on the client's own clock.
///
/// A client must send one input per server tick, and its interpolated view
/// must advance at the server's pace. Both are driven by the server's rate
/// *as the client's clock sees it*: clocks drift, and a virtual machine's
/// clock was once measured 9% slow, which made a client sending a nominal
/// 30 inputs per second flood its server. The estimate is the ticks advanced
/// over the last few seconds of snapshots divided by the local time taken.
#[derive(Clone, Debug, Default)]
pub struct TickRate {
    samples: VecDeque<(f64, u64)>,
}
impl TickRate {
    /// Records that a snapshot for server `tick` arrived at local `now`.
    pub fn observe(&mut self, now: f64, tick: u64) {
        // A new round or a reconnect can rewind the tick: start over.
        if self.samples.back().is_some_and(|(_, last)| tick < *last) {
            self.samples.clear();
        }
        self.samples.push_back((now, tick));
        while self.samples.len() > 2 && now - self.samples[1].0 >= RATE_WINDOW {
            self.samples.pop_front();
        }
    }
    /// Server ticks per local second, once a second or more has been seen.
    pub fn measured(&self) -> Option<f64> {
        let (first_at, first) = *self.samples.front()?;
        let (last_at, last) = *self.samples.back()?;
        let span = last_at - first_at;
        (span >= 1.0).then(|| ((last - first) as f64 / span).clamp(10.0, 60.0))
    }
    /// The measured rate, or the nominal one until there is a measurement.
    pub fn rate(&self) -> f64 {
        self.measured().unwrap_or(NOMINAL_TICK_RATE)
    }
    pub fn reset(&mut self) {
        self.samples.clear();
    }
}

#[derive(Clone, Debug, Default)]
pub struct Predictor {
    /// The server's latest state plus every unacknowledged input.
    arena: Option<Arena>,
    actor: usize,
    pending: VecDeque<(u32, ActorInput)>,
    corrections: u64,
}

impl Predictor {
    /// The predicted state of this player's fighter, once reconciled once.
    pub fn predicted(&self) -> Option<Combatant> {
        self.arena
            .as_ref()
            .and_then(|arena| arena.fighters().get(self.actor).copied())
    }
    /// Inputs sent but not yet acknowledged: how far ahead the prediction runs.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
    /// Reconciliations that moved the prediction.
    pub fn corrections(&self) -> u64 {
        self.corrections
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// While the match waits for players: forget inputs the server has
    /// acknowledged (it discards them) and predict nothing yet. Inputs still
    /// in flight stay queued, because the server may start the match before
    /// they arrive and then apply them; the first reconcile replays them.
    pub fn acknowledge(&mut self, ack: u32) {
        while self.pending.front().is_some_and(|(seq, _)| *seq <= ack) {
            self.pending.pop_front();
        }
        self.arena = None;
    }

    /// Applies this player's input `seq` for the next tick, as the server
    /// will. Returns the new predicted state, or `None` before the first
    /// reconcile.
    pub fn apply(&mut self, seq: u32, input: ActorInput) -> Option<Combatant> {
        let input = ActorInput {
            attack: None,
            ..input
        };
        self.pending.push_back((seq, input));
        if self.pending.len() > MAX_PENDING {
            self.pending.pop_front();
        }
        let actor = self.actor;
        let arena = self.arena.as_mut()?;
        step(arena, actor, input);
        arena.fighters().get(actor).copied()
    }

    /// Rebuilds from the server's state (`fighters` in server actor order,
    /// `actor` this player's seat, `ack` the last input it applied) and
    /// replays the inputs it has not applied yet. Returns how far the
    /// prediction moved, in millimetres, or `None` if there was none yet.
    pub fn reconcile(
        &mut self,
        layout: &Layout,
        tick: u64,
        fighters: Vec<Combatant>,
        result: Option<RoundResult>,
        actor: usize,
        ack: u32,
    ) -> Option<u64> {
        while self.pending.front().is_some_and(|(seq, _)| *seq <= ack) {
            self.pending.pop_front();
        }
        let before = self.predicted();
        let mut arena = Arena::restore(layout.clone(), tick, fighters, result)?;
        if actor >= arena.fighters().len() {
            self.arena = None;
            return None;
        }
        for (_, input) in &self.pending {
            step(&mut arena, actor, *input);
        }
        let after = arena.fighters()[actor];
        self.arena = Some(arena);
        self.actor = actor;
        let moved = before.map(|before| distance(before, after))?;
        if moved > 0 {
            self.corrections += 1;
        }
        Some(moved)
    }
}

fn step(arena: &mut Arena, actor: usize, input: ActorInput) {
    let mut slots = vec![ActorInput::default(); actor + 1];
    slots[actor] = input;
    arena.advance(&slots);
}

/// Distance between two predicted states, in millimetres, including height.
pub fn distance(a: Combatant, b: Combatant) -> u64 {
    let dx = i64::from(a.position.x) - i64::from(b.position.x);
    let dz = i64::from(a.position.z) - i64::from(b.position.z);
    let dy = i64::from(a.height) - i64::from(b.height);
    ((dx * dx + dz * dz + dy * dy) as u64).isqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_sim::arena::Axes;

    fn layout() -> Layout {
        arena_map::colosseum().layout
    }
    fn walk(seq: u32) -> (u32, ActorInput) {
        (
            seq,
            ActorInput {
                movement: Some(Axes { x: 0, z: -1000 }),
                facing: Some(0),
                jump: seq == 3,
                attack: Some(1),
            },
        )
    }

    /// A stand-in server applying one input per tick, as `arena-server` does.
    fn server_tick(server: &mut Arena, input: Option<ActorInput>) {
        server.advance(&[input.unwrap_or_default()]);
    }

    #[test]
    fn predictions_match_a_server_that_applies_one_input_per_tick() {
        let mut server = Arena::practice(layout(), 1).unwrap();
        let mut predictor = Predictor::default();
        // Nothing to predict from yet.
        assert_eq!(predictor.apply(0, ActorInput::default()), None);
        predictor.reconcile(&layout(), 0, server.fighters().to_vec(), None, 0, 0);
        // The client runs two ticks ahead of the server's acknowledgements.
        let mut sent = Vec::new();
        for seq in 1..=40 {
            let (seq, input) = walk(seq);
            let predicted = predictor.apply(seq, input).unwrap();
            sent.push(input);
            if seq > 2 {
                let applied = sent[(seq - 3) as usize];
                server_tick(&mut server, Some(applied));
                let moved = predictor.reconcile(
                    &layout(),
                    server.tick(),
                    server.fighters().to_vec(),
                    None,
                    0,
                    seq - 2,
                );
                assert_eq!(moved, Some(0), "tick {seq}");
                assert_eq!(predictor.predicted(), Some(predicted));
                assert_eq!(predictor.pending(), 2);
            }
        }
        assert_eq!(predictor.corrections(), 0);
        // Attacks are left to the server: the opponent was never hurt.
        assert_eq!(server.fighters()[1].health, 100);
    }

    #[test]
    fn a_server_that_disagrees_corrects_the_prediction() {
        let mut server = Arena::practice(layout(), 1).unwrap();
        let mut predictor = Predictor::default();
        predictor.reconcile(&layout(), 0, server.fighters().to_vec(), None, 0, 0);
        let (seq, input) = walk(1);
        predictor.apply(seq, input).unwrap();
        // The server never received it: one tick idle instead of a step.
        server_tick(&mut server, None);
        let moved = predictor
            .reconcile(&layout(), 1, server.fighters().to_vec(), None, 0, 0)
            .unwrap();
        assert_eq!(moved, 0, "still pending, so replayed: no change");
        // When the server reports seq 1 applied with the fighter standing
        // still, the prediction takes the server's word.
        let moved = predictor
            .reconcile(&layout(), 1, server.fighters().to_vec(), None, 0, 1)
            .unwrap();
        assert_eq!(moved, arena_sim::arena::MOVE_PER_TICK as u64);
        assert_eq!(predictor.corrections(), 1);
        assert_eq!(predictor.predicted(), Some(server.fighters()[0]));
    }

    #[test]
    fn inputs_in_flight_when_the_match_starts_are_predicted() {
        let mut server = Arena::practice(layout(), 1).unwrap();
        let mut predictor = Predictor::default();
        // Waiting: inputs 1-3 are sent; the server discards 1 and 2 (acked)
        // and then starts, applying 3, which was still in flight.
        for seq in 1..=3 {
            predictor.apply(seq, walk(seq).1);
        }
        predictor.acknowledge(2);
        assert_eq!(
            predictor.predicted(),
            None,
            "nothing predicted while waiting"
        );
        assert_eq!(predictor.pending(), 1);
        predictor.reconcile(&layout(), 0, server.fighters().to_vec(), None, 0, 2);
        let predicted = predictor.predicted().unwrap();
        server_tick(&mut server, Some(walk(3).1));
        let moved = predictor.reconcile(&layout(), 1, server.fighters().to_vec(), None, 0, 3);
        assert_eq!(moved, Some(0));
        assert_eq!(predictor.predicted(), Some(predicted));
        assert_eq!(predictor.corrections(), 0);
    }

    #[test]
    fn a_finished_round_and_other_seats_are_respected() {
        let server = Arena::practice(layout(), 2).unwrap();
        let mut predictor = Predictor::default();
        // Seat 3 predicts its own fighter.
        predictor.reconcile(&layout(), 0, server.fighters().to_vec(), None, 3, 0);
        let moved = predictor.apply(1, walk(1).1).unwrap();
        assert_ne!(moved.position, server.fighters()[3].position);
        // After the result nothing moves.
        let mut over = Predictor::default();
        over.reconcile(
            &layout(),
            0,
            server.fighters().to_vec(),
            Some(RoundResult::Draw),
            0,
            0,
        );
        assert_eq!(over.apply(1, walk(1).1), Some(server.fighters()[0]));
        // A seat outside the roster predicts nothing.
        let mut lost = Predictor::default();
        assert_eq!(
            lost.reconcile(&layout(), 0, server.fighters().to_vec(), None, 9, 0),
            None
        );
        assert_eq!(lost.predicted(), None);
    }
}

#[cfg(test)]
mod pacing_tests {
    use super::*;

    #[test]
    fn pacing_follows_the_lowest_queue_of_the_last_second() {
        let mut pacer = Pacer::default();
        assert_eq!(pacer.factor(), 1.0);
        // Clumped arrivals: the queue swings 0..4. It ran dry, so speed up,
        // even though the average looks generous.
        for queued in [3, 2, 1, 0, 4, 3, 2, 1, 0, 4] {
            pacer.observe(queued);
        }
        assert!(pacer.average().unwrap() > 1.0);
        assert_eq!(pacer.minimum(), Some(0));
        assert_eq!(pacer.factor(), 1.0 - MAX_PACE);
        // A swing of 1..5 keeps one in hand: steady.
        for queued in (0..PACE_WINDOW).map(|i| 1 + (i % 5) as u8) {
            pacer.observe(queued);
        }
        assert_eq!(pacer.factor(), 1.0);
        // Never below 3: two to spare, so slow down.
        for _ in 0..PACE_WINDOW {
            pacer.observe(3);
        }
        assert_eq!(pacer.factor(), 1.0 + MAX_PACE);
        pacer.reset();
        assert_eq!(pacer.minimum(), None);
    }
}

#[cfg(test)]
mod tick_rate_tests {
    use super::*;

    #[test]
    fn the_server_rate_is_measured_on_the_local_clock() {
        let mut rate = TickRate::default();
        assert_eq!(rate.rate(), NOMINAL_TICK_RATE);
        // A server whose clock runs 9% slow: its 30 ticks per its second
        // arrive as about 27.5 per local second, with arrival jitter.
        let local_per_tick = 1.0 / 30.0 * 1.09;
        for tick in 0..300u64 {
            let jitter = if tick % 3 == 0 { 0.004 } else { -0.002 };
            rate.observe(tick as f64 * local_per_tick + jitter, tick);
        }
        let measured = rate.measured().unwrap();
        assert!((measured - 30.0 / 1.09).abs() < 0.1, "{measured}");
        // A rewound tick (new round) starts over at the nominal rate.
        rate.observe(20.0, 3);
        assert_eq!(rate.measured(), None);
        assert_eq!(rate.rate(), NOMINAL_TICK_RATE);
    }
}
