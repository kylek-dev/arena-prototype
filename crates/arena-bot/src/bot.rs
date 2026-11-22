//! One bot seat: connects with its ticket, plays through ordinary inputs
//! only (it has no other power than a browser would), and measures what a
//! player would experience. Every byte in both directions passes through a
//! simulated [`Link`].

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use arena_headless::telemetry::CombatMetrics;
use arena_protocol::prediction::{InterpDelay, Pacer, Predictor, TickRate};
use arena_protocol::{
    ClientMessage, ErrorCode, FighterState, InputFrame, MAX_FIGHTERS, PROTOCOL_VERSION,
    ServerMessage, Snapshot, WireAction, WireEvent, WireSpell,
};
use arena_sim::arena::{ActorInput, CastAction, Combatant, MAX_ROUND_TICKS, Position};
use arena_sim::map::Layout;
use futures_util::{SinkExt, StreamExt};
use tokio::time::Instant;
use tokio_tungstenite::connect_async_with_config;
use tokio_tungstenite::tungstenite::Message;

use crate::Shared;
use crate::bandwidth::{BandwidthStats, BandwidthTracker};
use crate::fairness::{
    CastSubmission, FairnessStats, FairnessTracker, ViewBuffer, interp_delay_ticks,
};
use crate::link::{Link, Rng, delay_line};
use crate::stats::Samples;

#[derive(Clone)]
pub struct Seat {
    pub server: String,
    pub match_id: u64,
    pub actor: usize,
    pub ticket: Vec<u8>,
    pub link: Link,
    /// Use browser-parity `Pacer` + `TickRate` adaptive input cadence.
    pub paced: bool,
    /// Remote-fighter view delay (ms), matching browser interpolation (default 100 ms).
    pub interp_delay_ms: u64,
    /// Use browser-parity adaptive interpolation delay (`InterpDelay`).
    pub adaptive_interp: bool,
    pub seed: u64,
    /// Drop the connection and rejoin this often (staggered per seat).
    pub reconnect_every: Option<Duration>,
    /// How long a dropped seat stays away before rejoining.
    pub away: Duration,
    /// The match's map, for predicting this seat's own movement.
    pub layout: Arc<Layout>,
    pub shared: Arc<Shared>,
}

#[derive(Debug, Default)]
pub struct SeatStats {
    pub rtt: Samples,
    /// Input queued → first snapshot acknowledging it.
    pub input_latency: Samples,
    /// Gap between consecutive snapshots as the bot receives them.
    pub snapshot_gap: Samples,
    pub snapshots: u64,
    pub bytes_in: u64,
    pub inputs: u64,
    pub errors: HashMap<String, u64>,
    pub unexpected_closes: u64,
    /// Why connections were lost, by reason.
    pub lost: HashMap<&'static str, u64>,
    pub rejoins_attempted: u64,
    pub rejoins_succeeded: u64,
    /// Rejoins answered with `MatchOver`: the round ended while away.
    pub rejoins_too_late: u64,
    /// Snapshots that showed some other fighter as disconnected.
    pub saw_disconnected: u64,
    /// Results that do not match the fighters: a team declared beaten while
    /// one of its fighters still stands. Must stay 0.
    pub wrong_results: u64,
    /// Inputs applied to the predicted fighter, as a browser client does.
    pub predicted_inputs: u64,
    /// Reconciliations that moved the prediction, and by how much (mm).
    pub corrections: u64,
    pub correction_mm: Vec<u32>,
    /// Sampled effective interpolation delay (ms) when `adaptive_interp` is enabled.
    pub interp_delay_ms: Vec<u32>,
    /// Private rejection events received by this seat (e.g. `LineOfSight`, `OutOfRange`).
    pub rejections: CombatMetrics,
    /// Next event tick eligible for per-seat rejection recording (deduplicates reconnects).
    pub(crate) next_rejection_tick: u64,
    /// Targeting-fairness / lag-compensation disagreement counters.
    pub fairness: FairnessStats,
    /// Online full vs delta / rate-reduced snapshot bandwidth counters.
    pub bandwidth: BandwidthStats,
    pub round_finished: bool,
}

struct Leave {
    at: Instant,
    /// A planned drop to rejoin, rather than the end of the run.
    planned: bool,
}

enum End {
    /// Our own planned disconnect, to rejoin.
    Planned,
    /// The run is over.
    Deadline,
    Error(ErrorCode),
    Lost(&'static str),
}

/// Plays until `deadline`, the match ending, or an error.
pub async fn play(seat: Seat, deadline: Instant) -> SeatStats {
    let mut stats = SeatStats::default();
    let mut rng = Rng::new(seat.seed);
    // Stagger planned reconnects so seats do not all leave together.
    let mut next_drop = seat
        .reconnect_every
        .map(|every| Instant::now() + every / 2 + rng.below(every));
    let mut rejoining = false;
    loop {
        let planned = next_drop.filter(|at| *at < deadline);
        let leave_at = planned.unwrap_or(deadline);
        if rejoining {
            stats.rejoins_attempted += 1;
        }
        let leaving = Leave {
            at: leave_at,
            planned: planned.is_some(),
        };
        let outcome = session(&seat, &mut stats, &mut rng, leaving, rejoining).await;
        if rejoining
            && matches!(outcome, End::Error(ErrorCode::MatchOver))
            && stats.rejoins_attempted > stats.rejoins_succeeded + stats.rejoins_too_late
        {
            stats.rejoins_too_late += 1;
        }
        match outcome {
            End::Planned if Instant::now() < deadline => {
                // Stay away a moment so the other players can see it.
                tokio::time::sleep(seat.away).await;
                if Instant::now() >= deadline {
                    break;
                }
                next_drop = seat.reconnect_every.map(|every| Instant::now() + every);
                rejoining = true;
            }
            End::Planned | End::Deadline => break,
            End::Error(ErrorCode::MatchOver) => {
                stats.round_finished = true;
                break;
            }
            End::Error(code) => {
                *stats.errors.entry(format!("{code:?}")).or_default() += 1;
                break;
            }
            End::Lost(reason) => {
                stats.unexpected_closes += 1;
                *stats.lost.entry(reason).or_default() += 1;
                break;
            }
        }
    }
    stats
}

async fn session(
    seat: &Seat,
    stats: &mut SeatStats,
    rng: &mut Rng,
    leave: Leave,
    rejoining: bool,
) -> End {
    // Browsers disable Nagle's algorithm for WebSockets; so do the bots.
    let Ok((socket, _)) =
        connect_async_with_config(format!("ws://{}/play", seat.server), None, true).await
    else {
        return End::Lost("connect failed");
    };
    let (mut sink, mut stream) = socket.split();
    let (outgoing, mut delayed_out) = delay_line::<Vec<u8>>(seat.link, Rng::new(rng.next_u64()));
    let writer = tokio::spawn(async move {
        while let Some(frame) = delayed_out.recv().await {
            if sink.send(Message::binary(frame)).await.is_err() {
                return;
            }
        }
        let _ = sink.close().await;
    });
    let (incoming, mut inbox) = delay_line::<Vec<u8>>(seat.link, Rng::new(rng.next_u64()));
    tokio::spawn(async move {
        while let Some(Ok(message)) = stream.next().await {
            if let Message::Binary(frame) = message
                && !incoming.push(frame.to_vec())
            {
                break;
            }
        }
    });
    let send = |message: ClientMessage| outgoing.push(message.encode());
    send(ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        ticket: seat.ticket.clone(),
    });

    let mut tick_rate = 30u32;
    let mut round_ticks = MAX_ROUND_TICKS;
    let mut view_buffer = ViewBuffer::default();
    let mut fairness = FairnessTracker::default();
    let mut bandwidth = BandwidthTracker::default();
    let mut last_snapshot_at: Option<Instant> = None;
    let mut seq = 0u32;
    let mut pending_cast_ack: Option<u32> = None;
    let mut unacked: VecDeque<(u32, Instant)> = VecDeque::new();
    let mut pings: HashMap<u32, Instant> = HashMap::new();
    let mut nonce = 0u32;
    let mut welcomed = false;
    // Predicts this seat's movement and paces inputs like the browser client does,
    // to measure how real clients behave under latency, jitter, clumps, and loss.
    let mut predictor = Predictor::default();
    let mut pacer = Pacer::default();
    let mut measured_rate = TickRate::default();
    let mut interp_delay = InterpDelay::default();
    let mut last_view_tick = 0u64;
    let session_start = Instant::now();
    // Once the socket refuses writes, stop sending and let the incoming side
    // deliver what the server said last (usually an `Error`), which the
    // simulated delay may still be holding.
    let mut writable = true;
    let mut next_input = Instant::now();
    let mut pinger = tokio::time::interval(Duration::from_secs(1));
    let welcome_deadline = Instant::now() + Duration::from_secs(10);

    let end = loop {
        tokio::select! {
            frame = inbox.recv() => {
                let Some(first) = frame else { break End::Lost("closed without an error") };
                let now = Instant::now();
                let mut batch = vec![first];
                while let Ok(more) = inbox.try_recv() {
                    batch.push(more);
                }
                let mut frame_ticks = Vec::new();
                let mut stop = None;
                for frame in batch {
                    let frame_len = frame.len();
                    stats.bytes_in += frame_len as u64;
                    let Ok(message) = ServerMessage::decode(&frame) else {
                        stop = Some(End::Lost("undecodable frame"));
                        break;
                    };
                    match message {
                        ServerMessage::Welcome(welcome) => {
                            welcomed = true;
                            round_ticks = welcome.round_ticks;
                            predictor.set_round_limit(round_ticks);
                            pacer.reset();
                            measured_rate.reset();
                            interp_delay.reset();
                            last_view_tick = 0;
                            view_buffer.clear();
                            fairness.reset_connection();
                            bandwidth.reset_connection();
                            if rejoining {
                                stats.rejoins_succeeded += 1;
                            }
                            if u32::from(welcome.tick_rate) != tick_rate && welcome.tick_rate > 0 {
                                tick_rate = u32::from(welcome.tick_rate);
                            }
                            next_input = now;
                        }
                        ServerMessage::Snapshot(snapshot) => {
                            stats.snapshots += 1;
                            if let Some(previous) = last_snapshot_at.replace(now) {
                                stats.snapshot_gap.record(now - previous);
                            }
                            // Inputs before the match starts are acknowledged
                            // without acting; measure only a running match.
                            while unacked.front().is_some_and(|(sent, _)| *sent <= snapshot.ack) {
                                let (_, at) = unacked.pop_front().expect("checked front");
                                if snapshot.tick > 0 {
                                    stats.input_latency.record(now - at);
                                }
                            }
                            seat.shared
                                .observe_snapshot(seat.match_id, tick_rate, &snapshot);
                            bandwidth.observe(&mut stats.bandwidth, &snapshot, frame_len);
                            fairness.on_snapshot(
                                &mut stats.fairness,
                                seat.actor,
                                &snapshot,
                                &seat.layout,
                            );
                            if snapshot.tick > 0 {
                                frame_ticks.push(snapshot.tick);
                                pacer.observe(snapshot.queued);
                            }
                            record_seat_rejections(stats, &snapshot.events);
                            if let Some(result) = snapshot.result
                                && (!result_matches(
                                    result,
                                    &snapshot.fighters,
                                    snapshot.tick,
                                    round_ticks,
                                ) || snapshot
                                    .scoreboard
                                    .as_ref()
                                    .is_none_or(|scores| scores.len() != snapshot.fighters.len()))
                            {
                                stats.wrong_results += 1;
                            }
                            if snapshot.result.is_some() {
                                predictor.reset();
                            } else if snapshot.tick == 0 {
                                // Waiting: discarded inputs are acknowledged;
                                // ones in flight may act once the match starts.
                                predictor.acknowledge(snapshot.ack);
                            } else {
                                let fighters =
                                    snapshot.fighters.iter().map(FighterState::combatant).collect();
                                if let Some(moved) = predictor.reconcile(
                                    &seat.layout,
                                    snapshot.tick,
                                    fighters,
                                    None,
                                    seat.actor,
                                    snapshot.ack,
                                ) && moved > 0
                                {
                                    stats.corrections += 1;
                                    stats.correction_mm.push(u32::try_from(moved).unwrap_or(u32::MAX));
                                }
                            }
                            if snapshot
                                .fighters
                                .iter()
                                .enumerate()
                                .any(|(index, fighter)| index != seat.actor && !fighter.connected)
                            {
                                stats.saw_disconnected += 1;
                            }
                            if pending_cast_ack.is_some_and(|pending| snapshot.ack >= pending) {
                                pending_cast_ack = None;
                            }
                            view_buffer.push(snapshot);
                        }
                        ServerMessage::Pong { nonce } => {
                            if let Some(sent) = pings.remove(&nonce) {
                                stats.rtt.record(now - sent);
                            }
                        }
                        ServerMessage::Error(code) => {
                            stop = Some(End::Error(code));
                            break;
                        }
                    }
                }
                if !frame_ticks.is_empty() {
                    let now_s = now.saturating_duration_since(session_start).as_secs_f64();
                    measured_rate.observe_frame(now_s, frame_ticks.iter().copied());
                    interp_delay.observe_frame(now_s, frame_ticks, measured_rate.rate());
                }
                if let Some(end) = stop {
                    break end;
                }
            }
            _ = tokio::time::sleep_until(next_input), if welcomed && writable => {
                let step_secs = if seat.paced {
                    (pacer.factor() / measured_rate.rate()).clamp(0.005, 0.25)
                } else {
                    1.0 / f64::from(tick_rate.max(1))
                };
                let step = Duration::from_secs_f64(step_secs);
                let now = Instant::now();
                let scheduled = next_input + step;
                next_input = if scheduled > now {
                    scheduled
                } else {
                    now + step
                };
                let delay_ticks = if seat.adaptive_interp {
                    let ticks_f = interp_delay.delay_ticks();
                    let delay_ms = (ticks_f / measured_rate.rate().max(1.0) * 1000.0).round() as u32;
                    stats.interp_delay_ms.push(delay_ms);
                    ticks_f.round() as u64
                } else {
                    interp_delay_ticks(seat.interp_delay_ms, tick_rate)
                };
                let Some(decision) =
                    view_buffer.decision_view(seat.actor, predictor.predicted(), delay_ticks)
                else {
                    continue;
                };
                let running = view_buffer
                    .latest()
                    .is_some_and(|snapshot| snapshot.tick > 0 && snapshot.result.is_none());
                if running && decision.remote_tick > 0 {
                    last_view_tick = last_view_tick.max(decision.remote_tick);
                }
                let decision_view = decision.snapshot;
                seq += 1;
                let mut frame = InputFrame {
                    seq,
                    view_tick: if running { last_view_tick } else { 0 },
                    ..decide(seat.actor, &decision_view, rng)
                };
                // Hold still while a newly submitted timed cast is still in flight so the
                // next tick's movement frame never cancels it before the server acknowledges it.
                if pending_cast_ack.is_some() {
                    frame.movement = [0, 0];
                    frame.jump = false;
                    frame.action = WireAction::None;
                } else if matches!(
                    frame.action,
                    WireAction::Cast {
                        spell: WireSpell::RiftBolt | WireSpell::Mend,
                        ..
                    }
                ) {
                    pending_cast_ack = Some(seq);
                }
                if !send(ClientMessage::Input(frame)) {
                    writable = false;
                    continue;
                }
                stats.inputs += 1;
                if running
                    && let WireAction::Cast {
                        spell,
                        target: Some(target),
                    } = frame.action
                {
                    fairness.record_cast(
                        &mut stats.fairness,
                        CastSubmission {
                            seq,
                            actor: seat.actor,
                            spell: spell.into(),
                            target: usize::from(target),
                        },
                        &decision_view,
                        &seat.layout,
                    );
                }
                // Recorded from the start, as the browser client does.
                let mut slot = ActorInput::default();
                frame.merge_into(&mut slot);
                predictor.apply(seq, slot);
                if running {
                    stats.predicted_inputs += 1;
                }
                unacked.push_back((seq, now));
                // A stalled server must not grow this without bound.
                if unacked.len() > 1024 {
                    unacked.pop_front();
                }
            }
            _ = pinger.tick(), if welcomed && writable => {
                nonce = nonce.wrapping_add(1);
                pings.insert(nonce, Instant::now());
                writable = send(ClientMessage::Ping { nonce });
            }
            // A join in progress always finishes its handshake first.
            _ = tokio::time::sleep_until(leave.at), if welcomed => {
                break if leave.planned { End::Planned } else { End::Deadline };
            }
            _ = tokio::time::sleep_until(welcome_deadline), if !welcomed => break End::Lost("no welcome within 10 s"),
        }
    };
    // Closing the outgoing line lets the writer send a close frame.
    drop(outgoing);
    let _ = writer.await;
    end
}

/// Records this seat's private `WireEvent::Rejected` events while ignoring any
/// event tick already observed prior to a reconnect. Multiple rejections on the
/// same tick within a single snapshot are all counted.
pub(crate) fn record_seat_rejections(stats: &mut SeatStats, events: &[arena_protocol::TickEvent]) {
    let mut max_event_tick: Option<u64> = None;
    for tick_event in events {
        if tick_event.tick < stats.next_rejection_tick {
            continue;
        }
        max_event_tick = Some(max_event_tick.map_or(tick_event.tick, |m| m.max(tick_event.tick)));
        if let WireEvent::Rejected(reason) = tick_event.event {
            stats.rejections.record_rejection(reason.into());
        }
    }
    if let Some(max_tick) = max_event_tick {
        stats.next_rejection_tick = max_tick.saturating_add(1);
    }
}

/// Whether a round result agrees with the fighters' health and authoritative
/// round clock: a win needs the whole losing team down, a mutual-elimination
/// draw needs every fighter down, and a time-limit draw needs both teams
/// standing at or past `round_ticks`.
pub fn result_matches(
    result: arena_protocol::WireResult,
    fighters: &[FighterState],
    tick: u64,
    round_ticks: u64,
) -> bool {
    let standing = |team: usize| {
        fighters
            .iter()
            .enumerate()
            .any(|(index, fighter)| index % 2 == team && fighter.health > 0)
    };
    match result {
        arena_protocol::WireResult::Won { team } => {
            if team > 1 {
                return false;
            }
            let winner = usize::from(team);
            standing(winner) && !standing(1 - winner)
        }
        arena_protocol::WireResult::Draw => !standing(0) && !standing(1),
        arena_protocol::WireResult::TimeLimitDraw => {
            standing(0) && standing(1) && tick >= round_ticks
        }
    }
}

/// Bot combat decision delegated to the canonical `arena_headless::policy::decide`.
/// The server judges everything; the bot omits LOS checks on purpose.
pub fn decide(actor: usize, snapshot: &Snapshot, rng: &mut Rng) -> InputFrame {
    let count = snapshot.fighters.len().min(MAX_FIGHTERS);
    let mut fighters = [Combatant::new(Position { x: 0, z: 0 }, 0); MAX_FIGHTERS];
    for (dst, src) in fighters[..count]
        .iter_mut()
        .zip(&snapshot.fighters[..count])
    {
        *dst = src.combatant();
    }
    let input = arena_headless::policy::decide(actor, snapshot.tick, &fighters[..count], rng);
    let movement = input.movement.map_or([0, 0], |axes| [axes.x, axes.z]);
    let facing = input.facing.unwrap_or(0);
    let action = match input.action {
        None => WireAction::None,
        Some(CastAction::Cancel) => WireAction::CancelCast,
        Some(CastAction::Cast { spell, target }) => WireAction::Cast {
            spell: spell.into(),
            target: target.and_then(|t| u8::try_from(t).ok()),
        },
    };
    InputFrame {
        seq: 0,
        movement,
        facing,
        jump: input.jump,
        action,
        view_tick: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_protocol::{FighterState, WireCast};
    use arena_sim::spells::SpellId;

    fn fighter(x: i32, z: i32, health: u16) -> FighterState {
        FighterState {
            x,
            z,
            health,
            resource: arena_sim::spells::MAX_RESOURCE,
            connected: true,
            ..FighterState::default()
        }
    }

    fn snapshot(fighters: Vec<FighterState>) -> Snapshot {
        Snapshot {
            tick: 1,
            ack: 0,
            queued: 0,
            hash: 0,
            result: None,
            fighters,
            scoreboard: None,
            events: Vec::new(),
        }
    }

    #[test]
    fn bots_chase_the_nearest_standing_enemy_and_attack_in_range() {
        let mut rng = Rng::new(1);
        // Enemy 3 is nearer but defeated; enemy 1 is straight down −Z.
        let far = snapshot(vec![
            fighter(0, 0, 100),
            fighter(0, -20_000, 100),
            fighter(100, 0, 100),
            fighter(0, -1_000, 0),
        ]);
        let input = decide(0, &far, &mut rng);
        assert_eq!(input.action, WireAction::None);
        assert!(input.movement[1] < -700, "{input:?}");
        assert!(
            input
                .movement
                .iter()
                .all(|axis| (-1000..=1000).contains(axis))
        );
        assert!(
            input.facing < 300 || input.facing > 65_236,
            "faces −Z: {}",
            input.facing
        );
        let mut near_me = fighter(0, 0, 100);
        near_me.cooldowns[SpellId::RiftBolt.index()] = 60;
        let near = snapshot(vec![near_me, fighter(3_000, 0, 100)]);
        let input = decide(0, &near, &mut rng);
        assert_eq!(input.action, InputFrame::pulse(1));
        // Close-range bots strafe laterally instead of standing frozen.
        assert_ne!(input.movement, [0, 0]);
        assert!(input.movement[1].abs() > 700, "{input:?}");
        // Facing +X is a three-quarter turn counter-clockwise from −Z.
        assert!(
            (input.facing as i32 - 0xC000).abs() < 300,
            "{}",
            input.facing
        );
        // Locked-out close-range bots keep strafing instead of freezing or spamming Pulse.
        let mut locked = fighter(0, 0, 100);
        locked.lockout_remaining = 40;
        let locked_snap = snapshot(vec![locked, fighter(3_000, 0, 100)]);
        let locked_input = decide(0, &locked_snap, &mut rng);
        assert_eq!(locked_input.action, WireAction::None);
        assert_ne!(locked_input.movement, [0, 0]);
        // Mid-range (10 m): bot staggers Rift Bolt casts instead of freezing every tick,
        // and plants its feet whenever it chooses to cast.
        let mid = snapshot(vec![fighter(0, 0, 100), fighter(10_000, 0, 100)]);
        let mut saw_move = false;
        let mut saw_bolt = false;
        for _ in 0..80 {
            let step = decide(0, &mid, &mut rng);
            if step.action
                == (WireAction::Cast {
                    spell: WireSpell::RiftBolt,
                    target: Some(1),
                })
            {
                assert_eq!(step.movement, [0, 0]);
                assert!(!step.jump);
                saw_bolt = true;
            } else {
                assert_ne!(step.movement, [0, 0]);
                saw_move = true;
            }
        }
        assert!(saw_move && saw_bolt, "expected staggered move + Rift Bolt");
        // Actively casting bot stands still so its cast finishes.
        let mut self_casting = fighter(0, 0, 100);
        self_casting.active_cast = Some(WireCast {
            spell: WireSpell::RiftBolt,
            target: Some(1),
            remaining_ticks: 20,
            total_ticks: 45,
        });
        let self_cast_snap = snapshot(vec![self_casting, fighter(10_000, 0, 100)]);
        let cast_input = decide(0, &self_cast_snap, &mut rng);
        assert_eq!(
            (cast_input.action, cast_input.movement),
            (WireAction::None, [0, 0])
        );
        // Enemy casting within 8 m: bot waits past the human-like reaction window before
        // casting off-GCD Quell to interrupt while moving.
        let mut early_casting_enemy = fighter(7_000, 0, 100);
        early_casting_enemy.active_cast = Some(WireCast {
            spell: WireSpell::RiftBolt,
            target: Some(0),
            remaining_ticks: 43,
            total_ticks: 45,
        });
        let early_snap = snapshot(vec![fighter(0, 0, 100), early_casting_enemy]);
        assert_ne!(
            decide(0, &early_snap, &mut rng).action,
            WireAction::Cast {
                spell: WireSpell::Quell,
                target: Some(1),
            }
        );
        let mut casting_enemy = fighter(7_000, 0, 100);
        casting_enemy.active_cast = Some(WireCast {
            spell: WireSpell::RiftBolt,
            target: Some(0),
            remaining_ticks: 20,
            total_ticks: 45,
        });
        let interrupt_snap = snapshot(vec![fighter(0, 0, 100), casting_enemy]);
        let interrupt_input = decide(0, &interrupt_snap, &mut rng);
        assert_eq!(
            interrupt_input.action,
            WireAction::Cast {
                spell: WireSpell::Quell,
                target: Some(1),
            }
        );
        assert_ne!(interrupt_input.movement, [0, 0]);
        // Nobody left to fight, or no such actor: stand still.
        let alone = snapshot(vec![fighter(0, 0, 100), fighter(9, 9, 0)]);
        assert_eq!(decide(0, &alone, &mut rng), InputFrame::default());
        assert_eq!(decide(7, &alone, &mut rng), InputFrame::default());
    }

    #[test]
    fn result_matches_verifies_wins_mutual_draws_and_time_limit_draws() {
        use arena_protocol::WireResult;
        let both_alive = vec![fighter(0, 0, 100), fighter(1, 0, 50)];
        let team0_only = vec![fighter(0, 0, 100), fighter(1, 0, 0)];
        let both_down = vec![fighter(0, 0, 0), fighter(1, 0, 0)];

        assert!(result_matches(
            WireResult::Won { team: 0 },
            &team0_only,
            5,
            100
        ));
        assert!(!result_matches(
            WireResult::Won { team: 1 },
            &team0_only,
            5,
            100
        ));
        assert!(!result_matches(
            WireResult::Won { team: 2 },
            &team0_only,
            5,
            100
        ));
        assert!(!result_matches(
            WireResult::Won { team: 0 },
            &both_alive,
            5,
            100
        ));

        assert!(result_matches(WireResult::Draw, &both_down, 5, 100));
        assert!(!result_matches(WireResult::Draw, &both_alive, 100, 100));
        assert!(!result_matches(WireResult::Draw, &team0_only, 5, 100));

        assert!(result_matches(
            WireResult::TimeLimitDraw,
            &both_alive,
            100,
            100
        ));
        assert!(!result_matches(
            WireResult::TimeLimitDraw,
            &both_alive,
            99,
            100
        ));
        assert!(!result_matches(
            WireResult::TimeLimitDraw,
            &both_down,
            100,
            100
        ));
        assert!(!result_matches(
            WireResult::TimeLimitDraw,
            &team0_only,
            100,
            100
        ));
    }

    #[test]
    fn seat_rejections_count_multiple_same_tick_events_and_deduplicate_replays() {
        use arena_protocol::{TickEvent, WireRejection};
        let mut stats = SeatStats::default();
        // Two rejections on tick 5 in one snapshot must both be counted.
        record_seat_rejections(
            &mut stats,
            &[
                TickEvent {
                    tick: 5,
                    event: WireEvent::Rejected(WireRejection::LineOfSight),
                },
                TickEvent {
                    tick: 5,
                    event: WireEvent::Rejected(WireRejection::OutOfRange),
                },
            ],
        );
        assert_eq!(stats.rejections.los_blocks, 1);
        assert_eq!(stats.rejections.los_rejections, 1);
        assert_eq!(stats.rejections.out_of_range_blocks, 1);
        assert_eq!(stats.rejections.rejections_total, 2);
        assert_eq!(stats.next_rejection_tick, 6);

        // Replayed events for tick <= 5 (e.g. across reconnect) are ignored.
        record_seat_rejections(
            &mut stats,
            &[TickEvent {
                tick: 5,
                event: WireEvent::Rejected(WireRejection::LineOfSight),
            }],
        );
        assert_eq!(stats.rejections.rejections_total, 2);

        // Non-rejection events advance `next_rejection_tick` without adding rejections.
        record_seat_rejections(
            &mut stats,
            &[TickEvent {
                tick: 8,
                event: WireEvent::Jumped { actor: 0 },
            }],
        );
        assert_eq!(stats.rejections.rejections_total, 2);
        assert_eq!(stats.next_rejection_tick, 9);

        // Stale tick 7 is now ignored; fresh tick 9 is counted.
        record_seat_rejections(
            &mut stats,
            &[
                TickEvent {
                    tick: 7,
                    event: WireEvent::Rejected(WireRejection::Silenced),
                },
                TickEvent {
                    tick: 9,
                    event: WireEvent::Rejected(WireRejection::LineOfSight),
                },
            ],
        );
        assert_eq!(stats.rejections.los_blocks, 2);
        assert_eq!(stats.rejections.rejections_total, 3);
    }
}
