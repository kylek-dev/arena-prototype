//! Queues and parties (ADR 0046).
//!
//! Modes: `1v1`, `solo-2v2` and `solo-3v3` take players queueing alone;
//! `2v2` and `3v3` take parties of up to the team size, and fill each team
//! from other parties or single players in that queue (user decision
//! 2026-11-15). Party members always play on the same team.
//!
//! Every connected player belongs to a party; a player who has not formed
//! or joined one is a party of one without an invite code. A party's leader
//! (its first member) queues it; any member may cancel the search.
//!
//! When a queue can fill both teams, the parties chosen are asked to confirm
//! they are still there (a ready check, answered automatically by the game).
//! If everyone confirms in time, the match starts, and the parties stay
//! together to queue again afterwards. Otherwise players who did not
//! confirm are dropped, parties that lost a member stop searching, and
//! complete parties return to the front of the queue in their order.
//!
//! A clock-free state machine like [`crate::session::Session`]: the caller
//! passes in time and connection identifiers and carries out the returned
//! [`Out`] actions. It never issues tickets; [`Out::Start`] asks for one per
//! seat.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use tokio::time::Instant;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Duel,
    Team2v2,
    Team3v3,
    Solo2v2,
    Solo3v3,
}
impl Mode {
    pub const ALL: [Mode; 5] = [
        Mode::Duel,
        Mode::Team2v2,
        Mode::Team3v3,
        Mode::Solo2v2,
        Mode::Solo3v3,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Mode::Duel => "1v1",
            Mode::Team2v2 => "2v2",
            Mode::Team3v3 => "3v3",
            Mode::Solo2v2 => "solo-2v2",
            Mode::Solo3v3 => "solo-3v3",
        }
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.id() == id)
    }
    pub fn team_size(self) -> usize {
        match self {
            Mode::Duel => 1,
            Mode::Team2v2 | Mode::Solo2v2 => 2,
            Mode::Team3v3 | Mode::Solo3v3 => 3,
        }
    }
    pub fn players(self) -> usize {
        2 * self.team_size()
    }
    /// The largest party this queue takes.
    pub fn max_party(self) -> usize {
        match self {
            Mode::Team2v2 | Mode::Team3v3 => self.team_size(),
            _ => 1,
        }
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
    /// It did not confirm in time when a match was found.
    NotReady,
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
}
impl Refused {
    pub fn code(self) -> &'static str {
        match self {
            Refused::NotLeader => "not_leader",
            Refused::PartyTooBig => "party_too_big",
            Refused::NoSuchParty => "no_such_party",
            Refused::PartyFull => "party_full",
            Refused::PartyBusy => "party_busy",
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
    /// A member did not confirm a found match.
    MemberNotReady,
}
impl Stopped {
    pub fn code(self) -> &'static str {
        match self {
            Stopped::Cancelled => "cancelled",
            Stopped::MemberLeft => "member_left",
            Stopped::MemberNotReady => "member_not_ready",
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
    Party {
        conn: QueueConn,
        view: PartyView,
    },
    /// A match was found: confirm with [`Matchmaker::ready`].
    Found {
        conn: QueueConn,
        mode: Mode,
    },
    /// Someone else did not confirm; back at the front of the queue.
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
    /// Everyone confirmed: issue a ticket per seat. `seats[i]` takes seat
    /// `i` (even seats are one team, odd seats the other). The players stay
    /// connected and in their parties, no longer searching.
    Start {
        mode: Mode,
        seats: Vec<QueueConn>,
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
}

#[derive(Debug)]
struct Forming {
    mode: Mode,
    /// Queue order, for putting complete parties back.
    parties: Vec<u64>,
    /// Party ids per team.
    teams: [Vec<u64>; 2],
    ready: HashSet<QueueConn>,
    /// Parties that lost a member during the check.
    broken: HashSet<u64>,
    deadline: Instant,
}

/// Players waiting per mode, indexed by [`Mode::index`].
pub type Waiting = [usize; 5];

#[derive(Debug)]
pub struct Matchmaker {
    queues: [VecDeque<u64>; 5],
    parties: HashMap<u64, Party>,
    codes: HashMap<String, u64>,
    forming: HashMap<u64, Forming>,
    next_id: u64,
    /// Each connection's party.
    of: HashMap<QueueConn, u64>,
    guests: HashMap<Guest, QueueConn>,
    ready_timeout: Duration,
    /// A random value in `0..bound`: invite codes, team sides and seats.
    random: fn(usize) -> usize,
}

impl Matchmaker {
    pub fn new(ready_timeout: Duration, random: fn(usize) -> usize) -> Self {
        Self {
            queues: Default::default(),
            parties: HashMap::new(),
            codes: HashMap::new(),
            forming: HashMap::new(),
            next_id: 0,
            of: HashMap::new(),
            guests: HashMap::new(),
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
        let mut waiting = [0; 5];
        for party in self.parties.values() {
            let mode = match party.state {
                State::Queued(mode) => mode,
                State::Forming(id) => self.forming[&id].mode,
                State::Idle => continue,
            };
            waiting[mode.index()] += party.members.len();
        }
        waiting
    }

    /// Connections searching (not in a ready check), with their mode.
    pub fn searching(&self) -> impl Iterator<Item = (QueueConn, Mode)> + '_ {
        self.parties.values().flat_map(|party| {
            let mode = match party.state {
                State::Queued(mode) => Some(mode),
                _ => None,
            };
            party
                .members
                .iter()
                .filter_map(move |member| mode.map(|mode| (member.conn, mode)))
        })
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
            },
        );
        self.of.insert(conn, id);
        self.guests.insert(guest, conn);
        self.announce(id, out);
    }

    /// The connection went away.
    pub fn disconnect(&mut self, conn: QueueConn, now: Instant) -> Vec<Out> {
        self.remove_member(conn, now)
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
            State::Queued(_) => {
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
                forming.ready.remove(&conn);
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
            self.queues[mode.index()].retain(|queued| *queued != id);
        }
        party.state = State::Idle;
    }

    fn new_code(&mut self) -> String {
        loop {
            let code: String = (0..CODE_LENGTH)
                .map(|_| CODE_ALPHABET[(self.random)(CODE_ALPHABET.len())] as char)
                .collect();
            if !self.codes.contains_key(&code) {
                return code;
            }
        }
    }

    /// Gives `conn` a party with an invite code: its own party, or a new
    /// one if it was in someone else's.
    pub fn create_party(&mut self, conn: QueueConn, now: Instant) -> Vec<Out> {
        let Some(&id) = self.of.get(&conn) else {
            return Vec::new();
        };
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
        self.parties.get_mut(&id).expect("exists").state = State::Queued(mode);
        let mut out = Vec::new();
        self.announce(id, &mut out);
        out.extend(self.form(mode, now));
        out
    }

    /// Any member stops the party's search (not during a ready check).
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

    /// `conn` confirmed a found match.
    pub fn ready(&mut self, conn: QueueConn, now: Instant) -> Vec<Out> {
        let Some(&id) = self.of.get(&conn) else {
            return Vec::new();
        };
        let State::Forming(check) = self.parties[&id].state else {
            return Vec::new();
        };
        let forming = self.forming.get_mut(&check).expect("check exists");
        forming.ready.insert(conn);
        let everyone = forming.parties.iter().all(|party| {
            self.parties[party]
                .members
                .iter()
                .all(|member| forming.ready.contains(&member.conn))
        });
        if !everyone {
            return Vec::new();
        }
        let forming = self.forming.remove(&check).expect("exists");
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
        let mut out = vec![Out::Start {
            mode: forming.mode,
            seats,
        }];
        // Parties stay together after the match starts, ready to queue
        // again when it is over.
        for id in &forming.parties {
            self.parties.get_mut(id).expect("exists").state = State::Idle;
            self.announce(*id, &mut out);
        }
        out.extend(self.form(forming.mode, now));
        out
    }

    /// Fails ready checks whose time is up.
    pub fn tick(&mut self, now: Instant) -> Vec<Out> {
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

    /// Ends a ready check that cannot complete. Members who did not confirm
    /// are dropped; their parties, and parties that lost a member, stop
    /// searching; complete parties go back to the front of the queue.
    fn fail(&mut self, check: u64, now: Instant) -> Vec<Out> {
        let Some(forming) = self.forming.remove(&check) else {
            return Vec::new();
        };
        let mode = forming.mode;
        let mut out = Vec::new();
        let mut back = Vec::new();
        for id in forming.parties {
            let Some(party) = self.parties.get_mut(&id) else {
                continue;
            };
            party.state = State::Idle;
            let unready: Vec<QueueConn> = party
                .members
                .iter()
                .map(|member| member.conn)
                .filter(|conn| !forming.ready.contains(conn))
                .collect();
            if unready.is_empty() && !forming.broken.contains(&id) {
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
            for conn in unready {
                out.extend(self.remove_member(conn, now));
                out.push(Out::Close {
                    conn,
                    reason: Closed::NotReady,
                });
            }
            if let Some(party) = self.parties.get(&id) {
                let reason = if forming.broken.contains(&id) {
                    Stopped::MemberLeft
                } else {
                    Stopped::MemberNotReady
                };
                for member in &party.members {
                    out.push(Out::Stopped {
                        conn: member.conn,
                        reason,
                    });
                }
                self.announce(id, &mut out);
            }
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
            let chosen: HashSet<u64> = teams.iter().flatten().copied().collect();
            let parties: Vec<u64> = self.queues[mode.index()]
                .iter()
                .copied()
                .filter(|id| chosen.contains(id))
                .collect();
            self.queues[mode.index()].retain(|id| !chosen.contains(id));
            let check = self.id();
            for id in &parties {
                self.parties.get_mut(id).expect("exists").state = State::Forming(check);
                for member in &self.parties[id].members {
                    out.push(Out::Found {
                        conn: member.conn,
                        mode,
                    });
                }
            }
            self.forming.insert(
                check,
                Forming {
                    mode,
                    parties,
                    teams,
                    ready: HashSet::new(),
                    broken: HashSet::new(),
                    deadline: now + self.ready_timeout,
                },
            );
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
                }
                State::Forming(check) => assert!(self.forming[&check].parties.contains(id)),
                State::Idle => {}
            }
            if let Some(code) = &party.code {
                assert_eq!(self.codes[code], *id);
            }
        }
        assert_eq!(self.codes.len(), {
            let coded = self.parties.values().filter(|p| p.code.is_some());
            coded.count()
        });
        for queue in &self.queues {
            for id in queue {
                assert!(matches!(self.parties[id].state, State::Queued(_)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const READY: Duration = Duration::from_secs(10);
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
    fn found(out: &[Out]) -> Vec<QueueConn> {
        let mut found: Vec<_> = out
            .iter()
            .filter_map(|out| match out {
                Out::Found { conn, .. } => Some(*conn),
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
    fn all_ready(mm: &mut Matchmaker, conns: &[QueueConn], now: Instant) -> Vec<Out> {
        conns.iter().flat_map(|conn| mm.ready(*conn, now)).collect()
    }

    #[test]
    fn modes_round_trip_their_ids_and_limit_parties() {
        for mode in Mode::ALL {
            assert_eq!(Mode::from_id(mode.id()), Some(mode));
        }
        assert_eq!(Mode::from_id("4v4"), None);
        assert_eq!(Mode::Team3v3.max_party(), 3);
        assert_eq!(Mode::Solo3v3.max_party(), 1);
        assert_eq!(Mode::Duel.players(), 2);
    }

    #[test]
    fn solo_players_are_matched_when_everyone_confirms() {
        let now = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=4, now);
        for conn in 1..=3 {
            assert!(found(&mm.queue(conn, Mode::Solo2v2, now)).is_empty());
        }
        assert_eq!(mm.waiting()[Mode::Solo2v2.index()], 3);
        assert_eq!(found(&mm.queue(4, Mode::Solo2v2, now)), [1, 2, 3, 4]);
        assert!(all_ready(&mut mm, &[2, 4, 1], now).is_empty());
        let (mode, seats) = starts(&mm.ready(3, now)).remove(0);
        assert_eq!(mode, Mode::Solo2v2);
        let mut sorted = seats.clone();
        sorted.sort();
        assert_eq!(sorted, [1, 2, 3, 4]);
        // Still connected, no longer searching.
        assert!((1..=4).all(|conn| mm.contains(conn)));
        assert_eq!(mm.waiting(), [0; 5]);
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
        // Only the leader queues, and only where the party fits.
        assert_eq!(
            mm.queue(2, Mode::Team2v2, now),
            refused(2, Refused::NotLeader)
        );
        assert_eq!(
            mm.queue(1, Mode::Solo2v2, now),
            refused(1, Refused::PartyTooBig)
        );
        let out = mm.queue(1, Mode::Team2v2, now);
        assert_eq!(view(&out, 2).unwrap().queue, Some(Mode::Team2v2));
        assert_eq!(mm.waiting()[Mode::Team2v2.index()], 2);
        // A party that is searching takes nobody new.
        assert_eq!(
            mm.join_party(3, &invite, now),
            refused(3, Refused::PartyBusy)
        );
        assert_eq!(
            mm.join_party(3, "NOPE22", now),
            refused(3, Refused::NoSuchParty)
        );
        // Any member may cancel the search.
        let out = mm.cancel(2);
        assert!(out.contains(&Out::Stopped {
            conn: 1,
            reason: Stopped::Cancelled
        }));
        assert_eq!(view(&out, 1).unwrap().queue, None);
        mm.check();
    }

    #[test]
    fn parties_stay_together_and_teams_are_filled_with_others() {
        let now = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=6, now);
        // Two friends, then four players on their own, in the 3v3 queue.
        party(&mut mm, &[1, 2], now);
        mm.queue(1, Mode::Team3v3, now);
        for conn in 3..=5 {
            assert!(found(&mm.queue(conn, Mode::Team3v3, now)).is_empty());
        }
        assert_eq!(found(&mm.queue(6, Mode::Team3v3, now)), [1, 2, 3, 4, 5, 6]);
        let (_, seats) = starts(&all_ready(&mut mm, &[1, 2, 3, 4, 5, 6], now)).remove(0);
        let team = |conn| seats.iter().position(|seat| *seat == conn).unwrap() % 2;
        assert_eq!(team(1), team(2), "the party plays together");
        assert_eq!(seats.iter().filter(|c| team(**c) == team(1)).count(), 3);
        mm.check();
    }

    #[test]
    fn a_party_too_big_for_the_room_left_waits_without_blocking_others() {
        let now = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=8, now);
        // Queue order: parties (1,2), (3,4), (5,6), then solos 7 and 8.
        for pair in [[1, 2], [3, 4], [5, 6]] {
            party(&mut mm, &pair, now);
            mm.queue(pair[0], Mode::Team3v3, now);
        }
        assert!(found(&mm.queue(7, Mode::Team3v3, now)).is_empty());
        // (1,2) and (3,4) take a team each; (5,6) fits in neither, so the
        // solos complete the match and (5,6) waits at the front.
        assert_eq!(found(&mm.queue(8, Mode::Team3v3, now)), [1, 2, 3, 4, 7, 8]);
        assert_eq!(mm.waiting()[Mode::Team3v3.index()], 8);
        let out = all_ready(&mut mm, &[1, 2, 3, 4, 7, 8], now);
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
        assert_eq!(
            view(&out, 1),
            Some(PartyView {
                code: None,
                members: 1,
                leader: true,
                queue: None
            })
        );
        // The last member leaving dissolves the party and its code.
        mm.disconnect(2, now);
        mm.disconnect(3, now);
        assert_eq!(
            mm.join_party(4, &invite, now),
            refused(4, Refused::NoSuchParty)
        );
        // Creating a party while in someone else's starts a new one.
        let mine = party(&mut mm, &[4, 1], now);
        let out = mm.create_party(1, now);
        assert_ne!(code(&out, 1), mine);
        assert_eq!(view(&out, 4).unwrap().members, 1);
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
        assert!(out.contains(&Out::Stopped {
            conn: 1,
            reason: Stopped::MemberLeft
        }));
        assert_eq!(view(&out, 1).unwrap().queue, None);
        assert_eq!(mm.waiting()[Mode::Team2v2.index()], 0);
        mm.check();
    }

    #[test]
    fn no_shows_are_dropped_and_complete_parties_keep_their_place() {
        let start = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=5, start);
        // A party (1, 2) and two solos fill a 2v2; 5 waits behind.
        party(&mut mm, &[1, 2], start);
        mm.queue(1, Mode::Team2v2, start);
        mm.queue(3, Mode::Team2v2, start);
        assert_eq!(found(&mm.queue(4, Mode::Team2v2, start)).len(), 4);
        mm.queue(5, Mode::Team2v2, start);
        all_ready(&mut mm, &[1, 2, 3], start);
        assert!(mm.tick(start + READY / 2).is_empty());
        let out = mm.tick(start + READY);
        assert!(out.contains(&Out::Close {
            conn: 4,
            reason: Closed::NotReady
        }));
        for conn in [1, 2, 3] {
            assert!(out.contains(&Out::Requeued {
                conn,
                mode: Mode::Team2v2
            }));
        }
        // The requeued players and 5 form the next check at once.
        assert_eq!(found(&out), [1, 2, 3, 5]);
        mm.check();
    }

    #[test]
    fn a_party_member_who_does_not_confirm_stops_the_party() {
        let start = Instant::now();
        let mut mm = mm();
        connect(&mut mm, 1..=4, start);
        party(&mut mm, &[1, 2], start);
        mm.queue(1, Mode::Team2v2, start);
        mm.queue(3, Mode::Team2v2, start);
        mm.queue(4, Mode::Team2v2, start);
        all_ready(&mut mm, &[1, 3, 4], start);
        let out = mm.tick(start + READY);
        assert!(out.contains(&Out::Close {
            conn: 2,
            reason: Closed::NotReady
        }));
        assert!(out.contains(&Out::Stopped {
            conn: 1,
            reason: Stopped::MemberNotReady
        }));
        assert_eq!(view(&out, 1).unwrap().members, 1);
        assert!(out.contains(&Out::Requeued {
            conn: 3,
            mode: Mode::Team2v2
        }));
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
        assert!(!mm.contains(1) && mm.contains(2));
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
            now += Duration::from_millis(next(300));
            let conn = next(40) + 1;
            if !mm.contains(conn) {
                mm.connect(conn, Guest::from(conn), now);
            }
            let mode = Mode::ALL[next(5) as usize];
            let out = match next(12) {
                0 => mm.disconnect(conn, now),
                1 => mm.create_party(conn, now),
                2 if !codes.is_empty() => {
                    let code = codes[next(codes.len() as u64) as usize].clone();
                    mm.join_party(conn, &code, now)
                }
                3 => mm.leave_party(conn, now),
                4 => mm.cancel(conn),
                5..=7 => mm.queue(conn, mode, now),
                8..=10 => mm.ready(conn, now),
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
                // Started players are connected and no longer searching.
                for conn in &seats {
                    assert!(mm.contains(*conn));
                    assert!(mm.searching().all(|(searching, _)| searching != *conn));
                }
            }
            mm.check();
        }
        assert!(matches > 10, "{matches}");
    }
}
