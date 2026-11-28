//! One match's authoritative state machine, free of sockets and clocks.
//!
//! The match task feeds it attachments, inputs and ticks, and delivers the
//! [`Out`] messages it returns. Seats are fixed by join tickets; a
//! connection only ever fills its own seat's input slot.
//!
//! Lifecycle: `Waiting` until every seat has connected once (so nobody acts
//! before the others arrive), `Running` while the rules advance, `Finished`
//! for a short linger after the round result, then `Ended`. A waiting match
//! that never fills, or a running match that every player has left for the
//! grace period, also ends.

use std::collections::VecDeque;
use std::sync::Arc;

use arena_protocol::{
    EncodedSnapshotBody, ErrorCode, FighterScore, FighterState, Handle, InputFrame, MAX_FIGHTERS,
    PROTOCOL_VERSION, RULES_VERSION, ServerMessage, Snapshot, Tags, TickEvent, Welcome, WireAction,
    WireAppearance, WireEvent, WireResult,
};
use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::{ActorInput, Arena, ArenaEvent, MAX_LAG_COMP_TICKS};

use crate::metrics::Metrics;

pub type ConnId = u64;

/// Inputs that may stay waiting for a whole second before the excess is
/// folded together. A clump (network jitter, a tab back from the
/// background) above it is simply consumed one per tick, exactly as the
/// client predicted; only a client that keeps sending faster than the
/// server ticks is folded. Paced clients (`Pacer`) never are.
pub const MAX_BACKLOG: usize = 4;
/// Ticks a queue must stay above [`MAX_BACKLOG`] before folding: one second.
pub const FOLD_AFTER: u64 = TICKS_PER_SECOND;
/// Default broadcast delay for spectators: 0 ticks (live/real-time; configurable
/// up to 60 s via `--spectator-delay-seconds` if anti-ghosting delay is desired).
pub const DEFAULT_SPECTATOR_DELAY: u64 = 0;
/// Default cap on concurrent spectators per match.
pub const DEFAULT_MAX_SPECTATORS: usize = 4;

/// Session limits, in ticks where they are durations.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Inputs held per seat between ticks; the oldest is dropped beyond it.
    pub queue: usize,
    /// A match that has not filled every seat by then ends.
    pub start_timeout: u64,
    /// A running match that every player has left this long ends.
    pub grace: u64,
    /// How long a finished match stays open before closing connections.
    pub linger: u64,
    /// Anti-ghosting delay (in ticks) between live play and spectator snapshots.
    pub spectator_delay: u64,
    /// Maximum concurrent watch-only spectator connections per match.
    pub max_spectators: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            // About a second of inputs: room for clumps without dropping.
            queue: 32,
            start_timeout: 60 * TICKS_PER_SECOND,
            grace: 30 * TICKS_PER_SECOND,
            linger: 5 * TICKS_PER_SECOND,
            spectator_delay: DEFAULT_SPECTATOR_DELAY,
            max_spectators: DEFAULT_MAX_SPECTATORS,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
    Send {
        conn: ConnId,
        message: ServerMessage,
    },
    /// Close the connection, first sending `Error(code)` when given.
    Close {
        conn: ConnId,
        error: Option<ErrorCode>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Waiting,
    Running,
    Finished { at: u64 },
    Ended,
}

#[derive(Debug, Default)]
struct Seat {
    conn: Option<ConnId>,
    joined: bool,
    /// Highest `seq` accepted on the current connection.
    last_seq: Option<u32>,
    /// Last `seq` folded into a tick (reported as `Snapshot.ack`).
    ack: u32,
    /// Highest `view_tick` observed for this seat during this match (monotonic
    /// clamp preventing backwards tick-shopping across ticks or reconnects).
    max_view_tick: u64,
    pending: VecDeque<InputFrame>,
    absent_since: Option<u64>,
    /// When the queue last went above `MAX_BACKLOG`, while it stays there.
    over_backlog_since: Option<u64>,
    /// Inputs that arrived since the previous tick, for the jitter metric.
    arrived: u32,
    window: SeatWindow,
    /// Authoritative combat totals accumulated for this seat across the match
    /// (retained across disconnects and reconnects).
    score: FighterScore,
    /// Character look locked on this seat's first join (`None` until the seat joins).
    appearance: Option<Option<WireAppearance>>,
    /// `Name#1234` from the first ticket for this seat that names one.
    handle: Option<Handle>,
    /// Tags from that same ticket.
    tags: Tags,
}

/// One seat's input flow since the last report (development diagnostics).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeatWindow {
    pub ticks: u32,
    pub received: u32,
    pub applied: u32,
    pub folded: u32,
    pub starved: u32,
    pub queue_min: Option<usize>,
    pub queue_max: usize,
}

#[derive(Clone, Copy, Debug)]
struct Spectator {
    conn: ConnId,
    player: u64,
}

#[derive(Clone, Debug)]
struct DelayedSnapshot {
    deliver_at: u64,
    snapshot: Snapshot,
}

pub struct Session {
    arena: Arena,
    seats: Vec<Seat>,
    spectators: Vec<Spectator>,
    /// Public snapshots buffered for anti-ghosting delay, ordered by `deliver_at`.
    spectator_queue: VecDeque<DelayedSnapshot>,
    /// Latest snapshot already eligible for spectators (sent to mid-match joiners
    /// without ever leaking live state).
    last_spectator_snapshot: Option<Snapshot>,
    /// Whether seated player connections have already been closed after `linger`.
    seats_closed: bool,
    phase: Phase,
    /// Session ticks, which keep counting while the rules wait or linger.
    clock: u64,
    map: u64,
    limits: Limits,
    metrics: Arc<Metrics>,
}

impl Session {
    pub fn new(arena: Arena, map: u64, limits: Limits, metrics: Arc<Metrics>) -> Self {
        let seats = (0..arena.fighters().len())
            .map(|_| Seat::default())
            .collect();
        Self {
            arena,
            seats,
            spectators: Vec::new(),
            spectator_queue: VecDeque::new(),
            last_spectator_snapshot: None,
            seats_closed: false,
            phase: Phase::Waiting,
            clock: 0,
            map,
            limits,
            metrics,
        }
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn arena(&self) -> &Arena {
        &self.arena
    }
    /// Authoritative per-seat combat totals accumulated so far, in server actor order.
    pub fn scoreboard(&self) -> Vec<FighterScore> {
        self.seats.iter().map(|seat| seat.score).collect()
    }
    /// Each seat's handle, in actor order (`None` until a ticket names one).
    pub fn handles(&self) -> Vec<Option<Handle>> {
        self.seats.iter().map(|seat| seat.handle).collect()
    }

    /// Each seat's tags, in actor order.
    pub fn tags(&self) -> Vec<Tags> {
        self.seats.iter().map(|seat| seat.tags).collect()
    }

    /// The `Handles` relay, and `Tags` when any seat has one, for `to`.
    fn identity_relay(&self, to: ConnId, out: &mut Vec<Out>) {
        let handles = self.handles();
        if handles.iter().all(Option::is_none) {
            return;
        }
        out.push(Out::Send {
            conn: to,
            message: ServerMessage::Handles(handles),
        });
        let tags = self.tags();
        if tags.iter().any(|tags| *tags != Tags::default()) {
            out.push(Out::Send {
                conn: to,
                message: ServerMessage::Tags(tags),
            });
        }
    }

    /// After `conn` was welcomed to `seat`: locks the seat's handle and tags
    /// from its ticket the first time one names a handle, and returns the
    /// `Handles` (and `Tags`) relay, to everyone when they changed, else to
    /// `conn` alone. Nothing while no seat has a handle.
    pub fn relay_handles(
        &mut self,
        seat: usize,
        conn: ConnId,
        handle: Option<Handle>,
        tags: Tags,
    ) -> Vec<Out> {
        let changed = match self.seats.get_mut(seat) {
            Some(slot) if slot.handle.is_none() && handle.is_some() => {
                slot.handle = handle;
                slot.tags = tags;
                true
            }
            _ => false,
        };
        let targets: Vec<ConnId> = if changed {
            self.connections().collect()
        } else {
            vec![conn]
        };
        let mut out = Vec::new();
        for to in targets {
            self.identity_relay(to, &mut out);
        }
        out
    }

    /// The `Handles` (and `Tags`) relay for a newly attached spectator.
    pub fn handles_for(&self, conn: ConnId) -> Vec<Out> {
        let mut out = Vec::new();
        self.identity_relay(conn, &mut out);
        out
    }

    /// Locked character looks in server actor order (`None` for seats without a look).
    pub fn appearances(&self) -> Vec<Option<WireAppearance>> {
        self.seats
            .iter()
            .map(|seat| seat.appearance.clone().flatten())
            .collect()
    }
    /// Each connected seat's input flow since the previous call, which
    /// starts a new window.
    pub fn take_windows(&mut self) -> Vec<(usize, SeatWindow)> {
        self.seats
            .iter_mut()
            .enumerate()
            .filter(|(_, seat)| seat.conn.is_some())
            .map(|(index, seat)| (index, std::mem::take(&mut seat.window)))
            .collect()
    }
    pub fn is_ended(&self) -> bool {
        self.phase == Phase::Ended
    }
    /// Connections currently attached (seated players followed by spectators).
    pub fn connections(&self) -> impl Iterator<Item = ConnId> + '_ {
        self.seats
            .iter()
            .filter_map(|seat| seat.conn)
            .chain(self.spectators.iter().map(|spec| spec.conn))
    }
    /// Number of currently connected watch-only spectators.
    pub fn spectator_count(&self) -> usize {
        self.spectators.len()
    }

    /// A ticket-verified connection takes `seat`. A newer connection for the
    /// same seat (a reconnect) replaces the older one, which is told why so
    /// it does not reconnect and take the seat back.
    pub fn attach(&mut self, seat: usize, conn: ConnId) -> Vec<Out> {
        self.attach_with_appearance(seat, conn, None)
    }

    /// Takes `seat` and locks `appearance` on the seat's first join. Later
    /// reconnects keep the locked look so a player cannot swap looks mid-match.
    pub fn attach_with_appearance(
        &mut self,
        seat: usize,
        conn: ConnId,
        appearance: Option<WireAppearance>,
    ) -> Vec<Out> {
        if matches!(self.phase, Phase::Finished { .. } | Phase::Ended) {
            return vec![Out::Close {
                conn,
                error: Some(ErrorCode::MatchOver),
            }];
        }
        let team_size = (self.seats.len() / 2) as u8;
        let Some(slot) = self.seats.get_mut(seat) else {
            return vec![Out::Close {
                conn,
                error: Some(ErrorCode::BadTicket),
            }];
        };
        let mut out = Vec::new();
        if let Some(old) = slot.conn.replace(conn)
            && old != conn
        {
            out.push(Out::Close {
                conn: old,
                error: Some(ErrorCode::Replaced),
            });
        }
        let locked_new_look = if slot.appearance.is_none() {
            let has_look = appearance.is_some();
            slot.appearance = Some(appearance);
            has_look
        } else {
            false
        };
        slot.joined = true;
        slot.absent_since = None;
        // A new connection starts its own sequence, and nothing of it has
        // been applied yet: an old connection's acknowledgement would make
        // a predicting client discard inputs the server has not applied.
        slot.last_seq = None;
        slot.ack = 0;
        slot.pending.clear();
        let ack = slot.ack;
        out.push(Out::Send {
            conn,
            message: ServerMessage::Welcome(Welcome {
                protocol: PROTOCOL_VERSION,
                actor: seat as u8,
                team_size,
                tick: self.arena.tick(),
                tick_rate: TICKS_PER_SECOND as u8,
                round_ticks: self.arena.max_round_ticks(),
                rules: RULES_VERSION,
                map: self.map,
                spectator: false,
                hub: false,
            }),
        });
        out.push(Out::Send {
            conn,
            message: ServerMessage::Snapshot(Snapshot::capture(
                &self.arena,
                ack,
                Vec::new(),
                &self.connected(),
            )),
        });
        let looks = self.appearances();
        if looks.iter().any(Option::is_some) {
            if locked_new_look {
                for target_conn in self.connections() {
                    out.push(Out::Send {
                        conn: target_conn,
                        message: ServerMessage::Appearances(looks.clone()),
                    });
                }
            } else {
                out.push(Out::Send {
                    conn,
                    message: ServerMessage::Appearances(looks),
                });
            }
        }
        out
    }

    /// Attaches a ticket-verified watch-only spectator connection. Never
    /// grants a seat, never marks any seat joined, and never sends live
    /// state while an anti-ghosting delay is active.
    pub fn attach_spectator(&mut self, player: u64, conn: ConnId) -> Vec<Out> {
        let finished_has_deliverable = !self.spectator_queue.is_empty()
            || self
                .last_spectator_snapshot
                .as_ref()
                .is_some_and(|snap| snap.result.is_some());
        if self.phase == Phase::Ended
            || (matches!(self.phase, Phase::Finished { .. }) && !finished_has_deliverable)
        {
            return vec![Out::Close {
                conn,
                error: Some(ErrorCode::MatchOver),
            }];
        }
        let mut out = Vec::new();
        if let Some(existing) = self
            .spectators
            .iter_mut()
            .find(|spec| spec.player == player)
        {
            let old = existing.conn;
            existing.conn = conn;
            if old != conn {
                out.push(Out::Close {
                    conn: old,
                    error: Some(ErrorCode::Replaced),
                });
            }
        } else {
            if self.spectators.len() >= self.limits.max_spectators {
                return vec![Out::Close {
                    conn,
                    error: Some(ErrorCode::MatchFull),
                }];
            }
            self.spectators.push(Spectator { conn, player });
        }
        let team_size = (self.seats.len() / 2) as u8;
        let welcome_tick = match self.phase {
            Phase::Waiting => 0,
            Phase::Running if self.limits.spectator_delay == 0 => self.arena.tick(),
            Phase::Running | Phase::Finished { .. } => self
                .last_spectator_snapshot
                .as_ref()
                .map_or(0, |snap| snap.tick),
            Phase::Ended => 0,
        };
        out.push(Out::Send {
            conn,
            message: ServerMessage::Welcome(Welcome {
                protocol: PROTOCOL_VERSION,
                actor: 0,
                team_size,
                tick: welcome_tick,
                tick_rate: TICKS_PER_SECOND as u8,
                round_ticks: self.arena.max_round_ticks(),
                rules: RULES_VERSION,
                map: self.map,
                spectator: true,
                hub: false,
            }),
        });
        match self.phase {
            Phase::Waiting => {
                out.push(Out::Send {
                    conn,
                    message: ServerMessage::Snapshot(Snapshot::capture(
                        &self.arena,
                        0,
                        Vec::new(),
                        &self.connected(),
                    )),
                });
            }
            Phase::Running if self.limits.spectator_delay == 0 => {
                out.push(Out::Send {
                    conn,
                    message: ServerMessage::Snapshot(Snapshot::capture(
                        &self.arena,
                        0,
                        Vec::new(),
                        &self.connected(),
                    )),
                });
            }
            Phase::Running | Phase::Finished { .. } => {
                if let Some(delayed) = &self.last_spectator_snapshot {
                    out.push(Out::Send {
                        conn,
                        message: ServerMessage::Snapshot(delayed.clone()),
                    });
                }
            }
            Phase::Ended => {}
        }
        let looks = self.appearances();
        if looks.iter().any(Option::is_some) {
            out.push(Out::Send {
                conn,
                message: ServerMessage::Appearances(looks),
            });
        }
        out
    }

    /// Removes a watch-only spectator connection if still attached.
    pub fn detach_spectator(&mut self, conn: ConnId) {
        if let Some(at) = self.spectators.iter().position(|spec| spec.conn == conn) {
            self.spectators.swap_remove(at);
        }
    }

    /// Which seats have a connection attached, in actor order.
    pub fn connected(&self) -> Vec<bool> {
        self.seats.iter().map(|seat| seat.conn.is_some()).collect()
    }

    /// Whether `conn` currently holds `seat`.
    pub fn holds(&self, seat: usize, conn: ConnId) -> bool {
        self.seats
            .get(seat)
            .is_some_and(|slot| slot.conn == Some(conn))
    }

    /// The connection went away. Ignored unless it still holds the seat.
    pub fn detach(&mut self, seat: usize, conn: ConnId) {
        if let Some(slot) = self.seats.get_mut(seat)
            && slot.conn == Some(conn)
        {
            slot.conn = None;
            slot.pending.clear();
            slot.absent_since = Some(self.clock);
        }
    }

    /// Queues an input for the next tick. Inputs from a replaced connection,
    /// and repeated or regressed sequence numbers, are dropped.
    pub fn receive(&mut self, seat: usize, conn: ConnId, frame: InputFrame) {
        let Some(slot) = self.seats.get_mut(seat) else {
            return;
        };
        if slot.conn != Some(conn) {
            return;
        }
        if slot.last_seq.is_some_and(|last| frame.seq <= last) {
            Metrics::add(&self.metrics.inputs_stale, 1);
            return;
        }
        slot.last_seq = Some(frame.seq);
        slot.arrived += 1;
        slot.window.received += 1;
        slot.pending.push_back(frame);
        Metrics::add(&self.metrics.inputs, 1);
        if slot.pending.len() > self.limits.queue {
            slot.pending.pop_front();
            Metrics::add(&self.metrics.inputs_overflow, 1);
        }
    }

    pub fn tick(&mut self) -> Vec<Out> {
        let outcome = self.advance_tick();
        let mut out = Vec::new();
        match outcome {
            TickOutcome::None => {}
            TickOutcome::Snapshots { tick, events } => {
                out.extend(self.snapshots(&events, tick));
            }
            TickOutcome::CloseSeats(closes) => {
                out.extend(closes);
            }
            TickOutcome::Close(closes) => {
                // Flush any delayed spectator snapshot maturing on this exact closing
                // tick before closing connections (covers `linger == 0` with delay > 0).
                self.flush_spectator_queue(&mut out);
                out.extend(closes);
                return out;
            }
        }
        self.flush_spectator_queue(&mut out);
        out
    }

    /// Advances one tick and delivers pre-encoded snapshot frames directly,
    /// encoding the shared fighter state once per tick rather than once per
    /// connected seat or spectator.
    pub(crate) fn tick_encoded(&mut self, mut emit: impl FnMut(EncodedOut)) {
        match self.advance_tick() {
            TickOutcome::None => {}
            TickOutcome::Close(out) => {
                self.flush_spectator_queue_encoded(&mut emit);
                for item in out {
                    if let Out::Close { conn, error } = item {
                        emit(EncodedOut::Close { conn, error });
                    }
                }
                return;
            }
            TickOutcome::CloseSeats(out) => {
                for item in out {
                    if let Out::Close { conn, error } = item {
                        emit(EncodedOut::Close { conn, error });
                    }
                }
            }
            TickOutcome::Snapshots { tick, events } => {
                let any_seat = self.seats.iter().any(|seat| seat.conn.is_some());
                let any_spec = !self.spectators.is_empty();
                if any_seat || any_spec {
                    let hash = self.arena.state_hash();
                    let result = self.arena.result().map(WireResult::from);
                    let mut fighters = [FighterState::default(); MAX_FIGHTERS];
                    let count = self.arena.fighters().len();
                    for (index, fighter) in self.arena.fighters().iter().enumerate() {
                        let connected = self.seats.get(index).is_some_and(|s| s.conn.is_some());
                        fighters[index] = FighterState::new(fighter, connected);
                    }
                    let has_rejections = events
                        .iter()
                        .any(|event| matches!(event, ArenaEvent::Rejected { .. }));
                    let shared_events: Vec<TickEvent> = if has_rejections && !any_spec {
                        Vec::new()
                    } else {
                        events
                            .iter()
                            .filter_map(WireEvent::for_spectator)
                            .map(|event| TickEvent { tick, event })
                            .collect()
                    };
                    let mut scoreboard_buf = [FighterScore::default(); MAX_FIGHTERS];
                    let scoreboard = result.map(|_| {
                        for (dst, seat) in scoreboard_buf[..count].iter_mut().zip(&self.seats) {
                            *dst = seat.score;
                        }
                        &scoreboard_buf[..count]
                    });
                    if any_seat {
                        let body = EncodedSnapshotBody::new(
                            self.arena.tick(),
                            hash,
                            result,
                            &fighters[..count],
                            scoreboard,
                            None,
                            (!has_rejections).then_some(shared_events.as_slice()),
                        );
                        let mut seat_events = Vec::new();
                        for (index, seat) in self.seats.iter().enumerate() {
                            let Some(conn) = seat.conn else {
                                continue;
                            };
                            let queued = u8::try_from(seat.pending.len()).unwrap_or(u8::MAX);
                            if has_rejections {
                                seat_events.clear();
                                seat_events.extend(
                                    events
                                        .iter()
                                        .filter_map(|event| WireEvent::for_recipient(event, index))
                                        .map(|event| TickEvent { tick, event }),
                                );
                            }
                            let frame = body.encode_for_seat(seat.ack, queued, &seat_events);
                            emit(EncodedOut::Frame { conn, frame });
                        }
                    }
                    if any_spec {
                        let snap = Snapshot {
                            tick: self.arena.tick(),
                            ack: 0,
                            queued: 0,
                            hash,
                            result,
                            fighters: fighters[..count].to_vec(),
                            scoreboard: scoreboard.map(|s| s.to_vec()),
                            roster: None,
                            events: shared_events,
                        };
                        self.enqueue_spectator_snapshot(snap);
                    }
                }
            }
        }
        self.flush_spectator_queue_encoded(&mut emit);
    }

    fn enqueue_spectator_snapshot(&mut self, snapshot: Snapshot) {
        let delay = if snapshot.tick == 0 {
            0
        } else {
            self.limits.spectator_delay
        };
        let deliver_at = self.clock.saturating_add(delay);
        self.spectator_queue.push_back(DelayedSnapshot {
            deliver_at,
            snapshot,
        });
    }

    fn flush_spectator_queue(&mut self, out: &mut Vec<Out>) {
        while self
            .spectator_queue
            .front()
            .is_some_and(|entry| entry.deliver_at <= self.clock)
        {
            let entry = self.spectator_queue.pop_front().expect("checked front");
            if entry.snapshot.tick > 0 || self.phase != Phase::Waiting {
                self.last_spectator_snapshot = Some(entry.snapshot.clone());
            }
            for spec in &self.spectators {
                out.push(Out::Send {
                    conn: spec.conn,
                    message: ServerMessage::Snapshot(entry.snapshot.clone()),
                });
            }
        }
    }

    fn flush_spectator_queue_encoded(&mut self, emit: &mut impl FnMut(EncodedOut)) {
        while self
            .spectator_queue
            .front()
            .is_some_and(|entry| entry.deliver_at <= self.clock)
        {
            let entry = self.spectator_queue.pop_front().expect("checked front");
            if entry.snapshot.tick > 0 || self.phase != Phase::Waiting {
                self.last_spectator_snapshot = Some(entry.snapshot.clone());
            }
            if self.spectators.is_empty() {
                continue;
            }
            let frame = ServerMessage::Snapshot(entry.snapshot).encode();
            let shared: Arc<[u8]> = Arc::from(frame);
            for spec in &self.spectators {
                emit(EncodedOut::SharedFrame {
                    conn: spec.conn,
                    frame: shared.clone(),
                });
            }
        }
    }

    fn advance_tick(&mut self) -> TickOutcome {
        self.clock += 1;
        match self.phase {
            Phase::Waiting => {
                // Inputs before the start never act, but are acknowledged.
                for seat in &mut self.seats {
                    if let Some(last) = seat.pending.drain(..).next_back() {
                        seat.ack = last.seq;
                    }
                }
                if self.seats.iter().all(|seat| seat.joined) {
                    self.phase = Phase::Running;
                } else if self.clock >= self.limits.start_timeout {
                    return TickOutcome::Close(self.end(Some(ErrorCode::MatchOver)));
                }
                TickOutcome::Snapshots {
                    tick: 0,
                    events: Vec::new(),
                }
            }
            Phase::Running => {
                let clock = self.clock;
                let post_tick = self.arena.tick().saturating_add(1);
                let count = self.seats.len();
                let mut inputs = [ActorInput::default(); MAX_FIGHTERS];
                let mut starved = 0u64;
                let mut folded = 0u64;
                let mut arrivals = [0u64; 5];
                let apply_frame = |seat: &mut Seat, slot: &mut ActorInput, frame: InputFrame| {
                    frame.merge_into(slot);
                    seat.ack = frame.seq;
                    let rewind = if frame.view_tick == 0 {
                        0
                    } else {
                        let clamped = frame.view_tick.min(post_tick);
                        let effective = clamped.max(seat.max_view_tick);
                        seat.max_view_tick = effective;
                        post_tick
                            .saturating_sub(effective)
                            .min(u64::from(MAX_LAG_COMP_TICKS)) as u8
                    };
                    match frame.action {
                        WireAction::Cast { .. } => slot.rewind_ticks = rewind,
                        WireAction::CancelCast => slot.rewind_ticks = 0,
                        WireAction::None => {}
                    }
                };
                for (seat, slot) in self.seats.iter_mut().zip(&mut inputs[..count]) {
                    // One input per tick, in order, so a client can
                    // predict its own movement exactly: input `n` is one
                    // tick of rules. Nothing waiting means an idle tick.
                    if let Some(frame) = seat.pending.pop_front() {
                        apply_frame(seat, slot, frame);
                        seat.window.applied += 1;
                    } else if seat.conn.is_some() {
                        starved += 1;
                        seat.window.starved += 1;
                    }
                    // A queue that stays long only adds delay: after a
                    // second above the backlog, fold the excess into this
                    // tick. Shorter clumps drain on their own.
                    if seat.pending.len() > MAX_BACKLOG {
                        let since = *seat.over_backlog_since.get_or_insert(clock);
                        if clock - since >= FOLD_AFTER {
                            while seat.pending.len() > MAX_BACKLOG {
                                let frame = seat.pending.pop_front().expect("over the backlog");
                                apply_frame(seat, slot, frame);
                                folded += 1;
                                seat.window.folded += 1;
                            }
                            seat.over_backlog_since = None;
                        }
                    } else {
                        seat.over_backlog_since = None;
                    }
                    if seat.conn.is_some() {
                        arrivals[(seat.arrived as usize).min(4)] += 1;
                        let window = &mut seat.window;
                        window.ticks += 1;
                        let queued = seat.pending.len();
                        window.queue_min = Some(window.queue_min.map_or(queued, |m| m.min(queued)));
                        window.queue_max = window.queue_max.max(queued);
                    }
                    seat.arrived = 0;
                }
                if starved > 0 {
                    Metrics::add(&self.metrics.inputs_starved, starved);
                }
                if folded > 0 {
                    Metrics::add(&self.metrics.inputs_folded, folded);
                }
                self.metrics.record_arrivals_batch(arrivals);
                let mut pre_health = [0u16; MAX_FIGHTERS];
                for (dst, fighter) in pre_health[..count].iter_mut().zip(self.arena.fighters()) {
                    *dst = fighter.health;
                }
                let tick = self.arena.tick();
                let events = self.arena.advance(&inputs[..count]);
                self.record_tick_scores(&pre_health[..count], &events);
                Metrics::add(&self.metrics.ticks, 1);
                if let Some(result) = self.arena.result() {
                    self.phase = Phase::Finished { at: self.clock };
                    self.metrics.record_result(result);
                } else if self.seats.iter().all(|seat| {
                    seat.conn.is_none()
                        && seat
                            .absent_since
                            .is_some_and(|since| self.clock - since >= self.limits.grace)
                }) {
                    return TickOutcome::Close(self.end(None));
                }
                TickOutcome::Snapshots { tick, events }
            }
            Phase::Finished { at } => {
                for seat in &mut self.seats {
                    seat.pending.clear();
                }
                let elapsed = self.clock.saturating_sub(at);
                let spec_done_at = self
                    .limits
                    .spectator_delay
                    .saturating_add(self.limits.linger);
                if self.spectators.is_empty() && elapsed >= self.limits.linger {
                    return TickOutcome::Close(self.end(Some(ErrorCode::MatchOver)));
                }
                let due_now = self
                    .spectator_queue
                    .front()
                    .is_some_and(|entry| entry.deliver_at <= self.clock);
                if elapsed >= spec_done_at && !due_now {
                    return TickOutcome::Close(self.end(Some(ErrorCode::MatchOver)));
                }
                if !self.seats_closed && elapsed >= self.limits.linger {
                    self.seats_closed = true;
                    let closes: Vec<Out> = self
                        .seats
                        .iter_mut()
                        .filter_map(|seat| seat.conn.take())
                        .map(|conn| Out::Close {
                            conn,
                            error: Some(ErrorCode::MatchOver),
                        })
                        .collect();
                    if !closes.is_empty() {
                        return TickOutcome::CloseSeats(closes);
                    }
                }
                TickOutcome::None
            }
            Phase::Ended => TickOutcome::None,
        }
    }

    /// The server is stopping: tell every connection and end.
    pub fn shut_down(&mut self) -> Vec<Out> {
        self.end(Some(ErrorCode::ServerShutdown))
    }

    fn record_tick_scores(&mut self, pre_health: &[u16], events: &[ArenaEvent]) {
        let count = self.seats.len();
        let mut remaining = [0u16; MAX_FIGHTERS];
        remaining[..count].copy_from_slice(pre_health);
        let mut kill_credited = [false; MAX_FIGHTERS];
        for event in events {
            match *event {
                ArenaEvent::Hit {
                    actor,
                    target,
                    damage,
                } if actor < count && target < count => {
                    let amount = u32::from(damage);
                    self.seats[actor].score.damage_done =
                        self.seats[actor].score.damage_done.saturating_add(amount);
                    self.seats[target].score.damage_taken =
                        self.seats[target].score.damage_taken.saturating_add(amount);
                    if damage > 0
                        && pre_health[target] > 0
                        && self.arena.fighters()[target].health == 0
                        && !kill_credited[target]
                    {
                        remaining[target] = remaining[target].saturating_sub(damage);
                        if remaining[target] == 0 {
                            kill_credited[target] = true;
                            self.seats[actor].score.killing_blows =
                                self.seats[actor].score.killing_blows.saturating_add(1);
                        }
                    }
                }
                ArenaEvent::Healed {
                    actor,
                    target,
                    amount,
                } if actor < count && target < count => {
                    let value = u32::from(amount);
                    self.seats[actor].score.healing_done =
                        self.seats[actor].score.healing_done.saturating_add(value);
                    self.seats[target].score.healing_received = self.seats[target]
                        .score
                        .healing_received
                        .saturating_add(value);
                }
                _ => {}
            }
        }
    }

    fn end(&mut self, error: Option<ErrorCode>) -> Vec<Out> {
        self.phase = Phase::Ended;
        self.spectator_queue.clear();
        let mut out: Vec<Out> = self
            .seats
            .iter_mut()
            .filter_map(|seat| seat.conn.take())
            .map(|conn| Out::Close { conn, error })
            .collect();
        for spec in self.spectators.drain(..) {
            out.push(Out::Close {
                conn: spec.conn,
                error,
            });
        }
        out
    }

    fn snapshots(&mut self, events: &[ArenaEvent], tick: u64) -> Vec<Out> {
        let any_seat = self.seats.iter().any(|seat| seat.conn.is_some());
        let any_spec = !self.spectators.is_empty();
        if !any_seat && !any_spec {
            return Vec::new();
        }
        let hash = self.arena.state_hash();
        let result = self.arena.result().map(WireResult::from);
        let fighters: Vec<FighterState> = self
            .arena
            .fighters()
            .iter()
            .enumerate()
            .map(|(index, fighter)| {
                FighterState::new(
                    fighter,
                    self.seats.get(index).is_some_and(|s| s.conn.is_some()),
                )
            })
            .collect();
        let has_rejections = events
            .iter()
            .any(|event| matches!(event, ArenaEvent::Rejected { .. }));
        let public_events: Vec<TickEvent> = if has_rejections && !any_spec {
            Vec::new()
        } else {
            events
                .iter()
                .filter_map(WireEvent::for_spectator)
                .map(|event| TickEvent { tick, event })
                .collect()
        };
        let scoreboard = result.map(|_| self.scoreboard());
        let arena_tick = self.arena.tick();
        if any_spec {
            self.enqueue_spectator_snapshot(Snapshot {
                tick: arena_tick,
                ack: 0,
                queued: 0,
                hash,
                result,
                fighters: fighters.clone(),
                scoreboard: scoreboard.clone(),
                roster: None,
                events: public_events.clone(),
            });
        }
        self.seats
            .iter()
            .enumerate()
            .filter_map(|(index, seat)| {
                let conn = seat.conn?;
                let events = if has_rejections {
                    events
                        .iter()
                        .filter_map(|event| WireEvent::for_recipient(event, index))
                        .map(|event| TickEvent { tick, event })
                        .collect()
                } else {
                    public_events.clone()
                };
                Some(Out::Send {
                    conn,
                    message: ServerMessage::Snapshot(Snapshot {
                        tick: arena_tick,
                        ack: seat.ack,
                        queued: u8::try_from(seat.pending.len()).unwrap_or(u8::MAX),
                        hash,
                        result,
                        fighters: fighters.clone(),
                        scoreboard: scoreboard.clone(),
                        roster: None,
                        events,
                    }),
                })
            })
            .collect()
    }
}

pub(crate) enum EncodedOut {
    Frame {
        conn: ConnId,
        frame: Vec<u8>,
    },
    SharedFrame {
        conn: ConnId,
        frame: Arc<[u8]>,
    },
    Close {
        conn: ConnId,
        error: Option<ErrorCode>,
    },
}

enum TickOutcome {
    None,
    Snapshots { tick: u64, events: Vec<ArenaEvent> },
    CloseSeats(Vec<Out>),
    Close(Vec<Out>),
}

#[cfg(test)]
mod tests;
