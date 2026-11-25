//! `GET /api/queue`: parties, queues, and private custom lobbies over a WebSocket (ADR 0046).
//!
//! The browser sends small JSON messages and receives the same.
//!
//! From the browser:
//! - `{"type":"hello","guest":"<32 hex>"}` first, within 5 s.
//! - `{"type":"create_party"}`, `{"type":"join_party","code":"K7P2QX"}`,
//!   `{"type":"leave_party"}`.
//! - `{"type":"queue","mode":"1v1"|"2v2"|"3v3"}` (party leader only),
//!   `{"type":"cancel"}`.
//! - `{"type":"accept"}` or `{"type":"decline"}` when a match is ready.
//! - `{"type":"create_lobby","mode":"1v1"|"2v2"|"3v3"}`,
//!   `{"type":"join_lobby","code":"K7P2QX"}`, `{"type":"leave_lobby"}`.
//! - `{"type":"set_lobby_slot","slot":"team_a"|"team_b"|"spectator"}`.
//! - Host-only custom lobby controls: `move_lobby_member`, `kick_lobby_member`,
//!   `transfer_lobby_host`, `set_lobby_mode`, `set_lobby_settings`, and
//!   `start_lobby` (rate-limited per client address via `door.creations`).
//!
//! From the door:
//! - `party` describes the player's party whenever it changes.
//! - `queued` while searching, every 2 s: how many wait in that queue, how
//!   long this search has run and the expected wait (when known).
//! - `ready_check` when a match is ready to accept and whenever someone
//!   accepts: how many accepted out of how many, whether this player did,
//!   and the time left. `requeued` when it failed because of someone else,
//!   and `stopped` when the party's search ended.
//! - `lobby` describes the player's private custom lobby whenever it changes;
//!   `lobby_left` notes leaving or being removed.
//! - `refused` turns a request down; the connection stays open.
//! - `match` carries the player's own seat ticket (or watch-only spectator
//!   ticket when `spectator` is true). The connection stays open, so the party
//!   or custom lobby stays together when the match is over.
//! - `error` comes before the door closes the connection.
//!
//! One task owns the [`Matchmaker`]; connections talk to it over channels.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
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
use tracing::Span;

use super::{Door, unix_now};
use crate::RateWindow;
use crate::ingress;
use crate::logging::MatchId;
use crate::matchmaker::{
    CODE_LENGTH, Closed, Guest, LobbySlot, LobbyView, Matchmaker, Mode, Out, PartyView, QueueConn,
};
use crate::metrics::Metrics;
use crate::ticket::{self, Holder, MatchSettings};
use arena_protocol::Handle;

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
    Hello {
        guest: String,
    },
    CreateParty {},
    JoinParty {
        code: String,
    },
    LeaveParty {},
    Queue {
        mode: String,
    },
    Cancel {},
    Accept {},
    Decline {},
    CreateLobby {
        #[serde(default)]
        mode: Option<String>,
    },
    JoinLobby {
        code: String,
    },
    LeaveLobby {},
    SetLobbySlot {
        slot: LobbySlot,
    },
    MoveLobbyMember {
        member: u32,
        slot: LobbySlot,
    },
    KickLobbyMember {
        member: u32,
    },
    TransferLobbyHost {
        member: u32,
    },
    SetLobbyMode {
        mode: String,
    },
    SetLobbySettings {
        settings: MatchSettings,
    },
    StartLobby {},
    /// Invite a friend (by `Name#1234`) to this player's lobby or party.
    Invite {
        handle: String,
    },
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
    /// Searching: `waiting` players (including this party) want `mode`;
    /// this search has run `waited_ms`; recent matches took about
    /// `expected_ms` to find (`None` until there have been a few).
    Queued {
        mode: String,
        waiting: usize,
        waited_ms: u64,
        expected_ms: Option<u64>,
    },
    /// A match is ready: accept within `remaining_ms`. Sent again whenever
    /// someone accepts.
    ReadyCheck {
        mode: String,
        accepted: usize,
        total: usize,
        /// Whether this player has accepted.
        you: bool,
        remaining_ms: u64,
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
    /// This player's seat (or watch-only spectator pass): join `server` with
    /// `ticket` (hex). The match id (hex), seat and spectator flag are for
    /// display and bug reports; the signed ticket decides.
    Match {
        mode: String,
        match_id: String,
        seat: usize,
        #[serde(default)]
        spectator: bool,
        server: String,
        ticket: String,
        expires: u64,
    },
    /// Current state of the player's private custom lobby.
    Lobby(LobbyView),
    /// The player left or was removed from their private custom lobby.
    LobbyLeft {
        reason: String,
    },
    Error {
        code: String,
        message: String,
    },
    /// A friend invites this player: `kind` is `party` or `lobby`, and
    /// `code` joins it with `join_party` / `join_lobby`.
    Invite {
        from: String,
        kind: String,
        code: String,
    },
    /// The invite reached `to` (on at least one open connection).
    InviteSent {
        to: String,
    },
    /// Friend requests or friendships changed: fetch `/api/friends`.
    FriendsChanged {},
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
        address: IpAddr,
        /// The logged-in account (ADR 0055) and the handle to show, signed
        /// into this player's tickets.
        holder: Holder,
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
    Accept {
        conn: QueueConn,
    },
    Decline {
        conn: QueueConn,
    },
    CreateLobby {
        conn: QueueConn,
        mode: Mode,
    },
    JoinLobby {
        conn: QueueConn,
        code: String,
    },
    LeaveLobby {
        conn: QueueConn,
    },
    SetLobbySlot {
        conn: QueueConn,
        slot: LobbySlot,
    },
    MoveLobbyMember {
        conn: QueueConn,
        member: u32,
        slot: LobbySlot,
    },
    KickLobbyMember {
        conn: QueueConn,
        member: u32,
    },
    TransferLobbyHost {
        conn: QueueConn,
        member: u32,
    },
    SetLobbyMode {
        conn: QueueConn,
        mode: Mode,
    },
    SetLobbySettings {
        conn: QueueConn,
        settings: MatchSettings,
    },
    StartLobby {
        conn: QueueConn,
    },
    Gone {
        conn: QueueConn,
    },
    /// The account was deleted or logged out everywhere: its open queue
    /// connections stop signing it into tickets at once (ADR 0055).
    Revoke {
        account: u64,
    },
    /// The account picked a new handle: its open connections use it.
    Rename {
        account: u64,
        handle: Handle,
    },
    /// Deliver an invite from `conn` to the friend `to` (already checked).
    Invite {
        conn: QueueConn,
        /// The inviter's account as the connection last knew it; refused
        /// unless the connection still holds it (logged out everywhere).
        from: u64,
        to: u64,
        to_handle: String,
    },
    /// Tell every open connection of `account`.
    Notify {
        account: u64,
        message: ToClient,
    },
    /// Where these accounts are now, in order.
    Presence {
        accounts: Vec<u64>,
        reply: tokio::sync::oneshot::Sender<Vec<Presence>>,
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

/// Each searching player's trace (ADR 0054): a `queue.search` span from
/// entering a queue until a ticket, a stop or leaving, with a child span per
/// step (ready check offered, accepted, declined, requeued, ticket minted).
/// Spans only: these steps write no log lines.
#[derive(Default)]
struct Searches(HashMap<QueueConn, Span>);

impl Searches {
    fn observe(&mut self, out: &Out) {
        match out {
            Out::Party { conn, view } => match view.queue {
                Some(mode) => {
                    self.0.entry(*conn).or_insert_with(|| {
                        crate::control_span!(
                            parent: None,
                            "queue.search",
                            conn_id = *conn,
                            mode = mode.id(),
                            party_size = view.members,
                            outcome = tracing::field::Empty,
                            match_id = tracing::field::Empty
                        )
                    });
                }
                None => self.end(*conn, "idle"),
            },
            Out::ReadyCheck { conn, check } if check.accepted == 0 => {
                self.step(*conn, "ready_check.offer");
            }
            Out::Requeued { conn, .. } => self.step(*conn, "queue.requeue"),
            Out::Stopped { conn, reason } => self.end(*conn, reason.code()),
            _ => {}
        }
    }

    fn step(&self, conn: QueueConn, name: &'static str) {
        if let Some(search) = self.0.get(&conn) {
            drop(crate::control_span!(parent: search, "queue.step", step = name));
        }
    }

    fn end(&mut self, conn: QueueConn, outcome: &str) {
        if let Some(search) = self.0.remove(&conn) {
            search.record("outcome", outcome);
        }
    }

    fn matched(&mut self, conn: QueueConn, match_id: u64) {
        if let Some(search) = self.0.remove(&conn) {
            drop(crate::control_span!(
                parent: &search,
                "queue.step",
                step = "ticket.mint",
                match_id = %MatchId(match_id)
            ));
            search.record("outcome", "matched");
            search.record("match_id", tracing::field::display(MatchId(match_id)));
        }
    }
}

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
    let mut addresses: HashMap<QueueConn, IpAddr> = HashMap::new();
    let mut holders: HashMap<QueueConn, Holder> = HashMap::new();
    // When each connection last got a match ticket (for presence).
    let mut matched_at: HashMap<QueueConn, Instant> = HashMap::new();
    // Each logged-in account's open connections, kept with `holders`.
    let mut by_account: HashMap<u64, Vec<QueueConn>> = HashMap::new();
    let mut searches = Searches::default();
    let mut tick = tokio::time::interval(TICK_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut status_at = Instant::now() + STATUS_EVERY;
    let mut stop = door.shutdown.subscribe();
    loop {
        let now = Instant::now();
        // A party that just queued gets an immediate status; everyone else
        // updates on the periodic cadence (avoiding an O(N^2) broadcast storm
        // on queue arrivals).
        let mut queued_conn = None;
        let out = tokio::select! {
            _ = stop.wait_for(|stop| *stop) => break,
            _ = tick.tick() => mm.tick(now),
            command = commands.recv() => match command {
                None => break,
                Some(Command::Hello { conn, guest, address, holder, out }) => {
                    conns.insert(conn, out);
                    addresses.insert(conn, address);
                    if let Some(account) = holder.account {
                        by_account.entry(account).or_default().push(conn);
                    }
                    holders.insert(conn, holder);
                    mm.connect(conn, guest, now)
                }
                // Never record the invite code: it lets anyone join.
                Some(Command::CreateParty { conn }) => {
                    crate::control_span!(parent: None, "party.create", conn_id = conn)
                        .in_scope(|| mm.create_party(conn, now))
                }
                Some(Command::JoinParty { conn, code }) => {
                    crate::control_span!(parent: None, "party.join", conn_id = conn)
                        .in_scope(|| mm.join_party(conn, &code, now))
                }
                Some(Command::LeaveParty { conn }) => mm.leave_party(conn, now),
                Some(Command::Queue { conn, mode }) => {
                    queued_conn = Some(conn);
                    mm.queue(conn, mode, now)
                }
                Some(Command::Cancel { conn }) => mm.cancel(conn),
                Some(Command::Accept { conn }) => {
                    searches.step(conn, "ready_check.accept");
                    mm.accept(conn, now)
                }
                Some(Command::Decline { conn }) => {
                    searches.step(conn, "ready_check.decline");
                    mm.decline(conn, now)
                }
                Some(Command::CreateLobby { conn, mode }) => mm.create_lobby(conn, mode, now),
                Some(Command::JoinLobby { conn, code }) => mm.join_lobby(conn, &code, now),
                Some(Command::LeaveLobby { conn }) => mm.leave_lobby(conn),
                Some(Command::SetLobbySlot { conn, slot }) => mm.set_lobby_slot(conn, slot),
                Some(Command::MoveLobbyMember { conn, member, slot }) => {
                    mm.move_lobby_member(conn, member, slot)
                }
                Some(Command::KickLobbyMember { conn, member }) => {
                    mm.kick_lobby_member(conn, member)
                }
                Some(Command::TransferLobbyHost { conn, member }) => {
                    mm.transfer_lobby_host(conn, member)
                }
                Some(Command::SetLobbyMode { conn, mode }) => mm.set_lobby_mode(conn, mode),
                Some(Command::SetLobbySettings { conn, settings }) => {
                    mm.set_lobby_settings(conn, settings)
                }
                Some(Command::StartLobby { conn }) => mm.start_lobby(conn),
                Some(Command::Revoke { account }) => {
                    by_account.remove(&account);
                    for holder in holders.values_mut() {
                        if holder.account == Some(account) {
                            *holder = Holder {
                                account: None,
                                handle: Some(super::auth::guest_handle()),
                                admin: false,
                            };
                        }
                    }
                    Vec::new()
                }
                Some(Command::Rename { account, handle }) => {
                    for holder in holders.values_mut() {
                        if holder.account == Some(account) {
                            holder.handle = Some(handle);
                        }
                    }
                    Vec::new()
                }
                Some(Command::Invite { conn, from, to, to_handle }) => {
                    if holders.get(&conn).and_then(|h| h.account) == Some(from) {
                        invite(&mut mm, &conns, &holders, &by_account, now, conn, to, to_handle)
                    } else {
                        send(&conns, conn, ToClient::Refused { code: "not_logged_in".into() });
                        Vec::new()
                    }
                }
                Some(Command::Notify { account, message }) => {
                    for conn in by_account.get(&account).into_iter().flatten() {
                        send(&conns, *conn, message.clone());
                    }
                    Vec::new()
                }
                Some(Command::Presence { accounts, reply }) => {
                    let presence = accounts
                        .iter()
                        .map(|account| presence(&mm, &by_account, &matched_at, now, *account))
                        .collect();
                    let _ = reply.send(presence);
                    Vec::new()
                }
                Some(Command::Gone { conn }) => {
                    searches.end(conn, "left");
                    conns.remove(&conn);
                    addresses.remove(&conn);
                    unindex(&mut by_account, &holders, conn);
                    holders.remove(&conn);
                    matched_at.remove(&conn);
                    mm.disconnect(conn, now)
                }
            },
        };
        for action in out {
            searches.observe(&action);
            match action {
                Out::Party { conn, view } => send(&conns, conn, ToClient::party(view)),
                Out::ReadyCheck { conn, check } => send(
                    &conns,
                    conn,
                    ToClient::ReadyCheck {
                        mode: check.mode.id().into(),
                        accepted: check.accepted,
                        total: check.total,
                        you: check.you,
                        remaining_ms: check.remaining.as_millis() as u64,
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
                Out::Close {
                    conn,
                    reason: Closed::Replaced,
                } => {
                    // The replaced connection's task leaves without a Gone.
                    searches.end(conn, "replaced");
                    send(
                        &conns,
                        conn,
                        ToClient::error(
                            "replaced",
                            "You opened the game in another tab or window.",
                        ),
                    );
                    conns.remove(&conn);
                    addresses.remove(&conn);
                    unindex(&mut by_account, &holders, conn);
                    holders.remove(&conn);
                    matched_at.remove(&conn);
                }
                Out::Start { mode, seats } => {
                    let match_id = start_match(&door, &conns, &holders, mode, &seats);
                    for conn in &seats {
                        matched_at.insert(*conn, now);
                    }
                    for conn in seats {
                        searches.matched(conn, match_id);
                    }
                }
                Out::Lobby { conn, view } => send(&conns, conn, ToClient::Lobby(view)),
                Out::LobbyLeft { conn, reason } => send(
                    &conns,
                    conn,
                    ToClient::LobbyLeft {
                        reason: reason.code().into(),
                    },
                ),
                Out::StartCustom {
                    host,
                    mode,
                    settings,
                    seats,
                    spectators,
                } => {
                    // Spectators count only when the lobby lets them watch.
                    let watchers: &[QueueConn] = if settings.allow_spectators {
                        &spectators
                    } else {
                        &[]
                    };
                    let players: Vec<QueueConn> = seats.iter().chain(watchers).copied().collect();
                    if start_custom_match(
                        &door, &conns, &addresses, &holders, now, host, mode, settings, seats,
                        spectators,
                    ) {
                        for conn in players {
                            matched_at.insert(conn, now);
                        }
                    }
                }
            }
        }
        if now >= status_at {
            status_at = now + STATUS_EVERY;
            let waiting = mm.waiting();
            let expected = Mode::ALL.map(|mode| mm.expected_wait(mode));
            for (conn, mode, since) in mm.searching() {
                send(
                    &conns,
                    conn,
                    ToClient::Queued {
                        mode: mode.id().into(),
                        waiting: waiting[mode.index()],
                        waited_ms: now.saturating_duration_since(since).as_millis() as u64,
                        expected_ms: expected[mode.index()].map(|wait| wait.as_millis() as u64),
                    },
                );
            }
        } else if let Some(conn) = queued_conn
            && let Some((mode, since, members)) = mm.searching_party(conn)
        {
            let waiting = mm.waiting()[mode.index()];
            let expected_ms = mm.expected_wait(mode).map(|wait| wait.as_millis() as u64);
            let waited_ms = now.saturating_duration_since(since).as_millis() as u64;
            for member in members {
                send(
                    &conns,
                    member,
                    ToClient::Queued {
                        mode: mode.id().into(),
                        waiting,
                        waited_ms,
                        expected_ms,
                    },
                );
            }
        }
        publish_metrics(&door, &mut mm, conns.len());
        debug_assert!(
            searches.0.len() <= conns.len(),
            "a search outlived its connection"
        );
    }
    for sender in conns.values() {
        let _ = sender.try_send(ToClient::error(
            "shutdown",
            "The matchmaker is restarting. Try again in a moment.",
        ));
    }
}

/// Copies the matchmaker's gauges and totals into the door's `/metrics`.
fn publish_metrics(door: &Door, mm: &mut Matchmaker, connections: usize) {
    let metrics = &door.metrics;
    let waiting = mm.waiting();
    metrics
        .queued
        .store(waiting.iter().sum::<usize>() as u64, Ordering::Relaxed);
    for (gauge, count) in metrics.queued_by_mode.iter().zip(waiting) {
        gauge.store(count as u64, Ordering::Relaxed);
    }
    metrics
        .queue_connections
        .store(connections as u64, Ordering::Relaxed);
    metrics
        .parties
        .store(mm.parties() as u64, Ordering::Relaxed);
    metrics
        .lobbies
        .store(mm.lobbies() as u64, Ordering::Relaxed);
    let stats = mm.stats();
    for (counter, total) in [
        (&metrics.checks_started, stats.checks_started),
        (&metrics.checks_completed, stats.checks_completed),
        (&metrics.checks_failed, stats.checks_failed),
        (&metrics.accepts, stats.accepts),
        (&metrics.declines, stats.declines),
    ] {
        counter.store(total, Ordering::Relaxed);
    }
    for (counter, total) in metrics.searches_by_size.iter().zip(stats.searches_by_size) {
        counter.store(total, Ordering::Relaxed);
    }
    for (_, waited) in mm.take_found_waits() {
        metrics
            .queue_wait
            .0
            .record(u64::try_from(waited.as_millis()).unwrap_or(u64::MAX));
    }
}

/// Where a player is, for their friends (social-identity step 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Presence {
    Offline,
    Online,
    Searching,
    InLobby,
    /// Got a match ticket within [`IN_MATCH`] (shown unless searching right
    /// now; the door never hears when a match ends).
    InMatch,
}

impl Presence {
    pub(super) fn id(self) -> &'static str {
        match self {
            Presence::Offline => "offline",
            Presence::Online => "online",
            Presence::Searching => "searching",
            Presence::InLobby => "in_lobby",
            Presence::InMatch => "in_match",
        }
    }
}

/// How long after a match ticket a player counts as in a match: a round
/// (20 min) and a little. The door never hears when a match ends.
const IN_MATCH: Duration = Duration::from_secs(21 * 60);

/// Drops `conn` from the account index.
fn unindex(
    by_account: &mut HashMap<u64, Vec<QueueConn>>,
    holders: &HashMap<QueueConn, Holder>,
    conn: QueueConn,
) {
    let Some(account) = holders.get(&conn).and_then(|holder| holder.account) else {
        return;
    };
    if let Some(conns) = by_account.get_mut(&account) {
        conns.retain(|held| *held != conn);
        if conns.is_empty() {
            by_account.remove(&account);
        }
    }
}

fn presence(
    mm: &Matchmaker,
    by_account: &HashMap<u64, Vec<QueueConn>>,
    matched_at: &HashMap<QueueConn, Instant>,
    now: Instant,
    account: u64,
) -> Presence {
    by_account
        .get(&account)
        .into_iter()
        .flatten()
        .map(|conn| {
            if mm.is_searching(*conn) {
                Presence::Searching
            } else if matched_at
                .get(conn)
                .is_some_and(|at| now.saturating_duration_since(*at) < IN_MATCH)
            {
                Presence::InMatch
            } else if mm.in_custom_lobby(*conn) {
                Presence::InLobby
            } else {
                Presence::Online
            }
        })
        .max_by_key(|presence| match presence {
            Presence::Offline => 0,
            Presence::Online => 1,
            Presence::InLobby => 2,
            Presence::Searching => 3,
            Presence::InMatch => 4,
        })
        .unwrap_or(Presence::Offline)
}

/// Delivers a friend's invite: the inviter's lobby or party code, to every
/// open connection of `to`. Refuses `offline` when there is none.
#[allow(clippy::too_many_arguments)]
fn invite(
    mm: &mut Matchmaker,
    conns: &Senders,
    holders: &HashMap<QueueConn, Holder>,
    by_account: &HashMap<u64, Vec<QueueConn>>,
    now: Instant,
    conn: QueueConn,
    to: u64,
    to_handle: String,
) -> Vec<Out> {
    let targets: Vec<QueueConn> = by_account
        .get(&to)
        .into_iter()
        .flatten()
        .copied()
        .filter(|target| *target != conn)
        .collect();
    let from = holders.get(&conn).and_then(|holder| holder.handle);
    let (Some(from), false) = (from, targets.is_empty()) else {
        send(
            conns,
            conn,
            ToClient::Refused {
                code: "offline".into(),
            },
        );
        return Vec::new();
    };
    let (code, out) = mm.invite_code(conn, now);
    let Some(code) = code else {
        // The matchmaker said why (in a ready check, in a lobby, ...).
        return out;
    };
    let (kind, code) = match code {
        crate::matchmaker::Invite::Party(code) => ("party", code),
        crate::matchmaker::Invite::Lobby(code) => ("lobby", code),
    };
    for target in targets {
        send(
            conns,
            target,
            ToClient::Invite {
                from: from.to_string(),
                kind: kind.into(),
                code: code.clone(),
            },
        );
    }
    send(conns, conn, ToClient::InviteSent { to: to_handle });
    out
}

/// Issues a queue match's tickets; returns its id.
fn start_match(
    door: &Door,
    conns: &Senders,
    holders: &HashMap<QueueConn, Holder>,
    mode: Mode,
    seats: &[QueueConn],
) -> u64 {
    let expires = unix_now() + door.config.ticket_ttl.as_secs();
    let seat_accounts: Vec<Holder> = seats
        .iter()
        .map(|conn| holders.get(conn).copied().unwrap_or_default())
        .collect();
    let created = ticket::issue_seats(
        &door.config.key,
        mode.team_size(),
        None,
        &seat_accounts,
        &[],
        expires,
    )
    .expect("queue modes have playable team sizes");
    Metrics::add(&door.metrics.issued, 1);
    Metrics::add(&door.metrics.queue_matches, 1);
    Metrics::add(&door.metrics.tickets, created.tickets.len() as u64);
    tracing::info!(
        match_id = %MatchId(created.match_id),
        mode = mode.id(),
        team_size = mode.team_size(),
        "match formed from the queue"
    );
    for (seat, (conn, ticket)) in seats.iter().zip(created.tickets).enumerate() {
        if let Some(sender) = conns.get(conn) {
            let _ = sender.try_send(ToClient::Match {
                mode: mode.id().into(),
                match_id: format!("{:016x}", created.match_id),
                seat,
                spectator: false,
                server: door.config.match_server.clone(),
                ticket,
                expires,
            });
        }
    }
    created.match_id
}

#[allow(clippy::too_many_arguments)]
fn start_custom_match(
    door: &Door,
    conns: &Senders,
    addresses: &HashMap<QueueConn, IpAddr>,
    holders: &HashMap<QueueConn, Holder>,
    now: Instant,
    host: QueueConn,
    mode: Mode,
    settings: MatchSettings,
    seats: Vec<QueueConn>,
    spectators: Vec<QueueConn>,
) -> bool {
    if let Some(&address) = addresses.get(&host)
        && door.creations.take(address, now).is_err()
    {
        Metrics::add(&door.metrics.refused_rate, 1);
        send(
            conns,
            host,
            ToClient::Refused {
                code: "rate_limited".into(),
            },
        );
        return false;
    }
    let expires = unix_now() + door.config.ticket_ttl.as_secs();
    let spectators = if settings.allow_spectators {
        spectators
    } else {
        Vec::new()
    };
    let account_of = |conn: &QueueConn| holders.get(conn).copied().unwrap_or_default();
    let created = ticket::issue_seats(
        &door.config.key,
        mode.team_size(),
        Some(settings),
        &seats.iter().map(account_of).collect::<Vec<_>>(),
        &spectators.iter().map(account_of).collect::<Vec<_>>(),
        expires,
    )
    .expect("validated custom lobby settings and mode");
    Metrics::add(&door.metrics.issued, 1);
    Metrics::add(&door.metrics.custom_matches, 1);
    Metrics::add(
        &door.metrics.tickets,
        (created.tickets.len() + created.spectator_tickets.len()) as u64,
    );
    crate::control_span!(
        parent: None,
        "lobby.start",
        match_id = %MatchId(created.match_id),
        mode = mode.id()
    )
    .in_scope(|| {
        tracing::info!(
            match_id = %MatchId(created.match_id),
            mode = mode.id(),
            team_size = mode.team_size(),
            map = settings.map.id(),
            round_seconds = settings.round_seconds,
            spectators = spectators.len(),
            "match started from a custom lobby"
        );
    });
    let match_id = format!("{:016x}", created.match_id);
    for (seat, (conn, ticket)) in seats.into_iter().zip(created.tickets).enumerate() {
        if let Some(sender) = conns.get(&conn) {
            let _ = sender.try_send(ToClient::Match {
                mode: mode.id().into(),
                match_id: match_id.clone(),
                seat,
                spectator: false,
                server: door.config.match_server.clone(),
                ticket,
                expires,
            });
        }
    }
    for (conn, ticket) in spectators.into_iter().zip(created.spectator_tickets) {
        if let Some(sender) = conns.get(&conn) {
            let _ = sender.try_send(ToClient::Match {
                mode: mode.id().into(),
                match_id: match_id.clone(),
                seat: 0,
                spectator: true,
                server: door.config.match_server.clone(),
                ticket,
                expires,
            });
        }
    }
    true
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
    // A logged-in player's account and handle, when the page's origin says
    // it is ours; everyone else plays under a temporary guest handle.
    let account = super::auth::account(&door, &headers, true).await;
    let holder = Holder {
        account: account.map(|account| account.id),
        handle: Some(
            account
                .and_then(|account| account.handle)
                .unwrap_or_else(super::auth::guest_handle),
        ),
        // From deploy configuration, as of this connection.
        admin: account
            .zip(door.identity.as_ref())
            .is_some_and(|(account, identity)| identity.is_admin(account.id)),
    };
    ws.max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |mut socket| async move {
            connection(&mut socket, door, address, holder).await;
            // The address keeps its slot until the socket is really gone.
            crate::linger(socket).await;
            drop(slot);
        })
}

async fn say(socket: &mut WebSocket, message: &ToClient) -> bool {
    let text = serde_json::to_string(message).expect("messages serialize");
    socket.send(Message::Text(text.into())).await.is_ok()
}

/// Queue connections closed for floods or garbage: one throttled warning.
static REFUSALS: crate::anomaly::SharedThrottle =
    crate::anomaly::SharedThrottle::new(crate::anomaly::REPORT_EVERY);

async fn refuse(socket: &mut WebSocket, code: &str, message: &str) {
    if matches!(code, "rate_limited" | "bad_request")
        && let Some(suppressed) = REFUSALS.allow(std::time::Instant::now())
    {
        tracing::warn!(
            error_code = code,
            suppressed_reports = suppressed,
            reason = "refused",
            "closed a queue connection for flooding or malformed messages"
        );
    }
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
        FromClient::Accept {} => Command::Accept { conn },
        FromClient::Decline {} => Command::Decline { conn },
        FromClient::CreateLobby { mode } => Command::CreateLobby {
            conn,
            mode: match mode {
                Some(id) => Mode::from_id(&id).ok_or("Unknown match size.")?,
                None => Mode::Duel,
            },
        },
        FromClient::JoinLobby { code } if code.trim().len() == CODE_LENGTH => {
            Command::JoinLobby { conn, code }
        }
        FromClient::JoinLobby { .. } => Command::JoinLobby {
            conn,
            code: String::new(),
        },
        FromClient::LeaveLobby {} => Command::LeaveLobby { conn },
        FromClient::SetLobbySlot { slot } => Command::SetLobbySlot { conn, slot },
        FromClient::MoveLobbyMember { member, slot } => {
            Command::MoveLobbyMember { conn, member, slot }
        }
        FromClient::KickLobbyMember { member } => Command::KickLobbyMember { conn, member },
        FromClient::TransferLobbyHost { member } => Command::TransferLobbyHost { conn, member },
        FromClient::SetLobbyMode { mode } => Command::SetLobbyMode {
            conn,
            mode: Mode::from_id(&mode).ok_or("Unknown match size.")?,
        },
        FromClient::SetLobbySettings { settings } => Command::SetLobbySettings { conn, settings },
        FromClient::StartLobby {} => Command::StartLobby { conn },
        // Needs the store; checked by the connection first.
        FromClient::Invite { .. } => return Err("Invite through the connection."),
    })
}

/// Checks an invite before it reaches the matchmaker: the inviter is logged
/// in and under its invite limit, and `handle` is a friend of theirs. A
/// stranger and a missing handle get the same refusal.
async fn check_invite(
    door: &Door,
    conn: QueueConn,
    account: Option<u64>,
    handle: String,
) -> Result<Command, &'static str> {
    let (Some(identity), Some(account)) = (door.identity.as_ref(), account) else {
        return Err("not_logged_in");
    };
    if door.invites.take(account, Instant::now()).is_err() {
        return Err("rate_limited");
    }
    if handle.len() > 32 {
        return Err("no_such_friend");
    }
    let store = identity.store.clone();
    let lookup = handle.clone();
    let friend = tokio::task::spawn_blocking(move || -> rusqlite::Result<Option<u64>> {
        let Some(other) = store.find_handle(&lookup)? else {
            return Ok(None);
        };
        Ok(store.are_friends(account, other)?.then_some(other))
    })
    .await;
    match friend {
        Ok(Ok(Some(to))) => Ok(Command::Invite {
            conn,
            from: account,
            to,
            to_handle: handle,
        }),
        Ok(Ok(None)) => Err("no_such_friend"),
        _ => Err("unavailable"),
    }
}

async fn connection(socket: &mut WebSocket, door: Arc<Door>, address: IpAddr, holder: Holder) {
    let conn = door.next_conn.fetch_add(1, Ordering::Relaxed);
    let mut rate = RateWindow::new(MESSAGES_PER_SECOND, Instant::now());
    let mut stop = door.shutdown.subscribe();
    enum FirstError {
        RateLimited,
        BadRequest,
    }
    let first = tokio::time::timeout(HELLO_TIMEOUT, async {
        loop {
            match socket.recv().await {
                // Closing is leaving, not a malformed message.
                Some(Ok(Message::Close(_))) => return None,
                Some(Ok(message)) => match decode(message) {
                    None => {
                        if !rate.allow(Instant::now()) {
                            return Some(Err(FirstError::RateLimited));
                        }
                        continue;
                    }
                    Some(decoded) => return Some(decoded.map_err(|()| FirstError::BadRequest)),
                },
                _ => return None,
            }
        }
    })
    .await;
    let guest = match first {
        Err(_) => return refuse(socket, "bad_request", "Send hello within 5 s.").await,
        Ok(None) => return,
        Ok(Some(Err(FirstError::RateLimited))) => {
            return refuse(socket, "rate_limited", "Too many messages.").await;
        }
        Ok(Some(Ok(FromClient::Hello { guest }))) => match parse_guest(&guest) {
            Some(guest) => guest,
            None => return refuse(socket, "bad_request", "Unreadable guest id.").await,
        },
        Ok(Some(_)) => return refuse(socket, "bad_request", "Send hello first.").await,
    };
    let (sender, mut outbox) = mpsc::channel(OUTBOUND_QUEUE);
    let hello = Command::Hello {
        conn,
        guest,
        address,
        holder,
        out: sender,
    };
    if door.queue.send(hello).await.is_err() {
        return refuse(socket, "shutdown", "The matchmaker is not running.").await;
    }
    loop {
        tokio::select! {
            // Drop the borrow `wait_for` returns before awaiting the refusal.
            _ = async { let _ = stop.wait_for(|stop| *stop).await; } => {
                refuse(socket, "shutdown", "The matchmaker is restarting. Try again in a moment.").await;
                break;
            }
            incoming = socket.recv() => {
                let message = match incoming {
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(message)) => {
                        if !rate.allow(Instant::now()) {
                            refuse(socket, "rate_limited", "Too many messages.").await;
                            break;
                        }
                        match decode(message) {
                            None => continue,
                            Some(message) => message,
                        }
                    }
                    _ => break,
                };
                // Invites need the store, so they are checked here.
                let message = match message {
                    Ok(FromClient::Invite { handle }) => {
                        match check_invite(&door, conn, holder.account, handle).await {
                            Ok(command) => {
                                if door.queue.send(command).await.is_err() {
                                    break;
                                }
                            }
                            Err(code) => {
                                if !say(socket, &ToClient::Refused { code: code.into() }).await {
                                    break;
                                }
                            }
                        }
                        continue;
                    }
                    other => other,
                };
                let command = match message.map_err(|()| "Unreadable message.").and_then(|m| command(conn, m)) {
                    Ok(command) => command,
                    Err(why) => {
                        refuse(socket, "bad_request", why).await;
                        break;
                    }
                };
                if door.queue.send(command).await.is_err() {
                    break;
                }
            }
            outgoing = outbox.recv() => {
                let Some(message) = outgoing else { break };
                // The connection stays open after a match is found, so the
                // party can queue again when the match is over.
                let last = matches!(message, ToClient::Error { .. });
                if !say(socket, &message).await {
                    break;
                }
                if last {
                    let _ = socket.send(Message::Close(None)).await;
                    // Already out of the matchmaker.
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
            parse(r#"{"type":"accept"}"#),
            Ok(FromClient::Accept {})
        ));
        for bad in [
            r#"{"type":"start"}"#,
            r#"{"type":"accept","seat":1}"#,
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
        // Custom lobby requests parse strictly and reject free-form map/mode fields.
        assert!(matches!(
            parse(r#"{"type":"create_lobby","mode":"2v2"}"#),
            Ok(FromClient::CreateLobby { .. })
        ));
        assert!(matches!(
            parse(
                r#"{"type":"set_lobby_settings","settings":{"version":1,"map":"colosseum","round_seconds":600}}"#
            ),
            Ok(FromClient::SetLobbySettings { .. })
        ));
        assert!(
            parse(r#"{"type":"set_lobby_settings","settings":{"version":1,"map":"../etc/passwd","round_seconds":600}}"#)
                .is_err()
        );
        assert!(
            parse(r#"{"type":"set_lobby_settings","settings":{"version":1,"map":"colosseum","round_seconds":600,"extra":1}}"#)
                .is_err()
        );
        assert!(
            command(
                1,
                parse(r#"{"type":"set_lobby_mode","mode":"4v4"}"#).unwrap()
            )
            .is_err()
        );
    }
}
