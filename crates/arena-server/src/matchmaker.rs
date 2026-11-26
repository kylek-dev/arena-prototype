//! Queues and parties (ADR 0046, ADR 0047).
//!
//! Three queues: `1v1` takes players on their own; `2v2` and `3v3` take
//! players on their own or parties up to the team size, and fill each team
//! from whoever is queued (user decisions 2026-11-15 and 2026-11-16: no
//! separate solo queues). Party members always play on the same team.
//!
//! Every connected player belongs to a party; a player who has not formed
//! or joined one is a party of one without an invite code. A party's leader
//! (its first member) queues it; any member may cancel the search.
//!
//! When a queue can fill both teams, everyone chosen is asked to accept
//! the match within the ready check (15 s by default). If everyone accepts,
//! the match starts and the parties stay together to queue again
//! afterwards. If someone declines or the time runs out, players who did
//! not accept leave the queue (they stay connected), as do their parties,
//! since a party plays together; parties whose members all accepted go
//! back to the front of the queue in their order.
//!
//! The matchmaker also keeps how long recent matches took to find, per
//! queue, for an expected wait.
//!
//! A clock-free state machine like [`crate::session::Session`]: the caller
//! passes in time and connection identifiers and carries out the returned
//! [`Out`] actions. It never issues tickets; [`Out::Start`] asks for one per
//! seat.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use crate::ticket::MatchSettings;

#[path = "matchmaker_custom.rs"]
mod custom;
use custom::CustomLobby;
pub use custom::{LobbyLeft, LobbyMemberView, LobbySlot, LobbyView, MAX_LOBBY_SPECTATORS};

/// A queue connection, numbered by the caller.
pub type QueueConn = u64;

/// A browser's guest identity: a random 128-bit value it keeps. One guest
/// has at most one connection.
pub type Guest = u128;

/// Players per party at most: one full team.
pub const MAX_PARTY: usize = 3;
/// Parties looked at, from the front of a queue, when forming a match.
const FORMING_WINDOW: usize = 64;
/// Invite code characters: no 0/O or 1/I/L to misread.
const CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
pub const CODE_LENGTH: usize = 6;
/// Recent waits kept per queue for the expected wait ...
const RECENT_WAITS: usize = 20;
/// ... and how many it needs before it estimates anything.
const MIN_WAITS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[serde(rename = "1v1")]
    Duel,
    #[serde(rename = "2v2")]
    Team2v2,
    #[serde(rename = "3v3")]
    Team3v3,
}
impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Duel, Mode::Team2v2, Mode::Team3v3];
    pub fn id(self) -> &'static str {
        match self {
            Mode::Duel => "1v1",
            Mode::Team2v2 => "2v2",
            Mode::Team3v3 => "3v3",
        }
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.id() == id)
    }
    pub fn team_size(self) -> usize {
        match self {
            Mode::Duel => 1,
            Mode::Team2v2 => 2,
            Mode::Team3v3 => 3,
        }
    }
    pub fn players(self) -> usize {
        2 * self.team_size()
    }
    /// The largest party this queue takes: one team.
    pub fn max_party(self) -> usize {
        self.team_size()
    }
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Why a connection is closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closed {
    /// The same guest connected again elsewhere.
    Replaced,
}

/// A request the matchmaker turned down; the connection stays open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Only the party leader can queue the party.
    NotLeader,
    /// The party is larger than this queue takes.
    PartyTooBig,
    /// No party has that code.
    NoSuchParty,
    PartyFull,
    /// The party is searching or in a ready check; it cannot change now.
    PartyBusy,
    /// The player is in a private custom lobby and must leave it first.
    InLobby,
    /// No custom lobby has that code (or the player is not in one).
    NoSuchLobby,
    /// The custom lobby has no open team or spectator slots.
    LobbyFull,
    /// Only the lobby host can perform this action.
    NotHost,
    /// The chosen team or spectator slot is already full.
    SlotFull,
    /// Both teams must be full before the host can start the match.
    LobbyNotFull,
    /// Too many players in the lobby to shrink to that team size.
    LobbyTooBig,
    /// No member in the lobby matches that id.
    NoSuchMember,
    /// Match settings failed server version or allowlist validation.
    InvalidSettings,
    /// The lobby host disabled spectator slots.
    SpectatorsDisabled,
    /// Cannot disable spectator slots while spectators are in the lobby.
    SpectatorsOccupied,
}
impl Refused {
    pub fn code(self) -> &'static str {
        match self {
            Refused::NotLeader => "not_leader",
            Refused::PartyTooBig => "party_too_big",
            Refused::NoSuchParty => "no_such_party",
            Refused::PartyFull => "party_full",
            Refused::PartyBusy => "party_busy",
            Refused::InLobby => "in_lobby",
            Refused::NoSuchLobby => "no_such_lobby",
            Refused::LobbyFull => "lobby_full",
            Refused::NotHost => "not_host",
            Refused::SlotFull => "slot_full",
            Refused::LobbyNotFull => "lobby_not_full",
            Refused::LobbyTooBig => "lobby_too_big",
            Refused::NoSuchMember => "no_such_member",
            Refused::InvalidSettings => "invalid_settings",
            Refused::SpectatorsDisabled => "spectators_disabled",
            Refused::SpectatorsOccupied => "spectators_occupied",
        }
    }
}

/// Why a party's search stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stopped {
    /// A member cancelled.
    Cancelled,
    /// A member left the party or disconnected.
    MemberLeft,
    /// This player declined the match.
    Declined,
    /// This player did not accept the match in time.
    NotAccepted,
    /// A party member declined the match.
    MemberDeclined,
    /// A party member did not accept the match in time.
    MemberNotAccepted,
}
impl Stopped {
    pub fn code(self) -> &'static str {
        match self {
            Stopped::Cancelled => "cancelled",
            Stopped::MemberLeft => "member_left",
            Stopped::Declined => "declined",
            Stopped::NotAccepted => "not_accepted",
            Stopped::MemberDeclined => "member_declined",
            Stopped::MemberNotAccepted => "member_not_accepted",
        }
    }
}

/// What a member is told about their party.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartyView {
    /// The invite code; `None` for a player on their own.
    pub code: Option<String>,
    pub members: usize,
    pub leader: bool,
    /// The queue the party is in, if searching (or in a ready check).
    pub queue: Option<Mode>,
}

/// A ready check as one player sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadyCheck {
    pub mode: Mode,
    /// Players who accepted so far, out of `total`.
    pub accepted: usize,
    pub total: usize,
    /// Whether this player has accepted.
    pub you: bool,
    /// Time left to accept.
    pub remaining: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
    Party {
        conn: QueueConn,
        view: PartyView,
    },
    /// A match is ready to accept, or its acceptances changed.
    ReadyCheck {
        conn: QueueConn,
        check: ReadyCheck,
    },
    /// Someone else did not accept; back at the front of the queue.
    Requeued {
        conn: QueueConn,
        mode: Mode,
    },
    /// The party is no longer searching.
    Stopped {
        conn: QueueConn,
        reason: Stopped,
    },
    Refused {
        conn: QueueConn,
        reason: Refused,
    },
    /// Everyone accepted: issue a ticket per seat. `seats[i]` takes seat
    /// `i` (even seats are one team, odd seats the other). The players stay
    /// connected and in their parties, no longer searching.
    Start {
        mode: Mode,
        seats: Vec<QueueConn>,
    },
    /// Current state of the player's private custom lobby.
    Lobby {
        conn: QueueConn,
        view: LobbyView,
    },
    /// The player left or was removed from their private custom lobby.
    LobbyLeft {
        conn: QueueConn,
        reason: LobbyLeft,
    },
    /// The host started a full custom lobby: issue per-seat tickets carrying
    /// `settings`. Everyone remains in the lobby for rematches.
    StartCustom {
        host: QueueConn,
        mode: Mode,
        settings: MatchSettings,
        seats: Vec<QueueConn>,
        spectators: Vec<QueueConn>,
    },
    Close {
        conn: QueueConn,
        reason: Closed,
    },
}

#[derive(Clone, Copy, Debug)]
struct Member {
    conn: QueueConn,
    guest: Guest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    Queued(Mode),
    Forming(u64),
}

#[derive(Debug)]
struct Party {
    code: Option<String>,
    /// The first member leads.
    members: Vec<Member>,
    state: State,
    /// When this search began; kept through failed ready checks.
    since: Option<Instant>,
}

#[derive(Debug)]
struct Forming {
    mode: Mode,
    /// Queue order, for putting complete parties back.
    parties: Vec<u64>,
    /// Party ids per team.
    teams: [Vec<u64>; 2],
    accepted: HashSet<QueueConn>,
    declined: HashSet<QueueConn>,
    /// Parties that lost a member during the check.
    broken: HashSet<u64>,
    deadline: Instant,
}

/// What a friend is invited to, with the code that joins it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Invite {
    Party(String),
    Lobby(String),
}

/// Players waiting per queue, indexed by [`Mode::index`].
pub type Waiting = [usize; 3];

/// Running totals since start, for the front door's `/metrics`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Ready checks offered (queue pops).
    pub checks_started: u64,
    /// Ready checks everyone accepted: a match was issued.
    pub checks_completed: u64,
    /// Ready checks that failed: a decline, a timeout or someone leaving.
    pub checks_failed: u64,
    /// Players who accepted a ready check.
    pub accepts: u64,
    /// Players who declined a ready check, including ones who had accepted
    /// it first (they stay counted in `accepts`: both are counters).
    pub declines: u64,
    /// Searches started, by party size (index 0 is a solo player).
    pub searches_by_size: [u64; MAX_PARTY],
}

/// Found waits buffered between [`Matchmaker::take_found_waits`] calls;
/// beyond this, newer waits are not recorded (metrics only).
const FOUND_WAITS_BUFFER: usize = 256;

#[derive(Debug)]
pub struct Matchmaker {
    queues: [VecDeque<u64>; 3],
    /// Players currently searching or in a ready check per queue, maintained
    /// incrementally so `waiting()` is O(1).
    waiting: Waiting,
    parties: HashMap<u64, Party>,
    codes: HashMap<String, u64>,
    lobbies: HashMap<u64, CustomLobby>,
    lobby_codes: HashMap<String, u64>,
    in_lobby: HashMap<QueueConn, u64>,
    forming: HashMap<u64, Forming>,
    next_id: u64,
    /// Each connection's party.
    of: HashMap<QueueConn, u64>,
    guests: HashMap<Guest, QueueConn>,
    /// How long recent matches took to find, per queue, newest last.
    waits: [VecDeque<Duration>; 3],
    stats: Stats,
    /// Waits of parties whose match was found, not yet taken for metrics.
    found: Vec<(Mode, Duration)>,
    ready_timeout: Duration,
    /// A random value in `0..bound`: invite codes, team sides and seats.
    random: fn(usize) -> usize,
}

impl Matchmaker {
    pub fn new(ready_timeout: Duration, random: fn(usize) -> usize) -> Self {
        Self {
            queues: Default::default(),
            waiting: [0; 3],
            parties: HashMap::new(),
            codes: HashMap::new(),
            lobbies: HashMap::new(),
            lobby_codes: HashMap::new(),
            in_lobby: HashMap::new(),
            forming: HashMap::new(),
            next_id: 0,
            of: HashMap::new(),
            guests: HashMap::new(),
            waits: Default::default(),
            stats: Stats::default(),
            found: Vec::new(),
            ready_timeout,
            random,
        }
    }

    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn contains(&self, conn: QueueConn) -> bool {
        self.of.contains_key(&conn)
    }

    pub fn waiting(&self) -> Waiting {
        self.waiting
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Parties with an invite code (made with `create_party`).
    pub fn parties(&self) -> usize {
        self.codes.len()
    }

    /// Open private custom lobbies.
    pub fn lobbies(&self) -> usize {
        self.lobbies.len()
    }

    /// How long each party waited for the matches found since the last call.
    pub fn take_found_waits(&mut self) -> std::vec::Drain<'_, (Mode, Duration)> {
        self.found.drain(..)
    }

    /// The expected wait for `mode`: the median of how long recent matches
    /// took to find, or `None` until there have been a few.
    pub fn expected_wait(&self, mode: Mode) -> Option<Duration> {
        let recent = &self.waits[mode.index()];
        if recent.len() < MIN_WAITS {
            return None;
        }
        let mut sorted = [Duration::ZERO; RECENT_WAITS];
        let len = recent.len().min(RECENT_WAITS);
        for (slot, &wait) in sorted[..len].iter_mut().zip(recent.iter()) {
            *slot = wait;
        }
        sorted[..len].sort_unstable();
        Some(sorted[len / 2])
    }

    /// Connections searching (not in a ready check): their queue and when
    /// their search began.
    pub fn searching(&self) -> impl Iterator<Item = (QueueConn, Mode, Instant)> + '_ {
        Mode::ALL.into_iter().flat_map(move |mode| {
            self.queues[mode.index()].iter().flat_map(move |id| {
                let party = &self.parties[id];
                let since = party.since;
                party
                    .members
                    .iter()
                    .filter_map(move |member| since.map(|since| (member.conn, mode, since)))
            })
        })
    }

    /// If `conn`'s party is currently searching (not in a ready check), returns
    /// its queue, search start time, and member connections.
    pub fn searching_party(&self, conn: QueueConn) -> Option<(Mode, Instant, Vec<QueueConn>)> {
        let party = self.parties.get(self.of.get(&conn)?)?;
        let State::Queued(mode) = party.state else {
            return None;
        };
        let since = party.since?;
        let members = party.members.iter().map(|member| member.conn).collect();
        Some((mode, since, members))
    }

    /// The other members of `conn`'s party, in party order (leader first);
    /// empty on one's own. The door names them when it sends the view.
    pub fn party_mates(&self, conn: QueueConn) -> Vec<QueueConn> {
        self.of
            .get(&conn)
            .and_then(|id| self.parties.get(id))
            .map(|party| {
                party
                    .members
                    .iter()
                    .map(|member| member.conn)
                    .filter(|&other| other != conn)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The current party view for everyone in each of these connections'
    /// parties (each party once), say after a member's name changed.
    pub fn announce_parties_of(&self, conns: impl IntoIterator<Item = QueueConn>) -> Vec<Out> {
        let mut ids: Vec<u64> = conns
            .into_iter()
            .filter_map(|conn| self.of.get(&conn).copied())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let mut out = Vec::new();
        for id in ids {
            self.announce(id, &mut out);
        }
        out
    }

    fn view(&self, party: &Party, conn: QueueConn) -> PartyView {
        PartyView {
            code: party.code.clone(),
            members: party.members.len(),
            leader: party.members.first().is_some_and(|m| m.conn == conn),
            queue: match party.state {
                State::Idle => None,
                State::Queued(mode) => Some(mode),
                State::Forming(id) => Some(self.forming[&id].mode),
            },
        }
    }

    /// Tells every member of `party` how it stands.
    fn announce(&self, party: u64, out: &mut Vec<Out>) {
        if let Some(party) = self.parties.get(&party) {
            for member in &party.members {
                out.push(Out::Party {
                    conn: member.conn,
                    view: self.view(party, member.conn),
                });
            }
        }
    }

    /// Tells everyone in ready check `check` where it stands.
    fn ready_status(&self, check: u64, now: Instant, out: &mut Vec<Out>) {
        let Some(forming) = self.forming.get(&check) else {
            return;
        };
        let members: Vec<QueueConn> = forming
            .parties
            .iter()
            .flat_map(|id| self.parties[id].members.iter().map(|m| m.conn))
            .collect();
        for conn in &members {
            out.push(Out::ReadyCheck {
                conn: *conn,
                check: ReadyCheck {
                    mode: forming.mode,
                    accepted: forming.accepted.len(),
                    total: members.len(),
                    you: forming.accepted.contains(conn),
                    remaining: forming.deadline.saturating_duration_since(now),
                },
            });
        }
    }

    fn guest_of(&self, conn: QueueConn) -> Guest {
        self.parties[&self.of[&conn]]
            .members
            .iter()
            .find(|member| member.conn == conn)
            .expect("member of its party")
            .guest
    }

    /// A new connection for `guest`, on its own. An older connection of the
    /// same guest is closed as replaced.
    pub fn connect(&mut self, conn: QueueConn, guest: Guest, now: Instant) -> Vec<Out> {
        let mut out = Vec::new();
        if let Some(old) = self.guests.get(&guest).copied() {
            out.extend(self.disconnect(old, now));
            if old != conn {
                out.push(Out::Close {
                    conn: old,
                    reason: Closed::Replaced,
                });
            }
        }
        if self.of.contains_key(&conn) {
            out.extend(self.disconnect(conn, now));
        }
        self.alone(conn, guest, &mut out);
        out
    }

    /// Puts `conn` in a new party of its own.
    fn alone(&mut self, conn: QueueConn, guest: Guest, out: &mut Vec<Out>) {
        let id = self.id();
        self.parties.insert(
            id,
            Party {
                code: None,
                members: vec![Member { conn, guest }],
                state: State::Idle,
                since: None,
            },
        );
        self.of.insert(conn, id);
        self.guests.insert(guest, conn);
        self.announce(id, out);
    }

    /// The connection went away.
    pub fn disconnect(&mut self, conn: QueueConn, now: Instant) -> Vec<Out> {
        let mut out = self.remove_from_lobby(conn, None);
        out.extend(self.remove_member(conn, now));
        out
    }

    /// Takes `conn` out of its party, which stops searching (or fails its
    /// ready check).
    fn remove_member(&mut self, conn: QueueConn, now: Instant) -> Vec<Out> {
        let mut out = Vec::new();
        let Some(id) = self.of.remove(&conn) else {
            return out;
        };
        let party = self.parties.get_mut(&id).expect("member's party exists");
        let at = party
            .members
            .iter()
            .position(|member| member.conn == conn)
            .expect("member of its party");
        let member = party.members.remove(at);
        self.guests.remove(&member.guest);
        match party.state {
            State::Idle => {}
            State::Queued(mode) => {
                self.waiting[mode.index()] -= 1;
                self.unqueue(id);
                for member in &self.parties[&id].members {
                    out.push(Out::Stopped {
                        conn: member.conn,
                        reason: Stopped::MemberLeft,
                    });
                }
            }
            State::Forming(check) => {
                let forming = self.forming.get_mut(&check).expect("check exists");
                self.waiting[forming.mode.index()] -= 1;
                forming.accepted.remove(&conn);
                forming.broken.insert(id);
                out.extend(self.fail(check, now));
            }
        }
        if self.parties[&id].members.is_empty() {
            let party = self.parties.remove(&id).expect("exists");
            if let Some(code) = party.code {
                self.codes.remove(&code);
            }
        } else {
            self.announce(id, &mut out);
        }
        out
    }

    /// Takes a queued party out of its queue (not out of a ready check).
    fn unqueue(&mut self, id: u64) {
        let party = self.parties.get_mut(&id).expect("party exists");
        if let State::Queued(mode) = party.state {
            self.waiting[mode.index()] -= party.members.len();
            self.queues[mode.index()].retain(|queued| *queued != id);
        }
        party.state = State::Idle;
        party.since = None;
    }

    fn new_code(&mut self) -> String {
        loop {
            let code: String = (0..CODE_LENGTH)
                .map(|_| CODE_ALPHABET[(self.random)(CODE_ALPHABET.len())] as char)
                .collect();
            if !self.codes.contains_key(&code) && !self.lobby_codes.contains_key(&code) {
                return code;
            }
        }
    }

    /// Everyone in `conn`'s party (itself included).
    pub fn party_members(&self, conn: QueueConn) -> Vec<QueueConn> {
        self.of
            .get(&conn)
            .map(|id| {
                self.parties[id]
                    .members
                    .iter()
                    .map(|member| member.conn)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether `conn`'s party is searching or in a ready check.
    pub fn is_searching(&self, conn: QueueConn) -> bool {
        self.of
            .get(&conn)
            .is_some_and(|id| self.parties[id].state != State::Idle)
    }

    /// The code a friend can join `conn` with (social-identity step 4): its
    /// custom lobby's, else its party's, giving a player on their own a
    /// party first. `None` (with the refusal) when that is not possible now.
    pub fn invite_code(&mut self, conn: QueueConn, now: Instant) -> (Option<Invite>, Vec<Out>) {
        if let Some(code) = self.lobby_code(conn) {
            return (Some(Invite::Lobby(code)), Vec::new());
        }
        let Some(&id) = self.of.get(&conn) else {
            return (None, Vec::new());
        };
        if let Some(code) = self.parties[&id].code.clone() {
            return (Some(Invite::Party(code)), Vec::new());
        }
        let out = self.create_party(conn, now);
        let code = self
            .of
            .get(&conn)
            .and_then(|id| self.parties[id].code.clone());
        (code.map(Invite::Party), out)
    }

    /// Gives `conn` a party with an invite code: its own party, or a new
    /// one if it was in someone else's.
    pub fn create_party(&mut self, conn: QueueConn, now: Instant) -> Vec<Out> {
        let Some(&id) = self.of.get(&conn) else {
            return Vec::new();
        };
        if self.in_lobby.contains_key(&conn) {
            return vec![Out::Refused {
                conn,
                reason: Refused::InLobby,
            }];
        }
        if self.parties[&id].state != State::Idle {
            return vec![Out::Refused {
                conn,
                reason: Refused::PartyBusy,
            }];
        }
        let mut out = Vec::new();
        if self.parties[&id].members.len() > 1 {
            let guest = self.guest_of(conn);
            out.extend(self.remove_member(conn, now));
            self.alone(conn, guest, &mut Vec::new());
        }
        let id = self.of[&conn];
        if self.parties[&id].code.is_none() {
            let code = self.new_code();
            self.codes.insert(code.clone(), id);
            self.parties.get_mut(&id).expect("exists").code = Some(code);
        }
        self.announce(id, &mut out);
        out
    }

    /// Joins the party with invite `code`.
    pub fn join_party(&mut self, conn: QueueConn, code: &str, now: Instant) -> Vec<Out> {
        let Some(&current) = self.of.get(&conn) else {
            return Vec::new();
        };
        let refuse = |reason| vec![Out::Refused { conn, reason }];
        if self.in_lobby.contains_key(&conn) {
            return refuse(Refused::InLobby);
        }
        let Some(&target) = self.codes.get(&code.trim().to_ascii_uppercase()) else {
            return refuse(Refused::NoSuchParty);
        };
        if target == current {
            let mut out = Vec::new();
            self.announce(target, &mut out);
            return out;
        }
        if self.parties[&target].state != State::Idle || self.parties[&current].state != State::Idle
        {
            return refuse(Refused::PartyBusy);
        }
        if self.parties[&target].members.len() >= MAX_PARTY {
            return refuse(Refused::PartyFull);
        }
        let guest = self.guest_of(conn);
        let mut out = self.remove_member(conn, now);
        self.guests.insert(guest, conn);
        self.of.insert(conn, target);
        self.parties
            .get_mut(&target)
            .expect("exists")
            .members
            .push(Member { conn, guest });
        self.announce(target, &mut out);
        out
    }

    /// Leaves the party for one of one's own.
    pub fn leave_party(&mut self, conn: QueueConn, now: Instant) -> Vec<Out> {
        let Some(&id) = self.of.get(&conn) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if self.parties[&id].members.len() == 1 {
            // Alone already: just drop the invite code.
            if self.parties[&id].state != State::Idle {
                return vec![Out::Refused {
                    conn,
                    reason: Refused::PartyBusy,
                }];
            }
            if let Some(code) = self.parties.get_mut(&id).expect("exists").code.take() {
                self.codes.remove(&code);
            }
            self.announce(id, &mut out);
            return out;
        }
        let guest = self.guest_of(conn);
        out.extend(self.remove_member(conn, now));
        self.alone(conn, guest, &mut out);
        out
    }

    /// The leader queues its party for `mode`, or moves it to `mode`.
    pub fn queue(&mut self, conn: QueueConn, mode: Mode, now: Instant) -> Vec<Out> {
        let Some(&id) = self.of.get(&conn) else {
            return Vec::new();
        };
        let party = &self.parties[&id];
        let refuse = |reason| vec![Out::Refused { conn, reason }];
        if self.in_lobby.contains_key(&conn) {
            return refuse(Refused::InLobby);
        }
        if party.members[0].conn != conn {
            return refuse(Refused::NotLeader);
        }
        if party.members.len() > mode.max_party() {
            return refuse(Refused::PartyTooBig);
        }
        match party.state {
            State::Forming(_) => return refuse(Refused::PartyBusy),
            State::Queued(current) if current == mode => return Vec::new(),
            State::Queued(_) => self.unqueue(id),
            State::Idle => {}
        }
        self.queues[mode.index()].push_back(id);
        let party = self.parties.get_mut(&id).expect("exists");
        party.state = State::Queued(mode);
        party.since = Some(now);
        self.waiting[mode.index()] += party.members.len();
        self.stats.searches_by_size[party.members.len() - 1] += 1;
        let mut out = Vec::new();
        self.announce(id, &mut out);
        out.extend(self.form(mode, now));
        out
    }

    /// Any member stops the party's search (not during a ready check: then
    /// decline instead).
    pub fn cancel(&mut self, conn: QueueConn) -> Vec<Out> {
        let Some(&id) = self.of.get(&conn) else {
            return Vec::new();
        };
        match self.parties[&id].state {
            State::Queued(_) => {
                self.unqueue(id);
                let mut out: Vec<Out> = self.parties[&id]
                    .members
                    .iter()
                    .map(|member| Out::Stopped {
                        conn: member.conn,
                        reason: Stopped::Cancelled,
                    })
                    .collect();
                self.announce(id, &mut out);
                out
            }
            State::Forming(_) => vec![Out::Refused {
                conn,
                reason: Refused::PartyBusy,
            }],
            State::Idle => Vec::new(),
        }
    }

    /// `conn` accepted the match it was offered.
    pub fn accept(&mut self, conn: QueueConn, now: Instant) -> Vec<Out> {
        let Some(&id) = self.of.get(&conn) else {
            return Vec::new();
        };
        let State::Forming(check) = self.parties[&id].state else {
            return Vec::new();
        };
        let forming = self.forming.get_mut(&check).expect("check exists");
        if !forming.accepted.insert(conn) {
            return Vec::new();
        }
        self.stats.accepts += 1;
        let everyone = forming.parties.iter().all(|party| {
            self.parties[party]
                .members
                .iter()
                .all(|member| forming.accepted.contains(&member.conn))
        });
        let mut out = Vec::new();
        if !everyone {
            self.ready_status(check, now, &mut out);
            return out;
        }
        let forming = self.forming.remove(&check).expect("exists");
        self.stats.checks_completed += 1;
        // Team sides at random; members in random order within their team.
        let mut teams: [Vec<QueueConn>; 2] = forming.teams.map(|parties| {
            parties
                .iter()
                .flat_map(|party| self.parties[party].members.iter().map(|m| m.conn))
                .collect()
        });
        if (self.random)(2) == 1 {
            teams.swap(0, 1);
        }
        for team in &mut teams {
            for at in (1..team.len()).rev() {
                team.swap(at, (self.random)(at + 1));
            }
        }
        let seats = (0..forming.mode.players())
            .map(|seat| teams[seat % 2][seat / 2])
            .collect();
        out.push(Out::Start {
            mode: forming.mode,
            seats,
        });
        // Parties stay together after the match starts, ready to queue
        // again when it is over. Their waits feed the expected wait.
        let waits = &mut self.waits[forming.mode.index()];
        for id in &forming.parties {
            let party = self.parties.get_mut(id).expect("exists");
            self.waiting[forming.mode.index()] -= party.members.len();
            if let Some(since) = party.since.take() {
                let waited = now.saturating_duration_since(since);
                if self.found.len() < FOUND_WAITS_BUFFER {
                    self.found.push((forming.mode, waited));
                }
                waits.push_back(waited);
                if waits.len() > RECENT_WAITS {
                    waits.pop_front();
                }
            }
            party.state = State::Idle;
        }
        for id in &forming.parties {
            self.announce(*id, &mut out);
        }
        out.extend(self.form(forming.mode, now));
        out
    }

    /// `conn` declined the match: the ready check ends at once.
    pub fn decline(&mut self, conn: QueueConn, now: Instant) -> Vec<Out> {
        let Some(&id) = self.of.get(&conn) else {
            return Vec::new();
        };
        let State::Forming(check) = self.parties[&id].state else {
            return Vec::new();
        };
        let forming = self.forming.get_mut(&check).expect("check exists");
        forming.accepted.remove(&conn);
        forming.declined.insert(conn);
        self.stats.declines += 1;
        self.fail(check, now)
    }

    /// Fails ready checks whose time is up.
    pub fn tick(&mut self, now: Instant) -> Vec<Out> {
        if !self.forming.values().any(|forming| now >= forming.deadline) {
            return Vec::new();
        }
        let expired: Vec<u64> = self
            .forming
            .iter()
            .filter(|(_, forming)| now >= forming.deadline)
            .map(|(id, _)| *id)
            .collect();
        expired
            .into_iter()
            .flat_map(|id| self.fail(id, now))
            .collect()
    }

    /// Ends a ready check that cannot complete. Parties whose members all
    /// accepted go back to the front of the queue; every other party leaves
    /// the queue, each member told whether they or a party member declined
    /// or did not accept in time.
    fn fail(&mut self, check: u64, now: Instant) -> Vec<Out> {
        let Some(forming) = self.forming.remove(&check) else {
            return Vec::new();
        };
        self.stats.checks_failed += 1;
        let mode = forming.mode;
        let mut out = Vec::new();
        let mut back = Vec::new();
        for id in forming.parties {
            let Some(party) = self.parties.get_mut(&id) else {
                continue;
            };
            let complete = party
                .members
                .iter()
                .all(|member| forming.accepted.contains(&member.conn));
            if complete && !forming.broken.contains(&id) {
                party.state = State::Queued(mode);
                for member in &party.members {
                    out.push(Out::Requeued {
                        conn: member.conn,
                        mode,
                    });
                }
                back.push(id);
                continue;
            }
            party.state = State::Idle;
            party.since = None;
            self.waiting[mode.index()] -= party.members.len();
            let someone_declined = party
                .members
                .iter()
                .any(|member| forming.declined.contains(&member.conn));
            for member in &party.members {
                let reason = if forming.declined.contains(&member.conn) {
                    Stopped::Declined
                } else if !forming.accepted.contains(&member.conn) {
                    Stopped::NotAccepted
                } else if forming.broken.contains(&id) {
                    Stopped::MemberLeft
                } else if someone_declined {
                    Stopped::MemberDeclined
                } else {
                    Stopped::MemberNotAccepted
                };
                out.push(Out::Stopped {
                    conn: member.conn,
                    reason,
                });
            }
            self.announce(id, &mut out);
        }
        let queue = &mut self.queues[mode.index()];
        for id in back.into_iter().rev() {
            queue.push_front(id);
        }
        out.extend(self.form(mode, now));
        out
    }

    /// Picks parties from the front of `mode`'s queue that fill both teams:
    /// in queue order, each party goes to the fuller team it still fits in.
    /// The party at the front is always included, so later arrivals never
    /// skip it forever; a party too big for the room left waits for the
    /// next match without holding up the parties behind it.
    fn pick(&self, mode: Mode) -> Option<[Vec<u64>; 2]> {
        let size = mode.team_size();
        let mut teams: [Vec<u64>; 2] = [Vec::new(), Vec::new()];
        let mut filled = [0usize; 2];
        for &id in self.queues[mode.index()].iter().take(FORMING_WINDOW) {
            let members = self.parties[&id].members.len();
            let fits = |team: usize| filled[team] + members <= size;
            let team = match (fits(0), fits(1)) {
                (true, true) => usize::from(filled[1] > filled[0]),
                (true, false) => 0,
                (false, true) => 1,
                (false, false) => continue,
            };
            teams[team].push(id);
            filled[team] += members;
            if filled == [size, size] {
                return Some(teams);
            }
        }
        None
    }

    /// Starts ready checks while a queue can fill a match.
    fn form(&mut self, mode: Mode, now: Instant) -> Vec<Out> {
        let mut out = Vec::new();
        while let Some(teams) = self.pick(mode) {
            // At most 2 * MAX_PARTY (6) parties fill one match.
            let mut chosen = [0u64; MAX_PARTY * 2];
            let mut chosen_len = 0usize;
            for id in teams.iter().flatten().copied() {
                chosen[chosen_len] = id;
                chosen_len += 1;
            }
            let chosen = &chosen[..chosen_len];
            let parties: Vec<u64> = self.queues[mode.index()]
                .iter()
                .copied()
                .filter(|id| chosen.contains(id))
                .collect();
            self.queues[mode.index()].retain(|id| !chosen.contains(id));
            let check = self.id();
            self.stats.checks_started += 1;
            for id in &parties {
                self.parties.get_mut(id).expect("exists").state = State::Forming(check);
            }
            self.forming.insert(
                check,
                Forming {
                    mode,
                    parties,
                    teams,
                    accepted: HashSet::new(),
                    declined: HashSet::new(),
                    broken: HashSet::new(),
                    deadline: now + self.ready_timeout,
                },
            );
            self.ready_status(check, now, &mut out);
        }
        out
    }

    /// Checks that every index agrees (tests).
    #[cfg(test)]
    fn check(&self) {
        let members: usize = self.parties.values().map(|p| p.members.len()).sum();
        assert_eq!(members, self.of.len());
        assert_eq!(self.guests.len(), self.of.len());
        for (id, party) in &self.parties {
            assert!(!party.members.is_empty());
            assert!(party.members.len() <= MAX_PARTY);
            for member in &party.members {
                assert_eq!(self.of[&member.conn], *id);
                assert_eq!(self.guests[&member.guest], member.conn);
            }
            match party.state {
                State::Queued(mode) => {
                    let entries = self.queues[mode.index()].iter().filter(|q| *q == id);
                    assert_eq!(entries.count(), 1);
                    assert!(party.members.len() <= mode.max_party());
                    assert!(party.since.is_some());
                }
                State::Forming(check) => {
                    assert!(self.forming[&check].parties.contains(id));
                    assert!(party.since.is_some());
                }
                State::Idle => assert!(party.since.is_none()),
            }
            if let Some(code) = &party.code {
                assert_eq!(self.codes[code], *id);
            }
        }
        let coded = self.parties.values().filter(|p| p.code.is_some()).count();
        assert_eq!(self.codes.len(), coded);
        for queue in &self.queues {
            for id in queue {
                assert!(matches!(self.parties[id].state, State::Queued(_)));
            }
        }
        let mut expected_waiting = [0; 3];
        for party in self.parties.values() {
            let mode = match party.state {
                State::Queued(mode) => mode,
                State::Forming(id) => self.forming[&id].mode,
                State::Idle => continue,
            };
            expected_waiting[mode.index()] += party.members.len();
        }
        assert_eq!(self.waiting, expected_waiting);
        for waits in &self.waits {
            assert!(waits.len() <= RECENT_WAITS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const READY: Duration = Duration::from_secs(15);
    /// Deterministic pseudo-random choices (xorshift), so invite codes
    /// differ and runs repeat exactly.
    fn pseudo(bound: usize) -> usize {
        use std::sync::atomic::{AtomicU64, Ordering};
        static STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
        let mut x = STATE.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        STATE.store(x, Ordering::Relaxed);
        (x % bound as u64) as usize
    }
    fn mm() -> Matchmaker {
        Matchmaker::new(READY, pseudo)
    }
    /// Connections offered a match (first status of a ready check).
    fn offered(out: &[Out]) -> Vec<QueueConn> {
        let mut found: Vec<_> = out
            .iter()
            .filter_map(|out| match out {
                Out::ReadyCheck { conn, check } if check.accepted == 0 => Some(*conn),
                _ => None,
            })
            .collect();
        found.sort();
        found
    }
    fn starts(out: &[Out]) -> Vec<(Mode, Vec<QueueConn>)> {
        out.iter()
            .filter_map(|out| match out {
                Out::Start { mode, seats } => Some((*mode, seats.clone())),
                _ => None,
            })
            .collect()
    }
    fn view(out: &[Out], of: QueueConn) -> Option<PartyView> {
        out.iter().rev().find_map(|out| match out {
            Out::Party { conn, view } if *conn == of => Some(view.clone()),
            _ => None,
        })
    }
    fn stopped(out: &[Out], of: QueueConn) -> Option<Stopped> {
        out.iter().find_map(|out| match out {
            Out::Stopped { conn, reason } if *conn == of => Some(*reason),
            _ => None,
        })
    }
    fn code(out: &[Out], of: QueueConn) -> String {
        view(out, of)
            .and_then(|view| view.code)
            .expect("an invite code")
    }
    fn refused(conn: QueueConn, reason: Refused) -> Vec<Out> {
        vec![Out::Refused { conn, reason }]
    }
    /// Connects players `conns` (guest = conn).
    fn connect(mm: &mut Matchmaker, conns: impl IntoIterator<Item = QueueConn>, now: Instant) {
        for conn in conns {
            mm.connect(conn, conn.into(), now);
        }
    }
    /// A party of `conns`, led by the first.
    fn party(mm: &mut Matchmaker, conns: &[QueueConn], now: Instant) -> String {
        let invite = code(&mm.create_party(conns[0], now), conns[0]);
        for conn in &conns[1..] {
            mm.join_party(*conn, &invite, now);
        }
        invite
    }
    fn accept_all(mm: &mut Matchmaker, conns: &[QueueConn], now: Instant) -> Vec<Out> {
        conns
            .iter()
            .flat_map(|conn| mm.accept(*conn, now))
            .collect()
    }

    #[test]
    fn party_mates_list_the_others_leader_first_and_reannounce_on_rename() {
        let now = Instant::now();
        let mut mm = Matchmaker::new(Duration::from_secs(15), |_| 0);
        connect(&mut mm, 1..=4, now);
        let out = mm.create_party(1, now);
        let invite = code(&out, 1);
        mm.join_party(2, &invite, now);
        mm.join_party(3, &invite, now);
        assert_eq!(mm.party_mates(1), vec![2, 3]);
        assert_eq!(mm.party_mates(3), vec![1, 2]);
        assert!(mm.party_mates(4).is_empty());
        // Renamed members re-announce each party once, to every member
        // (player 4 is a party of one).
        let out = mm.announce_parties_of([2, 3, 4]);
        assert_eq!(out.len(), 4, "{out:?}");
    }

    #[test]
    fn three_queues_and_their_party_limits() {
        for mode in Mode::ALL {
            assert_eq!(Mode::from_id(mode.id()), Some(mode));
        }
        assert_eq!(Mode::from_id("solo-2v2"), None, "no separate solo queues");
        assert_eq!(Mode::Duel.max_party(), 1);
        assert_eq!(Mode::Team3v3.max_party(), 3);
        assert_eq!(Mode::Team2v2.players(), 4);
    }

    #[test]
    fn everyone_offered_a_match_sees_the_check_fill_up_and_it_starts() {
        let now = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=4, now);
        for conn in 1..=3 {
            assert!(offered(&mm.queue(conn, Mode::Team2v2, now)).is_empty());
        }
        assert_eq!(mm.waiting()[Mode::Team2v2.index()], 3);
        let out = mm.queue(4, Mode::Team2v2, now);
        assert_eq!(offered(&out), [1, 2, 3, 4]);
        assert!(out.contains(&Out::ReadyCheck {
            conn: 2,
            check: ReadyCheck {
                mode: Mode::Team2v2,
                accepted: 0,
                total: 4,
                you: false,
                remaining: READY,
            }
        }));
        // Each acceptance is shown to everyone, with the time left.
        let later = now + Duration::from_secs(4);
        let out = mm.accept(3, later);
        assert_eq!(out.len(), 4);
        assert!(out.contains(&Out::ReadyCheck {
            conn: 3,
            check: ReadyCheck {
                mode: Mode::Team2v2,
                accepted: 1,
                total: 4,
                you: true,
                remaining: Duration::from_secs(11),
            }
        }));
        assert!(
            mm.accept(3, later).is_empty(),
            "accepting twice changes nothing"
        );
        accept_all(&mut mm, &[1, 2], later);
        let (mode, seats) = starts(&mm.accept(4, later)).remove(0);
        assert_eq!(mode, Mode::Team2v2);
        let mut sorted = seats.clone();
        sorted.sort();
        assert_eq!(sorted, [1, 2, 3, 4]);
        // Still connected, no longer searching.
        assert!((1..=4).all(|conn| mm.contains(conn)));
        assert_eq!(mm.waiting(), [0; 3]);
        mm.check();
    }

    #[test]
    fn a_decline_ends_the_check_at_once_and_those_who_accepted_go_first() {
        let now = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=5, now);
        for conn in 1..=4 {
            mm.queue(conn, Mode::Team2v2, now);
        }
        mm.queue(5, Mode::Team2v2, now);
        accept_all(&mut mm, &[1, 3], now);
        let out = mm.decline(2, now);
        assert_eq!(stopped(&out, 2), Some(Stopped::Declined));
        assert_eq!(stopped(&out, 4), Some(Stopped::NotAccepted));
        for conn in [1, 3] {
            assert!(out.contains(&Out::Requeued {
                conn,
                mode: Mode::Team2v2
            }));
        }
        // Still connected, out of the queue.
        assert!(mm.contains(2) && mm.contains(4));
        assert_eq!(view(&out, 2).unwrap().queue, None);
        // 1 and 3 are back in front of 5; one more player completes a match.
        assert_eq!(mm.waiting()[Mode::Team2v2.index()], 3);
        assert_eq!(offered(&mm.queue(2, Mode::Team2v2, now)), [1, 2, 3, 5]);
        mm.check();
    }

    #[test]
    fn players_who_do_not_accept_in_time_leave_the_queue() {
        let start = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=2, start);
        mm.queue(1, Mode::Duel, start);
        mm.queue(2, Mode::Duel, start);
        mm.accept(1, start);
        assert!(mm.tick(start + READY - Duration::from_millis(1)).is_empty());
        let out = mm.tick(start + READY);
        assert_eq!(stopped(&out, 2), Some(Stopped::NotAccepted));
        assert!(out.contains(&Out::Requeued {
            conn: 1,
            mode: Mode::Duel
        }));
        assert!(mm.contains(2), "not disconnected");
        mm.check();
    }

    #[test]
    fn friends_form_a_party_by_code_and_the_leader_queues_it() {
        let now = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=3, now);
        let out = mm.create_party(1, now);
        let invite = code(&out, 1);
        assert_eq!(invite.len(), CODE_LENGTH);
        let out = mm.join_party(2, &format!(" {} ", invite.to_lowercase()), now);
        assert_eq!(
            view(&out, 1),
            Some(PartyView {
                code: Some(invite.clone()),
                members: 2,
                leader: true,
                queue: None
            })
        );
        assert!(!view(&out, 2).unwrap().leader);
        assert_eq!(
            mm.queue(2, Mode::Team2v2, now),
            refused(2, Refused::NotLeader)
        );
        assert_eq!(
            mm.queue(1, Mode::Duel, now),
            refused(1, Refused::PartyTooBig)
        );
        let out = mm.queue(1, Mode::Team2v2, now);
        assert_eq!(view(&out, 2).unwrap().queue, Some(Mode::Team2v2));
        assert_eq!(
            mm.join_party(3, &invite, now),
            refused(3, Refused::PartyBusy)
        );
        assert_eq!(
            mm.join_party(3, "NOPE22", now),
            refused(3, Refused::NoSuchParty)
        );
        let out = mm.cancel(2);
        assert_eq!(stopped(&out, 1), Some(Stopped::Cancelled));
        assert_eq!(view(&out, 1).unwrap().queue, None);
        mm.check();
    }

    #[test]
    fn parties_stay_together_and_teams_are_filled_with_others() {
        let now = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=6, now);
        party(&mut mm, &[1, 2], now);
        mm.queue(1, Mode::Team3v3, now);
        for conn in 3..=5 {
            assert!(offered(&mm.queue(conn, Mode::Team3v3, now)).is_empty());
        }
        assert_eq!(
            offered(&mm.queue(6, Mode::Team3v3, now)),
            [1, 2, 3, 4, 5, 6]
        );
        let (_, seats) = starts(&accept_all(&mut mm, &[1, 2, 3, 4, 5, 6], now)).remove(0);
        let team = |conn| seats.iter().position(|seat| *seat == conn).unwrap() % 2;
        assert_eq!(team(1), team(2), "the party plays together");
        assert_eq!(seats.iter().filter(|c| team(**c) == team(1)).count(), 3);
        mm.check();
    }

    #[test]
    fn a_party_whose_member_declines_leaves_the_queue_together() {
        let now = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=4, now);
        party(&mut mm, &[1, 2], now);
        mm.queue(1, Mode::Team2v2, now);
        mm.queue(3, Mode::Team2v2, now);
        mm.queue(4, Mode::Team2v2, now);
        accept_all(&mut mm, &[1, 3, 4], now);
        let out = mm.decline(2, now);
        assert_eq!(stopped(&out, 2), Some(Stopped::Declined));
        assert_eq!(stopped(&out, 1), Some(Stopped::MemberDeclined));
        assert_eq!(
            view(&out, 1).unwrap().members,
            2,
            "the party stays together"
        );
        assert!(out.contains(&Out::Requeued {
            conn: 3,
            mode: Mode::Team2v2
        }));
        mm.check();
    }

    #[test]
    fn a_party_too_big_for_the_room_left_waits_without_blocking_others() {
        let now = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=8, now);
        for pair in [[1, 2], [3, 4], [5, 6]] {
            party(&mut mm, &pair, now);
            mm.queue(pair[0], Mode::Team3v3, now);
        }
        assert!(offered(&mm.queue(7, Mode::Team3v3, now)).is_empty());
        assert_eq!(
            offered(&mm.queue(8, Mode::Team3v3, now)),
            [1, 2, 3, 4, 7, 8]
        );
        let out = accept_all(&mut mm, &[1, 2, 3, 4, 7, 8], now);
        assert_eq!(starts(&out).len(), 1);
        assert_eq!(mm.waiting()[Mode::Team3v3.index()], 2);
        mm.check();
    }

    #[test]
    fn parties_are_capped_and_leaving_hands_on_the_lead() {
        let now = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=4, now);
        let invite = party(&mut mm, &[1, 2, 3], now);
        assert_eq!(
            mm.join_party(4, &invite, now),
            refused(4, Refused::PartyFull)
        );
        let out = mm.leave_party(1, now);
        assert!(view(&out, 2).unwrap().leader, "the next member leads");
        assert_eq!(view(&out, 2).unwrap().members, 2);
        assert_eq!(view(&out, 1).unwrap().code, None);
        mm.disconnect(2, now);
        mm.disconnect(3, now);
        assert_eq!(
            mm.join_party(4, &invite, now),
            refused(4, Refused::NoSuchParty)
        );
        mm.check();
    }

    #[test]
    fn a_member_disconnecting_stops_the_party_search() {
        let now = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=2, now);
        party(&mut mm, &[1, 2], now);
        mm.queue(1, Mode::Team2v2, now);
        let out = mm.disconnect(2, now);
        assert_eq!(stopped(&out, 1), Some(Stopped::MemberLeft));
        assert_eq!(view(&out, 1).unwrap().queue, None);
        assert_eq!(mm.waiting()[Mode::Team2v2.index()], 0);
        mm.check();
    }

    #[test]
    fn a_guest_has_one_connection() {
        let now = Instant::now();
        let mut mm = mm();
        mm.connect(1, 42, now);
        mm.queue(1, Mode::Duel, now);
        let out = mm.connect(2, 42, now);
        assert!(out.contains(&Out::Close {
            conn: 1,
            reason: Closed::Replaced
        }));
        assert_eq!(mm.waiting()[Mode::Duel.index()], 0);
        mm.check();
    }

    #[test]
    fn the_expected_wait_is_the_median_of_recent_waits() {
        let start = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=2, start);
        assert_eq!(mm.expected_wait(Mode::Duel), None);
        // Duels found after 10 s, 30 s and 20 s (the first player waits).
        for (round, waited) in [10u64, 30, 20].into_iter().enumerate() {
            let at = start + Duration::from_secs(1000 * round as u64);
            mm.queue(1, Mode::Duel, at);
            let found = at + Duration::from_secs(waited);
            mm.queue(2, Mode::Duel, found);
            accept_all(&mut mm, &[1, 2], found);
            if round == 0 {
                // Two samples: the first player's wait, and the second's 0 s.
                assert_eq!(mm.expected_wait(Mode::Duel), None, "too few yet");
            }
        }
        // Samples: 10, 0, 30, 0, 20, 0 (the second player found at once).
        assert_eq!(mm.expected_wait(Mode::Duel), Some(Duration::from_secs(10)));
        assert_eq!(mm.expected_wait(Mode::Team2v2), None);
        // A failed check keeps the search's start for the next attempt.
        mm.queue(1, Mode::Duel, start);
        mm.queue(2, Mode::Duel, start);
        mm.accept(1, start);
        mm.decline(2, start);
        assert_eq!(mm.searching().collect::<Vec<_>>(), [(1, Mode::Duel, start)]);
        assert_eq!(mm.searching_party(1), Some((Mode::Duel, start, vec![1])));
        assert_eq!(mm.searching_party(2), None);
        mm.check();
    }

    #[test]
    fn stats_count_pops_accepts_declines_timeouts_and_found_waits() {
        let start = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=4, start);
        // A duel found after 7 s, accepted by both.
        mm.queue(1, Mode::Duel, start);
        let found = start + Duration::from_secs(7);
        mm.queue(2, Mode::Duel, found);
        accept_all(&mut mm, &[1, 2], found);
        // A repeated accept is not counted twice.
        mm.accept(1, found);
        assert_eq!(
            mm.take_found_waits().collect::<Vec<_>>(),
            [
                (Mode::Duel, Duration::from_secs(7)),
                (Mode::Duel, Duration::ZERO)
            ]
        );
        assert_eq!(mm.take_found_waits().count(), 0, "taken once");
        // One declined check, then one that times out.
        mm.queue(3, Mode::Duel, found);
        mm.queue(4, Mode::Duel, found);
        mm.accept(3, found);
        mm.decline(4, found);
        mm.queue(4, Mode::Duel, found);
        mm.tick(found + READY + Duration::from_secs(1));
        assert_eq!(
            mm.stats(),
            Stats {
                checks_started: 3,
                checks_completed: 1,
                checks_failed: 2,
                accepts: 3,
                declines: 1,
                // Solo searches: 1, 2, 3, 4 and 4 again after declining.
                searches_by_size: [5, 0, 0],
            }
        );
        assert_eq!(
            mm.take_found_waits().count(),
            0,
            "failed checks find nothing"
        );
        assert_eq!(mm.parties(), 0);
        mm.create_party(1, found);
        assert_eq!(mm.parties(), 1);
        assert_eq!(mm.lobbies(), 0);
        mm.create_lobby(2, Mode::Duel, found);
        assert_eq!(mm.lobbies(), 1);
        mm.check();
    }

    #[test]
    fn random_requests_keep_the_books_balanced() {
        // Random requests from forty players in a fixed pseudo-random order;
        // every index is checked after each step.
        let start = Instant::now();
        let mut mm = mm();
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = |bound: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % bound
        };
        let mut codes: Vec<String> = Vec::new();
        let mut now = start;
        let mut matches = 0;
        for _ in 0..30_000u64 {
            now += Duration::from_millis(next(400));
            let conn = next(40) + 1;
            if !mm.contains(conn) {
                mm.connect(conn, Guest::from(conn), now);
            }
            let mode = Mode::ALL[next(3) as usize];
            let out = match next(13) {
                0 => mm.disconnect(conn, now),
                1 => mm.create_party(conn, now),
                2 if !codes.is_empty() => {
                    let code = codes[next(codes.len() as u64) as usize].clone();
                    mm.join_party(conn, &code, now)
                }
                3 => mm.leave_party(conn, now),
                4 => mm.cancel(conn),
                5 => mm.decline(conn, now),
                6..=8 => mm.queue(conn, mode, now),
                9..=11 => mm.accept(conn, now),
                _ => mm.tick(now),
            };
            for out in &out {
                if let Out::Party { view, .. } = out
                    && let Some(code) = &view.code
                {
                    codes.push(code.clone());
                }
            }
            for (mode, seats) in starts(&out) {
                matches += 1;
                assert_eq!(seats.len(), mode.players());
                let distinct: HashSet<_> = seats.iter().collect();
                assert_eq!(distinct.len(), seats.len(), "one seat per player");
                for conn in &seats {
                    assert!(mm.contains(*conn));
                    assert!(mm.searching().all(|(searching, ..)| searching != *conn));
                }
            }
            mm.check();
        }
        assert!(matches > 10, "{matches}");
    }
}
