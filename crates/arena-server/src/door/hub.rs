//! Hub instances ([ADR 0056](../../../../docs/decisions/0056-social-hub.md)):
//! who is in which instance, where they stand and whom each one sees. Pure
//! state, driven by the queue task: it never touches sockets or clocks, and
//! hands each message to a `deliver` callback that says whether the
//! connection's outbox took it. The movement rules are `arena_sim::hub`,
//! which the browser predicts with.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use arena_protocol::{Handle, WireAppearance};
use arena_sim::arena::Position;
use arena_sim::hub::{self, Intent, MAX_HUB_PLAYERS};
use serde::{Deserialize, Serialize};

use crate::matchmaker::QueueConn;

/// Moves that may wait for their tick: about a second's worth, room for a
/// clump of network jitter.
pub const MAX_MOVES: usize = 8;

/// Why a player is no longer in the hub, as `hub_left` says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Left {
    /// They asked to leave.
    Left,
    /// A match (or spectator) ticket took them into a match.
    Match,
}
impl Left {
    pub fn id(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Match => "match",
        }
    }
}

/// Who a hub player is, as everyone else sees them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Who {
    pub handle: Handle,
    pub admin: bool,
}

/// Where to put a joining player.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Placement<'a> {
    /// Connections of the friend (or party-mate) to join beside, when asked;
    /// empty when not asked.
    pub follow: &'a [QueueConn],
    /// Whether a follow was asked for (and found no one in the hub).
    pub following: bool,
    /// The player's party-mates: join one already in the hub when possible.
    pub party: &'a [QueueConn],
}

/// A player coming into view, or one in view whose name or look changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entered {
    pub id: u32,
    pub handle: String,
    pub admin: bool,
    pub appearance: WireAppearance,
}

/// `hub {instance, you, x, z}`: joined.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Joined {
    pub instance: u32,
    pub you: u32,
    pub x: i32,
    pub z: i32,
}

/// `hub_state`: one per player per hub tick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub tick: u64,
    /// The sequence number of the last move applied.
    pub ack: u32,
    pub x: i32,
    pub z: i32,
    /// `[id, x, z, facing]` for each player in view.
    pub players: Vec<(u32, i32, i32, u16)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enter: Vec<Entered>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exit: Vec<u32>,
}

/// Who a client knows is in view: their hub id and version, by connection.
type View = HashMap<QueueConn, (u32, u32)>;

struct Player {
    id: u32,
    instance: u32,
    who: Who,
    appearance: Arc<WireAppearance>,
    at: Position,
    /// The last move applied (its facing is shown while standing).
    intent: Intent,
    ack: u32,
    /// Moves waiting, one applied per hub tick in order.
    moves: VecDeque<(u32, Intent)>,
    /// Those this player's client knows are in view, by connection, with
    /// the hub id and version it knows them by (kept after they leave, to
    /// send their `exit`).
    sees: View,
    /// Goes up with each rename, so every viewer gets the new name once its
    /// state is delivered.
    version: u32,
}

/// Every hub instance and its players.
#[derive(Default)]
pub struct Hubs {
    players: HashMap<QueueConn, Player>,
    /// Members of each open instance, in arrival order.
    instances: BTreeMap<u32, Vec<QueueConn>>,
    /// The instance each connection was last in, to return to after a match.
    last: HashMap<QueueConn, u32>,
    tick: u64,
    next_id: u32,
}

impl Hubs {
    #[cfg(test)]
    pub fn contains(&self, conn: QueueConn) -> bool {
        self.players.contains_key(&conn)
    }

    pub fn instance_of(&self, conn: QueueConn) -> Option<u32> {
        self.players.get(&conn).map(|player| player.instance)
    }

    /// Everyone in `conn`'s instance (including `conn`), for the `hub`
    /// chat channel; empty outside the hub.
    pub fn instance_members(&self, conn: QueueConn) -> Vec<QueueConn> {
        self.instance_of(conn)
            .and_then(|instance| self.instances.get(&instance))
            .cloned()
            .unwrap_or_default()
    }

    /// Players in the hub and open instances (for metrics).
    pub fn counts(&self) -> (usize, usize) {
        (self.players.len(), self.instances.len())
    }

    fn room(&self, instance: u32) -> bool {
        self.instances
            .get(&instance)
            .is_some_and(|members| members.len() < MAX_HUB_PLAYERS)
    }

    /// Puts `conn` into an instance: beside the followed friend when asked
    /// (refused when they are not in the hub, hidden from this player, or
    /// their instance is full), else a party-mate's, else the last one this
    /// connection was in, else the fullest with room, else a new one.
    pub fn join(
        &mut self,
        conn: QueueConn,
        who: Who,
        appearance: WireAppearance,
        placement: Placement<'_>,
        hidden: impl Fn(QueueConn, QueueConn) -> bool,
    ) -> Result<Joined, &'static str> {
        if self.players.contains_key(&conn) {
            return Err("in_hub");
        }
        if !appearance.is_valid() {
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
            placement
                .party
                .iter()
                .filter_map(|mate| self.instance_of(*mate))
                .chain(self.last.get(&conn).copied())
                .find(|instance| self.room(*instance))
                .or_else(|| {
                    // The fullest with room keeps instances lively; ties go
                    // to the lowest number.
                    self.instances
                        .iter()
                        .filter(|(_, members)| members.len() < MAX_HUB_PLAYERS)
                        .max_by_key(|(number, members)| {
                            (members.len(), std::cmp::Reverse(**number))
                        })
                        .map(|(number, _)| *number)
                })
                .unwrap_or_else(|| {
                    (1..=u32::MAX)
                        .find(|n| !self.instances.contains_key(n))
                        .unwrap_or(u32::MAX)
                })
        };
        let members = self.instances.entry(instance).or_default();
        // The first free spawn place (players leave from anywhere).
        let taken: Vec<Position> = members
            .iter()
            .filter_map(|member| self.players.get(member).map(|player| player.at))
            .collect();
        let at = (0..MAX_HUB_PLAYERS as u32)
            .map(hub::spawn_point)
            .find(|place| !taken.contains(place))
            .unwrap_or_else(|| hub::spawn_point(0));
        members.push(conn);
        // Ids never repeat while in use, even after wrapping around.
        loop {
            self.next_id = self.next_id.checked_add(1).unwrap_or(1);
            if !self
                .players
                .values()
                .any(|player| player.id == self.next_id)
            {
                break;
            }
        }
        let id = self.next_id;
        self.players.insert(
            conn,
            Player {
                id,
                instance,
                who,
                appearance: Arc::new(appearance),
                at,
                intent: Intent::default(),
                ack: 0,
                moves: VecDeque::new(),
                sees: HashMap::new(),
                version: 0,
            },
        );
        self.last.insert(conn, instance);
        Ok(Joined {
            instance,
            you: id,
            x: at.x,
            z: at.z,
        })
    }

    /// Takes `conn` out of the hub; those who saw it get its `exit` with the
    /// next tick. `false` when it was not in the hub.
    pub fn leave(&mut self, conn: QueueConn) -> bool {
        let Some(player) = self.players.remove(&conn) else {
            return false;
        };
        if let Some(members) = self.instances.get_mut(&player.instance) {
            members.retain(|member| *member != conn);
            if members.is_empty() {
                self.instances.remove(&player.instance);
            }
        }
        true
    }

    /// Forgets a closed connection entirely.
    pub fn gone(&mut self, conn: QueueConn) {
        self.last.remove(&conn);
        // Its `exit` still goes out to those who saw it with the next tick.
        self.leave(conn);
    }

    /// Queues one move of `conn`'s: each is one hub tick of walking,
    /// applied once, in order, so the client's prediction replays exactly
    /// what the door did. An old or repeated sequence number is ignored;
    /// past [`MAX_MOVES`] waiting, the oldest is dropped.
    pub fn intent(
        &mut self,
        conn: QueueConn,
        seq: u32,
        intent: Intent,
    ) -> Result<(), &'static str> {
        if !intent.is_valid() {
            return Err("bad_request");
        }
        let player = self.players.get_mut(&conn).ok_or("not_in_hub")?;
        let newest = player.moves.back().map_or(player.ack, |(last, _)| *last);
        if seq <= newest {
            return Ok(());
        }
        player.moves.push_back((seq, intent));
        if player.moves.len() > MAX_MOVES {
            player.moves.pop_front();
        }
        Ok(())
    }

    /// A new name or admin role: those who see the player get it again.
    pub fn rename(&mut self, conn: QueueConn, who: Who) {
        if let Some(player) = self.players.get_mut(&conn)
            && player.who != who
        {
            player.who = who;
            player.version = player.version.wrapping_add(1);
        }
    }

    /// One hub tick: everyone moves, then each player gets one `hub_state`
    /// with who is in view and what changed. A change is recorded only when
    /// `deliver` says the connection took the message, so a dropped state
    /// is re-sent, changes included, next tick. `hidden(a, b)` says whether
    /// either blocked the other.
    pub fn tick(
        &mut self,
        hidden: impl Fn(QueueConn, QueueConn) -> bool,
        mut deliver: impl FnMut(QueueConn, State) -> bool,
    ) {
        self.tick += 1;
        // One waiting move per player per tick; without one, they stand.
        for player in self.players.values_mut() {
            if let Some((seq, intent)) = player.moves.pop_front() {
                player.at = hub::step(player.at, intent);
                player.intent = intent;
                player.ack = seq;
            }
        }
        let mut accepted: Vec<(QueueConn, View)> = Vec::new();
        for members in self.instances.values() {
            for &conn in members {
                let me = &self.players[&conn];
                let visible: View = members
                    .iter()
                    .copied()
                    .filter(|other| {
                        *other != conn
                            && hub::in_interest(me.at, self.players[other].at)
                            && !hidden(conn, *other)
                    })
                    .map(|other| {
                        let them = &self.players[&other];
                        (other, (them.id, them.version))
                    })
                    .collect();
                let mut state = State {
                    tick: self.tick,
                    ack: me.ack,
                    x: me.at.x,
                    z: me.at.z,
                    players: Vec::with_capacity(visible.len()),
                    enter: Vec::new(),
                    // Gone from view, or back under a new id (left and
                    // rejoined between two delivered states).
                    exit: me
                        .sees
                        .iter()
                        .filter(|(seen, (id, _))| {
                            visible.get(seen).map(|known| known.0) != Some(*id)
                        })
                        .map(|(_, (id, _))| *id)
                        .collect(),
                };
                // Arrival order, so messages are stable for the same state.
                for other in members.iter().filter(|other| visible.contains_key(other)) {
                    let them = &self.players[other];
                    if me.sees.get(other) != Some(&(them.id, them.version)) {
                        state.enter.push(Entered {
                            id: them.id,
                            handle: them.who.handle.to_string(),
                            admin: them.who.admin,
                            appearance: (*them.appearance).clone(),
                        });
                    }
                    state
                        .players
                        .push((them.id, them.at.x, them.at.z, them.intent.facing));
                }
                if deliver(conn, state) {
                    accepted.push((conn, visible));
                }
            }
        }
        for (conn, visible) in accepted {
            if let Some(player) = self.players.get_mut(&conn) {
                player.sees = visible;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_sim::arena::{AXIS_LIMIT, Axes};

    fn look() -> WireAppearance {
        WireAppearance {
            ancestry: "human".into(),
            body: "female".into(),
            skin_tone: "s1".into(),
            hair_color: "h1".into(),
            facial_hair_color: "h1".into(),
            eyes: "e1".into(),
            build: 0,
            groups: vec![("hair".into(), "bob".into())],
            variants: vec![],
        }
    }

    fn who(n: u16) -> Who {
        Who {
            handle: Handle::new("Rook", n).unwrap(),
            admin: false,
        }
    }

    const NOBODY: fn(QueueConn, QueueConn) -> bool = |_, _| false;

    /// Runs a tick and returns each connection's state; `refuse` lists
    /// connections whose outbox is full.
    fn tick(hubs: &mut Hubs, refuse: &[QueueConn]) -> HashMap<QueueConn, State> {
        tick_hidden(hubs, refuse, NOBODY)
    }

    fn tick_hidden(
        hubs: &mut Hubs,
        refuse: &[QueueConn],
        hidden: impl Fn(QueueConn, QueueConn) -> bool,
    ) -> HashMap<QueueConn, State> {
        let mut out = HashMap::new();
        hubs.tick(hidden, |conn, state| {
            out.insert(conn, state);
            !refuse.contains(&conn)
        });
        out
    }

    fn join(hubs: &mut Hubs, conn: QueueConn) -> Joined {
        hubs.join(conn, who(conn as u16), look(), Placement::default(), NOBODY)
            .unwrap()
    }

    #[test]
    fn players_see_each_other_arrive_move_and_leave() {
        let mut hubs = Hubs::default();
        let a = join(&mut hubs, 1);
        let b = join(&mut hubs, 2);
        assert_eq!((a.instance, b.instance), (1, 1));
        assert_ne!((a.x, a.z), (b.x, b.z), "each arrival has a place");
        let states = tick(&mut hubs, &[]);
        let seen_by_a = &states[&1];
        assert_eq!(seen_by_a.enter.len(), 1);
        assert_eq!(seen_by_a.enter[0].handle, "Rook#0002");
        assert_eq!(seen_by_a.players, vec![(b.you, b.x, b.z, 0)]);
        // The look and name come once; then only positions.
        hubs.intent(
            2,
            7,
            Intent {
                movement: Axes {
                    x: AXIS_LIMIT,
                    z: 0,
                },
                facing: 900,
            },
        )
        .unwrap();
        let states = tick(&mut hubs, &[]);
        assert!(states[&1].enter.is_empty());
        assert_eq!(states[&1].players, vec![(b.you, b.x + 720, b.z, 900)]);
        assert_eq!((states[&2].ack, states[&2].x), (7, b.x + 720));
        // Leaving: the other sees it go on the next tick.
        assert!(hubs.leave(2));
        let states = tick(&mut hubs, &[]);
        assert_eq!(states[&1].exit, vec![b.you]);
        assert!(states[&1].players.is_empty());
        assert!(!states.contains_key(&2));
        assert!(tick(&mut hubs, &[])[&1].exit.is_empty());
    }

    #[test]
    fn a_dropped_state_is_sent_again_with_its_changes() {
        let mut hubs = Hubs::default();
        join(&mut hubs, 1);
        join(&mut hubs, 2);
        // A's outbox is full: its state (with B's arrival) is lost.
        assert_eq!(tick(&mut hubs, &[1])[&1].enter.len(), 1);
        assert_eq!(tick(&mut hubs, &[])[&1].enter.len(), 1);
        assert!(tick(&mut hubs, &[])[&1].enter.is_empty());
        hubs.leave(2);
        assert_eq!(tick(&mut hubs, &[1])[&1].exit.len(), 1);
        assert_eq!(tick(&mut hubs, &[])[&1].exit.len(), 1);
    }

    #[test]
    fn instances_fill_up_to_forty_then_open_another() {
        let mut hubs = Hubs::default();
        for conn in 1..=MAX_HUB_PLAYERS as QueueConn {
            assert_eq!(join(&mut hubs, conn).instance, 1);
        }
        assert_eq!(join(&mut hubs, 100).instance, 2);
        assert_eq!(hubs.counts(), (MAX_HUB_PLAYERS + 1, 2));
        // A newcomer goes to the fullest with room: instance 2 (1 player)
        // is the only one with room.
        assert_eq!(join(&mut hubs, 101).instance, 2);
        // A full instance never sends more than 39 others to anyone.
        let states = tick(&mut hubs, &[]);
        assert!(
            states
                .values()
                .all(|state| state.players.len() < MAX_HUB_PLAYERS)
        );
        assert_eq!(states[&1].players.len(), MAX_HUB_PLAYERS - 1);
        // Joining a friend in the full instance is refused.
        assert_eq!(
            hubs.join(
                102,
                who(102),
                look(),
                Placement {
                    follow: &[5],
                    following: true,
                    party: &[]
                },
                NOBODY
            ),
            Err("hub_full")
        );
        // Empty instances close; numbers are reused.
        hubs.leave(100);
        hubs.leave(101);
        assert_eq!(hubs.counts().1, 1);
        hubs.leave(7);
        assert_eq!(join(&mut hubs, 103).instance, 1, "the fullest with room");
    }

    #[test]
    fn friends_party_mates_and_the_last_instance_come_first() {
        let mut hubs = Hubs::default();
        for conn in 1..=MAX_HUB_PLAYERS as QueueConn {
            join(&mut hubs, conn);
        }
        join(&mut hubs, 50); // instance 2
        hubs.leave(3); // room in instance 1 again
        // A friend in instance 2 wins over the fuller instance 1.
        let follow = Placement {
            follow: &[50],
            following: true,
            party: &[],
        };
        assert_eq!(
            hubs.join(51, who(51), look(), follow, NOBODY)
                .unwrap()
                .instance,
            2
        );
        // A party-mate's instance.
        let party = Placement {
            follow: &[],
            following: false,
            party: &[50],
        };
        assert_eq!(
            hubs.join(52, who(52), look(), party, NOBODY)
                .unwrap()
                .instance,
            2
        );
        // Back from a match: the last instance.
        hubs.leave(52);
        assert_eq!(join(&mut hubs, 52).instance, 2);
        // A friend who is not in the hub, or hidden, cannot be followed.
        let nobody_there = Placement {
            follow: &[99],
            following: true,
            party: &[],
        };
        assert_eq!(
            hubs.join(53, who(53), look(), nobody_there, NOBODY),
            Err("not_in_hub")
        );
        assert_eq!(
            hubs.join(53, who(53), look(), follow, |_, _| true),
            Err("not_in_hub")
        );
        // Twice in the hub, or with a bad look, is refused.
        assert_eq!(join_err(&mut hubs, 52, look()), "in_hub");
        let mut bad = look();
        bad.body = "../etc".into();
        assert_eq!(join_err(&mut hubs, 60, bad), "bad_request");
        // A gone connection forgets its last instance.
        hubs.gone(52);
        assert!(!hubs.contains(52));
    }

    fn join_err(hubs: &mut Hubs, conn: QueueConn, appearance: WireAppearance) -> &'static str {
        hubs.join(conn, who(1), appearance, Placement::default(), NOBODY)
            .unwrap_err()
    }

    #[test]
    fn blocks_hide_players_both_ways_and_unblocking_shows_them() {
        let mut hubs = Hubs::default();
        join(&mut hubs, 1);
        let b = join(&mut hubs, 2);
        join(&mut hubs, 3);
        tick(&mut hubs, &[]);
        let blocked = |a: QueueConn, b: QueueConn| (a, b) == (1, 2) || (a, b) == (2, 1);
        let states = tick_hidden(&mut hubs, &[], blocked);
        assert_eq!(states[&1].exit, vec![b.you]);
        assert_eq!(states[&2].exit.len(), 1);
        assert_eq!(states[&1].players.len(), 1, "still sees the third");
        assert_eq!(states[&3].players.len(), 2, "the third sees both");
        let states = tick(&mut hubs, &[]);
        assert_eq!(states[&1].enter.len(), 1);
    }

    #[test]
    fn renames_reach_those_who_see_the_player() {
        let mut hubs = Hubs::default();
        join(&mut hubs, 1);
        join(&mut hubs, 2);
        tick(&mut hubs, &[]);
        hubs.rename(
            2,
            Who {
                handle: Handle::new("Ash", 1000).unwrap(),
                admin: true,
            },
        );
        let states = tick(&mut hubs, &[]);
        assert_eq!(states[&1].enter.len(), 1);
        assert_eq!(
            (
                states[&1].enter[0].handle.as_str(),
                states[&1].enter[0].admin
            ),
            ("Ash#1000", true)
        );
        assert!(states[&2].enter.is_empty());
        assert!(tick(&mut hubs, &[])[&1].enter.is_empty());
    }

    #[test]
    fn a_rejoin_between_two_states_is_an_exit_and_an_enter() {
        let mut hubs = Hubs::default();
        join(&mut hubs, 1);
        let first = join(&mut hubs, 2);
        tick(&mut hubs, &[]);
        hubs.leave(2);
        let again = join(&mut hubs, 2);
        assert_ne!(first.you, again.you);
        let states = tick(&mut hubs, &[]);
        assert_eq!(states[&1].exit, vec![first.you]);
        assert_eq!(states[&1].enter.len(), 1);
        assert_eq!(states[&1].enter[0].id, again.you);
    }

    #[test]
    fn a_rename_reaches_a_viewer_whose_state_was_dropped() {
        let mut hubs = Hubs::default();
        join(&mut hubs, 1);
        join(&mut hubs, 2);
        tick(&mut hubs, &[]);
        let ash = Who {
            handle: Handle::new("Ash", 1000).unwrap(),
            admin: false,
        };
        hubs.rename(2, ash);
        assert_eq!(tick(&mut hubs, &[1])[&1].enter.len(), 1);
        let states = tick(&mut hubs, &[]);
        assert_eq!(states[&1].enter[0].handle, "Ash#1000");
        assert!(tick(&mut hubs, &[])[&1].enter.is_empty());
    }

    #[test]
    fn moves_apply_once_each_in_order_then_the_player_stands() {
        let mut hubs = Hubs::default();
        let a = join(&mut hubs, 1);
        let east = Intent {
            movement: Axes {
                x: AXIS_LIMIT,
                z: 0,
            },
            facing: 16384,
        };
        for seq in 1..=3 {
            hubs.intent(1, seq, east).unwrap();
        }
        // A repeat or an older number is ignored.
        hubs.intent(1, 2, east).unwrap();
        for (tick, x) in [(1, 720), (2, 1440), (3, 2160), (4, 2160)] {
            let state = &tick_hidden(&mut hubs, &[], NOBODY)[&1];
            assert_eq!((state.x - a.x, state.ack), (x, tick.min(3)), "tick {tick}");
        }
        // A clump past the backlog keeps the newest moves.
        for seq in 10..10 + MAX_MOVES as u32 + 3 {
            hubs.intent(1, seq, east).unwrap();
        }
        assert_eq!(tick(&mut hubs, &[])[&1].ack, 13);
    }

    #[test]
    fn intents_are_checked_and_only_for_hub_players() {
        let mut hubs = Hubs::default();
        let bad = Intent {
            movement: Axes {
                x: AXIS_LIMIT + 1,
                z: 0,
            },
            facing: 0,
        };
        assert_eq!(hubs.intent(1, 1, Intent::default()), Err("not_in_hub"));
        join(&mut hubs, 1);
        assert_eq!(hubs.intent(1, 1, bad), Err("bad_request"));
        assert_eq!(hubs.intent(1, 2, Intent::default()), Ok(()));
    }

    #[test]
    fn state_messages_are_compact_json() {
        let state = State {
            tick: 3,
            ack: 2,
            x: -1,
            z: 2,
            players: vec![(4, 5, -6, 7)],
            enter: vec![],
            exit: vec![],
        };
        assert_eq!(
            serde_json::to_string(&state).unwrap(),
            r#"{"tick":3,"ack":2,"x":-1,"z":2,"players":[[4,5,-6,7]]}"#
        );
    }
}
