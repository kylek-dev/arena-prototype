//! Bots that queue through the front door (ADR 0046) instead of creating
//! matches: each one joins a solo queue, confirms the ready check, plays the
//! seat it is given and queues again until the run ends. Bots and people
//! share the queue, so a single tester can fill a solo 2v2 or 3v3 match.

use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::time::Instant;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::bot::{self, Seat};
use crate::link::Rng;
use crate::{Scenario, Shared, Totals, from_hex};

#[derive(Clone, Debug)]
pub struct QueueScenario {
    /// `host:port` of the front door.
    pub door: String,
    /// `1v1`, `2v2` or `3v3`. Bots queue alone.
    pub mode: String,
    pub bots: usize,
}

/// The door's messages this bot reads; others are ignored.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum FromDoor {
    ReadyCheck {
        you: bool,
    },
    Match {
        server: String,
        ticket: String,
        match_id: String,
        seat: usize,
    },
    Error {
        code: String,
    },
    Refused {
        code: String,
    },
    #[serde(other)]
    Other,
}

/// `ws://host:port/play` → `host:port`. Bots speak plain WebSockets only.
fn play_address(server: &str) -> Result<String, String> {
    server
        .strip_prefix("ws://")
        .and_then(|rest| rest.strip_suffix("/play"))
        .map(str::to_owned)
        .ok_or_else(|| format!("bots need a ws://HOST:PORT/play match server, not {server}"))
}

/// A seat the queue handed out.
struct Found {
    server: String,
    match_id: u64,
    seat: usize,
    ticket: Vec<u8>,
}

/// Waits in the queue until a seat is given.
async fn find_seat(queue: &QueueScenario, guest: u128) -> Result<Found, String> {
    let url = format!("ws://{}/api/queue", queue.door);
    let (mut socket, _) = connect_async(&url)
        .await
        .map_err(|error| format!("cannot reach {url}: {error}"))?;
    for message in [
        format!(r#"{{"type":"hello","guest":"{guest:032x}"}}"#),
        format!(r#"{{"type":"queue","mode":"{}"}}"#, queue.mode),
    ] {
        socket
            .send(Message::text(message))
            .await
            .map_err(|error| error.to_string())?;
    }
    while let Some(message) = socket.next().await {
        let Ok(Message::Text(text)) = message else {
            continue;
        };
        match serde_json::from_str::<FromDoor>(&text).map_err(|error| error.to_string())? {
            // Bots accept every match they are offered.
            FromDoor::ReadyCheck { you: false } => socket
                .send(Message::text(r#"{"type":"accept"}"#))
                .await
                .map_err(|error| error.to_string())?,
            FromDoor::ReadyCheck { you: true } => {}
            FromDoor::Match {
                server,
                ticket,
                match_id,
                seat,
            } => {
                return Ok(Found {
                    server: play_address(&server)?,
                    match_id: u64::from_str_radix(&match_id, 16)
                        .map_err(|_| format!("bad match id {match_id}"))?,
                    seat,
                    ticket: from_hex(&ticket).ok_or("bad ticket hex")?,
                });
            }
            FromDoor::Error { code } | FromDoor::Refused { code } => {
                return Err(format!("the queue refused the bot: {code}"));
            }
            FromDoor::Other => {}
        }
    }
    Err("the door closed the queue connection".into())
}

/// One queueing bot: queue, play, repeat until the deadline.
pub(crate) async fn run_bot(
    scenario: Scenario,
    queue: QueueScenario,
    bot: u64,
    deadline: Instant,
    shared: Arc<Shared>,
    layout: Arc<arena_sim::map::Layout>,
) -> Result<Totals, String> {
    let mut totals = Totals::default();
    let mut rng = Rng::new(scenario.seed ^ bot.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let guest = u128::from(rng.next_u64()) << 64 | u128::from(rng.next_u64());
    while Instant::now() < deadline {
        let found = tokio::select! {
            found = find_seat(&queue, guest) => found?,
            // Nobody else came before the run ended.
            _ = tokio::time::sleep_until(deadline) => break,
        };
        totals.rounds_started += 1;
        let seat = Seat {
            server: found.server,
            match_id: found.match_id,
            actor: found.seat,
            ticket: found.ticket,
            link: scenario.link,
            paced: scenario.paced,
            interp_delay_ms: scenario.interp_delay_ms,
            seed: rng.next_u64(),
            reconnect_every: scenario.reconnect_every,
            away: scenario.away,
            layout: layout.clone(),
            shared: shared.clone(),
        };
        let stats = bot::play(seat, deadline).await;
        let finished = stats.round_finished;
        totals.add(stats);
        if finished {
            totals.rounds_finished += 1;
        }
    }
    Ok(totals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bots_connect_to_plain_play_endpoints_only() {
        assert_eq!(
            play_address("ws://127.0.0.1:8080/play").as_deref(),
            Ok("127.0.0.1:8080")
        );
        assert!(play_address("wss://arena.example.com/play").is_err());
        assert!(play_address("ws://127.0.0.1:8080/").is_err());
    }
}
