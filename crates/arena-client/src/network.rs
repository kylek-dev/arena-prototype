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
//! The client never decides outcomes. It renders other fighters' authoritative
//! state a few ticks behind the newest snapshot, interpolating between
//! snapshots, and only sends intent. Its own fighter is predicted
//! (`arena_protocol::prediction`): each input moves it at once, and each
//! snapshot reconciles the guess with the server's state, easing any
//! correction out over a few frames instead of snapping.

use std::collections::VecDeque;
use std::sync::OnceLock;

use arena_protocol::prediction::{InterpDelay, Pacer, Predictor, TickRate};
use arena_protocol::{
    ClientMessage, ErrorCode, FighterScore, FighterState, Handle, InputFrame, ServerMessage, Tags,
    WireAction, WireAppearance, WireEvent,
};
use arena_sim::arena::{
    ActorInput, Arena, Axes, CastAction, Combatant, InterruptReason, MAX_ROUND_TICKS, Rejection,
    RoundResult,
};
use arena_sim::map::Layout;
use arena_sim::spells::SpellId;
use bevy::math::Vec3;

/// Snapshots kept for interpolation (a little over two seconds).
const BUFFER: usize = 72;
/// Beyond this many ticks of error the view clock snaps instead of easing.
const SNAP: f64 = 15.0;
/// Seconds over which a prediction correction fades (time constant).
const CORRECTION_FADE: f64 = 0.08;
/// Corrections larger than this (metres) snap instead of easing.
const CORRECTION_SNAP: f32 = 2.0;
/// Inputs sent in one rendered frame at most. After a stall (a tab back
/// from the background) the fixed clock catches up with a burst; the rest
/// of the burst is neither sent nor predicted, so both sides agree.
const MAX_INPUTS_PER_FRAME: u32 = 2;

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

/// A finished round's result from this player's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Won,
    Lost,
    /// Both teams were eliminated on the same tick.
    Draw,
    /// The round clock expired while both teams still stood.
    TimeLimitDraw,
}

/// Something to show when the view reaches its tick, in local indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    CastStarted {
        actor: usize,
        spell: SpellId,
        target: Option<usize>,
        duration_ticks: u32,
    },
    CastCompleted {
        actor: usize,
        spell: SpellId,
        target: Option<usize>,
    },
    CastInterrupted {
        actor: usize,
        spell: SpellId,
        reason: InterruptReason,
        by: Option<usize>,
    },
    Hit {
        actor: usize,
        target: usize,
        damage: u16,
    },
    Healed {
        actor: usize,
        target: usize,
        amount: u16,
    },
    Jumped {
        actor: usize,
    },
    Finished {
        outcome: Outcome,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct Received {
    tick: u64,
    /// Local order.
    fighters: Vec<Combatant>,
    connected: Vec<bool>,
    result: Option<RoundResult>,
    /// Local order; `Some` only on a finished match's final snapshot.
    scoreboard: Option<Vec<FighterScore>>,
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
    /// Inputs applied to the prediction but not yet acknowledged.
    pub predicted_ticks: usize,
    /// Reconciliations that moved the predicted fighter.
    pub corrections: u64,
    /// Size of the latest correction, in millimetres.
    pub last_correction_mm: Option<u64>,
    /// This player's inputs waiting on the server, averaged.
    pub queued: Option<f64>,
    /// The lowest that queue was over the last second (pacing steers this).
    pub queued_min: Option<u8>,
    /// Catch-up inputs not sent after a stall.
    pub skipped_inputs: u64,
    /// Fixed steps and inputs sent in the last second, on the browser's clock.
    pub steps_per_second: usize,
    pub sent_per_second: usize,
}

/// The locally predicted fighter, for drawing between fixed ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Own {
    /// Predicted world positions before and after the latest input.
    pub previous: Vec3,
    pub current: Vec3,
    /// Remaining visual correction, added to the drawn position; it fades.
    pub offset: Vec3,
}
impl Own {
    /// Where to draw the fighter, `fraction` of the way through a tick.
    pub fn at(&self, fraction: f32) -> Vec3 {
        self.previous.lerp(self.current, fraction) + self.offset
    }
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
    /// Authoritative end-of-match per-seat totals in local actor order.
    pub scoreboard: Option<Vec<FighterScore>>,
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
    spectator: bool,
    /// `order[local] = server actor`.
    order: Vec<usize>,
    map: Option<u64>,
    round_ticks: Option<u64>,
    snapshots: VecDeque<Received>,
    newest_at: f64,
    view_tick: Option<f64>,
    events: VecDeque<(u64, Shown)>,
    seq: u32,
    unacked: VecDeque<(u32, f64)>,
    nonce: u32,
    pings: VecDeque<(u32, f64)>,
    last_ping: Option<f64>,
    predictor: Predictor,
    pacer: Pacer,
    /// The server's tick rate on this client's clock.
    tick_rate: TickRate,
    /// Adaptive remote-fighter interpolation delay (ticks).
    interp_delay: InterpDelay,
    /// Monotonic server tick of remote fighters last rendered in [`Self::view`].
    last_view_tick: u64,
    /// The player's cast buffer in ticks, sent with each cast (ADR 0058).
    cast_buffer: u8,
    /// The frame (its `now`) of the latest input, and inputs sent in it.
    frame_inputs: (f64, u32),
    /// When recent fixed steps ran and inputs were sent (the last second).
    steps: VecDeque<f64>,
    sends: VecDeque<f64>,
    own: Option<Own>,
    /// The newest snapshot's state in server order, to reconcile with.
    newest_state: Option<(u64, Vec<Combatant>, Option<RoundResult>, u32)>,
    /// Whether this seat's heading has been taken from the server yet.
    heading_taken: bool,
    /// Relayed per-seat locked looks in local actor order.
    appearances: Vec<Option<WireAppearance>>,
    appearances_version: u64,
    /// Relayed per-seat `Name#1234` handles in local actor order (empty
    /// until the server sends them).
    handles: Vec<Option<Handle>>,
    /// Relayed per-seat tags (the admin tag) in local actor order.
    tags: Vec<Tags>,
    pub stats: Stats,
}

/// Relayed tags for the page: an array of `{"admin": bool}` per local
/// actor, or `null` when no seat has a tag.
pub fn tags_json(tags: &[Tags]) -> String {
    if tags.is_empty() {
        return "null".to_owned();
    }
    let entries: Vec<serde_json::Value> = tags
        .iter()
        .map(|tags| serde_json::json!({ "admin": tags.admin }))
        .collect();
    serde_json::Value::Array(entries).to_string()
}

/// Relayed handles for the page: an array of `"Name#1234"` or `null` per
/// local actor.
pub fn handles_json(handles: &[Option<Handle>]) -> String {
    let entries: Vec<serde_json::Value> = handles
        .iter()
        .map(|handle| handle.map_or(serde_json::Value::Null, |h| h.to_string().into()))
        .collect();
    serde_json::Value::Array(entries).to_string()
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
        static COLOSSEUM_HASH: OnceLock<u64> = OnceLock::new();
        let expected = *COLOSSEUM_HASH.get_or_init(|| fnv1a(arena_map::COLOSSEUM_JSON.as_bytes()));
        self.map.map(|digest| digest != expected)
    }
    /// Total ticks in the round announced by the server (`MAX_ROUND_TICKS` before welcome).
    pub fn round_ticks(&self) -> u64 {
        self.round_ticks.unwrap_or(MAX_ROUND_TICKS)
    }
    /// Monotonic counter bumped whenever the relayed per-seat appearances change.
    pub fn appearances_version(&self) -> u64 {
        self.appearances_version
    }
    /// Relayed per-seat locked appearances in local actor order.
    #[cfg(test)]
    pub fn appearances(&self) -> &[Option<WireAppearance>] {
        &self.appearances
    }
    /// Relayed per-seat handles in local actor order: the server's, from
    /// signed tickets, never a client's claim.
    pub fn handles(&self) -> &[Option<Handle>] {
        &self.handles
    }
    /// Relayed per-seat tags in local actor order (empty when no seat has
    /// one).
    pub fn tags(&self) -> &[Tags] {
        &self.tags
    }
    /// JSON representation of the relayed local-order appearances for the browser
    /// kit catalog (`null` until received, otherwise an array of `Appearance | null`).
    pub fn appearances_json(&self) -> String {
        if self.appearances.is_empty() {
            return "null".to_owned();
        }
        let entries: Vec<serde_json::Value> = self
            .appearances
            .iter()
            .map(|look| match look {
                None => serde_json::Value::Null,
                Some(look) => {
                    let groups: serde_json::Map<String, serde_json::Value> = look
                        .groups
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                        .collect();
                    let variants: serde_json::Map<String, serde_json::Value> = look
                        .variants
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                        .collect();
                    serde_json::json!({
                        "version": 1,
                        "ancestry": look.ancestry,
                        "body": look.body,
                        "skinTone": look.skin_tone,
                        "hairColor": look.hair_color,
                        "facialHairColor": look.facial_hair_color,
                        "eyes": look.eyes,
                        "build": f64::from(look.build) / 100.0,
                        "groups": groups,
                        "variants": variants,
                    })
                }
            })
            .collect();
        serde_json::to_string(&entries).expect("appearances serialize to json")
    }
    /// Whether intent should be sent now (never true while spectating).
    pub fn joined(&self) -> bool {
        !self.spectator
            && self.seat.is_some()
            && matches!(self.phase, Phase::Waiting | Phase::Playing)
    }
    /// Whether this connection is a watch-only spectator.
    pub fn is_spectator(&self) -> bool {
        self.spectator
    }

    /// Takes the runtime's feed for this frame and returns this player's
    /// rejected intents. `now` is in seconds.
    pub fn receive(&mut self, feed: &Feed, now: f64, layout: &Layout) -> Vec<Rejection> {
        let mut rejections = Vec::new();
        // Running ticks this frame delivered, for the tick-rate estimate.
        let mut frame_ticks = Vec::new();
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
            // Old sequence numbers mean nothing to the new connection.
            self.predictor.reset();
            self.pacer.reset();
            self.tick_rate.reset();
            self.interp_delay.reset();
            self.last_view_tick = 0;
            self.frame_inputs = (f64::NAN, 0);
            self.own = None;
            // The server always sends Appearances (and Handles) right after
            // Welcome when any seat has one.
            self.handles.clear();
            self.tags.clear();
            if !self.appearances.is_empty() {
                self.appearances.clear();
                self.appearances_version += 1;
            }
        }
        for frame in &feed.frames {
            let Ok(message) = ServerMessage::decode(frame) else {
                self.phase = Phase::Closed(ErrorCode::Malformed);
                continue;
            };
            match message {
                ServerMessage::Welcome(welcome) => {
                    let count = usize::from(welcome.team_size) * 2;
                    let seat = welcome.seat();
                    let invalid_seat = if welcome.spectator {
                        welcome.actor != 0
                    } else {
                        seat.is_none_or(|s| s >= count)
                    };
                    if invalid_seat
                        || welcome.round_ticks == 0
                        || welcome.round_ticks > MAX_ROUND_TICKS
                    {
                        self.phase = Phase::Closed(ErrorCode::Malformed);
                        continue;
                    }
                    if self.spectator != welcome.spectator
                        || self.seat != seat
                        || self.order.len() != count
                        || self.snapshots.back().is_some_and(|newest| {
                            newest.tick > welcome.tick || newest.result.is_some()
                        })
                    {
                        // A different role, seat, match, finished round, or tick rewind: nothing buffered applies.
                        self.snapshots.clear();
                        self.events.clear();
                        self.view_tick = None;
                        self.last_view_tick = 0;
                        self.heading_taken = false;
                        self.handles.clear();
                        self.tags.clear();
                        if !self.appearances.is_empty() {
                            self.appearances.clear();
                            self.appearances_version += 1;
                        }
                    }
                    self.spectator = welcome.spectator;
                    self.seat = seat;
                    self.order = match seat {
                        Some(seat) => seat_order(seat, count),
                        None => (0..count).collect(),
                    };
                    self.map = Some(welcome.map);
                    self.round_ticks = Some(welcome.round_ticks);
                    self.predictor.set_round_limit(welcome.round_ticks);
                    self.phase = Phase::Waiting;
                }
                ServerMessage::Snapshot(snapshot) => {
                    if (self.seat.is_none() && !self.spectator)
                        || snapshot.fighters.len() != self.order.len()
                        || snapshot.roster.is_some()
                    {
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
                    self.newest_state =
                        Some((snapshot.tick, authoritative.clone(), result, snapshot.ack));
                    if snapshot.tick > 0 {
                        frame_ticks.push(snapshot.tick);
                        if !self.spectator {
                            self.pacer.observe(snapshot.queued);
                            self.stats.queued = self.pacer.average();
                            self.stats.queued_min = self.pacer.minimum();
                        }
                    }
                    if (2..=arena_protocol::MAX_FIGHTERS).contains(&authoritative.len())
                        && authoritative.len().is_multiple_of(2)
                        && Arena::hash_state(snapshot.tick, &authoritative, result) != snapshot.hash
                    {
                        self.stats.hash_mismatches += 1;
                    }
                    let fighters: Vec<Combatant> = self
                        .order
                        .iter()
                        .map(|&actor| {
                            let mut fighter = authoritative[actor];
                            if let Some(cast) = fighter.active_cast.as_mut() {
                                cast.target = cast.target.and_then(|server_target| {
                                    self.order.iter().position(|&a| a == server_target)
                                });
                            }
                            if let Some(buffered) = fighter.buffered_cast.as_mut() {
                                buffered.target = buffered.target.and_then(|server_target| {
                                    self.order.iter().position(|&a| a == server_target)
                                });
                            }
                            fighter
                        })
                        .collect();
                    let connected = self
                        .order
                        .iter()
                        .map(|&actor| snapshot.fighters[actor].connected)
                        .collect();
                    let scoreboard = snapshot
                        .scoreboard
                        .as_ref()
                        .filter(|scores| scores.len() == self.order.len())
                        .map(|scores| self.order.iter().map(|&actor| scores[actor]).collect());
                    let my_team = self.seat.map_or(0, |seat| seat % 2);
                    // Translate result team parity into local numbering (local 0 is my team).
                    let local_result = result.map(|result| match result {
                        RoundResult::Won { team } => RoundResult::Won {
                            team: usize::from(team != my_team),
                        },
                        other => other,
                    });
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
                    // TCP stream, but a reconnect or new match may rewind; keep order.
                    let rewound = self
                        .snapshots
                        .back()
                        .is_some_and(|newest| newest.tick > snapshot.tick);
                    if rewound || snapshot.tick < self.last_view_tick {
                        self.last_view_tick = 0;
                        self.view_tick = None;
                    }
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
                        result: local_result,
                        scoreboard,
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
                ServerMessage::Appearances(server_looks) => {
                    if (self.seat.is_none() && !self.spectator)
                        || server_looks.len() != self.order.len()
                    {
                        continue;
                    }
                    let local: Vec<Option<WireAppearance>> = self
                        .order
                        .iter()
                        .map(|&actor| server_looks[actor].clone())
                        .collect();
                    if self.appearances != local {
                        self.appearances = local;
                        self.appearances_version += 1;
                    }
                }
                ServerMessage::Handles(server_handles) => {
                    if (self.seat.is_none() && !self.spectator)
                        || server_handles.len() != self.order.len()
                    {
                        continue;
                    }
                    self.handles = self
                        .order
                        .iter()
                        .map(|&actor| server_handles[actor])
                        .collect();
                }
                ServerMessage::Tags(server_tags) => {
                    if (self.seat.is_none() && !self.spectator)
                        || server_tags.len() != self.order.len()
                    {
                        continue;
                    }
                    self.tags = self.order.iter().map(|&actor| server_tags[actor]).collect();
                }
                // Hub worlds (ADR 0057) are not played by this client yet:
                // a hub `Welcome` (team size 0) closes the link above.
                ServerMessage::Member(_) | ServerMessage::MemberLeft { .. } => {}
            }
        }
        self.tick_rate
            .observe_frame(now, frame_ticks.iter().copied());
        self.interp_delay
            .observe_frame(now, frame_ticks, self.tick_rate.rate());
        self.reconcile(layout);
        rejections
    }

    /// Rebuilds the prediction from the newest snapshot. A correction moves
    /// the drawn position by nothing at first: the offset absorbs it and
    /// fades in `view`.
    fn reconcile(&mut self, layout: &Layout) {
        let seat = match (self.phase, self.seat) {
            (Phase::Playing, Some(seat)) => seat,
            (Phase::Waiting, Some(_)) => {
                // Inputs the server discarded while waiting are dropped;
                // ones still in flight may land after the start and act.
                if let Some((_, _, _, ack)) = self.newest_state.take() {
                    self.predictor.acknowledge(ack);
                }
                self.own = None;
                self.stats.predicted_ticks = self.predictor.pending();
                return;
            }
            _ => {
                // A finished round no longer moves: show the server's state.
                self.predictor.reset();
                self.own = None;
                return;
            }
        };
        let Some((tick, fighters, result, ack)) = self.newest_state.take() else {
            return;
        };
        let before = self.predictor.predicted();
        let moved = self
            .predictor
            .reconcile(layout, tick, fighters, result, seat, ack);
        let Some(after) = self.predictor.predicted() else {
            self.own = None;
            return;
        };
        let after = place(&after);
        match (&mut self.own, before) {
            (Some(own), Some(before)) => {
                let shift = after - place(&before);
                own.previous += shift;
                own.current = after;
                own.offset -= shift;
                if own.offset.length() > CORRECTION_SNAP {
                    own.offset = Vec3::ZERO;
                }
            }
            (own, _) => {
                *own = Some(Own {
                    previous: after,
                    current: after,
                    offset: Vec3::ZERO,
                });
            }
        }
        if let Some(moved) = moved.filter(|moved| *moved > 0) {
            self.stats.last_correction_mm = Some(moved);
        }
        self.stats.corrections = self.predictor.corrections();
        self.stats.predicted_ticks = self.predictor.pending();
    }

    /// Server ticks per second on this client's clock: the rate to send
    /// inputs at (before pacing) and to advance the view by.
    pub fn tick_rate(&self) -> f64 {
        self.tick_rate.rate()
    }

    /// Current adaptive remote-fighter interpolation delay, in server ticks.
    #[cfg(test)]
    pub fn interp_delay_ticks(&self) -> f64 {
        self.interp_delay.delay_ticks()
    }

    /// How much to stretch the fixed input interval so this player's queue
    /// on the server stays near one input: above 1 sends slower.
    pub fn pace(&self) -> f64 {
        if self.phase == Phase::Playing {
            self.pacer.factor()
        } else {
            1.0
        }
    }

    /// The predicted fighter, for drawing; `None` unless a round is running.
    pub fn own(&self) -> Option<Own> {
        self.own
    }
    /// This player's heading as the server has it (radians), once per seat:
    /// the first time a snapshot is in after joining a match or taking a
    /// different seat. The heading is local afterwards, for responsive
    /// turning; a reconnect to the same seat keeps it. Joining a new match
    /// thus starts facing out of the starting room, toward the other team.
    pub fn take_initial_heading(&mut self) -> Option<f32> {
        if self.spectator || self.heading_taken {
            return None;
        }
        let own = self.snapshots.back()?.fighters.first()?;
        self.heading_taken = true;
        Some(yaw(own.facing))
    }
    /// The predicted fighter's rules state, for the HUD's own checks.
    pub fn predicted(&self) -> Option<Combatant> {
        self.own.and(self.predictor.predicted())
    }

    fn local_event(&self, event: WireEvent, my_team: usize) -> Option<Shown> {
        let local = |actor: u8| self.order.iter().position(|&a| a == usize::from(actor));
        let opt_local = |actor: Option<u8>| match actor {
            None => Some(None),
            Some(a) => Some(Some(local(a)?)),
        };
        Some(match event {
            WireEvent::CastStarted {
                actor,
                spell,
                target,
                duration_ticks,
            } => Shown::CastStarted {
                actor: local(actor)?,
                spell: spell.into(),
                target: opt_local(target)?,
                duration_ticks,
            },
            WireEvent::CastCompleted {
                actor,
                spell,
                target,
            } => Shown::CastCompleted {
                actor: local(actor)?,
                spell: spell.into(),
                target: opt_local(target)?,
            },
            WireEvent::CastInterrupted {
                actor,
                spell,
                reason,
                by,
            } => Shown::CastInterrupted {
                actor: local(actor)?,
                spell: spell.into(),
                reason: reason.into(),
                by: opt_local(by)?,
            },
            WireEvent::Hit {
                actor,
                target,
                damage,
            } => Shown::Hit {
                actor: local(actor)?,
                target: local(target)?,
                damage,
            },
            WireEvent::Healed {
                actor,
                target,
                amount,
            } => Shown::Healed {
                actor: local(actor)?,
                target: local(target)?,
                amount,
            },
            WireEvent::Jumped { actor } => Shown::Jumped {
                actor: local(actor)?,
            },
            WireEvent::Finished(result) => Shown::Finished {
                outcome: match RoundResult::from(result) {
                    RoundResult::Won { team } if team == my_team => Outcome::Won,
                    RoundResult::Won { .. } => Outcome::Lost,
                    RoundResult::Draw => Outcome::Draw,
                    RoundResult::TimeLimitDraw => Outcome::TimeLimitDraw,
                },
            },
            WireEvent::Rejected(_) => return None,
        })
    }

    /// The input frame for this tick, if joined. `attack` is a local index.
    #[cfg(test)]
    pub fn input(
        &mut self,
        movement: [i16; 2],
        facing: u16,
        jump: bool,
        attack: Option<usize>,
        now: f64,
    ) -> Option<Vec<u8>> {
        self.input_action(
            movement,
            facing,
            jump,
            attack.map(|target| CastAction::Cast {
                spell: SpellId::Pulse,
                target: Some(target),
            }),
            now,
        )
    }

    /// Sets the cast buffer (in ticks) sent with later casts.
    pub fn set_cast_buffer(&mut self, ticks: u8) {
        self.cast_buffer = ticks;
    }

    /// The input frame for this tick with any spell or cast-cancel action.
    /// Target indices inside `action` are local indices (0 = local player).
    pub fn input_action(
        &mut self,
        movement: [i16; 2],
        facing: u16,
        jump: bool,
        action: Option<CastAction>,
        now: f64,
    ) -> Option<Vec<u8>> {
        if !self.joined() {
            return None;
        }
        self.steps.push_back(now);
        while self.steps.front().is_some_and(|at| now - at >= 1.0) {
            self.steps.pop_front();
        }
        self.stats.steps_per_second = self.steps.len();
        if self.frame_inputs.0 == now {
            if self.frame_inputs.1 >= MAX_INPUTS_PER_FRAME {
                self.stats.skipped_inputs += 1;
                return None;
            }
            self.frame_inputs.1 += 1;
        } else {
            self.frame_inputs = (now, 1);
        }
        self.seq += 1;
        self.sends.push_back(now);
        while self.sends.front().is_some_and(|at| now - at >= 1.0) {
            self.sends.pop_front();
        }
        self.stats.sent_per_second = self.sends.len();
        self.unacked.push_back((self.seq, now));
        // A stalled server must not grow this without bound.
        if self.unacked.len() > 256 {
            self.unacked.pop_front();
        }
        {
            // Move at once; the server's snapshots will confirm or correct.
            // Inputs sent while waiting are recorded too: the match may start
            // before they arrive, and then the server applies them.
            let predicted = self.predictor.apply(
                self.seq,
                ActorInput {
                    movement: Some(Axes {
                        x: movement[0],
                        z: movement[1],
                    }),
                    facing: Some(facing),
                    jump,
                    action: None,
                    rewind_ticks: 0,
                    cast_buffer_ticks: 0,
                },
            );
            if let (Some(predicted), Some(own)) = (predicted, self.own.as_mut()) {
                own.previous = own.current;
                own.current = place(&predicted);
            }
            self.stats.predicted_ticks = self.predictor.pending();
        }
        let wire_action = match action {
            None => WireAction::None,
            Some(CastAction::Cancel) => WireAction::CancelCast,
            Some(CastAction::Cast { spell, target }) => match target {
                None => WireAction::Cast {
                    spell: spell.into(),
                    target: None,
                },
                Some(local) => self
                    .order
                    .get(local)
                    .and_then(|&actor| u8::try_from(actor).ok())
                    .map_or(WireAction::None, |server_actor| WireAction::Cast {
                        spell: spell.into(),
                        target: Some(server_actor),
                    }),
            },
        };
        Some(
            ClientMessage::Input(InputFrame {
                seq: self.seq,
                movement,
                facing,
                jump,
                action: wire_action,
                view_tick: self.last_view_tick,
                cast_buffer: if matches!(wire_action, WireAction::Cast { .. }) {
                    self.cast_buffer
                } else {
                    0
                },
            })
            .encode(),
        )
    }

    /// A ping once a second while connected (seated players and spectators alike).
    pub fn ping(&mut self, now: f64) -> Option<Vec<u8>> {
        let active = self.joined()
            || (self.spectator && matches!(self.phase, Phase::Waiting | Phase::Playing));
        if !active || self.last_ping.is_some_and(|last| now - last < 1.0) {
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
        if let Some(own) = self.own.as_mut() {
            own.offset *= (-dt / CORRECTION_FADE).exp() as f32;
        }
        let rate = self.tick_rate.rate();
        let newest = self.snapshots.back()?;
        let newest_u64 = newest.tick;
        let oldest = self.snapshots.front()?.tick as f64;
        let newest_tick = newest_u64 as f64;
        // Where the view should be: behind the server's estimated clock by the
        // adaptive interpolation delay (ADR 0049).
        let delay = self.interp_delay.delay_ticks();
        let target = (newest_tick + (now - self.newest_at) * rate - delay).max(oldest);
        let mut tick = match self.view_tick {
            Some(tick) if (target - tick).abs() < SNAP => {
                // Run at real time, then ease a tenth of the remaining error
                // per frame so jitter never makes the view jump.
                let advanced = tick + dt * rate;
                advanced + (target - advanced) * 0.1
            }
            _ => target,
        };
        if tick > newest_tick {
            self.stats.underruns += 1;
            if self.phase == Phase::Playing && newest_u64 > 0 {
                self.interp_delay.observe_underrun(newest_u64);
            }
            tick = newest_tick;
        }
        self.view_tick = Some(tick);
        self.stats.delay_ms = (newest_tick + (now - self.newest_at) * rate - tick) / rate * 1000.0;

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
        if self.phase == Phase::Playing && a.tick > 0 {
            self.last_view_tick = self.last_view_tick.max(a.tick);
        }
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
            scoreboard: a.scoreboard.clone(),
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
            r#"{{"phase":"{phase}","error":{},"seat":{},"spectator":{},"mapDiffers":{},"rttMs":{},"inputMs":{},"delayMs":{:.1},"snapshots":{},"underruns":{},"hashMismatches":{},"predictedTicks":{},"corrections":{},"lastCorrectionMm":{},"queued":{},"queuedMin":{},"pace":{:.3},"skippedInputs":{},"stepsPerSecond":{},"sentPerSecond":{},"tickRate":{}}}"#,
            error.map_or("null".into(), |code| format!("\"{code:?}\"")),
            self.seat.map_or("null".into(), |seat| seat.to_string()),
            self.spectator,
            self.map_differs().unwrap_or(false),
            number(self.stats.rtt_ms),
            number(self.stats.input_ms),
            self.stats.delay_ms,
            self.stats.snapshots,
            self.stats.underruns,
            self.stats.hash_mismatches,
            self.stats.predicted_ticks,
            self.stats.corrections,
            self.stats
                .last_correction_mm
                .map_or("null".into(), |mm| mm.to_string()),
            number(self.stats.queued),
            self.stats
                .queued_min
                .map_or("null".into(), |min| min.to_string()),
            self.pace(),
            self.stats.skipped_inputs,
            self.stats.steps_per_second,
            self.stats.sent_per_second,
            number(self.tick_rate.measured()),
        )
    }
}

/// 64-bit FNV-1a, as the server digests its map file.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    fnv1a_update(0xcbf2_9ce4_8422_2325, bytes)
}

fn fnv1a_update(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
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
