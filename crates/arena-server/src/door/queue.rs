//! `GET /api/queue`: parties and queues over a WebSocket (ADR 0046).
//!
//! The browser sends small JSON messages and receives the same.
//!
//! From the browser:
//! - `{"type":"hello","guest":"<32 hex>"}` first, within 5 s.
//! - `{"type":"create_party"}`, `{"type":"join_party","code":"K7P2QX"}`,
//!   `{"type":"leave_party"}`.
//! - `{"type":"queue","mode":"1v1"|"2v2"|"3v3"|"solo-2v2"|"solo-3v3"}`
//!   (party leader only), `{"type":"cancel"}`.
//! - `{"type":"ready"}` after `found`, which the game sends automatically.
//!
//! From the door:
//! - `party` describes the player's party whenever it changes.
//! - `queued` (with how many wait in that mode) while searching, every 2 s.
//! - `found` when a ready check starts, `requeued` when it failed because of
//!   someone else, and `stopped` when the party's search ended.
//! - `refused` turns a request down; the connection stays open.
//! - `match` carries the player's own seat ticket, and `error` comes before
//!   the door closes the connection.
//!
//! One task owns the [`Matchmaker`]; connections talk to it over channels.

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
use crate::matchmaker::{CODE_LENGTH, Closed, Guest, Matchmaker, Mode, Out, PartyView, QueueConn};
use crate::metrics::Metrics;
use crate::ticket;

/// Largest message a queue client may send.
const MAX_MESSAGE: usize = 512;
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const MESSAGES_PER_SECOND: u32 = 10;
const STATUS_EVERY: Duration = Duration::from_secs(2);
const TICK_EVERY: Duration = Duration::from_millis(250);
/// Messages buffered per connection; status updates are skipped when full.
const OUTBOUND_QUEUE: usize = 32;

/// Empty structs rather than unit variants, so extra fields are refused.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum FromClient {
    Hello { guest: String },
    CreateParty {},
    JoinParty { code: String },
    LeaveParty {},
    Queue { mode: String },
    Cancel {},
    Ready {},
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToClient {
    /// The player's party: its invite code (none when on their own), size,
    /// whether this player leads it, and the queue it is in.
    Party {
        code: Option<String>,
        members: usize,
        leader: bool,
        queue: Option<String>,
    },
    /// Searching; `waiting` players (including this party) want `mode`.
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
    /// The party is no longer searching, and why.
    Stopped {
        reason: String,
    },
    /// A request was turned down; nothing changed.
    Refused {
        code: String,
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
    fn party(view: PartyView) -> Self {
        Self::Party {
            code: view.code,
            members: view.members,
            leader: view.leader,
            queue: view.queue.map(|mode| mode.id().into()),
        }
    }
}

pub(super) enum Command {
    Hello {
        conn: QueueConn,
        guest: Guest,
        out: mpsc::Sender<ToClient>,
    },
    CreateParty {
        conn: QueueConn,
    },
    JoinParty {
        conn: QueueConn,
        code: String,
    },
    LeaveParty {
        conn: QueueConn,
    },
    Queue {
        conn: QueueConn,
        mode: Mode,
    },
    Cancel {
        conn: QueueConn,
    },
    Ready {
        conn: QueueConn,
    },
    Gone {
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

type Senders = HashMap<QueueConn, mpsc::Sender<ToClient>>;

fn send(conns: &Senders, conn: QueueConn, message: ToClient) {
    if let Some(sender) = conns.get(&conn) {
        // A full outbox means a stalled client; it misses this message.
        let _ = sender.try_send(message);
    }
}

/// The task that owns the matchmaker. Ends when the door stops, telling
/// every connected player.
pub(super) async fn run(door: Arc<Door>, mut commands: mpsc::Receiver<Command>) {
    let mut mm = Matchmaker::new(door.config.ready_timeout, random_below);
    let mut conns: Senders = HashMap::new();
    let mut tick = tokio::time::interval(TICK_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut status_at = Instant::now() + STATUS_EVERY;
    let mut stop = door.shutdown.subscribe();
    loop {
        let now = Instant::now();
        // Searches that started now get a status at once.
        let mut announce_status = false;
        let out = tokio::select! {
            _ = stop.wait_for(|stop| *stop) => break,
            _ = tick.tick() => mm.tick(now),
            command = commands.recv() => match command {
                None => break,
                Some(Command::Hello { conn, guest, out }) => {
                    conns.insert(conn, out);
                    mm.connect(conn, guest, now)
                }
                Some(Command::CreateParty { conn }) => mm.create_party(conn, now),
                Some(Command::JoinParty { conn, code }) => mm.join_party(conn, &code, now),
                Some(Command::LeaveParty { conn }) => mm.leave_party(conn, now),
                Some(Command::Queue { conn, mode }) => {
                    announce_status = true;
                    mm.queue(conn, mode, now)
                }
                Some(Command::Cancel { conn }) => mm.cancel(conn),
                Some(Command::Ready { conn }) => mm.ready(conn, now),
                Some(Command::Gone { conn }) => {
                    conns.remove(&conn);
                    mm.disconnect(conn, now)
                }
            },
        };
        for action in out {
            match action {
                Out::Party { conn, view } => send(&conns, conn, ToClient::party(view)),
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
                Out::Stopped { conn, reason } => send(
                    &conns,
                    conn,
                    ToClient::Stopped {
                        reason: reason.code().into(),
                    },
                ),
                Out::Refused { conn, reason } => send(
                    &conns,
                    conn,
                    ToClient::Refused {
                        code: reason.code().into(),
                    },
                ),
                Out::Close { conn, reason } => {
                    let message = match reason {
                        Closed::Replaced => ToClient::error(
                            "replaced",
                            "You opened the game in another tab or window.",
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
        if announce_status || now >= status_at {
            if now >= status_at {
                status_at = now + STATUS_EVERY;
            }
            let waiting = mm.waiting();
            for (conn, mode) in mm.searching() {
                send(
                    &conns,
                    conn,
                    ToClient::Queued {
                        mode: mode.id().into(),
                        waiting: waiting[mode.index()],
                    },
                );
            }
        }
        door.metrics
            .queued
            .store(mm.waiting().iter().sum::<usize>() as u64, Ordering::Relaxed);
    }
    for sender in conns.values() {
        let _ = sender.try_send(ToClient::error(
            "shutdown",
            "The matchmaker is restarting. Try again in a moment.",
        ));
    }
}

fn start_match(door: &Door, conns: &mut Senders, mode: Mode, seats: Vec<QueueConn>) {
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
        if let Some(sender) = conns.remove(&conn) {
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
        Message::Binary(_) | Message::Close(_) => Some(Err(())),
        Message::Ping(_) | Message::Pong(_) => None,
    }
}

/// A request after `hello`, or why the connection must close.
fn command(conn: QueueConn, message: FromClient) -> Result<Command, &'static str> {
    Ok(match message {
        FromClient::Hello { .. } => return Err("Send hello only once."),
        FromClient::CreateParty {} => Command::CreateParty { conn },
        FromClient::JoinParty { code } if code.trim().len() == CODE_LENGTH => {
            Command::JoinParty { conn, code }
        }
        // A wrong-length code cannot exist; refuse it like an unknown one.
        FromClient::JoinParty { .. } => Command::JoinParty {
            conn,
            code: String::new(),
        },
        FromClient::LeaveParty {} => Command::LeaveParty { conn },
        FromClient::Queue { mode } => Command::Queue {
            conn,
            mode: Mode::from_id(&mode).ok_or("Unknown queue.")?,
        },
        FromClient::Cancel {} => Command::Cancel { conn },
        FromClient::Ready {} => Command::Ready { conn },
    })
}

async fn connection(mut socket: WebSocket, door: Arc<Door>, _slot: OpenSlot) {
    let conn = door.next_conn.fetch_add(1, Ordering::Relaxed);
    let mut rate = RateWindow::new(MESSAGES_PER_SECOND, Instant::now());
    let mut stop = door.shutdown.subscribe();
    let first = tokio::time::timeout(HELLO_TIMEOUT, async {
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
    let guest = match first {
        Err(_) => return refuse(&mut socket, "bad_request", "Send hello within 5 s.").await,
        Ok(None) => return,
        Ok(Some(Ok(FromClient::Hello { guest }))) => match parse_guest(&guest) {
            Some(guest) => guest,
            None => return refuse(&mut socket, "bad_request", "Unreadable guest id.").await,
        },
        Ok(Some(_)) => return refuse(&mut socket, "bad_request", "Send hello first.").await,
    };
    let (sender, mut outbox) = mpsc::channel(OUTBOUND_QUEUE);
    let hello = Command::Hello {
        conn,
        guest,
        out: sender,
    };
    if door.queue.send(hello).await.is_err() {
        return refuse(&mut socket, "shutdown", "The matchmaker is not running.").await;
    }
    loop {
        tokio::select! {
            // Drop the borrow `wait_for` returns before awaiting the refusal.
            _ = async { let _ = stop.wait_for(|stop| *stop).await; } => {
                refuse(&mut socket, "shutdown", "The matchmaker is restarting. Try again in a moment.").await;
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
                let command = match message.map_err(|()| "Unreadable message.").and_then(|m| command(conn, m)) {
                    Ok(command) => command,
                    Err(why) => {
                        refuse(&mut socket, "bad_request", why).await;
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
                    // Seated, or already out of the matchmaker.
                    return;
                }
            }
        }
    }
    let _ = door.queue.send(Command::Gone { conn }).await;
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
        let parse = |text: &str| serde_json::from_str::<FromClient>(text);
        assert!(matches!(
            parse(r#"{"type":"hello","guest":"ab"}"#),
            Ok(FromClient::Hello { .. })
        ));
        assert!(matches!(
            parse(r#"{"type":"join_party","code":"K7P2QX"}"#),
            Ok(FromClient::JoinParty { .. })
        ));
        assert!(matches!(
            parse(r#"{"type":"ready"}"#),
            Ok(FromClient::Ready {})
        ));
        for bad in [
            r#"{"type":"start"}"#,
            r#"{"type":"ready","seat":1}"#,
            r#"{"type":"queue","mode":"1v1","party":2}"#,
            r#"{"mode":"1v1"}"#,
            "ready",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert!(command(1, parse(r#"{"type":"queue","mode":"4v4"}"#).unwrap()).is_err());
        assert!(command(1, parse(r#"{"type":"hello","guest":"ab"}"#).unwrap()).is_err());
        assert_eq!(
            serde_json::to_string(&ToClient::party(PartyView {
                code: Some("K7P2QX".into()),
                members: 2,
                leader: true,
                queue: Some(Mode::Team2v2),
            }))
            .unwrap(),
            r#"{"type":"party","code":"K7P2QX","members":2,"leader":true,"queue":"2v2"}"#
        );
    }
}
