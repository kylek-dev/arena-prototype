//! Hub worlds on the world server ([ADR 0057](../../../docs/decisions/0057-hub-on-the-world-server.md)):
//! a long-lived `arena_sim::Arena` under the hub rules, which players join
//! and leave with `Role::Hub` tickets. Pure state like [`crate::session`]:
//! no sockets or clocks; the world task feeds it commands and ticks and
//! delivers what it returns.
//!
//! Each connected player gets every tick's snapshot (with the world's
//! roster) and the `Member` relay of who holds each fighter id. Blocks from
//! the front door hide players from each other both ways: each sees the
//! other's slot as vacant, never their member entry, and no event about
//! them, and neither can aim a spell at the other.

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

use arena_protocol::{
    ErrorCode, FighterState, InputFrame, Member, PROTOCOL_VERSION, RULES_VERSION, ServerMessage,
    Snapshot, Tags, TickEvent, Welcome, WireAppearance, WireEvent, WireSlot,
};
use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::{
    ActorInput, Arena, ArenaEvent, CastAction, HUB_PLAYER_CAP, Slot, vacant_fighter,
};

use crate::metrics::Metrics;
use crate::session::ConnId;
use crate::session::{FOLD_AFTER, MAX_BACKLOG, Out, apply_frame};

pub use arena_sim::hub::plaza_digest;

/// Blocked pairs of the door's player ids, smaller first, shared by every
/// world on the server.
pub type BlockSet = Arc<BTreeSet<(u64, u64)>>;

/// The player ids the front door has placed (in any world): once the door
/// has said, nobody else may be in a world. `None` until a first report.
pub type Allowed = Option<Arc<std::collections::HashSet<u64>>>;

/// The canonical form of a block list.
pub fn block_set(pairs: impl IntoIterator<Item = (u64, u64)>) -> BlockSet {
    Arc::new(
        pairs
            .into_iter()
            .filter(|(a, b)| a != b)
            .map(|(a, b)| pair(a, b))
            .collect(),
    )
}

/// World limits, in ticks where they are durations.
#[derive(Clone, Copy, Debug)]
pub struct WorldLimits {
    /// Inputs held per player between ticks; the oldest is dropped beyond it.
    pub queue: usize,
    /// A player whose connection went away keeps their place this long, so
    /// a reconnect with the same ticket finds them where they stood.
    pub grace: u64,
    /// A world nobody has been in for this long ends (it is disposable: the
    /// next ticket for it starts a fresh one).
    pub idle: u64,
}
impl Default for WorldLimits {
    fn default() -> Self {
        Self {
            queue: 32,
            grace: 10 * TICKS_PER_SECOND,
            idle: 60 * TICKS_PER_SECOND,
        }
    }
}

/// What a world tells the front door's instance directory.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorldReport {
    /// The world's instance id (the hub tickets' `match_id`), hex.
    pub instance: String,
    /// The door's player ids of everyone in the world (connected or within
    /// their grace).
    pub players: Vec<u64>,
    pub capacity: usize,
}

struct Player {
    /// The ticket's player id: the door's id for this person.
    id: u64,
    slot: usize,
    member: Member,
    conn: Option<ConnId>,
    absent_since: Option<u64>,
    last_seq: Option<u32>,
    ack: u32,
    max_view_tick: u64,
    pending: VecDeque<InputFrame>,
    over_backlog_since: Option<u64>,
    /// The door no longer stands behind them: they leave at the next tick.
    evicted: bool,
}

pub struct World {
    instance: u64,
    arena: Arena,
    players: Vec<Player>,
    blocks: BlockSet,
    allowed: Allowed,
    /// Events from between ticks (casts ended by a block), sent with the
    /// next tick's.
    pending_events: Vec<ArenaEvent>,
    clock: u64,
    empty_since: Option<u64>,
    ended: bool,
    map: u64,
    limits: WorldLimits,
    metrics: Arc<Metrics>,
}

fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

impl World {
    /// A fresh world, starting with the server's latest block list so a
    /// blocked pair never sees each other even before the next report.
    pub fn new(
        instance: u64,
        blocks: BlockSet,
        allowed: Allowed,
        limits: WorldLimits,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            instance,
            arena: Arena::hub(),
            players: Vec::new(),
            blocks,
            allowed,
            pending_events: Vec::new(),
            clock: 0,
            empty_since: Some(0),
            ended: false,
            map: plaza_digest(),
            limits,
            metrics,
        }
    }

    pub fn arena(&self) -> &Arena {
        &self.arena
    }

    pub fn is_ended(&self) -> bool {
        self.ended
    }

    /// Players in the world, connected or within their grace.
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    pub fn report(&self) -> WorldReport {
        WorldReport {
            instance: format!("{:016x}", self.instance),
            players: self.players.iter().map(|player| player.id).collect(),
            capacity: HUB_PLAYER_CAP,
        }
    }

    /// Connections currently attached.
    pub fn connections(&self) -> impl Iterator<Item = ConnId> + '_ {
        self.players.iter().filter_map(|player| player.conn)
    }

    fn blocked(&self, a: u64, b: u64) -> bool {
        a != b && self.blocks.contains(&pair(a, b))
    }

    /// Slots of the players `viewer` must not see.
    fn hidden_from(&self, viewer: u64) -> Vec<usize> {
        self.players
            .iter()
            .filter(|other| self.blocked(viewer, other.id))
            .map(|other| other.slot)
            .collect()
    }

    /// A ticket-verified hub connection for `player` arrives. A player
    /// already in the world (a reconnect, or a second tab) takes their place
    /// back and the older connection is told it was replaced; anyone else
    /// joins in a free slot, or is turned away when the world is full.
    pub fn attach(
        &mut self,
        player: u64,
        conn: ConnId,
        handle: Option<arena_protocol::Handle>,
        tags: Tags,
        appearance: Option<WireAppearance>,
    ) -> Vec<Out> {
        if self.ended {
            return vec![Out::Close {
                conn,
                error: Some(ErrorCode::ServerShutdown),
            }];
        }
        // A ticket the door no longer stands behind (the player left the
        // hub, took a match, or the door forgot them) lets nobody in.
        if self
            .allowed
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(&player))
        {
            return vec![Out::Close {
                conn,
                error: Some(ErrorCode::BadTicket),
            }];
        }
        let mut out = Vec::new();
        let index = match self.players.iter().position(|p| p.id == player) {
            Some(index) => {
                let existing = &mut self.players[index];
                if let Some(old) = existing.conn.replace(conn)
                    && old != conn
                {
                    out.push(Out::Close {
                        conn: old,
                        error: Some(ErrorCode::Replaced),
                    });
                }
                existing.absent_since = None;
                existing.last_seq = None;
                existing.ack = 0;
                existing.pending.clear();
                existing.over_backlog_since = None;
                index
            }
            None => {
                let Some(slot) = self.arena.join() else {
                    return vec![Out::Close {
                        conn,
                        error: Some(ErrorCode::MatchFull),
                    }];
                };
                let member = Member {
                    id: self.arena.slots()[slot].id,
                    handle,
                    tags,
                    appearance,
                };
                self.players.push(Player {
                    id: player,
                    slot,
                    member: member.clone(),
                    conn: Some(conn),
                    absent_since: None,
                    last_seq: None,
                    ack: 0,
                    max_view_tick: 0,
                    pending: VecDeque::new(),
                    over_backlog_since: None,
                    evicted: false,
                });
                self.empty_since = None;
                // Everyone who may see the newcomer hears who they are
                // before the snapshot that first shows them.
                for other in &self.players {
                    if other.id != player
                        && !self.blocked(other.id, player)
                        && let Some(to) = other.conn
                    {
                        out.push(Out::Send {
                            conn: to,
                            message: ServerMessage::Member(Box::new(member.clone())),
                        });
                    }
                }
                self.players.len() - 1
            }
        };
        let slot = self.players[index].slot;
        out.push(Out::Send {
            conn,
            message: ServerMessage::Welcome(Welcome {
                protocol: PROTOCOL_VERSION,
                actor: slot as u8,
                team_size: 0,
                tick: self.arena.tick(),
                tick_rate: TICKS_PER_SECOND as u8,
                round_ticks: self.arena.max_round_ticks(),
                rules: RULES_VERSION,
                map: self.map,
                spectator: false,
                hub: true,
            }),
        });
        for other in &self.players {
            if !self.blocked(player, other.id) {
                out.push(Out::Send {
                    conn,
                    message: ServerMessage::Member(Box::new(other.member.clone())),
                });
            }
        }
        let hidden = self.hidden_from(player);
        if let Some(snapshot) = self.view_for(index, &hidden) {
            out.push(Out::Send {
                conn,
                message: ServerMessage::Snapshot(snapshot),
            });
        }
        out
    }

    /// The connection went away. Ignored unless it is still the player's;
    /// they stay in the world for [`WorldLimits::grace`].
    pub fn detach(&mut self, conn: ConnId) {
        if let Some(player) = self.players.iter_mut().find(|p| p.conn == Some(conn)) {
            player.conn = None;
            player.pending.clear();
            player.absent_since = Some(self.clock);
        }
    }

    /// Queues an input for the next tick. Inputs from a replaced
    /// connection, and repeated or regressed sequence numbers, are dropped.
    pub fn receive(&mut self, conn: ConnId, frame: InputFrame) {
        let Some(player) = self.players.iter_mut().find(|p| p.conn == Some(conn)) else {
            return;
        };
        if player.last_seq.is_some_and(|last| frame.seq <= last) {
            Metrics::add(&self.metrics.inputs_stale, 1);
            return;
        }
        player.last_seq = Some(frame.seq);
        player.pending.push_back(frame);
        Metrics::add(&self.metrics.inputs, 1);
        if player.pending.len() > self.limits.queue {
            player.pending.pop_front();
            Metrics::add(&self.metrics.inputs_overflow, 1);
        }
    }

    /// The door's latest list of placed players: anyone in this world it no
    /// longer lists leaves at the next tick (their connection is closed
    /// with `MatchOver`; others hear `MemberLeft`).
    pub fn set_allowed(&mut self, allowed: Allowed) -> Vec<Out> {
        self.allowed = allowed;
        let mut out = Vec::new();
        let Some(allowed) = self.allowed.clone() else {
            return out;
        };
        for player in &mut self.players {
            if !allowed.contains(&player.id) {
                if let Some(conn) = player.conn.take() {
                    out.push(Out::Close {
                        conn,
                        error: Some(ErrorCode::MatchOver),
                    });
                }
                // Gone at the next tick, as when a grace runs out.
                player.evicted = true;
                player.pending.clear();
            }
        }
        out
    }

    /// The front door's current blocked pairs (see [`block_set`]). Players
    /// who are now blocked from each other stop seeing each other at once
    /// (`MemberLeft`) and anything one had aimed at the other ends; those
    /// unblocked see each other again (`Member`).
    pub fn set_blocks(&mut self, next: BlockSet) -> Vec<Out> {
        let mut out = Vec::new();
        for a in &self.players {
            for b in &self.players {
                let key = pair(a.id, b.id);
                if a.id != b.id && !self.blocks.contains(&key) && next.contains(&key) {
                    self.pending_events
                        .extend(self.arena.drop_aims(a.slot, b.slot));
                }
            }
        }
        for a in &self.players {
            for b in &self.players {
                if a.id == b.id {
                    continue;
                }
                let key = pair(a.id, b.id);
                let (was, now) = (self.blocks.contains(&key), next.contains(&key));
                let Some(to) = a.conn else { continue };
                let message = match (was, now) {
                    (false, true) => ServerMessage::MemberLeft { id: b.member.id },
                    (true, false) => ServerMessage::Member(Box::new(b.member.clone())),
                    _ => continue,
                };
                out.push(Out::Send { conn: to, message });
            }
        }
        self.blocks = next;
        out
    }

    /// The server is stopping: tell every connection and end.
    pub fn shut_down(&mut self) -> Vec<Out> {
        self.ended = true;
        self.players
            .iter_mut()
            .filter_map(|player| player.conn.take())
            .map(|conn| Out::Close {
                conn,
                error: Some(ErrorCode::ServerShutdown),
            })
            .collect()
    }

    /// One tick: players whose grace ran out leave, everyone's next input
    /// applies, the rules advance, and each connected player gets the
    /// snapshot they may see. A world empty for [`WorldLimits::idle`] ends.
    pub fn tick(&mut self) -> Vec<Out> {
        let mut out = Vec::new();
        if self.ended {
            return out;
        }
        self.clock += 1;
        let mut events = std::mem::take(&mut self.pending_events);
        events.extend(self.leave_the_departed(&mut out));
        if self.players.is_empty() {
            let since = *self.empty_since.get_or_insert(self.clock);
            if self.clock - since >= self.limits.idle {
                self.ended = true;
                return out;
            }
        }

        let post_tick = self.arena.tick().saturating_add(1);
        let clock = self.clock;
        let count = self.arena.fighters().len();
        let mut inputs = vec![ActorInput::default(); count];
        let mut folded = 0u64;
        let mut starved = 0u64;
        let mut refused = Vec::new();
        for index in 0..self.players.len() {
            let hidden = self.hidden_from(self.players[index].id);
            let player = &mut self.players[index];
            let slot = &mut inputs[player.slot];
            if let Some(frame) = player.pending.pop_front() {
                apply_frame(
                    &mut player.ack,
                    &mut player.max_view_tick,
                    slot,
                    frame,
                    post_tick,
                );
            } else if player.conn.is_some() {
                starved += 1;
            }
            if player.pending.len() > MAX_BACKLOG {
                let since = *player.over_backlog_since.get_or_insert(clock);
                if clock - since >= FOLD_AFTER {
                    while player.pending.len() > MAX_BACKLOG {
                        let frame = player.pending.pop_front().expect("over the backlog");
                        apply_frame(
                            &mut player.ack,
                            &mut player.max_view_tick,
                            slot,
                            frame,
                            post_tick,
                        );
                        folded += 1;
                    }
                    player.over_backlog_since = None;
                }
            } else {
                player.over_backlog_since = None;
            }
            // Nobody aims at someone they cannot see; they are told what
            // aiming at an empty slot tells them.
            if let Some(CastAction::Cast {
                target: Some(target),
                ..
            }) = slot.action
                && hidden.contains(&target)
            {
                slot.action = None;
                refused.push(player.slot);
            }
        }
        if starved > 0 {
            Metrics::add(&self.metrics.inputs_starved, starved);
        }
        if folded > 0 {
            Metrics::add(&self.metrics.inputs_folded, folded);
        }
        let tick = self.arena.tick();
        events.extend(self.arena.advance(&inputs));
        events.extend(refused.into_iter().map(|actor| ArenaEvent::Rejected {
            actor,
            reason: arena_sim::arena::Rejection::InvalidTarget,
        }));
        Metrics::add(&self.metrics.ticks, 1);

        for index in 0..self.players.len() {
            let player = &self.players[index];
            let Some(conn) = player.conn else { continue };
            let hidden = self.hidden_from(player.id);
            let slot = player.slot;
            let mine: Vec<TickEvent> = events
                .iter()
                .filter(|event| !names_any(event, &hidden))
                .filter_map(|event| WireEvent::for_recipient(event, slot))
                .map(|event| TickEvent { tick, event })
                .collect();
            if let Some(mut snapshot) = self.view_for(index, &hidden) {
                snapshot.events = mine;
                out.push(Out::Send {
                    conn,
                    message: ServerMessage::Snapshot(snapshot),
                });
            }
        }
        out
    }

    /// Players whose grace is over leave: their slots fall vacant, those who
    /// could see them hear `MemberLeft`, and the returned events (casts
    /// aimed at them ending) go out with this tick's.
    fn leave_the_departed(&mut self, out: &mut Vec<Out>) -> Vec<ArenaEvent> {
        let mut events = Vec::new();
        let grace = self.limits.grace;
        let clock = self.clock;
        let mut index = 0;
        while index < self.players.len() {
            let player = &self.players[index];
            let gone = player.evicted
                || player
                    .absent_since
                    .is_some_and(|since| clock - since >= grace);
            if !gone {
                index += 1;
                continue;
            }
            let leaver = self.players.swap_remove(index);
            events.extend(self.arena.leave(leaver.slot).unwrap_or_default());
            for other in &self.players {
                if !self.blocked(other.id, leaver.id)
                    && let Some(to) = other.conn
                {
                    out.push(Out::Send {
                        conn: to,
                        message: ServerMessage::MemberLeft {
                            id: leaver.member.id,
                        },
                    });
                }
            }
        }
        events
    }

    /// What player `index` may see of the world now (no events): their
    /// acknowledgement and queue, and every slot in `hidden` shown vacant,
    /// with aims at it dropped and the hash of what they see.
    fn view_for(&self, index: usize, hidden: &[usize]) -> Option<Snapshot> {
        let player = &self.players[index];
        let mut connected = vec![false; self.arena.fighters().len()];
        for other in &self.players {
            connected[other.slot] = other.conn.is_some();
        }
        let mut snapshot = Snapshot::capture(&self.arena, player.ack, Vec::new(), &connected);
        snapshot.queued = u8::try_from(player.pending.len()).unwrap_or(u8::MAX);
        if hidden.is_empty() {
            return Some(snapshot);
        }
        let roster = snapshot.roster.as_mut()?;
        for &slot in hidden {
            roster.slots[slot] = WireSlot::from(Slot::VACANT);
            snapshot.fighters[slot] = FighterState::new(&vacant_fighter(), false);
        }
        let aims_hidden =
            |target: Option<u8>| target.is_some_and(|t| hidden.contains(&usize::from(t)));
        for fighter in &mut snapshot.fighters {
            if let Some(cast) = fighter.active_cast.as_mut()
                && aims_hidden(cast.target)
            {
                cast.target = None;
            }
            if let Some(held) = fighter.buffered_cast.as_mut()
                && aims_hidden(held.target)
            {
                held.target = None;
            }
        }
        // The hash of the world as this player sees it, so their check of
        // it holds. A view the rules could not restore is never sent.
        snapshot.hash = snapshot.hub_arena()?.state_hash();
        Some(snapshot)
    }
}

/// Whether `event` names any of `slots` (as actor, target or interrupter).
fn names_any(event: &ArenaEvent, slots: &[usize]) -> bool {
    if slots.is_empty() {
        return false;
    }
    let named: [Option<usize>; 3] = match *event {
        ArenaEvent::Jumped { actor }
        | ArenaEvent::Moved { actor }
        | ArenaEvent::Rejected { actor, .. } => [Some(actor), None, None],
        ArenaEvent::CastStarted { actor, target, .. }
        | ArenaEvent::CastCompleted { actor, target, .. } => [Some(actor), target, None],
        ArenaEvent::CastInterrupted { actor, by, .. } => [Some(actor), by, None],
        ArenaEvent::Hit { actor, target, .. } | ArenaEvent::Healed { actor, target, .. } => {
            [Some(actor), Some(target), None]
        }
        ArenaEvent::Finished(_) => [None, None, None],
    };
    named
        .into_iter()
        .flatten()
        .any(|slot| slots.contains(&slot))
}

#[cfg(test)]
mod tests;
