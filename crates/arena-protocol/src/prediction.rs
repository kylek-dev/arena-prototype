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

use arena_sim::arena::{ActorInput, Arena, Combatant, MAX_ROUND_TICKS, RoundResult};
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
/// Ticks per bucket (half a server second): each keeps its fastest snapshot.
const RATE_BUCKET_TICKS: u64 = arena_sim::TICKS_PER_SECOND / 2;
/// Buckets a tick-rate estimate looks back over: twenty server seconds.
pub const RATE_WINDOW_BUCKETS: usize = 40;
/// Snapshots one frame may receive and still count as timing evidence.
pub const FRESH_PER_FRAME: usize = 2;
/// Snapshots arriving more than this much later than the fastest ones are
/// treated as delayed (a backlog), not as evidence about the clock.
const LATE: f64 = 0.02;
/// The estimate stays within this fraction of the nominal rate. Real clocks
/// differ by parts per million; a misbehaving virtual machine was measured
/// 9% off.
pub const MAX_SKEW: f64 = 0.15;

/// Default/initial remote-fighter interpolation delay in ticks (100 ms at 30 Hz).
pub const DEFAULT_INTERP_DELAY_TICKS: f64 = 3.0;
/// Lower bound for adaptive remote interpolation delay (~41.7 ms at 30 Hz):
/// one full tick span plus a quarter-tick guard so clean links interpolate
/// smoothly without underrunning while cutting ~58 ms of unnecessary staleness.
pub const MIN_INTERP_DELAY_TICKS: f64 = 1.25;
/// Upper bound for adaptive remote interpolation delay (~166.7 ms at 30 Hz).
pub const MAX_INTERP_DELAY_TICKS: f64 = 5.0;
/// Sliding window of fresh snapshot arrivals inspected for jitter (~2 s at 30 Hz).
pub const INTERP_WINDOW: usize = 60;
/// Minimum fresh samples (~0.5 s) required before relaxing below the initial delay.
const INTERP_MIN_SAMPLES: usize = 15;
/// Headroom multiplier over observed 2 s arrival spread.
const INTERP_HEADROOM: f64 = 1.15;
/// Fast upward adaptation factor when jitter or clumping spikes.
const INTERP_RISE_ALPHA: f64 = 0.5;
/// Smooth downward decay per fresh frame (~0.75 ticks/s at 30 Hz) when links calm down.
const INTERP_DECAY_PER_FRAME: f64 = 0.025;
/// Delay bump applied at most once per distinct stalled `newest_tick` on view underrun.
const INTERP_UNDERRUN_STEP: f64 = 0.5;

/// Adaptive remote-fighter interpolation delay estimator (ADR 0049).
///
/// Tracks the arrival offset spread `max(now - tick / rate) - min(now - tick / rate)`
/// across a two-second sliding window of fresh (non-backlog) snapshots. Storing
/// `(tick, now)` pairs makes the spread invariant to [`TickRate`] convergence.
/// Delay expands quickly on jitter/clumping/loss stalls or view underruns and
/// relaxes slowly back down to [`MIN_INTERP_DELAY_TICKS`] on steady links.
#[derive(Clone, Debug)]
pub struct InterpDelay {
    samples: VecDeque<(u64, f64)>,
    last_tick: Option<u64>,
    last_underrun_tick: Option<u64>,
    delay: f64,
}
impl Default for InterpDelay {
    fn default() -> Self {
        Self {
            samples: VecDeque::new(),
            last_tick: None,
            last_underrun_tick: None,
            delay: DEFAULT_INTERP_DELAY_TICKS,
        }
    }
}
impl InterpDelay {
    /// Current target interpolation delay in server ticks.
    pub fn delay_ticks(&self) -> f64 {
        self.delay
    }

    /// Records the running snapshot ticks delivered in one frame at local `now`
    /// under estimated server `rate` (ticks/s). Frames carrying more than
    /// [`FRESH_PER_FRAME`] snapshots are treated as backlog bursts (startup or
    /// background tab catch-up) and ignored as jitter evidence.
    pub fn observe_frame(&mut self, now: f64, ticks: impl IntoIterator<Item = u64>, rate: f64) {
        if !now.is_finite() || !rate.is_finite() || rate <= 0.0 {
            return;
        }
        let mut fresh = [0u64; FRESH_PER_FRAME];
        let mut count = 0usize;
        let mut newest = None;
        for tick in ticks {
            if count < FRESH_PER_FRAME {
                fresh[count] = tick;
            }
            count += 1;
            newest = Some(newest.map_or(tick, |n: u64| n.max(tick)));
        }
        let Some(newest) = newest else {
            return;
        };
        if count > FRESH_PER_FRAME {
            if self.last_tick.is_some_and(|last| newest < last) {
                self.samples.clear();
                self.last_underrun_tick = None;
            }
            self.last_tick = Some(newest);
            return;
        }
        if self.last_tick.is_some_and(|last| newest < last) {
            self.samples.clear();
            self.last_underrun_tick = None;
            self.last_tick = None;
        }
        let mut recorded = false;
        for &tick in &fresh[..count] {
            if self.last_tick.is_none_or(|last| tick > last) {
                self.samples.push_back((tick, now));
                if self.samples.len() > INTERP_WINDOW {
                    self.samples.pop_front();
                }
                self.last_tick = Some(tick);
                recorded = true;
            }
        }
        if !recorded || self.samples.len() < INTERP_MIN_SAMPLES {
            return;
        }
        let mut min_offset = f64::INFINITY;
        let mut max_offset = f64::NEG_INFINITY;
        for &(tick, sample_now) in &self.samples {
            let offset = sample_now - tick as f64 / rate;
            if !offset.is_finite() {
                return;
            }
            min_offset = min_offset.min(offset);
            max_offset = max_offset.max(offset);
        }
        let spread_ticks = (max_offset - min_offset).max(0.0) * rate;
        if !spread_ticks.is_finite() {
            return;
        }
        let target = (MIN_INTERP_DELAY_TICKS + spread_ticks * INTERP_HEADROOM)
            .clamp(MIN_INTERP_DELAY_TICKS, MAX_INTERP_DELAY_TICKS);
        if target > self.delay {
            self.delay = (self.delay + (target - self.delay) * INTERP_RISE_ALPHA)
                .min(MAX_INTERP_DELAY_TICKS);
        } else {
            self.delay = (self.delay - INTERP_DECAY_PER_FRAME).max(target);
        }
    }

    /// Records a view buffer underrun while waiting past `newest_tick`.
    /// Ignored while waiting (`newest_tick == 0`) and deduplicated per distinct
    /// `newest_tick` so high-refresh renders during one packet stall bump the
    /// buffer at most once.
    pub fn observe_underrun(&mut self, newest_tick: u64) {
        if newest_tick == 0 {
            return;
        }
        if self.last_underrun_tick != Some(newest_tick) {
            self.last_underrun_tick = Some(newest_tick);
            self.delay = (self.delay + INTERP_UNDERRUN_STEP).min(MAX_INTERP_DELAY_TICKS);
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Measures how fast the server's ticks advance on the client's own clock.
///
/// A client must send one input per server tick, and its interpolated view
/// must advance at the server's pace. Both are driven by the server's rate
/// as this client's clock sees it: clocks drift, and a virtual machine's
/// clock was once measured 9% slow, which made a client sending a nominal
/// 30 inputs per second flood its server.
///
/// Delivery delay must not be mistaken for clock rate. After a stall (a
/// background tab) snapshots trickle in late for seconds, and ticks seem to
/// advance slowly; an estimate from first and last samples once dropped to
/// a third of the true rate, and the player moved in slow motion. Delay can
/// only make a snapshot later, never earlier, so the clock is the *lower
/// edge* of the samples. As network time protocols do, the estimate keeps
/// the fastest-arriving snapshot of each half server second and fits a line
/// through those, repeatedly discarding points well above the line.
#[derive(Clone, Debug, Default)]
pub struct TickRate {
    /// Per half server second (bucket of ticks): the tick and lowest offset seen,
    /// where offset = local time − tick ÷ nominal rate.
    buckets: VecDeque<(u64, u64, f64)>,
    last_tick: Option<u64>,
    /// The latest good estimate, kept across rewinds and short windows.
    held: Option<f64>,
}
impl TickRate {
    /// Records the snapshots one rendered frame received at local `now`.
    ///
    /// Several in one frame are a backlog (a page still loading, a tab back
    /// from the background), not timing evidence: they say nothing about
    /// when each was sent. Only a frame with at most [`FRESH_PER_FRAME`]
    /// counts, through its newest snapshot. Without this, a page that
    /// loaded behind a backlog fitted its first line through samples all
    /// received at one instant, read an impossibly fast server, and flooded
    /// it until inputs were folded.
    pub fn observe_frame(&mut self, now: f64, ticks: impl IntoIterator<Item = u64>) {
        let (count, newest) =
            ticks
                .into_iter()
                .fold((0, None), |(count, newest): (usize, Option<u64>), tick| {
                    (count + 1, Some(newest.map_or(tick, |n| n.max(tick))))
                });
        let Some(newest) = newest else {
            return;
        };
        if count > FRESH_PER_FRAME {
            if self.last_tick.is_some_and(|last| newest < last) {
                self.buckets.clear();
            }
            self.last_tick = Some(newest);
            return;
        }
        self.observe(now, newest);
    }

    /// Records that a snapshot for server `tick` arrived at local `now`.
    pub fn observe(&mut self, now: f64, tick: u64) {
        // A new round or a reconnect rewinds the tick: start a new line,
        // keeping the last estimate meanwhile.
        if self.last_tick.is_some_and(|last| tick < last) {
            self.buckets.clear();
        }
        self.last_tick = Some(tick);
        let offset = now - tick as f64 / NOMINAL_TICK_RATE;
        let bucket = tick / RATE_BUCKET_TICKS;
        match self.buckets.back_mut() {
            Some((at, best_tick, best)) if *at == bucket => {
                if offset < *best {
                    (*best_tick, *best) = (tick, offset);
                }
            }
            _ => self.buckets.push_back((bucket, tick, offset)),
        }
        while self.buckets.len() > RATE_WINDOW_BUCKETS {
            self.buckets.pop_front();
        }
        if let Some(rate) = self.fit() {
            self.held = Some(rate);
        }
    }

    /// Fits the lower edge of the bucket minima: least squares, then drop
    /// points more than `LATE` above the line and refit, a few times.
    fn fit(&self) -> Option<f64> {
        let mut points = [(0.0_f64, 0.0_f64); RATE_WINDOW_BUCKETS];
        let mut len = 0usize;
        for &(_, tick, offset) in &self.buckets {
            if len < RATE_WINDOW_BUCKETS {
                points[len] = (tick as f64, offset);
                len += 1;
            }
        }
        let mut slope = 0.0;
        for _ in 0..4 {
            if len < 3 {
                return None;
            }
            let slice = &points[..len];
            let n = len as f64;
            let (first, last) = (slice[0], slice[len - 1]);
            // A first estimate after about a second keeps a skewed client
            // from flooding the server while it waits for one; it must span
            // a second of server ticks and of local time.
            let local = |(tick, offset): (f64, f64)| offset + tick / NOMINAL_TICK_RATE;
            if last.0 - first.0 < NOMINAL_TICK_RATE || local(last) - local(first) < 1.0 {
                return None;
            }
            let (mx, my) = slice
                .iter()
                .fold((0.0, 0.0), |(x, y), (px, py)| (x + px / n, y + py / n));
            let (sxy, sxx) = slice.iter().fold((0.0, 0.0), |(sxy, sxx), (px, py)| {
                (sxy + (px - mx) * (py - my), sxx + (px - mx) * (px - mx))
            });
            slope = sxy / sxx;
            let intercept = my - slope * mx;
            // The lowest residual marks the lower edge; keep points near it.
            let lowest = slice
                .iter()
                .map(|(x, y)| y - (intercept + slope * x))
                .fold(f64::INFINITY, f64::min);
            let before = len;
            let mut kept = 0usize;
            for i in 0..len {
                let (x, y) = points[i];
                if y - (intercept + slope * x) - lowest <= LATE {
                    points[kept] = (x, y);
                    kept += 1;
                }
            }
            len = kept;
            if len == before {
                break;
            }
        }
        // offset = n (1/R − 1/R₀) + c, so the slope gives R.
        let rate = 1.0 / (slope + 1.0 / NOMINAL_TICK_RATE);
        let (low, high) = (
            NOMINAL_TICK_RATE * (1.0 - MAX_SKEW),
            NOMINAL_TICK_RATE * (1.0 + MAX_SKEW),
        );
        rate.is_finite().then(|| rate.clamp(low, high))
    }

    /// Server ticks per local second, once a few seconds have been seen.
    pub fn measured(&self) -> Option<f64> {
        self.held
    }
    /// The measured rate, or the nominal one until there is a measurement.
    pub fn rate(&self) -> f64 {
        self.held.unwrap_or(NOMINAL_TICK_RATE)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Debug, Default)]
pub struct Predictor {
    /// The server's latest state plus every unacknowledged input.
    arena: Option<Arena>,
    actor: usize,
    round_ticks: Option<u64>,
    pending: VecDeque<(u32, ActorInput)>,
    corrections: u64,
}

impl Predictor {
    /// Configures the authoritative round limit announced by `Welcome`.
    /// Ignores invalid values outside `1..=MAX_ROUND_TICKS`.
    pub fn set_round_limit(&mut self, round_ticks: u64) {
        if round_ticks > 0 && round_ticks <= MAX_ROUND_TICKS {
            self.round_ticks = Some(round_ticks);
            if let Some(arena) = self.arena.as_mut() {
                arena.set_round_limit(round_ticks);
            }
        }
    }
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
    /// will. Combat actions (casts and cancellations) are never predicted:
    /// the server stays sole authority for casts, cooldowns, resources and
    /// interrupts. Returns the new predicted state, or `None` before the first
    /// reconcile.
    pub fn apply(&mut self, seq: u32, input: ActorInput) -> Option<Combatant> {
        let input = ActorInput {
            action: None,
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
    /// replays the inputs it has not applied yet. Active casts are stripped
    /// from the local prediction arena so timed cast completions, damage,
    /// kills, and round endings are never client-predicted (ADR 0048).
    /// Returns how far the prediction moved, in millimetres, or `None` if
    /// there was none yet.
    pub fn reconcile(
        &mut self,
        layout: &Layout,
        tick: u64,
        mut fighters: Vec<Combatant>,
        result: Option<RoundResult>,
        actor: usize,
        ack: u32,
    ) -> Option<u64> {
        while self.pending.front().is_some_and(|(seq, _)| *seq <= ack) {
            self.pending.pop_front();
        }
        let before = self.predicted();
        let valid =
            (2..=crate::MAX_FIGHTERS).contains(&fighters.len()) && fighters.len().is_multiple_of(2);
        if !valid {
            return None;
        }
        if actor >= fighters.len() {
            self.arena = None;
            return None;
        }
        // Never predict timed cast completions or phantom kills while replaying movement.
        for fighter in &mut fighters {
            fighter.active_cast = None;
        }
        let round_ticks = self.round_ticks.unwrap_or(MAX_ROUND_TICKS);
        let mut arena = match self.arena.take() {
            Some(mut existing) if existing.layout() == layout => {
                if !existing.restore_in_place(tick, &fighters, result)
                    || !existing.set_round_limit(round_ticks)
                {
                    return None;
                }
                existing
            }
            _ => Arena::restore(layout.clone(), tick, fighters, result)?
                .with_round_limit(round_ticks)?,
        };
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
    let mut slots = [ActorInput::default(); crate::MAX_FIGHTERS];
    if let Some(slot) = slots.get_mut(actor) {
        *slot = input;
        arena.advance(&slots[..=actor]);
    } else {
        let mut slots = vec![ActorInput::default(); actor + 1];
        slots[actor] = input;
        arena.advance(&slots);
    }
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
                action: ActorInput::pulse(1),
                rewind_ticks: 0,
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
        // After the result nothing moves (for both mutual-KO and time-limit draws).
        for result in [RoundResult::Draw, RoundResult::TimeLimitDraw] {
            let mut over = Predictor::default();
            over.reconcile(&layout(), 0, server.fighters().to_vec(), Some(result), 0, 0);
            assert_eq!(over.apply(1, walk(1).1), Some(server.fighters()[0]));
        }
        // A seat outside the roster predicts nothing.
        let mut lost = Predictor::default();
        assert_eq!(
            lost.reconcile(&layout(), 0, server.fighters().to_vec(), None, 9, 0),
            None
        );
        assert_eq!(lost.predicted(), None);
    }

    #[test]
    fn prediction_respects_custom_round_limit_across_time_expiry() {
        let mut server = Arena::practice(layout(), 1)
            .unwrap()
            .with_round_limit(4)
            .unwrap();
        let mut predictor = Predictor::default();
        predictor.set_round_limit(4);
        predictor.reconcile(&layout(), 0, server.fighters().to_vec(), None, 0, 0);
        // Send 6 movement inputs; only the first 4 ticks happen before TimeLimitDraw.
        let mut sent = Vec::new();
        for seq in 1..=6 {
            let (_, input) = walk(seq);
            predictor.apply(seq, input);
            sent.push(input);
        }
        let expected_after_4 = {
            let mut replay = server.clone();
            for input in sent.iter().take(4) {
                replay.advance(&[*input]);
            }
            assert_eq!(replay.result(), Some(RoundResult::TimeLimitDraw));
            replay.fighters()[0]
        };
        assert_eq!(predictor.predicted(), Some(expected_after_4));
        // Reconciling at tick 3 (one tick left) replays the remaining pending inputs
        // and still stops cleanly at tick 4 with zero corrections.
        for input in sent.iter().take(3) {
            server.advance(&[*input]);
        }
        let moved = predictor.reconcile(
            &layout(),
            server.tick(),
            server.fighters().to_vec(),
            server.result(),
            0,
            3,
        );
        assert_eq!(moved, Some(0));
        assert_eq!(predictor.corrections(), 0);
        assert_eq!(predictor.predicted(), Some(expected_after_4));
    }

    #[test]
    fn prediction_never_completes_active_casts_or_freezes_movement_on_phantom_kills() {
        use arena_sim::arena::{ActiveCastState, Position};
        use arena_sim::spells::SpellId;

        let mut fighters = Arena::practice(layout(), 1).unwrap().fighters().to_vec();
        fighters[0].position = Position { x: 0, z: 0 };
        fighters[1].position = Position { x: 0, z: -3_000 };
        // Both fighters are one tick away from lethal Rift Bolt completions at 30 HP.
        fighters[0].health = 30;
        fighters[1].health = 30;
        fighters[0].active_cast = Some(ActiveCastState {
            spell: SpellId::RiftBolt,
            target: Some(1),
            remaining_ticks: 1,
            total_ticks: 45,
        });
        fighters[1].active_cast = Some(ActiveCastState {
            spell: SpellId::RiftBolt,
            target: Some(0),
            remaining_ticks: 1,
            total_ticks: 45,
        });

        let mut predictor = Predictor::default();
        predictor.reconcile(&layout(), 10, fighters.clone(), None, 0, 0);
        // Replaying 3 movement inputs must keep moving smoothly without predicting
        // either fighter's lethal cast completion.
        for seq in 1..=3 {
            predictor.apply(seq, walk(seq).1);
        }
        let predicted = predictor.predicted().unwrap();
        assert_eq!(predicted.health, 30);
        assert_eq!(predicted.active_cast, None);
        assert_eq!(
            predicted.position.z,
            -3 * arena_sim::arena::MOVE_PER_TICK,
            "movement prediction must not freeze on unconfirmed casts"
        );
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

    /// Snapshot arrival times for a server whose clock runs `skew` slow
    /// against ours: tick `n` reaches us at `n × local_per_tick + delay(n)`.
    fn feed(
        rate: &mut TickRate,
        ticks: std::ops::Range<u64>,
        skew: f64,
        delay: impl Fn(u64) -> f64,
    ) {
        let local_per_tick = (1.0 + skew) / NOMINAL_TICK_RATE;
        for tick in ticks {
            let jitter = [0.004, 0.0, 0.009, 0.002][tick as usize % 4];
            rate.observe(tick as f64 * local_per_tick + jitter + delay(tick), tick);
        }
    }

    #[test]
    fn the_server_rate_is_measured_on_the_local_clock() {
        let mut rate = TickRate::default();
        assert_eq!(rate.rate(), NOMINAL_TICK_RATE);
        // A server whose clock runs 9% slow (its 30 Hz arrives as about 27.5
        // per local second), with arrival jitter.
        feed(&mut rate, 0..90, 0.09, |_| 0.0);
        let truth = NOMINAL_TICK_RATE / 1.09;
        assert!((rate.rate() - truth).abs() < 0.1, "{}", rate.rate());
        feed(&mut rate, 90..600, 0.09, |_| 0.0);
        assert!((rate.rate() - truth).abs() < 0.02, "{}", rate.rate());
    }

    #[test]
    fn a_backlog_after_a_background_tab_is_not_mistaken_for_a_slow_server() {
        let mut rate = TickRate::default();
        let truth = NOMINAL_TICK_RATE / 1.09;
        feed(&mut rate, 0..300, 0.09, |_| 0.0);
        // Hidden for ten seconds, then the backlog trickles in: snapshots
        // from the hidden stretch arrive late, the delay shrinking as the
        // stream catches up over the next six seconds.
        let hidden = 300..600;
        let catching_up = |tick: u64| {
            if hidden.contains(&tick) {
                // Delivered between 10 s and 16 s after the stall began.
                10.0 - (tick - 300) as f64 / NOMINAL_TICK_RATE + (tick - 300) as f64 * 0.02
            } else if tick < 800 {
                // Still behind while the queue drains.
                6.0 * (800 - tick) as f64 / 200.0
            } else {
                0.0
            }
        };
        let local_per_tick = 1.09 / NOMINAL_TICK_RATE;
        let mut worst: f64 = 0.0;
        for tick in 300..1200u64 {
            rate.observe(tick as f64 * local_per_tick + catching_up(tick), tick);
            worst = worst.max((rate.rate() - truth).abs());
        }
        assert!(worst < 0.3, "the estimate wandered by {worst}");
    }

    #[test]
    fn a_backlog_received_in_one_frame_is_not_timing_evidence() {
        let truth = NOMINAL_TICK_RATE / 1.09;
        let local_per_tick = 1.09 / NOMINAL_TICK_RATE;
        let mut rate = TickRate::default();
        // A page that loaded three seconds into the match: its first frame
        // receives 90 queued snapshots at once.
        rate.observe_frame(3.0 * 1.09, 0..90);
        assert_eq!(rate.measured(), None);
        // Then one snapshot per frame or so: a first estimate after about a
        // second, and it is right.
        for tick in 90..150u64 {
            rate.observe_frame(tick as f64 * local_per_tick, [tick]);
        }
        let first = rate.measured().expect("an estimate after two seconds");
        assert!((first - truth).abs() < 0.3, "{first}");
        // Two in a frame still count, through the newer.
        rate.observe_frame(150.0 * local_per_tick + 0.034, [150, 151]);
        assert!((rate.rate() - truth).abs() < 0.3);
        // Fed one by one, the same backlog reads as an impossibly fast server.
        let mut naive = TickRate::default();
        for tick in 0..90 {
            naive.observe(3.0 * 1.09, tick);
        }
        for tick in 90..150u64 {
            naive.observe(tick as f64 * local_per_tick, tick);
        }
        assert!(naive.rate() > truth + 2.0, "{}", naive.rate());
    }

    #[test]
    fn estimates_stay_near_nominal_survive_rewinds_and_reset() {
        let mut rate = TickRate::default();
        // An absurdly fast clock is clamped.
        feed(&mut rate, 0..300, -0.5, |_| 0.0);
        assert_eq!(rate.rate(), NOMINAL_TICK_RATE * (1.0 + MAX_SKEW));
        // A rewind (new round) keeps the last estimate while relearning.
        let mut rate = TickRate::default();
        feed(&mut rate, 0..300, 0.09, |_| 0.0);
        let before = rate.rate();
        rate.observe(100.0, 3);
        assert_eq!(rate.rate(), before);
        rate.reset();
        assert_eq!(rate.measured(), None);
        assert_eq!(rate.rate(), NOMINAL_TICK_RATE);
    }
}

#[cfg(test)]
mod interp_delay_tests {
    use super::*;

    #[test]
    fn clean_link_relaxes_smoothly_to_minimum_delay() {
        let mut interp = InterpDelay::default();
        assert_eq!(interp.delay_ticks(), DEFAULT_INTERP_DELAY_TICKS);
        // First 14 clean frames keep the safe default while gathering baseline.
        for tick in 1..=14u64 {
            interp.observe_frame(tick as f64 / NOMINAL_TICK_RATE, [tick], NOMINAL_TICK_RATE);
        }
        assert_eq!(interp.delay_ticks(), DEFAULT_INTERP_DELAY_TICKS);
        // After ~3 seconds of steady 30 Hz arrivals, delay settles to MIN_INTERP_DELAY_TICKS.
        for tick in 15..=120u64 {
            interp.observe_frame(tick as f64 / NOMINAL_TICK_RATE, [tick], NOMINAL_TICK_RATE);
        }
        assert!(
            (interp.delay_ticks() - MIN_INTERP_DELAY_TICKS).abs() < 1e-6,
            "got {}",
            interp.delay_ticks()
        );
    }

    #[test]
    fn jitter_clumping_and_underruns_expand_delay_within_bounds() {
        let mut interp = InterpDelay::default();
        // Settle on a clean link first.
        for tick in 1..=120u64 {
            interp.observe_frame(tick as f64 / NOMINAL_TICK_RATE, [tick], NOMINAL_TICK_RATE);
        }
        assert!((interp.delay_ticks() - MIN_INTERP_DELAY_TICKS).abs() < 1e-6);

        // Introduce 40 ms clumping / jitter bursts: delay expands rapidly above 2.3 ticks.
        for tick in 121..=150u64 {
            let jitter_s = if tick % 2 == 0 { 0.040 } else { 0.0 };
            interp.observe_frame(
                tick as f64 / NOMINAL_TICK_RATE + jitter_s,
                [tick],
                NOMINAL_TICK_RATE,
            );
        }
        assert!(
            interp.delay_ticks() > 2.3 && interp.delay_ticks() <= MAX_INTERP_DELAY_TICKS,
            "expected expanded delay, got {}",
            interp.delay_ticks()
        );

        // Repeated underruns on the SAME newest_tick only bump once (framerate-independent).
        let before = interp.delay_ticks();
        for _ in 0..10 {
            interp.observe_underrun(150);
        }
        assert!(
            (interp.delay_ticks() - (before + INTERP_UNDERRUN_STEP).min(MAX_INTERP_DELAY_TICKS))
                .abs()
                < 1e-6
        );

        // Subsequent severe stalls clamp at MAX_INTERP_DELAY_TICKS.
        for tick in 151..=170u64 {
            interp.observe_underrun(tick);
        }
        assert_eq!(interp.delay_ticks(), MAX_INTERP_DELAY_TICKS);
    }

    #[test]
    fn backlog_bursts_rewinds_and_skewed_clocks_are_handled_cleanly() {
        let mut interp = InterpDelay::default();
        // Startup backlog of 30 snapshots in one frame does not distort delay.
        interp.observe_frame(1.0, 1..=30, NOMINAL_TICK_RATE);
        assert_eq!(interp.delay_ticks(), DEFAULT_INTERP_DELAY_TICKS);

        // Server running 9% slow: once TickRate supplies the true rate, offset spread
        // is zero and delay relaxes cleanly to MIN_INTERP_DELAY_TICKS.
        let true_rate = NOMINAL_TICK_RATE / 1.09;
        let local_per_tick = 1.09 / NOMINAL_TICK_RATE;
        for tick in 31..=150u64 {
            interp.observe_frame(tick as f64 * local_per_tick, [tick], true_rate);
        }
        assert!(
            (interp.delay_ticks() - MIN_INTERP_DELAY_TICKS).abs() < 1e-6,
            "got {}",
            interp.delay_ticks()
        );

        // Non-finite or subnormal-overflow inputs and waiting-tick underruns are ignored safely.
        interp.observe_frame(f64::NAN, [151], true_rate);
        interp.observe_frame(10.0, [151], 0.0);
        interp.observe_frame(10.0, [152], f64::MIN_POSITIVE);
        let before_wait_underrun = interp.delay_ticks();
        interp.observe_underrun(0);
        assert_eq!(interp.delay_ticks(), before_wait_underrun);
        assert!(interp.delay_ticks().is_finite());

        // Rewind (new round) keeps the current delay while rebuilding its sample window.
        let held = interp.delay_ticks();
        interp.observe_frame(20.0, [1], true_rate);
        assert_eq!(interp.delay_ticks(), held);

        interp.reset();
        assert_eq!(interp.delay_ticks(), DEFAULT_INTERP_DELAY_TICKS);
    }
}
