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

use std::collections::{HashMap, HashSet};
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

use super::chat::{self as chat_rules, Channel, ChatBudget, ChatSeat};
use super::hub;
use super::worlds::{Placement as WorldPlacement, Worlds};
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

/// Largest message a queue client may send: room for 200 characters of chat
/// even when each is a 4-byte character escaped by the client.
const MAX_MESSAGE: usize = 4096;
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const MESSAGES_PER_SECOND: u32 = 10;
const STATUS_EVERY: Duration = Duration::from_secs(2);
const TICK_EVERY: Duration = Duration::from_millis(250);
/// Messages buffered per connection; status updates are skipped when full.
const OUTBOUND_QUEUE: usize = 32;
/// Longest `Name#1234` a challenge names: anything longer names nobody.
const MAX_HANDLE_TEXT: usize = 32;

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
    /// Chat (`door/chat.rs`): text to `public`, `match`, `team` or
    /// `spectator`. The door sets who it is from.
    Chat {
        channel: String,
        text: String,
    },
    /// Chat to a friend by `Name#1234`.
    Whisper {
        to: String,
        text: String,
    },
    /// Stop (or resume) hearing a player's chat, on this connection.
    Ignore {
        handle: String,
    },
    Unignore {
        handle: String,
    },
    /// Enter a hub world on the world server (ADR 0057), beside a friend or
    /// party-mate (`Name#1234`) when given: answered with a `world` ticket.
    WorldJoin {
        #[serde(default)]
        friend: Option<String>,
    },
    /// Leave the hub world (the page closes its world connection).
    WorldLeave {},
    /// Challenge the player named `handle` (`Name#1234`) in this player's
    /// hub world to a 1v1 (ADR 0060).
    Challenge {
        handle: String,
    },
    /// Answer the challenge from the player named `from`.
    AnswerChallenge {
        from: String,
        accept: bool,
    },
    /// Take back this player's open challenge.
    WithdrawChallenge {},
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
        /// The other members' handles, leader first; `null` for one the
        /// door cannot name. The page shows them as teammates.
        mates: Vec<Option<String>>,
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
    /// No longer in the hub: `left`, `match` when a ticket took them, or
    /// `renamed` (the page asks again for a place under the new name).
    HubLeft {
        reason: String,
    },
    /// A place in hub world `instance` (hex): join `server` with `ticket`
    /// (hex), a `Role::Hub` ticket, until `expires` (ADR 0057).
    World {
        instance: String,
        server: String,
        ticket: String,
        expires: u64,
    },
    /// `from` challenges this player to a 1v1; answer within
    /// `remaining_ms` (ADR 0060).
    Challenged {
        from: String,
        remaining_ms: u64,
    },
    /// The challenge reached `to`.
    ChallengeSent {
        to: String,
    },
    /// The challenge between this player and `with` closed without a match:
    /// `declined`, `expired`, `withdrawn`, `left` or `busy`.
    ChallengeEnded {
        with: String,
        reason: String,
    },
    /// Friend requests or friendships changed: fetch `/api/friends`.
    FriendsChanged {},
    /// A chat message. `from` and `admin` are the door's, never the
    /// sender's claim; `to` is set on the sender's copy of a whisper.
    Chat {
        channel: String,
        from: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<String>,
        admin: bool,
        text: String,
    },
}
impl ToClient {
    fn error(code: &str, message: &str) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }
    fn party(view: PartyView, mates: Vec<Option<String>>) -> Self {
        Self::Party {
            code: view.code,
            members: view.members,
            leader: view.leader,
            queue: view.queue.map(|mode| mode.id().into()),
            mates,
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
    WorldJoin {
        conn: QueueConn,
        follow: Option<(String, Option<u64>)>,
        account: Option<u64>,
    },
    WorldLeave {
        conn: QueueConn,
    },
    Challenge {
        conn: QueueConn,
        handle: String,
    },
    AnswerChallenge {
        conn: QueueConn,
        from: String,
        accept: bool,
    },
    WithdrawChallenge {
        conn: QueueConn,
    },
    /// The world server's instance directory answered exchange
    /// `generation` (`None` when it did not).
    Directory {
        generation: u64,
        reply: Option<crate::worlds::DirectoryReply>,
    },
    /// The world server reports these matches ended: their players and
    /// spectators are no longer in a match (presence, whispers, chat).
    MatchesEnded {
        ids: Vec<u64>,
    },
    /// The account was deleted or logged out everywhere: its open queue
    /// connections stop signing it into tickets at once (ADR 0055).
    Revoke {
        account: u64,
    },
    /// The account picked a new handle (`Some`), or an admin cleared it
    /// (`None`, each connection then shows its browser's guest name): its
    /// open connections use it.
    Rename {
        account: u64,
        handle: Option<Handle>,
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
    /// Chat already cleaned, masked and within the sender's budget.
    Chat {
        conn: QueueConn,
        channel: Channel,
        text: String,
    },
    /// A whisper to the friend `to`, checked like an invite.
    Whisper {
        conn: QueueConn,
        from: u64,
        to: u64,
        to_handle: String,
        text: String,
    },
    /// Ignore (`on`) or stop ignoring `handle` (lowercase) on `conn`.
    Ignore {
        conn: QueueConn,
        handle: String,
        on: bool,
    },
    /// `account` blocked (or unblocked) `other`: their chat stops (or
    /// resumes) reaching `account`'s open connections.
    SetBlock {
        account: u64,
        other: u64,
        blocked: bool,
    },
    /// Accounts `conn`'s player blocked, read after its hello so no block
    /// slips between the read and [`Command::SetBlock`].
    Blocks {
        conn: QueueConn,
        blocked: Vec<u64>,
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
    let mut chat = ChatState::default();
    let mut searches = Searches::default();
    // Hub worlds on the world server (ADR 0057): placements, the directory
    // exchange (one at a time) and tickets held until the exchange after
    // their placement has told the world server about blocks.
    let mut worlds = Worlds::default();
    let mut directory_tick = tokio::time::interval(crate::worlds::DIRECTORY_EVERY);
    directory_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut directory = DirectoryState::default();
    let mut tick = tokio::time::interval(TICK_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut status_at = Instant::now() + STATUS_EVERY;
    let mut check = tokio::time::interval(door.config.match_check_every);
    check.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut checking: Option<tokio::task::JoinHandle<()>> = None;
    let mut stop = door.shutdown.subscribe();
    loop {
        let now = Instant::now();
        // A party that just queued gets an immediate status; everyone else
        // updates on the periodic cadence (avoiding an O(N^2) broadcast storm
        // on queue arrivals).
        let mut queued_conn = None;
        let out = tokio::select! {
            _ = stop.wait_for(|stop| *stop) => break,
            _ = tick.tick() => {
                let mut out = mm.tick(now);
                // A challenge stands only while both players share a world.
                out.extend(mm.prune_challenges(
                    |a, b| together(&worlds, &chat, &holders, a, b),
                    |conn| worlds.instance_of(conn).is_some(),
                ));
                out
            }
            _ = directory_tick.tick(), if door.world_status.is_some() => {
                if worlds.is_empty()
                    && directory.held.is_empty()
                    && !directory.draining
                    && !directory.listed_anyone
                {
                    // Nobody placed: an old report would only mislead.
                    worlds.forget_reports();
                } else {
                    // Tickets the world server never heard about in time,
                    // and placements it never confirmed while unreachable.
                    for conn in directory.expire(now.into_std()) {
                        worlds.leave(conn);
                        send(&conns, conn, ToClient::Refused { code: "hub_unavailable".into() });
                    }
                    if !directory.answered_lately(now.into_std()) {
                        for conn in worlds.expire(now.into_std()) {
                            send(&conns, conn, ToClient::HubLeft { reason: hub::Left::Left.id().into() });
                        }
                    }
                    directory.start(&door, &worlds, |a, b| hidden(&chat, &holders, a, b));
                }
                Vec::new()
            }
            _ = check.tick(), if door.world_status.is_some() => {
                // Past the in-match window a seat counts for nothing: forget
                // it rather than ask about its match forever.
                for id in chat.stale_matches(now) {
                    for conn in chat.end_match(id) {
                        matched_at.remove(&conn);
                    }
                }
                if checking.as_ref().is_none_or(|task| task.is_finished())
                    && !chat.members.is_empty()
                {
                    checking = Some(tokio::spawn(check_matches(
                        door.clone(),
                        chat.members.keys().copied().collect(),
                    )));
                }
                Vec::new()
            }
            command = commands.recv() => match command {
                None => break,
                Some(Command::Hello { conn, guest, address, mut holder, out }) => {
                    // Without a chosen name, the browser's own guest name.
                    holder.handle.get_or_insert_with(|| super::auth::guest_handle_for(guest));
                    conns.insert(conn, out);
                    addresses.insert(conn, address);
                    chat.guests.insert(conn, guest);
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
                Some(Command::MatchesEnded { ids }) => {
                    for id in ids {
                        for conn in chat.end_match(id) {
                            matched_at.remove(&conn);
                        }
                    }
                    Vec::new()
                }
                Some(Command::Revoke { account }) => {
                    by_account.remove(&account);
                    let mut renamed = Vec::new();
                    for (conn, holder) in holders.iter_mut() {
                        if holder.account == Some(account) {
                            // A mute outlives logging out.
                            chat.formerly.insert(*conn, account);
                            *holder = Holder {
                                account: None,
                                handle: Some(chat.guests.get(conn).map_or_else(
                                    super::auth::guest_handle,
                                    |guest| super::auth::guest_handle_for(*guest),
                                )),
                                admin: false,
                            };
                            renamed.push(*conn);
                        }
                    }
                    // A world shows the name its ticket carried: leave it,
                    // and the page asks again with the new one.
                    for conn in &renamed {
                        directory.drop_held(*conn);
                        if worlds.leave(*conn) {
                            send(&conns, *conn, ToClient::HubLeft { reason: "renamed".into() });
                        }
                    }
                    // Lobby- and party-mates see the new (guest) name at once.
                    let mut out = mm.announce_lobbies_of(renamed.iter().copied());
                    out.extend(mm.announce_parties_of(renamed));
                    out
                }
                Some(Command::Rename { account, handle }) => {
                    let mut renamed = Vec::new();
                    for (conn, holder) in holders.iter_mut() {
                        if holder.account == Some(account) {
                            holder.handle = Some(handle.unwrap_or_else(|| {
                                chat.guests.get(conn).map_or_else(
                                    super::auth::guest_handle,
                                    |guest| super::auth::guest_handle_for(*guest),
                                )
                            }));
                            renamed.push(*conn);
                        }
                    }
                    // A world shows the name its ticket carried: leave it,
                    // and the page asks again with the new one.
                    for conn in &renamed {
                        directory.drop_held(*conn);
                        if worlds.leave(*conn) {
                            send(&conns, *conn, ToClient::HubLeft { reason: "renamed".into() });
                        }
                    }
                    let mut out = mm.announce_lobbies_of(renamed.iter().copied());
                    out.extend(mm.announce_parties_of(renamed));
                    out
                }
                Some(Command::Invite { conn, from, to, to_handle }) => {
                    if holders.get(&conn).and_then(|h| h.account) == Some(from) {
                        invite(&mut mm, &conns, &holders, &by_account, now, conn, to, to_handle)
                    } else {
                        send(&conns, conn, ToClient::Refused { code: "not_logged_in".into() });
                        Vec::new()
                    }
                }
                Some(Command::Chat { conn, channel, text }) => {
                    // Public posts also spend the account's allowance, so
                    // more tabs are not more messages.
                    let account = holders.get(&conn).and_then(|holder| holder.account);
                    let over = channel == Channel::Public
                        && account.is_some_and(|account| {
                            !chat
                                .public
                                .entry(account)
                                .or_insert_with(|| ChatBudget::new(now))
                                .take(now)
                        });
                    if over {
                        send(&conns, conn, ToClient::Refused { code: "rate_limited".into() });
                    } else {
                        Chat {
                            mm: &mm,
                            worlds: &worlds,
                            conns: &conns,
                            holders: &holders,
                            state: &chat,
                            muted: &door.muted,
                            now,
                        }
                        .post(conn, channel, text);
                    }
                    Vec::new()
                }
                Some(Command::Whisper { conn, from, to, to_handle, text }) => {
                    Chat {
                        mm: &mm,
                        worlds: &worlds,
                        conns: &conns,
                        holders: &holders,
                        state: &chat,
                        muted: &door.muted,
                        now,
                    }
                    .whisper(&by_account, conn, from, to, to_handle, text);
                    Vec::new()
                }
                Some(Command::Ignore { conn, handle, on }) => {
                    ignore(&conns, &holders, &mut chat, conn, &handle, on);
                    Vec::new()
                }
                Some(Command::SetBlock { account, other, blocked }) => {
                    for conn in by_account.get(&account).into_iter().flatten() {
                        let set = chat.blocks.entry(*conn).or_default();
                        if blocked {
                            set.insert(other);
                        } else {
                            set.remove(&other);
                        }
                    }
                    Vec::new()
                }
                Some(Command::Blocks { conn, blocked }) => {
                    if conns.contains_key(&conn) {
                        chat.blocks.entry(conn).or_default().extend(blocked);
                    }
                    Vec::new()
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
                Some(Command::WorldJoin { conn, follow, account }) => {
                    let party: Vec<QueueConn> = mm.party_mates(conn);
                    let Some(holder) = holders.get(&conn).copied() else {
                        continue;
                    };
                    if holder.handle.is_none() {
                        send(&conns, conn, ToClient::Refused { code: "no_name".into() });
                        continue;
                    }
                    if directory.draining {
                        send(&conns, conn, ToClient::Refused { code: "hub_unavailable".into() });
                        continue;
                    }
                    let followed = followed_conns(
                        follow, account, &holder, &by_account, &party, &holders,
                    );
                    let player = worlds.player_for(conn, ticket::random_u64);
                    let answer = match &followed {
                        Some((found, friend)) if found.is_empty() && friend.is_none() => {
                            Err("not_friends")
                        }
                        _ => worlds.place(
                            conn,
                            player,
                            WorldPlacement {
                                follow: followed.as_ref().map_or(&[][..], |(found, _)| found),
                                following: followed.is_some(),
                                party: &party,
                            },
                            |a, b| hidden(&chat, &holders, a, b),
                            ticket::random_u64,
                            now.into_std(),
                        ),
                    };
                    match answer {
                        Ok(instance) => {
                            let expires = unix_now() + door.config.ticket_ttl.as_secs();
                            let pass = ticket::issue_hub(&door.config.key, instance, player, holder, expires);
                            Metrics::add(&door.metrics.tickets, 1);
                            let message = ToClient::World {
                                instance: format!("{instance:016x}"),
                                server: door.config.world_server.clone(),
                                ticket: ticket::to_hex(&pass),
                                expires,
                            };
                            if door.world_status.is_some() {
                                // Tell the world server about blocks first.
                                directory.hold(conn, message, now.into_std());
                                directory.start(&door, &worlds, |a, b| hidden(&chat, &holders, a, b));
                            } else {
                                send(&conns, conn, message);
                            }
                        }
                        Err(code) => send(&conns, conn, ToClient::Refused { code: code.into() }),
                    }
                    Vec::new()
                }
                Some(Command::WorldLeave { conn }) => {
                    directory.drop_held(conn);
                    if worlds.leave(conn) {
                        send(&conns, conn, ToClient::HubLeft { reason: hub::Left::Left.id().into() });
                    }
                    Vec::new()
                }
                Some(Command::Challenge { conn, handle }) => {
                    challenge(&mut mm, &worlds, &conns, &holders, &chat, now, conn, &handle)
                }
                Some(Command::AnswerChallenge { conn, from, accept }) => mm.answer_challenge(
                    conn,
                    &from,
                    accept,
                    |a, b| together(&worlds, &chat, &holders, a, b),
                    now,
                ),
                Some(Command::WithdrawChallenge { conn }) => mm.withdraw_challenge(conn),
                Some(Command::Directory { generation, reply }) => {
                    if let Some(reply) = &reply {
                        directory.draining = reply.draining;
                        for conn in worlds.reconcile(reply, now.into_std()) {
                            directory.drop_held(conn);
                            send(&conns, conn, ToClient::HubLeft { reason: hub::Left::Left.id().into() });
                        }
                    }
                    // Only an answer releases tickets (the world server then
                    // knows the newcomers' blocks); a failure keeps them for
                    // the next exchange, until they expire.
                    for (conn, message) in directory.finished(generation, reply.is_some(), now.into_std()) {
                        // Still placed (not left or matched meanwhile).
                        if worlds.instance_of(conn).is_some() {
                            send(&conns, conn, message);
                        }
                    }
                    if !directory.held.is_empty() {
                        directory.start(&door, &worlds, |a, b| hidden(&chat, &holders, a, b));
                    }
                    Vec::new()
                }
                Some(Command::Gone { conn }) => {
                    worlds.gone(conn);
                    directory.drop_held(conn);
                    searches.end(conn, "left");
                    conns.remove(&conn);
                    addresses.remove(&conn);
                    unindex(&mut by_account, &holders, conn);
                    holders.remove(&conn);
                    matched_at.remove(&conn);
                    chat.forget(conn);
                    mm.disconnect(conn, now)
                }
            },
        };
        for action in out {
            searches.observe(&action);
            match action {
                Out::Party { conn, view } => {
                    let mates = mm
                        .party_mates(conn)
                        .into_iter()
                        .map(|mate| {
                            holders
                                .get(&mate)
                                .and_then(|holder| holder.handle)
                                .map(|handle| handle.to_string())
                        })
                        .collect();
                    send(&conns, conn, ToClient::party(view, mates));
                }
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
                    chat.forget(conn);
                    worlds.gone(conn);
                    directory.drop_held(conn);
                }
                Out::Start { mode, seats } => {
                    let match_id =
                        start_match(&door, &conns, &holders, mode, &seats, Formed::Queue);
                    for (seat, conn) in seats.iter().enumerate() {
                        matched_at.insert(*conn, now);
                        chat.seat(
                            *conn,
                            ChatSeat {
                                match_id,
                                team: Some((seat % 2) as u8),
                                at: now,
                            },
                        );
                    }
                    for conn in seats {
                        searches.matched(conn, match_id);
                        leave_hub_for_match(&mut worlds, &mut directory, &conns, conn);
                    }
                }
                Out::Challenged {
                    conn,
                    from,
                    remaining,
                } => send(
                    &conns,
                    conn,
                    ToClient::Challenged {
                        from,
                        remaining_ms: remaining.as_millis() as u64,
                    },
                ),
                Out::ChallengeSent { conn, to } => {
                    send(&conns, conn, ToClient::ChallengeSent { to })
                }
                Out::ChallengeEnded { conn, with, reason } => send(
                    &conns,
                    conn,
                    ToClient::ChallengeEnded {
                        with,
                        reason: reason.code().into(),
                    },
                ),
                Out::StartChallenge { seats } => {
                    let match_id = start_match(
                        &door,
                        &conns,
                        &holders,
                        Mode::Duel,
                        &seats,
                        Formed::Challenge,
                    );
                    for (seat, conn) in seats.into_iter().enumerate() {
                        matched_at.insert(conn, now);
                        chat.seat(
                            conn,
                            ChatSeat {
                                match_id,
                                team: Some((seat % 2) as u8),
                                at: now,
                            },
                        );
                        leave_hub_for_match(&mut worlds, &mut directory, &conns, conn);
                    }
                }
                Out::Lobby { conn, mut view } => {
                    for member in &mut view.members {
                        member.handle = holders
                            .get(&member.conn)
                            .and_then(|holder| holder.handle)
                            .map(|handle| handle.to_string());
                    }
                    send(&conns, conn, ToClient::Lobby(view));
                }
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
                    let teams: Vec<Option<u8>> = (0..seats.len())
                        .map(|seat| Some((seat % 2) as u8))
                        .chain(watchers.iter().map(|_| None))
                        .collect();
                    if let Some(match_id) = start_custom_match(
                        &door, &conns, &addresses, &holders, now, host, mode, settings, seats,
                        spectators,
                    ) {
                        for (conn, team) in players.into_iter().zip(teams) {
                            leave_hub_for_match(&mut worlds, &mut directory, &conns, conn);
                            matched_at.insert(conn, now);
                            chat.seat(
                                conn,
                                ChatSeat {
                                    match_id,
                                    team,
                                    at: now,
                                },
                            );
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
        publish_metrics(&door, &mut mm, conns.len(), worlds.counts());
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
fn publish_metrics(door: &Door, mm: &mut Matchmaker, connections: usize, hub: (usize, usize)) {
    let metrics = &door.metrics;
    metrics.hub_players.store(hub.0 as u64, Ordering::Relaxed);
    metrics.hub_instances.store(hub.1 as u64, Ordering::Relaxed);
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
    /// Got a match ticket within [`IN_MATCH`] and the world server has not
    /// reported the match ended (shown unless searching right now).
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

/// How long after a match ticket a player counts as in a match at most: a
/// round (20 min) and a little. Sooner when the world server reports the
/// match ended (`--world-status`); this is the fallback without an answer.
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

/// Asks the world server about each seated match and reports those that
/// ended. Unknown matches (no one has joined yet, or a restarted server)
/// stay seated until the in-match window runs out.
async fn check_matches(door: Arc<Door>, ids: Vec<u64>) {
    let (Some(client), Some(base)) = (&door.world_status, &door.config.world_status) else {
        return;
    };
    let base = base.trim_end_matches('/');
    // At most this many questions in flight at once.
    let gate = Arc::new(tokio::sync::Semaphore::new(16));
    let mut asks = tokio::task::JoinSet::new();
    for id in ids {
        let (client, gate) = (client.clone(), gate.clone());
        let url = format!("{base}/matches/{id:016x}");
        asks.spawn(async move {
            let _permit = gate.acquire_owned().await.ok();
            let reply = client.get(url).send().await.map_err(|_| ())?;
            match reply.status() {
                // Not joined yet, or a restarted server: stays seated.
                reqwest::StatusCode::NOT_FOUND => return Ok(None),
                status if !status.is_success() => return Err(()),
                _ => {}
            }
            let status: crate::MatchStatus = reply.json().await.map_err(|_| ())?;
            Ok((status.state == crate::MatchState::Ended).then_some(id))
        });
    }
    let (mut ended, mut failed) = (Vec::new(), 0);
    while let Some(answer) = asks.join_next().await {
        match answer {
            Ok(Ok(id)) => ended.extend(id),
            _ => failed += 1,
        }
    }
    if failed > 0 {
        warn_unanswered(failed);
    }
    if !ended.is_empty() {
        let _ = door.queue.send(Command::MatchesEnded { ids: ended }).await;
    }
}

/// Unanswered match checks leave players in the 21-minute window: say so,
/// at most once a minute.
fn warn_unanswered(failed: usize) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let last = LAST.load(Ordering::Relaxed);
    if now >= last + 60
        && LAST
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        tracing::warn!(
            failed,
            "the world server did not answer match checks; check --world-status"
        );
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

/// Players one connection may ignore.
const MAX_IGNORED: usize = 200;
/// Outbox slots chat leaves free, so a busy channel never pushes out a
/// ticket or ready check.
const CHAT_RESERVE: usize = 8;

/// Who said something, independent of the name shown: ignores and mutes
/// follow this, so renaming or sharing a guest number changes nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Speaker {
    Account(u64),
    Guest(Guest),
}

/// Chat state the matchmaker task keeps per connection.
#[derive(Default)]
struct ChatState {
    /// Each connection's guest id.
    guests: HashMap<QueueConn, Guest>,
    /// Accounts whose connections were logged out (still muted).
    formerly: HashMap<QueueConn, u64>,
    /// Each connection's last seat, and the connections seated per match.
    seats: HashMap<QueueConn, ChatSeat>,
    members: HashMap<u64, Vec<QueueConn>>,
    /// Speakers each connection ignores, with the handle used to ignore.
    ignores: HashMap<QueueConn, HashMap<Speaker, String>>,
    /// Accounts each connection's account blocked.
    blocks: HashMap<QueueConn, HashSet<u64>>,
    /// `public` allowance per account, across its tabs.
    public: HashMap<u64, ChatBudget>,
}

impl ChatState {
    fn seat(&mut self, conn: QueueConn, seat: ChatSeat) {
        self.unseat(conn);
        self.members.entry(seat.match_id).or_default().push(conn);
        self.seats.insert(conn, seat);
    }

    /// Matches whose seats are all past the in-match window.
    fn stale_matches(&self, now: Instant) -> Vec<u64> {
        self.members
            .iter()
            .filter(|(_, conns)| {
                conns.iter().all(|conn| {
                    self.seats
                        .get(conn)
                        .is_none_or(|seat| now.saturating_duration_since(seat.at) >= IN_MATCH)
                })
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Unseats everyone still seated in `match_id`, returning them.
    fn end_match(&mut self, match_id: u64) -> Vec<QueueConn> {
        let conns = self.members.remove(&match_id).unwrap_or_default();
        for conn in &conns {
            self.seats.remove(conn);
        }
        conns
    }

    fn unseat(&mut self, conn: QueueConn) {
        if let Some(old) = self.seats.remove(&conn)
            && let Some(list) = self.members.get_mut(&old.match_id)
        {
            list.retain(|other| *other != conn);
            if list.is_empty() {
                self.members.remove(&old.match_id);
            }
        }
    }

    fn speaker(&self, conn: QueueConn, holder: &Holder) -> Option<Speaker> {
        match holder.account {
            Some(account) => Some(Speaker::Account(account)),
            None => self.guests.get(&conn).copied().map(Speaker::Guest),
        }
    }

    fn forget(&mut self, conn: QueueConn) {
        self.unseat(conn);
        self.guests.remove(&conn);
        self.formerly.remove(&conn);
        self.ignores.remove(&conn);
        self.blocks.remove(&conn);
    }
}

/// What chat routing reads from the matchmaker task. One rule keeps
/// spectators from feeding players (ghosting): a player in a live match
/// hears only `match` and `team`, and only players of that match post there.
struct Chat<'a> {
    mm: &'a Matchmaker,
    worlds: &'a Worlds,
    conns: &'a Senders,
    holders: &'a HashMap<QueueConn, Holder>,
    state: &'a ChatState,
    muted: &'a std::sync::Mutex<HashMap<u64, Instant>>,
    now: Instant,
}

impl Chat<'_> {
    /// `conn`'s seat in a match still under way, if any.
    fn seat(&self, conn: QueueConn) -> Option<ChatSeat> {
        self.state.seats.get(&conn).copied().filter(|seat| {
            self.now.saturating_duration_since(seat.at) < IN_MATCH && !self.mm.is_searching(conn)
        })
    }

    /// Whether `conn` plays (not watches) in a live match.
    fn playing(&self, conn: QueueConn) -> bool {
        self.seat(conn).is_some_and(|seat| seat.team.is_some())
    }

    /// The live match's seated connections whose team (`Some` for players,
    /// `None` for spectators) is `wanted`.
    fn seated(&self, match_id: u64, wanted: impl Fn(Option<u8>) -> bool) -> Vec<QueueConn> {
        self.state
            .members
            .get(&match_id)
            .into_iter()
            .flatten()
            .copied()
            .filter(|other| {
                self.seat(*other)
                    .is_some_and(|seat| seat.match_id == match_id && wanted(seat.team))
            })
            .collect()
    }

    fn speaker(&self, conn: QueueConn, holder: &Holder) -> Option<Speaker> {
        self.state.speaker(conn, holder)
    }

    fn refuse(&self, conn: QueueConn, code: &str) {
        send(self.conns, conn, ToClient::Refused { code: code.into() });
    }

    /// Whether `conn`'s account (or the account it was logged out of) is
    /// muted.
    fn muted(&self, conn: QueueConn, holder: &Holder) -> bool {
        let Some(account) = holder
            .account
            .or_else(|| self.state.formerly.get(&conn).copied())
        else {
            return false;
        };
        let muted = self
            .muted
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        muted.get(&account).is_some_and(|until| self.now < *until)
    }

    /// Whether `to` hears `speaker`: not ignored, not blocked.
    fn hears(&self, to: QueueConn, speaker: Speaker) -> bool {
        let ignored = self
            .state
            .ignores
            .get(&to)
            .is_some_and(|ignored| ignored.contains_key(&speaker));
        let blocked = match speaker {
            Speaker::Account(account) => self
                .state
                .blocks
                .get(&to)
                .is_some_and(|set| set.contains(&account)),
            Speaker::Guest(_) => false,
        };
        !(ignored || blocked)
    }

    /// Sends to each recipient that hears the speaker and has outbox room
    /// beyond [`CHAT_RESERVE`].
    fn deliver(&self, recipients: &[QueueConn], speaker: Speaker, message: &ToClient) {
        for to in recipients {
            let room = self
                .conns
                .get(to)
                .is_some_and(|out| out.capacity() > CHAT_RESERVE);
            if room && self.hears(*to, speaker) {
                send(self.conns, *to, message.clone());
            }
        }
    }

    /// Routes a message to its channel's listeners (the sender included).
    fn post(&self, conn: QueueConn, channel: Channel, text: String) -> Option<u64> {
        let from = *self.holders.get(&conn)?;
        let (Some(handle), Some(speaker)) = (from.handle, self.speaker(conn, &from)) else {
            self.refuse(conn, "no_name");
            return None;
        };
        if self.muted(conn, &from) {
            self.refuse(conn, "muted");
            return None;
        }
        let seat = self.seat(conn);
        let spectating = seat.is_some_and(|seat| seat.team.is_none());
        if spectating && channel != Channel::Spectator {
            self.refuse(conn, "spectators_only");
            return None;
        }
        let mut charge = None;
        let recipients: Vec<QueueConn> = match (channel, seat) {
            (Channel::Public, _) => {
                // Guests have no account to mute or block, so only
                // logged-in players speak in public.
                let Some(account) = from.account else {
                    self.refuse(conn, "log_in");
                    return None;
                };
                charge = Some(account);
                self.holders
                    .keys()
                    .copied()
                    .filter(|other| *other == conn || !self.playing(*other))
                    .collect()
            }
            (Channel::Match, Some(mine)) if mine.team.is_some() => {
                self.seated(mine.match_id, |team| team.is_some())
            }
            (Channel::Match, _) => {
                self.refuse(conn, "not_in_match");
                return None;
            }
            (Channel::Team, Some(mine)) if mine.team.is_some() => {
                self.seated(mine.match_id, |team| team == mine.team)
            }
            (Channel::Team, _) => {
                let lobby = self.mm.lobby_members(conn);
                let group = if lobby.is_empty() {
                    self.mm.party_members(conn)
                } else {
                    lobby
                };
                // Lobby-mates now playing hear only their match.
                group
                    .into_iter()
                    .filter(|other| *other == conn || !self.playing(*other))
                    .collect()
            }
            (Channel::Spectator, Some(mine)) if spectating => {
                self.seated(mine.match_id, |team| team.is_none())
            }
            (Channel::Spectator, _) => {
                self.refuse(conn, "not_spectating");
                return None;
            }
            // Everyone in the speaker's hub instance (ADR 0056); blocks
            // and ignores apply as everywhere. Like `public`, only
            // logged-in players speak, since guests cannot be muted or
            // blocked.
            (Channel::Hub, _) => {
                if from.account.is_none() {
                    self.refuse(conn, "log_in");
                    return None;
                }
                // Everyone in the speaker's hub world (ADR 0057).
                let members = self.worlds.members(conn);
                if members.is_empty() {
                    self.refuse(conn, "not_in_hub");
                    return None;
                }
                // Blocks hide players both ways in the hub, chat included.
                members
                    .into_iter()
                    .filter(|member| !hidden(self.state, self.holders, conn, *member))
                    .collect()
            }
        };
        let message = ToClient::Chat {
            channel: channel.id().into(),
            from: handle.to_string(),
            to: None,
            admin: from.admin,
            text,
        };
        self.deliver(&recipients, speaker, &message);
        charge
    }

    /// Delivers a whisper to the open connections of the friend `to` that
    /// are not playing a live match, and a copy (with `to`) to the sender.
    fn whisper(
        &self,
        by_account: &HashMap<u64, Vec<QueueConn>>,
        conn: QueueConn,
        from: u64,
        to: u64,
        to_handle: String,
        text: String,
    ) {
        let Some(sender) = self.holders.get(&conn).copied() else {
            return;
        };
        if sender.account != Some(from) {
            return self.refuse(conn, "not_logged_in");
        }
        if self.muted(conn, &sender) {
            return self.refuse(conn, "muted");
        }
        let Some(handle) = sender.handle else {
            return self.refuse(conn, "no_name");
        };
        if self.seat(conn).is_some_and(|seat| seat.team.is_none()) {
            return self.refuse(conn, "spectators_only");
        }
        let open = by_account.get(&to).cloned().unwrap_or_default();
        if open.is_empty() {
            return self.refuse(conn, "offline");
        }
        let targets: Vec<QueueConn> = open
            .into_iter()
            .filter(|other| !self.playing(*other))
            .collect();
        if targets.is_empty() {
            return self.refuse(conn, "in_match");
        }
        // Someone ignoring the sender just does not hear it; the sender is
        // not told (as with a block).
        let message = ToClient::Chat {
            channel: "whisper".into(),
            from: handle.to_string(),
            to: None,
            admin: sender.admin,
            text,
        };
        self.deliver(&targets, Speaker::Account(from), &message);
        let ToClient::Chat { text, .. } = message else {
            unreachable!()
        };
        send(
            self.conns,
            conn,
            ToClient::Chat {
                channel: "whisper".into(),
                from: handle.to_string(),
                to: Some(to_handle),
                admin: sender.admin,
                text,
            },
        );
    }
}

/// Ignores (or stops ignoring) whoever is connected as `handle`, keyed on
/// who they are rather than the name: a rename or a new guest with the same
/// number changes nothing.
fn ignore(
    conns: &Senders,
    holders: &HashMap<QueueConn, Holder>,
    state: &mut ChatState,
    conn: QueueConn,
    handle: &str,
    on: bool,
) {
    let me = holders
        .get(&conn)
        .and_then(|holder| state.speaker(conn, holder));
    let found: HashSet<Speaker> = holders
        .iter()
        .filter(|(_, holder)| {
            holder
                .handle
                .is_some_and(|h| h.to_string().eq_ignore_ascii_case(handle))
        })
        .filter_map(|(other, holder)| state.speaker(*other, holder))
        .filter(|speaker| Some(*speaker) != me)
        .collect();
    let ignored = state.ignores.entry(conn).or_default();
    if !on {
        ignored
            .retain(|speaker, name| !name.eq_ignore_ascii_case(handle) && !found.contains(speaker));
        return;
    }
    let refusal = if found.is_empty() {
        "no_such_player"
    } else if ignored.len() + found.len() > MAX_IGNORED {
        "too_many"
    } else {
        for speaker in found {
            ignored.insert(speaker, handle.to_owned());
        }
        return;
    };
    send(
        conns,
        conn,
        ToClient::Refused {
            code: refusal.into(),
        },
    );
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

/// Sends `conn`'s challenge to the one player named `handle` in its hub
/// world (ADR 0060). Nobody else can be challenged: a player elsewhere, a
/// name two players in the world share, or a player either side blocked
/// all get `no_such_player`, as a missing one does.
#[allow(clippy::too_many_arguments)]
fn challenge(
    mm: &mut Matchmaker,
    worlds: &Worlds,
    conns: &Senders,
    holders: &HashMap<QueueConn, Holder>,
    chat: &ChatState,
    now: Instant,
    conn: QueueConn,
    handle: &str,
) -> Vec<Out> {
    let name = |conn: &QueueConn| {
        holders
            .get(conn)
            .and_then(|holder| holder.handle)
            .map(|handle| handle.to_string())
    };
    if worlds.instance_of(conn).is_none() {
        send(
            conns,
            conn,
            ToClient::Refused {
                code: "not_in_hub".into(),
            },
        );
        return Vec::new();
    }
    // Before the search through the world, so a flood costs nothing.
    if !mm.challenge_turn(conn, now) {
        send(
            conns,
            conn,
            ToClient::Refused {
                code: "rate_limited".into(),
            },
        );
        return Vec::new();
    }
    let mut found = worlds.members(conn).into_iter().filter(|other| {
        *other != conn
            && name(other).is_some_and(|other| other.eq_ignore_ascii_case(handle))
            && !hidden(chat, holders, conn, *other)
    });
    let (Some(from), Some(to), None) = (name(&conn), found.next(), found.next()) else {
        send(
            conns,
            conn,
            ToClient::Refused {
                code: "no_such_player".into(),
            },
        );
        return Vec::new();
    };
    let to_handle = name(&to).expect("matched by name");
    mm.challenge(conn, to, from, to_handle, now)
}

/// Whether a challenge between two connections may stand (ADR 0060): both
/// in one hub world, neither having blocked the other since it was sent.
fn together(
    worlds: &Worlds,
    chat: &ChatState,
    holders: &HashMap<QueueConn, Holder>,
    a: QueueConn,
    b: QueueConn,
) -> bool {
    worlds.instance_of(a).is_some()
        && worlds.instance_of(a) == worlds.instance_of(b)
        && !hidden(chat, holders, a, b)
}

/// Whether either of two connections' accounts blocked the other's: such
/// players never see each other in the hub (ADR 0056).
fn hidden(
    chat: &ChatState,
    holders: &HashMap<QueueConn, Holder>,
    a: QueueConn,
    b: QueueConn,
) -> bool {
    let account = |conn: QueueConn| holders.get(&conn).and_then(|holder| holder.account);
    let blocks = |by: QueueConn, of: QueueConn| {
        account(of)
            .is_some_and(|other| chat.blocks.get(&by).is_some_and(|set| set.contains(&other)))
    };
    blocks(a, b) || blocks(b, a)
}

/// The instance directory exchange with the world server (ADR 0057): at
/// most one in flight, and world tickets held until an exchange started
/// after their placement answers, so the world server knows the new
/// player's blocks before the page connects.
#[derive(Default)]
struct DirectoryState {
    /// The last exchange started.
    generation: u64,
    in_flight: bool,
    /// The last request's clock, so the next is always later.
    last_ms: u64,
    /// The world server is stopping: place nobody new.
    draining: bool,
    /// Tickets waiting for an answer to an exchange started after them.
    held: Vec<Held>,
    /// When the last exchange started and the last answer came.
    started_at: Option<std::time::Instant>,
    answered_at: Option<std::time::Instant>,
    /// Whether the last request listed anyone: when everyone has left, one
    /// more exchange tells the world server so.
    listed_anyone: bool,
}

struct Held {
    after: u64,
    conn: QueueConn,
    message: ToClient,
    at: std::time::Instant,
}

/// Exchanges start at most this often (a burst of joins shares one).
const DIRECTORY_GAP: std::time::Duration = std::time::Duration::from_millis(200);

impl DirectoryState {
    fn hold(&mut self, conn: QueueConn, message: ToClient, now: std::time::Instant) {
        self.held.push(Held {
            after: self.generation + 1,
            conn,
            message,
            at: now,
        });
    }

    fn drop_held(&mut self, conn: QueueConn) {
        self.held.retain(|held| held.conn != conn);
    }

    /// Held tickets older than the confirmation window (the world server
    /// has not answered since they were placed): their connections.
    fn expire(&mut self, now: std::time::Instant) -> Vec<QueueConn> {
        let (old, young): (Vec<_>, Vec<_>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|held| now.duration_since(held.at) >= super::worlds::CONFIRM_WITHIN);
        self.held = young;
        old.into_iter().map(|held| held.conn).collect()
    }

    /// Whether the directory answered within the confirmation window (or
    /// has not been asked that long).
    fn answered_lately(&self, now: std::time::Instant) -> bool {
        let within =
            |at: std::time::Instant| now.duration_since(at) < super::worlds::CONFIRM_WITHIN;
        self.answered_at.is_some_and(within) || self.started_at.is_none_or(within)
    }

    /// Whether an exchange may start now: none in flight, and none started
    /// within [`DIRECTORY_GAP`] (the tick or the answer starts the next).
    fn may_start(&self, now: std::time::Instant) -> bool {
        !self.in_flight
            && self
                .started_at
                .is_none_or(|at| now.duration_since(at) >= DIRECTORY_GAP)
    }

    /// Starts an exchange when [`Self::may_start`].
    fn start(
        &mut self,
        door: &Arc<Door>,
        worlds: &Worlds,
        hidden: impl Fn(QueueConn, QueueConn) -> bool,
    ) {
        let now = std::time::Instant::now();
        if !self.may_start(now) {
            return;
        }
        let (Some(client), Some(base)) =
            (door.world_status.clone(), door.config.world_status.clone())
        else {
            return;
        };
        let request = self.request(now, worlds, hidden);
        let generation = self.generation;
        let door = door.clone();
        tokio::spawn(async move {
            let reply = exchange(&door, &client, &base, &request).await;
            if reply.is_none() {
                warn_directory_unanswered();
            }
            let _ = door
                .queue
                .send(Command::Directory { generation, reply })
                .await;
        });
    }

    /// Begins exchange `generation + 1`: its request, with a clock that
    /// only rises.
    fn request(
        &mut self,
        now: std::time::Instant,
        worlds: &Worlds,
        hidden: impl Fn(QueueConn, QueueConn) -> bool,
    ) -> crate::worlds::DirectoryRequest {
        self.generation += 1;
        self.in_flight = true;
        self.started_at = Some(now);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as u64);
        self.last_ms = now_ms.max(self.last_ms + 1);
        let players = worlds.placed_players();
        self.listed_anyone = !players.is_empty();
        crate::worlds::DirectoryRequest {
            now_ms: self.last_ms,
            blocks: worlds.block_pairs(hidden),
            players: Some(players),
        }
    }

    /// Exchange `generation` answered (`answered`) or failed: the tickets
    /// an answer releases (none on failure: they wait for the next).
    fn finished(
        &mut self,
        generation: u64,
        answered: bool,
        now: std::time::Instant,
    ) -> Vec<(QueueConn, ToClient)> {
        if generation == self.generation {
            self.in_flight = false;
        }
        if !answered {
            return Vec::new();
        }
        self.answered_at = Some(now);
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|held| held.after <= generation);
        self.held = waiting;
        ready
            .into_iter()
            .map(|held| (held.conn, held.message))
            .collect()
    }
}

/// Unanswered directory exchanges leave hub tickets waiting: say so, at
/// most once a minute (apart from match checks, which have their own).
fn warn_directory_unanswered() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = unix_now();
    let last = LAST.load(Ordering::Relaxed);
    if now >= last + 60
        && LAST
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        tracing::warn!("the world server did not answer the hub directory; check --world-status");
    }
}

/// One signed `POST /worlds/report`, bounded by a few seconds.
async fn exchange(
    door: &Door,
    client: &reqwest::Client,
    base: &str,
    request: &crate::worlds::DirectoryRequest,
) -> Option<crate::worlds::DirectoryReply> {
    let body = serde_json::to_vec(request).ok()?;
    let tag = door.config.key.sign(crate::worlds::DIRECTORY_LABEL, &body);
    let url = format!("{}/worlds/report", base.trim_end_matches('/'));
    let reply = client
        .post(url)
        .header(crate::worlds::DIRECTORY_SIGNATURE, tag)
        .header("content-type", "application/json")
        .timeout(std::time::Duration::from_secs(3))
        .body(body)
        .send()
        .await
        .ok()?;
    if !reply.status().is_success() {
        return None;
    }
    reply.json().await.ok()
}

/// The connections to join beside when a follow was asked for: the
/// friend's (an accepted friend of the account this tab is logged into),
/// or a party-mate's by name; nobody else (no following strangers).
/// `None` when no follow was asked; the friend's account rides along.
fn followed_conns(
    follow: Option<(String, Option<u64>)>,
    account: Option<u64>,
    holder: &Holder,
    by_account: &HashMap<u64, Vec<QueueConn>>,
    party: &[QueueConn],
    holders: &HashMap<QueueConn, Holder>,
) -> Option<(Vec<QueueConn>, Option<u64>)> {
    let (name, friend) = follow?;
    // A friend found for an account this tab has since logged out of no
    // longer counts.
    let friend = friend.filter(|_| account.is_some() && holder.account == account);
    let mut found: Vec<QueueConn> = friend
        .and_then(|account| by_account.get(&account))
        .cloned()
        .unwrap_or_default();
    found.extend(party.iter().copied().filter(|mate| {
        holders
            .get(mate)
            .and_then(|h| h.handle)
            .is_some_and(|h| h.to_string().eq_ignore_ascii_case(&name))
    }));
    Some((found, friend))
}

/// A ticket takes its player out of the hub (the door's or a world); others
/// see them go next tick.
fn leave_hub_for_match(
    worlds: &mut Worlds,
    directory: &mut DirectoryState,
    conns: &Senders,
    conn: QueueConn,
) {
    directory.drop_held(conn);
    if worlds.leave(conn) {
        send(
            conns,
            conn,
            ToClient::HubLeft {
                reason: hub::Left::Match.id().into(),
            },
        );
    }
}

/// Where a match without custom settings came from.
#[derive(Clone, Copy)]
enum Formed {
    Queue,
    /// An accepted 1v1 challenge (ADR 0060).
    Challenge,
}

/// Issues a queue (or challenge) match's tickets; returns its id.
fn start_match(
    door: &Door,
    conns: &Senders,
    holders: &HashMap<QueueConn, Holder>,
    mode: Mode,
    seats: &[QueueConn],
    formed: Formed,
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
    Metrics::add(&door.metrics.tickets, created.tickets.len() as u64);
    match formed {
        Formed::Queue => {
            Metrics::add(&door.metrics.queue_matches, 1);
            tracing::info!(
                match_id = %MatchId(created.match_id),
                mode = mode.id(),
                team_size = mode.team_size(),
                "match formed from the queue"
            );
        }
        Formed::Challenge => {
            Metrics::add(&door.metrics.challenge_matches, 1);
            tracing::info!(
                match_id = %MatchId(created.match_id),
                mode = mode.id(),
                "match started from a challenge"
            );
        }
    }
    for (seat, (conn, ticket)) in seats.iter().zip(created.tickets).enumerate() {
        if let Some(sender) = conns.get(conn) {
            let _ = sender.try_send(ToClient::Match {
                mode: mode.id().into(),
                match_id: format!("{:016x}", created.match_id),
                seat,
                spectator: false,
                server: door.config.world_server.clone(),
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
) -> Option<u64> {
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
        return None;
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
                server: door.config.world_server.clone(),
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
                server: door.config.world_server.clone(),
                ticket,
                expires,
            });
        }
    }
    Some(created.match_id)
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
    // it is ours; everyone else plays under the guest handle of the guest id
    // their hello names (`guest_handle_for`).
    let account = super::auth::account(&door, &headers, true).await;
    let holder = Holder {
        account: account.map(|account| account.id),
        handle: account.and_then(|account| account.handle),
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
        FromClient::Chat { .. } | FromClient::Whisper { .. } => {
            return Err("Chat through the connection.");
        }
        FromClient::Ignore { handle } => Command::Ignore {
            conn,
            handle: handle.to_ascii_lowercase(),
            on: true,
        },
        FromClient::Unignore { handle } => Command::Ignore {
            conn,
            handle: handle.to_ascii_lowercase(),
            on: false,
        },
        // Needs the store to find a friend; checked by the connection first.
        FromClient::WorldJoin { .. } => return Err("Join the hub through the connection."),
        FromClient::WorldLeave {} => Command::WorldLeave { conn },
        // No handle is longer; such a name matches nobody.
        FromClient::Challenge { handle } if handle.len() <= MAX_HANDLE_TEXT => {
            Command::Challenge { conn, handle }
        }
        FromClient::Challenge { .. } => Command::Challenge {
            conn,
            handle: String::new(),
        },
        FromClient::AnswerChallenge { from, accept } if from.len() <= MAX_HANDLE_TEXT => {
            Command::AnswerChallenge { conn, from, accept }
        }
        FromClient::AnswerChallenge { accept, .. } => Command::AnswerChallenge {
            conn,
            from: String::new(),
            accept,
        },
        FromClient::WithdrawChallenge {} => Command::WithdrawChallenge { conn },
    })
}

/// The friend to join beside: the handle asked for, and the friend's account
/// when it is an accepted friend of `account`'s. Looking a friend up reads
/// the store, so it shares the per-account invite budget.
async fn follow_of(
    door: &Door,
    account: Option<u64>,
    friend: Option<String>,
) -> Option<(String, Option<u64>)> {
    let name = friend?;
    let found = match (door.identity.as_ref(), account) {
        (Some(identity), Some(account))
            if name.len() <= 32 && door.invites.take(account, Instant::now()).is_ok() =>
        {
            friend_by_handle(identity, account, &name)
                .await
                .ok()
                .flatten()
        }
        _ => None,
    };
    Some((name, found))
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
    match friend_by_handle(identity, account, &handle).await {
        Ok(Some(to)) => Ok(Command::Invite {
            conn,
            from: account,
            to,
            to_handle: handle,
        }),
        Ok(None) => Err("no_such_friend"),
        Err(_) => Err("unavailable"),
    }
}

/// Sends a checked command to the matchmaker task, or the refusal to the
/// client; `false` once the connection should end.
async fn forward_or_refuse(
    socket: &mut WebSocket,
    door: &Door,
    checked: Result<Command, &'static str>,
) -> bool {
    match checked {
        Ok(command) => door.queue.send(command).await.is_ok(),
        Err(code) => say(socket, &ToClient::Refused { code: code.into() }).await,
    }
}

/// The friend of `account` holding `handle`; `None` for a stranger, a
/// block or no such handle alike.
async fn friend_by_handle(
    identity: &crate::identity::Identity,
    account: u64,
    handle: &str,
) -> Result<Option<u64>, crate::identity::store::StoreError> {
    let Some(other) = identity.store.find_handle(handle).await? else {
        return Ok(None);
    };
    Ok(identity
        .store
        .are_friends(account, other)
        .await?
        .then_some(other))
}

/// A chat message within the connection's budget, cleaned and masked.
fn check_chat(door: &Door, budget: &mut ChatBudget, text: &str) -> Result<String, &'static str> {
    if !budget.take(Instant::now()) {
        return Err("rate_limited");
    }
    let text = chat_rules::clean(text)?;
    Ok(door.words.mask(&text))
}

/// A whisper is for a friend of a logged-in player, found by handle; a
/// stranger and a missing handle get the same refusal.
async fn check_whisper(
    door: &Door,
    conn: QueueConn,
    account: Option<u64>,
    handle: String,
    text: String,
) -> Result<Command, &'static str> {
    let (Some(identity), Some(account)) = (door.identity.as_ref(), account) else {
        return Err("not_logged_in");
    };
    if handle.len() > 32 {
        return Err("no_such_friend");
    }
    match friend_by_handle(identity, account, &handle).await {
        Ok(Some(to)) => Ok(Command::Whisper {
            conn,
            from: account,
            to,
            to_handle: handle,
            text,
        }),
        Ok(None) => Err("no_such_friend"),
        Err(_) => Err("unavailable"),
    }
}

async fn connection(socket: &mut WebSocket, door: Arc<Door>, address: IpAddr, holder: Holder) {
    let conn = door.next_conn.fetch_add(1, Ordering::Relaxed);
    let mut rate = RateWindow::new(MESSAGES_PER_SECOND, Instant::now());
    let mut stop = door.shutdown.subscribe();
    let mut chat_budget = ChatBudget::new(Instant::now());
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
    // Players this account blocked: their chat never reaches it. Read
    // after the hello, so a block made meanwhile arrives as a SetBlock.
    if let (Some(account), Some(identity)) = (holder.account, door.identity.as_ref()) {
        let blocked = identity.store.blocked_by(account).await.unwrap_or_default();
        if !blocked.is_empty() {
            let _ = door.queue.send(Command::Blocks { conn, blocked }).await;
        }
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
                        let decoded = decode(message);
                        // Every message, control frames included, spends
                        // the connection's budget.
                        if !rate.allow(Instant::now()) {
                            refuse(socket, "rate_limited", "Too many messages.").await;
                            break;
                        }
                        match decoded {
                            None => continue,
                            Some(message) => message,
                        }
                    }
                    _ => break,
                };
                // Chat is cleaned, masked and rate-limited here.
                let message = match message {
                    Ok(FromClient::Chat { channel, text }) => {
                        let checked = check_chat(&door, &mut chat_budget, &text).and_then(|text| {
                            Channel::from_id(&channel)
                                .map(|channel| Command::Chat { conn, channel, text })
                                .ok_or("no_such_channel")
                        });
                        if !forward_or_refuse(socket, &door, checked).await {
                            break;
                        }
                        continue;
                    }
                    Ok(FromClient::Whisper { to, text }) => {
                        let checked = match check_chat(&door, &mut chat_budget, &text) {
                            Ok(text) => check_whisper(&door, conn, holder.account, to, text).await,
                            Err(code) => Err(code),
                        };
                        if !forward_or_refuse(socket, &door, checked).await {
                            break;
                        }
                        continue;
                    }
                    other => other,
                };
                // Invites need the store, so they are checked here.
                let message = match message {
                    Ok(FromClient::WorldJoin { friend }) => {
                        let follow = follow_of(&door, holder.account, friend).await;
                        if door
                            .queue
                            .send(Command::WorldJoin {
                                conn,
                                follow,
                                account: holder.account,
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                        continue;
                    }
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

    fn ticket_for(conn: QueueConn) -> ToClient {
        ToClient::World {
            instance: format!("{conn:016x}"),
            server: "ws://x/play".into(),
            ticket: "00".into(),
            expires: 0,
        }
    }

    #[test]
    fn held_world_tickets_wait_for_an_answer_to_a_later_exchange() {
        let start = std::time::Instant::now();
        let mut worlds = Worlds::default();
        let mut directory = DirectoryState::default();
        // Exchange 1 is in flight when the ticket is placed: its answer
        // knows nothing of the newcomer, so the ticket waits for the next.
        let first = directory.request(start, &worlds, |_, _| false);
        assert!(!directory.may_start(start), "one at a time");
        directory.hold(1, ticket_for(1), start);
        assert!(directory.finished(1, true, start).is_empty());
        assert!(!directory.may_start(start), "not within the gap");
        // Exchange 2 fails: nothing is released.
        let later = start + DIRECTORY_GAP;
        assert!(directory.may_start(later));
        let second = directory.request(later, &worlds, |_, _| false);
        assert!(second.now_ms > first.now_ms, "the clock only rises");
        assert!(directory.finished(2, false, later).is_empty());
        assert_eq!(directory.held.len(), 1);
        // Exchange 3 answers: released.
        let last = later + DIRECTORY_GAP;
        let _ = directory.request(last, &worlds, |_, _| false);
        let released = directory.finished(3, true, last);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].0, 1);
        assert!(directory.held.is_empty());
        // Requests name every placed player.
        worlds
            .place(9, 99, Default::default(), |_, _| false, || 5, start)
            .unwrap();
        let request = directory.request(last + DIRECTORY_GAP, &worlds, |_, _| false);
        assert_eq!(request.players, Some(vec![99]));
    }

    #[test]
    fn held_tickets_expire_and_are_dropped_with_their_connection() {
        let start = std::time::Instant::now();
        let mut directory = DirectoryState::default();
        directory.hold(1, ticket_for(1), start);
        directory.hold(2, ticket_for(2), start);
        directory.drop_held(2);
        assert!(directory.expire(start).is_empty());
        assert_eq!(
            directory.expire(start + super::super::worlds::CONFIRM_WITHIN),
            vec![1]
        );
        assert!(directory.held.is_empty());
        // Never asked: still "lately"; asked long ago and never answered: not.
        assert!(directory.answered_lately(start));
        let _ = directory.request(start, &Worlds::default(), |_, _| false);
        assert!(!directory.answered_lately(start + super::super::worlds::CONFIRM_WITHIN));
        let _ = directory.finished(1, true, start + super::super::worlds::CONFIRM_WITHIN);
        assert!(directory.answered_lately(start + super::super::worlds::CONFIRM_WITHIN));
    }

    #[test]
    fn ending_a_match_unseats_only_its_players() {
        let mut chat = ChatState::default();
        let seat = |match_id, team| ChatSeat {
            match_id,
            team,
            at: Instant::now(),
        };
        chat.seat(1, seat(10, Some(0)));
        chat.seat(2, seat(10, None));
        chat.seat(3, seat(11, Some(1)));
        let mut ended = chat.end_match(10);
        ended.sort_unstable();
        assert_eq!(ended, vec![1, 2]);
        assert!(!chat.seats.contains_key(&1) && !chat.seats.contains_key(&2));
        assert!(chat.seats.contains_key(&3));
        assert!(!chat.members.contains_key(&10));
        assert!(chat.end_match(10).is_empty());
    }

    #[test]
    fn a_reseated_player_keeps_the_new_seat_when_the_old_match_ends() {
        let mut chat = ChatState::default();
        let start = Instant::now();
        let seat = |match_id, at| ChatSeat {
            match_id,
            team: Some(0),
            at,
        };
        chat.seat(1, seat(10, start));
        chat.seat(2, seat(10, start));
        chat.seat(1, seat(11, start));
        assert_eq!(chat.end_match(10), vec![2]);
        assert_eq!(chat.seats.get(&1).map(|seat| seat.match_id), Some(11));
        // Seats past the in-match window are stale; fresh ones are not.
        assert!(chat.stale_matches(start + IN_MATCH / 2).is_empty());
        assert_eq!(chat.stale_matches(start + IN_MATCH), vec![11]);
    }

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
            serde_json::to_string(&ToClient::party(
                PartyView {
                    code: Some("K7P2QX".into()),
                    members: 2,
                    leader: true,
                    queue: Some(Mode::Team2v2),
                },
                vec![Some("Rook#0042".into())]
            ))
            .unwrap(),
            r#"{"type":"party","code":"K7P2QX","members":2,"leader":true,"queue":"2v2","mates":["Rook#0042"]}"#
        );
        // Challenges (ADR 0060): strict fields; an overlong name names nobody.
        assert!(matches!(
            command(7, parse(r#"{"type":"challenge","handle":"Rook#0042"}"#).unwrap()),
            Ok(Command::Challenge { conn: 7, handle }) if handle == "Rook#0042"
        ));
        let long = format!(r#"{{"type":"challenge","handle":"{}"}}"#, "x".repeat(400));
        assert!(matches!(
            command(7, parse(&long).unwrap()),
            Ok(Command::Challenge { handle, .. }) if handle.is_empty()
        ));
        assert!(matches!(
            command(
                7,
                parse(r#"{"type":"answer_challenge","from":"Rook#0042","accept":false}"#).unwrap()
            ),
            Ok(Command::AnswerChallenge { accept: false, from, .. }) if from == "Rook#0042"
        ));
        assert!(matches!(
            command(7, parse(r#"{"type":"withdraw_challenge"}"#).unwrap()),
            Ok(Command::WithdrawChallenge { conn: 7 })
        ));
        for bad in [
            r#"{"type":"withdraw_challenge","handle":"Rook#0042"}"#,
            r#"{"type":"challenge"}"#,
            r#"{"type":"challenge","handle":"Rook#0042","mode":"2v2"}"#,
            r#"{"type":"answer_challenge","from":"Rook#0042"}"#,
            r#"{"type":"answer_challenge","from":"Rook#0042","accept":"yes"}"#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert_eq!(
            serde_json::to_string(&ToClient::ChallengeEnded {
                with: "Rook#0042".into(),
                reason: "declined".into(),
            })
            .unwrap(),
            r#"{"type":"challenge_ended","with":"Rook#0042","reason":"declined"}"#
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
