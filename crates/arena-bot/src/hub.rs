//! Hub load test ([ADR 0057](../../../docs/decisions/0057-hub-on-the-world-server.md)):
//! bots ask the front door for a hub world, join the world server with the
//! ticket it hands them, and wander, measuring what the world server sends
//! each of them. `arena-bot --hub --door HOST:PORT --bots 12` fills one
//! world. Each bot keeps its queue connection open: the door forgets the
//! placement of a connection that closes.

use std::fmt;
use std::time::Duration;

use arena_protocol::{ClientMessage, InputFrame, PROTOCOL_VERSION, ServerMessage, WireAction};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::link::Rng;

#[derive(Clone, Debug)]
pub struct HubScenario {
    /// `host:port` of the front door.
    pub door: String,
    pub bots: usize,
    pub duration: Duration,
    pub seed: u64,
}

/// What the world server sent, per bot and in total.
#[derive(Clone, Debug, Default, Serialize)]
pub struct HubReport {
    pub bots: usize,
    pub seconds: f64,
    /// Bots that got into a world (welcomed by the world server).
    pub joined: usize,
    /// Distinct worlds they were put in.
    pub instances: usize,
    /// Snapshots received in all.
    pub states: u64,
    /// Received bytes per second per bot: mean and largest.
    pub bytes_per_second_mean: f64,
    pub bytes_per_second_max: f64,
    /// Snapshots per second per bot (30 expected).
    pub states_per_second_mean: f64,
    /// Most other players one bot saw at once.
    pub largest_view: usize,
    /// The largest single frame, bytes.
    pub largest_message: usize,
    /// Snapshots whose hash did not match the world they describe.
    pub hash_mismatches: u64,
    /// Snapshots that came as deltas, and deltas that did not apply.
    pub deltas: u64,
    pub deltas_unapplied: u64,
    /// What the same snapshots would have cost in full, per bot, bytes per
    /// second (mean): the saving deltas make.
    pub full_bytes_per_second_mean: f64,
    pub refusals: Vec<String>,
    pub unexpected_closes: usize,
}

impl fmt::Display for HubReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "hub: {} bots, {} joined, {} world(s), {:.1} s",
            self.bots, self.joined, self.instances, self.seconds
        )?;
        writeln!(
            f,
            "  per bot: {:.0} B/s mean ({:.0} B/s in full snapshots), {:.0} B/s max, {:.1} snapshots/s ({} deltas, {} unapplied); largest view {}, largest frame {} B, {} hash mismatches",
            self.bytes_per_second_mean,
            self.full_bytes_per_second_mean,
            self.bytes_per_second_max,
            self.states_per_second_mean,
            self.deltas,
            self.deltas_unapplied,
            self.largest_view,
            self.largest_message,
            self.hash_mismatches
        )?;
        writeln!(
            f,
            "  refusals {:?}, unexpected closes {}",
            self.refusals, self.unexpected_closes
        )
    }
}

impl HubReport {
    pub fn healthy(&self) -> bool {
        self.refusals.is_empty()
            && self.unexpected_closes == 0
            && self.joined == self.bots
            && self.hash_mismatches == 0
            && self.deltas_unapplied == 0
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum FromDoor {
    World {
        instance: String,
        server: String,
        ticket: String,
    },
    Refused {
        code: String,
    },
    Error {
        code: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Default)]
struct One {
    instance: Option<String>,
    welcomed: bool,
    bytes: u64,
    states: u64,
    largest_view: usize,
    largest_message: usize,
    hash_mismatches: u64,
    deltas: u64,
    deltas_unapplied: u64,
    full_bytes: u64,
    refusals: Vec<String>,
    closed: bool,
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The door's answer to `world_join`: the world's instance, server and ticket.
async fn place(door: &str, guest: u128, one: &mut One) -> Option<(Socket, String, Vec<u8>)> {
    let url = format!("ws://{door}/api/queue");
    let Ok((mut queue, _)) = connect_async(&url).await else {
        one.refusals.push(format!("cannot reach {url}"));
        return None;
    };
    for message in [
        format!(r#"{{"type":"hello","guest":"{guest:032x}"}}"#),
        r#"{"type":"world_join"}"#.to_owned(),
    ] {
        if queue.send(Message::text(message)).await.is_err() {
            one.closed = true;
            return None;
        }
    }
    loop {
        match tokio::time::timeout(Duration::from_secs(10), queue.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => match serde_json::from_str(&text) {
                Ok(FromDoor::World {
                    instance,
                    server,
                    ticket,
                }) => {
                    one.instance = Some(instance);
                    let ticket = arena_protocol_hex(&ticket)?;
                    return Some((queue, server, ticket));
                }
                Ok(FromDoor::Refused { code } | FromDoor::Error { code }) => {
                    one.refusals.push(code);
                    return None;
                }
                _ => {}
            },
            Ok(Some(Ok(_))) => {}
            _ => {
                one.closed = true;
                return None;
            }
        }
    }
}

fn arena_protocol_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
        .collect()
}

/// One bot: placed by the door, then in its world sending one input a tick
/// with a new random direction now and then, until `until`.
async fn wander(door: String, guest: u128, seed: u64, until: Instant) -> One {
    let mut one = One::default();
    let Some((mut queue, server, ticket)) = place(&door, guest, &mut one).await else {
        return one;
    };
    let Ok((mut world, _)) = connect_async(&server).await else {
        one.refusals.push(format!("cannot reach {server}"));
        return one;
    };
    let hello = ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        ticket,
        appearance: None,
    };
    if world.send(Message::binary(hello.encode())).await.is_err() {
        one.closed = true;
        return one;
    }
    let mut rng = Rng::new(seed);
    let mut seq = 0u32;
    let mut heading = ([0i16, 0i16], 0u16);
    let mut last: Option<arena_protocol::Snapshot> = None;
    let mut turn = Instant::now();
    let mut step = Instant::now();
    let tick = Duration::from_nanos(1_000_000_000 / arena_sim::TICKS_PER_SECOND);
    loop {
        let now = Instant::now();
        if now >= until {
            break;
        }
        if now >= turn {
            turn = now + Duration::from_millis(300) + rng.below(Duration::from_millis(700));
            let axis = |rng: &mut Rng| (rng.next_u64() % 2001) as i16 - 1000;
            heading = (
                [axis(&mut rng), axis(&mut rng)],
                (rng.next_u64() % 65_536) as u16,
            );
        }
        if now >= step && one.welcomed {
            step = now + tick;
            seq += 1;
            let input = ClientMessage::Input(InputFrame {
                seq,
                movement: heading.0,
                facing: heading.1,
                jump: false,
                action: WireAction::None,
                view_tick: 0,
                cast_buffer: 0,
            });
            if world.send(Message::binary(input.encode())).await.is_err() {
                one.closed = true;
                break;
            }
        }
        let wait = step
            .max(now + Duration::from_millis(1))
            .min(until)
            .saturating_duration_since(now);
        tokio::select! {
            frame = tokio::time::timeout(wait, world.next()) => match frame {
                Err(_) => {}
                Ok(Some(Ok(Message::Binary(frame)))) => {
                    one.bytes += frame.len() as u64;
                    one.largest_message = one.largest_message.max(frame.len());
                    let decoded = match ServerMessage::decode(&frame) {
                        Ok(ServerMessage::WorldDelta(delta)) => {
                            one.deltas += 1;
                            match last.as_ref().map(|base| delta.apply(base)) {
                                Some(Ok(snapshot)) => Ok(ServerMessage::Snapshot(snapshot)),
                                _ => {
                                    one.deltas_unapplied += 1;
                                    continue;
                                }
                            }
                        }
                        other => other,
                    };
                    match decoded {
                        Ok(ServerMessage::Welcome(_)) => one.welcomed = true,
                        Ok(ServerMessage::Snapshot(snapshot)) => {
                            one.states += 1;
                            one.full_bytes +=
                                arena_protocol::delta::full_snapshot_encoded_len(&snapshot) as u64;
                            match snapshot.hub_arena() {
                                Some(arena) => {
                                    if arena.state_hash() != snapshot.hash {
                                        one.hash_mismatches += 1;
                                    }
                                    one.largest_view =
                                        one.largest_view.max(arena.players().saturating_sub(1));
                                }
                                None => one.hash_mismatches += 1,
                            }
                            last = Some(snapshot);
                        }
                        Ok(ServerMessage::Error(code)) => {
                            one.refusals.push(format!("{code:?}"));
                        }
                        Ok(_) => {}
                        Err(_) => one.refusals.push("undecodable frame".into()),
                    }
                }
                Ok(Some(Ok(_))) => {}
                Ok(_) => {
                    one.closed = true;
                    break;
                }
            },
            // The queue connection stays open (and quiet) all along.
            message = queue.next() => {
                if !matches!(message, Some(Ok(_))) {
                    one.closed = true;
                    break;
                }
            }
        }
    }
    let _ = world.close(None).await;
    let _ = queue.close(None).await;
    one
}

/// Runs `scenario.bots` bots in hub worlds for `scenario.duration`.
pub async fn run(scenario: HubScenario) -> HubReport {
    let started = Instant::now();
    let until = started + scenario.duration;
    let tasks: Vec<_> = (0..scenario.bots)
        .map(|index| {
            tokio::spawn(wander(
                scenario.door.clone(),
                0x4855_4200_0000_0000_0000_0000_0000_0000 + index as u128,
                scenario.seed.wrapping_add(index as u64),
                until,
            ))
        })
        .collect();
    let mut bots = Vec::new();
    for task in tasks {
        bots.push(task.await.unwrap_or_else(|_| One {
            closed: true,
            ..One::default()
        }));
    }
    let seconds = started.elapsed().as_secs_f64().max(0.001);
    let rates: Vec<f64> = bots.iter().map(|one| one.bytes as f64 / seconds).collect();
    let mut instances: Vec<String> = bots.iter().filter_map(|one| one.instance.clone()).collect();
    instances.sort_unstable();
    instances.dedup();
    let count = bots.len().max(1) as f64;
    HubReport {
        bots: scenario.bots,
        seconds,
        joined: bots.iter().filter(|one| one.welcomed).count(),
        instances: instances.len(),
        states: bots.iter().map(|one| one.states).sum(),
        bytes_per_second_mean: rates.iter().sum::<f64>() / count,
        bytes_per_second_max: rates.iter().copied().fold(0.0, f64::max),
        states_per_second_mean: bots.iter().map(|one| one.states as f64).sum::<f64>()
            / count
            / seconds,
        largest_view: bots.iter().map(|one| one.largest_view).max().unwrap_or(0),
        largest_message: bots
            .iter()
            .map(|one| one.largest_message)
            .max()
            .unwrap_or(0),
        hash_mismatches: bots.iter().map(|one| one.hash_mismatches).sum(),
        deltas: bots.iter().map(|one| one.deltas).sum(),
        deltas_unapplied: bots.iter().map(|one| one.deltas_unapplied).sum(),
        full_bytes_per_second_mean: bots.iter().map(|one| one.full_bytes as f64).sum::<f64>()
            / count
            / seconds,
        refusals: bots.iter().flat_map(|one| one.refusals.clone()).collect(),
        unexpected_closes: bots.iter().filter(|one| one.closed).count(),
    }
}
