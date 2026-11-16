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
    ErrorCode, InputFrame, PROTOCOL_VERSION, RULES_VERSION, ServerMessage, Snapshot, TickEvent,
    Welcome, WireEvent,
};
use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::{ActorInput, Arena};

use crate::metrics::Metrics;

pub type ConnId = u64;

/// Inputs left waiting after a tick before the excess is folded together.
/// Clients pace themselves to keep about one waiting (`Pacer`), so only a
/// real burst (a tab returning from the background) reaches this.
pub const MAX_BACKLOG: usize = 4;

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
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            queue: 8,
            start_timeout: 60 * TICKS_PER_SECOND,
            grace: 30 * TICKS_PER_SECOND,
            linger: 5 * TICKS_PER_SECOND,
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
    pending: VecDeque<InputFrame>,
    absent_since: Option<u64>,
}

pub struct Session {
    arena: Arena,
    seats: Vec<Seat>,
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
    pub fn is_ended(&self) -> bool {
        self.phase == Phase::Ended
    }
    /// Connections currently attached, in seat order.
    pub fn connections(&self) -> impl Iterator<Item = ConnId> + '_ {
        self.seats.iter().filter_map(|seat| seat.conn)
    }

    /// A ticket-verified connection takes `seat`. A newer connection for the
    /// same seat (a reconnect) replaces the older one, which is told why so
    /// it does not reconnect and take the seat back.
    pub fn attach(&mut self, seat: usize, conn: ConnId) -> Vec<Out> {
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
                rules: RULES_VERSION,
                map: self.map,
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
        out
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
        slot.pending.push_back(frame);
        Metrics::add(&self.metrics.inputs, 1);
        if slot.pending.len() > self.limits.queue {
            slot.pending.pop_front();
            Metrics::add(&self.metrics.inputs_overflow, 1);
        }
    }

    pub fn tick(&mut self) -> Vec<Out> {
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
                    return self.end(Some(ErrorCode::MatchOver));
                }
                self.snapshots(&[], 0)
            }
            Phase::Running => {
                let metrics = &self.metrics;
                let inputs: Vec<ActorInput> = self
                    .seats
                    .iter_mut()
                    .map(|seat| {
                        // One input per tick, in order, so a client can
                        // predict its own movement exactly: input `n` is one
                        // tick of rules. Nothing waiting means an idle tick.
                        let mut slot = ActorInput::default();
                        if let Some(frame) = seat.pending.pop_front() {
                            frame.merge_into(&mut slot);
                            seat.ack = frame.seq;
                        }
                        // A backlog (a burst after a stall) would add a tick
                        // of delay per queued input; fold the excess into
                        // this tick instead.
                        while seat.pending.len() > MAX_BACKLOG {
                            let frame = seat.pending.pop_front().expect("over the backlog");
                            frame.merge_into(&mut slot);
                            seat.ack = frame.seq;
                            Metrics::add(&metrics.inputs_folded, 1);
                        }
                        slot
                    })
                    .collect();
                let tick = self.arena.tick();
                let events = self.arena.advance(&inputs);
                Metrics::add(&self.metrics.ticks, 1);
                let out = self.snapshots(&events, tick);
                if self.arena.result().is_some() {
                    self.phase = Phase::Finished { at: self.clock };
                } else if self.seats.iter().all(|seat| {
                    seat.conn.is_none()
                        && seat
                            .absent_since
                            .is_some_and(|since| self.clock - since >= self.limits.grace)
                }) {
                    return self.end(None);
                }
                out
            }
            Phase::Finished { at } => {
                for seat in &mut self.seats {
                    seat.pending.clear();
                }
                if self.clock - at >= self.limits.linger {
                    self.end(Some(ErrorCode::MatchOver))
                } else {
                    Vec::new()
                }
            }
            Phase::Ended => Vec::new(),
        }
    }

    /// The server is stopping: tell every connection and end.
    pub fn shut_down(&mut self) -> Vec<Out> {
        self.end(Some(ErrorCode::ServerShutdown))
    }

    fn end(&mut self, error: Option<ErrorCode>) -> Vec<Out> {
        self.phase = Phase::Ended;
        self.seats
            .iter_mut()
            .filter_map(|seat| seat.conn.take())
            .map(|conn| Out::Close { conn, error })
            .collect()
    }

    fn snapshots(&self, events: &[arena_sim::arena::ArenaEvent], tick: u64) -> Vec<Out> {
        let connected = self.connected();
        self.seats
            .iter()
            .enumerate()
            .filter_map(|(index, seat)| {
                let conn = seat.conn?;
                let events = events
                    .iter()
                    .filter_map(|event| WireEvent::for_recipient(event, index))
                    .map(|event| TickEvent { tick, event })
                    .collect();
                let mut snapshot = Snapshot::capture(&self.arena, seat.ack, events, &connected);
                snapshot.queued = u8::try_from(seat.pending.len()).unwrap_or(u8::MAX);
                Some(Out::Send {
                    conn,
                    message: ServerMessage::Snapshot(snapshot),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
