//! `GET /api/queue`: the solo queues over a WebSocket (ADR 0046).
//!
//! The browser sends small JSON messages and receives the same:
//!
//! - `{"type":"join","mode":"1v1"|"solo-2v2"|"solo-3v3","guest":"<32 hex>"}`
//!   first, within 5 s; again later to switch modes.
//! - `{"type":"ready"}` after `found`, which the game sends automatically.
//! - `{"type":"leave"}`, or closing the socket, leaves the queue.
//!
//! The door answers `queued` (with how many wait in that mode) on joining
//! and every 2 s, `found` when a ready check starts, `requeued` when it
//! failed because of someone else, `match` with the player's own seat
//! ticket, and `error` before closing. One task owns the
//! [`Matchmaker`]; connections talk to it over channels.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior};

use super::{Door, unix_now};
use crate::RateWindow;
use crate::ingress::{self, OpenSlot};
use crate::matchmaker::{Closed, Guest, Matchmaker, Mode, Out, QueueConn};
use crate::metrics::Metrics;
use crate::ticket;

/// Largest message a queue client may send.
const MAX_MESSAGE: usize = 512;
const JOIN_TIMEOUT: Duration = Duration::from_secs(5);
const MESSAGES_PER_SECOND: u32 = 10;
const STATUS_EVERY: Duration = Duration::from_secs(2);
const TICK_EVERY: Duration = Duration::from_millis(250);
/// Messages buffered per connection; status updates are skipped when full.
const OUTBOUND_QUEUE: usize = 16;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum FromClient {
    Join { mode: String, guest: String },
    // Empty structs rather than unit variants, so extra fields are refused.
    Ready {},
    Leave {},
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToClient {
    /// In the queue; `waiting` players (including this one) want `mode`.
    Queued {
        mode: String,
        waiting: usize,
    },
    /// A match was found: answer `ready` within the ready check.
    Found {
        mode: String,
    },
    /// Someone else did not confirm; back at the front of the queue.
    Requeued {
        mode: String,
    },
    /// This player's seat: join `server` with `ticket` (hex). The match id
    /// (hex) and seat are for display and bug reports; the ticket decides.
    Match {
        mode: String,
        match_id: String,
        seat: usize,
        server: String,
        ticket: String,
        expires: u64,
    },
    Error {
        code: String,
        message: String,
    },
}
impl ToClient {
    fn error(code: &str, message: &str) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }
}

pub(super) enum Command {
    Join {
        conn: QueueConn,
        guest: Guest,
        mode: Mode,
        out: mpsc::Sender<ToClient>,
    },
    Ready {
        conn: QueueConn,
    },
    Leave {
        conn: QueueConn,
    },
}

/// The guest id: 32 hex digits.
fn parse_guest(text: &str) -> Option<Guest> {
    (text.len() == 32 && text.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| u128::from_str_radix(text, 16).ok())
        .flatten()
}

fn random_below(bound: usize) -> usize {
    (ticket::random_u64() % bound as u64) as usize
}

/// The task that owns the matchmaker. Ends when the door stops, telling
/// every queued player.
pub(super) async fn run(door: Arc<Door>, mut commands: mpsc::Receiver<Command>) {
    let mut mm = Matchmaker::new(door.config.ready_timeout, random_below);
    let mut conns: HashMap<QueueConn, (Mode, mpsc::Sender<ToClient>)> = HashMap::new();
    let mut tick = tokio::time::interval(TICK_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut status_at = Instant::now() + STATUS_EVERY;
    let mut stop = door.shutdown.subscribe();
    loop {
        let now = Instant::now();
        let out = tokio::select! {
            _ = stop.wait_for(|stop| *stop) => break,
            _ = tick.tick() => mm.tick(now),
            command = commands.recv() => match command {
                None => break,
                Some(Command::Join { conn, guest, mode, out }) => {
                    conns.insert(conn, (mode, out));
                    let actions = mm.join(conn, guest, mode, now);
                    // Confirm the entry unless it went straight to a check.
                    if !actions.iter().any(|action| matches!(action, Out::Found { conn: c, .. } if *c == conn)) {
                        let waiting = mm.waiting()[mode as usize];
                        if let Some((_, sender)) = conns.get(&conn) {
                            let _ = sender.try_send(ToClient::Queued { mode: mode.id().into(), waiting });
                        }
                    }
                    actions
                }
                Some(Command::Ready { conn }) => mm.ready(conn, now),
                Some(Command::Leave { conn }) => {
                    conns.remove(&conn);
                    mm.leave(conn, now)
                }
            },
        };
        for action in out {
            match action {
                Out::Found { conn, mode } => send(
                    &conns,
                    conn,
                    ToClient::Found {
                        mode: mode.id().into(),
                    },
                ),
                Out::Requeued { conn, mode } => send(
                    &conns,
                    conn,
                    ToClient::Requeued {
                        mode: mode.id().into(),
                    },
                ),
                Out::Close { conn, reason } => {
                    let message = match reason {
                        Closed::Replaced => ToClient::error(
                            "replaced",
                            "You joined the queue from another tab or window.",
                        ),
                        Closed::NotReady => ToClient::error(
                            "not_ready",
                            "A match was found but this game did not confirm in time.",
                        ),
                    };
                    send(&conns, conn, message);
                    conns.remove(&conn);
                }
                Out::Start { mode, seats } => start_match(&door, &mut conns, mode, seats),
            }
        }
        if now >= status_at {
            status_at = now + STATUS_EVERY;
            let waiting = mm.waiting();
            for (conn, mode) in mm.queued() {
                if let Some((_, sender)) = conns.get(&conn) {
                    // A full outbox only skips this update.
                    let _ = sender.try_send(ToClient::Queued {
                        mode: mode.id().into(),
                        waiting: waiting[mode as usize],
                    });
                }
            }
        }
        door.metrics
            .queued
            .store(mm.waiting().iter().sum::<usize>() as u64, Ordering::Relaxed);
    }
    for (_, (_, sender)) in conns {
        let _ = sender.try_send(ToClient::error(
            "shutdown",
            "The matchmaker is restarting. Queue again in a moment.",
        ));
    }
}

fn send(
    conns: &HashMap<QueueConn, (Mode, mpsc::Sender<ToClient>)>,
    conn: QueueConn,
    message: ToClient,
) {
    if let Some((_, sender)) = conns.get(&conn) {
        let _ = sender.try_send(message);
    }
}

fn start_match(
    door: &Door,
    conns: &mut HashMap<QueueConn, (Mode, mpsc::Sender<ToClient>)>,
    mode: Mode,
    seats: Vec<QueueConn>,
) {
    let expires = unix_now() + door.config.ticket_ttl.as_secs();
    let created = ticket::issue_match(&door.config.key, mode.team_size(), expires)
        .expect("queue modes have playable team sizes");
    Metrics::add(&door.metrics.issued, 1);
    Metrics::add(&door.metrics.queue_matches, 1);
    eprintln!(
        "match {:016x}: formed from the {} queue",
        created.match_id,
        mode.id()
    );
    for (seat, (conn, ticket)) in seats.into_iter().zip(created.tickets).enumerate() {
        if let Some((_, sender)) = conns.remove(&conn) {
            let _ = sender.try_send(ToClient::Match {
                mode: mode.id().into(),
                match_id: format!("{:016x}", created.match_id),
                seat,
                server: door.config.match_server.clone(),
                ticket,
                expires,
            });
        }
    }
}

pub(super) async fn upgrade(
    ws: WebSocketUpgrade,
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if !door.config.origins.allows(&headers) {
        Metrics::add(&door.metrics.refused_origin, 1);
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    let address = ingress::client_address(peer, &headers, &door.config.trusted_proxies);
    let Some(slot) = door.queue_open.open(address) else {
        Metrics::add(&door.metrics.refused_rate, 1);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "too many queue connections from this address",
        )
            .into_response();
    };
    ws.max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |socket| connection(socket, door, slot))
}

async fn say(socket: &mut WebSocket, message: &ToClient) -> bool {
    let text = serde_json::to_string(message).expect("messages serialize");
    socket.send(Message::Text(text.into())).await.is_ok()
}

async fn refuse(socket: &mut WebSocket, code: &str, message: &str) {
    let _ = say(socket, &ToClient::error(code, message)).await;
    let _ = socket.send(Message::Close(None)).await;
}

fn decode(message: Message) -> Option<Result<FromClient, ()>> {
    match message {
        Message::Text(text) => Some(serde_json::from_str(&text).map_err(|_| ())),
        Message::Binary(_) => Some(Err(())),
        Message::Ping(_) | Message::Pong(_) => None,
        Message::Close(_) => Some(Err(())),
    }
}

async fn connection(mut socket: WebSocket, door: Arc<Door>, _slot: OpenSlot) {
    let conn = door.next_conn.fetch_add(1, Ordering::Relaxed);
    let mut rate = RateWindow::new(MESSAGES_PER_SECOND, Instant::now());
    let mut stop = door.shutdown.subscribe();
    let first = tokio::time::timeout(JOIN_TIMEOUT, async {
        loop {
            match socket.recv().await {
                Some(Ok(message)) => match decode(message) {
                    None => continue,
                    Some(decoded) => return Some(decoded),
                },
                _ => return None,
            }
        }
    })
    .await;
    let (mut mode, guest) = match first {
        Err(_) => return refuse(&mut socket, "bad_request", "Send join within 5 s.").await,
        Ok(None) => return,
        Ok(Some(Ok(FromClient::Join { mode, guest }))) => {
            match (Mode::from_id(&mode), parse_guest(&guest)) {
                (Some(mode), Some(guest)) => (mode, guest),
                _ => {
                    return refuse(&mut socket, "bad_request", "Unknown mode or guest id.").await;
                }
            }
        }
        Ok(Some(_)) => return refuse(&mut socket, "bad_request", "Send join first.").await,
    };
    let (sender, mut outbox) = mpsc::channel(OUTBOUND_QUEUE);
    if door
        .queue
        .send(Command::Join {
            conn,
            guest,
            mode,
            out: sender.clone(),
        })
        .await
        .is_err()
    {
        return refuse(&mut socket, "shutdown", "The matchmaker is not running.").await;
    }
    loop {
        tokio::select! {
            // Drop the borrow `wait_for` returns before awaiting the refusal.
            _ = async { let _ = stop.wait_for(|stop| *stop).await; } => {
                refuse(&mut socket, "shutdown", "The matchmaker is restarting. Queue again in a moment.").await;
                break;
            }
            incoming = socket.recv() => {
                let message = match incoming {
                    Some(Ok(message)) => match decode(message) {
                        None => continue,
                        Some(message) => message,
                    },
                    _ => break,
                };
                if !rate.allow(Instant::now()) {
                    refuse(&mut socket, "rate_limited", "Too many messages.").await;
                    break;
                }
                let command = match message {
                    Ok(FromClient::Ready {}) => Command::Ready { conn },
                    Ok(FromClient::Leave {}) => break,
                    Ok(FromClient::Join { mode: next, guest: again }) => {
                        match (Mode::from_id(&next), parse_guest(&again)) {
                            (Some(next), Some(again)) if again == guest => {
                                mode = next;
                                Command::Join { conn, guest, mode, out: sender.clone() }
                            }
                            _ => {
                                refuse(&mut socket, "bad_request", "Unknown mode or a different guest id.").await;
                                break;
                            }
                        }
                    }
                    Err(()) => {
                        refuse(&mut socket, "bad_request", "Unreadable message.").await;
                        break;
                    }
                };
                if door.queue.send(command).await.is_err() {
                    break;
                }
            }
            outgoing = outbox.recv() => {
                let Some(message) = outgoing else { break };
                let last = matches!(message, ToClient::Match { .. } | ToClient::Error { .. });
                if !say(&mut socket, &message).await {
                    break;
                }
                if last {
                    let _ = socket.send(Message::Close(None)).await;
                    // The seat is handed out (or the entry is gone): nothing
                    // for this connection to leave.
                    return;
                }
            }
        }
    }
    let _ = door.queue.send(Command::Leave { conn }).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_ids_are_exactly_32_hex_digits() {
        assert_eq!(
            parse_guest(&"0f".repeat(16)),
            Some(0x0f0f_0f0f_0f0f_0f0f_0f0f_0f0f_0f0f_0f0f)
        );
        assert_eq!(parse_guest(&"0".repeat(31)), None);
        assert_eq!(parse_guest(&"0".repeat(33)), None);
        assert_eq!(parse_guest(&format!("+{}", "0".repeat(31))), None);
        assert_eq!(parse_guest(&"g".repeat(32)), None);
    }

    #[test]
    fn client_messages_parse_strictly() {
        let join: FromClient =
            serde_json::from_str(r#"{"type":"join","mode":"1v1","guest":"ab"}"#).unwrap();
        assert!(matches!(join, FromClient::Join { .. }));
        assert!(matches!(
            serde_json::from_str(r#"{"type":"ready"}"#).unwrap(),
            FromClient::Ready {}
        ));
        for bad in [
            r#"{"type":"start"}"#,
            r#"{"type":"ready","seat":1}"#,
            r#"{"mode":"1v1"}"#,
            "ready",
        ] {
            assert!(serde_json::from_str::<FromClient>(bad).is_err(), "{bad}");
        }
        assert_eq!(
            serde_json::to_string(&ToClient::Queued {
                mode: "solo-2v2".into(),
                waiting: 3
            })
            .unwrap(),
            r#"{"type":"queued","mode":"solo-2v2","waiting":3}"#
        );
    }
}
