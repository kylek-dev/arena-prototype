//! Networked play against `arena-server` (ADR 0044, Phase 5).
//!
//! The browser's TypeScript runtime owns the WebSocket and greets the server
//! itself on every new socket, so a background tab (whose engine is paused
//! by the browser) keeps its seat. This module owns the rest of the
//! protocol. Each frame it receives the socket's generation (bumped
//! whenever a new socket opens) and any binary frames, and it returns
//! inputs and pings to send. Time is passed in, so the session is testable
//! without a browser.
//!
//! The rest of the client assumes the local player is actor 0, with
//! friends on even and enemies on odd indices. A session therefore
//! renumbers the server's actors: local 0 is this seat, local `2k` the
//! `k`th friendly seat and local `2k + 1` the `k`th enemy seat. Targets and
//! events are translated at this boundary only.
//!
//! The client never decides outcomes. It renders the authoritative state a
//! few ticks behind the newest snapshot, interpolating between snapshots,
//! and only sends intent. There is no prediction yet (the roadmap asks for
//! measurements first), so the player's own movement shows after a round
//! trip plus the interpolation delay.

use std::collections::VecDeque;

use arena_protocol::{
    ClientMessage, ErrorCode, FighterState, InputFrame, ServerMessage, WireEvent,
};
use arena_sim::arena::{Arena, Combatant, Rejection, RoundResult};
use arena_sim::map::Layout;
use bevy::math::Vec3;

/// Ticks the view trails the newest snapshot: 100 ms at 30 Hz, enough to
/// ride out moderate jitter.
pub const INTERPOLATION_DELAY: f64 = 3.0;
const TICK_RATE: f64 = arena_sim::TICKS_PER_SECOND as f64;
/// Snapshots kept for interpolation (a little over two seconds).
const BUFFER: usize = 72;
/// Beyond this many ticks of error the view clock snaps instead of easing.
const SNAP: f64 = 15.0;

/// What the browser runtime hands over each frame while a match is set up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Feed {
    /// Bumped by the runtime for every newly opened socket; 0 before the
    /// first one opens. A closed socket keeps its generation until its last
    /// frames (often the server's final `Error`) have been read.
    pub generation: u32,
    pub frames: Vec<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Connecting,
    /// Joined; the match waits for every seat.
    Waiting,
    Playing,
    /// The round has a result; the server closes the match shortly.
    Finished,
    /// The server refused or ended the connection with this error.
    Closed(ErrorCode),
}

/// Something to show when the view reaches its tick, in local indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    Hit {
        actor: usize,
        target: usize,
        damage: u16,
    },
    Jumped {
        actor: usize,
    },
    /// The round's result from this player's side: `won` is `None` on a draw.
    Finished {
        won: Option<bool>,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct Received {
    tick: u64,
    /// Local order.
    fighters: Vec<Combatant>,
    connected: Vec<bool>,
    result: Option<RoundResult>,
}

/// Measured connection quality, for the debug overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    /// Smoothed ping round trip.
    pub rtt_ms: Option<f64>,
    /// Smoothed time from sending an input to a snapshot acknowledging it.
    pub input_ms: Option<f64>,
    /// How far the view trails the newest snapshot.
    pub delay_ms: f64,
    pub snapshots: u64,
    /// Frames on which the view caught up with the newest snapshot and had
    /// to hold still (late or missing snapshots).
    pub underruns: u64,
    /// Snapshots whose state did not hash to the server's value.
    pub hash_mismatches: u64,
}

/// The authoritative state to show this frame, in local indices.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    /// The snapshot at or before the view tick: the rules state the HUD
    /// reads (health, cooldowns, range).
    pub tick: u64,
    pub fighters: Vec<Combatant>,
    pub connected: Vec<bool>,
    pub result: Option<RoundResult>,
    /// World positions in metres, interpolated toward the next snapshot.
    pub positions: Vec<Vec3>,
    /// Headings in radians, interpolated the short way round.
    pub facings: Vec<f32>,
    /// Events whose tick the view has just passed, oldest first.
    pub shown: Vec<Shown>,
}

#[derive(Debug, Default)]
pub struct Session {
    greeted: Option<u32>,
    pub phase: Phase,
    seat: Option<usize>,
    /// `order[local] = server actor`.
    order: Vec<usize>,
    map: Option<u64>,
    snapshots: VecDeque<Received>,
    newest_at: f64,
    view_tick: Option<f64>,
    events: VecDeque<(u64, Shown)>,
    seq: u32,
    unacked: VecDeque<(u32, f64)>,
    nonce: u32,
    pings: VecDeque<(u32, f64)>,
    last_ping: Option<f64>,
    pub stats: Stats,
}

/// Local order for `seat` among `count` actors: `order[local] = actor`.
pub fn seat_order(seat: usize, count: usize) -> Vec<usize> {
    let team = seat % 2;
    let friends: Vec<usize> = std::iter::once(seat)
        .chain((0..count).filter(|actor| actor % 2 == team && *actor != seat))
        .collect();
    let enemies: Vec<usize> = (0..count).filter(|actor| actor % 2 != team).collect();
    (0..count)
        .map(|local| {
            if local % 2 == 0 {
                friends[local / 2]
            } else {
                enemies[local / 2]
            }
        })
        .collect()
}

/// Presentation yaw (radians, 0 facing −Z) of a rules binary angle.
pub fn yaw(facing: u16) -> f32 {
    f32::from(facing) / 65536.0 * std::f32::consts::TAU
}

impl Session {
    /// Whether the server announced a different map from the one this
    /// build embeds (`None` before the welcome).
    pub fn map_differs(&self) -> Option<bool> {
        self.map
            .map(|digest| digest != fnv1a(arena_map::COLOSSEUM_JSON.as_bytes()))
    }
    /// Whether intent should be sent now.
    pub fn joined(&self) -> bool {
        self.seat.is_some() && matches!(self.phase, Phase::Waiting | Phase::Playing)
    }

    /// Takes the runtime's feed for this frame and returns this player's
    /// rejected intents. `now` is in seconds.
    pub fn receive(&mut self, feed: &Feed, now: f64, layout: &Layout) -> Vec<Rejection> {
        let mut rejections = Vec::new();
        if feed.generation == 0 {
            return rejections;
        }
        if self.greeted != Some(feed.generation) {
            self.greeted = Some(feed.generation);
            // A new connection (already greeted by the runtime): its
            // sequence numbers start again, and the match continues from
            // whatever the server says next.
            self.phase = Phase::Connecting;
            self.seq = 0;
            self.unacked.clear();
            self.pings.clear();
            self.last_ping = None;
        }
        for frame in &feed.frames {
            let Ok(message) = ServerMessage::decode(frame) else {
                self.phase = Phase::Closed(ErrorCode::Malformed);
                continue;
            };
            match message {
                ServerMessage::Welcome(welcome) => {
                    let count = usize::from(welcome.team_size) * 2;
                    let seat = usize::from(welcome.actor);
                    if seat >= count {
                        self.phase = Phase::Closed(ErrorCode::Malformed);
                        continue;
                    }
                    if self.seat != Some(seat) || self.order.len() != count {
                        // A different seat or match: nothing buffered applies.
                        self.snapshots.clear();
                        self.events.clear();
                        self.view_tick = None;
                    }
                    self.seat = Some(seat);
                    self.order = seat_order(seat, count);
                    self.map = Some(welcome.map);
                    self.phase = Phase::Waiting;
                }
                ServerMessage::Snapshot(snapshot) => {
                    if self.seat.is_none() || snapshot.fighters.len() != self.order.len() {
                        continue;
                    }
                    while self
                        .unacked
                        .front()
                        .is_some_and(|(seq, _)| *seq <= snapshot.ack)
                    {
                        let (_, sent) = self.unacked.pop_front().expect("checked front");
                        if snapshot.tick > 0 {
                            smooth(&mut self.stats.input_ms, (now - sent) * 1000.0);
                        }
                    }
                    let authoritative: Vec<Combatant> = snapshot
                        .fighters
                        .iter()
                        .map(FighterState::combatant)
                        .collect();
                    let result = snapshot.result.map(RoundResult::from);
                    if let Some(arena) =
                        Arena::restore(layout.clone(), snapshot.tick, authoritative.clone(), result)
                        && arena.state_hash() != snapshot.hash
                    {
                        self.stats.hash_mismatches += 1;
                    }
                    let fighters = self
                        .order
                        .iter()
                        .map(|&actor| authoritative[actor])
                        .collect();
                    let connected = self
                        .order
                        .iter()
                        .map(|&actor| snapshot.fighters[actor].connected)
                        .collect();
                    let my_team = self.seat.map_or(0, |seat| seat % 2);
                    for event in &snapshot.events {
                        match event.event {
                            WireEvent::Rejected(reason) => rejections.push(reason.into()),
                            other => {
                                if let Some(shown) = self.local_event(other, my_team) {
                                    self.events.push_back((event.tick, shown));
                                    if self.events.len() > 256 {
                                        self.events.pop_front();
                                    }
                                }
                            }
                        }
                    }
                    self.stats.snapshots += 1;
                    self.phase = if result.is_some() {
                        Phase::Finished
                    } else if snapshot.tick > 0 {
                        Phase::Playing
                    } else {
                        Phase::Waiting
                    };
                    // Out-of-order or repeated ticks cannot happen over one
                    // TCP stream, but a reconnect may rewind; keep order.
                    while self
                        .snapshots
                        .back()
                        .is_some_and(|newest| newest.tick >= snapshot.tick)
                    {
                        self.snapshots.pop_back();
                    }
                    self.snapshots.push_back(Received {
                        tick: snapshot.tick,
                        fighters,
                        connected,
                        result,
                    });
                    self.newest_at = now;
                    while self.snapshots.len() > BUFFER {
                        self.snapshots.pop_front();
                    }
                }
                ServerMessage::Pong { nonce } => {
                    if let Some(index) = self.pings.iter().position(|(sent, _)| *sent == nonce) {
                        let (_, at) = self.pings.remove(index).expect("found ping");
                        smooth(&mut self.stats.rtt_ms, (now - at) * 1000.0);
                    }
                }
                ServerMessage::Error(code) => self.phase = Phase::Closed(code),
            }
        }
        rejections
    }

    fn local_event(&self, event: WireEvent, my_team: usize) -> Option<Shown> {
        let local = |actor: u8| self.order.iter().position(|&a| a == usize::from(actor));
        Some(match event {
            WireEvent::Hit {
                actor,
                target,
                damage,
            } => Shown::Hit {
                actor: local(actor)?,
                target: local(target)?,
                damage,
            },
            WireEvent::Jumped { actor } => Shown::Jumped {
                actor: local(actor)?,
            },
            WireEvent::Finished(result) => Shown::Finished {
                won: match RoundResult::from(result) {
                    RoundResult::Won { team } => Some(team == my_team),
                    RoundResult::Draw => None,
                },
            },
            WireEvent::Rejected(_) => return None,
        })
    }

    /// The input frame for this tick, if joined. `attack` is a local index.
    pub fn input(
        &mut self,
        movement: [i16; 2],
        facing: u16,
        jump: bool,
        attack: Option<usize>,
        now: f64,
    ) -> Option<Vec<u8>> {
        if !self.joined() {
            return None;
        }
        self.seq += 1;
        self.unacked.push_back((self.seq, now));
        // A stalled server must not grow this without bound.
        if self.unacked.len() > 256 {
            self.unacked.pop_front();
        }
        let attack = attack
            .and_then(|local| self.order.get(local))
            .and_then(|&actor| u8::try_from(actor).ok());
        Some(
            ClientMessage::Input(InputFrame {
                seq: self.seq,
                movement,
                facing,
                jump,
                attack,
            })
            .encode(),
        )
    }

    /// A ping once a second while joined.
    pub fn ping(&mut self, now: f64) -> Option<Vec<u8>> {
        if !self.joined() || self.last_ping.is_some_and(|last| now - last < 1.0) {
            return None;
        }
        self.last_ping = Some(now);
        self.nonce = self.nonce.wrapping_add(1);
        self.pings.push_back((self.nonce, now));
        if self.pings.len() > 8 {
            self.pings.pop_front();
        }
        Some(ClientMessage::Ping { nonce: self.nonce }.encode())
    }

    /// Advances the view clock by `dt` seconds and returns what to show.
    pub fn view(&mut self, dt: f64, now: f64) -> Option<View> {
        let newest = self.snapshots.back()?;
        let oldest = self.snapshots.front()?.tick as f64;
        let newest_tick = newest.tick as f64;
        // Where the view should be: behind the server's estimated clock.
        let target =
            (newest_tick + (now - self.newest_at) * TICK_RATE - INTERPOLATION_DELAY).max(oldest);
        let mut tick = match self.view_tick {
            Some(tick) if (target - tick).abs() < SNAP => {
                // Run at real time, then ease a tenth of the remaining error
                // per frame so jitter never makes the view jump.
                let advanced = tick + dt * TICK_RATE;
                advanced + (target - advanced) * 0.1
            }
            _ => target,
        };
        if tick > newest_tick {
            self.stats.underruns += 1;
            tick = newest_tick;
        }
        self.view_tick = Some(tick);
        self.stats.delay_ms =
            (newest_tick + (now - self.newest_at) * TICK_RATE - tick) / TICK_RATE * 1000.0;

        let after = self
            .snapshots
            .iter()
            .position(|snapshot| snapshot.tick as f64 > tick);
        let (a, b) = match after {
            Some(0) => (0, 0),
            Some(index) => (index - 1, index),
            None => (self.snapshots.len() - 1, self.snapshots.len() - 1),
        };
        // Older snapshots are no longer needed.
        for _ in 0..a {
            self.snapshots.pop_front();
        }
        let (a, b) = (&self.snapshots[0], &self.snapshots[b - a]);
        let span = (b.tick - a.tick) as f64;
        let fraction = if span > 0.0 {
            ((tick - a.tick as f64) / span).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let positions = a
            .fighters
            .iter()
            .zip(&b.fighters)
            .map(|(from, to)| place(from).lerp(place(to), fraction))
            .collect();
        let facings = a
            .fighters
            .iter()
            .zip(&b.fighters)
            .map(|(from, to)| {
                let turn = to.facing.wrapping_sub(from.facing) as i16;
                yaw(from.facing) + f32::from(turn) / 65536.0 * std::f32::consts::TAU * fraction
            })
            .collect();
        // Events show once the view has passed the tick they happened on.
        let mut shown = Vec::new();
        let passed = tick.floor() as u64;
        while self.events.front().is_some_and(|(at, _)| *at < passed) {
            shown.push(self.events.pop_front().expect("checked front").1);
        }
        Some(View {
            tick: a.tick,
            fighters: a.fighters.clone(),
            connected: a.connected.clone(),
            result: a.result,
            positions,
            facings,
            shown,
        })
    }

    /// The runtime's status line, as JSON for `onNetwork`.
    pub fn status_json(&self) -> String {
        let (phase, error) = match self.phase {
            Phase::Connecting => ("connecting", None),
            Phase::Waiting => ("waiting", None),
            Phase::Playing => ("playing", None),
            Phase::Finished => ("finished", None),
            Phase::Closed(code) => ("closed", Some(code)),
        };
        let number = |value: Option<f64>| value.map_or("null".into(), |v| format!("{v:.1}"));
        format!(
            r#"{{"phase":"{phase}","error":{},"seat":{},"mapDiffers":{},"rttMs":{},"inputMs":{},"delayMs":{:.1},"snapshots":{},"underruns":{},"hashMismatches":{}}}"#,
            error.map_or("null".into(), |code| format!("\"{code:?}\"")),
            self.seat.map_or("null".into(), |seat| seat.to_string()),
            self.map_differs().unwrap_or(false),
            number(self.stats.rtt_ms),
            number(self.stats.input_ms),
            self.stats.delay_ms,
            self.stats.snapshots,
            self.stats.underruns,
            self.stats.hash_mismatches,
        )
    }
}

/// 64-bit FNV-1a, as the server digests its map file.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Exponential smoothing: a new sample moves the estimate an eighth of the way.
fn smooth(estimate: &mut Option<f64>, sample: f64) {
    *estimate = Some(estimate.map_or(sample, |value| value + (sample - value) / 8.0));
}

fn place(fighter: &Combatant) -> Vec3 {
    Vec3::new(
        fighter.position.x as f32 / 1000.0,
        fighter.height as f32 / 1000.0,
        fighter.position.z as f32 / 1000.0,
    )
}

#[cfg(test)]
mod tests;
