//! Hub load test ([ADR 0056](../../../docs/decisions/0056-social-hub.md)):
//! bots that join the front door's hub and wander, measuring what the door
//! sends each of them. `arena-bot --hub --door HOST:PORT --bots 40` fills
//! one instance.

use std::fmt;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::link::Rng;

/// A valid look for every bot: the door checks only its shape.
const LOOK: &str = r#"{"ancestry":"human","body":"male","skin_tone":"s1","hair_color":"h1","facial_hair_color":"h1","eyes":"e1","build":0,"groups":[],"variants":[]}"#;

#[derive(Clone, Debug)]
pub struct HubScenario {
    /// `host:port` of the front door.
    pub door: String,
    pub bots: usize,
    pub duration: Duration,
    pub seed: u64,
}

/// What the door sent, per bot and in total.
#[derive(Clone, Debug, Default, Serialize)]
pub struct HubReport {
    pub bots: usize,
    pub seconds: f64,
    /// Bots that got into the hub.
    pub joined: usize,
    /// Distinct instances they were put in.
    pub instances: usize,
    pub states: u64,
    /// Received bytes per second per bot: mean and largest.
    pub bytes_per_second_mean: f64,
    pub bytes_per_second_max: f64,
    /// `hub_state` messages per second per bot (10 expected).
    pub states_per_second_mean: f64,
    /// Most players one bot saw at once.
    pub largest_view: usize,
    /// The largest single message, bytes.
    pub largest_message: usize,
    pub refusals: Vec<String>,
    pub unexpected_closes: usize,
}

impl fmt::Display for HubReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "hub: {} bots, {} joined, {} instance(s), {:.1} s",
            self.bots, self.joined, self.instances, self.seconds
        )?;
        writeln!(
            f,
            "  per bot: {:.0} B/s mean, {:.0} B/s max, {:.1} states/s; largest view {}, largest message {} B",
            self.bytes_per_second_mean,
            self.bytes_per_second_max,
            self.states_per_second_mean,
            self.largest_view,
            self.largest_message
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
        self.refusals.is_empty() && self.unexpected_closes == 0 && self.joined == self.bots
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum FromDoor {
    Hub {
        instance: u32,
    },
    HubState {
        players: Vec<(u32, i32, i32, u16)>,
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
    instance: Option<u32>,
    bytes: u64,
    states: u64,
    largest_view: usize,
    largest_message: usize,
    refusals: Vec<String>,
    closed: bool,
}

/// One bot: hello, join, then a new random direction every half second or
/// so, until `until`.
async fn wander(door: String, guest: u128, seed: u64, until: Instant) -> One {
    let mut one = One::default();
    let url = format!("ws://{door}/api/queue");
    let Ok((mut socket, _)) = connect_async(&url).await else {
        one.refusals.push(format!("cannot reach {url}"));
        return one;
    };
    for message in [
        format!(r#"{{"type":"hello","guest":"{guest:032x}"}}"#),
        format!(r#"{{"type":"hub_join","appearance":{LOOK}}}"#),
    ] {
        if socket.send(Message::text(message)).await.is_err() {
            one.closed = true;
            return one;
        }
    }
    let mut rng = Rng::new(seed);
    let mut seq = 0u32;
    // A new direction now and then; one move per hub tick, as the page's
    // renderer sends them (the door applies each once).
    let mut heading = (0i32, 0i32, 0u64);
    let mut turn = Instant::now();
    let mut step = Instant::now();
    loop {
        let now = Instant::now();
        if now >= until {
            break;
        }
        if now >= turn {
            turn = now + Duration::from_millis(300) + rng.below(Duration::from_millis(700));
            let axis = |rng: &mut Rng| (rng.next_u64() % 2001) as i32 - 1000;
            heading = (axis(&mut rng), axis(&mut rng), rng.next_u64() % 65_536);
        }
        if now >= step {
            step = now + Duration::from_millis(100);
            seq += 1;
            let (x, z, facing) = heading;
            let message =
                format!(r#"{{"type":"hub_move","seq":{seq},"x":{x},"z":{z},"facing":{facing}}}"#);
            if socket.send(Message::text(message)).await.is_err() {
                one.closed = true;
                break;
            }
        }
        let wait = step.min(until).saturating_duration_since(now);
        match tokio::time::timeout(wait, socket.next()).await {
            Err(_) => {}
            Ok(Some(Ok(Message::Text(text)))) => {
                one.bytes += text.len() as u64;
                one.largest_message = one.largest_message.max(text.len());
                match serde_json::from_str(&text) {
                    Ok(FromDoor::Hub { instance }) => one.instance = Some(instance),
                    Ok(FromDoor::HubState { players }) => {
                        one.states += 1;
                        one.largest_view = one.largest_view.max(players.len());
                    }
                    Ok(FromDoor::Refused { code } | FromDoor::Error { code }) => {
                        one.refusals.push(code);
                    }
                    _ => {}
                }
            }
            Ok(Some(Ok(_))) => {}
            Ok(_) => {
                one.closed = true;
                break;
            }
        }
    }
    let _ = socket.close(None).await;
    one
}

/// Runs `scenario.bots` bots in the hub for `scenario.duration`.
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
    let mut instances: Vec<u32> = bots.iter().filter_map(|one| one.instance).collect();
    instances.sort_unstable();
    instances.dedup();
    let count = bots.len().max(1) as f64;
    HubReport {
        bots: scenario.bots,
        seconds,
        joined: bots.iter().filter(|one| one.instance.is_some()).count(),
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
        refusals: bots.iter().flat_map(|one| one.refusals.clone()).collect(),
        unexpected_closes: bots.iter().filter(|one| one.closed).count(),
    }
}
