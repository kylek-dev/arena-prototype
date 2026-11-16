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

/// Inputs a client aims to keep queued on the server: one rides out a
/// late arrival without an idle tick, and an early one without a backlog.
pub const QUEUE_TARGET: f64 = 1.0;
/// Most a client stretches or shrinks its input interval to steer the queue.
pub const MAX_PACE: f64 = 0.04;

/// Steers how fast a client sends inputs so the server's queue for it stays
/// near [`QUEUE_TARGET`]. Sending exactly at the server's rate lets the
/// queue drift with timing jitter; when it grows past the server's limit
/// inputs are folded together and a predicted step is lost, and when it
/// empties ticks go idle.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pacer {
    average: Option<f64>,
}
impl Pacer {
    /// Records the queue length a snapshot reported.
    pub fn observe(&mut self, queued: u8) {
        let sample = f64::from(queued);
        // About half a second of snapshots at 30 Hz.
        self.average = Some(
            self.average
                .map_or(sample, |avg| avg + (sample - avg) / 15.0),
        );
    }
    pub fn average(&self) -> Option<f64> {
        self.average
    }
    /// Multiplier for the input interval: above 1 sends slower (the queue
    /// is long), below 1 faster (it keeps running dry).
    pub fn factor(&self) -> f64 {
        self.average.map_or(1.0, |avg| {
            1.0 + MAX_PACE * (avg - QUEUE_TARGET).clamp(-1.0, 1.0)
        })
    }
    pub fn reset(&mut self) {
        self.average = None;
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
