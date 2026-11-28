//! Hub world placement in the front door ([ADR 0057](../../../../docs/decisions/0057-hub-on-the-world-server.md)):
//! which world each queue connection was sent to, the instance directory
//! the world server answers once a second, and the block pairs the door
//! sends back. Pure state, driven by the queue task: no sockets or clocks
//! (it is handed `Instant`s).
//!
//! The door decides placement (ADR 0056's rules: beside the followed
//! friend, else a party-mate, else the world this connection was last in,
//! else the fullest with room, else a new world) and issues a `Role::Hub`
//! ticket; the world server runs the world. A placement the directory has
//! not confirmed within [`CONFIRM_WITHIN`] (the page never connected) or
//! stops reporting (it left) is dropped.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use arena_sim::arena::HUB_PLAYER_CAP;

use crate::matchmaker::QueueConn;
use crate::worlds::DirectoryReply;

/// How long a placement may wait for the directory to show the player in
/// their world before the door forgets it.
pub const CONFIRM_WITHIN: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Placed {
    instance: u64,
    /// The ticket's player id (the world server's name for this person).
    player: u64,
    issued: Instant,
    /// The directory has shown them in their world.
    confirmed: bool,
}

/// Where to put a joining player.
#[derive(Clone, Copy, Debug, Default)]
pub struct Placement<'a> {
    /// Connections of the friend (or party-mate) to join beside, when asked.
    pub follow: &'a [QueueConn],
    /// Whether a follow was asked for.
    pub following: bool,
    /// The player's party-mates: join one already in a world when possible.
    pub party: &'a [QueueConn],
}

#[derive(Default)]
pub struct Worlds {
    placed: HashMap<QueueConn, Placed>,
    /// Each connection's ticket player id, kept while it is open, so its
    /// page reconnecting to its world takes its own place back.
    players: HashMap<QueueConn, u64>,
    by_player: HashMap<u64, QueueConn>,
    /// The world each connection was last in, to return to after a match.
    last: HashMap<QueueConn, u64>,
    /// The directory's latest answer: players reported per instance.
    reported: BTreeMap<u64, Vec<u64>>,
}

impl Worlds {
    /// `conn`'s ticket player id, made with `fresh` on first use.
    pub fn player_for(&mut self, conn: QueueConn, fresh: impl FnOnce() -> u64) -> u64 {
        *self.players.entry(conn).or_insert_with(fresh)
    }

    /// Whether anyone is placed (the directory is asked only then).
    pub fn is_empty(&self) -> bool {
        self.placed.is_empty()
    }

    /// The world `conn` was sent to, if any.
    pub fn instance_of(&self, conn: QueueConn) -> Option<u64> {
        self.placed.get(&conn).map(|placed| placed.instance)
    }

    /// Everyone in `conn`'s world (including `conn`), for the `hub` chat
    /// channel; empty when `conn` is in none. Only placements the world has
    /// confirmed listen: a ticket never used hears nothing.
    pub fn members(&self, conn: QueueConn) -> Vec<QueueConn> {
        let Some(instance) = self.instance_of(conn) else {
            return Vec::new();
        };
        let mut members: Vec<QueueConn> = self
            .placed
            .iter()
            .filter(|(member, placed)| {
                placed.instance == instance && (placed.confirmed || **member == conn)
            })
            .map(|(conn, _)| *conn)
            .collect();
        members.sort_unstable();
        members
    }

    /// (players placed, worlds with someone placed), for metrics.
    pub fn counts(&self) -> (usize, usize) {
        let mut instances: Vec<u64> = self.placed.values().map(|p| p.instance).collect();
        instances.sort_unstable();
        instances.dedup();
        (self.placed.len(), instances.len())
    }

    fn count(&self, instance: u64) -> usize {
        let placed = self
            .placed
            .values()
            .filter(|placed| placed.instance == instance)
            .count();
        // Someone the world still holds (within their grace, or placed
        // elsewhere since) counts too.
        let reported = self.reported.get(&instance).map_or(0, |players| {
            players
                .iter()
                .filter(|player| {
                    self.by_player
                        .get(player)
                        .and_then(|conn| self.placed.get(conn))
                        .is_none_or(|placed| placed.instance != instance)
                })
                .count()
        });
        placed + reported
    }

    fn room(&self, instance: u64) -> bool {
        self.count(instance) < HUB_PLAYER_CAP
    }

    /// Chooses a world for `conn` and records the placement under ticket
    /// player id `player`. Refused (`in_hub`) when already placed;
    /// following a friend who is in no world (or hidden from this player)
    /// is `not_in_hub`, and into a full world `hub_full`. `fresh` names a
    /// new world when none has room.
    pub fn place(
        &mut self,
        conn: QueueConn,
        player: u64,
        placement: Placement<'_>,
        hidden: impl Fn(QueueConn, QueueConn) -> bool,
        fresh: impl FnOnce() -> u64,
        now: Instant,
    ) -> Result<u64, &'static str> {
        if self.placed.contains_key(&conn) {
            return Err("in_hub");
        }
        if self
            .by_player
            .get(&player)
            .is_some_and(|holder| *holder != conn)
        {
            return Err("bad_request");
        }
        let instance = if placement.following {
            let instance = placement
                .follow
                .iter()
                .filter(|friend| !hidden(conn, **friend))
                .find_map(|friend| self.instance_of(*friend))
                .ok_or("not_in_hub")?;
            if !self.room(instance) {
                return Err("hub_full");
            }
            instance
        } else {
            let fullest = || {
                let mut known: Vec<u64> = self
                    .placed
                    .values()
                    .map(|placed| placed.instance)
                    .chain(self.reported.keys().copied())
                    .collect();
                known.sort_unstable();
                known.dedup();
                known
                    .into_iter()
                    .filter(|instance| self.room(*instance))
                    // Ties go to the lowest id, so choices are stable.
                    .max_by_key(|instance| (self.count(*instance), std::cmp::Reverse(*instance)))
            };
            placement
                .party
                .iter()
                .filter_map(|mate| self.instance_of(*mate))
                .chain(self.last.get(&conn).copied())
                .find(|instance| self.room(*instance))
                .or_else(fullest)
                .unwrap_or_else(fresh)
        };
        self.placed.insert(
            conn,
            Placed {
                instance,
                player,
                issued: now,
                confirmed: false,
            },
        );
        self.by_player.insert(player, conn);
        self.last.insert(conn, instance);
        Ok(instance)
    }

    /// `conn` left its world (asked to, took a match ticket, or went away).
    /// Returns whether it was in one.
    pub fn leave(&mut self, conn: QueueConn) -> bool {
        let Some(placed) = self.placed.remove(&conn) else {
            return false;
        };
        self.by_player.remove(&placed.player);
        true
    }

    /// Every placed player id, for the directory request.
    pub fn placed_players(&self) -> Vec<u64> {
        let mut players: Vec<u64> = self.placed.values().map(|placed| placed.player).collect();
        players.sort_unstable();
        players
    }

    /// Forgets the last directory answer (nobody is placed any more).
    pub fn forget_reports(&mut self) {
        self.reported.clear();
    }

    /// Without directory answers: placements never confirmed within
    /// [`CONFIRM_WITHIN`] are dropped (as an answer would). Their
    /// connections.
    pub fn expire(&mut self, now: Instant) -> Vec<QueueConn> {
        let mut dropped: Vec<QueueConn> = self
            .placed
            .iter()
            .filter(|(_, placed)| {
                !placed.confirmed && now.duration_since(placed.issued) >= CONFIRM_WITHIN
            })
            .map(|(conn, _)| *conn)
            .collect();
        dropped.sort_unstable();
        for conn in &dropped {
            self.leave(*conn);
        }
        dropped
    }

    /// The connection is gone for good: forget it entirely.
    pub fn gone(&mut self, conn: QueueConn) {
        self.leave(conn);
        self.last.remove(&conn);
        self.players.remove(&conn);
    }

    /// Every blocked pair among players placed in the same world, by ticket
    /// player id, for the directory request.
    pub fn block_pairs(&self, hidden: impl Fn(QueueConn, QueueConn) -> bool) -> Vec<(u64, u64)> {
        let mut by_instance: BTreeMap<u64, Vec<(QueueConn, u64)>> = BTreeMap::new();
        for (conn, placed) in &self.placed {
            by_instance
                .entry(placed.instance)
                .or_default()
                .push((*conn, placed.player));
        }
        let mut pairs = Vec::new();
        for members in by_instance.values() {
            for (i, (a, pa)) in members.iter().enumerate() {
                for (b, pb) in &members[i + 1..] {
                    if hidden(*a, *b) {
                        pairs.push(((*pa).min(*pb), (*pa).max(*pb)));
                    }
                }
            }
        }
        pairs.sort_unstable();
        pairs
    }

    /// Applies the directory's answer: placements it shows are confirmed;
    /// confirmed ones it no longer shows (the player left the world) and
    /// unconfirmed ones older than [`CONFIRM_WITHIN`] are dropped. Returns
    /// the connections dropped.
    pub fn reconcile(&mut self, reply: &DirectoryReply, now: Instant) -> Vec<QueueConn> {
        self.reported = reply
            .worlds
            .iter()
            .filter_map(|world| {
                u64::from_str_radix(&world.instance, 16)
                    .ok()
                    .map(|id| (id, world.players.clone()))
            })
            .collect();
        let mut dropped = Vec::new();
        for (conn, placed) in &mut self.placed {
            let shown = self
                .reported
                .get(&placed.instance)
                .is_some_and(|players| players.contains(&placed.player));
            if shown {
                placed.confirmed = true;
            } else if placed.confirmed || now.duration_since(placed.issued) >= CONFIRM_WITHIN {
                dropped.push(*conn);
            }
        }
        for conn in &dropped {
            self.leave(*conn);
        }
        dropped.sort_unstable();
        dropped
    }
}

#[cfg(test)]
mod tests;
