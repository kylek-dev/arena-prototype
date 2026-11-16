//! One bot seat: connects with its ticket, plays through ordinary inputs
//! only (it has no other power than a browser would), and measures what a
//! player would experience. Every byte in both directions passes through a
//! simulated [`Link`].

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use arena_protocol::prediction::Predictor;
use arena_protocol::{
    ClientMessage, ErrorCode, FighterState, InputFrame, PROTOCOL_VERSION, ServerMessage, Snapshot,
};
use arena_sim::arena::{ATTACK_RANGE, ActorInput};
use arena_sim::map::Layout;
use futures_util::{SinkExt, StreamExt};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_tungstenite::connect_async_with_config;
use tokio_tungstenite::tungstenite::Message;

use crate::Shared;
use crate::link::{Link, Rng, delay_line};
use crate::stats::Samples;

#[derive(Clone)]
pub struct Seat {
    pub server: String,
    pub match_id: u64,
    pub actor: usize,
    pub ticket: Vec<u8>,
    pub link: Link,
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
    /// Inputs applied to the predicted fighter, as a browser client does.
    pub predicted_inputs: u64,
    /// Reconciliations that moved the prediction, and by how much (mm).
    pub corrections: u64,
    pub correction_mm: Vec<u32>,
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
    let mut latest: Option<Snapshot> = None;
    let mut last_snapshot_at: Option<Instant> = None;
    let mut seq = 0u32;
    let mut unacked: VecDeque<(u32, Instant)> = VecDeque::new();
    let mut pings: HashMap<u32, Instant> = HashMap::new();
    let mut nonce = 0u32;
    let mut welcomed = false;
    // Predicts this seat's movement exactly as the browser client does, to
    // measure how often the server corrects it.
    let mut predictor = Predictor::default();
    // Once the socket refuses writes, stop sending and let the incoming side
    // deliver what the server said last (usually an `Error`), which the
    // simulated delay may still be holding.
    let mut writable = true;
    let mut inputs = tokio::time::interval(Duration::from_secs(1) / tick_rate);
    inputs.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut pinger = tokio::time::interval(Duration::from_secs(1));
    let welcome_deadline = Instant::now() + Duration::from_secs(10);

    let end = loop {
        tokio::select! {
            frame = inbox.recv() => {
                let Some(frame) = frame else { break End::Lost("closed without an error") };
                stats.bytes_in += frame.len() as u64;
                let Ok(message) = ServerMessage::decode(&frame) else { break End::Lost("undecodable frame") };
                match message {
                    ServerMessage::Welcome(welcome) => {
                        welcomed = true;
                        if rejoining {
                            stats.rejoins_succeeded += 1;
                        }
                        if u32::from(welcome.tick_rate) != tick_rate && welcome.tick_rate > 0 {
                            tick_rate = u32::from(welcome.tick_rate);
                            inputs = tokio::time::interval(Duration::from_secs(1) / tick_rate);
                            inputs.set_missed_tick_behavior(MissedTickBehavior::Delay);
                        }
                    }
                    ServerMessage::Snapshot(snapshot) => {
                        let now = Instant::now();
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
                        if snapshot.tick > 0 {
                            seat.shared.check_hash(seat.match_id, snapshot.tick, snapshot.hash);
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
                        latest = Some(snapshot);
                    }
                    ServerMessage::Pong { nonce } => {
                        if let Some(sent) = pings.remove(&nonce) {
                            stats.rtt.record(Instant::now() - sent);
                        }
                    }
                    ServerMessage::Error(code) => break End::Error(code),
                }
            }
            _ = inputs.tick(), if welcomed && writable => {
                let Some(snapshot) = &latest else { continue };
                seq += 1;
                let frame = InputFrame { seq, ..decide(seat.actor, snapshot, rng) };
                if !send(ClientMessage::Input(frame)) {
                    writable = false;
                    continue;
                }
                stats.inputs += 1;
                // Recorded from the start, as the browser client does.
                let mut slot = ActorInput::default();
                frame.merge_into(&mut slot);
                predictor.apply(seq, slot);
                if snapshot.tick > 0 && snapshot.result.is_none() {
                    stats.predicted_inputs += 1;
                }
                unacked.push_back((seq, Instant::now()));
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

/// Chooses an input from the bot's (delayed) view: walk toward the nearest
/// standing enemy, attack when it looks in range, face it, and jump now and
/// then. The server judges everything; the bot is often wrong on purpose.
pub fn decide(actor: usize, snapshot: &Snapshot, rng: &mut Rng) -> InputFrame {
    let Some(me) = snapshot.fighters.get(actor) else {
        return InputFrame::default();
    };
    let target = snapshot
        .fighters
        .iter()
        .enumerate()
        .filter(|(index, fighter)| index % 2 != actor % 2 && fighter.health > 0)
        .min_by_key(|(_, fighter)| {
            let (dx, dz) = (i64::from(fighter.x - me.x), i64::from(fighter.z - me.z));
            dx * dx + dz * dz
        });
    let Some((index, enemy)) = target else {
        return InputFrame::default();
    };
    let (dx, dz) = (f64::from(enemy.x - me.x), f64::from(enemy.z - me.z));
    let distance = dx.hypot(dz).max(1.0);
    // Planar heading: 0 faces −Z, counter-clockwise from above.
    let yaw = (-dx).atan2(-dz);
    let facing = ((yaw / std::f64::consts::TAU).rem_euclid(1.0) * 65536.0).round() as u32 as u16;
    let close = distance < f64::from(ATTACK_RANGE) * 0.8;
    // Wander a little so bots round pillars instead of sticking to them.
    let wobble = (rng.unit() - 0.5) * 0.6;
    let (sin, cos) = wobble.sin_cos();
    let (ux, uz) = (dx / distance, dz / distance);
    let (mx, mz) = (ux * cos - uz * sin, ux * sin + uz * cos);
    InputFrame {
        seq: 0,
        movement: if close {
            [0, 0]
        } else {
            [(mx * 1000.0) as i16, (mz * 1000.0) as i16]
        },
        facing,
        jump: rng.unit() < 1.0 / 90.0,
        attack: (distance <= f64::from(ATTACK_RANGE)).then_some(index as u8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_protocol::FighterState;

    fn fighter(x: i32, z: i32, health: u16) -> FighterState {
        FighterState {
            x,
            z,
            height: 0,
            vertical_velocity: 0,
            momentum: [0, 0],
            health,
            ready_at: 0,
            facing: 0,
            connected: true,
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
        assert_eq!(input.attack, None);
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
        let near = snapshot(vec![fighter(0, 0, 100), fighter(3_000, 0, 100)]);
        let input = decide(0, &near, &mut rng);
        assert_eq!((input.attack, input.movement), (Some(1), [0, 0]));
        // Facing +X is a three-quarter turn counter-clockwise from −Z.
        assert!(
            (input.facing as i32 - 0xC000).abs() < 300,
            "{}",
            input.facing
        );
        // Nobody left to fight, or no such actor: stand still.
        let alone = snapshot(vec![fighter(0, 0, 100), fighter(9, 9, 0)]);
        assert_eq!(decide(0, &alone, &mut rng), InputFrame::default());
        assert_eq!(decide(7, &alone, &mut rng), InputFrame::default());
    }
}
