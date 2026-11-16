//! Solo queues (ADR 0046): players queue alone for 1v1, solo 2v2 or solo
//! 3v3, and the matchmaker forms matches first come, first served.
//!
//! A clock-free state machine like [`crate::session::Session`]: the caller
//! passes in time and connection identifiers and carries out the returned
//! [`Out`] actions. It never issues tickets itself; [`Out::Start`] asks the
//! caller to issue one per seat.
//!
//! When enough players are queued, they are taken from the front and asked
//! to confirm they are still there (a ready check, answered automatically
//! by the game). If everyone confirms in time, the match starts. Otherwise
//! those who did not confirm are dropped and the others return to the front
//! of the queue in their original order.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use tokio::time::Instant;

/// A queue connection, numbered by the caller.
pub type QueueConn = u64;

/// A browser's guest identity: a random 128-bit value it keeps. One guest
/// holds at most one queue entry.
pub type Guest = u128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Duel,
    Solo2v2,
    Solo3v3,
}
impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Duel, Mode::Solo2v2, Mode::Solo3v3];
    pub fn id(self) -> &'static str {
        match self {
            Mode::Duel => "1v1",
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
            Mode::Solo2v2 => 2,
            Mode::Solo3v3 => 3,
        }
    }
    pub fn players(self) -> usize {
        2 * self.team_size()
    }
    fn index(self) -> usize {
        self as usize
    }
}

/// Why a queue connection is closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closed {
    /// The same guest queued again from another connection.
    Replaced,
    /// It did not confirm in time when a match was found.
    NotReady,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
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
    /// Everyone confirmed: issue a ticket per seat. `seats[i]` takes seat
    /// `i`; the connections are then out of the matchmaker.
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
struct Entry {
    conn: QueueConn,
    guest: Guest,
}

#[derive(Debug)]
struct Forming {
    mode: Mode,
    /// In queue order.
    entries: Vec<Entry>,
    ready: Vec<bool>,
    deadline: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Queued(Mode),
    Forming(u64),
}

/// Players waiting per mode, for status updates.
pub type Waiting = [usize; 3];

#[derive(Debug)]
pub struct Matchmaker {
    queues: [VecDeque<Entry>; 3],
    forming: HashMap<u64, Forming>,
    next_forming: u64,
    places: HashMap<QueueConn, Place>,
    guests: HashMap<Guest, QueueConn>,
    ready_timeout: Duration,
    /// Shuffles seats; returns a value in `0..bound`.
    random: fn(usize) -> usize,
}

impl Matchmaker {
    pub fn new(ready_timeout: Duration, random: fn(usize) -> usize) -> Self {
        Self {
            queues: Default::default(),
            forming: HashMap::new(),
            next_forming: 0,
            places: HashMap::new(),
            guests: HashMap::new(),
            ready_timeout,
            random,
        }
    }

    pub fn waiting(&self) -> Waiting {
        let mut waiting = [0; 3];
        for mode in Mode::ALL {
            waiting[mode.index()] = self.queues[mode.index()].len()
                + self
                    .forming
                    .values()
                    .filter(|forming| forming.mode == mode)
                    .map(|forming| forming.entries.len())
                    .sum::<usize>();
        }
        waiting
    }

    /// Connections waiting in a queue (not in a ready check), with their mode.
    pub fn queued(&self) -> impl Iterator<Item = (QueueConn, Mode)> + '_ {
        self.places.iter().filter_map(|(conn, place)| match place {
            Place::Queued(mode) => Some((*conn, *mode)),
            Place::Forming(_) => None,
        })
    }

    pub fn contains(&self, conn: QueueConn) -> bool {
        self.places.contains_key(&conn)
    }

    /// Queues `conn` for `mode`. A guest already queued elsewhere loses that
    /// entry (its connection is closed as replaced).
    pub fn join(&mut self, conn: QueueConn, guest: Guest, mode: Mode, now: Instant) -> Vec<Out> {
        let mut out = Vec::new();
        if self.places.contains_key(&conn) {
            out.extend(self.leave(conn, now));
        }
        if let Some(old) = self.guests.get(&guest).copied() {
            out.extend(self.leave(old, now));
            out.push(Out::Close {
                conn: old,
                reason: Closed::Replaced,
            });
        }
        self.queues[mode.index()].push_back(Entry { conn, guest });
        self.places.insert(conn, Place::Queued(mode));
        self.guests.insert(guest, conn);
        out.extend(self.form(mode, now));
        out
    }

    /// Takes `conn` out of the matchmaker (cancelled or disconnected). If it
    /// was in a ready check, that check fails for it at once.
    pub fn leave(&mut self, conn: QueueConn, now: Instant) -> Vec<Out> {
        match self.places.remove(&conn) {
            None => Vec::new(),
            Some(Place::Queued(mode)) => {
                let queue = &mut self.queues[mode.index()];
                if let Some(at) = queue.iter().position(|entry| entry.conn == conn) {
                    let entry = queue.remove(at).expect("position is in range");
                    self.guests.remove(&entry.guest);
                }
                Vec::new()
            }
            Some(Place::Forming(id)) => {
                let forming = self.forming.get_mut(&id).expect("forming exists");
                let at = forming
                    .entries
                    .iter()
                    .position(|entry| entry.conn == conn)
                    .expect("member of its ready check");
                let entry = forming.entries.remove(at);
                forming.ready.remove(at);
                self.guests.remove(&entry.guest);
                self.fail(id, now)
            }
        }
    }

    /// `conn` confirmed it is still there.
    pub fn ready(&mut self, conn: QueueConn, now: Instant) -> Vec<Out> {
        let Some(Place::Forming(id)) = self.places.get(&conn).copied() else {
            return Vec::new();
        };
        let forming = self.forming.get_mut(&id).expect("forming exists");
        if let Some(at) = forming.entries.iter().position(|entry| entry.conn == conn) {
            forming.ready[at] = true;
        }
        if !forming.ready.iter().all(|ready| *ready) {
            return Vec::new();
        }
        let forming = self.forming.remove(&id).expect("forming exists");
        for entry in &forming.entries {
            self.places.remove(&entry.conn);
            self.guests.remove(&entry.guest);
        }
        let mut seats: Vec<QueueConn> = forming.entries.iter().map(|entry| entry.conn).collect();
        // Fisher-Yates: teams mix whoever queued together.
        for at in (1..seats.len()).rev() {
            seats.swap(at, (self.random)(at + 1));
        }
        let mut out = vec![Out::Start {
            mode: forming.mode,
            seats,
        }];
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

    /// Ends a ready check that cannot complete: members who have not
    /// confirmed are dropped, the rest go back to the front of the queue.
    fn fail(&mut self, id: u64, now: Instant) -> Vec<Out> {
        let forming = self.forming.remove(&id).expect("forming exists");
        let mode = forming.mode;
        let mut out = Vec::new();
        let mut back = Vec::new();
        for (entry, ready) in forming.entries.into_iter().zip(forming.ready) {
            if ready {
                self.places.insert(entry.conn, Place::Queued(mode));
                out.push(Out::Requeued {
                    conn: entry.conn,
                    mode,
                });
                back.push(entry);
            } else {
                self.places.remove(&entry.conn);
                self.guests.remove(&entry.guest);
                out.push(Out::Close {
                    conn: entry.conn,
                    reason: Closed::NotReady,
                });
            }
        }
        let queue = &mut self.queues[mode.index()];
        for entry in back.into_iter().rev() {
            queue.push_front(entry);
        }
        out.extend(self.form(mode, now));
        out
    }

    /// Starts ready checks while the queue holds enough players.
    fn form(&mut self, mode: Mode, now: Instant) -> Vec<Out> {
        let mut out = Vec::new();
        while self.queues[mode.index()].len() >= mode.players() {
            let entries: Vec<Entry> = self.queues[mode.index()].drain(..mode.players()).collect();
            let id = self.next_forming;
            self.next_forming += 1;
            for entry in &entries {
                self.places.insert(entry.conn, Place::Forming(id));
                out.push(Out::Found {
                    conn: entry.conn,
                    mode,
                });
            }
            self.forming.insert(
                id,
                Forming {
                    mode,
                    ready: vec![false; entries.len()],
                    entries,
                    deadline: now + self.ready_timeout,
                },
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const READY: Duration = Duration::from_secs(10);
    /// Fisher-Yates swapping each seat with itself: seats in queue order.
    fn in_order(bound: usize) -> usize {
        bound - 1
    }
    /// Always swapping with the first seat: a real reordering.
    fn scrambled(_: usize) -> usize {
        0
    }
    fn matchmaker() -> Matchmaker {
        Matchmaker::new(READY, in_order)
    }
    fn found(out: &[Out]) -> Vec<QueueConn> {
        out.iter()
            .filter_map(|out| match out {
                Out::Found { conn, .. } => Some(*conn),
                _ => None,
            })
            .collect()
    }
    fn starts(out: &[Out]) -> Vec<(Mode, Vec<QueueConn>)> {
        out.iter()
            .filter_map(|out| match out {
                Out::Start { mode, seats } => Some((*mode, seats.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn modes_round_trip_their_ids() {
        for mode in Mode::ALL {
            assert_eq!(Mode::from_id(mode.id()), Some(mode));
        }
        assert_eq!(Mode::from_id("2v2"), None, "premade queues come later");
        assert_eq!(Mode::Solo3v3.players(), 6);
    }

    #[test]
    fn a_full_queue_forms_a_match_once_everyone_is_ready() {
        let now = Instant::now();
        let mut mm = matchmaker();
        for conn in 1..=3 {
            assert!(mm.join(conn, conn.into(), Mode::Solo2v2, now).is_empty());
        }
        assert_eq!(mm.waiting(), [0, 3, 0]);
        let out = mm.join(4, 4, Mode::Solo2v2, now);
        assert_eq!(found(&out), [1, 2, 3, 4]);
        assert_eq!(mm.queued().count(), 0, "all four are in the ready check");
        assert_eq!(mm.waiting(), [0, 4, 0]);
        for conn in [2, 4, 1] {
            assert!(mm.ready(conn, now).is_empty());
        }
        let out = mm.ready(3, now);
        assert_eq!(starts(&out), [(Mode::Solo2v2, vec![1, 2, 3, 4])]);
        assert_eq!(mm.waiting(), [0, 0, 0]);
        assert!((1..=4).all(|conn| !mm.contains(conn)));
        // Their guests may queue again.
        assert!(mm.join(9, 1, Mode::Duel, now).is_empty());
    }

    #[test]
    fn modes_queue_separately_and_a_queue_can_form_several_matches() {
        let now = Instant::now();
        let mut mm = matchmaker();
        mm.join(1, 1, Mode::Duel, now);
        mm.join(2, 2, Mode::Solo2v2, now);
        assert_eq!(mm.waiting(), [1, 1, 0]);
        let out = mm.join(3, 3, Mode::Duel, now);
        assert_eq!(found(&out), [1, 3]);
        mm.join(4, 4, Mode::Duel, now);
        let out = mm.join(5, 5, Mode::Duel, now);
        assert_eq!(found(&out), [4, 5]);
        assert_eq!(mm.waiting(), [4, 1, 0]);
    }

    #[test]
    fn seats_are_shuffled_across_teams() {
        let now = Instant::now();
        let mut mm = Matchmaker::new(READY, scrambled);
        for conn in 1..=4 {
            mm.join(conn, conn.into(), Mode::Solo2v2, now);
        }
        for conn in 1..=3 {
            mm.ready(conn, now);
        }
        let (_, seats) = starts(&mm.ready(4, now)).remove(0);
        assert_eq!(seats.len(), 4);
        assert_ne!(seats, [1, 2, 3, 4]);
        let mut sorted = seats.clone();
        sorted.sort();
        assert_eq!(sorted, [1, 2, 3, 4], "every player gets one seat");
    }

    #[test]
    fn players_who_do_not_confirm_are_dropped_and_the_rest_keep_their_place() {
        let start = Instant::now();
        let mut mm = matchmaker();
        for conn in 1..=2 {
            mm.join(conn, conn.into(), Mode::Duel, start);
        }
        mm.ready(2, start);
        // A later arrival waits behind them.
        mm.join(3, 3, Mode::Duel, start);
        assert!(mm.tick(start + READY / 2).is_empty());
        let out = mm.tick(start + READY);
        assert!(out.contains(&Out::Close {
            conn: 1,
            reason: Closed::NotReady
        }));
        assert!(out.contains(&Out::Requeued {
            conn: 2,
            mode: Mode::Duel
        }));
        // 2 is back at the front, so it and 3 form the next check at once.
        assert_eq!(found(&out), [2, 3]);
        assert!(!mm.contains(1));
        assert!(
            mm.join(7, 1, Mode::Duel, start).is_empty(),
            "1's guest may requeue"
        );
    }

    #[test]
    fn leaving_during_a_ready_check_fails_it_at_once() {
        let now = Instant::now();
        let mut mm = matchmaker();
        for conn in 1..=4 {
            mm.join(conn, conn.into(), Mode::Solo2v2, now);
        }
        mm.ready(1, now);
        mm.ready(2, now);
        let out = mm.leave(3, now);
        assert_eq!(
            out,
            [
                Out::Requeued {
                    conn: 1,
                    mode: Mode::Solo2v2
                },
                Out::Requeued {
                    conn: 2,
                    mode: Mode::Solo2v2
                },
                Out::Close {
                    conn: 4,
                    reason: Closed::NotReady
                },
            ]
        );
        assert_eq!(mm.waiting(), [0, 2, 0]);
        assert!(
            mm.ready(1, now).is_empty(),
            "a stale confirmation does nothing"
        );
        // Leaving the queue, and leaving twice, are harmless.
        assert!(mm.leave(1, now).is_empty());
        assert!(mm.leave(1, now).is_empty());
        assert_eq!(mm.waiting(), [0, 1, 0]);
    }

    #[test]
    fn a_guest_holds_one_entry_and_the_newest_connection_keeps_it() {
        let now = Instant::now();
        let mut mm = matchmaker();
        mm.join(1, 42, Mode::Solo3v3, now);
        let out = mm.join(2, 42, Mode::Duel, now);
        assert_eq!(
            out,
            [Out::Close {
                conn: 1,
                reason: Closed::Replaced
            }]
        );
        assert_eq!(mm.waiting(), [1, 0, 0]);
        // A guest cannot be matched against itself.
        mm.join(3, 42, Mode::Duel, now);
        assert_eq!(mm.waiting(), [1, 0, 0]);
        // One connection switching modes moves its entry.
        mm.join(3, 42, Mode::Solo2v2, now);
        assert_eq!(mm.waiting(), [0, 1, 0]);
    }

    #[test]
    fn no_player_is_ever_in_two_matches() {
        // Many joins, leaves, confirmations and timeouts in a fixed
        // pseudo-random order: every connection starts at most one match.
        let start = Instant::now();
        let mut mm = matchmaker();
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = |bound: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % bound
        };
        let mut started = std::collections::HashSet::new();
        let mut now = start;
        for step in 0..20_000u64 {
            now += Duration::from_millis(next(400));
            let conn = next(60);
            let out = match next(10) {
                0..=3 => mm.join(
                    step + 1000,
                    u128::from(conn),
                    Mode::ALL[next(3) as usize],
                    now,
                ),
                4 => mm.leave(conn + 1000, now),
                5..=8 => {
                    let any: Vec<_> = mm.places.keys().copied().collect();
                    match any.get(next(any.len().max(1) as u64) as usize) {
                        Some(conn) => mm.ready(*conn, now),
                        None => Vec::new(),
                    }
                }
                _ => mm.tick(now),
            };
            for (mode, seats) in starts(&out) {
                assert_eq!(seats.len(), mode.players());
                for conn in seats {
                    assert!(started.insert(conn), "{conn} started twice");
                    assert!(!mm.contains(conn));
                }
            }
            // Queue bookkeeping stays consistent.
            let queued: usize = mm.queues.iter().map(VecDeque::len).sum();
            let forming: usize = mm.forming.values().map(|f| f.entries.len()).sum();
            assert_eq!(queued + forming, mm.places.len());
            assert_eq!(mm.guests.len(), mm.places.len());
        }
        assert!(!started.is_empty());
    }
}
